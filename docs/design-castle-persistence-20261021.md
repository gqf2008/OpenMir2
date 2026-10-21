# A-22 城堡持久化实现设计（2026-10-21）

> 卡片：`openmir2-castle-persist-drain`（建议名）＝ 盘点表 A-22 / 计划表 R-03 + R-13 / 既有登记 T-127。
> 基线：`master 8b8cb10d7`（`castle-persist` 分支从此拉出）。
> 性质：**只读分析**产物。本文不改任何代码/配置/脚本；所有读数来自对主检出、`E:/MirServer` 只读副本
> 的 `grep`/`sed`/`sha256sum`/`stat`/`iconv` 实测，未跑 `cargo`（避免与在飞车道争 CPU），
> 未起服务端/客户端/代理，未写数据库。
> 相关既有设计：`mir2-rs/tests/parity/m3/design-castle-war-area-20261021.md`（§6.5 落盘怪癖、§8.3 C14–C16、§9 W-4）、
> `design-castle-official-20261022.md` §5.3、`design-castle-file-load-20261013`（读侧）、
> `design-castle-entities-20261020.md`（实体内核）。

---

## 0. 结论摘要（先看这 7 条）

1. **城堡持久化不是数据库**：`sql/` 三个库对 `castle` / `sabuk` 的命中数都是 **0**（§2.1）。
   城堡状态全部落在 `Envir/Castle/` 下的**三个文本文件**：`List.txt`、`<N>/SabukW.txt`、`<N>/AttackSabukWall.txt`。
   ⇒ 本卡**不动 DB、不动 `crates/storage`**（该 crate 城堡相关命中 0，§2.2），因此**不触发 storage 接口冻结通知**。
2. **oracle 有 6 个写触发点、没有退出存档**（§1.4）。最主要的是 `SystemProcessor.ShowOnline` 里
   `ConsoleShowUserCountTime`（默认 10 分钟）门的 `CastleMgr.Save()`。
3. **同一个 `Save()` 里两条路径口径不同**：`SaveConfigFile` 写 **ctor 时绑定**的 `Castle/<CastleDir>/SabukW.txt`
   （部署 = `Castle/Castle/SabukW.txt`），而 `SaveAttackSabukWall` 用**当前** `ConfigDir`
   （部署 = `Castle/0/AttackSabukWall.txt`）；`LoadAttackSabukWall` 又用 **ctor 路径**
   ⇒ 申请表**写读不同源，重启后永远读不回来**。§1.6 用部署件字节/时间戳把这条链钉死。
4. **装载城（`LoadCastleList` 造出来的城）在 `SaveConfig` 里必 NRE**：它们只跑 ctor，没跑 `LoadConfig`，
   `Archers[i]` 为 `null`（`CastleConf.cs:266-268`）。NRE 打断整条 `CastleMgr.Save`，所以
   `Castle/0/SabukW.txt` 至今保持 2026-10-09 的原样（§1.6）。
5. **Rust 现状**：`CastleEvent::SaveAttackSabukWall` 只落 DBG 日志（`bins/game-svr/src/main.rs:18453-18456`；
   `CastleEvent` 定义 `crates/world/src/castle.rs:653-654`）；`crates/envir` **有** `List.txt`/`AttackSabukWall.txt`
   的 serializer，**完全没有 `SabukW.txt` 写侧**（模块头 `crates/envir/src/castle.rs:28` 明确"不写"）；
   启动只把防御规格喂进 world（`bins/game-svr/src/castle.rs:73-99`），
   **`TotalGold` / `TodayIncome` / `WarDate` / `IncomeToday` 全部丢弃**，`Name` 取自 `Config.CastleName`
   而不是文件里的 `[Setup] CastleName`，`master_guild` 恒 `None`；
   `load_castle_list` / `load_attack_sabuk_wall` 在 `bins/` 里**零调用**（只有 example 用）。
6. **实现切 3 片**：P1 world 快照与事件（`crates/world`）、P2 envir 有序 ini 写侧（`crates/envir`）、
   P3 host 排水与启动装载（`bins/game-svr`）。P1/P2 写集不相交，**可并行**；P3 依赖两者。
   tier：P1 = L；**P2 = R、P3 = R**（落盘面必 R），窗口 W-A22-1..4 不过不得 `closed`。
7. **两条"不许顺手修"的红线**：`CastleMapList` 每次启动 +6 累积（部署实测 **1332 项**，§1.6.4）与
   装载城 `SaveConfig` NRE，都是 oracle 现实；先 1:1 复刻并登记 `T-*`，不得在 A-22 里"修"。

---

## 1. oracle 事实（逐行锚 + 实测）

### 1.1 持久化面总览

| 文件 | 路径（部署） | 谁写 | 谁读 | 编码/换行 |
| --- | --- | --- | --- | --- |
| 城列表 | `Castle/List.txt`（`CastleFile=Castle/List.txt`） | `CastleManager.SaveCastleList` | `LoadCastleList` | GB2312 + CRLF（`StringList.SaveToFile`） |
| 城配置 | `Castle/Castle/SabukW.txt`（默认城，ctor 路径） | `CastleConf.SaveConfig` → `ConfigFile.Save` | `CastleConf.LoadConfig` | GB2312 + CRLF；**每段后多一空行** |
| 攻城申请 | `Castle/0/AttackSabukWall.txt`（当前 `ConfigDir`） | `UserCastle.SaveAttackSabukWall` | `UserCastle.LoadAttackSabukWall`（**ctor 路径** `Castle/Castle/...`） | GB2312 + CRLF |

常量：`CastleConst.AttackSabukWallList = "AttackSabukWall.txt"`（`src/M2Server/Castle/CastleConst.cs:18`）、
`CastleConst.SabukWFileName = "SabukW.txt"`（同文件 `:22`）。
配置默认：`CastleDir = "Envir\\Castle\\"`、`CastleFile = "Envir\\Castle\\List.txt"`
（`src/Modules/SystemModule/Conf/Model/GameSvrConf.cs:1293-1294`；
读侧 `ServerConf.cs:73-74`，`CastleFile` 缺省用**已读到的** `CastleDir` 拼）。
部署覆盖：`Server.conf`（UTF-8 BOM）`:65 CastleDir=Castle`、`:71 CastleFile=Castle/List.txt`。

### 1.2 `LoadAttackSabukWall` / `SaveAttackSabukWall`

