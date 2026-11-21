# B17 诊断：bigcloak / groupdeding 的受害者出帧顺序（只读，2026-11-22）

> **只读诊断**：不改代码、不跑构建、不碰 golden。承接 B22 due 调查交接的 B 型 2 支（见
> `docs/issues/b22-due-representation-refactor-design.md` §0/§2/§7.4）。

## 0. 结论速览

- **是行为层（B17 排水/步进时序），不是比较器、不是装置 `recog` 分配。**
- **不能靠"排序比较"修**：`bigcloak` 的 `due` **多重集都不同**（golden `{800,800}` vs rust `{800,900}`），
  排序只会**掩盖**真实差异（opc-19 的告诫成立）。

## 1. 现象精确化

### bigcloak（label=s1，caster；golden=重出件 vs rust 台账）
- **golden(regen)** `Sent`: `638@1001@0, 642@500@0, **657@1001@800, 657@1002@800**, 640@19@1000`
- **rust ledger** `Sent`: `638@1001@0, 642@500@0, **657@1002@800, 657@1001@900**, 640@19@1000`
- 两帧 `657`（`RM_CHANGENAMECOLOR`）是**两个受害者**（recog 1001/1002）的改名色帧。
- **不只是顺序互换**：
  - due 多重集：golden `{800,800}`（两受害者同一步）、rust `{800,900}`（隔一步）；
  - "谁在 800"也变：golden 1001@800、rust 1002@800。
- 周围帧 `638/642/640` 不变；**只有这两帧 657**。

### groupdeding（label 系列 all_miss_hp0/hit_ac_split/…）
- regen `Sent` 的 `716` 帧（SM_716）**全部 recog=1001、due=0**——**本批数据里没有 recog 互换**。
- 当前（全量 plane，bigcloak/groupdeding 为 held golden）rust 台账的 `716` Sent **无 `due` 字段** ⇒
  **精确 due 差需对"重出 golden"重跑一次 rust 才能钉**（是 build/run，超出只读范围）。
- 结构性同族：716 是**等优先级广播**（prio 221）；groupdeding 比较器已有 `sort_sm_ident(716)`
  （**仅 groupdeding**），按 `(param,tag,series,tail)` 排、**不含 due/recog** ⇒ 只覆盖"**同 due 的 tie 序**"，
  **不覆盖"不同 due 的 stagger"**。

## 2. 三候选逐一排除

- **(a) 产品/排水出帧顺序（B17 排水步进）—— 最可能。** `bigcloak` 是 `StepDrainMs=100`。golden 在
  step=800 把**两个**到期受害者 rename 帧一起排（都 `due=800`）；rust 泵把第二个推到 step=900（`due=900`）。
  即"**某一步到期集合里排几条、按什么序**"——正是 B17 家族（oracle 逐 actor `Run` vs 产品批量/步进泵）。
- **(b) 装置 `recog` 分配 —— 排除。** regen golden 的 `recog`（1001/1002）来自受害者 actor id、确定；
  bigcloak 两帧 recog **集合=`{1001,1002}` = rust 集合**（只是"谁@800"不同）。recog 分配本身没错。
- **(c) 比较器多重集化不足 —— 排除，且不能靠它修。** bigcloak due 多重集 **`{800,800} ≠ {800,900}`** ⇒
  即使把 657 组按多重集比也**不相等**（不是顺序问题，是"**何时排第二个受害者**"的真实差异）。
  排序只会**掩盖**这个真差异（opc-19 告诫成立）。既有 716 sort 是给"**同 due** 的 tie 序"（prio 221
  堆出队 artifact）用的，**不适用于不同 due**。

## 3. 结论（走"产品顺序要改"这一支）

- **归属：B17（排水/步进时序），不是比较器/装置表示。** 修法在 B17 层——让 rust 泵与 oracle 对
  "某一步的到期集合与出帧序"一致（例如 rust 在 step=800 把两个到期受害者一起排，而不是推到 900）。
- **影响面**：多受害者且**受害者非同一 tick 排（stagger）**的 stem——`bigcloak`/`groupdeding`（本 2 支）。
  `amyounsul`/`groupamyounsul` 是 `same_tick`（已有 `multi_victim_same_tick` scope，顺序不判），**不同类**。
- **不该做的**：把 657/716 组改成"排序比较"——会掩盖 `{800,800}` vs `{800,900}` 的真差异。要放宽只能放宽
  **已验证同 due** 的 tie 序（既有 716 sort 的边界），**不能扩到不同 due**。
- **下一步（非本卡执行，需 build/run）**：B17 层做一次"某步到期集合 rust vs oracle 并列"探针，钉
  "rust 为何把一个受害者推到 900"（是 `delivery` 排期不同？还是 pump 每步只排一个就 step？）。

## 4. 机制判定（opc-19 探针；代码语义证）

