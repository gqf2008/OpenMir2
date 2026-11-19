# openmir2-npc-parity

> 状态：**进行中（2026-11-20，分支 `card/npc-parity`，base `9c84ea226`）**。
> 任务：NPC 线 **1/11 → 11/11**（分母固定 11，唯一事实源 `docs/issues/parity-gap-npc.json`）。
> 本文件是**判定面清单**（开工前交 opc-07 过审）；未过审前不铺实现。
> 对拍母版：`E:\Users\gxh\Documents\GitHub\OpenMir2\.worktrees\magic-parity\docs\issues\openmir2-magic-parity.md`（§2 装置 / §19 记录面 vs 判定面 / §19b 落地形态 / §19c 咬合证明）。
> QA 硬约束：`E:\OPC\docs\qa-charter.md`（§3 每条 verified = 测试名 + 命令 + rc + 原文；§4 比较器空输入必红、三条篡改必狠；§12 文本牙不算行为证据；§14 符号锚优先）。

## 1. 覆盖率口径（不变）

- **分母 11**：NP-01…NP-11（`docs/issues/parity-gap-npc.json`）。**本次不改分母**；若发现分母错，先停并给证据。
- `verified` 门槛：与原版**同输入**、**语义层对拍**通过（RM 序列 + SM 帧 + 状态差 + RNG 账本），每条写测试名；对外只报 `coverage-report.ps1` 输出（当前 NPC 1/11）。
- 现状：NP-06 已 verified（M4 双跑），其余 10 条 implemented（未验证）。

## 2. 判定面总则（记录面 vs 判定面）

| 面 | 归属 | 产物 |
| --- | --- | --- |
| 记录面（C# 金标准） | 新工程 `mir2-rs/tests/parity/csharp/NpcGolden/`（net8.0，只读引用 `M2Server`/`GameSrv`/`OpenMir2` 真身） | `mir2-rs/tests/parity/golden/npc/<case>.json` |
| 判定面（Rust 比较器） | `mir2-rs/tests/parity/tests/npc_*_parity.rs` | 断言 + 三条咬合用例 |

记录面每例固定四段：
1. `rm[]`：进入 `ActorEntity.MsgQueue` 的 RM（`ident/wParam/nParam1..3/priority/delivery`），按出队序；
2. `sm{}`：按接收者（caster / 视野内观察者 / 视野外）归集的**实际下发帧**（`ident/param/tag/series/tail`），走会话桩截 `SendSocket`；
3. `state`：动作前/后快照（见 §4）；
4. `rng[]`：逐次抽取值 + 抽取点（site），接缝同 magic（反射替换 `OpenMir2.RandomNumber` 的私有静态 `random`，进程内、生产零改动）。

判定面规则：
- 逐接收者逐字段比 `rm/sm`；`state` 逐量比；`rng` 行数 + 逐次 + site ∈ 允许清单；
- **防空转**：golden 缺文件 / `cases` 空 / 两侧用例数不等 ⇒ 红；
- **三条篡改必红**：① 往 golden 的某接收者 SM 里塞一条不该有的帧；② 把一条 RNG 行的 `site` 改成清单外；③ 删一条 RNG 行；
- 恢复后复绿；三条构造写进报告（QA 独立可重放）。

## 3. 帧集合（NPC 线）

