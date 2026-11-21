# L2-4 实施计划：生产多区 + 入站 demux（先计划后代码）

> 状态：**计划（等审，无代码）**。日期：2026-11-22。基线 master `e65e68b0f`（L2-3 已合）。
> 前序：`docs/issues/l2-zone-split-plan-20261122.md`、`docs/issues/l2-3-two-phase-halo-proposal-20261122.md`、
> `docs/shard-runtime-20261116.md` §6、`docs/issues/shard-session-placement-design-20261121.md`（登录归属暂缓）。
> **只做 Leader 划的五条边界；边界外一律不做。**

## 1. 文件级写集

| 文件 | 改动 |
| --- | --- |
| `bins/game-svr/src/zone.rs` | `RegionLayout::from_zone_spans(Vec<Vec<RegionSpan>>)`（一区多 span）+ 覆盖 100% 校验 + `band_entity_capacity` 校验入口；`from_spans` 保留（测试用） |
| `bins/game-svr/src/config.rs` | 解析 `MIR2_ZONES`（语法见 §2）与 `max_halo_band_entities`（缺省 512）；非法/超限 ⇒ 启动失败 |
| `bins/game-svr/src/region_loop.rs` | 生产区驱动 `run_zone(...)`：`phase1→snapshot→barrier→phase2`；每区入站双通道；跨区 outbox（`(tick,src,seq)`）；`barriers.wait_*` 计时 |
| `bins/game-svr/src/lib.rs` | `GateEvent` 加 `line: u8`（见 §2）；`run_inner` 多区分支（`MIR2_ZONES` 未设 ⇒ 走现有单线程路径）；`(line,socket)→zone` 分派表；区 0 保持登录/DB/IDS/guild/castle（P2-3b gate 表不变）；跨区帧转发 |
| `bins/game-svr/src/perf_counters`（lib.rs 内模块） | TSV 末尾加 `region_barrier_us`（两屏障各记一次；**只记录不设门**） |
| `bins/game-svr/tests/l2_zone_layout.rs`（新） | 布局语法/覆盖/重叠/带规模门 的构造级用例 |
| `bins/game-svr/tests/l2_multi_zone.rs`（新） | 生产驱动 N=2 与单区基线的逐 tick 对拍（见 §4） |
| `docs/` | 本文 + 交付时更新 `shard-runtime` §6.2 的已知缺口 |

## 2. `MIR2_ZONES` 语法与 `(line_id, socket)` 来源

**语法（确定性、可复现）**：`<zone>:<map>=<x0>,<y0>-<x1>,<y1>[;<map>=...]`，区之间 `;`；
例：`0:0=0,0-99,199;1:0=100,0-199,199`。规则：

- 区号显式、必须 `0..N-1` 连续且每区非空；
- 同一张图**要么完全不提**（默认整图归区 0），**要么显式列全**（该图所有 span 覆盖 100%、
  互不重叠）；混写默认+显式 ⇒ 启动失败；
- 地图名按注册表口径（ASCII 大小写不敏感）；地图尺寸来自注册表/`WorldMaps`；
- 校验全在**启动期**：覆盖 100% / 不重叠 / 每区非空 / 每区边界带实体数 ≤
  `max_halo_band_entities`（512，**静态装载后统计**，超限拒绝启动）。

**`(line_id, socket)` 从哪取**：`gate_conn` 的形参 `service_id: u8` 就是网关线号，现有：
`Closed.index` 已存 `service_id`（G3/T-102）；`LoadDbInfo` 已带 `gate_idx`/`socket_id`；
`OnlineEntry` 是 `(gate_idx, socket)` 帧路由键。**缺的是 `Opened/Command/CertificationPending`
没有 line** ⇒ 给这三个变体加 `line: u8`（`Closed.index` 改名 `line` 或加同名字段消除歧义）。
分派表 = `HashMap<(u8,i32), ZoneId>`；**未知键一律区 0**。`AdminGm`/`KickAccount` 走
`chr/account → zone` 的二级表（登录后填充）。

> 登录归属仍**全部落区 0**（`shard-session-placement-design` 的暂缓结论）。本步 demux 的输入
> 是「已被分配到某区的会话」；生产分配来源只有区 0，N=2 证据用**测试分配缝**（见 §4）。

## 3. 会话在区界移动：**拒绝**（本步不做 handoff，也不允许“原区持有”）

- **选择**：任何会让实体离开本区 rect 的移动/传送 ⇒ fail-closed 拒绝（停留在边界格，
  口径同 `MapNotInShard`），累计 `zone_cross_refused`。
- **理由**：
  1. “仍由原区持有”会破坏「一个坐标只有一个 owner」：目标区的边界带会包含一个由源区
     模拟的外来实体，掉落/AI/命中权威与 halo 单一发射者规则全部含糊；
  2. handoff 是 L2-5/P2-5 的活，本步边界明确排除；
  3. 拒绝是结构性的（布局把关 + 入口判定），不需要新的跨区状态机。
