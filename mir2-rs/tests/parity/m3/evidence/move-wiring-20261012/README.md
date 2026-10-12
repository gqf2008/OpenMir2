# 卡 `openmir2-m3-move-wiring` 证据（Rust game-svr 移动接线：`CM_TURN/WALK/RUN/HORSERUN`）

**issue**：`ff0ea565a97997654361c04e4108ee4fbf3e2a57`
**patch**：`patch --base refs/heads/master --head refs/heads/worktree-m2-gamesvr-host`
**base**：`refs/heads/master`（`d81cfe7c`，本分支已 fast-forward 到它）
**worktree**：`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\m2-svr`（`git status` 干净）
**写集**：`mir2-rs/bins/game-svr/**` + `mir2-rs/crates/world/src/world.rs`（仅新增一个同步移动入口）+ `mir2-rs/tests/parity/m3/**`
**返工**：reviewer-5 review `da54b167` 判 `request_changes`（F-1 Important + F-2/F-3 Minor）；本目录即返工后的证据（head = 返工 tip）。逐条处置见 §2b。

## 0. 返工摘要（reviewer-5 F-1/F-2/F-3，全修、无"登记了事"）

| finding | 结论 | 落点 |
| --- | --- | --- |
| **F-1**（Important）`CM_TURN` 的 `SM_TURN` 多出 `EncodeString(ShowName/Color)` 名尾 | **修**：`CM_TURN` 支改用新的 `downlink::turn_pair_nameless`（尾只有 `Encode(8B CharDesc)`）；`turn_pair`（带名尾）保留给出现帧路径 | `downlink.rs::turn_pair_nameless` + `main.rs::handle_move_cmd` |
| **F-1 判据缺口**（原只断言头 12B） | **补**：进程级用例改为**逐字节**断言尾 = `Encode(8B CharDesc)` 且长度 23；新增 `downlink` 单测 `turn_pair_nameless_has_no_show_name_tail` | `main.rs` 用例 + `downlink.rs` 用例 |
| **F-2**（Minor）`+GD` 未按 oracle result 门 `SpaceMoved \|\| (CurrX==nX && CurrY==nY)` | **修**：`movewire::move_result_ok` 门；移动帧广播不变（oracle `WalkTo` 内已发），**仅** `+GD` 受门控 | `movewire.rs` + `main.rs` |
| **F-3**（Minor）`Death`/`STONE` 守卫缺失 + README §4.4 文案与实现不符 | **修 `Death`**（`Entity.death` ⇒ 四支拒绝）；`STONE` 属未移植的状态位面 ⇒ **登记为未接**（不是"已按 death 判过"） | `main.rs` + 本文 §4 |

`SpaceMoved`（地图路由换图 `EnterAnotherMap`/`ChangeSpaceMove`）本仓库未移植 ⇒ F-2 的 result 门退化为 `at_target`，**登记**（见 §4）。

## 1. 交付内容（DoD ①–③）

### ① 派发（oracle 行号）

| 命令 | oracle `case` | 处理器实现 | 本片落点 |
| --- | --- | --- | --- |
| `CM_TURN`(3010) | `PlayObject.Message.cs:1008-1017` | `ClientChangeDir`（`PlayObject.Operate.cs:310-348`） | `main.rs::handle_move_cmd` |
| `CM_WALK`(3011) | `PlayObject.Message.cs:1059-1068` | `ClientWalkXY`（`PlayObject.Attack.cs:1365-1435`） | 同上 → `ClientWalkTo` 的 1 格 |
| `CM_RUN`(3013) | `PlayObject.Message.cs:1166-1175` | `ClientRunXY`（`PlayObject.Attack.cs:1288-1345`） | 同上 → 2 格 |
| `CM_HORSERUN`(3035) | `PlayObject.Message.cs:1118-1127` | `ClientHorseRunXY`（`PlayObject.Attack.cs:970-1040`） | 同上 → 3 格 |

四者共用 **同一条入帧映射**（`WorldServer.ProcessUserMessage`，`src/GameSrv/Word/WorldServer.cs:830-851`）：

```text
SendMsg(Ident, Tag, LoWord(Recog), HiWord(Recog), 0)
  ⇒ 目标坐标 nX = (short)LoWord(Recog)、nY = (short)HiWord(Recog)、方向 nDir = (byte)Tag
```