| 类别 | RM | SM | 关键字段 | oracle 产生点 |
| --- | --- | --- | --- | --- |
| 说词 | `RM_MERCHANTSAY`(11009) | `SM_MERCHANTSAY`(643) | `param=MakeWord(p1,p2)`；文本 `ChrName+'/'+msg`；High 优先级 vs 普通队列 | `src/Modules/ScriptEngine/ScriptEngine.cs` 的 `GotoLableSendMerChantSayMsg`（第 987 行）；映射 `src/M2Server/Player/PlayObject.Message.cs:1702` |
| 关闭面板 | `RM_MERCHANTDLGCLOSE`(10127) | `SM_MERCHANTDLGCLOSE`(644) | `nParam1=ActorId` | `src/GameSrv/Npc/Merchant.cs:922` |
| 商品列表 | `RM_SENDGOODSLIST`(10128) | `SM_SENDGOODSLIST`(645) | `nParam1/nParam2` + 文本 | `src/GameSrv/Npc/Merchant.cs:991` |
| 明细列表 | `RM_SENDDETAILGOODSLIST`(10135) | `SM_SENDDETAILGOODSLIST`(652) | `nParam1..3` + 文本 | `src/GameSrv/Npc/Merchant.cs:1349` |
| 出售面板 | `RM_SENDUSERSELL`(10129) | `SM_SENDUSERSELL`(646) | `nParam1/nParam2` | `src/GameSrv/Npc/Merchant.cs:996` |
| 买价 | `RM_SENDBUYPRICE`(10130) | `SM_SENDBUYPRICE`(647) | | 映射 `src/M2Server/Player/PlayObject.Message.cs:1816` |
| 买成交/失败 | `RM_BUYITEM_SUCCESS`(10133)/`FAIL`(10134) | `SM_BUYITEM_SUCCESS`(650)/`FAIL`(651) | success: `Gold`, `Lo/HiWord(...)` | `src/GameSrv/Npc/Merchant.cs:1303`、`:1307` |
| 卖成交/失败 | `RM_USERSELLITEM_OK`(10131)/`FAIL`(10132) | `SM_USERSELLITEM_OK`(648)/`FAIL`(649) | | 映射 `src/M2Server/Player/PlayObject.Message.cs:1822`、`:1825` |
| 修理 | `RM_SENDUSERREPAIR`(10141)/`RM_SENDUSERSREPAIR`(50142) | `SM_SENDUSERREPAIR`(668)；cost 671；ok 669；fail 670 | | `src/GameSrv/Npc/Merchant.cs:958`；映射 `src/M2Server/Player/PlayObject.Message.cs:1852` |
| 仓库 | `RM_USERSTORAGEITEM`(10146)/`RM_USERGETBACKITEM`(10147) | `SM_SENDUSERSTORAGEITEM`(700)；`SM_STORAGE_OK/FAIL/FULL`(701/703/702)；`SM_TAKEBACKSTORAGEITEM_OK/FAIL/FULLBAG`(705/706/707) | | `src/GameSrv/Npc/Merchant.cs:1028`、`:1033` |
| 背包副作用 | — | `SM_ADDITEM`(200)/`SM_DELITEM`(202)/`SM_UPDATEITEM`(203) | 直接帧（`SendAddItem` 等） | 脚本 `give/take`、买卖、仓库 |
| 金币 | `RM_GOLDCHANGED`(10136) | `SM_GOLDCHANGED`(653) | `Gold`, `Lo/HiWord(GameGold)` | 映射 `src/M2Server/Player/PlayObject.Message.cs:1836` |
| 菜单/提示 | `RM_MENU_OK`(10309)、`RM_SYSMESSAGE`(11004) | 直发（`SendDefMessage`/`SendMsg`） | | `src/M2Server/Player/PlayObject.Operate.cs:685` 等 |
| NPC 显示 | — | `SM_TURN`(10)/`SM_HIT`(14)/`SM_FEATURECHANGED`(41)/出现帧 | 多观察者（AOI） | `src/GameSrv/Word/WorldServer.Merchant.cs:120` 的 `ProcessNpcs`；AOI 下行 |

> 说词帧的优先级：脚本 procedure 内的 say 走 `SendPriorityMsg(..., High)`（优先级 0），普通 say 走 `SendMsg`（优先级字节 = `(byte)11009 = 1`）——`src/Modules/ScriptEngine/ScriptEngine.cs:987-999`。判定面把**出队序**纳入比较（与 magic 的 `priority` 面同口径）。

## 4. 状态量

| 组 | 量 | oracle 落点 |
| --- | --- | --- |
| 脚本态 | `ScriptCurrLable` / `ScriptGoBackLable` / `ScriptLable` / `LastNpc` / `ScriptGotoCount` / 当前 `ScriptInfo` 身份 | `PlayObject` 字段；`NormNpc.UserSelect`（`:1633`）、`Merchant.UserSelect`（`:789`） |
| 玩家态 | `Gold` / 背包多重集（`Index/MakeIndex/Dura/DuraMax/Desc`）/ `QuestFlag`、`QuestUnit`、`QuestUnitOpen` 位图 / `Level`、`Exp` / `MagicList` / `HP`、`MP` / 负重 | 记录面同 `CharacterData`；任务位图见 QX-01 |
| NPC/商人态 | `IsHide` / `Merchant` 菜单标志（`IsBuy` 等，来自 ctor/配置）/ `CastleMerchant`、`Castle.UnderWar` / 行会职位 | `src/GameSrv/Npc/Merchant.cs:789-950` 的分支门 |
| RNG | 脚本 `#RAND` / `playdice` / 彩票等抽取：次数 + 逐次值 + site | `RandomNumber` 接缝（同 magic §2） |

## 5. Oracle 侧取数与语料

