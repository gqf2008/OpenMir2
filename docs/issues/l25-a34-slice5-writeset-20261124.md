> 原路径 E:/UserTemp/a34-slice5-writeset.md（2026-11-24 入仓，内容未改）
> 入仓时锚已规范化（oracle `src/**` 行号保留、本仓高 churn 文件改符号锚），结论与事实未改；原文锚对应 `250c8a97d`。
> 本件写于实施前；对应实现已合入 `796e4f325`（merge `250c8a97d`，A34 片 5）；§0 的"现状=Unsupported"是当时的快照，不是当前状态。
# A34 片 5（元宝寄售 7 条，档 2）——写集与依赖地图

> 只读准备件（不写码、不编译）。基准：master `e8c32dfe3`（A34 片 4 已合）。本文件只在 `E:/UserTemp`，不入库。
> oracle = 主检出 `src/**`；Rust = `mir2-rs/`（`selloff.rs` + `lib.rs`）。

---

## 0. 现状（区 k 行为）

- `zone_cmd_class`：7 条 `CM_SELLOFF*` **不在**任何清单（Local 链/Process 块/`CM_SAY`）⇒ 表外
  `Unsupported`；`handle_zone_command_on` 尾部 `note_zone_cmd_unsupported` + 丢弃、零帧。
- 区 0 侧：`INBOUND_PROCESS` 里 `matches!(cmd.ident, CM_SELLOFFADDITEM | CM_SELLOFFDELITEM |
  CM_SELLOFFCANCEL | CM_SELLOFFEND | CM_CANCELSELLOFFITEMING | CM_SELLOFFBUYCANCEL | CM_SELLOFFBUY)`
  臂已实现（调 `handle_selloff_cmd`，`mir2-rs/bins/game-svr/src/lib.rs` `run_selloff_command`）。
- 区 k 的两张表：`sell_off_sessions`（HostLoop 字段，**区本地**）与 `sell_off_book`
  （`ProcessServices.sell_off_book`，**进程级**）；`handle_zone_command` 的测试包装对区 k 传
  `None`/空表（`mir2-rs/bins/game-svr/src/lib.rs` `cancel_cross_zone_trade_and_selloff` 注释已登记"区 k 全 Unsupported ⇒ 两张表恒空"）。
- 现有测试：`selloff.rs` 的纯函数牙 + `lib.rs` 的派发/帧牙（区 0）；**没有**把 7 条 ident 当
  Unsupported 的守恒断言（已 grep）。片 3/4 不动它们。

---

## 1. 逐命令表（7 行）

字段口径：12B 头（recog/param/tag）+ 尾 `Msg`；`CM_SELLOFFADDITEM/DELITEM` 的 `recog` =
`MakeIndex`、尾 = 物品名；`CM_SELLOFFEND` 的尾 = 买家名、`param` = 售价、`tag` = 元宝数；
`CM_SELLOFFBUYCANCEL/BUY` 的尾 = 卖方名。**无 DB 请求**（`SellOffBook` 进程内存 + 会话散字段；
落库不属本路径）。

| # | ident / 名字 | oracle 处理器（`src/**`） | 现区 k | 依赖的**进程态** | 建议归属 | 依赖卡 |
|---|---|---|---|---|---|---|
| S1 | `CM_SELLOFFADDITEM(23002)` | `PlayObject.Message.cs:2057-2077` 派发 → `PlayObject.Sale.cs:60-125` `ClientAddSellOffItem` | Unsupported | 会话 `SellOffState`（区本地）+ `catalog`（区副本）；**不读 book** | **Process**（A6） | A6 |
| S2 | `CM_SELLOFFDELITEM(23007)` | `:127-183` `ClientDelSellOffItem` | 同上 | 会话 `SellOffState` + 背包（区本地） | **Process** | A6 |
| S3 | `CM_SELLOFFCANCEL(23012)` | `PlayObject.Sale.cs:431-434` `ClientCancelSellOff`（同族 `sell_off_cancel` `:516-591`） | 同上 | 会话 `SellOffState` + 背包 | **Process** | A6 |
| S4 | `CM_SELLOFFEND(23015)` | `:516-591` `ClientSellOffEnd`（**上架**：会话列表 → 进程 `SellOffBook`） | 同上 | **`SellOffBook`（进程级）** + 会话 + 背包 + `SellOffConfig` | **Process** | A6 |
| S5 | `CM_CANCELSELLOFFITEMING(23024)` | `:185-252` `ClientCancelSellOffIng`（取消自己上架中的条目/退回；oracle 缺陷照搬：分支无 else 静默丢） | 同上 | **`SellOffBook`** + 会话 | **Process** | A6 |
| S6 | `CM_SELLOFFBUYCANCEL(23025)` | `:254-279` `ClientBuyCancelSellOff`（买家撤单） | 同上 | **`SellOffBook`** + 会话 | **Process** | A6 |
| S7 | `CM_SELLOFFBUY(23026)` | `:281-385` `ClientBuySellOffItme`（**成交**；oracle 的 `j<=9` 越界 ⇒ `BUY_OK` 恒不可达，物品已入包/元宝已扣） | 同上 | **`SellOffBook`（进程级）** + **卖方会话**（`ground_sessions`/`sell_off_sessions`，按名从 `online` 解析） | **Process** | A6；**跨区卖方缺口**见 §5 |

