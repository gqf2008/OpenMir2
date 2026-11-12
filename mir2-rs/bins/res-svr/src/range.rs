//! 单段 `Range` 语义（卡 A4）。
//!
//! 支持形态：`bytes=a-b`（闭区间）与 `bytes=a-`（到文件尾）。
//! **忽略**（按 HTTP 语义回 200 全量）：多段 `a-b,c-d`、后缀 `-N`、语法非法、`a > b`、
//! 非 `bytes` 单位 —— 这些交给 [`resolve`] 前已归为 [`RangeRequest::Full`]。
//! 起点越界（`a >= size`）⇒ [`Resolution::Unsatisfiable`]（调用方回 416 + `Content-Range: bytes */size`）。
//! 终点越界按语义饱和到 `size - 1`（`bytes=990-2000` 在 1000 字节文件上 ⇒ `990-999`）。

/// 解析后的请求形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeRequest {
    /// 忽略 Range，回 200 全量。
    Full,
    /// 单段请求；`end=None` 表示开放到文件尾。
    Single {
        /// 起始字节（含）。
        start: u64,
        /// 结束字节（含）；`None` ⇒ 到 EOF。
        end: Option<u64>,
    },
}

/// 相对实际文件大小的裁决。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// 200 全量。
    Full,
    /// 206 单段（闭区间，已饱和）。
    Partial {
        /// 起始字节（含）。
        start: u64,
        /// 结束字节（含）。
        end: u64,
    },
    /// 416：起点不小于文件大小（含空文件上的任何 Range）。
    Unsatisfiable,
}

/// 解析 `Range` 头（`None` 或缺省 ⇒ 全量）。
#[must_use]
pub fn parse(header: Option<&str>) -> RangeRequest {
    let Some(raw) = header else {
        return RangeRequest::Full;
    };
    let raw = raw.trim();
    let Some(prefix) = raw.get(..6) else {
        return RangeRequest::Full;
    };
    if !prefix.eq_ignore_ascii_case("bytes=") {
        return RangeRequest::Full;
    }
    let spec = raw.get(6..).unwrap_or("").trim();
    if spec.contains(',') {
        // 多段 ⇒ 忽略（不实现 multipart/byteranges）。
        return RangeRequest::Full;
    }
    if spec.starts_with('-') {
        // 后缀段 `-N` ⇒ 忽略（回全量，符合「忽略」语义）。
        return RangeRequest::Full;
    }
    let Some((start_raw, end_raw)) = spec.split_once('-') else {
        return RangeRequest::Full;
    };
    let start_raw = start_raw.trim();
    let end_raw = end_raw.trim();
    if start_raw.is_empty() {
        return RangeRequest::Full;
    }
    let Ok(start) = start_raw.parse::<u64>() else {
        return RangeRequest::Full;
    };
    let end = if end_raw.is_empty() {
        None
    } else {
        match end_raw.parse::<u64>() {
            Ok(value) => Some(value),
            Err(_) => return RangeRequest::Full,
        }
    };
    if let Some(end) = end {
        if end < start {
            // `a > b` 的段按 RFC 7233 视为无效 ⇒ 忽略。
            return RangeRequest::Full;
        }
    }
    RangeRequest::Single { start, end }
}

/// 把请求解析结果落到实际文件大小上。
#[must_use]
pub fn resolve(request: RangeRequest, size: u64) -> Resolution {
    match request {
        RangeRequest::Full => Resolution::Full,
        RangeRequest::Single { start, end } => {
            if start >= size {
                return Resolution::Unsatisfiable;
            }
            let last = end.map_or(size - 1, |value| value.min(size - 1));
            if last < start {
                return Resolution::Full;
            }
            Resolution::Partial { start, end: last }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_forms() {
        assert_eq!(parse(None), RangeRequest::Full);
        assert_eq!(parse(Some("")), RangeRequest::Full);
        assert_eq!(
            parse(Some("bytes=0-99")),
            RangeRequest::Single {
                start: 0,
                end: Some(99)
            }
        );
        assert_eq!(
            parse(Some("BYTES = 5-9")),
            RangeRequest::Full,
            "带空格的单位名不是合法 `bytes=` 前缀"
        );
        assert_eq!(
            parse(Some("bytes=5-9")),
            RangeRequest::Single {
                start: 5,
                end: Some(9)
            }
        );
        assert_eq!(
            parse(Some("Bytes=5-")),
            RangeRequest::Single {
                start: 5,
                end: None
            },
            "单位名大小写不敏感"
        );
        assert_eq!(parse(Some("bytes=-10")), RangeRequest::Full, "后缀段忽略");
        assert_eq!(parse(Some("bytes=0-1,5-6")), RangeRequest::Full, "多段忽略");
        assert_eq!(parse(Some("items=0-1")), RangeRequest::Full);
        assert_eq!(parse(Some("bytes=abc")), RangeRequest::Full);
        assert_eq!(parse(Some("bytes=9-5")), RangeRequest::Full, "a > b 忽略");
        assert_eq!(
            parse(Some("bytes=0-")),
            RangeRequest::Single {
                start: 0,
                end: None
            }
        );
    }

    #[test]
    fn resolve_saturates_and_rejects_out_of_bounds() {
        let size = 1000;
        assert_eq!(
            resolve(
                RangeRequest::Single {
                    start: 0,
                    end: Some(99)
                },
                size
            ),
            Resolution::Partial { start: 0, end: 99 }
        );
        assert_eq!(
            resolve(
                RangeRequest::Single {
                    start: 990,
                    end: Some(2000)
                },
                size
            ),
            Resolution::Partial {
                start: 990,
                end: 999
            },
            "终点越界饱和到 size-1"
        );
        assert_eq!(
            resolve(
                RangeRequest::Single {
                    start: 5,
                    end: None
                },
                size
            ),
            Resolution::Partial { start: 5, end: 999 }
        );
        assert_eq!(
            resolve(
                RangeRequest::Single {
                    start: 1000,
                    end: Some(1010)
                },
                size
            ),
            Resolution::Unsatisfiable
        );
        assert_eq!(resolve(RangeRequest::Full, size), Resolution::Full);
        assert_eq!(
            resolve(
                RangeRequest::Single {
                    start: 0,
                    end: Some(0)
                },
                0
            ),
            Resolution::Unsatisfiable,
            "空文件上的任何 Range 都不可满足"
        );
    }
}
