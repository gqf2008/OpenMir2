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

## 落地记录（2026-11-29，card/items-scope-triage，openmir2-56）
- **装置**（MagicGolden/Program.cs，commit f11cba39e）：`MagicSpec.RunAfterMs` 旋钮
  （null=关 / -1=到期后 500ms 哨兵 / -2=恰在到期边界哨兵 / 正值=固定偏移）+ RunMagicCase
  新支（排水后 `ClockSeam.Set(castTick+offset) + CurrentDueMs=offset + caster.Run()` 直调，
  MG-12/40 heal-ticks 先例同款；插在 `casterExtra0` 捕获之后，保住 `CasterExtraAbil0`
  恒比键的施法后语义）。oracle 到期块=`PlayObject.Message.cs:168-227`（逐槽严格
  `now > ExtraAbilTimes[i]` ⇒ 清零+flag=0+`HealthSpellChanged()`；预告臂
  `flag==0 && now > times-10000` ⇒ 10 秒 SysMsg）；`PlayObject.cs:5326`
  `ExtraAbilTimes[0]=GetTickCount()+nTime*1000`。条件键 `run_after_ms`/`u3_n_time`/
  `extra_abil0_after_run` + case 列 `ExtraAbil0AfterRun`/`U3NTime`（WhenWritingNull）。
- **金标**：三支全自施（受惠者=caster）——`u3_run_before_expiry`（4600，Run=1000 ⇒
  Run 静默、buff=2、640(1000) 照常）；`u3_run_after_expiry`（4700，-1⇒250500：
  **53(250500)→100(250500)** 证实优先级 68<252 出帧序、`extra_abil0_after_run=0`）；
  `u3_run_at_expiry`（4800，-2⇒恰 250000：严格 `>` 不成立 ⇒ **不到期**、只出
  10 秒预告 100(250000)、`extra_abil0_after_run=2`）。`u3_n_time`=250000ms 三支一致
  （SC=15 ⇒ Hi−Lo+1=1 ⇒ 零抽；GP13(60)+150=250）。纯插入拼接：前缀 43018 字节逐字节
  不变、既有 5 例 deep-equal（程序断言）。
- **Rust 驱动**（lib.rs uenhancer_parity_drive）：golden `run_after_ms` 走 `as_i64`
  （as_u64 对负哨兵返 None 会静默丢键）；交付循环上界 `min(wait_ms, run_after)`
  （640 due=1000 仍走既有路径，零重排）；tick 分支拨钟 `run_after−200`（tick_head 的
  Clock 段自推进 +200ms ⇒ SimExtraAbil 恰落 `now==run_after`；at 例即 oracle 同款边界）
  → `world.tick()` → drain_extra_abil_expiries + flush_healthspell_slots（生产 RECOVERY
  形状；P1 恢复跳过=全血 no-op，帧 due=run_after）；RM 广播面 `hear_targets` 前置到
  交付前（tick 后取值为空 ⇒ 10008/10120/10139 缺失，实测红教训）。比较器标签 5→8；
  verify 恒比键 +`ExtraAbil0AfterRun`/`U3NTime`（双缺回落 i64::MIN 恒等，既有支零影响）。
- **判据对照**：草案 1 = before/after 两支金标+对拍绿 ✓；草案 2 = SC=15 即高低位差最小
  窗口（Hi−Lo+1=1），`u3_n_time` 两侧独立实测恒比 ✓；草案 3 = M-U3 变异
  （world.rs 到期判定 `>` 改 `>=`）⇒ **仅 u3_run_at_expiry 红**（ExtraAbil0AfterRun
  golden=2 rust=0 + 预告帧被 [53,恢复帧] 替换），其余 7 支绿；逆编辑精确还原后同 id
  （mg59-u3-deliver3）8 驱动 + 比较器全绿 ✓。
- **门禁**：fmt GREEN / clippy rc=0 / `cargo test --workspace --lib` 4232 passed /
  0 failed / 283 ignored / 散文锚门 GREEN / msgcodegen rc=0。
- **撤销条件**：未触发（本卡独立落地；MG-28/29 族共享装置未建）。
- **MG-59 scope 已升 full**（parity-gap-magic.json 三面齐），报总监裁定（派工单 opc-66）。
