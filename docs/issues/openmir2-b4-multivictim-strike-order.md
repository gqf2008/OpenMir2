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
