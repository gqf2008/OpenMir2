//! **IT-12 祝福油**（StdMode 3 / Shape 4）：`WeaptonMakeLuck()` 的纯决策。
//!
//! oracle：`PlayObject.Base.cs:2259`（`EatUseItems` 的 `case 4`）→ `PlayObject.cs:1500`
//! （原文拼写就是 `Weapton`）。
//!
//! # 语义（逐行照抄 `PlayObject.cs:1500-1555`）
//!
//! 1. 无武器（`UseItems[Weapon] == null || Index <= 0`）⇒ 函数返回 **false** ⇒ **不吃**；
//!    否则**恒返回 true**（中没中、诅咒与否都吃）。
//! 2. 先抽 `Random(WeaponMakeUnLuckRate)`（`:1512`）；`== 1` ⇒ **诅咒支**（`MakeWeaponUnlock`），
//!    **不再抽**：`Desc[3] > 0` ⇒ `Desc[3] -= 1`，否则 `Desc[4] < 10` ⇒ `Desc[4] += 1`。
//! 3. 否则按 `Desc[4]`/`Desc[3]` 的 else-if 链（`&&` 才抽，短路）：
//!    - `Desc[4] > 0` ⇒ `Desc[4] -= 1`（**不抽**）；
//!    - `Desc[3] < point1` ⇒ `Desc[3] += 1`（**不抽**）；
//!    - `Desc[3] < point2` 且 `Random(nRand + point2_rate) == 1` ⇒ `Desc[3] += 1`；
//!    - `Desc[3] < point3` 且 `Random(nRand * point3_rate) == 1` ⇒ `Desc[3] += 1`
//!      （**上一条抽了但没中时，这一条会再抽一次**）。
//! 4. `nRand = |HiByte(StdItem.DC) - LoByte(StdItem.DC)| / 5`（`:1509-1511`，整数除法）。
//!
//! **`Random(0)` 是合法输入**：.NET `Next(0)` 返回 0、**消耗一次抽样**；Rust
//! [`mir2_shared::rng::RandomNumber`] 同口径（`next_max(0)` 走 `sample()` 后乘 0）。
//! `nRand == 0` 时「点3」抽恒为 `Random(0) = 0` ⇒ 永不命中（但**仍抽**）。
//!
//! 状态面（帧上不可见，必须由宿主回写）：武器 `Desc[3]`（幸运）/ `Desc[4]`（诅咒计数，上限 10）。

use mir2_shared::hutil32::{hi_byte_u16, lo_byte_u16};
use mir2_shared::rng::RandomSource;

use crate::config::WeaponMakeLuckCfg;

/// `nRand = |HiByte(dc) - LoByte(dc)| / 5`（`PlayObject.cs:1509-1511`，整数除法）。
#[must_use]
pub fn luck_rand_from_dc(dc: u16) -> i32 {
    let hi = i32::from(hi_byte_u16(dc));
    let lo = i32::from(lo_byte_u16(dc));
    (hi - lo).abs() / 5
}

/// `WeaptonMakeLuck` 的三条出口。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LuckOutcome {
    /// 首抽 `== 1` ⇒ `MakeWeaponUnlock`：**红字** `TheWeaponIsCursed`。
    Cursed,
    /// 幸运/消诅咒成功 ⇒ **绿字** `WeaptonMakeLuck`。
    Made,
    /// 没中 ⇒ **绿字** `WeaptonNotMakeLuck`。
    NotMade,
}

/// 决策结果：`desc3`/`desc4` 为**回写值**（调用方写进 `UseItems[Weapon].Desc`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LuckResult {
    pub outcome: LuckOutcome,
    pub desc3: u8,
    pub desc4: u8,
}

