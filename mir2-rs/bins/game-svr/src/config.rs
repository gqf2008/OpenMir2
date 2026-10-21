//! GameSvr 的遗留 INI 读取（现网样件 `E:\MirServer\M2GameSvr\Server.conf`）。
//!
//! 本切片只读**网关链路**真正需要的那几个键，键名照现网事实（**不发明**）：
//! `[Server] GateAddr / GatePort`（网关连上来的监听端点；现网 `GateAddr= 127.0.0.1` **值带前导空格**，
//! 所以两边都要 trim）、`CheckBlock`（发包阻塞阈值，`ChannelMessageHandler.cs:74` 用它决定何时回 `GM_RECEIVE_OK`）。
//! 世界尺寸（S1 占位）用 `WorldMaps`：形如 `1000x1000` 或 `1000x1000,200x200`。
//!
//! **`WorldMaps` 是可选键**（owner 裁决，见 `openmir2-m2-gamesvr-host` 卡 comment `272582e1`）：
//! 真 `E:\MirServer\M2GameSvr\Server.conf` 的 `[Server]` 段**没有这个键**（它是 S1 引入的占位），
//! 真地图表在 `[Share] EnvirDir`（现网 `Envir`）下的 `MapInfo.txt`。`WorldMaps` 只在**没有 Envir**
//! （开发/测试）时兜底；**两者都没有必须拒绝启动**（fail-closed），由 `mapwire::placeholder_sizes` 判。
//! 其余键**缺键即报错**（照 M1 验收④的"失败要响"口径），不许静默取默认值。

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// 监听端点 + S1 需要的少量参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateConfig {
    /// `[Server] GateAddr`（现网带前导空格，解析时 trim）。
    pub gate_addr: String,
    /// `[Server] GatePort`（现网 5000）。
    pub gate_port: u16,
    /// `[Server] CheckBlock`（现网 12288）：`ChannelMessageHandler.cs:74` 的阻塞阈值。
    pub check_block: usize,
    /// `[Server] ServerIndex`（oracle `ServerConf.cs:17` 的 `ReadWriteByte`，缺省 0）——
    /// `MakeSaveRcd` 写 `characters.ServerIndex`（`WorldServer.cs:1236`）的来源。
    ///
    /// oracle 是 `byte`（`SystemShare.ServerIndex`，`SystemShare.cs:45`）⇒ 越界/非整数报错。
    pub server_index: u8,
    /// `[Server] TestServer`（`ServerConf.cs:23`：`ReadWriteString("Server","TestServer","FALSE")`
    /// 与 `"TRUE"` 做 **OrdinalIgnoreCase** 比较）。登录 `UserLogon` 的第三条 `SysMsg`
    /// （`StartNoticeMsg`，`PlayObject.cs:1372`）与 `PayMent=3` 覆盖都看它
    /// （`PlayObject.cs:1168-1171`）。
    pub test_server: bool,
    /// `[Server] ServiceMode`（`ServerConf.cs:27`：`ReadWriteString(...).CompareTo("TRUE") == 0`）。
    /// **注意**：`String.CompareTo(string)` 是 **culture-aware** 比较，**不是**
    /// `StringComparison.Ordinal`；只是对 ASCII 大小写仍敏感 ⇒ 部署语义是「只认大写 `TRUE`」
    /// （`true`/`True` 都为 false）。与 `TestServer` 同用于 `PayMent=3` 覆盖
    /// （`PlayObject.cs:1168-1171`）。
    pub service_mode: bool,
    /// S1 占位：世界地图尺寸列表（有 Envir 真地图注册表时被其尺寸取代；无 Envir 时才用）。
    ///
    /// **可选键**：配置里没有 `WorldMaps` ⇒ 空表。空表**不是**错误，但"空表 + 没有 Envir"
    /// 必须 fail-closed（见 `mapwire::placeholder_sizes`）——不要在这里提前拒绝，否则现网
    /// 无 `WorldMaps` 的真配置起不来。
    pub maps: Vec<(i32, i32)>,
    /// `[Share] EnvirDir`（oracle `SystemShare.Config.EnvirDir`，默认 `Envir`）——`MapInfo.txt` 所在目录。
    pub envir_dir: String,
    /// `[Share] MapDir`（oracle `SystemShare.Config.MapDir`，默认 `Map`）——`*.map` 所在目录。
    pub map_dir: String,
    /// `[Share] NoticeDir`（oracle `SystemShare.Config.NoticeDir`，默认 `Notice`）——
    /// `Notice.txt` 所在目录（`openmir2-m2-enter-world-frames` 的 658 `SM_SENDNOTICE` 用它）。
    pub notice_dir: String,
    /// `[Setup] RedMsgFColor`（oracle `GameSvrConf.cs:1540` 缺省 `0xFF`）——`SysMsg(..., Red, ...)`
    /// 的前景色，落在 `SM_SYSMESSAGE` 的 `Param` 低字节。
    ///
    /// 用途：`openmir2-m2-session-lifecycle` 的账号级顶号提示（`TCPNetChannel.KickUser`，
    /// `TCPNetChannel.cs:229/262`）。缺键用 oracle 缺省值（`ServerConf.cs:333` 的 `ReadWriteByte` 口径）。
    pub red_msg_f_color: u8,
    /// `[Setup] RedMsgBColor`（oracle `GameSvrConf.cs:1541` 缺省 `0x38`）——同上，高字节。
    pub red_msg_b_color: u8,
    /// `[Setup] GreenMsgFColor`（oracle `GameSvrConf.cs:1536` 缺省 `0xDB`）——交易 `SysMsg(..., Green, ...)`
    /// 的前景色（`PlayObject.Operate.cs:943/1199` 等）。现网 `=219`。
    pub green_msg_f_color: u8,
    /// `[Setup] GreenMsgBColor`（oracle `GameSvrConf.cs:1537` 缺省 `0xFF`）——同上，背景色。现网 `=255`。
    pub green_msg_b_color: u8,
    /// `[Setup] DisableInSafeZoneFireCross`（oracle `GameSvrConf.cs:1764` 缺省 `false`；
    /// **现网 `Server.conf:854` = 1**）——22 火墙的安全区门（`MagicManager.cs:1066`）。
    pub disable_in_safe_zone_fire_cross: bool,
    /// `[Setup] TryDealTime`（oracle `GameSvrConf.cs:1515` 缺省 `3000`）——
    /// `CM_DEALTRY` 的「稍后再试」时间门（`PlayObject.Operate.cs:922`）。
    pub try_deal_time: i32,
    /// `[Setup] DealOKTime`（oracle `GameSvrConf.cs:1516` 缺省 `1000`）——
    /// `CM_DEALEND` 的时间门（`PlayObject.Operate.cs:1092`）。
    pub deal_ok_time: i32,
    /// `[Setup] CanNotGetBackDeal`（oracle `GameSvrConf.cs:1517` 缺省 `true`）——
    /// 交易栏物品/金币不许取回（`PlayObject.Operate.cs:1003/1051`）。现网 `=1`。
    pub can_not_get_back_deal: bool,
    /// `[Setup] DisableDeal`（oracle `GameSvrConf.cs:1518` 缺省 `false`）——
    /// 关闭交易功能（`PlayObject.Operate.cs:913`）。现网 `=0`。
    pub disable_deal: bool,
    /// `[Setup] HearMsgFColor`（oracle `GameSvrConf.cs:1528` 缺省 `0x00`）——`CM_SAY` 普通说话
    /// 下行 `SM_HEAR` 的前景色（`BaseObject.Base.cs:478` 的 `btHearMsgFColor`）。现网 `=0`。
    pub hear_msg_f_color: u8,
    /// `[Setup] HearMsgBColor`（oracle `GameSvrConf.cs:1529` 缺省 `0xFF`）——同上，背景色。现网 `=255`。
    pub hear_msg_b_color: u8,
    /// `[Setup] WhisperMsgFColor`（oracle `ServerConf.cs:323`，`GameSvrConf.cs:1530` 缺省 `0xFC`）——
    /// 私聊(`/`)下行 `SM_WHISPER` 的前景色（`PlayObject.Chat.cs:57` 的 `btWhisperMsgFColor`）。现网 `=252`。
    pub whisper_msg_f_color: u8,
    /// `[Setup] WhisperMsgBColor`（oracle `ServerConf.cs:324`，`GameSvrConf.cs:1531` 缺省 `0xFF`）——
    /// 同上，背景色。现网 `=255`。
    pub whisper_msg_b_color: u8,
    /// `[Setup] BlueMsgFColor`（oracle `GameSvrConf.cs:1538` 缺省 `0xFF`；现网 `=251`）——`SysMsg(...,
    /// Blue, ...)` 的前景色（`PlayObject.Sale.cs` 寄售提示）。落在 `SM_SYSMESSAGE` 的 `Param` 低字节。
    pub blue_msg_f_color: u8,
    /// `[Setup] BlueMsgBColor`（oracle `GameSvrConf.cs:1539` 缺省 `0xFC`；现网 `=252`）——同上，背景色。
    pub blue_msg_b_color: u8,
    /// `[Setup] GroupMembersMax`（`ServerConf.cs:225` 的 `ReadWriteByte`，`GameSvrConf.cs:1402` 缺省 `10`）。
    /// M3⑦-实现 2：组队状态机的成员上限门（`PlayObject.Operate.cs:855` 的严格大于）。
    pub group_members_max: u8,
    /// `[Setup] GroupMsgFColor`（`ServerConf.cs:337`，`GameSvrConf.cs:1544` 缺省 `0xC4`）。
    pub group_msg_f_color: u8,
    /// `[Setup] GroupMsgBColor`（`ServerConf.cs:338`，`GameSvrConf.cs:1545` 缺省 `0xFF`）。
    pub group_msg_b_color: u8,
    /// `[Setup] CryMsgFColor`（`ServerConf.cs:327`，`GameSvrConf.cs:1534` 缺省 `0x00`）——
    /// `!` 喊话的 `RM_CRY`/`SM_HEAR` 前景色（`PlayObject.Chat.cs:199` 的 `btCryFColor`）。现网 `=0`。
    pub cry_msg_f_color: u8,
    /// `[Setup] CryMsgBColor`（`ServerConf.cs:328`，`GameSvrConf.cs:1535` 缺省 `0x97`）——同上，背景色。现网 `=151`。
    pub cry_msg_b_color: u8,
    /// `[Setup] CanShoutMsgLevel`（`ServerConf.cs:278`，`GameSvrConf.cs:1455` 缺省 `7`）——
    /// `!` 喊话的等级门：`Abil.Level <= CanShoutMsgLevel` ⇒ 拒（`PlayObject.Chat.cs:185`）。现网 `=34`。
    pub can_shout_msg_level: i32,
    /// `[Server] ServerName`（`ServerConf.cs:19` 的 `ReadWriteString("Server","ServerName",…)`；
    /// `GameSvrConf.cs:1233` 缺省 `SKY引擎`）。说词变量 `<$SERVERNAME>` 的取值。
    pub server_name: String,
    /// `[Server] IDSAddr`：登录服务器地址（oracle 是 TcpClient 主动连它，`AuthenticationService.cs:22-34`）。
    pub ids_addr: String,
    /// `[Server] IDSPort`（现网 5600）。
    pub ids_port: u16,
    /// `[Server] DBAddr`：DBSvr 地址（`DataQueryServer.cs:28` 用它做 `SetRemoteIPHost`）。
    pub db_addr: String,
    /// `[Server] DBPort`（现网 6000）。
    pub db_port: u16,
    /// `[DataBase] ConnctionString`（**原拼写照抄**，`ServerConf.cs:15`）——GameSvr 直连 MySQL 读
    /// `stditems` 用的连接串（oracle `MySqlDB.LoadItemsDB` 也是直连，不经过 DBSvr）。
    ///
    /// `None` = 配置里没这个键 ⇒ 兜底用 oracle 的源码缺省
    /// （`GameSvrConf.cs:1921`：`server=127.0.0.1;uid=root;pwd=;database=mir2;`）；两者都解析失败
    /// ⇒ 物品表为空（`GetStdItem` 恒 null ⇒ 帧 50/41 的装备形态计 0，与 oracle 空表分支等价）+ WRN。
    pub db_conn_string: Option<String>,
    /// `[Setup] ShowRankLevelName`（oracle `GameSvrConf.cs:376/1464` 缺省 `false`）。
    ///
    /// 帧 42 `SM_USERNAME` 的展示名走 `GetShowName()`（`PlayObject.cs:3329`）：
    /// `true` ⇒ `Format(RankLevelName, ChrName)`，`false` ⇒ 原名/转生名模板。现网 `=1`。
    pub show_rank_level_name: bool,
    /// `[Setup] HumanMaxGold`（oracle `GameSvrConf.cs:1451` 缺省 `10000000`）——
    /// `ground::PickUpContext.human_max_gold`（`PlayObject.Base.cs:171` 的 `IncGold` 上限）。
    pub human_max_gold: i32,
    /// `[Setup] FloorItemCanPickUpTime`（oracle `GameSvrConf.cs:1488` 缺省 `2*60*1000`）——
    /// `ground::PickUpContext.floor_item_can_pick_up_time`（`PlayObject.Base.cs:67`）。
    pub floor_item_can_pick_up_time: i32,
    /// `[Setup] ControlDropItem`（oracle `GameSvrConf.cs:1703` 缺省 `false`）——
    /// `ground::DropContext.control_drop_item`（`PlayObject.Operate.cs:284`）。
    pub control_drop_item: bool,
    /// `[Setup] InSafeDisableDrop`（oracle `GameSvrConf.cs:1704` 缺省 `false`）——
    /// `ground::DropContext.in_safe_disable_drop`（`PlayObject.Operate.cs:224/254`）。
    pub in_safe_disable_drop: bool,
    /// `[Setup] CanDropPrice`（oracle `GameSvrConf.cs:1706` 缺省 `500`）——
    /// `ground::DropContext.can_drop_price`（`PlayObject.Operate.cs:284` 的 `stdItem.Price <` 门）。
    pub can_drop_price: i32,
    /// `[Setup] CanDropGold`（oracle `GameSvrConf.cs:1705` 缺省 `1000`）——
    /// `inbound::DropGoldInputs.can_drop_gold`（`PlayObject.Operate.cs:229` 的 `nGold < CanDropGold` 门）。
    pub can_drop_gold: i32,
    /// `[Setup] DropItemRage`（oracle `GameSvrConf.cs:1754` 缺省 `3`，`byte`）——
    /// `ground::DropContext.drop_item_rage`（`PlayObject.Operate.cs:291` 的 `_MIN(...,3)` 散布半径）。
    pub drop_item_rage: u8,
    /// `[Setup] DieDropUseItemRate`（oracle `GameSvrConf.cs:768/1714` 缺省 `30`，`int`；
    /// `ServerConf.cs:492`）——玩家死亡 `DropUseItems` 的逐槽骰 `Random(nRate)`
    /// （`PlayObject.Message.cs:2183`，`PvpLevel() <= 2`）。部署件 `=50`。
    pub die_drop_use_item_rate: i32,
    /// `[Setup] DieRedDropUseItemRate`（oracle `GameSvrConf.cs:772/1715` 缺省 `15`）——
    /// 红名（`PvpLevel() > 2`）支。部署件 `=15`。
    pub die_red_drop_use_item_rate: i32,
    /// `[Setup] KillByHumanDropUseItem`（`GameSvrConf.cs:780/1717` 缺省 `false`；
    /// `ServerConf.cs:495`，部署件 `=1`）——`KillTargetTrigger` 第二段对玩家击杀者。
    pub kill_by_human_drop_use_item: bool,
    /// `[Setup] KillByMonstDropUseItem`（`GameSvrConf.cs:784/1718` 缺省 `true`；
    /// `ServerConf.cs:496`，部署件 `=1`）。
    pub kill_by_monst_drop_use_item: bool,
    /// `[Setup] SendRefMsgRange`（oracle `GameSvrConf.cs:499`，缺省 `12`，`byte`；
    /// `ServerConf.cs:280` 的 `ReadWriteInteger("Setup", "SendRefMsgRange", …)`）。
    ///
    /// 用途：`BaseObject.SendRefMsg`（`BaseObject.Message.cs:399-402`）扫描周围
    /// `CurrX/Y ± SendRefMsgRange` 的单元格，向其中的玩家广播 `RM_*`。
    /// `openmir2-m2-gamesvr-host` 的装备/卸下 `FeatureChanged()`（`SM_FEATURECHANGED`）用它。
    /// 缺键用 oracle 缺省 `12`（`GameSvrConf.cs:1503`）。
    pub send_ref_msg_range: u8,
    /// `[Setup] MagicAttackRage`（oracle `GameSvrConf.cs:873/1752` 缺省 `8`，`byte`；
    /// `ServerConf.cs:539` 的 `ReadWriteByte("Setup", "MagicAttackRage", …)`）——
    /// `DoSpell` 施法距离门（`MagicManager.cs:155`）：`|CurrX-nTargetX| > rage` 或 y 同理 ⇒ 直接失败。
    pub magic_attack_rage: u8,
    /// `[Setup] FireBoomRage`（`GameSvrConf.cs:903/1770` 缺省 1，`ServerConf.cs:548`）——
    /// P3-B2 的 23（爆裂火焰）/ 47（地狱烈焰）`MagBigExplosion` 半径。
    pub fire_boom_rage: u8,
    /// `[Setup] SnowWindRange`（`GameSvrConf.cs:904/1771` 缺省 1）—— P3-B2 的 33（冰咆哮）半径。
    pub snow_wind_range: u8,
    /// `[Setup] ElecBlizzardRange`（`GameSvrConf.cs:908/1772` 缺省 2）—— P3-B2 的 24（地狱雷光）半径。
    pub elec_blizzard_range: u8,
    /// `[Setup] MagTurnUndeadLevel`（`GameSvrConf.cs:1773` 缺省 `50`；
    /// `ServerConf.cs:530` 的 `ReadWriteInteger`）—— P3-B3 的 32（圣言术）等级门
    /// `target.Level < Config.MagTurnUndeadLevel`（`MagicManager.cs:842-853`）。
    pub mag_turn_undead_level: i32,
    /// `[Setup] PKFlagTime`（`GameSvrConf.cs:1395` 缺省 `60 * 1000`；
    /// `ServerConf.cs:156` 的 `ReadWriteInteger("Setup", "PKFlagTime", …)`）——
    /// `PlayObject.CheckPkStatus`（`PlayObject.cs:4357-4364`）的 `PvpFlag` 过期门。
    pub pk_flag_time: i32,
    /// `[Setup] MabMabeHitRandRate`（`GameSvrConf.cs:1778` 缺省 100）—— P3-B4 的 50 第三道成功率门。
    pub mab_mabe_hit_rand_rate: i32,
    /// `[Setup] MabMabeHitMinLvLimit`（`GameSvrConf.cs:1779` 缺省 10）。
    pub mab_mabe_hit_min_lv_limit: i32,
    /// `[Setup] MabMabeHitSucessRate`（`GameSvrConf.cs:1780` 缺省 21；键名照 oracle 拼写）。
    pub mab_mabe_hit_sucess_rate: i32,
    /// `[Setup] MabMabeHitMabeTimeRate`（`GameSvrConf.cs:1781` 缺省 20）。
    pub mab_mabe_hit_mabe_time_rate: i32,
    /// `[Setup] AmyOunsulPoint`（`GameSvrConf.cs:1763` 缺省 10）—— 6/48 毒 `point` 分母。
    pub amy_ounsul_point: i32,
    /// `[Setup] SuperRepairPriceRate`（`ServerConf.cs:487`，缺省 `3`）——特殊修理价格倍数
    /// （`Merchant.cs:1573/1619`）。
    pub super_repair_price_rate: i32,
    /// `[Setup] RepairItemDecDura`（`ServerConf.cs:488`，缺省 `30`；现网 `90`）——普通修理掉持久除数
    /// （`Merchant.cs:1656`）。
    pub repair_item_dec_dura: i32,
    /// `[Setup] MakeDurg`（oracle `ServerConf.cs:244` 的 `ReadWriteInteger("Setup","MakeDurg",…)`，
    /// `GameSvrConf.cs:1450` 缺省 `100`）——合成/制药单价
    /// （`Merchant.cs:1010` 面板价、`Merchant.cs:1510/1518` 收费与扣金）。
    pub make_durg_price: i32,
    /// `[Names] BlackStone`（`ServerConf.cs:91`，缺省 `黑铁矿`；`GameSvrConf.cs:1310`）——
    /// 武器升级的材料名（`Merchant.cs:537/408`，提交端**精确 `==`**、`CheckItems` 用
    /// `OrdinalIgnoreCase`）。
    pub black_stone: String,
    /// `[Setup] UpgradeWeaponPrice`（`ServerConf.cs:232`，`ReadWriteInteger`，缺省 `10000`）。
    pub upgrade_weapon_price: i32,
    /// `[Setup] UPgradeWeaponGetBackTime`（`ServerConf.cs:230`，缺省 `60*60*1000` 毫秒；
    /// 部署 `600000`）。注意 oracle 键名大小写照抄（解析时已归一大写）。
    pub upgrade_weapon_get_back_time: i32,
    /// `[Setup] ClearExpireUpgradeWeaponDays`（`ServerConf.cs:231`，`ReadWriteByte`，缺省 `8`；
    /// 部署 `3`）。
    pub clear_expire_upgrade_weapon_days: u8,
    /// `[Setup] UpgradeWeaponDCRate`（`ServerConf.cs:234`，byte，缺省 `100`；部署 `50`）。
    pub upgrade_weapon_dc_rate: u8,
    /// `[Setup] UpgradeWeaponDCTwoPointRate`（缺省 `30`）。
    pub upgrade_weapon_dc_two_point_rate: u8,
    /// `[Setup] UpgradeWeaponDCThreePointRate`（缺省 `200`）。
    pub upgrade_weapon_dc_three_point_rate: u8,
    /// `[Setup] UpgradeWeaponMCRate`（缺省 `100`；部署 `50`）。
    pub upgrade_weapon_mc_rate: u8,
    /// `[Setup] UpgradeWeaponMCTwoPointRate`（缺省 `30`）。
    pub upgrade_weapon_mc_two_point_rate: u8,
    /// `[Setup] UpgradeWeaponMCThreePointRate`（缺省 `200`）。
    pub upgrade_weapon_mc_three_point_rate: u8,
    /// `[Setup] UpgradeWeaponSCRate`（缺省 `100`；部署 `50`）。
    pub upgrade_weapon_sc_rate: u8,
    /// `[Setup] UpgradeWeaponSCTwoPointRate`（缺省 `30`）。
    pub upgrade_weapon_sc_two_point_rate: u8,
    /// `[Setup] UpgradeWeaponSCThreePointRate`（缺省 `200`）。
    pub upgrade_weapon_sc_three_point_rate: u8,
    /// `[Setup] CastleMemberPriceRate`（`ServerConf.cs:250`，缺省 `80`）——城堡成员价百分比
    /// （`Merchant.cs:772`）。
    pub castle_member_price_rate: i32,
    /// `[Setup] CastleGetAllNpcTax`（`ServerConf.cs:260`，缺省 `false`）——所有商人交税
    /// （`Merchant.cs:544/1250/1385/1637` 的 `Config.GetAllNpcTax`）。
    pub castle_get_all_npc_tax: bool,
    /// `[Setup] StartPointSize`（oracle `GameSvrConf.cs:236/1400`，缺省 `2`，`byte`）——
    /// `GetHomeInfo` 的 `Random(StartPointSize)` 上限（`WorldServer.cs:1388-1392`）。本宿主取
    /// `I=0`（见 `homemap` 模块头），该键只用于日志/复核。
    pub start_point_size: u8,
    /// `[Setup] HomeMap`（oracle `GameSvrConf.cs:1374`，缺省 `"0"`）——`StartPointList` 为空时
    /// `GetHomeInfo` 的回落地图（`WorldServer.cs:1403`）。
    pub home_map: String,
    /// `[Setup] HomeX`（oracle `GameSvrConf.cs:1375`，缺省 `289`）。
    pub home_x: i16,
    /// `[Setup] HomeY`（oracle `GameSvrConf.cs:1376`，缺省 `618`）。
    pub home_y: i16,
    /// `[Setup] Candle`（oracle `GameSvrConf.cs:1304`，缺省 `"蜡烛"`）——新角色`IsNewHuman` 首件（`PlayObject.cs:1207`）。
    pub candle: String,
    /// `[Setup] BasicDrug`（`GameSvrConf.cs:1305`，缺省 `"金创药(小量)"`）。
    pub basic_drug: String,
    /// `[Setup] WoodenSword`（`GameSvrConf.cs:1303`，缺省 `"木剑"`）。
    pub wooden_sword: String,
    /// `[Setup] ClothsMan`（`GameSvrConf.cs:1301`，缺省 `"布衣(男)"`）。
    pub cloths_man: String,
    /// `[Setup] ClothsWoman`（`GameSvrConf.cs:1302`，缺省 `"布衣(女)"`）。
    pub cloths_woman: String,
    /// `[Setup] ItemNumber`（oracle `GameSvrConf.cs:1699`，缺省 `0`）——`M2Share.GetItemNumber()` 的计数器初值。
    ///
    /// **本侧只读、不写回（与 oracle 一致）**：oracle 的 60s 定时器确实调
    /// `ServerConf.SaveVariable()`（`src/GameSrv/TimedService.cs:73/76/255-258`），但
    /// `SaveVariable`（`src/Modules/SystemModule/Conf/ServerConf.cs:668-680`）只对 10 个键调
    /// `ConfigFile.WriteInteger`（`src/OpenMir2/Common/ConfigFile.cs:419-434`）——那只是**内存缓存**
    /// `iniCahce` 赋值；真正落盘的 `ConfigFile.Save()`（同文件 `:348-381`）**没有任何调用点落在 `ServerConf` 实例上**
    /// （其它 `ConfigFile` 子类如 GameGate/LoginGate/CastleConf 确有调用点，与本判定无关；`ServerConf` 自身无 `Save()`、
    /// `ServerConf.` 全仓仅 2 处引用且均不落盘）⇒ oracle 跨重启**不保留** `ItemNumber`/`ItemNumberEx` 的累积
    /// （每次启动都从 conf 静态值重来，再各加屏障 A 的 1 抽）。⇒ 本侧同语义：`load()` 只读，不产写。
    /// 判据与可复跑证据见 `docs/itemnumber-persist-writeback-判据-20261017.md`
    /// （卡 `openmir2-itemnumber-persist-writeback`，2026-10-17 判定「前提不成立」）。
    pub item_number: i32,
    /// `[Setup] SaveHumanRcdTime`（oracle `GameSvrConf.cs:423/1484`，缺省 `10*60*1000`，**毫秒**；
    /// `ServerConf.cs:566` 的 `ReadWriteInteger("Setup", "SaveHumanRcdTime", …)`）——
    /// `WorldServer.ProcessPlayObjectData`（`WorldServer.cs:698-703`）的**周期存档**间隔。
    pub save_human_rcd_time: i32,
    /// `[Setup] HealthFillTime`（oracle `GameSvrConf.cs:170/1323`，缺省 `300`；
    /// `ServerConf.cs:152` 的 `ReadWriteInteger("Setup", "HealthFillTime", …)`）——
    /// `BaseObject.Run()`（`BaseObject.Base.cs:44/74/105`）的体力点回血/掉血阈值。
    pub health_fill_time: i32,
    /// `[Setup] SpellFillTime`（oracle `GameSvrConf.cs:174/1324`，缺省 `800`；
    /// `ServerConf.cs:153` 的 `ReadWriteInteger("Setup", "SpellFillTime", …)`）——
    /// `BaseObject.Run()`（`BaseObject.Base.cs:52/78`）的魔力点回蓝阈值。
    pub spell_fill_time: i32,
    /// **PL-1 / P32**：`[Setup] RevivalTime`（oracle `GameSvrConf.cs:804/1725` 缺省 `60*1000`ms；
    /// `ServerConf.cs:503` 的 `ReadWriteInteger("Setup", "RevivalTime", …)`）——
    /// 复活戒指的二次复活间隔门（`PlayObject.Message.cs:753`，**严格 `>`**）。
    pub revival_time: i32,
    /// **PL-3 / P26**：`[Setup] HungerSystem`（oracle `GameSvrConf.cs:501/1505` 缺省 `false`；
    /// `ServerConf.cs:282` 的 `ReadWriteBool`）——饥饿系统的总开关（`PlayObject.Message.cs:596`）。
    /// 部署 `Server.conf:396` = `0` ⇒ 生产整段短路。
    pub hunger_system: bool,
    /// **PL-3 / P26**：`[Setup] HungerDecHP`（oracle `GameSvrConf.cs:502/1506` 缺省 `false`；
    /// `ServerConf.cs:283`）——`HungerStatus <= 0` 时的扣血支（`PlayObject.Message.cs:612`）。
    /// 部署 `Server.conf:398` = `0`。
    pub hunger_dec_hp: bool,
    /// `[Setup] ShowLineNoticeTime`（oracle `GameSvrConf.cs:340/1440`，缺省 `5*60*1000`；
    /// `ServerConf.cs:587` 的 `ReadWriteInteger("Setup", "ShowLineNoticeTime", …)`）——
    /// 逐玩家滚动公告的周期（`WorldServer.cs:669` 的 `(now - ShowLineNoticeTick) > ShowLineNoticeTime`）。
    /// 现网 `Server.conf:335` = `300000`。
    pub show_line_notice_time: i32,
    /// `[Setup] LineNoticeColor`（oracle `GameSvrConf.cs:341/1441`，缺省 `2`=Blue；
    /// `ServerConf.cs:588` 的 `ReadWriteByte("Setup", "LineNoticeColor", …)`）——
    /// 无 `R`/`G`/`B` 前缀的公告行所用 `MsgColor`（`MsgColor`：`Red=0/Green=1/Blue=2/White=3`）。
    /// 现网 `Server.conf:336` = `0`（红）。
    pub line_notice_color: u8,
    /// `[Setup] ShowPreFixMsg`（oracle `GameSvrConf.cs:1751` 缺省 `false`；
    /// `ServerConf.cs:529` 的 `ReadWriteBool("Setup", "ShowPreFixMsg", …)`）——为真时公告前拼接
    /// `Config.LineNoticePreFix`。现网 `Server.conf:778` = `0`（假 ⇒ 前缀支不可达）。
    pub show_pre_fix_msg: bool,
    /// `[Setup] GuildMsgFColor`（oracle `GameSvrConf.cs:1542` 缺省 `0xDB`；
    /// `ServerConf.cs:335` 的 `ReadWriteByte`）——行会消息 `104` 的 `Param` 低字节。
    pub guild_msg_f_color: u8,
    /// `[Setup] GuildMsgBColor`（oracle `GameSvrConf.cs:1543` 缺省 `0xFF`；
    /// `ServerConf.cs:336`）——`104` 的 `Param` 高字节。
    pub guild_msg_b_color: u8,
    /// `[Setup] MonsterPowerRate`（oracle `GameSvrConf.cs:391/1476`，缺省 `10`；
    /// `ServerConf.cs:560` 的 `ReadWriteInteger("Setup", "MonsterPowerRate", …)`）——
    /// `load_monster_db` 的怪物能力缩放（`monitems::load_monsters` 的入参）。
    pub monster_power_rate: i32,
    /// `[Setup] ScriptGotoCountLimit`（oracle `GameSvrConf.cs:542/1527`，缺省 `30`；
    /// `ServerConf.cs:320` 的 `ReadWriteInteger("Setup", "ScriptGotoCountLimit", …)`）——
    /// `ScriptNpc.ScriptGotoCountLimit`（`ScriptEngine.cs:629` 的 `playerActor.ScriptGotoCount >`
    /// 守卫）——NPC 脚本 `goto` 递归上限，`NpcHost` 装载时注入。
    pub script_goto_count_limit: i32,
}

