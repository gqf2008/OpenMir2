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

> 判据：共享尾 `src/M2Server/Magic/MagicManager.cs:659-669` 的 `if (userMagic.Level<3 && boTrain) { … if(!CheckMagicLevelUp) SendSelfDelayMsg(RM_MAGIC_LVEXP,…,1000); }` ⇒ **640 延迟 1000ms**（未升级支）；升级支走 `UpdateDelayMsg(…,800)`（`PlayObject.cs:5216`）。**零处立即发**。`finish_spell_tail(..., None)` 的 `None` = **立即发**（`Some` 才按 1000/800 分支出帧）。

### 站点① `SKILL_SHIELD(31)` @ `bins/game-svr/.../lib.rs` `finish_spell_tail`（shield 支）
- **oracle**：`src/M2Server/Magic/MagicManager.cs:474-479` `case SKILL_SHIELD: if (MagBubbleDefenceUp(...)) boTrain=true; break;` ⇒ 落 **共享尾 659-669** ⇒ 640 延迟 **1000ms**。**是**共享尾（非 Attack.cs 3000 支）。
- **Rust 现状**：该支 `finish_spell_tail(..., None)` = **立即发**。
- **判定：缺陷**。640 应在施法后 **+1000ms** 交付批；Rust 在施法批即发 ⇒ 640 帧的**批次/相对 638/642 次序**错（golden 里 640 排在 31 之前即多 pass 伪影）。
- **修法建议**：传 `Some(1000)`（走 oracle 1000/800 两分支）；**golden 须加 `StepDrainMs` 时间忠实排水重出**后重跑比较器（与 MG-17 同教训，勿只改产品）。

### 站点② `SKILL_HANGMAJINBUB(14)`/`SKILL_DEJIWONHO(15)` @ buff-apply 邻接 `finish_spell_tail`
- **oracle**：`src/M2Server/Magic/MagicManager.cs:352-366` `MagMakeDefenceArea(...)>0 ⇒ boTrain=true; break;` ⇒ 落 **共享尾 659-669** ⇒ 640 延迟 **1000ms**。共享尾，非 3000 支。
- **Rust 现状**：`finish_spell_tail(..., None)` = 立即发。
- **判定：缺陷**（同上，14/15 两臂）；影响=640 批次/次序。
- **修法建议**：`Some(1000)` + golden `StepDrainMs` 重出。

### 站点③ `SKILL_44(44)` @ `finish_spell_tail`
- **oracle**：`src/M2Server/Magic/MagicManager.cs:557-561` `case SKILL_44: if (MagHbFireBall(...)) boTrain=true; break;` ⇒ 落 **共享尾 659-669** ⇒ 640 延迟 **1000ms**。共享尾。
- **Rust 现状**：`finish_spell_tail(..., None)` = 立即发。
- **判定：缺陷**；影响=640 批次/次序。
- **修法建议**：`Some(1000)` + golden `StepDrainMs` 重出。

### 站点④ `SKILL_LIGHTFLOWER(24)` @ `finish_spell_tail`
- **oracle**：`src/M2Server/Magic/MagicManager.cs:424-429` `case SKILL_LIGHTFLOWER: if (MagElecBlizzard(...)) boTrain=true; break;` ⇒ 落 **共享尾 659-669** ⇒ 640 延迟 **1000ms**。共享尾。
- **Rust 现状**：`finish_spell_tail(..., None)` = 立即发。
- **判定：缺陷**；影响=640 批次/次序。
- **修法建议**：`Some(1000)` + golden `StepDrainMs` 重出。

## 结论
四处**全部**为 oracle **延期 1000ms**、Rust **立即发** ⇒ **四处均为缺陷**（非"oracle 也立即"）。四者皆走共享尾 `src/M2Server/Magic/MagicManager.cs:659-669`（非 `Attack.cs` 3000ms 支）。修复须"产品+golden(StepDrainMs)同步"，单改产品会判红。

## 范围更正（2026-11-20，动工前核对）

