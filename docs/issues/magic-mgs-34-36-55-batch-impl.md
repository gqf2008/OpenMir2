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
  比较面按 `registered` 过滤或在界内保证全 registered（越界格事件仍进表 = **BUG-REPLICA 3，
  同构复刻已证 + 守卫牙钉住（2026-11-27）**：world-bevy 守卫断言
  `fire_wall_oob_cell_event_still_registered_replica`——oracle 活行为（`Envirnoment.cs:1344`
  GetCellInfo OOB ⇒ success=false ⇒ GetEvent ⇒ null ⇒ 判空门过 ⇒ AddEvent）+ Rust 同构
  （add_fire_burn_event_impl 无界检查）+ 部署不可达（0.map 700×700 实测边框 2796 格全
  HighWall、可走边框格=0 ⇒ 真图施法者无法站边角）；三段论证详见守卫断言注释）。
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

---

## 6. 实现批记录（MG-55）— 2026-11-21，**未编译**

> 基线 `0e2eb1316`；工作分支 `card/mg55-clone`。本节是 Leader 裁决（opc-19 2026-11-21）的落点：
> 三处偏差全部批准，偏差 1/2 各带硬条件；构建窗口**尚未**给出。**以下任何「零漂移 / 红绿」结论
> 在实跑前都只能标 `未验`。**

### 6.1 偏差 1（已批）：`RunMagicCase` 主流程增量

**增量清单（`mir2-rs/tests/parity/csharp/MagicGolden/Program.cs`）**
1. 4 行 `-1/false` 缺省门控：`CasterDir/CasterGender/CasterHair` 赋值 + `PreSpellFaceTarget` 转向
   （与生产 `PlayObject.Attack.cs:1269` 同源）；
2. 一个 `if (spec.MagicId == 46)` 观察块：扫主人同格找 `ChrName=="Clone"`，按 ident 10/41 过滤
   `recorder.Buffers`，**只读**（无共享状态赋值）；
3. `AttachStatusTimeArr` 追加 7 个可空参数（仅 46 传），并扩「全 null 直接返回」短路条件。

**Leader 条件**
- (a) 缺省路径零副作用：`-1`/`false` 时**不调用** `GetNextDirection`、不改 `caster.Dir` 的取值时机、
  不改任何既有键；
- (b) **必须实跑证明零漂移**：重生成 2 支既有 case 到 temp，与**同一棵树上的基线生成器产物**
  逐字节比对（剔除 `delivery` / `due_ms_rel` 两个墙钟字段）：
  - 带转向：`spacemove`（`EnableMaps=true` 走既有 `caster.Dir = GetNextDirection(...)`）；
  - 不带转向：`shield`；
  - **口径更正（2026-11-21，Leader 批准）**：原文写"与仓库件比对"——该字面口径在当前 master 上
    **本来就过不了**（见 §6.9 的既存漂移），正确判据是**相对漂移**：
    `master 生成器 vs 本分支生成器 = IDENTICAL(stripped)`。仓库件是否与当前生成器一致，
    属另一张卡（§6.9），不得混入本卡的零漂移判据。
- (c) 46 观察块严格门控——非 46 case 走不到，块内零赋值。

**状态**：(a)(c) 代码已满足（审阅面）；**(b) 未验**，等构建窗口。**未验前不得声称「既有 golden 零漂移」。**

### 6.2 偏差 2（已批）：克隆体 ActorId 归一化

**事实**：C# `ActorMgr.GetNextIdentity()` 是**进程级**自增（`src/M2Server/Actor/ActorEntity.cs:20`）；
harness 的 4 个对象随后被覆写成 `idBase+1..+4`，克隆体 ctor 取到的是全局计数（本支为 `5`），
与 `idBase=1000` 无固定关系 ⇒ **相对 id 跨栈不可比**（规划 §5.3 的「相对 id」不成立）。

**实现**
- 两侧各记本侧 `Clone.actor_id`（golden=C# 真值；ledger=Rust `alloc_id` 值），**不参与相等判据**；
- 10/41 帧的 `recog` 存**原始值**；比较器按「== 本侧 `Clone.actor_id`」归一化为 `0` 后走 `verify`
  的严格逐帧比较；
