# M3 脚本动作全集 × 部署使用面 × Rust 实现对照审计（2026-10-18）

- 任务：只读审计「部署脚本里实际用到的动作，Rust 是否都能执行且语义一致」。
- 仓库：`E:/Users/gxh/Documents/GitHub/OpenMir2`；worktree：`E:/tmp/impl-scriptaudit`（detached `69892d40b`）。
- 写集：**只有一个 md**（本文件）；未改代码、未起服务、未占端口。
- oracle 基线：`src/Modules/ScriptEngine/Consts/ExecutionCode.cs`（352 个动作）、
  `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs`（133 条注册）、
  `src/Modules/ScriptEngine/ScriptEngine.cs`（动作 switch）。
- 部署面：`E:/MirServer/M2GameSvr/Envir/**`（639 个 `.txt`，GBK 解码，只读 grep/统计）。
- Rust 面：`mir2-rs/crates/script/src/engine/{dispatch,actions}.rs`、`handlers.rs`、`parser.rs`。

> **行号口径**：oracle 行号逐条实测（`ExecutionCode.cs` 的行 = `[ScriptDefName]` 的**下一行**，即枚举成员行；
> `ExecutionProcessingSys.cs` 的行 = `ProcessExecutionMessage[...] = ...;` 注册行或方法声明行）。
> Rust 侧按纪律只引符号/文件路径，不写行号。

## 0. 一句话结论

**部署脚本里被枚举识别的 68 个动作中，67 个在 Rust 已实现或由引擎 switch 等价承接；
唯一例外 `DELAYCALL` 在 oracle 自身就没有处理器（被静默丢弃），Rust 同样 no-op，因此「可达动作」的 parity 缺口为 0。**
真正的缺口在**内容/枚举漂移**：部署写了 16 个非枚举 token（15 个命令名 + 1 个排版残留 `}`；如 `GAMEDIAMOND`/`GAMEGIRD`/`READRANDOMLINE`，
其中 8 个在「首个注册动作前」可达），oracle 与 Rust 都会把它们在解析期丢弃——两边行为一致，但脚本作者的意图在两边都落空。
另发现 oracle 自身的**动作列表提前返回**缺陷：每个 `#ACT`/`#ELSEACT` 只执行**第一条已注册动作**，
导致 6453 条规范动作行里只有 2446 条按脚本字面可达（37.9%），部署脚本大量命令实际是死代码。
（「可达」是**纯源文件文本口径**，未模拟 `#CALL` 内联；紧跟 `#CALL` 的行会被高估，内联修正后枚举可达 2440 条、非枚举 33 条，详见 §1.3。）

## 1. 口径与方法

1. **oracle 清册**：枚举 352 条（逐条记录 `ScriptDefName`、枚举成员、枚举值、行号）；
   注册表 133 条（`(int)ExecutionCode.X = ActionOfY`，逐条记录注册行与方法行）；
   动作 switch **16** 条（`ScriptEngine.GotoLableQuestActionProcess`；`grep 'case ExecutionCode\.'` 共 **19** 个 case 标签，
   扣除同时注册的 `Map`/`MapMove`/`PlayDice`；`CheckUserDate` 的 case 在条件 switch 且已注释）。
2. **部署使用面**：遍历 `Envir/**/*.txt`，GBK 解码；只统计 `#ACT` / `#ELSEACT` 块内的行首命令；
   跳过空行与 `;` 注释；按 oracle 解析器规则把 `前缀.命令` 规范化为最后一段（`H.GIVE` → `GIVE`，`<$STR(S9)>.GAMEPOINT` → `GAMEPOINT`）。
3. **可达性口径（关键）**：读 `GotoLableQuestActionProcess` 实测，
   `if (ExecutionProcessing.IsRegister(nCmdCode)) { Execute(...); return result; }`——
   **一个动作列表（一个 `#ACT`/`#ELSEACT` 块）执行到第一条已注册动作就返回**；
   `BREAK` 是引擎 switch 分支，只把返回值置 false（**不终止当前动作列表**；调用方在该列表跑完后停止后续 `#IF` 组）；
   其余未注册命令（`TAKE`/`GOTO`/`PARAM*`/未知命令/`BREAK`）都继续往下。因此本报告的「可达」= 该行之前没有已注册动作。
   **口径边界（重要）**：本报告的「可达」是**纯源文件文本口径**，不模拟 `ScriptParsers.cs` 的
   `LoadCallScript`/`LoadScriptCallScript`——`#CALL [f] @label` 会把 `f` 中 `@label` 到 `}` 的内容内联（含 `#IF`/`#ACT`），
   其首个注册动作会截断调用点之后的行。因此紧跟 `#CALL` 的行在本口径下被高估，这是已知的上界缺陷。
   实测受影响的有 **11 条文本可达行**：枚举 6 条（`DELAYCALL`×4、`BREAK`×2 ⇒ 可达 33→29、73→71，枚举可达合计 2446→2440），
   非枚举 5 条（`READRANDOMLINE`×3、`OFFLINEPLAYEX`×1、`}`×1 ⇒ 可达 8→5、1→0、2→1，非枚举可达合计 38→33）。
   反例：`QuestDiary/LEGM2.CC假人系统/假人登录脚本.txt:362` 的 `#CALL [假人回盟重省.txt] @回来了` 使 363-369 不可达；
   同文件 `:396` 的 `#CALL [假人随机回收.txt] @假人随机回收` 使 397-399、401 不可达。
   正文所有频次一律用文本口径（高估上界），内联修正值仅在此与 §3 注明。
4. **语义一致判据**：Rust `run_actions` 同样在第一个已实现动作后返回；未实现的注册动作显式 `NotImplemented` 并按 C# 语义停止；
   未注册动作走 `run_switch_action`；两者都不命中 ⇒ 静默 no-op。

## 2. Oracle 清册（逐条实测）

### 2.1 三层派发结构

| 层 | 位置 | 条数 | 说明 |
|---|---|---:|---|
| 枚举全集 | `ExecutionCode.cs` | **352** | `Set=1` … `TakeDlgItem=352`，无跳号 |
| 注册处理器 | `ExecutionProcessingSys.cs` `Initialize()` | **133** | 131 个唯一方法（`ClearNameList/ClearList` 共用、`SendMsg/LineMsg` 共用） |
| 动作 switch | `ScriptEngine.cs` `GotoLableQuestActionProcess` | **16** | 19 个 `case ExecutionCode.*` 标签，扣除同时注册的 `Map/MapMove/PlayDice`；`CheckUserDate` 在条件 switch（已注释），不计入 |
| 无处理器 | — | **203** | = 352 − 133 − 16；解析期映射到枚举值，但动作路径既未注册也无动作 switch ⇒ 静默丢弃 |

注：`handlers.rs` 的 `ENGINE_SWITCH_EXECUTIONS` 共 **20** 条 = C# 动作 switch 的 19 个 case + 误列的 `CheckUserDate`。
`ScriptEngine.cs` 里 `CheckUserDate` 的 case 实际位于**条件** switch `GotoLableQuestCheckCondition`（且整段已注释），
动作路径无此 case。Rust 把它当动作 no-op 处理，结果等价，但生成表的归类不准确（详见 §6 缺口 G-9）。
因此 §2.4 的无处理器数按动作路径口径为 **203**（352 − 133 − 16），不是 202。

### 2.2 133 条注册动作按效果面归组

行号为 `ExecutionProcessingSys.cs` 的方法声明行；动作名 = `ScriptDefName`。最后一组与其他组有意重叠（空实现是行为属性，不是效果面）。

- **货币/点数**：GAMEGOLD(`3058`)；GAMEPOINT(`3095`)；CREDITPOINT(`2756`)；BONUSPOINT(`5303`)；RESTBONUSPOINT(`5340`)；AUTOADDGAMEGOLD(`2667`)；AUTOSUBGAMEGOLD(`2733`)；TAKECASTLEGOLD(`3575`)；OPENYBDEAL(`2329`)；QUERYYBSELL(`2363`)；QUERYYBDEAL(`2468`)
- **物品**：GIVE(`4859`)；THROWITEM(`5638`)；CLEARNEEDITEMS(`4239`)；CLEARMAKEITEMS(`4272`)；OPENITEMBOX(`5490`)；UPGRADEITEM(`3979`)；UPGRADEITEMEX(`4028`)；UPGRADEDLGITEM(`2229`)；QUERYITEMDLG(`2234`)；QUERYBAGITEMS(`2306`)；REPAIRALL(`3929`)；CLEARNAMELIST/CLEARLIST(`3004`)
- **属性/技能/战斗数值**：CHANGELEVEL(`2907`)；CHANGEEXP(`2806`)；CHANGEJOB(`2880`)；CHANGEGENDER(`5428`)；PKPOINT(`2073`)；CHANGEPKPOINT(`2949`)；HUMANHP(`5185`)；HUMANMP(`5225`)；ADDSKILL(`2638`)；CLEARSKILL(`3010`)；DELSKILL(`3036`)；DELNOJOBSKILL(`3022`)；SKILLLEVEL(`3521`)；SETMEMBERLEVEL(`4789`)；SETMEMBERTYPE(`4824`)；RENEWLEVEL(`5400`)；RESTRENEWLEVEL(`5349`)；KILLMONEXPRATE(`5449`)；POWERRATE(`5539`)；SETRANKLEVELNAME(`3491`)；CHANGEPERMISSION(`5620`)；CHANGENAMECOLOR(`5355`)；HAIRSTYLE(`2866`)；SETAUTOGETEXP(`2691`)；OFFLINE(`2716`)
- **传送/地图**：MAP(`2169`)；MAPMOVE(`2175`)；RECALLMAP(`2186`)；EXCHANGEMAP(`2209`)；GROUPMOVEMAP(`3954`)
- **消息/客户端交互**：SENDMSG/LINEMSG(`3162`)；MESSAGEBOX(`3416`)；CLOSE(`2181`)；QUERYVALUE(`2273`)；PLAYDICE(`2030`)；SETSENDMSGFLAG(`2320`)；SETSCRIPTFLAG(`3503`)；GMEXECUTE(`4925`)
- **组队**：RECALLGROUPMEMBERS(`3487`)；GROUPADDLIST(`3905`)；GROUPRECALL(`3918`)
- **公会**：GUILDRECALL(`3897`)；GUILDBUILDPOINT(`4997`)；GUILDAURAEPOINT(`4950`)；GUILDSTABILITYPOINT(`5138`)；GUILDFLOURISHPOINT(`5091`)；GUILDCHIEFITEMCOUNT(`5044`)；SETMPAMODE(`4423`)；ADDGUILDLIST(`5921`)；DELGUILDLIST(`5929`)
- **怪物/宠物**：MONGEN(`2141`)；MONGENEX(`5466`)；MONCLEAR(`2154`)；CLEARMAPMON(`2983`)；MOBPLACE(`3462`)；RECALLMOB(`5375`)；KILLSLAVE(`5440`)；KILLSLAVENAME(`2247`)；MOBFIREBURN(`3436`)
- **持久化/名单文件**：SAVEVAR(`3697`)；LOADVAR(`4166`)；ADDNAMELIST(`5881`)；DELNAMELIST(`5893`)；ADDACCOUNTLIST(`5906`)；DELACCOUNTLIST(`5899`)；ADDIPLIST(`5911`)；DELIPLIST(`5916`)；ADDUSERDATE(`5842`)；DELUSERDATE(`5887`)；ADDNAMEDATELIST(`2572`)；DELNAMEDATELIST(`2609`)
- **变量/脚本状态**：MOV(`291`)；MOVR(`188`)；INC(`441`)；DEC(`1556`)；SUM(`1378`)；DIV(`891`)；MUL(`1039`)；PERCENT(`1223`)；VAR(`4099`)；CALCVAR(`3787`)；Set(`2036`)；RESET(`2043`)；SETOPEN(`2051`)；SETUNIT(`2058`)；RESETUNIT(`2065`)；MISSION(`3421`)；GETMARRY(`3132`)；GETMASTER(`3147`)；SETRANDOMNO(`2112`)
- **时序/会话**：TIMERECALL(`2132`)；BREAKTIMERECALL(`2107`)；DELAYGOTO(`3754`)；CLEARDELAYGOTO(`2125`)；KICK(`5265`)；KILL(`5272`)；MARRY(`3214`)；UNMARRY(`3593`)；DELMARRY(`5328`)；MASTER(`3334`)；UNMASTER(`4328`)；DELMASTER(`5334`)；CLEARPASSWORD(`5367`)
- **C# 空实现/整段注释**：EXEACTION(`2024`)；MAPTING(`3209`)；RANDOMMOVE(`5837`)；PKZONE(`5509`)；CHANGEMODE(`5556`)；GROUPADDLIST(`3905`)；GROUPRECALL(`3918`)；GUILDRECALL(`3897`)；RECALLGROUPMEMBERS(`3487`)；MOBFIREBURN(`3436`)

