# R2 场景 `upgrade_retrieve`：武器升级 NPC 全链（卡 `openmir2-scenario-upgrade-retrieve`）

- 基线：`walgit/master` = `302038193`；本卡 worktree `E:/tmp/wt-worker-69`，分支 `card/scenario-upgrade-retrieve`。
- tier = **L**：只造场景夹具（`tools/probe/scenarios/upgrade_retrieve.json` + 本文），零生产代码、零帧字节；
  真跑与逐抽对拍由三张 R 卡的窗口承担。
- 场景：`tools/probe/scenarios/upgrade_retrieve.json`（15 步），placement `map=3 (320,328)`。
- sha256（提交后树）：**`2a8323660fc77f2e31583056bc3ae9de83530db5f1b55d7eb957fd8063054934`**

---

## 0. 结论先行

1. **这不是「不可达登记」。** 卡面担心的 `TAKECHECKITEM` 在部署里确实 **0 命中**，但升级 NPC 走的是
   **另一条链**：`Market_Def/沙巴克铁匠-3.txt` 脚本头第 1 行把 `@upgradenow`/`@getbackupgnow` 声明成
   **系统命令**（`ScriptParsers.cs:741-749` 置 `IsUpgradenow`/`IsGetBackupgnow`），派发在
   `Merchant.UserSelect`（`Merchant.cs:883-899`）。NPC 本体也在部署里：`MerChant.txt:82`
   `沙巴克铁匠 3 332 317 沙巴克铁匠 0 0 0`（map 3 = 盟重省，`MapInfo.txt:705`）。
2. **场景实际可达提交链**：点 NPC → `@main` 643 → `@upgrade` → `@confirmupgrade` → `@upgradenow`
   （成功首帧 `SM_DELITEM(202)`，失败只回 643）。
3. **取回链要过 10 分钟时效门**：部署 `UPgradeWeaponGetBackTime=600000`（`Server.conf:331`），
   `Permission>=4` 的豁免在本部署**不可用**（`StartPermission=0` + `AdminList.txt` 唯一行缺 IP 被丢弃）。
   场景用 **`sleep 660000`** 把这条门编码进去 ⇒ **每次窗口 run 至少 11 分钟**，
   窗口 profile 必须把 **`probe_timeout_ms`** 抬到 ≥ 720000（见 §4.5）。
   > 订正（原：`timeout_ms`）：profile 的键名是 `probe_timeout_ms`（内置 alt 见 `card_window.py:297`；
   > CLI 是 `--timeout-ms`）；**写成 `timeout_ms` 会被静默忽略**（profile 顶层没有未知键校验），
   > 结果 probe 仍在 180 秒被杀 ⇒ 窗口白跑一轮。
4. **placement 取 (320,328) 的三条理由**（订正：原写「不能站铁匠邻格，否则被三只弓箭护卫射杀」是**假前提**，见 §4.2）：
   ① `.map` 该格 `bk=671/fr=0` 可走；
   ② 到铁匠 (332,318) 的 `|dx|=12, |dy|=11`，在 `CM_CLICKNPC` 的 `|dx|<=15 && |dy|<=15` 门内（`PlayObject.Base.cs:691`）；
   ③ x=332 是本行候选里 x 最大 ⇒ discovery 列序把 `auto:last_turn` 落在铁匠（§4.4）。
   **刷怪威胁不成立**：那三只的名字是 `弓箭护卫`，不在 `monsters` 表 ⇒ 根本不刷（证据见 §4.2）。
5. **Rust 侧有两个已登记缺口会让窗口变红**（不是场景问题）：升级结果 say 未执行（`T-145④` / `T-95④`：
   `@upgradenow`/`@getbackupgnow` 分支只登记 `goto_label`、不跑 `GotoLable`）。本场景的硬门放在**唯一 ident**
   （`709`/`200`）上，所以「提交/取回是否真的发生」不依赖 say；say 差异会以**多出来的 643 帧**形式出现在
   compare 里。

---

## 1. 第 1 步：可达性统计（先证可达，再设计）

### 1.1 口径（与卡面一致，不用 `grep -c`）

```sh
cd /e/MirServer/Mir200/Envir
find . -type f -print0 | xargs -0 grep -aoiE '^[[:space:]]*<CMD>' | wc -l
```

> 部署内容 `E:/MirServer/Mir200/Envir` 与 `E:/MirServer/M2GameSvr/Envir` 的 `MerChant.txt`、
> `Market_Def/沙巴克铁匠-3.txt` 逐字节相同（`cmp` 已验证）；下文行号对两边是同一份。

