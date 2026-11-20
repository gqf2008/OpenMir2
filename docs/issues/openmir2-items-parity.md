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

1. **IT-18（当时唯一的 `implemented_unverified`；现已转 `verified`，见 §7）先做成「单条 verified 样板」**（历史小节，保留原文）：
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
| `tooth_drain_hit_cap_red` | 内存台账的 `drain.hit_cap/max_passes` 克隆 | `hit_cap=true` 或 `max_passes!=8` | D1/D2 抽空完整性 |
| `vacuous_guard_empty_or_missing_cases_red` | 构造 `{"cases":[]}` / `{}` | 去掉/清空 `cases` | Z1/Z3 防空转 |
| `net_priority_queue_equal_keys_dequeue_is_sift_order_not_insertion` | `msgqueue::NetPriorityQueue` 直测 | 3 条同键入队 `[0,1,2]` | 出队必须是 `[0,2,1]`（sift 序，**不是排序**） |

（每条「篡改⇒红、还原⇒绿」各给原文；与魔法线 §19c 同法，**不碰夹具文件本身**。）
> **2026-11-20 更正（a2 清点）**：原表列的 `tooth_notice_same_priority_ident_swap_red`、`tooth_immediate_order_swap_red`、`tooth_state_field_changed_red` **在代码里不存在**——已改为上表三个真实存在的牙。其中"同 priority、不同 ident 互换"在**多重集**语义下本就不可咬（互换后多重集不变），故不再保留该牙名。

## 7. 待办与不能标 verified 的门槛

- **IT-18**：**已转 `verified`**（以清单 `docs/issues/parity-gap-items.json` 为准；①oracle 等价性 ②L2/D 判定面 ③Z1–Z6 ④clippy 均已落地并过审）。**原文「仍保持 implemented_unverified」于 2026-11-20 作废**（`a2` 清点发现卡面与清单相反）。
- **状态快照（2026-11-20，以清单为准）**：`verified` = IT-01/02/03/04/05/06/08/18/19；`implemented_unverified` = IT-14/15/16；`missing` = IT-07/09/10/11/13/17 与 **IT-12（实现与行为面证据已在 `card/items-it12`，status 待 QA 签后再动）**。
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

## 8.1 夹具源锚 `oracle_source_sha256` 的计算口径（C1，2026-11-20）

- 夹具里声明的 `oracle_source_sha256` **按 git blob（LF）** 计算：逐文件 `utf8(rel+"
") + blob 字节 + "
"` 后 sha256（与探针 `SourceAnchorSha256` 的算法逐字节一致，只是取 blob 而非工作树）。
- **为什么必须写明**：Windows 上 `core.autocrlf=true` 时工作树是 CRLF、blob 是 LF ⇒ 同一 commit 会算出两个值（工作树 `c8a18e59…` vs blob `51a289e2…`）。夹具声明的是 **blob 值**。
- **自证工具（已登记进工具门）**：`python -I -B tools/parity/oracle_source_sha.py --fixture mir2-rs/tests/parity/m3/fixtures/eat_vectors.json` ⇒ `MATCH` / rc=0；`--selftest` 覆盖顺序敏感、工作树 CRLF 不敏感、夹具 round-trip。

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
- **修正留痕（2026-11-20）**：非升级支的通知线序修正为 **622→44**（键 = RM `(byte)wIdent`；见 §9.12「修正：Shape13 通知线序与 oracle 相反」）。IT-08 的 verified 结论不变（升级支顺序未动），status 不动。
- 对拍测试 `tests::it08_levelup_host_parity`：过滤 idents `{53,44,45,52,752}` 按**线上序逐字段**比 `pumped`，逐字段比派生
  `max_hp/max_mp/hp/mp/dc`（golden `wabil`），并断言升级后 `WAbil==Abil`（exp/level/max_exp）。`NeedExps` 取夹具 `config.need_exps_head`
  （生产 `Exps.conf` 前缀，sha `07d7801c…` 已入夹具，`[6]==80` 守卫同源）。

### 9.8 IT-01/IT-03 可达性整改 + items 侧 `derived` 机检（A2）

**问题**：`eat_vectors.json` 各例 `level=1` 却写 `max_hp=100/max_mp=80`（shape3 101/55）；战士登录派生
`MaxHP(L)=14+Round((L/4+4.5+L/20)*L)`、`MaxMP(L)=11+Round(L*3.5)`（`Server.conf` WarrHP=4/Rate=4.5）⇒
`L1=19/15`。这两字段被 **Shape1**（`IncHealthSpell(AC,MAC)` clamp 到 MaxHP/MaxMP）与 **Shape3**
（`Round(MaxHP/100*AC)`/`Round(MaxMP/100*MAC)`）消费 ⇒ 生产不可达。

