# NPC 对话无响应标签审计（2026-11-11）

- 卡片：`openmir2-npc-dialog-label-audit`（tier L，无窗口，只读审计卡）
- 作者：principal `openmir2-worker-83`
- 分支：`card/npc-dialog-label-audit`（base = f208598ef914dac848c9380e7c68a360af7fe873）
- 触发：Owner 21:24（本地，UTC 13:24）实测 NPC 对话多处无响应；客户端侧已确认发出了
  `CM_MERCHANTDLGSELECT(1011)`。
- 性质：**只读**。本卡不改任何 Rust/C#/Envir 脚本/客户端/sql，只落本文档；所有「未验证」处
  显式标注，不作断言。

## 0. 勘误（2026-11-12，按 review 改判；后文与冲突处以本节为准）

> 本节更正 §4.1/§4.3/§6 中关于 `@zhuangyuan/@zhenai` 的传送预期。依据：review blob
> `63659930fc68251de871bbcd551194376d7b25e0`（openmir2-reviewer-26，卡
> `openmir2-npc-script-teleport-wiring`）的 `oracle_equivalence_zhuangyuan_zhenai` 与
> finding F1，及原卡 comment `d5a53a48…`。文档为历史叙事文档，正文保留初版叙事不改写。

1. **机制（oracle 语义）**：`GotoLableQuestActionProcess`（`src/Modules/ScriptEngine/ScriptEngine.cs`
   884–896）对动作列表逐项扫描，**首个已注册动作**（`ExecutionProcessing.IsRegister`）执行后
   **立即 return**；只有未注册动作才落 switch（Take/Break/Takew/Param1-4/Goto 等）并继续。
   **`MapMove`/`Map` 本身就是注册动作**（C# `ExecutionProcessingSys.cs` 138–139；Rust 生成表
   `(17, ActionOfMapMove)`/`(18, ActionOfMap)` 且在 `IMPLEMENTED_EXECUTIONS` 内）⇒ 传送标签
   若无更早的注册动作，`mapmove/map` 执行后即返回、传送可达。`SET` 亦是注册动作 ⇒ 脚本先
   `set` 的标签，`set` 执行完本 procedure 即返回，其后的 `mapmove/map` **不可达**。Rust 引擎
   同构（首个 execution-implemented 动作执行后 return）⇒ 两栈等价。
2. **§4.1 `@zhuangyuan` 改判**：动作序 `checklevel 11 → set [168] 1 → mapmove GA0 69 71`；
   Level≥11 时首个注册动作是 `set [168] 1`，执行即 return ⇒ **两栈都不传送**。正文写的
   「oracle 应执行 set + mapmove、传送庄园 GA0 (69,71)」是**错误预期**（§4.1 首条与 §2 表格
   「客户端预期」列同此误）。
   `set [168] 1` 的持久化缺口仍属 §6 卡 B 面，与本勘误无关。
3. **§4.3 同族（`@zhenai`）**：动作序 `CHECKLEVELEX>34 + checkgold 1000 → take 金币 1000 →
   set [168] 1 → map g002`——`TAKE` 未注册、走 switch 扣金后继续，随后 `set` 注册即 return
   ⇒ 条件满足时**扣 1000 金币但不传送**（`map g002` 不可达）；条件不满足 ⇒ `SM_MENU_OK(767)`。
4. **§4.1 其余传送标签预期不变**：`@bai`（mapmove 11 136,368）与 `@feng`（mapmove 4 241,202）
   的 `mapmove` **本身就是首个（唯一）注册动作**——执行即 return、传送可达；`@du`（take →
   mapmove 2 416,564）与 `@gohj2`（take → mapmove h001 74,69）的 `take` 未注册、走 switch
   处理后继续，随后的 `mapmove` 执行并返回。四条传送**可达**；§4.2/§4.3 对这四条的传送缺口
   结论维持。
5. **`@ss` 更正（含对本勘误沿自 review `63659930…` F4/A4 note 误判的更正声明）**：
   `比奇省传送员-0.txt` **没有** `@ss`——不存在 `<随机传送/@ss>` 链接（前次 review 把该文件
   `@main` 的 `<去[真爱质询屋]/@zhenai>` 误标成了 `<随机传送/@ss>`，此处一并更正留痕）。
   `@ss` 实际在 `盟重省传送员-3.txt` 的 `[@shangdian]` 段内（`<首饰店/@ss>`，75 行），且
   `[@ss]`（90–93 行）存在、内容为 `map 0158` ⇒ **非悬空链接，本身就是活的 MAP 样本**
   （不经 `@main` 触达）。MAP 随机落点窗口样本：`盟重省传送员-3 @yf`（85–88 行，map 0149）、
   `会战-3 @dian9`（map ZM6，需 HAVEGUILD+Level>34），或径用 `盟重省传送员-3 [@ss]`
   （map 0158）做跨 NPC 验证。
