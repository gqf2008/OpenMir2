# 车道 gmdesign — GM `@` 命令面设计（audit #17 / inventory A-06）

- 基线：`master` 实解到 **`b46805058`**（任务书写 `4a887354e`；`git worktree add /e/tmp/wt-gmdesign -b gm-commands-design master` 时 master 已前移到 `b46805058`）。本文所有行号以该提交为准。
- 车道：`/e/tmp/wt-gmdesign`，branch `gm-commands-design`；纯文档，未改 `src/**`、`sql/**`、`MirClient/`、`mir2-rs/crates/{protocol,storage}`、`E:/MirServer/**`；未起进程、未占端口、未写库。
- 索引来源：`docs/m3-remaining-inventory-20261022.md` A-06（audit #17）；oracle `src/Modules/GameCommand/**`（185 个 `[Command]` 类 / 177 个文件）、`src/M2Server/Player/PlayObject.Chat.cs`、`src/GameSrv/DB/LocalDB.cs`。
- 结论口径：**行为等价优先**；本文只做「现状清点 + 设计 + 判据草案」，不把既有缺口当新发现，也不替 owner 做产品裁决。

---

## 0. 摘要（8 条）

1. **分发链已完全查清**：`CM_SAY(3030)` → `PlayObject.ProcessUserLineMsg` → `sData.StartsWith("@")` → `GameCommandSystem.Execute` → `ExtractCommandAndParameters`（`Trim` + 去首 `@` + 按**单个 ASCII 空格**切首 token）→ `CommandMaps`（`OrdinalIgnoreCase`）→ `GameCommand.Handle`（权限门 + 按 `Execute` 形参个数三选一派发）→ 返回值非空则 `SysMsg(Red, Hint)`。
2. **命令名不是代码里的名字**：185 条里 160 条经 `[RegisterCommand]` 被 `Command.conf` 覆盖名字与权限。audit #17 点名的 `@LetGuild`/`@LetTrade`/`@GroupRecall`/`@GuildRecall`/`@AllowGroupReCall`/`@AllowGuildReCall` **在现网一个都不存在**；现网拼写分别是 `@加入门派`/`@交易`/`@天地合一`/`@df56dfs`/`@允许天地合一`/`@sadasd`。
3. **现网权限恒 0**：`Permission` 的唯一加载点 `PlayObject.cs:1188-1194`（`GetHumPermission`）**被注释**；`AdminList.txt` 唯一一行 `*\t阿瑞斯乐园` 缺 IP 字段会被 `continue` 丢弃；`StartPermission=0`；部署是 **Release**（`!deployed-from.json` 的 `build_dir=...\bin\Release`，且 `CommandSystem.dll` 里没有 `#if DEBUG` 的「当前运行调试模式」串）。⇒ 185 条里**只有 6 条在现网可达**：`@时间`、`@拒绝`、`@拒绝私聊`、`@LockLogin`、`@AddRobotPlay`、`@help`；其余 179 条（含点名 6 条）全部卡在 `权限不够!!!`。
4. **这与 inventory A-06 的「GM/管理端可达」冲突**：不是「GM 能用的面没实现」，而是「oracle 现网自己就把 GM 面锁死了」。若要真机能验，必须先用 `SETPERMISSION` 脚本或改内容/改 AdminList 换窗口，属**内容或配置变更**，需要 owner 裁决（见 §9 D-1/D-2）。
5. **`@GuildRecall` 是死功能**：`GuildRecallCommand.cs` 的成员遍历整段被注释，`nRecallCount`/`nNoRecallCount` 恒 0，永远只回「已传送0个成员，0个成员未被传送。」再重置 `GroupRcallTick/GroupRcallTime`。
6. **配置面有 49 个死键**：`Command.conf` 的 `[Command]` 201 条里 49 条不被任何 `GameCmdConf` 行消费（`Recall=ch`、`ReGoto=gz`、`Level`、`Attack`、`NameColor`、`Transparecy`、`ViewDiary`…），另有 5 条**键名拼写不匹配**，导致这些命令在现网只能按英文出厂名调用。
7. **Rust 侧现状 = 入口存在、分流有意丢弃**：`bins/game-svr/src/chat.rs` 模块头明写「`@` 命令系统…未接」，`main.rs::handle_say` 对 `@` 行只记一条 `DBG` 日志后 `return`；`MsgColors` 只有 red/hear/whisper 三色，缺 green/blue。任何 `@` 命令的差异都表现为「无回帧」。
8. **落地建议**：按 6 个批次切（表驱动派发 + 6 条可达命令 → 会话开关族 → 传送/召唤族 → 数值/落库族 → 行会/城堡族 → 桩与死键），前两批 `tier=L`，传送/数值/行会三批 `tier=R`（RNG 抽数、落库字段、广播时序）。判据以「命令语义表 + SM_SYSMESSAGE 逐字节 + 真机 A/B」三层带牙（§7）。

---

## 1. `@` 分发链（逐行）

### 1.1 入口帧与分流

- 客户端 GM 输入走普通说话帧：`CM_SAY(3030)`（`src/OpenMir2/Messages.cs:147`）。
- `src/M2Server/Player/PlayObject.Message.cs:1394`：`case Messages.CM_SAY: ProcessUserLineMsg(processMsg.Msg);`
- `src/M2Server/Player/PlayObject.Chat.cs:363-367`：

  ```csharp
  if (!sData.StartsWith("@")) { ProcessSayMsg(sData); return; }
  ```

  即：**`@` 判定在 `ProcessSayMsg` 之前**，与 `/`、`!` 分流互斥；Rust 侧 `chat::is_plain_say` 已按此语义实现（`!line.starts_with('@')`）。
- `PlayObject.Chat.cs:368`：`if (M2Share.CommandSystem.Execute(this, sData)) return;` —— 命令「命中」即整条 `CM_SAY` 结束，不再回落喊话/私聊。
- 注意：`ProcessUserLineMsg` 前面还有密码锁状态机（`IsUnLockPwd`/`IsCheckOldPwd`/`IsSetStoragePwd` 等，`Chat.cs:229-361`），`@` 行只有在这些状态都不命中时才会进命令分支。**这是 Rust 侧第一批必须一起搬的前置条件**（否则 `@SetPassword` 流程会错）。

### 1.2 解析：`ExtractCommandAndParameters`（`CommandSystem.cs:228-250`）

```csharp
line = line.Trim();                    // .NET Trim：Unicode 空白（含 \t\r\n 与全角空格 U+3000）
if (string.IsNullOrEmpty(line)) return false;
if (line[0] != '@') return false;
line = line[1..];
command = line.Split(' ')[0];          // 只按 ASCII 空格切；不用 ':' ',' '\t'
parameters = string.Empty;
if (line.Contains(' '))
    parameters = line[(line.IndexOf(' ') + 1)..].Trim();
```

推论（都会进判据表）：

1. `@` 后紧跟空格 ⇒ `command == ""` ⇒ 未知命令。
2. `@a\tb` ⇒ command 是整串 `a\tb`（含制表符），**不是** `a`。空格是唯一分隔符。
3. `@时间 ` 尾部空格被 `Trim` 吃掉 ⇒ `parameters == ""`。
4. 参数内部连续空格**保留**：`Handle` 里 `parameters.Split(' ')`（`GameCommand.cs:46`）不去空 ⇒ `"a  b"` 得 `["a","","b"]`；对 `Make`/`Mob` 这类按下标取参的命令，空串就是「参数缺失」而不是「跳过」。
5. 无参数命令（`Execute(IPlayerActor)` / `Execute()`）根本看不到参数串（`Handle` 的 `case 1` / `default` 不传 `@params`）。
6. 命令名查找 `StringComparer.OrdinalIgnoreCase`（`CommandSystem.cs:16`）——对 GBK 解码后的中文是同一码点，等价于「中文名按字节精确匹配、ASCII 名大小写不敏感」。

### 1.3 注册：反射扫描 + 配置覆盖（`CommandSystem.cs:24-116`）

启动序：

1. `GameApp.cs:55`：`M2Share.CommandSystem = new GameCommandSystem();` → 构造即 `new GameCmdConf(Path.Combine(SystemShare.BasePath, ConfConst.CommandFileName))`（`CommandSystem.cs:21`）。`SystemShare.BasePath = AppContext.BaseDirectory`（`SystemShare.cs:185`）、`ConfConst.CommandFileName = "command.conf"`（`ConfConst.cs:7`）⇒ 实际读 **`M2GameSvr/Command.conf`**（Windows 大小写不敏感；`Mir200/Command.conf` 是同一份的副本，`diff` 为空）。
2. `GameApp.cs:104`：`RegisterCommand()` → `CommandConf.LoadConfig(GameCommands)`（先读配置，`GameCmdConf.cs:14-357`）→ `RegisterCustomCommand()`（`CommandSystem.cs:39-73`）→ `RegisterCommandGroups()`（`76-116`）。
3. `RegisterCustomCommand`：遍历 `GameCommands` 的字段；**只处理带 `[RegisterCommand(typeof(X))]` 的字段**，把 `X` 的 `[Command(...)]` 名字当 key，值为 `GameCmd{CmdName, PerMissionMin, PerMissionMax}`。
4. `RegisterCommandGroups`：`Assembly.GetExecutingAssembly().GetTypes().Where(x => x.IsSubclassOf(typeof(GameCommand)))` —— **所有带 `[Command]` 的类都会注册**（含 `internal class AuthallyCommand`），与是否在 `GameCommands` 里登记无关；登记只决定「名字/权限是否被 `Command.conf` 覆盖」。
5. 覆盖顺序（`CommandSystem.cs:100-107`）：若类名在 custom map 中，则 `attribute.Name = customCommand.CmdName`、`PermissionMin/Max = customCommand.PerMissionMin/Max`。
6. 必须有 `public Execute`（`GetMethod("Execute")`，`CommandSystem.cs:108`），否则 `LogService.Error` 并跳过。

**三层权限/名字来源**（这是最容易搞错的地方）：

| 层 | 来源 | 何时生效 | 例 |
|---|---|---|---|
| L1 类属性 | `[Command(name, desc, help, minUserLevel)]` | 只有**未**在 `GameCommands` 登记（无 `[RegisterCommand]`）的 25 条 | `RecallHuman` 用 10 |
| L2 a 构造默认 | `GameCommands` ctor `new GameCmd{CmdName=..., PerMissionMin=10, PerMissionMax=10}` | 已登记但 `command.conf` 缺键 | `LetTrade` 名字 `LetTrade`、权限 10 |
| L2 b 配置 | `GameCmdConf` 用固定 key 读 `[Command]`/`[Permission]` | 已登记且配置命中 | `LetTrade` → `交易`，权限仍 10 |
| L3 运行时 | `SetPermission` 命令 / `SETPERMISSION` 脚本 | 运行期改 `PlayerActor.Permission` | 现网无入口（见 §2） |

