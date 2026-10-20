# Lane marketdef — `Market_Def` 商人脚本 + `OPENYBDEAL` 设计

- 仓库根：`/e/Users/gxh/Documents/GitHub/OpenMir2`；基线声明 `master = 4a887354e`（建树时实际
  `master = b46805058`，`git worktree add /e/tmp/wt-marketdef -b market-def-design master` 取的是后者；
  本文按实际 HEAD 记录）。
- 车道性质：**tier=L，纯文档 + 只读**。本文是本车道唯一改动；未改 `src/**`、`sql/**`、`MirClient/`、
  `mir2-rs/crates/{protocol,storage}`、`E:/MirServer/**`；未起服务端/客户端/代理，未占端口，未写数据库，
  未动主检出，未 push/合并/master。未运行 cargo（无编译产物）。
- oracle（只读）：`src/Modules/ScriptEngine/{ScriptParsers.cs,ScriptEngine.cs}`、
  `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs`、`src/GameSrv/Npc/Merchant.cs`、
  `src/GameSrv/Word/WorldServer.Merchant.cs`、`src/M2Server/Npc/NormNpc.cs`、
  `src/M2Server/Player/{PlayObject.cs,PlayObject.Sale.cs,PlayObject.Base.cs,PlayObject.Message.cs,PlayObject.Operate.cs}`、
  `src/M2Server/Actor/BaseObject.Message.cs`、`src/OpenMir2/Common/StringList.cs`、`src/OpenMir2/Data/DealOffInfo.cs`、
  `src/Modules/SystemModule/Const/ScriptFlagConst.cs`、`src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.{Query,Save}.cs`。
- Rust（只读）：`mir2-rs/crates/{script,envir,world}` 的对应模块、
  `mir2-rs/bins/game-svr/src/{merchant.rs,npc.rs,shop.rs,selloff.rs,main.rs}`。
- 部署件（只读，未写）：`E:/MirServer/Mir200/Envir/{MerChant.txt,Market_Def/**,Npc_def/**}`、
  `E:/MirServer/M2GameSvr/Envir/Market_Def`、`E:/MirServer/Mir200/Envir/QuestDiary/**`。
- 口径：**行为等价优先于修 bug**。本文把「oracle 已实现」「Rust 已实现」「Rust 缺口」「部署不可达」四类分开写；
  oracle 的怪行为（无帧、错前缀、首动作即返回、GameGold 不落库）照搬，不顺手修。

---

## 0. 摘要与关键结论

1. **装载半已基本就绪，句柄半整段缺失。** oracle 的 `Merchant.LoadMerchantScript`
   （`Merchant.cs:1089-1095`）用 `ScriptParsers.LoadScriptFile(..., "Market_Def", ScriptName-MapName, true)`
   把整份脚本解析进 `Merchant.ScriptList`；Rust 的 `crates/script` 已 1:1 实现解析器，
   `crates/envir::merchant::load_merchants`（`merchant.rs:195-250`）也逐商人调了它，**但只取走
   `merchant_flags/price_rate/item_type_list/refill_goods`，把 `LoadOutcome.scripts` 丢弃**
   （`merchant.rs:226-241`）。⇒ 世界/宿主没有商人脚本表，`GotoLable` 无脚本可用。
2. **Rust 从未对商人调用 `GotoLable`。** `CM_CLICKNPC` 只打印「Click→@main 未接线」
   （`main.rs:3785`）；`CM_MERCHANTDLGSELECT` 直接 `shop::user_select`（`main.rs:4418-4470`），
   不执行说词、不填 `CanJmpScriptLableMap`；`LableIsCanJmp` 因此恒只放行 `@main`
   （`ShopSession.can_jmp_labels` 生产恒空，`main.rs:12853`）。
   `CM_USERREPAIRITEM` 的 `~@repair`/`~@s_repair` 只登记日志（`main.rs:4790-4795`）。
3. **部署件里这两条 `~@repair` 是可观测的**：131 个商人中 **32 个**脚本含 `[~@repair]`
   （`封魔谷/恶魔布衣-B345.txt:25`「修得还算不错。\ \ \ <返 回/@Main>」），`[~@s_repair]` 1 个。
   当前 Rust 修好装备后不会说这句话 ⇒ 可离线判定的真实缺口。
4. **`OPENYBDEAL` 在部署内容里是死代码。** 全 `E:/MirServer/Mir200/Envir` 扫描
   `OPENYBDEAL`/`ybdeal`/`@dealybme` 均为 **0 处**；`IsYbDeal` 脚本头永不置位。
   ⇒ 该动作只能做**构造级/夹具级**验证（或 owner 批准在克隆 Envir 里加测试脚本），
   stock 真机窗口无法靠正常玩法触发。
5. **`OPENYBDEAL` 的 oracle 语义有四个必搬的怪点**：① 不改元宝帧（没有 `GameGoldChanged()`）；
   ② `SM_MERCHANTSAY` 的文本前缀用 **玩家自己的 `ChrName`**，不是 NPC 名；③ 不检查 `IsYbDeal`；
   ④ `GotoLableQuestActionProcess` 对「已注册动作」执行后**立即 return**（`ScriptEngine.cs:890-893`），
   同一 `#ACT` 列表后面的动作不再执行。
6. **`GetSellOffGlod` 是登录路径、纯内存、且不落库。** `PlayObject.cs:1486` 在 `UserLogon` 里调用；
   取 `M2Share.SellOffItemList` 中 `sDealChrName` 匹配且 `Flag==1` 的**最后一条**，`Flag=2`、
   `GameGold += nSellGold`、`GameGoldChanged()`；只结一条。帧 = `SM_GAMEGOLDNAME`
   （`SoftVersionDateEx != 0` 才发，`Recog=GameGold`，`Param/Tag=GamePoint` 的低/高字，无尾）。
   `GameGold` 载入读的是 `characters.Gold` 同列、保存只写 `Gold`（`PlayDataStorage.Query.cs:241` /
   `Save.cs:102`）⇒ **登录领取的元宝不会被存档保留**。Rust 的纯函数已存在
   （`selloff.rs:989`）但登录路径未接线。
7. **共用前置 = 一套通用 NPC 对话宿主**（脚本表 + `GotoLable` + 说词变量展开 + `CanJmp` 表 +
   `ScriptCurrLable/ScriptGoBackLable`），与 `guildofficial`（`Npc_def` 固定 NPC）和
   `castleofficial` 争同一写集。本设计建议由一条线先落地该宿主，商人/行会/城堡只接各自 `UserSelect`。
8. **判级**：本文档 **L**。实现拆卡：数据保留/纯解析 **L**；宿主对话链 **R**（帧出队序）；
   `OPENYBDEAL` + 登录领取 **R**（RNG 站点、登录时序、GameGold 状态）。理由见 §8。

---

## 1. 商人脚本装载链路（逐行）

### 1.1 文件发现

| 步骤 | oracle 锚 | 语义 |
| --- | --- | --- |
| 启动装载 | `GameApp.cs:227` `GameShare.LocalDb.LoadMerchant()` | 在 `LoadMapDoor` 之后、`LoadNpcs` 之前 |
| 解析 `MerChant.txt` | `LocalDB.cs:412-473` | 盘上文件名是 `MerChant.txt`（大写 C）；oracle 字面量是 `"Merchant.txt"`（Windows 大小写不敏感） |
| 挂图成功后装载脚本 | `WorldServer.Merchant.cs:8-31`：`Envir != null` ⇒ `Initialize()` ⇒ `LoadMerchantScript()` + `LoadNpcData()` | 地图不存在 ⇒ `Error + RemoveAt`，脚本**不装** |
| 脚本路径 | `Merchant.cs:1090-1094`：`Path = "Market_Def"`；`scriptPath = ScriptName + '-' + MapName`；`LoadScriptFile(this, "Market_Def", scriptPath, true)` | 固定子目录 + 脚本名；`boFlag=true` |
| `GetEnvirFilePath` | `M2Share.cs:1979-1986` | `Path.Combine(BasePath, EnvirDir, dirPath, filePath)`；`filePath` 以 `..` 开头时走 `BasePath/EnvirDir/filePath[3..]`（**丢 3 个字符**，不是按路径段解析） |
| 跨平台 | `ScriptParsers.cs:577-584` `GetScriptCrossPath` | Windows 保持 `\`；非 Windows 把 `\` 换成 `/` |
| 文件存在门 | `ScriptParsers.cs:598-599` + `:978` | 不存在 ⇒ `LogService.Error("Script file not found: " + path)`，不产脚本、不写任何头字段 |

部署事实：`ScriptName` 可含 `/` 子目录（如 `封魔谷/恶魔修理师`），路径
`Envir/Market_Def/封魔谷/恶魔修理师-d2008.txt` 直接成立。131 条商人记录的脚本全部存在（0 缺失）。

### 1.2 编码与逐行读取

`StringList.LoadFromFile`（`StringList.cs:276-291`）：

1. `File.Exists` 假 ⇒ 直接 return（空表）；
2. `GetEncoding(fileName, Encoding.GetEncoding("gb2312"))`（`:293-300` / `:302-357`）：
   - `FE FF` ⇒ `BigEndianUnicode`；
   - `FF FE` 且第三字节 `!= FF` ⇒ `Unicode`（UTF-16LE）；
   - `EF BB BF` ⇒ `UTF8`；
   - 否则默认 **GB2312（cp936）**；
3. `new StreamReader(fileName, encoding)` + `while (Peek() >= 0) AppendText(ReadLine())`：
   - 按行读取，CRLF/LF 都被 `ReadLine` 去掉；
   - BOM 由 `StreamReader` 自身吞掉；
   - 不做尾随空行补齐、不做 trim（trim 在主循环）。

Rust 等价物：`crates/envir/src/text.rs::decode_text` → `mir2_script::cp936::decode_string_list`
（BOM 模型与 `StringList` 同源，见该文件模块头 B-105/E-4）。脚本文本在内存里是 Unicode 字符串；
只有**说词下发**时才转回 GBK 并做 `EDCode.EncodeString`（`PlayObject.Message.cs:1724`）。

### 1.3 `#CALL` / `#DEFINE` / `#SETHOME` / `#INCLUDE`

