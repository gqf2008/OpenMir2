# 过期指针门（`stale_pointer_gate`）—— 首次接入报告

- 卡：`openmir2-docs-stale-pointer-gate`（tier=L，write_set = `tools/hygiene/` + `docs/`）；
  第二类判据由 `openmir2-stale-pointer-card-status-check`（tier=L）追加；
  **快照声明受检降级 + 卡号存在性**由 `openmir2-stale-pointer-snapshot-exemption`（tier=L）追加
- 交付：`tools/hygiene/stale_pointer_gate.py` + `tools/hygiene/stale_pointer_allowlist.toml` + 本报告
- 基线：`refs/remotes/walgit/master = 55a8e7fca`（第一类）→ `3738ad234`（第二类接入时点）
- 只读仓库与账本：不改 `src/**`、`rules.json`、场景 JSON、`mir2-rs/tests/parity/whitelist.md`、
  他人卡片状态；不起栈、不连库、不占端口。

本卡起因是今天四处「文档说未完成、commit 早已合入」全靠人肉撞见。这个门把文档里的**两类**漂移变成机判：

1. **hash 类**：带 commit hash 的未完成断言 ⇒ `hash` 已是目标分支祖先就报红；
2. **卡号类**：`openmir2-*` 卡号 + 未完成/未开始状态词 ⇒ 去**协作账本**核该卡，`closed` 或已有
   `merge_result{merged:true}` 就报红。第二类专治第一类的最大假绿：**rebase/force-push 后旧 head
   不是祖先，内容却已合入**（见 §3.2、§5-3）。

## 1. 它抓什么

`tools/hygiene/stale_pointer_gate.py` 扫描 `docs/**/*.md`（`--path` 可限定目录；只扫 `git ls-files` 的
已跟踪文件），逐行找「指针 + 未完成词」的邻近对：

- **指针**：7–40 位十六进制，且**至少含一个 `a`–`f` 字母**（避免把 `20261023` 这类日期/十进制 ID 当 hash）。
- **未完成词**（默认 14 个）：尚未合入 / 尚未合并 / 未合并 / 未合入 / 未上master / 不在master / 未进master /
  未 merge / 未merge / 未落地 / 待合 / 在办 / 未合 / 未接。
- **邻近**：指针与未完成词的字符距离 ≤ `--max-gap`（默认 24）。太远的「hash 在某处、未完成词在另一处」
  不判（大多是两件事，例如 `已落地（<hash>）…余量：未接的调用点`）。

对每个指针跑：

```sh
git merge-base --is-ancestor <hash> <ref>     # 默认 --ref refs/remotes/walgit/master
```

- rc=0（祖先）⇒ **红**，打印 `file:line  hash  片段`；
- rc=1（非祖先）⇒ 绿（该 commit 确实还没进目标分支）；
- 对象不在本仓（线程 entry oid、别仓 hash、已 GC）⇒ `NOTE unresolved`，**不判红**（门无法判断，不能瞎判）。

退出码：`0` 干净；`1` 有过期指针 / 过期卡状态 / 白名单腐烂 / 扫不了的文件（fail-closed）；`2` 不在 git 仓库 /
`--ref` 解析不到 / 白名单缺失或格式错 / **账本不可读**（fail-closed）。

## 1b. 第二类：卡号 + 未完成词 → 账本状态

**为什么加**：第一类按 commit identity 判，分支大量 rebase/force-push ⇒ 旧 head 永远不是祖先，内容却已合入
（`docs/attack-weapon-wear-642-20261024.md` 的 `f5a2e485b` 是典型，见 §3.2）。第二类按 **card identity** 判：
卡在账本里的状态与文档说法是否一致，与分支历史无关。

- **卡号**：`openmir2-[a-z0-9](…)[a-z0-9]`（行内任意位置；`openmir2-worker-67` 这类 principal 名在账本里
  查不到，天然绿）。
- **状态未完成词**：尚未合入 / 尚未合并 / 未合并 / 未合入 / 未上master / 不在master / 未进master / 未 merge /
  未merge / 未落地 / 待合并 / 待合 / 在办 / 未合 / 未接 / 未开始 / 未开工 / 待开工 / 未启动 / 未开 / 未认领 /
  待认领 / 未完成 / 未关闭。这是**卡的谓词**，与第一类的 14 词表相互独立；两表都只增不减。
- **邻近与同一行**：卡号与词必须在**同一行**、距离 ≤ `--max-gap`（默认 24）。表格行也是**一行**，
  所以「`| `openmir2-x` | 未合入 |`」这种「卡号一格、词另一格」**算**配对——判据是「同一行」，不是「同一格」
  （返工订正：`e5c9f169c` 曾额外要求同格，静默吞掉两处真漂移，见 §3.4 第 6/16 行）。
- **非断言复合词**（字面表，不缩词表）：`未接线`/`未接入`（子系统没接线，是方向词，不是卡的流程态）、
  `含未合并`/`其中未合并`（统计行的分类标签）。命中若落在这些字面串内即跳过；词表本身一个字都没减。
- **判据**：卡在账本里 `status == closed`，或**存在任一** `merge_result{merged:true}` ⇒ 红，
  打印 `file:line  卡号  账本状态`（`closed` / `merged (status=…)`）。「存在」是验收口径：
  之后的 `merged:false` 不清除先前的合并事实。
- **账本读取（一次聚合，不按卡起进程）**：
  1. `walgit collab board --format json` **一次**：卡片状态（board 投影）；
  2. `git cat-file blob refs/collab/meta/snapshot` **一次**：`gc` 折叠区；
  3. 一次 `git cat-file --batch` 读 `refs/collab/inbox/*` 尾部：条目正文里的 `merge_result` 旗标
     （board 投影**不暴露** `merged` 字段，所以必须读条目，否则 R 卡「已合入但保持 in-progress」判不出来）。
  实测 `docs` 全量 + 账本 **约 2 秒**（8139 条 inbox + 13 MB snapshot）。读取顺序与
  `docs/leader-goal-20261023.md` §5.3 的既有口径一致（snapshot 折叠区 + inbox tail）。
- **离线 / fail-closed**：`--no-ledger` ⇒ 跳过本类并**明确打印「已跳过」**；账本读不到
  （`walgit` 不在 PATH / board 非零 / snapshot 或 inbox 读失败 / board 有卡而账本按条为空）⇒ **rc=2**，
  与第一类的 rc=1 区分。**账本读成 0 线程也是 rc=2**：全新检出没 fetch `refs/collab/*` 时 board 0 卡、
  snapshot/inbox 空，看起来和「本项目没有卡」一模一样，而旧实现会静默全绿、还打印「N card id(s) judged」。
  「查不到 ≠ 无问题」是本项目三次返工的共同根因，所以默认 fail-closed；确需空账本跑，用
  `--allow-empty-ledger`（降级为醒目 NOTE，rc 照常判）。`--ledger-json PATH` 用 board 形状的 JSON 替代
  `walgit collab board`（不再起 `walgit`），并继续合并本地 snapshot/inbox 里的 merge 旗标；本地没有
  `refs/collab/*` 时只能判 `closed`，判不了 `merge_result`（这是离线快照的已知局限）。
