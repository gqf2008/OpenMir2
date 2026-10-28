# 武器磨损前 RNG 分歧分诊（run9，2026-10-29）

卡：`openmir2-rng-divergence-before-weapon-wear`；**R，`window_required=true`**。

证据根：`E:/tmp/window/run9/equip_struck_save/`（C#/Rust 各一份
`rng-stream.tsv`，均带 `INJECT_OK` + 同一 `SEED_PROBE`）；run9 report 的帧面已是
38/38/38 对齐，唯一 DB 差为 `characters_item.Dura 3995/3994`。

## 结论

**不是 Rust 攻击路径少抽/多抽，也不是探针记录注入差。**

两侧攻击局部序列完全同形，都是：

```text
GetAttackPower_Random(power+1)  →  GetAttackPower_Random(10)  →  AttackSuccess_Random(5)
```

C# 的这三次抽在全局日志中分别位于第 105027 / 105032 / 105129 条，中间夹了 **100 条
其他站点的随机抽**（92×`AnimalObject.Wondering:427`、6×`:432`、2×`:434`）；Rust 的
对应三次是第 66350 / 66351 / 66352 条，**连续、零夹层**。C# 的 `Random` 方法本身不加锁
（`src/OpenMir2/RandomNumber.cs:83-100`），而 `ProcessMonsters` 由多个
`new Thread(...)` 并行驱动（`src/GameSrv/Word/WorldServer.MonGen.cs:145-160`）。因此全局
`rng-stream.tsv` 是**多线程共享 RNG 的交错日志**，不是单线程确定性序列；同一个攻击内的
A/B/C 抽数会被其他线程的 Wondering 插入，攻击末次 `Random(5)` 的值不可作为跨实现逐值契约。

run9 的 `SEED_PROBE` 两侧完全相同：

```text
1434747710 302596119 269548474 1122627734 361709742 563913476 1555655117 1101493307
```

这证明种子相同；差异来自**消费交错/时序**，不是播种或 RNG 算法不同。

**处置**：本卡不修改 `src/**`（没有可安全修复的攻击调用点；把 Rust 变成多线程/复刻
C# 线程调度会违反确定性硬约束）。R 级收口需 owner 裁决“共享 RNG 的跨线程交错是否属于
非契约”，再用当前 master 的 n13 + `--rng-log` 复跑确认；不能把该差用 `ignore_columns`
或“两边都随机”直接洗绿。

## A1：站点等价表与 first divergence

站点表沿用 `mir2-rs/tests/parity/m3/rng-trace-probe/consumer_groups.tsv`（不另抄）：

| 组 | C# 站点（示例） | Rust 站点（示例） |
|---|---|---|
| construct | `BaseObject..ctor@BaseObject.cs:438` / `AnimalObject..ctor@AnimalObject.cs:75` / `PlayObject..ctor@PlayObject.cs:1046` | `crates/world/src/entity.rs:174-178` / `crates/world/src/ctor_fold.rs:545` |
| startup | `ServerConf.LoadConfig@ServerConf.cs:114` | `bins/game-svr/src/lib.rs:22164` |
| monster_ai | `AnimalObject.Wondering@AnimalObject.cs:427/432/434` | `crates/world/src/monai.rs:237/240/241` |
| merchant | `Merchant.Run@Merchant.cs:132/138` | `crates/world/src/merchant.rs:1599/1609` |
| attack | `PlayObject.GetAttackPower@PlayObject.cs:2929/2934` + `AttackSuccess@PlayObject.Attack.cs:223` | `crates/world/src/combat.rs:109/111/1152` |
| hit/damage | `BaseObject.GetHitStruckDamage@BaseObject.Base.cs:792/793` | `crates/world/src/combat.rs:856/858` |
| drop | `MonGetRandomItems@WorldServer.MonGen.cs:451` | `crates/formula/src/drop.rs:591` |

按 seed 后的**原始 draw index** 对齐：两侧第 1–4 抽值相同（1002/281/376/1568），
**first divergence = 第 5 抽**：

| 侧 | site | max | result |
|---|---|---:|---:|
| C# | `M2Server.Player.PlayObject..ctor@PlayObject.cs:1046` | 999999 | 168434 |
| Rust | `crates/world/src/entity.rs:174` | 1500 | 252 |

C# 的 `PlayObject` ctor 第 5 抽夹在怪物构造/刷怪抽之间；Rust 的同一
`ctor_fold.rs:545`（max=999999）出现在第 **53600** 抽。这是“登录/玩家构造”相对于
怪物线程/刷怪的时序差，不是攻击路径抽数差。由于 C# 共享 RNG 被多线程调用，原始全局
index 不是稳定契约。

