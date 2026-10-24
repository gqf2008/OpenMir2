# R 卡登记审计：今日 6 张 R 合并卡的 `window_required` / W 项 / goal_gate 分桶独立复核（2026-10-25）

- 卡：`openmir2-rcard-registration-audit`（tier **L**，owner `openmir2-reviewer-20`，branch `review/rcard-audit`）。
- 基线：本地 `master = 1c605ca4a`（walgit 故障期，本地主源）。
- 写集：**只新增本文档**；未改代码、未改任何卡片状态或他人条目；未起栈/占端口/连库。
- 口径：**本审计只核「登记是否真实、是否可判」，不代表任何一张卡已有真机窗口证据**（登记型 PASS ≠ 真机证据）。
  作者即 reviewer-20 ⇒ 复核由**另一 principal**（reviewer-21）出；本文不对任何卡作审查结论。

## 0. 一句话

6 张卡在 `goal_gate` 里**全部落在 `window_required` 桶**（`state=pass`），**0 张桶对不上**；
但「登记可判」≠「窗口当场可执行」：6 张卡的 W 项**都有具体判据文本**（无空登记），
其中 **6 张卡都至少有 1 条 W 项在当前 kit 下不能当场执行**（缺场景/夹具，或依赖未落地能力；见 §3 弱登记清单），
另有 **1 条登记卫生缺陷**（T-127 的 `merge_result.oid` 不是 commit，见 F1）。

## 1. 方法（只读、可复算）

1. 逐卡 `walgit collab thread <id>` 取**最后一条 `status`**、**最后一条 `merge_result`**、以及所有带
   `tier`/`window_required` 的 status（§2 的「字段出处」列）。
2. `python -I tools/goal/goal_gate.py --skip-tests --skip-script --json` 取条件② 分桶（§4）。
3. 逐卡桶归属**直接调用 goal_gate 自己的函数**（`load_entry_index` → `collect_r_evidence` → `r_category`），
   避免手抄桶名（§6 给出可执行片段）。
4. W 项可执行性对到 kit 实件：`tools/window/run3-scenarios.json`、`tools/conformance/card_window.py`、
   `tools/probe/mir2_probe.py`、`tools/gw-capture/gate_shadow_proxy.py`、部署件 `E:/MirServer/M2GameSvr/Envir`；
   与 `docs/window-kit-coverage-20261025.md`（tester-1，已随 `1c605ca4a` 合并）**独立对照**，其中 5 条关键事实我另行亲验（§3 脚注）。

## 2. 六张卡总表

| # | 卡 id | merge oid | 最后 status（oid / 状态） | `tier` / `window_required` 的**实际出处** | W 项（条数 / 出处） | goal_gate 桶 | 判定 |
|---:|---|---|---|---|---|---|---|
| 1 | `openmir2-castle-merchant-tax-wiring` | `99156c2c6` | `1a837e79` / in-progress（合并后） | merge_result `acf9b8ed`：`tier=R` + `window_required=true` + `window_gap`；needs-review status `7e200fff` 同样带 | 4（merge_result `window_gap`；报告 `docs/castle-merchant-tax-wiring-20261025.md` §6 模板） | `window_required` | **PASS（登记真）** |
| 2 | `openmir2-loader-fields` | `9a7b8c798` | `02dce445` / in-progress | merge_result `05b17a75`：`tier=R` + `window_required=true` + `window_gap`；needs-review `a939d623` 同样带 | 2（`window_gap` 的 W-LF-1/2；报告 `docs/loader-fields-wiring-20261025.md`） | `window_required` | **PASS**（W-LF-2 属弱登记，见 F2） |
| 3 | `openmir2-script-playdice-openybdeal` | `bff927e11` | `b9c4d872` / in-progress | merge_result `b515023e`：`tier=R` + `window_required=true` + `window_gap`；needs-review `c9a30a90` 同样带 | 2+1（GAMEPOINT 链、帧序 643×2 先于 5008×2、⑩-full 残余另卡；报告 `docs/playdice-gamapoint-residual-20261025.md`） | `window_required` | **PASS**（W-PD-2 `@back` 字面不可执行，见 F2） |
| 4 | `openmir2-clearmapmon-impl` | `f08496192` | `3fcaaaa0` / in-progress | merge_result `44c8806f`：`tier=R` + `window_required=true` + `window_gap`；needs-review `ea00d7c7` 带 `patch:4ed9fdf72` | 4（T118 / Q001 / RNG 流水观察项 / 失败门·切图序·Invisible；报告 `docs/clearmapmon-impl-20261025.md` §4） | `window_required` | **PASS**（W-CM-3 为观察项；范围需按 review `2d6db6ed` F1 限定） |
| 5 | `openmir2-m3-castle-siege-state`（T-127 残余片） | `263e77d7e` | `cbb94fb1` / in-progress | merge_result `c8a74b72`：`tier=R` + `window_required=true`；**但 `oid: "T127_MERGE"`、`merge_commit: "见 master tip"`**（F1）；needs-review `93ac6550` 带 | 5（W-T127-1..5；报告 `docs/castle-siege-t127-frames-20261025.md` §7） | `window_required` | **PASS（登记真；merge oid 不可机判 ⇒ F1）** |
| 6 | `openmir2-inbound-dispatch-red-batch`（R 片） | `a3bf5340f` | `9f3754f1` / in-progress（**唯一一条同带 `tier=R`+`window_required`+`window_items` 的 status**） | merge_result `241dbd81`：`tier=R` + `window_required=true`（该条用 `branch/target/method` 另一套字段） | 2（`window_items` 明列 W-DG-1/2；报告 `docs/dropgold-butcheffect-rng-20261025.md`） | `window_required` | **PASS（登记最完整）** |

