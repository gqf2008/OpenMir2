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
use crate::smframe;

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

/// `RM_WALK`/`RM_RUN`/`RM_HORSERUN` 支产出的**单帧**（`PlayObject.Message.cs:1403-1432`）。
///
/// 收件人口径与 `turn_pair` 同：oracle 三个 `case` 都带 `if (processMsg.ActorId != ActorId)`
/// ⇒ **只发给附近观察者，移动者自己不入帧**（自己那条连接靠 `+GD` ack）。
/// `ident` 非 `SM_WALK`/`SM_RUN`/`SM_HORSERUN` 时返回 `None`（不猜）。
#[must_use]
pub fn move_frame(ident: u16, target: i32, x: u16, y: u16, d: &ActorDisplay) -> Option<Vec<u8>> {
    match ident {
        mir2_protocol::messages::SM_WALK
        | mir2_protocol::messages::SM_RUN
        | mir2_protocol::messages::SM_HORSERUN => Some(smframe::sm_move(
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
}
