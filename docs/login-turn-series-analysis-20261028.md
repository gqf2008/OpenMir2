# 登录期 SM_TURN series 与多余尾帧分析（2026-10-28）

> 卡：`openmir2-login-turn-series-and-extra-tail`（tier=R，`window_required=true`）。
> 数据：`E:/tmp/window-20261026/run2/move_walk/{csharp,rust}`（run2，2026-10-26）。
> 本文只做离线归因；不改生产代码、`rules.json` 或场景语义，不声称真机收口。

## 1. 结论

- **Dir 差不是编码公式差**：两侧都按 `SM_TURN.series = MakeWord(Dir, Light)` 编码，Light 恒 2；差异只来自发帧时刻实体的 `Dir`。
- 两处 Dir 差都落在**固定商人**格：`(318,332)`（`MerChant.txt:78`）与 `(332,317)`（`MerChant.txt:82`）。
- oracle 多出的 `seq=37 / t=7376 / dir=0` 无名尾帧，是 `(318,332)` 商人随后再抽中一次随机闲置转身；Rust 已有同形生产路径，只是该次随机支未命中。
- 随机支来源：`Merchant.Run`（`src/GameSrv/Npc/Merchant.cs:132-141`）每轮 `Random(50)==0 ⇒ TurnTo(RandomByte(8))`；oracle 随机源墙钟播种、Rust 固定种子，两侧流位不同。
- 时点：658 前的 1383ms 差和 658→首扫的 896/1001ms 都是观测/登录批处理差；不能从既有帧时间戳推出服务端协议时点差。

## 2. A1：两处 Dir 差

| 坐标 | C# | Rust | 观测量 |
|---|---|---|---|
| `(318,332)` | `seq24 recog=1596436667 series=517`（Dir=5） | `seq20 recog=70 series=512`（Dir=0） | `param/tag=318/332` 相同；body 均为 30B |
| `(332,317)` | `seq26 recog=1857992563 series=515`（Dir=3） | `seq26 recog=72 series=516`（Dir=4） | `param/tag=332/317` 相同；body 均为 30B |

`series` 高字节恒为 2，低字节才是 Dir；`Recog` 属全局掩码字段，不能拿它做等价依据。

这两个坐标在 `E:/MirServer/Mir200/Envir/MerChant.txt` 是固定商人，不是玩家/怪物。`Merchant.Run` 的随机闲置支会在任意一轮改 `Dir`：

- oracle：`Merchant.cs:132-141`，先 `Random(50)`，命中则 `TurnTo(RandomByte(8))`；否则再 `Random(50)` 决定是否发 `RM_HIT`。
- Rust：`merchant_run`（`mir2-rs/crates/world/src/merchant.rs`）同样抽两次并写 `merchant_turns`；
- 发帧：`drain_npc_merchant_turns`（`mir2-rs/bins/game-svr/src/lib.rs:15591`）把 `merchant_turns` 落成无名 `SM_TURN(10)` + `SM_FEATURECHANGED(41)`；
- 构造级证据：`npc_and_merchant_turn_hit_frames_through_production_drain` 与 `npc_random_idle_frames_through_production_drain_run` 已钉住该路径。

因此“同实体两侧 Dir 不同”不是 Rust 用了快照 Dir、也不是漏转向；它正是 `rules.json` ident-10 现有说明里承认的**跨进程不可复现随机闲置方向**。不能把固定 Dir 当契约。

## 3. A2：oracle 多一条无名尾 SM_TURN

- C# 多出：`seq=37 / t_ms=7376 / ident=10 / param=318 / tag=332 / series=512 / body_len=11`。
- 同一 actor 在 C# `seq24 / t=3024` 已是 `series=517`（Dir=5）；该尾帧把它随机转到 Dir=0。
- 无名尾 `body_len=11` 是 `RM_TURN` 支的 `Encode(8B CharDesc)`，不是登录出现帧的带名尾；Rust 的 `drain_npc_merchant_turns` 对 `TurnTo` 事件也产同样的无名对。
- Rust 窗口内没有这条，是因为同一随机支未命中；不得用固定条数判据，也不得把它写成“Rust 少发一次转向”。