**按片读卡（防误判）**：第 5 行是 `openmir2-m3-castle-siege-state` 的 **T-127 残余片**（该卡另有 2026-10-13 的
`⑧-6` 片、tier=L 已合 `79b5bf41`）；第 6 行是 `openmir2-inbound-dispatch-red-batch` 的 **R 片**
（该卡此前已合 L 片 `c703f6a03`，两条 tier=L 的 needs-review 状态 `83a81b08`/`b1edb045` 属那一批）。
**「设计片已合、实现片待合/在飞」的卡（如 `openmir2-castle-official-design`）不入本表**，也不要整卡写单一状态。

## 3. 弱登记清单（W 项存在，但当前窗口不可当场执行）

> 「弱登记」判定 = W 项**有文本**但：(a) kit 无场景/夹具且无可用手工步骤，或 (b) 依赖未落地能力
> （C# 时钟注入、RNG 账本接线、资产白名单），或 (c) 字面在部署件不存在。**这不是「缺登记」**——
> 6 张卡的 `window_required` 都真；下面只影响「什么时候能跑」。

| 卡 | W 项 | 判据文本 | 当前可执行性 | 依据 |
|---|---|---|---|---|
| 1 商税 | W-TAX-1..3 | 四路径双栈金币帧 / `SabukW.txt` 三字段 diff=0 / `CastleGetAllNpcTax=1` 多城升序 / 前置两态 | **缺（手工可跑）**：无城堡商人场景（`merchant_dialog` 点的是 0103 铁器店），需窗口专用配置副本 + 客户端四操作 | kit 文档 §1.1；`E:/MirServer` 无城堡商人场景 |
| 1 商税 | W-TAX-4 | 日上限 + `SaveTick` 10min 门逐位 | **缺（被 C# 时钟注入阻塞）**：`grep -rIl MIR2_CLOCK src/` = **0** ⇒ 只能观察「同窗口不刷新」 | 我亲验：`src/` 0 命中 |
| 2 loader | W-LF-1 | 出生点 11 键/落点/朝向首包 | **部分/缺**：`login_only` 用既有角色；probe 无 `CM_NEWCHR`（我亲验 `mir2_probe.py` 0 命中）⇒ 手工建号 | 我亲验 |
| 2 loader | W-LF-2 | 5 个新列的消费（`MakeSafePkZone`/`InSafeZone`） | **构造级（非窗口）**：本片无消费点 ⇒ 建议从 `window_gap` 移出，挂后继接线段 | 卡面自述 + kit §1.2 |
| 3 playdice | W-PD-1 | GAMEPOINT 落库 + `5008` 字节 + 重连读值 | **缺**：无 `真爱-G002` 场景；`ASSETS_COLUMNS = ("Gold",)`（我亲验 `card_window.py:2980`）⇒ 前置 `GamePoint≥9` 无声明通道 | 我亲验 + kit §1.3 |
| 3 playdice | W-PD-2 | `@back` 排水帧序 | **字面不可执行**：部署件 `grep -rIl '@back>' Envir` = **0**；只有 `@back2>` **2 处**（`真爱-G002`/`真爱666-G002`，我亲验） ⇒ 须澄清是 `@back2` 还是构造级（与台账 errata `de37987d` 一致） | 我亲验 |
| 4 CLEARMAPMON | W-CM-1/2 | T118/Q001 帧序/实体/AOI | **部分（手工可跑）**：触发点存在（`武士-0114.txt` / `老人-1.txt`），无现成场景 | kit §1.4；我复读两处脚本 |
| 4 CLEARMAPMON | W-CM-3 | RNG 流水位置 | **缺（接线）**：私栈未接 `-RngLog`；且按 review `2d6db6ed` F1，部署「不可观测」的理由是**本宿主 `map` 不抽数（`space_move` 为已登记 no-op）**，且整段对拍会撞该缺口 ⇒ 建议窗口限定「CLEARMAPMON/MONGENEX 产出的帧与实体」 | 我的 review F1 |
| 5 T-127 | W-T127-1..5 | 开战链帧序 / 结束预告 / N1 观测者丢弃 / 多城次序 / planes 零副作用 | **缺**：`castle_zone` 夹具未建；W-T127-2 被 C# 时钟注入阻塞；W-T127-3 需第二客户端（kit 单 probe） | kit §1.5 |
| 6 inbound | W-DG-1/2 | DROPGOLD 零抽数复拍 / BUTCH 两抽流位 | **缺（手工可跑）**：无 drop 场景；RNG 账本两侧可采但私栈启动未接 `-RngLog` | kit §1.6 |

