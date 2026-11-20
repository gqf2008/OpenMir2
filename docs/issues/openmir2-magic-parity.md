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
- **覆盖率计数口径**：`verified` 数以 **master 上 `docs/issues/parity-gap-magic.json` 为准**；
  分支侧新增的 `verified` 在合入 master 前**不计入**对外覆盖率（`E:/OPC/tools/coverage-report.ps1`
  读的是主检出 `docs/issues`）。分支数只能作为进展证据，不能当对外数字。
- `missing` 2 条：35 `WINDTEBO`（`src/M2Server/Magic/MagicManager.cs:524-529`）、
  39 `GROUPDEDING`（`src/M2Server/Magic/MagicManager.cs:549-553`）。
- 单独待办（不混进 implemented）：
  1. `buff_tick_gate` — 14/15/31 的 buff 生产链仅 `World::buff_tick_enabled()` 时生效（缺省关），
     `mir2-rs/bins/game-svr/src/lib.rs` 的 `handle_spell_cmd_with_magic`（14/15/31 buff 生产链接线）。
     **现状（2026-11-20）**：生产 `sim_loop` 已 `enable_buff_tick()`；对拍夹具必须**显式开门**，
     并以"关门 ⇒ 逐字节不变"作**负向断言**（门本身的牙）。
  2. `anti_magic_recalc` — 目标 `AntiMagic` 的 `RecalcAbilitys` 面未同步（`lib.rs` 的 `spell_target_anti_magic` 实体同步面）。
     **登记位置（2026-11-20 核对）**：本条 + `spell_target_anti_magic` 的文档注释
     （"复活 `BaseObject.cs:2739`、怪物 `MakeClone`/`GainSlaveExp`、装备/卸下后的实体同步尚未建模"）；
     白名单**无独立 T- 行**（T-18 是 `recalc_abilitys_with_prior` 的**装备路径 prior-SubAbility 语义**，
     不覆盖目标侧同步）。涉及路径：复活、克隆、换装/卸装后的目标 `anti_magic` 同步。
  3. ~~`version_gate_167`~~ **已收口（2026-11-20）**：`DoSpell` 版本门**已实现**——
     `handle_spell_cmd_with_magic` 内、无条件 `SM_SPELL` 广播之后的 fail-closed 块（门在 `:159` 之后；
     失败 ⇒ 无效果体/无尾/零抽）；牙见 `spell_frame_unmodeled_magic_broadcasts_exactly_one`
     （`MagicId > 80` 子类）与 `spell_tail_p1_excluded_magic_ids_unchanged`。原"未接"说法作废。

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

1. `1/5 火球`（`src/M2Server/Magic/MagicManager.cs:174-203`）
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
| 1 | 21 瞬息移动 | `crates/world-bevy/src/magic.rs` 的 `mag_sace_move`；`crates/world-bevy/src/world.rs` 的 `cast_space_move_spell`；host `bins/game-svr/src/lib.rs` 的 `cast_space_move_spell_with` 调用点 | `src/M2Server/Magic/MagicManager.cs:403-410`、`:876-891` | 1 |
| 2 | 31 魔法盾 | `crates/world-bevy/src/world.rs` 的 `cast_shield_spell`；host `lib.rs` 的 `cast_shield_spell` 调用点 | `src/M2Server/Magic/MagicManager.cs:474-478` | 2 |
| 3 | 22 火墙 | `crates/world-bevy/src/world.rs` 的 `FireWallOutcome` 返回值；`crates/world-bevy/src/magic.rs` 的 `power_fire_wall`；host `lib.rs` 的 `handle_spell_cmd_with_magic`（22 支） | `src/M2Server/Magic/MagicManager.cs:412-413`、`:1064-1098` | 3 |
| 4 | 8/37 推人 | `crates/world-bevy/src/magic.rs` 的 `mag_push_arround`；host `lib.rs` 的 `cast_firewind_spell` 调用点、测试 `spell_cmd_firewind_enters_effect_draws_and_pushes` | `src/M2Server/Magic/MagicManager.cs:24-52`、`:259-264`、`:601-606` | 4 |
| 5 | 46 分身 | `crates/world-bevy/src/magic.rs` 的 `mag_make_clone`；`crates/world-bevy/src/slave.rs` 的 `spawn_clone` | `src/M2Server/Magic/MagicManager.cs:589-594`、`:1337-1341` | 5 |
| 6 | 20 驯服 | `crates/world-bevy/src/magic_effect.rs`（`mag_tamming`，经 `magic.rs` 再导出） | `src/M2Server/Magic/MagicManager.cs:394-401`、`:704-825` | 6 |
| 7 | 28 心灵启示 | `crates/world-bevy/src/world.rs` 的 `cast_show_hp_spell` | `src/M2Server/Magic/MagicManager.cs:431-440` | 7 |

**要求（每个迁移必须同时给）**
1. **状态组件化**：冷却/技能等级/施法目标/吟唱进度做成 Component，走 `Query`/`ResMut`；
2. **行为系统化**：施法判定、MP 扣除、命中/伤害结算、特效广播各自成 system，注册进 `HeadRunner`
   的 `TickStage` 序列（`crates/world-bevy/src/world.rs` 的 `HeadRunner::new` 注册序列）；
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

`--case fireball --out mir2-rs/tests/parity/golden/magic/fireball.json`（种子由扫描确定并写入产物）。

**数字以太唯一事实源为准：`mir2-rs/tests/parity/golden/magic/fireball.json`**——卡面不手抄具体
`p3`/`ident` 数字（手抄会随装置改进漂移；本表的旧数字已被 QA-1 抓到陈旧）。要点：
三支各一份账本（种子、RNG 逐次、每个接收者的 RM 与 SM），`p3` 是**目标 ActorId**，
按 1000/2000/3000 分段（同一进程多用例必须不同 ActorId，见洁癖区第 5 条）。

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

| 原版 RM（本卡各支出现过的） | 产生 SM 帧的 C# 位置（**硬要求：没有行号不算映射**） | Rust 侧对应物 / 比较方式 |
|---|---|---|
| `RM_SPELL(10008)` wParam=Effect, p1/p2=坐标, p3=MagicId, prio=24 | `src/M2Server/Player/PlayObject.Message.cs:1454-1459`（`MakeMessage(SM_SPELL, …, nParam1, nParam2, wParam)`，p3 走尾串） | `SM_SPELL` 帧：Effect/坐标/MagicId 逐项。**注意自守卫**：`:1455` 的 `if (processMsg.ActorId != ActorId)`（`:1456` 只是它的 `{`）⇒ **施法者自己不会收到 SM_SPELL**（只发给范围内其他玩家），RM 账本里出现 ≠ 该玩家收到客户端帧 |
| `RM_MAGICFIRE(10120)` wParam=0, p1=MakeWord(EffectType,Effect), p2=MakeLong(x,y), p3=目标ActorId, prio=136 | `src/M2Server/Player/PlayObject.Message.cs:1788-1794`（`SM_MAGICFIRE`，`LoWord/HiWord(nParam2)` + `nParam1`，`nParam3` 经 EDCode 作尾串） | `SM_MAGICFIRE` 帧：p3=0 表示无目标 |
| `RM_DELAYMAGIC(8021)` wParam=伤害, p1=坐标, p2=nRage, p3=目标, prio=255 | **服务端事件，不是客户端帧**：`src/M2Server/Actor/BaseObject.Base.cs:711-728`（`GetMagStruckDamage` → 动物 race 伤害 `/1.2` → 范围内则发 `RM_MAGSTRUCK`） | 比较"是否登记延时事件 + 伤害 + nRage + 目标"；不产 SM 帧 |
| `RM_MAGIC_LVEXP`（训练尾） | `src/M2Server/Player/PlayObject.Message.cs:1800-1801`（`SM_MAGIC_LVEXP`，p1/p2 + `LoWord/HiWord(nParam3)`） | `SM_MAGIC_LVEXP` 帧：字段逐项 |
| `RM_MAGHEALING(8034)` wParam=0, p1=nPower, p2=0, p3=0, prio=255（`SendSelfDelayMsg`） | **服务端事件，不是客户端帧**：`src/M2Server/Player/PlayObject.Message.cs:1619-1629`（到点只累计 `IncHealing`/`PerHealing`，上限 300 的比较在 `:1620`；实际回血由后续 `Run` 的恢复周期另出 53） | 比较"是否登记延时治疗事件 + nPower + 目标"；**不产 SM 帧**（与 `RM_DELAYMAGIC` 同类） |
| `RM_DURACHANGE(10125)` wParam=槽位, p1=持久, p2=DuraMax, prio=141（`10125 & 0xFF`）、late=false | `src/M2Server/Player/PlayObject.Message.cs:1803-1804`（→ `SM_DURACHANGE(642)`：Recog=Dura、Param=槽位、Tag/Series=DuraMax）。护身符支的触发者 = `MagicBase.UseAmulet`（`src/M2Server/Magic/MagicBase.cs:109-127`，每次 −100 持久；持久不足时改走 `SendDelItems` 并清 `Index`）⇒ **火符的 miss 支同样出现 642**。**队列序**：与 `RM_MAGICFIRE(10120, prio 136)` 同队、按 `(byte)ident` 出队 ⇒ **638 先、642 后**（火符修复卡按此把 642 纳入同批 `SpellFrameBuf`，仅 `UseAmulet` 路径） | `SM_DURACHANGE` 帧逐字段 + `UseItems[ArmRingl].Dura` 的 −100 + caster 的 638→642 顺序 |
| `RM_TRANSPARENT(8045)` wParam=0, p1=`nHTime`, p2/p3=0, prio=255（`SendSelfDelayMsg`；**队列 = 该目标自己**，延时 800ms） | 产生者 `src/M2Server/Magic/MagicManager.cs:384-389`（19 支）/ helper `:1209-1228`（`SendSelfDelayMsg(RM_TRANSPARENT, 0, nHTime, 0, 0, "", 800)`）；到点处理 `src/M2Server/Actor/BaseObject.Base.cs:760-761` ⇒ `MagicManager.MagMakePrivateTransparent(this, (ushort)nParam1)` ⇒ `StatusChanged()` | **收件人 = 被施加者自己**（到点由它自己处理，不产 SM 帧）；比较 = RM 多重集（who=被施加者、p1=`nHTime`、priority=255、late）。到点后的 657 面见 `RM_CHARSTATUSCHANGED(10139)` 行 |
| `RM_MAGSTRUCK(8044)` wParam=0, p1=`nPower`, prio=108（`8044 & 0xFF`） | 产生者 `src/M2Server/Magic/MagicManager.cs:1111`（23/33/47 的 `MagBigExplosion`）/`:1138`（24 的 `MagElecBlizzard`）/`src/M2Server/Actor/BaseObject.Base.cs:726`（`RM_DELAYMAGIC` 延时支）；handler `src/M2Server/Player/PlayObject.Message.cs:1563` 起（`wParam>0 && ActorId==自己` ⇒ `SetPkFlag(nParam3)`/`SetLastHiter`；`:1601` 出 `SM_STRUCK(31)`） | **收件人 = 受击者自己**（`SendMsg(baseObject, …)` 进其队列）；比较 = `SM 31` 严格序 + 字段（Recog=受击者、Param=HP、Tag=MaxHP、Series=damage）+ RM 多重集（p1=nPower、priority=108、late=false） |
| `RM_STRUCK_MAG(8103)` wParam=`nDamage`, p1=HP, p2=MaxHP, p3=攻击方 ActorId, prio=167（`8103 & 0xFF`） | 产生者 `src/M2Server/Actor/BaseObject.Base.cs:629`（受击者 `SendRefMsg(RM_STRUCK_MAG, nDamage, WAbil.HP, WAbil.MaxHP, 攻击方)`）；handler 同 `PlayObject.Message.cs:1563` 支（收件人各自出 `SM_STRUCK(31)`，受击者自己此外还对攻击方 `SetPkFlag`） | **收件人 = 受击者 ±`SendRefMsgRange` 的玩家（含受击者自己）**；比较 = `SM 31` 帧集 + RM 多重集（wParam=damage、p1=hp_after、p2=max_hp、p3=攻击方、167） |
| `RM_CHANGENAMECOLOR(8090)` wParam=0, p1..p3=0, prio=154（`8090 & 0xFF`） | 产生者 `src/M2Server/Actor/BaseObject.cs:730-733` `RefNameColor()`（受击者 `SetPkFlag(攻击方)` 命中后由**攻击方**发）；handler `src/M2Server/Player/PlayObject.Message.cs:1677-1679` ⇒ `SM_CHANGENAMECOLOR(656)` | **收件人 = 攻击方 ±`SendRefMsgRange`（含攻击方自己）**；比较 = `SM 656` 严格序 + 字段 + RM 多重集（priority=154）。**组装来源必须是内部事件 `hitter_name_color`，不得看 `SM 656` 帧是否存在来反推** |
| `RM_DELAYPUSHED(8043)` wParam=nDir, p1=坐标, p2=push, p3=目标 | **服务端事件**：`src/M2Server/Actor/BaseObject.Base.cs:733-739`（到点 `CharPushed`，逐格 `SendRefMsg(RM_PUSH)`）；`RM_PUSH(8092)` → `src/M2Server/Player/PlayObject.Message.cs:1525-1533`（`SM_BACKSTEP(9)`）。夹具前提：**`UserMagic.Level=3`**（`Random(Level)-1`，Level<3 时恒 ≤0 ⇒ 永不推挤；产生点在 `src/M2Server/Magic/MagicManager.cs:1048-1052`） | `SM_BACKSTEP(9)` 帧逐字段 + 逐格条数；其余 SM 不变 |
| `RM_ABILITY(10051)` wParam=0, p1/p2/p3=0, prio=67（`10051 & 0xFF`） | `src/M2Server/Player/PlayObject.Message.cs:1732-1735`（→ `SM_ABILITY(52)`：`Gold`、`MakeWord(Job,99)`、`LoWord/HiWord(GameGold)`，尾 `EDCode.EncodeMessage(WAbil)`）。产生者：`RecalcAbilitys` 收尾（`src/M2Server/Actor/BaseObject.cs:2161`/`:2185`）与 buff 系统（`src/M2Server/ActorBuffSystem.cs:229`），均 `SendMsg(this)`；`src/M2Server/Player/PlayObject.cs:1385` 的 `SendPriorityMsg(..., High)` 亦给自己 | **收件人 = 该对象自己**；比较 = `SM_ABILITY(52)` 按序 + 尾字节；RM 列多重集 |
| `RM_CHARSTATUSCHANGED(10139)` wParam=0, p1=`CharStatus`（int 位集，实测可为 `0x80000000`）, prio=155 | 产生者 `StatusChanged()`（`src/M2Server/Actor/BaseObject.cs:1514-1517`）走 `SendRefMsg`；handler → `src/M2Server/Player/PlayObject.Message.cs:1845`（`SM_CHARSTATUSCHANGED(657)`：`ActorId`、`LoWord/HiWord(nParam1)`、`this.HitSpeed`）；`src/M2Server/Player/PlayObject.cs:2876` 的 `SendUpdateMsg` 只给自己 | **收件人 = 产出者 ±`SendRefMsgRange` 的玩家（含自己）**；比较 = `SM 657` 严格序 + 字段；RM 列多重集 |
| `RM_SYSMESSAGE(11004)` wParam=0, p1=前景色, p2=背景色, p3=0, prio=252, `Msg`=文本 | 产生者 `SysMsg`（`src/M2Server/Actor/BaseObject.cs:1617/1641/1648/1655/1666` 的 `SendMsg(RM_SYSMESSAGE, 0, fColor, bColor, 0, sMsg)`）→ `src/M2Server/Player/PlayObject.Message.cs:1696` 支（`SM_SYSMESSAGE(100)`：`ActorId`、`MakeWord(fColor, bColor)`、0、1；尾 `EDCode.EncodeString(Msg)`） | **收件人 = 自己**；比较 = `SM 100` 字段 + 尾文本逐字节；RM 列多重集（`msg` 文本与尾同源） |
| `RM_DOOPENHEALTH(8040)` wParam=0, p1..p3=0, prio=255（`SendSelfDelayMsg`；**队列 = 目标自己**，延时 1500ms） | 产生者 `src/M2Server/Magic/MagicManager.cs:438`（28 支：`targetObject.SendSelfDelayMsg(RM_DOOPENHEALTH, 0,0,0,0, "", 1500)`）；到点处理 `src/M2Server/Actor/BaseObject.Base.cs:763` ⇒ `MakeOpenHealth()`（`src/M2Server/Actor/BaseObject.cs:944-950`：置 `ShowHp`、`CharStatusEx |= OPENHEATH`、`SendRefMsg(RM_OPENHEALTH, 0, WAbil.HP, WAbil.MaxHP, 0)`） | **服务端事件，不产 SM 帧**；比较 = RM 多重集（who=`目标`、priority=255、late）。**未到期时只进 `NotDue`（`now/delivery` 是墙钟、不可比）⇒ 需要 1100 帧的 case 必须把排水等待跨过 1500ms**（见 §25(a)） |
| `RM_OPENHEALTH(10410)` wParam=0, p1=`WAbil.HP`, p2=`WAbil.MaxHP`, p3=0, prio=170（`10410 & 0xFF`） | 产生者 `src/M2Server/Actor/BaseObject.cs:949`（`SendRefMsg`，±`SendRefMsgRange` 的玩家**含自己**）；handler `src/M2Server/Player/PlayObject.Message.cs:2009-2011`（`SendDefMessage(SM_OPENHEALTH=1100, processMsg.ActorId, baseObject.WAbil.HP, baseObject.WAbil.MaxHP, 0)`；`baseObject = ActorMgr.Get(processMsg.ActorId)`，见 `:774-778`） | **收件人 = 目标 ±`SendRefMsgRange` 的玩家（含目标自己）**；比较 = `SM 1100` 严格序 + 字段（Recog=`ActorId`、Param1=HP、Param2=MaxHP、Tag=0）。**字段来源 = 被开血者的实时 `WAbil`**（Rust 侧对应实体 `vitals.hp/max_hp`，见 §25(b)） |
| `RM_HEALTHSPELLCHANGED(10052)` wParam=0, p1..p3=0, prio=68（`10052 & 0xFF`） | 产生者 `src/M2Server/Actor/BaseObject.cs:700-711` 的 `HealthSpellChanged()`：`:704` `SendUpdateMsg(...)`（仅 `Race==Play`，给自己）+ `:708` `SendRefMsg(...)`（`ShowHp` 时，±range 含自）；handler `src/M2Server/Player/PlayObject.Message.cs:1736-1738`（`MakeMessage(SM_HEALTHSPELLCHANGED=53, processMsg.ActorId, baseObject.WAbil.HP, baseObject.WAbil.MP, baseObject.WAbil.MaxHP)`） | **收件人 = 自己（SendUpdateMsg）+ ±`SendRefMsgRange` 含自（SendRefMsg）**；比较 = `SM 53` 严格序 + 字段。28 支的 cast 窗口内无 HP 变化 ⇒ **预计 0 条**；**出现即须可比**（不得静默忽略） |