6. **§6 卡 A 边界同步（已收口）**：卡 A（`openmir2-npc-script-teleport-wiring`）已按上述
   改判口径实现并 closed；A4 真机窗口已过——`@bai` 真机 DB=map 11 (136,368)。
   §6 卡 A 验收文字里「Level≥11 会话点 `@zhuangyuan` ⇒ 落 GA0 (69,71)」（§6 卡 A「验收」段）
   按本节第 2 条作废，真机判据改为：不传送、核 800/换图簇均不发。

## 1. 证据源

| 证据 | 位置 | 说明 |
| --- | --- | --- |
| 服务端原始行 | `E:/tmp/m2-acceptance/stack/game-svr.out.log`（UTC） | 行号 65247–65318 为 Owner 实测批 |
| sim 命令行 | 同上 | 1011 的 `Recog` 字段直接记录了 NPC 实体 id，label 在 `tail` |
| 商人清单 | `E:/MirServer/M2GameSvr/Envir/MerChant.txt`（GBK） | game-svr 配置 base=`E:\MirServer\M2GameSvr`、`EnvirDir=Envir`（启动日志第 8 行）⇒ Envir 根即该目录 |
| 商人脚本 | `E:/MirServer/M2GameSvr/Envir/Market_Def/...`（GBK） | 用 `python -I` 按 GBK 解码后逐行比对，非二进制 grep |
| 角色状态 | `mir2_db.characters`（只读 SELECT） | `aaa`：Level=50、Gold=864、MapName=0、CX=331、CY=268 |
| oracle 参照 | `src/` 冻结的 C# 参照实现（只读对照） | `ScriptEngine.cs` 的 `GotoLable`/`GotoLableSendMerChantSayMsg`、`Merchant.cs` 的 `UserSelect`（sEXIT 臂）、`PlayObject.Base.cs` 的 `GetScriptLabel`、`PlayObject.Message.cs` 消息泵 |

限制：game-gate 不逐帧落日志，**没有**帧级抓包证据；本卡的帧面结论凡未抓帧处都标「未决」。

## 2. 观测清单（UTC 13:24:23–13:24:57）

「服务端结果」引用 game-svr.out.log 原行；「客户端预期」= 按 oracle 语义该标签**应当**产生的
可见响应。

| 时间 | 角色 | NPC id | label（1011 Recog） | 服务端结果 | oracle 客户端预期 | 实测 |
| --- | --- | --- | --- | --- | --- | --- |
| 13:24:23.492 | aaa | 13 | —（1010 Click） | `@main 说词 1 条`（INF） | 对话框打开 | 正常 |
| 13:24:25.918 | aaa | 13 | `@zhuangyuan`（Recog=13） | 无说词（DBG） | 传送庄园 GA0(69,71) | **无响应** |
| 13:24:27.198 | aaa | 13 | `@zhuangyuan` | 无说词 | 同上 | **无响应** |
| 13:24:27.820 | aaa | 13 | `@zhuangyuan` | 无说词 | 同上 | **无响应** |
| 13:24:28.904 | aaa | 13 | `@du`（Recog=13） | 无说词 | 传送毒蛇山谷 2(416,564) 且金币−1000；或弹「金币不足」框 | **无响应** |
| 13:24:32.080 | aaa | 13 | `@zhuangyuan` | 无说词 | 同 13:24:25 | **无响应** |
| 13:24:32.678 | aaa | 13 | `@zhuangyuan` | 无说词 | 同上 | **无响应** |
| 13:24:33.783 | aaa | 13 | `@huanjing`（Recog=13） | 说词 1 条（INF） | 幻境说明页 | 正常 |
| 13:24:35.916 | aaa | 13 | `@main`（Recog=13） | 说词 1 条（INF） | 回主页 | 正常 |
| 13:24:38.672 | aaa | 132 | —（1010 Click） | `@main 说词 1 条`（INF） | 对话框打开 | 正常 |
| 13:24:41.906 | aaa | 132 | `@元宝`（Recog=132） | 说词 1 条（INF） | 领取元宝页 | 正常 |
| 13:24:45.142 | aaa | 132 | `@exit`（Recog=132） | 无说词（DBG） | `SM_MERCHANTDLGCLOSE(644)` 一帧（header-only，Recog=132） | **无响应（商人路径漏接 sEXIT 臂，见 §4.4）** |
| 13:24:46.739 | aaa | 8 | —（1010 Click） | `@main 说词 1 条`（INF） | 对话框打开 | 正常 |
| 13:24:50.656 / 53.577 | aaa | — | —（1010） | `ClickNpcTime 限频内 ⇒ 忽略` | — | 正常分支 |
| 13:24:52.418 | aaa | 13 | —（1010 Click） | `@main 说词 1 条`（INF） | 对话框打开 | 正常 |
| 13:24:55.364 / 368 | aaa | 13 | `@bai`（Recog=13）×2 | 无说词（DBG） | 传送白日门 11(136,368) | **无响应** |
| 13:24:57.121 | aaa | 13 | `@du`（Recog=13） | 无说词（DBG） | 同 13:24:28 | **无响应** |

