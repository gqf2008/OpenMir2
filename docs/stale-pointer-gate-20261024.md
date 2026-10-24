# 过期指针门（`stale_pointer_gate`）—— 首次接入报告

- 卡：`openmir2-docs-stale-pointer-gate`（tier=L，write_set = `tools/hygiene/` + `docs/`）；
  第二类判据由 `openmir2-stale-pointer-card-status-check`（tier=L）追加
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
- **邻近与同一记录**：卡号与词距离 ≤ `--max-gap`（默认 24）；Markdown 表格行按 `|` 切成单元，
  **同格**才算配对（表格行是行内序列化的多条记录，跨格不是断言）。
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
  与第一类的 rc=1 区分。`--ledger-json PATH` 用 board 形状的 JSON 替代 `walgit collab board`（不再起
  `walgit`），并继续合并本地 snapshot/inbox 里的 merge 旗标；本地没有 `refs/collab/*` 时只能判 `closed`，
  判不了 `merge_result`（这是离线快照的已知局限）。
- **白名单腐烂**：白名单条目对两类命中都生效；`--no-ledger` 时，带卡号的候选行仍被视为「可能命中」，
  以免把只压卡号命中的历史条目误报为腐烂。

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

全量实跑：**136 个 md / 32396 行 + 账本（509 张 board 卡 + 11 张仅在 tail 的线程），17 处卡状态命中 / 16 行**，
约 2 秒。逐条：

| # | 位置 | 卡号 | 账本状态 | 事实 / 处置 |
|---:|---|---|---|---|
| 1 | `docs/Rust重写设计文档.md:695` | `openmir2-m5-convergence-drills` | merged（in-progress） | 行内「未开始/待认领」指 §17.1 其余四项；阶段0 已合入 `2f0b102c5` ⇒ **就地订正** |
| 2 | `docs/Rust重写设计文档.md:890` | `openmir2-rng-consumer-structure-parity` | merged（in-progress） | 方案 C 已合入 `e7d432189`，卡已认领 ⇒ **就地订正** |
| 3 | `docs/Rust重写设计文档.md:891` | `openmir2-m5-convergence-drills` | merged（in-progress） | 同上（Claim B/M5 两行） ⇒ **就地订正** |
| 4 | `docs/docs-state-drift-sweep-20261024.md:48` | `openmir2-sm-skeleton-wiring` | merged（in-progress） | 漂移表「订正」列的时点记录 ⇒ **白名单** |
| 5 | `docs/docs-state-drift-sweep-20261024.md:48` | `openmir2-dualrun-window-burndown` | closed | 同上（同行第二张卡） ⇒ **白名单** |
| 6 | `docs/m3-remaining-inventory-20261022.md:46` | `openmir2-monster-run-overrides` | merged（in-progress） | 2026-10-22 盘点快照（当时实现未开） ⇒ **白名单** |
| 7 | `docs/r-window-registry-batch-1-20261023.md:66` | `openmir2-scenario-spell-cast` | closed | 2026-10-23 批次登记表（当时「未合」；后由 `b13b29e39` 合入） ⇒ **白名单** |
| 8 | `docs/r-window-registry-batch-1-20261023.md:69` | `openmir2-scenario-spell-cast` | closed | 同上 ⇒ **白名单** |
| 9 | `docs/r-window-registry-batch-1-20261023.md:70` | `openmir2-scenario-spell-cast` | closed | 同上 ⇒ **白名单** |
| 10 | `docs/rewrite-remaining-inventory-20261022.md:376` | `openmir2-m3-fixture-marker-prefix-bug` | merged（in-progress） | 2026-10-22 剩余清单快照（当时未认领） ⇒ **白名单** |
| 11 | `docs/session-summary-20261023.md:39` | `openmir2-script-sendmsg-host` | merged（in-progress） | 2026-10-23 会话纪要（当时待认领） ⇒ **白名单** |
| 12 | `docs/spell-cast-window-blockers-20261024.md:213` | `openmir2-spell-frame-compare-rules` | closed | 「建议 id（未开）」已过时：该 id 已立卡并在 `89a3d02fd` 合入 ⇒ **就地订正** |
| 13 | `docs/spell-cast-window-blockers-20261024.md:237` | `openmir2-spell-frame-compare-rules` | closed | 同上 ⇒ **就地订正** |
| 14 | `docs/tmp-cleanup-proposal-20261017.md:3` | `openmir2-tmp-cleanup-audit-redo` | merged（in-progress） | 2026-10-17 提案抬头（当时待认领） ⇒ **白名单** |
| 15 | `docs/window-round3-runbook-20261021.md:1016` | `openmir2-merchant-race-bias` | merged（in-progress） | 2026-10-21 当日 BLOCKED 记录（后由 `a8ff9fdf9` 合入） ⇒ **白名单** |
| 16 | `docs/window-round3-runbook-20261021.md:1034` | `openmir2-merchant-race-bias` | merged（in-progress） | 同上 ⇒ **白名单** |
| 17 | `docs/交接-OpenMir2编排与联调-20261015.md:16` | `openmir2-m5-convergence-drills` | merged（in-progress） | 2026-10-15 交接清单（当时待认领） ⇒ **白名单** |

