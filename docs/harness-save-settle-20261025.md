# harness 存档落定时点：DB 快照前加「可观测静默」等待（卡 `openmir2-harness-save-time-symmetry`，tier R）

- 卡：`openmir2-harness-save-time-symmetry`（issue `45acbc29`；owner `openmir2-dev-1`；branch `card/harness-save-time-symmetry`；
  base **`055a774af`**（= 本 patch 的 `--base`，与 review 逐字对齐））。写集：`tools/conformance/card_window.py`、本文件。
- 授权：**只在 `.worktrees/dev-1` 的本分支改**；**不动 tester 的工作副本**（她的 W1 批次在跑）。tester 以 reviewer 身份审 delta，
  批次结束后由 Leader 合并。批次期间不改 `card_window.py` 的生产副本。
- **本卡只写代码与设计**：真机自证（停栈前后两份快照 + 重复两次）排在 W1 批次之后；本文件 §6 如实登记未做的部分。

## 1. 问题（实验 A，dev-1 comment `19f12a34`）

- `run_side` 的 DB 快照在 probe 之后、停栈之前无条件执行（对称）。
- 真正的不对称在**运行期断线存档**：probe 关闭客户端连接后，
  - **Rust** 在自己的 gate 事件里**同步** `save_human_rcd`（`lib.rs:22452` → DB 队列）⇒ 紧接着的快照看得到；
  - **C#** 的 GameGate `ClientSession` 只 `Socket.Close()`（`:87/161/173`，无显式登出消息）；GameSvr 靠 per-user socket
    关闭走 `WorldServer.cs:710-715` 离线支 → `SaveHumanRcd` → **异步** FrontEngine→DBSrv→MySQL；随后 harness 立刻拍库、
    并在 finally **强杀 C#**（`Stop-Process -Force`）⇒ 这条异步存档既可能没跑完、也会被强杀带走。
- 后果：DB 面出现「Rust 已落库、C# 仍是基线值」的假差异。W1 n=1 `login_only` 的
  `characters_ablity.Weight a=14 / b=0` 即此形态（两侧 `SM_ABILITY(52)` 内存 weight **都是 0**，dev-1 解码证据）。

## 2. 设计（Leader 修订版：低侵入）

在 **probe 之后、`dbsnap` 之前**插入「存档落定」等待，**判据可观测**：

- **指纹**（每次轮询一条 SQL，两侧同一句）：

  ```sql
  SELECT COALESCE(MAX(c.`LASTUPDATE`), ''), COUNT(DISTINCT c.`Id`),
         COALESCE(MAX(a.`ModifyTime`), '')
  FROM `<db>`.`characters` AS c
  LEFT JOIN `<db>`.`characters_ablity` AS a ON a.`PlayerId` = c.`Id`
  WHERE c.`ChrName` = '<角色>';
  ```

  - **列名以冻结 schema 为准**（`sql/mir2_db.sql`）：`characters` 的时间列是 **`LASTUPDATE`**，
    **没有 `ModifyTime`**。（`ModifyTime` 在 `characters_ablity` / `account` 上——写成
    `MAX(c.ModifyTime)` 会让每侧 mysql 报 Unknown column、两侧都 rc=2。这是 review `b48197f6` F1 的 Critical。）
  - **第二见证** `characters_ablity.ModifyTime`：两侧存档都同时写这两列
    （C# `PlayDataStorage.Save.cs:85-88` / `:141`、Rust `playdata.rs:423-427` / `:486`），任一列变了指纹就变。
- **静默检测**：按 `poll_ms` 轮询指纹，**当前「指纹不变」的连续区间持续 ≥ `quiet_ms`** 才认为落定；
- **静默是负向判据**：恒定指纹同样会被判成落定（`changes == 0` 也 settled）。所以默认
  `require_changes = true` —— **从 arm 点起至少观测到 1 次变化**，否则 `no_change_observed` 且该侧 **fail-closed**。
  **静默 ≠ 存档已触发**：不写库的场景，静默是常态，不能拿它当「已存档」的证据。
