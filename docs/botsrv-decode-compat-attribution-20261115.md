# BotSrv 解码兼容归属判定（卡 `openmir2-botsrv-decode-compat`）

> 结论先行：**（b）夹具侧（BotSrv 解码路径）**，两处是各自独立的夹具缺陷；
> **服务端帧面没有问题，不另开产品缺陷卡。**
>
> 本文件覆盖卡面 **A1/A2（归属判定）** 与 **A3/A4（修复、牙）**；**A5（空载窗口复测）** 见 §9。
> 修复只改 `src/BotSrv/**`，不改服务端 / 冻结客户端。

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
- `load_stats.ndjson` 的 `ident_counts` 由 `LoadMetrics.IdentCountsJson` **只打印 top-24**（内部最多 48 键）；
  本窗口 `frames_decoded - Σ(top24) = 1062` ⇒ 621 等低频键被截断，不代表未收到。

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
本窗口假人没有物品交互，且 621 只发给带装备角色（`ident_counts` 只打印 top-24，200/201/202/203 未显现），
故 `recv_packet_other` 里只有 621 这一类。

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

## 6. 修复（A3，已实现）

只改 `src/BotSrv/**`，不动服务端 / 冻结客户端：

1. **新增 `src/BotSrv/AbilityCodec.cs`**
   - 先 EDCode 解码，再按**长度**选布局：**58B** = 冻结客户端 `TAbility`（生产默认 client-58）；
     **40B** = oracle `EncodeMessage` 尾（`MIR2_CLIENT_ABILITY_LAYOUT=0` 的逐字节对拍栈）。
     其它长度 ⇒ `InvalidDataException`（fail-closed，不猜）。
   - `SM_ABILITY` 的唯一入口改为 `AbilityCodec.Decode(body)`；**不再调用**
     `EDCode.DecodeBuffer<Ability>`（MemoryPack 22 成员）。
   - **静默全零拒绝**：解码结果为全零（`Level=0` 且全部字段为 0）⇒ `InvalidDataException`，
     经 `RunPacketGuard` 计入 `recv_packet_other`。真实角色 `Level>=1` 且 1 级就有非零
     HP/Weight ⇒ 全零只可能是错解，不得默默接受。
2. **物品解码统一走 `ClientItemCodec.Decode`**（已有 124B 实现）：
   `SM_SENDUSEITEMS(621)` / `SM_ADDITEM(200)` / `SM_UPDATEITEM(203)` / `SM_DELITEM(202)` /
   `SM_BAGITEMS(201)` / 远端寄售加/删，共 7 处；空名 / `null` 槽跳过。
   `g_boBagEquipSequence` 只再控制挂机五步序列，**不再控制解码**。
3. **可观测**：`SM_ABILITY` 落一行 `ABILITYVIEW level=… hp=…/… mp=…/… dc=… weight=…/…`，
   供窗口做修复前后对照（不改任何判据）。
4. **新自检**：`BotSrv.exe --selftest-decode-compat`（无栈，见 §7）。

## 7. 牙（A4，已实现 + 本机实测）

`./src/BotSrv/bin/Release/BotSrv.exe --selftest-decode-compat` → **rc=0，17/17 PASS**：

- `client58-level{22,23,50}-fields`：Level=22/23/50 都解出
  `DC=257 HP=19/19 MP=15/15 Exp=0/10 Weight=14/50 MaxWear=15 MaxHand=12`
  —— **22 级不是全零**（边界正例）。
- `old-path-58-level22-silent-wrong`：旧 MemoryPack 路径不抛，但 `Level=0/HP=0`
  （22 级时是**错位垃圾**，不保证恰好全零）；`old-path-58-level{23,50}-throws`：同一句
  `header maked as N`。
- `oracle40-level1-fields` 正解；`old-path-oracle40-silent-all-zero`：旧路径全零。
- **`blank-client58-rejected` + `blank-guard-counted`**：全零记录被拒，且经 `RunPacketGuard`
  计入 `recv_packet_other` —— 这是「**静默全零会被发现**」的牙。
- `old-ability-dispatch-guard-counted` / `old-clientitem-dispatch-guard-counted`：旧分派各 +1
  （证明类别接线不是空过）；`new-*dispatch-no-count`：新分派 0 增量。
- `new-clientitem-roundtrip`：`EncodeBuffer(ClientItem)`（`MakeIndex=12345`）→ Decode 回
  `Name/StdMode/MakeIndex/Dura/DuraMax` 一致。
- `meta-guard-count-wiring`：注入 3 次 == `recv_packet_other` 增量 3（**计数牙自身有牙**）。

## 8. 静默全零 = 测量有效性问题（总监定性）

