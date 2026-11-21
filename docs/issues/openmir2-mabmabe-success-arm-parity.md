# 缺陷登记：50 `MabMabe` 成功臂 Rust/C# 不一致（非缺省对照臂暴露）

> **状态：只读定位，未改产品代码**。触发源是配置批的后续「非缺省对照臂」
> （[`magic-serverconf-nondefault-arm-plan.md`](magic-serverconf-nondefault-arm-plan.md)）：既有
> `mabmabe` 三支（s1/s5/s13）**全部未命中**，成功臂这条路径从未被对拍过；对照臂一进成功臂就红。
> 本批**不带**这条红：对照臂的驱动与比较器随本卡修复一起落地，master 不留红、不用 `#[ignore]` 掩盖。

## 1. 复现

```bash
# 1) 生成（非缺省夹具：50/40/90/7）
MagicGolden.exe --case mabmabe-nondefault --out <repo>/mir2-rs/tests/parity/golden/magic/mabmabe-nondefault.json
# 2) 驱动（台账落 target/magic-parity/mabmabe-nondefault_<label>.json）
MIR2_LEDGER_RUN_ID=<id> cargo test -q -p game-svr --lib -- --exact --ignored tests::mabmabe_nondefault_parity_hit
# 3) 比较器
MIR2_LEDGER_RUN_ID=<id> cargo test -q -p mir2-parity-tests --test magic_fireball_parity \
    -- --exact --ignored real_rust_ledger_mabmabe_nondefault
```

红只在 `nondef_hit`（seed 41）与 `nondef_randrate_flip`（seed 37）——即**进入成功臂**的两支；
`nondef_sucess_flip`（未进成功臂）帧面 gold == rust，说明差异来自成功臂链路。

## 2. 五条红原文（`nondef_hit`，golden vs Rust 台账）

台账：`mir2-rs/target/magic-parity/mabmabe-nondefault_nondef_hit.json`；
golden：`mir2-rs/tests/parity/golden/magic/mabmabe-nondefault.json`。

1. **时序**：`RM_DELAYMAGIC` 的两条到点帧，golden `due_ms_rel=600`（两条都在 600），
   Rust 依次 **700 / 800**。
2. **受击播报合并**：golden `RM_STRUCK_MAG(8103)` 两条 `wParam=15`（p1=85）与 `wParam=5`
   （p1=80）；Rust **只有一条** `wParam=20`（p1=80）。
   比较器原文：
   `[nondef_hit/caster] Rm 多重集不等（ident,wParam,p1,p2,p3,priority,late）： golden 有 rust 无=[[8103, 5, 80, 224, 1001, 167, 0], [8103, 15, 85, 224, 1001, 167, 0]] rust 有 golden 无=[[8103, 20, 80, 224, 1001, 167, 0]]`
3. **HP 快照/帧数**：golden target 侧 `53(80)` 一条、`31(80,15)`、`31(80,5)`；
   Rust `657(700)`、`100(700)`、`53(95,700)`、`31(95,5,700)`、`53(80,800)`、`31(80,15,800)`
   —— **多一条 53，且 31/53 带中间态 95**。
   比较器原文：
   `[nondef_hit/caster] Sent[3].param 不等：golden=80 rust=95`、`Sent[3].series 不等：golden=15 rust=5`、`Sent[2].due_ms_rel 不等：golden=600 rust=700`。
4. **`SM 100` 载荷**：golden `(14591, 1)` vs Rust `(0, 1)`（target 侧）。
5. **第二条 `RM_DELAYMAGIC` 未入 RM 台账**：golden caster `Rm` 有两条 `8021`（`wParam=15` 与 `5`），
   Rust 只有一条 `8021(5)`。

## 3. 证据点与嫌疑（函数 / 当前行为）

### (a) 同一时刻两条 `RM_DELAYMAGIC` 没有同步交付（时序 + 中间态的共同上游）

- 当前行为：魔法 50 在 `cast_mab_mabe_spell` 里**连发两条** `RM_DELAYMAGIC`（`power/3` 与 `n_power`，
  均 `delay=600`），并可能再发一条 `RM_POISON(650)`。
