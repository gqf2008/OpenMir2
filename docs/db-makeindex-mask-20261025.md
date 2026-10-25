# DB 面 `characters_bagitem.MakeIndex` 掩码 + 结构牙（卡 `openmir2-db-makeindex-mask`，tier L）

- 卡：`openmir2-db-makeindex-mask`（issue `ea83fc74`；owner `openmir2-dev-1`；
  branch `card/db-makeindex-mask`；base = 本 patch 的 `--base`）。
  写集：`tools/conformance/rules.json`、`tools/conformance/compare_runs.py`、
  `tools/conformance/selftest_impl.py`、`tools/db-parity/dbsnap_pair.py`、
  `mir2-rs/tests/parity/whitelist.md`、本文件。

## 1. 问题

`characters_bagitem.MakeIndex` 属已登记 **D-1**（`mir2-rs/tests/parity/whitelist.md:24`）：
oracle `M2Share.GetItemNumber()`（`M2Share.cs:547`）返回「计数器 + `HUtil32.GetTickCount()`」，
**含墙钟** ⇒ 两次 C# 运行都不一致，不存在可对拍真值。但 DB 对比没有对应掩码，一旦真机跑出
「新生成的物品」，DB 面就会多一条永不消的差异。本卡把**同一条 D-1 依据**从「掉落对拍」推广到「DB 列」
（抬头已在 `whitelist.md` 的 D-1 行登记这条推广）。

## 2. 实现（单处：`rules.json` 的 `db.masked_columns` → `dbsnap_pair` 消费）

```json
"masked_columns": {
  "mir2_db.characters_bagitem": {
    "MakeIndex": {"reason": "D-1：…（M2Share.cs:547 计数器+GetTickCount）",
                  "ref": "D-1 / mir2-rs/tests/parity/whitelist.md:24",
                  "group_by": "PlayerId",
                  "applies_to": "dbsnap-direct",
                  "require": ["nonzero_both_sides", "distinct_per_player", "changes_on_mutation"]}
  }
}
```

- 加载期 fail-closed：`reason`/`ref` 必须有；`require` 必须非空且只含三种已登记检查（**掩掉一列必须带牙**）；
  **声明 `changes_on_mutation` 就必须同时登记 `applies_to: "dbsnap-direct"`**——该检查需要 before 快照，
  而 `compare_runs` 没有 `--db-baseline-*` 透传（F1 裁决），不登记作用域就等于「三牙」退化成
  「两牙 + 一个永远 skipped 的名字」（`牙MI-7` 咬这条）。