**为什么是 Process 而不是 Local / 新通道**：`SellOffBook` 是 `M2Share.SellOffItemList` 的进程级等价物
（`SellOffBook` 单实例在 `ProcessServices`），区 k 不可复制；频率低（寄售操作），沿用 A6 控制邮箱
的 stop-and-wait 可接受。**不需要** A20 那种非阻塞通道。

---

## 2. 写集与与片 3/片 4 的重叠点

### 可复用机制（片 4 `GuildProcess` 就是模板）

片 4 已落地：`Process` 分类块 → `ControlPayload::GuildProcess{ident,tail}` → `ControlRequest` 变体
（`zone/chr_name/ident/tail/at_tick/epoch` + 四处 accessor）→ `a6_enqueue_control` 臂 →
`a6_dispatch_local` 的 Process 分支（**在 `GmSay` 之前**）→ 区 0 drain 臂持目标区 k 锁执行 →
`run_guild_command`（区 0 `INBOUND_PROCESS` 与 drain **共用一份装配**，refactor 提交）。

片 5 完全同构：**建议也做两个提交**——`refactor:`（抽 `run_selloff_command(...)`，区 0 臂改调它，
行为/日志逐字不变）+ `feat:`（区 k Process 路由 + 7 条执行）。

### R1 `mir2-rs/bins/game-svr/src/lib.rs`

| 位置（符号锚） | 改动 | 与片 3/片 4 的重叠 |
|---|---|---|
| `zone_cmd_class` 的 **Process 块** | 追加 `matches!(ident, 7 条 CM_SELLOFF*) ⇒ Process`（或 `SELLOFF_IDENTS` 常量，与 `GUILD_IDENTS` 并列） | **片 4 刚在紧邻位置加了 guild 块**（同一个 `if` 链附近）⇒ 机械相邻；按"只追加、两边保留"解。片 3 不动 Process 块。 |
| `ControlPayload`（enum） | 新增 `SellOffProcess { ident: u16, recog: i32, param: u16, tag: u16, tail: Option<String> }`（无 series：`handle_selloff_cmd` 不读） | 与片 3/4 不重叠（它们不加同变体） |
| `ControlRequest`（enum）+ `zone()/chr_name()/at_tick()/epoch()` 四处 match | 新增变体与四臂 | 不重叠 |
| `a6_enqueue_control`（payload→request match） | 新臂 | 不重叠 |
| `a6_dispatch_local` 的 `ZoneCmdClass::Process` 分支（ident 分派处） | 新增：7 条 ⇒ `SellOffProcess`（**在 guild 臂与默认 `GmSay` 之间**；顺序自由但**必须**在 `GmSay` 前） | **片 4 同处已加 guild 臂** ⇒ 相邻 append；片 3 不动 |
| 区 0 控制邮箱 drain 大 match | 新臂：持目标区 k 锁，调 `run_selloff_command`（区 k 的 `world/online/ground_sessions/sell_off_sessions/gates` + 进程级 `sell_off_book` + `enter_cfg.sell_off/catalog`） | 不重叠（片 3/4 未加该变体） |
| 区 0 `INBOUND_PROCESS` 的 selloff 臂 | 改为调 `run_selloff_command`（refactor 提交；行为/日志逐字不变） | 不重叠（片 3/4 不动） |
| 测试模块（`zone_command_tests` 等） | 新增 §4 的牙 | 与片 4 测试同为 append ⇒ 同模块冲突（两边都留） |

**不改**：`selloff.rs`（7 条纯函数完备）、`goldsales.rs`、`inbound.rs`。

### 与片 3（交易 6 + 组队 4）的冲突点

- **`zone_cmd_class`**：片 3 在 `ident_level` 链尾追加 10 条 Local；片 5 在 Process 块追加 7 条 ——
  同一函数、不同子块（片 3 已合入，片 5 追加不冲突，但同文件高 churn，解冲突原则不变）。
- **`handle_zone_command_on`**：片 3 加 Local 臂；片 5 **不改它**（Process 走邮箱）。
- **`a6_dispatch_local`**：片 3 的签名变更（`trade_sessions` 可变）已合入；片 5 只加 Process 分支内
  一个 ident 分支，不重叠。
