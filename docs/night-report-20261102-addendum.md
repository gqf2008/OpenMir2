# 夜间交付报告追加：2026-11-02 02:16 → 06:40

- 卡：`openmir2-night-report-addendum`（tier=L，纯文档汇总；不跑 cargo、不起栈、不碰 `E:/MirServer`）
- 上游报告：`docs/night-report-20261102.md`。该报告正文冻结在 `1634fff4b`（其自述区间 `13112da07` → `1634fff4b`，17 次 first-parent 合并）
- 本追加范围：报告**交付提交** `77a1fea14`（2026-11-02 02:16:38 +0800）→ **`f3c969ea7`**（2026-11-02 06:40:45 +0800）
- 规模：区间内 **9 次 first-parent 合并**、**23 个提交**（9 个 merge + 14 个被并提交）；9 张卡全部 `tier=L`，无 `tier=R`
- 时点：下文门禁读数、工具条数以 **`f3c969ea7`** 为基线；引用其它时点会显式标注

> **时点约定**（沿用原报告）：历史数字只作追溯，不作现况；所有真机项一律标 `NOT_RUN`——**未跑 ≠ 通过**。

## 0. 计数口径（为什么写 9 次，而不是 10 次）

原报告正文把自己冻结在 `1634fff4b`——那是报告写作时最后一个已合并提交。报告本身随后由 `77a1fea14` 合入。
所以从不同起点数，条数不同：

```sh
git rev-list --count --merges 1634fff4b..master    # 10
git rev-list --count --merges 77a1fea14..master    # 9
git rev-list --count 77a1fea14..master             # 23（9 merge + 14 被并提交）
```

- `1634fff4b..master` 的 10 条 = 报告自身的合并提交 `77a1fea14` + 报告交付之后的 **9 次合并**。
- 本追加逐条列的是后者 9 条；取法：

  ```sh
  git log --first-parent --merges \
    --format='%H|%h|%ad|%s' --date=iso 77a1fea14..f3c969ea7
  ```

  该命令的 9 行与本追加 §1 的 9 行**集合相等**。

## 1. 9 次合并逐条

时间均为 `+0800`；approve 列是 `walgit` 协作**条目 oid（blob）**的完整值，不是 commit。

| # | merge 提交 | 时间 | 卡 | approve（条目 oid） | tier | 做了什么 |
|---:|---|---|---|---|---|---|
| 1 | `f12710f34` | 2026-11-02 02:55:59 | `openmir2-docs-prose-anchor-scan` | reviewer-21 `45cd410c5f5cd31a4f4d6b613d1207d816f93f94` | L | 新增 `docs_anchor_gate --prose` 增量散文锚检查 |
| 2 | `7fe720d97` | 2026-11-02 03:15:31 | `openmir2-prose-anchor-wiring` | reviewer-20 `c9cb215c53b4670029c1d4720f2cd9e22c39c3fb` | L | 把 `--prose` 接进合并前流程（AGENTS.md + 两份 brief） |
| 3 | `68b692895` | 2026-11-02 03:20:51 | `openmir2-docs-anchor-selftest-tmp-leak` | reviewer-21 `e151da6ded9fc0c2e43a29f9d29094cead5efb72` | L | 修 `docs_anchor_gate` selftest 临时目录泄漏 |
| 4 | `aa0366216` | 2026-11-02 03:40:16 | `openmir2-prose-wiring-minors` | reviewer-20 `5ae025a7079a239cc147c6cf64709b1c903567d0` | L | 收口接线口径四条 Minor（四档计数/公式/五门禁/刷新 master） |
| 5 | `301fa645f` | 2026-11-02 04:23:24 | `openmir2-orchestration-five-gates` | reviewer-21 `92a0419f01df1f6ab1bddd2a09c3badf59d1b487` | L | 编排运行时统一五门禁 + 车道命令对齐 `walgit` 主源 |
| 6 | `6cc40462f` | 2026-11-02 05:20:15 | `openmir2-window-desktop-precheck` | reviewer-20 `537e471624249034c153118ea32341cdb7688f05` | L | 窗口执行单补开窗前桌面可用性三信号前置 |
| 7 | `0d83ed352` | 2026-11-02 05:39:39 | `openmir2-window-precheck-f2` | reviewer-21 `02b45db7e79f70936eef3f2289806e31ec846706` | L | 执行单 §2.3 补信号①假阴方向与触发信号注记 |
| 8 | `68e4c248e` | 2026-11-02 06:21:13 | `openmir2-gate-ref-flag-asymmetry` | reviewer-20 `686e8334884a35cedfb6c1797f1680d5778f78d5` | L | 三个 hygiene 门未知参数 fail-closed + `--ref` 行为普查 |
| 9 | `f3c969ea7` | 2026-11-02 06:40:45 | `openmir2-gate-ref-residuals` | reviewer-21 `1f1f22434d13fd2358a20619585761d1291f0552` | L | 两个 parity 门未知参数 fail-closed + 普查口径修正 |

