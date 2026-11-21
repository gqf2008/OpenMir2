# B4 多受害者同刻延迟受击的帧序：产品批量 flush vs oracle 逐 actor `Run()`

> 状态：**待立项**（本文件只登记发现，不修）。
> 发现于：MG-17（魔号 9 `SKILL_FIRE` 穿透）数据包生成（2026-11-20 夜）。
> 行号请在本文件的提交信息里看；散文里按卫生门规则用**符号锚**（`bins/game-svr/**` 属 HIGH_CHURN，禁 `path:line`）。

## 可观测差异（同一场景、同一 run）

| 接收者 | golden（oracle） | rust（当前产品） |
|---|---|---|
| `obs_in` 的 657 帧序 | `17, 638, 31(target), 53, 656, 31(own)` | `17, 638, 53, 31(own), 31(target), 656` |

场景：两名玩家（target 与 obs_in）在**同一 600ms** 到期收到穿透受击；oracle 对每个
actor 各跑一次 `Run()`，先处理 target 的受击广播（31 到 obs_in），再处理 obs_in 自己的
53/656/31；产品把两条 pending 一次性交给 B4。

## 产品侧事实（符号锚）

- `sim_loop` 每 tick 调**一次** `drain_b4_with_enter_cfg(&rec, …)`，`rec` 里是本 tick **全部**
  `slave_self_deliveries`。
- `drain_b4_self_deliveries`：**一个** `SpellFrameBuf::new()` → `for o in &rec.slave_self_deliveries`
  全量循环（8044/10052 与 8103 两段）→ 末尾**单次** `buf.flush(online, gates)` + `struck.flush`。
  **不存在"每条 pending 后 flush 一次"的结构** ⇒ 产品是**批量**，不是逐 actor。

## 定性

**这是可观测差异，不是装置问题。** 装置按产品语义记录台账；比较器逐帧严格比较，因此该差异在
多受害者同刻的场景里必然显形。单受害者场景无差异（既有绿卡不受影响）。

## 处置

**修不修、怎么修单独立项。** 修复方向（若立项）应是把 B4 改为"逐 `SelfDelayOutcome` 处理 +
条目间 drain"，与 oracle 的逐 `actor Run()` 同构；这会改变所有 stem 的排水序，必须全量重跑。
在此之前，MG-17/18 的 **B 口径**（见 `docs/issues/openmir2-magic-9-10-pass-through.md`）生效：
多受害者同刻的帧序不作判据。

## 施毒 4 支同源（2026-11-21，`card/poison-batch`）

**同一根因的第二实例**：施毒（魔号 6）与群体施毒（魔号 48）的 `RM_POISON` 在**目标**的
`Run()` 里投递，由此派生 656/657/100；这些帧与**施法者自己**的 640（`RM_MAGIC_LVEXP`，同为
1000ms 到期）分属两条 actor 队列。oracle 逐 actor `Run()`，产品把本 tick 的
`slave_self_deliveries` 一次性交给 B4 批量 flush ⇒ **跨 actor 的先后不保**。

**实测证据**（`amyounsul` s1，同一 run；只列 caster 的 `Sent` 六元组 `(ident,param,tag,series,tail)`）：

| golden（oracle） | rust（产品） |
|---|---|
| `638, 642, **640**, 656, 657` | `638, 642, 656, 657, **640**` |

两条序列的**全部帧字段逐一相等**（`tail`/`param`/`tag`/`series` 均同），仅 `640` 的**位置**不同；
`target`/`obs_in` 的序列与字段也逐项相等。⇒ **只是顺序**，不是字段差异。

**`due_ms_rel` 的例外**：派生帧（656/657/100）的 `due` 记录的是装置**落地步**
（`Sleep(100)` 过冲 ⇒ 900/1000，生成期有并发编译），不是产品语义；比较器对施毒件的非 640 帧
**只判批次类**（0 vs >0），`640` 的 `DeliveryTime − t0` 按 **|Δ|≤32ms**（2× `TickCount` 15.6ms
粒度）容差判。**判别力保留**：950/600 vs 1000 差 ≥50ms，远大于容差。

**白名单（写死，别处出现即 panic）**：`fire`（MG-17）+ `amyounsul` / `amyounsul2` /
`groupamyounsul` / `groupamyounsul2`。比较器入口有负例牙（非白名单 stem 带标记 ⇒ panic）。

**放宽的只是顺序**：帧的 `ident/recog/param/tag/series/tail` 仍逐项进多重集键；RM 七字段多重集
照旧；`Rng` 逐值；HP 面照旧。

**移除条件**：与上节同——产品改为“逐 `SelfDelayOutcome` 处理 + 条目间 drain”（与 oracle 逐
`actor Run()` 同构）后，删掉这 4 支 golden 的 `frame_order_scope` 并把白名单缩回 `fire`，
恢复严格按序。

## 2026-11-22 结构性更正（openmir2-1e，B17 装置对照）

**本卡此前措辞只覆盖 strike 队列层；实测 657 改名帧证明「帧广播层」也要 per-viewer 延迟。** 装置
（`MagicGolden` 的 `DrainOnce`）改成生产 `GetMessage` 逐条语义后，bigcloak s1 的 target/obs_in 的 657 变
`{1002@800, 1001@900}`（= Rust），但 caster 仍差 100；把扫序临时换成 `[target,caster]` 则 caster 对齐、target
差 100 —— **与扫序无关：per-viewer 队列模型下「先被 Run 的 victim 看不到另一个 victim 同 tick 的改名广播」**，
Rust 的同步批量 flush 没有这一层。⇒ 修法不是「逐 `SelfDelayOutcome` + 条目间 drain」一句话，而是两层：
**(i) 泵的 per-actor 回合序 + (ii) 帧广播的 per-viewer 延迟**。

详见施工图与影响面：`docs/issues/b4-per-actor-queue-blueprint-20261122.md`（含 7–9 编辑点、50 case 的
翻转清单、`DrainPasses` 语义变更坑、`warrskill` 覆盖面边界）。实验装置改动在 `card/b4-device-drain`
（`bcc232572`，不单独合并；须与产品 per-actor 改同批）。