> `PermissionMax` 只在 4 个命令里被读（`Make`/`RecallHuman`/`Smake` 等「支持权限分配」的命令内部再比一次），`Handle` 的门只比 `PermissionMin`（`GameCommand.cs:37`）。

**内建两条**（`CommandSystem.cs:236-300`）：

- `@commands`（`CommandsCommandGroup`）：`[Command("commands","列出可用的命令")]`，`Execute()` 空、**未重写 `Handle`** ⇒ 走 `Handle` 的 `default: Invoke(this, new object[]{null, PlayerActor})`，而 `Execute()` 零形参 ⇒ 反射抛 `TargetParameterCountException`；被 `ProcessUserLineMsg` 的 `catch` 吞掉并记日志 ⇒ **玩家看不到任何回帧**。这条是「已注册但每次调用都抛异常」的活标本。
- `@help`（`HelpCommandGroup`）：重写 `Handle`，`parameters == ""` 回 `"usage: help <command>"`，否则回 `"Unknown command: {group} {command}"`；**不受权限门约束**（重写里没比 `Permission`）⇒ 现网可达。

### 1.4 权限门与派发（`GameCommand.cs:28-58`）

```csharp
if (PlayerActor != null) {
#if DEBUG
    PlayerActor.Permission = 10;                    // 仅 Debug 构建
    PlayerActor.SysMsg("当前运行调试模式,权限等级：10", Red, Hint);
#endif
    if (PlayerActor.Permission < Command.PermissionMin)
        return CommandHelp.GameCommandPermissionTooLow;   // "权限不够!!!"
}
switch (MethodParameterCount) {
    case 2: return CommandMethod.Invoke(this, new object[]{ parameters.Split(' '), PlayerActor });
    case 1: return CommandMethod.Invoke(this, new object[]{ PlayerActor });
    default: return CommandMethod.Invoke(this, new object[]{ null, PlayerActor });
}
```

- `PlayerActor == null` 的路径只有 `ExecCmd`（`CommandSystem.cs:198-218`），而 `ExecCmd` **全仓无调用点**（`grep` 只有接口声明 `ICommandSystem.cs:20`）⇒ 控制台命令面是死代码，设计上不需要为它留入口。
- `#if DEBUG` 会把权限强制拉满且多发一条红字提示。部署是 Release，所以这条**不在 parity 目标里**；但若有人用 Debug 构建做真机对拍，行为会差一个「红字提示 + 全部命令可用」——必须写进窗口检查表（见 §8 W-1）。

### 1.5 未知命令回落与 `@@!` 红字（`PlayObject.Chat.cs:368-400`）

```
Execute(@cmd) == false
  ├─ Permission >= 2 且 sData.Length > 2:
  │    sCMD = GetValidStr3(sData, [' ', ':', ',', '\t'])[0]   // 含前导 '@'
  │    └─ Permission >= 6 且 sData[2] == GMRedMsgCmd('!'):     // 即玩家输入 "@@!..."
  │         节流 2000ms（SayMsgTick）
  │         sData = sData[2..]                                 // "!正文"
  │         截断 SayRedMsgMaxLen（现网 40）
  │         广播文本 = ShutRedMsgShowGMName ? (ChrName+": "+sData) : sData
  │         SendBroadCastMsg(..., MsgType.GameManger)
  │         return
  └─ SysMsg($"@{sCMD}此命令不正确，或没有足够的权限!!!", Red, Hint)
```

要点：

- `Execute` 命中（含返回空串）就 `return true`，**不会**再走回落；因此「已知命令但参数错」只出命令自己的提示，不出「此命令不正确」。
- 未知命令在现网（Permission=0）出**一条**：`@此命令不正确，或没有足够的权限!!!`（`sCMD` 为空串）；若权限 ≥2 则是两条（先 `未知命令: X`，再 `@@X此命令不正确…`）。
- `@@!红字` 需要 `sData[2]=='!'`，即字面量 **`@@!`** 三字符前缀（`@!` 不成立：`sData[2]` 是正文首字符）。现网 `ShutRedMsgShowGMName=0`、`SayRedMsgMaxLen=40`、`ShowPreFixMsg=0` ⇒ 广播正文就是 `!` + 正文前 40 字符。但现网 Permission 恒 0，**这条也不可达**（`Permission >= 6` 门）。

### 1.6 帧与颜色

- `SysMsg(sMsg, msgColor, msgType)`（`BaseObject.cs:1588-1685`）默认支按 `MsgColor` 选色，再由 `SendMsg(RM_SYSMESSAGE, ...)` 转 `SM_SYSMESSAGE(100)`：
  - Green ⇒ `GreenMsgFColor/GreenMsgBColor` = **219/255**
  - Blue ⇒ `BlueMsgFColor/BlueMsgBColor` = **251/252**
  - 其它（含 Red、`MsgType.Cust` 以外）⇒ `RedMsgFColor/RedMsgBColor` = **255/56**
  （数值取自部署 `Mir200/Server.conf:600-605`；`MsgType.Notice` 另走 `RM_MOVEMESSAGE`/`RM_SYSMESSAGE` 且解析 `[...]`/`<...>` 前缀，GM 命令都不用。）
- `SendBroadCastMsg`（`WorldServer.cs:1738-1778`）对每个非 Ghost 玩家 `SysMsg(sMsg, Red, msgType)` ⇒ 红字广播仍是 `SM_SYSMESSAGE`，只是逐人发；`ShowPreFixMsg=0` ⇒ 不加 `〖ＧＭ〗` 前缀。
- 传送类副作用帧：`RecallHuman`（`PlayObject.Base.cs:3321-3347`）= `GetFrontPosition` + `GetRecallXy`（RNG）+ `RM_SPACEMOVE_FIRE` + `SpaceMove`；`SpaceMove` 自身再发 `SM_SPACEMOVE` 系。**凡走 Recall/SpaceMove 的命令都是 R 级**（RNG 抽数 + 坐标时序）。
- 密码类：`SendMsg(RM_PASSWORD)` → `SendDefMessage(SM_PASSWORD)`（`PlayObject.Message.cs:2031-2033`）。

---

## 2. 权限来源与现网可达性（关键更正）

### 2.1 `Permission` 的唯一来源

- 字段：`PlayObject.cs:58` `public byte Permission { get; set; }`，默认 **0**。
- 赋值点共 4 处：
  1. `WorldServer.cs:1021-1037` `GetHumPermission`：查 `AdminList`，缺省 `Config.StartPermission`；
  2. `SetPermissionCommand.cs:40`：GM 命令，自身门 10（现网不可达）；
  3. `ExecutionProcessingSys.cs:5622-5625`：脚本 `SETPERMISSION`；
  4. `GameCommand.cs:34`：`#if DEBUG` 强制 10。
- **`GetHumPermission` 的唯一调用点被注释**：`PlayObject.cs:1188-1194`

  ```csharp
  //if (SystemShare.WorldEngine.GetHumPermission(ChrName, ref sIPaddr, ref Permission))
  //{
  //    if (SystemShare.Config.PermissionSystem) { ...IP 校验... }
  //}
  ```

  且 `PermissionSystem` 默认 false（`GameSvrConf.cs:1898`）。

### 2.2 `AdminList.txt` 解析（`LocalDB.cs:16-95`）

- 行首标记映射：`*`→10、`1`→9、`2`→8、`3`→7、`4`→6、`5`→5、`6`→4、`7`→3、`8`→2、`9`→1；其余跳过。
- 之后连续三次 `GetValidStrCap`（= `GetValidStr3`，`Split(div, 2, RemoveEmptyEntries)`）取「标记 / 角色名 / IP」；**`ChrName` 或 `IPaddr` 任一为空即 `continue`**（`LocalDB.cs:80-83`）。
- 部署文件 `Mir200/Envir/AdminList.txt` 的原始字节是 `2a 09 b0a2 c8f0 cbb9 c0d6 d4b0 0d0a`（`*\t阿瑞斯乐园\r\n`），**只有两段**：第三次 `GetValidStrCap` 返回空 ⇒ `sIPaddr == ""` ⇒ `continue` ⇒ **管理员表最终为空**。
- 因此即使把 `GetHumPermission` 的注释打开，现网管理员表也是空的——两层锁。

### 2.3 部署构建与配置

- `M2GameSvr/!deployed-from.json`：`build_dir = ...\src\GameSrv\bin\Release`，`git_head=8a621f6a…`（2026-10-20 00:19）。
- `strings -el CommandSystem.dll` 里**没有**「当前运行调试模式,权限等级：10」⇒ `#if DEBUG` 分支未被编译。
- `Mir200/Server.conf:365` `StartPermission=0`。

### 2.4 结论：现网可达命令 = 6 条

| # | 现网拼写 | 权限 | 行为 | 帧 |
|---|---|---:|---|---|
| 1 | `@时间` | 0 | `Date`：回 `当前日期时间: yyyy-MM-dd HH:mm:ss`（服务器本地时间） | SM_SYSMESSAGE Blue 251/252 |
| 2 | `@拒绝` | 0 | `PrvMsg`：把名字加入 `LockWhisperList`（去重分支被注释，**只加不删**） | Green 219/255 |
| 3 | `@拒绝私聊` | 0 | `AllowMsg`：`HearWhisper = !HearWhisper`（默认 true） | Green 219/255 |
| 4 | `@LockLogin` | 0 | `LockLogin`：`Config.LockHumanLogin` 未开则红字拒绝；否则 toggle `IsLockLogon` | Green / Red |
| 5 | `@AddRobotPlay` | 0 | `CreateRobotPlay`：校验后**全段被注释**（真正的加机器人没了）；`?` 出帮助 | 仅 `?` 时 Red |
| 6 | `@help` | 0 | 内建；重写 `Handle` 不过权限门 | Red 255/56 |

其余 **179 条**在现网只能得到 `权限不够!!!`（Red 255/56）。**audit #17 点名的 6 条全部在这 179 条里。**

### 2.5 与 inventory A-06 的冲突（必须显式记录）

`docs/m3-remaining-inventory-20261022.md` A-06 的「可达性」写「普通客户端不可达，GM/管理端可达」。按 §2.1-2.4，**「GM 端」也不可达**：不存在任何生产路径能把 `Permission` 抬到 >0。差异原因是 inventory 依据「命令面存在且权限表为 10」推断可达，没有追 `Permission` 的赋值链。

这不改变「该不该实现」的结论（Rust 现在连 `@` 分发都没有，任何一条都答不上），但改变**判据设计**：真机 A/B 要么选 6 条可达命令，要么在窗口内用**一次性脚本 `SETPERMISSION`**（内容侧写入，需 owner 批准）构造权限，不能假装 GM 命令天然可测。

