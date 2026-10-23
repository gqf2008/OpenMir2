# `computed.pack`：场景派生量的打包表达式（`openmir2-probe-computed-pack`）

- 卡：`openmir2-probe-computed-pack`（tier=L）；分支 `card/openmir2-probe-computed-pack`
  ——**叠在已 approve 的 `card/openmir2-scenario-recog-packing-fix`（`7ef4bda9c`）之上**，合并顺序：先 recog，再本卡。
- 起因：recog 打包修正要求 `move_walk.json` 的 `required` 含 `move_xy`，但 `computed` 只声明了
  `move_x/move_y` ⇒ `move_xy` 推不出 ⇒ `param_discovery check-all` 对 `move_walk` rc=2（我上一张卡的回归）。

## 1. 语义

```json
"computed": {
  "move_x": {"derived_from": "self_x", "delta": 1},
  "move_y": {"derived_from": "self_y", "delta": 0},
  "move_xy": {"pack": {"lo": "move_x", "hi": "move_y"}}
}
```

- `值 = (hi << 16) | lo`，与 oracle `recog = (Y<<16)|X`（`WorldServer.cs:831-851`）同向；
- `lo`/`hi` 可写任意可推导变量，**允许引用其它 `computed`**（按不动点求解，最多迭代 `n+1` 轮）；
- fail-closed：`pack` 缺字段 / 多键 / 与 `derived_from|delta` 混用 ⇒ `DiscoveryError`（场景坏掉）；
  依赖成环 ⇒ `DiscoveryError`；`lo/hi` 非整型（如 `item_name`）或越界 `0..65535` ⇒ 该变量 missing；
  依赖链断 ⇒ missing（绝不拿 0 顶替）；
- `mir2_probe` 仍**不读** `computed`：真跑时打包值经 `--var` 传入，`card_window` 用发现值填。

## 2. 复现与回归

| 步骤 | 命令 | 结果 |
|---|---|---|
| 复现（改前） | `param_discovery.py check-all <SM_LOGON 帧> tools/probe/scenarios` | `move_walk rc=2 missing: move_x,move_y,move_xy` |
| 修复后 | 同上 | `move_walk rc=0 computed`，`params=move_x=8,move_xy=786440,move_y=12` |
| 无新增红 | `check-all mir2-rs/tests/golden/c-line-baseline-20261010-092858.frames.ndjson tools/probe/scenarios` | `SUMMARY 19 scenarios / ok=8 / rc2=11`，与改前**逐项一致**（该录制无 `SM_LOGON`，`move_walk` 仍 missing，属既有缺口） |
| 场景形状 | `mir2_probe.py --check-scenario --scenario tools/probe/scenarios/move_walk.json --var …` | `scenario OK` |

## 3. 牙（实做；rc 为真实退出码；sha256 取提交后树）

| 牙 | 改动 | 真实结果 | 还原 |
|---|---|---|---|
| ① | `move_walk.json` 的 `pack` 对调为 `{lo: move_y, hi: move_x}` | `param_discovery selftest` **rc=1**，`㉗` 红（值 `524300` ≠ `786440`）；`check-all` 输出 `move_xy=524300`（值不符） | 还原后 `move_walk.json` sha256 = `239d4c289d7c5b20219ec5d40400065f6870558f40ede3791b042868f5db6bca`（= 提交后树） |
| ② | 删掉 `move_xy` 的 `computed` | `selftest` **rc=1**（`㉗: ['move_xy']`）；`check-all` `move_walk rc=2 missing: move_xy` | 同上 |

## 4. 门禁

```
param_discovery.py selftest      OK（38 cases + 11 teeth；用例计数改为动态，不再写死 25）
mir2_probe.py --selftest         OK（9 checks）
t_reservation_gate.py check      GREEN（rows=208 whitelist=208）
```

## 5. 写集与串行

- 写集：`tools/probe/param_discovery.py`、`tools/probe/scenarios/move_walk.json`、
  `tools/probe/scenarios/README.md`、`docs/`。未改 `card_window.py` / `rules.json` / `src/**`。
- 与在审的 `openmir2-scenario-spell-cast` 同改 `tools/probe/scenarios/README.md`（不同段落）⇒ 串行：
  建议顺序 `recog-packing-fix`（已 approve）→ 本卡 → spell-cast，或按 coordinator 裁定 rebase。
