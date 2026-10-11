# M2/M3 接线证据：进图门「空 CurMap 新角色」+ `CM_EAT(1006)` 收包派发

> 卡：`openmir2-m3-enter-eat-wiring`（issue `8d07694d`，owner 派活 `d6e51325`）。
> 车道：`m2svr`（worktree `m2-svr`，branch `worktree-m2-gamesvr-host`），身份 `openmir2-worker-11`。
> 采集时刻：2026-10-12 06:19–06:27（Rust 栈窗口；MySQL 已先起，栈起/停见 `start-stack.txt` / `stop-stack.txt`）。

## 0. 结论速览

| 判据 | 结果 |
| --- | --- |
| ① Rust 栈上 `-FixedNewChr` 新角色进世界 | **GREEN**：`进世界 ChrName:m3r06240 CurMap:0 (287,616)`，**无**「拒绝进图」；`in_world=1`；`ident_counts` 含 658/51/50/41/213/11029/42/766/54/5008/52/752/811/46（14/14 自帧齐） |
| ② 空 CurMap 归因（行号 + DB 原值） | **落实**：oracle `WorldServer.cs:250-260` + `GetHomeInfo`（`:1384-1410`）；DB 原值 `MapName=NULL/HomeMap=NULL`（`db-rows.txt`） |
| ③ `CM_EAT(1006)` 派发 | **已接线**（`main.rs` dispatch + `useitem::use_item` + 立即帧/延时通知）；单测/进程级用例 + 红检见 `red-eat-makeindex.txt` |
| ④ 用例与牙齿 | `eat_drug_dispatches_and_updates_session`（进程级）；红检 = `make_index` 改从 `param`/置 0 ⇒ 必红（已复现） |
| ⑤ 四门禁 | **GREEN**：fmt / clippy `-D warnings` / `test --workspace`（535 passed / 0 failed / 16 ignored / 71 suites）/ msgcodegen 604（`gates.txt`） |
| ⑥ 窗口纪律 | 起栈前 5000/7000/7100/7200 全空；收工 `-Stop`，实拍只余 3306（`ports-before.txt` / `ports-after.txt`） |

**未达成（如实登记，见 §4）**：T-14 的「跑完同序列」还缺一条 **`CM_QUERYBAGITEMS(81)` → `SM_BAGITEMS(201)`**（m3bag 的 T-26 第 2 条，归 m2-svr）：Rust game-svr 不答 `CM_QUERYBAGITEMS` ⇒ 假人背包视图恒空 ⇒ 序列的 step=1..4 全 `skip`。本片 DoD 只点名「进图门 + `CM_EAT` 派发」两条，故**不在本片实现**（避免无对拍向量地手写 `ClientItem` 序列化）。

## 1. 缺陷现场与根因（判据②）

### 1.1 DB 原值（决定性：空 `CurMap` 不是 Rust 解码问题）

`-FixedNewChr` 建出的新角色，DBSrv 写库时 `MapName`/`HomeMap` 都是 **NULL**：

```text
Id     LoginID   ChrName    MapName    CX  CY  Level HomeMap HomeX HomeY CREATEDATE
13356  m3r06240  m3r06240   [<NULL>]   0   0   0     NULL    0     0     2026-10-12 06:25:08   ← Rust 侧本轮新角色
13354  m3b16010  m3b16010   [0]        649 620 1     0       289   618   2026-10-12 05:49:01   ← C# 基线（进世界落库后）
```

（原始件 `db-rows.txt`；取数命令见 `commands.txt`。）

写库链（oracle）：`src/DBSrv/Services/Impl/UserService.cs:259-280` 的 `NewChrData` 只填
`ChrName/Account/Sex/Job/Hair`，其余用默认值；`src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Add.cs:94-108`
把 `hd.CurMap`/`hd.HomeMap` 原样 insert（新角色即空）。⇒ **`CurMap=""` 是数据的真实值，不是 Rust 读错**。

### 1.2 oracle 对空 `CurMap` 的处置（行号）

`src/GameSrv/Word/WorldServer.cs`：

```text
:1105  GetHumData: playObject.MapName = humData.CurMap;   // 空
:1127               playObject.HomeMap = humData.HomeMap; // 也空
:250   if (string.IsNullOrEmpty(playObject.HomeMap))
:254       playObject.HomeMap = GetHomeInfo(playObject.Job, ref homeX, ref homeY);
:257       playObject.MapName = playObject.HomeMap;        // ← 忽略空 CurMap，落到出生点
:258/9     playObject.CurrX = GetRandHomeX(playObject);    // = HomeX - 2 + Random(3)（:1414-1421）
```

