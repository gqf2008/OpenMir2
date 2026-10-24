# 窗口新场景第四批：`castle_merchant_tax`（W-TAX-1..3，tier L）—— **blocked**

- 卡：`openmir2-window-scenes-castle-tax`（owner `openmir2-dev-1`，branch `card/window-scenes-castle-tax`，base `4ae90226b`）。
- 写集（fork-point 口径 3 文件）：`tools/probe/scenarios/castle_merchant_tax.json`、`tools/window/run3-scenarios.json`（只追加 #22）、本报告。
- 来源：台账 `docs/window-closure-ledger-20261025.md` #1 行（R 片 `99156c2c6`）、`docs/window-kit-coverage-20261025.md` §1.1、
  `docs/castle-merchant-tax-wiring-20261025.md` §6。
- **不动**：`card_window.py` / `run_batch.sh` / `preflight_private_stack.py` / `mir2_probe.py`；不改生产代码；不起服务端；不改 `E:/MirServer`。
- **结论**：场景形状/步序/placement 全部落盘且 `--check-scenario` rc=0，但**整卡按 `blocked` 处理、不进批次**（三个缺口见 §4）。

## 1. 一句话

`castle_merchant_tax` 把「城堡武器铺（0151）买/卖/修 + 沙巴克铁匠（map 3）升级」四腿写成可加载的场景，
并**如实标 blocked**：买腿缺运行时 `MakeIndex` 通道、升级腿缺 `CastleGetAllNpcTax=1` 窗口副本且与武器铺不同图、
`SaveTick` 门不落盘不可观测。**不声称可跑**。三条子结论同时写进场景 `blocked` 与本文，避免只留在人话里。

## 2. oracle 证据链（商税四调用点 + 城堡归属判定）

### 2.1 四调用点（`CastleMerchant || GetAllNpcTax`）

| # | 路径 | oracle | 谁付 | 税去向 / 税基 | 失败即无税 |
|---|---|---|---|---|---|
| 1 | 武器升级 | `Merchant.cs:546-554` | 玩家（`DecGold(UpgradeWeaponPrice)` 已成功） | `Castle != null` ⇒ `Castle.IncRateGold(UpgradeWeaponPrice)`；否则 `GetAllNpcTax` ⇒ `CastleMgr.IncRateGold(UpgradeWeaponPrice)` | `DecGold` 失败 / 两条件皆假 |
| 2 | 买入 | `Merchant.cs:1257-1266` | 玩家（`Gold -= nPrice` + `AddItemToBag` 成功） | Castle ⇒ `IncRateGold(nPrice)`；否则 Manager ⇒ `IncRateGold(UpgradeWeaponPrice)` | 钱不够 / 背包满 / 负重不足 ⇒ 无税 |
| 3 | 卖出 | `Merchant.cs:1392-1400` | 玩家收钱（`IncGold(nPrice)` 成功） | Castle ⇒ `IncRateGold(nPrice)`；否则 Manager ⇒ `IncRateGold(UpgradeWeaponPrice)` | `nPrice<=0` / StdMode 25/30 耐久不足 / IncGold 超上限 |
| 4 | 修理 | `Merchant.cs:1644-1652` | 玩家（`DecGold(nRepairPrice)` 成功） | Castle ⇒ `IncRateGold(nRepairPrice)`；否则 Manager ⇒ `IncRateGold(UpgradeWeaponPrice)` | 不支持修理 / 价格<=0 / 金币不足 |

- 入账本体 `UserCastle.IncRateGold`（`UserCastle.cs:768-812`）：`Round(nGold*CastleTaxRate/100)`（AwayFromZero，部署 `CastleTaxRate=5`，`Server.conf:344`）
  → `TodayIncome` 日上限裁剪 → `TotalGold` 封顶（严格小于）→ `SaveTick` 10 分钟门（严格大于，`nGold=0` 也刷新）。
- `CastleMgr.IncRateGold`（`CastleManager.cs:209-215`）：**升序遍历 `CastleList` 全表**，每座城各入账一次。

### 2.2 城堡商人「Castle 非空」怎么来的（**不是**城主/归属）

```
NormNpc.Initialize (NormNpc.cs:1677-1680)
  Castle = CastleMgr.InCastleWarArea(this)          // CastleManager.cs:38-48
    → UserCastle.InCastleWarArea(envir, x, y)       // UserCastle.cs:454-480
        envir==CastleEnvir && |HomeX-x|<WarRangeX && |HomeY-y|<WarRangeY   // map 3 且距 (644,290) <100
      || envir==PalaceEnvir || envir==SecretEnvir
      || envir.MapName ∈ EnvirList                  // ← 关键：0151..0156
```

