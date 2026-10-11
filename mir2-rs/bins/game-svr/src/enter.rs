//! 入图帧的**世界/配置层输入装配**（`openmir2-m2-enter-world-frames` 的判据②「生产路径」层）。
//!
//! 编码层（`game_svr::smframe::EnterWorldInputs::frames()`）已逐字节对上 M0 金标准；本模块
//! 只做一件事：把 **Envir 真地图的 `Envir.Flag`/`MapDesc`**（`crates/envir`，`openmir2-m2-envir-inputs`）
//! 与 **`Server.conf` 配置层**（同卡）以及**角色记录全字段**（`record.rs`，`openmir2-m2-record-fields`）
//! 装配成 `EnterWorldInputs` 的 `Some` 字段。缺输入的帧**保持 `None`（跳过并登记），绝不猜尾字节**。
//!
//! 逐条对照 oracle（字段序与门都按行号，不按公式重推）：
//! - `766 SM_AREASTATE` ← `RefUserState()`（`PlayObject.Base.cs:270-286`）：`n8 = FightZone|1 | SafeArea|2 | InGuildWarArea|4`；
//! - `54 SM_MAPDESCRIPTION` ← `SendMapDescription()`（`:887-895`）：`nMusicid = Flag.Music ? Flag.MusicId : -1`，尾 = `MapDesc`；
//! - `46 SM_DAYCHANGING` ← `RM_DAYCHANGING`（`PlayObject.Message.cs:1741`）：`Bright = M2Share.GameTime`，`DayBright()`（`Base.cs:244-269`）；
//! - `11029 SM_SERVERCONFIG` ← `SendServerConfig()`（`Base.cs:941-971`）：`SoftVersionDateEx == 0` 时**直接 return**；
//! - `5008 SM_GAMEGOLDNAME` ← `SendGoldInfo(true)`（`Base.cs:921-939`）：同样门 `SoftVersionDateEx == 0`；
//! - `1106 SM_VERSION_FAIL` ← `RM_LOGON`（`Message.cs:1690-1692`）：**无门**，尾恒 `"<<<<<<"`，CRC 来自 `Config`（恒 0）；
//! - `811 SM_ADJUST_BONUS` ← `SendAdjustBonus()`（`Base.cs:1193-1210`）：按 `Job` 取 `Config.BonusAbilofXxx` / 角色 `BonusAbil` / `Config.NakedAbilofXxx`。
//!
//! **仍缺输入（本片登记，不接线）**：
//! - `50 SM_LOGON` / `41 SM_FEATURECHANGED` / `10 SM_TURN` 的 `GetFeatureToLong()`/`CharDesc`
//!   （物品/装备层，`PlayObject.cs:3395`；等 `m3bag` 的 `feature.rs` 接缝）；
//! - `213 SM_ATTACKMODE` 必须与 `50/41` **同组**下发（`SendLogon()` 一次产三条）⇒ 单发半组更坏；
//! - `42 SM_USERNAME` 的 `GetShowName()`、`52 SM_ABILITY`/`752 SM_SUBABILITY` 的 `RecalcAbilitys()` 产出
//!   （world 侧触发点）。
//!
//! `Bright`（`M2Share.GameTime`）是**世界时钟**（`SystemProcessor.cs:98-120` 按真实小时取值），
//! 本骨架还没实现 ⇒ `game_time = None`。`DayBright()` 只在「地图既无 `DAY` 又无 darkness」时才需要它；
//! 现网 `[0 比奇省] DAY` ⇒ `Some(0)`，不受影响。

use mir2_envir::{GameSvrConf, MapInfoFlag, NakedAbility as ConfNakedAbility};

use crate::record::NakedAbility as RecNakedAbility;
use game_svr::smframe::EnterWorldInputs;

