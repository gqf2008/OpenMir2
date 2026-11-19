# 玩法缺口表：物品（item parity gap）

> 卡：`card/parity-gap-survey`（只读分析）。本文件已出**物品**、**任务**、**NPC** 三块。
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
- 仓库 HEAD：`3679403bd`（master；补齐 T-17 头部登记，注释-only）；命令执行时间：2026-11-19 23:16（+0800），T-17 登记复核 23:25（+0800）；
- `src/` 在 `a5643481a..3679403bd` 之间无改动（`git diff --stat a5643481a 3679403bd -- src/` 输出为空）⇒ 上一版 §4 的旁证计数时刻仍可比；
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
| IT-10 | Shape2 随机传送（本图随机点，受 `boNORANDOMMOVE`） | `PlayObject.Base.cs:2238` | **missing** | — | S | 命中 T-17（`EatUseItems` 未实现）；登记缺口已在 `useitem.rs` 头部补齐（`3679403bd`）；登记缺口已在 `useitem.rs` 头部补齐（`3679403bd`）；地图 flag 门 |
| IT-11 | Shape3 地牢逃脱（`PvpLevel()<2` 回 Home，否则回红名村） | `PlayObject.Base.cs:2246` | **missing** | — | M | 命中 T-17（`EatUseItems` 未实现）；`Config.RedHome*` |
| IT-12 | Shape4 祝福油 `WeaptonMakeLuck()` | `PlayObject.Base.cs:2258` | **missing** | — | M | 命中 T-17（`EatUseItems` 未实现）；改武器幸运/诅咒 |
| IT-13 | Shape5 行会回城（城堡成员回城堡） | `PlayObject.Base.cs:2264` | **missing** | — | M–L | 命中 T-17（`EatUseItems` 未实现）；`CastleMgr.IsCastleMember`；登记缺口已在 `useitem.rs` 头部补齐（`3679403bd`） |
| IT-14 | Shape9 修复油 `RepairWeapon()` | `PlayObject.Base.cs:2286` | **missing** | — | S | 命中 T-17（`EatUseItems` 未实现） |
| IT-15 | Shape10 特修油 `SuperRepairWeapon()` | `PlayObject.Base.cs:2292` | **missing** | — | S | 命中 T-17（`EatUseItems` 未实现） |
| IT-16 | Shape11 彩票 `WinLottery()` | `PlayObject.Base.cs:2298` | **missing** | — | M | 命中 T-17（`EatUseItems` 未实现）；`Config.WinLotteryLevel1..6`（Rust 已有该组配置字段）；登记缺口已在 `useitem.rs` 头部补齐（`3679403bd`） |
| IT-17 | 技能书 StdMode 4 `ReadBook`（学魔法 + `+LNG/+WID`） | `PlayObject.Operate.cs:632 -> PlayObject.Base.cs:2171` | **missing** | — | S–M | 命中 T-17（`ReadBook` 未实现，`useitem.rs` 第 14 行）；Rust 有 `world-bevy::magic::can_read_book` 谓词但**未接线**到 `use_item` |
| IT-18 | 解包 StdMode 31 / `AniCount==0`（容量门 + 按 Shape 发 6 件） | `PlayObject.Operate.cs:655-666` | **implemented（未验证）** | 单测：`useitem::tests::unbind_stdmode31_creates_six_items_and_reweights`/`_missing_shape_eats_pack_but_creates_nothing`/`_capacity_gate_uses_pre_removal_count`；对拍仅 `eat_fail_stdmode31_unbind_bag_full`（失败路径） | — | 成功路径无 oracle 夹具 => 不给 verified |
| IT-19 | 解包 StdMode 31 / `AniCount!=0` -> 脚本 `@StdModeFunc{AniCount}` | `PlayObject.Operate.cs:668 -> PlayObject.Base.cs:1072` | **verified** | 同测试/夹具，用例 `eat_ok_stdmode31_funitem`、`eat_fail_stdmode31_no_functionnpc` | — | 脚本派发在 `lib.rs` `@StdModeFunc*` 接缝 |

汇总：`verified` 7（IT-01/02/03/04/05/06/19）、`implemented（未验证）` 1（IT-18）、`missing` 11（IT-07…17）。
`missing` 11 条**全部命中 T-17**（oracle 成功/条件成功吃掉 → Rust 落 `SM_EAT_FAIL`），不是静默跳过。
T-17 登记条位置（`mir2-rs/bins/game-svr/src/useitem.rs` 头部「未做清单」，按 master `3679403bd` 复核）：
第 64 行（标题"登记在卡 `openmir2-m3-bag-equip-alignment` 的 whitelist"）、第 66–70 行（StdMode 3/4 已证差异）、第 71–80 行（`EatUseItems` 的 8 个 Shape 逐条：IT-09…IT-16；第 76–79 行是 2026-11-19 补齐的 IT-13/14/15/16）、第 14 行（case 4 ReadBook 未实现）、第 88 行（T-17 字样）、第 426 行（"已证差异，登记在 T-17"）。
2026-11-19 复核更正：头部补齐了 IT-13/14/15/16 四条（此前只被"等大地图副作用"概括），并更正第 16 行"解包未做（T-17）"与第 81 行"解包：已实现"的自相矛盾；合并点 `3679403bd`（注释-only，`git diff -U0` 非注释行为空）。