**整改（A2，逐例挑可达级，保分支）**——`ReachAbil(level,hp,mp)` 用真身 `RecalcLevelAbilitys()`+`RecalcAbilitys()`
按 level 派生 MaxHP/MaxMP/AC/DC/MaxWeight 等，HP/MP 用入参（须 ≤ 派生 Max）：
| case | level | hp,mp | 保的分支 |
|---|---|---|---|
| `shape1_fixed` | **L2**(24/18) | 2,2 | `2+AC20=22<24` **固定量不触发 clamp**（A1 会丢） |
| `shape1_clamp` | **L2**(24/18) | 2,2 | `22≥24`→clamp 24/18 |
| `shape3_percent` | **L4**(37/25) | 10,10 | `Round(37/100*5=1.85)=2`；`Round(25/100*10=**2.5**)=**3**(远离零) |
| 其余非消费例 | L1(19/15) | hp≤19,mp≤15 | max_hp 不被消费，只需状态可达 + 过 derived 门 |
| `stdmode1_hunger_recalc` | L1(19/15) | 15,15 | 原已可达，保留 |

**弱牙修**：旧 `shape3` 用 `max_mp=55/mac=10 ⇒ Round(5.5)=6`——AwayFromZero 与银行家都得 6，**无区分力**；
改用 L4 的 `2.5`（远离零=3、银行家=2）。先实测 oracle（探针真身 `HUtil32.Round=(int)Math.Round(r,AwayFromZero)`）再写断言。Rust `round_away` 同 away。

**`derived` 机检（items 侧对等 `MagicGolden/FixtureContract.cs` 的 rc=2 门）**：探针 `RunCase` 内、写盘前，
按 case 的 `level` 用真身 `RecalcLevelAbilitys()+RecalcAbilitys()` 复算 `WAbil.MaxHP/MaxMP`，与向量声明断言相等；
不等 ⇒ stderr `DERIVED_MISMATCH …` + `Environment.Exit(2)`，**不落盘**。**负对照**（真跑）：注入一个手填不可达
`max_hp` 的 case ⇒ `DERIVED_MISMATCH` + `rc=2` + 夹具文件 sha 不变（证明"拒生成、不改盘"）。

**影响面**：这是**夹具状态改为可达**的整批重生成（Rust 代码不改，已正确镜像 oracle）；`expect` 变化逐 case 记
`old→new + 理由(夹具状态改为可达/golden 重生成)`，不得写"Rust 输出变了"。IT-08 的 exp 例 `wabil_before` 的
max_hp 随之变为派生值（exp 不消费它；levelup 例经 recalc 覆盖为 L6 值，golden 行为不变），属无害重生成。

**§9.8 交付补记（三句 + 门污染教训）**：
1. **旧弱牙为何弱**：旧断言 `max_mp=55/mac=10 ⇒ Round(5.5)=6`——`5.5` 在 `AwayFromZero` 与银行家舍入下**都得 6**，断言恒等、无区分力。
2. **新证据来源（非从 Rust 反推）**：重生成夹具 oracle 实测 `eat_ok_shape3_percent`（L4 max_hp=37/max_mp=25）`expect.wabil hp=12, mp=13`，即 `Round(1.85)=2`、`Round(2.5)=3`（远离零；银行家=2）。弱牙断言改为 `mp==13`（`2.5⇒+3`），能区分两种舍入。
3. **这是"夹具状态改动"引出的测试同步，非产品逻辑改动**：`useitem::tests::shape3_percent_rounds_away_from_zero` 读夹具、旧值 55/46 是旧夹具产物；仅同步该测试的注释+两条断言。`git diff e1d89cd12...HEAD -- '*useitem.rs'` 只含该测试行，`use_item` 实现零改动。
4. **门必须在 throwaway 对象上复算（"测量的动作改变被测对象"）**：`derived` 门最初在 **eat po 上**跑 `RecalcLevelAbilitys()+RecalcAbilitys()`——二者会 `SendMsg(RM_ABILITY)` / 改 `CharStatus`，**污染 eat po 队列**，令每例多出 `RM_CHARSTATUSCHANGED(10139)`。改到独立 throwaway po（`DeriveWarriorMaxHpMp`）复算后 10139 消失。**门/测量永远不得作用于被测对象。**

### 9.9 IT-14 修复油 Shape9（`RepairWeapon`）— verified（2026-11-20 升格）

**关键归类订正**：`EatUseItems(case9=RepairWeapon)` 由 **StdMode 3** 的 `default` 调用（`PlayObject.Base.cs` 的 `switch(stdItem.StdMode) case 3: ... default: EatUseItems(shape)`），**不是 StdMode0**；StdMode0 的 `default` 是 AC/MAC 入池。故修复油 = **StdMode3 / Shape9**（初版误放 StdMode0 臂，已被对拍抓出并订正）。

**实现**（`use_item` 臂 `3 if tpl.shape == 9`，操作 `state.extra.weapon: Option<BagItem>`）：无武器 / `Index<=0` / `DuraMax<=Dura`(已满) ⇒ 不吃（`SM_EAT_FAIL`）；否则 `DuraMax -= (DuraMax-Dura)/RepairItemDecDura`、`nDura=min(5000, DuraMax-Dura)`、`nDura<=0` 不吃、否则 `Dura+=nDura`、发 `RM_DURACHANGE(10125)` + 绿色 `RM_SYSMESSAGE(11004)`（颜色参数），eaten。宿主把 `state.extra.weapon` 写回 `sg.use_items[SLOT_WEAPON]` 持久化。

**配置链路（C）**：`[Setup] RepairItemDecDura`（缺省30/现网90）→ 探针读 `SystemShare.Config.RepairItemDecDura` 写入夹具 `ctx.repair_item_dec_dura` → 比较器 `i32v(&c[...])` 读回 → `use_item` 用它作除数。oracle 与 Rust 除同一配置值，判定路径无写死。

**比较面（耐久必须进，req）**：夹具新增 `weapon_before`（顶层）+ `expect.weapon_after`；比较器 `state.extra.weapon = weapon_of(&case["weapon_before"])`，`use_item` 后 `assert_eq!(state.extra.weapon 的 index/dura/dura_max, weapon_after)`；另 `RM_DURACHANGE` 的 `n_param1=Dura/n_param2=DuraMax` 本就在 notices 多重集比对内。三支用例：`eat_ok_shape9_repair_normal`（5000/10000→9945/9945、`[635]`+`[10115,10125,11004]`）、`eat_fail_shape9_full`（10000/10000→636、不改）、`eat_fail_shape9_no_weapon`（null→636）。

**A/B 说明（机械同步面）**：`handle_eat_cmd` 新参 `repair_item_dec_dura` 12 调用点（主 `enter_cfg.repair_item_dec_dura`；11 测试传 `90`——`ctx.repair_item_dec_dura` 在 `use_item` **仅 `9 =>` 臂一个读点**，非 Shape9 用例永不到达 ⇒ 占位值不参与判定）。`UseItemContext` 15 字面量；其中比较器 `matches_oracle_eat_fixture` 与 `nodrug_guard_does_not_consume` 两处**从夹具 `c` 取 green/dec**（配置进比较面），其余静态 `0/0/90`。Z5 豁免名单 +2（`eat_fail_shape9_full`、`eat_fail_shape9_no_weapon`，纯失败一层）。

**运行期帧缺口（用户可见，非仅夹具）**：`SysMsg(WeaponRepairSuccess, Green)` → `SM_SYSMESSAGE` 文本帧，运行期宿主 `notice_frame` 用静态 `session.sys_msg` ⇒ 修复成功绿字**线上不发**（oracle 发）。同 StdMode1 food 口径，见 **`docs/issues/openmir2-runtime-sysmsg-text-frames.md`**。⇒ **2026-11-20 已补齐**：`String.conf` 键 `WeaponRepairSuccess` 经 `SysMsgTexts` → `NoticeText::Inline` 入队，
`notice_frame` 用 `EDCode.EncodeString(文案)` 出尾；oracle 夹具 `eat_ok_shape9_repair_normal` 的 11004 `text`（hex `564c…5e5c`）
由比较器逐字节核 ⇒ **IT-14 升格 `verified`（card/items-b3）**。

### 9.10 IT-15 特修油 Shape10（`SuperRepairWeapon`）— verified（2026-11-20 升格）

**归类**：与 IT-14 同一条 **StdMode3** 的 `default: EatUseItems(stdItem.Shape)`（`PlayObject.Base.cs:2162-2163`），只是 `EatUseItems` 的 **case 10**（`:2292`）→ `SuperRepairWeapon()`（`PlayObject.cs:1589`）。**非 StdMode0**。

**实现**（`use_item` 臂 `3 if tpl.shape == 10`，操作 `state.extra.weapon`）：`UseItems[Weapon]==null || Index<=0` ⇒ 不吃（636）；否则 `Dura=DuraMax`、发 `RM_DURACHANGE(10125)` + 绿色 `RM_SYSMESSAGE(11004)`、eaten。**无"已满"守卫**、**无 `RepairItemDecDura`**。宿主写回 `sg.use_items[SLOT_WEAPON]`（复用 IT-14 写回）。

**"已满"语义两条相反（两行对照表，防误当 bug）**：
| Shape | 油 | 已满(Dura==DuraMax) | 依据 |
|---|---|---|---|
| 9 | 修复油 `RepairWeapon` | **不吃**（`DuraMax<=Dura ⇒ false ⇒ 636`） | `PlayObject.cs:1567` |
| 10 | 特修油 `SuperRepairWeapon` | **仍吃**（无已满守卫；`Dura=DuraMax` 不变、635 + `RM_DURACHANGE`） | `PlayObject.cs:1589` |

**三例**：`eat_ok_shape10_repair_normal`（5000/10000→10000/10000、`[635]`+`[10115,10125,11004]`）、
`eat_ok_shape10_full_still_eaten`（**10000/10000→10000/10000 不变、仍 635+`RM_DURACHANGE`**一—单独命名钉死反直觉行为）、
`eat_fail_shape10_no_weapon`（null→636）。weapon `weapon_before/after` 进比较面（复用 IT-14 的 `weapon_of`+`weapon_after` 断言）。

**sysmsg（复用）**：`SysMsg(MessageSettings.WeaponRepairSuccess, Green, Hint)`（`PlayObject.cs:1597`）——**与 IT-14 同一文案**（非单独"特修成功"）。⇒ IT-14/IT-15 **共用同一运行期 `SM_SYSMESSAGE` 文本帧缺口**，见 `docs/issues/openmir2-runtime-sysmsg-text-frames.md`（同时指两条）。

**牙（三颗）**：T1 `Dura=DuraMax`→`+1`（weapon_after 不符）；T2 去 `RM_DURACHANGE` 推送（notices 缺 10125）；**T3 `3 if tpl.shape == 10`→`== 9`（串到修复油支）⇒ 必红**（防 shape 臂写错）。

**状态**：`verified`（2026-11-20，card/items-b3）。同一 `WeaponRepairSuccess` 文本链路（IT-14）；夹具三例的
11004 `text` 非空且与 Rust 编码逐字节一致（`eat_ok_shape10_repair_normal` / `_full_still_eaten` 两例核文本）。

### 9.11 IT-16 彩票 Shape11（`WinLottery`）— implemented_unverified（失败支为已证死路）

**归类**：**StdMode3 / Shape11**，经 `default: EatUseItems(shape)` 的 **case 11**（`PlayObject.Base.cs:2298`）→ `WinLottery()`（`PlayObject.cs:3809`）。**物品恒被吃**（`case 11: WinLottery(); result=true;`）。

**RNG 口径（P1，逐值对拍）**：`nRate = M2Share.RandomNumber.Random(Config.WinLotteryRate)`（`PlayObject.cs:3813`）。
- **同一实例的符号链（可核，4 段）**：①`M2Share.RandomNumber = RandomNumber.GetInstance()`（`src/M2Server/M2Share.cs:194`）→ ②`OpenMir2.RandomNumber` 的**私有静态** `private static Random random`（`RandomNumber.cs`）→ ③`RandomNumber.Random(int)=random.Next(max)`→ ④魔法种子钩子 `mir2-rs/tests/parity/m3/rng-hook/StartupHook.cs` **文件头明写**：反射替换的正是这个 `RandomNumber.random`（换 `RecordingRandom(new Random(seed))`）。⇒ WinLottery 与魔法共用同一 RNG 实例。
- **caveat（重要）**：P1 的"同种子同序列"**只在 legacy `.NET Random` 算法口径下成立**（钩子把 `random` 换成定种子 `new Random(seed)`=legacy `CompatSeedImpl`/Knuth 减法）。**生产无参 `new Random()` 在 .NET6+ 是 xoshiro、非同序列**。**oracle 原版运行在 legacy 上 ⇒ legacy 即我们该对的目标；Rust `crates/shared/src/rng.rs` 复刻的正是它。** 别以为"任意 .NET 版本都行"。
- **P1 用例（已落地）**：夹具顶层 `lottery.cases` 15 例，每例用 **`MIR2_RNG_SEED + 序号` 独立播种**（抽值随例变化，常量冒充跑不过）⇒ ①各级中奖各一（目标档收成 `[d,d]`、其余档挪到 `d+1`）；②未中（全档挪开）；③`min`/`max` 端点各一边界例；④`Count<NoWinLotteryCount` **闸取假**（档命中但 `0<0` ⇒ 表现为未中、物品仍吃）；⑤部署档位一例（不改判据、按实记录：实测 `nRate=21242` 落空档 ⇒ 未中）；⑥P2 强制桶（`6Min=0/6Max=rate-1` ⇒ 必中 6）；⑦**闸闭的非零形态**（`Count == NoWinLotteryCount == 500`，5 档宽区间命中 ⇒ 未中；把闸写成 `<=` 的实现会红）；⑧**命中档但该档 `Gold == 0`**：oracle `PlayObject.cs:3868 if (nGold > 0)` 为假 ⇒ 虽等级 +1（`:3820` 的 `WinLotteryLevel6++` 在闸内、与 gold 无关）仍判**未中**（`NoWin += 500`、红字、无 `GoldChanged`）；把「命中即 Won」的实现会红；⑨档位重叠链首即停（6 档与 5 档同为 `[d,d]` ⇒ 只中 6 档）。**⑦⑧ 是 QA `3b` 指出的必需盲区**（闸 `<`/`<=`、命中但 0 金）；**⑨ 与既有 `forced_bucket_p2` 判别力重叠**（该例六档全 `[0,29999]`，抽值同时落六档、链首即停应取 6 档），**保留作直读回归，不构成新增验证**。**抽值逐值对拍**：oracle 侧把 `ProbeRandom(caseSeed)` 临时装进 `OpenMir2.RandomNumber.random`（即 `WinLottery` 读的私有静态字段），记录到的取数点实测为 **`PlayObject.cs:3813`**（符号链直接证据）且「本次调用恰抽一次」；夹具记 `seed/form/arg/result/site`，Rust 侧 `RandomNumber::with_seed(seed).random_below(rate)` 必须逐值相同（`useitem::tests::it16_lottery_fixture_rng_and_deltas`）。
- **⚠ 部署配置下 1 等奖为死代码**：1 级判据是 `WinLottery1Min + WinLottery1Max == nRate`（`PlayObject.cs:3859`，**和式等值**，不是区间），部署 `16180 + 16185 = 32365` ≥ `WinLotteryRate = 30000` ⇒ `nRate ∈ [0,30000)` **永远等不到**。⇒ **1 等奖用例必须覆盖 `WinLottery1Min/1Max`**（取 `5000/8000` ⇒ 和 `13000`；已核 `13000` 不落任一其它档 `[1,4999]/[14000,15999]/[16000,16149]/[16150,16169]/[16170,16179]`），并把它写进夹具 `config` 快照——**同时记原始值与覆盖值**（否则该用例不可复现，且后来人不知道它偏离了部署配置）。
- **未中档的 seed 空间（越宽 P1 越稳）**：`nRate = 0`（`6Min=1` 故 0 不算 6 档）、`[5000,13999]`、`[16180,29999]` 三段全落"未中"。
- **P2 稳健例一条**：配置强制桶（如 `6Min=0/6Max=Rate-1` 覆盖全区间 ⇒ 必中 6），断言只写行为/边界、不依赖抽值（兜底）。

**① 全局计数是进程级（必须复位）**：`WinLotteryCount/NoWinLotteryCount/WinLotteryLevel1..6` 在 `SystemShare.Config` **全局可变**，多 case 同进程**互相污染**。⇒ **探针每个 lottery case 前显式复位**（`WinLotteryCount=0`、`NoWinLotteryCount=初值`、`WinLotteryLevel1..6=0`）到**已知初值**再 `ClientUseItems`；**比较器断 delta 不断绝对值**：中奖 ⇒ `WinLotteryLevelN +1`、`gold += WinLotteryNGold`；未中/闸闭 ⇒ `NoWinLotteryCount +500`。Rust 宿主侧同口径 per-case 复位。（生产 `Server.conf` 两者=0 ⇒ `0<0` 闸初始**闭**；测"中奖"须把 `NoWinLotteryCount` 置>WinLotteryCount，如 500>0。）
- **启动初值来自配置，不是 0**：`GameApp.InitializeSystem()` 的 ctor 清零（`src/GameSrv/GameApp.cs:77-84`）发生在 `StartingAsync → GameShare.LoadConfig()` **之前**；`src/Modules/SystemModule/Conf/ServerConf.cs:645-652` 随后从 `[Setup]` 读这 8 个键**覆盖**清零值 ⇒ **有效初值 = 配置值**（部署全 0）。⇒ 这 8 个键必须一并解析进 `WinLotteryCfg`（见「配置同源」），否则「配 `NoWinLotteryCount>0` 才能中奖」——IT-16 的核心场景——在 Rust 侧**永远起不来**。探针的 per-case 复位值也取自这份配置，不写死 0。
- **默认配置下永不开奖**：`WinLotteryCount=NoWinLotteryCount=0 ⇒ 0<0` 为假 ⇒ **服务端默认状态永不中奖**——非 bug，是**玩法事实**：服主须配置开启，否则该物品在默认服里就是"花钱买空气"。别让后来者以为我们对拍漏了中奖分支。

**架构决议（作用域与所有权，`opc-07` 2026-11-20 批准）**：
- **`world` 是 per-World，不是进程单例**：今天进程只有 1 个 World（`lib.rs` 只 spawn 一条 `m2-sim` 线程，World 在 `sim_loop` 内建）；分片设计是「同进程多 World」（`docs/10k-shard-design-20261116.md` §0.1/§2.1、`docs/shard-runtime-20261116.md` §2「空片 = 永不接客的 World」、`shard.rs` `DEFAULT_SHARD_COUNT=1`）。oracle 这 8 个计数是**进程级静态** ⇒ 挂 `world` 会让 N>1 变成每区独立计数。
- ⇒ **可变计数放进程级共享**：新模块 `bins/game-svr/src/winlottery.rs` 的 `WinLotteryState { count, no_win, level: [i32;6] }` + `WinLotteryShared(Arc<Mutex<WinLotteryState>>)`；在 `run_inner`（紧挨 `ids_online`/`world_start`）创建，`Arc::clone` 进 `m2-sim` 闭包 → `sim_loop` 参数 → `handle_eat_cmd` 参数。**不用进程 static**（并行单测互相污染；static 只适合启动后只读的配置，如 `chat::install_send_ref_msg_range`）。
- **登记为「游戏状态层」的唯一显式例外**：设计件要求「World 间无共享可变状态」，而 oracle 该组计数本身就是进程全局 ⇒ 此处**显式共享**；RNG 仍走 `world.rng`（N>1 本就会被切开，与设计一致）。**不许**为"干净"改成 per-shard 语义。gate 层另有进程级共享 `TOMBSTONED_GATE_SOCKETS`（`OnceLock<Mutex<HashSet>>`，基础设施、非游戏状态；登记在 `openmir2-world-keeps-dead-session`）——故「唯一」限定在**游戏状态层**。
- **分层**：纯函数 `win_lottery(rng, &cfg, &mut state) -> WinLotteryOutcome` 出 outcome；宿主负责加锁、写金、发帧。

**② RNG 钩子是新测量口径（非干扰须给证）**：IT-14/15 的 eat-probe 未用 `DOTNET_STARTUP_HOOKS`；IT-16 启用后要证**既有非 lottery 例**不受影响——**实测 37 例**（"31 例"是更早的转述数，其后 B3 六例入夹具 ⇒ `eat_vectors.json` 顶层 `cases`=37 且全非 lottery；再早的"28 例"是 IT-15 之前的数，作废）。
- **"夹具逐字节相同"字面做不到**：解包产物 `make_index` = `GetItemNumber()` = `ItemNumber + HUtil32.GetTickCount()`（`src/Modules/SystemModule/SystemShare.cs:449-455`），任两次运行必不同；它同时出现在 `added_items` 与终态 `item_list`。⇒ **归一化 + 原始 diff 双存档**：
  1. `A1/A2` = 无 `MIR2_RNG_SEED`（钩子关）跑两次；`B1/B2` = 钩子开 + 两个不同 `MIR2_RNG_SEED`；
  2. **归一化清单（两项，均为墙钟派生、非行为）**：`added_items[*].make_index` 与 `expect.item_list[*].make_index`（**只掩新生成件**——按该例 `added_items` 的值集合匹配，既有件 `make_index=1` 保持可比）；**顶层 `lottery` 块单独比**（A 运行 `enabled=false`、`cases=[]`，B 运行有 15 例；它只存在于播种运行，不参与 A≈B 的逐字节面）。原始 diff 同档保存，不隐藏；
  3. 判据与**实测结果**（2026-11-20，seed `20261120`/`20261121`；早先一次为 31 例口径，下列为本轮 **37 例**复跑）：
     `normalize(A1)==normalize(A2)` **0 处差异**（噪声基线；A 运行 `cases=37`、lottery 关）→
     `normalize(A1)==normalize(B1)` **0 处**（钩子不改既有 **37 例**与元数据）→
     `normalize(B1)==normalize(B2)` **0 处**（同上面的 37 例面）→
     **`B1` 与 `B2` 的 15 例 lottery 抽值/种子 15/15 全不同**（`seeds_same=0/15 results_same=0/15`；B 运行 `cases=37, lottery.cases=15`）。
     三组 `it16_fixture_diff.py … --drop-lottery` 均 `diffs=0`、rc=0；钩子安装证明取 stdout
     `[RngSeedHook] 已注入种子 seed=20261120/20261121`。
  4. 钩子安装证明取 stdout 的 `[RngSeedHook] 已注入种子 seed=…`（钩子失败 exit 97）；探针侧另有「恰抽一次且等于预测」的自检（不一致 rc=2 不落盘）。
- **说明**：lottery 的抽值与来源行由探针安装的 `ProbeRandom` 直接记录（见 P1 用例），**不依赖钩子台账**；钩子在这批的唯一职责是第 3 条的 A/B 非干扰面。
- **证据脚本（入仓、可复跑）**：`mir2-rs/tests/parity/m3/tools/it16_fixture_diff.py` —— 归一化（两项）+ 逐路径 diff，`--drop-lottery` 排除顶层 `lottery` 块。用法（仓库根）：
  `python -I -B mir2-rs/tests/parity/m3/tools/it16_fixture_diff.py <A1.json> <B1.json> --drop-lottery`
  退出码 0 ⇒ `diffs=0`（钩子不改既有 37 例）；1 ⇒ 有差异并逐条打印路径。
- **grep 提示（仅接线提示，非行为证据，§12）**：eat 路径（Shape1/2/3/13/31、GetExp、`RepairWeapon`/`SuperRepairWeapon`、解包、StdMode1 食物）取值区**零 `RandomNumber` 调用**（仅 `WinLottery`@3813、未实现 Shape4@1512+、战斗 `GetAttackPower` 有抽）⇒ *提示*钩子无影响，**不作证据**。
- **冲突处置**：任何非 `make_index` 差异 ⇒ **停下报 `opc-07`**，不得拿 grep 洗白。

- **夹具重出（2026-11-20，card/items-b3，master 的 eat-probe）**：以 `MIR2_RNG_SEED=20261120` 重出后
  `cases=37`（31 既有 + B3 六例）、`lottery.cases=12`；与合入前逐路径比只剩 **8 处** —— 六例新增 case/template、
  `cases.len 31→37`、`oracle_assembly_sha256`（构建产物 hash，仅溯源）。**31 例除墙钟 `make_index` 外逐字节不变**；
  `oracle_source_sha.py --fixture …` ⇒ **MATCH**（`51a289e2…`）。
- **彩票 15 例的追加来源（2026-11-20，QA `3b` 三类盲区）**：⑦–⑨ 三例的期望**全部来自 oracle 探针实出**——
  `dotnet build mir2-rs/tests/parity/m3/eat-probe -c Release` + `MIR2_RNG_SEED=20261120 dotnet …/eat-probe.dll <temp>`，
  探针原文 `ORACLE_SOURCE_SHA256 51a289e2… / TEMPLATES 25 CASES 37`，**不是手写值**（防"为例过测试而编"的质疑）。
  追加方式：探针先出到临时文件，脚本证明 `temp.lottery.cases[0..12]` 与仓库**逐字符串相同**
  （`EXISTING_12_BYTE_IDENTICAL`）后只插入后 3 例；既有 37 例（含墙钟 `make_index`）与既有 12 例零改动
  （`git diff` 只 +3 例、`git diff --check` rc=0）。
  **两个 hash 字段角色不同**：`oracle_source_sha256`（M2Server 源码锚，**校验用**）不变 `51a289e2…`；
  `oracle_assembly_sha256` 因构建非位级可复现而变（`ee2cc2fb→a3b9f1fc`），夹具注释已声明该字段
  **仅溯源、不参与校验**——不是锚漂了。

**配置同源（27 键，不写死）**：部署 `Server.conf` 实际 **27** 个 `WinLottery*` = **19 参数 + 8 状态初值**（原卡面"25 键"作废）。
- 19 参数：`WinLotteryRate` + `WinLottery{1..6}Min/Max` + `WinLottery{1..6}Gold`（部署：Rate=30000、6Min=1/6Max=4999…1Min=16180/1Max=16185、Gold 1000000…500；与 `GameSvrConf.cs` ctor 缺省逐键相同）。
- 8 初值：`WinLotteryCount`/`NoWinLotteryCount`/`WinLotteryLevel1..6`（`src/Modules/SystemModule/Conf/ServerConf.cs:645-652`；部署全 0）。
- 落点：`GateConfig` 新字段 `win_lottery: WinLotteryCfg`（`config.rs`，仿 `repair_item_dec_dura` 的 struct+parse+default）+ `EnterConfig` 字段 + `handle_eat_cmd` 按值传（全 i32 ⇒ `Copy`）。**同一结构**（`opc-07` 2026-11-20 批准）：oracle 就是同一批字段同一批读入，"配置即初值"是 oracle 事实。
- 结构：`rate: i32` / `min|max|gold: [i32;6]`（idx0=1 等奖…idx5=6 等奖，映射只在解析处做一次）/ `init_count` / `init_no_win` / `init_level: [i32;6]`（`init_` 前缀标"只做启动初值"；唯一入口 `init_state()`）。

**宿主 WinLottery**（`handle_eat_cmd`，shape11 `eaten` 后）：`world.rng.random_below(rate)` 抽 → 6 段匹配（判序 6→5→4→3→2→1，1 级为和式等值）+ `Count < NoWinLotteryCount` 闸 → 中：`WinLotteryLevelN++`、绿 SysMsg、`ground::inc_gold(gold, nGold, HumanMaxGold)` 成功才 `GoldChanged()`（`SM_GOLDCHANGED`），失败走 `DropGoldDown` bug-replica（**净金币不变且不发 `GoldChanged`**；死路已证：`ItemId` 恒 0 ⇒ `AddItemToMap` 守卫直接 false）；未中/闸闭：`NoWinLotteryCount += 500` + 红 SysMsg。
- **oracle bug，为 parity 复制**：绿字 SysMsg 在 `IncGold` **之前**发出 ⇒ `IncGold` 失败也照样先报"中奖"（`PlayObject.cs:3873-3897`）。**照抄**；将来若要修正玩法，须单独开卡并接受与 oracle 偏离。
- **帧序断言（不是注释）**：
  - 主断言：宿主中奖用例捕获物化通知序列，断言 `ident as u8` **严格升序**（`RM_GOLDCHANGED` 10136→152 < `RM_SYSMESSAGE` 11004→252），写成**性质断言** `windows(2).all(|w| w[0] < w[1])`（不钉两元素字面量，将来加帧不用改测试）；比较器仍按 §3 七字段**多重集**比（夹具每条的 `priority` 是其旁证）。
  - 次断言：捕获 `SimToGate::SendFrame`，断言 `idents == [SM_EAT_OK(635), SM_GOLDCHANGED(653)]`；`RM_SYSMESSAGE` 因 `wire.sys_msg: None` 落 `notice_frame → None` ⇒ **"登记丢弃"必须出现在证据里**（现有 WRN 行或计数字段），不能只靠"我们知道它会 None"。

**恒吃单列断言**：`use_item` 的 `3 if tpl.shape == 11` 臂**仅** `eaten = true`（无 RNG、无金币、无全局计数）；断言"物品恒被吃"**与中/不中解耦**（最易写错处）。

**牙（七类，2026-11-20 逐个实跑：8 个变异体全红、还原后 sha 与基线相同）**。**牙必须打在被测行为路径上**：首轮曾把「帧序」牙打在 oracle 自洽用例上（红出 `[636]` vs `[635,622,653]`，说明打错目标），改打到宿主行为用例后才咬住 —— 过程留痕，别把「牙红了」当成功，先看红在哪条断言。
1. 区间边界改一：`>=` → `>`（`boundary_is_inclusive_on_both_ends` 红）；
2. 去 `IncGold`：`inc_gold(...)` 前置 `false &&`（宿主用例的金币/653 断言红）；
3. 把 `eaten` 与中奖绑定：Shape11 臂 `eaten = true` → `false`（闸闭/未中例的 635 断言红）；
4. 闸判定反转：`<` → `>=`（`gate_closed_stops_at_matched_band_and_counts_as_lost` 红）；
5. **运行时改读 `cfg.init_*`**：`st.count < st.no_win` → `cfg.init_count < cfg.init_no_win`（`pure_decision_reads_state_not_cfg_init` 红）；
6. **`init_state()` 逐键映射**：`count: self.init_count` → `count: 0`；以及 `level` 1↔6 交换（各红 `init_state_maps_all_eight_keys_without_shift`）；
7. **帧序对调**：`win_notices` 里绿字放到 GoldChanged 之前（`notice_order_is_pump_priority_ascending` 红）。

**状态**：`implemented_unverified`（运行期 `SM_SYSMESSAGE` 文本帧缺口：`WinLotteryNMsg`绿/`NotWinLotteryMsg`红，引用 `openmir2-runtime-sysmsg-text-frames.md`；该卡由 `opc-07` 加指 IT-16）。
- **已证死路（2026-11-20 修正，替代原「无 oracle 证据」口径；opc-19 裁定）**：`IncGold` 失败支**可达**（`Gold + nGold > HumanMaxGold`，`src/M2Server/Player/PlayObject.Base.cs:165-172`），但它的「掉堆 + `RM_ITEMSHOW`」效果在 oracle 里**结构性不可能发生**：
  1. `DropGoldDown` 构造的 `MapItem` 不设 `ItemId`（`src/M2Server/Actor/BaseObject.cs:740-748`）；
  2. `Envir.AddItemToMap` 首条守卫 `if (mapItem.ItemId == 0) return false;`（`src/M2Server/Maps/Envirnoment.cs:88-93`）；
  3. 全仓 `MapItem.ItemId` 唯一赋值点被注释掉（`src/Modules/SystemModule/Data/MapItem.cs:41`）⇒ 本地构造的 `MapItem` 恒 `ItemId=0`。
  ⇒ 失败支净效果 = **金币不变 + 不发 `RM_GOLDCHANGED` + 不发 `RM_ITEMSHOW` + 绿字照发**（绿字在 `IncGold` 之前，`PlayObject.cs:3873-3894`）。同一守卫也是 `ScatterGolds`（`BaseObject.cs:1728-1736`）与 `ClientDropGold`（`PlayObject.Operate.cs:242-249`）在 false 时回滚金币的根因 —— 一处根因解释三处行为。Rust 实现与该净效果一致并有用例（`lib.rs` 宿主 `inc_gold` 门 + `winlottery.rs` 单测 ③）；**本支不再计为缺口**，且后来者**不应再去实现掉落**。
- **原「探针跑不了」的说法要更正**：NRE 只来自 `CellArray` 为空（`src/M2Server/Maps/Envirnoment.cs:322-333` 的 `return ref CellArray[0]`；`CellArray` 私有 :36、由私有 `Initialize(w,h)` :1036 分配），反射调 `Initialize(8,8)` 即可；且失败支结果**与地图内容无关**（守卫在碰 cell 之前就 `return false`）。runtime 加固例另开小卡（不阻塞）。
- **follow-up（不并入 IT-16）**：`docs/issues/openmir2-lottery-ticket-gm-realtime.md`（master `50d290b8d`）——`@LotteryTicket` 现读 `GmConfig.win_lottery_*` 的 `Default` 常量快照（`gmcmd.rs`），IT-16 落地后必须改读实时 `WinLotteryShared`；交付里引用此卡。

EAD
### 9.12 IT-09 回城卷 Shape1（`EatUseItems` move 组）— 宿主拦截 + 宿主级对拍（implemented_unverified）

**归类**：`StdMode==3` 的 `default: EatUseItems(stdItem.Shape)`（`PlayObject.Base.cs:2163`）→ `case 1`（`:2231-2236`）：
`SendRefMsg(RM_SPACEMOVE_FIRE, …)` + `BaseObjectMove(HomeMap, 0, 0)` + `result=true`。
`BaseObjectMove`（`:2312-2327`）：**空 `sMap` ⇒ `MapName`**（`string.IsNullOrEmpty`，`:2314-2317`）；
`sX!=0 && sY!=0` ⇒ `SpaceMove`（精确点，落点可走则 0 抽），否则 `MapRandomMove`（两次抽
`Random(W-nEgdey-1)`/`Random(H-nEgdey-1)`，`nEgdey` 只看**目标图**高）。搬运位置/换图簇/RNG 是**宿主行为**，
纯函数 `use_item` 覆盖不到；且 `useitem.rs` 与 IT-16 车道共享写集 ⇒ 本组**零改 `useitem.rs`**。

**实现（宿主拦截，一条钩子）**：`bins/game-svr/src/eat_move.rs`
- `maybe_eat_move`：只接管「首件 `MakeIndex` 命中、模板 `StdMode==3 && Shape∈{1,2,3}`、非 `dead`、非
  `no_drug`」；`dead`/`no_drug`/模板缺失一律回落 `use_item`（禁药红字仍由 `use_item` 出，不造第二份真相）。
  出帧/动作序：`SM_EAT_OK(635)`（oracle 直发）→ `RM_SPACEMOVE_FIRE` 广播**移动前** ±12（含自己）的
  `SM_SPACEMOVE_HIDE(800)` → `World::base_object_move_player` → 移除物品 + `RecalcBagWeight` →
  `SM_WEIGHTCHANGED(622)`。633/634/801 由**共用**的 world tick `drain_map_change` 出（同一生产函数）。
- `handle_eat_cmd`：在 `no_drug` 之后调一次本函数，返回 `true` 即 `return`。
- **`HomeState`（`PlayObject.HomeMap/HomeX/HomeY` 会话快照）**：挂 `SessionGround.home`，**唯一两个写点**——
  ① 登录 `session_ground_from_login`（取 `apply_get_start_point_on_login` **之后**的 final 记录值；
  `red_home_*` = `red_home_of(enter_cfg)` 的 `SystemShare.Config.RedHome*` 快照）；② 3s 支
  `apply_start_point_for_fired_players`（与 `saves[chr].record.data.home_*` **同一次迭代、同源**写回）。
  oracle 节奏：登录（`PlayObject.cs:1199`）与每 3s `CheckDupObjTick`（`PlayObject.Message.cs:266`）各跑
  一次 `GetStartPoint()`（`PlayObject.Base.cs:704-721`：`PvpLevel()>=2` ⇒ `Config.RedHome*`；否则按
  **当前地图**命中 `StartPointList` 就覆盖）⇒ 本快照 **≤3s 陈旧，与 oracle 同节奏**。move 逻辑只读、不回写。

**装置（三向可寻）**：
- 探针：`mir2-rs/tests/parity/m3/move-eat-probe`（**独立于 `eat-probe`**，避免与 IT-16 的夹具重生成相撞）。
  手法：合成真格式 `.map`（52B 头 + 12B/格）+ 真 `Envirnoment` + 最小 `IMapSystem`（只做 `FindMap`）+
  真 `SystemShare.ActorMgr` + 真私有 `ClientUseItems`；`RandomNumber.random` 换固定种子录制包装
  （**先建 `PlayObject`、后注入种子**：ctor 自身 5 抽不进账本）；排空 `MsgQueue` 并按出队序
  pump `8062/10115/8097/8098/8100`。
- golden：`mir2-rs/tests/parity/m3/fixtures/move_eat_vectors.json`。生成（仓库根）：
  `dotnet run --project mir2-rs/tests/parity/m3/move-eat-probe -c Release -- mir2-rs/tests/parity/m3/fixtures/move_eat_vectors.json`
- 夹具契约（golden `scenario_note` + `ctx.presentation` / `ctx.map_file_name` 同步记录）：三图 60x60 全可走；
  **MapName == MapFileName 主干**（无 `[名|文件]` 覆盖）；地图 `Flag.DayLight=true`（`DayBright()=0`）；
  实体 `Dir=4`（`BaseObject.cs:378` ctor）/`Light=0`/`Feature=0`/`Status=0`。

**对拍面（用例 `it09_town_scroll_host_parity`，两例：home 随机 / 空 HomeMap 回落当前图）**：
① 线上帧**逐字段含尾**：`635 → 800 → 622 → 633 → 634 → 801`（634 尾 = `EDCode(MapFileName)`；
801 尾 = `EDCode(CharDesc)`；`Series=MakeWord(Dir,Light)`）② 终态 `MapName/X/Y`（实体+会话）
③ `ItemList` 清空 + `WAbil.Weight` ④ `Rng` 逐次 + 「移动后不得多抽」⑤ **断言 golden `pumped` 的剔除集
恰等于 `[766,54]`**（把豁免变成可验证的等式，不是隐性豁免）。

**牙（五颗，全红-还原-复绿，sha 核验）**：M1 `StdMode` 门 3→4（hook 永不接管 ⇒ 636）；
M2 Shape1 的 `HomeMap` 换当前图（例 1 终态图/落点变）；M3 删空 `HomeMap` 回落（例 2 不动）；
M4 不消耗物品（ItemList/Weight/622 变）；M5 3s 支不回写会话（tick 用例会话断言红）。

**共用路径缺口（登记，**不是** IT-09 私有；影响所有走换图的卡；已独立立卡
[`docs/issues/openmir2-map-change-cluster-gaps.md`](openmir2-map-change-cluster-gaps.md)）**：
1. **766 `SM_AREASTATE` + 54 `SM_MAPDESCRIPTION`**：oracle `RM_CHANGEMAP` 处理器
   （`PlayObject.Message.cs:1774-1779`）在 `SendDefMessage(SM_CHANGEMAP)` 后还调 `RefUserState()`
   （`PlayObject.Base.cs:285` ⇒ `SM_AREASTATE(766)`）与 `SendMapDescription()`（`:887-895` ⇒
   `SM_MAPDESCRIPTION(54)`）；宿主 `drain_map_change` 只出 633/634/801。**客户端影响**：每次换图后
   区域状态（安全区/战斗区位）与地图描述/音乐不刷新。**本卡不实现**：修改共用换图路径影响面远超
   IT-09，须另立卡（对拍面已用等式钉住它只能缺这两条）。
2. **`[名|文件]` 覆盖时的 `SM_CHANGEMAP` 尾**：oracle 的 `Envir.MapFileName` 是 MapInfo 的**取图文件主干**
   （`src/GameSrv/Maps/MapManager.cs:66-96`：`[名|文件]` 的 `|` 右半，`<...>`，缺省=逻辑名），宿主
   `drain_map_change` 用 `MapRegistry::name_of`（**逻辑名**）。现网 `MapInfo.txt` 有 `[M100A|M100 …]`
   形态 ⇒ 该图换图时尾会从 `M100` 变 `M100A`。本夹具无覆盖形态（逻辑名==主干）⇒ 本卡对拍面内**尾逐字节相等**；
   复现方式：给 `move-eat-probe` 造一条 `map_name != map_file_name` 的注册表条目即可击穿（留给修复卡）。

**状态**：`implemented_unverified`（对拍面全绿 + 五牙，但共用路径缺口 1/2 未清 ⇒ 不计 verified）。
转 verified 条件：上述两条并入共用换图路径并进本组对拍面。

### 9.13 IT-10 随机传送 Shape2（`EatUseItems` move 组）— 宿主拦截 + 两支对拍（implemented_unverified）

**归类**：`EatUseItems` case 2（`PlayObject.Base.cs:2238-2246`）：
```
if (!Envir.Flag.boNORANDOMMOVE)
{ SendRefMsg(RM_SPACEMOVE_FIRE); BaseObjectMove(MapName, 0, 0); result = true; }
```
即：**未置位** ⇒ 吃 + 本图随机点（两次抽）；**置位** ⇒ `result=false` ⇒ `ClientUseItems` 落
`SM_EAT_FAIL(636)`，**不消耗、不移动、无 FIRE**。门读的是**当前** `Envir.Flag`（不是目标图）。

**实现**：与 IT-09 同一个钩子（`eat_move::maybe_eat_move` 的 Shape2 分支，§9.12）；本笔补**行为面证据**：
探针两例 + Rust 侧两支用例 + 分支配读/目标图两颗牙。实现无新增代码路径（分支已随钩子落地）。
`boNORANDOMMOVE` 判定 = `registry.flag_of(entity.map_id).bo_norandommove`（当前图）。

**对拍（`it10_random_scroll_host_parity` / 单跑 `it10_random_branch_moves` 与
`it10_norandom_branch_no_eat`；共用 `run_move_parity` 比较面）**：
- 未置位（`eat_ok_shape2_random_scroll_current_map`，seed 20261122）：`635 → 800 → 622 → 633 → 634 → 801`
  逐字段含尾；终态 = `move_map` 随机点；物品消耗；Rng 恰 2 抽（记录器 `site=BaseObject.MapRandomMove`）。
- **置位（`eat_fail_shape2_norandommap_no_eat`）**：`immediate=[636]`、`pumped=[]`、`rng=[]`、
  `rng_draw_count=0`（**记录器自证 0 抽**，不是「没看到数」）、`item_list` 保留原件 1 件、`weight` 不变、
  终态 = 当前坐标。
- 编译期两次 `let mut world = …` 不再复用：**每个 case 独立注册表**（`boNORANDOMMOVE` 逐例），
  临时目录按测试实例编号（并行 `#[test]` 不互删）。
