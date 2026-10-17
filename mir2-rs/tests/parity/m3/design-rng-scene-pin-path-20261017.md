# M3 RNG 场景「两侧钉同一只怪」接线设计（可行路径、机判与缺口）

- 卡：`openmir2-m3-rng-scene-driver`（承接卡；`rng-trace-scene/DRIVER-CONTRACT.md` §7 指出的阻塞项）
- 上游约束：`rng_trace_scenario.json`（canon `m3-rng-trace-scene-v1`）、`rng-trace-scene/DRIVER-CONTRACT.md`、
  `rng-trace-scene/README.md`、`AGENTS.md`「判据分级」（R/L）
- 本件目标：**只解决「把同一只怪钉在同一场景」这一环**；不解决整包 W0 的全局消费者/墙钟确定性（见 §7）
- 写集：本文件（`mir2-rs/tests/parity/m3/**`）。**零** `crates/**`、`bins/**`、`src/**`、`sql/**`、`MirServer/**` 改动；
  零起栈、零占端口、零数据库写入
- 事实基准：仓库 **`b5212b36a`**（= `35e3cf468` 的文档子提交，生产代码未变：`git diff --stat 35e3cf468 b5212b36a`
  只有 1 个 docs 文件；`35e3cf468^` = `318bd1d81`；**`b3212b36a` 不存在**），部署件 `E:/MirServer/M2GameSvr`（只读）
- 结论：**推荐「启动期（pre-tick）双侧 env 门钉怪」**（Rust 侧在 `sim_loop` 守卫放置之后、tick 循环之前；
  C# 侧在 `InitializeWorld` 末端、怪物线程正式开跑之前），**并在 `RegenMonsterByName` 之后显式覆盖 `Dir`**。
  其它现有路径均因权限墙 / 方向不可控 / 内容冻结 / 额外抽数 / 未接线而不可选。判不准的部分按 **R 保守**。

> 本文所有 `file:line` 均在 2026-10-17 用 `grep -n` / `sed -n` / `awk` 在本机逐条抽验（命令见 §10）。
> 输入两份调查与本仓事实不一致处已在 §9 逐条订正；**不复制未验证结论**。
> 2026-10-17 对抗复验 12 条 must-fix 已逐条落实；全文行号普查见 §10 末（统计与命令）。

---

## 0. 一句话结论

C# oracle **没有任何一条现有命令/脚本路径能把稻草人的 `Dir` 钉成 0**，也没有一条能在部署 Release 栈上
绕过 `PermissionMin=10` 的权限墙；因此「零新增 oracle 代码」的钉怪路径**不存在**。可行解只有一条：
**两侧各加一个默认关、env 门的「启动期 pin」**——用现有 `RegenMonsterByName`（Rust 为
`guardlist::regen_monster_by_name` → `create_monster_oracle`）原样抽完生成期 RNG，再照
`LoadGuardList` 的既有语义（`LocalDB.cs:133` / Rust `place_guards` 的 `e.ai.dir = g.dir`）覆盖 `Dir`。
这样既保住「取数序逐抽同序」，又把 `dir=0` 变成可实现，且不产生权限/内容/登录期 `PlayObject` ctor 抽数问题。

---

## 1. 问题

`DRIVER-CONTRACT.md` §7 明确：两侧「场景描述件 `<side>-scene.json` + 全场景抽数流水
`<side>-rng-stream.tsv`」的**生产者不存在**，`run_rng_trace_pair.ps1 -Go -PinScene` 目前正确
fail-closed（`PREREQ_MISSING: …驱动尚未接线`，`exit 2`）。

场景冻结输入（`rng_trace_scenario.json`）：

| 项 | 值 |
| --- | --- |
| map | `0137`（竞技场） |
| seed | `42` |
| start_pose | `(17, 20) dir=2` |
| monster | `稻草人` @ `(18,20) dir=0` × **1** |
| mongen_rows | 逐行多重集 == `fixtures/mongen-map0137.tsv`（**0 行**） |
| groups_expected | `monster_ai` / `spawn` / `drop` |

本件要回答的只有：**C# 侧怎么把这只怪钉出来，Rust 侧怎么用同一条取数序镜像，以及怎么机判。**

---

## 2. 事实基线（逐条自验）

### 2.1 场景隔离事实

- `fixtures/mongen-map0137.tsv` **6 行注释、0 行数据**（`wc -l` = 6；`grep -vc '^#'` = 0；
  sha256 = `81f0cc3cba35563e96d9052a044d857f99130986d2e1ab514be9675f6c0971db`）。
- 部署件 `E:/MirServer/M2GameSvr/Envir/MonGen.txt` 与 `Mir200` 同名文件 **SHA256 相同**
  = `e5c39fb7eda66b70f82a61a5c2cea25df9cf0b6892b916fb6d579f88ff43748f`；`0137` 行数 = **0**。
- `0137.map` SHA256 = `af987e96cd2b15a5eb3b6297a0465f031a3df63b21e50106d23328d6263c8b33`（30×33）。
  落点可走（`mapfile.rs` 口径）：`(17,20)` 与 `(18,20)` 均 `True`，7×7 邻域 43 格可走
  （证据：`evidence-attack-pair-r1-nomonster-20261012/10-map0137-not-in-mongen.txt`、
  `11-mapinfo-and-mapfile-0137.txt`）。
  ⇒ **Rust `cell_walkable_ignore_actors(0137, 18, 20) == true`**，`create_monster_oracle` 的
  地形移位环（`bins/game-svr/src/mongen.rs:500-533`）**0 抽**。

### 2.2 C# 生成链：所有径路最终都进同一个 `CreateMonster`

- `WorldServer.MonGen.cs:382 RegenMonsterByName` → `:385 CreateMonster`；
  若 `:388 GetMonsterThreadId` < 0 ⇒ 返回 `null`（实体已由 `CreateMonster` 建出/入图 = oracle 的
  孤儿-on-map，Rust `guardlist.rs` 模块头已登记同形）。
- `CreateMonster` 定义 `:498`；`FindMap` 失败 `:501-504`（`return null` 在 `:504`，**0 抽**）；race switch `:507-728`；
  构造链在 switch 内同步执行；`:740 cert.Dir = RandomByte(8)`；`:744 MonGetRandomItems`（`:742` 是
  `cert.WAbil = cert.Abil`）；`:745 Random(100) < CoolEyeCode`；`:750 Initialize`（`:748` 是 `}`；内部
  `CanWalk` 真 ⇒ `AddToMap` 成功 ⇒ `AddtoMapSuccess=false` ⇒ **不移位、0 抽**，`BaseObject.Base.cs:14-22`）。
- 稻草人 = `race 83`：`sql/mir2_data.sql:228`（`3, '稻草人', 83, …`）；`WorldServer.MonGen.cs:566-567`
  的 `case 83: cert = new SlowAtMonster();`。
- 构造链 6 抽（逐层）：

  | # | 抽 | 源码 |
  | --- | --- | --- |
  | 1 | `Random(1500)` | `src/M2Server/Actor/BaseObject.cs:439` |
  | 2 | `Random(2000)` | `src/M2Server/Actor/BaseObject.cs:441` |
  | 3 | `Random(3000)` | `src/M2Server/Actor/AnimalObject.cs:76` |
  | 4 | `Random(3000)` | `src/M2Server/Actor/AnimalObject.cs:77` |
  | 5 | `Random(2000)` | `src/M2Server/Monster/MonsterObject.cs:65` |
  | 6 | `Random(1500)` | `src/M2Server/Monster/Monsters/ATMonster.cs:10` |

  > 输入 B 写「race 80 为 5 抽」——那是 race 80 的链；**race 83 是 6 抽**（AtMonster 多一层）。
- 掉落：`MonGetRandomItems` 定义 `:439`；7 条基础抽 `Random(MaxPoint)` 在 `:451`；金币命中再
  `Random(Count)` `:455`；非金币命中 `Random(80)` `:467`；`StdMode>0` 且
  `Random(MonRandomAddValue)==0` `:474`（及后续升级抽）。稻草人的 `MonItems/稻草人.txt` 实测
  7 条（`10/30 金币 300` + 六条 `10/200`，空行被 `LocalDB.cs:619` 的空行检查跳过）。

### 2.3 权限墙（部署 Release 栈上，所有 GM/脚本路径都被挡住）

- `GameCommand.cs:33-36`：`#if DEBUG` 才 `Permission = 10`（`:32` 是 `{`、`:33` `#if DEBUG`、`:34-35` 两句、
  `:36` `#endif`）；Release 编译掉；随后 `:37-40` 是
  `if (PlayerActor.Permission < Command.PermissionMin) return 权限不足`。
- 命令注册的 `PermissionMin`：`MobCommand.cs:11` = 10；`MobPlaceCommand.cs:11` = 10；
  `MissionCommand.cs:11` = 10；`ReCallMobExCommand.cs:11` = 10；`RecallMobCommand.cs:12` = 10。
- 部署 `M2GameSvr/Server.conf:365 StartPermission=0`；`Envir/adminlist.txt` 唯一行
  `*\t阿瑞斯乐园`（**缺 IP**）⇒ `LocalDB.cs:80-83` 的 `IsNullOrEmpty(sIPaddr) ⇒ continue` 跳过。
- `WorldServer.cs:1021-1037 GetHumPermission` 虽会读 AdminList，但唯一调用点
  `PlayObject.cs:1188` **整段被注释** ⇒ 玩家 `Permission` 不会从 AdminList 回填。
- 命令名改写：部署 `Command.conf:80 Mob=怪`（GBK 解码确认）、`:84 RecallMob=bb`、`:103 Mission=Mission`、
  `:104 MobPlace=MobPlace`。即使有权限，`@怪` 的 `GetFrontPosition` 也只给 `(18,20)`，方向仍随机。

### 2.4 `Dir` 不可钉（关键否定事实）

- 新怪方向唯一来源 = `WorldServer.MonGen.cs:740` 的 `RandomByte(8)`；之后只有 AI 的
  `TurnTo/WalkTo`（运行期，不是钉怪）。
- 所有命令/脚本参数**没有 dir 字段**（`MobCommand.cs:15-56`、`MobPlaceCommand.cs:15-62`、
  `MissionCommand.cs:15-35`、`ReCallMobExCommand.cs:15-58`、`RecallMobCommand.cs:15-72`（文件仅 74 行）；
  `ExecutionProcessingSys.cs` 的 `ActionOfMonGenEx:5466-5488`、`ActionOfRecallmob:5375-5398`）。
