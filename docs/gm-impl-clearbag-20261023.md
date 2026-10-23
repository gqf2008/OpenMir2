# GM `@清理垃圾`（`ClearBagItemCommand`）实现（T-218）

- 卡：`openmir2-gm-impl-clearbag`（tier=R）；分支 `card/openmir2-gm-impl-clearbag`，base `49983de07`
- 作者：`openmir2-worker-24`；worktree `OpenMir2-wt-cronclaim`

## oracle（`ClearBagItemCommand.cs:13-54`）

- `:20-24` 空名或首字符 `?`（下标 0，不越界）⇒ 红字 `命令格式: @清理垃圾 人物名称`；
- `:25-30` 目标不在线 ⇒ 红字 `"{0} 现在不在线，或在其它服务器上!!!"`；
- `:31-46` 目标背包非空 ⇒ **倒序**收集 `DeleteItem{ItemName, MakeIndex}` + `Clear()`；空包**完全静默**；
- `:47-52` 目标 `SendMsg(RM_SENDDELITEMLIST)` ⇒ 泵 `SendDelItemList`（`PlayObject.Base.cs:1637-1645`）：
  `MakeMessage(SM_DELITEMS(709), 0, 0, 0, Count)` + `EncodeString("名/makeindex/…")`。

## 实现

- `GmSession.pending_clear_bag: Option<String>`（目标名）+ `Imp::ClearBagItem` + `clear_bag_item()`；
- 宿主 `apply_pending_gm_clear_bag()`：`CM_SAY` 支收口——清目标会话 `item_list`、倒序组串、
  返回 709 帧体并发给**目标**；空包 ⇒ `None`（零帧）；
- `gm_registry.json` 重算（Implemented 98 → 99 / Unimplemented 87 → 86）。

**登记（未接）**：脚本路不落地待写缓冲（与 `@1/@2/@3` 的 `apply_pending_gm_entity` 同一限制）；
`ActorMgr.AddOhter(Sequence())` 是 oracle 内部句柄（不落 wire）⇒ 不建模。

## 用例（4）与牙（2）

表项；空名/`?` 帮助；离线文案 + 命中登记；宿主收口（清包 + 709 帧 `Recog=0`/`Series`=件数/
尾倒序 + 一次性 + 空包零帧）。牙：不登记目标 ⇒ 红；不清包 ⇒ 红（均还原、sha256 一致）。

## 门禁

fmt / clippy `-D warnings` / `cargo test --workspace`（141 bins 0 FAILED）/ msgcodegen 604 /
anchor 511 / registry GREEN / T 预约表 GREEN / gm_registry `build+gate+selftest` GREEN。

## 窗口项（R，未跑）

脚本 `假人上线NPC-jiaren.txt:19` / `桃源-N109.txt:87` 的 `Gmexecute 清理垃圾 <目标>` ⇒ 目标恰好一条
`SM_DELITEMS(709)`（倒序逐字节）+ 背包清空；空包两侧零帧；DB 后态 `hum_items` 全空。
窗口未跑前不 closed。未起栈/未占端口/未连库。

## 残留物

- `E:/tmp/w24-cb.sha`（牙基线）。
