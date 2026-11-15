# BotSrv 解码兼容归属判定（卡 `openmir2-botsrv-decode-compat`）

> 结论先行：**（b）夹具侧（BotSrv 解码路径）**，两处是各自独立的夹具缺陷；
> **服务端帧面没有问题，不另开产品缺陷卡。**
>
> 本文件只做只读定位（无窗口、不起栈、不改服务端/冻结客户端），对应卡面接受项 **A1/A2**；
> A3/A4/A5（修复、牙、空载窗口复测）见文末「修复方案」与「未完成项」。

- 执行身份：`openmir2-worker-85`（worktree `.worktrees/worker-85`）
- 判定方法：同 `+GD` 判定法 —— 先钉触发集合，再对照 oracle 编码、冻结客户端处理，最后离线复现异常。
- 证据根：窗口归档 `E:/UserFleet/archive/botsrv-census-verify-20261115/`；
  离线复现探针 `E:/UserTemp/decode-probe-20261115-w85/`（临时物，见末节）。

## 1. 触发集合：两类错误各恰好 10 条，且同时落在同一批角色上

窗口 `clean-200/tier-200` 的 `bots.log` 中，两类报错各 **10** 条，逐条映射到「上一条 `成功进入游戏` 的假人」：

| 假人 | 等级 | 装备槽（非零 StdIndex） | Ability 报错 | ClientItem 报错 |
| --- | --- | --- | --- | --- |
| loadbot10–loadbot19 | **50** | **11/13** | 各 1 条 | 各 1 条 |
| 其余 190 个 | 1 | 0/13 | 0 | 0 |

- 日志映射（`bots.log` 行号）：713/714→loadbot10、787/788→loadbot11、793/794→loadbot12、
  799/800→loadbot13、899/901→loadbot14、908/909→loadbot15、915/916→loadbot16、
  1021/1022→loadbot17、1027/1028→loadbot18、1033/1034→loadbot19。
- 等级/装备来自只读 SQL（`mir2_db.characters` + `mir2_db.characters_item`）：
  **等级 ≥23 的角色集合 = 带装备的角色集合 = loadbot10–19**，与报错集合逐条相同。
- 每对错误顺序恒为「Ability 先、ClientItem 后约 2ms」；这与登录队列的优先级/入队序一致
  （52 与 621 同为 High，52 先入队）。

> 这排除了「与负载无关的随机解码失败」：命中的是**角色数据形态**（等级 50、带装备），
> 不是夹具内部版本号，也不是服务端字段数。

## 2. 错误①「Ability property count is 22 but binary's header maked as 50」

### 2.1 机制

`SM_ABILITY(52)` 的体是 12B 命令头 + **EDCode 编码的 `TAbility` 原始记录**。BotSrv 的处理是：

- `src/BotSrv/Player/RobotPlayer.Operate.cs:630` —— `case Messages.SM_ABILITY` 调
  `EDCode.DecodeBuffer<Ability>(body)`。
- `src/OpenMir2/EDcode.cs:145` 的 `DecodeBuffer<T>` = 先 EDCode 解码，再
  `MemoryPackSerializer.Deserialize<T>`。
- `src/OpenMir2/Packets/ClientPackets/Ability.cs:8` 的 `[MemoryPackable]` 生成 formatter
  有 **22 个成员**；MemoryPack 把缓冲区**首字节当对象头（成员数）**。
- 服务端生产默认发的是冻结客户端的 **58B `TAbility`**，首字节 = `Level: Word` 的低字节。
  等级 50 ⇒ 首字节 `0x32 = 50` ⇒ MemoryPack 认为「二进制声明 50 个成员」，类型只有 22 ⇒ 抛
  `MemoryPackSerializationException`（原文即 census 里那句）。

### 2.2 离线复现（探针，与线上同一批 DLL）

用 `src/OpenMir2/obj/Release/net8.0/OpenMir2.dll` + `MemoryPack.Core.dll` 直接调用同一条解码路径：

