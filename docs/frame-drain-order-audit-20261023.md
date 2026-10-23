# 帧排水顺序系统性审计（卡 `openmir2-frame-drain-order-audit`，tier=R）

- 日期：2026-10-23
- 分支 / 基线：`card/frame-drain-order` / master `f7a32c2fd`
- 写集：`mir2-rs/bins/game-svr/src/`、`mir2-rs/crates/world/src/`、`mir2-rs/crates/world/tests/party_vectors.rs`
  （必要扩集，见 §7）、`mir2-rs/tests/parity/whitelist.md`、`mir2-rs/tests/parity/T-RESERVATIONS.md`、`docs/`
- 登记号：**T-208**（预约 `tools/whitelist/next_t.py --card openmir2-frame-drain-order-audit`）

## 0. oracle 语义（本审计的唯一判据，不再重新推导）

| 事实 | 出处 |
|---|---|
| `MsgQueue` 是 **per-instance**：`protected readonly PriorityQueue<SendMessage, byte> MsgQueue;`，ctor 每实例 `new` | `src/M2Server/Actor/ActorEntity.cs:16`、`:21` |
| 入队优先级 = `(byte)wIdent`（`SendMsg`/`SendRefMsg`）；`SendPriorityMsg` 用 `(byte)MessagePriority`（`High=0`/`Normal=1`/`Low=2`/`Lowest=255`）；`AddMessage` 用 `Normal` | `src/M2Server/Actor/BaseObject.Message.cs:30`、`:181`、`:94`；`src/OpenMir2/Enums/MessagePriority.cs` |
| 出队 = `TryDequeue`（**一次一条**），`BaseObject.Run()` 用 `while (GetMessage(...))` 抽干（客户端路径还有 `HumanGetMsgTime` 预算） | `BaseObject.Message.cs:334`、`BaseObject.Base.cs:28`、`PlayObject.Message.cs:360` |
| .NET 8 `PriorityQueue` = **四叉最小堆**（`Arity=4`）⇒ **同优先级不是入队序**：3 条出队 `[0,2,1]`、4 条 `[0,3,2,1]`（dotnet 实跑 = 本仓 `NetPriorityQueue` 逐位一致） | `mir2-rs/crates/world/src/msgqueue.rs` 模块头 + `ties_dequeue_in_four_ary_heap_order` |
| **关键推论**：堆里混入别的优先级元素会改变同优先级元素的相对序 ⇒ 复刻时必须「每人一条队列」分组，不能把多个收件人的帧混进同一个堆 | 本卡 + pickwire 卡 `openmir2-pickwire-remaining-frames` 的 `GuildOfficial::flush` 判据 |

**模型边界（登记 T-208）**：本项目不建模整条观察者队列（跨 tick / 跨系统的 pending 消息），只复刻
**同一排水点的批内**入队序。因此本审计的判据是「批内序 = oracle 四叉堆序」；混合队列场景仍是
`not_verified`（R 级窗口项，见 §6）。

## 1. 全量台账（逐点判定）

「是否用堆」= 该点出队是否经 `NetPriorityQueue`（四叉堆）。「同 ident 多帧可达」= 同一收件人在
**同一次排水**里能否收到 ≥2 条同 ident 帧（同 ident ⇒ 同优先级）。**不可达必须给枚举证据**。

