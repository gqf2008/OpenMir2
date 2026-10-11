# M2/M3 接线证据：进图门「空 CurMap 新角色」+ `CM_QUERYBAGITEMS`/`SM_BAGITEMS` + `CM_EAT(1006)` + 新角色新手物品

> 卡：`openmir2-m3-enter-eat-wiring`（issue `8d07694d`，owner 派活 `d6e51325`；owner 后续扩到**四条**：`576d81d5` ②、`f703dc66` ④）。
> 车道：`m2svr`（worktree `m2-svr`，branch `worktree-m2-gamesvr-host`），身份 `openmir2-worker-11`。
> 采集：2026-10-12 06:19–06:47（Rust 栈；MySQL 先起，起/停见 `start-stack.txt` / `stop-stack.txt` / `ports-*.txt`）。

## 0. 结论速览（四条全闭环）

| 卡项 | 判据 | 结果 |
| --- | --- | --- |
| ① 进图门（空 CurMap 的新角色走出生点） | 栈上 `-FixedNewChr` 新角色进世界、无「拒绝进图」、`in_world=1`、14 条自帧齐 | **GREEN**（`game-svr-enter-lines.txt` / `load-stats-rust.ndjson`） |
| ② `CM_QUERYBAGITEMS(81)` → `SM_BAGITEMS(201)` | 回帧头 + 尾（`EncodeBuffer(ClientItem)+'/'`）；bot 侧 `bag_view>0` | **GREEN**：server `4 件（680 字节）`；m3bag 驱动 `SM_BAGITEMS body_len=668 bag_view=4`（**与 C# 基线 body_len=668 一致**，`bot-m3bag-markers.txt`） |
| ③ `CM_EAT(1006)` 派发 | `main.rs` 收到 1006 → `useitem::use_item` → `SM_EAT_OK/FAIL` + 通知 | **GREEN**：`CM_EAT 结果 eaten=true immediate=1 notices=1` + `CM_EAT 已下发 2 帧`（`game-svr-server2-lines.txt`）；红检 `red-eat-makeindex.txt` |
| ④ 新角色 4 件新手物品 | `ItemList` = 蜡烛→金创药(小量)→木剑→布衣(男/女) | **GREEN**：server `ItemList=4 件`；红检 `red-newbie-order.txt` |
| ⑤ 四门禁 | fmt / clippy / test / msgcodegen | **GREEN**：分支树 541/0/16（71 suites）+ **合并树 549/0/16（72 suites）** + 604（`gates.txt` / `gates-merged.txt`） |
| ⑥ 窗口纪律 | 起栈前四端口全空；收工 `-Stop` 只余 3306 | **GREEN**（`ports-before.txt` / `ports-after.txt` / `stop-stack.txt`） |

**RNG 抽数账本（owner `784be562`/`020acba0`，本片已补）**：进图门走 oracle `WorldServer.cs:254-259` 的
**同序 3 抽**——`GetHomeInfo` 的 `1×Random(StartPointSize)`（`Count > StartPointSize` 时）+ `GetRandHomeX/Y`
各 `1×Random(3)`；用宿主既有的 `crates/world::World.rng`（`mir2_shared::rng::RandomNumber`）。
少抽会让**后续 RNG 流偏移 3 抽**（M3②③ 固定种子攻击序列对不上）。测例
`new_human_spawn_consumes_three_draws_in_oracle_order` 钉住次数与次序；红检 `red-rng-draws.txt`。

## 1. 缺陷现场与根因（①）

### 1.1 DB 原值（决定性：空 `CurMap` 不是 Rust 解码问题）

`-FixedNewChr` 建出的新角色，DBSrv 写库时 `MapName`/`HomeMap` 都是 **NULL**：

```text
Id     ChrName    MapName    CX  CY  Level HomeMap HomeX HomeY CREATEDATE
13356  m3r06240   [<NULL>]   0   0   0     NULL    0     0     2026-10-12 06:25:08   ← Rust 侧
13354  m3b16010   [0]        649 620 1     0       289   618   2026-10-12 05:49:01   ← C# 基线（落库后）
```

（`db-rows.txt`。）写库链：`src/DBSrv/Services/Impl/UserService.cs:259-280` 的 `NewChrData` 只填
`ChrName/Account/Sex/Job/Hair`；`src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Add.cs:94-108`
把 `hd.CurMap`/`hd.HomeMap` 原样 insert（新角色即空）。

