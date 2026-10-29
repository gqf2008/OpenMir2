# 窗口新场景第四批：`castle_merchant_tax`（blocked）+ `castle_merchant_tax_potion`（买腿可跑）

- 卡：`openmir2-window-scenes-castle-tax`（owner `openmir2-dev-1`，branch `card/window-scenes-castle-tax`，base `4ae90226b`）。
- 写集（fork-point 口径 4 文件）：`tools/probe/scenarios/castle_merchant_tax.json`、
  `tools/probe/scenarios/castle_merchant_tax_potion.json`、`tools/window/run3-scenarios.json`（只追加 #22/#23）、本报告。
- 来源：台账 `docs/window-closure-ledger-20261025.md` #1 行（R 片 `99156c2c6`）、`docs/window-kit-coverage-20261025.md` §1.1、
  `docs/castle-merchant-tax-wiring-20261025.md` §6。
- **不动**：`card_window.py` / `run_batch.sh` / `preflight_private_stack.py` / `mir2_probe.py`；不改生产代码；不起服务端；不改 `E:/MirServer`。
- 结论：**`castle_merchant_tax` = blocked（两缺口）**；**`castle_merchant_tax_potion` 可跑**——W-TAX-1 的**买腿**（Castle 分支）今天就能做。

## 1. 一句话

- `castle_merchant_tax`（0151 城堡武器铺，20 步）：卖/修两腿 + 买**武器**腿（需 652 解码）+ 升级腿说明；**blocked**，原因为
  ① 升级腿默认配置零税且与武器铺不同图；② `SaveTick` 10 分钟门不可观测（不落盘 + C# 无时钟注入）。
- `castle_merchant_tax_potion`（0153 城堡药店，7 步）：**买腿可跑**——`金创药(小量)` `StdMode=0` ⇒ `CM_USERBUYITEM` 免 `MakeIndex`
  （`Merchant.cs:1241` 的 `StdMode<=4` 支），买价 88（首次补货 `×1.1`，回落 80）⇒ 税 `Round(88*5/100)=Round(4.4)=4`；末步 `sleep 620000` 跨过 600s 存档门以读 `SabukW.txt`。

## 2. oracle 证据链（商税四调用点 + 城堡归属判定）

### 2.1 四调用点（`CastleMerchant || GetAllNpcTax`）

| # | 路径 | oracle | 谁付 | 税去向 / 税基 | 失败即无税 |
|---|---|---|---|---|---|
| 1 | 武器升级 | `Merchant.cs:546-554` | 玩家（`DecGold(UpgradeWeaponPrice)` 已成功） | `Castle != null` ⇒ `Castle.IncRateGold(UpgradeWeaponPrice)`；否则 `GetAllNpcTax` ⇒ `CastleMgr.IncRateGold(UpgradeWeaponPrice)` | `DecGold` 失败 / 两条件皆假 |
| 2 | 买入 | `Merchant.cs:1257-1266` | 玩家（`Gold -= nPrice` + `AddItemToBag` 成功） | Castle ⇒ `IncRateGold(nPrice)`；否则 Manager ⇒ `IncRateGold(UpgradeWeaponPrice)` | 钱不够 / 背包满 / 负重不足 ⇒ 无税 |
| 3 | 卖出 | `Merchant.cs:1392-1400` | 玩家收钱（`IncGold(nPrice)` 成功） | Castle ⇒ `IncRateGold(nPrice)`；否则 Manager ⇒ `IncRateGold(UpgradeWeaponPrice)` | `nPrice<=0` / StdMode 25/30 耐久不足 / IncGold 超上限 |
| 4 | 修理 | `Merchant.cs:1644-1652` | 玩家（`DecGold(nRepairPrice)` 成功） | Castle ⇒ `IncRateGold(nRepairPrice)`；否则 Manager ⇒ `IncRateGold(UpgradeWeaponPrice)` | 不支持修理 / 价格<=0 / 金币不足 |

