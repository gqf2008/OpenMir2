# warr 3/4/7（MG-01 等）显式/默认臂：**无 639 行为牙** + 缺零抽断言——补上

> 状态：**已补齐（2026-11-21，`card/warr-teeth`）**：639 牙已实测；9 支零抽断言已落码并配牙。oracle 锚：`src/M2Server/Player/PlayObject.Attack.cs:1269-1281`（default 支与 639 回帧）。来源：`docs/issues/parity-assertion-selfcheck.md`
> 判据 1/4 反向自查（MG-01 打叉）。关联：`openmir2-warrskill-harness-input-hardcode.md`（输入硬编码，单列）。

## 1. 当前事实

1. **无 639 牙**：`openmir2-warrskill-entry.md` C5 的变异表覆盖 12/25/26/27/34/38；
   **3/4/7（默认支经战士门出 `SM_MAGICFIRE_FAIL(639)`）没有任何变异证据**——删掉 `broadcast_ref_msg_with_self`
   的 639 调用，现有记录不保证会红。
2. **无零抽断言**：`warr-3/4/7/12/25/26/27/34/38` golden 的 `Rng` **全为空**，但比较器只比帧，
   **没有"Rust 零抽"与"golden Rng 为空"两侧断言**（43 已有，其余 9 支没有）。
3. 这三支**不在** `-Mode Both` 的 Driver/Comparator 集合（静态 golden `#[test]`）⇒ 门不覆盖，
   证据只能靠 `matches_golden` 的 `N passed`（交付时贴）。

## 2. 补法

1. **639 牙**：在 `handle_spell_cmd_with_magic` 的 `default_arm && is_warr_skill` 分支删/改 `sm_magicfire_fail`
   发射 ⇒ `warr_3/4/7` 中至少一支红；贴 `left/right`；
2. **零抽断言**：对 9 支通用化——golden `case.Rng` 为空 + Rust 侧 RNG 状态快照前后相等（或显式计数）；
   快照失效条件按 43 注释口径写明（自定义 `Debug` 会变盲 ⇒ 改 `draws()`）；
3. 每颗牙还原后 `git hash-object` 与改前相等、`MUTATION`=0、复跑绿。

## 3. 记录

- 更新 `openmir2-warrskill-entry.md` C5 表（补 3/4/7 行）与 manifest 对应条目的 `notes`/`scope`；
- scope 增一句"门不覆盖本叶，证据来自 `warrskill_parity_tests` 的 N passed"。

## 4. 完成记录（2026-11-21）

### 4a. 639 牙（纯实验，产品代码零残留）
**变异**：`handle_spell_cmd_with_magic` 的 `default_arm && is_warr_skill` 块里去掉
`sm_magicfire_fail` 发射 ⇒ `warr_3/4/7` **三支全红**（`warrskill_parity_tests.rs` 第 308 行）：
`warr-3/invoke 收件人 caster 的帧数不一致（got [] / want [(639, 0, 0, 0, "")]）` / `left: 0` / `right: 1`
（4/7 同形）。还原：`git hash-object` = 变异前 `1c9f963b41fcc…`、`MUTATION`=0、三支复跑 `1 passed`（rc=0）。

**三态塌缩（重要，单列）**：本支**没有 driver 层**——`warr_3/4/7_matches_golden` 既是驱动也是比较器
（静态 golden `#[test]`，无台账/`run_id` 分离）。因此"driver 红 / comparator 红"的三态分类在这里
**只剩两态**：**比较器层能否发现**。本变异证明：**能**。以后对照三态表时不要以为这一态漏了。

### 4b. 零抽断言（代码 + 牙）
**落点**：`warrskill_parity_tests::check_golden` 对**每一份 warr golden 的每个 case** 断两侧：
① golden `case.Rng` 必须为空；② Rust RNG 状态快照（`CaseObs.rng_before/after`）前后相等。
失效条件已按 43 口径写明（自定义 `Debug` 省略状态字段 ⇒ 变盲 ⇒ 改用 `draws()`）。

**牙**：`Arm::Ergum` 里插 `world.rng.random()` ⇒ `warr_12_matches_golden` 红（第 313 行）：
`Rust 零抽（RNG 状态不得变化）`（`inext 1/inextp 22` vs `0/21`）；
还原 `hash-object` = `ff7eb2cb85e8…`、`MUTATION`=0、`matches_golden` **19 passed / 0 failed**。

