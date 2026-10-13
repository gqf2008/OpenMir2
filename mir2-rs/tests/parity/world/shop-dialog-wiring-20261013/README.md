# M3⑤-实现 4 第①片：商店对话/查询售价/卖出/买入/查询修理成本/修理接线

> 卡：**`openmir2-m3-shop-dialog-wiring`**（M3⑤-实现 4）。`owner_hint = openmir2-worker-11`，
> 写集 `bins/game-svr/**` + `crates/world/**` + `crates/protocol/**`。
> 输入 = `openmir2-m3-npc-shop-align` 的盘点片（`mir2-rs/tests/parity/m3/M3⑤NPC商店现状盘点报告-20261013.md`）。
> 工作目录 `.claude/worktrees/m2-svr`（分支 `shop-wire-host`）；
> base = `refs/heads/master` @ **`15510a47`**（已含 ⑤-1 `d778ac30` / ⑤-2 `0caf5786` / ⑤-3 `5ff21013` / ⑥-3 `e1694c89`）。

## 0. 一句话

把 oracle 的商店「收包 → `FindMerchant` → `Merchant.Client*` → 价格公式 + 金币/背包/负重/库存
副作用 → `SM_*` 帧」链落到 Rust：新增 `game_svr::shop` 纯函数接缝（复用已合入的
`mir2_formula::shop` 取价/修理公式）+ `crates/world` 的在售库存（`MerchantState.goods_list`）+
`bins/game-svr` 的六条 `CM_*` 派发与 `[Setup]` 配置输入。

## 1. 本片覆盖（oracle 逐行）

| 命令 | oracle 链 | Rust |
| --- | --- | --- |
| `CM_MERCHANTDLGSELECT(1011)` | `PlayObject.Operate.cs:114` + `Merchant.cs:787`（`UserSelect`）| `shop::user_select`（`@buy`→`SM_SENDGOODSLIST(645)` / `@sell`→`646` / `@repair`/`@s_repair`→`668`）|
| `CM_MERCHANTQUERYSELLPRICE(1012)` | `Operate.cs:131` + `Merchant.cs:1345` | `shop::query_sell_price` → `SM_SENDBUYPRICE(647)` |
| `CM_USERSELLITEM(1013)` | `Operate.cs:162` + `Merchant.cs:1377`（`AddItemToGoodsList` `:1417`）| `shop::sell_item` → `648/649` + 库存回收 |
| `CM_USERBUYITEM(1014)` | `Operate.cs:192` + `Merchant.cs:1219` | `shop::buy_item` → `SM_ADDITEM(200)` + `650/651`，失败码 2/3 |
| `CM_MERCHANTQUERYREPAIRCOST(1024)` | `Operate.cs:1558` + `Merchant.cs:1555` | `shop::query_repair_cost` → `SM_SENDREPAIRCOST(671)` |
| `CM_USERREPAIRITEM(1023)` | `Operate.cs:1585` + `Merchant.cs:1599` | `shop::repair_item` → `669/670` + `@Superrepairfail` 的 `SM_MERCHANTSAY(643)` |

口径（照搬，不修 bug）：

- **收包字段**：`Recog = nParam1`（商人 `ActorId`）、`MakeIndex = param | (tag << 16)`
  （`HUtil32.MakeLong((ushort)nParam2, (ushort)nParam3)`，`PlayObject.Message.cs:891/894/939/942`）、
  `Msg` = tail。
- **距离 quirk**：`ClientUserBuyItem` 用 `|dx|>15 || |dy|>15` 拒（`<=15` 放行，`Operate.cs:200`），
  其余四条用 `IsWithinSight`（`<15`，`Operate.cs:1715`）⇒ 两者**不统一**、按 oracle 分别实现。
- **名称比较**：买 `sUserItemName.Equals(sItemName)` 大小写敏感（`Merchant.cs:1239`）；
  卖/查询/修理 `string.Compare(..., OrdinalIgnoreCase)`（`Operate.cs:139/169/1565/1590`）。