`EnvirList` 来自 `CastleConf` 解析 `[Defense] CastleMapList=0151,0152,0153,0154,0155,0156,`（`CastleConf.cs:25-38`），
部署 `Castle/0/SabukW.txt` 里正是这一行。⇒ **城堡武器铺/药店等 0151–0155 上的商人 `Castle` 恒非空，
与 `OwnGuild`（部署为空）无关**。因此「城主注入 / 行会归属」**不是**本卡的 missing_fixture——不要把它写成缺口。

### 2.3 证据面：`SabukW.txt` 只有三个字段可读，`SaveTick` 读不到

- `CastleConf` 落盘字段：`CastleName/ChangeDate/WarDate/IncomeToday/OwnGuild/NextWar/TotalGold/TodayIncome` + `[Defense]` 全段；
  **`SaveTick` 不在 `CastleConf.cs`（grep 0 命中）**——它只在内存（`UserCastle.cs:44` ctor 置 0、`:798-800` 门内刷新）。
  ⇒ 台账「`SaveTick` 字段 diff」在**任何**时钟注入下都不可从文件取得；W-TAX-4 只能验「行为级 10 分钟门」。
- 落盘时机：`SystemProcessor.ShowOnline`（`SystemProcessor.cs:54-62`）→ `CastleMgr.Save()`，
  间隔 `ConsoleShowUserCountTime=600000`（`Server.conf:334`）⇒ **窗口要等 ≥10 分钟**才能从 `SabukW.txt` 读到 `TotalGold/TodayIncome`；
  提前停栈不会 flush（`IncRateGold` 自身不写盘）。

## 3. 场景内容（已落盘，形状合法）

### 3.1 placement：`0151 (8,14)`

| 项 | 值 / 依据 |
|---|---|
| 目标商人 | `MerChant.txt:83`（GBK）`沙巴克/武器铺 0151 7 14 武器店老板 0 0 1`；列序 `脚本 地图 X Y 名 flag appr IsCastle canMove moveTime`（`LocalDB.cs:430-466`）⇒ `IsCastle=1` ⇒ `CastleMerchant=true` |
| 脚本 | `Market_Def/沙巴克/武器铺-0151.txt`，第 1 行 `(@buy @sell @repair @s_repair)` ⇒ `IsBuy/IsSell/IsRepair/IsSupRepair` 全 true；`[goods]`:30-37（木剑/匕首/青铜剑/鹤嘴锄/短剑/铁剑/青铜斧） |
| 站位 | (8,14) = NPC (7,14) 正东邻格，Chebyshev 1（`CM_CLICKNPC` 门 15，`PlayObject.Base.cs:674-698`） |
| 可走性 | 两侧 `0151.map` 同源（21×25）；(8,14) `bk=0 fr=49` 可走；**(7,15)/(6,14)/(7,13) `bk=32768` 不可走**；(7,14) 是 NPC 自身格（可走但不用） |
| 全图唯一 actor | `MonGen.txt` 无 0151 行、`Npcs.txt` 无 0151 行、`MerChant.txt` 仅 :83 一行 ⇒ discovery 得到的 `merchant_actor` 必然是武器店老板 |
| 服务器号 | `MapInfo.txt:838` `[0151 武器铺] FIGHT` ⇒ `server_index=0`，两侧 `Server.conf [Server] ServerIndex=0` ⇒ placement 校验可过 |

### 3.2 步序（20 步，先修→卖→买，同一把木剑）