### 2.3 16 条未注册、由动作 switch 承接的命令

| 动作 | 枚举行 | 枚举值 | switch 语义 |
|---|---:|---:|---|
| Take | 11 | 2 | 扣金币/背包物品 |
| TAKEW | 15 | 4 | 摘佩戴装备 |
| BREAK | 27 | 10 | 置 result=false（不终止当前列表） |
| PARAM1 | 31 | 12 | 读参数到未使用局部变量 |
| PARAM2 | 33 | 13 | 读参数到未使用局部变量 |
| PARAM3 | 35 | 14 | 读参数到未使用局部变量 |
| PARAM4 | 37 | 15 | 读参数到未使用局部变量 |
| TAKECHECKITEM | 45 | 19 | 从对话框取物品 |
| ADDBATCH | 85 | 39 | 整段注释 |
| BATCHDELAY | 87 | 40 | 写未使用局部变量 |
| BATCHMOVE | 89 | 41 | 整段注释 |
| GOQUEST | 113 | 53 | 进入任务脚本 |
| ENDQUEST | 115 | 54 | 退出任务脚本 |
| GOTO | 117 | 55 | 跳转标签（死循环计数） |
| GETDLGITEMVALUE | 763 | 351 | 空实现 |
| TAKEDLGITEM | 768 | 352 | 空实现 |

> `CHECKUSERDATE` 不在此表：其 case 位于 `ScriptEngine.cs` 的**条件** switch `GotoLableQuestCheckCondition`
> （成员行 678、`case (int)ExecutionCode.CheckUserDate`，整段已注释），动作路径没有 case。
> Rust `ENGINE_SWITCH_EXECUTIONS` 把它列为动作 no-op（生成表误列，见 §6 G-9）；矩阵第 343 行已按条件侧标注。

### 2.4 203 条无处理器命令

这 203 条枚举成员在解析器中能被识别（`ExecutionCodeDefMap` 有键），但动作路径既无注册也无动作 switch，
C# 会在解析后到达 `GotoLableQuestActionProcess` 的 `switch` 默认分支，**什么都不做**。
完整名单已并入 §5 矩阵（状态列 = `oracle 无处理器（执行 no-op）`）。其中真正「写了但 oracle 无处理器」的部署命令只有 `DELAYCALL`；
`MOBPLACE`/`SAVEVAR`/`LOADVAR`/`MONGENEX` 等是**已注册**动作，只是部署位置使其不可达。

## 3. 部署使用面（`E:/MirServer/M2GameSvr/Envir`）

- 扫描文件：**639** 个 `.txt`；`#ACT`/`#ELSEACT` 块：**1816** 个；块内动作行：**6798** 条。
- 命中 `ExecutionCode` 的动作行：**6453** 条，去重 **68** 个动作名；
  不在枚举中的命令行：**345** 条，去重 **16** 个名字（含 1 个排版残留 `}`）。
- 按「第一注册动作即返回」的**文本口径**，**枚举可达动作行 2446 条**（占全部 6798 行的 36.0%；占 6453 条规范动作的 37.9%），
  另有非枚举可达行 **38** 条 ⇒ 文本口径可达合计 2484 条。
  **#CALL 内联修正**：枚举可达 2446→2440，非枚举 38→33（修正依据与逐条反例见 §1.3）。
  可达行中 2446/2484 = **98.5%** 是 oracle 能识别的命令（文本口径）。

### 3.1 部署动作频次（按原始次数降序）

| 动作 | 原始次数 | 可达次数 | 文件数 | Rust 状态 |
|---|---:|---:|---:|---|
| BREAK | 1074 | 73 | 107 | 引擎内建（与 C# 有偏差） |
| SENDMSG | 818 | 204 | 44 | 已实现 |
| GIVE | 571 | 236 | 48 | 已实现 |
| GOTO | 448 | 89 | 37 | 引擎内建 |
| MESSAGEBOX | 437 | 370 | 63 | 已实现 |
| TAKE | 436 | 422 | 64 | 引擎内建 |
| GAMEGOLD | 286 | 274 | 13 | 已实现 |
| DELACCOUNTLIST | 238 | 0 | 3 | 已实现 |
| INC | 211 | 14 | 19 | 已实现 |
| ADDSKILL | 205 | 3 | 2 | 已实现 |
| CLOSE | 150 | 17 | 22 | 已实现 |
| BATCHDELAY | 144 | 144 | 4 | 引擎内建（等价 no-op） |
| BATCHMOVE | 144 | 0 | 4 | 引擎内建（等价 no-op） |
| GMEXECUTE | 139 | 61 | 12 | 已实现 |
| MAPMOVE | 126 | 103 | 46 | 已实现 |
| MOBPLACE | 111 | 0 | 1 | 已实现 |
| GAMEPOINT | 82 | 72 | 2 | 已实现 |
| TAKEW | 71 | 11 | 5 | 引擎内建 |
| CALCVAR | 68 | 21 | 4 | 已实现 |
| SAVEVAR | 68 | 0 | 4 | 已实现 |
| MOVR | 67 | 47 | 14 | 已实现 |
| DELAYCALL | 62 | 33 | 9 | oracle 无处理器（执行 no-op） |
| MOV | 61 | 29 | 15 | 已实现 |
| UPGRADEDLGITEM | 59 | 56 | 3 | 已实现 |
| SET | 47 | 37 | 19 | 已实现 |
| CREDITPOINT | 40 | 27 | 4 | 已实现 |
| UPGRADEITEMEX | 34 | 29 | 4 | 已实现 |
| MAP | 24 | 18 | 13 | 已实现 |
| VAR | 24 | 2 | 3 | 已实现 |
| LOADVAR | 24 | 0 | 3 | 已实现 |
| PLAYDICE | 24 | 0 | 4 | 已实现 |
| KICK | 14 | 2 | 13 | 已实现 |
| QUERYVALUE | 13 | 13 | 1 | 已实现 |
| MUL | 13 | 0 | 3 | 已实现 |
| CLEARDELAYGOTO | 10 | 0 | 4 | 已实现 |
| ADDNAMELIST | 9 | 1 | 6 | 已实现 |
| HUMANHP | 7 | 4 | 3 | 已实现 |
| HUMANMP | 7 | 2 | 3 | 已实现 |
| CHANGEEXP | 7 | 1 | 2 | 已实现 |
| REPAIRALL | 6 | 2 | 5 | 已实现 |
| QUERYITEMDLG | 5 | 5 | 3 | 已实现 |
| DELAYGOTO | 5 | 5 | 4 | 已实现 |
| CHANGELEVEL | 5 | 1 | 2 | 已实现 |
| MARRY | 4 | 4 | 1 | 已实现 |
| UNMARRY | 4 | 4 | 1 | 已实现 |
| OFFLINE | 4 | 0 | 2 | 已实现 |
| MISSION | 3 | 3 | 1 | 已实现 |
| CHANGEPKPOINT | 3 | 0 | 3 | 已实现 |
| PARAM1 | 3 | 0 | 1 | 引擎内建 |
| PARAM2 | 3 | 0 | 1 | 引擎内建 |
| PARAM3 | 3 | 0 | 1 | 引擎内建 |
| PARAM4 | 3 | 0 | 1 | 引擎内建 |
| DEC | 3 | 0 | 1 | 已实现 |
| CHANGEMODE | 3 | 0 | 1 | 已实现（C# 空实现） |
| DELNAMELIST | 2 | 2 | 2 | 已实现 |
| CLEARMAPMON | 2 | 1 | 2 | 已实现 |
| DELUSERDATE | 2 | 0 | 2 | 已实现 |
| ADDUSERDATE | 2 | 0 | 2 | 已实现 |
| KILL | 2 | 0 | 2 | 已实现 |
| SETSENDMSGFLAG | 2 | 0 | 2 | 已实现 |
| MONGENEX | 2 | 0 | 2 | 已实现 |
| SETSCRIPTFLAG | 2 | 0 | 2 | 已实现 |
| HAIRSTYLE | 2 | 0 | 2 | 已实现 |
| SETAUTOGETEXP | 1 | 1 | 1 | 已实现 |
| BREAKTIMERECALL | 1 | 1 | 1 | 已实现 |
| RENEWLEVEL | 1 | 1 | 1 | 已实现 |
| RESTRENEWLEVEL | 1 | 1 | 1 | 已实现 |
| TIMERECALL | 1 | 0 | 1 | 已实现 |

> 上表「可达」为纯文本口径。`BREAK`(73) 与 `DELAYCALL`(33) 各含 #CALL 内联高估（修正后 71 / 29）；受影响的 11 条行与反例见 §1.3。

### 3.2 部署中不在 `ExecutionCode` 枚举里的命令

| 命令 | 原始次数 | 可达次数 | 文件数 | 首个样本文件 | oracle 行为 |
|---|---:|---:|---:|---|---|
| `TAKEON` | 223 | 0 | 2 | `.\MapQuest_def\QManage.txt` | 解析期丢弃（nCMDCode=0，不进动作列表） |
| `GAMEGIRD` | 41 | 3 | 3 | `.\Market_Def\功能NPC\回收-3.txt` | 解析期丢弃（nCMDCode=0，不进动作列表） |
| `GAMEDIAMOND` | 37 | 18 | 2 | `.\Market_Def\功能NPC\回收-3.txt` | 解析期丢弃（nCMDCode=0，不进动作列表） |
| `}` | 11 | 2 | 11 | `.\QuestDiary\LEGM2.CC假人系统\假人初始化设置.txt` | 排版残留，解析失败后丢弃（文本可达 2 = `假人随机充值.txt:14`、`假人登录脚本.txt:401`；后者按 #CALL 内联不可达 ⇒ 内联口径 1） |
| `READRANDOMLINE` | 8 | 8 | 3 | `.\QuestDiary\LEGM2.CC假人系统\假人登录脚本.txt` | 解析期丢弃（nCMDCode=0，不进动作列表） |
| `ABILITYADD` | 6 | 0 | 1 | `.\Market_Def\LEGM2.CC假人NPC\假人上线NPC-jiaren.txt` | 解析期丢弃（nCMDCode=0，不进动作列表） |
| `CHANGEATTACKMODE` | 5 | 0 | 2 | `.\QuestDiary\LEGM2.CC假人系统\假人登录脚本.txt` | 解析期丢弃（nCMDCode=0，不进动作列表） |
| `SETOFFLINEPLAY` | 2 | 2 | 2 | `.\MapQuest_def\QManage - 副本.txt` | 解析期丢弃（nCMDCode=0，不进动作列表） |
| `WEBBROWSER` | 2 | 2 | 2 | `.\Market_Def\QFunction-0 - 副本.txt` | 解析期丢弃（nCMDCode=0，不进动作列表） |
| `SETSCTIMER` | 2 | 1 | 2 | `.\Market_Def\QFunction-0.txt` | 解析期丢弃（nCMDCode=0，不进动作列表） |
| `SENDSCROLLMSG` | 2 | 0 | 2 | `.\Market_Def\QFunction-0 - 副本.txt` | 解析期丢弃（nCMDCode=0，不进动作列表） |
| `RECALLHERO` | 2 | 0 | 1 | `.\QuestDiary\LEGM2.CC假人系统\假人登录脚本.txt` | 解析期丢弃（nCMDCode=0，不进动作列表） |
| `KILLSCTIMER` | 1 | 1 | 1 | `.\MapQuest_def\QManage.txt` | 解析期丢弃（nCMDCode=0，不进动作列表） |
| `OFFLINEPLAYEX` | 1 | 1 | 1 | `.\QuestDiary\LEGM2.CC假人系统\假人登录脚本.txt` | 解析期丢弃（nCMDCode=0，不进动作列表） |
| `DELLINELIST` | 1 | 0 | 1 | `.\Market_Def\功能NPC\桃源-N109.txt` | 解析期丢弃（nCMDCode=0，不进动作列表） |
| `ADDLINELIST` | 1 | 0 | 1 | `.\QuestDiary\游戏登陆\登陆脚本.txt` | 解析期丢弃（nCMDCode=0，不进动作列表） |