---

## 3. 现网命令清点（`Command.conf`）

### 3.1 文件、编码与读取

- 读取路径：`M2GameSvr/Command.conf`（§1.3）。`ConfigFile.Load`（`src/OpenMir2/Common/ConfigFile.cs:245-...`）用 **GB2312** 打开，`section`/`key` 都是 `OrdinalIgnoreCase`；重复 section 整段跳过。
- `ReadWriteString/Byte` 在键缺失时把默认值写进**内存**缓存（`ConfigFile.cs:103-110`、`419-480`），但 `GameCmdConf` 从未调用 `Save()` ⇒ **不落盘**，部署文件不会被启动过程改写。
- 部署文件与 `Mir200/Command.conf` 逐字节相同（`diff -q` 无输出）。

### 3.2 键统计

| 项 | 数量 | 说明 |
|---|---:|---|
| `[Command]` 条目 | 201 | 含 3 条非命令键（`表情`/`留言`/`招募队员`）与 `GMRedMsgCmd` |
| `[Permission]` 条目 | 171 | `GMRedMsgCmd=6` 是红字命令权限 |
| 代码消费的 `[Command]` 键 | 156 个互异键 + `GMRedMsgCmd`（专用行，不走 `commands.*`） | `GameCmdConf.cs` 的 `ReadWriteString` 行 |
| 代码消费的 `[Permission]` 键 | 123 个互异键 + `GMRedMsgCmd` | `ReadWriteByte` 行 |
| **死 `[Command]` 键** | **49** | 见 §3.3 |
| **死 `[Permission]` 键** | **52** | 含 `MakeMax`/`MoveMax`/`MoveMin` 等历史键 |
| 代码要用但部署缺失的键 | 5 | `ChangeMasterName`、`DelBonusPoint`、`DenyChrNameLogon`、`DelDenyChrNameLogon`、`ShowDenyChrNameLogon` |

**死 `[Command]` 键（49）**：
`AddFriend, AddToItemEvent, AddToItemEventAspieces, AdjuestTestLevel, AllowFarMsg, Attack, Ball, ChangeLuck, ChangeMasterrName, ChangeNGLevel, ChangeWeaponDura, DelBonuPoint, DelDenyCharNameLogon, DelFriend, DeleteAllItemEvent, DenyCharNameLogon, Friend, GameGoldEx, GameWen, Gsa, InitSabuk, ItemEventList, ItemEventTerm, Level, NameColor, OXQuizRoom, OpDeleteSkill, ReGoto, Recall, ReloadDiary, ReloadItemDB, ReloadMonsterDB, ReloadScript, RemoteMsg, RemoteMusic, RestBonuPoint, ShowDenyCharNameLogon, SignMove, StartIngGiftNO, StartItemEvent, TakeUserItem, TestGa, TestGoldChange, Transparecy, UnLockLogin, ViewDiary, 招募队员, 留言, 表情`

其中最能误导人的几条：

- `Recall=ch`、`ReGoto=gz`：`GameCommands` 里 `Recall`/`Regoto` 两个 `GameCmd` 字段**没有 `[RegisterCommand]`**，而真正实现它们的 `RecallHumanCommand`/`ReGotoHumanCommand` 也不在 `GameCommands` 里 ⇒ 现网**没有** `@ch`/`@gz`，只有英文 `@RecallHuman`/`@ReGotoHuman`（且权限 10）。
- `Level=j`：`ChangeLevelCommand` 登记到 `Level` 字段，但 `GameCmdConf` 读的是 key `Level0`（`Command.conf` 里 `Level0=Level0`）⇒ `@j` 是死名，现网是 `@Level0`。
- `ViewDiary`/`ReloadDiary`/`ReloadItemDB`/`ReloadMonsterDB`/`ReloadScript`/`Gsa`/`OXQuizRoom`/`Attack`/`NameColor`/`Transparecy`：有配置名，但**没有任何命令类消费**。

### 3.3 键名拼写漂移（5 条，直接影响现网拼写）

| 类 / 出厂名 | 代码读的键 | 部署里的键 | 后果 |
|---|---|---|---|
| `ChangeMasterNameCommand` / `ChangeMasterName` | `ChangeMasterName` | `ChangeMasterrName`（多个 r） | 部署名「MasterName」不生效，现网 `@ChangeMasterName` |
| `RestBonuPointCommand` / `RestBonuPoint` | `DelBonusPoint` | `DelBonuPoint` | 部署名「还原属性」不生效（且该类未登记，权限仍是 10） |
| `DenyCharNameLogonCommand` / `DenyChrNameLogon` | `DenyChrNameLogon` | `DenyCharNameLogon` | 部署名 `+jww` 不生效，现网 `@DenyChrNameLogon` |
| `DelDenyCharNameLogonCommand` / `DelDenyChrNameLogon` | `DelDenyChrNameLogon` | `DelDenyCharNameLogon` | 部署名 `-jww` 不生效 |
| `ShowDenyCharNameLogonCommand` / `ShowDenyChrNameLogon` | `ShowDenyChrNameLogon` | `ShowDenyCharNameLogon` | 部署名 `+-jww` 不生效 |

### 3.4 audit 点名 6 条的实际拼写（`Command.conf:12/14/30-33`）

| 代码类 | 出厂名 | 配置键 | 现网拼写（GBK 中文/混淆串） | 权限 | 生效门 |
|---|---|---|---|---:|---|
| `LetGuildCommand` | `Letguild` | `LetGuild` | **`加入门派`** | 10 | `Permission >= 10` |
| `LetTradeCommand` | `Lettrade` | `LetTrade` | **`交易`** | 10 | 同上 |
| `GroupRecallCommand` | `GroupRecall` | `GroupCall` | **`天地合一`** | 10 | 同上（内部还要 `RecallSuite || Permission>=6`） |
| `GuildRecallCommand` | `GuildRecall` | `GuildReCall` | **`df56dfs`** | 10 | 同上（内部 `GuildMove || Permission>=6` + 掌门人） |
| `AllowGroupReCallCommand` | `AllowGroupReCall` | `AllowGroupCall` | **`允许天地合一`** | 10 | 同上 |
| `AllowGuildRecallCommand` | `Allowguildrecall`（小写！） | `AllowGuildReCall` | **`sadasd`** | 10 | 同上 |

> `AllowGuildRecallCommand` 的类名与 `[Command]` 名都是小写 `Allowguildrecall`，只靠 `OrdinalIgnoreCase` 才能和配置键 / `GameCommands` 字段对齐；Rust 侧如果用 `String` 精确匹配会**静默失效**（这是第一批的必测点）。

### 3.5 只能按英文出厂名调用的 25 条（未登记 `[RegisterCommand]`）

`ClearCopyItem, ClearItemMap, AddRobotPlay, GameDiaMond, GameGird, GameGlory, GetUserItems, GiveMine, GuildWar, LoadAdmin, MobFireBurn, MoveMobTo, ReCallMobEx, ReGotoHuman, RecallHuman, ReloadItemDB, RestBonuPoint, ServerInfo, ShowEffect, SignMapMove, TestGetBagItems, TestSpeedMode, TrainingMagic, UnBindUseItem, UserCmd`

这 25 条的名字/权限取类属性（L1），`Command.conf` 对它们的任何条目都无效。

---

## 4. 逐命令解剖

### 4.1 audit 点名 6 条（输入 / 校验 / 副作用 / 帧）

#### 4.1.1 `@加入门派`（`LetGuildCommand.cs`，出厂 `@Letguild`）

- 输入：无（`Execute(IPlayerActor)`；命令行参数被丢弃）。
- 校验：`Handle` 的 `Permission < 10` 门；命令体无其它校验。
- 副作用：`PlayerActor.AllowGuild = !AllowGuild`。这是 `AllowGuild` 的**唯一生产写点**（`PlayObject.cs:110`，ctor `false`，不入库）；消费点 `PlayObject.Operate.cs:1317`（加入行会请求门）。
- 帧：Green `SM_SYSMESSAGE`，`[允许加入行会]` / `[禁止加入行会]`。
- 现网：不可达（门 10）。Rust 现状：`guild_cmd.rs:31` 明写 `allow_guild` 恒 false，正是缺这条。

#### 4.1.2 `@交易`（`LetTradeCommand.cs`，出厂 `@Lettrade`）

- 输入：无。校验：权限门 10。
- 副作用：`AllowDeal = !AllowDeal`（ctor `true`，`PlayObject.cs:1008`，不入库）；消费点 `PlayObject.Operate.cs:940`（对方发起交易时看 `targetPlayObject.AllowDeal && IsCanDeal`）。
- 帧：Green，`[允许交易]` / `[禁止交易]`。
- 现网：不可达。

#### 4.1.3 `@天地合一`（`GroupRecallCommand.cs`，出厂 `@GroupRecall`）

- 输入：无。校验序列：
  1. 权限门 10；
  2. 命令体：`RecallSuite || Permission >= 6`，否则红字「您现在还无法使用此功能!!!」。`RecallSuite` 由「记忆套装四件」置真（`PlayObject.cs:2525-2531`），普通玩家也可能满足；
  3. 冷却：`GroupRcallTick`（毫秒）+ `GroupRcallTime`（秒，配置 `GroupRecallTime`，代码默认 180，**部署实读 5**，`Server.conf:719`；**入库**）；用 `(now-tick)/1000` 的整秒差扣减，`Permission>=6` 时直接清零；
  4. `GroupOwner == ActorId`（必须是组长）。
- 副作用：遍历 `GroupMembers`，逐个判 `AllowGroupReCall` 与 `member.Envir.Flag.NoReCall`：通过的 `PlayerActor.RecallHuman(name)`（被召唤者 `RM_SPACEMOVE_FIRE` + `SpaceMove`，**RNG**）；不通过的发红字。结束重置 `GroupRcallTick = now`、`GroupRcallTime = Config.GroupRecallTime`。
- 帧：N 条红/绿 `SM_SYSMESSAGE` + 每个成功成员的传送帧。
- 现网：不可达（权限门 10 先挡）。tier：R（RNG + 成员遍历顺序 + 冷却时序）。

#### 4.1.4 `@df56dfs`（`GuildRecallCommand.cs`，出厂 `@GuildRecall`）

- 输入：无。校验：
  1. 权限门 10；
  2. `!GuildMove && Permission < 6` ⇒ 红字「您现在还无法使用此功能!!!」（`GuildMove` **无任何写点**，恒 false）；
  3. `IsGuildMaster()`，否则「行会掌门人才可以使用此功能!!!」；
  4. `Envir.Flag.NoGuildReCall` ⇒ 「本地图不允许使用此功能!!!」；
  5. `CastleMgr.InCastleWarArea(this)?.UnderWar` ⇒ 「攻城区域不允许使用此功能!!!」；
  6. 冷却同 4.1.3（`GuildRecallTime`，代码默认 180，**部署实读 600**，`Server.conf:718`）。
