# `dynamic_count_stamp_gate`：易漂计数必须带快照戳（2026-10-24）

- 卡：`openmir2-dynamic-count-stamp-gate`（tier=L）；写集 `tools/hygiene/`、`docs/`。
- 起因：`docs/` 里大量**会漂的数字**（看板列计数、R 卡总数、`FAIL` 计数、可跑场景数、recon `tracked` 计数）
  被当作现况引用；一天内已因此返工三次（见 `docs/leader-goal-20261023.md` §1.1「动态量必须带快照」）。
- 本门只做一件事：**同一行**上出现易漂计数、却**没有**邻近快照戳 ⇒ 红。它不判断数字对不对。

## 1. 判据（正则写死，`--selftest` 钉住）

**易漂计数**（`COUNT_PATTERNS`，中文按 Python unicode `re` 匹配，绝不用字节模式 `grep -P`）：

| 类别 | 形状（例） | 正则要点 |
|---|---|---|
| `board` | `进行中 <数字>`、`'待认领': <数字>`、`已完成 <数字>` | `待认领/进行中/待审/已完成` + 至多六个非数字分隔 + 数字（后面不能紧跟数字或 `)`） |
| `r-cards` | `R 卡 <数字> 张` | 必须带单位「张」，避免 `R 卡证据` 这类散文误报 |
| `fail` | `FAIL 卡 <数字>`、`FAIL <数字>` | 前面不能是字母或 `[`（排除 `[FAIL]`/`FAILED`），后面不能紧跟 `)`（排除 `FAIL (rc…)`） |
| `runnable` | `可跑场景 <数字>` | 数字紧跟 |
| `tracked` | `tracked=<数字>`、`tracked: <数字>` | 数字后不能是十六进制字符（排除 `tracked files … 5a6ed54c1` 这类哈希） |

**快照戳**（`STAMP_RE`，必须在计数前后 `--max-gap`（默认 24）字符内、**同一行**）：

```text
@ <sha7..40>        截至 <sha7..40>      as of <sha7..40>
@ 20YY-MM-DD        截至 20YY-MM-DD      as of 20YY-MM-DD
快照…<sha>          快照…20YY-MM-DD
```

例：`完成 <数字>（截至 3738ad234）` 绿；`完成 <数字>` 红；计数与 `@ 3738ad234` 之间超过 24 字符也红
（把 `--max-gap` 调大才绿——这条边界由 selftest ③/③b 钉住）。

## 2. 退出码与「不许静默降级」

| rc | 含义 |
|---:|---|
| 0 | 全部计数都带戳（或 selftest 通过） |
| 1 | 有缺戳命中，或有 allow-list 条目**腐烂**（压不到任何现行命中） |
| 2 | 用法错误；扫描目标不存在 / allow-list 不可读 / Markdown 非法 UTF-8 / `--path` 指向仓库外（含跨盘）——读不到一律 fail-closed，绝不当绿 |

`--no-stamp-check` 会把检查关掉，但**必须**在 stdout 打印 `stamp-check: 已跳过（--no-stamp-check）`；
不存在静默降级的路径（selftest ⑥ 断言这条打印 + rc=0）。`--path` 越界（仓外/跨盘）走 `resolve_scan_root()`，
`ValueError` 与骨架的 `GateError` 都收敛成 rc=2，**不会**变成 traceback + rc=1。

## 3. allow-list（`tools/hygiene/dynamic_count_allowlist.toml`）

条目形状（四项缺一 ⇒ rc=2）：

```toml
[[allow]]
path = "docs/foo.md"
line_sha256 = "<64 hex：整行去尾空白后的 sha256>"
reason = "为什么这一行必须保留裸计数"
added = "2026-10-24"
```

- 生成指纹：`python -I tools/hygiene/dynamic_count_stamp_gate.py --fingerprint docs/foo.md:123`
- **rot 检测**：条目压不到现行命中 ⇒ 报 `ROT` 并 rc=1；白名单不会静默腐烂。
- **rot 只看本次扫描子树**：`--path docs/x` 时，子树外的条目既不会被用掉、也不算腐烂（selftest ⑨/⑩）。
  这条作用域复用骨架的 `allowlist_in_scope()`，不是本门自己再写一遍。
- 校验同样复用骨架：`load_allowlist(require_added=True)` —— 缺 `path` / `line_sha256` / `reason` / `added`
  四项缺一 ⇒ rc=2，且**由 selftest ⑬–⑰ 钉住**（不只在运行期校验）。
- 现行 **92 条**（2026-10-24 第③轮返工后；R3-2 订正：此处此前写 86，而 §6.2/§10 当时实为 87，本轮再 +5 条
  `docs/board-merged-column-20261024.md` 的实时看板对比表 ⇒ 92）。理由按四类写实：文档级快照（头部已声明时点）、逐字机判摘录
  （`goal_gate --json` 的 detail）、规则文本（不是现时计数）、历史里程碑叙述。**引述**（`「可跑场景 16」`）
  已由门直接放行，不再占白名单。

## 4. 自检（`--selftest`，21/21，当天实跑）