- **语料根**：`E:\MirServer\M2GameSvr\Envir`（冻结）：`Npcs.txt`（6 行）、`MerChant.txt`（162 行）、`Market_Def/`（141 个脚本）、`MapQuest_def/`（QManage 等）。
- **用例输入**：`{npc_kind, script_file, label, player_state, msg_sequence}`；`npc_kind ∈ {NormNpc, Merchant, CastleOfficial, GuildOfficial}`；`msg_sequence` 至少含 `CM_CLICKNPC(1010)`，可选后续 `CM_MERCHANTDLGSELECT(1011)`。
- **首发用例（真实语料，已勘察）**：任务接/交链 `任务NPC/任务-3 @rw06`（`set [107] 1`）→ `苍月岛/首饰店-5 @main`（`check [107] 1` → `@rw02` 交物品）→ `比奇城/书店-0 @rw03`；以及 M4 已跑的 7 条（`@changeGold`、`@StdModeFunc26`、`@zum_bind1`、`@requestcastlewar`、Q001@main、传送×2）。
- **抽空为止**：每个 NP 的每个分支（成功/失败/门）至少 1 条；当新增用例不再产生**新的帧类型**或**新的状态量**时停止，并把"停止判据"写进报告。
- **取数不手抄**：golden JSON 由 NpcGolden 直出；报告只引路径。

## 6. 11 条 → 判据映射（初稿）

| ID | 判据面（帧） | 判据面（状态） | 必含负例 |
| --- | --- | --- | --- |
| NP-01 点 NPC→@main | 643 说词（文本+优先级） | `LastNpc`、当前 `Script` | `IsCanDeal=false`、`Death`、超 15 格、限频：零帧 |
| NP-02 对话选择跳转 | 643 说词文本变化 | `ScriptCurrLable`/`ScriptGoBackLable` | `@back` 空栈、同标签不重复入栈 |
| NP-03 特殊 NPC 覆写 | Merchant 菜单帧（645/646/668/700…）；Castle/Guild 说词 | 金币/面板打开态 | 权限不足（CastleOfficial/GuildOfficial）零帧或失败 |
| NP-04 @main 选择规则 | 说词文本（选中的脚本） | `LastNpc`、`Script` 身份 | `LastNpc` 失效回退到逆序选择 |
| NP-05 标签+extjmp | 说词有无 | `ScriptCurrLable` | `boExtJmp && !record.ExtJmp` 被拦 |
| NP-06 procedure | 说词拼接 + 条件真/假分支 | 动作状态（金币/背包/quest 位图） | 条件假走 `#ELSESAY` |
| NP-07 断链 | 后续 procedure 不执行 ⇒ 说词截断 | 动作后的状态停在断点 | 动作返回 false |
| NP-08 说词帧 | SM 643 `ident/param/series/tail` + 出队序 | — | 空文本/无说词不发帧 |
| NP-09 变量替换 | 说词**文本字节**（`<$USERNAME>` 等） | — | 未知变量原样保留 |
| NP-10 隐藏 NPC | 对 QManage/QFunction/RobotManage 的 643 说词 | `LastNpc` | 未加载 ⇒ 零帧 |
| NP-11 显示帧 | 多观察者 `SM_TURN(10)/SM_HIT(14)/SM_FEATURECHANGED(41)/出现帧` | 观察者集合 | 视野外零帧 |

> NP-03 的边界见 §8 问题 2；若采纳建议，**菜单/面板帧**进 NP-03，**成交链**（买卖/修理/仓库的 OK/FAIL + 状态）作为 NP-06 的动作副作用用例。

## 7. 装置与产物路径（拟）

```
mir2-rs/tests/parity/csharp/NpcGolden/          # C# 金标准生成器（新）
mir2-rs/tests/parity/golden/npc/<case>.json     # golden（机器产物）
mir2-rs/tests/parity/tests/npc_<entry>_parity.rs# Rust 比较器（逐条）
docs/issues/openmir2-npc-parity.md              # 本文件（判定面/进展/咬合证明）
```
- 驱动：Rust 侧走**生产入口**（`bins/game-svr` 的 `CM_CLICKNPC(1010)` / `CM_MERCHANTDLGSELECT(1011)` 派发），复用现有 `SpellFrameRig` 同族的会话/帧收集面；不新增测试专用分支。
- 清单更新：只改 `docs/issues/parity-gap-npc.json` 的 `status`/`evidence`（分母与 axes 不动），每改一条同改 `notes` 并跑 `coverage-report.ps1`。

## 8. opc-07 裁决（2026-11-20，已定）

