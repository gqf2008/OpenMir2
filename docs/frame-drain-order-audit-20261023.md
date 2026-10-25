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
| 1 | `crates/world/src/msgqueue.rs`（`NetPriorityQueue` 本体；`bins/game-svr/src/msgqueue.rs` 变为再导出） | `u8` 优先级，四叉堆 | ✅ 本体 | — | `bins/game-svr/src/msgqueue.rs` 的 `ties_dequeue_in_four_ary_heap_order` 钉 `[0,2,1]` / `[0,3,2,1]`（同一份实现） |
| 2 | `lib.rs` `SpellFrameBuf::flush`（B2/B3/B4/B5 全部效果体块共用） | `(socket, priority, body)`，**每收件人**各自建堆 | ✅ **本卡修**（原 `frames.sort_by_key(|f| f.1)` 稳定排序） | **可达** | 一次施法多目标 ⇒ 同一观察者多条 `SM_STRUCK(31)`（167）/ 多条 `SM_CHANGENAMECOLOR(656)`（154）。构造判据 `spell_p3b2_lightflower_center_self_and_undead_division`（911 收 4 条 31）、`spell_p3b2_fireboom_delivery_ledger_and_frames`（912 收 4 条 31） |
| 3 | `lib.rs` `emit_script_frames_raw` | `&SendMsg`，单收件人（本人） | ✅ 已有（pickwire） | **可达**（`RM_DURACHANGE` 每件装备一条） | 三处入口共用：商人 `emit_script_frames` / `emit_login_script_frames` / `run_party_goto_label`；判据 `script_drain_orders_same_priority_by_dotnet_heap` |
| 4 | `lib.rs` `drain_slave_mutiny_downlink` | `MutinyFrame`，按观察者分组 | ✅ 已有 | **可达**（N 只宝宝 × 色/名） | 观察者 `MsgQueue` 每只宝宝 `RefNameColor(); RefShowName();` |
| 5 | `lib.rs` `CM_ADJUST_BONUS` 成功支队列 | `NetPriorityQueue<u16>` + `send_update_msg` | ✅ 已有 | 不可达（657 头-only 去重后净 1 条） | 该支注释 I-2 + `msgqueue::tests::c9_send_update_msg_peeks_head_only` |
| 6 | `bins/game-svr/src/healthspell.rs` `HealthSpellSlots` | 每 actor 一条 `NetPriorityQueue<u16>` | ✅ 已有 | 可达 | `send_update_msg` 头-only 去重模型 |
| 7 | `bins/game-svr/src/run_frames.rs` `RunFrames` | 每 actor 一条 `NetPriorityQueue<GroupFrame>` | ✅ 已有 | 可达 | `flush_orders_by_rm_ident_low_byte` 等 |
| 8 | `bins/game-svr/src/guild_official.rs` `flush` | `QueuedFrame`，**按 `actor_id` 分组**后各自建堆 | ✅ 已有（pickwire 修） | 可达 | `flush_drains_per_recipient_heap` |
| 9 | `bins/game-svr/src/guild_official.rs` `build_queued_frames` | `out.sort_by_key(|(p, _)| *p)`（稳定） | ⚪ 稳定排序 | **不可达** | 该批优先级字节**两两不同**：`PRI_CHANGEGUILDNAME`=61、`PRI_BUILDGUILD_OK/FAIL`=63（互斥）、`PRI_GOLDCHANGED`=152 ⇒ 稳定序 = 堆序。函数头注释已订正 |
| 10 | `bins/game-svr/src/npc.rs` `dequeue_order`（说词） | 整批灌堆（`High=0` 组 + `SendMsg=1` 组） | ✅ **本卡修**（原「priority 组 ++ 非 priority 组」= 稳定退化；**防御性收口**） | **部署无实例**（构造级可达） | 真说词路径 = `GotoLableSendMerChantSayMsg`（`ScriptEngine.cs:988-999`）→ `NpcPlayer::send_say`。同优先级（`High=0`）多条需要同一 label 内 ≥2 次 `send_say(true)`；`goto_params.send_say_msg` 只由 `MAP`/`MAPMOVE`/`PLAYDICE` 置位且 sticky（`crates/script/src/engine/actions.rs:959-965`），而 `BREAK` 会终止整个 label 循环 ⇒ 实测 **0 个 label 能产出 ≥2 条同优先级说词**。**扫描口径（两个口径互不包含但并集闭合）**：① 作者口径 —— 三个脚本目录 `Market_Def`+`Npc_def`+`QuestDiary`（**不含 `MapQuest_def`**）共 361 个 `.txt`，含上述动作的 label 150 个，按 sticky+break 逐 procedure 建模 ⇒ ≥2 次发射 = **0**；② reviewer-5 口径 —— 全 Envir 642 文件、label 同时含 `#say` 且 ≥1 个该类动作 ⇒ 25 个 label 全 (1 trigger, 1 `#say`)、≥2 触发 = 0（只数动作数，是上界）。**盲区互补**：reviewer 补查了作者的 `MapQuest_def`（12 文件 / 24 label、`#say`+动作 0 个；`@OnTimer9` 虽有 2 个 `MAPMOVE`，但第一个 procedure 的 `BREAK`（`QManage.txt:113/138`）在第二个（`:145`）之前 ⇒ 至多 1 次发射且文本为空）⇒ **0 成立，二者都不欠扫**。故本修是**构造级防御性**（判据 `dequeue_order_same_priority_follows_four_ary_heap` / `dequeue_order_puts_priority_high_first`），**真机窗口永远验不到它**。**分级：本项按 L（构造级）记账；承接卡 `openmir2-script-sendmsg-host` 落地后转 R**（届时 `send_say_msg`/`world_broadcast` 让该路径变活）。 |
| 11 | `bins/game-svr/src/trade.rs` `Emitter::finish` | `QueuedEvent`，**按 target 分组**后各自建堆 | ✅ **本卡修**（原 `sort_by_key(|q| q.priority)`） | **可达** | `deal_success` 四个前置检查各发一条 `red_msg`（`RM_SYSMESSAGE`=252）给同一 `me.id`，`deal_cancel_em` 再叠。判据 `emitter_finish_same_priority_follows_four_ary_heap` 等 |
| 12 | `bins/game-svr/src/selloff.rs` `Emitter::finish` | `QueuedFrame`，**按 target 分组**后各自建堆 | ✅ **本卡修**（原 `sort_by_key(|(p, _, _)| *p)`） | **可达** | `client_cancel_sell_off_ing` 每取消一条寄售各一条 `menu_ok`（69，同一 `me.id`）；`client_buy_sell_off_item` 每颗金刚石各一条 `blue_msg`（252） |
| 13 | `crates/world/src/party.rs` `Emitter::finish` | `(u8, PartyEvent)`，**按 `PartyEvent::target()` 分组**后各自建堆 | ✅ **本卡修**（原 `sort_by_key(|(p, _)| *p)`） | **可达** | 团长死亡散组（`party_del_member_em` 的 `owner == target` 支）逐成员 `LeaveGroup()` ⇒ 同一成员一次收多条 `RM_GROUPMESSAGE`(253)。判据 `emitter_finish_same_priority_follows_four_ary_heap` |
| 14 | `bins/game-svr/src/ground.rs` `dequeue_order`（拾取/丢弃 self notice） | `sort_by_key(|n| (n.ident as u8) as u16)`（稳定） | ⚪ 稳定排序 | **不可达** | 枚举：`pick_up` 的 `notices` 只在三条**互斥且立即 return** 的分支里产生 ⇒ 可达集合 `{SYSMESSAGE 252}` / `{GOLDCHANGED 152}` / `{ITEMHIDE 149, WEIGHTCHANGED 131}`；`drop_item` 只有 `{MENU_OK 69}` / `{WEIGHTCHANGED 131}`；`update()` 已按同 ident 去重。组合内优先级字节两两不同 ⇒ 稳定序 = 堆序（模块头与函数注释已写清） |
| 15 | `bins/game-svr/src/useitem.rs` `use_item` 尾 | `sort_by_key(|n| n.ident as u8)`（稳定） | ⚪ 稳定排序 | **不可达** | 枚举：`notices` 只在五条互斥分支产生 —— ①禁药 `SYSMESSAGE`(252)（`eaten=false`）/ ②`HEALTHSPELLCHANGED`(68)（StdMode 0 的 shape 1 / shape 3，两支互斥且成功才入队）/ ③`CHARSTATUSCHANGED`(155) + `MYSTATUS`(166)（StdMode 1 状态跳档，成对）/ ④`MENU_OK`(69)（`can_use_item==false`）/ ⑤`WEIGHTCHANGED`(131)。可达组合共五个：`{252}` / `{68,131}` / `{155,166,131}` / **`{131}`**（纯 WeightChanged：StdMode 2、StdMode 31 解包成功、StdMode 0 的 shape 非 1/3 default 支）/ `{69}`——组合内两两不同（注释已写清） |
| 16 | `bins/game-svr/src/guild_cmd.rs` `Ctx::flush(Vec<Deferred>)` | 无排序 = 插入序 | ⚪ 无排序 | **不可达** | 每个分支对同一 recipient 至多各推 750(61)/42(165)/100(252) 一条，且插入序已升序 ⇒ 与堆序一致 |
| 17 | `lib.rs` `emit_def_frames` / `emit_shop_frames` | 无排序 = 源码序 | — | 不适用 | **`SendDefMessage` 直发**（不进 `MsgQueue`）⇒ oracle 同样按源码序写 socket |
| 18 | `crates/world/src/world.rs` `deliver_mag_struck`（`msgs.sort_by_key(|m| m.order)`） | 消息创建序 | ⚪ 稳定排序 | 本卡**不改** | 不是帧排水，是 `Operate(RM_MAGSTRUCK)` 的**交付队列**（跨 actor 处理序 + RNG 抽数序）；oracle 每人一条队列、由 `ProcessMonsters` 的目标序泵出，跨 actor 序已登记 `not_verified`（合同 §1.6）。同 target 重复只在「同 tick 多施法者命中同一目标」出现，属同一登记面 |
| 19 | `lib.rs` `observers.sort_unstable()`（叛变帧收件人集）、`chat.rs` `hear_targets`/`cry_targets` 的 `out.sort_unstable()` | 收件人**集合**排序 | — | 不适用 | 决定「谁收」而非「同一人的帧序」；每个收件人内部的帧序由 #4 的堆决定 |
| 20 | `lib.rs` 多处 `names.sort()` / `keys.sort()` / `socks.sort_unstable()` | `HashMap`/`HashSet` 迭代确定性 | — | 不适用 | 遍历序，非帧排水序（多数还在 `#[cfg(test)]` 断言里） |