- 入账 `UserCastle.IncRateGold`（`UserCastle.cs:768-812`）：`Round(nGold*CastleTaxRate/100)`（AwayFromZero；部署 `CastleTaxRate=5`，`Server.conf:344`）
  → `TodayIncome` 日上限（`CastleOneDayGold=2000000`，`Server.conf:337`）→ `TotalGold` 封顶（`CastleGoldMax=5000000`，`:310`）
  → `SaveTick` 10 分钟门（严格大于，`nGold=0` 也刷新）。
- `CastleMgr.IncRateGold`（`CastleManager.cs:209-215`）：**升序遍历 `CastleList` 全表**，每座城各入账一次。

### 2.2 「Castle 非空」的判定（**不是**城主/归属）

`NormNpc.Initialize`（`NormNpc.cs:1677-1680`）→ `CastleMgr.InCastleWarArea(this)`（`CastleManager.cs:38-48`）
→ `UserCastle.InCastleWarArea`（`UserCastle.cs:454-480`）三条任一命中即为该城：

1. `envir == CastleEnvir && |HomeX-x| < WarRangeX && |HomeY-y| < WarRangeY`（map 3 且距 `(644,290)` < 100）；
2. `envir == PalaceEnvir || envir == SecretEnvir`；
3. `envir.MapName ∈ EnvirList`（= `CastleMapList=0151,0152,0153,0154,0155,0156`，`CastleConf.cs:25-38`）。

⇒ **0151/0152/0153/0154/0155 上的城堡商人 `Castle` 恒非空**，与 `OwnGuild`（部署为空）无关；
**map 3 上的 `沙巴克/小贩`（`MerChant.txt:89`，`IsCastle=1`，(663,304)）** 也非空——它走第 1 条
（|663-644|=19、|304-290|=14 均 < 100）。所以「Castle 非空」**不必**在 0151–0155 内；但反过来说，
**没有任何 `IsCastle=1` 的 NPC 落在攻城区之外**（真机观察不到「CastleMerchant 但 Castle=null」的零税态，见 §5）。

### 2.3 证据面：`SabukW.txt` 只有三个字段，`SaveTick` 读不到

- `CastleConf` 落盘：`CastleName/ChangeDate/WarDate/IncomeToday/OwnGuild/NextWar/TotalGold/TodayIncome` + `[Defense]` 段；
  **`SaveTick` 不在 `CastleConf.cs`（grep 0 命中）**——只在内存（`UserCastle.cs:44` 置 0、`:798-800` 刷新）。
  ⇒ 台账「`SaveTick` 字段 diff」在任何时钟注入下都取不到；W-TAX-4 只能验行为级 10 分钟门。
- 落盘时机：`SystemProcessor.ShowOnline`（`SystemProcessor.cs:54-62`）→ `CastleMgr.Save()`，
  间隔 `ConsoleShowUserCountTime=600000`（`Server.conf:334`）⇒ **买完要等 ≥10 分钟**（`IncRateGold` 自身不写盘）。

## 3. 场景

### 3.1 `castle_merchant_tax`（0151 城堡武器铺，blocked；20 步）

| 项 | 值 / 依据 |
|---|---|
| 目标商人 | `MerChant.txt:83`（GBK）`沙巴克/武器铺 0151 7 14 武器店老板 0 0 1`；列序 `脚本 地图 X Y 名 flag appr IsCastle canMove moveTime`（`LocalDB.cs:430-466`）⇒ `IsCastle=1` ⇒ `CastleMerchant=true` |
| 脚本 | `Market_Def/沙巴克/武器铺-0151.txt` 第 1 行 `(@buy @sell @repair @s_repair)`；`[goods]`:30-37（木剑/匕首/青铜剑/…，全 StdMode>4） |
| placement | `0151 (8,14)` = NPC (7,14) 正东邻格；两侧 map 同源（21×25），(8,14) `bk=0 fr=49` 可走；**(7,15)/(6,14)/(7,13) `bk=32768` 不可走**；(7,14) 是 NPC 自身格 |
| 全图唯一 actor | `MonGen.txt`/`Npcs.txt` 无 0151 行，`MerChant.txt` 仅 :83 ⇒ discovery 的 `merchant_actor` 必为武器店老板 |
| 步序 | 点 NPC → 643 →`@repair`→ 668 → `1023`（MakeIndex 5001）→ [669/670] → `@main` → `@sell` → 646 → `1013` → [648/649] → `@main` → `@buy` → 645 → `1015`（详情）→ 652 → `1014`（待填 MakeIndex）→ [650/200] |
| 夹具 | `assets.Gold=10000`；`inventory 木剑 #207 make_index=5001 slot=40 dura=1000/4000`（修腿要 `Dura<DuraMax && StdMode!=43`，`Merchant.cs:1617-1618`）；先修后卖共用同一件 |