1. **判定面以服务端 oracle 语义为准**（"M2Server 发了什么 + 状态变成什么"），不看客户端长什么样；显示口径冲突要停并上报，不在判定面放宽。
2. **NP-03 只判"菜单/面板打开（帧 + 菜单门状态）"**；成交链（BUY/SELL/REPAIR/STORAGE 的 OK/FAIL + 金币/背包副作用）全部挂 **NP-06** 的动作副作用。
3. **出队序纳入账本**：通知类按 **`(priority, ident, 规范化 params)` 排序后逐位比，且重复条目计数**；**即时帧仍严格序**；不用"原始到达序"，也不用"纯多重集"。报告另给一条"同优先级内是否出现多个不同 ident"。
4. **只做 A 层语义；B 层暂不列**。A = 两侧各自解码后的字段级比较；**禁止共用编码器**（自证空转）。日后要字节证据须各用各的编码器再比，单列 B。
5. 附加硬要求：`DrainPassesHitCap=true` **必须红**；状态量从实体/DB 直读、不许从回包反推；分母 11 不许动；每条 verified 至少一颗**行为牙**。


## 9. 边界

- 不改分母；不碰 magic/物品/任务线文件（`tests/parity/tests/magic_*.rs`、`tests/parity/golden/magic/*`、`tests/parity/m3/*`、`docs/issues/openmir2-magic-parity.md`、`parity-gap-{magic,items,quests}.json`）。
- 只在 `card/npc-parity` worktree 提交；不 `commit -a`、不 `checkout -- <path>`、不自己合 master。
- 不派生下级代理。

## 10. 样板进展（NP-01，2026-11-20）

**装置（已落地）**
- C# 金标准生成器：`mir2-rs/tests/parity/csharp/NpcGolden/`（引用真身 `src/M2Server`；`new PlayObject()` + `new NormNpc()` + 真 `ScriptEngine`；反射调私有 `ClientClickNpc`，再按真实消息泵 `GetMessage`→`Operate` 把 RM 映射成 SM）。
- 金标准：`mir2-rs/tests/parity/golden/npc/np01_click_main.json`（4 例：ok / 超距 / Death / 限频；含 `rm[]`+`rm_priorities`+`sm[]`+`drain_passes`/`drain_hit_cap`）。
- Rust 比较器：`mir2-rs/bins/game-svr/src/lib.rs` 的 `tests::np01_click_main_matches_oracle_golden`（生产入口 `handle_click_npc_cmd`，与既有 `T137Rig` 同源）。
- 命令与 rc：`CARGO_BUILD_JOBS=6 cargo test -p game-svr --lib np01_click_main_matches_oracle_golden` ⇒ `1 passed / 0 failed`（原文见交付说明）。

**判定面（本样板）**
- 即时帧：`SM_MERCHANTSAY(643)` 严格序 + 说词尾文本（= golden `rm[0].msg`）。
- 门：超距 / Death / 限频三例零帧（golden 与 Rust 同为 0）。
- 通知账本：golden 记录 `(priority=1, ident=11009, params)`；Rust 侧该通知落在帧面（`SM 643`），**priority 不在线缆头**，故本样板按"golden 排序后的期望交付序 vs Rust 实际帧序"比较（单条时恒等）；多通知排序/重复计数在 NP-08 批量用例落地。
- 状态面缺口：`LastNpc`/`ScriptCurrLable` 在 Rust 会话**未建模**（`lib.rs` 的 T-137 残余①）⇒ 本样板不比这两项，**不伪造**；补齐后再加断言。

**QA 42 条自检（本样板）**
- R2：`parity-gap-npc.json` 的 `denominator`/`entries` 未被改动（本批不碰）。
- T2：本样板没有文本牙；行为牙见 §11（实现变异必红）。
- L3：本样板的状态只读**会话/实体字段**（`click_npc_time`、`life_cycle.death`、`transform`），不从回包反推。
- D1/D2：golden 每 case 带 `drain_passes`/`drain_hit_cap`；比较器断言 `drain_hit_cap == false`（true 即红）。
- Z1/Z2/Z3：golden 缺文件、零 cases、未知 case 名、`drain_hit_cap=true` ⇒ panic（防空转）。

## 11. 行为牙（NP-01 样板，待补原文）

在**本 worktree** 内逐条变异、跑同一条测试，期望红，恢复后 sha 相等并复绿；命令与原文将随交付说明给出（不手抄）。

1. 删 `handle_click_npc_cmd` 的距离门 ⇒ `click_main_far` 红；
2. 删 `handle_click_npc_cmd` 的 `normNpc.Click` 派发 ⇒ `click_main_ok` 红；
3. （可选）删 Death 门 / 限频门 ⇒ 对应用例红。
