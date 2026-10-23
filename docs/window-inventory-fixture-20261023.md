# 窗口背包夹具（`inventory`）——返工与机判证据（T-2xx 卡 `openmir2-window-inventory-fixture`）

- 卡：`openmir2-window-inventory-fixture`（tier=L）；分支 `card/openmir2-window-inventory-fixture`
- 复审：`c866f358`（reviewer-5，request_changes）——本文件逐条对应其 findings

## 1. 交付面

| 面 | 位置 |
|---|---|
| 场景声明 `inventory` 的解析/校验 | `tools/conformance/card_window.py`（`scenario_inventory` / `validate_inventory_content` / `compare_inventory_make_index`） |
| 起栈前预置 | 同上 `run_inventory`（restore 之后、stack 之前） |
| 运行手册 | `docs/window-runbook-20261023.md` **§1.6「背包夹具：声明即可」**，并改掉 §1 与 §4.5 两处「仍不支持/未接」的矛盾表述 |
| 机判 | `card_window.py --selftest` 的 INV 组（①-⑨）+ `selftest_mutations.py` 35 颗牙 |

## 2. fail-closed 口径（本 head）

- 形状：`index`/`make_index`/`position` 必填；`position` 0..45 且唯一；`make_index` >0 且唯一；未知键/类型错 ⇒ rc=2；
- 模板：`mir2_data.stditems` 查不到 ⇒ rc=2；声明了 `std_mode` 且与模板不符 ⇒ rc=2；
- **槽位必须原本为空**（M-1）：声明槽位已有行 ⇒ rc=2（DELETE+INSERT 会把该行其它列重置成默认值，工具不替操作者覆盖）；
- **DB 配对**（I-2）：与 `run_placement` 同口径调 `_check_db_pairing`，profile 的角色库与该侧部署 `Server.conf` 不符 ⇒ rc=2；
- 写后：声明槽位的行数必须覆盖声明件数；**"其它槽位"（排除声明位置）的行数与 MD5 指纹必须逐字节不变**，否则 rc=2 且记 `altered_columns`。

## 3. 机判覆盖（review I-3：此前四种改动都能绿）

`--selftest` 的 INV 组现在用假 runner 直接驱动 `run_inventory` 本体，并锁定生产接线：

| 用例 | 断言 |
|---|---|
| ⑤ | 生产路径走通：rc=0 且有 INSERT SQL |
| ⑥ | `StdMode` 与声明不符 ⇒ rc=2 且点名 |
| ⑦ | UPDATE 执行失败 ⇒ rc=2 且点名 |
| ⑧ | 其它槽位指纹变化 ⇒ rc=2 且 `altered_columns` 非空 |
| ⑨ | `run_side` 源码里 `if inventory:` 必须在 `run_inventory(` 之前、且 `run_inventory(` 在 `"stack"` 之前 |

**自检位置（B-1）**：INV 组整体移入 `try` 块、在 `if fail_n: return 1` **之前**运行（原来在判定之后 ⇒ 永不判红、且 tmp 已被删）。

## 4. 牙（实做，rc 为真实返回码）

| 牙 | 改动 | 真实结果 | 还原 |
|---|---|---|---|
| T1 | `run_side` 里 `if inventory:` → `if False and inventory:` | `--selftest` **rc=1**，`FAIL ⑨`（1 条 FAIL） | 逆向改回；`sha256 = f93fab42894b4681c2594987691c18821bcbdb984338c4f06ad312639375da48`（= 提交后树） |
| T2 | `run_inventory` 失败分支 `res["rc"] = 2` → `0` | `--selftest` **rc=1**，`FAIL ⑥⑦⑧`（3 条） | 同上，sha 一致 |
| T3 | `selftest_mutations.py` P3 的锚点去歧义后重跑全量 | `MUTATION HARNESS GREEN: 35/35` | harness 自校验还原 sha |

> T1 的第一次尝试用「源码里出现 `run_inventory(`」判接线，被同一条 `if False and inventory:` 绕过 ⇒ 已把判据改成「`if inventory:` 必须存在且在调用前」，再跑才咬到（这条本身记入本卡 comment）。

## 5. 门禁

```
python -I -B tools/conformance/card_window.py --selftest     GREEN 176/176（≥ 165）
python -I -B tools/conformance/selftest_mutations.py          GREEN 35/35（expect 锁判据组）
python -I -B tools/probe/mir2_probe.py --selftest             OK（8 checks）
python -I tools/whitelist/t_reservation_gate.py check         GREEN（rows=202 whitelist=202）
```

## 6. 其它复审项

- **作者/签名不一致**（minor）：本车道 `git config user.name/email` 已改为 `openmir2-worker-24`（与 principal 一致），后续提交的作者 = 签卡人。
- 临时目录泄漏（minor）：INV 组改到 `finally: rmtree` 之前运行，`_selftest_inventory` 内的 `inv/` 目录随之被清理。

## 7. 残留物

- `E:/tmp/inv-t1.log`、`E:/tmp/inv-t2.log`（牙的真实输出）、`E:/tmp/inv_*.py`（改树/还原脚本，只读工具）。
