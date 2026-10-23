# 动作 / 条件参数变量展开（`ProcessingBase.GetVariableText`）oracle 对照

- **卡**：`openmir2-script-action-var-text`（tier=R）／whitelist **T-213**／ledger 行 `action_variable_text`
- **基线**：`master 7ea1babe6`（本 worktree `card/action-var-text`）
- **结论先说**：动作/条件参数里**命名变量本来就不展开**（`$USERNAME` / `$STR(S1)` /
  `$HUMAN(X)` / `$GLOBAL(Y)` / `$ACCOUNT` …），这是 oracle 的既有行为（`IsStringNumber` 的
  `||` 缺陷），**不是**本仓缺口；真机上 C# 同样原样上线（`openmir2-action-line-variable-text`
  的窗口证据）。唯一能展开的是 `<$整数>` 形式，而部署语料里 **0 行**。
- **本卡交付**：`NpcPlayer::action_variable_text` 覆写（数字码 1/22/26）+ 6 组用例 +
  未接线码登记（T-213）。台账 `action_variable_text` 由 `needs_card` 转 `safe/已覆写`。

---

## 1. 入口链路（谁把脚本参数送进这个展开器）

| 命令 | oracle 调用点 | Rust 调用点 |
| --- | --- | --- |
| `SENDMSG` / `LINEMSG` | `ExecutionProcessingSys.cs:3162-3170` `ActionOfLineMsg`：`GetLineVariableText(playerActor, sParam2)`，随后 `%s`→玩家名、`%d`→NPC 名 | `crates/script/src/engine/actions.rs:208` |
| `MESSAGEBOX` | `ExecutionProcessingSys.cs:3418` `ActionOfMessageBox` | `actions.rs:122` |
| `CHECKONLINE` 等条件 | `ConditionProcessingSys.cs:1045/3645/3655/3796/4441/4504/4556` | `crates/script/src/engine/conditions.rs:110/389/820` |

`GetLineVariableText`（`ProcessingBase.cs:1438-1459`）的**循环骨架**已按 oracle 1:1 实现
（`crates/script/src/engine/mod.rs` 的 `expand_action_line_variable_text`，tier=L 已落地）：

```text
tempstr = sMsg;
while (tempstr.IndexOf('>') > 0) {                       // `<= 0` ⇒ 无 `>` / `>` 在位置 0 即止
    tempstr = HUtil32.ArrestStringEx(tempstr, "<", ">", ref sText);  // **前进**（返回值赋回）
    if (sText != "" && sText[0] == '$') GetVariableText(playerActor, ref sMsg, sText);
    if (++nC >= 101) break;
}
return sMsg;
```

与说词侧 `NormNpc.GetLineVariableText`（`NormNpc.cs:245-273`）的差别只有「前进 / 不前进」；
**展开器是另一张表**（`NormNpc.GetVariableText`，`NormNpc.cs:275-1630`），不得互相顶替。

## 2. `ProcessingBase.GetVariableText` 全分支（file:line + 语义）

