//! 记录-回放随机源（M3 对拍用）。
//!
//! 用途：C# oracle 侧的随机序列本身**不可固定**（无参 `new Random()`，见白名单 D-3），
//! 但可以**记录**——`m3/rng-hook` 把真实进程里每一次取数（形态/参数/结果/调用点）落盘。
//! Rust 侧用 [`ReplayRandom`] 逐条消费同一份记录：
//!
//! - 每次调用都要求**形态与参数**与记录一致：不一致即记入 `divergences`
//!   （调用顺序错、次数多/少、上限不同都会暴露，并给出记录里的调用点）；
//! - 返回值取记录值，于是"公式是否等价"退化成一个纯比较问题：
//!   只要 Rust 侧在同一输入下的**取数序列**与 C# 一致，两侧输出就必然一致。
//!
//! 记录格式（`RngSeedHook` 输出，`\t` 分隔；`#` 开头为注释）：
//! `seq \t kind \t arg \t result \t site?`，其中
//! `kind ∈ {Next(), Next(max), Next(min,max), NextDouble(), Sample()}`；
//! `Next(min,max)` 的 arg 打包为 `min<<32 | max`（高 32 位 min、低 32 位 max，按无符号打包）。

use crate::rng::RandomSource;

/// 记录里的一行。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayEntry {
    pub seq: u64,
    pub kind: String,
    pub arg: i64,
    pub result: i64,
    pub site: String,
}

impl ReplayEntry {
    /// 解包 `Next(min,max)` 的 arg。
    pub fn min_max(&self) -> (i32, i32) {
        let min = (self.arg >> 32) as i32;
        let max = (self.arg & 0xFFFF_FFFF) as i32;
        (min, max)
    }

    /// 把一条记录重放到给定的随机源上（用于「同种子重放」：用记录里的形态/参数驱动
    /// 本端实现，再比对返回值是否等于记录值）。
    pub fn replay_onto(&self, rng: &mut impl RandomSource) -> i64 {
        match self.kind.as_str() {
            "Next()" => i64::from(rng.random()),
            "Next(max)" => i64::from(rng.random_below(self.arg as i32)),
            "Next(min,max)" => {
                let (min, max) = self.min_max();
                i64::from(rng.random_range(min, max))
            }
            "NextDouble()" | "Sample()" => f64::to_bits(rng.next_double()) as i64,
            other => panic!("未知的记录形态: {other}"),
        }
    }
}

/// 记录流解析错误（本 crate 保持零依赖，手写而非引入 thiserror）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayError {
    BadLine { line: usize, text: String },
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplayError::BadLine { line, text } => {
                write!(f, "第 {line} 行格式非法: {text}")
            }
        }
    }
}

impl std::error::Error for ReplayError {}

/// 记录流回放源。
#[derive(Clone, Debug)]
pub struct ReplayRandom {
    entries: Vec<ReplayEntry>,
    pos: usize,
    divergences: Vec<String>,
}

impl ReplayRandom {
    /// 从记录文本构造（`#` 开头为注释行，空行忽略）。
    pub fn from_tsv(text: &str) -> Result<Self, ReplayError> {
        let mut entries = Vec::new();
        for (i, line) in text.lines().enumerate() {
            let line = line.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() < 4 {
                return Err(ReplayError::BadLine {
                    line: i + 1,
                    text: line.to_string(),
                });
            }
            let seq = f[0].parse::<u64>().unwrap_or(0);
            let arg = f[2].parse::<i64>().map_err(|_| ReplayError::BadLine {
                line: i + 1,
                text: line.to_string(),
            })?;
            let result = f[3].parse::<i64>().map_err(|_| ReplayError::BadLine {
                line: i + 1,
                text: line.to_string(),
            })?;
            entries.push(ReplayEntry {
                seq,
                kind: f[1].to_string(),
                arg,
                result,
                site: f.get(4).copied().unwrap_or("").to_string(),
            });
        }
        Ok(ReplayRandom {
            entries,
            pos: 0,
            divergences: Vec::new(),
        })
    }

    /// 从文件路径构造。
    pub fn from_file(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let text = std::fs::read_to_string(path)?;
        Ok(Self::from_tsv(&text)?)
    }

    pub fn entries(&self) -> &[ReplayEntry] {
        &self.entries
    }

