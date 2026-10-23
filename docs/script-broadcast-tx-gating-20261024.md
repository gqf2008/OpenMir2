# 脚本全服广播是否被「触发者自己的网关通道」把关

卡 `openmir2-script-broadcast-tx-gating`（tier=**R**，owner `openmir2-worker-58`）。
触发来源：`openmir2-sendmsg-login-qmanage-cases` 的复审观察
（reviewer-14 `8a9b5dab`）——`drain_login_script_batch` 整段在 `if let Some(tx)` 内。

## 1. 目标

`WorldEngine.SendBroadCastMsg`（`src/GameSrv/Word/WorldServer.cs:1738-1765`）遍历
`PlayObjectList` 逐个 `SendMsg`，**与发送者自己的 socket 状态无关**。本仓若把
「自己那一条帧」和「发给别的玩家的帧」绑在同一个 `tx` 上，就只在触发者网关掉线的瞬间
暴露。核这件事，并**只改有偏差的**排水点。

## 2. 先证伪（实测，不是推断）

两条构造级用例，夹具把触发者放在 **gate 0（不在 `gates` 表里）**，普通收件人放在
**gate 1（活线）**（`tests::gate_gone_world`）：

| 用例 | 路径 | 改前实测 | 结论 |
|---|---|---|---|
| `login_broadcast_reaches_other_player_when_trigger_gate_is_gone` | `@Login` → `drain_login_script_batch` | **红**：`fanned = 0`，`other` 0 帧 | **有偏差** |
| `stdmodefunc_broadcast_reaches_other_player_when_trigger_gate_is_gone` | `@StdModeFunc*` → `run_std_mode_func_script` | **绿**：`other` 收到 1 帧 | **无偏差** |

改前红证据（`cargo test -p game-svr --lib 'login_broadcast_reaches_other_player_when_trigger_gate_is_gone'`）：

```text
assertion `left == right` failed: 触发者网关线缺席时，其余在线玩家仍须收到 1 条 `SM_SYSMESSAGE`
  left: 0
 right: 1
```

根因（改前 `lib.rs:22234`）：`@Login` 的整段（入图帧、`MarkInWorld`、`@Login` 脚本执行、
`drain_login_script_batch` 排水、写回）都在
`if let Some(tx) = route_to_gate(&gates, gate_idx, socket, …)` 内 ⇒ `route_to_gate` 返回
`None` 时**连脚本都不跑**，其余玩家自然零帧。`@StdModeFunc*` 路不同：
`run_std_mode_func_script` 只在发**自己**的帧时 `route_to_gate`，扇出在函数级无条件执行
（`lib.rs:7094`），所以本来就对。

## 3. 修复（只动有偏差的那一个）

- `drain_login_script_batch` 的 `tx` 参数改为 `Option<&UnboundedSender<SimToGate>>`：
  `None` ⇒ 跳过**本例玩家自己**的脚本帧，`fan_out_world_sysmessage` 照常扇出。
- `sim_loop` 进世界块：`let own_tx = route_to_gate(...)`；只有入图帧 / `MarkInWorld` /
  `@Login` 说词 / GM 直发帧 / 自己的脚本帧受 `own_tx` 把关；**脚本执行、写回、其余玩家扇出
  一律无条件**。
- 帧的相对次序不变：同一批仍先「说词 → 直发帧 → 脚本帧（`NetPriorityQueue` 出队序）」，
  `fan_out_world_sysmessage` 仍复用**同一份**定义（`lib.rs:7400`，全树唯一）。
  既有期望序用例 `login_sendmsg_case0_fans_out_to_other_online_player` /
  `qmanage_login_drain_fans_out_case2_and_keeps_case7_self_only` 逐字节仍绿。

## 4. 五个同类漏斗（逐个核，只有 1 处有偏差）

| # | 排水点 | 位置 | 是否在 `tx` 判空内 | 扇出在不在里面 | 结论 |
|---|---|---|---|---|---|
| 1 | `emit_script_frames` | `lib.rs:7354` | 本体只收 `&tx`（自带自己的帧）；**无扇出** | 否（扇出在调用方 `drain_script_frames_with_host` `:7596`，无条件） | 已核一致 |
| 2 | `run_party_goto_label` | `lib.rs:2325` | 自己的说词/脚本帧按 `route_to_gate`（`:2449`/`:2566`） | 否——扇出 `:2554` 在函数级，与 tx 无关 | 已核一致 |
| 3 | `emit_login_script_frames` / `drain_login_script_batch` | `lib.rs:7687` / `:7745` | 改前：**整个调用点在** `if let Some(tx)` 内 | **是（改前）** ⇒ 触发送者掉线时别人也收不到 | **有偏差，已修**（§3） |
| 4 | `guild_official::queue_script_output` | `guild_official.rs:416` | 本体只入每个收件人的队列；**无扇出** | 否——扇出在 `handle_select` 本体 `guild_official.rs:593`，调用点 `lib.rs:21001` 不受 tx 判空 | 已核一致 |
| 5 | `run_merchant_goto` | `lib.rs:7111` | 本体只把 `SENDMSG 0-3` 正文收进 `broadcasts` 出参；**无扇出** | 否——扇出在 `drain_script_frames_with_host`（`:7596`，无条件），调用点传 `Option<&tx>` | 已核一致 |