```text
== Serialize<Ability> baseline ==
serialized len=41 hex=16-01-00-00-00-00-00-01-01-...   # 首字节 0x16=22 = 成员数
oracle wire (drop 1) len=40 first=1

== client-58 layout decoded as MemoryPack<Ability> ==
client58 level=1:  OK  abil=L=0 AC=0 DC=0 HP=0 MaxHP=0 ...   # 静默错解：全零
client58 level=22: OK  abil=L=0 AC=0 DC=0 HP=0 MaxHP=0 ...
client58 level=23: THROW ... Ability property count is 22 but binary's header maked as 23 ...
client58 level=50: THROW ... Ability property count is 22 but binary's header maked as 50 ...
```

结论：`50` **不是服务端声明的字段数**，是等级字节；错误门槛是「首字节 > 22」。
等级 1–22 的 190 个假人**不会抛**，但 `Ability` 被静默解成**全零/错位值** ——
比报错更隐蔽：夹具的 `WAbil` 视图一直不可信，只是没进 `recv_packet_other`。

补充：oracle 40B 尾（`EncodeMessage` 去掉对象头后的 40B）走同一条 `DecodeBuffer<T>` 同样静默错解为全零
（探针 `oracle40 level=1` → `L=0 AC=0 ...`）。**即该解码路径对 C# oracle 也从未正确**；
它与 `EncodeMessage` 的「丢首字节」约定不配套。历史提交（`git log -S "DecodeBuffer<Ability>"`）
显示这是 2019 年代移植代码，非本次回归。

### 2.3 对照：服务端发的是冻结客户端要的 58B

| 侧 | 依据 | 形态 |
| --- | --- | --- |
| 冻结客户端 | `Source/Common/Grobal2.pas` `TAbility`（`HIGHHP = True`） | `Level: Word` + 5×`Integer` + 4×`DWORD` + 2×`DWORD` + 6×`Word` = **58B** |
| 冻结客户端 | `Source/MirClient/ClMain.pas` `SM_ABILITY` 分支 | `DecodeBuffer(body, @g_MySelf.m_Abil, SizeOf(TAbility))` —— **整块拷 58B**，无对象头/无版本号 |
| C# oracle | `src/OpenMir2/Packets/ClientPackets/Ability.cs:8` + `src/OpenMir2/EDcode.cs:211` | `[StructLayout(Size=40)]`、`WriteObjectHeader(22)`，`EncodeMessage` 丢首字节 ⇒ **40B**（与 58B 布局本就不匹配，属已知 D-9 偏离） |
| Rust 服务端（生产默认） | `mir2-rs/bins/game-svr/src/record.rs` 的 `encode_ability_client`；`smframe.rs` 的 `sm_ability`；`enter.rs` 的 `ability_tail` | **58B client 布局**（`MIR2_CLIENT_ABILITY_LAYOUT` 未设 ⇒ client-58） |
| 真机证据 | `mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/01-oracle-anchors.txt`、`183-52-752-53-rootcause.txt` | client-58 ⇒ HUD `19/19`、`CM_RUN>=1`；oracle-40 ⇒ HUD `0/0`、`CanRun` 拒跑 |

窗口栈自证：`stack-logs/game-svr.out.log` 首行
`CLIENT_ABILITY_LAYOUT layout=client-58（MIR2_CLIENT_ABILITY_LAYOUT=<unset>）`。

**所以服务端帧没有「头声明 50、实际 22」的不符**：线上体就是 58B 记录，
`50` 是 BotSrv 用 MemoryPack 视角误读的首字节（Level=50）。

## 3. 错误②「ClientItem is not registered in this provider」

### 3.1 机制

- 等级 50 的角色带装备 ⇒ 登录时服务端发 `SM_SENDUSEITEMS(621)`（oracle 的 `SendUseItems`，
  Rust 侧 `enter.rs` 的 `send_use_items_tail`；**尾为空不发**，所以只有带装备的 10 个角色收到）。
