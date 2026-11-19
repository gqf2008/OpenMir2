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
- 状态面：`LastNpc`/`Script`/`ScriptCurrLable` **已建模**（`ShopSession.last_npc`/`script_index`/`script_curr_lable`；引擎 `goto_label` 写入）。**订正**：先前"未建模"的判断是错的（字段早已存在，缺的是断言语 fallback 用例），本批已补。

**QA 42 条自检（本样板）**
- R2：`parity-gap-npc.json` 的 `denominator`/`entries` 未被改动（本批不碰）。
- T2：本样板没有文本牙；行为牙见 §11（实现变异必红）。
- L3：本样板的状态只读**会话/实体字段**（`click_npc_time`、`life_cycle.death`、`transform`），不从回包反推。
- D1/D2：golden 每 case 带 `drain_passes`/`drain_hit_cap`；比较器断言 `drain_hit_cap == false`（true 即红）。
- Z1/Z2/Z3：golden 缺文件、零 cases、未知 case 名、`drain_hit_cap=true` ⇒ panic（防空转）。

### 10.1 状态面（已断言，不再 CONDITIONAL）

**订正（2026-11-20）**：早先写"`LastNpc`/`ScriptCurrLable` 未建模"是**错的**——`ShopSession` 已有
`last_npc`/`script_index`/`script_curr_lable`/`script_go_back_lable`，引擎 `goto_label`
（`crates/script/src/engine/mod.rs`）在 `@main` 命中与 fallback 两处都写 `set_last_npc`/`set_script_index`。
缺的只是比较器断言与 fallback 用例，现已补：

| 升 PASS 证明 | 用例锚 | 断言 |
| --- | --- | --- |
| 点击后 `LastNpc == 被点 NPC id` | `click_main_ok` / `click_no_main_fallback` | 读 `shop_sessions["hero"].last_npc`，与 golden `last_npc_is_npc` 逐例对齐 |
| 点击后 `ScriptCurrLable` 已清空 | 同上 | 读 `script_curr_lable`，与 golden `script_curr_lable` 对齐 |
| 无 `@main` 时 fallback 选同一脚本 | `click_no_main_fallback`（`[@greet]` 脚本） | 说词 0 条，但 `last_npc` 已设、`script_index` 有值（golden `script_selected=true`） |

结论：**NP-01 升 PASS（basis=semantic）**；状态面不再挂 CONDITIONAL。
### 10.2 QA 42 条四项自检（NP-01 样板）

| 项 | 自检 | 证据/命令 |
| --- | --- | --- |
| **R2** 分母不许动 | `git diff --stat master...HEAD -- docs/issues/parity-gap-npc.json` ⇒ 空输出 | 本批未触碰该文件（自检命令原文见交付说明） |
| **T2** 文本牙不算行为牙 | 本样板**没有**文本牙；牙 A 是实现变异（距离门 15→155 ⇒ `click_main_far` 红） | `E:/UserFleet/qa/npc-parity/np01-mutation-A-distance.txt` |
| **L3** 状态不许从回包反推 | 状态读取全部来自会话/实体：`sessions["hero"].click_npc_time`、`world.entities.get_mut(...).life_cycle.death`、`transform_mut(...).x/y`；帧只用来比帧 | 比较器源码（`lib.rs` 的 `np01_click_main_matches_oracle_golden`） |
| **D2** 命中上限必须红 | golden 每 case 带 `drain_passes`/`drain_hit_cap`（4/4 例）；比较器断言 `drain_hit_cap == false`，true ⇒ panic | `grep -c drain_hit_cap` 于 golden 与 `lib.rs`（原文见交付说明） |
| **状态牙 C** | 删引擎 `@main` 分支的 `set_last_npc` ⇒ `click_main_ok` 红在 `LastNpc 归属` | `E:/UserFleet/qa/npc-parity/np01-mutation-C-last-npc.txt`（FAILED 1；恢复 sha 相等；复绿 rc=0，`np01-test-green-final.txt`） |

### 10.3 CONDITIONAL 升 PASS 记录（2026-11-20 已满足）

opc-07 定的三条前置已全部满足（见 §10.1 三证明）；NP-01 签法改为 **PASS（basis=semantic）**。
批量硬规则保留：NP-04 及任何依赖 `LastNpc`/`ScriptCurrLable` 的用例仍必须显式断言，不得省略。

### 10.4 撤回记录（opc-07 要求：撤回什么、依据是什么）

