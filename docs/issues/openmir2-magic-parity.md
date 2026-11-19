# openmir2-magic-parity

> 状态：**进行中（2026-11-19，分支 `card/magic-parity`，base M9=`572a0b8a3`）**。
> Owner 指示：性能线冻结，全力补玩法对齐；魔法是四块（技能/物品/任务/NPC 脚本）里先补的一块，
> 且**必须按 ECS 实现**；对外的覆盖率口径是 `verified/65`（未验证的实现在覆盖率上等于 0）。

## 1. 覆盖率口径与清单

- 分母：`src/M2Server/Magic/MagicManager.cs` 的 `case` 分支数 = **65**（61 个 `MagicConst` case
  + 4 个 `Shape` 子 case；含 `IsWarrSkill` 的 9 个 case）。
- 清单文件：`E:\UserFleet\magic-parity.json`（每 15 分钟由 `E:\OPC\tools\coverage-report.ps1` 读取）。
  首次产出：**verified 0 / implemented 63 / missing 2**。
- `verified` 的门槛：**与原版同输入、帧级对拍通过**（帧逐字节 + RNG 抽取次数与逐次取值全等），
  并在清单里写测试名。没有测试名不许标 `verified`。
- `missing` 2 条：35 `WINDTEBO`（`MagicManager.cs:524-529`）、
  39 `GROUPDEDING`（`MagicManager.cs:549-553`）。
- 单独待办（不混进 implemented）：
  1. `buff_tick_gate` — 14/15/31 的 buff 生产链仅 `World::buff_tick_enabled()` 时生效（缺省关），
     `mir2-rs/bins/game-svr/src/lib.rs:13229`；
  2. `anti_magic_recalc` — 目标 `AntiMagic` 的 `RecalcAbilitys` 面未同步（`lib.rs:12170`）；
  3. `version_gate_167` — `DoSpell` 版本门未接（`MagicManager.cs:167`）。

## 2. 对拍装置（先建装置，再迁移/补分支）

**同输入**：`(magic_id, magic_level, 施法者 MC/SC/DC/HitPoint、目标 AntiMagic/SpeedPoint/LifeAttrib/Race、
双方坐标、地图遮挡、随机源序列、配置项)`。

**C# 侧**：`mir2-rs/tests/parity/csharp/MagicGolden/`（新工程，引用 `src/M2Server/M2Server.csproj`
与 `src/OpenMir2/OpenMir2.csproj`），调用**原版** `MagicManager.DoSpell`，落盘
`frames[]`（出帧顺序与逐字节）与 `rng[]`（每次抽取的值，按调用序）。

**RNG 替换点（不改进生产代码）**：`OpenMir2.RandomNumber` 的**私有静态字段 `random`**
（`src/OpenMir2/RandomNumber.cs:11`）。driver 先 `RandomNumber.GetInstance()` 初始化单例，
再用反射把该字段替换为 `LoggingRandom : Random`（固定种子；覆写 `Next()/Next(int)/Next(int,int)`，
逐次记录到账本）。

**还原方式**：替换是**进程内**的，driver 跑完即退出、进程销毁即还原；若同进程跑多用例，
每例前重新设种子、例后把原 `Random` 实例写回并断言 `GetInstance()` 返回同一单例。
**生产代码零改动**：没有 `#if`、没有测试专用分支、没有第二份 `MagicManager`。

**帧捕获点（下一步要解决的第一个技术问题）**：`DoSpell` 的出帧走
`playObject.SendRefMsg` / `SendSelfDelayMsg`（及 `MagMakeFireCross` 等的 `SendRefMsg`），
需要确认这些函数在**无会话**的离线对象上能否被拦截/记录（候选：`BaseObject` 的发送队列，
或给 `PlayObject` 挂一个只记录不发送的会话桩），确定后写进本节。