- **白名单腐烂**：白名单条目对两类命中都生效；`--no-ledger` 时，带卡号的候选行仍被视为「可能命中」，
  以免把只压卡号命中的历史条目误报为腐烂。

## 1c. 快照声明：受检才降级（`openmir2-stale-pointer-snapshot-exemption`）

**为什么**：会话摘要/审计报告是**某时点的快照**。它引用「当时还没进 master 的卡或 commit」
在快照语义下是正确的；门按现况判就是假红。但「文件头加一行 CUTOFF 就赦免全文」会把门变成摆设，
所以豁免必须**受检**，而且只降级、不静默丢弃。

**识别规则（正则写在 `stale_pointer_gate.py` 顶部，以下为契约）**：

- **anchor**：`CUTOFF|截至|快照|取数时点|SNAPSHOT`（不区分大小写）。
- **sha**：anchor 行本身或 **±2 行**（`SNAPSHOT_NEAR`）内的 7–40 位十六进制，且至少含一个 `a`–`f`
  （与第一类同口径，避免把日期当 hash）。窗口里可能有多条候选（头里常见的「`基线 <sha>`」+
  「`head <sha>`」），按**离 anchor 由近到远**逐个尝试，第一个通过全部检查的胜出；
  **被否的候选逐条打印 NOTE**（返工前是静默丢弃，相邻 `基线 <sha>` 会把合法声明变成假红——reviewer-3 M-1）。
- **time**：同一窗口内的 `YYYY-MM-DD` + 最多 6 个非数字字符 + `HH:MM[:SS]`
  （可带 `+HH:MM` / `+HHMM` / `Z` / `UTC`），或 `CUTOFF`/`EPOCH` 后跟 9–11 位 epoch。
- **时区**：显式 offset / `Z` / `UTC` 优先；**不写时按本项目约定解释为 +08:00**。
- **只看文档头**：第一个 `## ` 之前，**且**在前 60 行（`SNAPSHOT_PREAMBLE_MAX`）内。
  **两条边界各由 selftest ㉗ 钉住**（`##` 之后 / 第 61 行之后的声明必须**不降级**）：
  正文中间引用的「看板快照 `<sha>` @ `<ts>`」是**数据点**，不是「全文冻结于此」的声明。
  `docs/rewrite-remaining-inventory-20261022.md` 的正文里就有一条带真实时间的 `board 快照` 数据点。
  在 `e643b43fb` 上删掉任一边界，它**曾**把该文件的真滞后洗成 INFO、rc 1→0（reviewer-3 I-1 的变异实测）；
  现在 `:725` 已修复，这类回归就只剩夹具 ㉗ 能抓——所以 ㉗ 必须钉住两条边界。
- **歧义即放弃**：**时间**在窗口里出现多个不同值 ⇒ 不生成声明；**sha** 若最近的一条行内有多个不同值
  ⇒ 整条放弃（更远的行有多个 sha 时只跳过该行）。sha 的多候选改为逐个受检，**不是**「有歧义就赦免」。

**四项必查（任一不满足 ⇒ 仍判红 rc=1；被否的声明打印 `NOTE snapshot-declaration rejected`）**：

- (a) 声明 sha 存在，且是 `--ref`（默认 `refs/remotes/walgit/master`）的**祖先**；
- (b) 声明时间与该 commit 的 **author 或 committer** 时间相差 ≤ `--snapshot-tol`（默认 **900 秒**）。
  注意：这只约束「声明声称的时点 vs 它所指 commit 自己的提交时刻」，**不是「相对现在的新近性」**；
- (c) 整个声明（anchor、time 与**被采纳的** sha 行的**最后一行**）出现在命中行**之前**
  ——不许文件末尾补一行赦免全文；
- (d) **声明本身的可信性**：对 anchor / time / sha 三行跑
  `git blame -L <line>,<line> --porcelain -- <path>`，取该行的 **author 或 committer 时间**，
  与声明声称的时间相差 > `--decl-tol`（默认 **86400 秒 = 1 天**）⇒ 判红
  （`declaration written Δ… > --decl-tol`）。语义：声明只在该行**确实写于它所声称的时点附近**时才可信；
  事后往文件头补一行、或拿 2026-01-01 的旧 sha 用它自己的真实时间回填，都会被这条挡住。
  - **1 天的取值理由**：本仓现存 5 条真声明的 blame Δ 最大 **7519 秒（≈2.1 小时）**（见下），
    1 天留足「当天写、隔天提交」与会话跨零点的余量，同时让「几个月后才补的声明」必然判红；
  - 每行 author / committer 两个时间都接受（rebase 只改后者）；**整条声明以其最新一行为准**；
  - blame 取不到（行未提交 / 文件不在历史里）⇒ **不豁免**（「无法验证」不等于「可信」），
    所以合并前的门要在**干净工作树**上跑。

通过后，该行命中降级为 `INFO snapshot-exempt`，**打印声明原文**（含行号）、sha、声明时间、commit 时间、
Δ 秒、**声明行的 blame Δ**，以及被否的更近候选。白名单优先于快照降级
（同行两者都命中时按白名单记 used，避免腐烂误报）。

**实测（返工后，master `3fa6899a9`，138 个 md）**：全库 **5 条**受检声明（同一 anchor 窗口只算一条）——
`goal-gate-full-20261024.md`（blame Δ56s）、`goal-snapshot-late-20261023.md`（Δ130s）、
`r-card-evidence-sweep-20261023.md`（Δ7519s）、`runbook-tool-consistency-audit-20261023.md`（Δ943s）、
`session-digest-20261024.md`（Δ712s），**全部通过 (d)**，无 `NOTE snapshot-declaration rejected`。
本轮只有 `session-digest-20261024.md` 的命中落在其声明之后，降级为 INFO。

**卡号存在性（同一张卡的第二个修正）**：`openmir2-*` 命中后先查**账本线程集合**；
id 不在集合里 ⇒ 打印 `INFO 非账本卡号（按特性名/标签放行）`，**不红**。
实测被这样放行的有 `openmir2-spell-second-observer`、`openmir2-combat-hit-monster-fixture`、
`openmir2-adjust-bonus-wire` 等「计划中的卡 / 特性名」。
**账本不可读 / 0 线程 ⇒ 仍是 rc=2**：「读不到账本」与「id 不在可读账本里」是两件事，
只有后者才能判「不是卡号」；`--ledger-json` 指向不存在的文件也有 selftest ㉖ 钉住 rc=2。