- **`GetUserItemPrice` 对 `StdMode==43` 写回 `DuraMax=10000`**（`Merchant.cs:1158`）：查询售价/查修理价/
  修理/买价四处都同步写回。
- **失败码**：`2` = 负重不足/背包满（`Merchant.cs:1234/1261/1276`）、`3` = 钱不够或价 <= 0；
  修理不支持 ⇒ **无帧 return**（`Merchant.cs:1602-1614`）。

## 2. 三条从 ⑤-3 转入的副作用（owner comment `270a3919`，DoD「实现」项逐条认领）

1. **`@Superrepairfail` 文案**：`SendMsgToUser("对不起!我不能帮你修理这个物品。\\ \\ \\<返回/@main>")`
   （`Merchant.cs:1612`）⇒ 产出 `SM_MERCHANTSAY(643)`（`RepairResult::sent_to_user`；尾 = `ChrName + '/' + sMsg`，
   锚 `NormNpc.cs:1613-1616` + `PlayObject.Message.cs:1722-1724`），用例 `repair_ok_and_superrepairfail_side_effects` 钉住。
   **返工订正（reviewer-5 F-1）**：原实现发的是 `SM_SYSMESSAGE(100)` + 玩家 Recog + 无 `ChrName/` 前缀
   ⇒ 已改为 `SM_MERCHANTSAY(643)` + **商人** ActorId + Param=Tag=0 + Series=1 + `ChrName + '/' + sMsg`。
2. **城堡税**：条件照抄 `CastleMerchant || Config.GetAllNpcTax`（`Merchant.cs:1249/1384/1638`；
   买/卖/修理三处）⇒ `shop` 里 `let _tax = m.castle_merchant || ctx.get_all_npc_tax;`；
   城堡/行会对象属 ⑧ 家族未建模 ⇒ **效果无目标对象**（登记 whitelist **T-47**）。
3. **`GotoLable(REPAIROK/SUPERREPAIROK)`**：`Merchant.cs:1652/1659` ⇒ `RepairResult::goto_label`
   （`~@repair` / `~@s_repair`）；本宿主尚未装载 `Market_Def` 商人脚本（`Merchant.LoadScriptFile`，
   `Merchant.cs:1089`）⇒ 只回标签 + 记 WRN，**不执行**（登记 T-47 ⇒ 第②片）。
4. 另 reviewer-5 的 **O-1**：`ScriptLable` 与 `@s_repair` 的比较用 `eq_label`（OrdinalIgnoreCase 同口径）。

## 3. 在售库存（⑤-2 登记的「库存 = 接缝」本片落地）

- `crates/world/src/merchant.rs`：`MerchantState.goods_list: Vec<Vec<MerchantItem>>`
  （`MerchantItem = mir2_formula::drop::UserItem`），语义对齐：
  `AddItemToGoodsList`（`:1417`，`Dura==0` 不收 + 同 `Index` 组 `Insert(0)`）、
  `ClientBuyItem` 取件（`RemoveAt(j)` + 空组 `GoodsList.RemoveAt(i)`）、
  `RefillGoods`（`:209`）：补货门按**实际库存量**走 `goodCout > nRefillCount`
  （`RefillGoodsItems` 新建 `delta` 件 + oracle `return`）/ `goodCout < nRefillCount`
  （`RefillDelReFillItem` 从尾删）/ `GoodsList` 超 1000 的收缩；`CopyToUserItemFromName` 口径 =
  `Index` 1-based、`Dura=DuraMax=模板 DuraMax`、`MakeIndex = M2Share.GetItemNumber()`。
- `World::item_number` 由私有改 `pub(crate)`，供 `make_merchant_item_by_name` 分配 `MakeIndex`。

## 4. 读数与判据

- **新用例 11 条全绿**（见 `02-newtests.txt`）：
  `game_svr::shop`: 7 条（买面板价/库存、卖面板、卖成功/超上限失败、买失败码 2/3 + 成功、
  查售价/查修理价、修理 OK/@Superrepairfail/无帧）；
  `mir2_world::merchant`: 4 条（`AddItemToGoodsList` 前插、空组取件移除、
  `RefillGoods` 建件、`RefillGoods` 尾删）。