关键旁证：13:24:56.043 的 sim tick 行显示 `aaa(37473) map=26`（内部注册表 id，对应逻辑图 `0`），
且整批 1010 Click 的「同图 ±15」门在 13:24:46/13:24:52 两次通过 ⇒ 整个实测期间玩家**从未离图**；
DB 落库 CX=331、CY=268 与 13:24:21 的走位落点一致。`@bai` 是无条件 `MAPMOVE`，oracle 下必传送，
故「没有传送」是服务端事实，不依赖客户端观测。

## 3. NPC id → 脚本文件映射与证据

**装载链（启动日志 + 代码静态证据）**：

1. game-svr 启动参数 `config=E:\MirServer\M2GameSvr\Server.conf`、`EnvirDir=Envir`
   ⇒ 脚本根 `E:/MirServer/M2GameSvr/Envir`。
2. 商人清单 = `Envir/MerChant.txt`（GBK）。Rust 按 oracle 字面量读 `"Merchant.txt"`
   （crates/envir 商人装载器内注释明确：Windows 大小写不敏感，盘上文件名是 `MerChant.txt`）。
3. 启动日志：`隐藏商人 QFunction 已进世界 MerchantList（id=1…）`；
   `Envir/Merchant.txt 已载入：131 条（FindMap 门丢 0 条，挂图失败 0 条）`；
   `Envir/Npcs.txt 装载完成：3 条配置`。
4. 实体 id 分配是稠密单调计数（world-bevy `EcsStore::alloc_id`：`next_id` 自增），
   装载序 = QFunction(1) → MerChant.txt 装载序 131 条（id 2..132）→ 固定 NPC 3 条（id 133..135）。
   故 `id = 装载序下标 + 2`。**口径注明（review 22dfa829 F2）**：盘上 `MerChant.txt` 原始
   162 行 = 22 空行 + 9 个 `;` 注释行 + 131 条有效记录（本卡实测计数）；下标一律指**剔除
   空行/注释行之后**的装载序（与启动日志「131 条」吻合），不是盘上行号。
5. 独立交叉验证：CM_CLICKNPC 有「同图 ±15」门，整批三次 Click 都到达 Click 点，
   而玩家全程在 map `0` (331,268) 附近 ⇒ 三个 id 必是 ±15 内仅有的三条 MerChant 记录：

| NPC id | 装载序下标 | MerChant.txt 记录（GBK 解码，装载序） | 脚本文件（Market_Def/） | ±15 核验（玩家 331,268） |
| --- | --- | --- | --- | --- |
| 13 | 11 | `传送员/比奇省传送员 0 329 266 比奇老兵` | `传送员/比奇省传送员-0.txt` | dx=2, dy=2 ✓ |
| 8 | 6 | `举证 0 334 264 官方举证制度` | `举证-0.txt` | dx=3, dy=4 ✓ |
| 132 | 130 | `元宝充值 0 332 264 元宝充值使者` | `元宝充值-0.txt` | dx=1, dy=4 ✓ |

   再加 log 本身的一重证据：1011 的 `Recog` 与 label 的配对（`Recog=13`+`@zhuangyuan/@du/@bai`、
   `Recog=132`+`@元宝/@exit`）与上表逐条吻合。

