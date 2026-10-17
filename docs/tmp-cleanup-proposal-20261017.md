# E:/tmp 残留清理重审报告（第 2 版，只读车道）

> **卡片**：`openmir2-tmp-cleanup-audit-redo`（看板【待认领】；issue entry oid `51a981d1db6c1839a207f6233f90803d9b16a49d`，签名者 `openmir2-orchestrator`，ts 1792076379）
> **审计时间**：2026-10-17 12:09–12:55（本地，Windows）
> **审计身份**：只读清理重审车道；独立 worktree `E:/tmp/tmpaudit-redo`（branch `tmpaudit-redo`，base `2fa749471`）
> **交付性质**：**只出白名单式删除提案；本车道未删除任何文件，未 commit/push，未移动卡片。**
> **一句话结论**：A 类可回收 **448.1 GiB**（其中 cargo target 443.6 GiB），另有 `m3bag-toolseol` 三个子目录 **639 MiB**；C 类（非本项目）24.9 GiB 一律不动；D 类 29.5 GiB 须人核。

## 0. 硬约束遵守声明

- 未改冻结面：`src/**`（C# oracle）、`sql/**`、`MirClient/`、`mir2-rs/crates/protocol`、`mir2-rs/crates/storage` 均无写入。
- 未改共享主检出 `E:\Users\gxh\Documents\GitHub\OpenMir2` 的工作树；所有落盘写入只在本车道 worktree `E:/tmp/tmpaudit-redo` 内。`git worktree add` 只新增 `.git/worktrees/tmpaudit-redo` 元数据，未改任何 tracked 文件。
- 未改部署件 `E:/MirServer/**`；未启服务/客户端/假人；未占端口；未写数据库。
- **未删除任何文件**（`/e/tmp` 下原有一切保持原样）。
- 结论均附来源（`file:line` / 卡 id / commit / 命令输出），见 §2。

## 1. 首版假设为何被证伪，本次如何避免

### 1.1 首版假设（2026-10-15 工作流 C 线）

> 「28 个未注册目录无 `.git` ⇒ 删除零损失」。

按该清单执行会**不可逆销毁他人数据**，因此卡片要求重做审计（见本卡 issue body）。

### 1.2 证伪证据（本次实测）

1. **`find` 才是 ground truth**：`find E:/tmp -maxdepth 3 -name .git` 命中 **75** 个含 `.git` 的路径；放宽到 `-maxdepth 5` 命中 **88** 个（含 `m5-tools-draft/tool1/*`、`qemu-src/qemu-11.1.0/subprojects/*` 等嵌套仓）。首版用目测/`ls` 得到 28 个，数量级即错。
2. **"未注册"不等于"可删"**：本次首个快照只登记 5 个 worktree（主检出 + 4 个 /e/tmp），但 75 个含 `.git` 路径里大量是**死克隆**或**指向已删除 admin 目录的 dangling worktree**。例如 `buff-verify/.git` 是文件、指向 `.../.git/worktrees/impl-buff`（该目录已不存在），`git rev-parse HEAD` 失败但目录内有 ~211 MiB 完整源码树。
3. **有未合内容**：`eol-final2` 的 `git status --porcelain` 实测 **4140 个 `D` + 31 个 `??` = 4171 个脏条目**（与卡面「4171 脏文件」一致）；`eol-final`（HEAD `9d589918d`）、`eol-fix`/`eol-fix3`（HEAD `e4ca712ab`）的 HEAD **不是 master 祖先**。
4. **有独有引用**：`eol-mut`/`eol-mut2` 的 HEAD `6b1638fa2`、`eol-mut3`/`eol-mut4` 的 HEAD `fd1ba8f2b` 在主仓对象库中 **不存在**（`git cat-file -e` 失败）；`eol-base2`/`eol-base3` 的 `refs/heads/worktree-m3-gitattributes` 与 `refs/remotes/origin/*` 指向 `e4ca712ab`，**不在主仓 `git rev-list --all`（4674 commits）内**。⇒ 这些克隆是 `e4ca712ab` 的唯一 ref 持有者，不能删。
5. **跨项目数据混放**：`threadx-src` 的 remote 是 `https://github.com/eclipse-threadx/threadx.git`；`wechat_rpa` 下是 5 个个人仓（PyWxDump/chatlog/wcdb-key-tool/wechat-export-macos/weixin-decrypte-script）；另有 `abb-*`、`Ultralytics`、`ParamGen`、`qemu-src`、`ffmpeg`、`ort`(ONNX Runtime) 等。
6. **首版的硬失实（06run_6-small.png）**：`find E:/tmp -name '*06run_6-small*'` 无结果；原件只在 `client-e2e/verify1/06run_6.png`（1,197,236 B，2026-10-14 00:38）与 `client-e2e/verify2/06run_6.png`（1,189,735 B，2026-10-14 15:02）；`git ls-files --error-unmatch client-e2e/verify1/06run_6.png` 返回 **paths not known to git**。⇒ 首版「已提交 master」不成立，`client-e2e` 必须保留/先归档，不能随 critique 目录一起删。

### 1.3 本次防错措施（逐条对应）

| 首版错误 | 本次做法 |
|---|---|
| 用 `ls`/目测判断 `.git` | `find -maxdepth 5 -name .git` 扫描；每个 `.git` 单独记录 HEAD/branch/remotes/dirty/unique refs |
| 只看 HEAD 是否在 master | 对每个 ref 先 `rev-parse <ref>^{commit}` 剥 tag，再与主仓 `git rev-list --all`（4674 commits）比对；**tag 对象 `ed3a94196` 剥出 `c8edfc4f` 后确认在 master 上**，才放行 `eol-base` |
| 把"未注册"当"死克隆" | 死克隆必须同时满足：所有 ref（剥 tag 后）⊆ 主仓可达集 + `status --porcelain` 为空；否则进 B/D |
| 不区分项目 | 先按 remote/内容标记判归属；C 类**一律不动**，D 类须人核 |
| 输出通配前缀删除 | 只出逐路径白名单；`Remove-Item -LiteralPath`，**禁用 `E:\tmp\*-target` 这类通配** |
| 静态快照 | 审计期间并发变化已实测（见 §2.2）；执行前必须重新核对 worktree/进程/路径清单 |