/// `PlayObject.WeaptonMakeLuck()` 的纯决策半（RNG + `Desc` 迁移），**不含帧与 Recalc**。
///
/// 调用方必须先保证「有武器且 `Index > 0`」（oracle 的前置；否则根本不进本函数）。
#[must_use]
pub fn weapton_make_luck(
    rng: &mut impl RandomSource,
    cfg: &WeaponMakeLuckCfg,
    n_rand: i32,
    desc3: u8,
    desc4: u8,
) -> LuckResult {
    if rng.random_below(cfg.un_luck_rate) == 1 {
        // 诅咒支（`:1512-1515` → `MakeWeaponUnlock`，`PlayObject.cs:1601-1631`）。
        let (d3, d4) = if desc3 > 0 {
            (desc3.wrapping_sub(1), desc4)
        } else if desc4 < 10 {
            (desc3, desc4.wrapping_add(1))
        } else {
            (desc3, desc4)
        };
        return LuckResult {
            outcome: LuckOutcome::Cursed,
            desc3: d3,
            desc4: d4,
        };
    }
    let mut d3 = desc3;
    let mut d4 = desc4;
    let made = if d4 > 0 {
        // `:1519-1524`：先消诅咒（不抽）。
        d4 = d4.wrapping_sub(1);
        true
    } else if i32::from(d3) < cfg.point1 {
        // `:1525-1530`：点1 直接加（不抽）。
        d3 = d3.wrapping_add(1);
        true
    } else if i32::from(d3) < cfg.point2 && rng.random_below(n_rand + cfg.point2_rate) == 1 {
        // `:1531-1536`：点2 抽。
        d3 = d3.wrapping_add(1);
        true
    } else if i32::from(d3) < cfg.point3 && rng.random_below(n_rand * cfg.point3_rate) == 1 {
        // `:1537-1542`：点3 抽（`nRand * point3_rate`；可能为 0 ⇒ `Random(0) = 0` 恒不中）。
        d3 = d3.wrapping_add(1);
        true
    } else {
        false
    };
    LuckResult {
        outcome: if made {
            LuckOutcome::Made
        } else {
            LuckOutcome::NotMade
        },
        desc3: d3,
        desc4: d4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_shared::rng::RandomNumber;

    /// 脚本化随机源：按序返回给定值，并计数取数次数。
    struct Scripted {
        values: Vec<i32>,
        i: usize,
        draws: usize,
    }
    impl Scripted {
        fn new(values: &[i32]) -> Self {
            Self {
                values: values.to_vec(),
                i: 0,
                draws: 0,
            }
        }
    }
    impl RandomSource for Scripted {
        fn random(&mut self) -> i32 {
            self.random_below(1)
        }
        fn random_below(&mut self, _value: i32) -> i32 {
            self.draws += 1;
            let v = self.values[self.i];
            self.i += 1;
            v
        }
        fn random_range(&mut self, _min: i32, _max: i32) -> i32 {
            self.random_below(1)
        }
        fn next_double(&mut self) -> f64 {
            0.0
        }
    }

    fn cfg() -> WeaponMakeLuckCfg {
        WeaponMakeLuckCfg::default()
    }

    #[test]
    fn n_rand_is_abs_hi_lo_over_five() {
        assert_eq!(luck_rand_from_dc(0x0A05), 1); // |10-5|/5
        assert_eq!(luck_rand_from_dc(0x1400), 4); // |20-0|/5
        assert_eq!(luck_rand_from_dc(0), 0);
        assert_eq!(luck_rand_from_dc(0x0B09), 0); // |11-9|/5 = 0（整数除法）
    }

    #[test]
    fn curse_branch_subtracts_luck_first() {
        let mut r = Scripted::new(&[1]);
        let res = weapton_make_luck(&mut r, &cfg(), 2, 3, 0);
        assert_eq!(
            res,
            LuckResult {
                outcome: LuckOutcome::Cursed,
                desc3: 2,
                desc4: 0
            }
        );
        assert_eq!(r.draws, 1, "诅咒支只抽一次");
    }

    #[test]
    fn curse_branch_at_luck_zero_raises_curse_count_capped_at_ten() {
        let mut r = Scripted::new(&[1]);
        assert_eq!(
            weapton_make_luck(&mut r, &cfg(), 2, 0, 4),
            LuckResult {
                outcome: LuckOutcome::Cursed,
                desc3: 0,
                desc4: 5
            }
        );
        // 上界：`Desc[4] < 10` 才加；已 10 ⇒ 不变、**仍是诅咒支**（不是 `Desc[4]--` 支）。
        let mut r = Scripted::new(&[1]);
        assert_eq!(
            weapton_make_luck(&mut r, &cfg(), 2, 0, 10),
            LuckResult {
                outcome: LuckOutcome::Cursed,
                desc3: 0,
                desc4: 10
            }
        );
        assert_eq!(r.draws, 1);
    }

    #[test]
    fn removes_curse_without_drawing() {
        let mut r = Scripted::new(&[0]);
        assert_eq!(
            weapton_make_luck(&mut r, &cfg(), 2, 0, 3),
            LuckResult {
                outcome: LuckOutcome::Made,
                desc3: 0,
                desc4: 2
            }
        );
        assert_eq!(r.draws, 1, "只有首抽");
    }

    #[test]
    fn point1_adds_luck_without_drawing() {
        let mut r = Scripted::new(&[0]);
        assert_eq!(
            weapton_make_luck(&mut r, &cfg(), 2, 0, 0),
            LuckResult {
                outcome: LuckOutcome::Made,
                desc3: 1,
                desc4: 0
            }
        );
        assert_eq!(r.draws, 1);
    }

    #[test]
    fn point2_hit_draws_twice() {
        // desc3=1（>=point1, <point2）⇒ 点2 抽；命中。
        let mut r = Scripted::new(&[0, 1]);
        assert_eq!(
            weapton_make_luck(&mut r, &cfg(), 2, 1, 0),
            LuckResult {
                outcome: LuckOutcome::Made,
                desc3: 2,
                desc4: 0
            }
        );
        assert_eq!(r.draws, 2);
    }

    #[test]
    fn point2_miss_then_point3_draws_three_times() {
        // 点2 抽不中 ⇒ 落到点3 再抽一次，命中。
        let mut r = Scripted::new(&[0, 0, 1]);
        assert_eq!(
            weapton_make_luck(&mut r, &cfg(), 2, 1, 0),
            LuckResult {
                outcome: LuckOutcome::Made,
                desc3: 2,
                desc4: 0
            }
        );
        assert_eq!(r.draws, 3, "两次条件抽");
    }

    #[test]
    fn both_tiers_miss_is_not_made() {
        let mut r = Scripted::new(&[0, 0, 0]);
        assert_eq!(
            weapton_make_luck(&mut r, &cfg(), 2, 1, 0),
            LuckResult {
                outcome: LuckOutcome::NotMade,
                desc3: 1,
                desc4: 0
            }
        );
        assert_eq!(r.draws, 3);
    }

    #[test]
    fn desc3_at_point3_skips_both_tiers_single_draw() {
        let mut r = Scripted::new(&[0]);
        assert_eq!(
            weapton_make_luck(&mut r, &cfg(), 2, 7, 0),
            LuckResult {
                outcome: LuckOutcome::NotMade,
                desc3: 7,
                desc4: 0
            }
        );
        assert_eq!(r.draws, 1, "点2/点3 的条件都不成立 ⇒ 不抽");
    }

    #[test]
    fn n_rand_zero_point3_is_dead_but_still_draws() {
        // desc3=1（>=point1、<point2）⇒ 点2 抽不中后**仍会**落到点3 再抽一次；
        // nRand=0 ⇒ 点3 的 `Random(0)` 恒 0（!=1）⇒ 不中。本例共 3 抽（含首抽）。
        let mut r = Scripted::new(&[0, 0, 0]);
        assert_eq!(
            weapton_make_luck(&mut r, &cfg(), 0, 1, 0),
            LuckResult {
                outcome: LuckOutcome::NotMade,
                desc3: 1,
                desc4: 0
            }
        );
        assert_eq!(r.draws, 3);
    }

    fn oil_fixture() -> serde_json::Value {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/parity/m3/fixtures/oil_vectors.json"
        );
        let raw = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("oil 夹具缺失/不可读：{path}（{e}）"));
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{path} 不是合法 JSON：{e}"))
    }

    fn hex_to_bytes(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex"))
            .collect()
    }

    /// **IT-12 祝福油：纯决策与 oracle 逐例对拍**（`Desc[3]/[4]` 迁移 + 抽数）。
    ///
    /// 夹具由 `m3/oil-probe` 用**真身** `PlayObject.ClientUseItems` 产出；抽值由探针把
    /// `ScriptedRandom` 装进 `OpenMir2.RandomNumber.random` 脚本化 ⇒ 每例的 `script`
    /// 就是 oracle 实际消耗的抽数序列。Rust 用同一序列喂 [`weapton_make_luck`]，
    /// **Desc 迁移与消耗条数都必须相同**。
    #[test]
    fn it12_oil_fixture_matches_pure_decision() {
        let doc = oil_fixture();
        let cases = doc["cases"].as_array().expect("cases 必须是数组");
        assert_eq!(cases.len(), 12, "用例数固定 12（改动须同步本断言）");
        let cfg = WeaponMakeLuckCfg::default();
        let mut checked = 0usize;
        let mut saw_curse_at_ten = false;
        for c in cases {
            let name = c["name"].as_str().unwrap();
            let wb = &c["weapon_before"];
            let eaten = c["frames"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["ident"].as_u64() == Some(635));
            if wb.is_null() || !eaten {
                continue; // 无武器 / 禁药：不吃，不抽；由下一条用例断言
            }
            let d3b = wb["desc"][3].as_u64().unwrap() as u8;
            let d4b = wb["desc"][4].as_u64().unwrap() as u8;
            let wa = &c["weapon_after"];
            let d3a = wa["desc"][3].as_u64().unwrap() as u8;
            let d4a = wa["desc"][4].as_u64().unwrap() as u8;
            let script: Vec<i32> = c["script"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_i64().unwrap() as i32)
                .collect();
            let weapon_index = c["ctx"]["weapon_index"].as_i64().unwrap_or(0);
            // 武器模板 DC：2301 = 0x140A ⇒ nRand=2；2302 = 0 ⇒ nRand=0。
            let n_rand = if weapon_index == 2302 { 0 } else { 2 };
            let mut r = Scripted::new(&script);
            let res = weapton_make_luck(&mut r, &cfg, n_rand, d3b, d4b);
            assert_eq!(
                (res.desc3, res.desc4),
                (d3a, d4a),
                "{name}: Desc 迁移必须与 oracle 相同"
            );
            assert_eq!(
                r.draws,
                script.len(),
                "{name}: 消耗抽数必须与 oracle 相同（script={script:?}）"
            );
            // **SysMsg 文本**（卡 `openmir2-runtime-sysmsg-text-frames`）：oracle 记录
            // `EDCode.EncodeString(Buff)` 的原始字节 hex；Rust 按 outcome 取同一条文案再编码，逐字节相同。
            let sysmsgs: Vec<&serde_json::Value> = c["notices"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|n| n["ident"].as_i64() == Some(11004))
                .collect();
            // oracle `MakeWeaponUnlock`（`PlayObject.cs:1607-1621`）：`Desc[3]==0 && Desc[4]>=10` 时
            // **静默**（不发红字）；其余分支各一条。期望条数按此规则算，不是写死 1。
            let want_sysmsg = match res.outcome {
                LuckOutcome::Cursed => d3b > 0 || d4b < 10,
                LuckOutcome::Made | LuckOutcome::NotMade => true,
            };
            assert_eq!(
                sysmsgs.len(),
                usize::from(want_sysmsg),
                "{name}: SysMsg 条数（outcome={:?}）",
                res.outcome
            );
            if want_sysmsg {
                let got = hex_to_bytes(sysmsgs[0]["text"].as_str().unwrap_or(""));
                let texts = crate::useitem::SysMsgTexts::default();
                let want_text = match res.outcome {
                    LuckOutcome::Cursed => texts.the_weapon_is_cursed,
                    LuckOutcome::Made => texts.weapton_make_luck,
                    LuckOutcome::NotMade => texts.weapton_not_make_luck,
                };
                let want = mir2_protocol::edcode::encode(&crate::downlink::gbk(want_text));
                assert_eq!(
                    got, want,
                    "{name}: SysMsg 文本字节必须与 oracle 相同（outcome={:?}）",
                    res.outcome
                );
            }
            if name == "oil_curse_desc4_at_ten_stays" {
                saw_curse_at_ten = true;
                assert_eq!(
                    res.outcome,
                    LuckOutcome::Cursed,
                    "Desc[4]=10 必须走诅咒支（不是 Desc[4]-- 支）"
                );
                assert_eq!((res.desc3, res.desc4), (0, 10), "上界不越界");
            }
            checked += 1;
        }
        assert!(checked >= 10, "至少 10 例被核对（实得 {checked}）");
        assert!(saw_curse_at_ten, "缺 Desc[4]=10 诅咒支用例");
    }

    /// **IT-12 失败支（`use_item` 纯路径）**：无武器 ⇒ 不吃；禁药 ⇒ 不吃且红字通知。
    /// 同时核 oracle 夹具里这两例的线序（636、零能力帧、无武器零通知 / 禁药恰一条红字）。
    #[test]
    fn it12_oil_fail_cases_eat_and_frames() {
        use crate::ability::StdItemCatalog;
        use crate::useitem::{use_item, UseItemActorState, UseItemContext, UseItemOutcome};

        let doc = oil_fixture();
        for c in doc["cases"].as_array().unwrap() {
            let name = c["name"].as_str().unwrap();
            if name != "oil_fail_no_weapon" && name != "oil_fail_nodrug" {
                continue;
            }
            let ids: Vec<u64> = c["frames"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| f["ident"].as_u64().unwrap())
                .collect();
            assert_eq!(ids, vec![636], "{name}: 不吃 ⇒ 只发 SM_EAT_FAIL(636)");
            assert!(
                c["pumped"].as_array().unwrap().is_empty(),
                "{name}: 不吃 ⇒ 无能力帧"
            );
            if name == "oil_fail_no_weapon" {
                assert!(
                    c["notices"].as_array().unwrap().is_empty(),
                    "无武器：零通知"
                );
            } else {
                let notices: Vec<u64> = c["notices"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|n| n["ident"].as_u64().unwrap())
                    .collect();
                assert_eq!(notices, vec![11004], "禁药：恰一条红字 RM_SYSMESSAGE");
            }

            // Rust 纯路径：同一失败条件下的 `use_item` 必须 `eaten == false`。
            let mut catalog = StdItemCatalog::new();
            catalog.insert(
                2300,
                mir2_data::StdItem {
                    name: "bless_oil".into(),
                    std_mode: 3,
                    shape: 4,
                    ..Default::default()
                },
            );
            let mut state = UseItemActorState {
                wabil: Default::default(),
                inc_health: 0,
                inc_spell: 0,
                user_unlock_durg: false,
                hunger_status: 0,
                extra: crate::useitem::ItemsExtraState::default(), // weapon = None
            };
            let mut list = vec![crate::bag::BagItem {
                make_index: 1,
                index: 2300,
                dura: 0,
                dura_max: 0,
                desc: [0; 14],
            }];
            let ctx = UseItemContext {
                catalog: &catalog,
                can_use_item: true,
                dead: false,
                actor_id: 1,
                make_index: 1,
                no_drug: name == "oil_fail_nodrug",
                function_npc_present: false,
                unbind: &crate::unbindlist::UnbindList::default(),
                red_msg_fcolor: 255,
                red_msg_bcolor: 56,
                green_msg_fcolor: 219,
                green_msg_bcolor: 255,
                blue_msg_fcolor: 251,
                blue_msg_bcolor: 252,
                repair_item_dec_dura: 90,
                now_ms: 0,
                sysmsg: crate::useitem::SysMsgTexts::default(),
                recalc: None,
            };
            let item_number = std::sync::atomic::AtomicI32::new(0);
            let out: UseItemOutcome = use_item(&ctx, &mut state, &mut list, &item_number);
            assert!(!out.eaten, "{name}: 同条件不得吃");
        }
    }

    /// **`Random(0)` 跨语言口径**（opc-19 点名）：.NET 实测（PowerShell `System.Random`）为
    /// seed=1 ⇒ `Next(0)=0, Next(100)=11, 46, Next(30000)=23148`；seed=20261120 ⇒
    /// `0, 63, 32, 11637`。Rust 必须**返回值与后续序列都相同**（含「消耗一次抽样」）。
    /// 若这里红 ⇒ 是**真分歧**，不许在 Rust 侧加 `if bound == 0` 抹平。
    #[test]
    fn random_zero_matches_dotnet_value_and_stream() {
        let mut r = RandomNumber::with_seed(1);
        assert_eq!(r.random_below(0), 0);
        assert_eq!(r.random_below(100), 11);
        assert_eq!(r.random_below(100), 46);
        assert_eq!(r.random_below(30000), 23148);
        let mut r = RandomNumber::with_seed(20261120);
        assert_eq!(r.random_below(0), 0);
        assert_eq!(r.random_below(100), 63);
        assert_eq!(r.random_below(100), 32);
        assert_eq!(r.random_below(30000), 11637);
    }
}
