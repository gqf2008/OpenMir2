//! `openmir2-m3-attack-magic-wiring`（第二片：魔法派发）——`CM_SPELL(3017)` 与
//! `CM_MAGICKEYCHANGE(1008)` 的**纯函数接缝** + 会话魔法表的键切换/查表（不含 I/O / 世界状态）。
//!
//! 移植源（逐条照 oracle 行号，不按常识重推）：
//!
//! - **入帧映射**：
//!   - `CM_SPELL`（`src/GameSrv/Word/WorldServer.cs:783-790`）：
//!     `SendUpdateMsg(Ident, Tag, LoWord(Recog), HiWord(Recog), MakeLong(Param, Series), "")`
//!     （`SendUpdateMsg` 与 `SendMsg` 的形参序同，见 `BaseObject.Message.cs:283/303`）
//!     ⇒ `wParam = Tag`、`nParam1 = LoWord(Recog)`、`nParam2 = HiWord(Recog)`、
//!     `nParam3 = MakeLong(Param, Series)`。
//!   - `CM_MAGICKEYCHANGE` 走 `WorldServer` 的 `default` 支（`:857-859`）：
//!     `SendMsg(playObject, Ident, Series, Recog, Param, Tag, sMsg)`
//!     ⇒ `wParam = Series`、`nParam1 = Recog`、`nParam2 = Param`、`nParam3 = Tag`。
//! - **处理器**（`src/M2Server/Player/PlayObject.Message.cs`）：
//!   - `CM_SPELL`（`:1339-1340`）：
//!     `ClientSpellXY(wIdent, wParam, (short)nParam1, (short)nParam2, ActorMgr.Get(nParam3), LateDelivery, ref delayTime)`
//!     ⇒ `nKey = wParam`（**是魔法号 MagicId，不是按键**：`GetMagicInfo(int)` 比的是
//!     `userMagic.Magic.MagicId`，`PlayObject.cs:4174-4185`）、目标坐标 = `(nParam1, nParam2)`、
//!     `nParam3` = 目标 ActorId。
//!   - `CM_MAGICKEYCHANGE`（`:871-873`）：`ClientChangeMagicKey((ushort)nParam1, (char)nParam2)`。
//! - **守卫**（`PlayObject.Attack.cs:1054-1068`）：`ClientSpellXY` 依次
//!   `if (!IsCanSpell) return false;` → `if (Death || StatusTimeArr[PoisonState.STONE] != 0) return false;`
//!   → `if (GetMagicInfo(nKey) == null) return false;`。
//! - **成功支**（`PlayObject.Message.cs:1339-1400` 外层）：`ClientSpellXY(...)` 为真 ⇒
//!   `ActionTick = GetTickCount(); SendSocket(M2Share.GetGoodTick)`（`+GD` ack）；为假且
//!   `delayTime == 0` ⇒ `SendRefMsg(RM_MOVEFAIL, 0,0,0,0, "")`。
//! - **`ClientChangeMagicKey`**（`PlayObject.Operate.cs:774-786`）：遍历 `MagicList`，
//!   命中 `userMagic.Magic.MagicId == nSkillIdx` 即置 `userMagic.Key = nKey` 并 `break`；
//!   **无命中则什么都不做、无回帧**。
//!
//! # 本片未接（登记，别按常识补）
//!
//! **`ClientSpellXY` 的技能效果体**（`PlayObject.Attack.cs:1071-1275`）整个 `switch (UserMagic.MagIdx)` 未接：
//! 限速（`CheckActionStatus` + `MagicAttackTick`/`MagicAttackInterval`/`MagicAttackCount`/`OverSpeedCount`/`BoEmergencyClose`）、
//! `SpellTick -= 450`、战士技切换
//! （`ThrustingOnOff`/`HalfMoonOnOff`/`RedHalfMoonOnOff`/`SkillCrsOnOff`/`Skill43OnOff` 与
//! `SendSocket("+LNG"/"+ULNG"/"+WID"/"+UWID"/"+FIR"/"+CRS"/"+UCRS"/"+TWN"/"+CID"/"+UCID")`），
//! 以及 `default` 支的 `Dir = GetNextDirection`（`:1269-1270`）+ `CretInNearXy` + `DoSpell`（`MagicManager.cs:145-…`）。
//! 其中 `switch` 的 8 条独立 `case`（`:1120/1136/1152/1168/1187/1218/1234/1253`，魔号见
//! [`EXPLICIT_CASE_MAG_IDX`]）**不走 `default`** ⇒ Rust 侧只对这些之外的魔号套用 `Dir`。
//! `DoSpell` 开头的 `IsWarrSkill`/`MagicAttackRage` 门与
//! `SendRefMsg(RM_SPELL, Effect, x, y, MagicId, "")`（`MagicManager.cs:151-159`）**已接**
//! （片 `openmir2-default-arm-spell-surface`，2026-10-17：宿主 `handle_spell_cmd` 在这三条门之后
//! **无条件**广播一帧 `SM_SPELL(17)` —— 任何非战士魔号一律恰一条，与是否有 `case`/是否 `>80` 无关；
//! 逐行依据见 `main.rs` 该片的块注释）⇒ 原「本片不产出 `SM_SPELL(17)`」一句作废。
//! **已接（2026-10-17，default 支的 `PlayObject.DoSpell` 包装层）**：`GetSpellPoint`/
//! `DamageSpell`/`HealthSpellChanged`（`PlayObject.Base.cs:1505-1531`）与蓝不足早退的
//! `RM_MAGICFIREFAIL` ⇒ `SM_MAGICFIRE_FAIL(639)`（宿主 `main.rs::handle_spell_cmd`）。
//! **仍未接**：8 条显式 `case` 各自的 MP 逻辑（12/25/26/27/34/38/43/56），以及
//! `MagicManager.DoSpell` 的战士门/距离门失败回帧（`RM_MAGICFIREFAIL`）。
//! 技能效果面的向量与用例在 `openmir2-m3-combat-skills-align`（m3cs）的离线片，其接口就绪后再接这一层。
//!
//! **`LateDelivery`**：Rust 的入帧路径没有延时重投（`command` 直投）⇒ 恒按 `false` 处理；oracle 此时
//! `!boLateDelivery` 限速支**应生效**，本片按**直通**（等价 `boLateDelivery`）并登记（与
//! `attackwire`/`movewire` 同源登记）。
//!
//! **`IsCanSpell`**：oracle 默认 `true`（`PlayObject.cs:960` 初值；`Config.LockSpellAction` 与
//! `ClientUseItems` 的置位面未移植）⇒ 本片恒 `true`（登记）。
//!
//! **`STONE`**（`StatusTimeArr[PoisonState.STONE]`）：状态位面未移植 ⇒ `Death` 已接、`STONE` 未接
//! （与 `movewire`/`attackwire` 同源登记）。
//!
//! 因为依赖 bin 私有的 [`crate::record::MagicRcd`]（`MagicList` 的记录态），本模块留在 bin 里
//! （lib 里的 `movewire`/`attackwire` 只依赖 `mir2_protocol`/`mir2_world`）。