## 2. 三条豁免（都要求行内可见，两类判据共用）

1. **引号内不计**：未完成词位于 `「…」`/`『…』` 内 ⇒ 是在**引用别人的旧说法**，不是本行断言。
   漂移清扫报告就是逐行「原说法 → 订正」，整表引用旧词，逐条加白名单会把白名单变成垃圾桶。
2. **订正标记**：行内含 `〔…订正…〕` 或 `YYYY-MM-DD 订正`（本仓既有两种写法）⇒ 该行自带订正，
   视为历史叙述，不再按旧说法判。
3. **白名单**：`tools/hygiene/stale_pointer_allowlist.toml`，条目 = `(path, line_sha256)` + **必填 reason**。
   指纹 = 行尾空白剥离后的 `sha256`。生成一行的 stanza：

   ```sh
   python -I tools/hygiene/stale_pointer_gate.py --fingerprint docs/foo.md:123
   ```

   **防腐烂**：条目必须真的压掉一处命中；匹配不到（行被删/改写/本来就不命中）⇒ 门自己报
   `allow-list rot` 并 rc=1。白名单不能悄悄积累死条目。用 `--path` 缩窄扫描时，只对该子树内的条目
   做腐烂检查（子树外条目只是没被看到，不误报）。

## 3. 首次接入：master `55a8e7fca` 的真实命中

接入前全量实跑：**127 个 md / 30713 行，4 处过期指针 + 6 处 unresolved**。逐条：

| # | 位置 | hash | 事实 | 处置 |
|---:|---|---|---|---|
| 1 | `docs/M2顺序判据②-对账台账.md:200` | `090add78` | 祖先（`merge-base --is-ancestor` rc=0） | **就地订正**：加〔2026-10-24 订正：已合入 master；本句为 2026-10-11 时点记录〕 |
| 2 | `docs/M2顺序判据②-对账台账.md:202` | `090add78` | 同上 | **就地订正**：注明收敛已完成（`items.rs:65` 用 `StdItemCatalog::from_items`，`equipped_feature` 见 `bag.rs:763`/`lib.rs:4003`） |
| 3 | `docs/docs-state-drift-sweep-20261024.md:43` | `7bb48239a` | 祖先 | **白名单**：§2 表第 10 行「原说法」列逐字引用旧表述，行内 hash 是订正证据 commit |
| 4 | `docs/rewrite-remaining-inventory-20261022.md:470` | `bdf4e0ab0` | 祖先 | **白名单**：2026-10-22 时点的「在飞/待审」快照；「未接项」指 RNG 项，不是对该 commit 的合入断言 |

就地订正的两行都保留了原文（只追加带日期订正标记），不重写历史；白名单两行都写清「为什么必须保留」。
**没有为了让门变绿而放宽规则**：规则阈值、词表、邻近窗口在首次接入前后完全一致。

### 3.1 unresolved（门不判，仅提示）

```
docs/rules-pending-approval-pack-20261023.md:241  c3f5e854
docs/tmp-cleanup-proposal-20261017.md:201         962f7c80
docs/window-batch-rehearsal-20261024.md:256       8b810c894 / cfc96f1b
docs/window-criteria-quality-audit-20261024.md:121 7565076f…（线程 entry oid）
docs/window-criteria-quality-audit-20261024.md:123 e13059c5…（线程 entry oid）
```

这些不是本仓 commit（卡片 `status` 的 entry oid、别仓/已 GC 的 head），门无法判断合入状态，**不判红**。
代价见 §5：拼错一个 hash 也会静默过关。

### 3.2 门判不了、但本次顺手订正的一处（重要样本）

`docs/attack-weapon-wear-642-20261024.md:173` 写后继卡「head `f5a2e485b`，**未合入**」「待该卡合入后接线」。
事实：该卡已合入 master（merge `d38acf25b`，返工后 head `4c301e877`），接线也已落地
（`lib.rs:12965` 灌 `set_attack_weapon_strong`，`combat.rs:1266` 消费）。
**但 `f5a2e485b` 不是祖先**——分支在合并前被 rebase，旧 head 从未进主干。所以门对这一行是**绿**的。
本次按「能就地订正的订正」补了〔2026-10-24 订正〕，并把它作为 §5 反例登记：

> **hash 不是祖先 ≠ 内容没合入**（rebase/squash/force-push 会让旧 head 永远不是祖先）。

同类样本还有 `docs/window-batch-rehearsal-20261024.md:17`（drift-sweep v5 head `6ceb0689d` 标「尚未合入」，
v6 已由 `55a8e7fca` 合入，但 v5 head 同样因返工不是祖先）——它保持原文（当时时点的交付说明），
只在本报告登记，作为门的天花板证据。

### 3.3 对今天四个实例的回放（门能抓几个）

| 今天的实例 | hash 状态 | 门会不会抓 |
|---|---|---|
| 「`b5a37d3d5` 仍未合」 | 祖先（随 `bb4317a01` 合入） | **会**：指针 + 未完成词邻近 + 已合入 ⇒ 红 |
| 「Rust 642 未接」（`8dcb8c8ca` 已接） | 祖先 | **不会**：那句话没写 hash；若同一行带 `8dcb8c8ca` 就会红 |
| 「T-247 仍写 642 未接」（`05ca859bb` 已订正） | 祖先 | **不会**：落在 `mir2-rs/tests/parity/whitelist.md`，不在 `docs/` 扫描范围 |
| 「`spell_cast` 默认 alt rc=0 是假绿」（`9083f4863` 后 rc=2） | 祖先 | **不会**：「假绿」不在未完成词表，且原句无 hash |

结论：这类机判门只能覆盖「**写了 hash 且措辞落在词表内**」的漂移；今天四个实例里只有 1 个满足。
剩余三类要靠词表扩充/扩大扫描范围（另开卡），或继续人肉。这不是给门开脱，是它的明确边界。

### 3.4 第二类首次接入：master `3738ad234` 的真实命中

**口径（返工后）**：卡号与未完成词**同一行**、距离 ≤ `--max-gap`，再叠加 §1b 的两条字面复合词排除。
在 base `3738ad234` 的 docs 上实测（空白名单 + 真实账本）：