```text
PASS ① 带戳的计数 => 绿
PASS ② 裸计数 => 红
PASS ③ 戳在 --max-gap 之外 => 红
PASS ③b 同一条把 --max-gap 放大 => 绿
PASS ④ --path 缩窄生效（全树 2 红 vs 子目录 1 红）
PASS ⑤ 扫描目标不存在 => rc=2（fail-closed）
PASS ⑥ --no-stamp-check => 打印「已跳过」且 rc=0
PASS ⑦ allow-list 带理由+日期 => 抑制该命中
PASS ⑧ allow-list 条目压不到命中 => rot（rc=1 路径）
PASS ⑨ 非空 allow-list + 收窄 --path（不覆盖任何条目）=> 不报 rot、rc=0
PASS ⑩ 收窄覆盖某条目 => 该条目已用、子树外条目不判 rot
PASS ⑪ 引述「可跑场景 16」=> 绿（引述豁免）
PASS ⑪b 引述外仍有裸计数 => 红
PASS ⑪c 引述内的词不吞引述外数字（「待认领」指 §17.1）=> 绿
PASS ⑫ 扫描根在仓库外 => rc=2（不是 traceback）
PASS ⑬ 合法条目 => 载入成功（含 added）
PASS ⑭ 缺 reason => rc=2
PASS ⑮ 缺 added => rc=2
PASS ⑯ 坏 sha => rc=2
PASS ⑰ 重复条目 => rc=2
PASS ⑱ 枚举只用 git 跟踪集（未跟踪的裸计数不扫；改 os.walk 即红）
selftest: 21/21 passed
```

## 5. 六颗牙（串行；还原一律 `git checkout --`，索引即还原源）

在打补丁后的 head 上实跑，基线 `--selftest` = **21/21**：

```text
# (a) 判据恒绿：missing = [] ⇒ ②③④⑦⑨⑩⑪b 红
selftest: 14/21 passed      # rc=1

# (b) --max-gap 改成无限：DEFAULT_MAX_GAP = 10 ** 9 ⇒ ③ 红
FAIL ③ 戳在 --max-gap 之外 => 红
selftest: 20/21 passed      # rc=1

# (c) 删掉「缺戳即红」分支（if not missing → if True）⇒ ②③④⑦⑨⑩⑪b 红
selftest: 14/21 passed      # rc=1

# (d) 删掉 rot 检测（rotten = []）—— reviewer-3 自造的同一颗牙 ⇒ ⑧ 红
FAIL ⑧ allow-list 条目压不到命中 => rot（rc=1 路径）
selftest: 20/21 passed      # rc=1

# (e) 去掉 rot 的子树作用域（allowlist_in_scope(...) → allowlist）⇒ ⑨⑩ 红
FAIL ⑨ 非空 allow-list + 收窄 --path（不覆盖任何条目）=> 不报 rot、rc=0
FAIL ⑩ 收窄覆盖某条目 => 该条目已用、子树外条目不判 rot
selftest: 19/21 passed      # rc=1

# (f) 枚举改 os.walk（skeleton.markdown_files → 目录遍历）—— reviewer-3 M1 的原始变异 ⇒ ⑱ 红
FAIL ⑱ 枚举只用 git 跟踪集（未跟踪的裸计数不扫；改 os.walk 即红）
selftest: 20/21 passed      # rc=1
```

**六颗牙串行**执行；还原一律 `git checkout -- tools/hygiene/dynamic_count_stamp_gate.py`
（改前先 `git add`，索引即还原源），每颗还原后立即复跑 21/21。还原后全量扫 docs 也 rc=0。
(f) 是 M1 的原始变异：补牙前它 **20/20 全绿**（无牙），补 ⑱ 后才红。

## 6. 首次接入分类（冒烟，本轮不清零）

首跑（base `3738ad234`，head `1679b4fa9`）**85 处命中 / 84 条唯一行 / 28 个文件**
（同一行重复出现时指纹相同，合并为一条）。下表 §6.1 是那一刻的**历史快照**；现行数字见 §6.2。
处置：**0 处就地补戳，84 条全部进 allow-list**。为什么不在原地补：

- 命中的文件几乎都是**历史快照或逐字机判摘录**（`55a8e7fca`、`81ecc0d28` 等时点的 R 卡计数；
  `goal_gate --json` 的逐字 detail；看板列的历史值）。插戳要么破坏「逐字摘录」的可核对性，
  要么等于改写历史；两者都比「白名单 + 理由」更坏。
- 今天仍在维护的文档（例如本文件、`docs/needs-human-20261024.md` 的决策表）以后**新增**计数时按 §1 带戳即可。

| 文件 | 命中 |
|---|---:|
| `docs/rewrite-remaining-inventory-20261022.md` | 22 |
| `docs/goal-gate-full-20261024.md` | 6 |
| `docs/leader-goal-20261023.md` | 6 |
| `docs/needs-human-20261024.md` | 5 |
| `docs/交接-OpenMir2编排与联调-20261015.md` | 5 |
| `docs/Rust重写设计文档.md` | 4 |
| `docs/goal-snapshot-late-20261023.md` | 3 |
| `docs/recon-noop-false-positive-20261023.md` | 3 |
| 其余 20 个文件（每个 1–2 处） | 31 |

分类计数（按匹配类别）：`board` 34 / `tracked` 22 / `board`+`r-cards` 19 / `fail` 6 / `r-cards` 2 /
`runnable` 1 / `fail`+`r-cards` 1。

### 6.1 首跑逐条（历史快照：85 处；当时全部进 allow-list，`added=2026-10-24`）

