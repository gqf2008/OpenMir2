# 差异白名单（tests/parity）

规则（§6.0-3）：允许的差异必须登记——「差异点 / 为什么无害 / 谁审的」。没登记 = 不过。
当前对拍差异计数：**0**（四条门禁全绿）。以下登记的是「对拍刻意不覆盖」与「已知限制」，
不是测出的差异。

## D 线登记

| 编号 | 差异/豁免点 | 为什么无害 | 谁审的 |
| --- | --- | --- | --- |
| D-1 | 掉落对拍不含 `UserItem.MakeIndex` | C# `M2Share.GetItemNumber()`（`M2Share.cs:547`）返回 `计数器 + HUtil32.GetTickCount()`，含墙钟，**两次 C# 运行都不一致**，不存在可对拍的真值。字段本身只是唯一 ID，不影响掉落内容（物品/持久/极品属性已对拍，diff=0） | D 线（待 owner 复核） |
| D-2 | `equals_ordinal_ignore_case` 用 Unicode 大写折叠近似 .NET `OrdinalIgnoreCase` | 仅影响物品名匹配；现网物品名为中文 + ASCII，两种规则在 ASCII/中文域内判定相同，实测 1000 物品名 + 全部掉落表对拍 diff=0。若未来引入土耳其语 i 等特殊字符需重审 | D 线（待 owner 复核） |
| D-3 | 固定种子 RNG 复刻的是 legacy 减法算法；C# 生产路径 `new Random()`（无参）在 .NET 6+ 是 xoshiro256**。可对拍的边界 = **带种子构造**（legacy 减法，固定种子下两侧序列已证 0 差异；B 线脚本改名用的 `RandomNumber.GetRandomNumber` 同属此列） | 无参序列**无法**做固定种子对拍（两次运行不同）。这是"统计手感"层面的已知风险而非可测差异：固定种子下两侧算法与序列已证逐项相同（门禁②）。M3 若要求与现网逐次掉落一致，需另立"同种子注入"方案（C# 侧无法注入种子到无参 Random，届时不等价是 C# 自身的限制） | D 线（待 owner 复核） |
| D-4 | 掉落门禁对"只改概率数值"的篡改存在漏检窗口（N=100 采样踩不中翻转区间，实测 `10/100→10/101` 未变红） | 已如实记录；序列级错误（种子/顺序/次数）与内容级差异（物品、持久、属性）100% 可检出。提高 N 可缩小窗口，成本是运行时间 | D 线（待 owner 复核） |
| D-5 | `goldsales`：C# 读错列名（`DealChrName`，DDL 实为 `DealCharName`）导致有数据时也会全部静默跳过——Rust **复刻同一错列名**而非修正 | 行为等价优先于修 bug（§8.1）。当前表 0 行，两侧均加载 0 行；表有数据时两侧同样都加载 0 行。修复应单开批次，并保留改前/改后行为记录 | D 线（待 owner 复核） |
| D-6 | `GetLevelExp(nLevel>255)` 在 C# 是 `IndexOutOfRangeException`，Rust 复刻为越界 panic | 死代码分支（调用方传 byte 等级 0..=255）；崩溃语义一致。`level_exp_above_max_panics_like_csharp` 测试钉住 | D 线（待 owner 复核） |
| D-7 | **C# 世界 RNG 流只有前 ≈1828 次取数跨运行确定**，之后因非 RNG 熵源（NPC 处理顺序/`GetTickCount` 门控）分岔：实测两次同种子运行的调用点从 `Merchant.Run` 变成 `GuildOfficial.Run`（消耗样本数相同故流位置自愈，缩放上限不同故数值不同） | M3 基线因此**只对前 1500 次取数取 hash**（`prefix_calls`，留余量），整条流 hash 不作判据；L3 端到端要逐次一致需先冻结时钟与遍历顺序（`RNG同种子与记录回放方案.md` 第四节给了三条路径）。该边界是 C# oracle 自身的性质，不是 Rust 侧差异 | D 线（待 owner 复核） |
| D-8 | **C# 的 AOI 不排除观察者自己**：`BaseObject.ViewRange.cs:108-116` 的过滤器没有 `baseObject == this` 判断，自己格里的自己对象会被加进 `VisibleActors`（M2 骨架实测由金标准自证：`ent 1 30 30 vis 1:2 …`） | 复刻不修（行为等价优先于修 bug）；Rust 侧同语义，由 `tests/parity/world/golden/basic.txt` 钉住。若下游渲染/广播要排除自己，属 M2 后续批次的产品决策，需另立条目并保留改前/改后记录 | D 线（待 owner 复核） |

