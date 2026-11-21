# magic MG-34 / MG-36 / MG-55 实现批卡（新观测面落地）

> 基线 master `0e2eb1316`（配置批已合入：`parity_serverconf.rs` 十键入口在位）。
> 实现规范：[`magic-observe-surface-plan.md`](magic-observe-surface-plan.md)——MG-34 用其 v3.3 + v3.1 三条增强；
> MG-36 用其「5 格 + 安全区」定稿口径；MG-55 用其清单。
> **本卡只增装置/驱动/比较器**；唯一可能的产品改动是 MG-34 的 `RM_MAKEHOLYSEIZEMODE(60005)` 编码与发送（§3.5，需 Leader 单独批）。
> 不碰 `src/**` oracle 代码。装置文件 `MagicGolden/Program.cs` 当前另有两线在改：**只加新函数 + 一行 dispatch + 新 spec 字段**。

## 0. 判据归属（配置批的教训，先读）

| 层 | 负责什么 | 禁止 |
| --- | --- | --- |
| 装置（C# `MagicGolden`） | 产 golden（输入快照 + 术后观测 + 逐收件人 `Sent`）；**契约 fail-closed**（前提不满足 ⇒ rc=2 不落盘） | 把"期望帧序/期望值"写进生成期断言 |
| Rust 驱动 | 读 golden `Fixture` 输入 → 设世界/配置 → 执行 `handle_spell_cmd` → 产 ledger（**只记录，不判断**） | 在驱动里断言"配置==部署"（那是比较器的事） |
| 比较器 | ledger vs golden 逐字段 + **由输入推导**的期望（如 `wParam=(L*5+10)*1000`） | 用"模型自算"的期望替代 golden |

反例（配置批栽过）：把判据记到驱动层，导致"读了配置"与"吃缺省"不可分。本批三条的配置/推导面一律写进比较器，并保留输入快照。

---

## 1. MG-55（46 分身术）— 工作量档位：**小**

### 1.1 oracle 锚（一条一个位置；全路径）
- `src/M2Server/Magic/MagicManager.cs:1337` —— `MagMakeClone`
- `src/M2Server/Player/PlayCloneObject.cs:11` —— `ChrName = "Clone"`
- `src/M2Server/Player/PlayCloneObject.cs:14` —— `Dir = GetBackDir(...)`
- `src/M2Server/Player/PlayCloneObject.cs:19` —— `SendRefMsg(RM_TURN, ...)`
- `src/M2Server/Actor/BaseObject.cs:820` —— `GetBackDir`
- `src/M2Server/Actor/BaseObject.cs:463` —— 基类 ctor `ActorMgr.Add(this)`（收件人 handler 可取到克隆体）
- `src/M2Server/Player/PlayObject.Message.cs:1542` —— `SM_TURN` 构造
- `src/M2Server/Player/PlayObject.Message.cs:1555` —— `SM_FEATURECHANGED(41)` 支
- `src/M2Server/Player/PlayObject.cs:3395` —— `GetFeature`（克隆体无 UseItems）

### 1.2 Rust 现状
- **已实现**：`bins/game-svr/src/slave.rs` 的 `cast_clone_spell`；`crates/world-bevy/src/slave.rs` 的 `CloneSpawn`/`spawn_clone`；
  `bins/game-svr/src/downlink.rs` 的 `clone_display`；`bins/game-svr/src/downlink.rs` 的 `turn_pair`（含 10+41）。
- **缺**：装置 `CloneBatch`、Rust 驱动 `clone` ledger 块、比较器 `clone` 块。**产品代码不用改。**

### 1.3 判据归属
- **装置**：新 `CloneBatch`（新函数 + 一行 dispatch）。Fixture 记 `caster_dir/gender/hair`、`fast_train:false`（显式设置）、
  `train_level[level]/max_train[level]/tran_point_before`；逐收件人 `Sent` 过滤 `SM_TURN(10)`/`SM_FEATURECHANGED(41)`
  （**不新增记录机制**）；扫 `Envir` 图对象记克隆体字段（相对 id）。
- **驱动**：读上述输入 → 设置 caster → `handle_spell_cmd` → ledger 记 clone 字段、10/41、`SpellReturn`、640 存在性、rng、drain。
- **比较器**：逐字段；`SpellReturn == has_640`（训练前提已钉死）；`clone.dir == back_dir(caster_dir)`；
  10 的 feature 尾/41 的 param/tag = `MakeHumanFeature(0, gender, gender, hair*2+gender)`。