上面「逐处审计结论」按 **oracle case** 记站点，但 Rust 侧调用点是**共享尾**，实际覆盖面比单魔号大；动工时逐处核对了 `lib.rs` 的门：

- **站点③ = B1 共享尾**（门 `matches!(req.magic_id, 1 | 5 | 11 | 13 | 44 | 45)`）：覆盖 1/5/11/13/44/45 六魔号。**零行为变化**——这六支 golden 都没有 640（44 夹具 `UserMagicLevel=3` ⇒ `Level<3` 不成立；其余 `boTrain` 不成立），改它只多 `due_ms_rel` 键。
- **站点④ = B2 共享尾**（门 `matches!(req.magic_id, 2 | 23 | 24 | 29 | 33 | 47)`）：覆盖 2/23/24/29/33/47 六魔号。oracle 六 case 均 `boTrain = true; break;` 落共享尾（`:204-224` / `:418-423` / `:424-430` / `:443-449` / `:489-494` / `:595-600`）。**不能只给 24 加延期**——同一行调用点；按魔号分叉会把「oracle 只对 24 延期」写进代码，与原文矛盾。
- 站点①②仍是单臂（31；14/15 的护身符门内）。

## 修复落地（2026-11-20）

- **产品**：4 处 `finish_spell_tail(..., None)` → `Some(1000)`（1000/800 两支由 helper 内部分支）。
- **golden 重出 10 份**（`StepDrainMs=100` 时间忠实排水）：shield、hangmajinbub、dejiwonho、hbfireball、lightflower、fireboom、fireboom47、snowwind、healing、bighealing。
  - 变化面（机检）：640 的 `due_ms_rel=1000` 且 caster 序变为 `[638,31,656,640]`（31/656=600ms 在 640 之前）；新增 `due_ms_rel`/`recog`/actor-id 键；`DrainPasses` 随排水模型变（shield/14/15=2、hbfireball miss=1、四支 AoE=2、healing/bighealing 的 full=1 其余=2）；`Rng`/`TargetHp` **不变**；其余差异为墙钟 `delivery`。
- **装置口径**：施法批改为**排到空**（与步进循环同语义）。单趟会把 `LateDelivery=false` 的即时链（受害者 `RM_MAGSTRUCK`→`RM_STRUCK` 扇出）推到 100ms 步 ⇒ 非 640 帧 `due=100`，与 Rust 同拍出帧（`due=0`）不一致；排到空后施法拍统一 `due=0`。
- **驱动**：healing 专用驱动改为 100ms 步进排水并记录 `due_ms_rel`（此前单步 +1200ms、`Sent` 不带 due）；补 `drain_b3_self_deliveries` 让延迟 640 有帧。
- **牙（逐站点，含「红在哪测」）**：

| 站点 | 受影响 golden | 判别力（回 `None` 后红在哪/原文） |
| --- | --- | --- |
| ① 31 | `shield` | 比较器 3 条：`[s1/s5/s13 caster] Sent[2].due_ms_rel 不等：golden=1000 rust=0` |
| ② 14/15 | `hangmajinbub`、`dejiwonho` | 比较器各 3 条：`[s1/s5/s13 caster] Sent[51]`（hangmajinbub）/ `Sent[3]`（dejiwonho）`due_ms_rel 不等：golden=1000 rust=0`；另 `spell_tail_p2_*` 三条单测覆盖（施法批 0 / 交付 1） |
| ③ B1：1/5/11/13/44/45 | 该六支 golden **均不含 640**（44 夹具 `UserMagicLevel=3` ⇒ `Level<3` 不成立；其余 `boTrain` 不成立）⇒ **无 golden 可红** | 无 golden 侧红（帧面不变）；行为由 `spell_tail_p2_*` / `spell_p3b1_*` 单测覆盖。**不造假红** |
| ④ B2：2/23/24/29/33/47 | `lightflower`（其余五支同站点） | 驱动 3 条：`[lightflower/sN] caster 帧序 left=[638,640,31,656] right=[638,31,656,640]`；比较器 51 条（首条 `Sent[1].ident golden=31 rust=640`） |

  还原后 `git status` 0 行、无 MUTATION 残留。
