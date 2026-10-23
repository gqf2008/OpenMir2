# 脚本 `GMEXECUTE` 的宿主跳线（卡 `openmir2-gm-script-host-seam`，T-210）

> tier=R。T-209（`@禁言`）与 7 张 `openmir2-gm-impl-*` 卡（`-release`/`-kick`/`-showeffect`/`-mobfireburn`/
> `-smake`/`-clearbag`/`-startquest`）的**机制前置**：跳线此前不存在，任何脚本 `GMEXECUTE` 在 Rust 侧都是**零帧**。
>
> **范围订正（reviewer-6 B-1，2026-10-23）**：本卡只把生产跳线接到 `@Login`（QManage 段）——
> 部署里**只有 `禁言`** 落在 `@Login` 调用树；其余 7 条命令的落点在商人 `@main` / QFunction `@stdmodefunc` /
> 超链接，那两处执行点本卡未装桥（且 QFunction 的 stdmode 入口本身还没有生产调用点）⇒ **那 7 条仍零帧**。
> 承接卡：`openmir2-gm-script-exec-seams`。

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

## 6. 已知残余（登记 T-210；reviewer-6 B-1 订正后）

- **范围 = @Login-only（本卡不做扩写集）**：生产接线点只有 `@Login`（`lib.rs` 的 QManage 段）。
  reviewer-6 逐条解码 `gm_registry.json` 的 `script_evidence` + 部署件核过：
  - **只有 `禁言`** 在 `@Login` 调用树（`登陆脚本.txt:16` ← `QManage.txt:37` `#CALL @登陆设置`）⇒ 本卡打通；
  - 其余 7 条全在商人/对话/超链接：取消禁言（比武-GA2 `@aa`←`@main`）、改属性（比奇国王 `@qf`）、
    ShowEffect（QFunction-0 + 商人）、MobFireBurn（QFunction-0）、清理垃圾（`@main`）、开始提问（假人登录脚本）、
    VV踢人（桃源-N109 + 月卡系统）；
  - `run_merchant_goto` / `run_party_goto_label` 构造的 `NpcPlayer` 仍是 `gm: None` ⇒ 仍零帧；
  - 更甚：QFunction 的 stdmode 入口 `run_std_mode_func` 在 `bins` 里**没有生产调用点**（只有定义 + 单测）⇒
    MobFireBurn/ShowEffect 连执行入口都没有，不是"装个桥"就能通。
  - 因此 T-209 追加的"7 张子卡共同前置已通"**不成立**；7 张卡的通知已发更正 comment。
  - **follow-up 卡**：`openmir2-gm-script-exec-seams`（商人 `run_merchant_goto` 装配 `NpcGm` + QFunction
    stdmode 生产入口 + `run_party_goto_label`），阻断上述 7 张子卡。
- **无泄漏判据已订正（reviewer-6 I-2）**：原窗口项"非新角色登录 ⇒ 零条禁言帧"是空断言（非新角色根本不走
  `ISNEWHUMAN` 分支）。改为**客户端直呼** perm-10 命令 `@禁言 <名> 1`：两侧都必须回 `权限不够!!!`；若 10 泄漏到
  会话态，Rust 会真执行并回「已被禁言」文本。代码侧镜像 = `gmexecute_seam_does_not_leak_permission_to_client_commands`。
- **直发帧次序（reviewer-6 M-2，登记不修）**：`@Login` 内 `emit_npc_says` 先于 `gm_direct`，与 oracle 的即时
  `SendDefMessage`（先于队列排水）相反；现网不可达（唯一产生直发帧的 `@AttackMode` 不在 script-reachable 集内）。
- 密码状态机的**状态迁移**仍只在 `CM_PASSWORD` 路径实现；跳线只读取「是否在密码流里」并拒绝执行，
  不代做状态迁移（oracle 的 `Chat.cs` 密码支在 `CM_SAY` 路径上是另一条写入面，本卡不改）。

## 7. 残留物清单

- 临时脚本（仓外，已无需保留）：`E:/tmp/wt62-login-wire.py`、`E:/tmp/wt62-lifetimes.py`、
  `E:/tmp/wt62-restructure.py`、`E:/tmp/wt62-fix2.py`、`E:/tmp/wt62-npctests.py`、
  `E:/tmp/wt62-actions-tests{,2,3}.py`、`E:/tmp/wt62-anchor-fix.py`、`E:/tmp/wt62-whitelist.py`、
  备份 `E:/tmp/wt62-actions*.bak`。
- 未起栈、未占端口、未连库；`src/**` 未改。

## 8. 复审返工（reviewer-6 `36eb6263`，request_changes）

| 编号 | 处置 |
|---|---|
| B-1（blocking） | 选 **(b) 接受 @Login-only**：改正本报告 §1/§6、T-210 行与 status 的事实描述；给 7 张子卡各发**更正 comment**（明示仅 `禁言` 可验、其余等 `openmir2-gm-script-exec-seams`）；新开 follow-up 卡 `openmir2-gm-script-exec-seams`（商人桥 + QFunction stdmode 入口 + QFunction/组队路）。 |
| I-2（important） | 窗口泄漏探针改为**客户端直呼 perm-10 命令**（两侧应回 `权限不够!!!`）；新增代码侧镜像用例 `gmexecute_seam_does_not_leak_permission_to_client_commands`。 |
| M-1（minor） | T-209 行中段旧句（"跳线未接 / 引用已删的 `execute_script_line`"）改为"已由 T-210 解决，见行末更新"；窗口句改为"跳线已由 T-210 接通（@Login 路径）"。 |
| M-2（minor） | 已在 T-210 行登记（直发帧 vs 说词次序，现网不可达），本卡不改代码。 |
| M-3（minor） | 删除密码拦截用例里的死写 `player.permission = 10`（有桥时读写走 `GmSession.permission`）。 |
| 合并 | master 已到 `7b0e5955`（frame-drain 合并），`whitelist.md` 冲突按「保留本卡 T-209 更新 + T-210，master 新节保留 T-208、丢弃其重复 T-209」解析；全套门禁在**合并树**上重跑。 |
