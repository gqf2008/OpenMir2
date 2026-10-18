# 出站 `SM_*` 帧逐字段/逐字节审计（2026-10-18）

- 审计对象：`bins/game-svr` 宿主发出的全部 `SM_*` 帧（编码层 `smframe.rs` / `smframe_ext.rs` + 各手工拼帧点）。
- 基线：`03208e20e`（master，2026-10-18）。
- 性质：**只读静态审计 + 本地单测复核**；未起服务、未占端口、未改共享主检出（本报告在独立 worktree 内提交）。
- oracle：仓库内冻结的 C# 实现 `src/`（`MakeMessage`/`SendDefMessage`/`SendSocket`/`EDCode` 及各 `RM_*` 观察者支）。
- 结论摘要：`smframe*.rs` 的**字段位映射整体正确**；发现 **6 条 `❌` 级缺陷**（G-1…G-6；其中 G-1/G-2/G-3 为客户端可见的高影响，G-6 是唯一确证的头字段错值）与 **9 条低影响口径差/接线/文档差**（G-7…G-15）。逐条见 §5。

---

## 0. 方法与证据

### 0.1 已执行的证据命令

| # | 命令 | 输出 |
|---|---|---|
| E1 | `cargo test -p game-svr --lib smframe::` | `test result: ok. 36 passed; 0 failed; 0 ignored; 0 measured; 286 filtered out`（2026-10-18） |
| E2 | 回读 `tests/golden/session-20261010-c3-hook/capture.jsonl`（真 C# 服务端 + 真客户端，`port=7200 && dir=s2c`） | `enter_world_sequence_matches_golden_order_and_bytes` 把入图前 16 帧**逐字节**重建（含长度），全绿 |
| E3 | oracle 构造点逐条实测 | 见 §2 的行号表（用脚本从 `src/M2Server/Player/*.cs`、`src/M2Server/Actor/BaseObject.cs` 提取 `MakeMessage`/`SendDefMessage` 所在行） |
| E4 | 固定输入的手算期望字节 vs Rust 实得 | 见 §5 每条缺口的「可复现验证」 |

### 0.2 tier 规则（按 AGENTS.md「判据分级」）

- **R**：帧载荷直接承载战斗数值 / HP/MP / 经验 / 伤害 / 持久 / 落库相关字段 ⇒ 必须真机全量对拍（或显式 `window_required`）。
- **L**：纯字段打包、纯编码、UI/文本帧 ⇒ 逐行/语义等价 + 抽样冒烟即可。

> 说明：本报告是**静态审计**；矩阵里的 tier 是「该行若要判定 GREEN 所需的最低验收级别」，不是本报告的结论等级。

### 0.3 链路口径（先钉死，后面所有行都以此为准）

1. `CommandMessage`（`src/OpenMir2/Packets/ClientPackets/ClientMesaagePacket.cs:11-24`）：
   `Recog:i32 | Ident:u16 | Param:u16 | Tag:u16 | Series:u16`，小端，定长 12B。Rust 对应 `smframe::command_header`。
2. `Messages.MakeMessage(int msg, int recog, int param, int tag, int series)`（`src/OpenMir2/Messages.cs:846-857`）：
   把 `param/tag/series` 按 `(ushort)` **截断**（低 16 位），`recog` 存 i32（**不截断**）。Rust `command_header` 的 `param/tag/series` 是 `u16`、`recog` 是 `i32`，逐位一致。
3. `SendDefMessage(ident, recog, param, tag, series)`（`PlayObject.Socket.cs:17-25`）⇒ 12B 头 + 无尾。
4. `SendDefMessage(..., sMsg)`（`PlayObject.Socket.cs:27-43`）⇒ 非空 `sMsg` 时尾 = `EDCode.EncodeString(sMsg)`；空 `sMsg` ⇒ **header-only**（不是不发帧）。
5. `SendSocket(CommandMessage defMsg, string sMsg)`（`PlayObject.Socket.cs:101-116`）⇒ 尾 = `HUtil32.GetBytes(sMsg)` **原样**（调用方若已 `Encode*`，这里不再编码）。
6. 网关 `GameGate/Services/ClientSession.cs:891-920`：`bufferLen < 0` ⇒ `'#' + 原样 + '!'`（短包）；否则 `'#' + EDCode.Encode(头 12B) + 尾原样 + '!'`。Rust `smframe::client_frame` 只覆盖后者（与冻结客户端一致）。
7. `HUtil32.MakeWord(byte low, byte high)` = `high<<8 | low`（`HUtil32.cs:61-64`）；**另有** `MakeWord(ushort, ushort)` = `bLow | bHigh<<8`（`:66-69`）。**C# 重载解析**决定用哪一个：实参是 `byte` ⇒ byte 重载；实参是 `ushort`/`int` ⇒ ushort 重载（`ushort → byte` 无隐式转换）。这是 G-6 的根因。
8. `HUtil32.LoWord(int)` = 低 16 位；`HiWord(int)` = `(ushort)(v >> 16)`（算术右移后截断）；`MakeLong(ushort low, ushort high)` = `low | high<<16`。

---

## 1. Rust 侧清册

### 1.1 `bins/game-svr/src/smframe.rs`（公共编码器）

| # | 符号 | ident | 头字段来源 | 尾口径 |
|---|---|---|---|---|
| 1 | `sm_newmap` | 51 | `(recog=ActorId, param=CurrX, tag=CurrY, series=DayBright())` | `encode(MapFileName)` |
| 2 | `sm_logon` | 50 | `(ActorId, CurrX, CurrY, MakeWord(Dir,Light))` | `encode(MessageBodyWL 16B)` |
| 3 | `sm_featurechanged` | 41 | `(recog, LoWord(feature), HiWord(feature), FeatureEx)` | 无 |
| 4 | `sm_attackmode` | 213 | `(recog=mode, 0,0,0)` | 无 |
| 5 | `sm_additem` | 200 | `(ActorId, 0,0,1)` | **无（缺 `ClientItem`，G-1）** |
| 6 | `sm_delitem` | 202 | `(ActorId, 0,0,1)` | 调用方传已编码的 `ClientItem` 尾 |
| 7 | `sm_itemhide` | 611 | `(recog=ItemId, param=x, tag=y, series=0)` | 无 |
| 8 | `sm_weightchanged` | 622 | `(Weight, WearWeight, HandWeight, chk)` | 无 |
| 9 | `sm_healthspellchanged` / `_client` / `_frame` | 53 | `(ActorId, HP, MP, MaxHP)` | oracle：无；客户端布局：`encode(TCharDesc 12B)` |
| 10 | `sm_winexp` | 44 | `(Exp 总量, LoWord(delta), HiWord(delta), 0)` | 无 |
| 11 | `sm_levelup` | 45 | `(Exp 总量, Level, 0,0)` | 无 |
| 12 | `sm_goldchanged` | 653 | `(Gold, LoWord(GameGold), HiWord(GameGold), 0)` | 无 |
| 13 | `sm_menu_ok` | 767 | `(recog, 0,0,0)` | `encode(Msg)` |
| 14 | `sm_dropitem_reply` | 600/601 | `(recog=MakeIndex, 0,0,0)` | `encode(Msg)` |
| 15 | `sm_outofconnection` | 528 | `(0,0,0,0)` | 无 |
| 16 | `sm_version_fail` | 1106 | `(CRC1, LoWord(CRC2), HiWord(CRC2), 0)` | 字面量 `b"<<<<<<"` |
| 17 | `sm_sendnotice` | 658 | `(2000, 0,0,0)` | `encode(notice)` |
| 18 | `sm_sysmessage` | 100 | `(ActorId, MakeWord(fColor,bColor), 0, 1)` | `encode(msg)` |
| 19 | `sm_serverconfig` | 11029 | `(0,0,0,0)` | `encode(encode(ClientConf))`（双编码） |
| 20 | `sm_username` | 42 | `(targetId, nameColor, 0, 0)` | `encode(ShowName)` |
| 21 | `sm_ghost` | 803 | `(targetId, x, y, 0)` | 无 |
| 22 | `sm_senduserstate` | 751 | `(0,0,0,0)` | 已编码的 `UserStateInfo` 原样 |
| 23 | `sm_changenamecolor` | 656 | `(ActorId, color, 0, 0)` | 无 |
| 24 | `sm_areastate` | 766 | `(n8, 0,0,0)` | 无 |
| 25 | `sm_mapdescription` | 54 | `(nMusicid, 0,0,0)` | `encode(MapDesc)` |
| 26 | `sm_gamegoldname` | 5008 | `(GameGold, LoWord(GamePoint), HiWord(GamePoint), 0)` | `encode(msg)` |
| 27 | `sm_ability` | 52 | `(Gold, MakeWord((byte)Job,99), LoWord(GameGold), HiWord(GameGold))` | `encode(WAbil 40B)`（oracle）/ 58B（D-9 客户端布局） |
| 28 | `sm_subability` | 752 | `(MakeLong(MakeWord(AntiMagic,0),0), MakeWord(Hit,Speed), MakeWord(AntiPoison,PoisonRecover), MakeWord(Health,Spell))` | 无 |
| 29 | `sm_adjust_bonus` | 811 | `(BonusPoint, 0,0,0)` | `encode(A)+'/'+encode(B)+'/'+encode(C)` |
| 30 | `sm_daychanging` | 46 | `(0, Bright, DayBright(), 0)` | 无 |
| 31 | `sm_turn` | 10 | `(ActorId, x, y, MakeWord(Dir,Light))` | `encode(CharDesc 8B)` + 可选 `encode(Msg/color)` |
| 32 | `sm_move`/`sm_walk`/`sm_run`/`sm_horserun` | 11/13/5010 | `(ActorId, x, y, MakeWord(Dir,Light))` | `encode(CharDesc 8B)` |
| 33 | `sm_action_hit` | 14/15/16/18/19/24/25/26/8/4/43 | `(ActorId, x, y, Dir)` | **无（HEAVYHIT 缺尾，G-5）** |
| 34 | `sm_spell` | 17 | `(ActorId, x, y, Effect)` | `MagicId.to_string()` 原样 |
| 35 | `sm_magicfire` | 638 | `(ActorId, x, y, MakeWord(EffectType,Effect))` | `encode(i32 targetActorId)` |
| 36 | `sm_magic_lvexp` | 640 | `(MagicId, Level, LoWord(TranPoint), HiWord(TranPoint))` | 无 |
| 37 | `sm_durachange` | 642 | `(Dura, idx, DuraMax, 0)` | 无 |
| 38 | `sm_struck` / `_client` / `_frame` | 31 | `(ActorId, HP, MaxHP, damage)` | `encode(MessageBodyWL 16B)`（oracle：Param1=feature；客户端布局：Param1=MaxHP） |
| 39 | `sm_death` | 32/34 | `(ActorId, x, y, Dir)` | `encode(CharDesc 8B)` |
| 40 | `sm_movefail` / `_client` / `_frame` | 28 | `(ActorId, CurrX, CurrY, Dir)` | oracle：`encode(CharDesc 8B)`；客户端布局：12B `TCharDesc` |
| 41 | `sm_disappear` | 30 | `(ActorId, 0,0,0)` | 无 |
| 42 | `sm_clearobjects` | 633 | `(0,0,0,0)` | 无 |
| 43 | `sm_hear` | 40 | `(ActorId, MakeWord(f,b), 0, 1)` | `encode(ChrName + ':' + Msg)` |
| 44 | `sm_merchantsay` | 643 | `(recog, MakeWord(n1,n2), 0, 1)` | `encode(ChrName+'/'+Msg)` |
| 45 | `sm_groupmodechanged` | 659 | `(0, param, 0, 0)` | 无 |
| 46 | `sm_creategroup_ok` | 660 | `(0,0,0,0)` | 无 |
| 47 | `sm_creategroup_fail` | 661 | `(recog∈{-1..-4}, 0,0,0)` | 无 |
| 48 | `sm_groupaddmem_ok` | 662 | `(0,0,0,0)` | 无 |
| 49 | `sm_groupaddmem_fail` | 664 | `(recog∈{-1,-5,-2,-3,-4},0,0,0)` | 无 |
| 50 | `sm_groupdelmem_ok` | 663 | `(0,0,0,0)` | `encode(name)` |
| 51 | `sm_groupdelmem_fail` | 665 | `(recog∈{-1,-2,-3},0,0,0)` | 无 |
| 52 | `sm_groupcancel` | 666 | `(0,0,0,0)` | 无 |
| 53 | `sm_groupmembers` | 667 | `(0,0,0,0)` | `encode("a/b/c/")` |
| 54 | `sm_skeleton` | 33 | `(ActorId, x, y, Dir)` | `encode(CharDesc 8B)` |
| 55 | `sm_alive` | 27 | `(ActorId, x, y, Dir)` | `encode(CharDesc 8B)` |
| 56 | `sm_digup` | 20 | `(ActorId, x, y, MakeWord(Dir,Light))` | `encode(MessageBodyWL{feature,status,0,0})` |
| 57 | `sm_digdown` | 21 | `(ActorId, x, y, 0)` | 无 |
| 58 | `sm_flyaxe` | 22 | `(ActorId, x, y, Dir)` | `encode(MessageBodyW{tx,ty,LoWord,HiWord})` |
| 59 | `sm_lighting` | 23 | `(ActorId, x, y, Dir)` | `encode(MessageBodyWL{tx,ty,targetId,wParam})` |
| 60 | `sm_spacemove_hide` | 800/806 | `(ActorId, 0,0,0)` | 无 |
| 61 | `sm_spacemove_show` | 801/807 | `(ActorId, x, y, MakeWord(Dir,Light))` | `encode(CharDesc)` + 可选 `encode(Msg/color)` |

