# `ignore` 规则的每侧计数 + 形态比较（卡 `openmir2-ignore-rule-count-compare`，tier R）

- 卡：`openmir2-ignore-rule-count-compare`（issue `d29f757910`；owner `openmir2-dev-1`；
  branch `card/ignore-rule-count-compare`；base = 本 patch 的 `--base`）。
  写集：`tools/conformance/compare_runs.py`、`tools/conformance/selftest_impl.py`、本文件。
- **不 push、不启服务端、不跑 cargo**（walgit 故障期 + tester 的 W1 批次在跑机器）。

## 1. 问题

`rules.json` 里 ident 14（`SM_HIT`）是唯一的 `action=ignore` 规则，`approved_by` 仍为 PENDING。
原判据（CH-2）是：**PENDING 的 ignore 只要真的剔了帧 ⇒ 整轮 `UNDECIDABLE(rc=2, pending_ignore_rule)`**。
后果：W1 的 C 类场景跑得动却定不了判——
n=4 `dice_chain_auto`（0 diff）、n=8 `move_walk`（19 diff）都卡在这一条。

reviewer-2 当年的保留条件是「**直到 fixedhide 卡落地 或 工具加计数/形态比较**」。本卡做后半句。

## 2. 设计：计数是硬判据，形态是加强项

- **留痕（③）**：每条 ignore 规则、**每侧**都记命中计数和形态分布，进报告：
  - `frames.ignored_counts = {"oracle": {"14": n}, "rust": {...}}`；
  - `frames.ignored_rule_census[*]`：`{ident, action, approved_by, pending, oracle, rust, delta,
    counts_equal, shape_equal, shape_fields, shapes:{oracle:{...}, rust:{...}}}`；
    `shape_fields` **如实**列出 `param/tag/series` + **`body_sha256`**（follow-up F7：
    只报前三级会让「体不同」被读成「param/tag/series 不同」，而这三项完全相同）；
  - 终端摘要每个有命中的规则打一行（`ident=14 oracle=1 rust=1 delta=+0 shape=same => counted`）。
- **形态（④d 加强项）**：`IGNORE_SHAPE_FIELDS = ("param", "tag", "series")` **+ `body_sha256`**，
  每条规则按「形态串 → 次数」**多重集**比较（键 = `"param,tag,series|<body_sha256>"`，
  body 取 `fr.get("body_sha256") or sha256(fr["body"])`——`load_stream` 本来就两个字段都存，
  见 `compare_runs.py:513`）⇒ 「同量同头、体不同」也会红（reviewer-20 F2）。
  **不含 `recog`**：`SM_HIT(14)` 的 recog 是攻击者 **ActorId**
  （`src/M2Server/Player/PlayObject.Message.cs:1436`
  `MakeMessage(SM_HIT, processMsg.ActorId, nParam1, nParam2, wParam)`），两侧 ActorId 分配互不相关，
  比 recog 只会制造假红。残余风险见 §4。
- **判定（②，保护升级而不是取消）**：对**所有** `action=ignore` 规则（`pending` 或已批准）一视同仁——
  | 两侧计数 | 形态 | 判定 |
  |---|---|---|
  | 相等 | 相等（含体哈希多重集） | **放行**：记 `ignored_ident_rules_counted`（「已计数、形态已比」），不再 UNDECIDABLE |
  | 相等 | 不同 | **UNDECIDABLE**：`pending` → `pending_ignore_rule`，已批准 → `ignore_rule_mismatch`；detail 点明「形态不同」+ 两侧形态分布 |
  | 不同 | — | **UNDECIDABLE**：同上按 `pending` 分 kind；detail 点明 `oracle/rust/delta` |
  **转正不取消兜底**：批准只把 kind 从 `pending_ignore_rule` 换成 `ignore_rule_mismatch`，
  计数或形态对不上**仍是整轮 UNDECIDABLE**。旧文本「非 PENDING 只留痕、不参与判定」是转正前语义，
  已作废（reviewer-21 F1；ident 14 于 2026-10-29 转正后这条空洞会被直接利用）。
- **补 0 侧（reviewer-20 F3）**：`ignored_counts` 对任一侧命中过的 ident **两侧都建键**（没命中补 0），
  直接读 `ignored_counts["rust"]["658"]` 不会 KeyError；census 里 `oracle/rust` 也一并对齐。
- **不放宽现有边界**：CLI `--ignore-idents` 仍然独立（不计入规则计数），
  `pending_ignore_hits` 字段保留（向后兼容：仍是「被 PENDING 规则剔过的 ident」）。
- **判据其实是一件事（reviewer-20 F4，登记）**：`counts_equal` 被 `shape_equal` 蕴含
  （形态多重集的元素数之和就是计数）⇒ 真正起作用的是**形态多重集比较**，计数是给人和排障看的
  诊断量，**不是**独立于形态的第二道保障。转正 14 时请照此措辞，别写成「计数 + 形态两条独立保障」。

## 3. 牙（`compare_runs selftest`）

新增/改写 `t_pending_ignore`（原 1 条断言 → 4 条）：

- **牙a**：两侧各 1 帧 658（PENDING ignore），计数/形态相同 ⇒ **rc 0**，
  `ignored_ident_rules_counted == [658]`，`ignored_counts` 两侧各 1，且不再出 `pending_ignore_rule`；
