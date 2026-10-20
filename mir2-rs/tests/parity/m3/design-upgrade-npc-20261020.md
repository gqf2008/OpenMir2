# Lane A 派工简报：武器升级 NPC（`@upgradenow` / `@getbackupgnow`）

- 基线：`b0e618c33`（master）。只读车道：未建 worktree、未改仓库、未起进程、未碰 `E:/MirServer`（仅只读核对）。
- 对应审计：#10（`mir2-rs/tests/parity/m3/audit-uncovered-surfaces-20261019.md:22`）+ 评审员更正 D-4（**:108**：属“已登记未接线”，派卡须引用既有登记，不当新发现）。
- **tier = R**：RNG 抽数顺序 + 时限门（`GetBackTick`）+ 物品/持久化字段；按 `AGENTS.md`「判据分级」必须真机窗口对拍（本简报给出构造级判据与牙，真机窗口另排）。
- 一句话：**提交升级**（扣 1 万金币 + 吃掉包里所有黑铁矿/首饰，武器移出装备、算 Dc/Sc/Mc/Dura 存起来）与**取回**（默认 10 分钟后，按 Dura 抽数决定耐久、按 Dc/Mc/Sc 抽数决定 `Desc[10]` 标记）两条指令，**提交路径零 RNG，取回路径 RNG 密集**。

---

## 0. 可达性与既有登记（不重复排队）

| 事实 | 证据 |
|---|---|
| 部署确有该 NPC：`沙巴克铁匠 3 332 317 沙巴克铁匠 0 0 0`（Map=3=盟重省，非攻城区） | `E:/MirServer/M2GameSvr/Envir/MerChant.txt:82`（GBK） |
| 脚本头声明两标签；正文用 `@upgradenow`/`@getbackupgnow`，结果标签 `~@upgradenow_ok/_ing/_fail`、`~@getbackupgnow_ok/_ing/_fail`；**脚本里没有 `~@getbackupgnow_bagfull`** | `Envir/Market_Def/沙巴克铁匠-3.txt:1/8/120/126-152` |
| `CastleMerchant`（生产加载器）= `StrToInt(第8列,0) != 0`；本行第8列=0 ⇒ **false**。注意：`ReLoadMerchants()` 的变体是 `!= 1`（`LocalDB.cs:1050`），但该函数唯一调用点被注释（`ReloadNpcCommand.cs:24`）⇒ **生产不可达，不要照它移植** | `LocalDB.cs:412-476`（生产）/ `:989-1059`（死变体） |
| 城堡税对本 NPC 全部不触发：`CastleMerchant=false`、`GetAllNpcTax=0`、`Castle=null`（332,317 不在攻城图） | `Server.conf:345`；`Merchant.cs:544-556` |
| Rust 已有：脚本头标志解析、商人实体与 `MerchantFlags`、商店标签派发骨架、RNG 流、ClientItem 编码、能力重算 | 见 §3 |
| 已登记缺口（必须引用，不得当新发现）：① `smframe.rs:207`「`CheckWeaponUpgrade` 本仓未建模」；② `whitelist.md` T-79⑥「`SM_UPDATEITEM(203)` 武器升级发点不可达」与 T-79②「`SM_BREAKWEAPON`」；③ 审计 #15/T-24「`GetUpgradeStdItem` 差量未建模」；④ 审计附B「`upgradeinfo*` 三表 = 空桩」；⑤ `shop.rs:33`、`main.rs:4242-4246`「`Market_Def` 商人脚本未装载」 | 同左 |

---

## 1. 标签语义（守卫顺序、费用、失败码、回包）

### 1.1 派发守卫顺序（`Merchant.cs:787-960`，**顺序即语义**）

1. `base.UserSelect`（`NormNpc.cs:1633-1660`）：只维护 `ScriptGotoCount/ScriptCurrLable/ScriptGoBackLable`，不拦截。
2. **城堡战门**（`:794`）：`if (!CastleMerchant || !(Castle != null && Castle.UnderWar))` —— 不满足则**整段 body 不执行**（连 say 都不发）。部署 `CastleMerchant=false` ⇒ 恒进入。
3. `!playObject.Death && !string.IsNullOrEmpty(sData) && sData[0]=='@'`。
4. `sLabel` = `GetValidStr3(sData,…, '\r')` 首段；`playObject.ScriptLable = sData`（整串，含 `\r` 之后部分）。
5. `boCanJmp = LableIsCanJmp(sLabel)`（`PlayObject.Base.cs:2809-2826`）：命中 `@main` / `CanJmpScriptLableMap` / `PlayDiceLabel` 才 true。该表由**上一次商人说词**里 `<label/@xxx>` 经 `GetScriptLabel`（`Base.cs:2777-2805`）重建 ⇒ **必须先让玩家看到含 `<…/@upgradenow>` 的页面**（部署在 `[@confirmupgrade]`）才能触发。
6. `GotoLable(actor, sLabel, !boCanJmp)`（`ScriptEngine.cs:131-139`）：存在同名脚本标签才跳；`@upgradenow` 不是脚本标签 ⇒ 无动作。
7. **`if (!boCanJmp) return;`** —— 未注册标签到此为止，后面的命令派发**全部跳过**。
8. `else-if` 链按固定序匹配（`:800-955`）；本文两条位于 **`:883`（`@upgradenow`）** 与 **`:890`（`@getbackupgnow`）**（其后还有 `@getmarry`/`@getmaster` 也调 `GetBackupgWeapon`，部署无）。
9. 各自再查脚本头标志：`IsUpgradenow` / `IsGetBackupgnow`（`ScriptParsers.cs:741-749`；Rust 已解析并搬到 `MerchantFlags`）。