RNG 账本逐次比较用 `crates/shared/src/rng.rs` 的 `DotNetRandom`/`RandomNumber`（与原版同算法）。
**未登记在该表里的 RM 条目一律判"口径未定义"（不标 verified）**，避免拿"没比到"当"比过了"。

## 12. C# 侧记录面已支持两颗负向牙（**未断言**；判定在 Rust 比较器）

装置改为**真实 SM 帧**：`RecordingPlayObject` 覆写 `PlayObject.Socket.cs:101` 的 **virtual**
`SendSocket(CommandMessage,string)`，并用 `Operate(ProcessMessage)`（`PlayObject.Message.cs:768`）
走生产编码路径处理各接收者队列 ⇒ 记录的是**过完自守卫之后真正要发出去的帧**。

场景：施法者(100,100)、目标(101,100)、观察者-范围内(105,100)、观察者-范围外(140,100)。

帧账本**以金标准 `golden/magic/fireball.json` 为准**（本表不再手抄帧号）。要点：
- 三支里 target 与 obs_in 收到的帧集合**一致**（含 `RM_DELAYMAGIC` 到期后的伤害链帧），
  obs_out 恒空；caster 只收广播类帧、**不收 `SM_SPELL`**；
- 具体 `ident/param/series/tail` 逐字段以金标准为准（QA-1 已独立复现：绿）。

- **牙①（负向）**：三支里施法者收到的 `SM_SPELL(17)` **恒为 0**（只有 `SM_MAGICFIRE 638`）⇒
  `PlayObject.Message.cs:1455` 的 `if (processMsg.ActorId != ActorId)` 自守卫被钉住（`:1456` 是 `{`）。
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

## 14. Rust 侧驱动实现计划（下一步，可按此直接开工）

1. **位置**：`mir2-rs/bins/game-svr/src/lib.rs` 的测试模块——`handle_spell_cmd_with_magic`
   （`lib.rs` 的 `handle_spell_cmd_with_magic`）是 crate 私有函数，集成测试用不了；现有法术测试（如
   `spell_cmd_a5_defence_area_buffs_target_in_range`）就在同文件，直接沿用其夹具范式。
2. **输入复现**：读 `mir2-rs/tests/parity/golden/magic/fireball.json`（`include_str!`），
   对每支：`seed`、`anti_magic`、四个 ActorId（1001/1002/1003/1004 + 支偏移）、坐标
   (100,100)/(101,100)/(105,100)/(140,100)、目标 `WAbil.HP=100`。
3. **世界构造**：加载 `Map/0.map`（夹具路径走环境变量，默认 `E:/MirServer/M2GameSvr/Map`）、
   四个实体挂图；RNG 用 `crates/shared/src/rng.rs` 的 `DotNetRandom`/`RandomNumber` 以同一
   `seed` 初始化（`RngRes`），并在外层包一层**记账**取逐次抽取（与原版 `LoggingRandom` 同口径）。
4. **调用**：`handle_spell_cmd_with_magic(recog, tag, param, series, chr_name, socket, …)`，
   采 `SM_*` 帧（Rust 直接产帧，见 §11 的 RM↔SM 映射）。
5. **延时**：推进世界时钟 ≥600ms 并 tick，采第二段帧（`SM_STRUCK`）与目标 HP 前后值。
6. **比较器（逐接收者，不许比总数）**：
   - 帧层：每个接收者的 `(ident, param/series, tail)` 逐项；
   - 事件层：`RM_DELAYMAGIC` 等价事件是否登记（伤害/nRage/目标/到期）；
   - 状态层：`target HP 100→100`（miss）/ `100→85`（edge/hit）作为一级断言；
   - RNG 层：`[2]` / `[3,0,0,0,4]` / `[5,0,0,0,0]` **逐次取值**对齐；
   - 负向牙：caster 收 `SM_SPELL(17)` 恒 0；`obs_out` 全空无帧。
7. **全绿后**：`magic-parity.json` 第 1 条（`SKILL_FIREBALL` `:174`）改
   `status=verified`、`basis=semantic`、`test=<用例名>`；跑 `coverage-report.ps1` 自检后报数。

## 15. 分段采集的 RNG 口径（裁定：选方案 1，带降级）

**问题**：C# 侧 `DoSpell` 是**直接调用**、无世界 tick；Rust 侧若用完整 `world.tick()` 推时钟，
其它系统抽 RNG 会插队，逐次比较会红得没有意义。

**裁定（opc-07 倾向 + 我同意）：方案 1「最小世界」**——
- 第一段：**直接调用** `handle_spell_cmd(_with_magic)`（不 tick）；
- 第二段：**只调用延时结算系统**，不跑整条 `HeadRunner`。已确认可直接调用的入口：
  `mir2_world_bevy::world::sim_self_deliver_stage_system`（`world.rs`，`pub fn`），
  延时魔法的结算体在 `crates/world-bevy/src/slave.rs` 的 `deliver_delay_magic_impl` /
  `DelayMagicDelivery`（`slave.rs`）。
- 时钟推进：只改世界时钟字段（或把 `DeliveryTime` 直接置为已到期），**不跑 tick 循环**。

**降级条款（必须写死，防止方案 1 不可行时悄悄走样）**：若最小世界在**不改生产代码**的前提下
装不起来（例如延时结算必须经由宿主某段只存在于 tick 的接线），则退到**方案 3「账本分段」**：
记录"段 1（施法内）/ 段 2（延时结算）"两次采样，并加断言「两段之间 RNG 只被 magic 路径消耗」
（用记账器在段边界取快照、空段或非 magic 消耗即红）。**方案 2（独立 RNG 实例）永不采用**——
那是产品语义改动，除非登记 D-* 并有对拍证明。

### 15b. 两段直调的两条补充（opc-07，2026-11-19）

1. **时间源必须与生产一致**：直调 `sim_self_deliver_stage_system` 时传的 `clock` 必须是
   "推进后的世界时钟"（与 tick 内传的是同一个量），**禁止用系统墙上时间**——否则延时判定的
   边界会漂，逐次账本对不上。
2. **总量断言（便宜且早暴露）**：两段跑完后必须满足
   `RNG 总抽取次数 == 金标准账本长度`（miss 1 / edge 5 / hit 5）。
   这条先于逐次比对执行——它能抓"某个东西偷偷抽了一次随机"，且失败信息更直接。

## 16. Rust 侧夹具与记账配方（已勘察，可直接照做）

- **RNG 构造与注入**：`RandomNumber::with_seed(i32)`（`crates/shared/src/rng.rs:251`）；
  `World.rng` 是 **public 字段**（`crates/world-bevy/src/world.rs` 的 `SimState::rng`）⇒ 夹具里
  `world.rng = RandomNumber::with_seed(seed);` 即可，无需改生产代码。
- **实体属性落点**：`EntitySpawn` 有 `hp: u16`（`entity.rs` 的 `EntitySpawn::hp`）、`anti_magic: u16`（`:565`）、
  `abil: Option<BaseAbil>`（`:568`）⇒ 夹具按金标准置 `hp=100 / anti_magic=3`，MC/Level 走 `abil`。
- **记账器（现成，别自己包）**：`crates/shared/src/rng_trace.rs` 的生产级逐次 trace——
  设 `MIR2_RNG_LOG=<tsv 路径>` 后，`RandomNumber` 的每个取数入口都会落
  `seq \t kind \t max/arg \t result \t site`（与 C# 钩子 `StartupHook.cs` 同列同义），
  `site` 用 `#[track_caller]` 记到世界层调用点。**顺序即实际取数顺序**。
  用法：驱动测试用 `--exact` 单跑（trace 是进程级 `OnceLock`），开跑前截断目标文件；
  比较器的 **总量断言** 直接数 TSV 行数（应 = 金标准 `rng` 长度），再逐行比 `result` 列。
- **帧**：沿用 `a5_spell_rig` 的 `gates`（`mpsc::UnboundedSender<SimToGate>`）取 `SM_*`；
- **第二段**：`sim_self_deliver_stage_system(world)`，clock 用**推进后的世界时钟**（禁墙上时间）。
- **比较器顺序**：①总量（TSV 行数 == 金标准长度）→ ②RNG 逐次 `result` 列 → ③逐接收者 SM
  → ④HP 前后 → ⑤两颗负向牙。

### 16b. 总量断言的写法（定死：强形式 + 行号分段 + site 允许清单；**当前未实现**，随 Rust 比较器落地）

**事实**：夹具建实体本身会抽 RNG（`new_player_entity_with_login_ctor` 的注释写明
"`new PlayObject()` 5 抽里存活到实体的两个值"，`lib.rs` 的 `new_player_entity_with_login_ctor` 文档注释）⇒ 不能直接数整份 TSV。

**定死的写法（强形式）**：
1. **行号分段（不是截断文件）**：`rng_trace` 的写入端持句柄/缓冲，运行中截断会产生空洞或让
   写入端偏移错乱 ⇒ **禁止截断**。改为在夹具就绪时记**已有行数 `n0`**、第一段结束记 `n1`、
   第二段结束记 `n2`；账本 = 第 `n0..n1` 与 `n1..n2` 两段行（跳过前缀）。
2. **连续性自检**：两段账本的首尾 `seq` 必须连续、无缺号（`seq` 由 trace 自带且全局单调），
   `n2 - n0` 必须等于实际读到的行数——仪器自身先自证，再用它验魔法。
3. **来源断言（强）**：账本里**每一行**的 `site` 必须落在允许清单内：

   | site（前缀） | 为什么合法 |
   |---|---|
   | `crates/world-bevy/src/magic.rs` | 原版 `MagicManager` 的威力/骰门/推挤等纯函数取数点 |
   | `crates/world-bevy/src/magic_effect.rs` | 魔法效果计算（AntiMagic 骰门、`GetPower*` 等）|
   | `crates/world-bevy/src/world.rs`（`cast_*` 系列） | 施法效果体的落地实现，对应 `DoSpell` 的 case 体 |
   | `crates/world-bevy/src/slave.rs`（`deliver_delay_magic_impl`） | `RM_DELAYMAGIC` 到期结算，对应 `BaseObject.Base.cs:711` |
   | `crates/world-bevy/src/combat.rs`（攻击力区间） | `GetAttackPower(base,power)` 的随机区间（原版同函数）|
   | `bins/game-svr/src/lib.rs`（施法处理器） | 宿主侧施法接线（帧参数/训练尾等）|

   清单外站点 ⇒ **红**（"别的东西偷偷抽了随机"）。**扩充清单必须逐条写理由并进评审 diff**，
   不允许"能过就加"。
4. **数量断言**：两段合计行数 == 金标准账本长度（miss 1 / edge 5 / hit 5）。
5. 合法取数的 `site` 若落在清单外，只允许**按上表格式补条目并写理由**，不允许改成"数总数"
   来放行——**不出现"总数对得上但来源不同"的假绿**。

### 16c. 火球用例的两个执行约束（opc-07，2026-11-19）

1. **必须串行**：`rng_trace` 是**进程级全局状态**，并行测试同时 `enable` 会把 TSV 写串。
   要求：这组用例单跑（`cargo test -p game-svr --lib fireball_parity -- --test-threads=1`
   或 `--exact` 单测）；**夹具每次用唯一文件路径**（带用例名与种子），测完删除；
   跑完以**`seq` 连续**作为"没有串写"的证据（与 §16b 的自证同一条）。
2. **trace 的 seed 只作元数据**：真正驱动数值的是 `world.rng`。夹具里必须写成一行显式赋值并注释：
   `world.rng = RandomNumber::with_seed(seed);   // 必须与 rng_trace::enable(path, seed) 的 seed 一致`
   —— 两处不一致 ⇒ 账本记的和世界抽的不是同一个流，比较全废。

**属性落点（本轮查实，下一步直接用）**：`EntitySpawn` 直接有 `hp`（`entity.rs` 的 `EntitySpawn::hp`）、
`anti_magic`（`:565`）、`level`（`:571`）；`abil: Option<BaseAbil>` 只有 `dc / max_hp`（`:497-502`）
—— **MC 不在这两个字段里**，需在 `AbilitySnapshot`/WAbil 组件上落，写夹具前先确认该组件字段
（下一条提交解决，不猜）。

## 17. QA-1 独立复现（2026-11-19，worktree `qa-3b`）

独立 worktree @ `d35550db7` 重跑 C# 火球用例：**RC=0，三支账本与已提交金标准 `ALL_IDENTICAL`**
（仅 `delivery` 墙钟字段不同，不入比较面）。QA 同时指出卡面两处手抄数字陈旧（§10 的 `p3`、
§12 的 SM 集合），已按建议改为**只引用金标准路径 + 要点**，不再手抄数字。

## 18. `priority` 进比较面（QA-2，2026-11-19）

- 金标准已补 `priority` 字段（`DrainRaw` 连同 `MsgQueue` 的入队优先级一起取），
  `golden/magic/fireball.json` 三支全部带；取值来源：
  **`BaseObject.Message.cs:181`**（`MsgQueue.Enqueue(sendMessage, (byte)wIdent)`，普通 RM 用 ident 当优先级）
  与 **`:204`**（`SendSelfDelayMsg` 用 `(byte)MessagePriority.Lowest` = 255）。
- 实测值：`RM_SPELL 24`、`RM_MAGICFIRE 136`、`RM_DELAYMAGIC 255`、`RM_STRUCK_MAG 167`、
  `RM_MAGSTRUCK 108`、`RM_HEALTHSPELLCHANGED 68`、`RM_CHANGENAMECOLOR 154`。
- 比较器必须逐字段比 `priority`（不许只比 ident/params）；QA 将复核该列。

## 19. 记录面 vs 判定面（QA-3，2026-11-19）

**归属写清，避免再被读成"已有断言"**：

| 面 | 产物 | 现状 |
|---|---|---|
| 记录面（C# 金标准） | `golden/magic/fireball.json`：每个接收者的 RM（含 `priority`）与真实 SM、RNG 账本、HP 前后 | **已具备**（QA-1 独立复现绿） |
| 判定面（Rust 比较器） | 断言：逐接收者 RM/SM 逐字段、`priority`、HP 差、RNG 逐次；**以及两颗负向牙**（caster `SM_SPELL(17)` 计数 == 0；`obs_out` RM/SM 全空）与 §16b 的 RNG 总量/`site` 允许清单/`seq` 连续 | **未实现**（随驱动一起写） |

**判定面必须有"咬得住"的证明**（缺一不可，各给原文）：
1. 把金标准里 caster 的 SM 塞一条 `17` ⇒ 比较器红；
2. 把某条 RNG 行的 `site` 改成清单外的值 ⇒ 比较器红；
3. 删掉金标准里一条 RNG 行 ⇒ 比较器红（总量断言）；
恢复后复绿。

**没有比较器与这三条咬合证明之前，`1/5` 不得标 `verified`。**

## 20. QA 章程对开发侧的硬约束（`E:\OPC\docs\qa-charter.md`，2026-11-19）

1. **交付件必须能自证**：自报 `rev/dirty/built_at` + 侧车戳 + 冻结副本 + 构建记录；
   **只有侧车戳不算**。
2. **禁止手抄数据**：卡面/报告一律引用机器产物路径（golden / JSON / TSV / 日志）——
   §10/§12 已吃过一次亏，现均改为引用 `golden/magic/fireball.json`。
3. **防空转**：比较器在**空白/缺文件/零用例时必须红**；断言必须能被三条篡改咬住；
   清单必须有**真过滤代码**（`site` 允许清单目前仍未实现，随比较器落地）。
4. **合并前门禁在合并 head 上跑**，不得用分支 head 替代。
5. **替换生产件前必须备份**（旧件 + 旧戳 + 清单），回滚步骤写进交付记录。
6. **QA 三态签署**（PASS / CONDITIONAL / FAIL）：**没有 QA 签署的交付件不得进生产**。

**对 `1/5` 标 `verified` 的影响**：除三支全绿 + 两牙 + 三条篡改证明外，还需过
`openmir2-3b` 的 QA-4。顺序：**夹具 → 比较器 → 篡改证明 → 交 QA**。

### 21. MC / Level 的真实落点（2026-11-19 查实，夹具照此写）

- **MC 与 SC 不在实体组件上**，而在**会话态** `SessionGround.state.wabil.mc / .sc`
  （消费点：`lib.rs` 的 `handle_spell_cmd_with_magic`、`:14069`、`:14307`、`:14801`、`:15248` 均为
  `i32::from(sg.state.wabil.mc)`）；现有测试直接赋值有先例：`lib.rs` 的 `b2_rig`
  `sg.state.wabil.mc = 0x0F05;`。
