# 交付验收 runbook：walgit 恢复 + owner 开窗 两条路径（2026-10-25）

- 卡：`openmir2-delivery-acceptance-runbook`（tier **L**；owner `openmir2-dev-1`；worktree `.worktrees/dev-1`；
  分支 `card/delivery-acceptance-runbook`）。
- 写集：**本文件**（只新增；不改既有文档，只互引）。
- 快照锚：master `2eb147e6e` @ 2026-10-25（**活量**；所有计数在每步重数，不把快照写成定值）。
- 定位：把「三天交付目标」收口成**逐步可执行**的验收清单，覆盖两条并行路径：
  **A) walgit 恢复后补推与机判复位**；**B) owner 开窗后真机对拍与 R 卡收口**。
- 红线：不把未发生的窗口/恢复写成已完成；不起服务端、不占端口；`E:/MirServer` 内容冻结。

## 0. 四条纪律（每步都适用）

1. **活量重数**：任何 `git rev-list --count` / board 分桶 / refs 计数，执行前重数；本文只给命令与输出样式。
2. **窗口独占**：标 🔒 的步骤必须独占真机窗口（起栈/抓帧/跑 RNG 账本），同一时刻只能一条线做；
   标 ⚡ 的可与其它线并行（只读/文档/推送）。
3. **失败即停**：任何一步 rc≠0，按该步「失败怎么办」处理；**不得**用上一轮的产物冒充本轮证据
   （`run_batch` 的 `_stale-*` 归档与 `head 已变 ⇒ 重跑` 就是这条的实现）。
4. **证据四件套**：命令 + 输出 + 路径 + hash，落到卡线程的 patch body 或报告的「证据」段。

## 1. 基线与复算（acceptance ③；⚡ 可并行）

| # | 命令（可直接复制） | 输出样式（活量） | 判读 |
|---:|---|---|---|
| 1.1 | `git rev-parse --short master refs/remotes/walgit/master` | 两行短 sha；恢复前第二行恒为 `4ae3dc810` | 两 sha 相同 ⇒ 默认 ref 口径；不同 ⇒ 见 §4 第①类 |
| 1.2 | `git rev-list --count refs/remotes/walgit/master..master` | 一个整数（今日快照 **91**，快照 @ `1c605ca4a`） | 本地领先量；恢复后应回到 0 |
| 1.3 | `git rev-list --count origin/master..master` | 一个整数（镜像落后量，活量） | GitHub 镜像同步量 |
| 1.4 | `git for-each-ref refs/heads/card --format='%(refname:short) %(objectname:short)'` | 每行 `分支 短sha`；今日快照 **7** 条（快照 @ `1c605ca4a`） | 逐条判「已合并 / 未合并」见 §2.2 |
| 1.5 | `walgit collab board --format json \| python -I -c "import sys,json;d=json.load(sys.stdin);[print(c['name'],len(c['cards'])) for c in d['columns']]"` | 每行 `看板列 张数`（六列；活量） | 与 §2.3 卡状态对账 |
| 1.6 | `python -I tools/goal/goal_gate.py --full --json > "$TEMP/gate.json"; echo rc=$?` | `rc=0`；JSON 里 `conditions[*].state` 四行 | 恢复前条件④ 的 `master_sync` 必为 need_human |
| 1.7 | `python -I -c "import json,os;d=json.load(open(os.path.join(os.environ['TEMP'],'gate.json')));print(d['overall']);[print(c['id'],c['name'],c['state']) for c in d['conditions']]"` | `overall` + 4 行 `id 名称 state` | 目标：①pass ②pass（G11 裁决后）③pass ④pass |

> 时间仓促时用 `python -I tools/goal/goal_gate.py --json`（快速模式）先看 ①②③；④ 的 `cargo test` 腿
> 必须 `--full`。

## 2. 路径 A：walgit 恢复后（⚡ 可并行；不需要窗口）

### 2.1 恢复判定（先做，否则后面全是误判）