> 与 Rust 现状的差异（既有、非本卡引入）：`bins/game-svr/src/main.rs:4042-4078` **没有** `LableIsCanJmp` 门，也没有城堡战门；任何客户端可直接发 `@upgradenow`。本卡可沿用现状，但应在 review 登记；若要真等价，需在 `ShopSession` 增 `can_jmp_labels` 并由说词路径填充（会牵动所有商店标签，建议单开卡）。

### 1.2 费用与材料

- **金币**：`Config.UpgradeWeaponPrice`，部署 **10000**（`Server.conf:333`；`GameSvrConf.cs:1420` 同默认）。`gold >= price` 才进；命中后 `DecGold(price)` + `GoldChanged()`。
- **材料**：要求 `CheckItems(Config.BlackStone) != null`（`BaseObject.cs:2189`，包里**至少 1 件**按名 OrdinalIgnoreCase 命中）。部署 `BlackStone=黑铁矿石`（`Server.conf:87`，物品 Index=68、StdMode=43、DuraMax=5000，`sql/mir2_data.sql:1058`）。
- **实际消耗**：`UpgradeWaponAddValue` 会吃掉**背包里全部**黑铁矿与**全部**首饰（见 §2.2），不只是 1 件。
- 税：`if (CastleMerchant || GetAllNpcTax) { Castle?…: CastleMgr?… }`（`:544-556`）—— 部署两条件皆 false，**不触发**；若实现，走 `crates/world/src/castle_tax.rs`（B-16：CastleMgr 分支税基恒为 `UpgradeWeaponPrice`）。

### 1.3 失败码 = 标签名（无数字码）

| 场景 | 跳转标签（`ScriptFlagConst.cs:52-60`） | 部署脚本是否有该标签 | 客户端可见回包 |
|---|---|---|---|
| 该 NPC 已有同名升级记录 | `~@upgradenow_ing` | 有 | 1 条 `SM_MERCHANTSAY(643)` |
| 其余失败（无武器/钱不够/无黑铁） | `~@upgradenow_fail` | 有 | 1 条 say |
| 受理成功 | `~@upgradenow_ok` | 有 | 1 条 say |
| 包满（46 件） | `~@getbackupgnow_bagfull` | **无** | **无 say**（`GotoLable` 空跳） |
| 无升级记录 | `~@getbackupgnow_fail` | 有 | 1 条 say |
| 记录存在但未到时限 | `~@getbackupgnow_ing` | 有 | 1 条 say |
| 取回成功 | `~@getbackupgnow_ok` | 有 | 1 条 say |

say 帧形状：`SM_MERCHANTSAY(643)` `Recog=商人 ActorId`、`Param=0/Tag=0/Series=1`、尾 `EncodeString(商人名 + "/" + 文本)`（`PlayObject.Message.cs:1723-1730`；`NormNpc.cs:1615`）。Rust 已有同形（`shop.rs` 的 `SM_MERCHANTSAY` 用法）。

---

## 2. 升级链

### 2.1 提交：`UpgradeWapon`（`Merchant.cs:527-586`）——**零 RNG**

```
1  for i in 0..UpgradeWeaponList: if e.UserName == user.ChrName  (精确 ==，**区分大小写**)
       → GotoLable(~@upgradenow_ing); return            // 先于一切副作用
2  if UseItems[1] != null && UseItems[1].Index != 0
      && Gold >= UpgradeWeaponPrice
      && CheckItems(BlackStone) != null:                  // && 左到右：前两项不过就不查材料
3      DecGold(price); [税分支]; GoldChanged()
4      info = { UserName, UserItem = new UserItem(UseItems[1]) }   // 拷贝构造 **Desc 数组是浅拷贝**
5      SendDelItems(UseItems[1])                          // 直发 SM_DELITEM(202)
6      UseItems[1].Index = 0                              // 槽位对象保留（不是置 null！）
7      RecalcAbilitys(); FeatureChanged(); SendMsg(RM_ABILITY)   // 52
8      UpgradeWaponAddValue(user, user.ItemList, ref info.Dc/Sc/Mc/Dura)
9      info.UpgradeTime = DateTime.Now; info.GetBackTick = GetTickCount()
10     UpgradeWeaponList.Add(info); SaveUpgradingList()   // 空桩
11 if success → ~@upgradenow_ok else → ~@upgradenow_fail
```