- 副作用：**空**。整段行会成员遍历（原 `RankList`/`MemberList` → `RecallHuman`）被**注释掉**（源码 62-112 行），`nRecallCount`/`nNoRecallCount` 恒 0；仍会重置 tick 与冷却。
- 帧：Green `已传送0个成员，0个成员未被传送。`
- 现网：不可达（门 10）。tier：L（纯文本 + 计时器；若将来恢复成员遍历则升 R）。

#### 4.1.5 `@允许天地合一`（`AllowGroupReCallCommand.cs`）

- 输入：无。校验：权限门 10。
- 副作用：`AllowGroupReCall = !AllowGroupReCall`（**入库**：`WorldServer.cs:1156-1159` 载入、`:1305-1308` 存回，DB 列 `characters.AllowGroupReCall`；Rust `playerdata.rs:668` 已有）。
- 帧：Green，`[允许天地合一]` / `[禁止天地合一]`。
- 现网：不可达。tier：**R**（落库字段）。

#### 4.1.6 `@sadasd`（`AllowGuildRecallCommand.cs`，出厂 `@Allowguildrecall`）

- 输入：无。校验：权限门 10。
- 副作用：`AllowGuildReCall = !AllowGuildReCall`（**入库**，列 `characters.AllowGuildReCall`；Rust `playerdata.rs:665` 已有）。
- 帧：Green，`[允许行会合一]` / `[禁止行会合一]`。
- 现网：不可达。tier：**R**（落库字段）。

### 4.2 现网可达 6 条（§2.4）的判据要点

| 命令 | 判据 |
|---|---|
| `@时间` | 正文前缀 `当前日期时间: ` 固定，时间取服务器本地时钟（真机 A/B 只比格式与颜色，不比秒） |
| `@拒绝` | `LockWhisperList.Add(name)`，重复调用不去重、不删除（去重分支被注释）；回 `{0} 已被加入禁止私聊列表...` |
| `@拒绝私聊` | toggle `HearWhisper`（私有字段，会话内），回 `[禁止私聊]`/`[允许私聊]` |
| `@LockLogin` | `LockHumanLogin` 读键 `PasswordLockHumanLogin`（`ServerConf.cs:574`），**部署=0** ⇒ 恒回红字「本服务器还没有启用登录锁功能!!!」，`IsLockLogon` 永不变 |
| `@AddRobotPlay` | 带 `?` 回帮助；其余参数只做校验，**无任何副作用**（加机器人代码被注释） |
| `@help` | `""`→`usage: help <command>`；`x y`→`Unknown command: x y`；重写 `Handle`，不过权限门 |

### 4.3 全量 185 条清点表

> 列说明：**现网名** = 部署 `Command.conf` 生效后的实际拼写（未登记的 25 条等于出厂名）；**权限** = 生效 `PermissionMin`；**参数** = `Execute` 形参个数（2 = `string[]` + actor，1 = 仅 actor）；**效应** = 启发式扫描标记：`N` 桩（无可观察副作用）/`S` 仅自身 SysMsg /`F` 额外帧 /`C` 改其它在线角色 /`W` 改世界或自身状态 /`B` 广播 /`D` 落盘；**帧** = 主要下行帧。
> 精确校验与副作用请回源到 §4.4 的族说明与对应 `.cs` 文件；本表只用于批次写集切分与覆盖度核对。