| # | 文件:位置（本 patch 后） | 排序键 / 机制 | 堆？ | 同 ident 多帧可达？ | 证据 / 结论 |
|---|---|---|---|---|---|
| 1 | `crates/world/src/msgqueue.rs`（`NetPriorityQueue` 本体；`bins/game-svr/src/msgqueue.rs` 变为再导出） | `u8` 优先级，四叉堆 | ✅ 本体 | — | `ties_dequeue_in_four_ary_heap_order` 钉 `[0,2,1]` / `[0,3,2,1]` |
| 2 | `lib.rs` `SpellFrameBuf::flush`（B2/B3/B4/B5 全部效果体块共用） | `(socket, priority, body)`，**每收件人**各自建堆 | ✅ **本卡修**（原 `frames.sort_by_key(|f| f.1)` 稳定排序） | **可达** | 一次施法多目标 ⇒ 同一观察者多条 `SM_STRUCK(31)`（167）/ 多条 `SM_CHANGENAMECOLOR(656)`（154）。构造判据 `spell_p3b2_lightflower_center_self_and_undead_division`（911 收 4 条 31）、`spell_p3b2_fireboom_delivery_ledger_and_frames`（912 收 4 条 31） |
| 3 | `lib.rs` `emit_script_frames_raw` | `&SendMsg`，单收件人（本人） | ✅ 已有（pickwire） | **可达**（`RM_DURACHANGE` 每件装备一条） | 三处入口共用：商人 `emit_script_frames` / `emit_login_script_frames` / `run_party_goto_label`；判据 `script_drain_orders_same_priority_by_dotnet_heap` |
| 4 | `lib.rs` `drain_slave_mutiny_downlink` | `MutinyFrame`，按观察者分组 | ✅ 已有 | **可达**（N 只宝宝 × 色/名） | 观察者 `MsgQueue` 每只宝宝 `RefNameColor(); RefShowName();` |
| 5 | `lib.rs` `CM_ADJUST_BONUS` 成功支队列 | `NetPriorityQueue<u16>` + `send_update_msg` | ✅ 已有 | 不可达（657 头-only 去重后净 1 条） | 该支注释 I-2 + `msgqueue::tests::c9_send_update_msg_peeks_head_only` |
| 6 | `bins/game-svr/src/healthspell.rs` `HealthSpellSlots` | 每 actor 一条 `NetPriorityQueue<u16>` | ✅ 已有 | 可达 | `send_update_msg` 头-only 去重模型 |
| 7 | `bins/game-svr/src/run_frames.rs` `RunFrames` | 每 actor 一条 `NetPriorityQueue<GroupFrame>` | ✅ 已有 | 可达 | `flush_orders_by_rm_ident_low_byte` 等 |
| 8 | `bins/game-svr/src/guild_official.rs` `flush` | `QueuedFrame`，**按 `actor_id` 分组**后各自建堆 | ✅ 已有（pickwire 修） | 可达 | `flush_drains_per_recipient_heap` |
| 9 | `bins/game-svr/src/guild_official.rs` `build_queued_frames` | `out.sort_by_key(|(p, _)| *p)`（稳定） | ⚪ 稳定排序 | **不可达** | 该批优先级字节**两两不同**：`PRI_CHANGEGUILDNAME`=61、`PRI_BUILDGUILD_OK/FAIL`=63（互斥）、`PRI_GOLDCHANGED`=152 ⇒ 稳定序 = 堆序。函数头注释已订正 |
| 10 | `bins/game-svr/src/npc.rs` `dequeue_order`（说词） | 整批灌堆（`High=0` 组 + `SendMsg=1` 组） | ✅ **本卡修**（原「priority 组 ++ 非 priority 组」= 稳定退化） | **可达** | 部署 `E:/MirServer/M2GameSvr/Envir/Market_Def/功能NPC/桃源-N109.txt`：`@10cang` 等标签有 **9 条连续 `SendMsg`**（行 139-147 / 194-202 / 249-257）⇒ 同一玩家一次收 9 条 `RM_MERCHANTSAY`。判据 `dequeue_order_same_priority_follows_four_ary_heap`、`dequeue_order_puts_priority_high_first` |
| 11 | `bins/game-svr/src/trade.rs` `Emitter::finish` | `QueuedEvent`，**按 target 分组**后各自建堆 | ✅ **本卡修**（原 `sort_by_key(|q| q.priority)`） | **可达** | `deal_success` 四个前置检查各发一条 `red_msg`（`RM_SYSMESSAGE`=252）给同一 `me.id`，`deal_cancel_em` 再叠。判据 `emitter_finish_same_priority_follows_four_ary_heap` 等 |
| 12 | `bins/game-svr/src/selloff.rs` `Emitter::finish` | `QueuedFrame`，**按 target 分组**后各自建堆 | ✅ **本卡修**（原 `sort_by_key(|(p, _, _)| *p)`） | **可达** | `client_cancel_sell_off_ing` 每取消一条寄售各一条 `menu_ok`（69，同一 `me.id`）；`client_buy_sell_off_item` 每颗金刚石各一条 `blue_msg`（252） |
| 13 | `crates/world/src/party.rs` `Emitter::finish` | `(u8, PartyEvent)`，**按 `PartyEvent::target()` 分组**后各自建堆 | ✅ **本卡修**（原 `sort_by_key(|(p, _)| *p)`） | **可达** | 团长死亡散组（`party_del_member_em` 的 `owner == target` 支）逐成员 `LeaveGroup()` ⇒ 同一成员一次收多条 `RM_GROUPMESSAGE`(253)。判据 `emitter_finish_same_priority_follows_four_ary_heap` |
| 14 | `bins/game-svr/src/ground.rs` `dequeue_order`（拾取/丢弃 self notice） | `sort_by_key(|n| (n.ident as u8) as u16)`（稳定） | ⚪ 稳定排序 | **不可达** | 枚举：`pick_up` 的 `notices` 只在三条**互斥且立即 return** 的分支里产生 ⇒ 可达集合 `{SYSMESSAGE 252}` / `{GOLDCHANGED 152}` / `{ITEMHIDE 149, WEIGHTCHANGED 131}`；`drop_item` 只有 `{MENU_OK 69}` / `{WEIGHTCHANGED 131}`；`update()` 已按同 ident 去重。组合内优先级字节两两不同 ⇒ 稳定序 = 堆序（模块头与函数注释已写清） |
| 15 | `bins/game-svr/src/useitem.rs` `use_item` 尾 | `sort_by_key(|n| n.ident as u8)`（稳定） | ⚪ 稳定排序 | **不可达** | 枚举：`notices` 只在四条互斥分支产生 —— 禁药 `SYSMESSAGE`(252)（`eaten=false`）/ `CHARSTATUSCHANGED`(155) + `MYSTATUS`(166)（成对）/ `MENU_OK`(69) / `WEIGHTCHANGED`(131)。组合内两两不同（注释已写清） |
| 16 | `bins/game-svr/src/guild_cmd.rs` `Ctx::flush(Vec<Deferred>)` | 无排序 = 插入序 | ⚪ 无排序 | **不可达** | 每个分支对同一 recipient 至多各推 750(61)/42(165)/100(252) 一条，且插入序已升序 ⇒ 与堆序一致 |
| 17 | `lib.rs` `emit_def_frames` / `emit_shop_frames` | 无排序 = 源码序 | — | 不适用 | **`SendDefMessage` 直发**（不进 `MsgQueue`）⇒ oracle 同样按源码序写 socket |
| 18 | `crates/world/src/world.rs` `deliver_mag_struck`（`msgs.sort_by_key(|m| m.order)`） | 消息创建序 | ⚪ 稳定排序 | 本卡**不改** | 不是帧排水，是 `Operate(RM_MAGSTRUCK)` 的**交付队列**（跨 actor 处理序 + RNG 抽数序）；oracle 每人一条队列、由 `ProcessMonsters` 的目标序泵出，跨 actor 序已登记 `not_verified`（合同 §1.6）。同 target 重复只在「同 tick 多施法者命中同一目标」出现，属同一登记面 |
| 19 | `lib.rs` `observers.sort_unstable()`（叛变帧收件人集）、`chat.rs` `hear_targets`/`cry_targets` 的 `out.sort_unstable()` | 收件人**集合**排序 | — | 不适用 | 决定「谁收」而非「同一人的帧序」；每个收件人内部的帧序由 #4 的堆决定 |
| 20 | `lib.rs` 多处 `names.sort()` / `keys.sort()` / `socks.sort_unstable()` | `HashMap`/`HashSet` 迭代确定性 | — | 不适用 | 遍历序，非帧排水序（多数还在 `#[cfg(test)]` 断言里） |