use game_svr::feature;
use mir2_protocol::messages;

use crate::record::MagicRcd;

/// `CM_SPELL`（`src/OpenMir2/Messages.cs:139`）的接缝判定。
#[must_use]
pub const fn is_spell_ident(ident: u16) -> bool {
    ident == messages::CM_SPELL
}

/// `CM_MAGICKEYCHANGE`（`src/OpenMir2/Messages.cs:58`）的接缝判定。
#[must_use]
pub const fn is_magic_key_ident(ident: u16) -> bool {
    ident == messages::CM_MAGICKEYCHANGE
}

/// `switch (UserMagic.MagIdx)`（`PlayObject.Attack.cs:1119`）里**有独立 `case` 的 8 个魔号**：
/// `SKILL_ERGUM(12)` / `SKILL_BANWOL(25)` / `SKILL_FIRESWORD(26)` / `SKILL_MOOTEBO(27)` /
/// `SKILL_CROSSMOON(34)` / `SKILL_TWINBLADE(38)` / `SKILL_43(43)` / `SKILL_REDBANWOL(56)`
/// （`src/Modules/SystemModule/Const/MagicConst.cs:52-197`；`case` 行号 `:1120/1136/1152/1168/1187/1218/1234/1253`）。
///
/// 这 8 支**不执行** `default:` 首句 `Dir = M2Share.GetNextDirection(...)`（`:1269-1270`）——效果体
/// 整段未接（归 m3cs），本表只用于对齐“走哪条分支”：不在表内的魔号才套用 `default` 首句。
/// （reviewer-5 `1de8fca5` 的 **F-2** 收口。）
pub const EXPLICIT_CASE_MAG_IDX: [i32; 8] = [12, 25, 26, 27, 34, 38, 43, 56];