### 1.2 oracle 的处置（行号）

`src/GameSrv/Word/WorldServer.cs`：

```text
:1105  playObject.MapName = humData.CurMap;               // 空
:1127  playObject.HomeMap = humData.HomeMap;              // 也空
:250   if (string.IsNullOrEmpty(playObject.HomeMap))
:254       playObject.HomeMap = GetHomeInfo(playObject.Job, ref homeX, ref homeY);
:257       playObject.MapName = playObject.HomeMap;        // ← 忽略空 CurMap，落到出生点
:258/9     playObject.CurrX/Y = GetRandHomeX/Y(playObject);// = Random(3) + Home(X|Y) - 2
:262-277   if (Abil.Level == 0) { 新手 Abil 初始化; IsNewHuman = true; }
```

`GetHomeInfo`（`:1384-1410`）：`StartPointList` 非空 ⇒ `StartPointList[I]`（`I = Count > StartPointSize ? Random(StartPointSize) : 0`）；
空 ⇒ `Config.HomeMap/HomeX/HomeY`（缺省 `"0"/289/618`）。加载：`LocalDB.LoadStartPoint`（`:822-867`，`GameApp.cs:233` 调用）。

### 1.3 Rust 侧（本片）

- `mir2-rs/bins/game-svr/src/homemap.rs`（新）：`StartPoint.txt` 解析（**GBK**，`mir2_envir::text::decode_text`）+ `GetHomeInfo` 回落。
- `config.rs`：`[Setup] StartPointSize/HomeMap/HomeX/HomeY`（缺省 `2/"0"/289/618`）。
- `mapwire.rs`：`resolve_enter_map`（primary 失败且有非空 `HomeMap` ⇒ 次级回落，都找不到 fail-closed）。
- `main.rs`：`full` 可解且 `HomeMap` 空 ⇒ 出生点覆盖 `MapName` + 坐标。

实测（`game-svr-enter-lines.txt`）：

```text
出生点回落已载入：StartPoint.txt 8 条，StartPointSize=2，选中=Some(0)，Config.HomeMap=3 (839,668)
进世界 ChrName:m3r06240 CurMap:0 (287,616) Dir:0 entity=1     ← (287,616) = (289,618)-2，与 C# 落库 HomeX/HomeY 一致
```

## 2. ① 两侧 `ident_counts`（自帧 15 条）

| ident | 658 | 51 | 50 | 41 | 213 | 11029 | 42 | 766 | 54 | 5008 | 52 | 752 | 811 | 46 | 10 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| C# 基线 | 1 | 1 | 1 | 4 | 1 | 1 | 2 | 1 | 1 | 1 | 2 | 2 | 1 | 1 | 2 |
| Rust | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 0 |

`in_world` 两侧均 1；`10 SM_TURN` 是 AOI 帧（单客户端不出现，owner §22 裁决）。
原始件：`load-stats-csharp-baseline.ndjson`（C# 基线，m3bag 第十二格 `csharp-20261012-054849`）、`load-stats-rust.ndjson`（本片）。

### 2b. 帧面差额逐条归属（owner `c40f7460` 要求；C# 40 帧 vs Rust 本片 23 帧）

| 缺的 ident | 名称 | 归属 |
| --- | --- | --- |
| `201` | `SM_BAGITEMS` | **本片已闭**（②）：第二轮 Rust 已收 `201`×1（`load-stats-rust2.ndjson`） |
| `657`×3 | `SM_CHARSTATUSCHANGED` | **未实现**：oracle `RecalcAbilitys` 尾部 `StatusChanged()`（`PlayObject.cs:2875-2876`）与 buff/状态变化触发；Rust 的 `CharStatus` buff 部分未移植（会话初生恒 0，`enter.rs` 已登记） |
| `100`×4 | `SM_SYSMESSAGE` | **路径差异/未实现**：oracle 在背包与吃药步骤向客户端发了几条提示（`SysMsg`）；Rust 的 `SysMsg` 只在顶号/禁药等路径发，M3BAGSEQ 各步无对应提示 |
| `10`×2 | `SM_TURN` | **AOI/同屏帧**：oracle 单客户端进图时发 2 条是「看见另一个 actor」（同屏 NPC/怪），属世界侧 AOI 展示路径（owner §22 已裁） |
| `654` | `SM_CHANGELIGHT` | **未实现**：oracle 由 `RM_CHANGELIGHT`（光环/装备发光变化，`PlayObject.Message.cs:1838`）发；Rust 未接发光变化触发 |