**结论**：全仓「按优先级/ident 排序后发出」的排水点共 20 处；其中 **5 处**存在同 ident 多帧的分叉
风险（#2/#10/#11/#12/#13），**全部**改为 `NetPriorityQueue`（需要区分收件人的一律先分组）。按部署
可达性再分两级：**部署可达 4 处**（#2 施法、#11 交易、#12 寄售、#13 组队），**部署无实例 1 处**
（#10 NPC 说词 —— 构造级防御性收口，见该行）。其余「稳定排序」站点均已给出**不可达枚举证据**并
订正注释；已有堆实现的 6 处（#3–#8）保持不变。

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
   - `npc.rs::dequeue_order` 文档：「组内取入队序」→ 堆序。

## 3. 受影响的既有判据（期望值订正，逐条给理由）

| 用例 | 旧期望 | 新期望 | 理由 |
|---|---|---|---|
| `npc::tests::dequeue_order_puts_priority_high_first` | `[b,d,a,c]` | `[b,d,c,a]` | 整批 `[a(1),b(0),c(1),d(0)]` 灌堆后出队 `b,d,c,a`；旧值来自「两组各自稳定」的退化。**部署无该形态实例**（§1 #10）⇒ 只影响构造级判据 |
| `spell_p3b2_lightflower_center_self_and_undead_division`（911 的 4 条 31） | `[m2,m1,m3,p1]` | `[m3,m1,m2,p1]` | 批 = `638(136)` + 逐目标 `31(167)`/`656(154)`；四叉堆出队序（已用 `NetPriorityQueue` 逐位复核） |
| `spell_p3b2_fireboom_delivery_ledger_and_frames`（912 的 4 条 31） | `[m1,m2,p1,m3]` | `[p1,m2,m3,m1]` | 同上 |
| `trade::tests::deal_end_ok_time_gate_cancels` | 跨收件人交错 `(1,2,1)` | 分组 `(1,1,2)` | oracle 每人一条队列，跨收件人平坦序不是契约 |
| `tests::die_triggers_del_member_frames_on_production_path` | 平坦 5 帧序 | 集合 + 逐 socket 序 | 同上 |
| `tests::party_cm_dispatch_emits_oracle_frames_on_both_connections` | 平坦 7/6 帧序 | 逐 socket 序（`drain` 改为带 socket） | 同上 |
| `crates/world/tests/party_vectors.rs` 4 例 | 平坦事件序 | 逐 `target` 事件序 | 同上 |

