# evidence-m3-rng-scene-head-pack-20261014

卡：`openmir2-m3-rng-trace-scene-groups`（worktree `m3bag`，branch `wip/m3bag-anchor-teeth`）。
片：**reviewer-5 O-1 收口 —— `check_evidence_head.py` 新增 `check-pack` 模式（机判对象 = 包内声明的 head + 显式 `--since` 锚）**，
tier=L、零 `.rs`、纯离线。

- patch base = **origin/master `61f613702`**（本卡上一片 `9740b7d0c` 已合入 master `61f613702`）。
- 净差量全在 `mir2-rs/tests/parity/m3/**`（见 `10-boundary.txt`）；**零** `crates/**`、`bins/**`、`src/**`、`tests/parity/world/**`。

## 1. 为什么（reviewer-5 对 head `9740b7d0c` 的 approve 携带的 observation O-1）

见 `01-observation-o1.txt`（原文）。要点：上一片的 `check_evidence_head.py` 只有一个 `check` 子命令，
用法要求调用方**另传**一个 `--head` 当"声明 head"；O-1 片的证据里实际传的是**收口 patch 的 base
`30c52f85d`**，于是机判证明的是"本片边界全在 `mir2-rs/tests/parity/m3/**`"，而**不是**被审包自己声明的
head（`2591a7b31`）。⇒ 建议下一片加一个「从包内读 declared head + 显式 since 锚」的模式，使
**书证 head 与机判对象一致**。

## 2. 本片改了什么（零 `.rs`）

1. **`rng-trace-scene/check_evidence_head.py` 新增 `check-pack` 子命令**：
   - **机判对象 = 包内声明的 head**：从包内 `11-boundary.txt` → `10-boundary.txt` → `README.md` 依序
     探测 `head = <oid>` 声明（`--decl-file` / `--decl-regex` 可覆写）；读不到 ⇒ `PREREQ` rc=2。
   - **显式 `--since` 锚**：差量取 `since..声明 head`（而非 `声明 head..HEAD`），只允许证据/夹具侧路径；
     这样 master 前进后并入的**无关生产改动不再造成假红**，同时校的正是书证 oid。
   - fail-closed 逐条点名：`HEAD_UNRESOLVABLE` / `SINCE_NOT_ANCESTOR` / `HEAD_NOT_ANCESTOR` /
     `HEAD_SCOPE_DRIFT` ⇒ rc=1；缺目录/缺声明/坏输入/非 git 工作树 ⇒ rc=2。
   - 顺带让 `check`/`check-pack` 在 CLI 层**打印**各自的问题行（此前只打印 `HEAD_SCOPE_DRIFT` 那一类）。
   - `selftest` **7/7 → 16/16**（`check` 7 + `check-pack` 9；新增 4 条改坏必红 + 3 条 PREREQ + 2 条绿向）。
2. **`evidence-m3-rng-scene-head-o1-20261014/10-boundary.txt` 补 `### head = <oid>` 声明行**（此前该包只声明
   base，没有 head 声明 ⇒ 新门会对它 `PREREQ`）；其 `README.md` 加一行后续片注记（工具现为 16/16）。
3. **`rng-trace-scene/README.md` §5.4 / §6** 增 `check-pack` 用法与残留局限说明。

## 3. 判据读数（真跑；原始件见同目录）

| 项 | 命令（cwd = `mir2-rs/tests/parity/m3`） | 读数 | 原始件 |
| --- | --- | --- | --- |
| 工具自检 | `check_evidence_head.py selftest` | **16/16 GREEN**（含改坏必红 + PREREQ） | `02-gate-selftest.txt` |
| check-pack GREEN | `check-pack --dir ...anchor-teeth... --since 10f0daa93` | **GREEN**，declared=`2591a7b31`，changed=19 全证据侧 | `03-check-pack-green.txt` |
| check-pack GREEN | `check-pack --dir ...head-o1... --since 30c52f85` | **GREEN**，declared=`9740b7d0c`，changed=21 全证据侧 | `03-check-pack-green.txt` |
| 改坏必红 ×4 + PREREQ | 临时声明件（head 夹带生产改动 / 不可解析 / since 取后代 / 非 HEAD 祖先 / 无声明） | rc=1,1,1,1；rc=2 | `04-check-pack-mutation-red.txt` |
| 离线全链 | 8 个 selftest + `check_anchors check` + `check_window_batch check` | 全 rc=0（`fixture_chain_gate` 31/31） | `05-offline-regression.txt` |
| 四门禁（`CARGO_TARGET_DIR=E:/tmp/m3bag-target`，cwd=`mir2-rs`） | fmt / clippy `-D warnings` / workspace test / msgcodegen | rc=0 / rc=0 / **101 suites 1200 passed 0 failed 22 ignored** / 604 rc=0 | `06`–`09` |
| 边界 | `git diff --name-only <base>..HEAD` | 全在 `mir2-rs/tests/parity/m3/**`；**零 `.rs`**、冻结面空 | `10-boundary.txt` |
| 证据 hash / EOL | `verify_sha256sums.py verify --check-extra` / `eolcheck` | GREEN / GREEN（仓库 LF/git-blob 口径） | `SHA256SUMS.txt` |

## 4. 局限（如实登记）

`check-pack` 把**机判对象**换成了书证里写的 head，且用显式 `--since` 锚免疫"master 前进后的无关合并"。
它咬住三类声明错误：**不可解析 / 非 HEAD 祖先 / 自 since 起夹带生产代码**。它**仍咬不住** O-1 的
**原形**——把「证据侧的**其它祖先**」（例如中间 merge commit `2fcebc6cd`）当 head 写；实测
`git diff 10f0daa93..2fcebc6cd` 全在 `mir2-rs/tests/parity/m3/**`（0 条越界）⇒ 对同一 `since` 锚本门亦绿。
那一类属「书证命名」，只能靠人写对 + 评审；本门的作用是让**其余可机判**的相邻失败模式各自点名，
且不再把机判对象错位到 base 上（这是 O-1 意见的正文）。

## 5. 纪律 / 边界

- **未 merge、未自签 review**；只在自己 worktree 写；隔离 `CARGO_TARGET_DIR=E:/tmp/m3bag-target`。
- **零起栈 / 零占端口**（起门禁编译前实拍十端口：仅共享 3306）；共享主检出 `E:\Users\gxh\Documents\GitHub\OpenMir2` 零改动。
- 未动 `crates/**`、`bins/**`、`src/**`、`tests/parity/world/**`、drop-exp 实现。
- 卡级真机 DoD（同场景含怪两相 trace + `groupdiff --fold --strict` 的 site/order diff=0）仍
  **R / not_verified**，阻塞 = 承接卡 `openmir2-m3-rng-scene-driver` 未开 + owner 独占窗口；
  本片**不得**据此收卡。
