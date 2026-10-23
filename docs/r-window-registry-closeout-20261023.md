# `merged 未 closed 且无登记` 收口（`openmir2-r-window-registry-closeout`）

- 卡：`openmir2-r-window-registry-closeout`（tier=L）；分支 `card/openmir2-r-window-registry-closeout`
- 写集：`mir2-rs/tests/parity/whitelist.md`、`T-RESERVATIONS.md`、`docs/`（零代码）
- 快照：base = master `ed4dd05c6`；`goal_gate --json` 前后快照见 §2。

## 1. 名单（现取）

```sh
python -I tools/goal/goal_gate.py --json
```

条件② 里分类为 `merged_not_closed`（「merge_result merged=true 但 board 状态=in-progress 且无显式登记」）的 8 张：

`openmir2-enter-map-cell-rejected-vs-oracle`、`openmir2-run-gap-checkdupobj`、
`openmir2-disappear-recipients-gridscan`、`openmir2-gate-gmclose-race`、
`openmir2-sm-updateitem`、`openmir2-login-useitems-magic`、`openmir2-sm-merchantsay-recog`、`openmir2-smframe-minor-gaps`

> **head `5c28e6cef` 现取（2026-10-24，review `36efe6bc` B-1）**：`merged_not_closed` = **5** ——
> 上面 8 张里归位 4 张（T-264 覆盖），剩余 4 张（checkdupobj / disappear-recipients / enter-map-cell / gate-gmclose-race）
> 再加 **`openmir2-gm-batch-5b-cross-server`**（merge `6af73da3`、board=`blocked`；其 merge ts 晚于本卡 base，
> 因此不在 §1 的 8 张名单里）。该卡已在本轮补签名 comment `883cecb4`（桶②，见 §3）。

（另有 5 张「board 未列出该卡」的 `not_in_board`：`m3-monster-ai-tick`、`client-e2e-enter-gaps`、
`conformance-rng-nondeterminism`、`menu-ok-unify-pickwire`、`ybdeal-frames`——属于协作视图缺失，不在本卡 8 张范围。）

## 2. 前后对比（条件② detail 原文）

**改前**：

```
… whitelist 44 / 已裁定登记 3 / 三者皆无 1 / merged 未 closed 且无登记(需人工) 8 / 其它需人工 5 …
FAIL 卡：openmir2-gm-batch-5-guild-castle；需人工 13 张 …
```

**改后**（本卡新增 T-264 后）：

```
… whitelist 48 / 已裁定登记 3 / 三者皆无 1 / merged 未 closed 且无登记(需人工) 4 / 其它需人工 5 …
FAIL 卡：openmir2-gm-batch-5-guild-castle；需人工 9 张 …
```

⇒ 「merged 未 closed」8 → **4**（base `ed4dd05c6` 时点）。剩余 4 张每张都有本卡的**签名 comment**（见 §3），符合卡面判据的「或」支。

**本卡 head `5c28e6cef` 现取（2026-10-24，`goal_gate --json` rc=2）**：

```
… R 卡 193 张：已完成 11 / window_required 121 / whitelist 48 / 已裁定登记 3 / comment 证据 0 / 三者皆无 0 /
merged 未 closed 且无登记(需人工) 5 / 未开工或未合并 0 / 其它需人工 5；需人工 10 张 …
```

⇒ **计数随 board 动态**：合入时点以 `goal_gate --json` **现取**为准。base 快照 8→4、head 是 **5**
（多出的第 5 张是 `openmir2-gm-batch-5b-cross-server`，本轮已补 comment，不是回退）。

## 3. 逐张定性（8 张，全部留签名 comment）