| 事实 | 锚 |
| --- | --- |
| 读：路径 = `Path.Combine(BasePath, Config.CastleDir, **ConfigDir**)`；目录不存在则建；文件不存在则**直接 return**（不建文件） | `UserCastle.cs:242-256` |
| 读：先清空 `AttackWarList`；逐行 `HUtil32.GetValidStr3(sData, ref guildName, {' ', '\t'})` | `UserCastle.cs:259-268` |
| 读：`GuildMgr.FindGuild(guildName) == null ⇒ continue`（**未知行会丢弃**） | `UserCastle.cs:268-272` |
| 读：`ArrestStringEx(s20, "\"", "\"")` 取引号内；`DateTime.TryParse` 失败 ⇒ `AttackDate = DateTime.Now` | `UserCastle.cs:274-285` |
| 写：路径同上（当前 `ConfigDir`），目录不存在则建 | `UserCastle.cs:293-300` |
| 写：行 = `sGuildName + "       \"" + AttackDate + '"'`（**7 个空格**）；`StringList.SaveToFile` ⇒ `WriteLine` ⇒ CRLF | `UserCastle.cs:303-308`；`src/OpenMir2/Common/StringList.cs:260-270` |
| 写：**空表 ⇒ 0 字节文件**（`StreamWriter(fileName, false, ...)` 截断） | 同上；部署实测 `Castle/0/AttackSabukWall.txt` = 0 B |
| 写：`SaveToFile` 先 `GetEncoding(fileName)` ⇒ `new FileStream(FileMode.Open)`，**文件不存在则抛 `FileNotFoundException`** | `StringList.cs:260-263 / 293-299` |

`AttackerInfo` 只有三个字段：`AttackDate` / `sGuildName` / `Guild`（`src/Modules/SystemModule/Data/AttackerInfo.cs:5-17`）。

### 1.3 `CastleConf.LoadConfig` / `SaveConfig`（字段级）

`LoadConfig`：`src/M2Server/Castle/CastleConf.cs:15-94`。所有 `ReadWrite*` 在**缺键时把默认值写进内存缓存**
（`ConfigFile.cs:55-113`），`Save()` 再**整文件重写缓存**（`ConfigFile.cs:348-380`）⇒ 缺键会以默认值出现在输出里。

| 段.键 | 读（行） | 缺省 | 写（行） | 写条件 |
| --- | --- | --- | --- | --- |
| `Setup.CastleName` | `:17` | `Config.CastleName` | `:109-112` | `!string.IsNullOrEmpty(sName)` |
| `Setup.OwnGuild` | `:18` | `""` | `:113-116` | 非空 |
| `Setup.ChangeDate` | `:19` | `DateTime.Now` | `:117` | **无条件**（`yyyy-MM-dd HH:mm:ss`，会把 `1998-01-01` 归一成 `1998-01-01 00:00:00`） |
| `Setup.WarDate` | `:20` | `DateTime.Now` | `:118` | 无条件 |
| `Setup.IncomeToday` | `:21` | `DateTime.Now` | `:119` | 无条件 |
| `Setup.TotalGold` | `:22` | `0` | `:120-123` | **`!= 0` 才更新缓存值**（0 时保留文件里的旧值） |
| `Setup.TodayIncome` | `:23` | `0` | `:125-128` | 同上 |
| `Defense.CastleMapList` | `:24-38` | `""` | `:130-137` | 非空；逐项 `图名 + ","` ⇒ **带尾随逗号** |
| `Defense.CastleMap` | `:39` | `"3"` | `:139-142` | 非空 |
| `Defense.CastleHomeMap/X/Y` | `:40-42` | `"3"`/644/290 | `:144-157` | 非空 / `!= 0` |
| `Defense.CastleWarRangeX/Y` | `:43-44` | 100/100 | `:159-167` | `!= 0` |
| `Defense.CastlePlaceMap` | `:45` | `"0150"` | `:169-172` | 非空 |
| `Defense.CastleSecretMap` | `:46` | `"D701"` | `:174-177` | 非空 |
| `Defense.CastlePalaceDoorX/Y` | `:47-48` | 631/274 | `:179-187` | `!= 0` |
| `Defense.MainDoorX/Y/Name` | `:50-52` | 672/330/`MainDoor` | `:191-202` | `!= 0` / 非空 |
| `Defense.MainDoorOpen` | `:53` | `true` | `:203-207` | **只有 `MainDoor.BaseObject != null` 才写** |
| `Defense.MainDoorHP` | `:54` | 2000 | `:206` | 同上，写 `BaseObject.WAbil.HP` |
| `Defense.{Left,Center,Right}Wall{X,Y,Name}` | `:57-71` | 见默认值 | `:210-260` | `!= 0` / 非空 |
| `Defense.{Left,Center,Right}WallHP` | `:60/66/72` | 2000 | `:223-265` | **只有 `BaseObject != null`** |
| `Defense.Archer_{i}_X/Y/Name` | `:74-83` | `0/0/弓箭手` | `:266-282` | `!= 0` / 非空 |
| `Defense.Archer_{i}_HP` | `:80` | 2000 | `:283-290` | **无条件**：`BaseObject` 为空写 **0**，否则写 `WAbil.HP` |
| `Defense.Guard_{i}_*` | `:84-93` | `0/0/守卫` | `:292-317` | 同上（`HP` 无条件） |

其它硬事实：

- `SaveConfig` 开头先 `Directory.CreateDirectory(.../ConfigDir)`，随后
  `if (MapMgr.GetMapOfServerIndex(MapName) != ServerIndex) return;`（`CastleConf.cs:98-107`）。
  `GetMapOfServerIndex(null) == 0`（`src/GameSrv/Maps/MapManager.cs:169-180`）⇒ **`MapName=null` 的装载城在
  ServerIndex=0 下不会被这道门挡住**，会继续往下走到 NRE（§1.5）。
- **写的是 ctor 绑定的 `FileName`，不是 `ConfigDir`**：`CastleConf` 在 `UserCastle` ctor 里用
  `Path.Combine(BasePath, Config.CastleDir, ConfigDir)` 构造（`UserCastle.cs:50-55`），此时 `ConfigDir` 是 ctor 入参；
  `CastleManager.Initialize` 是在 `castle.Initialize()` **之后**才把 `ConfigDir` 改 `"0"`（`CastleManager.cs:66-75`）。
  `SaveConfig` 里的 `filePath`（`:98`）只用于建目录。
- `Save()` 的格式（`ConfigFile.cs:348-380`）：`[段]` → 逐键 `key=value` → **再补一个空行**；段/键顺序 =
  内存字典**插入序**（= 读入顺序，新键追加在末尾）；`StreamWriter(..., GB2312)`，无 BOM，`WriteLine` ⇒ CRLF。
- `Load()` 的怪癖（`ConfigFile.cs:256-346`）：跳过空行/`;` 注释/`/* */`；**重复段名 ⇒ `goto Label_02AE` 直接停止解析**
  （其后全部行丢弃）；同段重复键 ⇒ **先到先得**（`if (!curSec.ContainsKey(...))`）；值两侧 `Trim()`。
- `WriteDateTime` 固定 `"yyyy-MM-dd HH:mm:ss"`（`ConfigFile.cs:449-460`）；`WriteBool` 写 `1/0`（`:406-417`）；
  `WriteInteger` 写 `val.ToString()`（`:419-436`）。
- `ReadDateTime` 用 `DateTime.Parse(str)`（`ConfigFile.cs:155-168`）⇒ **非法日期串抛异常**（不是回落默认）。

