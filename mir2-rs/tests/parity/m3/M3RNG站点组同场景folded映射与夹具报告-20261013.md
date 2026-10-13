# M3 RNG 站点组同场景两相 trace：R-6 folded 映射与场景夹具报告（2026-10-13）

- 卡：`openmir2-m3-rng-trace-scene-groups`（issue `d9d03f47`；owner 派单 comment `6f19eacb`）
- 车道：`m3bag`／principal `openmir2-worker-4`／worktree `m3bag`／分支 `worktree-m3-rng-trace-scene-groups`
- base = `refs/heads/master` `17174149`（开工即头部；交付前再 `git merge origin/master`）
- 写集：`mir2-rs/tests/parity/m3/**`。**零** `crates/**`、`bins/**`、`src/**`、`tests/parity/world/**`。
  **未起栈、未占端口**（真机两相 trace 等 owner 发窗口）。
- 关联：`M3全服RNG消费者结构一致性基线报告-20261013.md`（Claim B 工具与基线）

> 一句话：按 owner R-6 裁决把「C# `construct` 折叠到 Rust `spawn`」做成**机判**（折叠后**计数与相对次序
> 必须相等**，只写白名单不校验不算达成）；并产出**同场景 / 同起点 / 同种子**两相 trace 的**场景夹具 +
> 场景一致性前置门**（MonGen/Monster 集合同口径，否则 fail-closed），最后写清 `drop` 组判据与 ④
> 「死亡掉落不落地」口径的兼容性。真机跑等窗口。

---

## 1. R-6 folded 映射（卡 DoD ③）

### 1.1 裁决与实现

owner 2026-10-13 裁决：Rust 把构造期抽数折叠进刷怪路径
（`crates/world/src/mongen.rs` 的 `AnimalState::from_spawn_draws`）⇒ **接受为登记建模差异**，
但**必须机判等价**：C# `construct` 折叠到 Rust `spawn` 后，**折叠计数与相对次序必须相等**；
**只写白名单不校验不算达成**。

实现落在两个共享文件（唯一真源，不各处手抄）：

- `rng-trace-probe/rng_consumer_groups.py`
  - `FOLD_RULES = {"construct": "spawn"}`；
  - `fold_group()` / `folded()` / `folded_counts()`；
  - `validate_folds()`（源/目标必须是已知组、源≠目标）**并入** `GroupsTable.validate()` ⇒ 表自检。
- `rng-trace-probe/rng_ledger_diff.py`：`groupdiff … --fold`
  - 先把两侧组序列经 `FOLD_RULES` 逐元素折叠，**在折叠后的序列上**重算：
    - 组集合（`GROUP_SET_DIFF`）；
    - **折叠计数**（`FOLDED_COUNT_DIFF`：逐组 `a`/`b` 计数不等即红）；
    - 共享组相对次序（`SHARED_REL_ORDER_DIFF`）。
  - `--strict` 下任一即 rc=1；**缺省不传 `--fold` 时行为与既有逐字节相同**（`construct` vs `spawn`
    仍如实报 `GROUP_SET_DIFF`）——折叠是**显式开关**，不是全局豁免。

### 1.2 机判与改坏必红（原始输出见证据包）

| 场景 | 命令 | 读数 |
|---|---|---|
| 折叠等价 | `groupdiff --fold --strict`：C# `[construct,spawn,monster_ai]` vs Rust `[spawn,spawn,monster_ai]` | **`VERDICT IDENTICAL` + `FOLDED_COUNT_EQUAL`** rc=0（`08-`） |
| 折叠**计数**不等 | C# `[construct,spawn]`（折叠 spawn=2）vs Rust `[spawn]` | **`FOLDED_COUNT_DIFF`** rc=1（`09-`） |
| 折叠后**次序**不等 | C# `[construct,monster_ai,spawn]` vs Rust `[spawn,spawn,monster_ai]` | **`SHARED_REL_ORDER_DIFF`** rc=1（`10-`） |
| 折叠表改坏 | `FOLD_RULES['construct']='no_such_group'` | `validate_folds()` **点名**未知目标组（`12-`） |
| 缺省不折叠 | `groupdiff --strict`（无 `--fold`） | `construct` vs `spawn` 仍 **`GROUP_SET_DIFF`**（`11-`） |
| 工具自检 | `rng_ledger_diff.py selftest` | **32/32 GREEN**（含 R-6 folded 全组用例，`01-`） |
| 回归 | `attack_rng_ledger.py selftest` | **32/32 GREEN**（语义未改，`03-`） |