## 复刻的 C# 已知 bug（不是差异，是刻意 1:1，供后续修复批次索引）

| 编号 | bug | 位置 |
| --- | --- | --- |
| B-1 | `MonGetRandomItems` 的 `itemName` 循环外只赋值一次：第一个非金币掉落确定后，后续非金币掉落全部复制第一件物品 | `WorldServer.MonGen.cs:441/453-456` → `crates/formula/src/drop.rs`（BUG-REPLICA） |
| B-2 | 道士 `MaxHandWeight` 判断用 `/13.0`、赋值用 `/42.0`（两处除数不一致） | `PlayObject.cs:5265-5271` → `crates/formula/src/level_ability.rs`（BUG-REPLICA） |
| B-3 | `GetLevelExp` else 分支 `NeedExps[NeedExps.Length]` 恒越界 | `BaseObject.cs:1418` → `crates/formula/src/exp.rs`（BUG-REPLICA） |
| B-4 | `LoadMagicDB`：`TrainLevel[3]` 取 `NeedL3`（无 NeedL4）、`MaxTrain[3]=MaxTrain[2]`、`TrainLv` 恒 3 | `MySqlDB.cs:134-139` → `crates/data/src/loaders.rs` |
| B-5 | `goldsales` 列名 `DealChrName`/`BuyChrName` 与 DDL 不符（见 D-5） | `MySqlDB.cs:264-265` → `crates/data/src/loaders.rs` |
| B-6 | `LoadMonsterDB` 移动/攻击速度下限判断写了两遍（`_MAX(200,..)` 后再 `if <200`） | `MySqlDB.cs:213-224` → `crates/data/src/loaders.rs` |
| B-7 | `RandomSelect` 异常消息写反（"selectCount必需大于sourceList.Count"，实际条件相反） | `RandomNumber.cs:53` → `crates/shared/src/rng.rs` |
| B-8 | 脚本命令码位移：解析器存 `GetFields() 字段序号-1`（特判 CHECK/CHECKOPEN/CHECKUNIT/Set/ReSet/SetOpen/SetUnit/ResetUnit 存原值），执行注册表以枚举值为键 ⇒ 脚本命令派发到前一个枚举成员的处理器（如 CHECKLEVEL→ConditionCheckUnit；端到端实测 `take→ActionOfSet`、`break→ActionOfResetUnit`、`goto→switch 的 EndQuest 分支`） | `ScriptParsers.cs:329/501` → `crates/script/src/parser.rs`（code-1）；实证 `tests/parity/envir-2026-10-10/{handler-maps.txt,dispatch-shift-evidence.md}`。上游 2023-06-15 提交 8a8a02d1 引入；现网 ScriptSystem.dll（2026-10-09 构建）同此行为。**【2026-10-10 S3 已修，见 T-4】**：C# 侧 `ScriptParsers.cs:329/501` 改为存**字段序号**（= 枚举值），此后每条命令派发到**自己**的处理器；上表"复刻"描述只对 **Rust 侧**仍然成立（Rust 默认保持旧位移，T-4）。影响面（实测校正，见 `evidence/S3/rules-diff-sweep.txt`）：两口径 CmdCode 不同的行为**条件 1401/1433 + 动作 6239/6286 = 7640 行**（未变的 79 行用的是特判命令 CHECK/CHECKOPEN/CHECKUNIT + Set/ReSet/SetOpen/SetUnit/ResetUnit）。 |
| B-9 | `IsStringNumber` 恒 true（`||` 缺陷 + 正则匹配空串） | `HUtil32.cs:477` → `crates/shared/src/hutil32.rs` |
| B-10 | `CaptureString` 从 c=1 起扫描（跳过 0 号引号）：dest 含开引号不含闭引号；无闭合/无空格抛 IndexOutOfRange | `HUtil32.cs:299-357` → `crates/shared/src/hutil32.rs`（Err(CaptureStringPanic)） |
| B-11 | `LoadScriptCallScript` 的 label 形同虚设：文件头到首个 `}` 之间的行全部并入 | `ScriptParsers.cs:80-111` → `crates/script/src/parser.rs` |
| B-12 | 重复 `#CALL` 路径 `callList[i]="#ACT"` 越界即抛 ArgumentOutOfRange | `ScriptParsers.cs:163` → `crates/script/src/parser.rs`（LoadPanic::CallListOutOfRange） |
| B-13 | `scriptType == 1` 死分支（从不赋 1），quest flag 头解析不执行 | `ScriptParsers.cs:794` → `crates/script/src/parser.rs`（unreachable 标记） |
| B-14 | 常量替换要求匹配位置 > 0（行首命中不替换），每 define 每行至多 10 次 | `ScriptParsers.cs:634-650` → `crates/script/src/parser.rs` |