**判据**：帧逐字节相等 **且** RNG 抽取次数与逐次取值全等 ⇒ 该分支在清单里标 `verified` 并写测试名。

## 3. 批序（先高频且确定）

1. `1/5 火球`（`MagicManager.cs:174-203`）
2. `11/45 雷电`（`:289-314` / `:563-588`）
3. `2 治愈`（`:204-224`）
4. `13 灵魂火符`（`:315-327`、`:328-351`）
5. `44 灭天火`（`:557-562`）
6. 再处理 buff / AntiMagic 复算相关（依赖第 1 节的三条待办先修）

每验证一条：`magic-parity.json` 对应条目改 `verified` + 写测试名；跑
`powershell -NoProfile -ExecutionPolicy Bypass -File E:\OPC\tools\coverage-report.ps1` 自检。

## 4. ECS 迁移清单（Owner 硬约束：状态组件化 + 行为系统化 + 段序钉死）

**迁移对象（逐条给 `文件:行`，代码现状为准）**

| # | 分支（原版） | Rust 现有入口 | oracle 依据 | 迁移顺序 |
|---|---|---|---|---|
| 1 | 21 瞬息移动 | `crates/world-bevy/src/magic.rs:904` `mag_sace_move`；`crates/world-bevy/src/world.rs:7070` `cast_space_move_spell`；host `bins/game-svr/src/lib.rs:13527` | `MagicManager.cs:403-410`、`:876-891` | 1 |
| 2 | 31 魔法盾 | `crates/world-bevy/src/world.rs:4395` `cast_shield_spell`；host `lib.rs:13306` | `MagicManager.cs:474-478` | 2 |
| 3 | 22 火墙 | `crates/world-bevy/src/world.rs:751` 返回值；`crates/world-bevy/src/magic.rs:188` `power_fire_wall`；host `lib.rs:15300` | `MagicManager.cs:412-413`、`:1064-1098` | 3 |
| 4 | 8/37 推人 | `crates/world-bevy/src/magic.rs:826` `mag_push_arround`；host `lib.rs:13552`、`lib.rs:68031` | `MagicManager.cs:24-52`、`:259-264`、`:601-606` | 4 |
| 5 | 46 分身 | `crates/world-bevy/src/magic.rs:967` `mag_make_clone`；`crates/world-bevy/src/slave.rs:2534` | `MagicManager.cs:589-594`、`:1337-1341` | 5 |
| 6 | 20 驯服 | `crates/world-bevy/src/magic_effect.rs`（`mag_tamming`，经 `magic.rs:35` 再导出） | `MagicManager.cs:394-401`、`:704-825` | 6 |
| 7 | 28 心灵启示 | `crates/world-bevy/src/world.rs:5801` `cast_show_hp_spell` | `MagicManager.cs:431-440` | 7 |

**要求（每个迁移必须同时给）**
1. **状态组件化**：冷却/技能等级/施法目标/吟唱进度做成 Component，走 `Query`/`ResMut`；
2. **行为系统化**：施法判定、MP 扣除、命中/伤害结算、特效广播各自成 system，注册进 `HeadRunner`
   的 `TickStage` 序列（`crates/world-bevy/src/world.rs:1838-1856` 是现有序列）；
3. **段序写死并测试钉住**：卡面写明"在 head 还是 tail、在 AOI 前后"及理由；新增顺序测试；
4. **行为不变**：迁移提交必须附该分支的对拍证据（第 2 节装置）；
5. **确定性**：系统顺序固定 ⇒ RNG 消耗顺序固定。

**禁止**：为快继续加自由函数；绕过 tick 阶段在主循环里插代码。

## 5. 边界

- 不碰 AOI、网关/背压、性能（性能线已按 Owner 指示冻结）；
- 不改 C# 原版逻辑（RNG 替换只在测试进程内，见第 2 节）；
- 每批走 `card/magic-*` 分支 + `--no-ff`，小步提交，贴对拍原文。

