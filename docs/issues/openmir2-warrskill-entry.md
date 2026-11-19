# openmir2-warrskill-entry

> 状态：**入口缺口的登记卡（不实现代码）**，2026-11-20，由卡 `openmir2-magic-parity`
> 的 18/28 接线期间发现。**本卡只登记入口缺口与验收要求，实现另派**。

## 背景：一个「看起来绿其实没测」的陷阱

`MagicManager.DoSpell` 的**第一句**就是战士门：

- `src/M2Server/Magic/MagicManager.cs:151-154`：`if (IsWarrSkill(userMagic.MagIdx)) return false;`
  （`IsWarrSkill` 见 `:82-104`，命中 3/4/7/12/25/26/27/34/38 九个魔号）。

⇒ 对派对拍装置 `tests/parity/csharp/MagicGolden` 现有的 `RunMagicCase`（**直调 `DoSpell`**）：
这 9 个魔号会得到 **`SpellReturn=false` + 空账本（零 RM/零 SM/零 RNG）**。
**这不是 oracle 的真实线上行为**——真实帧面由**调用方**产生：

- `ClientSpellXY`（`src/M2Server/Player/PlayObject.Attack.cs:1054`）的 `default` 支调用
  `DoSpell(...)`，返回 false 时发 `RM_MAGICFIREFAIL`（`:1274-1283`）⇒ 处理器
  `PlayObject.Message.cs:1794-1800`（**无自守卫**）⇒ `SM_MAGICFIRE_FAIL(639)` 给自己 + 范围内玩家。
- 9 个魔号里 **只有 3/4/7** 走 `default`（因此有 639）；**12/25/26/27/34/38** 在
  `ClientSpellXY` 有独立 `case`（`:1120/1136/1168/1187/1218/1234`），**根本不调用 `DoSpell`**
  （另有 `SKILL_43` 的 case `:1253`，它不在 `IsWarrSkill` 名单内）。

**结论**：用 `DoSpell` 直调产出的 golden 对这 9 条是**假零行为**；拿它对照 Rust——即便 Rust
也「零帧」——也**不能**证明对拍（两侧都错在同一个入口上）。

## 要求（补入口前禁止升格）

1. **装置**：`MagicGolden` 增加 `ClientSpellXY` 入口（构造与 `PlayObject.Attack.cs:1054`
   一致的调用上下文：`wIdent`、`nKey`、坐标、目标、`boLateDelivery`），并按魔号分派到
   `default`（3/4/7）或各自 `case`（12/25/26/27/34/38）。
2. **判据**：3/4/7 的 golden 必须体现**恰一条 639**（收件人含自己与范围内玩家、零 RNG、
   MP 不变、无 53、无训练尾）；12/25/26/27/34/38 的 golden 按其 `case` 臂的实际帧面生成
   （各自 `case` 的行为需先逐行读 `PlayObject.Attack.cs` 的对应块并写进卡面）。
3. **清单**：在入口与 golden 落地前，`parity-gap-magic.json` 的 **MG-01..MG-09 一律不得**
   标 `verified`；也不得用「Rust 也零帧」作为 `dead_in_oracle` 的依据（oracle 的真实线上帧面
   是 639 或该 case 臂的帧，不是"死"）。
4. **Rust 侧**：宿主 `default_arm && is_warr_skill` 块目前只覆盖 3/4/7 的 639
   （`bins/game-svr/src/lib.rs` 的注释已写明「9 个魔号中只有 3/4/7 进 default 支」）；
   12/25/26/27/34/38 的 `ClientSpellXY` 等价路径若缺失，属 `missing`（需登记偏差）。
5. **交付**：本卡实现的验收 = 「同一魔号、同一输入，`ClientSpellXY` 入口的 golden 与 Rust
   等价路径逐帧一致」；不得借用 `DoSpell` 的账本。

## 不做

- 本卡**不实现**任何代码；不动 `MagicGolden`、不动 Rust 宿主。
- 不把 MG-01..09 的 `status` 改成任何"已验证/死路径"。