**结论**：全仓「按优先级/ident 排序后发出」的排水点共 20 处；其中 **5 处**在批内存在可达的同 ident
多帧（#2/#10/#11/#12/#13），**全部**改为 `NetPriorityQueue`（需要区分收件人的一律先分组）；其余
「稳定排序」站点均已给出**不可达枚举证据**并订正注释；已有堆实现的 6 处（#3–#8）保持不变。

## 2. 本 patch 的实修

1. `crates/world/src/msgqueue.rs`（**下沉**）：把 `NetPriorityQueue` + `send_update_msg` 从
   `bins/game-svr` 移到 `crates/world`（`game_svr::msgqueue` 变成 `pub use mir2_world::msgqueue::*`），
   让 `crates/world` 的排水点（`party.rs`）与宿主用**同一份**堆，而不是各处手写。
   两处 dotnet 实跑用例随模块迁移；`(byte)RM_ABILITY=67` / `RM_SUBABILITY=62` 的协议常量交叉核对留在
   `bins/game-svr/src/msgqueue.rs`（world 不依赖 `mir2-protocol`）。
2. `SpellFrameBuf::flush`：全局稳定排序 → **按 socket 分组 + 每组四叉堆出队**。
3. `npc::dequeue_order`：两组拼接 → **整批灌堆**。
4. `trade::Emitter::finish` / `selloff::Emitter::finish` / `party::Emitter::finish`：
   全局稳定排序 → **按收件人分组 + 每组四叉堆出队**（`DealEvent` / `SellOffFrame` / `PartyEvent`
   各加一个 `target` 提取器）。
