# Lane P 派工简报：城堡税/成员价 + 交易栏物品名差量（audit #8/#15）

- 基线：`c83a9ae24`（master）。本车道**纯文档**：只读 oracle / Rust 源码与部署件（`E:/MirServer` 只读），不建服务端、不占端口、不写库、不改主检出。
- 对应审计：`mir2-rs/tests/parity/m3/audit-uncovered-surfaces-20261019.md` 第 **8** 行（城堡税/成员价）与第 **15** 行（交易栏 `GetUpgradeStdItem` desc 差量 + 自定义名，T-24）。本文只细化这两条既有登记，不把“已登记缺口”当新发现。
- **tier = L**（纯算术 + 帧编码，零 RNG / 零时序 / 零落库）。若实现把 `CastleMgr.Save()` 周期落盘或 `SaveTick` 文件行为一起接线，则该片**升 R**（见 §1.11 / §2.10 的升级条件）。
- 一句话：**税是“成交价 × 税率”在四个成交点各自收一次（成员价已经先折过）；物品名是“模板名或自定义名 + 14 字节 GBK 定长截断”，而 `ClientItem.Desc` 在线上恒为 14 个 0。**

---

## 0. 锚点与基线哈希（`sha256sum`，master `c83a9ae24`）

实现卡的 reviewer 应先用下列哈希确认自己的检出与本文一致；任何一项不同，行号引用即失效。

**归一化口径（2026-10-20 订正）**：表内 sha256 一律是**工作树原始字节**的 `sha256sum`，不做事先 EOL 转换。本仓 `core.autocrlf=false`；`git ls-files --eol` 显示 15 个文件 `w/lf`、`src/GameSrv/Npc/Merchant.cs` 因 blob 本身 CRLF 显示 `w/crlf`。等价的与检出无关的可复现写法是对 blob 内容做哈希（与工作树逐字节一致）：

```sh
git config core.autocrlf                             # false
git ls-files --eol <上述 16 个文件>                  # 15×w/lf；Merchant.cs w/crlf
sha256sum <上述 16 个文件>                           # 或：git show c83a9ae24:<path> | sha256sum
```

> 旧表有 8 个 C# 值是把 LF blob 逐行转 CRLF 后算的（`Merchant.cs` 本身 CRLF 入库，故 raw 字节偶然命中），在本仓 LF 检出上不可复现；下列值已全部按上述口径重算。

| 文件 | sha256 |
|---|---|
| `src/GameSrv/Npc/Merchant.cs` | `ccefc0ecf719baf8bec59077b198a146c4e2ecf2491973e603845ffd8b291bfc` |
| `src/M2Server/Castle/UserCastle.cs` | `b6660b12434529209de9b19b7dc9eefaf350830b2e2045ddcd54447c6c05ab52` |
| `src/M2Server/Castle/CastleManager.cs` | `913f0cbb18cb744912a3c95e7d846bfec4b8b53d0837fbaabdfb835f6e50798a` |
| `src/M2Server/Items/GameItemSystem.cs` | `e5acbfcf0ad282500b5c5d95175bdd2151a3fd5090aed371416731ac1c3149df` |
| `src/M2Server/Items/CustomItemSystem.cs` | `31f46666e4c0e1b2a234dc9bd94717673afd315f182b132e81c4c6693baf21aa` |
| `src/M2Server/Player/PlayObject.Base.cs` | `230eec7683e640ba42d690e0c20de365d5b688eb92b5fa2dcce3d22e94cbea6b` |
| `src/M2Server/Player/PlayObject.Operate.cs` | `42d9aa16ffd5568971eec4daad283c1c0f7ecaed16634615fecb3bf37ab052db` |
| `src/OpenMir2/Packets/ClientPackets/ClientItem.cs` | `7dbdd7219e8fa61448ab1b13417418b2a307f387132429d9174a6f1bcfe1e650` |
| `src/OpenMir2/Extensions/BinaryWriterExtension.cs` | `8c950be79e8d97537e49e3dd847621d5988f3d2d52eee1054aed2d4ffa835428` |
| `mir2-rs/crates/world/src/castle_tax.rs` | `3f8280027f3bf8abcf3155b7b0caf9be82819c1ca873b773d1460796199744e6` |
| `mir2-rs/crates/formula/src/shop.rs` | `7e9e594826fc43e17ce4ceb14338aeb4c0aa7ed874958847ae99f9816fceb09b` |
| `mir2-rs/bins/game-svr/src/shop.rs` | `58d1c92edf41e45d2228d57cf09dcee35fb4ac152bb9d7f3ddfbef12c4779de4` |
| `mir2-rs/bins/game-svr/src/trade.rs` | `1363df5513cd0799fa68b2cf6b49bf88d55872f204d4c3e37a2628ac7ac3534b` |
| `mir2-rs/bins/game-svr/src/itemwire.rs` | `c99df5c51cf07c83201cf8e55e66cbd5ea4a0cca5ebf72907d0cb25c184ffd78` |
| `mir2-rs/bins/game-svr/src/bag.rs` | `2418a5527c691df4e8cbc21f1a3f1fc5aa95154087a453911803dc10e65b4e97` |
| `mir2-rs/crates/data/src/lists.rs` | `fe7b3510b30242271c237b288d6201aa576a980a1213753629e6967d12161c1b` |

部署件读数（只读，仅作可达性证据）：

| 键/文件 | 部署值 | 出处 |
|---|---|---|
| `CastleDir` | `Castle` | `E:/MirServer/M2GameSvr/Server.conf:65` |
| `CastleFile` | `Castle/List.txt` | 同上 `:71` |
| `CastleTaxRate` | `5` | 同上 `:344` |
| `CastleGetAllNpcTax` | `0` | 同上 `:345` |
| `CastleGoldMax` | `5000000`（oracle 缺省 10,000,000） | 同上 `:310`；`GameSvrConf.cs:1792` |
| `CastleOneDayGold` | `2000000` | 同上 `:337`；`GameSvrConf.cs:1793` |
| `CastleMemberPriceRate` | `80` | 同上 `:574`；`GameSvrConf.cs:1796` |
| `UpgradeWeaponPrice` | `10000` | 同上 `:333`；`GameSvrConf.cs:1420` |
| `SuperRepairPriceRate` | `3` | 同上 `:725`；`GameSvrConf.cs:1709` |
| `Castle/List.txt` | `0`、`1` 两行 | `E:/MirServer/M2GameSvr/Castle/List.txt` |
| `Castle/Castle/SabukW.txt` | `[Defense] CastleMap=3`、`CastleHomeX/Y=(644,290)`、`CastleWarRangeX/Y=(100,100)`、`[Defense] CastleMapList=0151..0156`（逗号分隔、重复多轮；`CastleConf.LoadConfig` 解析进 `EnvirList`） | 实读文件（见 §1.2） |
| `Envir/ItemNameList.txt` | 5 行（GB2312） | 见 §2.4.4 |

---

## 1. 城堡税与成员价

### 1.1 取值链总图（谁写、谁读、在哪收）

```
Server.conf [Setup]
  CastleTaxRate ───────────────┐
  CastleOneDayGold ────────────┤
  CastleGoldMax ───────────────┤
  CastleGetAllNpcTax ──────────┤
  UpgradeWeaponPrice ──────────┤
  CastleMemberPriceRate ───────┤
                                ▼
MerChant.txt 第 8 列 ──> Merchant.CastleMerchant   (LOAD-TIME, LocalDB.cs:461-464)
                               │
                               ├─ Merchant.Initialize() → Castle = CastleMgr.InCastleWarArea(this)
                               │        (NormNpc.cs:1676-1679；行会/位置决定 Castle 是否为 null)
                               ▼
成交点（buy / sell / repair / upgrade）
  price = GetUserPrice(t)   ← CastleMerchant && Castle.IsMasterGuild(MyGuild) 时打 80% 折
  gold  = 扣/加 price
  if (CastleMerchant || GetAllNpcTax)
      Castle != null          → Castle.IncRateGold(price 或 nRepairPrice)
      else if (GetAllNpcTax)  → CastleMgr.IncRateGold(UpgradeWeaponPrice)   ← 税基常量怪行为 B-16
                               ▼
UserCastle.IncRateGold: Round(nGold * CastleTaxRate/100) → TodayIncome 日上限 → TotalGold 总上限 → SaveTick 10min 门
```

### 1.2 `CastleMerchant` 与 `Castle` 的解析链（**先于任何定价**）

1. **`CastleMerchant` 来源**：`MerChant.txt` 第 8 列（0-based 第 7 个字段）`StrToInt(x,0) != 0`。
   - 生产加载器 `LocalDB.LoadMerchant()`（`LocalDB.cs:412-476`）用 `!= 0`。
   - `LocalDB.ReLoadMerchants()`（`:989-1059`）用 `!= 1` **反向**判，但唯一调用点 `ReloadNpcCommand.cs:24` 被注释 ⇒ **死代码，不要照它移植**（既有登记：`design-upgrade-npc-20261020.md` §0）。
   - 部署 `MerChant.txt`（`E:/MirServer/M2GameSvr/Envir/MerChant.txt`，空白分隔）有 8 行第 8 列为 `1`：`0151` 武器铺、`0152` 仓库、`0153` 药店、`0154` 三家（手镯/项链/戒指）、`0155` 布衣店、地图 `3` 的小贩 `(663,304)`。⇒ `CastleMerchant=true`。