- 其余 recog（`SM_SPELL`/`SM_MAGICFIRE` 的施法者 id、`SM_MAGIC_LVEXP` 的 MagicId）不受影响、仍逐字节比。

**Leader 条件（牙，已写进比较器）**
1. 归一化前两侧都必须**至少一帧** `recog == 本侧 clone id`；零命中 ⇒ 红（「归一化空转，id 面未验」）；
2. 两侧 clone id 必须**不等**；相等 ⇒ 红（「归一化多余，请说明」）；
3. 原始 id 与未归一化帧在两侧产物中可见（`Clone.actor_id` + `frames[].recog`）。

**状态**：牙已写（`clone_recog_hits`），**未跑**（需 golden+ledger）。

> **修订（2026-11-21 首跑 golden 后）**：oracle 克隆体出生**零帧**（§6.7），没有任何帧的
> `recog` 等于 clone id ⇒ 「归一化」无对象可归一，本条 1/2 条的"≥1 帧"前提被事实推翻。
> 现行判据改为**负向**：两侧 `recog == 本侧 clone id` 的帧数必须为 **0**（重新广播即红），
> 两侧 id 仍必须不等；实现见 §6.8。

### 6.3 偏差 3（已批）：新增 7 个 Fixture 输入键

| 键 | 归属 | 比较器用法 |
| --- | --- | --- |
| `caster_dir` | 装置输入（转向**前**） | 驱动读入但不使用；比较 `Clone.caster_dir`（转向后）与 `dir == (caster_dir+4)%8` 推导式 |
| `caster_gender` / `caster_hair` | 装置输入 | 驱动建主人 `feature`；比较器核 `41.param/tag` == `MakeHumanFeature(0,g,g,h*2+g)` |
| `pre_spell_face_target` | 装置输入 | 驱动断言必须 `true`（不照做则与产品路径不同源） |
| `fast_train` | 装置输入（恒 false） | 驱动断言 `false`（`PlayObject.cs:2023` 缺省；只有快速修炼项链置 true） |
| `magic_train_lv` / `magic_max_train0` | 装置输入 | 驱动建魔表；训练前提由 `Train` 块（`level_*`、`max_train_at_level`、`tran_point_*`）与驱动断言核 |

`MagicSpec` 新字段 = 4（`CasterDir/CasterGender/CasterHair/PreSpellFaceTarget`），未超 8 的止损线。

### 6.4 待验清单（构建窗口一开就按序跑）

1. `dotnet build` MagicGolden（Release）；
2. `--case clone --out mir2-rs/tests/parity/golden/magic/clone46.json`；同时按 6.1(b) 重生成
   `spacemove`/`shield` 到 temp 与仓库件比对（剔除墙钟两字段）；
3. 驱动：`cargo test -p game-svr --lib tests::clone46_parity_clone -- --exact --ignored --test-threads=1`；
4. 比较器：同一 `MIR2_LEDGER_RUN_ID`，
   `cargo test -p mir2-parity-tests --test magic_fireball_parity -- --ignored --exact real_rust_ledger_clone46`；
5. 变异牙现场重放（§1.5 四行）并贴 `left/right` 原文；
6. 全绿后回填本节结论 + 提交（提交信息引用本卡）。

### 6.5 车道口径订正

Leader 2026-11-21：`cargo` 进程数**不等于**车道数（同一批测试滚动起落）。后续报窗口必须附
**命令行**（`Get-CimInstance Win32_Process` 的 `CommandLine`），由 Leader 判断哪些属于同一任务。

### 6.7 首跑 golden 发现（2026-11-21，**待 Leader 裁决**，未改产品）

golden 已生成（temp `E:/UserTemp/mg55-golden/clone46_v2.json`，seed=46，两次生成 Rng 逐值一致），
发现两处**产品级**缺口，按卡 §1.4 停报：

