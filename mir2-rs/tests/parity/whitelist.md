# 差异白名单（tests/parity）

规则（§6.0-3）：允许的差异必须登记——「差异点 / 为什么无害 / 谁审的」。没登记 = 不过。
当前对拍差异计数：**0**（四条门禁全绿）。以下登记的是「对拍刻意不覆盖」与「已知限制」，
不是测出的差异。

## D 线登记

| 编号 | 差异/豁免点 | 为什么无害 | 谁审的 |
| --- | --- | --- | --- |
| D-1 | 掉落对拍不含 `UserItem.MakeIndex` | C# `M2Share.GetItemNumber()`（`M2Share.cs:547`）返回 `计数器 + HUtil32.GetTickCount()`，含墙钟，**两次 C# 运行都不一致**，不存在可对拍的真值。字段本身只是唯一 ID，不影响掉落内容（物品/持久/极品属性已对拍，diff=0） | D 线（待 owner 复核） |
| D-2 | `equals_ordinal_ignore_case` 用 Unicode 大写折叠近似 .NET `OrdinalIgnoreCase` | 仅影响物品名匹配；现网物品名为中文 + ASCII，两种规则在 ASCII/中文域内判定相同，实测 1000 物品名 + 全部掉落表对拍 diff=0。若未来引入土耳其语 i 等特殊字符需重审 | D 线（待 owner 复核） |
| D-3 | 固定种子 RNG 复刻的是 legacy 减法算法；C# 生产路径 `new Random()`（无参）在 .NET 6+ 是 xoshiro256** | 无参序列**无法**做固定种子对拍（两次运行不同）。这是"统计手感"层面的已知风险而非可测差异：固定种子下两侧算法与序列已证逐项相同（门禁②）。M3 若要求与现网逐次掉落一致，需另立"同种子注入"方案（C# 侧无法注入种子到无参 Random，届时不等价是 C# 自身的限制） | D 线（待 owner 复核） |
| D-4 | 掉落门禁对"只改概率数值"的篡改存在漏检窗口（N=100 采样踩不中翻转区间，实测 `10/100→10/101` 未变红） | 已如实记录；序列级错误（种子/顺序/次数）与内容级差异（物品、持久、属性）100% 可检出。提高 N 可缩小窗口，成本是运行时间 | D 线（待 owner 复核） |
| D-5 | `goldsales`：C# 读错列名（`DealChrName`，DDL 实为 `DealCharName`）导致有数据时也会全部静默跳过——Rust **复刻同一错列名**而非修正 | 行为等价优先于修 bug（§8.1）。当前表 0 行，两侧均加载 0 行；表有数据时两侧同样都加载 0 行。修复应单开批次，并保留改前/改后行为记录 | D 线（待 owner 复核） |
| D-6 | `GetLevelExp(nLevel>255)` 在 C# 是 `IndexOutOfRangeException`，Rust 复刻为越界 panic | 死代码分支（调用方传 byte 等级 0..=255）；崩溃语义一致。`level_exp_above_max_panics_like_csharp` 测试钉住 | D 线（待 owner 复核） |

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
| B-8 | 脚本命令码位移：解析器存 `GetFields() 字段序号-1`（特判 CHECK/CHECKOPEN/CHECKUNIT/Set/ReSet/SetOpen/SetUnit/ResetUnit 存原值），执行注册表以枚举值为键 ⇒ 脚本命令派发到前一个枚举成员的处理器（如 CHECKLEVEL→ConditionCheckUnit） | `ScriptParsers.cs:329/501` → `crates/script/src/parser.rs`（code-1）；派发表实证 `tests/parity/envir-2026-10-10/handler-maps.txt` |
| B-9 | `IsStringNumber` 恒 true（`||` 缺陷 + 正则匹配空串） | `HUtil32.cs:477` → `crates/script/src/hutil32.rs` |
| B-10 | `CaptureString` 从 c=1 起扫描（跳过 0 号引号）：dest 含开引号不含闭引号；无闭合/无空格抛 IndexOutOfRange | `HUtil32.cs:299-357` → `crates/script/src/hutil32.rs`（Err(CaptureStringPanic)） |
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

协议事实（M0 已裁定并冻结，非差异）：EDCode 循环态 2→4→6→2；客户端帧 `#1...!` /
服务端帧 `#...!`；头 12B 与体分别编码后拼接；C# 网关无显式分帧器（按 TCP 段直读），
Rust 侧 `FrameSplitter` 属传输层健壮性差异。

## B 线登记

| 编号 | 差异/豁免点 | 为什么无害 | 谁审的 |
| --- | --- | --- | --- |
| B-101 | Envir 全量解析的 427 条"脚本错误"（M4 验收①原文写"解析错误 = 0"） | 非两侧差异：C# 参照在同一份语料上产出同样的 427 条（逐行多重集差 0/0）。`TakeOn`/`GAMEGIRD`/`ReadRandomLine` 等命令在 C# 源码中不存在（LEGM2 等变体遗留），内容冻结下不可通过实现它们收敛。建议验收①口径修订为"双侧逐行一致" | B 线（待 owner 复核口径） |
| B-102 | 重名 label 改名后缀：harness 反射播种 C# `Random(42)`，Rust 用复刻的 `System.Random(42)` | 后缀取值不进入任何验收计数；仅数据文件误喂解析器时影响崩溃时点（两侧同样整文件丢弃）。无参 Random 的固有不可复现性见 D-3 | B 线 |

证据：`tests/parity/envir-2026-10-10/`（双侧 parse-stats JSON + diff.txt + handler-maps.txt + SHA256SUMS）。
复现命令见 `crates/script/README.md`。