### 1.4 止损线
- 需要改 `RunMagicCase` 主流程、或需要动 `downlink.rs`/`slave.rs` 产品代码 ⇒ **停报**；
- 新 spec 字段 >8、或涉及文件 >4（`Program.cs`、Rust 驱动、比较器、golden 生成脚本）⇒ 停报；
- 既有 19 份 magic golden 出现"非新增 null 字段"的漂移 ⇒ 停报。

### 1.5 可判别性（改坏点 → 哪条断言红）
| 改坏点 | 红在哪 |
| --- | --- |
| 删 `turn_pair` 广播 | 收件人 10/41 缺失 ⇒ 比较器红 |
| `back_dir` 改成同向 | `clone.dir` 红 |
| 不复制 Gender/Hair | 10 的 feature 尾 / 41 的 param/tag 红 |
| 把技能等级置 3（绕过训练前提） | `SpellReturn==has_640` 红 |

---

## 2. MG-36（22 火墙）— 工作量档位：**中**

### 2.1 oracle 锚（一条一个位置）
- `src/M2Server/Magic/MagicManager.cs:1064` —— `MagMakeFireCross`
- `src/M2Server/Magic/MagicManager.cs:1067` —— 安全区门（`DisableInSafeZoneFireCross && InSafeZone`）
- `src/M2Server/Magic/MagicManager.cs:1069` —— 安全区红字
- `src/M2Server/Magic/MagicManager.cs:1072` —— N 格 `GetEvent`
- `src/M2Server/Magic/MagicManager.cs:1077` —— W 格 `GetEvent`
- `src/M2Server/Magic/MagicManager.cs:1082` —— 中心 `GetEvent`
- `src/M2Server/Magic/MagicManager.cs:1087` —— E 格 `GetEvent`
- `src/M2Server/Magic/MagicManager.cs:1092` —— S 格 `GetEvent`
- `src/OpenMir2/Grobal2.cs:48` —— `ET_FIRE = 5`
- `src/Modules/SystemModule/MagicEvent/MapEvent.cs:60` —— `MapEvent` ctor（`Visible=true` 自行 `AddMapEvent`）
- `src/M2Server/Maps/Envirnoment.cs:220` —— `AddMapEvent`（写 cell 对象表）
- `src/M2Server/Maps/Envirnoment.cs:1344` —— `GetEvent`（**两义**：无事件 / 格不可用）
- `src/M2Server/Actor/BaseObject.cs:1810` —— `InSafeZone`
- `src/GameSrv/Word/Threads/EventProcessor.cs:27` —— `Run()` 的定时服务（harness 不启动 ⇒ 本批验 tick 外）

### 2.2 Rust 现状
- **已实现**：`crates/world-bevy/src/world.rs` 的 `cast_fire_wall_spell`；`crates/world-bevy/src/world.rs` 的 `event_at`；
  `crates/world-bevy/src/world.rs` 的 `fire_burn_events`（public 表）；`crates/world-bevy/src/world.rs` 的 `run_map_events`；
  `bins/game-svr/src/lib.rs` 的 22 分支（含安全区红字）。
- **缺**：装置 `FireCrossBatch`、驱动 `firecross` ledger 块、比较器块。**产品代码不用改。**
- **建模差异（卡面登记）**：Rust 安全区读**施法者实体** `in_safe_zone`，oracle 读 `Envir.Flag.SafeArea`；装置须设实体标志并在 Fixture 记 `safe_zone_flag`。

### 2.3 判据归属
- **装置**：新 `FireCrossBatch`；契约 fail-closed——**未预占 ⇒ `events.len == 5`；预占 ⇒ `len == 4` 且缺格恰为预占格**，
  否则 rc=2（把"格不可用"从"去重支"里摘出来）。预占用 `ET_FIRE` + `ContinueTime=1`。记录
  **只** `EventType/nX/nY/ContinueTime/Damage`（不记 `RunTick`/时间戳，`GetTickCount` 派生）。
- **驱动**：读输入（`UserMagicLevel`、`CasterMcNoClamp`、安全区开关、预占格）→ 执行 → ledger 读 `world.fire_burn_events`；
  比较面按 `registered` 过滤或在界内保证全 registered（Rust 有"越界格事件仍进表"的 BUG-REPLICA 3）。
