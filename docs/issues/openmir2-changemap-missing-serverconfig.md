# 换图簇缺 `SM_SERVERCONFIG(11029)`（登记差异）

> 发现于卡 `magic-push-batch-impl`（MG-35 瞬息移动 21）。**本卡只登记，不改产品**。
> 结论：`MG-35` 本批**不升格**，原因是本条已登记缺口；`MG-16(8)` / `MG-57(37)` 不受影响。

## 1. 现象（golden 实测）

`spacemove` 三支（s1/s5/s13）的 caster 侧 golden `Sent`（`mir2-rs/tests/parity/golden/magic/spacemove.json`）：

```text
806, 638, 633, 634(x,y,series), 766, 54, 11029, 807, 640
```

Rust 在同一位置给出 `806, 638, 633, 634, 766, 54, 807, 640` —— **缺 `11029`**。
其余字段（634 的 `param/tag/series`、807 落点、640 训练尾）两侧逐字节一致。

## 2. oracle 依据

`src/M2Server/Player/PlayObject.Message.cs:1772-1778` 的 `case RM_CHANGEMAP` 是同支**四连发**：

```text
SendDefMessage(SM_CHANGEMAP, ActorId, CurrX, CurrY, DayBright(), processMsg.Msg);  // 634
RefUserState();                                                                      // 766
SendMapDescription();                                                                // 54
SendServerConfig();                                                                  // 11029
```

`SendServerConfig()`（`src/M2Server/Player/PlayObject.Base.cs:941-971`）在 `SoftVersionDateEx == 0`
时直接 `return`，否则 `EDCode.EncodeMessage(SystemShare.Config.ClientConf)` 经
`SendDefMessage(SM_SERVERCONFIG, 0,0,0,0, sMsg)` 发出（尾是**双重编码**）。
装置四个对象都设 `SoftVersionDateEx = 20240101` ⇒ 该帧必发。

## 3. Rust 现状与缺口位置

- 换图簇装配：`game_svr::sessionwire::map_change_frames`（633/634/[801|807]/766/54，
  `Route::SelfOnly` 与 `Route::Ref` 分类）。
- 生产 drain：`drain_map_change` → `drain_map_change_at_time`（同一 tick 的下行段统一发出）。
- `smframe::sm_serverconfig(cfg)` **已在仓内**（登录路径使用），本缺口缺的不是装配函数，
  而是**输入拿不到**。

## 4. 为什么本批不做（成本核算）

`SendServerConfig` 需要两样输入：

1. `GameSvrConf`（`client_conf.to_bytes()`）——只在 `sim_loop` 的 `enter_cfg.game_conf` 里；
2. 每个会话的 `SoftVersionDateEx`——在 `SessionGround.soft_version_date`（`handle_move_cmd`
   一侧有），但 `drain_map_change_at_time` 的入参里没有会话表。

显式传参要改：

- `drain_map_change` / `drain_map_change_at_time` **2 个签名**；
- **外部调用点 13 处**（生产 3：`handle_move_cmd` ×2 + `sim_loop` ×1；测试/driver 10：
  `eat_move` 5 + `lib.rs` 单测 **4** + 21 驱动 1），另 `drain_map_change` 体内 1 处转调
  ⇒ **14 处要动**；
- `handle_move_cmd` 还要加参数、并同步它的 **21 处调用**（1 生产 + 20 测试）。

复核实测（2026-11-21，`items-it13` 树）：

```text
$ grep -cE '^\s*handle_move_cmd\(' mir2-rs/bins/game-svr/src/lib.rs
22                       # = 1 个定义行 + 21 个调用点

$ grep -rn 'drain_map_change' mir2-rs/bins/game-svr/src/*.rs
# 外部调用：lib.rs 12577/12669/32818（生产）+ 68535/68623/92967/110036（单测 4）
#          + 71768（21 驱动）+ eat_move 484/516/731/1136/1394（5）
# 函数内转调：lib.rs 19795（drain_map_change → drain_map_change_at_time）
```

超过卡 `magic-push-batch-impl` 的止损线（>5 调用点）⇒ 停手登记。
另：靠全局 `OnceLock` 存配置会造成"第二份配置真源"，与 `enter_cfg` 单源口径冲突，不采用。