## 4. A3：时点差的重推

以两侧同序锚点（`frames.ndjson` 的 `t_ms`，相对各自 `run.json.start_wall`）：

| 锚 | C# | Rust | 差 |
|---|---:|---:|---:|
| `SM_STARTPLAY(525)` | 389ms | 378ms | 11ms |
| `SM_SENDNOTICE(658)` | 2128ms | 734ms | 1394ms |
| 首条 `SM_TURN(10)` | 3024ms | 1735ms | 1289ms |
| 658 → 首个出现帧 `51` | 896ms | 0ms | 896ms |
| 658 → 首条 `SM_TURN(10)` | 896ms | 1001ms | 105ms |
| 525 → 658 | 1739ms | 356ms | 1383ms |

- 绝对 1394ms 差**全部发生在 658 之前**（前四帧逐对差 = 1/-3/11/11ms，最大 11ms），不能归给 `SM_TURN` 或 41 的出队。
- **658 之后不能再说“两侧只差 105ms”**：首个出现帧 `51` 是 C# `2128→3024`（+896ms）、Rust `734→734`（+0ms），观测差 896ms。C# 的 896ms 延迟可能来自 `RunNotice → CM_LOGINNOTICEOK → UserLogon` 等待、TCP/探针批处理或真实服务端时点差；现有帧时间戳只有接收时刻，无法区分，必须留 A4 用服务端 tick/发送侧证据判定。
- C# `t=3024` 的登录尾帧与出现帧同批到达；帧时间戳是探针**接收时间**，不是 `SendSocket` 时刻，不能据此反推服务端 tick。
- oracle `PlayObject` ctor 覆盖为 `RunTick=GetTickCount()/RunTime=250/SearchTime=1000`（`src/M2Server/Player/PlayObject.cs:898-901`），首扫门在 `WorldServer.cs:663-667`；Rust 首扫基准是 `PLAYER_SEARCH_TIME=1000`（`mir2-rs/crates/world/src/entity.rs:18`），并有 `first_scan_base_is_playobject_ctor_not_map_entry` / `login_search_base_is_captured_at_playobject_ctor` 钉住。
- 因而只能在**同一观测窗**内比较相对关系；“C# 更晚绝对时点 = 协议时序差”不成立。尤其首个出现帧的 896ms 观测延迟既不能证明真差，也不能证明只是批处理；需要服务端 tick 日志与 A4 同窗重拍，`window_required` 保持。

## 5. 处置建议

1. **不新增生产修法**：A1/A2 归随机闲置支，Rust 生产路径与构造牙已在。
2. **不改 `head_mask:["series"]`**：它仍整字掩掉 Dir，不能作为本卡收口依据。
3. 若要消掉对拍噪声，应另立**actor 范围的随机转身规则**：仅对 `Merchant.Run`/`GuildOfficial.Run` 的 actor，把 Dir 从形状比较中剔除；同时对非随机 actor 的 `SM_TURN` 保持 Dir 逐字节比较。该规则不在本卡擅自落地。
4. 轴 (b) 顺序仍需当前 master 二进制同场景重拍才能关闭；A4 真实同窗判据未完成。

## 6. 证据

| 文件 | sha256 |
|---|---|
| `E:/tmp/window-20261026/run2/move_walk/csharp/frames.ndjson` | `b87cbf82f16fa7c32853a349d9a8fa79b8c4a35e7289ec06e222477360485b08` |
| `E:/tmp/window-20261026/run2/move_walk/rust/frames.ndjson` | `02acc1522a60a371da538d93ef2b81e9b7fe09fc3ddd15cb98b3b27ece87d4d6` |
| `E:/tmp/window-20261026/run2/move_walk/csharp/run.json` | `4d01a739a3839bfd129aa77c258bdc68ea1f955944082e29b686d33e93673cd7` |
| `E:/tmp/window-20261026/run2/move_walk/rust/run.json` | `a6bc490ea94a26962cf7ef570fe50848311ce663b52e1592db57ecf5987b0333` |