> 说明：`sm_healthspellchanged_frame` / `sm_struck_frame` / `sm_movefail_frame` 是**选路器**（`MIR2_CLIENT_ABILITY_LAYOUT` / `MIR2_CLIENT_MOVEFAIL_LAYOUT`），不新增 ident。

### 1.2 `bins/game-svr/src/smframe_ext.rs`（A 类批，28 条）

| # | 符号 | ident | 头字段 | 尾 |
|---|---|---|---|---|
| 1 | `sm_716` | 716 | `(ActorId, n1, n2, n3)` | 无 |
| 2 | `sm_breakweapon` | 1102 | `(ActorId,0,0,0)` | 无 |
| 3 | `sm_changeface` | 1104 | `(n1, LoWord(n2), HiWord(n2), 0)` | `encode(CharDesc 8B)` |
| 4 | `sm_changelight` | 654 | `(ActorId, Light, (u16)nClientKey, 0)` | 无 |
| 5 | `sm_changemap` | 634 | `(ActorId, CurrX, CurrY, DayBright())` | `encode(Msg)` |
| 6 | `sm_charstatuschanged` | 657 | `(ActorId, LoWord(status), HiWord(status), HitSpeed)` | 无 |
| 7 | `sm_closedoor` | 614 | `(0, n1, n2, 0)` | 无 |
| 8 | `sm_closehealth` | 1101 | `(ActorId,0,0,0)` | 无 |
| 9 | `sm_hideevent` | 805 | `(n1, wParam, n2, n3)` | 无 |
| 10 | `sm_instancehealguage` / `_client` / `_frame` | 1103 | `(ActorId, HP, MaxHP, 0)` | oracle 无；客户端布局 `encode(ShortMessage 4B)` |
| 11 | `sm_itemshow` | 610 | `(n1, n2, n3, wParam)` | `encode(Msg)` |
| 12 | `sm_lampchangedura` | 655 | `(n1,0,0,0)` | 无 |
| 13 | `sm_magicfire_fail` | 639 | `(ActorId,0,0,0)` | 无 |
| 14 | `sm_makedrug_fail` | 714 | `(n1,0,0,0)` | 无 |
| 15 | `sm_makedrug_success` | 713 | `(n1,0,0,0)` | 无 |
| 16 | `sm_merchantdlgclose` | 644 | `(n1,0,0,0)` | 无 |
| 17 | `sm_movemessage` | 99 | `(ActorId, MakeWord(n1,n2), n3, wParam)` | `encode(Msg)` |
| 18 | `sm_mystatus` | 708 | `(0, (short)status, 0, 0)` | 无 |
| 19 | `sm_opendoor_ok` | 612 | `(0, n1, n2, 0)` | 无 |
| 20 | `sm_openhealth` / `_client` / `_frame` | 1100 | `(ActorId, HP, MaxHP, 0)` | oracle 无；客户端布局 `encode(ShortMessage 4B)` |
| 21 | `sm_password` | 5009 | `(0,0,0,0)` | 无 |
| 22 | `sm_playdice` | 8001 | `(ActorId, wParam, 0, 0)` | `encode(MessageBodyWL 16B)` + `encode(Msg)` |
| 23 | `sm_queryybdeal` | 23023 | `(n1, n2, n3, wParam)` | Msg 原样 |
| 24 | `sm_queryybsell` | 23021 | `(n1, n2, n3, wParam)` | Msg 原样 |
| 25 | `sm_reconnect` | 802 | `(0,0,0,0)` | `encode(Msg)` |
| 26 | `sm_sendusermakedrugitemlist` | 712 | `(n1, n2, 0, 0)` | `encode(Msg)` |
| 27 | `sm_showevent` | 804 | `(n1, wParam, n2, n3)` | `encode(ShortMessage{HiWord(n2),0})` |
| 28 | `sm_whisper` | 103 | `(ActorId, MakeWord(n1,n2), 0, 1)` | `encode(Msg)` |

### 1.3 手工拼帧点（不在 `smframe*.rs` 内）

| 落点 | 产出的 ident | 说明 |
|---|---|---|
| `main.rs::emit_def_frames` | 任何 `inbound::DefFrame` | `command_header(f.recog, f.ident, f.param as u16, f.tag as u16, f.series as u16)` + 无尾（`SendDefMessage` 5 参口径） |
| `main.rs::emit_shop_frames` | `ShopFrame` 全族 | `tail_encoded ? 原样 : encode(tail)`（`main.rs` 内联拼接） |
| `main.rs` CM_EAT 回帧 | 635/636 + 通知族（53/622/767/100/657/708） | `gm_data_body(command_header(...), &[])` |
| `downlink.rs` | 10/41/6/7/9/11/13/5010/31/30/32/34/637 | 全部转调 `smframe::*`；自带 `wire_coord`（**633 不在此**，见 `sessionwire.rs`） |
| `sessionwire.rs` | 633/634/801/807/654/5009 | 转调 `smframe*` |
| `bag.rs` | 615/616/619/620 | `Frame{ident,recog,param,tag,series}` |
| `useitem.rs` | 635/636 | 同上 |
| `shop.rs` | 645/646/652/668/700/701/702/703/704/705/706/707/709/713/714/647/648/649/650/651/669/670/671 | `ShopFrame`，尾口径见 §3.3 |
| `trade.rs` | 673/674/675/676/677/678/681/682/683/684/685/686/687/200/202 | `DealEvent` → `gm_data_body`/`sm_sysmessage`/`sm_menu_ok` |
| `selloff.rs` | 23001/23003/23005/23008/23010/23014/23016/23018/23027/5008/100/767 | `SellOffFrame` |
| `itemwire.rs` | 201 | `SM_BAGITEMS` 头 + 原样尾 |
| `inbound.rs` | 710/711 | `DefFrame` |
| **缺失** | 621 / 211 / 203 / 1102 / 1104 / 655 / 8001 / 23021 / 23023 / 1101 | 无任何发射点（见 §5） |

---

## 2. oracle 侧清册（行号逐条实测）