- 存档记录侧同名字段在 `bins/game-svr/src/record.rs` 的 `Ability::mc`（由 `stage6_record()` 解出）。
- **火球夹具**：`a5_spell_rig(1, 11, 10)` 之后
  ① `ground_sessions["caster"].state.wabil.mc = 15;`（对应金标准 MC=15）
  ② 目标实体 `anti_magic = 3`、`hp = 100`；③ `world.rng = RandomNumber::with_seed(seed)`。
  **不再去找"实体 MC 组件"——它不存在。**

### 19b. 判定面已落地（2026-11-19）：`mir2-rs/tests/parity/tests/magic_fireball_parity.rs`

- **防空转**：金标准缺字段/`cases` 为空/两侧用例数不等 ⇒ 红（两条用例）。
- **两颗负向牙**：caster `SM_SPELL(17)` 计数 == 0；`obs_out` 的 `Rm`/`Sent` 全空。
- **逐接收者逐字段**：RM `ident/wParam/p1/p2/p3/priority/late`、SM `ident/param/tag/series` + `tail`。
- **HP 一级断言**：`TargetHpBefore/TargetHp` 两侧相等。
- **RNG**：行数、逐次取值、`RngSites` 每行 ∈ `SITE_ALLOWLIST`（代码里的表，逐条带理由）、`RngSeqs` 连续。
- **咬合证明（三条篡改）已跑通**：塞 `SM_SPELL(17)` ⇒ 红在负向牙①；把某条 site 改成
  `crates/world-bevy/src/mongen.rs` ⇒ 红在允许清单；删一条 RNG ⇒ 红在行数。
  `cargo test -p mir2-parity-tests --test magic_fireball_parity` ⇒ **6 passed / 0 failed**
  （两条防空转 + identity + 三条篡改）。
- **未完成的部分**：identity 台账是从金标准克隆的（`RngSites/RngSeqs` 为占位），
  **驱动的真实台账尚未接入** ⇒ `1/5` 仍不得标 `verified`；下一步是火球夹具产出真实台账后
  把这两侧接上（比较器本身不再改动）。

### 19c. 三条咬合证明的可重放构造（QA 独立复核用）

用例文件 `mir2-rs/tests/parity/tests/magic_fireball_parity.rs`；**每条都在内存里改副本，不碰金标准文件**：

| 用例 | 改哪份副本 | 怎么改 | 期望红在哪条断言 |
|---|---|---|---|
| `tooth_caster_sm_spell_present_red` | `golden()` 的克隆（`tampered`） | 对 `cases[0].Receivers` 里 `Who=="caster"` 的 `Sent` **追加** `{"ident":17,"param":0,"tag":0,"series":1,"tail":"1"}` | `负向牙①：caster 收到 N 条 SM_SPELL(17)` |
| `tooth_foreign_site_red` | `ledger_from(golden)`（Rust 侧台账） | `cases[0].RngSites[0] = "crates/world-bevy/src/mongen.rs"`（清单外） | `RNG[0] site 不在允许清单` |
| `tooth_missing_rng_line_red` | 同上 | `cases[0].Rng` **弹掉最后一条**（行数与 `RngSites` 同步少一条） | `RNG 行数不等` |

复跑：`cargo test -p mir2-parity-tests --test magic_fireball_parity`（6 例）。
**已知局限（写死）**：以上证明的是"篡改副本必红"；**真实驱动台账接上后，必须再确认
"清单外的真实 site 会红"**（现在只证明了"把已知 good 的 site 改成清单外会红"）。

### 22. 现成夹具 `SpellFrameRig`（驱动最后一处弯路已消除）

`lib.rs` 的 `SpellFrameRig` 已具备火球所需的**全部**结构：

- `SpellFrameRig::new(learned, &[(magic_id, effect)], observers: &[(&str, x, y, ghost, socket)])`
  —— caster 固定 (10,10)/socket 911；观察者按参数建实体并登记 `OnlineRegistry`（各自 socket）；
- 字段：`world / caster / online / saves / ground_sessions / magic_table / displays / gates / rx`；
- 帧：`rx`（`mpsc::UnboundedReceiver<SimToGate>`），`SendFrame { socket, body }` 可按 socket 归属；
- 先例 `b2_rig`（`lib.rs` 的 `b2_rig`）已演示如何落属性：`world.entities.get_mut(id)` 改
  `progression.level / vitals.hp / max_hp`，`ground_sessions["caster"].state.wabil.mc/level`。

**火球夹具因此只需**：`SpellFrameRig::new(1, &[(1,1)], &[("target",11,10,false,912),
("obs_in",15,10,false,913),("obs_out",60,10,false,914)])`
→ 覆盖 `magic_table`（effect=1/effect_type=1）→ target `anti_magic=3 / vitals.hp=100`
→ caster `state.wabil.mc=15 / level=30` → `world.rng = RandomNumber::with_seed(seed)`
→ `rng_trace::enable(path, seed)`（同 seed）→ 记 `n0` → `handle_spell_cmd(...)` → 记 `n1`
→ 从 `rx` 按 socket 归集 SM、从 TSV 取 `n0..n1` 的 RNG 行 → 组装与金标准同构的台账 JSON。

### 22b. 真实台账接线（2026-11-19）：A/B/C 已落地，比较器红在 D

- **记录面补全（口径 A，先修 oracle）**：C# 驱动不再只覆写双参 `SendSocket(msg, tail)`，
  改为把 `M2Share.NetChannel` 换成只记录不发送的 `RecordingNetChannel`——三处 `SendSocket`
  重载最终都汇到 `AddGateBuffer`（生产 C# 零改动），各玩家 `GateIdx = ActorId` 用于归属。
- **排水窗口 = 抽空为止（opc-07 裁定）**：固定两趟是**记录面伪影**——RM 由 `Operate` 触发
  下一条（`RM_STRUCK_MAG` ⇒ `SetPkFlag` ⇒ `RM_CHANGENAMECOLOR`），两趟会把「已入队未排水」的
  条目漏掉；真机上它们在后续 tick 会投给所有可见者。现循环到某趟无消息为止，硬上界
  `MaxDrainPasses = 8`（防自发消息增生），**上界命中进产物**（`DrainPasses` /
  `DrainPassesHitCap`，另有 stderr `DRAIN_PASSES` 行）——不得静默截断。
  实测趟数：miss 2 / edge 4 / hit 4，均未命中上界。
- **golden 重生成（新面）**：相对两趟版只新增（`delivery` 墙钟字段不入比较面）——
  target 的 `SM_HEALTHSPELLCHANGED(53)`（两趟版已含）之外，edge/hit 的 **caster / target /
  obs_in 三者的 `SM_CHANGENAMECOLOR(656)` 与 `RM_CHANGENAMECOLOR(8090)`**；miss 不变。
  **QA-1（`openmir2-3b`）必须在新的排水窗口上重跑**；旧 golden 的任何「ALL_IDENTICAL」结论作废。
- **QA-1 重跑通过（`openmir2-3b`，钉死 head `c5a6c4484`，2026-11-19 23:48 +0800）**：
  `--case fireball` rc=0；`DRAIN_PASSES miss=2 / edge=4 / hit=4、hit_cap=False`；产物与
  `golden/magic/fireball.json` **语义全等**（含 priority 与 `DrainPasses`，排除 `delivery` 墙钟），
  连跑两次逐项相同。QA 另对比较器三颗牙做变异验证：`late` 移出 `rm_fields` / 去掉 RM 排序 /
  `p1` 移出 `rm_fields` 分别红在对应用例，还原后源文件 sha 一致（比较器 9 passed / 1 ignored）。
- **判定口径（口径 B）**：RM 列改为**排序后按多重集**比较——顺序来自 C# `ActorEntity.MsgQueue`
  两趟排空与 `VisibleHumanList`，不是可复现的行为语义；SM 仍**严格按序逐字段 + 尾串**。
- **`late` 列假绿已修**：旧比较器对 JSON bool 取 `as_i64()` ⇒ 两侧恒 `i64::MIN`，该列从未真正
  比较；现按 1/0 比较。留档三颗牙：`tooth_rm_late_changed_red`、`rm_multiset_ignores_order_accepts`、
  `tooth_rm_field_changed_red`。
- **site 归一（口径 C）**：`file!()` 在 Windows 产出反斜杠；台账落盘前统一 `/`（只影响允许清单比较）。
- **Rust 真实台账**：`tests::fireball_parity_{miss,edge,hit}`（`#[ignore]`，必须 `--ignored --exact`
  单跑——`rng_trace` 进程级），产出 `<workspace>/target/magic-parity/fireball_<label>.json`。
  RM 列按 §11 映射表由 Rust 事件组装（`hear_targets` 收件人 + `self_delay_msgs` + `delay_magic`
  观察值；逐条来源见驱动注释）——**弱比较面**，玩家可见的真实证据是 SM 帧。
- **实测（三支）**：施法段 RNG 与 golden 逐次全等（`[2]` / `[3,0,0,0,4]` / `[5,0,0,0,0]`），
  HP 100→100/85/85；`SM_SPELL(17)` / `SM_MAGICFIRE(638)` / `SM_STRUCK(31)` 逐字段与尾串全等；
  RM 列除 `8090` 外**每个接收者全部字段多重集相等**。
- **当前红（恰 12 条 = edge/hit × {caster,target,obs_in} × {Rm 8090, Sent 656}）**：
  Rust `deliver_mag_struck` 对玩家的 `name_color` 恒 false ⇒ 魔法受击链不置 `PvpFlag`、
  不发 `RefNameColor` 广播。已报 opc-07 立卡 `openmir2-magstruck-name-color`；修好前
  **`1/5` 保持 implemented，不得标 verified**。
- **卡面锚清理（opc-07 批准，同日）**：31 条高 churn 文件的行号锚改为符号锚；
  11 条裸 `MagicManager.cs:NNN` 补全成 `src/M2Server/Magic/MagicManager.cs:NNN`
  （逐条核对 case 体：WINDTEBO / GROUPDEDING / 版本门 / 火球 / 瞬息移动 / 魔法盾 / 火墙 /
  推人 / 分身 / 驯服 / 心灵启示，**不是**已删符号）。本分支自己的改动已让 `lib.rs` 里
  `b2_rig` 一类行号漂移（70049→70613、70067→70627）——这就是改用符号锚的直接理由。
  `docs_anchor_gate --prose --base master`：**GREEN**（检查 32 / 悬空 0 / 越界 0 / 符号缺失 0 / 高 churn 0）。
- **2026-11-19 现场留痕**：曾因相对路径 `--out ../../golden/...` 把一份 golden 误落到主检出
  `golden/magic/fireball.json`；已按清单（单文件 17739B、父目录空）删除文件与目录，主检出
  `git status` 复原。后续删除一律「先列清单、再动手」，且报告在动手前。
- 复跑顺序（三支各自独立进程 → 真实台账比较；比较器新用例 `real_rust_ledger_parity` 为
  `#[ignore]`，缺台账时硬红不跳过）：
  ```text
  cargo test -p game-svr --lib tests::fireball_parity_miss -- --exact --ignored --test-threads=1
  cargo test -p game-svr --lib tests::fireball_parity_edge -- --exact --ignored --test-threads=1
  cargo test -p game-svr --lib tests::fireball_parity_hit  -- --exact --ignored --test-threads=1
  cargo test -p mir2-parity-tests --test magic_fireball_parity -- --ignored --exact real_rust_ledger_parity
  ```
### 23. 下一批 golden（C# 侧；Rust 驱动未接）

opc-07 派工：先做批序里下 4 支的 **C# golden**（`--case <name>`），Rust 驱动等
`openmir2-magstruck-name-color` 修完、比较器全绿后再逐支接。共用口径与火球一致：
golden 几何（caster(100,100)/target(101,100)/obs_in(105,100)/obs_out(140,100)，socket 911..914）、
ActorId 段 1001../2001../3001..、`M2Share.NetChannel` 旁路、排水「抽空为止」（上界 8；
每 case 落 `DrainPasses`/`DrainPassesHitCap`）、等待覆盖最长自延迟（1200ms）后再排水。
每 case 落 `Fixture`（魔法/施法者/目标输入），Rust 侧按它复现——**不手抄数字**。

| 分支 | 产物 | 状态 |
|---|---|---|
| 11/45 雷电 | `golden/magic/lightning-11.json`、`lightning-45.json` | **已接 Rust 驱动并升格**（MG-19 / MG-54，`basis=semantic`） |
| 2 治愈 | `golden/magic/healing.json` | **已产出（C#）**；Rust 未接。三支 = heal/full/self（本支**无** AntiMagic 骰） |
| 13 火符 | `golden/magic/firecharm.json` | **已接驱动（红→产品修复→绿）**；MG-27（`:328` body）升 `verified`；MG-20（`:315` 7 魔号共用组标签）按口径保持 `implemented_unverified` |
| 44 灭天火 | `golden/magic/hbfireball.json` | **已产出（C#）**；Rust 未接 |

雷电三支 seed 由 `Random(10)` 与 AntiMagic 的关系扫描（miss=1 / edge=5 / hit=13，与火球同）；
`DrainPasses` = miss 2 / edge 4 / hit 4，`hit_cap=False`；连跑两次除 `delivery` 外逐字节一致。
新表外 RM 的处置（是否进 §11 映射表）见 opc-07 裁定；**未自行扩大 oracle 补全范围**。

**2 治愈三支的预期差异**（没有 AntiMagic 骰，按行为分支取，seed 固定 1/5/13；不要为凑
miss/edge/hit 构造假骰）：

| 支 | 目标 | 目标 HP 增量 | `RM_MAGHEALING(8034)` | 训练尾 `RM_MAGIC_LVEXP(10123)`→`SM 640` | `targetObject` |
|---|---|---|---|---|---|
| `heal` | 目标(101,100)，HP=50/100 | 采集窗口内 **0**（回血按后续 `Run` 的恢复周期，本采集不跑 `Run`） | 有（p1=nPower） | 有 | 目标对象 |
| `full` | 目标 HP=100/100 | 0 | 无 | 无（仍 4 抽 + `SM 17/638`） | 目标对象 |
| `self` | 施法者(100,100)，HP=50/100 | 0 | 有 | 有 | **null**（`DoSpell` 首句把目标置为施法者，坐标也取施法者） |

实测：`heal` RNG `[0,0,0,7,1]`（多一抽 = 训练尾 `TrainSkill(Random(3)+1)`）、`full` `[0,0,0,6]`、
`self` `[0,0,0,3,0]`；三支 `DrainPasses=2`、`hit_cap=False`；两次连跑除 `delivery` 外逐字节一致。

**13 火符的夹具前置（为触发路径必须造，复现时别丢）**：离线物品系统
`SystemShare.ItemSystem = new GameItemSystem()` + `ArmRingl` 护身符
（`StdMode=25 / Shape=5 / Dura=600 / DuraMax=1000`，`Round(600/100)=6 ≥ CheckAmulet(1,1)` 的门）。
每支施法都会 −100 持久并回 `SM_DURACHANGE(642)`（Dura=500、槽位=5、DuraMax=1000）——**miss 支也有**；
`RM_DELAYMAGIC` 延时 1200ms（等待窗口 1200ms 覆盖）；`RM_MAGHEALING` 类表外项不出现。
实测：`RM 8021` 只在 edge/hit（命中才有延时事件）；三支 `DrainPasses` = 2/4/4、`hit_cap=False`。

**44 灭天火的夹具前提与推挤支**：`UserMagic.Level=3` 是硬前提（`Random(Level)-1` 在 Level<3 时
恒 ≤0，永不推挤）。`hit`（seed=13）推挤成立：`Random(20)=18 < 6+3*3+10=25`、`Random(3)=2 ⇒ push=1`，
目标被推 1 格（101,100→102,100），观测面 = `RM_DELAYPUSHED(8043)` 登记 + `RM_PUSH(8092)`/
`SM_BACKSTEP(9)`（param=102/tag=100/series=方向 6）；`edge`（seed=5）第 7 抽=0 ⇒ push=-1 不推；
`miss` 无延时事件。三支 `DrainPasses` = 2/4/4、`hit_cap=False`。

**id=5（`SKILL_FIREBALL2`）的 C# golden（Rust 驱动未接）**：`golden/magic/fireball-5.json`，
同一 rig 换 `magic_id=5`。与 id=1 的唯一差异 = `SM_SPELL(17)` 尾串 `'5'`（magic id）与
`RM_SPELL` 的 p3=5；抽数/帧集合/HP 与 id=1 逐项相同（RNG miss `[2]` / edge `[3,0,0,0,4]` /
hit `[5,0,0,0,0]`，`DrainPasses` 2/4/4、`hit_cap=False`）。自检：两次连跑除 `delivery` 外
逐字节一致；产物与仓库 golden 语义全等。**MG-11 已接 Rust 驱动**（`tests::fireball5_parity_*` + `real_rust_ledger_fireball5`，0 红）并升 `verified`（M18）。

**M17 合并的清单变更范围（补充说明，docs-only）**：本次 `docs/issues/parity-gap-magic.json`
的改动 = MG-10 `status`→`verified`（+ `basis=semantic` + evidence 五字段）+ `schema_version` 1→2
+ `schema_note` + MG-11 `notes` 文本；**`denominator.count`（65）与 65 个条目 id 不变，
MG-11 状态仍 `implemented_unverified`**。

**14/15/31 的 C# golden（golden-only，Rust 未接）**：`golden/magic/{hangmajinbub,dejiwonho,shield}.json`。

- **本三条无抗魔骰**（首抽即 `GetPower13`/`GetRPow`）：三 seed（1/5/13）只覆盖随机支，标签
  **`s1`/`s5`/`s13`**，**不构造假骰**（治愈先例）。14/15 有护身符前置（rig 自带 `ArmRingl`
  的 `StdMode=25/Shape=5/Dura=600`），31 无门。
