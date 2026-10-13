# M3 RNG 站点组「同场景 / 同起点 / 同种子」两相 trace 夹具（`rng-trace-scene/`）

- 卡：`openmir2-m3-rng-trace-scene-groups`（issue `d9d03f47`；owner 派单 comment `6f19eacb`）
- 车道：`m3bag`／principal `openmir2-worker-4`／分支 `worktree-m3-rng-trace-scene-groups`
- 状态：**离线骨架已就绪（`skeleton-window-pending`）**；真机两相 trace 等 owner 发放独占窗口
- 写集：`mir2-rs/tests/parity/m3/**`。**零** `crates/**`、`bins/**`、`src/**`、`tests/parity/world/**`；**未起栈、未占端口**。

> 一句话：把 Claim B 的「组集合 + 共享组相对次序」判据从**两个不同场景**的活栈 trace
> （基线的诚实边界，报告 §2.2）推进到**同场景 / 同起点 / 同种子**的**两相** trace——
> 先用**场景一致性前置门**证明「两侧同一场景」（fail-closed），再用 `groupdiff --fold --strict`
> 判「组集合 + R-6 折叠计数 + 共享组相对次序」。

---

## 1. 场景（固定图 + 固定怪 + 固定起点 + 同种子）

`rng_trace_scenario.json`（canon `m3-rng-trace-scene-v1`）：

| 维度 | 值 | 依据 |
|---|---|---|
| 图 | `0137`（竞技场） | `fixtures/mongen-map0137.tsv` = **0 行**（整张零刷怪）+ `ai-consumer-map0137.tsv` 五文件零引用 ⇒ 图上没有**其它** AI 消费者 |
| 起点 | `(17, 20) dir=2` | 与 ④ `drop-exp` / ②③ attack 夹具同起点（可同窗口同图） |
| 怪集 | `稻草人 @ (18,20) dir=0 × 1`（pin） | 夹具只 spawn 固定的一只怪 ⇒ `monster_ai`（Wondering）/`spawn`（CreateMonster）/`drop`（生成期掉落预算）三组都出现且集合确定 |
| 种子 | `42` | 两侧同口径（`MIR2_RNG_SEED`） |
| 判据 | `monster_ai` / `spawn` / `drop` 组集合 + 折叠计数 + 共享组相对次序 | 卡 DoD ②③ |

产物（每侧）：

```
<side>-scene.json      # 场景描述件（canon = m3-rng-scene-descriptor-v1）
<side>-rng-stream.tsv  # 全场景抽数流水：seq \t kind \t max \t result \t site
```

- C# 流水 = `MIR2_RNG_LOG`（`m3/rng-hook` + `MIR2_RNG_SITE=1`）；
- Rust 流水 = `<rust stack log dir>/rng-stream.tsv`。

> ⚠ 本场景**要** `monster_ai`（`AnimalObject.Wondering` 的墙钟游走）出现，**不得**沿用 ④ 纯度门的
> pin 抑制（那是把 Wondering 压到 0）。两套的口径分工见 §4。

---

## 2. 前置门：`rng_scene_gate.py`（同场景的机判）

卡 DoD ① 要求「两侧 trace 必须来自同场景 / 同起点 / 同种子（MonGen 与 Monster 集合同口径），
不满足 ⇒ `groupdiff` fail-closed 拒比」。本门就是这条。

**场景描述件**（`*-scene.json`）：

```json
{
  "side": "csharp",
  "canon": "m3-rng-scene-descriptor-v1",
  "map": "0137",
  "seed": 42,
  "start_pose": {"x": 17, "y": 20, "dir": 2},
  "mongen_rows": [],
  "monsters": [{"name": "稻草人", "x": 18, "y": 20, "dir": 0, "count": 1}],
  "trace": "…/rng-stream.tsv"
}
```

**门比什么**（任一不等 / 缺失 ⇒ fail-closed）：

