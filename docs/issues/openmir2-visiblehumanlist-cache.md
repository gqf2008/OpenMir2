# openmir2-visiblehumanlist-cache（产品缺口卡：`SendRefMsg` 的 500ms 缓存 + `<11` 二次过滤）

> 卡：`openmir2-visiblehumanlist-cache`（tier=L；由 NP-11 多观察者 AOI 对拍暴露）。
> 来源：`openmir2-npc-multiobserve-golden` 的 `cache_window` 用例——oracle 在 500ms 窗口内
> 复用上次扫格结果，Rust 逐次重算 ⇒ 边界观察者**多收帧**（玩家可见面差异）。

## 1. oracle 事实（逐行）

`src/M2Server/Actor/BaseObject.Message.cs` 的 `SendRefMsg`：

```text
if (((HUtil32.GetTickCount() - SendRefMsgTick) >= 500) || (VisibleHumanList.Count == 0)) {
    SendRefMsgTick = HUtil32.GetTickCount();
    VisibleHumanList.Clear();
    // 扫 ±SendRefMsgRange(=12) 方阵；对 Race==Play 的 actor 逐个 SendMsg + 记入 VisibleHumanList
    return;
}
for (i = 0; i < VisibleHumanList.Count; i++) {
    target = ActorMgr.Get(VisibleHumanList[i]);
    if (target.Ghost) continue;
    if (target.Envir == Envir && Math.Abs(target.CurrX - CurrX) < 11 && Math.Abs(target.CurrY - CurrY) < 11)
        target.SendMsg(...);
}
```

- **第一段**（刷新）：`±12` **闭区间**扫格（`nLx..nHx`），收件人进 `VisibleHumanList`；
- **第二段**（缓存，500ms 内）：只对缓存列表重放，并加 `|Δ|<11` **严格小于**过滤。
- 后果：同一发送者在 500ms 内连发两条 `RM_TURN`（例：`Merchant.Run` 的转身/受击），
  **恰在 12 格边界的观察者只收到首条**；11 格内观察者两条都收。

## 2. Rust 现状（差异）

`mir2-rs/bins/game-svr/src/lib.rs` 的 `broadcast_ref_msg` / `broadcast_ref_msg_opt` /
`send_ref_msg_to_targets` 每次调用都走 `chat::hear_targets`（`<= range`，闭区间）**重算**收件人，
**没有**任何缓存或 `SendRefMsgTick` 等价物（全仓无 `send_ref_msg_tick`/`visible_human`）。

⇒ 边界观察者在 500ms 窗口内会**多收** oracle 不会发的第二帧及之后的帧。

## 3. 证据（金标准已入库）

- `mir2-rs/tests/parity/golden/npc/aoi/cache_window.json`（生成器 `NpcAoiGolden`）：
  `edge`（恰 +12 格）只收 `SM_TURN(10)`+`SM_FEATURECHANGED(41)` **一对**；`fresh_scan.json`
  同几何收 6 帧（10/41×2 + 14 + 30）。
- `np11_aoi_matches_oracle_golden` 把 `cache_window` 钉成**必须红**的绊线：若有人实现了缓存，
  该断言会失败并提示改本卡/偏差登记后重判。

## 4. 设计要点（实现时）

1. 按**发送者**（actor id）持有 `VisibleHumanList: Vec<i32>` + `SendRefMsgTick: i64`；
2. 两段语义逐字照抄：第一段扫 `±12` 闭区间并入册；第二段对缓存重放 + `|Δ|<11`；
3. 时钟 = sim 时钟（`world.clock_ms`），不入墙钟；Ghost 过滤保持；
4. 影响面：`broadcast_ref_msg*` 的全部调用点（NPC/商人转身受击、怪物动作、`RM_PUSH` 等）——
   属**可见面**变更，tier 与窗口判据按改动面重评；
5. 牙：`cache_window` 由红转绿 + `fresh_scan` 不回归 + 缓存过期（>500ms）后重新扫格的对照例。

## 5. 登记

- 偏差号：`T-287`（`mir2-rs/tests/parity/whitelist.md`）；
- NP-11 的 `verified` 覆盖声明**已排除**该子面（`notes` 写明"cache_window 未覆盖"）；
- 移除条件：本卡实现并窗口验证后，把 T-287 标为已消解、NP-11 覆盖声明补回该子面。
