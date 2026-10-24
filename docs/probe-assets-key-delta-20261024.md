# probe 顶层键放行 assets + 同口径形状校验（Gold 通道 follow-up）

- 卡：`openmir2-probe-assets-key-delta`（tier=L）
- 分支：`card/probe-assets-key-delta`，head = 本文件所在提交
- base：`walgit/master` = `11c024583`（批 2 合并后的 master；开工时为 `968ede63c`，rebase 跟进）
- owner：`openmir2-worker-76`，worktree `E:/tmp/wt-worker-76`

## 1. 背景与目标

Gold 通道卡（`968ede63c`，review `f7a4acc9bf`）边界实测：`tools/probe/mir2_probe.py` 场景顶层键
白名单无 `assets` ⇒ 带 assets 声明的场景 `--check-scenario` rc=2 unknown key，通道端到端不可用。
本卡放行 `assets` 键并做与 `card_window.scenario_assets()` **同口径**的形状校验
（白名单列 Gold、JSON int 0..2^31-1、note 字符串），不碰通道实现、不碰场景 JSON。

## 2. delta（写集）

`tools/probe/mir2_probe.py` 一个文件，+128/−1：

1. `load_scenario` 顶层 `_need_keys` optional 白名单加 `"assets"`，并调 `_check_assets(data.get("assets"))`；
2. 新增常量 `ASSETS_COLUMNS=("Gold",)` / `ASSETS_MAX=0x7FFFFFFF` / `ASSETS_NOTE_KEY="note"`
   （注释注明事实源 = `card_window.py` 同名常量，两侧必须同步改）；
3. 新增 `_check_assets(assets)`：缺省放行；非空对象；未知列点名拒（fail-closed）；note 非字符串拒；
   值必须 JSON int（bool/浮点/字符串/null 一律拒）且 0..0x7FFFFFFF；note-only 拒；
   合法时 stderr 打一行「本驱动器不写库」提示（沿 `placement` 先例）；
4. selftest 新增 `_selftest_scenario_assets` 并注册 `scenario-assets` 项（4 正 + 12 负）。

Rust / `card_window.py` / 场景 JSON / rules.json 零变更（`git diff --name-only walgit/master -- src/` 空）。

## 3. 验收证据

### ① 正牙 / 红牙（CLI `--check-scenario`，16 份同形输入，临时场景用后即删）

命令：`python E:/tmp/wt76_verify_assets.py`（临时驱动，跑完已删；逐份 rc+点名输出见下表口径）

- 正牙 rc=0 共 4 份：无 assets / `{"Gold":10000,"note":…}` / `{"Gold":0}` / `{"Gold":2147483647}`（边界）；
- 红牙 rc=2 共 12 份，全部点名：空对象、`[]`、未知列 `Credit`、夹带 `Level`、note-only、
  `"10000"`(str)、`1.5`(float)、`true`(bool)、`null`、`-1`、`2147483648`(超 int32)、note 非字符串。

### ② 两侧同判（同形输入同判）

同一 16 份输入分别过 probe `--check-scenario`（rc）与 `card_window.scenario_assets()`（返回/ConfigError）：
**16/16 判定一致**（4 accept / 12 reject 双侧同向）。

### ③ 门禁（head `3bfe16a01` 自证）

| 门禁 | 结果 |
| --- | --- |
| `python tools/probe/mir2_probe.py --selftest` | OK（14 checks，含新 scenario-assets）rc=0 |
| `python -I -B tools/conformance/card_window.py --selftest` | GREEN 272/272 rc=0 |
| `python -I -B tools/conformance/compare_runs.py selftest` | GREEN 295/295 rc=0 |
| `python tools/hygiene/stale_pointer_gate.py` | rc=0 |
| `python tools/hygiene/dynamic_count_stamp_gate.py` | rc=0 |
| `python tools/hygiene/conflict_marker_gate.py` | rc=0 |
| `git diff --check` | rc=0 |
| `git merge-tree --write-tree HEAD walgit/master` | rc=0（无冲突） |
| Rust 零变更 | `git diff --name-only walgit/master -- src/` 空 |

### ④ 在库场景 JSON 未动

`git diff --stat walgit/master` = 仅 `tools/probe/mir2_probe.py`（+128/−1）。
批 2（`11c024583`）实测未采用 assets（全库无 `"assets"` 场景声明），场景采用仍在后续 delta。

## 4. 残留物清单

- 临时驱动脚本 `E:/tmp/wt76_patch_assets.py`、`E:/tmp/wt76_verify_assets.py`：已删；
- 16 份临时场景（`%TEMP%/wt76-assets-*`）：驱动 finally 整目录删除，已验证 `tmpdir removed: True`；
- 无进程残留（全程离线，未起栈/未连库）。
