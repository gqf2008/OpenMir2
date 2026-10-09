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
