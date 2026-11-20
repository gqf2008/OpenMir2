//! **IT-17b 结构层**：`PlayObject.RecalcHitSpeed` 的**技能被动** + `MagicArr`。
//!
//! # oracle 锚（`src/M2Server/Player/PlayObject.cs`）
//!
//! `RecalcHitSpeed`（`:3659-3713`，由 `BaseObject.Abil.cs:54` 在**装备循环之前**调用）：
//! 1. 按 `Job` 写 `HitPoint/SpeedPoint` 的**职业底**（本仓在 [`crate::ability::hit_speed_base`] 已建模，
//!    含道士不重置 `HitPoint` 的怪癖）；
//! 2. 遍历 `MagicList`：**填 `MagicArr[MagIdx]`**（`:3684`；`new UserMagic[50]`，`:1041`）并按技能加：
//!    - 基本剑法 `SKILL_ONESWORD(3)`：`Level>0 ⇒ HitPoint += Round(9/3*L)`（`:3687-3692`）
//!    - 精神力战法 `SKILL_ILKWANG(4)`：`HitPoint += Round(8/3*L)`（`:3693-3698`）
//!    - 攻杀剑法 `SKILL_YEDO(7)`：`HitPoint += Round(3/3*L)=L`；`HitPlus = DEFHIT(5)+L`；
//!      `AttackSkillCount = 7-L`；**`AttackSkillPointCount = RandomByte(7-L)`（每次 Recalc 抽 1 次）**
//!      （`:3699-3707`）
//!    - 烈火剑法 `SKILL_FIRESWORD(26)`：`HitDouble = 4 + 4*L`（`:3708-3710`）
//!
//! # 本批（结构层）的范围与边界
//!
//! - **无 RNG**：攻杀剑法的 `RandomByte(7-L)` 抽数**不在本批**（含攻杀的角色留到后续批，届时
//!   本函数要加 `rng` 形参并把这一抽记进账本）。本批只要 `MagicList` 不含攻杀 ⇒ 0 抽是**结构性**的。
//! - **`MagicArr` 三条硬事实照抄**：① `MagIdx >= 50` 在 oracle 会抛 `IndexOutOfRangeException`
//!   （数组 50）——本仓**跳过并 WRN**（不把内容错误升级成进程崩溃，登记）；② **从不清空**
//!   （删技能后留残影；oracle 只有 `:3684` 一个写点）；③ 下游 `GetAttackMagic`/`GetHitMode`
//!   （模式 4/5/8/12）/`SkillAttackDamage` 读它——消费接线在后续批。
//! - `SpeedPoint` 不受技能被动影响（只有职业底 + `AddAbil.SPEED`）。

use crate::record::MagicRcd;

/// `MagicConst.SKILL_ONESWORD`（基本剑法，`MagicConst.cs:16`）。
pub const SKILL_ONESWORD: u16 = 3;
/// `MagicConst.SKILL_ILKWANG`（精神力战法，`MagicConst.cs:20`）。
pub const SKILL_ILKWANG: u16 = 4;
/// `MagicConst.SKILL_YEDO`（攻杀剑法，`MagicConst.cs:32`）。
pub const SKILL_YEDO: u16 = 7;
/// `MagicConst.SKILL_FIRESWORD`（烈火剑法，`MagicConst.cs:108`）。
pub const SKILL_FIRESWORD: u16 = 26;
/// `MagicConst.SKILL_ERGUM`（刺杀剑术，`MagicConst.cs:52`）。
pub const SKILL_ERGUM: u16 = 12;
/// `MagicConst.SKILL_BANWOL`（半月弯刀，`MagicConst.cs:104`）。
pub const SKILL_BANWOL: u16 = 25;
/// `MagicConst.SKILL_CROSSMOON`（逐日剑法，`MagicConst.cs:159`）。
pub const SKILL_CROSSMOON: u16 = 34;
/// `MagicArr = new UserMagic[50]`（`PlayObject.cs:1041`）。
pub const MAGIC_ARR_LEN: usize = 50;
/// `MessageSettings.DEFHIT`（`MessageSettings.cs:10`；`HitPlus = DEFHIT + Level`）。
pub const DEF_HIT: u8 = 5;

/// `RecalcHitSpeed` 的技能被动输出（`PlayObject` 上的 `HitPoint/HitPlus/HitDouble/AttackSkill*` 增量）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SkillPassives {
    /// 加到 `HitPoint` 上的技能增量（职业底与 `AddAbil.HIT` 由 [`crate::ability`] 叠加）。
    pub hit_point: u8,
    /// `PlayObject.HitPlus`（`RecalcHitSpeed` 顶部清零；攻杀支置 `DEFHIT+L`）。
    pub hit_plus: u8,
    /// `PlayObject.HitDouble`（顶部清零；烈火支置 `4+4L`）。
    pub hit_double: u8,
    /// `PlayObject.AttackSkillCount`（攻杀支置 `7-L`）。
    pub attack_skill_count: u8,
}