### 3.3 使用面三条硬事实

1. **`H.` 前缀不是独立动作**：`H.GIVE`(92)、`H.ADDSKILL`(40)、`H.HUMANHP/MP`、`H.CHANGELEVEL`、`H.REPAIRALL` 等
   经 oracle `ScriptParser` 规范化后就是 `GIVE`/`ADDSKILL`/…，且 C# 处理器**从不读取 `sOpName`/`sOpHName`**，
   所以 `H.GIVE` 等价于给主号 `GIVE`。Rust 的 `parser.rs` 复刻了同一规范化、`actions.rs` 同样忽略 op 名 ⇒ 语义一致。
2. **`<$STR(S9)>.GAMEPOINT` 等变量前缀同理**：解析器剥掉最后一段之前的全部内容，命令 = `GAMEPOINT`。
   Rust `parser.rs` 复刻该规则；部署中此类前缀共 29 行（12 SENDMSG / 10 GAMEPOINT / 2 MESSAGEBOX / 其余 5）。
3. **`DELAYCALL` 是最大「沉默死代码」**：部署 62 次、可达 33 次，但 `ExecutionCode.DelayCall=346` 没有注册；
   唯一注册的是 `DelayGoto=345 → ActionOfDelayCall`（`ExecutionProcessingSys.cs:124`）。
   因此脚本里的 `DelayCall 15000 @假人技能` 在 C# 里**根本不排队**，Rust 也一致 no-op。
   （这解释了为什么假人脚本大量依赖 `DelayCall` 却仍要别的机制；修复属 R 级，须 owner 裁决是否偏离 oracle。）

## 4. Rust 清册

- 注册表：`handlers.rs` 的 `EXECUTION_HANDLERS` 133 条，与 C# 注册逐条对齐。
- 已实现：`engine/dispatch.rs` 的 `IMPLEMENTED_EXECUTIONS` = **64** 个处理器名，
  与 `engine/actions.rs` 的 `run_action` match 分支**逐名相等**（本次核对：两边差集均为空）。
- 未实现：C# 注册但 Rust 无分支 = **68 个枚举动作 / 67 个唯一处理器**；
  引擎在命中时压入 `EngineError::NotImplemented{kind:"action"}` 并按 C# 的「执行后返回」语义停止。
- `run_switch_action` 覆盖 16 条动作 switch（`Break`/`Take`/`Takew`/`Goto`/`PARAM*`/`Batch*`/`GoQuest`/`EndQuest` 等）；
  `handlers.rs` 生成表另有 `CheckUserDate`（条件侧误列，见 §2.1 与 G-9），
  `TakecheckItem` 显式 `NotImplemented`，其余与 C# 等价（含空实现）。
  例外：`Break` 有一处静默偏差——Rust 直接返回 false **结束当前动作列表**，而 C# 只置 `result=false` 并继续当前列表
  （调用方在该列表跑完后才停后续 `#IF` 组）。部署中 BREAK 后仅有 9 条 `}`，无注册动作 ⇒ 当前零影响（缺口 G-11）。
- 解析器：`parser.rs` 对未知命令返回「解析失败」（C# 同样 `result=false`，不进动作列表）⇒ 非枚举命令两边都在解析期丢弃。

### 4.1 68 条 Rust 缺失动作（C# 已注册）

| # | 动作 | 处理器 | C# 行 | C# 是否空实现 |
|---:|---|---|---:|---|
| 1 | EXEACTION | `ActionOfExeaction` | 2024 | 是 |
| 2 | MONGEN | `ActionOfMonGen` | 2141 | 否 |
| 3 | MONCLEAR | `ActionOfMonClear` | 2154 | 否 |
| 4 | SUM | `ActionOfSumData` | 1378 | 否 |
| 5 | DIV | `ActionOfDivData` | 891 | 否 |
| 6 | PERCENT | `ActionOfPercentData` | 1223 | 否 |
| 7 | PKPOINT | `ActionOfPkPoint` | 2073 | 否 |
| 8 | RECALLMOB | `ActionOfRecallmob` | 5375 | 否 |
| 9 | EXCHANGEMAP | `ActionOfExchangeMap` | 2209 | 否 |
| 10 | RECALLMAP | `ActionOfReCallMap` | 2186 | 否 |
| 11 | ADDGUILDLIST | `ActionOfAddGuildList` | 5921 | 否 |
| 12 | DELGUILDLIST | `ActionOfDelGuildList` | 5929 | 否 |
| 13 | ADDACCOUNTLIST | `ActionOfAddAccountList` | 5906 | 是 |
| 14 | ADDIPLIST | `ActionOfAddIpList` | 5911 | 是 |
| 15 | DELIPLIST | `ActionOfDelIpList` | 5916 | 是 |
| 16 | MAPTING | `ActionOfMapTing` | 3209 | 是 |
| 17 | CLEARNAMELIST | `ActionOfClearList` | 3004 | 否 |
| 18 | KILLSLAVE | `ActionOfKillSlave` | 5440 | 否 |
| 19 | CHANGEGENDER | `ActionOfChangeGender` | 5428 | 否 |
| 20 | GETMARRY | `ActionOfGetMarry` | 3132 | 否 |
| 21 | GETMASTER | `ActionOfGetMaster` | 3147 | 否 |
| 22 | CLEARSKILL | `ActionOfClearSkill` | 3010 | 否 |
| 23 | DELNOJOBSKILL | `ActionOfDelNoJobSkill` | 3022 | 否 |
| 24 | DELSKILL | `ActionOfDelSkill` | 3036 | 否 |
| 25 | SKILLLEVEL | `ActionOfSkillLevel` | 3521 | 否 |
| 26 | CHANGEJOB | `ActionOfChangeJob` | 2880 | 否 |
| 27 | SETMEMBERTYPE | `ActionOfSetMemberType` | 4824 | 否 |
| 28 | SETMEMBERLEVEL | `ActionOfSetMemberLevel` | 4789 | 否 |
| 29 | AUTOADDGAMEGOLD | `ActionOfAutoAddGameGold` | 2667 | 否 |
| 30 | AUTOSUBGAMEGOLD | `ActionOfAutoSubGameGold` | 2733 | 否 |
| 31 | CHANGENAMECOLOR | `ActionOfChangeNameColor` | 5355 | 否 |
| 32 | CLEARPASSWORD | `ActionOfClearPassword` | 5367 | 否 |
| 33 | KILLMONEXPRATE | `ActionOfKillMonExpRate` | 5449 | 否 |
| 34 | POWERRATE | `ActionOfPowerRate` | 5539 | 否 |
| 35 | CHANGEPERMISSION | `ActionOfChangePerMission` | 5620 | 否 |
| 36 | BONUSPOINT | `ActionOfBonusPoint` | 5303 | 否 |
| 37 | DELMARRY | `ActionOfDelMarry` | 5328 | 否 |
| 38 | DELMASTER | `ActionOfDelMaster` | 5334 | 否 |
| 39 | MASTER | `ActionOfMaster` | 3334 | 否 |
| 40 | UNMASTER | `ActionOfUnMaster` | 4328 | 否 |
| 41 | CLEARNEEDITEMS | `ActionOfClearNeedItems` | 4239 | 否 |
| 42 | CLEARMAKEITEMS | `ActionOfClearMakeItems` | 4272 | 否 |
| 43 | UPGRADEITEM | `ActionOfUpgradeItems` | 3979 | 否 |
| 44 | SETMPAMODE | `ActionOfSetMapMode` | 4423 | 否 |
| 45 | PKZONE | `ActionOfPkZone` | 5509 | 是 |
| 46 | RESTBONUSPOINT | `ActionOfRestBonusPoint` | 5340 | 否 |
| 47 | TAKECASTLEGOLD | `ActionOfTakeCastleGold` | 3575 | 否 |
| 48 | GUILDBUILDPOINT | `ActionOfGuildBuildPoint` | 4997 | 否 |
| 49 | GUILDAURAEPOINT | `ActionOfGuildAuraePoint` | 4950 | 否 |
| 50 | GUILDSTABILITYPOINT | `ActionOfGuildstabilityPoint` | 5138 | 否 |
| 51 | GUILDFLOURISHPOINT | `ActionOfGuildFlourishPoint` | 5091 | 否 |
| 52 | OPENITEMBOX | `ActionOfOpenMagicBox` | 5490 | 否 |
| 53 | SETRANKLEVELNAME | `ActionOfSetRankLevelName` | 3491 | 否 |
| 54 | GUILDCHIEFITEMCOUNT | `ActionOfGuildChiefItemCount` | 5044 | 否 |
| 55 | ADDNAMEDATELIST | `ActionOfAddNameDateList` | 2572 | 否 |
| 56 | DELNAMEDATELIST | `ActionOfDelNameDateList` | 2609 | 否 |
| 57 | RANDOMMOVE | `ActionOfRandomMove` | 5837 | 是 |
| 58 | GUILDRECALL | `ActionOfGuildRecall` | 3897 | 是 |
| 59 | GROUPADDLIST | `ActionOfGroupAddList` | 3905 | 是 |
| 60 | CLEARLIST | `ActionOfClearList` | 3004 | 否 |
| 61 | GROUPMOVEMAP | `ActionOfGroupMoveMap` | 3954 | 否 |
| 62 | SETRANDOMNO | `ActionOfSetRandomNo` | 2112 | 否 |
| 63 | QUERYBAGITEMS | `ActionOfQueryBagItems` | 2306 | 否 |
| 64 | THROWITEM | `ActionOfThrowitem` | 5638 | 否 |
| 65 | OPENYBDEAL | `ActionOfOpenSaleDeal` | 2329 | 否 |
| 66 | QUERYYBSELL | `ActionOfQuerySaleSell` | 2363 | 否 |
| 67 | QUERYYBDEAL | `ActionOfQueryTrustDeal` | 2468 | 否 |
| 68 | KILLSLAVENAME | `ActionOfKillSlaveName` | 2247 | 否 |

这 68 条**没有一条出现在部署脚本的规范动作使用面里**（§3.1 的状态列无「缺失」）。
因此「部署可达动作」的 Rust 缺口 = **0**；上表是后续批次的存量清单。

## 5. 矩阵（352 个 oracle 动作 + 16 个非枚举命令）

图例：`RA:H` = `crates/script/src/engine/actions.rs` 的 `run_action("H")`；
`RS:M` = 同文件 `run_switch_action("M")`；`H` = `crates/script/src/handlers.rs` `EXECUTION_HANDLERS`；
`—（H 有注册，RA 无分支）` = 生成表有、`run_action` 未实现。

排序：有部署使用的在前（原始次数降序，其次可达次数降序），零使用的按枚举行号。
「部署 原始/可达」为纯文本口径（未模拟 `#CALL` 内联，高估上界；修正见 §1.3）。
「oracle 行号」的审计口径：`ExecutionCode.cs:N` 的 N = `[ScriptDefName]` **下一行的枚举成员行**（属性行 + 1，逐条实测）。