## A 线待观察项（C# 侧既有口径差——非对拍差异，M1/M2 移植网关时逐条处置）

| 编号 | 口径差 | 触发条件 | 位置 |
| --- | --- | --- | --- |
| A-1 | `SM_EAT_FAIL` 限频分支按 `CommandFixedLength=16` 编码 12 字节头（C# 侧越界抛异常风险） | `IsEatInterval` 开启且超速吃粮 | `GameGate/Services/ClientSession.cs:634-638` |
| A-2 | 聊天过滤命令分支 `EncryptUtil.Encode(..., dstOffset=0)` 覆盖帧首 `#` | `ChatCommandFilterMap` 命中 | `GameGate/Services/ClientSession.cs:529-535` |
| A-3 | `LoginGate.SendDefMessage` 带 sMsg 时 `Array.Copy(sBuff, 0, tempBuf, 13, ...)` 超出 12+len 缓冲（必抛异常，说明该分支从未被真实触发） | 登录网关下行带文本消息 | `LoginGate/Services/ClientSession.cs:184-190` |
| A-6 | **授权偏离**：C# `LoginSrv/Storage/AccountStorage.Initialization` 连不上 MySQL 时**只打日志继续跑**（空内存表 ⇒ 之后所有登录都报"账号不存在"）；Rust 侧按 M1 验收④「连不上 MySQL 必须明确报错退出」**改为返回错误** | 依据＝设计文档 §6 M1 验收④（该条本身即要求偏离静默空转）；偏离点单一、可测（`AccountStore::connect` 返回 `Err`），且不影响任何正常路径行为 | A 线（M1④ 授权） |
| A-7 | 复刻的 C# 怪行为（**1:1 保留**，供后续修复批次索引）：① `Index()` 返回的是 **Id 而非位置**；② `Add` 的重复判定 `Index(s)>0` ⇒ **Id=0 的账号被当成不存在**；③ `Update(nIndex, …)` 越界判定写成 `_accountMap.Count <= nIndex` ⇒ **Id ≥ 账号数时更新被静默跳过**；④ 删号是软删（`State=1`）、查号是 `INNER JOIN account_protection`（缺行即"资料读取失败"） | 行为等价优先于修 bug（§8.1）：这些分支在现网是既成事实，M1 先 1:1，修复另立批次 | `LoginSrv/Storage/AccountStorage.cs:177-184/530-552/457-473/326-354` → `crates/storage/src/account.rs` |
| A-8 | s2c game 跳**多段体帧**：C# 把若干段**各自独立编码**后拼接进一个 `#…!` 帧（例：`SM_TURN` 家族体 = `EncodePacket(CharDesc 8B) + EncodeString("名字/颜色")`，源 `M2Server/Player/PlayObject.Message.cs:1541-1552`）。单段编码可产长度为 `n+ceil(n/3)`，实测不符帧 payload 长度**恒 ≡1 (mod 4)**（单段永远产不出）⇒ 多段。 | **已按消息号表建模**（`crates/protocol::frame::body_layout` + `encode_body`/`decode_server_payload`，分段编码/解码，字节级由 C# oracle 新增 4 条 SM_TURN 向量钉住）。**未建模的形态**（本批残余）：① 33 条 payload 恒 13 字符、导出器给不出头（`ident=None`，疑为"无头的 `enc(8B)+enc(1B)`"帧）；② `ident=811`(SM_ADJUST_BONUS) 1 条 + 其余若干。二者需按其 C# 发送点逐号补表 —— **归宿：A3 续批**。 | 金标准 C2-20261010（222 帧） | A 线（worker-1） |
| A-9 | C2 导出器的 `body_len`/`body_sha256` 对**多段体帧**是**按整段扁平解码**算的（段边界不在 4 字符周期上 ⇒ 解出错位字节），与修正后的分段解码不一致 ⇒ 本批 ② 出现 21 条"字段不符" | 是我的模型变正确后暴露的**生产端口径差**，不是服务端行为差异：头字段（ident/recog/param/tag/series）全部一致。修法：C2 导出器按同一张布局表分段解码（或由 replay 对多段帧只比头字段 + 标注 body 字段为"生产端口径"） | `tools/capture/GoldenExport` ↔ `crates/protocol::frame::body_layout` | A 线 + C 线（下一批对齐） |
| A-5 | 客户端独有号 `CM_QUERYDYNCODE = 3501`：冻结客户端在 `cnsIntro` 阶段发送（体 = `EncodeString(g_LoginKey)`，`g_LoginKey` 默认字面量 `"password"`），而服务端 `Messages.cs` 无此号 | 服务端行为＝**静默忽略、不回包**（C# `LoginSrv` 的 `ProcessUserMsg` switch 落到 default），Rust 侧同样忽略；号本身登记在 `crates/protocol::client_only`，使 M0③"未实现包号=0"可判。M0 金标准第 1 帧即此号 | 7000 跳 cnsIntro 阶段 | 客户端 `MirClient/Source/MirClient/ClMain.pas:16542`、`MShare.pas:1407` ↔ 服务端 `src/OpenMir2/Messages.cs`（无） |
| A-4 | 同一跳（7000）存在两种 s2c 帧尾：LoginSrv 构造的帧是 `#…!$`（`ClientSession.cs:745`），LoginGate 自身产生的帧是 `#…!` ⇒ 回放/移植时**不能按端口假设帧尾**，须按帧的实际尾字节复现（`crates/protocol::ServerFrameTail`），抓包记录用 `hop`/`tail` 给出独立期望 | 7000 跳的下行帧 | `LoginSrv/Services/ClientSession.cs:745` ↔ `LoginGate/Services/ClientSession.cs:181/197` |

