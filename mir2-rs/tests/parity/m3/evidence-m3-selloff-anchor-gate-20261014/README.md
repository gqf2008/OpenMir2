# evidence-m3-selloff-anchor-gate-20261014

M3⑩-3 元宝寄售夹具**残余 Minor 收口**片（离线、零起栈、零端口、零 `.rs`）。卡：
`openmir2-m3-selloff-fixture-botsrv`（⑩-3）。本片关闭 reviewer-5 `7a575abe`（approve @`39f270c0`）
登记的非阻塞 **M-2：「场景 §B 的 oracle 锚点本轮未行级核对」**。

tier：**L**（纯夹具/工具/文档；不触 RNG 抽数序列、时序、持久化落库、战斗数值 ⇒ 等价 + 抽样冒烟即放行）。

## 背景（为什么这是「残余」）

⑩ 夹具骨架（T-44，`m3/evidence-m3-selloff-fixture-botsrv-20261013/**`）当年只交了比较器 + 无栈自检，
`oracle_anchors` 的 `File.cs:行号` **没有任何机判门**。⑧-10 / ⑨-5 两族随后各补了
`guild_castle_anchor_gate.py` / `hero_pet_anchor_gate.py`（同形 fail-closed 门 + 常驻夹具链深检门），
⑩ 缺这一族 ⇒ 场景锚可以静默漂移 / 复制错行号而不被发现。本片把 ⑩ 补齐到同口径。

## 本片做了什么

### A. 新增 `selloff_anchor_gate.py`（与 ⑧/⑨ 两族同形）

* **覆盖面**：`oracle_anchors` 解析出的每个 `file:spec`（`.cs` 与 `.sql` 两族）必须在 `REGISTERED`
  表内；表内多出 / 场景里重复引用 ⇒ RED。
* **读侧行级**：每条锚声明的每个 `(line, token)` 必须命中冻结面（`src/M2Server/**`、`src/GameSrv/**`、
  `src/OpenMir2/**`、`src/Modules/**`、`sql/**`）；token 取冻结行本体。
* **缺陷本体 token 锁死**：oracle「寄售不落库」的两个缺陷——`MySqlDB.cs` 读列名
  `DealChrName`/`BuyChrName` 与 DDL 的 `DealCharName`/`BuyCharName` **不符**（恒加载 0 行）、
  `SaveSellOffItemList` 三个调用点**全注释**——被 token 钉住 ⇒ 谁顺手「修好」oracle 就 RED
  （行为等价优先，要修另开 `S-*`）。
* **token 强度**：每条锚整组至少要有一个有辨识度 token（含字母/数字/下划线/CJK）⇒ 防只钉空壳行。
* `check` / `selftest`（6 类牙齿）/ `mutation`（MUT1–MUT6，含 MUT6 = 用替身 src 复现「DDL 列名被修好」）。

### B. 订正 4 处行号漂移（本片逐条对冻结 `src/` 复核的产物）

| 锚 | 旧（漂移） | 新（订正） | 依据 |
| --- | --- | --- | --- |
| `MySqlDB.cs` 读 | `251-291` | `251-288` | 291 是函数后空行；`LoadSellOffItemList` 实际 251..288 |
| `MySqlDB.cs` 写 | `293-330` | `295-337` | 293 是注释、330 是 `catch` 的 `{`；`SaveSellOffItemList` 实际 295..337 |
| `sql/mir2_data.sql` | `24-36` | `24-33` | 36 是 `-- Records of goldsales` 注释；`CREATE TABLE` 实际止于 33 `) ENGINE = InnoDB …` |
| `Messages.cs` | `769-828` | `765-816` | 769 漏掉块首 `RM_SENDDEALOFFFORM=23000`；828 是无关的 `RM_PLAYERKILLMONSTER=60001`；寄售块实际 765..816 止于 `RM_SELLOFFBUY_OK=23028` |

其余 15 条锚逐条复核后**本就正确**（`PlayObject.Message.cs:2046-2108` 的 8 条 CM/RM 派发、
`PlayObject.Sale.cs` 的 8 个入口、`M2Share.cs:79`、`PlayObject.cs:856/864`、`GameApp.cs:181-182`、
`GameSvrConf.cs:1217/1919`）。

### C. 接入两处常跑面

* `fixture_chain_gate.py` 的 `SHARED_CHECKS` 新增 `selloff_anchor_selftest` / `selloff_anchor_check`
  （与 castle / guild / hero 三组同口径），并加**防回退牙**「元宝寄售锚门已接入共享管线」+
  `.py` 登记项下界 11→13（链门 selftest **35/35**）。
* `run_selloff_pair.ps1 -SelfCheckNoStack` 新增 `anchor-gate-check` / `anchor-gate-selftest` /
  `anchor-gate-mutation-red` 三条（**19/19 → 22/22**），与 ⑧/⑨ 两族 runner 同形。

### D. 文档/登记

* `whitelist.md` 的 **T-44** 证据列追加本证据包指针 + 订正 `-SelfCheckNoStack` 计数 19/19→22/22。

## 读数（本片实跑，原始件见同目录）

| 项 | 读数 | 原始件 |
| --- | --- | --- |
| 锚门 check | `GREEN: …（19 锚 / 36 行点全部命中冻结面且覆盖面闭合）` rc=0 | `01` |
| 锚门 selftest | `SELFTEST GREEN`（6 类牙齿）rc=0 | `01` |
| 锚门 mutation | `MUTATION GREEN`（MUT1–MUT6 全 RED/MUT4 真身 GREEN）rc=0 | `01` |
| 修前 RED 复现 | 旧场景（`a2d8eaf32`）对新门 ⇒ **RED 8 项** rc=1（4 漂移「未登记」+ 4「表过期」） | `02` |
| ⑩ 夹具 runner 无栈自检 | `SELFTEST ok=22/22`（含 3 条锚门）rc=0 | `03` |
| 夹具链深检门（runner 级） | `FIXTURE_CHAIN GREEN runner_ok=10 prereq_or_skip=0 red=0`（含 selloff 两向 rc=0） | `04` |
| 四门禁 | fmt rc=0 / clippy rc=0(-D warnings) / 101 suites 1200 passed 0 failed 22 ignored / msgcodegen 604 rc=0 | `05`–`08` |
| 证据四件套 | `verify --check-extra` GREEN（仓库 LF/git-blob 口径）+ `eolcheck` GREEN | `SHA256SUMS.txt` |

## 边界

见 `09-boundary.txt`：base = `origin/master` `a2d8eaf32205c720613ae8817073bf8d2fdaaae2`；
净差量全在 `mir2-rs/tests/parity/**`（新 `m3/selloff_anchor_gate.py` + `m3/selloff_scenario.json` +
`m3/fixture_chain_gate.py` + `m3/run_selloff_pair.ps1` + `whitelist.md` + 本目录）；冻结面
`src sql MirClient mir2-rs/crates/protocol mir2-rs/crates/storage mir2-rs/bins` = **空**；`*.rs` 计数 = **0**。

## 残留（如实登记）

1. 锚门「行内容 ⇄ 散文声称构造」的**语义层**仍是抽样人核（可机判部分 = 覆盖面 + 行点 token + 强度 + 缺陷本体）。
2. ⑩-3 的**卡级真机 DoD**（两侧同序列 `diff=0`、`openmir2-m3-clock-injection` + owner 独占窗口）**不在本片范围**，
   卡保持 `in-progress`。
3. `run_selloff_pair.ps1` 的 `-SelfCheckNoStack` 计数从 19/19 变 22/22（T-44 已同步）。