### 3.2 价格口径（纠正一个易错点）

脚本 `[goods]` 行是 **`名 / 数量 / 刷新分钟`**，**不是价格**（`ScriptParsers.cs:949-975` 的 `GetValidStrCap` 顺序：
`sItemName → sItemCount → sItemRefillTime`）。买价来自 `GetItemPrice(index)`：`ItemPriceList`（`LocalDB.LoadGoodPriceRecord`
在 C# 里整段被注释掉 ⇒ 部署恒空）→ 否则 `stdItem.Price`（要求 `StdMode ∈ ItemTypeList`，由脚本 `+N` 行填充，`ScriptParsers.cs:693-700`）。
所以 `castle_merchant_tax_potion` 的买价 = `GetItemPrice`：`ItemPriceList` 为空时回落 `stditems#3 Price=80`（`sql/mir2_data.sql:993`）；
但启动补货时 `CheckItemPrice` 会先把 `ItemPriceList[3]` 记为 `Round(80*1.1)=88`（`Merchant.cs:184-206/229-234`）⇒ **实际买价 88**。`[goods]` 里的 100 不是价格。

### 3.3 `castle_merchant_tax_potion`（0153 城堡药店，**可跑**；7 步）

| 项 | 值 / 依据 |
|---|---|
| 目标商人 | `MerChant.txt:90` `沙巴克/药店 0153 14 7 药店 0 1 1` ⇒ `IsCastle=1`；`0153 ∈ CastleMapList` ⇒ `Castle` 非空 |
| 脚本 | `Market_Def/沙巴克/药店-0153.txt`：`(@buy @sell)`、`%100`（PriceRate=100）、`+0`（ItemTypeList=[0]）；`[goods]` 首行 `金创药(小量) 100 3` = 库存 100 / 刷新 3 分钟 |
| placement | `0153 (15,7)` = 药店 (14,7) 正东邻格；两侧 map 同源（21×25），`bk=0 fr=500` 可走（两侧 `read_map_cell` 实跑一致）；`MapInfo.txt:840 [0153 药品店] FIGHT` ⇒ server_index=0；`MonGen/Npcs` 无 0153 行、`MerChant` 仅 :90 ⇒ 全图唯一 actor |
| 物品 | `金创药(小量)`：`StdMode=0`、`Weight=1`、`Price=80`（`sql/mir2_data.sql:993`） |
| 步序 | 点 NPC → 643 →`@buy`→ 645 → `CM_USERBUYITEM(1014, recog=merchant_actor, param=0, tag=0, body=金创药(小量))` → `wait_idents [650, 200]` → `sleep 620000` |
| 为什么免 MakeIndex | `ClientBuyItem` 取物门 `StdMode <= 4 \|\| StdMode == 42 \|\| StdMode == 31 \|\| userItem.MakeIndex == nInt`（`Merchant.cs:1241`）；药品 `StdMode=0` ⇒ 名字命中即可，`nInt` 不参与 |
| 期望买价 | **88**（不是 80、更不是 `[goods]` 的 100）：启动时 `LoadNpcData`→`RefillGoods` 首次补货先走 `CheckItemPrice`（`Merchant.cs:184-206/229-234`）
把 `ItemPriceList[3]` 记为 `Round(stdItem.Price 80 * 1.1) = 88`，`GetItemPrice` 先查该表 ⇒ `GetUserPrice(88)` 且 `PriceRate=100` ⇒ 88（若该表为空才回落到 80） |
| 期望税 | `Castle.IncRateGold(nPrice)` ⇒ `nInGold = Round(nPrice * 5 / 100)`：88 ⇒ `Round(4.4) = 4`；80 ⇒ `Round(4.0) = 4` ⇒ **`TotalGold`/`TodayIncome` 各 +4**（两种价都落 4，AwayFromZero） |
| 帧差即证据 | **`SM_BUYITEM_SUCCESS(650).recog`** = 购买后 `Gold`（10000−88 vs 10000−80）⇒ **买价口径写错会直接帧 diff**，不需要等 600s 存档。**钱在 `recog` 不在 `param`**：发送 `SendMsg(this, RM_BUYITEM_SUCCESS, 0, playObject.Gold, nInt, 0)`（`Merchant.cs:1303`；`SendMsg(baseObject,wIdent,wParam,nParam1,nParam2,nParam3)`）→ 泵 `SendDefMessage(SM_BUYITEM_SUCCESS, nParam1, LoWord(nParam2), HiWord(nParam2), 0)`（`PlayObject.Message.cs:1824-1826`；`SendDefMessage(wIdent,nRecog,nParam,nTag,nSeries)`，`PlayObject.Socket.cs:27`）⇒ `recog=Gold`、`param=LoWord(MakeIndex)=0`、`tag=HiWord(MakeIndex)=0`；**用 `param` 判价会恒读 0** |
| 期望帧 | `SM_BUYITEM_SUCCESS(650)` + `SM_ADDITEM(200)`；失败只回 651（钱/背包/负重门）⇒ 本步超时（fail-closed） |
| 文件证据 | 末步 `sleep 620000` 跨过 600s 存档门；之后手工比两侧 `Castle/0/SabukW.txt` 的 `TotalGold`/`TodayIncome`（**两侧 diff 必须 = 0**，且各自 +4）。`SaveTick` 不落盘（§2.3） |
| run3 | `#23 castle_merchant_tax_potion`，`extra=["--timeout-ms","700000"]`（默认 20000 会在 20s 杀掉带 620s sleep 的 probe） |