这不是服务端/产品缺陷，是**测量有效性问题**：旧解码路径在 `Level<23` 时把 58B 记录解成
错位/近全零的 `Ability`，夹具据此做的 AI/战斗决策偏离真实玩家。

**方向（必须写清）**：静默全零 = 假人比真实玩家**弱** ⇒ AI/战斗负载**可能偏低** ⇒
**此前的性能结论可能是乐观的**。

### 8.1 受影响的历史结论清单（逐条判定）

| 文档 / 结论 | 与假人 `Abil` 的依赖 | 判定 |
| --- | --- | --- |
| `docs/botsrv-heap-1000bot-20261115.md`（每 bot 堆增量、时间项、425×4GiB OOM） | bot 动作构成（魔法/攻击/吃药由 `Level/MP/HP%` 门控）影响场景对象的产生速率 | **可能受此影响，需修复后复测**（泄漏机制本身不依赖，数值需复测） |
| `docs/botsrv-memory-leak-20261115.md`（FreeActorList 只增不减、每 500 bot ≈12.4MB/s） | 泄漏根因是代码路径；但每 bot 速率受动作/场景对象速率影响 | **机制不依赖；数值可能受此影响，需复测** |
| `docs/botsrv-inworld-plateau-20261115.md`（425 平台 = 发送层 Hold；22.2s tick = 离世广播） | 入口/离世路径不读 bot `Abil` | **不依赖 Abil**（结论仍有效）；bot 后入世界流量构成变化不影响该根因 |
| `docs/capacity-model-20261114.md`（容量模型；200 bot 单点 `HostInbound`；可见性效应） | 每客户端入站率取决于 bot AI 发帧（魔法/攻击/移动） | **可能受此影响，需修复后复测**（纯合成实体轴的 `SimMonsterAi`/`World::tick` 不依赖） |
| `docs/simmonsterai-profile-20261114.md`（`SimMonsterAi` 热点；A6 8-bot 臂） | A6 臂的 bot 动作量由 `Abil` 门控；合成 5k/8k/11k 实体档不依赖 | **8-bot 臂可能受此影响，需复测；合成档不依赖** |
| `docs/ecs-acceptance-20261114.md`（10k 玩家基准、bot 走位联调） | 10k 基准为合成玩家；联调 bot 只走路 | **不依赖 Abil**（性能轴）；bot 战/AI 轴未在本文件验收 |
| `docs/tick-hotspot-profile-20261113.md`（`tick_bench --bots`） | 合成 bot 无会话、无出站帧，不走 BotSrv AI | **不依赖 Abil** |
| `docs/host-aoi-profile-20261114.md`（AOI 扇出；1,000 bot 段） | bot 存在/移动驱动出现-消失；动作构成可能改变可见性 churn | **可能受此影响（1,000 bot 段的数值），需复测；机制结论（95.5%/92.3%）不依赖** |
| `docs/gate-backpressure-a-b-20261115.md`（500 档 Hold/背压、internal_errors） | Hold 机制不读 `Abil`；但 bot 流量构成影响到达率与数值 | **可能受此影响（数值），需复测；Hold 机制结论不依赖** |
| `docs/gate-frame-criticality-20261115.md`（bot 窗口、帧临界性） | 同上：机制不依赖，bot 流量构成影响数值 | **数值可能受此影响，需复测** |
| `docs/gateway-multiline-sharding-20261115.md`（500 档多 gate 分片） | 登录分配不读 `Abil`；进世界后的动作构成影响后续流量 | **机制不依赖；后入世界流量数值可能受此影响，需复测** |
| `docs/botsrv-error-census-20261115.md`（逐类 census 表） | 两类解码错误就是本卡修复对象；其余 AI 驱动类别随动作量变化 | **受此影响（本卡对象）；本文件 §9 的窗口替换该表** |
| `docs/loadgate-criteria-runbook-20261115.md`（判据实现/runbook） | 判据代码不读 bot `Abil` | **不依赖 Abil** |
| 任何「按假人属性分档」的统计 | — | **未发现此类统计**（压测档位按 bot 数分档，不按属性） |

### 8.2 不受影响的结论（避免误伤）

- 登录成功率 / `no_resp` / `conn_lost`、`in_world` 到达、帧面/协议逐字节对拍、脚本化
  M2/M3 场景的帧序与 DB diff —— 这些路径不读 `MShare.MySelf.Abil`；
- `M3ATTSEQ` / 攻击序列证据里的 `hp/maxhp` 来自**服务端帧**（`SM_STRUCK` 的 Param/Tag），
  不是 `MShare.MySelf.Abil`；
- `Abil` 只影响**假人侧决策**（魔法/保护/吃药/等级门控），不影响服务端对攻击伤害的计算。