### 1.4 写触发时机（全仓调用链）

| # | 触发 | 锚 | 写什么 |
| --- | --- | --- | --- |
| 1 | 启动 `CastleMgr.Initialize()`（空表分支） | `CastleManager.cs:62-87`（`Save()` 在 `:83`） | `SaveCastleList` + 该城 `SaveConfig` + `SaveAttackSabukWall` |
| 2 | `SystemProcessor.ShowOnline`，门 `ConsoleShowUserCountTime`（默认 600000ms） | `SystemProcessor.cs:54-62`（`CastleMgr.Save()` 在 `:61`）；默认值 `GameSvrConf.cs:1439` | 同 #1，**每 10 分钟** |
| 3 | `UserCastle.Run` 每日 20:00 开战成功 | `UserCastle.cs:363-374`（`SaveAttackSabukWall()` 在 `:367`） | 申请文件 |
| 4 | `UserCastle.AddAttackerInfo(GuildInfo)` | `UserCastle.cs:998-1014`（`:1010`） | 申请文件 |
| 5 | `UserCastle.GetCastle(GuildInfo)`（改归属） | `UserCastle.cs:539-560`（`SaveConfigFile()` 在 `:546`） | 配置文件 |
| 6 | CastleOfficial `@@castlename` | `src/GameSrv/Npc/CastleOfficial.cs:139-154`（调 `Castle.Save()`，`UserCastle.cs:448-452`） | 配置文件 + 申请文件 |

- **没有退出存档**：`GameApp.cs` / `AppService.cs` 的 `Stopping/Stopped` 都不调 `CastleMgr.Save`
  （`grep -n "CastleMgr" src/GameSrv/{GameApp,AppService,Program,AppServer}.cs` 只有 `:90/:213/:246`）。
- `IncRateGold` 里的 10 分钟 `SaveTick` **不是落盘口**（`UserCastle.cs:796-807`）：只刷新 tick 并有一行被注释掉的
  EventLog。不要把它当成"税写盘"来接。
- `CastleManager.Save()` 的内部顺序固定：`SaveCastleList()` → 按 `CastleList` 下标升序逐城 `castle.Save()`，
  每城再 `SaveConfigFile()` → `SaveAttackSabukWall()`（`CastleManager.cs:144-152`、`UserCastle.cs:448-452`）。

### 1.5 事务 / 失败语义（无事务、非原子、异常吞）

| 情形 | oracle 行为 | 锚 |
| --- | --- | --- |
| 写文件失败 / 开关异常 | 无 try/catch、无事务、无回滚；`ConfigFile.Save` 用 `FileMode.Truncate` **原地截断**（崩溃可留半截文件） | `ConfigFile.cs:348-380` |
| 异常冒泡到 tick | `TimerScheduledService.ExecuteAsync` 捕获并 `LogService.Error`，**该 tick 后续工作全部跳过**（`ShowOnline` 在 `ProcessGameRun` 之前） | `src/OpenMir2/TimerScheduledService.cs:45-66`；`SystemProcessor.cs:45-50` |
| 装载城（未 `LoadConfig`）进 `SaveConfig` | `Archers[i]` 为 `null` ⇒ `objUnit.nX` **NullReferenceException**（`Archers` 数组在 ctor 里只 `new` 未填充） | `UserCastle.cs:49` + `CastleConf.cs:266-269` |
| 申请文件不存在时首次 `SaveAttackSabukWall` | `StringList.SaveToFile` → `GetEncoding(FileMode.Open)` ⇒ **抛 `FileNotFoundException`**（不建文件） | `StringList.cs:260-263/293-299` |
| 配置目录不存在 | 建目录（`SaveConfig`/`SaveAttackSabukWall`/`SaveCastleList` 都先建目录） | `CastleConf.cs:100-103` 等 |

> NRE 这条**已被部署件证实**：`Castle/0/SabukW.txt` 至今是 2026-10-09 的 1748 字节原样，
> 而同一 `Save()` 里的 `List.txt` 与 `Castle/Castle/SabukW.txt` 都是 2026-10-21 18:20:49（§1.6）。

### 1.6 启动装载顺序与"读/写不同源"（实测证据链）

启动序（`src/GameSrv/GameApp.cs`）：`CastleMgr.Initialize()` `:213` → … → `WorldEngine.Initialize()` `:244`
→ **`CastleMgr.LoadCastleList()` `:246`**。

1. `Initialize`（`CastleManager.cs:62-87`）：`new UserCastle(Config.CastleDir)` ⇒ ctor 里
   `CastleConf(Castle/Castle/SabukW.txt)` 并 `Load()`；`castle.Initialize()` 里 `LoadConfig()` + **`LoadAttackSabukWall()`**
   （`UserCastle.cs:110-112`）此时 `ConfigDir` 仍 = `"Castle"` ⇒ 读 `Castle/Castle/AttackSabukWall.txt`（部署**不存在** ⇒ 空表返回）；
   随后 `ConfigDir = "0"` + `EnvirList.Add("0151".."0156")` ⇒ `Save()`。
2. `LoadCastleList`（`CastleManager.cs:154-176`）：读 `Castle/List.txt`，逐行 `Trim()` 非空 ⇒ `new UserCastle(行)`
   —— **只跑 ctor，不跑 `Initialize()`** ⇒ `MapName=null`、`MainDoor=null`、`Archers[i]=null`、`AttackWarList` 空。
3. `SaveCastleList` 写的是**下标** `i.ToString()`（`CastleManager.cs:178-192`），不是目录名。
4. `CastleMapList` 只 append 不清空：`LoadConfig` 把文件里的列表全读进 `EnvirList`（`CastleConf.cs:24-38`），
   `Initialize` 再 `Add` 6 张（`CastleManager.cs:70-75`），`SaveConfig` 全量写回 ⇒ **每次启动 +6 项**。

实测（只读副本 `E:/MirServer/M2GameSvr/Castle/`，2026-10-21）：

| 文件 | 大小 | sha256 | mtime | 说明 |
| --- | --- | --- | --- | --- |
| `List.txt` | 6 B（`0\r\n1\r\n`，2 行） | `9b38d284bae4dc26e593ab0d5cb50d846c3328a8c0467291a8a473cf15c1615b` | 2026-10-21 18:20:49 | `SaveCastleList` 写（Count=2） |
| `Castle/SabukW.txt` | 8870 B，131 个 `=` 键 | `b84c26790c994ed0c8ab7d569c89f4edfb20f074e33f652995d386421b8dd053` | 2026-10-21 18:20:49 | 默认城 `SaveConfig`（ctor 路径） |
| `0/AttackSabukWall.txt` | 0 B | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` | 2026-10-21 18:20:49 | 默认城 `SaveAttackSabukWall`（当前 `ConfigDir="0"`，空表 ⇒ 截断为 0） |
| `0/SabukW.txt` | 1748 B，92 个 `=` 键 | `6021523b8bc4ada412588109c4417a9876dc30f62a6b086b18ec9cc230eb78ed` | **2026-10-09 01:32:29** | **未被运行期写过**（装载城 NRE 挡在前面） |
| `1/SabukW.txt` | 0 B | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` | 2026-10-10 22:21:25 | 某次 `new UserCastle("1")` 的 `ConfigFile.Load` 建空文件副作用 |
| `Castle/AttackSabukWall.txt` | 不存在 | — | — | 读路径指向它，但它从未生成 |

