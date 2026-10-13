# evidence-m3-anchor-prefix-gate-20261014 —— 锚门「路径前缀」判据（链深检门残余 O-1）

本证据包对应卡 `openmir2-m3-fixture-chain-gate`（owner_hint = `openmir2-worker-5` / m3cs），
收口 reviewer-6 在 approve `f98f56f3`（`@reviewed_head 003b229f`）登记的**非阻塞 Observation O-1**：

> 引用按文件**基名**解析：带错误目录前缀的锚（如 `src/Evil/Messages.cs:62`）会被解析成已登记的
> `Messages.cs:62` 而通过覆盖面。与既有 ⑧/⑨/⑩ 四锚门同口径，本片未引入；读侧 token 仍钉冻结行，风险低。

采集环境：worktree `.claude/worktrees/m3cs`，branch `worktree-m3-anchor-prefix-gate`，
base `origin/master` = `7df340fe7`（⑧/⑨/⑩ 全部夹具族已合入）。**零起栈、零占端口。**

## 改了什么（全部在 `mir2-rs/tests/parity/**`，零 `.rs`）

1. **五个场景锚门**（`quest_anchor_gate.py` / `party_trade_anchor_gate.py` /
   `guild_castle_anchor_gate.py` / `hero_pet_anchor_gate.py` / `selloff_anchor_gate.py`）
   新增**路径前缀判据**：锚文本里若显式写了目录前缀（每个目录段含字母），该前缀必须是
   `PATHS[基名]` 登记路径的**路径后缀**；只写基名仍按既有口径（不误伤）。
   - 新增纯函数 `_path_prefix_findings()` + `_PATHREF_RE`（独立于原 `FILE_RE`，不改变覆盖面口径）。
   - 目录段要求含字母 ⇒ 不会把 `:3966-4105/File.cs` 的 spec 区间误当目录前缀。
   - 每个门在 `cmd_mutation` 增一条 `MUT-O1` 牙：错前缀（`src/Evil/<基名>:1`）必 RED、
     正确前缀与裸基名必须不产生 finding（可失败向 + 不误伤双向）。
2. **`whitelist.md`**（顺带订正两处陈旧读数，均为既有 reviewer 非阻塞登记）：
   - T-41 的 `npc_shop_diff.py` 比较器 selftest 计数 `27/27` → **`32/32`**（reviewer-5 在
     ⑤-6 `bf8d4e03` approve 的 F-1；实测 = 32/32）。
   - 移除 T-42 与 T-43 两行之间的空行（reviewer-6 的 **O-3**：markdown 表格被拆成多张表）。

**未动**：`castle_oracle_anchor_gate.py`（⑧-6 工作表锚门）——它的锚写在 `castle.rs` 注释块里、
以**裸基名** `File.cs:NNN` 形书写，不存在目录前缀可校验；本片不改它的解析口径，避免与「工作表锚」
合同混淆。

## 判据读数（件）

| 文件 | 内容 |
| --- | --- |
| `01-boundary.txt` | 边界：`git diff --name-status origin/master` + 冻结面/`*.rs`/`*.toml`/`*.ps1` 空 |
| `10-<gate>-check/selftest/mutation.txt` | 五门各自的 `check`（真身锚 GREEN）、`selftest`（GREEN）、`mutation`（GREEN，含 `MUT-O1 … RED`） |
| `20-scenario-badprefix-red.txt` | **改坏必红①**：真场景 `quest_scenario.json` 一条锚前缀 `src/OpenMir2/`→`src/Evil/` ⇒ `check` RED 点名「锚路径前缀」；还原后 sha256 逐字节一致 |
| `21-tool-neutered-mutation-red.txt` | **改坏必红②**：把 `_path_prefix_findings` 首行插 `return []`（等于删判据）⇒ `mutation` RED 且 `MUT-O1` 显示 GREEN（证明牙可失败向）；还原后 sha256 逐字节一致 |
| `30-chain-gate-require-all.txt` | `fixture_chain_gate.py check --require-all` = **FIXTURE_CHAIN GREEN runner_ok=10 prereq_or_skip=0 red=0** |
| `32-chain-gate-selftest.txt` | 链深检门 `selftest` = **37/37** |
| `31-fixture-matrix-require-all.txt` | `fixture_matrix.py check --require-all` = **ready=10 pending=0 red=0** |
| `40/41/43/44` | 四门禁：`fmt` rc=0、`clippy -D warnings` rc=0、`test` 101 suites（逐行 `test result: ok … 0 failed`）、`msgcodegen --check` 604 rc=0 |
| `50-whitelist-diff.txt` | `whitelist.md` 两处订正的 diff |

## 口径

- **只读**：`check`/`selftest`/`mutation` 只读仓库文件 + 比较器自带 selftest，**不写任何仓库文件、不起栈、不占端口**。
- **真身未被放宽**：新增判据只**追加**，不改覆盖面/读侧 token/token 强度/tool 锚的任何既有判据；
  五门真身 `check` 在新判据下仍全 GREEN（159/31/27/14/22 锚）。
- **tier = L**：纯离线夹具/工具（`*.rs=0`、冻结面空；不触 RNG 抽数序列/时序/落库/战斗数值）⇒ 等价即放行 + 抽样冒烟。

## 残留

- 卡级真机 DoD（各 READY 夹具 `run_*.ps1` 两侧同序列 `diff=0`）仍待世界侧实现卡 +
  `openmir2-m3-clock-injection` + owner 独占 `-Go` 窗口；本片不据此收卡。
- 本片不改 `castle_oracle_anchor_gate.py`（工作表锚，见上）。