协议事实（M0 已裁定并冻结，非差异）：EDCode 循环态 2→4→6→2；客户端帧 `#1...!` /
服务端帧 `#...!`（**登录跳 s2c 例外的帧尾 `!$`，见 A-4**）；头 12B 与体分别编码后拼接；
C# 网关无显式分帧器（按 TCP 段直读），Rust 侧 `FrameSplitter` 属传输层健壮性差异。

M0 金标准验收状态（2026-10-10）：① 逐字节回放 0 差异、② 字段对拍 0 差异（16 帧真实抓包）；
③ 未实现包号 = 0，**阶段覆盖 5/8**（缺 移动/攻击/小退，属 C 线抓包拓扑缺口，解除判据见
`tests/golden/C线-交付说明.md`）。详见 `evidence/M0/金标准验收报告.md`。

## B 线登记

| B-107 | C# 参照在两处条件处理器上**自身崩溃**：`ConditionOfCheckRangeMonCount`（`String.get_Chars` 越界，`ConditionProcessingSys.cs:1112`）与（已随 mock 补全消除）`ConditionOfCheckSlaveListCount` 的从属列表读取。前者由 S2 接管处置，本线不修。 | 该崩溃由夹具数据触发、与脚本语义无关；本线按 S2 建议登记豁免，并把该用例改为 **B-8 落点回归语料**（`tests/parity/flow-cases.json` 的 `rust_expect`）：Rust 侧必须稳定产出登记的落点序列（`flag`/`NotImplemented` 清单/`[脚本错误]` 原文），落点漂移即 `EXEMPT-DRIFT` 变红 | B 线（S2 结论同步） |

