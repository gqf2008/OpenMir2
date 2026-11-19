# openmir2-magstruck-name-color

> 状态：**已立卡（2026-11-19，opc-07 批准实施）**——真语义差异，必须修，不是登记了事。
> 来源：`magic-parity-2d` 在火球真实台账对比中发现（RM 多重集比较暴露）。

## 现象

魔法受击链里，C# 会对**范围内观察者**发 `RM_CHANGENAMECOLOR(8090)`，进而出
`SM_CHANGENAMECOLOR(656)`；Rust 的 `deliver_mag_struck` 里 `name_color` 对玩家**恒 false**
⇒ 不发 656。

## 证据链（C#）

1. `src/M2Server/Player/PlayObject.Message.cs:1562-1572` —— `case RM_STRUCK / RM_STRUCK_MAG`：
   `wParam > 0` 且 `processMsg.ActorId == ActorId` 时，若 `nParam3` 的 actor 是玩家 ⇒
   `SetPkFlag(该玩家)`。
2. `src/M2Server/Player/PlayObject.cs:4326-4341` —— `SetPkFlag(IActor)`：当双方 `PvpLevel() < 2`、
   不在 `FightZone`/`Fight3Zone`、且自身 `!PvpFlag` 时，给**对方**置 `PvpFlag = true` 并调
   `targetObject.RefNameColor()`。
3. `src/M2Server/Actor/BaseObject.cs:730-733` —— `RefNameColor()`：
   `SendRefMsg(RM_CHANGENAMECOLOR, 0, 0, 0, 0, "")`（由编码层转 `SM_CHANGENAMECOLOR(656)`，
   编码器位置见 `PlayObject.Message.cs:1677-1678`）。

## Rust 现状

- `mir2-rs/bins/game-svr/src/lib.rs` 的 `deliver_mag_struck`（符号锚）：`if o.name_color { … sm_changenamecolor … }`；
  `o.name_color` 在受击投放的构造处恒为 false（玩家受击路径未按 C# 条件求值）。

## 影响

- 这是**语义差异**（少发一类帧）⇒ 在它闭环前 **`1/5` 不能标 `verified`**；
- 不影响 `magic-parity-2d` 的其他接线工作。

## 修复方向

在受击投放（`deliver_mag_struck` 的输入构造）处，**按 C# 同条件**求值：
`struck` 一方为玩家 ∧ 击打者 `nParam3` 是玩家 ∧ 双方 `PvpLevel()<2` ∧ 不在
`FightZone/Fight3Zone` ∧ 受击方 `!PvpFlag` ⇒ 置受击方 `PvpFlag=true` 且
`name_color = true`（并带上 `chr_color`）；其余情况 `false`。
**不得**放宽为"对所有人都发"。

## 验收

1. **该发**：构造上述条件全真的场景 ⇒ Rust 与 C# 都出 `SM_CHANGENAMECOLOR(656)`，
   帧字段逐项（ident/param/series/tag/tail）一致；
2. **不该发**：至少一条否定场景（如受击方已在 `PvpFlag`，或处于 `FightZone`）⇒ 两边都**不**出 656；
3. `cargo fmt/clippy/test` 三项 rc 贴出；合并在 QA 复核后 `--no-ff`。