## 2. 方法与证据来源

### 2.1 本次使用的命令（可复现）

```powershell
walgit collab board --format json            # 找到卡 id 与 issue oid
walgit collab thread openmir2-tmp-cleanup-audit-redo
git -C E:\Users\gxh\Documents\GitHub\OpenMir2 worktree list
git -C E:\Users\gxh\Documents\GitHub\OpenMir2 rev-list --all      # 4674 commits
git -C E:\Users\gxh\Documents\GitHub\OpenMir2 branch -a
find E:/tmp -maxdepth 5 -name .git -print
git -C E:/tmp/<dir> status --porcelain
git -C E:/tmp/<dir> rev-parse '<ref>^{commit}'
git -C E:\Users\gxh\Documents\GitHub\OpenMir2 cat-file -e <oid>
du -sh E:/tmp/<dir>
```

### 2.2 快照与并发变化（重要）

本次盘点以 **2026-10-17 12:09 的首个 `ls -d */` 快照**为基线（1466 个一级目录）。审计过程中实测到另一条车道在并发操作：

- `E:/tmp/m821-baseline`（首快照中存在，detached `0f8390dbb`）在 12:26 前**消失**；
- 新增 worktree：`checklist-base`、`verify-checklist`、`verify-m821`、`verify-m821-base`；
- 12:26 有 `cd /e/tmp/verify-m821/mir2-rs && CARGO_TARGET_DIR=E:/tmp/verify-m821-target cargo test --workspace` 在跑（日志 `E:/tmp/verify-m821-test.log`），该 target 已按 **B 类**保留；
- 最终快照 `_ro_dirs2.txt` 为 **1472** 个一级目录（+6 新增，−1 消失 + 本车道 `tmpaudit-redo`）。

- **12:45 复查**：`verify-m821-target`（12:26 时 13 GiB，B 类）已被其车道删除；本报告仍按首快照记录，但这再次证明「执行前复核」不是形式条款。

> 因此本报告的清单是**时点快照**。执行 A 类前必须重新跑 `git worktree list` 与 `Get-Process cargo,rustc`；任何在快照后变成活跃 worktree / 正在构建 target 的路径，**必须从 A 类剔除**。

### 2.3 快照规模

- 一级目录 **1472** 个；一级文件 **~7000** 个、合计 **2.9 GiB**（本报告只对目录分类，文件另见 §8 残留与 §7）。
- 目录总占用（已知）**563.4 GiB**。

## 3. 分类总览

| 类别 | 定义 | 目录数 | 大小 | 处置 |
|---|---|---:|---:|---|
| **A** | 本项目临时物，**证明可安全删**（cargo target 可再生 / 死克隆无独有 ref / 空目录） | 73 | **448.1 GiB** | 本报告白名单，owner 签字后逐路径执行 |
| **B** | 本项目但**须保留**（活跃 worktree/target、工具链、证据、未合内容） | 44 | 60.9 GiB（另有 `verify-m821-target` 13 GiB 未计入） | 不动 |
| **C** | **非本项目**（abb/zgc/个人仓/外部工具链） | 706 | 24.9 GiB | **一律不动**；删除须其 owner 决定 |
| **D** | 未知 / 未证明可删 | 649 | 29.5 GiB | 须人核，不进白名单 |

> 全量逐目录分类见 [manifest CSV](tmp-cleanup-proposal-20261017.manifest.csv)（1472 行，含 path/class/size/mtime/reason）。

## 4. A 类：白名单式删除提案（448.1 GiB）

### 4.1 A1 — cargo target：43 个，443.6 GiB

**安全理由（三证）**：① 目录内无 `.git`（已对每个 target `find -maxdepth 2 -name .git` 扫描，全部为空）；② 目录是 cargo 构建产物（`CACHEDIR.TAG` + `.rustc_info.json` + `debug/`；`verify-entershift-target`、`verify-a5b-target` 无 `CACHEDIR.TAG` 但有后两者）；③ 内容可由 `cargo build/test` 再生，源码与锁文件在仓库内，不存在只读产物。
**注意**：这些 target 多数 mtime 是今天（最近被构建过），删除会损失重建时间、不损失数据；执行前须确认无 `cargo/rustc` 进程且对应车道已空闲。

