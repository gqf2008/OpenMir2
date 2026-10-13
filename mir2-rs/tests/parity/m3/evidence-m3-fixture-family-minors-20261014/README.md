# evidence-m3-fixture-family-minors-20261014

M3 夹具族**残余 Minor 收口**片（离线、零起栈、零端口、零 `.rs`）。卡：`openmir2-m3-fixture-chain-gate`
（夹具族共享管线/常跑门 umbrella）；本片关闭的残余分布在 `openmir2-m3-guild-castle-fixture-botsrv`（⑧-10）、
`openmir2-m3-castle-file-load`（⑧-6）、`openmir2-m3-hero-pet-fixture-botsrv`（⑨-5）。

tier：**L**（纯夹具/工具/文档；不触 RNG 抽数序列、时序、持久化落库、战斗数值 ⇒ 等价 + 抽样冒烟即放行）。

## 本片修了什么（含一条 master 上的既存 RED）

### A. ⑧ 行会/攻城锚门：BotSrv 工具侧改「名字锚」，修 master 上 `FIXTURE_CHAIN` RED（核心）

**现象（`05`）**：④ 掉落/经验夹具片（T-50，`9dcfb6f34`）改了 `src/BotSrv/Player/RobotPlayer*.cs`
⇒ 行号合法漂移（`SendGuildDlg` 4708→4916、`SendMerchantDlgSelect` 4553→4761、
`case SM_OPENGUILDDLG:` 1319→1321）。`guild_castle_anchor_gate.py` 把 BotSrv **行号**钉进
fail-closed 表 ⇒ 在 **master 上**该门恒 RED ⇒ 常跑的 `fixture_chain_gate.py check` 也随之 RED。

**根因**：工具侧（`src/BotSrv/**`，T-41..T-50 在途面）拿**行号**当 oracle 判据。⑨-5 的
`hero_pet_anchor_gate.py` 早已按「工具侧只锚函数/字段名、不给行号」的口径处理；⑧-10 未对齐。

**本片做法**：`guild_castle_anchor_gate.py` 把 3 条 BotSrv 行点从 `REGISTERED` 移除，
新增 `TOOL_ANCHORS`（11 条 `(符号, 文件)`）+ `_tool_symbol_findings`（只机判符号逐字在文件里）；
`guild_castle_scenario.json` 的 `botsrv_primitives` 同步改为名字锚文本（无 `.cs:NNN`）。
⇒ 行号漂移不再打红，符号被改名/删除仍 RED（`selftest` 牙 + MUT6/MUT7）。

### B. 三锚门统一「读侧 token 强度」牙（⑧-10 残余「结束行只有 `}` 判据较弱」+ ⑧-6 O-1 可机判部分）

`castle_oracle_anchor_gate.py` / `guild_castle_anchor_gate.py` / `hero_pet_anchor_gate.py` 各新增判据：
每条锚**整组**至少要有一个**非纯标点** token（整组只剩 `}`/`;`/`(` 这类 ⇒ RED）。各门配一颗
`selftest` 牙 + 一颗 `mutation` 牙（castle MUT9 / guild MUT5 / hero_pet MUT5）。
「行内容 ⇄ 散文声称构造」的**语义层**仍诚实登记为抽样人核（见 ⑧-6 README 残留①）。

### C. 文档订正

* **⑧-6 O-2**：`evidence-m3-castle-anchor-fullcov-20261014/README.md` 的「改前锚数 = 83」与
  上一片机判的 **82** 冲突；本片复算 `git show 30c52f85d:...castle_oracle_anchor_gate.py` ⇒ 表项 = **82**
  （改后 159，净增 77）⇒ 订正为 82 并注明。
* **⑨-5（文案）**：`run_hero_pet_pair.ps1` 的 `scenario-fixed-input` 说明串补 `make_clone`。
* **⑨-5（文档）**：`evidence-m3-hero-pet-anchor-20261014/{README.md,00-commands.txt}` 的
  「101 suites 1199 passed」订正为 **1200**（合并树读数；`reviewer-5` 残余 Minor）。

## 读数（本片实跑，原始件见同目录）

| 项 | 读数 | 原始件 |
| --- | --- | --- |
| castle 锚门 | check GREEN 159 锚；selftest GREEN 9 牙；mutation MUT1–MUT9 GREEN | `01` |
| guild/castle 锚门 | check GREEN **31 锚 / 92 行点**；selftest GREEN 8 牙；mutation MUT1–MUT7 GREEN | `02` |
| hero/pet 锚门 | check GREEN 27 锚 / 57 行点；selftest GREEN 7 牙；mutation MUT1–MUT5 GREEN | `03` |
| 夹具链深检门（runner 级） | **FIXTURE_CHAIN GREEN: runner_ok=10 prereq_or_skip=0 red=0**（6 条主机门全 rc=0） | `04` |
| 修前 RED 复现 | origin/master 版锚门+场景对当前 `src/` ⇒ **RED 4 项 rc=1**（BotSrv 行漂移） | `05` |
| ⑧ 夹具 runner 无栈自检 | `SELFTEST ok=22/22` GREEN（含场景锚门 check/selftest/mutation） | `06` |
| ⑨ 夹具 runner 无栈自检 | `SELFTEST ok=23/23` GREEN（`scenario-fixed-input` 已含 `make_clone`） | `07` |
| 四门禁 | fmt rc=0 / clippy rc=0(-D warnings) / 101 suites 1200 passed 0 failed 22 ignored / msgcodegen 604 rc=0 | `08`–`11` |
| 证据四件套 | `verify --check-extra` GREEN（仓库 LF/git-blob 口径）+ `eolcheck` GREEN | 见 `SHA256SUMS.txt` |

## 边界

见 `12-boundary.txt`：base = `origin/master` `382eef5de`；净差量全在 `mir2-rs/tests/parity/m3/**`
（3 个锚门工具 + `guild_castle_scenario.json` + `run_hero_pet_pair.ps1` + 两个既有证据目录订正 + 本目录）；
冻结面 `src sql MirClient mir2-rs/crates/protocol mir2-rs/crates/storage`（排除已登记 `src/BotSrv/**`）= **空**；
`*.rs` 计数 = **0**。

## 残留（如实登记）

1. 锚门「行内容 ⇄ 散文声称构造」的**语义层**仍是抽样人核（本片只加了可机判的「非纯标点 token」牙）。
2. ⑧-6 / ⑧-10 / ⑨-5 三卡的**卡级真机 DoD**（两侧同序列 `diff=0`、`openmir2-m3-clock-injection` +
   owner 独占窗口）**不在本片范围**，卡保持 `in-progress`。
3. `evidence-m3-hero-pet-anchor-20261014/08-gate-test.txt` 为分支上原始转录（无汇总行），
   README/`00-commands.txt` 现标注为**合并树读数**（1200）。