## 4. 为什么 `castle_merchant_tax` 仍 blocked（两缺口）

| # | 缺口 | 证据 | 不解封时的行为 |
|---|---|---|---|
| ① | **升级腿已拆成独立场景** | 唯一带 `@upgradenow` 的脚本 = `Market_Def/沙巴克铁匠-3.txt`；`MerChant.txt:82` IsCastle=**0**；map 3 (332,317) 距 `Home(644,290)` |Δx|=312 ≥ `WarRangeX=100` ⇒ `Castle=null` ⇒ 只有 `CastleGetAllNpcTax=1` 走 Manager 分支（税基恒 `UpgradeWeaponPrice=10000` ⇒ 每城 `Round(500)`）。本卡已拆出 **`castle_merchant_tax_upgrade.json`**（12 步，upgrade_retrieve 前 11 步 + 落盘观察 sleep），在 run3 暂保持 blocked：**run3 批次只认内置 profile 名**（手工窗口可用 `--profile <path>.json` 声明 config_overrides，见 §4.2） | 升级腿不进批次；用**自定义 profile 文件**（`fixtures.config_overrides`）手工可跑，实测记录见 §4.2 |
| ② | **W-TAX-4 `SaveTick` 不可观测** | `SaveTick` 不落盘（§2.3）；C# `MIR2_CLOCK_*` 零实现（`grep -rIl MIR2_CLOCK src/` = 0） | 只能真实时点复拍；「同 tick 逐位」需 ⑧-1 时钟注入片 |

