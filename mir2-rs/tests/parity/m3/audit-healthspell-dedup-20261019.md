# Lane B 审计：SM_HEALTHSPELLCHANGED(53) 同 tick 去重模型（只读）

基线：`83beb2a2e`（master）。本文件只读审计，不改仓库、不碰 `/e/tmp/aihit`、不碰 `smframe.rs` 既有函数。

核心结论：**现有 `superseded_healthspell` 模型方向反了**。oracle 的 `SendUpdateMsg` 只删除「队首」的同 ident 消息；恢复段（Inc 池）的 53 在消息段按优先级 68 先于客户端消息（CM_HIT=198）出队，因此升级段的 53 **不会**取代它；钳制段在 base.Run 末尾，属于另一个 drain 批次，也**不会**取代它。真正同批去重的是 (A) Inc 池恢复 vs ExtraAbil 到期，和 (C) HP/MP 填充/负体力扣血 vs 钳制。

---

## 1. oracle 队列语义（逐行）

### 1.1 SendUpdateMsg（`src/M2Server/Actor/BaseObject.Message.cs:283-305`）

```csharp
i = 0;
while (true) {
    if (MsgQueue.Count <= i) break;
    if (MsgQueue.TryPeek(out sendMessage, out _)) {
        if (sendMessage.wIdent == wIdent) { MsgQueue.TryDequeue(out _, out _); Dispose(sendMessage); }
    }
    i++;
}
SendMsg(this, wIdent, wParam, lParam1, lParam2, lParam3, sMsg);
```

关键事实（`PriorityQueue<SendMessage, byte>`，`ActorEntity.cs:16`，.NET 自带，最小优先级先出）：

1. `TryPeek` 永远只看**当前队首**（最小优先级元素），不是第 i 个元素。非队首的同 ident 消息**删不掉**。
2. 循环 `Count <= i` 的预算意味着：若队首连续有 N 条同 ident，最多删 `ceil(N/2)` 条（N=1 时恰删 1 条；正常「每 actor 至多一条待发 53」的情形是精确的）。
3. `SendMsg(this,...)` 入队优先级 = `(byte)wIdent`（`BaseObject.Message.cs:94`）。`RM_HEALTHSPELLCHANGED = 10052` ⇒ **68**。
4. 载荷不随消息保存；`PlayObject.Message.cs:1736-1739` 在**出队时**用当时的 `WAbil.HP/MP/MaxHP` 拼 `SM_HEALTHSPELLCHANGED`。所以「生产时快照」不是 oracle 语义。
5. 优先级字节小于 68 的待处理消息会挡住替换（见 §1.4）；延时消息用 `MessagePriority.Lowest=255`（`MessagePriority.cs:8`），**不会**挡住 53（`GetMessage` 在 `BaseObject.Message.cs:334-355` 弹出未到期延时消息后重入 255 并返回 false，停止本轮 drain；255 是最大字节，排在 53 之后）。

### 1.2 SendMsg / SendRefMsg / HealthSpellChanged

- `SendMsg(IActor baseObject, ...)`：`(byte)wIdent` 入队，`ActorId = baseObject.ActorId`（`:98-181`）。
- `SendRefMsg`（`:383-460`）：扫描 ±`SendRefMsgRange` 格，**含源 actor 自己**（源格 ObjList 含自身；`ObMode/FixedHideMode` 分支则只发自己）。收件人执行 `targetObject.SendMsg(this, wIdent, ...)` ⇒ 在**收件人**队列里入一条，`ActorId = 源`。收件人在**自己的出队时刻**读**源**的 `WAbil` 拼帧。
- `HealthSpellChanged()`（`BaseObject.cs:700-710`）：
  1. `Race == Play` ⇒ `SendUpdateMsg(53)`（只自己）；
  2. `ShowHp` ⇒ 再 `SendRefMsg(53)`（含自己 + 观察者）。
  ⇒ ShowHp 开启时，**每次** HealthSpellChanged 会给**自己**入两条 53（一条 SendUpdateMsg、一条 SendRefMsg 自发副本），且两条同 ident、同为 68。

### 1.3 actor 的 Run 分段（UserProcessor 每 50ms，`UserProcessor.cs:7`；本宿主 world tick=200ms）

一次 `PlayObject.Run()`（`PlayObject.Message.cs`）：

