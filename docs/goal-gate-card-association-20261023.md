# `goal_gate` 卡号关联口径：整行扫描 → 结构化关联

> 卡：`openmir2-goal-gate-card-association`（tier=L）；分支 `card/goal-gate-card-association`，base `b13b29e39`。
> 写集：`tools/goal/`、`docs/`。**没有**改 `whitelist.md`、没有改任何卡片状态、**没有**放宽条件② 判据。

## 0. 结论

- **条件② 的 `detail` 改前改后逐字相同**（R 卡 192 / whitelist 17 / merged 未 closed 且无登记 35 / 需人工 40 / 卫生清单 133）；
  `merged_open_hygiene` 与 `need_items` 两个清单逐卡一致（脚本比对：`hygiene same: True`、`need_items same: True`）。
- 关联集合变化：**去掉 10 条误关联**（全部是行内「见 / 承接卡 / 归 / 已立卡」引用），**新增 0**；`card_to_t` 从 101 张降到 91 张。
- 新增两类**点名告警**（以前静默）：9 行列数≠4；108 行无法结构化关联卡号（全部 T id 打在 warning 里）。

## 1. 改前实现审计（先读代码）

`CARD_ID_RE`（改前 `tools/goal/goal_gate.py:62`：`r"openmir2-[A-Za-z0-9][A-Za-z0-9._-]*"`）**只有一个使用点**：
`build_whitelist()`（改前 `:361`）里的

```python
cards = {c for c in CARD_ID_RE.findall(line) if c not in exclude_ids}
```

⇒ **整行**（四个单元格全部）扫描。逐条回答卡面的两个问题：

| 问题 | 实测结论 |
|---|---|
| 「一行登记两张卡」会不会漏第二张？ | **不会**。`findall` 取整行全部命中再进 `set`。用合成行实测：`\| T-900 \| 改动 A（卡 \`card-a\`），同批还有卡 \`card-b\` \| … \|` ⇒ `cards={'card-a','card-b'}`。**卡面 evidence 里「会漏关联第二张」这一条与代码不符**（据此写的临时对策「一行一卡」并非必要；本卡仍保留「一行多卡全部计入」的正确行为）。 |
| 「行内引用别的卡」会不会误关联？ | **会**，这才是真缺陷。审查列里的「见 C6 原卡 `X`」「承接卡 `X`」「归 `X`」全部被算成该行卡。当前真实 whitelist 上有 **10 条**这类误关联（§4 逐条列上下文）。 |
| 无法结构化的行 | 静默：`cards=set()`，既不告警也不点名，读者看不出「这一行没关联到任何卡」。 |

另有一条噪声源：`line` 里大量出现 `` `a|b` `` 代码段与转义竖线，直接 `split('|')` 会把一行切成 8~13 列 —— 做「列数」判定前必须先做**转义 + 代码段感知**的切分（本卡已实现 `split_table_cells()`）。

## 2. 新口径（结构化关联，四条规则）

`build_whitelist()` 现在按行切列后取「归属卡」（`_owner_cards()`）：

1. **「卡 `openmir2-…`」写法**（`CARD_OWNER_RE`）——任何列里 `卡`/`卡片` + 可选冒号/反引号 + id；
2. **「作者登记（… `id` …）」写法**（`CARD_REGISTER_GROUP_RE`）——老行不写「卡」字，只在审查列的 `作者登记（principal，card，tier=…）` 里列 id；
3. **第二列（改动说明列）里的 id**——很多行直接写 id；
4. 以上任一方式命中 ⇒ 该行归属卡 = **全部命中**（一行多卡全计入）。

反向规则（防误关联）：命中点前 `REF_CUE_WINDOW=6` 个字符内出现
`见 / 原 / 已由 / 承接 / 立卡 / 引用 / 占用 / 详见 / 另见` ⇒ 视为**引用**，不关联。

- 归属卡之外的行内 id 一律进 `row["refs"]`（供人核，**不**进 `card_to_t`）。
- **fail-loud**：无法结构化关联的行进 `info["unassociated"]`，列数≠4 的行进 `info["bad_columns"]`；
  `collect_report()` 把两类都汇总成**点名 T id** 的 `warnings`（以前是 0 条，现在是 2 条汇总告警，逐行细节仍在 `info["warnings"]` 里）。
- 收紧 id 正则：`openmir2-[A-Za-z0-9](?:[A-Za-z0-9._-]*[A-Za-z0-9])?` —— 不再把行内截断的 `openmir2-gm-impl-` / `openmir2-m3-mail-` 当成卡号。

## 3. 三颗牙（改实现 ⇒ selftest 变红；逐字节还原）

夹具在 `SYNTH_WHITELIST`：`T-10` 一行两卡；`T-11` 归属卡 `kappa` + 引用 `zeta`/`eta`；`T-12` 无卡且 3 列；`T-13` 5 列但有归属卡 `iota`。
selftest 断言：`牙① 一行两卡 ⇒ 两张都计入`、`牙② 行内引用别的卡 ⇒ 不关联`、`牙③ 无卡号/格式异常 ⇒ 进 unassociated/bad_columns 并点名`。