| # | 动作 | oracle 行号（枚举 / 注册或方法） | 部署 原始/可达 | Rust 落点 | 状态 | 可观测后果 | tier | 判据 |
|---:|---|---|---|---|---|---|---|---|
| 1 | `BREAK` | ExecutionCode.cs:27 / ScriptEngine.cs switch | 1074 / 73 | RS:Break | 引擎内建（与 C# 有偏差） | C# 置 result=false 后继续当前列表；Rust 直接结束当前列表（当前部署无影响） | L | L：符号级等价 + 抽样冒烟 |
| 2 | `SENDMSG` | ExecutionCode.cs:69 / ExecutionProcessingSys.cs:129 | 818 / 204 | RA:ActionOfLineMsg | 已实现 | 发送聊天气泡/广播（消息） | L | L：符号级等价 + ≥1 抽样冒烟 |
| 3 | `GIVE` | ExecutionCode.cs:13 / ExecutionProcessingSys.cs:39 | 571 / 236 | RA:ActionOfGiveItem | 已实现 | 给金币或入包物品（货币/物品） | R | R：真机全量对拍（状态/落库 diff=0） |
| 4 | `GOTO` | ExecutionCode.cs:117 / ScriptEngine.cs switch | 448 / 89 | RS:Goto | 引擎内建 | 跳转标签（控制流） | L | L：符号级等价 + 抽样冒烟 |
| 5 | `MESSAGEBOX` | ExecutionCode.cs:475 / ExecutionProcessingSys.cs:108 | 437 / 370 | RA:ActionOfMessageBox | 已实现 | 向客户端弹消息框（消息） | L | L：符号级等价 + ≥1 抽样冒烟 |
| 6 | `Take` | ExecutionCode.cs:11 / ScriptEngine.cs switch | 436 / 422 | RS:Take | 引擎内建 | 扣金币或背包物品（物品/货币） | R | R：真机对拍（物品/任务状态 diff=0） |
| 7 | `GAMEGOLD` | ExecutionCode.cs:389 / ExecutionProcessingSys.cs:66 | 286 / 274 | RA:ActionOfGameGold | 已实现 | 元宝增减/赋值，夹 0（货币） | R | R：真机全量对拍（状态/落库 diff=0） |
| 8 | `DELACCOUNTLIST` | ExecutionCode.cs:107 / ExecutionProcessingSys.cs:152 | 238 / 0 | RA:ActionOfDelAccountList | 已实现 | 账号名单文件移除（持久化） | R | R：真机全量对拍（状态/落库 diff=0） |
| 9 | `INC` | ExecutionCode.cs:55 / ExecutionProcessingSys.cs:161 | 211 / 14 | RA:ActionOfIncInteger | 已实现 | 变量自增（变量） | L | L：符号级等价 + ≥1 抽样冒烟 |
| 10 | `ADDSKILL` | ExecutionCode.cs:371 / ExecutionProcessingSys.cs:57 | 205 / 3 | RA:ActionOfAddSkill | 已实现 | 学会技能并重算属性（属性/消息） | R | R：真机全量对拍（状态/落库 diff=0） |
| 11 | `CLOSE` | ExecutionCode.cs:17 / ExecutionProcessingSys.cs:137 | 150 / 17 | RA:ActionOfClose | 已实现 | 关闭 NPC 对话框（消息） | L | L：符号级等价 + ≥1 抽样冒烟 |
| 12 | `BATCHDELAY` | ExecutionCode.cs:87 / ScriptEngine.cs switch | 144 / 144 | RS:BatchDelay | 引擎内建（等价 no-op） | 写入未使用局部变量（无效果） | L | L：符号级等价 + 抽样冒烟 |
| 13 | `BATCHMOVE` | ExecutionCode.cs:89 / ScriptEngine.cs switch | 144 / 0 | RS:BatchMove | 引擎内建（等价 no-op） | C# 实现被注释（无效果） | L | L：符号级等价 + 抽样冒烟 |
| 14 | `GMEXECUTE` | ExecutionCode.cs:465 / ExecutionProcessingSys.cs:103 | 139 / 61 | RA:ActionOfGmExecute | 已实现 | 执行 GM 命令（GM/全局） | R | R：真机全量对拍（状态/落库 diff=0） |
| 15 | `MAPMOVE` | ExecutionCode.cs:41 / ExecutionProcessingSys.cs:138 | 126 / 103 | RA:ActionOfMapMove | 已实现 | 传送到地图坐标（传送） | R | R：真机全量对拍（状态/落库 diff=0） |
| 16 | `MOBPLACE` | ExecutionCode.cs:383 / ExecutionProcessingSys.cs:63 | 111 / 0 | RA:ActionOfMobPlace | 已实现 | 按等级/范围放置怪物（怪物/RNG） | R | R：真机全量对拍（状态/落库 diff=0） |
| 17 | `GAMEPOINT` | ExecutionCode.cs:441 / ExecutionProcessingSys.cs:67 | 82 / 72 | RA:ActionOfGamePoint | 已实现 | 游戏点增减/赋值（货币） | R | R：真机全量对拍（状态/落库 diff=0） |
| 18 | `TAKEW` | ExecutionCode.cs:15 / ScriptEngine.cs switch | 71 / 11 | RS:Takew | 引擎内建 | 摘除佩戴装备（物品） | R | R：真机对拍（物品/任务状态 diff=0） |
| 19 | `CALCVAR` | ExecutionCode.cs:487 / ExecutionProcessingSys.cs:114 | 68 / 21 | RA:ActionOfCalcVar | 已实现 | 脚本变量四则/赋值运算（变量） | L | L：符号级等价 + ≥1 抽样冒烟 |
| 20 | `SAVEVAR` | ExecutionCode.cs:485 / ExecutionProcessingSys.cs:113 | 68 / 0 | RA:ActionOfSaveVar | 已实现 | 变量写入文件（持久化） | R | R：真机全量对拍（状态/落库 diff=0） |
| 21 | `MOVR` | ExecutionCode.cs:79 / ExecutionProcessingSys.cs:166 | 67 / 47 | RA:ActionOfMovrData | 已实现 | 变量赋随机数（变量/RNG） | R | R：真机全量对拍（状态/落库 diff=0） |
| 22 | `DELAYCALL` | ExecutionCode.cs:738 / — | 62 / 33 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 23 | `MOV` | ExecutionCode.cs:53 / ExecutionProcessingSys.cs:160 | 61 / 29 | RA:ActionOfMovData | 已实现 | 变量间搬运/赋值（变量） | L | L：符号级等价 + ≥1 抽样冒烟 |
| 24 | `UPGRADEDLGITEM` | ExecutionCode.cs:758 / ExecutionProcessingSys.cs:134 | 59 / 56 | RA:ActionOfUpgradeDlgItem | 已实现 | 升级对话框中的物品（物品/RNG） | R | R：真机全量对拍（状态/落库 diff=0） |
| 25 | `Set` | ExecutionCode.cs:9 / ExecutionProcessingSys.cs:34 | 47 / 37 | RA:ActionOfSet | 已实现 | 设置任务数值旗标（任务状态） | R | R：真机全量对拍（状态/落库 diff=0） |
| 26 | `CREDITPOINT` | ExecutionCode.cs:423 / ExecutionProcessingSys.cs:83 | 40 / 27 | RA:ActionOfChangeCreditPoint | 已实现 | 调整声望（货币/属性） | R | R：真机全量对拍（状态/落库 diff=0） |
| 27 | `UPGRADEITEMEX` | ExecutionCode.cs:433 / ExecutionProcessingSys.cs:88 | 34 / 29 | RA:ActionOfUpgradeItemsEx | 已实现 | 升级装备属性扩展（物品/RNG） | R | R：真机全量对拍（状态/落库 diff=0） |
| 28 | `MAP` | ExecutionCode.cs:43 / ExecutionProcessingSys.cs:139 | 24 / 18 | RA:ActionOfMap | 已实现 | 随机传送到地图（传送） | R | R：真机全量对拍（状态/落库 diff=0） |
| 29 | `VAR` | ExecutionCode.cs:481 / ExecutionProcessingSys.cs:111 | 24 / 2 | RA:ActionOfVar | 已实现 | 定义脚本变量（变量） | L | L：符号级等价 + ≥1 抽样冒烟 |
| 30 | `PLAYDICE` | ExecutionCode.cs:91 / ExecutionProcessingSys.cs:157 | 24 / 0 | RA:ActionOfPlayDice | 已实现 | 下发骰子消息（消息/变量） | L | L：符号级等价 + ≥1 抽样冒烟 |
| 31 | `LOADVAR` | ExecutionCode.cs:483 / ExecutionProcessingSys.cs:112 | 24 / 0 | RA:ActionOfLoadVar | 已实现 | 从文件载入变量（持久化） | R | R：真机全量对拍（状态/落库 diff=0） |
| 32 | `KICK` | ExecutionCode.cs:409 / ExecutionProcessingSys.cs:123 | 14 / 2 | RA:ActionOfKick | 已实现 | 置踢线标志（会话） | R | R：真机全量对拍（状态/落库 diff=0） |
| 33 | `QUERYVALUE` | ExecutionCode.cs:743 / ExecutionProcessingSys.cs:131 | 13 / 13 | RA:ActionOfQueryValue | 已实现 | 请求客户端输入（消息） | L | L：符号级等价 + ≥1 抽样冒烟 |
| 34 | `MUL` | ExecutionCode.cs:63 / ExecutionProcessingSys.cs:164 | 13 / 0 | RA:ActionOfMulData | 已实现 | 变量乘法（变量） | L | L：符号级等价 + ≥1 抽样冒烟 |
| 35 | `CLEARDELAYGOTO` | ExecutionCode.cs:495 / ExecutionProcessingSys.cs:155 | 10 / 0 | RA:ActionOfClearDelayGoto | 已实现 | 清除延迟跳转（时序） | R | R：真机全量对拍（状态/落库 diff=0） |
| 36 | `ADDNAMELIST` | ExecutionCode.cs:97 / ExecutionProcessingSys.cs:145 | 9 / 1 | RA:ActionOfAddNameList | 已实现 | 名单文件追加（持久化） | R | R：真机全量对拍（状态/落库 diff=0） |
| 37 | `HUMANHP` | ExecutionCode.cs:449 / ExecutionProcessingSys.cs:95 | 7 / 4 | RA:ActionOfHumanHp | 已实现 | 调整人物 HP（战斗数值） | R | R：真机全量对拍（状态/落库 diff=0） |
| 38 | `HUMANMP` | ExecutionCode.cs:451 / ExecutionProcessingSys.cs:96 | 7 / 2 | RA:ActionOfHumanMp | 已实现 | 调整人物 MP（战斗数值） | R | R：真机全量对拍（状态/落库 diff=0） |
| 39 | `CHANGEEXP` | ExecutionCode.cs:377 / ExecutionProcessingSys.cs:60 | 7 / 1 | RA:ActionOfChangeExp | 已实现 | 调整经验值（属性） | R | R：真机全量对拍（状态/落库 diff=0） |
| 40 | `REPAIRALL` | ExecutionCode.cs:698 / ExecutionProcessingSys.cs:120 | 6 / 2 | RA:ActionOfRepairAllItem | 已实现 | 特修全部佩戴装备（物品） | R | R：真机全量对拍（状态/落库 diff=0） |
| 41 | `DELAYGOTO` | ExecutionCode.cs:733 / ExecutionProcessingSys.cs:124 | 5 / 5 | RA:ActionOfDelayCall | 已实现 | 设置延迟调用标签（时序） | R | R：真机全量对拍（状态/落库 diff=0） |
| 42 | `QUERYITEMDLG` | ExecutionCode.cs:753 / ExecutionProcessingSys.cs:133 | 5 / 5 | RA:ActionOfQueryItemDlg | 已实现 | 打开物品选择对话框（消息） | L | L：符号级等价 + ≥1 抽样冒烟 |
| 43 | `CHANGELEVEL` | ExecutionCode.cs:355 / ExecutionProcessingSys.cs:47 | 5 / 1 | RA:ActionOfChangeLevel | 已实现 | 改变等级（属性） | R | R：真机全量对拍（状态/落库 diff=0） |
| 44 | `MARRY` | ExecutionCode.cs:357 / ExecutionProcessingSys.cs:48 | 4 / 4 | RA:ActionOfMarry | 已实现 | 结婚状态变更（会话/属性） | R | R：真机全量对拍（状态/落库 diff=0） |
| 45 | `UNMARRY` | ExecutionCode.cs:359 / ExecutionProcessingSys.cs:51 | 4 / 4 | RA:ActionOfUnMarry | 已实现 | 解除婚姻关系（持久化/会话） | R | R：真机全量对拍（状态/落库 diff=0） |
| 46 | `OFFLINE` | ExecutionCode.cs:693 / ExecutionProcessingSys.cs:68 | 4 / 0 | RA:ActionOfOffLine | 已实现 | 进入挂机离线（会话/属性/时序） | R | R：真机全量对拍（状态/落库 diff=0） |
| 47 | `MISSION` | ExecutionCode.cs:381 / ExecutionProcessingSys.cs:62 | 3 / 3 | RA:ActionOfMission | 已实现 | 设置全局任务地图坐标（任务/全局） | R | R：真机全量对拍（状态/落库 diff=0） |
| 48 | `PARAM1` | ExecutionCode.cs:31 / ScriptEngine.cs switch | 3 / 0 | RS:Param1 | 引擎内建 | 参数读入未使用局部变量（无效果） | L | L：符号级等价 + 抽样冒烟 |
| 49 | `PARAM2` | ExecutionCode.cs:33 / ScriptEngine.cs switch | 3 / 0 | RS:Param2 | 引擎内建 | 参数读入未使用局部变量（无效果） | L | L：符号级等价 + 抽样冒烟 |
| 50 | `PARAM3` | ExecutionCode.cs:35 / ScriptEngine.cs switch | 3 / 0 | RS:Param3 | 引擎内建 | 参数读入未使用局部变量（无效果） | L | L：符号级等价 + 抽样冒烟 |
| 51 | `PARAM4` | ExecutionCode.cs:37 / ScriptEngine.cs switch | 3 / 0 | RS:Param4 | 引擎内建 | 参数读入未使用局部变量（无效果） | L | L：符号级等价 + 抽样冒烟 |
| 52 | `DEC` | ExecutionCode.cs:57 / ExecutionProcessingSys.cs:159 | 3 / 0 | RA:ActionOfDecInteger | 已实现 | 变量自减（变量） | L | L：符号级等价 + ≥1 抽样冒烟 |
| 53 | `CHANGEMODE` | ExecutionCode.cs:71 / ExecutionProcessingSys.cs:121 | 3 / 0 | RA:ActionOfChangeMode | 已实现（C# 空实现） | C# 方法体整段注释 ⇒ 无效果 | L | L：符号级等价 + ≥1 抽样冒烟 |
| 54 | `CHANGEPKPOINT` | ExecutionCode.cs:375 / ExecutionProcessingSys.cs:59 | 3 / 0 | RA:ActionOfChangePkPoint | 已实现 | 调整 PK 值（属性） | R | R：真机全量对拍（状态/落库 diff=0） |
| 55 | `DELNAMELIST` | ExecutionCode.cs:99 / ExecutionProcessingSys.cs:146 | 2 / 2 | RA:ActionOfDelNameList | 已实现 | 名单文件移除（持久化） | R | R：真机全量对拍（状态/落库 diff=0） |
| 56 | `CLEARMAPMON` | ExecutionCode.cs:437 / ExecutionProcessingSys.cs:90 | 2 / 1 | RA:ActionOfClearMapMon | 已实现 | 清空地图怪物（怪物） | R | R：真机全量对拍（状态/落库 diff=0） |
| 57 | `HAIRSTYLE` | ExecutionCode.cs:123 / ExecutionProcessingSys.cs:43 | 2 / 0 | RA:ActionOfChangeHairStyle | 已实现 | 改变发型（外观） | L | L：符号级等价 + ≥1 抽样冒烟 |
| 58 | `KILL` | ExecutionCode.cs:407 / ExecutionProcessingSys.cs:122 | 2 / 0 | RA:ActionOfKill | 已实现 | 按模式杀死玩家（会话/战斗） | R | R：真机全量对拍（状态/落库 diff=0） |
| 59 | `SETSENDMSGFLAG` | ExecutionCode.cs:429 / ExecutionProcessingSys.cs:86 | 2 / 0 | RA:ActionOfSetSendMsgFlag | 已实现 | 设置收消息标志（消息） | L | L：符号级等价 + ≥1 抽样冒烟 |
| 60 | `MONGENEX` | ExecutionCode.cs:435 / ExecutionProcessingSys.cs:89 | 2 / 0 | RA:ActionOfMonGenEx | 已实现 | 扩展刷怪（怪物/RNG） | R | R：真机全量对拍（状态/落库 diff=0） |
| 61 | `SETSCRIPTFLAG` | ExecutionCode.cs:477 / ExecutionProcessingSys.cs:109 | 2 / 0 | RA:ActionOfSetScriptFlag | 已实现 | 设置脚本行为开关（消息/会话） | L | L：符号级等价 + ≥1 抽样冒烟 |
| 62 | `ADDUSERDATE` | ExecutionCode.cs:683 / ExecutionProcessingSys.cs:147 | 2 / 0 | RA:ActionOfAddUseDateList | 已实现 | 会员姓名+到期日写文件（持久化） | R | R：真机全量对拍（状态/落库 diff=0） |
| 63 | `DELUSERDATE` | ExecutionCode.cs:688 / ExecutionProcessingSys.cs:148 | 2 / 0 | RA:ActionOfDelUseDateList | 已实现 | 会员到期日移除（持久化） | R | R：真机全量对拍（状态/落库 diff=0） |
| 64 | `BREAKTIMERECALL` | ExecutionCode.cs:67 / ExecutionProcessingSys.cs:143 | 1 / 1 | RA:ActionOfBreakTimereCall | 已实现 | 取消定时召回（时序） | R | R：真机全量对拍（状态/落库 diff=0） |
| 65 | `RENEWLEVEL` | ExecutionCode.cs:399 / ExecutionProcessingSys.cs:73 | 1 / 1 | RA:ActionOfReNewLevel | 已实现 | 转生等级变更（属性/货币） | R | R：真机全量对拍（状态/落库 diff=0） |
| 66 | `RESTRENEWLEVEL` | ExecutionCode.cs:413 / ExecutionProcessingSys.cs:80 | 1 / 1 | RA:ActionOfRestReNewLevel | 已实现 | 重置转生等级（属性） | R | R：真机全量对拍（状态/落库 diff=0） |
| 67 | `SETAUTOGETEXP` | ExecutionCode.cs:479 / ExecutionProcessingSys.cs:110 | 1 / 1 | RA:ActionOfAutoGetExp | 已实现 | 安全区自动获得经验（属性） | R | R：真机全量对拍（状态/落库 diff=0） |
| 68 | `TIMERECALL` | ExecutionCode.cs:29 / ExecutionProcessingSys.cs:142 | 1 / 0 | RA:ActionOfTimereCall | 已实现 | 定时召回（时序） | R | R：真机全量对拍（状态/落库 diff=0） |
| 69 | `RESET` | ExecutionCode.cs:19 / ExecutionProcessingSys.cs:35 | 0 / 0 | RA:ActionOfReSet | 已实现 | 批量清零任务旗标（任务状态） | R | R：真机全量对拍（状态/落库 diff=0） |
| 70 | `SETOPEN` | ExecutionCode.cs:21 / ExecutionProcessingSys.cs:36 | 0 / 0 | RA:ActionOfSetOpen | 已实现 | 设置任务开关旗标（任务状态） | R | R：真机全量对拍（状态/落库 diff=0） |
| 71 | `SETUNIT` | ExecutionCode.cs:23 / ExecutionProcessingSys.cs:37 | 0 / 0 | RA:ActionOfSetUnit | 已实现 | 设置单位数值旗标（任务状态） | R | R：真机全量对拍（状态/落库 diff=0） |
| 72 | `RESETUNIT` | ExecutionCode.cs:25 / ExecutionProcessingSys.cs:38 | 0 / 0 | RA:ActionOfResetUnit | 已实现 | 批量清零单位旗标（任务状态） | R | R：真机全量对拍（状态/落库 diff=0） |
| 73 | `EXEACTION` | ExecutionCode.cs:39 / ExecutionProcessingSys.cs:158 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失（C# 空实现） | C# 方法体为空 ⇒ 无效果 | L | L：实现后符号级等价（C# 空实现） |
| 74 | `TAKECHECKITEM` | ExecutionCode.cs:45 / ScriptEngine.cs switch | 0 / 0 | RS:TakecheckItem | 部分（显式 NotImplemented） | 从对话框取物品（物品；Rust 未实现） | R | R：补齐后真机对拍（当前部署可达=0） |
| 75 | `MONGEN` | ExecutionCode.cs:47 / ExecutionProcessingSys.cs:141 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 刷怪（怪物/RNG） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 76 | `MONGENP` | ExecutionCode.cs:49 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 77 | `MONCLEAR` | ExecutionCode.cs:51 / ExecutionProcessingSys.cs:140 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 清空地图怪物（怪物） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 78 | `SUM` | ExecutionCode.cs:59 / ExecutionProcessingSys.cs:162 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 变量加法（变量） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 79 | `DIV` | ExecutionCode.cs:61 / ExecutionProcessingSys.cs:163 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 变量除法（变量） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 80 | `PERCENT` | ExecutionCode.cs:65 / ExecutionProcessingSys.cs:165 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 变量百分比运算（变量） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 81 | `PKPOINT` | ExecutionCode.cs:73 / ExecutionProcessingSys.cs:144 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 重置 PK 值（属性） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 82 | `CHANGEXP` | ExecutionCode.cs:75 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 83 | `RECALLMOB` | ExecutionCode.cs:77 / ExecutionProcessingSys.cs:42 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 召唤宝宝/宠物（怪物） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 84 | `EXCHANGEMAP` | ExecutionCode.cs:81 / ExecutionProcessingSys.cs:135 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 与目标交换地图/坐标（传送） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 85 | `RECALLMAP` | ExecutionCode.cs:83 / ExecutionProcessingSys.cs:136 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 把地图玩家召回到 NPC（传送） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 86 | `ADDBATCH` | ExecutionCode.cs:85 / ScriptEngine.cs switch | 0 / 0 | RS:AddBatch | 引擎内建（等价 no-op） | C# 实现被注释（无效果） | L | L：符号级等价 + 抽样冒烟 |
| 87 | `PASTEMAP` | ExecutionCode.cs:93 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 88 | `LOADGEN` | ExecutionCode.cs:95 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 89 | `ADDGUILDLIST` | ExecutionCode.cs:101 / ExecutionProcessingSys.cs:149 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 公会名单文件追加（持久化/公会） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 90 | `DELGUILDLIST` | ExecutionCode.cs:103 / ExecutionProcessingSys.cs:150 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 公会名单文件移除（持久化/公会） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 91 | `ADDACCOUNTLIST` | ExecutionCode.cs:105 / ExecutionProcessingSys.cs:151 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失（C# 空实现） | 账号名单文件追加（持久化） | L | L：实现后符号级等价（C# 空实现） |
| 92 | `ADDIPLIST` | ExecutionCode.cs:109 / ExecutionProcessingSys.cs:153 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失（C# 空实现） | IP 名单文件追加（持久化） | L | L：实现后符号级等价（C# 空实现） |
| 93 | `DELIPLIST` | ExecutionCode.cs:111 / ExecutionProcessingSys.cs:154 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失（C# 空实现） | IP 名单文件移除（持久化） | L | L：实现后符号级等价（C# 空实现） |
| 94 | `GOQUEST` | ExecutionCode.cs:113 / ScriptEngine.cs switch | 0 / 0 | RS:GoQuest | 引擎内建 | 进入任务脚本（任务状态） | R | R：真机对拍（物品/任务状态 diff=0） |
| 95 | `ENDQUEST` | ExecutionCode.cs:115 / ScriptEngine.cs switch | 0 / 0 | RS:EndQuest | 引擎内建 | 退出任务脚本（任务状态） | R | R：真机对拍（物品/任务状态 diff=0） |
| 96 | `HAIRCOLOR` | ExecutionCode.cs:119 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 97 | `WEARCOLOR` | ExecutionCode.cs:121 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 98 | `MONRECALL` | ExecutionCode.cs:125 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 99 | `HORSECALL` | ExecutionCode.cs:127 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 100 | `HAIRRNDCOL` | ExecutionCode.cs:129 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 101 | `RANDSETDAILYQUEST` | ExecutionCode.cs:131 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 102 | `REFINEWEAPON` | ExecutionCode.cs:133 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 103 | `RECALLGROUPMEMBERS` | ExecutionCode.cs:135 / ExecutionProcessingSys.cs:44 | 0 / 0 | RA:ActionOfRecallGroupMembers | 已实现（C# 空实现） | C# 方法体为空 ⇒ 无效果 | L | L：符号级等价 + ≥1 抽样冒烟 |
| 104 | `MAPTING` | ExecutionCode.cs:137 / ExecutionProcessingSys.cs:46 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失（C# 空实现） | C# 方法体为空 ⇒ 无效果 | L | L：实现后符号级等价（C# 空实现） |
| 105 | `WRITEWEAPONNAME` | ExecutionCode.cs:139 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 106 | `ENABLECMD` | ExecutionCode.cs:141 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 107 | `LINEMSG` | ExecutionCode.cs:143 / ExecutionProcessingSys.cs:128 | 0 / 0 | RA:ActionOfLineMsg | 已实现 | 发送聊天气泡/广播（消息） | L | L：符号级等价 + ≥1 抽样冒烟 |
| 108 | `EVENTMSG` | ExecutionCode.cs:145 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 109 | `SOUNDMSG` | ExecutionCode.cs:147 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 110 | `SETMISSION` | ExecutionCode.cs:149 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 111 | `CLEARMISSION` | ExecutionCode.cs:151 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 112 | `MONPWR` | ExecutionCode.cs:153 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 113 | `ENTER_OK` | ExecutionCode.cs:155 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 114 | `ENTER_FAIL` | ExecutionCode.cs:157 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 115 | `MONADDITEM` | ExecutionCode.cs:159 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 116 | `CHANGEWEATHER` | ExecutionCode.cs:161 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 117 | `CHANGEWEAPONATOM` | ExecutionCode.cs:163 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 118 | `GETREPAIRCOST` | ExecutionCode.cs:165 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 119 | `KILLHORSE` | ExecutionCode.cs:167 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 120 | `REPAIRITEM` | ExecutionCode.cs:169 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 121 | `USEREMERGENCYCLOSE` | ExecutionCode.cs:171 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 122 | `BUILDGUILD` | ExecutionCode.cs:173 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 123 | `GUILDWAR` | ExecutionCode.cs:175 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 124 | `CHANGEUSERNAME` | ExecutionCode.cs:177 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 125 | `CHANGEMONLEVEL` | ExecutionCode.cs:179 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 126 | `DROPITEMMAP` | ExecutionCode.cs:181 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 127 | `CLEARITEMMAP` | ExecutionCode.cs:183 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 128 | `PROPOSECASTLEWAR` | ExecutionCode.cs:185 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 129 | `FINISHCASTLEWAR` | ExecutionCode.cs:187 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 130 | `MOVENPC` | ExecutionCode.cs:189 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 131 | `SPEAK` | ExecutionCode.cs:191 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 132 | `SENDCMD` | ExecutionCode.cs:193 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 133 | `INCFAME` | ExecutionCode.cs:195 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 134 | `DECFAME` | ExecutionCode.cs:197 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 135 | `CAPTURECASTLEFLAG` | ExecutionCode.cs:199 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 136 | `MAKESHOOTER` | ExecutionCode.cs:201 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 137 | `KILLSHOOTER` | ExecutionCode.cs:203 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 138 | `LEAVESHOOTER` | ExecutionCode.cs:205 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 139 | `CHANGEMAPATTR` | ExecutionCode.cs:207 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 140 | `RESETMAPATTR` | ExecutionCode.cs:209 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 141 | `MAKECASTLEDOOR` | ExecutionCode.cs:211 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 142 | `REPAIRCASTLEDOOR` | ExecutionCode.cs:213 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 143 | `CHARGESHOOTER` | ExecutionCode.cs:215 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 144 | `SETAREAATTR` | ExecutionCode.cs:217 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 145 | `TESTFLAG` | ExecutionCode.cs:219 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 146 | `APPLYFLAG` | ExecutionCode.cs:221 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 147 | `PASTEFLAG` | ExecutionCode.cs:223 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 148 | `GETBACKCASTLEGOLD` | ExecutionCode.cs:225 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 149 | `GETBACKUPGITEM` | ExecutionCode.cs:227 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 150 | `TINGWAR` | ExecutionCode.cs:229 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 151 | `SAVEPASSWD` | ExecutionCode.cs:231 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 152 | `CREATENPC` | ExecutionCode.cs:233 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 153 | `TAKEBONUS` | ExecutionCode.cs:235 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 154 | `SYSMSG` | ExecutionCode.cs:237 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 155 | `LOADVALUE` | ExecutionCode.cs:239 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 156 | `SAVEVALUE` | ExecutionCode.cs:241 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 157 | `SAVELOG` | ExecutionCode.cs:243 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 158 | `GETMARRIED` | ExecutionCode.cs:245 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 159 | `DIVORCE` | ExecutionCode.cs:247 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 160 | `CAPTURESAYING` | ExecutionCode.cs:249 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 161 | `CANCELMARRIAGERING` | ExecutionCode.cs:251 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 162 | `OPENUSERMARKET` | ExecutionCode.cs:253 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 163 | `SETTYPEUSERMARKET` | ExecutionCode.cs:255 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 164 | `CHECKSOLDITEMSUSERMARKET` | ExecutionCode.cs:257 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 165 | `SETGMEMAP` | ExecutionCode.cs:259 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 166 | `SETGMEPOINT` | ExecutionCode.cs:261 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 167 | `SETGMETIME` | ExecutionCode.cs:263 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 168 | `STARTNEWGME` | ExecutionCode.cs:265 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 169 | `MOVETOGMEMAP` | ExecutionCode.cs:267 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 170 | `FINISHGME` | ExecutionCode.cs:269 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 171 | `CONTINUEGME` | ExecutionCode.cs:271 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 172 | `SETGMEPLAYTIME` | ExecutionCode.cs:273 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 173 | `SETGMEPAUSETIME` | ExecutionCode.cs:275 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 174 | `SETGMELIMITUSER` | ExecutionCode.cs:277 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 175 | `SETEVENTMAP` | ExecutionCode.cs:279 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 176 | `RESETEVENTMAP` | ExecutionCode.cs:281 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 177 | `TESTREFINEPOINTS` | ExecutionCode.cs:283 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 178 | `RESETREFINEWEAPON` | ExecutionCode.cs:285 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 179 | `TESTREFINEACCESSORIES` | ExecutionCode.cs:287 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 180 | `REFINEACCESSORIES` | ExecutionCode.cs:289 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 181 | `APPLYMONMISSION` | ExecutionCode.cs:291 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 182 | `MAPMOVER` | ExecutionCode.cs:293 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 183 | `ADDSTR` | ExecutionCode.cs:295 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 184 | `SETEVENTDAMAGE` | ExecutionCode.cs:297 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 185 | `FORMATSTR` | ExecutionCode.cs:299 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 186 | `CLEARPATH` | ExecutionCode.cs:301 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 187 | `ADDPATH` | ExecutionCode.cs:303 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 188 | `APPLYPATH` | ExecutionCode.cs:305 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 189 | `MAPSPELL` | ExecutionCode.cs:307 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 190 | `GIVEEXP` | ExecutionCode.cs:309 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 191 | `GROUPMOVE` | ExecutionCode.cs:311 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 192 | `GIVEEXPMAP` | ExecutionCode.cs:313 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 193 | `APPLYMONEX` | ExecutionCode.cs:315 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 194 | `CLEARNAMELIST` | ExecutionCode.cs:317 / ExecutionProcessingSys.cs:45 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 清空名单文件（持久化） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 195 | `TINGCASTLEVISITOR` | ExecutionCode.cs:319 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 196 | `MAKEHEALZONE` | ExecutionCode.cs:321 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 197 | `MAKEDAMAGEZONE` | ExecutionCode.cs:323 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 198 | `CLEARZONE` | ExecutionCode.cs:325 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 199 | `READVALUESQL` | ExecutionCode.cs:327 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 200 | `READSTRINGSQL` | ExecutionCode.cs:329 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 201 | `WRITEVALUESQL` | ExecutionCode.cs:331 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 202 | `INCVALUESQL` | ExecutionCode.cs:333 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 203 | `DECVALUESQL` | ExecutionCode.cs:335 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 204 | `UPDATEVALUESQL` | ExecutionCode.cs:337 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 205 | `KILLSLAVE` | ExecutionCode.cs:339 / ExecutionProcessingSys.cs:74 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 杀死全部宠物（怪物） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 206 | `SETITEMEVENT` | ExecutionCode.cs:341 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 207 | `REMOVEITEMEVENT` | ExecutionCode.cs:343 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 208 | `RETURN` | ExecutionCode.cs:345 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 209 | `CLEARCASTLEOWNER` | ExecutionCode.cs:347 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 210 | `DISSOLUTIONGUILD` | ExecutionCode.cs:349 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 211 | `CHANGEGENDER` | ExecutionCode.cs:351 / ExecutionProcessingSys.cs:75 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 改变性别（属性） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 212 | `SETFAME` | ExecutionCode.cs:353 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 213 | `GETMARRY` | ExecutionCode.cs:361 / ExecutionProcessingSys.cs:52 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 配偶名写入变量（变量） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 214 | `GETMASTER` | ExecutionCode.cs:363 / ExecutionProcessingSys.cs:53 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 师傅名写入变量（变量） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 215 | `CLEARSKILL` | ExecutionCode.cs:365 / ExecutionProcessingSys.cs:54 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 清空全部技能（属性） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 216 | `DELNOJOBSKILL` | ExecutionCode.cs:367 / ExecutionProcessingSys.cs:55 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 删除非本职业技能（属性） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 217 | `DELSKILL` | ExecutionCode.cs:369 / ExecutionProcessingSys.cs:56 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 删除指定技能（属性） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 218 | `SKILLLEVEL` | ExecutionCode.cs:373 / ExecutionProcessingSys.cs:58 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 设置技能等级（属性） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 219 | `CHANGEJOB` | ExecutionCode.cs:379 / ExecutionProcessingSys.cs:61 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 改变职业（属性） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 220 | `SETMEMBERTYPE` | ExecutionCode.cs:385 / ExecutionProcessingSys.cs:64 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 设置会员类型（会话/属性） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 221 | `SETMEMBERLEVEL` | ExecutionCode.cs:387 / ExecutionProcessingSys.cs:65 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 设置会员等级（属性） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 222 | `AUTOADDGAMEGOLD` | ExecutionCode.cs:391 / ExecutionProcessingSys.cs:69 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 按时间自动加元宝（货币） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 223 | `AUTOSUBGAMEGOLD` | ExecutionCode.cs:393 / ExecutionProcessingSys.cs:70 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 按时间自动扣元宝（货币） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 224 | `CHANGENAMECOLOR` | ExecutionCode.cs:395 / ExecutionProcessingSys.cs:71 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 改变名字颜色（外观） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 225 | `CLEARPASSWORD` | ExecutionCode.cs:397 / ExecutionProcessingSys.cs:72 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 清除仓库密码锁（账号） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 226 | `KILLMONEXPRATE` | ExecutionCode.cs:401 / ExecutionProcessingSys.cs:76 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 设置杀怪经验倍率（属性） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 227 | `POWERRATE` | ExecutionCode.cs:403 / ExecutionProcessingSys.cs:77 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 设置战力倍率与时限（属性） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 228 | `CHANGEPERMISSION` | ExecutionCode.cs:405 / ExecutionProcessingSys.cs:78 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 改变权限（会话/账号） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 229 | `BONUSPOINT` | ExecutionCode.cs:411 / ExecutionProcessingSys.cs:79 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 调整可分配点数（属性） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 230 | `DELMARRY` | ExecutionCode.cs:415 / ExecutionProcessingSys.cs:81 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 强制离婚并刷新称谓（会话） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 231 | `DELMASTER` | ExecutionCode.cs:417 / ExecutionProcessingSys.cs:82 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 强制解除师徒（会话） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 232 | `MASTER` | ExecutionCode.cs:419 / ExecutionProcessingSys.cs:49 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 拜师状态变更（会话） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 233 | `UNMASTER` | ExecutionCode.cs:421 / ExecutionProcessingSys.cs:50 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 解除师徒关系（会话） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 234 | `CLEARNEEDITEMS` | ExecutionCode.cs:425 / ExecutionProcessingSys.cs:84 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 清空需求物品列表（物品） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 235 | `CLEARMAKEITEMS` | ExecutionCode.cs:427 / ExecutionProcessingSys.cs:85 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 清空制作物品列表（物品） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 236 | `UPGRADEITEM` | ExecutionCode.cs:431 / ExecutionProcessingSys.cs:87 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 升级装备属性（物品/RNG） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 237 | `SETMPAMODE` | ExecutionCode.cs:439 / ExecutionProcessingSys.cs:91 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 设置地图模式（公会/全局） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 238 | `PKZONE` | ExecutionCode.cs:443 / ExecutionProcessingSys.cs:92 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失（C# 空实现） | C# 方法体整段注释 ⇒ 无效果 | L | L：实现后符号级等价（C# 空实现） |
| 239 | `RESTBONUSPOINT` | ExecutionCode.cs:445 / ExecutionProcessingSys.cs:93 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 重置已分配点数（属性） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 240 | `TAKECASTLEGOLD` | ExecutionCode.cs:447 / ExecutionProcessingSys.cs:94 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 取城堡金币（货币） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 241 | `GUILDBUILDPOINT` | ExecutionCode.cs:453 / ExecutionProcessingSys.cs:97 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 调整公会建设点（公会） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 242 | `GUILDAURAEPOINT` | ExecutionCode.cs:455 / ExecutionProcessingSys.cs:98 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 调整公会灵气点（公会） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 243 | `GUILDSTABILITYPOINT` | ExecutionCode.cs:457 / ExecutionProcessingSys.cs:99 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 调整公会稳定点（公会） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 244 | `GUILDFLOURISHPOINT` | ExecutionCode.cs:459 / ExecutionProcessingSys.cs:100 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 调整公会繁荣点（公会） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 245 | `OPENITEMBOX` | ExecutionCode.cs:461 / ExecutionProcessingSys.cs:101 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 开宝箱（物品/怪物） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 246 | `SETRANKLEVELNAME` | ExecutionCode.cs:463 / ExecutionProcessingSys.cs:102 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 设置称号并刷新显示（属性） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 247 | `GUILDCHIEFITEMCOUNT` | ExecutionCode.cs:467 / ExecutionProcessingSys.cs:104 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 设置会长物品数（公会） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 248 | `ADDNAMEDATELIST` | ExecutionCode.cs:469 / ExecutionProcessingSys.cs:105 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 名单+日期文件追加（持久化） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 249 | `DELNAMEDATELIST` | ExecutionCode.cs:471 / ExecutionProcessingSys.cs:106 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 名单+日期文件移除（持久化） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 250 | `MOBFIREBURN` | ExecutionCode.cs:473 / ExecutionProcessingSys.cs:107 | 0 / 0 | RA:ActionOfMobFireBurn | 已实现（C# 空实现） | 参数校验后地图火焰效果整段被注释 ⇒ 无效果 | L | L：符号级等价 + ≥1 抽样冒烟 |
| 251 | `OFFLINEPLAY` | ExecutionCode.cs:489 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 252 | `KICKOFFLINE` | ExecutionCode.cs:491 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 253 | `STARTTAKEGOLD` | ExecutionCode.cs:493 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 254 | `CHANGERECOMMENDGAMEGOLD` | ExecutionCode.cs:497 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 255 | `ANSIREPLACETEXT` | ExecutionCode.cs:499 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 256 | `ENCODETEXT` | ExecutionCode.cs:501 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 257 | `DECODETEXT` | ExecutionCode.cs:503 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 258 | `ADDTEXTLIST` | ExecutionCode.cs:505 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 259 | `DELTEXTLIST` | ExecutionCode.cs:507 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 260 | `GROUPMAPMOVE` | ExecutionCode.cs:509 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 261 | `RECALLHUMAN` | ExecutionCode.cs:511 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 262 | `REGOTO` | ExecutionCode.cs:513 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 263 | `INTTOSTR` | ExecutionCode.cs:515 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 264 | `STRTOINT` | ExecutionCode.cs:517 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 265 | `GUILDMOVE` | ExecutionCode.cs:519 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 266 | `GUILDMAPMOVE` | ExecutionCode.cs:521 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 267 | `RANDOMMOVE` | ExecutionCode.cs:523 / ExecutionProcessingSys.cs:41 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失（C# 空实现） | C# 方法体为空（仅 Success=true）⇒ 无效果 | L | L：实现后符号级等价（C# 空实现） |
| 268 | `USEBONUSPOINT` | ExecutionCode.cs:525 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 269 | `TAKEONITEM` | ExecutionCode.cs:527 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 270 | `TAKEOFFITEM` | ExecutionCode.cs:529 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 271 | `GIVEEX` | ExecutionCode.cs:531 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 272 | `TIMEOPEN` | ExecutionCode.cs:533 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 273 | `TIMECLOSE` | ExecutionCode.cs:535 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 274 | `GUILDMEMBERMAXLIMIT` | ExecutionCode.cs:537 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 275 | `ADDGUILDNAMEDATELIST` | ExecutionCode.cs:539 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 276 | `DELGUILDNAMEDATELIST` | ExecutionCode.cs:541 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 277 | `GOHOME` | ExecutionCode.cs:543 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 278 | `ADDBLOCKIPLIST` | ExecutionCode.cs:545 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 279 | `MOVEDATA` | ExecutionCode.cs:547 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 280 | `SENDCOLORMSG` | ExecutionCode.cs:549 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 281 | `ADDRANDOMMAPGATE` | ExecutionCode.cs:551 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 282 | `DELRANDOMMAPGATE` | ExecutionCode.cs:553 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 283 | `GETRANDOMMAPGATE` | ExecutionCode.cs:555 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 284 | `OPENBOOK` | ExecutionCode.cs:557 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 285 | `OPENBOX` | ExecutionCode.cs:559 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 286 | `CHANGEITEMS` | ExecutionCode.cs:561 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 287 | `CLEARREMEMBER` | ExecutionCode.cs:563 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 288 | `SENDMOVEMSG` | ExecutionCode.cs:565 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 289 | `SETITEMSLIGHT` | ExecutionCode.cs:567 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 290 | `READRANDOMSTR` | ExecutionCode.cs:569 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 291 | `CHANGERANGEMONPOS` | ExecutionCode.cs:571 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 292 | `READ` | ExecutionCode.cs:573 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 293 | `WRITE` | ExecutionCode.cs:575 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 294 | `CHANGEITEMNEWADDVALUE` | ExecutionCode.cs:577 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 295 | `OPENHOMEPAGE` | ExecutionCode.cs:579 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 296 | `PKZONEX` | ExecutionCode.cs:581 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 297 | `SETITEMSLIGHTEX` | ExecutionCode.cs:583 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 298 | `SPELL` | ExecutionCode.cs:585 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 299 | `ADDMAPMAGICEVENT` | ExecutionCode.cs:587 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 300 | `RANDOMADDMAPMAGICEVENT` | ExecutionCode.cs:589 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 301 | `SNOW` | ExecutionCode.cs:591 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 302 | `RANDOMUPGRADEITEM` | ExecutionCode.cs:593 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 303 | `LOCK` | ExecutionCode.cs:595 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 304 | `UNLOCK` | ExecutionCode.cs:597 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 305 | `GETDUELITEMS` | ExecutionCode.cs:599 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 306 | `CANUSEITEM` | ExecutionCode.cs:601 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 307 | `AUTOGOTOXY` | ExecutionCode.cs:603 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 308 | `CHANGENEWSTATUS` | ExecutionCode.cs:605 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 309 | `SENDDELAYMSG` | ExecutionCode.cs:607 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 310 | `ACTVARLIST` | ExecutionCode.cs:609 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 311 | `SETMASKED` | ExecutionCode.cs:611 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 312 | `SENDCENTERMSG` | ExecutionCode.cs:613 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 313 | `CHANGEUSEITEMSTARSLEVEL` | ExecutionCode.cs:615 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 314 | `CHANGEBAGITEMSTARSLEVEL` | ExecutionCode.cs:617 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 315 | `BINDBAGITEM` | ExecutionCode.cs:619 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 316 | `SETONTIMER` | ExecutionCode.cs:621 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 317 | `SETOFFTIMER` | ExecutionCode.cs:623 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 318 | `PLAYSOUND` | ExecutionCode.cs:625 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 319 | `SHOWEFFECT` | ExecutionCode.cs:627 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 320 | `CHANGEITEMVALUE` | ExecutionCode.cs:629 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 321 | `VIBRATION` | ExecutionCode.cs:631 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 322 | `OPENBIGDIALOGBOX` | ExecutionCode.cs:633 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 323 | `CLOSEBIGDIALOGBOX` | ExecutionCode.cs:635 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 324 | `TAGMAPMOVE` | ExecutionCode.cs:637 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 325 | `TAGMAPINFO` | ExecutionCode.cs:639 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 326 | `AISTART` | ExecutionCode.cs:641 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 327 | `AISTOP` | ExecutionCode.cs:643 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 328 | `CHANGEIPADDRESS` | ExecutionCode.cs:645 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 329 | `CHANGEATTATCKMODE` | ExecutionCode.cs:647 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 330 | `AILOGON` | ExecutionCode.cs:649 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 331 | `AILOGONEX` | ExecutionCode.cs:651 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 332 | `KICKALL` | ExecutionCode.cs:653 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 333 | `TAKEITEMLIST` | ExecutionCode.cs:655 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 334 | `SHOWRANKLEVLNAME` | ExecutionCode.cs:657 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 335 | `LOADROBOTCONFIG` | ExecutionCode.cs:659 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 336 | `ACTMISSION` | ExecutionCode.cs:661 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 337 | `GUILDRECALL` | ExecutionCode.cs:663 / ExecutionProcessingSys.cs:115 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失（C# 空实现） | C# 召回调用被注释 ⇒ 无效果 | L | L：实现后符号级等价（C# 空实现） |
| 338 | `GROUPADDLIST` | ExecutionCode.cs:665 / ExecutionProcessingSys.cs:116 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失（C# 空实现） | C# 仅遍历，写入被注释 ⇒ 无效果 | L | L：实现后符号级等价（C# 空实现） |
| 339 | `CLEARLIST` | ExecutionCode.cs:667 / ExecutionProcessingSys.cs:117 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 清空名单文件（持久化） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 340 | `GROUPRECALL` | ExecutionCode.cs:669 / ExecutionProcessingSys.cs:118 | 0 / 0 | RA:ActionOfGroupRecall | 已实现（C# 空实现） | C# 召回调用被注释 ⇒ 无效果 | L | L：符号级等价 + ≥1 抽样冒烟 |
| 341 | `GROUPMOVEMAP` | ExecutionCode.cs:671 / ExecutionProcessingSys.cs:119 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 队长把全队传送到坐标（传送/组队） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 342 | `SAVESLAVES` | ExecutionCode.cs:673 / — | 0 / 0 | — | oracle 无处理器（执行 no-op） | —（C# 解析成功、动作路径无分支 ⇒ no-op） | L | 静态等价：C# 无处理器 ⇒ Rust 同样无行为 |
| 343 | `CHECKUSERDATE` | ExecutionCode.cs:678 / ScriptEngine.cs 条件 switch（已注释） | 0 / 0 | RS:CheckUserDate（生成表） | 生成表误列（等价 no-op） | 动作路径无此 case（无效果；条件 switch 已注释） | L | L：符号级等价 + 抽样冒烟 |
| 344 | `SETRANDOMNO` | ExecutionCode.cs:703 / ExecutionProcessingSys.cs:156 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 生成 0..999998 随机数（变量/RNG） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 345 | `QUERYBAGITEMS` | ExecutionCode.cs:708 / ExecutionProcessingSys.cs:130 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 刷新包裹给客户端（消息/物品） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 346 | `THROWITEM` | ExecutionCode.cs:713 / ExecutionProcessingSys.cs:40 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 把物品扔到地面（物品/货币） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 347 | `OPENYBDEAL` | ExecutionCode.cs:718 / ExecutionProcessingSys.cs:125 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 打开元宝交易界面（货币/消息） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 348 | `QUERYYBSELL` | ExecutionCode.cs:723 / ExecutionProcessingSys.cs:126 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 查询元宝寄售列表（货币/物品/消息） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 349 | `QUERYYBDEAL` | ExecutionCode.cs:728 / ExecutionProcessingSys.cs:127 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 查询元宝交易列表（货币/物品/消息） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 350 | `KILLSLAVENAME` | ExecutionCode.cs:748 / ExecutionProcessingSys.cs:132 | 0 / 0 | —（H 有注册，RA 无分支） | 缺失 | 杀死指定名字宠物（怪物） | R | R：实现后真机对拍（当前部署可达=0，不占窗口） |
| 351 | `GETDLGITEMVALUE` | ExecutionCode.cs:763 / ScriptEngine.cs switch | 0 / 0 | RS:GetDlgItemValue | 引擎内建（等价 no-op） | C# 空实现（无效果） | L | L：符号级等价 + 抽样冒烟 |
| 352 | `TAKEDLGITEM` | ExecutionCode.cs:768 / ScriptEngine.cs switch | 0 / 0 | RS:TakeDlgItem | 引擎内建（等价 no-op） | C# 空实现（无效果） | L | L：符号级等价 + 抽样冒烟 |
| 353 | `TAKEON` | 不在 `ExecutionCode` | 223 / 0 | `parser.rs` 解析期丢弃 | 不在枚举（oracle 亦无行为） | 部署意图落空；C#/Rust 均 no-op | R（若支持则涉经济/随机） | 内容侧裁决；实现须新增 oracle 外动作并登记 T-* |
| 354 | `GAMEGIRD` | 不在 `ExecutionCode` | 41 / 3 | `parser.rs` 解析期丢弃 | 不在枚举（oracle 亦无行为） | 部署意图落空；C#/Rust 均 no-op | R（若支持则涉经济/随机） | 内容侧裁决；实现须新增 oracle 外动作并登记 T-* |
| 355 | `GAMEDIAMOND` | 不在 `ExecutionCode` | 37 / 18 | `parser.rs` 解析期丢弃 | 不在枚举（oracle 亦无行为） | 部署意图落空；C#/Rust 均 no-op | R（若支持则涉经济/随机） | 内容侧裁决；实现须新增 oracle 外动作并登记 T-* |
| 356 | `}` | 不在 `ExecutionCode` | 11 / 2 | `parser.rs` 解析期丢弃 | 不在枚举（oracle 亦无行为） | 部署意图落空；C#/Rust 均 no-op | R（若支持则涉经济/随机） | 内容侧裁决；实现须新增 oracle 外动作并登记 T-* |
| 357 | `READRANDOMLINE` | 不在 `ExecutionCode` | 8 / 8 | `parser.rs` 解析期丢弃 | 不在枚举（oracle 亦无行为） | 部署意图落空；C#/Rust 均 no-op | R（若支持则涉经济/随机） | 内容侧裁决；实现须新增 oracle 外动作并登记 T-* |
| 358 | `ABILITYADD` | 不在 `ExecutionCode` | 6 / 0 | `parser.rs` 解析期丢弃 | 不在枚举（oracle 亦无行为） | 部署意图落空；C#/Rust 均 no-op | R（若支持则涉经济/随机） | 内容侧裁决；实现须新增 oracle 外动作并登记 T-* |
| 359 | `CHANGEATTACKMODE` | 不在 `ExecutionCode` | 5 / 0 | `parser.rs` 解析期丢弃 | 不在枚举（oracle 亦无行为） | 部署意图落空；C#/Rust 均 no-op | R（若支持则涉经济/随机） | 内容侧裁决；实现须新增 oracle 外动作并登记 T-* |
| 360 | `SETOFFLINEPLAY` | 不在 `ExecutionCode` | 2 / 2 | `parser.rs` 解析期丢弃 | 不在枚举（oracle 亦无行为） | 部署意图落空；C#/Rust 均 no-op | R（若支持则涉经济/随机） | 内容侧裁决；实现须新增 oracle 外动作并登记 T-* |
| 361 | `WEBBROWSER` | 不在 `ExecutionCode` | 2 / 2 | `parser.rs` 解析期丢弃 | 不在枚举（oracle 亦无行为） | 部署意图落空；C#/Rust 均 no-op | R（若支持则涉经济/随机） | 内容侧裁决；实现须新增 oracle 外动作并登记 T-* |
| 362 | `SETSCTIMER` | 不在 `ExecutionCode` | 2 / 1 | `parser.rs` 解析期丢弃 | 不在枚举（oracle 亦无行为） | 部署意图落空；C#/Rust 均 no-op | R（若支持则涉经济/随机） | 内容侧裁决；实现须新增 oracle 外动作并登记 T-* |
| 363 | `SENDSCROLLMSG` | 不在 `ExecutionCode` | 2 / 0 | `parser.rs` 解析期丢弃 | 不在枚举（oracle 亦无行为） | 部署意图落空；C#/Rust 均 no-op | R（若支持则涉经济/随机） | 内容侧裁决；实现须新增 oracle 外动作并登记 T-* |
| 364 | `RECALLHERO` | 不在 `ExecutionCode` | 2 / 0 | `parser.rs` 解析期丢弃 | 不在枚举（oracle 亦无行为） | 部署意图落空；C#/Rust 均 no-op | R（若支持则涉经济/随机） | 内容侧裁决；实现须新增 oracle 外动作并登记 T-* |
| 365 | `KILLSCTIMER` | 不在 `ExecutionCode` | 1 / 1 | `parser.rs` 解析期丢弃 | 不在枚举（oracle 亦无行为） | 部署意图落空；C#/Rust 均 no-op | R（若支持则涉经济/随机） | 内容侧裁决；实现须新增 oracle 外动作并登记 T-* |
| 366 | `OFFLINEPLAYEX` | 不在 `ExecutionCode` | 1 / 1 | `parser.rs` 解析期丢弃 | 不在枚举（oracle 亦无行为） | 部署意图落空；C#/Rust 均 no-op | R（若支持则涉经济/随机） | 内容侧裁决；实现须新增 oracle 外动作并登记 T-* |
| 367 | `DELLINELIST` | 不在 `ExecutionCode` | 1 / 0 | `parser.rs` 解析期丢弃 | 不在枚举（oracle 亦无行为） | 部署意图落空；C#/Rust 均 no-op | R（若支持则涉经济/随机） | 内容侧裁决；实现须新增 oracle 外动作并登记 T-* |
| 368 | `ADDLINELIST` | 不在 `ExecutionCode` | 1 / 0 | `parser.rs` 解析期丢弃 | 不在枚举（oracle 亦无行为） | 部署意图落空；C#/Rust 均 no-op | R（若支持则涉经济/随机） | 内容侧裁决；实现须新增 oracle 外动作并登记 T-* |