5. 注释订正（逐处点名）：
   - `guild_official.rs` 模块头「`sort_by_key` 是稳定排序 ⇒ 同优先级保持入队序」——**推理不成立**
     （oracle 四叉堆不保同优先级入队序），改为「该批优先级字节两两不同 ⇒ 稳定序 = 堆序」；
   - `ground.rs` 模块头与 `dequeue_order`：补「同 ident 不可达」的枚举证据；
   - `useitem.rs`：同上；
   - `trade.rs` / `selloff.rs` 模块头与 `later()`：「同优先级 FIFO」→ 四叉堆 sift 序；
   - `lib.rs` 五处效果体注释「按优先级稳定排序」→「按每收件人各自的四叉堆出队序」；
   - `npc.rs::dequeue_order` 文档：「组内取入队序」→ 堆序（并点名部署里 9 条连续 `SendMsg`）。

## 3. 受影响的既有判据（期望值订正，逐条给理由）

| 用例 | 旧期望 | 新期望 | 理由 |
|---|---|---|---|
| `npc::tests::dequeue_order_puts_priority_high_first` | `[b,d,a,c]` | `[b,d,c,a]` | 整批 `[a(1),b(0),c(1),d(0)]` 灌堆后出队 `b,d,c,a`；旧值来自「两组各自稳定」的退化 |
| `spell_p3b2_lightflower_center_self_and_undead_division`（911 的 4 条 31） | `[m2,m1,m3,p1]` | `[m3,m1,m2,p1]` | 批 = `638(136)` + 逐目标 `31(167)`/`656(154)`；四叉堆出队序（已用 `NetPriorityQueue` 逐位复核） |
| `spell_p3b2_fireboom_delivery_ledger_and_frames`（912 的 4 条 31） | `[m1,m2,p1,m3]` | `[p1,m2,m3,m1]` | 同上 |
| `trade::tests::deal_end_ok_time_gate_cancels` | 跨收件人交错 `(1,2,1)` | 分组 `(1,1,2)` | oracle 每人一条队列，跨收件人平坦序不是契约 |
| `tests::die_triggers_del_member_frames_on_production_path` | 平坦 5 帧序 | 集合 + 逐 socket 序 | 同上 |
| `tests::party_cm_dispatch_emits_oracle_frames_on_both_connections` | 平坦 7/6 帧序 | 逐 socket 序（`drain` 改为带 socket） | 同上 |
| `crates/world/tests/party_vectors.rs` 4 例 | 平坦事件序 | 逐 `target` 事件序 | 同上 |