/// 装配入图帧额外输入所需的世界/配置层数据（`None` = 该层未就绪，对应帧**跳过并登记**）。
#[derive(Debug, Clone, Copy)]
pub struct WorldFrameInputs<'a> {
    /// 当前地图的 `Envir.Flag`（`MapRegistry::flag_of(map_id)`；无 Envir 时 `None`）。
    pub flag: Option<&'a MapInfoFlag>,
    /// 当前地图的 `Envir.MapDesc`（`MapRegistry::map_desc_of(map_id)`）。
    pub map_desc: Option<&'a str>,
    /// `Server.conf` 的配置层（`GameSvrConf::load`；读不到时 `None`）。
    pub conf: Option<&'a GameSvrConf>,
    /// 角色记录的 `BonusAbil`（帧 811 的 B 段；`CharacterData.bonus_abil`）。
    pub bonus_abil: Option<&'a RecNakedAbility>,
    /// oracle `PlayObject.SoftVersionDateEx`（= `LoadDBInfo.SoftVersionDate`，客户端登录版本）。
    /// `0` ⇒ 帧 11029/5008 不发（`Base.cs:930/943` 的 `if (SoftVersionDateEx == 0) return;`）。
    pub soft_version_date: i32,
    /// 帧 5008 的 `GameGold`（角色记录 `game_gold`）。
    pub game_gold: i32,
    /// 帧 5008 的 `GamePoint`（角色记录 `game_point`）。
    pub game_point: i32,
    /// 帧 811 的 `Job`（角色记录 `job`，`PlayerJob`：0 战士 / 1 法师 / 2 道士）。
    pub job: u8,
    /// 帧 811 的 `BonusPoint`（角色记录 `bonus_point`）。
    pub bonus_point: i32,
    /// oracle `M2Share.GameTime`（世界时钟）。未实现 ⇒ `None`。
    pub game_time: Option<u8>,
}

/// `PlayObject.DayBright()`（`PlayObject.Base.cs:244-269`）的 1:1。
///
/// `Envir.Flag.DayLight` ⇒ 0；`Envir.Flag.boDarkness` ⇒ 1；否则按 `Bright`（= `M2Share.GameTime`）
/// 映射（1→0、3→1、其它→2）。`game_time = None`（世界时钟未实现）⇒ `None`（调用方记 WRN 并用既有占位值）。
#[must_use]
pub fn day_bright(flag: &MapInfoFlag, game_time: Option<u8>) -> Option<u16> {
    if flag.day_light {
        return Some(0);
    }
    if flag.bo_darkness {
        return Some(1);
    }
    game_time.map(|bright| match bright {
        1 => 0,
        3 => 1,
        _ => 2,
    })
}

/// `RefUserState()`（`PlayObject.Base.cs:270-286`）的 `n8`。
///
/// `InGuildWarArea` 在本骨架恒 `false`（无行会战系统）⇒ 只算 `FightZone|1`、`SafeArea|2`。
#[must_use]
pub fn area_state(flag: &MapInfoFlag) -> u16 {
    let mut n8 = 0u16;
    if flag.fight_zone {
        n8 |= 1;
    }
    if flag.safe_area {
        n8 |= 2;
    }
    n8
}

/// `SendMapDescription()`（`PlayObject.Base.cs:887-895`）的 `nMusicid`。
#[must_use]
pub fn music_id(flag: &MapInfoFlag) -> i32 {
    if flag.music {
        flag.music_id
    } else {
        -1
    }
}

/// 线上文本一律按 **GB2312/GBK 字节**（`HUtil32.GetBytes` = `Encoding.GetEncoding("gb2312")`，
/// `EDCode.EncodeString` 编码的就是它）——本层不做 UTF-8 线上转换。
#[must_use]
pub fn gbk(s: &str) -> Vec<u8> {
    let (bytes, _, _) = encoding_rs::GBK.encode(s);
    bytes.into_owned()
}