- **比较器**：按 `(x,y)` 排序后逐格比；`continue_ms == time*1000`（由输入推导）；`safe_zone_reject` + `red_sysmsg`；`drain_hit_cap==false`。

### 2.4 止损线
- 需要给 `world.rs` 新增公共 API（现有 `fire_burn_events/event_at/add_fire_burn_event` 已够）⇒ 停报；
- 需要改 `cast_fire_wall_spell` 语义 ⇒ 停报；
- 预占/安全区落点需要改 `RunMagicCase` 主流程 ⇒ 停报（改用新 spec 字段）。

### 2.5 可判别性
| 改坏点 | 红在哪 |
| --- | --- |
| 删安全区门 | `safe_zone_reject`/`red_sysmsg` 红 |
| 无视 `GetEvent` 去重 | 预占例 `events.len` 5 vs 4 红 |
| 漏一格（如 S） | 事件集合逐格比红 |
| `continue_ms` 用 `time` 而非 `time*1000` | 推导断言红 |
| 把 `RunTick` 当期望 | 本卡禁止：时钟派生、会 flaky |

---

## 3. MG-34（20 诱惑之光）— 工作量档位：**中偏大**

### 3.1 oracle 锚（一条一个位置；分支表见规划卡 v3.3）
- `src/M2Server/Magic/MagicManager.cs:707` —— 外层 `Random(4-level)`
- `src/M2Server/Magic/MagicManager.cs:710` —— `Master == playObject`
- `src/M2Server/Magic/MagicManager.cs:712` —— self-master 支发 60005
- `src/M2Server/Magic/MagicManager.cs:717` —— `Random(2)`
- `src/M2Server/Magic/MagicManager.cs:719` —— `Abil.Level <= caster.Level + 2`
- `src/M2Server/Magic/MagicManager.cs:721` —— `Random(3)`
- `src/M2Server/Magic/MagicManager.cs:723` —— `Random(caster.Abil.Level + 20 + magicLevel*5) > tLv + MagTammingTargetLevel`
- `src/M2Server/Magic/MagicManager.cs:725` —— 金币守卫四条（NoTame/UNDEAD/Level/Count）
- `src/M2Server/Magic/MagicManager.cs:736` —— `Random(n14)`
- `src/M2Server/Magic/MagicManager.cs:759` —— royalty 赋值
- `src/M2Server/Magic/MagicManager.cs:766` —— WalkSpeed 钳制
- `src/M2Server/Magic/MagicManager.cs:770` —— NextHitTime 钳制
- `src/M2Server/Magic/MagicManager.cs:775` —— `SlaveList.Add`
- `src/M2Server/Magic/MagicManager.cs:779` —— `Random(14)` ⇒ HP=0（HP0 ①）
- `src/M2Server/Magic/MagicManager.cs:787` —— UNDEAD && `Random(20)` ⇒ HP=0（HP0 ②）
- `src/M2Server/Magic/MagicManager.cs:795` —— 非 UNDEAD && `Random(20)` ⇒ 暴走（Crazy ②）
- `src/M2Server/Magic/MagicManager.cs:803` —— 非 UNDEAD ⇒ 暴走（Crazy ①，**只抽一次**）
- `src/M2Server/Magic/MagicManager.cs:812` —— `Random(2)!=0` 支发 60005
- `src/M2Server/Magic/MagicManager.cs:814` —— 外层 true 支无条件 `result = true`
- `src/M2Server/Magic/MagicManager.cs:819` —— 外层 false 支 `Random(2)==0 ⇒ result=true`
- `src/OpenMir2/Data/MagicInfo.cs:36` —— `TrainLevel`
- `src/GameSrv/DB/MySqlDB.cs:131` —— `TrainLevel[0]=NeedL1` 起
- `sql/mir2_data.sql:56` —— `NeedL1` 列
- `src/OpenMir2/Messages.cs:844` —— `RM_MAKEHOLYSEIZEMODE = 60005`

### 3.2 Rust 现状
- **已实现**：`bins/game-svr/src/slave.rs` 的 `cast_tamming_spell`；`crates/world-bevy/src/slave.rs` 的 `tame_adopt`；
  `crates/data/src/pet_slave.rs:146` 四键；`bins/game-svr/src/parity_serverconf.rs` 的 `apply_serverconf_fixture`（十键入口）（四键只返回、不落 World）。