| ident | 值 | oracle 构造点（`SendDefMessage`/`MakeMessage`） |
|---|---|---|
| SM_NEWMAP | 51 | `PlayObject.Message.cs:1681` |
| SM_LOGON | 50 | `PlayObject.cs:1131` |
| SM_FEATURECHANGED | 41 | `PlayObject.cs:1145`（SendLogon）、`PlayObject.Message.cs:1558`（RM_TURN 尾）、`:1763`（RM_FEATURECHANGED） |
| SM_ATTACKMODE | 213 | `PlayObject.cs:1146` |
| SM_ADDITEM | 200 | `PlayObject.Base.cs:226` |
| SM_DELITEM | 202 | `PlayObject.Base.cs:1663` |
| SM_DELITEMS | 709 | `PlayObject.Base.cs:1644` |
| SM_ITEMHIDE | 611 | `PlayObject.Message.cs:1748` |
| SM_WEIGHTCHANGED | 622 | `PlayObject.Message.cs:1760` |
| SM_HEALTHSPELLCHANGED | 53 | `PlayObject.Message.cs:1737` |
| SM_WINEXP | 44 | `PlayObject.Message.cs:1667` |
| SM_LEVELUP | 45 | `PlayObject.Message.cs:1671` |
| SM_GOLDCHANGED | 653 | `PlayObject.Message.cs:1834` |
| SM_MENU_OK | 767 | `PlayObject.Message.cs:1953` |
| SM_DROPITEM_SUCCESS / FAIL | 600/601 | `PlayObject.Message.cs:804` / `:808` |
| SM_OUTOFCONNECTION | 528 | `PlayObject.cs:1086` |
| SM_VERSION_FAIL | 1106 | `PlayObject.Message.cs:1690` |
| SM_SENDNOTICE | 658 | `PlayObject.cs:1076` |
| SM_SYSMESSAGE | 100 | `PlayObject.Message.cs:1715`（RM_SYSMESSAGE）、`:1718`（RM_GROUPMESSAGE） |
| SM_SERVERCONFIG | 11029 | `PlayObject.Base.cs:969` |
| SM_USERNAME | 42 | `PlayObject.Message.cs:1663`、`PlayObject.Operate.cs:28` |
| SM_GHOST | 803 | `PlayObject.Operate.cs:33` |
| SM_SENDUSERSTATE | 751 | `PlayObject.Operate.cs:110` |
| SM_CHANGENAMECOLOR | 656 | `PlayObject.Message.cs:1678` |
| SM_AREASTATE | 766 | `PlayObject.Base.cs:285` |
| SM_MAPDESCRIPTION | 54 | `PlayObject.Base.cs:894` |
| SM_GAMEGOLDNAME | 5008 | `PlayObject.Base.cs:938` |
| SM_ABILITY | 52 | `PlayObject.Message.cs:1673`、`:1733` |
| SM_SUBABILITY | 752 | `PlayObject.Message.cs:1675`、`:1935` |
| SM_ADJUST_BONUS | 811 | `PlayObject.Base.cs:1208` |
| SM_DAYCHANGING | 46 | `PlayObject.Message.cs:1741` |
| SM_TURN | 10 | `PlayObject.Message.cs:1542` |
| SM_WALK / RUN / HORSERUN | 11/13/5010 | `PlayObject.Message.cs:1406/1426/1416` |
| SM_HIT…SM_43 | 14/15/16/18/19/24/25/26/8/4/43 | `PlayObject.Message.cs:1436/1443/1450/1464/1478/1485/1492/1499/1506/1513/1520` |
| SM_SPELL | 17 | `PlayObject.Message.cs:1457` |
| SM_MAGICFIRE | 638 | `PlayObject.Message.cs:1789` |
| SM_MAGICFIRE_FAIL | 639 | `PlayObject.Message.cs:1795` |
| SM_MAGIC_LVEXP | 640 | `PlayObject.Message.cs:1801` |
| SM_DURACHANGE | 642 | `PlayObject.Message.cs:1804` |
| SM_STRUCK | 31 | `PlayObject.Message.cs:1601` |
| SM_DEATH / NOWDEATH | 32/34 | `PlayObject.Message.cs:1644` / `:1633` |
| SM_MOVEFAIL | 28 | `PlayObject.Message.cs:1469` |
| SM_DISAPPEAR | 30 | `PlayObject.Message.cs:1652` |
| SM_CLEAROBJECTS | 633 | `PlayObject.Message.cs:1772` |
| SM_HEAR | 40 | `PlayObject.Message.cs:1706`、`:1712` |
| SM_MERCHANTSAY | 643 | `PlayObject.Message.cs:1724` |
| SM_GROUPMODECHANGED | 659 | `PlayObject.Message.cs:923`/`:927` |
| SM_CREATEGROUP_OK/FAIL | 660/661 | `PlayObject.Operate.cs:839` / `:815/820/825/830` |
| SM_GROUPADDMEM_OK/FAIL | 662/664 | `PlayObject.Operate.cs:877` / `:852/857/862/867/872` |
| SM_GROUPDELMEM_OK/FAIL | 663/665 | `PlayObject.Operate.cs:904` / `:890/895/900` |
| SM_GROUPCANCEL | 666 | `PlayObject.cs:5150`、`PlayObject.Message.cs:1849` |
| SM_GROUPMEMBERS | 667 | `PlayObject.Base.cs:1365` |
| SM_SKELETON | 33 | `PlayObject.Message.cs:1656` |
| SM_ALIVE | 27 | `PlayObject.Message.cs:1885` |
| SM_DIGUP/DIGDOWN | 20/21 | `PlayObject.Message.cs:1892` / `:1901` |
| SM_FLYAXE | 22 | `PlayObject.Message.cs:1912` |
| SM_LIGHTING | 23 | `PlayObject.Message.cs:1924` |
| SM_SPACEMOVE_HIDE/HIDE2 | 800/806 | `PlayObject.Message.cs:1959` / `:1963` |
| SM_SPACEMOVE_SHOW/SHOW2 | 801/807 | `PlayObject.Message.cs:1971` / `:1975` |
| SM_716 | 716 | `PlayObject.Message.cs:1929` |
| SM_BREAKWEAPON | 1102 | `PlayObject.Message.cs:2016` |
| SM_CHANGEFACE | 1104 | `PlayObject.Message.cs:2024` |
| SM_CHANGELIGHT | 654 | `PlayObject.Message.cs:1840` |
| SM_CHANGEMAP | 634 | `PlayObject.Message.cs:1776` |
| SM_CHARSTATUSCHANGED | 657 | `PlayObject.Message.cs:1846` |
| SM_CLOSEDOOR | 614 | `PlayObject.Message.cs:1754` |
| SM_CLOSEHEALTH | 1101 | `PlayObject.Message.cs:2013` |
| SM_HIDEEVENT | 805 | `PlayObject.Message.cs:1993` |
| SM_INSTANCEHEALGUAGE | 1103 | `PlayObject.Message.cs:2019` |
| SM_ITEMSHOW | 610 | `PlayObject.Message.cs:1745` |
| SM_LAMPCHANGEDURA | 655 | `PlayObject.Message.cs:1843` |
| SM_MAKEDRUG_SUCCESS/FAIL | 713/714 | `PlayObject.Message.cs:1879` / `:1882` |
| SM_MERCHANTDLGCLOSE | 644 | `PlayObject.Message.cs:1807` |
| SM_MOVEMESSAGE | 99 | `PlayObject.Message.cs:1727` |
| SM_MYSTATUS | 708 | `PlayObject.Message.cs:1950` |
| SM_OPENDOOR_OK | 612 | `PlayObject.Message.cs:1751` |
| SM_OPENHEALTH | 1100 | `PlayObject.Message.cs:2010` |
| SM_PASSWORD | 5009 | `PlayObject.Message.cs:2032` |
| SM_PLAYDICE | 8001 | `PlayObject.Message.cs:2039` |
| SM_QUERYYBSELL/DEAL | 23021/23023 | `PlayObject.Message.cs:2050` / `:2054` |
| SM_RECONNECT | 802 | `PlayObject.Message.cs:1990` |
| SM_SENDUSERMAKEDRUGITEMLIST | 712 | `PlayObject.Message.cs:1876` |
| SM_SHOWEVENT | 804 | `PlayObject.Message.cs:1999` |
| SM_WHISPER | 103 | `PlayObject.Message.cs:1709` |
| SM_TAKEON_OK/FAIL | 615/616 | `PlayObject.Operate.cs:458` / `:495` |
| SM_TAKEOFF_OK/FAIL | 619/620 | `PlayObject.Operate.cs:547` / `:574` |
| SM_EAT_OK/FAIL | 635/636 | `PlayObject.Operate.cs:690` / `:698` |
| SM_STORAGE_OK/FULL/FAIL | 701/702/703 | `PlayObject.Operate.cs:1634` / `:1643` / `:1652` |
| SM_TAKEBACKSTORAGEITEM_OK/FULLBAG/FAIL | 705/707/706 | `PlayObject.Operate.cs:1688` / `:1697` / `:1711` |
| SM_DEALMENU | 673 | `PlayObject.Base.cs:2414` |
| SM_DEALTRY_FAIL | 674 | `PlayObject.Operate.cs:954`/`:960` |
| SM_DEALADDITEM_OK/FAIL | 675/676 | `PlayObject.Base.cs:2385` / `PlayObject.Operate.cs:997` |
| SM_DEALREMOTEADDITEM | 682 | `PlayObject.Base.cs:2397` |
| SM_DEALDELITEM_OK/FAIL | 677/678 | `PlayObject.Base.cs:2375` / `:2379`（另 `Operate.cs:1006/1040/1054`） |
| SM_DEALREMOTEDELITEM | 683 | `PlayObject.Base.cs:2370` |
| SM_DEALCHGGOLD_OK/FAIL | 684/685 | `PlayObject.Operate.cs:1071` / `:1059`、`:1081` |
| SM_DEALREMOTECHGGOLD | 686 | `PlayObject.Operate.cs:1072` |
| SM_DEALSUCCESS | 687 | `PlayObject.Operate.cs:1177`/`:1184` |
| SM_DEALCANCEL | 681 | `PlayObject.Base.cs:737` |
| SM_SELLOFF* / SM_SellOffCANCEL / SM_SENDDEALOFFFORM | 230xx | `PlayObject.Message.cs:2046-2107` |
| SM_BAGITEMS | 201 | `PlayObject.Operate.cs:61` |
| SM_SENDGOODSLIST | 645 | `PlayObject.Message.cs:1809-1810`（`SendDefMessage(nParam1,nParam2,0,0,Msg)`） |
| SM_SENDUSERSELL | 646 | `PlayObject.Message.cs:1812-1813` |
| SM_SENDDETAILGOODSLIST | 652 | `PlayObject.Message.cs:1830-1831` |
| SM_SENDBUYPRICE | 647 | `PlayObject.Message.cs:1815-1816` |
| SM_SENDUSERSTORAGEITEM | 700 | `PlayObject.Message.cs:1866-1867` |
| SM_SENDREPAIRCOST 等 | 668-671 | `PlayObject.Message.cs:1855-1863` |
| SM_USERSELLITEM_OK/FAIL、SM_BUYITEM_SUCCESS/FAIL、SM_USERREPAIRITEM_OK/FAIL | 648-651/669/670 | `PlayObject.Message.cs:1819-1828` |
| SM_SAVEITEMLIST | 704 | `PlayObject.Base.cs:1620` |
| SM_READMINIMAP_OK/FAIL | 710/711 | `PlayObject.Operate.cs:1208` / `:1212` |
| SM_BUTCH | 637 | `PlayObject.Message.cs:1784` |
| SM_SENDUSEITEMS | 621 | `PlayObject.Base.cs:1047` |
| SM_SENDMYMAGIC | 211 | `PlayObject.Base.cs:1067` |

---

## 3. 矩阵（行 = ident）

图例：**✅** 逐字段一致；**⚠️** 有意偏离（开关可回 oracle）；**❌** 缺陷/缺口；**—** oracle 侧不可达/未发射。

### 3.1 入图/登录序（golden 16 帧逐字节）