| # | 族 | 代码类 | 现网名 | 出厂名 | 权限 | 参数 | 效应 | 帧 |
|---:|---|---|---|---|---:|---:|---|---|
| 1 | 仓库/登录密码 | `ClearHumanPasswordCommand` | `清除仓库密码` | `ClearHumanPassword` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 2 | 仓库/登录密码 | `LockCommand` | `fdgfdg56` | `Lock` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 3 | 仓库/登录密码 | `PasswordLockCommand` | `PasswordLock` | `PasswordLock` | 10 | 2 | F+S | SM_SYSMESSAGE |
| 4 | 仓库/登录密码 | `SetPassWordCommand` | `hjhgj67` | `SetPassWord` | 10 | 2 | F+C+S | SM_SYSMESSAGE |
| 5 | 仓库/登录密码 | `UnLockCommand` | `gfh678dfg` | `UnLock` | 10 | 2 | F+C+S | SM_SYSMESSAGE |
| 6 | 仓库/登录密码 | `UnlockStorageCommand` | `hjhg556` | `UnlockStorage` | 10 | 2 | F+C+S | SM_SYSMESSAGE |
| 7 | 会话控制 | `KickHumanCommand` | `VV踢人` | `KickHuman` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 8 | 会话控制 | `KillCommand` | `vv杀死` | `Kill` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 9 | 会话控制 | `ReAliveCommand` | `复活` | `ReAlive` | 10 | 2 | F+C+S | SM_SYSMESSAGE |
| 10 | 会话控制 | `ReconnectionCommand` | `Reconnection` | `Reconnection` | 10 | 2 | F+S | SM_SYSMESSAGE |
| 11 | 传送/召唤 | `AllowDearRecallCommand` | `dfsgf567` | `AllowDearRecall` | 10 | 2 | S | SM_SYSMESSAGE |
| 12 | 传送/召唤 | `AllowGuildRecallCommand` | `sadasd` | `Allowguildrecall` | 10 | 1 | S | SM_SYSMESSAGE |
| 13 | 传送/召唤 | `AllowMasterRecallCommand` | `fdgh56j` | `AllowMasterRecall` | 10 | 1 | S | SM_SYSMESSAGE |
| 14 | 传送/召唤 | `DearRecallCommond` | `dsfsdt546` | `DearRecall` | 10 | 1 | C+S | SM_SYSMESSAGE |
| 15 | 传送/召唤 | `GroupRecallCommand` | `天地合一` | `GroupRecall` | 10 | 1 | C+S | SM_SYSMESSAGE |
| 16 | 传送/召唤 | `GuildRecallCommand` | `df56dfs` | `GuildRecall` | 10 | 1 | S | SM_SYSMESSAGE |
| 17 | 传送/召唤 | `MapMoveCommand` | `f` | `MapMove` | 10 | 2 | F+S | SM_SYSMESSAGE,SM_* 回显 |
| 18 | 传送/召唤 | `MapMoveHumanCommand` | `集体传送` | `MapMoveHuman` | 10 | 2 | S | SM_SYSMESSAGE |
| 19 | 传送/召唤 | `MasterRecallCommand` | `jkt76` | `MasterRecall` | 10 | 1 | C+S | SM_SYSMESSAGE |
| 20 | 传送/召唤 | `MoveMobToCommand` | `MoveMobTo` | `MoveMobTo` | 10 | 2 | S | SM_SYSMESSAGE |
| 21 | 传送/召唤 | `PositionMoveCommand` | `ff` | `PositionMove` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 22 | 传送/召唤 | `RecallHumanCommand` | `RecallHuman` | `RecallHuman` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 23 | 传送/召唤 | `RecallMobCommand` | `bb` | `RecallMob` | 10 | 2 | S | SM_SYSMESSAGE |
| 24 | 传送/召唤 | `ReGotoHumanCommand` | `ReGotoHuman` | `ReGotoHuman` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 25 | 传送/召唤 | `SignMapMoveCommand` | `SignMapMove` | `SignMapMove` | 10 | 1 | N | — |
| 26 | 传送/召唤 | `SuperTingCommand` | `SuperTingwa` | `SuperTing` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 27 | 传送/召唤 | `TingCommand` | `Tingwa` | `Ting` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 28 | 其它 | `AllowGroupReCallCommand` | `允许天地合一` | `AllowGroupReCall` | 10 | 1 | S | SM_SYSMESSAGE |
| 29 | 其它 | `AllowMsgCommand` | `拒绝私聊` | `Allowmsg` | 0 | 1 | S | SM_SYSMESSAGE |
| 30 | 其它 | `BanGuildChatCommand` | `禁止行会信息` | `Banguildchat` | 10 | 1 | S | SM_SYSMESSAGE |
| 31 | 其它 | `ChgpassWordCommand` | `ghjhg6` | `ChgpassWordCommand` | 10 | 2 | F+C+S | SM_SYSMESSAGE |
| 32 | 其它 | `ClearBagItemCommand` | `清理垃圾` | `ClearBagItem` | 10 | 2 | F+C+S | SM_SYSMESSAGE |
| 33 | 其它 | `ClearCopyItemCommand` | `ClearCopyItem` | `ClearCopyItem` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 34 | 其它 | `ClearItemMapCommand` | `ClearItemMap` | `ClearItemMap` | 10 | 2 | N | — |
| 35 | 其它 | `ClearMapMonsterCommand` | `清除怪物` | `ClearMapMonster` | 10 | 2 | S | SM_SYSMESSAGE |
| 36 | 其它 | `ClearMissionCommand` | `清除任务` | `ClearMission` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 37 | 其它 | `DataCommand` | `时间` | `Date` | 0 | 1 | S | SM_SYSMESSAGE |
| 38 | 其它 | `DelSkillCommand` | `删除技能` | `DelSkill` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 39 | 其它 | `DisableFilterCommand` | `DisableFilter` | `DisableFilter` | 10 | 1 | S | SM_SYSMESSAGE |
| 40 | 其它 | `LetGuildCommand` | `加入门派` | `Letguild` | 10 | 1 | S | SM_SYSMESSAGE |
| 41 | 其它 | `LetShoutCommand` | `拒绝组队聊天` | `Letshout` | 10 | 1 | S | SM_SYSMESSAGE |
| 42 | 其它 | `LetTradeCommand` | `交易` | `Lettrade` | 10 | 1 | S | SM_SYSMESSAGE |
| 43 | 其它 | `LockLoginCommand` | `LockLogin` | `LockLogin` | 0 | 1 | S | SM_SYSMESSAGE |
| 44 | 其它 | `LotteryTicketCommandL` | `LotteryTicket` | `LotteryTicket` | 10 | 1 | S | SM_SYSMESSAGE |
| 45 | 其它 | `MakeItemCommond` | `s` | `Make` | 10 | 2 | S | SM_SYSMESSAGE |
| 46 | 其它 | `MemberFunctionCommand` | `是我` | `MemberFunction` | 10 | 1 | N | — |
| 47 | 其它 | `MemberFunctionExCommand` | `MemberFuncEx` | `MemberFunctionEx` | 10 | 1 | N | — |
| 48 | 其它 | `PrvMsgCommand` | `拒绝` | `PrvMsg` | 0 | 2 | S | SM_SYSMESSAGE |
| 49 | 其它 | `SearchDearCommand` | `sdasdert` | `SearchDear` | 10 | 1 | S | SM_SYSMESSAGE |
| 50 | 其它 | `SearchHumanCommand` | `探测` | `SearchHuman` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 51 | 其它 | `SearchMasterCommand` | `fdht7ed` | `SearchMaster` | 10 | 1 | S | SM_SYSMESSAGE |
| 52 | 其它 | `SetFlagCommand` | `setflag` | `SetFlag` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 53 | 其它 | `SetMapModeCommamd` | `SetMapMode` | `SetMapMode` | 10 | 2 | N | — |
| 54 | 其它 | `SetOpenCommand` | `setopen` | `SetOpen` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 55 | 其它 | `SetPermissionCommand` | `VV给予权限` | `SetPermission` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 56 | 其它 | `SetUnitCommand` | `setunit` | `SetUnit` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 57 | 其它 | `SpirtStartCommand` | `祈祷生效` | `SpirtStart` | 10 | 2 | S | SM_SYSMESSAGE |
| 58 | 其它 | `SpirtStopCommand` | `停止叛变` | `SpirtStop` | 10 | 1 | S | SM_SYSMESSAGE |
| 59 | 其它 | `TakeOffHorseCommand` | `OffHorse` | `TakeOffHorse` | 10 | 1 | N | — |
| 60 | 其它 | `TakeOnHorseCommand` | `OnHorse` | `TakeOnHorse` | 10 | 1 | S | SM_SYSMESSAGE |
| 61 | 其它 | `UnPasswWordCommand` | `sadsa54` | `UnPassWord` | 10 | 2 | F+C+S | SM_SYSMESSAGE |
| 62 | 其它 | `UserMoveXyCommand` | `传送` | `UserMoveXY` | 10 | 2 | F+C+S | SM_SYSMESSAGE,SM_* 回显 |
| 63 | 封禁名单 | `DelDenyAccountLogonCommand` | `-jid` | `DelDenyAccountLogon` | 10 | 2 | S | SM_SYSMESSAGE |
| 64 | 封禁名单 | `DelDenyChrNameLogonCommand` | `DelDenyChrNameLogon` | `DelDenyChrNameLogon` | 10 | 2 | S | SM_SYSMESSAGE |
| 65 | 封禁名单 | `DelDenyIPaddrLogonCommand` | `-jip` | `DelDenyIPaddrLogon` | 10 | 2 | S | SM_SYSMESSAGE |
| 66 | 封禁名单 | `DenyAccountLogonCommand` | `+jid` | `DenyAccountLogon` | 10 | 2 | S | SM_SYSMESSAGE |
| 67 | 封禁名单 | `DenyChrNameLogonCommand` | `DenyChrNameLogon` | `DenyChrNameLogon` | 10 | 2 | S | SM_SYSMESSAGE |
| 68 | 封禁名单 | `DenyIPaddrLogonCommand` | `+jip` | `DenyIPaddrLogon` | 10 | 2 | S | SM_SYSMESSAGE |
| 69 | 封禁名单 | `ShowDenyAccountLogonCommand` | `+-jid` | `ShowDenyAccountLogon` | 10 | 1 | S | SM_SYSMESSAGE |
| 70 | 封禁名单 | `ShowDenyChrNameLogonCommand` | `ShowDenyCharNameLogon` | `ShowDenyChrNameLogon` | 10 | 1 | S | SM_SYSMESSAGE |
| 71 | 封禁名单 | `ShowDenyIPaddrLogonCommand` | `+-jip` | `ShowDenyIPaddrLogon` | 10 | 1 | S | SM_SYSMESSAGE |
| 72 | 数值/角色 | `AddGameGoldCommand` | `AddGamePoint` | `AddGameGold` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 73 | 数值/角色 | `AddGoldCommand` | `加钱` | `AddGold` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 74 | 数值/角色 | `AdjuestExpCommand` | `调整经验` | `AdjuestExp` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 75 | 数值/角色 | `AdjuestLevelCommand` | `1级` | `AdjuestLevel` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 76 | 数值/角色 | `AdjustExpCommand` | `AdjustExp` | `AdjustExp` | 10 | 2 | N | — |
| 77 | 数值/角色 | `BindUseItemCommand` | `BindUseItem` | `BindUseItem` | 10 | 2 | F+C+S | SM_SYSMESSAGE |
| 78 | 数值/角色 | `BonuPointCommand` | `调整属性` | `BonuPoint` | 10 | 2 | F+C+S | SM_SYSMESSAGE |
| 79 | 数值/角色 | `ChangeAdminModeCommand` | `1` | `ChangeAdminMode` | 10 | 1 | S | SM_SYSMESSAGE |
| 80 | 数值/角色 | `ChangeAttackModeCommand` | `AttackMode` | `AttackMode` | 10 | 1 | S | SM_SYSMESSAGE |
| 81 | 数值/角色 | `ChangeDearNameCommand` | `伴侣变更` | `ChangeDearName` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 82 | 数值/角色 | `ChangeGenderCommand` | `改变性别` | `ChangeGender` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 83 | 数值/角色 | `ChangeItemNameCommand` | `ChangeItemName` | `ChangeItemName` | 10 | 2 | S | SM_SYSMESSAGE |
| 84 | 数值/角色 | `ChangeJobCommand` | `职业变更` | `ChangeJob` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 85 | 数值/角色 | `ChangeLevelCommand` | `Level0` | `ChangeLevel` | 10 | 2 | N | — |
| 86 | 数值/角色 | `ChangeMasterNameCommand` | `MasterName` | `ChangeMasterName` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 87 | 数值/角色 | `ChangeObModeCommand` | `2` | `ChangeObMode` | 10 | 1 | F+S | SM_SYSMESSAGE,SM_* 回显 |
| 88 | 数值/角色 | `ChangeSalveStatusCommand` | `Rest` | `Rest` | 10 | 1 | S | SM_SYSMESSAGE |
| 89 | 数值/角色 | `ChangeSuperManModeCommand` | `3` | `ChangeSuperManMode` | 10 | 1 | S | SM_SYSMESSAGE |
| 90 | 数值/角色 | `ChangeUserFullCommand` | `CHGUSERFULL` | `ChangeUserFull` | 10 | 2 | S | SM_SYSMESSAGE |
| 91 | 数值/角色 | `ChangeZenFastStepCommand` | `CHGZENFASTSTEP` | `ChangeZenFastStep` | 10 | 2 | S | SM_SYSMESSAGE |
| 92 | 数值/角色 | `CreditPointCommand` | `调整声望` | `CreditPoint` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 93 | 数值/角色 | `DelBonuPointCommand` | `DelBonusPoint` | `DelBonuPoint` | 10 | 2 | F+C+S | SM_SYSMESSAGE |
| 94 | 数值/角色 | `DeleteItemCommand` | `删除物品` | `DeleteItem` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 95 | 数值/角色 | `DelGameGoldCommand` | `DelGamePoint` | `DelGameGold` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 96 | 数值/角色 | `DelGoldCommand` | `减钱` | `DelGold` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 97 | 数值/角色 | `FreePenaltyCommand` | `清除PK` | `FreePenalty` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 98 | 数值/角色 | `GameDiaMondCommand` | `GameDiaMond` | `GameDiaMond` | 10 | 2 | N | — |
| 99 | 数值/角色 | `GameGirdCommand` | `GameGird` | `GameGird` | 10 | 2 | N | — |
| 100 | 数值/角色 | `GameGloryCommand` | `GameGlory` | `GameGlory` | 10 | 2 | N | — |
| 101 | 数值/角色 | `GameGoldCommand` | `VV调整元宝` | `GameGold` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 102 | 数值/角色 | `GamePointCommand` | `调整泡点` | `GamePoint` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 103 | 数值/角色 | `GetUserItemsCommand` | `GetUserItems` | `GetUserItems` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 104 | 数值/角色 | `HairCommand` | `改变发型` | `Hair` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 105 | 数值/角色 | `HungerCommand` | `Hunger` | `Hunger` | 10 | 2 | F+C+S | SM_SYSMESSAGE |
| 106 | 数值/角色 | `IncPkPointCommand` | `增加PK` | `IncPkPoint` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 107 | 数值/角色 | `LuckPointCommand` | `LuckyPoint` | `LuckPoint` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 108 | 数值/角色 | `PKpointCommand` | `查看PK` | `PKpoint` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 109 | 数值/角色 | `RefineWeaponCommand` | `RefineWeaponwa` | `RefineWeapon` | 10 | 2 | F | — |
| 110 | 数值/角色 | `ReNewLevelCommand` | `转生` | `ReNewLevel` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 111 | 数值/角色 | `RestBonuPointCommand` | `RestBonuPoint` | `RestBonuPoint` | 10 | 2 | F+C+S | SM_SYSMESSAGE |
| 112 | 数值/角色 | `TrainingCommand` | `+jn` | `Training` | 10 | 1 | N | — |
| 113 | 数值/角色 | `TrainingMagicCommand` | `TrainingMagic` | `TrainingMagic` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 114 | 数值/角色 | `TrainingSkillCommand` | `增加技能` | `TrainingSkill` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 115 | 数值/角色 | `UnBindUseItemCommand` | `UnBindUseItem` | `UnBindUseItem` | 10 | 2 | N | — |
| 116 | 查询/展示 | `HumanCountCommand` | `HumanCountwa` | `HumanCount` | 10 | 2 | S | SM_SYSMESSAGE |
| 117 | 查询/展示 | `HumanInfoCommand` | `信息` | `HumanInfo` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 118 | 查询/展示 | `HumanLocalCommand` | `IP` | `HumanLocal` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 119 | 查询/展示 | `MapInfoCommand` | `Map` | `MapInfo` | 10 | 2 | S | SM_SYSMESSAGE |
| 120 | 查询/展示 | `MobCountCommand` | `MobCountwa` | `MobCount` | 10 | 2 | S | SM_SYSMESSAGE |
| 121 | 查询/展示 | `MobLevelCommand` | `查看周围` | `MobLevel` | 10 | 1 | N | — |
| 122 | 查询/展示 | `ServerInfoCommand` | `ServerInfo` | `ServerInfo` | 10 | 2 | N | — |
| 123 | 查询/展示 | `ShowEffectCommand` | `ShowEffect` | `ShowEffect` | 10 | 2 | N | — |
| 124 | 查询/展示 | `ShowHumanFlagCommand` | `showflag` | `ShowHumanFlag` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 125 | 查询/展示 | `ShowHumanUnitOpenCommand` | `showopen` | `ShowOpen` | 10 | 2 | N | — |
| 126 | 查询/展示 | `ShowMapInfoCommand` | `MapInfo` | `ShowMapInfo` | 10 | 1 | S | SM_SYSMESSAGE |
| 127 | 查询/展示 | `ShowMapModeCommand` | `ShowMapMode` | `ShowMapMode` | 10 | 2 | S | SM_SYSMESSAGE |
| 128 | 查询/展示 | `ShowUnitCommand` | `showunit` | `ShowUnit` | 10 | 0 | N | — |
| 129 | 查询/展示 | `ShowUseItemInfoCommand` | `ShowUseItem` | `ShowUseItem` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 130 | 查询/展示 | `TotalCommand` | `Total` | `Total` | 10 | 1 | N | — |
| 131 | 查询/展示 | `WhoCommand` | `who` | `Who` | 10 | 1 | N | — |
| 132 | 桩/测试 | `BackStepCommand` | `Backstep` | `BackStep` | 10 | 2 | N | — |
| 133 | 桩/测试 | `CreateRobotPlayCommand` | `AddRobotPlay` | `AddRobotPlay` | 0 | 2 | S | SM_SYSMESSAGE |
| 134 | 桩/测试 | `FireBurnCommand` | `FireBurn` | `FireBurn` | 10 | 2 | S | SM_SYSMESSAGE |
| 135 | 桩/测试 | `TestFireCommand` | `TestFire` | `TestFire` | 10 | 2 | N | — |
| 136 | 桩/测试 | `TestGetBagItemsCommand` | `TestGetBagItems` | `TestGetBagItems` | 10 | 1 | S | SM_SYSMESSAGE |
| 137 | 桩/测试 | `TestSpeedModeCommand` | `TestSpeedMode` | `TestSpeedMode` | 10 | 1 | S | SM_SYSMESSAGE |
| 138 | 桩/测试 | `TestStatusCommand` | `TestStatus` | `TestStatus` | 10 | 2 | S | SM_SYSMESSAGE |
| 139 | 桩/测试 | `UserCmdCommand` | `UserCmd` | `UserCmd` | 10 | 2 | N | — |
| 140 | 禁言/监听 | `DisableSendMsgCommand` | `+jy` | `DisableSendMsg` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 141 | 禁言/监听 | `DisableSendMsgListCommand` | `+-jy` | `DisableSendMsgList` | 10 | 1 | S | SM_SYSMESSAGE |
| 142 | 禁言/监听 | `EnableSendMsgCommand` | `-jy` | `EnableSendMsg` | 10 | 2 | S | SM_SYSMESSAGE |
| 143 | 禁言/监听 | `ShutupCommand` | `禁言` | `Shutup` | 10 | 2 | S | SM_SYSMESSAGE |
| 144 | 禁言/监听 | `ShutupListCommand` | `查看禁言` | `ShutupList` | 10 | 1 | S | SM_SYSMESSAGE |
| 145 | 禁言/监听 | `ShutupReleaseCommand` | `取消禁言` | `ShutupRelease` | 10 | 2 | S | SM_SYSMESSAGE |
| 146 | 禁言/监听 | `ViewWhisperCommand` | `查看私聊` | `ViewWhisper` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 147 | 行会/城堡/赛事 | `AddGuildCommand` | `创建行会` | `AddGuild` | 10 | 2 | C+S | SM_SYSMESSAGE |
| 148 | 行会/城堡/赛事 | `AnnouncementCommand` | `Announcement` | `Announcement` | 10 | 2 | N | — |
| 149 | 行会/城堡/赛事 | `AuthallyCommand` | `允许联盟` | `Authally` | 10 | 1 | N | — |
| 150 | 行会/城堡/赛事 | `AuthCancelCommand` | `取消联盟` | `AuthCancel` | 10 | 2 | N | — |
| 151 | 行会/城堡/赛事 | `AuthCommand` | `联盟` | `Auth` | 10 | 1 | N | — |
| 152 | 行会/城堡/赛事 | `ChangeSabukLordCommand` | `沙城` | `ChangeSabukLord` | 10 | 2 | S | SM_SYSMESSAGE |
| 153 | 行会/城堡/赛事 | `ContestPointCommand` | `ContestPoint` | `ContestPoint` | 10 | 2 | S | SM_SYSMESSAGE |
| 154 | 行会/城堡/赛事 | `DelGuildCommand` | `删除行会` | `DelGuild` | 10 | 2 | S | SM_SYSMESSAGE |
| 155 | 行会/城堡/赛事 | `EndContestCommand` | `EndContest` | `EndContest` | 10 | 2 | N | — |
| 156 | 行会/城堡/赛事 | `EndGuildCommand` | `退出门派` | `EndGuild` | 10 | 1 | S | SM_SYSMESSAGE |
| 157 | 行会/城堡/赛事 | `ForcedWallconquestWarCommand` | `开始攻城` | `ForcedWallconquestWar` | 10 | 2 | B+S | SM_SYSMESSAGE,广播(SM_HEAR 系) |
| 158 | 行会/城堡/赛事 | `GuildWarCommand` | `GuildWar` | `GuildWar` | 10 | 1 | N | — |
| 159 | 行会/城堡/赛事 | `SbkDoorControlCommand` | `SbkDoor` | `SbkDoorControl` | 10 | 1 | N | — |
| 160 | 行会/城堡/赛事 | `ShowSbkGoldCommand` | `SabukWallGold` | `ShowSbkGold` | 10 | 2 | S | SM_SYSMESSAGE |
| 161 | 行会/城堡/赛事 | `StartContestCommand` | `StartContest` | `StartContest` | 10 | 1 | S | SM_SYSMESSAGE |
| 162 | 行会/城堡/赛事 | `StartQuestCommand` | `开始提问` | `StartQuest` | 10 | 2 | S | SM_SYSMESSAGE |
| 163 | 造物/刷怪 | `DelNpcCommand` | `DelNpc` | `DelNpc` | 10 | 1 | S | SM_SYSMESSAGE |
| 164 | 造物/刷怪 | `GiveMineCommand` | `GiveMine` | `GiveMine` | 10 | 2 | S | SM_SYSMESSAGE |
| 165 | 造物/刷怪 | `MissionCommand` | `Mission` | `Mission` | 10 | 2 | S | SM_SYSMESSAGE |
| 166 | 造物/刷怪 | `MobCommand` | `怪` | `Mob` | 10 | 2 | S | SM_SYSMESSAGE |
| 167 | 造物/刷怪 | `MobFireBurnCommand` | `MobFireBurn` | `MobFireBurn` | 10 | 2 | S | SM_SYSMESSAGE |
| 168 | 造物/刷怪 | `MobNpcCommand` | `MobNpc` | `MobNpc` | 10 | 2 | S | SM_SYSMESSAGE |
| 169 | 造物/刷怪 | `MobPlaceCommand` | `MobPlace` | `MobPlace` | 10 | 2 | S | SM_SYSMESSAGE |
| 170 | 造物/刷怪 | `ReCallMobExCommand` | `ReCallMobEx` | `ReCallMobEx` | 10 | 2 | S | SM_SYSMESSAGE |
| 171 | 造物/刷怪 | `SmakeItemCommand` | `改属性` | `SmakeItem` | 10 | 2 | S | SM_SYSMESSAGE |
| 172 | 重载/管理 | `LoadAdminCommand` | `LoadAdmin` | `LoadAdmin` | 10 | 1 | S | SM_SYSMESSAGE |
| 173 | 重载/管理 | `NpcScriptCommand` | `NpcScript` | `NpcScript` | 10 | 1 | S | SM_SYSMESSAGE |
| 174 | 重载/管理 | `ReloadAbuseCommand` | `ReloadAbuse` | `ReloadAbuse` | 10 | 1 | N | — |
| 175 | 重载/管理 | `ReLoadAdminCommand` | `重读GM` | `ReLoadAdmin` | 10 | 1 | S | SM_SYSMESSAGE |
| 176 | 重载/管理 | `ReloadAllGuildCommand` | `ReloadGuildAll` | `ReloadAllGuild` | 10 | 1 | S | SM_SYSMESSAGE |
| 177 | 重载/管理 | `ReloadGameItemCommand` | `ReloadItemDB` | `ReloadItemDB` | 10 | 1 | S | SM_SYSMESSAGE |
| 178 | 重载/管理 | `ReloadGuildCommand` | `ReloadGuild` | `ReloadGuild` | 10 | 2 | S | SM_SYSMESSAGE |
| 179 | 重载/管理 | `ReloadLineNoticeCommand` | `重读公告` | `ReloadLineNotice` | 10 | 1 | S | SM_SYSMESSAGE |
| 180 | 重载/管理 | `ReloadMagicDbCommand` | `ReloadMagicDB` | `ReloadMagicDB` | 10 | 1 | S | SM_SYSMESSAGE |
| 181 | 重载/管理 | `ReloadManageCommand` | `重读登陆` | `ReloadManage` | 10 | 1 | S | SM_SYSMESSAGE |
| 182 | 重载/管理 | `ReloadMonItemsCommand` | `重读爆率` | `ReloadMonItems` | 10 | 1 | S | SM_SYSMESSAGE |
| 183 | 重载/管理 | `ReloadNpcCommand` | `重读商人` | `ReloadNpc` | 10 | 2 | N | — |
| 184 | 重载/管理 | `ReloadRobotCommand` | `重读机器人脚本` | `ReloadRobot` | 10 | 1 | S | SM_SYSMESSAGE |
| 185 | 重载/管理 | `ReloadRobotManageCommand` | `重读机器人配置` | `ReloadRobotManage` | 10 | 1 | S | SM_SYSMESSAGE |