| # | 位置 | 类别 | 处置 |
|---:|---|---|---|
| 1 | `docs/Rust重写设计文档.md:754` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 2 | `docs/Rust重写设计文档.md:813` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 3 | `docs/Rust重写设计文档.md:861` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 4 | `docs/Rust重写设计文档.md:886` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 5 | `docs/docs-state-drift-sweep-20261024.md:142` | runnable | ~~allow-list~~ **已撤出**：引述豁免直接放行（§9.2 ③）；条目撤出后 rot 检测会立刻抓住，见 §6.2 |
| 6 | `docs/goal-gate-card-association-20261023.md:70` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 7 | `docs/goal-gate-full-20261024.md:12` | fail,r-cards | allow-list（`added=2026-10-24`，理由见 toml） |
| 8 | `docs/goal-gate-full-20261024.md:13` | tracked | allow-list（`added=2026-10-24`，理由见 toml） |
| 9 | `docs/goal-gate-full-20261024.md:26` | board,r-cards | allow-list（`added=2026-10-24`，理由见 toml） |
| 10 | `docs/goal-gate-full-20261024.md:29` | tracked | allow-list（`added=2026-10-24`，理由见 toml） |
| 11 | `docs/goal-gate-full-20261024.md:46` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 12 | `docs/goal-gate-full-20261024.md:111` | fail | allow-list（`added=2026-10-24`，理由见 toml） |
| 13 | `docs/goal-gate-unmerged-selftest-20261024.md:59` | board,r-cards | allow-list（`added=2026-10-24`，理由见 toml） |
| 14 | `docs/goal-gate-unmerged-window-bucket-20261024.md:29` | board,r-cards | allow-list（`added=2026-10-24`，理由见 toml） |
| 15 | `docs/goal-gate-unmerged-window-bucket-20261024.md:36` | board,r-cards | allow-list（`added=2026-10-24`，理由见 toml） |
| 16 | `docs/goal-snapshot-late-20261023.md:14` | board,r-cards | allow-list（`added=2026-10-24`，理由见 toml） |
| 17 | `docs/goal-snapshot-late-20261023.md:15` | tracked | allow-list（`added=2026-10-24`，理由见 toml） |
| 18 | `docs/goal-snapshot-late-20261023.md:307` | fail | allow-list（`added=2026-10-24`，理由见 toml） |
| 19 | `docs/leader-goal-20261023.md:31` | tracked | allow-list（`added=2026-10-24`，理由见 toml） |
| 20 | `docs/leader-goal-20261023.md:41` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 21 | `docs/leader-goal-20261023.md:188` | tracked | allow-list（`added=2026-10-24`，理由见 toml） |
| 22 | `docs/leader-goal-20261023.md:193` | tracked | allow-list（`added=2026-10-24`，理由见 toml） |
| 23 | `docs/leader-goal-20261023.md:252` | tracked | allow-list（`added=2026-10-24`，理由见 toml） |
| 24 | `docs/leader-goal-20261023.md:254` | tracked | allow-list（`added=2026-10-24`，理由见 toml） |
| 25 | `docs/leader-loop-20261023.md:100` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 26 | `docs/leader-loop-20261023.md:129` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 27 | `docs/needs-human-20261023-evening.md:3` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 28 | `docs/needs-human-20261023-evening.md:41` | tracked | allow-list（`added=2026-10-24`，理由见 toml） |
| 29 | `docs/needs-human-20261023-night.md:68` | fail | allow-list（`added=2026-10-24`，理由见 toml） |
| 30 | `docs/needs-human-20261024.md:32` | fail | allow-list（`added=2026-10-24`，理由见 toml） |
| 31 | `docs/needs-human-20261024.md:39` | r-cards | allow-list（`added=2026-10-24`，理由见 toml） |
| 32 | `docs/needs-human-20261024.md:75` | board,r-cards | allow-list（`added=2026-10-24`，理由见 toml） |
| 33 | `docs/needs-human-20261024.md:80` | fail | allow-list（`added=2026-10-24`，理由见 toml） |
| 34 | `docs/needs-human-20261024.md:221` | fail | allow-list（`added=2026-10-24`，理由见 toml） |
| 35 | `docs/r-card-evidence-status-20261021.md:46` | r-cards | allow-list（`added=2026-10-24`，理由见 toml） |
| 36 | `docs/r-card-evidence-sweep-20261023.md:15` | board,r-cards | allow-list（`added=2026-10-24`，理由见 toml） |
| 37 | `docs/r-card-evidence-sweep-20261023.md:24` | board,r-cards | allow-list（`added=2026-10-24`，理由见 toml） |
| 38 | `docs/r-card-evidence-triage-20261023.md:58` | board,r-cards | allow-list（`added=2026-10-24`，理由见 toml） |
| 39 | `docs/r-window-registry-batch-1-20261023.md:32` | board,r-cards | allow-list（`added=2026-10-24`，理由见 toml） |
| 40 | `docs/r-window-registry-batch-1-20261023.md:42` | board,r-cards | allow-list（`added=2026-10-24`，理由见 toml） |
| 41 | `docs/r-window-registry-batch-2-20261023.md:27` | board,r-cards | allow-list（`added=2026-10-24`，理由见 toml） |
| 42 | `docs/r-window-registry-batch-2-20261023.md:35` | board,r-cards | allow-list（`added=2026-10-24`，理由见 toml） |
| 43 | `docs/r-window-registry-batch-3-20261023.md:33` | board,r-cards | allow-list（`added=2026-10-24`，理由见 toml） |
| 44 | `docs/r-window-registry-batch-3-20261023.md:42` | board,r-cards | allow-list（`added=2026-10-24`，理由见 toml） |
| 45 | `docs/r-window-registry-batch-4-20261023.md:32` | board,r-cards | allow-list（`added=2026-10-24`，理由见 toml） |
| 46 | `docs/r-window-registry-batch-4-20261023.md:40` | board,r-cards | allow-list（`added=2026-10-24`，理由见 toml） |
| 47 | `docs/r-window-registry-closeout-20261023.md:48` | board,r-cards | allow-list（`added=2026-10-24`，理由见 toml） |
| 48 | `docs/recon-noop-false-positive-20261023.md:158` | tracked | allow-list（`added=2026-10-24`，理由见 toml） |
| 49 | `docs/recon-noop-false-positive-20261023.md:168` | tracked | allow-list（`added=2026-10-24`，理由见 toml） |
| 50 | `docs/recon-noop-false-positive-20261023.md:183` | tracked | allow-list（`added=2026-10-24`，理由见 toml） |
| 51 | `docs/rewrite-remaining-inventory-20261022.md:18` | tracked | allow-list（`added=2026-10-24`，理由见 toml） |
| 52 | `docs/rewrite-remaining-inventory-20261022.md:21` | tracked | allow-list（`added=2026-10-24`，理由见 toml） |
| 53 | `docs/rewrite-remaining-inventory-20261022.md:22` | tracked | allow-list（`added=2026-10-24`，理由见 toml） |
| 54 | `docs/rewrite-remaining-inventory-20261022.md:23` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 55 | `docs/rewrite-remaining-inventory-20261022.md:36` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 56 | `docs/rewrite-remaining-inventory-20261022.md:215` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 57 | `docs/rewrite-remaining-inventory-20261022.md:218` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 58 | `docs/rewrite-remaining-inventory-20261022.md:219` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 59 | `docs/rewrite-remaining-inventory-20261022.md:240` | tracked | allow-list（`added=2026-10-24`，理由见 toml） |
| 60 | `docs/rewrite-remaining-inventory-20261022.md:250` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 61 | `docs/rewrite-remaining-inventory-20261022.md:526` | tracked | allow-list（`added=2026-10-24`，理由见 toml） |
| 62 | `docs/rewrite-remaining-inventory-20261022.md:527` | tracked | allow-list（`added=2026-10-24`，理由见 toml） |
| 63 | `docs/rewrite-remaining-inventory-20261022.md:530` | tracked | allow-list（`added=2026-10-24`，理由见 toml） |
| 64 | `docs/rewrite-remaining-inventory-20261022.md:534` | tracked | allow-list（`added=2026-10-24`，理由见 toml） |
| 65 | `docs/rewrite-remaining-inventory-20261022.md:630` | tracked | allow-list（`added=2026-10-24`，理由见 toml） |
| 66 | `docs/rewrite-remaining-inventory-20261022.md:652` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 67 | `docs/rewrite-remaining-inventory-20261022.md:677` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 68 | `docs/rewrite-remaining-inventory-20261022.md:705` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 69 | `docs/rewrite-remaining-inventory-20261022.md:717` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 70 | `docs/rewrite-remaining-inventory-20261022.md:798` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 71 | `docs/rewrite-remaining-inventory-20261022.md:853` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 72 | `docs/rewrite-remaining-inventory-20261022.md:955` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 73 | `docs/session-summary-20261023.md:12` | tracked | allow-list（`added=2026-10-24`，理由见 toml） |
| 74 | `docs/session-summary-20261023.md:164` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 75 | `docs/window-batch-plan-20261023.md:135` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 76 | `docs/window-batch-plan-20261023.md:170` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 77 | `docs/window-criteria-quality-audit-20261024.md:16` | board,r-cards | allow-list（`added=2026-10-24`，理由见 toml） |
| 78 | `docs/window-debt-reclass-20261023.md:409` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 79 | `docs/window-debt-reclass-20261023.md:492` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 80 | `docs/window-owner-decisions-20261018.md:10` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 81 | `docs/交接-OpenMir2编排与联调-20261015.md:12` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 82 | `docs/交接-OpenMir2编排与联调-20261015.md:13` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 83 | `docs/交接-OpenMir2编排与联调-20261015.md:16` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 84 | `docs/交接-OpenMir2编排与联调-20261015.md:44` | board | allow-list（`added=2026-10-24`，理由见 toml） |
| 85 | `docs/交接-OpenMir2编排与联调-20261015.md:152` | board | allow-list（`added=2026-10-24`，理由见 toml） |