## 4. 无响应标签根因分类

冻结脚本按 GBK 解码后精确比对 `[@label]`。传送员脚本原文（关键段，GBK 转码后）：

```text
[@zhuangyuan]  #if / checklevel 11 / #ACT / set [168] 1 / mapmove GA0 69 71 / #elseACT / messagebox … / break
[@du]          #IF / checkgold 1000 / #ACT / take 金币 1000 / mapmove 2 416 564 / #elseACT / messagebox 金币不足 / break
[@bai]         #IF / #ACT / mapmove 11 136 368 / break
[@bai◆] [@feng◆] [@feng] [@zhenai] [@gohj2] [@cha] [@jiangli*] 亦在文件内
```

### 4.1 `@zhuangyuan`（npc=13）—— Rust 漏接线，非脚本缺标签

- 标签**存在**（`传送员/比奇省传送员-0.txt`）。玩家 Level=50 ≥ 11 ⇒ `#if` 恒真支
  ⇒ oracle 应执行 `set [168] 1` + `mapmove GA0 69 71`（传送庄园）。
- Rust 生产路径两处**静默丢失**：
  1. `mapmove` → 引擎 `ActionOfMapMove` 调 `ScriptPlayer::space_move`；该 trait 方法的**缺省实现
     是空操作**（crates/script engine 模块注释明说「真世界适配层可覆写」），而
     `bins/game-svr` 的 `NpcPlayer`（`impl ScriptPlayer for NpcPlayer`）**没有覆写它**
     ⇒ 商人路径传送永不发生。
  2. `set [168] 1` → 写进 `NpcPlayer` 每次调用新建的**全零位图**；`npc_player_from_session`
     不 seed 会话 quest_flag，`run_merchant_goto` 也没有 quest_flag 写回 ⇒ 脚本结束即丢。
- 两者都是「代码缺接线」，与脚本内容无关。同族：`MAP` 动作（`map g002`/`map h001`）走的
  `map_random_move`/`send_ref_msg` 同为 trait 空缺省、同样无覆写。

### 4.2 `@bai`（npc=13）—— 同 `@zhuangyuan` 的传送缺口（运行时实锤）

- 标签**存在**，且是无条件 `#ACT mapmove 11 136 368`（无 `#if`）。
- 服务端日志实锤未传送：点击后 13:24:56 的 sim tick 仍 `map=26`，DB 落库仍在 (331,268)。
- 根因同 4.1 第 1 条。这是四条里唯一**不依赖条件分支**就能证明「传送动作被丢弃」的样本。

### 4.3 `@du`（npc=13）—— 传送缺口确定；messagebox 分支是否送达**未决**

- 标签**存在**。`checkgold 1000`：DB 落库 Gold=864，但这是断线后的终值，**实测时刻的金币未知**
  （两次点击若都走真支会共扣 2000 ⇒ 点击前 2864；若都走假支则全程 864）。DB 无历史，不可判。
- 真支（金币 ≥1000）：`take 金币 1000`（`dec_gold`+`gold_changed` 帧已接线）+ `mapmove`（同 4.1 缺口）。
- 假支（金币 <1000）：`messagebox 金币不足` ⇒ `SM_MENU_OK(767)`。该链路**静态完整**
  （引擎 `ActionOfMessageBox` → `NpcPlayer.frames` → pickwire 的 `script_frame` 映射
  RM_MENU_OK → SM_MENU_OK(767) 带编码尾 → 队列按 `(byte)RM_*` 优先级排水），但本窗口没有帧级
  证据证明它发出/到达；「无说词」日志只统计说词，messagebox 不计入。
- 结论：**传送缺口确定**（同 4.1）；「假支弹窗是否可达」列为未决，见 §6 卡 C。

### 4.4 `@exit`（npc=132）—— 商人 `UserSelect` 链漏接 sEXIT 臂（代码缺接线；初版判「oracle 等价零帧」有误，已按 review 22dfa829 改判）

- 脚本面事实不变：`元宝充值-0.txt` 只有 `[@main]` 与 `[@元宝]`，确无 `[@exit]` 记录——
  但这**不构成**零帧的理由：oracle 对 `@exit` 的处理不在 `GotoLable` 的记录查找里，
  而在 `UserSelect` 的**专用臂**。