**(A) oracle 克隆体构造消费 5 个随机数；Rust `spawn_clone` 消费 0**
`LoggingRandom.Next(int)` 临时调用栈定位（诊断补丁已还原）：
```text
DRAW[1] max=1500   v=1136   @ BaseObject..ctor <- AnimalObject..ctor <- CharacterObject..ctor
DRAW[2] max=2000   v=629    @ BaseObject..ctor <- AnimalObject..ctor <- CharacterObject..ctor
DRAW[3] max=3000   v=2764   @ AnimalObject..ctor <- CharacterObject..ctor <- PlayObject..ctor
DRAW[4] max=3000   v=1129   @ AnimalObject..ctor <- CharacterObject..ctor <- PlayObject..ctor
DRAW[5] max=999999 v=974376 @ PlayObject..ctor (RandomNo) <- PlayCloneObject..ctor <- MagMakeClone
DRAW[6] max=3      v=2      @ DoSpell 训练尾 Random(3)+1
```
影响：不只 `Rng` 长度 6 vs 1；训练抽位置也错（golden 的 `Random(3)=2` 是第 6 抽）⇒ 世界 RNG 流整体偏移。
反证锚：`crates/world-bevy/src/slave.rs` 的 `spawn_clone` 文档注释称"oracle 构造器无 Random"（与事实相反）；
`crates/world-bevy/src/slave.rs` 的 `spawn_clone` 放格测试（`构造克隆体不消费 RNG`）把错误行为钉住。

**(B) oracle 克隆体出生零帧；Rust 46 支无条件广播 `turn_pair`**
golden 逐收件人：`caster rm3/sm2`（638+640）、`target/obs_in rm2/sm2`（17+638）、`obs_out` 零；
**无任何 SM_TURN(10)/SM_FEATURECHANGED(41)**。Clone 块记录 `fixed_hide_mode=true, ob_mode=false`。
根因链：`PlayObject` ctor `FixedHideMode = true`（`src/M2Server/Player/PlayObject.cs:993`）→ 克隆体不登录、
无 `PlayObject.cs:1442` 复位 → ctor 尾 `SendRefMsg(RM_TURN)` 命中 `BaseObject.Message.cs:390-394`
"隐藏态只发自己" → 克隆体队列无人排空 ⇒ 0 帧。Rust 侧在 `bins/game-svr/src/lib.rs` 的 46 支
无条件 `turn_pair` 广播（既有单测同断言）。

**已验证一致的面**：clone 静态字段（name/pos/dir=6/gender=1/hair=7）、`caster_dir=2`（`6=(2+4)%8`，
验证 `pre_spell_face_target` 与 `GetBackDir`）、`has_640=true`、`drain_cap=false`、`SpellReturn=true`。
**非确定量**：clone `actor_id` 两次生成不同（`HUtil32.Sequence()` 为 GUID 派生）⇒ 归一化必要，
`rel_to_caster` 每次漂、**不得进比较面**。

**待裁决选项**
- **A 扩卡修产品**：① `spawn_clone` 按 1500/2000/3000/3000/999999 顺序消费 5 抽，并把
  上述 `spawn_clone` 放格测试改钉新语义；② 46 支去掉 `turn_pair` 广播（克隆可见性属 AOI 后续帧）。
  止损线：`slave.rs` / `lib.rs` 产品改动 + 既有 46 单测。
- **B 收窄 scope=partial**：不改产品；只比静态面 + 638/640/17 + 负向"不得有 10/41"；
  `Rng`/训练 TP 登记"构造抽未建模"不参与比较（同 skeleton 先例），并登记两处已知偏差。

裁决前不改 `slave.rs`/`lib.rs`；当前比较器会因 10/41 缺失与 Rng 6 vs 1 判红，属**已知红**。

### 6.8 选项 A 落地记录（2026-11-21；已构建/已跑，最终 tip 见交付）

> **消歧（2026-11-21）**：本节"选项 A"= **MG-55（46 分身）的产品修正**（`spawn_clone` ctor 5 抽 +
> 46 支出生不广播 `turn_pair`）。它与 `docs/issues/golden-harness-blind-spots.md` 的
> **B21-A（夹具物化）**（曾同名"选项 A"，指把 `caster_sc` 物化进 `Abil`，**已撤销**，理由见该卡 §牙与 hex）
> **无关**。跨卡引用请用"卡名 + 代号"，不要用裸代号。

