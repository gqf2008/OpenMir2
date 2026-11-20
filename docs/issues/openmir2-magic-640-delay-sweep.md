# 全库 640（`SM_MAGIC_LVEXP`）延迟出口收口（follow-up 卡）

> 状态：**立项（未开工）**。本卡只登记与排期，不改产品代码。
> 来源：`openmir2-spell-tail-delay-audit` 修复批（`card/spell-tail-fix`）核查时发现
> **全库共 9 处 640 出口**，其中本批只改了 4 处。

## oracle 事实（唯一判据）

共享尾 `src/M2Server/Magic/MagicManager.cs:659-669`：

```text
if (userMagic.Level < 3 && boTrain) {
    if (userMagic.Magic.TrainLevel[userMagic.Level] <= playObject.Abil.Level) {
        playObject.TrainSkill(userMagic, M2Share.RandomNumber.Random(3) + 1);
        if (!playObject.CheckMagicLevelUp(userMagic)) {
            playObject.SendSelfDelayMsg(RM_MAGIC_LVEXP, …, 1000);   // :666
        }
    }
}
```

⇒ 640 **只能在 +1000ms 交付批**（升级支 `PlayObject.cs:5216` 为 800ms）。**任何在施法批直发的
640 都是缺陷**。

## 9 处出口总表

| # | 出口（`bins/game-svr/src/lib.rs`，master `1104c26f6` 行号） | 覆盖魔号 | 现状 | 本批 |
| --- | --- | --- | --- | --- |
| 1 | `finish_spell_tail(..., Some(1000))`（约 :14691） | 31 | 已修 | ✅ `card/spell-tail-fix` |
| 2 | 同（约 :14918） | 14/15 | 已修 | ✅ 同上 |
| 3 | 同（约 :15753） | B1：1/5/11/13/44/45 | 已修（本支无 640 ⇒ 帧面不变） | ✅ 同上 |
| 4 | 同（约 :15917） | B2：2/23/24/29/33/47 | 已修 | ✅ 同上 |
| 5 | B5 门 `6 \| 48 \| 9 \| 10 \| 22 \| 36` 内的 inlined 尾（出口约 :15134 `send_self_frame`） | 8 / 37 / 21（`SKILL_FIREWIND` / `SKILL_ENERGYREPULSOR` / `SKILL_SPACEMOVE`） | **立即发** | ❌ 待收口 |
| 6 | 召唤门 `SKILL_SKELLETON \| SKILL_SINSU \| SKILL_ANGEL` 内的 inlined 尾（出口约 :15285） | 17 / 30 / 41 | **立即发** | ❌ 待收口 |
| 7 | `if req.magic_id == SKILL_TAMMING` 内的 inlined 尾（出口约 :15394） | 20 | **立即发** | ❌ 待收口 |
| 8 | `if req.magic_id == SKILL_46` 内的 inlined 尾（出口约 :15504） | 46 | **立即发** | ❌ 待收口 |
| 9 | P1 default arm（`spell_tail_bo_train`）inlined 尾（出口约 :17233） | A 类 55/57/58/59/66–80（`boTrain=false` ⇒ 无 640）；B 类 49/51/52/53/54 | **立即发**（B 类） | ❌ 由 `c6` 的 `skill49` 批处理 |

> A 类零抽、无 640 ⇒ 本卡只涉 B 类（49/51/52/53/54）与 #5–#8。

## 待收口四处的现有 golden 与"被掩盖的缺陷"

### #5 `8 | 37 | 21`

- **golden**：无（`MagicGolden` 无对应 `--case`）。
- **行为**：效果体取数后立即 `send_self_frame(640)`。`boTrain`：8/37 取效果体返回值、21 恒 true。
- **影响**：线上 640 会早 1000ms 到客户端（训练点提示与其它交付帧相对次序错）。

### #6 `17 | 30 | 41`

- **golden**：`golden/magic/skeleton.json`（17，`summon_ok`）、`golden/magic/sinsu.json`（30，`summon_ok`）；
  41 无 golden（数据缺口见卡 `openmir2-angel-summon-monster-missing`）。
- **已存在的掩盖（实测）**：
  - `sinsu.json` 的 caster `Sent` 已带 `(638,0) (642,0) (640,1000)` —— **oracle 侧 640 在 1000ms**；
    Rust 台账把全部帧记 `due=0`（`sinsu_parity_drive` 用 `(s,b,0)`），而 sinsu 比较器的
    `keep` 只含 `17/638/642`（`sent_filter_sinsu`）⇒ **640 的 due 差被过滤面吃掉，当前绿不代表已验**。
  - `skeleton.json` 是更早世代（`Sent` 无 `due_ms_rel` 键）；临时用现行装置重生成得
    `640 due=1000`（**未提交**），说明 17 支同样属本缺陷，只是 golden 缺 due 键而看不出。
