# T-127 落帧残余：`CastleEvent` 的 Broadcast / ServerGroupMsg / RefreshShowName / Ref* 接线（tier R）

- 卡：`openmir2-m3-castle-siege-state`（本片 = 既有 R 卡的 T-127 残余，owner `openmir2-dev-2`，
  branch `card/castle-siege-t127`）。
- 基线：`master = 18ee8003b`（2026-10-25）。
- 写集：`mir2-rs/bins/game-svr/src/castle.rs`（+3 函数）、`mir2-rs/bins/game-svr/src/castle/tests.rs`
  （+4 合同用例）、`mir2-rs/bins/game-svr/src/lib.rs`（城堡 `Run` 事件分派）、本报告。
- **未改**：`crates/**`、`src/**`（C# oracle）、`sql/`、`MirClient`、协议号表、场景 JSON。
- **未动**（卡面明令）：tick 驱动 `run_castle_tick` / `CastleMgr.Run` 的 10s 门、`SaveAttackSabukWall`、
  `crates/world/src/castle.rs` 的状态机本体（第①片已合并 `092a1f24b`）。
- tier **R**：本片改变**可观测帧**（全服 `SM_SYSMESSAGE` / `SM_USERNAME` / `SM_DIGDOWN`），
  合并后保持 in-progress，真机窗口复拍后 closed。

## 1. 接线前状态（残余点）

`lib.rs` 城堡 `Run` 循环的事件分派只有三支落地（`Structure(SetMapXyFlag)` / `SaveAttackSabukWall` /
兜底 `other => glog!("DBG", "城堡状态机事件（未接线）：{other:?}")`）；`Broadcast` / `ServerGroupMsg` /
`RefreshShowName` / `Structure(Ref*)` 全部落进兜底日志 ⇒ **零帧**。注释自称「单服部署 `UnderWar=false`
时这些分支不可达」——本片核实该前提**只在平时成立**：部署 `Server.conf:380 StartCastlewarTime=20`、
`Castle/0/AttackSabukWall.txt` 存在 ⇒ 每日 20 点若有当日申请即进 `UnderWar`，四类事件变为可达。

## 2. oracle 锚定（逐条）

