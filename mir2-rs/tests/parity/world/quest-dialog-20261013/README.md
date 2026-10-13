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
  `HostNpc`（`ScriptNpc` 适配）、`merchant_say_text`/`merchant_say_gbk`（说词行 + GBK 原文；
  **F-2 订正**：GBK 原文 ≠ 线上字节，线上尾 = `edcode::encode`）。

## 2. oracle 锚（详见 `01-oracle-anchors.txt`）

- `@Login`：`PlayObject.cs:1440` → `NormNpc.cs:1889` → `ScriptEngine.cs:38`。
- `@StdModeFunc<N>`：`PlayObject.Base.cs:1072`（`FunctionNPC = QFunction`）。
- 说词：`ScriptEngine.cs:987-998` 的 `ChrName + '/' + sMsg`；`PlayObject.Message.cs:1722-1724` → `SM_MERCHANTSAY(643)`。
- 装载：`NormNpc.cs:1591`（任务 NPC，`boFlag=false`）/ `Merchant.cs:1094`（QFunction，`boFlag=true`）。

## 3. 判据（构造级，全离线）

`03-unit-tests.txt`：`cargo test -p game-svr --lib npc::` **7 passed / 0 failed**：
1. `merchant_say_text_and_gbk_bytes` —— 行格式 `ChrName + '/' + sMsg` + 「欢迎」的 GBK 字节。
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

- **F-1（reviewer-5 `b164fa48`，阻塞）已返工**：`NpcPlayer` 的三组任务位图改用本仓已合入的
  `crates/script::engine::quest_state::QuestBitmap`（oracle `PlayObject.cs:3966-4110` 的
  `nFlag-=1` + `128>>n` MSB-first 位语义；数组 128），并补上 `quest_unit_status`/
  `quest_unit_open_status` 覆写；订正 `quest_flag` 由 `[u8;512]` → `[u8;128]`（M-1）。
  新用例 `npc::tests::quest_flag_and_unit_use_oracle_bit_semantics` + 返工说明见
  `../quest-dialog-wire-20261013/07-f1-f2-rework.txt`。
- **F-2（reviewer-5，重要）已订正**：`merchant_say_wire` 改名 `merchant_say_gbk`，doc/用例明确
  「GBK 原文 ≠ 线上字节」；线上尾 = `edcode::encode`（`PlayObject.Message.cs:1724`），由第②片的
  `smframe::sm_merchantsay` 负责。
- **M-3（适配器近似，登记不静默）**：`give_item` 只 push 不入堆叠合并、`bag_count = items.len()`、
  `worn`/`items` 由调用方填、`map_name` 在 `HostNpc` 恒 `"0"`、`curr_x/y` 恒 0 —— 这些都是
  「隐藏 NPC 不经地图」的占位，接真 `SessionGround`/`World`（第②片）前不得被下游当权威值用。
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
