# 玩法缺口表：物品（item parity gap）

> 卡：`card/parity-gap-survey`（只读分析）。本文件**先出物品一块**，任务与 NPC 脚本按 opc-07 要求后续追加。
> 口径基准时间：2026-11-19 23:16（+0800），仓库 HEAD 见文末"计数时刻"。

## 1. 分母定义（定下后不再改；要改必须写明改前/改后/原因）

| 角色 | 定义 | 可复跑命令 | 本次结果 |
| --- | --- | --- | --- |
| **主分母** | **玩家物品使用入口（`CM_EAT`）分派树上的"行为叶子"数**：一个叶子 = 一种玩家可见效果，逐条记 `文件:行`（见 §7/§8） | `grep -rn 'ClientUseItems(' --include=*.cs src/` + 手工展开（§8） | **19** |
| 旁证① | 代码里按物品类型分叉的 `==` **位点数**（含非使用路径：商店/机器人/属性/显示） | `grep -rn 'StdMode ==\|Shape ==' --include=*.cs src/ \| wc -l` | **111** |
| 旁证② | **数据侧** `StdItems` 行数（内容量，不是行为量） | 见 §9.2 | 待数（不影响本块） |
| 装备轴（不计入主分母） | `CM_TAKEONITEM`/`CM_TAKEOFFITEM` 的穿/卸机制：通用迁移 + 校验 + 属性重算；**不是按物品类型的行为分派**（理由见 §7.3） | `grep -n 'ClientTakeOnItems\|ClientTakeOffItems' src/M2Server/Player/PlayObject.Operate.cs` | 单列，见 §8.2 |

**为什么主分母不用 111**：111 是"`==` 位点数"，同一行为可被拆成多处判断（虚高），一处 `switch` 也能覆盖多物品（漏计）。
**旁证只能证明覆盖广度，不能作为主分母的上界**；两者不在同一集合上（`==` 位点 vs 分派树的 case 叶子），比较无意义。
反例：111 条里落在 `CM_EAT` 分派树上的命中数是 **0**（旁证的 grep 模式抓不到 `switch` 的 case；逐文件归属见 §9）。

## 2. 状态口径（照 opc-07 章程 §5）
- `verified`：必须在 `mir2-rs` 里有**对拍测试名**可指（指向 oracle 生成的夹具）；本表出现的每一条 `verified` 都写在"测试名"列；
- `implemented（未验证）`：Rust 有实现但没有对拍测试；
- `missing`：C# 有、Rust 无（或未接线）。

## 3. 已排除的候选（附证据）
- `src/M2Server/Player/PlayObject.Base.cs:1758` 的 `switch (clientItem.Item.Need)` —— **装备需求**分派（0/1/2/3/4/5/6/10/11/12/13/40/41/42/43/44/60/70/81/82），
  是"能不能穿"的校验，不是物品使用行为。Rust 侧同口径：`bag::check_take_on_items`（已对拍）。
- `src/M2Server/Player/PlayObject.Base.cs:1733` 的 `switch (clientItem.Item.StdMode)`（case 10/11 性别）—— 同上，穿戴校验。
- `src/GameSrv/Npc/Merchant.cs:421` 与 `:1146`、`:979`/`:1184`/`:1249`/`:1374` —— 商店买卖/收购的 `StdMode` 分派。
- `src/BotSrv/BotHelper.cs:1053`、`src/BotSrv/MShare.cs:2374`、`src/BotSrv/Player/RobotPlayer.cs:3563`、`src/Modules/RobotSystem/**` —— 机器人侧，非玩家使用入口。
- `src/M2Server/Player/CharacterObject.cs:161`（`PlaySuperRock`，气血石/魔血石 StdMode 7 Shape 2/3）—— **被动 tick**，不是客户端使用入口（见 §8.3）。
- `src/M2Server/Magic/MagicBase.cs:58/:63/:82/:87`（护身符 StdMode 25 / Shape 5）—— **施法时自动消耗**，不是使用入口（见 §8.3）。
- `src/M2Server/Player/PlayObject.Attack.cs:892`（武器 Shape 19 挖矿）—— 攻击/采集路径（见 §8.3）。
- 骑马：oracle 只走 GM 命令 `@TakeOnHorse`/`@TakeOffHorse`（`src/Modules/GameCommand/Commands/TakeOnHorseCommand.cs`），
  `HorseType` 无赋值点（记忆 `horse-riding-inert-server-side`）；不是物品使用分派。

