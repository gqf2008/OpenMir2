# ⑧-10 残余 Minor 收口：行会/攻城夹具锚门接入夹具链深检门 —— 证据包

- 卡：`openmir2-m3-guild-castle-fixture-botsrv`（M3⑧-实现 10；关该卡 merge 检查点里携带的
  非阻塞 Minor「**`guild_castle_anchor_gate.py` 未注册进 `fixture_chain_gate.py` 常跑**」）
- 关联卡：`openmir2-m3-fixture-chain-gate`（夹具链深检门本体；本片只把它的 `SHARED_CHECKS` 再补两项）
- 分支：`worktree-m3-guild-castle-chain-gate`
- base：`origin/master 1cf88f6ad`（本 worktree 起始 HEAD）
- tier：**L**（纯离线夹具/工具片：零 `.rs` 差量、零生产码、零起栈；无 RNG/时序/落库/战斗数值）
- 写集：`mir2-rs/tests/parity/m3/fixture_chain_gate.py` + 本证据目录。
  零 `src/ sql/ MirClient`、零 `crates/protocol`、零 `crates/storage`、零 `bins/`、零 `*.rs`。

## 1. 本片做了什么

⑧-10 合并检查点（merge `f51849a0b`，approve reviewer-6 `8ffcdd2c` @`8d19d4f0`）如实登记了 2 条
非阻塞 Minor 随卡带走，其中一条是：

> 「`guild_castle_anchor_gate.py` 接在夹具自身 `run_guild_castle_pair.ps1 -SelfCheckNoStack`，
> **未注册进 `fixture_chain_gate.py` 常跑**；作者 comment 说明原因是与在审的 ⑧-6 O-1 片
> 改同一 `SHARED_CHECKS` 区、按写集纪律不并行改。」

⑧-6 的 O-1 片（`c17171aa1`）已合入 master ⇒ **写集冲突已消失**，本片把这条 Minor 收口：

**`fixture_chain_gate.py`**
1. `SHARED_CHECKS` 新增两条主机门（与既有 `castle_anchor_selftest` / `castle_anchor_check` 同形）：
   - `guild_castle_anchor_selftest` → `guild_castle_anchor_gate.py selftest`
   - `guild_castle_anchor_check` → `guild_castle_anchor_gate.py check`
2. `selftest` 新增 4 条牙齿（26/26 → **30/30**）：
   - 「行会/攻城锚门已接入共享管线（⑧-10 残余 Minor，防回退）」：断言 `(file, ["selftest"])`
     与 `(file, ["check"])` 两向都在 `SHARED_CHECKS` 里 —— 防止有人把它又挪回「只在夹具 runner 内部」；
   - 「主机门（.py）登记项 ≥4」；
   - 「共享管线登记名唯一」「共享管线 `(file,args)` 登记唯一」（防重复注册偷懒/名字撞车）。

**为什么值得登记（不是重复劳动）**：该锚门此前只被 `run_guild_castle_pair.ps1 -SelfCheckNoStack`
间接跑到，而链深检门**只在系统 8 状态为 `READY` 时才跑**该系统 runner ⇒ 一旦 ⑧ 夹具三件套缺件
（`PENDING`）或半拉子（`INCONSISTENT`），锚门就只剩「手工读数」；登记进 `SHARED_CHECKS` 后它
**不看矩阵状态、独立常跑**（在场却坏 ⇒ 恒 RED）。这与 ⑧-6 O-1 对 `castle_oracle_anchor_gate.py`
的处置完全同口径，两条锚门现在同等受保护。

## 2. 判据（tier=L：等价 + 抽样冒烟）

| 面 | 命令 | 读数（原始件） |
| --- | --- | --- |
| 等价 | `git diff --name-only origin/master -- '*.rs'` | **空**（零 `.rs`）——`09` |
| 等价 | 冻结面 `git diff --name-only origin/master -- src sql MirClient mir2-rs/crates/protocol mir2-rs/crates/storage mir2-rs/bins` | **空** ——`09` |
| 抽样冒烟（直接门） | `python -I guild_castle_anchor_gate.py check` | **GREEN**（34 锚 / 97 行点命中冻结 `src/` 且覆盖面闭合）rc=0 ——`01` |
| 抽样冒烟（直接门） | `python -I guild_castle_anchor_gate.py selftest` | **SELFTEST GREEN** rc=0 ——`01` |
| 抽样冒烟（链深检门） | `python -I fixture_chain_gate.py selftest` | **SELFTEST GREEN 30/30** rc=0 ——`02` |
| 抽样冒烟（链深检门） | `python -I fixture_chain_gate.py check` | 两条新登记 `guild_castle_anchor_selftest`/`_check` 均 `present rc=0`；**唯一 RED = 系统 B**（他卡，见 §4）——`03` |
| **改坏必红** | 场景锚 `UserCastle.cs:311-446` → `:311-448`（历史漂移值） | 直接门 rc=1；链深检门 rc=1 且**点名** `guild_castle_anchor_check`/`_selftest`（red=4）；`cp` 还原后 `git status` 干净 ——`04` |
| 四门禁 | fmt / clippy(-D warnings) / test / msgcodegen | rc=0 / rc=0 / **101 suites 1200 passed 0 failed 22 ignored** / rc=0 604 常量 ——`05`–`08` |