`missing` 11 条的成本分层（按条点名）：
- **S（4 条）**：IT-08 卷轴·经验、IT-10 随机传送、IT-14 修复油、IT-15 特修油；
- **S–M（2 条）**：IT-09 回城卷、IT-17 技能书；
- **M（4 条）**：IT-07 属性 buff、IT-11 地牢逃脱、IT-12 祝福油、IT-16 彩票；
- **M–L（1 条）**：IT-13 行会回城。

**装备轴（§8.2，不入 19）**：`bag::tests::matches_oracle_bag_fixture`（21 条：穿成功/换装/失败码 0/-1/-4/卸下/满包/Dealing…）
=> 机制层 `verified`；卡 `openmir2-m3-bag-equip-alignment` 的"双栈同序列"判据仍 RED（BotSrv 驱动属登记面）。

---

## 11. 任务块（quest）

### 11.1 分母定义

| 角色 | 定义 | 可复跑命令 | 本次结果 |
| --- | --- | --- | --- |
| **主分母** | **任务专属脚本命令的行为分支**：条件 3（CHECK / CHECKOPEN / CHECKUNIT）+ 动作 5（SET / RESET / SETOPEN / SETUNIT / RESETUNIT）+ 脚本选择门 1（`{Quest N}`） | §11.3 | **9** |
| 旁证③ | 任务态 API 调用位点（6 个 get/set 入口，含 GM/脚本/管理器） | `grep -rn 'GetQuestFalgStatus\|SetQuestFlagStatus\|GetQuestUnitStatus\|SetQuestUnitStatus\|GetQuestUnitOpenStatus\|SetQuestUnitOpenStatus' --include=*.cs src/ \| wc -l` | **30** |
| 数据旁证 | 真实语料中各任务命令的使用文件数（`E:/MirServer/M2GameSvr/Envir`，冻结） | 见 §11.5 | CHECK **12** / SET **18** / CHECKUNIT·CHECKOPEN·SETUNIT·SETOPEN·RESETUNIT **0** / `{Quest` **0** |

与物品块同一口径：**旁证（API 位点 30）与主分母（命令分支 9）不在同一集合上，比较无意义**；旁证只用来发现"还有别的入口/用法"。

### 11.2 方法 5：任务入口（先证"没有任务专用消息号"）

```
$ grep -nE '\b(CM|SM|RM)_QUEST[A-Z_]*' src/OpenMir2/Messages.cs
（零命中：无任务专用消息号）
```
任务行为全部走 **NPC 脚本**，客户端入口只有"点 NPC"与"进世界"两条：
```
CM_CLICKNPC = 1010                      src/OpenMir2/Messages.cs:62
  PlayObject.Message.cs:885  case CM_CLICKNPC: ClientClickNpc(nParam1)
  PlayObject.Base.cs:674     ClientClickNpc -> 校验 -> NPC.Click
  src/M2Server/Npc/NormNpc.cs:68   Click(): GotoLable(PlayObject, "@main", false)   (:73)
  src/Modules/ScriptEngine/ScriptEngine.cs:141  GotoLable(npc, player, label)
  src/Modules/ScriptEngine/ScriptEngine.cs:146  CheckGotoLableQuestStatus（{Quest N} 选择门，调用点 :76）
进世界（无客户端消息）：
  src/M2Server/Player/PlayObject.cs:1440  ManageNPC.GotoLable(this, "@Login", false)
帧：RM_MERCHANTSAY(11009) src/OpenMir2/Messages.cs:651 -> SM_MERCHANTSAY(643) :304
```

### 11.3 分派清单（9 条）

| 类别 | 命令 | oracle 注册 | oracle 实现 |
| --- | --- | --- | --- |
| 选择门 | `{Quest N}` + `#IF [flag] value` / `#RAND` | — | `src/Modules/ScriptEngine/ScriptEngine.cs:146` |
| 条件 | `CHECK [flag] value` | `src/Modules/ScriptEngine/Processings/ConditionProcessingSys.cs:48` | `:4174` |
| 条件 | `CHECKOPEN [flag] value` | `:52` | `:199` |
| 条件 | `CHECKUNIT [flag] value` | `:53` | `:4104` |
| 动作 | `SET flag value` | `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:34` | `:2036` |
| 动作 | `RESET flag count` | `:35` | `:2043` |
| 动作 | `SETOPEN flag value` | `:36` | `:2051` |
| 动作 | `SETUNIT flag value` | `:37` | `:2058` |
| 动作 | `RESETUNIT flag count` | `:38` | `:2065` |

