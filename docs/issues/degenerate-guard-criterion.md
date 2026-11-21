# 退化守卫判据：效果体"零次执行"必须具名红

> 用途：凡"夹具/实现看起来都对、但效果体一次都没执行"的卡（推挤族 8/37/47 首批），
> 验收里必须挂这条判据。由来见 §9（MG-50 四刀变异全绿 = 循环零次）。
> 本文所有 Rust 指针一律**符号锚**（`bins/game-svr/**` 属 HIGH_CHURN，行号禁止进散文）。

## 1. 判据（一句话）

**该 case 的 golden 与台账两侧，都必须含 ≥1 条效果帧**（推挤族 = `SM_BACKSTEP`）；
**任一侧为 0 ⇒ 具名红，禁止升格。**

## 2. 谁跑、跑什么

| 阶段 | 命令 |
| --- | --- |
| 驱动（采集台账） | `cargo test -p game-svr --lib tests::<stem>_parity_<label> -- --exact --ignored --test-threads=1`（`MIR2_LEDGER_RUN_ID` 与比较器同一 id） |
| 比较器（合并门） | `cargo test -p mir2-parity-tests --test magic_fireball_parity real_rust_ledger_<stem> -- --ignored` |

两侧都要挂：驱动侧防"实现零执行"，比较器侧防"golden 退化"（否则换个人跑比较器就绕过了）。

## 3. 放哪

- 驱动：`per_receiver_frame_seq_matches_golden(stem, label)` **之后**；
- 比较器：`real_ledger_verify_labels(...)` / `_opts(...)` **之后**，逐 label 各一次。

## 4. 可照抄实现（三处防"诊断退化"）

```rust
/// 退化守卫：golden 与台账两侧都必须含 ≥1 条效果帧（推挤族 = `SM_BACKSTEP`）。
/// 非推挤类卡：把 `EFFECT_IDENT` 换成本卡的效果帧 ident，其余照抄。
fn assert_effect_frames_non_degenerate(stem: &str, label: &str) {
    const EFFECT_IDENT: u16 = mir2_protocol::messages::SM_BACKSTEP; // 不要硬编码 9
    // **不要 `.unwrap()`**：字段缺失时 `unwrap` 会 panic 在 `Option::unwrap`，
    // 人只会看到 "called Option::unwrap on None"，看不出这是退化守卫。
    let arr = |v: &serde_json::Value, k: &str| -> &[serde_json::Value] {
        v.get(k).and_then(|x| x.as_array()).map(|a| a.as_slice()).unwrap_or(&[])
    };
    let count = |v: &serde_json::Value| -> usize {
        arr(v, "Receivers").iter()
            .flat_map(|r| arr(r, "Sent"))
            .filter(|m| m.get("ident").and_then(|x| x.as_i64()) == Some(i64::from(EFFECT_IDENT)))
            .count()
    };
    // helper 在本文件（符号锚）：`golden_case_for(stem, label)` 取 golden case，
    // `defence_case(stem, label)` 取台账 case（内部读 `<stem>_<label>.json` 并取 `v[0]`）。
    let g = count(&golden_case_for(stem, label));
    let c = count(&defence_case(stem, label));
    // **红消息带实际计数**：两侧都为 0 时才看得出"到底是夹具有问题还是实现没执行"。
    assert!(
        g >= 1 && c >= 1,
        "[{stem}/{label}] golden 推挤帧={g} 台账推挤帧={c}，必须各 ≥1 ⇒ 效果体零次执行（退化），禁止升格"
    );
}
```

**行号为什么不写进散文**：同一天里这两个 helper 在不同树上相差 3 行（本文写作时实测
`golden_case_for` 在 `lib.rs` 的 73800、`defence_case` 在 73866；另一棵树上是 73803 / 73869）
⇒ **一律用符号锚**，行号最多出现在代码块里当临时参考。

## 5. 红原文模板

```text
thread 'tests::firewind_parity_s1' panicked at bins\game-svr\src\lib.rs:<行>:<列>:
[firewind/s1] golden 推挤帧=2 台账推挤帧=0，必须各 ≥1 ⇒ 效果体零次执行（退化），禁止升格
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; ...
```

## 6. 空集语义（关键）

只写 **`>= 1`**（下限）。**不写**"不多于"/"若存在则…"——那类写法在空集上整体空过，
正是本守卫要消灭的形态（见 `tooth-must-answer-empty-input` 的教训）。

## 7. 双向牙（实现批必须各跑一次）

| # | 变异 | 必须红在 |
| --- | --- | --- |
| ① | golden 副本里删掉该 case 的 `ident = SM_BACKSTEP` 条目 | **golden 侧**的具名断言（`golden 推挤帧=0 台账推挤帧=N`） |
| ② | 8/37：去掉 `caster.UpdateVisibleGay(target)`；47：把目标挪出前格 | **台账侧**的具名断言（`golden 推挤帧=N 台账推挤帧=0`） |

**两条都红才算接线完成**——只红一条等于只接了一半（例如只挂了 golden 侧，换个实现照样零执行也不报）。

## 8. 适用范围与换 ident

| 卡 | stem | 效果帧 ident |
| --- | --- | --- |
| MG-16 抗拒火环(8) | `firewind` | `SM_BACKSTEP` |
| MG-57 气功波(37) | `energyrepulsor` | `SM_BACKSTEP` |
| MG-47 灭天火(35) | `windtebo` | `SM_BACKSTEP` |
| 其它"效果体可能零次执行"的卡 | 各自 stem | 该卡的效果帧 ident（其余照抄） |

## 9. 反例登记（为什么需要这条）

`docs/issues/openmir2-groupamyounsul-nondegenerate-golden.md`：MG-50 的 golden
`rng_total_draws = 0`、两侧无 `RM_POISON`、`TargetHp` 前后均 100；**四刀变异（毒伤系数 / 范围 +5 /
移除 proper-target 门 / 强制 poison dice 通过）全部保持绿** ⇒ 逐目标循环零次执行，
而帧面 `17`/`638` 逐帧对得上——**接线"验通"但效果体一次没跑**。本守卫就是防这个。

## 10. 边界

- 本文只定义判据，不含实现；实现批按 §4 照抄。
- 不改任何 golden、不改任何 stem 的既有驱动。