/// `SendGoldInfo(true)`（`PlayObject.Base.cs:921-939`）的 `sMsg = GameGoldName + '\r' + GamePointName`。
#[must_use]
pub fn game_gold_msg(conf: &GameSvrConf) -> Vec<u8> {
    gbk(&format!(
        "{}\r{}",
        conf.game_gold_name, conf.game_point_name
    ))
}

/// 角色记录 `NakedAbility`（`record.rs`）→ 配置层 `NakedAbility`（同字段，仅类型不同）。
#[must_use]
pub fn conf_naked(n: &RecNakedAbility) -> ConfNakedAbility {
    ConfNakedAbility {
        dc: n.dc,
        mc: n.mc,
        sc: n.sc,
        ac: n.ac,
        mac: n.mac,
        hp: n.hp,
        mp: n.mp,
        hit: n.hit,
        speed: n.speed,
        reserved: n.reserved,
    }
}

/// `SendAdjustBonus()`（`PlayObject.Base.cs:1193-1210`）的三段 `(A, B, C)`（各自 MemoryPack 原文，20B）。
///
/// - `A` = `Config.BonusAbilofXxx`；`B` = 角色 `BonusAbil`；`C` = `Config.NakedAbilofXxx`；
/// - `Job` 不在 `{0,1,2}` 或 `BonusAbil` 缺失 ⇒ `None`（oracle 的 `switch` 无 `default`，此时 `sSendMsg` 为空；
///   本骨架不许猜，跳过并登记）。
#[must_use]
pub fn adjust_bonus_parts(
    conf: &GameSvrConf,
    job: u8,
    bonus_abil: Option<&RecNakedAbility>,
) -> Option<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    let player = bonus_abil?;
    let (a, c) = match job {
        0 => (&conf.bonus_abil_warr, &conf.naked_abil_warr),
        1 => (&conf.bonus_abil_wizard, &conf.naked_abil_wizard),
        2 => (&conf.bonus_abil_taos, &conf.naked_abil_taos),
        _ => return None,
    };
    Some((a.to_bytes(), conf_naked(player).to_bytes(), c.to_bytes()))
}