**产品修正 1：`spawn_clone` 构造期 5 抽**（`crates/world-bevy/src/slave.rs`）
- 新增 `consume_clone_ctor_draws(rng)`：按 C# ctor 链顺序消费
  `Random(1500)/Random(2000)/Random(3000)/Random(3000)/Random(999999)`，注释引用
  `BaseObject → AnimalObject → CharacterObject → PlayObject` 调用栈，并明写"值不建模，只为对齐 RNG 流"；
- 调用点：`spawn_clone` 的两道早退门（主人不在/图越界）**之后**（oracle 早退不执行 ctor）；
- 原文"不抽随机数（oracle 构造器无 `Random`，RNG 账本不变）"已改为事实版；批记录§6.7 保留了
  "此处曾有错误来源断言"的说明；
- 单测 `spawn_clone_creates_player_clone_and_occupies_cell` **挪旧牙**：原断言
  `assert_eq!(w.rng.random(), fresh.random(), "构造克隆体不消费 RNG")` 是 Rust 跟 Rust 比
  （`fresh` 为构造前克隆），对 oracle 无约束力，已改为断言 seed=46 的 oracle 五抽
  `[1136, 629, 2764, 1129, 974376]` + "下一次取数与参照流相同"（恰 5 抽）。

**产品修正 2：46 支出生不广播 turn_pair**（`bins/game-svr/src/lib.rs`）
- 46 支只登记 `displays`（AOI/后续视野用），删除 `downlink::turn_pair` + `broadcast_ref_msg`；
- 既有单测 `spell_cmd_clone_wires_entity_and_frames` **挪旧牙**：原断言 `turns.len()==2` +
  名尾 `Clone/255` 编码的是"无条件广播"错误行为，已改为断言三 socket 均**无** 10/41。

**驱动/比较器同步**
- 驱动：`cast_rng` 恰 6 抽 + `TranPoint 增量 == 第 6 抽 + 1`；帧序 caster `[638,640]`、
  target/obs_in `[17,638]`、obs_out 零；RM 列不再合成 10001（克隆体自投递）；
- 比较器：`clone_recog_hits` 扫 `Receivers.Sent + Clone.receivers`，两侧**必须 0 命中**
  （重新广播即红）；两侧 clone id 必须不等；golden 侧断言 `fixed_hide_mode=true / ob_mode=false`；
  Clone 块逐字段 + `dir == (caster_dir+4)%8` + `SpellReturn == has_640`。

**变异牙（现场重放，已还原 + sha 核对）**
- A（删第 5 抽 `Random(999999)`）：驱动
  `应恰 6 抽（克隆体 ctor 5 + 训练尾 1）；实得 [1136, 629, 2764, 1129, 2]`；比较器
  `[clone] RNG 行数不等：golden=6 rust=5`。
- B（恢复 turn_pair 广播）：驱动
  `caster 帧序应为 [638,640]（出生零帧） left: [10,41,638,640] right: [638,640]`；比较器
  `台账不应有帧的 recog == Clone.actor_id(1005)（产品不得广播 turn_pair；实得 12 帧）`。
- 还原后两份文件 sha 均与变异前一致（`MUTATION` 标记为 0），重跑 46 三用例复绿。

**零漂移证明（Leader 条件 1(b)；结论需按下面口径读）**
- 用 `master`（`015f9c760`）的 `Program.cs` 与我们的 `Program.cs` 各生成 `spacemove`/`shield` 到 temp；
  剔除 `delivery`/`due_ms_rel` 后：**base vs ours = `IDENTICAL(stripped)`（两支）**
  ⇒ 本次对 `RunMagicCase`/`AttachStatusTimeArr` 的增量**零漂移**。