- **回归**：`fire` / `bigcloak`（既有 StepDrainMs 支）驱动+比较器重跑仍绿。跨 stem 隔离核的 `fire`/`sinsu` 重出与提交版**幂等一致**；`bigcloak`/`skeleton` 为旧世代缺 `due_ms_rel`/actor-id 键（重出会多键），**未重出、未提交**，差异与任何批无关。
- **未动**：`spell_tail_bo_train`（49/51/52/53/54，另一批）。

## 已知限制（写进 scope，勿当「已验」）

- `Rm` 比较键 = `(ident, wParam, p1, p2, p3, priority, late)`，**不含 due** ⇒ `RM_MAGHEALING`(800ms)/`RM_MAGIC_LVEXP`(1000ms) 的**延期本身不判**，只判「发没发、载荷对不对」。延期/批次由 `Sent` 侧 `due_ms_rel` 判。
- `RM_MAGICHEALING` 已建模（`slave.rs` 常量、`drain_mag_healing_delivery`、驱动捕获、`Rm` 记账），oracle `Operate` 只累加 `IncHealing` 不发帧 ⇒ golden 记在 `Rm`，无 807 式不对称缺口。

## 全库 640 出口清单（2026-11-20，核查用；**本次只改前四处**）

oracle 共享尾 `src/M2Server/Magic/MagicManager.cs` 的 `:659-669` 是**唯一** 640 出口语义 ⇒ 全库每一处
出 640 的地方都必须是 `SendSelfDelayMsg(…,1000)`（升级支 800）。当前仓内共 **9 处**：

| # | 位置（`bins/game-svr/src/lib.rs`，行号为 master `1104c26f6` 时） | 覆盖魔号 | 现状 | 本批 |
| --- | --- | --- | --- | --- |
| 1 | `finish_spell_tail` 调用（约 14691） | 31 | 本批改 `Some(1000)` | ✅ |
| 2 | `finish_spell_tail` 调用（约 14918） | 14/15 | 同上 | ✅ |
| 3 | `finish_spell_tail` 调用（约 15753） | B1：1/5/11/13/44/45 | 同上（本支 golden 无 640 ⇒ 帧面不变） | ✅ |
| 4 | `finish_spell_tail` 调用（约 15917） | B2：2/23/24/29/33/47 | 同上 | ✅ |
| 5 | B5 门 `if matches!(req.magic_id, 6 \| 48 \| 9 \| 10 \| 22 \| 36)`（约 14966）内的 inlined 尾（约 15134 `send_self_frame`） | 8/37/21 等 | **仍立即发** | ❌ 未改 |
| 6 | 召唤门 `SKILL_SKELLETON \| SKILL_SINSU \| SKILL_ANGEL`（约 15147）内 inlined 尾（约 15285） | 17/30/41 | **仍立即发** | ❌ 未改 |
| 7 | `if req.magic_id == SKILL_TAMMING`（约 15325）内 inlined 尾（约 15394） | 20 | **仍立即发** | ❌ 未改 |
| 8 | `if req.magic_id == SKILL_46`（约 15419）内 inlined 尾（约 15504） | 46 | **仍立即发** | ❌ 未改 |
| 9 | P1 default arm（`spell_tail_bo_train`）inlined 尾（约 17233） | A 类 55/57/58/59/66–80（零抽无 640）/ B 类 49/51/52/53/54 | **仍立即发**（B 类另一批在改） | ❌ 未改 |

> 行号会随合并漂移；按门条件/函数名定位。5–8 建议单开一张 follow-up 卡收口（A 类零抽不受影响）。

## 跨 stem 影响核（装置「施法批排到空」的隔离证据）