- **帧量推导（2026-11-20 按 oracle 源码更正，旧文本作废）**：`MagMakeDefenceArea(nTargetX,
  nTargetY, 3, nPower, btState)`（`src/M2Server/Magic/MagicManager.cs:354/361`；`src/M2Server/Actor/BaseObject.cs:2094-2134`）的
  `nRange=3` ⇒ **7×7 = 49 次循环**，且每轮 `Envir.GetCellInfo(nX, nY)` 固定读**中心格**
  （oracle 原样，不是「区域枚举」）⇒ 中心格上的唯一 proper friend 被施加 49 次。
  - **14**（`btState=1`，`MagDefenceUp`）：**无条件** `AddBuff` + `SysMsg` + `RecalcAbilitys()`
    + `SendMsg(RM_ABILITY)` ⇒ 每次一组帧 ⇒ target 198 帧（52×49 + 657×98 + 100×49），
    obs_in 51、caster 53。
  - **15**（`btState=0`，`DefenceUp`，`src/M2Server/Actor/BaseObject.cs:2137-2147`）：首轮 `AddBuff` 出帧，其后
    48 轮 `HasBuff ⇒ TryUpdate` **早退**（不 SysMsg/不 Recalc/不 SendMsg）⇒ target 仅 6 帧
    （17 + 52 + 638 + 657×2 + 100），obs_in 3、caster 4。
  三支均 `DrainPasses=2`、`hit_cap=false`。
- 31（`MagBubbleDefenceUp`）：RNG 2 抽，各接收者 1 条 657。
- **表外 RM 已入表**：`RM_ABILITY(10051)`、`RM_CHARSTATUSCHANGED(10139)`、`RM_SYSMESSAGE(11004)`
  （见 §11 表行：C# 位置、收件人、Rust 比较方式）。

### 24. 14/15/31 接线收口（2026-11-20）

**产品面**（提交 `051f0562e` + 本笔）：

- `World::BuffApplyEvent`（`BuffApplyKind::{DefenceUp,MagDefenceUp,MagicShield}`）：只记录
  「施加事实 + 当时 `CharStatus`」；`take_buff_applies` 供宿主消费、`buff_apply_log` 是只读观测面。
  帧面由宿主按 §11 出，**不由 SM 帧反推**。
- `World::defence_up` 补 oracle 早退（`src/M2Server/Actor/BaseObject.cs:2137-2147`）：已有 `DefensePower` ⇒ 只
  `TryUpdate` 后 return（不 `CharStatus=GetCharStatus()`、不 `SysMsg`、不 `RecalcAbilitys()`、
  不 `SendMsg(RM_ABILITY)`）。范围施法 49 次循环因此只有**首组**帧（§23 更正）。
- 宿主（`bins/game-svr`）：31 支的 657 并入 `SpellFrameBuf`（`(byte)ident`：638=136 < 657=155 <
  640=255）；14/15 支护身符 642（10125&0xFF=141）并入同批 buf；随后按事件出 SM 100（仅被施加者）
  / 657×2 / 52（仅被施加者）。文案取 `MessageSettings` 源码缺省（`防御力增加{0}秒` /
  `魔法防御力增加{0}秒`；部署 `E:\MirServer\M2GameSvr\String.conf` 同串），配置面未接入
  `handle_spell_cmd`（登记）；绿色 `GreenMsgFColor/BColor = 219/255`（部署值）。

**结构性原因：本仓把 oracle 的两个函数合并了**（本次分歧的根因，写死在此）：

- oracle 登录序 `src/M2Server/Player/PlayObject.cs:1308-1309`：`CharStatus = GetCharStatus(); RecalcLevelAbilitys();
  RecalcAbilitys();`；升级序 `:5245-5246` 同两函数连调。
- `RecalcAbilitys()` 本体 `src/M2Server/Actor/BaseObject.Abil.cs:15-32`：`WAbil = Abil.Clone()`（保留旧 `HP/MP`、
  三个重量清零），**不重算等级派生字段**——它假定 `Abil` 已由本会话更早的
  `RecalcLevelAbilitys()` 写成自洽值。
- 本仓 `recalc_session_ability`（`bins/game-svr/src/lib.rs`）是这两个函数的**合并实现**
  （见其文档注释「`RecalcLevelAbilitys()` + `RecalcAbilitys()` 的单一实现」）。合并等价的前提是
  输入 `sg.equip.abil` 与 `equip.abil.level` 自洽——生产成立（DB 记录的
  `characters_ablity.MaxHP/MaxMP` 恒 0，必须靠等级派生补齐）；而旧 C# 夹具不成立。

**C# 夹具修正**（`tests/parity/csharp/MagicGolden/Program.cs` 的 `RunMagicCase`）：target 原先只设
`Abil.Level=20` + `Abil.MaxHP=100`，等于「等级 20 + `BaseObject` ctor 缺省 Abil」，于是
`RecalcAbilitys()` 克隆出**不自洽**的 WAbil（MaxHP=100 / DC=1 / MaxMP=15）。现按登录序：

```csharp
target.Abil.Level = (byte)spec.TargetLevel;
target.RecalcLevelAbilitys();   // PlayObject.cs:5254-5317：等级派生 MaxHP/MaxMP/Weight/DC/MC/SC/AC/MAC
target.WAbil.HP = (ushort)targetHp;
```

并删去手动的 `target.Abil.MaxHP = spec.TargetMaxHp`。`Abil.MaxExp` 保持 ctor 缺省 50
（登录后的 `Abil.MaxExp = GetLevelExp(...)` 属数据面，不在本夹具建模范围；Rust 夹具同置
`equip.abil.max_exp = 50`）。**`spec.TargetMaxHp` 自此只作元数据与治愈 self 支输入**，
14/15/31 的 target 实际 MaxHP = 等级派生值（20 级战士 = 224）。

**golden 重生成（逐字段；C# 窗口 2026-11-20；提交 `6e86a0c10`）**：

| 文件 | old sha256 | new sha256 |
|---|---|---|
| `golden/magic/hangmajinbub.json` | `fb2cd3c8cb54…` | `1f267540d883…` |
| `golden/magic/dejiwonho.json` | `9931589c2b04…` | `fb77993cd0a0…` |
| `golden/magic/shield.json` | `a22c2af0b759…` | **未变**（重跑仅 `delivery` 墙钟不同） |

变化面**只有** target 的 `SM_ABILITY(52)` 尾（EDCode 解出 40B `WAbil`）：hangmajinbub 每 label
49 条、dejiwonho 每 label 1 条；**其余全部不变**：帧集合与顺序（17/52/638/640/642/657/100）、
RM 多重集、RNG 序列（4 抽 / 2 抽）、`DrainPasses=2`、`hit_cap=false`、`TargetHpBefore/TargetHp`。

逐字段（old → new，20 级战士 `RecalcLevelAbilitys` 派生值，`src/M2Server/Player/PlayObject.cs:5300-5310`）：

| 字段 | old | new |
|---|---|---|
| Level | 20 | 20 |
| AC | 0 | 0x0200 |
| MAC | 0 | 0 |
| DC | 1 | 0x0403（3–4） |
| MC | 1 | 0 |
| SC | 1 | 0 |
| HP | 100 | 100（cur 值保留：`WAbil.HP = wOldHP`） |
| MP | 0 | 0 |
| MaxHP | 100 | **224** |
| MaxMP | 15 | **81** |
| MaxExp | 50 | 50（ctor 缺省） |
| MaxWeight | 100 | **183** |
| MaxWearWeight | 0 | **35** |
| MaxHandWeight | 0 | **43** |

另：`RM_MAGIC_LVEXP(10123)` 的 `delivery` 字段随墙钟变化——比较面**不含** `delivery`（§11/§19b
口径），非语义变化。无新增表外 RM。

**判定面**：

- 驱动 `tests::hangmajinbub_parity_s{1,5,13}` / `tests::dejiwonho_parity_s{1,5,13}`：target 的
  SM 52/100/657 计数（49/49/98 或 1/1/2）、caster/obs_in 657、`638→642→…→640` 顺序，以及
  `sm_ability_wabil` 把 target 52 尾解码为 40B 后**与 golden 逐字节一致**且 Level=20 /
  MaxHP=224 / MaxMP=81 / DC=0x0403——即「两侧同一登录语义」的直接断言。
- 比较器 `real_rust_ledger_hangmajinbub` / `real_rust_ledger_dejiwonho`：0 红（SM 严格序+字段+尾，
  RM 多重集）。`real_rust_ledger_shield` 继续 0 红。
- **牙 1（门关）**：`tests::shield_buff_tick_gate_off_emits_no_buff_frames`（绿）——不开
  `enable_buff_tick` ⇒ `buff_apply_log` 空、四 socket 均 0 条 657。
- **牙 2（删事件面）**：把 `take_buff_applies` 改成返回空 ⇒ hangmajinbub s1
  `target 的 SM 52 计数` 0≠49、dejiwonho s1 0≠1、shield s1 `caster 恰 1 条 SM 657` 0≠1 全红；
  `git checkout -- crates/world-bevy/src/world.rs` 复原后 sha `ad47e746…` 相等、`MUTATION`
  残留 0。证据：`mir2-rs/target/magic-parity/evidence/tooth-buff-surface-final.log`。
- **回归**：30 支驱动全 rc=0（`--ignored --exact --test-threads=1` 逐进程）、10 支比较器全过、
  既有 9 颗比较器牙全过。证据：`…/evidence/regression-1415-final.log`。

**清单**：MG-28（`src/M2Server/Magic/MagicManager.cs:352`）/ MG-29（`:359`）/ MG-43（`:474`）升 `verified`
（`basis=semantic`）；MG-21 / MG-22（`:316`/`:317` 外层护身符门组标签）保持
`implemented_unverified`——组内其余魔号未验（同 MG-20 的 firecharm 先例）。

**未做 / 登记**：`String.conf` 的 `DefenceUpTime`/`MagDefenceUpTime` 未接 `handle_spell_cmd`
（文案用源码缺省 + 部署同串，替换点一旦接上需同步 100 尾）；buff 时长的 tick 淘汰由
`enable_buff_tick` 生产线负责，本对拍夹具只显式开启、不推进淘汰窗口。

### 25. 28 心灵启示（`SKILL_SHOWHP`）：接线前口径（opc-07 批准）

清单条目 `MG-39`（`src/M2Server/Magic/MagicManager.cs:431-442`）；无外层组条目。以下三条口径先定死，
实现（golden + 驱动 + 断言）另笔。

#### (a) 排水等待：每 case 可配 `wait_ms`（默认 1200；28 用 1600）

- 依据：28 通过 `SendSelfDelayMsg(..., 1500)`（`src/M2Server/Magic/MagicManager.cs:438`）把
  `RM_DOOPENHEALTH` 排到**目标**队列；`MakeOpenHealth()` 只在到点后发生 ⇒ 不跨过 1500ms
  就观察不到 `RM_OPENHEALTH`/`SM_OPENHEALTH(1100)`，账本里只留 `NotDue`（`now/delivery` 是墙钟，
  不可比）。
- 做法：C# `RunMagicCase` 的 `Thread.Sleep` 由魔法数改为**参数 `wait_ms`**（默认 1200，28 = 1600）；
  Rust 驱动把写死的 `rig.world.clock_ms += 1500` 改为传参，**两侧推进量同值**。`wait_ms` 进 golden
  的 `Fixture`，Rust 侧直接读它复现（**不手抄**）。
- 既有 10 个 golden 不受影响（默认值不变）。

#### (b) `SM_OPENHEALTH(1100)` 的 HP/MaxHP 来源：oracle 语义 = 被开血者的**实时 WAbil**

- 锚：handler `src/M2Server/Player/PlayObject.Message.cs:2009-2011` 取
  `baseObject.WAbil.HP/MaxHP`；而 `baseObject` 在 `Operate` 开头由
  `SystemShare.ActorMgr.Get(processMsg.ActorId)` 取得（`src/M2Server/Player/PlayObject.Message.cs:774-778`）
  ⇒ 是**消息源 actor（被开血者）**，不是收件人。C# 的 `WAbil` 是该对象的**实时** HP/MP 存储
  （`HealthSpellChanged`/`IncHealthSpell`/受击均直接读写它；`BaseObject` 是所有 actor 的基类，
  怪物同样只有这一个 `WAbil`）。
- Rust 的对应物：**实体 `vitals.hp/max_hp`** 是运行时 HP 真源（伤害/回血都写它；怪物**没有**
  会话 `wabil`）。会话 `state.wabil` 是快照，由 `apply_ability_to_entity`（wabil→entity，登录/重算）
  与 `sg.state.wabil.hp = e.hp`（entity→wabil，多处同步点）保持同值。
  **⇒ 产品读实体是对的，不改产品**；反过来读 `sg.state.wabil` 会在「快照未同步」或「怪物目标」
  两种真实形态下读错（与 IT-08 的 Abil/WAbil 是同类坑，方向相反）。
- **同一结构里两种字段的「实时性」不同（别拿 IT-08 的结论套到 HP 上）**：oracle 的
  `WAbil.HP/MP` 是**实时被写**的（受击/回血/`HealthSpellChanged` 都直接读写它）；而
  `WAbil.Exp` 只在**登录/重算**时从 `Abil` 同步（`PlayObject.cs:1310-1312`）——IT-08 说的
  「Abil/WAbil 不同步」是**数据面（Exp 类）**问题，不能反向套到 HP/MP 上。
- 前提与钉法：登录后二者同源（`apply_ability_to_entity`），28 支窗口内无 HP 变化 ⇒ 恒等成立。
  夹具必须把目标**实体与 `Abil`（登录派生后的 WAbil）对齐**，并加断言
  「entity.hp/max_hp == 该 case 的 WAbil.hp/max_hp」；**不允许**用夹具对齐去掩盖产品读错对象——
  本条的结论是产品读实体正确，断言只是把前提钉死。

#### (c) 其它

- 事件面**不缺**：`World::cast_show_hp_spell` 返回 `ShowHpOutcome`（门/抽/训练）、
  `make_open_health` 返回 `OpenHealthDelivery`，宿主 `drain_b3_self_deliveries` 已出 1100/640
  （`SM 1101` 的到期路径属 `openmir2-sm-closehealth-wire` 卡，不在本支）。
- 骰与标签：门 = `Random(6) <= userMagic.Level + 3`（`src/M2Server/Magic/MagicManager.cs:434`；
  非 AntiMagic 骰），按 **miss(roll=5)/edge(roll=4)/hit(roll≤3)** 三标签；seed 用
  `FindSeedForFirstDraw(6, k)` 扫描（C# helper 由 `Random(10)` 版参数化）。
- `GetRPow(SC)` 在 `HiByte<=LoByte` 时**不抽**（`src/M2Server/Magic/MagicManager.cs:121-131`）；
  夹具 `SC=15` ⇒ pass 支只多 `GetPower13` 的 `Random(0)` 一抽；`boTrain=true` ⇒ 共享尾训练抽 `Random(3)+1`。

#### (d) 实现记录（2026-11-20，提交 `fadb5db5e`）

- **C#**：`RunMagicCase` 等待改 `spec.WaitMs`（默认 1200，28 = 1600，写进 `Fixture.wait_ms`）；
  新增 `ShowHpBatch`（`FindSeedForFirstDraw(6, {5,4,3})` ⇒ miss/edge/hit，seed 6/2/13）；
  登录序补最后一步 `target.RecalcAbilitys()`（把 `Abil` 物化到 `WAbil`）+ 装 NetChannel 前
  `DrainRaw()` 清登录帧。**红证**：去掉这一步时 28 的 `SM_OPENHEALTH(1100)` 的 MaxHP=0
  （RM 10410 的 p2 同）——证据 `mir2-rs/target/magic-parity/evidence/showhp-fixture-red.log`。
- **golden `golden/magic/showhp.json`**：miss(RNG `[5]`, 2 趟) / edge(`[4,0,0]`, 3 趟) /
  hit(`[3,0,2]`, 3 趟)，`hit_cap=false`。edge/hit：caster `[638,640,1100]`、
  target `[17,638,1100]`、obs_in 同 target、obs_out 空；1100 的 param=100(HP)/tag=224(MaxHP)；
  RM = 10008/10120（众人）+ 10123(仅 caster,255,late) + 8040(仅 target,255,late) + 10410(众人,170)。
- **Rust 驱动**：交付段改为**按 due 时刻分步**（步长 100ms、总量 = golden `Fixture.wait_ms`），
  每步按 `sim_loop` 同序 B4→B3 —— 28 的 640@1000ms 必须先于 1100@1500ms；一次跳满会让同一
  `SpellFrameBuf` 按优先级把 1100(170) 排在 640(255) 前（牙 B 实测 `[638,1100,640]`）。
  RM 8040/10410 由内部事件 `SelfDelayOutcome.open_health` 组装（不由 SM 帧反推）；28 的目标
  实体/WAbil 对齐 HP=100/MaxHP=224，并在驱动内断言「实体 HP/MaxHP == WAbil HP/MaxHP」。
- **牙**（`…/evidence/tooth-showhp-final.log`）：A 删 B3 调用 ⇒ `caster 恰 1 条 SM 1100` 0≠1；
  B 一次跳满 ⇒ `caster 帧序` `[638,1100,640]`≠`[638,640,1100]`；C 实体 max_hp 置 100 ⇒
  前提断言 (100,100)≠(100,224)。三颗复原后 `lib.rs` sha 相等、无 `MUTATION` 残留。
- **回归**：33 支驱动全 rc=0（`…/evidence/regression-showhp-1.log`）、11 支比较器 0 红、
  既有 9 颗比较器牙全过。**分步交付对既有 10 条 verified 的台账无影响**（同批回归已证）。

### 26. 18 隐身术（`SKILL_CLOAK`）接线记录（2026-11-20）