### 6.2 返工后（base `e9d00c6b3`，现行判据）

全量扫 **140 个 markdown → 88 处命中 / 87 条唯一行 / 28 个文件**（同一行重复出现时指纹相同、合并为一条：
`docs/rewrite-remaining-inventory-20261022.md` 的 recon `tracked` 摘录在 `:215` 与 `:1102` 各命中一次），
allow-list **87 条全部压到现行命中**（`rot=0`，rc=0）。与 §6.1（`3738ad234`）的差异有三处来源：

1. **引述豁免**：`docs/docs-state-drift-sweep-20261024.md:142`（「可跑场景 16」是引述）由红转绿，对应条目撤出 →
   命中 85→84、文件面 -1。类别计数里 `runnable` 从 1 变成 0。
2. **master 前进 `3738ad234` → `dc6e96995`**：新落地的文档带来 3 处 —— `docs/rewrite-remaining-inventory-20261022.md:727`
   （当时的 minimap 逐字摘录）、`docs/session-digest-20261024.md:18`（看板列，文档级快照）、
   `docs/session-digest-20261024.md:171`（**假红**：正则把「在待审/在写，1 张」的 `1` 当成待审数）。
3. **master 再前进 `dc6e96995` → `e9d00c6b3`**（`5634ec03d` B 表审计 + runbook 批次）：
   **+1 处新命中** —— `docs/rewrite-remaining-inventory-20261022.md:464`（B 表审计当次 `walgit collab board --format json`
   的逐字输出）；**同一行内容变化并下移** —— minimap 摘录由 `:727` 挪到 `:1028`，白名单按新指纹重登
   （`ed38b665…` → `c9626857…`；旧指纹未被任何命中压到，本门会先报 `rot`）。

净结果 84 + 3 + 1 = **88 处 / 87 行 / 28 文件**，87 条 allow-list 一一对应。分类（按类别）：
`board` 37 / `board`+`r-cards` 19 / `tracked` 23 / `fail` 6 / `r-cards` 2 / `fail`+`r-cards` 1 / `runnable` 0。

> `docs/rewrite-remaining-inventory-20261022.md` 是**另一张卡的写集**，本卡不改它；该条目按当前行指纹登记，
> 那一卡合入后若行内容变化，本门会先报 rot（这正是 rot 检测的用途），按新指纹重登即可。

## 7. 这个门抓不到什么（别当它是计数审计）