- `rng_draw_count` 是记录器自证计数（`RecordingRandom.Draws.Count`），用例先断言它 == 逐次账本长度，
  再用「Rust 侧下一抽 == 同种子重放下一抽」独立证明 Rust 0 抽。

**牙（分两次跑、分别留原文）**：
- M6 `if norandom` → `if !norandom`：`it10_norandom_branch_no_eat` 红（期望 `[636]`、实得全套 move 帧）、
  `it10_random_branch_moves` 红（期望全套、实得 `[636]`）——**先置位支后未置位支各一次**。
- M7 Shape2 目标图 `cur_map` → `home_map`：`it10_random_branch_moves` 红（634 尾 `EDCode(map)` 由
  `move_map` 变 `home_map`）。还原后三颗用例复绿（sha 相等）。
- 变异脚本注意：`shutil.copy2` 还原会带回旧 mtime ⇒ cargo 可能复用变异体二进制；脚本在还原后
  `os.utime` 强制刷新（本次实测：未刷时「还原后仍红」= 假红）。

**状态**：`implemented_unverified`（同 §9.12：共用换图路径缺口 766/54 与 `[名|文件]`
见 [`openmir2-map-change-cluster-gaps.md`](openmir2-map-change-cluster-gaps.md)）。

### 9.14 IT-11 地牢逃脱 Shape3（`EatUseItems` move 组）— 宿主拦截 + 四向分支对拍（implemented_unverified）

