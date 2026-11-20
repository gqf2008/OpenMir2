# ExtraAbil → WAbil 修正卡（2026-11-20）

> 依据：`docs/issues/qa-extraabil-adjudication-20261120.md`（`3b` 只读裁定）。
> 记录点：`master = 165c6edd1`。**只改 `ExtraAbil` 的"折进 `WAbil`"这一步**；字段、存档、机器人 AI 读全部保留。

## 1. oracle 结论（复述，含锚）

- 折算段：`PlayObject.cs:2818-2846`（DCUP/MCUP/SCUP 写 `WAbil.DC/MC/SC` 高字节、HITSPEEDUP 写临时 `HitSpeed`、
  HPUP/MPUP 写 `WAbil.MaxHP/MaxMP`、PWRRATE 乘 DC/MC/SC）。
- 覆盖段：`PlayObject.cs:2899-2906`（`HitSpeed = AddAbil.HitSpeed; WAbil.MaxHP/MaxMP/AC/MAC/DC/MC/SC = Abil + AddAbil`）。
- 两段在**同一方法体**（`RecalcAbilitys()`，`2008-2907`），之间**无 `return`/`goto`/`throw`**
  ⇒ 折算值**必定**被整体覆盖 ⇒ 对最终 `WAbil`/`HitSpeed` **永远没有影响**。
- `GetAttackPower`（`:2909-2960`）不读 `ExtraAbil`。`ExtraAbil` 仅剩的用途：存档往返
  （`WorldServer.Switch.cs:65/139-144`）与机器人 AI（`RobotPlayer.cs`×4）。

## 2. Rust 改动

| 位置 | 改前 | 改后 |
| --- | --- | --- |
| `bins/game-svr/src/lib.rs`（`recalc_session_ability`，原 12562） | `sg.state.wabil = recalc.wabil;`（= oracle 2899-2906）之后**再** `apply_extra_abil(...)` ⇒ 与 oracle 顺序相反、叠加**留存** | **停用该调用**；保留 `extra_abil` 形参（状态面照传）与 `ExtraAbil` 字段本身；函数注释写明"oracle `:2899-2906` 覆盖 `:2818-2846`" |

`apply_extra_abil` 函数本身保留（`ability.rs:784-825`，现有单测仍测其算术；当前无生产调用者）。

## 3. 既有期望修正（逐条：改前 → 改后 + 依据行）

| # | 测试 | 改前期望（错误行为） | 改后期望 | 依据 |
| --- | --- | --- | --- | --- |
| 1 | `tests::gm_del_skill_host_apply_emits_212_and_greens`（F2-R3，原 `lib.rs:49571-49575`） | `ExtraAbil[0]=3 ⇒ wabil.dc == 0x0400`（Hi 1+3） | **`0x0100`** | oracle `:2899-2906` 覆盖 `:2818-2846` |
| 2 | 同测试准备段注释（原 `49472`） | "不传 `Some(&extra_abil)` 会把加成抹掉（丢失 0x0300）" | 仍按值传（状态面），但**不折进 WAbil** | 同上 |
| 3 | `tests::spell_p3b5_36_uenhancer_frames_amulet_and_gates`（原 `lib.rs:81872`） | `after.dc == 0x0E05`（Hi 10+4） | **`0x0A05`** | 同上（技能 36 `AttPowerUp` 写槽 0） |
| 4 | `tests::upgrade_npc_submit_and_getback_full_chain` 文档注释（原 `88329`） | "`ExtraAbil[0]=5` ⇒ WAbil.dc=0x0601" | **`0x0101`** | 同上 |
| 5 | 同测试准备注释（原 `88524`） | "+5 后 WAbil.dc 应 0x0601" | 仍 0x0101 | 同上 |
| 6 | 同测试断言（原 `88713-88716`） | `sg_after.state.wabil.dc == 0x0601` | **`0x0101`** | 同上 |
| 7 | 同测试实体镜像断言（原 `88719-88722`） | `entity.dc == 0x0601`（"含 ExtraAbil"） | **`0x0101`**（"不含 ExtraAbil"） | 同上 |
| 8 | `:22382` 附近注释（技能审计调 `recalc_session_ability` 处） | "extra_abil 取活跃值（加成不被抹）" | 仍按值传，但不折进 WAbil | 同上 |