| 事件 | oracle 位置 | 语义 |
|---|---|---|
| 开战链 | `UserCastle.cs:340-376` | 倒序遍历 `AttackWarList`（`:347`）→ `StartWallconquestWar()`（`:366`）→ `SaveAttackSabukWall()`（`:367`）→ `SS_212`（`:368`）→ `SendBroadCastMsgExt("[{0} 攻城战已经开始]")`（`:369-370`）+ `SS_204`（`:371`）→ `MainDoorControl(true)`（`:373`） |
| 结束预告 | `UserCastle.cs:408-418` | 严格 `>` 门：`GetTickCount()-StartCastleWarTick > CastleWarTime-ShowCastleWarEndMsgTime` ⇒ `"[{0} 攻城战离结束还有{1}分钟]"`（分钟 = `ShowCastleWarEndMsgTime/60000`）；本 tick 广播 + `SS_204` |
| 超时停战 | `UserCastle.cs:419-422` → `:577-600` | 严格 `>` 门 ⇒ `UnderWar=false` + 清 `AttackGuildList` + `"[{0} 攻城战已经结束]"` + `SS_204`；`GetMapOfRangeHumanCount` 返回值被丢弃、`ListC` 恒空 ⇒ `ChangePkStatus`/`MapRandomMove` 从不执行（既有照搬） |
| `Broadcast` | `WorldServer.cs:1726-1737` | `for (PlayObjectList) if (!Ghost) SysMsg(s, MsgColor.Red, MsgType.System)` |
| `SysMsg` 红字支 | `BaseObject.cs:1588-1680` | 非 `Notice` ⇒ `default` 分支：`SendMsg(RM_SYSMESSAGE, 0, RedMsgFColor, RedMsgBColor, 0, sMsg)`（`:1655-1679`） |
| 帧 | `PlayObject.Message.cs:1715-1718` | `SM_SYSMESSAGE(100)`：`Recog` = 收件人自己、`Param = MakeWord(F,B)`、`Tag = 0`、`Series = 1`、尾 `EncodeString` |
| `ServerGroupMsg` | `WorldServer.cs:885-893` | `ServerIndex==0 ⇒ PlanesServer.SendServerSocket(code/server/msg)`，否则 `PlanesClient.SendSocket(...)`（`PlanesServer.cs:120-134` / `PlanesClient.cs:139-146`） |
| `RefreshShowName` | `UserCastle.cs:564-575`、`WorldServer.cs:1698-1725`、`BaseObject.Base.cs:1174-1177` | `GetMapRageHuman(PalaceEnvir, HomeX, HomeY, 100)`（`!Death && !Ghost && Envir==envir && abs(dx)<=100 && abs(dy)<=100`）逐目标 `RefShowName()` ⇒ `SendRefMsg(RM_USERNAME, 0,0,0,0, GetShowName())` |
| `SM_USERNAME` | `PlayObject.Message.cs:1662-1665` | `MakeMessage(SM_USERNAME, ActorId, GetChrColor(baseObject), 0, 0)` + 尾 `EncodeString(msg)` |
| `GetChrColor`（观察者侧） | `PlayObject.cs:3585-3655` | 非玩家/`PvpLevel>=2` 之外的 PVP/行会关系/`Fight3Zone`；攻城区（`:3613-3652`）先写 `InFreePKAreaNameColor`，观察者有行会时走 `castle.IsAttackGuild(...)` ⇒ **N1 抛异常** |
| `Ref*`（关门） | `UserCastle.cs:890-909` → `CastleDoor.cs:60-76` | `MainDoorControl(true)` ⇒ `CastleDoor.Close()`：`Dir=3-Round(HP/MaxHP*3)`（`>=3 ⇒ 0`）→ `SendRefMsg(RM_DIGDOWN, Dir, X, Y, 0, "")` → `IsOpened=false` → `StoneMode=false` → `SetMapXyFlag(1)` → `HoldPlace=true` |
| `SM_DIGDOWN` | `PlayObject.Message.cs:1900-1903` | `MakeMessage(SM_DIGDOWN, ActorId, nParam1=X, nParam2=Y, 0)`，无尾 |

## 3. 实现（三个宿主排水点）

| 函数 | 位置 | 口径 |
|---|---|---|
| `fan_out_sysmessage_to_all` | `castle.rs:1088` | `SendBroadCastMsgExt`：**每个** `!Ghost` 在线玩家一条 `SM_SYSMESSAGE`；`Recog` = 收件人自己、`Param = MakeWord(RedF,RedB)`、`Series=1`、尾 = GBK 原文的 EDCode。**不排除 caller、不拼 `SysMsgPreFix`**（与脚本 `SENDMSG 0-3` 的 `fan_out_world_sysmessage` 的两点差别）。逐收件人 `NetPriorityQueue`，入队序 = 城堡列表序 × 事件序 |
| `refresh_show_name_in_area` | `castle.rs:1164` | `GetMapRageHuman` 目标集（`<=` 边界、`PalaceEnvir==null ⇒ 零目标`）→ 逐目标 `RefShowName`：收件人 = `chat::hear_targets`（±`SendRefMsgRange`、同图、`Race==Play && !Ghost`、含目标自己）；**逐收件人**重算 `GetChrColor`（`query_user_name_color`，行会名来自会话态、`InGuildWarArea` 来自 `castle_war_states`）；`Err`（N1）⇒ 丢该帧 |
| `queue_structure_ref_msg` | `castle.rs:1279` | `Structure(Ref*)` 重新入队成 `MonsterRefMsg`（收件人在发帧时刻快照），与门/墙 `Run` 共用 `drain_monster_ref_msgs` 的逐 ident 口径（`SM_DIGUP`/`SM_DIGDOWN`/`SM_TURN`+`SM_FEATURECHANGED`/`SM_ALIVE`）。`actor_id` 由事件坐标反查 `MainDoor/LeftWall/CenterWall/RightWall` 的 `entity_id` |