- 但 **base vs 仓库件 ≠ 0**（spacemove 20 行、shield 101 行）：差异只有两类**既存漂移**，
  与 MG-55 无关：
  1. `"Angel": null`——master 的 angel 提交给 `FireballResult` 加了无 `JsonIgnore` 的属性，任何重生成都会出现；
  2. shield 的 `Fixtures` 缺 poison 批新增键（`amulet_shape`/`amy_ounsul_point`/`red_msg_*`/
     `target_anti_poison`/`you_poisoned`）。
  ⇒ 卡 §6.1(b) **原字面口径（"重生成 == 仓库件"）在当前 master 上本来就过不了**；已按 Leader
  裁定改为**相对漂移**（master 生成器 vs 本分支生成器），本卡以 `IDENTICAL(stripped)` 为证据。
  仓库件与当前生成器的既存漂移单列 §6.9（需另立卡），不混入本卡判据。

**横向影响（skeleton/sinsu/angel；Leader 要求必回）**
- 三支 `summon_ok`（seed=1）golden `Rng` 均为 `[373,221,1401,2314,1315,1]`；`cap_hit` 为 `[]`；
- 用 .NET `Random(1)` 按 `[1500,2000,3000,3000,2000]` 再 `Random(3)` 重放，逐值完全一致
  ⇒ 前 5 抽 = `BaseObject(1500,2000)` + `AnimalObject(3000,3000)` + **`MonsterObject` ctor
  `SearchTime = 3000 + Random(2000)`**（`src/M2Server/Monster/MonsterObject.cs`），第 6 抽为训练尾。
- ⇒ "造怪抽数不可复刻"的登记**错误**：它是**可复现且已记录在 golden 里**的确定序列；Rust 驱动只是没消费。
  `angel` 的 scope（`parity-gap-magic.json`）与 `openmir2-skeleton.md` 的同一句需要更正（另卡）。

**已登记残差（本卡不修）**
- Rust 骨架未建模克隆体 `fixed_hide_mode/ob_mode`（golden 记录 true/false 作为零帧根因；AOI 可见性差异另行登记）；
- golden 的 `Clone.actor_id/rel_to_caster` **不参与相等判据**：`HUtil32.Sequence()` 由 GUID 派生，
  每次生成都不同。**B20（2026-11-21）已从 golden 移除这两个绝对值**，改为
  `actor_id_policy="oracle_guid_nondeterministic"` + `clone_ordinal=0` + 关系断言；
  详见 `docs/issues/golden-harness-blind-spots.md` 的 B20 节。

### 6.9 需另立卡：既有 magic golden 与当前生成器的既存漂移（与 MG-55 无关）

**证据（2026-11-21，`master = 015f9c760` 的生成器直接重生成，未含任何 MG-55 改动）**
- `spacemove.json`：重生成件与仓库件剔除 `delivery`/`due_ms_rel` 后仍差 **20 行**，差异键唯一来源
  `"Angel": null`（master 的 angel 提交给 `FireballResult` 增加了无 `JsonIgnore` 的属性）；
- `shield.json`：差 **101 行**，除 `"Angel": null` 外，仓库件**缺** poison 批新增键
  `amulet_shape` / `amy_ounsul_point` / `red_msg_f_color` / `red_msg_b_color` /
  `target_anti_poison` / `you_poisoned`（当前生成器会写，仓库件是旧版）。
⇒ 这两支仓库 golden 与当前生成器**已不一致**，与 MG-55 无关；任何"重生成即应逐字节相等"的验收
口径在 master 上都会假红/真红混杂。

**建议卡面（供 Leader 立卡）**
- 背景：golden 是装置产物，装置 schema 演进后未重生成 ⇒ 仓库件逐步陈旧；本次两例已实证。
- 期望结果：一次性重生成受影响的既有 golden（至少 `spacemove`/`shield`，并先枚举 `Angel: null`
  与 poison 键两类差异的受影响集合），或为"新增可空键"提供统一序列化抑制策略（二选一由 Leader 定）。
- 验收：每支重生成件与仓库件的差异**逐键归因**到已知 schema 变更（不允许多余差异）；
  重生成后对应 Driver/Comparator 全绿且 `RED_SET` 不扩大。
- 风险：重生成会带入 GUID/墙钟类非判据字段（如 clone 的 `actor_id`），必须先列白名单。

### 2.6 MG-36 实现记录（2026-11-21，已构建/已跑）