口径已由 whitelist **T-6** 实测钉住（`Recog = MakeLong(X,Y)`、`Tag = 方向`、`Param = 0`、`Series = 0`；
C3 金标准锚 `0x026B011F` = `X=287,Y=619`、`Tag=4`）。本片用 `feature::{lo_word,hi_word}` 解码，
方向用 `M2Share.GetNextDirection`（`src/M2Server/M2Share.cs:299-378`）逐行重写（含 `|sy-dy|>2` /
`|sx-dx|>2` 两处压平修正，**包括 `> dy-1` 的非对称开区间**）。

### ② `CM_TURN` 的 `+GD` ack 语义

- `ClientChangeDir`（`PlayObject.Operate.cs:330-347`）**只有** `nX == CurrX && nY == CurrY` 时才
  `Dir = nDir` + `Walk(RM_TURN)`；否则返回 false（`delayTime == 0` 时调用方发 `RM_MOVEFAIL`）。
- 成功支由调用方 `SendSocket(M2Share.GetGoodTick)`（`PlayObject.Message.cs:1012`）回 ack，
  `GetGoodTick = string.Format(MessageSettings.sSTATUS_GOOD, GetTickCount())`，`sSTATUS_GOOD = "+GD/{0}"`
  （`M2Share.cs:205` / `SystemModule/MessageSettings.cs:6`）。
- `SendSocket(string)`（`PlayObject.Socket.cs:48-71`）是**短包**：`PackLength = -len`，正文原样；
  game-gate 的 `client_frame(pack_length<0)` 产出真机观测到的 `#+GD/<tick>!`。
  Rust 侧新增 `SimToGate::SendRaw`（`main.rs`）+ `apply_sim_send_raw`（`pack_length = -len`）。

### ③ 下行帧 + AOI 收件人口径

| 帧 | oracle 编码点 | builder |
| --- | --- | --- |
| `SM_TURN`(10) | `PlayObject.Message.cs:1524-1560` | 已有 `smframe::sm_turn`（金标准锚定）；**`CM_TURN` 走 `downlink::turn_pair_nameless`（无名尾），出现帧走 `downlink::turn_pair`（带名尾）** |
| `SM_WALK`(11) | `PlayObject.Message.cs:1403-1410` | **新增** `smframe::sm_walk` |
| `SM_RUN`(13) | `PlayObject.Message.cs:1423-1430` | **新增** `smframe::sm_run` |
| `SM_HORSERUN`(5010) | `PlayObject.Message.cs:1413-1420` | **新增** `smframe::sm_horserun` |

**F-1（名尾归属）**：`PlayObject.Message.cs:1549-1555` 只在 `!string.IsNullOrEmpty(processMsg.Msg)`
时追加 `EncodeString($"{Msg}/{nObjCount}")`。`CM_TURN` → `ClientChangeDir` → `Walk(RM_TURN)`
→ `SendRefMsg(RM_TURN, Dir, CurrX, CurrY, 0, "")`（`PlayObject.Operate.cs:334` / `PlayObject.cs:1922`）
⇒ `Msg` **恒空** ⇒ `CM_TURN` 下行的 `SM_TURN` 尾**只有** `Encode(8B CharDesc)`，无名尾。
名尾**只属于**出现帧路径（`PlayObject.cs:3240` 传 `baseObject.GetShowName()`）；
`RM_WALK`/`RM_RUN`/`RM_HORSERUN` 同样无名尾。

移动帧形状（与 `SM_TURN` 的差别）：`MakeMessage(SM_*, ActorId, x, y, MakeWord(Dir, Light))`
+ 尾 `EncodePacket(CharDesc(Feature,Status))`（= `Encode(8B)`），**没有** `SM_FEATURECHANGED`、
**没有** `ShowName/Color`；`x/y` 是**移动后**的 `CurrX/CurrY`（`BaseObject.Walk` 在坐标更新后
`SendRefMsg(nIdent, Dir, CurrX, CurrY, 0, "")`，`BaseObject.cs:1585`）。

**收件人口径（"自己 vs 附近玩家"）**：帧由 `SendRefMsg`（`BaseObject.Message.cs:383-440`：
以自己为中心 ±`SendRefMsgRange`(12) 格内、`Race==Play && !Ghost`，**含自己**）广播；而
`RM_TURN/WALK/RUN/HORSERUN` 的收件人处理器都带 `if (processMsg.ActorId != ActorId)`
（`PlayObject.Message.cs:1403/1413/1423/1526`）⇒ **移动/转身帧只发给附近观察者，自己那条连接只收
`+GD` ack**。Rust 侧复用既有 AOI 先例 `chat::hear_targets`（±12 格 `Race==Play && !Ghost`），
**去掉自己**后按各收件人的 `[GateIdx, Socket]` 路由（`route_to_gate`）。