2. **`Castle` 来源**：`NormNpc.Initialize()`（`NormNpc.cs:1676-1679`）在商人挂图时执行
   `Castle = SystemShare.CastleMgr.InCastleWarArea(this)`。`UserCastle.InCastleWarArea`（`UserCastle.cs:454-478`）三支：
   - `envir == CastleEnvir && |HomeX-x| < WarRangeX && |HomeY-y| < WarRangeY`（严格 `<`）；
   - `envir == PalaceEnvir || envir == SecretEnvir`；
   - `EnvirList` 中任一名与地图名 `OrdinalIgnoreCase` 相等。
   - **与 `CastleMerchant` 无关**：任何商人只要站在攻城区/皇宫/密道/`EnvirList` 图内，`Castle` 都会被赋值。
3. **部署解析结果**：实读 `Castle/Castle/SabukW.txt` 得 `[Defense] CastleMap=3`、`CastleHomeX/Y=(644,290)`、`CastleWarRangeX/Y=(100,100)`，以及 `[Defense] CastleMapList=0151,0152,0153,0154,0155,0156,...`（逗号分隔、重复多轮）。该键由 `CastleConf.LoadConfig`（`CastleConf.cs:24-37`）解析进 `userCastle.EnvirList`；`InCastleWarArea` 第三支比对的是 `EnvirList`，不是原始键名。
   ⇒ 上述 8 个城堡商人的 `Castle` **都非 null**；`3` 图 `(663,304)` 距 `(644,290)` 为 `(19,14)`，严格落在 100 内。
4. **时序**：`GameApp.cs:213` `CastleMgr.Initialize()` → `GameApp.cs:227` `LoadMerchant()` → `WorldEngine.Initialize()`（内部 `MerchantInitialize()`，`WorldServer.Merchant.cs:8-35`，这里跑 `merchant.Initialize()`）→ `GameApp.cs:246` `LoadCastleList()`。
   - 商人初始化时城堡表里只有 **1 座**已初始化城堡；`LoadCastleList()` 之后才追加 `Castle/0`、`Castle/1` 两座**从未 `Initialize()`** 的城堡。
   - 所以 `merchant.Castle` 永远指向那座“已初始化城堡”；追加的两座只参与 `CastleMgr.IncRateGold` 的全表遍历（见 §1.6.4）。
5. **移动不刷新**：`Merchant.Run` 的 `MapRandomMove` 之后不会重算 `Castle` ⇒ 以 `Initialize` 时刻为准（Rust 侧应在 spawn 时固定 `castle_index`，不要每帧按坐标重算）。

### 1.3 `GetUserPrice` 逐行（`Merchant.cs:765-783`）

```csharp
if (CastleMerchant) {
    if (Castle != null && Castle.IsMasterGuild(playObject.MyGuild)) {
        int n14 = HUtil32._MAX(60, HUtil32.Round(PriceRate * (CastleMemberPriceRate / 100.0)));
        result = HUtil32.Round(nPrice / 100.0 * n14);
    } else {
        result = HUtil32.Round(nPrice / 100.0 * PriceRate);
    }
} else {
    result = HUtil32.Round(nPrice / 100.0 * PriceRate);
}
```

- **成员判定**：`Castle.IsMasterGuild(MyGuild)` = `MasterGuild != null && MasterGuild == guild`（`UserCastle.cs:639-642`，**对象引用相等**）。即：只有**城主行会**成员；同城其它行会、攻方、盟友、无行会都**不**打折。Rust 的 `is_master_guild_name`（`castle.rs:1528-1538`）在 `master_guild` 已挂时按 `OwnGuild` 与会名逐字比较，语义等价。
- **下限 60**：`n14 = max(60, Round(PriceRate × CastleMemberPriceRate / 100))`。默认 `100×80% = 80`。
  - 当 `PriceRate` 落在 `55..59`（`%PRICERATE` 最低生效值 55）时，`n14` 被抬到 60 ⇒ **成员价反而高于非成员价**（60% vs 55%）。这是照搬的怪行为，不要“修正”。
- **取整**：`HUtil32.Round` = `Math.Round(x, MidpointRounding.AwayFromZero)`（`HUtil32.cs:131-134`），**不是银行家舍入**。中间量是 `double`（`PriceRate * (rate/100.0)`、`nPrice/100.0*n14`）。
- `nPrice` 来自 `GetUserItemPrice`（`Merchant.cs:1130-1210`，double）；其内部还有耐久/属性折算与 `HUtil32.Round`（Rust 已在 `crates/formula/src/shop.rs:127-207` 逐行落地）。
- **只有 `GetUserPrice` 打折**；`GetSellItemPrice(nPrice) = Round(nPrice / 2.0)`（`:1358-1361`）**不经过** `GetUserPrice`。

### 1.4 谁会 / 不会被打折（逐场景）

| 场景 | 打折？ | 原因 |
|---|---|---|
| 商人 `CastleMerchant=false` | **否** | 外层 `if (CastleMerchant)` 不成立 |
| `CastleMerchant=true`、`Castle=null` | **否** | `Castle != null` 不成立 |
| `CastleMerchant=true`、`Castle!=null`、玩家属城主行会 | **是** | 走 `n14` 分支 |
| 同上但玩家属其它行会/无行会/盟友/攻方 | **否** | `IsMasterGuild` 引用不等 |
| 买入成交 | **是** | `:1244 GetUserPrice(GetUserItemPrice)` |
| 买入面板报价（`@buy` 列表） | **是** | `:974 GetUserPrice(GetItemPrice)`（注意：**不**做耐久折算，与实买价可能不同） |
| 详情列表（`CM_USERGETDETAILITEM`） | **是** | `:1330 GetUserPrice(GetUserItemPrice)`，结果写进 `clientItem.DuraMax`（展示位） |
| 查询卖出价（`CM_MERCHANTQUERYSELLPRICE`） | **否** | `:1347 GetSellItemPrice(...)` 绕过 `GetUserPrice` |
| 卖出成交 | **否** | `:1380 GetSellItemPrice(...)` |
| 查询修理价 / 实际修理 | **是** | `:1557` / `:1616 GetUserPrice(...)`，之后才乘 `SuperRepairPriceRate` 与耐久比例 |
| 武器升级提交 | **否** | 固定 `UpgradeWeaponPrice`（`:540`），不取 `GetUserPrice` |
| 仓库/取回 | **无价格** | `PlayObject.Operate.cs:1607-1713` 无定价调用 |
| 元宝寄售 | **否** | `PlayObject.Sale.cs` 不走 `Merchant.GetUserPrice` |

> 结论：**“城堡成员价”= 城主行会成员在城堡商人处的买入/修理/展示价；卖出永不享受。**

### 1.5 四个收税调用点与税基

分支形状（四处完全相同，`Merchant.cs:544-553 / 1250-1259 / 1385-1393 / 1637-1646`）：

```csharp
if (CastleMerchant || SystemShare.Config.GetAllNpcTax) {
    if (Castle != null)            Castle.IncRateGold(<price>);
    else if (GetAllNpcTax)         SystemShare.CastleMgr.IncRateGold(Config.UpgradeWeaponPrice);
    // else: 什么也不做
}
```

| 调用点 | oracle 行 | 传入 `Castle.IncRateGold` 的值 | 备注 |
|---|---|---|---|
| 武器升级提交 | `:544-553` | `Config.UpgradeWeaponPrice` | 与本次交易价无关；固定 10000 |
| 买入 | `:1250-1259` | `nPrice` = **已打折**的买入价 | 成员买得便宜 ⇒ 城税收得少 |
| 卖出 | `:1385-1393` | `nPrice` = `Round(GetUserItemPrice/2)` | 无成员折扣 |
| 修理 | `:1637-1646` | `nRepairPrice` = `Round(修价/3/DuraMax×(DuraMax-Dura))`；特修先 `×SuperRepairPriceRate` | 先 `GetUserPrice`，再特修倍率，再耐久比例 |

**必须照搬的三条怪行为**：

1. **`CastleMgr` 分支税基恒为 `UpgradeWeaponPrice`**（B-16，已有白名单）。买入 777 金币也按 10000 计税；卖出/修理同样。
2. **`Castle != null` 优先于 `CastleMerchant`**：`CastleMerchant=false` 但 `GetAllNpcTax=true`、且商人恰在攻城区/皇宫/密道/`EnvirList` 图内 ⇒ 税进**那座城**，不走 `CastleMgr`。
3. **`CastleMerchant=true`、`Castle=null`、`GetAllNpcTax=false` ⇒ 完全不收税**（虽然商人被标成城堡商人）。部署 `GetAllNpcTax=0`，这是可能的真实分支。

### 1.6 `UserCastle.IncRateGold` 逐行（`UserCastle.cs:768-812`）