- **结论**：#6 两处（17/30）是**已被证据确认、但被比较面掩盖**的缺陷；41 待数据补齐后同改。

### #7 `20`（`SKILL_TAMMING`）/ #8 `46`（`SKILL_46`）

- **golden**：均无。
- **行为**：inlined 尾立即发 640。

## 收口做法（每处一致）

1. 产品：把该处 inlined 尾的 640 改成 `world.send_self_delay_msg_full(actor_id, RM_MAGIC_LVEXP, …,
   1000)`（升级支 800 + 去重，与 `finish_spell_tail_with` 的 `Some` 支同口径；或直接收敛到
   `finish_spell_tail_with(..., Some(1000))`）。
2. golden：受影响 scenario 加 `StepDrainMs=100` 后重出；**结构 diff 只允许** 640 `due=1000`、次序、
   `recog`/actor-id 键、`DrainPasses` 与墙钟 `delivery`。
3. 比较器：#6 必须把 640 纳入 `keep`（`sent_filter_sinsu` 现只比 17/638/642）；skeleton 同。
4. 牙：每处至少一颗「改回立即发 ⇒ 比较器红（640 due 或次序）」。
5. 门禁：四门 + `--lib` 三数 + `clippy --all-targets`；`lib.rs` 改动按合并顺序与其它批协同（同文件）。

## 不做的事

- 本卡外**不顺手改**这四处（等排期）。
- 不动 A 类（55/57/58/59/66–80）：`boTrain=false` ⇒ 无 640。
- 不动 `spell_tail_bo_train` 的 A/B 分派逻辑（B 类由 `c6` 的批处理）。

## 关联

- `docs/issues/openmir2-spell-tail-delay-audit.md`（本批审计与修复落地）
- `openmir2-spell-tail-delay-audit` 卡（本批 4 处）
- `c6` 的 `skill49` 批（#9 B 类）

## 评估（c6，2026-11-20，**只读**；基于 master `7fdf2fe95`）

> 本节只做评估，未改任何代码。行号均指 master `7fdf2fe95` 的 `bins/game-svr/src/lib.rs`。

### A. 逐处逐魔号的 oracle 事实

**施法（`CM_SPELL` → `DoSpell`）路径上，640 只有一个发射点**：共享尾
`src/M2Server/Magic/MagicManager.cs:666` 的 `SendSelfDelayMsg(RM_MAGIC_LVEXP, …, **1000**)`
（升级支由 `PlayObject.cs:5216` 的 `UpdateDelayMsg(…, **800**)` 覆盖，同 ident、先去重；
全仓另有 19 处 `SendSelfDelayMsg(…, 3000)` 在 `PlayObject.Attack.cs` 的**普攻**路径，与本卡无关）。
⇒ **四处 inlined 出口的正确行为一律是「+1000ms 交付批」**，不存在"别的延期值"。
差别只在 `boTrain` 的门：

| # | Rust 出口 | 魔号 | oracle `boTrain` 门 |
| --- | --- | --- | --- |
| 1 | `:15133` `send_self_frame` | 8 `SKILL_FIREWIND` | `src/M2Server/Magic/MagicManager.cs:259-264` `MagPushArround(...) > 0` |
| 1 | 同上 | 37 `SKILL_ENERGYREPULSOR` | `:601-607` 同上（`:611 boSpellFail=true`，但门在支内不置） |
| 1 | 同上 | 21 `SKILL_SPACEMOVE` | `:403-411` `MagSaceMove(...)` 为真（此支 `boSpellFire=false`，638 在支内先发） |
| 2 | `:15284` | 17 `SKILL_SKELLETON` | `:372-377` `MagMakeSlave(...)` 为真；且 `:322 boSpellFail=true`（护身符/目标门先过） |
| 2 | 同上 | 30 `SKILL_SINSU` | `:450-461`（同组 `:451 boSpellFail=true`） |
| 2 | 同上 | 41 `SKILL_ANGEL` | `:462-473`（同组 `:463 boSpellFail=true`） |
| 3 | `:15393` | 20 `SKILL_TAMMING` | `:394-402` `MagTamming(...)` 为真 |
| 4 | `:15503` | 46 `SKILL_46` | `:589-594` `MagMakeClone(...)` 为真 |

