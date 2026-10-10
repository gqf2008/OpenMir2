library Mir2ClientHook;

{ ============================================================================
  Mir2ClientHook —— C2 线：客户端进程内 dump（不动客户端、不动 oracle）

  做法：把本 DLL 注入 MirClinet.exe（32 位），在**主模块的导入表**里就地改写
  wsock32.dll 的 send / recv / connect / closesocket 三项 IAT 槽位，
  指向本 DLL 的 detour；detour 记录原始字节后转调原函数。
  不碰磁盘上的客户端文件（二进制逐字节不变），也不改服务端端口/配置。

  产物与 tools/capture/mir2_proxy.py 同构，便于复用下游（segment_frames /
  GoldenExport 无需改动）：
    <out>/chunks.ndjson      open/data/close 事件（t_wall_ms / t_rel_us / off / n / sha256）
    <out>/conn-NNNN.c2s.bin  客户端→服务端 原始字节（含 `#1…!` 帧）
    <out>/conn-NNNN.s2c.bin  服务端→客户端 原始字节
  输出目录取环境变量 MIR2_HOOK_OUT（由编排脚本在启动客户端前设好，子进程继承）。

  端口 → 跳：7000=login / 7100=sel / 7200=game（等于 GoldenExport 的 hop 口径）。
  ============================================================================ }

{$APPTYPE CONSOLE}

uses
  Windows, SysUtils, Classes, Winsock;

const
  MAX_CONNS = 4096;

type
  TDWordArray = array[0..0] of DWORD;
  PTDWordArray = ^TDWordArray;
  THookSend = function(s: TSocket; buf: PAnsiChar; len: Integer; flags: Integer): Integer; stdcall;
  THookRecv = function(s: TSocket; buf: PAnsiChar; len: Integer; flags: Integer): Integer; stdcall;
  THookConnect = function(s: TSocket; name: PSockAddr; namelen: Integer): Integer; stdcall;
  THookCloseSocket = function(s: TSocket): Integer; stdcall;

  TConnRec = record
    used: Boolean;
    sock: TSocket;
    port: Integer;
    fC2S: Integer;
    fS2C: Integer;
    offC2S: Int64;
    offS2C: Int64;
    nC2S: Int64;
    nS2C: Int64;
  end;

var
  GOutDir: string;
  GList: Integer = 0;            // chunks.ndjson 文件句柄
  GConns: array[0..MAX_CONNS - 1] of TConnRec;
  GConnSeq: Integer = 0;
  GLock: TRTLCriticalSection;
  GOrigSend: THookSend = nil;
  GOrigRecv: THookRecv = nil;
  GOrigConnect: THookConnect = nil;
  GOrigCloseSocket: THookCloseSocket = nil;
  GReady: Boolean = False;

{ ---------------------------------------------------------------- 落盘小工具 }

function NowWallMs: Int64;
var
  ft: TFileTime;
  sys: TSystemTime;
begin
  GetSystemTime(sys);
  SystemTimeToFileTime(sys, ft);
  Result := (Int64(ft.dwHighDateTime) shl 32 or ft.dwLowDateTime) div 10000 - 11644473600000;
end;

procedure WriteAll(h: Integer; const buf; n: Integer);
var
  written, off: Integer;
  p: PAnsiChar;
begin
  if (h = 0) or (n <= 0) then Exit;
  p := @buf;
  off := 0;
  while off < n do
  begin
    written := FileWrite(h, p[off], n - off);
    if written <= 0 then Break;
    Inc(off, written);
  end;
end;

procedure AppendLine(const s: AnsiString);
begin
  if GList = 0 then Exit;
  WriteAll(GList, PAnsiChar(s)^, Length(s));
end;

function Sha256Hex(const buf; n: Integer): string;
begin
  // 不在客户端进程里算 sha256：Windows 单元的 CryptoAPI 符号依赖额外单元，
  // 而完整性 hash 下游（segment_frames.py / verify_golden.py）会从 .bin 重算。
  // 这里留空串，schema 字段保留，便于与代理 dump 同构。
  Result := '';
end;

function HopOfPort(port: Integer): string;
begin
  case port of
    7000: Result := 'login';
    7100: Result := 'sel';
    7200: Result := 'game';
  else
    Result := 'unknown';
  end;
end;

function ConnIndexOf(sock: TSocket; const peerPort: Integer): Integer;
var
  i, freeIdx: Integer;