## 5. 移除条件

满足任一即可接线，并在同一提交删除本条登记：

- 把 `GameSvrConf`（或已序列化的 `client_conf` 字节 + 会话 `SoftVersionDateEx`）显式注入
  `drain_map_change*`；或
- 让 `MapChange` 事件携带该输入（会动 `crates/world-bevy` 的公开 API，需单独立卡评估）。

接线后的验收：`spacemove` 三支的 caster 侧 `Sent` 出现 `54 → 11029 → 807`（**严格序**），
且 `SM_SERVERCONFIG` 的尾与 golden 逐字节一致；`SoftVersionDateEx == 0` 的负例不得发帧
（可用登录路径既有用例同形）。

## 6. 比较器的处置（本批）

`real_rust_ledger_spacemove` **保持未通过**，且**不得**用白名单/`keep` 集合把 `11029` 排除
（那会把"缺帧"洗成静默绿）。失败原句必须点名 `11029`：

```text
[某 label/caster] Sent 条数不等（golden=9 rust=8）：golden 有 rust 无=[(11029, 0, 0, 0, 0, 0, "<尾>")]
```

比较器的 `#[ignore]` 理由保持"需先跑驱动生成台账"，**不**加"已知缺 11029"之类跳过措辞
（默认 `cargo test` 不受影响；显式 `--ignored --exact` 才会跑到）。

## 7. 与 `634.series` 无关

`634` 的 `series` 时变（真机小时 ⇒ `DayBright()`）已在卡 `magic-push-batch-impl` §4b 处理
（golden 记 `Fixture.map_bright`、驱动注入），与本条的缺帧是两个独立问题。

## 8. 第三条路评估：「把两样输入放进 `World`」（2026-11-21，复核 `0e2eb1316`）

> 提议（opc-19）：`drain_map_change_at_time` 已有 `&mut World`；若把 `client_conf` 字节
> 建 World 时写一次、把每会话 `SoftVersionDateEx` 放进 World（或从 `world.sessions` 读），
> 则签名一处不用改、成本接近零。
>
> **结论：不可行（按当前口径），不构成「接近零成本」**。它把 §4 的「14 处调用点」成本
> 换成 **`crates/world-bevy` 公开 API 变更 + 会话状态复制**，与 §5 第二条
> （「让 `MapChange` 事件携带该输入……需单独立卡评估」）属同一类改动，风险更大。

### 8.1 三个问题的答复

**Q1：放进 World 算不算「第二份配置真源」？**
- 判据：**是否存在第二条写入路径、副本能否与源分歧**。
- 事实：`GameSvrConf` 是本仓唯一配置源（`lib.rs` 的 `red_home_of` 注释明写「这里只从
  `enter_cfg.game_conf` 取，**game-svr 不再另存第二份**」）；`game_conf` 全程只读
  （`grep -rn "enter_cfg.game_conf" lib.rs` = 读 10 处、写 **0** 处）。
- 结论：把 `client_conf.to_bytes()` 作为**派生字节缓存**放进 World，**不构成第二真源**
  （可在 `sim_loop` 启动的唯一写入点写入），但它**突破上述既有纪律的措辞**，需要在该处
  显式登记例外；且它只是两样输入里「较简单的一半」。

**Q2：`soft_version_date` 是否每会话可变？能否建 World 写一次？**
- **不能写一次**。事实：
  - `world.sessions` 的 `Session`（`crates/world-bevy/src/session.rs` 的 `pub struct Session`）
    字段只有 `chr_name / actor_id / map_id / stage / soft_close_count` —— **没有** `soft_version_date`；
  - 该值只在宿主 `SessionGround`（`lib.rs` 的 `struct SessionGround`；
    `ground_sessions: HashMap<String, SessionGround>`，`handle_move_cmd` 等宿主函数以 `&mut` 持有）；
  - oracle 写入点：登录 `src/GameSrv/Word/WorldServer.cs` 的 `GetPlayObject`（`SoftVersionDateEx =
    LoadUser.SoftVersionDate`）、init 清零 `src/M2Server/Player/PlayObject.cs` 的 `Clear`；
    宿主在登录链 `lib.rs` 的 `GateEvent::LoadDbRequest` 装配（`soft_version_date: cert.client_version`）取得。
  - 重登/重连会重建会话 ⇒ 同一角色不同客户端版本可得到不同值 ⇒ **必须每会话读当前值**。