| 阶段 | oracle 锚 | 行为 |
| --- | --- | --- |
| `#CALL` 预处理 | `ScriptParsers.cs:605-608` 调 `:141-188` | 统计 `#CALL` 次数；逐行 `#CALL [path] label`；`path` 在 `Envir/QuestDiary/` 下；把被调文件 `[label]` 到 `}` 之间的行**插入**当前位置；`{` 行跳过；`}` 行结束；`CallScriptDict` 防重复 |
| `#CALL` 路径 | `:112-139` `GetCallScriptPath`（内容读写 `:74-110`） | Windows：去掉开头 1 或 2 个 `\`；非 Windows：额外把 `\` 换 `/` |
| 定义装载 | `:189-233` `LoadScriptDefineInfo` | `#SETHOME <label>` 设默认主页；`#DEFINE <Name> <Text>` 收进定义表（Name 转大写）；`#INCLUDE <file>` 递归读 `Envir/Defines/<file>`；处理过的行清空 |
| 默认主页 | `:611-617` | `defline` 空 ⇒ `@main`；再以 `@HOME` 名字加进定义表 |
| 常量替换 | `:619-666` | 只在 `boDefine` 为真时替换：`[` 行把 `boDefine=false`；`#IF`/`#ACT`/`#ELSEACT` 行置 `true`；每行每个定义最多替换 10 次，且 `IndexOf` 命中位置 `<=0` 不替换（**行首不替换**） |

部署事实：131 个商人脚本中 **5 个**使用 `#CALL`（`元宝充值-0/-3`、`赌博NPC/赌场-B013`、
`LEGM2.CC假人NPC/假人上线NPC-jiaren`、`...假人开始点我-jiaren`）；`#INCLUDE`/`#SETHOME`/`#DEFINE` 均 0。
`元宝充值-0.txt:10` 的 `#CALL [\冲值元宝\元宝.txt] @领取元宝` 会把 `QuestDiary/冲值元宝/元宝.txt`
的 `[@领取元宝]` 段插进脚本；被调文件里又有 `checkaccountlist ..\QuestDiary\...` 等相对路径。

两个 `#CALL` 怪点（照搬）：① `CallScriptDict` 的 `ContainsKey(sLabName)` 与 `Add(sFileName, …)` 键不一致（`ScriptParsers.cs:171-181`）；
② 目标文件/标签缺失时 `LoadScriptCallScript` 返回 false、`callCount` 不递减，外层 `while (!success)`
会**无限循环**（部署件所有 `#CALL` 目标均存在，故生产不可达；测试夹具不要制造缺失目标）。

### 1.4 主解析状态机（`ScriptParsers.cs:674-976`）

逐行 `Trim()`；空行、`;` 开头、`/` 开头直接跳过（`:676-680`）。

`scriptType` 初值 0。只有 **`scriptType == 0 && boFlag`** 时才认脚本头（`:681-781`）：

| 行首 | 处理 | 落点 |
| --- | --- | --- |
| `%` | 去掉 `%`，`StrToInt(...,-1)`；`>=55` 才写 | `Merchant.PriceRate`（`:683-692`） |
| `+` | 去掉 `+`，`StrToInt(...,-1)`；`>=0` 追加 | `Merchant.ItemTypeList`（`:693-702`） |
| `(` | `ArrestStringEx(line,"(",")")` 取括号内；按 `HUtil32.Separator` 逐个命令 | `IsBuy/IsSell/IsMakeDrug/IsPrices/IsStorage/IsGetback/IsUpgradenow/IsGetBackupgnow/IsRepair/IsSupRepair/IsSendMsg/IsUseItemName/IsOffLineMsg/IsYbDeal`（`:703-780`） |

命令串映射（大小写不敏感，`:713-779`）：`@buy`/`@sell`/`@makedrug`/`@prices`/`@storage`/`@getback`/
`@upgradenow`/`@getbackupgnow`/`@repair`/`@s_repair`/`@@sendmsg`/`@@useitemname`/`@@offlinemsg`/`@ybdeal`。

随后：

| 行形态 | 处理 | 锚 |
| --- | --- | --- |
| `{Quest <n>` | 新建 `ScriptInfo`，`QuestCount=n`，`questCount=n+1` | `:788-807` |
| `{~Quest` | `continue`（跳过） | `:808-811` |
| `scriptType==1` 分支 | **死分支**（`scriptType` 从不赋 1），无行为 | `:812-826` |
| `[goods]` | `scriptType=20`；`NPC.ProcessRefillIndex = SystemShare.CurrentMerchantIndex++`；`continue` | `:835-840` |
| `[<label>] [TRUE]` | 若当前 `Script==null` 先建；`ArrestStringEx` 取 label；其后 `GetValidStrCap` 若为 `TRUE` ⇒ `boExtJmp=true`；建 `SayingRecord` + 首个 `SayingProcedure`；**重名 label 追加 `RandomNumber.GetRandomNumber(1,200)`**；`RecordList.Add`（字典 `OrdinalIgnoreCase`） | `:827-862` |
| `#IF` | 若当前 procedure 已有条件或说词 ⇒ 新建 procedure；`scriptType=11` | `:864-877` |
| `#ACT` / `#SAY` / `#ELSEACT` / `#ELSESAY` | `scriptType=12/10/13/14`；未知 `#` 静默丢弃 | `:878-896` |
| `scriptType==10` | 追加到 `SayingProcedure.sSayMsg`（**直接拼接，无分隔符**） | `:897-902` |
| `scriptType==11` | `LoadScriptFileQuestCondition`；失败 `LogService.Error("脚本错误: "+line+" 第:"+i+" 行: "+file)` | `:903-916` |
| `scriptType==12/13` | `LoadScriptFileQuestAction` 追加到 `ActionList`/`ElseActionList`；失败同上 | `:917-940` |
| `scriptType==14` | 追加到 `sElseSayMsg` | `:941-944` |
| `scriptType==20 && boFlag` | `GetValidStrCap` 取「物品名 数量 补货分钟」；名字与补货时间非空才收；名字以 `"` 开头时 `ArrestStringEx`；`CanSellItem(name)`（实现恒 `true`，`SystemShare.cs:841-852`）⇒ `RefillGoodsList.Add` | `:946-970` |

要点：

- **`[goods]` 的 `ProcessRefillIndex` 赋值不判 `boFlag`**（`:835-840` 在 `boFlag` 分支之外）⇒ 任何 NPC 脚本出现 `[goods]` 都会推进全局 `CurrentMerchantIndex`（部署 `Market_Def` 里只有商人脚本用，故不影响）。
- **`boFlag=false` 时**：`%`/`+`/`(` 不识别，落到普通行；`[goods]` 仍把 `scriptType` 置 20，但 `:946` 的 `boFlag` 门使商品条目不进表 ⇒ 只推进全局计数。
- **标签说词行里没有分隔符**：`#SAY` 段每一行原样拼接，`\` 是文本里的换行标记（由客户端显示），不是解析分隔符。
- **标签重名改名是一次 RNG 抽**（`ScriptParsers.cs:857`）。部署件 131 个商人脚本**无重名 label**（实测 0），所以生产装载的 RNG 抽取为 **0**；Rust `rename_rand` 注入点仍必须保留（`bins/game-svr/src/merchant.rs:125-129`）。

### 1.5 与 `Npc_def` 的关系

同一个解析器和同一个 `ScriptEngine`，差异只在「入口 + 目录 + `boFlag`」：

| NPC 类别 | 来源 | 路径 | 脚本名 | `boFlag` | oracle 入口 |
| --- | --- | --- | --- | --- | --- |
| 商人 | `MerChant.txt` | `Envir/Market_Def/<ScriptName>-<MapName>.txt` | `ScriptName-MapName` | **true** | `Merchant.LoadMerchantScript`（`Merchant.cs:1089`） |
| 固定 NPC（`Npcs.txt` type 0/1/2） | `Npcs.txt` | `Envir/Npc_def/<ChrName>-<MapName>.txt`（`IsQuest=true`） | `ChrName-MapName` | false | `NormNpc.LoadNPCScript`（`NormNpc.cs:1590-1601`） |
| 隐藏任务 NPC | `MapQuest_def` | 记录里的 `file_path` | 记录里的 `script_name` | false | `MapLoader`/`NpcHost` |
| `QFunction` | 隐藏记录（`Market_Def/QFunction-0.txt`） | `Market_Def/QFunction-0.txt` | `QFunction` | **true** | `Merchant.LoadScriptFile`（`mapquest.rs:319-323`） |
| `RobotManage` | `Robot_def/RobotManage.txt` | `Robot_def` | `RobotManage` | false | `MapQuest` 记录 |

⇒ **商人脚本是 `Npc_def` 的「加强版」**：多认 `%`/`+`/`(...)`/`[goods]`，其余标签/条件/动作语法完全同源。
`Merchant.Path` 在装载前被置为 `"Market_Def"`（`Merchant.cs:1092`），供 `CHECKNAMELIST` 一类按
`normNpc.Path` 拼相对路径的条件使用；`Npc_def` NPC 的 `Path` 为 `"Npc_def"`。

### 1.6 部署件读数（只读扫描）

| 指标 | 读数 |
| --- | --- |
| `MerChant.txt` 商人记录 | 131（脚本 0 缺失） |
| 含 `[@main]` | 130（唯一例外 `功能NPC/空白-3`，文件 0 字节） |
| 含 `[goods]` | 63 个商人脚本 |
| 含 `#IF` / `#ACT` | 62 / 72 |
| 含 `#CALL` | 5 |
| 含 `[~@repair]` / `[~@s_repair]` | 32 / 1 |
| 含 `OPENYBDEAL` / `ybdeal` / `@dealybme` | **0 / 0 / 0**（全 Envir） |
| 重名 label | 0 |
| 关键脚本 sha256 | `卫家店-0103.txt` `73c8dec6…cfc9b`；`恶魔修理师-d2008.txt` `14a3fd4d…fed7210`；`恶魔布衣-B345.txt` `0b433a8f…acfaa0` |

