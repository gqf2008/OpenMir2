# M3 RNG 场景两相 trace「承接卡」规格 —— `openmir2-m3-rng-scene-driver`

> **本文件是什么**：把 owner 在卡 `openmir2-m3-rng-trace-scene-groups` 上给出的承接卡规格
> 落成**仓内文件**（依据 `AGENTS.md` §5：「面向人的报告作为文件进仓库，不要只留在 thread body」）。
> 来源条目（walgit，verified）：
> - `222bffe3011c1adeed6bffdb6ccfedf82c791ca5` 的 `missing_driver`（卡名 / 一句话 DoD / 接口 / 写集）；
> - `4ca2854c0b44cecf9027f6ff83c48486dba73093` 的 `driver_card_spec_delta`（**宿主 AI 模式**这一维度）。
>
> 本文件**不改**任何门的阈值、不新增判据口径，只是把上述规格转写入库，避免承接卡落地时
> 还要回翻 thread / 现场误配（尤其避免浪费一次 ④ 与本卡共用的**独占窗口**）。

## 0. 一句话

两侧『场景描述件 `<side>-scene.json`（canon `m3-rng-scene-descriptor-v1`）+ 全场景抽数流水
`<side>-rng-stream.tsv`（5 列；头 `INJECT_OK <seed>`）』的**生产者**目前不存在；
它在 `crates/world/**` + `bins/game-svr/**` + `src/**`（C# 侧 spawn/dump 需 `T-*` 登记）里，
超出本卡 10:35 派单的写集禁令 ⇒ 需要一张独立承接卡把它做出来。

## 1. 为什么本卡与 ④ 共用这一件

| | 本卡 `openmir2-m3-rng-trace-scene-groups` | ④ `openmir2-m3-drop-exp-align` |
| --- | --- | --- |
| 图 / 种子 / 起点 | `0137` / 42 / `(17,20) dir=2` | 同左（`check_window_batch.py` 机判相等） |
| 也要的制品 | `<side>-scene.json` + `<side>-rng-stream.tsv` | 同左（外加 `drop_exp_raw.jsonl`） |
| 宿主模式 | **AI 开**（要 `monster_ai`） | **AI 抑制**（`purity.wallclock_policy=pinned`，`monai==0`） |

⇒ 两卡要的是**同一件缺件**（两侧场景驱动）。**建议合成一张卡**，别两卡各写一套 pin / spawn。
两卡在同一独占窗口**串行**跑（`rng-trace-scene/README.md` §4 ①–⑤）。

## 2. 交付物（两侧各一份，逐字节同口径）

| 制品 | canon / 形态 | 说明 |
| --- | --- | --- |
| `<side>-scene.json` | `m3-rng-scene-descriptor-v1` | `map` / `seed` / `start_pose` / `mongen_rows` / `monsters`（**运行时实际集合**）/ 指向流水 |
| `<side>-rng-stream.tsv` | 5 列 `seq kind max result site` | 全场景抽数流水；**头行 `INJECT_OK <seed>`**（制品级同种子证据） |
| C# 侧流水 | `MIR2_RNG_LOG`（m3/rng-hook + `MIR2_RNG_SITE=1`） | 现有 hook 已支持 |
| Rust 侧流水 | `<rust stack log dir>/rng-stream.tsv`（`start_rust_stack.ps1 -RngLog`） | 见 `crates/shared/src/rng_trace.rs` |
| C# 侧 spawn/dump | 0137 图上**运行时 pin 出固定怪** + dump 实际 monster 集合 | 地图自身 MonGen 0 行（`fixtures/mongen-map0137.tsv`） ⇒ 不能靠数据文件刷怪 |

## 3. 场景（固定输入，勿改）

| 项 | 值 | 依据 |
| --- | --- | --- |
| map | `0137`（竞技场；MonGen 0 行、Guard/Npc/MapEvent/MapQuest 零引用） | `rng_trace_scenario.json` `isolation`；`fixtures/mongen-map0137.tsv` |
| seed | `42`（两侧同值，且**流水头**须 `INJECT_OK 42`） | `rng_trace_scenario.json` `seed`；门 `rng_scene_gate.py` |
| start_pose | `(17, 20) dir=2` | 与 ④ / ②③ attack 夹具同起点 |
| monster | `稻草人 @ (18,20) dir=0 × 1`（pin） | `rng_trace_scenario.json` `monsters` |
| mongen_rows | 逐行多重集 == `fixtures/mongen-map0137.tsv`（**0 行**） | 门 `rng_scene_gate.py` |

## 4. 宿主 AI 模式（承接卡必须**显式**给出的维度）