- **oracle**：组门 `CheckAmulet(1,1)`（`:315-327`，与 13/14/15 同组）→ `:378-383`
  → `MagMakePrivateTransparent`（`:673-699`）：先置 `StatusTimeArr[STATETRANSPARENT]`、
  `CharStatus = GetCharStatus()`、`StatusChanged()`，**再**置 `HideMode=true; Transparent=true`。
- **观测面（新增，只记录）**：`crates/world-bevy/src/world.rs` 的 `StatusChangeEvent` +
  `World::status_changes()/clear_status_changes()`；生产点 = `cast_private_transparent_spell`
  （`d.status_changed` 时压一条）。**生产无行为变化**：宿主帧面仍走该函数的返回值；
  **有界**：`sim_loop` 每 tick 清空一次。对拍驱动施法后取一次即清。
- **golden `golden/magic/cloak.json`**（三 seed s1/s5/s13，无 AntiMagic 骰；`DrainPasses=2`）：
  caster `[638,642,657,640]`、target/obs_in `[17,638,657]`、obs_out 空；RNG `[0,0]/[0,0]/[0,1]`
  （`GetPower13(30)` 的 `Random(0)` + 训练尾 `Random(3)+1`），RM = 10008/10120（众人）+
  10125(642,141)/10139(155)/10123(慢 255)（caster）。
- **对领导 (b) 要求的数据订正**：要求写「657 的 param 含 `STATETRANSPARENT`」，但 **oracle
  实测 657 的 `(param,tag,series) = (0,0,0)`、RM 10139 的 p1 = 0**——`GetCharStatus()` 不含
  `STATETRANSPARENT`（隐身走 `HideMode/Transparent` 字段，不进 `CharStatus`）。断言据此改为
  「657 在 638 之后 + caster 仍收 638（隐身不影响本次广播）+ 657 字段与 golden 逐项相等」；
  牙 B 实测把 p1 改成 `0x80000000` 会红，证明该字段确实在比较面内。
- **(c) 怪循环**：`MagMakePrivateTransparent` 的 9 格扫描只对 `Race>=Animal &&
  TargetCret==caster` 的怪抽 `Random(2)`；**本夹具无怪 ⇒ 零抽**。将来若给夹具加怪观察者，
  RNG 流会平移、账本变化——不得当作回归。
- **(d) 不开 `enable_buff_tick`**：18 的 `StatusTimeArr[STATETRANSPARENT]` 由 `Run`/毒计时递减；
  开 buff tick 可能在采集窗口内触发额外 `StatusChanged()`/657。夹具（`fireball_parity_drive`）
  只对 14/15/31 开，18 保持关闭。
- **牙**（§16 流程：快照 `E:/UserTemp/cloak-snap/` + sha；`cp` 还原 + sha 复核 + 基线复跑）：
  A 删除观测面生产 ⇒ driver `caster 恰 1 条 RM 10139` 0≠1、比较器 3 条红；
  B `char_status ^= 0x80000000` ⇒ 比较器 3 条红（p1 −2147483648 vs 0）。
  证据 `mir2-rs/target/magic-parity/evidence/tooth-cloak.log`。
- **回归**：36 支驱动全 rc=0（`…/evidence/regression-cloak-1.log`）、12 支比较器 0 红、
  9 颗比较器牙全过。
- **清单**：MG-32（`:378` body）升 `verified`；外层组标签 MG-25（`:320`）保持
  `implemented_unverified`（组内其它魔号未验）。
- **衍生卡**：战士 9 技（3/4/7/12/25/26/27/34/38）不能用 `DoSpell` 直调产 golden——见
  `docs/issues/openmir2-warrskill-entry.md`；补 `ClientSpellXY` 入口前禁止标 verified。

### 27. 29 群体治愈术（`SKILL_BIGHEALLING`）接线记录（2026-11-20）

- **oracle**：`src/M2Server/Magic/MagicManager.cs:443-449`（`nPower = GetAttackPower(GetPower(MPow) +
  Lo(SC)*2, (Hi-Lo)*2+1)`）→ `MagBigHealing`（`:54-76`）：`GetMapBaseObjects(nX,nY,1,...)` 的
  **3×3 有序枚举**，对 `IsProperFriend && WAbil.HP < WAbil.MaxHP` 的每个对象
  `SendSelfDelayMsg(RM_MAGHEALING, 0, nPower, 0, 0, "", 800)`（prio 255、late）。
- **夹具几何（钉死）**：点击 (101,100) 的 3×3 = (100..102)×(99..101)，其中在线玩家只有
  **caster(100,100) 与 target(101,100)** ⇒ friend 数 **N=2**；`heal` 只治 target（caster 满血）
  ⇒ 8034 条数 = 1；`both`（施法者 50/100 + 目标 50/224）⇒ 条数 = **2**（覆盖 (d) 的枚举序/条数）。
- **(a) 每 case 目标 HP 参数化**：rig 的 hp 不再写死——驱动从 golden `Fixture.target_hp`
  （缺省 100）与 `caster_hp/caster_max_hp`（`0` = 不设）读取；登录序下目标 `MaxHP = 224`。
  三案：`heal`(50/224)、`full`(224/224)、`both`(目标 50/224 + 施法者 50/100)。
- **(b) 口径（写死）**：**本支窗口内不回血、不产 53**——`RM_MAGHEALING` 到点只累计
  `IncHealing/PerHealing`（`src/M2Server/Player/PlayObject.Message.cs:1619-1629`），实际回血在后续
  `Run` 的恢复周期。判据 = RM 8034 的**条数/载荷**（p1=nPower=30）+ RNG 抽数（heal/both 5、
  full 4）+ 帧集合（caster `[638,640]` / `[638]`，target/obs_in `[17,638]`）。
- **(c) `AbilSeeHealGauge`(1103) 显式未覆盖**：`UserMagic.Level=1` 恒 false ⇒ 本支 verified 的
  覆盖声明**不含** 1103 分支（要覆盖需另立 Level≥2 的 case 与 §11 的 RM_ABILSEEHEALGAUGE 行）。
- **golden `golden/magic/bighealing.json`**：`DrainPasses=2`、`hit_cap=false`；RM 10008/10120（众人）
  + 8034（被治者各一条）+ 10123（caster，慢 255；仅 heal/both）。
- **牙**（§16 快照：`E:/UserTemp/bighealing-snap/`，world.rs `ef61320b…`、lib.rs `8a9014d8…`）：
  A 不组装 8034 ⇒ driver `target 的 8034 条数` 0≠1 + 比较器红；B 产品条件 `hp < max` 改 `<=`
  ⇒ full 支多出 8034/640（比较器 5 条红）；C 驱动忽略 golden `target_hp` ⇒ `目标 HP 前`
  100≠50/224 红。三颗均 `cp` 还原、sha 相等、基线复跑绿。
  **过程披露**：牙 A 首跑用 `std::iter::empty::<>()` 作变异体，**编译失败**；当时比较器读的是
  **陈旧台账**（驱动没跑成 ⇒ 没重写 ledger）⇒ 一度假绿。已按「编译失败不作判据、变异后必须重跑
  驱动再看比较器」改正为 `&maghealing_msgs[..0]` 的有效变异。证据
  `mir2-rs/target/magic-parity/evidence/tooth-bighealing.log`。
- **回归**：39 支驱动全 rc=0（`…/evidence/regression-bighealing-1.log`）、13 支比较器 0 红、
  9 颗比较器牙全过。
- **清单**：MG-40 升 `verified`（magic 13/65）。

### 28. 空转防线：台账 `run_id`（2026-11-20，应 opc-07 要求并入 29 支）

**触发**：29 支的牙 A 首跑用了一个**编译不过**的变异体 ⇒ 驱动根本没运行 ⇒ 比较器读到的是**上一次
运行留下的台账**，一度判绿。这类「驱动没跑、比较器照绿」的空转必须由机制拦死。

**纪律（写死）**：**编译失败不作判据**；**每次看比较器前必须确认台账来自本次驱动**——靠下面的
`run_id` 机制自动证明，不靠人眼。

**机制**：
- 环境变量 `MIR2_LEDGER_RUN_ID`（建议「时间戳+随机串」），**驱动与比较器用同一个值**；
  未设 ⇒ 两边都 **fail-closed 红**。
- 驱动每个 case 在台账旁落侧车 `<stem>_<label>.run.json`（`{"run_id","seed"}`）——通用驱动
  `fireball_parity_drive` 与治愈专用驱动 `healing_parity_drive` 都写。
- 比较器 `real_ledger_verify_labels`（`mir2-rs/tests/parity/tests/magic_fireball_parity.rs`）：
  侧车缺失 / `run_id` 不符 / 未设环境变量 ⇒ 红，错误串点名「台账陈旧，run_id 不匹配」。
- 命令范本：
  ```sh
  export MIR2_LEDGER_RUN_ID="mp-$(date +%s)-$$"
  cargo test -p game-svr --lib tests::<drive> -- --exact --ignored --test-threads=1   # 逐支
  cargo test --manifest-path tests/parity/Cargo.toml --test magic_fireball_parity -- --ignored
  ```

**牙 D（证据 `mir2-rs/target/magic-parity/evidence/tooth-runid.log`）**：
- D1 **不跑驱动**、用不同 id 跑比较器 ⇒ `[miss] 台账陈旧：run_id 不匹配（当前环境 = …）` 红；
- D2 不设环境变量 ⇒ `未设置 MIR2_LEDGER_RUN_ID…` 红（fail-closed）；
- D3 删掉侧车 ⇒ `防空转：台账缺 run.json 侧车 …` 红；
- 恢复后同 id 基线复跑绿。

**回归（同一 id `mp-1795121294-1416322`）**：39 支驱动全 rc=0、13 支比较器 0 红、9 颗比较器牙全过
（`…/evidence/regression-runid.log`）。**注意**：§27 及更早的证据日志产生于本机制之前，命令行未带
`MIR2_LEDGER_RUN_ID`；此后所有驱动/比较器运行一律按上面范本执行。

### 29. 19 集体隐身术（`SKILL_BIGCLOAK`）接线记录（2026-11-20）

- **oracle**：组门 `CheckAmulet(1,1)`（`:315-327`）→ `:384-389` → `MagMakeGroupTransparent`
  （`:1209-1228`）：`GetMapBaseObjects(nX,nY,1,...)` 的 **3×3 有序枚举**，对
  `IsProperFriend && StatusTimeArr[STATETRANSPARENT]==0` 的对象
  `SendSelfDelayMsg(RM_TRANSPARENT, 0, nHTime, 0, 0, "", 800)`；到点由该对象
  `MagMakePrivateTransparent` ⇒ `StatusChanged()` ⇒ 657 广播（含自）。
- **夹具几何**：点击 (101,100) ⇒ 3×3 内 caster(100,100)+target(101,100) 两个 proper friend
  各一条 8045；obs_in(105,100) 不在 3×3。RNG：`GetPower13(30)` 的 `Random(0)` 1 抽 +
  训练尾 `Random(3)+1` ⇒ **2 抽**；到点 9 格怪循环零抽（无怪）。
- **C# 装置新增「步进排水」**（`MagicSpec.StepDrainMs`，默认 0 = 旧口径；19 用 100ms）：
  旧口径「单次 `Sleep(WaitMs)` 后一次排空」会按 4 叉堆序把 1000ms 的 640 排到 800ms 的 657
  **前面**（记录面伪影）；步进排水只在**到期时刻**处理（未到期按原优先级放回队列），与真机
  每 tick `Run()` 及 Rust 分步交付同序 ⇒ caster `[638,642,657,657,640]`。既有 golden 全部
  未重生成、不受影响。
- **顺序取舍（opc-07 2026-11-20 裁定；QA 复核后订正措辞）**：golden 与比较器**不记录 `Recog`**（`ParseFrames` 只取 `ident/param/tag/series/tail`）；19 的两条 657 **仅 `Recog` 不同**（`SendDefMessage(SM_CHARSTATUSCHANGED, processMsg.ActorId, …)`，`src/M2Server/Player/PlayObject.Message.cs:1845`；`ActorId = baseObject.ActorId`，`src/M2Server/Actor/BaseObject.Message.cs:149-154`）⇒ **按总监裁定不判先后**——这是**我们的取舍**（不扩比较面），**不是**「不可判」，也**不是**「对客户端无差异」。判定面 = 每个 socket 内 657 的**条数（各 2）** + 字段逐项
  相等 + RM 多重集。**纪律：这是「不判」，不是「顺序相等」。**
- **golden `golden/magic/bigcloak.json`**：`DrainPasses=3`、`hit_cap=false`、`NotDue=0`；
  caster `[638,642,657,657,640]`、target/obs_in `[17,638,657,657]`；RM = 10008/10120（众人）
  + 10125（caster,642,141）+ 8045（caster/target 各 1，p1=95，255）+ 10139（各 2，155）
  + 10123（caster，255）。
- **(c)(d)**：隐身位不进 `CharStatus`（657 全 0，§26 已钉）；`hear_targets` 只看
  `ob_mode||fixed_hide_mode` ⇒ 隐身不影响后续 640；`StatusTimeArr` 置位后重复施加走「已隐身」
  分支（单次 cast 不触发）；夹具无怪 ⇒ 怪循环零抽。
- **牙（§16 快照 `E:/UserTemp/bigcloak-snap/`，world.rs `ef61320b…`、lib.rs `c737d13c…`）**：
  A 不组装 8045 ⇒ driver `caster 8045` 0≠1 + 比较器红；B 不组装到点的 10139 ⇒ driver
  `caster 的 RM 10139 条数` 0≠2 + 比较器红；C 产品早退门 `>0` 改 `>=0`（恒早退）⇒
  caster 帧序 `[638,642,640]` ≠ `[638,642,657,657,640]`。三颗均 `cp` 还原、sha 相等。
  **附加观察（run_id 防线实战）**：牙后只重跑 s1 时，比较器因 s5/s13 的侧车仍是上一 id 而红
  （`[s5] 台账陈旧：run_id 不匹配`）——这正是 §28 机制要拦的空转；同 id 重跑三支后基线绿。
  证据 `mir2-rs/target/magic-parity/evidence/tooth-bigcloak.log`。
- **回归**：42 支驱动全 rc=0、14 支比较器 0 红、9 颗比较器牙全过（同一 run id，
  `…/evidence/regression-bigcloak.log`）。
- **清单**：MG-33（`:384` body）升 `verified`（magic **14/65**）；MG-26（`:321` 外层组标签）
  保持 `implemented_unverified` 并补说明。

### 30. 23 爆裂火焰（`SKILL_FIREBOOM`）接线记录（2026-11-20）

- **oracle**：`src/M2Server/Magic/MagicManager.cs:418-423`（`MagBigExplosion(GetAttackPower(GetPower(MPow)
  + Lo(MC), ΔMC+1), nTargetX, nTargetY, FireBoomRage)`）→ helper `:1100-1117`：对 3×3
  （部署 `FireBoomRage=1`）内 `IsProperTarget` 的对象 `SendMsg(baseObject, RM_MAGSTRUCK, 0, nPower, 0, 0)`
  并 `SetTargetCreat(target)`（只改施法者）。
- **(a) `IsProtectTarget` 以 golden 实况为准**：golden 显示 **target HP 100→85（damage=15）**、
  `SM_STRUCK(31)`/53/656 齐全 ⇒ 本夹具下 `IsProtectTarget` 实为 **true**、伤害链成立；`IsProperTarget`
  自排除（`PlayObject.cs:1926-1928`）⇒ **单目标**（obs_in 只是观察者）。
- **golden `golden/magic/fireboom.json`（s1/s5/s13）**：`DrainPasses=4`、`Rng` **7 抽**；
  caster `[638,640,31,656]`、target `[17,638,53,31,656]`、obs_in `[17,638,31,656]`；
  RM = 10008/10120（众人）+ 8044(target,p1=15,108)+10052(target,68)+8103(众人,
  w=15/p1=85/p2=224/p3=caster,167)+8090(攻击方 hearers,154)+10123(caster,255)。
- **C# 装置两处改进（只影响新 golden）**：
  1. **`Rng` = `DoSpell` + 排水的全部抽取**（RngSeam 直到排水结束才还原）：23 的 `RM_MAGSTRUCK`
     是**即时**投递，`R(1000)`/MSD/STD 抽发生在排水 `Operate` 里；旧口径在 `DoSpell` 后就还原，
     这些抽会落到未播种 RNG ⇒ 金标准不可复现、Rust（施法段内同步结算）无法对齐。
  2. 宿主 **两段出帧**（代码条件 `if req.magic_id == 24 { 旧合并 flush } else { 两段 }`——
     当前可达集合只有 {23,24,33,47}，故等价于「24 旧、23/33/47 两段」，但**卡面按代码事实写**）：
     尾帧（638/640）先 flush，受击帧（53/31）再入独立缓冲 flush；
     否则同一 `SpellFrameBuf` 按 `(byte)ident` 会把 53(68) 排到 638(136) 前面（golden 相反）。
- **`struck_log` 观测面（新增，只记录）**：`World::struck_log()/clear_struck_log()`；生产点
  `deliver_mag_struck`；宿主 `sim_loop` 每 tick 清空（有界）；驱动施法后与交付后各取一次并清。
  **零行为证据**：既有 45 支驱动全绿（含 13 条 verified 的 golden 逐字节不变）、15 支比较器 0 红、
  9 颗比较器牙；`game-svr --lib` 1996 / `world-bevy` 878。
- **`8090` 组装改为内部事件**：原先「看是否发出 `SM_CHANGENAMECOLOR(656)` 帧」的反推式组装**删除**，
  改用 `StruckEvent.hitter_name_color`（即时支 `struck_cast` / 延迟支 `struck_delivered`），
  符合「RM 必须来自内部事件」的口径。
