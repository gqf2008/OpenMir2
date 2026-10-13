# evidence-m3-rng-scene-anchor-coverage-20261014

卡：`openmir2-m3-rng-trace-scene-groups`（worktree `m3bag`，branch `wip/m3bag-window-batch-premise`）。

> **后续片注（2026-10-14）**：本包的 01/02 已按 reviewer-6 M-2 改写为**实路径**命令；同族 M-1 的
> 三条未覆盖分支已由后继片补牙 ⇒ `check_anchors.py selftest` 总数从本包记录的 **10/10** 增至
> **13/13**（含 12 条改坏必红）。后继片的读数见
> `../evidence-m3-rng-scene-anchor-teeth-20261014/`。本包其余读数（03–11）保持本片拍摄时口径。
本片：**承接卡规格锚点保鲜机判（`check_anchors.py`）的覆盖面收口** —— 收 reviewer-5 M-1 /
reviewer-6 M-1+M-2+M-3（均为对 `a173eba89`「锚点闭世界」片记的**同族非阻塞 Minor**）。

## 1. 为什么

`DRIVER-CONTRACT.md`（承接卡 `openmir2-m3-rng-scene-driver` 的规格）用 `path:line` 引用承重源码锚点。
上游 `a173eba89` 片把「文档行号 == 当前源码首个匹配行」+「闭世界（每个引用都必须有锚点）」钉成了
fail-closed 机判，但 reviewer 复算后仍点出**两处静默通道**：

| # | 漏报 | 旧口径 |
| --- | --- | --- |
| M-1 / M-2 | **同名 basename、别的目录**：文档写 `foo/monai.rs:5` 会被 `_anchor_covers` 的 `token.endswith("/" + literal)` 判 COVERED；而 `_anchor_re` 的负向环视排除 `/` ⇒ 该引用**既非** `UNGUARDED_ANCHOR` **也不** `ANCHOR_DRIFT` | 静默 |
| M-1 | **同行裸续引用**：`…consumer_groups.tsv:35`（折叠理由）、`:53`（spawn 行）里的 `:53` 不在 `_DOC_REF_RE`（只认 `<path>.<ext>:<line>`）里 ⇒ 那半行可静默漂移（当刻 `:53` 实核正确） | 静默 |
| （同族） | **未识别扩展名**：`_DOC_REF_RE` 的扩展名硬枚举 6 个 ⇒ 其它 `path.<ext>:line` 被跳过 | 静默 |
| M-3 | 上一片证据 `11-boundary.txt` 尾部记的是**拍摄当时**（提交前）的 in-flight `git status`（含本片自身三件正在写盘的证据），易被误读成「边界里多出三件」 | cosmetic |

## 2. 本片改了什么

- `rng-trace-scene/check_anchors.py`
  - 覆盖判定改为**精确解析**：带目录的引用必须与锚点 `literal`/`path` **全等**；裸 basename 仅接受
    显式 `basename_ok=True` 的锚点（`monai.rs`/`mongen.rs`）⇒ 新增 `UNGUARDED_ANCHOR` 牙。
  - 新增**同行裸续引用**解析：`_BARE_REF_RE` 把 `:NN` 归到**同一物理行里前一个**完整引用，并按锚点
    `also`（`line`+`rx`）校验**行内容** ⇒ `BARE_REF_UNDECLARED` / `BARE_REF_OUT_OF_RANGE` /
    `BARE_REF_DRIFT` / `BARE_REF_FILE_MISSING`。同行无前置完整引用的 `:NN` 视为散文（文档里记历史值的
    `:5229`/`:5440` 即此），不误伤。
  - 新增扩展名白名单 `KNOWN_EXTS`；不在其中 ⇒ `UNKNOWN_REF_EXT` fail-closed。
  - `selftest` 5 条改坏必红 ⇒ **10/10（含 9 条改坏必红）**；新增 4 条：同名目录串台 / 裸续引用未登记 /
    裸续**内容**漂移 / 未识别扩展名。
- `rng-trace-scene/DRIVER-CONTRACT.md`：① §4 的宿主 AI 调用点 `main.rs` 锚随 master 合并（⑧-10 +
  client-e2e AOI 诊断）从 `:5440` 漂到 `:5488`，在**合并树的** `-SelfCheckNoStack` 里被本工具当场咬住
  （`ANCHOR_DRIFT`）⇒ 已订正；② §6 增一条「锚点覆盖面收口」说明。改完后 7 个引用与锚点表一致（`03-…`）。