（这四条各自都是与①无关的帧面缺口，逐条登记，不静默。）

## 3. ② ④ ③ 真机证据（owner `f7ff82c8` 的 02/04 复核）

### ② `CM_QUERYBAGITEMS` → `SM_BAGITEMS`
oracle：`PlayObject.Message.cs:784-795`（30s 限频）→ `ClientQueryBagItems()`（`PlayObject.Operate.cs:37-64`）
→ `MakeMessage(SM_BAGITEMS, ActorId, 0, 0, ItemList.Count)` + 尾 `EncodeBuffer(ClientItem)+'/'`。
Rust：`itemwire.rs`（`ClientItem.WritePacket` 1:1，`ClientItem.cs:22-84`）+ `main.rs::handle_query_bag_cmd`。

server 侧（`game-svr-server2-lines.txt`）：
```text
CM_QUERYBAGITEMS：ChrName=m3r06460 ItemList=4 件
SM_BAGITEMS 下发：ChrName=m3r06460 socket=328 gate=0 4 件（680 字节）
```
bot 侧（m3bag 驱动，`bot-m3bag-markers.txt`）：`M3BAGSEQ SM_BAGITEMS body_len=668 bag_view=4`
—— **`body_len=668` 与 C# 基线逐字节长度相同**，`bag_view=4`（蜡烛/金创药(小量)/木剑/布衣）。

### ④ 新手物品
oracle：`PlayObject.cs:1205-1245`（`IsNewHuman` ⇒ 顺序 `Candle → BasicDrug → WoodenSword → ClothsMan/Woman`，
模板名取自 `[Setup]`，配置字段见 `GameSvrConf.cs:1301-1305`）。
Rust：`main.rs::add_newbie_items`（`GameItemSystem.CopyToUserItemFromName` 的 `find_by_name` + `Dura=DuraMax=tpl.DuraMax`）。
附带修：`bag_items_from` 补上 oracle 的 `Index > 0` 门（`WorldServer.cs:1187-1189`；新角色记录里 46 个 `StdIndex=0` 空槽此前被当成物品 ⇒ `ItemList` 虚高）。

bot 侧五步不再 skip（`bot-m3bag-markers.txt`）：
```text
M3BAGSEQ snapshot bag_view=4 takeon=6 takeoff=1 drop=6 eat=1
step=1 op=CM_TAKEONITEM bag=6 where=1 make=406900 name=木剑
step=2 op=CM_TAKEOFFITEM where=1 make=406900 name=木剑
step=3 op=CM_DROPITEM bag=6 make=406900 name=木剑
step=4 op=CM_EAT bag=1 make=406899 stdmode=0 name=金创药(小量)
done
```

### ③ `CM_EAT`
oracle：`PlayObject.Message.cs:826`（`ClientUseItems(nParam1, Msg)`）→ `PlayObject.Operate.cs:601`；纯判定在 `useitem::use_item`。
Rust：`handle_eat_cmd`（`make_index=recog`；立即帧 `SM_EAT_OK/FAIL`；通知 `RM_WEIGHTCHANGED`/`RM_HEALTHSPELLCHANGED`/… → `SM_*`）。
```text
CM_EAT 结果：ChrName=m3r06460 make_index=406899 eaten=true immediate=1 notices=1
CM_EAT 已下发 2 帧（socket=328 gate=0）
```

> **观察（登记，不判红）**：bot 侧 `ident_counts` 里没有 `635/622`（Rust 与 **C# 基线都没有**，见两份
> `load-stats-*.ndjson`）——服务端已产出并下发（日志为证），bot 侧未计到属两侧共有的驱动/组帧现象，
> 不是本片的服务端差异；跨侧字段级 diff 归 m3bag 的 T-14。

## 4. 未达成 / 登记（不静默）

1. **新手 Abil 初始化**（`WorldServer.cs:262-277`：`Level==0 ⇒ AC/MAC=0、DC/MC/SC=MakeLong(1,2)、
   HP/MP/MaxHP/MaxMP=15、Exp=0、MaxExp=100、Weight=0、MaxWeight=30`）：**本片未接** ⇒ 新角色 `52/752`
   的字段值可能与 oracle 不同（本片判据只要求 ident 齐；跨侧字节对拍归 m3bag/T-14）。