| ident | oracle 构造点 | 字段语义 | Rust 落点 | 一致 | 证据 | tier | 判据 |
|---|---|---|---|---|---|---|---|
| 658 | `PlayObject.cs:1076` | recog=**2000**；param/tag/series=0；尾=`EncodeString(notice)`；空串仍发 header-only | `sm_sendnotice` | ✅ | golden #1；`empty_notice_still_sends_header_only_658` | L | 空/非空两态逐字节 |
| 51 | `Message.cs:1681` | recog=ActorId；param=CurrX；tag=CurrY；series=DayBright()；尾=`EncodeString(MapFileName)` | `sm_newmap` | ✅ | golden #2 `sm_newmap_matches_golden_frame_2` | L | 逐字节 |
| 50 | `PlayObject.cs:1131` | recog=ActorId；param=CurrX；tag=CurrY；series=`MakeWord(Dir,Light)`；尾=`EncodePacket(MessageBodyWL 16B)` | `sm_logon` | ✅ | golden #3 | L | 逐字节 |
| 41 | `PlayObject.cs:1145`；`Message.cs:1763` | recog=ActorId；param=LoWord(feature)；tag=HiWord(feature)；series=FeatureEx；无尾 | `sm_featurechanged` | ✅ | golden #4 | L | 逐字节 |
| 213 | `PlayObject.cs:1146` | recog=`(byte)AttatckMode`（**不是 param**）；其余 0 | `sm_attackmode` | ✅ | golden #5 + `attack_mode_goes_in_recog_not_param`（非 0 值钉死） | L | 非 0 值字段位 |
| 11029 | `Base.cs:969` | 头全 0；尾=`EncodeString(EncodeMessage(ClientConf))`（双编码） | `sm_serverconfig` | ✅ | golden #6（线缆 31B→单解 23B→双解 17B） | L | 双编码长度链 |
| 42 | `Message.cs:1663`；`Operate.cs:28` | recog=targetId；param=GetChrColor；tag/series=0；尾=`EncodeString(GetShowName)` | `sm_username` | ✅ | golden #7 | L | 逐字节 |
| 766 | `Base.cs:285` | recog=n8（FightZone=1/SafeArea=2/InGuildWarArea=4）；其余 0 | `sm_areastate` | ✅ | golden #8 | L | 逐字节 |
| 54 | `Base.cs:894` | recog=nMusicid（无 MUSIC ⇒ -1）；其余 0；尾=`EncodeString(MapDesc)` | `sm_mapdescription` | ✅ | golden #9 | L | 逐字节 |
| 5008 | `Base.cs:938` | recog=GameGold；param=LoWord(GamePoint)；tag=HiWord(GamePoint)；series=0；尾=`EncodeString(名称)` | `sm_gamegoldname` | ✅ | golden #10 | L | 逐字节 |
| 1106 | `Message.cs:1690` | recog=ClientFile1_CRC；param=LoWord(CRC2)；tag=HiWord(CRC2)；尾=字面量 `"<<<<<<"`（**不编码**） | `sm_version_fail` | ✅ | golden #11 + `version_fail_field_layout` | L | 逐字节 |
| 52 | `Message.cs:1673/1733` | recog=Gold；param=`MakeWord((byte)Job,99)`；tag=LoWord(GameGold)；series=HiWord(GameGold)；尾=`EncodeMessage(WAbil)` | `sm_ability` | ⚠️ | golden #12（oracle 40B 尾）；D-9 默认 58B 客户端布局，须 pin `MIR2_CLIENT_ABILITY_LAYOUT=0` 才逐字节 | R | 真机窗口 + 显式 pin |
| 752 | `Message.cs:1675/1935` | recog=`MakeLong(MakeWord(AntiMagic,0),0)`；param=`MakeWord(Hit,Speed)`；tag=`MakeWord(AntiPoison,PoisonRecover)`；series=`MakeWord(Health,Spell)` | `sm_subability` | ❌ | golden #12（recog=1）；**G-6**：`AntiMagic>255` 时高字节被丢 | R | 见 §5-G6 |
| 811 | `Base.cs:1208` | recog=BonusPoint；其余 0；尾=`EncodeMessage(A)+'/'+EncodeMessage(B)+'/'+EncodeMessage(C)` | `sm_adjust_bonus` | ✅ | golden #14；三段各自编码 | L | 逐字节 |
| 46 | `Message.cs:1741` | recog=0；param=Bright；tag=DayBright()；series=0 | `sm_daychanging` | ✅ | golden #15 | L | 逐字节 |
| 10 | `Message.cs:1542` | recog=ActorId；param=x；tag=y；series=`MakeWord(Dir,Light)`；尾=`Encode(CharDesc 8B)` + 可选 `EncodeString(Msg/color)` | `sm_turn` | ✅ | golden #16 | L | 逐字节（含两段拼接） |

**登录序里 oracle 会发、Rust 不发的帧**：

| ident | oracle 点 | 语义 | Rust | 一致 | 影响 |
|---|---|---|---|---|---|
| 654 | `Message.cs:1840`（RM_LOGON 里 `SendMsg(RM_CHANGELIGHT)`） | recog=ActorId；param=Light；tag=(short)nClientKey；无尾 | 无登录发射点（仅换装重算 `sessionwire::changelight_frame`） | ❌ | 客户端从 SM_LOGON 的 `HiByte(series)` 已得 Light ⇒ **自视角无影响**；其他观察者视角需 654（G-7） |
| 621 | `Base.cs:1047` | recog/param/tag/series 全 0；尾=`i + '/' + EncodeBuffer(ClientItem) + '/'`（逐槽） | **无** | ❌ | 客户端装备面板为空（G-3，高） |
| 211 | `Base.cs:1067` | recog=0；series=`MagicList.Count`；尾=`EncodeBuffer(ClientMagic) + '/'`（逐条） | **无** | ❌ | 客户端技能列表为空（G-3，高） |

### 3.2 移动 / 战斗 / 受击 / 死亡

| ident | oracle 点 | 字段语义 | Rust | 一致 | 证据 | tier | 判据 |
|---|---|---|---|---|---|---|---|
| 11/13/5010 | `Message.cs:1406/1426/1416` | recog=ActorId；param=x；tag=y；series=`MakeWord(wParam,Light)`；尾=`Encode(CharDesc)` | `sm_move`/`sm_walk`/`sm_run`/`sm_horserun` | ✅ | 单测 + 金标准 11/13 | L | 逐字节 |
| 10（RM_PUSH/RUSH/RUSHKUNG 支） | `Message.cs:1533-1542` | 同 walk；`SM_BACKSTEP(9)`/`SM_RUSH(6)`/`SM_RUSHKUNG(7)` 的方向 = `nBackDir`/`nDir`（不是角色 Dir） | `downlink::backstep_frame`/`rush_frame`/`move_frame` | ✅ | `downlink` 单测（series=`MakeWord(dir,light)`） | L | 逐字节 |
| 14/15/16/18/19/24/25/26/8/4/43 | `Message.cs:1436-1520` | recog=ActorId；param=x；tag=y；series=Dir | `sm_action_hit` | ✅（普通攻击） | `sm_action_hit` 单测 | R | 逐字节 |
| 15（挖矿） | `Message.cs:1443` + `Base.cs:1586` | 尾 = `processMsg.Msg` **原样**；挖矿命中 ⇒ `s1C="1"` | `sm_action_hit` **无尾参数** | ❌ | G-5 | R | 见 §5-G5 |
| 17 | `Message.cs:1457` | recog=ActorId；param=x；tag=y；series=Effect；尾=`MagicId.ToString()`（ASCII 原样，不编码） | `sm_spell` | ✅ | `spell_and_magicfire_field_layout` | R | 逐字节 |
| 638 | `Message.cs:1789` | recog=ActorId；param=LoWord(packed x/y)；tag=HiWord(packed)；series=`MakeWord(EffectType,Effect)`；尾=`Encode(i32 target)` | `sm_magicfire` | ✅ | 同上 | R | 逐字节 |
| 639 | `Message.cs:1795` | recog=ActorId；其余 0 | `sm_magicfire_fail` | ✅ | 单测 | L | 逐字节 |
| 640 | `Message.cs:1801` | recog=MagicId；param=Level；tag=LoWord(TranPoint)；series=HiWord(TranPoint) | `sm_magic_lvexp` | ✅ | `magic_lvexp_and_durachange_field_layout` | R | 逐字节 |
| 642 | `Message.cs:1804` | recog=Dura；param=idx；tag=LoWord(DuraMax)；series=HiWord(DuraMax) | `sm_durachange` | ✅ | 同上 | R | 逐字节 |
| 31 | `Message.cs:1601` | recog=受击者 ActorId；param=HP；tag=MaxHP；series=damage；尾=`Encode(MessageBodyWL{Param1=GetFeature,Param2=CharStatus,Tag1=hitter,Tag2=mag})` | `sm_struck`/`_client`/`_frame` | ⚠️ | golden 31（param=15/tag=19/series=2，尾 22B）；D-9 默认客户端布局，pin `=0` 才逐字节 | R | 真机窗口 + pin |
| 32/34 | `Message.cs:1644/1633` | recog=死者 ActorId；param=x；tag=y；series=Dir；尾=`Encode(CharDesc)` | `sm_death` | ✅ | 单测 | R | 逐字节 |
| 28 | `Message.cs:1469` | recog=**收件人自己** ActorId；param=自己 x；tag=自己 y；series=自己 Dir；尾=`Encode(CharDesc{GetFeatureToLong,CharStatus})` | `sm_movefail`/`_client`/`_frame` | ✅（默认 oracle） | `sm_movefail_oracle_layout_is_8b_chardesc` + `..._defaults_to_oracle_layout` | L | 逐字节（12B 变体仅显式开关） |
| 30 | `Message.cs:1652` | recog=对象 ActorId；其余 0 | `sm_disappear` | ✅ | golden 30（15 条） | L | 逐字节 |
| 633 | `Message.cs:1772` | 全 0 | `sm_clearobjects` | ✅ | `sm_clearobjects_is_header_only_all_zero` | L | 逐字节 |
| 33/27 | `Message.cs:1656/1885` | recog=ActorId；param=x；tag=y；series=Dir；尾=`Encode(CharDesc 8B)` | `sm_skeleton`/`sm_alive` | ✅ | `sm_skeleton_frame`/`sm_alive_frame` | L | 逐字节 |
| 20 | `Message.cs:1892` | recog=ActorId；param=x；tag=y；series=`MakeWord(wParam,Light)`；尾=`Encode(MessageBodyWL{feature,status,0,0})`（`Tag1` 被 oracle 覆盖为 0） | `sm_digup` | ✅ | `sm_digup_frame_has_zero_tag1` | L | 逐字节 |
| 21 | `Message.cs:1901` | recog=ActorId；param=x；tag=y；series=0；无尾 | `sm_digdown` | ✅ | `sm_digdown_frame_header_only` | L | 逐字节 |
| 22 | `Message.cs:1912` | recog=ActorId；param=x；tag=y；series=Dir；尾=`Encode(MessageBodyW{targetX,targetY,LoWord,HiWord})` | `sm_flyaxe` | ✅ | `sm_flyaxe_frame` | L | 逐字节（目标门由调用方） |
| 23 | `Message.cs:1924` | recog=ActorId；param=x；tag=y；series=**baseObject.Dir**（不是 wParam）；尾=`Encode(MessageBodyWL{targetX,targetY,targetId,wParam})` | `sm_lighting` | ✅ | `sm_lighting_series_is_caster_dir` | L | 逐字节 |
| 800/806 | `Message.cs:1959/1963` | recog=ActorId；其余 0 | `sm_spacemove_hide` | ✅ | `sm_spacemove_hide_frame_header_only` | L | 逐字节 |
| 801/807 | `Message.cs:1971/1975` | recog=ActorId；param=x；tag=y；series=`MakeWord(wParam,Light)`；尾=`Encode(CharDesc)` + 可选 `EncodeString(Msg/color)` | `sm_spacemove_show` | ✅ | `sm_spacemove_show_frame_with_and_without_name` | L | 逐字节 |

