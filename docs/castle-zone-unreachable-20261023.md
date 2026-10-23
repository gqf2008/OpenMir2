# R6 `castle_zone`：可达性登记（**单会话探针不可达 / 需开战钟点 + 多会话夹具**）

- 卡：`openmir2-scenario-castle-zone`（tier=L）；基线 `walgit/master` = `8dac04eb0`；worktree `E:/tmp/wt-worker-69`，分支 `card/scenario-castle-zone`。
- 结论：**不造场景**。本卡交付「不可达登记 + 缺什么夹具」，并按卡面要求写清 4 张 R 卡（`castle-guard-target`、
  `castle-war-area-impl`、`castle-merchant-fixedhide-hit`、`castle-merchant-disappear-downlink`）的窗口**当前不可跑**。
- 依据全部是**部署件 + oracle 源码行号**（只读核对），零起栈、零连库、零端口。

---

## 0. 结论先行

R6 场景要产生「开战前后」两侧的帧（`766 SM_AREASTATE` 的 bit4 + 城堡 NPC 可见性），前提是
`Castle.UnderWar == true`。部署里这条链有 **4 条相互独立的阻断**，任意一条都足以让单会话探针跑不出来：

| # | 阻断 | 证据 |
|---|---|---|
| B1 | **玩家申请攻城的链在 oracle 里直接抛异常**：`@requestcastlewarnow` → `ReQuestCastleWar` → `Castle.AddAttackerInfo(MyGuild: IGuild)` 命中 `UserCastle.cs:670-673` 的 `throw new NotImplementedException()`（真正的实现在 `:998-1016`，形参是 `GuildInfo`，**类型不同、绑不上**），异常被 `GuildOfficial.UserSelect` 的 `catch` 吞 ⇒ 静默无效果 | 白名单 **T-38**（`mir2-rs/tests/parity/whitelist.md:326`，点名 `UserCastle.cs:601/646/672/677/709/913/1042/1047/1052/1057` 十个 stub） |
| B2 | **唯一的按需开战入口是 GM 命令** `@ForcedWallconquestWar <城堡名>`（`ForcedWallconquestWarCommand.cs:8-53`，`PermissionMin=10`），而 `GameCommand.Handle`（`GameCommand.cs:33-40`）在 Release 下按 `Permission` 拦截；部署是 **Release**（`mir2-rs/tests/parity/m3/design-gm-commands-20261023.md:14/185`：`CommandSystem.dll` 不含 `#if DEBUG` 的「当前运行调试模式」串），`StartPermission=0`、`AdminList.txt` 唯一行缺 IP 被丢弃 ⇒ Permission 恒 0 ⇒ `权限不够!!!` | 见左 |
| B3 | **开战钟点是墙钟小时**：`UserCastle.Run` 只有 `DateTime.Now.Hour == Config.StartCastlewarTime` 才开战（`UserCastle.cs:340-362`），部署 `StartCastlewarTime=20`（`Server.conf:380`）⇒ 只在 **20:00–20:59** 那一小时内发生，探针无法预约 | 见左 |
| B4 | **攻击方列表是空的**：`Castle/0/AttackSabukWall.txt` **0 字节**、`SabukW.txt` 的 `OwnGuild=` 为空 ⇒ 即使钟点到了也没有 `AttackWarList` 条目 ⇒ `UnderWar` 永不置位 | 部署件与两侧窗口根均如此（`cat` 核对） |

另加 B5（非阻断但决定观测面）：**这 4 张卡要的是多角色/多行会语义**——守卫三态（同城/盟友/UnderWar 全打）、
四臂落点（非成员进皇宫/成员/死亡/红名）、商人进入/退出战中的帧集合——单会话探针（`mir2_probe.py`
一次只驱动一个客户端）表达不了。

---

## 1. 第 1 步：可达性统计表

### 1.1 脚本入口 `@requestcastlewar*`

```sh
find . -type f -print0 | xargs -0 grep -anoiE '@requestcastlewar[A-Za-z0-9_]*'
```

| 结果 | 值 |
|---|---|
| 总命中行数 | **7** |
| 文件 | 只有 `Npc_Def/比奇国王-0122.txt`（唯一一个 `Npcs.txt` type=1 `GuildOfficial`） |
| 行号 | `:10`（@main 的 `<申请攻城战争/@requestcastlewarA>`）、`:19`（#elsesay 同一链接）、`:27`（`[@requestcastlewarAA]`，说词「尚未开放…」）、`:174`（`[@requestcastlewarA]`：要金条）、`:177`（`<缴纳金条/@requestcastlewar>`）、`:180`（`[@requestcastlewar]`）、`:189`（`<给祖玛头像/@requestcastlewarnow>`） |

