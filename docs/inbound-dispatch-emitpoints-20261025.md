# 入站派发（R1 批）出帧发射点逐帧清单（L 片取证）

- 卡：`openmir2-inbound-dispatch-red-batch`（**L 片=取证**，owner `openmir2-dev-2`，branch `card/inbound-dispatch-emitpoints`）。
- 基线：`master = 007ad730a`（2026-10-25）。**只出取证表，不改代码、不改 RNG/时序/落库**。
- 范围：`game_svr::inbound::INBOUND_IDENTS` 的 **10 条**（`mir2-rs/bins/game-svr/src/inbound.rs:59`）
  + 生产派发臂 `handle_inbound_cmd`（`mir2-rs/bins/game-svr/src/lib.rs:2824`）。
- 口径：
  - **直连** = oracle `SendDefMessage` / `SendSocket`（当场写 socket，不进 `MsgQueue`）；
  - **入队** = oracle `SendMsg` / `SendUpdateMsg` / `SendRefMsg` / `SysMsg`（进 `PlayObject.MsgQueue`，
    handler 末尾按 `(byte)wIdent` 出队）；
  - 本仓多数入站帧是**单帧**，宿主 `tx.send` 立即写 socket——与 oracle「单帧队列出队」等价
    （没有同 handler 其它队列帧与之竞争时）。
- **R 片分界**：`CM_DROPGOLD` 的 `DropGoldDown/AddItemToMap` 深体（丢金币位置、物品落图、RNG）
  与 `CM_BUTCH` 的 `ButchEffect` 效果体（`MeatQuality/Skeleton` 抽数）**属 R 片**，本表只在
  「是否已经入队/是否有帧」层面记录，**不实现、不改动**（见 §3）。

## 1. 逐帧清单（10 条入户命令）