## 4. 新增构造级判据

| 模块 | 用例 | 钉住 |
|---|---|---|
| `msgqueue`（`bins/game-svr`，实现来自 `crates/world`） | `ties_dequeue_in_four_ary_heap_order` | 3 条 `[0,2,1]`、4 条 `[0,3,2,1]`（堆本体；原 4 条 dotnet 对照用例随模块迁到 `crates/world/src/msgqueue.rs`） |
| `msgqueue`（`bins/game-svr`） | `protocol_constants_match_world_literals` | `(byte)RM_*` 字面量与协议常量一致 |
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
| T-A | `bins/game-svr/src/lib.rs`（`SpellFrameBuf::flush`） | 按收件人堆 → 改回 `frames.sort_by_key(|f| f.1)` 全局稳定排序 | `spell_frame_buf_tests::same_priority_frames_follow_dotnet_heap_order` | **FAILED**（`n=3：同优先级出队序必须是四叉堆序`，`test result: FAILED. 1 passed; 1 failed`）——**已在返工后的 head 上重跑**（旧值 `5c54e755…` 是 `d7f17a6cf` 之前的树，作废） | `a86d505ae25144e4949ea0cb4d1fbd08a9e2ab1cc3304b8096eb2ddf12bc6921` |
| T-B | `crates/world/src/party.rs`（`Emitter::finish`） | 按收件人堆 → 改回 `sort_by_key(|(p, _)| *p)` | `party::tests::emitter_finish_same_priority_follows_four_ary_heap` | **FAILED**（`n=3`，`test result: FAILED. 2 passed; 1 failed`） | `a71d3e2f0aff7c15059b4555e1198153e1c67db74b63ca3bc38794b5c69ba1ba` |
| T-C | `bins/game-svr/src/trade.rs`（`Emitter::finish`） | 同上 | `trade::tests::emitter_finish_same_priority_follows_four_ary_heap` | **FAILED**（`n=3`，`test result: FAILED. 1 passed; 1 failed`） | `d8ece1a754417738bb565e2bc97c899a29a1a63f608b29f32617331654a92f97` |

