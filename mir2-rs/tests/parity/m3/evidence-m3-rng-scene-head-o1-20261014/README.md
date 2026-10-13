# evidence-m3-rng-scene-head-o1-20261014

卡：`openmir2-m3-rng-trace-scene-groups`（worktree `m3bag`，branch `wip/m3bag-anchor-teeth`）。
片：**reviewer-6 O-1 收口**（证据包「声明 head」字段订正 + 机判），tier=L、零 `.rs`、纯离线。

- patch base = **origin/master `30c52f85`**；本片净差量全在 `mir2-rs/tests/parity/m3/**`（见 `10-boundary.txt`）。
- 本片**不**改任何判据阈值/口径；**不**动 `crates/**`、`bins/**`、`src/**`、`tests/parity/world/**`。

## 1. 为什么（O-1）

reviewer-6 在 approve head `2591a7b31`（entry `2f19ee0b`）时记了一条 **O-1（observation，非阻塞）**：

> `evidence-m3-rng-scene-anchor-teeth-20261014/11-boundary.txt` 与 `README.md §5` 把本包的
> head 写成 **merge commit `2fcebc6cd`**，而**被交付/被审的 head 是 `2591a7b31`**（其上再一提交
> 「证据 05–11 在合并树重生成」）。实际影响为零（两者对 `origin/master..HEAD` 的路径集合同为 19，
> 判据/冻结面/零 `.rs` 结论不变），但证据头部引用的不是被交付/被审 head ⇒ 建议下一片把
> 05–11/README 的 head 字段重生成到交付 head，避免消费方按旧 oid 定位。

## 2. 本片改了什么

1. **订正书证**（`evidence-m3-rng-scene-anchor-teeth-20261014/`）：`11-boundary.txt` 的 `### head`
   行、`README.md §1/§5`、`05-selfcheck-nostack.txt`、`06-offline-regression.txt` 的 head 由
   `2fcebc6cd` 订正为 **`2591a7b31`**（= 交付/review head；`2fcebc6cd` 记为中间 merge commit）。
   before/after 见 `01-o1-before-after.txt`。
2. **新增机判** `rng-trace-scene/check_evidence_head.py`：把「证据包声明的 head 必须可解析、是当前
   HEAD 的祖先、且自其至 HEAD **只动证据/夹具侧**（缺省 `mir2-rs/tests/parity/m3/`）」收成
   fail-closed；`selftest` 7/7（4 条改坏必红：生产代码漂移 / oid 不可解析 / 非祖先 / 显式放行绿，
   + 2 条 PREREQ）。读数见 `02-gate-selftest-check.txt`。
   > 后续片（`check-pack`，见 `evidence-m3-rng-scene-head-pack-20261014/`）把该工具扩到 **selftest 16/16**，
   > 并新增「机判对象 = 包内声明 head + 显式 `--since` 锚」模式；本包 `10-boundary.txt` 已补
   > `### head = <oid>` 声明行（`check-pack --since 30c52f85` GREEN，changed_since=21）。
   - 在 `rng-trace-scene/README.md` 增 §5.4 说明（含**局限**：本门不咬「声明的是真实祖先但不是读数
     head」那一类 O-1 原形——那属书证命名，靠人写对 + 评审）。
3. **同片记录四门禁 + 边界 + 无栈自检**（`04`–`10`）。

## 3. 判据读数（真跑；原始件见同目录）

| 项 | 命令（cwd = `mir2-rs/tests/parity/m3`） | 读数 | 原始件 |
| --- | --- | --- | --- |
| 声明 head 机判 | `python -I rng-trace-scene/check_evidence_head.py selftest` / `check --dir evidence-m3-rng-scene-anchor-teeth-20261014 --head <base>` | **selftest 7/7**（工具现为 16/16，见 head-pack 包）；check GREEN（其后变更全证据侧） | `02-gate-selftest-check.txt` |
| 承接卡锚点保鲜 | `python -I rng-trace-scene/check_anchors.py check` | **GREEN 7/7**（未削弱） | `03-checks-ancillary.txt` |
| 无栈自检 | `run_rng_trace_pair.ps1 -SelfCheckNoStack` | **31/31 GREEN**（项数未变） | `04-selfcheck-nostack.txt` |
| 离线全链 | 8 个 selftest | 全 rc=0 | `05-offline-regression.txt` |
| 四门禁（`CARGO_TARGET_DIR=E:/tmp/m3bag-target`，cwd=`mir2-rs`） | fmt / clippy `-D warnings` / workspace test / msgcodegen | rc=0 / rc=0 / **101 suites 1200 passed 0 failed 22 ignored** / 604 rc=0 | `06`–`09` |
| 边界 | `git diff --name-only <base>..HEAD` | 全在 `mir2-rs/tests/parity/m3/**`；**零 `.rs`**、冻结面空 | `10-boundary.txt` |
| 证据 hash / EOL | `verify_sha256sums.py verify --check-extra` / `eolcheck` | GREEN / GREEN（仓库 LF/git-blob 口径） | `SHA256SUMS.txt` |

## 4. 纪律 / 边界

- **未 merge、未自签 review**；只在自己 worktree 写；共享主检出零改动；**零起栈 / 零占端口**。
- 未动 `crates/**`、`bins/**`、`src/**`、`tests/parity/world/**`、drop-exp 实现。
- 卡级真机 DoD（同场景含怪两相 trace + `groupdiff --fold --strict` 的 site/order diff=0）仍
  **R / not_verified**，阻塞 = 承接卡 `openmir2-m3-rng-scene-driver` 未开 + owner 独占窗口；
  本片**不得**据此收卡。