- 结论：第三条路在此处必须二选一——**(a)** 在 World 里复制一份会话状态并在登录/重登同步
  （公开 API + 状态复制 + 陈旧风险）；**(b)** 把 `ground_sessions` 传进 drain（即 §4 已计路径）。

**Q3：`World` 加字段是否违反其它口径？**
- `crates/world-bevy/src/world.rs` 模块头：「**不实现 M1 的边界服务（登录/网关/DBSrv），
  只做世界内的 tick/实体/地图/AOI/会话/战斗**」。
- 既有 World 配置字段（`World::b4_cfg`、`World::mag_turn_undead_level`）是**玩法参数**
  （世界模拟的直接输入）；`client_conf` 是**客户端协议载荷**（边界产物），
  `soft_version_date` 是**登录身份字段**——性质不同。
- 结论：放进 World 需先修订该边界声明并说明理由（不能在 magic 批次里「顺手加字段」）。

### 8.2 若仍要做：改动面与止损线（供单独立卡）

- `crates/world-bevy/src/world.rs`：新增 `client_conf: Option<Vec<u8>>` 与写入口 / `World::new` 缺省；
- `crates/world-bevy/src/session.rs`：`Session` 增 `soft_version_date: i32`（或 World 层 actor→value 表）
  并定义同步时机（登录、重登、小退再进）；
- `bins/game-svr/src/lib.rs`：`sim_loop` 启动写 client_conf（1 处）、登录链写会话值（1–2 处）；
- `sessionwire::map_change_frames`：仍要增一个参数（新签名 1 + 调用 2 + 测试若干）；
- **止损线**：跨 `crates/world-bevy` 公开 API ⇒ 单独立卡（§5 第二条同款）。

### 8.3 复用计数核对（复核实测，`0e2eb1316`）

- `grep -cE '^\s*handle_move_cmd\(' lib.rs` = **22**（1 定义 + 21 调用）——与 §4 一致；
- `drain_map_change(` / `drain_map_change_at_time(` 的外部调用：`drain_map_change` 在 `lib.rs` 7 处
  （其中生产 2 处位于 `handle_move_cmd`、1 处位于 `sim_loop`，其余为单测）+ `eat_move.rs` 4 处；
  直接调 `drain_map_change_at_time` 2 处（`lib.rs` 的 21 驱动、`eat_move.rs` 的注入时钟测试）
  = **外部 13**，加 `drain_map_change` 体内转调 1 处 = **14** —— 与 §4 一致。第三条路不减少
  这 13 处中的任何一处（仍要读 World），只是把输入「送进 World」的代价从宿主签名转移到
  world-bevy API。

### 8.4 将来接线时的验收口径（沿用 §5）

- `spacemove` 三支 caster 侧 `Sent` 严格序 `54 → 11029 → 807`；
- `SM_SERVERCONFIG` 尾（双重 `EDCode`）与 golden 逐字节相等；
- `SoftVersionDateEx == 0` 负例不发 11029（登录路径既有用例同形）；
- `client_conf = None`（`Server.conf` 读不到）同样不发并登记方向（oracle 该情形起不来）；
- 全量比较器零漂移（含 `spacemove` 由红转绿后的 42 支）。
## 9. 第五路评估：「在 `map_change_frames` 里拼 11029」（2026-11-21，同一 head）

> 提议（opc-19）：换图簇由 `sessionwire::map_change_frames` **装配**、drain 只负责发；
> 而 `map_change_frames` 的调用方在 `handle_move_cmd` 里 —— 那里 `&mut SessionGround` 在手，
> 若能就地拼 11029，则 `drain_map_change*` 签名一处不用改，成本从 14 处掉到 2–3 处。
>
> **结论：前提不成立（按代码事实）。** `map_change_frames` 的生产调用点**不在**
> `handle_move_cmd`，而在 `drain_map_change_at_time` 内；该处既无会话表、也无 `game_conf`。
> 成本仍是 §4 的 14 处（或改为 `world-bevy` 公开 API 变更），**不降到 2–3 处**。