**归类**：`EatUseItems` case 3（`PlayObject.Base.cs:2248-2262`）：
```
SendRefMsg(RM_SPACEMOVE_FIRE, …);
if (PvpLevel() < 2) BaseObjectMove(HomeMap, HomeX, HomeY);
else                BaseObjectMove(SystemShare.Config.RedHomeMap, RedHomeX, RedHomeY);
result = true;   // 恒吃（与 IT-10 的禁随机支相反）
```
`PvpLevel()` = `(byte)(PkPoint / 100)`（`PlayObject.cs:4352`）——**在吃的那一刻**按当前 `PkPoint` 判，
不是登录时判。`BaseObjectMove` 的坐标门是 `if (sX != 0 && sY != 0)`（**`&&`**，`PlayObject.Base.cs:2318`）
⇒ 精确点（可走则 0 抽）否则 `MapRandomMove`（两抽）。

**实现**：与 IT-09 同一个钩子（`eat_move::maybe_eat_move` 的 Shape3 分支，§9.12）；`HomeMap/HomeX/HomeY`
与 `Config.RedHome*` 都取自 `SessionGround.home`（`HomeState`）快照——home 由登录 + 3s `GetStartPoint`
维护，red 是登录时 `red_home_of(enter_cfg)` 的只读 Config 快照。本笔补行为面证据，无新增代码路径。

