# MG-59 U-3 buff 超时立卡（②档，openmir2-a0 分诊批 2026-11-28）

## 缺口
`SKILL_UENHANCER`（MG-59）DoSpell 时刻已验（self-target 主链 + 657 Series 0/7/7），
**未验**：`AttPowerUp(nPower, nTime)`（`MagicManager.cs:618-620` 区段，UENHANCER 臂
`nTime = GetAttackPower(GetPower13(60) + LoByte(SC)*10, HiByte(SC) − LoByte(SC) + 1)`）
之后的 **buff 加成消费与超时消失**（`BuffApplyEvent.seconds` 的 tick 消费端）。

## 需要的机制面
**buff 生命期/超时面**：装置需在施法后推进世界时钟越过 `nTime`，验证加成在到期 tick
消失、期内 `WAbil`/657 稳定（与 MG-28/29 的 DefenceUp 秒数观测同族，但 UENHANCER 走
`AttPowerUp` 通道、观测面是目标 `DC/MC/SC` 加成值而非 `SM 100` tail）。

## 判据草案
1. golden（时钟越过 nTime 前后各一支）与 Rust 对拍：到期前加成在、到期后无；
2. `nTime` 公式边界（SC 高低位差 = 1 的最小窗口）一支；
3. 牙：到期 tick 判定 ±1 ⇒ 红。

## 撤销条件
若 buff 超时面由 MG-28/29 族卡统一建（共享 `BuffApplyEvent` 消费端装置），本卡并入该卡。