### 9.1 三个问题的答复

**Q1：`map_change_frames` 的调用点在哪？调用处能否拿到该会话的 `soft_version_date`？**
- 生产调用点**只有 1 处**：`drain_map_change_at_time` 体内对 `map_change_frames` 的调用；
  另有 **4 处单测**（`sessionwire.rs` 的 `mod tests` 内）。
- `drain_map_change_at_time` 的参数是 `(world, online, gates, displays, registry, game_time)`——
  **没有会话表**；`OnlineEntry`（`lib.rs` 的 `struct OnlineEntry`）只有
  `account/chr_name/session_id/gate_idx/socket/actor_id` 六个字段，**没有** `soft_version_date`
  （`grep "OnlineEntry {" ... | grep -c soft_version_date` = 0）。
- ⇒ 该调用处**拿不到** `soft_version_date`。

**Q2：该调用处能否拿到 `client_conf`？离 `enter_cfg.game_conf` 有多远？**
- 拿不到。链路：`sim_loop`（持有 `enter_cfg`，`game_conf` 只读）
  → `drain_map_change`（宿主包装）
  → `drain_map_change_at_time`（`drain_map_change` 的体内转调，**跨两层**）
  → `map_change_frames`。
- 另两个生产 drain 中继点在 `handle_move_cmd` 内（其参数含 `sessions`，但**不含** `game_conf`）。

**Q3：若在 `map_change_frames` 里拼 11029，`drain_map_change*` 是否一处不用改？**
- **仍要改**：两样输入必须从 `sim_loop` 到达 `map_change_frames`，而中间两层的参数里都没有；
  除非（i）改 `drain_map_change`/`drain_map_change_at_time` 签名（2 签名 + 13 外部调用点 = §4 的 14），
  或（ii）把输入放进 `World`/`MapChange`（`world-bevy` 公开 API，§5 第二条已定「单独立卡」）。
- ⇒ 第五节路不减少任何调用点，只是把「拼帧位置」从 drain 尾部移到装配函数内部。

### 9.2 评估中确认的一个真实简化（只解决一半）

`client_conf` 有**既有先例**可不再走会话表：`chat::install_send_ref_msg_range` 在 `sim_loop` 启动时
把 `[Setup] SendRefMsgRange` 装进**进程级 atomic**（`SEND_REF_MSG_RANGE_CONFIGURED`，
`chat.rs` 的 `install_send_ref_msg_range`），并保留线程内测试覆盖。
若按同款把 `client_conf.to_bytes()` 在启动时 install 一次（单源、只读、无第二条写入路径），
则 `client_conf` 这一半的成本 ≈ **1 处 install + 1 处读取**，且与 §8-Q1 的「派生缓存非第二真源」判定一致，
**不违反**现有纪律（先例就在仓内）。

但**另一半仍无解**：`soft_version_date` 是**每会话**值（登录设、init 清零、重登可变，
见 §8.1-Q2），既不在 `OnlineEntry`、也不在 `World::Session`、也不在 drain 的入参里。
⇒ 最小组合只能是：
- (A) `client_conf`：启动 install 一次（约 2 处）；
- (B) `soft_version`：二选一 —— ①给 `world-bevy::Session`（或 `MapChange`）加字段并在登录/重登同步
  （公开 API，单独立卡）；②把 `sessions` 传进 `drain_map_change*`（2 签名 + 13 调用点）。
- 无论 (B) 选哪条，(总成本) 都不低于「1 处公开 API 变更」或「14 处调用点」，**没有 2–3 处的组合**。

### 9.3 将来接线时的验收口径

沿用 §5/§8.4：`spacemove` 三支 `54 → 11029 → 807` 严格序；`SM_SERVERCONFIG` 双 EDCode 尾逐字节；
`SoftVersionDateEx==0` 与 `client_conf=None` 负例不发；42 只比较器零漂移。
## 10. (B) 路线前置事实与设计约束（2026-11-21，复核同一 head，待 opc-19 定稿）

