//! M2 **S3 后半段（第一格）**：世界 → 客户端的**同屏 AOI 下行**编码层。
//!
//! 机理（照 oracle，不按公式重推）：
//!
//! ```text
//! 世界 tick 的 AOI 刷新（crates/world 的 SearchViewRange）
//!   ⇒ 观察者实体的 VisibleActors 里，本轮"新看见"的对象标 VisibleFlag.Show
//!   ⇒ PlayObject.Run 的 Show 支（PlayObject.cs:3230-3244）
//!        SendMsg(baseObject, RM_TURN, Dir, CurrX, CurrY, 0, GetShowName())
//!   ⇒ PlayObject.GetMessage 的 RM_TURN 支（PlayObject.Message.cs:1524-1560）
//!        SM_TURN(12B 头 + Encode(CharDesc) [ + Encode("ShowName/Color") ])
//!        紧接 SM_FEATURECHANGED(LoWord(feature), HiWord(feature), GetFeatureEx())
//! ```
//!
//! 本模块只做**纯编码 + 展示表 + 亮度计算**，不碰 I/O：路由（按 `[GateIdx, Socket]` 选通道、
//! 发 `SimToGate::SendFrame`）留在 `main.rs` 的 sim 循环里（它才持有 `online`/`gates`）。
//!
//! **为什么需要展示表**：`crates/world` 的 `Entity` 只存世界模拟字段（坐标/HP/…），
//! 不含 `Dir/Light/Feature/CharStatus/ShowName` 这些**展示层**字段（它们来自角色记录 + 装备，
//! 属于 `bins/game-svr` 的读档面）。故在进世界时把 oracle 在 `SendLogon` 同轮算出的值
//! 登记进 [`DisplayTable`]，同屏下行时按 `ActorId` 查回。**不把展示字段塞进 `crates/world`**
//! （那是 wbevy/m3bag 的共享写集）。
//!
//! **本格仍缺（登记，不猜）**：
//! - `RM_DISAPPEAR`（`VisibleFlag.Hidden` 支，`PlayObject.cs:3228`）——没有 `sm_disappear` 编码器；
//! - 怪物的 `SM_TURN`：`Entity` 有怪物，但生产路径尚未生成怪物（无 `DisplayTable` 项）⇒ 跳过；
//! - `GetChrColor` 的**行会/PK/战争**分支（需要行会与战争系统，未移植）⇒ 用 `NameColor`；
//!   无行会/PK 时 oracle 的结果就是 `NameColor`（`PlayObject.cs:3585-3595` 的非战斗支）。

use crate::ability::ItemCatalog;
use crate::feature::{make_human_feature, make_monster_feature};
use crate::smframe;
use mir2_data::models::MonsterInfo;

/// `EfftypeConst.EFFECTIVE_HIGHLEVEL`（`src/M2Server/Actor/ActorConst.cs:43`）：
/// `GetMyLight` 的等级下限（`PlayObject.cs:5407`）。
pub const EFFECTIVE_HIGHLEVEL: u8 = 50;

/// `ItemLocation.Dress`..`Charm` 的槽数（`src/OpenMir2/Enums/ItemLocation.cs`：0..=12）。
pub const USE_ITEM_SLOTS: usize = 13;

/// `PlayObject.GetMyLight()`（`PlayObject.cs:5404-5430`）：等级 ≥50 记 1；再取
/// `UseItems[Dress..Charm]` 中 `Index>0 && Dura>0` 的模板 `StdItem.Light` 的最大值。
///
/// `equipped` = `(Index, Dura)` 序列，**按 `ItemLocation` 顺序**（Dress=0 … Charm=12）。
#[must_use]
pub fn my_light(level: u8, equipped: &[(u16, u16)], catalog: &dyn ItemCatalog) -> u8 {
    let mut light = if level >= EFFECTIVE_HIGHLEVEL { 1 } else { 0 };
    for (index, dura) in equipped.iter().take(USE_ITEM_SLOTS) {
        if *index > 0 && *dura > 0 {
            if let Some(std) = catalog.get(*index) {
                if light < std.light {
                    light = std.light;
                }
            }
        }
    }
    light
}

/// 线上文本一律按 **GB2312/GBK 字节**（`HUtil32.GetBytes` = `Encoding.GetEncoding("gb2312")`，
/// `EDCode.EncodeString` 编码的就是它）——本层不做 UTF-8 线上转换。
///
/// `bins/game-svr` 的 `enter::gbk` 已委托向此（同一实现，避免两份口径）；本函数放在 lib 侧是为了
/// 让 [`monster_display`] 这样的纯编码函数不依赖 bin 模块。
#[must_use]
pub fn gbk(s: &str) -> Vec<u8> {
    encoding_rs::GBK.encode(s).0.into_owned()
}

/// oracle `BaseObject.NameColor` 的构造缺省（`src/M2Server/Actor/BaseObject.cs:385`）。
///
/// 怪物既非玩家、也无 master，观察者侧的 `GetChrColor`（`PlayObject.GetChrColor` 的非玩家支
/// ⇒ `BaseObject.GetChrColor`）最终落到 `baseObject.GetNameColor()` = 该字段。
pub const MONSTER_NAME_COLOR: u8 = 255;

/// `M2Share.FilterShowName(sName)`（`src/M2Server/M2Share.cs:571-594`）：从**首个 ASCII 数字或
/// `-`** 处截断（`"稻草人2"` ⇒ `"稻草人"`、`"a-1"` ⇒ `"a"`）；没有则原样返回。
///
/// 玩家走的是 [`crate::enter::show_name`]（`PlayObject.GetShowName()` 的 override，**不**过滤）；
/// 怪物/守卫/门墙走 `BaseObject.GetShowName()`（`BaseObject.Base.cs:779`），它先过本函数。
#[must_use]
pub fn filter_show_name(name: &str) -> &str {
    match name.find(|c: char| c.is_ascii_digit() || c == '-') {
        Some(i) => &name[..i],
        None => name,
    }
}