要点：
- **武器不由客户端选择**，固定 `ItemLocation.Weapon = 1`。
- 第 6 步保留槽位对象、只把 `Index` 清 0；后续 `RecalcAbilitys` 以 `Index > 0` 过滤（`PlayObject.Base.cs:1024`）。Rust 必须表示成 `Some(BagItem{index:0, ..原 make_index/desc})`，**不能** `None`（否则存档/重登行为会分叉）。
- `new UserItem(src)` 的 `Desc = src.Desc` 是**同一数组引用**（`ClientUserItem.cs:46-52`）。此处后续只改 `info.UserItem.Desc[10]`，而原槽已 `Index=0`，故 oracle 下无副作用；Rust 用 clone 即可，但**不要在取回时把 Desc 写回空槽**。
- 重复检查用区分大小写的 `==`，取回匹配用 `OrdinalIgnoreCase`（`:603`）——**不对称，照抄**。

### 2.2 加值：`UpgradeWaponAddValue`（`:373-525`）——**零 RNG**

对 `user.ItemList`（背包）**从后往前**扫一遍：
- 名（`GetStdItemName(Index) == Config.BlackStone`，**精确 `==`**）命中 ⇒ `duraList.Add(Round(Dura/1000.0))`，记 `DeleteItem{ItemName=Config.BlackStone, MakeIndex}`，**删除**。
- `M2Share.IsAccessory(Index)`（StdMode ∈ {19,20,21,22,23,24,26}，`M2Share.cs:187/207`）⇒
  `GetUpgradeStdItem`（`GameItemSystem.cs:731+`）后按 **`clientItem.Item.StdMode`**：
  - 19/20/21、22/23：`n = HiWord(X) + LoWord(X)`（X=DC/MC/SC）
  - 24/26：同上 **+1**
  - 维护“最大、次大”三元组（`if nMin<n {nMax=nMin; nMin=n} else if nMax<n {nMax=n}`，逐项独立）
  
  记 `DeleteItem{ItemName=stdItem.Name, MakeIndex}`，**删除**。
- 其余物品不动。

收尾（**畸形但必须照抄**）：
```
for i in 0..duraList.len:
  for j in (i+1..duraList.len).rev():
     if duraList[j] > duraList[j-1] { duraList.Reverse(); }     // 整体反转，**不是排序**
nDura = Σ 前 5 项;  nItemCount = min(5, len)
Dura = (byte)Round( min(5,n) + min(5,n) * (nDura / n / 5.0) )   // 先整除再 /5.0
Dc   = (byte)(nDcMin/5 + nDcMax/3)                              // 其余两列同形
```
- `nItemCount ≥ 1` 有保证（提交前已确认至少 1 件黑铁），故 `/ n` 不除零。
- 黑铁**全部删除**，但 Dura 只累计“畸形置换后的前 5 项”。
- 5 件满耐久黑铁（Dura=5000 ⇒ 每项 5）⇒ `Dura = 5 + 5*(25/5/5.0) = 10`；1 件 ⇒ 2。
- 删件集合经 `RM_SENDDELITEMLIST(10148)` → `SM_DELITEMS(709)`（`Series=条数`，尾 `EncodeString("名/makeindex/…")`，顺序 = 背包倒序）。

### 2.3 取回：`GetBackupgWeapon`（`:592-759`）——**RNG 密集**

```
1  if !IsEnoughBag()  (ItemList.Count < 46) → GotoLable(~@getbackupgnow_bagfull); return   // 先查包，后找记录
2  nFlag=0; for i in 0..list:
      if OrdinalIgnoreCase(list[i].UserName, user.ChrName):
         nFlag=1
         if (GetTickCount() - GetBackTick) > UPgradeWeaponGetBackTime   // 严格 >
            || user.Permission >= 4:
            取走该项; SaveUpgradingList(); nFlag=2; break
3  if info.UserName 非空:
     耐久分支（见 §2.4 A）
     n1C = (Dc==Mc==Sc) ? Random(3) : -1
     DC/MC/SC 三支（见 §2.4 C/D/E），写 Desc[10]
     SendAddItem(info.UserItem)        // 直发 SM_ADDITEM(200)；**没有 AddItemToBag**（`:744` 被注释）
4  switch nFlag: 0→~@getbackupgnow_fail; 1→~@getbackupgnow_ing; 2→~@getbackupgnow_ok
```

**oracle 缺陷（照抄 + 登记）**：`user.AddItemToBag(userItem)` 被注释（`:744`）⇒ 武器只发给客户端、**不进 `ItemList`**；存档不含它，小退/重登即丢。必须写一条“断言 `item_list` 长度不变”的用例把它钉住，并在 review 里写明这是 BUG-REPLICA。

时限：`UPgradeWeaponGetBackTime` 部署 **600000 ms = 10 分钟**（`Server.conf:331`；默认 1h 见 `GameSvrConf.cs:1421`）。`Permission` 在生产恒 0（唯一赋值点 `GetHumPermission` 的调用被注释，`PlayObject.cs:1188`；GM 命令/脚本可改，本仓库未接）⇒ 生产 GM 豁免不可达。

### 2.4 RNG 抽数点与顺序（**取回路径**，固定流）