**世界侧移动入口**：`crates/world/src/world.rs` 新增 `World::try_step_now(actor_id, dx, dy, steps)`：
与 `Command::Walk` 共用同一个 `try_step`（阻挡/格子判据逐字节同构），只是**同步**（oracle 在消息
处理线程上立即改坐标）且 `steps>1` 时照 `RunTo`/`HorseRunTo` 的
`CanWalkEx(cur+1) && … && MoveToMovingObject(cur, cur+n)` **先逐格判可走、再一次性移动**（原子）。

**F-2（`+GD` 的 result 门）**：oracle 移动后还要求 `SpaceMoved || (CurrX==nX && CurrY==nY)`
才 `result=true`（`PlayObject.Attack.cs:1420`/`:1348`/`:1037`），`result` 为假则**不回** `+GD`
（`delayTime==0` 时改发 `RM_MOVEFAIL`）。但移动帧广播在 `WalkTo`/`RunTo` 内部的
`Walk(RM_WALK/RM_RUN)` 里**已无条件发出**（坐标更新后）⇒ 本片把「广播」与「`+GD`」分离：
广播照旧，`+GD` 受 `movewire::move_result_ok` 门控（本仓库无 `SpaceMoved` ⇒ 退化为 `at_target`）。

## 2. 用例与红检（DoD ④）

- `movewire::tests::{next_direction_eight_ways_and_flattening, direction_delta_is_unit_clockwise,
  steps_and_sm_ident_map, turn_ack_only_at_target, move_result_gate_is_at_target, good_tick_payload_shape}`：
  8 向 + 两处压平修正（含 oracle 的非对称边界）+ 1/2/3 格映射 + ack 条件 + **F-2 的 result 门**
  （目标非相邻 ⇒ 门假）+ `+GD` 形状。
- `world::tests::try_step_now_is_synchronous_and_atomic`：1/2/3 格成功、中间格被占 ⇒ **整段不动**、
  `(0,0)`/`steps<=0` 拒绝。
- `downlink::tests::move_frame_rebuilds_golden_sm_walk`：**从 M0 金标准 s2c `SM_WALK`(ident 11) 的
  真字节回读**（`x/y` 取头 `Param/Tag`、`Dir/Light` 取 `Series` 两字节、`Feature/Status` 取尾
  `Encode(8B CharDesc)` 解码）后重建，**逐字节相等**；非移动 ident 返回 `None`（不猜）。
- `downlink::tests::turn_pair_nameless_has_no_show_name_tail`（**F-1**）：展示表里有名字时，
  `turn_pair_nameless` 的 `SM_TURN` 尾**仍只有** `Encode(8B CharDesc)`（长度 12+11），不得含
  `ShowName` 字节；与 `turn_pair`（出现帧、带名尾）区分。
- `main.rs::tests::move_cmd_dispatches_turn_walk_ack_and_aoi`（**进程级**）：① `CM_TURN` 目标≠当前位置
  ⇒ 不改朝向、不回 ack、不发帧；② 目标=自身 ⇒ 改朝向 + 观察者收 `SM_TURN`+`SM_FEATURECHANGED`
  两帧 + 发起者收 `+GD`，且 **`SM_TURN` 尾逐字节 = `Encode(8B CharDesc)`**（F-1）；
  ③ `CM_WALK` ⇒ 坐标 10,10→11,10 + 观察者收 `SM_WALK`（ident 11、`MakeWord(Dir,Light)=MakeWord(2,0)`、
  移动后坐标 11,10）+ 发起者收 `+GD`；④ **F-2**：`CM_WALK` 目标非相邻（(14,10)）⇒ 仍广播
  `SM_WALK`（坐标 12,10），但**不得回 `+GD`**；⑤ **F-3**：`Entity.death=true` ⇒ `CM_TURN`/`CM_WALK`
  均不发帧/不回 ack/不移动/不改朝向。

**红检（改坏 ⇒ 必红；原始件 `red-*.txt`，均已 `cp` 还原、`git status` 干净）**：