- **牙（§16 快照 `E:/UserTemp/fireboom-snap/`，world.rs `09c59a33…`、lib.rs `e95da05a…`）**：
  A2 把 `MagBigExplosion` 的目标门改成全部排除 ⇒ 三支 driver `目标 HP` 100≠85、比较器 24 条红；
  B 不组装 8044/10052/8103 ⇒ driver `target 8044` 0≠1、比较器 3 条红。均 `cp` 还原、sha 相等、
  三支驱动 + 比较器基线复跑绿。**如实登记两次无效判据**：① 首版牙 A「range→0」无效——点击格就是
  目标格，1×1 仍含目标，driver 照常通过；② 同一轮只重跑 s1，比较器的红是 **run_id 陈旧**（§28 机制），
  不是变异造成的红；均已在证据里标注并按有效变异重跑。证据
  `mir2-rs/target/magic-parity/evidence/tooth-fireboom.log`。
- **回归**：同一 run id 下 45 支驱动全 rc=0、15 支比较器 0 红、9 颗比较器牙全过
  （`…/evidence/regression-fireboom.log`）。
- **范围与登记（两处边界，写死）**：
  1. **出帧分支的代码事实**：条件是 `if req.magic_id == 24 { 旧合并 flush } else { 两段出帧 }`；
     当前可达集合 = {23,24,33,47} ⇒ 等价于「24 旧、其余两段」。24（`MagElecBlizzard`）的 31
     同优先级 tie 序目前只有 Rust 单测钉着、没有 oracle golden，**留待 24 自己的 golden 统一定序**。
     **将来新增即时受击魔号时需复核该条件**（新魔号默认落进两段支——若它与 24 同构则应显式纳入判断）。
  2. **多目标 tie 序无 golden 可钉**：`spell_p3b2_fireboom_delivery_ledger_and_frames` 的两处
     断言已按新模型改写并写明依据——① `53` 改为在 `638` 之后（旧期望是「尾帧+受击帧同批
     flush」的实现序，oracle 是后续趟处理受击）；② 同优先级 31 的 tie 序由 `NetPriorityQueue`
     两段模型的出队序给出（`[p1,m3,m2,m1]`），**不是** oracle 实证（本支多目标无 golden）。
- **清单**：MG-37 升 `verified`（magic **15/65**）。

## 21. M52 迁移清单（15 条逐条实测；应 QA 条件① 追加，2026-11-20）

> 目的：M53 更换入库件前，把「每条 golden 的现状、迁移后形态、**实测**帧面变化、`MaxExp` 三态」逐条落盘。
> 实测口径：用 M52 装置（`FixtureContract` 七道门 + `ExpConf.LoadConfig()` 修复）对 **15 条**逐条重生成到
> 仓外 `E:/UserFleet/qa/warrskill/m52-migration/`，与入库件逐接收者逐字段比（排除 `delivery`/`now` 等墙钟）。
> 变化归因只允许三种：**登录序重算 / `rng_mode` 口径 / Exps 修复**；不含者写「无」。

| case | 现状（runner / 登录序 / Rng 口径） | 迁移后（M53） | 帧面变化（实测） | `MaxExp` 三态 | 变化归因 |
| --- | --- | --- | --- | --- | --- |
| fireball（MG-10，id=1） | `FireballCase` 独立 runner；无 `Fixture` 块；无登录序；`freeze_at_dospell` | **M52.1 改造 runner**（seam + login_order 参数）→ 登录序主件；旧件移 `golden/magic/legacy/` 标 `legacy_artifact` | **0 差异**（runner 未改造，重生成即原样；非「已对齐」） | 不涉及 | 无 |
| fireboom | `BehaviorBatch`；target 登录序；`cover_drain` | 登录序主件（带契约字段） | **0 差异** | 不涉及 | 无 |
| hangmajinbub（MG-28） | `BehaviorBatch`；target 登录序；`cover_drain` | 登录序主件；`Abil.MaxExp=GetLevelExp(Level)` | **0 差异** | **涉及且相等（50=50，非真值）**；整改后两侧同变 6000 | 无 |
| dejiwonho（MG-29） | 同上 | 同上 | **0 差异** | **涉及且相等（50=50，非真值）**；同上 | 无 |
| shield / cloak / bigcloak | `BehaviorBatch` | 登录序主件 | **0 差异** | 不涉及 | 无 |
| bighealing（MG-40 的 caster 元组） | `BigHealingBatch` | caster L30 元组改公式 419（M53 整改） | **0 差异**（现行元组；整改后以重生成实测为准） | 不涉及 | 无 |
| showhp | `ShowHpBatch` | 登录序主件 | **0 差异** | 不涉及 | 无 |
| lightning-11 | `AntiMagicBatch`；target 登录序；`cover_drain` | 登录序主件（`anti_magic=1`、`max_hp`=公式值） | **变化**：3 子 case、18 处帧面差 | 不涉及 | **登录序**（唯一变量） |
| lightning-45 | 同上 | 同上 | **变化**：3 子 case、18 处 | 不涉及 | **登录序** |
| firecharm | 同上 | 同上 | **变化**：3 子 case、18 处 | 不涉及 | **登录序** |
| hbfireball | 同上 | 同上 | **变化**：3 子 case、18 处 | 不涉及 | **登录序** |
| fireball-5 | 同上 | 同上 | **变化**：3 子 case、18 处 | 不涉及 | **登录序** |
| healing | `HealingBatch` | 登录序主件 | **变化**：3 处帧面差（1 子 case `Rng` 变） | 不涉及 | **登录序** |

**结论与后续**
- `MaxExp` 三态：仅 MG-28/29 为「涉及且相等（50=50，非真值）」，其余 12 条「不涉及」；**无「涉及但为 0」**。
- **Exps 修复对 15 条帧面 0 影响**（实测：`hangmajinbub`/`dejiwonho`/`showhp`/`bighealing` 复跑 0 差异）；
  真值 `Exps.conf Level20=6000`，整改后 MG-28/29 两侧同变 6000（**先落 `LoadConfig` 修复、再重生成**）。
- 6 条变化的唯一变量是**登录序重算**（target 的 `AntiMagic`/`WAbil.MaxHP` 被消费）；`Rng` 口径在 master 已是
  `cover_drain`（未切换）。
- `cover_drain` 的**逐 case 真实性差分**（`case | cover Rng | freeze Rng | 判定依据`）作为 **M53 交接产物**：
  在 `login_order=false` 的可达手设件上做两模式对照；相同者必须给结构性依据（无延时受击 / 排水 `Operate` 不调 RNG），
  落到符号锚——缺依据即判非法声明。
- 本表只落实测与归因，**不改变任何条目 status**；入库件替换与比较器收紧归 M53。
## 22. M52.1：`fireball` 的 seam 证据（应 QA 条件②）

- **装置**：`FireballCase(rngMode, loginOrder)` + `--fireball-rng-mode/--fireball-login-order`；cover 路径的
  `draws` 捕获与 `RngSeam.Restore` **位于排水循环之后**（`DRAIN_PASSES` 行紧后），freeze 在 `DoSpell` 紧后收尾。
  **返工记录（2026-11-20，QA 实测）**：首版误把 cover 块放在 `Thread.Sleep(700)` 与排水循环之前 ⇒
  `QA_DRAIN_PROBE draws=5 total=5`（排水窗口抽取未经 LoggingRandom，cover 为空声明）；本笔已移到排水之后，
  探针改为注入**真实排水阶段**（`Thread.Sleep` 之后、循环之前）——牙②方向改为"若把捕获/Restore 提前 ⇒ 探针红"。
  `login_order=true` 时：`RecalcLevelAbilitys()` → `Abil.MaxExp = GetLevelExp(Level)`（冻结表）→
  `RecalcAbilitys()` → `WAbil.HP = MaxHP` → `DrainRaw()`。
- **记录面**：`LoggingRandom.TotalDraws`（不被 `Restore` 清零）；golden 记 `rng_total_draws` 与 `rng_recorded`；
  生成器断言 cover 下 `Rng.Count == rng_total_draws`（不等 ⇒ rc=2）。
- **实测事实（2026-11-20 返工后）**：火球在**真实排水窗口**确有抽取——cover 记 **8** 次、freeze 只记 **5** 次
  （`cover=5,0,0,0,0,0,0,9` vs `freeze=5,0,0,0,0`；探针 `cover=8→9 / freeze=5→5`）⇒ 两模式**可区分**。
  首版观察到的"5 次全在施法期、两模式相同"是 **cover 块误置于排水之前**导致的**空声明**，不是火球的真实性质。
- **证据（自测探针，不进 golden）**：在**真实排水阶段**（`Thread.Sleep(700)` 之后、排水循环之前）注入一次抽取——
  cover 记到（`8→9`）、freeze 不记（`5→5`）；变异 T2（把捕获/Restore 提前到排水前）⇒ `cover_caught=False`、
  自测红；还原后逐字节相等（`git hash-object` == `HEAD:<path>`）、复绿。QA 侧独立探针（自带 `Random(1000)`，
  不用本装置的 `probeDrainDraw`）复测：`cover draws=9 total=9`、`freeze draws=5 total=5`。
- **MaxExp 门两例**：设 `GetLevelExp(20)=6000` ⇒ 绿；留 ctor `50` ⇒ rc=2。
- 旧合成件（`login_order=false` + `freeze_at_dospell`，`anti_magic=3`/`target_max_hp=100`）可生成，
  标 `legacy_artifact`；登录序主件（`anti_magic=1`、`max_hp=224`、`rng_total_draws=8`，三例均 8）可生成。

## 31. 33 冰咆哮（`SKILL_SNOWWIND`）接线记录（2026-11-20，已重取）

- **oracle 分派（只认魔号，不按名字）**：`src/M2Server/Magic/MagicManager.cs:489-494`
  的 `case SKILL_SNOWWIND` → `MagBigExplosion(…, SystemShare.Config.SnowWindRange)`；
  helper `:1100-1117`。判据全部来自 33 自己的 golden（不沿用 23 的结论）。
- **范围来源落进夹具**：部署 `E:\MirServer\M2GameSvr\Server.conf`（sha256 `ce5e3ce9…`）
  的 `SnowWindRange=1`；golden `Fixture` 记录 `snow_wind_range`、`fire_boom_rage`、
  `elec_blizzard_range` 与 `server_conf_sha256`。
- **实测（33 自己的 golden）**：target **HP 100→85（damage 15）**⇒ 本夹具
  `IsProtectTarget` 为 true；`Rng` = **7 抽**（`[0,0,0,7,1,0,3]`）、`DrainPasses` = 4、`NotDue` = 0；
  caster `[638,640,31,656]`、target `[17,638,53,31,656]`、obs_in `[17,638,31,656]`；
  RM：8044(target,p1=15,108)/10052(target,68)/8103(众人,w=15/p1=85/p2=224/p3=caster,167)/
  8090(154)/10123(caster,p1=33,255)、10008.p3=33；golden = `golden/magic/snowwind.json`。
- **`DeliverRng` 口径（统一判据下实测）**：33 的受击抽发生在**施法段内**，已并入主 `Rng`（7 抽）
  并被 `verify` **逐值比较**；台账 `DeliverRng` 实测 = `[]`，按统一判据 `golden.Rng == rust.Rng
  ++ rust.DeliverRng` 成立（7 == 7 + 0）。本支**不**保留任何"DeliverRng 必须为空"的特例断言
  （`5ef8c923c` 已撤回）。比较器收紧（含 `rng_total_draws` 等式）另案（M57）。
- **重取证据（同一 `run_id`）**：`MIR2_LEDGER_RUN_ID=magic33-retake-20261120T0625` 下
  `cargo test -p game-svr --lib -- --exact --ignored tests::snowwind_parity_s1/s5/s13`
  三支逐个 rc=0（1 passed each）；`cargo test -p mir2-parity-tests --test
  magic_fireball_parity -- --exact --ignored real_rust_ledger_snowwind` rc=0；
  三份侧车 `<stem>_<label>.run.json` 均记该 run_id。负对照：换 id（`WRONG-run-id-negctl`）
  比较器 rc=101，报「台账陈旧：run_id 不匹配」。
- **全平面回归（同一 run_id）**：`MIR2_LEDGER_RUN_ID=magic-plane-1795127273-1441175` 跑
  `tools/parity/rerun_magic_plane.sh`（驱动 `--ignored --exact` 逐支 + 比较器 `--ignored`，同一 run_id；脚本按 `--list` 自动取驱动清单并做 run_id 侧车校验）⇒ 49 个
  `test result: ok`、0 failed、rc=0（`evidence/regression-33-plane-final.log`）。
- **门禁（红→绿留痕）**：首跑 `cargo clippy --workspace --all-targets -- -D warnings` **rc=101**
  —— `doc_lazy_continuation` 两处（`bins/game-svr/src/lib.rs:67718/67719`：列表项后的
  「范围来源」段缺空行；`evidence/33-clippy.log` 原文保留）。修复 = 加一行 `///` 空行；
  复跑 rc=0（`evidence/33-clippy-final.log`）。`cargo fmt --all -- --check` rc=0。
- **帧族与 C2 交集**：本支帧族（53/638/656/31）与 `push_broadcast` 缓存接线相交，
  **`2f` 的 C2 落地后必须重跑全平面**。
- **MG-45 升格（已写入清单）**：`status=verified`、`basis=semantic`、
  `test=real_rust_ledger_snowwind`、`fixture=golden/magic/snowwind.json`、
  `cases=s1/s5/s13`、`unit_tests=tests::snowwind_parity_s1/s5/s13` ⇒ magic 域 16/65。

## 23. M57 前序：`RunMagicCase` 的 `Fixture` 输入语义（2026-11-20）

- `RunMagicCase` 的四个**驱动输入**（`target_level`/`target_hp`/`target_max_hp`/`anti_magic`）在
  「target 构造完成、RNG/`DoSpell` 之前」捕获并写入 `Fixture`；术后状态（受击/治疗后的 HP 等）只在
  结果顶层 `TargetHp`，**不得**回写成 `Fixture` 输入。
- 缺陷史：合入 M52 的装置曾在排水循环之后取 `target.WAbil.HP`（术后值）；旧世代 golden 记的是输入（100）。
  本笔改为施法前输入；附 `--case runmagic-input-selftest`（**`lightning-11` 的 `hit`（seed 13）：输入 100 →
  术后 85，断言 `Fixture.target_hp == 100`**）——若捕获再被挪到排水后，该用例必红。
- **为什么换样本（可证伪性留痕）**：最初用 `healing/heal`（输入 50），实测**术后 HP == 输入 50** ⇒ 在"把捕获
  挪到排水后"的变异下断言仍成立、**牙咬不住**；故改用术后确实变化的雷电 hit。自测样本必须按"能否证伪捕获点
  错位"来选，而不是按"看起来相关"。

## 24. `card/golden-params-4`：A/B/C 三处装置缺陷（2026-11-20，`2d` 15 条重生成的前置）

`2d` 在重生成 15 条 golden 时发现装置三处缺陷，均落在 `MagicGolden/Program.cs`（仓库自留的 oracle 侧
装置，非 `src/**` 冻结面）；本笔只修这三处，不改 golden 内容（golden 由 `2d` 统一重生成）。

### A. `RunMagicCase` 缺 `rng_total_draws` 与 cover 完整性守卫

- `Fixture` 新增 `rng_total_draws`（seam 实例累计抽取数；排水后、`RngSeam.Restore` 前取值）与
  `rng_recorded`（`Rng.Count` 快照）。`RunFireball` 早已有这两键，本笔只补齐 `RunMagicCase`。
- 新增 `EnsureCoverAccounting`：`rng_mode=cover_drain` 时要求 `rng_recorded == rng_total_draws`，
  否则抛 `FixtureContractViolation` ⇒ 顶层捕获打 `RED FIXTURE_CONTRACT`、rc=2。
  `freeze_at_dospell` 是 legacy 口径（本来漏记排水期抽取），只记两值不判等。
- 守卫咬的是**捕获点错位**（M52.1 的失效模式：`draws` 在排水前取快照、`totalDraws` 在排水后读）。
  正确代码里两者相邻同值，一旦有人把捕获挪到排水之前就立刻不等。
- **牙（实测，本 worktree）**：把 `RunMagicCase` 的 `draws` 改成「`DoSpell` 后立即取快照」
  （`drawsEarly`）⇒ `--case lightning11` rc=2：
  `RED FIXTURE_CONTRACT [runmagic/lightning-11/edge] cover_drain 记账不完整：Rng.Count=5 != rng_total_draws=8`
  （该支排水期确有 3 次抽取）；`miss` 支无排水抽取、保持绿——守卫只在真有排水抽的支上发声。
  还原（`cp` 快照；`sha256` 与快照相等、无 `MUTATION` 残留）后四支自测复绿。

### B. `AntiMagicBatch` 的种子门槛必须用**生效** `AntiMagic`

- 缺陷：`AntiMagicBatch` 用 `spec.AntiMagic`（缺省 3）选种子，但 `RunMagicCase` **恒走登录序**
  （`RecalcLevelAbilitys(); RecalcAbilitys();`），而 `RecalcAbilitys()`（`BaseObject.Abil` 同名方法）
  无条件 `AntiMagic = 1` 再 `+= AddAbil.AntiMagic`；本装置 `Equipment` 恒空 ⇒ 生效值 = 1。
  于是三支首抽（2/3/5）在门槛 1 下**全是 hit**，miss/edge 丢失。
- **选择与理由**：**不改 `MagicSpec.AntiMagic` 缺省**。它只是登录前的构造值（登录序里必被覆盖），
  改缺省只是让巧合成立；一旦引入装备或登录序变化又会静默退化。改为显式常量
  `LoginOrderAntiMagic = 1` 作门槛，并在 `RunMagicCase`/`RunFireball` 用 `CheckBranchThreshold`
  与**实测** `target.AntiMagic` 断言相等——预测错了就生成失败，不会静默产出退化用例。
- 三支首抽语义（`Random(10) >= AntiMagic` 才通过；11/45/13/44 与火球同式）：
  miss = 首抽 < 门槛、edge = 首抽 == 门槛、hit = 首抽 > 门槛 ⇒ 门槛 1 下 **0 / 1 / ≥2**。