## 7. 对拍口径裁定（opc-07，2026-11-19）

- **A（先做）＝语义层对拍**：比较 **RM 消息序列（ident + params + priority）+ RNG 账本 + 状态差**。
  卡面口径写死：**`verified`（`basis=semantic`）= 语义对齐（RM+RNG+状态），不是逐字节**。
- **B（第二阶段，必须有）＝编码层逐字节**：首批 **5 个分支必须做 B**
  （`1/5 火球`、`11/45 雷电`、`2 治愈`、`13 火符`、`44 灭天火`），用共享编码器证明"语义对 ⇒ 字节对"；
  卡面写「B 覆盖样本集 5 条，其余按共享编码器推定」。对外报两个数：
  **`verified(semantic)/65`** 与 **`verified(bytes)/5`**。
- 清单每条 `verified` 必须带 `basis` 字段（`semantic` / `bytes`）。
- 两条线并行排：A 先出数，**不得**因为 A 容易跳过 B，也不得为 B 推迟首个分支。

## 8. 帧捕获点的技术现状（下一步的硬骨头）

- `SendSelfDelayMsg`（`BaseObject.Message.cs:188`）**不需要 Envir**，直接进 `ActorEntity.MsgQueue`
  （`protected readonly PriorityQueue<SendMessage, byte>`，`src/M2Server/Actor/ActorEntity.cs:16`）——
  `DoSpell` 的 `RM_DELAYMAGIC` 这类可被派生 `PlayObject` 直接取出。
- `SendRefMsg`（`:383`）**需要 `Envir != null`**，且要 `Envir.CellMatch/GetCellInfo` + `SystemShare.ActorMgr`
  才能把消息投递给范围内 actor（施法者自身也在其格内 ⇒ `RM_SPELL`/`RM_MAGICFIRE` 会落进**施法者自己的队列**）。
- 因此 A 路径需要：真实 `Envirnoment`（有参数构造 + `LoadMapData`）或最小 `IEnvirnoment` 实现
  （按格返回含施法者的 `ObjList`）+ 把施法者登记进 `SystemShare.ActorMgr`；二者都不改生产代码。
  选定并跑通后，`1/5 火球` 的 RM+RNG+状态三项即可落第一个 `verified(semantic)`。

## 6. 装置进展（2026-11-19）

- **骨架已建**：`mir2-rs/tests/parity/csharp/MagicGolden/`（net8.0，只读引用 `M2Server`/`OpenMir2`）。
- **RNG 接缝已跑通**（`--rng-selftest`，rc=0）：
  ```
  seam=OpenMir2.RandomNumber.random  seed=12345
  seq   = [6,7,77,51,79,82,16,73]
  draws = [6,7,77,51,79,82,16,73]        （账本与业务取值逐次相等）
  same_seed_same_seq=true  diff_seed_diff_seq=true  restored=true  ledger_matches=true
  ```
  自检四项：①反射找到私有静态字段并注入；②同种子两次安装序列全等；③不同种子序列不同（证明真的接上、
  不是恒等）；④`Restore()` 后字段回到原实例。
- **下一步（装置）**：确定 `DoSpell` 的帧捕获点——出帧走 `BaseObject.SendRefMsg` /
  `SendSelfDelayMsg`（`src/M2Server/Actor/BaseObject.Message.cs:383` / `:188`），需要一个离线可记录的
  发送面（候选：`MsgQueue`（`SendMsg` 入队点）或最小 `Envir` 桩）；确定后先打通 `1/5 火球`。

## 9. 切片进展（2026-11-19 续）

- **Slice 1（配置面）已完成**：`--config-selftest` 把冻结部署的 5 个 conf 拷进输出目录后读
  `SystemShare.Config`：`magic_attack_rage=8 send_ref_msg_range=12 fireboom_rage=1`，
  `CONFIG_SEAM_SELFTEST OK`。（GB2312 需先 `Encoding.RegisterProvider(CodePagesEncodingProvider)`，
  与 `ConfigFile.Load` 的 `GetEncoding("GB2312")` 对应；`LogService.Logger` 需给无 sink 实例，
  否则错误路径会以 NRE 掩盖真实原因。）