- `rng-trace-scene/README.md`：§5.2 读数 6/6→10/10 + 新增 §5.2.1（两处漏报的改前/改后对照）；
  §6 命令注释同步。
- `evidence-m3-rng-scene-anchor-closure-20261014/11-boundary.txt`：修 reviewer-6 M-3（把 in-flight
  `git status` 段改成提交后实拍 + 订正说明）；同目录 `SHA256SUMS.txt` 用 `verify_sha256sums.py write`
  重记仓库字节口径（**12/12 GREEN / eolcheck 13/13 GREEN**）。

**不改** `run_rng_trace_pair.ps1` 的自检项数（仍 **31/31**：`check_anchors selftest` 的 GREEN 正则只认
分子==分母，不硬编码条数）；**不改**任何判据阈值/口径；**零 `.rs`**。

## 3. 判据读数（改前静默 / 改后 RED 或 GREEN，改坏必红真跑）

| 项 | 命令（cwd = `mir2-rs/tests/parity/m3`） | 读数 | 原始件 |
| --- | --- | --- | --- |
| 改前①：旧工具 + 坏样本（同名目录） | `python -I <a173eba89 工具> check --doc <doc+foo/monai.rs:999>` | **GREEN 7/7（漏报）** | `01-…` |
| 改前②：旧工具 + 坏样本（裸 `:53`→`:54`） | 同上 | **GREEN 7/7（漏报）** | `01-…` |
| 改后①：新机判 + 同名目录坏样本 | `python -I rng-trace-scene/check_anchors.py check --doc <坏样本>` | **RED 1 项**：`UNGUARDED_ANCHOR …foo/monai.rs:999` | `02-…` |
| 改后②：新机判 + 裸续漂移坏样本 | 同上 | **RED 1 项**：`BARE_REF_UNDECLARED …:54` | `02-…` |
| 改后真身 | `python -I rng-trace-scene/check_anchors.py check` | **GREEN 7/7**（含合并 master 后 `main.rs` 锚 `:5440`→`:5488` 的当场咬住 + 订正） | `03-…` |
| 机判自检 | `python -I rng-trace-scene/check_anchors.py selftest` | **10/10 GREEN**（含 9 条改坏必红） | `04-…` |
| 无栈自检 | `run_rng_trace_pair.ps1 -SelfCheckNoStack` | **31/31 GREEN**（`anchors-freshness-selftest` 真接线，项数未变） | `05-…` |
| 离线全链 | 9 个 selftest | 全 rc=0（`15/15`,`37/37`,`22/22`,`13/13`,`7/7`,`10/10`,`32/32`,`23/23`,`12/12`） | `06-…` |
| 四门禁（`CARGO_TARGET_DIR=E:/tmp/m3bag-target`） | fmt / clippy `-D warnings` / workspace test / msgcodegen | rc=0 / rc=0 / **101 suites 1200 passed 0 failed 22 ignored** / 604 rc=0 | `07`–`10` |
| 边界 | `git diff --name-only origin/master..HEAD` | 见 `11-…`；冻结面 `src/ sql/ MirClient mir2-rs/crates/protocol mir2-rs/crates/storage mir2-rs/bins` 空、`*.rs`=0 | `11-…` |
| 证据 hash / EOL | `verify_sha256sums.py verify <本包> --check-extra` / `eolcheck` | GREEN / GREEN | `SHA256SUMS.txt` |

`01-…` 的对照用「a173eba89 的工具 + 同目录临时副本（保持 `REPO_ROOT` 正确，跑完删除）」+ 内存/临时
坏样本；证明确是旧机判的**漏报面**，不是文档「本来就该错」。

## 4. 纪律 / 边界

- **未 merge、未自签 review**；只在自己 worktree 写；共享主检出
  `E:\Users\gxh\Documents\GitHub\OpenMir2` 零改动；**零起栈 / 零占端口**（仅共享 MySQL 3306）。
- 未动 `crates/world`、`bins/**`、`src/**`、`tests/parity/world/**`、drop-exp 实现。
- 工作树未跟踪 `mir2-rs/tests/parity/m3/sessions/csharp-20261012-23xx/`（8 个，**他卡遗留**）未并入本片。
- 卡级真机 DoD 仍阻塞：承接卡 `openmir2-m3-rng-scene-driver` 未开 + owner 独占窗口
  （与本卡两相 trace、④ 真机 100 杀同批次串行）。