- **跨行戳**：戳在下一行/段落标题里就判不到（窗口不跨行）；这类要么挪到同一行，要么进白名单。
- **同义表述**：「R 级卡共 …」「待办 3 张」不命中（只认 §1 的固定形状）；想覆盖就扩 `COUNT_PATTERNS` 并补 selftest。
- **戳本身过期**：`@ 某个很旧的 commit` 也算「有戳」——它证明「当时是这个数」，不证明「现在还是」。
- **代码块/引用块**：没有跳过，代码里的 `tracked=<数字>` 同样会被要求带戳（真实命中里就有这类）。
- **引述**：只认 `「…」`/`『…』`（骨架同一个 `QUOTE_RE`），不认 `""`/`''`/`<q>`；判定看**计数起始词**是否在引述内，
  所以 `「未开始/待认领」指 §17.1` 不会把 `§1` 当计数，但引述结束后紧跟的 `进行中 <数字>` 仍会红。
- **未跟踪文件**：文件枚举走骨架的 `git ls-files`（跟踪集），**新建但没 `git add` 的 `.md` 不会被扫**；
  这也是「仓内非法 UTF-8 ⇒ rc=2」的探针要先 `git add -N` 才能复现的原因。
- **只扫 Markdown**：`.toml`/`.json`/`.rs` 里的计数不在本门范围（账本类由 `board_coverage_gate` / `goal_gate` 管）。
- **数字对不对**：本门只判「有没有戳」，不判计数与账本是否一致（那是 `board_coverage_gate`、`goal_gate` 的活）。

## 8. 复算命令

```sh
python -I tools/hygiene/dynamic_count_stamp_gate.py --selftest          # 21/21
python -I tools/hygiene/dynamic_count_stamp_gate.py                     # 全量 docs（当前应为 OK：140 文件 / 0 缺戳）
python -I tools/hygiene/dynamic_count_stamp_gate.py --path docs/dynamic-count-stamp-gate-20261024.md   # 缩窄到单个文件
python -I tools/hygiene/dynamic_count_stamp_gate.py --path docs/zz_probe # 缩窄到空/未跟踪目录：0 文件、rc=0（不再误报 rot）
python -I tools/hygiene/dynamic_count_stamp_gate.py --no-stamp-check     # 打印「已跳过」并 rc=0
python -I tools/hygiene/dynamic_count_stamp_gate.py --fingerprint docs/foo.md:123
```

`docs/` 当前没有子目录，所以缩窄示例用**单个文件**；换成任何存在的子目录同样成立（selftest ④/⑨/⑩ 用合成子树钉住）。
`docs/zz_probe` 需自行 `mkdir`（收窄到不存在目录是 rc=2，不是"跳过"）。

## 9. 返工记录（reviewer-3 `request_changes` on `fb80ea1f17cf`，被审 head `1679b4fa9`）

本轮 base = `walgit/master = dc6e969958cba05b2593a3100d8412c022118c2b`。**先说保留的部分**：`rc=2` 面
（目标不存在 / allow-list 不存在 / 缺 reason / 缺 added / 坏 sha / 仓内非法 UTF-8）与「与骨架的 rc/allow-list/rot/selftest 形状」
都被复核认可，本轮不改语义，只改实现路径。

### 9.1 F1（Important）rot 没按扫描子树过滤

原实现把 allow-list 全部条目当「必须在本轮被用掉」，于是**任何 `--path` 收窄都报 84 条 ROT、rc=1**；
selftest ④ 用的是**空 allow-list**，正好盖住了这条路径。

修法：复用骨架的 `allowlist_in_scope()`（本轮把它从 `stale_pointer_gate.scan()` 的内联块提为模块级函数，
骨架行为不变、30/30 selftest 仍过），rot 只取「扫描根 == 条目 path」或「条目 path 在扫描根下」的条目。

实测对照（同一 checkout、同一 allow-list）：

```text
# 收窄前（head 1679b4fa9）
$ mkdir -p docs/zz_probe && echo x > docs/zz_probe/a.md
$ python -I tools/hygiene/dynamic_count_stamp_gate.py --path docs/zz_probe
ROT docs/Rust重写设计文档.md: line_sha256=2bec82c836b9f857 未抑制任何命中（allow-list 腐烂）
...（共 84 条）
dynamic-count gate: FAIL (0 缺戳 / 84 allow-list 腐烂；扫描 1 个 markdown，path=docs/zz_probe，max-gap=24，...)
rc=1

# 收窄后（本 head）
$ python -I tools/hygiene/dynamic_count_stamp_gate.py --path docs/zz_probe
dynamic-count gate: OK（0 缺戳；扫描 0 个 markdown，path=docs/zz_probe，max-gap=24）
rc=0

$ python -I tools/hygiene/dynamic_count_stamp_gate.py --path docs/dynamic-count-stamp-gate-20261024.md
dynamic-count gate: OK（0 缺戳；扫描 1 个 markdown，path=docs/dynamic-count-stamp-gate-20261024.md，max-gap=24）
rc=0
```

（`docs/zz_probe` 里那 1 个 `.md` 未跟踪，所以骨架的 `git ls-files` 枚举返回 0 个文件；见 §9.2 第 3 点。）
selftest 新增 ⑨（非空 allow-list + 收窄不覆盖任何条目 ⇒ 不报 rot、rc=0）与 ⑩（收窄覆盖某条目 ⇒ 该条目 used、
子树外条目不判 rot）；牙 (e) 证明这两条有牙：去掉作用域过滤后 ⑨⑩ 立刻红（18/20）。

### 9.2 F2（Important）没有真的复用骨架，且复制已分叉 —— 选 a）真复用

