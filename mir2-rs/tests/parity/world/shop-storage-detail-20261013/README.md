# M3⑤-实现 4 第②片：物品详情列表（652）+ 仓库存/取回（1031/1032）+ 仓库面板/分页

> 卡：**`openmir2-m3-shop-dialog-wiring`**（M3⑤-实现 4，第②片；owner_hint = `openmir2-worker-11`）。
> 写集 `bins/game-svr/**`（第①片已落库存；本片只读复用 `crates/world` 的 `merchant_goods_groups*`）。
> 工作目录 `.claude/worktrees/m2-svr`（分支 `shop-wire-host`）；
> base = `refs/heads/master` @ **`793d3e71e`**（含 ⑤-4 第①片 `ee40c98a` / ⑤-5 `d4d5b89c`）。

## 0. 一句话

把第①片登记为「归第②片」的商店面补齐：`CM_USERGETDETAILITEM(1015)` → `SM_SENDDETAILGOODSLIST(652)`；
`CM_USERSTORAGEITEM(1031)`/`CM_USERTAKEBACKSTORAGEITEM(1032)` → `SM_STORAGE_OK/FULL/FAIL(701/702/703)`、
`SM_TAKEBACKSTORAGEITEM_OK/FULLBAG/FAIL(705/707/706)`；`@storage`/`@getback` 面板 →
`SM_SENDUSERSTORAGEITEM(700)` / `SM_SAVEITEMLIST(704)` 分页（每页 50）。

## 1. 本片覆盖（oracle 逐行）

| 命令/标签 | oracle 链 | Rust |
| --- | --- | --- |
| `CM_USERGETDETAILITEM(1015)` | `PlayObject.Message.cs:900-901`（与买入共用 `ClientUserBuyItem`）→ `Merchant.cs:1304-1343` | `shop::detail_goods_list` → `SM_SENDDETAILGOODSLIST(652)`（Recog=商人 ActorId，Param=nItemCount，Tag=nInt） |
| `CM_USERSTORAGEITEM(1031)` | `PlayObject.Message.cs:963-965` → `Operate.cs:1607-1654` | `shop::storage_item` → `701/702/703` |
| `CM_USERTAKEBACKSTORAGEITEM(1032)` | `PlayObject.Message.cs:966-968` → `Operate.cs:1656-1713` | `shop::take_back_storage_item` → `SM_ADDITEM(200)` + `705/707/706`（负重不足另发 `SM_SYSMESSAGE(100)`） |
| `@storage` | `Merchant.cs:862-865/1024-1027` → `PlayObject.Message.cs:1865-1866` | `shop::user_select`（`SM_SENDUSERSTORAGEITEM(700)`） |
| `@getback` | `Merchant.cs:867-873/1029-1032` → `PlayObject.Message.cs:1867-1868` → `PlayObject.Base.cs:1590-1620` | `shop::back_item_list`（`SM_SAVEITEMLIST(704)` 分页） |

口径（照搬，不修 bug）：

- **收包字段**：`Recog = nParam1`（商人 `ActorId`）、`MakeIndex = param | (tag << 16)`、
  `Msg` = tail；`CM_USERGETDETAILITEM` 的 `nInt`（页顶行）= 收包 `Param`（`processMsg.nParam2`，
  `PlayObject.Message.cs:901` 把 `nParam2` 传成 `nZz`）。
- **详情名比较**：`item.Name == sItemName`（`Merchant.cs:1320`，**ordinal/大小写敏感**，非 IgnoreCase）；
  仓库两条用 `string.Compare(..., OrdinalIgnoreCase)`（`Operate.cs:1623/1676`）。
- **详情页顶夹取**：`if (list20.Count - 1 < nInt) nInt = max(0, list20.Count - 10)`（`:1322-1323`）；
  取件恒从**组尾往前**最多 10 件（`:1324/1339`）。
- **详情尾双重编码**：每件 `EDCode.EncodeBuffer(ClientItem) + '/'`，再经 RM→SM 的 `SendDefMessage`
  施加 `EDCode.EncodeString`（`PlayObject.Message.cs:1830-1831` + `PlayObject.Socket.cs:26-43`）。
  `EncodeBuffer` 输出字节 ∈ `[0x3C,0x7B]`（ASCII，不含 `/`）⇒ `EncodeString` 的 GBK 往返无损，可按 `/` 切分。
- **`SM_SAVEITEMLIST` 尾直连**：`SendSaveItemList` 走 `SendSocket(ClientMsg, sSendMsg)`（`Base.cs:1616`），
  **不再** `EncodeString` ⇒ 新增 `ShopFrame.tail_encoded` 区分两种尾口径。整页模板全缺 ⇒ 该页不发。
- **仓库存入商人门**：`merchant.IsStorage && (同图 && IsWithinSight || FunctionNPC)`（`Operate.cs:1626`）；
  `FunctionNPC` 未建模（登记）。
- **存入 `bo19`**：`<39` 存成功 与 `>=39`（`SM_STORAGE_FULL`）**两分支都置 true**（`Operate.cs:1644-1645`）
  ⇒ 只有「未命中名/号」或「商人门不过」才落 `SM_STORAGE_FAIL`。
- **取回前置（无帧 return）**：`FindMerchant == null`（`:1658-1662`）与 `!IsCanGetBackItem`（`:1669-1673`）
  ⇒ **一个字节都不发**（`handle_storage_cmd` 照此提前 return）。