- 「生成后覆盖方向」的既有语义（**配置驱动**）是 `LocalDB.cs:100-144 LoadGuardList` 的
  `guard.Dir = (byte)StrToInt(direction, 0)`（`:133`）。**这正是本设计要复用的语义**，不是新增行为。
  **订正（2026-10-17）**：「唯一」不成立——另有非配置驱动的先例 `MonsterObject.cs:121`
  `ElfMon.Dir = OldMon.Dir`（MakeClone 在 `:102` 调 `RegenMonsterByName` 之后覆盖 Dir）。
- ⇒ 锚定的 `dir=0` **不可能**由现有路径满足；要么改冻结 scenario 的 `monsters[].dir`（owner 决策），
  要么新增一个「生成后置向」的 T-* 钩子（本设计推荐，且与 LoadGuardList 同形）。

### 2.5 Rust 侧最小接线面（已存在）

- `bins/game-svr/src/guardlist.rs:73 pub fn regen_monster_by_name` = oracle `RegenMonsterByName`；
  内部 `:86 create_monster_oracle`（`bins/game-svr/src/mongen.rs:472`），**不额外抽数**。
- `create_monster_oracle` 取数序（`:472-551`；`spawn_monster` 请求 `:539-550`）：`map_size`（FindMap，不抽）→
  `race_is_spawnable`（不抽）→ `draw_monster_construct` → `random_byte(8)`（`:490`）→ `sink.budget_drops`
  （`:491`）→ `random_below(100)` CoolEye（`:493`；`:492` 是 `cool_eye_code` 读表）→ 地形移位（`:500-533`，本例 0 抽）。
- `draw_monster_construct`（`:326`）：race 83 走 `81|83|84|86|88|89|100|130` 分支
  （`draw_monster_object` + `draw_at_monster`）= 公共 4 抽 + 1 + 1 = **6 抽**。
- `WorldMonsterSink`（`mongen.rs:627`）负责落库/AI 初始态/展示层；`budget_drops` 在 `:698`。
- `place_guards`（`guardlist.rs:129`）已有「regen + 覆盖 dir」的完整先例：
  `:164` 写 `e.ai.dir`，`:166-168` 写 `DisplayTable` 的 `dir`；`std::mem::replace(world.rng, …)`
  手法在 `:142`。**Rust pin 可以逐行照抄，不需要改 `crates/world`。**
- 生产驱动点：`main.rs:8340 MonGenHost::start`（一次）；`main.rs:9957 step_mon_gen`（每 tick，
  先于 `world.tick()` `:9993`）；怪物 AI 在 `world.tick` 内 `world.rs:1313-1314 → monai.rs:861`。
- 启动构造段（不可破坏的前缀）：`main.rs:8302` 段① 16 抽 → `:8319` 屏障 A（第 17 条）→ `:8331`
  段② 36 抽 → `:8338-8340` 屏障 B（`MonGenHost::start`）→ `:8369` 段③ 16 抽
  ⇒ 部署件共 **69 抽 / 18 站点**（`crates/world/src/ctor_fold.rs` 模块头「实测计数」段）。
- 守卫放置：`main.rs:8538 place_guards`；守卫块结束于 `:8560`；tick 循环起于 `:8628`。
- 登录进图：`main.rs:9307 shift_enter_position`；`(17,20)` 可走 ⇒ **0 抽**
  （`mapwire.rs:336-356`：首格可走时一次都不抽；单测 `mapwire.rs:1239`）。

### 2.6 三个必须看见的额外事实（决定「推荐形态」）

**(F1) C# 玩家登录的 construct 抽是 5 抽，Rust 全未建模。**
`PlayObject` 的 ctor 链 = `PlayObject : CharacterObject : AnimalObject : BaseObject`（`PlayObject.cs:21` /
`CharacterObject.cs:12` / `AnimalObject.cs:12`），每层各抽：
`BaseObject.cs:439 Random(1500)`、`:441 Random(2000)`、`AnimalObject.cs:76 Random(3000)`、
`:77 Random(3000)`、`PlayObject.cs:1047 Random(999999)` = **5 抽**。
参照流水 `fixtures/rng_stream_seed42_head1828.tsv` **第 5–9 行**（seq 1–5）逐条就是这 5 抽，seq 5 =
`5  Next(max)  999999  168434  M2Server.Player.PlayObject..ctor@PlayObject.cs:1046`；
`consumer_groups.tsv:61` 把 `PlayObject..ctor` 归 `construct`（seq 1–2 由 `:57` = `BaseObject..ctor`、
seq 3–4 由 `:58` = `AnimalObject..ctor` 归 `construct`）。**注**：fixture / `consumer_groups.tsv` 的 site 列
按 hook 调用行记 `PlayObject.cs:1046`，源码实际在 `:1047`（差 1 行；以源码行号为准，二者指同一抽）。
Rust 登录路径 `main.rs:6050 fn new_player_entity` 走 `mir2_world::Entity::new`（`:6051`），**5 抽一条都不建**；
全仓也没有 `999999` / `RandomNo` 对应抽（`grep -rn "999999" mir2-rs --include=*.rs` 仅命中不相关文本）。
⇒ **任何「登录后再钉」的两侧全场景 trace，C# 都比 Rust 多 5 个 construct 抽**；折叠到 `spawn` 后
`FOLDED_COUNT_DIFF`（`rng_ledger_diff.py:497-501`）必红，除非 Rust 补齐这 5 条 construct 抽（R 级，影响所有
含登录的流）或场景不产生玩家会话。**本设计取后者（无客户端会话）。**

**(F2) C# 的怪物线程在 `InitializeWorld` 中途就已启动，且会对所有地图刷怪。**
`GameApp.cs:238 InitializationMonsterThread`（内部 `WorldServer.MonGen.cs:155-159`：`:155` `new Thread(...)`、`:159 .Start()`）
早于 `:241-243 LoadGuardList` 与本设计的 pin 点；`ProcessMonsters`（`:192-361`；regen 分支 `:207-254`）——**订正（2026-10-17）**：`RegenMonstersTick` 由
`MonsterThread.cs:38` 的 ctor 初始化为 `HUtil32.GetTickCount()`，判据 `:207` 为
`(GetTickCount()-RegenMonstersTick) > RegenMonstersTime`；部署 `E:/MirServer/M2GameSvr/Server.conf:50`
`RegenMonstersTime=200`（代码缺省 `GameSvrConf.cs:1433` 亦 200）⇒ **首轮差≈0 不进 regen，首次 regen 在启动后约 200ms**
（原写「第一轮立即」不成立）。结论不变：线程一旦跑起来就会对全图 MonGen 抽数（墙钟/线程调度不确定），
故场景模式仍应把它推迟到 pin 之后。
⇒ 若场景模式不**推迟线程启动**，pin 之前就可能已有不确定抽数。推荐：场景模式下把
`InitializationMonsterThread()` 推迟到 pin 之后（仅 env 开时；默认路径一字不改）。

**(F3) 墙钟消费者使 `monster_ai` 组在窗口内不可逐抽确定。**
C# 怪物 `Run` 门 = `(now - RunTick) > RunTime(250ms)`（`WorldServer.MonGen.cs:280`、
`BaseObject.cs:440`），线程循环 `Thread.Sleep(20)`（`:358`）；Rust 是每 **200ms** tick 无条件跑一次
`monster_ai_tick`（`world.rs:1313-1314`、`monai.rs:861`），**没有 `RunTime` 门**（`monai.rs` 无
`run_tick/run_time` 引用；该缺口已在 `design-buff-and-tick-20261016.md:518` 的 **B-L2** 立项，未落地）。
另外 C# 的 `HUtil32.GetTickCount` 是墙钟 `Environment.TickCount`（`src/OpenMir2/HUtil32.cs:36-39`），
C# 侧**没有** `MIR2_CLOCK_MS` 注入（`clock.rs` 模块头自陈「C# 半仍未落地」，`rng-hook/StartupHook.cs`
无 CLOCK 处理）；Rust 侧 AI 全部读 `world.clock_ms`（`monai.rs:248/352/427/496/…`）。
⇒ 在 600 tick / 120s 窗口里，`monster_ai` 的**抽数个数**大概率两侧不等
（`Wondering` 自身另有 1400ms `WalkSpeed` 门，只能把差异压小、不能消掉）。
**结论：pin 能保证 `spawn`/`drop` 组同序；`monster_ai` 组要绿，必须先落 B-L2 + C# 时钟注入（或 owner
批准等价的「固定 AI 抽数预算」口径）。** 这是本设计最重要的诚实边界，见 §7 O3。

---

## 3. 候选路径对照表

记号：**前置抽** = 进入 `CreateMonster` 之前该路径自身消耗的抽数；**生成期** = 构造链 6 + `Dir` 1 +
掉落（7 基础 + 条件）+ CoolEye 1；`(18,20)` 可走 ⇒ 移位 0 抽。**折叠** = 与 Rust `spawn` 组
（`FOLD_RULES={construct:spawn}`，`rng_consumer_groups.py:57`）的可比性。

### 3.1 逐条

**P1 · GM 命令 `@怪`（Mob）**
- 触发：`CM_SAY(3030)`（`Messages.cs:147`）→ `PlayObject.Message.cs:1394` → `PlayObject.Chat.cs:229`
  `ProcessUserLineMsg` → `:363-368` 以 `@` 开头 → `CommandSystem.Execute`。
- oracle：`MobCommand.cs:15` → `:40 GetFrontPosition`（`BaseObject.cs:977-1038`，玩家 `(17,20) dir=2`
  ⇒ `(18,20)`）→ `:43 RegenMonsterByName` → `:47-49 SlaveExpLevel/RecalcAbilitys/RefNameColor`。
- RNG：前置 0；生成期同链；`RefNameColor` 0 抽（`BaseObject.cs:730-733` 只发 `RM_CHANGENAMECOLOR`）。
- 帧面：`AddToMap` 发 `RM_TURN`（`BaseObject.cs:1375`）⇒ 观察者 `SM_TURN(10)` + `SM_FEATURECHANGED(41)`
  （`PlayObject.Message.cs:1524-1561`）；`RefNameColor` ⇒ `SM_CHANGENAMECOLOR(656)`（`:1677-1678`）。
- 折叠：可折。
- **不可选**：① 权限墙（§2.3）；② `Dir` 随机（§2.4），与 scenario `dir=0` 冲突；③ 登录期多 **5** 个
  construct 抽（F1）。要救需同时动权限（内容/源码）+ 改 scenario + 补 Rust 登录 5 抽，代价最大。

**P2 · GM 命令 `@Mission` + `@MobPlace`**
- 触发：`CM_SAY` `@Mission 18 20`（`MissionCommand.cs:30-33` 设 `BoMission/MissionMap/MissionX/Y`）+
  `@MobPlace 18 20 稻草人 1`（`MobPlaceCommand.cs:21-27`、`:37-46`、`:49-51`）。
