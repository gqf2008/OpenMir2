# 脚本 `GMEXECUTE` 宿主跳线：商人 / 点击 / QFunction / 组队四路（T-211）

- 卡：`openmir2-gm-script-exec-seams`（tier=R；承 T-210 `openmir2-gm-script-host-seam` 的 `@Login`-only 范围）
- 分支：`card/openmir2-gm-script-exec-seams`；base = T-210 head `a4b87fee5`（前置卡未合并，**合并顺序：先 T-210**）
- 作者：`openmir2-worker-24`；worktree `OpenMir2-wt-cronclaim`

## 1. 目标

T-210 把 `NpcPlayer` 的 `permission`/`set_permission`/`process_user_line_msg` 覆写与
`PermissionGuard` 落到 `@Login` 一路，并实测证明部署里唯一落在 `@Login` 调用树的是「禁言」。
本卡把**同一套桥**装到另外三个脚本执行点，并补上 QFunction 的 `@StdModeFunc*` 生产入口：

| 路 | 生产入口 | 载体 |
| --- | --- | --- |
| 商人对话 | `CM_MERCHANTDLGSELECT` → `handle_shop_cmd` → `run_merchant_goto` | `MerchantScriptBook` |
| 点击 NPC | `CM_CLICKNPC` → `handle_click_npc_cmd` → `run_merchant_goto` | `MerchantScriptBook`（`@main`） |
| QFunction（物品使用） | `CM_EAT` → `handle_eat_cmd` → `run_std_mode_func_script` | `NpcHost`（`@StdModeFunc<N>`） |
| 组队 | `PartyEvent::GotoLabel` → `handle_party_cmd` → `run_party_goto_label` | `NpcHost`（QFunction） |
| 固定 NPC | `CM_CLICKNPC` → `guild_official::handle_select` → `FixedNpcScriptCtx::goto_label` | `MerchantScriptBook`（`Npc_Def/<ChrName>-<Map>`） |

## 2. oracle 链路（逐段）

- 抬权 / `finally` / `ProcessUserLineMsg` / 密码状态机 / 权限门：见 T-210（本卡不改语义）。
- 商人：`ScriptEngine.cs:38-129` 的 `GotoLable`（`Merchant.ScriptList`）；两条入口
  `CM_CLICKNPC(@main)` 与 `CM_MERCHANTDLGSELECT(@label)` 都进 `run_merchant_goto`。
- QFunction 生产入口：`src/M2Server/Player/PlayObject.Base.cs:1072` `UseStdModeFunItem`
  ```csharp
  if (SystemShare.FunctionNPC != null) { FunctionNPC.GotoLable(this, "@StdModeFunc" + stdItem.AniCount, false); result = true; }
  ```
  调用点 `src/M2Server/Player/PlayObject.Operate.cs:668`（`ClientUseItems` 的 `StdMode 31` 分支）：
  **只有** `AniCount != 0` 才进；`AniCount == 0` 是解包支（已由 `openmir2-unbindlist-consumer` 接）。
  顺序：脚本 → `Dispose` → `ItemList.RemoveAt` → `eatSuccess = true` → `SM_EAT_OK`。
- 组队：`FunctionNPC.GotoLable`（既有 `run_party_goto_label`，T-194 起发帧）。

## 3. 实现

- `lib.rs` 新增 `ScriptGmEnv { guilds, maps, cfg, pwd_sessions }` + `bridge()`：字段与 `@Login`
  装配逐字段同源（`GmWorld` 的 `speaker`/`speaker_id`/`now_ms = world.clock_ms`，
  `pwd_busy` 取会话密码状态机的五个 `Is*` 位）。
- 四路统一「**脚本前装桥、脚本后摘桥**」。会话 GM 态用 `mem::take` **移出/放回**：
  `NpcPlayer<'r, 'g>` 的 `'g` 在 `player` 存活期内都是活的（NLL 对 `Option<&mut …>` 字段
  不做流敏），若直接借 `&mut sg.gm`，后续 `write_back_*` 就再也借不到 `&mut SessionGround`。
  移出期间跳线是唯一持有者 ⇒ 与 oracle「同一个 `PlayObject` 对象」逐字段等价。
- `handle_eat_cmd`：`world` 改 `&mut mir2_world::World`（`@StdModeFunc*` 要抽 `world.rng`）；
  吃之前按 `make_index` 查模板，`StdMode 31 && AniCount != 0` 且 `FunctionNPC != null`
  ⇒ 先跑 `run_std_mode_func_script`，`use_item` 的 `function_npc_present` 由
  `NpcHost::has_function_npc()` 给出（部署恒 true）。
- `CM_CLICKNPC` 在 sim 循环**提前接管**：`dispatch_inbound_p0` 那一路要传
  `&mut guilds`（`GuildDisconnect`），与跳线要借的 `&guilds` 冲突；dispatch 内同 ident 分支
  保留为单元级入口（`gm = None`，注释写明）。

## 4. 部署证据（`gm_registry.json` 的 7 条 `script_evidence`）