/// `HUtil32.Round`（`(int)Math.Round(r, AwayFromZero)`）：Rust `f64::round` 同为半数远离零。
fn round_away(r: f64) -> i32 {
    r.round() as i32
}

/// `PlayObject.RecalcHitSpeed` 的**技能循环**（`:3681-3712`）：填 `MagicArr`（**不清空**）并返回技能被动。
///
/// `arr` 长度必须是 [`MAGIC_ARR_LEN`]；`mag_idx >= 50` 的条目**跳过**（oracle 越界崩溃；本仓登记不崩）。
pub fn recalc_hit_speed_skills(
    magics: &[Option<MagicRcd>],
    arr: &mut [Option<MagicRcd>],
) -> SkillPassives {
    let mut out = SkillPassives::default();
    for m in magics.iter().flatten() {
        match arr.get_mut(usize::from(m.mag_idx)) {
            // `MagicArr[userMagic.MagIdx] = userMagic;`（`PlayObject.cs:3684`）——**只写不清**。
            Some(slot) => *slot = Some(m.clone()),
            None => crate::glog!(
                "WRN",
                "RecalcHitSpeed：MagIdx={} >= {MAGIC_ARR_LEN}（oracle MagicArr 越界；本仓跳过并登记）",
                m.mag_idx
            ),
        }
        let level = i32::from(m.level);
        match m.mag_idx {
            SKILL_ONESWORD => {
                if level > 0 {
                    out.hit_point = out
                        .hit_point
                        .wrapping_add(round_away(9.0 / 3.0 * f64::from(level)) as u8);
                }
            }
            SKILL_ILKWANG => {
                if level > 0 {
                    out.hit_point = out
                        .hit_point
                        .wrapping_add(round_away(8.0 / 3.0 * f64::from(level)) as u8);
                }
            }
            SKILL_YEDO => {
                if level > 0 {
                    out.hit_point = out.hit_point.wrapping_add(level as u8);
                    out.hit_plus = DEF_HIT.saturating_add(level as u8);
                    out.attack_skill_count = 7u8.wrapping_sub(level as u8);
                }
            }
            SKILL_FIRESWORD => {
                out.hit_double = 4u8.wrapping_add((level * 4) as u8);
            }
            _ => {}
        }
    }
    out
}