- 事实链（双侧源码逐条核过）：
  1. Owner 点 `@元宝` ⇒ `#CALL QuestDiary/冲值元宝/元宝.txt` 的 `@领取元宝` ⇒ 对 aaa 全部
     `checkaccountlist` 为假 ⇒ `#ELSESAY` 页发出（即日志 13:24:41.906「说词 1 条」），
     该页页尾正是 `<关闭/@exit>`（元宝.txt 648–652 行，GBK 解码）。
  2. oracle 说词下发前 `GotoLableSendMerChantSayMsg` 调 `PlayObject.GetScriptLabel`
     （`ScriptEngine.cs` 内 `playerActor.GetScriptLabel(sMsg)` 一处）：按
     `HUtil32.Backslash`（**常量值是 `'/'`**，`src/OpenMir2/HUtil32.cs`）分词、
     `GetValidStr3` 取分隔符**后段** ⇒ `<关闭/@exit>` 被登记成 `@exit` 进 can-jmp 表
     ——登记处**不跳过** `@exit`。
  3. 本批日志 65292 走的是派发行（`GotoLable(@exit) 无说词`），而不是 lib.rs 的
     「`LableIsCanJmp(label) == false ⇒ 不派发`」行 ⇒ **Rust 侧该门同样通过**
     （`@元宝` 页说词里的 `<关闭/@exit>` 被 `get_script_label` 收进了 can_jmp 表）。
  4. oracle `Merchant.UserSelect` 的 sEXIT 臂（`src/GameSrv/Npc/Merchant.cs` 920–923）
     **无任何门**（`Merchant` 没有 `IsExit` 一类旗；相邻的 `@getbackupgnow` 等臂都要
     `Is*` 旗短路）⇒ `SendMsg(playObject, RM_MERCHANTDLGCLOSE, 0, ActorId, 0, 0)`；
     消息泵（`PlayObject.Message.cs` 的 `RM_MERCHANTDLGCLOSE` case）⇒
     `SendDefMessage(SM_MERCHANTDLGCLOSE, nParam1, 0, 0, 0)` ⇒ 客户端收到
     **`SM_MERCHANTDLGCLOSE(644)` header-only 一帧、Recog=132（NPC ActorId）**。
  5. Rust：`@exit → sm_merchantdlgclose(merchant_id)` 臂只写在 `handle_shop_cmd_inner`
     的 `is_fixed_npc`（= `me.merchant.is_none()`）分支里（lib.rs 7021 起，注释还自写
     「`@exit` 是唯一无门的分支（Merchant.cs 929–932）」）；商人 132 是 merchant ⇒
     落到 `shop::user_select`，那里没有 `@exit` 匹配 ⇒ **零帧**。
- 结论：**与 §4.1 同家族的漏接线**（oracle 有一帧、Rust 丢帧），不是「脚本缺标签」，
  更不是「oracle 等价」。旁证：Rust 对 `GuildOfficial` 已实现同款（guild_official.rs 的
  `LABEL_EXIT` → `SM_MERCHANTDLGCLOSE`）⇒ 是商人路径漏接，不是 oracle 无此行为。
- 误判归因（review 22dfa829 F3）：初版按字面 `@exit` 全仓 grep，只命中链接收集解析器
  （ScriptParsers.cs 1075 附近，收集对话链接时跳过 `@exit`/`@close`/`@main`——那处与
  运行时无关）和 `LocalDB` 生成的 QFunction 文本，漏掉 `ScriptFlagConst.sEXIT` 常量的
  三处 `UserSelect` 使用点（Merchant/GuildOfficial/CastleOfficial）；正确检索口径是
  按 `sEXIT` 常量使用点找运行时臂。

### 4.5 日志文案勘误（文档级，不改代码）

「`GotoLable(...) 无说词（脚本无该标签/文件缺失）`」的括注**不可按字面读**：该行只表示
「说词集合为空」。对 `@zhuangyuan/@du/@bai` 这类纯动作标签，**oracle 下说词也为空**；
本次四条里字面上唯一「无该标签」的是 `@exit`，但其「无响应」另有根因（漏接 sEXIT 臂，
见 §4.4），该行同样给不出定位。后续读日志判案时，不能把该行当成「标签缺失」的证明。

## 5. 已实现（本批正常工作）清单