## 4. 计数时刻
- 仓库 HEAD：`572a0b8a3`（master＝本分支 base）；命令执行时间：2026-11-19 23:16（+0800）；
- `src/` 在 `a5643481a..572a0b8a3` 之间无改动（`git diff --stat a5643481a master -- src/` 输出为空）⇒ 上一版 §4 的旁证计数时刻仍可比；
- 旁证①命令原文与输出：
```
$ grep -rn 'StdMode ==\|Shape ==' --include=*.cs src/ | grep -v worktrees | wc -l
111
```

## 5. 结论（物品块）
- 主分母已钉死：**19** 条行为叶子（§8.1），其中 `verified` 7、`implemented（未验证）` 1、`missing` 11。
- 离"能玩"最近的缺口：**S 级 4 条**（IT-08 卷轴·经验、IT-10 随机传送、IT-14 修复油、IT-15 特修油）+ **S–M 级 2 条**（IT-09 回城卷、IT-17 技能书）。
- **已证行为差异**（不是"一致地不吃"）：StdMode 3/4 在 oracle 有成功吃掉路径，Rust 落 `SM_EAT_FAIL`；
  已登记在 `bins/game-svr/src/useitem.rs` 头部的 T-17 whitelist。§8.1 的 11 条 `missing` 全部落在这个差异面内。

## 6. 方法论（写给接手方：为什么"位点数"不能当分母）
1. **位点计数会双向失真**：一个行为可能被拆成多处判断（重复计数，分母虚高）；一处判断也可能覆盖多种物品（漏计，分母虚低）。
   所以位点数只能当**旁证**，用来给主分母设上界（主分母 ≤ 旁证数），不能直接当分母。
2. **主分母必须是"分派点上的分支集合"**：找到**唯一的玩家物品使用入口**，把它对 `StdMode`/`Shape` 的
   `switch/case`（或等价 if 链）逐条列出 `文件:行`。这份集合才是"原版有多少种物品行为"的可数定义。
3. **旁证不能当上界**：主分母（分派树的 case 叶子）与旁证位点（`==` 出现次数）**不在同一集合上**，
   数值大小关系没有信息量——旁证只能用来证明"覆盖面足够广/还存在别的入口"，不能给主分母设上界，也不能反推主分母漏了。
   反例（本仓实测）：111 条旁证里，落在 `CM_EAT` 主分派树上的命中数 = **0**（grep 的 `==` 模式抓不到 `switch` 分派）。
   因此**作废**"主分母 ≤ 111"这种比较；要核对"是否漏了入口"，用 §7 的调用点唯一性证明（`ClientUseItems` 全仓仅一处调用）。
   旁证的正确用法是归属分类（§9）：把每条位点分到"使用/装备/商店/机器人/其他"，用来发现**别的**入口或已排除项。
4. **数据侧单列**：`StdItems` 行数是**内容量**（有多少件物品），不是**行为量**（有多少种行为），两者不可混。
5. **定位入口的方法（避免重复调查）**：不要从 `grep 'StdMode =='` 逆向猜——先从**协议入口**下手
   （客户端"使用物品"消息号 → 服务端 handler → 调用链），再在调用链内找分派；已排除的候选见 §3。
6. **状态列的证据要求**：`verified` 必须能指到 `mir2-rs` 的对拍测试名（章程 §5）；没有测试名的实现一律
   `implemented（未验证）`——这条对后续覆盖率脚本也是硬约束，脚本应按"测试名是否可解析"来判定。

## 7. 主分母定位（方法 5：协议入口 → handler → 调用链）