| 段 | 行 | 53 生产者 | 发送时机 |
|---|---|---|---|
| P1 恢复 | 70-141（`IncHealthSpell` 126 → `BaseObject.cs:974`） | IncHealth/IncSpell/IncHealing 池消费 | 同 Run 的消息段 P3 |
| P2 ExtraAbil 到期 | 163-227（226） | 任一槽到期 ⇒ `HealthSpellChanged()` | 同 Run 的 P3 |
| P3 消息段 | 358-366 `while (GetMessage) Operate` | `RM_MAGSTRUCK`(628)、DoSpell 扣蓝(1520)、GetExp 升级(164)、命中模式扣蓝、DisableStruck 分支(1597) | 在各自 Operate 之后、同一 drain 内（每次成为队首时发） |
| P4 地图旗标/饥饿 | 569-635（580/593/631） | DeHP / IncHP / 饥饿扣血 | 下一次 drain P6 |
| P5 复活 | 747-763（758） | 复活戒指满血 | P6 |
| P6 base.Run 消息段 | `BaseObject.Base.cs:26-30` | —（drain：把 P4/P5 的 pending 53 发出去） | 同 Run |
| P7 体力/魔力填充 | `BaseObject.Base.cs:44-69`（55/68） + 负体力扣血 `:105-115`（109） | HP 支 / MP 支 / 负值扣血 | **下一 Run 的 P3** |
| P8 钳制 | `BaseObject.Base.cs:223-245`（244，VerifyTick 30s 门） | HP>Max 或 MP>Max ⇒ 钳到 Max-1 | **下一 Run 的 P3** |

分组（本报告的核心抽象）：

- **G-A** = P1 ∪ P2（消息段之前入队，P3 发送）。
- **G-P3** = P3 内部各 Operate 产生的 53（同一 drain 内按出队时刻逐代发送）。
- **G-B** = P4 ∪ P5（P6 发送）。
- **G-C** = P7 ∪ P8（下一 Run 的 P3 发送）。
- **G-O** = 独立 50ms 线程的 buff 毒伤（`ActorBuffProcessor.cs:21` → `ActorBuffSystem.cs:255`），与上述任意 pending 竞争（并发，非单线程）。

### 1.4 关键推论：恢复 vs 升级/钳制是两条，不是一条

Queue 在 P1 后：`[recovery53(68), CM_HIT(198), ...]`。P3 的 `GetMessage` 按最小字节出队 ⇒ **recovery53(68) 先于 CM_HIT(198) 被发出**。随后 CM_HIT 的 Operate 才击杀 → GetExp（`PlayObject.Base.cs:149-166`）→ `HasLevelUp`（`PlayObject.cs:5242-5252`）→ `IncHealthSpell(2000,2000)` → 新的 53(68) 入队；它在下一次 `GetMessage` 作为队首被发出。⇒ **两条线上帧**，顺序 = 恢复（旧 WAbil）→ 升级（新 WAbil）。

同理，钳制在 P8（base.Run 末尾、P6 之后），与 P1/P3 的 53 不在同一个 drain 批次 ⇒ **不会**删除 P1/P3 已发出的 53；它与 P7 的填充/扣血同属 G-C，才会互相取代。

同批（同一队列、中间无 drain）才发生替换：
- G-A：P2 的 53 替换 P1 的 53（若 P1 的 53 在队首）。
- G-C：P7 的 HP 支 53、MP 支 53、负值扣血 53 依次互相替换，最后 P8 的钳制 53 替换前一条 ⇒ **一次 Run 恰一条**（无 ShowHp、无队首阻塞）。
- G-P3 内同一 Operate 的多个生产者（如命中模式扣蓝 → 击杀升级）也会互相替换（后者替换前者），跨 Operate 则通常各自成帧。

**跨 Run 的携带**：G-C 的 pending 53 在下一 Run 的 P3 才发；若下一 Run 的 P1/P2 先产生 53，则它被替换、**永不发出**。因此「每 actor 每 200ms 至多一条」也不成立；正确说法是「每 actor 每个 drain 前至多一条 **pending**，pending 被后续生产者替换，直到某个 drain 把它发出」。