| 路径 | 大小 | mtime | 三证 |
|---|---|---|---|
| `E:\tmp\barriera-target` | 15.0 GiB | 2026-10-17 05:40 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\ctorfold-target` | 14.0 GiB | 2026-10-17 04:46 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\mapcell-target` | 14.0 GiB | 2026-10-17 06:24 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\mrm-target` | 14.0 GiB | 2026-10-17 11:20 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\a5-target` | 13.0 GiB | 2026-10-17 03:31 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\dealtooth-target` | 13.0 GiB | 2026-10-17 02:20 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\spellframe-target` | 13.0 GiB | 2026-10-17 04:08 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\teeth-target` | 13.0 GiB | 2026-10-17 09:05 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\verify-barriera-target` | 13.0 GiB | 2026-10-17 06:01 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\verify-enter-target` | 13.0 GiB | 2026-10-17 09:24 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\verify-entershift-target` | 13.0 GiB | 2026-10-17 11:36 | 无 CACHEDIR.TAG，但 `.rustc_info.json` + `debug/` 确认是 cargo target |
| `E:\tmp\verify-teeth-target` | 13.0 GiB | 2026-10-17 09:30 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\ctorsite-target` | 12.0 GiB | 2026-10-17 02:26 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\enter-target` | 12.0 GiB | 2026-10-17 09:12 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\entershift-target` | 12.0 GiB | 2026-10-17 11:24 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\mcpolish-target` | 12.0 GiB | 2026-10-17 06:55 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\merge-target` | 12.0 GiB | 2026-10-16 23:45 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\prodwire-target` | 12.0 GiB | 2026-10-17 03:10 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\recall-target` | 12.0 GiB | 2026-10-17 09:30 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\snapfix-target` | 12.0 GiB | 2026-10-17 04:29 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\verify-a5fix-target` | 12.0 GiB | 2026-10-17 04:05 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\verify-mrm-target` | 12.0 GiB | 2026-10-17 11:45 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\verify-recall-target` | 12.0 GiB | 2026-10-17 09:50 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\verify-tick2-target` | 12.0 GiB | 2026-10-16 23:17 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\a5fix2-target` | 11.0 GiB | 2026-10-17 03:56 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\bapolish-target` | 11.0 GiB | 2026-10-17 06:13 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\fcg-target` | 11.0 GiB | 2026-10-17 09:08 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\mongen-target` | 11.0 GiB | 2026-10-17 07:44 | **无 `CACHEDIR.TAG`**（仅 `.rustc_info.json` + `debug/`）——2026-10-17 复验订正；三证口径下与 `verify-entershift-target` 等同类，列为例外；安全性结论不变（cargo target 可再生） |
| `E:\tmp\npcctor-verify-target` | 11.0 GiB | 2026-10-17 02:14 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\persist-target` | 11.0 GiB | 2026-10-17 07:41 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\rnggroup-target` | 11.0 GiB | 2026-10-17 11:30 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\verify-fcg-target` | 11.0 GiB | 2026-10-17 09:34 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\verify-spellframe-target` | 11.0 GiB | 2026-10-17 04:27 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\mrm-base-target` | 10.0 GiB | 2026-10-17 11:28 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\verify-a5b-target` | 9.9 GiB | 2026-10-17 03:42 | 无 CACHEDIR.TAG，但 `.rustc_info.json` + `debug/` 确认是 cargo target |
| `E:\tmp\verify-ctorfold-target` | 6.5 GiB | 2026-10-17 05:13 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\verify-a5b-mut-target` | 5.0 GiB | 2026-10-17 03:46 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\ctorsite-mutant-target` | 2.3 GiB | 2026-10-17 02:39 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\rng-group-target` | 2.3 GiB | 2026-10-17 11:22 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\buff-verify-target` | 1.6 GiB | 2026-10-17 01:41 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\verify-a5fix-mut-target` | 1.3 GiB | 2026-10-17 04:10 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\routefix-target2` | 656 MiB | 2026-10-17 00:33 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |
| `E:\tmp\rev-gcf-target` | 85 MiB | 2026-10-13 20:33 | CACHEDIR.TAG + `.rustc_info.json` + `debug/` |

### 4.2 A2 — 死克隆：16 个，4.5 GiB

**安全理由（三证）**：① 所有 ref（annotated tag 先 `^{commit}` peel）都在主仓 `git rev-list --all` 的可达集内；② `git status --porcelain` 为空；③ 内容在主仓对象库中，可随时重新 checkout/clone。
**明确排除**：`eol-base2`/`eol-base3`（唯一指向 `e4ca712ab`）→ B；`eol-final`/`eol-final2`/`eol-fix`/`eol-fix3`/`eol-mut*`/`eol-stale` → B；`m3bag-f1-clone`/`clone2`/`fabc-clone`/`minors6-clone`（HEAD 非 master 祖先）→ B；`owner-f9-fresh`（名字表明是 owner 基线）→ B；`m3cs21-evidence/clean-clone`（证据目录内）→ 随 `m3cs21-evidence` 保留。

| 路径 | 大小 | HEAD | 三证 |
|---|---|---|---|
| `E:\tmp\eol-fix4` | 300 MiB | `0dc48618` | HEAD 为 master 祖先；`status --porcelain` 空；所有 ref peel 后 ⊆ `git rev-list --all` |
| `E:\tmp\eol-base` | 299 MiB | `94aa08f8` | HEAD 为 master 祖先；`status --porcelain` 空；所有 ref peel 后 ⊆ `git rev-list --all` |
| `E:\tmp\m3bag-fu-clone` | 294 MiB | `e0f9adad` | HEAD 为 master 祖先；`status --porcelain` 空；所有 ref peel 后 ⊆ `git rev-list --all` |
| `E:\tmp\m3bag-r1refine-clone` | 293 MiB | `d00e27d6` | HEAD 为 master 祖先；`status --porcelain` 空；所有 ref peel 后 ⊆ `git rev-list --all` |
| `E:\tmp\m3bag-minors6-clone2` | 286 MiB | `db3897d3` | HEAD 为 master 祖先；`status --porcelain` 空；所有 ref peel 后 ⊆ `git rev-list --all` |
| `E:\tmp\m3bag-minors6-clone3` | 286 MiB | `2c4344b9` | HEAD 为 master 祖先；`status --porcelain` 空；所有 ref peel 后 ⊆ `git rev-list --all` |
| `E:\tmp\m3bag-ledger-clone` | 285 MiB | `a72a40e5` | HEAD 为 master 祖先；`status --porcelain` 空；所有 ref peel 后 ⊆ `git rev-list --all` |
| `E:\tmp\m3bag-myfield-clone` | 285 MiB | `40166a82` | HEAD 为 master 祖先；`status --porcelain` 空；所有 ref peel 后 ⊆ `git rev-list --all` |
| `E:\tmp\m3bag-verify-clone` | 285 MiB | `a72a40e5` | HEAD 为 master 祖先；`status --porcelain` 空；所有 ref peel 后 ⊆ `git rev-list --all` |
| `E:\tmp\m3bag-pinretry-clone` | 284 MiB | `6b39556e` | HEAD 为 master 祖先；`status --porcelain` 空；所有 ref peel 后 ⊆ `git rev-list --all` |
| `E:\tmp\m3bag-r1f2` | 284 MiB | `d5e07531` | HEAD 为 master 祖先；`status --porcelain` 空；所有 ref peel 后 ⊆ `git rev-list --all` |
| `E:\tmp\m3bag-r1-clone` | 283 MiB | `f04b354f` | HEAD 为 master 祖先；`status --porcelain` 空；所有 ref peel 后 ⊆ `git rev-list --all` |
| `E:\tmp\m3bag-clone-verify` | 279 MiB | `8f193a6c` | HEAD 为 master 祖先；`status --porcelain` 空；所有 ref peel 后 ⊆ `git rev-list --all` |
| `E:\tmp\m3bag-clone-verify-tick` | 279 MiB | `8f193a6c` | HEAD 为 master 祖先；`status --porcelain` 空；所有 ref peel 后 ⊆ `git rev-list --all` |
| `E:\tmp\m3bag-attack-verify` | 278 MiB | `568c6c47` | HEAD 为 master 祖先；`status --porcelain` 空；所有 ref peel 后 ⊆ `git rev-list --all` |
| `E:\tmp\m3bag-f1-final` | 276 MiB | `3516271c` | HEAD 为 master 祖先；`status --porcelain` 空；所有 ref peel 后 ⊆ `git rev-list --all` |

