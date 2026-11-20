# `finish_spell_tail` 训练帧（640）延迟口径审计（后续项）

> 来源：MG-17（魔号 9 穿透）修 640 时发现——`id 9/10` 的尾延迟曾被错改成"立即出"，
> 根因是 C# 装置多 pass 排水的次序伪影。审计其余传 `None`（立即）的尾调用。

## oracle 事实

共享尾（`src/M2Server/Magic/MagicManager.cs:659-666`）：
`CheckMagicLevelUp` 未升级 ⇒ `SendSelfDelayMsg(RM_MAGIC_LVEXP, …, **1000**)`。
全仓 `RM_MAGIC_LVEXP`：22 个发射点 + 1 个处理器，**零处立即发**：
`SendSelfDelayMsg(…,1000)` ×2（`src/M2Server/Magic/MagicManager.cs:666`、
`src/M2Server/Player/PlayObject.Attack.cs:1210`）、`SendSelfDelayMsg(…,3000)` ×19
（`src/M2Server/Player/PlayObject.Attack.cs:209…571`）、`UpdateDelayMsg(…,800)` ×1
（`src/M2Server/Player/PlayObject.cs:5216`）。

## 待审站点（`finish_spell_tail(..., None)`）

> 行号按当前分支记录；散文锚用符号（`bins/game-svr/**` 属 HIGH_CHURN）。

| # | 站点（符号锚） | 上下文 | oracle 预期 |
|---|---|---|---|
| 1 | `SKILL_SHIELD` 支的 `finish_spell_tail` | shield(31) | 共享尾 ⇒ 640 = **1000ms** |
| 2 | buff-apply 邻接块的 `finish_spell_tail` | 14/15/31 邻近（arm 号待定位） | 共享尾 ⇒ **1000ms** |
| 3 | `44` 支的 `finish_spell_tail` | `SKILL_HBFIREBALL` | 共享尾 ⇒ **1000ms** |
| 4 | `24` 支的 `finish_spell_tail` | `SKILL_LIGHTFLOWER` | 共享尾 ⇒ **1000ms** |

## 影响与做法（供立项）

- **这四处当前也是"立即发"**；是否与 oracle 的 `SendSelfDelayMsg(…,1000)` 冲突，**需逐处核**
  （审计的人一进来先做这件事）。
- 若确认这四处也应为 1000ms：**每条的 golden 都要加 `StepDrainMs`（时间忠实排水）并重出**，
  因为它们现在的 golden 里 640 排在 31 之前同样是**多 pass 伪影**；随后逐条重跑比较器。
- **不要**只改产品而不同步 golden——那会把比较器直接判红（与本卡 MG-17 的教训相同）。
- 与 `openmir2-b4-multivictim-strike-order.md`（多受害者同刻帧序）是**两个独立问题**：
  本卡是"640 延迟值"，那张卡是"同刻多受害者帧序"。

## 逐处审计结论（2026-11-20，只读）

> 判据：共享尾 `MagicManager.cs:659-669` 的 `if (userMagic.Level<3 && boTrain) { … if(!CheckMagicLevelUp) SendSelfDelayMsg(RM_MAGIC_LVEXP,…,1000); }` ⇒ **640 延迟 1000ms**（未升级支）；升级支走 `UpdateDelayMsg(…,800)`（`PlayObject.cs:5216`）。**零处立即发**。`finish_spell_tail(..., None)` 的 `None` = **立即发**（`Some` 才按 1000/800 分支出帧）。

### 站点① `SKILL_SHIELD(31)` @ `bins/game-svr/.../lib.rs` `finish_spell_tail`（shield 支）
- **oracle**：`MagicManager.cs:474-479` `case SKILL_SHIELD: if (MagBubbleDefenceUp(...)) boTrain=true; break;` ⇒ 落 **共享尾 659-669** ⇒ 640 延迟 **1000ms**。**是**共享尾（非 Attack.cs 3000 支）。
- **Rust 现状**：该支 `finish_spell_tail(..., None)` = **立即发**。
- **判定：缺陷**。640 应在施法后 **+1000ms** 交付批；Rust 在施法批即发 ⇒ 640 帧的**批次/相对 638/642 次序**错（golden 里 640 排在 31 之前即多 pass 伪影）。
- **修法建议**：传 `Some(1000)`（走 oracle 1000/800 两分支）；**golden 须加 `StepDrainMs` 时间忠实排水重出**后重跑比较器（与 MG-17 同教训，勿只改产品）。

### 站点② `SKILL_HANGMAJINBUB(14)`/`SKILL_DEJIWONHO(15)` @ buff-apply 邻接 `finish_spell_tail`
- **oracle**：`MagicManager.cs:352-366` `MagMakeDefenceArea(...)>0 ⇒ boTrain=true; break;` ⇒ 落 **共享尾 659-669** ⇒ 640 延迟 **1000ms**。共享尾，非 3000 支。
- **Rust 现状**：`finish_spell_tail(..., None)` = 立即发。
- **判定：缺陷**（同上，14/15 两臂）；影响=640 批次/次序。
- **修法建议**：`Some(1000)` + golden `StepDrainMs` 重出。

### 站点③ `SKILL_44(44)` @ `finish_spell_tail`
- **oracle**：`MagicManager.cs:557-561` `case SKILL_44: if (MagHbFireBall(...)) boTrain=true; break;` ⇒ 落 **共享尾 659-669** ⇒ 640 延迟 **1000ms**。共享尾。
- **Rust 现状**：`finish_spell_tail(..., None)` = 立即发。
- **判定：缺陷**；影响=640 批次/次序。
- **修法建议**：`Some(1000)` + golden `StepDrainMs` 重出。

### 站点④ `SKILL_LIGHTFLOWER(24)` @ `finish_spell_tail`
- **oracle**：`MagicManager.cs:424-429` `case SKILL_LIGHTFLOWER: if (MagElecBlizzard(...)) boTrain=true; break;` ⇒ 落 **共享尾 659-669** ⇒ 640 延迟 **1000ms**。共享尾。
- **Rust 现状**：`finish_spell_tail(..., None)` = 立即发。
- **判定：缺陷**；影响=640 批次/次序。
- **修法建议**：`Some(1000)` + golden `StepDrainMs` 重出。

## 结论
四处**全部**为 oracle **延期 1000ms**、Rust **立即发** ⇒ **四处均为缺陷**（非"oracle 也立即"）。四者皆走共享尾 `MagicManager.cs:659-669`（非 `Attack.cs` 3000ms 支）。修复须"产品+golden(StepDrainMs)同步"，单改产品会判红。