1. `nInGold = HUtil32.Round(nGold * (CastleTaxRate / 100.0))`（AwayFromZero）。
2. 日上限裁剪（`:771-788`）：
   - `TodayIncome + nInGold <= CastleOneDayGold` ⇒ `TodayIncome += nInGold`；
   - 否则若 `TodayIncome >= CastleOneDayGold` ⇒ `nInGold = 0`；
   - 否则 `nInGold = CastleOneDayGold - TodayIncome; TodayIncome = CastleOneDayGold;`（只记余量）。
3. 金库上限（`:789-796`）：`nInGold > 0` 时，若 `TotalGold + nInGold < CastleGoldMax`（**严格小于**）⇒ 累加；否则 `TotalGold = CastleGoldMax`（相等/超限都落到“设为上限”，同值）。
4. `SaveTick` 门（`:798-806`）：`(GetTickCount() - SaveTick) > 10*60*1000`（**严格大于**）⇒ 刷新 `SaveTick`。**函数内没有任何文件写入**（`M2Share.GameLogGold` 的日志调用被注释）；`nGold=0` 也照样评估/刷新。Rust 侧用 `world.clock_ms`（sim 时钟）即可——该门当前只改内存值、无任何可观测副作用（oracle 的墙钟初值通常 >10min，首次调用会刷新；差异不可见）。
5. `CastleManager.IncRateGold`（`CastleManager.cs:211-217`）：**升序**遍历 `CastleList`，对每座城各调一次；每座城有各自的 `TodayIncome`/`TotalGold`。
6. **持久化不在本函数**：`CastleMgr.Save()`（`CastleManager.cs:144-152`）由 `SystemProcessor.ShowOnline()` 按 `ConsoleShowUserCountTime` 周期调用（`SystemProcessor.cs:54-63`），`SaveConfigFile → CastleConf.SaveConfig` 才写 `TotalGold/TodayIncome`（`CastleConf.cs:96-128`；值为 0 时不写键）。⇒ 本片若只做内存计数/帧面，**不碰持久化**，保持 L。
7. **生产 `CastleList` 有 3 座**：`Initialize()` 建 1 座（读 `Castle/Castle/SabukW.txt`，随后 `ConfigDir="0"`），`LoadCastleList()` 再追加 `Castle/0`、`Castle/1` 两座**未 Initialize** 的城。追加的两座 `MapName` 为空、`EnvirList` 为空 ⇒ 不参与 `InCastleWarArea`，但**会参与 `CastleMgr.IncRateGold`**（B-16 分支）。`Castle` 分支只指向第一座。

### 1.7 查询/展示/仓库的边界（不产生税）

- `CM_MERCHANTQUERYSELLPRICE`（`:1345-1355`）：只回 `RM_SENDBUYPRICE(nC)`，价格 `GetSellItemPrice(GetUserItemPrice)`；`nC < 0` 回 0。
- `CM_MERCHANTQUERYREPAIRCOST`（`:1555-1592`）：`GetUserPrice` → 耐久比例 → 特修倍率或 `-1`（`IsSupRepair/IsRepair` 门）；只回 `RM_SENDREPAIRCOST`。
- `CM_USERGETDETAILITEM`（`:1304-1341`）：`GetUpgradeStdItem` + `GetUserPrice(GetUserItemPrice)` 写 `clientItem.DuraMax`；只回帧。
- `@buy` 面板（`:962-980`）：`GetUserPrice(GetItemPrice(Index))`；只回字符串。
- 仓库/取回（`PlayObject.Operate.cs:1607-1713`）：无 `GetUserPrice/GetSellItemPrice/IncRateGold`。

### 1.8 Rust 现状与缺口

**已实现（不要重写）**：

| 能力 | 位置 | 证据 |
|---|---|---|
| 税公式（round/日上限/总上限/SaveTick/多城升序） | `crates/world/src/castle_tax.rs:130-220` | 单测 `inc_rate_gold_rounds_away_from_zero_like_hutil` 等 6 条 |
| 商人税目的地/税基判定 | `castle_tax.rs:229-270`（`merchant_tax_target` / `merchant_tax_amount`） | 单测 `merchant_tax_target_matches_oracle_branches`、`merchant_tax_amount_uses_upgrade_weapon_price_for_manager_branch` |
| 成员价公式 | `crates/formula/src/shop.rs:211-229` | `get_user_price` 已含 `max(60, ...)` 与 AwayFromZero |
| 定价/耐久折算 | `crates/formula/src/shop.rs:127-207` | `get_user_item_price` 逐行 |
| `Castle.IsMasterGuild` 名字等价 | `crates/world/src/castle.rs:1528-1538` | — |
| `InCastleWarArea` | `castle.rs:1557-1571`；`World::castle_war_area_at` `world.rs:3482-3490` | — |
| 配置读取 | `crates/data/src/guild_castle.rs:264-269`；`bins/game-svr/src/config.rs:996/1131` | 两处读同一键，值一致 |

**缺口（按“实现可独立合入”排序）**：

1. **税从未落到状态**：`bins/game-svr/src/shop.rs:937`（卖）、`:1042`（买）、`:1217`（修）只有 `let _tax = ...`，条件算完即丢；升级武器（`crates/world/src/merchant.rs:964-1005`）也**没有**税分支。⇒ 沙巴克商人交易不增加 `TotalGold`。
2. **成员价恒死（两层原因，缺一不可）**：
   - **表层：配置开关写死**。`bins/game-svr/src/main.rs:4015` 的 `ShopContext.cfg.castle_member: false` 写死；`ShopContext` 没有玩家行会名，`MerchantState` 没有 `castle_index`（`crates/world/src/merchant.rs:130-235`）。⇒ 80% 永不生效。
   - **底层：城主身份恒空**。即使把 `castle_member` 置 `true`，`UserCastle::is_master_guild_name`（`crates/world/src/castle.rs:1528-1538`）在 `master_guild == None` 时**恒 false**；`master_guild` 只由 `LoadConfig`（`UserCastle.cs:231`）/`GetCastle`（`UserCastle.cs:543`）赋值，本仓生产不赋值、`GetCastle` 未接线（白名单 **T-131**：「生产恒 false」）。⇒ 成员价仍恒死，必须**先接 `GetCastle`/`MasterGuild`**；只把 `castle_member` 改成 `is_master_guild_name(...)` 不足以让 80% 生效。
3. **商人 `Castle` 未建模**：`MerchantState` 只有 `castle_merchant`/`castle_under_war`（后者恒 `false`，见 `merchant.rs:181`），没有“Initialize 时刻命中的城堡下标”。`castle_under_war` 是 `Merchant.Run` 显隐用的另一个 ⑧ 面，本片不要动。
4. **`CastleMgr` 分支无目标**：`bins/game-svr/src/main.rs` 无 `CastleManager`（`World.castles: Vec<UserCastle>` 即全表，`world.rs:906`）。B-16 的“税基常量”实现好但**没有调用点**。
5. **城堡表组成**：Rust `main.rs:15403-15410` 只 `load_sabuk_specs(Castle/Castle/SabukW.txt)` 建 1 座；oracle 另有 2 座未初始化城参与 `CastleMgr` 分支。部署 `GetAllNpcTax=0` ⇒ 当前无观测差异；若接 B-16 分支必须补齐（或显式登记“部署不可达”）。
6. **落盘**：game-svr 无 `CastleMgr.Save()` 调用点（grep 仅注释）。⇒ `TotalGold/TodayIncome` 不写文件（oracle 也是周期写，见 §1.6.6）。

### 1.9 写集切分（与 merchant/shop 现有代码的边界）

> 并发约束：`bins/game-svr/**` 与 `crates/world/src/guild.rs` 当前被其它车道占用。**T1a 只动 world，可独立先行；T1b/T2/T3 都在 bins，必须等车道释放后串行。**