按代码顺序，**严格** A → B → C → D → E：

| 序 | 条件 | 抽取 | 备注 |
|---|---|---|---|
| A | `Dura ∈ [9,15]` | `Random(Dura)`（1 次） | `< 6` ⇒ `DuraMax>1000` 时 `-=1000` |
| A | `Dura ∈ [18,255]` | `Random(Dura-18)`（1 次） | **`Dura==18` ⇒ `Random(0)`，仍消耗 1 次**；`1..4→+1000`、`5..7→+2000`、`8..255→+4000` |
| A | `Dura ∈ [0,8]` | 0 次 | `DuraMax>3000 ? -=3000 : >>=1`（右移一半） |
| A | `Dura ∈ {16,17}` | **0 次** | 三档都不命中，耐久不变 |
| B | `Dc==Mc==Sc` | `Random(3)`（1 次） | 否则 `n1C=-1`。**n1C 永远不影响分支选择**（全相等时三支条件本来就全真）⇒ 纯“空转抽” |
| C | `Dc>=Mc && Dc>=Sc \|\| n1C==0` | `Random(UpgradeWeaponDCRate)`；成功且 `n10>63` ⇒ `Random(DCTwoPointRate)`；成功后仍 `n10>79` ⇒ `Random(DCThreePointRate)` | 失败支不再抽 |
| D | `Mc>=Dc && Mc>=Sc \|\| n1C==1` | 同上，MC 三键 | |
| E | `Sc>=Mc && Sc>=Dc \|\| n1C==2` | 同上，SC 三键 | **照抄 bug：`n90 = min(11, upgradeInfo.Mc)`（用的是 Mc 不是 Sc）** |

判定式（三支同形）：
```
n90 = min(11, stat)
n10 = min(85, (n90 << ((3 - n90) & 31)) + 10 + UserItem.Desc[3] - UserItem.Desc[4] + BodyLuckLevel)
成功 ⇔ Random(Rate) < n10
成功时 Desc[10] = 10/20/30；n10>63 且 Random(TwoPoint)==0 ⇒ 11/21/31；n10>79 且 Random(ThreePoint)==0 ⇒ 12/22/32
失败时 Desc[10] = 1
```
- `<<` 的 C# 语义：`3 - n90` 为负时按 `& 31` 掩码（Rust 用 `wrapping_shl`）。
- 部署 `DCRate=MCRate=SCRate=50`、`TwoPoint=30`、`ThreePoint=200`（`Server.conf:294-302`）。
- n90→n10 基数（不计 Desc/luck）：`0→10, 1→14, 2→14, 3→13, 4→10, 5→85(cap), 6→负数(必失败), 7..11→85(cap)`。**n90=6 恒失败是很强的牙**。
- 多支同时成立时**按 DC→MC→SC 顺序覆盖**，最后一支赢。

### 2.5 持久化（哪张表哪个字段）

| 对象 | oracle 落点 | 结论 |
|---|---|---|
| 在途升级列表 `WeaponUpgradeList` | `SaveUpgradeWeaponRecord` → `MySqlDB.cs:345-348` **恒 return -1，无 SQL**；`LoadUpgradeWeaponRecord` 同（`:339-343`） | **不落库**，进程内、按 Merchant 实例（`ScriptName-MapName`）保存；重启即丢。Rust 照抄：内存态，不实现 `upgradeinfo*` 三表（审计附B） |
| 取回后的武器 | 若进背包则随 `characters_item_attr.VALUE10` 存 `Desc[10]`（`playdata.rs:625/994`） | 但 `AddItemToBag` 被注释 ⇒ 实际不会进背包/不会落库；重登丢失 |
| `Desc[10]` 之后如何生效 | 下一次**攻击**时 `CheckWeaponUpgrade`（`PlayObject.Attack.cs:28-33` → `PlayObject.cs:4110-4172`）把 `10/20/30` 转成 `Desc[0]/[1]/[2]` 并清零；`Desc[10]==1` 则 `Index=0`（武器碎）+ `SM_BREAKWEAPON(1102)` + say；成功则 `SM_UPDATEITEM(203)` + 52/752 | **依赖武器破零链**：Rust 未建模（T-79②⑥、`smframe.rs:207`）⇒ 本卡交付后，取回武器永远只是带 `Desc[10]` 的死数据，直到那条链补上 |
| 过期清理 | `Merchant.Run` 每 10min（`:126-131`）→ `ClearExpreUpgradeListData`（`:1076-1088`）：`(DateTime.Now - UpgradeTime).TotalDays >= ClearExpireUpgradeWeaponDays` 则删 | 部署 3 天（`Server.conf:332`）。oracle 用墙钟；Rust 用 sim 时钟（`world.clock_ms`）等价（进程内无差异），需登记 |

### 2.6 帧序（必须逐字节钉死）

