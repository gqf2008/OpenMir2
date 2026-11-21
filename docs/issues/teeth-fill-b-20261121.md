# 批次 B 补牙记录：MG-15 红毒臂 + MG-12 HEALLING（2026-11-21）

> 来源：`E:/OPC/docs/teeth-audit-B.md`（批次 B 审计：MG-15 证据不足、MG-12 无牙）。
> 本卡只记录**产品侧变异重放**；变异已还原，产品代码零变化（两文件 sha 与 HEAD 相等）。
> 纪律：快照 → 变异 → 同 `MIR2_LEDGER_RUN_ID` 跑驱动 + 比较器 → `cp` 还原 → `git hash-object == git rev-parse HEAD:path` → `grep MUTATION` = 0 → 复跑绿。

## 0. 基线（变异前，全绿）

- `MIR2_LEDGER_RUN_ID=teeth-b-base-1795239541`
- `amyounsul2` 驱动 `tests::amyounsul2_parity_s1/s5/s13`：**3/3 rc=0**；比较器 `real_rust_ledger_amyounsul2`：**rc=0**。
- `healing` 驱动 `tests::healing_parity_heal/full/self`：**3/3 rc=0**；比较器 `real_rust_ledger_healing`：**rc=0**。

## 1. MG-15（Shape=2 红毒臂，`amyounsul2`）

- **变异**：`crates/world-bevy/src/magic.rs` 的 `power_poison`：`... * 2` → `... * 3 // MUTATION teeth-fill-b MG-15`（快照 `E:/UserTemp/teeth-fill-b-snap/magic.rs`）。
- **为什么**：`amyounsul2` 的 8037 = `(type=1, p1=80/62/70)`；绿毒面已有红（type=0 / 97→112），红毒臂从未咬到。
- **run_id**：`teeth-b-mg15-mut-1795239545`（驱动 3 支 + 比较器同一 id）。
- **驱动**：`tests::amyounsul2_parity_s1/s5/s13` **3/3 rc=0**——p1 不进驱动的条数断言，如实记“驱动绿”。
- **比较器**：`real_rust_ledger_amyounsul2` **rc=101**，3 条红原文：
  ```
  [s1/target] Rm 多重集不等（ident,wParam,p1,p2,p3,priority,late）： golden 有 rust 无=[[8037, 1, 80, 1001, 2, 255, 1]] rust 有 golden 无=[[8037, 1, 95, 1001, 2, 255, 1]]
  [s5/target] Rm 多重集不等： golden 有 rust 无=[[8037, 1, 62, 2001, 1, 255, 1]] rust 有 golden 无=[[8037, 1, 68, 2001, 2, 255, 1]]
  [s13/target] Rm 多重集不等： golden 有 rust 无=[[8037, 1, 70, 3001, 2, 255, 1]] rust 有 golden 无=[[8037, 1, 80, 3001, 2, 255, 1]]
  ```
  ⇒ `wParam=1`（DAMAGEARMOR）在比较键内；`p1` 80→95 / 62→68 / 70→80；s5 的 `p3` 1→2（`poison_point` 依赖 power）。
- **还原**：`cp` 快照；`git hash-object crates/world-bevy/src/magic.rs` = `git rev-parse HEAD:mir2-rs/crates/world-bevy/src/magic.rs` = `8ca6c6a0fffcadfdcb2cea83ce29c662885d3365`；`MUTATION` 残留 0。
- **复跑**：见 §3 最终绿。

## 2. MG-12（HEALLING(2)，治疗量）

- **变异**：`crates/world-bevy/src/world.rs` 的 `cast_healing_spell`（**不是** `cast_big_healing_spell` 的同名行，按最近前置函数名定位）：`let power = ...power_healing(...);` → `... + 1; // MUTATION teeth-fill-b MG-12`（快照 `E:/UserTemp/teeth-fill-b-snap2/world.rs`）。
- **为什么**：2 号治疗量经 RM 8034 `p1` 观测（heal 15 / self 20），此前无产品侧红；“最小可观测幅度从 +1 起”。
- **run_id**：`teeth-b-mg12-mut-1795239597`。
- **驱动**：`tests::healing_parity_heal/full/self` **3/3 rc=0**（驱动只判条数），如实记“驱动绿”。
- **比较器**：`real_rust_ledger_healing` **rc=101**，2 条红原文：
  ```
  [heal/target] Rm 多重集不等（ident,wParam,p1,p2,p3,priority,late）： golden 有 rust 无=[[8034, 0, 15, 0, 0, 255, 1]] rust 有 golden 无=[[8034, 0, 16, 0, 0, 255, 1]]
  [self/caster] Rm 多重集不等： golden 有 rust 无=[[8034, 0, 20, 0, 0, 255, 1]] rust 有 golden 无=[[8034, 0, 21, 0, 0, 255, 1]]
  ```
  ⇒ 最小 +1 幅度（15→16 / 20→21）可见；`full` 支无 8034，不受影响。
- **还原**：`cp` 快照；`git hash-object crates/world-bevy/src/world.rs` = `git rev-parse HEAD:mir2-rs/crates/world-bevy/src/world.rs` = `449022ae2f16d56e911550e87feac40da6695790`；`MUTATION` 残留 0。

## 3. 还原后的最终绿

- 两文件还原后再跑（`MIR2_LEDGER_RUN_ID=teeth-b-final-1795239618`）：
  - `amyounsul2` 驱动 `s1/s5/s13` **3/3 rc=0** + 比较器 **rc=0**；
  - `healing` 驱动 `heal/full/self` **3/3 rc=0** + 比较器 **rc=0**。

## 4. 命令（同一 run_id 贯穿驱动与比较器）

```
cd mir2-rs
export MIR2_LEDGER_RUN_ID=<本次 id>
cargo test -q -p game-svr --lib -- --exact --ignored tests::amyounsul2_parity_s1 --test-threads=1
cargo test -q -p game-svr --lib -- --exact --ignored tests::amyounsul2_parity_s5 --test-threads=1
cargo test -q -p game-svr --lib -- --exact --ignored tests::amyounsul2_parity_s13 --test-threads=1
cargo test -q -p mir2-parity-tests --test magic_fireball_parity -- --exact --ignored real_rust_ledger_amyounsul2
# healing 同理：tests::healing_parity_heal/full/self + real_rust_ledger_healing
```

## 5. 结论与清单

- **MG-15：有牙（补）**——产品侧 `power_poison *2→*3` ⇒ 比较器 Rm 8037 `type=1` / `p1` 红；驱动绿、比较器红已分记。
- **MG-12：有牙（补）**——产品侧 `cast_healing_spell` 治疗量 +1 ⇒ 比较器 Rm 8034 `p1` 红（heal 15→16 / self 20→21）。
- 清单 scope 在**下一笔**（manifest 独立提交）更新；本笔不动 status / 计数。