- **撤回的结论**：`LastNpc`/`ScriptCurrLable` 在 Rust 未建模（2026-11-20 早先版本 §10/§10.1 的写法）。
- **依据（为什么撤回）**：
  1. 状态载体不是 `SessionGround` 而是 `ShopSession`：`mir2-rs/bins/game-svr/src/lib.rs` 的 `ShopSession`
     已有 `script_lable`、`script_curr_lable`、`script_go_back_lable`、`script_index: Option<usize>`、`last_npc: i32`（结构体定义约第 20508-20528 行）；
  2. 引擎写入点：`crates/script/src/engine/mod.rs` 的 `goto_label`（约 979-1015）在 `@main` 命中与 fallback
     两处都调 `set_last_npc`/`set_script_index`；
  3. 会话同步：`run_merchant_goto` 在约 9373-9374 与 9427-9428 把 `player.last_npc/script_index` 与会话双向同步。
- **替代结论**：状态已建模，缺的是**断言**与 **fallback 用例**；已补（§10.1 三证明）并把 NP-01 升 PASS。
- **教训**：旧结论来自"只 grep `SessionGround` 字段"的窄搜索，漏了同文件里承载脚本态的另一 struct。
  以后判"是否建模"必须沿**状态载体**（会话/实体/引擎三处）全量定位，且给出写入点。

### 10.5 NP-03 用例来源（真实脚本，禁造）

按 opc-07：商品/修理/仓库菜单一律用**部署件里真实存在的商家脚本**，不自行编造。

- **脚本文件**：`E:/MirServer/M2GameSvr/Envir/Market_Def/封魔谷/恶魔修理师-d2008.txt`（冻结语料）
- **语料指纹（必记）**：`sha256 = 14a3fd4d7c816bc31c0fce689a451bd15b2159736ea92c68e6b681c93fed7210`
  （2026-11-20 实测）。**规则**：每条 NP-03 golden 的 `fixture` 元数据必须带
  `{source_path, sha256}`；语料一变（hash 不同）⇒ 该证据失效，必须重录，不得沿用旧 golden。
- **商人在册行**：`MerChant.txt` → `封魔谷/恶魔修理师 d2008 31 24 封魔修理师 0 15 0`
- **可用标签（真实）**：`[@main]`（第 18 行）、`[@cangku]`(28)、`[@storage]`(36)、`[@getback]`(39)、
  `[@Mbind]`(43)、`[@changeGold]`(54)、`[@changeMoney]`(65)、`[@z_bind]`(78)、`[@zum_bind1..4]`(85/94/112/121/139/148/166/175)、
  `[@buy]`(194)、`[@sell]`(198)、`[goods]`(203)。
- **用例映射（NP-03 = 菜单/面板打开）**：
  | 动作 | 标签 | 期望帧（若对应 flag 开） |
  | --- | --- | --- |
  | 点 NPC | `@main` | `SM_MERCHANTSAY(643)` 文本 |
  | 选购买 | `@buy` | `RM_SENDGOODSLIST(10128)→SM 645` |
  | 选出售 | `@sell` | `RM_SENDUSERSELL(10129)→SM 646` |
  | 选仓库 | `@storage` / `@cangku` | `RM_USERSTORAGEITEM(10146)→SM 700` |
  | 选取回 | `@getback` | `RM_USERGETBACKITEM(10147)→SM 700/列表` |
  | 货币兑换 | `@changeGold` | M4 已有双跑（`商店-货币兑换`） |
- **flag 来源**：`is_buy`/`is_sell`/`is_storage`/`is_repair` 由 **加载器**从脚本 `[goods]`/`%` 指令解析
  （`crates/envir/src/merchant.rs` 的 `merchant_flags`），**不得在用例里手设**；用例只断言"开/未开"两种真实形态。

### 10.6 golden 重录纪律（opc-07，2026-11-20；适用于以后任何一次重录）

**重录不是免检操作——它把旧证据换成了新对象。** 每次重录必须：

1. **重录前/后各存一份 JSON**（仓外 `E:/UserFleet/qa/npc-parity/`），并给 `git diff` 原文；
2. 用 `mir2-rs/tests/parity/npc/golden_recheck.py before.json after.json` 证明差异**只落在新增元数据**。
   **剥离键白名单写死为三个**：`script_name`、`script_sha256`、`fixture`。
   **新增任何其它字段一律按行为变化处理**（工具不剥离，会报 `BEHAVIOR_CHANGED`）；要扩白名单必须改工具并报 opc-07。
   工具剥离这三键后整树逐字节比，行为字段有任何变化即 `BEHAVIOR_CHANGED`（rc=1）；
3. 行为字段一旦变化 ⇒ **原 PASS 作废**，按新 golden 重判并在报告说明原因；
4. 报告写明"PASS 对应的 golden 版本 = sha256 + 提交"。