**提交成功**（`SendMsg` 队列优先级 = `(byte)wIdent`，低字节升序，`BaseObject.Message.cs:98`）：
1. **立即**（`SendSocket` 直发）：`SM_DELITEM(202)` `Recog=ActorId, Param=0, Tag=0, Series=1`，尾 `EncodeBuffer(ClientItem(武器))`。
2. 排队：`RM_MERCHANTSAY(11009→prio **1**)` → `SM_MERCHANTSAY(643)` `~@upgradenow_ok` 文本；
3. `RM_ABILITY(10051→67)` → `SM_ABILITY(52)` `Recog=Gold, Param=MakeWord(job,99), Tag=LoWord(GameGold), Series=HiWord(GameGold)`，尾 `EncodeMessage(WAbil)`；
4. `RM_FEATURECHANGED(10116→132)` → `SM_FEATURECHANGED(41)` 对 12 格内可见者（含自己）；
5. `RM_GOLDCHANGED(10136→152)` → `SM_GOLDCHANGED(653)` `Recog=Gold, Param=LoWord(GameGold), Tag=HiWord(GameGold), Series=0`；
6. `RM_CHARSTATUSCHANGED(10139→**155**)×2` → `SM_CHARSTATUSCHANGED(657)`：`RecalcAbilitys` 尾段
   `CharStatus = GetCharStatus(); StatusChanged(); SendUpdateMsg(RM_CHARSTATUSCHANGED, …)`
   （`PlayObject.cs:2874-2876`）。`StatusChanged()` 的 `SendRefMsg` 给 12 格内玩家（含自己）一条，
   `SendUpdateMsg` 的去重因 `TryPeek` 不前进而无效 ⇒ 自己**两条**；`Series` = 每个**收件人自己**的
   `HitSpeed`（`PlayObject.Message.cs:1846` 的 `this.HitSpeed`）。近邻支只收一条；
7. `RM_SENDDELITEMLIST(10148→164)` → `SM_DELITEMS(709)` `Series=条数`，尾 `EncodeString("名/makeindex/…")`。

⇒ 线上自助视角顺序：**SM_DELITEM → SM_MERCHANTSAY → SM_ABILITY → SM_FEATURECHANGED → SM_GOLDCHANGED → [SM_CHANGELIGHT(654)] → SM_CHARSTATUSCHANGED(657)×2 → SM_DELITEMS**（say 优先级 1，会插到最前，**不是**调用序；654 仅 `oldlight != Light`，本宿主无展示表 ⇒ 不发）。近邻视角：`SM_FEATURECHANGED(41) → [654] → 657`。本路径**不发 752**。

**提交失败**：只有一条 `SM_MERCHANTSAY`。**取回成功**：`SM_ADDITEM(200)`（`ActorId,0,0,1` + `EncodeBuffer(ClientItem)`）立即，然后 `SM_MERCHANTSAY`；**无 52/41**。

---

## 3. 复用面与写集

### 可复用（禁止复制第二份实现）
- 标签比较：`shop::eq_label`（ASCII 大小写不敏感，同 `OrdinalIgnoreCase` 口径）。
- 出帧：`ShopFrame` + `main.rs:3682 emit_shop_frames`（`tail_encoded` 区分 `SendSocket` 直发 vs `SendDefMessage` 的 `EncodeString`）。
- 物品线缆：`itemwire::ClientItem` / `encode_client_item`（124B 定长）/ `mir2_protocol::edcode::encode`；`smframe::sm_delitem` 已存在。
- 能力：`main.rs:5586 recalc_session_ability`；`record::encode_ability` / `encode_ability_client`（52 的 40B/58B 口径开关，默认 58B，与登录帧一致）。
- 金币：`shop::dec_gold`（`gold >= n`）。
- 税：`crates/world/src/castle_tax.rs`（本部署不触发）。
- RNG：`world.rng`（`mir2_shared::rng::RandomNumber::random_below`；`Random(0)` 仍消耗 1 抽，与 .NET 一致）。
- 脚本标志：`crates/script` 已解析 `is_upgradenow/is_get_backupgnow`，`bins/game-svr/src/merchant.rs:65` 已搬到世界态。

