# openmir2-refmsg-cache-residual-sites（残余卡：`SendRefMsg` 缓存未接入的广播点）

> 卡：`openmir2-refmsg-cache-residual-sites`（由 `openmir2-visiblehumanlist-cache`（T-287）的 C2 拆分）。
> 背景：T-287 的 500ms `VisibleHumanList` 缓存已接入 `broadcast_ref_msg_opt`（含
> `broadcast_ref_msg` / `broadcast_ref_msg_with_self` / `..._with_self_at` 的内部转调）与
> `drain_merchant_disappears`；**其余按 `chat::hear_targets` 自算收件人的广播点仍未经过缓存**，
> 属明确残余（不是"已消解"）。

## 1. 口径

- oracle 侧：**所有** `SendRefMsg(ident,…)` 都走同一份缓存（`src/M2Server/Actor/BaseObject.Message.cs`
  的 `SendRefMsg`，`>=500` 重扫 `±SendRefMsgRange` 闭区间；否则复用快照 + `|Δ|<11` 严格小于）。
- Rust 侧：只有经 `broadcast_ref_msg_opt` / `drain_merchant_disappears` 的路径命中缓存；
  下列站点直接调 `chat::hear_targets` 或 `send_ref_msg_to_targets` ⇒ **每次重算、无缓存**。
- 可观测差异（共同形态）：同一发送者在 500ms 内连发两帧时，**恰在 `SendRefMsgRange`(=12) 边界**
  的观察者（以及窗口内新进范围的玩家）会**多收** oracle 不会发的帧；11 格内与范围外两侧一致。

## 2. 残余站点（逐条）

