# M3 RNG 场景驱动器（`rng-scene/`）—— 卡 `openmir2-m3-rng-scene-driver`（R3 承接卡）

> 来源：`docs/移植计划重审-20261015.md` §1-R3（本报告新立卡）+ §2-4（「同序列对齐键 + 断号/缺件即红」
> 通用规约）+ §5-风险 5（「W9 RNG trace 窗到了但 rng-scene-driver 未实现」）。
> 规格来源：`rng-trace-scene/DRIVER-CONTRACT.md`（owner thread 条目 `222bffe3` / `4ca2854c`）。
> 写集：`mir2-rs/tests/parity/**` + `src/BotSrv/**`（T-59 式白名单）。**零** `crates/**`、`bins/**`；
> 未起栈、未占端口。
> **判级 = R**（语义 = RNG 抽数序列；按 `AGENTS.md`「判据分级」policy）：离线自检（严格账本校验器 / 编排骨架 /
> BotSrv 纯函数）属纯逻辑，可离线自证等价；但**本卡判据本体 = 两侧真机账本逐次对拍** ⇒ **必须真机全量对拍**
> （占 W9 独占窗口）。**「真机两侧各跑一次落账本」= 显式未验项**（见 §4 / §5）；未跑之前本卡不得 `closed`。

## 0. 一句话

两侧『场景描述件 `<side>-scene.json` + 全场景抽数流水 `<side>-rng-stream.tsv`』的**生产者**此前
不存在（`DRIVER-CONTRACT.md` §0/§7，其写集在 `crates/world` + `bins/game-svr` + `src`）。本片把
它在**本卡写集内**能落的部分落地：一个**严格账本校验器 + 编排骨架**，把「流水形状本身是否可信」
从**静默通道**钉成 fail-closed，其余（两侧真机各跑一次）登记为 **W9 窗项**。

## 1. 为什么需要「严格账本校验器」（新增的牙）

既有比较器 `rng-trace-probe/rng_ledger_diff.py` 的 `parse_trace` / `rng_consumer_groups.iter_sites`
对下列**形状**问题要么静默跳过、要么完全不看 ⇒ 「组集合相等」会变成**假绿**：

| 通道 | 既有行为 | 本驱动器（`rng_trace_diff.py`）|
| --- | --- | --- |
| **seq 断号 / 重复** | **完全不看 seq**（只作 key） | `seq-gap` / `seq-dupe` ⇒ RED（点名缺失值 / 首个重复位） |
| 列数 ≠ 5 | `iter_sites` 计入 `rows_dropped`（只在被调用方的 `--strict` 里落红） | `row-shape` ⇒ RED（逐行点名） |
| 缺 site 列（4 列；`MIR2_RNG_SITE` 未开） | 归到空站 / `other` | `site-missing` ⇒ RED |
| 头 `INJECT_OK <seed>` 缺失/异值 | 只有场景门（给两份流水时）核 | `header-inject-missing` / `header-seed-mismatch` ⇒ RED |
| 数据行 = 0 | 退化为空 ⇒ 上层 PREREQ | `PREREQ`（rc=2，fail-closed 不判绿） |

**seq 从 1 连续的根据**：两侧生产者同形 —— 头行写死 `1\tINJECT_OK\t<seed>\t0`，其后每条取数 `seq++`
（C# `rng-hook/StartupHook.cs` 的 `_seq++` 与写盘同在 `lock (Sync)` 里；Rust `crates/shared/src/rng_trace.rs`
的 `SEQ.fetch_add` 是原子）⇒ 主流水数据行 seq 必然 **1..N 连续**。断号只可能来自「写盘被打断 /
行被截断 / 两个生产者拼错」⇒ 必须红。

## 2. 交付物

| 文件 | 作用 |
| --- | --- |
| `rng_trace_diff.py` | 严格账本校验器（`check`）+ 单一比较入口（`diff`：先严格校验两侧，**任一不 GREEN ⇒ 拒比**，否则把原始流水交给既有 `groupdiff --fold --strict`）+ `selftest`（11/11，含 10 条改坏必红） |
| `run_rng_scene_pair.ps1` | 驱动器编排骨架：`-SelfCheckNoStack`（23/23）/ `-TraceCheckOnly` / `-GateOnly` / `-SceneGroupDiff`（**委托**既有 `rng-trace-scene/run_rng_trace_pair.ps1`，不复制门→比逻辑）/ real-run fail-closed |
| `rng_scene_scenario.json` | 驱动场景（canon `m3-rng-scene-driver-v1`）：固定图/怪/起点/种子 + 固定动作序列 + 注入口；`scene_source` 指向 `rng-trace-scene/rng_trace_scenario.json`（**唯一真源**，自检机判逐字段相等） |
| `../src/BotSrv/SceneSeqSelfCheck.cs` + `MShare` 纯函数 | 驱动侧 BotSrv 支持（**T-59** 登记，默认关）：固定动作序列解析 / 固定怪 key / 场景描述件行组装；`--selftest-sceneseq` 无栈自检（23/23） |

