# sm716-tie-order — 施法帧未进同一堆 ⇒ 同级 tie 序不可复现（已知偏差）

**状态**：known deviation（登记、不修）。
**发现**：2026-11-20，卡 `docs/issues/openmir2-magic-parity.md` §39（magic-3，`magic3/work` head `c4af73e91`）。
**裁定**：opc-19 2026-11-20——接受 39 用多重集比较（标注 `sm716_order_unverified`），
**不**现在改主机堆；修复另立本卡。

## 事实（机理）

- oracle 的 `MsgQueue` 是**四叉堆**：同一 actor 的 RM 按 `(byte)wIdent` 优先级出队，
  **同级**（例如 `RM_10205` 的 prio 221）的先后由四叉堆的入队/出队历史决定
  （§29：泵序 = 堆出队，不是稳定排序）。
- 装置的 `SpellFrameBuf::flush` 用 `NetPriorityQueue` 复现四叉堆；但它只装**进缓冲的帧**：
  - `RM_SPELL`（出 `SM_SPELL(17)`）在 `handle_spell_cmd_with_magic` 顶部由 `broadcast_ref_msg`
    **直发**，**不进** `SpellFrameBuf`；
  - `RM_DELAYMAGIC` / `RM_MAGIC_LVEXP` 走**自延迟队列**，也不进同一缓冲。
- ⇒ 对带 `RM_SPELL` 的接收者（caster 之外的可见玩家），221 组里少了一个（或多个）入队项，
  主机堆与 oracle 堆**不是同一个堆** ⇒ 同级 tie 序不可复现。
- **影响面不止 39**：任何会广播 `RM_SPELL`、且同等级广播帧 ≥3 条的施法路径（例如 42 群体雷电
  的多条 `RM_10205`）都可能有此差；既有 golden 只是尚未触发 ≥3 同级条。

## 现状处置（39）

- `groupdeding` 比较器对 **SM_716 组**做**多重集归一**（逐帧 `ident/param/tag/series/tail`
  仍严格），标注 **`sm716_order_unverified`**：帧字段严格、**组内顺序未验**。
- 其余 SM 与所有其他魔号仍**严格按序**。
- 39 的判据面主证据是 **RM 收件人集合 + 每只怪 HP 前后 + 逐目标命中门抽值**（不是 SM 组内序）。

## 修复方向（未做，需单独立项）

让 `SpellFrameBuf` 的 per-actor 堆包含 oracle 在**同一队列**里的全部入队项：
把 `RM_SPELL`（以及影响同级 tie 序的 `RM_DELAYMAGIC`/`RM_MAGIC_LVEXP` 的入队位次）按 oracle
入队序塞进**同一个** per-actor 堆再 flush；或由 driver 按 oracle 全消息序重建 per-actor 堆。

**代价**：触及共享施法尾 / 直发路径，改变所有魔号的帧序 ⇒ 需全量重跑 goldens 与多处比较器。
