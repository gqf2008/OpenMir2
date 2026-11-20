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
  `8044(108,false)`）；`RM_SPELL(10008)` 的 p1/p2 用**点击点**（oracle 在 case 体前发，
  9/10 的终点改写发生在 case 体内）。

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