> 下表带 `scenario-*` 前缀的六项是 **M-1 的 sibling**：scenario 是人对夹具的**声明**，
> `rng_consumer_groups`（`FOLD_RULES`/`KNOWN_GROUPS`）是**真源**；门直接 import 同一份实现来比，
> 防止 `groupdiff --fold` 用的映射与 scenario 写的不是一回事（doc-vs-code 漂移）。这条对
> `-GateOnly` 与离线 `scenario` 模式（锁住已入库 `rng_trace_scenario.json`）都生效。

| 检查 | 口径 |
|---|---|
| `scenario-canon` / `scenario-fields` | scenario 自洽（map/seed/start_pose/monsters 齐备） |
| `scenario-trace-present` / `scenario-fold-code-valid` | scenario 有 `trace` 对象；代码侧 `FOLD_RULES` 自检自洽（已知组→已知组、源≠目标） |
| `scenario-fold-declared` / `scenario-fold-matches-code` | scenario 声明的 `trace.fold` 必须**显式**且**逐项等于**代码真源 `rng_consumer_groups.FOLD_RULES`（缺声明/漂移 ⇒ 红，不静默绿） |
| `scenario-groups-declared` / `scenario-groups-known` | scenario 声明的 `trace.groups_expected` 必须**显式**且每个组名都是账本 `KNOWN_GROUPS`（未知组 ⇒ 红） |
| `sides-distinct` | 两侧 side = 一 `csharp` 一 `rust` |
| `map-equal` / `seed-equal` / `pose-equal` | 两侧都等于 scenario |
| `monsters-equal:scenario-vs-{a,b}` / `a-vs-b` | **Monster 集合同口径**（`(name,x,y,dir)` 多重集 + 计数） |
| `mongen-equal:{a,b}-vs-fixture` / `a-vs-b` | **MonGen 集合同口径**：两侧 `mongen_rows` 逐行多重集都等于冻结件 `../fixtures/mongen-map<map>.tsv` |
| `stream-inject-ok:{a,b}` / `stream-seed-equal:{a,b}` / `stream-seed-equal:a-vs-b` | **制品级同种子**（给了 `--a-stream` / `--b-stream` 时）：两份 trace **流水头**的 `INJECT_OK <seed>`（C# `rng-hook` 与 Rust `crates/shared/src/rng_trace.rs` 都写）必须存在且 `== scenario.seed`。描述件里的 `seed` 只是**自报**，不算证据；缺该行 ⇒ 无法证明种子（fail-closed RED） |

退出码：`0` = GREEN（同场景，`groupdiff` 次序结论可采信）；`1` = RED（场景漂移**或 scenario 声明与代码真源漂移**，点名哪一项）；
`2` = PREREQ（缺件 / 坏 JSON，fail-closed）。