本门现在 `import stale_pointer_gate as skeleton`，共用 `GateError`、`repo_root`、`repo_relative`、
`markdown_files`（`git ls-files` 跟踪集）、`line_fingerprint`、`load_allowlist`、`allowlist_in_scope`、`QUOTE_RE`；
本卡只剩判据：`STAMP_RE`、`COUNT_PATTERNS`、专属 selftest 与输出格式。

**为什么选 a）**：三处已观测分叉里有两处是「复制之后骨架修了、副本没跟上」（rot 作用域、仓外路径守卫），
这正是复制会持续付的代价；抽公共模块（reviewer 的另一选项）要同时改两条线的引用，写集更大。

为复用对 `stale_pointer_gate.py` 做了**两处行为不变**的改动（30/30 selftest 仍过、全量扫仍 rc=0）：

| 改动 | 为什么行为不变 |
|---|---|
| `load_allowlist(path, require_added=False, prog="stale-pointer gate")` | 两个参数都有默认值；默认路径下错误文本、返回字段、异常类型与原来逐字一致；`require_added=True` 时才要求 `added` 并在返回项里带 `added`，`prog` 只换错误前缀 |
| 抽出 `allowlist_in_scope(allowlist, rel_path)`，`scan()` 改调它 | 与原内联块的 `prefix` 判定逐行等价（含 `rel_path` 为 `""`/`"."` 时检查全部条目） |

三处分叉逐项消除：

1. **仓外 / 跨盘 `--path`**：新增 `resolve_scan_root()`，捕获跨盘时 `ntpath.relpath` 抛的 `ValueError`
   以及骨架的 `GateError`，统一 rc=2。实测：
   `--path C:/…` → `is on another drive than the repository … (fail-closed)` rc=2；
   `--path ../wt-worker-66` → `is outside the repository … (fail-closed)` rc=2。
   **注**：骨架自身在跨盘时也没接 `ValueError`（同样会 traceback + rc=1）；本门在**调用点**兜住，没有改骨架行为。
2. **引述豁免**：复用骨架 `QUOTE_RE`（`「…」`/`『…』`），判定**计数起始词**是否落在引述内 —— 与骨架
   `candidate_cards` 判定状态词的口径一致。效果：`docs/docs-state-drift-sweep-20261024.md:142`（「可跑场景 16」）
   由红转绿并**撤出白名单**；`行内「未开始/待认领」指 §17.1`（`stale-pointer-gate-20261024.md:229`）也不再误报。
   selftest ⑪/⑪b/⑪c 钉住「引述内绿、引述外仍红、引述词不吞引述外数字」。
3. **文件枚举**：改用骨架 `markdown_files`（`git ls-files`），不再 `os.walk`。**已知后果**：未跟踪的 `.md` 不扫；
   非法 UTF-8 探针需先 `git add -N`（实测仍 rc=2：`cannot read … (fail-closed)`）。已写进 §7 盲区。

与骨架的**已知差异清单**（每处为什么必要）：

| 差异 | 为什么必要 |
|---|---|
| 没有 `--ref` / 账本 / 卡状态 / 快照声明检查 | 判据不同：本门只判「计数有没有戳」，不做 git 祖先查询；引入账本会让纯文档检查依赖 `refs/collab/*` |
| `resolve_scan_root()` 把 `ValueError` 收敛为 rc=2 | 骨架的 `repo_relative` 在跨盘时抛 `ValueError`；不复用其行为缺陷 |
| 扫描目标存在性检查放在 `scan()` 而非 `main()` | selftest 要能在不建 git 仓的临时目录里复现 rc=2 |
| `--no-stamp-check` 打印「已跳过」 | 卡面 ② 要求（骨架无此开关） |
| 输出格式 `file:line<TAB>片段<TAB>缺快照戳（类别）` | 卡面 ① 的输出约定；rc 语义与骨架一致 |

### 9.3 F3（Minor）10 条 2026-10-24 文档的 `reason` 日期不实

原模板话术「2026-10-15～2026-10-23 时点记录」与文件实际快照不符。按各文件头部的真实快照逐条改写：

| 文件 | 条数 | 真实快照（写进 reason） |
|---|---:|---|
| `docs/docs-state-drift-sweep-20261024.md` | 1 | 条目**已撤出**（引述豁免覆盖），无需 reason |
| `docs/goal-gate-unmerged-selftest-20261024.md` | 1 | `walgit/master = a30a6fa79`（2026-10-24） |
| `docs/goal-gate-unmerged-window-bucket-20261024.md` | 2 | `master = 2000e1cc5`（2026-10-24） |
| `docs/needs-human-20261024.md` | 5 | `master = 55a8e7fca`（2026-10-24） |
| `docs/window-criteria-quality-audit-20261024.md` | 1 | 审计基线 HEAD `2a5101606`（截至 2026-10-24 01:06） |

其余条目的模板日期（10-15～10-23）与文件实际时点（10-15/10-18/10-21/10-22/10-23）相符，保留。

### 9.4 F4（Minor）selftest 不覆盖 allow-list 的 `reason`/`added` 校验

新增 ⑬–⑰：合法条目载入（且返回项带 `added`）/ 缺 `reason` / 缺 `added` / 坏 sha / 重复条目 ⇒ 后四者 rc=2。
这四条断言直接走 `load_allowlist(require_added=True)`；把 `require_added` 分支删掉，⑮ 立刻红。

### 9.5 F6（Minor）报告 §8 的 `--path docs/m2` 目录不存在

改为实跑可用的 `--path docs/dynamic-count-stamp-gate-20261024.md`（`docs/` 当前没有子目录），
并补一条「收窄到空/未跟踪目录 ⇒ rc=0」的示例（与 §9.1 对照）。

### 9.6 本轮未处理：F5（board 正则精度，Minor）

