# 脚本 `GMEXECUTE` 的宿主跳线（卡 `openmir2-gm-script-host-seam`，T-210）

> tier=R。7 张 `openmir2-gm-impl-*` 卡（`-release`/`-kick`/`-showeffect`/`-mobfireburn`/`-smake`/`-clearbag`/`-startquest`）
> 与 T-209（`@禁言`）的共同前置：`@Login` 里的 `Gmexecute 禁言 self 10000` 此前在 Rust 侧**零帧**。

## 1. 问题与结论

- 现象：`bins/game-svr/src/npc.rs` 的 `impl ScriptPlayer for NpcPlayer` 未覆写 `permission`/`set_permission`/
  `process_user_line_msg`，而 `crates/script/src/engine/mod.rs:185/547/549` 的缺省是 no-op（`permission()=0`、
  setter/命令入口为空）⇒ 脚本动作 `ActionOfGmExecute` 抬权后"执行"的那一行什么都没发生。
- 结论：跳线已接。`@Login`（`lib.rs` 的 QManage 段）现在把会话 `GmSession` 作为 `Permission` 的权威装进
  `NpcPlayer::gm`；命令结果按 `MsgQueue` 投影入 `NpcPlayer.frames`，`SendDefMessage` 直发帧另列
  `gm_direct_frames` 并在排水前下发。

## 2. oracle 链路逐段（`src/` 冻结面，只读核对）

| # | 位置 | 行为 |
|---|---|---|
| 1 | `ExecutionProcessingSys.cs:4925` | `ActionOfGmExecute(normNpc, playerActor, questActionInfo, ref Success)` |
| 2 | `:4932-4936` | 取 `sParam1..sParam6` |
| 3 | `:4933-4934` | `string.Compare(sParam2, "Self", OrdinalIgnoreCase) == 0` ⇒ `sParam2 = playerActor.ChrName` |
| 4 | `:4937` | `sData = string.Format("@{0} {1} {2} {3} {4} {5}", …)`（尾部空参 ⇒ 尾随空格） |
| 5 | `:4938` | `byte btOldPermission = playerActor.Permission` —— **进入前的值**（不是常量 0） |
| 6 | `:4939`/`:4944`/`:4946` | `try { … } finally { playerActor.Permission = btOldPermission; }` |
| 7 | `:4941` | `playerActor.Permission = 10` |
| 8 | `:4942` | `playerActor.ProcessUserLineMsg(sData)` |
| 9 | `PlayObject.Chat.cs:229` | `ProcessUserLineMsg(string sData)`；`:402-406` 整段 `catch` 吞命令体异常（零帧） |
| 10 | `Chat.cs:241-362` | 密码状态机：`IsSetStoragePwd` → `IsReConfigPwd` → `IsUnLockPwd\|\|IsUnLockStoragePwd` → `IsCheckOldPwd`，**先于** `@` 支，命中即整条线被吃 |
| 11 | `Chat.cs:363-367` | 非 `@` ⇒ `ProcessSayMsg(sData)` |
| 12 | `Chat.cs:368-370` | `@` ⇒ `M2Share.CommandSystem.Execute(this, sData)`，返回 true 即结束 |
| 13 | `CommandSystem.cs:125-196` | `ExtractCommandAndParameters`（`:171`，失败 ⇒ 无帧）→ `CommandMaps.TryGetValue`（`:179`）→ 命中 `command.Handle(parameters, PlayerActor)`（`:181`）→ 未命中先发 `未知命令: {command} {parameters}`（`:185-188`）→ 输出非空则 `SysMsg(output, Red, Hint)`（`:191-194`） |
| 14 | `GameCommand.cs:37-40` | `PlayerActor.Permission < PermissionMin` ⇒ `CommandHelp.GameCommandPermissionTooLow`（`CommandHelp.cs:12` = `权限不够!!!`） |
| 15 | `GameCommand.cs:42-58` | 按 `MethodParameterCount` 派发命令体 |
| 16 | `Chat.cs:372-400` | 未命中回落第二条：`@{sCMD}此命令不正确，或没有足够的权限!!!`（`Red`/`Hint`） |

**还原值核对**：`:4938` 保存、`:4946` 还原的都是 `playerActor.Permission` 的**进入前值**（`byte`），
不是 0。Rust 侧 `PermissionGuard.old` 同口径，用例分别以进入值 `7`（脚本 crate）与 `3`/`7`/`0`/`10`
（宿主 crate）验证。

## 3. 实现

1. `crates/script/src/engine/actions.rs`
   - 新增 `PermissionGuard<'a> { player: &mut dyn ScriptPlayer, old: i32 }`，`Drop` 里 `set_permission(old)`
     ⇒ 表达 `finally`：正常出口、提前返回、panic 展开三条路径都还原。
   - `ActionOfGmExecute` 改为「取旧值 → 抬到 10 → 守卫 → `process_user_line_msg(&s_data)` → drop 守卫」。
   - 新增 `permission_guard_tests`：驱动**真实** `Engine::run_action(229, …)`，panic 与正常出口各一条。
2. `bins/game-svr/src/npc.rs`
   - 新增 `NpcGm<'r> { sess: &'r mut GmSession, cfg: &'r GmConfig, world: GmWorld<'r>, pwd_busy: bool }`。
   - `NpcPlayer<'r, 'g>` 增加 `permission`/`gm`/`gm_direct_frames`/`gm_notes`（第二个生命期 `'g` 让桥的借用
     只活到 `player.gm = None`/块尾）。
   - 覆写：`permission()`（有桥读 `GmSession.permission`）、`set_permission(v)`（`unchecked` 截断到 `u8`）、
     `process_user_line_msg(line)`（密码流 ⇒ 不执行 + 登记；非 `@` ⇒ 不代发说话 + 登记；`@` ⇒ `gmcmd::execute`
     → `SysMsg` 入队 → `!found && parsed` 补回落 → `GmFrame` 入直发队列）。
   - `crates/script` 的缺省 no-op **未改**；无桥的 `NpcPlayer` 行为与接卡前完全一致（有用例）。