`lib.rs:23415-23520` 的分派：`Broadcast` → 跨城收集（`Vec<String>`）；`ServerGroupMsg` → **日志**
（单服无 planes 对端，客户端不可见；与 `guild_official.rs:690` 的 `SS_205` 同口径）；
`RefreshShowName` → 收集 `(idx,x,y,range)`；`Structure(Ref*)` → `queue_structure_ref_msg`；
`SpawnDefenseUnit(_)` → 零动作（只有 `Initialize` 会产）。城堡循环结束后统一扇出广播、再跑 `RefShowName`
（`RM_SYSMESSAGE` 优先级 252 > `RM_USERNAME` 165，分段排水与「同一条 `MsgQueue`」在线缆序上等价）。

## 4. 合同测试（4 条，`castle/tests.rs`）

| 用例 | 断言 |
|---|---|
| `t127_broadcast_fans_out_to_every_nonghost_player_in_heap_order` | 3 条广播：Ghost 玩家 0 帧；`!Ghost` 玩家收 3 条，**四叉堆出队序 `[0,2,1]`**；逐帧 `Recog` = 收件人、`Param=0x01FC`、`Tag=0`、`Series=1`、尾 GBK 原文 |
| `t127_ref_show_name_targets_only_palace_area_players` | 皇宫图内两玩家各收 2 条（目标序 甲→乙），宫外玩家 0 条；`Recog` = 被展示者、`Param` = `name_color`（255）、尾 = `GetShowName()` |
| `t127_ref_show_name_n1_drops_frame_for_guilded_viewer_in_war_area` | 有会 + 双方 `InGuildWarArea` + `UnderWar` ⇒ **0 帧**（N1 丢弃）；同一夹具去掉行会 ⇒ 4 帧且 `Param = InFreePKAreaNameColor`（`world.castle_name_colors.in_free_pk`） |
| `t127_door_close_structure_event_drains_to_sm_digdown` | 坐标对不上门/墙 ⇒ 不入队；命中 ⇒ `MonsterRefMsg{ident=RM_DIGDOWN, actor_id=门, recipients=[±12 内玩家]}`，经 `drain_monster_ref_msgs` 出**一条** `SM_DIGDOWN(21)`（`Recog`/`Param=X`/`Tag=Y`/`Series=0`/无尾，12B） |

## 5. 牙（4 条，各还原后逐字节比对源文件 sha256）

| 牙 | 变异 | 结果 |
|---|---|---|
| A | `fan_out_sysmessage_to_all` 的逐条优先级改 `priority - i`（=丢掉「同 ident 同优先级」） | **RED**：出队序变为 `[0,1,2]`，`第 0 帧 尾` 断言失败 |
| B | `refresh_show_name_in_area` 去掉 `target.map_id != palace_map_id` 门 | **RED**：`sent=5 != 4`（宫外玩家也被瞄准） |
| C | `queue_structure_ref_msg` 的 `RM_DIGDOWN`/`RM_DIGUP` 对调 | **RED**：`SM_DIGDOWN` ident 断言失败 |
| D | `refresh_show_name_in_area` 的 `viewer_in_war` 硬改 `false` | **RED**：N1 段 `dropped=4 != 0` |

还原后 `castle.rs` sha256 = `9ae04a7b246b62f301ceb8bdc6346cce6cacae6ea6f2a11285a8ea38e375d59e`（与备份同），
`grep TOOTH` = 0，`cargo test -p game-svr --lib t127_` 4/4 GREEN。

## 6. 门禁

| 检查 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | rc=0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 |
| `cargo test --workspace` | rc=0：**141 suites / 3204 passed / 0 failed / 62 ignored**（含本卡 4 条 `t127_*`） |
| `cargo run -p msgcodegen -- --check` | rc=0（604 条） |
| `conflict_marker` / `stale_pointer` / `dynamic_count_stamp` / `board_coverage` | rc=0 |
| `tools/whitelist/t_reservation_gate.py check` + `selftest` | rc=0（255 行）+ SELFTEST GREEN 18/18 |
| `git diff --check` | rc=0 |
| 边界 | 仅 `bins/game-svr/src/{castle.rs,castle/tests.rs,lib.rs}` + `tests/parity/whitelist.md`（T-127 条目）；`src/ sql/ MirClient crates/` 零改动 |

