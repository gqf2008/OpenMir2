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

**实测结论（`e5` 探针，`nondef_hit` seed=41）——落在"前 2 / 后 2 ⇒ 台账装配"**：

> - 累计 `pending_mag_struck = 2`、累计 `struck_log().len() = 2`；**逐排水调用是 1→1 两次**
>   （step 700 取 1 条、step 800 取 1 条）；
> - `struck_log` 里受害者 1002 有 **2 条**：`power 5 / damage 5 / hp 95` 与
>   `power 15 / damage 15 / hp 80` ⇒ **产品逐条产出、没有合并**；
> - **一行 `8103(damage=20)` 是驱动造的**：`delay_magic = all_outs.iter().find_map(|o| o.delay_magic)`
>   （**首条**）+ `damage = hp_before − hp_after`（**聚合**）+ `hp_after = entities.get(target).hp`
>   （循环后末值）⇒ 只 push 一帧；
> - **关键机制**：`let record_delayed_struck = matches!(magic_id, 9 | 10);` **排除 50**，所以魔号 50
>   走不到 per-event 的 8103 装配循环，只能落到上面那个 `find_map` + 聚合块。

**三条符号锚依据**：① `World::deliver_mag_struck` 对每条 `PendingMagStruck` 各 push 一条
`StruckEvent`（逐条、无合并）；② 驱动 `fireball_parity_drive` 的 RM `8103` 装配块（`find_map` +
`hp_before − hp_after` 聚合）；③ 同一函数里的 `record_delayed_struck = matches!(magic_id, 9 | 10)`。

**结论**：
- **第二条 `RM_DELAYMAGIC` 未入 `Rm`** = 台账装配（`self_delay_msgs.iter().find(...)` 单数）——装置缺口；
- **`8103(damage=20)`** = 台账装配（上述 `find_map` + 聚合块，且魔号 50 被 `9 | 10` 排除）——**装置缺口**，
  **产品侧 `deliver_mag_struck` 逐条逻辑是对的，不得按"产品合并"去改**；
- **时序 600/600（golden）vs 700/800（Rust）与 53 中间态 = 已判定：装置缺口（B17：golden 装置
  `DrainOnce` 偏离生产 `GetMessage`）**；**Rust 泵与生产同构、无需修改**。证据（源码亲核）：
  - oracle 生产 `BaseObject.Message.cs::GetMessage`：未到期队首 ⇒ `Enqueue(Lowest)` + `return false`，
    **遇未到点即停**；`ActorEntity.MsgQueue` = `PriorityQueue<SendMessage, byte>`（**只按 priority**）；
  - 偏离的是 golden 装置 `MagicGolden/Program.cs::DrainOnce`：`DrainRaw()` 取全部 → 逐条判到期 →
    到期的处理、未到期的 `PushBack` ⇒ **一步内处理所有到点项**；
  - ⇒ 装置在某一步里把两条 600ms 的到点项一起结算（golden 两条都是 600），而 Rust 泵按生产语义
    只能逐步推进（700/800），**两侧差异来自装置口径，不是产品**。
  - B17 归口卡：`docs/issues/golden-harness-blind-spots.md`（`e5` 在写）。
  - 受影响 stem（`89` 扫描，**引用不重判**）：MG-12/33/40/51/53 + `mabmabe-nondefault`；
    `pr1_*` 三条单测**正确地断言了生产语义、应保留**。
  **本卡的驱动/比较器不得为迁就而放宽**：不许放大 `due_ms_rel` 容差、不许跳过帧/接收者
  （该红线在"装置对齐生产"的新方案下同样成立）——**装置对齐生产后重出受影响 golden，再判本臂**。
  **定性沿革（留痕）**：①最初假设"600 被 650 挡后"→ ②按 golden 表现判"泵堆序（产品）"→
  ③`89` 读 C# 真机后定为 **B17 装置缺口**。对拍两侧一致时永远暴露不了模板偏离生产。

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
2. **受击播报"合并"——实为驱动装配（装置缺口，非产品）**：golden `RM_STRUCK_MAG(8103)` 两条
   `wParam=15`（p1=85）与 `wParam=5`（p1=80）；Rust 台账**只有一条** `wParam=20`（p1=80）。
   探针证明世界层 `struck_log` 有两条（5/15），一行 20 来自驱动的 `find_map` 首条 + `hp_before−hp_after`
   聚合（且 `record_delayed_struck = matches!(magic_id, 9|10)` 排除 50）。详见 §0/§3(b)。
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

### (a) 两条 `RM_DELAYMAGIC` 的交付时刻与 golden 不一致（**已判定：装置缺口 B17**）

- 现象：golden 两条 `due_ms_rel=600`；Rust 依次 700 / 800，并出现中间态 53（95 → 80）。
- 判定（源码证据，见 §0）：**Rust 泵与生产 `GetMessage` 同构（未到点即停 + 只按 priority 的队列），
  无缺陷**；偏离的是 golden 装置 `DrainOnce`（一步内处理所有到点项）。此前把它当"产品泵堆序"
  是按 golden 表现做的推断，已被 C# 真机核对推翻。
- 归口：[`golden-harness-blind-spots.md`](golden-harness-blind-spots.md)（B17，`e5` 在写）。
  本臂不为此放宽任何比较口径（不加 `due_ms_rel` 容差、不跳帧）；**装置对齐生产后重出受影响
  golden，再判本臂**。

