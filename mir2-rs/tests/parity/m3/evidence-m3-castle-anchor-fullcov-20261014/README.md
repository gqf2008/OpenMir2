# ⑧-6 锚门残余 Minor 收口（M-1 `audit` 入库 + M-2 全文件覆盖面）

卡：`openmir2-m3-castle-file-load`（M3⑧-6：`Castle/List.txt` + `SabukW.txt` + `AttackSabukWall.txt`
→ 门/墙/弓手/守卫实体）。
tier：**L**（纯工具 + 注释/证据；`mir2-rs/crates/world/src/castle.rs` 只有 doc-comment 与
测试注释改动，机判过滤注释后**零语义差异**；`castle_oracle_anchor_gate.py` 是离线判据工具）。

## 收口的两条 reviewer Minor

来源 = reviewer-5 `8276f7bb`（⑧-6 锚门片 `2981b5a2f` 的 approve）。

* **M-1（Important→非阻塞）**：`evidence-m3-castle-anchor-audit-20261014/10-anchor-audit.txt` 的
  「覆盖区间 50 锚 / 49 distinct / unresolved=0」由**未入库的临时脚本**
  `E:/tmp/m3cs-work/anchor_audit.py` 产出 ⇒ **该件在检出内不可独立复算**。
  reviewer 建议：「把 audit 逻辑并入 gate 或把脚本入库」。
  **本片做法**：把清点逻辑并入 `castle_oracle_anchor_gate.py` 的 **`audit` 子命令**
  （与 `check` 的覆盖面**同源**：同一 `_covered_spans` + 同一正则），仓内一条命令可复算（见 02）。
* **M-2（Minor）**：`COVERED_MARKERS` 原只括住 `castle.rs:1-175`（⑧-7 模块头）⇒ ⑧-8/⑧-9 段落的
  `文件.cs:NNN` 锚不在「未登记即红」覆盖面内。
  **本片做法**：覆盖面扩到**整个 `castle.rs`**（起标记 = 文件首行，止标记 = `None` ⇒ EOF），
  并把区间内**原先未登记**的锚逐条补进 `ANCHORS`（每条带 `(line, token)` 读侧机判），
  同时把区间内**裸锚**（只有 `:NNN`、无文件名）逐条改写为 `文件.cs:NNN`。

## 读数（本片，见 01–04）

| 项 | 读数 |
| --- | --- |
| `check` | **GREEN**：159 锚命中冻结 `src/` + 写侧在位 + 覆盖区间无未登记/裸锚 |
| `audit` | **GREEN**：覆盖区间 `[(1, 2807)]`（全文 2807 行）；已登记 159；区间内锚 158；未登记 0；裸锚 0 |
| `selftest` | **GREEN**（新牙：全文件覆盖面 = `[(1,2807)]`；audit 清点器真身两张清单都空） |
| `mutation` | **GREEN**：MUT1–MUT8 全 RED（新增 MUT7 全文覆盖面尾部未登记锚、MUT8 audit 清点器删表项） |
| 外部改坏必红 | RC-1 `UserCastle.cs:930`→`:931` ⇒ `check`/`audit` RED 且点名；RC-2 文件尾部注入裸锚 `:7777` ⇒ RED；**还原后复绿且 `castle.rs` sha256 与改前一致**（05） |
| 四门禁 | `fmt` rc=0 / `clippy -D warnings` rc=0 / `test` 101 suites **1200 passed 0 failed 22 ignored** / `msgcodegen` 604 rc=0（06–09） |

改前锚数（`ANCHORS` 表项，`git show 30c52f85d:mir2-rs/tests/parity/m3/castle_oracle_anchor_gate.py` 机判）
= **82**；改后 = **159**（净增 **77**）。覆盖面区间从 `[(1,175)]` 变为 `[(1,2807)]`。

> **O-2 订正（reviewer-6，收口片）**：原文写的「改前 = **83**（原件 158−76 补登+1 补 `:939`）」
> 与上一片 reviewer-5/6 在 `2981b5a2f` 上机判的 **82** 冲突；本次复算 base `30c52f85d` 的表项
> 确实 = 82（159−82 = 净增 77）⇒ 83 为笔误，已订正；`audit`/`check` 仍以 159 为准且自洽。

## M-2 的补登口径（重要，供复核）

* 补登的 **76** 条锚**全部取自 `castle.rs` 既有散文**（本片只改它们的**写法**：裸锚补文件名；
  3 处 doc 引用的 oracle 行号**订正为实测值**：`RepairDoor` 的 `UnderWar` 守卫 `:920-923`→`:920-927`、
  60s 门 `:951`→`:930`（活门，死门另有 `:939`）、满血守卫 `:923`→`:924`）。
* 两处**空白/纯注释端点**（如范围止行为 `}`）的读侧 token 取该区间内**最近的非注释代码行**
  （例：`:920-955` 止行 token 取 948 的 `return result;`）——覆盖面判定不受影响；
  逐条语义级复核（每条锚的「声称构造」与行内容一一对应）仍建议由后续片做（见下）。
* `COVERED_MARKERS` 的止标记 `None` 语义由 `_covered_spans` 新增支持（`None` ⇒ 到 EOF）。

## 残留（如实登记，不在本片范围）

1. **逐条语义级锚复核**：76 条补登锚的 `(line, token)` 已机判「该行含该串」，但「该行**就是**散文所声称
   的构造」这一层仍是抽样人核；建议后续片用 `audit --json` 的清单逐条做语义核对。
   **部分收口（O-1，收口片）**：已在 `check`/`selftest`/`mutation` 新增「读侧 token 强度」牙
   （每条锚整组至少要有一个非纯标点 token，整组只剩 `}`/`;` ⇒ RED；MUT9）—— 防「只钉在
   空壳行上」的弱锚；「行内容 ⇄ 散文声称构造」的**语义层**仍诚实登记为抽样人核。
2. `castle.rs` 的既有行号散文仍可能有历史漂移（本片只订正了 3 处机判确证的）。
3. ⑧-6 卡级剩余 DoD 不变：① `bins/game-svr` 把 `SabukWConfig→CastleDefenseSpecs` 并调 `initialize`
   （等 owner 对 `bins` 串行序裁决）② 真机两侧 `run_guild_castle_pair.ps1` 同序列 `diff=0`
   （归 ⑧-10 夹具 + clock-injection + 独占窗口）。

## 边界

见 `10-boundary.txt`：净差量 = `mir2-rs/crates/world/src/castle.rs`（注释/测试注释）+ 
`mir2-rs/tests/parity/m3/castle_oracle_anchor_gate.py`（工具）+ 本证据目录。
冻结面 `src/ sql/ MirClient/ crates/protocol/ crates/storage` 空；`src/BotSrv` 空；
零新增 `.rs` 行为改动（`castle.rs` 机判注释过滤后为空）。
