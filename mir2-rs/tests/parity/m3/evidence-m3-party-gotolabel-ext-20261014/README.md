# evidence-m3-party-gotolabel-ext-20261014

卡：`openmir2-m3-party-gotolabel-ext`（M3⑦ 组队/交易残余）。作者 `openmir2-worker-21`，车道 `m3pt`。
类型：**L 级**（纯逻辑分支 + 宿主接线；不触 RNG 抽数序列/时序竞态/持久化/战斗数值）。
base = `origin/master` = `f0a9ab911ddc84846b69f0a3541390e81dba977a`（首交 base 为 `67898a834`；返工时合并最新 master 对齐）。
返工说明见文末「返工：T-54→T-55 撞号订正」。

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
| 登记 | `tests/parity/whitelist.md` | **T-55**（原 T-54，与 ⑧-5 撞号 ⇒ 返工改号）：脚本非说词面（脚本自身立即帧 + `QuestUnit`/`ItemList`/`LastNpc`/`Script`/`ScriptGotoCount` 写回）未建模；现网语料不可核 |

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
3. **改坏必红 6/6**：见 `07-mutations-red.txt`（删 `@GroupCreate` 发射 / 宿主强制早退 / 优先级 1→253 /
   删 `@GroupClose` 发射 / Gold 误取陈旧读档记录 / **MUT6 把登记号 T-55 还原成 T-54 ⇒ ⑧-5 硬门判 `DUPLICATE T-54` RED**），
   每条都被至少一条判据判红；还原后 `git status` 干净。

## 门禁（返工 head，cwd=`mir2-rs`，`CARGO_TARGET_DIR=E:/tmp/openmir2-m3pt-target`）

- `cargo fmt --all -- --check` → rc=0（`03-gate-fmt.txt`）
- `cargo clippy --workspace --all-targets -- -D warnings` → rc=0（`04-gate-clippy.txt`）
- `cargo test --workspace` → **101 suites / 1214 passed / 0 failed / 22 ignored**，rc=0（`05-gate-test.txt`）
- `cargo run -p msgcodegen -- --check` → 604 条一致，rc=0（`06-gate-msgcodegen.txt`）
- 边界 `git diff --name-only <base> HEAD -- src sql MirClient` = 空（`08-boundary.txt`）

## 边界与残留

- 未起真机栈、未占任何端口；未 merge、未自签 review；共享主检出未动。
- `not_verified`：真机两侧 `run_party_trade_pair.ps1` 同序列 diff=0（归 `openmir2-m3-party-trade-align`
  + owner 独占窗口）；本卡无真机可跑判据（离线 L）。
- `T-55`（原 T-54）：脚本非说词面未建模（本机无游戏语料，不可核）。`@GroupRecall` GameCommand 不在本卡（须另立卡）。
- 返工附带**仓库卫生**：删除上一提交误带入的 `mir2-rs/SHA256SUMS.txt`（内容 = `mir2-rs/README.md` 的 sha256，单行，非本卡
  声明写集，疑证据哈希工具在 `mir2-rs/` 下误跑产物）。删除后分支写集回到「声明路径」集合。

## 返工：T-54→T-55 撞号订正（2026-10-14）

**触发**：master 推进到 `f0a9ab911`（⑧-5 `openmir2-m3-guild-war` reviewer-6 O-1 收口片合并）。该片在 `mir2-rs/tests/parity/whitelist.md`
也登记了 **T-54**（「`ReQuestGuildWar` 先扣钱后判掌门」），其 `m3_guild_war_quirk_registry_gate.py` 已接入 `fixture_chain_gate.py` 共享管线，
`check` 对 T-54 **编号唯一性** fail-closed。

**改前（RED，合并树）**：`python -I m3_guild_war_quirk_registry_gate.py check` ⇒
`GUILD_WAR_QUIRK_REGISTRY_RED 1：DUPLICATE T-54 x 2 @ L289,L331`，rc=1 ⇒ 本卡直合会打断链门。

**处置**：按「后注册者让号」先例（同卡 `T-51`→`T-52`）把**本卡**登记行改为 **T-55**（T-54 唯一归属 ⑧-5），并同步本 README + 编号连号说明。
零行为改动、零 `.rs`。

**改后（GREEN）**：同一 `check` ⇒ `GUILD_WAR_QUIRK_REGISTRY GREEN（1 行登记齐 + 锚点齐 + 编号唯一）`，rc=0。
改前/改后读数、公共审计前后对比、以及「改坏必红（把 T-55 还原成 T-54 ⇒ 必 `DUPLICATE T-54` RED）」见
[`09-whitelist-t55-renumber.txt`](09-whitelist-t55-renumber.txt)；`07-mutations-red.txt` 的 MUT6 为该红检。
