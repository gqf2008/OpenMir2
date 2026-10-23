# 过期指针门（`stale_pointer_gate`）—— 首次接入报告

- 卡：`openmir2-docs-stale-pointer-gate`（tier=L，write_set = `tools/hygiene/` + `docs/`）
- 交付：`tools/hygiene/stale_pointer_gate.py` + `tools/hygiene/stale_pointer_allowlist.toml` + 本报告
- 基线：`refs/remotes/walgit/master = 55a8e7fca`
- 只读仓库：不改 `src/**`、`rules.json`、场景 JSON、`mir2-rs/tests/parity/whitelist.md`；不起栈、不连库。

本卡起因是今天四处「文档说未完成、commit 早已合入」全靠人肉撞见。这个门把「带 commit hash 的未完成断言」
变成机判：**hash 已是目标分支祖先 ⇒ 报红**。

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

退出码：`0` 干净；`1` 有过期指针 / 白名单腐烂 / 扫不了的文件（fail-closed）；`2` 不在 git 仓库 /
`--ref` 解析不到 / 白名单缺失或格式错（fail-closed）。

## 2. 三条豁免（都要求行内可见）

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

## 4. 白名单：条目数与防腐烂验证

当前 **2 条**（见 §3 表 #3/#4），两条都会被命中一次（实跑汇总 `2 allow-listed line(s)`）。腐烂验证：

```sh
cp tools/hygiene/stale_pointer_allowlist.toml /tmp/rot.toml
# 追加一条指向 docs/stale-pointer-gate-20261024.md 的伪条目
python -I tools/hygiene/stale_pointer_gate.py --allowlist /tmp/rot.toml
# => ERROR allow-list rot: docs/stale-pointer-gate-20261024.md (entry #3) …
#    stale-pointer gate: FAIL (0 stale pointer(s), 1 allow-list rot, 0 error(s)) ，rc=1
```

`--selftest` 里的 ⑦ 也是同一断言（合成仓库、不依赖真实仓库状态）。

## 5. 这个门抓不到什么（别把绿当「文档全对」）

**门绿只说明：被扫的 `docs/**/*.md` 里，没有「已知 commit hash + 邻近未完成词 + hash 已是目标分支祖先」的行。**
以下都抓不到：

1. **没写 hash 的口头断言**：如「`preflight.py` 尚未合入」「某卡在办」而不带 commit —— 门看不见。
   （今天不少漂移正是这种形态。）
2. **hash 对，但语义被改写**：写「`abc1234` 已合入…但功能未接」时，门不会判——它只回答「该 hash
   是否祖先」，不回答「功能是否真接」；反之
   「`abc1234` 未合入」若 hash 拼错成另一个存在但无关的 commit（极低概率），门也按那个 hash 判。
3. **hash 不是祖先但内容已合入**：rebase / squash / force-push / 返工重提。已在 §3.2 用 `f5a2e485b`
   实测登记——**这是最常见的一类假绿**。
4. **unresolved 指针**：线程 entry oid、别仓 hash、已 GC 对象，以及**拼错的 hash**，一律不判红。
5. **全数字短 hash**：规则要求至少一个 `a`–`f` 字母；全数字的 7 位缩写 hash 会被当成数字跳过
   （换来的收益是不把日期 `20261023` 当 hash）。
6. **距离 > `--max-gap`（24）**：同一行但隔得远的「hash … 未完成词」不判（多为两件事，也可能是真漂移）。
7. **引号内的断言**：真的过期表述若被作者自己写进 `「…」`，门会当引用放过。
8. **订正标记行**：行内有 `YYYY-MM-DD 订正` 就整行豁免，**不校验订正内容是否覆盖了该 hash**。
9. **词表外的状态词**：仍缺 / 待建 / 阻塞 / 未验收 / 未实现 / needs-review 等不在词表内，不触发。
10. **范围与口径**：只扫 `docs/`（`--path` 可改）；只对 `--ref`（默认 `refs/remotes/walgit/master`）判祖先；
    跨分支/跨仓「已合入某分支」的断言不在判据内；非 md 文件（thread、卡片、代码注释）不扫。

因此本门的定位是**把今天那类「hash 指得到、话没跟上」的漂移变成机判红点**，不是文档事实核查器。

## 6. 用法

```sh
# 全量（合并前跑；先 git fetch walgit master，否则 --ref 解析失败 rc=2）
python -I tools/hygiene/stale_pointer_gate.py

# 只看某个目录 / 换基线
python -I tools/hygiene/stale_pointer_gate.py --path docs/m2 --ref refs/remotes/walgit/master

# 自测（合成 git 仓库，不依赖真实仓库状态）
python -I tools/hygiene/stale_pointer_gate.py --selftest
```

`--ref` 解析不到时 **rc=2**（fail-closed，不静默通过）；白名单缺失/格式错也是 rc=2。

## 7. 自测（`--selftest`）

7 条，全部在临时目录里造独立 git 仓库（base commit A = 已合入；side 分支 commit B = 对象存在但非祖先）：

| # | 场景 | 期望 |
|---:|---|---|
| ① | A 已合入 + 写「未合入」 | 红（rc=1，点名该行） |
| ② | B 真未合入 | 绿 |
| ③ | A 已合入 + 行内〔订正〕标记 | 绿 |
| ④ | A 已合入 + 白名单命中 | 绿 |
| ⑤ | `deadbeef` 不可解析 | 绿 + unresolved 提示 |
| ⑥ | A 已合入但「未合入」在 `「…」` 内 | 绿 |
| ⑦ | 白名单条目压不到任何行 | 红（rot） |

实测：`selftest: 7/7 passed`，rc=0。

## 8. 告警还是阻断（留给 Leader 合并时定）

建议：**先作为独立工具（可 rc=1）挂到「改 docs 的自查」里，不直接进本地必跑四门禁**。理由：

- 它依赖 `refs/remotes/walgit/master` 已 fetch；没 fetch 的环境会 rc=2，噪声大。
- 它只覆盖 docs 的 hash 型漂移，覆盖面窄，但一旦变红是真问题，适合作为**合并前一条廉价检查**。

若接入门禁清单，建议命令固定为：

```sh
git fetch walgit master && python -I tools/hygiene/stale_pointer_gate.py
```

## 9. 残留与未做

- 未改任何 `src/**`、规则、场景、whitelist；未起栈、未连库。
- 未把本门写进 `AGENTS.md` 的本地门禁清单（卡面明确「是否接入由 Leader 在合并时定」）。
- 未做 `docs/**` 以外的扫描（`mir2-rs/tests/parity/*.md`、thread body、代码注释里的旧断言仍靠人肉）。
- 本报告自身的示例行（如 §3.2 引用的旧 head）已用引号/订正标记隔离，故本文件被门扫时是绿的；
  这是「引号内不计」规则的正常应用，不是特例豁免。
