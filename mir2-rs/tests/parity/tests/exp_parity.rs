//! 门禁 ③：经验曲线逐级一致（1→255 全表 diff = 0），含 `[Exp]` 标量。
//!
//! 金标准：`golden/exp_table.txt`，由 `csharp/ParityGolden exp` 用**真实
//! `ConfigFile.cs`** 读夹具 `fixtures/Exps.conf`（现网 `E:\MirServer\M2GameSvr\Exps.conf`
//! 的只读副本）产出：1000 行 NeedExps + 11 行标量。

use std::path::Path;

use mir2_data::exps::ExpsConfig;
use mir2_data::models::MAX_LEVEL;
use mir2_formula::exp::get_level_exp;

fn fixture_exps() -> ExpsConfig {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/Exps.conf");
    ExpsConfig::from_file(&path).expect("读取夹具 Exps.conf 失败")
}

fn golden_lines() -> Vec<&'static str> {
    include_str!("../golden/exp_table.txt").lines().collect()
}

#[test]
fn need_exps_table_matches_golden() {
    let cfg = fixture_exps();
    let golden = golden_lines();
    assert!(golden.len() >= 1000, "金标准行数异常: {}", golden.len());
    let mut diffs = 0;
    for (i, line) in golden.iter().take(1000).enumerate() {
        let expected: i32 = line.parse().unwrap();
        if cfg.need_exps[i] != expected {
            diffs += 1;
            if diffs <= 5 {
                eprintln!("Level{i}: rust={} csharp={expected}", cfg.need_exps[i]);
            }
        }
    }
    assert_eq!(diffs, 0, "NeedExps 全表 diff = {diffs}（要求 0）");
}

#[test]
fn get_level_exp_1_to_max_matches_golden() {
    // 验收 ③：经验曲线（含英雄，与人物同表）逐级一致
    let cfg = fixture_exps();
    let golden = golden_lines();
    for level in 1..=MAX_LEVEL {
        let expected: i32 = golden[level as usize].parse().unwrap();
        assert_eq!(
            get_level_exp(&cfg.need_exps, MAX_LEVEL, level),
            expected,
            "Level{level} 升级经验不一致"
        );
    }
}

#[test]
fn exp_scalars_match_golden() {
    let cfg = fixture_exps();
    let golden = golden_lines();
    let scalars: Vec<(&str, String)> = vec![
        ("LimitExpLevel", cfg.limit_exp_level.to_string()),
        ("LimitExpValue", cfg.limit_exp_value.to_string()),
        ("KillMonExpMultiple", cfg.kill_mon_exp_multiple.to_string()),
        // C# bool.ToString() 是 "True"/"False"
        (
            "HighLevelKillMonFixExp",
            if cfg.high_level_kill_mon_fix_exp {
                "True".into()
            } else {
                "False".into()
            },
        ),
        (
            "HighLevelGroupFixExp",
            if cfg.high_level_group_fix_exp {
                "True".into()
            } else {
                "False".into()
            },
        ),
        (
            "UseFixExp",
            if cfg.use_fix_exp {
                "True".into()
            } else {
                "False".into()
            },
        ),
        (
            "MonDelHptoExp",
            if cfg.mon_del_hp_to_exp {
                "True".into()
            } else {
                "False".into()
            },
        ),
        ("BaseExp", cfg.base_exp.to_string()),
        ("AddExp", cfg.add_exp.to_string()),
        ("MonHptoExpLevel", cfg.mon_hp_to_exp_level.to_string()),
        ("MonHptoExpmax", cfg.mon_hp_to_exp_max.to_string()),
    ];
    for (key, value) in scalars {
        let expected = format!("{key}={value}");
        assert!(
            golden.iter().any(|l| *l == expected),
            "标量不一致: rust `{expected}` 不在金标准中"
        );
    }
}

/// 红检（自动部分）：篡改一级经验后必须可检出。
#[test]
fn redcheck_tampered_table_must_differ() {
    let cfg = fixture_exps();
    let golden = golden_lines();
    let mut tampered = cfg.need_exps;
    tampered[50] = tampered[50].wrapping_add(1);
    let expected: i32 = golden[50].parse().unwrap();
    assert_ne!(tampered[50], expected, "红检失效：篡改未改变比较结果");
}