| # | CM（Rust 号） | Rust handler / 发射点 | Rust 机制 | 帧（SM） | oracle 直连/入队 | oracle 发送点 | 备注 |
|---:|---|---|---|---|---|---|---|
| 1 | `CM_TEST(1017)` | `mir2-rs/bins/game-svr/src/lib.rs:2856` `emit_def_frames` | **直发** | `SM 1`（空 def 帧） | 直连（`SendDefMessage(1,…)`） | `PlayObject.Message.cs:909-911` | 无门/无尾 |
| 2 | `CM_1042(1042)` | `mir2-rs/bins/game-svr/src/lib.rs:2859`（仅日志） | 无 | **零帧** | — | `PlayObject.Message.cs:996-998` | `LogService.Warn` |
| 3 | `CM_WANTMINIMAP(1033)` | `mir2-rs/bins/game-svr/src/lib.rs:2871` `emit_def_frames` | **直发** | `SM_READMINIMAP_OK(710)` / `_FAIL(711)` | 直连 | `PlayObject.Message.cs:969-971` → `Operate.cs:1204-1214` | `MinMap` 由 `inbound::session_min_map` 等价还原 |
| 4 | `CM_OPENDOOR(1002)` | `mir2-rs/bins/game-svr/src/lib.rs:2873`（无发帧） | 无 | **零帧** | — | `Message.cs:817` → `Operate.cs:363-374` + `WorldServer.cs:1445-1452` | **BUG-REPLICA 1**：守卫写反 ⇒ 恒零出帧 |
| 5 | `CM_ADJUST_BONUS(1043)` | 非法：`mir2-rs/bins/game-svr/src/lib.rs:2908` `tx.send`；合法：`mir2-rs/bins/game-svr/src/lib.rs:2999` 队列出帧（752→52→657）；广播 `:3003` | **真入队**（`NetPriorityQueue`；非法单帧 `tx.send`） | 非法：`SM_SYSMESSAGE(100)`；合法：`SM_SUBABILITY(752)`、`SM_ABILITY(52)`、`SM_CHARSTATUSCHANGED(657)`×1（本人）+ 657（±`SendRefMsgRange` 他人） | 入队（`SendMsg(RM_ABILITY/SUBABILITY)` `Base.cs:1174/1175`；657 `SendRefMsg`；SysMsg `BaseObject.cs:1678-1682`） | `Message.cs:999-1001` → `Base.cs:1156-1180` | 队列序 62→67→155；657 头-only 去重后净 1 条 |
| 6 | `CM_SITDOWN(3012)` | 成功：`mir2-rs/bins/game-svr/src/lib.rs:3030`（当前仅日志；本卡后补 `send_good_tick`）；失败：`mir2-rs/bins/game-svr/src/lib.rs:3043` `emit_def_frames` | 失败=**直发**；成功=**直发短包**（`SendRaw`，待补） | 成功：`+GD/<tick>`（`SendRaw`）+ `RM_POWERHIT`（见 §4）；失败：`SM_MOVEFAIL(28)` | 成功：`SendSocket(GetGoodTick)` + `SendRefMsg(RM_POWERHIT)`；失败：入队（`SendRefMsg(RM_MOVEFAIL)` `Message.cs:1018`） | `Message.cs:1284-1338`；`Operate.cs:343-360`（`SendRefMsg(RM_POWERHIT)` `:359`；`SendSocket(GetGoodTick)`） | **缺发帧 1/2**：`RM_POWERHIT` 发送缺失（接收端无 case，客户端零帧）；`+GD` 由 `openmir2-sitdown-powerhit-broadcast` 补 |
| 7 | `CM_DROPGOLD(1016)` | 拒绝：`mir2-rs/bins/game-svr/src/lib.rs:3086` `emit_def_frames`；成功：`mir2-rs/bins/game-svr/src/lib.rs:3095` | **直发**（`DefFrame` 单帧） | 拒绝：`SM_MENU_OK(767)`；成功：`SM_GOLDCHANGED(653)` | 均入队（`SendMsg(RM_MENU_OK)` `Base.cs:678`；`SendUpdateMsg(RM_GOLDCHANGED)` `Message.cs:528`） | `Message.cs:903-908` → `Operate.cs:222-249` + `BaseObject.cs:735-771` | **BUG-REPLICA 2**：`DropGoldDown` 恒 false ⇒ 净金币不变，但 653 照发；深体 RNG 属 R 片 |
| 8 | `CM_BUTCH(1007)` | 入队：`mir2-rs/bins/game-svr/src/lib.rs:3152`；效果：`mir2-rs/bins/game-svr/src/lib.rs:3145`；帧排水：`mir2-rs/bins/game-svr/src/lib.rs:14616` `downlink::butch_frame` | **世界命令 → `broadcast_ref_msg_with_self`** | `SM_BUTCH(637)`（对观察者，`downlink.rs:458-460`） | 入队（`SendRefMsg(RM_BUTCH)` `Operate.cs:749`；优先级 `10119&0xFF=55`） | `Message.cs:829-872` → `Operate.cs:702-751` | 效果体（`:718-745`，RNG）归 R 片；帧面已接 |
| 9 | `CM_USERMAKEDRUGITEM(1034)` | 入站兜底 `mir2-rs/bins/game-svr/src/lib.rs:3163`（零帧）；生产走商店：`mir2-rs/bins/game-svr/src/lib.rs:6365` `out = r.frames` → `mir2-rs/bins/game-svr/src/lib.rs:6375` `emit_shop_frames`（仓库路由同形在 `:6512`） | **直发**（`emit_shop_frames`；`SM_ADDITEM/SM_DELITEMS` 直连，`713/714` 经 oracle 队列但本宿主单批直发） | `SM_ADDITEM(200)`、`SM_DELITEMS(709)`、`SM_MAKEDRUG_SUCCESS(713)` / `FAIL(714)` | 混合（200/709 直连；713/714 `SendMsg` 166/167） | `Message.cs:972-974` → `Operate.cs:1216-1227` + `Merchant.cs:1432-1551` | 入站臂不可达（商店派发先命中）；帧体在 `shop::make_drug_item`（`shop.rs:512`） |
| 10 | `CM_PASSWORD(2001)` | `mir2-rs/bins/game-svr/src/lib.rs:3191` `tx.send(sessionwire::password_frame())` | **直发**（单帧 `tx.send`） | `SM_PASSWORD(5009)` | 入队（`SendMsg(RM_PASSWORD)` `Base.cs:3207/3304`） | `Message.cs:1397-1399` → `Base.cs:3190-3298` | `send_password_frame` 由 `inbound::process_client_password` 判定 |


## 2. 线缆映射（非出帧，已接）

- `CM_CLICKNPC(1010)` 的 `Recog=npcActorId` 线缆映射：`lib.rs:4638`（文档）/ `lib.rs:4998`
  （生产派发级，卡面给的 `5003` 是旧 base）。本表不含它（不是出帧点），仅确认已接。

## 3. R 片分界（本卡不碰）