### 3.3 物品 / 背包 / 商店 / 交易 / 寄售

| ident | oracle 点 | 字段语义 | Rust | 一致 | 证据 | tier | 判据 |
|---|---|---|---|---|---|---|---|
| 200 | `Base.cs:226` | recog=ActorId；param/tag=0；series=1；尾=`EncodeBuffer(ClientItem 124B)` | `sm_additem`（header-only）在 `main.rs`（卸下/换装）、`shop.rs`（507/806/1036）、`ground.rs`→`pickwire`；`trade.rs`/`selloff.rs` 已带完整尾 | ❌ | **G-1**；`ClientItem` 编码器已存在（`itemwire::encode_client_item`，124B） | R | 客户端可见 |
| 202 | `Base.cs:1663` | recog=ActorId；series=1；尾=`EncodeBuffer(ClientItem)`（`Dura` 可为 0） | `sm_delitem(actor, tail)` | ✅ | `slave::deleted_amulet_body` + `delitem_field_layout_and_tail_passthrough` | L | 逐字节 |
| 203 | `Base.cs:1683/1704/1725` | recog=ActorId；series=1；尾=`EncodeBuffer(ClientItem)` | **无发射点/无编码器** | ❌ | **G-2**（穿衣后装备显示不刷新） | L | 逐字节 |
| 709 | `Base.cs:1644` | recog=0；param=0；tag=0；series=count；尾=`EncodeString("Name/MakeIndex/"×n)` | `shop::del_items_frame` | ✅ | `shop` 单测 | L | 逐字节 |
| 611 | `Message.cs:1748` | recog=ItemId；param=x；tag=y；series=0 | `sm_itemhide` | ✅ | `pickwire` 单测 | L | 逐字节 |
| 622 | `Message.cs:1760` | recog=Weight；param=WearWeight；tag=HandWeight；series=`((W+W+H)^0x3A5F)^0x1F35^0xaa21`（u16） | `sm_weightchanged` | ✅ | 单测 + `ability.rs` 重算同源 | R | 逐字节 |
| 615/616 | `Operate.cs:458/495` | OK：recog=GetFeatureToLong；param=GetFeatureEx；其余 0。FAIL：recog=n18；其余 0 | `bag.rs` | ✅ | `bag-probe` 21 用例 | L | 逐字段 |
| 619/620 | `Operate.cs:547/574` | 同上（n10） | `bag.rs` | ✅ | 同上 | L | 逐字段 |
| 635/636 | `Operate.cs:690/698` | 全 0 | `useitem.rs` | ✅ | `eat-probe` 22 用例 | L | 逐字段 |
| 701/702/703 | `Operate.cs:1634/1643/1652` | 全 0 | `shop.rs` | ✅ | `shop` 单测 | L | 逐字段 |
| 705/707/706 | `Operate.cs:1688/1697/1711` | OK：recog=nItemIdx；其余 0。FULLBAG/FAIL：全 0 | `shop.rs` | ✅ | `shop` 单测 | L | 逐字段 |
| 673 | `Base.cs:2414` | recog/param/tag/series=0；尾=`EncodeString(DealCreat.ChrName)` | `trade.rs` | ✅ | `trade` 单测 | L | 逐字节 |
| 674 | `Operate.cs:954/960` | 全 0 | `trade.rs` | ✅ | `trade` 单测 | L | 逐字节 |
| 675/676 | `Base.cs:2385` / `Operate.cs:997` | 全 0 | `trade.rs` | ✅ | `trade` 单测 | L | 逐字节 |
| 682/683 | `Base.cs:2397/2370` | recog=ActorId；series=1；尾=`EncodeBuffer(ClientItem)` | `trade::remote_item_body` | ✅ | `trade` 单测 | L | 逐字节 |
| 677/678 | `Base.cs:2375/2379` + `Operate.cs:1006/1040/1054` | 全 0 | `trade.rs` | ✅ | `trade` 单测 | L | 逐字节 |
| 684/685/686 | `Operate.cs:1071/1059/1072` | 684：recog=DealGolds；param=LoWord(Gold)；tag=HiWord(Gold)；series 0。686：recog=DealGolds；其余 0 | `trade.rs` | ✅ | `trade` 单测（含 `Card` 支） | L | 逐字段 |
| 687/681 | `Operate.cs:1177/1184`；`Base.cs:737` | 全 0 | `trade.rs` | ✅ | `trade` 单测 | L | 逐字节 |
| 23001 | `Message.cs:2046` | recog=商人 ActorId；param=0；尾=`EncodeString(Msg)`（空则 header-only） | `selloff.rs` | ✅ | `selloff` 单测 | L | 逐字段 |
| 23003/23005/23008/23010/23014/23016/23018/23027 | `Message.cs:2079-2107` | oracle 发送点全部 `SendMsg(...,0,0,0,0)` ⇒ 全 0；尾=Msg 原样（空） | `selloff.rs` | ✅ | `selloff` 单测 | L | 逐字节 |
| 23021/23023 | `Message.cs:2050/2054` | recog/param/tag/series=n1..wParam（现网全 0）；尾=Msg 原样（已编码列表文本） | `sm_queryyb*`（**无接线**） | ❌ | **G-13**（脚本 `[querysell]`/`[querydeal]` 列表不下发） | L | 接线后逐字节 |
| 201 | `Operate.cs:61` | recog=ActorId；param=0；tag=0；series=`ItemList.Count`；尾=`EncodeBuffer(ClientItem)+'/'`×n（原样） | `itemwire.rs` | ✅ | `itemwire` 124B 布局单测 | L | 逐字节 |
| 645 | `Message.cs:1809` | recog=商人 ActorId；param=n10（件数）；tag/series 0；尾=`EncodeString("name/sub/price/stock/"×)` | `shop.rs` | ✅ | `shop` 单测 | L | 逐字节 |
| 646 | `Message.cs:1812` | recog=商人 ActorId；param=n2；尾=`EncodeString(Msg)` | `shop.rs` | ✅ | `shop` 单测 | L | 逐字节 |
| 652 | `Message.cs:1830` | recog=商人 ActorId；param=nItemCount；tag=nInt；尾=`EncodeString(已编码 ClientItem+'/' 拼接)` | `shop.rs` | ✅ | `shop` 单测 | L | 逐字节 |
| 647/648/649/650/651/668/669/670/671/700 | `Message.cs:1815-1866` | 见各构造点；`SM_SENDBUYPRICE` recog=nC；`SM_SENDUSERREPAIR` recog=商人/param=0；`SM_SENDUSERSTORAGEITEM` recog=商人/param=0 | `shop.rs` | ✅ | `shop` 单测 | L | 逐字段 |
| 704 | `Base.cs:1620` | recog=**merchantId**；param=0；tag=p（页）；series=`(page-1)<0?0:page-1`；尾=已编码 `EncodeBuffer(ClientItem)+'/'` 原样 | `shop::back_item_list` | ✅ | `back_item_list_pages_fifty_and_tags_series` | L | 逐字节 |
| 712/713/714 | `Message.cs:1876/1879/1882` | 712：recog=n1；param=n2；尾=`EncodeString(Msg)`；713/714：recog=n1 | `shop.rs`/`smframe_ext` | ✅ | `shop` 单测 | L | 逐字段 |
| 710/711 | `Operate.cs:1208/1212` | 710：recog=0；param=Envir.MinMap；其余 0。711：全 0 | `inbound.rs` | ✅ | `inbound` 单测 | L | 逐字段 |
| 637 | `Message.cs:1784` | recog=ActorId（`!=0` 门）；param=x；tag=y；series=Dir；无尾 | `downlink::butch_frame` | ✅ | `downlink` 单测 | L | 逐字节 |

### 3.4 聊天 / 系统 / 组队 / 其它

