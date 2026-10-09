//! 等级基础属性/负重的对拍（验收外的加固门禁，覆盖「负重/属性加成」公式）。
//!
//! 金标准：`golden/levelabil.txt`（`csharp/ParityGolden levelabil`，
//! `RecalcLevelAbilitys` 原 C# 逐字拷贝，三职业 × 0..=255 级共 768 行）。
//! 行格式：`job level maxHP maxMP maxWeight maxWearWeight maxHandWeight dc mc sc ac mac`

use mir2_formula::level_ability::{recalc_level_abilitys, LevelValueConfig, PlayerJob};

#[test]
fn level_ability_all_jobs_levels_match_golden() {
    let golden = include_str!("../golden/levelabil.txt");
    let cfg = LevelValueConfig::default();
    let mut checked = 0;
    for (lineno, line) in golden.lines().enumerate() {
        let f: Vec<i64> = line.split(' ').map(|s| s.parse().unwrap()).collect();
        assert_eq!(f.len(), 12, "金标准行 {} 格式异常", lineno + 1);
        let job = match f[0] {
            0 => PlayerJob::Warrior,
            1 => PlayerJob::Wizard,
            2 => PlayerJob::Taoist,
            other => panic!("未知 job {other}"),
        };
        let a = recalc_level_abilitys(job, f[1] as u8, &cfg);
        let actual = [
            i64::from(a.max_hp),
            i64::from(a.max_mp),
            i64::from(a.max_weight),
            i64::from(a.max_wear_weight),
            i64::from(a.max_hand_weight),
            i64::from(a.dc),
            i64::from(a.mc),
            i64::from(a.sc),
            i64::from(a.ac),
            i64::from(a.mac),
        ];
        assert_eq!(
            actual,
            f[2..],
            "job={} level={} 不一致（金标准行 {}）",
            f[0],
            f[1],
            lineno + 1
        );
        checked += 1;
    }
    assert_eq!(checked, 768, "应覆盖 3 职业 × 256 级");
}

/// 红检（自动部分）：改配置参数 → 结果必须不同。
#[test]
fn redcheck_changed_config_must_differ() {
    let mut cfg = LevelValueConfig::default();
    let base = recalc_level_abilitys(PlayerJob::Warrior, 50, &cfg);
    cfg.warr_hp = 5; // nLevelValueOfWarrHP 4 → 5
    let changed = recalc_level_abilitys(PlayerJob::Warrior, 50, &cfg);
    assert_ne!(base, changed, "红检失效：配置变更未改变结果");
}
