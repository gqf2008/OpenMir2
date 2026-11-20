# 需新观测面的三条 magic — 方案与成本（MG-34 / MG-36 / MG-55）

> 状态：**规划（只读调研，2026-11-21）**；实现前需 `opc-19` 审。
> 装置基线：`mir2-rs/tests/parity/csharp/MagicGolden/Program.cs`（master `b480336f9`，2688 行；
> **该文件当前有两条线在改**——施毒批与 MG-18；本文给出**最小增量**以免扩大冲突面）。
> 所有 oracle 锚已独立复核（不是转述）。**本文不改任何代码。**

## 0. 装置现状（三条共用的缺口）

- **派发**：`Program.cs:228` 对未知用例输出 `未知用例: <name>` 并 rc=2；**20/22/46 均无 batch 入口**。
- **单支驱动**：`RunMagicCase`（`Program.cs:1760`）构 caster/target/obsIn/obsOut，`DoSpell` 直调在
  `Program.cs:762`；怪物走 `RecordingMonster : MonsterObject`（`Program.cs:2587`）+ `MonsterSpec`
  （`Program.cs:2507`，字段仅 `X/Y/Race/Level/Hp/LifeAttrib/Master/TargetCret/SpeedPoint/Death/AC/AntiMagic`）。
- **Golden 形状**：Fixture 有按魔号门控的可选块，例如 `Skeleton = spec.MagicId != 17 ? null : {…}`
  （`Program.cs:2438`）⇒ **非 17 的魔号该块恒 `null`**；新增观测面必须加**各自的门控块**并同步 Rust 比较器
  （比较器按 `magic_id` 取块；未知块被忽略，不会误读）。
- **排水**：`MaxDrainPasses = 8`（`Program.cs:788/804`；步进排水同义 `:2019/2078`），命中上界记
  `DrainPassesHitCap`（`Program.cs:922/2414`）。**新 batch 若引入多跳投递，必须自查 `cap=false`**，
  否则 golden 自带"账本不完整"标记。

---

## 1. MG-34（魔号 20，诱惑之光 `MagTamming`）

### 1.1 oracle 行为（已核）
- 派发：`src/M2Server/Magic/MagicManager.cs:396-404`（`case MagicConst.SKILL_TAMMING`）：
  门 = `playObject.IsProperTarget(targetObject) && !targetObject.Animal`（**不是动物**），然后调
  `MagTamming(playObject, (MonsterObject)targetObject, nTargetX, nTargetY, userMagic.Level)`。
- 函数：`MagicManager.cs:704-819`。关键链（按代码序）：
  1. 外层 `Random(4 - magicLevel) == 0`（`:707`）；
  2. `targetObject.TargetCret = null`；
  3. **`Master == playObject` 支**：`SendMsg(RM_MAKEHOLYSEIZEMODE, (level*5+10)*1000, …)`（`:712`）→ `result=true`；
  4. 否则：`Random(2)==0`（`:716`）→ `Abil.Level <= caster.Level+2`（`:718`）→ `Random(3)==0`（`:720`）→
     `Random(caster.Level + 20 + magicLevel*5) > target.Level + MagTammingTargetLevel`（`:723`）→
     收服链：`!NoTame && LifeAttrib != LA_UNDEAD && target.Level < MagTammingLevel &&
     SlaveList.Count < MagTammingCount`（`:725`）→ `n14 = target.MaxHP / MagTammingHPRate`（`:727`，≤2 取 2，否则翻倍）
     → `Random(n14)==0`（`:735`）→ **收服**：`BreakCrazyMode`、Master 原有则 `WAbil.HP /= 10`、
     `CanReAlive`/`MonGen.ActiveCount--`、`Master=playObject`、`MasterRoyaltyTick=…Random(Level*2)…`、
     `SlaveMakeLevel`、`MasterTick`、`BreakHolySeizeMode`、`WalkSpeed/NextHitTime` 钳制、`RefShowName`、
     `SlaveList.Add`；失败支 `Random(14)==0 ⇒ WAbil.HP=0`（`:783`）；
     不死系支 `Random(20)==0 ⇒ HP=0`（`:788`）；暴走/变红支 `OpenCrazyMode(Random(20)+10)`（`:795/803`）；
  5. **`else` 支**（`Random(2)!=0`）：`SendMsg(RM_MAKEHOLYSEIZEMODE, …)`（`:807`）；外层失败支
     `Random(2)==0 ⇒ result=true`（`:812`）。