- **四门禁（合并树）**：fmt rc=0 / clippy `-D warnings` rc=0 / msgcodegen 604 rc=0 /
  `test --workspace --no-fail-fast` **94 suites · 889 passed · 0 failed · 21 ignored**（见 `04-gates.txt`）。
- **改坏必红 4 条**（`03-redchecks.txt` + `redcheck/` 原始件）：mutation ⇒ FAILED(rc=101)，
  `git checkout` 还原 ⇒ ok/rc=0，还原后 `git status --porcelain` 空。
- **边界**：`git diff --name-only master..HEAD -- src sql MirClient` = **空**（`05-boundary.txt`）。

## 5. 逐条 DoD 落地（卡上 `machine_checkable_acceptance`）

1. **oracle 锚定** ✓ `00-oracle-anchors.txt`（grep -n，逐条可复跑）。
2. **实现/RNG** ✓ 八条命令中的六条按 oracle 逐行；**本片四条链均不消费 RNG**
   （买/卖/查询/修理都不调 `world.rng`）⇒ 不影响 Claim B 账本；零并行、同 tick 确定序。
3. **判据 + 改坏必红** ✓（§4；真身向量 = 复用 ⑤-3 的 `formula` 真身向量口径）。
4. **四门禁 + 证据四件套 + 边界** ✓。
5. **交付**：`patch` + `status: needs-review`；不 merge、未自签；真机对拍等窗口（登记在卡）。

## 6. 未做（登记 whitelist T-47，归本卡第②片）

- `CM_USERGETDETAILITEM(1015)` + `SM_SENDDETAILGOODSLIST(652)`（`Merchant.cs:1304`）；
- 仓库两条 `CM_USERSTORAGEITEM(1031)` / `CM_USERTAKEBACKSTORAGEITEM(1032)`（会话态 + 39 上限 +
  `IsCanGetBackItem` 锁 + `SendSaveItemList` 分页）⇔ `openmir2-m3-storage-persist`；
- `Market_Def` 商人脚本装载 + `GotoLable` 执行；
- 城堡税**效果**（城堡对象属 ⑧）；
- 商店路径的 `SM_WEIGHTCHANGED` 帧（`WeightChanged`，`BaseObject.cs:1771`）——本片只更新
  `WAbil.Weight`，帧面登记；
- `CustomItemSystem.GetItemName` 的自定义名（`Desc[13]`，与 T-24 同族）。

## 7. 残留物清单

- `E:/tmp/m2svr-target`（隔离 CARGO_TARGET_DIR，保留复用）。
- `E:/tmp/m2svr-gates-shop/**`（四门禁 + redcheck 原始件；已拷入本目录 `redcheck/`）。
- `E:/tmp/m2svr-redcheck-shop.py`（红检驱动，未入库）。
- worktree 干净；共享主检出 `E:/Users/gxh/Documents/GitHub/OpenMir2` 未在其内写/编译/提交；
  十端口保持开工前状态（仅共享 3306）。本片未起栈、未占端口。

## 8. 第①片返工（reviewer-5 `request_changes` @ `dd375e4e`，2026-10-13）

> 背景：reviewer-5 对本片 head `dd375e4e` 判 `request_changes`（F-1/F-2 Important + M-1 Minor）。
> 本返工只动 `bins/game-svr/**`（`shop.rs`/`main.rs`）+ whitelist T-47 + 本证据目录；不改 oracle、不碰 `crates/world`。

### F-1（Important）`@fail_s_repair` 说词帧四处口径
- **改前**：`ident=SM_SYSMESSAGE(100)`、`Recog=ctx.player_actor_id`（玩家）、`series=0`、尾无 `ChrName/` 前缀。
- **改后**：`ident=SM_MERCHANTSAY(643)`、`Recog=m.actor_id`（**商人** ActorId）、`Param=Tag=0`、`Series=1`、
  尾 = `downlink::gbk("<商人ChrName>/<sMsg>")`。`MerchantView` 新增 `chr_name`（宿主从世界实体 `me.name` 取）。