- BotSrv 处理：`src/BotSrv/Player/RobotPlayer.cs:5768`（`ClientGetSendUseItems`）对每槽
  `EDCode.DecodeBuffer<ClientItem>(data)` —— 又是 MemoryPack 反序列化。
- 但 `src/OpenMir2/Packets/ClientPackets/ClientItem.cs:6` 的 `ClientItem : ClientPacket`
  **没有 `[MemoryPackable]`**，也从未注册 formatter ⇒ MemoryPack 抛
  `ClientItem is not registered in this provider`。

同样的错误面还有 `SM_ADDITEM(200)` / `SM_UPDATEITEM(203)` / `SM_DELITEM(202)` /
远端寄售两条（`src/BotSrv/Player/RobotPlayer.cs:5371/5384/5423/6352/6361`）；
`SM_BAGITEMS(201)` 走 `DecodeClientBuffer<ClientItem>`（`src/BotSrv/Player/RobotPlayer.cs:5587`），
而 `ClientPacket.ToPacket` 把 `ReadPacket` 的 `NotImplementedException` 吞掉返回 `null`
（`src/OpenMir2/Packets/ClientPackets/ClientItem.cs:20`）—— 背包视图恒空，静默漏覆盖。
本窗口只收到 621（未收到 200/201/202/203），故只计数了 621 这一类。

### 3.2 离线复现

```text
ClientItem.GetBuffer len=124 nameBlock15B first=4 MakeIndex@100=12345   # 124B 二进制布局
EDCode.EncodeBuffer(ClientItem) len=166
EDCode.DecodeBuffer<ClientItem>: THROW MemoryPack.MemoryPackSerializationException:
    OpenMir2.Packets.ClientPackets.ClientItem is not registered in this provider.
```

即：只要服务端发的是合法的 `EncodeBuffer(ClientItem)`（oracle 与 Rust 都是），
BotSrv 这条路径**必抛**，与帧内容无关。

### 3.3 对照：真实客户端按固定 124B 记录读，没有「注册表」概念

| 侧 | 依据 | 形态 |
| --- | --- | --- |
| 冻结客户端 | `Source/MirClient/ClMain.pas` `SM_SENDUSEITEMS` → `ClientGetSendUseItems` | 每槽 `DecodeBuffer(data, @cu, SizeOf(TClientItem))` —— **整块拷 124B** |
| 冻结客户端 | 同上，槽号 `Index in [0..U_FASHION]` | 只有槽位概念，**没有物品类型注册表**；未知 id 也只是记录里的字段值 |
| C# oracle | `src/M2Server/Player/PlayObject.Operate.cs` 的 `ClientQueryBagItems`；`src/OpenMir2/EDcode.cs:187` | `new ClientItem()` + `EDCode.EncodeBuffer(clientItem)`（`WritePacket` 124B 逐字段） |
| Rust 服务端 | `mir2-rs/bins/game-svr/src/itemwire.rs` 的 `encode_client_item` / `encode_client_item_prepared`；`bagitems_tail` | 同一个 124B 布局（`MakeIndex@100` 已在探针钉住） |

**所以不是「服务端发了客户端未知的物品 id」**：线上根本没有「id 注册」这一层；
错误来自 BotSrv 用 MemoryPack 解一个本来就不是 MemoryPack 的类型。

## 4. 归属结论（对应卡面两问）

1. **「22 vs 50」**：不是机器人端模型版本旧，也不是服务端发的字段数与协议头声明不符。
   服务端按冻结客户端布局发 58B `TAbility`（首字节 = 等级）；`50` 是等级 50，
   `22` 是 BotSrv 的 MemoryPack `Ability` 成员数。**指向夹具解码路径，不指向服务端。**
   附带发现：等级 1–22 的角色不抛但被静默错解为全零，夹具 `WAbil` 视图长期不可信。
2. **「ClientItem is not registered」**：不是「服务端发了客户端未知的物品 id」。
   夹具用 MemoryPack 解 124B `ClientItem` 二进制记录；真实客户端与两侧服务端都用固定记录，
   没有类型注册表。**指向夹具解码路径。**