**队首阻塞例外**（`SendUpdateMsg` 删不掉非队首）：待处理消息中优先级字节 <68 的会挡住替换，导致同一 ident 累积多条，drain 时全部发出。可达的 blocker：`AddMessage` 的 `MessagePriority.Normal=1`（`ActorManager.cs:67`，仅 `RM_UPDATEVIEWRANGE` 使用）、`RM_HIT=20`、`RM_BIGHIT=22`、`RM_HEAVYHIT=23`、`RM_MOVEFAIL=26`、`RM_LONGHIT=27`、`RM_FIREHIT=30`、`RM_CRSHIT=31`、`RM_DEATH=37`、`RM_CHANGEGUILDNAME=61`、`RM_SUBABILITY=62`、`RM_BUILDGUILD_OK=63`、`RM_BUILDGUILD_FAIL=64`、`RM_DONATE_OK=65`、`RM_LOGON/RM_DONATE_FAIL=66`、`RM_ABILITY=67`。注意 `RM_STRUCK=82`、`RM_MAGSTRUCK=108`、`RM_WINEXP=139`、`RM_LEVELUP=150` 都 >68，不挡；客户端消息 CM_HIT=198、CM_SPELL=201、CM_WALK=195、CM_EAT=238 也都 >68（`ProcessUserMessage` 用 `SendMsg(playObject, defMsg.Ident, ...)`）。

**ShowHp 多次调用的堆算术**：每次调用入 2 条（SendUpdateMsg + SendRefMsg 自发），下一次 SendUpdateMsg 只能删队首连续同 ident 的约一半 ⇒ pending 自发副本会稳定在 2-3 条（2→3→3…），而非每次恰 2 条。

---

## 2. 生产点枚举

### 2.1 oracle 侧（全部 `HealthSpellChanged()` 调用者 + 直接 SendRefMsg）

| # | oracle 行 | 段/组 | 触发 | 备注 |
|---|---|---|---|---|
| O1 | `BaseObject.Base.cs:55` | G-C/P7 | HP 自然填充 | `SendUpdateMsg` |
| O2 | `BaseObject.Base.cs:68` | G-C/P7 | MP 自然填充 | 紧接 O1，替换 O1 |
| O3 | `BaseObject.Base.cs:109` | G-C/P7 | `HealthTick < -HealthFillTime` 扣 1 HP | 与 O1 同 Run 互斥；与 O2 可同 Run |
| O4 | `BaseObject.Base.cs:244` | G-C/P8 | `HP>MaxHP || MP>MaxMP` 钳到 Max-1 | 30s VerifyTick 门；替换 P7 |
| O5 | `BaseObject.cs:793` | 随调用者（通常 G-P3） | `DamageHealth` 魔法盾扣 MP（先扣蓝、后扣 HP） | 随后调用者常再调一次（O8 等），后者替换前者 |
| O6 | `BaseObject.cs:974`（`IncHealthSpell`） | P1 或 G-P3 | 恢复池、GetExp 尾(2000,2000)、吃东西、治疗等 | 调用者决定组：P1 / GetExp(P3) / EatItems(P3) |
| O7 | `ActorBuffSystem.cs:255` | G-O | 绿毒 DoT（50ms 独立线程） | 跨线程，与任意 pending 竞争 |
| O8 | `BaseObject.Base.cs:628` | G-P3 | `RM_MAGSTRUCK` Operate：`StruckDamage` 后 `HealthSpellChanged` | 延迟消息到点 |
| O9 | `PlayObject.Message.cs:226` | G-A/P2 | ExtraAbil 任一槽到期（`boNeedRecalc`） | 先 SysMsg，后 53 |
| O10 | `PlayObject.Message.cs:580` | G-B/P4 | 地图 `boDECHPPOINT` 扣 HP | |
| O11 | `PlayObject.Message.cs:593` | G-B/P4 | 地图 `boINCHP` 加 HP | 紧接 O10，替换它 |
| O12 | `PlayObject.Message.cs:631` | G-B/P4 | 饥饿 `HungerDecHP` 扣 HP | |
| O13 | `PlayObject.Message.cs:758` | G-B/P5 | 复活戒指 `HP=MaxHP` | |
| O14 | `PlayObject.Base.cs:1520` | G-P3 | `DoSpell` 非战士技扣 MP（魔法包装层） | 先于 `MagicManager.DoSpell` |
| O15 | `PlayObject.Attack.cs:41/52/63` | G-P3 | 命中模式 5/8/12 扣 MP | 先于伤害/经验 |
| O16 | `PlayObject.Attack.cs:1179/1199/1245` | G-P3 | 烈火/攻杀/狂风斩扣 MP | |
| O17 | `PlayObject.Message.cs:1597` | G-P3 | `DisableStruck/DisableSelfStruck` 时 `SendRefMsg(RM_HEALTHSPELLCHANGED)` | 默认 false（`GameSvrConf.cs:1825-1826`），默认不可达 |
| O18 | `BaseObject.cs:708`（ShowHp 支） | 每次调用 | `SendRefMsg(53)` 自发 + 观察者 | 不是独立生产者，是每次调用的**副本倍增器**；观察者按自己出队时刻读源 WAbil |