| ident | oracle 点 | 字段语义 | Rust | 一致 | 证据 | tier | 判据 |
|---|---|---|---|---|---|---|---|
| 40 | `Message.cs:1706/1712` | recog=说话者 ActorId；param=`MakeWord(n1,n2)`；tag=0；series=1；尾=`EncodeString(Msg)` | `sm_hear` | ✅ | `main.rs` 单测 | L | 逐字节 |
| 103 | `Message.cs:1709` | recog=ActorId；param=`MakeWord(n1,n2)`；tag=0；series=1；尾=`EncodeString(Msg)` | `sm_whisper` | ✅ | `sm_whisper_header_makeword_series1_and_tail` | L | 逐字节 |
| 100 | `Message.cs:1715/1718` | recog=processMsg.ActorId；param=`MakeWord(fColor,bColor)`；tag=0；series=1；尾=`EncodeString(Msg)` | `sm_sysmessage` | ✅ | golden `SM_SYSMESSAGE`（recog=0x7C303998、param=0xFFDB、series=1） | L | 逐字节 |
| 643 | `Message.cs:1724` | recog=processMsg.ActorId；param=`MakeWord(n1,n2)`；tag=0；series=1；尾=`EncodeString(ChrName+'/'+Msg)` | `sm_merchantsay` | ⚠️ 编码层 ✅；**调用点 recog 错** | `sm_merchantsay_header_and_tail`；`emit_npc_says_priority_first_and_recog_by_path`（断言了错误值） | L | **G-4** |
| 99 | `Message.cs:1727` | recog=ActorId；param=`MakeWord(n1,n2)`；tag=nParam3；series=wParam；尾=`EncodeString(Msg)` | `sm_movemessage` | ✅ | `sm_movemessage_header_makeword_and_tail`；`line_notice` 单测 | L | 逐字节 |
| 659 | `Message.cs:923/927` | recog=0；param=1/0；其余 0 | `sm_groupmodechanged` | ✅ | 组队单测 | L | 逐字节 |
| 660/661 | `Operate.cs:839/815-830` | 660 全 0；661 recog∈{-1,-2,-3,-4} | `sm_creategroup_*` | ✅ | 组队单测 | L | 逐字段 |
| 662/664 | `Operate.cs:877/852-872` | 662 全 0；664 recog∈{-1,-5,-2,-3,-4} | `sm_groupaddmem_*` | ✅ | 组队单测 | L | 逐字段 |
| 663/665 | `Operate.cs:904/890-900` | 663 全 0 + 尾 `EncodeString(name)`；665 recog∈{-1,-2,-3} | `sm_groupdelmem_*` | ✅ | 组队单测 | L | 逐字段 |
| 666 | `PlayObject.cs:5150`；`Message.cs:1849` | recog=0（`SendMsg(RM_GROUPCANCEL,0,...)` 的 nParam1） | `sm_groupcancel` | ✅ | 组队单测 | L | 逐字节 |
| 667 | `Base.cs:1365` | recog=0；尾=`EncodeString("a/b/c/")` | `sm_groupmembers` | ✅ | 组队单测 | L | 逐字节 |
| 528 | `PlayObject.cs:1086` | 全 0 | `sm_outofconnection` | ✅ | `sm_outofconnection_is_header_only_with_recog_zero` | L | 逐字节 |
| 803 | `Operate.cs:33` | recog=目标 ActorId；param=x；tag=y；series=0 | `sm_ghost` | ✅ | `main.rs` 单测 | L | 逐字节 |
| 751 | `Operate.cs:110` | 全 0；尾=`EncodeBuffer(UserStateInfo)` | `sm_senduserstate` | ✅ | `main.rs` 单测 | L | 逐字节 |
| 656 | `Message.cs:1678` | recog=processMsg.ActorId；param=GetChrColor；其余 0 | `sm_changenamecolor` | ✅ | `sm_changenamecolor_header_only` | L | 逐字节 |
| 708 | `Message.cs:1950` | recog=0；param=(short)GetMyStatus()；其余 0 | `sm_mystatus` | ✅ | `pickwire` 单测 | L | 逐字节 |
| 767 | `Message.cs:1953` | recog=nParam1；其余 0；尾=`EncodeString(Msg)` | `sm_menu_ok` | ✅ | `main.rs` 单测 | L | 逐字节 |
| 600/601 | `Message.cs:804/808` | recog=nParam1（MakeIndex）；其余 0；尾=`EncodeString(Msg)` | `sm_dropitem_reply` | ✅ | `ground` 单测 | L | 逐字节 |
| 644 | `Message.cs:1807` | recog=nParam1；其余 0 | `sm_merchantdlgclose` | ✅ | `smframe_ext` 单测 | L | 逐字节 |
| 654 | `Message.cs:1840` | recog=ActorId；param=Light；tag=(short)nClientKey；series=0 | `sm_changelight`（仅换装重算接线） | ⚠️ | golden seq 62 实测 `recog=0x7C303998,param=0,tag=500`；登录点缺（G-7） | L | 逐字节 |
| 657 | `Message.cs:1846` | recog=ActorId；param=LoWord(status)；tag=HiWord(status)；series=HitSpeed | `sm_charstatuschanged` | ✅ | `smframe_ext` 单测 | L | 逐字节 |
| 634 | `Message.cs:1776` | recog=ActorId；param=CurrX；tag=CurrY；series=DayBright()；尾=`EncodeString(Msg)` | `sm_changemap` | ✅ | `sessionwire` 单测 | L | 逐字节 |
| 716 | `Message.cs:1929` | recog=ActorId；param=n1；tag=n2；series=n3 | `sm_716` | ✅ | `smframe_ext` 单测 | L | 逐字节 |
| 1102 | `Message.cs:2016` | recog=ActorId；其余 0 | `sm_breakweapon`（**无发射点**） | ❌ | **G-14**（武器升级破零 `PlayObject.cs:4118-4126` 才发） | L | 接线后逐字节 |
| 1104 | `Message.cs:2024` | recog=n1；param=LoWord(n2)；tag=HiWord(n2)；series=0；尾=`Encode(CharDesc)` | `sm_changeface`（**无发射点**） | ❌ | **G-15**（ElfWarriorMonster 变脸未建模） | L | 接线后逐字节 |
| 655 | `Message.cs:1843` | recog=n1；其余 0 | `sm_lampchangedura`（**无发射点**） | ❌ | **G-13**（火把持久变化未建模） | L | 接线后逐字节 |
| 1100 | `Message.cs:2010` | recog=ActorId；param=HP；tag=MaxHP；series=0 | `sm_openhealth`/`_frame` | ⚠️ | D-9 默认客户端布局（`ShortMessage` 尾）；pin `=0` 回 oracle | R | 真机窗口 + pin |
| 1101 | `Message.cs:2013` | recog=ActorId；其余 0 | `sm_closehealth`（**无发射点**） | ❌ | 需 `BreakOpenHealth`（`BaseObject.cs:933-942`）在 `ShowHp` 关闭时发；G-15 | R | 真机窗口 |
| 1103 | `Message.cs:2019` | recog=ActorId；param=HP；tag=MaxHP；series=0 | `sm_instancehealguage`/`_frame` | ⚠️ | D-9 默认客户端布局；pin `=0` 回 oracle | R | 真机窗口 + pin |
| 5009 | `Message.cs:2032` | 全 0 | `sm_password`（`sessionwire::password_frame`） | ✅ | `sessionwire` 单测 | L | 逐字节 |
| 8001 | `Message.cs:2039` | recog=ActorId；param=wParam；其余 0；尾=`Encode(MessageBodyWL 16B)+EncodeString(Msg)` | `sm_playdice`（**无接线**） | ❌ | **G-13**（脚本 `@playdice` 未接） | L | 接线后逐字节 |
| 802 | `Message.cs:1990` | 全 0；尾=`EncodeString(Msg)` | `sm_reconnect` | ✅ | `sm_reconnect_header_all_zero_and_tail` | L | 逐字节 |
| 805/804 | `Message.cs:1993/1999` | 805：recog=n1；param=wParam；tag=n2；series=n3。804：同 + 尾=`Encode(ShortMessage{HiWord(n2),0})` | `sm_hideevent`/`sm_showevent` | ✅（oracle 侧 MapEvent 不可达，登记） | `sm_showevent_header_and_short_message_tail` | L | 逐字节 |
| 612/614 | `Message.cs:1751/1754` | recog=0；param=n1；tag=n2；series=0 | `sm_opendoor_ok`/`sm_closedoor` | ✅（oracle 侧 CM_OPENDOOR 守卫写反 ⇒ 不可达，登记） | `smframe_ext` 单测 | L | 逐字节 |
| 610 | `Message.cs:1745` | recog=n1；param=n2；tag=n3；series=wParam；尾=`EncodeString(Msg)` | `sm_itemshow` | ✅（oracle 侧 `AddItemToMap` 恒 false ⇒ 不可达，T-32） | `sm_itemshow_header_and_tail` | L | 逐字节 |
| 53 | `Message.cs:1737` | recog=processMsg.ActorId；param=HP；tag=MP；series=MaxHP；无尾 | `sm_healthspellchanged`/`_client`/`_frame` | ⚠️ | golden 53 实测 `param=16,tag=15,series=19`（HP/MP/MaxHP）；D-9 默认客户端 `TCharDesc` 尾，pin `=0` 回 oracle | R | 真机窗口 + pin |

---

## 4. 重点核（任务点名的 6 项）

### 4.1 `wire_coord` 的 i16 位模式

- 实现：`main.rs::wire_coord` 与 `downlink.rs` 各一份：`u16::from_ne_bytes(i16::try_from(v).unwrap_or(0).to_ne_bytes())`。
- 对 `v ∈ [-32768, 32767]`：与 oracle `(ushort)v`（低 16 位/二补数）**逐位相同**。所有地图坐标（0..~1000）都落在此区间 ⇒ **生产路径无差**。
- 对 `v ∉ i16`：Rust 归 0；oracle 取低 16 位。例：`v=40000`（0x9C40）⇒ oracle 线上 `40 9c`，Rust `00 00`。**判定：口径差，但地图坐标不可达**（见 G-8）。同一坑也出现在 `record.cur_x: i16 → u16::try_from(...).unwrap_or(0)`（负值 ⇒ 0，oracle 为二补数）。

### 4.2 `recog` 用 ActorId 还是名字

- 逐帧核对：**所有帧的 `Recog` 都是 i32 ActorId/数值，没有一帧把名字塞进 `Recog`**；名字只出现在尾（`SM_USERNAME` 的 ShowName、`SM_MERCHANTSAY` 的 `ChrName+'/'`、`SM_GROUPDELMEM_OK` 的 name、`SM_GROUPMEMBERS` 的名单）。
- 关键取值源：
  - `SM_ATTACKMODE`：`Recog=(byte)AttatckMode`（**不是 Param**）——Rust 正确（`attack_mode_goes_in_recog_not_param`）。
  - `SM_VERSION_FAIL`：`Recog=ClientFile1_CRC`（**不是 ActorId**）——Rust 正确。
  - `SM_MAPDESCRIPTION`：`Recog=nMusicid`（无 MUSIC ⇒ -1 = 0xFFFFFFFF）——Rust 正确。
  - `SM_SENDNOTICE`：`Recog` **恒 2000**——Rust 正确。
  - `SM_SENDUSERSTATE`/`SM_SAVEITEMLIST`：前者 recog=0，后者 recog=**merchantId**（不是玩家 ActorId）——Rust `back_item_list(m.actor_id)` 的 `m.actor_id` 即商人实体 id，正确。
  - `SM_MERCHANTSAY`：`Recog` = `processMsg.ActorId`，两条入队路径分别是**玩家**（`SendPriorityMsg(High)`）与**NPC**（`SendMsg(normNpc, …)` 的 `baseObject.ActorId`）。**Rust 非 priority 支恒发 0 ⇒ G-4**。

### 4.3 `param/tag/series` 的高低字节打包

