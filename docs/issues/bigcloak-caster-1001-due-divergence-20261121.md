# bigcloak：caster `1001` 的 `due` 分歧（装置 800 vs Rust 900，2026-11-21）

> 触发：B4(ii) pilot 的 `big_cloak_assert` 断言 `caster` 的 657 `recog=1001` 必须在 `due_ms_rel=900`。
> 独立核对后发现：**oracle 装置（新旧两版）都给 800**，`900` 是 **Rust 侧自写的期望**，
> 且 bigcloak 的 `due` **从未跨栈比较过**（见 §2）。
>
> 本文只记录事实与判断：**未改 Rust、未改 golden、未改 pilot 断言**。

## 1. 三份 blob 与两次运行原文

| 角色 | blob | 说明 |
| --- | --- | --- |
| 旧装置（master `c99f7dd47`） | `48966e170c37363b8acaa1df8ebcad8c63cc5d9f` | `git rev-parse c99f7dd47:mir2-rs/tests/parity/csharp/MagicGolden/Program.cs`；取出后 `git hash-object` 复核同 sha |
| 新装置（本卡实现，分支 tip） | `3f34d9c04fc6bdafe92390865ca24bf0bdbfa251` | `HEAD:…Program.cs`；方向 A（每步每 actor 恰 Run 一次） |
| Rust 台账 | — | 由 `tests::big_cloak_parity_s1/s5/s13` 写入；值见下 |

**运行原文（同一 `--case bigcloak`，只取 caster 的 657 两对 `(recog, due_ms_rel)`）**

```
旧装置 c99f7dd47：
  s1  caster [(1001, 800), (1002, 800)]
  s5  caster [(2001, 800), (2002, 800)]
  s13 caster [(3001, 800), (3002, 800)]

新装置（方向 A）：
  s1  caster [(1001, 800), (1002, 900)]
  s5  caster [(2001, 800), (2002, 900)]
  s13 caster [(3001, 800), (3002, 900)]
```

**Rust 侧（台账 / 断言口径）**
- pilot 前实测台账（旧驱动 exe 跑出的 `bigcloak_s1`）：`caster [(1001, 900), (1002, 800)]`。
- pilot 提交 `dab72ec06` 的提交信息与断言：`caster (1002,800)→(1002,900)`、**`1001@900` 不变**。

⇒ 两条要分开看：
- **`1002`**：旧装置 800 → 新装置 900；pilot 期望 900 ⇒ **新模型与 pilot 一致，旧装置那条才是假绿**。
- **`1001`**：旧装置 800、新装置 800、Rust（pilot 前后）900 ⇒ **Rust 与 oracle 的既有分歧**，与本次模型无关。

## 2. 为什么从没被发现

1. **golden 根本没有这个字段**：`mir2-rs/tests/parity/golden/magic/bigcloak.json` 全文
   `due_ms_rel` 出现 **0 次**；`Receivers[*]` 只有 `Rm/Sent/Who`，`Sent[*]` 只有
   `ident/param/tag/series/tail`（`Rm 10120` 为 `p3=1002, delivery=0`）。
2. **比较器只在 golden 带该字段时才比 due**：`mir2-rs/tests/parity/tests/magic_fireball_parity.rs`
   的 `sent_fields.push("due_ms_rel")` 由
   `if ga.first().and_then(|f| f.get("due_ms_rel")).is_some() { … }` 守卫 ⇒ 对 bigcloak **从不比较 due**。
3. 于是 `caster 1001@900` 只是 **Rust 驱动内的自写期望**（`big_cloak_assert` 的 `due_of(…, 1001)`），
   自测自证，**没有任何跨栈证据**。

## 3. 两条结论（分开写）

- **结论 A（已证实）**：`caster 1002@900` —— 新装置（RM 入队层模型）重现了它；旧装置的 800 是多趟排水
  把同一 step 内 later pass 当成 tick 的产物。
- **结论 B（Rust 既有分歧，另立卡修）**：`caster 1001@800` —— 旧/新装置都给 800，Rust pilot 前后都是 900。
  **pilot 的"1001 保持不变"把 Rust 侧的值固化成了期望**；该值没有 oracle 依据。
  本条**不回退 pilot 的 1002 部分**，但需要另立卡修正 Rust 侧（或解释 oracle 为何会给 800）。

## 4. 待答问题：oracle 为什么是 800（需要 `src/` 证据才能定案）

已知的 oracle 链路（均为读代码，未跑）：

- 状态变更的产生：`src/M2Server/Actor/BaseObject.cs:1514`（`StatusChanged`）→
  `SendRefMsg(Messages.RM_CHARSTATUSCHANGED, 0, CharStatus, 0, 0, "")`。
- 广播的口径：`src/M2Server/Actor/BaseObject.Message.cs:383`（`SendRefMsg`）——对视野内每个 actor
  `SendMsg`（入**对方队列**）；隐身/`ObMode` 时退化为只给自己 `SendMsg`。
- 帧的落地：收件人下一次 `Run` 的 `GetMessage → Operate`
  （`src/M2Server/Player/PlayObject.Message.cs:360` 的泵），
  `RM_CHARSTATUSCHANGED` 分支（`src/M2Server/Player/PlayObject.Message.cs:1845`）里
  `SendDefMessage(SM_CHARSTATUSCHANGED, …)` ⇒ `SendSocket`。

**推断（待证）**：caster 自己的状态变更由 **caster 本步的 `Run`** 触发（例如隐身/伪装置位时的
`StatusChanged()`），`SendRefMsg` 把 RM 放进自己与视野内各 actor 的队列；caster 的 `while (GetMessage())`
会在**同一个 Run 内**继续处理自己那条 ⇒ 800；target/obs_in 在该步的后续 `Run` 处理 ⇒ 也是 800。
**Rust 侧为什么会是 900 未查**（pump 把自投递推到下一步？turn 边界？）——属 Rust 侧的另立卡内容。

## 5. 验收缺口

- **R3（`due` 记生产 step 的红证）未覆盖**：bigcloak 三支的审计行显示每一帧
  `enqueue_step == run_step`（例：`caster 657 recog=3002 enqueue 900 / run 900`）⇒ 该夹具里
  "due 记生产 step" 与正确实现不可区分。
- 补法：在 60 支装置 casing 中扫 `enqueue_step < run_step`（跨 step 入队）的夹具；找不到就构造最小夹具，
  否则明写「当前 60 支无此类，R3 不可覆盖」并进扩面验收缺口。
- 该项在 **sweep 窗口放行后**执行，不在本卡完成。