| 片 | 写集 | 内容 | 可独立验证 |
|---|---|---|---|
| **T1a（world）** | `crates/world/src/merchant.rs`（+可能 `world.rs` 一个 helper） | `MerchantState` 增 `castle: Option<usize>`（名字建议 `castle_index`），`MerchantState::new` 增参数或设 `None` + setter；World 侧给一个 `set_merchant_castle_index(actor, idx)` / `merchant_castle_index(actor)`。**不改** `castle_tax.rs`/`formula`。 | 单测：构造两座城（一座含该 map/坐标、一座不含），设置后断言下标；再 `MapRandomMove` 后断言**不变**（照搬 §1.2.5）。 |
| **T1b（bins spawn，冲突域）** | `bins/game-svr/src/merchant.rs` | spawn 时（城堡初始化之后）写 `world.castle_war_area_at(Some(map_id), x, y)`。 | 同 T1a 的集成断言。 |
| **T2（bins shop，冲突域）** | `bins/game-svr/src/shop.rs` | `SellResult` / `BuyResult` / `RepairResult` 增 `tax: Option<(MerchantTaxTarget, i32)>`；四个成功分支用 `merchant_tax_target(m.castle_merchant, ctx.get_all_npc_tax, m.castle.is_some())` + `merchant_tax_amount(...)` 生成意图。**不在此片改 `main.rs`**。 | 单测：买/卖/修各一例，断言 `tax` 的 target 与 amount（含 B-16）。 |
| **T3（bins main，冲突域）** | `bins/game-svr/src/main.rs` + `shop.rs` 的 `ShopContext` | `handle_shop_cmd`（`:4305`）拿到意图后：`Castle` ⇒ `world.castles[i].inc_rate_gold(&tax_cfg, amount, world.clock_ms)`；`CastleManager` ⇒ 遍历 `world.castles` 逐个 `inc_rate_gold`（**不要**新建 `CastleManager`，否则与 `world.castles` 双份状态）。`tax_cfg` 由 `guild_castle_cfg` + `ShopCfg.upgrade.price` 组装（`CastleTaxConfig::from_guild_castle_config`）。同时把 `shop_context` 的 `castle_member` 改为 `merchant.castle.is_some_and(|i| world.castles[i].is_master_guild_name(player_guild))`——**这只是表层接线**：`master_guild == None` 时 `is_master_guild_name` 仍恒 false（§1.8 底层原因 / T-131），生产成员价要等 `GetCastle`/`MasterGuild` 接线；T3 的成员价集成向量必须先在夹具里挂 `master_guild`。 | 集成向量（见 §1.10）。 |
| **T4（升级税，冲突域）** | `bins/game-svr/src/main.rs` 升级分支（`:4580-4600` 附近） | `UpgradeWaponOutcome::Ok` 后按 `cfg.shop.upgrade.price` 生成同一意图并应用。 | 单测断言扣 10000 后 `TotalGold +500`。 |

**不要动的边界**：

- `crates/world/src/castle_tax.rs` 已完整，除非发现 B-16 之外的新差异；本片不需要改。
- `crates/formula/src/shop.rs::get_user_price` 已完整；不要为了“接成员价”重写公式。
- `MerchantState.castle_under_war` 属 ⑧ 攻城显隐面，**不要**在本片顺手接线（会牵动 `Merchant.Run` 的 RNG 站点）。
- 持久化（`SaveConfigFile`）单开卡；若并进来，tier 升 R。

### 1.10 判据草案（带牙）

所有断言都给出**精确期望值**（HUtil32.Round = AwayFromZero）；括号内是“改坏生产路径后哪条必红”。

#### G-T1 `merchant_castle_index_is_fixed_at_spawn`（T1a/T1b）

- 构造：两座城，城 A `map_id=3, home=(644,290), range=(100,100)`；城 B `map_id=5`。商人 `(663,304,map 3)`。
- 期望：`castle_index == Some(A)`；把商人挪到 `(800,304)`（仍在 map 3）后仍 `Some(A)`（oracle 不重算）。
- **牙**：把 T1b 的“spawn 时固定”改成“每次按当前坐标算” ⇒ 第二条断言红。
- 变体：`castle_merchant=false` 但位置在攻城区 ⇒ 仍 `Some(A)`（§1.2.2）。

#### G-T2 `castle_member_price_vectors`（T3，纯函数部分可先测 `formula`）

| 输入 | 期望 |
|---|---|
| `PriceRate=100, memberRate=80, nPrice=100` | 80 |
| `PriceRate=100, memberRate=80, nPrice=101` | 81（80.8 AwayFromZero） |
| `PriceRate=100, memberRate=80, nPrice=1` | 1（0.8） |
| `PriceRate=55, memberRate=80, nPrice=100` | 60（`max(60,44)`） |
| `PriceRate=80, memberRate=80, nPrice=100` | 64 |
| `PriceRate=100, memberRate=80, nPrice=100, castle_merchant=false` | 100（打折门不成立） |
| 同上但 `castle_member=false` | 100 |
| 集成：`castle_merchant=true`、玩家属**非城主**行会（`is_master_guild_name=false`） | 100 |
| 卖出：`nPrice=100`（成员态） | 50（`GetSellItemPrice` 不打折） |

- **牙**：删 `max(60, …)` ⇒ 第 4 行得 44，红；把 `castle_member` 的来源改成“有行会”而不比对城主 ⇒ 集成行得 80，红；把卖出改成先 `get_user_price` ⇒ 成员态卖出向量从 50 变 40，红。

#### G-T3 `castle_tax_shop_vectors`（T2/T3）

前置：`CastleTaxRate=5, OneDay=2_000_000, GoldMax=5_000_000, UpgradeWeaponPrice=10_000, GetAllNpcTax=0`，商人有 `Castle`。

| 场景 | 成交价 | 城堡 `TotalGold` 增量 |
|---|---|---|
| 买入（非成员，物品模板价 100，StdMode 0） | 100 | +5 |
| 买入（成员，同上） | 80 | +4（4.0） |
| 卖出（模板价 100 → 卖价 50） | 50 | +3（2.5 AwayFromZero） |
| 普通修理（`Price=100,DuraMax=10,Dura=4` → 20） | 20 | +1 |
| 特修（同上，`SuperRepairPriceRate=3` → 60） | 60 | +3 |
| 武器升级 | 10_000 | +500 |
| 买入 777，但 `GetAllNpcTax=1` 且 `Castle=null` | 777 | `CastleMgr` 各城 +500（B-16，不是 +39） |
| 买入 777，`Castle!=null`，`GetAllNpcTax=1` | 777 | 该城 +39（`Castle != null` 优先） |
| `CastleMerchant=true, Castle=null, GetAllNpcTax=0` | 777 | +0 |

- **牙**：把 `_tax` 改回丢弃 ⇒ 全部增量 0，红；把 B-16 的 `upgrade_weapon_price` 改成 `price` ⇒ 777 行得 39，红；把 `Castle != null` 判断挪到 `GetAllNpcTax` 之后 ⇒ 第三行“该城 +39”变成各城 +500，红。

#### G-T4 `castle_tax_limits_and_save_tick`（复用 `castle_tax.rs` 现有单测）

- `nGold=10 ⇒ +1`；`nGold=110 ⇒ +6`；`nGold=90 ⇒ +5`（AwayFromZero）。
- `TodayIncome=1_999_995, nGold=110` ⇒ 只加 5，`TodayIncome=2_000_000`；再加 ⇒ +0。
- `TotalGold=4_999_998, nInGold=6` ⇒ `TotalGold=5_000_000`（**超限截断**：直接累加会得 `5_000_004`；这是可观测的封顶向量）。恰好加满与已超限都回落到上限。
- `tick=600_000`（10min 整）⇒ `save_tick_fired=false`；`600_001` ⇒ true；`nGold=0, tick=600_001` ⇒ true。
- **牙**：`hutil::round` 换 `as i32` 截断 ⇒ 10→0、110→5，红；`>` 改 `>=` ⇒ 600000 行红；把封顶 `else` 支改成直接累加（`self.total_gold += n_in_gold`）⇒ 超限向量 `4_999_998+6` 得 `5_000_004` ≠ `5_000_000`，红（实证：`cargo test -p mir2-world --lib castle_tax` ⇒ `inc_rate_gold_caps_total_gold_at_castle_gold_max` 失败，`left: 5000004, right: 5000000`）。
- **不咬的牙（订正 2026-10-20）**：`castle_tax.rs:147` 的严格 `<` 改 `<=` **没有可观测输出**——`TotalGold + nInGold == CastleGoldMax` 时两支都把 `TotalGold` 置为 `CastleGoldMax`，`>` 时两支都走 `else`；实证改后 11 项单测全绿。故该算符只能按**行级等价**验收（照抄 `UserCastle.cs:789` 的 `<`），不能写成可红的判据。

#### G-T5 `castle_tax_target_branches`（复用/扩展 `castle_tax.rs` 单测）

- 八个组合穷举 `(castle_merchant, get_all_npc_tax, castle_present)`，逐项断言 `None/Castle/CastleManager`。
- **牙**：删 `castle_present` 优先支 ⇒ `(false,true,true)` 回 `CastleManager`，红；把 `!(castle_merchant || get_all_npc_tax)` 改成 `!(castle_merchant && get_all_npc_tax)` ⇒ **`(false,true,false)`** 由 `CastleManager` 变 `None`，红。**`(true,false,false)` 不是判别向量**：正确实现走到底为 `None`，变异式 `!(true&&false)=!(false)` 早退也 `None`，两支同值；可区分的还有 `(true,false,true)` 与 `(false,true,true)`（现有测试在 `(true,false,true)` 处首先失败：`left: None, right: Some(Castle)`）。

#### G-T6 `repair_cost_frames_use_member_price`（T3 帧面）

- `CM_MERCHANTQUERYREPAIRCOST` 的 `RM_SENDREPAIRCOST` 值必须用成员折后价再算耐久比例；非成员同参数对照。
- **牙**：查询路径漏传 `castle_member` ⇒ 成员帧值变大，红。

### 1.11 tier 与窗口项

- **tier = L**。依据：纯整数/浮点算术 + 帧字段；零 RNG、零时序门（`SaveTick` 是内存布尔，不触发 I/O）、零落库。
- **升级为 R 的触发条件**（实现 review 必须显式判级）：
  1. 接线 `CastleMgr.Save()`/`CastleConf.SaveConfig`（文件字段级 diff）；
  2. 把税计算放进 tick/队列（时序）；
  3. 把 `castle_under_war` 一并接线（会改 `Merchant.Run` 的 RNG/时序）。