- **牙b**：oracle 侧多 1 帧 ⇒ **rc 2**，`pending_ignore_rule` 点名 `oracle=2/rust=1/delta=+1`；
- **牙c**：rust 侧 0 帧 ⇒ **rc 2**，点名 `oracle=1/rust=0/delta=+1`（不许静默放过）；
- **牙d**：计数相等但头部形态不同（oracle 侧 658 的 `param` 改 9）⇒ **rc 2**，
  `counts_equal=true / shape_equal=false`，两侧形态分布都进报告；
- **牙e（F6）**：两侧各 3 帧、总数相同、分布**互换**（`{1×2,2×1}` vs `{1×1,2×2}`）⇒ **rc 2**
  ——锁住「只比 distinct 集合」的退化（变异实测：把 `o_s == r_s` 改成 `set(o_s) == set(r_s)` ⇒ 牙e 红）；
- **牙f（F2）**：计数与头部形态相同但 **body 不同** ⇒ **rc 2**（形态含 `body_sha256`；
  变异实测：把形态键里的 body 去掉 ⇒ 牙f 红）；
- **牙g（F7）**：同一例里 `shape_fields` 与 `undecidable.detail` 都必须点名 `body_sha256`
  （变异实测：`shape_fields` 退回只报 `param/tag/series` ⇒ 牙g 红——报错串会误导成「三项不同，而它们相同」）。

守护「ignore 真在剔帧」的既有牙不变且仍绿：把 14 改回 `compare` ⇒ rc 1；
已批准 ignore 下多出的 14 ⇒ rc 0 且 `ignored` 记 1 条；
内置 14 为 PENDING 且两侧不同量（1 vs 0）⇒ 仍 rc 2（这正是牙c 的真实形态）。

`python -I -B tools/conformance/compare_runs.py selftest` ⇒ **322/322 GREEN**（改动前 316/316）。

## 4. 残余风险（登记，不隐瞒）

1. **recog 不参与形态比较**（ActorId 侧别独立）⇒ 「两侧同样次数、但是不同攻击者打的」在本判据下
   视为同形。要补只能先给 recog 找**跨侧可比**的等价物（例如按实体属性/位置归因），目前没有。
   **这是对验收 ④d 示例（「recog 不同 ⇒ 必须红」）的显式偏离**：按裁决登记于此；
   **转正 14 时必须在 `rules.json` 的 note 与 `docs/rules-pending-adjudication-20261025.md`
   写明「形态未含 recog」**，否则读文档的人会以为 recog 也受保护。
2. ~~body 不参与~~（**已修**，reviewer-20 F2）：`body_sha256` 现在进形态多重集，
   因此「同量同头、体不同」会红；仍未比的是 body **内容本身的可读差异**（只比哈希）。
3. **形态是顺序无关的多重集**（reviewer-21 N1，登记）：同一组形态、**顺序互换** ⇒ `rc 0`，
   不会触发 UNDECIDABLE。对 14 这类「被剔除、不进有序对齐」的帧可辩护（它们本来就不参与序列比较），
   但要知道：剔帧的**相对顺序差异**只能靠 `census` 的形态分布观察，不是判据。
4. **形态相等 ≠ 逐帧同源**：两侧各剔 1 帧 SM_HIT，也可能是「不同时刻、不同来源」的两帧；
   本判据保证的是**量级 + 头部形态 + body 哈希多重集**一致，不是逐帧同源。
4. **跨侧巧合**：两侧都因随机支触发同一 ident 时计数可能偶然相等（应然情况）；若一侧随机支触发、
   另一侧因 bug 多打帧，计数大概率不等就会被抓；但**理论上**存在「多打一次 + 少打一次」相互抵消的
   极端情形——这类要靠场景设计（固定 NPC、固定攻击者）从源头消除，计数比较不承诺覆盖。

## 5. 未做

- **真机复跑（⑥）**：W1 的 n=4 `dice_chain_auto`（应转为 PASS/FAIL 可判）与 n=8 `move_walk`
  （应给出可读差异）——机器由 tester 排期（本卡不占窗口）。
- **ident 14 转正（⑤）—— 已由 Leader 裁定「不转正」（2026-10-25，master `0f172ee7c`）**：
  reviewer-21 第二意见实测——计数/形态比较**只对 PENDING 生效**；一旦转正，「形态不等」也只进
  census、verdict 仍可绿 ⇒ **保持 PENDING 反而更严**。所以 `rules.json` 的 14 **不动**，
  原 ⑤ 作废；`docs/rules-pending-adjudication-20261025.md` §6b 已记录该裁定与残余风险。
  以下为原计划，保留作历史说明：
  转正要按 Leader 的裁定流程另出一份决定（并在 `rules.json` 的 note、
  `docs/rules-pending-adjudication-20261025.md` 里写明「转正依据 = 计数/形态比较已落地」并保留
  §4 的残余风险）。**本卡未擅自翻 `approved_by`**——那不是作者能自签的决定。
- 真机复跑之前，`tier=R` 卡保持 `in-progress`，`window_required: true`。

## 6. 门禁

`compare_runs selftest` 319/319；`conflict_marker` / `stale_pointer --ref master` /
`dynamic_count --ref master` / `board_coverage` / `t_reservation check` /
`scenario_undecidable_audit` 全 rc=0；`git diff --check <base> <head>` rc=0；
`merge_tree_helper` vs_base/vs_master 均 rc=0。未跑 cargo、未起栈、未占端口。
