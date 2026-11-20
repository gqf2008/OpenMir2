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

### 9.9 IT-14 修复油 Shape9（`RepairWeapon`）— implemented_unverified

**关键归类订正**：`EatUseItems(case9=RepairWeapon)` 由 **StdMode 3** 的 `default` 调用（`PlayObject.Base.cs` 的 `switch(stdItem.StdMode) case 3: ... default: EatUseItems(shape)`），**不是 StdMode0**；StdMode0 的 `default` 是 AC/MAC 入池。故修复油 = **StdMode3 / Shape9**（初版误放 StdMode0 臂，已被对拍抓出并订正）。

**实现**（`use_item` 臂 `3 if tpl.shape == 9`，操作 `state.extra.weapon: Option<BagItem>`）：无武器 / `Index<=0` / `DuraMax<=Dura`(已满) ⇒ 不吃（`SM_EAT_FAIL`）；否则 `DuraMax -= (DuraMax-Dura)/RepairItemDecDura`、`nDura=min(5000, DuraMax-Dura)`、`nDura<=0` 不吃、否则 `Dura+=nDura`、发 `RM_DURACHANGE(10125)` + 绿色 `RM_SYSMESSAGE(11004)`（颜色参数），eaten。宿主把 `state.extra.weapon` 写回 `sg.use_items[SLOT_WEAPON]` 持久化。

**配置链路（C）**：`[Setup] RepairItemDecDura`（缺省30/现网90）→ 探针读 `SystemShare.Config.RepairItemDecDura` 写入夹具 `ctx.repair_item_dec_dura` → 比较器 `i32v(&c[...])` 读回 → `use_item` 用它作除数。oracle 与 Rust 除同一配置值，判定路径无写死。

**比较面（耐久必须进，req）**：夹具新增 `weapon_before`（顶层）+ `expect.weapon_after`；比较器 `state.extra.weapon = weapon_of(&case["weapon_before"])`，`use_item` 后 `assert_eq!(state.extra.weapon 的 index/dura/dura_max, weapon_after)`；另 `RM_DURACHANGE` 的 `n_param1=Dura/n_param2=DuraMax` 本就在 notices 多重集比对内。三支用例：`eat_ok_shape9_repair_normal`（5000/10000→9945/9945、`[635]`+`[10115,10125,11004]`）、`eat_fail_shape9_full`（10000/10000→636、不改）、`eat_fail_shape9_no_weapon`（null→636）。

**A/B 说明（机械同步面）**：`handle_eat_cmd` 新参 `repair_item_dec_dura` 12 调用点（主 `enter_cfg.repair_item_dec_dura`；11 测试传 `90`——`ctx.repair_item_dec_dura` 在 `use_item` **仅 `9 =>` 臂一个读点**，非 Shape9 用例永不到达 ⇒ 占位值不参与判定）。`UseItemContext` 15 字面量；其中比较器 `matches_oracle_eat_fixture` 与 `nodrug_guard_does_not_consume` 两处**从夹具 `c` 取 green/dec**（配置进比较面），其余静态 `0/0/90`。Z5 豁免名单 +2（`eat_fail_shape9_full`、`eat_fail_shape9_no_weapon`，纯失败一层）。

**运行期帧缺口（用户可见，非仅夹具）**：`SysMsg(WeaponRepairSuccess, Green)` → `SM_SYSMESSAGE` 文本帧，运行期宿主 `notice_frame` 用静态 `session.sys_msg` ⇒ 修复成功绿字**线上不发**（oracle 发）。同 StdMode1 food 口径，见 **`docs/issues/openmir2-runtime-sysmsg-text-frames.md`**。⇒ **IT-14 记 `implemented_unverified`，不计 verified**（待该卡实现文本帧后转 verified）。

### 9.10 IT-15 特修油 Shape10（`SuperRepairWeapon`）— implemented_unverified

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

**状态**：`implemented_unverified`（运行期文本帧缺口，同 IT-14 口径，不计 verified）。

### 9.11 IT-16 彩票 Shape11（`WinLottery`）— 开写口径（代码待窗口内写）

**归类**：**StdMode3 / Shape11**，经 `default: EatUseItems(shape)` 的 **case 11**（`PlayObject.Base.cs:2298`）→ `WinLottery()`（`PlayObject.cs:3809`）。**物品恒被吃**（`case 11: WinLottery(); result=true;`）。