### 写集（按文件，含理由）
1. `mir2-rs/crates/world/src/merchant.rs`：`WeaponUpgradeInfo{user_name,user_item,dc,sc,mc,dura,upgrade_time_ms,get_back_tick:i32}`；`MerchantState.upgrade_weapon_list: Vec<_>`（**默认空、不要改 `MerchantState::new` 签名**，避免大面积调用点改动）；`upgrade_weapon(...)` / `get_back_weapon(...)` / `clear_expired_upgrades(...)` 纯函数；**RNG 抽取必须落在这里**（见下“红线”）。
2. `mir2-rs/bins/game-svr/src/shop.rs`：`LABEL_UPGRADENOW/LABEL_GETBACKUPGNWOW` 常量 + `user_select` 两个分支（返回“需宿主执行”的标记或直接调 world 纯函数）。
3. `mir2-rs/bins/game-svr/src/main.rs::handle_shop_cmd`：取 `sg.state.gold/item_list`、`sg.use_items[1]`、`sg.equip.body_luck_level`、`world.rng`、`world.entities[merchant].merchant.upgrade_weapon_list`、`now_tick = world.clock_ms as i32`；成功后 `recalc_session_ability`；`emit_shop_frames`；`goto_label` 只登记（脚本未装载）。
4. `mir2-rs/bins/game-svr/src/config.rs`：新增读 `[Names] BlackStone`、`[Setup] UpgradeWeaponPrice / UPgradeWeaponGetBackTime / ClearExpireUpgradeWeaponDays / UpgradeWeaponMaxPoint / UpgradeWeapon{D,M,S}C{Rate,TwoPointRate,ThreePointRate}`、`StartPermission`（可只留 `permission: u8` 恒 0）。
5. `mir2-rs/crates/world/src/merchant.rs::merchant_run`：10min 门到点调 `clear_expired_upgrades`（现在只推进 tick，无可观测效果，见该文件 `:26-27` 登记）。
6. RNG 账本/基线：新增站点（oracle `Merchant.cs:677/698/719` 的 `Random(Rate)` 与 `:681/685/702/706/723/727` 的加抽）需进 `mir2-rs/tests/parity` 的 RNG 账本基线；**站点文件不得是 `bins/game-svr/src/main.rs`**（`tools/m2-acceptance/check_rng_trace_purity.ps1` 的纯度门会红）。
7. 测试：`mir2-rs/bins/game-svr/src/shop.rs` 单测 + `mir2-rs/crates/world/src/merchant.rs` 单测（夹具 + 固定种子）。

### 冲突面
- `main.rs`、`shop.rs` 是热文件（商店/税/交易/寄售车道）；`crates/world/src/merchant.rs` 与商人实体车道重叠。建议本卡**只动 merchant 世界态 + 一个独立 `shop.rs` 分支 + `main.rs` 的一处 `match` 臂**，其余逻辑放 world，减少写集。
- 接口冻结线 `crates/protocol`/`crates/storage`：本卡**不需要**改（消息号齐全）。

---

## 4. 判据（构造级，≥6 条，每条带牙）

夹具真源（部署值）：黑铁矿石 Index=68（StdMode 43，DuraMax 5000）、price=10000、getback=600000、days=3、rates 50/30/200、MaxPoint=7、`CastleMerchant=false`、`GetAllNpcTax=false`。

**C1 提交成功状态与费用**
- 武器 Index=5/6、gold=20000、包里 5×黑铁（Dura 5000）+ 2 件首饰（StdMode 22 / 24）。
- 断言：gold=10000；`use_items[1]` 仍 `Some`、`index==0`、`make_index/desc` 保留；背包黑铁/首饰全消失；`upgrade_weapon_list.len()==1` 且 `UserItem` 四字段拷贝正确；`dc/sc/mc/dura` 等于手算值；`get_back_tick==now_tick`。
- 牙：把 `use_items[1]=None`（而非 index=0）⇒ 红；把 `nDcMin/5 + nDcMax/3` 写成 `/5+/5` ⇒ 红。

**C2 黑铁计数与“畸形置换”（含 >5 件）**
- 6 件黑铁 Dura 值 [3,1,4,1,5,9]（×1000）。oracle 置换后仍是 `[3,1,4,1,5,9]` ⇒ `nDura=14`、`n=5`、`Dura=Round(5+5*(14/5/5.0))=7`；真降序排序会得 `[9,5,4,3,1,1]` ⇒ 9。
- 断言：`dura==7`；`SM_DELITEMS` 条数=6（**全部**黑铁被吃，不只 5 件）；顺序 = 背包倒序。
- 牙：把畸形循环换成真排序 ⇒ 9（红）；把删除截断到 5 件 ⇒ 条数红。

**C3 提交门控顺序**
- 四例：① 列表已有同名（大小写不同）⇒ `~@upgradenow_ing`、金币/背包/列表全不动；② 武器槽 index=0 ⇒ fail；③ gold=9999 ⇒ fail 且**金币不变、材料不消耗**；④ gold 足够但无黑铁 ⇒ fail 且金币不变。
- 牙：把 `DecGold` 提到 `CheckItems` 之前 ⇒ ③/④ 红；把重复检查移到扣款后 ⇒ ① 红。

**C4 取回时限三态 + 包满**
- `now = get_back_tick + 600000` ⇒ `_ing`（严格 `>` 才 ok）；`+600001` ⇒ `_ok` 且该条从列表移除；无记录 ⇒ `_fail`；背包 46 件 ⇒ `_bagfull` 且**记录仍在**。
- 断言：`_ok` 时 `item_list.len()` **不变**（钉死 `AddItemToBag` 被注释的 oracle 缺陷）。
- 牙：`>` 改 `>=` 在边界红；把包满检查挪到查找之后 ⇒ 红。

**C5 取回耐久分支的抽数与边界**
- Dura=5 ⇒ `DuraMax>3000 ? -3000 : >>1`，并 clamp `Dura`；Dura=12（固定种子）⇒ 走 `Random(12)<6`；Dura=16/17 ⇒ 不改耐久且**不抽**；Dura=18 ⇒ 抽一次 `Random(0)`（结果 0，不改耐久）。
- 牙：把 `Dura==18` 提前 return 以“省掉无意义抽取” ⇒ 后续流偏移 ⇒ 红（用同一流下一次 `random()` 与对照比）。

