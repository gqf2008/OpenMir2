# openmir2-items-parity

> 状态：**判定面草案（2026-11-20，分支 `card/items-parity`，base master=`9c84ea226`）**。
> 本文件是「物品线」的方法论与**判定面清单 / 账本格式**，照抄魔法线
> `docs/issues/openmir2-magic-parity.md` 的方法，不另发明。第一份交付，待 `opc-07` 过审后再铺量。
>
> 分母：`docs/issues/parity-gap-items.json` 的 `denominator.count = 19`（**不许改**）。
> 现状（机器口径，`E:/OPC/tools/coverage-report.ps1`）：**物品 7/19**（verified 7 / implemented_unverified 1 / missing 11）。

## 0. 本文件对 42 草案的改动一览（先看这一节）

| # | 42 草案 | 本文件 | 依据 |
|---|---|---|---|
| G1 | 延时通知「按**优先级序**比」 | 改为**按 `(priority, ident, 规范化 params)` 排序后的序列**逐位比（比纯多重集严、比原始到达序松）；重复条目计数；报告同 priority 内多 ident | QA 清单 L2；`opc-07` 2026-11-20 定死；魔法线 §22b「RM 顺序是堆布局产物」 |
| G2 | 通知面列 `10052/10115/10309` | 补全为 `8102/10052/10115/10139/10309/11004`（现测并集） | 夹具 `eat_vectors.json` 实测；常量见 `useitem.rs` 顶部 |
| G3 | 通知字段 = 5 元组 | 补 `priority`（`TryDequeue` 出队键）与 `late`（`SendMessage.LateDelivery`）⇒ 七字段 | QA L2；`eat-probe` 现只记 5 字段 |
| G4 | 「抽空为止」仅写 `MaxDrainPasses=8` | 明确产物须带 **`DrainPasses` + `DrainPassesHitCap`**，`HitCap=true` ⇒ **红/显式 WARN**，禁静默 | QA D1/D2/D3；`eat-probe` 现为单趟无上限 `Drain` |
| G5 | RNG「Rust `MIR2_RNG_LOG`；C# 如需账本用 StartupHook 同名口径」 | 明确**哪些条目会抽**、C# 侧用与魔法线**同一**进程内 `LoggingRandom` 反射注入（零生产改动）、比较器核 总量/`seq` 连续/逐次/`site` 清单；0 抽条目记**空账本 + 正锚** | QA L6/Z4；魔法线 §2/§16b |
| G6 | 状态量列 HP/MP/Hunger/Weight/ItemList | 补全为 `wabil` **全字段** + `IncHealth/IncSpell/UserUnLockDurg` + `item_list` **对称差** + `item_number` + `added_items`；且**从实体状态直读** | QA L3/L4；`UseItemActorState`/`UseItemOutcome` 字段 |
| G7 | 即时帧只列 `635/636` | 补 `SM_ADDITEM(200)`（解包成功路径），比较面 = `added_items`（结构化）+ 严格序 | IT-18 成功路径缺口；`UseItemOutcome::added_items` 文档注释 |
| G8 | 防空转仅 `cases.len()>=12` | 补 Z1–Z6 专用负例（空/缺文件/零用例/全零/两层非零/三颗篡改） | QA Z1–Z6 |
| G9 | 分母 19 | **不动**；交付 diff 禁改 `denominator`/删 `entries` | QA R1/R2 |

## 1. 记录面 vs 判定面（归属写清，避免被读成「已有断言」）

| 面 | 产物 | 现状 |
|---|---|---|
| 记录面（C# 金标准） | `mir2-rs/tests/parity/m3/fixtures/eat_vectors.json`：由 `mir2-rs/tests/parity/m3/eat-probe` 引用 `src/M2Server` 真身、反射调**私有** `ClientUseItems`，`INetChannel` 替身截即时帧 + 反射读 `MsgQueue` 取出队通知 + 反射读 `ItemList/WAbil/…` | 已有 22 例；**缺 priority/late、缺 RNG、缺解包成功路径、Drain 无上限** |
| 判定面（Rust 比较器） | `mir2-rs/bins/game-svr/src/useitem.rs` 的 `mod tests::matches_oracle_eat_fixture`（回读同一份夹具，逐案把输入喂 `use_item` 再比） | 已比对 即时帧/通知/ItemList/WAbil/IncHealth/IncSpell/UserUnLockDurg/HungerStatus；**Z1–Z6 负例不足、RNG 与 ADDITEM 未接、无抽空口径** |