- **C# 侧火球用例已跑通**（`--case fireball`，真实地图 `Map/0.map` + 真实 `Envirnoment` +
  `SystemShare.ActorMgr` 登记 + `RecordingPlayObject` 抽 `MsgQueue`）：
  ```
  spell_return=true
  rm=[{ident=10008(RM_SPELL),wParam=1,p1=101,p2=100,p3=1,priority=24},
      {ident=10120(RM_MAGICFIRE),wParam=0,p1=257,p2=6553701,p3=0,priority=136}]
  rng=[0]
  ```
  该种子上 `Random(10)=0 < AntiMagic=3` 不满足命中门 ⇒ 走「未命中」支（`targetObject=null`，
  `RM_MAGICFIRE` 的 p3=0、无 `RM_DELAYMAGIC`）。命中支需另取种子（`Random(10) >= AntiMagic`）。
- **Rust 侧驱动（待做）**：在 `bins/game-svr` 的测试路径上用 `handle_spell_cmd_with_magic`
  产出同结构的 RM/RNG 账本，再写比较器（帧逐项 + RNG 逐次），两条都过才标
  `verified(semantic)`。

## 10. 火球三支 C# 侧账本（2026-11-19）

`--case fireball --out mir2-rs/tests/parity/golden/magic/fireball.json`（种子由扫描确定并写入产物）：

| 支 | 种子 | 首抽 `Random(10)` | RNG 账本 | RM 账本（ident, p3） |
|---|---|---|---|---|
| miss | 1 | 2（< AntiMagic=3） | `[2]` | `(RM_SPELL 10008, p3=1)`、`(RM_MAGICFIRE 10120, p3=0)` |
| edge | 5 | 3（== AntiMagic） | `[3,0,0,0,4]` | 10008(p3=1)、10120(p3=1002)、8021(p3=1002) |
| hit | 13 | 5（> AntiMagic） | `[5,0,0,0,0]` | 10008(p3=1)、10120(p3=1002)、8021(p3=1002) |

（8021 = `RM_DELAYMAGIC`；命中支的额外抽取来自 `GetAttackPower` 与训练尾 `Random(3)`。）

**洁癖区（不记下来下次必踩）**
1. `ConfigFile.Load` 用 `Encoding.GetEncoding("GB2312")` ⇒ 进程必须先
   `Encoding.RegisterProvider(CodePagesEncodingProvider.Instance)`，否则 `SystemShare` 静态构造即抛。
2. `LogService.Logger` 是 `public static ILogger`（`src/OpenMir2/LogService.cs:9`）⇒ 离线进程要给一个
   无 sink 的实例，否则错误路径（如地图缺失）以 NRE 掩盖真实原因。
3. `SendRefMsg` 的 `VisibleHumanList` 有 **500ms 刷新门**（`BaseObject.Message.cs:395`）：同一 tick
   内第二次调用会跳过重扫、可能少发。装置侧要求：每个用例用**全新对象**，或显式清
   `VisibleHumanList`；并加"两次调用不丢消息"的自检断言。

## 11. A 路径的 RM↔SM 映射口径（Rust 侧驱动前必须定死）

原版 `DoSpell` 产出的是 **RM_ 消息**（进 `ActorEntity.MsgQueue`，由会话编码层再编码成 SM_ 字节）；
Rust 宿主**没有 RM 层**，直接产出 SM_ 帧。因此语义层对拍必须用一张**显式映射表**（写死、可审）：

