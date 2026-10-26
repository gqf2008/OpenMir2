# 规则表 ident 缺口处置（2026-10-26）

卡 `openmir2-rules-ident-gaps`（tier L）。证据来源：run2（`E:/tmp/window-20261026/run2/`）+
reviewer-21 的调研 comment `673ea9cd`（「未登记 ≠ 未实现」）。

## 0. 一句话

窗口暴露的两类缺口：**ident 14 的 ignore 规则仍是 PENDING**（多场景因此判不了）与
**5 个未登记 ident（32/34/44/637/656）**。本卡给 4 个加 `compare` 规则（让差异可见，不是掩掉）、
1 个（656）**不加**并登记待查；ident 14 **保持 PENDING** 并写清解除条件。

## 1. 五个 ident（32 / 34 / 44 / 637 / 656）

| ident | 名称 | 帧形（oracle） | 处置 | run2 观察 |
|---|---|---|---|---|
| 32 | `SM_DEATH` | `recog=ActorId`、`param=X`、`tag=Y`、`series=Dir`、尾 CharDesc 8B；发点 = **可见尸体重显**（`PlayObject.cs:3224-3236`，`RM_DEATH` 且 `nParam3==0`，收件人侧 `PlayObject.Message.cs:1631-1645`） | **登记 `compare`**（R）。Rust 已实现（`downlink.rs:469 death_frame` + `appear_frames` 的 dead 分派，红检 `redcheck_appear_dead_uses_sm_death_not_turn`）⇒ butch 的 oracle 1/rust 0 更像「重显没发生」；**要构造级用例（死怪重新进视距）才能定性**，别拿这份录制下结论。**不许掩码** | butch oracle 1 / rust 0 |
| 34 | `SM_NOWDEATH` | 与 32 **同形**；`RM_DEATH` 且 `nParam3==1`（真死，`PlayObject.Message.cs:1631-1635`；发点 `BaseObject.Base.cs:351`） | **登记 `compare`**（R），**与 32 分开判**（同一 RM_DEATH 的两种编码）。坐标/朝向**必须真比**：butch 里 oracle `330,329,Dir7` vs rust `329,330,Dir2` 是**杀了不同的鸡**（8 格内 32 只、nearest 选靶），先改夹具（固定 1 只、固定坐标）再比，**不许掩坐标** | butch 1 / 1（坐标不同） |
| 44 | `SM_WINEXP` | `recog=**累计经验**（不是 ActorId）`、`param/tag=本次经验增量低/高 16 位`、`series=0`、无尾（`PlayObject.Message.cs:1666-1668`；发点 `PlayObject.Base.cs:153`，只发给自己） | **登记 `compare` + `head_compare:["recog"]`**（R）。全局 recog 掩码会把累计经验吞掉 ⇒ **掩掉它就是把最硬的数值判据变成假绿** | butch 两侧逐字段相同（5/5/0/0） |
| 637 | `SM_BUTCH` | `recog=ActorId`、`param=X`、`tag=Y`、`series=wParam(Dir)`、无尾、不跟 41；整段只被 `if (ActorId != 0)` 门住（`PlayObject.Message.cs:1781-1788`；发点 `PlayObject.Operate.cs:749`） | **登记 `compare`**（L）。reason 必须写清「**无条件回帧 ≠ 挖肉成功**」，成功要看 `SM_BAGITEMS(201)`/背包 | butch 两侧各 1 条、`param/tag=330,330`、`series=0` 相同 |
| 656 | `SM_CHANGENAMECOLOR` | `recog=ActorId`、`param=GetChrColor(baseObject)`、`tag/series=0`、无尾（`PlayObject.Message.cs:1677-1679`；发点 `BaseObject.cs:730 RefNameColor()`，调用点很多） | **暂不登记**（`compare` 会把「Rust 少发」长期变成红/UNDECIDABLE 噪声）。**需实现车道查**：oracle 侧 `AnimalObject.BreakHolySeizeMode`（`AnimalObject.cs:330-334`）**没有 if 守卫、恒发 `RefNameColor()`**（`BreakCrazyMode` 有守卫），而 Rust `monai.rs:4239 break_holy_seize_mode` **只清标志** ⇒ 实现链候选缺陷 | butch oracle 2（`param=255`，颜色其实没变）/ rust 0 |

## 2. ident 14：保持 PENDING，写清解除条件

- **不改 action**（仍是 `ignore`）、**不加掩码**、`approved_by` 仍 PENDING。理由写进 `rules.json`：
  - **解除条件**：当前证据只到「**跨轮不可复现**」——同一 oracle 二进制（`sha256=ab655008…`）+ 同一
    `scenario_sha256` 下 castle 14=29 vs 27、dice_chain 0/1/0、move_walk 0/0/1/0。解除理由只能写
    「不可逐帧对齐」，**绝不能写成「已比对一致」**；真要解除需 R 级窗口证据（证明「不该出现的一侧不发」）
    或换成形状级规则。
  - **整帧 ignore 的已知代价**：`RM_HIT` 还有三类**未登记**来源——动物攻击（`AnimalObject.cs:171/382`）、
    怪物特殊攻击（11 个 Monsters 文件 + `SuperGuard:33`）、**玩家攻击**（`PlayObject.Attack.cs:45/56/67/82/86`）。
    butch 那 2 条正是**鸡在啄玩家**（recog=199、同坐标 329,330）⇒ 整帧 ignore 会把战斗命中一起吞掉。
  - **长期因此判 UNDECIDABLE 的场景**（两侧 14 计数不齐）：`npc_click`、`merchant_dialog`、
    `butch_two_draws`、`castle_merchant_tax_potion`。