| 步 | 动作 | 期望帧 | 关键 oracle |
|---:|---|---|---|
| 1 | `click_npc merchant_actor` | — | `1010` → `ClientClickNpc`（同图 + ≤15 格）→ `NormNpc.Click` |
| 2 | `wait_for 643` | `SM_MERCHANTSAY` | `武器铺-0151.txt:6-15` @main（`@buy/@sell/@repair/@s_repair`） |
| 3 | `dlg_select @repair` | — | `Merchant.UserSelect` sREPAIR → `UserSelectRepairItem` |
| 4 | `wait_for 668` | `SM_SENDUSERREPAIR` | `Messages.cs:319` |
| 5 | `send 1023`（`param/tag=MakeLong(5001)`，body=木剑） | — | `ClientRepairItem`（`Merchant.cs:1606-1665`）；税在 `DecGold` 成功后 |
| 6 | `wait_for [669, 670]` | OK / FAIL | `Messages.cs:328-329`；FAIL/零帧 ⇒ 无税 |
| 7–8 | `@main` → `643` | 同步 | `@main` 恒可跳（`PlayObject.Base.cs:2816-2819`） |
| 9–10 | `@sell` → `wait 646` | `SM_SENDUSERSELL` | sSELL → `UserSelectSellItem` |
| 11 | `send 1013`（MakeIndex + 名） | — | `ClientUserSellItem`（`Operate.cs:162-189`）→ `ClientSellItem`；税在 `IncGold` 成功后 |
| 12 | `wait_for [648, 649]` | OK / FAIL | `Messages.cs:310-311` |
| 13–14 | `@main` → `643` | 同步 | — |
| 15–16 | `@buy` → `wait 645` | `SM_SENDGOODSLIST` | body=`名/submenu/价格/库存/`（`Merchant.cs:983-986`，**无 MakeIndex**） |
| 17–18 | `send 1015`（body=木剑）→ `wait 652` | `SM_SENDDETAILGOODSLIST` | body=EDCode `ClientItem` 串，**含 MakeIndex**（`Merchant.cs:1329-1341`） |
| 19 | `send 1014`（`param/tag` = 652 解析出的 MakeIndex；**现为占位 0**） | — | `ClientBuyItem`（`Merchant.cs:1225-1305`）；StdMode>4 要求 `MakeIndex` 精确匹配（`:1241`） |
| 20 | `wait_for [650, 200]` | `SM_BUYITEM_SUCCESS` + `SM_ADDITEM` | `Merchant.cs:1299-1305` + `SendAddItem`；失败只回 651 |

- 夹具：`assets.Gold=10000`（修/买前置）、`inventory 木剑 #207 make_index=5001 slot=40 dura=1000/4000`（修腿要 `Dura<DuraMax && StdMode!=43`，`Merchant.cs:1617-1618`）。
- 先修后卖：修完 `Dura=DuraMax`，卖价 = `Round(GetUserItemPrice/2)`（`GetSellItemPrice`）仍 >0，两腿共用同一件物品。

## 4. 为什么 blocked（三缺口，逐条可复核）

| # | 缺口 | 证据 | 不解封时的行为 |
|---|---|---|---|
| ① | **买腿拿不到 `MakeIndex`** | 0151 货架全 StdMode>4（武器）⇒ 买要 `MakeIndex` 精确匹配（`Merchant.cs:1241`）；645 载荷只有 `名/submenu/价格/库存/`（`:983-986`），`MakeIndex` 只在 652 的 EDCode `ClientItem` buffer（`:1329-1341`）；`mir2_probe.py` 无 ClientItem 解码 | 第 19 步 `param/tag=0` ⇒ 只回 651 / 无帧，第 20 步超时（fail-closed，不假绿） |
| ② | **升级腿默认配置零税** | 唯一带 `@upgradenow` 的脚本 = `Market_Def/沙巴克铁匠-3.txt`（`grep -rl @upgradenow Market_Def` 仅 1 个）；`MerChant.txt:82` 该行 IsCastle=**0**；map 3 (332,317) 距 `Home(644,290)` |Δx|=312 ≥ `WarRangeX=100` ⇒ `Castle=null` ⇒ 只有 `CastleGetAllNpcTax=1` 走 Manager 分支（税基恒 `UpgradeWeaponPrice=10000`，`Server.conf:333` ⇒ 每城 500）。两侧 conformance `Server.conf:345` 现均 `=0`；且铁匠在 map 3、武器铺在 0151 ⇒ **单 placement 场景不能同场覆盖** | 该腿无税入账；写进场景只会得到「无税=0」的假绿 |
| ③ | **W-TAX-4 的 `SaveTick` 不可观测** | `SaveTick` 不落盘（§2.3）；C# `MIR2_CLOCK_*` 零实现（`grep -rIl MIR2_CLOCK src/` = 0） | 只能真实时点复拍；「同 tick 逐位」需 ⑧-1 时钟注入片 |