| # | 位置 | 分支 | 语义 | 可达？ |
| --- | --- | --- | --- | --- |
| 1 | `ProcessingBase.cs:1468` → `HUtil32.cs:475-478` | `if (HUtil32.IsStringNumber(sVariable))` | `!IsNullOrEmpty(str) \|\| regex.IsMatch(str)` —— `\|\|` 让**任意非空串恒真**（whitelist **B-9**） | 恒真 |
| 2 | `:1470` | `string sIdx = sVariable[1..];` | 无条件截掉首字符（调用方已保证是 `$`） | 恒走 |
| 3 | `:1471` | `int nIdx = HUtil32.StrToInt(sIdx, -1);` | `int.TryParse`（`HUtil32.cs:359-362`）：容忍首尾空白与 `+/-` 号，**必须整串是整数** | 恒走 |
| 4 | `:1472-1475` | `if (nIdx == -1) return;` | 非整数 ⇒ **直接返回，文本原样** | `$USERNAME` / `$STR(S1)` / `$HUMAN(X)` / `$GLOBAL(Y)` / `$GUILD(G)` / `$ACCOUNT` / `$125/S1` 全在这里返回 |
| 5 | `:1475` | `GrobalVarProcessingSys.Handler(playerActor, nIdx, sVariable, ref sMsg);` | 按 `GrobalVarCode.nVAR_*` 查表分发（`GrobalVarProcessingSys.cs:20-143`） | **唯一可达的展开** |
| 6 | `:1479-1494` | `$CMD_ATTACKMODE` / `$CMD_REST` / `$CMD_UNLOCK` | 三个 `return`，`CombineStr` 全被注释（无操作） | 否（死在 #4） |
| 7 | `:1496-1521` | `$HUMAN(` | `ArrestStringEx(..., "(", ")")` → `playerActor.DynamicVarMap[名字]`；`Integer→Internet` / `String→String`；名字大小写不敏感比较，**找不到 ⇒ `sMsg = "??"`**（整条消息被替换成 `??`） | 否（死在 #4） |
| 8 | `:1523-1553` | `$GUILD(` | `playerActor.MyGuild.DynamicVarList`；`MyGuild == null` ⇒ **直接 return（原样）**；找不到 ⇒ `??` | 否 |
| 9 | `:1555-1585` | `$GLOBAL(` | `SystemShare.DynamicVarList`；名字比较用 `Ordinal`（**非** IgnoreCase，与 #7/#8 不同）；找不到 ⇒ `??` | 否 |
| 10 | `:1587-1630` | `$STR(` | `SystemShare.GetValNameNo(名字)`（`SystemShare.cs:502+`）→ 八段区间：`0-499 GlobalVal` / `1100-1109 MNVal` / `1110-1119 MDyVal` / `1200-1299 MNMval` / `1000-1099 GlobaDyMval` / `1300-1399 MNInteger` / `1400-1499 MSString` / `2000-2499 GlobalAVal`；名字号 `< 0` 时再看 `dynamicName[1]`（**下标 1，不是 0**）是 `S`→`m_StringList`（缺省 `""`）、`N`→`IntegerList`（缺省 `-1`） | 否 |
| 11 | `:1811-1827` | `CombineStr(sMsg, sStr, sText)` | `sMsg.IndexOf(sStr)` 命中**第一处**后替换；未命中返回原串（**只换一次、大小写敏感**） | 工具函数 |

> ⚠️ 任何「让 `$USERNAME` 在动作参数里展开」的改动都是**偏离 oracle**：
> 真机窗口已确认 C# 系统广播里命名变量原样（见 `docs/window-debt-reclass-20261023.md:289`
> 与 `docs/conformance-a-card-scenario-map-20261023.md:48`）。这也是 `ActionOfLineMsg`
> 专门补 `%s`→玩家名、`%d`→NPC 名（`ExecutionProcessingSys.cs:3165-3166`）的原因。
> 本仓**禁止**用说词侧展开器顶替（`crates/script/src/engine/mod.rs:148-170` 的文档已明令）。

## 3. `GrobalVarProcessingSys` 逐码表（注册顺序 = `GrobalVarProcessingSys.cs:20-135`）

「token 形状」是**能否换到文本**的决定项：只有拼出 `<$N>` 的 handler 才可能命中
`GetLineVariableText` 交进来的 `sVariable`（例：`<$46>` 传进 handler 时 `sVariable == "$46"`，
硬编码 `<$GAMEGOLD>` 的 handler 找不到，`CombineStr` 原样返回）。