| 原版 RM（本卡三支出现过的） | 产生 SM 帧的 C# 位置（**硬要求：没有行号不算映射**） | Rust 侧对应物 / 比较方式 |
|---|---|---|
| `RM_SPELL(10008)` wParam=Effect, p1/p2=坐标, p3=MagicId, prio=24 | `src/M2Server/Player/PlayObject.Message.cs:1454-1459`（`MakeMessage(SM_SPELL, …, nParam1, nParam2, wParam)`，p3 走尾串） | `SM_SPELL` 帧：Effect/坐标/MagicId 逐项。**注意自守卫**：`:1456` 的 `processMsg.ActorId != ActorId` ⇒ **施法者自己不会收到 SM_SPELL**（只发给范围内其他玩家），RM 账本里出现 ≠ 该玩家收到客户端帧 |
| `RM_MAGICFIRE(10120)` wParam=0, p1=MakeWord(EffectType,Effect), p2=MakeLong(x,y), p3=目标ActorId, prio=136 | `src/M2Server/Player/PlayObject.Message.cs:1788-1794`（`SM_MAGICFIRE`，`LoWord/HiWord(nParam2)` + `nParam1`，`nParam3` 经 EDCode 作尾串） | `SM_MAGICFIRE` 帧：p3=0 表示无目标 |
| `RM_DELAYMAGIC(8021)` wParam=伤害, p1=坐标, p2=nRage, p3=目标, prio=255 | **服务端事件，不是客户端帧**：`src/M2Server/Actor/BaseObject.Base.cs:711-728`（`GetMagStruckDamage` → 动物 race 伤害 `/1.2` → 范围内则发 `RM_MAGSTRUCK`） | 比较"是否登记延时事件 + 伤害 + nRage + 目标"；不产 SM 帧 |
| `RM_MAGIC_LVEXP`（训练尾） | `src/M2Server/Player/PlayObject.Message.cs:1800-1801`（`SM_MAGIC_LVEXP`，p1/p2 + `LoWord/HiWord(nParam3)`） | `SM_MAGIC_LVEXP` 帧：字段逐项 |

RNG 账本逐次比较用 `crates/shared/src/rng.rs` 的 `DotNetRandom`/`RandomNumber`（与原版同算法）。
**未登记在该表里的 RM 条目一律判"口径未定义"（不标 verified）**，避免拿"没比到"当"比过了"。

## 12. C# 侧两颗负向牙已过（2026-11-19，编码层真实 SM 帧）

装置改为**真实 SM 帧**：`RecordingPlayObject` 覆写 `PlayObject.Socket.cs:101` 的 **virtual**
`SendSocket(CommandMessage,string)`，并用 `Operate(ProcessMessage)`（`PlayObject.Message.cs:768`）
走生产编码路径处理各接收者队列 ⇒ 记录的是**过完自守卫之后真正要发出去的帧**。

场景：施法者(100,100)、目标(101,100)、观察者-范围内(105,100)、观察者-范围外(140,100)。

| 支 | caster SM | target SM | obs_in SM | obs_out |
|---|---|---|---|---|
| miss | `638`(SM_MAGICFIRE) | `17`(SM_SPELL)+`638` | `17`+`638` | 无 |
| edge | `638` | `17`+`638` | `17`+`638` | 无 |
| hit | `638` | `17`+`638` | `17`+`638` | 无 |

- **牙①（负向）**：三支里施法者收到的 `SM_SPELL(17)` **恒为 0**（只有 `SM_MAGICFIRE 638`）⇒
  `PlayObject.Message.cs:1456` 的自守卫被钉住。
- **牙②（范围）**：范围内观察者与目标各收 2 帧，范围外观察者 **0 帧** ⇒ 范围判定被钉住，
  不是"发了一份"。
- 金标准：`mir2-rs/tests/parity/golden/magic/fireball.json`（含每个接收者的 RM 与 SM 账本）。

**洁癖区补充（新增三条，均来自本步踩坑）**
4. `PlayObject` 构造后 `FixedHideMode` 可能为 true ⇒ 装置必须显式置 `false`（否则 `SendRefMsg`
   走"只发自己"分支，观察者永远收不到，且 `VisibleHumanList` 恒空）。