补充读数：

- `Castle/SabukW.txt` 的 `CastleMapList` = **1332 项 = 6 × 222**，带尾随逗号
  （`iconv -f GBK -t UTF-8 Castle/SabukW.txt | grep '^CastleMapList=' | sed 's/^CastleMapList=//' | tr ',' '\n' | awk 'NF' | wc -l`）。
  **注意：`design-castle-war-area-20261021.md` §6.5 写的「1314 项 = 219 次启动」已过期**——该文件在
  2026-10-21 18:20:49 又被重写了一次（+18 项）。以实测 1332 为准。
- 键数自洽：`[Setup] 7 + [Defense] 11 + 门/墙 5+12 + 弓手 12×4 + 守卫 12×4 = 131`，与 `Castle/SabukW.txt` 的 131 个 `=` 一致。
- `0/SabukW.txt` 的键序与 `Castle/SabukW.txt` 不同（多了 `NextWar=0`，段内有空行）⇒ **写侧必须保留"读入顺序"**，
  不能按 `LoadConfig` 调用序重排（§4 P2 的 `IniOrdered`）。

---

## 2. SQL 面与 storage 面

### 2.1 `sql/`：城堡相关表 = 0

```
$ for f in sql/*.sql; do echo -n "$f castle="; grep -io castle "$f" | wc -l; echo -n " sabuk="; grep -io sabuk "$f" | wc -l; done
sql/mir2_account.sql castle=0 sabuk=0
sql/mir2_data.sql    castle=0 sabuk=0
sql/mir2_db.sql      castle=0 sabuk=0
```

`CREATE TABLE` 清单里也没有城堡表（`mir2_db.sql`：`characters*` 11 张；`mir2_data.sql`：`goldsales`/`magics`/`monsters`/
`stditems`/`upgradeinfo*`）。
⇒ **结论：不得新增城堡表**（违反"内容冻结"）。任何"把城堡改存 DB"的方案直接否决。

### 2.2 `crates/storage`：城堡相关 API = 0

```
$ grep -rn "castle" mir2-rs/crates/storage/src | wc -l
0
$ grep -rn "fn .*castle" mir2-rs/crates/storage/src -i | wc -l
0
```

`crates/storage` 只镜像 DBSrv 的四个接口（`PlayRecordStorage` / `PlayDataStorage` / `CacheStorage` / `MarketStorage`，
`crates/storage/src/lib.rs:1-22`），与城堡无关。
⇒ **缺口不是"storage 少了一套 API"，而是"写文件这一层根本还没实现"**；正确落点是
`crates/envir`（路径/序列化）+ `bins/game-svr`（定时器与 IO），与行会文件层 `crates/data/src/guild.rs` 同构。

### 2.3 冻结通知义务

接口冻结线只覆盖 `crates/protocol`、`crates/storage`（AGENTS「编码约定」）。本设计两者都不动
⇒ **无需通知**。真正需要通知的是 `crates/envir` 的公开面变化：`SabukWConfig` 被 `bins/game-svr/src/castle.rs:73-99`
消费，若增删字段须同步适配层（非冻结面，但属跨 crate 写集）。

---

## 3. 当前 Rust 状态（实测行号）

### 3.1 事件与排水

| 事实 | 锚 |
| --- | --- |
| `CastleEvent::SaveAttackSabukWall` 变体（无载荷） | `mir2-rs/crates/world/src/castle.rs:653-654` |
| `UserCastle.run` 开战时发射（注释 `UserCastle.cs:367`） | `crates/world/src/castle.rs:1306-1307` |
| `add_attacker_info` 发射（`UserCastle.cs:1010`） | `crates/world/src/castle.rs:1498-1502` |
| 单测钉死事件序 | `crates/world/src/castle.rs:2173`、`:2206` |
| host 排水：**只落 DBG 日志** | `mir2-rs/bins/game-svr/src/main.rs:18453-18456`（注释 `:18453-18455` + `other => glog!("DBG", "城堡状态机事件（未接线）：{other:?}")` `:18456`） |
| 10s `CastleMgr.Run` 门已接、与 Guild 同块、Guild 先、严格 `>` | `bins/game-svr/src/main.rs:15907-15908`（`castle_run_tick`）、`:18423-18460` |
| **没有** `CastleMgr.Save` 对应的 10 分钟 tick 变量 | `grep -n "castle_save_tick" bins/game-svr/src/main.rs` ⇒ 0 命中 |

### 3.2 `crates/envir`：读齐、写缺

| 项 | 状态 | 锚 |
| --- | --- | --- |
| `LoadCastleList` 路径 + 解析 | 有（`load_castle_list`） | `crates/envir/src/castle.rs:561-571` |
| `LoadAttackSabukWall` 路径 + 逐行解析 | 有（`load_attack_sabuk_wall` / `parse_attack_sabuk_wall_line`） | `:473-490`、`:596-616` |
| `SaveAttackSabukWall` 序列化 | 有（`serialize_attack_sabuk_wall`，CRLF） | `:493-501` |
| `SaveCastleList` 序列化 | 有（`serialize_castle_list`，写下标 + CRLF） | `:445-455` |
| `SabukW.txt` **读** | 有（`SabukWConfig::from_ini` / `load_sabuk_w`） | `:150-380`、`:578-590` |
| `SabukW.txt` **写** | **完全没有**（模块头明写"不写 SabukW.txt"） | `:28-30` |
| 有序 ini（保留段/键的读入序） | **没有**：`IniFile` 用 `HashMap`（无序） | `crates/data/src/exps.rs:18-21` |
| 写侧"缺键回写默认值"语义 | **没有** | — |
| `AttackSabukWall` 未知行会过滤 | 由调用方负责（注释已声明） | `:467-471` |
| 日期原串保留（已登记偏差） | 有（`attack_date: String` + `attack_date_parsed`） | `:458-470` |

### 3.3 `crates/world` 字段分类：该落盘的 / 运行期态

`UserCastle` 结构见 `crates/world/src/castle.rs:936-1003`。

