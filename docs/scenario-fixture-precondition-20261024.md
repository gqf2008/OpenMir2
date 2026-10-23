# 场景夹具前置（`requires`）：封死 dry-run 假绿通道

卡：`openmir2-scenario-fixture-precondition`（tier=L，issue `cd35ac9684`）。
写集授权：`tools/probe/mir2_probe.py` 的扩写由 coordinator 的签名 comment `ffd9a859`
（`write_set_authorization`，范围见下）授权。

## 1. 问题（不是设想，是实测）

`card_window.py` 的 `profile_magic(prof, side)` 在 profile 没有 `magic` 段时返回 `None`，
于是 `--dry-run` 的 magic 段**整段跳过**：

```
python -I -B tools/conformance/card_window.py --dry-run \
  --scenario tools/probe/scenarios/spell_cast.json --card X --out E:/tmp/spellcast-dry
# 扩写前：rc=0，日志只有「预置位」「端口预检」「就绪等待」，**无「魔法夹具」行**
```

`spell_cast` 在 `openmir2-spell-cast-unblock` 摘掉 `blocked` 后，就落进了这条通道：
**场景需要 `characters_magic` 夹具，但默认 `alt` profile 没声明 `magic` 段时 dry-run 仍 rc=0**。
真跑不会出假 PASS（`wait_for` 会超时），但 **dry-run 绿会被误读成「可以真跑」**——正是 BL 硬门
当初要防的事。

## 2. 语义：场景顶层 `requires`（可选）

```json
"requires": ["magic"]
```

* **可选**；不写 = 行为逐字节不变（既有 222 条 selftest 的 rc 与输出不变）。
* 目前**只认 `magic`**。声明后 `card_window.py` 在**加载期**核**生效 profile**（顶层 `magic`
  应用两侧，`sides.<side>.magic` 可覆盖本侧）；`magic` 要在**两侧**分别写各侧测试库，所以
  **两侧都要有** `magic` 段，任一侧缺 ⇒ **rc=2**。
* 未知名（拼错、未来值）fail-closed rc=2 并点名：避免「加了值却没生效」再次假绿。
* 判定位置与 BLOCKED 硬门同级（建 out / 起栈 / 跑 discovery / 写 report 之前），
  `--dry-run` 与真跑走同一条路径。

**fail-closed 消息样例（实测，alt profile）**：

```
card_window.py: 场景 `spell_cast` 要求 `magic` 夹具，但 profile `alt` 未声明 `magic` 段（或某侧被覆盖成空）：缺 csharp、rust 侧（场景文件 E:\tmp\wt-worker-63\tools\probe\scenarios\spell_cast.json）
rc=2
```

带 `magic` 段的 profile（`--profile <json>`）⇒ rc=0，且两侧各一条预置行：

```
[csharp] 魔法夹具（dry-run 不连库）：db=mir2_test_db magic_ids=1
[rust]   魔法夹具（dry-run 不连库）：db=mir2_test_db_r magic_ids=1
```

## 3. 实现

| 文件 | 改动 |
|---|---|
| `tools/conformance/card_window.py` | 新增 `REQUIRES_KNOWN=("magic",)`、`scenario_requires(path)`（形状/取值 fail-closed）、`validate_scenario_requires(...)`（按生效 profile 逐侧核）；`execute()` 加载场景时读 `requires`，在 BLOCKED 硬门之后、缺密码/建 out/起栈之前校验。报错点名 `profile_label`（内建名/路径原样，inline dict 用 `prof["name"]`）。 |
| `tools/probe/mir2_probe.py`（授权扩写） | `load_scenario` 的 optional 顶层键加 `"requires"`；新增 `_check_requires`（非空、无重复、非空字符串数组；值只认 `magic`，未知值点名）；`run_selftest` 新增 `scenario-requires`（6 个形状用例）。**不动**加载/执行链的其它语义。 |
| `tools/probe/scenarios/README.md` | 「文件形状」顶层键列表 + `requires` 专节 + 退出码 fail-closed 列表 + selftest 计数 9→10。 |
| `tools/probe/scenarios/spell_cast.json` | **只加** `"requires": ["magic"]`（steps/notes/description 未动）。 |
| `tools/conformance/selftest_mutations.py` | 新增 3 颗牙：`RQ1`/`RQ2`/`MG7D`。 |