- **窗口项（L 级只做抽样冒烟，不占独占窗口）**：
  - **W1**：真机抽 1 个沙巴克商人（建议 `0151 武器铺`，有买/卖/修）跑“买入→卖出→修理→升级”各一次；用 C# 侧探针读 `CastleMgr.CastleList[0].TotalGold/TodayIncome`（注意 `@ShowSbkGold` 的 `GetCastleGoldInfo` 函数体整段被注释，**不可用**），或等 `ConsoleShowUserCountTime` 后的 `Castle/0/SabukW.txt`。Rust 同序列对照增量。
  - **W2**：把一个角色设为城主行会成员（部署 `OwnGuild` 为空 ⇒ 需攻城或临时改配置），对比同一商品的面板价与实扣金币；再换非成员对照。
  - **W3**：`GetAllNpcTax=1` 的构造级/真机抽样（部署为 0，正常不可达）。

---

## 2. 交易栏物品名：`GetUpgradeStdItem` desc 差量 + 自定义名（T-24）

### 2.1 oracle 调用链（帧序 + 行号）

```
CM_ADDDEALITEM(操作) → PlayObject.Operate.cs:964 ClientAddDealItem
  1 DealCreat==null || !Dealing → return
  2 sItemName 含空格 → GetValidStr3(sItemName, ' ') 取首段（信件物品名带使用次数）
  3 !DealCreat.DealSuccess 才继续
  4 for ItemList: MakeIndex==nItemIdx
        sUserItemName = CustomItemSystem.GetItemName(userItem)
        if string.Compare(sUserItemName, sItemName, OrdinalIgnoreCase)==0 && DealItemList.Count<12
            DealItemList.Add; this.SendAddDealItem(userItem); ItemList.RemoveAt(i); break
  5 失败 → SendDefMessage(SM_DEALADDITEM_FAIL=676)

SendAddDealItem (PlayObject.Base.cs:2383-2403):
  SendDefMessage(SM_DEALADDITEM_OK=675)                  // 无条件、先发（立即帧）
  if DealCreat != null:
      stdItem = GetStdItem(userItem.Index); if null → 结束
      clientItem = new ClientItem()
      GetUpgradeStdItem(stdItem, userItem, ref clientItem)   // ← desc 差量在这里
      clientItem.Item.Name = CustomItemSystem.GetItemName(userItem)
      clientItem.MakeIndex/Dura/DuraMax = userItem.*
      SendSocket(MakeMessage(SM_DEALREMOTEADDITEM=682, ActorId, 0,0,1), EncodeBuffer(clientItem))
      DealCreat.DealLastTick = DealLastTick = GetTickCount()
```

删除侧对称（`Operate.cs:1001-1042` → `Base.cs:2357-2381`）：先 `SM_DEALREMOTEDELITEM=683`（模板非空时）再 `SM_DEALDELITEM_OK=677`；`DealCreat==null` 回 `SM_DEALDELITEM_FAIL=678`。`CanNotGetBackDeal` 在**最前**（先 `RM_MENU_OK` + `SM_DEALDELITEM_FAIL`）。

Rust 现状已对齐帧序/守卫：`bins/game-svr/src/trade.rs:529-548`（OK 先、remote 后）、`:551-580`（add）、`:583-606`（del）、`:612-...`（CanNotGetBackDeal 最前）。**本片不改状态机，只改 `ClientItem` 的内容口径。**

### 2.2 `ClientItem` 字段来源表（deal 路径）

`ClientItem.WritePacket`（`ClientItem.cs:25-84`）固定 124 字节：

| 偏移 | 字段 | deal 路径来源 | 备注 |
|---|---|---|---|
| 0..15 | `Name`（1B 长度 + 14B GBK） | `CustomItemSystem.GetItemName(userItem)` | 见 §2.4 |
| 15 | `StdMode` | 模板 | |
| 16 | `Shape` | 模板原值 | **交易路径不改** `Desc[8]⇒0/130`（与 `SendAddItem` 不同） |
| 17.. | Weight/AniCount/SpecialPwr/ItemDesc/... | 模板，**经 `GetUpgradeStdItem` 差量** | 见 §2.3 |
| 84..99 | `Reference`（1B+14B） | 模板 | |
| 99 | 0 | 常量 | |
| 100..104 | `MakeIndex` | 实例 | |
| 104..106 | `Dura` | 实例 | |
| 106..108 | `DuraMax` | 实例 | 交易路径**不**用 `GetUserPrice` 覆盖（详情列表才覆盖） |
| 108..122 | `Desc[14]` | **恒 0** | ← **关键：oracle 从不给 `ClientItem.Desc` 赋值**（`ClientItem.cs:14-18` 构造 `new byte[14]`；全仓 `grep clientItem.Desc` 0 命中） |
| 122..124 | 0,0 | 常量 | |

> 区间一律左闭右开：`Reference` = [84,99)、常量 0 = [99,100)、`MakeIndex` = [100,104)、`Dura` = [104,106)、`DuraMax` = [106,108)、`Desc` = [108,122)、尾部两个 0 = [122,124)。

> **现有 Rust 偏差（未登记）**：`itemwire::encode_client_item` 把 `ci.desc`（来自 `item.desc`）原样写入（`itemwire.rs:138`），而 oracle 该 14 字节恒 0。所有调用点都用 `desc: item.desc`（`trade.rs:957/980/1002`、`shop.rs` 详情/仓库、`itemwire` bag 列表、`ground.rs`、`enter.rs`、`slave.rs`）。⇒ **带升级属性的物品在 `SM_ADDITEM`/`SM_DEALADDITEM` 等所有 `ClientItem` 尾上，Rust 的 Desc 14 字节是 `item.desc` 的非零值，而 oracle 是 14 个 0（帧长不变）。** 这是本片要修的“隐藏第二差量”，必须在判据里钉死（G-N1）。

### 2.3 `GetUpgradeStdItem` 分支表（`GameItemSystem.cs:731-1140`）

记 `dN = userItem.Desc[N]`；`W(a,b) = MakeWord = (ushort)(a | b<<8)`；`byte(x) = unchecked (byte)x`；`min255(x)=_MIN(255,x)`。返回的 `count` 被**所有**调用点忽略（grep 验证），只需保证字段。

#### StdMode 5 / 6（武器）

| 字段 | 变换 |
|---|---|
| `DC` | `W(lo(DC), min255(hi(DC)+d0))` |
| `MC` | `W(lo(MC), min255(hi(MC)+d1))` |
| `SC` | `W(lo(SC), min255(hi(SC)+d2))` |
| `AC` | `W((ushort)(lo(AC)+d3), (ushort)(hi(AC)+d5))` —— **无 255 夹紧**；低字节 >255 的进位会 OR 进高字节 |
| `MAC` | 先 `W((ushort)(lo(MAC)+d4), (ushort)hi(MAC))`，**立即被下一行覆盖** ⇒ **d4 丢弃** |
| `MAC`（最终） | `W(lo(MAC), GetAttackSpeed(hi(MAC), d6))` |
| `SpecialPwr` | `1 <= d7 <= 10 && SpecialPwr >= 0` ⇒ `SpecialPwr = d7`（`sbyte`） |
| `ItemDesc` | `d10 != 0` ⇒ `std.ItemDesc \| 0x01` |
| `Slowdown` | `byte(std.Slowdown + d12)`（回绕） |
| `Tox` | `byte(std.Tox + d13)`（回绕） |
| `count` | `d0..d7>0`、`d12>0`、`d13>0`（**不含 d10**） |

`GetAttackSpeed(bStd,bUser)`（`:1168-1172`）：

```
RealAttackSpeed(x)   = x <= 10 ? -x : x-10
NaturalAttackSpeed(i)= i <= 0 ? (short)-i : (short)(i+10)
GetAttackSpeed       = (byte)NaturalAttackSpeed(RealAttackSpeed(bStd)+RealAttackSpeed(bUser))
```

#### StdMode 10 / 11

| 字段 | 变换 |
|---|---|
| `AC` | `W(lo, min255(hi+d0))` |
| `MAC` | `W(lo, min255(hi+d1))` |
| `DC` | `W(lo, min255(hi+d2))` |
| `MC` | `W(lo, min255(hi+d3))` |
| `SC` | `W(lo, min255(hi+d4))` |
| `Agility` | `byte(+d11)` |
| `MgAvoid` | `byte(+d12)` |
| `ToxAvoid` | `byte(+d13)` |
| `count` | `d0..d4, d11..d13` |

#### StdMode 15

| 字段 | 变换 |
|---|---|
| `AC/MAC/DC/MC/SC` | 各自 `W(lo, min255(hi+d0..d4))` |
| `Accurate` | `byte(+d11)` |
| `MgAvoid` | `byte(+d12)` |
| `ToxAvoid` | `byte(+d13)` |
| `Need` | `d5 > 0` ⇒ `= d5` |
| `NeedLevel` | `d6 > 0` ⇒ `= d6` |
| `count` | `d0..d4, d11..d13` |

