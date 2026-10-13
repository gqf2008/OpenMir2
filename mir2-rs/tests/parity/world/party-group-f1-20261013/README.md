# M3⑦-实现 2 返工（F-1 死亡退组接线 + M-1 只扫玩家）证据 —— 卡 `openmir2-m3-party-group-state`

- 分支：`party-group-host`。
- 旧被审 head（reviewer-5 `fc68afca` 判 `request_changes`）= `28c9a8d5c7d541f8b59fed943dcf0546a977c68c`。
- **本返工 patch head = 本证据提交**（其父 commit = `0affa9441a34e61375dec02e9b8e8f9c44f4fb95`：返工 commit `4b6c843be` + 两次 `git merge origin/master` 对齐；本目录为其证据子提交）。
  （`git log --oneline -1 --format='%H' -- mir2-rs/tests/parity/world/party-group-f1-20261013` 即本 head。）
- base（交付时 `refs/heads/master`）= `65602bf862b873402fe2281b2b687506f95d6cb8`（交付 head 已 merge 它）。
- 写集：`mir2-rs/crates/world/**` + `mir2-rs/bins/game-svr/**`（+ 本证据目录）。边界见 [`20-boundary.txt`](20-boundary.txt)：
  `git diff --name-only origin/master HEAD -- src sql MirClient mir2-rs/crates/protocol mir2-rs/crates/storage` = **空**。
- 窗口纪律：**未起栈、未占端口**；判据全在构造级/进程级（`cargo test`）。

## 0. 结论（先看这条）

按 reviewer-5 的 F-1 **(a) 接线** 与 M-1 收口：

1. **F-1（Important，唯一阻塞）**：oracle `PlayObject.Die()`（`src/M2Server/Player/PlayObject.cs:1964-1978`，关键行
   `:1975-1977 groupOwnerPlay.DelMember(this)`）的「人物死亡立即退组」**已接线**：
   - `crates/world/src/world.rs::settle_deaths`（= 本仓 `hp == 0` 的死亡写入点，oracle `Die()` 的等价处）
     在置 `death = true` 后判 `group_owner != 0` ⇒ 调 `party_del_member(group_owner, id, &party_cfg)`，
     事件落新字段 `TickRecord::party_events`；
   - 宿主 `bins/game-svr/src/main.rs` 新增 `drain_party_death_events`，在 `world.tick()` 之后、
     `party_run_tick()` 之前回帧；**死亡者仍在线 ⇒ 它自己的 `SM_GROUPCANCEL(666)` / 退出说词照发**
     （不复用断线路径「已摘 online ⇒ 丢自身帧」口径）。
   - 后果修复：死亡成员 `group_owner` 立即清 0（可重新建组/入组）；团长死亡 ⇒ 整组散；
     2 人组成员死亡 ⇒ 双方各收 `SM_GROUPCANCEL` + 团长侧解散说词。
2. **M-1（Minor）**：`party_play_object_by_name` 按 `Race == ACTOR_RACE_PLAY` 过滤，贴 oracle
   `WorldEngine.GetPlayObject` 只扫 `PlayObjectList`（`GameSrv/Word/WorldServer.cs:937-955`）：
   用怪物/NPC 名当目标时 oracle 判 `null`（`-2`），不再误命中非玩家实体。
3. **未登记 → 已登记**：reviewer 指出作者锚点 `01-oracle-anchors.txt` §E 把 `OnDie…DelMember` 列为锚点
   造成「已覆盖」假象 ⇒ 本片改为真接线，并新增 3 条构造级用例 + 1 条进程级用例钉死。

读数：`party_vectors` **15/15**（原 12 + 新 3）；`game-svr` **组队 2/2 + 死亡退组进程级 1/1**；
四门禁全绿（**95 suites · 927 passed · 0 failed · 21 ignored · 604**）；
改坏必红 **2 条**（RC-1 F-1 / RC-2 M-1，逐条 FAILED rc=101 → 还原 ok）。

## 1. oracle 锚点

见 [`01-oracle-anchors.txt`](01-oracle-anchors.txt)（F-1 差量：`Die()` 行号 + `DelMember`/`LeaveGroup`/`CancelGroup`
语义 + tick 内相对次序；M-1 差量：`GetPlayObject` 的行号与四条过滤条件；并列出**未覆盖的残差**）。
第①片完整锚点仍在 [`../party-group-state-20261013/01-oracle-anchors.txt`](../party-group-state-20261013/01-oracle-anchors.txt)。