| 定位规则 | 候选对 / 行 | 判红命中 |
|---|---:|---:|
| 同格（`e5c9f169c` 的旧规则） | 29 / 27 | 17 处 / 16 行 |
| **同一行（本版，含字面排除）** | 36 / 34 | **19 处 / 18 行** |
| 同一行、去掉字面复合词排除 | 38 / 36 | 21 处 / 20 行（多出的 2 条是假红：`goal-gate-full-20261024.md:47` 的统计标签、`window-criteria-backfill-20261024.md:78` 的方向词） |
| 同一行、并把 `--max-gap` 也放开（`100000`） | 120 / 80 | 84 处 |

即**同格规则只少 2 条真命中、却多出 2 条假红**，收益为负，故回到卡面原文的「同一行」。
旧报告写的「不采用时 60+ 条」是「连 `--max-gap` 一起去掉」的口径；放开跨格的真实数字是上表第三行
（多 2 处真命中 + 2 条假红），已按实测口径改写。**假红用字面复合词排除处理，不靠收窄规则降噪。**

base 上 **19 处 / 18 行**逐条处置：

| # | 位置 | 卡号 | 账本状态 | 事实 / 处置 |
|---:|---|---|---|---|
| 1 | `docs/Rust重写设计文档.md:695` | `openmir2-m5-convergence-drills` | merged（in-progress） | 行内「未开始/待认领」指 §17.1 其余四项；阶段0 已合入 `2f0b102c5` ⇒ 前一轮**就地订正** |
| 2 | `docs/Rust重写设计文档.md:890` | `openmir2-rng-consumer-structure-parity` | merged（in-progress） | 方案 C 已合入 `e7d432189`，卡已认领 ⇒ 前一轮**就地订正** |
| 3 | `docs/Rust重写设计文档.md:891` | `openmir2-m5-convergence-drills` | merged（in-progress） | 同上（Claim B/M5 两行） ⇒ 前一轮**就地订正** |
| 4 | `docs/docs-state-drift-sweep-20261024.md:48` | `openmir2-sm-skeleton-wiring` | merged（in-progress） | 漂移表「订正」列的时点记录，行文写的是已合/在办 ⇒ **白名单**（理由已按事实重写） |
| 5 | `docs/docs-state-drift-sweep-20261024.md:48` | `openmir2-dualrun-window-burndown` | closed | 同上（同行第二张卡） ⇒ 同一条**白名单** |
| 6 | `docs/leader-quota-block-20261023.md:22` | `openmir2-monster-showhp-53-broadcast` | merged（in-progress） | **本轮新增（跨格）**：blame 2026-10-23T03:49Z（11:49+08），账本 `merge_result` 条目 12:05(+08)=04:05Z，merge commit **`65a8af8d6`**（12:02+08）⇒ 早 16 分钟（对条目）/ 13 分钟（对 merge commit）的时点快照，**就地订正** |
| 7 | `docs/m3-remaining-inventory-20261022.md:46` | `openmir2-monster-run-overrides` | merged（in-progress） | 2026-10-22 盘点快照；「实现未开」与卡最新 status 一致，只因设计件带来 merge_result 判红 ⇒ **白名单**（理由时间线已订正） |
| 8 | `docs/r-window-registry-batch-1-20261023.md:66` | `openmir2-scenario-spell-cast` | closed | 2026-10-23 批次登记表：blame 14:49Z 早于首次 merge 14:58Z ⇒ **白名单** |
| 9 | `docs/r-window-registry-batch-1-20261023.md:69` | `openmir2-scenario-spell-cast` | closed | 同上 ⇒ **白名单** |
| 10 | `docs/r-window-registry-batch-1-20261023.md:70` | `openmir2-scenario-spell-cast` | closed | 同上 ⇒ **白名单** |
| 11 | `docs/rewrite-remaining-inventory-20261022.md:376` | `openmir2-m3-fixture-marker-prefix-bug` | merged（in-progress） | **判据纠正**：该卡 2026-10-13T07:21Z 已认领、当日 4 次 merge，行写于 10-22T15:02Z ⇒ 写时即错，不是时点快照；**就地订正并删除原白名单条目** |
| 12 | `docs/session-summary-20261023.md:39` | `openmir2-script-sendmsg-host` | merged（in-progress） | blame 11:27Z 早于 merge 19:23Z ⇒ 时点快照 ⇒ **白名单** |
| 13 | `docs/spell-cast-window-blockers-20261024.md:213` | `openmir2-spell-frame-compare-rules` | closed | 「建议 id（未开）」已过时：该 id 已立卡并在 `89a3d02fd` 合入 ⇒ 前一轮**就地订正** |
| 14 | `docs/spell-cast-window-blockers-20261024.md:237` | `openmir2-spell-frame-compare-rules` | closed | 同上 ⇒ 前一轮**就地订正** |
| 15 | `docs/tmp-cleanup-proposal-20261017.md:3` | `openmir2-tmp-cleanup-audit-redo` | merged（in-progress） | blame 05:21:01Z，merge 05:21:40Z（晚 39 秒）⇒ 时点快照 ⇒ **白名单** |
| 16 | `docs/window-round3-runbook-20261021.md:30` | `openmir2-merchant-race-bias` | merged（in-progress） | **本轮新增（跨格）**：blame 2026-10-21T10:35Z，卡 11:15Z 由 `a8ff9fdf9` 合入 ⇒ 早 40 分钟的时点快照，**就地订正** |
| 17 | `docs/window-round3-runbook-20261021.md:1016` | `openmir2-merchant-race-bias` | merged（in-progress） | 2026-10-21 当日 BLOCKED 记录 ⇒ **白名单** |
| 18 | `docs/window-round3-runbook-20261021.md:1034` | `openmir2-merchant-race-bias` | merged（in-progress） | 同上 ⇒ **白名单** |
| 19 | `docs/交接-OpenMir2编排与联调-20261015.md:16` | `openmir2-m5-convergence-drills` | merged（in-progress） | blame 10-15T05:14Z 早于 merge 12:27Z ⇒ 时点快照 ⇒ **白名单** |

**第 6、16 行是本轮新增的两处真漂移**（同格规则看不到）。两行分别写于对应卡合入前 16 分钟 / 40 分钟，
属当日时点记录：保留原文、就地追加〔2026-10-24 订正〕并写明合入 commit，不重写历史。

**2026-10-24 订正（M-4，合并提交写错）**：第 6 行原写「卡 12:05(+08)=04:05Z 由 `ca7cb5acc` 合入」。
`65a8af8d6` 才是**合并提交**（`git rev-list --parents -n1 65a8af8d6` 给出两个父：`bf26c39eb` + `ca7cb5acc`，
提交时间 2026-10-23 12:02:02 +0800）；`ca7cb5acc` 是分支内提交（11:56:53 +0800，正是 `merge_result` 的
`merged_head`）；12:05:25 +0800 是 `merge_result` **条目自己的 `ts`**，不是任何 commit 的时刻。复现：

