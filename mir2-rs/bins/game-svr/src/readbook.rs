//! **IT-17a**：`ReadBook`（StdMode 4 技能书）的宿主拦截。
//!
//! # oracle 锚（`src/M2Server/Player/PlayObject.Base.cs`）
//!
//! `ClientUseItems` 的 `case 4:`（`PlayObject.Operate.cs:632-654`）→ `ReadBook(stdItem)`
//! （`PlayObject.Base.cs:2171-2200`）：
//! 1. `SystemShare.WorldEngine.FindMagic(stdItem.Name)` —— **按物品名**查魔法表（`WorldServer.cs:1559-1571`，
//!    忽略大小写、装表序首匹配）；
//! 2. `!IsTrainingSkill(magic.MagicId)`（`PlayObject.Attack.cs:340-351`：`MagicList` 已有该 `MagIdx` ⇒ 不学）；
//! 3. `magic.Job == 99 || magic.Job == (byte)Job`；
//! 4. `Abil.Level >= magic.TrainLevel[0]`（**基础 `Abil.Level`**）。
//!
//! 全过 ⇒ `MagicList.Add(UserMagic{ MagIdx, **Key=0, Level=0, TranPoint=0** })` → `RecalcAbilitys()` →
//! `SendAddMagic`（`MakeMessage(SM_ADDMAGIC,0,0,0,1)` + `EDCode(ClientMagic{Key,Level,CurTrain,Def})`）。
//! 尾段（`Operate.cs:633-653`）：`Dispose/RemoveAt`；`MagicArr[SKILL_ERGUM(12)]!=null && !UseThrusting`
//! ⇒ `ThrustingOnOff(true)`（`UseThrusting=true` + `SysMsg(ThrustingOn, Green)`）+ `SendSocket("+LNG")`；
//! `SKILL_BANWOL(25)` 同型 `+WID`；`WeightChanged()`（RM_WEIGHTCHANGED）→ `SM_EAT_OK(635)`。
//!
//! `RecalcAbilitys` 的尾（`PlayObject.cs:~2874`）把 `CharStatus/HitSpeed` 经
//! `RM_CHARSTATUSCHANGED` 入队；`MagicArr` 由 `MagicList` 填（`:3681-3684`）⇒ 尾段的
//! `MagicArr[id] != null` 等价于「`MagicList` 含该 id」（本模块如此判定）。
//!
//! **17a 的范围**：`RecalcAbilitys` 的**技能被动**（`HitPoint/SpeedPoint/DC` 与 `WAbil` 重算）
//! 未建模（IT-17b）。本模块按 oracle 的**同值**发 `RM_CHARSTATUSCHANGED`（`GetCharStatus()`
//! 与当前 `HitSpeed`）——对**无被动技能**与 oracle 逐字节相同；有被动技能时 `17b` 才补齐。
//! **IT-17 输入注入批**：657 上线三字段的 oracle 语义（转换点 `PlayObject.Message.cs:1845-1848`）
//! 是 `param=LoWord(CharStatus)`、`tag=HiWord(CharStatus)`、`series=泵时实时 `HitSpeed` 字段`
//! （发射点 `:2876` 的 wParam 是 `:2024` 重置后的旧值 0，转换点**不读它**）——本模块的
//! `sm_charstatuschanged(actor, world.get_char_status(actor), sg.hit_speed)` 与之逐字节同构；
//! `sg.hit_speed` 由 `recalc_session_ability` 按 oracle `:2899` 写入，buff 位由 `world.buffs`
//! 的 `GetBuffStatus` 等价实现（`OR(0x80000000>>i)`）给出——向量批以探针真值对拍（golden
//! `expect.hit_speed/char_status` 全 8 例非零）。
//!
//! # 帧序（线上）
//!
//! 即时：`SM_ADDMAGIC(210)` → `+LNG`/`+WID`（**原始串帧**，`SendSocket(string)`）→ `SM_EAT_OK(635)`；
//! 随后按队列优先级：`RM_WEIGHTCHANGED(10115→622)` → `RM_CHARSTATUSCHANGED(10139→657)` →
//! `RM_SYSMESSAGE(11004→100)`。

use std::collections::HashMap;
use std::sync::Arc;

use crate::ability::ItemCatalog;
use crate::record::MagicRcd;
use crate::{OnlineRegistry, SessionGround, SimToGate};