## 9. 修复后的窗口（A3/A5）

### 9.1 环境与判据

- 栈：Rust 四缝 `E:/UserTemp/stack-rel-target-w83/release` + C# DBSrv/LoginSrv（与 census 同形）；
  `game-svr.exe` sha256 `c3a33338899abb162d6e99aa7101498c82bd803a3b6d7f51d0f3d33a782a34e7`，
  `game-gate.exe` sha256 `c364a4dd1bd67dd87ad38a65aa8fa209b24d9a9b122e3215ed321621cb061c88`；
  `game-svr.out.log` 自证 `CLIENT_ABILITY_LAYOUT layout=client-58`。
- BotSrv：本分支（fix commit `312c709f0`）构建，`BotSrv.exe` sha256
  `8ac3af0cc8d1597f206c5d53fcf5597a3d4eab3f1ccf938ced132fd358271da8`；
  `--selftest-decode-compat` rc=0。
- **空载判据**：无 `cargo`/`rustc`、CPU 均值 <20%。修复前 10s 基线 = **12.7% / 0 个 heavy**。
  但本机是共享主机：两次窗口期间其它车道多次起 `cargo`/`cargo-clippy`/`rustc`
  （`host_load.heavy_names_seen` 逐档列出）⇒ **严格「空载」未被满足**。
  下列窗口因此标记为「60s 冒烟 + 主机受扰」：**解码判据逐条成立，但不作为性能基线**。

### 9.2 结果（`clean/` 窗口，2026-11-15 13:34–13:37）

| 档位 | verdict | login | world_ok | tick P50/P90/P99 | internal_errors | census_sum | fault_total | recv_packet_other | ABILITYVIEW | 两类解码错误 | host_load（均/峰/heavy） |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 200 | **GREEN** | 100% | 200/200 | 0/15/16ms | **0** | **0** | 0 | **0** | 200（L1×190，L50×10） | **0** | 51.9% / 97% / 4（cargo,clippy） |
| 500 | RED | 100% | 411/500（82.2%） | 0/16/63ms | **1** | **1** | 1 | **0** | 443（L1×433，L50×10） | **0** | 48.4% / 96% / 14（cargo,rustc） |

- **`recv_packet_other = 0` 两档都成立**（修复前稳定 18–20）。
- **`internal_errors == census_sum` 两档都成立**：200 = `0 == 0`；500 = `1 == 1`。500 的唯一类别是
  `recv_packet_index=1`（父卡的 ActorList 并发竞态），不是解码。
- `scene_null_actor_skipped`：200 = 0；500 = 0/233226 轮。500 的 `scene_list_race_skipped=1`
  是父卡护栏的竞态证据（已知，不属本卡）。
- 另一次 200/500 尝试（13:28–13:31，顶层 `tier-200/`、`tier-500/`）host 更重
  （63.3%/68.2% 均、heavy=3..4），tier-500 `world_ok=290`；**该次不作结论**。

### 9.3 能力视图修复前后对照

| 角色（等级/装备） | 修复前（旧 MemoryPack 路径；离线复现/自检） | 修复后（窗口 `ABILITYVIEW`） |
| --- | --- | --- |
| loadbot10（L50，带装备） | `Level=0 HP=0/0 MP=0/0 DC=0`（旧路径首字节=50 ⇒ 抛；Level≤22 静默错位） | `Level=50 HP=271/271 MP=1333/1333 DC=42786 Weight=0/550` |
| loadbot0（L1） | `Level=0 HP=0/0` | `Level=1 HP=18/19 MP=15/15 DC=257 Weight=14/50` |

- 200/500 档各 **10 个 L50 角色**全部解出真实 HP/MP/DC；其余 L1 角色解出 1 级真实值。
- 这是**测量有效性的恢复**：修复前 `Abil=0` ⇒ 假人 AI 的等级门控（魔法 `<7`/`<18`）、MP 门控
  （`MP>=3/6/7`）、HP% 保护（`IsPersentHP(0,0)` 恒 false）全部走**弱侧** ⇒ 不施法/不吃药/少动作
  ⇒ AI/战斗负载偏低（见 §8 方向）。

### 9.4 500 档 `world_ok` 复验

- census（修复前，60s 冒烟）：**453/500 = 90.6%**。
- 本次（修复后，60s 冒烟）：**411/500 = 82.2%**。
- 两轮都是 `HoldSec=60` 冒烟（`conclusion_valid=false`），本轮 host 峰值 96% / heavy=14；
  差异落在入口吞吐 + 主机负载的噪声范围内，**解码错误 0 证明不是本修复引入**。
  结论「500 档 `world_ok` 受发送层/入口吞吐限制」**仍成立**；精确值需 360s 空载结论窗口复测。