### 2.2 Rust 侧（全部 `sm_healthspellchanged_frame` 发送点）

| # | Rust 位置 | 对应 | 当前去重 |
|---|---|---|---|
| R1 | `bins/game-svr/src/main.rs:10545 drain_stamina`（发帧 10593；逻辑 `stamina.rs`） | O1/O2/O3 | 逐 `changes` 快照各发一条；一次重放 Run 最多 2 条，200ms 内最多 8 条；无任何合并 |
| R2 | `main.rs:9769 drain_verify_health_changes`（发帧 9794） | O4 | 每事件一条；无与 R1 的合并 |
| R3 | `main.rs:10469 drain_recovery`（发帧 10506） | O6（P1） | 若 `superseded_healthspell` 命中则**整条不发** |
| R4 | `main.rs:10305 drain_levelup_healthspell`（发帧 10359） | O6（GetExp） | 每升级者一条；进 `superseded_healthspell` |
| R5 | `main.rs:9833 drain_buff_downlink`（发帧 9862） | O7（及 O5 的毒伤路径） | 每事件一条；无合并 |
| R6 | `main.rs:5801 drain_mag_struck_b2`（发帧 5828） | O8（O5 的盾支未记录） | 每受击玩家一条；无合并 |
| R7 | `main.rs:6324` CM_SPELL 默认支（延后发送 8555 / `emit_deferred_health` 10834） | O14 | 立即/延后一条；无合并 |
| R8 | `main.rs:12447 drain_player_revival`（发帧 12592） | O13 | 一条；无同组其他生产者（O10-O12 未接） |
| R9 | `main.rs:12724 drain_b5_self_deliveries`（发帧 12768） | O9 | 每到期事件一条；无与 R3 合并；末尾还会调用 R6 |
| R10 | `pickwire.rs:96`（生产 `useitem.rs:295/314`，CM_EAT 处理 `main.rs:4481` 起） | O6（EatItems） | 立即一条；在 world tick 之前发出，无共享 pending |
| R11 | `main.rs:9751 superseded_healthspell_ids`（用于 15014/15021） | — | 升级 ∪ 钳制 ⇒ 抑制 R3（见差异 D1/D2） |

未接的 oracle 生产者：O5（普通战斗的魔法盾 53；只有 `buff.rs:509-530` 的毒伤路径记录了盾支，且注明只记一次）、O10、O11、O12、O15、O16、O17。它们一旦接入，分别加入 G-B / G-P3，必须走同一 pending 模型。

---

## 3. 差异表

判定口径：「oracle 帧数」按同一 actor、同一批可见帧计；载荷均为 oracle 出队时读到的当前 `WAbil`。

