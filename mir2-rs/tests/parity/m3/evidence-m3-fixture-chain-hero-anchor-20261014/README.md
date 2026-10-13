# ⑨-5 残余 Minor 收口（续片）：英雄/宠物夹具锚门接入夹具链深检门 —— 证据包

- 卡：`openmir2-m3-fixture-chain-gate`（M3 夹具链深检门 runner 级伴生门）
  - 前片（同卡、同文件）：⑧-10 残余 Minor「`guild_castle_anchor_gate.py` 未注册进
    `fixture_chain_gate.py` 常跑」 ⇒ patch `b3525404` @ head `67e77e496`
  - **本片**：⑨-5 残余 Minor「`hero_pet_anchor_gate.py` 未注册进 `fixture_chain_gate.py` 常跑」
    ⇒ 在同一分支 **叠加提交**（不改写已推送的 `67e77e496`）
- 关联卡：`openmir2-m3-hero-pet-fixture-botsrv`（⑨-5 夹具；其收口片 `3f927bf6` 已随 master
  `f0b333c96` 合入 ⇒ master 里才有 `hero_pet_anchor_gate.py`，本登记才可能）
- 分支：`worktree-m3-guild-castle-chain-gate`
- base：`origin/master 10f0daa93`（并入 ⑨-5 收口片 `f0b333c96`）
- tier：**L**（纯离线夹具/工具片：零 `.rs` 差量、零生产码、零起栈；无 RNG/时序/落库/战斗数值）
- 写集：`mir2-rs/tests/parity/m3/fixture_chain_gate.py` + 本证据目录。
  零 `src/ sql/ MirClient`、零 `crates/protocol`、零 `crates/storage`、零 `bins/`、零 `*.rs`。

## 1. 本片做了什么

⑨-5 收口片（master `f0b333c96`）新增了 `hero_pet_anchor_gate.py`（英雄/宠物场景锚门，与
`guild_castle_anchor_gate.py` / `castle_oracle_anchor_gate.py` 同形）。它与 ⑧-10 的锚门有
**同一个结构性缺陷**：只被自身夹具 runner `run_hero_pet_pair.ps1 -SelfCheckNoStack` 跑到，
而 `fixture_chain_gate.py` **只在系统 9 状态为 `READY` 时才跑该 runner** ⇒ 一旦 ⑨ 夹具三件套
缺件（`PENDING`）或半拉子（`INCONSISTENT`），锚门就只剩「手工读数」。

**`fixture_chain_gate.py`**
1. `SHARED_CHECKS` 新增两条主机门（与既有 `castle_anchor_*` / `guild_castle_anchor_*` 同形）：
   - `hero_pet_anchor_selftest` → `hero_pet_anchor_gate.py selftest`
   - `hero_pet_anchor_check`    → `hero_pet_anchor_gate.py check`
2. `selftest` 新增 1 条牙齿（30/30 → **31/31**）：
   - 「英雄/宠物锚门已接入共享管线（⑨-5 残余 Minor，防回退）」：断言 `(file, ["selftest"])`
     与 `(file, ["check"])` 两向都在 `SHARED_CHECKS` 里 —— 防止有人把它又挪回「只在夹具 runner
     内部」；
   - 既有的「主机门（.py）登记项 ≥4」随之收紧为 **≥6**（castle + guild_castle + hero_pet 各
     selftest/check），使「少登记一族」直接变红。

**为什么值得登记**：登记进 `SHARED_CHECKS` 后，锚门**不看矩阵状态、独立常跑**（在场却坏 ⇒ 恒
RED）。三条锚门现在同等受保护。这与 ⑧-6 O-1 对 `castle_oracle_anchor_gate.py` 的处置完全同口径。

## 2. 判据（tier=L：等价 + 抽样冒烟）