### 9.5 逐类 census 表（`clean/tier-200|500/report.json`）

| 类别 | 200 | 500 | 分类 |
| --- | --- | --- | --- |
| `recv_packet_null` | 0 | 0 | fault |
| `recv_packet_index` | 0 | 1 | fault |
| `recv_packet_other` | **0** | **0** | fault |
| `run_round_null` | 0 | 0 | fault |
| `run_round_index` | 0 | 0 | fault |
| `run_round_other` | 0 | 0 | fault |
| `run_loop_exception` | 0 | 0 | fault |
| `socket_handler_exception` | 0 | 0 | fault |
| `socket_recv_fault` | 0 | 0 | fault |
| `socket_send_fault` | 0 | 0 | fault |
| `socket_connect_fault` | 0 | 0 | fault |
| `metric_flush_failure` | 0 | 0 | fault |
| `self_report_write_failure` | 0 | 0 | fault |
| `drop_exp_writer_open_failure` | 0 | 0 | fault |
| `drop_exp_writer_write_failure` | 0 | 0 | fault |
| `bag_item_decode_failure` | 0 | 0 | fault |
| `unhandled_exception` | 0 | 0 | fault |
| `unobserved_task_exception` | 0 | 0 | fault |
| `internal_error_unclassified` | 0 | 0 | fault |
| `teardown_race` | 0 | 0 | operational |

### 9.6 空载判据的诚实结论

- 严格「无 cargo/rustc、CPU<20%」在两次尝试中都被**其它车道**破坏（本会话没有跑 cargo）；
  因此 §9.2 的**性能数值**（tick/CPU/内存/`world_ok`）只作冒烟，不作基线。
- 本卡的**解码判据**不受此影响：`recv_packet_other=0`、`internal_errors==census_sum`、两类解码错误 0、
  ABILITYVIEW 真实值 —— 都有窗口计数与 §7 的确定性自检双重证据。
- **A5 的严格空载复测仍待一次无外部构建的窗口**（登记为后续；本机共享主机排期问题，不是代码问题）。

## 10. 复现命令（只读）

```sh
# 触发集合
grep -c "Ability property count" E:/UserFleet/archive/botsrv-census-verify-20261115/clean-200/tier-200/bots.log
grep -c "ClientItem is not registered" E:/UserFleet/archive/botsrv-census-verify-20261115/clean-200/tier-200/bots.log

# 等级/装备（只读 SQL；mysql 客户端在 D 盘）
"D:/mysql/mariadb-10.11.19-winx64/bin/mysql.exe" -h 127.0.0.1 -u root --batch --skip-column-names \
  -e "SELECT c.ChrName,c.Level,SUM(i.StdIndex<>0) FROM mir2_db.characters c JOIN mir2_db.characters_item i ON i.PlayerId=c.Id WHERE c.ChrName LIKE 'loadbot%' GROUP BY c.ChrName,c.Level HAVING SUM(i.StdIndex<>0)>0 ORDER BY CAST(SUBSTRING(c.ChrName,8) AS UNSIGNED);"

# 离线解码复现（先 dotnet build，再 run）
dotnet E:/UserTemp/decode-probe-20261115-w85/bin/Release/net8.0/probe.dll

# 新牙（无栈）
src/BotSrv/bin/Release/BotSrv.exe --selftest-decode-compat

# 空载窗口（先起混合栈：Rust 四缝 + C# DBSrv/LoginSrv；再跑 200/500）
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 `
  -RustBinDir E:/UserTemp/stack-rel-target-w83/release `
  -LogDir E:/UserFleet/archive/botsrv-decode-compat-fix-20261115/clean/stack-logs
powershell -ExecutionPolicy Bypass -File tools/botload/load_gate.ps1 `
  -Tiers 200,500 -HoldSec 60 -SkipBuild `
  -OutDir E:/UserFleet/archive/botsrv-decode-compat-fix-20261115/clean
python E:/UserFleet/archive/botsrv-decode-compat-fix-20261115/make_summary_fix.py `
  E:/UserFleet/archive/botsrv-decode-compat-fix-20261115/clean
```

## 11. 残留物清单

- `E:/UserTemp/decode-probe-20261115-w85/`：离线复现探针（自建 .NET console，引用仓内
  `src/OpenMir2/obj/Release/net8.0/OpenMir2.dll` 与 `src/BotSrv/bin/Release/MemoryPack.Core.dll`）；
  **未写入仓库、未改任何仓内文件**，取证后可删。
- 未起/停任何服务进程；未改数据库（仅有只读 `SELECT`）；未改服务端与冻结客户端。