| 码 | 常量 | handler | token 形状（决定 `<$N>` 是否真能换到） |
| --- | --- | --- | --- |
| 1 | `SERVERNAME` | `GetServerName` | `$"<{sVariable}>"` ⇒ `<$N>` |
| 2 | `SERVERIP` | `GetServerIp` | 硬编码 <$SERVERIP> |
| 3 | `WEBSITE` | `GetWebSite` | `$"<{sVariable}>"` ⇒ `<$N>` |
| 4 | `BBSSITE` | `GetBbsWeiSite` | 硬编码 <$BBSSITE> |
| 5 | `CLIENTDOWNLOAD` | `GetCilentDownLoad` | 硬编码 <$CLIENTDOWNLOAD> |
| 6 | `QQ` | `GetQQ` | 硬编码 <$QQ> |
| 7 | `PHONE` | `GetPhone` | 硬编码 <$PHONE> |
| 8 | `BANKACCOUNT0` | `GetBankAccount0` | 硬编码 <$BANKACCOUNT0> |
| 9 | `BANKACCOUNT1` | `GetBankAccount1` | 硬编码 <$BANKACCOUNT0> |
| 10 | `BANKACCOUNT2` | `GetBankAccount2` | 硬编码 <$BANKACCOUNT0> |
| 11 | `BANKACCOUNT3` | `GetBankAccount3` | 硬编码 <$BANKACCOUNT0> |
| 12 | `BANKACCOUNT4` | `GetBankAccount4` | 硬编码 <$BANKACCOUNT0> |
| 13 | `BANKACCOUNT5` | `GetBankAccount5` | 硬编码 <$BANKACCOUNT0> |
| 14 | `BANKACCOUNT6` | `GetBankAccount6` | 硬编码 <$BANKACCOUNT0> |
| 15 | `BANKACCOUNT7` | `GetBankAccount7` | 硬编码 <$BANKACCOUNT0> |
| 16 | `BANKACCOUNT8` | `GetBankAccount8` | 硬编码 <$BANKACCOUNT0> |
| 17 | `BANKACCOUNT9` | `GetBankAccount9` | 硬编码 `<$BANKACCOUNT0>` |
| 18 | `GAMEGOLDNAME` | `GetGameGoldName` | 硬编码 <$GAMEGOLDNAME> |
| 19 | `GAMEPOINTNAME` | `GetPointName` | 硬编码 <$GAMEPOINTNAME> |
| 20 | `USERCOUNT` | `GetUserCount` | NO-OP（无 CombineStr：空体/全注释） |
| 21 | `DATETIME` | `GetDateTime` | `$"<{sVariable}>"` ⇒ `<$N>` |
| 22 | `USERNAME` | `GetUserName` | `$"<{sVariable}>"` ⇒ `<$N>` |
| 23 | `MAPNAME` | `GetMapName` | `$"<{sVariable}>"` ⇒ `<$N>` |
| 24 | `GUILDNAME` | `GetGuilidName` | `string.Format("<0>", sVariable)` ⇒ 字面 `<0>` ⇒ **不换** |
| 25 | `RANKNAME` | `GetGuilidRankName` | `$"<{sVariable}>"` ⇒ `<$N>` |
| 26 | `LEVEL` | `GetLevel` | `$"<{sVariable}>"` ⇒ `<$N>` |
| 27 | `HP` | `GetHP` | `$"<{sVariable}>"` ⇒ `<$N>` |
| 28 | `MAXHP` | `GetMaxHP` | `$"<{sVariable}>"` ⇒ `<$N>` |
| 29 | `MP` | `GetMP` | `$"<{sVariable}>"` ⇒ `<$N>` |
| 30 | `MAXMP` | `GetMaxHP` | `$"<{sVariable}>"` ⇒ `<$N>` |
| 31 | `AC` | `GetAc` | 硬编码 <$AC> |
| 32 | `MAXAC` | `GetMaxAc` | 硬编码 <$MAXAC> |
| 33 | `MAC` | `GetMac` | 硬编码 <$MAC> |
| 34 | `MAXMAC` | `GetMaxMac` | 硬编码 <$MAXMAC> |
| 35 | `DC` | `GetDC` | 硬编码 <$DC> |
| 36 | `MAXDC` | `GetMaxDC` | 硬编码 <$MAXDC> |
| 37 | `MC` | `GetMc` | 硬编码 <$MC> |
| 38 | `MAXMC` | `GetMaxMc` | 硬编码 <$MAXMC> |
| 39 | `SC` | `GetSc` | 硬编码 <$SC> |
| 40 | `MAXSC` | `GetMaxSc` | 硬编码 <$MAXSC> |
| 41 | `EXP` | `GetExp` | 硬编码 <$EXP> |
| 42 | `MAXEXP` | `GetMaxExp` | 硬编码 <$MAXEXP> |
| 43 | `PKPOINT` | `GetPkPoint` | 硬编码 <$PKPOINT> |
| 44 | `CREDITPOINT` | `GetCreditPoint` | 硬编码 <$CREDITPOINT> |
| 45 | `GOLDCOUNT` | `GetGoldCount` | 硬编码 <$GOLDCOUNT> |
| 46 | `GAMEGOLD` | `GetGameGold` | 硬编码 <$GAMEGOLD> |
| 47 | `GAMEPOINT` | `GetGamePoint` | 硬编码 <$GAMEPOINT> |
| 48 | `LOGINTIME` | `GetLoginTime` | `$"<{sVariable}>"` ⇒ `<$N>` |
| 49 | `LOGINLONG` | `GetLoginTime` | `$"<{sVariable}>"` ⇒ `<$N>` |
| 50 | `DRESS` | `GetDress` | 硬编码 <$DRESS> |
| 51 | `WEAPON` | `GetWeapon` | 硬编码 <$WEAPON> |
| 52 | `RIGHTHAND` | `GetRightHand` | 硬编码 <$RIGHTHAND> |
| 53 | `HELMET` | `GetHelmet` | 硬编码 <$HELMET> |
| 54 | `NECKLACE` | `GetNecklace` | 硬编码 <$NECKLACE> |
| 55 | `RING_R` | `GetRing_R` | 硬编码 <$RING_R> |
| 56 | `RING_L` | `GetRing_L` | 硬编码 <$RING_L> |
| 57 | `ARMRING_R` | `GetArmring_R` | 硬编码 <$ARMRING_R> |
| 58 | `ARMRING_L` | `GetArmring_L` | 硬编码 <$ARMRING_L> |
| 59 | `BUJUK` | `GetBujuk` | 硬编码 <$BUJUK> |
| 60 | `BELT` | `GetBelt` | 硬编码 <$BELT> |
| 61 | `BOOTS` | `GetBoots` | 硬编码 <$BOOTS> |
| 62 | `CHARM` | `GetChrm` | 硬编码 <$CHARM> |
| 65 | `IPADDR` | `GetIpAddr` | 硬编码 <$IPADDR> |
| 66 | `IPLOCAL` | `GetIpLocal` | 硬编码 <$IPLOCAL> |
| 67 | `GUILDBUILDPOINT` | `GeTGuildBuildPoint` | `$"<{sVariable}>"` ⇒ `<$N>` |
| 68 | `GUILDAURAEPOINT` | `GeTGuildAuraePoint` | `$"<{sVariable}>"` ⇒ `<$N>` |
| 69 | `GUILDSTABILITYPOINT` | `GeTGuildStabilityPoint` | `$"<{sVariable}>"` ⇒ `<$N>` |
| 70 | `GUILDFLOURISHPOINT` | `GeTGuildFlourishPoint` | `$"<{sVariable}>"` ⇒ `<$N>` |
| 72 | `REQUESTCASTLEWARITEM` | `GetRequestCastlewarItem` | `$"<{sVariable}>"` ⇒ `<$N>` |
| 73 | `REQUESTCASTLEWARDAY` | `GetRequestCastleWarday` | 硬编码 <$REQUESTCASTLEWARDAY> |
| 74 | `REQUESTBUILDGUILDITEM` | `GetRequestBuildGuildItem` | 硬编码 <$REQUESTBUILDGUILDITEM> |
| 75 | `OWNERGUILD` | `GetOwnerGuild` | `$"<{sVariable}>"` ⇒ `<$N>` |
| 76 | `CASTLENAME` | `GetCastleName` | `string.Format("<{0}>")` **无实参** ⇒ 抛 `FormatException` |
| 77 | `LORD` | `GetLord` | `$"<{sVariable}>"` ⇒ `<$N>` |
| 78 | `GUILDWARFEE` | `GeTGuildWarfee` | 硬编码 <$GUILDWARFEE> |
| 79 | `BUILDGUILDFEE` | `GetBuildGuildfee` | 硬编码 <$BUILDGUILDFEE> |
| 80 | `CASTLEWARDATE` | `GetCastleWarDate` | `$"<{sVariable}>"` ⇒ `<$N>` |
| 81 | `LISTOFWAR` | `GetListofWar` | `$"<{sVariable}>"` ⇒ `<$N>` |
| 82 | `CASTLECHANGEDATE` | `GetCastleChangeDate` | `$"<{sVariable}>"` ⇒ `<$N>` |
| 83 | `CASTLEWARLASTDATE` | `GetCastlewarLastDate` | `string.Format("<{0}>")` **无实参** ⇒ 抛 `FormatException` |
| 84 | `CASTLEGETDAYS` | `GetCastlegetDays` | `$"<{sVariable}>"` ⇒ `<$N>` |
| 85 | `CMD_DATE` | `GetCmdDate` | NO-OP（无 CombineStr：空体/全注释） |
| 87 | `CMD_ALLOWMSG` | `GetCmdAllowmsg` | NO-OP（无 CombineStr：空体/全注释） |
| 88 | `CMD_LETSHOUT` | `GetCmdletshout` | NO-OP（无 CombineStr：空体/全注释） |
| 89 | `CMD_LETTRADE` | `GetCmdLettrade` | NO-OP（无 CombineStr：空体/全注释） |
| 90 | `CMD_LETGuild` | `GetCmdLeTGuild` | NO-OP（无 CombineStr：空体/全注释） |
| 91 | `CMD_ENDGUILD` | `GetCmdEndGuild` | NO-OP（无 CombineStr：空体/全注释） |
| 92 | `CMD_BANGUILDCHAT` | `GetCmdBanGuildChat` | NO-OP（无 CombineStr：空体/全注释） |
| 93 | `CMD_AUTHALLY` | `GetCmdAuthally` | NO-OP（无 CombineStr：空体/全注释） |
| 94 | `CMD_AUTH` | `GetCmdAuth` | NO-OP（无 CombineStr：空体/全注释） |
| 95 | `CMD_AUTHCANCEL` | `GetCmdAythCcancel` | NO-OP（无 CombineStr：空体/全注释） |
| 96 | `CMD_USERMOVE` | `GetCmdUserMove` | NO-OP（无 CombineStr：空体/全注释） |
| 97 | `CMD_SEARCHING` | `GetCmdSearching` | NO-OP（无 CombineStr：空体/全注释） |
| 98 | `CMD_ALLOWGROUPCALL` | `GetCmdAllowGroupCall` | NO-OP（无 CombineStr：空体/全注释） |
| 99 | `CMD_GROUPRECALLL` | `GetCmdGroupCall` | NO-OP（无 CombineStr：空体/全注释） |
| 125 | `STR` | `GetGrobalVarStr` | `$"<{sVariable}>"` ⇒ `<$N>` |