- 同一缺陷在 `FireballCase`（`login_order=true`）同样存在（构造期 `antiMagic=3` 被登录序覆盖成 1），
  按同一修法处理：`effectiveAnti = loginOrder ? LoginOrderAntiMagic : antiMagic`。
- **牙**：把 `RunAntiMagicTriplet` 的门槛改回 `spec.AntiMagic` ⇒ `--case lightning11` rc=2，
  `种子门槛与生效 AntiMagic 不符：门槛=3 实测 target.AntiMagic=1`；分支判定本身也有牙
  （自测里把 `miss` 支的首抽当成 1 ⇒ 违规）。

### C. `Fixture` 增 `seed_reason`（逐支写）

- 每支写明种子来历，例如 `首抽=0(seed 14)，门槛 anti_magic=1（骰 Random(10) >= 门槛 才通过；miss）`。
  值取自**实测**首抽与实测门槛（不是硬编码文案）；`RunFireball` 与 `RunMagicCase` 产物同带。

### 自测与回归（本笔实测）

- 新 `--case antimagic-selftest`：`lightning-11` 三支在生效 `AntiMagic=1` 下首抽 **0 / 1 / 3**
  （seed 14 / 18 / 5），逐支断言 `fixture.anti_magic==1`、`seed_reason` 含门槛、
  `rng_recorded==rng_total_draws`；另钉 A/B 三颗牙的负例（伪造 3 != 5、相等放行、freeze 放行、
  miss 首抽=门槛、实测门槛不符）。rc=0，`ANTIMAGIC_SELFTEST OK`。
- 回归：`fixture-selftest`、`fireball-selftest`（`FIREBALL_SEAM_PROBE cover=8->9 freeze=5->5`）、
  `runmagic-input-selftest` 三者 rc=0。
- 生成面抽查：`lightning11`/`lightning45`/`firecharm`/`hbfireball`/`fireball5` 五支 rc=0
  （三支首抽落段断言全过）；`--case fireball` 三种组合（登录序 cover / 非登录序 cover /
  登录序 freeze ⇒ rc=2 门 3）行为不变。
- `dotnet build` rc=0（0 error）。
- 已知遗留（不改，交 `2d` 判断）：`--case fireball` 顶层 `anti_magic` 仍是构造参数（3），
  逐 case 的 `fixture.anti_magic` 才是生效值（登录序 1）；比较器若读顶层字段应按后者对齐。

## 25. `card/golden-params-5`：12 号 `healing` 的 `full` 支静默退化（2026-11-20）

- **结论：12 号 `full` 应在登录序 `MaxHP` 下构造，29 号（`bighealing` 的 `full` 传 224）已是正确做法。**
- 缺陷：`HealingBatch` 给 `full` 传的输入 HP 仍是 100，而登录序 `MaxHP=224` ⇒ `WAbil.HP < WAbil.MaxHP`
  成立 ⇒ 治疗真的发生，`full` 与 `heal` 同型（8034/640/10123 齐发，「满血不该治疗」这条玩法行为被静默吃掉）。
- 修法（一行）：`RunMagicCase(spec, "full", 5, 2000, 100, false)` → `224`。
- 修前/修后（`--case healing --out`，仓外
  `E:/UserFleet/qa/golden-params-5/healing-{before,after}.json`）：

| 支 | 修前 | 修后 |
| --- | --- | --- |
| `full` | `rng=5`；target rm 含 **8034**、caster rm 含 **10123**、caster sm 含 **640** | `rng=4`；三帧**全部消失**（10008/10120/17/638 保留：施法本身仍发生） |
| `heal` | `rng=5`，8034/640/10123 齐 | 逐字段不变（仅 `Rm[].delivery` 墙钟字段不同） |
| `self` | `rng=5`，8034/640/10123 齐 | 逐字段不变（仅墙钟 `delivery` 不同） |

- 门禁：`dotnet build` rc=0；`antimagic-selftest` / `fixture-selftest` / `fireball-selftest` /
  `runmagic-input-selftest` 全 rc=0。入库 golden 未改（`healing.json` 替换归 `2d`）。
- **补门（同日第 2 笔，应总监要求「别把 224 留成魔法数」）**：`full` 的输入 HP 不再写字面量，改为
  `FixtureContract.Recompute(spec.Job, spec.TargetLevel).MaxHp`（与 `derived.max_hp` 同一条派生路径）；
  并加生成期自检 `EnsureFullHpBranch`——调用点在 `EmitGolden` **之前**，断言
  `Fixture.target_hp == derived.max_hp`，不等 ⇒ `RED FIXTURE_CONTRACT` rc=2、**不落盘**，
  错误串点名「full 支必须整血构造」。效果：改 `TargetLevel` 时 `full` 自动跟到新 MaxHP；
  有人手改成非满血，生成器直接拒绝——「满血不治疗」分支不会再静默消失。
- **牙（实测，最终形态）**：把 `fullHp` 故意设成 `MaxHp - 1` ⇒ `--case healing` rc=2：
  `RED FIXTURE_CONTRACT [healing/full] full 支必须整血构造：Fixture.target_hp=223 != derived.max_hp=224（Fixture.target_max_hp=224；…）`；
  同次 `--out` 的目标文件**未创建**（`ls` 不存在）。还原（`cp` 快照、`sha256` 相等、无 MUTATION 残留）后复绿，
  `healing-after-final.json` 与派生前的 `healing-after.json` 除墙钟 `delivery` 外逐字段相同。
- **初版自证无牙（留痕，未入库）**：第一版把门槛复用构造用的 `fullHp` —— 「传什么就比什么」，门恒真；
  变异 `MaxHP-1` 仍 rc=0 并落盘。改为在门内**从 spec 等级现算** `derived.max_hp` 后才咬住。
  教训：自检的门槛必须独立于被检对象的来源，否则只是同义反复。

### 32. 47（`SKILL_47`）接线记录（2026-11-20）

- **oracle 分派**：`src/M2Server/Magic/MagicManager.cs` 的 `case MagicConst.SKILL_47` →
  `MagBigExplosion(…, SystemShare.Config.FireBoomRage)`；与 23 `SKILL_FIREBOOM` **同 helper**，
  仅魔号不同（`MagicConst.cs` `SKILL_47 = 47`）。Rust 侧早已共用实现（P3-B2 的 23/47 半径来源用例
  已钉住 `47 ⇒ FireBoomRage`），本次补的是**装置 golden + 对拍面**。
- **范围来源落进夹具**：部署 `Server.conf` 的 `FireBoomRage=1`（`Fixture.fire_boom_rage=1`、
  `server_conf_sha256=ce5e3ce9…`）。
- **实测（`golden/magic/fireboom47.json`，s1/s5/s13）**：`Rng` = **7 抽**（`[0,0,0,7,1,0,3]`）、
  `DrainPasses` = 4；target **HP 100→85（damage 15）**；caster `[638,640,31,656]`、
  target `[17,638,53,31,656]`、obs_in `[17,638,31,656]`、obs_out 空；
  RM：target 8044/10052、三收件人 8103/8090、caster 10123。
- **契约**：`cover_drain` / `login_order=true` / `reachable` / 逐 case `anti_magic=1` /
  `construction{Warrior L20, [], [FireBoom47]}` / `target_max_hp=224` / `rng_total_draws=7`。
- **证据**：run_id `magic47-20261120T095010`；驱动 `fireboom47_parity_s1/s5/s13` 各 rc=0；
  比较器 `real_rust_ledger_fireboom47` ok；fmt/clippy（两包）rc=0。
- **MG-56 升格**：`status=verified`、`basis=semantic` ⇒ magic 域 **17/65**。

### 33. 24（`SKILL_LIGHTFLOWER`）接线 + 两段出帧统一（2026-11-20）

- **oracle 分派**：`src/M2Server/Magic/MagicManager.cs` 的 `case MagicConst.SKILL_LIGHTFLOWER` →
  `MagElecBlizzard(playObject, power)`；helper 按 `SystemShare.Config.ElecBlizzardRange`（部署值 2）。
- **golden**：`golden/magic/lightflower.json`（s1/s5/s13，C# oracle 生成）——`Rng` 各 7 抽
  （`[0,0,0,7,1,0,3]` / `[0,0,0,6,1,0,1]` / `[0,0,0,3,0,0,7]`）、`DrainPasses=4`、HP `100→99`（damage 1）；
  caster `[638,640,31,656]`、target `[17,638,53,31,656]`、obs_in `[17,638,31,656]`、obs_out 空；
  契约 `cover_drain`/`login_order`/`reachable`/`anti_magic=1`/`target_max_hp=224`/`rng_total_draws=7`。
- **发现并修复分流差异**：24 原走"旧合并 flush"，Rust 出帧为 caster `[638,31,640,656]`、
  target `[17,53,638,31,656]`——与 oracle golden 相反。golden 落地即证伪旧序 ⇒ 24 **并入两段出帧**
  （与 23/33/47 同路径），产品 diff 只有该 `if` 分流与其注释。
- **单测期望改动的出处**：`spell_p3b2_lightflower_center_self_and_undead_division` 的 31 tie 序
  旧期望 `[M3,M1,M2,P1]` 属"尾帧+受击帧同批 flush"时代的堆解。**本支多目标 tie 序无 oracle 锚
  （golden 仅单目标），期望取自两段模型下 `NetPriorityQueue` 出队序，属 synthetic 预期**；
  与 23 的多目标支同口径（卡 §30 登记）。**该用例不得被当作 24 的行为证据；MG-38 升格的证据只算
  单目标三支（`fireball_case_assert` + 比较器对 `caster/target/obs_in` 的帧序逐值比对）。**
  负对照（2026-11-20 实跑）：把期望改回旧序 `[M3,M1,M2,P1]` ⇒ 该用例红（left `[4,2,3,5]` /
  right `[4,3,2,5]`），还原后 sha 相等且复跑绿。
- **证据**：run_id `magic24-final-20261120T095906`——24 三支 + 23/33/47 九支驱动全 rc=0；
  四个比较器（lightflower/fireboom/snowwind/fireboom47）ok；`cargo test -p game-svr --lib`
  **2055 passed / 0 failed**；`fmt` rc=0；`clippy --workspace --all-targets -D warnings` rc=0。
- **MG-38 升格**：`status=verified`、`basis=semantic` ⇒ magic 域 **18/65**。

### 34. 40（`SKILL_UNAMYOUNSUL`）接线 + 夹具契约扩展（P3-B3 状态族，2026-11-20）

- **oracle 分派**：`src/M2Server/Magic/MagicManager.cs` 的 `case MagicConst.SKILL_UNAMYOUNSUL`：
  `IsProperFriend` → `Random(7) - (UserMagicLevel + 1) < 0` 门 → 对目标三态计时器
  `StatusTimeArr[DECHEALTH/DAMAGEARMOR/STONE]` **逐项 `!= 0 ⇒ 置 1`**（不是清 0），并 `boTrain`。
- **为什么必须扩夹具**：旧夹具 target 三态全 0 ⇒ 门就算过也"什么都不做"，帧面只剩 `17/638`
  ⇒ 按旧模板升格等于只证空转路径。**断言只到 `DoSpell` 时刻**——真正"解毒"生效在 `Run` 的
  计时器处理，那是另一条跑 tick 的案子，不在本支范围。
- **契约扩展（schema v2，向后兼容）**：
  - 输入 `MagicSpec.PoisonStates[3]`（**走到状态分支的用例必须显式写**，不靠缺省 0）；
    golden `Fixture.status_time_arr = {dechealth,damagearmor,stone}`（仅该字段被显式给出时出现，
    既有件**零变化**——已重跑装置输出核对：**16 件零差异**（除既有墙钟 `delivery`）；另两件各有原因：
    `lightflower` 当时**分派缺失**（已由 `896390238` 修复，见 §37）、`firecharm` **两次运行不一致**
    （既有装置墙钟依赖，已另立卡 `openmir2-firecharm-nondeterminism`）。
  - 输出 case `StatusTimeArr = {dechealth,damagearmor,stone}`（术后值）。
  - 台账同形两键 `StatusTimeArrBefore/After`；比较器三处逐值比（golden Fixture ↔ 台账 Before、
    golden case ↔ 台账 After）。
- **四支分支矩阵**（非零初值 5/6/7 互不相同 ⇒ "置 1"前后可辨）：
  | case | 术前 | 门 | 术后（oracle 侧独立推导） |
  |---|---|---|---|
  | `gate_open_all`（seed 14，首抽 0） | `[5,6,7]` | 开 | `[1,1,1]` |
  | `gate_closed_all`（seed 11，首抽 3） | `[5,6,7]` | 关 | `[5,6,7]` |
  | `gate_open_zero`（seed 14） | `[0,0,0]` | 开 | `[0,0,0]` |
  | `gate_open_mixed`（seed 14） | `[5,0,7]` | 开 | `[1,0,1]` |
  术后值在**装置与驱动两侧各自**由"门结果 + 术前值"独立推导并断言（不拿实现输出自证）；
  混合支只有"仅非零项置 1"的实现才能通过。
- **负对照**：① 把门当恒开（`if false && …`）⇒ `gate_closed_all` 红（before `[5,6,7]`、after
  `[1,1,1]`）；② 改成无条件三项全置 1 ⇒ `gate_open_mixed` 红（期望 `[1,0,1]`）、`gate_open_zero`
  红（期望 `[0,0,0]`）；两处变异均 cp 快照还原、`sha256` 相等后基线复跑绿。
- **证据**：run_id `magic40-final-20261120T101556`——四支驱动 rc=0、比较器
  `real_rust_ledger_unamyounsul` ok；`cargo test -p game-svr --lib` 2055 passed/0 failed；
  fmt rc=0；`clippy --workspace --all-targets -D warnings` rc=0。
- **MG-46 升格**：`status=verified`、`basis=semantic` ⇒ magic 域 **19/65**（边界见上：只证 DoSpell 时刻）。
### 35. 已升格条目的对象类型覆盖口径（2026-11-20，普查）

**方法（可复跑）**：在 `src/M2Server/Magic/MagicManager.cs` 里同时统计
`ActorRace.Animal` / `ActorRace.Play` / `LifeAttrib`（undead） / `Master|IsSlave` 四类判据，
并按"最近一个 `case MagicConst.X:` + 当前所在 `static` 函数"归属；再与 verified 清单对照。

**结论**：**7 条已升格条目的 verified 只覆盖"玩家 target"路径**，其 oracle 中的对象类型分支未覆盖：

| 条目 | 魔号 | 未覆盖分支（oracle 位置） |
|---|---|---|
| MG-11 | 5 `SKILL_FIREBALL2` | `Race >= ActorRace.Animal` |
| MG-19 | 11 `SKILL_LIGHTENING` | `LifeAttrib == LA_UNDEAD` 与 `Race >= Animal` |
| MG-27 | 13 `SKILL_FIRECHARM` | `Race >= Animal` |
| MG-54 | 45 `SKILL_45` | `LifeAttrib == LA_UNDEAD` 与 `Race >= Animal` |
| MG-53 | 44 `SKILL_44` | 共享 helper `MagHbFireBall` 的 `Race >= Animal` |
| MG-38 | 24 `SKILL_LIGHTFLOWER` | 共享 helper `MagElecBlizzard` 的 `LifeAttrib != LA_UNDEAD` |
| MG-32 | 18 `SKILL_CLOAK` | 共享 helper `MagMakePrivateTransparent` 的 `Race >= Animal && TargetCret == baseObject` |

**其余 10 条已升格条目（魔号 1、2、14、15、19、28、29、31、33、47）的对象门只有
`IsProperTarget` / `IsProperFriend` 一类**，对玩家目标已覆盖；`MagBigExplosion`（23/33/47）与
`MagMakeGroupTransparent`（19）内无对象类型分支。

**口径**：上述 7 条的 `verified` 结论**不得**外推到怪物/不死目标；怪物/不死目标的覆盖需等"动物/怪物夹具"
落地后逐条补场景（另立卡）。本清单只做范围限定，**不改任何条目 status**。

**范围外（未升格，登记供后续）**：`SKILL_AMYOUNSUL(6)`、`SKILL_GROUPLIGHTENING(42)`、`SKILL_GROUPAMYOUNSUL(48)`、
`SKILL_GROUPDEDING(39)`、`SKILL_TAMMING`（`!Animal` 反门 + `MagTamming` 的 Master/undead/几率分支）、
`MagTurnUndead`（`SuperMan || LifeAttrib != LA_UNDEAD`）、`MabMabe`（`Race == Play` 两支）——它们本来就未升格。

### 36. 非玩家对象夹具 v1（被动怪物）+ 16 的对象选择面（2026-11-20）

**范围声明（§26）**：v1 只判**对象选择/门判据**——哪些对象收到 `RM_MAKEHOLYSEIZEMODE(60005)`。
v1 **不 Operate** 怪物 ⇒ 效果面（状态置位/伤害/诱捕）**仍空**；任何由目标处理的效果分支
**不得**以 v1 升格，`scope = gate_only`。effect 面见卡 `openmir2-monster-actors-v2`。

**装置扩展（schema v3，向后兼容）**
- `MagicSpec.Monsters[]`：`(x,y,race,level,hp,life_attrib,master,target_cret)`；`master/target_cret`
  是**本列表 1 基序号**（0 = 无）。默认空 ⇒ 既有件零变化。
- golden `Fixture.monsters[]` 同形；每个怪物在 `Receivers` 里以 `mon{i}` 出现，只记其 RM、`Sent` 恒空。
- 离线进程补 `SystemShare.EventMgr = new EventManager()`（16 的 `MagMakeHolyCurtain` 会 AddEvent；
  缺失时 NullReference）。**既有件零变化已实测**：重跑装置输出，**16 件零差异**（除既有墙钟 `delivery`）；
  `lightflower` 分派缺失与 `firecharm` 不可复现两件按 §37 / `openmir2-firecharm-nondeterminism` 记
  **0 差异**。