分析器输出（同一结论）：
`E:/tmp/dev-2/rngdiv-run9-analysis-fixed.json` 的 `rng_alignment.first_value_divergence`
与 `player_ctor_draw`。

## A2：攻击窗口逐调用表

| 步骤 | C# 全局 draw / 值 | Rust 全局 draw / 值 | 语义 |
|---|---|---|---|
| A `GetAttackPower(power+1)` | 105027 / max5=3 | 66350 / max5=0 | 攻击力第一抽 |
| B `GetAttackPower(10)` | 105032 / max10=8 | 66351 / max10=6 | Luck 门第二抽 |
| C `AttackSuccess(max5)` | 105129 / max5=3 | 66352 / max5=4 | 武器磨损 `Random(5)` |

- **玩家独占 canonical 的调用次数与次序相同**：A→B→C，各一次；没有多插/漏抽
  （`player_attack_counts` 两侧 A/B/C 各 1）。共享 canonical（`atk_power_base`/`hit_gate`/
  `hit_struck_damage`）在 C# 里无法归属 actor，只在 `all_attack_counts` 作背景。
- C# A/B/C 之间夹层共 **100 抽**（A→B 4 条 Wondering，B→C 96 条 Wondering）；
  Rust A/B/C **连续 0 夹层**（单线程）。
- C# `RandomNumber.Random(int)` 直接调 `System.Random.Next`，没有加锁
  （`src/OpenMir2/RandomNumber.cs:83-100`）；`ProcessMonsters` 用多个后台 Thread
  （`src/GameSrv/Word/WorldServer.MonGen.cs:145-160`）⇒ 全局流是并发交错。
- 两侧种子相同（`SEED_PROBE` 逐值相同），攻击局部形状完全相同
  `[atk_power_a, atk_power_b, attack_success]`；C# 末值 3 / Rust 末值 4 是**流位置差**，
  不是算法差。
- 因此 **不能把 Dura 3995/3994 归为 Rust 少抽/多抽**；也没有安全的 `src/**` 修复点。
  复刻 C# 线程交错会违反 Rust 单线程确定性约束，并仍不可复现 C# 的墙钟调度。

## A3：分析器与负例

`tools/window/equip_struck_dura_triage.py` 已扩展（仍走既有注册项）：
`--root <run> --json ...` 的 `rng_alignment` 现在输出：

- `seed_probe` / `seed_probe_equal`（两侧 probe 行都必须存在、非空且 token 全等才为 true）；
- `first_value_divergence`（含两侧 site/max/result）；
- `player_ctor_draw`（C# 第5 / Rust 第53600）；
- `attack_windows_total`：两侧各自唯一的 `atk_power_a` 起点数；缺失或 ≠1 即
  `UNDECIDABLE_ATTACK_WINDOW`，不静默取第一个窗口；
- `player_attack_counts`：全流**玩家独占** canonical（A/B/C）的抽数，与分类同口径；
- `all_attack_counts`：全流**所有**攻击族 canonical 的抽数，作背景（含怪物攻击的
  `atk_power_base`/`hit_gate`/`hit_struck_damage` 等），**不可跨侧直接比**；
- `attack_local`：**shape = A→首个 Success 之间玩家独占 canonical 的有序序列（含重复）**、
  `attack_counts`、`maxes`、`foreign_attack_draws_between_a_success` /
  `foreign_attack_sites`（窗口内共享 canonical 的外来抽，只记录、不参与分类）、
  `trailing_success_after_window`（首个 Success 之后、下一个 `atk_power_a` 之前的 Success
  抽数，毒门/多抽 C 的判别面）、`trailing_foreign_attack_draws` 与夹层站点计数；
- `classification`：`ATTACK_LOCAL_SHAPE_EQUAL_GLOBAL_STREAM_INTERLEAVED` /
  `ATTACK_CALL_SHAPE_DIVERGENCE` / `ATTACK_CALL_ARG_DIVERGENCE` / `UNDECIDABLE_ATTACK_WINDOW`。

> 2026-10-29 返工1（reviewer-20 F1–F5）：旧实现先搜到一个 A、一个 B、一个 C 再拼固定三元组，
> 所以 `ATTACK_CALL_SHAPE_DIVERGENCE` 不可达——一侧多抽一次 B/C 仍报 EQUAL。现在 shape 是
> 有序序列（含重复），多/少抽会直接改变 shape；首个 Success 之后的 Success 单列比较；
> 起点数、攻击计数、SEED_PROBE 是否存在与 token 是否全等也都进入判据。
>
> 2026-10-29 返工2（reviewer-20 F1–F4）：shape 一度收进**全部**攻击族 canonical，导致 C# 怪物
> 线程插一条 `atk_power_base`/`hit_gate` 就被当成本玩家攻击的 shape（合成
> `[A, atk_power_base, B, C]` vs `[A,B,C]` 误报 DIVERGENCE）。账本没有线程/actor 列，
> 共享 canonical **无法归属**，因此现在 shape 只取玩家独占的 A/B/C；外来攻击族抽单列
> foreign、不改分类、不报 DIVERGENCE。`trailing` 扫描改为到「下一个 `atk_power_a`」为止，
> 外来行不再截断扫描。计数按 `player_attack_counts` / `all_attack_counts` 拆开。