```sh
git log -1 --format='%H %ai %ci %s' 65a8af8d6 ca7cb5acc
git rev-list --parents -n1 65a8af8d6
walgit collab thread --repo . openmir2-monster-showhp-53-broadcast   # merge_result: oid / merged_head / ts
```

**同错残留（不在本卡写集）**：`docs/leader-quota-block-20261023.md:22` 的就地〔订正〕标记也写了
`ca7cb5acc`。该文件属另一张卡的写集，本卡不改，已作为 comment 留在本卡线程。

**第 11 行改判**：原白名单理由称「2026-10-22 时点未认领，其后多次合入」，但账本是该卡 **2026-10-13T07:21Z 已认领**、
当日 4 次 `merge_result{merged:true}`，markers 前缀子项也已随 `9ff7ff064`（2026-10-13 15:38+08）合入——
写作时间 2026-10-22T15:02Z，**写时即错**，「其后」更是反的（`9ff7ff064` / `350cb923b` 都早于该行）。
按「能就地订正的订正」改为就地加〔订正〕，并从白名单删除该条（12 条 = 第一类 2 + 第二类 10）。

**其余 10 条卡类白名单逐条重核**（自算 blame 时间 vs 卡首次 `merge_result` 时间，未采信他人结论）：

| 条目 | 行 blame（UTC） | 卡首次 merge（UTC） | 重核结论 |
|---|---|---|---|
| `docs-state-drift-sweep` :48 | 2026-10-23T18:28Z | sm-skeleton 10-23T05:22Z / dualrun 10-23T04:29Z | 行是「订正」列，写的是已合/在办（措辞准确），只因「存在 merge ⇒ 红」被豁免 ⇒ 成立，reason 已按行文重写 |
| `m3-remaining-inventory` :46 | 2026-10-20T13:23Z | monster-run-overrides 10-17T13:30Z（设计件） | 行文与卡最新 status 一致 ⇒ 成立（原 reason 的「其后才合入」已订正） |
| `r-window-registry-batch-1` :66/:69/:70 | 2026-10-23T14:49Z | scenario-spell-cast 10-23T14:58Z | 早 9 分钟 ⇒ 成立（真时点快照） |
| `session-summary-20261023` :39 | 2026-10-23T11:27Z | script-sendmsg-host 10-23T19:23Z | 早 8 小时 ⇒ 成立 |
| `tmp-cleanup-proposal-20261017` :3 | 2026-10-17T05:21:01Z | tmp-cleanup-audit-redo 10-17T05:21:40Z | 早 39 秒 ⇒ 成立 |
| `window-round3-runbook` :1016/:1034 | 2026-10-21T10:35Z | merchant-race-bias 10-21T11:15Z | 早 40 分钟 ⇒ 成立 |
| `交接-OpenMir2编排与联调-20261015` :16 | 2026-10-15T05:14Z | m5-convergence-drills 10-15T12:27Z | 早 7 小时 ⇒ 成立 |

**行 blame 口径（M-2 订正，2026-10-24）**：上表取 **author 时间**（写作时刻）。原版前两行误取了
**committer 时间**——`docs/docs-state-drift-sweep-20261024.md:48` 的 committer 是 2026-10-23T21:22Z，
`docs/m3-remaining-inventory-20261022.md:46` 的 committer 是 2026-10-20T14:22Z；两行都被 rebase 改写过
committer 列，用它会把写作时刻推晚约 3 小时 / 1 小时。复现：

```sh
git blame -L 48,48 --porcelain -- docs/docs-state-drift-sweep-20261024.md | grep -E '^(author-time|committer-time)'
# author-time 1792780112 (2026-10-23T18:28:32Z) / committer-time 1792790577 (2026-10-23T21:22:57Z)
git blame -L 46,46 --porcelain -- docs/m3-remaining-inventory-20261022.md | grep -E '^(author-time|committer-time)'
# author-time 1792502625 (2026-10-20T13:23:45Z) / committer-time 1792506159 (2026-10-20T14:22:39Z)
```

对应提交：`9bebfa16f`（author 02:28:32+08 / committer 05:22:57+08）、
`923b0c17d`（author 21:23:45+08 / committer 22:22:39+08）。

base 19 处命中里 `closed` 6 处（3 张卡：`openmir2-dualrun-window-burndown`、`openmir2-scenario-spell-cast`、
`openmir2-spell-frame-compare-rules`），`merged:true` 13 处；第一类对这些行全是绿。
**第二类抓不到本次的 `f5a2e485b` 实例**：`attack-weapon-wear-642-20261024.md:173` 那一行没有卡号，只有 hash。
所以第二类不是第一类的替代，而是补上「rebase 后 hash 非祖先、但同一行写了卡号」那类。

## 3.5 快照/卡号修正后：全量重扫（master `3fa6899a9`）

返工前（base `e643b43fb`）：`FAIL (2 stale pointer(s), 3 card-status hit(s))`。本卡修正后：

| 位置 | 类别 | 处置 |
|---|---|---|
| `docs/session-digest-20261024.md:57` | 卡号 `openmir2-goal-gate-unmerged-selftest` | **INFO snapshot-exempt**：声明 `L4 取数时点 2026-10-24 07:18:30 +08`，sha `c4b66ddb1` 是祖先，Δ106s，声明行 blame Δ712s。该格里的「未合并窗口桶」是**特性名**；整张表是 07:18 的合并记录，快照语义正确 |
| `docs/session-digest-20261024.md:177` | hash `478ff9a33` / `3bd8c6580` + 卡号 `openmir2-gate-baseline-snapshot` | **INFO snapshot-exempt**：同一份声明（`c4b66ddb1`，Δ106s / blame Δ712s）；该行是当日「在办/待审」快照 |
| `docs/rewrite-remaining-inventory-20261022.md:725` | 卡号 `openmir2-merchant-script-load` | **已由 `3fa6899a9`（`3570f31d4`）消解**：`openmir2-inventory-top5-audit` 返工已把该行改写为「已合入 460691b24」，合并树全量扫不再出现该红点。**本卡未改该文件**（另一张卡的写集） |

现状：在 `git merge-tree --write-tree walgit/master HEAD` 的合并树上全量扫 ⇒
`stale-pointer gate: OK … 0 card-status hit(s)`，快照豁免按**行**计（当时 2 条命中降级），**rc=0**。现行 head（ref=`5848a872b`）实测 `3 snapshot-exempt line(s)` / 5 条 INFO（见 §5-16）。
零残留、零白名单新增。本卡没有靠收窄词表、扩大引号或放宽 `--max-gap` 来变绿。

## 4. 白名单：条目数与防腐烂验证