| # | 场景 | oracle | Rust 现状 | 差异 | 级别/建议 |
|---|---|---|---|---|---|
| D1 | Inc 池恢复（P1）+ 本 Run 升级（GetExp） | **2 条**：恢复（旧值）→ 升级（新值）；恢复 53 在 CM_HIT 之前出队 | **1 条**（R3 被 `superseded` 抑制，只留升级） | 少一条，且丢失恢复段的中间载荷 | **Critical**；删除「升级抑制恢复」 |
| D2 | Inc 池恢复 + 钳制（P8） | **2 条**：恢复在 P3；钳制在下一 Run 的 P3 | **1 条**（R3 被抑制） | 少一条；阶段错判 | **Critical**；删除「钳制抑制恢复」 |
| D3 | Inc 池恢复 + ExtraAbil 到期（P1+P2） | **1 条**（P2 替换 P1；载荷 = 恢复后的当前 WAbil） | **2 条**（R3 与 R9 各发） | 多一条；载荷/顺序也偏（R9 在 R3 之前且读实体 hp，未含恢复增量） | **Important**；G-A 合并并在恢复状态落定后 flush |
| D4 | G-C 同一 Run 内 HP 支 + MP 支（甚至负值支） | **1 条**（后者替换前者；载荷 = 该 Run 末/出队时 WAbil） | 每个分支各 1 条；一次 Run 最多 2 条，200ms（4 个 Run）最多 8 条 | 多 1 倍以上；载荷用了「当次快照」（HP 支带未加 MP 的中间态） | **Critical**；每 Run 只保留最后一条，载荷取终态 |
| D5 | G-C 的填充/负值扣血 + 钳制 | **1 条**（钳制替换） | R2 + R1 各 1 条（≥2） | 多 1 条；且宿主顺序是「钳制先、填充后」，会把 `Max-1` 又填回 `Max`（状态也错） | **Critical**；P7 状态 → P8 钳制 → flush 一条 |
| D6 | G-C pending（Run N 尾）+ 下一 Run P1/P2 恢复 | **1 条**（P1 的 53 替换尚未发出的 G-C pending） | R1/R2 已在 tick N 立即发出，R3 在 tick N+1 再发一条 | 跨 tick 多一条（G-C 的帧本应在下一次 drain 前被替换掉） | **Important**；需要「跨 tick pending 槽」，或登记为 R 级窗口项 |
| D7 | 吃药的 53（P3）+ G-A 恢复 | **1 条**（同队列，后入者替换先入者，取决于队首） | R10 + R3 两条 | 多/少取决于时序；宿主无共享队列 | **Important**；纳入 P3/G-A 的 pending 模型 |
| D8 | 魔法扣蓝（P3）+ 同 Operate 的升级 | **1 条**（升级替换扣蓝；若被 <68 消息挡住则 2） | R7 与 R4 各自立即发（若命中模式扣蓝接入后会更明显） | 同 Operate 内应为一条 | **Important**（等 O15/O16 接入时） |
| D9 | `RM_MAGSTRUCK` 53（P3）+ ExtraAbil 到期（P2） | **1 条**（G-A 的 pending 被 P3 替换，前提是还排在队首） | R9（b5）与 R6 各一条 | 多一条 | **Info/登记**（依赖队首） |
| D10 | 毒伤（G-O）+ 任意 G-A/G-C | 可能合并成 1 条（毒伤 pending 被后续替换，或反之） | R5 + 其他各一条 | 时序相关，可能多一条 | **Info/登记**；毒伤在独立线程，需真机窗口 |
| D11 | 队首被 <68 消息挡住（RM_HIT/RM_DEATH/RM_ABILITY/RM_SUBABILITY/AddMessage=1 等） | 同一 ident 可累积 2+ 条 pending，drain 时全部发出；ShowHp 时更易累积 | Rust 固定按组一条 | 少发/多发取决于场景 | **Info/登记**（要完整队列才能精确） |
| D12 | ShowHp 开启，同一组内两次调用 | 自发 2 条/次，后续调用按 `ceil(N/2)` 删，pending 稳定在 2-3 条；观察者在自己出队时读源 WAbil | 每次调用固定自发 2 条（`broadcast_healthspellchanged` 含自己），观察者拿生产时 body | 次数/载荷都可能不同 | **Info/登记**；单次调用口径已一致 |
| D13 | G-B（复活）与 P4 地图/饥饿 | 同组，后入者替换 | O10-O12 未接 ⇒ 目前只有 R8，计数一致 | 接入后需合并 | **登记** |
| D14 | 出帧顺序（跨组） | 按优先级：53(68) 在 44(139)/45/52/752(150) 之前；SM_HIT(20) 在 53 之前；G-A 的 53 在 P3 帧之前；G-C 在下一次 drain | 宿主物理顺序：verify(14904) → buff(14940) → b4/b3/b5(14972-14983) → recovery(15015) → kill chain(53→44→45/52/752) → stamina(10593)。verify 在 recovery 之前、b5 在 recovery 之前，与 oracle 相反 | 跨组顺序不一致 | **Important**；用逻辑分组 flush 固定顺序 |

补充：R1 的注释/测试（`stamina.rs:349`、`main.rs:21849`）声称「HP 支/MP 支各一条、内容为当时快照」。按 §1.1/§1.3，这两点都不成立：同一 Run 内后者替换前者（一条），且载荷在出队时读当前 WAbil（终态），不是调用时快照。

---

## 4. 统一模型建议

### 4.1 结论：不要用「每 actor 每 tick 一条、取最后生产者」

成立的说法是：

> **每个 actor 在每个 drain 之前至多只有一个 pending 53；任何后续 HealthSpellChanged 在 pending 位于队首时替换它；pending 在下一个 drain 被发出（整条队列按优先级出队）。**

据此：
- 同组（G-A、G-B、G-C）内「最后生产者胜」；
- 跨组（G-A → G-P3 → G-B → G-C）是**不同批次**，各自成帧；
- 每个组内部还可能因 ShowHp 自发副本、队首阻塞、G-P3 的跨 Operate 多代而产生额外帧；
- 载荷永远在**发出时**读当前 `WAbil`，不是生产时快照。