- **代价**：区界必须放在低流量/不可走格（map-zone 设计 Q1）；否则玩家会撞“看不见的墙”。
  验收里 bots 的脚本不得跨界；生产布局的边界选择由 P0-3 类分析给。

## 4. “同位置单区基线”怎么构造（同一 bots/种子/tick 数）

- **驱动形态**：进程内确定性 bot 驱动，走**生产区驱动**（`run_zone` + `(line,socket)` 分派 +
  跨区 outbox），不用网络栈。原因：登录仍落区 0，真实栈无法把 bot 放进区 1；本步证明的是
  demux/发射/屏障的机器，不是登录归属。测试分配缝：`MIR2_ZONE_ASSIGN`（测试专用、
  fail-closed 解析，chr/line 显式映射）；生产不设该变量 ⇒ 全部区 0。
- **脚本**：固定 tick 索引的动作表（走 N/E/S/W、转向；**不含战斗/掉落等 RNG 动作**），
  同一初始坐标、同一地图、同一 `MIR2_RNG_SEED`、同一 tick 数（如 300）。
- **两次运行**：
  - A（拆）：`MIR2_ZONES` 设 N=2，bots 按分配缝分别落在两区、坐标与 B 完全相同；
  - B（基线）：`MIR2_ZONES` 不设（单区），同一脚本/坐标/种子/tick。
- **比对（逐 tick）**：
  1. 每个 bot 的**可见 actor 集合**（本地 + halo）逐 tick 相同；
  2. 每个 bot 收到的**有序帧流**（`ident/param/tag/series/tail`）逐项相同；
  3. 生产路径上的 `dup=0`：每个 `(observer, entity, event)` 只有一个发射者；
     `sent==recv`、`never=0`（按 tick 合并计数）。
- **前置自证**：B 本身跑两遍（同种子）必须逐字节相同（证明 harness 确定性）；否则比较无意义。
- **N=1**：五门 + `-Mode Both` + 归一化 stdout 不变（同一方法）。
- 若你要求“真实栈 + 真 bot”，需要先做登录归属/分配缝的网络面——超出本步边界，届时另开卡。

## 5. 风险（三条）

1. **测试分配缝泄漏成生产语义**：`MIR2_ZONE_ASSIGN` 若被误用，等于绕过登录归属把会话塞进
   非 0 区（那正是被暂缓的设计）。缓解：测试专用命名、默认不设、启动日志明确 WARN、
   fail-closed 解析；生产路径只有区 0。
2. **区界拒绝的可用性**：边界若落在主路/桥，玩家会被“看不见的墙”挡住。缓解：布局启动门
   （覆盖/带规模）+ 边界选择按低流量/不可走格；`zone_cross_refused` 计数进窗口日志；
   演练中一旦踩中即判布局不合格。
3. **屏障活性（最严重）**：某区在 phase1/phase2 panic 或超时 ⇒ 其余区**永久阻塞在
   `Barrier::wait`**，整机挂死而不是单区失败。缓解：沿用“任一片失败整机 Abort”的既有语义，
   加**tick 看门狗**（协调线程监测每区 tick 时长，超阈值 ⇒ 立即 abort 进程）；std `Barrier`
   无超时接口，看门狗是唯一可靠兜底。此条在 L2-4 必须先落，否则多区是全机单点。

**附**：同图多区仍需每区一份地图网格（R2 未解，v1 复制、排除含城堡地图）；DB 仍在区 0，
非 0 区本步无真实会话 ⇒ 无存档面。二者均登记，不在本步扩。

## 6. 段 3 固化的设计（压缩后按此续写，勿凭记忆）

- **语法**（每 token 一段，`;` 分隔）：`<zone>:<mapName>=<x0>,<y0>-<x1>,<y1>`；
  例 `0:0=0,0-9,9;0:1=0,0-3,3;1:0=10,0-19,9`。一区可多段；未提到的图默认整图归 default 区（0）。
- **纯函数**：`parse_zone_layout(raw, map_names, map_sizes, default_zone) -> Result<RegionLayout,
  ZoneParseError>`——不读环境、不碰 I/O；成功路径直接调 `from_zone_spans`（段 2 的覆盖/重叠/界内校验）。
- **区号必须 `0..=max` 连续**（Leader 2026-11-22 拍板）：稀疏（只有 0 和 2）⇒ 报错并**点名缺失区号**；
  "区存在但没有任何 span"同样报错（复用 `EmptyZone` 语义）。理由：稀疏会让"永远空的区仍被建线程/屏障"
  变成隐性坑。
- **重复区号合法**（= 同区多段）。
- **fail-closed 错误清单（含原始 token）**：空串 / 缺 `:` 或 `=` / 区号非数 / 区号越界 /
  区号不连续（点名缺失号）/ 地图名不在注册表 / 坐标非数 / 矩形非法（`x0>x1` 等）。
- **测试**：正例 3（单 span、多 span、default 兜底）；反例 ≥6（空串、语法错、区号越界、
  区号不连续、地图不存在、坐标非数、矩形非法），其中 ≥2 条断言错误文本含原始 token。