### 3.1 由表得出的「真能换文本」集合

过滤条件 = token 形状为 `<$N>` 且 handler 非空体。共 **24 个码**：

```text
1, 3, 21, 22, 23, 25, 26, 27, 28, 29, 30, 48, 49,
67, 68, 69, 70, 72, 75, 77, 80, 81, 82, 84
```

其中三个**必须照搬的怪癖**：

| 码 | 怪癖 |
| --- | --- |
| 30 `MAXMP` | 注册表把 `nVAR_MAXMP` 指到了 **`GetMaxHP`**（`GrobalVarProcessingSys.cs:36`）⇒ `<$30>` 输出的是 `WAbil.MaxHP`，不是 `MaxMP`。 |
| 81 `LISTOFWAR` | handler 先把 `sMsg` **整个覆盖**成攻城列表，再 `CombineStr(sMsg, "<$81>", sMsg)`（源与目标同一个变量）⇒ 命中与否都只剩列表文本。 |
| 125 `STR` | 形状对了，但 `sIdx = sVariable[1..]` = `"125"` → `GetValidStr3(..., "/")` 切出的 `sID` 恒为 `""` → `GetValNameNo("") = -1`（`SystemShare.cs:506` `Length >= 2` 门）⇒ **什么都不换**；`<$125/S1>` 更在 `StrToInt` 处就失败。 |