### 4.3 A3 — 空目录与 Python 字节码缓存：14 个，≈0

空目录删除零损失（快照时刻 `os.listdir` 为空）；`__pycache__` 为 `mutate_run` / `rev-castle-*` 的 `.pyc`，Python 导入时自动再生。

| 路径 | 大小 | mtime |
|---|---|---|
| `E:\tmp\cfl-mut` | 0 | 2026-10-13 23:54 |
| `E:\tmp\eolwork` | 0 | 2026-10-12 22:54 |
| `E:\tmp\m3-csharp-hook` | 0 | 2026-10-12 21:38 |
| `E:\tmp\m3cs-party` | 0 | 2026-10-13 05:07 |
| `E:\tmp\m3cs-threads` | 0 | 2026-10-14 07:49 |
| `E:\tmp\om-merge-DOC` | 0 | 2026-10-13 07:32 |
| `E:\tmp\om-merge-M3FA3` | 0 | 2026-10-13 08:40 |
| `E:\tmp\om-merge-PET1` | 0 | 2026-10-13 08:32 |
| `E:\tmp\om-merge-PT1` | 0 | 2026-10-13 08:19 |
| `E:\tmp\openclaw` | 0 | 2026-08-15 01:23 |
| `E:\tmp\openmir2-rev-mine` | 0 | 2026-10-16 22:58 |
| `E:\tmp\rev-castle-mut` | 0 | 2026-10-14 06:14 |
| `E:\tmp\zz-prewarm` | 0 | 2026-10-12 19:41 |

| 路径 | 大小 | mtime | 理由 |
|---|---|---|---|
| `E:\tmp\__pycache__` | 0 | 2026-10-17 06:30 | Python bytecode cache; auto-regenerated |

### 4.4 A4 — `m3bag-toolseol` 的安全子目录（639 MiB）

`m3bag-toolseol` 顶层整体**不删**（内含 `fix1` 独有 commit `962f7c80`，主仓对象库不存在）。仅以下 3 个子目录满足"所有 ref ⊆ 主仓可达集 + status 干净"：

| 路径 | 大小 | HEAD | 依据 |
|---|---|---:|---|
| `E:\tmp\m3bag-toolseol\base` | 301 MiB | `363c0bd1` | refs 被主仓 `archive/om-m3-alt-merge` 等包含 |
| `E:\tmp\m3bag-toolseol\fix2` | 169 MiB | `c75e6486` | 同上 |
| `E:\tmp\m3bag-toolseol\head` | 169 MiB | `f6d5485f` | 同上 |
| `E:\tmp\m3bag-toolseol\fix1`（**保留**） | 169 MiB | `962f7c80` | 主仓无此对象，属本项目未合内容 |

### 4.5 执行前置门禁（缺一不可）

```powershell
# 1) 无构建进程
(Get-Process cargo,rustc -ErrorAction SilentlyContinue | Measure-Object).Count   # 必须为 0
# 2) worktree 与报告快照一致；新增 worktree 的 target 不得在删除清单
git -C E:\Users\gxh\Documents\GitHub\OpenMir2 worktree list
# 3) 逐路径复核：存在、mtime、size 与 §4 表格一致；不存在则跳过并记录
# 4) owner 在卡上签字确认本清单（本卡产出=清单，不是删除）
```

### 4.6 建议执行命令（逐路径，禁用通配前缀）

> 以下是 **owner 批准后**的参考命令；本车道**未执行**。