**additional（夹具不对称，跑 W-TAX-2 前必须修）**：两侧 Castle 夹具现在**城数不同**——
`csharp-root/M2GameSvr/Castle/List.txt` = `0`（1 城），`rust-root/.../List.txt` = `0,1`（2 城）；
`Castle/0|1/SabukW.txt` 两侧逐字节同源（`diff` = SAME0/SAME1，`TotalGold=0`/`TodayIncome=0`/`OwnGuild=` 空）。
`CastleMgr.IncRateGold` 升序遍历全表 ⇒ 两侧城数不同会让 W-TAX-2 必然假红（Rust 两城各 +500、C# 一城 +500）。

## 5. 解封条件（写进场景 `blocked.missing_fixture`，逐条）

1. **probe 侧**：解析 `SM_SENDDETAILGOODSLIST(652)` 的 EDCode `ClientItem` buffer 取 `MakeIndex`，并把值作为第 19 步 `param/tag`
   （需改 `tools/probe/mir2_probe.py` —— 本卡写集外，另开工具卡）。备选（不可行）：把货架 `MakeIndex` 钉成已知值——`CopyToUserItemFromName`
   用 `M2Share.GetItemNumber()` 全局计数（`GameItemSystem.cs:113-140`），冻结内容下不可注入。
2. **W-TAX-2 / 升级腿**：两侧 conformance 副本（`E:/tmp/conformance/{csharp-root,rust-root}/M2GameSvr/Server.conf`）
   **同值**置 `CastleGetAllNpcTax=1` 并重启；`restore-baseline *.sh` 只回滚 DB 不覆盖配置 ⇒ 需登记并在跑完还原；
   外加一条 **map 3 铁匠**的独立场景（本卡写集只允许一个场景文件 ⇒ 解封时另开卡）。
3. **Castle 夹具对齐**：两侧 `Castle/List.txt` 同值（建议都 `0`）、`Castle/0/SabukW.txt` 初值同源（现已是）；否则 W-TAX-2 必假红。
4. **W-TAX-4**：真实时点（10 分钟门）+ 存档门（`ConsoleShowUserCountTime=600000`）；「同 tick 逐位」等 ⑧-1 的 C# 时钟注入。
5. **W-TAX-3 的前半**（`CastleMerchant=1` 但不在攻城区 ⇒ 零税）：**冻结内容里不存在这样的 NPC**——所有 IsCastle=1 行都在
   0151/0152/0153/0154/0155，全部 ∈ `CastleMapList` ⇒ 只能构造级（合同测试已覆盖，`castle_tax.rs` 的
   `tax_precondition_without_castle_and_without_global_tax_is_none`）；真机只可观测后半「非城主仍收税」（测试账号无行会）。

## 6. run3 注册（只追加一行）

`tools/window/run3-scenarios.json` 追加 `#22 castle_merchant_tax`（`profile=alt`、`extra=[]`、带 `blocked` 字符串）；
既有 21 条逐字节未动（`git diff` 只有 #21 行尾逗号 + 2 行新增）。实测：

```text
$ bash tools/window/run_batch.sh --list
21  butch_two_draws      alt           YES      | blocked: 无「单只固定动物尸体」夹具 + RNG 面未接线（…）
22  castle_merchant_tax  alt           YES      | blocked: 买腿缺 652 ClientItem 解析（MakeIndex 不可得）+ 升级腿需 CastleGetAllNpcTax=1 副本（…）+ SaveTick 不落盘（…）
场景数 22（blocked 2 ⇒ 入批 20）
```

`blocked` 语义按 `run_batch.sh:107-108/223-225`：**只在 `--list` 标注、不进批次**；只有 `--only castle_merchant_tax` 点名时才 die（rc=2）——预期行为。

## 7. 自证与门禁

- `python tools/probe/mir2_probe.py --scenario tools/probe/scenarios/castle_merchant_tax.json --check-scenario --var merchant_actor=1`
  ⇒ `scenario OK: castle_merchant_tax (20 steps, …)`，**rc=0**；既有 26 条场景 check 仍全 rc=0。
- 卫生门禁（conflict_marker / stale_pointer `--ref master` / dynamic_count `--ref master` / board_coverage）+
  `git diff --check <base> <head>` + `merge_tree_helper`：见 patch body。
- 只读证据：本文件引用的部署件（`MerChant.txt` / `Market_Def` / `MapInfo.txt` / `0151.map` / `Castle/*` / `Server.conf`）均只读复核；
  **未起栈、未占端口、未连库、未改 `E:/MirServer`**。