另有三个**看起来能换、其实不能**（照搬，不修）：

| 码 | 原因 |
| --- | --- |
| 20 `USERCOUNT` | handler 体整段被注释（`GrobalVarProcessingSys.cs:193-196`）⇒ 无操作。 |
| 24 `GUILDNAME` | 两分支：**有行会**时 token 是 `string.Format("<0>", sVariable)` 的**字面 `<0>`**（格式串没有占位符）⇒ 匹配不到 `<$24>`，但消息里若恰有字面 `<0>` 会被换成行会名；**无行会**时 `else { sMsg = "无"; }` ⇒ **整条消息被替换成「无」**（不是「不换」）。 |
| 76 `CASTLENAME` / 83 `CASTLEWARLASTDATE` | `string.Format("<{0}>")` **少传实参** ⇒ 运行期抛 `FormatException`（不是「不替换」，是**抛**）。Rust 侧选择原样（不抛），并登记 T-213。 |

> **整条消息替换族**（不是「往文本里换个 token」，而是把 `sMsg` 整体覆盖）：`<$24>` 无行会 ⇒ `无`；`<$80>` 的 `Castle == null` ⇒ `????`、`UnderWar` ⇒ `现正在攻城中！！！\ \<返回/@main>`、`GetWarDate()` 为空 ⇒ `暂时没有行会攻城！！！\ \<返回/@main>`；`<$81>` 命中即把整条消息换成攻城列表。本仓对这些一律**原样**（未实现页）⇒ 上述分支与 oracle 分叉；部署数字码 0 行 ⇒ 当前不可观测（T-213 **B 组**）。