/// 默认配置文件：可执行文件同目录的 `Server.conf`（与 C# `AppContext.BaseDirectory` 一致）。
#[must_use]
pub fn default_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Server.conf")
}

/// 读取并校验配置。
///
/// # Errors
/// 文件不存在、缺 `[Server]` 段、缺 `GateAddr`/`GatePort`/`CheckBlock`/`IDSAddr`/`IDSPort`/`DBAddr`/`DBPort`、
/// 端口非法、`WorldMaps` **写了但非法** —— 都返回错误（调用方据此非零退出）。
/// `WorldMaps` **没写**不算错（可选键，见模块头）；`WorldMaps=` 空值 ⇒ 空表。
pub fn load(path: &Path) -> Result<GateConfig> {
    let bytes =
        std::fs::read(path).with_context(|| format!("读不到配置文件 {}", path.display()))?;
    // **编码口径**（E-5）：oracle 的 `ConfigFile.Load` 是
    // `new StreamReader(File.Open(...), Encoding.GetEncoding("GB2312"))`
    // （`Common/ConfigFile.cs:263`）。GB2312/cp936 的 preamble 长度为 0 ⇒
    // `StreamReader` **会**做 BOM 探测并据此改编码：UTF-8 BOM / UTF-16LE / UTF-16BE /
    // UTF-32LE（`FF FE 00 00`）/ UTF-32BE 都能读，无 BOM 按 cp936。
    // 统一走 `mir2_script::cp936::decode_config_text`（= `mir2_encoding::decode_config_text`，
    // 与 `crates/envir` 的 `GameSvrConf` 同一条链），**不再**用严格 UTF-8 的
    // `read_to_string`（GBK/UTF-16 的 `Server.conf` 会直接启动失败，oracle 正常）。
    let text = mir2_script::cp936::decode_config_text(&bytes);
    let text = text.as_str();
    let mut section = String::new();
    let mut gate_addr: Option<String> = None;
    let mut gate_port: Option<u16> = None;
    let mut check_block: Option<usize> = None;
    let mut server_index: Option<u8> = None;
    let mut test_server: Option<bool> = None;
    let mut service_mode: Option<bool> = None;
    let mut maps: Option<Vec<(i32, i32)>> = None;
    let mut envir_dir: Option<String> = None;
    let mut map_dir: Option<String> = None;
    let mut notice_dir: Option<String> = None;
    let mut red_msg_f_color: Option<u8> = None;
    let mut red_msg_b_color: Option<u8> = None;
    let mut green_msg_f_color: Option<u8> = None;
    let mut green_msg_b_color: Option<u8> = None;
    let mut disable_in_safe_zone_fire_cross: Option<bool> = None;
    let mut try_deal_time: Option<i32> = None;
    let mut deal_ok_time: Option<i32> = None;
    let mut can_not_get_back_deal: Option<bool> = None;
    let mut disable_deal: Option<bool> = None;
    let mut guild_msg_f_color: Option<u8> = None;
    let mut guild_msg_b_color: Option<u8> = None;
    let mut hear_msg_f_color: Option<u8> = None;
    let mut hear_msg_b_color: Option<u8> = None;
    let mut whisper_msg_f_color: Option<u8> = None;
    let mut whisper_msg_b_color: Option<u8> = None;
    let mut blue_msg_f_color: Option<u8> = None;
    let mut blue_msg_b_color: Option<u8> = None;
    let mut group_members_max: Option<u8> = None;
    let mut group_msg_f_color: Option<u8> = None;
    let mut group_msg_b_color: Option<u8> = None;
    let mut cry_msg_f_color: Option<u8> = None;
    let mut cry_msg_b_color: Option<u8> = None;
    let mut can_shout_msg_level: Option<i32> = None;
    let mut show_rank_level_name: Option<bool> = None;
    let mut human_max_gold: Option<i32> = None;
    let mut floor_item_can_pick_up_time: Option<i32> = None;
    let mut control_drop_item: Option<bool> = None;
    let mut in_safe_disable_drop: Option<bool> = None;
    let mut can_drop_price: Option<i32> = None;
    let mut can_drop_gold: Option<i32> = None;
    let mut drop_item_rage: Option<u8> = None;
    let mut die_drop_use_item_rate: Option<i32> = None;
    let mut die_red_drop_use_item_rate: Option<i32> = None;
    let mut kill_by_human_drop_use_item: Option<bool> = None;
    let mut kill_by_monst_drop_use_item: Option<bool> = None;
    let mut send_ref_msg_range: Option<u8> = None;
    let mut magic_attack_rage: Option<u8> = None;
    let mut fire_boom_rage: Option<u8> = None;
    let mut snow_wind_range: Option<u8> = None;
    let mut elec_blizzard_range: Option<u8> = None;
    let mut mag_turn_undead_level: Option<i32> = None;
    let mut pk_flag_time: Option<i32> = None;
    let mut mab_mabe_hit_rand_rate: Option<i32> = None;
    let mut mab_mabe_hit_min_lv_limit: Option<i32> = None;
    let mut mab_mabe_hit_sucess_rate: Option<i32> = None;
    let mut mab_mabe_hit_mabe_time_rate: Option<i32> = None;
    let mut amy_ounsul_point: Option<i32> = None;
    let mut super_repair_price_rate: Option<i32> = None;
    let mut repair_item_dec_dura: Option<i32> = None;
    let mut make_durg_price: Option<i32> = None;
    let mut black_stone: Option<String> = None;
    let mut upgrade_weapon_price: Option<i32> = None;
    let mut upgrade_weapon_get_back_time: Option<i32> = None;
    let mut clear_expire_upgrade_weapon_days: Option<u8> = None;
    let mut upgrade_weapon_dc_rate: Option<u8> = None;
    let mut upgrade_weapon_dc_two_point_rate: Option<u8> = None;
    let mut upgrade_weapon_dc_three_point_rate: Option<u8> = None;
    let mut upgrade_weapon_mc_rate: Option<u8> = None;
    let mut upgrade_weapon_mc_two_point_rate: Option<u8> = None;
    let mut upgrade_weapon_mc_three_point_rate: Option<u8> = None;
    let mut upgrade_weapon_sc_rate: Option<u8> = None;
    let mut upgrade_weapon_sc_two_point_rate: Option<u8> = None;
    let mut upgrade_weapon_sc_three_point_rate: Option<u8> = None;
    let mut castle_member_price_rate: Option<i32> = None;
    let mut castle_get_all_npc_tax: Option<bool> = None;
    let mut start_point_size: Option<u8> = None;
    let mut home_map: Option<String> = None;
    let mut home_x: Option<i16> = None;
    let mut home_y: Option<i16> = None;
    let mut candle: Option<String> = None;
    let mut basic_drug: Option<String> = None;
    let mut wooden_sword: Option<String> = None;
    let mut cloths_man: Option<String> = None;
    let mut cloths_woman: Option<String> = None;
    let mut item_number: Option<i32> = None;
    let mut save_human_rcd_time: Option<i32> = None;
    let mut health_fill_time: Option<i32> = None;
    let mut spell_fill_time: Option<i32> = None;
    let mut revival_time: Option<i32> = None;
    let mut hunger_system: Option<bool> = None;
    let mut hunger_dec_hp: Option<bool> = None;
    let mut show_line_notice_time: Option<i32> = None;
    let mut line_notice_color: Option<u8> = None;
    let mut show_pre_fix_msg: Option<bool> = None;
    let mut monster_power_rate: Option<i32> = None;
    let mut script_goto_count_limit: Option<i32> = None;
    let mut server_name: Option<String> = None;
    let mut ids_addr: Option<String> = None;
    let mut ids_port: Option<u16> = None;
    let mut db_addr: Option<String> = None;
    let mut db_port: Option<u16> = None;
    let mut db_conn_string: Option<String> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            // oracle `ConfigFile` 的 section 字典是 `StringComparer.OrdinalIgnoreCase`
            // （`src/OpenMir2/Common/ConfigFile.cs:13`，R3-2）⇒ 段名按 ASCII 大小写不敏感归一。
            section = name.trim().to_ascii_uppercase();
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        // 键字典同样 `OrdinalIgnoreCase`（`ConfigFile.cs:301`，R3-2）：键名也归一到 ASCII 大写。
        let (k, v) = (k.trim().to_ascii_uppercase(), v.trim());
        // **M3⑥-实现 3 第②片**：`[Setup] ScriptGotoCountLimit`（`ServerConf.cs:320` 的
        // `ReadWriteInteger`；现网 `Server.conf` 写 `=10000`）在下面的 `"SETUP"` match **之前**
        // 单独处理——不并入那个 match（并入会让 rustfmt 因此重排整段，产生与本片无关的 diff）。
        if section == "SETUP" && k == "SCRIPTGOTOCOUNTLIMIT" {
            script_goto_count_limit = Some(
                v.parse()
                    .with_context(|| format!("ScriptGotoCountLimit 不是合法整数：{v:?}"))?,
            );
        }
        // **P3 收尾残余**：`ServerConf.cs:530` / `:156`。放在 `"SETUP"` match 之前的原因同
        // `ScriptGotoCountLimit`：给共享大 match 加臂会让 rustfmt 把整臂折成块（无谓 diff）。
        // 空值 ⇒ 保持 `None` ⇒ 回落 oracle 缺省（任务口径）。
        if section == "SETUP" && k == "MAGTURNUNDEADLEVEL" && !v.is_empty() {
            mag_turn_undead_level = Some(
                v.parse()
                    .with_context(|| format!("MagTurnUndeadLevel 不是合法整数：{v:?}"))?,
            );
        }
        if section == "SETUP" && k == "PKFLAGTIME" && !v.is_empty() {
            pk_flag_time = Some(
                v.parse()
                    .with_context(|| format!("PKFlagTime 不是合法整数：{v:?}"))?,
            );
        }
        match section.as_str() {
            // 网关链路真正需要的键（本切片读这些）。
            "SERVER" => match k.as_str() {
                "GATEADDR" => gate_addr = Some(v.to_string()),
                "GATEPORT" => {
                    gate_port = Some(
                        v.parse()
                            .with_context(|| format!("GatePort 不是合法端口：{v:?}"))?,
                    );
                }
                "CHECKBLOCK" => {
                    check_block = Some(
                        v.parse()
                            .with_context(|| format!("CheckBlock 不是合法整数：{v:?}"))?,
                    );
                }
                "SERVERINDEX" => {
                    // oracle `ReadWriteByte` -> `Read<byte>`：键存在但值为**空串**时
                    // `string.IsNullOrEmpty(str)` => 返回缺省 `defValue`（0，`ConfigFile.cs:131-133`）。
                    // 缺键同样回落 0（下面 `unwrap_or`）；只有非法值按 byte 解析口径报错。
                    server_index = Some(if v.is_empty() {
                        0
                    } else {
                        v.parse()
                            .with_context(|| format!("ServerIndex 不是合法字节：{v:?}"))?
                    });
                }
                // `ServerConf.cs:23/27`：`TestServer` 用 `OrdinalIgnoreCase`（大小写不敏感）；
                // `ServiceMode` 用 `String.CompareTo("TRUE") == 0`——culture-aware，**非
                // `StringComparison.Ordinal`**，对 ASCII 大小写敏感 ⇒ **只认大写 TRUE**。
                // 缺键都是 `false`。
                "TESTSERVER" => test_server = Some(v.eq_ignore_ascii_case("TRUE")),
                "SERVICEMODE" => service_mode = Some(v == "TRUE"),
                "WORLDMAPS" => maps = Some(parse_maps(v)?),
                "SERVERNAME" => server_name = Some(v.to_string()),
                "IDSADDR" => ids_addr = Some(v.to_string()),
                "DBADDR" => db_addr = Some(v.to_string()),
                "DBPORT" => {
                    db_port = Some(
                        v.parse()
                            .with_context(|| format!("DBPort 不是合法端口：{v:?}"))?,
                    );
                }
                "IDSPORT" => {
                    ids_port = Some(
                        v.parse()
                            .with_context(|| format!("IDSPort 不是合法端口：{v:?}"))?,
                    );
                }
                _ => {}
            },
            // `[Names] BlackStone`（`ServerConf.cs:91` 的 `ReadWriteString("Names", …)`）。
            "NAMES" => {
                if k == "BLACKSTONE" {
                    black_stone = Some(v.to_string());
                }
            }
            // oracle 的取图目录（`ServerConf.cs:75-76` 从 `[Share]` 读，缺省 Envir/Map）。
            "SHARE" => match k.as_str() {
                "ENVIRDIR" => envir_dir = Some(v.to_string()),
                "MAPDIR" => map_dir = Some(v.to_string()),
                "NOTICEDIR" => notice_dir = Some(v.to_string()),
                _ => {}
            },
            // `MySqlDB` 直连用（`ServerConf.cs:15` 从 `[DataBase]` 读 `ConnctionString`；原拼写照抄）。
            // 现网样件 `E:\MirServer\M2GameSvr\Server.conf` 在 `[DataBase]` 段（键 `ConnctionString`）。
            "DATABASE" => {
                if k.as_str() == "CONNCTIONSTRING" {
                    db_conn_string = Some(v.to_string());
                }
            }
            // 消息颜色（`SystemShare.Config.RedMsgFColor/BColor`，`ServerConf.cs:333-334` 的 `ReadWriteByte`）。
            // 缺键不是错误（oracle 读不到就沿用 `GameSvrConf` 的缺省 0xFF/0x38）；写了但非法才报错。
            "SETUP" => match k.as_str() {
                "REDMSGFCOLOR" => {
                    red_msg_f_color = Some(
                        v.parse()
                            .with_context(|| format!("RedMsgFColor 不是合法字节：{v:?}"))?,
                    );
                }
                "REDMSGBCOLOR" => {
                    red_msg_b_color = Some(
                        v.parse()
                            .with_context(|| format!("RedMsgBColor 不是合法字节：{v:?}"))?,
                    );
                }
                "GREENMSGFCOLOR" => {
                    green_msg_f_color = Some(
                        v.parse()
                            .with_context(|| format!("GreenMsgFColor 不是合法字节：{v:?}"))?,
                    );
                }
                "GREENMSGBCOLOR" => {
                    green_msg_b_color = Some(
                        v.parse()
                            .with_context(|| format!("GreenMsgBColor 不是合法字节：{v:?}"))?,
                    );
                }
                // 22 火墙安全区门（`GameSvrConf.cs:1764` 的 `ReadWriteBool`，源码缺省 false）。
                // `ReadBool` 口径（`ConfigFile.cs:188-203`）：大写后仅 `是`/`YES`/`1`/`TRUE`
                // 为真；空 ⇒ 缺省 false；其余（含 `False`/`no`/`off`/垃圾）一律 false。
                "DISABLEINSAFEZONEFIRECROSS" => {
                    disable_in_safe_zone_fire_cross = Some(read_bool_or_false(v));
                }
                // P26 饥饿系统（`GameSvrConf.cs:501-502` 的 `ReadWriteBool`，源码缺省 false；
                // 空/未识别值 ⇒ false，不报错——与 `ReadBool` 同口径）。
                "HUNGERSYSTEM" => {
                    hunger_system = Some(read_bool_or_false(v));
                }
                "HUNGERDECHP" => {
                    hunger_dec_hp = Some(read_bool_or_false(v));
                }
                // 元宝寄售 `SysMsg(..., Blue, ...)` 的颜色（`ServerConf.cs:331-332` 的 `ReadWriteByte`）。
                "BLUEMSGFCOLOR" => {
                    blue_msg_f_color = Some(
                        v.parse()
                            .with_context(|| format!("BlueMsgFColor 不是合法字节：{v:?}"))?,
                    );
                }
                "BLUEMSGBCOLOR" => {
                    blue_msg_b_color = Some(
                        v.parse()
                            .with_context(|| format!("BlueMsgBColor 不是合法字节：{v:?}"))?,
                    );
                }
                // `ServerConf.cs:308-311` 的四条交易配置（M3⑦-实现 3）。
                "TRYDEALTIME" => {
                    try_deal_time = Some(
                        v.parse()
                            .with_context(|| format!("TryDealTime 不是合法整数：{v:?}"))?,
                    );
                }
                "DEALOKTIME" => {
                    deal_ok_time = Some(
                        v.parse()
                            .with_context(|| format!("DealOKTime 不是合法整数：{v:?}"))?,
                    );
                }
                "CANNOTGETBACKDEAL" => {
                    can_not_get_back_deal = Some(parse_bool(v, "CanNotGetBackDeal")?);
                }
                "DISABLEDEAL" => {
                    disable_deal = Some(parse_bool(v, "DisableDeal")?);
                }
                "GUILDMSGFCOLOR" => {
                    guild_msg_f_color = Some(
                        v.parse()
                            .with_context(|| format!("GuildMsgFColor 不是合法字节：{v:?}"))?,
                    );
                }
                "GUILDMSGBCOLOR" => {
                    guild_msg_b_color = Some(
                        v.parse()
                            .with_context(|| format!("GuildMsgBColor 不是合法字节：{v:?}"))?,
                    );
                }
                "HEARMSGFCOLOR" => {
                    hear_msg_f_color = Some(
                        v.parse()
                            .with_context(|| format!("HearMsgFColor 不是合法字节：{v:?}"))?,
                    );
                }
                "HEARMSGBCOLOR" => {
                    hear_msg_b_color = Some(
                        v.parse()
                            .with_context(|| format!("HearMsgBColor 不是合法字节：{v:?}"))?,
                    );
                }
                "WHISPERMSGFCOLOR" => {
                    whisper_msg_f_color = Some(
                        v.parse()
                            .with_context(|| format!("WhisperMsgFColor 不是合法字节：{v:?}"))?,
                    );
                }
                "WHISPERMSGBCOLOR" => {
                    whisper_msg_b_color = Some(
                        v.parse()
                            .with_context(|| format!("WhisperMsgBColor 不是合法字节：{v:?}"))?,
                    );
                }
                // `ServerConf.cs:327-328` 的喊话颜色（`ReadWriteByte`）；缺省 `0x00/0x97`。
                "CRYMSGFCOLOR" => {
                    cry_msg_f_color = Some(
                        v.parse()
                            .with_context(|| format!("CryMsgFColor 不是合法字节：{v:?}"))?,
                    );
                }
                "CRYMSGBCOLOR" => {
                    cry_msg_b_color = Some(
                        v.parse()
                            .with_context(|| format!("CryMsgBColor 不是合法字节：{v:?}"))?,
                    );
                }
                // `ServerConf.cs:278` 的 `ReadWriteInteger("Setup", "CanShoutMsgLevel", …)`
                // （`GameSvrConf.cs:1455` 缺省 7）。
                "CANSHOUTMSGLEVEL" => {
                    can_shout_msg_level = Some(
                        v.parse()
                            .with_context(|| format!("CanShoutMsgLevel 不是合法整数：{v:?}"))?,
                    );
                }
                // `ServerConf.cs:225/337/338` 的组队配置（M3⑦-实现 2）。
                "GROUPMEMBERSMAX" => {
                    group_members_max = Some(
                        v.parse()
                            .with_context(|| format!("GroupMembersMax 不是合法字节：{v:?}"))?,
                    );
                }
                "GROUPMSGFCOLOR" => {
                    group_msg_f_color = Some(
                        v.parse()
                            .with_context(|| format!("GroupMsgFColor 不是合法字节：{v:?}"))?,
                    );
                }
                "GROUPMSGBCOLOR" => {
                    group_msg_b_color = Some(
                        v.parse()
                            .with_context(|| format!("GroupMsgBColor 不是合法字节：{v:?}"))?,
                    );
                }
                // `ServerConf.cs:271` 的 `ReadWriteBool("Setup", ...)`：现网 `0/1`。
                "SHOWRANKLEVELNAME" => {
                    show_rank_level_name = Some(parse_bool(v, "ShowRankLevelName")?);
                }
                // `ServerConf.cs:264` 的 `ReadWriteInteger("Setup", "HumanMaxGold", ...)`。
                "HUMANMAXGOLD" => {
                    human_max_gold = Some(
                        v.parse()
                            .with_context(|| format!("HumanMaxGold 不是合法整数：{v:?}"))?,
                    );
                }
                // `ServerConf.cs:439` 的 `ReadWriteInteger("Setup", "FloorItemCanPickUpTime", ...)`。
                "FLOORITEMCANPICKUPTIME" => {
                    floor_item_can_pick_up_time =
                        Some(v.parse().with_context(|| {
                            format!("FloorItemCanPickUpTime 不是合法整数：{v:?}")
                        })?);
                }
                // `ServerConf.cs:481-482/484` 的三条 `ReadWriteBool/Integer("Setup", ...)`。
                "CONTROLDROPITEM" => {
                    control_drop_item = Some(parse_bool(v, "ControlDropItem")?);
                }
                "INSAFEDISABLEDROP" => {
                    in_safe_disable_drop = Some(parse_bool(v, "InSafeDisableDrop")?);
                }
                "CANDROPPRICE" => {
                    can_drop_price = Some(
                        v.parse()
                            .with_context(|| format!("CanDropPrice 不是合法整数：{v:?}"))?,
                    );
                }
                "CANDROPGOLD" => {
                    can_drop_gold = Some(
                        v.parse()
                            .with_context(|| format!("CanDropGold 不是合法整数：{v:?}"))?,
                    );
                }
                // `GameSvrConf.DropItemRage` 是 `byte`（`GameSvrConf.cs:878`）；`Server.conf` 现网未写。
                "DROPITEMRAGE" => {
                    drop_item_rage = Some(
                        v.parse()
                            .with_context(|| format!("DropItemRage 不是合法字节：{v:?}"))?,
                    );
                }
                // `GameSvrConf.SendRefMsgRange` 是 `byte`（`GameSvrConf.cs:499`）；`Server.conf` 现网未写。
                "SENDREFMSGRANGE" => {
                    send_ref_msg_range = Some(
                        v.parse()
                            .with_context(|| format!("SendRefMsgRange 不是合法字节：{v:?}"))?,
                    );
                }
                // `GameSvrConf.MagicAttackRage` 是 `byte`（`GameSvrConf.cs:873`）缺省 8。
                "MAGICATTACKRAGE" => {
                    magic_attack_rage = Some(
                        v.parse()
                            .with_context(|| format!("MagicAttackRage 不是合法字节：{v:?}"))?,
                    );
                }
                "FIREBOOMRAGE" => {
                    fire_boom_rage = Some(
                        v.parse()
                            .with_context(|| format!("FireBoomRage 不是合法字节：{v:?}"))?,
                    );
                }
                "SNOWWINDRANGE" => {
                    snow_wind_range = Some(
                        v.parse()
                            .with_context(|| format!("SnowWindRange 不是合法字节：{v:?}"))?,
                    );
                }
                "ELECBLIZZARDRANGE" => {
                    elec_blizzard_range = Some(
                        v.parse()
                            .with_context(|| format!("ElecBlizzardRange 不是合法字节：{v:?}"))?,
                    );
                }
                // 出生点回落（`WorldServer.cs:250-260` 的 `GetHomeInfo`）：现网 `Server.conf`
                // 写 `HomeMap=3`/`HomeX=839`/`HomeY=668`/`StartPointSize=2`。
                "STARTPOINTSIZE" => {
                    start_point_size = Some(
                        v.parse()
                            .with_context(|| format!("StartPointSize 不是合法字节：{v:?}"))?,
                    );
                }
                "HOMEMAP" => home_map = Some(v.to_string()),
                "HOMEX" => {
                    home_x = Some(
                        v.parse()
                            .with_context(|| format!("HomeX 不是合法整数：{v:?}"))?,
                    );
                }
                "HOMEY" => {
                    home_y = Some(
                        v.parse()
                            .with_context(|| format!("HomeY 不是合法整数：{v:?}"))?,
                    );
                }
                // 新角色 `IsNewHuman` 的 4 件新手物品（`PlayObject.cs:1205-1245`）+ `GetItemNumber` 初值。
                "CANDLE" => candle = Some(v.to_string()),
                "BASICDRUG" => basic_drug = Some(v.to_string()),
                "WOODENSWORD" => wooden_sword = Some(v.to_string()),
                "CLOTHSMAN" => cloths_man = Some(v.to_string()),
                "CLOTHSWOMAN" => cloths_woman = Some(v.to_string()),
                "ITEMNUMBER" => {
                    item_number = Some(
                        v.parse()
                            .with_context(|| format!("ItemNumber 不是合法整数：{v:?}"))?,
                    );
                }
                // 周期存档（`ServerConf.cs:566`；现网 `Server.conf` 未写 ⇒ 缺省 10 分钟）。
                "SAVEHUMANRCDTIME" => {
                    save_human_rcd_time = Some(
                        v.parse()
                            .with_context(|| format!("SaveHumanRcdTime 不是合法整数：{v:?}"))?,
                    );
                }
                // 体力/魔力点阈值（`ServerConf.cs:152-153`）：现网 `Server.conf:785-786` 写了
                // `HealthFillTime=300` / `SpellFillTime=800`；缺键时 oracle 用
                // `GameSvrConf.cs:1323-1324` 的源码缺省 `300`/`800`。
                "HEALTHFILLTIME" => {
                    health_fill_time = Some(
                        v.parse()
                            .with_context(|| format!("HealthFillTime 不是合法整数：{v:?}"))?,
                    );
                }
                "SPELLFILLTIME" => {
                    spell_fill_time = Some(
                        v.parse()
                            .with_context(|| format!("SpellFillTime 不是合法整数：{v:?}"))?,
                    );
                }
                // `ServerConf.cs:503` 的 `ReadWriteInteger("Setup", "RevivalTime", …)`；
                // `GameSvrConf.cs:1725` 源码缺省 `60*1000`。
                "REVIVALTIME" => {
                    revival_time = Some(
                        v.parse()
                            .with_context(|| format!("RevivalTime 不是合法整数：{v:?}"))?,
                    );
                }
                // 滚动公告周期（`ServerConf.cs:587`；`GameSvrConf.cs:1440` 缺省 5 分钟）。
                "SHOWLINENOTICETIME" => {
                    show_line_notice_time = Some(
                        v.parse()
                            .with_context(|| format!("ShowLineNoticeTime 不是合法整数：{v:?}"))?,
                    );
                }
                // 公告缺省颜色（`ServerConf.cs:588`；`GameSvrConf.cs:1441` 缺省 2=Blue）。
                "LINENOTICECOLOR" => {
                    line_notice_color = Some(
                        v.parse()
                            .with_context(|| format!("LineNoticeColor 不是合法字节：{v:?}"))?,
                    );
                }
                // 公告前缀开关（`ServerConf.cs:529`；`GameSvrConf.cs:1751` 缺省 false）。
                "SHOWPREFIXMSG" => {
                    show_pre_fix_msg = Some(matches!(
                        v.to_ascii_lowercase().as_str(),
                        "1" | "true" | "yes" | "on"
                    ));
                }
                // 怪物能力缩放（`ServerConf.cs:560`；现网 `Server.conf:377` 写 `=10`）。
                "MONSTERPOWERRATE" => {
                    monster_power_rate = Some(
                        v.parse()
                            .with_context(|| format!("MonsterPowerRate 不是合法整数：{v:?}"))?,
                    );
                }
                _ => {
                    // 商店定价/修理/税（`ServerConf.cs:250/260/487/488`）。单独一层，避免改动
                    // 上面共享 `[Setup]` 大 match 的既有臂（rustfmt 会因臂数增长把整臂折成块 ⇒
                    // 在共享文件里制造无谓 diff）。
                    match k.as_str() {
                        // **P3-B4**：`MabMabe*` / `AmyOunsulPoint`（`ServerConf.cs:535-541`）。
                        "MABMABEHITRANDRATE" => {
                            mab_mabe_hit_rand_rate = Some(v.parse().with_context(|| {
                                format!("MabMabeHitRandRate 不是合法整数：{v:?}")
                            })?);
                        }
                        "MABMABEHITMINLVLIMIT" => {
                            mab_mabe_hit_min_lv_limit = Some(v.parse().with_context(|| {
                                format!("MabMabeHitMinLvLimit 不是合法整数：{v:?}")
                            })?);
                        }
                        "MABMABEHITSUCESSRATE" => {
                            mab_mabe_hit_sucess_rate = Some(v.parse().with_context(|| {
                                format!("MabMabeHitSucessRate 不是合法整数：{v:?}")
                            })?);
                        }
                        "MABMABEHITMABETIMERATE" => {
                            mab_mabe_hit_mabe_time_rate = Some(v.parse().with_context(|| {
                                format!("MabMabeHitMabeTimeRate 不是合法整数：{v:?}")
                            })?);
                        }
                        "AMYOUNSULPOINT" => {
                            amy_ounsul_point =
                                Some(v.parse().with_context(|| {
                                    format!("AmyOunsulPoint 不是合法整数：{v:?}")
                                })?);
                        }
                        "DIEDROPUSEITEMRATE" => {
                            die_drop_use_item_rate = Some(v.parse().with_context(|| {
                                format!("DieDropUseItemRate 不是合法整数：{v:?}")
                            })?);
                        }
                        "DIEREDDROPUSEITEMRATE" => {
                            die_red_drop_use_item_rate = Some(v.parse().with_context(|| {
                                format!("DieRedDropUseItemRate 不是合法整数：{v:?}")
                            })?);
                        }
                        "KILLBYHUMANDROPUSEITEM" => {
                            kill_by_human_drop_use_item = Some(read_bool_or_false(v));
                        }
                        "KILLBYMONSTDROPUSEITEM" => {
                            kill_by_monst_drop_use_item = Some(read_bool_or_false(v));
                        }
                        "SUPERREPAIRPRICERATE" => {
                            super_repair_price_rate = Some(v.parse().with_context(|| {
                                format!("SuperRepairPriceRate 不是合法整数：{v:?}")
                            })?);
                        }
                        "REPAIRITEMDECDURE" => {
                            repair_item_dec_dura = Some(v.parse().with_context(|| {
                                format!("RepairItemDecDura 不是合法整数：{v:?}")
                            })?);
                        }
                        // `ServerConf.cs:244` 的 `ReadWriteInteger("Setup", "MakeDurg", …)`。
                        "MAKEDURG" => {
                            make_durg_price = Some(
                                v.parse()
                                    .with_context(|| format!("MakeDurg 不是合法整数：{v:?}"))?,
                            );
                        }
                        // 武器升级 NPC（`ServerConf.cs:230-242`；`ReadWriteInteger`/`ReadWriteByte`，
                        // 空值 ⇒ oracle 返回缺省值）。
                        "UPGRADEWEAPONPRICE" => {
                            upgrade_weapon_price =
                                Some(read_i32_or_default(v, 10_000, "UpgradeWeaponPrice")?);
                        }
                        "UPGRADEWEAPONGETBACKTIME" => {
                            upgrade_weapon_get_back_time = Some(read_i32_or_default(
                                v,
                                60 * 60 * 1000,
                                "UPgradeWeaponGetBackTime",
                            )?);
                        }
                        "CLEAREXPIREUPGRADEWEAPONDAYS" => {
                            clear_expire_upgrade_weapon_days =
                                Some(read_u8_or_default(v, 8, "ClearExpireUpgradeWeaponDays")?);
                        }
                        "UPGRADEWEAPONDCRATE" => {
                            upgrade_weapon_dc_rate =
                                Some(read_u8_or_default(v, 100, "UpgradeWeaponDCRate")?);
                        }
                        "UPGRADEWEAPONDCTWOPOINTRATE" => {
                            upgrade_weapon_dc_two_point_rate =
                                Some(read_u8_or_default(v, 30, "UpgradeWeaponDCTwoPointRate")?);
                        }
                        "UPGRADEWEAPONDCTHREEPOINTRATE" => {
                            upgrade_weapon_dc_three_point_rate =
                                Some(read_u8_or_default(v, 200, "UpgradeWeaponDCThreePointRate")?);
                        }
                        "UPGRADEWEAPONMCRATE" => {
                            upgrade_weapon_mc_rate =
                                Some(read_u8_or_default(v, 100, "UpgradeWeaponMCRate")?);
                        }
                        "UPGRADEWEAPONMCTWOPOINTRATE" => {
                            upgrade_weapon_mc_two_point_rate =
                                Some(read_u8_or_default(v, 30, "UpgradeWeaponMCTwoPointRate")?);
                        }
                        "UPGRADEWEAPONMCTHREEPOINTRATE" => {
                            upgrade_weapon_mc_three_point_rate =
                                Some(read_u8_or_default(v, 200, "UpgradeWeaponMCThreePointRate")?);
                        }
                        "UPGRADEWEAPONSCRATE" => {
                            upgrade_weapon_sc_rate =
                                Some(read_u8_or_default(v, 100, "UpgradeWeaponSCRate")?);
                        }
                        "UPGRADEWEAPONSCTWOPOINTRATE" => {
                            upgrade_weapon_sc_two_point_rate =
                                Some(read_u8_or_default(v, 30, "UpgradeWeaponSCTwoPointRate")?);
                        }
                        "UPGRADEWEAPONSCTHREEPOINTRATE" => {
                            upgrade_weapon_sc_three_point_rate =
                                Some(read_u8_or_default(v, 200, "UpgradeWeaponSCThreePointRate")?);
                        }
                        "CASTLEMEMBERPRICERATE" => {
                            castle_member_price_rate = Some(v.parse().with_context(|| {
                                format!("CastleMemberPriceRate 不是合法整数：{v:?}")
                            })?);
                        }
                        "CASTLEGETALLNPCTAX" => {
                            castle_get_all_npc_tax = Some(parse_bool(v, "CastleGetAllNpcTax")?);
                        }
                        _ => {}
                    }
                }
            },
            _ => {}
        }
    }
    let (
        Some(gate_addr),
        Some(gate_port),
        Some(check_block),
        Some(ids_addr),
        Some(ids_port),
        Some(db_addr),
        Some(db_port),
    ) = (
        gate_addr,
        gate_port,
        check_block,
        ids_addr,
        ids_port,
        db_addr,
        db_port,
    )
    else {
        bail!(
            "[Server] 段缺键：需要 GateAddr/GatePort/CheckBlock/IDSAddr/IDSPort/DBAddr/DBPort（现网样件见 \
             E:\\MirServer\\M2GameSvr\\Server.conf；`WorldMaps` 可选）"
        );
    };
    // `WorldMaps` 可选：没写 ⇒ 空表（合法，是否 fail-closed 由 `mapwire::placeholder_sizes` 决定）。
    let maps = maps.unwrap_or_default();
    if gate_port == 0 {
        bail!("GatePort 不能为 0");
    }
    Ok(GateConfig {
        gate_addr,
        gate_port,
        check_block,
        // oracle 缺省值（`SystemShare.cs:45` 的 `ServerIndex = 0`）：缺键不算错。
        server_index: server_index.unwrap_or(0),
        maps,
        // oracle 缺省 `FALSE`（`ServerConf.cs:23/27`）：缺键不算错。
        test_server: test_server.unwrap_or(false),
        service_mode: service_mode.unwrap_or(false),
        // oracle 缺省值（`GameSvrConf.cs:1295-1296`）：缺 `[Share]` 键不算错。
        envir_dir: envir_dir.unwrap_or_else(|| "Envir".to_string()),
        map_dir: map_dir.unwrap_or_else(|| "Map".to_string()),
        notice_dir: notice_dir.unwrap_or_else(|| "Notice".to_string()),
        // oracle 缺省值（`GameSvrConf.cs:1540-1541`）：`[Setup]` 里没写也照常起。
        red_msg_f_color: red_msg_f_color.unwrap_or(0xFF),
        red_msg_b_color: red_msg_b_color.unwrap_or(0x38),
        // oracle 缺省值（`GameSvrConf.cs:1536-1537/1515-1518`）：`[Setup]` 里没写不算错。
        green_msg_f_color: green_msg_f_color.unwrap_or(0xDB),
        green_msg_b_color: green_msg_b_color.unwrap_or(0xFF),
        // oracle 缺省 `false`（`GameSvrConf.cs:1764`）。
        disable_in_safe_zone_fire_cross: disable_in_safe_zone_fire_cross.unwrap_or(false),
        // oracle 缺省值（`GameSvrConf.cs:1538-1539`）：`[Setup]` 里没写不算错。
        blue_msg_f_color: blue_msg_f_color.unwrap_or(0xFF),
        blue_msg_b_color: blue_msg_b_color.unwrap_or(0xFC),
        try_deal_time: try_deal_time.unwrap_or(3_000),
        deal_ok_time: deal_ok_time.unwrap_or(1_000),
        can_not_get_back_deal: can_not_get_back_deal.unwrap_or(true),
        disable_deal: disable_deal.unwrap_or(false),
        // oracle 缺省值（`GameSvrConf.cs:1528-1529`）：`HearMsgFColor=0x00` / `HearMsgBColor=0xFF`。
        hear_msg_f_color: hear_msg_f_color.unwrap_or(0x00),
        hear_msg_b_color: hear_msg_b_color.unwrap_or(0xFF),
        // oracle 缺省值（`GameSvrConf.cs:1530-1531`）：`WhisperMsgFColor=0xFC` / `WhisperMsgBColor=0xFF`。
        whisper_msg_f_color: whisper_msg_f_color.unwrap_or(0xFC),
        whisper_msg_b_color: whisper_msg_b_color.unwrap_or(0xFF),
        // oracle 缺省值（`GameSvrConf.cs:1402/1544/1545`）：`[Setup]` 里没写不算错。
        group_members_max: group_members_max.unwrap_or(10),
        group_msg_f_color: group_msg_f_color.unwrap_or(0xC4),
        group_msg_b_color: group_msg_b_color.unwrap_or(0xFF),
        // oracle 缺省值（`GameSvrConf.cs:1534-1535`）：`CryMsgFColor=0x00` / `CryMsgBColor=0x97`。
        cry_msg_f_color: cry_msg_f_color.unwrap_or(0x00),
        cry_msg_b_color: cry_msg_b_color.unwrap_or(0x97),
        // oracle 缺省（`GameSvrConf.cs:1455`）：`CanShoutMsgLevel=7`。
        can_shout_msg_level: can_shout_msg_level.unwrap_or(7),
        // oracle 缺省值（`GameSvrConf.cs:1464`）：`[Setup]` 里没写不算错。
        show_rank_level_name: show_rank_level_name.unwrap_or(false),
        // oracle 缺省值（`GameSvrConf.cs:1451/1488/1703/1704/1706/1754`）：`[Setup]` 里没写不算错。
        human_max_gold: human_max_gold.unwrap_or(10_000_000),
        floor_item_can_pick_up_time: floor_item_can_pick_up_time.unwrap_or(2 * 60 * 1000),
        control_drop_item: control_drop_item.unwrap_or(false),
        in_safe_disable_drop: in_safe_disable_drop.unwrap_or(false),
        can_drop_price: can_drop_price.unwrap_or(500),
        can_drop_gold: can_drop_gold.unwrap_or(1000),
        drop_item_rage: drop_item_rage.unwrap_or(3),
        // oracle 缺省值（`GameSvrConf.cs:1714/1715`）。
        die_drop_use_item_rate: die_drop_use_item_rate.unwrap_or(30),
        die_red_drop_use_item_rate: die_red_drop_use_item_rate.unwrap_or(15),
        // oracle 缺省值（`GameSvrConf.cs:1717/1718`）。
        kill_by_human_drop_use_item: kill_by_human_drop_use_item.unwrap_or(false),
        kill_by_monst_drop_use_item: kill_by_monst_drop_use_item.unwrap_or(true),
        // oracle 缺省值（`GameSvrConf.cs:1503`）：`[Setup]` 里没写不算错。
        send_ref_msg_range: send_ref_msg_range.unwrap_or(12),
        // oracle 缺省值（`GameSvrConf.cs:1752`）：`[Setup]` 里没写不算错。
        magic_attack_rage: magic_attack_rage.unwrap_or(8),
        // oracle 缺省值（`GameSvrConf.cs:1770-1772`）。
        fire_boom_rage: fire_boom_rage.unwrap_or(1),
        snow_wind_range: snow_wind_range.unwrap_or(1),
        elec_blizzard_range: elec_blizzard_range.unwrap_or(2),
        // oracle 缺省值（`GameSvrConf.cs:1773`）：`MagTurnUndeadLevel = 50`。
        mag_turn_undead_level: mag_turn_undead_level.unwrap_or(50),
        // oracle 缺省值（`GameSvrConf.cs:1395`）：`PKFlagTime = 60 * 1000`。
        pk_flag_time: pk_flag_time.unwrap_or(60 * 1000),
        // oracle 缺省值（`GameSvrConf.cs:1763/1778-1781`）。
        mab_mabe_hit_rand_rate: mab_mabe_hit_rand_rate.unwrap_or(100),
        mab_mabe_hit_min_lv_limit: mab_mabe_hit_min_lv_limit.unwrap_or(10),
        mab_mabe_hit_sucess_rate: mab_mabe_hit_sucess_rate.unwrap_or(21),
        mab_mabe_hit_mabe_time_rate: mab_mabe_hit_mabe_time_rate.unwrap_or(20),
        amy_ounsul_point: amy_ounsul_point.unwrap_or(10),
        super_repair_price_rate: super_repair_price_rate.unwrap_or(3),
        repair_item_dec_dura: repair_item_dec_dura.unwrap_or(30),
        // oracle 缺省值（`GameSvrConf.cs:1450`）：`[Setup]` 里没写不算错。
        make_durg_price: make_durg_price.unwrap_or(100),
        // oracle 缺省值（`GameSvrConf.cs:1310/1419-1432`；部署不同值见 `Server.conf`）。
        black_stone: black_stone.unwrap_or_else(|| "黑铁矿".to_string()),
        upgrade_weapon_price: upgrade_weapon_price.unwrap_or(10_000),
        upgrade_weapon_get_back_time: upgrade_weapon_get_back_time.unwrap_or(60 * 60 * 1000),
        clear_expire_upgrade_weapon_days: clear_expire_upgrade_weapon_days.unwrap_or(8),
        upgrade_weapon_dc_rate: upgrade_weapon_dc_rate.unwrap_or(100),
        upgrade_weapon_dc_two_point_rate: upgrade_weapon_dc_two_point_rate.unwrap_or(30),
        upgrade_weapon_dc_three_point_rate: upgrade_weapon_dc_three_point_rate.unwrap_or(200),
        upgrade_weapon_mc_rate: upgrade_weapon_mc_rate.unwrap_or(100),
        upgrade_weapon_mc_two_point_rate: upgrade_weapon_mc_two_point_rate.unwrap_or(30),
        upgrade_weapon_mc_three_point_rate: upgrade_weapon_mc_three_point_rate.unwrap_or(200),
        upgrade_weapon_sc_rate: upgrade_weapon_sc_rate.unwrap_or(100),
        upgrade_weapon_sc_two_point_rate: upgrade_weapon_sc_two_point_rate.unwrap_or(30),
        upgrade_weapon_sc_three_point_rate: upgrade_weapon_sc_three_point_rate.unwrap_or(200),
        castle_member_price_rate: castle_member_price_rate.unwrap_or(80),
        castle_get_all_npc_tax: castle_get_all_npc_tax.unwrap_or(false),
        // oracle 缺省值（`GameSvrConf.cs:1400/1374/1375/1376`）：`[Setup]` 里没写不算错。
        start_point_size: start_point_size.unwrap_or(2),
        home_map: home_map.unwrap_or_else(|| "0".to_string()),
        home_x: home_x.unwrap_or(289),
        home_y: home_y.unwrap_or(618),
        // oracle 缺省值（`GameSvrConf.cs:1301-1305/1699`）。
        candle: candle.unwrap_or_else(|| "蜡烛".to_string()),
        basic_drug: basic_drug.unwrap_or_else(|| "金创药(小量)".to_string()),
        wooden_sword: wooden_sword.unwrap_or_else(|| "木剑".to_string()),
        cloths_man: cloths_man.unwrap_or_else(|| "布衣(男)".to_string()),
        cloths_woman: cloths_woman.unwrap_or_else(|| "布衣(女)".to_string()),
        item_number: item_number.unwrap_or(0),
        // oracle 缺省值（`GameSvrConf.cs:1484` 的 `SaveHumanRcdTime = 10 * 60 * 1000`）。
        save_human_rcd_time: save_human_rcd_time.unwrap_or(10 * 60 * 1000),
        // oracle 缺省值（`GameSvrConf.cs:1323-1324` 的 `HealthFillTime = 300` / `SpellFillTime = 800`）。
        health_fill_time: health_fill_time.unwrap_or(300),
        spell_fill_time: spell_fill_time.unwrap_or(800),
        // oracle 缺省值（`GameSvrConf.cs:1725` 的 `RevivalTime = 60 * 1000`）。
        revival_time: revival_time.unwrap_or(60 * 1000),
        // P26：缺键 ⇒ oracle 源码缺省 false（`GameSvrConf.cs:1505-1506`）。
        hunger_system: hunger_system.unwrap_or(false),
        hunger_dec_hp: hunger_dec_hp.unwrap_or(false),
        // oracle 缺省值（`GameSvrConf.cs:1440-1441` / `:1751`）：公告周期 5 分钟、缺省色 2、
        // 前缀关。缺 `[Setup]` 键不算错。
        show_line_notice_time: show_line_notice_time.unwrap_or(5 * 60 * 1000),
        line_notice_color: line_notice_color.unwrap_or(2),
        show_pre_fix_msg: show_pre_fix_msg.unwrap_or(false),
        // oracle 缺省值（`GameSvrConf.cs:1542-1543`；`ServerConf.cs:335-336`）。
        guild_msg_f_color: guild_msg_f_color.unwrap_or(0xDB),
        guild_msg_b_color: guild_msg_b_color.unwrap_or(0xFF),
        // oracle 缺省值（`GameSvrConf.cs:1476` 的 `MonsterPowerRate = 10`）。
        monster_power_rate: monster_power_rate.unwrap_or(10),
        // oracle 缺省值（`GameSvrConf.cs:1527` 的 `ScriptGotoCountLimit = 30`）。
        script_goto_count_limit: script_goto_count_limit.unwrap_or(30),
        // `GameSvrConf.cs:1233` 的 ctor 缺省。
        server_name: server_name.unwrap_or_else(|| "SKY引擎".to_string()),
        ids_addr,
        ids_port,
        db_addr,
        db_port,
        db_conn_string,
    })
}

