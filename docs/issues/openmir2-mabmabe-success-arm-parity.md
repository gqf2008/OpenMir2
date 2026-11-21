# 缺陷登记：50 `MabMabe` 成功臂 Rust/C# 不一致（非缺省对照臂暴露）

> **状态：只读定位，未改产品代码；修复前必须先做 §0 的"形态判定"**——若判成台账装配，
> 本卡要**降级为装置缺口**并改写，不得直接改产品。触发源是配置批的后续「非缺省对照臂」
> （[`magic-serverconf-nondefault-arm-plan.md`](magic-serverconf-nondefault-arm-plan.md)）：既有
> `mabmabe` 三支（s1/s5/s13）**全部未命中**，成功臂这条路径从未被对拍过；对照臂一进成功臂就红。
> 本批**不带**这条红：对照臂的驱动与比较器随本卡修复一起落地，master 不留红、不用 `#[ignore]` 掩盖。

## 0. 修复前第一步：判形态（c6 提供，关键判别）

同一现象有两类完全不同的成因，修法相反；**先判形态，再改任何代码**：

> **判据**：看 `pending_mag_struck` 在**排水前**有几条、`struck_log` 在**排水后**有几条。
> - 排水前 N 条、排水后 **1** 条 ⇒ **产品合并**（`deliver_mag_struck` 把同受害者多条 pending 合算）⇒ 改产品；
> - 排水前 N 条、排水后 **N** 条、台账只记 1 条 ⇒ **台账装配** ⇒ 改驱动/装配，**不动产品**。

**已可由代码判定的部分**：
- **第二条 `RM_DELAYMAGIC` 未入 `Rm`** = **台账装配**（已证）。`fireball_parity_drive` 抓延迟载荷用
  `self_delay_msgs.iter().find(...)`（单数），同一 caster 有多条 `8021` 时天然只记首条。
- `8103` 的装配与 `struck_log` 逐条记录：`World::deliver_mag_struck` 在**每条** `PendingMagStruck`
  上各 push 一条 `StruckEvent`（代码逐条，无合并）；`drain_mag_struck_b2` 也逐条出 31/53。**因此
  "一条 `8103(damage=20)`"与代码读法矛盾**——必须实测"排水前 pending 数 / 排水后 struck_log 数"，
  不能先下"产品合并"结论。
- **时序 600 vs 700/800 与 53 中间态**：属"同一时刻多条 due 消息如何在泵内结算 + 出帧取 HP 的时刻"，
  同样需要该计数证据（泵是 `deliver_self_delay_msgs_impl`，队首未到点即重入队 + break）。

**结论**：本卡的 §3(b)(a)(e)(d) 目前只能标"**待实测判定**"；§3(c) 已证台账缺口。
若实测落在第二类，本卡整体降级为**装置缺口**，产品侧不动。

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

### (c) 第二条 `RM_DELAYMAGIC` 未进 `Rm`（**已确定是驱动台账缺口**，代码可证）

- 驱动 `fireball_parity_drive` 的 RM 组装段注释与实现是"`world.self_delay_msgs` 里 caster 的
  **那条** 8021"（单数）；实际成功臂有两条 ⇒ 只组装了一条。位置：`bins/game-svr/src/lib.rs`
  的 `fireball_parity_drive`（"RM 列组装"段，`RM_DELAYMAGIC(8021)` 那一行）。
- 处置：属于对照臂落地批的**驱动**修复（多条 8021 逐条组装），不影响产品判据。

### (f) `8103(damage=20)` 的出处（**待实测**）

- 代码读法：`deliver_mag_struck` 逐条 push `StruckEvent`、`drain_mag_struck_b2` 逐条出帧 ⇒ 两条 pending
  应得两条 `8103(5)`/`8103(15)`，与 observed 的一条 20 矛盾。
- 需要实测：**排水前 `pending_mag_struck.len()`** 与 **排水后 `struck_log().len()`**（两者都是世界层
  公开观测面，测试里读即可，不改产品）；若前者 1 后者 1 ⇒ 回到 `cast_mab_mabe_spell` 看第二条
  `SendSelfDelayMsg` 的 `w_param` 实际是多少；若前者 2 后者 1 ⇒ 产品合并；若前者 2 后者 2 ⇒
  查驱动 RM 装配是否还有别的聚合点。

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