---

## 2. `GotoLable` 说词路径与 Rust 缺口对齐

### 2.1 oracle 调用链（逐行）

**点击链**：

```text
CM_CLICKNPC(1010)（PlayObject.Message.cs:885-887）
 → ClientClickNpc(nParam1)（PlayObject.Base.cs:674-694）
    if (!IsCanDeal) → RM_MENU_OK(CanotTryDealMsg); return
    if (Death || Ghost) return
    ClickNpcTime 限频（默认 1000ms，严格 >）
    npc = FindMerchant(actorId) ?? FindNpc(actorId)
    npc.Envir == Envir && |dx|<=15 && |dy|<=15 → npc.Click(this)
 → Merchant.Click（Merchant.cs:1097-1100）→ NormNpc.Click（NormNpc.cs:68-74）
    ScriptGotoCount=0; ScriptGoBackLable=""; ScriptCurrLable="";
    GotoLable(this, "@main", false)
```

**对话选择链**：

```text
CM_MERCHANTDLGSELECT(1011)（PlayObject.Message.cs:888-890）
 → ClientMerchantDlgSelect（PlayObject.Operate.cs:114-127）
    Death/Ghost 拒；FindMerchant ?? FindNpc；同图且 IsWithinSight(<15) 或 IsHide
    → npc.UserSelect(this, sMsg.Trim())
 → Merchant.UserSelect（Merchant.cs:787-950）
    base.UserSelect（NormNpc.cs:1633-1657）
      ScriptGotoCount=0
      sData 以 @ 开头：sLabel = 首段（按 '\r'）
        if ScriptCurrLable != sLabel:
          if sLabel != "@back": ScriptGoBackLable = ScriptCurrLable; ScriptCurrLable = sLabel
          else: ScriptCurrLable 非空 ⇒ 清空；否则清空 ScriptGoBackLable
    sMsg = GetValidStr3(sData, ref sLabel, '\r')
    ScriptLable = sData（整串，含 '\r' 后的参数）
    boCanJmp = LableIsCanJmp(sLabel)
    GotoLable(player, ActorId, sLabel, !boCanJmp)
    if (!boCanJmp) return
    // 之后才是 @buy/@sell/@repair/@storage/@getback/@upgradenow/@getbackupgnow/
    // @@sendmsg/@@offlinemsg/@s_repair/@makedrug/@prices/@exit/@back/@dealybme/插件事件
```

**`GotoLable` 本体**（`ScriptEngine.cs:38-129`）：

1. `if (player.LastNpc != npc.ActorId) player.LastNpc = 0`；
2. `@main`：按 `ScriptList` **正序**找第一个含 `@main` 的脚本，命中即 `player.Script = script; LastNpc = npc.ActorId; break`；
3. 否则：若 `player.Script != null`，从 `ScriptList` **倒序**找同一个对象（`ScriptEngine.cs:64-70`，
   这个循环**也没有 `break`**，但每次赋的都是同一个对象引用，故无行为差异）；仍 null 则再从倒序扫一遍
   `CheckGotoLableQuestStatus` 通过的脚本并写回 `Script/LastNpc`——**这个循环同样没有 `break`**
   （`ScriptEngine.cs:74-82`），每次命中都覆盖 `script` ⇒ 最终保留的是**下标最小的通过者**
   （即正序第一个通过者），**不是「倒序第一个」**。商人脚本的 `CheckGotoLableQuestStatus`
   （`ScriptEngine.cs:146-173`）对 `IsQuest == false` 直接返回 true，而 `MerChant.txt` 引用的 131 个
   `Market_Def` 脚本实测**无一个含 `{Quest`** ⇒ 恒真 ⇒ 实际取 `ScriptList[0]`；
4. `RecordList.TryGetValue(sLabel)`；`boExtJmp && !record.boExtJmp` ⇒ **return**（冷标签被挡）；
5. 逐 `SayingProcedure`：条件真 ⇒ 拼 `sSayMsg` + 执行 `ActionList`；假 ⇒ 拼 `sElseSayMsg` + 执行 `ElseActionList`；
   动作返回 false（`break`）⇒ 结束；`gotoLabParams.SendSayMsg` ⇒ 立即发已拼说词（`priority`）；
6. 循环结束仍有 `sSendMsg` ⇒ 发一次普通说词（`SendMsg(normNpc,...)`）。

**说词与标签表**（`ScriptEngine.cs:987-998`、`PlayObject.Base.cs:2775-2826`）：

```text
GotoLableSendMerChantSayMsg(npc, player, sMsg, boFlag):
  sMsg = npc.GetLineVariableText(player, sMsg)   // 变量展开；最多 101 次
  player.GetScriptLabel(sMsg)                    // Clear 后按 '<...>' 重建 CanJmpScriptLableMap
  boFlag ? SendPriorityMsg(RM_MERCHANTSAY,..., npc.ChrName+"/"+sMsg, High)
         : SendMsg(npc, RM_MERCHANTSAY,..., npc.ChrName+"/"+sMsg)
```

`GetScriptLabel` 用 `ScriptRegex` = `(?<=(<))[.\s\S]*?(?=(>))` + `RegexOptions.RightToLeft`
（`M2Share.cs:270/272-275`）逐段提取；`LableIsCanJmp`（`PlayObject.Base.cs:2814-2829`）有**三支**：
① `@main` 恒真（`:2816-2819`）；② `CanJmpScriptLableMap` 命中（`:2820-2823`）；
③ `PlayDiceLabel` 相等（`:2824-2828`）——**命中后立即把 `PlayDiceLabel` 清空**，是一次性口令。
`PlayDiceLabel` 由 `ActionOfPlayDice` 从动作的 `sParam2` 写入（`ExecutionProcessingSys.cs:2030-2033`）；
部署件 `赌博NPC/赌场-B013.txt` 有 6 处 `playdice 1 @cube82/@cube91/@cube52/@cube61/@cube2/@cube1`，
而这些 `@cubeNN` 在该脚本里**只出现两次**（`playdice` 参数 + `[@cubeNN]` 标签定义），
**没有任何 `<.../@cubeNN>` 说词链接** ⇒ 第 ③ 支是它们唯一的放行通道，缺它则掷骰后客户端回跳必被拒。

**变量展开**（`NormNpc.cs:245-274` + `:275-...`；`Merchant.cs:1102-1124`）：
循环条件 `sMsg.IndexOf('>') >= 1`；`ArrestStringEx(sMsg,"<",">", ref sVariable)`；仅 `sVariable[0]=='$'`
调 `GetVariableText`；最多 101 次。Merchant 覆写在基类之后追加 `$PRICERATE`/`$UPGRADEWEAPONFEE`/`$USERWEAPON`。
部署 **131 个商人脚本**中实际出现的变量（次数；口径：对 `MerChant.txt` 引用的 131 个脚本按
`\$[A-Za-z_][A-Za-z0-9_]*` 全文计数，大小写不敏感）：`$GAMEGIRD`(23)、`$GAMEDIAMOND`(20)、
`$USERNAME`(17)、`$CREDITPOINT`(14)、`$SERVERNAME`(11)、`$LORD`(7)、`$OWNERGUILD`(7)、
`$USERWEAPON`(3)、`$GAMEGOLD`(3)、`$HIGHSCINFO/$HIGHMCINFO/$HIGHDCINFO/$HIGHLEVELINFO`(各 1)、
`$UPGRADEWEAPONFEE`(1)、`$GAMEPOINT`(1)、`$GUILDNAME`(1)、**`$STR(...)`(96，跨 7 个脚本：
真爱-G002 38、桃源-N109 35、铸造-B351 9、赌场-B013 6、打铁-0 5、客栈-0148 2、监狱-0147 1)**；
`$PRICERATE` 部署未用。（全 `Market_Def` 目录的计数会更高，这里只统计这 131 个脚本。）

`$STR(...)` 不是旁路：它同时被**说词**与**动作参数**两套展开器识别，两条路径都必搬：

- **说词路径**：`NormNpc.GetVariableText` 的 `$STR(` 支（`NormNpc.cs:1532-1587`）——先
  `M2Share.GetValNameNo(槽名)` 得槽位号，再按 `0..99 MNVal / 100..199 GlobalVal / 200..299 MDyVal /
  300..399 MNMval / 400..499 GlobaDyMval / 500..599 MNInteger / 600..699 MSString / 700..799 GlobalAVal /
  800..1199 GlobalVal[n-700] / 1200..1599 GlobalAVal[n-1100] / 1600..1699 ServerStrVal /
  1700..1799 ServerIntVal` 取值（**800..1199 的偏移是 -700**）；槽位号 `< 0` 时**不替换**（原样保留）。
- **动作参数路径**：`ProcessingBase.GetLineVariableText` / `GetVarValue` 的 `$STR(` 支
  （`ProcessingBase.cs:1347-1351`、`:1584-1622`、`:1196-1230`），槽位映射与说词侧**不同**：
  多出 `0..499 → GlobalVal[n]`、`1000..1099 GlobaDyMval`、`1100..1109 MNVal`、`1110..1119 MDyVal`、
  `1200..1299 MNMval`、`1300..1399 MNInteger`、`1400..1499 MSString`、`2000..2499 GlobalAVal` 一套；
  `S` 开头且 `GetValNameNo < 0` 时回落到 `playerActor.m_StringList`。
  同一 token 在两侧可能解析到不同槽位 ⇒ 这是必须照搬的实际行为，不要「顺手统一」。