- **RNG 面**：上述多处 `Random` 顺序**全部**是可对拍量（`RecordingRandom` 已能记录）。
- **配置面**：`MagTammingLevel / MagTammingCount / MagTammingTargetLevel / MagTammingHPRate`
  （`GameSvrConf.cs`，`SystemShare.Config`）。

### 1.2 现有装置缺什么（逐条指到现有代码）
1. **没有 batch 入口**：`Program.cs:228` 直接落"未知用例"。
2. **怪物夹具字段不够**：`MonsterSpec`（`Program.cs:2507-2535`）缺 `NoTame / MonGen / CanReAlive /
   WalkSpeed / NextHitTime / WAbil.HP 读取面 / MasterRoyaltyTick / SlaveMakeLevel`；
   `RecordingMonster`（`Program.cs:2587`）也没有**逐怪物术后快照**。
3. **收服结果没有记录面**：现有 `Skeleton` 块只记 `slave_count_before/after` 与 `regen_calls`
   （`Program.cs:2438` 起），没有"目标怪物 Master 指向谁 / WalkSpeed / 下一次受击 / MonGen 计数"。
4. **`RM_MAKEHOLYSEIZEMODE` 观测**：装置按收件人记 RM（`DrainRaw`），但**没有该 ident 的命名断言面**
   （Skeleton 验的是 adopt 后的状态与 640/RM10123，不是这条）。
5. **`IsProperTarget`/`Animal` 前提**：`MonsterSpec` 无 `Animal` 旋钮；RecordingMonster 的默认值未写进 spec。

### 1.3 最小增量 + 风险 + 工作量
- **增量**（只加新分支，不动既有 batch）：
  1. `MagicSpec` 加 `Tamming*` 配置字段（或直接写 `SystemShare.Config.*`，参考现有 `spec.PreSlaveCount` 写法）；
  2. `MonsterSpec` 加 `NoTame / MonGenActive / CanReAlive / Animal / WalkSpeed / NextHitTime`；
  3. 新 `TammingBatch(spec)`：三支——`self_master`（Master=施法者 ⇒ RM + 不抽）、`adopt_ok`、`fail/暴走`；
     每支断言 RNG 序列 ≥ 已知抽数（用 `RecordingRandom` 账本，不断言"恰好 N"以避免脆弱）；
  4. Fixture 加 `tamming = spec.MagicId != 20 ? null : { monsters_after:[{actor, master, walk_speed,
     next_hit, mongen_active, hp}], slave_list, rm_makeholyseize:[{who, w_param}] }`；
  5. Rust 比较器对应块 + 负例牙：把 `Random(4-lvl)` 改成恒真 ⇒ RNG 账本与 adopt 结果红。
- **风险**：① `MonGen.ActiveCount--` 需要可构造的 MonGen（离线替身目前只实现 `RegenMonsterByName`）；
  ② `RefShowName`/`OpenCrazyMode` 的下游帧若进入队列，可能推高 `DrainPasses`（须断言 `cap=false`）；
  ③ 三支需要**互异 seed** 才能覆盖收服/暴走/不死系（沿用现有 `seed` 参数，不新造随机）。
- **工作量：中偏大**（新 batch + 两个 spec 扩展 + 新 golden 块 + Rust 比较器；约 200–300 行）。

---

## 2. MG-36（魔号 22，火墙 `MagMakeFireCross`）

### 2.1 oracle 行为（已核）
- 派发：`MagicManager.cs:412-417`（`case MagicConst.SKILL_EARTHFIRE`）；参数：
  `nDamage = playObject.GetAttackPower(GetPower(userMagic, MPow(userMagic)) + Lo(WAbil.MC),
  Hi(MC) - Lo(MC) + 1)`（**内部有 RNG**）、`time = (ushort)(GetPower(userMagic, 10) + (GetRPow(MC) >> 1))`。
- 函数：`MagicManager.cs:1064-1102`：
  - **安全区门**：`Config.DisableInSafeZoneFireCross && playObject.InSafeZone(Envir, nX, nY)` ⇒ 红字并 `return false`（`:1066-1071`）；
  - 五个格子 **N(nX,nY−1)、W(nX−1,nY)、中心、E(nX+1,nY)、S(nX,nY+1)** 各自：
    `Envir.GetEvent(x,y) == null` ⇒ `new FireBurnEvent(playObject, x, y, ET_FIRE, time*1000, nDamage)`
    并 `SystemShare.EventMgr.AddEvent(...)`（`:1072-1096`）；
  - **逐 tick 伤害不在此函数**：`FireBurnEvent.Run()`（`src/Modules/SystemModule/MagicEvent/Events/FireBurnEvent.cs:27-47`）
    每 `>3000ms` 对格内合法目标 `SendMsg(RM_MAGSTRUCK_MINE, Damage)`。