> **物品线与魔法线的差异**：魔法线 Rust 宿主有独立驱动产「真实台账」再双向比；物品线 Rust 侧是**纯函数
> `use_item`** 直接在同进程算，夹具即 oracle 记录。故「判定面」= 本仓测试**逐案回放并逐字段比**，
> 不存在第二份 Rust 台账。这不降低判据强度（仍两套实现、同输入、比输出），但**必须**防「夹具全零 /
> 用例恒空 / 只比一层」的自旋绿（见 §4）。
>
> **对外覆盖率口径（`opc-07` 2026-11-20，与魔法线同）**：**分支侧 `verified` 不计入对外覆盖率**——
> 统计以合并后的 master 为准；合并前对外数字仍是 `物品 7/19`，合并后才含 IT-18 的 8/19。
> 卡面/报告只引 `E:/OPC/tools/coverage-report.ps1` 的输出（该脚本读主检出 master）。

## 2. 判定面清单（物品）

| 面 | 物品线具体内容 | oracle 从哪读 | 比较方式 |
|---|---|---|---|
| **即时帧** | `SM_EAT_OK(635)` / `SM_EAT_FAIL(636)`；解包成功时前置 `SM_ADDITEM(200)`（每件一条） | `eat-probe` 的 `INetChannel` 捕获（`net.Frames`） | **严格序** + 逐字段；`SM_ADDITEM` 另比结构化 `added_items` |
| **延时通知** | `RM_MYSTATUS(8102)`、`RM_HEALTHSPELLCHANGED(10052)`、`RM_WEIGHTCHANGED(10115)`、`RM_CHARSTATUSCHANGED(10139)`、`RM_MENU_OK(10309)`、`RM_SYSMESSAGE(11004)` | `eat-probe` 反射读 `MsgQueue`（`TryDequeue` 取 出队序 + 优先级键） | **七字段多重集**（不见顺序；缺/多逐条报） |
| **状态量（一级）** | `WAbil` 全字段（HP/MP/MaxHP/MaxMP/Weight/Exp…）、`HungerStatus`、`IncHealth`、`IncSpell`、`UserUnLockDurg`、`ItemList` 终态、`item_number`（新 MakeIndex 计数）、解包 `added_items` | `eat-probe` 反射直读 `PlayObject` 字段（**非帧参数**） | 逐字段相等；`ItemList` 用**对称差** |
| **RNG** | 仅抽数的条目（回城/随机传送/地牢逃脱/祝福油/彩票）：逐次 `result` + `site` + `seq` | C# 侧 `LoggingRandom`（同魔法线 §2）；Rust 侧 `MIR2_RNG_LOG`（`crates/shared/src/rng_trace.rs`） | 总量==夹具长度 → `seq` 连续 → 逐次相等 → 每行 `site` ∈ 允许清单 |

**比较原则**：即时帧**严格序**；通知**优先级多重集**；状态量**一级断言**（不只比帧、不从帧反推）。

## 3. 账本格式（夹具 JSON，记录面 schema）

顶层沿用现结构：`generator / oracle_assembly(_sha256) / oracle_source(_sha256) / templates / cases[]`。
每个 `case` **新增/明确**以下字段（缺字段 ⇒ 比较器按「防空转」红，不按缺省放行）：

