#!/usr/bin/env python3
"""Regenerate `src/cp936_encode.bin` from the checked-in .NET probe table.

Input (唯一输入，已入库): `tests/data/cp936-encode-bmp.txt`
    Each line is `XXXX<TAB>HEXBYTES` (one non-surrogate BMP code point per line),
    produced by the .NET probe described in
    `mir2-rs/tests/parity/m3/audit-encoding-v2-20261021.md` §1.1:
        Encoding.GetEncoding(936).GetBytes(new[] { (char)cp })

Output: `src/cp936_encode.bin`
    65,536 big-endian u16 values indexed by BMP code point. A value <= 0x00FF is a
    single-byte result; a larger value is a two-byte (lead, trail) result; 0xFFFF is
    the sentinel for "not encodable" (including the 2,048 surrogate code points,
    which this table does not list).

The table content is not modified here: this script is a mechanical, reproducible
conversion only. Run it from anywhere:
    python3 crates/script/tools/gen_cp936.py
Expected sha256(cp936_encode.bin):
    a1ad4d6d44f62cd90cfa445ee759ba05a5226f3187af873704ef0498d16af867
"""

from __future__ import annotations

import hashlib
import pathlib

SENTINEL = 0xFFFF
EXPECTED_CODEPOINTS = 63_488  # 65,536 BMP - 2,048 surrogates
EXPECTED_SHA256 = "a1ad4d6d44f62cd90cfa445ee759ba05a5226f3187af873704ef0498d16af867"


def main() -> int:
    crate = pathlib.Path(__file__).resolve().parents[1]
    src = crate / "tests" / "data" / "cp936-encode-bmp.txt"
    dst = crate / "src" / "cp936_encode.bin"

    table = [SENTINEL] * 0x10000
    seen: set[int] = set()
    for lineno, line in enumerate(src.read_text(encoding="utf-8").splitlines(), 1):
        if not line:
            continue
        key, sep, hex_bytes = line.partition("\t")
        if not sep:
            raise SystemExit(f"{src}:{lineno}: expected KEY<TAB>VALUE")
        cp = int(key, 16)
        if not 0 <= cp <= 0xFFFF:
            raise SystemExit(f"{src}:{lineno}: code point out of BMP: {key}")
        if 0xD800 <= cp <= 0xDFFF:
            raise SystemExit(f"{src}:{lineno}: unexpected surrogate {key}")
        if cp in seen:
            raise SystemExit(f"{src}:{lineno}: duplicate code point {key}")
        raw = bytes.fromhex(hex_bytes)
        if len(raw) == 1:
            table[cp] = raw[0]
        elif len(raw) == 2:
            table[cp] = (raw[0] << 8) | raw[1]
        else:
            raise SystemExit(f"{src}:{lineno}: expected 1 or 2 bytes, got {raw!r}")
        seen.add(cp)

    if len(seen) != EXPECTED_CODEPOINTS:
        raise SystemExit(
            f"{src}: expected {EXPECTED_CODEPOINTS} code points, got {len(seen)}"
        )

    out = bytearray(0x10000 * 2)
    for cp, value in enumerate(table):
        out[2 * cp] = value >> 8
        out[2 * cp + 1] = value & 0xFF
    dst.write_bytes(out)

    digest = hashlib.sha256(out).hexdigest()
    print(f"wrote {dst} ({len(out)} bytes)")
    print(f"sha256 = {digest}")
    if digest != EXPECTED_SHA256:
        raise SystemExit(f"sha256 mismatch: expected {EXPECTED_SHA256}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
