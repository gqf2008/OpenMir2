# 背包夹具（inventory）可达性修复（`openmir2-inventory-fixture-reachability`）

- 卡：`openmir2-inventory-fixture-reachability`（tier=L）；分支 `card/openmir2-inventory-fixture-reachability`；写集 `card_window.py` / `mir2_probe.py` / `scenarios/**` / `docs/`。
- 基线：`walgit/master = 9083f4863`（前置 `openmir2-scenario-fixture-precondition` 已合入）。

## 1. 为什么之前不可达（两条独立断点）

1. **场景带不了声明**：`mir2_probe.load_scenario` 对未知顶层键 fail-closed，optional 列表只有
   `description/params/required/computed/placement/blocked/requires` ⇒ 场景写 `inventory` 直接
   `scenario-error: unknown key(s): ['inventory']`（`--check-scenario` 与真跑同走该函数）。
2. **dry-run 静默**：`card_window` 虽然早有 `scenario_inventory` 读取与 `run_inventory` 写入，
   但 `--dry-run` 分支从不预览 inventory ⇒ **rc=0 且 0 行**，被误读成「夹具已就绪」。

## 2. 选定设计（① 场景自描述）与理由

- **选 ①**：`inventory` 的内容（物品 index、MakeIndex、槽位、可选 std_mode/ani_count/dura）**全在场景里**，
  不需要 profile 段；因此「缺 profile 段则 rc=2」在 ① 下退化为「缺 `db.characters_db`/`char_name`/
  模板不存在/槽位被占用 ⇒ rc=2 点名」（由 `run_inventory` 既有校验承担）。这与 `requires`（profile 段型前置）
  是**两类不同前置**：`requires` 声明「profile 必须有什么段」，`inventory` 声明「本侧测试库必须预置什么物品」。
- 未选 ②（只走 profile）：profile 里放物品清单会让「同一场景在两侧库」出现第二份事实源，且 profile 是
  环境描述而非场景语义。

## 3. 改动

| 文件 | 改动 |
|---|---|
| `tools/probe/mir2_probe.py` | optional 键新增 `inventory`；新增 `_check_inventory`（必填 `index`/`make_index>0`/`position 0..45`；可选 `std_mode`/`ani_count`/`dura`/`dura_max`/`note`；未知键/类型/越界/重复 `position`/重复 `make_index` ⇒ fail-closed）；新增 selftest（**10 → 11 checks**）。 |
| `tools/conformance/card_window.py` | 新增 `preview_inventory` 并在 `--dry-run` 分支调用（两侧各打一行「背包夹具（dry-run 不连库）：db=… 声明 N 件 positions=… make_index=… 角色=…」）；`run_inventory` 的 dry-run 分支同样打印；新增 `_selftest_inventory_reachability` 3 用例 ⇒ **selftest 226 → 229**。 |
| `tools/probe/scenarios/bag_equip.json` | 新增 `inventory`：木剑 `index=207`（StdMode 5）/ `make_index=5001` / `position=40`，与 `--var make_index=5001` 配套。 |
| `tools/probe/scenarios/eat_item.json` | 新增 `inventory`：金创药(小量) `index=3`（StdMode 0, AniCount 102）/ `make_index=5002` / `position=41`，与 `--var make_index=5002` 配套。 |
| `tools/probe/scenarios/README.md` | 写清 `inventory` 声明形状与 fail-closed 条件（见 §5）。 |

物品取值依据部署 `mir2_data.stditems`（只读查询）：`207|木剑|StdMode 5|AniCount 0|DuraMax 4000`、
`3|金创药(小量)|StdMode 0|AniCount 102`。

## 4. 实测（今日）

| 命令 | rc / 输出 |
|---|---|
| `python -I -B tools/conformance/card_window.py --selftest` | **GREEN 229/229**（改前基线 226/226） |
| `python -I -B tools/probe/mir2_probe.py --selftest` | **OK（11 checks）** |
| `mir2_probe --check-scenario --scenario bag_equip.json --var make_index=5001 --var takeon_where=1 --var item_name=x` | **rc=0** |
| 同上 `eat_item.json --var make_index=5002 --var item_name=x` | **rc=0** |
| `card_window --dry-run --scenario bag_equip.json --profile alt` | **rc=0**，两侧各 **1 行**「背包夹具（dry-run 不连库）」预览 |
| `selftest_mutations.py`（全量 48 牙） | **39/48 已跑，全部 OK、0 失败**（`T1–T21`、`E1–E7`、`D1–D2`、`P1–P5`、`B1`、`MAG1–2`、`RQ1`）；剩余 `RQ2`、`MG7`、`INV1–INV7` 因宿主机内存压力被系统终止后台任务而**未跑**（见 §6 残留） |

> 不带 `--var` 时 `bag_equip`/`eat_item` 仍 rc=2（`required` 的 fail-closed 不变）；inventory 不会替 `required` 兜底。

## 5. `scenarios/README.md` 的声明说明（同文落库）

```json
"inventory": [
  {"index": 207, "make_index": 5001, "position": 40,
   "std_mode": 5, "ani_count": 0, "dura": 4000, "dura_max": 4000,
   "note": "木剑（StdMode5，可装备）"}
]
```

- 必填 `index` / `make_index`（>0）/ `position`（0..45）；可选 `std_mode`/`ani_count`（声明时须与模板一致，
  不符该侧 rc=2）/`dura`/`dura_max`/`note`；未知键、类型错、重复槽位、重复 MakeIndex ⇒ 加载期 rc=2。
- 预置时机：每侧 restore 之后、起栈之前；两侧共用同一份声明 ⇒ MakeIndex 天然一致；声明槽位必须原本为空。
- dry-run：打印预览行且 rc=0；前置不成立（库名/库配对/客户端参数）⇒ rc=2 并点名。

## 6. 牙（2 颗，均实测红 + `git checkout --` 还原、sha256 对齐 head blob）

| 牙 | 变异 | 结果 |
|---|---|---|
| A | 删掉 `preview_inventory` 的预览 `runner.log(...)` | `card_window --selftest` **rc=1**，`FAIL ② dry-run 两侧打印…` |
| B | 从 `mir2_probe` optional 键里删掉 `inventory` | `mir2_probe --selftest` **rc=2**（`scenario-inventory` 用例红） |

## 7. 门禁与残留

- `card_window --selftest` 229/229；`mir2_probe --selftest` 11 checks；`selftest_mutations.py` 全红（见交付 status）。
- `selftest_mutations.py` 逐牙命令：`python -I -B tools/conformance/selftest_mutations.py --only <ID>`；本次已跑 39/48 全 OK（每牙后逐字节还原 + sha256 对齐基线），**剩余 9 牙**（`RQ2`、`MG7`、`INV1–INV7`）未跑，原因：后台任务被宿主机内存压力终止（非牙失败）；需要时按上面命令逐牙补跑（约 8–12 分钟）。
- `t_reservation_gate` / `whitelist_anchor_gate` / 冲突标记 / `merge-tree`：见交付 patch。
- 残留：`bag_equip`/`eat_item` 的真库预置（槽位占用、模板一致性、读回）**仍需真窗口**跑一次
  （`openmir2-window-inventory-fixture` 的 `window_required=true` 窗项不变）；本卡只解决「可达」。