### 4.4 命令族要点（12 族）

1. **数值/角色（44）**：`AddGold/DelGold/GameGold/GamePoint/CreditPoint/...`。共同结构：`@params` 下标取值 → `WorldEngine.GetPlayObject(name)` → 改对方字段 → 双向 `SysMsg`。**全部要求目标在线**，离线只出 `{0} 现在不在线，或在其它服务器上!!!`。落库由玩家保存路径异步完成（`GameGold` 等不入库，`GameGold` 经 `GameGoldChanged()` 发帧）。tier：数值类 **R**（字段级 diff），纯开关类 L。
2. **传送/召唤（17）**：`RecallHuman/ReGotoHuman/Ting/SuperTing/PositionMove/MapMove(集体)/UserMoveXY/...`。`PositionMove`/`MapMove` 走 `SpaceMove`（RNG + 地图可达性），`RecallHuman` 走 `GetRecallXy`（RNG）。**R**。
3. **行会/城堡/赛事（16）**：`AddGuild/DelGuild/ChangeSabukLord/ForcedWallconquestWar/StartContest/...`。副作用含 `GuildMgr` 改动 + `SS_205/206/211/212` 跨服消息 + `SendBroadCastMsg`；`ChangeSabukLord` 的 `bool.Parse(@params[2])` 在缺参时抛 `IndexOutOfRange`（外层 catch 吞）。**R**（落库 + 广播时序）。
4. **重载/管理（14）**：`Reload*`/`LoadAdmin`/`NpcScript`。读文件、改内存表；`ReloadGuild`/`ReloadAllGuild` 有主服限制（`ServerIndex != 0` 拒绝）。**L**（纯逻辑，抽样冒烟即可；`ReloadItemDB` 等涉及物品库的归 L 但需内容夹具）。
5. **封禁名单（9）**：`Deny*`/`DelDeny*`/`ShowDeny*`。**核心逻辑整段被注释**（`DenyIPaddrLogonCommand.cs` 只剩 `SaveDenyIPAddrList()` 与提示），且 `sFixDeny = @params[3]` 在只有 2 个参数时抛越界。**L**（照抄 stub + 提示文本）。
6. **禁言/监听（7）**：`Shutup`/`ShutupList`/`ShutupRelease`/`DisableSendMsg*`/`EnableSendMsg`/`ViewWhisper`。`Shutup` 的 `DenySayMsgList` 写入被注释，只剩提示；`ViewWhisper` 设监听标志。**L**，涉及 `DenySayMsgList` 时升 R。
7. **仓库/登录密码（6）**：`SetPassWord/UnPassWord/ChgpassWord/UnlockStorage/UnLock/Lock/ClearHumanPassword`。输入走 `RM_PASSWORD` 二次交互（`IsSetStoragePwd`/`IsCheckOldPwd`/`IsUnLockPwd`/`IsUnLockStoragePwd` 状态机），落库列 `StoragePwd`。**部署 `PasswordLockSystem=0`（`Server.conf:382`）⇒ 全部命令第一句就回「没有启用密码锁功能」**，状态机在现网不可达。**R**（落库 + 状态机时序）。
8. **造物/刷怪（9）**：`Make/Smake/Mob/ReCallMobEx/MobNpc/DelNpc/MobPlace/Mission/GiveMine`。`Make` 有「权限 < PermissionMax 时只能 1 件 + 安全区 + 攻城禁造 + 禁造列表」四门；`Mob` 上限 64、`GetFrontPosition`。**R**（RNG 抽数 + 实体生成）。
9. **查询/展示（16）**：`Who/Total/HumanCount/MobCount/MapInfo/Info/...`。多数 `stub`（`Who`/`Total`/`MobLevel`/`ServerInfo`/`ShowUnit`/`showopen` 均为空体）。**L**。
10. **会话控制（4）**：`KickHuman`（置 `BoKickFlag/BoEmergencyClose`）、`Kill`、`ReAlive`、`Reconnection`。**L**（帧序随离线路径，另有卡覆盖）。
11. **桩/测试（8）**：`Test*`/`AddRobotPlay`/`UserCmd`/`BackStep`/`FireBurn`。空体或仅参数校验。**L**（照抄 no-op 即可）。
12. **其它（35）**：`Show*`/`Change*`/`Hair`/`Hunger`/`Mission` 等零散项，按 §4.3 表逐条归类到上面 11 族或直接并入最近族。