- **窗口 `window_ms`（默认 5000，上限 5000）**；窗口内不落定/无变化 ⇒ **fail-closed**：该侧 `rc=2`，
  `save_settle.json` 的 `error` 写 `save_unsettled: …`（含采样数与 changes），**不拍库**；
- **禁止固定 sleep 充当证据**：`sleep` 只用于轮询节拍（`poll_ms`），不是判据；判据永远是「可观测到的静默（且观测到过变化）」；
- `--dry-run` 只打印计划（db/行/window/quiet/poll），不连库；
- 每侧证据：`<side>/save_settle.json` =
  `{side, db, enabled, window_ms, quiet_ms, poll_ms, require_changes,
    samples:[{t_ms, sha256_12, witness:[lastupdate, count, abil_modifytime](, witness_error)}],
    witness_first, witness_last, changes, settled, elapsed_ms,
    action(settled|no_change_observed|timeout|disabled|dry-run), rc, error}`；
  - **每条采样带原始见证**（`mysql --batch` 的一行三列原值），首末另存 `witness_first/last`：
    真机那一轮要能回答「变化来自哪一列 / 基线是不是 NULL / 两次变化是否落在**同一秒**」
    （`LASTUPDATE` / `ModifyTime` 都是 `datetime` ⇒ **秒粒度**；同秒两次存档在指纹上看不出变化）。
    见证解析失败不改变判据（判据仍是整行指纹的 sha256），只在样本里记 `witness_error`；
  窗口报告 `card-window.json` 的 `save_settle.sides.<side>` 另带 `{changes, settled, elapsed_ms, action, rc, error}`。

配置（profile 顶层 `save_settle`，`sides.<side>.save_settle` 覆盖本侧；全部 fail-closed）：

```json
{"save_settle": {"enabled": true, "window_ms": 5000, "quiet_ms": 800, "poll_ms": 200,
                 "require_changes": true}}
```

| 键 | 允许 | 默认 | 非法即拒 |
|---|---|---|---|
| `enabled` | bool | `true` | 非 bool |
| `window_ms` | 0..5000 | 5000 | 越界（>5s） |
| `quiet_ms` | 0..5000 且 ≤ `window_ms` | 800 | 越界 / > `window_ms` |
| `poll_ms` | 50..1000 | 200 | 越界 |
| `require_changes` | bool | **`true`** | 非 bool |

- **缺键 = 安全侧**：`wait_save_settled` 用 `settle.get("require_changes", True)`。任何**不经过
  `profile_save_settle()`** 的调用（测试、将来新加的路径）都不能因为「少一个键」把这道负向门
  静默关掉——实测过：不带该键 + 恒定指纹曾给出 `rc=0 settled / changes=0`。

- **`require_changes` 默认开的理由**：静默只证明「此刻没人在写」，不证明「已经写过」。**确实不写库的只读场景**
  才可显式关掉，并在该线程 `comment` 里写明「本场景不产生状态变化」。
- **`require_changes=false` 的证据等级（必须照此读）**：关掉后该侧 `save_settle.json` 只能读作
  「**未观测到变化**」，**不得**当作「存档已完成」的证据——它既可能是「本来就不写库」，
  也可能是「存档还没到/根本没触发」，两者在证据上不可区分。要用它给 DB 面背书，必须另有
  独立证据（例如同场景下两侧 DB 快照逐字段一致且与帧面一致）。
- **边界语义（显式定死，别靠猜）**：
  - `quiet_ms = 0` ⇒ 第 **2** 次采样即判落定（跨度 0 ≥ 0），即约 `poll_ms` 后返回；这是**显式放行**的语义，
    不表示「立刻确认存档」。
  - `window_ms = 0` ⇒ 循环只采一次就 `elapsed >= window` ⇒ **必 `timeout`**（rc=2）。要关等待请用 `enabled: false`，
    不要用 `window_ms: 0`。
  - `poll_ms` 是节拍：`quiet_ms < poll_ms` 时实际静默粒度就是 `poll_ms`。
