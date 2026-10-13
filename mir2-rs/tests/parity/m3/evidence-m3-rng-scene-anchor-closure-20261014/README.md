# 证据包 —— 承接卡规格「锚点闭世界」机判（2026-10-14 第四次窗口前置审计）

卡：`openmir2-m3-rng-trace-scene-groups`（m3bag / `openmir2-worker-4`）
分支：`wip/m3bag-window-batch-premise`
base：`origin/master` = `d4673b16c`（本卡前三片——Claim B 离线收口 `05c3b4283`、锚点保鲜 `6ecdae644`、
站点表 `oracle_ref` 可解析性 `06f5faf7b`（合并检查点 `b50960ec`）——均已合入 master；
本片 rebase 到 `d4673b16c` 之上，避免「base 未含 master ⇒ 回退假差量」）
tier：**L**（纯离线 工具/文档片，零 `.rs`）；卡级真机 DoD 仍 **R / not_verified**。

## 1. 本片修什么（真缺陷：上一片自己造成的静默漂移 + 旧机判的漏报面）

卡的**离线①②③ 均已达成**（并入 master）或已交付待审；剩余 DoD = **真机两侧两相 trace +
`groupdiff --fold --strict`**，阻塞在两张卡外条件上（承接卡 `openmir2-m3-rng-scene-driver` 未开 +
owner 独占窗口）。本 burst 继续**窗口前置漂移审计**，发现**同族第 4 例**，而且这次是**本卡上一片
（`06f5faf7b`，已合入 master `d4673b16c`）自己造成的**：

- `06f5faf7b` 在 `rng-trace-probe/rng_consumer_groups.py` 顶部插入了 5 行
  （`_HERE` / `REPO_ROOT`，用于把 `oracle_ref` 解析成真实路径）⇒ `FOLD_RULES = {"construct": "spawn"}`
  这个**定义的物理行号从 `:52` 漂到 `:57`**。
- 而承接卡规格 `DRIVER-CONTRACT.md:94` 仍写「定义 = `rng-trace-probe/rng_consumer_groups.py:52`」，
  承接卡/评审按它去源码里找会**找到注释而非定义**（差 5 行），**且没有任何门会拦**。
- 根因（机判的漏报面）：`check_anchors.py` 的锚点表 `ANCHORS` 只**硬编码 5 条**锚点
  （主调用点 / mongen 接缝 / monai tick / monai wondering / oracle Wondering）；`DRIVER-CONTRACT.md`
  里还有 `rng_consumer_groups.py:52`、`consumer_groups.tsv:35` 两个**未登记**引用。旧工具对**未登记**
  引用不提任何要求 ⇒ 文档新增/漂移的 `path:line` 全部静默放过（旧口径在坏文档上仍报 `5/5 GREEN`）。

⇒ 本片：① 订正引用 `:52` → `:57`；② 把 `rng_consumer_groups.py` 的 `FOLD_RULES` 定义与
`consumer_groups.tsv` 的折叠理由行纳入锚点表；③ 新增**闭世界**判据 `UNGUARDED_ANCHOR`——
文档里**每个** `<path>.<ext>:<line>` 引用都必须有锚点覆盖，否则 fail-closed 点名。

## 2. 交付物

| 文件 | 变更 |
| --- | --- |
| `rng-trace-scene/DRIVER-CONTRACT.md` | `rng_consumer_groups.py:52` → `:57`（真身漂移订正） |
| `rng-trace-scene/check_anchors.py` | 锚点表 5 → 7 条（+`FOLD_RULES` 定义、+`consumer_groups.tsv:35` 折叠理由）；新增 `_DOC_REF_RE`/`_anchor_covers` 闭世界检查（`UNGUARDED_ANCHOR`）；selftest 4/4 → 6/6（+未登记引用、+折叠定义回退 2 条改坏必红） |
| `rng-trace-scene/README.md` | §5.2（闭世界牙 + 本片真身）、§6（`check_anchors` 读数 4/4 → 6/6） |
| `rng-trace-scene/run_rng_trace_pair.ps1` | ㉙ 注释：3 条改坏必红 → 5 条（含闭世界 / 折叠定义回退） |
| 本证据包 | 13 件（01 旧口径漏报 / 02 新机判必红 / 03 改后绿 / 04 selftest / 05 无栈 / 06 离线全链 / 07–10 四门禁 / 11 boundary / README / SHA256SUMS） |