3. `bins/game-svr/src/gmcmd.rs`
   - **删除 `execute_script_line`**：它是 oracle 抬权点之外的第二份「抬/还」，跳线接通后会与
     `ActionOfGmExecute` 重复。抬权唯一地址 = 脚本动作（oracle 同址）。原用例改为走 `NpcPlayer` 跳线。
4. `bins/game-svr/src/lib.rs`（`@Login` 生产段）
   - 装配 `NpcGm`（`sess = &mut ground_sessions[chr].gm`、`GmWorld` 取 `&guilds`/`registry`/`&world`）；
     `pwd_busy` 由会话层 `pwd_sessions`（`CM_PASSWORD` 状态机）算出。
   - 「建快照 + 跑 `@Login`」收进独立块，块内取出 `LoginScriptWriteBack`（写回标量）与三组帧；
     块外排水/写回，避免 `&mut session` 与后续不可变借用冲突。
   - `emit_login_script_frames` 改为收 `frames: &[SendMsg]`（+ 无会话回退标量）。
   - 直发帧在 `MsgQueue` 投影排水**之前**下发（oracle `SendDefMessage` 即时语义）；`gm_notes` 记 DBG。

## 4. 用例与牙

用例（`cargo test -p game-svr --lib gmexecute_seam` / `-p mir2-script --lib permission_guard`）：

| 判据 | 用例 |
|---|---|
| ① 抬权后真执行（恰好一条 SysMsg，逐字节） | `gmexecute_seam_shutup_self_emits_exactly_one_sysmsg` |
| ② finally 还原（进入值 `7` / `10` 各一次） | `gmexecute_seam_restores_entering_permission_seven` / `_ten` |
| ③ 失败路径仍还原 | `gmexecute_seam_unknown_command_still_restores_permission`；`gmexecute_action_restores_permission_on_panic`（panic 展开）；`gmexecute_action_restores_permission_on_normal_exit` |
| ④ 未抬权仍 `权限不够!!!`（不泄漏） | `gmexecute_seam_without_lift_is_permission_too_low` |
| ⑤ `self` 与显式名字 | `gmexecute_seam_target_resolution_self_and_explicit_name` |
| 拦截层 | `gmexecute_seam_is_blocked_by_password_state_machine` |
| 无桥 no-op | `gmexecute_seam_without_bridge_is_noop` |
| 跳线层（gmcmd） | `shutup_script_gmexecute_path_through_npc_player_seam` |

牙（逐字节还原后重跑全绿）：

1. **删抬权**：`actions.rs` 注释掉 `self.player.set_permission(10);` ⇒
   `gmexecute_seam_shutup_self_emits_exactly_one_sysmsg` 红：
   `left: [("权限不够!!!", 255, 56)] right: [("新人 已被禁言10000分钟", 255, 56)]`。
2. **还原只在正常路径**：把 `PermissionGuard` 换回「执行后顺序赋值」⇒
   `gmexecute_action_restores_permission_on_panic` 红：`left: 10, right: 3`。

## 5. 门禁

| 门 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | rc=0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 |
| `cargo test --workspace` | 3028 passed / 0 failed / 62 ignored |
| `cargo run -p msgcodegen -- --check` | rc=0（604 条） |
| `python -I -B tests/parity/m3/whitelist_anchor_gate.py check` | ALL PASS（503 条锚） |
| `python -I mir2-rs/tests/parity/m3/m3_whitelist_registry_audit.py audit` | GREEN（198 条、无重复） |
| `python -I tools/whitelist/t_reservation_gate.py check` | GREEN（198 行 / 198 号 / max=T-210） |
| `python -I mir2-rs/tests/parity/coverage/gm_registry.py gate` | GREEN（87 条与源码一致） |

## 6. 已知残余（登记 T-210）

- **只接了 `@Login` 一路**：商人 `run_merchant_goto` 与 QFunction `run_party_goto_label` 两处脚本执行点
  尚未装配 `NpcGm`（它们的调用点当前不持有 `GmWorld`/`&mut GmSession` 的同域借用），若部署脚本在这两路
  里写 `GMEXECUTE`，仍是零帧。`@Login` 是 8 条 script-reachable 命令的真机判据路径（T-209 的窗口场景）。
- 密码状态机的**状态迁移**仍只在 `CM_PASSWORD` 路径实现；跳线只读取「是否在密码流里」并拒绝执行，
  不代做状态迁移（oracle 的 `Chat.cs` 密码支在 `CM_SAY` 路径上是另一条写入面，本卡不改）。
- `SendDefMessage` 直发帧目前只有 `GmFrame::AttackMode` 一种；`@Login` 排水点已按「直发先于队列」实现。

## 7. 残留物清单

- 临时脚本（仓外，已无需保留）：`E:/tmp/wt62-login-wire.py`、`E:/tmp/wt62-lifetimes.py`、
  `E:/tmp/wt62-restructure.py`、`E:/tmp/wt62-fix2.py`、`E:/tmp/wt62-npctests.py`、
  `E:/tmp/wt62-actions-tests{,2,3}.py`、`E:/tmp/wt62-anchor-fix.py`、`E:/tmp/wt62-whitelist.py`、
  备份 `E:/tmp/wt62-actions*.bak`。
- 未起栈、未占端口、未连库；`src/**` 未改。