## 3. 证据清单

| 文件 | 内容 |
| --- | --- |
| `00-commands.txt` | 本包全部命令（含 base/branch/tier/写集口径） |
| `01-guild-castle-anchor-gate.txt` | `guild_castle_anchor_gate.py check` + `selftest` 实跑 |
| `02-fixture-chain-gate-selftest.txt` | 链深检门 `selftest` 30/30（原 26 + 本片 4 条牙齿） |
| `03-fixture-chain-gate-check.txt` | 链深检门 `check` 全表（含 `-- 共享管线 / 主机门 --`，两条新登记 rc=0） |
| `04-redcheck.txt` | 改坏必红：直接门 rc=1 + 链深检门 rc=1 点名两条新登记 |
| `05-gate-fmt.txt` / `06-gate-clippy.txt` / `07-gate-test.txt` / `08-gate-msgcodegen.txt` | 四门禁原始读数 |
| `09-boundary.txt` | 净差量（1 文件 + 本证据目录）、冻结面空、`*.rs` 空 |
| `10-hash-check.txt` | `verify --check-extra` + `eolcheck` 绿 |
| `SHA256SUMS.txt` | 本目录清单（仓库 LF/git-blob 口径，由 `verify_sha256sums.py write` 生成） |

## 4. 未做 / 如实登记（勿重复劳动）

- **另一条 ⑧-10 Minor「读侧结束行 token 判据较弱」未改**：reviewer-6 已核该弱判据被「覆盖面
  双向」牙齿兜住（`MUT1/MUT2` 与自造变异都因覆盖面判 RED，实际不产生漏检）⇒ 本片不动，避免为
  一个已被覆盖的弱判据改动锚表语义。
- **⑨-5 的 `hero_pet_anchor_gate.py` 暂不能登记**：它只存在于在审片
  `worktree-m3-hero-pet-fixture-minors`（patch head `3f927bf6`），**master 里还没有该文件** ⇒
  现在登记会让 `SHARED_CHECKS` 解析缺件（`selftest` 的「件在本仓库树内可解析」牙 + `--require-all`
  判红）。**该登记作为 ⑨-5 片合入后的 follow-up**（同卡随身走的 Minor）。
- **`fixture_chain_gate.py check` 当前 RED 不是本片引入**：**系统 B「全服 RNG 消费者账本（Claim B）」**
  的 runner `-SelfCheckNoStack` rc=1（`anchors-freshness-selftest`：`DRIVER-CONTRACT.md` 写
  `mir2-rs/bins/game-svr/src/main.rs:5440`，实测首个匹配已在 `:5488` ⇒ 宿主 AI 调用点行号漂移）。
  该 runner/文档属 **`openmir2-m3-rng-trace-scene-groups`**（owner `openmir2-worker-4`，本刻在【待审】），
  **不是本片写集**，本片不修、不登记白名单；`--skip-shared` 与四门禁均不受影响。
- 本片**不收 ⑧-10 卡**：该卡 DoD②③（真机两侧 `run_guild_castle_pair.ps1` 同序列 diff=0）仍需
  实现卡 + `openmir2-m3-clock-injection` + owner 独占窗口。

## 5. 纪律 / 残留物

- 只在本 worktree（`m3cs`）写/提交/推；**共享主检出 `E:/Users/gxh/Documents/GitHub/OpenMir2` 未动**。
- **零起栈、零占端口**；编译用检出外 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target`。
- 未自签 review、未 merge。
- 临时物（不入库，均在 `E:/tmp/m3cs-work/`）：`fcg-after.txt` / `fcg-after.json` /
  `fcg-mutation.txt`（本包 `04` 的链深检门段原样取自它）、`gate-*.log`（本包 `05`–`08` 来源）；
  另有 `E:/tmp/gcs.bak`（改坏必红前的场景锚备份，已还原）。