### 2.2 Rust 现状与缺口（逐项）

| # | 链路项 | oracle | Rust 现状 | 位置 | 判定 |
| --- | --- | --- | --- | --- | --- |
| 1 | 脚本解析器 | `ScriptParsers.cs:586-980` | 已实现（含 `#CALL`/`#DEFINE`/标签/条件/动作/`[goods]`/重名改名） | `crates/script/src/parser.rs:194-530` | ✅ |
| 2 | 编码/BOM | `StringList.cs:276-357` | 已实现（`decode_text`） | `crates/envir/src/text.rs` + `crates/script/src/cp936.rs` | ✅ |
| 3 | 商人装载并保留脚本 | `Merchant.cs:1089-1095` | **丢弃 `LoadOutcome.scripts`**，只搬四个头字段 | `crates/envir/src/merchant.rs:226-241` | ❌ |
| 4 | 商人脚本表（actor → `ScriptList`） | `Merchant.ScriptList` | 无；`MerchantState` 无脚本字段 | `crates/world/src/merchant.rs:130-198` | ❌ |
| 5 | 点击 → `@main` | `NormNpc.cs:68-74` | 只到 Click 点并打日志「Click→@main 未接线」 | `bins/game-svr/src/main.rs:3785` | ❌ |
| 6 | 1011 → `base.UserSelect` 标签账本 | `NormNpc.cs:1633-1657` | 只写 `script_lable`；`script_curr_lable/go_back_lable` 只在 guildofficial 用例里用 | `main.rs:4438-4446`、`ShopSession` `main.rs:12846-12861` | ❌ |
| 7 | 1011 → `GotoLable` | `Merchant.cs:808` | 不调用；说词不产出 | `main.rs:4418-4470` | ❌ |
| 8 | `CanJmpScriptLableMap` 填充 | `GetScriptLabel` 每次说词重建 | `get_script_label` 已实现，但生产不调用 ⇒ 表恒空 | `shop.rs:1459-1479`；`main.rs:4444` | ❌（仅测试 seed） |
| 9 | `LableIsCanJmp` | `PlayObject.Base.cs:2814-2829`（三支） | **部分实现**：`@main` + `CanJmpScriptLableMap` 两支；`PlayDiceLabel` 支**未接**（`lable_is_can_jmp` 无该形参；`ScriptPlayer::set_play_dice_label` 是空钩子、`NpcPlayer` 未存值；「命中即清空」的一次性语义未建模） | `shop.rs:1448-1451`；`engine/mod.rs:528-529`；`actions.rs:803-805` | ⚠ 部分（另：生产输入 `can_jmp_labels` 恒空） |
| 10 | 说词帧 | `ScriptEngine.cs:987-998` | 引擎能产 `SayMsg`，`emit_npc_says` 能落 643；无商人调用点 | `npc.rs:246-266`、`main.rs:5035-5057` | ⚠ 半 |
| 11 | 变量展开 | `NormNpc.cs:245-274` | 钩子存在但 `NpcPlayer` 未覆写 ⇒ 原样返回 | `crates/script/src/engine/mod.rs:130-133` | ❌ |
| 12 | `Script`/`LastNpc`/`ScriptGotoCount` 跨调用保持 | `PlayObject.cs:622-626`、`ScriptEngine.cs:41-79` | `NpcPlayer` 每次调用新建；`ShopSession` 不存 `script_index/last_npc` | `npc.rs:336-405` | ❌ |
| 13 | 修理成功 `GotoLable("~@repair")` | `Merchant.cs:1652/1659` | 只回标签并打日志 | `shop.rs:1134-1140`、`main.rs:4790-4795` | ❌ |
| 14 | 升级成功标签 `~@upgradenow_ok` 等 | `Merchant.cs:571` 等 | 只回标签并打日志 | `main.rs:4633-4637/4676-4680` | ❌ |
| 15 | `@back` | `Merchant.cs:922-929` | `shop::user_select` 无 `@back` 分支（固定 NPC 的 `@back` 有测试） | `shop.rs:267+` | ❌（商人侧） |
| 16 | `OPENYBDEAL` | `ExecutionProcessingSys.cs:125/2329` | 未实现（不在 `IMPLEMENTED_EXECUTIONS`） | `crates/script/src/engine/dispatch.rs:26-100` | ❌ |
| 17 | `GetSellOffGlod` 登录 | `PlayObject.cs:1486` | 纯函数已实现，登录不调用 | `selloff.rs:989-1010`；`main.rs` 进世界块 | ❌ |

### 2.3 缺口导致的可见差异

- 点商人 / 发 1011：oracle 会先出 `SM_MERCHANTSAY(643)`（`@main` 页），Rust 只出面板帧或零帧。
- 页面里的 `<买/@buy>` 等标签：oracle 在 `@main` 说词后登记，下一次 1011 才能跳；Rust 表为空 ⇒
  除 `@main` 外全部被 `LableIsCanJmp` 丢弃（构造客户端直发也丢，与 oracle **当前**行为一致，但
  oracle 在正常点开后不丢）。
- 修好装备：oracle `SM_USERREPAIRITEM_OK` 之后还会 `GotoLable("~@repair")` 产一句
  「修得还算不错。」；Rust 无。
- 升级武器：`@upgradenow` 页的 `~@upgradenow_ok/_fail/_ing` 等同样缺失。
- `@back`：oracle 在 `Merchant.UserSelect` 里回跳 `ScriptGoBackLable`；Rust 商人路径无。
- **掷骰回跳**：`赌场-B013` 的 `playdice 1 @cubeNN` 之后客户端发 `@cubeNN`；oracle 靠
  `LableIsCanJmp` 第 ③ 支（`PlayDiceLabel` 一次性放行）通过——这些标签**不在**说词表里。
  Rust 的 `lable_is_can_jmp` 只认 `@main` + 表 ⇒ `boCanJmp = false`，随后
  `GotoLable(..., boExtJmp=true)` 又被 `[@cubeNN]` 的 `boExtJmp=false` 挡住（`ScriptEngine.cs:89-92`）
  ⇒ **点完骰子无任何反应**。补齐时务必带「命中即清空」，否则退化成永久可跳。

---

## 3. `OPENYBDEAL` 规格

### 3.1 解析与触发

- 命令码：`ExecutionCode.OpenYbDeal = 342`（`crates/script/src/codes.rs:574`；C# 注册表
  `handlers.rs:250` 同值）。
- 注册：`ExecutionProcessingSys.cs:125` `ProcessExecutionMessage[342] = ActionOfOpenSaleDeal`。
- 语法：`OPENYBDEAL <param1>`（大小写不敏感；`LoadScriptFileQuestAction` `:400-560` 用
  `GetValidStrCap` 按空格/tab 切出 `sCmd` 与 `sParam1`；`sParam1` 首字符 `"` 会去引号）。
- 触发位置：任意脚本的 `#ACT` / `#ELSEACT` 动作列表。**不要求商人、不要求 `IsYbDeal`、不要求
  `@dealybme`**——`Merchant.UserSelect` 的头标志只控制 `@dealybme` 面板，动作本身由脚本引擎直接执行。
- 动作列表语义（**必搬**）：`GotoLableQuestActionProcess`（`ScriptEngine.cs:884-894`）对
  `ExecutionProcessing.IsRegister(cmd)` 为真的动作执行一次后**立即 `return result`**；同一
  `ActionList` 的后续动作不再执行。`OPENYBDEAL` 恒被注册 ⇒ 它所在的动作列表到此为止。

### 3.2 参数取值

```csharp
int nGameGold = 0;
if (!GetValValue(playerActor, questActionInfo.sParam1, ref nGameGold))
    nGameGold = HUtil32.StrToInt(GetLineVariableText(playerActor, questActionInfo.sParam1), 0);
```

- `GetValValue`（`ProcessingBase.cs:1106-1178`）：按 `SystemShare.GetValNameNo`（`SystemShare.cs:502+`）
  解析出**槽位号**，再按区间取当前值——共 **12 段**，不止 0..599：
  `0..99`→`MNVal[n]`；`100..199`→`GlobalVal[n-100]`；`200..299`→`MDyVal[n-200]`；
  `300..399`→`MNMval[n-300]`；`400..499`→`GlobaDyMval[n-400]`；`500..599`→`MNInteger[n-500]`；
  `600..699`→`StrToInt(MSString[n-600], 0)`；`700..799`→`StrToInt(GlobalAVal[n-700], 0)`；
  `800..1199`→`GlobalVal[n-700]`（**偏移 -700 而非 -800**，与 100..499 段重叠，必搬）；
  `1200..1599`→`StrToInt(GlobalAVal[n-1100], 0)`；`1600..1699`→`StrToInt(ServerStrVal[n-1600], 0)`；
  `1700..1799`→`ServerIntVal[n-1700]`。只有落在 12 段之外才 `return false`（回落到
  `GetLineVariableText` 展开 + `StrToInt(...,0)`）。
- 未命中 ⇒ `GetLineVariableText`（`ProcessingBase.cs:1438+`）展开 `<$...>` 后再 `StrToInt(...,0)`。
  例：`OPENYBDEAL <$GAMEGOLD>` ⇒ 费用 = 当前元宝数（必然通过，扣到 0）；`OPENYBDEAL abc` ⇒ 0（免费开通）；
  `OPENYBDEAL 1000` ⇒ 1000。
- 负数/溢出：`StrToInt` 默认按 C# 语义；`GameGold >= nGameGold` 对负费用恒真，`-= 负数` 等于加钱
  （照搬，不额外校验）。

### 3.3 分支、帧与副作用