### (b) 一条聚合 `8103(damage=20)`（**已实测：驱动装配，装置缺口**）

- 产品行为（**正确**）：`World::deliver_mag_struck` 逐条处理 `pending_mag_struck`，每条各记一条
  `StruckEvent`、各出 31/53。`e5` 探针：累计 `pending=2` / `struck_log=2`，逐排水 1→1 两次；
  `StruckEvent` 为 (power 5/damage 5/hp 95) 与 (power 15/damage 15/hp 80)。
- 台账成因：驱动 `fireball_parity_drive` 的 `8103` 装配用
  `delay_magic = all_outs.iter().find_map(|o| o.delay_magic)`（首条）+ `damage = hp_before − hp_after`
  （聚合）+ `hp_after = entities.get(target).hp`（循环后末值），只 push 一帧；并且
  `record_delayed_struck = matches!(magic_id, 9 | 10)` **排除魔号 50**，使 50 无法走 per-event 循环。
- 处置：**改驱动**（按 `struck_log` 逐条组装 8103，或把 50 纳入 per-event 口径），**不改产品**。

### (c) 第二条 `RM_DELAYMAGIC` 未进 `Rm`（**已确定是驱动台账缺口**，代码可证）

- 驱动 `fireball_parity_drive` 的 RM 组装段注释与实现是"`world.self_delay_msgs` 里 caster 的
  **那条** 8021"（单数）；实际成功臂有两条 ⇒ 只组装了一条。位置：`bins/game-svr/src/lib.rs`
  的 `fireball_parity_drive`（"RM 列组装"段，`RM_DELAYMAGIC(8021)` 那一行）。
- 处置：属于对照臂落地批的**驱动**修复（多条 8021 逐条组装），不影响产品判据。

### (f) （已并入 (b)）`8103(damage=20)` 的出处

`e5` 探针已定为驱动装配（前 2 / 后 2），不再单列；细节见 §0 与 §3(b)。

### (d) `SM 100` 载荷 `14591` vs `0`

- 双方都有 `SM 100`（`SM_SYSMESSAGE`）且 `series=1`，但 `param` 不同：golden 14591、Rust 0。
- 嫌疑：该帧来自 `drain_b4_self_deliveries`/`RM_POISON`（`BaseObject.Base.cs` 的 `MakePosion`
  `SysMsg` 颜色/文本参数），或 golden 的 `RedMsgFColor/RedMsgBColor` 打包（14591 = 0x38FF？）
  与 Rust 用的配置源不同。未定，随 (a)(b) 修完复测。

### (e) 帧编码时刻（批量刷 vs 应用时取值）

- oracle 的 31/53 在**出队编码时**读当前 `WAbil` ⇒ 两条受击都带最终 HP=80，53 只出现一次；
  Rust 在**应用时**出帧 ⇒ 95/80 两条、53 两条。
- **归属**：本项与 (a) 同源，归到 B17 装置缺口卡
  [`golden-harness-blind-spots.md`](golden-harness-blind-spots.md)，不另开定性。
  装置对齐生产（逐步结算）后重出受影响 golden 再看；若仍在，按 oracle 的"入队—出队编码"分层评估。

## 4. 结论与建议顺序

0. **形态判定已完成**（§0）：`8103` 与第二条 `8021` 都是**装置缺口/驱动装配**，产品逐条逻辑正确；
   **不得**按"产品合并"改 `deliver_mag_struck`。
1. **时序（a/e）已判定为装置缺口 B17**（golden 装置 `DrainOnce` 偏离生产；Rust 泵无缺陷），
   见 `golden-harness-blind-spots.md`；本批不动产品、不放宽任何比较口径，
   **装置对齐生产后重出受影响 golden（MG-12/33/40/51/53 + `mabmabe-nondefault`）再判本臂**。
2. (b)(c) 随对照臂驱动批一起改（8103 逐条组装 + 多条 8021）；
3. 修完后再把 `mabmabe-nondefault` 的驱动/比较器（per-stem 期望表、3 个 `#[ignore]` 驱动、
   `real_rust_ledger_mabmabe_nondefault`）从待落地补丁恢复进批——补丁副本见
   `E:/UserTemp/arm-rust-deferred.patch`（仓外临时物，非交付件）。

> **重同步注意（本支 golden 不得沿用旧值）**：装置对齐生产（B17）后，本支 golden **必须重出**——
> 31 帧 `tail` 的值会变。依据（bit 级解码）：`tail` 是 `MessageBodyWL`（`Param1=GetFeature`、
> `Param2=CharStatus`、`Tag1=施法者 ActorId`、`Tag2=1`）经 `EDCode.EncodePacket` 的编码；committed 与
> current 解码后只有 `Param2` 的 **bit31** 不同（`0x80000000` vs `0`），该 bit = `ActorBuffSystem.GetBuffStatus`
> 的 **buff 列表下标 0**（本支 STONE 毒）。旧装置一步内把 600/600/650 一起处理（毒先于两次受击应用）⇒
> 编码时带 buff 位；对齐生产按步结算 ⇒ 两次 31 在毒之前编码。**⇒ "只变顺序"对本支不成立，重出时连该值一起更新**。 

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