**与 tester kit 文档的关系**：`docs/window-kit-coverage-20261025.md`（已合 `1c605ca4a`）给出同一批 W 项的
「现成/部分/缺」划分；我逐条对照后**结论一致**，并另行亲验了其中 5 条事实（`@back>`=0/`@back2>`=2、
`ASSETS_COLUMNS=("Gold",)`、C# `MIR2_CLOCK`=0、probe 无 `newchr`、`run3-scenarios.json` 共 17 场景且
`king_castle_war` 条目无 castlewar 步骤）。本文**不重复**该文档的场景提案，只做登记层判定。

## 4. goal_gate 分桶对账（`--skip-tests --skip-script`，快照 `1c605ca4a`）

```
R 卡 213 张（快照 @ 1c605ca4a）：closed 12 / window_required 145（另未合并 3）/ whitelist 48 / adjudicated 3 /
merged_not_closed 1 / 其它需人工 0 / 未开工或未合并 1（不计缺口）
登记型 PASS ≠ 已有真机证据：merged 未 closed 但已凭登记 PASS = 194 张（卫生清单，快照 @ 1c605ca4a）
```

- **6/6 落 `window_required`，0 张对不上**（逐卡判定见 §2；机器判定片段见 §6）。
- 其它桶具名（便于对账）：
  - `merged_not_closed` 1 = `openmir2-gm-batch-5b-cross-server`（board=受阻，G11 跨服依赖；已有 owner 决策页）。
  - `window_required_unmerged` 3 = `openmir2-gm-batch-5-guild-castle` / `openmir2-script-handler-msgqueue`（在飞）/ `openmir2-window-gate-two-runs`（受阻）。
  - `not_merged` 1 = `openmir2-castle-official-actions`（in-progress，未开工/未合并 ⇒ 不计缺口）。
- 卡面 notes 写 `window_required 145（另未合并 2）` 且无「未开工或未合并」行；本次输出为 `145（另未合并 3）`
  + `not_merged 1`。两时点 R 总数同为 213 但分项不同（有卡在桶间移动/新卡插入）⇒ **本文以本次同一时刻的输出为准**，
  不对旧快照作归因；这不影响 6 张卡的登记真实性。

## 5. Findings

| # | 级别 | 内容 | 建议 |
|---|---|---|---|
| **F1** | **Important（登记卫生）** | `openmir2-m3-castle-siege-state` 的 T-127 片 `merge_result`（`c8a74b72`）写成 `oid: "T127_MERGE"`、`merge_commit: "见 master tip"` ⇒ **机器不可判**：审计/台账/复算都无法从条目直接对到 `263e77d7e`（本次靠 `git log --first-parent --merges` 反查）。其余 5 张卡的 `merge_result.oid` 都是真 commit。 | 由卡片 owner 补一条 `comment`（不改 status）写明真实 merge oid `263e77d7e` 与 branch `card/castle-siege-t127`；以后 merge_result 的 `oid` 只写 commit。 |
| **F2** | Minor（弱登记） | §3 的 10 条 W 项：**6 张卡**都存在「有文本、当前不可当场执行」的情形（缺场景/夹具、C# 时钟注入未落地、RNG 账本未接线、资产白名单只有 `Gold`）。**其中两处性质更硬**：① `loader W-LF-2` 本片无消费点 ⇒ 应移出窗口项；② `playdice W-PD-2` 的 `@back` 字面在部署件 **0 命中**（只有 `@back2`）⇒ 要么改指 `@back2`，要么标构造级。 | 逐条按 §3「依据」列收口：能手工跑的在窗口 kit 里补手工步骤（tester 已做）；不能跑的改登记（移出/挂后继片/标观察项）。 |
| **F3** | Minor（口径） | 5/6 卡的**最后 status** 是合并后的 `in-progress`，**不带** `tier`/`window_required`；字段实际在 `merge_result` 与更早的 needs-review status 上。`goal_gate` 采「**任一**条目的 `window_required=true`」⇒ 桶正确；但若有人按「最后 status」核对会误判缺登记。 | 审计/复核口径统一写成「以 merge_result（或任一 needs-review status）为准」；不建议为此回填旧 status。 |
| **F4** | 红线（自检） | 本文全部表述遵守：不把「登记型 PASS」写成「已有真机证据」（§0/§4 明写 194 张卫生清单只是登记型 PASS）；本文不对任何卡作审查结论（复核由 reviewer-21 出）。 | — |