| 步 | 命令 | 预期 | 失败怎么办 | 证据落哪 |
|---|---|---|---|---|
| A1 | `git ls-remote walgit refs/heads/master` | 一行 sha，且 `git ls-remote walgit` 不再 500 | 仍 500（`wal: object not found …`）⇒ 停在这里，回到 `docs/needs-human-20261025.md` §4（等恢复 / GitHub 临时主源） | 粘贴命令+输出到卡 |
| A2 | `git fetch walgit '+refs/collab/inbox/*:refs/collab/inbox/*' master` | `fetch` rc=0，`refs/remotes/walgit/master` 前移 | 部分 ref 冲突 ⇒ 记下远端 sha，后续推送按 fast-forward 判；不强推 | fetch 输出尾部 |

### 2.2 补推 master 与分支

| 步 | 命令 | 预期 | 失败怎么办 | 证据落哪 |
|---|---|---|---|---|
| A3 | `git push walgit master` | `master -> master` 且远端 sha == 本地 | non-fast-forward ⇒ 先 `git fetch walgit master` 再 `--ff-only` 合；**禁止** `--force` | push 输出 + `git ls-remote` |
| A4 | `git push walgit card/<名> …`（以 §1.4 现列为准，逐条） | 每条 `new branch` 或 `up to date` | 已合并分支推送仅作审计留存；失败不阻塞 A5 | push 输出 |
| A5 | `git push walgit 'refs/collab/inbox/*:refs/collab/inbox/*' 'refs/collab/meta/*:refs/collab/meta/*'` | 大量 `* [new ref]`，rc=0 | 单条 ref 拒绝 ⇒ 记录 sha 后重试（协作 refs 是活量） | push 输出 + `git for-each-ref refs/collab/inbox \| wc -l` |

### 2.3 机判与 CI 复位

| 步 | 命令 | 预期 | 失败怎么办 | 证据落哪 |
|---|---|---|---|---|
| A6 | `walgit ci run --once --task fast-stale-pointer` 等 `.walgit/ci.toml` 的 7 个 fast 任务 | 每个 `claimed→passed`；`rust-*` 四项留给夜间组 | 任一红 ⇒ 修/登记后重跑；`master_sync` 只有 A3 成功才绿 | ci 日志文件路径 |
| A7 | §1.6/§1.7 重跑 goal_gate --full | 条件④ `master_sync` 由 need_human 转 pass；其余三项与 A 前一致 | 出现新红 ⇒ 与 A 前快照 diff，先归因再动 | `$TEMP/gate.json` + 报告 |
| A8 | 看板对账（§1.5） | 「待审」清空、「已完成」= 合并数 | 卡状态滞后 ⇒ 用签名 `status` 收口（**不**直接改 board） | board json 输出 |

## 3. 路径 B：owner 开窗后（🔒 独占窗口；R 卡证据）

> 前置：`docs/needs-human-20261025.md` §2 的窗口授权已给（Q1），且 A1 不要求先恢复
>（walgit 故障期窗口仍可跑，只是补推另走 A）。

### 3.1 起栈与开窗前自检（🔒）

| 步 | 命令 | 预期 | 失败怎么办 | 证据落哪 |
|---|---|---|---|---|
| B1 | `powershell -NoProfile -ExecutionPolicy Bypass -File E:/MirServer/start-all.ps1` | 十端口监听；**勿加 `-Stop`**（会连 mysqld 一起停） | 端口占用 ⇒ `E:/MirServer/port-status.ps1` 定位后清进程；**不要**动 mysqld | 十端口状态输出 |
| B2 | `bash tools/window/run_batch.sh --dry-run` | 只读自检全绿 + 17 场景清单；不起栈 | 缺参/前置失败 ⇒ 按脚本输出修；`--dry-run` 不写产物 | dry-run 输出 |
| B3 | `bash tools/window/run_batch.sh --list` | 打印 17 场景（alt×16 + window-magic×1） | 清单与 `tools/window/run3-scenarios.json` 不一致 ⇒ 停，别跑 | 清单输出 |