### 11.4 任务全表（9 条 + 轴 + 死路径）

| ID | 原版行为 | 原版依据 | Rust 状态 | 测试名 / 证据路径 | 备注 |
| --- | --- | --- | --- | --- | --- |
| QT-01 | `{Quest N}` 脚本选择门（至多 10 组 `#IF`） | `src/Modules/ScriptEngine/ScriptEngine.cs:146-168` | **implemented（未验证）** | 解析：`crates/script/tests/parser_tests.rs::quest_creates_new_script_info`、`::quest_block_keeps_writing_into_previous_record`；运行时 `engine::mod.rs` 的 `check_quest_status`（第 1074 行）无 oracle 夹具 | `#RAND` 支按 0 保守处理，登记 **B-13**（`mir2-rs/tests/parity/whitelist.md`）；语料 `{Quest` 0 个文件 |
| QT-02 | `CHECK [flag] value` | `src/Modules/ScriptEngine/Processings/ConditionProcessingSys.cs:4174` | **implemented（未验证）** | `crates/script/src/engine/conditions.rs` 的 `cond_check`（第 456 行）；底层位读有 oracle 向量（见 QX-01） | 判定本身无 oracle 夹具；语料 **12** 个文件使用 |
| QT-03 | `CHECKOPEN`（含 oracle 缺陷：恒假） | `src/Modules/ScriptEngine/Processings/ConditionProcessingSys.cs:199` | **verified** | `crates/script/tests/engine_tests.rs::condition_truth_table_matches_oracle`（读 oracle 探针 `tests/parity/m3/quest-state-probe/vectors/quest_condition_vectors.tsv`）、`::checkopen_is_always_false_like_oracle` | 语料 0；**oracle 缺陷照搬，登记 T-37**（不得当 bug 修掉，改它会与 oracle 不一致；断言原文见 §11.4.1） |
| QT-04 | `CHECKUNIT` | `src/Modules/ScriptEngine/Processings/ConditionProcessingSys.cs:4104` | **verified** | 同上的真值表 + `::checkunit_matches_quest_unit_status`、`::checkunit_checkopen_parse_anchors` | 语料 0 |
| QT-05 | `SET flag value` | `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:2036` | **implemented（未验证）** | `crates/script/src/engine/actions.rs` 的 `ActionOfSet`（第 192 行）；登记即实现守卫 `engine_tests.rs::implemented_registry_entries_are_actually_implemented` | 无动作级 oracle 夹具；语料 **18** 个文件使用 |
| QT-06 | `RESET flag count` | `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:2043` | **implemented（未验证）** | `actions.rs` 的 `ActionOfReSet`（第 199 行）+ 同一守卫 | 语料 0 |
| QT-07 | `SETOPEN flag value` | `:2051` | **implemented（未验证）** | `actions.rs` 的 `ActionOfSetOpen`（第 205 行）+ 同一守卫 | 语料 0 |
| QT-08 | `SETUNIT flag value` | `:2058` | **implemented（未验证）** | `actions.rs` 的 `ActionOfSetUnit`（第 210 行）+ 同一守卫 | 语料 0 |
| QT-09 | `RESETUNIT flag count` | `:2065` | **implemented（未验证）** | `actions.rs` 的 `ActionOfResetUnit`（第 155 行）+ 同一守卫 | 语料 0 |

#### 11.4.1 CHECKOPEN 恒假：T-37 登记与断言原文

`CHECKOPEN` 无论 `QuestUnitOpen` / `value` 如何都恒假——这是 oracle 的**怪行为**，不是 Rust 的 bug。
登记编号 **T-37**（`mir2-rs/tests/parity/whitelist.md` 的 T-37 行；批准链 approve `e55c3d76` / merge `a443296e`）。
**不得修掉**：改它会与 oracle 不一致（AGENTS.md 硬约束③「行为等价优先于修 bug」）；将来要"修好"必须单开 `S-*` 批次并保留改前/改后读数。

oracle 探针向量（`tests/parity/m3/quest-state-probe/vectors/quest_condition_vectors.tsv`，真 `ConditionProcessingSys` 产出）4 行全 `success=0`：
```
ConditionOfCheckOpen	5	0	0	0
ConditionOfCheckOpen	5	1	0	0
ConditionOfCheckOpen	5	0	1	0
ConditionOfCheckOpen	5	1	1	0
```
Rust 实现 `crates/script/src/engine/conditions.rs::cond_check_open` 起手 `*result = false;`，三个分支都不改回 `true`（与同族的 `cond_check` / `cond_check_unit` 起手 `true` 不对称）。
Rust 断言原文（`crates/script/tests/engine_tests.rs::checkopen_is_always_false_like_oracle`）：
```rust
assert!(
    npc.says.iter().any(|(s, _)| s.contains("关")),
    "CHECKOPEN 应恒假（oracle 缺陷照搬）：open={open} value={value} says={:?}",
    npc.says
);
```
真值表测试 `condition_truth_table_matches_oracle` 逐行读上述 TSV 并断言同一结果（8 行：CHECKUNIT 4 + CHECKOPEN 4）。