| # | 站点（file:符号） | 帧族 | oracle 锚 | 现行为 | 预期可观测差异 | 修法与牙 |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | `bins/game-svr/src/lib.rs::push_broadcast` | 施法尾帧族（ident `53`=SM_HEALTHSPELLCHANGED、`638`=SM_MAGICFIRE、`656`=SM_CHANGENAMECOLOR、`31`=SM_STRUCK、`1103`=SM_INSTANCEHEALGUAGE、`640`=SM_MAGIC_LVEXP） | `src/M2Server/Magic/MagicManager.cs` 的 `playObject.SendRefMsg(Messages.RM_MAGICFIRE, …)` | 每帧重算 | 施法同 tick 多帧 ⇒ 12 格边界多收后续帧 | 改用 `ref_msg_cache::targets`；牙：魔法线双侧 golden（需魔法线回归，故单列 C2） |
| 2 | `lib.rs::push_charstatuschanged` | `SM_CHARSTATUSCHANGED(657)` | `src/M2Server/Actor/BaseObject.cs` 的 `SendRefMsg(Messages.RM_CHARSTATUSCHANGED, …)` | 每帧重算 | 同窗口两帧 ⇒ 边界多收 | 同 1；牙：golden 边界观察者帧数 |
| 3 | `lib.rs::broadcast_charstatuschanged_except_self` | 657 | 同上 | 每帧重算 | 同上 | 同 1 |
| 4 | `lib.rs::flush_healthspell_slots` | `SM_HEALTHSPELLCHANGED(53)` | `src/M2Server/Actor/BaseObject.cs` 的 `SendRefMsg(Messages.RM_HEALTHSPELLCHANGED, …)` | 每帧重算 | 同上 | 同 1 |
| 5 | `lib.rs::drain_stamina` | 53 族 | 同上（`RM_HEALTHSPELLCHANGED`） | 每帧重算 | 同上 | 同 1 |
| 6 | `lib.rs::flush_run_frames` | 53 族 | 同上 | 每帧重算 | 同上 | 同 1 |
| 7 | `lib.rs::handle_say_with_saves` | `RM_HEAR`（`SM_HEAR`） | `src/M2Server/Actor/BaseObject.Base.cs` 的 `SendRefMsg(Messages.RM_HEAR, …)` | 每帧重算 | 连说两句 ⇒ 边界多收第二句 | 改用缓存；牙：说话两帧边界观察者 |
| 8 | `lib.rs::drain_slave_mutiny_downlink` | 叛变提示族 | 由 `SendRefMsg` 发出（宝宝叛变） | 每帧重算 | 边界多收 | 同 7 |
| 9 | `lib.rs::drain_combat_downlink`（两处：18513/18556） | 战斗/死亡族 | `SendRefMsg(RM_STRUCK/…)` | 每帧重算 | 同 tick 多目标结算 ⇒ 边界多收 | 同 1 |
| 10 | `lib.rs::drain_weapon_durability` | 武器耐久族 | `SendRefMsg`（耐久变化） | 每帧重算 | 边界多收 | 同 1 |
| 11 | `lib.rs::apply_pending_gm_target_ops` | GM 目标操作族 | `SendRefMsg`（GM 命令触发） | 每帧重算 | 边界多收 | 同 1 |
| 12 | `lib.rs::apply_pending_gm_guild_commands` | 行会/名牌族 | `SendRefMsg(RM_USERNAME/…)` | 每帧重算 | 边界多收 | 同 1 |
| 13 | `lib.rs::fan_out_npc_hear` | `RM_HEAR`（NPC 说话扇出） | `BaseObject.Base.cs` 的 `SendRefMsg(RM_HEAR, …)` | 每帧重算 | 连说两句 ⇒ 边界多收 | 同 7 |
| 14 | `lib.rs::broadcast_ref_msg_from_walk_candidates` | 走位候选广播（`RM_PUSH` 等） | `src/M2Server/Actor/BaseObject.cs` 的 `SendRefMsg(Messages.RM_PUSH, …)` | 每帧重算（候选集自算） | 逐步推挤 ⇒ 边界多收 | 改用 `targets_from_scan` + 牙 |
| 15 | `castle.rs::refresh_show_name_in_area` | `SM_USERNAME`（165 族） | `src/M2Server/Actor/BaseObject.cs` 的 `SendRefMsg(Messages.RM_USERNAME, …)` | 每帧重算 | 同窗口两次刷新 ⇒ 边界多收 | 同 1 |
| 16 | `guild_cmd.rs::ref_show_name` | `SM_USERNAME` | 同上 | 每帧重算 | 同上 | 同 1 |
| 17 | `guild_official.rs::handle_select` | `SM_USERNAME`（`RefShowName`） | `RM_USERNAME` 同上 | 每帧重算 | 同上 | 同 1 |
| 18 | `guild_official.rs::war_events_to_frames` | `SM_USERNAME` | 同上 | 每帧重算 | 同上 | 同 1 |

> 说明：opc-07 点名的 13 处即 #1–#12 与 #9 的两处；本表另列 #13/#14/#15–#18 共 6 处同类
> （`hear_targets` 直算），避免"数完了其实还有"。**两个例外不属 SendRefMsg、不列**：
> 纯 `SendMsg` 点对点发送（不进 `SendRefMsg` 的扫格面）。
>
> **已覆盖对照（不占残余编号）**：`drain_npc_merchant_turns` / `drain_npc_merchant_hits` 的
> 收件人走 `broadcast_ref_msg_opt`，已在 T-287 接入缓存；此处仅列作对照，不计入"残余 19 处"。
> （残余计数 = #1–#18 共 18 行，其中 #9 含两处站点 ⇒ 19 处。）

## 3. 修法（统一）

1. 每处把 `chat::hear_targets(world, id, range)` 换成 `crate::ref_msg_cache::targets(world, id, range)`；
   候选集形态（#14）用 `targets_from_scan(world, id, candidates)`。
2. 不同帧族若属**同一次** `SendRefMsg` 的多帧（#1 的 53/638/656/31/1103/640），只调用一次缓存判定、
   共用同一收件人列表（oracle 是同一趟 `SendRefMsg` 内 `Sendmsg` 多次，不重扫）。
3. 牙：对每处构造"同窗口两帧 + 12 格边界观察者"的 golden 或单测，断言边界只收首帧；
   §16 流程（快照/还原核 sha/复跑基线）。
4. 与魔法线交叉（#1 动 `push_broadcast`）需魔法对拍回归 ⇒ 排期由 opc-07 定。