/// 该 `mag_idx` 是否走 `switch (UserMagic.MagIdx)` 的 `default` 支（⇒ 才执行 `Dir = GetNextDirection`）。
#[must_use]
pub fn takes_default_dir(mag_idx: i32) -> bool {
    !EXPLICIT_CASE_MAG_IDX.contains(&mag_idx)
}

/// `CM_SPELL` 经 `WorldServer.cs:783-790` 映射后的 `ClientSpellXY` 入参（除 `wIdent`/`LateDelivery`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpellRequest {
    /// `ClientSpellXY` 的 `nKey`：**魔法号**（`wParam = Tag`）。
    pub magic_id: u16,
    /// `nTargetX = (short)LoWord(Recog)`。
    pub target_x: i32,
    /// `nTargetY = (short)HiWord(Recog)`。
    pub target_y: i32,
    /// `ActorMgr.Get(nParam3)` 的入参：`nParam3 = MakeLong(Param, Series)`。
    pub target_actor_id: i32,
}

/// `CM_SPELL` 的入帧映射（`WorldServer.cs:783-790` + `BaseObject.Message.cs:283/303`）。
///
/// 坐标走 `HUtil32.LoWord/HiWord` 后按 `(short)` 解释（C# 处 `(short)processMsg.nParam1`），
/// 与 `movewire`/`attackwire` 的同段口径一致。
#[must_use]
pub fn parse_spell(recog: i32, tag: u16, param: u16, series: u16) -> SpellRequest {
    SpellRequest {
        magic_id: tag,
        target_x: i32::from(feature::lo_word(recog) as i16),
        target_y: i32::from(feature::hi_word(recog) as i16),
        target_actor_id: feature::make_long(param, series),
    }
}

/// `CM_MAGICKEYCHANGE` 的入帧映射（`WorldServer.cs:857-859` 的 `default` 支）：
/// `nParam1 = Recog`、`nParam2 = Param`；处理器再 `(ushort)nParam1`、`(char)nParam2`
/// （`PlayObject.Message.cs:871-873`）⇒ `(魔法号, 按键)`。
#[must_use]
pub fn parse_magic_key(recog: i32, param: u16) -> (u16, u16) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let skill_idx = recog as u16; // C# `(ushort)nParam1`。
    (skill_idx, param) // `(char)nParam2` = 低 16 位；`param` 已是 `u16`。
}

/// oracle `PlayObject.GetMagicInfo(int nMagicId)`（`PlayObject.cs:4174-4185`）：
/// 线性找 `userMagic.Magic.MagicId == nMagicId` 的第一项。玩家魔法表里 `MagIdx` 即 `MagicId`
/// （`PlayObject.cs:3923-3931` 的 `AddItemSkill` / 记录 `MagicRcd.MagIdx`）。
#[must_use]
pub fn find_magic(magics: &[Option<MagicRcd>], magic_id: u16) -> Option<&MagicRcd> {
    magics.iter().flatten().find(|m| m.mag_idx == magic_id)
}

/// oracle `ClientChangeMagicKey(ushort nSkillIdx, char nKey)`（`PlayObject.Operate.cs:774-786`）：
/// 命中 `mag_idx == nSkillIdx` 即置 `magic_key = nKey` 并停；返回是否命中。
///
/// 无命中时不动任何槽（oracle 的循环自然结束）⇒ 调用方只记日志、**不发帧**。
pub fn change_magic_key(magics: &mut [Option<MagicRcd>], skill_idx: u16, key: u16) -> bool {
    for slot in magics.iter_mut().flatten() {
        if slot.mag_idx == skill_idx {
            slot.magic_key = key;
            return true;
        }
    }
    false
}

/// `ClientSpellXY` 开头三个守卫的拒绝原因（`PlayObject.Attack.cs:1057-1068`，顺序即 oracle）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpellReject {
    /// `!IsCanSpell`。
    NotCanSpell,
    /// `Death || StatusTimeArr[PoisonState.STONE] != 0`（`STONE` 未接，只有 `Death` 会命中）。
    DeathOrStone,
    /// `GetMagicInfo(nKey) == null`（玩家没有该魔法号）。
    NoMagic,
}

