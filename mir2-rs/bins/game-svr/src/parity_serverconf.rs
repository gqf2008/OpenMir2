//! 卡 `openmir2-magic-serverconf-batch-plan`：**Rust 侧 ServerConf 共享入口**（只服务 magic 夹具）。
//!
//! 与 C# 侧 `tests/parity/csharp/MagicGolden/ServerConfFixture.cs` 的分工：
//!   * C#：从**部署 `Server.conf`** 装载（`ServerConf.LoadConfig()`），先断 sha、再断十键 == 部署值；
//!   * 本文件：**只读 golden** 的 `cases[Label==label].Fixture.server_conf`（十键），
//!     **不读 `Server.conf`**——magic 驱动不依赖外部路径，golden 仍是可移植的冻结件。
//!
//! 语义：缺键 / 非整数 / ≤0 ⇒ **具名 panic**（绝不吃缺省；**唯一例外**是
//! `mab_mabe_hit_rand_rate=0`——§27 的 `Random(0)` 边界，`Random(0)` 仍推进且恒返 0）；
//! 通过后把五项落到 `&mut World`
//! （`set_mag_turn_undead_level` + `set_b4_config`），并返回十键快照。
//!
//! **`mag_tamming_*` 四键只返回、不落 `World`**：它们由 `PetSlaveConfig`（`mir2_data`）的消费者使用，
//! `World` 不持有这份配置。**MG-34（收服批）落地时必须接到读 `PetSlaveConfig` 的那一侧**——
//! 别以为这里"丢了四键"。
//! 键名与 C# `Keys` 表、golden `server_conf` 完全一致（`mab_mabe_hit_sucess_rate` 一个 c）。

use mir2_world_bevy::World;
use serde_json::Value;

/// 十键 golden 键名（顺序固定；与 C# `ServerConfFixture.Keys` 的 `GoldenKey` 一致）。
pub const SERVER_CONF_KEYS: [&str; 10] = [
    "mag_turn_undead_level",
    "mab_mabe_hit_rand_rate",
    "mab_mabe_hit_min_lv_limit",
    "mab_mabe_hit_sucess_rate",
    "mab_mabe_hit_mabe_time_rate",
    "amy_ounsul_point",
    "mag_tamming_level",
    "mag_tamming_target_level",
    "mag_tamming_hp_rate",
    "mag_tamming_count",
];

/// 十键解析值快照（字段顺序/命名与 [`SERVER_CONF_KEYS`] 对齐）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerConfSnapshot {
    pub mag_turn_undead_level: i32,
    pub mab_mabe_hit_rand_rate: i32,
    pub mab_mabe_hit_min_lv_limit: i32,
    pub mab_mabe_hit_sucess_rate: i32,
    pub mab_mabe_hit_mabe_time_rate: i32,
    pub amy_ounsul_point: i32,
    pub mag_tamming_level: i32,
    pub mag_tamming_target_level: i32,
    pub mag_tamming_hp_rate: i32,
    pub mag_tamming_count: i32,
}