当前 **12 条**：第一类 2 条（§3 表 #3/#4），第二类 10 条（§3.4 表 #4/#5 共用 1 条、#7-#10、#12、
#15、#17-#19；每条都是**重核过 blame 时间 vs 卡首次 merge 时间**的时点快照，理由见 §3.4 末表）。
实跑汇总（本卡 head `5848a872b`，2026-10-24）：`stale-pointer gate: OK (144 files / 35094 lines …; 0 card-status hit(s), 10 card id(s) judged, 23 card-bearing line(s))`，即 `12 allow-listed line(s)`、`3 snapshot-exempt line(s)`、0 rot。腐烂验证：

```sh
cp tools/hygiene/stale_pointer_allowlist.toml /e/tmp/rot.toml
# 追加一条指向 docs/stale-pointer-gate-20261024.md 的伪条目
python -I tools/hygiene/stale_pointer_gate.py --allowlist /e/tmp/rot.toml
# => ERROR allow-list rot: docs/stale-pointer-gate-20261024.md (entry #13) matches no line …
#    stale-pointer gate: FAIL (0 stale pointer(s), 0 card-status hit(s), 1 allow-list rot, 0 error(s)) ，rc=1
```

`--selftest` 里的 ⑦/⑭ 也是同一断言（合成仓库、不依赖真实仓库状态；⑭ 覆盖 `--path .` 根扫描）。
**`--path .` 的修复**：此前 `rel_path == "."` 时 `in_scope` 为空 ⇒ 根扫描的腐烂检查被静默关闭
（reviewer-3 在 `eeb8d064f` 记账的 Minor）。现在 `prefix in ("", ".")` 时 `in_scope = allowlist`。
`--no-ledger` 下，带卡号的候选行仍算「可能命中」，只压卡号命中的历史条目不会被误报成腐烂。

## 5. 这个门抓不到什么（别把绿当「文档全对」）

**门绿只说明：被扫的 `docs/**/*.md` 里，既没有「已知 commit hash + 邻近未完成词 + hash 是目标分支祖先」的行，
也没有「账本可判的 `openmir2-*` 卡号 + 邻近状态词 + 卡已 closed / 已 merged」的行。**
以下都抓不到：

1. **没写 hash 也没写卡号的口头断言**：如「`preflight.py` 尚未合入」——两类都看不见。
   （今天不少漂移正是这种形态。）
2. **hash 对，但语义被改写**：写「`abc1234` 已合入…但功能未接」时，门不会判——它只回答「该 hash
   是否祖先」，不回答「功能是否真接」；反之
   「`abc1234` 未合入」若 hash 拼错成另一个存在但无关的 commit（极低概率），门也按那个 hash 判。
3. **hash 不是祖先但内容已合入**：rebase / squash / force-push / 返工重提。已在 §3.2 用 `f5a2e485b`
   实测登记——第一类的**最常见假绿**；第二类只能补上「同一行也写了卡号」的那些，
   只写 hash 的行（`attack-weapon-wear-642-20261024.md:173`）仍然漏。
4. **unresolved 指针**：线程 entry oid、别仓 hash、已 GC 对象，以及**拼错的 hash**，一律不判红。
5. **全数字短 hash**：规则要求至少一个 `a`–`f` 字母；全数字的 7 位缩写 hash 会被当成数字跳过
   （换来的收益是不把日期 `20261023` 当 hash）。
6. **距离 > `--max-gap`（24）**：同一行但隔得远的「指针 … 未完成词」不判（多为两件事，也可能是真漂移）。
   卡号类只要求**同一行**（跨表格单元也算配对），代价是 `--max-gap` 成了唯一降噪闸：把它放开到 `100000`，
   base 上候选对从 36 涨到 120、命中从 19 涨到 84。所以**调宽 `--max-gap` 会爆量**，不要为了抓漏而随手放大。
   另外两点已知不精确：① 复合词排除只有两条字面（未接线/未接入、含未合并/其中未合并），其它复合词仍可能假红；
   ② 一行写多张卡时，词按**最近距离**配到某一张（实测 `docs-state-drift-sweep-20261024.md:48` 的「在办」
   同时配上同行相邻的 `openmir2-dualrun-window-burndown`）；该行本来已判红/已豁免，即当前 0 新增假红，
   但多卡行上的配对本身不是语义绑定的。
7. **引号内的断言**：真的过期表述若被作者自己写进 `「…」`，门会当引用放过。
8. **订正标记行**：行内有 `YYYY-MM-DD 订正` 就整行豁免，**不校验订正内容是否覆盖了该 hash/卡号**。
9. **词表外的状态词**：仍缺 / 待建 / 阻塞 / 未验收 / 未实现 / needs-review 等不在词表内，不触发
   （「needs-review」是流程态，不是「未完成」断言；写进词表会把正常待审卡全判红）。
10. **范围与口径**：只扫 `docs/`（`--path` 可改）；只对 `--ref`（默认 `refs/remotes/walgit/master`）判祖先；
    跨分支/跨仓「已合入某分支」的断言不在判据内；非 md 文件（thread、卡片、代码注释）不扫。
11. **`--ref` 没 fetch 是最危险的假绿（reviewer-3 Suggestion，本卡登记）**：rc=2 只在 ref **解析不到**时出现；
    若 `refs/remotes/walgit/master` 存在但**落后**于真实 master，祖先判断基于旧基线 ⇒ 明明已合入的 hash
    判成「非祖先」而绿。所以合并前必须 `git fetch walgit master`，不能拿陈旧 ref 当基线。
12. **账本口径之外的卡号**：第二类只认**账本里查得到的卡号**。
    - principal 名（`openmir2-worker-67`）与拼错的卡号在账本里查不到 ⇒ 绿（不是红）；
    - 文档写了「未来要开的卡」（账本还没有该线程）⇒ 绿，正确；
    - 账本本身没同步（`refs/collab/*` 没 fetch/没 gc）：**读成 0 线程时 rc=2**（不再静默全绿），
      但「账本存在、只是旧的」（有线程、缺最近变化）无法自动识别，仍会漏。
      `--allow-empty-ledger` 只在明知账本为空仍要跑时使用（降级为 NOTE）。
13. **`merged:true` 是存在性判据**：卡曾合并、其后又 `merged:false`（返工失败）仍按已合并判红；
    这是验收口径的保守方向，宁可多报不漏报。