`validate_oracle_refs`（上一片）与 `check_anchors` 是**两条独立**的牙：前者查 `consumer_groups.tsv`
的 `oracle_ref`（源文件存在 + 行号在范围内），后者查 `DRIVER-CONTRACT.md` 的 `path:line`（行号 == 首个匹配）。
本片补的是后者的**覆盖率**（闭世界），不改任何判据阈值。

## 3. 判据读数（改前 RED / 改后 GREEN，改坏必红真跑）

| 项 | 命令（cwd = `mir2-rs/tests/parity/m3`） | 读数 |
| --- | --- | --- |
| 改前①：旧工具 + 旧文档 | `python -I <旧 check_anchors> check --doc <旧 DRIVER-CONTRACT.md>` | **GREEN 5/5（漏报）** —— 见 `01-…` |
| 改前②：新机判 + 旧文档 | `python -I rng-trace-scene/check_anchors.py check --doc <旧文档副本>` | **RED 1 项**：`ANCHOR_DRIFT …文档写 …:52，但首个匹配在 :57` —— 见 `02-…` |
| 改后 GREEN | `python -I rng-trace-scene/check_anchors.py check` | **GREEN 7/7**（7 个引用全部保鲜）—— 见 `03-…` |
| 机判自检 | `python -I rng-trace-scene/check_anchors.py selftest` | **6/6 GREEN**（5 条改坏必红：行号漂移 / 缺锚点 / 歧义 / 未登记引用 / 折叠定义回退）—— 见 `04-…` |
| 无栈自检 | `run_rng_trace_pair.ps1 -SelfCheckNoStack` | **31/31 GREEN**（`anchors-freshness-selftest` 仍真接线，项数未变）—— 见 `05-…` |
| 离线全链 | 9 个 selftest | 全 rc=0（`15/15`,`37/37`,`22/22`,`13/13`,`7/7`,`6/6`,`32/32`,`23/23`,`12/12`）—— 见 `06-…` |
| 四门禁（`CARGO_TARGET_DIR=E:/tmp/m3bag-target`） | fmt / clippy `-D warnings` / workspace test / msgcodegen | rc=0 / rc=0 / **101 suites 1199 passed 0 failed 22 ignored** / 604 rc=0 —— 见 `07`–`10` |
| 边界 | `git diff --name-only origin/master..HEAD` | 17 路径，全在 `mir2-rs/tests/parity/m3/**`；冻结面 `src/ sql/ MirClient mir2-rs/crates/protocol mir2-rs/crates/storage mir2-rs/bin` 空、`*.rs`=0 —— 见 `11-…` |

`01-…` 的对照命令用「旧工具 + 旧文档」：旧文档 = `git show 06f5faf7b:…/DRIVER-CONTRACT.md`，
旧工具 = `git show 06f5faf7b:…/check_anchors.py`（写进同目录临时副本以保持 `REPO_ROOT` 正确，跑完删除）。
两条对照合起来证明：**这是旧机判的漏报面，不是文档「本来就该错」**。

## 4. 纪律 / 边界

- **未 merge、未自签 review**；只在自己 worktree 写；共享主检出 `E:\Users\gxh\Documents\GitHub\OpenMir2`
  零改动；**零起栈 / 零占端口**；未动 `crates/world`、`bins/**`、`src/**`、`tests/parity/world/**`、drop-exp 实现。
- 工作树未跟踪 `mir2-rs/tests/parity/m3/sessions/csharp-20261012-23xx/`（8 个，**他卡遗留**）未并入本片。
- 卡级真机 DoD 仍阻塞：承接卡 `openmir2-m3-rng-scene-driver` 未开 + owner 独占窗口
  （与本卡两相 trace、④ 真机 100 杀同批次串行）。