### 4.5 oracle 怪癖清单（必须原样搬，不是 bug fix）

1. `DenyIPaddrLogonCommand.cs:18`：`string sFixDeny = @params.Length > 1 ? @params[3] : "";` —— 2 个参数即越界抛 `IndexOutOfRangeException`（由 `ProcessUserLineMsg` 的 `catch` 吞并记日志）。
2. 多处 `sHumanName[1] == '?'`（`PrvMsg`/`RecallHuman`/`Shutup`/`GameGold`…）：单字符名字直接越界抛异常。
3. `RecallHumanCommand.cs:18` 的判空写成 `sHumanName[1]`，与其它命令的 `[0]` 不一致，照抄。
4. `@commands` 每次调用抛 `TargetParameterCountException`（§1.3）。
5. `GroupRecall` 的 `(short)dwValue` 与 `GuildRecall` 的 `(short)dwValue` 强转不同（前者先 `short` 再乘 1000，后者 `int` 参与运算），长 uptime 下溢出行为不同。
6. `ChangeSabukLord` 的 `bool.Parse(@params[2])` 对非 `True/False` 抛异常。
7. `Shutup`/`Deny*`/`ClearMapMonster`/`AddGuild` 的部分逻辑被注释，属于「oracle 自己就是 no-op」，不要补实现。
8. `AuthallyCommand` 整个类是 `internal`，`Execute` 体内所有逻辑被注释 ⇒ 注册了但永远无副作用。
9. `Make` 的 `SystemShare.RandomNumber.Random(MakeRandomAddValue) == 0`（升级抽数）虽被注释掉调用，但 **RNG 调用本身在**（`if` 条件里）——这条决定 `@Make` 是 R 级。

---

## 5. Rust 现状

### 5.1 入口与丢弃点

- `mir2-rs/bins/game-svr/src/chat.rs` 模块头：「**仍未接（登记）**：`@` 命令系统、GM 红字、…」。
- `main.rs::handle_say`（约 `:1291-1335`）：`/` → `!` → `is_plain_say`；`@` 行落到

  ```rust
  glog!("DBG", "CM_SAY：非普通说话（@命令），本切片未接 ⇒ 丢弃 ChrName={} tail={line:?}", ...);
  return;
  ```

  即：**入口在、分流在、命令表与回帧全无**。
- `bins/game-gate/src/rules.rs:629`：`@` 行不走发言间隔（oracle `!StartsWith("@")` 之外的等价物），网关侧已正确放行。

### 5.2 缺失的会话状态（按命令族）

| 状态 | oracle 字段 | Rust 现状 |
|---|---|---|
| 权限 | `PlayObject.Permission` | **无**（`Entity` 无该字段；`merchant.rs` 的 `permission` 是函数参数，恒 0） |
| 会话开关 | `AllowDeal/AllowGuild/AllowGroupReCall/AllowGuildReCall/HearWhisper/IsLockLogon/IsLockLogoned` | `guild_cmd.rs` 只有 `allow_guild`；`inbound.rs:659` 有 `is_lock_logoned`；其余无 |
| 组队传送 | `GroupRcallTick/GroupRcallTime/GroupOwner/GroupMembers` | `party.rs` 有队伍；冷却无 |
| 禁言/监听 | `LockWhisperList/DenySayMsgList/ViewWhisper` | 无 |
| 密码状态机 | `IsSetStoragePwd/IsCheckOldPwd/IsUnLockPwd/IsUnLockStoragePwd/PwdFailCount/StoragePwd` | `inbound.rs` 有部分锁字段；`StoragePwd` 在 `playerdata.rs` |
| 持久化列 | `AllowGroupReCall/AllowGuildReCall/GroupRcallTime/StoragePwd` | `crates/storage/src/playerdata.rs:665-668` **已有** |

### 5.3 可复用的现成能力

- 帧：`smframe::sm_sysmessage(recog, f, b, &msg)`（`bins/game-svr/src/smframe.rs:507`），`SM_SYSMESSAGE=100` 已在 `crates/protocol`。
- 传送：`crates/world/src/space_move.rs`（`space_move_get_rand_xy`）、`world` 的 `try_step*/SpaceMove` 语义、`mir2-rs/crates/world/src/slave.rs` 的 recall 夹具。
- 行会：`GuildManager::add_guild/del_guild`（`crates/world/src/guild.rs:1018-1060`）。
- 城堡：`crates/world/src/castle.rs` 的 `castle_official_action` / `UnderWar` 状态（`@ForcedWallconquestWar` 依赖）。
- 物品：`ItemSystem`/`Make` 相关在 `bins/game-svr/src/bag.rs`、`items.rs`（T-95 残余）。
- 颜色：`MsgColors` 只有 `red/hear/whisper`，**需补 green(219/255)/blue(251/252)**（`main.rs:12624`）。

### 5.4 与现有设计约束的对齐

- 单线程 tick、顺序显式化：命令副作用必须通过 `World`/`OnlineRegistry` 的既有有序结构（玩家遍历按 `actor_id`/注册序），不得直接迭代 `HashMap`。
- 协议层按原始字节：命令名的比较应在 **GBK 字节**或「GBK→UTF-8 后按码点」两种等价口径里二选一并写死（建议后者，复用现有 `gbk_encode/decode`）。
- 不改 `crates/protocol`、`crates/storage`（本车道红线）；新命令层只进 `bins/game-svr`。

---

## 6. 写集切分与批次

| 批次 | 内容 | 写集（未来实现） | tier | 依赖 / 冲突 |
|---|---|---|---|---|
| **G1 派发骨架 + 6 条可达** | `parse_command`、`CommandTable`（185 行常量）、权限门、未知命令回落、`@时间/@拒绝/@拒绝私聊/@LockLogin/@AddRobotPlay/@help` | `bins/game-svr/src/gmcmd.rs`（新）、`chat.rs`、`main.rs`（`handle_say` 一行分流）、`MsgColors` 扩 green/blue | L | 与在飞 `bins/game-svr/**` 车道冲突，须串行；**先合 G1 再动别的** |
| **G2 会话开关族** | `@加入门派/@交易/@允许天地合一/@sadasd/@拒绝组队聊天/@禁止行会信息/@Rest/@OnHorse/@OffHorse/@AttackMode/@showopen/@showunit` | `gmcmd.rs`、`guild_cmd.rs`、`party.rs`、新增 session 字段 | L（`AllowGroupReCall/AllowGuildReCall` 落库 → **R**） | 依赖 G1；`guild_cmd.rs` 是已有面，改动需通知行会车道 |
| **G3 传送/召唤族** | `@天地合一/@RecallHuman/@ReGotoHuman/@Ting/@SuperTing/@PositionMove/@MapMove/@UserMoveXY` | `gmcmd.rs`、`world` 的 recall/space_move 调用（只读 API） | **R**（RNG） | 依赖 G2 的 `Allow*ReCall` 与队伍状态 |
| **G4 数值/落库族** | `@加钱/@减钱/@GameGold/@AddGamePoint/@调整经验/@调整声望/…` | `gmcmd.rs`、`ability.rs`、`bag.rs`、`dbclient.rs`（写回路径复用现有存档） | **R**（字段级 diff） | 与 `crates/world/src/{monai,monster*}` 无关；但与数值/存档车道冲突 |
| **G5 行会/城堡/赛事** | `@创建行会/@删除行会/@沙城/@开始攻城/@StartContest/…` | `gmcmd.rs`、`guild_cmd.rs`、`castle.rs`、跨服 `SS_*`（单服登记不实现） | **R**（落库 + 广播） | 与 `docs/window-round2-runbook*` 车道无关；与城堡车道冲突 |
| **G6 桩/死键/红字** | 30 条 stub 的 no-op 与帮助文本、`@@!` 红字、`@commands` 抛异常语义 | `gmcmd.rs` | L | 依赖 G1 |