- RNG：命令本身 0 抽；生成期同链；无 `GetFrontPosition`；无 `RefNameColor`（少 `SM_CHANGENAMECOLOR`）。
- 帧面：`SM_TURN` + `SM_FEATURECHANGED`。
- **不可选**：同 P1（权限墙 + `Dir` 随机 + F1）；且 `MobPlace` 还依赖 `SystemShare.BoMission`，
  需要两条命令的时序。

**P3 · `@ReCallMobEx` / `@bb`（RecallMob）**
- `ReCallMobExCommand.cs:21-24` 参数 → `:46 RegenMonsterByName` → `:49-56 Master/SlaveExpLevel/
  NameColor/RecalcAbilitys/RefNameColor/SlaveList`；`RecallMobCommand.cs:43-69`（`bb`，
  `Command.conf:84`）= `GetFrontPosition` + 循环。
- RNG：生成期同链；`RecalcAbilitys`（`BaseObject.Abil.cs:15`）/`RefNameColor` 0 抽。
- 帧面：`SM_TURN` + `SM_FEATURECHANGED` + `SM_CHANGENAMECOLOR`；宝宝身份改变后续 AI（`SlaveAttackTarget`，
  `ATMonster.cs:29-68`）⇒ tick 段不能与裸怪比。
- **不可选**：权限墙 + `Dir` 随机 + F1 + slave 语义污染 tick 段。

**P4 · NPC 脚本 `MONGENEX` / `RECALLMOB`**
- 触发：`CM_CLICKNPC(1010)`/`CM_MERCHANTDLGSELECT(1011)`（`Messages.cs:62-63`）→
  `PlayObject.Base.cs:674 ClientClickNpc` → `NormNpc.Click`；脚本 `MONGENEX`
  `ExecutionProcessingSys.cs:5466-5488`（每只先 2 抽 `Random(range*2+1)`，`:5481-5482`）/ `RECALLMOB`
  `:5375-5398`。
- RNG：`MONGENEX` 前置 2 抽/只，且站点无 `spawn` 映射 ⇒ `other` ⇒ `--strict` 的 `GROUP_SET_DIFF`
  （`rng_ledger_diff.py:497`）；`RECALLMOB` 生成期同链但要求已有 NPC/脚本。
- 现状：仓内脚本仅 `T118/Q001` 的 `MONGENEX`；0137 无 NPC/脚本引用 ⇒ 要落 0137 必须改内容。
- **不可选**：权限墙 + `Dir` 随机 + F1 + 改内容/站点不可折。

**P5 · 其余脚本/命令（事实是不可用，不是路径）**
- 脚本 `MONGEN`（`ExecutionProcessingSys.cs:2141-2152`）：map 与怪名都用 `sParam1`、X/Y 都用
  `nParam1` ⇒ `FindMap(怪名)` 必失败；先抽 2 次后 `CreateMonster` 在 `CreateMonster` 的 `FindMap` 处
  （`WorldServer.MonGen.cs:501-504`）返回 null（0 抽）。
- 脚本 `MOBPLACE`（`:3462-3485`）：`nX/nY/nCount/nRange` 恒 0 ⇒ 循环 0 次，纯 no-op。
- `MONGENP` 只有枚举（`src/Modules/ScriptEngine/Consts/ExecutionCode.cs:48`）无处理器；`MobNpcCommand` / `MoveMobToCommand` /
  `NpcScriptCommand` 关键段注释/恒空。
- **不可选**：不可达或 no-op。

**P6 · `MonGen.txt` 启动期加行**
- `LocalDB.cs:501-590 LoadMonGen` → `WorldServer.MonGen.cs:30 InitializeMonster`；`ProcessMonsters`
  门 `:207-254`（方法到 `:361`；`RegenMonstersTick` 初值 = `MonsterThread.cs:38` 的 `GetTickCount()`，
  `RegenMonstersTime=200` ⇒ 首轮约 200ms 后）；`RegenMonsters` `:811-878`。
- RNG：`InitializeMonster` 每怪名 `Random(limit)` + `MonsterList.Values` 每模板 `Random(limit)`
  （`:66`/`:94`，部署 `limit=1` 仍推进流）；每只再 `Random(range*2+1)`×2（`:849-850`）→ 生成期。
- 折叠：位置 2 抽登记在 `spawn`（`consumer_groups.tsv:56`），理论可折；
  **但** 场景门要求两侧 `mongen_rows` 逐行等于冻结件 **0 行**（`rng_scene_gate.py` 的
  `mongen-equal:{a,b}-vs-fixture`）⇒ 加行即 RED。
- **不可选**：内容冻结 + 场景门直接 RED + `Dir` 仍随机。

**P7 · 客户端 `CM_SPELL(3017)` 召唤技能**
- `Messages.cs:139`；`PlayObject.Attack.cs:1054 ClientSpellXY` → `MagicManager.cs:319/372-373`
  `SKILL_SKELLETON` → `:1308 MagMakeSlave` → `BaseObject.cs:1179 MakeSlave` → `CreateMonster`。
- 怪名来自 `Config.Skeleton`/`SkeletonArray`/`Dragon`/`Angel`（不是稻草人；部署 `Server.conf` 无
  `Skeleton` 行）。要出稻草人必须改配置（内容）。
- RNG：生成期同链（`CheckAmulet/UseAmulet` 不抽；`DoSpell` 先发 `RM_SPELL ⇒ SM_SPELL`）。
- **不可选**：内容改动 + 需已学技能 + 宠物语义 + `Dir` 随机 + F1。

**P8 · 其它内部 `RegenMonsterByName` 调用点**
- `BeeQueen.cs:37`、`BoneKingMonster.cs:34`、`ScultureKingMonster.cs:47`、`SpiderHouseMonster.cs:40`、
  `MonsterObject.cs:100-130 MakeClone`、`UserCastle.cs`/`CastleOfficial.cs`、`LocalDB.cs:130`。
- 都进同一 `CreateMonster`，但怪名/坐标/触发不可由客户端任意指定；`WallStructure.cs:19` 的 `Dir=0`
  只适用于城墙。
- **不可选**：不可控。

**P9 · BotSrv 现状（T-59）**
- **T-59 = 现有登记**（`mir2-rs/tests/parity/whitelist.md:108`）：`src/BotSrv/**` 的「M3 RNG 场景驱动器」
  只落了**纯函数 + 默认关静态位**，并明确写「**驱动侧不新增任何发送/日志/文件**」「**本期不接入
  `RobotPlayer` 主循环**——只落纯函数供 real wiring 调用」；固定动作序列的 **real wiring**（进 0137 +
  pin 单只固定怪 + dump 描述件）属 **W9 窗项、显式未验项，判级 = R**，未跑之前不得 `closed`。
- 能做：真实协议栈；`SendSay` 已实现（`RobotPlayer.cs:4644-4703`，`CM_SAY` 在 `:4695` 编码发送）
  但**无生产调用者**（唯一引用 `:6433` 是注释）。
- 不能做：服务端生成怪、设置服务端 `Permission`、读服务端随机流。
- 场景纯函数：`MShare.cs:1014 g_boSceneSequence`、`:1025 ParseSceneSteps`、`:1055 SceneStepOp`、
  `:1079 SceneMonsterKey`、`:1094 BuildSceneMonstersJson`、`:1113 BuildSceneDescriptorLine`；
  仅被 `SceneSeqSelfCheck.cs:32+` / `Program.cs:55-58 --selftest-sceneseq` 调用；
  `RobotOptions.cs` 无任何 `Scene*` 配置项；主循环无 pin/pose/idle 执行器、无 `csharp-scene.json` 落盘。
- **不可选（单独）**：T-59 的 real wiring 全空（W9/R，显式未验）；且即使接上 `CM_SAY`，仍受权限墙 +
  `Dir` 随机 + F1（5 抽）。要走 BotSrv 即等于先落 T-59 real wiring（另立 W9 窗口与 R 级判据），
  不能算本设计的现成路径。

**P10 · 客户端协议面**
- 无「召唤/生成怪并指定坐标+方向」的专用 `CM_*`；与生成相关的只有 `CM_OPENDOOR(1002)`、
  `CM_CLICKNPC(1010)`、`CM_MERCHANTDLGSELECT(1011)`、`CM_SPELL(3017)`、`CM_TEST(1017)`。
- **不可选**：不存在这条链路。

**P11 · 启动期 env 门 pin（推荐；C# 需 T-*，Rust 仅 `bins/game-svr`）**
- 触发：两侧启动期、所有启动抽数之后、tick/怪物线程正式跑之前。
- 前置抽：0；生成期 = 现有 `RegenMonsterByName`/`create_monster_oracle` 原样。
- `Dir`：生成后覆盖为 scenario 值（复用 `LocalDB.cs:133` / `place_guards` 的语义，**不新增抽**）。
- 帧面：无玩家会话 ⇒ 无观察者，`RM_TURN` 不产生下行帧（`SendRefMsg` 扫不到人）；不影响流水。
- 折叠：spawn+drop 可直接比；`monster_ai` 见 F3。
- 改动：C# `src/GameSrv/**` 一处（T-* 登记）；Rust `bins/game-svr/**`（不改 `crates/world`）。
- **可选（推荐）**。

**P12 · 仅扩展现有 `rng-hook`（不改 `src/**`）**
- 通过反射在 `SystemShare.WorldEngine` 就绪后调 `RegenMonsterByName` + 覆盖 `Dir` + dump。
- 优点：零 `src/**` 改动，T-* 不涉及 oracle 源码。
- 缺点：钩子是进程外反射 + 轮询，**无法在确定性接缝上同步**（`InitializeMonster` 结束 → LoadMerchant →
  LoadGuardList → pin 之间没有任何可从 hook 稳定观测的状态位；`StartReady` 只能轮询，且怪物线程
  已启动（F2））。与「对拍一旦 flaky 即回退」冲突。
- **备选（仅当 owner 拒绝 T-* 源码改动时）**：作为 fallback，不推荐。

**P13 · 登录后 pin（GM 或 enter 钩子）**
- 触发：玩家进世界 tick 后。
- RNG：生成期同链，**但前置多 5 个 ctor 抽**（`BaseObject` 2 + `AnimalObject` 2 + `PlayObject Random(999999)` 1，见 F1）。
- 优点：与 `@Mob` 语义最接近，`start_pose` 是真实观察值。
- 缺点：必须给 Rust 补 construct 抽（R 级，影响所有含登录流）或接受 `FOLDED_COUNT_DIFF`；仍需权限或 T-*；
  `Dir` 仍需覆盖。
- **不选**（除非 owner 明确要求「必须有客户端会话」且接受 R 级改动）。

### 3.2 对照表