## 2. 改前 / 改后

见 [`02-change-summary.txt`](02-change-summary.txt)（语义改动仅返工 commit `4b6c843be` 的 4 个 `.rs`；
head 上的 `git merge origin/master` 只带入别的卡已合入 master 的改动，不碰本片语义）。

## 3. 判据（构造级 + 进程级，全离线）

见 [`03-tests.txt`](03-tests.txt)：
- `crates/world/tests/party_vectors.rs`（15 条，新增 3）：
  - `die_member_triggers_oracle_del_member_and_group_survives`：3 人组、成员 `hp=0` + `tick()`（**真实死亡写入点**）
    ⇒ 死者 `group_owner=0`、组不散、其余成员保留、事件向量 = `SendGroupMembers`×2（立即）+ `GroupCancel(t)`
    + 说词×3（排队序）。
  - `die_leader_disbands_group`：团长 `hp=0` ⇒ 全组 `group_owner=0`、成员表清空、逐成员 `GroupCancel`。
  - `play_object_by_name_ignores_non_player_entities`：同名怪物不误命中；只有怪物 ⇒ `CreateGroupFail{-2}`。
- `bins/game-svr/src/main.rs`（新增 1 条进程级）：`die_triggers_del_member_frames_on_production_path` ——
  走生产路径 `handle_party_cmd` 建组（两条连接）→ 死者 `hp=0` → `world.tick()` +
  `drain_party_death_events` ⇒ 断言帧序 `GCANCEL(L) → GCANCEL(T) → SYS×3`、死亡者自身 `T` 仍收
  `GCANCEL + SYS(退出说词)`、说词尾 = `EncodeString(〖组队〗T 已经退出了本组.)`。
- `bins/game-svr/src/party.rs`（1 条）：每条事件 → `SM_*` 帧形状。

## 4. 改坏必红（2 条，逐条 mutation → FAILED rc=101 → 还原）

见 [`redchecks/`](redchecks/)（`README.md` 索引 + 原始件）：
- **RC-1（F-1，reviewer 明确要求）**：`settle_deaths` 的 `if group_owner != 0` → `if false && group_owner != 0`
  （等价于删掉本次 `DelMember`）⇒ `die_member_...` + `die_leader_disbands_group`（2 FAILED）
  + 进程级 `die_triggers_...`（1 FAILED）。
- **RC-2（M-1）**：`party_play_object_by_name` 去掉 `e.race == ACTOR_RACE_PLAY &&` ⇒
  `play_object_by_name_ignores_non_player_entities` 必红。
- 还原后全部重新 ok；`git status --porcelain` 空（见 `RC1RC2-restored.txt`）。

## 5. 四门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/m2svr-target`，`--offline`）

见 [`gates/`](gates/)：
- `10-fmt.txt` rc=0
- `11-clippy.txt` rc=0（`--workspace --all-targets -- -D warnings`）
- `12-msgcodegen.txt` rc=0（604 条）
- `13-workspace-test.txt` rc=0（**95 suites · 927 passed · 0 failed · 21 ignored**）

## 6. 残差登记（如实登记，不静默）

- `GetPlayObject` 的其余过滤条件未逐字照搬（本片按 reviewer 建议**至少**加 `Race == Play`）：
  ① `!Ghost`；② `!(IsPasswordLocked && ObMode && AdminMode)`；③ `OrdinalIgnoreCase` 名字比较；④ 命中即 break（不继续找）。
  其中 `IsPasswordLocked` 在 Rust `Entity` **未建模** ⇒ 无法照搬；其余触发面窄、无本片判据覆盖。
  **旧 head 就存在的窄差异，不属 reviewer F-1/M-1 的收口范围**；如需严格等价另开小卡。
- `Die()` 的 `SuperMan` 早退与 `Die` 之后的掉落/经验链（`PlayObject.cs:1979+`）：不属本片，归 ②③/④ 卡。
- 第①片已登记项（`FunctionNPC.GotoLable` 脚本 label、`[String]` 两键、`AllowGroupReCall`、
  `GainExp`/`HAM_GROUP` 归 ⑦-4、tick 内相对次序）不变，详见 `../party-group-state-20261013/README.md §6`。
