//! M3⑥-2 判据③：把 oracle 真身探针产出的任务态位向量**逐 op 逐字节**核对进 Rust。
//!
//! 向量来源：`mir2-rs/tests/parity/m3/quest-state-probe`（C#，`new PlayObject()` 直调
//! `GetQuestFalgStatus`/`SetQuestFlagStatus`/`GetQuestUnitStatus`/`SetQuestUnitStatus`/
//! `GetQuestUnitOpenStatus`/`SetQuestUnitOpenStatus`，`PlayObject.cs:3966-4110`）
//!   → `quest-state-probe/vectors/quest_state_vectors.{json,tsv}`
//!
//! 为什么读 TSV 而不是 JSON：本 crate 无 `serde_json` 依赖；探针与 JSON 同一次运行写出
//! 行式伴生件 `.tsv`，本用例零依赖解析。
//!
//! 被测实现：`mir2_script::engine::quest_state::QuestBitmap`（`crates/script/src/engine/quest_state.rs`）。
//!
//! 判据覆盖（盘点报告要求）：0/31/32 位边界、跨数组下标（8→9、31→32→33、255→256、1024）、
//! 清零语义（只清目标位）、越界 no-op（0 / 负数 / 1025）。
//!
//! 红检（自动）：`redcheck_bit_direction` 把同样逐字节比较作用在一份**故意改坏**的位方向
//! 实现（`1 << n` 而非 `128 >> n`）上，断言必出差异；`redcheck_comparator` 证明比较器
//! 对「把 oracle 向量改一个字节」也会红 —— 逐字段一致不是恒绿。

use std::collections::BTreeMap;
use std::path::PathBuf;

use mir2_script::engine::quest_state::QuestBitmap;

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/parity/m3/quest-state-probe/vectors")
}

struct Row {
    array: String,
    op: String,
    flag: i32,
    value: i32,
    get: i32,
    bytes: String,
}

fn read_rows() -> Vec<Row> {
    let path = vectors_dir().join("quest_state_vectors.tsv");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("读不到位向量 {}: {e}", path.display()));
    text.lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            assert_eq!(f.len(), 6, "TSV 列数错误：{l}");
            Row {
                array: f[0].to_string(),
                op: f[1].to_string(),
                flag: f[2].parse().unwrap(),
                value: f[3].parse().unwrap(),
                get: f[4].parse().unwrap(),
                bytes: f[5].to_string(),
            }
        })
        .collect()
}

fn hex(bytes: &[u8; 128]) -> String {
    let mut s = String::with_capacity(256);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// 按 oracle 向量回放：`set` 才写、`read` 只读；每步都比对 `get` 与原始 128 字节。
fn replay(rows: &[Row]) -> Result<(), String> {
    let mut maps: BTreeMap<&str, QuestBitmap> = BTreeMap::new();
    for a in ["quest_flag", "quest_unit", "quest_unit_open"] {
        maps.insert(a, QuestBitmap::new());
    }
    for (i, r) in rows.iter().enumerate() {
        let map = maps
            .get_mut(r.array.as_str())
            .ok_or_else(|| format!("第 {i} 行未知数组 {}", r.array))?;
        if r.op == "set" {
            map.set(r.flag, r.value);
        } else {
            assert_eq!(r.op, "read", "第 {i} 行未知 op {}", r.op);
        }
        if map.get(r.flag) != r.get {
            return Err(format!(
                "第 {i} 行 {} {} get 不符：oracle={} rust={}",
                r.array,
                r.op,
                r.get,
                map.get(r.flag)
            ));
        }
        let got = hex(map.as_bytes());
        if got != r.bytes {
            return Err(format!(
                "第 {i} 行 {} {} 128 字节不符：\n  oracle={}\n  rust  ={}",
                r.array, r.op, r.bytes, got
            ));
        }
    }
    Ok(())
}

#[test]
fn quest_bitmap_matches_oracle_byte_for_byte() {
    let rows = read_rows();
    // 三组数组都要有行（每 32 个 op 一组）。
    for a in ["quest_flag", "quest_unit", "quest_unit_open"] {
        let n = rows.iter().filter(|r| r.array == a).count();
        assert_eq!(n, 32, "{a} 向量行数");
    }
    replay(&rows).expect("任务态位向量应逐 op 逐字节与 oracle 一致");

    // 关键边界的存在性（防止向量被意外删空后测试恒绿）。
    let has = |array: &str, flag: i32| rows.iter().any(|r| r.array == array && r.flag == flag);
    for flag in [0, 1, 32, 107, 1024, 1025, -3] {
        assert!(has("quest_flag", flag), "缺 flag={flag} 的边界向量");
    }
}

/// 改坏必红（位方向）：`1 << n` 的实现与 oracle 的 `128 >> n` 在**每个** set 行上必然不同，
/// 证明这组向量真的钉住了位序，而不是只要"能置位"就过。
#[test]
fn redcheck_bit_direction() {
    struct BadBitmap {
        bytes: [u8; 128],
    }
    impl BadBitmap {
        fn get(&self, n_flag: i32) -> i32 {
            let n_flag = n_flag - 1;
            if n_flag < 0 {
                return 0;
            }
            let idx = n_flag / 8;
            let bit_no = n_flag % 8;
            if (idx - 128) < 0 {
                if (1u8 << bit_no) & self.bytes[idx as usize] != 0 {
                    1
                } else {
                    0
                }
            } else {
                0
            }
        }
        fn set(&mut self, n_flag: i32, n_value: i32) {
            let n_flag = n_flag - 1;
            if n_flag < 0 {
                return;
            }
            let idx = n_flag / 8;
            let bit_no = n_flag % 8;
            if (idx - 128) < 0 {
                let bit = 1u8 << bit_no;
                let old = self.bytes[idx as usize];
                self.bytes[idx as usize] = if n_value == 0 { !bit & old } else { bit | old };
            }
        }
    }

    let rows = read_rows();
    let mut m = BadBitmap { bytes: [0; 128] };
    let mut mismatches = 0usize;
    for (i, r) in rows.iter().enumerate() {
        if r.array != "quest_flag" {
            continue;
        }
        if r.op == "set" {
            m.set(r.flag, r.value);
        }
        if m.get(r.flag) != r.get {
            mismatches += 1;
            continue;
        }
        let bad_hex = hex(&m.bytes);
        if bad_hex != r.bytes {
            let _ = i;
            mismatches += 1;
        }
    }
    assert!(
        mismatches > 0,
        "位方向反了的实现应与 oracle 向量不符（否则向量没有仲裁力）"
    );
}

/// 改坏必红（比较器）：把 oracle 向量改掉一个字节，比较器必须报错。
#[test]
fn redcheck_comparator_detects_mutated_vector() {
    let mut rows = read_rows();
    // 找第一行 bytes 非全零的 set 行，翻转它的一个 hex nibble。
    let target = rows
        .iter_mut()
        .find(|r| r.op == "set" && r.bytes.chars().any(|c| c != '0'))
        .expect("应有至少一行非零 set 向量");
    let mut chars: Vec<char> = target.bytes.chars().collect();
    chars[0] = if chars[0] == 'f' { '0' } else { 'f' };
    target.bytes = chars.into_iter().collect();
    assert!(replay(&rows).is_err(), "改坏 oracle 向量后比较器必须报红");
}
