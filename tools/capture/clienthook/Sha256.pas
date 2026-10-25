unit Sha256;

{ ============================================================================
  Sha256 —— 纯 Pascal 的 SHA-256（FIPS 180-4），**不依赖任何外部加密单元**。

  为什么不用 Windows CryptoAPI：Win32 的 Crypt* 符号要额外 uses/链接，
  而本 DLL 是注入进冻结客户端的 hook，依赖越少越好（见 Mir2ClientHook.dpr 头注）。
  本单元只做标准 SHA-256：输入任意字节，输出小写十六进制（64 字符）。

  与 tools/capture/mir2_proxy.py 的 hashlib.sha256(...).hexdigest() 同口径：
  对**同一次 send/recv 的原始 n 字节**取指纹，下游按
  conn-NNNN.c2s-or-s2c.bin 的 [off, off+n) 区间复算即可比对。

  自检：tools/capture/clienthook/selftest_sha256.ps1（已知向量 + 与 .NET 交叉）。
  ============================================================================ }

interface

function Sha256Hex(const Buf; Len: Integer): AnsiString;
function Sha256HexStr(const S: AnsiString): AnsiString;

implementation

{$Q-}{$R-}

type
  TSha256State = record
    H: array[0..7] of Cardinal;
    Block: array[0..63] of Byte;
    BlockLen: Integer;
    TotalBytes: UInt64;
  end;

const
  K: array[0..63] of Cardinal = (
    $428a2f98, $71374491, $b5c0fbcf, $e9b5dba5, $3956c25b, $59f111f1, $923f82a4, $ab1c5ed5,
    $d807aa98, $12835b01, $243185be, $550c7dc3, $72be5d74, $80deb1fe, $9bdc06a7, $c19bf174,
    $e49b69c1, $efbe4786, $0fc19dc6, $240ca1cc, $2de92c6f, $4a7484aa, $5cb0a9dc, $76f988da,
    $983e5152, $a831c66d, $b00327c8, $bf597fc7, $c6e00bf3, $d5a79147, $06ca6351, $14292967,
    $27b70a85, $2e1b2138, $4d2c6dfc, $53380d13, $650a7354, $766a0abb, $81c2c92e, $92722c85,
    $a2bfe8a1, $a81a664b, $c24b8b70, $c76c51a3, $d192e819, $d6990624, $f40e3585, $106aa070,
    $19a4c116, $1e376c08, $2748774c, $34b0bcb5, $391c0cb3, $4ed8aa4a, $5b9cca4f, $682e6ff3,
    $748f82ee, $78a5636f, $84c87814, $8cc70208, $90befffa, $a4506ceb, $bef9a3f7, $c67178f2
  );

function Ror32(x: Cardinal; n: Integer): Cardinal;
begin
  Result := (x shr n) or (x shl (32 - n));
end;

procedure Sha256Init(var st: TSha256State);
begin
  st.H[0] := $6a09e667;
  st.H[1] := $bb67ae85;
  st.H[2] := $3c6ef372;
  st.H[3] := $a54ff53a;
  st.H[4] := $510e527f;
  st.H[5] := $9b05688c;
  st.H[6] := $1f83d9ab;
  st.H[7] := $5be0cd19;
  st.BlockLen := 0;
  st.TotalBytes := 0;
end;

procedure Sha256Transform(var st: TSha256State);
var
  W: array[0..63] of Cardinal;
  a, b, c, d, e, f, g, h: Cardinal;
  t1, t2, s0, s1, ch, maj: Cardinal;
  i: Integer;