轴（不进主分母）：

| ID | 事项 | 原版依据 | Rust 状态 | 证据 |
| --- | --- | --- | --- | --- |
| QX-01 | 任务态载体 `QuestFlag`/`QuestUnit`/`QuestUnitOpen`（各 128B，MSB-first 位语义） | `PlayObject.cs:3966-4110` | **verified** | `crates/script/tests/quest_state_vectors.rs::quest_bitmap_matches_oracle_byte_for_byte`（oracle 探针 `quest_state_vectors.tsv`；红检 `redcheck_bit_direction`/`redcheck_comparator_detects_mutated_vector`） |
| QX-02 | `MapQuest.txt` + 隐藏 NPC（QManage/QFunction/RobotManage）加载 | `src/GameSrv/DB/LocalDB.cs:341-405`、`src/GameSrv/Maps/MapQuestManager.cs:10` | **implemented（未验证）** | `crates/envir/tests/mapquest.rs::synthetic_vectors`、`::load_orders_hidden_npcs_and_reports_result`（合成向量）；`::deployment_mapquest` 为 `#[ignore]` 真数据门，无 oracle 探针 |
| QX-03 | 会话载入/存档（`CharacterData.Quest*` ↔ `PlayObject.Quest*`） | `WorldServer.cs:1160-1162` / `:1291-1293` | **implemented（未验证）** | 接线在 `bins/game-svr/src/lib.rs`（载入第 3295/3429 行、写回第 21250 行）；结构往返 `crates/storage/tests/playerdatainfo_vectors.rs` 覆盖；无端到端 oracle 夹具 |
| QX-04 | 落库（`characters_quest` / `characters` 任务列） | oracle 只 `DELETE` 不 `INSERT`，无调用点（`src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Save.cs:493-495`） | **dead_in_oracle（两侧一致：都不写）** | `bins/db-srv/tests/quest_no_persist.rs::db_srv_save_and_load_sql_have_no_quest_columns`、`::workspace_rust_sources_never_reference_characters_quest`、`::captured_cross_side_baselines_show_zero_characters_quest_rows` |

死路径（**登记为不实现**，照搬 oracle 的"死"）：

| ID | 死路径 | 证据 |
| --- | --- | --- |
| QT-D1 | `MapQuest` 怪/物品触发（`GetQuestNpc`） | 调用点被注释：`PlayObject.cs:1670` / `:1677`；定义 `src/GameSrv/Maps/MapQuestManager.cs:58` |
| QT-D2 | `SendQuestMsg`（全服任务广播） | 唯一调用点在注释里：`src/Modules/GameCommand/Commands/StartQuestCommand.cs:22`；定义 `WorldServer.cs:1830` |
| QT-D3 | `QuestDiary`（解析结果丢弃） | `src/GameSrv/DB/LocalDB.cs:804`/`:808` 的 `Add` 被注释 |
| QT-D4 | `characters_quest` 写库 | 同 QX-04：`SaveQuest` 只 DELETE、从不 INSERT、全仓无调用点 |

### 11.5 交叉核对（任务块）

- 主分母 9 与旁证 30 **不在同一集合上**（命令分支 vs API 位点），比较无意义；这里只做归属：
  30 个位点分布 = `PlayObject.cs` 7（位读写定义）、`IPlayerActor.cs` 6（接口）、`SetPassWordCommand.cs` 6（GM 密码门）、`ExecutionProcessingSys.cs` 5（动作）、`ConditionProcessingSys.cs` 3（条件）、`ScriptEngine.cs` 1（选择门）、`ShowHumanFlagCommand.cs` 1、`MapQuestManager.cs` 1。
- 数据旁证（计数时刻同 §4；数据根 `E:/MirServer/M2GameSvr/Envir`）：
```
$ grep -rli "check \[" Market_Def MapQuest_def | wc -l      -> 12
$ grep -rli "set \["   Market_Def MapQuest_def | wc -l      -> 18
$ grep -rli "checkunit|checkopen|setunit|setopen|resetunit" Market_Def MapQuest_def | wc -l -> 0
$ grep -rli "{Quest"   Market_Def MapQuest_def | wc -l      -> 0
```
⇒ 真实语料只用到 2/9（CHECK + SET）；其余 7 条是"代码可达、语料未用"。这正是缺口优先级低的来源，但不等于不存在。

### 11.6 结论（任务块）
- 主分母 9：`verified` 2（CHECKOPEN / CHECKUNIT）、`implemented（未验证）` 7（选择门 + CHECK + 5 个动作）、`missing` **0**。
- 真正的缺口不在命令实现，而在**用法**：真实语料只跑 CHECK/SET；`{Quest` / CHECKUNIT / CHECKOPEN / SETUNIT / SETOPEN / RESETUNIT 在冻结语料里 **0 用量**。
- 4 条死路径（QT-D1…D4）必须保持"死"，不得顺手实现；QX-04 的"两侧都不落库"已由 `quest_no_persist` 门禁钉死。