**NP-01 的 PASS 对应 golden 版本（重录后）**：`mir2-rs/tests/parity/golden/npc/np01_click_main.json`
sha256 = `42fd29302505b33188f5818c576a1fd7a88ac864d3c61ea1bd3b0d0de8fc2bc4`（提交见本批）。

- **旧版本**：sha256 = `41e87d9b60c6e827833b2e9e755ea7ef0ef3f54aa9a4600b5a0be79b003c5fce`（提交 `4878fdbfe`）。
- **重录判定（行为字段确有变化 ⇒ 旧 PASS 作废并重判）**：重录暴露 `ctx.click_npc_time` 是墙钟值（每次运行不同，
  L5 必须排除）。修正为确定性布尔 `ctx.rate_limited`，根级加 `excluded_wallclock_fields: ["ctx.click_npc_time"]`。
  - before/after 对比：`E:/UserFleet/qa/npc-parity/rerecord/np01-before-vs-after.txt`
    （`BEHAVIOR_CHANGED`，差异**只有** `click_npc_time` 删除 + `rate_limited` 新增 + `excluded_wallclock_fields` 新增）；
  - 确定性证明（同探针连录两次）：`E:/UserFleet/qa/npc-parity/rerecord/determinism-two-runs.txt`
    ⇒ `BEHAVIOR_IDENTICAL`（rc=0），两次文件 sha256 相同（`42fd2930…2bc4`）；
  - **重判**：新 golden 上跑 `np01_click_main_matches_oracle_golden` ⇒ `rc=0`，`1 passed / 0 failed`
    （原文 `E:/UserFleet/qa/npc-parity/rerecord/np01-rejudge-green.txt`）。
- **PASS 重出结论**：NP-01 PASS（basis=semantic）对应 golden sha `42fd2930…2bc4`；生效前提即上述三份原文。
- **墙钟排除清单**：`ctx.click_npc_time`（仅此一项；golden 根级 `excluded_wallclock_fields` 记录）。

工具自测（可复跑，输出已落盘）：`python -I mir2-rs/tests/parity/npc/golden_recheck.py selftest`
⇒ 元数据新增=0 / 行为改动=1 / 零用例=2（ok）。
原文：`E:/UserFleet/qa/npc-parity/golden-recheck-selftest.txt`
（`selftest: metadata-only=0 behavior-change=1 empty=2 => OK`，rc=0）。

**重录后重出的 PASS 必须包含**：新 golden sha256 + 旧 golden sha256（本节当前值）+ 工具输出路径 +
`BEHAVIOR_IDENTICAL` 原文。缺任一项 ⇒ PASS 不成立。

## 12. NP-02（对话标签选择）交付记录（2026-11-20）

- **机制**：`CM_MERCHANTDLGSELECT(1011)` → `ClientMerchantDlgSelect`（`src/M2Server/Player/PlayObject.Operate.cs:114`）
  → `Merchant.UserSelect`（`src/GameSrv/Npc/Merchant.cs:789`）→ 标签账本 + `GotoLable` → 说词。
  **oracle 细节**：`NormNpc.UserSelect`（`src/M2Server/Npc/NormNpc.cs:1633`）只记标签、不 `GotoLable`；
  说词由 **Merchant 覆写**发出，且 `GotoLable(player, actorId, …)` 经 `SystemShare.ActorMgr.Get<INormNpc>` 解析 NPC
  ⇒ 探针必须把 NPC 注册进 `ActorMgr`（已做）。
- **golden**：`mir2-rs/tests/parity/golden/npc/np02_dialog_select.json`（4 例：`dlg_ask_ok` / `dlg_back_to_main` /
  `dlg_unknown_label` / `dlg_ask_back`），与 NP-01 golden **分文件**（避免每加一条就换掉旧证据对象）。
- **断言**：说词序列（`SM_MERCHANTSAY` 尾文本）+ `ScriptCurrLable` / `ScriptGoBackLable` / `ScriptLable`；
  单 `@back`（无先前 `@ask`）与 `@nope` 均为零说词（oracle 真实行为）。
- **对拍钉出的产品差异（已修）**：oracle `src/GameSrv/Npc/Merchant.cs:922-929` 在 `@back` 空栈时**写回**
  `ScriptGoBackLable = "@main"`；Rust 原来只在本地取默认值、不落状态。已改为先写回再跳转。