| 分类 | 字段 | 是否落盘 | 说明 |
| --- | --- | --- | --- |
| **配置（读自 SabukW，写回原样）** | `name`、`map_name`、`home_map`、`home_x/y`、`war_range_x/y`、`main_door/left/center/right_wall` 的 `x/y/hp`、`archers[12]`/`guards[12]` 的 `x/y/hp` | **是** | 但 Rust 在 `initialize` 之后**只保留已造怪成功的槽**（`None` = 无 `BaseObject`），**丢失 `Archer_i_Name` 与未造怪槽的 `x/y`** ⇒ P1 必须新增 `specs: Option<CastleDefenseSpecs>` 承载配置面 |
| **状态（运行期写回）** | `own_guild`、`total_gold`、`today_income`、`income_today`、`war_date`、门/墙 `hp` | 部分是 | `TotalGold/TodayIncome/ChangeDate/WarDate/IncomeToday` 落盘；`UnderWar` **不落盘**（oracle 也没有对应键） |
| **纯运行期（不落盘）** | `is_start_war`、`show_over_msg`、`start_castle_war_tick`、`save_gold_tick`、`attack_guild_list`、门/墙的 `opened/stone_mode/death/ghost/dir/hold_place/set_map_flaged/struck_tick/health_tick/death_tick`、`castle_map_id/palace_map_id/secret_map_id/envir_map_ids`、`server_index/map_owner_server_index`、`config` | **否** | 重启后由配置+`Initialize`重建；`MainDoorOpen` 落盘的是**配置值**，不是运行期 `IsOpened` |
| **申请队列** | `attack_war_list` | **是**（独立文件） | 但读写路径不同源（§1.6）⇒ 生产上重启必空 |

### 3.4 启动装载的丢失项（P3 的核心缺口）

`specs_from_sabuk`（`bins/game-svr/src/castle.rs:73-99`）只搬防御面，
`initialize_castle`（`:201-259`）只把 `CastleDefenseSpecs` 喂进 `UserCastle::initialize`。
因此启动后：`total_gold=0`、`today_income=0`、`war_date=None`、`name=Config.CastleName`（不是文件里的 `CastleName`）、
`own_guild` 只有名字、`attack_war_list=[]`。**部署文件里 `TotalGold=0` 掩盖了这个洞**，
一旦城主收税后重启就会丢钱——这是本卡最实质的行为差异。
`UserCastle::initialize` 也**不设置** `total_gold/today_income/income_today/war_date`（`castle.rs:1089-1240`）。

---

## 4. 实现切分（3 个 patch）

> 总原则：先纯层、再写侧、最后宿主；`crates/envir` 与 `crates/world` 写集不相交可并行；
> 任何写盘路径都必须先有"字节在构造级可判定"的用例，再进窗口。

### P1 `castle-persist-world`（纯层快照 + 事件语义）

- **写集**：`mir2-rs/crates/world/src/castle.rs`（+ 文件内 `mod tests`）。**不碰** `bins/**`、`crates/envir/**`。
- **依赖**：无（可并行于 P2）。
- **内容**：
  1. `UserCastle` 增 `specs: Option<CastleDefenseSpecs>`（`initialize` 时 `Some`），用于写回
     `Archer_i_X/Y/Name`、`Guard_i_*` 与 `MainDoorOpen`（oracle 的 `ArcherUnit` 数组恒非空）；
  2. `UserCastle` 增 `config_dir: String` 与 `ctor_config_dir: String`（oracle 两个路径口径，§1.6），
     默认空串，由宿主填；
  3. `UserCastle` 增 `change_date: Option<String>`（读入原串）+ `pub fn get_castle(&mut self, guild, name, now)` 设置
     `change_date`（对应 `UserCastle.cs:539-560`；本卡只做字段，不改 `GetCastle` 的可达性登记）；
  4. 新纯函数 `pub fn save_snapshot(&self) -> Result<CastleSaveSnapshot, CastleSaveSkip>`（含 §3.3「该落盘」全集，**不含 IO**）；
  5. `CastleSaveSkip { NreArchersNull, MapOwnerNotSelf }`：`specs.is_none()`（= 未 `LoadConfig`）⇒ `NreArchersNull`；
     `map_name.is_some() && map_owner_server_index != server_index` ⇒ `MapOwnerNotSelf`；
  6. 事件面**保持现状**（`SaveAttackSabukWall` 已在 `run`/`add_attacker_info` 发射，`:1306/:1500`），
     不新增 `CastleEvent::Save`（10 分钟 Save 是宿主定时器，不是状态机副作用）。
- **判据与牙**（构造级）：
  - `C-A22-1 castle_save_snapshot_fields`（**L**）：构造城 = 部署 `0/SabukW.txt` 语义 +
    `total_gold=123456`、`today_income=77`、门 HP=1800、左墙 `Some`、右墙 `None` ⇒ 断言快照逐字段。
    **牙**：把 `total_gold/today_income` 互换 ⇒ 红；`RightWall=None` 仍写 HP ⇒ 红；未 `initialize` 的城不返回
    `NreArchersNull` ⇒ 红。
  - `C-A22-2 castle_save_path_dirs`（**L**）：`config_dir="0"`、`ctor_config_dir="Castle"`
    ⇒ 快照带两个目录名，`envir` 拼出 `Castle/Castle/SabukW.txt` 与 `Castle/0/AttackSabukWall.txt`。
    **牙**：把两者合并成一个字段 ⇒ 两路径相同 ⇒ 红。
- **tier**：**L**（纯内存，不可能只在真机暴露差异；单测 + 语义等价自证）。
  `tier_why`：无 RNG、无时序、无 IO；但 A-22 父卡仍按 R 收口，P1 合入不等于卡 `closed`。

### P2 `castle-persist-writer`（envir 有序 ini 写侧）