**对拍（5 例，`it11_dungeon_escape_host_parity`；单跑 `it11_home_branch_exact` /
`it11_red_home_branch_exact` / `it11_red_home_alt_values` / `it11_home_x_zero_y_nonzero_random` /
`it11_home_zero_coords_random`）**：
| case | 输入 | 期望 |
| --- | --- | --- |
| `…home_exact` | pvp=0，home=(home_map,5,6) | 精确落 (5,6)，0 抽，吃 |
| `…red_home_exact` | pvp=2，red=(red_map,20,30)，home=(5,6) | 精确落 (20,30)，0 抽，吃（**误用 home ⇒ 会落 (5,6)**） |
| `…red_home_alt_values` | pvp=2，red=(red_map,**25,35**) | 精确落 (25,35)（证明读快照、非写死 20/30） |
| `…home_x_zero_y_nonzero_random` | pvp=0，home=(**0**,6) | **随机点**（`&&` 语义：缺一维即随机；写成 `\|\|` 会落精确 (0,6) 且 0 抽） |
| `…home_coords_zero_random` | pvp=0，home=(0,0) | 随机点（两抽） |

两组的来源写进 golden `ctx.note`（alt 例）：home 是"登录时 pvp<2 捕获的字段快照"、red 是"登录时
`Config.RedHome*` 快照"；并写明反例期望（误用 home ⇒ (5,6)）。

**牙（§31 五点表；每支单跑留原文）**：
| 变异 | 红在哪 | 预期落点/断言 |
| --- | --- | --- |
| M8 `if pvp >= 2` → `if pvp < 2` | `it11_red_home_branch_exact` 红、`it11_home_branch_exact` 红（分两次跑） | 红名支落 home(5,6)、home 支落 red(20,30) |
| M9 home/red 目标互换 | `it11_red_home_branch_exact` / `…alt_values` 红 | pvp=2 支落 **home_map(5,6)**；红在帧断言（634 尾 `EDCode("home_map")`）与终态 |
| M10 坐标门恒真（`if true`） | `it11_home_zero_coords_random` 红 | 落 (0,0) 且 0 抽（期望随机点 + 2 抽） |
| M11 `&&` → `\|\|` | `it11_home_x_zero_y_nonzero_random` 红 | 落精确 (0,6) 且 0 抽（期望随机点 + 2 抽） |
每次变异前 `touch` 强制重编；还原后 `sha256` 相等 + 基线六颗复绿。

**状态**：`implemented_unverified`（同 §9.12/§9.13：共用换图路径缺口 766/54 与 `[名|文件]`
见 [`openmir2-map-change-cluster-gaps.md`](openmir2-map-change-cluster-gaps.md)）。

**§33/§36 补记（2026-11-20，`card/items-move-09` 复审返工）**：
1. **lane 偶发（§33）**：IT-10 牙脚本首跑 9 项中 **2 项 MISMATCH（1 次）**，当时全机 **4 个 cargo** 在场；
   车道空后同脚本复跑 **9/9** 全绿（无改动）⇒ 基线/牙只在车道空（≤1 条、free ≥5G）时跑，偶发按「次数/总数」报。