| 编号 | 差异/豁免点 | 为什么无害 | 谁审的 |
| --- | --- | --- | --- |
| B-101 | Envir 全量解析的 427 条"脚本错误"（M4 验收①原文写"解析错误 = 0"） | 非两侧差异：C# 参照在同一份语料上产出同样的 427 条（逐行多重集差 0/0）。`TakeOn`/`GAMEGIRD`/`ReadRandomLine` 等命令在 C# 源码中不存在（LEGM2 等变体遗留），内容冻结下不可通过实现它们收敛。建议验收①口径修订为"双侧逐行一致" | B 线（待 owner 复核口径） |
| B-106 | ~~与 D 线 `crates/shared` 的功能重叠~~ **已收敛（2026-10-10）**：`HUtil32` 原语迁入 `crates/shared/src/hutil32.rs`（全 workspace 唯一实现，`shared::hutil` 的 `str_to_int`/`get_valid_str3` 改为委托），`System.Random` 复刻统一用 `shared::rng`（并修正其大区间分支为 .NET 精确的 `GetSampleForLargeRange`，补探针值测试）；B 线删除 `random.rs`/`hutil32.rs` | 收敛后全量对拍仍 `== 全部一致 ==`（639/639 结构 + 原文指纹），workspace 门禁全绿 | B 线 |
| B-105 | GB2312(cp936) 字符集：.NET `Encoding.GetEncoding("gb2312")` 是 cp936，比 WHATWG GBK 多定义若干字节对（多映射到 PUA），且单字节特例 `0x80`→U+20AC、`0xFF`→U+F8F5、非法字节回退 `?`（encoding_rs 用 U+FFFD）。**已用生成表精确复刻**：`gbk_overrides.rs`（7032 条，由 `script-tool gen-gbk-overrides` 从 .NET 探测表机械生成）+ `textfile.rs` 状态机；穷举门禁 `tests/gbk_decode.rs` 覆盖全部 32256 个双字节对与边界单字节。回退字符按分支区分（.NET 实测：UTF-8/UTF-16/UTF-32 分支保留 U+FFFD，只有 cp936 用 `?`）；BOM 模型按 `StringList`（`StreamReader`）实测重写：`EF BB BF`→UTF-8、`FF FE`→UTF-16LE（`FF FE 00 00` 亦然）、`FE FF`→UTF-16BE、`00 00 FE FF`→UTF-32BE，其余 gb2312。语料实测命中过（`GuildRankNameFilter.txt` 的 `A8BF`），修正后 639/639 原文摘要一致 | 无残余差异（原为差异，已消除）；三字节以上序列由单/双字节组合推得，若今后遇组合型差异需重审 | B 线（独立审查两轮：A 线 F-R1 + code-review 技能） |
| B-103 | `ToUpper`/`OrdinalIgnoreCase` 用 Unicode 简单大写折叠 + `eq_ignore_ascii_case` 近似 .NET 语义（D-2 同族） | C# `string.ToUpper()` 走 CurrentCulture；生产为 zh-CN、语料为 GBK 中文 + ASCII，两种规则在该域内判定相同（639 文件逐文件指纹一致）。若引入 tr-TR 等特殊文化需重审 | B 线 |
| B-104 | 文件集口径：Rust 侧按 Win32 通配语义枚举 `*.txt`（扩展名前 3 字符为 txt，含 `x.txt2`），与 C# `Directory.GetFiles(root,"*.txt",AllDirectories)` 对齐 | 两侧文件集**逐条**比对（结构摘要表以相对路径为键），当前 639 == 639；新增/缺失文件会使门禁变红。注：设计文档 §2/§5.2 写 652、M4 验收①写 634，实测 M2GameSvr\Envir 递归 = 639、Mir200\Envir = 634（见 B-101 口径提请） | B 线 |

证据：`tests/parity/envir-2026-10-10/`（双侧 parse-stats JSON + diff.txt + handler-maps.txt + SHA256SUMS）。
复现命令见 `crates/script/README.md`。

## C 线登记（工具侧例外）

| 编号 | 差异/豁免点 | 为什么无害 | 谁审的 |
| --- | --- | --- | --- |
| T-1 | 改动 `src/BotSrv/**`：`LogService` 引用修正（`BotShare.LogService` → `OpenMir2.LogService`）、空角色列表解码守卫（`SelectChrScene.ClientGetReceiveChrs` 对空 body 不再抛 `ArgumentNullException`）、新增 `SocketShim.cs`（上游 `ScoketClient`/`DSCClient*` 被移除后按原 API 表面用 `System.Net.Sockets` 重写）、`AppServer` 的 Host 与 Serilog 装配（`LogService.Logger` 必须先赋值） | BotSrv **不在 oracle 七进程内**：不监听 oracle 端口、不被其它组件依赖，仅作压测/夹具客户端。改动前它无法编译（依赖的类型已在上游重构中删除）、编出来也起不来（`LogService.Logger` 未初始化）；改动后可编可跑，200 并发登录实测 200/200、0 失败。oracle 二进制与配置未被触碰（§13.1 时间戳复核可证：网关 2026-10-09 01:18、`M2Server.dll` 2026-10-10 00:10） | 协调者（§13.4-1 判为工具侧例外并接受） |
<<<<<<< HEAD