| 牙 | 变异（就地改实现） | selftest 结果 | 还原 |
|---|---|---|---|
| ① 多卡全计入 | `_owner_cards` 末尾 `return set(sorted(owners)[:1])`（只留第一张） | **67/68，FAIL 牙①**（`alpha` 只剩 T-1/T-10，`delta` 丢失） | `cp` 还原 |
| ② 引用不关联 | `_owner_cards` 末尾追加「整行 findall 全部塞进 owners」（退回旧口径） | **65/68**：FAIL 牙②（`refs[T-11]` 变空）+ 另两条既有断言连带红（证明旧口径会把引用卡串进来） | `cp` 还原 |
| ③ 异常行点名 | 删掉 `unassociated` / `bad_columns` 两处 append | **67/68，FAIL 牙③**（两类清单为空） | `cp` 还原 |

- 还原后 `sha256sum tools/goal/goal_gate.py` = **`bc1f57c313edb38b6b4f438d42eed4c41e150e6fec737231c86fdd2395489aac`**（与变异前逐字节一致），selftest 回到 **68/68 通过**。

## 4. 回归：真实 whitelist 上的改前/改后

跑的时刻：**2026-10-23 晚，master `b13b29e39`**（`goal_gate --json`，快速模式；board/index 为本地 `refs/collab` 快照）。

| 项 | 改前（整行扫描） | 改后（结构化） |
|---|---|---|
| 条件① state | need_human（快速模式未跑 parse-stats） | need_human（同上，未变） |
| **条件② state** | **fail** | **fail**（未变） |
| 条件② detail | R 卡 192：已完成 10 / window_required 121 / **whitelist 17** / 已裁定登记 3 / comment 0 / 三者皆无 1 / **merged 未 closed 且无登记 35** / 未开工 0 / 其它需人工 5；**需人工 40**；卫生清单 **133** | **逐字相同** |
| 条件③ state | pass | pass |
| 条件④ state | need_human（快速模式未跑 cargo test） | need_human（同上，未变） |
| `warnings` | 0 条 | 2 条汇总：列数≠4 **9 行**（T-23/69/71/157/158/159/172/201/206）+ 无法关联 **108 行**（T id 全列表） |
| `card_to_t` 卡数 | 101 | 91（−10 误关联，+0） |

**为什么 detail 一字不差**：去掉的 10 条关联都落在「本来就不靠这些行拿 whitelist 分类」的卡上（要么这些卡另有真归属行，要么它们本就不是 R 或已被其它条件归类）。逐卡比对 `merged_open_hygiene` 与 `need_items` 完全一致 ⇒ **判据没有被放宽，也没有被悄悄收紧**。

**被去掉的 10 条误关联（逐条上下文，全部是引用）**：

| 卡 | 行 | 原文片段 |
|---|---|---|
| `openmir2-c6-e2e-loadgate` | T-6 | 「见 C6 原卡 `openmir2-c6-e2e-loadgate`」 |
| `openmir2-discovery-bagitems-201` | T-211 | 「状态=阻塞：等 `openmir2-discovery-bagitems-201`」 |
| `openmir2-m3-exp-misc` | T-32 | 「收敛随 `openmir2-m3-exp-misc`（组队）」 |
| `openmir2-m3-gamesvr-save` | T-27 | 「承接卡 `openmir2-m3-gamesvr-save` 已落地」 |
| `openmir2-m3-quest-state-dump` | T-58 | 「归承接卡 `openmir2-m3-quest-state-dump`」 |
| `openmir2-m3-rust-stack-seed-plumb` | T-30 | 「种子透传管线归 `openmir2-m3-rust-stack-seed-plumb`」 |
| `openmir2-m3-storage-persist` | T-47 | 「与 ⑤-5 `openmir2-m3-storage-persist` 相接」 |
| `openmir2-scenario-spell-cast` | T-228 | 「承接卡 `openmir2-scenario-spell-cast`」 |
| `openmir2-script-sendmsg-host` | T-208 | 「承接卡 `openmir2-script-sendmsg-host` 落地后转 R」 |
| `openmir2-window-character-placement` | T-211 | 「该能力已立卡 **`openmir2-window-character-placement`**」 |

## 5. 告警口径（新增，fail-loud）

```
whitelist.md 有 9 行列数≠4（格式异常，按已解析列处理）：T-23, T-69, T-71, T-157, T-158, T-159, T-172, T-201, T-206
whitelist.md 有 108 行无法结构化关联卡号（不计入任何卡，需人工补『卡 `openmir2-…`』）：T-1, T-2, …
```

- 这两条**不改变任何 state**（warnings 只做可视化）；它们的作用是：下一批登记时，凡是没有按结构化写法标卡号的行都会**显式**出现，而不是静默变成「没有关联」。
- 108 行主要是 M0–M3 时代的老登记（当时没有卡号字段）；补卡号属于登记批次（`openmir2-r-window-registry-batch`）的活，不在本卡写集。
- 9 行列数≠4 是**单元格里未转义的竖线**（markdown 表格里本应写 `\|` 或用代码段外转义）；不影响关联（第二列仍可解析），但会让「列结构」判定失真 —— 已点名，留给后续清理。

## 6. 门禁

- `python -I -B tools/goal/goal_gate.py selftest` ⇒ **68 条断言 / 68 通过**（含本卡新增 4 条）。
- `python -I -B tools/goal/goal_gate.py --json` ⇒ rc=1（条件② fail，与改前一致）。
- `python -I tools/hygiene/conflict_marker_gate.py` ⇒ 0 markers。