5. `SystemShare.ActorMgr` 是**进程级**注册表且不会覆盖同 id ⇒ 同一进程跑多用例必须用**不同
   ActorId**，否则后面的用例会发给前面用例的旧对象（表现为"后续用例全 0 帧"）。
6. `SendRefMsg` 的 500ms 门（`BaseObject.Message.cs:395`）已确认：首次调用扫描并填充
   `VisibleHumanList`，其后 500ms 内走缓存列表（`<11` 判距）。

## 13. 三支差异清单（跨 tick 采集后；2026-11-19）

采集方式：`DoSpell` → 等 700ms（`RM_DELAYMAGIC` 到期）→ 两趟排水 + 生产 `Operate`。
目标 HP 初值 100（不给 HP 时账本为 0→0，不可观测——已入洁癖区）。

| 字段 | miss | edge | hit |
|---|---|---|---|
| RNG 账本 | `[2]` | `[3,0,0,0,4]` | `[5,0,0,0,0]` |
| 首抽 `Random(10)` | 2 (<3) | 3 (==3) | 5 (>3) |
| `RM_DELAYMAGIC(8021)` | **无** | 有（p3=目标） | 有 |
| `RM_MAGSTRUCK(8044)` | **无** | 有（target） | 有 |
| `RM_STRUCK_MAG(8103)` | **无** | 有（caster/target/obs_in） | 有 |
| `SM_STRUCK(31)` | **无** | 有：`param=85`（新 HP）、`series=15`（伤害） | 同 |
| target HP | **100 → 100** | **100 → 85** | 100 → 85 |
| caster `SM_SPELL(17)` | 0 | 0 | 0 |
| obs_out 帧 | 无 | 无 | 无 |

- **miss 与 edge/hit 的差异是实质性的**（有无伤害链 + HP 差 + 有无 8021/8044/8103/31）。
- **edge 与 hit 行为相同**（`Random(10) == AntiMagic` 与 `> AntiMagic` 走同一支，这是原版语义），
  差异只在 RNG 账本的首抽值——卡面如实写明，不把它当作"两个独立行为"。
- 伤害链还产生 `RM_HEALTHSPELLCHANGED(10052)`、`RM_CHANGENAMECOLOR(8090)`（以及存在死亡时的
  `RM_DEATH(10021)`），均已在映射表登记（下表）。
- **未到期事件**：三支 `not_due` 均为 0（700ms 等待覆盖 600ms 延时）；若将来出现未到期条目，
  必须单列为"未覆盖"，不得计入已比。

### 映射表补充（伤害链，均带 C# 产生位置）

| RM | 产生 SM 帧的 C# 位置 |
|---|---|
| `RM_MAGSTRUCK(8044)` / `RM_STRUCK_MAG(8103)` | `PlayObject.Message.cs:1562-1563`（→ `SM_STRUCK(31)`） |
| `RM_HEALTHSPELLCHANGED(10052)` | `PlayObject.Message.cs:1736`（→ `SM_HEALTHSPELLCHANGED`） |
| `RM_CHANGENAMECOLOR(8090)` | `PlayObject.Message.cs:1677-1678`（→ `SM_CHANGENAMECOLOR`，内部读 `GetChrColor`） |
| `RM_DEATH(10021)` | `PlayObject.Message.cs:1630`（本三支未出现） |

**洁癖区补充**
7. 帧编码会读 `SystemShare.CastleMgr.InCastleWarArea` ⇒ 离线进程必须 `new CastleManager()` 赋给
   `SystemShare.CastleMgr`（空列表 ⇒ 恒 null），否则 `GetChrColor` NRE。
8. 观测 HP 变化前必须给目标 `WAbil.HP` 初值（默认 0 时伤害差不可见）。