14. **`--no-ledger` 的绿**：显式关闭第二类时打印「已跳过」，第一类照常判。这个绿不包含卡状态结论。
15. **表的投影与 tail 的差**：`walgit collab board` 实测漏掉 11 张线程（**2 张 `closed`**：
    `openmir2-gate-outbound-producer-dimension`、`openmir2-noclear-mon-list-gate-red`，另 9 张 `needs-review`）。
    机制：board 把线程折叠成「最新一条 status 或 merge_result」，最新一条是 `merge_result` 时投影状态为
    `merged`，不命中 `board.toml` 任何列 ⇒ 整卡消失。门因此不用 board 当唯一来源，而是「board 状态 +
    snapshot 折叠区 + inbox tail」合并判，board 缺席的线程仍可判（selftest ⑮ 钉住这条并集）。
    ⚠ board 是**在线变动**的：同一口径三次实测是 509 → 511/512 → 514 张卡（对应 525 条线程），
    引用具体数字时必须带时点。

16. **伪造快照声明（本版已加机判防线，但仍有人工边界）**：
    - **声明 sha 本身没有「新近性」要求**：任意祖先 sha 都可以被声明（包括 2026-01-01 的老 commit）。
      「900 秒」容差只约束「**声明时间 vs 该 sha 自己的提交时间**」，**与「现在」无关**——
      别把它读成「只有近期提交的 sha 才能声明」（旧版本段的措辞会误导，此处订正）。
    - **可机判的伪造 = 事后补声明**：`--decl-tol`（默认 1 天）比较「声明行的 blame commit 时间」
      与「声明声称的时间」。声明行是几个月后才补的 ⇒ `NOTE snapshot-declaration rejected … Δ… > --decl-tol`
      且命中**保持红**。用旧 sha 的真实时间回填、却在今天提交，同样判红（差值 = 今天 − 旧时点）。
    - **挡不住的**：整份文件（声明 + 正文断言）在**同一次提交**里写好——声明行与命中行 blame 到同一个
      commit，时间也自洽，机判无差异（reviewer-3 用 A/B 两个祖先的 `fakerepo` 实测过）。
      这种只能人工核：对每条 `INFO snapshot-exempt` 看声明时点与文件名/正文日期是否自洽、
      正文断言的事实是否真在该时点成立。注意「声明行早于命中行」在文件顺序上恒真（(c) 已保证），
      **不能**当作防伪证据。
    - **blame 不可用一律不豁免（fail-closed），并按具体原因打 NOTE**：行未提交（`git blame` 的 commit 全 0 /
      输出含 `Not Committed Yet`，2026-10-24 实测**已 stage 未 commit** 就走这一支——卡
      `openmir2-stale-pointer-blame-failclosed`，selftest ㉛ 钉住）、文件不在历史里、解析不出
      author/committer 时间。**「合并前在干净工作树上跑」不再是唯一防线**：即使工作树有未提交的声明行，
      现在也会由机制兜底判红，而不是靠操作纪律。
    - 盘点统计口径：`snapshot-exempt` 按**行**计数，汇总行写作 `snapshot-exempt line(s)`（与逐行 INFO 一致）。
17. **账本存在但过期**：卡号存在性依赖 `refs/collab/*` 与 board 投影。账本**读得到但落后**
    （新卡还没 fetch）时，真卡号会被当成「非卡号」⇒ INFO 放行。自动判不出来（这就是要求
    「读不到 ⇒ rc=2」的原因）；`INFO 非账本卡号` 行也要人看一眼。
18. **时间解释假设 +08**：文档时间不写 offset 时按 +08 解释。作者用 UTC 却漏写 `Z` ⇒ 差 8 小时、
    不豁免（假红，方向安全）；显式 offset 写错的伪造声明仍可能落进窗口（见 16）。

因此本门的定位是**把「hash 指得到 / 卡号写得出、话没跟上」的漂移变成机判红点**，不是文档事实核查器。

## 6. 用法

```sh
# 全量（合并前跑；先 git fetch walgit master + refs/collab，否则 rc=2 或基线陈旧）
git fetch walgit master '+refs/collab/*:refs/collab/*'
python -I tools/hygiene/stale_pointer_gate.py

# 只看某个目录 / 换基线
python -I tools/hygiene/stale_pointer_gate.py --path docs/m2 --ref refs/remotes/walgit/master

# 只跑 hash 类（例如没有账本的环境）；会明确打印「已跳过」
python -I tools/hygiene/stale_pointer_gate.py --no-ledger

# 用 board 形状的 JSON 快照离线重放账本
python -I tools/hygiene/stale_pointer_gate.py --ledger-json /e/tmp/board.json

# 明知账本为空（例如只做了部分 fetch）仍要跑：0 线程默认 rc=2，此开关降级为 NOTE
python -I tools/hygiene/stale_pointer_gate.py --allow-empty-ledger

# 放宽/收紧「声明可信性」容差（默认 86400s = 1 天；见 §1c (d)）
python -I tools/hygiene/stale_pointer_gate.py --decl-tol 172800

# 自测（合成 git 仓库，不依赖真实仓库状态与真实账本）
python -I tools/hygiene/stale_pointer_gate.py --selftest
```

`--ref` 解析不到、白名单缺失/格式错、账本不可读 ⇒ **rc=2**（fail-closed，不静默通过）。
`--no-ledger` 与 `--ledger-json` 互斥（rc=2）。

## 7. 自测（`--selftest`）

31 条，全部在临时目录里造独立 git 仓库（base commit A = 已合入；带 2026-01-01 日期的祖先 O；
side 分支 commit B = 对象存在但非祖先；卡号类用内存 `Ledger` **或**合成 `refs/collab/inbox` blob，
不需要真 `walgit`）：