> **制品级同种子（新增）**：DoD① 要求「两侧 trace 必须来自同场景/同起点/**同种子**」。描述件由各侧自报，
> 所以门在两份流水在场时（`-SceneGroupDiff` 恒传；`-GateOnly` 给了 `-AStream`/`-BStream` 才核）还要核对
> **流水头**的 `INJECT_OK <seed>`。这样「描述件写 42、实际流水是别的种子」这种伪造会被拦在**拒比**而非
> 被误判成实现差异。未给流水时保持既有描述件级行为（向后兼容）。

**DoD① 的判定入口 = `-SceneGroupDiff`（本片收口）**：它把「场景门 → 组账本比较」串成**一条命令**，
门非 GREEN ⇒ **拒比**（不跑 `groupdiff`、绝不输出 `RNG_CONSUMER_GROUPDIFF` 结论，也不把它当绿），
门 GREEN 才跑 `groupdiff --fold --strict` 并原样传播其 rc/文本。`-GateOnly` / `-GroupDiffOnly` 语义不变，
其中 `-GroupDiffOnly` 是**跨场景对照用的裸比较**口径，**不构成 DoD① 判据**（不得单独用它下"实现等价"结论）。

---

## 3. R-6 folded 映射：`groupdiff --fold --strict`

**owner 裁决（2026-10-13）**：C# 的 `construct` 组（`BaseObject..ctor` / `AnimalObject..ctor` /
`MonsterObject..ctor` / `Monsters.*..ctor` / `PlayObject..ctor` 的构造期抽数）在 Rust 侧被**折叠进
刷怪路径**（`crates/world/src/mongen.rs` 的 `AnimalState::from_spawn_draws`）⇒ 接受为**登记建模差异**，
但**必须机判等价**：

> C# `construct` 折叠到 Rust `spawn` 后，**折叠计数与相对次序必须相等**；**只写白名单不校验不算达成**。

实现（`rng-trace-probe/rng_consumer_groups.py` + `rng_ledger_diff.py`）：

- `FOLD_RULES = {"construct": "spawn"}`（`validate()` 里做表自检：源/目标必须是已知组、源≠目标）；
  **且** scenario 声明的 `trace.fold` 必须与它逐项相等（门 `scenario-fold-matches-code` + 离线 `scenario` 模式机判）；
- `groupdiff --fold` 先把两侧组序列逐元素折叠，再在**折叠后的序列**上重算：
  - 组集合（`GROUP_SET_DIFF`）；
  - **折叠计数**（`FOLDED_COUNT_DIFF`：逐组 `a` / `b` 计数不等即红）；
  - 共享组相对次序（`SHARED_REL_ORDER_DIFF`）。
- `--strict` 下任一即 rc=1。**缺省（不传 `--fold`）行为与既有逐字节相同**——`construct` vs `spawn`
  仍如实报 `GROUP_SET_DIFF`（折叠是显式开关，不是全局豁免）。

**当前基线实拍**（现存唯一一对**不同场景**活栈 trace，仅作「折叠不是豁免」的对照；
C# `a7df7116…` / Rust `4963e9f6…`，见报告 §2）：

```
groupdiff … --strict            → VERDICT GROUP_SET_DIFF+SHARED_REL_ORDER_DIFF                     rc=1
groupdiff … --fold --strict     → VERDICT …+FOLDED_COUNT_DIFF（spawn 折叠后 a=146545 b=0）         rc=1
```

折叠后 C# `spawn` = `82840(construct) + 63705(spawn) = 146545`，Rust `spawn = 0` ⇒ 折叠计数不等 ⇒ 红。
**这正是「折叠后计数必须相等」的牙**：若 Rust 侧只有 `spawn` 而数量对不上，折叠不会掩盖缺口。

> 同场景下若两侧折叠计数与相对次序全等 ⇒ `groupdiff --fold --strict` 输出 `VERDICT IDENTICAL`
> （`FOLDED_COUNT_EQUAL`），这才算 R-6 达成。

---

## 4. `drop` 组判据与 ④「死亡掉落不落地」口径的兼容性（卡 DoD ④ / R-3）

两边比的是**不同的东西**，因此**既不冲突也不互相掩盖**：

| | 本卡（RNG 站点组） | ④ `openmir2-m3-drop-exp-align` |
|---|---|---|
| 对象 | **掉落点的 RNG 取数账本**：`MonGetRandomItems`（`WorldServer.MonGen.cs:440`）+ `GameItemSystem.GetUpgrade/UpgradeRandom*` | **死亡掉落的落地产物**：地面物 / `SM_ITEMSHOW(610)` / `RM_ITEMSHOW` |
| 判据 | `drop` 组两侧**取数账本** diff = 0（本卡：组集合 + 折叠计数 + 次序；④：逐次取数） | 落地**不出现**（oracle 已裁决死亡掉落不落地） |
| 数据源 | 两侧全场景抽数流水（`rng-stream.tsv`） | 掉落原始件 `drop_exp_raw.jsonl` + 场景纯度门 |

**oracle 口径（owner 2026-10-13 `aace16e0`/`8684d755` 裁决，已在 master `4521d451`/`aec…`）**：
`MapItem.cs:41` 唯一 `ItemId` 赋值点被注释 + `Envirnoment.cs:88-93` 首行 `if (ItemId == 0) return false`
⇒ **oracle 仍抽 RNG、只是不落地**。因此：

1. **本卡要求 `drop` 组两侧取数账本 diff=0**——它测的是「抽了多少次、抽的顺序」；
2. **④ 要求落地不出现**——它测的是「抽完不生成地面物 / 不发 `SM_ITEMSHOW(610)`」；
3. 二者**没有共享判据**：本卡**不**断言地面物存在；④ **不**断言 RNG 抽数。⇒ **同窗口、同图、同起点、
   同种子、串行**即可，**不会互相打架**（本卡**禁**沿用 ④ 的 pin 抑制：那会压掉 `monster_ai`）。

**同窗口编排建议（owner 定顺序）**：

```
① 起 C# 栈 → 本卡 csharp 相（含 <csharp>-scene.json + rng-stream.tsv）
② 起 Rust 栈 → 本卡 rust 相（-PeerSession <csharp 会话>）
③ run_rng_trace_pair.ps1 -SceneGroupDiff（**DoD① 单一入口**：场景门非 GREEN ⇒ 拒比；GREEN 才跑 groupdiff --fold --strict）
④ start_rust_stack -Stop → 同一路 ④ 100 杀（drop_exp_raw.jsonl + 纯度门）
⑤ 收工：写「窗口已释放」+ 十端口实拍
```

一轮窗口只跑一次起栈；①–⑤ 串行，**不并行起栈**（互相污染）。

---

## 5. 无栈自检（`-SelfCheckNoStack`，零端口）

```powershell
powershell -ExecutionPolicy Bypass -File run_rng_trace_pair.ps1 -SelfCheckNoStack
# 转录由脚本自己以 LF 落盘（避免 `> file` 的 CRLF 重定向）→ 证据件
powershell -ExecutionPolicy Bypass -File run_rng_trace_pair.ps1 -SelfCheckNoStack -SelfCheckOut <path.txt>
```

自检覆盖（27 项；全离线）：scenario 固定输入、MonGen 冻结件零行、场景门 selftest（21/21）、
场景门进程内正/负样本（同场景绿 / 怪集合漂移红）、账本 selftest（含 R-6 folded）、
folded 等价绿 / 折叠计数不等红 / 折叠次序不等红、缺件 PREREQ、LF 写出、
`scenario-fold-group-consistency-green`（已入库 scenario 与代码同口径）、
`scenario-fold-drift-red` / `scenario-groups-unknown-red`（改坏必红）、
`scenario-drift-samples-json-native`（三条 drift 样本由 JSON 字段改写构造、与源不同，
不再对原始文本做正则替换 —— M-1 sibling 收口；base 值牙内自解析，无跨块 `$sc` 依赖 —— O-1）、
`scenario-drift-tooth-not-vacuous`（空转样本 ⇒ 同一 fold 谓词为 false，证明上一条牙非恒真）、
**DoD① 单一入口 `-SceneGroupDiff` 四项**：
`scene-groupdiff-identical-green`（同场景 + 等价账本 ⇒ 门绿 + `VERDICT IDENTICAL` rc=0）、
`scene-groupdiff-scene-drift-refused`（改坏必红：怪集合漂移 ⇒ rc=1 **且拒比**，文本里无任何 `RNG_CONSUMER_GROUPDIFF` 结论行）、
`scene-groupdiff-missing-prereq-refused`（缺场景件 ⇒ rc=2 同样拒比）、
`scene-groupdiff-diff-red-propagates`（反向对照：门绿而账本确有不一致 ⇒ groupdiff 的 rc=1 与结论行必须**传播**，防「恒拒比」假绿）、
**制品级同种子四项（新增）**：`scene-gate-stream-seed-green`（两侧流水头 `INJECT_OK 42` ⇒ 门绿）、
`scene-gate-stream-seed-mismatch-red`（改坏必红：B 侧流水头 43、描述件仍自报 42 ⇒ rc=1 点名 `stream-seed-equal:b`）、
`scene-gate-stream-inject-missing-red`（改坏必红：B 侧流水头无 `INJECT_OK` ⇒ rc=1 点名 `stream-inject-ok:b`）、
`scene-groupdiff-stream-seed-refused`（改坏必红：制品级种子漂移下 `-SceneGroupDiff` 整链拒比：rc=1、`ranDiff=False`、无 `RNG_CONSUMER_GROUPDIFF`）、零端口。
real-run 分支在 `-Go` 未给时 **fail-closed exit 2**（不静默起栈）。

## 6. 命令（可复算）

```sh
cd mir2-rs/tests/parity/m3

# 工具自检
python -I rng-trace-probe/rng_ledger_diff.py selftest          # 32/32 GREEN（含 R-6 folded）
python -I rng-trace-scene/rng_scene_gate.py selftest           # 16/16 GREEN（含 scenario 声明 vs 代码）

# scenario 件自身（离线；锁住声明与代码真源同口径）
python -I rng-trace-scene/rng_scene_gate.py scenario --scenario rng-trace-scene/rng_trace_scenario.json

# 场景一致性前置门（离线；制品级同种子：给 -AStream/-BStream 时还核流水头 INJECT_OK）
powershell -ExecutionPolicy Bypass -File rng-trace-scene/run_rng_trace_pair.ps1 -GateOnly \
  -AScene <csharp-scene.json> -BScene <rust-scene.json> \
  -AStream <csharp-rng-stream.tsv> -BStream <rust-rng-stream.tsv>

# 组账本 diff（R-6 折叠）—— **裸比较口径**，不构成 DoD① 判据
powershell -ExecutionPolicy Bypass -File rng-trace-scene/run_rng_trace_pair.ps1 -GroupDiffOnly \
  -AStream <csharp-rng-stream.tsv> -BStream <rust-rng-stream.tsv>

# DoD① 单一入口（本片收口；推荐）：**先**场景门，非 GREEN ⇒ 拒比；GREEN 才跑 groupdiff --fold --strict
powershell -ExecutionPolicy Bypass -File rng-trace-scene/run_rng_trace_pair.ps1 -SceneGroupDiff \
  -AScene <csharp-scene.json> -BScene <rust-scene.json> \
  -AStream <csharp-rng-stream.tsv> -BStream <rust-rng-stream.tsv>
```

## 7. 窗口纪律

- 真机两相 trace 需要 **5000/7000/7100/7200 独占窗口**；起栈命令**禁接管道**（`| tee/tail/head/grep`
  会把车道挂死），一律重定向到文件。
- **未获 owner `-Go` / 窗口发放前不许起栈**；本夹具默认 real-run `exit 2`。
- 跑完 `start_rust_stack.ps1 -Stop` + 十端口实拍（只应余共享 3306）+ 在卡上写「窗口已释放」。

## 8. 未做 / 边界（如实登记）

- **真机两相 trace 未跑**：需窗口 + 两侧「场景描述件 + 全场景抽数流水」驱动接线（承接卡，本片外）。
- **Rust 侧 `monster_ai`/`spawn`/`drop` 站点在含怪场景的实跑读数**尚未取得 ⇒ 基线 §2.1 的 Rust 侧
  「0」是窗口/场景缺，不是代码没写（`crates/world/src/monai.rs` / `mongen.rs`、`crates/formula/src/drop.rs` 已在 master）。
- `merchant` / `npc_ai` / `revive` 组仍缺 Rust 实现（R-4/R-7），本卡不覆盖，随各自系统卡收拢。