**为什么不是几十条**：不加「同一表格单元 + `--max-gap` 邻近」时，同一行里「卡号在某格、未完成词在另一格」的
长表会产出 60+ 条，其中多数不是对卡的断言（如「A/B 已合，D 未开工」并列多张卡）。两条定位规则只排除
**跨格/远距**配对，词表一个字都没减。**残留误报类**：同一格内「卡的某个子项未开」仍可能被当成卡状态
（例：`docs/Rust重写设计文档.md:695` 的「未开始」其实指里程碑其余四项）——这类只能靠就地订正/白名单登记。

**第二类抓不到本次的 `f5a2e485b` 实例**：`attack-weapon-wear-642-20261024.md:173` 那一行**没有卡号**，
只有 hash。所以第二类不是第一类的替代，而是补上「rebase 后 hash 非祖先、但同一行写了卡号」那类。
今天 17 处命中里，`closed` 6 处（3 张卡：`openmir2-dualrun-window-burndown`、`openmir2-scenario-spell-cast`、
`openmir2-spell-frame-compare-rules`），`merged:true` 11 处；第一类对这些行全是绿。

## 4. 白名单：条目数与防腐烂验证

当前 **13 条**：第一类 2 条（§3 表 #3/#4），第二类 11 条（§3.4 表 #4-#11/#14-#17，全部是带日期的
时点快照）。实跑汇总 `13 allow-listed line(s)`。腐烂验证：

```sh
cp tools/hygiene/stale_pointer_allowlist.toml /tmp/rot.toml
# 追加一条指向 docs/stale-pointer-gate-20261024.md 的伪条目
python -I tools/hygiene/stale_pointer_gate.py --allowlist /tmp/rot.toml
# => ERROR allow-list rot: docs/stale-pointer-gate-20261024.md (entry #14) …
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
   卡号类另有**同格**约束（表格行按单元切开），跨格的「卡号 … 未完成词」不判。
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
    - 账本本身没同步（`refs/collab/*` 没 fetch/没 gc）⇒ 卡号查不到即绿。门只能看见它读到的账本；
      账本按条为空而 board 有卡时已 fail-closed（rc=2），但「账本存在、只是旧的」无法自动识别。
13. **`merged:true` 是存在性判据**：卡曾合并、其后又 `merged:false`（返工失败）仍按已合并判红；
    这是验收口径的保守方向，宁可多报不漏报。
14. **`--no-ledger` 的绿**：显式关闭第二类时打印「已跳过」，第一类照常判。这个绿不包含卡状态结论。
15. **表的投影与 tail 的差**：`walgit collab board` 实测漏掉 11 张线程（含 1 张 `closed`）；门因此不用
    board 当唯一来源，而是「board 状态 + snapshot 折叠区 + inbox tail」合并判。board 缺席的线程仍可判。

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
python -I tools/hygiene/stale_pointer_gate.py --ledger-json /tmp/board.json

# 自测（合成 git 仓库，不依赖真实仓库状态与真实账本）
python -I tools/hygiene/stale_pointer_gate.py --selftest
```

`--ref` 解析不到、白名单缺失/格式错、账本不可读 ⇒ **rc=2**（fail-closed，不静默通过）。
`--no-ledger` 与 `--ledger-json` 互斥（rc=2）。

## 7. 自测（`--selftest`）

14 条，全部在临时目录里造独立 git 仓库（base commit A = 已合入；side 分支 commit B = 对象存在但非祖先；
卡号类用内存里的合成 `Ledger`，不需要 walgit）：

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
| ⑪ | 表格行：卡号与未完成词在**不同单元** | 绿 |
| ⑫ | 卡号命中位于 `「…」` 内 | 绿 |
| ⑬ | `--no-ledger`（scan + 子进程 CLI） | 绿且**打印「已跳过」**（rc=0） |
| ⑭ | `--path .`（仓库根）扫描 + 腐烂白名单 | 红（rot，验证 §4 的 `in_scope` 归一化） |

实测：`selftest: 14/14 passed`，rc=0（原 7 条语义未改）。

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
- 未做 `docs/**` 以外的扫描（`mir2-rs/tests/parity/*.md`、thread body、代码注释里的旧断言仍靠人肉）。
- 第二类只支持**未折叠与已折叠（snapshot + tail）**两种账本形态；`gc` 之后若快照格式再变，门会
  fail-closed（rc=2）而不是静默绿。
- 白名单现有 13 条中 11 条是「带日期时点快照」，这是本轮首次接入的处置，不是常态增长：
  新增条目必须满足「该行是历史记录、改写即伪造历史」，活文档优先就地加〔订正〕。
- 本报告自身的示例行（如 §3.2 引用的旧 head、§3.4 表格里的卡号与状态词）已用引号/订正标记/跨格隔离，
  故本文件被门扫时是绿的；这是规则正常应用，不是特例豁免。