## 6. 缺口清单（按「部署使用次数 × 影响」排序）

### 6.1 排序表

影响权重：A=经济/属性/战斗数值；B=任务/传送/持久化；C=消息/机器人/UI。
排序键 = 可达次数优先，其次原始次数。

| 排名 | 缺口 | 原始/可达 | 影响 | 性质 |
|---:|---|---:|---|---|
| 1 | `GAMEDIAMOND` 非枚举命令（钻石货币） | 37 / 18 | A | 内容/枚举漂移；oracle 与 Rust 均解析期丢弃 |
| 2 | `READRANDOMLINE` 非枚举命令（随机行读取） | 8 / 8 | A | 内容/枚举漂移；同上 |
| 3 | `DELAYCALL` 枚举存在但无任何处理器 | 62 / 33 | B | oracle 自身缺陷（注册错挂到 `DelayGoto`）；Rust 已按其 no-op 复刻 |
| 4 | `GAMEGIRD` 非枚举命令（灵符货币） | 41 / 3 | A | 内容/枚举漂移；同上 |
| 5 | `TAKEON` 非枚举命令（穿装备） | 223 / 0 | B | 内容/枚举漂移；部署 223 次但可达 0（均排在注册动作之后） |
| 6 | `SETOFFLINEPLAY` / `OFFLINEPLAYEX` 非枚举命令 | 3 / 3 | B | 内容/枚举漂移 |
| 7 | `WEBBROWSER` / `SENDSCROLLMSG` / `SETSCTIMER` / `KILLSCTIMER` 非枚举命令 | 7 / 4 | C | 内容/枚举漂移（假人/客户端交互） |
| 8 | `ABILITYADD` / `CHANGEATTACKMODE` / `RECALLHERO` / `DELLINELIST` / `ADDLINELIST` 非枚举命令 | 15 / 0 | C | 内容/枚举漂移；可达 0 |
| 9 | `handlers.rs` 把 `CheckUserDate` 列入动作 switch（实际是条件 switch 的 case，且已注释） | 0 / 0 | C | 生成表归类错误；动作路径两边都 no-op，行为等价。部署仅 1 处且在 `#IF` 条件行（`QuestDiary/月卡/月卡系统.txt:5`），不进动作统计 |
| 10 | Rust 缺失的 68 条 C# 注册动作（`NotImplemented`） | 0 / 0 | A/B/C | 存量缺口；部署可达 = 0，不占窗口 |
| 11 | Rust `run_switch_action("Break")` 提前结束当前列表；C# 只置 `result=false` 并继续 | 9 / 0 | C | 控制流偏差；部署中 BREAK 后仅有 9 条 `}`（无注册动作），当前零影响 |