| # | 场景 | 期望 |
|---:|---|---|
| ① | A 已合入 + 写「未合入」 | 红（rc=1，点名该行） |
| ② | B 真未合入 | 绿 |
| ③ | A 已合入 + 行内〔订正〕标记 | 绿 |
| ④ | A 已合入 + 白名单命中 | 绿 |
| ⑤ | `deadbeef` 不可解析 | 绿 + unresolved 提示 |
| ⑥ | A 已合入但「未合入」在 `「…」` 内 | 绿 |
| ⑦ | 白名单条目压不到任何行 | 红（rot） |
| ⑧ | 账本 X=closed + 行写「卡 X 未合入」 | 红，`state == closed` |
| ⑨ | 账本无 X + 行写「卡 X 未开始」 | 绿 |
| ⑩ | 账本 X=in-progress 但 `merge_result{merged:true}` + 行写「卡 X 待开工」 | 红，`state` 以 `merged` 开头 |
| ⑪ | 表格行：卡号与未完成词在**不同单元** | 红（同一行即配对；返工前这条断言是「绿」，正是 I-1） |
| ⑫ | 卡号命中位于 `「…」` 内 | 绿 |
| ⑬ | `--no-ledger`（scan + 子进程 CLI） | 绿且**打印「已跳过」**（rc=0） |
| ⑭ | `--path .`（仓库根）扫描 + 腐烂白名单 | 红（rot，验证 §4 的 `in_scope` 归一化） |
| ⑮ | **board 缺卡、`refs/collab/inbox` 有该卡的 status+merge_result** | 并集补回（`on_board=False`、`merged=True`）并判红 |
| ⑯ | `_fold_entries`（最新 status + 存在 merge）与 `_cards_from_raw`（columns/cards/threads 三形状） | 折叠结果与三形状解析正确 |
| ⑰ | `未接线` / `含未合并` 与卡号同行 | 绿（字面复合词排除，`card_lines == 0`） |
| ⑱ | `--no-ledger` + 只压卡号命中的白名单条目 | 条目仍算 used、`allow_exempt==1`、不报 rot |
| ⑲ | `_merged_flag(True/False/None/"9ff7ff064")` | 只有布尔 `true` 为真（字符串分支已删） |
| ⑳ | 账本 0 线程 + docs 有卡号 | 默认 rc=2；`--allow-empty-ledger` ⇒ rc=0 + NOTE |
| ㉑ | 受检快照声明（祖先 + 时间吻合 + 声明在前），行内既有 stale hash 又有 closed 卡号 | 降级 INFO（`snapshot_exempt==2`），rc=0 |
| ㉒ | 声明 sha 存在但**不是** `--ref` 祖先 | 不豁免，仍红 |
| ㉓ | 声明时间与 commit 差 3600s > `--snapshot-tol` | 不豁免，仍红 |
| ㉔ | 声明写在命中**之后** | 不豁免，仍红 |
| ㉕ | 卡号形状但不在账本（特性名） | rc=0，`card_nonid` 记一条 |
| ㉖ | `--ledger-json` 指向不存在的账本 | rc=2（「查不到」≠「不是卡号」） |
| ㉗ | 声明在首个 `##` **之后** / 在第 **61** 行之后（两条边界各一个文件） | 不降级，两个文件都仍红（`snapshot_exempt==0`）；删掉边界后本检查 FAIL |
| ㉘ | 声明行的事后补提交（blame 时间 = 声称时间 + 30 天） | 不降级，仍红 + `NOTE … > --decl-tol 86400s` |
| ㉙ | 2026-01-01 的祖先用**它自己的真实时间**声明、文档今天写入 | (a)(b)(c) 全过、仅 (d) 判红 ⇒ 仍红 + NOTE |
| ㉚ | 头里 anchor 上一行有无关的 `基线 <sha>`，真 sha 在下一行 | 候选近到远试，真 sha 胜出 ⇒ INFO，且 NOTE 里写明被否的基线候选 |
| ㉛ | 声明行已 `git add` 但未 commit（blame 全 0 / `Not Committed Yet`） | 不豁免，仍红 + NOTE 点名 blame 不可用的原因 |

实测：`selftest: 31/31 passed`，rc=0（①-⑦ 语义未改；⑧-⑳ 为第二类卡，㉑-㉖ 为首版快照/卡号修正，
㉗-㉚ 为返工：㉗ 钉住文档头两条边界、㉘㉙ 钉住声明可信性 (d) 的两类伪造、
㉚ 钉住「相邻基线 sha 不再吞掉真声明」；㉛ 为 blame fail-closed：声明行已 stage 未 commit ⇒ 不豁免）。

## 8. 告警还是阻断（留给 Leader 合并时定）

建议：**先作为独立工具（可 rc=1）挂到「改 docs 的自查」里，不直接进本地必跑四门禁**。理由：

- 第一类依赖 `refs/remotes/walgit/master` 已 fetch；没 fetch 的环境会 rc=2，基线陈旧则假绿（§5-11）。
- 第二类依赖协作账本（`walgit` + `refs/collab/*`）：无账本的环境必须显式 `--no-ledger`，否则 rc=2。
- 一旦变红是真问题，适合作为**合并前一条廉价检查**（实测约 2 秒）。

若接入门禁清单，建议命令固定为：

```sh
git fetch walgit master '+refs/collab/*:refs/collab/*' \
  && python -I tools/hygiene/stale_pointer_gate.py
```

## 9. 残留与未做

- 未改任何 `src/**`、规则、场景、whitelist；未起栈、未连库、未占端口、未写生产库。
- 未把本门写进 `AGENTS.md` 的本地门禁清单（卡面明确「是否接入由 Leader 在合并时定」）。
- 快照声明只从**文档头**识别（首个 `## ` 之前、前 60 行）；正文中段声明的「本节快照」不生效。
  这是有意的保守选择：整份豁免的代价太大，宁可假红让人补一个头部声明（见 §5-16 的伪造风险）。
  **两条边界分别由 selftest ㉗ 钉住**——首版缺这条夹具，删掉边界后 26 条全过、真漂移被洗绿。
- 声明可信性 (d) 只能抓「声明行是后来才提交的」；**同一 commit 里连声明带断言一起写**的伪造
  在 blame 上无差异，仍靠人工复核（§5-16）。blame 不可用时不豁免（fail-closed），
  因此本地带未提交的文档改动会让声明暂不生效。
- 卡号存在性以**可读账本**为前提；账本落后会把新卡误判为「非卡号」并打 INFO（§5-17），
  合并前仍应 `git fetch walgit '+refs/collab/*:refs/collab/*'`。
- 未做 `docs/**` 以外的扫描（`mir2-rs/tests/parity/*.md`、thread body、代码注释里的旧断言仍靠人肉）。
- 第二类只支持**未折叠与已折叠（snapshot + tail）**两种账本形态；`gc` 之后若快照格式再变，门会
  fail-closed（rc=2）而不是静默绿。
- 白名单现有 12 条中 10 条是「带日期时点快照」（且逐条按 blame vs merge 时间重核过），这是本轮接入的
  处置，不是常态增长：新增条目必须满足「该行是历史记录、改写即伪造历史」，活文档优先就地加〔订正〕。
  **能就地订正的一律不进白名单**（本轮把 `rewrite-remaining-inventory-20261022.md:376` 从白名单改为就地订正，
  就是这条的落地）。
- 本报告自身的示例行（如 §3.2 引用的旧 head、§3.4 表格里的卡号与状态词）已用引号/订正标记隔离；
  返工后判据是「同一行」，原来的「跨格隔离」不再豁免，故本报告所有卡片状态词都写成「…」引用。
  本文件被门扫时是绿的，这是规则正常应用，不是特例豁免。
- 未做的两件事：`tools/hygiene/stale_pointer_gate.py` 仍不扫 `docs/` 之外（thread body、代码注释）；
  board 投影漏线程属 `walgit` 侧行为，本卡只绕开、未修（已在该卡线程留证据）。