begin
  for i := 0 to 15 do
    W[i] := (Cardinal(st.Block[i * 4]) shl 24) or
            (Cardinal(st.Block[i * 4 + 1]) shl 16) or
            (Cardinal(st.Block[i * 4 + 2]) shl 8) or
            Cardinal(st.Block[i * 4 + 3]);
  for i := 16 to 63 do
  begin
    s0 := Ror32(W[i - 15], 7) xor Ror32(W[i - 15], 18) xor (W[i - 15] shr 3);
    s1 := Ror32(W[i - 2], 17) xor Ror32(W[i - 2], 19) xor (W[i - 2] shr 10);
    W[i] := W[i - 16] + s0 + W[i - 7] + s1;
  end;

  a := st.H[0]; b := st.H[1]; c := st.H[2]; d := st.H[3];
  e := st.H[4]; f := st.H[5]; g := st.H[6]; h := st.H[7];

  for i := 0 to 63 do
  begin
    s1 := Ror32(e, 6) xor Ror32(e, 11) xor Ror32(e, 25);
    ch := (e and f) xor ((not e) and g);
    t1 := h + s1 + ch + K[i] + W[i];
    s0 := Ror32(a, 2) xor Ror32(a, 13) xor Ror32(a, 22);
    maj := (a and b) xor (a and c) xor (b and c);
    t2 := s0 + maj;
    h := g; g := f; f := e; e := d + t1;
    d := c; c := b; b := a; a := t1 + t2;
  end;

  st.H[0] := st.H[0] + a; st.H[1] := st.H[1] + b;
  st.H[2] := st.H[2] + c; st.H[3] := st.H[3] + d;
  st.H[4] := st.H[4] + e; st.H[5] := st.H[5] + f;
  st.H[6] := st.H[6] + g; st.H[7] := st.H[7] + h;
end;

procedure Sha256Update(var st: TSha256State; const Buf; Len: Integer);
var
  p: PByte;
  take: Integer;
begin
  if Len <= 0 then Exit;
  p := @Buf;
  Inc(st.TotalBytes, UInt64(Len));
  while Len > 0 do
  begin
    take := 64 - st.BlockLen;
    if take > Len then take := Len;
    Move(p^, st.Block[st.BlockLen], take);
    Inc(st.BlockLen, take);
    Inc(p, take);
    Dec(Len, take);
    if st.BlockLen = 64 then
    begin
      Sha256Transform(st);
      st.BlockLen := 0;
    end;
  end;
end;

function Sha256Digest(var st: TSha256State): AnsiString;
const
  Hex: array[0..15] of AnsiChar = '0123456789abcdef';
var
  bitLen: UInt64;
  pad: array[0..71] of Byte;
  padLen, i, j: Integer;
  w: Cardinal;
begin
  bitLen := st.TotalBytes * 8;

  FillChar(pad, SizeOf(pad), 0);
  pad[0] := $80;
  if st.BlockLen < 56 then
    padLen := 56 - st.BlockLen
  else
    padLen := 120 - st.BlockLen;
  Sha256Update(st, pad, padLen);

  FillChar(pad, SizeOf(pad), 0);
  for i := 0 to 7 do
    pad[i] := Byte(bitLen shr ((7 - i) * 8));
  Sha256Update(st, pad, 8);

  SetLength(Result, 64);
  j := 1;
  for i := 0 to 7 do
  begin
    w := st.H[i];
    Result[j]     := Hex[(w shr 28) and $F];
    Result[j + 1] := Hex[(w shr 24) and $F];
    Result[j + 2] := Hex[(w shr 20) and $F];
    Result[j + 3] := Hex[(w shr 16) and $F];
    Result[j + 4] := Hex[(w shr 12) and $F];
    Result[j + 5] := Hex[(w shr 8) and $F];
    Result[j + 6] := Hex[(w shr 4) and $F];
    Result[j + 7] := Hex[w and $F];
    Inc(j, 8);
  end;
end;

function Sha256Hex(const Buf; Len: Integer): AnsiString;
var
  st: TSha256State;
begin
  Sha256Init(st);
  Sha256Update(st, Buf, Len);
  Result := Sha256Digest(st);
end;

function Sha256HexStr(const S: AnsiString): AnsiString;
var
  dummy: Byte;
begin
  if Length(S) = 0 then
  begin
    dummy := 0;
    Result := Sha256Hex(dummy, 0);
  end
  else
    Result := Sha256Hex(PAnsiChar(S)^, Length(S));
end;

end.