### 4.2 宿主落地（最小可行）

引入每 actor 一个 pending 槽（不是 body）：

```text
struct HealthSpellPending { /* 仅标记 */ }
fn mark_healthspell(actor)          // 生产者只置位，不读 HP/MP、不发帧
fn flush_healthspell(group)         // 在组边界：对每个置位 actor 读当前 WAbil，发 1 条（ShowHp 再发 SendRefMsg 副本），清位
```

组边界与宿主现有 drain 的映射：

1. **G-A**：`drain_recovery` 只做状态（`recovery_tick`），然后 `drain_b5` 的 ExtraAbil 到期只做状态/SysMsg；**恢复必须先于到期**（oracle P1→P2）。在两者之后 flush 一次（载荷用**会话** `WAbil`，不是实体 `hp`，因为恢复只写会话）。当前 sim 顺序是 b5(14983) 在 recovery(15015) 之前，需要调整或用一个统一的 A 组函数重排。
2. **G-P3**：保留逐代 flush。`drain_levelup_healthspell`、`CM_SPELL`、`drain_mag_struck_b2` 等各自是一个「代」；同一 Operate 内的多个生产者（扣蓝 + 升级）应合并为最后一条。**不要**再用 `superseded` 去抑制 `drain_recovery`。
3. **G-B**：`drain_player_revival` 一条；O10-O12 接入后同组合并。
4. **G-C**：先跑 `stamina_tick` 的 4 个 50ms Run（每个 Run 内部只保留最后一条事件，载荷取该 Run 结束时的 WAbil），再应用记录下来的钳制（O4，钳制在 P7 之后），然后每个发生变化的 Run flush 一条；若无法按 Run 粒度发送，至少保证**每 Run 至多一条**、载荷取该 Run 终态。
   - 跨 tick：G-C 的 pending 在 oracle 下一 Run 的 P3 才发；若下一 tick 的 G-A 先产生 53，则 G-C 的 pending 应被替换。**精确实现需要把 G-C 的帧也放进 pending 槽、跨 tick 携带**；若不改架构，至少登记为 R 级差异（帧早发 0-200ms、且可能多发一条）。
5. **删除 `superseded_healthspell_ids`**（`main.rs:9751/10496/15014`）。它同时错在：升级/钳制都不能抑制 P1 恢复；而钳制真正要抑制的是 P7（`drain_stamina`），当前完全没接。
6. **状态顺序**：G-C 必须先 P7（填充/扣血）后 P8（钳制）。当前 `drain_verify_health_changes` 在 14904 先同步钳制值，`drain_stamina` 之后又写回实体；钳制后的 `MP=Max-1` 会被随后的 MP 填充抬回 `Max`。建议把钳制记录延迟到 `drain_stamina` 之后重新应用（会话 + 实体都要写）。
7. **ShowHp**：按「一次 HealthSpellChanged = SendUpdateMsg 1 条 + SendRefMsg 1 条（含自发）」建模；观察者的载荷在出队时读源 WAbil。多次调用的累积（`ceil(N/2)`）先登记，不做堆模拟。

### 4.3 若要做「完全等价」

唯一的完整解是给每个玩家建一个出站 `PriorityQueue<SendMessage, byte>`，所有 `SendMsg/SendUpdateMsg/SendRefMsg` 都入队，drain 时按 `(byte)wIdent` 出队、Operate 时读 WAbil。代价大、影响面广（AOI/战斗/聊天全部帧），不建议在本批做；建议用 §4.2 的 pending 槽 + 明确登记（队首阻塞、ShowHp 堆序、G-O 并发）换取可判定的计数/顺序等价。

---

## 5. 判据草案（构造级）与牙

以下均为**构造级**（不占真机窗口，除非标注 R）。每条都给出「改坏必红」的牙。