2. **钟点依赖（§36）**：`ae` 11:00 跑 `cargo test --lib` = **2071 passed / 9 failed / 61 ignored**，9 红全在
   `it09_town_scroll_host_parity` 的帧序列断言（`assert_eq!(got, want)`），**差异仅
   `SM_CHANGEMAP(634).Series` 一个字段**（left 2 / right 0；其余帧逐字节相同）。
   根因：产品 `drain_map_change` 的 `DayBright()` 取**真实本地小时**（`enter::game_time_now()`），
   而夹具注册表的 `MapInfoFlag` 是 `default`（`day_light=false`）⇒ 只有 5–10/16–19/21 点映射到 0 才绿
   （当时绿是钟点巧合）。**修法（三条都做）**：
   - `eat_move.rs::registry_from` 的 `MapInfoFlag` 置 `day_light: true`（与探针夹具一致）；
   - **非白天钟点复跑**：11:10 跑 `--lib eat_move` = **11 passed / 0 failed**（含 IT-09/10/11 全部）；
   - **补注入时钟覆盖**：`drain_map_change` 拆出 `drain_map_change_at_time(..., game_time)` 注入缝，
     新用例 `map_change_634_day_bright_buckets` 断言 634 的 `Series` 三桶
     （`game_time 1→0 / 3→1 / 其它→2`，`DayLight⇒0`，`boDarkness⇒1`）；
   - **负对照**：删除 `day_light: true` 行后在 11:10 跑 `it09` ⇒ 红（634 `Series` 2 vs 0，与 ae 现象一致），
     还原 `sha256` 相等后复绿。⇒ 「避开依赖」不再无覆盖，「全绿」附了跑的时刻（11:10，非白天窗口）。
### 9.15 IT-17a 技能书 `ReadBook`（StdMode 4）— 宿主拦截 + 真身对拍（implemented_unverified）

**为什么拆 17a/17b**：`ReadBook` 的学习面（四道门 + `MagicList` + `SM_ADDMAGIC` + 两个原始串帧）
是一条物品行为的完整边界；而 `RecalcAbilitys` 的**技能被动**（`HitPoint/SpeedPoint/DC`、`MagicArr` 由
`MagicList` 填充后的战斗效果）是**能力层缺口**（`attackwire` 头注释：`MagicArr` 恒空）——并入会让本卡
一半判据不属于它。**17b 单独立卡**（[`openmir2-recalc-skill-passives-17b.md`](openmir2-recalc-skill-passives-17b.md)），本卡只认学习面。

**oracle 锚**：`ClientUseItems` 的 `case 4:`（`PlayObject.Operate.cs:632-654`）→ `ReadBook`
（`PlayObject.Base.cs:2171-2200`）：① `FindMagic(stdItem.Name)`（按**物品名**、忽略大小写、装表序首匹配）
② `!IsTrainingSkill`（`MagicList` 已有 `MagIdx`）③ `magic.Job == 99 || == (byte)Job` ④
`Abil.Level >= TrainLevel[0]`。三门 ②③④ 的**唯一实现**复用 `world-bevy::magic::can_read_book`
（本模块不复制第二份判定）。成功 ⇒ `MagicList.Add(UserMagic{Level=0,Key=0,TranPoint=0})` →
`RecalcAbilitys` → `SendAddMagic`（`SM_ADDMAGIC 0/0/0/1` + `EDCode(ClientMagic)`）→ case 4 尾段：
`SKILL_ERGUM(12)`/`SKILL_BANWOL(25)` 的 `UseThrusting/UseHalfMoon` 开关 + `SendSocket("+LNG"/"+WID")`
（**原始串帧**）+ 绿字 `SysMsg` → `WeightChanged()` → `SM_EAT_OK(635)`。**ReadBook 路径 0 抽**。

**实现**：
- `bins/game-svr/src/readbook.rs`：`MagicState`（`MagicList` 快照 + 两开关 + `Arc<MagicTable>`）+
  `maybe_read_book`（非书/失败 ⇒ 返回 false，交回 `use_item` ⇒ StdMode4 无臂落 636，与 oracle 失败支同帧；
  书**不受 `boNODRUG`**，只受 `dead`）。帧序：`210 → (+LNG/+WID) → 635 → 622 → 657 → 100`。
- `SessionGround.magic`：写点 = 登录（`magicaudit::filter_for_logon` + 进程级 `Arc` 魔法表 + 无文案）
  + 学成追加；`build_save_record` 以 `magicaudit::pad_for_save` 回写记录（学到即落库，重登不丢）。
- `smframe`：`sm_addmagic_with(level, def)` 参数化；`sm_addmagic` 包装保持 `AddItemSkill` 的 `Level=1`
  并有回归 `sm_addmagic_wrapper_keeps_additemskill_level`（读本的 `Level=0` 走 `_with(0, …)`）。
- `strings.rs`：`[String] ThrustingOn/HalfMoonOn`（缺键回落 `MessageSettings.cs:48/50` 的默认文案）；
  `handle_eat_cmd` 已从 master 带入 `&StringConf` 参数，本模块直接用，不再造会话文案快照。

**装置**：`mir2-rs/tests/parity/m3/readbook-probe`（**独立目录**，不动 eat/oil/move 探针）→
`mir2-rs/tests/parity/m3/fixtures/readbook_vectors.json`。`IWorldEngine` 用 `DispatchProxy` 只实现
`FindMagic(string|int)`；`RecordingRandom` 固定种子记录（本卡 0 抽自证）；帧记 **payload 全 hex**
（`ServerMessage` 20B 头之后：SM 帧 = 12B 命令头 + 尾；`+LNG`/`+WID` = 明文原始串帧）；golden 逐字段
记 `MagicInfo`（`ClientMagic.Def` 的来源）+ `MagicList/MagicArr/两开关/背包/Weight`。
生成（仓库根）：
`dotnet run --project mir2-rs/tests/parity/m3/readbook-probe -c Release -- mir2-rs/tests/parity/m3/fixtures/readbook_vectors.json`

**对拍（8 例；`it17a_read_book_host_parity` + 五颗单跑）**：
成功×3（刺杀 `+LNG` / 半月 `+WID` / 等级边界 20==20 可学）、门①表无名、门②已学、门③职业不符、
门④等级不足、门②+③同时不满足（两条失败路径帧相同 ⇒ **顺序不可观测**，只证各自可达）。
比较面：帧 `payload` **逐字节**（`Recog` 按夹具 ActorId 归一）、`MagicList` 终态（保序）、两开关、
背包件数、`Weight`、`rng_draw_count==0`。
**17b 边界**：`RecalcAbilitys` 的 657 只按**同值**发（`GetCharStatus()` + 当前 `HitSpeed`）；技能被动与
`WAbil` 重算不在本卡（探针记录值在无被动技能下与 oracle 相同）。

**牙（每支单跑留原文）**：M1 `is_training` 恒 false ⇒ 已学例红；M2 把 `can_read_book` 的 job 实参
换成 `magic.job`（职业门恒过）⇒ 职业例红；M3 等级 -1 ⇒ 边界例红（证 `>=`）；M4 不追加 MagicList ⇒
成功例红；M5 `sm_addmagic_with(0)`→`(1)` ⇒ 成功例 210 尾红，而 `sm_addmagic` 的 AddItemSkill 回归**仍绿**
（两条路径分离）；M6 删 `+LNG` 原始串帧 ⇒ 成功例帧序红。还原后 `sha256` 相等 + 基线七颗复绿
（变异脚本对 `copy2` 的旧 mtime 做 `utime`）。**首试不咬留痕**：M2 第一次把「玩家 job」改 99 **不咬**
（谓词是 `magic.job == 99 || magic.job == (byte)Job`，玩家 job=99 不改变判定）；换成上句的
`magic.job` 实参后才咬住——`readbook.rs` 的测试注释同记这一行。

**§33 补记（lane 偶发，回填 §9.13 的缺口）**：IT-10 牙脚本首跑时 9 项里 **2 项 MISMATCH（共 1 次）**，
当时**全机 4 个 cargo 在场**；车道空后同一脚本复跑 **9/9 全绿**（无改动）。⇒ 基线/牙只在车道空
（≤1 条在编、free ≥5G）时跑；报告偶发必须给「次数/总数」。该补记应在 `card/items-move-09` 签署完成后
以 follow-up 提交回填到 §9.13（避免在签审 head 上追加提交）。

**状态**：`implemented_unverified`（17a 学习面全绿；17b 技能被动未建模，见 §9.16/独立卡）。
EAD
### 9.17 IT-12 祝福油 Shape4（`WeaptonMakeLuck`）— verified（2026-11-20 升格）

**归类**：**StdMode 3 / Shape 4**。链路：`EatItems`（`PlayObject.Base.cs:2047`）→ `case 3`（`:2098`）→ `switch (stdItem.Shape)`（`:2099`）→ `default: EatUseItems(...)`（`:2162`）→ `case 4`（`:2259`）→ `WeaptonMakeLuck()`（`PlayObject.cs:1500`，原文拼写即 `Weapton`）。**有武器且 `Index > 0` 时恒吃**（函数恒 `return true`）；无武器 ⇒ 返回 false ⇒ **不吃**（636）。

**门**：`Envir.Flag.boNODRUG`（`src/M2Server/Player/PlayObject.Base.cs:2049-2053`，红字、不吃）；武器缺失（`PlayObject.cs:1502-1505`，不吃）。无等级/职业门。

**RNG（抽数随 `Desc` 变，必须逐值对拍）**：先抽 `Random(WeaponMakeUnLuckRate)`（`:1512`）==1 ⇒ **诅咒支**、不再抽；否则：
`Desc[4]>0` ⇒ 消诅咒（不抽）｜`Desc[3]<point1` ⇒ +1（不抽）｜`Desc[3]<point2` ⇒ 抽 `Random(nRand+point2_rate)`｜`Desc[3]<point3` ⇒ 抽 `Random(nRand*point3_rate)`（**上一条抽了没中时这一条还会再抽**）。`nRand = |HiByte(StdItem.DC) − LoByte(StdItem.DC)| / 5`（`:1509-1511`，整数除法）。抽数：诅咒支 1；`Desc4=0 且 Desc3∈[1,2]` 最多 3；`Desc3∈[3,6]` 最多 2；`Desc3≥7` 恰 1。