三颗牙分属**三个不同模块 / 两个 crate**（`game-svr` 的 `lib.rs`、`crates/world` 的 `party.rs`、
`game-svr` 的 `trade.rs`）。注：`recipients_get_independent_heaps` 在 T-A/T-C 下仍绿（两个收件人各 2 条
等优先级 ⇒ 稳定序恰好等于堆序），所以「分组」本身由 `[0,2,1]` 用例兜底 —— 这正是 `[0,2,1]`/`[0,3,2,1]`
判据的必要性。

## 6. 残余与 R 级窗口项（`window_required: true`）

### 6.1 混合队列（模型边界）

本项目不建模整条观察者队列（跨 tick、跨系统的 pending）。批内序已与 oracle 堆算法一致；真机同一 tick
内若同一收件人既收到本批帧、又有其它系统入队的同优先级消息，sift 结果会与批内模型不同 ⇒ 需窗口逐帧序
对拍（登记 T-208）。

### 6.2 窗口项（可复跑命令，gw-capture 范式同 T-207）

**捕获（两侧各跑一次）**：

```sh
# C# 真值侧：把客户端登录端口指向影子代理 7101（代理转发到 7100）
python -I -B tools/gw-capture/gate_shadow_proxy.py --out E:/tmp/w208/csharp/sel --map 7101=127.0.0.1:7100 --gate sel --direction s2c
# Rust 侧：同一命令、同一场景，输出换目录
python -I -B tools/gw-capture/gate_shadow_proxy.py --out E:/tmp/w208/rust/sel   --map 7101=127.0.0.1:7100 --gate sel --direction s2c
# 每侧跑完复核 chunks/frames 无损
python -I -B tools/gw-capture/gate_shadow_proxy.py --verify E:/tmp/w208/csharp/sel
python -I -B tools/gw-capture/gate_shadow_proxy.py --verify E:/tmp/w208/rust/sel
```

