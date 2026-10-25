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
                  "require": ["nonzero_both_sides", "distinct_per_player", "changes_on_mutation"]}
  }
}
```

- 加载期 fail-closed：`reason`/`ref` 必须有；`require` 必须非空且只含三种已登记检查（**掩掉一列必须带牙**）。
- 掩码列从值比较里排除，但**同时**跑结构牙（`dbsnap_pair.masked_column_checks`）；
  每条掩码发一条 `masked_column_applied` 证据，由 `compare_runs` 写进 `report["whitelisted"]`
  （`kind=db_column_masked`，带 `reason`/`ref`/`checks`/两侧占用行数）——**不许静默丢弃**。

## 3. 结构牙的语义（**按真实数据校正过**，与卡面字面不同，必须留痕）

复算 `E:/tmp/window/db-rerun/run2` 得到的事实：

1. `characters_bagitem` 把**空槽也物化成行**且 `MakeIndex=0`（实测 330,702 / 337,640 行为 0）
   ⇒ 「每一行都非 0」**在数据上不可能成立**，不是判据。改为按**占用的槽**（`MakeIndex != 0`）定义：
   - `nonzero_both_sides`：**两侧占用槽数都 > 0 且相等**（防「一侧根本没写」）；
2. 同一 `PlayerId` 内**合法重复**确实存在（run2 `login_only` 的 PlayerId=6227：28 个占用槽、25 个不同值）
   ⇒ 「各槽互不相同」同样不成立。改为 `distinct_per_player`：**占用槽 ≥2 的玩家，不同取值数必须 ≥2**
   （防「全填同一个常量」）；
3. `changes_on_mutation`：给了 before 快照（`--baseline-*`）时，占用集合或取值必须变化（防写死常量）；
   没有 before 快照时记 `skipped` 并在报告里写明。

任一不成立 ⇒ `masked_column_violation`（**差异**，不是「判不了」）。

## 4. 牙与变异

`compare_runs selftest` **340/340**（新增五颗）：

| 牙 | 断言 |
|---|---|
| MI-1 | 两侧仅 `MakeIndex` 不同：**无掩码 ⇒ rc 1（有 db 差异）**；**加掩码 ⇒ rc 0 且 `whitelisted` 有一条 `db_column_masked`、`ref` 含 `D-1`** |
| MI-2 | 一侧占用槽全 0 ⇒ `nonzero_both_sides` 判红 |
| MI-3 | 同一玩家两槽同值 ⇒ `distinct_per_player` 判红 |
| MI-4 | before/after 有变化 ⇒ 通过；完全相同 ⇒ `changes_on_mutation` 判红 |
| MI-5 | **用仓库 `rules.json` 本身**（不传 `--rules`）跑同一构造：rc 0 且 `whitelisted` 点名 `D-1`、`skipped` 含 `changes_on_mutation` |

> **为什么有 MI-5**：MI-1 用的是自造规则文件，它证明「机制生效」，但**删掉 `rules.json` 里那条登记后
> MI-1..4 仍全绿**（登记被删没人发现 = 没牙）。MI-5 把牙咬在**仓库登记表**上。

### 4.1 变异实证（逐条放宽 ⇒ 对应牙红；输出为当次实跑）

| 变异 | 结果 |
|---|---|
| A `nonzero_both_sides` 放宽为 `if False` | `FAIL 牙MI-2`，`RED: 338/339` |
| B `distinct_per_player` 放宽为 `if False` | `FAIL 牙MI-3`，`RED: 338/339` |
| C `changes_on_mutation` 放宽为 `if False` | `FAIL 牙MI-4`，`RED: 338/339` |
| D **去掉 `rules.json` 的 `db.masked_columns`**（反向） | `FAIL 牙MI-5`，`rc=1`，差异记录 `{"kind":"value","record":{"column":"MakeIndex","a":"101","b":"201"}}` ⇒ 正是「每次都报一条 MakeIndex 差异」的旧行为；`RED: 339/340` |

回归后 `GREEN: compare_runs selftest 340/340`。

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

**如实说明**：run2 这批里 `MakeIndex` 差异**改前就是 0**（两侧 DB 都是同一次 restore 的基线，
物品的 MakeIndex 未被重新生成）⇒ 本轮掩码在这批证据上是**无操作**；「掩码真生效」由 **牙 MI-1**
（构造级：仅 MakeIndex 不同 ⇒ 无掩码 rc1、有掩码 rc0）实证。其余差异条数**改前=改后**（0/0/0/0/5/0 不变）。

## 6. 红线与未做

- **不掩帧面**：只动 DB 列；`SM_ADDITEM(200)`/`SM_BAGITEMS(201)` 的 `MakeIndex` 判据（规则表 `bagitems_records`）未改。
- **不顺手掩其它列**：`masked_columns` 只登记了 `characters_bagitem.MakeIndex`；新增列必须同样带 reason/ref/牙。
- 未做：真机复跑（本卡纯工具面；将来有「新生成物品」的窗口证据时，掩码才会真正生效）。