> 派发侧：`GuildOfficial.UserSelect` 按 `HUtil32.CompareLStr(sLabel, "@requestcastlewarnow")` 前缀进
> `ReQuestCastleWar`（`src/GameSrv/Npc/GuildOfficial.cs:84`，实现 `:206-245`）。

### 1.2 Castle 配置

| 项 | 值 | 依据 |
|---|---|---|
| 城堡目录/列表文件 | `Castle` / `Castle/List.txt` | `Server.conf:65/71`；缺省 `GameSvrConf.cs:1293-1294` |
| 列表内容 | **2 座**（`0`、`1`） | `E:/MirServer/M2GameSvr/Castle/List.txt`（GBK，两行） |
| 城堡 0 配置 | `[Setup] CastleName=沙巴克 / OwnGuild=(空) / WarDate=1998-01-01`；`[Defense] CastleMapList=0151,0152,0153,0154,0155,0156, / CastleMap=3 / CastlePlaceMap=0150 / CastleSecretMap=D701` + **16 个 Guard/Archer 条目** | `Castle/0/SabukW.txt` |
| 攻击方列表 | `Castle/0/AttackSabukWall.txt` **0 字节** | 部署件与 `E:/tmp/conformance/{csharp,rust}-root/M2GameSvr/Castle/0/` 均空 |
| 读入点 | `LoadCastleList`（`CastleManager.cs:154-177`）、`LoadAttackSabukWall`（`UserCastle.cs:242-284`，要求 `GuildMgr.FindGuild(行会名)` 命中且日期可解析） | 见左 |

### 1.3 NPC / 守卫 / 城堡商人

| 类别 | 结果 | 依据 |
|---|---|---|
| `Npcs.txt` type=2（`CastleOfficial`） | **1 条**：`沙巴克城堡管理 0150 (7,16)` | `Npcs.txt` |
| `Npcs.txt` type=1（`GuildOfficial`） | 1 条：`比奇国王 0122 (29,32)`（`@requestcastlewar*` 宿主） | `Npcs.txt` |
| 城堡守卫 | **16 个**（弓箭手 ×8 + 城墙/城门等），由 `UserCastle` 从 `SabukW.txt` 逐条 `RegenMonsterByName` 并 `BaseObject.Castle = this`（`UserCastle.cs:176-193`）⇒ `Castle != null` 分支真实存在 | 见左；`GuardUnit.IsProperTarget` 的 Castle 分支 = 卡 `openmir2-castle-guard-target` |
| 城堡地图上的商人 | **7 条**，全部 `CastleMerchant=1`（`MerChant.txt` 第 8 列非 0，`LocalDB.cs:462-464`）：武器铺 0151、仓库 0152、药店 0153、手镯/项链/戒指 0154、布衣 0155 | `MerChant.txt:83-90` |
| 城堡地图上的 `MonGen` 行 | **0**（0150–0156） | `MonGen.txt` 扫描 |
| `CastleOfficial.Click` 门 | `Castle.IsMasterGuild(MyGuild) || Permission >= 3`（`CastleOfficial.cs:19-28`）⇒ 无行会、Permission=0 的测试角色**点不开**城堡管理 NPC | 见左 |

### 1.4 `766 = SM_AREASTATE` 的 bit4 由哪段代码置位

```text
PlayObject.RefUserState()                        src/M2Server/Player/PlayObject.Base.cs:270-286
    n8 = 0
    if (Envir.Flag.FightZone)  n8 |= 1           :273-276
    if (Envir.Flag.SafeArea)   n8 |= 2           :277-280
    if (InGuildWarArea)        n8 |= 4           :281-284      <-- 这里
    SendDefMessage(SM_AREASTATE(766), n8,0,0,0)  :285           (Messages.cs:355)

InGuildWarArea 的写点只有 ChangePkStatus(bool)   src/M2Server/Player/PlayObject.cs:4343-4348
    if (InGuildWarArea != boWarFlag) { InGuildWarArea = boWarFlag; NameColorChanged = true; }

ChangePkStatus 的调用点                          src/M2Server/Player/PlayObject.Message.cs
    IUserCastle castle = SystemShare.CastleMgr.InCastleWarArea(this);     :286
    if (castle != null && castle.UnderWar) ChangePkStatus(true);          :287-290
    else                                   ChangePkStatus(false);         :340-343
```