/// 从 golden 的 `Fixture.server_conf` 读十键，落到 `world`，返回快照。
///
/// 落盘范围：`World` 只有魔法六键对应的存储（`mag_turn_undead_level` 与 `MabMabeConfig`）；
/// `mag_tamming_*` 四键在 `PetSlaveConfig`（`mir2_data`），**由它的消费者使用、`World` 不持有**，
/// 因此只随快照返回——MG-34（收服批）落地时必须接到读 `PetSlaveConfig` 的那一侧。
///
/// # Panics
/// golden 缺 `Label` / `Fixture.server_conf` / 任一键，或值非整数、≤0（`mab_mabe_hit_rand_rate
/// 的 0 除外，见模块注释）、越界 `i32`。
#[must_use]
pub fn apply_serverconf_fixture(
    golden: &Value,
    world: &mut World,
    stem: &str,
    label: &str,
) -> ServerConfSnapshot {
    let case = golden
        .get("cases")
        .and_then(Value::as_array)
        .and_then(|cases| {
            cases
                .iter()
                .find(|c| c.get("Label").and_then(Value::as_str) == Some(label))
        })
        .unwrap_or_else(|| panic!("[{stem}/{label}] 金标准缺少 Label={label} 的 case"));

    let server_conf = case
        .get("Fixture")
        .and_then(|f| f.get("server_conf"))
        .unwrap_or_else(|| {
            panic!("[{stem}/{label}] Fixture 缺 `server_conf`（十键契约，不许默认兜底）")
        });

    let read = |key: &str| -> i32 {
        let v = server_conf
            .get(key)
            .unwrap_or_else(|| panic!("[{stem}/{label}] server_conf 缺键 `{key}`（不许默认兜底）"));
        let n = v
            .as_i64()
            .unwrap_or_else(|| panic!("[{stem}/{label}] server_conf.{key} 非整数：{v}"));
        // **MG-48（2026-11-26）**：`mab_mabe_hit_rand_rate=0` 是 §27 的合法边界
        // （oracle `Random(0)` 仍推进、恒返 0），不是「缺失/错误」；**只放行这一个键**，
        // 其余键（含 `mab_mabe_hit_sucess_rate`）仍严格 >0——`mab_mabe_hit_mabe_time_rate=0`
        // 会让 `nPower / timeRate` 除零，同样不许。
        if n < 0 || (n == 0 && key != "mab_mabe_hit_rand_rate") {
            panic!("[{stem}/{label}] server_conf.{key}={n} 必须 >0（配置缺失/错误不许吃缺省）");
        }
        i32::try_from(n)
            .unwrap_or_else(|_| panic!("[{stem}/{label}] server_conf.{key}={n} 越界 i32"))
    };

    let snapshot = ServerConfSnapshot {
        mag_turn_undead_level: read("mag_turn_undead_level"),
        mab_mabe_hit_rand_rate: read("mab_mabe_hit_rand_rate"),
        mab_mabe_hit_min_lv_limit: read("mab_mabe_hit_min_lv_limit"),
        mab_mabe_hit_sucess_rate: read("mab_mabe_hit_sucess_rate"),
        mab_mabe_hit_mabe_time_rate: read("mab_mabe_hit_mabe_time_rate"),
        amy_ounsul_point: read("amy_ounsul_point"),
        mag_tamming_level: read("mag_tamming_level"),
        mag_tamming_target_level: read("mag_tamming_target_level"),
        mag_tamming_hp_rate: read("mag_tamming_hp_rate"),
        mag_tamming_count: read("mag_tamming_count"),
    };

    world.set_mag_turn_undead_level(snapshot.mag_turn_undead_level);
    world.set_b4_config(mir2_world_bevy::magic_effect::MabMabeConfig {
        hit_rand_rate: snapshot.mab_mabe_hit_rand_rate,
        hit_min_lv_limit: snapshot.mab_mabe_hit_min_lv_limit,
        hit_success_rate: snapshot.mab_mabe_hit_sucess_rate,
        hit_mabe_time_rate: snapshot.mab_mabe_hit_mabe_time_rate,
        amy_ounsul_point: snapshot.amy_ounsul_point,
    });

    snapshot
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 部署口径十键（与 `serverconf_parse_parity` 的 `DEPLOY`、C# `Keys.Expected` 逐字一致）。
    fn full_server_conf() -> Value {
        json!({
            "mag_turn_undead_level": 44,
            "mab_mabe_hit_rand_rate": 100,
            "mab_mabe_hit_min_lv_limit": 10,
            "mab_mabe_hit_sucess_rate": 21,
            "mab_mabe_hit_mabe_time_rate": 20,
            "amy_ounsul_point": 15,
            "mag_tamming_level": 40,
            "mag_tamming_target_level": 1,
            "mag_tamming_hp_rate": 100,
            "mag_tamming_count": 5,
        })
    }

    fn golden_with(label: &str, server_conf: Value) -> Value {
        json!({
            "cases": [
                { "Label": label, "Fixture": { "server_conf": server_conf } }
            ]
        })
    }

    fn world() -> World {
        World::new(64, 64)
    }

    /// 牙①：全键 ⇒ 快照正确，且魔法六键已落到 `World`。
    #[test]
    fn full_keys_snapshot_matches_and_world_applied() {
        let g = golden_with("s1", full_server_conf());
        let mut w = world();
        let s = apply_serverconf_fixture(&g, &mut w, "turnundead", "s1");

        assert_eq!(s.mag_turn_undead_level, 44);
        assert_eq!(s.mab_mabe_hit_rand_rate, 100);
        assert_eq!(s.mab_mabe_hit_min_lv_limit, 10);
        assert_eq!(s.mab_mabe_hit_sucess_rate, 21);
        assert_eq!(s.mab_mabe_hit_mabe_time_rate, 20);
        assert_eq!(s.amy_ounsul_point, 15);
        assert_eq!(s.mag_tamming_level, 40);
        assert_eq!(s.mag_tamming_target_level, 1);
        assert_eq!(s.mag_tamming_hp_rate, 100);
        assert_eq!(s.mag_tamming_count, 5);

        assert_eq!(w.mag_turn_undead_level, 44, "圣言等级必须落到 World");
        assert_eq!(w.b4_cfg.hit_rand_rate, 100);
        assert_eq!(w.b4_cfg.hit_min_lv_limit, 10);
        assert_eq!(w.b4_cfg.hit_success_rate, 21);
        assert_eq!(w.b4_cfg.hit_mabe_time_rate, 20);
        assert_eq!(w.b4_cfg.amy_ounsul_point, 15);
    }

    /// 牙②：缺一键 ⇒ 具名 panic（消息含键名）。
    #[test]
    #[should_panic(expected = "server_conf 缺键 `mag_tamming_count`")]
    fn missing_key_panics_with_key_name() {
        let mut sc = full_server_conf();
        sc.as_object_mut().unwrap().remove("mag_tamming_count");
        let g = golden_with("s1", sc);
        let _ = apply_serverconf_fixture(&g, &mut world(), "turnundead", "s1");
    }

    /// 牙③：某键为 0 / 负 ⇒ panic（≤0 不许吃缺省）。
    #[test]
    #[should_panic(expected = "必须 >0")]
    fn zero_value_panics() {
        let mut sc = full_server_conf();
        sc["mag_turn_undead_level"] = json!(0);
        let g = golden_with("s1", sc);
        let _ = apply_serverconf_fixture(&g, &mut world(), "turnundead", "s1");
    }

    #[test]
    #[should_panic(expected = "必须 >0")]
    fn negative_value_panics() {
        let mut sc = full_server_conf();
        sc["amy_ounsul_point"] = json!(-5);
        let g = golden_with("s1", sc);
        let _ = apply_serverconf_fixture(&g, &mut world(), "turnundead", "s1");
    }

    /// **MG-48 牙⑤（放松面）**：`mab_mabe_hit_rand_rate=0` 必须**放行**并落到 `World`
    /// （§27 边界：`Random(0)` 仍推进、恒返 0）。
    #[test]
    fn rand_rate_zero_allowed_and_applied() {
        let mut sc = full_server_conf();
        sc["mab_mabe_hit_rand_rate"] = json!(0);
        let g = golden_with("s1", sc);
        let mut w = world();
        let s = apply_serverconf_fixture(&g, &mut w, "mabmabe-rand0", "s1");
        assert_eq!(s.mab_mabe_hit_rand_rate, 0);
        assert_eq!(w.b4_cfg.hit_rand_rate, 0);
        // 其余键仍是部署值（只放行一个键，不是整体放松）。
        assert_eq!(s.mab_mabe_hit_sucess_rate, 21);
    }

    /// **MG-48 牙⑥（放松面的负例）**：严格性只对 `mab_mabe_hit_rand_rate` 放松——
    /// ①同键 -1 仍红；②`mab_mabe_hit_sucess_rate=0` 仍红（同为 `Random(rate)` 参数但**未**放行）。
    #[test]
    #[should_panic(expected = "必须 >0")]
    fn rand_rate_negative_panics() {
        let mut sc = full_server_conf();
        sc["mab_mabe_hit_rand_rate"] = json!(-1);
        let g = golden_with("s1", sc);
        let _ = apply_serverconf_fixture(&g, &mut world(), "mabmabe-rand0", "s1");
    }

    #[test]
    #[should_panic(expected = "mab_mabe_hit_sucess_rate=0")]
    fn success_rate_zero_still_panics() {
        let mut sc = full_server_conf();
        sc["mab_mabe_hit_sucess_rate"] = json!(0);
        let g = golden_with("s1", sc);
        let _ = apply_serverconf_fixture(&g, &mut world(), "mabmabe", "s1");
    }

    /// 牙④：非整数 ⇒ panic（不能 `as_i64` 失败后当成 0/缺省）。
    #[test]
    #[should_panic(expected = "非整数")]
    fn non_integer_panics() {
        let mut sc = full_server_conf();
        sc["mab_mabe_hit_rand_rate"] = json!("abc");
        let g = golden_with("s1", sc);
        let _ = apply_serverconf_fixture(&g, &mut world(), "turnundead", "s1");
    }
}
