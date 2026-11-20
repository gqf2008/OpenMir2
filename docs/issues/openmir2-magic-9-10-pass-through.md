# MG-17 / MG-18（魔号 9 / 10 穿透）数据包与 scope 限制

> 魔号 9 `SKILL_FIRE`（`MagicManager.cs` 的 `case SKILL_FIRE`）与 10 `SKILL_SHOOTLIGHTEN`：
> 沿方向穿透 5 / 8 格，逐格 `GetMovingObject` + `Random(10) >= AntiMagic`，命中发
> `RM_MAGSTRUCK`（600ms 延迟）。Rust 实现：`cast_pass_through_spell`；host 臂 `9 | 10`。

## 装置（本包新增，旧件零变化）

- `MonsterSpec.AntiMagic`（缺省 0 = 必中）+ `Fixture.monsters[].anti_magic`；
  Rust `golden_monsters` 11→12 字段、`place_monsters` 写 `e.anti_magic`。
- `RunMagicCase` / `RunFireball` 给 `obs_in`/`obs_out` 初始化 `WAbil.HP/MaxHP=100`
  （此前从未初始化 = 0，会让"旁观者受击"的 53/31 带 0/0，与实现侧不可比）。
- id 9/10 尾 640 改为**立即**出（`finish_spell_tail(..., None)`；oracle 的 640 在施法段，
  延迟 1000ms 会排到 600ms 的 31 之后）。
- 通用驱动：延迟受击的 RM 重建（玩家 `8044(255, late)` + `10052` + `8103×2`；怪物
  **延迟支同样是 `8044(255, late)`**——`SendStruckDelayMsg` 入队用 `Lowest`(255) +
  `LateDelivery=true`。**`8044(108,false)` 只属于即时受击支**，别混）；`RM_SPELL(10008)`
  的 p1/p2 用**点击点**（oracle 在 case 体前发，9/10 的终点改写发生在 case 体内）。

## 数据（`--case fire`，魔号 9）

- 布置：施法者 (100,100) → 点击 (101,100)；玩家 target 在 101；怪在 102/103/104
  （`AntiMagic` = **1/9/1**，互异）+ 106（给"穿透 5→8 格"刀留红点）。
- `rng_total_draws`：**s1=14 / s5=14 / s13=12**（全 >0）。
- `TargetHp`：100→**85 / 94 / 100**（s13 玩家骰门失败）。
- 帧面：mon0/mon2 各 1 条 `RM_MAGSTRUCK(8044)`；mon1（AM=9，必不中）/ mon3（5 格外）**0 条**；
  `obs_out` 空。

## SCOPE / 判据限制（B 口径）

- **多受害者同刻延迟受击的帧序不作判据**：产品是批量 flush、oracle 是逐 actor `Run()`
  （见 `docs/issues/openmir2-b4-multivictim-strike-order.md`）。本包的有效面 =
  **HP / RNG / RM 多重集 / 单受害者帧序**。
- 该差异为**可观测差异、非装置问题**；修复单独立项。

## `recog` 的语义（**不是**"恒为 actor id"）

SM 头前 4 字节的 `Recog` 槽按帧类不同：
- `SM_STRUCK(31)` / `SM_CHARSTATUSCHANGED(657)` 等：受击者/主体 ActorId；
- **`SM_MAGIC_LVEXP(640)`：装的是 MagicId**（oracle `src/M2Server/Player/PlayObject.Message.cs:1801`
  `SendDefMessage(SM_MAGIC_LVEXP, processMsg.nParam1, …)`，`nParam1` = MagicId；
  `SendDefMessage(short wIdent, int nRecog, …)` 见
  `src/M2Server/Player/PlayObject.Socket.cs:17`）。

⇒ **不要写"`recog` 必须是 actor id"的断言**；本包 `fire.json` 里 640 的 `recog=9`（魔号）是正确值。

## 帧时序（`due_ms_rel`）

`Sent[]` 每帧记 `due_ms_rel`（**相对施法时刻的毫秒**，立即发送 = 0）：
C# 侧 = `DeliveryTime - castTick`（非延迟消息记 0）；Rust 侧 = 交付段的步进 `elapsed`。
**绝对墙钟（`delivery`）永不进比较面**（跨栈不可比）。多受害者同刻口径下，
多重集 key = `(ident, recog, param, tag, series, due_ms_rel, tail)` ⇒ 只放宽"同 due 内的次序"，
**时序本身仍被判**（牙：`tooth_multi_victim_due_ms_rel_compared_red`）。