---

## 12. 机器可读清单（schema v1）

机器清单（**覆盖率的唯一机器事实源**）：`docs/issues/parity-gap-items.json`、`parity-gap-quests.json`、`parity-gap-npc.json`、`parity-gap-magic.json`。
**改状态必须同时改 `notes` 并跑 `coverage-report.ps1` 自检**；覆盖率脚本切到 master 之前，不要移动这些文件的位置（`coverage-report.ps1` 读的是 worktree 路径）。

### 12.1 顶层字段

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `schema_version` | int | 固定 `1`；不兼容变更才升 |
| `domain` | string | `items` / `quests` / `npc` |
| `source_doc` | string | 人类可读表所在文档（相对仓库根） |
| `counting_moment` | object | `{head: <40hex>, time: <ISO8601+08:00>}` |
| `status_enum` | string[] | 见 §12.3 |
| `status_legend` | object | 每个枚举值的判定口径（脚本只读 `status_enum` 做校验） |
| `cost_enum` | string[] | `S` / `S-M` / `M` / `M-L` / `L` |
| `denominator` | object | `{kind, definition, entry, count, commands[]}` |
| `axes` | array | 不进主分母但必须可见的轴（物品：装备轴；任务：QX-01…04） |
| `dead_paths` | array | **可选**；死路径登记（`status` 固定 `dead_in_oracle`），不参与 `denominator.count`；与 `entries` 同形 |
| `entries` | array | 主分母逐条，数量必须 == `denominator.count` |
| `legacy_source` | string | 可选；旧清单绝对路径（技能块指向 `E:/UserFleet/magic-parity.json`） |
| `migration_note` | string | 可选；旧 schema 转写规则（状态只降不升、cost 映射） |
| `todos` | array | 可选；未分派到条目的遗留事项 `{id, desc, ref}` |

### 12.2 条目字段（`entries[]` / `axes[]` 同形）

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `id` | string | `IT-01` / `EQ-01` / `QT-01` / `QX-01` |
| `legacy_id` | string | 可选；旧清单条目 id（技能块用，保留同名技能的多个分派位点） |
| `behavior` / `name` | string | 行为一句话 |
| `oracle_anchor` | object | `{file, line, line_end?}`；`file` 相对仓库根 |
| `status` | string | ∈ `status_enum` |
| `evidence` | object | `{test, test_file, fixture, cases[], unit_tests[]}`：`test` = **入口测试**（可跑的那个）；`cases` = **夹具里的 case 名**或空；多个测试共同证明时放 `unit_tests`，**不塞 `cases`**；任一项可 `null`/空 |
| `cost` | string \| null | ∈ `cost_enum`；已完成项为 `null` |
| `deviation` | object \| null | `{id, file, section, lines?}`；`missing` **必填**，`id` **必须是 T-*/B-*/D-* 编号**（技能块两条已转正为 **T-285/T-286**）；旧清单只作 `legacy_source` 溯源，不得当 `deviation.id`；照搬 oracle 缺陷（如 CHECKOPEN 的 **T-37**）时也可填 |
| `notes` | string | 备注（可空串） |

### 12.3 状态枚举（脚本按此判定）

| JSON 值 | 文档写法 | 判定规则 |
| --- | --- | --- |
| `verified` | verified | 有对拍测试名，且夹具由 oracle 生成（章程 §5） |
| `implemented_unverified` | implemented（未验证） | Rust 有实现/单测，但无 oracle 对拍夹具 |
| `missing` | missing | C# 有、Rust 无；**必须**带 `deviation.id`（T-* / D-*） |
| `dead_in_oracle` | dead_in_oracle | oracle 自身死路径（调用点注释/结果丢弃），Rust 不应实现 |
| `out_of_scope` | out_of_scope | 不属本分母（环境/工具/数据侧） |

### 12.4 不变量（脚本可直接断言）

1. `len(entries) == denominator.count`；
2. `all(e.status in status_enum)`；
3. `e.status == "missing" ⇒ e.deviation.id` 非空且能在 whitelist/差异登记中查到（`id` 必须是 T-*/B-*/D-*；技能块两条为 T-285/T-286，`legacy_source` 只作溯源）；
4. `e.status == "verified" ⇒ e.evidence.test` 非空且 `test_file` 存在；
5. `e.cost ∈ cost_enum ∪ {null}`；
6. `sum(1 for e in entries if e.status=="verified")` 与文档汇总行一致（物品：7；任务：8；NPC：1）。
7. `dead_paths`（可选）不计入 `denominator.count`；其中每条 `status == "dead_in_oracle"`。

---