`docs/Rust重写设计文档.md` 的 4 条命中（`:754` hash 前缀 `3`、`:813` 日期 `2026`、`:861` 任务号 `14`、`:886` 日期 `2026`）
是**正则噪声**而不是计数。收紧分隔符/数字后瞻属于**判据变更**（会同时改变真阳面），不在本轮返工范围内；
这 4 条继续留在白名单，reason 已标注「假红（非计数）」。留给下一轮单独判（需要负向夹具 + 全量重分类）。

## 10. 返工记录 ②（reviewer-3 `request_changes` on `bfb42a3e`，被审 head `7baa268fc`）

上一轮（§9）的 F1/F2 已被复核认可；本轮只剩一条**不在被审分支上**的阻塞项 —— **合并结果树是红的** ——
外加一条 Minor（枚举口径没有牙）。判据未改，只做机械返工与补牙。

### 10.1 I1（Important）合并树上本门 rc=1

master 已从评审 base `dc6e96995` 前进到 `5634ec03d`（B 表审计合入），`git merge-tree` 本身无冲突，
但在**合并结果树**上本门报 `FAIL (2 缺戳 / 1 allow-list 腐烂)`。根因是只在旧 base 上跑过门。

修法：**rebase 到 `walgit/master = e9d00c6b3`**（不是停在 `5634ec03d`；之后还合了 runbook 批次），
再按新树上的行重算 allow-list（**不碰 inventory 文档本身** —— 它是另一张卡的写集）：

| 位置 | 新指纹 | 处置 |
|---|---|---|
| `docs/rewrite-remaining-inventory-20261022.md:1028` | `c9626857…` | 把旧 `:727` 的 `ed38b665…` 换掉（同一 minimap 摘录，内容变化并下移） |
| `docs/rewrite-remaining-inventory-20261022.md:464` | `d4de3f85…` | **新增**条目，`added=2026-10-24`，理由：B 表审计当次 `board --format json` 的逐字输出 |

为什么必须 rebase 而不能只在旧 base 上加条目：这两行在旧 base 上不存在（`:464` 是 B 表审计新增；
minimap 行在旧 base 是 `:727`），只加条目会立刻被判 `rot`。

实测（rebase 后）：

```text
$ python -I tools/hygiene/dynamic_count_stamp_gate.py --fingerprint docs/rewrite-remaining-inventory-20261022.md:464
d4de3f8527f29c2e2fd576df7be92a78b571ed3d4067f508f8662f56dc861fe8
$ python -I tools/hygiene/dynamic_count_stamp_gate.py --fingerprint docs/rewrite-remaining-inventory-20261022.md:1028
c9626857ba1270f76376d817435205df1575990f5ac3218da6ea466cd1e8f2fa
$ python -I tools/hygiene/dynamic_count_stamp_gate.py
dynamic-count gate: OK（0 缺戳；扫描 140 个 markdown，path=docs，max-gap=24）
rc=0
```

空 allow-list 重算（与 §6.2 同口径）：**88 处命中 / 87 唯一 / 28 文件**；87 条 allow-list 全部 `used`、`rot=0`。

### 10.2 M1（Minor）「只用 git 跟踪集」此前没有牙

复核实测：把 `markdown_files` 从 `skeleton.markdown_files`（`git ls-files`）换成 `os.walk`，`--selftest`
**仍 20/20 全绿**，但运行期口径已经变了（`--path docs/zz_probe` 由 rc=0 变 rc=1；未跟踪的非法 UTF-8
由 rc=0 变 rc=2）。盲区本身在 §7 已登记，缺的是**断言**：F2 刚消除的分叉可以被一次「顺手简化」静默回退。

补的牙：selftest ⑱ 在临时目录里 `git init`、只 `git add` 一个干净文件，另一未跟踪文件写裸计数，然后
`scan(..., files=None)`，断言 `files == 1 && findings == []`。改回 `os.walk` ⇒ 会扫到未跟踪文件 ⇒ ⑱ 红
（见 §5 第 (f) 颗牙）。这是**行为级**断言，不依赖「函数对象同一性」这种实现细节。

## 11. 返工记录 ③（竞态复现：交付后 master 前进两次，合并树再红）

本卡第②轮交付 head `7e27de47a`（base `e9d00c6b3`）之后、复核跑之前，master 又前进两次：
`af1f2708b`（board merged 列合入，09:58:49）与 `eb9137615`（lessons 合入，10:00:00）。在
`walgit/master = eb9137615` 上造合并树，本门报 `FAIL (5 缺戳 / 0 rot; 扫描 142 个 markdown)`，
5 处全在 `docs/board-merged-column-20261024.md`（`:40,41,42,45,49`）—— 该文件是**另一张卡的写集**，
本卡不改它，按下表登记 allow-list；`stale_pointer_gate` 仍 rc=0。

| 位置 | 新增条目指纹 | 处置 |
|---|---|---|
| `docs/board-merged-column-20261024.md:40` | `f26ad041…` | 新增，实时看板对比表（待认领） |
| `docs/board-merged-column-20261024.md:41` | `dd44c6fb…` | 新增，实时看板对比表（进行中） |
| `docs/board-merged-column-20261024.md:42` | `37127778…` | 新增，实时看板对比表（待审） |
| `docs/board-merged-column-20261024.md:45` | `be1da8a1…` | 新增，实时看板对比表（已完成） |
| `docs/board-merged-column-20261024.md:49` | `9c1889ed…` | 新增，对比表说明句（reviewer 复核时 521→532） |

这 5 行同属该文件的「加列前/加列后」实时对比表，该段已声明两次 board 调用间隔 2.26s 内的逐字实测；
行内插戳会破坏贴出的输出与命令的逐字可核性。同时按 R3-2 订正 §3 的 allow-list 条数（此前写 86，
与 §6.2/§10 的 87 自相矛盾；本轮再 +5 ⇒ 92）。

实测（rebase 到 `walgit/master = eb9137615` 后）：