/// `GetHitMode` 的**技能态**（`PlayObject.Attack.cs:78-120`）：本批只接 `MagicArr` 的
/// `ergum/banwol/crossmoon` 三个条件（`arr[id].is_some()`）；`PowerHit/FireHitSkill`（攻杀/烈火的
/// **战斗态**）**不在本批**，恒 `false`（另计，见卡 §6）。
#[must_use]
pub fn hit_skill_state(arr: &[Option<MagicRcd>]) -> game_svr::attackwire::HitSkillState {
    game_svr::attackwire::HitSkillState {
        power_hit: false,
        fire_hit_skill: false,
        ergum: arr
            .get(usize::from(SKILL_ERGUM))
            .is_some_and(Option::is_some),
        banwol: arr
            .get(usize::from(SKILL_BANWOL))
            .is_some_and(Option::is_some),
        crossmoon: arr
            .get(usize::from(SKILL_CROSSMOON))
            .is_some_and(Option::is_some),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(id: u16, level: u8) -> Option<MagicRcd> {
        Some(MagicRcd {
            mag_idx: id,
            level,
            magic_key: 0,
            tran_point: 0,
        })
    }

    fn arr() -> Vec<Option<MagicRcd>> {
        vec![None; MAGIC_ARR_LEN]
    }

    /// 四条技能公式逐条（`Round` 用 `AwayFromZero`）：基本剑法 3L、精神力 Round(8/3L)、
    /// 攻杀 L + HitPlus/HitCount、烈火 4+4L。红检：把 9/3 改 8/3 或把 `Round` 改截断 ⇒ 必红。
    #[test]
    fn skill_formulas_match_oracle() {
        let mut a = arr();
        let p = recalc_hit_speed_skills(
            &[
                m(SKILL_ONESWORD, 3),
                m(SKILL_ILKWANG, 3),
                m(SKILL_YEDO, 2),
                m(SKILL_FIRESWORD, 2),
            ],
            &mut a,
        );
        // 3*3=9（基本）+ Round(8/3*3)=8（精神）+ 2（攻杀）= 19。
        assert_eq!(p.hit_point, 19, "HitPoint = 9 + 8 + 2");
        assert_eq!(p.hit_plus, DEF_HIT + 2, "HitPlus = DEFHIT + L");
        assert_eq!(p.attack_skill_count, 5, "AttackSkillCount = 7 - L");
        assert_eq!(p.hit_double, 4 + 2 * 4, "HitDouble = 4 + 4L");
        // 魔法 id 全部进 MagicArr。
        for id in [SKILL_ONESWORD, SKILL_ILKWANG, SKILL_YEDO, SKILL_FIRESWORD] {
            assert_eq!(a[usize::from(id)].as_ref().map(|x| x.mag_idx), Some(id));
        }
    }

    /// Level==0 的技能**不加** HitPoint（基本/精神/攻杀三支的 `if (Level > 0)` 门）；
    /// 烈火支**没有** Level 门（`HitDouble = 4 + 0`）。
    #[test]
    fn level_zero_skills_add_nothing_but_fire_sword() {
        let mut a = arr();
        let p = recalc_hit_speed_skills(
            &[
                m(SKILL_ONESWORD, 0),
                m(SKILL_ILKWANG, 0),
                m(SKILL_YEDO, 0),
                m(SKILL_FIRESWORD, 0),
            ],
            &mut a,
        );
        assert_eq!(p.hit_point, 0);
        assert_eq!(p.hit_plus, 0);
        assert_eq!(p.attack_skill_count, 0);
        assert_eq!(p.hit_double, 4, "烈火支无 Level 门 ⇒ 4+0");
    }

    /// **`MagicArr` 从不清空**：第二次以**空** MagicList 重算后，旧条目仍在（oracle 只有 `:3684` 一个写点）。
    ///
    /// 红检：在函数开头 `arr.fill(None)` ⇒ 本用例必红。
    #[test]
    fn magic_arr_is_never_cleared() {
        let mut a = arr();
        let _ = recalc_hit_speed_skills(&[m(SKILL_ONESWORD, 1)], &mut a);
        assert!(a[usize::from(SKILL_ONESWORD)].is_some());
        let p = recalc_hit_speed_skills(&[], &mut a);
        assert_eq!(p, SkillPassives::default(), "空表不加被动");
        assert!(
            a[usize::from(SKILL_ONESWORD)].is_some(),
            "删技能后 MagicArr 仍非空（残影，oracle 原样）"
        );
    }

    /// `hit_skill_state` 只认 `MagicArr` 三个条件（12/25/34），`PowerHit/FireHitSkill` 恒 false。
    #[test]
    fn hit_skill_state_reads_magic_arr_only() {
        let mut a = arr();
        a[usize::from(SKILL_ERGUM)] = m(SKILL_ERGUM, 1);
        let st = hit_skill_state(&a);
        assert!(st.ergum && !st.banwol && !st.crossmoon);
        assert!(!st.power_hit && !st.fire_hit_skill, "战斗态不在本批");
        let empty = arr();
        let st = hit_skill_state(&empty);
        assert!(!st.ergum && !st.banwol && !st.crossmoon);
    }

    /// **跨 Recalc 的顺序例**：两次重算用**不同**技能 ⇒ `arr` 里应**同时**留两次的条目
    /// （「只写不清」的直接后果，不是第二次覆盖第一次）。红检：在函数开头 `arr.fill(None)` 或
    /// 每次只写当前项（先清再写）⇒ 本用例必红。
    #[test]
    fn magic_arr_keeps_entries_across_recalcs() {
        let mut a = arr();
        let _ = recalc_hit_speed_skills(&[m(SKILL_ONESWORD, 1)], &mut a);
        let _ = recalc_hit_speed_skills(&[m(SKILL_ILKWANG, 1)], &mut a);
        assert!(
            a[usize::from(SKILL_ONESWORD)].is_some(),
            "第一次的条目必须在（不清空）"
        );
        assert!(a[usize::from(SKILL_ILKWANG)].is_some(), "第二次的条目也在");
    }

    /// **`MagIdx >= 50` 不崩、不写**（oracle 越界抛异常；本仓跳过并 WRN——登记的原样差异）。
    #[test]
    fn magic_arr_ignores_id_out_of_range() {
        let mut a = arr();
        let p = recalc_hit_speed_skills(&[m(50, 1), m(SKILL_ONESWORD, 1)], &mut a);
        assert_eq!(p.hit_point, 3, "越界条目不参与；合法条目照算");
        assert!(a.iter().all(|x| x.as_ref().map(|v| v.mag_idx) != Some(50)));
        assert!(a[usize::from(SKILL_ONESWORD)].is_some());
    }

    /// **会话级接线**（IT-17b）：`recalc_session_ability` 把技能被动带进 `752` 的
    /// `Tag = MakeWord(HitPoint, SpeedPoint)`，并填 `MagicArr`；删技能后 `MagicArr` 仍有残影。
    ///
    /// 红检：`recalc_session_ability` 不传 `skill`（回 `with_prior`）⇒ 第一步 `sub.hit_point` 断言红；
    /// 在 `recalc_hit_speed_skills` 开头清空 `arr` ⇒ 第四步断言红。
    #[test]
    fn session_recalc_feeds_subability_and_magic_arr() {
        use mir2_protocol::frame::CommandMessage;
        let mut sg = crate::tests::test_session_ground(1);
        sg.equip.job = 0;
        let catalog = game_svr::ability::StdItemCatalog::new();
        let _ = crate::recalc_session_ability(&mut sg, &catalog, None, None);
        let h0 = sg.sub.hit_point;
        let s0 = sg.sub.speed_point;
        // 学基本剑法 L3 ⇒ HitPoint +9（`PlayObject.cs:3687-3692`）。
        sg.magic.list = vec![Some(MagicRcd {
            mag_idx: SKILL_ONESWORD,
            level: 3,
            magic_key: 0,
            tran_point: 0,
        })];
        let _ = crate::recalc_session_ability(&mut sg, &catalog, None, None);
        assert_eq!(sg.magic.passives.hit_point, 9, "技能被动 3L=9");
        assert_eq!(
            sg.sub.hit_point,
            h0.wrapping_add(9),
            "752 HitPoint 必须含技能被动"
        );
        assert_eq!(sg.sub.speed_point, s0, "技能不改 SpeedPoint");
        assert!(
            sg.magic.arr[usize::from(SKILL_ONESWORD)].is_some(),
            "MagicArr 填了基本剑法"
        );
        // 752 帧：`Tag = MakeWord(HitPoint, SpeedPoint)`（`PlayObject.Message.cs:1675`）。
        let body = game_svr::smframe::sm_subability(
            sg.sub.anti_magic,
            sg.sub.hit_point,
            sg.sub.speed_point,
            sg.sub.anti_poison,
            (sg.sub.poison_recover & 0xFF) as u8,
            (sg.sub.health_recover & 0xFF) as u8,
            (sg.sub.spell_recover & 0xFF) as u8,
        );
        let m = CommandMessage::from_bytes(&body).expect(">=12B");
        // oracle 调用 `MakeMessage(SM_SUBABILITY, MakeLong(AntiMagic,0), MakeWord(HitPoint,SpeedPoint),
        // MakeWord(AntiPoison,PoisonRecover))` ⇒ 线上 **`Param`** = HitPoint(lo)/SpeedPoint(hi)
        // （oracle 源码里该实参位置上写作 `nParam`；`0` 是 `Recog` 槽）。`Tag` 是 AntiPoison/PoisonRecover。
        assert_eq!(
            m.param,
            u16::from(sg.sub.hit_point) | (u16::from(sg.sub.speed_point) << 8),
            "752 Param 的低/高字节 = HitPoint/SpeedPoint"
        );
        assert_eq!(
            m.tag,
            u16::from(sg.sub.anti_poison) | ((sg.sub.poison_recover & 0xFF) << 8),
            "752 Tag = AntiPoison/PoisonRecover"
        );
        // 删技能（MagicList 清空）后再重算：被动归零，但 MagicArr 留残影（oracle 从不清空）。
        sg.magic.list.clear();
        let _ = crate::recalc_session_ability(&mut sg, &catalog, None, None);
        assert_eq!(sg.magic.passives.hit_point, 0);
        assert_eq!(sg.sub.hit_point, h0);
        assert!(
            sg.magic.arr[usize::from(SKILL_ONESWORD)].is_some(),
            "删技能后 MagicArr 仍非空（残影）"
        );
    }

    /// Round 的半数行为（精神力 L1：8/3=2.667⇒3；L2：5.333⇒5；L3：8）——与 oracle `HUtil32.Round` 同。
    #[test]
    fn spirit_round_is_away_from_zero() {
        for (level, want) in [(1u8, 3u8), (2, 5), (3, 8)] {
            let mut a = arr();
            let p = recalc_hit_speed_skills(&[m(SKILL_ILKWANG, level)], &mut a);
            assert_eq!(p.hit_point, want, "精神力 L{level}");
        }
    }
}
