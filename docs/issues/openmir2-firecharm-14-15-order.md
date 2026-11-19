# openmir2-firecharm-14-15-order

> 状态：**设计/证据卡（不实现）**，2026-11-20，由卡 `openmir2-magic-parity` 火符 13 修复（`9f420a94b`）
> 的"同类扫描"派生。**本卡只写设计与证据要求，代码改动另派**。

## 背景

`openmir2-magic-parity` 修火符 13 时确认：oracle 的护身符耐久帧 `RM_DURACHANGE(10125)`
与技能尾 `RM_MAGICFIRE(10120)` 进**同一** `ActorEntity.MsgQueue`，按 `(byte)ident` 出队：

- `(byte)RM_MAGICFIRE` = `10120 & 0xFF` = **136**
- `(byte)RM_DURACHANGE` = `10125 & 0xFF` = **141**
- `RM_MAGIC_LVEXP` 走 `SendSelfDelayMsg` 的 `MessagePriority.Lowest` = **255**（**不是** `(byte)10123` = 139）

⇒ 施法者自身帧序必须是 **`SM_MAGICFIRE(638) → SM_DURACHANGE(642) → SM_MAGIC_LVEXP(640)`**。
13 支已按此把 642 纳入同批 `SpellFrameBuf`（卡 `openmir2-magic-parity` 提交 `9f420a94b`）。

## 问题（14/15 路径同构缺陷，尚未修）

`SKILL_HANGMAJINBUB(14)` / `SKILL_DEJIWONHO(15)` 与 13 共用 `MagicManager` 的护身符门
case 组（`src/M2Server/Magic/MagicManager.cs:315` 起的外层组，`:325` 的 `UseAmulet`），
内层 `switch (MagicId)` 的 14/15 支同属该组。Rust 侧 14/15 仍把 642 **直发**（`UseAmulet`
时立即 `send_self_frame`），早于共享尾缓冲的 638 ⇒ 实际帧序为 `642 → 638 → 640`。

既有测试 `spell_tail_p2_defence_amulet_consumed_and_tail_fires`（`bins/game-svr/src/lib.rs`
内，断言 `i_spell < i_dura && i_dura < i_fire && i_fire < i_lv`）把**旧实现顺序当成了期望**——
按上面的 oracle 优先级链，该期望大概率**陈旧**。火符 13 的同类断言已在 `b60685e33` 按 oracle 改正，
并写了依据；14/15 的同类断言本卡**不擅改**（需先修产品，否则断言改动会是红的）。

## 设计（待实现卡）

1. **产品**：14/15 支的护身符耐久帧同样纳入该支的同一 `SpellFrameBuf`（与 13 的修法同构，
   按魔号隔离）。范围只到 `UseAmulet` 的 642 路径；13 支、其它 642 生产者（武器磨损等）不动。
2. **断言**：把上述 p2 用例的帧序断言改为 `638 → 642 → 640`，旁注写明优先级链依据与
   "旧期望是直发时代实现顺序、不是 oracle"（写法照 13 的 `b60685e33`）。
3. **证据要求（实现前先备）**：14/15 目前**没有**对拍 golden；需按 13 的 `--case firecharm`
   同构造 C# 夹具（护身符前置、`magic_id=14/15`、miss/edge/hit 语义按各自分支——
   14/15 无 AntiMagic 骰，应像治愈一样按行为分支持续记录），再走"红→修→绿+牙"流程。
4. **牙**：把 14/15 的护身符帧改回直发 ⇒ 修正后的顺序断言必红；复原后绿。
5. **回归**：改后必须复跑 `openmir2-magic-parity` 已有 6 条 verified 的全部驱动/比较器
   （id=1/2/5/11/13/45），并把 `spell_tail_p2_defence_amulet_consumed_and_tail_fires`
   与火符 13 的顺序用例一起跑绿。

## 不做

- 本卡**不实现**代码；不碰 `bins/game-svr/src/lib.rs` 的 14/15 支、不改任何断言。
- 不把 14/15 计入 `parity-gap-magic.json` 的 verified（在 golden 与驱动落地前）。
