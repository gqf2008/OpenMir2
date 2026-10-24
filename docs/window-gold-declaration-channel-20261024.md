# 夹具 Gold/资产声明通道（`openmir2-window-gold-declaration-channel`，tier=L）

- 卡：`openmir2-window-gold-declaration-channel`；分支 `card/window-gold-declaration-channel`；
  执行者 openmir2-worker-76；基线 `walgit/master = 21484daaa`（本人上一卡
  `card/window-inventory-fixture-adapt` 合并点）。
- 写集：`tools/conformance/card_window.py`、本文档。**零 Rust 变更、零场景 JSON 变更、
  零 probe 变更**（`git diff --stat` 仅上述两文件）。
- 红线遵守：写库只走两侧测试库（`mir2_test_db` / `mir2_test_db_r`）且 UPDATE 仅限基线角色
  `winplay0`（Id=6228）一行；跑完带外 restore 复原；mysqld（3306）全程在线未动；
  未写 `mir2_db` 生产数据；未改 `src/**`、服务端 `rules.json`、`upgrade_retrieve.json`。

## 1. 通道设计（acceptance①）

背景：部署前置卡（`openmir2-window-deploy-prereq-batch1`，在审）披露 upgrade_retrieve 需要
「Gold>=10000」前置，而 placement（只写 MapName/CX/CY）与 inventory（只写 characters_bagitem）
两通道都不含 `characters.Gold` ⇒ 金币前置无法声明。本卡在 card_window.py 夹具层新增第三条
「资产声明通道（assets）」，实现范式照 inventory：白名单 + UPDATE + 读回断言 + 指纹进账 +
restore 链闭合。

### 1.1 场景声明形状（BNF）

场景**顶层**声明（与 placement/inventory 平级）：

```bnf
<assets_decl> ::= '"assets"' ':' '{' <pair> ( ',' <pair> )* '}'
<pair>        ::= <col_pair> | <note_pair>
<col_pair>    ::= '"' <col> '"' ':' <value>
<col>         ::= 'Gold'                        (* 白名单 ASSETS_COLUMNS，首版仅 Gold *)
<value>       ::= <JSON integer 0..2147483647>  (* bool/浮点/字符串/null 一律拒 *)
<note_pair>   ::= '"note"' ':' <string>
```

- 空 `{}` / 只有 note 没有列 ⇒ 拒（无意义声明）。
- 未知键（不在白名单也不是 note）⇒ 加载期 rc=2 点名列名与白名单（fail-closed）。

最小示例（本卡验证用临时场景，非仓内场景）：
`"assets": {"Gold": 10000, "note": "upgrade_retrieve 前置 Gold>=10000"}`

### 1.2 列白名单与扩展规则（只读取证）

`DESCRIBE mir2_test_db.characters`（2026-10-24，`mir2_test_db_r` 同构）：42 列；
`Gold int(11) YES Default 0`——有符号 32 位。据此：

- 首版白名单 `ASSETS_COLUMNS = ("Gold",)`，值域 `0..2147483647`（`ASSETS_MAX`）；
- 扩展规则（写进常量注释）：只加「有符号整数、语义是角色资产/点数、服务端只在登录/结算时读」
  的列（如 `GamePoint int(11)` 同型可加）；绝不加字符串列/时间列/主键外键列；
- 通道依赖列 `ASSETS_REQUIRED_COLUMNS = (Id, ChrName, LoginID, Deleted)`，缺列即拒；
- 基线 `winplay0`（双侧测试库）`Gold=100`，声明 `Gold=10000` 即为真实变更。

### 1.3 运行语义（与 placement/inventory 同范式）

- 时机：每侧 **restore 之后、起栈之前**（restore 复位库、角色记录只在登录时读一次）；
- 定位：按 profile `char_name`/`account`（alt=winplay0/winplay0）在 `characters` 上解析，
  命中非 1 行或 `Deleted=1` ⇒ rc=2；
- 写入：`UPDATE <测试库>.characters SET Gold=10000 WHERE Id=<row>` + `SELECT ROW_COUNT()`
  （影响行数 != 1 ⇒ rc=2）；已是目标值 ⇒ `noop`（不打 UPDATE）；
