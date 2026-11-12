//! Bearer token 鉴权：`Authorization: Bearer <token>` 缺失/不符一律 403。
//!
//! 比较是**常量时间**的：长度相等时逐字节异或累积、不按首个不等字节提前返回。
//! 长度不等直接返回 `false`（会泄漏长度差——这是业界通行取舍；token 长度本身不是秘密，
//! 内容才是。文档与测试都以行为用例覆盖：等长但逐字节不同的输入必须不通过）。

use hyper::header::{HeaderMap, AUTHORIZATION};

/// 校验请求头里的 `Authorization: Bearer <token>`。
///
/// * scheme 大小写不敏感（`Bearer`/`bearer` 均可）；
/// * 缺失、非 UTF-8、无空格分隔、非 Bearer scheme、token 不符 ⇒ `false`。
#[must_use]
pub fn authorized(headers: &HeaderMap, token: &str) -> bool {
    let Some(value) = headers.get(AUTHORIZATION) else {
        return false;
    };
    let Ok(value) = value.to_str() else {
        return false;
    };
    let Some((scheme, presented)) = value.split_once(' ') else {
        return false;
    };
    scheme.eq_ignore_ascii_case("bearer") && tokens_equal(presented.trim(), token)
}

/// 常量时间比较：长度相等时逐字节累积异或，**不按首个不等字节提前返回**。
#[must_use]
pub fn tokens_equal(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers_with(value: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(value) = value {
            headers.insert(AUTHORIZATION, value.parse().expect("头值合法"));
        }
        headers
    }

    #[test]
    fn token_comparison_is_exact_and_constant_time_shaped() {
        assert!(tokens_equal("s3cret", "s3cret"));
        assert!(!tokens_equal("s3cret", "s3creT"));
        assert!(!tokens_equal("s3cret", "s3cret "), "长度不同必须不等");
        assert!(!tokens_equal("s3cre", "s3cret"), "长度不同必须不等");
        assert!(!tokens_equal("", "x"));
        assert!(
            tokens_equal("", ""),
            "空 token 的语义由配置层拒绝，比较器不特判"
        );
    }

    #[test]
    fn same_length_byte_differences_are_rejected() {
        let token = "0123456789abcdef";
        for index in 0..token.len() {
            let mut bytes = token.as_bytes().to_vec();
            bytes[index] ^= 0x01;
            let candidate = String::from_utf8(bytes).expect("仍是 UTF-8");
            assert!(
                !tokens_equal(&candidate, token),
                "第 {index} 字节不同必须不通过"
            );
        }
        // 不同位置同时翻转（全等长的最坏形态）也必须不通过。
        let candidate = "b123456789abcde0";
        assert!(!tokens_equal(candidate, token));
    }

    #[test]
    fn authorization_header_is_required() {
        assert!(!authorized(&headers_with(None), "t"));
        assert!(authorized(&headers_with(Some("Bearer t")), "t"));
        assert!(
            authorized(&headers_with(Some("bearer t")), "t"),
            "scheme 大小写不敏感"
        );
        assert!(!authorized(&headers_with(Some("Bearer  tt")), "t"));
        assert!(!authorized(&headers_with(Some("Bearer t")), "other"));
        assert!(!authorized(&headers_with(Some("Basic t")), "t"));
        assert!(!authorized(&headers_with(Some("Bearert")), "t"));
        assert!(!authorized(&headers_with(Some("Bearer")), "t"));
    }
}