```jsonc
{
  "name": "eat_ok_...",
  "ctx": { "can_use_item":.., "dead":.., "actor_id":.., "no_drug":..,
           "function_npc_present":.., "red_msg_fcolor":.., "red_msg_bcolor":.. },
  "request": { "make_index":.., "name":".." },
  "wabil_before": { /* 22 字段，同 Ability */ },
  "inc_health_before":.., "inc_spell_before":.., "user_unlock_durg_before":..,
  "hunger_status_before":..,
  "item_list_before": [ /* ItemDto[] */ ],
  "expect": {
    "immediate_frames": [ /* 严格序：SM_ADDITEM(200)… 再 635|636 */
      { "ident":200, "recog":0, "param":.., "tag":.., "series":.., "tail":"<EDCode 尾串>" },
      { "ident":635, "recog":0, "param":0,  "tag":0,  "series":0,  "tail":"" }
    ],
    "added_items": [ /* 新增：解包产物结构化（生成序），与 SM_ADDITEM 一一对应 */ ],
    "notices": [ /* 七字段；按 (priority,ident,规范化 params) 排序后逐位比；重复条目计数 */
      { "ident":.., "w_param":.., "n_param1":.., "n_param2":.., "n_param3":..,
        "priority":.., "late":0|1 }
    ],
    "wabil": { /* 22 字段终态 */ },
    "inc_health":.., "inc_spell":.., "user_unlock_durg":.., "hunger_status":..,
    "item_list": [ /* ItemDto[] 终态 */ ],
    "item_number": ..,             // C# M2Share.GetItemNumber() 前后差
    "rng": [ /* 逐次 result；0 抽条目为 [] */ ],
    "rng_sites": [ /* 每次抽取的 site（file!()，落盘前统一 '/'） */ ],
    "rng_seqs": [ /* trace 全局单调 seq；比较器核连续 */ ],
    "drain": { "passes": .., "max_passes": 8, "hit_cap": false }
  }
}
```

- **`priority` 来源**：`ActorEntity.MsgQueue` 的入队键（`BaseObject.Message.cs` 的 `Enqueue(…, (byte)Priority)`；
  普通 RM 用 `(byte)wIdent`，`SendSelfDelayMsg` 用 `(byte)MessagePriority.Lowest`）。
  取法：`PriorityQueue.TryDequeue(out msg, out prio)`——现 `eat-probe` 用 `Dequeue()`（丢键），须改。
- **`late` 来源**：`SendMessage.LateDelivery`（`src/Modules/SystemModule/Data/SendMessage.cs`），按 1/0 比。
- **墙钟排除清单（L5）**：`DeliveryTime`、`ActorId`、`Buff`（投递层）与探针进程墙钟**不入比较面**。
  物品线无「按 tick 到期」的延时魔法，本项天然为空；一旦引入，须在此写明。

## 4. 判定规则（逐条对应 QA `module-harness-checklist-20261120.md`）

### 4.1 账本双面（L）
- **L1 即时帧严格序**：`immediate_frames` 按 `ident/param/tag/series/tail` **逐帧按序**相等；顺序错即红。
  `SM_ADDITEM(200)` 的尾串（`EDCode.EncodeBuffer(ClientItem)`）另由结构化 `added_items` 逐字段钉住。
- **L2 通知排序序列**（`opc-07` 2026-11-20 定死）：`notices` 按 `(priority, ident, 规范化 params)`
  排序（同键组再按 `late` 定序，使全序唯一）后**逐位逐字段**比；**重复条目必须计数**（两条全等 ⇒ 必须两条）；
  报告须**额外汇总「同 priority 内是否存在多个不同 ident」**（供裁定是否需要单列同优先级内序）。
  为什么不是纯多重集：多重集对顺序完全不敏感，priority 互换仍相等，而优先级决定客户端处理先后（玩家可见）；
  为什么不是原始到达序：`PriorityQueue` 同优先级内次序在 oracle 侧不稳定，比它等于测抖动。
