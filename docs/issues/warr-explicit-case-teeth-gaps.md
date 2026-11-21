# warr 3/4/7（MG-01 等）显式/默认臂：**无 639 行为牙** + 缺零抽断言——补上

> 状态：**已知缺口登记（2026-11-21）**，未实现。oracle 锚：`src/M2Server/Player/PlayObject.Attack.cs:1269-1281`（default 支与 639 回帧）。来源：`docs/issues/parity-assertion-selfcheck.md`
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