### 7.1 消息号与 handler（原文）
```
$ grep -n 'CM_EAT\s*=\|CM_TAKEONITEM\s*=\|CM_TAKEOFFITEM\s*=' --include=*.cs src/OpenMir2/Messages.cs
47:        public const int CM_TAKEONITEM = 1003;
48:        public const int CM_TAKEOFFITEM = 1004;
53:        public const int CM_EAT = 1006;

$ sed -n '820,828p' src/M2Server/Player/PlayObject.Message.cs
                case Messages.CM_TAKEONITEM:
                    ClientTakeOnItems((byte)processMsg.nParam2, processMsg.nParam1, processMsg.Msg);
                    break;
                case Messages.CM_TAKEOFFITEM:
                    ClientTakeOffItems((byte)processMsg.nParam2, processMsg.nParam1, processMsg.Msg);
                    break;
                case Messages.CM_EAT:
                    ClientUseItems(processMsg.nParam1, processMsg.Msg);
                    break;
```
网关层（`src/GameGate/Services/ClientSession.cs:628`、`src/CloudGate/Services/ClientSession.cs:552`）只对 `CM_EAT` 做**限速**（超频回 `SM_EAT_FAIL`），不改变分派。

### 7.2 唯一性证明（使用入口只有一处）
```
$ grep -rn 'ClientUseItems(' --include=*.cs src/
src/M2Server/Player/PlayObject.Message.cs:827:                    ClientUseItems(processMsg.nParam1, processMsg.Msg);
src/M2Server/Player/PlayObject.Operate.cs:601:        private void ClientUseItems(int nItemIdx, string sItemName)
```
=> 全仓只有 `PlayObject.Message.cs:827` 一个调用点。`EatItems`/`ReadBook`/`EatUseItems`/`UseStdModeFunItem` 的调用点也只在
`PlayObject.Operate.cs:625/:633/:668` 与 `PlayObject.Base.cs:2163`（同一条链）。

### 7.3 为什么"穿/卸"不进主分母
`ClientTakeOnItems`（`PlayObject.Operate.cs:377`）与 `ClientTakeOffItems`（`:499`）**不按物品类型产生不同玩法**：
迁移逻辑对所有物品同形；`StdMode` 只用于（a）校验（性别 10/11、`CheckUserItems` 槽位白名单、`Need` 需求——§3 已排除）、
（b）衣服（10/11）成功后的**显示帧选择**（`Shape` 9/龙袍/王袍/plain，`:460-476`）。这是"怎么显示"，不是"物品能干什么"。
据此单列为装备轴（§8.2），不进 §8.1 的 19 条。

## 8. 分派清单（逐条 `文件:行`）

### 8.1 主分母：`CM_EAT -> ClientUseItems` 的行为叶子（19）
```
switch (stdItem.StdMode)                       PlayObject.Operate.cs:619
  |-- case 0  (药)            :621 -> EatItems   PlayObject.Base.cs:2047
  |     `-- switch (stdItem.Shape)               PlayObject.Base.cs:2058
  |           case 1 :2060 / case 2 :2064 / case 3 :2068 / default :2072
  |-- case 1  (食物)          :622 -> EatItems case 1   PlayObject.Base.cs:2085
  |-- case 2                  :623 -> EatItems case 2   PlayObject.Base.cs:2095
  |-- case 3  (卷轴)          :624 -> EatItems case 3   PlayObject.Base.cs:2098
  |     `-- switch (stdItem.Shape)               PlayObject.Base.cs:2099
  |           case 12 :2101 / case 13 :2158 / default :2162 -> EatUseItems(stdItem.Shape) :2163
  |                 `-- switch (nShape)          PlayObject.Base.cs:2231
  |                       case 1 :2233 / 2 :2238 / 3 :2246 / 4 :2258 / 5 :2264 / 9 :2286 / 10 :2292 / 11 :2298
  |-- case 4  (书)            :632 -> ReadBook    PlayObject.Base.cs:2171
  `-- case 31 (解包)          :655
        |-- AniCount == 0 :656 -> ClientUseItemsGetUnBindItems  PlayObject.Operate.cs:583
        `-- AniCount != 0 :668 -> UseStdModeFunItem             PlayObject.Base.cs:1072