**RNG 口径（P1，逐值对拍）**：`nRate = M2Share.RandomNumber.Random(Config.WinLotteryRate)`（`PlayObject.cs:3813`）。
- **同一实例的符号链（可核，4 段）**：①`M2Share.RandomNumber = RandomNumber.GetInstance()`（`M2Share.cs:194`）→ ②`OpenMir2.RandomNumber` 的**私有静态** `private static Random random`（`RandomNumber.cs`）→ ③`RandomNumber.Random(int)=random.Next(max)`→ ④魔法种子钩子 `mir2-rs/tests/parity/m3/rng-hook/StartupHook.cs` **文件头明写**：反射替换的正是这个 `RandomNumber.random`（换 `RecordingRandom(new Random(seed))`）。⇒ WinLottery 与魔法共用同一 RNG 实例。
- **caveat（重要）**：P1 的"同种子同序列"**只在 legacy `.NET Random` 算法口径下成立**（钩子把 `random` 换成定种子 `new Random(seed)`=legacy `CompatSeedImpl`/Knuth 减法）。**生产无参 `new Random()` 在 .NET6+ 是 xoshiro、非同序列**。**oracle 原版运行在 legacy 上 ⇒ legacy 即我们该对的目标；Rust `crates/shared/src/rng.rs` 复刻的正是它。** 别以为"任意 .NET 版本都行"。
- **P1 用例**：固定 `MIR2_RNG_SEED` ⇒ `nRate` 确定 ⇒ ①各级中奖各一（选使 `nRate` 落该段的 seed，`WinLottery{1..6}Min/Max` 区间有空档故可精细落点）；②未中（`nRate` 落空档）；③各 Min/Max 边界；④`Count<NoWinLotteryCount` **闸取假**（`nRate` 中奖但闸闭 ⇒ 表现为未中）。draws 逐值对拍（夹具 `rng` 台账 site+seq，Rust 同 seed 同 stream）。
- **⚠ 部署配置下 1 等奖为死代码**：1 级判据是 `WinLottery1Min + WinLottery1Max == nRate`（`PlayObject.cs:3859`，**和式等值**，不是区间），部署 `16180 + 16185 = 32365` ≥ `WinLotteryRate = 30000` ⇒ `nRate ∈ [0,30000)` **永远等不到**。⇒ **1 等奖用例必须覆盖 `WinLottery1Min/1Max`**（取 `5000/8000` ⇒ 和 `13000`；已核 `13000` 不落任一其它档 `[1,4999]/[14000,15999]/[16000,16149]/[16150,16169]/[16170,16179]`），并把它写进夹具 `config` 快照——**同时记原始值与覆盖值**（否则该用例不可复现，且后来人不知道它偏离了部署配置）。
- **未中档的 seed 空间（越宽 P1 越稳）**：`nRate = 0`（`6Min=1` 故 0 不算 6 档）、`[5000,13999]`、`[16180,29999]` 三段全落"未中"。
- **P2 稳健例一条**：配置强制桶（如 `6Min=0/6Max=Rate-1` 覆盖全区间 ⇒ 必中 6），断言只写行为/边界、不依赖抽值（兜底）。

**① 全局计数是进程级（必须复位）**：`WinLotteryCount/NoWinLotteryCount/WinLotteryLevel1..6` 在 `SystemShare.Config` **全局可变**，多 case 同进程**互相污染**。⇒ **探针每个 lottery case 前显式复位**（`WinLotteryCount=0`、`NoWinLotteryCount=初值`、`WinLotteryLevel1..6=0`）到**已知初值**再 `ClientUseItems`；**比较器断 delta 不断绝对值**：中奖 ⇒ `WinLotteryLevelN +1`、`gold += WinLotteryNGold`；未中/闸闭 ⇒ `NoWinLotteryCount +500`。Rust 宿主侧同口径 per-case 复位。（生产 `Server.conf` 两者=0 ⇒ `0<0` 闸初始**闭**；测"中奖"须把 `NoWinLotteryCount` 置>WinLotteryCount，如 500>0。）
- **启动初值来自配置，不是 0**：`GameApp.InitializeSystem()` 的 ctor 清零（`src/GameSrv/GameApp.cs:77-84`）发生在 `StartingAsync → GameShare.LoadConfig()` **之前**；`src/Modules/SystemModule/Conf/ServerConf.cs:645-652` 随后从 `[Setup]` 读这 8 个键**覆盖**清零值 ⇒ **有效初值 = 配置值**（部署全 0）。⇒ 这 8 个键必须一并解析进 `WinLotteryCfg`（见「配置同源」），否则「配 `NoWinLotteryCount>0` 才能中奖」——IT-16 的核心场景——在 Rust 侧**永远起不来**。探针的 per-case 复位值也取自这份配置，不写死 0。
- **默认配置下永不开奖**：`WinLotteryCount=NoWinLotteryCount=0 ⇒ 0<0` 为假 ⇒ **服务端默认状态永不中奖**——非 bug，是**玩法事实**：服主须配置开启，否则该物品在默认服里就是"花钱买空气"。别让后来者以为我们对拍漏了中奖分支。