> 本车道**不**修改上述任何文件；上表只是给后续卡用的边界。当前并发写集（`bins/game-svr/**`、`crates/world/src/{monai,monster*}.rs`、`docs/window-round2-runbook*`）与本卡零交集——本卡只新增 `mir2-rs/tests/parity/m3/design-gm-commands-20261023.md`。

---

## 7. 判据草案（带牙）

### 7.1 语义表（改动即红）

1. **命令表完整性**：`gmcmd::COMMANDS` 长度 == 185；按 `(name.to_ascii_lowercase())` 唯一；与本文 §4.3 表逐行 diff（列：现网名/权限/参数个数）。
2. **解析器**：`parse_command` 表驱动 15 例，逐例比对 `ExtractCommandAndParameters`（§1.2 的 6 条推论每条至少 2 例，含 `@ 时间`、`@时间\t1`、`@时间  1`、`@`、`@时间 `、`@@!hi`、`@天地合一:1`）。
3. **权限门**：`@交易` 在 perm 0 → 输出 `权限不够!!!`（Red 255/56）；`@时间` perm 0 放行。
4. **未知命令**：perm 0 → 恰好一条 `@此命令不正确，或没有足够的权限!!!`；perm ≥2 → 两条（`未知命令: X Y` + `@@X此命令不正确…`）。
5. **名字匹配**：`@sadasd` 命中 `AllowGuildRecallCommand`（验证小写类名 + 配置覆盖）；`@Allowguildrecall` 在现网配置下**不**命中（验证覆盖确实发生）。
6. **参数语义**：`@Make 木剑  1`（双空格）得到的 `@params` 必须是 `["木剑","","1"]`（验证 `Split(' ')` 保留空项）。

### 7.2 帧级（逐字节）

- `@时间`：`SM_SYSMESSAGE(100)`，`series=1`，`param=MakeWord(251,252)`，正文 GBK `当前日期时间: ` + 格式化时间；用现有 `sm_sysmessage` 与金标准 seq 对齐。
- `@拒绝私聊`：Green `MakeWord(219,255)`，正文 `[禁止私聊]` / `[允许私聊]`（GBK 字节）。
- `权限不够!!!`：Red `MakeWord(255,56)`。
- `@@!正文`：广播逐人 `SM_SYSMESSAGE(Red, GameManger)`，正文 = `!` + 前 40 字符（GBK 字符数截断口径需与 `string.Length` 对齐：oracle 是 UTF-16 字符数，GBK 编码后字节数可能不同——**这是 G6 的必测歧义点**）。

### 7.3 真机 A/B（窗口项）

- 仅跑现网可达 6 条：同一账号在 C# 与 Rust 各发一次 `@时间/@拒绝/@拒绝私聊/@LockLogin/@AddRobotPlay/@help`，比 `SM_SYSMESSAGE` 的 `param`（颜色）+ 正文（时间字段掩码后比对）。
- 若要验非 0 权限命令：窗口内用一次性 `SETPERMISSION` 脚本把测试号抬到 10（内容侧写入，需 owner 批准），再跑 `@加入门派/@交易/@天地合一/@df56dfs/@允许天地合一/@sadasd` 六条，比 SysMsg + 落库字段（`AllowGroupReCall`/`AllowGuildReCall`/`GroupRcallTime`）。

### 7.4 「牙」清单（改坏生产路径必红）

| 破坏点 | 必红断言 |
|---|---|
| `handle_say` 里把 `@` 分支恢复成「直接丢弃」 | 真机 `@时间` 零回帧 → A/B diff 非空；单测 `dispatch_help_returns_usage` 红 |
| 权限门写成 `<=` 或漏掉 `PlayerActor != null` 判断 | `@交易` perm 0 用例红（应出「权限不够!!!」而实际执行） |
| 配置覆盖顺序写反（先注册后读配置） | `@sadasd`/`@天地合一` 名字解析用例红（会解析成 `Allowguildrecall`/`GroupRecall`） |
| 命令名比较改成大小写敏感 | `@sadasd` 与 `AllowGuildReCall` 键匹配用例红；25 条英文名不受影响（正好暴露差异） |
| `parameters.Split(' ')` 改成 `split_whitespace` | `@Make 木剑  1` 参数数组用例红 |
| `@时间` 颜色写成 Red | 帧字节用例红（`param` 差 251/252） |
| `@GuildRecall` 补上成员遍历（「修 bug」） | 「已传送0个成员」文本用例红（oracle 是 no-op） |

### 7.5 反例（明确不许）

- 不许把「现网权限 0 ⇒ 命令不可达」解释成「命令不需要实现」：Rust 连分发都没有，`@时间` 这种普通玩家可用的命令已经在真机分歧。
- 不许用「语义等价」把 `@commands` 的反射异常、`Deny*` 的越界异常、`GuildRecall` 的 no-op 抹平。
- 不许为了「更好用」把混淆命令名（`sadasd`/`df56dfs`）改回英文——内容冻结。

---

## 8. tier 与窗口项

- **本文档**：`tier=L`（纯文档，无生产路径改动）。
- **G1/G2(开关部分)/G6**：`tier=L`，审查者在钉住的 commit 上自证「命令表 == oracle 抽取结果」+ 至少 1 条抽样冒烟（`@时间` 进程级跑通）。
- **G2(落库两条)/G3/G4/G5**：`tier=R`。理由：`AllowGroupReCall`/`AllowGuildReCall`/`GroupRcallTime`/`StoragePwd` 落库（字段级 diff）、`RecallHuman`/`Make`/`Mob` 抽 RNG（抽数序列）、`SendBroadCastMsg` 的逐人帧序。

窗口项：

| 编号 | 内容 | 为何必须占窗口 |
|---|---|---|
| W-1 | 确认对拍用的 C# 构建是 **Release** | Debug 会强制 `Permission=10` 并多发一条红字，命令可达集完全不同 |
| W-2 | 确认 `M2GameSvr/Command.conf` 与 `Mir200/Command.conf` 一致 | 名字/权限全来自这份文件；被改动过则命令名整体漂移 |
| W-3 | 非 0 权限命令的测试前置：`SETPERMISSION` 脚本或等价授权 | 现网 `Permission` 恒 0，无它则 179 条不可测 |
| W-4 | `characters.AllowGroupReCall/AllowGuildReCall/GroupRcallTime` 的库前后态 | R 级落库判据 |
| W-5 | `@@!` 红字的截断口径（UTF-16 字符 vs GBK 字节） | 中文正文 40 字符处会分叉 |

---

## 9. 未决问题（需 owner / 协调者裁决）

| ID | 问题 | 选项 | 建议 |
|---|---|---|---|
| D-1 | 现网 `Permission` 恒 0（`GetHumPermission` 被注释、`AdminList` 空）——是否要在 Rust 侧**复刻这个「锁死」**，还是按 `AdminList.txt` 预期语义恢复？ | (a) 1:1 复刻 = `Permission` 恒 0；(b) 恢复调用 = 与 oracle 现网不等价 | 建议 **(a)**：硬约束③「行为等价优先」，并在卡面写明「GM 面在现网不可达」；恢复属单开 bug-fix 批次 |
| D-2 | 非 0 权限命令的真机判据如何构造 | (a) 一次性脚本 `SETPERMISSION`（改内容）；(b) 只做 L 级等价 + 进程级冒烟，不占窗口 | 建议 (a) 走 owner 特批窗口，否则本面整体停在 L |
| D-3 | `@@!` 红字（`GMRedMsgCmd`）是否纳入本卡 | (a) 并入 G6（需 `Permission>=6`，现网不可达）；(b) 登记不实现 | 建议 (a) 但标 `window_required`，语义按 oracle |
| D-4 | 49 个死配置键 / 5 个拼写漂移键，Rust 是否要「修」 | (a) 照抄（死键继续死）；(b) 修键名 = 改变现网命令名 | 建议 **(a)**；修属内容/配置变更 |
| D-5 | `@GuildRecall` 的成员遍历被注释——是否保持 no-op | (a) 保持；(b) 恢复 | 建议 **(a)**；恢复会改行为且需 DB 行会成员表 |
| D-6 | 命令名比较口径：GBK 字节 vs GBK→UTF-8 码点（`OrdinalIgnoreCase`） | 两者对现网 185 个名字等价，但对「非法 GBK 字节」行为不同 | 建议 UTF-8 码点 + ASCII 大小写折叠，并在单测里钉 `sadasd`/中文名 |
| D-7 | 写集冲突：`bins/game-svr/**` 当前有在飞车道 | 需要排期串行 | 建议 G1 单独开窗，合入后再放 G2+ |

---

## 10. 证据四件套（本车道复算命令与摘要）

```sh
# 1) 命令类与配置映射抽取（脚本在 /e/tmp/gmscan/，不进仓）
python -I /e/tmp/gmscan/scan.py      # classes: 185 / registerCommand: 160 / conf mapped fields: 182
python -I /e/tmp/gmscan/classify.py  # stubs: 30
python -I /e/tmp/gmscan/tablegen.py  # 185 行表

# 2) 部署配置与 oracle 摘要
sha256sum src/Modules/GameCommand/GameCommands.cs \
          src/Modules/GameCommand/CommandSystem.cs \
          src/Modules/GameCommand/Conf/GameCmdConf.cs \
          /e/MirServer/M2GameSvr/Command.conf \
          /e/MirServer/Mir200/Envir/AdminList.txt
# 39490ffb78a110ff2f27cbd7adf806ad3e9c320def4d7c2f5805f3e423258e2e  GameCommands.cs
# 3c3e7212ae7b7e9a904a5d4e34f78b64c423284d854f33d4eb6afb262a624243  CommandSystem.cs
# 902f042bae72af068335986c29ca6a4c87e3bbb907ae559903262c0510d793fd  GameCmdConf.cs
# 062eda7d2e3600a5cca2fbc9b64e62f6acbb29cbe9e6c7de46a0260305b5970a  Command.conf
# 28e8d54c48fb6a5d8bb2642c9d267e59dd8ad6f9adfc9e3138a525fa97fa4f1e  AdminList.txt

# 3) 部署构建与权限链
cat /e/MirServer/M2GameSvr/!deployed-from.json            # build_dir=...\bin\Release
strings -el /e/MirServer/M2GameSvr/CommandSystem.dll | grep 调试模式   # 期望空
xxd /e/MirServer/Mir200/Envir/AdminList.txt | head -2     # 2a 09 ... 0d0a（无 IP 段）
```

残留物：`/e/tmp/gmscan/{scan.py,classify.py,tablegen.py,rows.json,rows2.json,table.md}`（本车道分析中间物，可随 `/e/tmp` 清理策略回收）；worktree `/e/tmp/wt-gmdesign`（本卡进行中，收卡后按 Cleanup DoD 删除）。