- 锚：`Merchant.cs:1612` → `NormNpc.cs:1613-1616`（`ChrName + '/' + sMsg`）→ `PlayObject.Message.cs:1722-1724`
  （`SM_MERCHANTSAY(643)`, `MakeWord(0,0)`, `Series=1`）。
- 用例（重钉）：`shop::tests::repair_ok_and_superrepairfail_side_effects`（商人 id 与玩家 id 取不同值 `5555` vs `4242`；
  断言 ident/recog/param/tag/series/尾）；改坏必红 **RC5**（回退旧帧 ⇒ FAILED，`left: 100 right: 643`）。

### F-2（Important）`CM_USERREPAIRITEM`「找不到件」分支按 `BUG-REPLICA` 收口
- oracle `ClientRepairItem`（`Operate.cs:1585-1597`）的循环体 `userItem = ItemList[i]` **无条件**赋值，
  未命中时循环结束后 `userItem` 仍是**最后一件**（非 null）⇒ oracle 会继续修**最后一件**；只有空背包才 return。
  （同源的 `ClientQueryRepairCost`（`:1558-1583`）用 `userItemA = null` 初值 ⇒ 未命中 return；**只有 repair 有这条 bug**。）
- 按 §1 硬约束③ 1:1 照搬：新增 `repair_target_index`（= `find_bag_item_by_make_index(...).or_else(|| len-1)`），
  `CM_USERREPAIRITEM` 改调它（`CM_MERCHANTQUERYREPAIRCOST` 保持「未命中 return」）。
- 用例：`tests::repair_target_index_replicates_oracle_last_item_bug`；改坏必红 **RC6**（删回退 ⇒ FAILED，`left: None right: Some(2)`）。

### 自报 Important（字节等价）shop 帧尾缺 `EDCode.EncodeString`
- 复核 F-1 时发现：⑤-4 第①片所有 shop 帧的**线上尾**都缺 `EDCode.EncodeString`。
  oracle：shop 帧经 `SendDefMessage`（`PlayObject.Socket.cs:26-43`，非空尾走 `SendSocket(ClientMsg, EDCode.EncodeString(sMsg))`）
  或 `SendSocket(ClientMsg, EDCode.EncodeString(processMsg.Msg))`（`PlayObject.Message.cs:1731`）下发；
  而 `emit_shop_frames` 原为 `gm_data_body(header, &f.tail)` **裸拼**（`f.tail` = `downlink::gbk` 原文）⇒ 与 oracle 逐字节不等。
- **改后**：`emit_shop_frames` 统一 `edcode::encode(&f.tail)`（空尾 `encode([]) == []` ⇒ header-only 帧逐字节不变）。
  `ShopFrame.tail` 语义明确为「未编码 GBK 原文」（文档已注）；与 ⑥-3 已合入的 `smframe::sm_merchantsay`（`edcode::encode`）同口径。
- 用例：`tests::emit_shop_frames_edcodes_tail_like_send_def_message`（含空尾不变）；改坏必红 **RC7**（退回裸拼 ⇒ FAILED，
  `left` = 裸 GBK 字节 / `right` = EDCode 字节）。

### M-1（Minor）`ClientUserBuyItem` 的 `Dealing` 前置门
- oracle `Operate.cs:196-199` 的 `if (Dealing) return;` 在 `FindMerchant` 之前。⑦ 未落地 ⇒ `Dealing` 恒 false ⇒ 行为暂时等价。
- 按 reviewer 建议在 whitelist **T-47** 增一行登记（接 ⑦ 时补门或继续登记），避免日后静默漂移。

### 返工读数
- 用例：`shop` 7 条重钉 1 条；`game-svr` 二元用例 +2（`repair_target_index...` / `emit_shop_frames...`）。
- 改坏必红：原 4 条 + 返工 3 条（RC5/RC6/RC7）= **7 条**。
- 四门禁与边界见 `04-gates.txt` / `04b-workspace-test.txt` / `05-boundary.txt`（base = master `15510a47`）。