| 类别 | 结果（行数） | 说明 |
|---|---|---|
| 动作 `TAKECHECKITEM`（`ExecutionCode.cs:44`） | **0** | 部署脚本零命中 ⇒ 该动作链不可达（本场景不用） |
| 动作 `UPGRADEITEM`（`ExecutionCode.cs:430`） | **0** | 精确匹配（排除 `UPGRADEITEMEX`） |
| 动作 `UPGRADEITEMEX`（`ExecutionCode.cs:432`） | **34** | 4 个文件，明细见 §1.2 |
| 动作 `UPGRADEDLGITEM`（`ExecutionCode.cs:757`） | **59** | 客栈-0148/回收-3/比武-GA2；**oracle 实现是空函数**（见 §1.3），零副作用 |
| 动作 `RANDOMUPGRADEITEM`（`ExecutionCode.cs:592`） | **0** | 零命中 |
| 条件 `CHECKUPGRADEITEMS`（`ConditionCode.cs:359`） | **0** | 零命中 |
| 脚本里 `^CM_[A-Z0-9_]+` | **0** | `CM_*` 是线上协议号、不是脚本语法；本层走 `CM_MERCHANTDLGSELECT(1011)`（`Messages.cs:63`） |
| `@upgradenow` 类标签 | **18 处 / 17 行**（见 §1.3；订正：原写「16 行」） | `@upgradenow`/`@getbackupgnow` 只在沙巴克铁匠-3.txt；`@UPGRADEDLGITEM` 是脚本标签、不是系统命令 |

### 1.2 `UPGRADEITEMEX` 34 行明细（`file:line`）

| 文件 | 行数 | 行号 |
|---|---|---|
| `Market_Def/功能NPC/勋章-0146.txt` | 15 | 231,232,235,257,258,261,283,284,287,309,310,313,335,336,339 |
| `Market_Def/功能NPC/属性-H106.TXT` | 15 | 30,42,54,70,82,94,110,122,134,151,163,175,192,204,216 |
| `Market_Def/沙巴克铁匠-3.txt` | 3 | 59,76,93（`[@gjgj]/[@dao]/[@mo]` 的「灵符洗属性」支） |
| `Npc_Def/比奇国王-0122.txt` | 1 | 53 |

**注意（可达性真值）**：沙巴克铁匠这 3 处 `UPGRADEITEMEX` 前面的 `#IF CHECKGAMEGIRD > 0` 在两侧解析器里
都是 `ScriptError`（`CHECKGAMEGIRD`/`GAMEGIRD` 不是已注册的条件/动作码；对照
`mir2-rs/tests/parity/envir-2026-10-10/parse-stats-{cs,rust}.json` 的 `:55/57/72/74/89/91` 同点报错）。
oracle 对未知条件行只 `LogService.Error` 后**丢弃**（`ScriptParsers.cs:909-940`）⇒ 该 `#IF` 的条件表为空、
`GAMEGIRD - 1` 动作也被丢 ⇒ 一旦进入 `@gjgj/@dao/@mo` 分支，`UPGRADEITEMEX` 会**无条件执行**。
这条支不属本场景（它升级的是**已装备**物品、不是武器升级 NPC 的取回链），见 §5.2。

### 1.3 升级标签逐条（`file:line`）

```sh
find . -type f -print0 | xargs -0 grep -anoiE '@[A-Za-z0-9_]*upgrade[A-Za-z0-9_]*|@getbackupgnow[A-Za-z0-9_]*'
```

| `file:line` | 标签 | 角色 |
|---|---|---|
| `Market_Def/沙巴克铁匠-3.txt:1` | `(@upgradenow @getbackupgnow )` | **脚本头系统命令声明**（`ScriptParsers.cs:741-749`） |
| `:5` / `:104` | `@upgrade` | `[@main]` 的链接 / `[@upgrade]` 记录 |
| `:6` | `@getbackupgnow` | `[@main]` 的「取回升级武器」链接 |
| `:110` / `:113` | `@confirmupgrade` | 链接 / 记录 |
| `:120` | `@upgradenow` | `[@confirmupgrade]` 的「请求修炼」链接（**只在 `CHECKUSEITEM 1` 命中支**） |
| `:126/131/136` | `@upgradenow_ok/_ing/_fail` | `GotoLable` 回调标签 |
| `:141/146/150` | `@getbackupgnow_ok/_ing/_fail` | 同上 |
| `Market_Def/任务NPC/客栈-0148.txt:24,27`、`功能NPC/比武-GA2.txt:23,26` | `@UPGRADEDLGITEM` | `QUERYITEMDLG` 的回调标签（动作本体是空函数） |