> 裁定方向：选 (B)——`soft_version_date` 进 `World::Session`、`client_conf` 走进程级 install，
> 单独成批、单独开卡。本节是落笔前的三问答复与硬约束，**不改代码**。

### 10.1 Q1：会话建立路径清点（逐个给位置）

- **创建点只有一个**：`SessionRegistry::open`（`crates/world-bevy/src/session.rs` 的
  `pub(crate) fn open`），`Session::new` 在全仓**只被它调用**；公开包装是
  `World::open_session`（`session.rs` 的 `pub fn open_session`）。
- **生产调用点只有 1 处**：`sim_loop` 在 DB 载入回复时 `world.open_session(&head.chr_name)`
  （`lib.rs` 内、`session_ground_from_login(SessionGroundInputs { … })` 同一段）；其余 246 处均为单测。
- **进图/换图不建会话**：`World::enter_map`/`World::switch_map` 只改同一 `Session` 的
  `map_id/stage`（`switch_map` 用例可证：切图后 `visible_actors` 清空、实体换图，但 Session 不重建）。
- **重登不重建会话**：`SessionRegistry::open` 幂等（按 `chr_name` 命中即返回），且模块注明确写
  「会话**从不删除**」（`session.rs` 的 `SessionRegistry` 文档）⇒ 重登复用同一 `Session` 对象。
- ⇒ **值同步点是「每次登录/重登」而不是「首次建会话」**：同一处在 `session_ground_from_login`
  之后执行（生产唯一一处）；交易等命令里的 `ground_sessions.insert` 是**既有结构写回**
  （`handle_deal_cmd` 回写 `me_sg`/`other_sg`），不新建会话、无需额外同步。

### 10.2 Q2：`SoftVersionDateEx == 0` 的生产语义（负例怎么造）

- oracle：init 清零（`src/M2Server/Player/PlayObject.cs` 的 `Clear`），登录时
  `SoftVersionDateEx = LoadUser.SoftVersionDate`（`src/GameSrv/Word/WorldServer.cs` 的 `GetPlayObject`）。
  ⇒ **0 = 未带版本的客户端/库值为 0**（老客户端；`PlayObject.cs` 的 `boOldClientShowHiLevel` 分支
  也以 0 判定老客户端）。生产里「谁是 0」= 客户端证书里 `client_version == 0` 的连接。
- 宿主：该值来自登录链的 `cert.client_version`（`GateEvent::LoadDbRequest` 装配），
  0 会被原样带进 `SessionGround.soft_version_date`。
- 负例造法：把该会话的 `soft_version_date` 置 0（宿主既有用例已这样做），断言 11029 **不出现**；
  装置侧四个对象全设 `20240101`（golden 已固定）。

### 10.3 Q3：止损线（超过即停手回报）

- 允许的文件面 **≤3**：`crates/world-bevy/src/session.rs`（字段+访问器）、
  `bins/game-svr/src/lib.rs`（登录同步 + `client_conf` install + 读取）、
  以及（若确需）`crates/world-bevy/src/world.rs`（`World::open_session` 透传）。
- 值同步点 **≤2**（登录/重登同一处算 1；若出现第 3 条必须回报）。
- **任一创建/同步路径拿不到版本值 ⇒ 停手回报**，不得靠缺省 0 静默通过（见 10.4）。

### 10.4 硬约束：缺省 0 且 0 恰好 = 不发（opc-19 裁定）

- `Session` 新字段缺省 **0**，且 0 **必须**对应「不发 11029」（与 oracle `SoftVersionDateEx == 0 ⇒ return` 同义）。
  ⇒ 「漏设某条路径」的失败方向是**少一帧**（比较器点名可见），而不是**发错版本值**（尾字节不等，难辨）。
- **一条测试枚举所有会话建立/同步路径**（`open_session`、`enter_map`、`switch_map`、重登复用），
  逐路径断言该会话的 `soft_version_date` 是「非 0」或「显式登记为 0」⇒ 把「我数过所有路径」变成证据。
- 任何「拿不到版本值」的路径必须在本卡点名登记，不得依赖缺省 0 静默略过。

### 10.5 验收口径（补充 §5/§9.3）

