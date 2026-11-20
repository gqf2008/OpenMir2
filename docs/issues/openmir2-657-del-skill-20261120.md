# GM 删技能的 657 接线（`#11/#12`，2026-11-20）

> base `master = 2f46a4d94`（2026-11-20 rebase）；分支 `card/657-del-skill`。对应计划单 `qa-657-wiring-plan` 的直发组第二批。

## 1. 实现

`apply_pending_gm_target_ops` 的删技能支（`del_skill`）在**审计直发帧（210/212）之后、绿字（252）之前**
补 657：

- **自见恰 1 条**：单自见帧（`Series` = 目标**实体**的 `hit_speed`，重算后已由
  `apply_ability_to_entity` 镜像）+ `broadcast_charstatuschanged_except_self`（观察者逐收件人取**各自实体**）；
- **不用 `broadcast_ref_msg_with_self`**（`bodies: &[Vec<u8>]` 单 body ⇒ `Series` 固定）。
- `Series` 源**统一到实体**（oracle `this.HitSpeed`；与 `43` 的 spellbuf 口径一致）。

位置依据：oracle `DelSkillCommand.cs` 循环后**无条件** `RecalcAbilitys()`；其尾段在 `Race==Play` 时
`StatusChanged(); SendUpdateMsg(RM_CHARSTATUSCHANGED, …)`（`src/M2Server/Player/PlayObject.cs:2875-2876`）
⇒ 自见**净 1 条**（`SendUpdateMsg` 队首去重，见 §3.5）+ 观察者各 1 条；657 入队优先级 155，
在 `RM_SYSMESSAGE`(252) 之前。

## 2. 两处 oracle 事实纠正（会被写进合并信息）

1. **`All` 支也执行 `RecalcAbilitys()`**（oracle 循环后无条件调用）⇒ **`All` 也出 657**。
   原断言"All 支不发帧不发字"改为"**只允许 657（自见净 1 条），不发 212/绿字**"。
2. **自见 657 的 `Series` 取**实体**的 `hit_speed`**（= 重算落地值，`apply_ability_to_entity` 之后镜像），
   **不是**插入前实体上的旧值、也不是会话投影。两臂都断言三件：`assert_ne!(hs, 0)`（守"镜像丢了 ⇒
   实体回落 0"）+ `assert_eq!(hs, 65531)` + 帧 == 实体。`65531` 来历：武器 `mac=0x0500` ⇒ `hi_byte=5`
   ⇒ `real_attack_speed(5) = -5` ⇒ `AddAbil.HitSpeed` 按 `u16` 回绕。**限定：该值未经 oracle 侧实测
   对照**（oracle 同为 `ushort`、预期同样回绕；若日后发现 oracle 用别的宽度/符号，断言要跟着改）。
   预置实体 5 与观察者 7/9/11 都不等于 65531 ⇒ "预置被覆盖/读错侧"均可区分。

**`ae` 复核结论（`75ffd2588`）——证据边界**：
- **`Series` 源从会话改为实体，在本路径 behavior-neutral**——上游 `apply_ability_to_entity`
  已在 657 发射前完成镜像，实体与会话恒等 ⇒ 本组断言证的是**值**，不是**源的选择**。
- 该改动的正当性由项目裁定「**实体为权威、会话是镜像**」承担，**不由本卡断言锁定**；
  若日后需要独立锁定，须构造"实体≠会话"的区分态（当前不作为要求）。

## 3. 既有期望的改动（逐处）

| 用例 | 改前 | 改后 | 依据 |
| --- | --- | --- | --- |
| `gm_del_skill_host_apply_emits_212_and_greens` 主案例（s902） | `[212, 100]` | `[212, 657, 100]`（净 1，§3.5） | `PlayObject.cs:2875-2876` |
| 同用例第二子案例（s902） | `[212, 212, 100]` | `[212, 212, 657, 100]`（净 1） | 同上 |
| 同用例 `All` 支 | `collect().is_empty()` | 只允许 657（902 恰 1 条） | `DelSkillCommand.cs` 循环后无条件 `RecalcAbilitys()` |
| `gmcmd::tests::x2b_del_skill_recalc_writes_back_flame_ring_and_revival` 三处 | `collect().is_empty()`（"不得出 210/212"） | 只允许 657（不得出 210/212 的原意保留） | 同上 |