| 判据 | 构造 | 期望 | 牙（改坏） |
|---|---|---|---|
| C1 G-C 每 Run 一条 | 一个 50ms replay Run 内 HP<Max 且 MP<Max，`HealthTick>=HealthFillTime`、`SpellTick>=SpellFillTime` | 恰 **1** 条 53，载荷 = `(HP+fill, MP+fill)` 终态 | 改回逐分支发 ⇒ 2 条（现有 `stamina.rs:349` 就是这个错误期望） |
| C2 G-C 多 Run 计数 | 200ms tick 内 4 个 Run 都命中（构造 4 次跨阈值） | 恰 **4** 条，顺序 = Run 0..3，每条载荷为该 Run 终态 | 合并成 1 条/或每分支 8 条 ⇒ 红 |
| C3 钳制 vs 填充 | HP=Max+5（触发 O4）、MP<Max 且 SpellTick 到期；同 Run | 恰 **1** 条，载荷 MP=Max-1、HP=Max-1；会话与实体一致 | 不抑制 stamina ⇒ 2 条；钳制先于填充 ⇒ MP 被抬回 Max |
| C4 钳制 vs 负体力 | HP=Max+5 且 HealthTick<-FillTime；同 Run | 恰 **1** 条；状态 HP=Max-1（先 -1 再钳） | 钳制先应用 ⇒ HP 少 1/多一条帧 |
| C5 恢复 vs 升级 | Inc 池到期 + 本 tick 击杀升级（`exp_awards` leveled） | **2** 条：先恢复（升级前 HP/MP），后升级（IncHealthSpell 后） | 保留 `superseded` ⇒ 1 条（现有 `main.rs:25900` 需反转断言） |
| C6 恢复 vs 钳制 | Inc 池到期 + `verify_health_changes` 命中 | **2** 条：恢复（P3）→ 钳制（下一 drain） | 保留 verify 抑制 ⇒ 1 条（现有 `main.rs:40149` 需反转断言） |
| C7 恢复 vs ExtraAbil | Inc 池到期 + 同一 tick 一个 ExtraAbil 槽到期 | **1** 条，载荷 = 恢复后的会话 WAbil；顺序 = 53(68) → SysMsg(252) | 不合并 ⇒ 2 条；先发 SysMsg ⇒ 顺序红 |
| C8 ShowHp 单次 | `show_hp=true`，一次 HealthSpellChanged | 自己 **2** 条（SendUpdateMsg + SendRefMsg 自发），观察者各 1 条；同字节 | 去掉自发副本或去重成 1 条 ⇒ 红 |
| C9 队首阻塞（模型级） | 纯函数单测：队列含 `RM_HIT(20)` + pending 53(68)，再调 SendUpdateMsg(53) | 旧 53 **不被删**；新建后 pending=2；drain 时 2 条 | 把「只 peek 队首」改成「全队列扫描」⇒ 红（这会把 oracle 的 bug 修掉） |
| C10 优先级顺序 | 同批出现 53、44、45/52/752、SM_HIT(20) | 20 → 68 → 139 → 150（45/52/752 在 150 的 Operate 内直发） | 按入队序发 ⇒ 红（现有 `main.rs:21929` 的口径可复用） |
| C11 G-A 顺序 | recovery 与 ExtraAbil 同 tick | 53 先于 SysMsg；载荷含恢复增量 | b5 先于 recovery 发 ⇒ 载荷/顺序红 |
| C12 跨 tick 替换（R） | tick N 的 G-C pending + tick N+1 的 Inc 池恢复 | oracle 只发 1 条（tick N+1 的恢复）；若宿主立即发 G-C 则 2 条 | 需要真机/网关抓包窗口；登记为 R 级待裁决 |

需要同步修改的既有用例：
- `bins/game-svr/src/stamina.rs:349` `both_branches_in_one_run_produce_two_distinct_change_snapshots` → 改为「一次 Run 一条、终态」。
- `bins/game-svr/src/main.rs:25900` `recovery_healthspell_is_superseded_by_levelup_in_same_tick` → 改为「恢复 + 升级两条」。
- `bins/game-svr/src/main.rs:40149` `clamp_healthspell_supersedes_recovery_in_same_tick` → 改为「恢复 + 钳制两条」，并新增「钳制取代 P7 填充/扣血一条」。
- `bins/game-svr/src/main.rs:21929` `tick_frame_order_puts_stamina_after_kill_exp_chain`：若该夹具的 Inc 池也命中，期望会多一条恢复 53；否则保持。
- `bins/game-svr/src/main.rs:21851` `drain_stamina_drops_one_hp_writes_back_and_emits_frame` 的单条负值支用例仍成立（单 Run 单事件）。

---

## 6. 与 oracle 的逐点核对（便于 review 复现）