/// 技能 id 的唯一源在 [`crate::skill_ability`]（避免两处常量漂移）。
pub use crate::skill_ability::{SKILL_BANWOL, SKILL_ERGUM};

/// `PlayObject.MagicList` + `MagicArr` 的等价快照 + 战士技开关 + 魔法表/文案快照。
///
/// **写点**：`list` 只由两处写——① 登录 `session_ground_from_login`（`magicaudit::filter_for_logon`
/// 对 `record.data.magic` 的过滤结果，与 oracle `GetHumData` 同口径）；② 本模块学成时追加一件。
/// 持久化由 `build_save_record` 从本快照回写记录（`pad_for_save`）。
/// `use_thrusting`/`use_half_moon` 对应 `PlayObject.cs:168-169`；`table` 是进程级 `M2Share.MagicList`
/// 查表面的 `Arc` 快照（登录时取一次）；两个 sys 文案来自 `[String] ThrustingOn/HalfMoonOn`。
#[derive(Clone)]
pub struct MagicState {
    pub list: Vec<Option<MagicRcd>>,
    pub use_thrusting: bool,
    pub use_half_moon: bool,
    /// `PlayObject.CrsHitkill`（`PlayObject.cs`；34 逐日剑法开关，`ClientSpellXY` case `:1218`）。
    pub crs_hitkill: bool,
    /// `PlayObject.FireHitSkill`（26 烈火剑法充能态，`ClientSpellXY` case `:1168`）。
    pub fire_hit_skill: bool,
    /// `PlayObject.LatestFireHitTick`（26 的 10s 限速戳；`AllowFireHitSkill` `PlayObject.Base.cs:653-664`）。
    pub latest_fire_hit_tick: i64,
    /// `PlayObject.TwinHitSkill`（38 狂风斩充能态，`AllowTwinHitSkill` `PlayObject.Base.cs:666-672`）。
    pub twin_hit_skill: bool,
    /// `PlayObject.LatestTwinHitTick`（38 的限速戳，同上）。
    pub latest_twin_hit_tick: i64,
    /// `PlayObject.DoMotaeboTick`（27 的 3s 限速戳，`ClientSpellXY` case `:1189`）。
    pub do_motaebo_tick: i64,
    /// `PlayObject.MBo43Kill`（43 `SKILL_43` 开关，`ClientSpellXY` case `:1253-1266` 写；同样无消费者）。
    pub mbo43_kill: bool,
    pub table: Option<Arc<game_svr::magics::MagicTable>>,
    /// `PlayObject.MagicArr`（`new UserMagic[50]`，`PlayObject.cs:1041`）：**只写不清**（`:3684`），
    /// 删技能后留残影（oracle 原样）。IT-17b 结构层填充；下游 `GetAttackMagic`/`GetHitMode` 读。
    pub arr: Vec<Option<MagicRcd>>,
    /// 最近一次 `RecalcHitSpeed` 的技能被动（IT-17b）：`HitPoint/HitPlus/HitDouble/AttackSkillCount`。
    pub passives: game_svr::skill_ability::SkillPassives,
    /// `PlayObject.PowerHit`（`PlayObject.cs:177`；攻杀剑法 arm 标志）。**攻击批 A** 只负责置位
    /// （`drain_yedo_events` 在 `armed` 时写 `true`）；消费（模式 3 的 `nPower += HitPlus` +
    /// 清零 + `SM_POWERHIT` 帧）在批 B —— 在那之前本字段只进不出，不参与任何判定。
    pub power_hit: bool,
    /// **攻击批 A（登记偏差的观测计数）**：有攻杀但武器 `Dura == 0` 而被跳过的次数。
    ///
    /// oracle 在那种状态下 `UseItems[Weapon]` 为 `null` ⇒ `PlayObject.Attack.cs:943` 解引用抛
    /// NRE、被 `:963` 的 `catch` 吞掉（攻击照常完成，只丢计数块）。本仓**跳过 + WRN**，并在
    /// **WRN 的同一分支**自增本计数 ⇒ 测试可以断言「跳过了且报告了」，而不是「静默跳过」或
    /// 「根本没跑到」（WRN 文本走 stdout，无捕获缝）。不参与任何游戏判定。
    pub yedo_no_weapon_warns: u32,
}