| 路径 | 触发时机 | 前置抽 | 进同一 CreateMonster | Dir 可控 | 与 Rust spawn 折叠 | 需改 `src/**` | 需改内容 | 现实可跑 | 结论 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| P1 `@怪` | 登录后 | 0 | 是 | **否** | 可 | 否（但权限需） | 权限需内容或源码 | 否（权限墙） | 否 |
| P2 `@Mission`+`@MobPlace` | 登录后 | 0 | 是 | **否** | 可 | 否 | 权限 | 否 | 否 |
| P3 `@ReCallMobEx`/`@bb` | 登录后 | 0 | 是 | **否** | 可（生成段） | 否 | 权限 | 否 | 否 |
| P4 NPC `MONGENEX` | 登录后 | **2/只** | 是 | **否** | **否**（other） | 否 | 脚本 | 否 | 否 |
| P4' NPC `RECALLMOB` | 登录后 | 0 | 是 | **否** | 可（生成段） | 否 | 脚本 | 否 | 否 |
| P5 其余脚本 | — | — | no-op | — | — | — | — | 否 | 否 |
| P6 MonGen.txt 行 | 启动 | 2/只 | 是 | **否** | 可（spawn） | 否 | **是** | 否（门 RED） | 否 |
| P7 `CM_SPELL` | 登录后 | 0 | 是 | **否** | 可（生成段） | 否 | **是** | 否 | 否 |
| P8 内部调用 | 运行期 | 0 | 是 | 否 | 可 | — | — | 不可控 | 否 |
| P9 BotSrv | — | — | — | — | — | **是** | 否 | 未接线 | 否（单独） |
| P10 协议 | — | — | — | — | — | — | — | 不存在 | 否 |
| **P11 启动 pin** | **启动/pre-tick** | **0** | **是** | **是** | **可** | **是（T-*）** | **否** | **可实现** | **推荐** |
| P12 hook 反射 pin | 启动/轮询 | 0 | 是 | 是 | 可 | 否 | 否 | 时序不稳 | 备选 |
| P13 登录后 pin | 登录后 | **+5（构造 5 抽）** | 是 | 是 | 需补 Rust 抽 | 是（T-*） | 否 | 代价大 | 否 |

---

## 4. 推荐路径与理由

### 4.1 选择

**P11 启动期 pin**，具体形态：

1. **C# 侧**（新增 T-*，建议编号 **T-73**；实际以 `m3_whitelist_registry_audit.py --strict` 为准）：
   在 `GameApp.InitializeWorld` 末端（`GameApp.cs:222-250`）加一个**默认关**的 `ScenePin.Apply()`：
   - 环境变量 `MIR2_PIN_SCENE=<scene-pin.json 路径>`；未设/空 ⇒ 完全不走（0 抽、0 日志、0 文件）。
   - 场景模式下把 `SystemShare.WorldEngine.InitializationMonsterThread()`（`GameApp.cs:238`）**推迟**
     到 pin 之后（只为消除 F2 的并发刷怪；默认路径不动）。
   - pin 调 `SystemShare.WorldEngine.RegenMonsterByName(spec.map, spec.x, spec.y, spec.name)`，
     成功后 `mon.Dir = spec.dir`（照 `LocalDB.cs:133`）；返回 null ⇒ 打 `SCENE_PIN_FAIL` 并以非 0 退出
     （fail-closed，禁止空场景静默产流）。
   - dump `<side>-scene.json`（字段见 §5.4）。
2. **Rust 侧**（仅 `bins/game-svr/**`）：在 `main.rs` 守卫块结束 `:8560` 之后、tick 循环 `:8628` 之前，
   调一个 `scene_pin::apply_from_env(...)`：
   - 同一 `MIR2_PIN_SCENE`、同一 JSON 格式；未设/空 ⇒ `Ok(None)`，0 抽、0 帧。
   - 解析后 `std::mem::replace(world.rng, …)`（照 `place_guards:142`），循环 `count` 次调
     `guardlist::regen_monster_by_name`（复用 `create_monster_oracle`），成功后覆盖
     `e.ai.dir = spec.dir` 与 `displays` dir（照 `place_guards:164` / `:166-168`）。
   - 返回 `None`（线程表缺名/地图缺/模板缺）⇒ `glog!("ERR", …)` + `std::process::exit(97)`（fail-closed）。
   - dump `<side>-scene.json`。

### 4.1a 硬约束：两侧必须同相（同启动期或同登录后）

**不许混相。** pin 的抽数必须与另一侧处在**同一阶段**的同一位置：要么两侧都在启动期（pre-tick），
要么两侧都在登录后。理由：

- RNG 流是**进程级单一序列**；混相时 pin 抽在两侧与**不同的前置消费者**交错——C# 登录先耗 5 个
  ctor 抽（§2.6-F1），Rust 进图先跑 `shift_enter_position`（本例 `(17,20)` 可走 ⇒ 0 抽，但通用场景非 0）。
- 即使把 `construct` 折叠到 `spawn` 后**总计数相等**，`groupdiff` 的**共享组相对次序**
  （`SHARED_REL_ORDER_DIFF`）也**不可比**：pin 的 spawn 抽在 C# 流里的相邻前驱是那 5 个 construct，
  在 Rust 流里是进图移位（0 或若干抽）——两侧「紧邻 pin 抽之前的条目」不是同一类。
- ⇒ 「折叠后计数相等」**不能**替代「同相」；同相是 `SHARED_REL_ORDER_DIFF` 判据成立的前提。

**本设计选双侧启动期的理由**：① 避开 F1 的 5 抽不对称；② C# 场景模式下把怪物线程启动推迟到 pin
之后（F2），启动期是唯一能保证「pin 之前没有并发 spawn 抽」的接缝；③ Rust 侧 `place_guards` 之后、
`world.tick`/`step_mon_gen` 之前同样是确定性接缝；④ 两侧 pin 前的前缀都只含已建模的启动构造段
（69 抽 / 18 站点）与守卫，可逐项对拍。

### 4.2 为什么是它

- **唯一同时满足四个硬条件**的路径：不改内容、不碰权限墙、`Dir` 可控、生成期取数序 100% 复用 oracle 函数。
- **不与构造段 69 抽/屏障 A 冲突**：调用点在 `:8302/:8319/:8331/:8340/:8369` 全部之后（§5.5 给出证明）。
- **避开 F1**：无玩家会话 ⇒ 不产生登录的 5 个 ctor 抽（含 `PlayObject..ctor Random(999999)`）；Rust 无需补登录抽。
- **避开 F2**：场景模式推迟 C# 怪物线程启动；两侧的「pin 之后才开始刷怪/AI」同构。
- **可离线构造级验证**：`regen_monster_by_name` 已有 `RecordingSink` 测试面（`mongen.rs:781-998`），
  pin 只是它的第二个调用者。
- **判据分级**：本件属 **R**（触 RNG 抽数序列 + `Dir` 影响 AI）。真机窗口未跑 ⇒ `review` 必须
  `window_required: true`；不许用「语义等价」盖掉。

### 4.3 不选其它路径的一句话理由

P1–P4/P7 全部被 §2.3 权限墙挡住，且 `Dir` 不可控、F1 多 **5** 抽；P5 不可达；P6 改内容且场景门直接 RED；
P8 不可控；P9 只有 T-59 的纯函数、real wiring 属 W9/R 且显式未验；P10 协议不存在；P12 时序不稳（违确定性）；
P13 需 R 级补 Rust 登录 5 抽。
**判不准 ⇒ 按 R 保守**：凡涉及 `Dir`/抽数序的取舍，一律选「不动流、只覆盖存储值」的方案。

---

## 5. Rust 侧镜像规范（承接卡可直接照做）

### 5.1 开关与输入

- env 名：`MIR2_PIN_SCENE`
- 取值：一个 UTF-8 JSON 文件路径（避免 Windows 代码页把 CJK 怪名弄坏）。建议 schema：

```json
{
  "canon": "m3-rng-scene-pin-v1",
  "map": "0137",
  "x": 18,
  "y": 20,
  "name": "稻草人",
  "dir": 0,
  "count": 1,
  "seed": 42,
  "start_pose": {"x": 17, "y": 20, "dir": 2},
  "descriptor_out": "E:/tmp/.../rust-scene.json",
  "trace": "E:/tmp/.../rng-stream.tsv"
}
```

- **默认关**：未设/空串 ⇒ 不读文件、不抽数、不写描述件、不新增日志。
- **fail-closed**：文件不存在/坏 JSON/字段缺失/类型错/`count<=0`/`seed` 与 `MIR2_RNG_SEED` 不等
  ⇒ 非 0 退出（建议 97，与 `seed.rs` / `MIR2_RNG_LOG` 同族纪律）。**不许静默降级为空场景。**
- 只允许 `count=1` 用于当前 scenario；`count>1` 可支持但描述件多重集必须如实累加（改坏必红会抓）。

### 5.2 注入点（Rust）

插入在 `main.rs`：

```text
main.rs:8538  place_guards(...)                 // 守卫抽数（既有）
main.rs:8560  }                                  // 守卫块结束
   ← 在这里插 scene_pin::apply_from_env(...)
main.rs:8565  state_log::StateLog::from_env()    // 0 抽
main.rs:8628  loop {                             // tick 循环
main.rs:9957      step_mon_gen(...)              // pin 之后才刷 MonGen
main.rs:9993      world.tick()                   // pin 之后才跑 monster_ai
```

- **必须**在 `place_guards` 之后（否则与 C# 的 LoadGuardList 抽数相对次序相反）。
- **必须**在 `world.tick` / `step_mon_gen` 之前（否则 pin 的 `spawn` 组会排在首批 AI/MonGen 抽之后）。
- `mon_gen_host` 为 `None` 而 env 已设 ⇒ fail-closed（无 `MonsterThreadMap` 无法复现 oracle 的
  `GetMonsterThreadId` 门）。
- `enter_cfg.rng_seed` 为 `None` 而 env 已设 ⇒ fail-closed（无法证明同种子）。

建议代码骨架（写集仅 `bins/game-svr/src/{scene_pin.rs,main.rs}`）：