另：`drain_script_frames_with_host`（`:7555`）本身就是「自己帧 `tx.map_or` + 扇出无条件」的
正确形态，`@StdModeFunc*`（`:7094`）同。

## 5. 牙

把扇出挪回 `tx` 判空（在 `drain_login_script_batch` 里改成
`if tx.is_some() { fan_out_world_sysmessage(...) } else { 0 }`）⇒
`login_broadcast_reaches_other_player_when_trigger_gate_is_gone` 红：

```text
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1409 filtered out
exit code 101
```

`git checkout -- mir2-rs/bins/game-svr/src/lib.rs` 还原；`sha256sum` 与 HEAD blob 对齐（见 §7）。

## 6. 判级与窗口（R）

- `tier`: **R**。理由：结论涉及**收件人集合**与帧是否发出（玩家可见广播），且触发条件是
  网络状态而非构造级输入 ⇒ 构造级先钉分支，真机窗口再确认。
- `window_required`: **true**。窗口项（可复跑）：
  1. 起栈，两个客户端 A/B 进同一图（A 为触发者）。
  2. 断开/杀掉 A 的网关连接（保留 A 在 GameSvr 的在线登记与 `@Login` 触发路径；见下注）。
  3. 触发脚本广播（`@Login` 用令 A 重新进世界；`@StdModeFunc14` 用 A 吃「礼炮」）。
  4. 断言 **B 的客户端仍收到** 对应 `SM_SYSMESSAGE`（系统红字）。
  5. 反例对照：改前二进制同场景 B 收不到。

  > 注：真机上「A 掉线」与「A 仍在 `PlayObjectList`」在 oracle 是互斥的（断线即
  > `Disappear`/摘链）。本卡修的接缝是**宿主侧 `gates` 表缺线**而世界侧在线登记仍在
  > （`route_to_gate` 返回 `None`），这正是 `try_send`/网关重启瞬间的形态；窗口里以
  > 「网关线不可达但 GameSvr 仍认为 A 在线」构造，而不是让 A 走完整掉线流程。

## 7. 证据四件套

- 命令：`cargo test -p game-svr --lib 'trigger_gate_is_gone'`（2 passed）；
  `cargo test -p game-svr --lib 'sendmsg'`（10 passed）。
- 输出：见 §2 / §5。
- 路径：`mir2-rs/bins/game-svr/src/lib.rs`（`drain_login_script_batch`、`sim_loop` 进世界块、
  两条新用例）、本文件。
- hash：`mir2-rs/bins/game-svr/src/lib.rs` sha256 = `fc3c485028024cda6587dbbacbfcf5113acfe38b6ef0472511586e5428ebe5dc`
  （工作树 = `git show HEAD:…` blob，两处一致）。
- 门禁数值（合并树 `62db66c7e` + 本分支）：
  - `cargo fmt --all -- --check` rc=0；
  - `cargo clippy --workspace --all-targets -- -D warnings` rc=0；
  - `cargo test --workspace`：**141 suites / 3144 passed / 0 failed / 62 ignored**；
  - `cargo run -p msgcodegen -- --check`：604 条常量一致；
  - `python -I -B mir2-rs/tests/parity/m3/whitelist_anchor_gate.py check`：587 条锚 ALL PASS；
  - `python -I -B mir2-rs/tests/parity/coverage/script_host_noop_audit.py check --strict-lines`：GREEN 168 行；
  - 冲突标记：无；`git diff --check` rc=0；
  - `git merge-tree --write-tree refs/remotes/walgit/master HEAD` rc=0（tree `8ea92c9e`）。

## 8. 残留物清单

- 无新增进程、无起栈、无连库；未改 `tools/**`、`rules.json`、场景 JSON。
- 未复制第二份扇出；`fan_out_world_sysmessage` 全树仍只有 1 处定义。
- 窗口项未跑（R 卡，按 §6 登记为 `window_required: true`）。