/// 怪物的展示层 [`ActorDisplay`]（oracle `BaseObject` 的怪侧取值）。
///
/// 逐项锚点：
/// - `feature` = `GetFeature()` = `MakeMonsterFeature(RaceImg, MonsterWeapon, Appr)`
///   （`BaseObject.Base.cs:911`；`RaceImg`/`Appr` 来自 `MonsterInfo`，`MonsterWeapon` =
///   `HUtil32.LoByte(monster.MP)`，`WorldServer.MonGen.cs:888-902`）；
/// - `feature_ex` = `GetFeatureEx()` = 0（`BaseObject.Base.cs:926`，怪物不 override）；
/// - `light` = `BaseObject.Light` = 0（构造缺省 `BaseObject.cs:384`；怪物从不 `GetMyLight`）；
/// - `status` = `GetCharStatus()`（`BaseObject.cs:1492`）= buff 位；怪物无 buff ⇒ 0；
/// - `name_color` = [`MONSTER_NAME_COLOR`]（`GetChrColor` 的非玩家/非 NPC/无 master 支）；
/// - `show_name` = `FilterShowName(ChrName)` 的 GBK 字节（`BaseObject.Base.cs:779`）。
#[must_use]
pub fn monster_display(dir: u8, info: &MonsterInfo) -> ActorDisplay {
    let monster_weapon = (info.mp & 0x00FF) as u8;
    ActorDisplay::new(
        dir,
        0,
        make_monster_feature(info.race_img, monster_weapon, info.appr),
        0,
        0,
        MONSTER_NAME_COLOR,
        gbk(filter_show_name(&info.name)),
    )
}

/// 玩家缺省名色（`enter::PLAY_NAME_COLOR` = 255，`src/M2Server/Player/PlayObject.cs` 的
/// `NameColor` 缺省）。`bins/game-svr` 的 bin 模块 `enter` 不在 lib 里 ⇒ 此处复写常量值。
pub const PLAYER_NAME_COLOR: u8 = 255;

/// **⑨-2 残余（46 分身）**：`PlayCloneObject` 的展示层 [`ActorDisplay`]。
///
/// oracle `PlayCloneObject` 只从主人处拷 `Gender`/`Hair`（`PlayCloneObject.cs:14-15`），
/// `Dress`/`Weapon` 是其自身 `PlayObject` 构造器的缺省 `0` ⇒
/// `GetFeatureToLong()` = `MakeHumanFeature(0, gender, gender, hair*2+gender)`
/// （`M2Share.cs:1965`：`dress*2+gender` 里 `dress=0`、`gender+weapon*2` 里 `weapon=0`）。
///
/// 骨架 `Entity` 不存 Gender/Hair（见 `crates/world::slave::spawn_clone`），它们无损编码在
/// `caster.feature` 的位域 `[raceImg:8][weapon':8][hair':8][dress':8]` 里：
/// `weapon' = gender + weapon*2` ⇒ `gender = byte1 & 1`；
/// `hair' = hair*2 + gender` ⇒ `hair = byte2 >> 1`。
///
/// `status = 0`（无 buff）、`feature_ex = 0`（未骑马/无斗篷特效）、`light = 0`
/// （`PlayObject` 构造器 `Light` 缺省 `0`，`BaseObject.cs:384`）、`name_color` =
/// [`PLAYER_NAME_COLOR`]、`show_name` = GBK `"Clone"`（`crates/world::CLONE_NAME`）。
#[must_use]
pub fn clone_display(dir: u8, caster: &ActorDisplay) -> ActorDisplay {
    let f = caster.feature as u32;
    let gender = ((f >> 8) & 1) as u8;
    let hair = (((f >> 16) & 0xFF) as u8) >> 1;
    ActorDisplay::new(
        dir,
        0,
        make_human_feature(0, gender, gender, hair * 2 + gender),
        0,
        0,
        PLAYER_NAME_COLOR,
        gbk(mir2_world::CLONE_NAME),
    )
}

/// 一个 actor 的**展示层**字段（oracle `PlayObject` 在 `SendLogon` 同轮产出的那几项）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorDisplay {
    /// `BaseObject.Dir`（`ClientChangeDir`/`ClientWalkXY` 会改；本格登记进世界时的初值 `record.dir`）。
    pub dir: u8,
    /// `BaseObject.Light` = `GetMyLight()`。
    pub light: u8,
    /// `GetFeatureToLong()` = `MakeHumanFeature(0, dress*2+gender, gender+weapon*2, hair*2+gender)`。
    pub feature: i32,
    /// `GetFeatureEx()` = `MakeWord(OnHorse ? HorseType : 0, DressEffType)`
    /// （`CharacterObject.cs:146-149`；`OnHorse`/`HorseType`/`DressEffType` 现网无赋值点 ⇒ 0）。
    pub feature_ex: u16,
    /// `GetCharStatus()` = buff 状态位（buff 系统未移植 ⇒ 0，与金标准 #3/#16 的 `Status==0` 一致）。
    pub status: i32,
    /// 观察者相对被观察者的 `NameColor`（`GetChrColor` 的**非行会/PK/战争**支
    /// ⇒ `baseObject.NameColor`，玩家默认 `PLAY_NAME_COLOR` = 255）。
    pub name_color: u8,
    /// `GetShowName()` 的 GBK 字节（帧 42 `SM_USERNAME` 用的是同一份）。
    pub show_name: Vec<u8>,
}

/// `PlayObject._dir` 字段尚未进 `crates/world`，这里由进世界时的记录值初始化；
/// 后续装备/骑乘变化会改它（属能力层，登记）。
impl ActorDisplay {
    /// `status`/`name_color` 也由调用方给（前者 = buff 状态位，后者 = `enter::PLAY_NAME_COLOR`），
    /// 免得把 bin 的常量搬进 lib。
    #[must_use]
    pub fn new(
        dir: u8,
        light: u8,
        feature: i32,
        feature_ex: u16,
        status: i32,
        name_color: u8,
        show_name: Vec<u8>,
    ) -> Self {
        ActorDisplay {
            dir,
            light,
            feature,
            feature_ex,
            status,
            name_color,
            show_name,
        }
    }
}

/// `ActorId -> ActorDisplay` 的确定性表（`Vec`，不靠 `HashMap` 迭代序）。
#[derive(Clone, Debug, Default)]
pub struct DisplayTable {
    entries: Vec<(i32, ActorDisplay)>,
}

impl DisplayTable {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记/覆盖（按 `ActorId`）。
    pub fn set(&mut self, actor_id: i32, display: ActorDisplay) {
        if let Some(slot) = self.entries.iter_mut().find(|(id, _)| *id == actor_id) {
            slot.1 = display;
            return;
        }
        self.entries.push((actor_id, display));
    }

    #[must_use]
    pub fn get(&self, actor_id: i32) -> Option<&ActorDisplay> {
        self.entries
            .iter()
            .find(|(id, _)| *id == actor_id)
            .map(|(_, d)| d)
    }

