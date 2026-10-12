//! 行会/攻城配置项默认值向量门禁（`mir2-data`）。
//!
//! 向量文件 [`data/guild_castle_defaults.tsv`](data/guild_castle_defaults.tsv) 逐行给出
//! `字段 <TAB> 默认值 <TAB> oracle 锚`，来源是 `src/Modules/SystemModule/Conf/Model/GameSvrConf.cs`
//! 的构造函数初值（盘点 §1.K）。本测试把 [`GuildCastleConfig::default()`] 的每个字段与向量逐条比对：
//! 向量错、默认值错、或字段漏进向量（行数不符）都会红。
//!
//! 交付卡：`openmir2-m3-guild-load-config`（M3⑧-实现 2）。

use mir2_data::guild_castle::GuildCastleConfig;

const VECTOR: &str = include_str!("data/guild_castle_defaults.tsv");

/// 向量字段名 → `GuildCastleConfig` 当前默认值的字符串形式。
fn default_value(cfg: &GuildCastleConfig, field: &str) -> Option<String> {
    let v = match field {
        "guild_dir" => cfg.guild_dir.clone(),
        "guild_file" => cfg.guild_file.clone(),
        "castle_dir" => cfg.castle_dir.clone(),
        "castle_file" => cfg.castle_file.clone(),
        "woma_horn" => cfg.woma_horn.clone(),
        "zuma_piece" => cfg.zuma_piece.clone(),
        "build_guild_price" => cfg.build_guild_price.to_string(),
        "guild_war_price" => cfg.guild_war_price.to_string(),
        "guild_war_time" => cfg.guild_war_time.to_string(),
        "guild_name_len" => cfg.guild_name_len.to_string(),
        "guild_member_max_limit" => cfg.guild_member_max_limit.to_string(),
        "guild_rank_name_len" => cfg.guild_rank_name_len.to_string(),
        "castle_name_len" => cfg.castle_name_len.to_string(),
        "start_castle_war_days" => cfg.start_castle_war_days.to_string(),
        "start_castlewar_time" => cfg.start_castlewar_time.to_string(),
        "show_castle_war_end_msg_time" => cfg.show_castle_war_end_msg_time.to_string(),
        "castle_war_time" => cfg.castle_war_time.to_string(),
        "get_castle_time" => cfg.get_castle_time.to_string(),
        "hire_guard_price" => cfg.hire_guard_price.to_string(),
        "hire_archer_price" => cfg.hire_archer_price.to_string(),
        "repair_door_price" => cfg.repair_door_price.to_string(),
        "repair_wall_price" => cfg.repair_wall_price.to_string(),
        "castle_gold_max" => cfg.castle_gold_max.to_string(),
        "castle_one_day_gold" => cfg.castle_one_day_gold.to_string(),
        "castle_tax_rate" => cfg.castle_tax_rate.to_string(),
        "get_all_npc_tax" => cfg.get_all_npc_tax.to_string(),
        "castle_member_price_rate" => cfg.castle_member_price_rate.to_string(),
        "castle_name" => cfg.castle_name.clone(),
        "castle_home_map" => cfg.castle_home_map.clone(),
        "castle_home_x" => cfg.castle_home_x.to_string(),
        "castle_home_y" => cfg.castle_home_y.to_string(),
        "castle_war_range_x" => cfg.castle_war_range_x.to_string(),
        "castle_war_range_y" => cfg.castle_war_range_y.to_string(),
        "guild_notice" => cfg.guild_notice.clone(),
        "guild_war_header" => cfg.guild_war_header.clone(),
        "guild_all" => cfg.guild_all.clone(),
        "guild_member" => cfg.guild_member.clone(),
        "guild_member_rank" => cfg.guild_member_rank.clone(),
        "guild_chief" => cfg.guild_chief.clone(),
        _ => return None,
    };
    Some(v)
}

#[test]
fn defaults_match_oracle_vector() {
    let cfg = GuildCastleConfig::default();
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
                "向量第 {} 行字段 `{field}` 不存在于 GuildCastleConfig",
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
    assert_eq!(checked, 39, "向量行数应为 39（全部行会/攻城配置项）");
}