**备注（工具缺口，不阻断买腿）**：买**武器**需要运行时 `MakeIndex`——货架全 StdMode>4，645 载荷只有 `名/submenu/价格/库存/`
（`Merchant.cs:983-986`），`MakeIndex` 只在 652 的 EDCode `ClientItem` buffer（`:1329-1341`），而 `mir2_probe.py` 没有该解码。
买**药品**腿不经过这里（`StdMode<=4`），已由 `castle_merchant_tax_potion` 覆盖。

**夹具不对称（已解，门固化）**：曾出现 `csharp-root` 1 城 / `rust-root` 2 城（`CastleMgr.IncRateGold`
升序遍历全表 ⇒ W-TAX-2 必假红）。现由 preflight `castle_list_symmetry` 门固化：两侧（含
`M2GameSvr-hook` 影子）必须同为 `0,1`，不一致即 FAIL；`SabukW` 两侧逐字节同源。

## 4.2 升级腿实测记录（2026-10-25，私有栈，5 次）

- **配置覆盖生效，且 profile JSON 路径是推荐用法（缺口①撤回）**：`card_window.load_profile`
  支持 **profile JSON 路径**（`--profile <path>.json`）。把 `BUILTIN_PROFILES["alt"]` dump 成 JSON、
  加 `fixtures.config_overrides.Server.conf = {CastleGetAllNpcTax:1, ConsoleShowUserCountTime:5000}`，
  传路径即可 ⇒ 本轮 `fixtures.json` 记录 `-CastleGetAllNpcTax=0 / +CastleGetAllNpcTax=1`，
  C# baseline+shadow 与 Rust 根均被写；跑完三方 `Server.conf` 均还原为 `600000/0`。
  **限制只在 run3 批次**：`run_batch.sh` 清单只认内置 profile 名，场景不能携带 profile
  ⇒ 该场景暂不进批（需 run_batch 支持 profile 路径，另卡）。标准做法示例：

  ```powershell
  python -c "import json,card_window as c; print(json.dumps(c.BUILTIN_PROFILES['alt'],ensure_ascii=False))" > myprofile.json
  # 编辑 myprofile.json 加 "fixtures": {"config_overrides": {"Server.conf": {"CastleGetAllNpcTax": 1}}}
  python -I -B tools/conformance/card_window.py --profile myprofile.json --scenario ... --out ...
  ```
- **升级提交链可达（C# 成功一次）**：`frames.ndjson` 含 `SM_DELITEMS(709)` 与 `SM_DELITEM(202)`，
  说明 `MOVR/升级` 提交支真实执行；对应场景 12 步。