⇒ **bit4（值 4）只由 `Castle.UnderWar == true` 驱动**；`InCastleWarArea` 本身只是地理谓词
（`UserCastle.cs:454-476`：城堡图 `CastleMap=3` 的 `±WarRangeX/Y`、皇宫 `0150`、密道 `D701`、`EnvirList=0151..0156`），
**不检查 UnderWar**——所以「站在城堡区」本身不会点亮 bit4。

### 1.5 `UnderWar` 的赋值点（全量）

| 位置 | 语义 |
|---|---|
| `UserCastle.cs:40` | 构造 `false` |
| `UserCastle.cs:355` | **开战唯一正常入口**：`Run()` 里墙钟小时 == `StartCastlewarTime` 且 `AttackWarList` 有**今天**的申请 |
| `UserCastle.cs:580` | `StopWallconquestWar()` → `false` |
| `ForcedWallconquestWarCommand.cs:30` | GM 命令 `@ForcedWallconquestWar` **直接翻转**（PermissionMin=10，见 B2） |

### 1.6 守卫/商人的战中行为（4 张卡的观测面）

| 卡 | 观测面（oracle 出处） | 前置 |
|---|---|---|
| `castle-guard-target` | `GuardUnit.IsProperTarget` 的 `Castle != null` 分支（`GuardUnit.cs:35-121`；同城/盟友豁免、`Castle.UnderWar` 全打） | `Castle != null` 守卫已存在（16 个）；**三态需要至少 2–3 个不同行会/角色的会话**，UnderWar 臂需要开战 |
| `castle-war-area-impl` | `InCastleWarArea` / 进图落点改写 / `766` bit4 / 名字色（`UserCastle.cs:454-476`、`PlayObject.Message.cs:286-345`、`UserCastle.cs:564-577`） | **UnderWar + 多角色**（四臂落点：非成员进皇宫/成员/死亡/红名） |
| `castle-merchant-fixedhide-hit` | `Merchant.Run` 的 FixedHideMode 解除支：`Merchant.cs:150-157`（`!UnderWar` 且 `FixedHideMode` ⇒ 置 false 并 `SendRefMsg(RM_HIT)`） | 必须先进入过 `UnderWar`（否则 `FixedHideMode` 从未置位） |
| `castle-merchant-disappear-downlink` | 同处的置位支：`Merchant.cs:142-149`（`CastleMerchant && Castle != null && UnderWar` ⇒ `SendRefMsg(RM_DISAPPEAR)`，`FixedHideMode=true`） | **UnderWar 翻转**（进入/退出战中） |

---

## 2. 这 4 张卡的窗口因此**当前不可跑**

| 卡 | 缺什么 | 现在能不能跑 |
|---|---|---|
| `openmir2-castle-guard-target` | 行会三态夹具（城主行会成员 / 盟友 / 攻击方）+ UnderWar + 多会话（至少两个客户端互相站进守卫视野） | **不能** |
| `openmir2-castle-war-area-impl` | UnderWar + 四臂落点所需的四种角色状态（非成员/成员/死亡/红名）+ 766 bit4 的翻转前后 | **不能** |
| `openmir2-castle-merchant-fixedhide-hit` | 一次真实的 `UnderWar true → false` 切换（且商人要在观察者视野内） | **不能** |
| `openmir2-castle-merchant-disappear-downlink` | 一次真实的 `UnderWar false → true` 切换 | **不能** |

> 四张卡的构造级判据不受影响（`cargo test` 向量照跑）；**只有「真机窗口」这一层**被本登记卡住。

---

## 3. 缺什么夹具（可执行规格，需 owner 裁决）

要真正跑通 R6，需要**同时**满足下列三项（缺一不可，且都越过本仓库的内容/构建冻结面）：

1. **开战时刻**：把窗口安排在 **20:00–20:59**（`StartCastlewarTime=20`，`Server.conf:380`），
   *或* owner 授权在**窗口私有根**把 `StartCastlewarTime` 改成运行窗口的小时（部署件 `E:/MirServer` **禁改**；
   窗口根 `E:/tmp/conformance/{csharp,rust}-root` 是私有副本，但 `Server.conf` 属内容面，仍要 owner 裁决）。
   > 用 **Debug 构建**绕过权限是**不可取**的：`GameCommand.cs:33-35` 的 `#if DEBUG` 会把每个玩家的
   > `Permission` 强设成 10 并多发一条红字提示 ⇒ 行为与 Release 部署差一整类，窗口不可比
   > （见 `design-gm-commands-20261023.md:107`）。
