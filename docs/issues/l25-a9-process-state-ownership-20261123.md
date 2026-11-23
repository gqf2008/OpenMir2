# A9 · M1c 前置 — PROCESS 命令的状态归属（2026-11-23）

> **本表服务 M1c（物品 / NPC / 商店 / 管理面）**，不是 M1b。
> **M1b 的 PROCESS 族只有 `say` 的 GM `@` 支**（form (d) 区 0 持区 k 锁 + A6 接排水层），**不在本表范围**。
> 基线 master `07b00e85`。锚**优先符号名**，行号仅辅助（同卡内会漂）。只读整理，未改码。
> 判据分级：本文均为**构造级/代码锚**，R/L 由 M1c 首片开卡时按 qa-charter 定。

## 0. 口径：什么叫"PROCESS 命令、状态在玩家所在区"

- **PROCESS 命令**：M1c 分流里要"回区 0 执行"的命令/事件（因触进程级单点：`!Send` 宿主、文件/存档、全局计数）。
- **矛盾**：这些 handler 除了进程单点，**多数还要读写玩家所在区（区 k）的本地态**（`SessionGround`、`ground_sessions`、`shop_sessions`、`world`、`online`）。
- 方案编码：**(a)** 数据随控制邮箱带到区 0；**(b)** 区 k prepare + 区 0 commit；**(c)** 留本区执行 + 全局点改原子/共享；**(d)** 其他——本仓既有 **form (d) = 区 0 持区 k 锁、在区 k 状态上跑 handler（§3 登录 `finish_login_on_zone_state` 已用此形）**。

## 1. PROCESS 族状态归属表（M1c）

| # | 族（入口 + 命令/事件；符号锚） | 需要读/写的状态 | 住哪 | 方案倾向 + 一句 | 风险 |
|---|---|---|---|---|---|
| 1 | **guild**：`handle_guild_cmd`（`GUILD_IDENTS`[9]，`CM_GUILD*`） | `guilds:GuildManager`、`cfg:GuildConfig`、`sections:GuildFileSections`（进程/文件） **＋** `online`/`world`/`displays`/`sessions`（玩家区） | **混合** | **(d)** 区 0 持区 k 锁跑：`guilds` 是区 0 线程局部、`sessions` 在区 k 锁内 | 低（成立；受 §3 三硬边界约束） |
| 2 | **shop/商店**：`handle_shop_cmd_inner`（`CM_USERBUYITEM/USERSELLITEM/MERCHANT*`…） | `merchant_book`/`npc_vars`（区 0 `!Send`）**＋** `sell_off_book`（进程）**＋** `item_number`（进程计数，见 §3）**＋** `sessions`/`shop_sessions`/`sell_off_sessions`/`trade_sessions`/`world`（玩家区） | **混合** | **(d)** 区 0 持区 k 锁跑；npc 宿主 !Send 必须区 0，玩家态在锁内 | 中（`item_number` 见 §3） |
| 3 | **click-npc/点击NPC脚本**：`handle_click_npc_cmd`（`CM_CLICKNPC`） | `merchant_book`/`npc_vars`/`script`（区 0 `!Send`）**＋** `item_number`（进程）**＋** `sessions`/`shop_sessions`/`sell_off_sessions`/`world`（玩家区） | **混合** | **(d)** 脚本宿主 !Send 物理上只能区 0，玩家态在区 k 锁内 | 中（`item_number` 见 §3） |
| 4 | **selloff/元宝寄售**：`handle_selloff_cmd`（`CM_SELLOFF*`/`CM_CANCELSELLOFFITEMING`） | `book:SellOffBook`（进程单点）**＋** `ground_sessions`/`sell_off_sessions`/`world`（玩家区）**＋** `catalog`(读) | **混合** | **(d)** 同区收卖；**跨区买卖需两区会话 ⇒ 已定性 defer 到 ⑤/网关门禁**（非本表能消） | 中（跨区对） |
| 5 | **say 的 GM `@` 支**：`handle_say_with_saves`(GM 支)（`CM_SAY` payload `@`） | `guilds`/`castle_war_states`/`gm_*`（进程）**＋** 说话者 `sessions`（玩家区） | **混合** | **(d)** 已在 M1b 定：区 0 持区 k 锁 + **A6 排水层** defer | 低（M1b 已处置） |
| 6 | **kick/踢人·顶号**：`handle_kick_account`（`GateEvent::KickAccount`） | `online`（**玩家区**）、`engine`/`kick_pending`/`soft_close_pending`（进程） | **混合** | 见 **§2 单列**（需跨区定位 + (d)） | **高**（跨区定位，单列裁） |
| 7 | **AdminGm/本机GM**：`execute_admin_gm_command`（`GateEvent::AdminGm`） | `ground_sessions`/`world`/`displays`/`online`（**玩家区**）**＋** `guilds`/`castle_war_states`/`item_number`/`saves`…（进程） | **混合** | 见 **§2 单列**（目标在区 k 时同样要跨区定位） | **高**（同上，单列裁） |
| 8 | **Closed/断线存档**：`handle_zone_closed`（`GateEvent::Closed`） | 玩家 `SessionGround`/world（玩家区）→ 编码存档 → `save_outbox` → 区 0 DB | **混合** | **(a)** 区 k 编码存档载荷 → `save_outbox` → 区 0 `drain_zone_saves`（**已存在**） | 低（既有） |
| 9 | **LoadDbRequest/取档**：`GateEvent::LoadDbRequest` → `engine`/`db` | `engine`/`db`（进程 DB 链） | 进程 | **(a)** 数据经 login 落区路由送给目标区（**已存在**，段 1b） | 低（既有） |