### 3.2 真机对拍（🔒；R 卡按 `docs/window-closure-ledger-20261025.md` 逐行）

| 步 | 命令 | 预期 | 失败怎么办 | 证据落哪 |
|---|---|---|---|---|
| B4 | `bash tools/window/run_batch.sh --run-dir E:/tmp/window/run3 --timeout-sec 2700` | 批次 rc=0；`batch-summary.json` 里每场景 `PASS/FAIL`，**UNDECIDABLE 计失败** | rc=2 ⇒ 看 `report/card-window/<场景>/card-window.json` 的 `verdict/compare_rc`；**不得**手工改 verdict | `E:/tmp/window/run3/**` + sha256 |
| B5 | 带 RNG 账本的场景：`bash tools/window/run_batch.sh --rng-log …`（或在 B4 同参数） | 两侧 `MIR2_RNG_LOG` 的 site/order 流水逐位一致；缺账本 ⇒ 该场景 rc=2 | 流水差异 ⇒ 按卡的 W 项定位（如 W-MQ-2 四叉堆序、W-DG-2 两抽流位） | 两侧账本文件 + diff 输出 |
| B6 | 逐卡回填 `docs/window-closure-ledger-20261025.md` 对应行（证据路径 / 帧 sha256 / db 快照） | 每行「窗口状态」由 `window_required(in-progress)` 改为 `verified`（**只在该行证据确实入卡后**） | 证据不全 ⇒ 保持 in-progress，写「缺什么」 | 台账行 + 卡线程签名 `comment` |
| B7 | R 卡收口：签名 `status: closed`（`work` 写明 W 项与证据路径） | 卡的 `window_required` 桶随 `goal_gate` 反映 | 未全验 ⇒ **不 closed**，保持 `in-progress` | 线程条目 + goal_gate 输出 |
| B8 | 收窗：按窗口 runbook §5 的收尾（进程结束、端口复原、临时物登记） | 十端口与开窗前一致；`E:/tmp` 无本线专属残留 | 有残留 ⇒ 先登记到报告再清 | 端口状态 + 残留清单 |

### 3.3 收窗后复算（⚡）

| 步 | 命令 | 预期 | 失败怎么办 | 证据落哪 |
|---|---|---|---|---|
| B9 | §1.6/§1.7 重跑 goal_gate --full | 条件② 的 `window_required` 桶下降（closed 上升）；② 仅剩 G11 项 | 新红 ⇒ 归因到具体卡/场景 | `$TEMP/gate.json` |
| B10 | `docs/needs-human-20261025.md` 的 Q1 标记「已完成」并写窗口日期 | 决策页与看板一致 | 不一致 ⇒ 以看板签名条目为准 | needs-human 页 |

## 4. 卫生门禁口径（acceptance ④；⚡）

按 `docs/window-runbook-20261023.md` **§6** 的三类口径执行：

```sh
# 故障期 / 本地模式：带 ref 的两门显式 --ref master；其余按原样。
python -I tools/whitelist/t_reservation_gate.py check
python -I tools/hygiene/conflict_marker_gate.py
python -I -B tools/hygiene/stale_pointer_gate.py --ref master
python -I mir2-rs/tests/parity/m3/whitelist_anchor_gate.py check
python -I mir2-rs/tests/parity/m3/m3_whitelist_registry_audit.py audit
python -I -B tools/hygiene/dynamic_count_stamp_gate.py --ref master
python -I -B tools/hygiene/board_coverage_gate.py --board .walgit/board.toml
python -I mir2-rs/tests/parity/coverage/script_host_noop_audit.py check   # 台账门
python -I -B tools/hygiene/merge_tree_helper.py --selftest               # CI 第 8 项
git diff --check <base> <head>                                           # 带范围！
```

