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
| `bins/game-svr/src/lib.rs`（`recalc_session_ability` 内） | `sg.state.wabil = recalc.wabil;`（= oracle 2899-2906）之后**再** `apply_extra_abil(...)` ⇒ 与 oracle 顺序相反、叠加**留存** | **停用该调用**；保留 `extra_abil` 形参（状态面照传）与 `ExtraAbil` 字段本身；函数注释写明"oracle `:2899-2906` 覆盖 `:2818-2846`" |

`apply_extra_abil` 函数本身保留（`ability::apply_extra_abil`（符号），现有单测仍测其算术；当前无生产调用者）。

## 2.5 临时 `HitSpeed` 的可观测面（证据 + 一次已撤回的指令）

**结论：`ExtraAbil[HITSPEEDUP]` 只进"队列载荷 `wParam`"，不进线缆帧 `Series`。**

三层证据：
1. **oracle 源码**：临时写 `HitSpeed = HitSpeed + ExtraAbil[HITSPEEDUP]`（`PlayObject.cs:2830-2833`）；
   `:2876` `SendUpdateMsg(RM_CHARSTATUSCHANGED, HitSpeed, CharStatus, …)` 把**临时值放进消息 `wParam`**；
   `:2899` `HitSpeed = AddAbil.HitSpeed` 覆盖落地字段；657 handler（`PlayObject.Message.cs:1845-1847`）
   用 `processMsg.nParam1` + **出队时的 `this.HitSpeed`（=落地值）**，**不读 `wParam`**。
2. **oracle 真身探针**（`card/items-it07` 的 `buff_vectors.json`，六命中例，`ExtraAbil[3]=4`）：
   队列 `10139` 的 **`w_param=4`**，而 pumped 的 657 帧 **`series=0`**。
3. **Rust**：`sg.hit_speed = recalc.hit_speed`（`recalc_session_ability` 内，紧随 `sg.state.wabil = recalc.wabil;`）
   ——落地口径、不含 ExtraAbil；657 出帧一律取 `sg.hit_speed`。

**另**：`apply_extra_abil` **从未写过 `hit_speed`**（只改 DC/MC/SC 高字节与 MaxHP/MaxMP；槽 3 在其函数文档里
就登记为"本函数只回 `WAbil`"）⇒ 停用它**不会丢任何临时贡献**；旧实现下落地 `hit_speed` 也是最终值。

**留痕（指令撤回）**：本节初版曾收到"657 的 `Series` 应带临时值"的指令（把队列载荷当成帧面），
已据上面 1+2 撤回；**勿再按"帧带临时值"改代码**。载荷面的牙（`w_param == extra_abil[3]`）归
`card/items-it07` / G2 模板，不在本卡。

## 3. 既有期望修正（逐条：改前 → 改后 + 依据行）

| # | 测试 | 改前期望（错误行为） | 改后期望 | 依据 |
| --- | --- | --- | --- | --- |
| 1 | `tests::gm_del_skill_host_apply_emits_212_and_greens`（F2-R3，原 `tests::gm_del_skill_host_apply_emits_212_and_greens` 的 F2-R3 断言块） | `ExtraAbil[0]=3 ⇒ wabil.dc == 0x0400`（Hi 1+3） | **`0x0100`** | oracle `:2899-2906` 覆盖 `:2818-2846` |
| 2 | 同测试准备段注释（同一测试的准备段） | "不传 `Some(&extra_abil)` 会把加成抹掉（丢失 0x0300）" | 仍按值传（状态面），但**不折进 WAbil** | 同上 |
| 3 | `tests::spell_p3b5_36_uenhancer_frames_amulet_and_gates`（原 `tests::spell_p3b5_36_uenhancer_frames_amulet_and_gates`） | `after.dc == 0x0E05`（Hi 10+4） | **`0x0A05`** | 同上（技能 36 `AttPowerUp` 写槽 0） |
| 4 | `tests::upgrade_npc_submit_and_getback_full_chain` 文档注释（该测试的文档注释） | "`ExtraAbil[0]=5` ⇒ WAbil.dc=0x0601" | **`0x0101`** | 同上 |
| 5 | 同测试准备注释（该测试的准备注释） | "+5 后 WAbil.dc 应 0x0601" | 仍 0x0101 | 同上 |
| 6 | 同测试断言（该测试的 `wabil.dc` 断言） | `sg_after.state.wabil.dc == 0x0601` | **`0x0101`** | 同上 |
| 7 | 同测试实体镜像断言（该测试的实体 `dc` 镜像断言） | `entity.dc == 0x0601`（"含 ExtraAbil"） | **`0x0101`**（"不含 ExtraAbil"） | 同上 |
| 8 | `recalc_session_ability` 调用点附近的注释（技能审计处）（技能审计调 `recalc_session_ability` 处） | "extra_abil 取活跃值（加成不被抹）" | 仍按值传，但不折进 WAbil | 同上 |

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
  `spell_p3b5_36` 的输入武器模板 的输入武器模板值）。
- 测试侧只有两处写 `extra_abil[0]`：F2-R3（#1）与 upgrade 复验 F2（#4-#7），均已改。
- `world-bevy` 侧只有到期清零与 `AttPowerUp` 写槽（状态面），无 `WAbil` 期望。
- 全仓测试**未发现**第三族"以重算后 DC/MC/SC/MaxHP/MaxMP 含 ExtraAbil"为前提的断言（改后全量 0 红即为旁证）。

## 5. 登录/运行期口径（只登记，不改）

- 登录路径 `enter.rs::recalc_for_session_with_skills`（`enter::recalc_for_session_with_skills`）直接调纯函数
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
- **新增牙**：`tests::extraabil_hitspeedup_stays_out_of_landed_hit_speed`——同一会话两次重算
  （`extra=None` vs `[0,0,0,4,0,0,0]`），落地 `hit_speed` 与 657 `Series` 必须相同。
  绿/红条件：旧实现绿、新实现绿；"把临时值塞进落地 `hit_speed`"（保留过头）必红。
  载荷面（"删过头"）的牙归 it07/G2 模板，不在本卡。
- **改后绿**：同一条命令 **2146 passed / 0 failed / 70 ignored（rc=0）**。
- **新牙实跑（区分"删对了/删过头"）**：
  - 旧实现（加回 `apply_extra_abil`）：`extraabil_hitspeedup_stays_out_of_landed_hit_speed` **绿**
    （证明该函数从未碰 `hit_speed`，删它不丢临时贡献）；
  - 新实现：**绿**；
  - 变异「把 `extra_abil[3]` 加进落地 `hit_speed`（保留过头）」：**红**（`assertion failed`）。
  还原后 `lib.rs` sha 与快照一致（`1b6ce084…`），`MUTATION` 标记 0。
- **最终四件套（rebase 到 `cc62ee05e` 后，tip `b6dcd306f` 实跑）**：
  - `cargo test -p game-svr --lib`：**2147 passed / 0 failed / 70 ignored**（rc=0）
  - `cargo test --workspace`：**4372 / 0 / 155**（rc=0）
  - `cargo clippy --workspace --all-targets -- -D warnings`：**rc=0**（0 车道时独占跑）
  - `cargo fmt --all -- --check`：**rc=0**
  - `msgcodegen -- --check`：rebase 前 rc=0（604 条一致）；rebase 后待车道空补跑一次（未改动常量面）。

## 7. 禁区自证

- 未改任何清单 `status`；未删清单行。
- `IT-13` 位移两支与 766/54 的"不能进 verified"结论未动。
- 仅改 `lib.rs`（1 处调用 + 4 处期望 + 4 处注释）与本卡。