### B. 既有 golden 与「改完会不会红」（已机判）

| 魔号 | golden | caster 640 | `due_ms_rel` 键 | 比较器是否比 640 |
| --- | --- | --- | --- | --- |
| 17 | `golden/magic/skeleton.json`（summon_ok / cap_hit） | 有（`[638,642,640]`） | **无** | **不比**：`sent_filter` 的 `keep = [17,638,642]` |
| 30 | `golden/magic/sinsu.json`（summon_ok / cap_hit） | 有（due=1000） | 有 | **不比**：`sent_filter_sinsu` 同 `keep` |
| 8 / 21 / 37 / 20 / 46 / 41 | **无 golden** | — | — | 无比较面 |

结论：**改产品后，现有比较器一个都不会红**（四种情形都绿）。原因与「绿 ≠ 已验」同源——
640 被 `keep` 剪掉（17/30），或者根本没有比较面（其余六个魔号）。
另：`skeleton_parity_drive` / `sinsu_parity_drive` 都是**单次 `drain()`、无交付回路**；
改完后 640 会从台账里**直接消失**（落进自延迟队列），只是没人比它。

### C. `sinsu`（30）的口径决策 —— **报 opc-19 定，不自行决定**

**不能简单把 640 加进 `keep`**：640 的载荷 `tag/series = TranPoint`，而 TranPoint 来自
`Random(3)+1`；sinsu 的 RNG 因离线替身 `RegenMonsterByName` 造怪 0 抽而整体偏移
（实测 golden `tag=2` / Rust `tag=3`）⇒ 纳入 keep 会红在 **payload**，不是红在 due。
可选：

- **B1（推荐，成本最小）**：`keep` 不动；另加一条**只比 due** 的断言（640 恰 1 条且
  `due_ms_rel = 1000`），驱动补 1000ms 交付回路；payload 仍不比，理由写进 scope。
- B2：把 640 纳入 keep —— 需先修离线替身的造怪抽数（另一张卡），短期做不到。
- B3：sinsu 比较器面不动，只靠新单测覆盖 30 的 640 延迟（比较面仍留空）。

### D. `skeleton`（17）的额外成本

- 比较器同样 `keep` 掉 640；golden 是**旧世代**（整份无 `due_ms_rel`）。
- 要让改动可观测，需三件事一起做：①驱动加 1000ms 交付回路；②golden **按新 schema 重出**（带 due）；
  ③把 640 纳入 keep。②属"新世代 golden"，须按 `openmir2-golden-fixture-drift` 记录旧→新。
- 好消息：次序口径不用改——golden 的 caster `Sent` 是 `[638,642,640]`，改后 Rust 是
  施法批 `[638,642]` + 交付批 `[640]`，拼起来同序。

### E. 实施顺序建议（先小后大，每处先造红）

1. **#7（20）/ #8（46）**：无 golden、无比较面 ⇒ 用 P1 风格单测先造红，再改产品。
2. **#5（8/21/37）**：同上；但这一块的既有单测最多（firewind / spacemove / energyrepulsor 十余条），
   改前先 `cargo test -p game-svr --lib` 核一遍有没有"整串帧序"断言会被顺带打断。
3. **#6（17/30/41）最后**：先定 C 的口径、再做 D 的三件事；41 无 golden，可随批加单测。

### F. 牙（每处至少一颗，形态统一）

对 #5 / #7 / #8（及 41）：新增单测 `spell_tail_inline_<id>_is_delayed`，断言
（a）施法段 `drain()` 后该 socket 恰 **0** 条 640；
（b）`deliver_self_delay_msgs()` 跨 1000ms 后恰 **1** 条，且 `recog = MagicId`、`param = Level`、
`tag/series = TranPoint`；
（c）**反向牙**：把该处改回 `send_self_frame` ⇒ (a) 立即红。
对 #6：上述单测 + sinsu 按 C 的选项 + skeleton 按 D。

### G. 顺带发现（不属本卡范围，供排期）

- `finish_spell_tail_with` 里 `lv_exp_delay_ms == None` 的兜底分支（`:13975` 的 `send_self_frame`）
  现在**已无调用者**（本次机判：18 处 `finish_spell_tail(` 调用点尾参**全部** `Some(1000)`，
  `None` 为 0）⇒ 已成死兜底；建议四处收口后一并决定"保留作防御 / 删除"。
- 四处 inlined 块与 `finish_spell_tail_with` 是**重复实现**，每多留一处就多一个漂移源；
  建议收口时把四处收敛为 `finish_spell_tail_with(…, Some(1000))`，只保留一份实现。