- **牙 D**：把 `@back` 二次跳转分支短路（`if false && eq_label("@back")`）⇒ `dlg_ask_back` 红在
  `@back 回 @main 说词`；原文 `E:/UserFleet/qa/npc-parity/np02-mutation-D-back-jump.txt`，恢复 sha 相等、复绿
  `E:/UserFleet/qa/npc-parity/np02-green-final.txt`。
- **NP-01 未受影响的证明**：拆分后重生成 `np01_click_main.json`，`golden_recheck.py` 对已验收版本
  （`E:/UserFleet/qa/npc-parity/rerecord/np01-after.json`，sha `42fd2930…2bc4`）判 `BEHAVIOR_IDENTICAL`（rc=0）。

### 12.0 产品修复单列（NP-02，动过产品代码 ⇒ 合并门禁需全量窗口）

| 项 | 内容 |
| --- | --- |
| 修复文件/行 | `mir2-rs/bins/game-svr/src/lib.rs` 的 `CM_MERCHANTDLGSELECT` `@back` 分支（`eq_label(label, "@back")` 内） |
| 修复内容 | 空栈时先**写回** `session.script_go_back_lable = "@main"`，再按该值跳转（原来只在本地取默认值，不落状态） |
| oracle 依据（符号锚） | `src/GameSrv/Npc/Merchant.cs:922-929` 的 `@back` 支：`if (string.IsNullOrEmpty(playObject.ScriptGoBackLable)) playObject.ScriptGoBackLable = "@main";` 然后 `GotoLable(...)` |
| 牙 D | 短路 `@back` 二次跳转（`if false && eq_label("@back")`）⇒ `dlg_ask_back` 红在"@back 回 @main 说词"；恢复 sha 相等、复绿（`E:/UserFleet/qa/npc-parity/np02-mutation-D-back-jump.txt` / `np02-green-final.txt`） |
| 合并门禁 | 本线**动过产品代码** ⇒ 合并前需单车道全量窗口（opc-07 排期） |

**变异纪律（opc-07 2026-11-20 加硬）**：变异前除"先提交"外，还要**给目标文件拍 `sha256` 快照**；
恢复后不只核 `git diff`，还要核**快照相等**——即使有未提交改动也不会被静默带走。

### 12.1 oracle `#ACT` 只执行第一条已注册命令（opc-07，2026-11-20；批量硬规则）

`ScriptEngine.GotoLableQuestActionProcess`（`src/Modules/ScriptEngine/ScriptEngine.cs:884-893`）在 `#ACT` 列表里
**命中第一条已注册命令就 `Execute; return;`** ⇒ 同段 `#ACT` 里后续已注册命令**不执行**。

- 批量夹具一律按"第一条生效"建期望；**不得**为让两条都跑而拆脚本（等于帮 oracle 改行为）。
- **NP-04 将加一条显式"第二条未执行"用例**（同一 `#ACT` 两条 `SET`/`SETOPEN`，断言只有第一条落状态），并引用本符号锚。

### 12.2 过程事故与纪律（2026-11-20，须留痕）

- **事故**：做 NP-02 牙 D 时，变异前未提交，恢复用 `git checkout -- lib.rs` ⇒ **把未提交的 NP-02 修复与断言一并回滚**
  （C# 探针/golden 未受影响）。已按记录重放全部改动、复绿，并以 `245c0fcf0` 提交。
- **纪律**：**变异前必须先提交**；恢复后除 sha 相等等，还要核对 `git diff` 与预期一致；共享/未提交状态禁止 `checkout --` 恢复。
- 已按此纪律执行后续牙；本条随卡保留，供复核。

## 11. 行为牙（NP-01 样板）

在**本 worktree** 内逐条变异、跑同一条测试，期望红，恢复后 sha 相等并复绿。原文落**仓外**：
`E:/UserFleet/qa/npc-parity/`（不手抄到卡面）。

| # | 变异 | 期望红在 | 证据 |
| --- | --- | --- | --- |
| A | `handle_click_npc_cmd` 距离门 `<=15` → `<=155` | `click_main_far` | `E:/UserFleet/qa/npc-parity/np01-mutation-A-distance.txt`（FAILED 1；恢复 sha 相等） |
| B | `@main` 派发（`run_merchant_goto` 的 label）改成不存在的 label | `click_main_ok` 即时帧空 vs `[643]` | `E:/UserFleet/qa/npc-parity/np01-mutation-B-main-dispatch.txt`（FAILED 1；恢复 sha 相等；复绿 rc=0，green 原文同目录） |
| C | 删 `goto_label` 的 `@main` 分支 `set_last_npc` | `click_main_ok` 的 `LastNpc 归属` | `E:/UserFleet/qa/npc-parity/np01-mutation-C-last-npc.txt`（FAILED 1；恢复 sha 相等；复绿 rc=0） |