### 6.2 前三名的最小修复面

> 前提：三条硬约束里的「行为等价优先」意味着**不许**为了「让脚本能用」而单方面给 Rust 加 oracle 没有的语义。
> 下列「最小修复面」分两条路径：**(a) 纯 parity 路径**（只登记/不行为变更，0 风险）；
> **(b) owner 裁决后的能力扩展**（新增 oracle 外动作，必须登记 `T-*` 并占 R 级窗口）。

#### G-1 `GAMEDIAMOND`（37 原始 / 18 可达 / 2 文件）
- 部署位置：`Market_Def/功能NPC/回收-3.txt`（回收装备时 `GAMEDIAMOND + 10`）、`Market_Def/功能NPC/回收-2.txt`。
- oracle：`ExecutionCode` 无此键 ⇒ `nCMDCode=0` ⇒ 该行不进 `ActionList` ⇒ **完全无效**。
- Rust：同样在 `parser.rs` 解析失败丢弃 ⇒ parity 保持。
- **(a) 最小 parity 面**：0 行代码；在本报告/内容清单登记「内容侧死命令」，由内容 owner 决定改用 `GAMEGOLD`/`GAMEPOINT` 或删。
- **(b) 最小扩展面**：`parser.rs` 增加别名表（`GAMEDIAMOND → GamePoint` 或新枚举）＋ `engine/actions.rs`/`player` 增加钻石字段；
  涉及货币落库，tier=R，必须真机对拍 + owner 签字。**不建议在 M3 内做。**