**这是上一版规格没写清、会以窗口期 ④ 纯度门 RED 的形式撞出来的一件。**

- 本卡要 `monster_ai > 0`：`rng_trace_scenario.json` 的
  `isolation.wallclock_ai_expected` + `trace.groups_expected == [monster_ai, spawn, drop]`。
- ④ 要 `monai == 0`：`drop-exp/drop_exp_scenario.json` `purity.wallclock_policy = "pinned"`；
  `drop-exp/drop_exp_consumers.tsv` 的 rust `wallclock` 行 = `crates/world/src/monai\.rs:`，`max=0`
  （「pin 抑制必须使其为 0」）。
- master 生产宿主**无条件**开怪 AI：`mir2-rs/bins/game-svr/src/main.rs`（调用
  `game_svr::mongen::enable_production_monster_ai(&mut world)`；**故意不写行号锚**：main.rs 是
  ④-D / client-e2e / ⑧-10 / ⑦ 等多车道共享的增长文件，行号锚实测已漂 6 次
  `5229 → 5440 → 5488 → 5598 → 5665 → 5795`，全是别的车道插代码推平 ⇒ 改由「符号存在且唯一」
  机判保鲜，见 §6）
  → `mongen.rs:61 pub fn enable_production_monster_ai` → `world.enable_monster_ai()`
  ⇒ 只要场上有活怪，`World::tick` 就按槽位驱动 `crates/world/src/monai.rs:3429 monster_ai_tick`；
  无目标且 walk 到点时 `monai.rs:215 wondering` **至少抽 1 次 `Random(20)`**
  （oracle 锚 `src/M2Server/Actor/AnimalObject.cs:425`）。

⇒ **承接卡必须提供【可切换的宿主 AI 模式】**：本卡跑「AI 开」（默认生产宿主即可），
④ 跑「`monai==0` 的 pin 抑制模式」（AI 关，或能证明含怪场景下 `monai` 恒 0）。
这是夹具/宿主口径，**不改任何门的阈值**。

## 5. 写集（§0b：与其它卡切分）

- `crates/world/**`（Rust pin/spawn + 场景描述件落盘）
- `bins/game-svr/**`（参数 / 出口 / 宿主 AI 模式开关）
- `src/**`（C# 侧 GM/脚本 spawn + 描述件 dump）——**按 `T-*` 口径登记**后动
- 夹具侧只读消费本卡的 `rng-trace-scene/**`（不要改本卡已审定的门/账本）

## 6. 机判验收（承接卡落地后，本卡窗口判据）

```sh
cd mir2-rs/tests/parity/m3
# ① 场景一致性前置门（制品级同种子：还核流水头 INJECT_OK）
powershell -ExecutionPolicy Bypass -File rng-trace-scene/run_rng_trace_pair.ps1 -GateOnly \
  -AScene <csharp-scene.json> -BScene <rust-scene.json> \
  -AStream <csharp-rng-stream.tsv> -BStream <rust-rng-stream.tsv>
# ② DoD① 单一入口：门非 GREEN ⇒ 拒比；GREEN 才跑 groupdiff --fold --strict
powershell -ExecutionPolicy Bypass -File rng-trace-scene/run_rng_trace_pair.ps1 -SceneGroupDiff \
  -AScene <csharp-scene.json> -BScene <rust-scene.json> \
  -AStream <csharp-rng-stream.tsv> -BStream <rust-rng-stream.tsv>   # 期望 VERDICT IDENTICAL
```

- 判据 = `groupdiff --fold --strict` 的 `GROUP_SET_DIFF` / `SHARED_REL_ORDER_DIFF` /
  `FOLDED_COUNT_DIFF` / `MISSING_EXPECTED_GROUPS` / `MIN_GROUP_DRAWS` / `ROW_COVERAGE_DRIFT` 全无。
- **改坏必红**（承接卡须自带）：描述件 monster 集合漂移 / 流水头种子不符 / 删一组 ⇒ 门或 `groupdiff` 必红。
- R-6 折叠口径（owner 2026-10-13 裁决）：`rng_consumer_groups.FOLD_RULES = {"construct": "spawn"}`
  （定义 = `rng-trace-probe/rng_consumer_groups.py:57`；说明/站点 = `rng-trace-probe/consumer_groups.tsv:38`（折叠理由）、`:57`（Rust `spawn` 行））；
  折叠后**计数与相对次序必须相等**。