0. **先做 §0 形态判定**（读测试可取的 `pending_mag_struck` / `struck_log` 计数）；结论若指向台账装配，
   本卡降级为装置缺口、改驱动而不动产品。
1. **再修 (a)**（泵/堆序 + 同时刻多消息），再复测：预期 (e) 自动收敛、(b)(d) 可能随之消失；
2. (b) 若仍红，用探针区分"世界层合并"还是"驱动组装合并"；
3. (c) 随对照臂驱动批一起改（多条 8021）；
4. 修完后再把 `mabmabe-nondefault` 的驱动/比较器（per-stem 期望表、3 个 `#[ignore]` 驱动、
   `real_rust_ledger_mabmabe_nondefault`）从待落地补丁恢复进批——补丁副本见
   `E:/UserTemp/arm-rust-deferred.patch`（仓外临时物，非交付件）。

## 5. 边界

- 本卡**不**改 `crates/world-bevy` / 驱动 / 比较器；只登记行为与证据点。
- 既有 `mabmabe` 三支 golden 与台账**不受影响**（未命中路径）。
- 合并门不应出现这条红：本批不含对照臂驱动与比较器。

## 6. §8.3d 判据口径（d8 独立复核 2026-11-21 的三项落字）

判据已改为**总量恒等式**（末帧 `param + Σseries == target_hp`；致死支 `Σseries >= target_hp`）。
独立复核确认其口径边界，写死如下（代码注释同款，见 `magic_fireball_parity.rs` 的 §8.3d 段）：

1. **本契约是「输入来源 + 总量自洽」检查，不是逐帧完整性检查**。**中间** `31` 的
   `param/series` 由 `verify()` 的跨栈逐帧比较守（golden vs Rust 台账逐值）——"契约绿"
   **不等于**"每帧都对"。
2. **致死支**：多事件致死只验「末帧 `param==0` + `Σseries >= target_hp`」，**不验逐事件致死**；
   前提 `target_hp > 0`（`target_hp==0` 时 `Σ>=0` 恒真，属既有弱面）。
3. **已知接受面**：`(85,15),(85,0)`（中间 `series=0` 帧）与 `(70,15)→(80,5)`（末帧 `param`
   错但 Σ 凑平）两种补偿形态本契约**不抓**。要抓中间帧就必须回到逐帧链式——那正是本次
   改动的目的所要避开的"批量刷新误判为输入/输出混淆"。**后来人不要为此再加牙把口径拧回去。**

Q2 定性（复核同意）：致死支从"单帧 `damage >= target_hp`"放宽到"`Σseries >= target_hp`"是
**有意修正**（多段小伤害致死时旧判据会误红合法 golden）；跨栈伤害面的正确性仍由 `verify()`
逐帧守，此放宽只作用于 golden 自洽检查。

## 7. 旧 stem 零漂移证据（送审复跑原文）

```
# 快照本次 merge 的目标 master（c31580a57）的生成器，放在仓外（src 用 junction 指回本树）
git archive c31580a57 -- mir2-rs/tests/parity/csharp/MagicGolden | tar -x -C E:/UserTemp/mgbase-c315
New-Item -ItemType Junction -Path E:/UserTemp/mgbase-c315/src -Target <本 worktree>/src
dotnet build E:/UserTemp/mgbase-c315/mir2-rs/tests/parity/csharp/MagicGolden -c Release
MAGICGOLDEN_EXPS_JSON=<本 worktree>/mir2-rs/tests/parity/fixtures/exps-need-exps.json   <base>/MagicGolden.exe --case <stem> --out E:/UserTemp/armcheck5/base/<stem>.json
<本 worktree>/mir2-rs/tests/parity/csharp/MagicGolden/bin/Release/MagicGolden.exe   --case <stem> --out E:/UserTemp/armcheck5/new/<stem>.json
# 归一化只剔墙钟字段 delivery / due_ms_rel 后逐字段比较
```

输出（9 个既有 stem）：

```
fireball: True
fireball5: True
shield: True
firewind: True
spacemove: True
amyounsul: True
groupamyounsul-shape2: True
snowwind: True
groupdeding: True
ALL_ZERO_DRIFT True
```