- **写集**：`mir2-rs/crates/envir/src/castle.rs`、`crates/envir/src/lib.rs`（re-export）、`crates/envir/tests/**`。
- **依赖**：无（可并行于 P1）；`SabukWConfig` 读侧已就绪。
- **内容**：
  1. `IniOrdered`（新）：`Vec<(String, Vec<(String, String)>)>` + `parse_ordered(bytes)`，**逐条复刻 `ConfigFile.Load`**
     （空行/`;`/`/* */` 跳过；`;;` 段注释行；重复段 ⇒ 停止解析；同段重复键先到先得；值 `Trim`）；
  2. `sabuk_w_write_back(cache: &mut IniOrdered, defaults: &SabukWDefaults)`：按 §1.3 的 `LoadConfig` **调用序**补默认键
     （缺则追加，已有则不动），复刻 `ReadWrite*`。`CastleName/CastleHomeMap/CastleHomeX/Y/CastleWarRangeX/Y`
     这六个缺键默认在 oracle 里取 `UserCastle` 构造器的 `SystemShare.Config.*`（`Server.conf` 可覆盖，
     `GameSvrConf.cs:1782-1787`）⇒ 抽成 `SabukWDefaults` **参数**由 P3 注入；其余默认（`CastleMap="3"` 等）
     是 oracle 字面量，仍用常量。读侧对应 `SabukWConfig::from_ini_with_defaults`。
  3. `pub fn serialize_sabuk_w(base: &IniOrdered, overlay: &SabukWOverlay, defaults: &SabukWDefaults)
     -> Result<Vec<u8>, EnvirError>`：
     按 §1.3「写」列逐键 `set_in_place_or_append`，最后按段序 + `\r\n` + 每段末空行输出，
     `mir2_encoding::encode_gb2312_dotnet` 编码。三个日期键写前按 `DateTime.Parse` 归一成
     `yyyy-MM-dd HH:mm:ss`（`ConfigFile.WriteDateTime`，`ConfigFile.cs:449-459`；部署件
     `0/SabukW.txt` 的 `1998-01-01` ⇒ `1998-01-01 00:00:00`）；`MainDoorX/Y` 与三面墙 `*WallX/Y`
     按 `ReadWriteInt16`（`short`）解析，越界/非数字 ⇒ `EnvirError::SabukWInt16Invalid`
     （oracle `Convert.ChangeType(short)` 抛异常，不许 i32 静默回落）。
  4. `pub struct SabukWOverlay { castle_name, own_guild, change_date, war_date, income_today,
     total_gold, today_income, envir_list, main_door: { present, hp }, walls: [Option<i32>; 3],
     archers: [(name, x, y, hp_or_zero); 12], guards: [(...); 12] }`（与 `crates/world` 解耦，纯 envir 类型）;
  5. `pub fn write_sabuk_w(path, base, overlay, defaults) -> Result<(), EnvirError>`：截断写（非原子，
     与 oracle `FileMode.Truncate` 同语义；**不要** temp+rename），并复刻 `ConfigFile.Save` 的
     父目录 `Directory.CreateDirectory` 与只读文件「临时清 `IsReadOnly`、写成功后恢复」（`ConfigFile.cs:353-365`）；
  6. `pub fn write_attack_sabuk_wall(path, entries)` / `write_castle_list(path, count)` 薄封装（复用已有 serializer），
     并**复刻"文件不存在则不建"**：打开失败 ⇒ `EnvirError::Io`
     （与 `crates/data/src/guild.rs:399-405` 的 `MissingSaveTarget` 同款）。
- **判据与牙**：
  - `C-A22-3 sabuk_w_rewrite_idempotent`（**R**）：把部署 `Castle/Castle/SabukW.txt` 原样读入 →
    `serialize_sabuk_w` 用"同值 overlay"写回 ⇒ **sha256 完全不变**（`b84c2679…`）。
    **牙**：段末空行删掉 / `\r\n`→`\n` / 键序按字母排 / 用 `IniFile`(HashMap) 输出 ⇒ 各自红。
  - `C-A22-4 sabuk_w_missing_keys_writeback`（**L**，构造级）：输入只含 `[Setup]` + `CastleName=x`
    ⇒ 输出 131 个键、`[Defense]` 全默认、`CastleMapList=` 行为空 ⇒ **键存在**。
    **牙**：实现成"只写显式设置过的键" ⇒ 键数 ≠ 131 红。
  - `C-A22-5 sabuk_w_duplicate_section_stops`（**L**）：`[Setup]…[Setup]…` 之后还有 `[Defense]` ⇒ 后者**不出现**。
    **牙**：允许重复段 ⇒ 多出 `[Defense]` 红。
  - `C-A22-6 sabuk_w_gbk_bytes`（**L**）：含 `沙巴克/弓箭手/守卫` 的输出逐字节等于 cp936
    （`mir2_encoding::encode_gb2312_dotnet`，E-4 口径；不许 WHATWG GBK）。**牙**：UTF-8 ⇒ 字节红。
  - `C-A22-7 sabuk_w_conditional_values`（**L**）：`total_gold=0` ⇒ 保留 base 里的旧值（夹具 base 写 `TotalGold=9`）；
    `total_gold=5` ⇒ 变 5；`RightWall` 不存在 ⇒ `RightWallHP` 保留 base 值；`Archer_1` 无 entity ⇒ `Archer_1_HP=0`
    但 `Archer_1_X/Y/Name` 仍写。
    **牙**：无条件写 0 ⇒ 第一条红；把 `Archer_i_HP` 写成配置 HP ⇒ 最后一条红。
  - 复核回归（2026-10-21，`castle_writer.rs`）：`f1_sabuk_w_datetime_normalized`（F1 日期归一，夹具
    `0/SabukW.txt`；牙 = 写侧退回原串）、`low1_sabuk_w_defaults_injected`（默认值注入；牙 = 退回常量）、
    `low2_write_sabuk_w_creates_parent_dirs` / `low2_write_sabuk_w_toggles_readonly`（`ConfigFile.Save` 的
    建目录/只读位；牙 = 去掉对应分支）、`low3_main_door_and_wall_xy_reject_out_of_short`（`short` 越界显式错误；
    牙 = 退回 i32 静默回落）。
- **tier**：**R**。`tier_why`：落盘逐字节（存档面）；构造级只能证明"自洽 + 规则表"，**与 C# 同字节必须真机窗口**
  （W-A22-1）。`window_required: true`。

### P3 `castle-persist-host`（宿主排水 + 启动装载）

- **写集**：`mir2-rs/bins/game-svr/src/castle.rs`、`mir2-rs/bins/game-svr/src/main.rs`（城堡块 + 启动块）、
  `mir2-rs/tests/parity/m3/**`（证据/判据脚本）。
- **依赖**：P1 + P2 合入（或同批 rebase）。
- **内容**：
  1. 启动：`load_sabuk_specs` 之外再 `loader.load_sabuk_w` 保留**整份有序 base**；建城后把
     `total_gold/today_income/income_today/war_date/name/own_guild/config_dir/ctor_config_dir` 灌进 `UserCastle`（P1 字段）；
  2. 启动：`CastleMgr.Initialize` 的 `Save()`（`CastleManager.cs:83`）——写 `List.txt`（1 行 `0`）+ 默认城 `SaveConfig`
     + `SaveAttackSabukWall`；随后 `LoadCastleList`（`GameApp.cs:246`）读刚被截断的 `List.txt` ⇒ 装载 0/1 个**惰性城**
     （`map_name=None`、不挂实体、进 `world.castles` 但 `run` 立即返回）；
  3. 每 tick：新增 `castle_save_tick`，门 `world.clock_ms - castle_save_tick > ConsoleShowUserCountTime`
     （严格 `>`，默认 600000；**需先在配置层解析 `Server.conf [Setup] ConsoleShowUserCountTime`——当前 Rust 零命中**），
     在 10s 块内执行 `CastleMgr.Save` 等价物：`SaveCastleList` → 升序逐城 `SaveConfig` → `SaveAttackSabukWall`；
  4. 事件排水：`CastleEvent::SaveAttackSabukWall` ⇒ 立刻写 `Castle/<config_dir>/AttackSabukWall.txt`
     （对齐 §1.4 #3/#4；`Broadcast`/`ServerGroupMsg` 仍按 A-07 登记不实现）；
  5. 失败语义：`CastleSaveSkip::NreArchersNull` ⇒ 该城两文件都不写（见 §6 未决 2 对"是否中断后续城"的裁决）。