#### G-2 `READRANDOMLINE`（8 原始 / 8 可达 / 3 文件；#CALL 内联修正后可达 5）
- 部署位置：`QuestDiary/LEGM2.CC假人系统/假人登录脚本.txt` 等假人脚本。
- oracle：无枚举 ⇒ 解析期丢弃；Rust 同。
- **最小 parity 面**：0 行代码；登记内容漂移即可（假人脚本的随机登录文案不会生效，但假人主流程仍由其它命令驱动）。
- **最小扩展面（若 owner 要支持）**：`parser.rs` 别名 + `engine/actions.rs` 新 handler（读文件、取随机行 ⇒ RNG + 文件 IO，tier=R），
  需复用 `SystemShare.RandomNumber` 同流并占 R 级窗口。

#### G-3 `DELAYCALL`（62 原始 / 33 可达 / 9 文件；#CALL 内联修正后可达 29）
- 部署位置：假人脚本、QFunction 等；语义是「n 毫秒后跳到标签」。
- oracle 实测：`ExecutionCode.DelayCall=346` **没有注册**；`ActionOfDelayCall` 挂在 `DelayGoto=345` 上
  （`ExecutionProcessingSys.cs:124`）。所以 `DELAYCALL` 是**oracle 自身的接线 bug**，脚本作者以为在延时，实际什么都不发生。