**C6 升级判定 RNG 顺序（核心）**
- 例 A（无首饰，Dc=Mc=Sc=0）：断言抽取序列 = `Random(3), Random(50), Random(50), Random(50)`（n90=0 ⇒ n10=10，成功也不触发两点）；用同种子手算期望 `Desc[10]`。
- 例 B（首饰给 Dc=7，Mc=Sc=0）：无 n1C；DC 支 `Random(50)` 必成（n10=85），随后 `Random(30)`、`Random(200)`；MC/SC 支不成立 ⇒ 共 3 抽，`Desc[10] ∈ {10,11,12}` 与手算一致。
- 例 C（Dc=6）：n10 为负 ⇒ 任何种子 `Desc[10]==1`（分支仍跑，仍抽 `Random(50)`）。
- 牙：删掉 n1C 的 `Random(3)` ⇒ 例 A 流偏移红；调换 DC/MC/SC 顺序 ⇒ 全相等用例（三支全成）`Desc[10]=30` 变 10 ⇒ 红。

**C7 SC 支照抄 bug（`n90 = min(11, upgradeInfo.Mc)`，`:721`）**
- 构造 `Dc=0, Mc=3, Sc=6`（无 n1C：三值不等）：DC 支不成立、MC 支不成立（`3>=6` 假），**只有 SC 支成立**。oracle 的 SC 支取 `n90=min(11,Mc)=3` ⇒ `n10=13`（部署 rate=50 ⇒ 约 26% 成功）；若“修正”成用 `Sc`，则 `n90=6` ⇒ n10 为负 ⇒ 恒失败。
- 断言：挑一颗 `Random(50)<13` 的种子，oracle 得 `Desc[10]∈{30,31,32}`；“修正版”得 1。
- 牙：把 SC 的 `Mc` 改成 `Sc` ⇒ 该种子红。

**C8 帧序与字节**
- 提交成功断言完整序列与头字段：`SM_DELITEM(202, Recog=ActorId, Series=1)` → `SM_MERCHANTSAY(643, Recog=商人, Series=1)` → `SM_ABILITY(52, Recog=Gold, Param=MakeWord(job,99), Tag/Series=GameGold 低/高)` → `SM_FEATURECHANGED(41)` → `SM_GOLDCHANGED(653)` → `SM_CHARSTATUSCHANGED(657)×2`（`Recog=ActorId, Param/Tag=Lo/Hi(CharStatus), Series=HitSpeed`） → `SM_DELITEMS(709, Series=count)`；尾逐字节。近邻另断言 `41 → 657`。
- 取回成功：`SM_ADDITEM(200)` 先于 say。
- 牙：把 say 的队列优先级写成 643（而非 1）⇒ 顺序红；`SM_DELITEMS` 的 `Series` 写 0 ⇒ 红。

**C9 标签/标志门**
- `flags.is_upgradenow=false` 时 `@upgradenow` no-op（`@getbackupgnow` 同）；`CastleMerchant && UnderWar` 时整段派发跳过（合成用例，部署不可达）。
- 牙：漏掉标志检查 ⇒ 红。

**C10 过期清理**
- `UpgradeTime` 距今 3 天整 ⇒ 删除（`>=`）；2 天 23:59 ⇒ 保留；同时断言清理不影响其它记录。
- 牙：`>=` 改 `>` ⇒ 边界红。

（可选 C11：`GetUpgradeStdItem` 差量 —— 本卡不实现，见 §5；但 `SM_DELITEM` 尾对“已升级过的武器”会与 oracle 不一致，属 #15/T-24，必须在 review 显式声明。）

---

## 5. 已知缺口引用（不要在本卡“顺手”重做）

1. **`Market_Def` 商人脚本未装载**（`shop.rs:33`、`main.rs:4242-4246`）：`~@upgradenow_*`/`~@getbackupgnow_*` 的 say 文本**当前发不出**。本卡建议：handler 返回 `goto_label`，宿主按修理路径 `glog!` 登记；C8 对 say 的断言先在“标签名”层判，等脚本装载卡落地再升级为 `SM_MERCHANTSAY` 字节判据。若派工方要求一次做全，需把 `crates/envir` 的 `MerchantRecord` 保留 `outcome.scripts` 并注册进 `NpcHost`（写集扩大，建议单开）。
2. **`GetUpgradeStdItem` 差量**（#15/T-24）：`bag.rs:674 effective_item` 是恒等拷贝 ⇒ 已带 `Desc[0..2]` 的武器在 `SM_DELITEM`/`SM_ADDITEM` 尾里与 oracle 不同。
3. **武器破零链**（`CheckWeaponUpgrade`，T-79②⑥）：`Desc[10]` 标记→属性/碎武器未实现 ⇒ 取回武器在 Rust 侧只是死数据。
4. **`upgradeinfo*` 三表**（审计附B）：oracle 空桩，**不实现**。
5. **`CanJmpScriptLableMap` 门**（`PlayObject.Base.cs:2809-2826`）：Rust 全商店标签都未建模，非本卡引入；若要补，单开卡。