begin
  // 调用方持锁
  for i := 0 to GConnSeq - 1 do
    if GConns[i].used and (GConns[i].sock = sock) then Exit(i);
  freeIdx := GConnSeq;
  if freeIdx >= MAX_CONNS then Exit(-1);
  Inc(GConnSeq);
  FillChar(GConns[freeIdx], SizeOf(TConnRec), 0);
  GConns[freeIdx].used := True;
  GConns[freeIdx].sock := sock;
  GConns[freeIdx].port := peerPort;
  GConns[freeIdx].fC2S := CreateFile(PChar(Format('%s\conn-%.4d.c2s.bin', [GOutDir, freeIdx + 1])),
    GENERIC_WRITE, FILE_SHARE_READ, nil, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, 0);
  GConns[freeIdx].fS2C := CreateFile(PChar(Format('%s\conn-%.4d.s2c.bin', [GOutDir, freeIdx + 1])),
    GENERIC_WRITE, FILE_SHARE_READ, nil, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, 0);
  AppendLine(Format('{"event":"open","conn":%d,"port":%d,"peer":"127.0.0.1:%d","upstream":"","t_wall_ms":%d,"t_rel_us":0}' + #10,
    [freeIdx + 1, peerPort, peerPort, NowWallMs]));
  Result := freeIdx;
end;

procedure LogData(idx: Integer; const dir: string; buf: Pointer; n: Integer);
var
  h: Integer;
  sha: string;
  rec: ^TConnRec;
  off: Int64;
  hop: string;
begin
  rec := @GConns[idx];
  if n <= 0 then Exit;
  if dir = 'c2s' then
  begin
    h := rec.fC2S; off := rec.offC2S;
  end
  else
  begin
    h := rec.fS2C; off := rec.offS2C;
  end;
  WriteAll(h, buf^, n);
  hop := HopOfPort(rec.port);
  AppendLine(Format('{"event":"data","conn":%d,"port":%d,"hop":"%s","dir":"%s","off":%d,"n":%d,"sha256":"%s","t_wall_ms":%d}' + #10,
    [idx + 1, rec.port, hop, dir, off, n, sha, NowWallMs]));
  if dir = 'c2s' then
  begin
    Inc(rec.offC2S, n); Inc(rec.nC2S, n);
  end
  else
  begin
    Inc(rec.offS2C, n); Inc(rec.nS2C, n);
  end;
end;

function PeerPortOf(sock: TSocket): Integer;
var
  sa: TSockAddr;
  len: Integer;
begin
  Result := 0;
  len := SizeOf(sa);
  FillChar(sa, SizeOf(sa), 0);
  if getpeername(sock, sa, len) = 0 then
    Result := ntohs(PSockAddrIn(@sa)^.sin_port);
end;

{ ------------------------------------------------------------------- detours }

function DetourSend(s: TSocket; buf: PAnsiChar; len: Integer; flags: Integer): Integer; stdcall;
begin
  if GReady and (buf <> nil) and (len > 0) then
  begin
    EnterCriticalSection(GLock);
    try
      LogData(ConnIndexOf(s, PeerPortOf(s)), 'c2s', buf, len);
    finally
      LeaveCriticalSection(GLock);
    end;
  end;
  Result := GOrigSend(s, buf, len, flags);
end;

function DetourRecv(s: TSocket; buf: PAnsiChar; len: Integer; flags: Integer): Integer; stdcall;
var
  n: Integer;
begin
  n := GOrigRecv(s, buf, len, flags);
  if GReady and (n > 0) and (buf <> nil) then
  begin
    EnterCriticalSection(GLock);
    try
      LogData(ConnIndexOf(s, PeerPortOf(s)), 's2c', buf, n);
    finally
      LeaveCriticalSection(GLock);
    end;
  end;
  Result := n;
end;

function DetourConnect(s: TSocket; name: PSockAddr; namelen: Integer): Integer; stdcall;
begin
  Result := GOrigConnect(s, name, namelen);
  if GReady and (Result = 0) and (name <> nil) then
  begin
    EnterCriticalSection(GLock);
    try
      ConnIndexOf(s, ntohs(PSockAddrIn(name)^.sin_port));   // 先记下对端口，便于首个包就归到正确跳
    finally
      LeaveCriticalSection(GLock);
    end;
  end;
end;

function DetourCloseSocket(s: TSocket): Integer; stdcall;
var
  i: Integer;