## S1 登记（oracle 缺陷修复；owner 2026-10-10 决策"修"，见设计文档 §15）

> 纪律（卡片 `openmir2-s1-oracle-defects`）：oracle 已修 ⇒ **Rust 侧默认复刻旧行为**做对拍，
> 待 M3 之后再按需切到新行为；改前/改后行为各留记录；修后重跑 M0/M1 门禁。
> 记录与证据：`evidence/S1/S1-报告.md`、`evidence/S1/before/`、`evidence/S1/after/`。

| 编号 | 差异/豁免点 | 为什么无害 | 谁审的 |
| --- | --- | --- | --- |
| T-2 | 改动 `src/M2Server/Net/TCP/TCPNetChannel.cs` 与 `src/M2Server/Net/ChannelMessageHandler.cs`：GameSvr 的网关槽位不再按 TCP accept 序绑定（原 `int.Parse(SocketId) - 1`），改为「连接 SocketId → 槽位」映射；`CloseGate` 释放槽位并摘除映射；`Connecting` 按**最小空闲槽**重新分配；世界侧 `SetGateUserList`/`AddGateBuffer`/`CloseUser` 加越界与空槽守卫，收包线程取到空槽改为丢弃 | 这是**修 oracle 缺陷①**（§13.4-5 登记：旧网关断开把槽位 `UserList` 置 null ⇒ 之后进世界的玩家在 `SetGateUserList` 上 NRE 死循环、客户端黑屏且此后每次登录都失败，直到重启 GameSvr）。**正常路径行为逐项不变**（网关不抖动 / 单网关时槽位分配结果与改前一致），唯一可观测变化是「抖动后还能不能继续用」。残留边界：多网关配置下若各网关**不按声明编号顺序**重连，最小空闲槽仍可能错配（改前是同场景必错并 NRE）——本部署只配了一个 GameGate，故不构成实际差异，已在报告里记为 accepted risk。**Rust 侧默认复刻旧行为**（槽位按 accept 序、断开不释放），M3 之后再按需切换到本修复 | S1 登记（作者 `openmir2-svc-1`）→ 待独立 principal 复核 + coordinator 批准 |
| T-3 | oracle 部署件版本对账：`E:\MirServer\M2GameSvr` 由「两批次混装」整体刷新为**同一份构建**（混装见 `evidence/S1/before/reconcile.md`：`M2Server.dll`/`GameSrv.dll`/`GameSrv.deps.json` 是 2026-10-10 00:10 的构建，而 `OpenMir2.dll`/`SystemModule.dll`/`ScriptSystem.dll`/`CommandSystem.dll`/`PlanesSystem.dll`/`GameSrv.exe` 还是 2026-10-09 01:18~21:53 的）；`E:\MirServer` 下 5 个未声明目录（`Mir200`/`CloudGate`/`DBServer`/`LoginSrv`/`MakePlay`）核定为**旧血统内容副本**（macOS 发布件、`.NETCoreApp,Version=v6.0`、程序集名 `GameSvr.dll`），不是 oracle，本机不可运行 | 刷新前的混装本身就是 §13.4-5 登记的缺陷②（"部署件与源码版本漂移"）；刷新后部署件 = 当前 master 的一次 Release 构建（含 T-2 修复），与 `crates/protocol` 消息号表所对的**同一份源码**同源，反而消除了"Rust 对着 A 版源码、oracle 跑 B 版二进制"的错位。日志、`Envir`/`Map`/`Castle` 内容目录、`*.conf`、三方件与 `IPLocal.dll` 插件均未触碰；`Mir200` 等旧目录仅登记不删（其 `Envir` 是设计文档 §2 引用的内容副本）。已录制的金标准是历史数据，不受影响；**此后任何新抓包/对拍都以刷新后的部署为准** | S1 登记（作者 `openmir2-svc-1`）→ 待独立 principal 复核 + coordinator 批准 |


## S3 登记（B-8 命令码位移修复 = oracle 行为变更；卡片 `openmir2-s3-b8-shift-fix`）

> 纪律同 S1：改前/改后行为各留记录 + `T-*` 白名单（Rust 侧默认复刻旧行为）+ 修后重跑 B 线 flow-diff 与 M0/M1 门禁。
> 全文与证据：`mir2-rs/tests/parity/evidence/S3/S3-报告.md`（含 `axis-audit.txt` / `rules-diff-sweep.txt` /
> `regression-120.txt` / `probe-line-itemaddvalue.txt` / `m0m1-and-flowdiff.txt`（门禁 + flow-diff 改前改后）/ `deploy.txt`）。