| 面 | 归属 | 证据 |
|---|---|---|
| `CM_DROPGOLD` 的 `DropGoldDown` / `AddItemToMap` 深体（丢金币位置、物品落图、RNG） | **R 片**（接线即升 R） | `inbound.rs:32-34` BUG-REPLICA 2；`ground.rs` 模块头；reviewer-5 O-1 |
| `CM_BUTCH` 的 `ButchEffect`（`MeatQuality/Skeleton` 抽数） | **R 片** | `world.enqueue(Command::ButchEffect)` `lib.rs:3145`；`world.rs:7700` `butch_now` |
| 本卡 | **L 片**：只列已有发射点/缺帧；RNG/时序/落库零改动 | 本文件 |

## 4. 缺发帧登记（oracle 有、Rust 无；**只登记，不扩写集**）

| # | 命令 | 缺失帧 | oracle 证据 | 当前 Rust |
|---:|---|---|---|---|
| M-1 | `CM_SITDOWN` 成功支 | `RM_POWERHIT` **发送**（`SendRefMsg`，`Operate.cs:359`）——接收端**无 `RM_POWERHIT` case**（`PlayObject.Message.cs`/`BaseObject.Base.cs`），客户端零帧；缺口=server-side send | `Operate.cs:359` | `mir2-rs/bins/game-svr/src/lib.rs:3030` 只日志；`attackwire` 只映射 `RM_SPELL2→SM_POWERHIT` |
| M-2 | `CM_SITDOWN` 成功支 | `GetGoodTick` 直发字节（`SendSocket(string)`，`+GD/<tick>!`） | `M2Share.cs:205`；`Message.cs:1284-1338` | 真正发送器是 `send_good_tick`（`mir2-rs/bins/game-svr/src/lib.rs:13572`），当前已在 CM_TURN（`:9162`）等路径调用；SITDOWN 支未调用（本卡后补） |
| M-3 | `CM_USERMAKEDRUGITEM` | 入站臂零帧（生产走商店臂，帧在 `shop::make_drug_item`） | `Merchant.cs:1432-1551` | 非缺帧，属**路由**；已登记 §1 第 9 行 |
| M-4 | `CM_DROPGOLD` | `RM_ITEMSHOW` 广播（仅 `DropGoldDown` 成功时） | `BaseObject.cs:735-771` | BUG-REPLICA 2 ⇒ oracle 恒不发；**不是缺帧** |

> M-1/M-2 是**真实缺发帧**，按卡面要求逐条登记，本卡不扩写实现集；建议另开 L 卡
> `openmir2-sitdown-powerhit-goodtick`（帧 + 直连字节）。

## 5. 证据命令（可复跑）

```sh
# 入户表与派发臂
grep -n 'pub const INBOUND_IDENTS' -A 13 mir2-rs/bins/game-svr/src/inbound.rs   # :59
grep -n 'fn handle_inbound_cmd' mir2-rs/bins/game-svr/src/lib.rs                # :2824

# 各发射点（本表行号）
grep -n 'emit_def_frames(gates' mir2-rs/bins/game-svr/src/lib.rs                 # 2856/2871/3043/3086/3095
grep -n 'CM_ADJUST_BONUS 非法支\|CM_ADJUST_BONUS 回帧' mir2-rs/bins/game-svr/src/lib.rs  # 2906/2941
grep -n 'world.enqueue(mir2_world::Command::Butch' mir2-rs/bins/game-svr/src/lib.rs     # 3145/3152
grep -n 'downlink::butch_frame' mir2-rs/bins/game-svr/src/lib.rs                        # 14616
grep -n 'CM_PASSWORD/SM_PASSWORD' mir2-rs/bins/game-svr/src/lib.rs                      # 3189
grep -n 'emit_shop_frames(tx, socket, &out)' mir2-rs/bins/game-svr/src/lib.rs            # 6375（M-3；仓库路由 6512）
grep -n 'pub fn make_drug_item' mir2-rs/bins/game-svr/src/shop.rs                       # 512

# oracle 发送点
grep -n 'RM_POWERHIT' src/M2Server/Player/PlayObject.Operate.cs            # :359
grep -n 'SendUpdateMsg(Messages.RM_GOLDCHANGED' src/M2Server/Player/PlayObject.Message.cs  # :528
grep -n 'SendRefMsg(Messages.RM_MOVEFAIL' src/M2Server/Player/PlayObject.Message.cs        # :1018
grep -n 'SendMsg(Messages.RM_PASSWORD' src/M2Server/Player/PlayObject.Base.cs              # :3207/3304
```

## 6. 门禁 / 残留

- 本卡**零代码改动**（纯 docs）：卫生三门禁 + `git diff --check` + merge-tree（提交后复跑）见 patch body。
- 残留：无；R 片（DROPGOLD 深体、ButchEffect RNG）与 M-1/M-2 实现均明确留给后续卡。
- 不 merge、不自审。