| NPC | 脚本 | 已验证工作的标签 |
| --- | --- | --- |
| 13 比奇省传送员 | `Market_Def/传送员/比奇省传送员-0.txt` | `@main`、`@huanjing`（纯说词） |
| 132 元宝充值使者 | `Market_Def/元宝充值-0.txt` | `@main`、`@元宝`（含 `#CALL [\冲值元宝\元宝.txt] @领取元宝` 的跨文件调用，说词照出） |
| 8 官方举证制度 | `Market_Def/举证-0.txt` | `@main`（该脚本只有 `@main` 一个标签） |

固定 NPC（`Envir/Npcs.txt` 3 条）与隐藏商人 QFunction 装载正常（启动日志），`Npcs.txt` 的
`Npc_def` 脚本 3 条全部载入、无缺失。

## 6. 后续修复卡建议（本卡不动代码）

### 卡 A：脚本传送动作接线（MAPMOVE/MAP，tier 建议 R，真机窗口）

- 背景：`@zhuangyuan/@bai`（及未点击的 `@feng/@zhenai/@gohj2`）在商人路径全部静默不传送。
  部署 Envir 影响面（`Market_Def/` 142 个 txt，GBK 计数）：`MAPMOVE` 107 行、`MAP` 22 行
  （另有 `MESSAGEBOX` 356 行、`SET [n]` 32 行与卡 B/C 相关）。
- 写集建议：`mir2-rs/bins/game-svr/src/npc.rs`（`NpcPlayer` 覆写 `space_move`/`map_random_move`/
  `send_ref_msg`，按既有 `pending_map_modes` 先例把「待传送」排进会话 pending，不引世界借用）、
  `mir2-rs/bins/game-svr/src/lib.rs`（`run_merchant_goto` 写回 + 1011/1010 各调用点应用：
  世界侧换图/换点、AOI 与可见表重建、会话 map 字段与落库面）；世界侧换图原语优先复用
  crates/world 既有 SpaceMove/入图件，不新造。引擎 trait 缺省实现**不改**（flow 夹具口径不动）。
- 验收（R）：真机窗口点比奇省传送员 `@bai` ⇒ 玩家落到 map `11` (136,368)；Level≥11 会话点
  `@zhuangyuan` ⇒ 落 GA0 (69,71)；帧面（SPACEMOVE_FIRE 扇出、玩家自身换图帧清单照 oracle
  `BaseObject.SpaceMove` 逐帧抄，卡上先抄清单再动手）+ DB 面 CX/CY/MapName 落库一致；
  变异牙：摘掉 pending 应用点必红。
- 风险：换图是会话级状态迁移（AOI、可见表、落库、网关路由都碰），是本卡主要工作量；
  `MAP`（随机落点）带 RNG 面，建议拆第二片；131 个商人脚本全量回归窗口成本高，
  可用代表性三图（0/11/GA0）+ 全量单元牙覆盖。

### 卡 B：商人路径 quest_flag round-trip（tier L）

- 背景：`set [168] 1` 写进每次新建的全零位图、无写回；同会话后续 `CHECK [n]` 恒读 0，
  与 oracle「同一 PlayObject 字段」语义不符（登录路径已有 quest_flag 语义，两路必须对齐）。
- 写集建议：`mir2-rs/bins/game-svr/src/lib.rs`（`npc_player_from_session` seed +
  `run_merchant_goto` 写回，照 `pending_map_modes`/offline 四字段同法）。
- 验收（L）：脚本 `set [168] 1` 后同会话脚本 `CHECK [168] 1` 为真；未命中 SET 的调用恒等回写
  （不清零既有位图）；DB 面核 quest_flag 落列与登录路径一致。
- 风险：低；注意与登录路径的 seed 源保持同一字段，避免两处口径漂移。

### 卡 C：`@du` messagebox 分支取证（小型 R 片，可与卡 A 同窗口）

- 背景：§4.3 的未决——假支 `SM_MENU_OK(767)` 静态链路完整，但缺帧级实锚。
- 做法：构造金币 <1000 的会话点 `@du`，帧面断言收到 `SM_MENU_OK(767)` 且尾为
  cp936「金币不足」；顺带钉「`无说词` 日志行不区分标签缺失与纯动作标签」的判据口径
  （判案改用 frames 面，不读该行字面）。
- 风险：若取证结果为「已送达」，则 Owner 侧观感与帧面矛盾要回到客户端面另查（本卡不下该结论）。

### 卡 D：商人路径 `@exit` 臂补接线（tier L，可与卡 A 同窗口顺带取证）

