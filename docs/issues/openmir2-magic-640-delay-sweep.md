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