| 帧 | oracle 打包 | Rust | 结论 |
|---|---|---|---|
| 44 | `LoWord(nParam1)/HiWord(nParam1)` | `lo_word(delta)/hi_word(delta)` | ✅ |
| 52 | `MakeWord((byte)Job,99)`、`LoWord/HiWord(GameGold)` | `make_word(job,99)`、`lo/hi` | ✅ |
| 752 | `MakeWord(AntiMagic,0)` + 三组 `MakeWord` | `i32::from(u8 anti_magic)` + 三组 `make_word` | **❌ AntiMagic 高字节丢（G-6）** |
| 653 | `LoWord/HiWord(GameGold)` | `lo/hi` | ✅ |
| 1104 | `LoWord/HiWord(nParam2)` | `lo/hi` | ✅ |
| 657 | `LoWord/HiWord(nParam1)` + `Series=HitSpeed` | `lo/hi` + `hit_speed` | ✅ |
| 638 | `LoWord/HiWord(nParam2)` + `Series=MakeWord(EffectType,Effect)` | `x,y` + `make_word` | ✅ |
| 640/642 | `LoWord/HiWord(nParam3/nParam2)` | `lo/hi` | ✅ |
| 100/40/103/99/643 | `MakeWord(nParam1,nParam2)` | `make_word(f,b)` | ✅ |
| 10/11/13/5010/20/801/807/9/6/7 | `MakeWord(wParam,Light)` | `make_word(dir,light)`，`Series` | ✅ |
| 54 | `Recog=nMusicid`（i32，不拆） | `i32` | ✅ |
| 5008 | `LoWord/HiWord(GamePoint)` | `lo/hi` | ✅ |
| 804 | 尾 `ShortMessage.Ident=HiWord(nParam2)` | `hi_word(n_param2)` | ✅ |
| 716/805/610/712/23021/23023 | `nParam2/nParam3/wParam` 直落 16 位 | `as u16`（与 `(ushort)` 同） | ✅ |
| 615/619 | OK：`Recog=GetFeatureToLong`、`Param=GetFeatureEx` | `recog=f`、`param=feature_ex` | ✅ |
| 684 | `Recog=DealGolds`、`Param/Tag=Lo/Hi(Gold)` | 同 | ✅ |

### 4.4 GBK 文本尾的 EDCode 编解码

- oracle `EDCode.EncodeString(s)`（`EDcode.cs:175-185`）= `HUtil32.GetBytes(s)`（`gb2312`）→ `EncryptUtil.Encode`。`SendDefMessage(..., sMsg)` 与 `SendSocket(..., sMsg)` 最终都按 **GBK 字节** 处理文本。
- Rust：`edcode::encode(&downlink::gbk(text))` / `enter::gbk` 等；`edcode::encode` 与 `EncryptUtil.Encode` 逐字节同算法（3B→4 字符，`SEED=0xAC`, `BASE=0x3C`）。
- 编码输出的字符范围恒为 `0x3C..=0x7B`（ASCII）⇒ 经 C# `GetString`/`GetBytes` 的 GB2312 往返无损；Rust 直接产出字节等价。
- **注意**：`EncryptUtil.EncodeString(string)`（`EncryptUtil.cs` 里的另一个 helper）用 `str.Length`（UTF-16 码元数）而不是字节数，对中文会截断——但**出站帧不走这个 helper**（`SendDefMessage` 走 `EDCode.EncodeString`，用 `bSrc.Length`），Rust 无需复刻该 bug。
- 逐帧尾编码口径核对：`smframe*.rs` 中 `SendDefMessage(..., sMsg)` 形态全部用 `edcode::encode`；`SendSocket(defMsg, sMsg)` 形态（`SM_SPELL` 的 ASCII 数字、`SM_QUERYYB*` 的已编码文本、`SM_PLAYDICE` 的两段拼接、`SM_SAVEITEMLIST` 的已编码分页）全部**原样透传**（`tail_encoded`/直接拼接）——与 oracle 一致。
- 一个已知的**非帧层**风险：`SM_NEWMAP` 的地图名在 `main.rs` 用 `.as_bytes()`（UTF-8）而不是 GBK；现网地图文件名是 ASCII（`0`/`0101`/`0122`）⇒ 字节相同；非 ASCII 地图名会差（G-11）。

### 4.5 `SM_SYSMESSAGE` 的 `MakeWord(f_color,b_color)`

- oracle：`SendMsg(RM_SYSMESSAGE, 0, fColor, bColor, 0, sMsg)` → `PlayObject.Message.cs:1715`：`MakeMessage(SM_SYSMESSAGE, processMsg.ActorId, MakeWord((ushort)nParam1, (ushort)nParam2), 0, 1)`。
- `nParam1=fColor`、`nParam2=bColor`，`MakeWord(ushort,ushort)` ⇒ `param = fColor | (bColor << 8)`（低字节=前景、高字节=背景）。
- Rust `sm_sysmessage(recog, f_color, b_color, msg)` → `make_word(f_color, b_color)` = `f | b<<8`，`tag=0`、`series=1`。**✅ 一致**（golden 实测 `param=0xFFDB`：低 0xDB=GreenMsgFColor、高 0xFF=GreenMsgBColor）。
- 边界：oracle 用 `ushort` 全 16 位；Rust 入参是 `u8`。当颜色值 >255（只可能来自脚本公告标签 `[f,b]` 的 `StrToUInt16`）时，oracle 会把 `fColor` 的高位 OR 进高字节，而 Rust `line_notice` 的 `as u8` 只留低字节 ⇒ G-9。配置项颜色本身是 `byte`（`GameSvrConf.cs:555`），不受影响。

### 4.6 `SM_HEALTHSPELLCHANGED` 的 HP/MP/MaxHP 三字段

- oracle `Message.cs:1737`：`MakeMessage(SM_HEALTHSPELLCHANGED, processMsg.ActorId, baseObject.WAbil.HP, WAbil.MP, WAbil.MaxHP)` ⇒ **`Param=HP`、`Tag=MP`、`Series=MaxHP`**（顺序不是 HP/MaxHP/MP）。
- 金标准实测（`53`）：`param=16`、`tag=15`、`series=19` —— 与同一时刻 `SM_ABILITY` 尾里的 `HP=16/MP=15/MaxHP=19` 对应。Rust `sm_healthspellchanged(recog, hp, mp, max_hp)` → `command_header(recog, 53, hp, mp, max_hp)`。**✅ oracle 布局一致**。
- 生产默认是 D-9 `HIGHHP` 客户端布局（`sm_healthspellchanged_client`）：头仍是同样的 `param=HP/tag=MP/series=MaxHP`，追加 `Encode(TCharDesc{HP:i32,MP:i32,MaxHP:i32} 12B)`；冻结客户端从尾读三字段，头字段被忽略。§6.0-3 已登记的故意偏离；对拍必须 pin `MIR2_CLIENT_ABILITY_LAYOUT=0`。
- 调用点核对：`pickwire::notice_frame`（`RM_HEALTHSPELLCHANGED`）传 `session.wabil.hp/mp/max_hp`；`main.rs` 的受击/进图/升级/用药路径同源。**字段顺序无误**。

---

## 5. 缺口清单（按「客户端可见 × 影响」排序）

> 「期望字节 vs 实际字节」的固定输入均以 12B 头解码态（小端）表示；`encode_len(n)=n+ceil(n/3)`。

### G-1（高）`SM_ADDITEM(200)` 只有 12B 头，缺 `ClientItem` 尾
- **位置**：`smframe::sm_additem`；发射点 `main.rs`（卸下/换装 `out.swapped_out`）、`shop.rs`（购买/打造/取回）、`ground.rs`→`pickwire`（拾取）；**`trade.rs`/`selloff.rs` 已按 oracle 构造完整 ClientItem 尾**，不属缺口。
- **oracle**：`PlayObject.Base.cs:225-227` —— `MakeMessage(200, ActorId,0,0,1)` 后 `SendSocket(ClientMsg, EDCode.EncodeBuffer(clientItem))`。
- **客户端可见性**：**高**。脱装备/买药/拾取都会触发；客户端按 `DecodeBuffer(ClientItem)` 读包内物品，header-only 会得到空/垃圾项。
- **注**：T-24 登记说「拾取路径未上真机、线上不可达」——但**卸下（`Operate.cs:452`）与商店购买**是可达的，该条的实际影响被低估。`ClientItem` 编码器**已存在**（`itemwire::encode_client_item`，固定 124B，单测钉住），缺的只是接线。
- **最小修复面**：给 `sm_additem` 增加 `tail: &[u8]`（照 `sm_delitem`），复用 `itemwire::encode_client_item`；`main.rs` 用 `out.swapped_out` 构造尾，`shop.rs`/`ground.rs` 把已构造的 item 传入。
- **可复现验证**：`actor_id=0x01020304`、物品名 `abc`、`ClientItem` 124B。期望 body = `04030201 c800 0000 0000 0100` + `encode(124B)`（`encode_len(124)=166`）⇒ **178B**；实际 **12B**。断言 `body[12..] == edcode::encode(client_item)`。

### G-2（高）`SM_UPDATEITEM(203)` 完全未发射
- **oracle**：`PlayObject.Base.cs:1683/1704/1725` —— `MakeMessage(203, ActorId,0,0,1)` + `EncodeBuffer(ClientItem)`；发点 `Operate.cs:462-478`（穿上衣服后按 Shape 选 `SendUpdateItem/WithLevel/ByJob`）、`PlayObject.cs:4131`（武器升级成功）。
- **Rust**：`smframe.rs` 无 `sm_updateitem`；`bag.rs` **无发射**（只有 615/616/619/620 的帧规格与未使用的 `SM_UPDATEITEM` 常量）。
- **客户端可见性**：**高**（穿衣后装备栏不显示实际外观/等级差量）。
- **最小修复面**：新增 `sm_updateitem(actor_id, tail)`（形同 `sm_delitem`），在 `bag.rs` 成功支产出、`main.rs` 排在 `SM_TAKEON_OK` 之后。
- **可复现验证**：期望 `0xCB00` ident 的头 `recog=ActorId,param=0,tag=0,series=1` + `encode(124B)`；实际无帧。

### G-3（高）登录帧 `SM_SENDUSEITEMS(621)` / `SM_SENDMYMAGIC(211)` 未实现
- **oracle**：`PlayObject.cs:1389-1390`（`UserLogon` 的 `SendPriorityMsg`）→ `Base.cs:1019-1050` / `:1052-1073`。
  - 621：头全 0；尾 = 逐槽 `i + '/' + EncodeBuffer(ClientItem) + '/'`（`i` 是槽位十进制 ASCII）。
  - 211：头 `recog=0, param=0, tag=0, series=MagicList.Count`；尾 = 逐条 `EncodeBuffer(ClientMagic) + '/'`。
- **Rust**：`EnterWorldInputs::frames()` 的 16 条序里没有 621/211；全仓无发射点。
- **客户端可见性**：**高**（登录后装备面板/技能列表为空——有装备/有技能的角色必现）。
- **最小修复面**：新增两个编码器 + 在 `enter.rs` 的 `WorldFrameInputs` 里从 `use_items`/`magic` 装配；`MagicList` 已在存档记录里。
- **可复现验证**：固定 2 件装备 + 1 个技能 ⇒ 期望两条帧：621 尾 = `"0/"+encode(item0)+"/"+"5/"+encode(item5)+"/"`；211 `series=1` + `encode(magic)+"/"`。实际 0 条。

