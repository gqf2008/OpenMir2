//! **Lane B**：oracle `RecalcAbilitys()` 尾段的 `DelItemSkill`/`AddItemSkill` 链
//! （`SM_DELMAGIC(212)` / `SM_ADDMAGIC(210)`）——纯函数移植，不做 I/O。
//!
//! 逐行来源（`src/M2Server/Player/PlayObject.cs`）：
//!
//! - 尾段调用序（`:2764-2779`）：
//!   ```text
//!   if (FlameRing) AddItemSkill(AM_FIREBALL); else DelItemSkill(AM_FIREBALL);
//!   if (RecoveryRing) AddItemSkill(AM_HEALING); else DelItemSkill(AM_HEALING);
//!   ```
//! - `DelItemSkill(nIndex)`（`:5103-5124`）：`Race != Play ⇒ return`（本函数只在玩家路径用，
//!   恒 `Play`）；`nIndex==1 && Job != Wizard ⇒ DeleteNameSkill(Config.FireBallSkill)`；
//!   `nIndex==2 && Job != Taoist ⇒ DeleteNameSkill(Config.HealSkill)`。
//! - `DeleteNameSkill`（`:5089-5102`）：按 `userMagic.Magic.MagicName == sSkillName`
//!   （C# `==`：**序数、区分大小写**）线性找**第一条**，`SendDelMagic` 后 `RemoveAt(i)` 并 `break`
//!   ⇒ **一次只删一条**。
//! - `AddItemSkill`（`:3907-3935`）：`WorldEngine.FindMagic(name)`（**`OrdinalIgnoreCase`**、
//!   线性首匹配）；`!IsTrainingSkill(magic.MagicId)`（`PlayObject.Attack.cs:340-350`，按
//!   `MagIdx` 扫 `MagicList`）⇒ 追加 `Level=1, Key=0, TranPoint=0` 并发 210。
//!
//! **旗标语义由调用方落账**（oracle 原样）：
//! - `FlameRing` 是**粘滞锁存**：`RecalcAbilitys` 顶部只重置 `RecoveryRing`（`:2017`），
//!   全仓唯一写入点是 `:2245` 的 `FlameRing = true` ⇒ 本函数收到的 `flame_ring` 应是
//!   `历史 || 本次命中`。戴上后即使耐久归零/脱下，本会话内也走 **Add 分支**（不是 Del）。
//! - `RecoveryRing` **每轮重置**（`:2017`）⇒ 本函数收到的值就是**本次命中**；脱下/打碎后
//!   下一次重算立即删治愈术并发 212。
//!
//! 比较口径**不能共用一个比较器**（oracle 两处不同）：Del 走 [`MagicTable::get`] 取模板名后
//! 用 Rust `==`（字节序数、区分大小写）；Add 走 [`MagicTable::find_by_name`]（忽略大小写）。

use game_svr::magics::MagicTable;
use game_svr::smframe::MagicEvent;
use mir2_data::MagicInfo;

use crate::record::MagicRcd;

/// `MessageSettings.AM_FIREBALL`（`src/Modules/SystemModule/MessageSettings.cs:12`）。
pub const AM_FIREBALL: u8 = 1;
/// `MessageSettings.AM_HEALING`（同文件 `:13`）。
pub const AM_HEALING: u8 = 2;

/// `PlayerJob.Wizard`（`src/OpenMir2/Enums/PlayerJob.cs`）。
pub const JOB_WIZARD: u8 = 1;
/// `PlayerJob.Taoist`。
pub const JOB_TAOIST: u8 = 2;

/// `CharacterData.Magic` 的固定槽数（`PlayerDataInfo.cs:118` 的 `new MagicRcd[20]`）。
pub const MAGIC_RECORD_SLOTS: usize = 20;

/// `WorldServer.MakeSaveRcd`（`:1332-1355`）的存档数组形态：`CharacterData.Magic` 恒为 20 槽，
/// 前 N 槽写 `MagicList`，其余槽填 `HUtil32.DetailtMagicRcd`（非 null 的默认 `MagicRcd`）。
/// 这里把紧凑的 `MagicList` 补齐到 20 槽（超过 20 时保持原样——oracle 在 >20 时会越界，
/// 本仓不复制该崩溃；真线记录最多 20 槽）。
#[must_use]
pub fn pad_for_save(mut list: Vec<Option<MagicRcd>>) -> Vec<Option<MagicRcd>> {
    while list.len() < MAGIC_RECORD_SLOTS {
        list.push(Some(MagicRcd {
            mag_idx: 0,
            level: 0,
            magic_key: 0,
            tran_point: 0,
        }));
    }
    list
}