#### StdMode 19 / 20 / 21

| 字段 | 变换 |
|---|---|
| `AC/MAC/DC/MC/SC` | 同 15 的 d0..d4 |
| `AtkSpd` | `byte(+d9)` |
| `Slowdown` | `byte(+d12)` |
| `Tox` | `byte(+d13)` |
| 19 | `Accurate = byte(+d11)` |
| 20 | `MgAvoid = byte(+d11)` |
| 21 | `Accurate = byte(+d11)`；`MgAvoid = byte(+d7)` |
| `Need/NeedLevel` | `d5>0 / d6>0` |
| `count` | `d0..d4, d9, d11..d13` |

#### StdMode 22 / 23

| 字段 | 变换 |
|---|---|
| `AC/MAC/DC/MC/SC` | 同 15 的 d0..d4 |
| `AtkSpd` | `byte(+d9)` |
| `Slowdown` | `byte(+d12)` |
| `Tox` | `byte(+d13)` |
| `Need/NeedLevel` | `d5>0 / d6>0` |
| `count` | `d0..d4, d9, d12, d13` |

#### StdMode 24

`AC/MAC/DC/MC/SC` 同 15 的 d0..d4；`Need/NeedLevel` 同；`count` = d0..d4。

#### StdMode 26

`AC/MAC/DC/MC/SC` 同 15 的 d0..d4；`Accurate = byte(+d11)`；`Agility = byte(+d12)`；`Need/NeedLevel` 同；`count` = d0..d4, d11, d12。

#### StdMode 52

`AC = W(lo, min255(hi+d0))`；`MAC = W(lo, min255(hi+d1))`；`Agility = byte(+d3)`；`count` = d0,d1,d3。

#### StdMode 54

`AC = W(lo, min255(hi+d0))`；`MAC = W(lo, min255(hi+d1))`；`Accurate = byte(+d2)`；`Agility = byte(+d3)`；`ToxAvoid = byte(+d13)`；`count` = d0,d1,d2,d3,d13。

#### 其它 StdMode

`CopyItemToClientItem` 的恒等拷贝，无差量。

#### 四个可执行样例（判据基准）

**V1（StdMode 5，无自定义名）**：模板 `AC=W(1,2)`, `MAC=W(3,4)`, `DC=W(5,6)`, `MC=W(7,8)`, `SC=W(9,10)`, `Slowdown=1`, `Tox=0`, `SpecialPwr=1`, `ItemDesc=0`；`Desc=[1,2,3,4,5,6,7,8,0,0,0,1,1,0]`；`d13=0` ⇒ 名字=模板名。

期望：`DC=W(5,7)`, `MC=W(7,10)`, `SC=W(9,13)`, `AC=W(5,8)`, `MAC=W(3,11)`, `SpecialPwr=8`, `Slowdown=2`, `Tox=0`, `ItemDesc=0`；线上 `Desc[108..122]` 全 0。

**V2（同上但 `d13=1`）**：模板名被 `GetCustomItemName(MakeIndex,Index)` 覆盖（命中则自定义名，未命中则**空串**）；`Tox=byte(0+1)=1`。⇒ “自定义名”和 “Tox+1” 由**同一个** `Desc[13]` 驱动，不要拆开。

**V3（AC 低字节溢出）**：`StdMode=5`, `AC=W(253,2)`, `d3=4`, `d5=0` ⇒ 低字节 `253+4=257=0x0101`；`hi=2 ⇒ 2<<8=0x0200`；`OR = 0x0301`（低字节 1，高位 2 被进位 OR 成 3）。**牙**：若实现用 `min255(lo+d3)` ⇒ `0x02FF`；若用 `byte` 截断 ⇒ `0x0201`；两者都与 `0x0301` 不同 ⇒ 红。

**V4（回绕）**：`Slowdown=250, d12=10` ⇒ `4`；`Tox=200, d13=100` ⇒ `44`。

### 2.4 名字链

#### 2.4.1 `CustomItemSystem.GetItemName`（`CustomItemSystem.cs:122-130`）

```csharp
result = SystemShare.ItemSystem.GetStdItemName(userItem.Index);   // Index-1 查表；越界 → ""
if (userItem.Desc[13] == 1)                                        // 精确等于 1
    result = M2Share.CustomItemMgr.GetCustomItemName(userItem.MakeIndex, userItem.Index);
return result;
```

- **无回退**：`Desc[13]==1` 但自定义表无条目 ⇒ 返回**空串**，不会退回模板名。
- 对比：寄售路径 `PlayObject.Sale.cs:88-96` 有“自定义名为空 → 回到模板名”的回退；**交易/商店没有**。这是两条不同口径，不要统一。

#### 2.4.2 `GetCustomItemName`（`:28-41`）

**倒序**扫描，返回**最后一个**同 `(MakeIndex, ItemIndex)` 的 `sItemName`；无命中返回 `""`。

#### 2.4.3 `LoadCustomItemName`（`:80-112`）

- 路径 `Envir/ItemNameList.txt`（`M2Share.GetEnvirFilePath`）。
- `StringList.LoadFromFile`：GB2312（有 BOM 时按 BOM，`StringList.cs:276-330`）。
- 每行 `Trim()` → 三次 `GetValidStr3({' ', '\t'})`；`StrToInt(...,-1)`；`MakeIndex<0 || ItemIndex<0` 跳过；**不去重**、不跳过空行/`;`（靠整型解析失败自然落空）。
- 文件不存在 ⇒ `SaveToFile` 建空文件（**写盘副作用**，Rust `lists.rs` 只读，已登记）。
- Rust 已有逐行等价实现：`crates/data/src/lists.rs:106-123`（parse）、`:129-138`（倒序取）、`:140-175`（add/del）、`:179-...`（文件读取，GBK）。

#### 2.4.4 部署件

`E:/MirServer/M2GameSvr/Envir/ItemNameList.txt`（GB2312，5 行）：

```
1073741869	372	1000
26282853	443	一束红玫瑰
26275438	372	★火神之怒★
28699174	372	玖玖玖朵玫瑰
32043762	372	泰兰德的记忆
```

⇒ 只有当 `UserItem.MakeIndex` 与这些值完全相等且 `Index` 对应时才会命中；`ItemIndex` 372/443 是否存在于 `StdItemList`、以及运行期 `GetItemNumber()` 是否会生成这些值，需真机抽样确认（见未决问题 U2）。

#### 2.4.5 自定义名的写侧（为什么 `Desc[13]==1` 可达）

- 脚本命令 `@@useitemname`（`ScriptFlagConst.UseItemName`）→ `Merchant.ChangeUseItemName`（`Merchant.cs:1765-1800`）：
  `BoChangeItemNameFlag`（脚本 `ExecutionProcessingSys.cs:3513` 置位）→ 槽位号取 `sLabel` 后缀 → 槽位 `Index==0` 则提示 → `Desc[13]==1` 先删旧名 → 新名非空 `AddCustomItemName` + `Desc[13]=1`，否则删名 + `Desc[13]=0` → `SaveCustomItemName()`（**倒序**写文件，`\t` 分隔）→ `RM_SENDUSEITEMS` + `RM_MENU_OK`。
- `Desc` 随 `UserItem` 落库/读档，所以 DB 里的旧物品也可能带 `Desc[13]==1`。
- 相邻 oracle 缺陷：`if (btWhere >= 0 && btWhere <= playObject.UseItems.Length)` 允许 `== Length`，越界异常被 `UserSelect` 外层 `try/catch` 吞掉（照搬时保持 no-op，不要 panic）。

#### 2.4.6 线上编码（`WriteAsciiString`，`BinaryWriterExtension.cs:12-58`）

- `buffer = HUtil32.StringToByte(value)`（GB2312）；空串 ⇒ 长度 0。
- `buffer.Length > 14` ⇒ **截断到 14 字节**，长度字节写 `14`（不是原始长度）；**可能切断双字节汉字**。
- 否则长度字节 = `buffer.Length`，内容补 0 到 14。
- 总计 **1 + 14 = 15 字节**；`Reference` 同规则。
- Rust 已逐字节实现：`itemwire.rs:64-79`（`write_ascii_string`），并有单测 `write_ascii_string_matches_oracle_semantics`（`:445-464`）。

### 2.5 交易路径 vs 包裹/卸下路径（**不要混淆**）

| 行为 | `SendAddItem`（200，拾取/买入） | `SendDelItems`（202） | `SendAddDealItem`（682） / `SendDelDealItem`（683） |
|---|---|---|---|
| `GetUpgradeStdItem` 差量 | 有 | 有 | 有 |
| `GetItemName` | 有 | 有 | 有 |
| `StdMode==50` ⇒ `" #"+Dura` 后缀 | **有**（`Base.cs:212-215`） | **有**（`:1658-1661`） | **无** |
| `StdModeMap` ⇒ `Shape=0/130`（`Desc[8]`） | **有**（`:215-224`） | **无** | **无** |
| `ClientItem.Desc` | 恒 0 | 恒 0 | 恒 0 |
| `DuraMax` | 实例 | 实例 | 实例（详情列表才用价格覆盖） |

