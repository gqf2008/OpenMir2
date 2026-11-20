# GM 删技能的 657 接线（`#11/#12`，2026-11-20）

> base `master = c0cafb4ee`；分支 `card/657-del-skill`。对应计划单 `qa-657-wiring-plan` 的直发组第二批。

## 1. 实现

`apply_pending_gm_target_ops` 的删技能支（`del_skill`）在**审计直发帧（210/212）之后、绿字（252）之前**
补 657：

- `send_charstatuschanged_self_and_observers`（自广播副本 + 广播他人 + 自更新副本，逐收件人按
  `world.entities.get(target).hit_speed` 重编码）+ 不手工追加（helper 已含两自见副本）；
- **不用 `broadcast_ref_msg_with_self`**（`bodies: &[Vec<u8>]` 单 body ⇒ `Series` 固定）。

位置依据：oracle `DelSkillCommand.cs` 循环后**无条件** `RecalcAbilitys()`；其尾段在 `Race==Play` 时
`StatusChanged(); SendUpdateMsg(RM_CHARSTATUSCHANGED, …)`（`src/M2Server/Player/PlayObject.cs:2875-2876`）
⇒ 自见 2 条 + 观察者各 1 条；657 入队优先级 155，在 `RM_SYSMESSAGE`(252) 之前。

## 2. 两处 oracle 事实纠正（会被写进合并信息）

1. **`All` 支也执行 `RecalcAbilitys()`**（oracle 循环后无条件调用）⇒ **`All` 也出 657**。
   原断言"All 支不发帧不发字"改为"**只允许 657（自见 2 条），不发 212/绿字**"。
2. **自见 657 的 `Series` 取"重算后的落地 `hit_speed`"**（`apply_ability_to_entity` 之后），
   **不是**插入前实体上的旧值。本用例给目标配 StdMode 5 武器（`mac=0x0500`）使落地值非 0
   （实际为回绕值 65531，非 0 即非退化），**两臂都断言 `!= 0` 防退化区间**。

## 3. 既有期望的改动（逐处）

| 用例 | 改前 | 改后 | 依据 |
| --- | --- | --- | --- |
| `gm_del_skill_host_apply_emits_212_and_greens` 主案例（s902） | `[212, 100]` | `[212, 657, 657, 100]` | `PlayObject.cs:2875-2876` |
| 同用例第二子案例（s902） | `[212, 212, 100]` | `[212, 212, 657, 657, 100]` | 同上 |
| 同用例 `All` 支 | `collect().is_empty()` | 只允许 657（902 恰 2 条） | `DelSkillCommand.cs` 循环后无条件 `RecalcAbilitys()` |
| `gmcmd::tests::x2b_del_skill_recalc_writes_back_flame_ring_and_revival` 三处 | `collect().is_empty()`（"不得出 210/212"） | 只允许 657（不得出 210/212 的原意保留） | 同上 |

## 4. 两臂用例 + 两颗方向不同的牙

- 臂 1（有 saves ⇒ `(Some(tsg), Some(magic))`）：`gm_del_skill_host_apply_emits_212_and_greens`，
  逐 socket 断言：目标 902 自见 2 条（`Series == 落地 hit_speed`）、观察者 901/903 各 1 条（各自 7/9）、
  超 range 904 零帧；同 socket 内 657 在 212/210 之后、100 之前。
- 臂 2（无 saves ⇒ `(Some(tsg), None)`）：`gm_del_skill_without_saves_arm_emits_657`——只出 657
  （自见 2 + 观察者各 1），**无 212/绿字**。

**牙（实跑原文）**：
| 变异 | 结果 |
| --- | --- |
| T1 删 657 发射 | `cs(902): left [] right [65531, 65531]`（逐 socket 计数红） |
| T2 把 657 移到绿字之后 | s902 `left [212,100,657,657]` vs `right [212,657,657,100]`（顺序红） |

还原后 `lib.rs` sha 与基线一致、`MUTATION` 标记 0。

## 5. 门禁

- `cargo test -p game-svr --lib`：**2153 passed / 0 failed / 73 ignored**（rc=0）
- `cargo test --workspace`：**4385 passed / 0 failed / 159 ignored**（rc=0）
- `cargo run -p msgcodegen -- --check`：rc=0（604 条常量一致）
- `cargo fmt --all -- --check`：rc=0
- `cargo clippy --workspace --all-targets -- -D warnings`：待 0 车道独占（v4）