```rust
// bins/game-svr/src/scene_pin.rs
pub fn apply_from_env(
    world: &mut mir2_world::World,
    table: &game_svr::mongen::MonsterTable,
    host: &mut game_svr::mongen::MonGenHost,
    registry: &mir2_envir::MapRegistry,
    displays: &mut downlink::DisplayTable,
    injected_seed: Option<i32>,
) -> Result<Option<PinReport>, String> {
    let Some(path) = std::env::var_os("MIR2_PIN_SCENE") else { return Ok(None) };
    if path.is_empty() { return Ok(None); }
    let spec = PinSpec::load(&path)?;               // fail-closed 解析
    let seed = injected_seed.ok_or("MIR2_PIN_SCENE 需要 MIR2_RNG_SEED")?;
    if spec.seed != seed { return Err("pin spec seed != MIR2_RNG_SEED".into()); }

    let mut rng = std::mem::replace(&mut world.rng, RandomNumber::with_seed(0));
    let mut report = PinReport::default();
    {
        let mut sink = game_svr::mongen::WorldMonsterSink { world, registry, table, displays };
        for _ in 0..spec.count {
            let id = game_svr::guardlist::regen_monster_by_name(
                &mut rng, &mut sink, table, &mut host.state,
                &spec.map, spec.x, spec.y, &spec.name,
            ).ok_or_else(|| "SCENE_PIN_FAIL: regen_monster_by_name -> None".to_string())?;
            if let Some(e) = sink.world.entities.get_mut(id) { e.ai.dir = spec.dir; }
            if let Some(mut d) = sink.displays.get(id).cloned() {
                d.dir = spec.dir;
                sink.displays.set(id, d);
            }
            report.ids.push(id);
        }
        report.descriptor = build_descriptor(&spec, sink.world, sink.registry, host)?;
    }
    world.rng = rng;
    write_descriptor(&spec.descriptor_out, &report.descriptor)?;
    glog!("INF", "SCENE_PIN map={} name={} x={} y={} dir={} count={} ids={:?} descriptor={}",
          spec.map, spec.name, spec.x, spec.y, spec.dir, spec.count, report.ids,
          spec.descriptor_out.display());
    Ok(Some(report))
}
```

### 5.3 必须复用 `create_monster_oracle` 的哪一段

**整段复用**，不重写、不复制：

- Rust 入口：`guardlist::regen_monster_by_name`（`guardlist.rs:73`）。
- 它内部 `create_monster_oracle`（`mongen.rs:472`）→ `draw_monster_construct`（`:326`）→
  `random_byte(8)`（`:490`）→ `budget_drops`（`:491`，`WorldMonsterSink::budget_drops` `:698`）→
  `random_below(100)`（`:493`；`:492` 是读 `cool_eye_code`）→ 地形移位（`:500-533`；`spawn_monster` 请求 `:539-550`）。
- **禁止**在 pin 里另写一套抽数（会与 oracle 站点失配，`consumer_groups` 的 `spawn` 行无法命中）。
- **禁止**为了置 `Dir` 而在 `create_monster_oracle` 内部改序；置向必须发生在 `regen_monster_by_name`
  返回之后（`RandomByte(8)` 照抽，只是结果被覆盖），与 `place_guards` 完全同法。

### 5.4 描述件字段来源（两侧同口径）

| 字段 | 来源（Rust） | 来源（C#） |
| --- | --- | --- |
| `side` | 常量 `"rust"` | 常量 `"csharp"` |
| `canon` | `"m3-rng-scene-descriptor-v1"` | 同左 |
| `map` | `spec.map` | `spec.map` |
| `seed` | `enter_cfg.rng_seed`（必须 == `spec.seed`） | `MIR2_RNG_SEED` 解析值 |
| `start_pose` | `spec.start_pose`（**声明值**；无玩家会话时不是运行观察值，见 O5） | 同左 |
| `mongen_rows` | `host.state.entries` 中 `map_name == spec.map` 的 `[x, y, range]`（文件序） | 按 `extract_mongen.py` 同口径读 `Envir/MonGen.txt`（本场景 `[]`） |
| `monsters` | pin 后按 `world.entities.iter()` 过滤 `map_id == 0137 && !death && !ghost` 聚合 `(name,x,y,dir)` + count | pin 后 `WorldEngine.GetMapMonster(envir, list)`（`WorldServer.cs:1618-1655`，只收 `!Death && !Ghost`）聚合 |
| `trace` | `MIR2_RNG_LOG` 路径 | `MIR2_RNG_LOG` 路径 |

- `mongen_rows` **不要硬编码 `[]`**：从实际输入算，才能在 scenario/Envir 漂移时被门抓住。
- `monsters` 必须是**运行时实际集合**（`DRIVER-CONTRACT.md` §2），不是把 spec 抄一遍。

### 5.5 与构造段 69 抽 / 屏障 A seq=17 的相互影响

**证明无影响**：

- 69 抽 / 屏障 A 的调用点全部在 pin 之前：
  `8302`（段① 16）、`8319`（屏障 A，第 17 条）、`8331`（段② 36）、`8338-8340`（屏障 B）、`8369`（段③ 16）。
- pin 的调用点在 `8560` 之后 ⇒ 只在账本**尾部追加** pin 的抽数（race 83：6 + 1 dir + 7 基础 drop + 条件
  drop + 1 CoolEye + 0 移位）。
- 屏障 A 的 seq=17 位置由 `crates/world/src/ctor_fold.rs` 模块头（「段① 16 抽，屏障 A 即第 17 条」，`:68-70`）
  与两个**实际存在**的测试固定：`barrier_a_position_matters`（`:606`：把屏障 A 挪到段①之前 ⇒ 段①的
  `result` 列与屏障自身值都变）与 `dropping_barrier_a_shifts_the_following_draws`（`:682`：不 push 屏障 A
  ⇒ 其后段②首抽错位、**第 17 条起分歧**）。仓内**没有**字面上的「恰第 17 条 `assert`」；上述两个测试是
  位置敏感性的机判本体。pin 不改变任何前缀条目。
- **会破坏 69/17 的写法**（明令禁止）：把 pin 插到 `8302` 之前；给 `MonsterTable`/`MonGenHost` 在
  `start` 之前增行；在 `MonGenHost::start` 之前调用任何 `random_*`。
- 守卫：`pin_preserves_ctor_prefix_69_17` **必须与 pin 接线同片落地**——它现在是**尚不存在**的测试，
  没有它 M2 的「改坏必红」不成立。测试内容：用 `RecordingSink` + 独立 `RandomNumber` 跑「构造段 → pin」，
  断言前 17 条的 `(kind,max,result)` 与部署件基线逐字节相等（`fixtures/rng_stream_seed42_*` 只覆盖前
  1828 条 **C#** 流，不能直接当 69/17 基线；69/17 以 `crates/world/src/ctor_fold.rs` 模块头 + 上述两个
  屏障位置测试为准）。

### 5.6 C# 侧镜像（T-*，与 Rust 同形）

`ScenePin` 的最小实现要点：

1. `Active` = `MIR2_PIN_SCENE` 非空；读同一个 JSON（UTF-8）。
2. `InitializeWorld` 中：
   - `if (!ScenePin.Active) SystemShare.WorldEngine.InitializationMonsterThread();`
   - 保持 `LoadGuardList()`（`:241-243`）原位置；
   - `SystemShare.WorldEngine.Initialize()`（`:244`）照跑（实测 0 抽：`BaseObject.Base.cs:14-22` 只有
     `AddToMap`，`MerchantInitialize/NpCinitialize` 的 `Initialize()` 不抽）；
   - pin：`RegenMonsterByName` + `mon.Dir = spec.dir` + dump；
   - `if (ScenePin.Active) SystemShare.WorldEngine.InitializationMonsterThread();`（推迟到此处）；
   - 之后 `GuildMgr.LoadGuildInfo/CastleMgr.LoadCastleList/AutoBot.Initialize`（实测这些 load/ctor
     无 `RandomNumber`：`GuildManager.cs`/`GuildInfo.cs`/`CastleManager.cs`/`UserCastle.cs` ctor 段）
     照旧；`StartReady = true`。
3. `RegenMonsterByName` 返回 null ⇒ `SCENE_PIN_FAIL` + `Environment.Exit(97)`。
4. dump 用 `SystemShare.WorldEngine.GetMapMonster(envir, list)`（`WorldServer.cs:1618-1655`）。
5. `AddToMap` 已用随机 `Dir` 发过 `RM_TURN`（`BaseObject.cs:1375`）；无玩家会话时无人接收。
   若 owner 要求有客户端，需在置向后再发一次 `RM_TURN`（新增帧，另立判据），见 O5。

---

## 6. 机判验收（承接卡可直接抄）

### 6.1 默认关零差异

- 单测 `pin_env_unset_is_noop`：`MIR2_PIN_SCENE` 未设/空 ⇒ `apply_from_env` 返回 `Ok(None)`；
  `world.rng` 的抽数计数不变；不写描述件；不打印 `SCENE_PIN`。
- **进程级（同一命令跑两次；基线件必须先入库，否则不可判）**。判据 = `MIR2_PIN_SCENE` 未设时，同一二进制 /
  同一命令产出的 `rng-stream.tsv` 在「接线前（基线 commit `b5212b36a`）」与「接线后」**逐字节相同**。

  ```sh
  # B1. 接线前采基线（在 b5212b36a 的干净检出上；owner 窗口，起栈禁接管道）
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 \
    -RustBinDir E:/tmp/m3-scene-pin/base/debug -Seed 42 \
    -LogDir E:/tmp/m3-scene-pin/default-off/base \
    -RngLog E:/tmp/m3-scene-pin/default-off/base/rng-stream.tsv
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 -Stop

  # B2. 基线件入库（承接卡必须做；本文不预填未知 hash ⇒ 标「未验证」）
  mkdir -p mir2-rs/tests/parity/m3/evidence-scene-pin-default-off/provenance
  cp E:/tmp/m3-scene-pin/default-off/base/rng-stream.tsv \
     mir2-rs/tests/parity/m3/evidence-scene-pin-default-off/provenance/rust-rng-stream-baseline.tsv
  (cd mir2-rs/tests/parity/m3/evidence-scene-pin-default-off/provenance && \
     sha256sum rust-rng-stream-baseline.tsv > SHA256SUMS.txt)

  # B3. 接线后同一命令产 after，逐字节机判
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 \
    -RustBinDir E:/tmp/m3-scene-pin/after/debug -Seed 42 \
    -LogDir E:/tmp/m3-scene-pin/default-off/after \
    -RngLog E:/tmp/m3-scene-pin/default-off/after/rng-stream.tsv
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 -Stop
  cmp E:/tmp/m3-scene-pin/default-off/base/rng-stream.tsv \
      E:/tmp/m3-scene-pin/default-off/after/rng-stream.tsv
  (cd mir2-rs/tests/parity/m3/evidence-scene-pin-default-off/provenance && sha256sum -c SHA256SUMS.txt)
  ```

  - **基线文件（仓内路径）**：`mir2-rs/tests/parity/m3/evidence-scene-pin-default-off/provenance/rust-rng-stream-baseline.tsv`；
    **sha256 期望** = 同目录 `SHA256SUMS.txt` 里登记的值，`sha256sum -c SHA256SUMS.txt` 必须打印
    `rust-rng-stream-baseline.tsv: OK`。该值由 B1 产出；**本文不预填（未验证）**——预填一个没跑过的
    hash 等于假证据。
  - 同一次 run 的启动日志必须 `grep -c 'SCENE_PIN' <log>` = **0**，且 `descriptor_out` 指向的路径**不存在**。
  - **C# 半**（`rng-baseline.ps1` 已入库，`-VerifyRepeat` 自证两次同种子逐字节一致）：
    `powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/rng-baseline.ps1 -Seed 42 -PrefixCalls 1828 -VerifyRepeat`
    ⇒ 确定性前缀 == `fixtures/rng_stream_seed42_head1828.tsv` 的数据行；机判
    `tail -n +5 mir2-rs/tests/parity/m3/fixtures/rng_stream_seed42_head1828.tsv | sha256sum`
    = **`640a7eb412fd5291557ac598e3f2821e52f649c4af996fe06b87f4c2e882d6bb`**（1828 行；本件已实测）。