判读：从两侧 `sel/frames.ndjson` 取同一 `ident` 的**出现次序**逐帧比（不只比集合）；同优先级 N 条的
期望序 = .NET 四叉堆序（3 条 `[0,2,1]`、4 条 `[0,3,2,1]`）。

| # | 场景（触发点） | 期望 | 覆盖的修复点 |
|---|---|---|---|
| ① | 角色带 9/10（地狱火/疾光电影）技能，对**同一视野内 ≥3 只怪**施放（B2 块） | 观察者侧 `ident=31` 的**出现次序** = 堆序（不是目标枚举序）；条数 = 命中目标数 | #2 `SpellFrameBuf::flush` |
| ② | ~~桃源-N109 的 9 条连续 `SendMsg`~~ **已撤回**：那是 `SENDMSG 0` ⇒ `WorldEngine.SendBroadCastMsg` ⇒ `RM_SYSMESSAGE(252)`，**不是** `RM_MERCHANTSAY`；且宿主 `world_broadcast`/`send_say_msg` 目前是 no-op（`crates/script/src/engine/mod.rs`，`NpcPlayer` 未覆写）⇒ 当前产 0 帧。**真说词路径在部署无 ≥2 同优先级实例**（§1 #10）⇒ 本项**无窗口场景**，由构造级判据兜底；接上 `SENDMSG` 宿主（承接卡 `openmir2-script-sendmsg-host`，issue `bdddbeba`）后，`world_broadcast` 的逐玩家 `SM_SYSMESSAGE` 会**新增一个同 ident 排水点，必须按四叉堆序处理**，届时另开窗口。 | — | #10（构造级） |
| ③ | 两账号交易，双方背包/金币同时越界使 `DealSuccess` 的**四个前置检查全部失败** | 接收侧 `ident=100`（`SM_SYSMESSAGE`）红字的**次序** = 该玩家队列的堆序（4 条 ⇒ `[0,3,2,1]`），不是提示语序 | #11 `trade::Emitter::finish` |
| ④ | 元宝寄售：同一玩家**连续取消 ≥3 条**寄售（多条 `SM_MENU_OK`）；另跑「一次买多颗金刚石」（多条 `SM_SYSMESSAGE`） | `ident=767`（`SM_MENU_OK`，`mir2-rs/crates/protocol/src/messages.rs:354`；**不是** 769 = `SM_GUILDMAKEALLY_FAIL`）/`ident=100` 的条数与次序 = 堆序；`Recog` 均为该玩家 | #12 `selloff::Emitter::finish` |
| ⑤ | 3 人组（L/T/U）由团长 L 死亡触发散组 | 每名成员收到的 `ident=100`（`RM_GROUPMESSAGE`）条数 = 退组事件数；同一成员的**次序** = 其队列堆序 | #13 `party::Emitter::finish` |

