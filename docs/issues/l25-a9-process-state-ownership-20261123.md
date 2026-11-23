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

## 2. 【单列·给总监裁】kick / AdminGm：跨区目标的定位与加锁

- **现状（host-only）**：`handle_kick_account` 用 **区本地 `online`**（区 0 的）；`execute_admin_gm_command` 用 **区本地 `ground_sessions`/`world`/`online`**。**不含区 k 的玩家**。
- **"目标 actor 属于哪个区"的可判据**：进程级 **`ZoneRouter::lookup_registered(line, socket) -> Option<ZoneId>`**（`zone_router.rs:98`）——给定目标的 `(网关线, socket)` 即得区。**但**要拿到目标的 `(line,socket)`：kick 只有 `account`、AdminGm 只有 `chr`，`online` 是**每区一份、无全局 account/chr→(line,socket) 索引** ⇒ **仍须跨区搜**（各查本区 online，或广播）。
- **不解决会怎样（今天 / M1c 前）**：
  - **kick**：区 0 的 online 无区 k 账号 ⇒ **空踢（静默无效，玩家不被踢）**——**不是误踢/踢错区**（按 account 精确匹配），但**真要踢的跨区玩家踢不掉**。
  - **AdminGm**：目标在区 k ⇒ 区 0 的 `ground_sessions`/`world` 无他 ⇒ **GM 命令静默空操作**（对区 k 玩家无效）。
- **修法方向（(d) 变体）**：先**定位**目标区（`ZoneRouter` 若有 socket / 否则广播或建全局索引），再**区 0 持目标区锁**执行（form (d)，如登录）。**kick 我先前推荐"广播 + 各区自查"**（低频、幂等、不建全局索引）；AdminGm 同款定位。**这两族是"M1c 前必裁"项**。

## 3. 【单列·给总监裁】item_number 跨区撞号

- **分配点**：`*item_number += 1`（`lib.rs` add_newbie `~7442`、`drop_full_bag_items` `~10787/10843`、GM 给物 `~27103/27168`、`useitem.rs` 造药/拆包、shop 买）。`make_index: *item_number`。
- **分配器住哪**：**`HostServices.item_number: i32`（每实例一份，`&mut i32` 逐 handler 传入）**——**非全局、非共享**。区 0/区 k 各持自己的 `item_number`，从**同一初值**（`cfg.item_number`）起 `++`。
- **N 区后会不会两区发同一个号**：**会**。区 k 造件（drop/drug/shop，M1c 才会有）用区 k 计数、区 0 造件用区 0 计数 ⇒ **两件同 MakeIndex** ⇒ `bag.rs` `make_index` 身份匹配错 / 跨区引用/落库冲突。
- **是否已有全局序列可用**：**无**。C# 是 `M2Share.GetItemNumber()` 全局 `++`；Rust 是 per-`HostServices` `i32`。**需改 `Arc<Mutex<i32>>` 共享**（我方 §11/M1c 第一件事已建议）——改后撞号消失、值随跨区分配序波动但落 **D-1 掩码**（`whitelist.md` D-1：MakeIndex 对拍不含、DB 列掩码；C# 含 GetTickCount 无对拍真值）⇒ **不破帧面**。
- **是否单立卡**：**建议并入 M1c 第一件事（改共享计数器）**，不必单立；但若 M1c 先开物品前不共享 ⇒ 必红（撞号），故是 **M1c 放行条件**。

## 4. 结论：哪几族能第一批分流、哪几族等方案

- **第一批可分流（方案清楚）**：
  - **form (d)**：`guild`(#1)、`shop`(#2)、`click-npc`(#3)、`selloff`(#4，同区)、`say-GM@`(#5) —— 区 0 持区 k 锁跑，受 **§3 三硬边界**（不重入 HostLoop / 不取进程锁 / 不阻塞 DB）+ A6 排水层约束。
  - **form (a)**：`Closed`(#8)、`LoadDbRequest`(#9) —— 数据携带，**已存在**。
- **必须等方案定（单列给总监裁）**：
  - **kick / AdminGm**（#6/#7）—— 跨区目标定位（ZoneRouter/广播/全局索引）+ form (d)；
  - **item_number 撞号** —— 改 `Arc<Mutex>` 共享（M1c 第一件事）。

## 5. 给总监裁（2 条）
1. **kick / AdminGm 跨区定位方案**（§2）：广播+各区自查 vs 建全局 account/chr→(line,socket) 索引 vs 仅 ZoneRouter（需 socket）；未定则跨区踢/GM 静默失效。
2. **item_number 共享**（§3）：M1c 第一件事改 `Arc<Mutex<i32>>`？抑或另立卡；不改则区 k 造件即撞号。