impl Default for MagicState {
    fn default() -> Self {
        Self {
            list: Vec::new(),
            use_thrusting: false,
            use_half_moon: false,
            crs_hitkill: false,
            fire_hit_skill: false,
            latest_fire_hit_tick: 0,
            twin_hit_skill: false,
            latest_twin_hit_tick: 0,
            do_motaebo_tick: 0,
            mbo43_kill: false,
            table: None,
            arr: vec![None; game_svr::skill_ability::MAGIC_ARR_LEN],
            passives: game_svr::skill_ability::SkillPassives::default(),
            power_hit: false,
            yedo_no_weapon_warns: 0,
        }
    }
}

impl std::fmt::Debug for MagicState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MagicState")
            .field("list", &self.list)
            .field("use_thrusting", &self.use_thrusting)
            .field("use_half_moon", &self.use_half_moon)
            .field("has_table", &self.table.is_some())
            .field("arr_len", &self.arr.iter().filter(|x| x.is_some()).count())
            .field("passives", &self.passives)
            .finish_non_exhaustive()
    }
}

/// 书籍（`StdMode 4`）的宿主拦截。返回 `true` = 已接管（调用方 `return`）；`false` = 未命中/四门之一
/// 失败 ⇒ 交回 `use_item`（其 StdMode 4 无臂 ⇒ 落 `SM_EAT_FAIL(636)`，与 oracle 失败支同帧）。
///
/// **`boNODRUG` 不适用于书**（oracle 的 `EatItems` 只覆盖 StdMode 0..=3；`case 4` 直接 `ReadBook`）。
#[allow(clippy::too_many_arguments)]
pub(crate) fn maybe_read_book(
    world: &mut mir2_world_bevy::World,
    sg: &mut SessionGround,
    actor_id: i32,
    make_index: i32,
    socket: i32,
    online: &OnlineRegistry,
    gates: &HashMap<u8, tokio::sync::mpsc::UnboundedSender<SimToGate>>,
    catalog: &dyn ItemCatalog,
    colors: &crate::MsgColors,
    strings: &crate::strings::StringConf,
    dead: bool,
) -> bool {
    // `ClientUseItems`：`if (BoCanUseItem) { if (!Death) { … } }`（书不受 `boNODRUG`）。
    if dead {
        return false;
    }
    let Some(pos) = sg
        .state
        .item_list
        .iter()
        .position(|it| it.make_index == make_index)
    else {
        return false;
    };
    let Some(tpl) = catalog.get(sg.state.item_list[pos].index) else {
        return false;
    };
    if tpl.std_mode != 4 {
        return false;
    }
    let Some(table) = sg.magic.table.as_deref() else {
        // 未装配魔法表 ⇒ 无法判定「表里有没有」——按失败回落（不伪造）。
        return false;
    };
    // ① `FindMagic(stdItem.Name)`：缺表项 ⇒ 失败（回落 use_item ⇒ 636）。
    let Some(magic) = table.find_by_name(&tpl.name) else {
        return false;
    };
    // ②③④ 三门的**唯一实现**在 `world-bevy::magic::can_read_book`（`is_training_skill` 由
    // `magicaudit::is_training_skill` 给出）——本模块不复制第二份判定（防两处口径漂移）。
    let is_training = crate::magicaudit::is_training_skill(&sg.magic.list, magic.magic_id);
    if !mir2_world_bevy::magic::can_read_book(magic, sg.equip.job, sg.equip.abil.level, is_training)
    {
        return false;
    }

    let gate_idx = online
        .iter()
        .find(|e| e.actor_id == actor_id)
        .map_or(0, |e| e.gate_idx);
    let tx = crate::route_to_gate(gates, gate_idx, socket, "读书回帧");

    // `MagicList.Add(UserMagic{ Level=0, Key=0, TranPoint=0 })`；`MagicArr` 由它派生（见模块头）。
    sg.magic.list.push(Some(MagicRcd {
        mag_idx: magic.magic_id,
        level: 0,
        magic_key: 0,
        tran_point: 0,
    }));

    // ① `SM_ADDMAGIC(210)`：`ReadBook` 的 `UserMagic.Level = 0`（**不是** `AddItemSkill` 的 1）。
    if let Some(tx) = &tx {
        let _ = tx.send(SimToGate::SendFrame {
            socket,
            body: crate::smframe::sm_addmagic_with(0, magic),
        });
    }
    // ② 战士技开关：只在学到 12/25 且当前关着时触发（`MagicArr[id]!=null && !Use*`）。
    let mut sys_msg: Option<String> = None;
    if magic.magic_id == SKILL_ERGUM && !sg.magic.use_thrusting {
        sg.magic.use_thrusting = true;
        if let Some(tx) = &tx {
            let _ = tx.send(SimToGate::SendRaw {
                socket,
                payload: b"+LNG".to_vec(),
            });
        }
        sys_msg = Some(strings.thrusting_on().to_string());
    }
    if magic.magic_id == SKILL_BANWOL && !sg.magic.use_half_moon {
        sg.magic.use_half_moon = true;
        if let Some(tx) = &tx {
            let _ = tx.send(SimToGate::SendRaw {
                socket,
                payload: b"+WID".to_vec(),
            });
        }
        sys_msg = Some(strings.half_moon_on().to_string());
    }
    // ③ `SM_EAT_OK(635)`（直发，先于队列消息）。
    if let Some(tx) = &tx {
        let _ = tx.send(SimToGate::SendFrame {
            socket,
            body: crate::smframe::gm_data_body(
                crate::smframe::command_header(0, crate::useitem::SM_EAT_OK, 0, 0, 0),
                &[],
            ),
        });
    }
    // ④ 吃尾巴：移除本件 + `WeightChanged()`（`RM_WEIGHTCHANGED` ⇒ 622）。
    sg.state.item_list.remove(pos);
    sg.state.wabil.weight = crate::ground::recalc_bag_weight(catalog, &sg.state.item_list);
    if let Some(tx) = &tx {
        let _ = tx.send(SimToGate::SendFrame {
            socket,
            body: crate::smframe::sm_weightchanged(
                sg.state.wabil.weight,
                sg.state.wabil.wear_weight,
                sg.state.wabil.hand_weight,
            ),
        });
    }
    // ⑤ `RecalcAbilitys` 的 `RM_CHARSTATUSCHANGED`（657）：与本字段快照同值。
    if let Some(tx) = &tx {
        let _ = tx.send(SimToGate::SendFrame {
            socket,
            body: crate::smframe_ext::sm_charstatuschanged(
                actor_id,
                world.get_char_status(actor_id),
                sg.hit_speed,
            ),
        });
    }
    // ⑥ `ThrustingOnOff/HalfMoonOnOff` 的绿字（`RM_SYSMESSAGE` ⇒ 100）。
    if let (Some(tx), Some(text)) = (&tx, sys_msg.as_deref()) {
        let _ = tx.send(SimToGate::SendFrame {
            socket,
            body: crate::smframe::sm_sysmessage(
                actor_id,
                u16::from(colors.green_f),
                u16::from(colors.green_b),
                &crate::enter::gbk(text),
            ),
        });
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ability::StdItemCatalog;
    use crate::bag::BagItem;
    use crate::handle_eat_cmd;
    use crate::EatProcessEnv;
    use crate::{online_set, MsgColors, OnlineEntry, SessionGround};
    use serde_json::Value;
    use std::collections::HashMap;
    use tokio::sync::mpsc;

    fn fixture() -> Value {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/parity/m3/fixtures/readbook_vectors.json"
        );
        let raw = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("oracle 向量夹具缺失/不可读：{path}（{e}）—— 不许静默跳过"));
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{path} 不是合法 JSON：{e}"))
    }

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    fn magic_from(v: &Value) -> mir2_data::MagicInfo {
        let tl = v["train_level"].as_array().unwrap();
        let mt = v["max_train"].as_array().unwrap();
        let mut train_level = [0u8; 4];
        let mut max_train = [0i32; 4];
        for i in 0..4 {
            train_level[i] = tl.get(i).map_or(0, |x| x.as_u64().unwrap() as u8);
            max_train[i] = mt.get(i).map_or(0, |x| x.as_i64().unwrap() as i32);
        }
        mir2_data::MagicInfo {
            magic_id: v["magic_id"].as_u64().unwrap() as u16,
            magic_name: v["name"].as_str().unwrap().to_string(),
            effect_type: v["effect_type"].as_u64().unwrap() as u8,
            effect: v["effect"].as_u64().unwrap() as u8,
            spell: v["spell"].as_u64().unwrap() as u16,
            power: v["power"].as_u64().unwrap() as u16,
            max_power: v["max_power"].as_u64().unwrap() as u16,
            job: v["job"].as_u64().unwrap() as u8,
            train_level,
            max_train,
            train_lv: v["train_lv"].as_u64().unwrap() as u8,
            delay_time: v["delay_time"].as_i64().unwrap() as i32,
            def_spell: v["def_spell"].as_u64().unwrap() as u8,
            def_power: v["def_power"].as_u64().unwrap() as u8,
            def_max_power: v["def_max_power"].as_u64().unwrap() as u8,
            desc: v["desc"].as_str().unwrap_or("").to_string(),
        }
    }

    /// 一帧的可比形态：`Cmd` = 12B 命令头 + 尾（`recog` 按夹具身份归一），`Raw` = 原始串帧 payload。
    #[derive(Debug, PartialEq, Eq)]
    enum Frame {
        Cmd(i32, u16, u16, u16, u16, String),
        Raw(String),
    }

    fn frame_from_bytes(b: &[u8], remap: Option<(i32, i32)>) -> Frame {
        if b.len() < 12 {
            return Frame::Raw(hex(b));
        }
        let raw_recog = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        let recog = match remap {
            Some((from, to)) if raw_recog == from => to,
            _ => raw_recog,
        };
        Frame::Cmd(
            recog,
            u16::from_le_bytes([b[4], b[5]]),
            u16::from_le_bytes([b[6], b[7]]),
            u16::from_le_bytes([b[8], b[9]]),
            u16::from_le_bytes([b[10], b[11]]),
            hex(&b[12..]),
        )
    }

    fn frame_from_hex(s: &str, remap: Option<(i32, i32)>) -> Frame {
        let bytes: Vec<u8> = (0..s.len() / 2)
            .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).expect("hex"))
            .collect();
        frame_from_bytes(&bytes, remap)
    }

    /// **IT-17a 宿主级对拍**：驱动 `handle_eat_cmd`，比
    /// ① 线上帧**逐字节**（`payload` hex：SM 帧的 12B 头 + 尾；`+LNG`/`+WID` 原始串帧同面）
    /// ② `MagicList` 终态（mag_idx/level/key/tran_point，保序）③ 两战士技开关 ④ 背包/Weight
    /// ⑤ 失败四门（各自只落 636、物品保留、MagicList 不变）⑥ **IT-17 批点亮的两个状态字段**
    /// `hit_speed`/`char_status`（8 例交叉相等：成功例 = case 内 Recalc 后值，失败例 = 预 Recalc 保持值）。
    ///
    /// **17a 边界**：`RecalcAbilitys` 的技能被动（HitPoint/SpeedPoint/DC/WAbil）未建模（IT-17b）；
    /// 本批选无被动技能。**IT-17 输入注入批**：按 `ctx.prebuffs`/`ctx.weapon` 装配同款会话输入
    /// （`world.buffs.add_buff` + `use_items[SLOT_WEAPON]`），并**先跑一次建模 Recalc**（登录态，
    /// oracle 探针预 Recalc 的等价物）——657 的 `tag=HiWord(位公式)`、`series=hit_speed`、
    /// `param=LoWord(…)=0`（转换点 `PlayObject.Message.cs:1845-1848`）。
    ///
    /// 红检（四门+两尾+两字段）：删「已学」判 ⇒ 已学例变成功；`job` 判恒真 ⇒ 职业不符例成功；
    /// （首试不咬留痕：把「玩家 job」改 99 **不咬**——谓词是 `magic.job == 99 || magic.job == (byte)Job`；
    ///  把 `can_read_book` 的 job 实参换成 `magic.job` 才咬住。）
    /// 删 MagicList append ⇒ 成功例的帧/状态红；210 传 `sm_addmagic`（Level=1）⇒ 尾字节红；
    /// 掉 `use_items[SLOT_WEAPON]` 装配 ⇒ `hit_speed` 0⇒657 series 字节红+⑥红；掉 `add_buff` ⇒ tag 红+⑥红。
    fn run_readbook_parity(only: Option<&str>) {
        let doc = fixture();
        let magics: Vec<mir2_data::MagicInfo> = doc["magic_table"]
            .as_array()
            .expect("magic_table")
            .iter()
            .map(magic_from)
            .collect();
        let magic_table = game_svr::magics::MagicTable::new(magics);
        let mut catalog = StdItemCatalog::new();
        for t in doc["templates"].as_array().expect("templates") {
            catalog.insert(
                t["index"].as_u64().unwrap() as u16,
                mir2_data::StdItem {
                    name: t["name"].as_str().unwrap().to_string(),
                    std_mode: t["std_mode"].as_u64().unwrap() as u8,
                    shape: t["shape"].as_u64().unwrap() as u8,
                    weight: t["weight"].as_u64().unwrap() as u8,
                    dura_max: 20,
                    // IT-17 批新增键：RealAttackSpeed 的源字节（武器 MAC 高字节；书类恒 0）。
                    mac: t["mac"].as_u64().unwrap() as u16,
                    ..Default::default()
                },
            );
        }

        let mut cases = doc["cases"].as_array().expect("cases").clone();
        if let Some(name) = only {
            cases.retain(|c| c["name"].as_str() == Some(name));
            assert_eq!(cases.len(), 1, "单例筛选 {name} 未命中");
        } else {
            assert!(cases.len() >= 8, "至少 8 例（成功×3 + 四门 + 双失败）");
        }
        for c in &cases {
            let ctx = &c["ctx"];
            let expect = &c["expect"];
            let index = ctx["index"].as_u64().unwrap() as u16;
            let mut world = mir2_world_bevy::World::new(32, 32);
            world.open_session("hero");
            let mut e = mir2_world_bevy::EntitySpawn::new(0, "hero", 5, 5);
            e.race = mir2_world_bevy::ACTOR_RACE_PLAY;
            let actor_id = world.enter_map("hero", e, 0).expect("进图");

            let mut sg = crate::tests::test_session_ground(actor_id);
            sg.equip.job = ctx["job"].as_u64().unwrap() as u8;
            sg.equip.abil.level = ctx["level"].as_u64().unwrap() as u8;
            sg.state.item_list = vec![BagItem {
                make_index: 1,
                index,
                dura: 10,
                dura_max: 20,
                desc: [0; 14],
            }];
            // `SM_WEIGHTCHANGED(622)` 的三字段前值（`WeightChanged` 只改 Weight）。
            sg.state.wabil.weight = ctx["wabil_before"]["weight"].as_u64().unwrap() as u16;
            sg.state.wabil.wear_weight = ctx["wabil_before"]["wear_weight"].as_u64().unwrap() as u8;
            sg.state.wabil.hand_weight = ctx["wabil_before"]["hand_weight"].as_u64().unwrap() as u8;
            sg.magic = MagicState {
                list: ctx["known"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| {
                        Some(MagicRcd {
                            mag_idx: v.as_u64().unwrap() as u16,
                            level: 0,
                            magic_key: 0,
                            tran_point: 0,
                        })
                    })
                    .collect(),
                use_thrusting: false,
                use_half_moon: false,
                table: Some(std::sync::Arc::new(magic_table.clone())),
                ..MagicState::default()
            };
            // **IT-17 输入注入批**：按 ctx 新键装配与 oracle 探针同款的会话输入，并**先跑一次
            // 建模 Recalc**（登录态）——oracle 侧预 Recalc 的等价物。此后 `sg.hit_speed`/`wabil`
            // 与 world buff 位就是 case 起点的会话历史；失败例不再 Recalc ⇒ 两字段保持该值
            // （oracle 同构：探针失败例的字段也停留在预 Recalc 值）。
            if let Some(w) = ctx["weapon"].as_object() {
                sg.use_items[crate::ability::SLOT_WEAPON] = Some(BagItem {
                    make_index: w["make_index"].as_i64().unwrap() as i32,
                    index: w["index"].as_u64().unwrap() as u16,
                    dura: w["dura"].as_u64().unwrap() as u16,
                    dura_max: 20,
                    desc: [0; 14],
                });
            }
            for pb in ctx["prebuffs"].as_array().unwrap() {
                let code = pb["type"].as_u64().unwrap() as u8;
                let bt = match code {
                    2 => mir2_world_bevy::BuffType::Defense,
                    3 => mir2_world_bevy::BuffType::CanNotRun,
                    c => panic!("夹具 buff 类型未建模: {c}"),
                };
                world.buffs.add_buff(
                    actor_id,
                    bt,
                    pb["dura"].as_i64().unwrap() as i32,
                    pb["state_val"].as_i64().unwrap() as i32,
                    0,
                );
            }
            let _ = crate::recalc_session_ability(&mut sg, &catalog, None, None, None);
            let mut sessions: HashMap<String, SessionGround> = HashMap::new();
            sessions.insert("hero".into(), sg);
            let mut online: OnlineRegistry = OnlineRegistry::new();
            online_set(
                &mut online,
                OnlineEntry {
                    account: "ua".into(),
                    chr_name: "hero".into(),
                    session_id: 1,
                    gate_idx: 0,
                    socket: 901,
                    actor_id,
                },
            );
            let (tx, mut rx) = mpsc::unbounded_channel::<SimToGate>();
            let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
            gates.insert(0, tx);
            let colors = MsgColors {
                red_f: 255,
                red_b: 56,
                hear_f: 0,
                hear_b: 255,
                whisper_f: 0,
                whisper_b: 0,
                green_f: 0xDB,
                green_b: 0xFF,
                blue_f: 0xFF,
                blue_b: 0xFC,
            };
            let unbind = game_svr::unbindlist::UnbindList::default();
            let item_number = std::sync::atomic::AtomicI32::new(0);
            handle_eat_cmd(
                1,
                0,
                0,
                None,
                "hero",
                901,
                &online,
                &mut sessions,
                &gates,
                &catalog,
                &mut world,
                &colors,
                90,
                &game_svr::strings::StringConf::default(),
                None,
                &unbind,
                &item_number,
                Some(EatProcessEnv {
                    npc_host: None,
                    npc_vars: &game_svr::npc::NpcVars::default(),
                    gm: None,
                    lottery: &crate::winlottery::WinLotteryShared::new(
                        crate::config::WinLotteryCfg::default().init_state(),
                    ),
                    castle_war_states: &std::collections::HashMap::new(),
                }),
                crate::config::WinLotteryCfg::default(),
                10_000_000,
                crate::config::WeaponMakeLuckCfg::default(),
            );

            // ① 帧：golden immediate ++ pumped 的 `payload` 逐字节（`recog` 按夹具身份归一；
            //    原始串帧按 payload 字节比）。`+LNG`/`+WID` 是 `Raw`，不解析成命令头。
            let golden_actor = ctx["actor_id"].as_i64().unwrap() as i32;
            let remap = Some((golden_actor, actor_id));
            let mut want: Vec<Frame> = Vec::new();
            for f in expect["immediate_frames"].as_array().unwrap() {
                want.push(frame_from_hex(f["payload"].as_str().unwrap(), remap));
            }
            for f in expect["pumped"].as_array().unwrap() {
                want.push(frame_from_hex(f["payload"].as_str().unwrap(), remap));
            }
            let mut got: Vec<Frame> = Vec::new();
            while let Ok(ev) = rx.try_recv() {
                match ev {
                    SimToGate::SendFrame { body, .. } => got.push(frame_from_bytes(&body, None)),
                    SimToGate::SendRaw { payload, .. } => {
                        got.push(frame_from_bytes(&payload, None))
                    }
                    _ => {}
                }
            }
            assert_eq!(got, want, "{}: 帧逐字节（含原始串帧）", c["name"]);

            // ⑤ 0 抽由**记录器**自证（ReadBook 路径无 `Random`）。
            assert_eq!(
                expect["rng_draw_count"].as_i64().unwrap(),
                0,
                "{}: ReadBook 路径记录器抽数必须为 0",
                c["name"]
            );
            assert!(
                expect["rng"].as_array().unwrap().is_empty(),
                "{}: 抽数为 0 ⇒ 逐次账本为空",
                c["name"]
            );

            // ② MagicList 终态。
            let sg = &sessions["hero"];
            let got_list: Vec<(i64, i64, i64, i64)> = sg
                .magic
                .list
                .iter()
                .flatten()
                .map(|m| {
                    (
                        i64::from(m.mag_idx),
                        i64::from(m.level),
                        i64::from(m.magic_key),
                        i64::from(m.tran_point),
                    )
                })
                .collect();
            let want_list: Vec<(i64, i64, i64, i64)> = expect["magic_list"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| {
                    (
                        m["mag_idx"].as_i64().unwrap(),
                        m["level"].as_i64().unwrap(),
                        m["key"].as_i64().unwrap(),
                        m["train_point"].as_i64().unwrap(),
                    )
                })
                .collect();
            assert_eq!(got_list, want_list, "{}: MagicList 终态", c["name"]);

            // ③ 战士技开关。
            assert_eq!(
                sg.magic.use_thrusting,
                expect["use_thrusting"].as_bool().unwrap(),
                "{}: UseThrusting",
                c["name"]
            );
            assert_eq!(
                sg.magic.use_half_moon,
                expect["use_half_moon"].as_bool().unwrap(),
                "{}: UseHalfMoon",
                c["name"]
            );

            // ④ 背包/Weight（失败支保留原件）：逐件比 `(make_index,index,dura,dura_max)`。
            // **夹具未记录 `desc`** ⇒ 该字段本卡不覆盖（见 openmir2-itemlist-element-compare）。
            let golden_items = expect["item_list"].as_array().unwrap();
            assert_eq!(
                sg.state.item_list.len(),
                golden_items.len(),
                "{}: 背包件数",
                c["name"]
            );
            for (got, want) in sg.state.item_list.iter().zip(golden_items.iter()) {
                assert_eq!(
                    (
                        i64::from(got.make_index),
                        i64::from(got.index),
                        i64::from(got.dura),
                        i64::from(got.dura_max),
                    ),
                    (
                        want["make_index"].as_i64().unwrap(),
                        want["index"].as_i64().unwrap(),
                        want["dura"].as_i64().unwrap(),
                        want["dura_max"].as_i64().unwrap(),
                    ),
                    "{}: 背包元素字段（夹具未记录 desc ⇒ 不覆盖）",
                    c["name"]
                );
            }
            assert_eq!(
                i64::from(sg.state.wabil.weight),
                expect["weight"].as_i64().unwrap(),
                "{}: Weight",
                c["name"]
            );

            // ⑥ **IT-17 批点亮的两字段状态面**（8 例交叉相等）：`hit_speed` 是 657 `series` 的源
            // 字段（`PlayObject.cs:2899`），`char_status` 是 657 `param/tag` 的源（`:2874`
            // `GetCharStatus()` = buff 位公式 | CharStatusEx 低 16 位；两侧同公式）。
            assert_eq!(
                i64::from(sg.hit_speed),
                expect["hit_speed"].as_i64().unwrap(),
                "{}: HitSpeed 字段（657 series 源）",
                c["name"]
            );
            assert_eq!(
                world.get_char_status(actor_id),
                expect["char_status"].as_i64().unwrap() as i32,
                "{}: CharStatus（buff 位公式 | CharStatusEx）",
                c["name"]
            );
        }
    }

    /// 全量（8 例）。
    #[test]
    fn it17a_read_book_host_parity() {
        run_readbook_parity(None);
    }

    /// **成功支 + 原始串帧**（`+LNG`）：逐字节比 210/`+LNG`/635/622/657/100。
    #[test]
    fn it17a_success_ergum_raw_socket() {
        run_readbook_parity(Some("eat_ok_book_ergum_learns_and_thrusting"));
    }

    /// **门②已学**：`MagicList` 已有 ⇒ 636、物品保留。
    #[test]
    fn it17a_gate_already_learned() {
        run_readbook_parity(Some("eat_fail_book_already_learned"));
    }

    /// **门③职业不符**。
    #[test]
    fn it17a_gate_job_mismatch() {
        run_readbook_parity(Some("eat_fail_book_job_mismatch"));
    }

    /// **门④等级边界**（`Abil.Level == TrainLevel[0]` ⇒ 可学，证明 `>=` 不是 `>`）。
    #[test]
    fn it17a_gate_level_boundary_equal() {
        run_readbook_parity(Some("eat_ok_book_level_boundary_equal"));
    }

    /// **门①表里无名**。
    #[test]
    fn it17a_gate_magic_not_in_table() {
        run_readbook_parity(Some("eat_fail_book_magic_not_in_table"));
    }
}