| 分支 | 条件 | 状态副作用 | 帧 |
| --- | --- | --- | --- |
| 已开通 | `playerActor.SaleDeal == true` | 无 | `SM_MERCHANTSAY(643)`，尾 `EncodeString(player.ChrName + "/您已开通寄售服务,不需要再开通!!!\\ \\<返回/@main>")` |
| 成功 | `GameGold >= nGameGold` | `GameGold -= nGameGold`；`SaleDeal = true` | 尾 `player.ChrName + "/开通寄售服务成功!!!\\ \\<返回/@main>"` |
| 不足 | `GameGold < nGameGold` | 无 | 尾 `player.ChrName + "/您身上没有" + GameGoldName + ",或" + GameGoldName + "数不够!!!\\ \\<返回/@main>"` |
| 异常 | 任何异常 | 被 `catch` 吞 | 无（`LogService.Error("{异常} TNormNpc.ActionOfOPENYBDEAL")`） |

帧细节（`PlayObject.Message.cs:1695-1724`）：

- `playerActor.SendMsg(normNpc, Messages.RM_MERCHANTSAY, 0, 0, 0, 0, text)` ⇒ `SendMessage.ActorId =
  normNpc.ActorId`（`BaseObject.Message.cs:95-115`），入 `MsgQueue` 优先级 `(byte)RM_MERCHANTSAY = 1`
  （`:180`）。
- 出队映射 `SM_MERCHANTSAY`：`MakeMessage(SM_MERCHANTSAY, processMsg.ActorId, MakeWord(nParam1,nParam2), 0, 1)`
  ⇒ `Recog = NPC ActorId`、`Param = 0`、`Tag = 0`、`Series = 1`；尾 `EDCode.EncodeString(processMsg.Msg)`
  （`PlayObject.Message.cs:1722-1724`）。线上文本是 **GBK 字节 → EDCode 6-bit 变换**。
- **前缀是 `playerActor.ChrName`，不是 `normNpc.ChrName`**——这是与所有正常说词（`npc.ChrName`）不同的怪点。
- **不发 `SM_GAMEGOLDNAME`**：`ActionOfOpenSaleDeal` 直接改 `GameGold` 字段，**没有** `GameGoldChanged()`；
  客户端在本次动作里看不到元宝变化（要等下一次 `SendGoldInfo`）。
- **不落库**：`SaleDeal` 是 `PlayObject` 的会话字段（`PlayObject.cs:856`），无保存/读取点；
  `GameGold` 的持久化见 §4.4（读完即被 `Gold` 覆盖，写只写 `Gold`）。
- `Success` 参数未被赋值 ⇒ 调用方 `result` 保持 `true` ⇒ 动作列表「继续」语义在 C# 里也因
  `return result` 而终止（同一列表后续动作不执行）；`break` 之后的 procedure 仍会继续。

### 3.4 可达性（部署内容）

- 全 `E:/MirServer/Mir200/Envir` 扫描：`OPENYBDEAL` **0** 处、`ybdeal` **0** 处、`@dealybme` **0** 处。
- 结论：**stock 内容无法触发 `OPENYBDEAL`**；`IsYbDeal` 永不置位，`@dealybme` 面板永不可达。
- 因此本动作的判据只能是：
  1. 纯引擎构造测试（直接构造 `QuestActionInfo{ nCmdCode=342, sParam1=... }` + 假玩家）；
  2. 或 owner 批准在**克隆 Envir** 里加一个临时商人脚本（不改 `E:/MirServer`、不改 `src`），
     再走真机。当前未获授权，登记为窗口前置。
- 相邻命令 `QUERYYBSELL(343)` / `QUERYBDEAL(344)` 同样未实现；部署也无使用。本设计不展开其功能，
  但要求实现 `OPENYBDEAL` 时不要把它们误注册成同一个处理器（C# 是 `ActionOfQuerySaleSell` /
  `ActionOfQueryTrustDeal`，`ExecutionProcessingSys.cs:2363/2468`）。

---

## 4. `GetSellOffGlod` 规格

### 4.1 调用点与次序

`PlayObject.UserLogon()`（`PlayObject.cs:1152` 起）内，`@Login` 说词在 `:1440`，随后是支付/冒险模式、
地图随机移动、禁登检查、配偶/师徒检查、密码锁……最后在 `:1486`：

```csharp
GetSellOffGlod(); // 检查是否有元宝寄售交易结束还没得到元宝
```

⇒ 登录帧序：先 `@Login` 脚本说词（若有），再 `GetSellOffGlod` 的 `SM_GAMEGOLDNAME`（若有）。
`M2Share.SellOffItemList` 是进程级静态列表（`M2Share.cs:79`），所有会话共享。

### 4.2 算法（`PlayObject.Sale.cs:387-429`）

```text
for (i = SellOffItemList.Count-1; i >= 0; i--):
  if (Count <= 0) break
  e = SellOffItemList[i]
  if (e != null && e.sDealChrName == ChrName [OrdinalIgnoreCase] && e.Flag == 1):
      UpdateSellOffInfo(0)     // 先清自己的 Flag==2 旧记录
      e.Flag = 2               // 交易结束
      GameGold += e.nSellGold
      GameGoldChanged()
      break                    // 只结一条
```

- **不检查 `SaleDeal`**：即使未开通服务，只要列表里有自己的 `Flag==1` 条目，也会入账。
- `UpdateSellOffInfo(0)`（`PlayObject.Sale.cs:19-56`）：仅当 `SaleDeal` 为真才执行；**倒序**删除
  所有 `Flag==2` 且 `sDealChrName == ChrName` 的条目（C# 用 `==`，**大小写敏感**，与上面
  `string.Compare(..., OrdinalIgnoreCase)` 不一致）；`Dispose` 掉。
- `break` 位置：**反序遍历到的第一条** `Flag==1` 被结；更早的条目（若有）留到下次登录。
- 反序 + 删除会导致下标平移；C# 靠对象引用避免错删，Rust 的 `get_sell_off_gold` 已用
  `removed` 下标平移（`selloff.rs:989-1010`）。

### 4.3 帧

`GameGoldChanged()`（`BaseObject.cs:593-598`）→ `SendUpdateMsg(RM_GAMEGOLDCHANGED, ...)`：

- `SendUpdateMsg` 先按 `wIdent` 去重（`BaseObject.Message.cs:283-306`，实现有 peek/出队怪点，
  单条时等价），再入队；
- 出队 `RM_GAMEGOLDCHANGED` → `SendGoldInfo(false)`（`PlayObject.Message.cs:1836-1838`）；
- `SendGoldInfo`（`PlayObject.Base.cs:927-940`）：
  - `SoftVersionDateEx == 0` ⇒ **直接 return，无帧**；
  - 否则 `SendDefMessage(SM_GAMEGOLDNAME, GameGold, LoWord(GamePoint), HiWord(GamePoint), 0, "")`
    ⇒ `Recog = GameGold`、`Param = GamePoint 低 16 位`、`Tag = GamePoint 高 16 位`、`Series = 0`、无尾。
- 这是 `SendDefMessage` **直发**（不是 `SendMsg` 入队），但发生在 `GameGoldChanged` 被出队时；
  与登录时 `SendGoldInfo(true)` 的 5008 是**两条**帧（后者带 `GameGoldName + '\r' + GamePointName` 尾）。

### 4.4 持久化/经济事实（必搬）

| 事实 | 证据 |
| --- | --- |
| `GameGold` 载入读 `characters.Gold` 同列 | `PlayDataStorage.Query.cs:239-241`：`humInfoData.Gold = dr.GetInt32("Gold"); humInfoData.GameGold = dr.GetInt32("Gold");` |
| 保存只写 `Gold`，不写 `GameGold`（DDL 也无 `GameGold` 列） | `PlayDataStorage.Save.cs:76/102`；`sql/mir2_db.sql:37` 只有 `Gold`、`:38` 是 `GamePoint` |
| ⇒ 登录领取的元宝 | 本会话内存有效；下次登录被 `Gold` 覆盖（**不保留**） |
| `goldsales` 读取列名错（**有行即抛**） | `MySqlDB.cs:264-265` 读 `DealChrName/BuyChrName`，DDL 是 `DealCharName/BuyCharName`（`sql/mir2_data.sql:26-27`）。`dr.GetString` 经 `dr.GetOrdinal`（`src/OpenMir2/Extensions/DataReaderExtension.cs:9-19`），列名不存在时 `GetOrdinal` 抛 `IndexOutOfRangeException`；`LoadSellOffItemList`（`MySqlDB.cs:258-287`）只有 `try/finally` **没有 `catch`**，调用点 `GameApp.cs:182`（`Initialize()`）也不在 `:224` 那个 `try` 里 ⇒ **不是「静默跳过」**。0 行的直接原因是表本身为空（DDL 的 `Records of goldsales` 段为空，`sql/mir2_data.sql:36-37`）。⇒ **表空 ⇒ 0 行；一旦播种 ⇒ oracle 装载器抛异常**（启动链中断） |
| `goldsales` 写入 | `SaveSellOffItemList` 的三个调用点全被注释（`PlayObject.Sale.cs:240/332/577`）⇒ 永不写 |
| 运行时列表来源 | `ClientSellOffEnd` 往 `M2Share.SellOffItemList` 里加（`PlayObject.Sale.cs:516-591`）；条目 `Flag=1` 由买方在卖方离线时购买置位（`:281-385`） |

Rust `bins/game-svr/src/main.rs:827-845` 已把 `goldsales` 只读种子加载接上；`SellOffBook` 是 sim 线程内
唯一实例（`main.rs:15677-15682`）。**注意**：Rust 的 `get_string`（`crates/data/src/loaders.rs:104-110`）
把 `ColumnNotFound` 降级成空串 ⇒ 表里有行时 **Rust 静默得 0 行、oracle 抛异常**，二者不等价；
stock 表为空故当前不可观测。`goldsales.rs` 模块头「读错列名 ⇒ 恒 0 行」的归因同样需订正为
「表空 ⇒ 0 行」。是否复刻抛异常见 §9 第 9 条。