```text
$ python -I tools/hygiene/dynamic_count_stamp_gate.py
dynamic-count gate: OK（0 缺戳；扫描 142 个 markdown，path=docs，max-gap=24）
rc=0
```

> **结构性观察（供 Leader 判断，不在本卡实现）**：这是本卡第二次因「master 合入新文档 ⇒ 新文档带实时
> 看板计数 ⇒ 合并树变红 ⇒ 逐条加白名单」返工。可考虑的方向见交付 status body（围栏代码块豁免 / 段级戳自动
> 匹配 / 维持现状），属另一张卡。

python -I tools/hygiene/dynamic_count_stamp_gate.py --path docs/goal-gate-full-20261024.md \n  --ref refs/remotes/walgit/master --paragraph-tol 3600 --decl-tol 86400   # 段落声明三参数

## 9. 段落级快照声明（2026-10-24，卡 `openmir2-dynamic-count-paragraph-snapshot-exemption`）

动机：allow-list 里大量条目属「整篇已声明快照、段内计数随之冻结」的文档；每次文档作者改一行，
`ROT` 就把别人写集的改动变成对本门的阻塞（已致两轮返工）。本轮加一个**受检**的段落级豁免，
让这类文档不再靠白名单兜。

### 9.1 规则（精确）

- **「段」的定义（机械可判）**：命中行向上找**最近的标题行**（`#`..`######`）作为段首；
  段尾 = 其后第一个**同级或更高级**标题的前一行；没有上级标题 ⇒ 到文件末尾；全文无标题 ⇒ 整篇为一段。
  **不用**「同一行 ± N 字符」这种会随排版漂移的口径。
- 段内出现**快照声明**（锚词 `截至 / 快照 / 取数时点 / CUTOFF / SNAPSHOT`，与骨架
  `stale_pointer_gate.SNAPSHOT_ANCHOR_RE` 同一套），且 sha 与时间在锚词 **±2 行**内（同一段）。
- 只有**四道检查全过**才把命中降级为 `INFO`（并打印声明原文）；任一不过 ⇒ 仍红，并打印 `NOTE` 原因：
  (a) sha 是 `--ref`（默认 `refs/remotes/walgit/master`）的**祖先**；
  (b) `|声明时间 − 该 commit 时间| ≤ --paragraph-tol`（默认 **3600s**；理由：同一会话里采集计数常在
       commit 后数十分钟内完成，1h 覆盖正常编辑窗口，同时拒绝「隔天补戳」）；
  (c) 声明必须出现在**命中行之前**（`position < hit_line`）；
  (d) **blame 新近性**：声明行所在 commit 的时间与声称时间差 `≤ --decl-tol`（默认 **86400s**，
      与 `stale_pointer_gate` 的 `--decl-tol` 同值）——防「事后补声明」。
- 段外的同类计数**仍红**（防「声明一次赦免全文」），由 selftest ㉔ 钉住。

### 9.2 白名单变化（92 → 89）

改造后新豁免覆盖 3 条，已从 allow-list 撤出（各条目 path 见下；`git diff` 可见）：

- `docs/goal-gate-full-20261024.md`（段头 `截至 55a8e7fca @ 2026-10-24 05:46:17`）
- `docs/rewrite-remaining-inventory-20261022.md`（段头 `6d79be4da @ 2026-10-23T07:31+08:00`）
- `docs/window-criteria-quality-audit-20261024.md`（段头 `2a5101606… @ 2026-10-24 01:06`）

其余同类条目**不能撤**，原因实测为两类：① 声明在**文首 preamble**（骨架的文档级声明通道），
不在计数所在段内——把它算作「段内声明」等于文档级赦免，违反 ㉔ 的防伪目标；
② 计数与声明之间隔着标题（属另一段）。两者都按「段外 still red」处理，保留白名单并写理由。

### 9.3 自检与牙

`--selftest` 由 21 条增至 **27** 条，新增 ⑲–㉔：段内合法声明 ⇒ INFO；sha 非祖先 ⇒ 红；
时间差超容差 ⇒ 红；声明在命中之后 ⇒ 红；blame 新近性不符 ⇒ 红；段外裸计数 ⇒ 红。

三颗牙（串行，`cp` 备份还原，sha256 与备份一致）：

```text
# (a) 删「声明须在命中之前」⇒ ㉒ 红（26/27）
FAIL ㉒ 声明出现在命中之后 => 仍红
# (b) 删 blame 新近性 ⇒ ㉓ 红（26/27）
FAIL ㉓ 声明行 3 天后才提交（blame 新近性不符）=> 仍红
# (c) 把祖先判定改成恒真 ⇒ ㉑ 红（26/27）；⑳ 仍红因为 commit 时间读不到（另一道检查兜底）
FAIL ㉑ 声明时间与 commit 时间差 > --paragraph-tol => 仍红
```

还原后 `--selftest` 27/27；全量扫 docs：`OK（0 缺戳；3 条段落声明降级 INFO；扫描 143 个 markdown）`，rc=0。

### 9.4 仍抓不到什么（残余风险）

- **伪造一个「与 commit 时间吻合」的声明**：有提交权限的人可以 `GIT_AUTHOR_DATE/GIT_COMMITTER_DATE`
  回填到与声称时间一致，则检查 (b)(d) 同时通过；sha 仍须是祖先（不能凭空捏 commit 引用），
  但这只能提高成本、不能证明「计数确实取于该时点」。⇒ 声明行仍建议**人工复核**：
  看声明的时间与同段数字是否与当时报告/命令输出对得上。
- **声明与计数不在同一段**（preamble、跨标题）不豁免——那是设计选择，不是漏洞；这类条目留在白名单。
- 段内多处声明时，当前取**最近一个通过检查的声明**；多条互相矛盾的声明不会各自生效。