- 读回断言：写后同一条读语句复验 `Gold=10000`，不符 ⇒ rc=2；
- 指纹进账：写前后对**声明列之外的其余列**做 `MD5(CONCAT_WS(CHAR(31), IFNULL(CAST(col AS
  CHAR), '~')))` 对比，变化 ⇒ rc=2（只许写声明列）；
- 库安全：库名走 `placement_characters_db`（prod 名/无法判定为测试库 ⇒ 拒绝）+ `_check_db_pairing`
  与该侧 `Server.conf` 的 `[DataBase] database` 交叉核对；
- 证据：每侧落 `<out>/<side>/assets.json`，summary 增 `assets` 块（enabled/declared/sides/note）；
- dry-run：预览行 `资产声明通道（dry-run 不连库）：db=… Gold=10000 角色=winplay0` + 离线校验
  （库名/库配对），不连库。

### 1.4 实现位置（`tools/conformance/card_window.py`）

| 位置 | 内容 |
|---|---|
| 头注释 | 「资产声明通道（assets…）」用法块（白名单/值域/时机/边界） |
| `run_assets` 节注释块 | 选型、声明形态、probe 边界注记 |
| `ASSETS_COLUMNS` / `ASSETS_MAX` / `ASSETS_REQUIRED_COLUMNS` / `ASSETS_NOTE_KEY` | 白名单常量 |
| `scenario_assets(path)` | 场景顶层解析（fail-closed） |
| `_assets_row_sql` / `_parse_assets_row` | 读一行：命中数/Id/声明列现值/其余列 MD5/Deleted |
| `run_assets(runner, side, prof, sc, assets, env)` | 真跑（定位/UPDATE/ROW_COUNT/读回/指纹/noop） |
| `preview_assets(...)` | dry-run 预览（离线校验 + 命令序列） |
| `execute()` | 加载期解析、summary `assets` 块、dry-run 预览接线、`run_side(assets=…)`、侧证据合并 |
| `run_side()` | 签名加 `assets=None`；restore 后、起栈前执行，失败 rc=2 不起栈 |
| `_selftest_assets(tmp)` + selftest 内联 18n | 牙与接线断言（见 §3） |

## 2. 边界：probe 侧顶层键白名单（如实记录，非本卡写集）

`tools/probe/mir2_probe.py::load_scenario` 的顶层 optional 键白名单暂未放行 `assets`，
实测 `--check-scenario` 对带 `assets` 声明的场景 rc=2：
`scenario-error: scenario has unknown key(s): ['assets'] (fail-closed)`
（留痕 `E:/tmp/goldfix76/probe-boundary.log`）。

含义：**通道本身已就绪**（card_window 加载/dry-run/真跑全通），但某仓库内场景 JSON 要带上
`assets` 声明时，还需在场景采用卡里同步两行 delta：probe 白名单加 `"assets"` + 形状校验
`_check_assets`（与 `scenario_assets` 同口径）。本卡按卡面约束不碰场景 JSON、不碰 probe；
此边界已在卡面 notes 预授权（「通道就绪后场景采用另说」）。

## 3. selftest 牙（acceptance②：牙 ≥3）

`_selftest_assets` + 内联 18n 共 **16 条新增 chk**，selftest 总分 256 → **272/272 全绿**：

| 编号 | 断言 | 对应 acceptance② |
|---|---|---|
| ASSET 牙1 | 非白名单列（Level/CreditPoint/MasterName/CREATEDATE）⇒ ConfigError 且点名白名单 | ① 非白名单列 ⇒ 红 |
| ASSET 牙2 | 负数/字符串/浮点/bool/null/超 int32 ⇒ ConfigError | ② 负数/非数字 ⇒ 红 |
| ASSET 牙3 | dry-run 预览行出现 `Gold=10000` 且 rc=0 | ③ dry-run 预览行出现目标值（正牙） |
| ASSET1/3/4/4b | 合法声明解析透传；只有 note/空对象拒；行 SQL 现值槽/指纹分工；退化兜底 | 支撑 |
| ASSET5/5b | 假 runner 走通 run_assets 真路径（UPDATE 只 SET Gold=10000、读回相符）；noop 幂等 | 支撑 |
| ASSET6a-6d | 指纹变化/读回不符/ROW_COUNT=0/角色不存在 ⇒ rc=2 | 失败面 |
| ASSET7 | `run_side` 源码序：`if assets:` → `run_assets(` → `"stack"`（防旁路/挪位） | 接线 |
| 18n dry-run | execute(dry_run=True) 带 assets 场景 rc=0 且预览命令行出现 `SET Gold=10000` | 正牙（CLI 级） |
| 18n execute | run_assets 失败 ⇒ summary 记证据、整体 rc=2、不起栈 | 失败接线 |