## 3. 10 / 50 也仍是 PENDING（判读时必须一起写）

`butch` 的 `report.json` 里 `pending_review` 原本就有 `rules[0]:ident=[10]` 与 `rules[5]:ident=[50]`：
**靠 `SM_TURN` 的 `head_mask:["series"]`（ident 10）与 ident 50 规则通过的场景，并不是「已批准规则下的绿」**。
本卡把这句写进 `rules.json` 14 的 reason 与本文；账本（见 §4）也把它显式列出来。

## 4. 自检账本更新（`selftest_impl.py` 一行，Leader 批准扩写）

`selftest_impl.py` 原断言把「当前 PENDING 的 ident」硬编码成 `{10,14,50}`——本卡新增 4 条 draft 规则
（`approved_by` 仍 PENDING）后它必然红。该断言的本意是**账本**（「现在有哪些 ident 是 PENDING」），
不是不变式；硬编码会让**任何新 draft 规则**都变红。

- 更新为 `{10,14,32,34,44,50,637}`，断言名改成
  `PENDING ledger: 当前 PENDING 的 ident = {...}（新增 draft 规则必须同步此表；账本更新 ≠ 批准）`；
- **仍是完全相等（fail-closed）**，没有改成子集包含——将来再来一条新 draft 规则时，它仍会红并逼人更新账本；
- **账本更新 ≠ 批准**：`rules_pending_review` 仍为 `true`，`pending_review_used` 仍按本次实际涉及的规则报告。

## 5. 手工重放牙（临时夹具；`selftest_impl.py` 的永久牙另开卡）

夹具：`E:/tmp/dev-2/ident-gaps-teeth/`（每侧一帧的最小 `frames.ndjson`，空尾用 `body_hex:""` 显式给，
否则会判 `body_unavailable`）。命令：
`python -I -B tools/conformance/compare_runs.py compare --oracle <on> --rust <rust> --out <rep> --quiet`。

```
=== 牙①：44 的 recog 是累计经验，必须取回逐值比 ===
A 新规则（44 带 head_compare:recog）: rc=1 verdict=FAIL diff_fields=['recog']
B 对照（44 不加 head_compare，recog 被全局掩掉）: rc=0 verdict=PASS diffs=0   <== 假绿对照

=== 牙②：637 只做帧级比较，不是「挖肉成功」判据 ===
A 637 param 330 vs 331: rc=1 verdict=FAIL diff_fields=['param']
B 两侧 637 逐字段相同: rc=0 verdict=PASS diffs=0
C rust 少一条 637: rc=1 verdict=FAIL diff_fields=['block_count']
```

结论：44 的 `head_compare:["recog"]` **是必要的**（去掉就是假绿）；637 是**帧级判据**（参数不同/缺帧都红），
规则里没有任何 success 语义——成功与否仍看 201/背包。

## 6. run2 重算（同一批产物，只换 rules）

方法：`git show master:tools/conformance/rules.json > rules-master.json`，用同一个 `compare_runs.py`
分别以 `--rules rules-master.json`（旧）与默认（新）跑 13 个两侧都有 `frames.ndjson` 的场景。
**代码零改动**（本卡只改 rules.json + 文档 + 一行账本）。

| 场景 | verdict 旧→新 | diffs | unclassified 旧→新 |
|---|---|---|---|
| butch_two_draws | UNDECIDABLE → UNDECIDABLE | 67 → 67 | **{32,34,44,637,656} → {656}** |
| bag_equip / eat_item | FAIL → FAIL | 10/10、9/9 | [] |
| castle_merchant_tax_potion / move_walk / npc_click / merchant_dialog | UNDECIDABLE → UNDECIDABLE | 1/0/11/0 不变 | [] |
| dice_chain_auto / gm_batch / gm_time / login_only / npc_click_auto / spell_cast | PASS → PASS | 0 | [] |

**verdict 变化：0 条**。唯一实质变化是 `butch_two_draws` 的 `UNCLASSIFIED` 从 5 个缩到 `{656}`
（32/34/44/637 现在有规则）；它仍 UNDECIDABLE，原因是 **656 未登记** + **ident 14 的 PENDING ignore 计数不齐**，
这两条都**没有**为了变绿去动。

## 7. 边界与残余

- 32/34 的「Rust 是否真的重显/真死」**没有**在本卡判定——要构造级用例；规则只保证「出现就要比」。
- 34 的坐标差异需要**夹具固定**（固定 1 只鸡、固定坐标）后才有判据意义；本卡**没有**掩坐标。
- 656 未登记 ⇒ `butch_two_draws` 会继续 UNDECIDABLE；待实现车道核对 `monai.rs:4239` 后另开卡。
- ident 14 与 10/50 仍 PENDING —— 判读任何「靠它们通过」的场景时必须一并写明。