/// oracle `ConfigFile.cs:188-203` 的 `ReadBool` 语义（`ServerConf.ReadWriteBool` 走这里）：
/// 去空白并 `ToUpper` 后**仅** `是`/`YES`/`1`/`TRUE` 为真；空与其余值一律假。
///
/// 与 [`parse_bool`] 的区别：后者对未识别值报错（用于必须写明的键）；本函数面向
/// oracle 的 `ReadWriteBool` 键——**未识别值不报错、静默取 false**（fail-closed）。
fn read_bool_or_false(v: &str) -> bool {
    matches!(
        v.trim().to_uppercase().as_str(),
        "是" | "YES" | "1" | "TRUE"
    )
}

/// `ReadWriteBool` 口径（`ConfigFile.cs` 的布尔读法）：`0/1` + `true/false`（大小写不敏感），其余报错。
/// `ReadWriteInteger` 的缺键/空值口径：空串 ⇒ oracle 返回 `def_value`；非法值报错（失败要响）。
fn read_i32_or_default(v: &str, def_value: i32, name: &str) -> Result<i32> {
    if v.is_empty() {
        return Ok(def_value);
    }
    v.parse()
        .with_context(|| format!("{name} 不是合法整数：{v:?}"))
}

/// `ReadWriteByte` 的缺键/空值口径（同 `read_i32_or_default`；越界按 byte 报错）。
fn read_u8_or_default(v: &str, def_value: u8, name: &str) -> Result<u8> {
    if v.is_empty() {
        return Ok(def_value);
    }
    v.parse()
        .with_context(|| format!("{name} 不是合法字节：{v:?}"))
}

