//! EDCode —— Mir2 经典线缆编码（XOR 0xAC + 6-bit 分组，base 0x3C）。
//!
//! 逐字节移植自 `src/OpenMir2/EncryptUtil.cs`（与 Delphi 客户端
//! `Source/Common/EDcode.pas` 的 `EncodeBuf/DeCodeBuf` 算法一致，已对照核实）。
//!
//! 算法形态（三字节 → 四字符，密钥常量 `SEED`/`BASE` 内嵌）：
//! 每 3 个明文字节产出 4 个编码字符；余数 1 字节补 1 字符、余数 2 字节补 1 字符。
//! 编码长度公式：`len + ceil(len/3)`。解码反之，且**输入长度 % 4 == 1 时**
//! 末尾 1 字符被静默丢弃（与 C# `GetDecodeLen` 行为一致，属镜像既有行为而非设计）。
//!
//! 注意：本层只处理字节；文本内容是 GB2312 字节流，编解码前后都不做 UTF-8 转换。

/// C# `EncryptUtil.BySeed` / Delphi `bySeed`。
pub const SEED: u8 = 0xAC;
/// C# `EncryptUtil.ByBase` / Delphi `byBase`。
pub const BASE: u8 = 0x3C;

/// 编码输出长度：`len + ceil(len/3)`（C# 侧缓冲区按 `len * 2` 分配，此为精确值）。
#[must_use]
pub const fn encoded_len(len: usize) -> usize {
    len + len.div_ceil(3)
}

/// 解码输出长度，镜像 C# `GetDecodeLen`：`cycles * 3 + (bytesLeft == 2 ? 1 : bytesLeft == 3 ? 2 : 0)`。
/// `len % 4 == 1` 时尾字节被丢弃（C# 既有行为）。
#[must_use]
pub const fn decoded_len(len: usize) -> usize {
    let cycles = len / 4;
    let bytes_left = len % 4;
    cycles * 3
        + match bytes_left {
            2 => 1,
            3 => 2,
            _ => 0,
        }
}

/// 加密，等价 C# `EncryptUtil.Encode(byte[] srcBuf, int len, Span<byte> dstBuf, int dstOffset)`。
///
/// 把 `src` 编码后写入 `dst[dst_offset..]`，返回写入字节数。
/// `dst` 剩余空间必须 ≥ `encoded_len(src.len())`，否则 panic（调用方保证，同 C# 的调用约定）。
///
/// 关键细节：状态 `no` 的循环是 2→4→6→2（`no % 6 + 2`，**不是** 2→3→4→5→6），
/// 即每第 3 个字节多产出 1 个字符。
pub fn encode_into(src: &[u8], dst: &mut [u8], dst_offset: usize) -> usize {
    let mut no: u8 = 2;
    let mut remainder: u8 = 0;
    let mut dst_pos = dst_offset;
    for &b in src {
        let c = b ^ SEED;
        if no == 6 {
            dst[dst_pos] = (c & 0x3F).wrapping_add(BASE);
            dst_pos += 1;
            remainder |= (c >> 2) & 0x30;
            dst[dst_pos] = remainder.wrapping_add(BASE);
            dst_pos += 1;
            remainder = 0;
        } else {
            let temp = c >> 2;
            dst[dst_pos] = ((temp & 0x3C) | (c & 0x3)).wrapping_add(BASE);
            dst_pos += 1;
            remainder = (remainder << 2) | (temp & 0x3);
        }
        no = no % 6 + 2;
    }
    if no != 2 {
        dst[dst_pos] = remainder.wrapping_add(BASE);
        dst_pos += 1;
    }
    dst_pos - dst_offset
}

/// 加密，返回新分配的 `Vec<u8>`。
#[must_use]
pub fn encode(src: &[u8]) -> Vec<u8> {
    let mut dst = vec![0u8; encoded_len(src.len())];
    let n = encode_into(src, &mut dst, 0);
    debug_assert_eq!(n, dst.len());
    dst
}

/// 解密，等价 C# `EncryptUtil.Decode(byte[] srcBuf, int len, ref int decodeLen)`。
///
/// 输入不是合法编码流时不报错（与 C# 一致：产出垃圾字节）；
/// 调用方靠上层校验（如帧头字段、长度对拍）发现坏数据。
#[must_use]
pub fn decode(src: &[u8]) -> Vec<u8> {
    let len = src.len();
    let cycles = len / 4;
    let bytes_left = len % 4;
    let mut dst = Vec::with_capacity(decoded_len(len));
    for i in 0..cycles {
        let base = i * 4;
        let remainder = src[base + 3].wrapping_sub(BASE);
        let temp = src[base].wrapping_sub(BASE);
        let c = ((temp << 2) & 0xF0) | (remainder & 0x0C) | (temp & 0x3);
        dst.push(c ^ SEED);
        let temp = src[base + 1].wrapping_sub(BASE);
        let c = ((temp << 2) & 0xF0) | ((remainder << 2) & 0x0C) | (temp & 0x3);
        dst.push(c ^ SEED);
        let temp = src[base + 2].wrapping_sub(BASE);
        let c = temp | ((remainder << 2) & 0xC0);
        dst.push(c ^ SEED);
    }
    if bytes_left == 2 {
        let remainder = src[len - 1].wrapping_sub(BASE);
        let temp = src[len - 2].wrapping_sub(BASE);
        let c = ((temp << 2) & 0xF0) | ((remainder << 2) & 0x0C) | (temp & 0x3);
        dst.push(c ^ SEED);
    } else if bytes_left == 3 {
        let remainder = src[len - 1].wrapping_sub(BASE);
        let temp = src[len - 3].wrapping_sub(BASE);
        let c = ((temp << 2) & 0xF0) | (remainder & 0x0C) | (temp & 0x3);
        dst.push(c ^ SEED);
        let temp = src[len - 2].wrapping_sub(BASE);
        let c = ((temp << 2) & 0xF0) | ((remainder << 2) & 0x0C) | (temp & 0x3);
        dst.push(c ^ SEED);
    }
    dst
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 往返一致性（非判据：判据是 C# oracle 向量，见 tests/edcode_vectors.rs）。
    #[test]
    fn roundtrip_all_lengths() {
        for len in 0..=255usize {
            let src: Vec<u8> = (0..len)
                .map(|i| u8::try_from((i * 37 + len) % 256).unwrap())
                .collect();
            let enc = encode(&src);
            assert_eq!(enc.len(), encoded_len(len), "encoded len for {len}");
            let dec = decode(&enc);
            assert_eq!(dec, src, "roundtrip failed for len={len}");
        }
    }

    /// 编码长度公式锚点：12 字节消息头 → 16 字符（4 的倍数，保证头体拼接后解码不错位）。
    #[test]
    fn header_encodes_to_multiple_of_4() {
        assert_eq!(encoded_len(12), 16);
        assert_eq!(16 % 4, 0);
    }

    /// 锚定已知坑：改 1 位种子常量输出必须变（红检① 的静态形态）。
    #[test]
    fn seed_is_load_bearing() {
        let src = b"mir2";
        assert_eq!(encode(src), encode(src)); // 确定性
                                              // 用错误的种子手算首字节：c = b'm' ^ 0xAC = 0xC1；temp=0x30；首字符 = ((0x30&0x3C)|(0xC1&0x3))+0x3C = 0x31+0x3C = 0x6D
        assert_eq!(encode(src)[0], 0x6D);
    }
}
