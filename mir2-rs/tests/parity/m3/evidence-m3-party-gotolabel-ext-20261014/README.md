# evidence-m3-party-gotolabel-ext-20261014

卡：`openmir2-m3-party-gotolabel-ext`（M3⑦ 组队/交易残余）。作者 `openmir2-worker-21`，车道 `m3pt`。
类型：**L 级**（纯逻辑分支 + 宿主接线；不触 RNG 抽数序列/时序竞态/持久化/战斗数值）。
base = `origin/master` = `67898a834e67f40a01e6129f5089e2a5f95c3169`。

## 交付内容

oracle `PlayObject.Operate.cs:806/843/881/907`：`ClientGroupClose/Create/AddGroupMember/DelGroupMember`
在成功（或 `GroupOwner != 0`）路径尾调 `SystemShare.FunctionNPC.GotoLable(this, "@GroupClose"/
"@GroupCreate"/"@GroupAddMember"/"@GroupDelMember", false)`。此前 `crates/world/src/party.rs` 模块头
把该面登记为「未建模，本片不发事件」。

| 层 | 文件 | 改动 |
| --- | --- | --- |
| 世界 | `crates/world/src/party.rs` | 新增 `PartyEvent::GotoLabel{target,label}`（**不是帧**）+ 四常量 `PARTY_LABEL_GROUP_*`；`PRIO_RM_MERCHANTSAY = 11009 & 0xFF = 1`；四处写入点按 oracle 顺序 `em.later(1, GotoLabel)`；`party_group_close_em` 的 `GroupOwner==0` 早退**不发射** |
| 接缝 | `bins/game-svr/src/party.rs` | `event_body` 返回 `Option`（`GotoLabel => None`） |
| 宿主 | `bins/game-svr/src/main.rs` | `handle_party_cmd` 增 `saves/ground_sessions/npc_host/catalog` 形参；drain 遇 `GotoLabel` 在**该位置**调 `run_party_goto_label`：`FunctionNPC`（`QFunction`）装载才执行，`NpcPlayer` 取数 / `world.rng` 临时移出归位 / 回写 `Gold`+`QuestFlag`；说词经 `SM_MERCHANTSAY(643)` 下发 |
| 登记 | `tests/parity/whitelist.md` | **T-54**：脚本非说词面（脚本自身立即帧 + `QuestUnit`/`ItemList`/`LastNpc`/`Script`/`ScriptGotoCount` 写回）未建模；现网语料不可核 |

**顺序依据（本卡关键）**：`RM_MERCHANTSAY(11009)` 的优先级字节 = `1`，是三个组队排队帧
（`RM_GROUPCANCEL` 156 / `RM_SYSMESSAGE` 252 / `RM_GROUPMESSAGE` 253）里**最小**的 ⇒ oracle 同一
`Run()` 出队序里脚本说词排在组队说词之前。世界侧把 `GotoLabel` 入**排队桶**（`em.later(1, ..)`），
`Emitter::finish = immediate ++ queued(优先级稳定排序)` ⇒ 线上序 = 立即帧 → 脚本说词 → 组队排队帧。

**取数口径**：组队发生在**进世界之后** ⇒ `PlayObject.Gold/ItemList/WAbil` 的运行时真身在
`ground_sessions`（`saves.record` 只在读档/存档时刷新）：`run_party_goto_label` 的 `gold/level/bag`
**优先取 `ground_sessions`**，无会话回退读档记录；`QuestFlag` 运行时唯一存储仍在 `saves.record`
（与进世界块 `@Login` 同族口径）。判据用「运行时 Gold 12345 ≠ 读档 Gold 0」钉住该选择。

## 判据

1. **世界向量**（`crates/world/tests/party_vectors.rs`）：`create_group_matches_oracle_call_order_and_dequeue_order`、
   `del_member_regular_keeps_leader_with_cancel_and_delmem_ok`、`group_close_non_leader_...`、
   `group_close_leader_emits_red_sysmsg_only`、`die_member_triggers_oracle_del_member_and_group_survives`
   均把 `GotoLabel` 钉在「立即帧之后、排队帧之前」的确切位置。
2. **进程级**（`bins/game-svr/src/main.rs::tests::party_gotolabel_runs_qfunction_script_at_queued_position`）：
   走生产路径 `handle_party_cmd` + 装载 `QFunction` 说词，断言帧序
   `[660, 667, 667, **643**, 100 ×4]` 且 643 尾 = `EncodeString("QFunction/grp")`；另断言运行时 Gold 取数/回写。
3. **改坏必红 5/5**：见 `07-mutations-red.txt`（删 `@GroupCreate` 发射 / 宿主强制早退 / 优先级 1→253 /
   删 `@GroupClose` 发射 / Gold 误取陈旧读档记录），每条都被上述至少一条判据判红；还原后 `git status` 干净。

## 门禁（head `293a34a8e`，cwd=`mir2-rs`，`CARGO_TARGET_DIR=E:/tmp/openmir2-m3pt-target`）

- `cargo fmt --all -- --check` → rc=0（`03-gate-fmt.txt`）
- `cargo clippy --workspace --all-targets -- -D warnings` → rc=0（`04-gate-clippy.txt`）
- `cargo test --workspace` → **101 suites / 1214 passed / 0 failed / 22 ignored**，rc=0（`05-gate-test.txt`）
- `cargo run -p msgcodegen -- --check` → 604 条一致，rc=0（`06-gate-msgcodegen.txt`）
- 边界 `git diff --name-only <base> HEAD -- src sql MirClient` = 空（`08-boundary.txt`）

## 边界与残留

- 未起真机栈、未占任何端口；未 merge、未自签 review；共享主检出未动。
- `not_verified`：真机两侧 `run_party_trade_pair.ps1` 同序列 diff=0（归 `openmir2-m3-party-trade-align`
  + owner 独占窗口）；本卡无真机可跑判据（离线 L）。
- `T-54`：脚本非说词面未建模（本机无游戏语料，不可核）。`@GroupRecall` GameCommand 不在本卡（须另立卡）。