> **部署脚本没有 `~@getbackupgnow_bagfull`**（`src/Modules/SystemModule/Const/ScriptFlagConst.cs:59`（`sGETBACKUPGFULL`）有常量、脚本没有标签；订正：原写 `:285`）⇒
> 包满支 `GotoLable` 空跳、**零帧**。任何 `wait_for` 都等不到它，本场景不覆盖（§5.2）。

---

## 2. 路线与 oracle 链（逐条 `file:line` + 消息号）

选定路线：**脚本链**（点 NPC → `@upgrade` → `@confirmupgrade` → `@upgradenow`）+
**商人系统命令**（`@getbackupgnow`）。理由：两条都在部署件里真实可达，且与三张 R 卡的判据同一条代码路径。

| # | 线上动作 | ident | oracle 出处 |
|---|---|---|---|
| 1 | `CM_CLICKNPC` | 1010（`Messages.cs:62`） | `PlayObject.Base.cs:674-696`：`IsCanDeal`/`Death`/`ClickNpcTime` 三门前置；`FindMerchant ?? FindNpc`；`同图 && |dx|<=15 && |dy|<=15` ⇒ `normNpc.Click`（`NormNpc.cs:67-73`：清 goto 账本后 `GotoLable(@main)`） |
| 2 | `SM_MERCHANTSAY` | 643（`Messages.cs:304`） | `NormNpc.SendMsgToUser`（`NormNpc.cs:1613-1616`）；每句说词重建 `CanJmpScriptLableMap`（`PlayObject.Base.cs:2775-2811`） |
| 3 | `CM_MERCHANTDLGSELECT` | 1011（`Messages.cs:63`） | `Merchant.UserSelect`（`Merchant.cs:789-812`）：`LableIsCanJmp`（`PlayObject.Base.cs:2814-2826`，`@main` 恒可跳、其余只认最近说词里的链接）⇒ 不通过则 `GotoLable` 后 `return`（零帧） |
| 4 | `@upgradenow` 派发 | — | `Merchant.cs:883-889`（`IsUpgradenow` 门）→ `UpgradeWapon`（`:529-590`） |
| 5 | 提交成功帧 | **202** | `SendDelItems` 单件重载（`PlayObject.Base.cs:1648-1665`）：`Recog=ActorId`、`Param/Tag=0`、`Series=1`、尾 `EncodeBuffer(ClientItem)`；`Messages.cs:222` |
| 6 | 后续帧 | 52 / 41 / 653 / 657×2 | `Merchant.cs:557`（`GoldChanged()`→653）、`:570-572`（`RecalcAbilitys()`→52、`FeatureChanged()`→41、`RM_ABILITY`）；`RecalcAbilitys` 尾段 657×2（`PlayObject.cs:2874-2876`）。订正：原写 `566-570` 覆盖不到 `GoldChanged`（在 `:557`） |
| 7 | 吃材料清单 | **709** | `RM_SENDDELITEMLIST`→`SendDelItems(IList)`（`PlayObject.Base.cs:1636-1646`），`Series=条数`、尾 `EncodeString("名/makeindex/…")`；`Messages.cs:357` |
| 8 | 提交结果 say | 643 | `GotoLable(~@upgradenow_ok/_ing/_fail)`（`ScriptFlagConst.cs:53-55`） |
| 9 | `@getbackupgnow` 派发 | — | `Merchant.cs:890-896` → `GetBackupgWeapon`（`:594-759`） |
| 10 | 取回成功帧 | **200** | `SendAddItem`（`PlayObject.Base.cs:198-238`）：`Recog=ActorId`、`Series=1`、尾 `EncodeBuffer(ClientItem)`（含 `Desc[10]`）；`Messages.cs:220` |
| 11 | 取回结果 say | 643 | `GotoLable(~@getbackupgnow_ok/_ing/_fail)`（`src/Modules/SystemModule/Const/ScriptFlagConst.cs:57/58/60`；订正：原写 `57-59`，把 `_fail` 与 `_ing` 弄混、且漏了 `:60`） |

### 2.1 提交支的三道门（顺序即语义，`Merchant.cs:538-541`）

```
UseItems[Weapon].Index != 0   -> 有武器（[@confirmupgrade] 的 CHECKUSEITEM 1 只是说词门）
Gold >= UpgradeWeaponPrice    -> 部署 10000（Server.conf:333）
CheckItems(BlackStone) != null-> 部署 BlackStone=黑铁矿（Server.conf:87），包里至少 1 件
```