- Rust：`handlers.rs` 只有 345 的条目，`run_action`/`run_switch_action` 都没有 346 ⇒ 同样 no-op，parity 成立。
- **最小 parity 面**：0 行代码（保持现状）；在本报告登记该 oracle 缺陷，避免后续误修。
- **若要修 oracle 缺陷**：把 `ExecutionCode.DelayCall` 也注册到 `ActionOfDelayCall`（C# 侧一行），Rust 同批在 `handlers.rs` 生成源/注册表加 346；
  这会改变部署行为（33 条可达 `DELAYCALL` 开始排队），属 **R 级 + owner 裁决**，不能在 parity 批次里偷做。

### 6.3 结论

- **Rust 是否能执行部署可达动作**：能。68 个规范动作里 67 个已实现/引擎内建，`DELAYCALL` 与 oracle 同为 no-op；
  44 个在部署中实际可达的动作名里，43 个命中已实现处理器或等价 switch，`DELAYCALL` 与 oracle 同为 no-op。
- **语义是否一致**：在「动作级」一致；但两边共同继承了三个会改变内容意图的行为：
  ① 每块只执行第一条注册动作；② `DELAYCALL` 无处理器；③ 非枚举命令解析期丢弃。
- **需要 owner 知道的数字**：6453 条规范动作行中 4007 条（62.1%）因提前返回而不可达；
  部署脚本的「字面意图」与「实际执行」差距远大于 Rust/C# 的实现差距。

## 7. 复现与残留物

- oracle 清册命令：`grep -n "ScriptDefName" src/Modules/ScriptEngine/Consts/ExecutionCode.cs`；
  `grep -n "ProcessExecutionMessage\[(int)ExecutionCode" src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs`。
- 部署统计：对 `E:/MirServer/M2GameSvr/Envir/**/*.txt` 做 GBK 解码，仅统计 `#ACT`/`#ELSEACT` 块行首 token；
  统计脚本与中间 JSON 在 `E:/tmp`（`scriptaudit_usage.json` / `scriptaudit_maps.json`），**未入库**。
- Rust 清册：`handlers.rs` 的 `EXECUTION_HANDLERS` / `ENGINE_SWITCH_EXECUTIONS`；
  `engine/dispatch.rs` 的 `IMPLEMENTED_EXECUTIONS`；`engine/actions.rs` 的 `run_action` / `run_switch_action`。
- 本报告未改任何 `.rs`/`.cs`/脚本文件；worktree 只新增本 md。

---

矩阵行数：**368**（352 个枚举动作 + 16 个非枚举命令）；缺口条目：**11**。