### 4.5 Rust 现状

- `selloff::get_sell_off_gold`（`selloff.rs:989-1010`）已 1:1 实现：反序找 `flag==1`、
  `update_sell_off_info(0)`、`flag=2`、`game_gold +=`、`game_gold_changed`；
  `game_gold_changed` 由 `send_game_gold_info` 门（`selloff.rs:392-406`）控制，对应 `SoftVersionDateEx != 0`。
- **登录路径未接线**：`@Login` 块（`main.rs:17197-17285`）之后没有调用；`sell_off_sessions` 在
  进世界时也不 seed（首次访问才 `unwrap_or_default()`，`main.rs:4165`/`4468`）。
- `SaleDeal` 无脚本写点（`OPENYBDEAL` 未实现，`selloff.rs:80` 已登记）⇒ 生产恒 `false`；
  但 `get_sell_off_gold` 不受该门限制，仍可入账。
- `GameGold` 在 Rust 的权威源是 `SessionGround.game_gold`（`gamegold.rs:41-43`），
  载入自 `characters.Gold`（`bins/db-srv/src/playdata.rs:116`）；登录领取后应只写回
  `SessionGround.game_gold`/`SellOffState`，**不得**改 `SavedHuman.Gold`（oracle 不写）。

---

## 5. 设计建议（实现形态）

### 5.1 数据层：把脚本从 envir 带到宿主（不二次解析）

建议把 `MapLoader::load_merchants` 的返回改为「记录 + 对齐的脚本表」，让**同一次解析**的
`LoadOutcome.scripts` 直接进入宿主：

```rust
// crates/envir/src/merchant.rs
pub struct LoadedMerchant {
    pub record: MerchantRecord,
    pub scripts: Vec<ScriptInfo>, // 本次 load_script_file 的 LoadOutcome.scripts（移动，不 Clone）
}
pub fn load_merchants(...) -> Result<Option<Vec<LoadedMerchant>>, EnvirError>;
```

或保留 `Vec<MerchantRecord>` + 并行 `Vec<Vec<ScriptInfo>>`（下标对齐）。**不要**在宿主重新
`load_script_file`：那会二次解析，且在重名 label 场景二次抽 `rename_rand` ⇒ RNG 流水与 oracle 不等
（部署件当前 0 重名，但判据必须防回归）。

`MerchantRecord` 现有 `#[derive(Clone)]`；`ScriptInfo` 无 `Clone`（`crates/script/src/model.rs:12-19`）。
两条路线：
1. 用上面的 `LoadedMerchant`（推荐，避免改 `crates/script`）；
2. 给 `ScriptInfo` 加 `Clone`（会动共用 `crates/script` 写集，需与其它 script 车道串行）。

### 5.2 共用对话宿主（与 guildofficial/castle 的前置）

建议新增/扩展 `bins/game-svr/src/npc.rs` 的 `NpcHost` 为通用 `NpcDialogHost`：

```rust
pub struct DialogSession {
    pub script_index: Option<usize>, // PlayObject.Script
    pub last_npc: i32,               // PlayObject.LastNpc
    pub script_goto_count: i32,
    pub script_curr_lable: String,   // ScriptCurrLable
    pub script_go_back_lable: String,
    pub script_lable: String,        // Merchant.UserSelect 写的 ScriptLable（含 '\r' 参数）
    pub can_jmp_labels: Vec<String>, // CanJmpScriptLableMap
}
pub struct NpcDialogHost { /* 每个 actor_id 的 ScriptList + chr_name/map/x/y/path */ }
impl NpcDialogHost {
    pub fn goto_label(&self, actor_id: i32, st: &mut DialogSession, p: &mut NpcPlayer, label: &str, bo_ext_jmp: bool) -> Vec<SayMsg>;
}
```

要求：
- `ScriptNpc::actor_id()` 返回**真实世界 ActorId**（商人是 `World` 实体 id；隐藏 NPC 0）；
  `map_name/curr_x/curr_y/path` 用实体的值（脚本错误日志、按 `Path` 拼文件的条件下需要）。
- 每次 `goto_label` 前由调用方做 `ScriptGotoCount=0`（`NormNpc.Click` / `NormNpc.UserSelect` 的口径），
  或由 `DialogSession` 在入口统一清。
- 说词展开：在 `send_say` 之前实现 `NormNpc.GetLineVariableText` 的循环（`'>'` 位置门、`$` 门、101 次上限、
  `ArrestStringEx` 语义），再调 `GetScriptLabel` 重建 `can_jmp_labels`（**每次 Clear**）。
- 变量表：基类 `$USERNAME/$GAMEGOLD/$GAMEGIRD/$GAMEDIAMOND/$CREDITPOINT/$SERVERNAME/$OWNERGUILD/$LORD/
  $CURRENTX/$CURRENTY/$CURRENTMAP/...` + 商人覆写 `$PRICERATE/$UPGRADEWEAPONFEE/$USERWEAPON`。
  实现形态二选一：① 在 `ScriptPlayer::line_variable_text` 里做（玩家侧变量）；② 给 `ScriptNpc` 加
  `variable_text(token) -> Option<String>`（NPC 侧变量，如 `$PRICERATE`），引擎在玩家侧展开后调用。
  建议 ②，避免把商人状态塞进玩家适配器。
- 帧：说词统一走 `emit_npc_says` / `sm_merchant_say`；普通 `SendMsg(normNpc)` 的 `Recog` = NPC ActorId，
  `SendPriorityMsg` 的 `Recog` = 玩家 ActorId（现有 `emit_npc_says` 已按此实现，`main.rs:5035-5057`）。
- **出队序必须统一排序**：oracle 把说词和面板帧都塞进同一个 `MsgQueue`，按 `(byte)RM_*` 升序出队
  （`BaseObject.Message.cs:180/334`）。Rust 现在把 `shop::user_select` 的 `Vec<ShopFrame>` 与说词分开
  发送 ⇒ 必须合并成一个 `(priority, seq)` 列表后统一发出，否则「先 say 后面板/先面板后 say」会错。
  优先级就是 **`(byte)wIdent`**（`BaseObject.Message.cs:180` `MsgQueue.Enqueue(sendMessage, (byte)wIdent)`，
  即 `wIdent & 0xFF`；常量见 `src/OpenMir2/Messages.cs`）。逐个复核（按低 8 位）：
  `RM_MERCHANTSAY=11009(0x2B01) ⇒ 1`；`RM_SENDREPAIRCOST=10142(0x279E) ⇒ 158`；
  `RM_USERREPAIRITEM_OK=10143(0x279F) ⇒ 159`；`RM_MERCHANTDLGCLOSE=10127(0x278F) ⇒ 143`；
  `RM_SENDGOODSLIST=10128(**0x2790**) ⇒ **144**`（旧稿写 208，已订正）；`RM_MENU_OK=10309(0x2845) ⇒ 69`。
  ⇒ 同批次出队序（小者先）：say(1) < menu_ok(69) < dlgclose(143) < goodslist(144) <
  sendrepaircost(158) < userrepairitem_ok(159)。
  （`SendPriorityMsg` 的 `MessagePriority.High` 走另一条入口 `Enqueue(..., (byte)Priority)`
  （`BaseObject.Message.cs:30`），与 `(byte)wIdent` 不是同一套数值，比较时不要混用。）

### 5.3 商人接线

1. **装载**（`bins/game-svr/src/merchant.rs`）：
   - `load_merchant_spawns` 同时返回脚本表；
   - `spawn_merchants` 需要把 `World::insert_merchant` 返回的 id 与文件序对齐返回
     （当前只返回 `(kept, failed)`，`merchant.rs:168-194`）；
   - 建议把 `MerchantScriptBook { by_actor: BTreeMap<i32, Vec<ScriptInfo>>, by_name: ... }` 存到
     `NpcDialogHost` 或新的 `MerchantScriptHost`。
2. **`CM_CLICKNPC`**（`main.rs:3708-3795`）：在 `FindMerchant` 命中且同图/`<=15` 后，按
   `NormNpc.Click` 重置 `ScriptGotoCount/ScriptGoBackLable/ScriptCurrLable`，调
   `goto_label(actor_id, "@main", false)`，发说词。
3. **`CM_MERCHANTDLGSELECT`**（`main.rs:4418+`）：
   - 先 `base.UserSelect` 的账本（`ScriptGotoCount=0`；`@back`/`curr/go_back` 更新）；
   - `script_lable = sData`（整串）；
   - `sLabel = sData` 按 `\r` 首段；`boCanJmp = lable_is_can_jmp`；
   - `goto_label(actor_id, sLabel, !boCanJmp)` 并**先收集说词帧**；
   - `if !boCanJmp { 只发说词（若有）; return; }`（oracle 在 `Merchant.cs:809-812` 直接 return）；
   - 再 `shop::user_select`，把它产生的帧与说词帧按优先级合并发出；
   - `@back`：`ScriptGoBackLable` 空则 `@main`，否则用其值再 `goto_label`（`Merchant.cs:922-929`）；
   - `@exit`/`@@sendmsg`/`@@offlinemsg`/`@s_repair`/`@buy`/... 仍按 `Merchant.cs:811-937` 的顺序。
4. **修理/升级**：`shop::repair_item` / `upgrade_wapon_outcome` 已回 `goto_label`；宿主在对应
   `SM_USERREPAIRITEM_OK` / `SM_...OK` 之后调 `goto_label("~@repair"/"~@s_repair"/...)`，
   并把说词帧插入统一队列（不要只打日志）。
5. **`@dealybme`**：保留现有 `selloff::open_deal_off_form` 支（`main.rs:4465-4500`），
   但 `goto_label` 仍需先执行（oracle 先 `GotoLable` 再 `switch`）。
