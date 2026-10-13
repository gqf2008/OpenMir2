//! 宝宝/召唤配置项默认值向量门禁（`mir2-data`）。
//!
//! 向量文件 [`data/pet_slave_defaults.tsv`](data/pet_slave_defaults.tsv) 逐行给出
//! `字段 <TAB> 默认值 <TAB> oracle 锚`，来源是 `src/Modules/SystemModule/Conf/Model/GameSvrConf.cs`
//! 的构造函数初值（盘点 §1.C/§1.F）。本测试把 [`PetSlaveConfig::default()`] 的每个字段与向量逐条比对：
//! 向量错、默认值错、或字段漏进向量（行数不符）都会红。
//!
//! 交付卡：`openmir2-m3-pet-slave-config`（M3⑨-实现 1）。

use mir2_data::pet_slave::{PetSlaveConfig, RecallMigic};

const VECTOR: &str = include_str!("data/pet_slave_defaults.tsv");

/// `RecallMigic` → `nHumLevel:sMonName:nCount:nLevel`（C# null 的 `sMonName` 记为空串）。
fn migic_repr(m: &RecallMigic) -> String {
    format!(
        "{}:{}:{}:{}",
        m.n_hum_level, m.s_mon_name, m.n_count, m.n_level
    )
}

/// 向量字段名 → `PetSlaveConfig` 当前默认值的字符串形式。
fn default_value(cfg: &PetSlaveConfig, field: &str) -> Option<String> {
    let v = match field {
        "skeleton" => cfg.skeleton.clone(),
        "skeleton_count" => cfg.skeleton_count.to_string(),
        "skeleton_array" => cfg
            .skeleton_array
            .iter()
            .map(migic_repr)
            .collect::<Vec<_>>()
            .join(";"),
        "dragon" => cfg.dragon.clone(),
        "dragon1" => cfg.dragon1.clone(),
        "dragon_count" => cfg.dragon_count.to_string(),
        "dragon_array" => cfg
            .dragon_array
            .iter()
            .map(migic_repr)
            .collect::<Vec<_>>()
            .join(";"),
        "angel" => cfg.angel.clone(),
        "mon_up_lv_need_kill_base" => cfg.mon_up_lv_need_kill_base.to_string(),
        "mon_up_lv_rate" => cfg.mon_up_lv_rate.to_string(),
        "mon_up_lv_need_kill_count" => cfg
            .mon_up_lv_need_kill_count
            .iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join(","),
        "slave_color" => cfg
            .slave_color
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(","),
        "spirit_mutiny" => cfg.spirit_mutiny.to_string(),
        "spirit_mutiny_time" => cfg.spirit_mutiny_time.to_string(),
        "master_die_mutiny" => cfg.master_die_mutiny.to_string(),
        "master_die_mutiny_rate" => cfg.master_die_mutiny_rate.to_string(),
        "master_die_mutiny_power" => cfg.master_die_mutiny_power.to_string(),
        "master_die_mutiny_speed" => cfg.master_die_mutiny_speed.to_string(),
        "bb_mon_auto_change_color" => cfg.bb_mon_auto_change_color.to_string(),
        "bb_mon_auto_change_color_time" => cfg.bb_mon_auto_change_color_time.to_string(),
        "mag_tamming_level" => cfg.mag_tamming_level.to_string(),
        "mag_tamming_target_level" => cfg.mag_tamming_target_level.to_string(),
        "mag_tamming_hp_rate" => cfg.mag_tamming_hp_rate.to_string(),
        "mag_tamming_count" => cfg.mag_tamming_count.to_string(),
        _ => return None,
    };
    Some(v)
}

#[test]
fn defaults_match_oracle_vector() {
    let cfg = PetSlaveConfig::default();
    let mut checked = 0usize;
    for (lineno, line) in VECTOR.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut cols = line.split('\t');
        let field = cols.next().expect("字段列");
        let expected = cols.next().expect("默认值列");
        let anchor = cols.next().expect("oracle 锚列");
        let actual = default_value(&cfg, field).unwrap_or_else(|| {
            panic!(
                "向量第 {} 行字段 `{field}` 不存在于 PetSlaveConfig",
                lineno + 1
            )
        });
        assert_eq!(
            actual,
            expected,
            "向量第 {} 行 `{field}`（{anchor}）与 oracle 初值不一致",
            lineno + 1
        );
        checked += 1;
    }
    assert_eq!(checked, 24, "向量行数应为 24（全部宝宝/召唤配置项）");
}

/// 数组长度钉死（oracle `new RecallMigic[10]` / 15 项标量数组）：改长度 ⇒ 本用例必红。
#[test]
fn array_lengths_match_oracle() {
    let cfg = PetSlaveConfig::default();
    assert_eq!(cfg.skeleton_array.len(), 10);
    assert_eq!(cfg.dragon_array.len(), 10);
    assert_eq!(cfg.mon_up_lv_need_kill_count.len(), 15);
    assert_eq!(cfg.slave_color.len(), 15);
}

/// 等级/叛变窗口常量（`Grobal2.cs:23` / `MagicManager.cs:22`）。
#[test]
fn constants_match_oracle() {
    use mir2_data::pet_slave::{DEFAULT_ROYALTY_SEC, SLAVE_MAX_LEVEL};
    assert_eq!(SLAVE_MAX_LEVEL, 50);
    assert_eq!(DEFAULT_ROYALTY_SEC, 10 * 24 * 60 * 60);
}