### 1.3 基线实拍：折叠**不是豁免**

在现存唯一一对**不同场景**活栈 trace 上（C# `a7df7116…` / Rust `4963e9f6…`）：

```
--strict          → VERDICT GROUP_SET_DIFF+SHARED_REL_ORDER_DIFF                                   rc=1  (13-)
--fold --strict   → VERDICT GROUP_SET_DIFF+SHARED_REL_ORDER_DIFF+FOLDED_COUNT_DIFF                 rc=1  (14-)
                   其中 spawn 折叠后 a=146545（=82840 construct + 63705 spawn） vs b=0
```

⇒ 若 Rust 只是「没有独立 `construct` 站点」而折叠后的 `spawn` 数量对不上，**照样红**。这正是 owner
要的「只写白名单不校验不算达成」。

---

## 2. 同场景夹具 + 场景一致性前置门（卡 DoD ①②）

### 2.1 夹具（`rng-trace-scene/`）

| 件 | 作用 |
|---|---|
| `rng_trace_scenario.json` | canon `m3-rng-trace-scene-v1`：图 `0137`、种子 `42`、起点 `(17,20) dir=2`、pin 怪 `稻草人 @ (18,20) dir=0 ×1`、MonGen 冻结件 `../fixtures/mongen-map0137.tsv` |
| `rng_scene_gate.py` | 场景一致性前置门（+ selftest 11/11） |
| `run_rng_trace_pair.ps1` | 两相驱动骨架：`-SelfCheckNoStack` / `-GateOnly` / `-GroupDiffOnly`；real-run 无 `-Go` ⇒ exit 2 |
| `README.md` | 场景、门、折叠、drop↔④ 兼容性、窗口纪律、边界 |

**为什么是 0137**：`fixtures/mongen-map0137.tsv` = **0 行**（整张零刷怪），
`fixtures/ai-consumer-map0137.tsv` 五文件（GuardList/MerChant/Npcs/MapEvent/MapQuest）**零引用**
⇒ 图上没有**其它** AI 消费者；夹具只 pin 一只怪 ⇒ `monster_ai`（Wondering）/`spawn`（CreateMonster）/
`drop`（生成期掉落预算）三组都出现且集合确定。该图/起点/种子与 ④ `drop-exp` 一致，
**可与 ④ 100 杀编排在同一窗口批次**（串行）。

### 2.2 前置门（卡 DoD ①：不满足 ⇒ fail-closed 拒比）

两侧各产出一份**场景描述件**`<side>-scene.json`（canon `m3-rng-scene-descriptor-v1`）。门比：

| 检查 | 口径 |
|---|---|
| `sides-distinct` | 一 `csharp` 一 `rust` |
| `map-equal` / `seed-equal` / `pose-equal` | 两侧都等于 scenario |
| `monsters-equal:*` | **Monster 集合同口径**：`(name,x,y,dir)` 多重集 + 计数（a==b==scenario） |
| `mongen-equal:*` | **MonGen 集合同口径**：两侧 `mongen_rows` 逐行多重集都等于冻结件 |

退出码 `0`/`1`/`2`。改坏必红：怪集合多一只（`06-`）、缺件（`07-`）分别 rc=1/rc=2 点名；
自检 11/11 GREEN（`02-`），并覆盖 seed/起点/map/MonGen 行漂移与「同 side 去比」的负样本。

### 2.3 无栈自检（DoD：`-SelfCheckNoStack`）

`run_rng_trace_pair.ps1 -SelfCheckNoStack` = **12/12 GREEN**（`04-`，LF 转录由脚本自己落盘，零端口）：
scenario 固定输入、MonGen 冻结件零行、场景门 selftest、场景门进程内正/负样本、账本 selftest（含 folded）、
folded 等价绿 / 计数不等红 / 次序不等红、缺件 PREREQ、LF 写出、零端口。real-run 无 `-Go` ⇒ **exit 2**。

---

