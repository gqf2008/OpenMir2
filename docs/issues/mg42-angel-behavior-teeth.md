# MG-42（SKILL_ANGEL 41）：已 verified 但**无行为牙**——补一颗

> 状态：**已知缺口登记（2026-11-21）**，未实现。oracle 锚：`src/M2Server/Magic/MagicManager.cs:462-473`（`case SKILL_ANGEL`）。来源：`docs/issues/parity-assertion-selfcheck.md` 判据 1
> 反向自查（MG-42 打叉）。现行口径（`opc-19` 2026-11-21）：**升格必须附至少一颗行为牙，或在 scope 写明"无牙（理由）"**。

## 1. 当前事实（不是"待优化"）

- `docs/issues/mg42-angel-stub-impl.md` 全文**没有变异/牙记录**；manifest 的 `scope` 也未声明"无牙"。
- 覆盖来源：`tests::angel_parity_summon_ok` / `angel_parity_cap_hit`（驱动）+ `real_rust_ledger_angel`（比较器）。
  比较器确实比对 `Angel` 标量块与帧，但**从未证明"改坏产品侧会红"**——删除一帧或改一个字段，
  现有记录里没有任何 `left/right` 实测。

## 2. 补法（候选变异，任选其一，需实测红）

1. **删帧**：驱动路径里少发 `17/638/642` 之一（或把 640 的延迟从 1000ms 挪位）⇒ 比较器帧集合/多重集应红；
2. **改字段**：`new_slaves` 的 `level` 或 `hp` 写差 1 ⇒ 标量块对比应红；
3. 变异必须**只红 angel 那两条**，其余 `real_rust_ledger_*` 不受影响。

## 3. 记录要求

- 变异点、红断言原文（含 `left/right`）、还原后 `git hash-object` 与改前相等、`MUTATION`=0；
- 更新 `mg42-angel-stub-impl.md` 的覆盖表与 manifest `notes`/`scope`（引用本卡）。

## 4. 非目标

- `Rng`（summon_ok 6 抽）与 640 的 `tag` 仍**不入比较面**（已登记：离线替身不抽）；本卡不改变该边界。
- 生产可召唤仍不成立（`monsters` 无 `精灵` 行，见 `openmir2-angel-summon-monster-missing.md`）。

## 5. 实验记录（2026-11-21，已完成）

**变异**：`lib.rs` 41/17/30 共用段的 `if outcome.magic_fire` 加 `&& req.magic_id != SKILL_ANGEL`
⇒ **只 41 不发芽 638**（17/30 不受影响；oracle 对应 `src/M2Server/Magic/MagicManager.cs:648-658` 的 `boSpellFire ⇒ SendRefMsg(RM_MAGICFIRE)`）。

- **driver**：`tests::angel_parity_summon_ok` / `tests::angel_parity_cap_hit` 均 **1 passed**（driver 不比帧）；
- **comparator**：`real_rust_ledger_angel` **红**——`tests/parity/tests/magic_fireball_parity.rs:2132`：
  `[summon_ok] caster 的 17/638/642 帧不一致：Rust [(642, 5, 1000, 0, "")] vs golden [(638, 101, 100, 257, "Nghcd?"), (642, 5, 1000, 0, "")]`
  / `left: [(642,…)]` / `right: [(638,…), (642,…)]`；
- **归类（`opc-19` 三态）**：driver 绿 + comparator 红 ⇒ **牙完整**（台账面能发现）；
- **还原**：`git hash-object` = 变异前 `1c9f963b41fcc19b6d4b5393e28d7feaccde09d2`、`MUTATION`=0；
  复跑 driver 两支 **1 passed**、comparator **1 passed**（同一 run id）。

⇒ MG-42 的 scope 已由"无牙"改为"牙：41 不发芽 638 ⇒ comparator 红"（manifest 同步）。