- 泵：`slave.rs` 的 `deliver_self_delay_msgs_impl` —— 逐 actor 泵四叉堆，**队首未到点时
  "出队 + 以 Lowest 重入队 + break"**（复刻 oracle `GetMessage`）。同一 actor 同时存在
  600/600/650 三条时，堆根序可能先出现 650 ⇒ 两条 600 被推迟，observed = 700/800。
- 待证：oracle 侧 `.NET PriorityQueue` 的根序下两条 600 是同一 `Run()` 内一起出队（golden 均为 600）。
  需要一条**只读探针**（在测试里打印 `self_delay_msgs` 的 `(ident, deliver_at, enqueue_seq)`
  与每次泵的出队序），不改产品代码。

### (b) 两条受击事件 vs 一条聚合 `8103(damage=20)`

- 当前行为：`World::deliver_mag_struck`（`crates/world-bevy/src/world.rs`）**逐条**处理
  `pending_mag_struck`，每条各记一条 `StruckEvent`、各出 31/53。
- 但台账 `Rm` 只有一条 `8103(20)`：要么两条 `PendingMagStruck` 被合并（功率相加），
  要么驱动侧 `struck_log` → RM 组装把它聚合了。驱动注释本身声明 RM 是**弱比较面**
  （"RM 是组装出来的，玩家可见证据是 SM"），所以这一条**可能是台账缺口而非产品**；
  需在 `drain_mag_struck_b2` 的返回值与 `World::struck_log()` 上探针确认（各几条、damage 各多少）。

### (c) 第二条 `RM_DELAYMAGIC` 未进 `Rm`（**已确定是驱动台账缺口**）

- 驱动 `fireball_parity_drive` 的 RM 组装段注释与实现是"`world.self_delay_msgs` 里 caster 的
  **那条** 8021"（单数）；实际成功臂有两条 ⇒ 只组装了一条。位置：`bins/game-svr/src/lib.rs`
  的 `fireball_parity_drive`（"RM 列组装"段，`RM_DELAYMAGIC(8021)` 那一行）。
- 处置：属于对照臂落地批的**驱动**修复（多条 8021 逐条组装），不影响产品判据。

### (d) `SM 100` 载荷 `14591` vs `0`

- 双方都有 `SM 100`（`SM_SYSMESSAGE`）且 `series=1`，但 `param` 不同：golden 14591、Rust 0。
- 嫌疑：该帧来自 `drain_b4_self_deliveries`/`RM_POISON`（`BaseObject.Base.cs` 的 `MakePosion`
  `SysMsg` 颜色/文本参数），或 golden 的 `RedMsgFColor/RedMsgBColor` 打包（14591 = 0x38FF？）
  与 Rust 用的配置源不同。未定，随 (a)(b) 修完复测。

### (e) 帧编码时刻（批量刷 vs 应用时取值）

- oracle 的 31/53 在**出队编码时**读当前 `WAbil` ⇒ 两条受击都带最终 HP=80，53 只出现一次；
  Rust 在**应用时**出帧 ⇒ 95/80 两条、53 两条。与 (a) 互为因果：若(a)修好（同时刻一起结算），
  本项大概率一并消失；若仍在，则需要按 oracle 的"入队—出队编码"分层重构该段。

## 4. 结论与建议顺序

1. **先修 (a)**（泵/堆序 + 同时刻多消息），再复测：预期 (e) 自动收敛、(b)(d) 可能随之消失；
2. (b) 若仍红，用探针区分"世界层合并"还是"驱动组装合并"；
3. (c) 随对照臂驱动批一起改（多条 8021）；
4. 修完后再把 `mabmabe-nondefault` 的驱动/比较器（per-stem 期望表、3 个 `#[ignore]` 驱动、
   `real_rust_ledger_mabmabe_nondefault`）从待落地补丁恢复进批——补丁副本见
   `E:/UserTemp/arm-rust-deferred.patch`（仓外临时物，非交付件）。

## 5. 边界

- 本卡**不**改 `crates/world-bevy` / 驱动 / 比较器；只登记行为与证据点。
- 既有 `mabmabe` 三支 golden 与台账**不受影响**（未命中路径）。
- 合并门不应出现这条红：本批不含对照臂驱动与比较器。