该改动只在 `spec.StepDrainMs > 0` 分支生效（非步进支根本走不到），但仍做了**同装置双版本对照**：
用现行装置（排到空）与临时回退成单趟的装置，各重生成 `fire`（步进支既有）、`bigcloak`（步进支既有）、
`sinsu`、`skeleton`（非步进支对照），逐字段（除墙钟 `delivery`）比较：**四者全部完全一致**。
⇒ 该改动对未受本批影响的 stem **零语义变化**；差异只出现在 t=0 存在「即时消息链」的 4 支 AoE
（23/24/33/47）上。

> 注：`sinsu`/`skeleton` 的旧 golden 是更早世代（skeleton 无 `due_ms_rel` 键）；把它们按现行装置重生成
> 会多出 due 键（`640=1000`），那是**世代漂移**、不是本改动造成——两版装置之间零差异。

## 单测连带改动（9 条，均为非 ignore）

`lib.rs` 四处改 `Some(1000)` 后，9 个单测仍断言「640 在施法批」；按 oracle 改判据：

| 用例 | 旧期望 | 新期望 |
| --- | --- | --- |
| `spell_tail_p2_defence_amulet_consumed_and_tail_fires` | 施法批 911 有 640；序 `SPELL<FIRE<DURA<LVEXP` | 施法批 640=0（912 仍 0）；序 `SPELL<FIRE<DURA`；`deliver(1000)` 后 911 恰 1、912=0；载荷从交付帧取 |
| `spell_tail_p2_amulet_rounding_and_delete_boundaries` | 门过支施法批 911 有 640 | 施法批 0；`deliver(1000)` 后 1（门不过支仍 0） |
| `spell_tail_p2_shield_without_amulet_always_trains` | 门开 911 有 640；门关 0 | 门开施法批 0 + 交付 1；门关仍 0 |
| `spell_p3b1_fireball_family_draw_order_and_frames` | 911 有 640 | 施法批 0；末尾 `deliver(1000)` 后 1（**放在取数/对照流断言之后**：`deliver(1000)` 会执行交付段抽数——如 `RM_DELAYMAGIC` 的 MSD `R(1000)`——放在前面会把交付抽数算进施法段窗口，实测由 6 抽变 7 抽，且对照流全等断言也会红） |
| `spell_p3b1_lightening_family_draw_order_and_frames` | 同上 | 同上 |
| `spell_p3b1_firecharm_amulet_gate_tail_and_asymmetry` | 自身序 `638<642<640`；911 有 640 | 施法批序 `638<642`、640=0；`deliver(1000)` 后 1 |
| `spell_p3b1_hb_fireball_push_draws_follow_power` | 两支 911 各有 640 | 施法批各 0；交付各 1 |
| `spell_p3b1_training_gate_differs_by_exactly_one_draw` | 门开 911 有 640；门关 0 | 门开施法批 0 + 交付 1；门关仍 0 |
| `six_ids_enqueue_delay_magic_and_deliver_only_at_due_time` | 到点后 31 落地（单次 `deliver(1)`） | 到点后**有界补泵**直到 31 落地（见下条堆语义） |

测试辅助 `SpellFrameRig::deliver` / `P2Rig::deliver` 同步新增。

## 装置/驱动限制：.NET 四叉堆不稳定与补泵上界

oracle 每 tick 泵一次；.NET `PriorityQueue`（4 叉）**不稳定**：队根若是未到点项，「出队 + 以
`Lowest` 重入队」只结束本次泵（该项移到堆尾）。同优先级（`Lowest=255`）的
`RM_MAGIC_LVEXP`(1000ms) 与 `RM_DELAYMAGIC`(600/1200ms) 同堆时会互相挡路 ⇒ 单次泵可能一条都不投。

**上界依据（非试数）**：设本轮起点队列长度 q，则 q 次泵内每条消息都轮到过队根 ⇒ 所有到期项必已
投递或只剩「泵门」拦截项；因此测试辅助取上界 **q + 1**（另设 64 上限防御），队列清空即停。