## 13. NPC 块（NPC 交互链）

### 13.1 分母定义

| 角色 | 定义 | 可复跑命令 | 本次结果 |
| --- | --- | --- | --- |
| **主分母** | **NPC 交互链上的行为叶子**：点 NPC、对话选择、特殊 NPC 覆写、@main 脚本选择、标签+extjmp 门、procedure 执行、断链、说词帧、变量替换、隐藏 NPC、NPC 显示帧 | §13.3 | **11** |
| 旁证④ | `UserSelect` 实现/调用位点 | `grep -rn 'UserSelect(' --include=*.cs src/ \| grep -v worktrees \| wc -l` | **9** |
| 旁证⑤ | `GotoLable(` 调用位点 | `grep -rn 'GotoLable(' --include=*.cs src/ \| grep -v worktrees \| wc -l` | **133** |
| 数据侧 | 内容量（不是行为量） | `wc -l Npcs.txt MerChant.txt`；`find Market_Def -name '*.txt' \| wc -l` | Npcs **6** 行 / MerChant **162** 行 / Market_Def **141** 个脚本 |

旁证 9 / 133 与分母 11 **不在同一集合上**（位点 vs 行为叶子），比较无意义；它们只用来确认"入口不止一处、脚本调用遍布全仓"。

### 13.2 方法 5：NPC 入口（与任务不同，**有**专用消息号）

```
CM_CLICKNPC = 1010               src/OpenMir2/Messages.cs:62
CM_MERCHANTDLGSELECT = 1011      src/OpenMir2/Messages.cs:63
src/M2Server/Player/PlayObject.Message.cs:885  case CM_CLICKNPC: ClientClickNpc(nParam1)
src/M2Server/Player/PlayObject.Message.cs:888  case CM_MERCHANTDLGSELECT: ClientMerchantDlgSelect(...)

点击链：ClientClickNpc（src/M2Server/Player/PlayObject.Base.cs:674）
  -> NormNpc.Click（src/M2Server/Npc/NormNpc.cs:68）-> GotoLable("@main")（:73）
  -> NormNpc.GotoLable（:1889）-> ScriptEngine.GotoLable（src/Modules/ScriptEngine/ScriptEngine.cs:141）
选择链：ClientMerchantDlgSelect（src/M2Server/Player/PlayObject.Operate.cs:114）
  -> npc.UserSelect（NormNpc 虚方法 :1633；Merchant :789；CastleOfficial :93；GuildOfficial :55）
说词帧：RM_MERCHANTSAY(11009)（Messages.cs:651）-> SM_MERCHANTSAY(643)（:304）；
        映射在 src/M2Server/Player/PlayObject.Message.cs:1702
```
客户端**没有**"请求说词"的消息——说词只由服务端推送。这是与"物品有 CM_EAT"、任务"无专用消息"并列的第三种入口形态。

### 13.3 分派清单（11 条）
```
点 NPC（CM_CLICKNPC 1010）
  |-- NP-01 Click() -> GotoLable("@main")                    NormNpc.cs:68/:73
  `-- NP-02 对话选择 -> UserSelect 标签跳转（@back/同标签/新标签） Operate.cs:114 -> NormNpc.cs:1633
特殊 NPC
  `-- NP-03 UserSelect 覆写（Merchant/CastleOfficial/GuildOfficial） Merchant.cs:789 / CastleOfficial.cs:93 / GuildOfficial.cs:55
脚本引擎（ScriptEngine.GotoLable :141）
  |-- NP-04 @main 脚本选择（LastNpc 保持 -> 逆序 Quest 门）      :46-75
  |-- NP-05 标签查找 + extjmp 门                               :89
  |-- NP-06 逐 procedure 条件真/假分支 + 动作执行 + say 累积      :95-125
  |-- NP-07 动作返回 false 断链                                :101 / :113
  `-- NP-08 说词帧（ChrName 前缀、高/普通优先级）                 :987-999 -> Message.cs:1702
说词内容
  `-- NP-09 变量替换（$USERNAME/$PRICERATE/$USERWEAPON…）        NormNpc.cs:275 / Merchant.cs:1109
实体
  |-- NP-10 隐藏脚本 NPC 可点/可选（QManage/QFunction/RobotManage） LocalDB.cs:236/:192/:293
  `-- NP-11 NPC 显示/转向/受击/空闲帧（ProcessNpcs 逐 tick）        WorldServer.Merchant.cs:120