## 4. 新增构造级判据

| 模块 | 用例 | 钉住 |
|---|---|---|
| `msgqueue`（world） | `ties_dequeue_in_four_ary_heap_order` | 3 条 `[0,2,1]`、4 条 `[0,3,2,1]` |
| `msgqueue`（game-svr） | `protocol_constants_match_world_literals` | `(byte)RM_*` 字面量与协议常量一致 |
| `lib.rs` `SpellFrameBuf` | `same_priority_frames_follow_dotnet_heap_order` | 3/4 条同 ident（167） |
| `lib.rs` `SpellFrameBuf` | `recipients_get_independent_heaps` | `[A0,B1,A2,B3]` 不混堆 |
| `npc.rs` | `dequeue_order_same_priority_follows_four_ary_heap` | 3/4 条同组说词 |
| `trade.rs` | `emitter_finish_same_priority_follows_four_ary_heap` / `..._keeps_recipients_in_separate_heaps` | 3/4 条 + 分组 |
| `selloff.rs` | 同上两例 | 3/4 条 + 分组 |
| `party.rs` | 同上两例 + `emitter_finish_keeps_immediate_order` | 3/4 条 + 分组 + 立即桶不变 |

## 5. 牙（teeth）

牙（在**提交后的树**上做；每颗 = 改回稳定排序 → 对应用例红 → `git checkout --` 逐字节还原 →
`sha256` 与 `git show HEAD:<path>` 一致；工作树回到干净）：

| 牙 | 模块 / 文件 | 改坏内容 | 期望红 | 实测 | 提交后文件 sha256（还原后复核一致） |
|---|---|---|---|---|---|
| T-A | `bins/game-svr/src/lib.rs`（`SpellFrameBuf::flush`） | 按收件人堆 → 改回 `frames.sort_by_key(|f| f.1)` 全局稳定排序 | `spell_frame_buf_tests::same_priority_frames_follow_dotnet_heap_order` | **FAILED**（`n=3：同优先级出队序必须是四叉堆序`，`test result: FAILED. 1 passed; 1 failed`） | `5c54e755ff859d5b514db5ad8bd1c6d438bfdb39c80ceb44fd1a9d007be9d9c0` |
| T-B | `crates/world/src/party.rs`（`Emitter::finish`） | 按收件人堆 → 改回 `sort_by_key(|(p, _)| *p)` | `party::tests::emitter_finish_same_priority_follows_four_ary_heap` | **FAILED**（`n=3`，`test result: FAILED. 2 passed; 1 failed`） | `a71d3e2f0aff7c15059b4555e1198153e1c67db74b63ca3bc38794b5c69ba1ba` |
| T-C | `bins/game-svr/src/trade.rs`（`Emitter::finish`） | 同上 | `trade::tests::emitter_finish_same_priority_follows_four_ary_heap` | **FAILED**（`n=3`，`test result: FAILED. 1 passed; 1 failed`） | `d8ece1a754417738bb565e2bc97c899a29a1a63f608b29f32617331654a92f97` |

三颗牙分属**三个不同模块 / 两个 crate**（`game-svr` 的 `lib.rs`、`crates/world` 的 `party.rs`、
`game-svr` 的 `trade.rs`）。注：`recipients_get_independent_heaps` 在 T-A/T-C 下仍绿（两个收件人各 2 条
等优先级 ⇒ 稳定序恰好等于堆序），所以「分组」本身由 `[0,2,1]` 用例兜底 —— 这正是 `[0,2,1]`/`[0,3,2,1]`
判据的必要性。

## 6. 残余与 R 级窗口项（`window_required: true`）