| 编号 | 差异/豁免点 | 为什么无害 | 谁审的 |
| --- | --- | --- | --- |
| T-4 | 改动 `src/Modules/ScriptEngine/ScriptParsers.cs`（**两处**：条件解析 `LoadScriptFileQuestCondition` 与动作解析的同构分支）：命令码由 `nCMDCode = code - 1` 改为 `nCMDCode = code`。`code` 是 `ConditionCodeDefMap` / `ExecutionCodeDefMap` 里存的 **`GetFields()` 字段序号**，而两张执行注册表（`_conditionMap` / `ProcessExecutionMessage`）是按**枚举值**建键的；`--axis-audit` 实测两个枚举**字段序号 == 枚举值，0 处不等**（条件 207 成员 / 动作 352 成员）⇒ 存 `code` 才命中「这条命令自己的」处理器，原 `code - 1` 恒派发到**前一个枚举成员**的处理器 | 这是**修 oracle 缺陷 B-8**（whitelist B-8 登记的上游 bug）。判决依据：该位移让脚本命令系统性执行错处理器，实机可复现（S2：`CHECKITEMADDVALUE` 4 参数行落到 `ConditionOfCheckRangeMonCount` 的 `sParam5[1]` ⇒ `IndexOutOfRangeException`，被 NPC 层 catch 吞掉、玩家侧「点了没反应」；真实语料同形状 **120 行**，已作 S3 回归集）。**影响面 7640 行**（条件 1401 + 动作 6239，占全部命令行的 99%；未变的 79 行是特判命令）——这不是"窄改动"，是整条脚本派的派发面被纠正。**Rust 侧默认复刻旧位移**（`crates/script/src/parser.rs` 的 `code-1` 保持不动），M3 之后再按需切新行为；因此**修后 B 线 flow-diff 会因两侧映射不同而全 DIFF（实测 0/3，改前 1/3）**，这是本条目**预期并登记**的差异，不是新缺陷。**若要让 flow-diff 恢复可比，需 B 线在同一批把 Rust 那一行一起翻转**（`code-1` → `code`），届时本条目收敛 | S3 登记（作者 `openmir2-svc-1`）→ 待独立 principal 复核 + coordinator 批准；**Rust 是否同批翻转需 coordinator 拍板**（涉及 B 线在飞的 93 个派发目标语义工作） |
=======
| T-2 | 改动 `src/BotSrv/**`（C6 压测批次）：① 新增 `LoadMetrics.cs` —— 压测采集（tick 服务时延直方图 1ms 桶 / 登录成功 / 连不上 / 掉线 / 内部异常计数），每 5s 落 `load_stats.ndjson`；② `RobotOptions.ConnectStaggerMs`（默认 3000 = 原行为）；③ `RobotPlayer.ProcessActMsg` 采样一行（放在 `g_rtime` 全局去重**之前**）；④ `RobotPlayer.SocketError` 按 kind 计数（拒绝链接 vs 掉线）；⑤ `LoginScene` 登录成功计数（放在 if/else 之外，避免漏计）；⑥ 崩溃守卫：`TMap.CanMove` 补数组上界、`RobotPlayer.AttackTarget` 补 `MShare.MySelf`/`target` 空守卫（跨假人共享态的竞态）、`AppService.Run` 每轮 try/catch 兜异常并计数；⑦ `RobotPlayer.EnsureAutoPlay()` + `ClientManager.RunAutoPlay` 重臂（进图后挂机定时器会被停掉）；⑧ `RobotPlayer.ProbeActionTick()` 动作探针（`MIR2_BOT_ACTION_PROBE=1`，进世界后每 N ms 发一个 `CM_TURN`，**只依赖本假人自己的 socket 与场景状态**，不碰 `MShare` 世界态） | BotSrv **不在 oracle 七进程内**（T-1 已登记：不监听 oracle 端口、不被其它组件依赖，仅作压测/夹具客户端）。本批改动全部是**采集与健壮性**：不改变上线字节语义（`+GD`、动作消息形状照 C3 金标准实测），不改 oracle、不改协议层；唯一的默认行为变化是崩溃不再打死整个压测进程（改为计数，`load_stats.ndjson.internal_errors` 可见） | 待协调者复核（C6 批次） |
>>>>>>> worktree-c-tools