- 门禁：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、
  `cargo test --workspace` 全绿（§AGENTS 本地门禁）。

### 6.2 构造级逐抽期望

用一个 `RecordingSink`（照 `mongen.rs:781-998`）+ 独立 `RandomNumber::with_seed(42)`：

1. 断言 pin 的 `max` 序列前缀为：
   `[1500, 2000, 3000, 3000, 2000, 1500, 8, 30, 200, 200, 200, 200, 200, 200, 100, …]`
   - 前 6 = race 83 构造链；
   - 第 7 = `Dir` `RandomByte(8)`；
   - 第 8–14 = `MonItems/稻草人.txt` 7 行的 `Random(MaxPoint)`（金币行 `10/30` ⇒ 30；其余 6 行 `10/200`）；
   - 期间的 `Random(300)`/`Random(80)`/`Random(MonRandomAddValue)` 只在命中时出现（**数量随种子确定**，
     由第一次窗口实测固化；不要把它写死成常量）；
   - 最后的 `Random(100)` = CoolEye；
   - 之后**无地形移位抽**（`(18,20)` walkable；断言 `attempts == 0`）。
2. 断言 `regen_monster_by_name` 返回 `Some(id)`，`world.entities[id].ai.dir == 0`，
   `displays[id].dir == 0`，且**覆盖前后抽数计数相同**（置向 0 抽）。
3. 断言 `create_monster_oracle` 的抽数站点仍落在 `bins/game-svr/src/mongen.rs`（`spawn` 组，`consumer_groups.tsv:79`），
   `budget_drops`（`mongen.rs:698` → `World::budget_monster_drops` `world.rs:1522` → `generate_drops`）内部的
   `mon_get_random_items` 抽数站点仍落在 `crates/formula/src/drop.rs`（`drop` 组，`consumer_groups.tsv:50`）。

### 6.3 描述件通过 `rng_scene_gate.py`

```sh
cd mir2-rs/tests/parity/m3
python -I rng-trace-scene/rng_scene_gate.py gate \
  --scenario rng-trace-scene/rng_trace_scenario.json \
  --a <csharp-scene.json> --b <rust-scene.json> \
  --a-stream <csharp-rng-stream.tsv> --b-stream <rust-rng-stream.tsv>
```

期望 `rc=0`（GREEN）：`map/seed/pose` 两侧相等、`monsters` 多重集 == scenario
（`[(稻草人,18,20,0)]`）、`mongen_rows` 两侧 == 冻结件（0 行）、两份流水头 `INJECT_OK 42`。

### 6.4 `groupdiff --fold --strict` 的期望

```sh
python -I rng-trace-probe/rng_ledger_diff.py groupdiff \
  <csharp-rng-stream.tsv> <rust-rng-stream.tsv> \
  --a-side csharp --b-side rust --fold --strict \
  --scenario rng-trace-scene/rng_trace_scenario.json
```

- **spawn / drop 段**：折叠后计数与相对次序必须相等（pin 的 6 ctor + 1 dir + 1 CoolEye = `spawn`；
  7+条件 = `drop`）。
- **monster_ai 段**：在 F3 未收口前，**预计 `SHARED_REL_ORDER_DIFF`**（两侧抽数个数受墙钟/250ms 门影响）。
  这不是 pin 失败；报告必须把差异限定在 `monster_ai` 组，并引用 F3。
- 一旦 B-L2 + C# 时钟注入落地，期望 `VERDICT IDENTICAL`。
- **前提（硬约束）**：两侧必须同相（都启动期或都登录后，§4.1a）。混相时即使折叠后计数相等，
  共享组相对次序也**不可比**（`SHARED_REL_ORDER_DIFF` 的结论无效）。

### 6.5 改坏必红（≥3；建议 6 条）

| # | 改坏 | 必红信号 |
| --- | --- | --- |
| M1 | 把置向写成**强制错向**：Rust `e.ai.dir = (spec.dir + 1) % 8`，C# `mon.Dir = (byte)((spec.dir + 1) % 8)` | 描述件 `monsters` 的 dir = 1 ≠ scenario 的 0 ⇒ 门 `monsters-equal:{scenario-vs-a/b,a-vs-b}` RED。**注**：仅删掉 `e.ai.dir = spec.dir` **不保证红**——`RandomByte(8)`（`mongen.rs:490` / `WorldServer.MonGen.cs:740`）在 seed 42 的该流位置完全可能恰为 0，删掉后描述件仍可能是 `dir=0` |
| M2 | 把 pin 调到 **`main.rs:8302`（构造段①）之前** | **前提**：`pin_preserves_ctor_prefix_69_17` 必须与接线**同片落地**。现状仓内只有 `crates/world/src/ctor_fold.rs` 模块头与 `barrier_a_position_matters` / `dropping_barrier_a_shifts_the_following_draws` 两个屏障位置测试，**没有** 69/17 前缀机判；该测试缺席时本条不成立。落地后：断言前 17 条 `(kind,max,result)` 与部署件基线逐字节相等 ⇒ 红。**订正（2026-10-17）**：只把 pin 从「启动期正常位置」挪到 `MonGenHost::start`（`:8340`）之前**不会**让前缀测试红——pin 落在屏障 A（`:8319`）之后时前 17 条（段① 16 + 屏障 A 1）不变，只有 groupdiff `SHARED_REL_ORDER_DIFF` 会红；要咬住前缀测试必须把插入点钉在 `:8302` 之前 |
| M3 | 删掉 pin 调用但保留 env | **可达信号**：根本不会写 `<side>-scene.json` ⇒ `rng_scene_gate.py` 报 `PREREQ_MISSING`（rc=2，缺描述件）⇒ `run_rng_trace_pair.ps1 -SceneGroupDiff` 打 `REFUSED` 并**拒跑** groupdiff（rc=2）。**注意**：`MISSING_EXPECTED_GROUPS(spawn)` / `MIN_GROUP_DRAWS(drop)` 需要先手工构造描述件让门过，且**依赖 O4** 的影子 Envir 把其它图的 MonGen 消费者清零；O4 未落地时该信号不可达 |
| M4 | spec 的 `map` 改成 **`NOPE`**（不存在；**不要用 `0138`**——`0138` 是有效图：`E:/MirServer/Mir200/Envir/MapInfo.txt:437 [0138 …]`、`E:/MirServer/Mir200/Map/0138.map` 存在 3292 bytes，`RegenMonsterByName` 很可能仍成功），或把 `name` 改成不存在的怪（`GetMonRace` 返回 -1） | **确定性失败点**：`NOPE` ⇒ C# `FindMap` 空在 `WorldServer.MonGen.cs:501-504` 返回 null、Rust `sink.map_size` 在 `mongen.rs:483` 返回 `None`（两侧均 **0 抽**）；坏怪名 ⇒ C# race=-1 ⇒ switch 无 case ⇒ `cert == null` 在 `:730-732` 返回 null、Rust `race_is_spawnable(-1)` 假在 `mongen.rs:485-487` 返回 `None`（均 **0 抽**）⇒ `SCENE_PIN_FAIL` + exit 97 ⇒ `run_rng_trace_pair` PREREQ（无流水/描述件） |
| M5 | `count` 改成 2 | 描述件多重集 `(稻草人,18,20,0)×2` ≠ scenario ×1 ⇒ 门 RED |
| M6 | 跳过 `budget_drops`（不调 `budget_drops`） | C# `drop` 有 7+ 抽、Rust 0 ⇒ `FOLDED_COUNT_DIFF(drop)` / `MISSING_EXPECTED_GROUPS(drop)` |

另加两条无栈牙：`pin_env_unset_is_noop`（M7：把 env 读取改成默认开 ⇒ 红）与
`pin_seed_mismatch_fails_closed`（M8：spec.seed=41 ⇒ 非 0 退出）。

### 6.6 命令清单（承接卡落地后）

```sh
# 0) 离线：默认关 + 构造级
cargo test -p game-svr scene_pin
cargo test --workspace            # 合并前门禁

# 1) 真机（owner 窗口；两侧各自起栈，env 由启动器透传）
#    C#: start-csharp-hook.ps1 -PinSceneFile <scene-pin.json>
#        （需给该脚本加 -PinSceneFile，写集 = tests/parity/m3/**）
#    Rust: start_rust_stack.ps1 -Seed 42 -RngLog <...>/rng-stream.tsv
#          并透传 MIR2_PIN_SCENE / MIR2_RNG_SEED / MIR2_RNG_SITE=1
# 2) 场景门 + 单一入口 groupdiff
powershell -ExecutionPolicy Bypass -File rng-trace-scene/run_rng_trace_pair.ps1 -GateOnly \
  -AScene <csharp-scene.json> -BScene <rust-scene.json> \
  -AStream <csharp-rng-stream.tsv> -BStream <rust-rng-stream.tsv>
powershell -ExecutionPolicy Bypass -File rng-trace-scene/run_rng_trace_pair.ps1 -SceneGroupDiff \
  -AScene <csharp-scene.json> -BScene <rust-scene.json> \
  -AStream <csharp-rng-stream.tsv> -BStream <rust-rng-stream.tsv>
```

---

## 7. 未决 / 需 owner 拍板项