## 4. 本宿主覆盖与残留

### 4.1 已实现（数据面已在本宿主）

| 码 | 变量 | oracle 取值 | 本宿主 |
| --- | --- | --- | --- |
| 1 | `<$1>` | `SystemShare.Config.ServerName`（`:203-207`） | `vars.server_name`（`NpcVars`，缺省 `SKY引擎`） |
| 22 | `<$22>` | `playerActor.ChrName`（`:234-240`） | `chr_name()` |
| 26 | `<$26>` | `playerActor.Abil.Level`（`:767-772`） | `level()` |

实现要点（`bins/game-svr/src/npc.rs` 的 `action_variable_text`）：
`strip_prefix('$')` → `trim().parse::<i32>()`（对应 `int.TryParse`，容忍前导 `0` / `+` / 空白）→
按码取值 → `combine_str(s_msg, &format!("<{variable}>"), &text)`（对应 `CombineStr`：只换第一处）。

### 4.2 残留（可展开但数据面不在本宿主）——whitelist **T-213**

| 码 | 变量 | 缺什么 |
| --- | --- | --- |
| 3 | `WEBSITE` | `SystemShare.Config.sWebSite` 未进 `NpcVars` |
| 21 | `DATETIME` | `DateTime.Now`（无注入时钟；格式串 `"dddddd,dddd,hh:mm:nn"` 本身是 .NET 自定义格式怪例） |
| 23 | `MAPNAME` | `playerActor.Envir.MapDesc`（`NpcPlayer.map_name` 承载的是 `MapName` 文件图号，不是描述） |
| 25 | `RANKNAME` | `playerActor.GuildRankName` 无 trait 取值口 |
| 27 / 28 / 29 / 30 | `HP` / `MAXHP` / `MP` / `MAXMP` | `WAbil.*`：`ScriptPlayer` 缺省回 **0**，`NpcPlayer` 未覆写 ⇒ 接上会把 `0` 发给玩家，**比原样更糟** ⇒ 保持原样 |
| 48 / 49 | `LOGINTIME` | `playerActor.LogonTime` 无字段 |
| 67 / 68 / 69 / 70 | 行会建设 / 人气 / 稳定 / 繁荣度 | `MyGuild` 只承载 `guild_name`，四点无口 |
| 72 | `REQUESTCASTLEWARITEM` | `Config.ZumaPiece` 未建模 |
| 75 / 77 | `OWNERGUILD` / `LORD` | 读的是 **`playerActor.Castle`**，不是 `NpcVars::castle` 承载的 `NormNpc.Castle`（两个对象，不能混用） |
| 80 / 82 / 84 | 攻城日期 / 占领日期 / 占领天数 | `Castle` 的日期字段未建模 |
| 81 | `LISTOFWAR` | `playerActor.Castle.GetAttackWarList()`；且该 handler 会把 `sMsg` 整个覆盖成列表（见 §3.1 怪癖） |

> `dyn_var_get` 在本方法上**无关**：`$HUMAN(` / `$GUILD(` / `$GLOBAL(` 都死在 `:1474`
> （见 §2 #4），动态变量表在这条路径上一次都不会被读到。`openmir2-script-dyn-var-host`
> 承接的是**说词侧**的 `$HUMAN(...)` 等，不是本卡。

> **B 组（照搬但会整条替换 ⇒ 未实现页与 oracle 分叉）**：`<$24>` 无行会 ⇒ 整条消息 `无`（有行会 ⇒ token 字面 `<0>`，可能误中消息里的字面 `<0>`）；`<$80>` 的 `Castle == null` / `UnderWar` / `GetWarDate()` 为空三支 ⇒ 整条消息替换；`<$76>`/`<$83>` 少传实参 ⇒ C# 抛 `FormatException`。部署 0 行 ⇒ 潜伏，但**不得当作 no-op 记账**。

## 5. 部署口径（可复算）