**帧（ident + 低字节泵序）**：直发 `SM_EAT_OK(635)`/`SM_EAT_FAIL(636)`；泵出 `RM_SUBABILITY`(10302→**62**，SM_SUBABILITY 752) < `RM_ABILITY`(10051→**67**，SM_ABILITY 52) < `RM_WEIGHTCHANGED`(10115→131) < `RM_SYSMESSAGE`(11004→252)。**oracle 先 `SendMsg(RM_ABILITY)` 后 `RM_SUBABILITY`（`:1546/1547`），但线序 752 在 52 之前**——宿主必须**显式按低字节排序**，不能靠发送序。通知：正常支绿字 `WeaptonMakeLuck`/`WeaptonNotMakeLuck`；诅咒支红字 `TheWeaponIsCursed`；`RM_ABILITY`/`RM_SUBABILITY` 两支都发（`Race==Play`）。

**状态量**：武器 `Desc[3]`（幸运）±1、`Desc[4]`（诅咒计数，上限 10）±1；`RecalcAbilitys()`。**无全局计数器**（与 IT-16 不同，无进程级共享状态）。

**配置（6 键，不写死）**：`WeaponMakeUnLuckRate=20`、`Point1=1`、`Point2=3`、`Point3=7`、`Point2Rate=6`、`Point3Rate=40`（`Server.conf:843-848`；`src/Modules/SystemModule/Conf/Model/GameSvrConf.cs:1879-1884` ctor 缺省同值；`src/Modules/SystemModule/Conf/ServerConf.cs:622-627`）。

**可观测面（章程 §26）**：核心效果在**状态**（`Desc[3]/Desc[4]`），帧面只有「绿/红字 + 两条能力帧 + 635/636」。⇒ 探针必须记 `weapon_before/after.desc[14]`；宿主从实体 `UseItemSlot.Desc[14]` 读写（已有），不新造存储。

**opc-19 2026-11-20 追加的四条**：
1. **`Random(0)` 跨语言**：.NET 实测 seed=1 ⇒ `Next(0)=0, Next(100)=11,46, Next(30000)=23148`；seed=20261120 ⇒ `0,63,32,11637`。Rust `RandomNumber` 必须返回同值**且后续序列相同**（含「消耗一次抽样」）。分歧就停下报，**不许** `if bound==0 { return 0 }` 抹平。
2. **泵序牙**：变异体对调 `RM_ABILITY`/`RM_SUBABILITY` 的**发送序**，线序**必须不变**（因按 `(byte)ident` 排序）；若红 ⇒ 线序断言其实是「跟随发送序」的假断言。
3. **`Desc[4]==10`** 单列一例且必须走**诅咒支**（`Desc[4]++` 上界不越界）——与 `Desc[4]--` 支方向相反。
4. 探针**新建 `m3/oil-probe`**（不动 `eat-probe`：`3b` 正在用它签 IT-16 第二批）。

**泵序（2026-11-20 实测修正，opc-19 三条要求）**：吃路径的通知排水改为 `NetPriorityQueue`（= .NET `PriorityQueue` 四叉最小堆的逐位复刻），入队序 = 发送序，出队即泵序。

#### 修正：Shape13 通知线序与 oracle 相反（2026-11-20，发现既有偏差）

- **旧行为**：Shape13 **非升级**支吃成功后，线上序为 `SM_WINEXP(44) → SM_WEIGHTCHANGED(622)`。
- **oracle 依据**：`MsgQueue` = `PriorityQueue<SendMessage, byte>`，键 = `(byte)wIdent`（`src/M2Server/Actor/BaseObject.Message.cs:181`）；夹具 `eat_ok_stdmode3_shape13_exp_nonlevelup` 的记录序 `[(10115,131),(8075,139)]` ⇒ **622 先于 44**。
- **影响面**：仅 Shape13 **非升级**支。升级支入队序本就是 `53(68)→622(131)→44(139)`（升序），`tests::it08_levelup_host_parity`（`lib.rs` 的「直发 635 先，泵出 53→622→44→45/52/752」逐帧断言）**不受影响、仍绿**。
- **为什么此前没暴露**：比较器按**七字段多重集**比（§3），不含顺序；且没有任何用例断言非升级支的线序 ⇒ 偏差不可见。
- **断言清单（§25 扫一遍，逐个点名）**：`eat_vectors.json` 两例（`..._nonlevelup` / `..._levelup`）记录的是 **oracle 序**，**未改动**；`tests::it08_levelup_host_parity` 未受影响；新增钉子 `tests::eat_notice_drain_matches_oracle_order_for_shape13`（入队 `[8075,10115]` ⇒ 出队 `[622,44]`）。
- **作废的说法（原文）**：「现有路径不受影响：`use_item` 产出的通知本来就是低字节升序且互不相同」——**假的**：Shape13 入队序是 `139→131`（降序）。
- **`(byte)ident` 升序在同优先级下不唯一**：同键 3 条入队 `[0,1,2]`、出队实测 **`[0,2,1]`**（sift 序）；`sort_by_key` 会给 `[0,1,2]` ⇒ **只能堆出队，不能排序**。
- **一条既有偏差被顺带修正**：`use_item` 对 Shape13 先 push `RM_WINEXP`(8075→139)、公共尾再 push `RM_WEIGHTCHANGED`(10115→131)，旧实现按入队序发 ⇒ 线上 44 在 622 之前；oracle 夹具记录的是 `[(10115,131),(8075,139)]`（622 先）⇒ 新排水后与 oracle 一致。**此前"现有路径不受影响"的说法作废**（那是假设，已被本例推翻）。
- **三条钉住用例**：`tests::net_priority_queue_equal_keys_dequeue_is_sift_order_not_insertion`（等键 `[0,2,1]`）、`tests::eat_notice_drain_matches_oracle_order_for_shape13`（622 先于 44）、`tests::it12_oil_frame_order_desc_and_always_eaten`（祝福油 `[635,752,52,622]` + `Desc[3]/[4]` 迁移 + 诅咒支上界）。
- **牙**：对调宿主里 `RM_ABILITY`/`RM_SUBABILITY` 的**发送序**，线序必须不变（由堆决定，不由 push 序决定）。

**第二批（探针/夹具/比较器/牙，2026-11-20）**
- **探针**：新建 `mir2-rs/tests/parity/m3/oil-probe`（不动 `eat-probe`），产物 `mir2-rs/tests/parity/m3/fixtures/oil_vectors.json`（12 例）。`weapon_before/after` 记**整套 `desc[14]`**（后续 shape 可复用同一 schema）；抽值用 `ScriptedRandom` 装进 `OpenMir2.RandomNumber.random`（= `WeaptonMakeLuck` 读的字段）；pump `RM_ABILITY`/`RM_SUBABILITY`/`RM_WEIGHTCHANGED` ⇒ 每例 `pumped=[752,52,622]`、直发 `[635]`。
- **Oracle 实测要点**：`oil_curse_desc4_at_ten_stays` 在 oracle 里**一条 sysmsg 都没有**（`Desc[3]=0` 且 `Desc[4]=10` ⇒ `MakeWeaponUnlock` 两支都不进）——这是"上界不越界"的最强证据；失败支 `oil_fail_no_weapon` = 636 + 零通知、`oil_fail_nodrug` = 636 + 恰一条红字。
- **新增缺口（登记，不静默；2026-11-20 已闭合，原口径保留作废留痕）**：所有"吃掉"例的 oracle 通知都含 `RM_CHARSTATUSCHANGED(10139)`（`RecalcAbilitys()` 的侧效应）；[作废]Rust 宿主未建模 10139（`notice_frame` 无映射、按 WRN 丢弃）——**已由 `card/charstatuschanged` 在 master 映射（657 臂）**，见 `docs/issues/openmir2-charstatuschanged-gap.md`（产生点/丢弃点/blast radius grep/复评清单）。**泵序键：`10139 & 0xFF = 155`** ⇒ 排 `622(131)` 之后、`252` 之前（`62 < 67 < 131 < 155 < 252`）。
- **比较器**：`wepmake_luck::tests::it12_oil_fixture_matches_pure_decision`（逐例 Desc[3]/[4] 迁移 + 消耗抽数）、`it12_oil_fail_cases_eat_and_frames`（636/通知/纯路径不吃）。
- **牙（2026-11-20 按章程 §31 五点重述；变异还原后 `touch`，见下方 mtime 教训）**：
  | 变异 | 变异行（原文） | 已重编 | 红在预期断言 | 还原 sha | 基线 |
  | --- | --- | --- | --- | --- | --- |
  | T1 诅咒阈值 | `if rng.random_below(cfg.un_luck_rate) == 2 {` | `Compiling game-svr` 在场 | `it12_oil_fixture_matches_pure_decision` + 宿主帧序例 | 相等 | 绿 |
  | T2 点1 比较 | `} else if i32::from(d3) <= cfg.point1 {` | 同上 | `oil_point2_hit: 消耗抽数必须与 oracle 相同（script=[0, 1]）` | 相等 | 绿 |
  | T3 Desc[4] 上界 | `} else if desc4 < 11 {` | 同上 | `it12_oil_fixture_matches_pure_decision` + 宿主帧序例 | 相等 | 绿 |
  | T4 去点3 抽 | `} else if i32::from(d3) < cfg.point3 {` | 同上 | `oil_point2_miss_point3_hit: 消耗抽数必须与 oracle 相同（script=[0, 0, 1]）` | 相等 | 绿 |

**升格（2026-11-20，card/items-b3）**：两条缺口都已闭合——
1. **`SysMsg` 文本**：`String.conf` 三键 `WeaptonMakeLuck`/`WeaptonNotMakeLuck`/`TheWeaponIsCursed` → `SysMsgTexts` → `NoticeText::Inline`；
   oil-probe 的 `Drain` 增 `text = TextHex(...)`（口径同 eat-probe：`EDCode.EncodeString(Buff)` 原始字节 hex），12 例重出后
   每例 11004 都带 `text`；比较器按 `res.outcome` 取同一条文案再编码，**逐字节相等**。
2. **`RM_CHARSTATUSCHANGED(10139)`**：已由 `card/charstatuschanged` 在 master 映射（`pickwire::notice_frame` 的 657 臂）。
- 顺带两条口径：`oil_curve_desc4_at_ten_stays`（`Desc[3]==0 && Desc[4]>=10`）oracle **零 sysmsg** ⇒ 宿主按
  `cursed_noop`（诅咒支且 `Desc` 未变）**不入队**红字，比较器按同一规则算期望条数；`oil_vectors.json` 的
  `config.exps_conf` 由绝对路径改回**文件名**（进仓可复现，`oracle_source_sha.py` MATCH）。
- **状态**：`verified`（12 例 oracle 夹具 + 比较器 + 四牙）。
  | 泵序对调 | `RM_ABILITY`/`RM_SUBABILITY` 两条 push 互换 | 同上 | **不红**（线序由堆决定，这正是要证的） | 相等 | 绿 |