计数复算：

```sh
git rev-list --count --first-parent --merges 77a1fea14..f3c969ea7   # 9
git rev-list --count --first-parent --merges 1634fff4b..f3c969ea7   # 10（含报告自身的 77a1fea14）
```

## 2. 新增的关键发现

### 2.1 编排车道提示词曾要求 dual push + `git reset --hard origin/master`

**症状**：编排运行时的合并车道提示词要求同时推 `origin` 与 `walgit`，并在合并前
`git reset --hard origin/master`。而 `origin`（GitHub）只是**人工同步镜像**，不是代码主源。

**危害**：照提示词跑会把本车道重置到**陈旧的镜像 master**；本地已合并但尚未推到 `origin` 的提交
会被 `reset --hard` 直接丢掉。即使不丢，也会让 `origin` 与 `walgit` 谁是准绳变得含糊。

**证据**（修复前）：

```sh
git show 301fa645f^1:tools/orchestration/lane-run.sh | sed -n '17p'
# … follow the brief's admission criteria, four gates and dual push exactly; …
git show 301fa645f^1:tools/orchestration/briefs/brief-merge.md | sed -n '29p;56p'
# git fetch origin && git reset --hard origin/master
# 6. 推送：`git push origin HEAD:refs/heads/master` + `git push walgit HEAD:refs/heads/master`
```

**修复**：`301fa645f`（卡 `openmir2-orchestration-five-gates`，approve reviewer-21 `92a0419f`，tier=L）。

- `lane-run.sh` 第 17 行改为 `five gates (4 cargo: fmt/clippy/test/msgcodegen + prose anchor ...) and push walgit only (origin is a manual mirror, never auto-push)`。
- `brief-merge.md`：`git fetch origin && git reset --hard origin/master` → `walgit/master`；双端推送 → 只推 `walgit`；准入判据的 `ls-remote` 与散文锚 `--base` 全部改用 `walgit`。
- 首审 `request_changes`（reviewer-21 `5f3094f6`）曾抓到 `lane-run.sh` 第 17 行仍写 dual push；返工后才合入。

**修复后复算**（`f3c969ea7` 树）：

```sh
grep -rnE 'dual push|origin/master|双端推送' tools/orchestration/ | wc -l          # 0
grep -rnE 'push walgit only|origin is a manual mirror' tools/orchestration/ | wc -l # 1（lane-run.sh 第 17 行）
```

### 2.2 门禁「静默忽略未知参数」：hygiene 3 处 + parity 2 处，已全部 fail-closed

**问题类别**：门禁不认识 `--ref`，却不报错——它落在裸位置参数或手工 argv 解析里被静默忽略。
于是「我按 ref 跑了门」与「我根本没传 ref」输出逐字节相同，形成**假绿**。