任一不过 ⇒ 走 `~@upgradenow_fail`/`~@upgradenow_ing`，**只有 643、没有任何 202/709**。
场景把「提交真的成功」钉在 `wait_idents [709]` 上（§3），所以夹具缺件时是**红**而不是假绿。

### 2.2 取回支的门（`Merchant.cs:600-616`）

```
1  !IsEnoughBag()            (ItemList.Count >= 46) -> ~@getbackupgnow_bagfull（部署无标签 -> 零帧）
2  for e in UpgradeWeaponList: e.UserName == ChrName（OrdinalIgnoreCase）
3      (GetTickCount()-e.GetBackTick) > UPgradeWeaponGetBackTime  || Permission >= 4  -> 受理；否则 nFlag=1 -> _ing
4  取到记录后按 Dura/Dc/Mc/Sc 抽数写 Desc[10]，SendAddItem(200)
```

---

## 3. 场景本体（15 步）

| # | 步骤 | 断言/说明 |
|---|---|---|
| 1 | `click_npc merchant_actor` | recog 由 `param_discovery` 从 643.recog 发现（不写死） |
| 2 | `wait_for 643` | `[@main]` 说词（含 `/@upgrade`、`/@getbackupgnow`） |
| 3 | `dlg_select @upgrade` | 标签来自第 2 步说词 |
| 4 | `wait_for 643` | `[@upgrade]` 说词（含 `$UPGRADEWEAPONFEE`） |
| 5 | `dlg_select @confirmupgrade` | 标签来自第 4 步说词 |
| 6 | `wait_for 643` | `[@confirmupgrade]`：`CHECKUSEITEM 1` 命中支含 `/@upgradenow`；无武器则 #elsesay，第 8 步会红 |
| 7 | `dlg_select @upgradenow` | 系统命令派发 |
| 8 | `wait_for [202, 643]` | **成败两支都覆盖**：成功首帧 202；失败只回 643 |
| 9 | `wait_idents [709]` | **成功硬门**（709 只在吃黑铁/首饰时发） |
| 10 | `sleep 660000` | 过 600000ms 取回时效门（+10% 余量，理由见 §4.5） |
| 11 | `dlg_select @main` | `@main` 恒可跳（不依赖上一句说词） |
| 12 | `wait_for 643` | `[@main]` 再次下发（含 `/@getbackupgnow`） |
| 13 | `dlg_select @getbackupgnow` | 系统命令派发 |
| 14 | `wait_for [200, 643]` | **成败两支都覆盖**：成功首帧 200；`_ing`/`_fail` 只回 643 |
| 15 | `wait_idents [200]` | **取回成功硬门**（200 在整条会话里只有取回支发；登录是 201） |

设计取舍：

- **成功/失败两支**：每处 `wait_for` 的集合都同时含成功首帧与失败 say，任一分支都不会恒超时；
  再用 `wait_idents` 把「本夹具应当走成功支」钉死，避免「拿 643 蒙混过关」的假绿。
- **为什么用 `wait_idents` 而不是第二个 `wait_for`**：`wait_idents` 是累计口径，不依赖上一步是否已经把帧 pump 掉；
  `wait_for` 只认「本步开始之后」且**只按 ident 匹配**（`mir2_probe.py:291-303`）——缓冲帧也会被算进来（见 §7.3）。
- **第 10 步的 sleep**：`mir2_probe` 的 `sleep` 是裸 `time.sleep`、**不 pump**；掉线风险已核：GameGate 会话超时
  15 天（`src/GameGate/GateShare.cs:26`），GameSvr 侧没有「无输入踢人」（`OffLineFlag` 是挂机标记，不是超时踢）。

---

## 4. 夹具前置（本场景能不能跑，全看这几条）

### 4.1 角色状态（**目前仍需手工 DB 夹具**）

- 已装备一把武器（`ItemLocation.Weapon`，槽 1）；缺 ⇒ 第 6 步后无 `/@upgradenow` 链接，第 8 步超时。
- 金币 ≥ **10000**（`Server.conf:333`）。
- 背包至少 1 件 **黑铁矿**（`Server.conf:87`）；`UpgradeWaponAddValue` 会吃掉**全部**黑铁矿与**全部**首饰
  （`Merchant.cs:373-525`），别指望跑完还在。
- 背包件数 **< 46**（否则取回走包满支 ⇒ 零帧）。
- `characters.MapName/CX/CY` 由 `card_window.py` 按 `placement` 自动预置（`--dry-run` 已验证两侧 `map=3 x=320 y=328` 可走）。
- 背包/装备夹具不在 `card_window.py` 能力内（runbook §1.5 末段：归 **`openmir2-window-inventory-fixture`**；订正：原写 `openmir2-discovery-bagitems-201`，那张卡是**参数发现规则**、且已 closed）。