begin
  if GReady then
  begin
    EnterCriticalSection(GLock);
    try
      for i := 0 to GConnSeq - 1 do
        if GConns[i].used and (GConns[i].sock = s) then
        begin
          AppendLine(Format('{"event":"close","conn":%d,"port":%d,"bytes_c2s":%d,"bytes_s2c":%d,"t_wall_ms":%d}' + #10,
            [i + 1, GConns[i].port, GConns[i].nC2S, GConns[i].nS2C, NowWallMs]));
          if GConns[i].fC2S <> 0 then CloseHandle(GConns[i].fC2S);
          if GConns[i].fS2C <> 0 then CloseHandle(GConns[i].fS2C);
          GConns[i].used := False;
          Break;
        end;
    finally
      LeaveCriticalSection(GLock);
    end;
  end;
  Result := GOrigCloseSocket(s);
end;

{ ------------------------------------------------------------ IAT 就地改写 }

function PatchIat(const modName, dllName, funcName: string; newAddr: Pointer;
  out origAddr: Pointer): Boolean;
// 直接按 PE 结构走（Win32：thunk 数组就是 DWORD 序列）——
// 不用 RTL 的 TImageThunkData（不同 Delphi 版本字段名不一致：Function_ / _Function）。
var
  base: Pointer;
  dos: PImageDosHeader;
  nt: PImageNtHeaders;
  impRva: Cardinal;
  imp: PTDWordArray;
  idx, thunkIdx: Integer;
  dllNamePtr: PAnsiChar;
  firstThunkRva, origThunkRva, nameRva: Cardinal;
  ent, fnRva: Cardinal;
  slot: PPointer;
  oldProtect: Cardinal;
  pname: PAnsiChar;
  found: Boolean;
begin
  Result := False;
  origAddr := nil;
  base := Pointer(GetModuleHandle(PChar(modName)));
  if base = nil then Exit;
  dos := PImageDosHeader(base);
  if dos^.e_magic <> IMAGE_DOS_SIGNATURE then Exit;
  nt := PImageNtHeaders(PAnsiChar(base) + dos^._lfanew);
  if nt^.Signature <> IMAGE_NT_SIGNATURE then Exit;
  impRva := nt^.OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_IMPORT].VirtualAddress;
  if impRva = 0 then Exit;
  imp := PTDWordArray(PAnsiChar(base) + impRva);
  idx := 0;
  while True do
  begin
    // IMAGE_IMPORT_DESCRIPTOR 是 5 个 DWORD：OriginalFirstThunk, TimeDateStamp,
    // ForwarderChain, Name, FirstThunk
    origThunkRva := imp^[idx * 5 + 0];
    nameRva := imp^[idx * 5 + 3];
    firstThunkRva := imp^[idx * 5 + 4];
    if nameRva = 0 then Break;
    dllNamePtr := PAnsiChar(base) + nameRva;
    if SameText(string(AnsiString(dllNamePtr)), dllName) then
    begin
      if origThunkRva = 0 then origThunkRva := firstThunkRva;
      thunkIdx := 0;
      found := False;
      while True do
      begin
        ent := PDWORD(PAnsiChar(base) + origThunkRva + Cardinal(thunkIdx) * 4)^;
        if ent = 0 then Break;
        if (ent and IMAGE_ORDINAL_FLAG32) = 0 then
        begin
          fnRva := ent;
          pname := PAnsiChar(base) + fnRva + 2;          // 跳过 Hint(2B)
          if SameText(string(AnsiString(pname)), funcName) then
          begin
            found := True;
            Break;
          end;
        end;
        Inc(thunkIdx);
      end;
      if found then
      begin
        slot := PPointer(PAnsiChar(base) + firstThunkRva + Cardinal(thunkIdx) * 4);
        if VirtualProtect(slot, SizeOf(Pointer), PAGE_READWRITE, oldProtect) then
        begin
          origAddr := slot^;
          slot^ := newAddr;
          VirtualProtect(slot, SizeOf(Pointer), oldProtect, oldProtect);
          Result := True;
        end;
      end;
      Exit;
    end;
    Inc(idx);
  end;
end;

function GetMainModuleName: string;
var
  buf: array[0..MAX_PATH] of Char;
begin
  GetModuleFileName(0, buf, MAX_PATH);
  Result := ExtractFileName(string(buf));
end;

procedure Install;
var
  orig: Pointer;
  fh: THandle;
  exeName: string;
  marker: string;
  markerText: AnsiString;