## 4. selftest / 牙计数

* `card_window.py --selftest`：**222 → 226**（新增 4 条，全部在 RED/GREEN 判定之前入账）：
  * `RQ-1` 合成场景 `requires=["magic"]` + profile 无段 ⇒ ConfigError（rc=2 口径），消息点名
    场景 / profile / **两侧都缺**，且一步未执行（不建 out、不起栈）；
  * `RQ-2` 同场景 + profile 有段 ⇒ rc=0，且**两侧都打印**「魔法夹具（dry-run 不连库）」预置行；
  * `RQ-3` `requires=["magic","inventory"]` ⇒ 加载期 ConfigError 并点名未知值（只认 `magic`）；
  * `RQ-4` 顶层无 `magic`、只有 `sides.csharp.magic` ⇒ 仍 rc=2 且**只点名 rust 侧**（两侧分别判）。
* `selftest_mutations.py`：**45 → 48** 颗牙，全红（含逐字节还原 sha256 校验）：
  * `RQ1` 摘掉加载期校验调用点 ⇒ RQ-1 变红；
  * `RQ2` 把 `missing` 写死成两侧（有段也判缺）⇒ RQ-2 变红；
  * `MG7D` `run_side` 的 `profile_magic(prof, side)` → `profile_magic(prof)` ⇒ 只有 MG-7 组变红。
* `mir2_probe.py --selftest`：**9 → 10 checks**，新增 `scenario-requires`。

## 5. 顺带收掉上一张卡的两条 minor

1. **`execute` 层 `res["inventory"]` 的 rc 兜底是死分支** — `run_side` 的 `rc` 默认 2，背包夹具失败时
   它在写 `<side>/inventory.json` 后立刻 `return`（`rc=2` + `error`）；本层再判一次永远不会生效。
   **处置：删除该 rc 兜底**，只保留「把该侧证据并进 summary」；真正判据在 `run_side` 的
   `if ires.get("rc") != 0:`（既有 ⑰ 用例继续覆盖）。
2. **MG-7 断言补一颗牙** — 登记为 `MG7D`（见上）：把 `profile_magic(prof, side)` 改成
   `profile_magic(prof)` ⇒ `card_window --selftest` **rc=1**，FAIL 落在
   `MG-7 sides.csharp.magic 覆盖顶层`（csharp 读到顶层 `[1]`，rust 同为 `[1]`）。

## 6. 门禁（本 worktree 实测）

| 门禁 | 命令 | 结果 |
|---|---|---|
| 卡窗口自检 | `python -I -B tools/conformance/card_window.py --selftest` | GREEN 226/226 |
| 变异牙 | `python -I -B tools/conformance/selftest_mutations.py` | 48/48 全红 + 逐字节还原 |
| probe 自检 | `python -I -B tools/probe/mir2_probe.py --selftest` | OK（10 checks） |
| 场景加载 | 3 个未声明 `requires` 的既有场景（`login_only`/`gm_time`/`dice_chain`）`--check-scenario` | 输出与扩写前逐字节相同 |
| 真实场景 | `--dry-run --scenario spell_cast.json`（alt） | rc=2，点名缺 `magic` |
| T 预约表 | `python -I -B tools/whitelist/t_reservation_gate.py check` | GREEN（rows=252 / whitelist=252） |
| 冲突标记 | `python -I tools/hygiene/conflict_marker_gate.py` | OK（0 markers / 7436 files） |
| 合并树 | `git merge-tree --write-tree refs/remotes/walgit/master HEAD` | rc=0 |

## 7. 残留 / 未做

* 只实现 `magic` 一种夹具；`inventory` / `placement` 若要同样声明，需另开卡（`requires` 已按
  未知值 fail-closed，写了也不会静默不生效）。
* `requires` 的「两侧都要有」是当前 `magic` 实现的语义（两侧各写各的测试库）；若将来出现
  「只需要一侧」的夹具，需要显式扩展 schema（例如按侧声明），不要靠 profile 覆盖去猜。
* 本卡全程离线（`--dry-run` / `--selftest` / 合成 profile），**未起栈、未连库、未占端口**。