## 3.5 自见条数修正：2 → **1**（2026-11-20，`89` 清算 + 本卡独立复核）

**推理（两会话独立同得）**：`SendUpdateMsg`（`BaseObject.Message.cs:283-304`）做**队首同 ident** 去重：
peek 队首，同 `wIdent` 就弹出再入队。删技能支在 `:2875` 入队那一刻，队列里只有循环里 `SysMsg` 的
`RM_SYSMESSAGE`(252)；657 的优先级 `(byte)RM_CHARSTATUSCHANGED = 155 < 252` ⇒ **657 成新队首**
⇒ `:2876` 的 `SendUpdateMsg` 把它弹掉再入 ⇒ **自见净 1 条**、观察者各 1 条。

**旧期望 2 的来历与为何错**：本卡初版（以及 `#9` 修复）依据的是"凡经 `RecalcAbilitys` 就 2 条"
（自见广播副本 + 自更新副本），当时没有检查 `:2876` 时刻的**队列状态**；`89` 指出该结论只在
"队列里有 <155 的待办（如商店路径的 41/52/622）"时成立。删技能路径的队首恰是 657 ⇒ 应为 1。
⇒ **已按 1 改实现（单自见帧 + `broadcast_charstatuschanged_except_self`）与四类期望**。

**`gmcmd::x2b_del_skill_recalc_writes_back_flame_ring_and_revival` 三处断言**：已核，**只判"全为 657"、
不判数量** ⇒ 不受 2→1 影响，未改。

## 4. 两臂用例 + 两颗方向不同的牙

- 臂 1（有 saves ⇒ `(Some(tsg), Some(magic))`）：`gm_del_skill_host_apply_emits_212_and_greens`，
  逐 socket 断言：目标 902 自见 **1 条**（`Series == 实体 hit_speed`；`assert_ne!(…,0)` + `== 65531`）、
  观察者 901/903 各 1 条（各自 7/9）、超 range 904 零帧；同 socket 内 657 在 212/210 之后、100 之前。
- 臂 2（无 saves ⇒ `(Some(tsg), None)`）：`gm_del_skill_without_saves_arm_emits_657`——只出 657
  （自见净 1 + 观察者各 1），**无 212/绿字**；`Series` 同样断言实体值（`assert_ne!(…,0)` + `== 65531`）。

**牙（实跑原文）**：
| 变异 | 结果 |
| --- | --- |
| T1 删 657 发射 | `left [212, 100]` vs `right [212, 657, 100]`；无 saves 臂 `left []` vs `right [65531]`（计数红） |
| T2 把 657 移到绿字之后 | s902 `left [212,100,657]` vs `right [212,657,100]`（顺序红） |

还原后 `lib.rs` sha 与基线一致、`MUTATION` 标记 0。

## 5. 门禁

- `cargo test -p game-svr --lib`：**2153 passed / 0 failed / 73 ignored**（rc=0）
- `cargo test --workspace`：**4385 passed / 0 failed / 159 ignored**（rc=0）
- `cargo run -p msgcodegen -- --check`：rc=0（604 条常量一致）
- `cargo fmt --all -- --check`：rc=0
- `cargo clippy --workspace --all-targets -- -D warnings`：**rc=0**
  （首跑红 2 处，**均本卡引入**：新用例插入时落在 `gm_clear_copy_self_target_uses_live_table` 的
  `#[test]` 与 `fn` 之间 ⇒ ① 新用例上出现重复 `#[test]`、② 该旧用例丢失 `#[test]` 变成
  `never used`。已把属性/文档移回原位——**不是旧债**。）