1. `BaseObject.Message.cs:283-305`：`TryPeek` 只看队首 + `Count<=i` 预算（不是全扫描、不是删全部）。✔
2. `BaseObject.Message.cs:94`：优先级 = `(byte)wIdent`；`10052 & 0xFF = 68`。✔
3. `BaseObject.Message.cs:334-355`：`GetMessage` 按最小优先级出队；未到期延时消息重入 255 并停止本轮 drain；255 不挡 53。✔
4. `BaseObject.cs:700-710`：Play ⇒ SendUpdateMsg；ShowHp ⇒ 再 SendRefMsg（`SendRefMsg` 扫格含自己）。✔
5. `PlayObject.Message.cs:1736-1739`：载荷在 Operate（出队）时读 `WAbil`。✔
6. `PlayObject.Message.cs:70-141`（P1）+ `:358-366`（P3）：P1 的 53 在 P3 先于 CM_HIT 出队 ⇒ 升级不能替换它。✔
7. `BaseObject.Base.cs:26-30`（P6，base.Run 的 drain）+ `:44-69`（P7）+ `:105-115` + `:223-245`（P8）：G-C 在 P6 之后、下一 Run 的 P3 才发；P7 内部与 P8 互相替换。✔
8. `BaseObject.Base.cs:55/68/109/244` 与 `PlayObject.Message.cs:226`：这些才是同批去重的正确对象（G-C/G-A），当前 Rust 的 `superseded` 指错了对象。✔
9. `ActorBuffProcessor.cs:21` → `ActorBuffSystem.cs:255`：buff 在独立 50ms 线程，与 actor Run 共享（非线程安全的）`MsgQueue` ⇒ 毒伤 53 的替换关系是竞态，不能假设单线程顺序。✔
10. `GameSvrConf.cs:1825-1826`：`DisableStruck/DisableSelfStruck` 默认 false ⇒ O17 默认不可达。✔

---

## 7. 残留/登记清单

- 未接 oracle 生产者：O5（普通战斗魔法盾）、O10（地图 DECHP）、O11（地图 INCHP）、O12（饥饿扣血）、O15/O16（命中/技能扣蓝）、O17（DisableStruck）。接入时必须走 §4.2 的 pending 槽。
- 未建模：队首阻塞（<68 优先级消息）、ShowHp 多次调用的堆序累积、G-O 并发时序、观察者按自己出队时刻读源 WAbil。
- 本次审计为只读；未改 `src/**`、`sql/**`、`MirClient/`、`crates/protocol`、`crates/storage`，未起服务器/客户端，未碰 `/e/tmp/aihit` 与 `smframe.rs` 既有函数。


---

## 评审员更正（2026-10-19，**以本节为准**）

- **支持的核心**：`SendUpdateMsg` 只 `TryPeek` 队首 + `Count<=i` 预算（连续同 ident 最多删 `ceil(N/2)`）；
  优先级 `(byte)wIdent`（`RM_HEALTHSPELLCHANGED=10052 ⇒ 68`）；载荷在出队时读当前 `WAbil`；
  恢复(P1) 53 在 CM_HIT(198) 之前出队 ⇒ **恢复+升级 = 2 条**；**G-C（填充/负体力 vs 钳制）一 Run 恰 1 条**是本次最有价值的结论；
  现有 `stamina.rs:349` 断言两条中间态是错的。
- **D7 错误**：恢复 53(68) 会先于 CM_EAT(1006⇒238) 出队 ⇒ 吃药不可能替换恢复 ⇒ 差异是**顺序反转**（恢复→吃药），不是条数。
- **D9 错误**：P2 的 ExtraAbil 53(68) 同样先于 `RM_MAGSTRUCK`(8044⇒108) 出队 ⇒ oracle 2 条，Rust 也是 2 条且同序 ⇒ **不是差异**。
- **「只有 G-A/G-C」错误**：G-B = P4∪P5（P25/P26 地图段 + P32）同样是同批替换组 ⇒ 应为 **G-A、G-B、G-C 三组**。
- **C3 拆开**：HP>Max+MP<Max ⇒ HP=Max-1、MP=填充值；HP>Max+MP>Max ⇒ 两者 Max-1。
- **C6 必须同时断言顺序 + 两条载荷 + 终态 Max-1**，并要把钳制移到恢复之后（只删抑制仍会把 Max-1 抬回 Max）。
- **C2 构造**：必须显式给 `HealthFillTime/SpellFillTime<=2` 的配置，否则 4 个 Run 不会都命中。
- **C9 是纯函数语义钉子**，不算生产判据。
- **tier = R**（帧数/出队序/观察者时序 + HP/MP 载荷），需 `window_required: true` 或 owner 替代证据。
- **行号**：`clamp_healthspell_supersedes_recovery_in_same_tick` 在 `main.rs:40326`。