| # | 事项 | 现状事实 | 建议 / 影响 |
| --- | --- | --- | --- |
| O1 | **是否允许新增 T-* 改 `src/GameSrv/**`**（启动 pin + 推迟怪物线程启动） | 现有 `whitelist.md` 最高 M3 号为 **T-72**；`src/BotSrv/**` 已有 T-22/29/41… 工具侧例外先例 | 推荐批准 **T-73**（编号以 `m3_whitelist_registry_audit.py --strict` 为准）。若拒绝，只能退到 P12（hook 反射，时序不稳，**R 级 flaky 风险**） |
| O2 | **scenario 的 `dir=0` 是否保持** | 现有路径无一能钉 dir（§2.4） | 推荐保持，并批准「生成后覆盖 `Dir`」（与 `LoadGuardList` 同形，0 抽）。若 owner 改 scenario 为实际抽到的 dir，则 pin 的置向步骤删掉，M1 失效 |
| O3 | **`monster_ai` 组的确定性前置** | C# 无 `MIR2_CLOCK_MS`（`clock.rs` 自陈 C# 半未落地）；Rust 无 `RunTime=250ms` 门（B-L2 未落地，`design-buff-and-tick-20261016.md:518`） | 推荐先落 **C# 时钟注入 + B-L2**，再开窗口；否则 `monster_ai` 预期 `SHARED_REL_ORDER_DIFF`，DoD② 不能算达成。或 owner 明确接受「固定 AI 抽数预算」的替代口径（需另立设计） |
| O4 | **全局非场景消费者（其它图的 Merchant/NPC/MapEvent/MonGen）** | C# 进程级全局 RNG；F2/F3 已证 | 推荐窗口用**影子 Envir**（`start-csharp-hook.ps1` 已是 robocopy 影子；冻结基线不动）把非场景消费者的输入降为 0，或另立「场景隔离流」；需 owner 裁定影子内容 policy。**不改 `E:/MirServer` 原文件** |
| O5 | **是否需要真实客户端会话**（`start_pose` 是否为观察值） | pin 可在无玩家时完成；descriptor 的 `start_pose` 只是声明 | 推荐无客户端（避开 F1）。若 owner 要求有玩家：必须给 Rust 补登录的 **5 个 ctor 抽**（`BaseObject` 2 + `AnimalObject` 2 + `PlayObject Random(999999)` 1；R 级，影响所有含登录流），并把本件 §2.6-F1 的约束改写 |
| O6 | **窗口批次编排** | `rng_trace_scenario.json` 要求与 ④ `drop-exp` 同窗口串行（`check_window_batch.py`） | 按 `DRIVER-CONTRACT.md` §6 / `README.md` §4 执行；本件不占窗口 |
| O7 | **C# 描述件的 producer 归属（T-59）** | 当前 BotSrv 无 producer（§P9）；T-59（`whitelist.md:108`）只登记纯函数，明确「驱动侧不新增任何发送/日志/文件」「本期不接入 `RobotPlayer` 主循环」；real wiring 属 W9、**显式未验**、判级 **R** | 推荐由 T-73 的 `ScenePin` 直接 dump；不扩 BotSrv。若 owner 要求复用 BotSrv 场景件，必须先落 **T-59 real wiring**（W9 独占窗口 + R 级真机判据），并另立 T-* 登记；未落地前该路径不可判 |

---

## 8. 自失效声明（本件哪些句子在落地后必须改写）

1. 「C# 没有任何一条现有路径能把 `Dir` 钉成 0」——**仍是事实**；但若 T-73 落地，§4.1 的
   「推荐形态」要改成「已实现」的版本，并补上 `ScenePin` 的源码行号/符号锚。
2. 「`src/**` 无改动 / 本件零 `src` 写集」——T-73 落地后，§4.1 与卡面写集必须改为
   `src/GameSrv/**` + `mir2-rs/tests/parity/whitelist.md`（T-* 行），并补证据四件套。
3. 「`monster_ai` 预期 `SHARED_REL_ORDER_DIFF`」——C# 时钟注入或 B-L2 任一落地后，§6.4 的期望要改为
   `VERDICT IDENTICAL`（若两者都落地），并在 §2.6-F3 标注「已收口」。
4. 「无玩家会话」——若 O5 改为有会话，§2.6-F1、§5.5、§6.2 的期望全部要改写（多 **5** 个 construct 抽：`BaseObject` 2 + `AnimalObject` 2 + `PlayObject` 1）。
5. 「`dir=0` 用生成后覆盖」——若 O2 改为 scenario 采用实际抽值，§2.4/§4.1/§6.5-M1 要删改。
6. 「`MIR2_PIN_SCENE` 未实现」——落地后 §5.1–§5.5 的「建议」措辞改为「规范」，并补默认关的进程级证据。
7. 本文引用的 `main.rs:8xxx` 行号会随共享文件漂移（`DRIVER-CONTRACT.md` §6 已登记 main.rs 行号漂移 6 次）；
   落地后应改用**符号锚**（`symbol_unique`）或把 pin 调用封装成独立函数名做唯一性机判。
8. **T-59 real wiring 落地后必须改写的句子**：§3-P9 的「real wiring 全空 / 无 producer」、§3.2 表 P9 行
   「未接线」、§4.3 的「P9 未接线」、§7-O7 的「BotSrv 无 producer」都改为「已接线（W9，证据 …）」；
   若 real wiring 真的产 `<csharp-scene.json>`，§5.4/§6.3 的 producer 归属与 §6.6 的启动参数也要换成
   BotSrv 口径。T-59 判级 **R** 不因本设计降低：它落地后仍必须真机全量对拍（W9 独占窗口）。

---

## 9. 输入调查的订正与补强（本件自验）

| 输入说法 | 本件实测 | 处理 |
| --- | --- | --- |
| A：「`MonsterObject.cs:65` / `ATMonster.cs:10`」 | 实际路径 `src/M2Server/Monster/MonsterObject.cs:65`、`src/M2Server/Monster/Monsters/ATMonster.cs:10`（不在 `src/M2Server/Actor/`） | 以实际路径为准 |
| A：「`GameCommand.cs:33-40`」 | `#if DEBUG` 在 `:33-36`，权限检查在 `:37-40`（`:32` 是 `{`） | 引用拆开 |
| A：「Mob 被改名成“怪”」 | `Command.conf:80` GBK 解码 = `Mob=怪`；`:84 RecallMob=bb` | 确认 |
| A：「稻草人 MonItems 7 行」 | `E:/MirServer/M2GameSvr/Envir/MonItems/稻草人.txt` 实测 7 条数据行（含金币 300） | 确认（来源是文件而非 DB；`LocalDB.cs:596 LoadMonitems`） |
| A：「MONGENEX 每只先 2 抽 `Random(range*2+1)`」 | `ExecutionProcessingSys.cs:5481-5482` | 确认 |
| B：「ctor 链（race 80 为 5 抽）」 | race 83 = **6 抽**（AtMonster 多一层 `Random(1500)`） | 以 6 为准 |
| B：「`main.rs:8340 MonGenHost::start`」 | `:8338 let mut mon_gen_host = match …`，`:8340 MonGenHost::start(` | 确认（引用 8338-8340） |
| B：「事件 drain 结束 main.rs:9888」 | `:9888` 是 `while let Ok(ev) …`（起于 `:8630`）的闭括号（`awk` 对 `8630..9888` 括号配平 = 0）；`:9889` 是「踢旧落地」注释 | 确认（本轮复核仍为 9888/9889；对抗复验给的 9887/9888 在 `b5212b36a` 上不成立） |
| B：「`world.rs:1311-1314 monster_ai_tick`」 | `:1311-1312` 注释，`:1313 if self.ai_enabled`，`:1314 monster_ai_tick` | 确认 |
| 两份调查均未提 | **F1** 登录 `Random(999999)`（`PlayObject.cs:1047`，参照流水 seq 5）；**F2** C# 怪物线程启动早于 pin 且全图刷怪；**F3** 墙钟/250ms 门导致 `monster_ai` 抽数不可确定 | 本件新增为 §2.6，直接决定推荐形态与 open items |

---

## 10. 抽验命令（可复现）

```sh
# 生成链 / 构造链
sed -n '382,438p' src/GameSrv/Word/WorldServer.MonGen.cs
sed -n '498,510p;730,750p' src/GameSrv/Word/WorldServer.MonGen.cs
grep -n "RunTick = .*Random(1500)\|SearchTime = .*Random(2000)" src/M2Server/Actor/BaseObject.cs
grep -n "AttackTick = \|WalkTick = " src/M2Server/Actor/AnimalObject.cs
grep -n "SearchTime = 3000" src/M2Server/Monster/MonsterObject.cs
sed -n '1,15p' src/M2Server/Monster/Monsters/ATMonster.cs

# 掉落 7 行
python -c "print(open(r'E:/MirServer/M2GameSvr/Envir/MonItems/稻草人.txt','rb').read().decode('gbk').splitlines())"

# 权限墙 / dir
sed -n '25,45p' src/Modules/GameCommand/GameCommand.cs
grep -n "StartPermission" src/GameSrv/Word/WorldServer.cs src/Modules/SystemModule/Conf/ServerConf.cs
sed -n '16,95p' src/GameSrv/DB/LocalDB.cs
sed -n '1180,1195p' src/M2Server/Player/PlayObject.cs

# Rust 取数序 / 注入点
sed -n '460,548p' mir2-rs/bins/game-svr/src/mongen.rs
sed -n '60,180p' mir2-rs/bins/game-svr/src/guardlist.rs
sed -n '8290,8375p;8530,8570p;8618,8635p;9870,9895p' mir2-rs/bins/game-svr/src/main.rs
grep -n "TICK_INTERVAL_MS\|fn tick\|monster_ai_tick" mir2-rs/crates/world/src/world.rs

# 场景/门
cat mir2-rs/tests/parity/m3/rng-trace-scene/rng_trace_scenario.json
sed -n '250,330p' mir2-rs/tests/parity/m3/rng-trace-scene/rng_scene_gate.py
sed -n '480,510p' mir2-rs/tests/parity/m3/rng-trace-probe/rng_ledger_diff.py

# F1 参照
sed -n '1,10p' mir2-rs/tests/parity/m3/fixtures/rng_stream_seed42_head1828.tsv
grep -rn "999999\|RandomNo" mir2-rs --include=*.rs

# 场景可走证据
cat mir2-rs/tests/parity/m3/evidence-attack-pair-r1-nomonster-20261012/11-mapinfo-and-mapfile-0137.txt
```

### 10.1 全文行号普查（2026-10-17，本轮）

- **总引用数：131**（显式 `文件:行号` / `文件:行号-行号`；`纯 `:N` 续引用` 已在正文按同一文件逐条人工复核，未计入此 131）。
- **实测通过：131**（逐条 `sed -n 'Np'` / `awk 'NR>=a&&NR<=b'` 打出目标行并核对内容与行上语义；
  越界检查：每个 range 的末行 ≤ 文件总行数）。