```powershell
Remove-Item -LiteralPath 'E:\tmp\barriera-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\ctorfold-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\mapcell-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\mrm-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\a5-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\dealtooth-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\spellframe-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\teeth-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\verify-barriera-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\verify-enter-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\verify-entershift-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\verify-teeth-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\ctorsite-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\enter-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\entershift-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\mcpolish-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\merge-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\prodwire-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\recall-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\snapfix-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\verify-a5fix-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\verify-mrm-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\verify-recall-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\verify-tick2-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\a5fix2-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\bapolish-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\fcg-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\mongen-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\npcctor-verify-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\persist-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\rnggroup-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\verify-fcg-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\verify-spellframe-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\mrm-base-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\verify-a5b-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\verify-ctorfold-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\verify-a5b-mut-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\ctorsite-mutant-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\rng-group-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\buff-verify-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\verify-a5fix-mut-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\routefix-target2' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\rev-gcf-target' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\eol-fix4' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\eol-base' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\m3bag-fu-clone' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\m3bag-r1refine-clone' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\m3bag-minors6-clone2' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\m3bag-minors6-clone3' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\m3bag-ledger-clone' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\m3bag-myfield-clone' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\m3bag-verify-clone' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\m3bag-pinretry-clone' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\m3bag-r1f2' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\m3bag-r1-clone' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\m3bag-clone-verify' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\m3bag-clone-verify-tick' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\m3bag-attack-verify' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\m3bag-f1-final' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\cfl-mut' -Force
Remove-Item -LiteralPath 'E:\tmp\eolwork' -Force
Remove-Item -LiteralPath 'E:\tmp\m3-csharp-hook' -Force
Remove-Item -LiteralPath 'E:\tmp\m3cs-party' -Force
Remove-Item -LiteralPath 'E:\tmp\m3cs-threads' -Force
Remove-Item -LiteralPath 'E:\tmp\om-merge-DOC' -Force
Remove-Item -LiteralPath 'E:\tmp\om-merge-M3FA3' -Force
Remove-Item -LiteralPath 'E:\tmp\om-merge-PET1' -Force
Remove-Item -LiteralPath 'E:\tmp\om-merge-PT1' -Force
Remove-Item -LiteralPath 'E:\tmp\openclaw' -Force
Remove-Item -LiteralPath 'E:\tmp\openmir2-rev-mine' -Force
Remove-Item -LiteralPath 'E:\tmp\rev-castle-mut' -Force
Remove-Item -LiteralPath 'E:\tmp\zz-prewarm' -Force
Remove-Item -LiteralPath 'E:\tmp\__pycache__' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\m3bag-toolseol\base' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\m3bag-toolseol\fix2' -Recurse -Force
Remove-Item -LiteralPath 'E:\tmp\m3bag-toolseol\head' -Recurse -Force
```

### 4.7 回滚 / 再生成本

- cargo target：`cargo build --workspace`（或对应 `-p`/feature）重建；无源码丢失。
- 死克隆：`git clone E:\Users\gxh\Documents\GitHub\OpenMir2` 后 checkout 到表列 HEAD 即可复现。
- 空目录：按需 `New-Item -ItemType Directory` 重建。
- `m3bag-toolseol` 子目录：从主仓 `archive/*` 分支 checkout。

## 5. B 类：本项目须保留（60.9 GiB + 13 GiB）

### 5.1 活跃 worktree / 正在跑的 target（9 个 worktree）

最终快照 `git worktree list`：

```
E:/Users/gxh/Documents/GitHub/OpenMir2 2fa749471 [master]
E:/tmp/checklist-base                  2fa749471 (detached)
E:/tmp/impl-checklist                  2fa749471 [checklist]
E:/tmp/impl-m821                       0f8390dbb [m821]
E:/tmp/m2svr-window-origin             da2df1fdb (detached)
E:/tmp/tmpaudit-redo                   2fa749471 [tmpaudit-redo]   # 本车道
E:/tmp/verify-checklist                2fa749471 (detached)
E:/tmp/verify-m821                     0f8390dbb (detached)
E:/tmp/verify-m821-base                0f8390dbb (detached)
```

- `m2svr-window-origin`：窗口构建源（任务明确要求保留）。
- `impl-checklist-target`（11 GiB）、`m821-target`（17 GiB）、`verify-m821-target`（13 GiB）：活跃 worktree 的 cargo target，**保留**。

### 5.2 其余 B 类