### 6.3 本卡不覆盖 / 承接

- `crates/world` 的 `deliver_mag_struck` 交付队列（非帧排水，见台账 #18）与跨 actor 处理序
  （`not_verified`，合同 §1.6）。reviewer-5 `O-2` 建议另开卡审计「同 target 多消息」面。
- **承接卡 `openmir2-script-sendmsg-host`（issue `bdddbeba48e6cdbc75aab35f781fb67e917660f9`）**：
  部署 806 行 `SENDMSG` 里 case0-3（`world_broadcast`，逐玩家 `SM_SYSMESSAGE`）= **318 行**、
  case4（`send_say_msg`）= **31 行**，合计 **349 行当前零帧**。这是台账 #10 的可达性误判牵出来的真缺口；
  该卡实现后 `world_broadcast` 会成为新的同 ident 排水点，须复用 `NetPriorityQueue`（其验收条件已写入）。

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

`+12` 全部是**新增判据**，逐条对得上（用两份日志的 `test <名字> ... ok` 行集合求差，`REMOVED 0`）：
`bins/game-svr/src/msgqueue.rs` +2（`ties_dequeue_in_four_ary_heap_order`、`protocol_constants_match_world_literals`；
原有 4 个 dotnet 对照用例**随模块迁到** `crates/world/src/msgqueue.rs`，是同一批用例、名字不变 ⇒ 不增不减）、
`lib.rs` +2、`npc.rs` +1、`trade.rs` +2、`selloff.rs` +2、`party.rs` +3。
**suite 数不变**（141 = 141）：没有 crate / 测试目标增减；`msgqueue` 只是从 `game-svr` 的 `lib` 内模块
迁到 `mir2-world` 的 `lib` 内模块，仍是同两个 lib 测试目标。

## 9. 返工记录（review `09d2125f` / reviewer-5）

