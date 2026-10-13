//! 组队/交易配置项默认值向量门禁（`mir2-data`）。
//!
//! 向量文件 [`data/party_defaults.tsv`](data/party_defaults.tsv) 逐行给出
//! `字段 <TAB> 默认值 <TAB> oracle 锚`，来源是 `src/Modules/SystemModule/Conf/Model/GameSvrConf.cs`
//! 的构造函数初值（盘点 §1.N）。本测试把 [`PartyConfig::default()`] 的每个字段与向量逐条比对：
//! 向量错、默认值错、或字段漏进向量（行数不符）都会红。
//!
//! 交付卡：`openmir2-m3-party-load-config`（M3⑦-实现 1）。

use mir2_data::party::PartyConfig;

const VECTOR: &str = include_str!("data/party_defaults.tsv");

/// 向量字段名 → `PartyConfig` 当前默认值的字符串形式。
fn default_value(cfg: &PartyConfig, field: &str) -> Option<String> {
    let v = match field {
        "group_members_max" => cfg.group_members_max.to_string(),
        "try_deal_time" => cfg.try_deal_time.to_string(),
        "deal_ok_time" => cfg.deal_ok_time.to_string(),
        "can_not_get_back_deal" => cfg.can_not_get_back_deal.to_string(),
        "disable_deal" => cfg.disable_deal.to_string(),
        "lock_deal_action" => cfg.lock_deal_action.to_string(),
        _ => return None,
    };
    Some(v)
}

#[test]
fn defaults_match_oracle_vector() {
    let cfg = PartyConfig::default();
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
                "向量第 {} 行字段 `{field}` 不存在于 PartyConfig",
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
    assert_eq!(checked, 6, "向量行数应为 6（全部组队/交易配置项）");
}