- **可观测面（本卡建议）**：事件表的 5 条登记（格/类型/持续/伤害）与安全区分支；**不含**逐 3000ms 伤害。

### 2.2 现有装置缺什么（逐条指到现有代码）
1. **没有 batch 入口**：`Program.cs:228`。
2. **事件表没有被读**：装置只初始化空 `EventManager`（`Program.cs:108-112`，为 16 的 `MagMakeHolyCurtain`），
   **从未读取 `EventMgr.Events`**（`src/M2Server/Event/EventManager.cs` 的 `Events` 是 public IList）。
3. **没有安全区配置/地图输入**：`MagicSpec` 无 `DisableInSafeZoneFireCross` 旋钮；`Envirnoment` 的安全区
   判定依据（地图 Flag `SAFE`）在夹具里未设置（`RunMagicCase` 只 `LoadMapData`）。
4. **`GetEvent` 去重支不可造**：需要**预置一个 MapEvent** 在某格（现有装置无任何预置事件 API）。
5. **RNG 面**：`GetAttackPower` 的抽取已入账本（`RecordingRandom`），无需新增机制。

### 2.3 最小增量 + 风险 + 工作量
- **增量**：
  1. 新 `FireCrossBatch(spec)`：`spec` 加 `FireCrossSafeZone`（bool）与 `PreEvents`（格列表）；
  2. 施法后读 `SystemShare.EventMgr.Events`，按 `(nX,nY,EventType,Damage,ContinueTime)` 记录；
     `ContinueTime` 为 `MapEvent` 私有字段 ⇒ **反射**（装置已大量用反射，如 `Skeleton` 的 stub 字段）；
     同时记 `CloseTick-OpenStartTick` 作交叉核（`OpenStartTick` 为 protected ⇒ 反射）；
  3. 安全区分支：用 `spec.FireCrossSafeZone` 打开 `Config.DisableInSafeZoneFireCross`，并把地图 Flag 设 `SAFE`
     （或 `InSafeZone` 可注入点——需先确认 `Envirnoment` 的最小设置面）；
  4. Fixture 加 `firecross = spec.MagicId != 22 ? null : { events:[{x,y,type,damage,continue_ms}],
     safe_zone_reject: bool, red_sysmsg: bool }`；
  5. Rust 比较器 + 牙：删安全区门 ⇒ `safe_zone_reject` 红；把第 5 格去掉 ⇒ `events.len` 红。
- **风险**：① `InSafeZone` 的实现依赖 `Envir.Flag`（`SAFE`）与坐标；离线地图需能设 Flag（确认 `OfflineMapSystem` 是否能改 Flag，否则要加最小设置方法）；② 预置事件的 `Id` 由 `MapEvent` 构造分配——比较时排除 `Id`；③ `DisableInSafeZoneFireCross` 是**进程级配置**，必须在用例后还原（现有 spec 字段模式已如此）。
- **工作量：中**（新 batch + 事件表反射记录 + 安全区夹具；约 150–200 行）。
- **明确不在本卡**：逐 3000ms 伤害（需要事件 tick 模拟与等待语义；与 `DrainPasses`/墙钟交互复杂，另立卡）。

---

## 3. MG-55（魔号 46，分身术 `MagMakeClone`）

### 3.1 oracle 行为（已核）
- 派发：`MagicManager.cs:589-594`（`case MagicConst.SKILL_46`）⇒ `MagMakeClone(playObject, userMagic)`。
- 函数：`MagicManager.cs:1337-1341` —— **只有一行**：`new PlayCloneObject(playObject); return true;`。
- 克隆体：`src/M2Server/Player/PlayCloneObject.cs`：
  - ctor：`ChrName="Clone"`、`CurrX/CurrY = 施法者`、`Dir = GetBackDir(施法者.Dir)`（`BaseObject.cs:820`）、
    共享 `Envir`、`Gender/Hair` 复制、`Envir.AddMapObject(x,y,CellType.Play,ActorId,this)`，
    然后 **`SendRefMsg(RM_TURN, Dir, CurrX, CurrY, 0, "Clone")`**；
  - **`SM_FEATURECHANGED(41)` 不在 ctor 里**——它来自 recipient 侧 `RM_TURN` handler：
    `src/M2Server/Player/PlayObject.Message.cs:1524-1558`，当 `RM_TURN` 且
    `processMsg.ActorId != ActorId`（收件人≠发件人）时，除 `SM_TURN(10)` 外还发
    **`SM_FEATURECHANGED(clone.ActorId, LoWord(GetFeatureToLong()), HiWord(...), GetFeatureEx())`**（`:1555-1558`）。
    ⇒ 观察者（含同格施法者）收 `10` + `41`；克隆体自己没有客户端、不处理。