- **5000ms 只是待标定值**：真机应记录「probe 结束 → 首次 `LASTUPDATE`/`ModifyTime` 变化」的间隔，
  用它来定默认 `window_ms`（§6 待办）。

**备选方案（未采用，登记）**：C# 侧显式关会话（协议内干净 logout）后等 `SaveHumanRcd` 落地——同样需要可观测判据；
若将来实测显示「窗口内 C# 仍不落库」，应升级到该方案或给 C# 换优雅停（WM_CLOSE/ConsoleCtrlEvent，触发
`AppService.SavePlayer()`），并补「停栈后 DB 可观测变化」的证据。

## 3. 影响面（**重要**，对应验收⑤）

- 今日 W1 批次的 **DB 面 verdict 全部作废、需重跑**（`db` 段、`characters_*`、`assets` 等以 DB 快照为判据的项）；
- **帧面证据不受影响**（本改动不触碰 probe/compare）；
- 语义变化：DB 快照从「probe 结束瞬间的内存/未落库混合态」改为「**两侧都观测到存档静默（且观测到过变化）之后**的态」；
- **行为面新红**：默认 `require_changes=true`，窗口内一次变化都没观测到的侧会 `rc=2 save_unsettled / no_change_observed`
  ——这是有意的 fail-closed（该侧 DB 面本来就不该采信）；只读场景请在 profile 里显式关掉并说明；
- 依赖 DB 快照的既有 selftest：编排顺序断言改为剔除 settle 轮询步（`restore/stack/probe/save_settle*/db/stop`），
  自测 profile 用快档 `300/100/50` + `require_changes:false` 避免每次真等 0.8s；settle 语义由 `SETTLE1..16` 覆盖。

## 4. 实现要点（`card_window.py`）

- `profile_save_settle(prof, side=None)`：配置校验（见 §2）；
- `settle_decision(samples, quiet_ms)`：纯函数静默判定（`changes`/`elapsed` 一并返回）；
- `_save_settle_sql(db, chr_name)`：两见证指纹 SQL（见 §2）；
- `_save_settle_witness(fingerprint)`：把一行三列拆成 `[lastupdate, count, abil_modifytime]`
  （解析失败返回 `(None, error)`，不影响判据）；
- `_schema_columns(table, sql_path=None)`：从**冻结 schema** `sql/mir2_db.sql` 解析列集合（离线，不连库）；
  `_sql_identifiers(sql)`：取出 SQL 里的反引号标识符（先剥块注释）。二者仅供离线牙使用；
- `wait_save_settled(runner, side, prof, sc, settle, env, sql_fn=None, clock=None)`：
  dry-run 只打印；生产走该侧 mysql 客户端（`stdin_text` 喂 SQL）；`sql_fn`/`clock` 可注入供单测；
- `run_side`：`# 6) probe` 之后、`# 7) 抓库` 之前调用；失败写 `<side>/save_settle.json` 并 rc=2 提前返回；
- summary/`card-window.json`：`summary_save_settle_decl` 展示每侧生效配置，`save_settle.sides.<side>` 带
  `changes/settled/elapsed_ms/action/rc/error`（review `b48197f6` F2①）；
- dry-run 编排：打印每侧 settle 计划。

## 5. 测试与牙