**16 四支（`golden/magic/holyshield.json`，`--case holyshield`，ArmRingAmulet=true）**
| case | 怪物 | 命中 | 说明 |
|---|---|---|---|
| `mon_hit_two` | 2 只 L1 | `mon0`/`mon1` 各 1 条 60005 | 全选 |
| `mon_blocked_level` | 1 只 L200 | 0 | `Random(4)+(30-1) > 200` 不成立 |
| `mon_blocked_master` | 1 只 L1（Master 自指） | 0 | `Master == null` 门挡 |
| `mon_partial` | L1 + L200 | `mon0` 1 条、`mon1` 0 | 部分命中（证明非同全选/全否） |

**判据**：四支比的是**收件人集合**（哪只怪收到几条 `RM_MAKEHOLYSEIZEMODE`），不是"帧数相等"；
另断言**同一只怪同种 RM 恰 1 条**（不重复投递；装置侧 `mon{i}` 捕获时校验）。

**harness substitution（必须知道）**：离线进程装了**空 `EventManager`**（只维护列表）。`MagMakeHolyCurtain`
在选完对象后会对 6 个格子 `AddEvent(HolyCurtainEvent)`，`SystemShare.EventMgr == null` 时那里 NRE。
**对象选择本身不读 `EventMgr`**：选择循环只读 `GetMapBaseObjects` 的 Actor 与
`Race/Level/Master` 三个字段并写目标 RM 队列，AddEvent 发生在选择之后、且事件对象只被存进列表
（无 Operate、无回写）⇒ 空实现不改变"谁被选中"。**但事件链（AddEvent→后续 Operate）不在本 golden
的覆盖面**——v2 接手时不要把它读成"oracle 不产生事件"。

**未做**：Rust 侧怪物夹具与 16 的驱动/比较器（下一支）；16 **不升格**（v1 不足以判效果面）。

### 39. `SKILL_GROUPDEDING`(39) 门序更正与夹具设计（2026-11-20）

**此前读法作废（留痕）**：本文档上一轮曾写"`MagGroupDeDing` 只打 `Race>=Animal`、玩家目标空转"。
读完整 helper 后确认**该读法错误**：`Race>=Animal` 只决定 `result`（⇒ `boTrain`），**不参与伤害**。
玩家目标那次 `HP 100→100` 的真正原因是**命中门**（`Random(SpeedPoint) >= HitPoint ⇒ nPower=0`）
或护甲衰减到 0。保留这条更正而不是静默替换，是因为两套夹具会长得完全不同（"必须放动物" vs "必须让命中门过"）。

**正确门序（`src/M2Server/Magic/MagicManager.cs` 的 `MagGroupDeDing`，逐条带行号）**
1. 跳过 `Death || Ghost || self`（`:950`）；
2. `IsProperTarget(baseObject)`（`:954`）；
3. `nPower = GetAttackPower(LoByte(DC), HiByte(DC)-LoByte(DC))`（`:956`）；
4. **命中门**：`Random(baseObject.SpeedPoint) >= playObject.HitPoint ⇒ nPower = 0`（`:957`；
   `SpeedPoint=0` 时 `Random(0)=0`，若 `HitPoint=0` 则 `0>=0` 恒真 ⇒ **必 miss**）；
5. `nPower>0` ⇒ `GetHitStruckDamage`（护甲衰减，`:963` 起）；
6. `StruckDamage(nPower)`（`:967`）+ 施法者 200ms `RM_DELAYMAGIC(nPower, …, 1, target)`（`:968`）；
7. `Race >= Animal` **仅**置 `result=true`（`:970-972`）；
8. 对每个对象（含被跳过者）发 `RM_10205`（`:974`）；**泵序低字节 = `10205 & 0xFF = 221`**。

**夹具设计（`GroupDeDingBatch`，待实现）**
- **全额怪**：命中门必过（`HitPoint>0` 且该目标 `SpeedPoint` 抽到小值/为 0 但 HitPoint 使门不成立）+ 低 AC；
- **衰减怪**：高 AC ⇒ `GetHitStruckDamage` 生效；与全额怪对照证明"同公式、伤害随属性变"；
- **不该被命中 ×2（分别断言不同分支）**：① `SpeedPoint=0` ⇒ 命中门判 miss（走 4→nPower=0）；② `Death=true`
  ⇒ 走 1 早退。**若两者帧面相同（都无 31/都只 10205），必须在卡里写明"这两支在帧面不可区分，
  区分依据 = 是否进入 `IsProperTarget`/是否产生 200ms `RM_DELAYMAGIC`"**，不许用"都没掉血"充当两条断言；
- **逐目标抽数**：命中门是 `Random(target.SpeedPoint)` ⇒ 每只目标抽数随其 `SpeedPoint` 变；case 里逐目标
  记抽数并给总数，两只怪 `SpeedPoint` 不同时总抽数也应不同（"抽数随参数变"的正例）；
- **HP before/after 双值**入 case，与 `mon{i}` 收件人集合并列。

**"空转 golden"的处置（本交付定）：选 ①**——`GroupDeDingBatch` 落地后**替换**同名
`golden/magic/groupdeding.json`，旧件删除并在本卡记"旧件作废，原因 = 空转（玩家目标、命中门恒 miss）"；
**不保留**同名旧件（避免后来者分不清哪份是证据；若将来要留痕，用带 `.superseded` 后缀并标"非证据"）。

**实现中的硬点（待解，先记）**：命中门要"确定命中"与"确定 miss"两支都稳定，需要
`HitPoint > 0`（否则 `Random(0)=0 >= 0` 恒 miss、无人能被击中）且要控制每只怪的 `SpeedPoint` 与 seed；
但 `Random(SpeedPoint)` 均匀分布 ⇒ "必 miss"不天然确定，需按 seed 扫描把 miss 落在目标怪身上。
⇒ 装置要补 `MagicSpec.CasterHitPoint` 与 `MonsterSpec.SpeedPoint` 两个输入，并为每支选 seed（`seed_reason` 记"第 i 只怪抽值 x ≥/< HitPoint"）。

**四支定稿（甲方案，opc-19 2026-11-20 确认）**
| case | caster `HitPoint` | 怪物（**顺序显式固定**） | 证明什么 |
|---|---|---|---|
| `all_miss_hp0` | 0 | M1(S=0)、M2(S=1) | 门恒真 ⇒ 全场 miss（不需要扫 seed） |
| `hit_ac_split` | 5 | M1(S=0,AC低)、M2(S=0,AC高) | 必中（`SpeedPoint=0` ⇒ `Random(0)=0 < HitPoint` 恒真）+ 护甲衰减 |
| `miss_by_draw` | 5 | M1(S=0) 命中、M2(S=10) 经 seed 选到 `Random(10)>=5` | 门按抽值分支（区别于恒真组） |
| `dead_excluded` | 5 | M1(S=0, `Death=true`) | 走 `:950` 早退 |

**两条附加约束**
1. `hit_ac_split` 的两只怪**除 AC 外参数完全相同**（`SpeedPoint/Level/max_hp/Death` 全同），参数并排放进
   golden，让读者一眼看出"只差 AC"；否则 HP 减量差无法归因。
2. **`Random(0)` 也消耗抽样状态**（IT-12 跨语言对拍已确认）⇒ `M1(S=0)` 那次抽占序列位置；
   `hit_ac_split` / `miss_by_draw` 要**按顺序记每只怪的抽值（含 `Random(0)` 那次）**，
   `seed_reason` 能解释"M1 抽了 0、M2 的抽值是序列里第几个"；**怪物顺序改动 ⇒ 重扫 seed**（已入 golden）。

**(乙) 的否决理由（opc-19 原话）**：不是"时间不划算"，而是"唯一能升 verified 的那支，恰恰不能少分支"。

**scope 依据**：39 可升 `verified`，依据是"**伤害由施法者侧同步 `StruckDamage` 计算**"（`:967`），
不是"HP 变了"——HP 只是该计算的结果观测。

### 39b. `GroupDeDingBatch` 落地：夹具裁决、五支与 golden 替换（2026-11-20，magic-3）

**空转旧件作废**：`golden/magic/groupdeding.json` 旧件（三支 s1/s5/s13、玩家目标、`DC=0`、
命中门恒 miss、`HP 100→100`）已由本节五支**替换**（同名覆盖）。原因 = 空转（玩家目标 +
`GetAttackPower` 在 `DC=0` 时 `nPower=0`）；不保留同名旧件。

**卡面未列的两个必要夹具输入（opc-19 2026-11-20 批准）**

1. `MagicSpec.CasterDC`（落 golden `Fixture.caster_dc`）：
   - 四支统一 `HUtil32.MakeWord(20,20)` ⇒ `Hi-Lo=0` ⇒ `GetAttackPower` 区间抽为 `Random(1)`、
     `nPower` 恒 20（伤害常数）。**代价：本批四支不覆盖 `GetAttackPower` 的区间抽。**
   - 为此另加第五支 `dc_range`：`MakeWord(20,30)`（区间 10）⇒ `Random(11)` 进伤害
     （seed 7 抽到 4 ⇒ `nPower=24`、HP 300→276），覆盖区间抽。
2. `MagicSpec.CasterAttackMode = HAM_PEACE`（落 golden `Fixture.caster_attack_mode`）：
   - oracle ctor 默认 `HAM_ALL`；`PlayObject.IsAttackTarget` 的 `HAM_ALL` 对 `Race=Play(0< NPC=10)`
     返回 true ⇒ 玩家 target 也进伤害链、也抽 `Random(15)`，逐只抽值序列会被玩家打断。
   - `HAM_PEACE` 只放 `Race>=Animal` ⇒ 只有怪物进伤害链，抽值序列与卡面「怪物顺序固定」一致。
   - **这是夹具输入、不是 oracle 行为；本批不覆盖 `HAM_ALL` 语义**（玩家 target 仍收
     `RM_10205`/`SM_716`，其 HP 不变）。

**五支（怪物顺序 = `GetMapBaseObjects` 收集序 x↑y↑；`CasterDC` 除 `dc_range` 外均 `MakeWord(20,20)`）**

| case | seed | caster HitPoint | 怪物 | 期望（golden 实测） |
|---|---|---|---|---|
| `all_miss_hp0` | 0 | 0 | M1(101,101,S=0)、M2(102,100,S=1) | 门恒真全 miss；HP 300/300；RNG 7 |
| `hit_ac_split` | 5 | 5 | M1(101,101,S=0,AC=0)、M2(102,100,S=0,AC=MakeWord(5,5)) | M1 HP 300→280、M2 300→287；RNG 12（含 2 次交付 `Random(1)`） |
| `miss_by_draw` | 2 | 5 | M1(S=0)、M2(S=10) | M1 中（280）、M2 `Random(10)=8>=5` miss（300）；RNG 9 |
| `dead_excluded` | 5 | 5 | M1(101,101,S=0,Death=true) | 死怪收集期被过滤；零抽；M1 无 RM；RNG 0 |
| `dc_range` | 7 | 5 | M1(S=0,AC=0) | `Random(11)=4` ⇒ nPower=24 ⇒ HP 300→276；RNG 6 |

**`hit_ac_split` 的 AC-only 约束（卡面约束①）**：两只怪除 `ac` 外 `x/y/race/level/hp/life_attrib/
master/target_cret/speed_point/death` 全同（golden `Fixture.monsters` 并排可查）。x/y 不同仅为
放置/收集序。

**逐目标抽值与 `Random(0)`（卡面约束②）**：golden `mon{i}` 记 `GateIndex`/`GateDraw`/`GateHit`
（命中门 `Random(SpeedPoint)` 在 `Rng` 中的序号与抽值）；`Random(0)` 也占位（M1 的 gate 抽为 0）。
装置侧走查**恰好耗尽** `Rng`：怪物段（每只 2 抽攻击 + 1 抽门 + 命中时 1–2 抽护甲）+ 共享尾训练
`Random(3)+1`（有非死怪时 1 抽）+ 交付段每命中怪 1 抽（`GetMagStruckDamage` 的 `Random(1)`，MAC=0）。
不耗尽 ⇒ `FixtureContractViolation`（rc=2）。

**`dead_excluded` 口径更正（opc-19 批准）**：死怪不是走 `:950` 的 continue，而是在
`GetMapBaseObjects`（`BaseObject.cs` 收集期 `!baseObject.Death && !baseObject.Ghost`）就被过滤，
**根本没进 objectList**。对外判据（无 `RM_10205` / 无 `RM_MAGSTRUCK` / HP 不变）一致，但
**变异牙必须打收集过滤**（Rust `World::mag_objects_in_range_impl` 的 `!e.death && !e.ghost`），
打 `:950` 无效。（§21 补注「变异要打到目标分支」的又一实例。）

**附：`ArmRingAmulet` 对 39 无效**：39 不在 `src/M2Server/Magic/MagicManager.cs:315` 的护身符组（该组为
13/14/15/16/17/18/19）；39 是 `:549` 的独立 case，无 `CheckAmulet`。装置保留 `ArmRingAmulet=true`
只装护身符物品，不触发消耗/门。

**scope 标注（opc-19 2026-11-20 裁定）**：`sm716_order_unverified` —— `groupdeding` 的
**SM_716 组帧字段严格、组内顺序未验**。机理（主机把 `RM_SPELL` 直发、不进 `SpellFrameBuf` ⇒
同级 221 组不是同一个四叉堆）与修复方向见 `docs/issues/openmir2-sm716-tie-order.md`；
**不止影响 39**，所有带 `RM_SPELL` 且同级广播 ≥3 条的施法路径都受影响。

**ActorId 接缝修正（非 oracle 行为）**：Rust `place_monsters` 的 `ActorId = idBase+10+i` 是
**夹具设施**对齐（只影响 `RM_MAGICFIRE`/`RM_DELAYMAGIC` 的 p3），不是 oracle 语义。

**新增 `#[ignore]` +5（测试三数 ignored 65→70 的解释）**：
`groupdeding_parity_{all_miss_hp0,hit_ac_split,miss_by_draw,dead_excluded,dc_range}`
—— 真实台账驱动，需 `--exact --test-threads=1` 单独进程（`rng_trace` 进程级 `OnceLock`），
故标 `#[ignore]`；由 `cargo test -p game-svr --lib` 计入 ignored（2055 passed / 0 failed / 70 ignored）。

### 40. 验证前置链（所有 magic 条目的复跑前提，2026-11-20）

**事实**：`mir2-rs/tests/parity/tests/magic_fireball_parity.rs` 里 `real_rust_ledger_*` 全部带
`#[ignore = "需先跑 game-svr 三支驱动…生成真实台账"]` ⇒ **默认 `cargo test` 一条 ledger 都不跑**；
"magic 已验"的真实前提是两步：

```bash
# 1) 驱动：逐支单跑（rng_trace 进程级），同一 run_id
export MIR2_LEDGER_RUN_ID=<id>
cargo test -p game-svr --lib -- --exact --ignored tests::<driver>
# 2) 台账比较器：必须 --ignored 且同一 run_id
cargo test -p mir2-parity-tests --test magic_fireball_parity -- --ignored
```
驱动清单与 case 映射见各条目 `evidence.unit_tests` 与 `evidence.test`；缺一步 = 比对不成立（会被
run_id 侧车挡住）。**新条目交付时卡里必须写这三项：驱动名、比较器名、run_id 与命令原文。**

### 41. 交付前检查清单（模板，每笔必过）

1. **锚**：写 `xxx.cs:N` / `xxx.rs:N` 前先确认是**全路径**或改**符号锚**；高 churn 文件（`lib.rs` 等）禁行号锚；
   提交前跑 `docs_anchor_gate.py --prose --base master`，红不过。
2. **装置分派**：改 `Program.cs` 附 `grep -c 'if (name =='` 修前/修后 + `check-dispatch-regression -Base auto -Head <head>`
   的 GREEN 原文（防相邻行覆盖）。
3. **验证前置链（§40）**：新条目写清 驱动名 / 比较器名 / `run_id` 与命令原文（驱动 `--ignored --exact` →
   比较器 `--ignored` 同 run_id）。
4. **清单**：`validate-parity-manifests.ps1 -RepoRoot <wt>` 与 `coverage-report.ps1 -RepoRoot <wt>` 双绿，贴原文。
5. **变异牙**：§21 补注——共享臂用镜像变异、变异后仍绿先核"是否打到目标分支"；附 `Ran=(passed,failed)`。
6. **时钟（§36）**：不依赖真实时钟；时长可注入并写进 golden。
7. **status 只由 QA 改**；作者只留证据。

### 42. 清单去重（2026-11-20）：7 条同一叶子的重复行不计分母

- **根因**：`legacy_source`（`E:/UserFleet/magic-parity.json`）迁移时**按 `case` 行/值分支逐行直译**，
  未按"叶子"去重 ⇒ 外层 `case` 标签行与内层同魔号分派行（20↔27、21↔28、22↔29、25↔32、26↔33），
  以及既有 30≡23、31≡24，各多出一行。**生成器缺陷，需按叶子去重后再迁移**。
- **处置**：这 7 行标 `status=duplicate` + `merged_into` + `pre_duplicate_status`（历史真值）+ 凭据 notes；
  `denominator.count = 58`（65 − 7）。
- **Shape 两对不是重复**：`:239/:243` 在 `DoSpell` 内联 switch（单目标 Amyounsul），`:919/:923` 在
  `MagGroupAmyounsul`（群体 48）——不同函数/魔号/helper；其 `legacy_id` 原为**值标签**，已改为位点写法
  `Shape=1@DoSpell` / `Shape=1@MagGroupAmyounsul` 等，原值保留在 notes。
- **对外口径**：分母 104→97 是**去重**（7 条同一叶子的重复行）造成的，**不是新增验证**；覆盖率因此
  从 51/102 变为 51/97（技能 20/58）。