---

## 6. 需 owner/审查者裁决

1. say 文本随脚本装载卡延期（推荐）还是一次做全？
2. `Permission >= 4` 生产恒 0（赋值点被注释）⇒ 是否直接以常量 0 建模（推荐）？
3. 在途列表用 sim 时钟替代 `DateTime.Now`（过期清理）——按“进程内等价”登记，是否需要 R 级真机窗口覆盖重启场景（建议：不需要，因 oracle 重启也丢）。
4. #15 的 `GetUpgradeStdItem` 会让帧字节不一致：本卡是否必须等它，还是按“同卡前置、不同卡并行”登记 `window_required`？

---

## 摘要（≤1000 字）

本卡=武器升级 NPC 两条指令 1:1 接线，tier=R。守卫：UserSelect→城堡战门（部署 false）→`@`→LableIsCanJmp（Rust 未建模）→派发（Merchant.cs:883/890）→脚本头标志。提交 UpgradeWapon（:527-586）零 RNG：同名记录（区分大小写）→武器 index≠0/金币≥10000/包内有黑铁→扣 10000→发 SM_DELITEM→槽 Index=0（对象保留）→52/41→AddValue 吃掉全部黑铁与首饰并算 Dc/Sc/Mc/Dura（整体反转伪排序，Dura 只累前 5 件）→SM_DELITEMS→say `_ok`；失败仅 `_ing/_fail`。取回（:592-759）：包满门→不区分大小写找记录→未到期 `_ing`，到期或权限≥4 取出→耐久分支（9-15 Random(Dura)；18-255 Random(Dura-18)，Dura=18 仍抽 Random(0)；16-17 不抽）→Dc=Mc=Sc 时 Random(3)（空转）→DC/MC/SC 按序 Random(50)+条件 Random(30)/Random(200)，n90<<((3-n90)&31) 定 n10（n90=6 恒败；SC 支用 Mc 的 bug）→写 Desc[10]→SM_ADDITEM（不 AddItemToBag，缺陷）。持久化：upgradeinfo 三表空桩，在途列表仅内存、重启即丢。帧序：立即 SM_DELITEM；排队 say(1)→52(67)→41(132)→653(152)→657×2(155)→709(164)。写集：world/merchant.rs（含 RNG，禁止 main.rs）、shop.rs 两分支、main.rs 一处臂、config.rs。判据 C1-C10 各带牙。缺口引用 #15/T-24、T-79②⑥、smframe.rs:207、Market_Def 未装载。


---

## 评审员更正（2026-10-20，**以本节为准**）

- **A1**：提交重复检查是 `upgradeInfo.UserName == user.ChrName`（C# 区分大小写）⇒ 「大小写不同」**会继续升级并再加一条记录**，
  不是 `_ing`；取回端用 OrdinalIgnoreCase 且找到不 break ⇒ 可取到后一条。C3① 期望写反了。
- **A2**：C7 的期望集合错——`Dc=0/Mc=3/Sc=6` 时 SC 支 `n90=min(11,Mc)=3 ⇒ n10=13`，成功后 `Desc[10]=30`，不会出现 31/32。
- **A3**：C8 与「Market_Def 未装载 ⇒ say 发不出」自相矛盾。拆成 C8a（无 say 的 7 帧顺序：202→52→41→653→657×2→709；657×2 见复验 F3）
  与 C8b（say 字节，随脚本装载卡）；Rust 的 `emit_shop_frames` **没有优先级队列**（按 vector 直发），
  牙应写「把 say 排在 52 之后 ⇒ 顺序红」。handler 只能调 `recalc_session_ability`，不得复用登录侧带 752 的助手。
- **A4**：C9 的城堡战门在本卡写集里不存在（`MerchantView` 只有 `castle_merchant`，World 无 under_war/城堡表）——
  要么补最小门，要么删掉该半条并登记（与 T-47 ③ 同族），不能当已实现写进判据。
- **A5**：写集漏项——`consumer_groups.tsv` 的 `merchant` 组只匹配 `Merchant.Run@...`，新增的 `UpgradeWapon@...` /
  `GetBackupgWeapon@...` 会落成未归类站点（`--require-closed` 必红），必须加行；`shop.rs` 的接缝返回值要定死
  （如 `UpgradeOutcome { frames, goto_label, upgrade_request }`）；`UpgradeWeaponMaxPoint` 本卡不用。
- **A6（Medium）**：`LableIsCanJmp` 的守卫缺失是**可达性差异**（构造客户端可直发 `@upgradenow`：oracle 丢弃、Rust 会扣钱）——
  不能只写「登记」就过，需实现 label 集或 owner 签字放行 + T-*。
- **A7/A8/A9（Minor）**：C6 需钉 `upgradeInfo.Dura` 的抽取分支；C2 公式变量名应为 `nItemCount = min(5,len)`；
  缺口引用应为 T-79①（`SM_BREAKWEAPON`）而非②，T-24 已退役只余 `GetUpgradeStdItem` 的 desc 差量（写「残余」）。