执行口径：全部 Rust 门禁用隔离 `CARGO_TARGET_DIR=E:/tmp/t127-target` + `--offline`；全量测试原始日志 `E:/tmp/t127-ws-test.log`。

## 7. 登记的偏差 / 未建模（**均为 oracle 侧不可判或本仓既有约定**）

1. **跨源队列合并**：同一玩家的 `MsgQueue` 里若同时有城堡广播与其它系统帧（脚本 `SENDMSG`、行会通知
   等），本仓仍按「每个排水点只排本次产出」分段排水（既有各排水点同口径）。本片内 `RM_USERNAME(165)`
   先于 `RM_SYSMESSAGE(252)` 与 oracle 的单队列优先级序一致；**跨排水点**的相对序未建模。
2. **N1 的「消息段中止」**：oracle 抛异常被吞后，该玩家**本 tick 剩余消息**一并中止；本仓只丢这一帧
   （与 `handle_query_user_name_cmd` 的既有处理同口径）。
3. **`ServerGroupMsg`**：oracle 走 planes 通道（`PlanesServer`/`PlanesClient`），客户端看不到；本仓单服
   无 planes 对端 ⇒ 只记 `INF` 日志（`guild_official.rs` 的 `SS_205` 先例）。**未建模 planes 侧连接**。
4. **`RefreshShowName` 的逐观察者色**比 `guild_cmd::ref_show_name` 的单色近似更严（那条路径仍取展示表
   缓存色）；两条路径尚未统一，登记（不在本卡写集）。
5. **`WarDate` 落盘时分秒**（第①片 M-2）未涉本片。
6. **`CastleWarEvent::ShowName`（`InGuildWarArea` 变化时逐观察者重发名帧）** 属 T-128，本卡未动
   （`lib.rs` 该分支仍 `{}`）。

## 8. R 级窗口项（`window_required: true`）

1. **开战链帧序**：真机把 `Castle/0/AttackSabukWall.txt` 置为当日申请（或改 `StartCastlewarTime` 到
   当前小时），两侧 20 点整点复拍：全服每条 `SM_SYSMESSAGE` 的文本/颜色/收件人与 oracle **diff=0**；
   `SM_DIGDOWN`（关主门）与 `SM_USERNAME`（皇宫 100 格）帧及其相对序 diff=0。
2. **结束预告 / 超时停战**：`CastleWarTime` / `ShowCastleWarEndMsgTime` 用可注入时钟（`CastleNow`）
   压到分钟级复拍，`[离结束还有10分钟]`/`[已经结束]` 两帧逐字节同。
3. **N1 丢弃**：真机「有会观察者在攻城区」场景下该观察者收不到 `SM_USERNAME`，且 oracle 侧日志/后续帧
   表现一致（本仓只丢该帧，见 §7.2）。
4. **多城**：部署 2 座城（`Castle/List.txt` = 0/1），两城同时开战时每玩家的两条广播次序按四叉堆
   `[0,1]`（= 城堡列表序）；真机逐字节复核。
5. `SS_204`/`SS_212` 属 planes 通道，客户端不可观测；真机以 oracle 侧「无 PlanesServer 连接时零副作用」
   对照（本仓只记日志）。

## 9. 残留

- 本片零起栈、零端口；未改 C#/SQL/协议号表。
- 临时物：`E:/tmp/t127-target/`（隔离 CARGO_TARGET_DIR）、`E:/tmp/t127-castle.bak`（牙还原备份）、
  `E:/tmp/t127-block.rs` / `E:/tmp/t127-tests.rs` / `E:/tmp/t127-edit.py`（生成用中间件）、
  `E:/tmp/t127-tooth-a.txt`（牙读数）、`E:/tmp/t127-thread.json`（卡片线程快照）。均可删。
- 未 merge、未自签 review。
