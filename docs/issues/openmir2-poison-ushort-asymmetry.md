# 施毒 `nPower` 的 `(ushort)` 不对称：**未复刻、当前不可达、仅登记**

> 状态：**已知不对称登记**（2026-11-21，只读调研；不改代码）。
> **不进 manifest `deviation`**（理由见 §4）；也不写成 scope 的「未验」（语义不同，见 §4）。
> 来源：施毒批（`card/poison-batch`，合并 `0e8988dd8`）复核时 `openmir2-10` 提出的观察项。

## 1. oracle 的不对称

| 路径 | 代码 | 是否有 `(ushort)` |
| --- | --- | --- |
| 单目标 `SKILL_AMYOUNSUL(6)` 绿毒 | `src/M2Server/Magic/MagicManager.cs:240` `nPower = (ushort)(GetPower13(userMagic, 40) + GetRPow(SC) * 2)` | **有**（`nPower` 在 `DoSpell` 内声明为 `int`，`:171`；先截断再回写） |
| 单目标红毒 | `src/M2Server/Magic/MagicManager.cs:244` `nPower = (ushort)(GetPower13(userMagic, 30) + GetRPow(SC) * 2)` | **有** |
| 群体 `MagGroupAmyounsul(48)` 绿毒 | `src/M2Server/Magic/MagicManager.cs:920` `nPower = MagicBase.GetPower13(40, userMagic) + MagicBase.GetRPow(SC) * 2` | **无**（helper 内 `int nPower`，`:916`） |
| 群体红毒 | `src/M2Server/Magic/MagicManager.cs:924` 同形 | **无** |

## 2. Rust 现状：未复刻（两路共用、无截断）

两路都走 `mir2-rs/crates/world-bevy/src/world.rs` 的 `cast_poison_on_target`，其功率来自
`mir2-rs/crates/world-bevy/src/magic.rs` 的 `power_poison`：

```rust
get_power13(rng, magic, level, n_int) + i32::from(get_rpow(rng, sc)) * 2
```

⇒ **i32 运算、无 `(ushort)` 截断**；单目标与群体因此共用同一实现，oracle 的不对称**未复刻**。

## 3. 当前不可达（上界证明，供「为什么可以不修」用）

- `GetRPow`（`src/M2Server/Magic/MagicBase.cs:29-41`）取 `HiByte/LoByte(w_int)` 的随机/低字节 ⇒ **≤255**，×2 ≤ **510**。
- `GetPower13` 末项 `DefPower + Random(DefMaxPower − DefPower)`：`DefPower/DefMaxPower` 在 C# 是 `byte`
  （`src/OpenMir2/Data/MagicInfo.cs:60/68`），在 Rust 是 `u8`（`mir2-rs/crates/data/src/models.rs:82-83`）⇒ **≤255**。
- 首项 `d18/(TrainLv+1)*(Level+1)`：`n_int = 40` ⇒ `d10 = 40/3 ≈ 13.33`、`d18 = 26.67`；
  `Level` 是 `byte`（`src/OpenMir2/Packets/ClientPackets/UserMagic.cs:10`）⇒ ≤255；`TrainLv ≥ 0`
  ⇒ 首项 ≤ `26.67 × 256 ≈ 6827`（加 `d10` ≤ 6840）。
- **合计上界 ≈ 6840 + 255 + 510 ≈ 7605 ≪ 65535** ⇒ 冻结数据与合法等级域内 `(ushort)` 恒等，
  **任何合法输入都不会回绕**。

## 4. 为什么不进 manifest `deviation`、也不写 scope「未验」

- `deviation` 的语义 = 与 oracle 有**可达**差异、用户可能撞上；本条**当前不可达**，登记成 deviation
  会让后来人误以为线上会差。
- scope 的「未验」读起来是「还没验」；本条是**已知不对称 + 有上界证明的不可达**，是第三种状态，
  塞进 scope 会把两种语义混在一起。⇒ 单列本卡。

## 5. 解除 / 处理条件（何时必须回头修）

出现下列任一情形，必须按 oracle 复刻该 `(ushort)` 截断并补用例：

1. 数据/公式使上界可能越过 **65535**（例如 `DefPower/DefMaxPower` 升位、`Level/TrainLv` 域放宽、
   `GetPower13` 的 `n_int` 变大）；
2. 两条路径（单目标 / 群体）被拆成各自函数，或 `power_poison` 的入参语义变化；
3. oracle 侧修正该不对称（届时本卡作废，按下一条 oracle 事实重登）。

## 6. 不造牙（刻意）

合法输入无法造出 >65535，写出来的牙会是**真空断言**（与 `tooth-must-answer-empty-input` 同理）。
验「该不对称存在」只能靠代码对照（§1/§2），不能靠变异。

## 7. 关联

- 施毒批交接与 follow-up：`docs/issues/openmir2-groupamyounsul-nondegenerate-golden.md` §7.6/§7.7
- 装置缺口审计（同类"前提要拿证据"）：`docs/issues/device-gap-premise-audit.md`