```

### 13.4 NPC 全表（11 条）

| ID | 原版行为 | 原版依据 | Rust 状态 | 测试名 / 证据路径 | 备注 |
| --- | --- | --- | --- | --- | --- |
| NP-01 | 点 NPC 打开对话（`Click` → `@main`） | `src/M2Server/Player/PlayObject.Base.cs:674`、`src/M2Server/Npc/NormNpc.cs:68` | **verified** | `mir2-rs/bins/game-svr/src/lib.rs::np01_click_main_matches_oracle_golden`（oracle golden `mir2-rs/tests/parity/golden/npc/np01_click_main.json` 5 例，basis=semantic；行为面 + 状态面 `LastNpc`/`Script`/`ScriptCurrLable` 均断言） | 牙 A（距离门）/B（@main 派发）/C（set_last_npc）原文在 `E:/UserFleet/qa/npc-parity/`；无 `@main` 的 fallback 用例已覆盖 |
| NP-02 | 对话标签选择跳转（@back/同标签/新标签） | `src/M2Server/Player/PlayObject.Operate.cs:114`、`src/M2Server/Npc/NormNpc.cs:1633` | **verified** | `mir2-rs/bins/game-svr/src/lib.rs::np01_click_main_matches_oracle_golden`（golden `mir2-rs/tests/parity/golden/npc/np02_dialog_select.json` 4 例，basis=semantic；说词序列 + ScriptCurrLable/GoBackLable/ScriptLable） | 对拍钉出并修复 @back 空栈未写回 `ScriptGoBackLable=@main`；牙 D 原文在 `E:/UserFleet/qa/npc-parity/` |
| NP-03 | 特殊 NPC 的 `UserSelect` 覆写（商店/城堡/行会） | `src/GameSrv/Npc/Merchant.cs:789`、`src/GameSrv/Npc/CastleOfficial.cs:93`、`src/GameSrv/Npc/GuildOfficial.cs:55` | **verified** | `mir2-rs/bins/game-svr/src/lib.rs::np01_click_main_matches_oracle_golden`（golden `np03_merchant_menu.json` 4 例：`@buy→645`、`@sell→646`、`@cangku→@storage→700`，真实装载链） | 真实脚本 sha256 `14a3fd4d…7210`；flag-关语料 0 条（单测覆盖）；牙 J 原文 `E:/UserFleet/qa/npc-parity/` |
| NP-04 | `@main` 脚本选择规则（LastNpc → 逆序 → Quest 门） | `src/Modules/ScriptEngine/ScriptEngine.cs:46-75` | **verified** | `mir2-rs/bins/game-svr/src/lib.rs::np01_click_main_matches_oracle_golden`（golden `np04_main_selection.json` 2 例：两 NPC 切换 + `QFunction-0 [@UserCmd6]` “#ACT 只跑首条”；状态断言 `LastNpc`/`Script`/`game_gold`） | 真实脚本 sha256 `9c8187bf…09df`、标签第 382 行；牙 E 原文 `E:/UserFleet/qa/npc-parity/` |
| NP-05 | 标签记录查找 + `boExtJmp` 门 | `src/Modules/ScriptEngine/ScriptEngine.cs:89` | **verified** | `mir2-rs/bins/game-svr/src/lib.rs::np01_click_main_matches_oracle_golden`（golden `np05_extjmp.json` 2 例：`@sub TRUE` 放行 / `@plain` 拦截） | **synthetic 夹具**（语料 0 使用，sha256 `d7cd88bd…d028`）；引擎门等价证据，非线上用例；牙 F 原文同目录 |
| NP-06 | 逐 procedure 条件真/假分支 + 动作执行 + say 累积 | `src/Modules/ScriptEngine/ScriptEngine.cs:95-125` | **verified** | `tests/parity/M4/run_npc_cases.py`（7 条 port-free 用例，C# 参照 vs Rust 逐行 diff；证据 `tests/parity/evidence/M4/npc-cases.txt`） | 用例：传送×2、行会(`@requestcastlewar`)、任务(`Q001@main`)、商店(`@changeGold`)、修理(`@StdModeFunc26`)、仓库(`@zum_bind1`) |
| NP-07 | 动作返回 false 断链 | `src/Modules/ScriptEngine/ScriptEngine.cs:101`/`:113` | **verified** | `mir2-rs/bins/game-svr/src/lib.rs::np01_click_main_matches_oracle_golden`（golden `np07_chain_break.json` 2 例：`BREAK` 断同 record 后续 procedure、其它 record 不受影响） | synthetic 夹具（语料 0 条符合该构造，sha256 `4904046f…c8e`）；引擎门等价证据；牙 H 原文 `E:/UserFleet/qa/npc-parity/` |
| NP-08 | 说词帧（ChrName 前缀、High/普通优先级） | `src/Modules/ScriptEngine/ScriptEngine.cs:987-999` → `src/M2Server/Player/PlayObject.Message.cs:1702` | **verified** | `mir2-rs/bins/game-svr/src/lib.rs::np01_click_main_matches_oracle_golden`（帧面：643 的 ident/param/tag/series/tail + 前缀逐字；用例 `click_main_ok`/`dlg_ask_ok`/`chain_break`/`var_username`） | **优先级子面未断言**（Rust 无 wire 级 priority；oracle 本批普通队列=1），见卡面未建模清单 |
| NP-09 | 说词变量替换（玩家/NPC/价格等） | `src/M2Server/Npc/NormNpc.cs:275`、`src/GameSrv/Npc/Merchant.cs:1109` | **verified** | `mir2-rs/bins/game-svr/src/lib.rs::np01_click_main_matches_oracle_golden`（golden `np09_var_text.json`：`<$USERNAME>`→hero） | 真实脚本 武士-0114.txt（sha256 `ead2df9f…bae7`）；牙 I 原文 `E:/UserFleet/qa/npc-parity/` |
| NP-10 | 隐藏脚本 NPC 可点/可选（QManage/QFunction/RobotManage） | `src/GameSrv/DB/LocalDB.cs:236`/`:192`/`:293` | **implemented（未验证）** | `npc.rs::manage_and_function_npc_gates_require_loaded`、`::loads_from_disk_and_runs_login`；加载见 QX-02 | |
| NP-11 | NPC 显示/转向/受击/空闲帧 | `src/GameSrv/Word/WorldServer.Merchant.cs:120` | **implemented（未验证）** | — | 装置未建（多观察者 AOI 金标准），需独立卡 `openmir2-npc-multiobserve-golden` |

### 13.5 点/选的校验门（不计入行为分母）

| 门 | oracle 依据 |
| --- | --- |
| `IsCanDeal == false` → `RM_MENU_OK(CanotTryDealMsg)` | `PlayObject.Base.cs:676` |
| `Death || Ghost` → 零帧 | `PlayObject.Base.cs:681`、`src/M2Server/Player/PlayObject.Operate.cs:116` |
| `ClickNpcTime` 限频 | `PlayObject.Base.cs:685` |
| 同图 + 15 格距 | `PlayObject.Base.cs:689` |
| 对话选择：异图/超视距且非 `IsHide` | `PlayObject.Operate.cs:123` |

### 13.6 交叉核对（NPC 块）

- 旁证④ 9 个 `UserSelect(` 位点分布：接口 1（`INormNpc.cs`）、虚方法 1（`src/M2Server/Npc/NormNpc.cs:1633`）、覆写 3（`src/GameSrv/Npc/Merchant.cs:789`/`src/GameSrv/Npc/CastleOfficial.cs:93`/`src/GameSrv/Npc/GuildOfficial.cs:55`）、`Merchant` 内部子选择 4（`UserSelectBuyItem` 等）；旁证⑤ 133 个 `GotoLable(` 位点遍布脚本/命令/任务/行会。
- 数据侧：`Npcs.txt` 6 行、`MerChant.txt` 162 行、`Market_Def` 141 个脚本——内容量，不是行为量。

### 13.7 死路径（NPC 块）

| ID | 死路径 | 证据 |
| --- | --- | --- |
| NP-D1 | MapQuest 自动对话触发（怪/物品事件点 NPC） | 同 QT-D1：`src/M2Server/Player/PlayObject.cs:1670`/`:1677` 被注释；全仓无活 `GetQuestNpc` 调用 |

### 13.8 结论（NPC 块）

- 主分母 11：`verified` **9**（NP-06 经 M4 双跑；其余 8 条经 `np01_click_main_matches_oracle_golden`）、`implemented（未验证）` 2（NP-10 等装载语义补测；NP-11 装置未建）、`missing` **0**。
- 机制层没有 known-missing；缺口在**证据强度**：10 条只有 Rust 侧集成测试，没有 oracle 夹具。
- **脚本命令语言全集**（条件/动作注册表）不在本块重复计数：M4 已有双跑对拍（7 条）与注册表门（`IMPLEMENTED_CONDITIONS`/`IMPLEMENTED_EXECUTIONS` + `implemented_registry_entries_are_actually_implemented`）；任务块 §11 的 9 条是其中的任务专属子集。

---

## 14. 技能块（magic，迁移清单）

- 机器清单：**`docs/issues/parity-gap-magic.json`**（与本节同 schema，由 `E:/UserFleet/magic-parity.json` **机械转写**）。
- 分母：`src/M2Server/Magic/MagicManager.cs` 的 case 分支 **65**（含同名技能在多个分派位点，如 `SKILL_FIRECHARM` 在 `:315` 与 `:328`）。
- 状态：**verified 0**、implemented（未验证）**63**、missing **2**（`SKILL_WINDTEBO(id=35)` = **T-285**、`SKILL_GROUPDEDING(id=39)` = **T-286**；均在 `whitelist.md` 有登记行）。
- **未升格**：旧清单 `note` 明写"帧级对拍 **0/65**、`basis` 全空"，7 条带 `test` 的只是 Rust 侧用例名；按 opc-07 要求一律不升 `verified`。
- 旧清单 3 条 `todos` 原样保留在 JSON（buff tick 门 / anti-magic recalc / `DoSpell :167` 版本门）。
- 与前三块的差异：技能块未做口径重审（按"以旧清单为准"），分母沿用旧定义；将来重审另开卡。