/// `ClientSpellXY` 的守卫（`PlayObject.Attack.cs:1057-1068`）：`None` = 全部通过（后续恒 `return true`）。
///
/// 只在守卫都过之后才进入效果体；本片效果体未接（模块头登记），故 `None` 即代表
/// 「按 oracle 的 `SendSocket(GetGoodTick)` 回 `+GD`」。
#[must_use]
pub fn spell_reject(is_can_spell: bool, death: bool, has_magic: bool) -> Option<SpellReject> {
    if !is_can_spell {
        return Some(SpellReject::NotCanSpell);
    }
    if death {
        return Some(SpellReject::DeathOrStone);
    }
    if !has_magic {
        return Some(SpellReject::NoMagic);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn magic(mag_idx: u16, key: u16) -> Option<MagicRcd> {
        Some(MagicRcd {
            mag_idx,
            level: 1,
            magic_key: key,
            tran_point: 0,
        })
    }

    /// `ClientChangeMagicKey` 只改命中那一项，且保序。
    #[test]
    fn change_magic_key_sets_only_matching_id() {
        let mut list = vec![magic(1, u16::from(b'm')), None, magic(9, u16::from(b'q'))];
        assert!(change_magic_key(&mut list, 9, u16::from(b'f')));
        assert_eq!(list[0].as_ref().unwrap().magic_key, u16::from(b'm'));
        assert_eq!(list[2].as_ref().unwrap().magic_key, u16::from(b'f'));
    }

    /// 未命中 ⇒ 不改动任何槽、返回 false（oracle 的 `break` 不触发）。
    #[test]
    fn change_magic_key_ignores_when_absent() {
        let mut list = vec![magic(1, u16::from(b'm'))];
        assert!(!change_magic_key(&mut list, 42, u16::from(b'z')));
        assert_eq!(list[0].as_ref().unwrap().magic_key, u16::from(b'm'));
    }

    /// `GetMagicInfo` 比的是 `mag_idx`（MagicId），不是按键；`None` 槽跳过。
    #[test]
    fn find_magic_matches_magic_id_and_skips_none() {
        let list = vec![None, magic(7, u16::from(b'a'))];
        assert_eq!(
            find_magic(&list, 7).map(|m| m.magic_key),
            Some(u16::from(b'a'))
        );
        assert!(find_magic(&list, 8).is_none());
    }

    /// `CM_SPELL` 入帧映射：坐标取自 `Recog` 的低/高字、魔法号取自 `Tag`、ActorId 取自
    /// `MakeLong(Param, Series)`（照 `WorldServer.cs:783-790`）。
    #[test]
    fn parse_spell_maps_world_server_fields() {
        let req = parse_spell(feature::make_long(0x0134, 0x026A), 0x0011, 0x00A1, 0x0000);
        assert_eq!(req.magic_id, 0x0011, "wParam(Tag) = 魔法号");
        assert_eq!((req.target_x, req.target_y), (0x0134, 0x026A));
        assert_eq!(req.target_actor_id, feature::make_long(0x00A1, 0x0000));
    }

    /// `CM_MAGICKEYCHANGE` 入帧映射：魔法号 = `(ushort)Recog`、按键 = `(char)Param`。
    #[test]
    fn parse_magic_key_truncates_recog_to_u16() {
        assert_eq!(parse_magic_key(1, u16::from(b'f')), (1, u16::from(b'f')));
        assert_eq!(
            parse_magic_key(0x0001_0001, 0x0046),
            (0x0001, 0x0046),
            "Recog 按 (ushort) 截断"
        );
    }

    /// 守卫顺序与结果照 oracle：`!IsCanSpell` → `Death/STONE` → `GetMagicInfo==null`。
    #[test]
    fn spell_reject_order_and_results() {
        assert_eq!(
            spell_reject(false, true, false),
            Some(SpellReject::NotCanSpell)
        );
        assert_eq!(
            spell_reject(true, true, false),
            Some(SpellReject::DeathOrStone)
        );
        assert_eq!(spell_reject(true, false, false), Some(SpellReject::NoMagic));
        assert_eq!(spell_reject(true, false, true), None);
    }

    /// **F-2 收口**：`switch (UserMagic.MagIdx)` 的 8 个独立 `case` 不套用 `default` 首句 `Dir`。
    /// 断言**精确集合**（改/漏一个魔号 ⇒ 必红）。
    #[test]
    fn default_dir_only_for_non_explicit_case_magics() {
        assert_eq!(EXPLICIT_CASE_MAG_IDX, [12, 25, 26, 27, 34, 38, 43, 56]);
        for m in EXPLICIT_CASE_MAG_IDX {
            assert!(
                !takes_default_dir(m),
                "mag_idx={m} 有独立 case ⇒ 不套用 default 的 Dir"
            );
        }
        assert!(
            takes_default_dir(1),
            "SKILL_FIREBALL(1) 无独立 case ⇒ 走 default"
        );
        assert!(takes_default_dir(999));
    }
}
