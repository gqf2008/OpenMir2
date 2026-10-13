# evidence-m3-rng-scene-anchor-teeth-20261014

卡：`openmir2-m3-rng-trace-scene-groups`（worktree `m3bag`，branch `wip/m3bag-anchor-teeth`）。

本 head 的 patch base = master `1cf88f6ad`；它落在**上一片 `f11129a84`（锚点覆盖面收口）之上** ⇒
净差量与 review 面 = 上一片 18 路径 + 本片 11 路径（本片即上一片 + 3 条新牙 + 证据实路径订正）。

本片：**收口 reviewer-6 对 `f11129a84`（锚点覆盖面收口片）记的两条非阻塞 Minor（M-1 / M-2）**。

## 1. 为什么

reviewer-6 在 approve `f11129a84`（entry `31c7e48b…`）时记了：

- **M-1（Minor）**：`check_anchors.py` 的 `check_doc_text` ③ 段里 `BARE_REF_UNGUARDED` /
  `BARE_REF_FILE_MISSING` / `BARE_REF_OUT_OF_RANGE` **三条 fail-closed 分支没有活 selftest 用例**
  （`mutation-bare-ref-undeclared-red` / `mutation-bare-ref-content-drift-red` 只覆盖到
  `BARE_REF_UNDECLARED` / `BARE_REF_DRIFT`）。fail-closed 方向未受损，但「牙没被咬过」⇒ 无法证明
  这三条分支在被触发时真会红。建议后续片补改坏样本。
- **M-2（Minor）**：`evidence-m3-rng-scene-anchor-coverage-20261014/01,02` 的命令把 doc 路径写成
  占位 `<...>`（如 `<同名 basename、别的目录（foo/monai.rs:999）>`）⇒ 不能直接复制复跑；reviewer 自己
  用具体 `--doc` 复现出同结果。建议后续片在证据里写实路径。
- M-3（Minor）：分支 in-flight 有两个同题提交（`184e73c67` + `f11129a84`，夹一次 origin/master
  merge），**非缺陷**（净差量自洽、`a173eba89` 仍是 head 祖先、merge-base=master）⇒ **无需动作**，
  本片如实登记。

## 2. 本片改了什么（零 `.rs`）

- `rng-trace-scene/check_anchors.py`：`selftest` 补 **3 条改坏必红**（10/10 → **13/13**，改坏必红
  9 → **12** 条），三条分支各咬一次：
  - `mutation-bare-ref-unguarded-red`：同行裸续引用归属的**完整引用本身未登记锚点**（`merchant.rs:999`、`:12`）
    ⇒ `BARE_REF_UNGUARDED`；
  - `mutation-bare-ref-file-missing-red`：注入一条 `path` 指向不存在文件的锚点 + 其 `also` 登记行
    ⇒ `BARE_REF_FILE_MISSING`；
  - `mutation-bare-ref-out-of-range-red`：已登记的 `also` 行号改成 `99999`（超出文件行数）并把文档裸续
    引用同步改 `:99999` ⇒ `BARE_REF_OUT_OF_RANGE`。
- `rng-trace-scene/README.md`：§5.2 读数 10/10 → 13/13；新增 **§5.2.2**（三条分支的活用例补齐 +
  M-2 实路径说明）；§6 命令注释同步。
- `rng-trace-scene/DRIVER-CONTRACT.md`：§6 的 `selftest` 计数 10/10 → 13/13（含三条分支活用例）。
- `evidence-m3-rng-scene-anchor-coverage-20261014/01,02`：M-2 —— 命令由占位 `<...>` 改为**实路径 +
  构造命令**（`git worktree add a173eba89` 取旧工具、`cp/printf/sed` 造坏样本），并重记该包 `SHA256SUMS.txt`。

**不改**任何判据阈值/口径；**不改** `run_rng_trace_pair.ps1` 的自检项数（仍 **31/31**：`check_anchors
selftest` 作为一个子项被调用，其 GREEN 正则只认「分子==分母」，不硬编码条数）。

## 3. 判据读数（真跑）

| 项 | 命令（cwd = `mir2-rs/tests/parity/m3`） | 读数 | 原始件 |
| --- | --- | --- | --- |
| 机判自检 | `python -I rng-trace-scene/check_anchors.py selftest` | **13/13 GREEN**（含 12 条改坏必红） | `01-selftest-13.txt` |
| 三条分支独立构造（不经 selftest） | `python -I E:/tmp/m3bag-teeth-firings.py`（构造见 `02-…` 头部） | 三条分别点名 `BARE_REF_UNGUARDED` / `BARE_REF_FILE_MISSING` / `BARE_REF_OUT_OF_RANGE` | `02-branch-firings.txt` |
| 真身 | `python -I rng-trace-scene/check_anchors.py check` | **GREEN 7/7**（文档锚点未变，未削弱） | `03`（旧包） |
| 无栈自检 | `run_rng_trace_pair.ps1 -SelfCheckNoStack` | **31/31 GREEN**（`anchors-freshness-selftest` 真接线，项数未变） | `05-selfcheck-nostack.txt` |
| 离线全链 | 9 个 selftest | 全 rc=0 | `06-offline-regression.txt` |
| 四门禁（`CARGO_TARGET_DIR=E:/tmp/m3bag-target`） | fmt / clippy `-D warnings` / workspace test / msgcodegen | rc=0 / rc=0 / **101 suites 1200 passed 0 failed 22 ignored** / 604 rc=0 | `07`–`10` |
| 边界 | `git diff --name-status <base>..HEAD` | 见 `11-boundary.txt`；**零 `.rs`**、冻结面 `src/ sql/ MirClient mir2-rs/crates/protocol mir2-rs/crates/storage mir2-rs/bins` 空 | `11-boundary.txt` |
| 证据 hash / EOL | `verify_sha256sums.py verify <本包+旧包> --check-extra` / `eolcheck` | GREEN / GREEN（仓库 LF/git-blob 口径） | `SHA256SUMS.txt` |

## 4. 纪律 / 边界

- **未 merge、未自签 review**；只在自己 worktree 写；共享主检出
  `E:\Users\gxh\Documents\GitHub\OpenMir2` 零改动；**零起栈 / 零占端口**（仅共享 MySQL 3306）。
- 未动 `crates/world`、`bins/**`、`src/**`、`tests/parity/world/**`、drop-exp 实现。
- **本片方向是「加严 + 补牙」**：`ANCHORS` 表未改、原 5 条判据分支未改，只增 3 条 selftest 用例与文档
  计数；不存在任何判据放宽。
- 卡级真机 DoD 仍阻塞：承接卡 `openmir2-m3-rng-scene-driver` 未开 + owner 独占窗口（与本卡两相 trace、
  ④ 真机 100 杀同批次串行）。本片**不得**据此收卡。