    pub fn remove(&mut self, actor_id: i32) {
        self.entries.retain(|(id, _)| *id != actor_id);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// `CharDesc`（8B，`PlayObject.Message.cs` 的 `turnmessage`/`walkmessage`）：
/// `Feature(i32 LE) + Status(i32 LE)`。
#[must_use]
pub fn char_desc(feature: i32, status: i32) -> [u8; 8] {
    let mut d = [0u8; 8];
    d[0..4].copy_from_slice(&feature.to_le_bytes());
    d[4..8].copy_from_slice(&status.to_le_bytes());
    d
}

/// **出现帧**路径（`PlayObject.Run` 的 `Show` 支）产出的**一组两帧**：`SM_TURN` + 紧随的
/// `SM_FEATURECHANGED`（`PlayObject.Message.cs:1552-1560`）。
///
/// 该路径的 oracle `SendMsg(baseObject, RM_TURN, Dir, CurrX, CurrY, 0, baseObject.GetShowName())`
/// （`PlayObject.cs:3240`）**带非空 `Msg`** ⇒ `PlayObject.Message.cs:1549-1555` 追加
/// `EncodeString($"{Msg}/{nObjCount}")` **名尾**。
///
/// - `target` = 被看见对象的 `ActorId`（命令头 `Recog`）；
/// - `x`/`y` = 该对象的 `CurrX`/`CurrY`（oracle 的 `processMsg.nParam1/nParam2`）；
/// - `SM_TURN` 的 `Series` = `MakeWord(Dir, Light)`，尾 = `Encode(CharDesc(Feature,Status))`，
///   非空 `ShowName` 时再直接相接 `Encode("ShowName/Color")`；
/// - `SM_FEATURECHANGED` = `(Recog=target, Param=LoWord(feature), Tag=HiWord(feature), Series=feature_ex)`。
///
/// ⚠️ **`CM_TURN` 路径不要用本函数**（`Msg` 恒空、无名尾）——见 [`turn_pair_nameless`]。
#[must_use]
pub fn turn_pair(target: i32, x: u16, y: u16, d: &ActorDisplay) -> Vec<Vec<u8>> {
    let desc = char_desc(d.feature, d.status);
    let mut msg = d.show_name.clone();
    msg.push(b'/');
    msg.extend_from_slice(d.name_color.to_string().as_bytes());
    vec![
        smframe::sm_turn(target, x, y, d.dir, d.light, &desc, &msg),
        smframe::sm_featurechanged(target, d.feature, d.feature_ex),
    ]
}

/// **`CM_TURN` 下行**路径产出的**一组两帧**：`SM_TURN` + 紧随的 `SM_FEATURECHANGED`（`RM_TURN` 支）。
///
/// oracle 路径：`CM_TURN` → `ClientChangeDir` 成功 → `Walk(RM_TURN)`（`PlayObject.Operate.cs:334`）
/// → `SendRefMsg(RM_TURN, Dir, CurrX, CurrY, 0, "")`（`PlayObject.cs:1922`；`sMsg` 空原样透传）
/// ⇒ `processMsg.Msg` **恒空** ⇒ `PlayObject.Message.cs:1549-1555` 的 `if (!string.IsNullOrEmpty(Msg))`
/// **不成立**，帧尾只有 `Encode(8B CharDesc)`，**没有** `Encode("ShowName/Color")` 名尾。
/// `SM_FEATURECHANGED` 照发（同一 `case` 里 `wIdent == RM_TURN` 支无条件发）。
///
/// 名尾**只属于**「出现帧」路径（[`turn_pair`]，`PlayObject.cs:3240`）；`RM_WALK`/`RM_RUN`
/// 同样不带名尾（见 [`move_frame`]）。
#[must_use]
pub fn turn_pair_nameless(target: i32, x: u16, y: u16, d: &ActorDisplay) -> Vec<Vec<u8>> {
    let desc = char_desc(d.feature, d.status);
    vec![
        smframe::sm_turn(target, x, y, d.dir, d.light, &desc, &[]),
        smframe::sm_featurechanged(target, d.feature, d.feature_ex),
    ]
}

/// `RM_WALK`/`RM_RUN`/`RM_HORSERUN`（`PlayObject.Message.cs:1403-1432`）与
/// `RM_PUSH`/`RM_RUSH`/`RM_RUSHKUNG`（同文件 `:1525-1560`）共用的**单帧**编码：
/// `MakeMessage(ident, ActorId, nParam1 /*=x*/, nParam2 /*=y*/, MakeWord(wParam /*=帧方向*/, Light))`
/// + 尾 `Encode(8B CharDesc)`。
///
/// **帧方向（`Series` 低字节）来自 `d.dir`**——这是本函数的契约：调用方必须把**该帧该用的方向**
/// 放进 `d.dir`。五条 ident 的方向来源各不同（逐条照 oracle）：`SM_WALK/SM_RUN/SM_HORSERUN`
/// 用**移动者当前 `Dir`**；`SM_BACKSTEP` 用 `nBackDir`、`SM_RUSH`/`SM_RUSHKUNG` 用 `nDir`——后三者的
/// 方向**不是**角色 `Dir`，故宿主在调本函数前把事件方向写进一份 `d` 的克隆（见
/// [`backstep_frame`]/[`rush_frame`]，避免误用角色朝向）。
///
/// 收件人口径（oracle）：`SM_WALK/SM_RUN/SM_HORSERUN` 的 `case` 带 `if (processMsg.ActorId != ActorId)`
/// ⇒ **移动者自己不入帧**；而 `RM_PUSH/RM_RUSH/RM_RUSHKUNG` 的复合条件
/// `ActorId != ActorId || wIdent == RM_PUSH || == RM_RUSH || == RM_RUSHKUNG` **恒真** ⇒
/// **移动者自己也收**（宿主用带自身的广播，见 `main.rs::broadcast_ref_msg_with_self`）。
///
/// 不在此列的 ident 返回 `None`（不猜）。
#[must_use]
pub fn move_frame(ident: u16, target: i32, x: u16, y: u16, d: &ActorDisplay) -> Option<Vec<u8>> {
    match ident {
        mir2_protocol::messages::SM_WALK
        | mir2_protocol::messages::SM_RUN
        | mir2_protocol::messages::SM_HORSERUN
        | mir2_protocol::messages::SM_BACKSTEP
        | mir2_protocol::messages::SM_RUSH
        | mir2_protocol::messages::SM_RUSHKUNG => Some(smframe::sm_move(
            ident,
            target,
            x,
            y,
            d.dir,
            d.light,
            &char_desc(d.feature, d.status),
        )),
        _ => None,
    }
}

/// oracle 的坐标 u16 线缆写法（`i16` 位模式回填；与 `main.rs::wire_coord` 同口径，本模块自持一份
/// 以免 lib 依赖 bin）。
fn wire_coord(v: i32) -> u16 {
    u16::from_ne_bytes(i16::try_from(v).unwrap_or(0).to_ne_bytes())
}

/// **出站接线片 W2**：世界 [`mir2_world::action::PushEvent`]（`RM_PUSH`）→ `SM_BACKSTEP(9)`。
///
/// oracle `PlayObject.Message.cs:1533-1545`：`MakeMessage(SM_BACKSTEP, ActorId, nParam1, nParam2,
/// MakeWord((ushort)processMsg.wParam /*= nBackDir*/, baseObject.Light))` + `Encode(8B CharDesc)`。
/// 即帧方向 = 事件的 `nBackDir`（**不是**角色 `Dir`）⇒ 这里把 `ev.dir` 写进 `d` 的克隆再走
/// [`move_frame`]。
#[must_use]
pub fn backstep_frame(ev: &mir2_world::PushEvent, d: &ActorDisplay) -> Option<Vec<u8>> {
    let mut framed = d.clone();
    framed.dir = ev.dir;
    move_frame(
        mir2_protocol::messages::SM_BACKSTEP,
        ev.actor_id,
        wire_coord(ev.x),
        wire_coord(ev.y),
        &framed,
    )
}

/// **出站接线片 W2**：世界 [`mir2_world::action::RushEvent`]（`RM_RUSH`/`RM_RUSHKUNG`）→
/// `SM_RUSH(6)`/`SM_RUSHKUNG(7)`。
///
/// oracle `PlayObject.Message.cs:1536-1542`：两条同形
/// `MakeMessage(SM_RUSH|SM_RUSHKUNG, ActorId, nParam1, nParam2, MakeWord((ushort)processMsg.wParam
/// /*= 事件方向*/, baseObject.Light))` + `Encode(8B CharDesc)`（`Msg` 恒空 ⇒ 无名尾）。ident 由
/// [`mir2_world::action::RushEvent::ident`] 给出（`Rush` → 6、`RushKung` → 7）。
#[must_use]
pub fn rush_frame(ev: &mir2_world::action::RushEvent, d: &ActorDisplay) -> Option<Vec<u8>> {
    let mut framed = d.clone();
    framed.dir = ev.dir;
    move_frame(
        ev.ident(),
        ev.actor_id,
        wire_coord(ev.x),
        wire_coord(ev.y),
        &framed,
    )
}

/// **出站接线片 W2**：世界 [`mir2_world::action::ButchEvent`]（`RM_BUTCH`）→ `SM_BUTCH(637)`。
///
/// oracle `PlayObject.Message.cs:1781-1787`：`if (processMsg.ActorId != 0) ClientMsg =
/// MakeMessage(SM_BUTCH, processMsg.ActorId, nParam1 /*=CurrX*/, nParam2 /*=CurrY*/,
/// processMsg.wParam /*=Dir*/); SendSocket(ClientMsg);`——**无尾**（与 `sm_action_hit` 的
/// `SM_HIT` 族同形：头 `Series = Dir`、`gm_data_body(h, &[])`）。`ActorId != 0` 门由世界事件
/// 保证（事件只对在世界里的 actor 产生，`actor_id` 必 `> 0`）。
#[must_use]
pub fn butch_frame(ev: &mir2_world::action::ButchEvent) -> Vec<u8> {
    smframe::sm_action_hit(
        mir2_protocol::messages::SM_BUTCH,
        ev.actor_id,
        wire_coord(ev.x),
        wire_coord(ev.y),
        ev.dir,
    )
}

/// **出站接线片 W2**：`SM_DEATH(32)`（**出现帧**支，`PlayObject.cs:3235` 的
/// `SendMsg(baseObject, RM_DEATH, Dir, CurrX, CurrY, 0)`，`nParam3 == 0` 走
/// `PlayObject.Message.cs:1644` 的 **else** 支）。
///
/// 头 = `MakeMessage(SM_DEATH, ActorId, nParam1 /*=CurrX*/, nParam2 /*=CurrY*/, wParam /*=Dir*/)`；
/// 尾 = `EncodePacket(CharDesc)`（`Feature`/`Status`）。**与 `SM_NOWDEATH(34)` 是两条路径**：
/// `RM_DEATH(nParam3=1)` 来自 `BaseObject.Die()`（`BaseObject.Base.cs:351`，本仓已有发射点）；
/// 本条只用于**观察者首次看见一个已死对象**（AOI `VisibleFlag.Show` 支）。
#[must_use]
pub fn death_frame(actor_id: i32, x: u16, y: u16, dir: u8, d: &ActorDisplay) -> Vec<u8> {
    smframe::sm_death(
        mir2_protocol::messages::SM_DEATH,
        actor_id,
        x,
        y,
        dir,
        &char_desc(d.feature, d.status),
    )
}

/// **出站接线片 W2**：AOI **出现帧**的出现对象分派（`PlayObject.Run()` 的
/// `VisibleFlag.Show` 支，`src/M2Server/Player/PlayObject.cs:3231-3243`）：
///
/// ```text
/// if (baseObject.Death) { SM_SKELETON(33) 或 SM_DEATH(32) }
/// else                  { SM_TURN + SM_FEATURECHANGED }   // = turn_pair
/// ```
///
/// `dead == true` ⇒ 走 [`death_frame`]（`SM_DEATH`）。**`Skeleton` 支**（`SM_SKELETON(33)`）不在本片：
/// `SM_SKELETON` 无编码器（属 R2 红名单 A 类）且 `mir2_world::Entity` 无 `skeleton` 字段 ⇒
/// 已死即按 `SM_DEATH` 发（登记，见 rationale）。
///
/// 红检：把 `dead` 支改成 `turn_pair`（恒发 `SM_TURN`）⇒ 死对象出现帧 ident 断言必红。
#[must_use]
pub fn appear_frames(target: i32, x: u16, y: u16, d: &ActorDisplay, dead: bool) -> Vec<Vec<u8>> {
    if dead {
        vec![death_frame(target, x, y, d.dir, d)]
    } else {
        turn_pair(target, x, y, d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    fn between<'a>(s: &'a str, a: &str, b: &str) -> &'a str {
        let i = s.find(a).expect("缺少左标记") + a.len();
        let j = s[i..].find(b).expect("缺少右标记") + i;
        &s[i..j]
    }

    /// 从 M0 金标准里取一条 `port=7200/s2c` 的帧（返回 `(ident, recog, param, tag, series, wire)`）。
    fn golden_line(needle: &str) -> (u16, i32, u16, u16, u16, Vec<u8>) {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tests/golden/session-20261010-c3-hook/capture.jsonl"
        );
        let capture =
            std::fs::read_to_string(path).unwrap_or_else(|e| panic!("金标准不存在：{path}（{e}）"));
        for line in capture.lines() {
            if !line.contains("\"dir\":\"s2c\"")
                || !line.contains("\"port\":7200")
                || !line.contains(needle)
            {
                continue;
            }
            let raw = hex(between(line, "\"raw_hex\":\"", "\""));
            let ident: u16 = between(line, "\"ident\":", ",").parse().unwrap();
            let recog: i32 = between(line, "\"recog\":", ",").parse().unwrap();
            let param: u16 = between(line, "\"param\":", ",").parse().unwrap();
            let tag: u16 = between(line, "\"tag\":", ",").parse().unwrap();
            let series: u16 = between(line, "\"series\":", ",").parse().unwrap();
            return (ident, recog, param, tag, series, raw);
        }
        panic!("金标准里没有 7200/s2c 帧：{needle}");
    }

    fn split_wire(raw: &[u8]) -> ([u8; 12], Vec<u8>) {
        assert_eq!(raw[0], b'#');
        assert_eq!(raw[raw.len() - 1], b'!');
        let head: [u8; 12] = mir2_protocol::edcode::decode(&raw[1..17])
            .try_into()
            .expect("头解码必须是 12B");
        (head, raw[17..raw.len() - 1].to_vec())
    }

    /// **本格的判据**：`turn_pair` 必须逐字节重建金标准里那一对
    /// `SM_TURN`(ident 10) + `SM_FEATURECHANGED`(ident 41)（同 `Recog`）。
    ///
    /// 输入全部**从金标准的真字节回读**（不按公式重推）：x/y/dir/light 取 `SM_TURN` 头，
    /// `feature`/`feature_ex` 取 `SM_FEATURECHANGED` 头，`CharDesc`/`ShowName` 取 `SM_TURN` 尾解码。
    ///
    /// 红检：把 `turn_pair` 的 `char_desc[4..8]`（Status）改成 `0xDEADBEEF`，或把
    /// `sm_featurechanged` 的 `lo/hi` 互换 ⇒ 本用例必红。
    #[test]
    fn turn_pair_rebuilds_golden_sm_turn_and_featurechanged() {
        let (ident, recog, x, y, series, turn_wire) = golden_line("\"ident\":10,");
        assert_eq!(ident, 10);
        let (_turn_head, turn_tail) = split_wire(&turn_wire);
        // SM_TURN 尾 = Encode(8B CharDesc)（11 字符）+ Encode(msg)（长度随名字）。
        let char_desc = mir2_protocol::edcode::decode(&turn_tail[..11]);
        assert_eq!(char_desc.len(), 8, "CharDesc 必须是 8B");
        let msg = mir2_protocol::edcode::decode(&turn_tail[11..]);
        // msg = "ShowName/Color"；ShowName 是 GBK 字节、Color 是十进制 ASCII。
        let slash = msg.iter().rposition(|b| *b == b'/').expect("msg 含 '/'");
        let show_name = msg[..slash].to_vec();
        let color: u8 = std::str::from_utf8(&msg[slash + 1..])
            .unwrap()
            .parse()
            .expect("颜色是十进制");

        // 同 Recog 的 SM_FEATURECHANGED（紧随其后：金标准 seq 30 -> 31）。
        // 捕获 JSON 的字段序是 `recog` 在 `ident` 前（见 `capture.jsonl`）。
        let feat_needle = format!("\"recog\":{recog},\"ident\":41,");
        let (fident, frecog, fparam, ftag, fseries, feat_wire) = golden_line(&feat_needle);
        assert_eq!(fident, 41);
        assert_eq!(frecog, recog, "配对帧的 Recog 必须相同");
        let feature = i32::from(fparam) | (i32::from(ftag) << 16);

        let display = ActorDisplay {
            dir: u8::try_from(series & 0xFF).unwrap(),
            light: u8::try_from(series >> 8).unwrap(),
            feature,
            feature_ex: fseries,
            status: i32::from_le_bytes(char_desc[4..8].try_into().unwrap()),
            name_color: color,
            show_name,
        };
        let built = turn_pair(recog, x, y, &display);
        assert_eq!(built.len(), 2);
        assert_eq!(
            smframe::client_frame(&built[0]),
            turn_wire,
            "SM_TURN 必须逐字节等于金标准 ident=10 那条"
        );
        assert_eq!(
            smframe::client_frame(&built[1]),
            feat_wire,
            "SM_FEATURECHANGED 必须逐字节等于金标准 ident=41 那条"
        );
    }

    /// **F-1 判据（`openmir2-m3-move-wiring` 返工）**：`CM_TURN` 下行走 [`turn_pair_nameless`]，
    /// 即便展示表里有名字，`SM_TURN` 尾也**只能**是 `Encode(8B CharDesc)`，**不得**含
    /// `Encode("ShowName/Color")` 名尾（oracle `PlayObject.Message.cs:1549-1555`：`CM_TURN` 的
    /// `processMsg.Msg` 恒空）。
    ///
    /// 红检：把本函数改回 [`turn_pair`]（或给 `sm_turn` 传非空 `msg`）⇒ 尾长度/内容断言必红。
    #[test]
    fn turn_pair_nameless_has_no_show_name_tail() {
        let d = ActorDisplay::new(2, 3, 111, 7, 0, 255, b"mover".to_vec());
        let pair = turn_pair_nameless(42, 11, 10, &d);
        assert_eq!(pair.len(), 2, "RM_TURN 支 = SM_TURN + SM_FEATURECHANGED");
        let desc = char_desc(d.feature, d.status);
        assert_eq!(
            &pair[0][12..],
            &mir2_protocol::edcode::encode(&desc),
            "CM_TURN 的 SM_TURN 尾必须只有 Encode(8B CharDesc)，无名尾"
        );
        assert_eq!(pair[0].len(), 12 + 11, "尾 = Encode(8B) = 11 字符");
        assert!(
            !pair[0].windows(d.show_name.len()).any(|w| w == d.show_name),
            "CM_TURN 的 SM_TURN 不得把 GetShowName() 拼进尾"
        );
        assert_eq!(
            u16::from_le_bytes([pair[1][4], pair[1][5]]),
            mir2_protocol::messages::SM_FEATURECHANGED
        );
    }

    /// **`openmir2-m3-move-wiring` ③（SM_WALK 的逐字节判据）**：`move_frame` 必须重建 M0 金标准里
    /// 的 `SM_WALK`(ident 11) —— 输入全部从该帧真字节回读：`x`/`y` 取头 `Param`/`Tag`，
    /// `Dir`/`Light` 取 `Series` 两个字节，`Feature`/`Status` 取尾 `Encode(8B CharDesc)` 解码。
    ///
    /// 红检：把 `sm_move` 的尾从 `Encode(char_desc)` 改成原样拼接（或把 `Series` 改成 `MakeWord(Light,
    /// Dir)`）⇒ 本用例必红。
    #[test]
    fn move_frame_rebuilds_golden_sm_walk() {
        let (ident, recog, x, y, series, wire) = golden_line("\"ident\":11,");
        assert_eq!(ident, mir2_protocol::messages::SM_WALK);
        let (_head, tail) = split_wire(&wire);
        let desc = mir2_protocol::edcode::decode(&tail);
        assert_eq!(desc.len(), 8, "SM_WALK 尾必须是 Encode(8B CharDesc)");
        let d = ActorDisplay {
            dir: u8::try_from(series & 0xFF).unwrap(),
            light: u8::try_from(series >> 8).unwrap(),
            feature: i32::from_le_bytes(desc[0..4].try_into().unwrap()),
            feature_ex: 0,
            status: i32::from_le_bytes(desc[4..8].try_into().unwrap()),
            name_color: 255,
            show_name: Vec::new(),
        };
        let built = move_frame(mir2_protocol::messages::SM_WALK, recog, x, y, &d)
            .expect("SM_WALK 必须有编码器");
        assert_eq!(
            smframe::client_frame(&built),
            wire,
            "SM_WALK 必须逐字节等于金标准 ident=11 那条"
        );
        // 不猜：非移动 ident 返回 None。
        assert!(move_frame(mir2_protocol::messages::SM_TURN, recog, x, y, &d).is_none());
    }

    /// `GetMyLight` 的三条分支（`PlayObject.cs:5404-5430`）：等级 ≥50 记 1；
    /// 装备 `Index>0 && Dura>0` 取模板 `Light` 最大值；`Dura==0` 的槽不计。
    #[test]
    fn my_light_follows_get_my_light() {
        use crate::ability::StdItemCatalog;
        use mir2_data::StdItem;
        let mut catalog = StdItemCatalog::new();
        catalog.insert(
            10,
            StdItem {
                light: 7,
                ..StdItem::default()
            },
        );
        catalog.insert(
            11,
            StdItem {
                light: 3,
                ..StdItem::default()
            },
        );
        assert_eq!(my_light(1, &[(10, 5)], &catalog), 7);
        assert_eq!(my_light(1, &[(10, 0)], &catalog), 0, "Dura==0 的槽不计");
        assert_eq!(my_light(1, &[(10, 5), (11, 5)], &catalog), 7, "取最大值");
        assert_eq!(my_light(60, &[], &catalog), 1, "等级 ≥50 记 1");
        assert_eq!(my_light(60, &[(10, 5)], &catalog), 7, "装备更大者胜");
        assert_eq!(
            my_light(1, &[(999, 5)], &catalog),
            0,
            "GetStdItem==null 不计"
        );
    }

    /// `FilterShowName`（`M2Share.cs:571`）：首个 ASCII 数字或 `-` 处截断；无则原样。
    ///
    /// 红检：把 `find` 的谓词改成只找 `'-'`（或改成 `rfind`）⇒ 数字用例必红。
    #[test]
    fn filter_show_name_truncates_at_first_digit_or_dash() {
        assert_eq!(filter_show_name("稻草人"), "稻草人");
        assert_eq!(filter_show_name("稻草人2"), "稻草人");
        assert_eq!(filter_show_name("a-1"), "a");
        assert_eq!(filter_show_name("1ab"), "");
        assert_eq!(filter_show_name("-x"), "");
        assert_eq!(filter_show_name(""), "");
    }

    /// 怪物的 [`monster_display`] 逐字段对齐 oracle：
    /// `feature = MakeLong(MakeWord(RaceImg, LoByte(MP)), Appr)`、`Light/FeatureEx/Status = 0`、
    /// `NameColor = 255`、`ShowName = FilterShowName(ChrName)` 的 GBK 字节。
    ///
    /// 红检：把 `MONSTER_NAME_COLOR` 改成 `0`、把 `feature` 换成 `MakeHumanFeature`、
    /// 或漏掉 `LoByte`（直接用整 `mp`）⇒ 本用例必红。
    #[test]
    fn monster_display_matches_oracle_fields() {
        let info = MonsterInfo {
            name: "稻草人2".to_string(),
            race: 51,
            race_img: 0x0A,
            appr: 0x3344,
            mp: 0x2211,
            ..MonsterInfo::default()
        };
        let d = monster_display(5, &info);
        assert_eq!(d.dir, 5, "Dir 取生成/当前朝向");
        assert_eq!(d.light, 0, "BaseObject.Light 缺省 = 0");
        assert_eq!(
            d.feature, 0x3344_110A_u32 as i32,
            "MakeLong(MakeWord(raceImg, LoByte(mp)), appr)"
        );
        assert_eq!(d.feature_ex, 0, "BaseObject.GetFeatureEx() = 0");
        assert_eq!(d.status, 0, "怪物无 buff ⇒ GetCharStatus() = 0");
        assert_eq!(d.name_color, MONSTER_NAME_COLOR);
        assert_eq!(
            d.show_name,
            gbk("稻草人"),
            "ShowName = FilterShowName(ChrName) 的 GBK 字节（在 '2' 处截断）"
        );
        // 反证：漏掉 LoByte（用整 mp 的 0x22）会得到不同的位。
        assert_ne!(d.feature, make_monster_feature(0x0A, 0x22, 0x3344));
    }

    /// 怪物的 AOI **出现帧**逐字节：`SM_TURN`(ident 10) 的
    /// `Series = MakeWord(Dir, Light)`、尾 = `Encode(8B CharDesc) ++ Encode("ShowName/255")`；
    /// 紧随的 `SM_FEATURECHANGED`(ident 41) 的 `Param/Tag = Lo/HiWord(feature)`、`Series = 0`。
    ///
    /// 红检：把 `turn_pair` 的名尾去掉、把 `light` 换成非 0、或把 `feature` 的 Lo/Hi 互换 ⇒ 必红。
    #[test]
    fn monster_aoi_turn_pair_bytes() {
        use crate::feature::{hi_word, lo_word, make_word};
        let info = MonsterInfo {
            name: "稻草人2".to_string(),
            race: 51,
            race_img: 0x0A,
            appr: 0x3344,
            mp: 0x2211,
            ..MonsterInfo::default()
        };
        let d = monster_display(5, &info);
        let pair = turn_pair(9001, 18, 20, &d);
        assert_eq!(pair.len(), 2, "出现帧 = SM_TURN + SM_FEATURECHANGED");

        let turn = &pair[0];
        assert_eq!(i32::from_le_bytes(turn[0..4].try_into().unwrap()), 9001);
        assert_eq!(
            u16::from_le_bytes([turn[4], turn[5]]),
            mir2_protocol::messages::SM_TURN
        );
        assert_eq!(u16::from_le_bytes([turn[6], turn[7]]), 18);
        assert_eq!(u16::from_le_bytes([turn[8], turn[9]]), 20);
        assert_eq!(
            u16::from_le_bytes([turn[10], turn[11]]),
            make_word(5, 0),
            "Series = MakeWord(Dir, Light)，怪物 Light=0"
        );
        let desc = mir2_protocol::edcode::decode(&turn[12..23]);
        assert_eq!(
            desc,
            char_desc(d.feature, d.status).to_vec(),
            "尾首 = Encode(8B CharDesc(feature, status))"
        );
        let mut expected_msg = gbk("稻草人");
        expected_msg.extend_from_slice(b"/255");
        assert_eq!(
            mir2_protocol::edcode::decode(&turn[23..]),
            expected_msg,
            "名尾 = Encode(\"ShowName/NameColor\")"
        );

        let feat = &pair[1];
        assert_eq!(
            u16::from_le_bytes([feat[4], feat[5]]),
            mir2_protocol::messages::SM_FEATURECHANGED
        );
        assert_eq!(u16::from_le_bytes([feat[6], feat[7]]), lo_word(d.feature));
        assert_eq!(u16::from_le_bytes([feat[8], feat[9]]), hi_word(d.feature));
        assert_eq!(
            u16::from_le_bytes([feat[10], feat[11]]),
            0,
            "GetFeatureEx() = 0"
        );
    }

    /// **⑨-2 残余（46 分身）**：`clone_display` 从主人 `feature` 反解 Gender/Hair，并按
    /// `PlayCloneObject` 的缺省 Dress/Weapon=0 重算 feature。
    ///
    /// 红检：把 `hair` 的位移从 `>> 1` 改成 `& 1`（或把 gender 取 byte2）⇒ feature 断言必红。
    #[test]
    fn clone_display_copies_gender_and_hair_only() {
        // 主人：gender=1、weapon=2、dress=3、hair=5。
        let caster = ActorDisplay::new(
            4,
            0,
            make_human_feature(0, 3 * 2 + 1, 1 + 2 * 2, 5 * 2 + 1),
            0,
            0,
            PLAYER_NAME_COLOR,
            gbk("caster"),
        );
        let d = clone_display(6, &caster);
        assert_eq!(d.dir, 6);
        assert_eq!(d.light, 0);
        assert_eq!(d.status, 0);
        assert_eq!(d.name_color, PLAYER_NAME_COLOR);
        assert_eq!(d.show_name, gbk(mir2_world::CLONE_NAME));
        // 克隆体 Dress=0/Weapon=0 ⇒ feature = MakeHumanFeature(0, gender, gender, hair*2+gender)。
        assert_eq!(
            d.feature,
            make_human_feature(0, 1, 1, 5 * 2 + 1),
            "Gender/Hair 照抄、Dress/Weapon 归 0"
        );
    }

    // ---- 出站接线片 W2：SM_BACKSTEP/SM_RUSH/SM_RUSHKUNG/SM_BUTCH/SM_DEATH 的帧面 --------------

    /// 测试用展示表项：`dir=99`（一个**不会**被事件方向覆盖的哨兵值）、`light=7`、
    /// `feature=0x01020304`、`status=0x05060708`。`dir=99` 让「方向取自事件」的断言有区分度。
    fn w2_display() -> ActorDisplay {
        ActorDisplay::new(99, 7, 0x0102_0304, 0, 0x0506_0708, 255, b"hero".to_vec())
    }

    /// `SM_BACKSTEP(9)`（`RM_PUSH`）逐字节：12B 头手算
    /// `recog=9001 → 29 23 00 00`、`ident=9 → 09 00`、`param=x=18 → 12 00`、`tag=y=20 → 14 00`、
    /// `series=MakeWord(nBackDir=5, Light=7) → 05 07`；尾 = `Encode(8B CharDesc)`。
    ///
    /// 红检（见 [`redcheck_backstep_direction_must_come_from_event`]）。
    #[test]
    fn backstep_frame_bytes_match_oracle_layout() {
        let d = w2_display();
        let ev = mir2_world::PushEvent {
            actor_id: 9001,
            dir: 5,
            x: 18,
            y: 20,
        };
        let body = backstep_frame(&ev, &d).expect("SM_BACKSTEP 必须有编码器");
        assert_eq!(
            &body[..12],
            &[0x29, 0x23, 0x00, 0x00, 0x09, 0x00, 0x12, 0x00, 0x14, 0x00, 0x05, 0x07],
            "12B 命令头逐字节（recog/ident/param/tag/series）"
        );
        assert_eq!(
            &body[12..],
            &mir2_protocol::edcode::encode(&char_desc(d.feature, d.status)),
            "尾 = Encode(8B CharDesc)，无名尾"
        );
        assert_eq!(body.len(), 12 + 11, "尾 = Encode(8B) = 11 字符");
    }

    /// 变异同义牙：`SM_BACKSTEP` 的帧方向**必须**取自事件的 `nBackDir`，与展示表 `dir` 无关。
    /// 用「展示表 dir=99、事件 dir=5」与「展示表 dir=5、事件 dir=5」两组输入 ⇒ 同字节；
    /// 「事件 dir=99」⇒ 字节**必不同**（若实现误用 `d.dir`，第一组断言即红）。
    #[test]
    fn redcheck_backstep_direction_must_come_from_event() {
        let ev5 = mir2_world::PushEvent {
            actor_id: 9001,
            dir: 5,
            x: 18,
            y: 20,
        };
        let sentinel = w2_display(); // dir = 99
        let same = ActorDisplay {
            dir: 5,
            ..w2_display()
        };
        let a = backstep_frame(&ev5, &sentinel).unwrap();
        let b = backstep_frame(&ev5, &same).unwrap();
        assert_eq!(a, b, "方向只由事件决定，与展示表 dir 无关");
        let ev99 = mir2_world::PushEvent { dir: 99, ..ev5 };
        assert_ne!(
            &backstep_frame(&ev99, &sentinel).unwrap()[..12],
            &a[..12],
            "事件方向变 ⇒ series 必变"
        );
    }

    /// `SM_RUSH(6)` / `SM_RUSHKUNG(7)` 逐字节：ident 由事件的 `frame` 决定；`series = MakeWord(nDir, Light)`。
    ///
    /// 红检（见 [`redcheck_rushkung_ident_distinct_from_rush`]）。
    #[test]
    fn rush_frame_bytes_by_frame_kind() {
        use mir2_world::{RushEvent, RushFrame};
        let d = w2_display(); // dir=99 sentinel、light=7
        let rush = RushEvent {
            actor_id: 9001,
            frame: RushFrame::Rush,
            dir: 2,
            x: 30,
            y: 31,
        };
        let body = rush_frame(&rush, &d).unwrap();
        assert_eq!(
            &body[..12],
            &[0x29, 0x23, 0x00, 0x00, 0x06, 0x00, 0x1E, 0x00, 0x1F, 0x00, 0x02, 0x07],
            "SM_RUSH(6)：ident=06 00、series=MakeWord(2,7)=02 07"
        );

        let kung = RushEvent {
            frame: RushFrame::RushKung,
            ..rush
        };
        let body2 = rush_frame(&kung, &d).unwrap();
        assert_eq!(
            &body2[..12],
            &[0x29, 0x23, 0x00, 0x00, 0x07, 0x00, 0x1E, 0x00, 0x1F, 0x00, 0x02, 0x07],
            "SM_RUSHKUNG(7)：ident=07 00，其余同形"
        );
        assert_eq!(kung.ident(), mir2_protocol::messages::SM_RUSHKUNG);
    }

    /// 变异同义牙：`Rush` 与 `RushKung` 的 ident **必须**不同（6 vs 7）——若实现把两族并成一条
    /// ident，本断言必红。
    #[test]
    fn redcheck_rushkung_ident_distinct_from_rush() {
        use mir2_world::{RushEvent, RushFrame};
        let d = w2_display();
        let ev = RushEvent {
            actor_id: 9001,
            frame: RushFrame::Rush,
            dir: 2,
            x: 30,
            y: 31,
        };
        let a = rush_frame(&ev, &d).unwrap();
        let b = rush_frame(
            &RushEvent {
                frame: RushFrame::RushKung,
                ..ev
            },
            &d,
        )
        .unwrap();
        assert_ne!(&a[4..6], &b[4..6], "6 vs 7 必须不同");
    }

    /// `SM_BUTCH(637)` 逐字节：`MakeMessage(SM_BUTCH, ActorId, CurrX, CurrY, Dir)` **无尾**
    /// （`series = Dir`，不叠 Light）。
    #[test]
    fn butch_frame_bytes_no_tail() {
        let ev = mir2_world::ButchEvent {
            actor_id: 9001,
            dir: 3,
            x: 18,
            y: 20,
        };
        let body = butch_frame(&ev);
        assert_eq!(
            body,
            vec![0x29, 0x23, 0x00, 0x00, 0x7D, 0x02, 0x12, 0x00, 0x14, 0x00, 0x03, 0x00],
            "SM_BUTCH=637(0x027D → 7D 02)、series=Dir=3、无尾"
        );
        assert_eq!(
            u16::from_le_bytes([body[4], body[5]]),
            mir2_protocol::messages::SM_BUTCH
        );
    }

    /// `SM_DEATH(32)` 逐字节（出现帧支）+ [`appear_frames`] 分派：
    /// `dead=true` ⇒ 单帧 `SM_DEATH`（ident=32）；`dead=false` ⇒ `turn_pair`（`SM_TURN`+`SM_FEATURECHANGED`）。
    ///
    /// 红检（见 [`redcheck_appear_dead_uses_sm_death_not_turn`]）。
    #[test]
    fn death_frame_and_appear_frames_dispatch() {
        let d = w2_display();
        let dead = death_frame(9001, 18, 20, 4, &d);
        assert_eq!(
            &dead[..12],
            &[0x29, 0x23, 0x00, 0x00, 0x20, 0x00, 0x12, 0x00, 0x14, 0x00, 0x04, 0x00],
            "SM_DEATH(32=0x20)：ident=20 00、series=Dir=04 00"
        );
        assert_eq!(
            &dead[12..],
            &mir2_protocol::edcode::encode(&char_desc(d.feature, d.status))
        );

        let dead_set = appear_frames(9001, 18, 20, &d, true);
        assert_eq!(dead_set.len(), 1, "已死出现帧 = 单帧 SM_DEATH");
        assert_eq!(
            dead_set[0],
            death_frame(9001, 18, 20, d.dir, &d),
            "出现帧支的 SM_DEATH 用被观察者的 Dir（展示表 dir=99）"
        );
        assert_eq!(
            u16::from_le_bytes([dead_set[0][4], dead_set[0][5]]),
            mir2_protocol::messages::SM_DEATH
        );

        let alive_set = appear_frames(9001, 18, 20, &d, false);
        assert_eq!(
            alive_set.len(),
            2,
            "存活出现帧 = SM_TURN + SM_FEATURECHANGED"
        );
        assert_eq!(
            u16::from_le_bytes([alive_set[0][4], alive_set[0][5]]),
            mir2_protocol::messages::SM_TURN
        );
    }

    /// 变异同义牙：已死对象的出现帧**必须**是 `SM_DEATH(32)`，不得退化成 `SM_TURN`。
    #[test]
    fn redcheck_appear_dead_uses_sm_death_not_turn() {
        let d = w2_display();
        let dead = appear_frames(9001, 18, 20, &d, true);
        let alive = appear_frames(9001, 18, 20, &d, false);
        assert_ne!(
            u16::from_le_bytes([dead[0][4], dead[0][5]]),
            u16::from_le_bytes([alive[0][4], alive[0][5]]),
            "死/活出现帧 ident 必不同（32 vs 10）"
        );
        assert_eq!(dead.len(), 1);
        assert_ne!(dead.len(), alive.len());
    }
}