/// `WorldServer.GetHumData`（`src/GameSrv/Word/WorldServer.cs:1198-1213`）的 `MagicList` 装载过滤：
/// 非空槽且 `FindMagic(MagIdx) != null` 才进 `MagicList`（DB 回包的空槽 `MagIdx=0` 与模板缺失
/// 项都**不进**）。同时这是 211 帧「空槽导致整帧跳过」的修复点。
#[must_use]
pub fn filter_for_logon(
    magics: Vec<Option<MagicRcd>>,
    table: &MagicTable,
) -> Vec<Option<MagicRcd>> {
    magics
        .into_iter()
        .flatten()
        .filter(|m| m.mag_idx != 0 && table.get(m.mag_idx).is_some())
        .map(Some)
        .collect()
}

/// `PlayObject.IsTrainingSkill(int nIndex)`（`PlayObject.Attack.cs:340-350`）：
/// `MagicList` 里存在 `MagIdx == nIndex` 的条目（`null` 项跳过）。
#[must_use]
pub fn is_training_skill(magics: &[Option<MagicRcd>], magic_id: u16) -> bool {
    magics.iter().flatten().any(|m| m.mag_idx == magic_id)
}

/// `PlayObject.AddItemSkill(int nIndex)`（`PlayObject.cs:3907-3935`）。
///
/// 返回新追加的模板（用于发 210）；名字找不到 / 已在列表 ⇒ `None`（无帧、无改动）。
pub fn add_item_skill(
    n_index: u8,
    magics: &mut Vec<Option<MagicRcd>>,
    table: &MagicTable,
    fire_ball_skill: &str,
    heal_skill: &str,
) -> Option<MagicInfo> {
    let name = match n_index {
        AM_FIREBALL => fire_ball_skill,
        AM_HEALING => heal_skill,
        _ => return None,
    };
    // `WorldEngine.FindMagic(string)`：忽略大小写、线性首匹配（`WorldServer.cs:1559-1571`）。
    let magic = table.find_by_name(name)?;
    if is_training_skill(magics, magic.magic_id) {
        return None;
    }
    magics.push(Some(MagicRcd {
        mag_idx: magic.magic_id,
        level: 1,
        magic_key: 0,
        tran_point: 0,
    }));
    Some(magic.clone())
}

/// `PlayObject.DeleteNameSkill(string sSkillName)`（`PlayObject.cs:5089-5102`）：
/// 按 `MagicName == sSkillName`（**序数、区分大小写**）找第一条，删掉并返回其 `MagIdx`。
pub fn delete_name_skill(
    magics: &mut Vec<Option<MagicRcd>>,
    table: &MagicTable,
    skill_name: &str,
) -> Option<u16> {
    for i in 0..magics.len() {
        let Some(m) = magics[i].as_ref() else {
            continue;
        };
        let Some(def) = table.get(m.mag_idx) else {
            continue;
        };
        // C# `string ==` ⇒ 序数比较、区分大小写（Rust `str` 的 `==` 同为字节/码点相等）。
        if def.magic_name == skill_name {
            let idx = m.mag_idx;
            magics.remove(i);
            return Some(idx);
        }
    }
    None
}