```sh
cd E:/MirServer/Mir200/Envir     # 台账 deploy_corpus 记的是 E:/MirServer/M2GameSvr/Envir，两者结果相同
grep -rIoh '<\$[0-9]\+>' . | wc -l      # ⇒ 0        （数字码：本方法的全部可观测面）
grep -rIh  '<\$' . | wc -l              # ⇒ 693      （全树含 `<$` 的行；卡面记 694，差 1 行为计数口径）
grep -rIiE '^[[:space:]]*SENDMSG[[:space:]].*<\$' . | wc -l     # ⇒ 378
grep -rIiE '^[[:space:]]*CHECKONLINE[[:space:]].*<\$' . | wc -l # ⇒ 14
grep -rIiE '^[[:space:]]*MESSAGEBOX[[:space:]].*<\$' . | wc -l  # ⇒ 4
```

⇒ 本方法 direct 集合（396 行）里含 `<$` 的全部是**命名变量**，oracle 也不展开；
数字码 0 行 ⇒ **当前部署下本卡改动无可观测差异**（与 T-165 的结论一致）。

## 6. 用例与牙

用例（`bins/game-svr/src/npc.rs`，均断言展开后的**字节**）：

| 用例 | 覆盖 |
| --- | --- |
| `action_variable_text_expands_numeric_codes_bytewise` | 码 1 / 22 / 26 展开 |
| `action_variable_text_repeated_values_expand_and_scan_advances` | 前进扫描（链接在前）+ 同变量两次 + 未命中不改文本 |
| `action_variable_text_leaves_unwired_and_unknown_codes_literal` | `<$27>`/`<$46>`/`<$9999>`/`<$125>`/`<$125/S1>`/`<$0>`/`<$-5>` 原样 |
| `action_variable_text_leaves_named_and_expression_variables_literal` | `$USERNAME`/`$GAMEGOLD`/`$STR(S1)`/`$HUMAN(`/`$GLOBAL(`/`$GUILD(`/`$ACCOUNT`/`$CMD_*` 原样 |
| `action_variable_text_parses_numbers_like_str_to_int` | `<$022>`/`<$+22>`/`<$ 22 >` 展开；`<$22x>`/`<$>`/`<$->`/`<$1.5>` 原样 |
| `action_variables_expand_through_messagebox_gbk_and_edcode` | 生产入口：MESSAGEBOX → 帧文本 → cp936 逐字节 → `SM_MENU_OK(767)` EDCode 尾逐字节 |

牙（详见卡线程 `patch` 的 body）：① 删覆写 ⇒ 数字码用例红；② `_ => return` 改成空串 ⇒
「未接线/未注册原样」用例红。逐字节还原后 `sha256` 见提交后的树（`git show HEAD:<file> | sha256sum`）。

## 7. 复跑

```sh
cd mir2-rs
cargo test -p game-svr --lib npc::tests::action          # 9 passed
cargo test --workspace                                    # 全量
python -I tests/parity/coverage/script_host_noop_audit.py check
python -I ../tools/whitelist/t_reservation_gate.py check
```

## 8. 窗口项（R 卡收口条件，T-213 同步登记）

| # | 分支 | 判据 | 素材 / 前置 |
| --- | --- | --- | --- |
| W-1 | **命名变量「不展开」** | 双栈跑同一脚本，`SM_*` 文本逐字节相同且 `<$USERNAME>`/`<$STR(S1)>` **原样** | `E:/MirServer/M2GameSvr/Envir/Market_Def/QFunction-0.txt:388` = `SENDMSG 0 <$USERNAME>:<$STR(S1)> 151 235`（已核实存在）；场景 = R1 `npc_script_chain`（尚未建） |
| W-2 | **数字码展开** | 两侧同一 `<$22>` 输出一致 | 冻结内容 0 行 ⇒ **真机无证据**。候选：(a) 夹具 Envir root（临时目录）加一行 `messagebox <$22>`；(b) owner 裁决以「代码级逐行等价 + 单测逐字节」替代 |

复跑：

```sh
CONFORMANCE_PASSWORD='<口令>' python -I -B tools/conformance/card_window.py \
  --card openmir2-script-action-var-text \
  --scenario tools/probe/scenarios/npc_script_chain.json \
  --out E:/tmp/conformance/runs/w213-npc-chain --profile alt --post-comment
```

> 本卡 `tier=R`：`window_required: true`；W-1 未跑或 W-2 未裁决前**不得 closed**。