- **本轮修正：见 `git diff`（相对输入 patch）**；代表性修正：
  `WorldServer.MonGen.cs` `500-503→501-504`、`504-729→507-728`、`742→744`、`748→750`、`149-158→155-159`；
  `guardlist.rs` `90→86`、`146-150→142`、`157-163→164`、`164-168→166-168`；
  `mongen.rs` `492→493`、`497-545→500-533`；`main.rs` `9888/9889`（复核后确认原值正确，见 §9）；
  `LocalDB.cs` `616→619`、`501-587→501-590`、`596-654→596-650`、`129-135→100-144`；
  `GameCommand.cs` `32-35→33-36`；`RecallMobCommand.cs` `15-80→15-72`（文件仅 74 行）；
  `BaseObject.cs` `977-1020→977-1038`；`ExecutionProcessingSys.cs` `5375-5392→5375-5398`、
  `5466-5486→5466-5488`、`3462-3482→3462-3485`；`WorldServer.cs` `1021-1038→1021-1037`、
  `1618-1650→1618-1655`；`MonsterObject.cs` `100-125→100-130`、`251-360→251-358`。
- **两条手工核（不在 `mir2-rs/**` 遍历内）**：`E:/MirServer/Mir200/Envir/MapInfo.txt:437` =
  `[0138 …] FIGHT`；`sql/mir2_data.sql:228` = `(3, '稻草人', 83, …)`。
- 复跑命令（在仓库根；逐条抽出 `文件:行号` → 映射到仓内路径 → 越界机判）：

```sh
python -I - <<'PY'
import re, io, os
DOC = 'mir2-rs/tests/parity/m3/design-rng-scene-pin-path-20261017.md'
M = {
 'WorldServer.MonGen.cs':'src/GameSrv/Word/WorldServer.MonGen.cs',
 'BaseObject.cs':'src/M2Server/Actor/BaseObject.cs',
 'AnimalObject.cs':'src/M2Server/Actor/AnimalObject.cs',
 'MonsterObject.cs':'src/M2Server/Monster/MonsterObject.cs',
 'ATMonster.cs':'src/M2Server/Monster/Monsters/ATMonster.cs',
 'LocalDB.cs':'src/GameSrv/DB/LocalDB.cs',
 'GameCommand.cs':'src/Modules/GameCommand/GameCommand.cs',
 'PlayObject.cs':'src/M2Server/Player/PlayObject.cs',
 'RecallMobCommand.cs':'src/Modules/GameCommand/Commands/RecallMobCommand.cs',
 'ReCallMobExCommand.cs':'src/Modules/GameCommand/Commands/ReCallMobExCommand.cs',
 'MobCommand.cs':'src/Modules/GameCommand/Commands/MobCommand.cs',
 'MobPlaceCommand.cs':'src/Modules/GameCommand/Commands/MobPlaceCommand.cs',
 'MissionCommand.cs':'src/Modules/GameCommand/Commands/MissionCommand.cs',
 'GameApp.cs':'src/GameSrv/GameApp.cs',
 'WorldServer.cs':'src/GameSrv/Word/WorldServer.cs',
 'Messages.cs':'src/OpenMir2/Messages.cs',
 'ExecutionProcessingSys.cs':'src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs',
 'MagicManager.cs':'src/M2Server/Magic/MagicManager.cs',
 'PlayObject.Message.cs':'src/M2Server/Player/PlayObject.Message.cs',
 'PlayObject.Chat.cs':'src/M2Server/Player/PlayObject.Chat.cs',
 'PlayObject.Attack.cs':'src/M2Server/Player/PlayObject.Attack.cs',
 'PlayObject.Base.cs':'src/M2Server/Player/PlayObject.Base.cs',
 'BaseObject.Base.cs':'src/M2Server/Actor/BaseObject.Base.cs',
 'BaseObject.Abil.cs':'src/M2Server/Actor/BaseObject.Abil.cs',
 'HUtil32.cs':'src/OpenMir2/HUtil32.cs',
 'GameSvrConf.cs':'src/Modules/SystemModule/Conf/Model/GameSvrConf.cs',
 'ExecutionCode.cs':'src/Modules/ScriptEngine/Consts/ExecutionCode.cs',
 'RobotPlayer.cs':'src/BotSrv/Player/RobotPlayer.cs',
 'SceneSeqSelfCheck.cs':'src/BotSrv/SceneSeqSelfCheck.cs',
 'Program.cs':'src/BotSrv/Program.cs',
 'MShare.cs':'src/BotSrv/MShare.cs',
 'BeeQueen.cs':'src/M2Server/Monster/Monsters/BeeQueen.cs',
 'BoneKingMonster.cs':'src/M2Server/Monster/Monsters/BoneKingMonster.cs',
 'ScultureKingMonster.cs':'src/M2Server/Monster/Monsters/ScultureKingMonster.cs',
 'SpiderHouseMonster.cs':'src/M2Server/Monster/Monsters/SpiderHouseMonster.cs',
 'WallStructure.cs':'src/M2Server/Monster/Monsters/WallStructure.cs',
 'CharacterObject.cs':'src/M2Server/Player/CharacterObject.cs',
 'main.rs':'mir2-rs/bins/game-svr/src/main.rs',
 'mongen.rs':'mir2-rs/bins/game-svr/src/mongen.rs',
 'guardlist.rs':'mir2-rs/bins/game-svr/src/guardlist.rs',
 'mapwire.rs':'mir2-rs/bins/game-svr/src/mapwire.rs',
 'world.rs':'mir2-rs/crates/world/src/world.rs',
 'entity.rs':'mir2-rs/crates/world/src/entity.rs',
 'monai.rs':'mir2-rs/crates/world/src/monai.rs',
 'rng_ledger_diff.py':'mir2-rs/tests/parity/m3/rng-trace-probe/rng_ledger_diff.py',
 'rng_consumer_groups.py':'mir2-rs/tests/parity/m3/rng-trace-probe/rng_consumer_groups.py',
 'consumer_groups.tsv':'mir2-rs/tests/parity/m3/rng-trace-probe/consumer_groups.tsv',
 'rng_stream_seed42_head1828.tsv':'mir2-rs/tests/parity/m3/fixtures/rng_stream_seed42_head1828.tsv',
 'design-buff-and-tick-20261016.md':'mir2-rs/tests/parity/m3/design-buff-and-tick-20261016.md',
 'whitelist.md':'mir2-rs/tests/parity/whitelist.md',
}
# 显式带目录的引用按末段名映射；两条非仓内文件单独核（见正文手工核）
EXTRA = {'MapInfo.txt':'E:/MirServer/Mir200/Envir/MapInfo.txt',
         'mir2_data.sql':'sql/mir2_data.sql'}
s = io.open(DOC, encoding='utf-8').read()
seen = set(); bad = []; ok = 0
pat = re.compile(r'([A-Za-z0-9_.\-]+\.(?:cs|rs|py|ps1|tsv|json|md|sql|txt)):(\d+)(?:-(\d+))?')
for m in pat.finditer(s):
    name, a, b = m.group(1), int(m.group(2)), (int(m.group(3)) if m.group(3) else None)
    if (name,a,b) in seen: continue
    seen.add((name,a,b))
    rel = M.get(name) or EXTRA.get(name)
    if not rel:
        bad.append((name,a,b,'NO MAPPING')); continue
    fp = rel if os.path.isabs(rel) else rel
    lines = io.open(fp, encoding='utf-8', errors='replace').read().splitlines()
    hi = b or a
    if a < 1 or hi > len(lines):
        bad.append((name,a,b,'OUT OF RANGE %d' % len(lines)))
    else:
        ok += 1
print('distinct=%d ok=%d bad=%d' % (len(seen), ok, len(bad)))
for x in bad: print('BAD', x)
raise SystemExit(1 if bad else 0)
PY
```

  本轮实测：`distinct=131 ok=131 bad=0`（另有正文约 70 处纯 `:N` 续引用已按上下文逐条人工核对，见上）。

---

## 附录 A · 稻草人掉落表（部署件实测，GBK 解码）

```
10/30   金币  300
10/200  金创药(小量)
10/200  魔法药(小量)
10/200  乌木剑
10/200  传统项链
10/200  铁手镯
10/200  古铜戒指
```

`LocalDB.cs:596-650 LoadMonitems` 解析：`SelPoint=n18-1`、`MaxPoint=n1C`、`Count=StrToInt(sData,1)`。
金币行命中时额外 `Random(300)`；非金币行命中时额外 `Random(80)`；
`StdMode>0` 且 `Random(MonRandomAddValue)==0` 时走 `RandomUpgradeItem`（现网 `MonRandomAddValue=40`，`Server.conf:612`；代码缺省 10，`GameSvrConf.cs:1550`）。**条件抽的数量由种子/流位置决定，不做硬编码期望。**

## 附录 B · 关键符号/行号速查（本件已逐条抽验）

| 侧 | 锚点 |
| --- | --- |
| C# | `WorldServer.MonGen.cs:382/385/388/498/501-504/507-728/566-567/730-732/740/744/745/750/439/451/455/467/474` |
| C# | `MobCommand.cs:11/15/40/43`、`MobPlaceCommand.cs:11/37-46`、`MissionCommand.cs:11/30-33`、`ReCallMobExCommand.cs:11/46-56`、`RecallMobCommand.cs:12/43-69` |
| C# | `GameCommand.cs:33-36/37-40`、`LocalDB.cs:16/75-83/100-144`、`WorldServer.cs:80/176-183/1021-1037`、`PlayObject.cs:1047/1188` |
| C# | `GameApp.cs:155/222-250/238/241-244/246`、`WorldServer.MonGen.cs:155-159/192-361/280/315/358` |
| C# | `BaseObject.cs:439/441/730-733/977-1038/1375`、`AnimalObject.cs:76/77/425-441`、`MonsterObject.cs:65/100-130/251-358`、`ATMonster.cs:10` |
| C# | `PlayObject.Message.cs:1394/1524-1561/1677-1678`、`PlayObject.Chat.cs:229/363-368`、`Messages.cs:161/202/318/555/690` |
| C# | `ExecutionProcessingSys.cs:2141-2152/3462-3485/5375-5398/5466-5488`、`MagicManager.cs:319/372-373/1308`、`BaseObject.cs:1179` |
| Rust | `bins/game-svr/src/mongen.rs:201/226-241/247/326/472-551/566/589/627/698/711` |
| Rust | `bins/game-svr/src/guardlist.rs:73/86/129-178` |
| Rust | `crates/world/src/mongen.rs:271-356`、`crates/world/src/entity.rs:254`、`crates/world/src/monai.rs:171/600/757/861`、`crates/world/src/world.rs:33/1122/1313-1314` |
| Rust | `bins/game-svr/src/main.rs:8302/8319/8331/8338-8340/8369/8538/8560/8565/8628/9307/9888/9951/9957/9993` |
| 夹具 | `rng-trace-scene/rng_scene_gate.py`（`fixture_rows`/`stream_inject_seed`）、`rng-trace-probe/rng_ledger_diff.py:480-510`、`rng-trace-probe/rng_consumer_groups.py:57` |
