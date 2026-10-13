# M3⑥-实现 3 第①片证据 —— NPC 脚本运行时宿主核心（`openmir2-m3-quest-dialog-wiring`）

卡：`openmir2-m3-quest-dialog-wiring`（issue `3e9ef712a1`，`owner_hint = openmir2-worker-11`）。
本片 = **第①片（离线核心）**：把 `crates/script` 引擎接到 GameSvr 宿主侧（`bins/game-svr`），
装载隐藏任务 NPC 脚本、按 oracle 进入 `GotoLable`、并把说词收成可下发帧。
**不含** `main.rs` 的会话接线（见 §5）。

## 1. 写集 / 文件

- `mir2-rs/bins/game-svr/Cargo.toml`：+`mir2-script`（+`Cargo.lock`）。
- `mir2-rs/bins/game-svr/src/lib.rs`：+`pub mod npc;`。
- `mir2-rs/bins/game-svr/src/npc.rs`（新）：`NpcHost`（装载 + `@Login`/`@StdModeFunc<N>` 入口）、
  `NpcPlayer`（`ScriptPlayer` 宿主适配，字段由第②片用 `SessionGround`/`World` 填充）、
  `HostNpc`（`ScriptNpc` 适配）、`merchant_say_text`/`merchant_say_wire`（说词行 + GBK 原文）。

## 2. oracle 锚（详见 `01-oracle-anchors.txt`）

- `@Login`：`PlayObject.cs:1440` → `NormNpc.cs:1889` → `ScriptEngine.cs:38`。
- `@StdModeFunc<N>`：`PlayObject.Base.cs:1072`（`FunctionNPC = QFunction`）。
- 说词：`ScriptEngine.cs:987-998` 的 `ChrName + '/' + sMsg`；`PlayObject.Message.cs:1722-1724` → `SM_MERCHANTSAY(643)`。
- 装载：`NormNpc.cs:1591`（任务 NPC，`boFlag=false`）/ `Merchant.cs:1094`（QFunction，`boFlag=true`）。

## 3. 判据（构造级，全离线）

`03-unit-tests.txt`：`cargo test -p game-svr --lib npc::` **7 passed / 0 failed**：
1. `merchant_say_text_and_wire` —— 行格式 `ChrName + '/' + sMsg` + 「欢迎」的 GBK 字节。
2. `run_login_emits_say_with_chrname_prefix` —— `@Login` 经 `QManage` 产说词（前缀 = NPC 名，非优先）。
3. `run_std_mode_func_uses_numeric_label` —— `@StdModeFunc7` 命中、未定义 label 无说词。
4. `send_msg_args_are_recorded_in_order` —— `SendMsg` 五参实参保序（第②片落帧的输入）。
5. `loads_from_disk_and_runs_login` —— 真文件 `MapQuest_def/QManage.txt` 装载 + `@Login` 说词。
6. `missing_script_is_not_an_error` —— 缺脚本 `loaded=false` 且不报错（`File.Exists` 早退口径）。
7. `say_text_must_carry_chrname_prefix` —— 牙齿：前缀不能被吞。

## 4. 改坏必红（`04-redchecks.txt`，原始输出）

| 改坏 | 目标用例 | 读数 |
|---|---|---|
| `merchant_say_text` 去掉 `ChrName+'/'` | 1/2/3/7 | FAILED（4 failed），还原 ⇒ 7 ok |
| `send_say` 恒 `priority=true` | 2 | FAILED（1 failed），还原 ⇒ 7 ok |
| `run_std_mode_func` 丢 `AniCount` | 3 | FAILED（1 failed），还原 ⇒ 7 ok |

## 5. 未接（登记，第②片）

- `playerActor.GetScriptLabel(sMsg)`（`ScriptEngine.cs:990`）未建模。
- `main.rs`：进世界完成点调 `run_login` + 会话态 → `NpcPlayer` 快照/回写 + `RM_MERCHANTSAY` 经
  `MsgQueue` **出队序**落 `SM_MERCHANTSAY(643)` 帧（`SM_*` 编码层可复用 `downlink`/`smframe`）。
- 用物品 `UseStdModeFunItem` 的接线点：`bins/game-svr/src/useitem.rs` 的 `function_npc_present`（T-17）。
- `@StdModeFunc<N>` 的 `AniCount` 目前由调用方传；隐藏 NPC `ActorId` 恒 0（oracle 同）。
- 真机两侧同序列对拍要窗口 ⇒ 登记等 owner。

## 6. 门禁 / 边界 / 窗口

- `05-gates.txt`：fmt rc=0；clippy `-D warnings` rc=0；`test --workspace --no-fail-fast`
  **93 suites · 860 passed · 0 failed · 21 ignored**；msgcodegen 604 rc=0。
- `06-boundary.txt`：`git diff --name-only master..HEAD -- src sql MirClient` **空**。
- **未起栈 / 未占端口**（判据全离线）。

## 7. 证据清单

`01-oracle-anchors.txt` / `02-commands.txt` / `03-unit-tests.txt` / `04-redchecks.txt` /
`05-gates.txt` / `06-boundary.txt` / `README.md` / `SHA256SUMS.txt`。