### 4.2 placement 选择（**原写的「护卫射杀」前提是假的，已换证据**）

> **订正（原：部署 `MonGen.txt:51/58/59` 在 (325,311)/(336,321)/(339,313) 各刷 1 只弓箭护卫，会主动射杀，
> 邻格站 11 分钟必死）**：三处刷怪行的怪名是 **`弓箭护卫`**，而 `monsters` 表**没有这一行**
> （部署 `sql/mir2_data.sql` 与窗口 baseline `E:/tmp/conformance/baseline/mir2_test_data.sql` 均 **0 行**；
> 表里只有 `弓箭手`/`弓箭守卫`/`恶魔弓箭手`）⇒ `GetMonRace` 返回 -1（`WorldServer.cs:167-175`），
> `RegenMonsters` 的 `if (monGen.Race > 0)` 直接跳过（`WorldServer.MonGen.cs:819`）⇒ **这三只根本不刷**，
> 「11 分钟必被射杀」不成立。

- **placement (320,328) 的理由（三条，都与刷怪无关）**：
  ① `.map` 该格 `bk=671/fr=0` 可走；
  ② 到铁匠 (332,318) 的 `|dx|=12, |dy|=11`，在 `CM_CLICKNPC` 的 15 格门内（`PlayObject.Base.cs:691`）；
  ③ x=332 是本行候选里 x 最大 ⇒ discovery 列序把 `last_turn` 落在铁匠（§4.4）。
- **即便真刷也不怕**（作为旁证）：`GuardUnit.SearchViewRange` 只在 `|dx|<=6 && |dy|<=6` 且视线通时才把目标
  加入 `VisibleActors`（`GuardUnit.cs:176`），而 (320,328) 到三点 17/16/19 全在 6 之外。
- **不要**把 placement 改回铁匠邻格——理由不再是「会被射杀」，而是：邻格 (332,318) 在 `CM_CLICKNPC` 门内但
  会让 discovery 的 `last_turn` 落在别的 actor 上（§4.4 的列序口径），夹具不稳。

### 4.3 时效门与 Permission

- `Permission>=4` 是本部署**唯一**能跳过 10 分钟等待的开关，但：`[Setup] StartPermission=0`（`Server.conf:365`），
  `AdminList.txt` 唯一行 `* <阿瑞斯乐园>` **缺 IP 列**，`LocalDB.LoadAdminList`（`LocalDB.cs:79-83`）直接丢弃 ⇒ 恒 0。
  改 `AdminList.txt` 属内容冻结面（禁改）。
- 升级记录是**内存态**：`SaveUpgradingList`/`LoadUpgradeWeaponRecord` 两侧都是空桩（`MySqlDB.cs:339-347`
  的 `//todo`；Rust 侧同样只活在 `MerchantState::upgrade_weapon_list` 世界内存里、`bins/game-svr` 无该表的任何读写）⇒ 不能靠「预置一条到期记录」抄近路，
  也不能跨栈/跨重启续跑。

### 4.4 discovery 为什么选这个格（`auto:last_turn` 必须落到铁匠）

- discovery 场景是 `npc_click_auto.json`（`card_window.py:163`），点击「最近一条非己 `SM_TURN(10)`」。
- 外观帧按 `VisibleActors` 的追加序发出（`PlayObject.cs:3220-3243`），而 `SearchViewRange` 是
  **x 升序、同 x 再 y 升序**（`PlayObject.cs:3094-3097`）⇒ 最后一条 = x 最大、同 x 里 y 最大者。
- (320,328) 的 `view_range=12` 窗口里可见的商人：足球场(318,332)、会战(318,336)、传送员(327,327)、**铁匠(332,317)**；
  x=332 只有铁匠 ⇒ **`last_turn` = 铁匠**。改成别的格（例如 (320,329) 会多出赌场、或把铁匠挤出 x=px+12 那一列）
  就可能点到别人，场景会**响亮失败**（点错 NPC ⇒ 没有 `@upgrade` ⇒ 第 8 步超时），不会静默通过。

### 4.5 窗口成本（**必须改 profile**）

- 场景有 `sleep 660000` ⇒ probe 进程至少跑 11 分钟。
- `card_window.py:1786` 的进程超时 = `max(180, probe_timeout_ms/1000 + 120)` 秒，默认 `probe_timeout_ms=20000`
  ⇒ 只有 **180 秒**，会被腰斩。**窗口 profile 必须设 `probe_timeout_ms >= 720000`**（= 840 秒 > 660 + 登录/余量）。
  > 订正（原：`timeout_ms`）：键名写错会被**静默忽略**，UI 上没有任何报错——probe 仍在 180 秒被杀。
  > 也可以不改 profile，直接用 CLI `--timeout-ms 720000`（等价，`card_window.py:696-697`）。