fn parse_bool(v: &str, key: &str) -> Result<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "1" | "true" => Ok(true),
        "0" | "false" => Ok(false),
        other => bail!("{key} 不是布尔：{other:?}"),
    }
}

/// `WorldMaps=1000x1000[,200x200...]`。
fn parse_maps(v: &str) -> Result<Vec<(i32, i32)>> {
    let mut out = Vec::new();
    for part in v.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Some((w, h)) = part.split_once(['x', 'X']) else {
            bail!("WorldMaps 里 {part:?} 不是 WxH 形式");
        };
        let w: i32 = w.trim().parse().with_context(|| format!("宽非法：{w:?}"))?;
        let h: i32 = h.trim().parse().with_context(|| format!("高非法：{h:?}"))?;
        if w <= 0 || h <= 0 {
            bail!("地图尺寸必须为正：{part:?}");
        }
        out.push((w, h));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 写一份**部署形态**的 `Server.conf`：现网配置是 GBK（cp936）字节。
    /// 中文值（如 `ReadWriteBool` 的 `是`）必须按 GBK 落盘，否则 oracle 的
    /// `StreamReader(..., GB2312)` 会把它当 cp936 读成乱码（E-5 的口径）。
    fn write(dir: &Path, body: &str) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let p = dir.join("Server.conf");
        std::fs::write(&p, mir2_script::cp936::encode_gb2312_dotnet(body)).unwrap();
        p
    }

    /// 现网样件形态（注意 `GateAddr` 值前的空格——照 `Server.conf` 事实）。
    #[test]
    fn parses_deployed_shape() {
        let dir = std::env::temp_dir().join(format!("gs-conf-{}", std::process::id()));
        let p = write(
            &dir,
            "[DataBase]\nDBPort=6000\n[Server]\nDBPort=6000\nCheckBlock=12288\nGateAddr= 127.0.0.1\n\
             GatePort=5000\nIDSAddr= 127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\nWorldMaps=1000x1000,200x200\n[Share]\nX=1\n[Setup]\nShowRankLevelName=1\nShowGuildName=0\n",
        );
        let c = load(&p).unwrap();
        assert_eq!(c.gate_addr, "127.0.0.1", "值要 trim（现网带前导空格）");
        assert_eq!(c.gate_port, 5000);
        assert_eq!(c.check_block, 12288);
        assert_eq!(c.maps, vec![(1000, 1000), (200, 200)]);
        assert_eq!(c.ids_addr, "127.0.0.1", "IDSAddr 值也带前导空格，要 trim");
        assert_eq!(c.ids_port, 5600);
        assert_eq!(c.db_addr, "127.0.0.1");
        assert_eq!(c.db_port, 6000);
        assert!(
            c.show_rank_level_name,
            "[Setup] ShowRankLevelName=1（现网值）"
        );
        assert_eq!(
            c.hear_msg_f_color, 0x00,
            "缺 [Setup] HearMsgFColor ⇒ oracle 缺省 0x00（GameSvrConf.cs:1528）"
        );
        assert_eq!(
            c.hear_msg_b_color, 0xFF,
            "缺 [Setup] HearMsgBColor ⇒ oracle 缺省 0xFF（GameSvrConf.cs:1529）"
        );
        assert_eq!(
            c.envir_dir, "Envir",
            "缺 [Share] EnvirDir 时用 oracle 缺省值"
        );
        assert_eq!(c.map_dir, "Map", "缺 [Share] MapDir 时用 oracle 缺省值");
        assert_eq!(
            c.notice_dir, "Notice",
            "缺 [Share] NoticeDir 时用 oracle 缺省值（GameSvrConf.cs:1297）"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **P3 收尾残余 1**：`[Setup] MagTurnUndeadLevel`（oracle 缺省 50）与
    /// `[Setup] PKFlagTime`（oracle 缺省 60*1000）的装载。判据三档：
    /// 键=50 / 键=30 / 缺键 / 空值 ⇒ 缺键与空值都必须回落 oracle 缺省（不静默给 0）。
    ///
    /// 牙：删掉 `MAGTURNUNDEADLEVEL` 解析臂 ⇒ 键=30 用例读成 50 ⇒ 必红。
    #[test]
    fn mag_turn_undead_level_and_pk_flag_time_read_with_defaults() {
        let dir = std::env::temp_dir().join(format!("gs-conf-p3res-{}", std::process::id()));
        let nl = "\n";
        let base = format!(
            "[Server]{nl}GateAddr=127.0.0.1{nl}GatePort=5000{nl}CheckBlock=12288{nl}\
             IDSAddr=127.0.0.1{nl}IDSPort=5600{nl}DBAddr=127.0.0.1{nl}DBPort=6000{nl}"
        );
        // 键=50（oracle 缺省值也写出来时取文件值）。
        let p = write(
            &dir,
            &format!("{base}[Setup]{nl}MagTurnUndeadLevel=50{nl}PKFlagTime=60000{nl}"),
        );
        let c = load(&p).unwrap();
        assert_eq!(c.mag_turn_undead_level, 50);
        assert_eq!(c.pk_flag_time, 60_000);
        // 键=30 / 1200：文件值覆盖缺省。
        let p = write(
            &dir,
            &format!("{base}[Setup]{nl}MagTurnUndeadLevel=30{nl}PKFlagTime=1200{nl}"),
        );
        let c = load(&p).unwrap();
        assert_eq!(c.mag_turn_undead_level, 30, "键=30 必须取文件值");
        assert_eq!(c.pk_flag_time, 1200);
        // 缺键：oracle 源码缺省（GameSvrConf.cs:1773 / :1395）。
        let p = write(&dir, &format!("{base}[Setup]{nl}ShowRankLevelName=1{nl}"));
        let c = load(&p).unwrap();
        assert_eq!(c.mag_turn_undead_level, 50, "缺键 ⇒ 缺省 50");
        assert_eq!(c.pk_flag_time, 60_000, "缺键 ⇒ 缺省 60s");
        // 空值：同样回落缺省（任务口径；空串不得当成 0）。
        let p = write(
            &dir,
            &format!("{base}[Setup]{nl}MagTurnUndeadLevel={nl}PKFlagTime={nl}"),
        );
        let c = load(&p).unwrap();
        assert_eq!(c.mag_turn_undead_level, 50, "空值 ⇒ 缺省 50");
        assert_eq!(c.pk_flag_time, 60_000, "空值 ⇒ 缺省 60s");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `openmir2-m2-gamesvr-host`（sim 循环对接片）：`CM_PICKUP`/`CM_DROPITEM` 的 `ground::*` 输入
    /// 键（`GameSvrConf.HumanMaxGold` 等 6 个）。两条口径：
    /// ① 缺键 ⇒ 用 oracle 源码缺省（`GameSvrConf.cs:1451/1488/1703/1704/1706/1754`），不算错；
    /// ② 写了但非法（如 `DropItemRage=300` 超出 `byte`）⇒ 报错。
    #[test]
    fn setup_drop_pickup_keys_and_defaults() {
        let dir = std::env::temp_dir().join(format!("gs-conf-drop-{}", std::process::id()));
        let base = "[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=12288\nIDSAddr=127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n";
        let p = write(
            &dir,
            &format!(
                "{base}[Setup]\nHumanMaxGold=10000000\nFloorItemCanPickUpTime=120000\nControlDropItem=1\nInSafeDisableDrop=0\nCanDropPrice=500\nDropItemRage=3\nDieDropUseItemRate=50\nDieRedDropUseItemRate=15\nKillByHumanDropUseItem=1\nKillByMonstDropUseItem=1\nSendRefMsgRange=15\nSaveHumanRcdTime=1200000\n"
            ),
        );
        let c = load(&p).unwrap();
        assert_eq!(c.human_max_gold, 10_000_000);
        assert_eq!(c.floor_item_can_pick_up_time, 120_000);
        assert!(c.control_drop_item);
        assert!(!c.in_safe_disable_drop);
        assert_eq!(c.can_drop_price, 500);
        assert_eq!(c.drop_item_rage, 3);
        assert_eq!(c.die_drop_use_item_rate, 50, "部署件 DieDropUseItemRate=50");
        assert_eq!(
            c.die_red_drop_use_item_rate, 15,
            "部署件 DieRedDropUseItemRate=15"
        );
        assert!(
            c.kill_by_human_drop_use_item,
            "部署件 KillByHumanDropUseItem=1"
        );
        assert!(
            c.kill_by_monst_drop_use_item,
            "部署件 KillByMonstDropUseItem=1"
        );
        assert_eq!(c.send_ref_msg_range, 15, "[Setup] SendRefMsgRange 实值");
        assert_eq!(
            c.save_human_rcd_time, 1_200_000,
            "[Setup] SaveHumanRcdTime 实值（毫秒）"
        );

        // 缺键 ⇒ oracle 缺省（`SendRefMsgRange` 缺省 12，`GameSvrConf.cs:1503`）。
        let p2 = write(&dir, base);
        let c2 = load(&p2).unwrap();
        assert_eq!(c2.human_max_gold, 10_000_000);
        assert_eq!(c2.floor_item_can_pick_up_time, 120_000);
        assert!(!c2.control_drop_item);
        assert!(!c2.in_safe_disable_drop);
        assert_eq!(c2.can_drop_price, 500);
        assert_eq!(c2.drop_item_rage, 3);
        assert_eq!(
            c2.die_drop_use_item_rate, 30,
            "缺键 ⇒ DieDropUseItemRate 缺省 30"
        );
        assert_eq!(
            c2.die_red_drop_use_item_rate, 15,
            "缺键 ⇒ DieRedDropUseItemRate 缺省 15"
        );
        assert!(
            !c2.kill_by_human_drop_use_item,
            "缺键 ⇒ KillByHumanDropUseItem 缺省 false"
        );
        assert!(
            c2.kill_by_monst_drop_use_item,
            "缺键 ⇒ KillByMonstDropUseItem 缺省 true"
        );
        assert_eq!(c2.send_ref_msg_range, 12, "缺键 ⇒ SendRefMsgRange 缺省 12");
        assert_eq!(
            c2.save_human_rcd_time, 600_000,
            "缺键 ⇒ SaveHumanRcdTime 缺省 10*60*1000（GameSvrConf.cs:1484）"
        );

        // 写了但非法 ⇒ 必须报错（不静默截断）。
        let p3 = write(&dir, &format!("{base}[Setup]\nDropItemRage=300\n"));
        assert!(load(&p3).is_err(), "DropItemRage=300 超出 byte ⇒ 必须报错");
        let p5 = write(&dir, &format!("{base}[Setup]\nSendRefMsgRange=300\n"));
        assert!(
            load(&p5).is_err(),
            "SendRefMsgRange=300 超出 byte ⇒ 必须报错"
        );
        let p4 = write(&dir, &format!("{base}[Setup]\nControlDropItem=maybe\n"));
        assert!(load(&p4).is_err(), "ControlDropItem 非布尔 ⇒ 必须报错");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `[Setup] StartPointSize/HomeMap/HomeX/HomeY`（`WorldServer.cs:250-260` 的 `GetHomeInfo` 回落地图）：
    /// 现网实值 `2/3/839/668` 解析到 `GateConfig`；缺键 ⇒ oracle 源码缺省 `2/"0"/289/618`。
    #[test]
    fn setup_home_fallback_keys_and_defaults() {
        let dir = std::env::temp_dir().join(format!("gs-conf-home-{}", std::process::id()));
        let base = "[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=12288\nIDSAddr=127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n";
        let p = write(
            &dir,
            &format!("{base}[Setup]\nStartPointSize=2\nHomeMap=3\nHomeX=839\nHomeY=668\n"),
        );
        let c = load(&p).unwrap();
        assert_eq!(c.start_point_size, 2, "现网 StartPointSize=2");
        assert_eq!(c.home_map, "3", "现网 HomeMap=3");
        assert_eq!((c.home_x, c.home_y), (839, 668), "现网 HomeX/HomeY");

        let p2 = write(&dir, base);
        let c2 = load(&p2).unwrap();
        assert_eq!(
            c2.start_point_size, 2,
            "缺键 ⇒ 缺省 2（GameSvrConf.cs:1400）"
        );
        assert_eq!(c2.home_map, "0", "缺键 ⇒ 缺省 \"0\"（GameSvrConf.cs:1374）");
        assert_eq!((c2.home_x, c2.home_y), (289, 618), "缺键 ⇒ 缺省 289/618");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `[Setup] GuildMsgFColor/GuildMsgBColor`（`ServerConf.cs:335-336`；缺省 0xDB/0xFF）——
    /// G3 的 104 `Param` 低/高字节（`GuildConfig` 唯一来源，M-8）。
    #[test]
    fn guild_msg_colors_from_setup_section() {
        let dir = std::env::temp_dir().join(format!("gs-conf-guild-{}", std::process::id()));
        let base = "[Server]
GateAddr=127.0.0.1
GatePort=5000
CheckBlock=12288
IDSAddr=127.0.0.1
IDSPort=5600
DBAddr=127.0.0.1
DBPort=6000
";
        let p = write(
            &dir,
            &format!(
                "{base}[Setup]
GuildMsgFColor=10
GuildMsgBColor=11
"
            ),
        );
        let c = load(&p).unwrap();
        assert_eq!((c.guild_msg_f_color, c.guild_msg_b_color), (10, 11));
        let p2 = write(&dir, base);
        let c2 = load(&p2).unwrap();
        assert_eq!((c2.guild_msg_f_color, c2.guild_msg_b_color), (0xDB, 0xFF));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `[Setup] HearMsgFColor/HearMsgBColor`（现网 `Server.conf` 实值 `0` / `255`）：解析到
    /// `GateConfig` 供 `CM_SAY` 回帧 `SM_HEAR` 的 `Param` 低/高字节。
    #[test]
    fn hears_msg_colors_from_setup_section() {
        let dir = std::env::temp_dir().join(format!("gs-conf-hear-{}", std::process::id()));
        let p = write(
            &dir,
            "[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=12288\nIDSAddr=127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n[Setup]\nHearMsgFColor=0\nHearMsgBColor=255\n",
        );
        let c = load(&p).unwrap();
        assert_eq!(c.hear_msg_f_color, 0);
        assert_eq!(c.hear_msg_b_color, 255);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `[Setup] HealthFillTime/SpellFillTime`（`ServerConf.cs:152-153` 的 `ReadWriteInteger`）：
    /// 现网 `Server.conf:785-786` 的实值 `300`/`800` 解析到 `GateConfig`；缺键 ⇒ oracle 源码缺省
    /// `300`/`800`（`GameSvrConf.cs:1323-1324`）；写了但非整数 ⇒ 报错（不静默）。
    #[test]
    fn setup_stamina_fill_times_and_defaults() {
        let dir = std::env::temp_dir().join(format!("gs-stamina-cfg-{}", std::process::id()));
        let base = "[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=1\nIDSAddr=127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n";
        let p = write(
            &dir,
            &format!("{base}[Setup]\nHealthFillTime=250\nSpellFillTime=900\n"),
        );
        let c = load(&p).unwrap();
        assert_eq!(c.health_fill_time, 250, "[Setup] HealthFillTime 实值");
        assert_eq!(c.spell_fill_time, 900, "[Setup] SpellFillTime 实值");
        let p2 = write(&dir, base);
        let c2 = load(&p2).unwrap();
        assert_eq!(
            c2.health_fill_time, 300,
            "缺键 ⇒ HealthFillTime 缺省 300（GameSvrConf.cs:1323）"
        );
        assert_eq!(
            c2.spell_fill_time, 800,
            "缺键 ⇒ SpellFillTime 缺省 800（GameSvrConf.cs:1324）"
        );
        let p3 = write(&dir, &format!("{base}[Setup]\nHealthFillTime=abc\n"));
        assert!(load(&p3).is_err(), "HealthFillTime 非整数 ⇒ 必须报错");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **M3⑥-实现 3 第②片**：`[Setup] ScriptGotoCountLimit`（`ServerConf.cs:320` 的
    /// `ReadWriteInteger`；现网 `Server.conf` 写 `=10000`）——实值解析到 `GateConfig`；
    /// 缺键 ⇒ oracle 源码缺省 `30`（`GameSvrConf.cs:1527`）；写了但非整数 ⇒ 报错（不静默）。
    #[test]
    fn setup_script_goto_count_limit_and_default() {
        let dir = std::env::temp_dir().join(format!("gs-goto-cfg-{}", std::process::id()));
        let base = "[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=1\nIDSAddr=127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n";
        let p = write(
            &dir,
            &format!("{base}[Setup]\nScriptGotoCountLimit=10000\n"),
        );
        let c = load(&p).unwrap();
        assert_eq!(c.script_goto_count_limit, 10000, "现网实值");
        let p2 = write(&dir, base);
        let c2 = load(&p2).unwrap();
        assert_eq!(
            c2.script_goto_count_limit, 30,
            "缺键 ⇒ ScriptGotoCountLimit 缺省 30（GameSvrConf.cs:1527）"
        );
        let p3 = write(&dir, &format!("{base}[Setup]\nScriptGotoCountLimit=abc\n"));
        assert!(load(&p3).is_err(), "ScriptGotoCountLimit 非整数 ⇒ 必须报错");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **卡 `openmir2-persist-single-sided-columns`（G-10）**：`[Server] ServerIndex` 必须解析，
    /// 缺键回落 oracle 缺省 0，非法值报错（oracle `ReadWriteByte("Server","ServerIndex",…)`）。
    /// **F3**：空值（`ServerIndex=`）也按 oracle `Read<byte>` 的 `IsNullOrEmpty => defValue`
    /// 返回 0（旧实现 `v.parse()` 对空串报错）。红检：删掉 `v.is_empty()` 支 => 必红。
    #[test]
    fn server_index_parses_with_oracle_default() {
        let dir = std::env::temp_dir().join(format!("gs-svridx-{}", std::process::id()));
        let base = "[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=1\nIDSAddr=127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n";
        let p = write(&dir, base);
        assert_eq!(
            load(&p).unwrap().server_index,
            0,
            "缺 ServerIndex ⇒ oracle 缺省 0"
        );
        let p2 = write(&dir, &format!("{base}ServerIndex=7\n"));
        assert_eq!(load(&p2).unwrap().server_index, 7, "现网多机配置");
        let p3 = write(&dir, &format!("{base}ServerIndex=999\n"));
        assert!(load(&p3).is_err(), "ServerIndex 是 byte，999 必须报错");
        let p4 = write(&dir, &format!("{base}ServerIndex=\n"));
        assert_eq!(
            load(&p4).unwrap().server_index,
            0,
            "F3：空串 => oracle Read<byte> 缺省 0（不得报错）"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **B-1**（reviewer-2 `b49897e8`）：现网 `Server.conf` 首字节是 UTF-8 BOM（`ef bb bf`）。
    ///
    /// oracle `ConfigFile.Load` 用 `new StreamReader(path, GB2312)` 且**默认做 BOM 探测**
    /// （`ConfigFile.cs:263`）⇒ 首行 BOM 被吞掉，`[DataBase]` 段照常识别。Rust 若不剥 BOM，
    /// `[DataBase]` 头丢失 ⇒ `ConnctionString` 缺键 ⇒ 回落 `database=mir2`（现网无此库）
    /// ⇒ `stditems` 恒空 ⇒ **带装备角色的 50/41 与 oracle 偏离**（裸装夹具/真机号看不出来）。
    /// 本用例是 **BOM 剥离逻辑的 pin**（不是整条编码链的牙）：它只证明现网 `ef bb bf`
    /// 形态能被读通。更广的红检（GBK / UTF-16LE / 含 `0xFF` 夹具）落在
    /// `server_conf_bom_matrix_both_chains`。
    #[test]
    fn deployed_conf_with_utf8_bom_reads_database_section() {
        let dir = std::env::temp_dir().join(format!("gs-bom-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("Server.conf");
        // 首 3 字节照现网事实（`head -c 32 E:/MirServer/M2GameSvr/Server.conf | xxd` = `ef bb bf`）。
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(
            b"[DataBase]\nDBType=MySQL\nConnctionString=server=127.0.0.1;uid=root;pwd=;database=mir2_data;\n[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=1\nIDSAddr=127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n",
        );
        std::fs::write(&p, bytes).unwrap();
        let c = load(&p).expect("带 BOM 的现网形态必须能解析（BOM 只在文件首剥一个）");
        assert_eq!(
            c.db_conn_string.as_deref(),
            Some("server=127.0.0.1;uid=root;pwd=;database=mir2_data;"),
            "BOM 被吞后 [DataBase] ConnctionString 必须读到（现网 database=mir2_data）"
        );
        assert_eq!(c.gate_port, 5000, "BOM 后的 [Server] 段也要正常解析");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **红检 ①**（owner 裁决）：真 `Server.conf` **没有** `WorldMaps` ⇒ 必须能解析（`maps` 为空表）。
    ///
    /// 现网样件的 `[Server]` 段唯一缺的键就是 `WorldMaps`（`272582e1` 逐行核过）；把它当必需键
    /// 就等于"现网配置起不来"，所以它是可选键。"起得来"的另一半（有真 Envir ⇒ 用注册表）由
    /// `mapwire::envir_registry_covers_missing_worldmaps` 覆盖。
    #[test]
    fn deployed_conf_without_worldmaps_parses() {
        let dir = std::env::temp_dir().join(format!("gs-nowm-{}", std::process::id()));
        let p = write(
            &dir,
            "[Server]\nCheckBlock=12288\nGateAddr= 127.0.0.1\nGatePort=5000\n\
             IDSAddr= 127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n\
             [Share]\nEnvirDir=Envir\nMapDir=Map\nNoticeDir=Notice\n",
        );
        let c = load(&p).expect("缺 WorldMaps 不许报错（可选键）");
        assert!(
            !c.show_rank_level_name,
            "缺 [Setup] ShowRankLevelName ⇒ oracle 缺省 false"
        );
        assert!(c.maps.is_empty(), "没写 WorldMaps ⇒ 空表");
        assert_eq!(c.gate_port, 5000);
        assert_eq!(c.envir_dir, "Envir");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **红检 ③**：`WorldMaps` **写了但非法**（`10x0`）仍必须报错——可选≠不校验。
    #[test]
    fn bad_worldmaps_value_is_still_fatal() {
        let dir = std::env::temp_dir().join(format!("gs-badwm-{}", std::process::id()));
        let p = write(
            &dir,
            "[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=1\nWorldMaps=10x0\n\
             IDSAddr=127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n",
        );
        assert!(load(&p).is_err(), "WorldMaps=10x0 必须报错");
        let p2 = write(
            &dir,
            "[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=1\nWorldMaps=nope\n\
             IDSAddr=127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n",
        );
        assert!(load(&p2).is_err(), "WorldMaps 非 WxH 必须报错");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `[Share] EnvirDir/MapDir` 照 oracle 的键读（`ServerConf.cs:75-76`），值要 trim。
    #[test]
    fn parses_share_map_dirs() {
        let dir = std::env::temp_dir().join(format!("gs-share-{}", std::process::id()));
        let p = write(
            &dir,
            "[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=1\nWorldMaps=10x10\n\
             IDSAddr=127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n\
             [Share]\nEnvirDir= Envir2\nMapDir=Map2\n",
        );
        let c = load(&p).unwrap();
        assert_eq!(c.envir_dir, "Envir2");
        assert_eq!(c.map_dir, "Map2");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `[Share] NoticeDir` 照 oracle 的键读（`ServerConf.cs:77`），值要 trim。
    #[test]
    fn parses_share_notice_dir() {
        let dir = std::env::temp_dir().join(format!("gs-notice-cfg-{}", std::process::id()));
        let p = write(
            &dir,
            "[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=1\nWorldMaps=10x10\n\
             IDSAddr=127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n\
             [Share]\nNoticeDir= Notice2\n",
        );
        let c = load(&p).unwrap();
        assert_eq!(c.notice_dir, "Notice2", "NoticeDir 值要 trim");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **`openmir2-channel-chat`**：`[Setup] CryMsgFColor/CryMsgBColor/CanShoutMsgLevel`
    /// （`ServerConf.cs:278/327-328`；缺省 `0x00/0x97/7`）——`!` 喊话的帧颜色与等级门。
    ///
    /// 牙：删掉 `CRYMSGBCOLOR` 或 `CANSHOUTMSGLEVEL` 解析臂 ⇒ 现网值用例必红。
    #[test]
    fn parses_setup_cry_colors_and_can_shout_level_with_oracle_defaults() {
        let dir = std::env::temp_dir().join(format!("gs-setup-cry-{}", std::process::id()));
        let base = "[Server]
GateAddr=127.0.0.1
GatePort=5000
CheckBlock=1
                    IDSAddr=127.0.0.1
IDSPort=5600
DBAddr=127.0.0.1
DBPort=6000
";
        // 不写 ⇒ oracle 源码缺省。
        let p = write(&dir, base);
        let c = load(&p).unwrap();
        assert_eq!(c.cry_msg_f_color, 0x00, "GameSvrConf.cs:1534 缺省");
        assert_eq!(c.cry_msg_b_color, 0x97, "GameSvrConf.cs:1535 缺省");
        assert_eq!(c.can_shout_msg_level, 7, "GameSvrConf.cs:1455 缺省");
        // 现网 `Server.conf:354/598-599` 的值（34 / 0 / 151）。
        let p2 = write(
            &dir,
            &format!(
                "{base}[Setup]
CanShoutMsgLevel=34
CryMsgFColor=0
CryMsgBColor=151
"
            ),
        );
        let c2 = load(&p2).unwrap();
        assert_eq!(c2.can_shout_msg_level, 34);
        assert_eq!(c2.cry_msg_f_color, 0);
        assert_eq!(c2.cry_msg_b_color, 151);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **`openmir2-m2-session-lifecycle`**：`[Setup] RedMsgFColor/RedMsgBColor` 照
    /// `ServerConf.cs:333-334` 的 `ReadWriteByte` 读；**缺键用 oracle 缺省**（0xFF/0x38）不自造。
    #[test]
    fn parses_setup_red_msg_colors_with_oracle_defaults() {
        let dir = std::env::temp_dir().join(format!("gs-setup-colors-{}", std::process::id()));
        let base = "[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=1\n\
                    IDSAddr=127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n";
        // 不写 [Setup] ⇒ 缺省（现网 Server.conf: RedMsgFColor=255 / RedMsgBColor=56）。
        let p = write(&dir, base);
        let c = load(&p).unwrap();
        assert_eq!(c.red_msg_f_color, 0xFF, "GameSvrConf.cs:1540 缺省");
        assert_eq!(c.red_msg_b_color, 0x38, "GameSvrConf.cs:1541 缺省");
        // 写了就照写读，值带前导空格也要 trim。
        let p2 = write(
            &dir,
            &format!("{base}[Setup]\nRedMsgFColor= 255\nRedMsgBColor=56\n"),
        );
        let c2 = load(&p2).unwrap();
        assert_eq!(c2.red_msg_f_color, 255);
        assert_eq!(c2.red_msg_b_color, 56);
        // 写了但越界（300 > u8::MAX）⇒ 错误（不是静默取默认）。
        let p3 = write(&dir, &format!("{base}[Setup]\nRedMsgBColor=300\n"));
        assert!(load(&p3).is_err(), "RedMsgBColor=300 超出 u8 ⇒ 必须报错");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **`openmir2-m3-selloff-consignment-world`**：`[Setup] BlueMsgFColor/BlueMsgBColor` 照
    /// `ServerConf.cs:331-332` 的 `ReadWriteByte` 读；**缺键用 oracle 缺省**（0xFF/0xFC）不自造。
    #[test]
    fn parses_setup_blue_msg_colors_with_oracle_defaults() {
        let dir = std::env::temp_dir().join(format!("gs-setup-blue-{}", std::process::id()));
        let base = "[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=1\n\
                    IDSAddr=127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n";
        // 不写 `[Setup]` ⇒ oracle 缺省（`GameSvrConf.cs:1538-1539`）。
        let p = write(&dir, base);
        let c = load(&p).unwrap();
        assert_eq!(c.blue_msg_f_color, 0xFF, "GameSvrConf.cs:1538 缺省");
        assert_eq!(c.blue_msg_b_color, 0xFC, "GameSvrConf.cs:1539 缺省");
        // 现网 `Server.conf:602-603` 的值（251/252）——带前导空格也要 trim。
        let p2 = write(
            &dir,
            &format!("{base}[Setup]\nBlueMsgFColor= 251\nBlueMsgBColor=252\n"),
        );
        let c2 = load(&p2).unwrap();
        assert_eq!(c2.blue_msg_f_color, 251);
        assert_eq!(c2.blue_msg_b_color, 252);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **R3-2**：oracle `ConfigFile` 的 section/key 字典都是 `OrdinalIgnoreCase`（`ConfigFile.cs:13/301`）
    /// ⇒ `[server]`/`gateaddr`/`[share]`/`envirdir` 都必须认，不能把 oracle 认的键当没写。
    #[test]
    fn section_and_key_matching_is_ascii_case_insensitive() {
        let dir = std::env::temp_dir().join(format!("gs-case-{}", std::process::id()));
        let p = write(
            &dir,
            "[server]\ngateaddr=127.0.0.1\ngateport=5000\ncheckblock=1\nworldmaps=10x10\n\
             idsaddr=127.0.0.1\nidsport=5600\ndbaddr=127.0.0.1\ndbport=6000\n\
             [share]\nenvirDir= Envir3\nMAPDIR=Map3\n",
        );
        let c = load(&p).unwrap();
        assert_eq!(c.gate_port, 5000, "[server]/gateport 小写必须认");
        assert_eq!(c.envir_dir, "Envir3", "[share]/envirDir 混合大小写必须认");
        assert_eq!(c.map_dir, "Map3", "[share]/MAPDIR 大写必须认");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 缺键/非法值必须报错（M1④"失败要响"）。
    #[test]
    fn missing_or_bad_keys_are_fatal() {
        let dir = std::env::temp_dir().join(format!("gs-bad-{}", std::process::id()));
        let p = write(&dir, "[Server]\nGateAddr=127.0.0.1\n");
        assert!(
            load(&p).is_err(),
            "缺 GatePort/CheckBlock 必须报错（`WorldMaps` 已降为可选，不再计入缺键）"
        );
        let p2 = write(
            &dir,
            "[Server]\nGateAddr=127.0.0.1\nGatePort=abc\nCheckBlock=1\nWorldMaps=10x10\n",
        );
        assert!(load(&p2).is_err(), "GatePort 非数字必须报错");
        let p3 = write(
            &dir,
            "[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=1\nWorldMaps=10x0\n",
        );
        assert!(load(&p3).is_err(), "地图尺寸非正必须报错");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **P3-B5 must-fix**：`DisableInSafeZoneFireCross` 的布尔解析要按 oracle
    /// `ConfigFile.cs:188-203` 的 `ReadBool`：大写后仅 `是`/`YES`/`1`/`TRUE` 为真；
    /// 空 ⇒ 缺省 false；其余（含 `False`/`no`/`off`/垃圾）一律 false。
    /// 旧实现 `!matches!(v, "0"|"false"|"FALSE"|"")` 对 `False`/`no`/`off`/垃圾 fail-open。
    #[test]
    fn disable_in_safe_zone_fire_cross_read_bool() {
        for v in ["是", "YES", "yes", "1", "TRUE", "true", " 1 ", " Yes "] {
            assert!(read_bool_or_false(v), "{v:?} ⇒ oracle ReadBool 为真");
        }
        for v in [
            "False", "false", "FALSE", "no", "NO", "off", "OFF", "垃圾", "0", "", "  ",
        ] {
            assert!(
                !read_bool_or_false(v),
                "{v:?} ⇒ oracle ReadBool 为假（fail-closed）"
            );
        }
        // 键缺失 ⇒ 缺省 false；键存在且为 `False` 也必须是 false（不看缺省口径）。
        let dir = std::env::temp_dir().join(format!("gs-bool-{}", std::process::id()));
        let base = "[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=1\nIDSAddr=127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n";
        let p_missing = write(&dir, base);
        assert!(
            !load(&p_missing).unwrap().disable_in_safe_zone_fire_cross,
            "缺键 ⇒ oracle 缺省 false"
        );
        for (v, want) in [
            ("1", true),
            ("是", true),
            ("YES", true),
            ("TRUE", true),
            ("False", false),
            ("no", false),
            ("off", false),
            ("垃圾", false),
            ("", false),
        ] {
            let p = write(
                &dir,
                &format!("{base}[Setup]\nDisableInSafeZoneFireCross={v}\n"),
            );
            assert_eq!(
                load(&p).unwrap().disable_in_safe_zone_fire_cross,
                want,
                "DisableInSafeZoneFireCross={v:?}"
            );
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **PL-1 / P32**：`[Setup] RevivalTime`（`ServerConf.cs:503` 的 `ReadWriteInteger`；
    /// `GameSvrConf.cs:1725` 源码缺省 `60*1000`）——实值解析、缺键回落缺省、非整数报错。
    /// 红检：把缺省改成 0 ⇒ 第二条断言必红；把缺键分支写成报错 ⇒ 第二条必红。
    #[test]
    fn setup_revival_time_and_defaults() {
        let dir = std::env::temp_dir().join(format!("gs-revival-cfg-{}", std::process::id()));
        let base = "[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=1\nIDSAddr=127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n";
        let p = write(&dir, &format!("{base}[Setup]\nRevivalTime=12345\n"));
        let c = load(&p).unwrap();
        assert_eq!(c.revival_time, 12345, "[Setup] RevivalTime 实值");
        let p2 = write(&dir, base);
        let c2 = load(&p2).unwrap();
        assert_eq!(
            c2.revival_time, 60_000,
            "缺键 ⇒ oracle 源码缺省 60*1000（GameSvrConf.cs:1725）"
        );
        let p3 = write(&dir, &format!("{base}[Setup]\nRevivalTime=abc\n"));
        assert!(load(&p3).is_err(), "RevivalTime 非整数 ⇒ 必须报错");
        std::fs::remove_dir_all(&dir).ok();
    }
    /// **PL-3 / P26**：`[Setup] HungerSystem/HungerDecHP` 的 `ReadBool` 口径
    /// （`是/YES/1/TRUE` 为真；其余含 `False`/垃圾一律假；缺键回落 oracle 缺省 false）。
    ///
    /// 牙：删掉两个解析臂 ⇒ `=1` 用例读成 false ⇒ 必红。
    #[test]
    fn hunger_keys_use_read_bool_semantics() {
        let dir = std::env::temp_dir().join(format!("gs-conf-hunger-{}", std::process::id()));
        let base = "[Server]
DBPort=6000
CheckBlock=1
GateAddr=127.0.0.1
GatePort=5000
                    IDSAddr=127.0.0.1
IDSPort=5600
DBAddr=127.0.0.1
DBPort=6000
";
        for (body, want) in [
            (
                "[Setup]
HungerSystem=1
HungerDecHP=TRUE
",
                (true, true),
            ),
            (
                "[Setup]
HungerSystem=是
HungerDecHP=YES
",
                (true, true),
            ),
            (
                "[Setup]
HungerSystem=0
HungerDecHP=False
",
                (false, false),
            ),
            (
                "[Setup]
HungerSystem=garbage
",
                (false, false),
            ),
            (
                "[Setup]
",
                (false, false),
            ),
        ] {
            let p = write(&dir, &format!("{base}{body}"));
            let c = load(&p).unwrap();
            assert_eq!((c.hunger_system, c.hunger_dec_hp), want, "body={body:?}");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **Minor 3（2026-10-21）**：`[Server] TestServer` / `ServiceMode` 的解析语义。
    ///
    /// - `TestServer`：`ServerConf.cs:23` 用 `OrdinalIgnoreCase` ⇒ `TRUE/true/True` 都算 true。
    /// - `ServiceMode`：`ServerConf.cs:27` 用 `String.CompareTo("TRUE") == 0`——culture-aware
    ///   （**不是** `StringComparison.Ordinal`），对 ASCII 大小写敏感 ⇒ **只认大写 `TRUE`**。
    ///
    /// 牙：把 `SERVICEMODE` 臂改成 `eq_ignore_ascii_case` ⇒ 小写用例第二条必红；
    /// 把 `TESTSERVER` 臂改成 `v == "TRUE"` ⇒ `true`/`True` 两条必红。
    #[test]
    fn parses_test_server_ignoring_case_and_service_mode_exact_uppercase() {
        let dir = std::env::temp_dir().join(format!("gs-conf-svc-{}", std::process::id()));
        let nl = "\n";
        let base = format!(
            "[Server]{nl}GateAddr=127.0.0.1{nl}GatePort=5000{nl}CheckBlock=12288{nl}\
             IDSAddr=127.0.0.1{nl}IDSPort=5600{nl}DBAddr=127.0.0.1{nl}DBPort=6000{nl}"
        );
        // (TestServer 原值, ServiceMode 原值, 期望 (test_server, service_mode))
        for (ts, sm, want) in [
            ("TRUE", "TRUE", (true, true)),
            ("true", "TRUE", (true, true)),
            ("True", "TRUE", (true, true)),
            ("FALSE", "TRUE", (false, true)),
            // ServiceMode 只认大写 TRUE：小写/混合大小写/false 都是 false。
            ("TRUE", "true", (true, false)),
            ("TRUE", "True", (true, false)),
            ("TRUE", "FALSE", (true, false)),
            ("TRUE", "1", (true, false)),
            // 值前后的空白照 `ReadWriteString` 口径 trim。
            (" true ", " TRUE ", (true, true)),
            // 缺键 ⇒ false。
            ("", "", (false, false)),
        ] {
            let body = if ts.is_empty() && sm.is_empty() {
                String::new()
            } else if sm.is_empty() {
                format!("TestServer={ts}{nl}")
            } else {
                format!("TestServer={ts}{nl}ServiceMode={sm}{nl}")
            };
            let p = write(&dir, &format!("{base}{body}"));
            let c = load(&p).unwrap();
            assert_eq!(
                (c.test_server, c.service_mode),
                want,
                "TestServer={ts:?} ServiceMode={sm:?}"
            );
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **E-5 BOM 矩阵（两条链同口径）**：`.NET ConfigFile.Load` =
    /// `new StreamReader(File.Open(...), Encoding.GetEncoding("GB2312"))`（`ConfigFile.cs:263`）。
    /// GB2312 的 preamble 长度为 0 ⇒ `StreamReader` 做 BOM 探测：UTF-8 BOM / UTF-16LE BOM /
    /// UTF-32LE（`FF FE 00 00`）都能读；无 BOM 按 cp936；cp936 的 `0xFF` 解成 `U+F8F5`。
    ///
    /// 两条链都要能读且关键键值一致：`game_svr::config::load`（`[DataBase] ConnctionString` +
    /// `[Server] ServerName`）与 `mir2_envir::GameSvrConf::from_bytes`（同一文件的 `[Names]`）。
    ///
    /// 牙：把 `load` 改回 `read_to_string` ⇒ GBK / UTF-16LE / 0xFF 三份夹具红；把
    /// `decode_config_text` 改回严格 UTF-8 ⇒ UTF-8 BOM 之外的夹具红（与 envir 链同源）。
    #[test]
    fn server_conf_bom_matrix_both_chains() {
        use mir2_script::cp936::encode_gb2312_dotnet;
        let dir = std::env::temp_dir().join(format!("gs-bom-matrix-{}", std::process::id()));
        let base = "[DataBase]\n\
             ConnctionString=server=127.0.0.1;uid=root;pwd=;database=mir2_data;\n\
             [Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=1\n\
             IDSAddr=127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n\
             ServerName=传奇\n[Names]\nGameGold=元宝\n";
        // 「含 0xFF 字节」夹具：cp936 的 U+F8F5 ↔ 0xFF；服务名末尾带一个 0xFF。
        let text_ff = base.replace("ServerName=传奇", "ServerName=传奇\u{F8F5}");
        let gbk = encode_gb2312_dotnet(base);
        let gbk_ff = encode_gb2312_dotnet(&text_ff);
        assert!(gbk_ff.contains(&0xFF), "夹具必须真的含 0xFF 字节");
        let mut utf8_bom = vec![0xEF, 0xBB, 0xBF];
        utf8_bom.extend_from_slice(base.as_bytes());
        let mut utf16le = vec![0xFF, 0xFE];
        for u in base.encode_utf16() {
            utf16le.extend_from_slice(&u.to_le_bytes());
        }
        for (name, bytes, want_name) in [
            ("gbk-no-bom", gbk, "传奇"),
            ("utf8-bom", utf8_bom, "传奇"),
            ("utf16le-bom", utf16le, "传奇"),
            ("gbk-0xff", gbk_ff, "传奇\u{F8F5}"),
        ] {
            std::fs::create_dir_all(&dir).unwrap();
            let p = dir.join("Server.conf");
            std::fs::write(&p, &bytes).unwrap();
            let c =
                load(&p).unwrap_or_else(|e| panic!("{name}: game_svr::config::load 必须能读：{e}"));
            assert_eq!(
                c.db_conn_string.as_deref(),
                Some("server=127.0.0.1;uid=root;pwd=;database=mir2_data;"),
                "{name}: [DataBase] ConnctionString"
            );
            assert_eq!(c.server_name, want_name, "{name}: [Server] ServerName");
            // 第二条链：envir 用同一个 decode_config_text 口径。
            let e = mir2_envir::GameSvrConf::from_bytes(&bytes);
            assert_eq!(
                e.game_gold_name, "元宝",
                "{name}: envir 链 [Names] GameGold"
            );
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