**普查**：`docs/gate-ref-flag-survey-20261102.md` 实跑 77 个门禁（hygiene 12 + parity 65）。
本追加所覆盖区间里分两步收口：

| 步骤 | 卡 / merge | 修的脚本 | 修复前 | 修复后 |
|---|---|---|---|---|
| 第一批（3 处 hygiene） | `openmir2-gate-ref-flag-asymmetry` / `68e4c248e` | `conflict_marker_gate.py`、`docs_anchor_gate.py`、`scenario_manifest_gate.py` | 五处静默忽略 | 剩 2 处（write_set 外） |
| 第二批（2 处 parity） | `openmir2-gate-ref-residuals` / `f3c969ea7` | `m3/rng-trace-scene/check_anchors.py`、`coverage/script_host_noop_audit.py` | 仍静默忽略 | **静默忽略 0** |

**口径**：只改参数处理，**判定逻辑零改动**；未知参数 ⇒ `rc=2` + stderr 点名 + usage。
第二批还专门把「真拒绝」与「值被当子命令」区分开：三种形态 `--ref HEAD` / `--ref=HEAD` / 裸 `--ref`
必须都在 stderr 点名 `--ref`，不能靠「碰巧 rc=2」。

**本追加 head 上的复算**（`f3c969ea7` 树，5 条全部 rc=2）：

```sh
python -I -B tools/hygiene/conflict_marker_gate.py --ref HEAD          # rc=2
python -I -B tools/hygiene/docs_anchor_gate.py --ref HEAD              # rc=2
python -I -B tools/hygiene/scenario_manifest_gate.py --ref HEAD        # rc=2
python -I -B mir2-rs/tests/parity/m3/rng-trace-scene/check_anchors.py --ref HEAD    # rc=2
python -I -B mir2-rs/tests/parity/coverage/script_host_noop_audit.py --ref=HEAD     # rc=2
```

> 仍保留的「接受并生效」3 个门（`dynamic_count_stamp_gate`、`stale_pointer_gate`、
> `check_rust_window_preflight`）不在本类问题内：它们确实消费 `--ref`，换成与 HEAD 无祖先关系的 ref 会改变结论。

### 2.3 锁屏检测：`OpenInputDesktop` 不能当单一判据

**两例相反的实测**：

| 观察方 | 信号① `OpenInputDesktop` | 其它信号 | 结论 |
|---|---|---|---|
| 对端会话（2026-11-02） | 返回 **0**，`GetLastWin32Error()` = 5（ACCESS_DENIED） | `LockApp`(11472)、`LogonUI`(7328) 在场；`GetForegroundWindow()` 为空 | 锁屏 |
| 本方 dev-1 车道（同一锁屏会话） | 返回**非 0**；仓内留档句柄为 `SIG1_OK hWnd=2016` | ② 命中 `LockApp` ⇒ `SIG2_FAIL err=secure-desktop-owner` | 锁屏 |

> 卡面把本方这一例记作「2204」。该具体句柄值在仓内文档与协作线程中未检索到留档；
> 本节以可复核的 `hWnd=2016`（`docs/mcp-admin-window-sheet-20261101.md` §2.3、
> merge_result `95b04a71`）为准。两例相反这一**共同结论**不受影响。

**因此承重判据不是 ①**：

1. **② 前台窗口属主不是 `LockApp` / `LogonUI`**（`GetForegroundWindow()` 非空且进程名不匹配安全桌面）。
2. **③ `WM_CHAR` 探针后客户端日志出现 `key_char`**（证明 SDL 真拿到键盘焦点；② 会漏掉「桌面看着没锁但没焦点」）。
3. ① 只作快速预筛。它还有**假阴**方向：可用桌面上可能返回 0，会误中止（安全方向，但白等窗口）。
   区分「① 单独假阴」与「真锁屏」看 ②③。