- **产品缺口（需 §3.5 决策）**：`RM_MAKEHOLYSEIZEMODE(60005)` 无编码器、从未发送——`bins/game-svr/src/lib.rs` 的 20 分支登记注释（`RM_MAKEHOLYSEIZEMODE`）。
- **模型缺口（scope=partial）**：`CanReAlive` / `MonGen.ActiveCount` 未建模（`bins/game-svr/src/slave.rs` 模块头登记）。
- **配置接线**：驱动必须把 `Fixture.server_conf` 的**四键**接到 **`PetSlaveConfig` 消费侧**（不是 World；`parity_serverconf.rs` 文件头明写）。

### 3.3 判据归属
- **装置**：`TammingBatch` 7 例（`self_master/adopt_ok/level_gate_fail/branch_else/outer_fail_true/outer_fail_false/tamming_gate_fail_crazy`）。
  契约：配置 40/1/100/5、`FastTrain=false`（显式）、训练前提（`level<3`、`NeedL{level+1}<=caster.Abil.Level`、
  `TranPoint <= MaxTrain-4`）、`n14 ∉ {3,20}`、MonsterSpec 新字段、预置 `Master/SlaveList`。
- **驱动**：读输入 → 执行 → ledger：`SpellReturn`、640 存在性、逐支 RNG 前缀、`monsters_after`、`slave_list_rel`、
  `Rm` 过滤 60005、crazy/HP 状态、`drain_hit_cap`。
- **比较器**：分支表期望**由输入推导**（`wParam=(L*5+10)*1000`、royalty 精确式 + 时钟窗、`n14` bound、钳制值）；
  `SpellReturn == has_640`；Crazy ② 连续两条独立 `R(20)`；`hp_zero_2` **无** `R(n14)` 记录。

### 3.4 止损线
- 产品改动（60005）>2 文件、或改动既有 golden ⇒ 停报；
- 新 `MonsterSpec` 字段 >8、新 golden 块字段 >12 ⇒ 停报；
- `tame_adopt` 需要语义改动（它应只读状态并落尾）⇒ 停报。

### 3.5 产品改动（**Leader 已批，按下列形状执行**）
- **独立第一笔** `feat(smframe): RM_MAKEHOLYSEIZEMODE 编码器`；**独立第二笔** 两处发送
  （`bins/game-svr/src/lib.rs` 的 20 分支，对应 `src/M2Server/Magic/MagicManager.cs:712` / `src/M2Server/Magic/MagicManager.cs:812`）。
  **不得与装置改动混笔。**
- **止损线收紧**：编码器 + 发送涉及文件 **>2** ⇒ 停报；既有 19 份 golden **任何漂移** ⇒ 停报。
- 若需改 `smframe` 的**公共接口** ⇒ **先报**，不自行定形。
- `RefNameColor()`→656 已在（`bins/game-svr/src/lib.rs` 相关说明处），**不要顺手改它**。

### 3.6 可判别性
| 改坏点 | 红在哪 |
| --- | --- |
| `:719` 的门删掉/改 `>=` | `level_gate_fail` 的"恰 2 抽"前缀红 |
| 忽略外层 `Random(4-L)` | `outer_fail_*` 两例红 |
| 不发送 60005（若实现） | `self_master`/`branch_else` 的 `Rm` 过滤红 |
| Crazy ② 复用判定值当时长 | "连续两条 `R(20)`"断言红 |
| `hp_zero_2` 抽 `R(n14)` | "无 `R(n14)`"断言红（夹具 `n14∉{3,20}` 防撞 arg） |
| 把 `SpellReturn` 与 640 拆开 | 贯穿等式红 |

---

## 4. 共用验收

- 门：`cd mir2-rs && cargo fmt --all -- --check`；`cargo test -p game-svr --lib`（三数）；C# `dotnet build MagicGolden`；
  既有 19 份 magic golden **零漂移**（仅允许新增 null 字段）。
- 判定层错位反例：配置值必须由**比较器**对 `Fixture.server_conf` 断言，驱动只读取/落值。
- 每卡至少一条"改坏点 → 红断言"的实测（§1.5 / §2.5 / §3.6），并在交付里贴 `left/right` 原文。

## 5. 关联

- 规划与实现规范：[`magic-observe-surface-plan.md`](magic-observe-surface-plan.md)
- 配置批与判据层教训：[`magic-serverconf-batch-impl.md`](magic-serverconf-batch-impl.md)
- 十键入口：`bins/game-svr/src/parity_serverconf.rs`（`apply_serverconf_fixture`）