**架构决议（作用域与所有权，`opc-07` 2026-11-20 批准）**：
- **`world` 是 per-World，不是进程单例**：今天进程只有 1 个 World（`lib.rs` 只 spawn 一条 `m2-sim` 线程，World 在 `sim_loop` 内建）；分片设计是「同进程多 World」（`docs/10k-shard-design-20261116.md` §0.1/§2.1、`docs/shard-runtime-20261116.md` §2「空片 = 永不接客的 World」、`shard.rs` `DEFAULT_SHARD_COUNT=1`）。oracle 这 8 个计数是**进程级静态** ⇒ 挂 `world` 会让 N>1 变成每区独立计数。
- ⇒ **可变计数放进程级共享**：新模块 `bins/game-svr/src/winlottery.rs` 的 `WinLotteryState { count, no_win, level: [i32;6] }` + `WinLotteryShared(Arc<Mutex<WinLotteryState>>)`；在 `run_inner`（紧挨 `ids_online`/`world_start`）创建，`Arc::clone` 进 `m2-sim` 闭包 → `sim_loop` 参数 → `handle_eat_cmd` 参数。**不用进程 static**（并行单测互相污染；static 只适合启动后只读的配置，如 `chat::install_send_ref_msg_range`）。
- **登记为分片原则的唯一显式例外**：设计件要求「World 间无共享可变状态」，而 oracle 该组计数本身就是进程全局 ⇒ 此处**显式共享**；RNG 仍走 `world.rng`（N>1 本就会被切开，与设计一致）。**不许**为"干净"改成 per-shard 语义。
- **分层**：纯函数 `win_lottery(rng, &cfg, &mut state) -> WinLotteryOutcome` 出 outcome；宿主负责加锁、写金、发帧。

**② RNG 钩子是新测量口径（非干扰须给证）**：IT-14/15 的 eat-probe 未用 `DOTNET_STARTUP_HOOKS`；IT-16 启用后要证**既有 31 例**不受影响（原写"28 例"是 IT-15 之前的数，作废）。
- **"夹具逐字节相同"字面做不到**：`eat_ok_stdmode31_unbind_success` 的 6 个 `added_items[].make_index` = `GetItemNumber()` = `ItemNumber + HUtil32.GetTickCount()`（`src/Modules/SystemModule/SystemShare.cs:449-455`），任两次运行必不同。⇒ 用**归一化 + 原始 diff 双存档**：
  1. `A1/A2` = 钩子关跑两次；`B1/B2` = 钩子开 + 两个不同 `MIR2_RNG_SEED`；
  2. **归一化清单只允许 `added_items[*].make_index`**（理由：墙钟派生、非行为字段）；置哨兵后逐字节比，**原始 diff 同档保存，不隐藏**；
  3. 判据顺序：`normalize(A1)==normalize(A2)`（噪声基线）→ `normalize(A1)==normalize(B1)`（钩子不改既有）→ **`B1` 与 `B2` 在 IT-16 新增 lottery 用例上必须不同**（nRate 随种子变）——这一条把"钩子没生效"与"钩子无影响"分开；
  4. 钩子安装证明取 stdout 的 `[RngSeedHook] 已注入种子 seed=…`（钩子失败 exit 97），不接受"没报错所以装了"。
- **grep 提示（仅接线提示，非行为证据，§12）**：eat 路径（Shape1/2/3/13/31、GetExp、`RepairWeapon`/`SuperRepairWeapon`、解包、StdMode1 食物）取值区**零 `RandomNumber` 调用**（仅 `WinLottery`@3813、未实现 Shape4@1512+、战斗 `GetAttackPower` 有抽）⇒ *提示*钩子无影响，**不作证据**。
- **冲突处置**：任何非 `make_index` 差异 ⇒ **停下报 `opc-07`**，不得拿 grep 洗白。