按三选一：**（b）夹具解码版本/路径缺失（夹具侧）**。
不属于（a）产品级：冻结客户端对这两处的处理分别是 `SizeOf(TAbility)`（58B）与
`SizeOf(TClientItem)`（124B）整块拷，服务端两处字节布局都与之一致；
也不属于（c）「客户端另有容错」——客户端没有容错层，只是根本不使用 MemoryPack。

## 5. 根因（BotSrv，文件:行）

| 位置 | 问题 |
| --- | --- |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:630` | `SM_ABILITY` 用 `EDCode.DecodeBuffer<Ability>`（MemoryPack）解 58B/40B 原始记录 |
| `src/BotSrv/Player/RobotPlayer.cs:5768` | `SM_SENDUSEITEMS` 每槽用 `EDCode.DecodeBuffer<ClientItem>` |
| `src/BotSrv/Player/RobotPlayer.cs:5371/5384/5423` | `SM_ADDITEM/SM_UPDATEITEM/SM_DELITEM` 同类 |
| `src/BotSrv/Player/RobotPlayer.cs:6352/6361` | 远端寄售加/删物品同类 |
| `src/BotSrv/Player/RobotPlayer.cs:5587` | `SM_BAGITEMS` 用 `DecodeClientBuffer`（`ReadPacket` 恒抛 ⇒ 静默 null，背包视图恒空） |

已有正确实现：`src/BotSrv/ClientItemCodec.cs` 的 `ClientItemCodec.Decode`（124B，与
`ClientItem.WritePacket` 1:1），但只在 `-BagEquipSequence` 打开时启用；
`Ability` 没有对应的 codec。

## 6. 修复方案（本卡 write_set `src/BotSrv/**` + `docs/`）

### 6.1 新增 `src/BotSrv/AbilityCodec.cs`（按 EDCode 解码后的长度选布局）

- **58B（生产默认 client-58）**：`Level` u16@0；`AC/MAC/DC/MC/SC` i32@2/6/10/14/18；
  `HP/MP/MaxHP/MaxMP` u32@22/26/30/34；`Exp/MaxExp` i32@38/42；
  `Weight/MaxWeight/WearWeight/MaxWearWeight/HandWeight/MaxHandWeight` u16@46/48/50/52/54/56。
  C# 专有字段 `Reserved1/Reserved2/ExpCount/ExpMaxCount` 置 0。
- **40B（`MIR2_CLIENT_ABILITY_LAYOUT=0` 的 oracle 尾）**：按 MemoryPack 成员序**去掉对象头**后读
  （Level u8@0、Reserved1 u8@1、AC..MaxMP u16@2..18、Reserved2 i16@20、ExpCount u8@22、
  ExpMaxCount u8@23、Exp i32@24、MaxExp i32@28、Weight/MaxWeight u16@32/34、
  Wear/MaxWear/Hand/MaxHand u8@36..39）。
- 其它长度：保持 fail-closed（保留现有路径并计数，不静默）。
- `SM_ABILITY` 分支改为调用该 codec。

### 6.2 物品解码统一走 `ClientItemCodec.Decode`

- 替换 `RobotPlayer.cs:5371/5384/5423/5768/6352/6361` 的 `DecodeBuffer<ClientItem>`；
  `5587` 的 `DecodeClientBuffer<ClientItem>` 也替换，并对返回 `null`（空名槽）跳过。
- `g_boBagEquipSequence` 保留，仅控制挂机五步序列，**不再控制解码**。
- 行为影响：旧路径要么抛、要么恒 `null`；新路径只是让夹具看到服务端实际发出的字节。
  但 `MShare.UseItems/ItemArr` 会开始被填充（全局静态、多假人共享）——见「残留风险」。

## 7. 牙（A4 设计）

1. **AbilityCodec 正例**：58B Level=50 + 已知字段 → 解出 `Level=50/HP=19/MaxHP=19/Weight=14/MaxWeight=50`；
   40B oracle 尾 → 解出对应字段，**不得全零**。
2. **AbilityCodec 负例（钉旧路径）**：同一 58B 输入经 `EDCode.DecodeBuffer<Ability>` 必抛，
   且断言 message 关键子串（`property count is 22 but binary's header maked as 50`）；
   经新 codec 不抛且字段正确。边界用 Level=22/23 各一例（旧路径 22 不抛、23 抛；新路径两者都正解）。
3. **ClientItemCodec 正例**：`EncodeBuffer(ClientItem)`（`MakeIndex=12345`）→ Decode 回
   `Name/MakeIndex/Dura/DuraMax` 相同；**负例**：`EDCode.DecodeBuffer<ClientItem>` 必抛同一句。
4. **端到端自检 `--selftest-decode-compat`**：构造完整 `SM_ABILITY(52)` 与
   `SM_SENDUSEITEMS(621)` 帧（12B 头 + EDCode 尾），走 `RunPacketGuard` 等价入口：
   修复前 `recv_packet_other` +2 且 `MShare` 未被正确赋值；修复后 0 且字段正确。
   自检必须**先证旧分派会红**（用旧解码路径跑一次并断言计数 +1），再证新路径绿。
5. **窗口复测（A5）**：空载 200 档，`load_stats.ndjson` 的 `recv_packet_other` 必须 0；
   同时 `ident_counts` 中 `52` 仍为 200（覆盖不降）。

## 8. 未完成项 / 残留风险

- **A3/A5 未验证**：本卡本次只做只读定位，未改代码、未起栈。`recv_packet_other=0` 与窗口复测需修复后在空载窗口跑。
- **A4 未落地**：牙为设计稿，随修复提交。
- **全局静态 `MShare`**：`UseItems/ItemArr/Abil` 是全进程共享；修复后这些值开始被真实填充，
  可能改变挂机/自护/用药行为，需窗口观测。若影响其它对拍，应改为每会话状态（更大改动，另开卡）。
- **T-14 口径**：`ClientItemCodec` 从「仅 `-BagEquipSequence`」变为默认解码，会改变默认背包视图；
  需确认不破坏 T-14 既有对拍（其五步序列仍由开关控制）。
- **探针临时物**：`E:/UserTemp/decode-probe-20261115-w85/`（自建 console 工程，仅引用仓内 DLL，未改仓库）；
  归档后可删。

## 9. 复现命令（只读）

```sh
# 触发集合
grep -c "Ability property count" E:/UserFleet/archive/botsrv-census-verify-20261115/clean-200/tier-200/bots.log
grep -c "ClientItem is not registered" E:/UserFleet/archive/botsrv-census-verify-20261115/clean-200/tier-200/bots.log

# 等级/装备（只读 SQL；mysql 客户端在 D 盘）
"D:/mysql/mariadb-10.11.19-winx64/bin/mysql.exe" -h 127.0.0.1 -u root --batch --skip-column-names \
  -e "SELECT c.ChrName,c.Level,SUM(i.StdIndex<>0) FROM mir2_db.characters c JOIN mir2_db.characters_item i ON i.PlayerId=c.Id WHERE c.ChrName LIKE 'loadbot%' GROUP BY c.ChrName,c.Level HAVING SUM(i.StdIndex<>0)>0 ORDER BY CAST(SUBSTRING(c.ChrName,8) AS UNSIGNED);"

# 离线解码复现（先 dotnet build，再 run）
dotnet E:/UserTemp/decode-probe-20261115-w85/bin/Release/net8.0/probe.dll
```

## 10. 残留物清单

- `E:/UserTemp/decode-probe-20261115-w85/`：离线复现探针（自建 .NET console，引用仓内
  `src/OpenMir2/obj/Release/net8.0/OpenMir2.dll` 与 `src/BotSrv/bin/Release/MemoryPack.Core.dll`）；
  **未写入仓库、未改任何仓内文件**，取证后可删。
- 未起/停任何服务进程；未改数据库（仅有只读 `SELECT`）；未改服务端与冻结客户端。