- **判据与牙**：
  - `C-A22-8 save_10min_gate_and_order`（**R**）：10000ms 跑 `run`、10001 跑；600000 **不**存、600001 存；
    一次 Save 的写序 = `List.txt` → 城 0 `SabukW` → 城 0 `AttackSabukWall` → 城 1 …
    **牙**：`>`→`>=` 红；把 Save 塞进 10s 门 ⇒ 假 tick 序列下多写红；城序倒置 ⇒ 文件内容序红。
  - `C-A22-9 attack_list_path_asymmetry`（**R**）：读 `Castle/Castle/AttackSabukWall.txt`、写 `Castle/0/AttackSabukWall.txt`；
    夹具让 `0/` 有 1 行、`Castle/` 为空 ⇒ 启动后 `attack_war_list.len()==0`；再触发一次事件 ⇒ `0/` 被写。
    **牙**：读路径改成 `config_dir` ⇒ 长度 1 红。
  - `C-A22-10 startup_restores_gold`（**R**）：夹具 `TotalGold=123456`、`TodayIncome=77`、`WarDate=…` ⇒
    启动后 world 字段一致；`save_snapshot` 回写后仍是 123456。
    **牙**：不恢复 ⇒ 0 红；把 `total_gold` 恢复成 `today_income` ⇒ 红。
  - `C-A22-11 startup_castle_shape`（**R**）：部署副本上 `List.txt` 2 行 ⇒ `Initialize` 的 Save 先把文件截成 1 行 ⇒
    `world.castles.len()==2`（默认 + 装载 `"0"`），装载城 `map_name=None`，默认城 `CastleMapList` 项数 = 原值 + 6。
    **牙**：把 `LoadCastleList` 提到 `Initialize` 之前 ⇒ 3 城红；dedupe `CastleMapList` ⇒ 项数红。
  - `C-A22-12 save_skip_loaded_castle`（**R**）：装载城 `SaveConfig` 走 `NreArchersNull` ⇒
    **`Castle/0/SabukW.txt` 字节不变**（sha 仍 `6021523b…`）。
    **牙**：给装载城补 `specs` 让它写成功 ⇒ sha 变红。
- **tier**：**R**。`tier_why`：时序（10 分钟门 / tick 边界）+ 落盘（存档字节）+ 启动形状。
  `window_required: true`。

### 4.4 顺序、并行与批次

| patch | 写集 | 依赖 | tier | 建议批次 |
| --- | --- | --- | --- | --- |
| P1 | `crates/world/src/castle.rs` | 无 | L | 与 P2 并行（不同 crate） |
| P2 | `crates/envir/src/{castle.rs,lib.rs}` | 无 | R | 与 P1 并行 |
| P3 | `bins/game-svr/{castle.rs,main.rs}` | P1+P2 | R | **H2 宿主串行队列**（与 R-11/R-12/R-22 同队） |

### 4.5 窗口项（R 不过不得 closed）

| ID | 内容 | 为什么必须窗口 |
| --- | --- | --- |
| **W-A22-1 文件字节** | 把 `E:/MirServer/M2GameSvr/Castle/` 复制到 `E:/tmp/...` 只读副本，改 BasePath 指向副本；C# 与 Rust 各跑「启动 + 一次 10 分钟 Save」；比 `List.txt`、`Castle/SabukW.txt`、`0/AttackSabukWall.txt`、`0/SabukW.txt`（应不变）的 sha256；**跨午夜**（把系统时区/时钟推过 00:00 后再 Save）比 `IncomeToday` 时间戳，**开战**（今日申请 + `StartCastlewarTime`）比 `WarDate` 时间戳——宿主 `refresh_castle_date_texts` 必须换成新墙钟（`UserCastle.cs:337/357` 的 `DateTime.Now`） | 存档逐字节（GBK/CRLF/键序/空行）+ 运行期时间戳 |
| **W-A22-2 tick 时序** | `ConsoleShowUserCountTime` 门与 10s/10min 边界的真机取值（含配置覆盖） | 时序 |
| **W-A22-3 申请表跨重启** | 构造 `0/AttackSabukWall.txt` 1 行 + `Castle/AttackSabukWall.txt` 不存在 ⇒ 重启后 `$CASTLEWARDATE` 仍「暂时没有行会攻城」 | 证明读写不同源 |
| **W-A22-4 失败路径** | 删掉 `List.txt` 再触发 Save ⇒ 两侧都不写**任何**城堡文件（oracle `SaveCastleList` 抛，`CastleManager.Save` 循环不执行）；删掉 `AttackSabukWall.txt` 再触发 Save ⇒ 两侧都抛/记错且**不建文件**；装载城 Save ⇒ 两侧都不写 `0/SabukW.txt` | 异常语义 |

> 窗口执行禁止写 `E:/MirServer`；不要停 `mysqld`（owner 2026-10-21 授权只覆盖 C# 栈）。

---

## 5. 风险与冲突面

### 5.1 与在飞车道的写集冲突

| 车道 / 卡 | 写集 | 与本设计的交集 | 处置 |
| --- | --- | --- | --- |
| A-09 `openmir2-weapon-upgrade-desc`（`wt-a09`，在飞） | `bins/game-svr/{bag.rs,itemwire.rs,trade.rs}` | **无**（P3 只碰 `castle.rs`/`main.rs` 城堡块） | 可并行；P3 提交前 rebase |
| R-11 / R-12 / R-22（商人税 / 城堡 NPC / 固定 NPC，H2 队列） | `bins/game-svr/{merchant.rs,shop.rs,npc.rs,npcs.rs,main.rs}` | `main.rs` 重叠（热点） | **必须串行**：P3 排在它们之后或让它们先让出 `main.rs` |
| `guildofficial` / `guild_runtime` | `bins/game-svr/src/guild*`、`crates/world/src/guild.rs` | 10s 块相邻（Guild 先、Castle 后） | 不改顺序、不改 guild 文件；仅在其后追加 Save 块 |
| A-01..A-21 其余 | `crates/world/{monai,combat,merchant}` | 无 | — |

`main.rs` 是**单线串行热点**：P3 的 10s/10min 块与 `guild_runtime::step` 同段，
合并前必须 rebase 并重跑四门禁（`cargo fmt --check` / `clippy -D warnings` / `test` / `msgcodegen --check`）。

### 5.2 冻结与通知义务

- `crates/protocol`、`crates/storage` **不动** ⇒ 无冻结通知。
- `crates/envir` 的 `SabukWConfig` 增字段（若需要）⇒ 同步 `bins/game-svr/src/castle.rs::specs_from_sabuk`；
  这是跨 crate 写集，P2 合入时必须通知 P3 owner。
- **禁止**新增 SQL 表/列（§2.1）。

### 5.3 既有登记（必须在本卡 review 里引用）