- **① 2 门接受 `--ref`**：`stale_pointer_gate` / `dynamic_count_stamp_gate`。
- **② 4 门硬传会 rc=2**：`t_reservation` / `whitelist_anchor` / `m3_registry` / `board_coverage`。
- **③ 2 门静默忽略（假绿）**：`conflict_marker_gate` / `script_host_noop_audit`——传了也 rc=0，
  故障期只能「不传 + 用 §1.1 确认默认 ref 已指向本地 master」。
- 恢复后（A1/A2 绿且 §1.1 两 sha 相同）⇒ 去掉 `--ref master`，回归 CI 默认口径。

## 5. 并行 / 窗口矩阵

| 步骤组 | 并行? | 说明 |
|---|---|---|
| §1 全部、A6 之后 | ⚡ | 只读/推送，不占端口 |
| A1–A5 | ⚡ | 推送不依赖窗口；恢复判定先做 |
| B1–B5、B8 | 🔒 | 起栈/抓帧/跑账本/收窗：同刻只一条线 |
| B6–B7、B9–B10 | ⚡ | 文档/卡状态/机判，可与其它线并行 |
| A7 与 B9 | ⚡ | 两次 goal_gate 可各跑各的；**不要**与 B1–B5 同机抢内存（tester 测得 OOM 风险） |

## 6. 互引（acceptance ②；本文件不复制它们的内容）

| 文档 | 引用点 |
|---|---|
| `docs/three-day-report-20261025.md` **§4** | 补推清单的「值/分支表」以该节为准；本文件 §2 只给**流程与判据** |
| `docs/window-runbook-20261023.md` **§6** / **§7.4**（batch-plan） | 门禁三类口径（§4 引用）；17 场景清单（§3 B2/B3 引用） |
| `docs/window-closure-ledger-20261025.md` | R 卡逐行登记与窗口状态（§3.2 B6 引用）；**登记 ≠ 已有真机证据** |
| `docs/needs-human-20261025.md` | G11 裁决 / Q1 窗口 / Q2–Q5 口径 / walgit 恢复（A1、§3 前置、B10 引用） |
| `docs/gate-baseline-*.md`、`docs/goal-gate-full-20261024.md` | goal_gate 的历史基线与四条件原文（§1.6/§1.7 对照） |

## 7. 失败处置总表（速查）

| 现象 | 处置 |
|---|---|
| `git ls-remote walgit` 500（`wal: object not found`） | A1 停下，走 needs-human §4；不重试推送 |
| `git push` non-fast-forward | `fetch` + `--ff-only` 合；**禁止 force** |
| `run_batch` rc=2 / 场景 UNDECIDABLE | 看 `card-window.json`；不得改 verdict；缺账本/缺参按脚本提示修 |
| `stale_pointer_gate` 红但文件未改 | **先**看 §1.1：两 sha 不同 ⇒ 加 `--ref master`；仍红 ⇒ 文档确实过期，改文档（走卡） |
| 两门（静默忽略）传了 `--ref` 仍绿 | 正常但不代表生效；改用 §1.1 判默认 ref |
| goal_gate ④ 的 `master_sync` need_human | A3 未完成；恢复前属预期，不是回归 |
| 内存压力 / 测试 OOM | 测试与 B1–B5 不同机并行；`cargo test -- --test-threads=2` |

## 8. 残余 / 未发生项（不写成已完成）

- 路径 A 的推送**未执行**（walgit 仍 500，本地模式）；本文只给步骤与判据。
- 路径 B 的窗口**未开**（W1/W2/W3 轮次见 `window-closure-ledger-20261025.md` §2）；本文件不声称任何
  R 卡已有真机证据。
- 条件② 的唯一 `need_human`（G11）仍待 owner 裁决（needs-human §1）；条件④ 的 `master_sync` 待 A3。
- 本文件没有新增脚本；所有命令均为既有工具/CI 任务的直接调用。

## 9. 门禁（本卡交付时）

- 只新增本文件；`git diff --check <base> <head>` rc=0；7 项卫生门禁（§4 口径）+ `script_host_noop_audit`
  + `merge_tree_helper --selftest` 全 rc=0；`merge_tree_helper` 的 vs_base / vs_master 值写进 patch body。