**装置**（`mir2-rs/tests/parity/csharp/MagicGolden/Program.cs`）
- `MagicSpec.FireCrossSafeZone` + `PreEventCells`（缺省 false/空 ⇒ 既有 stem 零变化）；
- `RunMagicCase` 额外两处门控：① 预置事件 `new FireBurnEvent(caster, x, y, ET_FIRE, 1, 0)`
  （oracle ctor 只落格子、**不进** `EventMgr.Events`，用于 `GetEvent` 去重支）；② 安全区
  （`env.Flag.SafeArea=true` + 临时 `Config.DisableInSafeZoneFireCross=true`，**施法后立即还原**）；
- 事件快照：只取 `Envirnoment == env` 的 `EventMgr.Events`，反射读 `ContinueTime`
  （private readonly，沿基类链），只记 `EventType/x/y/ContinueTime/Damage`，**不记 RunTick/时间戳**；
- 契约 fail-closed（不满足 ⇒ rc=2 不落盘）：预占格 ⊆ 五格且无重复；事件数 == 5 − 预占数；
  安全区支 == 0 且必须有 `RM_SYSMESSAGE(11004)`；`type==ET_FIRE(5)`、`continue_ms>0`；
- **离线替身坑**：`InSafeZone(env,x,y)` 在 `SafeArea=false` 时也遍历 `M2Share.StartPointList`
  （生产由 StartPoint.txt 装载；离线为 null ⇒ NRE）⇒ 装置补**空表**（"离线替身必须比真实环境更空"）。

**golden `golden/magic/firecross.json`**（`--case firecross`，seed=22/23/24）
| case | 事件 | 安全区 | 红字 | 640 | Rng |
| --- | --- | --- | --- | --- | --- |
| `base` | 5（四邻+中心，damage=6，continue=7000） | 否 | 无 | 有 | 7（6+训练） |
| `pre_center` | 4（缺中心） | 否 | 无 | 有 | 7 |
| `safe_zone` | 0 | 是 | `SM_SYSMESSAGE(100)` | 无 | 6（无训练） |

**驱动/比较器**
- `tests::firecross_parity_{base,pre_center,safe_zone}`（各自独立进程）+ `real_rust_ledger_firecross`；
- 驱动自带退化守卫：非安全区支"事件数 == 5−预占，且 ≥1；0 条即具名红"
  （**安全区支形态不同**：判据是"0 事件 + 红字 + 无 640"三者同时成立）；
- 比较器：`verify` 通用面 + `FireCross` 块逐字段 + 推导面（blocked ⇒ 0 事件/红字/无 640；
  非 blocked ⇒ 5−预占/无红字/有 640；`type==5`；`continue_ms>0` 且 1000 整数倍、同支一致）。

**变异牙（现场重放；基线/变异期望 + 红在哪类判据）**
| 变异 | 基线期望 | 变异期望 | 红在哪 |
|---|---|---|---|
| A 删安全区门 | safe_zone：0 事件/红字/无 640 | 5 事件/无红字/有 640 | 驱动 `火墙事件数 5/0`；比较器 `red_sysmsg/has_640/events 条数` + `safe_zone/caster Sent[1]`；共 14 条 |
| B 无视 `GetEvent` 去重 | pre_center：4 事件 | 5 事件 | 驱动 `火墙事件数 5/4`；比较器 `[pre_center] events 条数不等：golden=4 rust=5`（仅 2 条） |
| C 漏 S 格 | base 5 / pre 4 | base 4 / pre 3 | 驱动 `base 4/5`、`pre_center 3/4`；比较器 4 条 |
| D `continue_ms` 用 `time`（不 ×1000） | 7000 | 7 | 驱动全绿（不断言数值）；比较器 `continue_ms 不等：golden=7000 rust=7` + 倍数检查，共 18 条 |

四颗均 `cp` 还原后 sha 与变异前一致、`MUTATION` 标记 0，重跑三驱动 + 比较器复绿。

**零漂移**：`master a209f36fd` 生成器 vs 本分支生成器，`spacemove`/`shield` 剔 `delivery`/`due_ms_rel`
后均 `IDENTICAL(stripped)` ⇒ `RunMagicCase` 增量与 `StartPointList` 空表替身对既有 stem 零影响。