6. **`[goods]`**：已由 ⑤-4 接 `refill_goods`，本设计不改。

### 5.4 `OPENYBDEAL` 引擎与宿主

- `ScriptPlayer` 增：
  ```rust
  fn sale_deal(&self) -> bool { false }
  fn set_sale_deal(&mut self, _v: bool) {}
  ```
  （`game_gold/set_game_gold` 已有，`crates/script/src/engine/mod.rs:159-160/386-394`。）
- `actions.rs` 增 `"ActionOfOpenSaleDeal"` 分支，逐行照 §3.3：
  - 已开通 ⇒ `SendMsg(RM_MERCHANTSAY, 0,0,0,0, Some(player.chr_name()+"/..."))`；
  - 取参 `get_val_value`/`line_variable_text` + `str_to_int(...,0)`；
  - 成功 ⇒ `set_game_gold(game_gold - n)`、`set_sale_deal(true)`、发成功文本；
  - 不足 ⇒ 发不足文本；
  - **不调用 `game_gold_changed()`**；**不设 Success**（Rust 的 `run_actions` 已按「已注册动作执行后
    return」的 C# 语义实现，见 `dispatch.rs` 与 `actions.rs` 的注释；实现时复核）。
- 加入 `IMPLEMENTED_EXECUTIONS`（`dispatch.rs:26`），使 `unimplemented_handlers()` 少一项。
- 宿主：`NpcPlayer` 增 `sale_deal` 字段，`ScriptPlayer` 覆写读写；会话侧写回
  `SellOffState.sale_deal`、`SessionGround.game_gold`。**不要**发额外帧。
- 由于 stock 内容 0 使用，生产路径不可达；判据以构造级为主（§7 C10-C12）。

### 5.5 登录 `GetSellOffGlod`

在进世界块（`main.rs:17197-17285`）的 `@Login` 说词之后、回写 `SavedHuman` 之前：

```rust
let mut me_so = sell_off_sessions.remove(chr_name).unwrap_or_default();
let mut side = SellOffSide { /* id, name, game_gold: &mut sg.game_gold, game_point,
                                send_game_gold_info: sg.soft_version_date != 0,
                                state: &mut me_so, ... */ };
let events = game_svr::selloff::get_sell_off_gold(&mut side, &mut sell_off_book);
sell_off_sessions.insert(chr_name.to_string(), me_so);
for ev in events { /* 与 selloff_event_body 同路：即时/排队帧发到该 socket */ }
```

- 必须用**同一个** `sell_off_book`（进程级列表）；
- `send_game_gold_info` = `SoftVersionDateEx != 0`（`SessionGround.soft_version_date`）；
- 只改 `sg.game_gold`，不改 `SavedHuman.record.data.game_gold`（Rust 侧若从 `Gold` 载入，
  保存路径也不应写回 GameGold；见 §4.4）；
- 帧目标 = 本人 actor id（`SellOffSide.id`）。

---

## 6. 写集切分（与 guildofficial/castle 的共用前置）

| 写集 | 当前状态 | 本设计的用法 |
| --- | --- | --- |
| `mir2-rs/crates/envir/src/merchant.rs`（+ `tests/merchant.rs`） | 本车道理应独占 | **MD-A**：`LoadedMerchant`/脚本表；L |
| `mir2-rs/crates/script/src/{engine/*,codes.rs}` | 与 quest/guild script 车道共用 | **MD-B**：`SaleDeal` trait + `ActionOfOpenSaleDeal`；只在 MD-A 之后，且需与 script 车道串行；R |
| `mir2-rs/bins/game-svr/src/merchant.rs` | 商人线 | **MD-C**：id↔脚本表；L |
| `mir2-rs/bins/game-svr/src/{npc.rs,shop.rs,main.rs}` | **与 guildofficial/castleofficial 争同一写集**；当前并发写集已把 `bins/game-svr/**` 列为禁止 | **MD-D**：通用 `NpcDialogHost` + 商人接线；必须等这些车道让出；R |
| `mir2-rs/bins/game-svr/src/selloff.rs` + `main.rs`（登录块） | ⑩ 元宝寄售线 | **MD-E**：`GetSellOffGlod` 登录接线；R |
| `mir2-rs/tests/parity/m3/**` | 本文档 | 设计/判据；L |
| `src/**`、`sql/**`、`MirClient/`、`E:/MirServer/**`、`crates/{protocol,storage}` | 冻结 | **零改动** |

**共用前置卡建议**：`openmir2-m3-npc-dialog-host`（L/R）——只做「脚本表 + `GotoLable` + 变量展开 +
`CanJmp` + 会话标签账本 + 说词帧」，商人/行会/城堡三条线各自接 `UserSelect`。所有者必须与
`guildofficial`/`castleofficial` 不同线或显式串行；否则 `npc.rs/main.rs` 必然冲突。

**串行顺序建议**：MD-A（envir）→ MD-B（script）→ MD-D（宿主对话）→ MD-E（登录领取）；
MD-C 可与 MD-D 同卡但必须先返回 id 表。`[goods]`/`%`/`+` 已存在，不在本卡重复改。

---

## 7. 判据清单（带牙草案）

> 牙的定义：**改坏生产/纯函数路径 ⇒ 指定具名断言必红**；实现时先制造红、再逐字节还原，
> 并用 `sha256sum -c` 钉住还原后的文件。文档车道未运行 cargo。

| ID | 名称 | 断言要点 | 改坏必红 | tier |
| --- | --- | --- | --- | --- |
| C1 | `market_def_scripts_are_retained` | 部署/夹具 131 条商人：每条 `scripts` 非空（除 0 字节 `空白-3`）；`Labels` 集合与 oracle 解析结果一致（`@main`/`@buy`/`@repair`/`[goods]` 等） | `load_merchants` 仍丢弃 `scripts`、或只保留头字段 ⇒ 红 | L |
| C2 | `market_def_no_second_parse_no_extra_rng` | 装载 131 条商人后 `rename_rand` 调用数 = 0；宿主不再调 `load_script_file`；注入计数器后断言 = 1 次/文件 | 宿主二次解析 / 删掉 `rename_rand` 注入 ⇒ 红 | R（RNG 站点） |
| C3 | `merchant_click_main_say` | `卫家店-0103`/`恶魔修理师-d2008` 的 `@main` 说词文本逐字（`$USERNAME` 等按固定玩家展开）；`SM_MERCHANTSAY` 的 `Recog` = NPC ActorId、`Series=1`、尾 = `ChrName + '/' + 文本` 的 GBK+EDCode | 不调 `GotoLable`、把 `Recog` 用玩家 id、漏 `ChrName + '/'` ⇒ 红 | L（离线帧编码） |
| C4 | `merchant_dialog_label_gate` | 首次 1011 `@main` 后 `can_jmp_labels` 含页面里的全部 `<.../@label>`；冷发 `@upgradenow` 被拒（0 帧）；`@buy` 在说词后放行；`@exit`/`@back` 行为 | `GetScriptLabel` 不 Clear / 不填表 / 把 `@main` 特判删掉 ⇒ 红 | L |
| C5 | `merchant_base_userselect_back_state` | `ScriptCurrLable/ScriptGoBackLable`：`@a → @b` 后 go_back=@a；`@back` 清 curr；再 `@back` 清 go_back；`@main` 特判；`ScriptGotoCount` 每次 0 | 不调 base / 把两个字段合并 / 不重置 goto ⇒ 红 | L |
| C6 | `merchant_repair_ok_label` | 修好普通装备：`SM_USERREPAIRITEM_OK` 之后有 `SM_MERCHANTSAY`「修得还算不错。\ \ \ <返 回/@Main>」（`恶魔布衣-B345`）；特殊修理 1 个脚本；修失败/不支持无 say | 删 `goto_label` 调用 / 把 `~@repair` 写成 `@repair` ⇒ 红 | L（帧序另见 C13） |
| C7 | `merchant_variable_text_vectors` | 部署商人页出现的 17 种 `$...` 变量逐个展开（含 `$STR(...)` 的 12 段槽位映射，且说词侧与动作参数侧映射不同）；`$PRICERATE/$UPGRADEWEAPONFEE/$USERWEAPON` 商人数值；未知 token 原样；101 次上限；`>` 位置门 | 改文案/大小写敏感/去 101 上限/未知 token 置空 ⇒ 红 | L |
| C8 | `market_def_call_define_vectors` | `#CALL [\冲值元宝\元宝.txt] @领取元宝` 插入后的标签/动作序列与 oracle 一致；`#DEFINE` 行首不替换、最多 10 次；`#SETHOME` 默认主页 | 漏插入/把行首也替换/去 10 次上限 ⇒ 红 | L |
| C9 | `market_def_encoding_bom_vectors` | GB2312 无 BOM、UTF-8 BOM、UTF-16LE BOM、`FF FE 00 00`（UTF-16LE）逐字节；说词尾 GBK+EDCode | 选错编码/把 `FF FE 00 00` 当 UTF-32 ⇒ 红 | L |
| C10 | `openybdeal_branches` | 已开通 ⇒ 已开通文案、元宝不变；成功 ⇒ 扣费/`sale_deal=true`/成功文案；不足 ⇒ 文案、元宝不变；异常 ⇒ 无帧 | 任一文案/扣费方向/分支顺序错 ⇒ 红 | L |
| C11 | `openybdeal_quirks` | 不发 `SM_GAMEGOLDNAME`；尾前缀是玩家 `ChrName`；`Recog`=NPC；不要求 `IsYbDeal`；参数 `M6`/`<$GAMEGOLD>`/`abc`/`1000` 的取值 | 加 `game_gold_changed`、改用 NPC 名前缀、加 `IsYbDeal` 门 ⇒ 红 | L |
| C12 | `openybdeal_first_registered_action_only` | `#ACT OPENYBDEAL 1000\n goto @x`：只执行 OPENYBDEAL，`@x` 不执行；`#ACT` 后续 `break` 同理 | 让动作列表继续执行 ⇒ 红 | L |
| C13 | `selloff_login_credit` | `Flag==1` 卖方：`flag→2`、`game_gold += n`、`GetSellOffGlod` 恰追加一条 `SM_GAMEGOLDNAME`（区别 `RM_LOGON` 的 `SendGoldInfo(true)` 那条）（`Recog=GameGold`、`Param/Tag=GamePoint` 低/高）；两条 `Flag==1` 只结反序第一条；`UpdateSellOffInfo` 清 `Flag==2` 且大小写敏感；无匹配 ⇒ 0 帧 | 结多条 / 用 `OrdinalIgnoreCase` 做删除 / 改帧字段 ⇒ 红 | R（登录时序 + 经济状态） |
| C14 | `selloff_login_frame_gate` | `SoftVersionDateEx==0` ⇒ 元宝仍加、**0 帧**；`!=0` ⇒ 一条 5008；`GameGold` 回写只进 `SessionGround`，`SavedHuman.Gold` 不变 | 去掉门 / 写 `SavedHuman.Gold` / 多发帧 ⇒ 红 | R |
| C15 | `market_def_deployment_readout` | 只读部署件：131 商人、130 `@main`、63 `[goods]`、32 `~@repair`、0 `OPENYBDEAL`；关键脚本 sha256 钉死 | 夹具/扫描口径改动导致读数漂移 ⇒ 红 | L |