/// `PlayObject.DelItemSkill(int nIndex)`（`PlayObject.cs:5103-5124`）。
///
/// `Race != Play ⇒ return`：本函数只服务玩家会话（`PlayObject` 的 `Race` 恒 `Play`，
/// `PlayObject.cs:872`），故不重复该门。返回被删条目的 `MagIdx`（发 212）。
pub fn del_item_skill(
    job: u8,
    n_index: u8,
    magics: &mut Vec<Option<MagicRcd>>,
    table: &MagicTable,
    fire_ball_skill: &str,
    heal_skill: &str,
) -> Option<u16> {
    match n_index {
        AM_FIREBALL => {
            if job != JOB_WIZARD {
                delete_name_skill(magics, table, fire_ball_skill)
            } else {
                None
            }
        }
        AM_HEALING => {
            if job != JOB_TAOIST {
                delete_name_skill(magics, table, heal_skill)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// `RecalcAbilitys()` 尾段（`PlayObject.cs:2764-2779`）：先火球分支、再治愈分支。
///
/// `flame_ring` 必须是**落账后的粘滞值**（`历史 || 本次命中`）；`recovery_ring` 是
/// **本次命中**。事件序 = oracle 调用序，供 `EnterWorldInputs::magic_journal` / 会话内直发。
pub fn audit(
    job: u8,
    magics: &mut Vec<Option<MagicRcd>>,
    table: &MagicTable,
    fire_ball_skill: &str,
    heal_skill: &str,
    flame_ring: bool,
    recovery_ring: bool,
) -> Vec<MagicEvent> {
    let mut events = Vec::new();
    if flame_ring {
        if let Some(def) = add_item_skill(AM_FIREBALL, magics, table, fire_ball_skill, heal_skill) {
            events.push(MagicEvent::Add { def });
        }
    } else if let Some(mag_idx) =
        del_item_skill(job, AM_FIREBALL, magics, table, fire_ball_skill, heal_skill)
    {
        events.push(MagicEvent::Del { mag_idx });
    }
    if recovery_ring {
        if let Some(def) = add_item_skill(AM_HEALING, magics, table, fire_ball_skill, heal_skill) {
            events.push(MagicEvent::Add { def });
        }
    } else if let Some(mag_idx) =
        del_item_skill(job, AM_HEALING, magics, table, fire_ball_skill, heal_skill)
    {
        events.push(MagicEvent::Del { mag_idx });
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    fn magic(id: u16, name: &str) -> MagicInfo {
        MagicInfo {
            magic_id: id,
            magic_name: name.to_string(),
            ..MagicInfo::default()
        }
    }

    fn rec(id: u16) -> Option<MagicRcd> {
        Some(MagicRcd {
            mag_idx: id,
            level: 0,
            magic_key: 0,
            tran_point: 0,
        })
    }

    fn added(id: u16) -> Option<MagicRcd> {
        Some(MagicRcd {
            mag_idx: id,
            level: 1,
            magic_key: 0,
            tran_point: 0,
        })
    }

    fn table() -> MagicTable {
        MagicTable::new(vec![
            magic(1, "火球术"),
            magic(2, "治愈术"),
            magic(3, "雷电术"),
        ])
    }

    const FIRE: &str = "火球术";
    const HEAL: &str = "治愈术";

    /// **A1/A2（评审员更正）**：非本职业才删——法师保留火球、道士保留治愈；
    /// 战士/法师带治愈才删，道士带火球才删。
    #[test]
    fn job_gate_matches_oracle_correction_a2() {
        let t = table();
        // 战士 + 火球 ⇒ Del(1)。
        let mut m = vec![rec(1)];
        assert_eq!(
            audit(0, &mut m, &t, FIRE, HEAL, false, false),
            vec![MagicEvent::Del { mag_idx: 1 }]
        );
        assert!(m.is_empty());
        // 法师 + 火球 ⇒ 不删。
        let mut m = vec![rec(1)];
        assert!(audit(1, &mut m, &t, FIRE, HEAL, false, false).is_empty());
        assert_eq!(m.len(), 1);
        // 道士 + 治愈 ⇒ 不删（A2 更正后的反向）。
        let mut m = vec![rec(2)];
        assert!(audit(2, &mut m, &t, FIRE, HEAL, false, false).is_empty());
        // 战士/法师 + 治愈 ⇒ Del(2)。
        for job in [0, 1] {
            let mut m = vec![rec(2)];
            assert_eq!(
                audit(job, &mut m, &t, FIRE, HEAL, false, false),
                vec![MagicEvent::Del { mag_idx: 2 }],
                "job={job} 非道士 ⇒ 删治愈术"
            );
        }
    }

    /// **A4 + 牙 2**：`break` 语义——两条火球一次只删**第一条**；牙 = 删全部 ⇒ 本用例红。
    #[test]
    fn delete_removes_only_first_match() {
        let t = table();
        let mut m = vec![rec(1), rec(1)];
        let ev = audit(0, &mut m, &t, FIRE, HEAL, false, false);
        assert_eq!(ev, vec![MagicEvent::Del { mag_idx: 1 }]);
        assert_eq!(m.len(), 1, "只删一条（DeleteNameSkill 的 break）");
        // 下一次重算才删第二条（与 oracle 一致）。
        assert_eq!(
            audit(0, &mut m, &t, FIRE, HEAL, false, false),
            vec![MagicEvent::Del { mag_idx: 1 }]
        );
        assert!(m.is_empty());
    }

    /// **A5 + 牙 4**：Del 侧**区分大小写**（C# `==`），Add 侧**忽略大小写**（OrdinalIgnoreCase）。
    /// 两侧口径对调 ⇒ 本用例红。
    #[test]
    fn del_is_case_sensitive_but_add_is_case_insensitive() {
        // 模板名 `FireBall`、配置名 `fireball`：Del 不命中。
        let t_ascii = MagicTable::new(vec![magic(1, "FireBall")]);
        let mut m = vec![rec(1)];
        assert!(audit(0, &mut m, &t_ascii, "fireball", HEAL, false, false).is_empty());
        assert_eq!(m.len(), 1, "Del 区分大小写 ⇒ 不删");
        // 同名（精确）⇒ Del 命中。
        assert_eq!(
            audit(0, &mut m, &t_ascii, "FireBall", HEAL, false, false),
            vec![MagicEvent::Del { mag_idx: 1 }]
        );
        // Add 侧：列表为空 + 配置 `fireball` ⇒ 命中 `FireBall` 模板并追加。
        let mut m = Vec::new();
        let ev = audit(0, &mut m, &t_ascii, "fireball", HEAL, true, false);
        assert_eq!(ev.len(), 1);
        match &ev[0] {
            MagicEvent::Add { def } => assert_eq!(def.magic_id, 1),
            other => panic!("应为 Add，实得 {other:?}"),
        }
        assert_eq!(m.len(), 1);
        // 中文名字：配置改成形近字「火球術」⇒ Del 不命中（C# 序号比较）。
        let t = table();
        let mut m = vec![rec(1)];
        assert!(audit(0, &mut m, &t, "火球術", HEAL, false, false).is_empty());
        assert_eq!(m.len(), 1);
    }

    /// **B1/B3 + 牙 6**：火球/治愈戒指的 Add 事件携带模板（210 的 `Def`），新条目
    /// `Level=1/Key=0/TranPoint=0`；不校验职业（wizard 也能加治愈）。
    #[test]
    fn ring_adds_skill_with_oracle_defaults_and_no_job_gate() {
        let t = table();
        let mut m = Vec::new();
        let ev = audit(0, &mut m, &t, FIRE, HEAL, true, true);
        assert_eq!(ev.len(), 2, "先火球后治愈两条");
        match (&ev[0], &ev[1]) {
            (MagicEvent::Add { def: a }, MagicEvent::Add { def: b }) => {
                assert_eq!(a.magic_id, 1);
                assert_eq!(b.magic_id, 2);
            }
            other => panic!("事件序/类型不符：{other:?}"),
        }
        assert_eq!(
            m,
            vec![added(1), added(2)],
            "新条目 Level=1/Key=0/TranPoint=0"
        );
        let added = m[0].as_ref().unwrap();
        assert_eq!((added.level, added.magic_key, added.tran_point), (1, 0, 0));

        // wizard + 治愈戒指 ⇒ Add（AddItemSkill 不查 Job）。
        let mut m = Vec::new();
        assert_eq!(audit(1, &mut m, &t, FIRE, HEAL, false, true).len(), 1);
        assert_eq!(m[0].as_ref().unwrap().mag_idx, 2);
    }

    /// **B2 + 牙（IsTrainingSkill）**：列表已有同 `MagIdx` ⇒ 不重复追加、不发 210；
    /// 牙 = 去掉 `is_training_skill` 门 ⇒ 本条必红。
    #[test]
    fn training_skill_blocks_duplicate_add() {
        let t = table();
        let mut m = vec![Some(MagicRcd {
            mag_idx: 1,
            level: 3,
            magic_key: 7,
            tran_point: 55,
        })];
        let ev = audit(0, &mut m, &t, FIRE, HEAL, true, false);
        assert!(ev.is_empty(), "已有火球 ⇒ 不发 210");
        assert_eq!(m.len(), 1);
        let kept = m[0].as_ref().unwrap();
        assert_eq!(
            (kept.level, kept.magic_key, kept.tran_point),
            (3, 7, 55),
            "不覆盖等级"
        );
    }

    /// **B4/B5/B6**：粘滞 `FlameRing` 在列表已有火球时即便无戒指也**不删**（走 Add 分支被
    /// `IsTrainingSkill` 挡住）；`RecoveryRing` 每轮重置 ⇒ 下一轮恢复 false 立即删并发 212。
    #[test]
    fn sticky_flame_skips_delete_but_recovery_resets_each_round() {
        let t = table();
        let mut m = vec![rec(1), rec(2)];
        // 第 1 轮：戴过 115/116（历史 latch 已真）但本轮无戒指 ⇒ flame 仍真（Add 分支，无删），
        // recovery 本轮 false ⇒ 删治愈。
        let ev = audit(0, &mut m, &t, FIRE, HEAL, true, false);
        assert_eq!(ev, vec![MagicEvent::Del { mag_idx: 2 }]);
        assert_eq!(m, vec![rec(1)], "火球仍在（粘滞），治愈被删");
        // 第 2 轮仍无戒指 ⇒ flame 仍真：火球永远不删。
        assert!(audit(0, &mut m, &t, FIRE, HEAL, true, false).is_empty());
        assert_eq!(m, vec![rec(1)]);
        // 下次登录 = 新对象（flame=false）⇒ 火球才被删并发 212。
        assert_eq!(
            audit(0, &mut m, &t, FIRE, HEAL, false, false),
            vec![MagicEvent::Del { mag_idx: 1 }]
        );
        assert!(m.is_empty());
        // RecoveryRing 每轮重置：本轮真 ⇒ Add 治愈（AddItemSkill 不查 Job）。
        // 用战士观察下一轮：道士的 `DelItemSkill(2)` 有 `Job != Taoist` 门（治愈保留）。
        let mut m = Vec::new();
        assert_eq!(audit(0, &mut m, &t, FIRE, HEAL, false, true).len(), 1);
        // 下一轮 false ⇒ 立即 Del。
        assert_eq!(
            audit(0, &mut m, &t, FIRE, HEAL, false, false),
            vec![MagicEvent::Del { mag_idx: 2 }]
        );
    }

    /// **V1 / 211 空槽修复**：装载过滤丢弃空槽（`MagIdx=0`）与模板缺失项，保序保留有效项。
    #[test]
    fn logon_filter_drops_empty_and_missing_slots() {
        let t = table();
        let raw = vec![
            Some(MagicRcd {
                mag_idx: 0,
                level: 0,
                magic_key: 0,
                tran_point: 0,
            }),
            rec(3),
            rec(9999),
            None,
            rec(1),
        ];
        assert_eq!(filter_for_logon(raw, &t), vec![rec(3), rec(1)]);
    }

    /// **存档形态**：`MakeSaveRcd` 的 `CharacterData.Magic` 恒 20 槽（前 N = MagicList，
    /// 其余 = 默认 `MagicRcd`）。
    #[test]
    fn save_padding_keeps_twenty_slots() {
        let padded = pad_for_save(vec![rec(1)]);
        assert_eq!(padded.len(), MAGIC_RECORD_SLOTS);
        assert_eq!(padded[0], rec(1));
        assert!(padded[1..].iter().all(|m| {
            m.as_ref()
                .is_some_and(|m| (m.mag_idx, m.level, m.magic_key, m.tran_point) == (0, 0, 0, 0))
        }));
    }

    /// **A6**：模板缺失的条目在 Del/Add 都**不命中**（oracle 在 `GetHumData` 装载时已丢弃，
    /// 这里防御性验证不 panic、不误删）。
    #[test]
    fn missing_template_is_ignored() {
        let t = table();
        let mut m = vec![rec(9999)];
        assert!(audit(0, &mut m, &t, FIRE, HEAL, false, false).is_empty());
        assert_eq!(m, vec![rec(9999)]);
    }
}
