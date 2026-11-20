# `finish_spell_tail` 训练帧（640）延迟口径审计（后续项）

> 来源：MG-17（魔号 9 穿透）修 640 时发现——`id 9/10` 的尾延迟曾被错改成"立即出"，
> 根因是 C# 装置多 pass 排水的次序伪影。审计其余传 `None`（立即）的尾调用。

## oracle 事实

共享尾（`src/M2Server/Magic/MagicManager.cs:659-666`）：
`CheckMagicLevelUp` 未升级 ⇒ `SendSelfDelayMsg(RM_MAGIC_LVEXP, …, **1000**)`。
全仓 `RM_MAGIC_LVEXP`：22 个发射点 + 1 个处理器，**零处立即发**：
`SendSelfDelayMsg(…,1000)` ×2（`src/M2Server/Magic/MagicManager.cs:666`、
`src/M2Server/Combat/Attack.cs:1210`）、`SendSelfDelayMsg(…,3000)` ×19
（`src/M2Server/Combat/Attack.cs:209…571`）、`UpdateDelayMsg(…,800)` ×1
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

- 若确认这四处也应为 1000ms：**每条的 golden 都要加 `StepDrainMs`（时间忠实排水）并重出**，
  因为它们现在的 golden 里 640 排在 31 之前同样是**多 pass 伪影**；随后逐条重跑比较器。
- **不要**只改产品而不同步 golden——那会把比较器直接判红（与本卡 MG-17 的教训相同）。
- 与 `openmir2-b4-multivictim-strike-order.md`（多受害者同刻帧序）是**两个独立问题**：
  本卡是"640 延迟值"，那张卡是"同刻多受害者帧序"。