---

## 8. tier 与窗口项

### 8.1 判级

- 本文档：**L**（纯文档 + 只读；无代码、无 cargo、无端口）。
- **MD-A / MD-C**（envir 保留脚本、id 对齐）：**L**——纯数据搬运，不改变任何生产帧/数值；
  唯一 RNG 站点是重名改名，部署件 0 触发。
- **MD-D**（通用对话宿主 + 商人接线）：**R**。`tier_why`：
  1. 说词与面板帧共用 `MsgQueue` 的**出队序**（时序）；
  2. `ScriptGotoCount` 的 101 上限与 `goto` 递归（控制流时序）；
  3. 说词变量展开含 `$GAMEGOLD/$GAMEGIRD` 等**经济状态**读取（虽只读）。
  可拆 L 子卡：C3/C4/C5/C7/C8/C9/C10/C11/C12 的纯函数部分。
- **MD-E**（`GetSellOffGlod` 登录）：**R**。`tier_why`：登录时序 + `GameGold`/`GamePoint` 数值 +
  进程级共享列表 + 帧门（`SoftVersionDateEx`）；且 `SellOffBook` 的条目由其它会话写入（竞态面）。
- 按仓库政策：R 卡 `merge_result` 后保持 `in-progress`，窗口项未过不得 `closed`。

### 8.2 窗口项（建议登记到 runsheet 车道）

| ID | 内容 | 需要什么 | 断言 |
| --- | --- | --- | --- |
| W-MD-1 | BotSrv 发 `CM_CLICKNPC` 点 `卫家店`/`恶魔修理师`（同图、`<=15`；`ClientClickNpc` 门） | 已有 BotSrv 点击/1011 原语；oracle `FindNpc` 不需要（商人走 `FindMerchant`） | 收到 `SM_MERCHANTSAY(643)`，尾 `NPC名/页面文本`（变量按玩家展开，`$DATETIME` 类 mask）；`Recog`=NPC ActorId |
| W-MD-2 | 点开后发 1011 `@main`，再发 `@repair`/`@buy` 等 | 同上 | 说词逐字节；`CanJmp` 表允许已说过的标签；冷标签 0 帧 |
| W-MD-3 | 修理成功（普通 + 特殊各一） | 背包有可修装备；商人脚本 `~@repair`/`~@s_repair` | 帧序：`SM_MERCHANTSAY`（优先级 1）先于 `SM_USERREPAIRITEM_OK`（优先级 159）；文本「修得还算不错。」；金币/耐久按公式 |
| W-MD-4 | `OPENYBDEAL` | **stock 内容不可达**（0 处）。需要 owner 批准：克隆 Envir + 临时商人脚本加 `OPENYBDEAL 1000`，或只在构造级 | 见 C10-C12；若批准真机：`GameGold` 扣减、`SaleDeal` 置位、无 5008、前缀玩家名 |
| W-MD-5 | `GetSellOffGlod` 登录领取 | 两个 Bot/SQL pin：卖方上架（`CM_SELLOFFADDITEM`+`CM_SELLOFFEND`）→ 卖方离线 → 买方 `CM_SELLOFFBUY` 置 `Flag=1` → 卖方重新登录 | 登录后 `RM_LOGON` 的 5008 之外**再追加**一条 `SM_GAMEGOLDNAME`（`Recog=GameGold`、`Param/Tag=GamePoint` 低/高）；内存 `GameGold` +n；`characters.Gold` **不变**（不落库）；若 `SoftVersionDateEx=0` 则 0 帧但内存 +n |
| W-MD-6 | `@back`/`@exit`/页面跳转 | 1011 序列 | `@back` 回上一页；`@exit` 一条 `SM_MERCHANTDLGCLOSE`；`ScriptGoBackLable` 链不丢 |

> 窗口注意：`$DATETIME`、`$CURRENTX/Y` 等是墙钟/位置相关；比较需固定玩家位姿或用 mask。
> `OPENYBDEAL` 的窗口项在 owner 未批准合成脚本前**不得**写进必过项（登记为 blocked）。

---

## 9. 未决问题（需要 owner/reviewer 裁决）

1. **脚本表形态**：`MerchantRecord.scripts`（需 `ScriptInfo: Clone`，动共用 `crates/script`）还是
   新 `LoadedMerchant`/并行表（不动 script）？本文推荐后者。
2. **共用宿主归属**：`openmir2-m3-npc-dialog-host` 由商人线、行会线还是独立线先落地？
   `bins/game-svr/**` 当前是并发禁区，必须指定唯一 owner 与让出时点。
3. **`OPENYBDEAL` 的实现级别**：stock 内容不可达，是否仍按 R 实现并只为构造级判据？
   本文建议实现（行为等价 + 防止未来内容回归），但窗口项 blocked。
4. **`GetSellOffGlod` 的 GameGold 落库口径**：Rust 当前 `bins/db-srv` 已按「GameGold=Gold 同列」
   载入、保存只写 Gold；登录领取后是否需要在 `SavedHuman` 里保留内存增量直到下次保存？
   oracle 不保留；本文建议严格照 oracle（不写回）。
5. **`GetScriptLabel` 的 RightToLeft 正则**：现有 Rust `shop::get_script_label` 是左到右扫描，
   与 `(?<=<)...(?=>)` + `RightToLeft` 在嵌套/多尖括号串上可能不同。是否需要按 .NET 正则语义
   重写，还是以部署页面向量钉住即可？本文建议先以部署 131 页 + 人工构造的嵌套向量钉住，若出现
   差异再单独卡。
6. **`@back` 商人侧**：`Merchant.UserSelect` 的 `@back` 分支当前 Rust 无；是否并入通用宿主卡？
   本文建议并入（与 guild/castle 共用）。
7. **`[goods]` 全局 `CurrentMerchantIndex`**：parser 目前只统计 `goods_sections`，未暴露自增序号；
   是否需要在 `LoadedMerchant` 里带 `process_refill_index`（影响 `Merchant.Run` 的补货延时）？
   本文建议顺带带上，避免下一张补货卡再改 envir。
8. **`QUERYYBSELL/QUERYBDEAL`**：是否与 `OPENYBDEAL` 同卡实现？部署 0 使用；本文建议只登记，
   不扩大本卡写集。
9. **`goldsales` 列名 bug 的异常语义**：oracle 在表有行时抛 `IndexOutOfRangeException`（无 catch），
   Rust 当前降级成 0 行。是否按「行为等价优先」复刻抛异常（会让播种过的库直接启动失败），
   还是登记 `T-*` 白名单保留静默？本文建议复刻并配构造级判据（`D-5/B-5` 的措辞需改为「异常复刻」）。

---

## 10. 边界与残留物

- 本车道只新增本文档；**未运行 cargo**，无 `CARGO_TARGET_DIR` 产物；无临时目录/进程/端口。
- 只读访问了 `E:/MirServer/Mir200/Envir`（Market_Def/Npc_def/QuestDiary/MerChant.txt）与
  `E:/MirServer/M2GameSvr/Envir/Market_Def`；未写任何部署文件。
- 关键复现命令（只读，供复核）：
  ```sh
  grep -rail "openybdeal" /e/MirServer/Mir200/Envir | wc -l     # 0
  grep -rail "ybdeal"     /e/MirServer/Mir200/Envir | wc -l     # 0
  # 131 商人 / 130 @main / 63 [goods] / 32 [~@repair]（Python 逐文件扫描，见 §1.6）
  sha256sum "Market_Def/比奇城/卫家店-0103.txt" \
            "Market_Def/封魔谷/恶魔修理师-d2008.txt" \
            "Market_Def/封魔谷/恶魔布衣-B345.txt"
  ```
- 与既有报告的衔接：`M3⑤商人MerChant加载报告-20261013.md`（装载/头字段）、
  `M3⑤NPC商店现状盘点报告-20261013.md`（买/卖/修/仓派发）、
  `M3⑩元宝寄售动作序列夹具骨架报告-20261013.md`（元宝寄售夹具）、
  `design-castle-official-20261022.md`（共用 `Npc_def` 对话宿主/`CanJmp`/变量文本的同类缺口）。
- 本文不把「Rust 当前缺件」写成新发现的 oracle 行为；所有 oracle 行号均在当前 `src/` 实读。