> 变异 B 与 A 同法：`git checkout -- mir2-rs/bins/game-svr/src/lib.rs` 恢复后断言 sha 相等并复绿；
> 构建按总监窗口约束，窗口内不新起构建。
## 13. 子批①（NP-03/04/05）设计与裁定记录（2026-11-20）

### 13.1 裁定

- **NP-03**：等 `ca` 的 NPC 装载链改动合入后，从新 master 起；Rust 装置改用**商家装载链**（`crates/envir` 的 `load_merchants_loaded` + 真 `MerChant.txt` 行），不再用 `T137Rig` 固定 NPC。flag-开用真实语料；**flag-关无真实数据（全语料 0 条 `[@buy]` 标签而 flag 未开）⇒ 不造脚本，由既有单测覆盖并在卡面注明**；不改 `MerChant` 行凑负例。
- **NP-05**：选"**synthetic 夹具**"方案，绑定四条：入仓夹具 + sha256；两侧同一份脚本；`notes` 写明冻结语料 **0 使用**；报告注明"引擎门等价证据，非线上用例覆盖"。
- **NP-04**：不等产品改动，照常做。

### 13.2 NP-03 用例来源（真实，禁造）

- 脚本：`E:/MirServer/M2GameSvr/Envir/Market_Def/封魔谷/恶魔修理师-d2008.txt`
  首行 flag 指令 `(@buy @sell @repair @s_repair @storage @getback @getnextpage @getPreviouspage)`，`%100` 价格率；
  sha256 = `14a3fd4d7c816bc31c0fce689a451bd15b2159736ea92c68e6b681c93fed7210`。
- 在册行：`MerChant.txt` → `封魔谷/恶魔修理师 d2008 31 24 封魔修理师 0 15 0`。
- 用例（flag-开）：点 NPC → `@main` 说词；选 `@buy` → `RM_SENDGOODSLIST(10128)→SM 645`；
  选 `@sell` → `SM 646`；选 `@storage` → `SM 700`。
- **flag-关**：语料 0 条，不做 golden；由既有单测覆盖（卡面注明）。

### 13.3 NP-04 用例（真实脚本 + 机制）

1. **两 NPC 切换**（`click_switch`）：点 NPC-A → 排空 → 点 NPC-B ⇒ 说词 B；`LastNpc == B`、
   `Script` 切到 B 的脚本（状态直读 `ShopSession`）。
2. **`#ACT` 只执行第一条已注册命令**（真实脚本）：`E:/MirServer/M2GameSvr/Envir/Market_Def/任务NPC/打铁-0.txt`
   （sha256 见下）`[@main]` 首个 `#IF`（`LARGE M6 9` + `CHECKBAGSIZE 1`）命中时：
   - oracle（`src/Modules/ScriptEngine/ScriptEngine.cs:884-893`）只执行 `MOV M6 0`；
   - `set [106] 0`、`MOVR D6 5`、`goto @jiasx` **均不执行**；
   - 断言：说词="你的命运之刃已经完成…"、`M6==0`、`flag 106` **保持 1**（若全执行会被清 0）。
   - 前置：M6=10、flag 106=1（两侧同一初始态）。
   - 打铁-0 sha256：`356b53dfa49aad7f4fe0d20b0b9a4f777d33be0e88271da7265f6298a5d8b86e`（实测 2026-11-20）。

### 13.4 NP-05 用例（synthetic 夹具，四条已绑）

- 夹具：`mir2-rs/tests/parity/golden/npc/fixtures/np05_extjmp_synthetic.txt`
  sha256 = `c8302373725443ac56d516ef18a7497f0c57bef0d563af5f7f8a50edf18b7575`
- 内容：`[@main]`（**不**链接 `@sub`）/ `[@sub] TRUE` / `[@plain]`（未标 TRUE）。
- 用例：`extjmp_allowed`（选 `@sub` ⇒ 说词"你已进入子页。"）/ `extjmp_blocked`（选 `@plain` ⇒ 零说词）。
- **冻结语料 `Market_Def` + `MapQuest_def` 对 `] TRUE` 的用量为 0**（实测 `grep -rn "] TRUE"` 零命中）⇒
  本条是**引擎门等价**证据，不是线上用例覆盖。

### 13.5 进度

C# 探针（`ScriptSourcePath`/fixture 元数据、`click_switch`、`act_first` 状态）与 Rust 断言骨架已写入工作树，
**未编译未提交**（等 `ca` 合入 + opc-07 放行窗口）；NP-03 的 Rust 装置待新 master。