- **过程留痕（教训）**：变异还原用 `shutil.copy2` 会**保留 mtime** ⇒ cargo 不重编 ⇒ 首轮全量 `2083 passed / 4 failed` 是**陈旧二进制的假红**；`touch` 源码后复跑 `2087 passed / 0 failed / 61 ignored`。后续变异脚本还原后必须 `touch` 或 `cargo clean -p game-svr`。**状态**：`implemented_unverified`（运行期 `SM_SYSMESSAGE` 文本帧缺口，同 IT-14/15/16）。

### 9.13 条目补记（A4：卡面此前无小节的 5 条）+ B3 单侧缺口清单

> 2026-11-20 清点（`a2`）：下列条目在卡面 grep=0（无小节），此处各补一段「能站住」的说明。
> 来源均为清单 `docs/issues/parity-gap-items.json` 的 `behavior`/`anchor`/`evidence` 与夹具实况。

- **IT-02 解锁药（Shape2）**：`UserUnLockDurg = true`，**无即时帧**；夹具 `eat_ok_shape2_unlock` 覆盖状态位与背包 -1（比较器含 `user_unlock_durg`）。
- **IT-04 入池（Shape default）**：`IncHealth += AC` / `IncSpell += MAC`（`ushort` 回绕）；两例 `eat_ok_shape0_inc_pool`（AC=15/MAC=7）、`eat_ok_shape0_inc_zero`（两侧 0）。
- **IT-05 食物（StdMode1）**：`HungerStatus += DuraMax/10`，上限 5000；跨档触发 `RefMyStatus → RecalcAbilitys`（尾段登记 T-17，`useitem.rs` 头部未做清单）。三例覆盖不跨档 / 跨档 / 夹取。
- **IT-06 StdMode2**：无任何效果、只吃掉；一例 `eat_ok_stdmode2_noop`（635 + 背包 -1）。
- **IT-19 解包（StdMode31 / AniCount!=0）**：脚本 `@StdModeFunc{AniCount}` 派发（`PlayObject.Operate.cs:668`）；两例覆盖 FunctionNPC 在场/缺失；脚本副作用在 `lib.rs` 的 `@StdModeFunc*` 接缝，本条目只判「吃/不吃」。

**B3 单侧缺口（待补用例，按 `a2` 清单；本次只登记、不改夹具——`eat-probe` 正被 IT-16 签署占用）**：
- **IT-01（Shape1 clamp）**：两例都是「两侧同时到顶 / 同时不顶」，**只 HP 顶而 MP 不顶（及反向）未覆盖**。
- **IT-03（Shape3 百分比）**：单例，同上**单侧截断/不截断**未覆盖。
- **IT-04（default 入池）**：两例是「AC>0 且 MAC>0」与「两侧 0」，**`if(AC>0)`/`if(MAC>0)` 的单侧分支未覆盖**。
> 补法：每条各加 1–2 例（HP 顶/MP 不顶、AC>0/MAC=0 及反向），重生成 `eat_vectors.json` 后逐例对拍；**须在 IT-16 签署结束、`eat-probe` 解冻后进行**，并同步 `counting_moment`。
### 9.18 IT-13 行会回城 Shape5（`EatUseItems` case 5）— oracle 事实 + 探针（C# 半批）

**归类**：**StdMode 3 / Shape 5**。链路：`EatItems`（`src/M2Server/Player/PlayObject.Base.cs:2047`）→ `case 3`（`:2098`）→ `switch (Shape)` 的 `default`（`:2162`）→ **`EatUseItems` case 5**（`src/M2Server/Player/PlayObject.Base.cs:2264`）。

**五支与"吃/不吃"（不对称，最易写错）**
| 条件 | 行 | 结果 | 吃？ | 抽 |
|---|---|---|---|---|
| `MyGuild == null` | `:2265` | 整段不做 | **否**（636） | 0 |
| `InGuildWarArea` | `:2267/2282` | 红字「此处无法使用」 | **否**（636） | 0 |
| 非战区、`IsCastleMember` 返 null | `:2269/2276` | 红字「无效」 | **是**（635，位置不变） | 0 |
| 非战区、城堡 `MasterGuild == MyGuild` | `:2270/2272` | `BaseObjectMove(home)` | **是**（635，位移） | **2**（先 X 后 Y） |
| 同上，但 `HomeX-4+draw == 0` 或 Y 侧为 0 | `BaseObjectMove:2312` | 走 `MapRandomMove` | **是** | 2 + MapRandomMove 的抽数 |

- **RNG**：`GetHomeX()` = `HomeX-4+Random(9)`（`src/M2Server/Castle/UserCastle.cs:649-652`）、`GetHomeY()` 同（`:654-657`）；实参从左到右 ⇒ 先 X 后 Y。`BaseObjectMove`（`src/M2Server/Player/PlayObject.Base.cs:2312`）在 `sX!=0 && sY!=0` 时 `SpaceMove`，否则 `MapRandomMove`。
- **帧/泵序**：直发 635/636；通知红字 `RM_SYSMESSAGE(11004→252)`、成功恒有 `RM_WEIGHTCHANGED(10115→131)` ⇒ `131 < 252`。位移帧走与 IT-09/10/11 **同一条换图路径**（本夹具实测 `pumped=[622,633,634,766,54,801]`）；**766/54 属该线已登记缺口**（见 `docs/issues/openmir2-map-change-cluster-gaps.md`，不在此重抄）。
- **状态**：位置（MapName/X/Y）与 `IsTimeRecall`；**行会/城堡只读**（`MyGuild`/归属/战区均不改写）。

**部署不可达（§18.1）**：`Castle.MasterGuild` 在部署态**恒 `null`**——唯一赋值点 `UserCastle.LoadConfig`（`src/M2Server/Castle/UserCastle.cs:228-232`）用 `GuildMgr.FindGuild(OwnGuild)`，而 `CastleMgr.Initialize` 早于行会装载（仓内 `castle.rs` 的 R-0 注释同口径）。⇒ **位移两支生产不可达**；生产只会走"不吃"或"无效但吃"。
- **构造路径**：夹具里**显式构造** `MasterGuild == 玩家行会` 的 `UserCastle`（`HomeMap/HomeX/HomeY` 可设），把 `CastleMgr.CastleList` 换成本探针的实例。
- **证据性质**：位移两支是 **synthetic 构造**证据，**不得**当作"生产路径已验证"。

**探针（C# 半批，已产）**：`mir2-rs/tests/parity/m3/guild-probe`（由 move-eat-probe 改造），产物 `mir2-rs/tests/parity/m3/fixtures/guild_vectors.json`，5 例：
`eat_fail_shape5_no_guild`（636/0 抽）｜`eat_fail_shape5_in_war_area`（636/红字/0 抽）｜`eat_ok_shape5_invalid_castle_eaten`（635/红字/0 抽/位置不变）｜`eat_ok_shape5_castle_home_exact`（635/move 全套/2 抽/落 `(30±4,40±4)`）｜`eat_ok_shape5_castle_home_zero_coord_random`（635/move 全套/4 抽/`MapRandomMove`）。
- **探针私有构造声明**：行会对象与城堡只在探针内构造，**不写回产品状态**；`SystemShare.GuildMgr` 在探针里是空 `GuildManager`。
- **§35 负向实测（2026-11-20，自检必须被证明会挡）**：`MIR2_GUILDPROBE_BREAK=<case>` 给该例注入一条伪帧 ⇒ 走**同一条真实断言**失败：
  `PROBE_SELFTEST_FAIL case=eat_ok_shape5_invalid_castle_eaten … 实得 im=[635,999]` ⇒ **rc=2**；
  同路径复跑前后夹具 sha 不变（`2cfcefc6…`）；换新输出路径 ⇒ rc=2 且**文件不存在**（不落盘）。

**Rust 侧（2026-11-20 落地，`eat_move::maybe_eat_move` 的 Shape5 臂；零改 `useitem.rs`）**
- 五支按 oracle `PlayObject.Base.cs:2264-2286`：`MyGuild==null` ⇒ 不吃（636，交回 `use_item`）；战区 ⇒ 红字「此处无法使用」不吃；
  非战区无城主会籍 ⇒ 红字「无效」但**吃**（635、位置不变、0 抽）；有效城堡 ⇒ `BaseObjectMove(HomeMap, HomeX-4+Rand9, HomeY-4+Rand9)`
  （**先 X 后 Y**，2 抽），任一坐标为 0 ⇒ `MapRandomMove`（+2 抽）。
- **Shape5 不发 `RM_SPACEMOVE_FIRE`**：oracle `case 5` 只调 `BaseObjectMove`（FIRE 只在 case1/2/3 显式发）⇒ 夹具 `pumped` 无 800；
  本实现按 `tpl.shape != 5` 门控（变异 M5 删门 ⇒ 位移两支多 800 红）。
- **宿主形参**：`handle_eat_cmd` / `maybe_eat_move` 各增 `castle_war_states: &HashMap<i32, CastleWarState>`
  （`PlayObject.InGuildWarArea` 的每 actor 会话位；生产调用点传 sim_loop 的真表，测试传 `&war_states`/空表）。
  红字颜色取 `MsgColors.red_f/red_b`（255/56）。
- **对拍（`eat_move::tests::it13_*`，6 条：总跑 + 逐支单跑）**：帧逐字段（含 `SM_SYSMESSAGE(100)` 文本尾
  `EDCode(gbk(文案))`）、终态 X/Y（实体+会话）、背包/重量、RNG 逐次重放 + 「无多抽」流位置等式；
  与 IT-09/10/11 的 `run_move_parity` 同构。
- **牙（五颗，逐个实跑；还原后 sha 与基线相同** `15b0b77a…`**）**：
  | 变异 | 变异行 | 红在 |
  | --- | --- | --- |
  | M1 战区判反 | `if in_war {` → `if !in_war {` | 5/6 例红（war/invalid/castle×2/总跑） |
  | M2 城主判反 | `master_guild == Some(guild)` → `!=` | 4 例红（invalid/castle×2/总跑） |
  | M3 去 `-4` | `i32::from(home_x) - 4 + dx` → `+ dx` | 3 例红（castle×2/总跑） |
  | M4 抽序对调 | 先 X 后 Y → 先 Y 后 X | 2 例红（零坐标支/总跑，终态 X/Y 与流位置） |
  | M5 删 FIRE 门 | `if tpl.shape != 5 {` → `if true {` | 3 例红（多一帧 800） |
- **部署不可达（§18.1）**：位移两支靠 `castle.master_guild = Some(玩家行会)` 的 synthetic 构造（部署态 `MasterGuild` 恒 null）
  ⇒ **该两支证据非生产路径**；生产只会走「不吃」或「无效但吃」。清单 `scope = castle_move_branches_synthetic_unreachable`。

**状态**：`implemented_unverified`（行为面：5 例 oracle 夹具 + 比较器 + 五牙；**不升格**——位移两支是 synthetic 路径，
且共用换图缺口 766/54 未清）。C# 半批与 Rust 半批同支 `card/items-it13`。