1. **混合队列**：本项目不建模整条观察者队列（跨 tick、跨系统的 pending）。批内序已与 oracle 堆算法
   一致；真机同一 tick 内若同一收件人既收到本批帧、又有其它系统入队的同优先级消息，sift 结果会与
   批内模型不同 ⇒ 需窗口逐帧序对拍（登记 T-208）。
2. **真机判据（R 级，必跑）**：
   - 施法：`SM_STRUCK(31)` 多条到同一观察者的**顺序**（B2 地狱雷光 9/10、B4）；
   - NPC 说词：`桃源-N109` 的 `@10cang` 等 9 条连续 `SendMsg` 的到达顺序；
   - 交易：`DealSuccess` 四门同失败时 4 条红字的顺序；取消交易的绿字；
   - 元宝寄售：取消寄售多条 `SM_MENU_OK`、买多颗金刚石多条 `SM_SYSMESSAGE`；
   - 组队：3 人组团长死亡散组的 `SM_SYSMESSAGE` 顺序。
   两侧（C# 真值栈 / Rust 栈）同一账号、同一场景，抓 `frames.ndjson` 逐帧比 `ident + 次序`。
3. 本卡不覆盖：`crates/world` 的 `deliver_mag_struck` 交付队列（非帧排水，见台账 #18）与跨 actor
   处理序（`not_verified`）。

## 7. 写集说明

新增 `mir2-rs/crates/world/tests/party_vectors.rs`：该文件断言的是**跨收件人的平坦事件序**，而 oracle
每人一条队列 ⇒ 平坦序本就不是契约。本卡把 4 个用例改为按 `target` 断言（帧集合与逐收件人序不变），
否则新实现必然撞这些过规格的旧期望。

## 8. 门禁

全部在**本分支 head** 上跑（`mir2-rs/` 下；reviewer 请自行重跑）：

| 门禁 | 命令 | 结果 |
|---|---|---|
| fmt | `cargo fmt --all -- --check` | rc=0（clean） |
| clippy | `cargo clippy --workspace --all-targets -- -D warnings` | rc=0（`Finished dev profile`，无 warning） |
| test | `cargo test --workspace` | **141 suites / 3025 passed / 0 failed / 62 ignored**，rc=0 |
| msgcodegen | `cargo run -p msgcodegen -- --check` | `OK: 604 条常量（Messages.cs 594 + Grobal2 白名单 10）` |
| anchor | `python -I tests/parity/m3/whitelist_anchor_gate.py check` | `ALL PASS（496 条锚）` |
| registry | `python -I tests/parity/m3/m3_whitelist_registry_audit.py audit` | `WHITELIST_REGISTRY GREEN` |
| T 预约表 | `python -I tools/whitelist/t_reservation_gate.py check`（仓库根） | `GREEN（预约表 197 行 / whitelist 197 号）` |
| 并行守卫 | `cargo test -p mir2-world --test no_parallel` | 5 passed；**白名单未改**（`git diff <base> -- mir2-rs/crates/world/tests/no_parallel.rs` 为空） |

### 8.1 suite 数对照（改前 master vs 本 head）

| 树 | suites | passed | failed | ignored |
|---|---:|---:|---:|---:|
| 基线 `f7a32c2fd`（临时 worktree 实测同一条命令） | 141 | 3013 | 0 | 62 |
| 本 head | 141 | 3025 | 0 | 62 |
| 差 | **0** | **+12** | 0 | 0 |

`+12` 全部是**新增判据**，逐条对得上：`crates/world/src/msgqueue.rs` +1（`ties_dequeue_in_four_ary_heap_order`；
原有 4 个用例是**随模块迁移**，不增不减）、`bins/game-svr/src/msgqueue.rs` +2（再导出模块的两个新用例；
原 4 个迁走后净 -4+4=0，另加 2）、`lib.rs` +2、`npc.rs` +1、`trade.rs` +2、`selloff.rs` +2、`party.rs` +3。
**suite 数不变**（无 crate/测试目标增减；`msgqueue` 只是从 `game-svr` 的 `lib` 内模块迁到
`mir2-world` 的 `lib` 内模块，仍是同两个 lib 测试目标）。