| 路径 | 大小 | mtime | 保留理由 |
|---|---|---|---|
| `E:\tmp\delphi` | 25.0 GiB | 2026-10-08 20:59 | 项目工具链/证据（保留） |
| `E:\tmp\m821-target` | 17.0 GiB | 2026-10-17 11:56 | 活跃 worktree 的 cargo target（车道在用） |
| `E:\tmp\impl-checklist-target` | 11.0 GiB | 2026-10-17 12:14 | 活跃 worktree 的 cargo target（车道在用） |
| `E:\tmp\rev-e` | 1012 MiB | 2026-10-16 02:11 | 未证明可删：归档/脏工作树/独有 commit |
| `E:\tmp\rebuild-WA` | 509 MiB | 2026-10-16 16:48 | 未证明可删：归档/脏工作树/独有 commit |
| `E:\tmp\m3cs21-evidence` | 366 MiB | 2026-10-12 12:53 | 未证明可删：归档/脏工作树/独有 commit |
| `E:\tmp\window` | 334 MiB | 2026-10-14 07:12 | 未证明可删：归档/脏工作树/独有 commit |
| `E:\tmp\verify-y1b` | 307 MiB | 2026-10-16 13:46 | 未证明可删：归档/脏工作树/独有 commit |
| `E:\tmp\eol-final2` | 301 MiB | 2026-10-13 05:28 | 未证明可删：归档/脏工作树/独有 commit |
| `E:\tmp\eol-final` | 300 MiB | 2026-10-13 05:24 | 未证明可删：归档/脏工作树/独有 commit |
| `E:\tmp\eol-mut3` | 300 MiB | 2026-10-13 05:14 | 未证明可删：归档/脏工作树/独有 commit |
| `E:\tmp\eol-mut4` | 300 MiB | 2026-10-13 05:14 | 未证明可删：归档/脏工作树/独有 commit |
| `E:\tmp\eol-stale` | 300 MiB | 2026-10-13 05:21 | 未证明可删：归档/脏工作树/独有 commit |
| `E:\tmp\eol-base2` | 299 MiB | 2026-10-13 04:23 | 未证明可删：归档/脏工作树/独有 commit |
| `E:\tmp\eol-base3` | 299 MiB | 2026-10-13 04:26 | 未证明可删：归档/脏工作树/独有 commit |
| `E:\tmp\eol-fix` | 299 MiB | 2026-10-13 04:19 | 未证明可删：归档/脏工作树/独有 commit |
| `E:\tmp\eol-fix3` | 299 MiB | 2026-10-13 04:30 | 未证明可删：归档/脏工作树/独有 commit |
| `E:\tmp\eol-mut` | 299 MiB | 2026-10-13 04:35 | 未证明可删：归档/脏工作树/独有 commit |
| `E:\tmp\eol-mut2` | 299 MiB | 2026-10-13 04:37 | 未证明可删：归档/脏工作树/独有 commit |
| `E:\tmp\impl-checklist` | 296 MiB | 2026-10-17 12:09 | 已登记 git worktree（活跃车道） |
| `E:\tmp\m3bag-minors6-clone` | 286 MiB | 2026-10-12 18:54 | 未证明可删：归档/脏工作树/独有 commit |
| `E:\tmp\m3bag-fabc-clone` | 279 MiB | 2026-10-12 12:10 | 未证明可删：归档/脏工作树/独有 commit |
| `E:\tmp\m3bag-f1-clone` | 276 MiB | 2026-10-12 09:18 | 未证明可删：归档/脏工作树/独有 commit |
| `E:\tmp\m3bag-f1-clone2` | 276 MiB | 2026-10-12 09:20 | 未证明可删：归档/脏工作树/独有 commit |
| `E:\tmp\owner-f9-fresh` | 273 MiB | 2026-10-12 03:30 | 未证明可删：归档/脏工作树/独有 commit |
| `E:\tmp\impl-m821` | 221 MiB | 2026-10-17 11:58 | 已登记 git worktree（活跃车道） |
| `E:\tmp\m2svr-window-origin` | 205 MiB | 2026-10-14 07:41 | 已登记 git worktree（活跃车道） |
| `E:\tmp\client-e2e` | 71 MiB | 2026-10-14 15:01 | 项目工具链/证据（保留） |
| `E:\tmp\m2svr-window-run` | 58 MiB | 2026-10-15 13:05 | 项目工具链/证据（保留） |
| `E:\tmp\mirclient-ref` | 31 MiB | 2026-10-16 13:45 | 项目工具链/证据（保留） |
| `E:\tmp\mirbuild` | 23 MiB | 2026-10-10 00:36 | 项目工具链/证据（保留） |
| `E:\tmp\mirold-analysis` | 10 MiB | 2026-10-13 07:52 | 项目工具链/证据（保留） |
| `E:\tmp\m5-tools-draft` | 2 MiB | 2026-10-15 20:17 | 未证明可删：归档/脏工作树/独有 commit |
| `E:\tmp\mir-head-keep` | 0 MiB | 2026-10-09 00:50 | 项目工具链/证据（保留） |
| `E:\tmp\spec` | 0 MiB | 2026-10-16 17:23 | 项目工具链/证据（保留） |
| `E:\tmp\mir2codec` | 0 | 2026-10-16 11:46 | 项目工具链/证据（保留） |
| `E:\tmp\checklist-base` | n/a | 2026-10-17 12:28 | 已登记 git worktree（活跃车道） |
| `E:\tmp\m2svr-window` | n/a | 2026-10-14 07:53 | 项目工具链/证据（保留） |
| `E:\tmp\mirdcu` | n/a | 2026-10-08 12:23 | 项目工具链/证据（保留） |
| `E:\tmp\tmpaudit-redo` | n/a | 2026-10-17 12:24 | 已登记 git worktree（活跃车道） |
| `E:\tmp\verify-checklist` | n/a | 2026-10-17 12:30 | 已登记 git worktree（活跃车道） |
| `E:\tmp\verify-m821` | n/a | 2026-10-17 12:23 | 已登记 git worktree（活跃车道） |
| `E:\tmp\verify-m821-base` | n/a | 2026-10-17 12:29 | 已登记 git worktree（活跃车道） |
| `E:\tmp\verify-m821-target` | n/a | 2026-10-17 12:25 | 活跃 worktree 的 cargo target（车道在用） |

> `delphi`（25.0 GiB）为项目 Delphi 工具链/安装介质（`dcc32.exe`、GOF 提取脚本、Rad Studio ISO/ESD）。其中 ISO/ESD 约 17.6 GiB 属"可重下的安装介质"，但**本提案不动**；若 owner 确认已安装且不再需要 ISO，可另开一条 A 类提案单独回收。

## 6. C 类：非本项目，一律不动（24.9 GiB）

**总原则：这些不是 OpenMir2 的数据，删除须其 owner 决定；本车道不列入删除提案。**

| 组 / 路径 | 目录数 | 大小 | 归属证据 |
|---|---|---|---|
| `abb-*` | 644 | 19.0 GiB | abb 项目：`abb-target-msvc` 内含 `buzz-agent`；`abb-gates/*.log` 为 Rust clippy/build 日志；非 OpenMir2 |
| `zgc-*` | 41 | 2.7 GiB | Zig 构建缓存（`b/h/o/tmp/z` 布局）；非 OpenMir2 workspace |
| `qemu-src` | 1 | 848 MiB | QEMU 11.1.0 源码 + 3 个 submodule，非 OpenMir2 |
| `zig0` | 1 | 454 MiB | Zig 0.15.2 工具链，非 OpenMir2 |
| `zig0152` | 1 | 454 MiB | Zig 0.15.2 下载包，非 OpenMir2 |
| `zig17` | 1 | 397 MiB | Zig 0.17.0 工具链，非 OpenMir2 |
| `ort` | 1 | 366 MiB | ONNX Runtime 1.20.1 + `ort.zip`，非 OpenMir2 |
| `ffmpeg` | 1 | 249 MiB | FFmpeg 7.1 zip 与 win64 构建，非 OpenMir2 |
| `threadx-src` | 1 | 113 MiB | 个人仓：remote=`https://github.com/eclipse-threadx/threadx.git` |
| `zig17dl` | 1 | 96 MiB | Zig 0.17.0 zip，非 OpenMir2 |
| `ziggcache` | 1 | 74 MiB | Zig 全局缓存，非 OpenMir2 |
| `ziggcache2` | 1 | 74 MiB | Zig 全局缓存，非 OpenMir2 |
| `wechat_rpa` | 1 | 66 MiB | 个人仓群：PyWxDump/chatlog/wcdb-key-tool/wechat-export-macos/weixin-decrypte-script |
| `zigsrc` | 1 | 16 MiB | Zig 源码仓（github.com/ziglang/zig），非 OpenMir2 |
| `zigcb` | 1 | 11 MiB | Zig 源码仓（codeberg.org/ziglang/zig），非 OpenMir2 |
| `wayland-cache-holdout` | 1 | 3 MiB | Wayland 仓库快照（README/cursor/egl），非 OpenMir2 |
| `ParamGen` | 1 | 1 MiB | Delphi `ParamGen.dpr` 项目，非 OpenMir2 仓库 |
| `clawed_delivery` | 1 | 1 MiB | Clawed TUI 交付截图，非 OpenMir2 |
| `claude` | 1 | 1 MiB | Claude Code cache-break diff 试验，非 OpenMir2 |
| `.claude`（**隐藏目录**） | 1 | 0 MiB | 仅含 `setup_complete` 标记；Claude Code 产物，非 OpenMir2。**首版清单漏列**——根因：清单用 `ls -d */` 取快照，shell 通配**不匹配点目录**；2026-10-17 复验发现后补入（C 类一律不动） |
| `hermes-results` | 1 | 0 MiB | Hermes 项目测试结果（id-*.txt），非 OpenMir2 |
| `clawed_tui_verification` | 1 | 0 | Clawed TUI 验证快照，非 OpenMir2 |
| `Ultralytics` | 1 | 0 | Ultralytics YOLO `settings.json`，非 OpenMir2 |
| `buzz-*` | 1 | 0 | buzz 项目（`buzz-*-triage.md` / `acp_shape.json`）；非 OpenMir2 |