**卡面结论**：以上 #1/#3/#6/#7 是把"被覆盖值"当成最终值的**被测错误**，不是实现偏差；
旧注释（#2/#4/#5/#8）同源。全部换成 oracle 口径并留痕"旧期望编码了错误行为"。

## 4. 同款扫描（`[[fix-sweep-and-delivery-proof]]`）

扫法（只读）：

```text
grep -rn "0x0400|0x0601|0x0E05" mir2-rs/bins/game-svr/src/*.rs mir2-rs/crates/*/src/*.rs
grep -rn "extra_abil\["   mir2-rs/bins/game-svr/src/*.rs mir2-rs/crates/world-bevy/src/*.rs
grep -rn "nPower|AttPowerUp" mir2-rs/bins/game-svr/src/lib.rs
```

结果：
- 命中的其余 `0x0400` 均为**无关**（`bag.rs` 的 Desc 边界、`enter.rs` 的 featureEx、`itemwire.rs` 的模板值、
  `lib.rs:81148` 的输入武器模板值）。
- 测试侧只有两处写 `extra_abil[0]`：F2-R3（#1）与 upgrade 复验 F2（#4-#7），均已改。
- `world-bevy` 侧只有到期清零与 `AttPowerUp` 写槽（状态面），无 `WAbil` 期望。
- 全仓测试**未发现**第三族"以重算后 DC/MC/SC/MaxHP/MaxMP 含 ExtraAbil"为前提的断言（改后全量 0 红即为旁证）。

## 5. 登录/运行期口径（只登记，不改）

- 登录路径 `enter.rs::recalc_for_session_with_skills`（`enter.rs:172-190`）直接调纯函数
  `recalc_abilitys_with_prior_and_skills`，**本来就绕过** `apply_extra_abil`。
- 修正前：登录重算不含 ExtraAbil、运行期重算含 ⇒ 同角色登录后与 buff 期间的 52 帧 DC 不同（缺陷侧面）。
- 修正后：**两条路径一致**（都不含）。无需另改；"登录后恢复 ExtraAbil 状态"仍按 `WorldServer.Switch.cs:65`
  只恢复数组值、不折进 WAbil，与 oracle 一致。

## 6. 证据（实跑原文）

- **改前红（变异 = 把 `apply_extra_abil` 加回去，标记 `MUTATION-EA-REFOLD`）**：
  全量 `cargo test -p game-svr --lib` ⇒ **2143 passed / 3 failed / 70 ignored（rc=101）**，红的恰是
  `gm_del_skill_host_apply_emits_212_and_greens`、`spell_p3b5_36_uenhancer_frames_amulet_and_gates`、
  `upgrade_npc_submit_and_getback_full_chain` —— 三条正是本卡改期望的三处。
  ⇒ 这颗牙证明"再把 ExtraAbil 折进去会红"。
- **还原**：`cp` 快照 + `touch`，`lib.rs` sha 与变异前一致（`4324796d…`），`MUTATION` 标记 0。
- **改后绿**：同一条命令 **2146 passed / 0 failed / 70 ignored（rc=0）**。
- 其余门禁（fmt/clippy/`--workspace`/msgcodegen）待车道空后补跑，结果回填本节。

## 7. 禁区自证

- 未改任何清单 `status`；未删清单行。
- `IT-13` 位移两支与 766/54 的"不能进 verified"结论未动。
- 仅改 `lib.rs`（1 处调用 + 4 处期望 + 4 处注释）与本卡。