- 掩码列从值比较里排除，但**同时**跑结构牙（`dbsnap_pair.masked_column_checks`）；
  每条掩码发一条 `masked_column_applied` 证据，由 `compare_runs` 写进 `report["whitelisted"]`
  （`kind=db_column_masked`，带 `reason`/`ref`/`checks`/`applies_to`/**`occupied_a`/`occupied_b`**
  两侧占用行数）——**不许静默丢弃**，也不许「文档承诺了字段、实现没给」。

## 3. 结构牙的语义（**按真实数据校正过**，与卡面字面不同，必须留痕）

> **口径已追认**：两条重定义由 orchestrator 在卡上 comment `244d9f39` 正式采纳
> （「验收口径按作者版本生效，卡面字面作废」），本文件不再是与卡面对抗的孤证。

复算 `E:/tmp/window/db-rerun/run2` 得到的事实：

1. `characters_bagitem` 把**空槽也物化成行**且 `MakeIndex=0`（实测 330,702 / 337,640 行为 0）
   ⇒ 「每一行都非 0」**在数据上不可能成立**，不是判据。改为按**占用的槽**（`MakeIndex != 0`）定义：
   - `nonzero_both_sides`：**两侧占用槽数都 > 0 且相等**（防「一侧根本没写」）；
2. 同一 `PlayerId` 内**合法重复**确实存在（run2 `login_only` 的 PlayerId=6227：28 个占用槽、25 个不同值）
   ⇒ 「各槽互不相同」同样不成立。改为 `distinct_per_player`：**占用槽 ≥2 的玩家，不同取值数必须 ≥2**
   （防「全填同一个常量」）；
3. `changes_on_mutation`：给了 before 快照（`--baseline-a/b`）时，**每侧取 `before_keys ^ after_keys`
   的对称差**，键 = `(group_by 值, Position, 取值)`，两侧对称差都为空才判红（防写死常量）。
   用对称差而不是「只遍历 before 的键」，是因为**纯新增一个槽**（卡面 ②(c)「增删物品」的常见形态）
   在旧写法下会被误判成「没变化」；键里带 `Position` 是为了抓住**同值换槽**（F10，reviewer-20 `49e75392`）。
   没有 before 快照时记 `skipped` 并在报告里写明。
   **作用域**：`compare_runs` 管道**恒无** before 快照 ⇒ 这条牙在窗口跑批里永远 `skipped`；
   它**只在 dbsnap 直连**（真给了 `--baseline-a/b`）时可跑。**「直连」指 CLI + API 两处都行**：
   `dbsnap_pair diff --masked-columns FILE`（F12 新增；在此之前只有 `run_pair()` API 能传掩码，
   照文档用 CLI 试会得到「没有掩码」的假象）。这条不是纸面声明——见 §4.2 的直连实证，
   以及为让它真能咬而修的实现（before 行透传 + `masked_column_applied` 不算差异）。

任一不成立 ⇒ `masked_column_violation`（**差异**，不是「判不了」）。

## 4. 牙与变异

`compare_runs selftest` **343/343**（新增八颗）：

| 牙 | 断言 |
|---|---|
| MI-1 | 两侧仅 `MakeIndex` 不同：**无掩码 ⇒ rc 1（有 db 差异）**；**加掩码 ⇒ rc 0 且 `whitelisted` 有一条 `db_column_masked`、`ref` 含 `D-1`** |
| MI-2 | 一侧占用槽全 0 ⇒ `nonzero_both_sides` 判红 |
| MI-3 | 同一玩家两槽同值 ⇒ `distinct_per_player` 判红 |
| MI-4 | before/after 有变化 ⇒ 通过；完全相同 ⇒ `changes_on_mutation` 判红 |
| MI-5 | **用仓库 `rules.json` 本身**（不传 `--rules`）跑同一构造：rc 0 且 `whitelisted` 点名 `D-1`、`skipped` 含 `changes_on_mutation` |
| MI-6 | `whitelisted` 真带 `occupied_a`/`occupied_b`（相等且 >0；run2 实测 6938/6938）且 `applies_to=dbsnap-direct` |
| MI-7 | 声明 `changes_on_mutation` 却不登记 `applies_to` ⇒ **加载期 rc 2**（不许退化成永远 skipped） |
| MI-8 | **纯新增**一个占用槽（`[101,102] → [101,102,103]`）⇒ rc 0（旧「只看 before 键」写法会判红） |
| MI-9 | **同值换槽**（Position 0/1 互换）⇒ rc 0（键里不带 Position 会被当成没变化） |
| MI-10 | before 快照**列不符** ⇒ `masked_column_violation(check=changes_on_mutation)`、rc 1（不许静默 skipped） |
| MI-12 | `applies_to` **非法值**（`"pipeline"`，且 require 不含 `changes_on_mutation`）⇒ rc 2 |

> **为什么有 MI-5**：MI-1 用的是自造规则文件，它证明「机制生效」，但**删掉 `rules.json` 里那条登记后
> MI-1..4 仍全绿**（登记被删没人发现 = 没牙）。MI-5 把牙咬在**仓库登记表**上。

### 4.1 变异实证（逐条放宽 ⇒ 对应牙红；输出为当次实跑）

| 变异 | 结果 |
|---|---|
| A `nonzero_both_sides` 放宽为 `if False` | `FAIL 牙MI-2`，`RED: 338/339` |
| B `distinct_per_player` 放宽为 `if False` | `FAIL 牙MI-3`，`RED: 338/339` |
| C `changes_on_mutation` 放宽为 `if False` | `FAIL 牙MI-4`，`RED: 338/339` |
| D **去掉 `rules.json` 的 `db.masked_columns`**（反向） | `FAIL 牙MI-5`，`rc=1`，差异记录 `{"kind":"value","record":{"column":"MakeIndex","a":"101","b":"201"}}` ⇒ 正是「每次都报一条 MakeIndex 差异」的旧行为；`RED: 339/340` |
| E 去掉 `occupied_a/occupied_b` 的透传（F2 承诺落空） | `FAIL 牙MI-6`（`(None, None, 'dbsnap-direct')`）；`RED: 341/342` |
| F 去掉「声明 `changes_on_mutation` 必须登记 `applies_to`」的加载期拒绝 | `FAIL 牙MI-7`（`rc=0`，本该 2）；`RED: 341/342` |
| G 还原「只看 before 的键」（F10 修法回退） | `FAIL masked_changes_on_mutation_add_only`：纯新增被判 `rc=1 violation`；`RED: 34/35` |
| H CLI 不接线（`load_masked_columns` 恒返回空） | `FAIL masked_columns_cli_entry`（masked rc=1 / plain rc=1）；`RED: 34/35` |
| I 删掉 before 列不符的 violation 分支 | `FAIL masked_baseline_column_mismatch_red`（rc=1 但 check=[]）；`RED: 34/35` |
| J 放宽 `applies_to` 取值校验 | `FAIL 牙MI-12`（`rc=0`，本该 2）；`RED: 342/343` |

回归后 `GREEN: compare_runs selftest 343/343`；`GREEN: dbsnap_pair selftest 35/35`。

### 4.2 F1 实证：dbsnap **直连**的 before/after 三态（`changes_on_mutation` 真会咬）

`python -I -B tools/db-parity/dbsnap_pair.py selftest` 现在就带这颗（`masked_changes_on_mutation_direct`）：

```text
PASS masked_changes_on_mutation_direct      no-baseline rc=0 skip=['changes_on_mutation'] /
     same rc=1 viol=['changes_on_mutation'] / changed rc=0 viol=0
PASS masked_changes_on_mutation_add_only    rc=0 viol=[]（纯新增 101,102 -> 101,102,103）
PASS masked_changes_on_mutation_slot_swap   rc=0 viol=[]（槽位互换 0<->1）
PASS masked_baseline_column_mismatch_red    rc=1 check=['changes_on_mutation']
PASS masked_columns_cli_entry               masked rc=0 / plain rc=1
GREEN: dbsnap_pair selftest 35/35
```

同一个块里还有：`masked_changes_on_mutation_add_only`（**纯新增** ⇒ rc 0，F10）、
`masked_changes_on_mutation_slot_swap`（同值换槽 ⇒ rc 0）、`masked_baseline_column_mismatch_red`
（before 列不符 ⇒ violation，F11a）、`masked_columns_cli_entry`（**CLI `--masked-columns FILE`**
带掩码 rc 0 / 不带 rc 1，F12）。

三态 = ① 不给 `--baseline-*` ⇒ `skipped`（与管道侧同形）；② 给了 baseline 且**前后完全没变**
（把占用槽 `MakeIndex` 写成与前次相同）⇒ `masked_column_violation(check=changes_on_mutation)`、`rc=1`；
③ 某一占用槽真的改了（101→901）⇒ 无违规、`rc=0`。

> **做这条实证时发现并修了两个真缺陷**（都属「这条牙本来不存在」）：
> 1. `compare_tables` 调 `masked_column_checks` 时**根本没传 before 行**——即使给了 `--baseline-a/b`，
>    `changes_on_mutation` 也恒 `skipped`。现在把 `BA/BB` 的对应表行透传（列不符 ⇒ 报
>    `masked_column_violation`，不静默降级）。
> 2. `masked_column_applied` 被算进 `dbsnap_pair` 的 `red`/`diff_counts` ⇒ **掩码生效本身就把 rc 打成 1**。
>    现在证据记录单列 `masked_column_evidence`，不进 `red`（`masked_column_violation` 照常算差异）。

## 5. 复算影响面（`E:/tmp/window/db-rerun/run2`，唯一按新口径跑的 DB 证据）

复现命令（当次实跑；`--ignore-idents 14` 与本批原始 argv 一致；「改前」= 把 `rules.json`
的 `db.masked_columns` 摘掉后另存一份传入）：

```sh
python -I -B tools/conformance/compare_runs.py --oracle <run2>/<场景>/csharp   --rust <run2>/<场景>/rust --out /tmp/x.json --quiet --ignore-idents 14 [--rules <无掩码副本>]
```

| 场景 | 改前：rc / db 差异 / MakeIndex 差异 | 改后：rc / db 差异 / MakeIndex 差异 / whitelist |
|---|---|---|
| gm_batch | 1 / 0 / 0 | 1 / 0 / 0 / 1 |
| gm_time | 0 / 0 / 0 | 0 / 0 / 0 / 1 |
| login_only | 0 / 0 / 0 | 0 / 0 / 0 / 1 |
| merchant_dialog | 1 / 0 / 0 | 1 / 0 / 0 / 1 |
| move_walk | 1 / 5 / 0 | 1 / **5** / 0 / 1 |
| npc_click_auto | 0 / 0 / 0 | 0 / 0 / 0 / 1 |

（`rc` 非 0 的 3 条是**帧面**差异：`gm_batch` / `merchant_dialog` / `move_walk`，与本卡无关；
**其余差异条数改前=改后**。）

每条 `whitelisted` 记录现在带 `occupied_a = occupied_b = 6938`（六场景同值，即两侧占用槽数相等且非 0）。

**如实说明**：run2 这批里 `MakeIndex` 差异**改前就是 0**（两侧 DB 都是同一次 restore 的基线，
物品的 MakeIndex 未被重新生成；进一步核对：六场景 6938 个占用槽**逐 Id 完全相同**）⇒ 本轮掩码在这批
证据上**只能证明「rc/差异条数不变」，不能当「掩码必要性」的证据**。「掩码真生效」由构造级
**牙 MI-1/MI-5** 实证。

**必要性/真实效果的反向证据**在 `e23-reshot`（orchestrator 记录，见 review `49e75392` 的引述）：
旧规则下该场景有 **2 条** db 差异——`characters.Gold 9912/9920` + `characters_bagitem.MakeIndex
645768579/16281`；新规则下剩 **1 条**（Gold）+ 1 条 `masked`（occupied 6939/6939）
⇒ 掩码确实吃掉了一条**只为 D-1 墙钟差**而存在的差异，且没有连带吞掉 Gold。

## 6. 红线与未做

- **不掩帧面**：只动 DB 列；`SM_ADDITEM(200)`/`SM_BAGITEMS(201)` 的 `MakeIndex` 判据（规则表 `bagitems_records`）未改。
- **不顺手掩其它列**：`masked_columns` 只登记了 `characters_bagitem.MakeIndex`；新增列必须同样带 reason/ref/牙。
- 未做：真机复跑（本卡纯工具面；将来有「新生成物品」的窗口证据时，掩码才会真正生效）。

## 9. Post-merge 修复：合成夹具不真实（master 红 `card_window --selftest` 412/435）

**症状**：master `60051a068` 上 `card_window --selftest` **412/435**，23 条「期望 PASS」的用例变成 FAIL。
失败记录统一是 `{"check":"nonzero_both_sides","column":"MakeIndex","table":"mir2_db.characters_bagitem","occupied_a":0,"occupied_b":0}`。

**根因**（不是判据过严）：`card_window` 自测桩写出的合成 DB 快照是一行 `MakeIndex = 0` 的**空壳行**，
而 `nonzero_both_sides` 的语义就是「**声明了掩码，就要证明这一列真的被填过**」。真机上此列**永远有占用槽**
（`run2` 六场景各 **6938**、`e23` **6939**），0/0 只可能出现在合成夹具里 ⇒ **撞到它说明夹具不真实**。

**修法**：只改**夹具**，`masked_column_checks` 的语义**一行不动**（没有放宽 `require`、没有加「仅真实 dbsnap 生效」
的环境开关——那会让自测与真实路径行为分叉）：

- `tools/conformance/card_window.py` 的自测桩：`characters_bagitem` 那一行改成 **`MakeIndex = 5001`**（非零占用槽），行数仍为 1；
- **牙**（`dbsnap_pair` selftest 新增 `masked_distinct_per_player_constant`）：合成夹具填**非零占用槽但全填同一个常量**
  （`5001/5001`）⇒ `distinct_per_player` **必须红**（`rc=1`、2 条 violation）。这条用来区分「把夹具改真实」与
  「顺手把牙绕过去」——后者在这颗牙下会立刻暴露。
- **可诊断性**：`nonzero_both_sides` 的 violation 文案拆成两种——`na == 0 and nb == 0` ⇒
  「**整列无占用** ⇒ 掩码无意义/夹具不真实」；否则 ⇒ 「**一侧没写或两侧不等**（真放水）」。

**运维后果（必须记住）**：这条牙是**全局登记**（`rules.json` 的 `db.masked_columns`），所以
**任何「该列整列为空」的场景都会 fail-closed**。当前真实数据不会触发；将来若出现**合法**的
「角色无背包物品」场景，**正确处置是显式登记 per-scenario 豁免（带 reason/ref）**，
而不是削弱这条牙——削弱等于允许「一个从来没被写过的列被永久掩掉」。

**回归证据**：`card_window --selftest` **435/435 GREEN**；`dbsnap_pair selftest` **36/36 GREEN**。