- **表侧补登记（2026-10-13 窗口前置审计）**：宿主 `bins/game-svr/src/mongen.rs`
  （`create_monster_oracle` / `draw_monster_construct`）也抽同一批构造期随机数 ⇒ 同归 `spawn`
  （`consumer_groups.tsv` 表末补一行）。固定场景（图 0137，MonGen 0 行）里它是**唯一**的
  `spawn` 来源；漏归类会被算成 `other` ⇒ `--strict` 的 `GROUP_SET_DIFF` + DoD② 的
  `MISSING_EXPECTED_GROUPS(spawn)` 双红（会在共用窗口里空转一轮）。机判 = `python -I rng-trace-probe/rng_consumer_groups.py selftest`（含删行必红）。
- **本文档自身的源码锚点保鲜（2026-10-13 第二次窗口前置审计；2026-10-14 根因收口）**：§4 引用的
  源码锚点是**承重锚点**（宿主 AI 调用点 / 世界层 tick / oracle Wondering）。两类机判
  （`python -I rng-trace-scene/check_anchors.py selftest`）：
  ① **行号锚**：文档写的行号 == 该符号在当前源码里的**首个**匹配行（缺失 / 重复 / 漂移都 fail-closed
     点名）——用于**稳定文件**（`mongen.rs` / `monai.rs` / oracle `AnimalObject.cs` / 本卡自己的
     `rng_consumer_groups.py` / `consumer_groups.tsv`）；
  ② **符号唯一性锚**（`symbol_unique=True`）：**共享增长文件**（`bins/game-svr/src/main.rs`——行号锚
     实测已漂 6 次 `5229 → 5440 → 5488 → 5598 → 5665 → 5795`，每次都是别的车道插代码推平）
     **不做行号锚**：文档只引用文件路径、不得再写 `:line`，机判改为「该符号在文件里**恰好出现一次**」
     （0 次 ⇒ `ANCHOR_SYMBOL_MISSING`；>1 次 ⇒ `ANCHOR_AMBIGUOUS`）。**判据不降级**：宿主 AI 调用点
     被删 / 被复制仍必红，只是把不稳定的行号换成稳定的唯一性，从根上消掉「别的车道合并 ⇒ 本卡假 RED」
     这一族。已接入 `run_rng_trace_pair.ps1 -SelfCheckNoStack`（`anchors-freshness-selftest`）。

  > **合并订正（2026-10-14，本片与链门车道的同一处修法并集）**：链门车道 `42d421188` 的收口是把
  > 该行号**再钉一次**（`5665 → 5795`）——那是本片要消除的**成因**（第 6 次漂移正是它造成的假 RED）。
  > 本片合并时取**根因**侧：main.rs 不再做行号锚（改符号唯一性），链门侧新增的 fmt 证据门与
  > `.py` 登记下界（`≥23`）原样保留（不在本卡写集，未动）。行号锚仍保留于**稳定文件**。
- **锚点覆盖面收口（2026-10-14 reviewer-5 M-1 / reviewer-6 M-1+M-2）**：旧口径有两处静默通道 ——
  ① 覆盖判定用「结尾 basename 匹配」⇒ **别的目录下的同名文件**会被误判 COVERED（既非
  `UNGUARDED_ANCHOR` 也不 `ANCHOR_DRIFT`）；② 同行**裸续引用**（同一条引用后面再跟一个冒号行号，
  本 §6 的 R-6 站点写法即此）完全没有被解析。现机判（`check_anchors.py`）：带目录引用必须与锚点的
  `literal`/`path` **全等**（裸 basename 仅限显式 `basename_ok`）、未识别扩展名走
  `UNKNOWN_REF_EXT` fail-closed、裸续引用归到同行前一个引用并按锚点 `also` 校验**行内容**
  （未登记 / 越界 / 内容不符都判 RED）；`selftest` 17/17（含 16 条改坏必红；裸续引用三条分支
  `BARE_REF_UNGUARDED`/`BARE_REF_FILE_MISSING`/`BARE_REF_OUT_OF_RANGE` 亦各有活用例）。

## 7. 当前状态（阻塞）

- 承接卡**尚未开**（owner 动作；作者不自建卡）⇒ 两侧驱动未接线。
- `run_rng_trace_pair.ps1 -Go -PinScene` 目前**正确 fail-closed**：
  `PREREQ_MISSING: 两侧「场景描述件 + 全场景抽数流水」驱动尚未接线` ⇒ `exit 2`（不静默起栈）。
- 承接卡落地 + owner 发放独占窗口后，按 §6 跑两相；与 ④ 真机 100 杀**同批次串行**，
  跑完 `start_rust_stack.ps1 -Stop` + 十端口实拍 + 卡上写「窗口已释放」。