| 面 | 命令 | 读数（原始件） |
| --- | --- | --- |
| 等价 | `git diff --name-only origin/master -- '*.rs'` | **空**（零 `.rs`）——`09` |
| 等价 | 冻结面 `git diff --name-only origin/master -- src sql MirClient mir2-rs/crates/protocol mir2-rs/crates/storage mir2-rs/bins` | **空** ——`09` |
| 抽样冒烟（直接门） | `python -I hero_pet_anchor_gate.py check` | **GREEN**（27 锚 / 57 行点命中冻结 `src/` 且覆盖面闭合）rc=0 ——`01` |
| 抽样冒烟（直接门） | `python -I hero_pet_anchor_gate.py selftest` | **SELFTEST GREEN** rc=0 ——`01` |
| 抽样冒烟（链深检门） | `python -I fixture_chain_gate.py selftest` | **SELFTEST GREEN 31/31**（前片 30 + 本片 1）rc=0 ——`02` |
| 抽样冒烟（链深检门） | `python -I fixture_chain_gate.py check` | **FIXTURE_CHAIN GREEN**（runner_ok=10 / red=0）；`hero_pet_anchor_selftest`/`_check` 两条 `present rc=0`；6 条 `.py` 主机门全绿 ——`03` |
| **改坏必红 A** | 场景锚 `PlayObject.Base.cs:361-371` → 漂移值 `:361-370` | 直接门 rc=1（覆盖面：未登记 + 表过期）；链深检门 rc=1 且**点名** `hero_pet_anchor_check`/`_selftest`（red=3）；`cp` 还原后 `git status` 干净 ——`04` |
| **改坏必红 B** | 删掉本片新增的 hero_pet 两条登记 | `fixture_chain_gate.py selftest` rc=1，新牙两项 FAIL（29/31）；`cp` 还原 ——`04` |
| 四门禁 | fmt / clippy(-D warnings) / test / msgcodegen | rc=0 / rc=0 / **101 suites 1200 passed 0 failed 22 ignored** / rc=0 604 常量 ——`05`–`08` |

## 3. 证据清单

| 文件 | 内容 |
| --- | --- |
| `00-commands.txt` | 本包全部命令（含 base/branch/tier/写集口径） |
| `01-hero-pet-anchor-gate.txt` | `hero_pet_anchor_gate.py check` + `selftest` 实跑 |
| `02-fixture-chain-gate-selftest.txt` | 链深检门 `selftest` 31/31（前片 30 + 本片 1 条牙齿） |
| `03-fixture-chain-gate-check.txt` | 链深检门 `check` 全表（含 `-- 共享管线 / 主机门 --`，两条新登记 rc=0） |
| `04-redcheck.txt` | 改坏必红 A（场景锚 → 直接门 rc=1 + 链深检门 rc=1 点名两条新登记）+ B（删登记 → selftest 新牙 FAIL） |
| `05-gate-fmt.txt` / `06-gate-clippy.txt` / `07-gate-test.txt` / `08-gate-msgcodegen.txt` | 四门禁原始读数 |
| `09-boundary.txt` | 净差量（1 工具文件 + 两证据目录）、冻结面空、`*.rs` 空 |
| `10-hash-check.txt` | `verify --check-extra` + `eolcheck` 绿 |
| `SHA256SUMS.txt` | 本目录清单（仓库 LF/git-blob 口径，由 `verify_sha256sums.py write` 生成） |

## 4. 未做 / 如实登记（勿重复劳动）

- **本片不改 `hero_pet_anchor_gate.py` 本体**：它的锚表/牙齿是 ⑨-5 收口片的交付（已 review +
  合入 `f0b333c96`）；本片只做「接入共享管线」这一件事，避免触碰已审内容。
- **前片（guild_castle 登记）随本 head 一并交付**：本 head = `67e77e496` + 本片叠加提交，
  两族锚门（guild_castle / hero_pet）都在 `SHARED_CHECKS` 里。前片的证据目录
  `evidence-m3-fixture-chain-guild-anchor-20261014/` 仍在（其读数对该提交仍成立）；被 supersede
  的是前片的 **patch 条目**（`b3525404` @ `67e77e496`），不是证据。
- **`fixture_chain_gate.py check` 本刻已全绿**（`runner_ok=10 prereq_or_skip=0 red=0`）：前片
  交付时唯一 RED「系统 B `anchors-freshness`」已由 `openmir2-m3-rng-trace-scene-groups` 的
  `10f0daa93` 修入 master。
- **另两条 ⑧-10 Minor 未改**：①「读侧结束行 token 判据较弱」（reviewer-6 已核被覆盖面双向牙
  齿兜住，不产生漏检）；② 本片与 ⑧-10 都未收 ⑧/⑨ 夹具卡 —— 真机两侧同序列 diff=0 仍需
  实现卡 + `openmir2-m3-clock-injection` + owner 独占窗口。
- **未自签 review、未 merge**；后续同类登记（若有第三条锚门）应走同一卡的同口径。

## 5. 纪律 / 残留物

- 只在本 worktree（`m3cs`）写/提交/推；**共享主检出 `E:/Users/gxh/Documents/GitHub/OpenMir2` 未动**。
- **零起栈、零占端口**；编译用检出外 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target`。
- 临时物（不入库，均在 `E:/tmp/m3cs-ev-raw/`）：`01.txt`–`08.txt`（本包 `01`–`08` 来源）、
  `04b.txt`（改坏必红 B）、`bnd.txt`（`09` 来源）；另有 `E:/tmp/hps.bak`（改坏必红 A 前的
  `hero_pet_scenario.json` 备份，已还原）、`E:/tmp/fcg.bak`（改坏必红 B 前的
  `fixture_chain_gate.py` 备份，已还原）。