- **`ControlPayload/ControlRequest`**：片 3/4 与片 5 各自新增变体；片 4 已占 `GuildProcess`，
  片 5 用 `SellOffProcess`，**名字不可复用**，四处 accessor match 各加一行。

---

## 3. 帧面（ident 与收件人）

| 命令 | 产帧（`selloff_event_body` 映射） | 收件人 |
|---|---|---|
| S1 ADDITEM | `SM_SELLOFFADDITEM_OK(23003)` / `SM_SellOffADDITEM_FAIL(23005)` | **自己** |
| S2 DELITEM | `SM_SELLOFFDELITEM_OK(23008)` / `SM_SELLOFFDELITEM_FAIL(23010)` | 自己 |
| S3 CANCEL | `SM_SellOffCANCEL(23014)`（`RM_SELLOFFCANCEL(23013)` ⇒ 229） | 自己 |
| S4 END | `SM_SELLOFFEND_OK(23016)` / `SM_SELLOFFEND_FAIL(23018)`（330/235 队列） | 自己 |
| S5 CANCELSELLOFFITEMING | 成功/失败支按模块实现（复用 `SM_SellOffCANCEL(23014)`/SysMsg 口径，以 `selloff.rs` 事件为准） | 自己 |
| S6 BUYCANCEL | 取消帧（复用 `SM_SellOffCANCEL(23014)` 口径，以 `selloff.rs` 事件为准） | 自己 |
| S7 BUY | `SM_SELLOFFBUY_OK(23027)` **oracle 不可达**（`j<=9` 越界，物品已入包/元宝已扣）；卖方侧可能出 `RM_GAMEGOLDCHANGED`/`SM_WEIGHTCHANGED`/`SM_ADDITEM`/SysMsg；买方 `RM_GAMEGOLDCHANGED` + 立即 `SM_ADDITEM` | **买方自己** + **卖方**（`target = seller.actor_id`；卖方必须是**执行区 online** 里能解析到的人） |

- **没有 `SM_GUILDMESSAGE(104)` 类广播**；除 S7 的卖方帧外全部只回自己；S7 的卖方帧由
  `online.find_linear_actor(target)` 直发（本区）。
- 帧序：立即 `SM_ADDITEM`（`SendAddItem`）先于排队帧；排队帧按 `(byte)RM` 升序出队（模块头已有的
  优先级表，实施时直接复用现有 `selloff_event_body` + 事件序，不新造）。

---

## 4. 牙草案（每条：同区成功 / 库中不存在 / 邮箱满）

统一夹具：`ZoneState::new(zone_k, …)` + `SharedServices`（注入 `sell_off`/`catalog`）+ 会话 A（买卖双方各
一名，`sell_off_sessions` + `ground_sessions`）；`SellOffBook` 进程级实例；`gates` 单通道。

1. **S1 ADDITEM 成功**：A 背包有 `MakeIndex=9001` 的物品，送 `recog=9001` + 尾=物品名 ⇒ 恰一条
   `23003`、`sell_off_sessions[A]` 临时列表 +1；`zone_cmd_unsupported` 不增。
   **库中不存在**：`recog=9999` ⇒ 恰一条 `23005`（FAIL）、列表不变。
2. **S2 DELITEM**：先 ADD 再 DEL ⇒ `23008` + 列表 -1；DEL 不在列表 ⇒ `23010`。
3. **S3 CANCEL**：ADD 两件后 CANCEL ⇒ `23014` + 列表清空 + 物品回包。
4. **S4 END 上架**：会话列表 1 件 + 合法售价 ⇒ `book.entries` +1、`23016`；售价越界/未确认
   ⇒ `23018`，book 不变。
5. **S5/S6 取消上架/买家撤单**：book 条目存在 ⇒ 对应帧 + 条目移除/复位；**库里不存在** ⇒ 零帧
   （纯函数早退）。
6. **S7 BUY 同区**：A 上架、B 买 ⇒ B 元宝扣、`book` 条目 flag 置位、物品入 B 包（`SM_ADDITEM` 立即）、
   **卖方 A 收到元宝变更帧**；`SELLOFFBUY_OK` 不可达（oracle bug 照搬，**不得**因测试期望去"修"）；
   **库中不存在**（名字不匹配）⇒ **零帧**（函数 rposition 早退）；卖方缺席（见 §5）⇒ 买方付款、
   卖家不进账、条目 flag=1。
7. **邮箱满守恒**（三条命令任选其一）：预填 `CONTROL_MAILBOX_CAP` ⇒ 丢新（返回 false）、
   `control_pending` 不变、`zone_cmd_unsupported` 不增（Process 归属证据）。

