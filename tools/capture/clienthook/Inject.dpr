program Inject;

{ ============================================================================
  Inject —— 把 Mir2ClientHook.dll 注入指定进程（32 位）。

  用法：Inject.exe --pid <PID> --dll <DLL 完整路径>
  做法：OpenProcess → VirtualAllocEx → WriteProcessMemory(路径) →
        CreateRemoteThread(LoadLibraryA)。不修改目标进程的磁盘文件。

  注意：DLL 与目标进程位数必须一致（MirClinet.exe 是 Win32 ⇒ 本工具也要 32 位）。
  ============================================================================ }

{$APPTYPE CONSOLE}

uses
  Windows, SysUtils;

function ArgValue(const name: string; const def: string): string;
var
  i: Integer;
begin
  Result := def;
  for i := 1 to ParamCount - 1 do
    if SameText(ParamStr(i), name) then
    begin
      Result := ParamStr(i + 1);
      Exit;
    end;
end;

function InjectDll(pid: Cardinal; const dllPath: string; out err: string): Boolean;
var
  hProc, hThread: THandle;
  pRemote: Pointer;
  bytes: NativeUInt;
  tid: DWORD;
  hKernel: THandle;
  pLoadLib: Pointer;
  pathBytes: AnsiString;
begin
  Result := False;
  err := '';
  if not FileExists(dllPath) then
  begin
    err := 'DLL 不存在: ' + dllPath;
    Exit;
  end;
  hProc := OpenProcess(PROCESS_CREATE_THREAD or PROCESS_QUERY_INFORMATION or
    PROCESS_VM_OPERATION or PROCESS_VM_WRITE or PROCESS_VM_READ, False, pid);
  if hProc = 0 then
  begin
    err := Format('OpenProcess 失败 (pid=%d, err=%d)', [pid, GetLastError]);
    Exit;
  end;
  try
    pathBytes := AnsiString(dllPath);
    pRemote := VirtualAllocEx(hProc, nil, Length(pathBytes) + 1, MEM_COMMIT or MEM_RESERVE, PAGE_READWRITE);
    if pRemote = nil then
    begin
      err := Format('VirtualAllocEx 失败 (err=%d)', [GetLastError]);
      Exit;
    end;
    if not WriteProcessMemory(hProc, pRemote, PAnsiChar(pathBytes), Length(pathBytes) + 1, bytes) then
    begin
      err := Format('WriteProcessMemory 失败 (err=%d)', [GetLastError]);
      Exit;
    end;
    hKernel := GetModuleHandle('kernel32.dll');
    pLoadLib := GetProcAddress(hKernel, 'LoadLibraryA');
    if pLoadLib = nil then
    begin
      err := 'GetProcAddress(LoadLibraryA) 失败';
      Exit;
    end;
    hThread := CreateRemoteThread(hProc, nil, 0, pLoadLib, pRemote, 0, tid);
    if hThread = 0 then
    begin
      err := Format('CreateRemoteThread 失败 (err=%d)', [GetLastError]);
      Exit;
    end;
    try
      WaitForSingleObject(hThread, 10000);
    finally
      CloseHandle(hThread);
    end;
    Result := True;
  finally
    CloseHandle(hProc);
  end;
end;

var
  pid: Cardinal;
  dll, err: string;
begin
  pid := StrToIntDef(ArgValue('--pid', '0'), 0);
  dll := ArgValue('--dll', '');
  if (pid = 0) or (dll = '') then
  begin
    Writeln('usage: Inject.exe --pid <PID> --dll <path-to-Mir2ClientHook.dll>');
    Halt(2);
  end;
  if not InjectDll(pid, dll, err) then
  begin
    Writeln('INJECT_FAIL: ', err);
    Halt(1);
  end;
  Writeln(Format('INJECT_OK pid=%d dll=%s', [pid, dll]));
end.