**配置同源（27 键，不写死）**：部署 `Server.conf` 实际 **27** 个 `WinLottery*` = **19 参数 + 8 状态初值**（原卡面"25 键"作废）。
- 19 参数：`WinLotteryRate` + `WinLottery{1..6}Min/Max` + `WinLottery{1..6}Gold`（部署：Rate=30000、6Min=1/6Max=4999…1Min=16180/1Max=16185、Gold 1000000…500；与 `GameSvrConf.cs` ctor 缺省逐键相同）。
- 8 初值：`WinLotteryCount`/`NoWinLotteryCount`/`WinLotteryLevel1..6`（`src/Modules/SystemModule/Conf/ServerConf.cs:645-652`；部署全 0）。
- 落点：`GateConfig` 新字段 `win_lottery: WinLotteryCfg`（`config.rs`，仿 `repair_item_dec_dura` 的 struct+parse+default）+ `EnterConfig` 字段 + `handle_eat_cmd` 按值传（全 i32 ⇒ `Copy`）。**同一结构**（`opc-07` 2026-11-20 批准）：oracle 就是同一批字段同一批读入，"配置即初值"是 oracle 事实。
- 结构：`rate: i32` / `min|max|gold: [i32;6]`（idx0=1 等奖…idx5=6 等奖，映射只在解析处做一次）/ `init_count` / `init_no_win` / `init_level: [i32;6]`（`init_` 前缀标"只做启动初值"；唯一入口 `init_state()`）。

**宿主 WinLottery**（`handle_eat_cmd`，shape11 `eaten` 后）：`world.rng.random_below(rate)` 抽 → 6 段匹配（判序 6→5→4→3→2→1，1 级为和式等值）+ `Count < NoWinLotteryCount` 闸 → 中：`WinLotteryLevelN++`、绿 SysMsg、`ground::inc_gold(gold, nGold, HumanMaxGold)` 成功才 `GoldChanged()`（`SM_GOLDCHANGED`），失败走 `DropGoldDown` bug-replica（**净金币不变且不发 `GoldChanged`**）；未中/闸闭：`NoWinLotteryCount += 500` + 红 SysMsg。
- **oracle bug，为 parity 复制**：绿字 SysMsg 在 `IncGold` **之前**发出 ⇒ `IncGold` 失败也照样先报"中奖"（`PlayObject.cs:3873-3897`）。**照抄**；将来若要修正玩法，须单独开卡并接受与 oracle 偏离。
- **帧序断言（不是注释）**：
  - 主断言：宿主中奖用例捕获物化通知序列，断言 `ident as u8` **严格升序**（`RM_GOLDCHANGED` 10136→248 < `RM_SYSMESSAGE` 11004→252），写成**性质断言** `windows(2).all(|w| w[0] < w[1])`（不钉两元素字面量，将来加帧不用改测试）；比较器仍按 §3 七字段**多重集**比（夹具每条的 `priority` 是其旁证）。
  - 次断言：捕获 `SimToGate::SendFrame`，断言 `idents == [SM_EAT_OK(635), SM_GOLDCHANGED(653)]`；`RM_SYSMESSAGE` 因 `wire.sys_msg: None` 落 `notice_frame → None` ⇒ **"登记丢弃"必须出现在证据里**（现有 WRN 行或计数字段），不能只靠"我们知道它会 None"。

**恒吃单列断言**：`use_item` 的 `3 if tpl.shape == 11` 臂**仅** `eaten = true`（无 RNG、无金币、无全局计数）；断言"物品恒被吃"**与中/不中解耦**（最易写错处）。

**牙（七类，全部只在变异体下红、还原后绿）**：
1. 区间边界改一（`>=`/`<=` 换边或 `Min/Max` 对调）；
2. 去 `IncGold`（或去 `GoldChanged`）；
3. 把 `eaten` 与中奖绑定（应解耦：未中/闸闭时该牙必红）；
4. 闸判定反转（`<` → `>=`）；
5. **运行时改读 `cfg.init_*`**：构造 `init_no_win=12345`、`state.no_win=0`，断言结果只随 `state` 变；
6. **`init_state()` 逐键映射**（漏映一个键必红）+ **1↔6 等奖键交换**（映射错位必红）；
7. **帧序对调**（宿主两条 push 顺序对调 ⇒ 主断言红）。

**状态**：`implemented_unverified`（运行期 `SM_SYSMESSAGE` 文本帧缺口：`WinLotteryNMsg`绿/`NotWinLotteryMsg`红，引用 `openmir2-runtime-sysmsg-text-frames.md`；该卡由 `opc-07` 加指 IT-16）。
- **follow-up（不并入 IT-16）**：`docs/issues/openmir2-lottery-ticket-gm-realtime.md`（master `50d290b8d`）——`@LotteryTicket` 现读 `GmConfig.win_lottery_*` 的 `Default` 常量快照（`gmcmd.rs`），IT-16 落地后必须改读实时 `WinLotteryShared`；交付里引用此卡。