- 两侧各 11 分钟 ⇒ 一次 `card_window` 至少 ~23 分钟（含起停栈）。排窗口时按这个量级预留。

---

## 5. 覆盖到哪几张卡 / 覆盖不到什么

### 5.1 覆盖（作为窗口输入时能产出的证据）

- **`openmir2-upgrade-npc`**：提交链的帧面与顺序 `202 → 52 → 41 → 653 → [657×2] → 709`、
  取回链的 `200`、以及「提交/取回是否真的发生」的成败判别；`202/200` 的 `ClientItem` 尾（含 `Desc`）逐字节可比。
- **`openmir2-weapon-upgrade-values`**：`@upgradenow` 的**输入侧**（吃黑铁/首饰的顺序、删除清单 709）与
  取回 `200` 的落点；四字段（Dc/Sc/Mc/Dura）的数值由世界层算，帧面里只体现在 202/200 的尾。
- **`openmir2-a09-upgrade-item`**：`202/200` 的 ClientItem 尾（124B 定长、`Desc` 差量、自定义名）在真机上的
  生成路径被推到；`GetUpgradeStdItem` 的差量在 `UpgradeWaponAddValue`（吃首饰）与取回展示两处都会走到。

### 5.2 **不覆盖**（写死，别当已覆盖）

1. **RNG 抽数序**（A 耐久 → B `Random(3)` 空转 → C/D/E）：要窗口双开 `MIR2_RNG_LOG` 逐抽对拍，本场景只管把两侧
   推到同一条链上。`Desc[10]` 的具体值（1/10/11/12/20/21/22/30/31/32）本场景**不断言**。
2. **`TAKECHECKITEM` / `CHECKUPGRADEITEMS`**：部署 0 命中 ⇒ 不可达，无法造场景。
3. **`UPGRADEITEMEX` 链**（勋章-0146 / 属性-H106 / 比奇国王-0122）：可达、且带 RNG + `SM_UPDATEITEM(203)`，
   但它升级的是**已装备**物品，与武器升级 NPC 不是同一条链；本场景不覆盖，建议另开场景
   （对 `a09-upgrade-item` 的 Desc 面反而更直接）。
4. **`UPGRADEDLGITEM`**：59 处脚本调用，但 oracle 实现是**空函数**（`ExecutionProcessingSys.cs:2229-2232`）
   ⇒ 零副作用、零帧；不能拿来当升级链。
5. **包满支**（`~@getbackupgnow_bagfull`）：部署脚本没有该标签 ⇒ `GotoLable` 空跳、零帧，任何 wait 都等不到。
   本场景夹具要求背包 < 46 件。
6. **ItemNameList 5 条**：`Envir/ItemNameList.txt` 5 行，但需要包里正好有这些 `Desc[13]==1` 的自定义名物品；
   本场景的夹具只保证武器+黑铁，不保证自定义名 ⇒ 这一条仍要靠 `a09` 卡自己的夹具。
7. **税**：部署 `CastleMerchant=false`、`GetAllNpcTax=0`、`Castle=null` ⇒ 升级路径不触发税（`Merchant.cs:544-556`）。
8. **说词文本本身**：第 2/4/6/12 步只断言 ident=643，不比对 body（`$UPGRADEWEAPONFEE` 等变量展开由 compare 负责）。

---

## 6. 牙 + sha256 + 门禁（实跑，命令 + rc）

场景 sha256：**`2a8323660fc77f2e31583056bc3ae9de83530db5f1b55d7eb957fd8063054934`**
（T1~T4 每次都 `cp` 备份后逐字节还原，还原后 sha256 与基线一致）。

| 命令 | rc |
|---|---|
| `python -I -B tools/probe/mir2_probe.py --check-scenario --scenario tools/probe/scenarios/upgrade_retrieve.json` | **2**（按设计：`required` 缺 `merchant_actor`，点名缺哪个变量，fail-closed；见下） |
| 同上 + `--var merchant_actor=12345` | **0**（`scenario OK: upgrade_retrieve (15 steps, params=merchant_actor=12345)`） |
| `python -I -B tools/probe/mir2_probe.py --selftest` | **0**（8 checks OK） |
| `python -I -B tools/conformance/card_window.py --card openmir2-scenario-upgrade-retrieve --scenario tools/probe/scenarios/upgrade_retrieve.json --profile alt --dry-run` | **0**（两侧 `map=3 x=320 y=328` 图 1000x800 可走） |
| **T1** 删关键 `send` 步（`dlg_select @upgradenow`） | **0 —— 未达成，工具缺口** |
| **T2** 把 `wait_for [202,643]` 改成不可达集合 `[4095]` | **0 —— 未达成，工具缺口** |
| **T3**（对照）删掉全部 wait 步 | **2**（N-5：`has no wait step`） |
| **T4**（对照）`wait_for` 越界 u16 `70000` | **2**（`out of range [0,65535]`） |