## 6. 复算命令（可直接执行）

```sh
cd <本检出>

# ① 六张卡的最后 status / merge_result / tier 出处（逐卡打印）
for id in openmir2-castle-merchant-tax-wiring openmir2-loader-fields \
          openmir2-script-playdice-openybdeal openmir2-clearmapmon-impl \
          openmir2-m3-castle-siege-state openmir2-inbound-dispatch-red-batch; do
  echo "### $id"
  walgit collab thread "$id" | python -I -c '
import json,sys,datetime
d=json.load(sys.stdin)
st=[e for e in d if e["entry"]["kind"]=="status"]
mr=[e for e in d if e["entry"]["kind"]=="merge_result"]
if st:
    e=st[-1]; b=e["entry"]["body"]
    print(" LAST_STATUS",e["oid"],b.get("status"),"tier=",b.get("tier"),"wr=",b.get("window_required"))
if mr:
    e=mr[-1]; b=e["entry"]["body"]
    print(" LAST_MERGE ",e["oid"],"oid=",b.get("oid"),"commit=",b.get("merge_commit"),"tier=",b.get("tier"),"wr=",b.get("window_required"))
'
done

# ② 条件② 分桶（快速模式，只读）
python -I tools/goal/goal_gate.py --skip-tests --skip-script --json > "$TEMP/rcard-gate.json"
python -I -c 'import json,os;d=json.load(open(os.path.join(os.environ["TEMP"],"rcard-gate.json")));
[print(c["id"],c["state"],c["detail"][:200]) for c in d["conditions"] if c["id"]=="2"]'

# ③ 六张卡的桶归属（直接调用 goal_gate 的分类函数，不手抄桶名）
python -I - <<'PY'
import importlib.util, pathlib
spec = importlib.util.spec_from_file_location("gg", "tools/goal/goal_gate.py")
gg = importlib.util.module_from_spec(spec); spec.loader.exec_module(gg)
repo = pathlib.Path(".").resolve()
board, _ = gg.load_board(repo); index, _ = gg.load_entry_index(repo)
wl = gg.build_whitelist(repo/"mir2-rs/tests/parity/whitelist.md", exclude_ids=gg.load_principals(repo))
for c in ["openmir2-castle-merchant-tax-wiring","openmir2-loader-fields",
          "openmir2-script-playdice-openybdeal","openmir2-clearmapmon-impl",
          "openmir2-m3-castle-siege-state","openmir2-inbound-dispatch-red-batch"]:
    ev = gg.collect_r_evidence(c, index, board, wl)
    print(c, gg.r_category(ev), gg.classify_r_evidence(ev)[0])
PY

# ④ 弱登记的几个事实（只读）
grep -rIl '@back>' E:/MirServer/M2GameSvr/Envir | wc -l      # 期望 0
grep -ro '@back2>' E:/MirServer/M2GameSvr/Envir | wc -l      # 期望 2
grep -n 'ASSETS_COLUMNS' tools/conformance/card_window.py | head -1   # ("Gold",)
grep -rIl 'MIR2_CLOCK' src/ | wc -l                          # 期望 0
grep -c 'CM_NEWCHR\|newchr' tools/probe/mir2_probe.py        # 期望 0
```

## 7. 门禁 / 残留

- 本卡纯文档：`git diff --check <base> <head>` rc=0；7 项卫生门禁（`stale_pointer` 用 `--ref master`）+ helper selftest 见 patch body；未跑 cargo（无代码改动）。
- 残留：无临时物；`E:/MirServer/**` 只读（`Castle/List.txt` 由运行中的 C# GameSrv 每 10 分钟重写、内容恒 `0\r\n1\r\n`，与本卡无关）。
- 本文**不移动任何卡片、不改任何他人条目**；F1 的补登由卡片 owner 执行。