/// 把 [`WorldFrameInputs`] 装配进 `base`（`base` 里 `notice`/`map_file_name`/`actor_id`/坐标/方向
/// 由 `main.rs` 从网关链路与角色记录填好）。**只填**本片有输入的那几条；其余保持 `None`。
///
/// 返回本步产生的 WRN 文案（调用方逐条 `glog!`，不静默）。
#[must_use]
pub fn apply(base: &mut EnterWorldInputs, wx: &WorldFrameInputs<'_>) -> Vec<String> {
    let mut warns = Vec::new();

    if let Some(flag) = wx.flag {
        // 帧 51 的 `Series` = `DayBright()`（`Message.cs:1681`）。
        match day_bright(flag, wx.game_time) {
            Some(d) => base.day_bright = d,
            None => warns.push(format!(
                "当前地图的 DayBright 依赖 GameTime（世界时钟未实现）⇒ 帧 51 的 Series 沿用占位 {}（登记）",
                base.day_bright
            )),
        }
        // 帧 766 `SM_AREASTATE`。
        base.area_state = Some(area_state(flag));
        // 帧 54 `SM_MAPDESCRIPTION`（尾 = `MapDesc`，GBK 字节）。
        if let Some(desc) = wx.map_desc {
            base.map_description = Some((music_id(flag), gbk(desc)));
        }
    }

    match wx.conf {
        Some(conf) => {
            // 门：`SoftVersionDateEx == 0 ⇒ SendServerConfig/SendGoldInfo 直接 return`。
            if wx.soft_version_date != 0 {
                base.client_conf = Some(conf.client_conf.to_bytes());
                base.game_gold_name = Some((wx.game_gold, wx.game_point, game_gold_msg(conf)));
            }
            // 帧 1106 无门（`RM_LOGON` 无条件发）。
            base.version_fail = Some((conf.client_file1_crc, conf.client_file2_crc));
            // 帧 811（`BonusPoint` 落在命令头 Recog，由 `sm_adjust_bonus` 取出）。
            match adjust_bonus_parts(conf, wx.job, wx.bonus_abil) {
                Some((a, b, c)) => base.adjust_bonus = Some((wx.bonus_point, a, b, c)),
                // oracle 的 `switch (Job)` 无 `default` ⇒ 此时 `sSendMsg` 为空（`SendSocket` 只发 12B 头）；
                // 本骨架不发半帧，跳过并登记（真线上 `Job` 恒 ∈ {0,1,2}）。
                None => warns.push(format!(
                    "帧 811 输入不足（Job={} 不在 {{0,1,2}} 或角色 BonusAbil 缺失）⇒ 跳过（登记）",
                    wx.job
                )),
            }
        }
        None => warns.push(
            "未加载 Server.conf（GameSvrConf）⇒ 帧 11029/1106/5008/811 跳过（登记）".to_string(),
        ),
    }
    warns
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_envir::parse_section_line;
    use mir2_protocol::edcode;
    use mir2_protocol::frame::CommandMessage;

    /// `[0 比奇省] DAY`（现网 `Envir/MapInfo.txt:281`）。
    fn flag_day() -> (MapInfoFlag, String) {
        let e = parse_section_line("[0 比奇省] DAY").expect("section");
        (e.flag, e.desc)
    }

    fn rec_naked() -> RecNakedAbility {
        RecNakedAbility {
            dc: 1,
            mc: 2,
            sc: 3,
            ac: 4,
            mac: 5,
            hp: 6,
            mp: 7,
            hit: 8,
            speed: 9,
            reserved: 10,
        }
    }

    fn idents(frames: &[Vec<u8>]) -> Vec<u16> {
        frames
            .iter()
            .map(|b| CommandMessage::from_bytes(b).expect("12B 头").ident)
            .collect()
    }

    /// 帧 766/54 的输入：`[0 比奇省] DAY` ⇒ `n8=0`、`nMusicid=-1`、尾 = `比奇省` 的 GBK 字节。
    #[test]
    fn area_state_and_map_description_follow_real_map0_flag() {
        let (flag, desc) = flag_day();
        assert_eq!(area_state(&flag), 0, "[0 比奇省] 无 FIGHT/SAFE ⇒ n8=0");
        assert_eq!(music_id(&flag), -1, "无 MUSIC ⇒ nMusicid=-1");

        let mut base = EnterWorldInputs {
            day_bright: 0,
            ..EnterWorldInputs::default()
        };
        let wx = WorldFrameInputs {
            flag: Some(&flag),
            map_desc: Some(&desc),
            conf: None,
            bonus_abil: None,
            soft_version_date: 0,
            game_gold: 0,
            game_point: 0,
            job: 0,
            bonus_point: 0,
            game_time: None,
        };
        let warns = apply(&mut base, &wx);
        assert_eq!(warns.len(), 1, "只该有「未加载 conf」一条：{warns:?}");
        assert!(warns[0].contains("Server.conf"), "{warns:?}");
        assert_eq!(base.area_state, Some(0));
        assert_eq!(
            base.map_description,
            Some((-1, gbk("比奇省"))),
            "帧 54 = (nMusicid=-1, MapDesc 的 GBK 字节)"
        );
        let frames = base.frames();
        assert_eq!(idents(&frames), vec![658, 766, 54], "只装配了 flag 层");
        let cm766 = CommandMessage::from_bytes(&frames[1]).unwrap();
        assert_eq!(cm766.recog, 0, "766 的 n8 落在 Recog");
        let cm54 = CommandMessage::from_bytes(&frames[2]).unwrap();
        assert_eq!(cm54.recog, -1);
        assert_eq!(
            edcode::decode(&frames[2][12..]),
            gbk("比奇省"),
            "54 尾必须与 `Encode(GBK(\"比奇省\"))` 一致"
        );
    }

    /// 红检：`area_state` 的位映射反了（`|1`↔`|2`）⇒ 本用例必红。
    #[test]
    fn area_state_bits_are_fight1_safe2() {
        let both = parse_section_line("[X 战场] FIGHT SAFE DAY").unwrap().flag;
        assert_eq!(area_state(&both), 1 | 2, "FightZone|1、SafeArea|2");
        let only_safe = parse_section_line("[Y 安全区] SAFE").unwrap().flag;
        assert_eq!(area_state(&only_safe), 2, "SafeArea 只置 bit1");
        let only_fight = parse_section_line("[Z 战斗] FIGHT").unwrap().flag;
        assert_eq!(area_state(&only_fight), 1, "FightZone 只置 bit0");
    }

    /// 红检：`music_id` 的 `!music ⇒ -1` 回落被拿掉（恒用 `music_id`）⇒ 本用例必红。
    #[test]
    fn music_id_falls_back_to_minus_one() {
        let (no_music, _) = flag_day();
        assert!(!no_music.music);
        assert_eq!(music_id(&no_music), -1);
        let with_music = parse_section_line("[M 音乐] MUSIC(7)").unwrap().flag;
        assert!(with_music.music);
        assert_eq!(music_id(&with_music), 7);
    }

    /// 帧 11029/5008 的 **`SoftVersionDateEx == 0`** 门（`Base.cs:930/943`）；1106 无门；811 由 `Job` 选模板。
    #[test]
    fn soft_version_gate_and_config_frames() {
        let conf = GameSvrConf {
            game_gold_name: "元宝".to_string(),
            game_point_name: "游戏点".to_string(),
            ..GameSvrConf::from_bytes(b"[Server]\nGatePort=5000\n")
        };
        let rec_bonus = rec_naked();
        let wx = WorldFrameInputs {
            flag: None,
            map_desc: None,
            conf: Some(&conf),
            bonus_abil: Some(&rec_bonus),
            soft_version_date: 0, // 门关
            game_gold: 0,
            game_point: 0,
            job: 0,
            bonus_point: 0,
            game_time: None,
        };
        let mut base = EnterWorldInputs {
            actor_id: 7,
            ..EnterWorldInputs::default()
        };
        let warns = apply(&mut base, &wx);
        assert!(warns.is_empty(), "conf 在、无 flag ⇒ 不该有 WRN：{warns:?}");
        assert!(base.client_conf.is_none(), "门关 ⇒ 11029 不发");
        assert!(base.game_gold_name.is_none(), "门关 ⇒ 5008 不发");
        assert_eq!(base.version_fail, Some((0, 0)), "1106 无门，CRC 恒 0");
        assert_eq!(idents(&base.frames()), vec![658, 1106, 811], "门关序列");

        // 门开 ⇒ 11029/5008 出现，且序为金标准的相对序（11029 在 5008 前、5008 在 1106 前）。
        let wx_open = WorldFrameInputs {
            soft_version_date: 120_020_522,
            ..wx
        };
        let mut base2 = EnterWorldInputs {
            actor_id: 7,
            ..EnterWorldInputs::default()
        };
        let warns2 = apply(&mut base2, &wx_open);
        assert!(
            warns2.is_empty(),
            "conf 在、无 flag ⇒ 不该有 WRN：{warns2:?}"
        );
        let frames = base2.frames();
        assert_eq!(
            idents(&frames),
            vec![658, 11029, 5008, 1106, 811],
            "门开序列"
        );
        let f5008 = frames
            .iter()
            .find(|b| CommandMessage::from_bytes(b).unwrap().ident == 5008)
            .expect("5008");
        assert_eq!(
            edcode::decode(&f5008[12..]),
            gbk("元宝\r游戏点"),
            "5008 尾 = 金币名 + '\\r' + 游戏点名"
        );
    }

    /// 红检：把 `game_gold_msg` 的 `'\r'` 改成 `'\n'` ⇒ 本用例必红。
    #[test]
    fn gold_msg_uses_carriage_return() {
        let conf = GameSvrConf {
            game_gold_name: "元宝".to_string(),
            game_point_name: "成长".to_string(),
            ..GameSvrConf::from_bytes(b"[Server]\nGatePort=5000\n")
        };
        assert_eq!(game_gold_msg(&conf), gbk("元宝\r成长"));
        assert!(game_gold_msg(&conf).contains(&0x0D), "分隔符必须是 0x0D");
        assert!(!game_gold_msg(&conf).contains(&0x0A));
    }

    /// 帧 811 按 `Job` 取模板：0 战士 / 1 法师 / 2 道士；越界/缺输入 ⇒ `None`（不猜）。
    #[test]
    fn adjust_bonus_selects_template_by_job() {
        let conf = GameSvrConf::from_bytes(b"[Server]\nGatePort=5000\n");
        let rec = RecNakedAbility {
            dc: 111,
            ..rec_naked()
        };
        let warr = adjust_bonus_parts(&conf, 0, Some(&rec)).expect("战士");
        assert_eq!(
            warr.0,
            conf.bonus_abil_warr.to_bytes(),
            "A = BonusAbilofWarr"
        );
        assert_eq!(warr.1, conf_naked(&rec).to_bytes(), "B = 角色 BonusAbil");
        assert_eq!(
            warr.2,
            conf.naked_abil_warr.to_bytes(),
            "C = NakedAbilofWarr"
        );
        let wiz = adjust_bonus_parts(&conf, 1, Some(&rec)).expect("法师");
        assert_eq!(wiz.0, conf.bonus_abil_wizard.to_bytes());
        assert_ne!(warr.0, wiz.0, "不同职业的 A 段模板不同");
        let tao = adjust_bonus_parts(&conf, 2, Some(&rec)).expect("道士");
        assert_eq!(tao.0, conf.bonus_abil_taos.to_bytes());
        assert!(
            adjust_bonus_parts(&conf, 9, Some(&rec)).is_none(),
            "越界 Job ⇒ 不猜"
        );
        assert!(
            adjust_bonus_parts(&conf, 0, None).is_none(),
            "缺 BonusAbil ⇒ 不猜"
        );
    }

    /// **生产路径端到端**：真全字段 oracle 记录（`player_data_full_oracle.txt` 的 `stage6`）+
    /// 真 `[0 比奇省] DAY` flag + 默认配置层 ⇒ `apply` + `frames()` 应产出金标准**相对序**的
    /// `658, 51, 11029, 766, 54, 5008, 1106`（缺 50/41/213/42/52/752/46/10 的输入；
    /// 该 oracle 记录的 `Job=3` 不在 `{0,1,2}` ⇒ 811 按登记跳过）。
    #[test]
    fn production_path_emits_world_and_config_frames_for_real_record() {
        let txt = include_str!("../tests/fixtures/player_data_full_oracle.txt");
        let hex = txt
            .lines()
            .find_map(|l| l.strip_prefix("stage6="))
            .expect("stage6 fixture");
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let rec = crate::record::parse_player_data(&bytes).expect("全字段 oracle 记录");
        assert_eq!(rec.data.job, 3, "fixture 的 Job=3（不在 {{0,1,2}}）");

        let (flag, desc) = flag_day();
        let conf = GameSvrConf::from_bytes(b"[Server]\nGatePort=5000\n");
        let mut base = EnterWorldInputs {
            actor_id: 0x7C30_3998,
            cur_x: u16::try_from(rec.data.cur_x).unwrap_or(0),
            cur_y: u16::try_from(rec.data.cur_y).unwrap_or(0),
            dir: rec.data.dir,
            notice: Vec::new(),
            map_file_name: Some(rec.data.cur_map.clone().unwrap_or_default().into_bytes()),
            ..EnterWorldInputs::default()
        };
        let wx = WorldFrameInputs {
            flag: Some(&flag),
            map_desc: Some(&desc),
            conf: Some(&conf),
            bonus_abil: rec.data.bonus_abil.as_ref(),
            soft_version_date: 120_020_522,
            game_gold: rec.data.game_gold,
            game_point: rec.data.game_point,
            job: rec.data.job,
            bonus_point: rec.data.bonus_point,
            game_time: None,
        };
        let warns = apply(&mut base, &wx);
        assert_eq!(warns.len(), 1, "只该有 811 跳过的一条：{warns:?}");
        assert!(warns[0].contains("811"), "{warns:?}");
        let frames = base.frames();
        assert_eq!(
            idents(&frames),
            vec![658, 51, 11029, 766, 54, 5008, 1106],
            "金标准相对序（子序列）"
        );
        // 766 的 n8 来自真 flag；54 的尾 = 真 MapDesc。
        let f766 = &frames[3];
        assert_eq!(CommandMessage::from_bytes(f766).unwrap().recog, 0);
        let f54 = &frames[4];
        let cm54 = CommandMessage::from_bytes(f54).unwrap();
        assert_eq!(cm54.recog, -1);
        assert_eq!(edcode::decode(&f54[12..]), gbk("比奇省"));
        // 5008 的尾 = 金币名（真 `Server.conf` 默认值）。
        let f5008 = &frames[5];
        assert_eq!(edcode::decode(&f5008[12..]), gbk("元宝\r游戏点"));
        // 1106 无尾 + `<<<<<<`。
        let f1106 = &frames[6];
        assert_eq!(&f1106[12..], b"<<<<<<");
    }

    /// 红检：`apply` 的 `soft_version_date == 0` 门或 flag/conf 装配被改坏 ⇒ 上面两个用例必红。
    /// `DayBright()`：`DAY` ⇒ 0、darkness ⇒ 1、否则依赖 `GameTime`（未实现 ⇒ `None` + WRN）。
    #[test]
    fn day_bright_matches_oracle() {
        let (day, _) = flag_day();
        assert_eq!(day_bright(&day, None), Some(0), "DAY ⇒ 0，与 GameTime 无关");
        let dark = parse_section_line("[D 暗] DARK").unwrap().flag;
        assert_eq!(day_bright(&dark, None), Some(1));
        let neutral = parse_section_line("[N 中立] NORANDOMMOVE").unwrap().flag;
        assert!(!neutral.day_light && !neutral.bo_darkness);
        assert_eq!(
            day_bright(&neutral, None),
            None,
            "无 DAY/darkness 且无 GameTime ⇒ 不猜"
        );
        assert_eq!(day_bright(&neutral, Some(1)), Some(0));
        assert_eq!(day_bright(&neutral, Some(3)), Some(1));
        assert_eq!(day_bright(&neutral, Some(2)), Some(2));

        // 有 flag 但 DayBright 依赖 GameTime ⇒ WRN（不静默），且沿用调用方占位值。
        let mut base = EnterWorldInputs {
            day_bright: 7,
            ..EnterWorldInputs::default()
        };
        let wx = WorldFrameInputs {
            flag: Some(&neutral),
            map_desc: None,
            conf: None,
            bonus_abil: None,
            soft_version_date: 0,
            game_gold: 0,
            game_point: 0,
            job: 0,
            bonus_point: 0,
            game_time: None,
        };
        let warns = apply(&mut base, &wx);
        assert_eq!(warns.len(), 2, "DayBright + 未加载 conf 各一条：{warns:?}");
        assert_eq!(base.day_bright, 7, "不猜 ⇒ 沿用调用方给的占位值");
        assert_eq!(base.area_state, Some(0));
    }
}