**变异牙（AST-M1..M7，仓外驱动，逐个变异 → selftest 必须判红 → 逐字节还原，sha256 复核一致）**：
白名单检查、值域检查、dry-run 预览行、指纹比对、读回断言、run_side 接线、ROW_COUNT 检查
——7/7 全红后原样还原。

## 4. 真库链路（acceptance①「UPDATE 仅限基线 winplay0/测试库 + 读回断言 + restore 复原链闭合」）

驱动 `E:/tmp/goldfix76/acc4.py`（不入仓；SQL/逐步留痕 `E:/tmp/goldfix76/acc4-run.log`）。
前态：双侧 `winplay0`（Id=6228）`Gold=100`。

| 用例 | 库 | rc / action | 通道读回 | 独立 SELECT（不经通道） | 指纹 |
|---|---|---|---|---|---|
| csharp 声明 Gold=10000 | mir2_test_db | 0 / updated | Gold=10000 | 10000 | before==after |
| rust 声明 Gold=10000 | mir2_test_db_r | 0 / updated | Gold=10000 | 10000 | before==after |
| 双侧重跑（已达标） | 两侧 | 0 / **noop** | 不打 UPDATE | — | — |

**恢复现场**：带外 restore（`E:/tmp/conformance/restore-baseline.sh` + `restore-baseline-r.sh`，
机制见 `docs/runbook-tool-consistency-audit-20261023.md` §5）双侧 rc=0；复验：两侧
`winplay0` `Gold=100`、命中行数各 1——与开工前一致。mysqld 全程未动。

**真库实锤并修复的 bug**：首版 `_assets_row_sql` 的派生表没把声明列透传，外层
`MIN(`Gold`)` 报 `ERROR 1054 Unknown column 'Gold'`——假 runner 桩测不出（桩按 tag 返回
合成输出），只有真库取证暴露。修复：派生表 SELECT 显式透传声明列，并把「派生表透传」
固化进 ASSET4 断言（防回退）。这也是本卡坚持真库验证的原因。

## 5. 门禁（acceptance④；全部在最终代码上实跑）

| 项 | 结果 |
|---|---|
| `card_window.py --selftest` | GREEN 272/272 |
| `compare_runs.py selftest` | GREEN 293/293 |
| `stale_pointer_gate.py` | rc=0（154 files scanned） |
| `dynamic_count_stamp_gate.py` | rc=0（0 缺戳，扫 154 markdown） |
| `conflict_marker_gate.py` | rc=0 |
| `git diff --check` | rc=0 |
| `git merge-tree --write-tree walgit/master HEAD` | rc=0 无冲突 |
| `mir2_probe.py --check-scenario`（带 assets 临时场景） | rc=2 unknown key（§2 边界，预期行为） |
| Rust 变更 | 无（diff 仅 `tools/conformance/card_window.py` + 本文档） |

## 6. 残留物清单

| 路径 | 是什么 | 处置 |
|---|---|---|
| `E:/tmp/goldfix76/`（patch 脚本、acc4.py、acc4-run.log、mutations*.py、gate-*.log、dryrun-acc.log、scen-assets-acc.json、probe-boundary.log） | 交付驱动与门禁留痕 | 保留供 review 取证 |
| `E:/tmp/goldfix76/dryrun-acc/`、`acc4-out/` | dry-run/真跑 out 目录 | 可删 |
| 两侧测试库 | 已 restore 回基线（winplay0 Gold=100、行唯一） | 与开工前一致 |