begin
  InitializeCriticalSection(GLock);
  // 无条件留一个加载标记（固定路径，不依赖环境变量）：用来分辨
  // "DLL 根本没执行 Install" vs "Install 跑了但环境变量是空的"
  marker := IncludeTrailingPathDelimiter(GetEnvironmentVariable('TEMP')) + 'mir2hook-loaded.txt';
  fh := CreateFile(PChar(marker), GENERIC_WRITE, FILE_SHARE_READ, nil, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, 0);
  if fh <> INVALID_HANDLE_VALUE then
  begin
    markerText := AnsiString('loaded ' + FormatDateTime('hh:nn:ss', Now) +
      ' env=[' + GetEnvironmentVariable('MIR2_HOOK_OUT') + ']' + #10);
    WriteAll(fh, PAnsiChar(markerText)^, Length(markerText));
    CloseHandle(fh);
  end;
  // 输出目录两级来源：环境变量优先，其次 DLL 同目录的 hook.conf。
  // 加 hook.conf 是因为实测"由脚本设 env 再 Start-Process"偶发继承不到（流程里见过 env=[]），
  // 文件不依赖进程环境，稳得多。hook.conf 内容：第一行＝输出目录。
  GOutDir := GetEnvironmentVariable('MIR2_HOOK_OUT');
  if GOutDir = '' then
  begin
    var confPath := IncludeTrailingPathDelimiter(ExtractFilePath(GetModuleName(HInstance))) + 'hook.conf';
    if FileExists(confPath) then
    begin
      var lines := TStringList.Create;
      try
        lines.LoadFromFile(confPath);
        if lines.Count > 0 then GOutDir := Trim(lines[0]);
      finally
        lines.Free;
      end;
    end;
  end;
  if GOutDir = '' then Exit;
  ForceDirectories(GOutDir);
  GList := CreateFile(PChar(GOutDir + '\chunks.ndjson'), GENERIC_WRITE, FILE_SHARE_READ,
    nil, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, 0);
  if GList = Integer(INVALID_HANDLE_VALUE) then GList := 0;

  exeName := GetMainModuleName;
  PatchIat(exeName, 'wsock32.dll', 'send', @DetourSend, orig);
  if orig <> nil then GOrigSend := THookSend(orig);
  PatchIat(exeName, 'wsock32.dll', 'recv', @DetourRecv, orig);
  if orig <> nil then GOrigRecv := THookRecv(orig);
  PatchIat(exeName, 'wsock32.dll', 'connect', @DetourConnect, orig);
  if orig <> nil then GOrigConnect := THookConnect(orig);
  PatchIat(exeName, 'wsock32.dll', 'closesocket', @DetourCloseSocket, orig);
  if orig <> nil then GOrigCloseSocket := THookCloseSocket(orig);

  // 缺一个原函数都不开工（宁可空 dump 也不要半截行为）
  // 注意：过程型变量在 Delphi 里裸写会被当成"调用"，必须用 Assigned
  GReady := Assigned(GOrigSend) and Assigned(GOrigRecv) and
            Assigned(GOrigConnect) and Assigned(GOrigCloseSocket);

  // 就绪标记：编排脚本靠它确认注入成功（不建连、不探测）
  if GReady then
  begin
    fh := CreateFile(PChar(GOutDir + '\hook-ready.flag'), GENERIC_WRITE, FILE_SHARE_READ,
      nil, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, 0);
    if fh <> INVALID_HANDLE_VALUE then CloseHandle(fh);
  end
  else
  begin
    fh := CreateFile(PChar(GOutDir + '\hook-failed.flag'), GENERIC_WRITE, FILE_SHARE_READ,
      nil, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, 0);
    if fh <> INVALID_HANDLE_VALUE then CloseHandle(fh);
  end;
end;

procedure Uninstall;
var
  i: Integer;
begin
  if not GReady then Exit;
  GReady := False;
  EnterCriticalSection(GLock);
  try
    for i := 0 to GConnSeq - 1 do
      if GConns[i].used then
      begin
        AppendLine(Format('{"event":"close","conn":%d,"port":%d,"bytes_c2s":%d,"bytes_s2c":%d,"t_wall_ms":%d}' + #10,
          [i + 1, GConns[i].port, GConns[i].nC2S, GConns[i].nS2C, NowWallMs]));
        if GConns[i].fC2S <> 0 then CloseHandle(GConns[i].fC2S);
        if GConns[i].fS2C <> 0 then CloseHandle(GConns[i].fS2C);
        GConns[i].used := False;
      end;
    if GList <> 0 then CloseHandle(GList);
    GList := 0;
  finally
    LeaveCriticalSection(GLock);
  end;
end;

procedure DllMainProc(reason: Integer); stdcall;
begin
  case reason of
    DLL_PROCESS_ATTACH: Install;
    DLL_PROCESS_DETACH: Uninstall;
  end;
end;

begin
  DLLProc := @DllMainProc;
  DllMainProc(DLL_PROCESS_ATTACH);
end.