| 项 | 处置 |
|---|---|
| **I-1**（important）台账 #10 的「可达」证据错误 | **已撤回并订正**。原引「桃源-N109 的 9 条连续 `SendMsg`」是 `SENDMSG 0` ⇒ `WorldEngine.SendBroadCastMsg`（`ExecutionProcessingSys.cs:3169-3170` → `WorldServer.cs:1738-1765`）⇒ 逐玩家 `RM_SYSMESSAGE(252)`，**不是** `RM_MERCHANTSAY`；且宿主 `world_broadcast`/`send_say_msg` 目前是 no-op（`crates/script/src/engine/mod.rs`，`NpcPlayer` 未覆写）⇒ 当前产 0 帧。真说词路径重新枚举后**部署无 ≥2 同优先级实例**（0/150 label，sticky+break 规则；reviewer-5 独立扫 25 个 label max 1）。#10 改写为「部署无实例、构造级防御性修复」，结论段落同步改为「部署可达 4 处 + 部署无实例 1 处」。窗口项 ② 改为**无窗口场景**（§6.2）。承接卡 `openmir2-script-sendmsg-host`（issue `bdddbeba`，349 行零帧）与「实现后新增同 ident 排水点须按堆序」已写进 §1 #10 / §6.3 / whitelist T-208。 |
| **M-1** `useitem.rs` 枚举漏 `RM_HEALTHSPELLCHANGED(68)` | 已补：①252 禁药；②**68**（StdMode 0 的 shape 1/3 两支互斥、成功才入队）；③155+166；④69；⑤131。结论不变（各组合内两两不同）。 |
| **M-2** `guild_official.rs` 仍写 `(priority, seq)` 排序 | 已改为「按收件人分组 + 每组 `NetPriorityQueue` 出队；同优先级 = 四叉堆 sift 序，不是 FIFO」。 |
| **M-3** §5 T-A 的 `lib.rs` sha 过期 | **T-A 已在返工前的当前 head（`4d2420934`）上重跑**（红：`same_priority_frames_follow_dotnet_heap_order`，`n=3`），`git checkout --` 还原后 sha256 = `a86d505ae25144e4949ea0cb4d1fbd08a9e2ab1cc3304b8096eb2ddf12bc6921`（= 提交后树 `git show HEAD:<path>` 同值，且与 reviewer 测得的 `a86d505a…` 一致）。旧值 `5c54e755…` 作废（它在 `d7f17a6cf` 之前取，属「fmt/后续提交后哈希变化」同类事故）。T-B/T-C 不受影响。 |
| **M-4** T-208 窗口项无可复跑命令 | 已按 T-207 的 gw-capture 范式重写（`tools/gw-capture/gate_shadow_proxy.py` 捕获 + `--verify`，两侧同命令、独立输出目录、逐帧次序判读），五个场景逐条写清触发点与期望（§6.2 与 whitelist T-208）。 |

### 9.1 第二轮（review `549aee7e` / reviewer-5）

| 项 | 处置 |
|---|---|
| **M-4r**（important）窗口④ 的 `ident=769` | **订正为 `ident=767`（`SM_MENU_OK`，`mir2-rs/crates/protocol/src/messages.rs:354`）；769 = `SM_GUILDMAKEALLY_FAIL`**。三处同步：台账 §6.2 ④、whitelist T-208、返工 status 的窗口列表（新 status）。 |
| **m-1** 「.NET 八叉堆序」 | 改为**四叉**（8 处堆都是 Arity=4）。 |
| **m-2** §2 第 5 条残留「点名部署里 9 条连续 `SendMsg`」 | **删除**（该点名在 I-1 已撤回且从未成立，台账里只保留 §6.2/I-1 的「撤回记录」，不再作为修复理由出现）。 |
| **m-3** useitem 可达组合漏纯 `{131}` | 已补：五组合 `{252}` / `{68,131}` / `{155,166,131}` / **`{131}`**（StdMode 2、StdMode 31 解包成功、StdMode 0 shape 非 1/3 的 default 支）/ `{69}`；台账 #15 同步。 |
| **m-4** 「361 脚本」未注明范围 | 已注明：三个脚本目录（`Market_Def`+`Npc_def`+`QuestDiary`）**不含 `MapQuest_def`**；并写入 reviewer 的补查（`MapQuest_def` 12 文件 / 24 label、`#say`+动作 0 个；`@OnTimer9` 的两个 `MAPMOVE` 之间隔着一个 procedure 的 `BREAK`）与「两口径互不包含但并集闭合」。 |
| **分级** | 按 reviewer 裁决写入台账 §1 结论段：`npc::dequeue_order` **按 L（构造级）记账**、`openmir2-script-sendmsg-host` 落地后转 R；**整卡保持 R**（另 4 处部署可达）；**不拆卡**（同一排水原语 + 避免承接卡落地时留下无人负责的未验路径）。#10 与 T-208 均加了「本项 L（构造级）；承接卡落地后转 R」。 |
| **机器守卫缺失** | 台账新增「已知局限」：§1 的不可达枚举是散文、无断言（reviewer N2：删 useitem 68 那条仍绿）⇒ 改动需人工复核；代码侧新增同 ident 帧有夹具兜底（N3 红）。 |