### G-4（中高）`SM_MERCHANTSAY(643)` 非 priority 支的 `Recog` 恒 0
- **oracle**：`ScriptEngine.cs:993/997` → `BaseObject.Message.cs:154` 的 `ActorId = baseObject.ActorId` ⇒ **NPC 的 ActorId**。`NormNpc : AnimalObject : BaseObject : ActorEntity`，`ActorEntity()` 构造即 `GetNextIdentity()`（正整数，`ActorManager.cs:31` / `HUtil32.cs:20-33`）⇒ **实际恒非 0**（`HUtil32.Sequence()` 只重掷负值，理论上 2^-32 可为 0）。
- **Rust**：`main.rs::emit_npc_says` 的 `let recog = if s.priority { player_actor_id } else { 0 };`；`npc::SayMsg` 结构体没有 NPC ActorId 字段。单测 `emit_npc_says_priority_first_and_recog_by_path` 把「非 priority ⇒ 0」写成了断言，**固化了错误假设**。
- **客户端可见性**：中高。客户端 `ClientGetMerchantSay(msg.Recog, msg.Param, …)`（BotSrv `RobotPlayer.Operate.cs:1051-1052`）把 `Recog` 当说话 NPC 的 id（经典客户端用它做 `g_nCurMerchant` 关联）；0 会让 NPC 对话框/说话归属错乱。
- **最小修复面**：`SayMsg` 增加 `npc_actor_id: i32`；`npc.rs::send_say` 从 `self.def` 的实体 id 填；`emit_npc_says` 非 priority 支用它；改单测断言。
- **可复现验证**：`npc_actor_id=0x11223344`, `priority=false`, 文本 `QManage/b`。期望头 `44332211 8302 0000 0000 0100`；实际 `00000000 8302 0000 0000 0100`。

### G-5（中）`SM_HEAVYHIT(15)` 缺尾（挖矿命中）
- **oracle**：`PlayObject.Message.cs:1440-1443` `SendSocket(ClientMsg, processMsg.Msg)`；发点 `PlayObject.Base.cs:1586`，`s1C` 仅在 `StoneMineEvent` 命中且 `Random(MakeMineHitRate)==0` 时为 `"1"`。
- **Rust**：`sm_action_hit` 恒 `gm_data_body(header, &[])`，无法表达尾。
- **客户端可见性**：中（挖矿命中反馈）。普通战斗的 `s1C` 恒空 ⇒ 现有攻击路径无差。
- **最小修复面**：给 `sm_action_hit` 增加 `tail: &[u8]`（仅 `SM_HEAVYHIT` 用；其余传空），挖矿事件传入 `b"1"`。
- **可复现验证**：`actor=1,x=2,y=3,dir=4,Msg="1"`。期望 `01000000 0f00 0200 0300 0400` + `31`（13B）；实际 12B。

### G-6（中）`SM_SUBABILITY(752)` 的 `AntiMagic>255` 丢高字节
- **oracle**：`Message.cs:1675/1935` `MakeLong(MakeWord(AntiMagic, 0), 0)`。`AntiMagic` 是 `ushort`（`BaseObject.cs:113`）⇒ 选中 `MakeWord(ushort,ushort)` 重载 ⇒ `Recog = AntiMagic`（**完整 16 位**；`Recog` 是 i32，不截断）。
- **客户端**：`g_nMyAntiMagic = LoByte(LoWord(msg.Recog))`、`g_nMyIPowerRecover = HiByte(LoWord(msg.Recog))`（BotSrv `RobotPlayer.Operate.cs:626-627`）⇒ oracle 在 `AntiMagic=300` 时给出 `AntiMagic=44, IPowerRecover=1`。
- **Rust**：`ability::SubAbility::to_frame_bytes` 用 `lo_byte(anti_magic)`，`sm_subability` 入参 `u8` ⇒ `Recog=44`，`IPowerRecover=0`。**注释「MakeWord(x,0) 只取低字节」与 C# 重载解析不符。**
- **可达性**：`AntiMagic = 1 + AddAbil.AntiMagic`，`AddAbil.AntiMagic += MgAvoid(byte)`（多件）`+ HiByte(AC)`（`PlayObject.cs:4434-4540`）⇒ 可超 255。
- **最小修复面**：`to_frame_bytes` 返回 `anti_magic: u16`（删 `lo_byte`）；`sm_subability` 的 `anti_magic` 改 `u16`，`recog=i32::from(anti_magic)`。其余 6 项保持 `lo_byte`（`MakeWord(byte/ushort 混用)` 对它们确实是低字节语义）。
- **可复现验证**：`anti_magic=300, hit=17, speed=15, 其余 0`。期望 body `2c010000 f002 0f11 0000 0000`；实际 `2c000000 f002 0f11 0000 0000`。

### G-7（低）登录时 `SM_CHANGELIGHT(654)` 未发
- **oracle**：`Message.cs:1680-1689` 的 `RM_LOGON` 支里 `SendMsg(RM_CHANGELIGHT, 0,0,0,0)`（队列后发）⇒ `Message.cs:1840`。金标准 seq 62 实测：`recog=0x7C303998, param=0, tag=500, series=0`（18B 线缆）。
- **Rust**：登录序不含 654；只有换装重算走 `sessionwire::changelight_frame`。
- **客户端可见性**：**低**——客户端从 `SM_LOGON` 的 `HiByte(series)` 已设置 `m_nChrLight`（BotSrv `PlayScene.cs:1086`），自视角不缺；但其他观察者视角（AOI 出现）需要 654。
- **最小修复面**：在进世界完成时补发一条 `sm_changelight(actor_id, light, 500)`，或在 AOI 出现帧里带上。
- **可复现验证**：期望头 `9839307c 8e02 0000 f401 0000`；实际无帧。

### G-8（低/不可达）`wire_coord` 越界归 0
- 见 §4.1；修复：`u16::from_ne_bytes((v as i16).to_ne_bytes())`（即 `v as u16`）以完全等同 `(ushort)` 截断。

### G-9（低）颜色 >255 时 `MakeWord` 低位重叠
- `line_notice` 的 `str_to_uint16(...) as u8` 与 `pickwire` 的 `u8::try_from(...).unwrap_or(0)` 都不等价于 oracle 的 `MakeWord(ushort,ushort)`（低 16 位）。
- 可达性：仅脚本公告标签 `[f,b]`/`{f,b,t}` 写 >255 时；配置颜色是 `byte`。
- 修复：把 4 个颜色入参改为 `u16` 并按 oracle 的 `(ushort)(f | b<<8)` 打包。

### G-10（低）`SM_STRUCK` 伤害饱和
- `main.rs` 有一处 `u16::try_from(o.damage).unwrap_or(u16::MAX)`；oracle `(ushort)processMsg.wParam` 是**回绕**。伤害 >65535 不可达（Mir2 数值域），记口径差。

### G-11（低）地图文件名用 UTF-8 字节
- `main.rs` 进图/切图处 `name.as_bytes()`（UTF-8）而非 `downlink::gbk`。现网地图文件名 ASCII ⇒ 无差；非 ASCII 文件名会差。修复：统一 `gbk(&name)`。

### G-12（文档）`enter.rs` 注释把 `SM_ABILITY` 的 Job/99 高低字节写反
- 代码 `make_word(job, 99)` = `Job 低、99 高`（与 oracle 一致）；注释「Job 在高字节、99 在低字节」是反的。不影响字节，只影响后来者判断。

### G-13（低）脚本/物品面未接线的帧：23021/23023/8001/655
- `SM_QUERYYBSELL/DEAL`（寄售列表，`ExecutionProcessingSys.cs:2443/2550`）、`SM_PLAYDICE`（`@playdice`）、`SM_LAMPCHANGEDURA`（火把持久）编码器已备，但无发射点。修复面 = 对应脚本标签/物品处理器接线。

### G-14（低）`SM_BREAKWEAPON(1102)` 未接线
- oracle 可达点 = `CheckWeaponUpgrade` 武器升级破零（`PlayObject.cs:4118-4126`）；Rust 未建模武器升级流程。接线后在破零支发 `sm_breakweapon(actor_id)`（header-only）。

### G-15（低）`SM_CHANGEFACE(1104)` / `SM_CLOSEHEALTH(1101)` 未接线
- `RM_CHANGEFACE` 来自 `ElfWarriorMonster.cs:149`（变脸怪）；`RM_CLOSEHEALTH` 来自 `BaseObject.cs:933-942` 的 `BreakOpenHealth`（`ShowHp` 关闭）。两者都需要世界侧状态；编码器已备。

---

## 6. 结论

1. **编码层质量高**：16 条入图帧与 M0 金标准**逐字节相等**（含长度），`smframe` 的 36 个单测全绿；`MakeMessage` 的 12B 字段序、`MakeWord` 的两个重载、`LoWord/HiWord`、`EDCode` 双编码/多段拼接、网关 `'#'+Encode(头)+尾+'!'` 都逐位对上 oracle。
2. **真实缺陷集中在「尾/接线」而不是「头字段」**：
   - 头字段唯一确证的错值是 **G-6（`SM_SUBABILITY` 的 AntiMagic 高字节）**；
   - 高影响缺失全部是**尾或整帧未发**：G-1（`SM_ADDITEM` 尾）、G-2（`SM_UPDATEITEM` 整帧）、G-3（登录 621/211 整帧）；
   - 其余为调用点取值错（G-4）、不可达/边界口径（G-5、G-8…G-11）与未接线（G-13…G-15）。
3. **建议修复批次**（按性价比）：
   - 批 A（R 级、客户端必现）：G-1 + G-2（复用已存在的 `itemwire::encode_client_item`，改动面只在 `smframe.rs`/`main.rs`/`shop.rs`/`bag.rs`）。
   - 批 B（R 级）：G-3（621/211 编码器 + 登录装配）。
   - 批 C（L 级）：G-4 + G-6（单点字段修正 + 单测改断言）。
   - 批 D（L 级、按可达性排期）：G-5、G-7、G-8…G-15。
4. **对拍约定（必须写入各批的 review）**：任何按 oracle 字段位解 52/53/31/1100/1103 的夹具，必须显式 `MIR2_CLIENT_ABILITY_LAYOUT=0`；28 帧同理 `MIR2_CLIENT_MOVEFAIL_LAYOUT` 保持默认关。否则 D-9 客户端布局的**有意偏离**会被误判为「Rust 缺陷」或反过来把缺陷洗绿。

### 残留物清单
- 本报告文件：`mir2-rs/tests/parity/m3/audit-smframe-fields-20261018.md`（唯一新增文件）。
- worktree：`/e/tmp/impl-frameaudit`（detached `03208e20e`），未 push。
- 未启动任何服务/端口；未改共享主检出；本地 `cargo test` 只在 worktree 的 `target/` 下产生构建产物。