- **但整体未绿，三个独立原因**（全部实测）：
  1. **baseline 背包 46/46 满**：`characters_bagitem` 对 winplay0 已有每个 Position 的行（`StdIndex=0`），
     而 `inventory` 夹具要求**空槽**（计数行）⇒ `inventory: 声明槽位里已有 N 行` fail-closed。
     `restore-baseline*.sh` 会把这些行恢复回来，手工删槽在 run_side 的 restore 之后又被还原。
     ⇒ 声明式 `inventory` 在当前基线上**写不进去**（另卡 `openmir2-window-fixture-inventory`
     解决 `inventory.replace/drop`；本轮用**一次性手工 DB 夹具**绕过后才得到上条 202/709 证据）。
  2. **discovery 选错 NPC**：`required merchant_actor` 走 `npc_click_auto` 的 `auto:last_turn`，
     在 (320,328)/(331,317) 都点到「装备回收」NPC（643 body_text 实测），`@upgrade` 无链接 ⇒
     `dlg_select @upgrade` 后 643 超时。**铁匠在 map 3 与其它可转身 NPC 同图**，`last_turn`
     不保证是铁匠；解封需要「按坐标/名字选 NPC」的 discovery 能力（工具卡）。
  3. **税入账未观测到（原因未定位；已排除 `SaveTick` 与 600s 门两项假设）**：一次成功提交
     （帧 `SM_DELITEM(202)`/`SM_DELITEMS(709)`，角色 Gold 10000→3）后，`Castle/0/SabukW.txt` 的
     `TotalGold` 仍为 0。**观测是写实的**：四份根（生产 `E:/MirServer/M2GameSvr`、私栈 baseline、
     hook 影子、Rust 根）的 `Castle/0/SabukW.txt`（1748B）与 `Castle/1/SabukW.txt`（0B）mtime
     全部仍是 2026-10-09 01:32 / 2026-10-10 22:21 ⇒ **文件零写入**；`Castle/List.txt` 的 mtime
     变化（06:32/07:41/07:52/07:58）来自夹具写入与事后手工还原，内容已回到 6 字节
     `0\r\n1\r\n`，**不是服务端写的**。
     **归因：原因未定位**。两条假设已排除：
     - `SaveTick` **不控制写盘**——`UserCastle.IncRateGold` 只更新内存 tick
       （`src/M2Server/Castle/UserCastle.cs:798-805`；旧稿误写 `src/GameSrv/CastleManager/`，reviewer-20 F1 订正），不是落盘门；
     - 600s 门不是拦路者——`ConsoleShowUserCountTime` 已覆盖为 5000ms，且
       `SystemProcessor.ExecuteInternal` **每 tick 都调 `ShowOnline()`**
       （`ShowOnlineTick` 在 Startup 置初值）⇒ 8s 窗口足够触发一次 `CastleMgr.Save()`。
     帧 `202/709` + `Gold 10000→3` 已证明升级成功支真实执行 ⇒ `IncRateGold(10000)` 被调用，
     两个上限（5e6/2e6）也不截断。⇒ 「文件零写入」是观测事实，但**为什么没落盘尚未定位**。
     **待补的两行诊断**（下次受控重跑从该轮 `GameSvr.out.log` 抓）：`已读取 [N] 个城堡信息`
     （CastleList 实际条数）与 `在线数: <N>`（`ShowOnline()` 是否按预期走到）——若 N 与夹具
     不符、或 `在线数` 行缺失，即可把问题钉到「加载/ShowOnline 未按预期执行」。
     **W-TAX 判据暂不改**：在诊断补齐并定位前，不得据此把判据缩成「只判帧」。
- **结论**：升级腿**保持 blocked**，缺口收窄为两项 + 一条批次限制：
  ① `inventory.replace/drop`（另卡，dev-1 已知）；
  ② 按名字/坐标 discovery 铁匠（`auto:last_turn` 在密集 NPC 区不可靠——以后同类场景应显式给坐标/名字，不要靠 discovery）；
  ③ **批次限制**：手工窗口可用 `--profile <path>.json` 声明 config_overrides（已实测），
  但 run3/`run_batch` 只认内置 profile 名，进批需工具卡。**不声称可跑**。
- 证据目录：`E:/tmp/scriptcov/taxleg/`（profile、临时场景、card 日志、frames、fixtures diff）。
- **重跑尝试与处置（traceability）**：用于抓上述两行诊断的受控重跑**未能启动**——15xxx 私栈端口
  被另一线（dev-1）占用，`retry1..5` 一律 `UNDECIDABLE`（观测记录见 `E:/tmp/scriptcov/taxleg/retry*.log`）；
  本线**未杀他人进程**，按栈互斥签 `release-without-start`。期间 hook 影子 `Castle/List.txt` 曾被
  运行期改写，由 Leader 手工还原为 6 字节，四份根现有 sha/内容一致（复核口径见 runbook §0.9/§0.10）。
  重跑本身仍待端口释放后进行；其证据将作为本卡后续 `comment` 挂上，不再开新版本。

---

## 5. W-TAX 项 × 状态