    /// 已消费条数。
    pub fn consumed(&self) -> usize {
        self.pos
    }

    /// 未消费条数。
    pub fn remaining(&self) -> usize {
        self.entries.len().saturating_sub(self.pos)
    }

    /// 记录到的形态/参数不匹配（空 = 两侧取数序列一致）。
    pub fn divergences(&self) -> &[String] {
        &self.divergences
    }

    fn take(&mut self, kind: &str, arg: i64) -> i64 {
        let Some(entry) = self.entries.get(self.pos) else {
            self.divergences.push(format!(
                "第 {} 次调用越界: 记录已耗尽，多出 {kind} arg={arg}",
                self.pos + 1
            ));
            return 0;
        };
        if entry.kind == kind && entry.arg == arg {
            self.pos += 1;
            return entry.result;
        }
        self.divergences.push(format!(
            "第 {} 次调用不匹配: 记录为 {} arg={} (seq={} site={}), 本端为 {kind} arg={arg}",
            self.pos + 1,
            entry.kind,
            entry.arg,
            entry.seq,
            entry.site
        ));
        // 不推进游标：后续调用继续与同一条记录比对，便于一次性看清整段偏移
        entry.result
    }
}

impl RandomSource for ReplayRandom {
    fn random(&mut self) -> i32 {
        self.take("Next()", 0) as i32
    }

    fn random_below(&mut self, value: i32) -> i32 {
        self.take("Next(max)", i64::from(value)) as i32
    }

    fn random_range(&mut self, min_value: i32, max_value: i32) -> i32 {
        let packed = ((min_value as i64) << 32) | (max_value as u32 as i64);
        self.take("Next(min,max)", packed) as i32
    }

    fn next_double(&mut self) -> f64 {
        f64::from_bits(self.take("NextDouble()", 0) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::RandomNumber;

    /// 用本端同种子实现产出记录，再回放：应零分歧且恰好消费完。
    #[test]
    fn replay_round_trip() {
        let mut seed_rng = RandomNumber::with_seed(42);
        let mut tsv = String::new();
        for i in 1..=20u64 {
            let (kind, arg, result) = if i % 3 == 0 {
                ("Next(max)", 100, seed_rng.random_below(100))
            } else if i % 3 == 1 {
                ("Next()", 0, seed_rng.random())
            } else {
                let packed = ((10i64) << 32) | (1000u32 as i64);
                ("Next(min,max)", packed, seed_rng.random_range(10, 1000))
            };
            tsv.push_str(&format!("{i}\t{kind}\t{arg}\t{result}\tsite{i}\n"));
        }
        let mut replay = ReplayRandom::from_tsv(&tsv).unwrap();
        for i in 1..=20u64 {
            if i % 3 == 0 {
                replay.random_below(100);
            } else if i % 3 == 1 {
                replay.random();
            } else {
                replay.random_range(10, 1000);
            }
        }
        assert!(
            replay.divergences().is_empty(),
            "{:?}",
            replay.divergences()
        );
        assert_eq!(replay.consumed(), 20);
        assert_eq!(replay.remaining(), 0);
    }

    /// 红检：调用形态/参数不一致必须被记录（顺序错、上限错、多调用三种都要能抓）。
    #[test]
    fn replay_detects_divergence() {
        let tsv = "1\tNext(max)\t100\t5\tsiteA\n2\tNext(max)\t50\t7\tsiteB\n";
        // 形态不符
        let mut r = ReplayRandom::from_tsv(tsv).unwrap();
        r.random_below(100);
        r.random_below(51); // 记录是 50
        assert_eq!(r.divergences().len(), 1, "{:?}", r.divergences());
        assert!(
            r.divergences()[0].contains("Next(max)"),
            "{}",
            r.divergences()[0]
        );
        assert!(
            r.divergences()[0].contains("siteB"),
            "{}",
            r.divergences()[0]
        );

        // 多调用（越界）
        let mut r = ReplayRandom::from_tsv(tsv).unwrap();
        r.random_below(100);
        r.random_below(50);
        r.random_below(50);
        assert_eq!(r.divergences().len(), 1);
        assert!(
            r.divergences()[0].contains("越界"),
            "{}",
            r.divergences()[0]
        );
    }
}