**处置口径**：任一信号不满足 ⇒ **窗口不开**、记 `NOT_RUN (desktop-locked)`、不采任何帧面/RNG 证据；
不得记「未通过」，也不得「跳过即通过」。台账条目要注明触发信号（`sig1=SIG1_FAIL` /
`sig2=secure-desktop-owner` / `sig3=no-key_char`），便于区分环境问题与判据问题。

**证据**：`docs/mcp-admin-window-sheet-20261101.md` §2.3（`6cc40462f` 新增，`0d83ed352` 补假阴方向）；
台账 comment `event=mcp-admin-window-blocked-desktop-locked`（oid `874122105c94241db0bb8f66324144ef1fb9233b`）。

### 2.4 散文锚检查为什么必要

**事故**：夜间报告首版（`e40820ad4`）在正文里引用了一个**不存在**的源码路径——`mir2-rs/crates/protocol/src/smframe.rs`，
配行号 `:39-51`。真实文件在 `mir2-rs/bins/game-svr/src/smframe.rs`。当时所有门禁仍 GREEN。

**根因**：`docs_anchor_gate` 的原生模式只遍历 `docs_anchor_registry.toml` 登记的极少数文件，
且只扫 Markdown **表格的「位置」列**（源码里 `if not line.startswith("|"): continue`）；
`docs/**` 正文（散文）里的 `path:line` 锚**完全在域外**。

**修复链**：

- `f5cb900b4` 人工订正了该锚（`git show e40820ad4:docs/night-report-20261102.md` 可见旧串；
  `git show f5cb900b4 -- docs/night-report-20261102.md` 可见改动）。
- `f12710f34` 新增 `docs_anchor_gate --prose --base <ref>`：只扫 `git diff <merge-base> <worktree>`
  **新增行**里的反引号 `.rs`/`.cs` `path:line`，逐个核 ① 文件存在 ② 行号不越界 ③ 锚定行含声明符号。
  四档计数「检查 N / 悬空 M / 越界 K / 符号缺失 S」，`M+K+S>0 ⇒ rc=1`；空输入显式「检查 0」且 rc=0。
- **域限制**：只识别反引号内的 `.rs`/`.cs`；`.py`/`.md`/`.toml` 与非反引号裸锚不可见。
  所以「检查 0」只表示**本模式域内没有锚**，不等于「新增行里所有锚都干净」。

## 3. 证据索引

### 3.1 每条合并的 merge_result 关键结论

账本条目是 `kind=merge_result` 的协作条目 oid（blob，不是 commit）。要核提交用 merge 列。