## 3. 命令（可复算）

```sh
cd mir2-rs/tests/parity/m3

# 工具自检
python -I rng-scene/rng_trace_diff.py selftest                       # 11/11 GREEN
powershell -ExecutionPolicy Bypass -File rng-scene/run_rng_scene_pair.ps1 -SelfCheckNoStack   # 23/23 GREEN

# 只严格校验一侧流水（离线）
powershell -ExecutionPolicy Bypass -File rng-scene/run_rng_scene_pair.ps1 -TraceCheckOnly \
  -Side csharp -AStream <csharp-rng-stream.tsv> -Seed 42

# 单一比较入口（先严格校验两侧；任一形状不可信 ⇒ 拒比）
python -I rng-scene/rng_trace_diff.py diff <csharp.tsv> <rust.tsv> \
  --a-side csharp --b-side rust --seed 42 --scenario rng-trace-scene/rng_trace_scenario.json

# 委托既有 runner（门 / DoD① 单一入口）
powershell -ExecutionPolicy Bypass -File rng-scene/run_rng_scene_pair.ps1 -GateOnly \
  -AScene <csharp-scene.json> -BScene <rust-scene.json> -AStream <csharp.tsv> -BStream <rust.tsv>
powershell -ExecutionPolicy Bypass -File rng-scene/run_rng_scene_pair.ps1 -SceneGroupDiff \
  -AScene <csharp-scene.json> -BScene <rust-scene.json> -AStream <csharp.tsv> -BStream <rust.tsv>
```

## 4. 真机两相（**W9 窗项：显式未验项，未跑**）

两侧各跑一次「固定图 0137 + 稻草人 @ (18,20) dir=0 ×1 + 固定起点 (17,20) dir=2 + 种子 42」，
落 `<side>-scene.json` + `<side>-rng-stream.tsv`：

```
① C#: start-csharp-hook.ps1 -Seed 42 -LogDir <会话>（写 csharp-rng-stream.tsv，头 INJECT_OK 42 + site）
        → 固定场景（-SceneSequence，默认关）→ dump csharp-scene.json
② Rust: start_rust_stack.ps1 -RngLog（写 rust-rng-stream.tsv）→ 同场景 → dump rust-scene.json
③ rng-scene/run_rng_scene_pair.ps1 -SceneGroupDiff（门非 GREEN 拒比；GREEN 才 groupdiff --fold --strict）
```

- 需 5000/7000/7100/7200 **独占窗口**；起栈命令**禁接管道**。
- **未获 owner `-Go` 一律 fail-closed**（exit 2），绝不静默起栈。

## 5. 未做 / 边界（如实登记）

- **两侧真机各跑一次（W9）未做**：需窗口 + 两侧 `spawn/dump` 接线（`crates/world` + `bins/game-svr`），
  在承接卡写集外。`run_rng_scene_pair.ps1 -Go -PinScene` 现正确 fail-closed（`PREREQ_MISSING` rc=2）。
- **BotSrv 固定场景序列的 real wiring 未接**：本期只落**纯函数 + 默认关的静态位**（T-59），
  未接入 `RobotPlayer` 主循环（`g_boSceneSequence` 默认 `false`，行为零变化）。
- **`fixture_chain_gate.py` 只登记了 `rng_trace_diff.py selftest`**（py、纯离线、零端口）。
  **未**把 `run_rng_scene_pair.ps1 -SelfCheckNoStack` 登记进共享管线 —— 它会触发 `dotnet build`
  （比锚门重、且缺 dotnet 的环境会恒 RED）。以「本工具 selftest 常驻 + 派 runner 的手工/窗口项」
  折中；如 owner 要在链门常跑全套，另立一行登记。
- **`merchant`/`npc_ai`/`revive` 组**仍缺 Rust 实现（R-4/R-7），随各自系统卡收拢，本卡不覆盖。