3. **`GetUpgradeStdItem` 的 desc 升级差量**（`GameItemSystem.cs:731-…` 的大 switch）：`itemwire` 只做
   `CopyItemToClientItem` 等价（默认物品 `Desc` 全 0 ⇒ 与该 switch 的 0 值退化一致）；带升级物品未覆盖。
4. **`MakeIndex` 生成**：oracle `GetItemNumber() = ++Config.ItemNumber + GetTickCount()`（`M2Share.cs:547`）；
   本片用单调计数器（无 tick 项）⇒ 跨侧 `MakeIndex` 不会相等（bot 用的是本侧服务端的值，故序列自洽）。
5. **限频命中时的 `SysMsg(MessageSettings.QUERYBAGITEMS)`**：`String.conf` 无该键 ⇒ 未建模（记 WRN）。

## 5. 四门禁原始输出（⑤）

`gates.txt`（分支树）：`cargo fmt --all -- --check` = `FMT_OK`；`cargo clippy --workspace --all-targets -- -D warnings` 干净；
`cargo test --workspace --offline` = **541 passed / 0 failed / 16 ignored（71 suites）**；`msgcodegen --check` = `604 条常量`。
`gates-merged.txt`（交付前把 master `459d2b37` 合入后的**合并树**）：**549 passed / 0 failed / 16 ignored（72 suites）** + `604`。
隔离 target：`CARGO_TARGET_DIR=E:\tmp\openmir2-m2svr-target`。

## 6. 红检原始输出

- `red-eat-makeindex.txt`：`make_index: recog` → `0` ⇒ `eat_drug_dispatches_and_updates_session` FAILED（得 `[(0,636)]`，期望 `[(0,635),(7,53),(0,622)]`）。
- `red-bagitems-series.txt`：`sm_bagitems` 的 `series` 改 0 ⇒ `query_bagitems_replies_sm_bagitems_with_items` FAILED（`left: 0, right: 4`）。
- `red-newbie-order.txt`：`add_newbie_items` 去掉 `basic_drug` ⇒ `newbie_items_follow_oracle_order_and_slots` FAILED（`left: [1,3,4], right: [1,2,3,4]`）。
- `red-enter-fallback.txt`：`resolve_enter_map` 去 fallback 支 ⇒ `enter_map_falls_back_to_home_map` FAILED（`MapNameNotFound("")`）。
- `red-rng-draws.txt`：`new_human_spawn` 去掉 `X/Y` 的 `Random(3)`（回到坐标下界）⇒ `new_human_spawn_consumes_three_draws_in_oracle_order` FAILED（第 4 抽 `140 != 522`）。
- `homemap::tests::load_start_points_decodes_gbk_real_file`：把 GBK 解码改回 `read_to_string` ⇒ 必红（真机第一次起栈即踩到）。

## 7. 文件清单

| 文件 | 说明 |
| --- | --- |
| `ports-before.txt` / `ports-after.txt` | 起栈前 / 收工后十端口实拍 |
| `start-stack.txt` / `stop-stack.txt` | `start_rust_stack.ps1` 起 / 停原始输出 |
| `game-svr.out.txt` / `game-svr-enter-lines.txt` | 第一轮（①）全量 / 关键行 |
| `game-svr-server2.out.txt` / `game-svr-server2-lines.txt` | 第二轮（②③④）全量 / 关键行 |
| `bot-rust.txt` / `bot-run-stdout.txt` | 第一轮假人日志（master BotSrv，无 ClientItemCodec ⇒ 仅证 ①） |
| `bot-m3bag.txt` / `bot-m3bag-markers.txt` | 第二轮假人日志（m3bag 驱动，含 `ClientItemCodec`） |
| `load-stats-rust.ndjson` / `load-stats-csharp-baseline.ndjson` / `load-stats-rust2.ndjson` | 两侧 `ident_counts` 原始件 |
| `db-rows.txt` | `characters` 表新角色 vs C# 基线行 |
| `red-eat-makeindex.txt` / `red-bagitems-series.txt` / `red-newbie-order.txt` / `red-enter-fallback.txt` / `red-rng-draws.txt` | 五条红检原始输出 |
| `gates.txt` / `gates-merged.txt` | 四门禁原始输出（分支树 / 合并 master `459d2b37` 后） |
| `commands.txt` | 复现命令（含 hash 口径） |