非前缀的具名目录：

| 路径 | 大小 | mtime | 归属证据 |
|---|---|---|---|
| `E:\tmp\qemu-src` | 848 MiB | 2026-08-27 18:51 | QEMU 11.1.0 源码 + 3 个 submodule，非 OpenMir2 |
| `E:\tmp\zig0` | 454 MiB | 2026-10-16 11:59 | Zig 0.15.2 工具链，非 OpenMir2 |
| `E:\tmp\zig0152` | 454 MiB | 2026-10-16 11:40 | Zig 0.15.2 下载包，非 OpenMir2 |
| `E:\tmp\zig17` | 397 MiB | 2026-10-16 23:08 | Zig 0.17.0 工具链，非 OpenMir2 |
| `E:\tmp\ort` | 366 MiB | 2026-09-19 13:07 | ONNX Runtime 1.20.1 + `ort.zip`，非 OpenMir2 |
| `E:\tmp\ffmpeg` | 249 MiB | 2026-09-19 13:07 | FFmpeg 7.1 zip 与 win64 构建，非 OpenMir2 |
| `E:\tmp\threadx-src` | 113 MiB | 2026-08-29 21:35 | 个人仓：remote=`https://github.com/eclipse-threadx/threadx.git` |
| `E:\tmp\zig17dl` | 96 MiB | 2026-10-16 23:03 | Zig 0.17.0 zip，非 OpenMir2 |
| `E:\tmp\ziggcache` | 74 MiB | 2026-10-16 16:55 | Zig 全局缓存，非 OpenMir2 |
| `E:\tmp\ziggcache2` | 74 MiB | 2026-10-16 16:56 | Zig 全局缓存，非 OpenMir2 |
| `E:\tmp\wechat_rpa` | 66 MiB | 2026-08-17 23:26 | 个人仓群：PyWxDump/chatlog/wcdb-key-tool/wechat-export-macos/weixin-decrypte-script |
| `E:\tmp\zigsrc` | 16 MiB | 2026-10-16 22:59 | Zig 源码仓（github.com/ziglang/zig），非 OpenMir2 |
| `E:\tmp\zigcb` | 11 MiB | 2026-10-16 23:16 | Zig 源码仓（codeberg.org/ziglang/zig），非 OpenMir2 |
| `E:\tmp\wayland-cache-holdout` | 3 MiB | 2026-10-16 23:10 | Wayland 仓库快照（README/cursor/egl），非 OpenMir2 |
| `E:\tmp\ParamGen` | 1 MiB | 2026-10-09 01:08 | Delphi `ParamGen.dpr` 项目，非 OpenMir2 仓库 |
| `E:\tmp\clawed_delivery` | 1 MiB | 2026-05-16 13:08 | Clawed TUI 交付截图，非 OpenMir2 |
| `E:\tmp\claude` | 1 MiB | 2026-02-15 01:48 | Claude Code cache-break diff 试验，非 OpenMir2 |
| `E:\tmp\hermes-results` | 0 MiB | 2026-04-13 19:26 | Hermes 项目测试结果（id-*.txt），非 OpenMir2 |
| `E:\tmp\clawed_tui_verification` | 0 | 2026-05-16 13:08 | Clawed TUI 验证快照，非 OpenMir2 |
| `E:\tmp\Ultralytics` | 0 | 2026-09-23 21:51 | Ultralytics YOLO `settings.json`，非 OpenMir2 |

> 特别声明：`threadx-src`、`wechat_rpa` 是**个人仓**（卡面要求"永不触碰，列清单即止"）；`abb-*` 目录内的 `buzz-agent`/`abb-gates` 日志表明属于另一个 Rust 项目；`zgc-*` 是 Zig 构建缓存；`ort` 是 ONNX Runtime。

## 7. D 类：须人核（29.5 GiB）

D 类 = 本项目可能相关但**没有足够证据**证明可删的目录。典型：