## 3. `drop` 组判据与 ④「死亡掉落不落地」的兼容性（卡 DoD ④ / R-3）

**结论：两者测不同的东西，不冲突、不互相掩盖；同窗口同图串行即可同跑。**

| | 本卡（RNG 站点组） | ④ `openmir2-m3-drop-exp-align` |
|---|---|---|
| 对象 | 掉落点 **RNG 取数账本**：`MonGetRandomItems`（`WorldServer.MonGen.cs:440`）+ `GameItemSystem.GetUpgrade/UpgradeRandom*` | 死亡掉落的**落地产物**：地面物 / `SM_ITEMSHOW(610)` / `RM_ITEMSHOW` |
| 判据 | `drop` 组两侧取数账本 diff=0（本卡：组集合 + 折叠计数 + 次序） | 落地**不出现**（oracle 已裁决不落地） |

oracle 口径（owner 2026-10-13 `aace16e0`/`8684d755` 裁决，实现已合 master `4521d451`）：
`MapItem.cs:41` 唯一 `ItemId` 赋值点被注释 + `Envirnoment.cs:88-93` 首行 `if (ItemId == 0) return false`
⇒ **oracle 仍抽 RNG、只是不落地**。

- 本卡**不**断言地面物存在（只测抽数账本）⇒ 与 ④-D 的「不落地」**无矛盾**；
- ④ **不**断言 RNG 抽数 ⇒ 与本卡无矛盾；
- **唯一要避的冲突**：④ 的场景纯度门（whitelist T-35）用 **pin 抑制**把墙钟消费者
  （`AnimalObject.Wondering`）压到 **0**；而本卡**要** `monster_ai` 出现 ⇒ **本卡不得沿用 pin 抑制**，
  两卡是**同图同起点同种子的两次独立跑**，在**同一窗口串行**编排，不并行起栈。

同窗口编排建议顺序见 `rng-trace-scene/README.md` §4。

---

## 4. 判据与命令（可复算）

```sh
cd mir2-rs/tests/parity/m3
python -I rng-trace-probe/rng_ledger_diff.py selftest      # 32/32 GREEN（含 R-6 folded）
python -I rng-trace-scene/rng_scene_gate.py selftest       # 11/11 GREEN
powershell -ExecutionPolicy Bypass -File rng-trace-scene/run_rng_trace_pair.ps1 -SelfCheckNoStack   # 12/12 GREEN
# 窗口期（owner 发 -Go 后）：
#   rng-trace-scene/run_rng_trace_pair.ps1 -GateOnly -AScene <csharp-scene.json> -BScene <rust-scene.json>
#   rng-trace-scene/run_rng_trace_pair.ps1 -GroupDiffOnly -AStream <csharp.tsv> -BStream <rust.tsv>
```

证据包：`evidence-m3-rng-trace-scene-groups-20261013/**`（18 件；`--check-extra` + `eolcheck` GREEN）。

---

## 5. 未做 / 边界（如实登记）

- **真机两相 trace 未跑**：需 owner 发放独占窗口 + 两侧「场景描述件 + 全场景抽数流水」驱动接线
  （承接卡，本片外）。real-run 分支在无 `-Go` 时 fail-closed。
- 基线 §2.1 的 Rust 侧 `monster_ai`/`spawn`/`drop` = 0 是**窗口/场景缺**（不同场景、旧代码件），
  不是代码没写（`crates/world/src/monai.rs` / `mongen.rs`、`crates/formula/src/drop.rs` 已在 master）。
- `merchant` / `npc_ai` / `revive` 组仍缺 Rust 实现（R-4 / R-7），本卡不覆盖，随各自系统卡收拢。
- 折叠规则目前只有 `construct -> spawn`（owner R-6）；若将来新增折叠，必须同时补 `validate_folds`
  与 `groupdiff --fold` 的计数/次序用例（selftest 已按此形态构建）。

## 6. 残留物清单

- 仓外临时件：`E:/tmp/m3bag-fold-evid/**`（生成脚本与基线 raw 输出）、`E:/tmp/rng-scene-selfcheck.txt`
  —— 未入库，可清。
- 工作树内未跟踪 `mir2-rs/tests/parity/m3/sessions/csharp-20261012-2323..2346/`（8 个，**别车道遗留**）
  —— 未并入本片。
