//! reviewer-5 Minor ① 的真身对拍：`PlayObject.StruckDamage`（`src/M2Server/Player/PlayObject.cs:2954`）
//! 的衣服位（`ItemLocation.Dress = 0`）特判与逐格耐久磨损的**取数账本 + 前后状态**。
//!
//! 向量来源 = `mir2-rs/tests/parity/m3/struck-durability-probe`（C#，直调真身
//! `PlayObject.StruckDamage`，`RecordingRandom` 记账；`NullItemSystem` 替身只为避开
//! `SendDelItems` 的帧面）→ `vectors/struck_durability_vectors.{json,tsv}`。
//!
//! 本用例用 [`ReplayRandom`] **强制回放 oracle 的取数结果**，再断言：
//!   1. 取数序列零分歧（kind/arg 逐条相同）且恰好消费完；
//!   2. `use_items` 的「仍装备（`Index > 0`）」状态逐字段相同。
//!
//! 关键分类（C# `ushort` 的两个易错角落，全部由真身实测坐实）：
//!   - `dress_break`：`Dura == nDam` ⇒ 特判里破零清 `Index` ⇒ **同一击**逐格循环跳过一次
//!     `Random(8)`（`PlayObject.cs:2965-2996` 在 `:2998-3039` 之前）；
//!   - `dress_wrap`：`Dura < nDam` ⇒ unchecked `ushort` 回绕成大正数 ⇒ **不破零**、照抽
//!     `Random(8)`（`:2969-2970`）；
//!   - `loop_break`：循环里 `Random(8)==0` 且 `Dura == nDam` ⇒ 清 `Index`（`:3008`/`:3012`），
//!     影响**后续攻击**的取数。
//!
//! 改坏必红：把 `crates/world/src/combat.rs::wear_durability` 的 `slot.index = 0` 去掉 ⇒
//! `dress_break`/`loop_break` 状态不符；把 `wrapping_sub` 换成 `saturating_sub` ⇒ `dress_wrap` 不符。

use std::path::PathBuf;

use mir2_shared::replay::ReplayRandom;
use mir2_world::combat::struck_damage_durability;
use mir2_world::{Entity, ACTOR_RACE_PLAY};

fn vectors_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/parity/m3/struck-durability-probe/vectors/struck_durability_vectors.tsv")
}

#[derive(Debug)]
struct Case {
    name: String,
    n_dam: i32,
    draws: String,
    items_in: Vec<(usize, u16, u16)>,
    items_out: Vec<(usize, u16, u16)>,
}

fn parse_items(s: &str) -> Vec<(usize, u16, u16)> {
    if s.is_empty() {
        return Vec::new();
    }
    s.split(';')
        .map(|part| {
            let f: Vec<&str> = part.split(':').collect();
            assert_eq!(f.len(), 3, "装备位编码非法: {part}");
            (
                f[0].parse().expect("slot"),
                f[1].parse().expect("index"),
                f[2].parse().expect("dura"),
            )
        })
        .collect()
}

fn load() -> Vec<Case> {
    let path = vectors_path();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("读不到真身向量 {}: {e}", path.display()));
    text.lines()
        .skip(1) // 表头
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            assert_eq!(f.len(), 7, "向量列数非法: {line}");
            assert_eq!(f[0], "cloth", "未知行类型: {line}");
            Case {
                name: f[1].to_string(),
                n_dam: f[3].parse().expect("n_dam"),
                draws: f[4].to_string(),
                items_in: parse_items(f[5]),
                items_out: parse_items(f[6]),
            }
        })
        .collect()
}

/// 把探针的行式账本（`seq|kind|arg|result|site`，以 `;` 连接）转成 [`ReplayRandom`]。
fn replay(draws: &str) -> ReplayRandom {
    let mut tsv = String::new();
    for (i, line) in draws.split(';').enumerate() {
        let f: Vec<&str> = line.split('|').collect();
        assert_eq!(f.len(), 5, "取数账本行非法: {line}");
        tsv.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            i + 1,
            f[1],
            f[2],
            f[3],
            f[4]
        ));
    }
    ReplayRandom::from_tsv(&tsv).unwrap_or_else(|e| panic!("取数账本非法: {e} ({draws})"))
}

fn mk_target(items: &[(usize, u16, u16)]) -> Entity {
    let mut target = Entity::new(202, "victim", 0, 0);
    target.race = ACTOR_RACE_PLAY;
    target.hp = 30000;
    target.max_hp = 30000;
    for &(slot, index, dura) in items {
        target.use_items[slot].index = index;
        target.use_items[slot].dura = dura;
    }
    target
}

fn equipped(target: &Entity) -> Vec<(usize, u16, u16)> {
    target
        .use_items
        .iter()
        .enumerate()
        .filter(|(_, s)| s.index > 0)
        .map(|(i, s)| (i, s.index, s.dura))
        .collect()
}

#[test]
fn struck_durability_matches_oracle_vectors() {
    let cases = load();
    assert!(cases.len() >= 5, "真身向量至少 5 条，实得 {}", cases.len());

    for case in &cases {
        let mut target = mk_target(&case.items_in);
        let mut rng = replay(&case.draws);
        struck_damage_durability(&mut rng, &mut target);

        assert!(
            rng.divergences().is_empty(),
            "[{}] 取数账本分歧: {:?}",
            case.name,
            rng.divergences()
        );
        assert_eq!(
            rng.remaining(),
            0,
            "[{}] 取数账本未消费完（多出 {} 条）",
            case.name,
            rng.remaining()
        );
        assert_eq!(
            equipped(&target),
            case.items_out,
            "[{}] 耐久/清位状态与 oracle 不一致（nDam={}）",
            case.name,
            case.n_dam
        );
    }

    // 分类牙齿：五个关键用例都必须在，且两类取数次数确实不同（不是恒绿）。
    let names: Vec<&str> = cases.iter().map(|c| c.name.as_str()).collect();
    for need in [
        "dress_break",
        "dress_wrap",
        "dress_normal",
        "dress_empty",
        "loop_break",
    ] {
        assert!(names.contains(&need), "真身向量缺少用例 {need}");
    }

    let draw8_count = |name: &str| {
        cases
            .iter()
            .find(|c| c.name == name)
            .unwrap()
            .draws
            .matches("Next(max)|8")
            .count()
    };
    assert_eq!(
        draw8_count("dress_break"),
        2,
        "衣服位破零 ⇒ 同一击只给 slot1/slot7 抽 Random(8)（slot0 被跳过）"
    );
    assert_eq!(
        draw8_count("dress_wrap"),
        2,
        "衣服位短耐久回绕不破零 ⇒ slot0/slot1 各抽一次 Random(8)"
    );

    let wrap_dura = cases
        .iter()
        .find(|c| c.name == "dress_wrap")
        .unwrap()
        .items_out
        .iter()
        .find(|(slot, _, _)| *slot == 0)
        .expect("dress_wrap 的 slot0 仍在装备")
        .2;
    assert_eq!(
        wrap_dura, 65534,
        "Dura=11、nDam=13 ⇒ unchecked ushort 回绕到 65534（不是 saturating 到 0）"
    );
}