**变异点**（实施时逐条实测）：
- 分类器删 selloff Process 块 ⇒ 分类/路由牙红（Unsupported）。
- dispatch 删 `SellOffProcess` 分支（落 `GmSay`）⇒ 路由牙红。
- S4 不写 `book` ⇒ 上架牙红。
- S7 不扣买方元宝 ⇒ 成交牙红；S7 不给卖方加元宝 ⇒ 卖方帧牙红。
- 队列判满失效 ⇒ 邮箱满牙红。
- 「库中不存在」牙：把 rposition 条件放宽为 `true`（不看名字）⇒ S7 零帧期望变成功帧 ⇒ 红
  （若签名可行；不可行时如实标"回归守卫"，与片 4 镜像牙同口径）。

---

## 5. 已知限制（原句进卡）

> **寄售 7 条本片只做「同区可执行」；`SellOffBook` 虽是进程级（列表跨区可见），但
> 成交/取消需要卖方会话在**执行区**的 `online` 里。** 具体：
> - **S7 买方在区 k、卖方在别区**：卖方解析（`online.iter()`）不到 ⇒ `seller=None` ⇒ **买方仍扣元宝、
>   条目 flag=1、物品入买方包**，**卖方不进账**（oracle 单世界不会发生）。这是**可见差异**
>   （卖家元宝丢失），登记而不是含糊。
> - 卖方侧的元宝变更/重量/物品帧只在**执行区**发（`online.find_linear_actor`）；别区卖方的观察帧不发。
> - 不做跨区寄售、不做行会/交易跨区（另卡）、不做 DB 落库（本路径 oracle 也无 DB）。
> - 走 A6 stop-and-wait；不产 `SM_GUILDMESSAGE(104)` 类广播。

---

## 6. 裁定（opc-27 2026-11-24，开工令 base=`e4ad16f8e`）

1. **`j<=9` 越界 bug 照抄**：`SM_SELLOFFBUY_OK(23027)` 不可达，注释/报告写明「oracle 越界 bug，不得顺手修」。
2. **卖方缺席 ⇒ fail-closed（本卡唯一偏离，必须登记）**：执行区 `online` 找不到卖方 ⇒ **拒绝成交**（不扣元宝、
   不给物品、条目不变），计数 `zone_selloff_buy_seller_absent` + 限速 WRN，买方收到「现有寄售失败帧中最贴近的一条」。
   - **oracle 事实**（`PlayObject.Sale.cs:303-317`）：`GetPlayObject == null`（单世界 = 卖方离线）时
     `Flag=1`、买方**仍扣元宝**、物品**仍发给买方**；随后 `for j<=9` NRE 使 `BUY_OK/MENU_OK` 不可达。
     卖方的补偿靠**登录时** `GetSellOffGlod()`（`:387-429`，`PlayObject.cs:1486`）——**本仓该登录点未接线**
     （`selloff.rs` 模块头已登记）⇒ Rust 里卖方离线是**永久蒸发**，oracle 只是**延期**。
   ⇒ fail-closed 同时覆盖「跨区」与「离线」两种「执行区找不到卖方」；登记为偏离（跨区寄售未实现，先防蒸发）。
   - **失败帧**：复用同函数「元宝不足」失败支的 `RM_MENU_OK` ⇒ `SM_MENU_OK`、文本
     `[错误] 您的申请提交不成功`（`mir2-rs/bins/game-svr/src/selloff.rs` `client_buy_sell_off_item` 已在用，字节一致）；**不自造文本**。
3. 其余按 §1–§5；两提交 `refactor:`（抽 `run_selloff_command`）+ `feat:`。

### 6.1 修正裁定（opc-27，oracle 事实核后）

- **不 fail-closed，照抄 oracle `Flag=1` 延期结算**：卖方缺席（离线/别区）⇒ 买方扣元宝、物品入包、条目
  `flag=1`、**零 `BUY_OK`/`MENU_OK`**（`j<=9` NRE 照抄）；`SM_MENU_OK` 方案与 `zone_selloff_buy_seller_absent` **作废**。
- **登录补偿接线（硬要求）**：`get_sell_off_gold`（`selloff.rs` 已有 1:1，原 `#[cfg(test)]`）改生产可见；
  新增 `pay_selloff_on_login(...)`，在 **host 登录** 与 **区 k 登录** 两个完成点各调一次（卖方在登录所在区结算）。
- **模块头错说明订正**：付费路径**不读 `SaleDeal`**（只有 `UpdateSellOffInfo(0)` 清理段早退）；错说明会误导后人。
- 计数列 **`zone_selloff_sold_offline_seller`**（卖方缺席成交数，插在 `zone_metrics` 之前）。
- `sale_deal=false` 时 `flag==2` 旧条目不清理：与 oracle 同形残余，**不顺手改**。