- `card_window.py --selftest` ⇒ **424/424 GREEN**（本卡新增 `SETTLE1..19`，共 26 条含牙）；
- 牙（离线）：
  1. 未知键 / `window_ms>5000` / `quiet_ms>window_ms` / `poll_ms<50` / `enabled` 非 bool / `require_changes` 非 bool ⇒ 各一条拒；
  2. 静默判定四例（恒定、静默不足、变化后静默、持续变化）；
  3. `wait_save_settled` 注入 sql：变化后静默 ⇒ rc=0；持续变化 ⇒ **rc=2 `save_unsettled`**；
     sql 抛错 ⇒ rc=2；`enabled=false` ⇒ 不连库 rc=0；
  4. **schema 牙（F1）**：`_save_settle_sql` 里出现的每个反引号标识符都必须落在
     `sql/mir2_db.sql` 解析出的 `characters` ∪ `characters_ablity` 列集（+ 库名/表名）里，
     且 `LASTUPDATE` / `ModifyTime` / `ChrName` 必须出现、`characters` 的列集里不得有 `ModifyTime`；
     **变异实测**（把指纹列改坏再跑，牙必须红）：
     `MAX(c.LASTUPDATE)`→`MAX(c.Nonsense)` ⇒ `FAIL SETTLE13 ['Nonsense']`；
     →`MAX(c.ModifyTime)`（错表）⇒ `FAIL SETTLE13`（`LASTUPDATE` 消失）；
     追加 `MAX(a.Bogus)` ⇒ `FAIL SETTLE13 ['Bogus']`；
  5. **`require_changes`（F2②）**：恒定指纹 ⇒ `rc=2 / no_change_observed`（`settled` 仍为 true，说明静默单独不足以采信）；
     观测到 1 次变化 ⇒ rc=0；显式 `require_changes:false` 的只读场景 ⇒ 零变化也 rc=0（逃生阀本身要有牙）；
  6. **边界（F5）**：`quiet_ms=0` ⇒ 2 次采样即 settled；`window_ms=0` ⇒ `timeout`、只 1 次采样；
  7. **缺键落安全侧（F2 补）**：`settle` 不带 `require_changes` + 恒定指纹 ⇒ 必须 `rc=2
     no_change_observed`（不许静默降级放行）；
  8. **原始见证**：合法三列 ⇒ `samples[*].witness` 与 `witness_first/last` 逐字保留；
     解析不了 ⇒ `witness=null` + `witness_error`，判据（整行 sha256）不受影响；
  9. **报错串用实际 `elapsed_ms`**：`no_change_observed` 里的「实际观测的 Nms」必须是真实
     经过的时间（< `window_ms`），不许拿窗口上限冒充「等满了」；
  10. 源码级接线：`run_side` 在 `# 7) 抓库` **之前**调用 `wait_save_settled`。

## 6. 未做 / 待真机（如实登记）

- **真机自证**（验收③，判据已钉死四条）：① 每侧 `action=settled` 且 `changes>=1`（`no_change_observed`
  直接红）；② **停栈前**快照两侧 `characters_ablity.Weight` 一致（证伪 W1 n=1 的 14/0）；
  ③ **停栈后**快照与停栈前**逐字段一致**（证明存档在拍库前确实已落定，不是关机才写）；
  ④ 重复两次结论相同，四份快照的 sha256 与 `<side>/db/` 产物路径能对上 —— **等 W1 批次结束**后跑；
- **`window_ms` 标定**：真机用 `samples[*].t_ms` + `witness` 原始时间戳标定「probe 结束 → 首次变化」
  的真实间隔（并记两次变化是否落在**同一秒**），据此回填默认 `window_ms`；当前 5000ms 只是占位值；
- **C# 优雅停**（验收②）：本版按 Leader 修订走「落定等待」，未改 C# 停法。因此真机那一轮**必须证明
  C# 侧确实产生了可观测变化**（`changes>=1`）——「等了 5s 没变化也放行」不算通过；
  若实测仍不落库，再上显式关会话/优雅停并补存档发生证据；
- **Weight 卡**（`openmir2-ability-weight-login-writeback`）保持挂起：本改动落地并复拍后，若 Weight diff 消失 ⇒
  该卡前提不成立，应改判据/关卡（另开卡处理）；本卡**不改** `src/**`。

## 7. 门禁

`card_window.py --selftest` 421/421；卫生门（`conflict_marker` / `stale_pointer --ref master` /
`dynamic_count --ref master` / `board_coverage` / `t_reservation check` / `scenario_undecidable_audit`）+
`git diff --check <base> <head>` + `merge_tree_helper`：见 patch body。**未跑 cargo、未起栈、未占端口。**