- **pump 不是「每步只取一条就 advance」**：`SpellFrameRig::deliver(advance_ms)` 先 `clock_ms += advance_ms`，再**循环到队空/无到期**（`bound = queue_len+1`（min 64），反复 `deliver_self_delay_msgs()`）⇒ 每个 `deliver(100)` 把该步**所有 `DeliveryTime<=now` 的消息排尽**。所以 step=800 时「`delivery<=800` 的条数」是**全排**，不会取一条就 advance。
- ⇒ **候选 (ii)「pump 每步只取一条」——排除（代码证）**。**判定 = 候选 (i)：第二个受害者的触发消息（strike/rename）`DeliveryTime` 落在 `(800,900]`**（或它依赖的上游 strike 被排到 700 那步、rename 随之落 900）——**是排期/链路时序差，不是 pump 粒度**。
- **修法方向（不实施）**：B17 **排期层**——钉「两个受害者的 strike/rename 消息 `delivery` 为何差一步（600 vs 700 或 rename `+200` vs `+300`）」，让 oracle/rust 的「每步到期集合」一致。**在排期/`SendSelfDelayMsg` 链修，不在 pump、更不在比较器。**
- **严格 runtime 探针（下次，需再 build）**：给 `deliver_self_delay_msgs` 加每步 log（`ident`+`DeliveryTime`），跑 `big_cloak_parity_s1`，dump step=800 的到期集合，钉「第二条 `delivery` 到底几」。本次以代码语义钉到「非 pump 粒度、是排期」这一层。

## 5. Runtime probe 结果（2026-11-22 夜间）——**修正 §4 的读法错误**

给 `deliver_self_delay_msgs_impl` 加临时 `eprintln!`（每条 peek 打 `now/actor/ident/delivery/due`），跑 `big_cloak_parity_s1`（`--nocapture`），跑完**还原**（`git status` 干净）。日志 `e5-logs/b17_probe_run.log`。

**关键行（strike `8045`）**：
```
now=1200 actor=1001 ident=10123 deliv=1400 due=false   # 1001 队头是【未到期】的 640 消息
now=1200 actor=1002 ident=8045  deliv=1200 due=true    # 1002 队头是【到期】的 strike，本步排
now=1300 actor=1001 ident=8045  deliv=1200 due=true    # 1001 的 strike 晚一步（1300）才排
```

**三个数（opc-19 要的）**：
1. 两受害者的 strike(`8045`) `delivery` **相同 = 1200**（不是 800/900 不同）。
2. 不是不同上游 strike 派生——actor 1001/1002 **各一条 8045、同一 delivery**；657 由 strike **同步派生**（rename `10139` 不在 pump 队列，strike 处理时同步发帧）。
3. **oracle**：golden Rm `8045 delivery=1000800`（单条、相对 800）⇒ 两 657 都 @800。

**修正 §4**：我 §4 写「deliver 每步把所有到期消息排尽」——**错**。`deliver_self_delay_msgs_impl` 有 **per-actor「遇未到期队首即停（dequeue + enqueue(Lowest) + break）」**（B17 对齐生产 `GetMessage`）。一条**已到期**的消息若排在某 actor 队列的**未到期队首之后**，该步**不会被排**，要等下一步。

**判定（推翻 §3/§4 的「候选(i) delivery 排期差」）**：**不是 delivery，是 pump/队列序**——actor 1001 队头是未到期的 640(`10123`,deliv1400)，把后面到期的 strike(`8045`,deliv1200) 挡了一步。**正是 opc-19 给的「同 delivery 但只排一条 ⇒ 回 pump，立即报」分支。**

**修法方向（不实施）**：B17 **pump/队列序**——钉「**为什么 actor 1001 队头是 10123(640) 而 actor 1002 队头是 8045(strike)**」（两 victim actor 的入队序 / 四叉堆 tie 序不同？）。这决定同一 strike 是否被未到期队首挡住。**在 pump/入队序，不在 delivery 计算、更不在比较器。**

## 6. tie 序对照实验（2026-11-22 夜间）——arity 假设被数据否掉

probe（`SelfDelayQueue::enqueue` + pump peek 打 log，跑 `big_cloak_parity_s1`，跑完**还原 git 干净**；日志 `e5-logs/b17_probe2_run.log`）。

**入队序列（全部 `prio=255` 相同）**：
- actor `1001`: `[8045(seq0,deliv1200), 10123(seq2,deliv1400)]`
- actor `1002`: `[8045(seq1,deliv1200)]`  ← 只有 strike，无 10123

**arity 假设（4 叉 vs 二叉堆 tie 序）——被否**：`SelfDelayQueue` 只比 `priority`；enqueue sift-up `>=` 即停、dequeue sift-down `<=` 即停。**全等优先级下两者都不交换** ⇒ 二叉(.NET)与四叉(Rust) 在等优先级下**行为同构**：`swap_remove(0)` + 末元素移根、无重排。**arity 在等优先级下不起作用。**

**churn 模拟**（等优先级：dequeue=swap_remove(0)末→根、enqueue=append）：
- actor `1001` `[8045,10123]`：每次未到期队首 re-enqueue 后**交替** `[8045,10123]↔[10123,8045]`；
- actor `1002` `[8045]`：恒 head=8045。

⇒ 机制：2 元素等优先级队列的 swap churn 让「到期的 strike」与「未到期的 640」**每步交替出队首**；now 首次到 1200 那步若队首是 640 ⇒ strike 推后一步到 1300。此 churn **是 .NET-faithful** 的（Rust 已复刻 swap_remove+等停）。

**判定（回答分叉）**：**不是堆实现(arity)差异**（等优先级下两堆同构）。是**等优先级 swap_remove churn × actor 1001 多一条 640 形成 2 元素队列**；分歧 phase＝「strike vs 640 谁先入队」或「peek 次数 parity」——即**入队序/cadence**，不是堆实现、也不是纯「实现自由度」。

**still open（需 C# 侧/对照）**：oracle 侧自延迟队列的入队序/peek 次数无 log（C# 内部），无法从 golden 直接读；要钉 phase 差异落在「入队先后」还是「pump cadence」，需在 oracle 侧或 C# harness 的 `SendSelfDelayMsg`/`GetMessage` 加同类探针对照。**本卡停在诊断，不实施。**
