# openmir2-npc-multiobserve-golden（装置卡，只写卡不实现）

> 卡：`openmir2-npc-multiobserve-golden`（NPC 线 NP-11 的独立装置卡；tier=L）。
> 背景：NP-11「NPC 显示/转向/受击/空闲帧」需要对**多观察者 AOI** 做 oracle 金标准，
> 与魔法线 §8 的「帧捕获硬骨头」同源（`SendRefMsg` 需真 `Envir`/`ActorMgr`）。
> 本轮子批③**不实现**，NP-11 清单保持 `implemented（未验证）` + notes 指向本卡。

## 1. 目标

用 oracle 真身（C# `src/M2Server`）产出「NPC 显示类帧」的多观察者金标准，并与 Rust 生产
下行（`bins/game-svr` 的 AOI 下行）逐接收者逐帧对拍。

## 2. 观察者集合（每例至少 5 类）

| 观察者 | 位置/条件 | 期望 |
| --- | --- | --- |
| 自己 | 施动玩家本身 | 按 oracle 是否发给自身（出现/转向/受击） |
| 同屏近邻 A | 同图、±视野内 | 收到出现/转向/受击帧 |
| 视野边缘 B | 恰在 `SendRefMsgRange` 边界 | 边界含/不含按 oracle（±1 格对照） |
| 视野外 C | 超出范围 | 零帧 |
| 异图 D | 不同地图 | 零帧 |

隐藏 NPC（`IsHide`）另加一条：是否向他人可见（oracle `AddQuestNpc` 语义）。

## 3. 帧集合

`SM_TURN(10)`、`SM_HIT(14)`、`SM_FEATURECHANGED(41)`、出现/消失帧（AOI）、NPC 空闲随机帧；
每帧按 `(ident, recog, param, tag, series, tail)` 逐字节记录，按**接收者**归集。

## 4. 「抽空为止」

- 每例记录 `DrainPasses`/`DrainPassesHitCap`（命中上限 ⇒ 红），直到新增用例不再产生**新帧类型**或**新接收者类别**；
- 排除墙钟字段并显式列出（`delivery`/tick 类）。

## 5. 牙（缺一不可）

1. 删一个观察者的发送 ⇒ 该接收者期望非空时红；
2. 把视野边界 ±1 格 ⇒ `B` 类期望翻转，必红；
3. 把某帧从金标准删掉 ⇒ 比较器行数断言红；
4. 全零账本对全零金标准 ⇒ 必须红（防空转）。

## 6. 装置与写集（预登记）

- C#：`mir2-rs/tests/parity/csharp/NpcAoiGolden/`（真 `Envir` + `ActorMgr` 注册 + 替身 `INetChannel` 多接收者）；
- golden：`mir2-rs/tests/parity/golden/npc/aoi/*.json`；
- Rust 比较器：`bins/game-svr` 测试内多 socket rig（复用 `SpellFrameRig` 同族）；
- 写集：`tests/parity/csharp/NpcAoiGolden/**`、`golden/npc/aoi/**`、`bins/game-svr` 的 `#[cfg(test)]` 段；
  **不碰** NPC 装载链、`crates/script`。

## 7. DoD

- 5 类观察者 + 隐藏 NPC 各至少 1 例；帧类型覆盖 §3 全部；
- 四颗牙各红→复原→复绿（原文入 `E:/UserFleet/qa/npc-parity/`）；
- 清单 NP-11 升 `verified` 时 notes 指向本卡与 golden 路径。