**顺手订正**：`FireBurnEvent.event_type` 的文档注释原写 "`ET_FIRE = 1`"（实为 5，实现一直是 5）
⇒ 改正并注明是注释错误（同"注释如何骗人"清单的骗法 A）。

**门**：三驱动 1 passed；比较器 1 passed；`clippy -D warnings` 三包 0；`cargo fmt --check` 0；
全量 `cargo test -p game-svr --lib` = **2198 passed / 0 failed / 142 ignored**。

### 2.7 牙的"三层形态"（2026-11-21 汇总，回答"这条判据在哪一层守着"）

同一颗产品变异，红出现在哪一层是**可分类**的；分类本身能暴露"驱动只记录不判"或"比较器漏判"：

| 形态 | 含义 | 今日样本 | 处置 |
| --- | --- | --- | --- |
| **驱动 + 比较器双红** | 驱动自身也断言该面（帧序/抽数/事件数），比较器再判一次 | MG-48 `power/3 +1`；MG-36 牙 A/B/C | 正常形态；两层都留断言，别为"只红一处"而删驱动牙 |
| **仅比较器红（驱动绿）** | 驱动只记录、不断言该字段；判据完全在比较器 | MG-42（driver 绿 / comparator 红）；MG-36 牙 D（`continue_ms` 7 vs 7000，驱动全绿、比较器 18 条） | 正常形态，但**该字段必须有比较器断言**；若比较器 someday 被放宽，此字段即失守 |
| **仅驱动红（比较器绿）** | **可疑**：要么比较器没比该面，要么驱动失败前没落盘（读到的还是旧台账） | 本批未作为"有效牙"采信 | 按缺陷处置：先查 driver 是否"写台账在断言前"，再查比较器字段清单；**不得**把它记成"牙生效" |

**本批的实践规则**
1. 驱动内的断言必须落在**台账落盘之后**（否则驱动红了比较器读旧台账可假绿——MG-36 牙 A 第一版就踩到）；
2. 每颗牙写"基线期望 / 变异期望 / 红在哪一层"，形态为"仅比较器红"时要在卡里点名该字段由比较器独守；
3. 出现"仅驱动红"先按缺陷查，不当作覆盖证明。

### 3.6 MG-34 实现记录（2026-11-21，已构建/已跑）

**产品三笔**（按 Leader 顺序，不混笔）
1. `25e254132` 纯模型：`TammingOutcome.holy_seize_wparam: Option<i32>`，仅 `src/M2Server/Magic/MagicManager.cs:712`
   （self-master）与 `:812`（`Random(2)!=0`）两支置 `Some((L*5+10)*1000)`；把
   `Random(2)==0 && level门` 合取拆回嵌套以**保留求值顺序**；单测含"其它 result=true 支必须 None"
   与"level 门失败仍已消费 R(2)"（参照流下一次取数相等）。
2. `ec56a5fea` 宿主发送：`cast_tamming_spell` 在两支投递 `RM_MAKEHOLYSEIZEMODE`（**内部 RM**：
   `AnimalObject.cs:134-136` `OpenHolySeizeMode`，无 SM/EDCode）；两处到达证据各自断言
   `tamming_self_master_queues_holy_seize_rm` / `..._else_branch_...`。
3. `7b89f3fcf` 帧面接线（**通用，无魔号分支**）：`tame_adopt` 走 `break_holy_seize_mode`（8090）
   + `RM_USERNAME`（8101）；`open_crazy_mode` 与条件 `break_crazy_mode` 补 8090；宿主 drain 接
   `8101 → SM_USERNAME(42)`（尾 `FilterShowName(ChrName) [+ "(Master.ChrName)" when !ObMode]`）；
   `BreakCrazyMode` 调到 adopt 之前（oracle `:738` 先于 `:753-775`）。