| 命令 | 部署落点 | 走到本卡哪一路 | 覆盖 |
| --- | --- | --- | --- |
| 取消禁言 | `Market_Def/功能NPC/比武-GA2.txt:43/68/106`（`@aa ← @main`） | 商人对话（`@aa`）/ 点击（`@main`） | 用例 ① ② |
| 改属性 | `Envir/Npc_def/比奇国王-0122.txt:71-126`（`@qf ← @rw03 ← @rw02 ← @main`；`Npcs.txt` type 1 = GuildOfficial） | **固定 NPC 路**（本卡新接） | 用例 ⑤ |
| ShowEffect | `Market_Def/QFunction-0.txt:104-149/170-220` + 比奇国王 + 月老-M101 | QFunction `@StdModeFunc*` / 商人 / 固定 NPC | 用例 ④ ①②⑤ |
| MobFireBurn | `Market_Def/QFunction-0.txt:156/163`（`@StdModeFunc15/16`） | **QFunction 生产入口**（本卡新接） | 用例 ④ |
| 清理垃圾 | `Market_Def/假人上线NPC-jiaren.txt:19`（`@main`）+ `桃源-N109.txt:87` | 点击 / 商人对话 | 用例 ② ① |
| 开始提问 | `QuestDiary/假人登录脚本.txt:40`（`@Login` 调用树） | `@Login`（T-210 已接） | T-210 用例 |
| VV踢人 | `Market_Def/功能NPC/桃源-N109.txt:49` + `月卡系统.txt:28`（`@踢人 ← @月卡` 超链接点击 ← `QManage:36`） | 商人对话 | 用例 ① |

> 命令体本身在各自的 `openmir2-gm-impl-*` 卡里实现/登记；本卡只保证**路径**把
> `GMEXECUTE` 送进 `ProcessUserLineMsg`（此前这些点在 Rust 侧是零帧）。
> 文件路径中的中文目录与部署件一致（`Market_Def/功能NPC/…`）。

## 5. 用例与牙

用例（`bins/game-svr/src/lib.rs`）：

- `merchant_goto_gmexecute_emits_one_sysmsg_and_restores_permission`：`CM_MERCHANTDLGSELECT`
  的 `GMEXECUTE 禁言 self 10000` ⇒ 恰好一条 `SM_SYSMESSAGE`（正文逐字节
  `hero 已被禁言10000分钟`）+ `Permission 3 → 10 → 3` + **无泄漏探针**（同一 `GmSession`
  上客户端直呼 `@禁言 hero 10` 仍回 `权限不够`）。
- `click_npc_gmexecute_produces_sysmsg`：`CM_CLICKNPC(@main)` 同判据。
- `party_goto_label_gmexecute_emits_sysmsg_and_restores_session`：QFunction `@GroupCreate`，
  会话 `Permission 5` 原样放回。
- `eat_stdmode31_anicount_runs_qfunction_gmexecute_then_eats`：`CM_EAT` 吃
  `StdMode 31 / AniCount 15` ⇒ `@StdModeFunc15` 脚本出帧 + `SM_EAT_OK` + 物品离包。
- `guild_official_fixed_npc_gmexecute_produces_sysmsg`：固定 NPC（type 1）的
  `Npc_def` 漏斗里 `GMEXECUTE 禁言 self 20` 出帧。

牙（逐条实做并还原，`sha256` 与编辑前一致）：

| 牙 | 改动 | 结果 |
| --- | --- | --- |
| A | 删 `run_merchant_goto` 的装桥块 | merchant + click 两用例红 |
| B | 删 `run_party_goto_label` 的装桥块 | party 用例红 |
| C | 删 `handle_eat_cmd` 的 `run_std_mode_func_script` 调用 | stdmode 用例红 |
| D | `function_npc_present` 固定 `false`（oracle `FunctionNPC == null`） | stdmode 用例红（不吃、无 SysMsg） |
| E | `FixedNpcScriptCtx::goto_label` 的 `ScriptGmParts::with` 换成 `None` | 固定 NPC 用例红 |

## 6. 门禁

```
cargo fmt --all -- --check                     OK
cargo clippy --workspace --all-targets -- -D warnings   OK
cargo test --workspace                         （见卡面结果）
cargo run -p msgcodegen -- --check             （见卡面结果）
python -I -B tests/parity/m3/whitelist_anchor_gate.py check     506 条锚 ALL PASS
python -I -B tests/parity/m3/m3_whitelist_registry_audit.py audit   GREEN
python -I -B tools/whitelist/t_reservation_gate.py              （见卡面结果）
```

## 7. 残余（已登记，不静默）

1. 商人 / 组队两路的 `gm_direct_frames`（`SendDefMessage`，目前只有 `@AttackMode` 产生）
   未排水；QFunction 路已在 `run_std_mode_func_script` 内排水。
2. QFunction 路的 `SETMPAMODE` 只记快照：`pending_map_modes` 是 `ShopSession` 的队列，
   该路没有会话载体。
3. **窗口项未跑**（R）：见 T-211 行与 §8。

> **固定 NPC 路（`Npc_Def/`，如「改属性」的 `比奇国王-0122`）已接**：`handle_select` 同时持
> `&mut world`/`&mut guilds`，故 `FixedNpcScript::goto_label` 增加两个只读形参由 handler
> 在调用点重借，桥零件（`ScriptGmParts`）随 ctx 构造。

## 8. 窗口项（R，可复跑）

```
CONFORMANCE_PASSWORD='<口令>' python -I -B tools/conformance/card_window.py \
  --card openmir2-gm-script-exec-seams \
  --scenario tools/probe/scenarios/login_only.json \
  --out E:/tmp/conformance/runs/gm-script-exec-seams --profile alt --post-comment
```

判据：① 脚本触发的 `GMEXECUTE` 与 `@Login` 同款（`SM_SYSMESSAGE` 逐字节 + 次序）；
② `CM_EAT` 吃 `StdMode31/AniCount!=0` 的包：两侧都先跑 `@StdModeFunc<N>`、再 `SM_EAT_OK`，
物品离包；③ 无 `QFunction` 装载的对照支 ⇒ 不吃（`SM_EAT_FAIL`）。窗口未跑前本卡不得 `closed`。

## 9. 残留物清单

- `E:/tmp/w24_seams_edit1.py`、`E:/tmp/w24_fix_calls.py`（一次性改树脚本，保留供复核）
- `E:/tmp/w24-seams.sha`（牙的 sha256 基线）
