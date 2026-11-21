# ActorId 在 u16 帧尾的截断（P2-3b 暴露；P2-5 前必须决断）

- 来源：P2-3b（`card/shard-multi-world`）把 ActorId 按片分段：`base = shard * (1<<22)`
  （`bins/game-svr/src/shard.rs::ACTOR_ID_SEGMENT`）。
- 结论：分段 id 对**绝大多数**帧安全（12B 命令头的 `Recog` 是 i32），但**一处**帧尾按
  `(ushort)ActorId` 截断，非 0 片的高 16 位会丢。

## 1. 帧清单（全仓扫描 `& 0xFFFF` / `as u16` 的结果）

| 帧 / 位置 | 字段 | 宽度 | 说明 |
| --- | --- | --- | --- |
| `SM_MOUNTRIDE(1118)` `bins/game-svr/src/mount.rs:97` | 尾 `[2] actor_id` | u16 LE | **唯一 ActorId 截断点**；头 `Recog` 仍是完整 i32 |
| `bins/game-svr/src/magicwire.rs:146` | `Recog → skill_idx` | u16 | 值是技能号，不是 ActorId |
| `bins/game-svr/src/smframe.rs:2734` | `Recog & 0xFFFF → AntiMagic` | u16 | 值域本来就是 ushort（G-6），不是 ActorId |
| `crates/world-bevy/src/slave.rs:2429` | `actor_ref → ident` | u16 | 该分支只接受 `RM_STRUCK` 常量（小整数），不是 ActorId |
| `bins/game-svr/src/lib.rs:52482` | 测试断言 | — | 钉的就是上表第 1 行 |

## 2. 「oracle 同形」的证据边界

- `SM_MOUNTRIDE` **不是 oracle 帧**：C# 里 `HorseType` 无赋值点、无马类物品
  （`mount.rs` 模块头 T-277）。该帧由 Rust 侧按 Zig 客户端 T286 约定新增；帧格式
  `12B 头 ++ [mount_id][actor_id:u16 LE]` 是**客户端契约**（`SM_MOUNTRIDE` 头注释、
  `lib.rs` 的 A4 断言）。
- 因此这处截断不是「oracle 有、我们照抄」，而是「客户端协议规定 u16」。改变它需要客户端侧
  同意（客户端冻结 ⇒ 本卡不能单方改）。

## 3. 与 `1<<22` 段宽的关系

- 非 0 片首个 id = `shard * 4,194,304 + 1` > 65,535（u16 上限）⇒ 尾字段只剩低 16 位。
- 片 0 的 id 仍是 1..（基址 0）⇒ 片 0 玩家不受影响。
- 本批非 0 片无玩家（登录永落片 0；handoff 归 P2-5）⇒ 生产路径不触发；构造级也不易暴露
  （`SM_MOUNTRIDE` 需要骑乘会话）。这是「潜伏到 handoff 才炸」的类型。

## 4. P2-5 前必须三选一

1. **缩段宽**：`ACTOR_ID_SEGMENT = 1<<16`（或更小）。16 片 ⇒ id < 1,048,576，仍 > 65,535，
   **不解决问题**；只有「每片 id ≤ 65,535」才不截断，会把每片实体数钉死在 6.5 万以内
   （10k 设计下勉强够、没有余量，且要与容量卡一起定）。
2. **改帧**：帧尾改 4B `actor_id`（或删掉尾字段，客户端改用头 `Recog`）。需要客户端解冻/改版，
   **超出服务器单方范围**。
3. **换 id 分配器**：由协调器全局递增分配（与 oracle 同形：id 始终从小整数开始）。
   分段取消 ⇒ 截断问题消失，且更接近 oracle 的 ActorId 语义；代价是每个新实体要一次跨线程
   分配（可批量化）。**推荐**，与 P2-4 协调器同批。

## 5. 验收（follow-up 完成时）

- 逐帧复扫 `& 0xFFFF` / `as u16`，台账与本文表一致（新增帧必须补表）；
- 选定路线后，N>1 真机上：片 0 → 片 1 的 handoff 玩家，`SM_MOUNTRIDE` 尾字段与头 `Recog`
  一致（或明确登记客户端只读头）；
- 若走路线 3：删除 `ACTOR_ID_SEGMENT`，`SlotTable::set_id_base` 只保留 0（或直接删），
  P2-3b 的三条段测试改为「协调器分配单调且跨片唯一」。