- `--check-scenario` 不带 `--var` 返回 2 是**DSL 的 required 语义**（`mir2_probe.py:580-584`，
  「refusing to send guessed values」），与本卡禁止「写死 actor id」一致；带 `--var` 后 rc=0。
- **T1/T2 是卡面点名要求的牙，当前工具做不到**：`--check-scenario` 只校验形状/必填/placement/「至少一个 wait」，
  不校验「关键 send 是否存在」「wait_for 的 ident 是否可达」。已按卡面要求**如实登记**（不写成 PASS），
  归属卡 **`openmir2-probe-scenario-validation-gaps`**（该卡 20261023 批次也记录了
  `send.tag=99`、`send.ident=9999` 均 rc=0 的同类缺口）。T3/T4 是当前工具**确实能红**的相邻牙。

---

## 7. 残留项（只登记，不改）

1. **工具缺口（T1/T2）** —— `openmir2-probe-scenario-validation-gaps`：需要「场景必须含某个 send」
   与「wait_for ident 必须在消息表/可达集里」两类离线校验。
2. **Rust 侧结果 say 未执行（T-145④ / T-95④）** —— `@upgradenow`/`@getbackupgnow` 分支只
   `glog!("GotoLable(...) 未执行（Market_Def 商人脚本未装载，登记）")`（`bins/game-svr/src/lib.rs:5849-5853/5891-5895`），
   而 `MerchantScriptBook` 其实已装载（`lib.rs:19560-19595`）。后果：Rust 栈取回失败/未到期支**零帧**、
   成功支只缺结果 643。→ 现在的窗口会在 compare 里看到「C# 多 643」，修复前本场景不作为 PASS 依据。
3. **`wait_for` 只按 ident 匹配、且把「未 pump 的缓冲帧」算作本步回包**（`mir2_probe.py:291-303`；
   steps 之间只 `time.sleep` 不 pump，`mir2_probe.py:1153-1156`）⇒ 第 12 步可能匹配到第 8 步缓冲的结果 say。
   不影响成败（硬门在 `wait_idents 709/200`），但证据归属会指错帧。根治需 wait 支持 recog/body 过滤或「发送后新帧」口径，
   归 `openmir2-probe-scenario-validation-gaps`。
4. **背包夹具：工具已支持，本场景尚未声明**（runbook §1.6「声明即可」）——**2026-10-24 订正**：工具随 `bb4317a01` 支持场景顶层 `inventory`，本场景仍靠手工 DB 夹具只是因为**尚未写 `inventory` 声明**；且夹具卡 `openmir2-window-inventory-fixture` 的 acceptance ④（真库写入·读回）未跑、卡保持 `window_required=true` 不 closed。归属 **`openmir2-window-inventory-fixture`**（原写 `openmir2-discovery-bagitems-201` 的归属亦已订正）。
5. **窗口成本**：≥11 分钟/侧，profile **`probe_timeout_ms`** >= 720000（§4.5；订正：原写 `timeout_ms`）。
6. **`UPGRADEITEMEX` 链未覆盖**（§5.2 第 3 条），建议另开场景。
7. 本卡只在 worktree 内跑离线工具（`--check-scenario`/`--selftest`/`--dry-run`），**零起栈、零连库、零端口**；
   `--dry-run` 会打印 `restore/stack/...` 命令但 `Runner.dry_run` 不执行（`card_window.py:795`）。

## 8. 订正对照表（原错 → 现文，逐条可复算）

> 来源：review `9d3f9b14`（reviewer-6）的 5 条 minor；本卡（`openmir2-upgrade-retrieve-doc-fixes`）一次性修完。
> 每条都给「复算命令」，审查者不需要相信本文。