| 卡 | 桶 | 处置与依据 |
|---|---|---|
| `sm-updateitem` | ① 可登记 | **T-264**（whitelist 行点名本卡）：203/621/211 的登录序与 T-80 模型 D′ 逐字节；comment `abc4708d` |
| `login-useitems-magic` | ① 可登记 | **T-264**：登录序 18 帧逐字节；comment `5a6c9cb2` |
| `sm-merchantsay-recog` | ① 可登记 | **T-264**：643 非 priority 路径 `recog` = NPC ActorId；comment `b2f6ed01` |
| `smframe-minor-gaps` | ① 可登记 | **T-264**：T-79 残余不可达复核 + 9 条低影响项；comment `19ae4ac6` |
| `run-gap-checkdupobj` | ② 需 owner | 需 ≥3 会话同格（单 probe 不可构造）；等多会话探针 slice-2（T-221）；构造级替代已在卡面；comment `40514dd6` |
| `disappear-recipients-gridscan` | ② 需 owner | 需第二观察者进 ±12；同上；comment `cff88bf1` |
| `enter-map-cell-rejected-vs-oracle` | ③ 建议关闭 | status 自述「生产无 `switch_map` 调用者 ⇒ 真机不可达」、无可跑窗口判据；建议 owner `closed`；comment `3730b07a` |
| `gate-gmclose-race` | ③ 建议关闭 | status 自述「无待窗口项」、review `window_required=false`；建议 owner `closed`；comment `6ace05bf` |
| `openmir2-gm-batch-5b-cross-server`（**head 新增，非 §1 的 8 张**） | ② 需 owner | docs 已合（merge `6af73da3`，`merged=true, blocked=true`）、实现 **blocked-by G11**；G11 前无窗口判据，构造级替代见 `docs/gm-batch-5b-g11-dependencies-20261024.md` §6；comment `883cecb4` |

> **术语订正（review `36efe6bc` minor）**：§1 的 8 张里有 4 张（`sm-updateitem` / `login-useitems-magic` / `sm-merchantsay-recog` /
> `smframe-minor-gaps`）在 patch body 里被写作「未开班」，**不准确**——它们是**已合并待窗口**的帧面批次 G-2..G-5
> （`merge_result merged=true` + `status tier=R` + 余量=真机窗口），归 ① 可登记，不是「从未开工」。

> **未做**：没有改任何他人卡片的 `status`；没有为清零给 L 卡登记窗口项；T-264 的四张都是 R 卡且有真实机窗口面。

## 4. 机制说明（为什么「剩余 4 张」仍是 need_items）

`goal_gate` 只认三种机读登记（`window_required=true` / whitelist 强关联 / approve+`window_required=false`+理由），
**不看 comment**（除非 `--allow-verdict-grep`）。所以②桶与③桶的 comment 不会让计数变化——这正是卡面
「或每张剩余都有该卡签名 comment 说明」的适用范围：**剩余 5 张**（base 的 4 张 + head 新出现的
`gm-batch-5b-cross-server`）要靠 owner 动作才能真正归零：关闭 2 张（enter-map-cell / gate-gmclose-race）、
给 2 张排多会话探针窗口（checkdupobj / disappear-recipients）、**`gm-batch-5b` 等 G11**。

## 5. 门禁

```
t_reservation_gate.py check      GREEN（rows=247 whitelist=247，含 T-264）
whitelist_anchor_gate.py check   ALL PASS（538 条锚）
m3_whitelist_registry_audit.py   GREEN
conflict_marker_gate             （由 CI/门禁侧检查；本卡只改 md 两处与 docs）
```

**head `5c28e6cef` 实测（2026-10-24）**——订正 §5 的 base 快照数字（review `36efe6bc` B-2）：

```
t_reservation_gate.py check      GREEN（rows=248 whitelist=248）
whitelist_anchor_gate.py check   ALL PASS（541 条锚）
m3_whitelist_registry_audit.py   GREEN
conflict_marker_gate             0 markers / 7415 files，rc=0
goal_gate.py --json              rc=2；merged_not_closed = 5（detail 见 §2）
```

## 6. 残留物

- `E:/tmp/w24-closeout-before.json`、`E:/tmp/w24-closeout-after.json`（goal_gate 前后快照）。