| 编号 | 关系 |
| --- | --- |
| **T-127**（`whitelist.md:538`） | 本设计的父登记：`CastleMgr.Run/Save` 排水未接线、`tier=R`、`window_required: true`。P3 落地后改写该条目的"未接线"措辞并保留窗口项。 |
| **T-131**（`whitelist.md:542`） | `MasterGuild` 装载相位（`GameApp.cs:213` 早于 `GuildMgr.LoadGuildInfo` `:245`）⇒ 启动恒 `None`。本设计**不得**在启动装载里"补 MasterGuild"。 |
| **B-15**（`whitelist.md:47`） | `CastleDoor` 9 格 `LowWall` 掩码不可恢复（BUG-REPLICA）。交叉点：`MainDoorOpen` 落盘的是**配置值**，重启后 `Initialize` 若为真会重放 `Open()` 掩码；不要把"开着的门"当成可持久化状态。 |
| B-105 / E-4 | cp936 编码口径（`encode_gb2312_dotnet`）：P2 写文件必须用它，不能用 WHATWG GBK。 |
| R-03 / R-13 | 计划表已把本卡拆成"world 事件面（R）"与"host 排水（R）"两行；本设计 P1/P3 与之对齐。 |
| C-17（盘点 §C） | 部署 `AttackSabukWall.txt` 0 字节 / `OwnGuild=` 空 / `TotalGold=0` ⇒ 窗口项**不应占 2h 块**，只做构造级 + W-A22 的只读副本冒烟。 |

### 5.4 行为等价 vs 修 bug 的红线

以下三处都是 oracle 的**真实行为**，A-22 只复刻、不修（要修另开批次并登记 `T-*`）：

1. `CastleMapList` 每次启动 +6、不 dedupe、带尾随逗号（§1.6.4）。
2. 装载城 `SaveConfig` NRE，导致 `Castle/<N>/SabukW.txt` 永不更新（§1.5）。
3. 申请表读 ctor 路径 / 写 `ConfigDir` 路径（§1.6）——**哪怕它让申请表永远读不回来**。

---

## 6. 未决问题（需 owner / reviewer 裁决，或后续片确认）

1. **`ConsoleShowUserCountTime` 的配置面**：Rust 当前**完全没解析**该键（`grep -rn ConsoleShowUserCountTime mir2-rs` = 0）。
   P3 需要新增解析（`Server.conf [Setup]`，默认 600000）。是否允许 P3 顺手扩 `GuildCastleConfig` 读侧？
   （写集会从 `bins` 扩到 `crates/data`，影响其它车道。）
2. **装载城 NRE 的宿主表达**：oracle 是"抛异常 ⇒ 中断整条 `CastleMgr.Save`（后续城/文件都不写）"；
   Rust 若用 `Result` 逐城跳过，则在"≥3 城"的构造下行为不同。默认建议 **1:1 中断**（更保守），
   但会牺牲健壮性；请 reviewer 在 `review.tier_why` 里明确。
3. **`ChangeDate/WarDate/IncomeToday` 的完整时间戳来源**：world 只有 `CastleDate(y,m,d)`；
   落盘要 `yyyy-MM-dd HH:mm:ss`。读入 `1998-01-01` 后 oracle 内存是 `1998-01-01 00:00:00` ⇒ 写回必然是**归一化串**，
   Rust 必须复刻归一化而不是"原串透传"；`GetCastle`/开战/跨天改写时用宿主注入的完整时间戳。**待确认**
   宿主注入点（`chrono::Local::now()`）放在 P1（world 收 `i64` 时间戳）还是 P3（envir overlay 收字符串）。
4. **`LoadCastleList` 是否纳入 A-22**：它目前生产零调用。不接 ⇒ `List.txt` 形状与 oracle 不同（少 2 行）；
   接 ⇒ 必须同时复刻装载城的 NRE 路径。建议接（C-A22-11/12 已按"接"设计），请 reviewer 确认范围。
5. **`Castle/1/SabukW.txt` 的来历**：mtime 2026-10-10 22:21，按"`Initialize` 先 Save 把 `List.txt` 截成 1 行"的
   收敛推论，正常启动不该装载目录 `1`。**未验证**：可能是 owner 手工/拷贝产生，也可能存在第二条装载路径。
   需窗口执行时在只读副本上打点确认（不阻塞本设计）。
6. **`AttackSabukWall` 的日期串**：oracle 落盘用 `DateTime.ToString()`（culture 相关），Rust 保留原串属已登记偏差
   （`crates/envir/src/castle.rs:41/461-496`）。本设计不改该偏差；逐字节判据只覆盖结构部分（会名 + 7 空格 + 引号）。

---

## 附录 A：实测命令与读数（可复现）

```sh
# A. SQL 面无城堡表
cd E:/Users/gxh/Documents/GitHub/OpenMir2
for f in sql/*.sql; do echo -n "$f castle="; grep -io castle "$f" | wc -l; done   # 全部 0

# B. storage 无城堡 API
grep -rn "castle" mir2-rs/crates/storage/src | wc -l                              # 0

# C. Rust 现状：事件只落 DBG
sed -n '18453,18456p' mir2-rs/bins/game-svr/src/main.rs
grep -n "CastleEvent::SaveAttackSabukWall" mir2-rs/crates/world/src/castle.rs      # 653/1307/1500/2173/2206

# D. 部署件字节
cd E:/MirServer/M2GameSvr/Castle
stat -c '%y %s %n' List.txt 0/SabukW.txt 0/AttackSabukWall.txt 1/SabukW.txt Castle/SabukW.txt
sha256sum List.txt 0/SabukW.txt 0/AttackSabukWall.txt Castle/SabukW.txt
iconv -f GBK -t UTF-8 Castle/SabukW.txt | grep '^CastleMapList=' | sed 's/^CastleMapList=//' \
  | tr ',' '\n' | awk 'NF' | wc -l                                                 # 1332（= 6 × 222）
iconv -f GBK -t UTF-8 Castle/SabukW.txt | grep -c '='                              # 131
iconv -f GBK -t UTF-8 0/SabukW.txt      | grep -c '='                              # 92

# E. 配置口径
grep -a -n -i "castle" E:/MirServer/M2GameSvr/Server.conf | head -20              # CastleDir=Castle / CastleFile=Castle/List.txt
file E:/MirServer/M2GameSvr/Server.conf                                          # UTF-8 (with BOM)
```

**读数汇总**（2026-10-21，只读）：

| 项 | 值 |
| --- | --- |
| `List.txt` | 6 B / 2 行 / CRLF |
| `Castle/SabukW.txt` | 8870 B / 131 键 / `CastleMapList` 1332 项 / mtime 18:20:49 |
| `0/SabukW.txt` | 1748 B / 92 键 / mtime **2026-10-09**（运行期未写） |
| `0/AttackSabukWall.txt` | 0 B / mtime 18:20:49 |
| `1/SabukW.txt` | 0 B / mtime 2026-10-10 |
| `Castle/AttackSabukWall.txt` | 不存在 |
| `sql/` castle 命中 | 0 |
| `crates/storage` castle 命中 | 0 |
| `ConsoleShowUserCountTime`（Rust 解析） | 0 命中 |