`GetHomeInfo`（`:1384-1410`）：`StartPointList` 非空 ⇒ 取 `StartPointList[I]`（`I = Random(StartPointSize)`，`StartPointSize` 缺省 2）；
空 ⇒ `Config.HomeMap/HomeX/HomeY`（缺省 `"0"/289/618`）。
另有 `:319-341` 的次级回落：`FindMap(MapName)==null ⇒ Abil.HP=0 ⇒ MapName=HomeMap`。

⇒ **oracle 不拒绝空 `CurMap`**。Rust 缺的正是这条回落，两侧才不等价。

### 1.3 Rust 侧修法与口径（本片）

- 新模块 `mir2-rs/bins/game-svr/src/homemap.rs`：1:1 复刻 `LocalDB.LoadStartPoint`（`:822-867`）的
  `Envir/StartPoint.txt` 解析 + `GetHomeInfo` 的回落。**GBK 解码**（`mir2_envir::text::decode_text`）——
  现网首行是 GBK `;安全区/出生点`，用 `read_to_string` 会静默得 0 条（本轮真机第一次起栈即踩到）。
- `config.rs`：新增 `[Setup] StartPointSize/HomeMap/HomeX/HomeY`（oracle 缺省 `2/"0"/289/618`）。
- `main.rs`：`full` 可解且 `HomeMap` 空 ⇒ `enter_map_name = get_home_info()`、坐标取 `HomeX-2/HomeY-2`；
  `resolve_enter_map`（`mapwire.rs`）先试 `primary` 再试记录 `HomeMap`，都找不到才 fail-closed。
- **确定性口径**：oracle 用 `Random(StartPointSize)`；本宿主取 `I=0`（真实 `StartPoint.txt` 前 `StartPointSize`
  条的 `MapName` 都是 `0` ⇒ 进图地图与随机无关；坐标的 `-2 + Random(3)` 取 `Random=0` 下界）。见 `homemap` 模块头。

**本轮实测**（`game-svr-enter-lines.txt` / `bot-rust.txt`）：

```text
22:24:42.128 [INF] 出生点回落已载入：StartPoint.txt 8 条，StartPointSize=2，选中=Some(0)，Config.HomeMap=3 (839,668)
22:25:08.742 [INF] 收到角色数据 queryId=1 Ident=1100 Recog=1
22:25:08.743 [INF] 进世界 ChrName:m3r06240 CurMap:0 (287,616) Dir:0 entity=1
```

`(287,616)` = `StartPoint[0] (289,618) - 2`，与 C# 侧落库的 `HomeX=289/HomeY=618` 一致。

## 2. 真机 ①：两侧 `ident_counts` 并排

| ident | 658 | 51 | 50 | 41 | 213 | 11029 | 42 | 766 | 54 | 5008 | 52 | 752 | 811 | 46 | 10 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| C# 基线 | 1 | 1 | 1 | 4 | 1 | 1 | 2 | 1 | 1 | 1 | 2 | 2 | 1 | 1 | 2 |
| Rust（本片） | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 0 |

- 原始件：`load-stats-csharp-baseline.ndjson`（C# 基线，取自 `m3bag` 第十二格会话 `csharp-20261012-054849`，
  即 `bots-20261012-054852/load_stats.ndjson`）、`load-stats-rust.ndjson`（本片）。
- `frames_decoded`：C# 40、Rust 22；`in_world`：两侧均 1。
- `10 SM_TURN`：单客户端时是 **AOI 帧**、本就不该出现（owner §22 裁决）⇒ Rust=0 正确，单列。
- 判据要求「至少 658/50/41/42/46/51/52/752/213 齐」⇒ **齐**。

## 3. 真机 ⑥：端口与起停

- 起栈前（`ports-before.txt`）：只有 `3306`（本片预起的 MySQL）⇒ 5000/7000/7100/7200 全空。
- 起栈（`start-stack.txt`）：`RUST_STACK_OK`，game-svr 入口 = `E:\tmp\openmir2-m2svr-target\debug\game-svr.exe`（Rust，非 `E:\MirServer`）。
- 收工 `-Stop`（`stop-stack.txt`）：`ports-after.txt` 只剩 `3306`。
- 假人驱动：`tools/botload/run_bots.ps1 -Count 1 -Prefix m3r0624 -NewAccount -Port 7000 -FixedNewChr -NewChrJob 0 -NewChrHair 2 -NewChrSex 0 -BagEquipSequence -BagEquipStepMs 1000 -BagEquipStartDelayMs 3000 -RequireWorld` ⇒
  `GREEN: 1/1 登录成功 (100%)；in_world=1/1`（`bot-run-stdout.txt`）。

## 4. 未达成 / 登记（不静默）

**T-26 第 2 条（`CM_QUERYBAGITEMS`）⇒ T-14「跑完同序列」仍受阻**：