2. **攻击方申请**：DB 预置**两个行会**（城主 + 攻击方），并在窗口私有根写
   `Castle/0/AttackSabukWall.txt`：`<攻击方行会名> "<今天日期>"`（`LoadAttackSabukWall` 要求
   `GuildMgr.FindGuild` 命中 + 日期可解析，`UserCastle.cs:242-284`；`Run()` 只认 `Year/Month/Day` 相同的条目，`:350-354`）。
   —— 这是**替代 B1（玩家申请链抛异常）** 的唯一合法姿势，不是「修 bug」。
3. **多会话**：≥2 个客户端（守卫三态/四路落点/商人可见性都要「另一个玩家」在视野里）。
   仓库里多会话探针仍在办（`card/openmir2-multi-session-probe`），当前 `card_window.py` 每侧只起一个 probe。

**结论**：R6 的门槛是「内容冻结面 + 构建面 + 多会话工具」三件事，不是一张场景卡能解决的；
本卡按卡面要求只做**显式登记**，不编造一条「只在新月之夜能跑」的路。

### 3.1 顺带核实的「看起来可达」的两条死路

- **`@requestcastlewarnow`**：即使角色是会长、背包有祖玛头像、且不是城主会成员，
  `Castle.AddAttackerInfo(IGuild)` 仍抛 `NotImplementedException`（B1）⇒ 服务端日志有异常、客户端零帧。
- **`@ForcedWallconquestWar 沙巴克`**：Release 部署下 `GameCommand.Handle` 直接返回「权限不够!!!」（B2）。

---

## 4. 门禁（本卡无场景文件）

按卡面：「若走不可达登记路线，就没有场景文件，只需在 docs 与 status 里写清」⇒ 无 `--check-scenario` / `--dry-run`
对象，跑的是工具自身的回归门禁：

| 命令 | 结果 |
|---|---|
| `python -I -B tools/probe/mir2_probe.py --selftest` | **0（9 checks）** |
| `python -I -B tools/conformance/card_window.py --selftest` | **0（GREEN 186/186）** |
| `python -I -B tools/whitelist/t_reservation_gate.py check` | **0（GREEN：预约表 208 行 / whitelist 208 号）** |
| `python -I -B tools/hygiene/conflict_marker_gate.py` | **0（0 markers）** |

### 4.1 牙

**本卡没有场景文件 ⇒ 没有 `--check-scenario` 的 DSL 牙可做**（卡面已预判：把 `wait_for` 改成不可达集合、
或删中间 `send`，当前工具里 **T1 类「删关键 send 仍 rc=0」** 仍是缺口；T2/T5 已在 master 上补齐）。
该缺口不因本卡消失，登记指向 **`openmir2-probe-scenario-validation-gaps`**（本卡**不**写成 PASS）。

---

## 5. 覆盖 / 不覆盖

- **覆盖**：R6 的可达性结论与「4 张卡的窗口前置」；`766` bit4 的完整置位链（`RefUserState` → `InGuildWarArea`
  → `ChangePkStatus` → `UnderWar`）；`UnderWar` 的 4 个赋值点；城堡配置/守卫/城堡商人清单。
- **不覆盖**：任何真机帧（本卡零起栈）；4 张卡的构造级判据（各自卡内已跑）；`ISM_CHANGECASTLEOWNER(211)` /
  `ISM_RELOADCASTLEINFO(212)` 一类**跨服**消息（不在客户端线缆上，**未**被当作判据帧）。

---

## 6. 残留项

1. **owner 裁决项**：`StartCastlewarTime` 的窗口值（内容面）、DB 双行会夹具、`AttackSabukWall.txt` 夹具、
   多会话探针（`card/openmir2-multi-session-probe`）。四项齐了才能开 R6 窗口。
2. **T-38 保持**：`IUserCastle` 的 10 个 `NotImplementedException` 是 oracle 怪行为的 1:1 照搬，
   **不要**为了「让场景跑起来」去修它（硬约束③）。
3. **工具缺口**：T1 类（删关键 send 仍 rc=0）→ `openmir2-probe-scenario-validation-gaps`。
4. 本卡只在 worktree 内做只读核对，零起栈、零连库、零端口；无临时目录残留。
