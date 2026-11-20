# 交接：magic 伤害面补靶（2026-11-20，2d → 接手人）

> 本文件随同一提交入仓；分支 `card/magic-damage-teeth`（基于 `card/magic-worldengine @ 88613175c`）。
> **本批是半成品**：B 变异未跑、firecharm/lightflower 未修、cloak 两处未改。别当完成态。

## 0. 现状（已完成部分）
- `MagicSpec.CasterMcNoClamp`（`MakeWord(5,10)`）+ `RunMagicCase` 逐 case 覆写（`s5`/`s13`/`hit` 用变体，`s1`/`edge`/`miss` 保持 15 钳制支）；
- `FireballCase` 新增 `mc` 参数（缺省 15；仅 `hit` 支传变体）；
- 9 条分派加 `CasterMcNoClamp`；重出 10 份 golden；四条 selftest 全绿。
- **实测三支伤害**：8 条分叉 ✅（fireball/fireball-5/lightning-11/45/hbfireball：0/15/9；fireboom/47/snowwind：15/6/10）；**firecharm 0/15/15 ❌**（用错字段，见 P1）；**lightflower 1/0/1 ⚠️**（下限钳制，见 P1）。

## 1. cloak / big_cloak 的机制与修法（§4.2 第二种形态）
- 两处断言：`bins/game-svr/src/lib.rs:71181`（big_cloak）、`:71415`（cloak）——对**Rust 台账自己的 657** 断言
  `(param,tag,series)==(Some(0),Some(0),Some(0))`，注释写"与 golden 同源"。
- **机制**：它**不读 golden**，`Some(0)` 是从当时实现输出抄来的字面量；实现恒发 0 ⇒ 自证式绿；golden/实现任一变 7 就把对的行为判红。
- **修法**：改为**从 golden 读该收件人的 657 三元组再比**（同源化），不要改数字。

## 2. P0–P3（每条第一动作 / 判据）
- **P0 MG-51 groupdeding**：夹具无 `CasterDc` 字段 ⇒ 期望依赖 ctor 缺省（**输入不可追溯**）。第一步：加 `MagicSpec.CasterDc` 并显式写值，重出 `groupdeding.json`，**确认旧 case 期望未变**（若变，说明此前靠缺省蒙对，单独说明）。
- **P1 firecharm**：伤害式用 **SC**（`MagicManager.cs:337`）⇒ 换 `CasterScNoClamp`（同逐 case 机制）。
  **lightflower**：用 **MC**（`:425`），`s5=0` 是 `Hi>Lo` 后仍被目标 MAC 扣到 0 ⇒ **加大 Lo**（如 `MakeWord(10,15)`）让三支都 >0。
- **P2 治疗面（MG-12/40）**：判据 = **被治疗者 `TargetHp` 是否随 seed 分叉**；能分叉 ⇒ 加 `CasterScNoClamp`；不能 ⇒ 登记"治疗量在现夹具下不可观测"。
- **P3 MG-28/29（DefenceUp 强度来自 SC）**：先答"buff 强度值是否出现在任何被比较字段"（buff 帧 param / 后续伤害）；出现 ⇒ 加 SC 变体；不出现 ⇒ 登记"不可观测"。
- 已裁定不再动：MG-32/33/43/45(33)/46/59（非区间随机或已由其它面观测）；**MG-14/64、MG-15/65 已判非 duplicate，别重开**。

## 3. 退化面全表（机械口径）
13 条 = 原 10 条（fireball、fireball-5、lightning-11、lightning-45、firecharm、hbfireball、fireboom、fireboom47、snowwind、lightflower）+ MG-12 healing、MG-40 bighealing（SC 区间）+ MG-51 groupdeding（DC 未控）。
夹具：`CasterMc=15`、`CasterSc=15`（`Hi=0<Lo=15` ⇒ 钳制）。

## 4. B 的两发变异（未跑；变异绿 = 牙假）
```
# 变异①：Random(nPower+1) 恒 0（把伤害随机项打死）—— 新变体例必须红
# 变异②：Hi/Lo 传反 —— 必须红
```
命令模板：改 `MagicManager.cs`/`crates` 对应行 → `cargo test -q -p game-svr --lib -- --exact --ignored tests::<driver>` →
`cargo test -p mir2-parity-tests --test magic_fireball_parity -- --exact --ignored real_rust_ledger_<stem>`（同一 run_id）。
**变异红之前，任何人不得把这几条写成"已点亮"。**

## 5. 夹具位置
`mir2-rs/tests/parity/csharp/MagicGolden/Program.cs`（`MagicSpec`/`RunMagicCase`/`FireballCase`）、
`…/MagicGolden/OfflineWorldEngine.cs`（`IWorldEngine` 替身）、`mir2-rs/tests/parity/golden/magic/*.json`。