Rust 现状：`trade.rs:939-960 add_item_tail`（后缀 + Shape）、`:968-983 del_item_tail`（后缀、无 Shape）、`:989-1007 remote_item_body`（无后缀、无 Shape）——**帧形口径已对**；缺的是 §2.3/§2.2 的内容。

### 2.6 名字比较的相邻差异（同 T-24 家族，建议同卡收敛或显式登记）

| 路径 | oracle | Rust 现状 | 差异 |
|---|---|---|---|
| 交易加/删物品 | `GetItemName` + `OrdinalIgnoreCase`（`Operate.cs:979/1019`） | `item_display_name` + `to_lowercase`（`trade.rs:314-329`） | `Desc[13]==1` 时 Rust 得空串，客户端用自定义名 ⇒ 永远匹配失败；`to_lowercase` 与 .NET OrdinalIgnoreCase 对非 ASCII 大小写折叠有理论差异 |
| 商店买入 | `GetItemName` + `Equals`（**区分大小写**，`Merchant.cs:1234/1237`） | `item_name(catalog,index)` 模板名 + `==`（`shop.rs:1023`） | 自定义名商品无法按自定义名买入；Rust 用模板名 |
| `@buy/@sell` 面板 | `GetItemName`（`Merchant.cs:973`） | `item_name` 模板名（`shop.rs:286`） | 自定义名显示为模板名 |
| 寄售加物品 | 自定义名，空则回退模板名（`Sale.cs:88-96`） | 先按 `Desc[13]==1` 置空，再用 `item_display_name` 回退（但后者对 `Desc[13]==1` 也返回空串 ⇒ 实际仍空，`selloff.rs:551-558`） | 与 oracle 的回退语义不同 |
| 地面拾取/丢弃/背包/装备 | `GetItemName` | `item_display_name`（`bag.rs:465-470`） | `Desc[13]==1` 显示空名 |
| 商店详情/仓库列表 | `GetItemName`（`Merchant.cs:973/1327`） | 模板名 | 同上 |

**收敛建议**：给 `ability::ItemCatalog` 增一个带默认实现的方法
`fn custom_name(&self, make_index: i32, item_index: u16) -> Option<String> { None }`，
`item_display_name` 改为 `if item.desc[13] == 1 { catalog.custom_name(...).unwrap_or_default() } else { template }`。
这样 13 个调用点签名不变；生产 catalog 包装 `Vec<ItemName>`（`crates/data::lists`）即可。寄售路径额外保留“空则回退模板名”的单独分支。

### 2.7 Rust 现状与缺口

| 项 | 现状 | 缺口 |
|---|---|---|
| `GetUpgradeStdItem` 差量 | `bag.rs:674-676 effective_item` 恒等拷贝；`itemwire.rs:32-34` 明写未实现 | 全 switch 未实现；所有 `ClientItem` 产物都用模板 |
| `ClientItem.Desc` | `itemwire.rs:138` 写 `ci.desc`；所有调用点传 `item.desc` | **oracle 恒 0**；Rust 这 14 字节是 `item.desc` 的非零内容——**Desc 14 字节内容不同，帧长不变**（仍 124B，不是“多 14 字节”） |
| 自定义名 | `bag.rs:465-470` 对 `Desc[13]==1` 返回空串；`crates/data/src/lists.rs` 已有完整解析/查询 | 未装载、未接进 catalog；13 个调用点受影响 |
| `StdMode==50` 后缀 | `trade.rs:942-945`（add）、`:970-973`（del）、`remote_item_body` 无 | 已对 |
| Shape 改写 | `add_item_tail` 有、`del_item_tail`/`remote_item_body` 无 | 已对 |
| 帧序/守卫 | `trade.rs` 状态机 | 已对（本片不动） |

### 2.8 写集切分（与 merchant/shop 现有代码的边界）

> 全部落在 `bins/game-svr/**`（当前被其它车道占用），必须串行；`crates/data/src/lists.rs` **无需改动**。

| 片 | 写集 | 内容 |
|---|---|---|
| **N1（差量 + Desc 清零）** | `bins/game-svr/src/bag.rs`（`effective_item`）、`itemwire.rs`（`ClientItem` 构造/编码）、`trade.rs`（三个 tail 函数）、`shop.rs`（详情/仓库）、`ground.rs`、`enter.rs`、`slave.rs`、`main.rs`（背包/装备出帧） | 实现 §2.3 全 switch；`encode_client_item` 写 `[0u8;14]`（或 `ClientItem` 删 `desc` 字段，强制不传）；所有发点改用统一的 `ClientItem::from_item(tpl, item)`。**注意 `DuraMax` 在详情列表仍是 `GetUserPrice` 覆盖值**，不要在统一构造器里抹掉。 |
| **N2（自定义名）** | `bins/game-svr/src/ability.rs`（trait 默认方法）、`bag.rs`（`item_display_name`）、生产 catalog 包装、`main.rs`（启动装载 `ItemNameList.txt`） | `crates/data::lists::load_item_name_list_file` 装载；`custom_name` 用倒序 `get_custom_item_name`；`Desc[13]==1` 无命中返回空串（不回落）。 |
| **N3（相邻命名面，可选同卡）** | `shop.rs`（买/面板/详情）、`selloff.rs`（回退模板名） | 按 §2.6 收敛；如果不同卡，必须在 review 显式登记差异。 |
| **N4（写侧 `@@useitemname`）** | 脚本/NPC 面 | 若实现，`SaveCustomItemName` 是文件落盘 ⇒ **升 R**。本片不做，登记。 |

**不要动的边界**：

- `crates/data/src/lists.rs`、`crates/data` 的 `ItemName` 结构已完整；只读复用。
- `trade.rs` 的 6 条 `CM_DEAL*` 状态机与帧序不要改（已合入且有真机证据）。
- `itemwire::write_ascii_string` 已逐字节等价，不要重写。
- `smframe.rs::sm_additem` 的 `tail` 透传不变；改的是 tail 的**内容生成**。

### 2.9 判据草案（带牙）

#### G-N1 `client_item_desc_bytes_are_always_zero`

- 构造任意 `BagItem.desc=[非零;14]`，走 `add_item_tail` / `del_item_tail` / `remote_item_body` / 商店详情。
- 断言解码后的 124B 体中 `[108..122] == [0;14]`（见 §2.2 偏移表）。
- **牙**：把实现改回 `desc: item.desc` ⇒ 该断言立刻红（**当前生产就是红的**，这是本片的第一条“先红”证据）。还原后用 `sha256sum -c` 对 §0 哈希。

#### G-N2 `upgrade_std_item_weapon_desc_deltas`（V1/V2/V3/V4）

- 逐字段断言 §2.3 的期望值；`Desc` 线上 0。
- **牙**：
  - 把 `AC` 低字节改成 `min255` 或 `byte` 截断 ⇒ V3 红；
  - 把 `MAC` 的丢弃行补回（用 d4 且删掉后面的覆盖行） ⇒ V1 的 `MAC=W(3,11)` 变 `W(8,4)`，红；
  - 把 `GetAttackSpeed` 改成直接 `hi+d6` ⇒ 换一组 `hi=20,d6=3` 区分（`GetAttackSpeed(20,3)`：Real=10+(-3)=7 ⇒ Natural=17；直接加=23），红；
  - 把 `SpecialPwr` 门改成 `d7>=1`（去掉 `<=10`）⇒ d7=11 向量红；
  - 回绕改成 saturating ⇒ V4 红。
- 逐 StdMode 覆盖：5、6、10、11、15、19、20、21、22、23、24、26、52、54，每类至少一条；共 14 条 + default 一条。

#### G-N3 `upgrade_std_item_does_not_touch_name`

- `GetUpgradeStdItem` 前后 `clientItem.Item.Name` 应保持模板名（`CopyItemToClientItem` 设的），最终被 `GetItemName` 覆盖；断言差量函数本身不碰 Name。
- **牙**：若实现在差量函数里按 Desc 改 Name ⇒ 红。

#### G-N4 `deal_remote_item_uses_custom_name_without_fallbacks_or_suffix`

- `Desc[13]=1` + 自定义表有条目 ⇒ `SM_DEALREMOTEADDITEM` 的名字字节 = GBK(custom)，长度 = 字节数（>14 则截断且长度字节=14）。
- 同上但自定义表无条目 ⇒ 名字长度字节 = 0（空），**不是**模板名。
- `StdMode=50` + `Desc[13]=0` ⇒ 交易帧名字**不带** `" #Dura"`；同一物品走 `SendAddItem` 则**带**。
- `Desc[8]!=0` + `StdMode ∈ StdModeMap` ⇒ 交易帧 `Shape` = 模板值（不改 0/130）；`SendAddItem` 改成 130。
- **牙**：在 `remote_item_body` 复用 `add_item_tail` ⇒ 后缀/Shape 两条同时红；在 `GetItemName` 加模板回退 ⇒ 空名向量红。

#### G-N5 `deal_name_comparison_matches_oracle`

