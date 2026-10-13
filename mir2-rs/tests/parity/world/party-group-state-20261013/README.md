# M3⑦-实现 2（组队会话状态机 + 4 个组队 `CM_*` + 全部组队帧）证据 —— 卡 `openmir2-m3-party-group-state`

- 分支：`party-group-host`（base = 交付时 `refs/heads/master` = `d4d5b89c73e77ed995188c2668dc3eda9a926dc0`）。
- 写集：`mir2-rs/crates/world/**` + `mir2-rs/bins/game-svr/**`（+ 本证据目录）。
- 边界：`git diff --name-only <base> HEAD -- src sql MirClient` = **空**（见 `20-boundary.txt`）。
- 窗口纪律：本片**未起栈、未占端口**；判据全在构造级/进程级（`cargo test`）。

## 0. 结论（先看这条）

按 oracle 把**组队会话状态机**落到 `crates/world`（权威状态在实体上）+ 4 条 `CM_*` 派发 + 9 条
`SM_*` 帧编码：

1. **状态**（`Entity`）：`allow_group` / `allow_group_re_call` / `group_owner` / `group_members`
   （含团长自己，加入序），构造初值照 `PlayObject.cs:902/224/583/1042`。
2. **状态机**（`crates/world/src/party.rs`）：`party_group_mode` / `party_create_group` /
   `party_add_group_member`（**严格大于** `Max`）/ `party_del_group_member` / `party_del_member` /
   `party_group_close` / `party_is_group_member` / `party_run_tick`。
   `DelMember`/`CancelGroup`/`SendGroupMembers`/`SendGroupText`/`JoinGroup`/`LeaveGroup` 逐行照搬。
3. **帧面**（`bins/game-svr/src/{smframe,party}.rs`）：`SM_GROUPMODECHANGED`/`SM_CREATEGROUP_OK/FAIL`/
   `SM_GROUPADDMEM_OK/FAIL`/`SM_GROUPDELMEM_OK/FAIL`/`SM_GROUPCANCEL`/`SM_GROUPMEMBERS` +
   组队说词/`SysMsg`（都出 `SM_SYSMESSAGE(100)`）。
4. **出队序**：`Emitter` 的「立即帧（源码序）+ 排队帧（`(byte)wIdent` 升序）」精确复刻
   `BaseObject.Run()` 的 `while(GetMessage(...)) Operate(...)`（`RM_GROUPCANCEL=156` <
   `RM_SYSMESSAGE=252` < `RM_GROUPMESSAGE=253`）。这是本片最容易做错的一处，已用整条事件向量钉死。
5. **接线**：`main.rs` 的 4 条 `CM_*` 派发 + `world.party_run_tick()`（每 tick 清死亡成员/团长死亡散组）+
   `GateEvent::Closed` 的 `Disappear → DelMember`（离线退组/团长离线整组散）。

读数：`party_vectors` **12/12**；`game-svr` 组队用例 **2/2**（编码层 + 进程级派发）；四门禁全绿
（**95 suites · 910 passed · 0 failed · 604**）；改坏必红 **3 条**（见 §4）。

## 1. oracle 锚点

见 [`01-oracle-anchors.txt`](01-oracle-anchors.txt)（逐函数行号 + 帧面/出队序口径）。

## 2. 改前 / 改后

见 [`02-change-summary.txt`](02-change-summary.txt)。

## 3. 判据（构造级 + 进程级，全离线）

见 [`03-tests.txt`](03-tests.txt)：
- `crates/world/tests/party_vectors.rs`（12 条）：建组事件序 + 状态、失败码 `-1..-5`、**严格大于**、
  `DelMember` 的立即/排队分桶与出队序、团长自己退组、关组、`Run` 清死亡成员、`IsGroupMember`。
- `bins/game-svr/src/party.rs`（1 条）：每条事件 → `SM_*` 帧形状（ident/param/series/尾）。
- `bins/game-svr/src/main.rs`（1 条，进程级）：走**生产路径** `handle_party_cmd`，两条连接完成
  `CM_GROUPMODE → CM_CREATEGROUP → CM_DELGROUPMEMBER`，断言路由 + `EncodeString` 尾 + 颜色。

## 4. 改坏必红（3 条，逐条 mutation → FAILED rc=101 → 还原）

见 [`redchecks/`](redchecks/)（`README.md` 索引 + 3 份原始件）：
- **RC-1**：`party_add_group_member` 的 `>` 改 `>=` ⇒ `add_member_uses_strict_greater_than_max` 必红。
- **RC-2**：`Emitter::finish` 去掉队列优先级排序 ⇒ `del_member_regular_...` 必红（事件序错位）。
- **RC-3**：`sm_groupmembers` 尾去掉 `edcode::encode` ⇒ 进程级派发用例必红（尾非 `EncodeString`）。

## 5. 四门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/m2svr-target`，`--offline`）

见 [`gates/`](gates/)：
- `10-fmt.txt` rc=0
- `11-clippy.txt` rc=0（`--workspace --all-targets -- -D warnings`）
- `12-msgcodegen.txt` rc=0（604 条）
- `13-workspace-test.txt` rc=0（**95 suites · 910 passed · 0 failed**）

## 6. 未建模（登记，随 ⑦ 对齐窗口 / 后续片收口）

- `FunctionNPC.GotoLable(this, "@GroupClose"/"@GroupCreate"/"@GroupAddMember"/"@GroupDelMember")`
  （`PlayObject.Operate.cs:806/843/881/907`）：脚本触发面，本片不发事件（不产生帧）。
- `[String] GroupMsgPreFix`/`JoinGroupMsg`（`StringConf.cs:39/223`）与 `[Setup] GroupMembersMax`
  已读 `[Setup]`（`GroupMembersMax`/`GroupMsgFColor`/`GroupMsgBColor`），但 `[String]` 两键本仓尚无读取器
  ⇒ 用源码缺省（`〖组队〗` / `{0} 已加入小组.`）；部署 `Server.conf` 未覆盖它们 ⇒ 与 oracle 一致。
- `AllowGroupReCall`（`PlayObject.cs:224`）字段已就位，本片无消费点（`@GroupRecall` 扩展面）。
- `ClientGroupClose` 的团长提示是 oracle `SysMsg`（经 `RM_SYSMESSAGE` 排队、优先级 252）；
  本片按出队序发 `SM_SYSMESSAGE`，与「同 tick 内立即帧之后」一致。
- 组队经验分配 `GainExp`（`PlayObject.Base.cs:764-818`）与 `HAM_GROUP` 攻击过滤
  （`PlayObject.cs:4249-4256`）归 **⑦-4 `openmir2-m3-party-attribution`**（拾取/归属/分配）。
- **tick 内相对次序**（登记）：oracle 的组队清理段在 `PlayObject.Run()` 的**消息处理之前**
  （`PlayObject.Message.cs:704-719`，`base.Run()` 在 `:765`）；Rust 宿主在同一 tick 的 `world.tick()` 之后
  调用 `party_run_tick()`（消息派发更早一步）。仅影响「同一 tick 内既死亡又发组队 CM」的罕见交错。