| W 项 | 现状 | 依据 |
|---|---|---|
| W-TAX-1 四路径 | **买腿（Castle 分支）可跑**（`castle_merchant_tax_potion`）；卖/修两腿场景已写但随本卡 blocked；买武器腿缺 652 解码；升级腿见缺口① | §3.1/§3.3 |
| W-TAX-2 `GetAllNpcTax=1` | **blocked**：需配置覆盖通道/两侧副本同值 + 城数对齐 | §4 备注、§6 |
| W-TAX-3 前置 | **前半只能构造级**：冻结内容里**没有** `IsCastle=1` 且 `Castle=null` 的 NPC（0151–0155 走 EnvirList；map 3 的小贩 (663,304) 在 WarRange 内）⇒ 真机只可观测后半「非城主仍收税」（测试账号无行会；potion 场景即证据的一例） | §2.2 |
| W-TAX-4 日上限/`SaveTick` | **blocked**：`SaveTick` 不落盘 + C# 无时钟；只能真实时点 + 600s 存档门 | §2.3、§4② |

## 6. 解封条件（写在场景 `blocked.missing_fixture`）

1. **W-TAX-2 / 升级腿**：配置覆盖**已有通道**（`fixtures.config_overrides`，`--profile <path>.json`
   可用；见 §4.2 第 1 条与本卡 F3 订正），未解的是**进批**（`run_batch` 只认内置 profile 名）与
   **map 3 铁匠**的独立场景（单场景一个 placement ⇒ 另开卡）；另有上条「税入账原因未定位」。
2. **买武器腿**：解析 `SM_SENDDETAILGOODSLIST(652)` 的 ClientItem buffer（EDCode）取 `MakeIndex`，喂给第 19 步
   （需改 `tools/probe/mir2_probe.py`，本卡写集外）。
3. **Castle 夹具对齐**：两侧**含影子** `Castle/List.txt` 同值 **`0,1`**（与 runbook §0.9/§0.10 一致；W-TAX-2 需 ≥2 城；reviewer-20 F2 订正，旧稿「建议都 0」作废），`Castle/0/SabukW.txt` 初值同源（现已是）。
4. **W-TAX-4**：真实时点（10 分钟门）+ 存档门（600000ms）；「同 tick 逐位」等 ⑧-1 的 C# 时钟注入。

## 7. run3 注册（只追加两行）

`tools/window/run3-scenarios.json`：`#22 castle_merchant_tax`（带 `blocked`）、`#23 castle_merchant_tax_potion`
（`extra=["--timeout-ms","700000"]`）；既有 21 条逐字节未动（`git diff` 只见 #21 行尾逗号 + 4 行新增）。实测：

```text
$ bash tools/window/run_batch.sh --list
21  butch_two_draws      alt           YES      | blocked: 无「单只固定动物尸体」夹具 + RNG 面未接线（…）
22  castle_merchant_tax  alt           YES      | blocked: 买武器腿缺 652 ClientItem 解析（买药品腿已由 #23 覆盖）+ 升级腿需 CastleGetAllNpcTax=1 副本（…）+ SaveTick 不落盘（…）
23  castle_merchant_tax_potion alt      -        --timeout-ms 700000
场景数 23（blocked 2 ⇒ 入批 21）
```

`blocked` 语义（`run_batch.sh:107-108/223-225`）：只在 `--list` 标注、不进批次；只有 `--only castle_merchant_tax` 点名时才 die（rc=2）——预期行为。

## 8. 自证与门禁

- `--check-scenario`：`castle_merchant_tax` rc=0（20 步）、`castle_merchant_tax_potion` rc=0（7 步）；全库 28/28 场景 rc=0。
- `validate_placement_content`（离线、两侧实跑）：`0151 (8,14)` OK、`0153 (15,7)` OK；把 0151 改 `(7,15)` ⇒ 两侧 `ConfigError`（bk=0x8000）。
- 卫生门禁（conflict_marker / stale_pointer `--ref master` / dynamic_count `--ref master` / board_coverage）+
  `git diff --check <base> <head>` + `merge_tree_helper`：见 patch body。
- 只读证据：`MerChant.txt` / `Market_Def` / `MapInfo.txt` / `0151.map` / `0153.map` / `Castle/*` / `Server.conf` 均只读复核；
  **未起栈、未占端口、未连库、未改 `E:/MirServer`**。