- 自定义名条目 `"屠龙宝刀"`：`CM_ADDDEALITEM` 带自定义名 ⇒ 成功；带模板名 ⇒ `SM_DEALADDITEM_FAIL`。
- 无条目但 `Desc[13]=1`（自定义名为空）：带空名 ⇒ oracle 的 `OrdinalIgnoreCase` 比较空==空成立 ⇒ 成功（照搬）；带模板名 ⇒ 失败。
- **牙**：把 `item_display_name` 改为回退模板名 ⇒ 第一/第二条红；把比较改成区分大小写 ⇒ ASCII 大小写向量红。

#### G-N6 `item_name_wire_truncation`

- 8 个汉字（GBK 16B）⇒ 长度字节 14，内容 = 前 14 字节（可能半个汉字）；7 个汉字（14B）⇒ 长度 14、内容完整；空 ⇒ 长度 0 + 14 个 0。
- **牙**：把截断改成按字符（12B/字符边界）或长度字节写原长 ⇒ 红。

#### G-N7 `item_name_list_parse_and_lookup`（数据层已部分覆盖）

- GB2312 5 行样例；重复 `(MakeIndex,ItemIndex)` 时倒序取**最后**一条；`MakeIndex<0` 跳过；空格/Tab 混排；文件名不存在 ⇒ `Ok(None)`（不落盘）。
- **牙**：把倒序改正序 ⇒ 重复条目向量红；把 `-1` 解析改成 `0` ⇒ 负值向量红。

### 2.10 tier 与窗口项

- **tier = L**：纯帧编码/查表/分支；零 RNG、零时序、零落库（只读 `ItemNameList.txt`）。
- **升级为 R 的条件**：实现 `@@useitemname` 写侧（`SaveCustomItemName` 落盘）或把 `ItemNameList.txt` 缺失时的建空文件副作用接进来。
- **窗口项（抽样冒烟）**：
  - **W4**：造一件带 `Desc[0..2]` 的武器（可在构造级直接写 `Desc`，或真机用武器升级 NPC 的取回产物），交易栏添加；对拍 `SM_DEALREMOTEADDITEM` 的 124B 体逐字节（名字 + 属性差量 + Desc 全 0）。
  - **W5**：用 `@@useitemname` 给一件物品起 8 个汉字的名字，再交易/寄售/背包查看；对拍名字截断与显示。部署 `ItemNameList.txt` 已有 5 条，可先用其中一条（`★火神之怒★`，Index=372）构造。
  - **W6**：`Desc[13]=1` 且自定义表无条目 ⇒ 客户端显示空名（oracle 行为）；确认不是 Rust 独有的空名。

---

## 3. 判据总表（实现卡按此勾）

| ID | 面向 | 断言 | 改坏点 → 必红 | tier |
|---|---|---|---|---|
| G-T1 | 税/成员价 | spawn 固定 `castle_index` | 改每帧重算 | L |
| G-T2 | 成员价 | 8 组 `get_user_price` 期望值 | 删 `max(60)` / 只判有行会 / 卖出打折 | L |
| G-T3 | 税 | 买/卖/修/升级 × 9 组 `TotalGold` 增量 | `_tax` 丢弃 / B-16 改税基 / 目的地优先级 | L |
| G-T4 | 税 | round/日上限/金库上限/SaveTick 边界 | 截断 / `>=` / 封顶 else 改累加（`<` vs `<=` 不可观测） | L |
| G-T5 | 税 | 8 组目的地穷举 | 删 `Castle` 优先 / `&&` 代替 `\|\|` | L |
| G-T6 | 税/帧 | 修理查询帧用成员价 | 查询漏传 member | L |
| G-N1 | 物品名 | `ClientItem.Desc` 恒 0 | 改回 `item.desc`（**当前即红**） | L |
| G-N2 | 物品名 | 15 类 StdMode 差量 + V1..V4 | min255/byte 截断、d4 丢弃、AtkSpd 公式、回绕 | L |
| G-N3 | 物品名 | 差量不改 Name | 在差量里改 Name | L |
| G-N4 | 交易帧 | 自定义名/无后缀/无 Shape | 复用 `add_item_tail` / 加模板回退 | L |
| G-N5 | 交易帧 | 名字比较口径 | 回退模板名 / 区分大小写 | L |
| G-N6 | 名字编码 | 14B 截断/长度字节 | 按字符截断 / 写原长 | L |
| G-N7 | 数据层 | 倒序取最后一条/负值跳过 | 正序 / 默认 0 | L |

**数值复算证据**（设计阶段，不跑 cargo）：本节所有期望值已用独立 Python 镜像复算（`python -I /e/tmp/pricedesign-check/check.py` ⇒ `ALL EXPECTATIONS OK`，2026-10-22）。脚本同时验证了“改坏就不同值”的判别力：删 `max(60)` ⇒ 55 档变 44；AC 改 `min255` ⇒ `0x02FF`；AC 改 `byte` ⇒ `0x0201`；保留 d4 ⇒ MAC 低字节 8；`GetAttackSpeed` 直加在 `(20,3)` 档得 23（正确 17）。脚本在仓库外，不随提交进入源码。

**执行纪律**（AGENTS.md「每条判据带牙」）：实现卡必须先让 G-N1/G-N2 在**当前 master** 上红，再改生产路径转绿；每次改坏 → 跑指定断言 → 记录失败输出 → `git checkout -- <文件>` 还原 → `sha256sum -c` 对照 §0 哈希。评测人自己在钉住的 commit 上重跑，不采信作者自述。

---

## 4. 未决问题（需要 owner/审查者裁决或真机输入）

1. **U1 — 城堡表组成**：oracle 生产 `CastleList` = 1 座已初始化城 + 2 座未初始化城（`Castle/List.txt` 的 `0`/`1`）。部署 `GetAllNpcTax=0` 时无观测差异。实现 B-16 分支时是否要复刻这 2 座“死城”？建议：先按部署不可达登记，接 `GetAllNpcTax` 时再补；否则 `CastleMgr` 税会少加到 2 座城。
2. **U2 — `ItemNameList.txt` 的 5 条是否可达**：`MakeIndex` 是运行期 `M2Share.GetItemNumber()` 生成值，静态表里的 1e9 级数值是否仍可能命中、`ItemIndex=372/443` 是否在 `StdItemList` 内，需要真机抽样或读 DB 确认；若不可达，G-N4/N5 只能靠构造级向量。
3. **U3 — `castle_member` 的运行时来源**：部署 `Castle/Castle/SabukW.txt` 的 `OwnGuild=` 为空 ⇒ `MasterGuild=null` ⇒ **当前线上无人享受成员价**。真机窗口 W2 必须先攻城或临时写 `OwnGuild`（后者算改内容，不建议）。Rust 侧只要按 `MasterGuild` 已挂时比较即可，不依赖行会系统是否已接线；但本仓生产目前不赋值 `master_guild`（T-131），所以在接 `GetCastle`/`MasterGuild` 之前，成员价在线上仍不可达（见 §1.8 底层原因）。
4. **U4 — 持久化归属**：`CastleMgr.Save()`/`SaveConfigFile` 的文件字段级对拍属 R；本片是否只做内存计数？建议本片 L、另开 R 卡（与 audit #4「攻城文件/状态机接线」合并）。
5. **U5 — 名字比较的大小写口径**：`to_lowercase`（Unicode full）与 .NET `OrdinalIgnoreCase`（Unicode simple fold）在非 ASCII 上不等价。建议改成逐字符 simple case fold 或显式登记“物品名假定 ASCII/GBK 汉字，无大小写折叠差异”。需审查者定级。
6. **U6 — `Desc[13]` 的双重语义**：它既是“自定义名标记”，又是武器 `Tox` 的增量（§2.3）。实现差量时必须先算 Tox 再按 `Desc[13]` 取名字，顺序不能反；但两者互不影响，建议用一条 V2 向量钉死。
7. **U7 — 商店买入的自定义名**：`ClientBuyItem` 在 oracle 用 `GetItemName` 且 `Equals` **区分大小写**；Rust 现在用模板名。若 N3 不做，需在 review 登记“自定义名商品无法按自定义名买入”。建议同卡做掉（同一 catalog 方法）。

---

## 5. 关键结论（一句话版）

- 税：`Castle.IncRateGold(成交价)` 在买/卖/修/升级四处各收一次，税率 5%，AwayFromZero，日上限 200 万、金库上限 500 万；`CastleMgr` 分支的税基恒为 `UpgradeWeaponPrice=10000`（B-16）；成员价只影响买/修/展示，且只对**城主行会**成员生效。
- 物品名：`GetUpgradeStdItem` 按 StdMode 改**属性字段**（不改 Name、不碰 Desc），Name 由 `GetItemName` 决定（`Desc[13]==1` ⇒ 自定义名，无回退），`ClientItem.Desc` 线上恒 0；交易帧**没有** `StdMode==50` 后缀与 Shape 改写。
- Rust 的公式/税算术/数据层都已存在，缺的是**接线**（税意图、`castle_index`、`castle_member`）与**帧内容**（desc 差量、Desc 清零、自定义名）。