| 红检 | 改法 | 结果 |
| --- | --- | --- |
| `red-turn-ack.txt`（原片） | `movewire::turn_is_at_target` 改成恒 `true`（无条件回 ack） | `move_cmd_dispatches_turn_walk_ack_and_aoi` **FAILED**（第①段「目标≠当前位置 ⇒ 不得回 ack」） |
| `red-sm-ident.txt`（原片） | `movewire::sm_ident_for` 的 `CM_WALK` 支改成 `SM_RUN` | 同上用例 **FAILED**（walk 帧 ident/坐标断言） |
| `red-f1-name-tail.txt`（**F-1**） | `handle_move_cmd` 的 `CM_TURN` 支改回 `downlink::turn_pair`（多出名尾） | 同上用例 **FAILED**（`left` 25B 尾 vs `right` 11B 尾） |
| `red-f2-ack-gate.txt`（**F-2**） | 去掉 `movewire::move_result_ok` 门（恒 `send_good_tick`） | 同上用例 **FAILED**（④ 段「非相邻仍回 +GD」） |
| `red-f3-death-guard.txt`（**F-3**） | 去掉 `handle_move_cmd` 的 `Entity.death` 守卫 | 同上用例 **FAILED**（⑤ 段「death=true 仍发帧/回 ack」） |

## 3. 门禁（DoD ⑤，隔离 `CARGO_TARGET_DIR=E:\tmp\openmir2-m2svr-target`，全 `--offline`）

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| fmt | `cargo fmt --all -- --check` | rc=0（`gates-fmt.txt`） |
| clippy | `cargo clippy --workspace --all-targets --offline -- -D warnings` | rc=0（`gates-clippy.txt`） |
| test | `cargo test --workspace --offline` | **73 suites / 576 passed / 0 failed / 16 ignored**（`gates-test-workspace.txt`） |
| msgcodegen | `cargo run -p msgcodegen --offline -- --check` | rc=0，604 条（`gates-msgcodegen.txt`） |

新用例所属 `-p game-svr` **3 连跑**：`gamesvr-3run-1..3.txt`，每次
`66 passed`（lib）+ `126 passed`（bin）+ `13 passed`（relay）= **207 passed**，0 failed。

## 4. 未接（登记，不静默、不按常识补）

见 `bins/game-svr/src/movewire.rs` 模块头：

1. **限速/超速**：`CheckActionStatus` + `MoveTick`/`WalkIntervalTime`/`RunIntervalTime` +
   `MoveCount`/`OverSpeedCount`/`BoEmergencyClose`/`CloseSpeedHackCheck`（`ClientWalkXY`/`ClientRunXY`
   的前半段）**未移植** ⇒ 本片不对移动频率做任何判定（客户端发多快就按多快走）。
2. **失败支 `RM_MOVEFAIL`**：`SendRefMsg(Messages.RM_MOVEFAIL, 0, 0, 0, 0, "")`（四个 `case` 的
   `delayTime == 0` 支）需要新的 `SM_MOVEFAIL` 编码器，本片只记 `DBG` 日志。
3. **`HealthTick` 扣减**：`ClientWalkXY` 的 `HealthTick -= 10`、`ClientRunXY`/`ClientHorseRunXY` 的
   `-60`（体力/饥饿面未移植）。
4. **`IsCanWalk`/`IsCanRun` + `STONE`（麻痹）**：状态位面未移植 ⇒ 本片**已按 `Entity.death` 判
   `Death` 守卫，但 `StatusTimeArr[PoisonState.STONE] != 0`（防麻）这一半**仍**未接**（不造假状态位）；
   骑马相关状态位同样未接。
5. **`SpaceMoved`**：地图路由换图（`Walk` 的 `CellType.MapRoute` 支 / `ChangeSpaceMove`）未移植
   ⇒ F-2 的 result 门退化为 `at_target`（`SpaceMoved` 那一半已登记）。
6. `CM_SITDOWN`(3014) 与四个移动 ident 同组（`WorldServer.cs:833`），**本片未接**（不在本卡范围）。

## 5. 边界与残留物（DoD ⑥）

- 未动 `src/**`（C# oracle）、`MirClient`、`crates/data`/`crates/storage`/`crates/protocol`、
  表结构/脚本语法/数据格式；**未扩任何白名单**。
- `crates/world` 只新增一个**同步移动**公开方法（与 `Command::Walk` 同判据），未改 tick/模拟语义
  （`no_parallel`、`ecs_migration`、world parity 用例全绿）。
- **端口**：本片不起栈、不占端口（只跑离线单测）。`ports-preflight.txt`（2026-10-12 08:19 实拍）中
  5000/5100/5500/5600/5700/6000/7000/7100/7200 全 free、仅共享 MySQL `3306` LISTENING（与本线无关）。
- 临时物：`E:\tmp\movewire.rs.bak`、`E:\tmp\m2svr-main.rs.bak`/`m2svr-main-f2.bak`/`m2svr-main-f3.bak`
  （红检还原用备份），均在 `E:\tmp`、未入库；证据全部入库到本目录。
