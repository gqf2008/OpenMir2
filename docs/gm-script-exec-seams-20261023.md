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
| 开始提问 | `Market_Def/LEGM2.CC假人NPC/假人上线NPC-jiaren.txt:@main` → `QuestDiary/LEGM2.CC假人系统/假人登录脚本.txt:@假人到指定地图a`（`@main → @假人设置页面 → @下一页 → @假人到指定地图 → @假人到指定地图a`） | 商人/点击路（本卡商人/点击桥覆盖；**不是** `@Login`） | 用例 ② + R1 多步链窗口 |
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
4. **脚本路的 `pending_entity` 未应用**：管理态命令（`@1/@2/@3/@Rest/@AttackMode`）把权威值装进
   `GmSession.pending_entity`，由 `apply_pending_gm_entity` 落到实体；该函数目前只在聊天路径
   （`handle_say`）被调用。本卡新接的商人/点击/QFunction/组队/固定 NPC 路脚本若执行这类命令，
   pending 会滞留到下一次聊天命令才被应用（相位错）。本卡 8 条 script-reachable 命令里没有管理态命令，
   部署脚本也未在脚本路调用它们 ⇒ **当前不可达**，属未来 GMEXECUTE 命令的登记项（reviewer-6 M-3）。

> **固定 NPC 路（`Npc_Def/`，如「改属性」的 `比奇国王-0122`）已接**：`handle_select` 同时持
> `&mut world`/`&mut guilds`，故 `FixedNpcScript::goto_label` 增加两个只读形参由 handler
> 在调用点重借，桥零件（`ScriptGmParts`）随 ctx 构造。

## 8. 窗口项（R，按路给场景）

> **`login_only` 对本卡无效**：它只有 `wait_idents`（等 50/52/658/654/657/100），
> 不发任何客户端动作 ⇒ 不能发 `CM_EAT`、不能点 NPC/选商人标签、不能触发组队 `GotoLabel`，
> 对本卡新接的五路**一条证据都产不出**（reviewer-6 I-2）。窗口命令必须按下面每一路选场景。
>
> `card_window.py` 不直接收 `--var`；用自定义 profile 的 `probe_extra` 传：
> `"probe_extra": ["--var", "npc_actor=<id>"]`（见 `tools/conformance/README.md` §profile）。
> `--var` 的值必须**两侧各自发现/各自取**，推不出就 fail-closed，不许拿一侧的值跑另一侧。

| 路 | 场景 / 命令 | 前置 | 判据 | 现状 |
| --- | --- | --- | --- | --- |
| **① 点击路**（清理垃圾） | `npc_click.json` + `probe_extra ["--var","npc_actor=<假人上线NPC actor>"]` | 角色名必须在 `QuestDiary/LEGM2.CC假人系统/假人名单列表.txt`（否则 `@main` 走 `#ELSEACT` MESSAGEBOX）；`假人上线NPC-jiaren`（MerChant.txt:1）在视野内 | 点击后两侧**恰好一条** `SM_SYSMESSAGE(100)`（GMEXECUTE 回执，正文/Recog/Param/尾逐字节），随后 `SM_MERCHANTSAY(643)`（`@继续` 菜单）次序一致；actor 由 643 规则发现，推不出则两侧各自手填 | **可跑**（第 1 批） |
| **② QFunction 路**（MobFireBurn `@StdModeFunc15/16`、ShowEffect `@StdModeFunc14`） | `eat_item.json` + `probe_extra ["--var","make_index=<MakeIndex>","--var","item_name=<包名>"]` | 角色背包有 `StdMode 31` 且 `AniCount ∈ {14,15,16}` 的包；`MakeIndex/item_name` 从 `SM_BAGITEMS(201)` 手工取（**发现规则无卡**） | 两侧都先跑脚本：`SM_SYSMESSAGE(100)` 正文逐字节（15=「爱情火焰放置成功」/16=「神圣之光放置成功」；14 另有 `SENDMSG 0` 广播与 2000ms `DelayCall` 链）+ 效果帧；**之后**才 `SM_EAT_OK(635)`，物品离包（202/201 后态）。C2S 的 MakeIndex 跨栈不同 ⇒ 只判 S2C | **可跑但需手工 --var**；无物品时需构造背包夹具（记缺口） |
| **③ 商人多步链**（取消禁言 `@aa←@main`；清理垃圾 `@06←@main`；VV踢人 `@01←@main`；开始提问 `@main→@假人设置页面→@下一页→@假人到指定地图→@假人到指定地图a`） | **需新场景 R1 `npc_script_chain`**（多步 `dlg_select`；现有 `dice_chain` 固定 `@kaiz/@ya5/@ya52`、`merchant_dialog` 只发 `@buy`） | `比武-GA2`（MerChant.txt:137）、`桃源-N109`（:138）、`假人上线NPC`（:1）；开始提问要求 `ISADMIN` + 在假人名单 | 每一步 `643` 后最终**恰好一条** `SM_SYSMESSAGE(100)`，正文/次序与 C# 逐字节 | **需新场景**（第 3 批） |
| **④ 固定 NPC 路**（改属性 `@qf←@rw03←@rw02←@main`） | **需 R1 + 固定 NPC 多步**（`npc_click` 只发一次 `@main`） | `Npcs.txt` type 1 的 `比奇国王-0122` 在视野内 | 同上（100 回执 + 次序） | **需新场景**（第 3 批） |
| **⑤ 组队路**（`PartyEvent::GotoLabel`） | **需新场景/夹具**：发 `CM_GROUPMODE`/`CM_CREATEGROUP` 触发 `GotoLabel`；`run_party_trade_pair.ps1` 是 bot 夹具，需按本判据改造 | 两个会话（组队至少 2 人） | `SM_SYSMESSAGE(100)` 回执 + 会话 `Permission` 原样放回（5→5） | **需新场景 + 多会话**（第 3 批） |
| **⑥ 无 QFunction 对照**（判据③） | 不跑窗口 | 冻结部署 `FunctionNPC` 恒装载 ⇒ 真机构造不出「无 QFunction」 | **替代判据 = 构造级牙 D**：`function_npc_present=false` ⇒ 只回 `SM_EAT_FAIL(636)`、0 条 SysMsg、物品留在包内 | **构造级**（不占窗口） |
| **⑦ `@Login` 回归**（T-210） | `login_only.json` | 新角色登录 | T-210 既有判据 | 只验 @Login，**不算本卡五路证据** |

> 窗口未跑前本卡不得 `closed`；验完后在 `status` 写明**哪几路已验、哪几路没验及原因**。
> 缺口登记：① `SM_BAGITEMS(201)` 发现规则需新开卡（`bag_equip`/`eat_item` 共用）；
> ② R1 `npc_script_chain` 新场景（覆盖 ③④）；③ 组队路新场景/夹具（⑤）。

## 9. 残留物清单

- `E:/tmp/w24_seams_edit1.py`、`E:/tmp/w24_fix_calls.py`（一次性改树脚本，保留供复核）
- `E:/tmp/w24-seams.sha`（牙的 sha256 基线）