```
计数：`StdMode 0` 4 + `1` 1 + `2` 1 + `3`（Shape 12/13 两条 + `EatUseItems` 8 条）10 + `4` 1 + `31` 2 = **19**。

### 8.2 装备轴（单列，不进主分母）
- 入口：`CM_TAKEONITEM(1003)` -> `PlayObject.Operate.cs:377 ClientTakeOnItems`；`CM_TAKEOFFITEM(1004)` -> `:499 ClientTakeOffItems`；
- 校验：`PlayObject.Base.cs:1730 CheckTakeOnItems`（性别 `:1733`、`Need :1758`）、`M2Share.cs:376 CheckUserItems`（槽位白名单 `:382-449`）；
- 显示帧选择：`PlayObject.Operate.cs:460`（`StdMode 10/11`）-> `:462 Shape 9` / `:466 龙袍` / `:470 王袍` / `:474 plain`；
- 属性应用：`PlayObject.cs:4414 ApplyItemParameters`、`:4617 ApplyItemParametersEx`（由 `RecalcAbilitys` 调用，属能力面）。
- Rust：`bins/game-svr/src/bag.rs` 全链 + `tests/parity/m3/fixtures/bag_vectors.json`（21 条，`bag::tests::matches_oracle_bag_fixture`）。

### 8.3 旁路（使用入口之外、但会被误当"物品行为"的位点）
| 位点 | 行为 | 为何不计入 |
| --- | --- | --- |
| `CharacterObject.cs:161` | 气血石/魔血石（StdMode 7 Shape 2/3）自动回血 | 每 tick 被动触发，无 `CM_*` 入口 |
| `MagicBase.cs:58/:82` | 护身符（StdMode 25 Shape 5）施法消耗 | 施法链路内自动扣耐久 |
| `PlayObject.Attack.cs:892` | 武器 Shape 19 挖矿 | 攻击/采集链路 |
| `Merchant.cs:421/:979/:1146/:1184/:1249/:1374` | 商店买卖/修理/寄售 | 交易面 |
| `BotSrv/**`、`Modules/RobotSystem/**` | 机器人侧同类判断 | 非玩家入口 |
| `CM_PICKUP`/`CM_DROPITEM` | 拾取/丢弃 | 物品搬运面（归 m3 bag 卡） |

## 9. 交叉核对

### 9.1 主分母 19 vs 旁证 111
**19 与 111 不在同一集合上，比较无意义**（`==` 位点 vs `switch` case 叶子），不能用它证明任何"上界"或"遗漏"。
这里只做**归属核对**：
- 旁证①的 grep 模式是 `StdMode ==|Shape ==`，而主入口的分派是 **`switch (…StdMode)` / `switch (…Shape)`**——
  **结构性地抓不到 switch 的 case**。111 条里落在 `CM_EAT` 链上的为 **0**；
- 111 逐文件归属（合计 111）：

| 角色 | 条数 | 文件 |
| --- | --- | --- |
| 机器人侧 | 44 | `BotSrv/Player/RobotPlayer.cs` 16、`Modules/RobotSystem/Services/RobotPlayer.cs` 10、`BotSrv/Objects/HeroActor.cs` 8、`BotSrv/MShare.cs` 6、`RobotPlayer.Conf.cs` 2、`RobotPlayer.Base.cs` 1、`BotSrv/DecodeCompatSelfCheck.cs` 1 |
| 装备/属性/显示 | 44 | `M2Server/Player/PlayObject.cs` 21、`M2Server/M2Share.cs` 12（CheckUserItems）、`PlayObject.Operate.cs` 5、`PlayObject.Base.cs` 5（StdMode 50 名字后缀）、`BaseObject.Abil.cs` 1（翅膀） |
| 商店 | 4 | `GameSrv/Npc/Merchant.cs` |
| 魔法/护身符 | 4 | `M2Server/Magic/MagicBase.cs` |
| 脚本引擎 | 4 | `Modules/ScriptEngine/**` |
| 物品系统（Desc 升级） | 3 | `M2Server/Items/GameItemSystem.cs` |
| 物品状态 40/45 | 3 | `M2Server/Actor/BaseObject.cs` |
| GM 命令 | 2 | `Modules/GameCommand/**` |
| 挖矿 | 1 | `PlayObject.Attack.cs` |
| 气血石 | 1 | `CharacterObject.cs` |
| 怪物生成 | 1 | `WorldServer.MonGen.cs` |

=> 结论：**111 既不构成使用行为的上界，也不构成"使用行为都在这里"的覆盖证明**；它能说明的只是"按 `==` 出现的物品判断遍布全仓"。
主分母的独立性来自 §7.2 的唯一入口证明（`ClientUseItems` 全仓仅一处调用），而不是 111。

### 9.2 数据侧（旁证②，单列）
`StdItems` 行数 = 内容量（可掉/可买多少件物品），与行为量（19）不可比；本块不据此算覆盖率。

## 10. 物品全表（19 条行为叶子）

> 证据全名：`verified` 行指向 `mir2-rs/bins/game-svr/src/useitem.rs` 的 `useitem::tests::matches_oracle_eat_fixture`（`#[test]`，第 683 行）；
> 夹具 `mir2-rs/tests/parity/m3/fixtures/eat_vectors.json`（由 `mir2-rs/tests/parity/m3/eat-probe` 反射调用 `src/M2Server` 真身生成）；"用例"= 夹具 `cases[].name`。
> `missing` 行均命中 T-17 已知差异：登记条在 `useitem.rs` 头部「未做清单」第 64–81 行（StdMode 3/4 在第 66–70 行、ReadBook 在第 14 行、T-17 字样在第 78 行）。

| ID | 原版行为 | 原版依据（文件:行） | Rust 状态 | 测试名 / 证据路径 | 成本 | 备注 |
| --- | --- | --- | --- | --- | --- | --- |
| IT-01 | 药·Shape1 固定量补血补魔 `IncHealthSpell(AC,MAC)` | `PlayObject.Base.cs:2060` | **verified** | `useitem::tests::matches_oracle_eat_fixture` @ `bins/game-svr/src/useitem.rs`（第 683 行）；夹具用例 `eat_ok_shape1_fixed`、`eat_ok_shape1_clamp` | — | |
| IT-02 | 药·Shape2 解锁药 `UserUnLockDurg=true` | `PlayObject.Base.cs:2064` | **verified** | 同测试/夹具，用例 `eat_ok_shape2_unlock` | — | 无即时帧 |
| IT-03 | 药·Shape3 百分比补血补魔 | `PlayObject.Base.cs:2068` | **verified** | 同测试/夹具，用例 `eat_ok_shape3_percent`；另单测 `useitem::tests::shape3_percent_rounds_away_from_zero` | — | |
| IT-04 | 药·default 叠加 `IncHealth/IncSpell` 池 | `PlayObject.Base.cs:2072` | **verified** | 同测试/夹具，用例 `eat_ok_shape0_inc_pool`、`eat_ok_shape0_inc_zero` | — | ushort 回绕 |
| IT-05 | 食物 `HungerStatus += DuraMax/10`（上限 5000、跨档刷新） | `PlayObject.Base.cs:2085` | **verified** | 同测试/夹具，用例 `eat_ok_stdmode1_hunger_no_recalc`、`_hunger_recalc`、`_hunger_clamp` | — | `RefMyStatus` 的 `RecalcAbilitys` 尾段登记 T-17（第 75–79 行） |
| IT-06 | StdMode 2 空消耗（只吃掉） | `PlayObject.Base.cs:2095` | **verified** | 同测试/夹具，用例 `eat_ok_stdmode2_noop` | — | |
| IT-07 | 卷轴·Shape12 六属性瞬间 buff（`ExtraAbil` ×6 + 计时） | `PlayObject.Base.cs:2101-2157` | **missing** | — | M | 命中 T-17（`ExtraAbil` 未实现；oracle 成功吃掉 / Rust 落 `SM_EAT_FAIL`；`useitem.rs` 第 66–70 行） |
| IT-08 | 卷轴·Shape13 `GetExp(DuraMax)` | `PlayObject.Base.cs:2158` | **missing** | — | S | 命中 T-17（同上；oracle 此支**无条件**成功、必吃掉） |
| IT-09 | 卷轴·default->Shape1 回城卷（回 `HomeMap` 随机点） | `PlayObject.Base.cs:2162->2163`、`:2233` | **missing** | — | S–M | 命中 T-17（`EatUseItems` 未实现；oracle 条件成功 / Rust 落 `SM_EAT_FAIL`） |
| IT-10 | Shape2 随机传送（本图随机点，受 `boNORANDOMMOVE`） | `PlayObject.Base.cs:2238` | **missing** | — | S | 命中 T-17（`EatUseItems` 未实现）；地图 flag 门 |
| IT-11 | Shape3 地牢逃脱（`PvpLevel()<2` 回 Home，否则回红名村） | `PlayObject.Base.cs:2246` | **missing** | — | M | 命中 T-17（`EatUseItems` 未实现）；`Config.RedHome*` |
| IT-12 | Shape4 祝福油 `WeaptonMakeLuck()` | `PlayObject.Base.cs:2258` | **missing** | — | M | 命中 T-17（`EatUseItems` 未实现）；改武器幸运/诅咒 |
| IT-13 | Shape5 行会回城（城堡成员回城堡） | `PlayObject.Base.cs:2264` | **missing** | — | M–L | 命中 T-17（`EatUseItems` 未实现）；`CastleMgr.IsCastleMember` |
| IT-14 | Shape9 修复油 `RepairWeapon()` | `PlayObject.Base.cs:2286` | **missing** | — | S | 命中 T-17（`EatUseItems` 未实现） |
| IT-15 | Shape10 特修油 `SuperRepairWeapon()` | `PlayObject.Base.cs:2292` | **missing** | — | S | 命中 T-17（`EatUseItems` 未实现） |
| IT-16 | Shape11 彩票 `WinLottery()` | `PlayObject.Base.cs:2298` | **missing** | — | M | 命中 T-17（`EatUseItems` 未实现）；`Config.WinLotteryLevel1..6`（Rust 已有该组配置字段） |
| IT-17 | 技能书 StdMode 4 `ReadBook`（学魔法 + `+LNG/+WID`） | `PlayObject.Operate.cs:632 -> PlayObject.Base.cs:2171` | **missing** | — | S–M | 命中 T-17（`ReadBook` 未实现，`useitem.rs` 第 14 行）；Rust 有 `world-bevy::magic::can_read_book` 谓词但**未接线**到 `use_item` |
| IT-18 | 解包 StdMode 31 / `AniCount==0`（容量门 + 按 Shape 发 6 件） | `PlayObject.Operate.cs:655-666` | **implemented（未验证）** | 单测：`useitem::tests::unbind_stdmode31_creates_six_items_and_reweights`/`_missing_shape_eats_pack_but_creates_nothing`/`_capacity_gate_uses_pre_removal_count`；对拍仅 `eat_fail_stdmode31_unbind_bag_full`（失败路径） | — | 成功路径无 oracle 夹具 => 不给 verified |
| IT-19 | 解包 StdMode 31 / `AniCount!=0` -> 脚本 `@StdModeFunc{AniCount}` | `PlayObject.Operate.cs:668 -> PlayObject.Base.cs:1072` | **verified** | 同测试/夹具，用例 `eat_ok_stdmode31_funitem`、`eat_fail_stdmode31_no_functionnpc` | — | 脚本派发在 `lib.rs` `@StdModeFunc*` 接缝 |

汇总：`verified` 7（IT-01/02/03/04/05/06/19）、`implemented（未验证）` 1（IT-18）、`missing` 11（IT-07…17）。
`missing` 11 条**全部命中 T-17**（oracle 成功/条件成功吃掉 → Rust 落 `SM_EAT_FAIL`），不是静默跳过。
T-17 登记条位置（`mir2-rs/bins/game-svr/src/useitem.rs` 头部「未做清单」）：
第 64 行（标题"登记在卡 `openmir2-m3-bag-equip-alignment` 的 whitelist"）、第 66–70 行（StdMode 3/4 已证差异）、第 14 行（case 4 ReadBook 未实现）、第 78 行（T-17 字样）、第 413 行（"已证差异，登记在 T-17"）。

`missing` 11 条的成本分层（按条点名）：
- **S（4 条）**：IT-08 卷轴·经验、IT-10 随机传送、IT-14 修复油、IT-15 特修油；
- **S–M（2 条）**：IT-09 回城卷、IT-17 技能书；
- **M（4 条）**：IT-07 属性 buff、IT-11 地牢逃脱、IT-12 祝福油、IT-16 彩票；
- **M–L（1 条）**：IT-13 行会回城。

**装备轴（§8.2，不入 19）**：`bag::tests::matches_oracle_bag_fixture`（21 条：穿成功/换装/失败码 0/-1/-4/卸下/满包/Dealing…）
=> 机制层 `verified`；卡 `openmir2-m3-bag-equip-alignment` 的"双栈同序列"判据仍 RED（BotSrv 驱动属登记面）。