## 2. 【总裁裁】kick / AdminGm —— 广播 + 各区自查（不建全局索引）

> 裁决（opc-27 2026-11-23）：**广播 + 各区自查，不建全局 `(account|chr)→(line,socket)` 索引**。理由：全局索引＝**第二份真相**、会与各区 `online` 漂移（同 A7 registry 风险）；广播代价＝每区一次查表，管理命令频率可忽略。

- **形态**：命令**发到各区** → **每区在自己锁内**按自己 `online`/`ground_sessions` 自查并执行 ⇒ **执行方＝状态持有方，不需要 form (d) 的跨区持锁**（比原稿简单，已改）。
- **可判据（定位）**：仍是 `ZoneRouter::lookup_registered(line,socket)`（若有 `(line,socket)`）＋ 各区 `online` 自查；但**不需要建全局索引**。
- **硬要求（防"静默空效"成永久盲区）**：
  - 计 **`matched`（命中区数）/ `acted`（执行数）**；**0 命中单独计数 + 一条 WRN（限流）**。
  - 判据：对**在线账号** kick ⇒ `matched=1, acted=1`；对**不存在账号** ⇒ `matched=0` 且 WRN **恰 1 条**、限流生效。
  - **同一账号出现在 ≥2 区（本不该发生）**⇒ 计数 `matched>=2` + WRN；**不得**改成"只踢第一个"掩盖。

## 3. 【总裁裁】item_number —— **M1c 第一件事，进程级共享计数器**

> 裁决（opc-27 2026-11-23）：`Arc<AtomicI32>` 共享；**不单立卡**，并入 M1c 第一件事。

- **形态**：**`Arc<AtomicI32>`**（比 Mutex 少一层；`fetch_add(1)+1` 保"先增后用"旧语义）。C# 本为 `M2Share` 全局 ⇒ **向 oracle 对齐，非新设计**。
- **M1c 放行条件**：任何"**区 k 可造物品**"路径开启之前，本改动必须**已在 master**。
- **必须两条证据**：
  1. **N=1 两树逐字节相同**（单区序列不变 ⇒ 帧面零变化）；
  2. **并发行为牙**：两区各造 K 件、收集全部 `make_index`，断言**无重复且计数 == 2K**（行为证据，非文本牙）。
- **接受一处偏离（明确记）**：N>1 下号码的**跨区交错序**每次运行可能不同（全局原子决定），属**数据而非顺序契约**，**不影响口径 A 三面**（同屏可见 / 帧内容取值 / 同收件人相对序）。

## 4. 第一批可分流族：排序 + 依赖（**form (d) 三硬边界逐条**）

第一批（不需等裁决）建议**排序**（状态耦合少→多 / 已存在→需新接）：
1. **Closed / LoadDb**（form (a)，**已存在**，零新接）；
2. **selloff**（form (d)，`book` 单点 + 玩家区会话；同区）；
3. **guild**（form (d)，`guilds`/`cfg`/`sections` + 玩家区会话）；
4. **shop**（form (d)，npc 宿主 `!Send` + `item_number` + 玩家区；**须 §3 已共享**）；
5. **click-npc**（form (d)，脚本宿主 `!Send` 最重 + 玩家区）；
6. **say-GM@**（form (d)+A6，M1b 已处置）。

form (d) 族**必须逐条守三硬边界**（名字）：
- **(1) 持目标区锁期间不重入、不跨区锁任何 `HostLoop`**（防自死锁 / ABBA）；
- **(2) 不取进程锁**（进程态是区 0 线程局部 `&mut`，非锁）；
- **(3) 不阻塞 DB**（存档 → `save_outbox` 异步出口）。

（A6 另需排水层 stop-and-wait，见 M1b 方案 §1.2/§5.3。）

## 5. 必须等方案：**阻塞哪一片**

- **item_number**（§3）：**阻塞 M1c-1**——区 k 首条「可造物品」路径（drop / drug / shop）开启那一片，**M1c-1 放行前必须在 master**。
- **kick / AdminGm**（§2）：**管理面**，不属某物品片；**阻塞"区 k 有真玩家后的管理命令片"（≈ M1c-1 或单列管理面）**——区 k 一有真玩家、踢人/管理就要跨区有效，否则空效。
- 其余（§4 的 form (d)/(a) 族）不被上述阻塞，可随 M1c 各片推进。

> 若 M1c-1 / M1c-2 的正式切片名与此不同，按实际片名替换。