- 假人收到 `SM_LOGON` 后发 `CM_QUERYBAGITEMS(81)`（`src/BotSrv/Player/RobotPlayer.Operate.cs:281`）；
  oracle 在 `PlayObject.Message.cs:784-795` 处理 → `ClientQueryBagItems()`（`PlayObject.Operate.cs:37-64`）
  → `SM_BAGITEMS(201)`（`EncodeBuffer(ClientItem) + '/'` 每条）。
- Rust game-svr **未派发 `CM_QUERYBAGITEMS`**（`main.rs` 只派发 `CM_SAY`/`CM_PICKUP`/`CM_DROPITEM`/`CM_TAKEONITEM`/`CM_TAKEOFFITEM`/`CM_EAT`）
  ⇒ 假人背包视图恒空 ⇒ 序列 step=1..4 全 `skip`（`bot-rust.txt`）：

```text
M3BAGSEQ step=1 op=CM_TAKEONITEM skip=no_equippable_bag_item
M3BAGSEQ step=2 op=CM_TAKEOFFITEM skip=no_equipped_item
M3BAGSEQ step=3 op=CM_DROPITEM   skip=no_bag_item
（无 step=4 / 无 done；对比 C# 基线 markers 有 9 条含 step=4 CM_EAT make=29033300 金创药(小量)）
```

- 因此 `CM_EAT` 的真机闭环（step=4）在本片**跑不出来**；③ 的验收改由 **进程级用例 + 红检** 承担（DoD ④ 允许「进程级/单测」）。
- 该条是 **m3bag 第十二格 commit `f726fe76` 的 T-26 登记项**（原话：「不答 CM_QUERYBAGITEMS … 均 main.rs，归 m2-svr」），
  但**不在** `openmir2-m3-enter-eat-wiring` 的 DoD 清单里。`SM_BAGITEMS` 需 `ClientItem` 的 `EncodeBuffer`
  序列化（`src/OpenMir2/Packets/ClientPackets/ClientItem.cs` 的 `WritePacket`，~60 字段 + `WriteAsciiString`），
  属独立协议切片，**建议单开一片**（可复用本轮的真机栈与 `ClientItemCodec` 反序列化做逐字节对拍）。
- 另有 1 条**顺带**登记：`-FixedNewChr` 新角色的**新手属性**（oracle `WorldServer.cs:260-277` 的
  `Abil.Level==0 ⇒ HP/MP/MaxHP/MaxMP=15、DC/MC/SC=MakeLong(1,2)、MaxExp=100、MaxWeight=30、IsNewHuman`）
  本片未接 ⇒ `52/752` 的字段值可能与 oracle 不同（本片判据只要求 ident 齐；跨侧字节对拍属 m3bag/T-14）。

## 5. 四门禁原始输出（判据⑤）

见 `gates.txt`：`cargo fmt --all -- --check` = `FMT_OK`；`cargo clippy --workspace --all-targets -- -D warnings` 干净；
`cargo test --workspace --offline` = **535 passed / 0 failed / 16 ignored（71 suites）**；`msgcodegen --check` = `604 条常量`。
隔离 target：`CARGO_TARGET_DIR=E:\tmp\openmir2-m2svr-target`。

## 6. 红检原始输出（判据④）

- `red-eat-makeindex.txt`：把 `make_index: recog` 改成 `make_index: 0` ⇒ `eat_drug_dispatches_and_updates_session`
  FAILED（收到 `[(0,636)]`=SM_EAT_FAIL，期望 `[(0,635),(7,53),(0,622)]`）。已还原（`git status` 无 RED-CHECK 残留）。
- `red-enter-fallback.txt`：把 `resolve_enter_map` 的 fallback 支去掉（只单发 `resolve_map_id(primary)`）⇒
  `enter_map_falls_back_to_home_map` FAILED（`MapNameNotFound("")`）。已还原。
- 另：`homemap::tests::load_start_points_decodes_gbk_real_file` 把 GBK 解码改回 `read_to_string` ⇒ 必红（真机踩到过）。

## 7. 文件清单

| 文件 | 说明 |
| --- | --- |
| `ports-before.txt` / `ports-after.txt` | 起栈前 / 收工后十端口实拍 |
| `start-stack.txt` / `stop-stack.txt` | `start_rust_stack.ps1` 起 / 停原始输出 |
| `game-svr.out.txt` / `game-svr-enter-lines.txt` | Rust game-svr 全量日志 / 进图关键行 |
| `bot-rust.txt` / `bot-run-stdout.txt` | 假人日志 / 驱动输出 |
| `load-stats-rust.ndjson` / `load-stats-csharp-baseline.ndjson` | 两侧 `ident_counts` 原始件 |
| `db-rows.txt` | `characters` 表新角色 vs C# 基线行 |
| `red-eat-makeindex.txt` / `red-enter-fallback.txt` | 两条红检原始输出 |
| `gates.txt` | 四门禁原始输出 |
| `commands.txt` | 复现命令（含 hash 口径） |