| merge | 账本条目 | reviewed_head | 关键结论（摘要） |
|---|---|---|---|
| `f12710f34` | `c82b0e4405230d0d90b83ecbb644456e597f09eb` | `08162761acf905a1617330b11d6744fae6a048ce` | `--prose` 新增 +336/−0，默认表格模式零改动；首审 request_changes 抓到「末尾换行多计一行」真 bug，返工 splitlines()；selftest 25→31；git 不可用收敛 rc=2 |
| `7fe720d97` | `85f03062a1b9b6002b4c500af229e41a1a492236` | `18599b522406ba496415aa48ef9a00f7038f9511` | AGENTS.md +12 / brief-review +6 / brief-merge +9（+27/−0）；三处写清「在待合并分支上跑、base 取合并目标 ref、合并后以 master 为 base = 空 diff = 检查 0」；不可替代 stale_pointer_gate |
| `68b692895` | `86f6cc218f61b73803efc4c732592e20e0d1e417` | `3b6f0ea84f55bb7eb8ebd4a2c8a6b4f077c3a142` | `_rmtree_force`（chmod S_IWRITE 后重试）+19/−4，四处清理改 onerror；名称集合差 3→3、added/removed 皆空；变异回 ignore_errors=True ⇒ 泄漏 3 个目录且 selftest 仍 rc=0 |
| `aa0366216` | `6e66ec2dcb02bbf364344adf6d9d302d245ebf34` | `8a95a234725d15f8427bfd897d35e04d415ae74a` | 四条 Minor +14/−7：四档计数 M+K+S>0；公式改 worktree 并注明 dirty 树会扫；brief-merge 改「五门禁」；brief-review 补 git fetch walgit master |
| `301fa645f` | `6c71f6ecaef9258fc0110efd2c3adbc97ad0da72` | `7df233e224cbbd53f70f8269fb28111c56a698fa` | 编排四门禁→五门禁；lane-run.sh 第 17 行改 push walgit only；brief-merge 的 origin 命令全改 walgit；首审 request_changes 抓到第 17 行仍 dual push |
| `6cc40462f` | `95b04a7146c02373c6633d9acfd23ff1e1c551d6` | `0d1563793ec1f39bdae5a38d3c1e9ac15e037055` | 执行单 §2.3 三信号 +131/−0；关键实测 ① 锁屏下仍可能非 0（SIG1_OK hWnd=2016，② SIG2_FAIL 命中 LockApp）⇒ ① 只作预筛 |
| `0d83ed352` | `008aa1456ad8d11b9b7b1e175c8e05c113e2e979` | `372fac994026050cfcf3d23b78935526ccb0a154` | §2.3 补 ① 假阴方向 + 台账触发信号（+9/−0）；处置口径不变 |
| `68e4c248e` | `264ebe6be9d632922ec789dbcbc5c2364ec4aadf` | `befbbb3ca0357556f9a5f257821947b377ed2a8f` | 三个 hygiene 门未知参数 fail-closed；普查 77 门（修复前静默忽略 5，修复后 2）；变异删拒绝分支三处分别红 |
| `f3c969ea7` | `69c2c03a07b5faea447e7d6b6fba6cf43e0657d2` | `cb8f29f94fdc0e8bed3a6bd1b8c3f7753194e416` | 两个 parity 门 fail-closed；`--ref HEAD` / `--ref=HEAD` / 裸 `--ref` 三形态均 rc=2 且点名；普查静默忽略归 0；§7.1 登记 check_anchors 两处既有 RED |

### 3.2 工具门现状（时点：`f3c969ea7`）

```sh
python -I -B tools/hygiene/tool_selftest_gate.py --inventory
# 清点：66 个带 selftest 的工具；门禁内 61；已分类出域 5；未分类 0   rc=0
```

- 工具门：**61 项在门 / 清点 66 = 61 + 5 + 0**。
- 与报告正文 §3.3 的 61/66 相同：这 9 次合并没有增减工具，只改门禁参数处理与文档。
- 原报告记录过更早的 59/64（2026-11-01 20:45 时点，`b1242001`）；那不是现况。

### 3.3 散文锚 `--prose` 已接进合并前流程

| 位置 | 内容 |
|---|---|
| `AGENTS.md:214-217` | 「本地门禁（合并前全绿）」之后的散文锚命令与口径 |
| `tools/orchestration/briefs/brief-review.md:7,11-12` | 先 `git fetch walgit master`；在待审 head 所在分支上跑 `--prose --base <合并目标 ref>` |
| `tools/orchestration/briefs/brief-merge.md:37,45-47` | 五门禁（4 条 cargo + 散文锚）；`--base walgit/master`；合并后以 master 为 base 会空 diff |

统一命令：

```sh
python -I -B tools/hygiene/docs_anchor_gate.py --prose --base walgit/master
```

口径：**在待合并/待审分支上跑**；`--base` 取**合并目标 ref**；只核新增行，历史悬空锚仍归
`stale_pointer_gate`，二者不可互相替代。

### 3.4 普查与台账

- `docs/gate-ref-flag-survey-20261102.md`：77 门逐门实跑分类（§1 结论、§3 hygiene、§4 parity、§7 收口）。
  该文档的 `--ref` 静默忽略从 5 → 0，由 `68e4c248e` + `f3c969ea7` 两步完成。