| 路径 | 大小 | mtime | 未决原因 |
|---|---|---|---|
| `E:\tmp\verify-a5` | 11.0 GiB | 2026-10-17 03:21 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\openmir2-watch` | 1.2 GiB | 2026-10-16 22:55 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\wlrepro` | 1.2 GiB | 2026-10-16 23:15 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\verify-y1a` | 837 MiB | 2026-10-16 13:54 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\m3bag-toolseol` | 807 MiB | 2026-10-13 07:10 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\m3cs-f1` | 676 MiB | 2026-10-12 20:11 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\critic` | 628 MiB | 2026-10-16 16:48 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\adv-e` | 613 MiB | 2026-10-16 01:35 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\m2acc` | 518 MiB | 2026-10-12 03:24 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\taskC-verify` | 483 MiB | 2026-10-16 22:27 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\x2-verify` | 458 MiB | 2026-10-16 12:19 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\m3bag-smoke` | 443 MiB | 2026-10-12 13:07 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\m3cs-fn1` | 440 MiB | 2026-10-13 02:05 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\rebuild-WB` | 436 MiB | 2026-10-16 16:05 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\x1v5` | 428 MiB | 2026-10-16 12:18 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\verify-a5b` | 422 MiB | 2026-10-17 03:49 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\m3cs-iso2` | 276 MiB | 2026-10-12 22:06 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\xdd-behavior-analysis` | 235 MiB | 2026-10-13 22:26 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\openmir2-wbevy-s2` | 232 MiB | 2026-10-11 16:17 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\m2svr-k2` | 230 MiB | 2026-10-12 17:32 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\base-a185` | 211 MiB | 2026-10-17 09:18 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\buff-verify` | 211 MiB | 2026-10-17 01:37 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\verify-anchor-copy` | 211 MiB | 2026-10-17 01:52 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\d1-iso` | 210 MiB | 2026-10-16 23:33 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\npcctor-verify-wt` | 210 MiB | 2026-10-17 02:07 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\rebuild-a` | 210 MiB | 2026-10-16 16:11 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\rebuild-ab` | 210 MiB | 2026-10-16 16:07 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\rebuild-b` | 210 MiB | 2026-10-16 16:11 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\rebuild-ba` | 210 MiB | 2026-10-16 16:11 | 未知/未证明可删；本条目不进删除白名单 |
| `E:\tmp\rebuild-replay` | 210 MiB | 2026-10-16 16:09 | 未知/未证明可删；本条目不进删除白名单 |

其余 619 个 D 类目录见 manifest。重点人核项：

- `verify-a5`（11.0 GiB）：内含 `combat.rs.orig`、`iso*.patch`、`*-gate.log`，像证据/补丁归档，**不可当缓存删**。
- `buff-verify`（211 MiB）、`d1-iso`（210 MiB）、`verify-anchor-copy`（211 MiB）、`npcctor-verify-wt`（210 MiB）：`.git` 指向主仓已不存在的 worktree admin 目录，但目录内有完整源码树，可能含未提交改动 ⇒ 须 diff/归档后人核。
- `verify-tick2`/`verify-tick2-clean`/`verify-w3/work`/`verify-y1a/*`/`x1v5/*`/`x2-verify/work`/`c-fixcheck`/`v7check`：`.git` 存在但 0 commits，工作树是 31 个未跟踪顶层条目（完整源码快照）⇒ 不能按"空仓"删。
- `rebuild-WA/work`（staged `M`/`A` 修改）、`rev-e/work*`（脏）、`owner-f9-fresh`、`m3cs21-evidence`、`eol-*`、`m3bag-f1-clone*` 等：明确有未合内容，保留。
- `openmir2-watch`（1.2 GiB）、`wlrepro`（1.2 GiB）、`critic`（613 MiB）等：项目 scratch/证据，未证明可删。

## 8. 06run_6-small.png 专项（对应卡面验收 ③）

- `/e/tmp` 下**不存在** `06run_6-small.png`（`find -maxdepth 4 -name '*06run_6-small*'` 空）。
- 原件路径：`E:\tmp\client-e2e\verify1\06run_6.png`、`E:\tmp\client-e2e\verify2\06run_6.png`。
- `git ls-files --error-unmatch` 证明两者**未被 master 跟踪**。
- ⇒ 本次不删 `client-e2e`（已列 B）；如未来要删，必须先把这两张图归档进仓库/证据包。

## 9. 残留物清单（本次审计留下、本车道不删除）

- `E:\tmp\tmpaudit-redo`：本车道 worktree（branch `tmpaudit-redo`，base `2fa749471`）。
- `E:\tmp\_ro_dirs.txt`、`_ro_dirs2.txt`、`_ro_sizes.txt`、`_ro_mtimes.txt`、`_ro_gitdirs.txt`、`_ro_gitdirs5.txt`、`_ro_gitmeta.txt`、`_ro_main_reachable.txt`、`_ro_classified.txt`、`_ro_manifest.csv`、`_ro_classify.py`、`_ro_err.txt`、`_ro_board.json`、`_ro_missing.txt`（704 行：并发期间**尺寸/存在性检查失败**的路径清单——`du`/`stat` 失败或条目在盘点中被并发车道删除；**不影响分类结论**，但执行删除前须按本报告 §4.5 重核）。
  - 均为本次只读审计的中间产物；按"绝不删除任何文件"约束**保留**，可随 A 类一起由 owner 决定清理。
- 未启动任何服务/客户端/假人；未占端口；未写数据库。
- 本报告与 manifest：
  - `docs/tmp-cleanup-proposal-20261017.md`
  - `docs/tmp-cleanup-proposal-20261017.manifest.csv`

## 10. 结论与建议

1. **先执行 A1（cargo target）**：443.6 GiB，风险最低、收益最大；执行前只需"无 cargo/rustc 进程 + target 未被新 worktree 引用"两道检查。
2. **再做 A2/A3/A4**：4.5 GiB + ≈0 + 639 MiB；逐路径执行，遇到 `git status` 非空立即停止并转入 B/D。
3. **C 类不动**：24.9 GiB；需要其 owner 单独决定。
4. **D 类人核**：29.5 GiB；尤其 `verify-a5`（11 GiB）与 4 个 dangling worktree，不能按缓存处理。
5. 本卡产出到此为止；**未删除、未 commit、未 push、未移卡**。删除须 owner 在本卡/线程签字后再执行。

---

*本报告由只读清理重审车道生成；所有数字来自 2026-10-17 快照，执行前请按 §4.5 复核。*