- **取回次序**：`AddItemToBag`（内含 `WeightChanged`）→ `SendAddItem` → `StorageItemList.RemoveAt`
  → `SM_TAKEBACKSTORAGEITEM_OK(Recog=nItemIdx)`（`:1683-1688`）；`SM_ADDITEM` 在 `OK` **之前**。
- **取回负重不足**：`SysMsg(CanotGetItems, Red, Hint)`（`:1704`）→ `SM_SYSMESSAGE(ActorId,
  MakeWord(RedMsgFColor,RedMsgBColor), 0, 1, "无法携带更多的东西!!!")`（`BaseObject.cs:1678` +
  `PlayObject.Message.cs:1714-1716`），随后 `bo19=false` ⇒ 再发 `SM_TAKEBACKSTORAGEITEM_FAIL`。
- **`WeightChanged`**：只重算 `WAbil.Weight`（`RecalcBagWeight`）⇒ Rust 同步写 `state.wabil.weight`，
  **不发** `SM_WEIGHTCHANGED`（T-47 ⑤ 登记）。

## 2. 本片登记（沿用 T-24 / T-47，未新增编号）

- **T-47 ①–③ 已落地**：详情列表（①）、两条仓库 `CM_*`（②）本片完成；③（`Market_Def` 脚本装载 +
  `GotoLable` 执行）仍登记（本宿主只回标签）。
- **T-47 ⑤**（商店路径 `SM_WEIGHTCHANGED` 未发）：本片仓库路径同样只改 `WAbil.Weight`、不发帧，
  仍在该条覆盖内。
- **T-47 ⑥ / T-24**（物品展示层）：详情/仓库列表用**模板名**（`CustomItemSystem.GetItemName` 的
  `Desc[13]` 自定义名未建模）；`SM_ADDITEM` 取回帧沿 T-24 只发 header-only（尾 = `EncodeBuffer(ClientItem)`）。
- **`PayMent == 1 && !Config.TryModeUseStorage` 门**（`Operate.cs:1615/1664`）：部署
  `E:\MirServer\M2GameSvr\Server.conf:11 TestServer=TRUE` ⇒ oracle `PlayObject.cs:1168-1171` 把
  `PayMent` 置 **3**（且 `TryModeUseStorage=0`）⇒ 该门在本部署**恒假**；本宿主未把 `PayMent` 落到
  游戏会话 ⇒ **行为暂等价**（登记，接 `PayMent` 消费方时补门）。
- **`Dealing` 前置门**（`Operate.cs:196-199`，详情与买入共用）：⑦ 未落地 ⇒ `Dealing` 恒 false，
  行为暂等价（T-47 ⑦ 既有登记）。

## 3. 改动点

- `mir2-rs/bins/game-svr/src/shop.rs`
  - 新增 `detail_goods_list` / `back_item_list` / `storage_item` / `take_back_storage_item` +
    `StorageResult` / `TakeBackResult`；`user_select` 增 `@storage`/`@getback` 支（新增
    `storage_items: &[BagItem]` 形参）。
  - `ShopFrame` 增 `tail_encoded: bool`；`frame()`（原文，`EncodeString`）/`frame_encoded()`（直连尾）。
  - `ShopContext` 增 `red_f`/`red_b`（取回负重不足的 `SM_SYSMESSAGE` 用）。
- `mir2-rs/bins/game-svr/src/main.rs`
  - 新增 `handle_storage_cmd`（仓库两条独立派发，允许「无商人/无帧」两态）；
    `handle_shop_cmd` 增 `CM_USERGETDETAILITEM` 支 + `colors` 形参；`shop_context` 透传颜色；
    `emit_shop_frames` 按 `tail_encoded` 决定是否 `EncodeString`。
  - 派发表增 `CM_USERGETDETAILITEM`（归 `handle_shop_cmd`）与 `CM_USERSTORAGEITEM`/
    `CM_USERTAKEBACKSTORAGEITEM`（归 `handle_storage_cmd`）。

## 4. 判据与改坏必红

- 构造级 5 条 + 进程级 1 条（见 `02-newtests.txt`）；`shop` lib **12 passed**、`emit_shop_frames` bin **2 passed**。
- 改坏必红 3 条（RC1 详情 10 件上限 / RC2 直连尾被重复编码 / RC3 取回负重门），
  均「mutated FAILED rc=101 → restored 1 passed rc=0」（`03-redchecks.txt` + `03-redcheck-*`）。

## 5. 门禁 / 边界

- 四门禁：`fmt rc=0` / `clippy --workspace --all-targets -D warnings rc=0` /
  `test --workspace` **94 suites 915 passed 0 failed 21 ignored** / `msgcodegen --check 604`（`04-*.txt`）。
- 边界：`git diff --name-only 793d3e71e -- src sql MirClient mir2-rs/crates/protocol mir2-rs/crates/storage` = **空**（`05-boundary.txt`）。

## 6. 未跑 / 等窗口

- **真机两侧同序列对拍**（`run_npc_shop_pair.ps1`，BotSrv `M3SHOPSEQ` 已含详情/仓库两条）需要独占窗口；
  本片零起栈、零占端口。
- 本片未动 `crates/world`/`crates/protocol`；库存面复用第①片 `merchant_goods_groups{,_mut}`。