- 台账 `openmir2-real-machine-verification-master` 的窗口受阻 comment：
  `event=mcp-admin-window-blocked-desktop-locked`，oid
  `874122105c94241db0bb8f66324144ef1fb9233b`。它记录的是**排期受阻**，不改变任何未跑状态。

## 4. 仍未完成

> **未跑 ≠ 通过**。以下项目没有真机证据，不能写成已验收。

### 4.1 MCP admin 三条真机项：`NOT_RUN`，阻塞原因是桌面锁屏

执行单：`docs/mcp-admin-window-sheet-20261101.md`（全部 `NOT_RUN`）。三条判据：

1. `@怪`（`Imp::MobSpawn`）/ `@清除怪物`（`Imp::ClearMapMonster`）经 admin 通道执行，
   与脚本 `GMEXECUTE` 路径的抽数序列与帧面等价（真机双栈对拍）。
   注意 `@清除怪物` 恒回红且不抽数，只能作「同一红字 + 零新增抽数」的反向判据。
2. 真栈上 `AdminPort` 开/关两种配置下客户端帧面 `diff = 0`。
3. 抬权只作用于单次调用、不落盘：基线 / 单次调用后 / 重登后三条一致。

**阻塞证据**：

- 对端会话实测本机桌面锁屏：`OpenInputDesktop` 返回 0 / err=5（ACCESS_DENIED），
  `LockApp`(11472) 与 `LogonUI`(7328) 在场，`GetForegroundWindow()` 为空。
- 本方台账 comment：`event=mcp-admin-window-blocked-desktop-locked`（oid `87412210…`），
  status 为「继续 NOT_RUN；窗口暂时不可开」。
- 锁屏下客户端 SDL 收不到键盘焦点与鼠标 motion ⇒ 采到的帧面/RNG 证据不可信（见 §2.3）。

**解锁后的动作**：按 `docs/mcp-admin-window-sheet-20261101.md` 从头执行；
证据根 `E:/tmp/admin-window-20261101/` **当前不存在**（窗口未开，未创建）。
`T-273` 是 R 级卡，真机项未跑时保持 `in-progress`，不得按已验收收口。

### 4.2 `check_anchors.py` 两处既有 RED（已登记，未修）

在 `f3c969ea7` 树上复算：

```sh
cd mir2-rs && python -I -B tests/parity/m3/rng-trace-scene/check_anchors.py check
# rc=1
# RED ANCHOR_DRIFT … monai.rs:5227，但第一个匹配在 :5259
# RED ANCHOR_DRIFT … monai.rs:234，但第一个匹配在 :236
```

- 两处都在 `mir2-rs/crates/world/src/monai.rs`：文档写 `5227`，实际 `5259`；文档写 `234`，实际 `236`。
- 这是**既有锚点漂移**，与 `--ref` 参数处理无关；`f3c969ea7` 只修参数处理，按卡面要求只登记不修（另案重钉）。
- 注意：该门的 `check` 现在仍 rc=1，属**判据红**；它的 `--ref` 形态已是 rc=2 的**用法拒绝**，两者不同。

### 4.3 普查文档 §7.1 的行号未按 `path:line` 单块书写

`docs/gate-ref-flag-survey-20261102.md` §7.1 把路径与行号写成两个分开的反引号片段
（路径一个反引号，`:5227` 另一个反引号），而 `--prose` 的识别式要求**同一个反引号内**是
`path:line`。因此这两处行号**不受机器锚保护**：后续源码漂移时 `--prose` 会报「检查 0」，
不会报红。

- 现状：这是 §7.1 登记项本身的书写方式问题，不是 `--prose` 的漏判（域限制已在 §2.4 写清）。
- 建议另案：修 `monai.rs` 漂移时，把锚改成单一 `path:line` 形式，随后机器锚即可覆盖。
- 本次追加**不改**该普查文档（卡面 write_set 只含本文件）。

## 5. 本追加文档自身的门禁（rc 表）

在交付 head（本分支最后一次提交）上直取退出码；命令按书写顺序运行。