- **L3 状态量一级**：全部从 `UseItemActorState`/`ItemList`/`item_number` 直读，**禁止**从 `SM_*` 参数反推。
- **L4 增删对称差为空**：`ItemList` 终态比较两侧「golden 有 rust 无」与「rust 有 golden 无」都为 0。
- **L5 墙钟字段排除**：见 §3 排除清单，卡/报告须写明。
- **L6 RNG 账本**：总量==夹具长度（不截断）、`seq` 连续、逐次 `result` 相等、每行 `site` ∈
  **代码里的允许清单**（清单外红）。物品线的 `site` 允许清单**逐条写理由**，随首条抽数条目落定；
  `file!()` 的 `\` 落盘前统一 `/`（沿用魔法线口径 C）。
  **0 抽条目**：`rng=[]` 合法，但**必须配一个非零正锚**（HP 差/帧数/ItemList 差），否则 Z4 红；
  且**正锚要能咬**：给一条负例——人为让该路径抽 1 次 ⇒ 该条必须红（证明「0 抽」是被核过的，不是没核）。

### 4.2 防空转（Z）
- **Z1 空输入必红**：空 `cases` / 空 `expect` / 空 `notices`（对**应有多条**的条目）/ 空夹具 ⇒ 比较器非 0 且点名。
- **Z2 缺夹具必红**：`fixture()` 读不到文件 ⇒ panic 点名路径（rc≠0），不得 PASS。
- **Z3 零用例必红**：`cases` 为空或 `< 已知例数` ⇒ 红；`cargo test` 断言 `passed+failed >= 1`。
- **Z4 全零必红（定义钉死，`opc-07` 2026-11-20）**：
  - **适用** = 该例 golden 有**非零锚**（帧 `ident≠0` / 通知 / 状态差 / 解包件 / RNG 任一）⇒ **Rust 必须有非零锚**；
  - **不适用** = golden **全零**（合法失败例）⇒ 该例不解析 Z4；
  - **计数式**：`适用数 + 不适用数 == 夹具例数`；**不适用以 case 名列出**（当前为空 ⇒ 写「无」）；
  - 当前：适用 **23** + 不适用 **0** = 23；不适用名单：**无**。
- **Z5 两层非零（定义钉死，`opc-07` 2026-11-20）**：
  - **两层** = `SM 帧>0` 且（`RM 通知>0` 或 `状态有变`）；
  - **例外（只需一层）** = **纯失败例**：只发 `SM_EAT_FAIL`、无 RM/状态变化是合法行为，强判两层会误红；
  - **计数式**：`两层数 + 豁免数 == 总例数`；**豁免以 case 名列出**；
  - 当前：两层 **17** + 豁免 **6** = 23；豁免名单：`eat_fail_not_found`、`eat_fail_stdmode_other`、
    `eat_fail_template_missing`、`eat_fail_dead`、`eat_fail_stdmode31_no_functionnpc`、`eat_fail_stdmode31_unbind_bag_full`；
  - 上述两个计数与两个名单在 `useitem.rs` 由 **`assert_eq!`** 固化（**不是打印字符串**）：夹具变更导致名单漂移 ⇒ 红，
    须与用例**同一次提交**更新并写进提交信息。
- **Z6 三颗篡改牙**：至少三条内存篡改（改字段/改序/删条目）各红，还原后绿（见下 §6）。

### 4.3 抽空为止（D）
- **D1 上限可见**：每 case 落 `drain.passes` / `drain.max_passes=8` / `drain.hit_cap`，另有 stderr `DRAIN_PASSES` 行。
- **D2 命中上限必红**：`hit_cap=true` ⇒ 比较器**必须红**（`opc-07` 2026-11-20 定死，**不接受 WARN**——
  物品线没有「丢帧继续」的正当理由，与任务线同口径）；静默通过 = FAIL。
- **D3 禁固定趟数冒充**：每 case 记**实际**趟数，不得保留「固定 N 趟」旧口径。

> **物品线的诚实说明（D 面语义，`opc-07` 2026-11-20 定）**：
> - **`drain_ok` 是「金标准完整性门」**：它核的是「录制时抽空为止、未触顶 ⇒ 这份 golden 完整」，
>   **不是**「Rust 产品排空行为」。Rust 宿主直接应用 `immediate_frames`/`notices`，**无排空趟、不产生 `hit_cap`**
>   （`hit_cap` 仅由 C# `eat-probe` 录制面产生）——后人勿以为 Rust 有排空逻辑。
> - `eat-probe` 只**排空队列、不跑 `Operate`**（通知是「已入队未投递」快照）；物品线 1 趟即空、
>   `hit_cap` 恒 false。**没有投递重入链路就照实写「1 趟排空、hit_cap=false」**，不许伪造成多趟。
> - D 面证据 = **D3a**（C# 探针 `DrainPassLoop` 自测：假队列每趟重满跑到 `MaxDrainPasses` ⇒ `hitCap=true`）
>   + **D2**（Rust gate `drain_ok` 对 `hit_cap=true` 必红）；**不新增 Rust 排空**（属产品/设计变更，另立卡）。

### 4.4 牙（T）
- **T1**：每条 `verified` 至少一颗**行为牙**（改实现关键判断 ⇒ 对应测试必红）。
- **T2**：`grep`/正则/AST 片段匹配只作**接线提示**，卡面标注「接线提示，非行为证据」。
- **T3**：多分支逐支变异（if/else、fallback 每支各删一次）。
- **T4**：每次还原后源文件 sha256 == 改动前、`git status` 干净、复跑绿。
- **T5**：变异按用例所在包/过滤跑，断言 `passed+failed >= 1`（0 tests 的「不咬」是假象）。
- **T6**：写/删文件的牙只对 tempdir/假根（**绝不**指向 `E:/MirServer` 生产根）。

### 4.5 身份链 / 证据 / 签署（I/E/S，交付件门禁，非比较器本身）
- **I1–I5**：被引用的二进制须 `--version` 自报 + 侧车戳同源 + 冻结副本 + 构建记录；行为等价不要求字节复现。
- **E1–E4**：卡面只引机器产物路径（禁手抄数字）；高 churn 文件只用**符号锚**；每条结论给 测试名+命令+rc+原文；引用路径须实存。
- **S1–S4**：三态签署；合并 head 重跑；替换生产件前备份+回滚步骤；不私改标准。

## 5. 条目推进顺序与判据落点

1. **IT-18（唯一 `implemented_unverified`）先做成「单条 verified 样板」**：
   - 记录面补**解包成功**用例（6 件、`SM_ADDITEM(200)`×6 + `SM_EAT_OK`、`ItemList` +6、`WAbil.Weight` 更新）；
   - 判定面接 `added_items` + 严格序；
   - 至少一颗**行为牙**（改 `UnbindList` 查名/改容量门/改 6 件数 各能咬红，给原文与还原后复绿）。
2. 样板过审后，按 `missing` 1 条一条推：IT-07/08/17（相对独立）→ IT-12/14/15（武器副作用）→
   IT-13/16（城堡/彩票）→ IT-09/10/11（地图/会话态，成本最高）。
3. 每条转 `verified` 前须满足：夹具由 oracle 产出 + 判定面全绿 + ≥1 行为牙 + 证据四件套 + 分母不动。

## 6. 三颗咬合证明（可重放构造，QA 独立复核用）

| 用例 | 改哪份副本 | 怎么改 | 期望红在哪条断言 |
|---|---|---|---|
| `tooth_notice_priority_changed_red` | 内存里的 `case.expect.notices` 克隆 | 翻转一条 `priority`（或 `late`） | L2 排序序列不等 |
| `tooth_notice_duplicate_count_red` | 同上 | 复制一条全等通知（两处都加）/或删一条重复项 | L2 重复计数（多重集语义） |
| `tooth_notice_same_priority_ident_swap_red` | 同上（构造同 priority、不同 ident 的两条） | 只互换两条的 `ident`（priority 不变） | L2 按 `(priority,ident,params)` 排序后逐位不等 |
| `tooth_immediate_order_swap_red` | 内存里的 `expect.immediate_frames` 克隆（对有多帧的条目） | 交换两条 `SM_ADDITEM` / 把 `SM_EAT_OK` 提到前面 | L1 即时帧严格序 |
| `tooth_state_field_changed_red` | 内存里的 `expect.wabil`/`item_list` 克隆 | 改一个 HP 或删一件物品 | L3/L4 状态量一级 / 对称差 |

（每条「篡改⇒红、还原⇒绿」各给原文；与魔法线 §19c 同法，**不碰夹具文件本身**。）

## 7. 待办与不能标 verified 的门槛

- **IT-18**：成功路径夹具已产出（`eat_ok_stdmode31_unbind_success`）；但仍保持 `implemented_unverified`，
  直到 ①oracle 等价性（见 §8）②L2/D 判定面 ③Z1–Z6 ④clippy 全部落地并过审——之后才转 `verified`。
- **IT-07…17**：`missing`，均带 `deviation.id = T-17`（指向 `useitem.rs` 头部未做清单）；实现 + 对拍后才可改 `verified`。
- **RNG `site` 允许清单**：首条抽数条目落地时必须给出**代码表 + 逐条理由**，清单外红；不得「能过就加」。
- **分母 19 不动**：若认为分母错，先停手交证据给 `opc-07`。

## 8. oracle 等价性（2026-11-20，`opc-07` 复核后修正）

- **不用转写件冒充 oracle**：`eat-probe` 的物品表直接用**真身 `M2Server.Items.GameItemSystem`**
  （`SystemShare.ItemSystem = new GameItemSystem()`），把模板按 id 稠密铺进 `StdItemList`
  （位置 i = id i+1，空位放空名占位）。`GetStdItem` / `GetUpgradeStdItem` / `CopyToUserItemFromName`
  全部走 oracle 同一份代码——不存在"位置+1 vs 字典键""同名取首个""空名/NRE"的转写偏差。
  （此前用 `ProbeItemSystem` 转写 `CopyToUserItemFromName`，被 `opc-07` 判为"转写冒充 oracle"，已废弃。）
- **行尾归一（卡面记录）**：`eat_vectors.json` 原 blob 为 CRLF；按 `mir2-rs/.gitattributes` 的
  `*.json eol=lf` 归一为 LF。**语义改动仅**：+模板 `2017`、+用例 `eat_ok_stdmode31_unbind_success`、
  各例新增 `added_items`（旧例为 `[]`）、新增 `unbind` 表与 `drain` 字段；其余为行尾归一。

## 9. items harness 状态扩展（`opc-07` 2026-11-20 路线1）+ 交付表

> **显著事实（第 2 次「实现归类」修正）**：`use_item` 是**纯决策函数**——只产出 `immediate_frames`/`notices`/
> `item_list` 迁移/`added_items`，**不做位置搬运**。IT-09/10/11/13 的 `EatUseItems→BaseObjectMove`（+ 随机点 `Rng`）
> 是**宿主行为**，纯函数不覆盖，需宿主级 move 夹具对拍。（第 1 次修正：Rust 无 cap 排空实现，见 §4.3。）

一次扩展、逐条 verified 前的前置：把 oracle `PlayObject` 上散落、`UseItemActorState` 原未承载的状态集中为
`ItemsExtraState`，作为 `UseItemActorState.extra`（`useitem.rs`）。分批（slice）提交，每笔单独证明零回归；均**复用宿主已有原语
（不重写）**：`add_body_luck`、实体 `extra_abil` 组件 + `apply_extra_abil`、`sg.use_items`、
`mir2_formula::level_ability::recalc_level_abilitys`、`HasLevelUp`/`GetLevelExp`、`pvp_level`、`is_castle_member`、`smframe::send_add_magic`。

### 9.1 交付表（**分两组**：纯状态组 / move 组）—— QA 判 `verified` 覆盖范围的依据

**纯状态组（IT-07/08/12/14/15/17）——走现有 eat 夹具 + `ItemsExtraState`**

| 字段 | 是否 wire | 宿主来源 | 用它的条目 |
|---|---|---|---|
| `body_luck` / `body_luck_level` | **✓**（slice 1） | `sg.equip.body_luck(_level)`，`add_body_luck` | IT-08 |
| `extra_abil` / `_flag` / `_times`（`[_;7]`） | **✓**（slice 2） | 实体组件 `extra_abil(_flag/_times)` | IT-07 |
| `weapon: Option<BagItem>` | **✓**（slice 2） | `sg.use_items[SLOT_WEAPON=1]` | IT-12 / IT-14 / IT-15 |
| `magic_list` | **✗ 未 wire**（IT-17 接 `SavedHuman.record.data.magic`） | — | IT-17 |

**move 组（IT-09/10/11/13）——需宿主 move 夹具；eat 夹具/Context 不覆盖**

| 面 | 已覆盖 | 未覆盖（待宿主 move 夹具） | 条目 |
|---|---|---|---|
| `EatUseItems→BaseObjectMove` 位置搬运 + 随机点 `Rng` + 终态 `MapName/X/Y` | **✗ 全部未覆盖** | 全部 | IT-09 / IT-10 / IT-11 / IT-13 |

**未 wire 字段 / 未覆盖面**：在受影响用例里标「**未判 / 未覆盖**」，不得当「已相等 / 已覆盖」。

> **IT-09/10/11/13 当前 status = `missing`**（C# 有、Rust 无 ⇒ 未实现；**不是** `implemented_unverified`）。
> 在宿主 move 夹具落地前保持 `missing`，`notes` 记「需宿主 move 夹具（`EatUseItems→BaseObjectMove` + 随机点 +
> 终态 MapName/X/Y），`use_item` 纯函数不覆盖移动」。IT-17 同理当前 `missing`；实现但 `magic_list` 未 wire 时才是 `implemented_unverified`。

### 9.2 3a / 3b 结论（防后人误以为已接）

- **3a `magic_list`**：`handle_eat_cmd` 入参**无** `SavedHuman`（只有 `sessions: &mut HashMap<String,SessionGround>` + `world`）；
  magic 在 `SavedHuman.record.data.magic`（`magic_list_mut`，`lib.rs`），而 `handle_eat_cmd` 有 **9+ 调用点**且无 saves 直取路径。
  ⇒ 为单一（IT-17）用途穿 9+ 调用点「改动面与验证面不对等」⇒ **推迟到 IT-17** 接线；IT-17 在 `magic_list` wire 前保持
  `implemented_unverified`，notes 写「待 saved 源接线」。
- **3b map/session**：IT-09/10/11/13 是**宿主 move 行为**（`BaseObjectMove` + 随机点 `Rng`）；**纯函数 `use_item` 只返回帧/通知/状态、不搬运位置**。
  故 map/session 不能用「只读入 `UseItemContext`」覆盖——它需要**宿主级 move 夹具**（仿 magic `a5_spell_rig`）+ 观测终态 `MapName/X/Y`，
  `boNORANDOMMOVE`/`pk_point`/`home_map` 等只支撑「决策」分支。**move 组另立夹具，不并入本 Context 扩展。**

### 9.2b move 组判定面（宿主 move 夹具落地时执行）

三项缺一不可，**不许只断言「走了」**：
1. **终态 `MapName/X/Y`**：`BaseObjectMove` 后落点/地图逐字段对 oracle；
2. **`Rng` 逐次**：本图随机点 / Home 随机点每次抽取与 oracle 账本逐次相等（含 `site` 清单、`seq` 连续、不截断）；
3. **目标点分支**：`PvpLevel()`（=`(byte)(PkPoint/100)`）<2 → Home，否则红名村（`RedHome*`）；`boNORANDOMMOVE` 拒绝随机传送；城堡成员 → castle home。

### 9.3 `ExtraAbilTimes` 位宽

oracle 是 `int`（`GetTickCount()` i32，含回绕），宿主存 `[i64;7]`。判据**按 oracle `int` 算术（i32 回绕）算、再以 i64 存储比较**
（「宽存窄算」）——不因位数宽就判相等。

### 9.4 `UseItemActorState` 去 `Eq`

因新增 `f64`（`body_luck`）无法 `Eq`；保留 `PartialEq`（仍**逐全字段**比较，含 `extra`）。已 grep 确认无代码依赖 `Eq` 约束
（`Eq` 仅标记；`==`/`assert_eq!` 用 `PartialEq`）⇒ 不存在「原本靠 Eq 的相等判定变恒真」。

### 9.5 `Abil` vs `WAbil`（Exp 真值源；`opc-07` 2026-11-20 质询后钉死）

**oracle：同 `Ability` 类型、两个实例**
- `Abil` = 基础真值（`BaseObject.cs` `public Ability Abil`）；`WAbil` = 客户端/下发视图（`public Ability WAbil`），
  `AbilCopyToWAbil()` 里 `WAbil = Abil.Clone()`。
- `GetExp` 只写 `Abil.Exp/Level/MaxExp`；`RM_WINEXP→SM_WINEXP` 读 `Abil.Exp`；`WAbil.Exp` 仅在登录/重算块从 `Abil` 同步
  ⇒ **非升级 `GetExp` 后 `WAbil.Exp` 是陈旧值**。

**Rust 对应**
- `sg.equip.abil` = 基础 `Abil`（`RecalcInputs.abil = &sg.equip.abil`；宿主战斗版 GetExp 写 `sg.equip.abil.exp/level/max_exp`）。
- `sg.state.wabil` = `WAbil`（`recalc_session_ability` 输出；`UseItemActorState.wabil` 文档即「`PlayObject.WAbil`」）。

**对本轮的后果**：`e42c43fdd` 的 IT-08 曾 `state.wabil.exp +=`（WAbil，**错对象**）⇒ **返工**：item `GetExp` 必须落到
基础 `sg.equip.abil.exp` 并同步 WAbil/entity，`SM_WINEXP` 取基础 exp。返工方式（待 `opc-07` 定 A/B）：
A = `Outcome` 回 `exp_delta`，宿主落基础 + 同步（与宿主战斗版 `GetExp` 同源）；B = `UseItemActorState` 加 `abil: Ability`（基础）字段。

**A 已实现（`e42c43fdd` 返工）**：`use_item` 不再改 `WAbil.Exp`，回 `Outcome.exp_delta`；宿主 `handle_eat_cmd` 落**基础** `sg.equip.abil.exp`、
**不同步 `WAbil`**。待补（据此再加证）：①宿主级用例（吃经验书后 `base_exp` 变、`wabil.exp` 未变；触发一次重算后两者相等）；②`SM_WINEXP` 取基础 exp；
③oracle 夹具（Shape13 非升级）；④升级支。

### 9.6 宿主同步点与 oracle 触发点对应（IT-08 `WAbil.Exp = Abil.Exp`）

- **oracle 触发点**：登录 ctor（`AbilCopyToWAbil()` 后 `RecalcLevelAbilitys()`→`RecalcAbilitys()`，再显式 `WAbil.Exp = Abil.Exp`）、
  以及 `AbilCopyToWAbil()` 的全量克隆（`WAbil = Abil.Clone()`）。**吃物品的 `GetExp`（非升级支）不触发**这两条 ⇒ `WAbil.Exp` 陈旧。
- **宿主对应物**：`recalc_session_ability`（`RecalcAbilitys` 等价，**携带 exp**：`level_ability_base(..abil.clone())`→`..base`→`wabil.exp=abil.exp`）。
  生产调用事件（同一函数）：登录建会话、取/放装备、升级 downlink、`ChangeLevel`/脚本经验等。
- **测试 `it08_eat_exp_book_view_stale_then_real_sync_catches_up`**：吃一本/两本（均**不调** recalc）⇒ `WAbil.Exp` 持续陈旧（`base` 变、视图仍旧，**不是**"过一会儿自然相等"）；
  调**生产** `recalc_session_ability`（非自造函数）⇒ 视图追上。即**触发时机一致**：吃书不同步、登录/升级/换装才同步，与 oracle 同。

### 9.7 IT-08 升级支（`Exp>=MaxExp` 块）：宿主层实现与对拍

`GetExp` 的升级块需要 `Exps.conf` 的 `NeedExps` 表（`GetLevelExp`）、`Job`、`Gold` 等**纯 `use_item` 拿不到的宿主上下文**，
故按 §9 归类为**宿主层**行为，复用宿主既有原语，不塞进纯决策函数：

- 顺序（对齐 oracle `BaseObject.GetExp`）：`abil.exp -= abil.max_exp` → `abil.level+1`（<255）→
  `abil.max_exp = get_level_exp(world.need_exps, MAX_LEVEL, level)`（`RecalcLevelAbilitys` 等价，`level_ability_base`）→
  `recalc_session_ability`（`RecalcAbilitys` 等价，把 `WAbil` 从基础重算）→ `inc_health_spell(2000,2000)`（`IncHealthSpell`）→
  `add_body_luck(…, 100.0)`。
- **帧/通知线上序**（oracle 经 `Operate` 泵出；golden `pumped` 实测）：
  `SM_HEALTHSPELLCHANGED(53)`（`IncHealthSpell` 触发，本批最前）→ `SM_WINEXP(44)`（notice 排水）→
  `SM_LEVELUP(45)/SM_ABILITY(52)/SM_SUBABILITY(752)`（`RM_LEVELUP` 处理器，notice 之后）。
- **`WAbil == Abil`（升级后）是行为证据，非同义反复**：升级块内 `HasLevelUp→RecalcAbilitys` 让 `WAbil` 追上基础，
  与非升级支"吃书后 `WAbil.Exp` 持续陈旧"形成对照。**旧实现**（只改基础、不 recalc）会让 `WAbil.exp/level/max_exp` 停在
  升级前值 ⇒ `it08_levelup_host_parity` 的 `WAbil==Abil` 断言必红（§16 变异 A 证）。
- **`53` 只在升级支出现，非升级支没有**（非升级 `IncHealthSpell` 不被触发）。
- **`body_luck`**：非升级 `AddBodyLuck(DuraMax*0.002)`=0.2 由 `use_item` 落，升级再 `AddBodyLuck(100)` ⇒ 终值 100.2（golden）。
- 对拍测试 `tests::it08_levelup_host_parity`：过滤 idents `{53,44,45,52,752}` 按**线上序逐字段**比 `pumped`，逐字段比派生
  `max_hp/max_mp/hp/mp/dc`（golden `wabil`），并断言升级后 `WAbil==Abil`（exp/level/max_exp）。`NeedExps` 取夹具 `config.need_exps_head`
  （生产 `Exps.conf` 前缀，sha `07d7801c…` 已入夹具，`[6]==80` 守卫同源）。
