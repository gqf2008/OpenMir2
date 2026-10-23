# GM `@清理垃圾`（`ClearBagItemCommand`）实现（T-218）

- 卡：`openmir2-gm-impl-clearbag`（tier=R）；分支 `card/openmir2-gm-impl-clearbag`（已合 master `43ce8dca`）
- 作者：`openmir2-worker-24`（返工：`openmir2-worker-65`，按 reviewer-3 `daa2368b`）；worktree `OpenMir2-wt-cronclaim` / `wt-worker-65`

## oracle（`ClearBagItemCommand.cs:13-54`）

- `:20-24` 空名或首字符 `?`（下标 0，不越界）⇒ 红字 `命令格式: @清理垃圾 人物名称`；
- `:25-30` 目标不在线 ⇒ 红字 `"{0} 现在不在线，或在其它服务器上!!!"`；
- `:31-46` 目标背包非空 ⇒ **倒序**收集 `DeleteItem{ItemName, MakeIndex}` + `Clear()`；空包**完全静默**；
- `:47-52` 目标 `SendMsg(RM_SENDDELITEMLIST)` ⇒ 泵 `SendDelItemList`（`PlayObject.Base.cs:1637-1645`）：
  `MakeMessage(SM_DELITEMS(709), 0, 0, 0, Count)` + `EncodeString("名/makeindex/…")`。

## 实现

- `GmSession.pending_clear_bag: Option<String>` + `Imp::ClearBagItem` + `clear_bag_item()`；
- **复审 important-2**：命中时存**实体规范名**（`entity.name.clone()`）。`play_by_name` 是
  OrdinalIgnoreCase（与 oracle `GetPlayObject` 同口径），而宿主按 `chr_name` **精确**匹配 ⇒
  存输入大小写会「命令说命中、宿主找不到目标」（静默不清包，pending 还被消费丢失）；
- **复审 blocking**：宿主 `apply_pending_gm_clear_bag()`（清目标会话 `item_list`、倒序组串、
  返回 709 帧体）改由 `collect_gm_clear_bag_pending()` 在 **sim 循环每 tick（`world.tick()` 前）**
  收口并发给**目标**——原先只在 `CM_SAY` 支 ⇒ 本卡唯一生产路（脚本 `GMEXECUTE`）从不落地，
  且 pending 会残留到该 GM 下一次任意聊天或被下一条 GM 命令静默丢弃。空包 ⇒ `None`（零帧）；
- `gm_registry.json`：合并后重算，只有 `sources.rust_table.sha256` 变化。

**登记（未接）**：`GetPlayObject` 的第三门（`IsPasswordLocked && ObMode && AdminMode`）未建模
（**对非管理员恒真**）；`ActorMgr.AddOhter(Sequence())` 是 oracle 内部句柄（不落 wire）⇒ 不建模。

## 用例（7）与牙（4）

表项；空名/`?` 帮助（**字面量 + GBK 字节**，不再拿常量自证）；离线文案（**GBK 字节**）+ 命中登记
**实体规范名**；**部署字面量形态**（`@清理垃圾 <$USERNAME>` ⇒ 一条红字、零 709、零清包）；
宿主收口（清包 + 709 帧 `Recog=0`/`Series`=件数/尾倒序 + 一次性 + 空包零帧）；
**混合大小写端到端**（`ViCtIm` → 规范名 → 收集器 → 709）；**脚本路 `@QF`**
（`GMEXECUTE 清理垃圾 self` ⇒ 收集器清包 + 709 到目标；`<$USERNAME>` 形态 ⇒ 一条红字、零 709）。

牙（实做并还原，sha256 一致）：

| 牙 | 改动 | 结果 |
| --- | --- | --- |
| 1 | 命令体不登记目标 | 离线用例红 |
| 2 | 宿主不清包 | 落地用例红 |
| 3 | **删 sim 循环的 `collect_gm_clear_bag_pending` 调用**（调用点保鲜） | `gm_clear_bag_apply_production_call_is_wired_in_sim_loop` 红 |
| 4 | **命中改回存输入串 `name.to_string()`** | 混合大小写用例红 |
| 5 | 帮助常量改字 | 字面量 + GBK 用例红 |

## 门禁

fmt / clippy `-D warnings` / `cargo test --workspace`（141 bins 0 FAILED）/ msgcodegen 604 /
anchor 511 / registry GREEN / T 预约表 GREEN / gm_registry `build+gate+selftest` GREEN。

## 窗口项（R，未跑）

判据**两例拆开**（`ActionOfGmExecute` 直接拼 `sParam1..6`、不调 `GetLineVariableText`，T-165）：

- **(a) 部署字面量形态**：`假人上线NPC-jiaren.txt:19` 的 `Gmexecute 清理垃圾 <$USERNAME>` ⇒ 参数
  **原样** ⇒ 两侧**各一条** `「<$USERNAME>」 现在不在线，或在其它服务器上!!!`，**零 709、零清包**；
  ⚠️ 旧判据「目标恰好一条 709」在这里会**误报**。
- **(b) `self` 形态**：`桃源-N109.txt:87` 的 `Gmexecute 清理垃圾 self` ⇒ `self` 折成 ChrName ⇒
  目标恰好一条 `SM_DELITEMS(709)`（倒序逐字节）+ 背包清空；空包两侧零帧；DB 后态 `hum_items` 全空。

窗口未跑前不 closed。未起栈/未占端口/未连库。

## 残留物

- `E:/tmp/w24-cb.sha`（牙基线）。