`--check` 扩到 **27/27**。新增/改造的常驻牙（每条都做过“删掉即 FAIL”的变异复验）：

| 牙 | 断言 |
|---|---|
| 缺 B（少抽） | `[A,C]` vs `[A,B,C]` ⇒ `ATTACK_CALL_SHAPE_DIVERGENCE` |
| 多抽 B | `[A,B,B,C]` vs `[A,B,C]` ⇒ `ATTACK_CALL_SHAPE_DIVERGENCE`（旧实现报 EQUAL） |
| 多抽 C | 首个 Success 之后仍有 Success ⇒ `trailing_success_after_window` 1 vs 0 ⇒ DIVERGENCE |
| C# 外来 `atk_power_base` | 窗口内插入 ⇒ 仍 EQUAL，`foreign_attack_draws_between_a_success` C#=1/Rust=0 |
| C# 外来 `hit_struck_damage` | 同上，`foreign_attack_sites={"hit_struck_damage":1}` |
| 计数拆分 | `player_attack_counts` 不含 `atk_power_base`；`all_attack_counts` 才含 |
| 取数上界不同 | shape 相同、B 的 max 10→9 ⇒ `ATTACK_CALL_ARG_DIVERGENCE` |
| 外来行不截断 trailing | `[A,B,C, atk_power_base, C]` vs `[A,B,C]` ⇒ trailing C#=1 ⇒ DIVERGENCE |
| 缺终点 | 只有 A/B ⇒ `UNDECIDABLE_ATTACK_WINDOW` 且 reason 点名 `attack_success` |
| 起点不唯一 | 两个 A ⇒ UNDECIDABLE + `attack_windows_total={csharp:1,rust:2}` |
| 两套 ctor 站点 | 合成件里 C#/Rust 构造抽各一行 ⇒ `player_ctor_draw.index` = 1 / 2 |
| 无 probe 行 | 两侧都缺 ⇒ `seed_probe_equal=false`（`None==None` 不得为真） |
| 只有字样 | 头行只是出现 `SEED_PROBE` 字样、无以它开头的行 ⇒ 账本不合格 |
| analyze 接线 | 合成 root 两侧合法账本 ⇒ `rng_alignment` 真挂上；一侧坏头 ⇒ UNDECIDABLE 带 reason |

原 Dura/RNG 账本负例（无攻击、同值、run6 形态、快照不一致、重复 642、比较器非恒等、
账本缺失/坏头、首差 index、夹层计数、SEED_PROBE 不同）全部保留。真实 run9 输出：
`E:/tmp/dev-2/rngdiv-run9-analysis-v3.json`（`status=DIAGNOSED`，classification 同下，
`attack_windows_total=1/1`，`player_attack_counts` 两侧 A/B/C 各 1、窗口 foreign 0/0、
trailing 0/0）。注意 run9 的 `all_attack_counts` 两侧差别很大（C# `atk_power_base:16`、
`hit_gate:16`、`hit_struck_damage:18`；Rust 无前三者、`hit_struck_damage:206`），
那是怪物/共享站点的背景，**不作为攻击路径判据**。

## A4：R 窗口收口

未在本卡运行真机（本卡只读诊断），也没有修改实现。当前 master 的 n13 复跑应：

1. 用 `--rng-log` 采集两侧账本，确认 `SEED_PROBE` 相同、`attack_windows_total` 两侧为 1、
   玩家独占 shape 仍为 `[atk_power_a, atk_power_b, attack_success]`、`player_attack_counts`
   A/B/C 各 1；窗口内若出现 `foreign_attack_draws_between_a_success > 0`（C# 怪物线程插入），
   按 foreign 记录，不据此判 DIVERGENCE；
2. 观察 C# 同一攻击内是否仍有 Wondering 夹层、Rust 是否为 0；必要时做两次同 seed C# 复跑，
   若 C# Dura 自身在 run-to-run 变化，则作为“共享 RNG 跨线程非契约”的直接证据；
3. Dura 相等不是本实现可单方面保证的量；若仍差，须 owner/coordinator 给出“并发 RNG 交错
   非契约”的 R 级等价裁决，或另立全局 RNG 并发模型卡；不得用 ignore/mask 洗绿；
4. 帧面仍须 38/38/38，Dura 差不得冒充 PASS。

本卡保持 R/in-progress，真机项与裁决未完成不 closed。