- `spacemove` 三支 caster 侧 `Sent` 严格序 `54 → 11029 → 807`；尾（双 EDCode）逐字节等于 golden；
- `soft_version_date == 0` 的会话不发 11029（负例）；`client_conf = None` 同样不发并登记方向；
- 全量比较器零漂移（含 `spacemove` 42 支）；
- 10.4 的「路径枚举测试」必须存在且在其覆盖面内全绿。

## 11. 变异实验教训（2026-11-21，MG-35 实现批）

做「640 `due_ms_rel` +50ms 必红」的牙时，曾用 `git checkout -- <path>` 还原变异，把**尚未提交的实现**一并还原；已按补丁重放，并在**已提交态**上把全部证据重跑。**变异前必须先提交或快照**（章程 §16 的实操版）。判断标准很简单：**如果 `git status` 里有你要保留的未提交改动，就不要用 `git checkout -- <path>` 还原。**

## 12. MG-35 行为牙：640 `due_ms_rel` 的**最小可观测幅度 = +1ms**（2026-11-21，实跑记录）

> 背景：牙审计（`3b` 批次 C）把 MG-35 判"证据不足"而非"无牙"——§11 记过做过「+50ms 必红」
> 的牙，但**没留 `left/right` 原文**；卡里的"Δ=50 必红"是判别力声明。本节把它补成**记录**。

**变异点**：`mir2-rs/bins/game-svr/src/lib.rs` 共享尾分支（8/21/37）的 640 延迟发射——
把 `emit_magic_lvexp_delayed(...)`（未升级 ⇒ 1000ms）改为内联，**仅对 21（`SKILL_SPACEMOVE`）
把 delay 置 `1001`**（另试 `1050`），其余魔号仍 1000。UP 支仍 800（未触碰）。

**基线**：`spacemove` golden 的 caster 640 `due_ms_rel` = **s1 `1016` / s5、s13 `1000`**
（`DeliveryTime − t0`，`Environment.TickCount` 15.6ms 粒度）；Rust 基线 = `1000`。

**红线（+1ms，最小幅度）**
- **驱动（各自独立进程）**——不是"只有比较器红"：
  ```
  [spacemove/s1]  caster 640 的 due_ms_rel 超 TickCount 容差（|Δ|≤32ms）：golden [1016] 台账 [1100]
  [spacemove/s5]  caster 640 的 due_ms_rel 超 TickCount 容差（|Δ|≤32ms）：golden [1000] 台账 [1100]
  [spacemove/s13] caster 640 的 due_ms_rel 超 TickCount 容差（|Δ|≤32ms）：golden [1000] 台账 [1100]
  ```
- **比较器**（`real_rust_ledger_spacemove`）：
  ```
  真实台账与金标准不等（3 条）：
  [s1/caster]  Sent[8].due_ms_rel 超出 TickCount 容差（|Δ|≤32ms）：golden=1016 rust=1100
  [s5/caster]  Sent[8].due_ms_rel 超出 TickCount 容差（|Δ|≤32ms）：golden=1000 rust=1100
  [s13/caster] Sent[8].due_ms_rel 超出 TickCount 容差（|Δ|≤32ms）：golden=1000 rust=1100
  ```
⇒ 该字段由**驱动 + 比较器双守**（不是仅比较器独守）。

**最小可观测幅度 = +1ms**：装置/驱动按 **100ms 步进**排水；delay ∈ (1000,1100] 都会落到下一步
⇒ 实测 `due_ms_rel = 1100`。`+50ms`（`1050`）给出**同一** `1100`，因此"50"不是阈值，**+1ms 即可**；
更小（0ms 差）不红是预期（基线）。**这是"装置步进粒度决定的判别力下限"，不是判据放宽。**

**还原证明**：`cp` 快照还原后
`git hash-object mir2-rs/bins/game-svr/src/lib.rs` = `git rev-parse HEAD:mir2-rs/bins/game-svr/src/lib.rs`
= `08f137a3f71fb55e56fa5a35f24de865fdf6714d`；`grep -c MUTATION` = 0；
重跑 spacemove s1/s5/s13 驱动（各 1 passed）与比较器（1 passed）复绿。