**装置**：`TammingBatch` 七支（`self_master/adopt_ok/level_gate_fail/branch_else/outer_fail_true/
outer_fail_false/crazy`），生成期按 oracle 取数序扫 seed；`MonsterSpec` 加
`NoTame/Animal/MonGenActive/CanReAlive/WalkSpeed/NextHitTime/MasterCaster/Name`；配置十键由
`ServerConfFixture.LoadOrThrow` 记录，驱动侧用 `Fixture.server_conf` 构造 `PetSlaveConfig`（**不落 World**）；
Fixture 记录 `caster_name/slave_colors/monsters[].name`（驱动输入）。

**golden 五面**：`rm_makeholyseize`（self/else 各 1 条 wParam=10000）、`slave_list_rel`/`slave_count_after`、
`monsters_after`（master/hp/walk=1500/next=2000/slave_make_level/crazy）、帧面（adopt `[17,638,656,42]`+640、
crazy `[17,638,656]`+640）、`Rng`（self 2 / adopt 7 / level 3 / else 3 / outer_t 3 / outer_f 2 / crazy 7）。

**变异牙（现场重放；基线/变异/红在哪）**
| 牙 | 基线 | 变异 | 红原文（摘） |
|---|---|---|---|
| A 删 60005 发送 | self/else mon0 恰 1 条 | 0 条 | 比较器 `rm_makeholyseize 条数不等：golden=1 rust=0` + `mon0 Rm 多重集`（驱动绿） |
| B 删 adopt 8090/8101 | adopt 帧 [17,638,656,42] | [17,638] | 比较器 `adopt Rm 缺 [8090,8101]`、`Sent 条数不等（golden=4 rust=2）`（驱动绿） |
| C 删 OpenCrazy 的 RefNameColor | crazy 含 656 | 无 656 | 比较器 `crazy Rm 缺 8090`、`Sent golden=3 rust=2`（驱动绿） |
| D wParam 漏 ×1000 | 10000 | 10 | 比较器 `rm_makeholyseize[0].w_param 不等` + 推导式 `60005 w_param=10 ≠ 10000`（驱动绿） |

四颗均 `cp` 还原 + sha 核对、`MUTATION` 标记 0，重跑七驱动 + 比较器复绿。
**四颗全部是"驱动绿 + 比较器红"**（§2.7 的"仅比较器红"层）⇒ 本批判据**全部落在台账面**：
`Rm` 多重集与 `Tamming` 块字段由**比较器独守**；驱动侧只带 `[17,638,642]` 类帧序与抽数断言。

**零漂移**：master `aed1efdeb` 生成器 vs 本分支生成器，`spacemove`/`shield` 剔 `delivery`/`due_ms_rel`
后仅多 `caster_name:null`/`slave_colors:null` 两个**新增 null 键**（允许项）；去掉后
`IDENTICAL_after_allowed_nulls=True`。

**scope = partial（未建模/不比较）**
- `CanReAlive` 清除后效、`MonGen.ActiveCount--`：golden 记录（留锚），Rust 无字段 ⇒ 不比较；
- `NoTame`：Rust 常量 false（`MonsterInfo.boTame` 全仓无赋值点），golden 也全 false；
- 60005 的**下游效果**（`RefNameColor → 656`）在装置不可观测（`RecordingMonster` 被动、不 Operate），
  本叶只验 **RM 投递本身**（`Rm` 恰一条、wParam 公式、只在目标队列）；
- follow-up：`MagMakeHolyCurtain`（`src/M2Server/Magic/MagicManager.cs:1168`）也发 60005，公式 `nPower*1000`，**本次未接**，
  该叶子的 60005 投递面未验（别把两处共用 helper 当作"60005 已全接"）。

**装置盲区登记（新增一类）**：`OfflineWorldEngine.RegenMonsterByName` 返回的替身怪**未入图/未进
ActorMgr** ⇒ 任何 `SendRefMsg` 首句 `Envir == null` 早退。⇒ `MakeSlave` 的 `RefNameColor`（656，
`BaseObject.cs:1203`）在 17/30/41 的 golden 中**不可见**，**生产应有、未验**；42 只由 adopt 的
`RefShowName` 产生，召唤路径不调。已同步在 `parity-gap-magic.json` 的 MG-24/41/42 scope 记一句
"656 未验（装置盲区：替身怪未入图 ⇒ SendRefMsg 早退；生产应有）"。