| # | 原错（保留原文要点） | 现文 | 复算命令 / 实测 |
|---|---|---|---|
| 1 | 「`MonGen.txt:51/58/59` 刷了弓箭护卫，邻格站 11 分钟必被射杀」，并以此解释 placement | placement 的三条理由 = 可走 + 点击门内 + `last_turn` 落铁匠；刷怪威胁**不成立**（`弓箭护卫` 不在 `monsters` 表 ⇒ 不刷） | `grep -c "'弓箭护卫'" sql/mir2_data.sql` ⇒ **0**；同命令对 `E:/tmp/conformance/baseline/mir2_test_data.sql` ⇒ **0**；`grep -c "'弓箭手'"` ⇒ 1（名字相近但不同行）。`WorldServer.cs:167-175`（GetMonRace 未命中 -1）、`WorldServer.MonGen.cs:819`（`if (monGen.Race > 0)`） |
| 2 | 窗口 profile 要 `timeout_ms >= 720000` | **`probe_timeout_ms`**（或 CLI `--timeout-ms`）；并写明「写错键名会被静默忽略」 | `card_window.py:297`（alt 内置值）、`:973-974`（传给 probe）、`:1949`（子进程超时 `max(180, v/1000+120)`）、`:696-697`（CLI 覆盖）；profile 顶层无未知键校验 ⇒ `timeout_ms` 被忽略 |
| 3 | 「`@upgradenow` 类标签 **16 行**」 | **18 处 / 17 行** | `cd E:/MirServer/Mir200/Envir && find . -type f -print0 \| xargs -0 grep -anoiE '@[A-Za-z0-9_]*upgrade[A-Za-z0-9_]*\|@getbackupgnow[A-Za-z0-9_]*'` ⇒ **18 行输出**；`cut -d: -f1-2 \| sort -u \| wc -l` ⇒ **17**（`:1` 一行有两个标签） |
| 4 | 背包夹具「归 `openmir2-discovery-bagitems-201`」 | 归 **`openmir2-window-inventory-fixture`** | `openmir2-discovery-bagitems-201` 是**参数发现规则**卡且已 closed；`card_window.py` 里 `grep -c inventory` ⇒ 0（该能力尚未进 master） |
| 5 | `ScriptFlagConst.cs:57-59`、`Merchant.cs:566-570`（含 `GoldChanged`） | `src/Modules/SystemModule/Const/ScriptFlagConst.cs:57/58/60`；`Merchant.cs:557`（GoldChanged）+ `:570-572`（RecalcAbilitys/FeatureChanged/RM_ABILITY） | `grep -n 'sGETBACKUPG' src/Modules/SystemModule/Const/ScriptFlagConst.cs` ⇒ 56 NOW / **57 OK / 58 FAIL** / 59 FULL / **60 ING**；`grep -n 'GoldChanged' src/GameSrv/Npc/Merchant.cs` ⇒ **557** |

**机判（改完实跑）**：`python -I -B tools/probe/mir2_probe.py --check-scenario --scenario tools/probe/scenarios/upgrade_retrieve.json --var merchant_actor=1` ⇒ rc=0；
`mir2_probe --selftest` ⇒ 9 checks OK；`t_reservation_gate.py check` ⇒ GREEN；`conflict_marker_gate` ⇒ 0 markers。

### 8.1 文档牙（把键名改回去必须变红）

```sh
python -I -c "
import io,sys
q=chr(96)
files=['tools/probe/scenarios/README.md','docs/upgrade-retrieve-scenario-20261023.md']
wrong=[q+'timeout_ms'+q+' 抬到', q+'timeout_ms >= 720000', '必须把 '+q+'timeout_ms'+q]
bad=[]
for p in files:
    for i,line in enumerate(io.open(p,encoding='utf-8').read().splitlines(),1):
        if line.lstrip().startswith('|'):   # 跳过 §8 的「原错」对照表行
            continue
        for w in wrong:
            if w in line: bad.append((p,i,w))
print('DOC-TIMEOUT-KEY', 'OK' if not bad else 'RED')
print('  BAD', bad[:4]); sys.exit(0 if not bad else 1)"
```

判据：**两处活文案（README 的 `upgrade_retrieve` 段、本文 §0/§4.5/§7）不得出现旧键名写法**
（脚本里 `wrong` 列的那三条）；§8 的「原错」对照表按表行跳过（以竖线开头的行）。
—— 注意两个踩过的坑：① 只用「720000 那一行必须有 `probe_timeout_ms`」不够，因为同一行还写着 CLI `--timeout-ms`；
② 不跳过对照表行会把「保留原错」本身判红。本牙实测两种写法都能挡住。

实测：基线 rc=0（`DOC-TIMEOUT-KEY OK`）；把 README 的 `probe_timeout_ms` 改回旧键名 ⇒ **rc=1（RED，打印文件+行号）**；逐字节还原后 sha256 回到基线值。