- **可观测面**：克隆体对象（id/name/pos/dir）+ 每个收件人的 `SM_TURN` 与 `SM_FEATURECHANGED` 载荷。

### 3.2 现有装置缺什么（逐条指到现有代码）
1. **没有 batch 入口**：`Program.cs:228`。
2. **没有克隆体对象记录**：`RunMagicCase`（`Program.cs:1760`）只建 caster/target/obsIn/obsOut；
   `RecordingMonster/RecordingPlayObject`（`Program.cs:2587/2597`）不截获"运行期新建对象"。
3. **没有克隆体字段白名单**：`Fixture` 无 `clone` 块；Rust 比较器也无对应面。
4. **ActorId 不确定性**：`ActorMgr.GetNextIdentity()` 全局递增 ⇒ golden 需**归一化**（记录相对 id 或按 ChrName 定位），
   否则每次生成都漂。
5. **收件人集合可复用**：`SendRefMsg` 的 ±range 扫描与 `RecordingPlayObject.DrainRaw` 已具备；
   `SM_TURN`/`SM_FEATURECHANGED` 映射由真身 handler 产生，不需要新映射。

### 3.3 最小增量 + 风险 + 工作量
- **增量**：
  1. 新 `CloneBatch(spec)`：施法后扫 `env` 图对象找 `ChrName=="Clone"`（或 `ActorMgr` 反射取最后创建者），
     记录 `{pos, dir, gender, hair, actor_rel_id}`；
  2. 各收件人记 `SM_TURN`（recog=clone id、param/tag=x/y、series=dir）与
     `SM_FEATURECHANGED`（recog=clone id、param/tag=Lo/Hi(feature)、series=featureEx）；
  3. Fixture 加 `clone = spec.MagicId != 46 ? null : { field:[…], receivers:[{who, turn:{…}, feature:{…}}] }`；
  4. Rust 比较器 + 牙：删 ctor 的 `SendRefMsg(RM_TURN)` ⇒ 两帧全缺红；把 `GetBackDir` 改成同向 ⇒ `dir` 红。
- **风险**：① ActorId 归一化必须做（按 ChrName/相对 id），否则 golden 天然漂；② 克隆体生命周期（无 tick 清理）
  在离线进程不会触发 `Closed`，OK；③ `GetFeatureToLong` 依赖 Gender/Hair 复制——需断言夹具输入
  Gender/Hair 非零，否则 feature 面退化（低字节相同）。
- **工作量：小**（新 batch + 扫图对象 + 两块记录；约 100 行；无需新基础设施）。
- **注**：此条是三条里**唯一不需要配置/事件系统**的，优先做。

---

## 4. 排序与建议

| 魔号 | 条目 | 工作量 | 关键新面 | 建议 |
| --- | --- | --- | --- | --- |
| 46 | MG-55 分身术 | **小** | 克隆体字段 + 收件人 10/41 | **先做**（无新基础设施） |
| 22 | MG-36 火墙 | **中** | 事件表 5 格（反射）+ 安全区 | 其次（需 Flag/反射面） |
| 20 | MG-34 诱惑之光 | **中偏大** | 收服状态 + 多抽 RNG + MonGen | 最后（怪物夹具扩展最大） |

**跨条注意事项**：
- 三条都必须加**各自的 `spec.MagicId != N ? null` 门控块**（仿 `Program.cs:2438`）并同步 Rust 比较器；
- 三条都要在 golden 里记 `DrainPasses/HitCap` 并断言 `cap=false`（`Program.cs:922/2414` 已有字段）；
- 装置文件当前有两条线在改：**新代码全部以"新函数 + 一行 dispatch + 新 spec 字段"加入**，不重排既有 batch；
- 若实现时发现需要动 `RunMagicCase` 主流程（例如造安全区/预置事件），**先与在改该文件的两条线协调**。