- 背景：§4.4——oracle `Merchant.UserSelect` 的 sEXIT 臂无门，玩家点 `@exit` 必回
  `SM_MERCHANTDLGCLOSE(644)` 一帧；Rust 该臂只写在 `is_fixed_npc` 分支里，商人路径零帧
  （Owner 实测的「无响应」之一）。
- 写集建议：`mir2-rs/bins/game-svr/src/lib.rs`——把 `@exit → sm_merchantdlgclose(merchant_id)`
  从 `is_fixed_npc` 分支提为商人/固定 NPC 共用，位置放在 `LableIsCanJmp` 门之后、
  `is_fixed_npc` 分叉之前；口径复用 guild_official.rs 已有的 `LABEL_EXIT`/
  `PRI_MERCHANTDLGCLOSE`（那处就是按 oracle `GuildOfficial.cs` 78–88 移植的同款臂）。
- 验收（L，静态可断言）：真机（或并卡 A 窗口）复现 §4.4 的点击序列（npc=132 →
  `@元宝` → `@exit`）⇒ 客户端收帧恰一条 `SM_MERCHANTDLGCLOSE(644)`、**header-only 12B**、
  Recog=132（= 消息自带的 NPC ActorId，不是玩家会话 id）；变异牙：摘掉该臂必红；
  `LableIsCanJmp` 门保持原样（未登记 label 仍走「不派发」行）。
- 风险：低（单臂、无世界状态迁移、无 RNG 面）；注意固定 NPC 既有臂行为不得回退
  （`is_fixed_npc` 分支的既有断言/日志文本保持）。

### 不立卡

- 「无说词」日志文案：勘误记录在案（§4.5），是否改文案由 Leader 裁（改了会动多卡基线文本，非必要）。

## 7. 限制与未决清单

1. **没有离线重放 Rust 解析器**验证三条 label 是否进了脚本表（本卡不跑 cargo）。间接证据：
   同文件的 `@main/@huanjing` 运行时命中（表存在）、`@元宝` 的 `#CALL` 命中、解析器对
   `#IF/#ACT` 为 ASCII 大小写不敏感比对、且这三段是 Mir2 脚本最普通形态。若卡 A 开工时
   发现记录缺失，则以「漏解析」另登记（当前证据不支持该假设）。
2. **实测时刻 aaa 的金币未知**（864 是断线落库终值）⇒ `@du` 真支/假支不可判（§4.3/卡 C）。
3. **无帧级抓包**（game-gate 不逐帧落日志）⇒ 所有「是否发出某帧」均为静态链路证据；
   `@exit` 的「oracle 发 644 / Rust 商人路径零帧」同样属双侧静态源码结论（方向性由两侧
   独立成立，未做 C# 实跑对拍）。
4. **客户端侧行为**（弹窗显示、`@exit` 本地关窗）在冻结客户端内，不可观测、不在本仓职责面。
5. 本文档为历史叙事文档，不进 anchor registry；正文只写文件路径与符号名，不写行号锚。
6. DB 侧仅做只读 SELECT（`characters`、`characters_quest`）；`characters_quest` 中 `aaa`
   无行，与「商人路径 quest_flag 不落库」一致，但不能区分「没写」与「写了没存」。

## 8. 交付信息

- 分支 `card/npc-dialog-label-audit`；改动恰一个新文档 `docs/npc-dialog-label-audit-20261111.md`。
- 提交史：57360067f 初版审计；随后按 review 22dfa829（openmir2-reviewer-26，
  request_changes）追加改判提交——F1：@exit 改判为「商人 `UserSelect` 链漏接 sEXIT 臂」
  （§4.4）、§2 该行 oracle 预期改 644 一帧；F2：§3 补「原始 162 行 → 131 条装载序」口径；
  F3：改写链接收集解析器的误述并登记误判归因；§6 撤销 @exit「不立卡」、新增卡 D。
  A1/A2/A3 已 PASS 的内容未放松。
- 门禁（改判提交后复跑）：`python -I -B tools/hygiene/docs_anchor_gate.py --prose --base f208598ef914dac848c9380e7c68a360af7fe873`
  rc=0；`git diff --check` rc=0；`git diff --numstat f208598ef..HEAD` 仍恰 1 个 docs 文件。
- 只读声明：未改 Rust/C#/Envir/客户端/sql；MerChant.txt 与脚本文件只读解码；
  DB 只读 SELECT。
