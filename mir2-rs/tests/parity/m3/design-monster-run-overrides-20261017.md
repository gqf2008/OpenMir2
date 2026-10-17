# 怪物 `Run` 覆写清册与 `FireballMonster` 实现设计

- **基线**：`568b0a962`（worktree `/e/tmp/impl-monrun2`，detached HEAD）。
- **任务来源**：`82-two-side-window-runbook.md` §2f 总表第 9 行（卡 `openmir2-spell-frame-bypass-paths`）：
  机器人 `AutoSpell` 与怪物硬编码 `RM_SPELL(48)`（`FireballMonster.cs:39`）两条旁路未建模，
  **接上之前不得判「帧面缺口已全清」**。
- **本件只做界定与设计**：不写实现代码、不动 `src/**`（C# oracle）、`sql/**`、`MirClient/`、
  `crates/{protocol,storage}/**`、`E:/MirServer/**`；不起服务、不占端口、不写库。
- **引用纪律**：oracle 行号逐条实测（`568b0a962` 树）；Rust 侧只引符号/文件路径，不写 `:行号`
  （`crates/world/**`、`bins/game-svr/**` 会随并行车道位移）。
- **复核命令**（本件所有计数均可重放）：

```bash
grep -rn "override void Run" src/M2Server/ | wc -l                    # 48
grep -rn "override void Run" src/M2Server/Monster/Monsters/ | wc -l   # 43
grep -rn "override void Run" src/M2Server/Monster/ | wc -l            # 44（含 MonsterObject 基类）
```

---

## 1. oracle 覆写清册

### 1.1 统计与层级

`src/M2Server/` 共 **48** 处 `override void Run`：怪物子类 **43**、怪物基类 2（`MonsterObject`、
`AnimalObject`）、非怪物 3（`SafeEvent`、`NormNpc`、`PlayObject`）。本清册以 43 个怪物子类为主，
基类 `Run` 作为「替代/增量」判定的参照。

类层级（只列与本清册相关的边）：

```text
BaseObject.Run                     （消息泵 + 恢复/体力点/TargetCret 清理，BaseObject.Base.cs:25）
└─ AnimalObject.Run                （HolySeize 到期后 BreakHolySeizeMode，AnimalObject.cs:142）
   └─ MonsterObject.Run            （Think/AttackTarget/GotoTargetXY/Wondering + CheckRoyalty/Crazy，MonsterObject.cs:251）
      ├─ AtMonster.Run             （索敌门 8000/1000 + SearchTarget，ATMonster.cs:13）
      │  └─ CowKing/CowMonster/MagCow/ZilKin/WhiteSkeleton/DoubleCritical/Scorpion/SpitSpider/
      │     GasAttack/GasMoth/GasDung/MinorNuma/RockMan…
      ├─ MagicMonster.Run          （MonsterObject.Run 的复制：去 CheckRoyalty/Crazy；
      │                             自有 private Think、override AttackTarget（不造成伤害），MagicMonster.cs:84）
      │  └─ FireballMonster.Run    （FireballMonster.cs:14）
      ├─ PercentMonster.Run        （同 MagicMonster 的复制；死类，全仓无 new PercentMonster）
      └─ ChickenDeer/Clone/DualAxe/BoneKing/DigOut/Elf/Fire/Green/Red/Khazard/
         Lighting/Sculture/ScultureKing/Tele/MagicMonObject/ExplosionSpider…
AnimalObject
├─ StickMonster.Run               （AnimalObject 直派生：没有 MonsterObject AI）
│  ├─ CentipedeKingMonster.Run
│  └─ SandMobObject.Run
├─ BeeQueen.Run / BigHeartMonster.Run / SoccerBall.Run / SpiderHouseMonster.Run
NormNpc.Run                       （清 Master + base.Run；NormNpc : AnimalObject）
└─ GuardUnit
   ├─ ArcherGuard.Run / SuperGuard.Run
   │  ├─ ArcherPolice（继承 ArcherGuard.Run）
   │  └─ PetSuperGuard（继承 SuperGuard.Run）
   └─ CastleDoor.Run / WallStructure.Run
```

关键区分：**直接派生自 `AnimalObject` 的类（`BeeQueen`/`BigHeartMonster`/`SoccerBall`/
`SpiderHouseMonster`/`StickMonster`，以及经 `StickMonster` 的 `CentipedeKing`/`SandMob`）
其 `base.Run()` 只到 `AnimalObject.Run`，不经过 `MonsterObject.Run`** ⇒ 它们自己的 `Run`
就是全部 AI；而 `MonsterObject`/`AtMonster` 家族的 `base.Run()` 会再跑一遍基类 AI。

另外 3 条 `override void Run` 不属于怪物子类，仅列出以闭合 `grep` 全量：
`SafeEvent.Run`（`Event/Events/SafeEvent.cs:17-21`，事件调度）、
`NormNpc.Run`（`Npc/NormNpc.cs:1604-1611`，守卫基类，见上）、
`PlayObject.Run`（`Player/PlayObject.Message.cs:19-766`，玩家消息分派）。

### 1.2 全量清册（43 条）

图例：`M`=`MonsterObject.Run`；`MM`=`MagicMonster.Run`；`A`=`AnimalObject.Run`；
`N`=`NormNpc.Run`；`B`=`BaseObject.Run`。RNG 只列 `Run` 体内**直接**抽取；间接（被调 helper/生成）另注。

| # | 类（文件） | Run 区间 | 触发条件 | RNG 取数序（逐抽） | 自有帧 | 与基类关系 |
|---|---|---|---|---|---|---|
| 1 | `ATMonster`（ATMonster.cs） | 13–24 | `CanMove()`；`>8000 \|\| (>1000 && 无目标)` | —（`SearchTarget` 不抽） | — | 增量（索敌）→ M |
| 2 | `ArcherGuard`（ArcherGuard.cs） | 39–91 | `CanMove()`；每 `WalkSpeed` 扫 `VisibleActors` 选最近合法目标；有目标且 `AttackTick>=NextHitTime` 攻；无目标朝 `GuardDirection` | — | —（`AttackTarger` 出攻击帧） | 增量 → `NormNpc.Run` → A → B（**无 M**） |
| 3 | `BeeQueen`（BeeQueen.cs） | 47–74 | `CanMove()`；每 `WalkSpeed` 且命中钟到 ⇒ `SearchTarget`；有目标 ⇒ `MakeChildBee`；清理 `BeeList` | —（`RM_ZEN_BEE` 到点 `RegenMonsterByName(Bee)` 抽构造链） | `RM_HIT`（广播）+ `RM_ZEN_BEE`（自延时 500ms） | 增量 → A（**无 M**） |
| 4 | `BigHeartMonster`（BigHeartMonster.cs） | 44–54 | `CanMove() && VisibleActors.Count>0` ⇒ `AttackTarget()` | —（攻击 helper 内抽） | — | 增量 → A（**无 M**） |
| 5 | `BoneKingMonster`（BoneKingMonster.cs） | 48–76 | `CanMove() && WalkTick` 门；索敌门；`DangerLevel<HP/MaxHP*5 && >0` ⇒ `CallSlave`；满血置 5；清 `SlaveObjectList` | —（`CallSlave` 生成抽） | — | 增量 → M |
| 6 | `CastleDoor`（CastleDoor.cs） | 84–104 | 无外层门；死亡置 `DeathTick`；`!IsOpened` 时 `n08=3-Round(HP/MaxHP*3)`，`Dir!=n08 && n08<3` | — | `RM_TURN`（:100） | 增量 → `NormNpc.Run` → A → B（**无 M**） |
| 7 | `CentipedeKingMonster`（CentipedeKingMonster.cs） | 91–143 | `CanMove() && WalkTick` 门；`FixedHideMode` 每 10s 扫近身 `ComeOutValue` 内目标 ⇒ `FindAttackTarget`；否则每 3s `AttackTarget`（命中即 `base.Run(); return`），每 10s `ComeDown` | — | — | 增量 → `StickMonster.Run` → A（**无 M**） |
| 8 | `ChickenDeer`（ChickenDeer.cs） | 13–63 | `CanMove()`；每 `WalkSpeed` 扫可见合法目标取最近 ⇒ `RunAwayMode`/`TargetCret`；逃跑且近距时选方向 + `GetNextPosition(…,5)`（**bug**：距离用循环残留的 `baseObject` 且两次都判 `CurrX`） | — | — | 增量 → M |
| 9 | `CloneMonster`（Clone.cs） | 62–88 | `CanMove() && SearchEnemyTick>8000`；`>1000 && 无目标` ⇒ `SearchTarget`；`WalkTick` 门且目标 ≤4：≤2 且 `Random(3)!=0` ⇒ `base.Run(); return`，否则 `GetBackPosition`；目标 <6 且命中钟到 ⇒ `LightingAttack` | **#1 `Random(3)`**（条件：`\|Δx\|<=2 && \|Δy\|<=2`） | — | 增量 → M；**提前 `base.Run(); return`** 跳过本类后续 |
| 10 | `CowKingMonster`（CowKingMonster.cs） | 40–94 | `!Death && !Ghost && 30s JumpTime`；有目标且 `SiegeLock>=5` ⇒ 目标格 `SpaceMove`/`MapRandomMove` 后 return；`CrazyCount=7-HP/(MaxHP/7)` ⇒ 8s `CrazyReadyMode`（`NextHitTime=10000`）→ 8s `CrazyKingMode`（`NextHitTime=500`/`WalkSpeed=400`） | —（`MapRandomMove` 内抽） | — | 增量 → `AtMonster.Run` → M |
| 11 | `DefendMonster`（DefendMonster.cs） | 11–14 | 无 | — | — | 纯透传 → M |
| 12 | `DigOutZombi`（DigOutZombi.cs） | 18–55 | `CanMove() && WalkTick` 门；`FixedHideMode` 扫 ≤3 格目标 ⇒ `Turned()` + `WalkTick+=1000`；否则索敌门 ⇒ `SearchTarget` | — | — | 增量 → M |
| 13 | `DualAxeMonster`（DualAxeMonster.cs） | 84–137 | `CanMove()`；每 5s 扫可见合法目标取最近 ⇒ `SetTargetCreat`；`WalkTick` 门且目标 ≤4：≤2 且 `Random(5)==0` ⇒ `GetBackPosition`；否则（3–4 格）`GetBackPosition`（**bug**：两个 4 格门都写成 `CurrX`，未判 `CurrY`） | **#1 `Random(5)`**（条件：`\|Δx\|<=2 && \|Δy\|<=2`，仅此支） | — | 增量 → M |
| 14 | `ElectronicScolpionMon`（ElectronicScolpionMon.cs） | 33–70 | `CanMove()`；`UseMagic = HP<MaxHP/2`；`>1000 && 无目标` ⇒ `SearchTarget`；目标 ≤2 且（`UseMagic` 或恰 2 格）且命中钟到 ⇒ `LightingAttack` | —（`LightingAttack` 内抽） | — | 增量 → M |
| 15 | `ElfMonster`（ElfMonster.cs） | 43–86 | `BoIsFirst` ⇒ 清 `FixedHideMode`、`RM_DIGUP`、`ResetElfMon`；死亡 >2s ⇒ `MakeGhost`；有目标/主人有目标 ⇒ `MakeClone(Dragon1)` + `AppearNow` + 解主 | —（clone 生成抽） | `RM_DIGUP` | 增量 → M |
| 16 | `ElfWarriorMonster`（ElfWarriorMonster.cs） | 112–166 | `BoIsFirst` ⇒ `RM_DIGUP`+`ResetElfMon`；死亡 2s ⇒ `MakeGhost`；无目标且 60s ⇒ 名字去尾 `1` 后 `MakeClone`，发帧后解主；否则刷新 `DigDownTick` | —（clone 生成抽） | `RM_DIGUP`、`RM_DIGDOWN`、`RM_CHANGEFACE` | 增量 → `SpitSpider`→`AtMonster.Run` → M |
| 17 | `ExplosionSpider`（ExplosionSpider.cs） | 79–90 | `!Death && !Ghost && 60s` ⇒ `DoSelfExplosion` | —（爆炸 helper 内抽） | —（爆炸 helper 出帧） | 增量 → M |
| 18 | `FireballMonster`（FireballMonster.cs） | 14–62 | `!Death && !Ghost`；有目标 → `MagCanHitTarget(target 自身坐标)` + `IsProperTarget` + `\|TargetX-CurrX\|<=8 && \|TargetY-CurrY\|<=8`；无目标置 null；索敌门 | **#1 `Random(\|HiByte(DC)-LoByte(DC)\|+1)`**（四道门后）；**#2 `GetMagStruckDamage` 内 `Random(\|HiByte(MAC)-LoByte(MAC)\|+1)`**；**#3 `StruckDamage`**（玩家目标：`Random(10)` + 每个已装备格 `Random(8)`）；**#4–#6** 见 §4.1 延迟链 | `RM_SPELL(48,x,y,48)`、`RM_MAGICFIRE(0,MakeWord(2,48),MakeLong(x,y),targetId)`、`RM_DELAYMAGIC`（自延时 600ms）；`BreakHolySeizeMode` → `RM_CHANGENAMECOLOR` | 增量 → `MagicMonster.Run` → M（**两个 AI pass**） |
| 19 | `FireMonster`（FireMonster.cs） | 23–82 | `CanMove()`；对自身及 8 个偏移格逐格 `GetEvent==null` ⇒ 建 `FireBurnEvent`；索敌门 ⇒ `SearchTarget` | — | —（事件系统后出火帧） | 增量 → M |
| 20 | `FrostTiger`（FrostTiger.cs） | 14–36 | `CanMove()`；无目标且非透明 ⇒ `MagMakePrivateTransparent(this,180)`；有目标清透明；索敌门 | — | — | 增量 → M |
| 21 | `GasMothMonster`（GasMothMonster.cs） | 24–35 | `CanMove() && WalkTick` 门；索敌门 ⇒ `GasMothAttack()`（扫最近合法目标并 `SetTargetCreat`） | —（`GasAttack` override 命中路径另有 `Random(3)`，:17） | — | 增量 → `GasAttack`→`AtMonster.Run` → M |
| 22 | `GreenMonster`（GreenMonster.cs） | 13–36 | `CanMove()`；有目标：`TargetX/Y=目标格`；`\|Δx\|==1 && \|Δy\|==1` 且目标无 `DECHEALTH` | **#1 `Random(AntiPoison+7)`**（条件同上）；`<=6` ⇒ `MakePosion(DECHEALTH,30,1)` | — | 增量 → M |
| 23 | `Khazard`（Khazard.cs） | 13–59 | `CanMove()`；有目标且 `\|Δx\|==2 && \|Δy\|==2` 进入分支 | 进入分支后**固定 3 抽**：`Random(2)`（`:25`）；`TargetCret.SpaceMove`（`:29`/`:43` → `BaseObject.cs:1103` → `SpaceMoveGetRandXY`（`:1040`））在落点不可走时另抽 `Random(Width)`（`:1084`）、必要时 `Random(Height)`（`:1091`）；随后 `Random(1)` + `Random(AntiPoison+7)`（`:30`/`:44`，两支同序，`Random(1)` 恒 0）——**固定 3 抽之外无上界** | `RM_SPACEMOVE_FIRE`（发给目标） | 增量 → M |
| 24 | `LightingZombi`（LightingZombi.cs） | 32–58 | `CanMove() && SearchEnemyTick>8000`；`>1000 && 无目标` ⇒ `SearchTarget`；`WalkTick` 门且目标 ≤4：≤2 且 `Random(3)!=0` ⇒ `base.Run(); return`，否则 `GetBackPosition`；目标 <6 且命中钟到 ⇒ `LightingAttack` | **#1 `Random(3)`**（条件：`\|Δx\|<=2 && \|Δy\|<=2`） | — | 增量 → M |
| 25 | `MagicMonObject`（MagicMonObject.cs） | 23–60 | `CanMove()`；`UseMagic=HP<MaxHP/2`；`>1000 && 无目标` ⇒ `SearchTarget`；`Master==null` ⇒ return；否则 `Master` ≤5 且（`UseMagic` 或恰 5）且命中钟到 ⇒ `LightingAttack` | —（`LightingAttack` 内抽） | — | 增量 → M |
| 26 | `MagicMonster`（MagicMonster.cs） | 84–186 | `CanMove() && !FixedHideMode && !StoneMode`；与 M 同构；**无 CheckRoyalty/CrazyMode 尾** | —（`Think` 的 `WalkTo(RandomByte(8))`；`AttackTarget` 不攻击） | — | **替代**（M 的复制）→ `base.Run()` = M（第二 pass） |
| 27 | `MinorNumaObject`（MinorNumaObject.cs） | 12–23 | `!Death`；索敌门 ⇒ `SearchTarget` | — | — | 增量 → `AtMonster.Run` → M |
| 28 | `PercentMonster`（PercentMonster.cs） | 79–181 | 与 M 同构；**死类**（全仓无实例化） | — | — | **替代**（M 的复制） |
| 29 | `RedMonster`（RedMonster.cs） | 13–36 | `CanMove()`；有目标：`TargetX/Y=目标格`；`\|Δx\|==1 && \|Δy\|==1` | **#1 `Random(AntiPoison+7)`**；`<=6 && StatusTimeArr[DECHEALTH]==0`（`RedMonster.cs:23` 与 `GreenMonster.cs:23` **同门**）⇒ `MakePosion(DAMAGEARMOR,30,1)`（与 Green 的唯一差异是毒型 `DAMAGEARMOR` vs `DECHEALTH`） | — | 增量 → M |
| 30 | `RockManObject`（RockManObject.cs） | 10–13 | 无 | — | — | 纯透传 → `AtMonster.Run` → M |
| 31 | `RonObject`（RonObject.cs） | 35–51 | `CanMove()`；索敌门 ⇒ `SearchTarget`；目标 <6 且命中钟到 ⇒ `AroundAttack` | —（`AroundAttack` 内抽） | — | 增量 → M |
| 32 | `RunAway`（RunAway.cs） | 12–64 | `!Death && !Ghost`；有目标：`HP<=MaxHP/2` ⇒ `GetFrontPosition` + `RM_SPACEMOVE_FIRE` + `SpaceMove(nx-2,ny-2)`；逃跑且 5s（**局部 `time1` 恒 0 ⇒ 门恒过**）⇒ `WalkTo(RandomByte(4/7))`；索敌门 | **#1 `RandomByte(4)`**（目标邻接）或 **`RandomByte(7)`**；每次逃跑判定一支 | `RM_SPACEMOVE_FIRE` | 增量 → M；**死类**（无实例化，且不在 `CreateMonster` switch） |
| 33 | `SandMobObject`（SandMobObject.cs） | 14–57 | `!Death && !Ghost && WalkTick` 门；`FixedHideMode` 且 `HP>MaxHP/20 && CheckComeOut` ⇒ 记出现时刻；否则 HP<MaxHP/20 且 3s ⇒ `ComeDown`；目标双轴 >15 ⇒ `ComeDown`；命中钟到 ⇒ `SearchTarget`；非隐藏 ⇒ `AttackTarget` | — | — | 增量 → `StickMonster.Run` → A（**无 M**） |
| 34 | `ScultureKingMonster`（ScultureKingMonster.cs） | 61–115 | `CanMove() && WalkTick` 门；`StoneMode` 扫 ≤2 格 ⇒ `MeltStone`；否则索敌门 ⇒ `SearchTarget` + danger `CallSlave`；清 slave 表 | —（`CallSlave` 生成抽） | — | 增量 → M |
| 35 | `ScultureMonster`（ScultureMonster.cs） | 43–79 | `CanMove() && WalkTick` 门；`StoneMode` 扫 ≤2 格 ⇒ `MeltStoneAll`；否则索敌门 ⇒ `SearchTarget` | — | — | 增量 → M |
| 36 | `SoccerBall`（SoccerBall.cs） | 19–74 | `N550>0`：前格可走 ⇒ 按 `Dir` 反射表改向 + `GetNextPosition(…,N550)`；否则 `TargetX=-1`；有目标格 ⇒ `GotoTargetXY`，到达即 `N550=0` | — | —（移动帧由 `GotoTargetXY` 家族出） | 增量 → A（**无 M**） |
| 37 | `SpiderHouseMonster`（SpiderHouseMonster.cs） | 51–78 | `CanMove()`；`WalkTick` 门；命中钟到 ⇒ `SearchTarget` + `GenBb`；清 `_bbList` | —（`RM_ZEN_BEE` 到点生成 Spider 抽构造链） | `RM_HIT` + `RM_ZEN_BEE`（自延时 500ms） | 增量 → A（**无 M**） |
| 38 | `StickMonster`（StickMonster.cs） | 101–149 | `CanMove() && WalkTick` 门；`FixedHideMode` ⇒ `CheckComeOut` → `FindAttackTarget`；否则命中钟到 ⇒ `SearchTarget`；目标超 `AttackRange` 或无目标 ⇒ `ComeDown`；否则 `AttackTarget`（命中即 `base.Run(); return`） | — | — | 增量 → A（**无 M**） |
| 39 | `SuperGuard`（SuperGuard.cs） | 68–115 | 清 `Master`；命中钟到扫 `VisibleActors`：死/幽灵/不可攻 race 移除；玩家 `PvpLevel>=2` 或怪物 ⇒ `SetAttackTarget`；有目标死/幽灵 ⇒ `DelTargetCreat`，否则 `AttackTarget` | — | —（攻击帧） | 增量 → `NormNpc.Run` → A → B（**无 M**） |
| 40 | `TeleMonster`（TeleMonster.cs） | 12–31 | `CanMove()`；有目标且距 `TargetX/TargetY` 任一 >5 ⇒ `RM_SPACEMOVE_FIRE` + `SpaceMove(目标图/坐标)`；索敌门 ⇒ `SearchTarget` | — | `RM_SPACEMOVE_FIRE` | 增量 → M |
| 41 | `WallStructure`（WallStructure.cs） | 47–82 | 无外层门；死亡 ⇒ `DeathTick` + 地图格标记；否则 `HealthTick=0` + 清标记；`n08=3-Round(HP/MaxHP*3)`（HP=0 时 4）；`Dir!=n08 && n08<5` | — | `RM_DIGUP` | 增量 → `NormNpc.Run` → A → B（**无 M**） |
| 42 | `WhiteSkeleton`（WhiteSkeleton.cs） | 29–39 | `BoIsFirst` ⇒ `Dir=5`、清 `FixedHideMode`、`RM_DIGUP` | — | `RM_DIGUP` | 增量 → `AtMonster.Run` → M |
| 43 | `ZilKinZombi`（ZilKinZombi.cs） | 35–47 | `Death && !Ghost && STONE==0 && ZilKillCount>=0 && VisibleActors.Count>0 && ReAliveTick` 到 | — | `RM_ALIVE`（`ReAlive()` 内） | 增量 → `AtMonster.Run` → M |

### 1.3 「替代 / 增量」三类关系

1. **增量（主体）**：类体先做自己的事，再 `base.Run()`。因基类不同，实际尾部不同：
   - `MonsterObject`/`AtMonster`/`GuardUnit` 家族 ⇒ `base.Run()` 会继续跑基类 AI（`M` 或 `N`）；
   - `AnimalObject` 直派生（`BeeQueen`/`BigHeart`/`SoccerBall`/`SpiderHouse`/`StickMonster` 及其子类）
     ⇒ `base.Run()` 只到 `A`，**类体就是全部 AI**；
   - `MagicMonster`/`PercentMonster` ⇒ `base.Run()` 再跑一遍 `M`（**两个 AI pass**，见 §4.1）。
2. **替代（复制）**：`MagicMonster.Run`、`PercentMonster.Run` 是 `MonsterObject.Run` 的逐行复制，
   去掉 `CheckRoyalty`/`CrazyMode` 尾（逐行 diff 仅此差异，外加上移的局部变量与注释）。
   `MagicMonster` 另有 private `Think`（**无 HolySeize 门**，`MagicMonster.cs:18-45`）与
   `AttackTarget` override（**只刷新 `AttackTick`/`TargetFocusTick`，不调用 `Attack`/不造成物理伤害**，
   `MagicMonster.cs:47-82`）。
3. **透传**：`DefendMonster`、`RockManObject` 只 `base.Run()`；`ArcherPolice`/`PetSuperGuard`
   不覆写，继承父类 `Run`。
4. **提前返回**：`Clone`/`LightingZombi`/`CentipedeKing`/`StickMonster`/`SandMob` 在命中支
   `base.Run(); return;`，跳过本类后续（但仍执行基类链）。

### 1.4 RNG 汇总（`Run` 体内直接抽）

| 抽数 | 类 | 条件 |
|---|---|---|
| 1 | `CloneMonster` | `\|Δx\|<=2 && \|Δy\|<=2` 且 `WalkTick` 门且目标 ≤4：`Random(3)` |
| 1 | `DualAxeMonster`（含 `ThornDarkMonster`/`ArcherMonster` 继承） | `\|Δx\|<=2 && \|Δy\|<=2` 且 `WalkTick` 门：`Random(5)` |
| 1 | `GreenMonster` | 邻接 1 格且目标无 `DECHEALTH`：`Random(AntiPoison+7)` |
| 1 | `RedMonster` | 邻接 1 格且目标无 `DECHEALTH`：`Random(AntiPoison+7)` |
| 1 | `LightingZombi` | `\|Δx\|<=2 && \|Δy\|<=2`：`Random(3)` |
| 1 | `FireballMonster` | 四道门后：`Random(\|Hi(DC)-Lo(DC)\|+1)`；全链见 §4.1 |
| 3（+落点抽） | `Khazard` | 对角 2,2：`Random(2)`；`TargetCret.SpaceMove` 落点不可走时另抽 `Random(Width)`/`Random(Height)`；随后 `Random(1)` + `Random(AntiPoison+7)`（两支同序，`Random(1)` 恒 0）——固定 3 抽之外无上界 |
| 1 | `RunAway` | 目标邻接：`RandomByte(4)`；否则 `RandomByte(7)`（局部 `time1` bug ⇒ 门恒过） |

间接消费（不在 `Run` 体内、但同属该 tick 的取数）：
- `FireballMonster`：`GetMagStruckDamage` 的 MAC 抽 + `StruckDamage` 的玩家耐久抽 + 延迟链（§4.1）；
- `BeeQueen`/`SpiderHouseMonster`：`RM_ZEN_BEE` 到点 `RegenMonsterByName` 的构造链；
- `BoneKing`/`ScultureKing`：`CallSlave` 的生成链；
- `ElfMonster`/`ElfWarriorMonster`：`MakeClone` 的生成链；
- `CowKing`：`MapRandomMove`；`ElectronicScolpion`/`MagicMonObject`/`RonObject`/`LightingZombi`/`Clone`：
  `LightingAttack`/`AroundAttack` 的伤害抽；
- `GasMothMonster`：`GasAttack` override 的 `Random(3)`（命中路径，`GasMothMonster.cs:17`）。

### 1.5 自有帧汇总

| 帧 | 产生点 |
|---|---|
| `RM_SPELL` / `RM_MAGICFIRE` / `RM_DELAYMAGIC` | `FireballMonster.cs:39/40/41` |
| `RM_CHANGENAMECOLOR` | `BreakHolySeizeMode()`（`AnimalObject.cs:330-334`）→ `RefNameColor()`（`BaseObject.cs:730-733`）；`FireballMonster` 每 tick 有目标即调用；`AnimalObject.Run` 的 HolySeize 到期也调用 |
| `RM_TURN` | `CastleDoor.cs:100` |
| `RM_DIGUP` | `WallStructure`、`ElfMonster`、`WhiteSkeleton` |
| `RM_DIGDOWN` / `RM_CHANGEFACE` | `ElfWarriorMonster` |
| `RM_SPACEMOVE_FIRE` | `Khazard`、`TeleMonster`、`RunAway` |
| `RM_HIT` + `RM_ZEN_BEE`（自延时） | `BeeQueen.MakeChildBee`（`:29-30`）、`SpiderHouseMonster.GenBb`（`:27-28`） |
| `RM_ALIVE` | `ZilKinZombi.ReAlive` |

> 注：`SendRefMsg` 的收件人集合 = 同图、`Race==Play`、非 `Ghost`、切比雪夫 ≤
> `SendRefMsgRange`(12)；缓存命中时按 `<11` 复核（`BaseObject.Message.cs:395-475`）。帧的**出队序**
> 是观察者 `MsgQueue` 的优先级字节升序，不是源码入队序（见 §4.3）。

### 1.6 `Operate` 覆写清册（范围说明）

上面的 43 条统计只针对 `override void Run`。另有 **6 处怪物子类 `override bool Operate`**
本清册未展开；它们不改变 `Run` 的抽数，但属于帧/消息面旁路，必须交代建模范围：

| # | 位置 | 处理的消息 | 行为 | 分期/处置 |
|---|---|---|---|---|
| 1 | `MonsterObject.cs:177` | `RM_UPDATEVIEWRANGE` | `UpdateMonsterVisible(wParam)` 后 `return true`；其余交 `base.Operate` | **登记不建模**（与 Fireball 无关；视野广播另卡） |
| 2 | `BeeQueen.cs:33` | `RM_ZEN_BEE` | `RegenMonsterByName(Bee)` + `SetTargetCreat(TargetCret)` + 加入 `BeeList`；再 `base.Operate` | **P2**（与 `RM_ZEN_BEE` 延迟队列同批） |
| 3 | `Clone.cs:14` | `RM_STRUCK` / `RM_MAGSTRUCK` / `RM_SPELL` | `Master != null` 时：`Master.MP <= 0` ⇒ 自身 `HP=0`；`RM_SPELL` 扣 `Master.MP -= nParam3`，其余扣 `wParam`；**`return false`**（不再交 `base.Operate`） | **登记不建模**（race 201 在当前部署不可达；完整语义留 P4） |
| 4 | `SpiderHouseMonster.cs:32` | `RM_ZEN_BEE` | 在 `(CurrX, CurrY+1)` 可走时 `RegenMonsterByName(Spider)` + `SetTargetCreat` + 加入 `_bbList`；再 `base.Operate` | **P2**（同上） |
| 5 | `StickMonster.cs:96` | —（纯透传） | 只 `return base.Operate(processMsg)` | 无需建模（基类链已覆盖） |
| 6 | `SuperGuard.cs:58` | —（纯透传） | 只 `return base.Operate(processMsg)` | 无需建模（基类链已覆盖） |

补充：

- 基类 `AnimalObject.cs:114` 的 `Operate`（`RM_STRUCK`/`RM_MAKEHOLYSEIZEMODE`）是上述子类
  `base.Operate` 的公共段，也是 P1 `RM_MAGSTRUCK` 泵之前的链；本件只把它当基类语义，不重复列。
- **P1（Fireball）不新增任何 `Operate` 覆写**：它读的 `RM_MAGSTRUCK` 是 `BaseObject.Operate`
  的 case（§4.1.3），不是子类覆写。
- **P2** 的 `BeeQueen`/`SpiderHouseMonster` 目前只在 §1.5 帧表出现（`RM_HIT`+`RM_ZEN_BEE`）；
  其 `RM_ZEN_BEE` 的**消费端**就是表中第 2/4 行的 `Operate`，实现延迟队列时必须一并接上，
  否则自延时帧到点后不会生成子蜂/子蛛。
- **`Clone` 的 MP 扣减完全未建模**：它用 `return false` 吞掉 `RM_STRUCK`/`RM_MAGSTRUCK`/`RM_SPELL`，
  既不走 `BaseObject` 的 struck 泵，也不发 `RM_STRUCK_MAG`；只有主人 MP 被扣（`MP<=0` 时 clone 自毁）。
  race 201 在当前部署 `MonGen`/库中均不可达（§2.4），故登记到 P4 而非 P3。

---

## 2. Rust 现状对照（符号级）

### 2.1 `crates/world/src/monai.rs` 已建模的通用行为

| Rust 符号 | 对位 oracle | 说明 |
|---|---|---|
| `monster_ai_tick` | `ProcessMonsters` 的单线程等价 | 按 `EntityStore` 槽位序；每只怪跑 `monster_run`，再跑 `base_object_run_monster`；守卫 race（`is_guard_race`）只跑后者 |
| `monster_run` | `MonsterObject.Run`（251–358） | `CanMove/Think/WalkTick/WalkWait/AttackTarget/GotoTargetXY/Wondering/CheckRoyalty`；另在顶部为 `is_active_seek_race` 的 race 复刻 `ATMonster.Run` 的索敌门 |
| `base_object_run_monster` | `BaseObject.Run` 的怪侧公共段 | 先出队 `pending_struck`（`RM_STRUCK` 等价物），再恢复/体力点/`TargetCret` 清理 |
| `think` | `MonsterObject.Think`（187–218） | 含 `DupMode` 的 `RandomByte(8)` 与 `HolySeize` 早退 |
| `attack_target` | `MonsterObject.AttackTarget`（220–245） | 邻接且到点 ⇒ **真实攻击**（`World::attack_once`） |
| `search_target` | `AnimalObject.SearchTarget`（388–423） | 不消费 RNG |
| `wondering` / `goto_target_xy` / `walk_to` / `turn_to` / `can_move` | `AnimalObject`/`BaseObject` | 见 `monai` 模块头 |
| `is_active_seek_race` | `ATMonster.Run` 的索敌门 | 仅 `53/81/83/86/88/89`（覆盖不完整，见 §3.1） |
| `is_guard_race` | `GuardUnit:NormNpc` 的「无 M」 | 守卫只跑 `base_object_run_monster` |
| `animal_struck` | `AnimalObject.Struck` | 由 `pending_struck` 驱动 |

**明确未建模**（`monai` 模块头已登记）：全部 43 个怪物子类 `Run` 的类体（除 `ATMonster.Run`
的索敌门这一条共用行为）；`Mission`/`RunAway`/`Master`/`CrazyMode` 分支；`AnimalObject.Run`
的 HolySeize 到期段；`ProcessMonsters` 的 `RunTime` 节拍门（`WorldServer.MonGen.cs:280-282`）。

### 2.2 `bins/game-svr` 已建模的生成/帧面符号

| Rust 符号 | 作用 |
|---|---|
| `mongen::race_is_spawnable` | `CreateMonster` switch 的 race 集合（含 `215`） |
| `mongen::draw_monster_construct` | 构造链逐抽：`215` = `draw_monster_object` + `3000+Random(2000)` + `draw_search_over(1500)`，**已逐抽对齐** |
| `mongen::create_monster_oracle` | `Dir=RandomByte(8)`、`MonGetRandomItems`、`CoolEye`、地形移位环 |
| `smframe::sm_spell` | `RM_SPELL` → `SM_SPELL`（尾 = `MagicId` ASCII） |
| `smframe::sm_magicfire` | `RM_MAGICFIRE` → `SM_MAGICFIRE`（尾 = 4 字节 target id 的 EDCode） |
| `smframe::sm_changenamecolor` | `RM_CHANGENAMECOLOR` → `SM_CHANGENAMECOLOR` |
| `smframe::sm_struck` | `RM_STRUCK`/`RM_STRUCK_MAG` → `SM_STRUCK`（`mag` 位控制 `Tag2`） |
| `main::broadcast_ref_msg` / `broadcast_ref_msg_with_self` | `SendRefMsg` 的收件人集合（±12、`Race==Play`、非 Ghost） |
| `main::handle_spell_cmd` | 玩家 `DoSpell` 的 `RM_SPELL` 发射点（**与怪物旁路不同源**） |
| `main::drain_combat_downlink` | `TickRecord::attacks` → `SM_STRUCK`（当前 `mag=false`） |
| `main::apply_ability_to_entity` | 只写 `dc/ac/hit_point/speed_point/hp/max_hp/level`（**无 MAC/无 AntiMagic**） |
| `game_svr::msgqueue::NetPriorityQueue` | .NET 4 叉最小堆的逐位复刻（用于观察者帧出队序） |
| `mir2_world::mongen::{regen_monsters, should_regen, initialize_monster, step_threads}` | `WorldServer.MonGen` 的刷怪循环/线程/复活门（**不含**任何类 `Run` 体） |

### 2.3 43 条覆写的覆盖矩阵

| 覆盖状态 | 类 |
|---|---|
| **已覆盖**（共用门） | `ATMonster.Run` 的索敌门，仅对 race `53/81/83/86/88/89` |
| **等价透传**（可不写） | `DefendMonster`、`RockManObject`（只 `base.Run()`） |
| **未覆盖，但有独立行为** | 其余 40 条（含 `MagicMonster`/`PercentMonster` 的复制体、`FireballMonster`、`ArcherGuard`/`SuperGuard`/`CastleDoor`/`WallStructure`、`BeeQueen`/`SpiderHouse`/`StickMonster` 家族、`Clone`/`DualAxe`/`LightingZombi`/`Green`/`Red`/`Khazard`/`RunAway`、`Elf` 家族、`CowKing`/`Sculture*`/`DigOut`/`ZilKin`/`Tele`/`Ron`/`BoneKing`/`ExplosionSpider`/`FireMonster`/`FrostTiger`/`SoccerBall`/`BigHeart`/`ChickenDeer`/`MagicMonObject`） |
| **不可达（数据层）** | `FireballMonster`(215)、`MagicMonster` 其余无子类、`PercentMonster`/`RunAway`（死类）、`Elf`(113/114)、`Khazard`(206)、`Green`(208)、`Red`(209)、`FrostTiger`(210)、`FireMonster`(214)、`ElectronicScolpion`(200)、`Clone`(201)、`Tele`(203)、`ExplosionSpider`(117)、`SoccerBall`(120)、`HighRiskSpider`(118)、`DoubleCritical`(130) 等（以 §2.4 为准）。 |

### 2.4 真机可生成 race / 名字清单（部署 MySQL 在线只读实测）

数据源与命令（只读）：
- `E:/MirServer/M2GameSvr/Envir/MonGen.txt`（GBK）：**3080 行 / 2827 条有效记录 / 207 个不同名字**；
- 名字 → `Race`：部署 GameSrv 实际读取的 MySQL `mir2_data.monsters`（`Server.conf` 的
  `[DataBase] DbType=MySQL`；`MySqlDB.LoadMonsterDB` = `select * from monsters`，`MySqlDB.cs:168-171`）。
  在线只读复核：**705 行**、`Race=215` **0 行**。
  （`E:/MirServer/M2GameSvr/Envir/Data.mdb` 只有 `MSysACEs`/`MSysObjects`/`MSysQueries`/
  `MSysRelationships`/`TBL_GABOARD`/`TBL_ITEMMARKET` 六张表，**没有 `monsters` 表**，不是怪物数据源。）

**映射结果**：2751 条记录 / 194 个名字 → **31 个 race**；**76 条 / 13 个名字**在线库中不存在
（`弓箭护卫`×57、`宝箱0`×6、`邪恶蜈蚣1/2`×4、其余各 1）。这 13 个名字被 `MonGen.txt` 引用但
`MonsterList` 无对应行：`GetMonRace` 返回 -1（`WorldServer.cs:167-174`），`CreateMonster` 的 switch
无此 case ⇒ 返回 null，**出生失败**（不是「快照漂移」）。`FireballMonster` 的 race 215
**既无 MonGen 条目，也无任何 215 行的名字**。

| race | 条目 | 名字数 | 类（`CreateMonster`） | 名字示例 | Run 覆写 |
|---:|---:|---:|---|---|---|
| 81 | 1258 | 97 | `AtMonster` | 半兽勇士、半兽勇士0 | ATMonster（Rust 已覆盖） |
| 101 | 338 | 7 | `ScultureMonster` | 祖玛卫士、祖玛卫士0 | 自有（未覆盖） |
| 105 | 279 | 4 | `GasMothMonster` | 月魔蜘蛛、楔蛾 | 自有（未覆盖） |
| 104 | 169 | 7 | `ArcherMonster` | 祖玛弓箭手、魔龙射手 | 继承 `DualAxeMonster.Run`（未覆盖） |
| 96 | 120 | 6 | `ZilKinZombi` | 僵尸3、僵尸4 | 自有 + `AtMonster.Run`（未覆盖） |
| 103 | 98 | 1 | `BeeQueen` | 角蝇 | 自有（未覆盖） |
| 97 | 58 | 11 | `CowMonster`（AtMonster 无覆写） | 沃玛勇士、沃玛卫士 | `AtMonster.Run`（未覆盖 race） |
| 93 | 50 | 4 | `ThornDarkMonster` | 暗黑战士、暴牙蜘蛛 | 继承 `DualAxeMonster.Run`（未覆盖） |
| 94 | 47 | 3 | `LightingZombi` | 僵尸1、电僵王 | 自有（未覆盖） |
| 95 | 45 | 3 | `DigOutZombi` | 僵尸2、石墓尸王 | 自有（未覆盖） |
| 83 | 34 | 6 | `SlowAtMonster` | 半兽人、森林雪人、稻草人 | `AtMonster.Run`（已覆盖） |
| 119 | 31 | 2 | `BigPoisionSpider`（SpitSpider） | 花吻蜘蛛 | `AtMonster.Run`（未覆盖 race） |
| 90 | 25 | 1 | `GasAttackMonster` | 洞蛆 | `AtMonster.Run`（未覆盖 race） |
| 112 | 21 | 1 | `ArcherGuard` | 恶魔弓箭手 | 自有（未覆盖；另有 57 条 `弓箭护卫` 为 MonGen 引用但库中不存在的缺失名） |
| 107 | 19 | 2 | `CentipedeKingMonster` | 宝箱、触龙神 | 自有 + `StickMonster.Run`（未覆盖） |
| 92 | 18 | 7 | `CowKingMonster` | 双头血魔、暗之双头血魔 | 自有 + `AtMonster.Run`（未覆盖） |
| 89 | 18 | 5 | `AtMonster` | 骷髅战将 | `AtMonster.Run`（已覆盖） |
| 87 | 18 | 4 | `DualAxeMonster` | 半兽统领、掷斧骷髅 | 自有（未覆盖，**RNG**） |
| 91 | 18 | 2 | `MagCowMonster`（AtMonster 无覆写） | 火焰沃玛 | `AtMonster.Run`（未覆盖 race） |
| 86 | 16 | 2 | `AtMonster` | 骷髅 | 已覆盖 |
| 88 | 16 | 2 | `AtMonster` | 骷髅战士 | 已覆盖 |
| 52 | 13 | 2 | `ChickenDeer`（`Random(30)==0` 才选） | 鹿、羊 | 自有（未覆盖） |
| 82 | 10 | 4 | `SpitSpider` | 剧毒蜘蛛、毒蜘蛛 | `AtMonster.Run`（未覆盖 race） |
| 116 | 8 | 1 | `SpiderHouseMonster` | 幻影蜘蛛 | 自有（未覆盖） |
| 84 | 6 | 2 | `Scorpion` | 蝎子、蝎子王 | `AtMonster.Run`（未覆盖 race） |
| 85 | 5 | 1 | `StickMonster` | 食人花 | 自有（未覆盖） |
| 115 | 4 | 2 | `BigHeartMonster` | 千年树妖、赤月恶魔 | 自有（未覆盖） |
| 102 | 3 | 1 | `ScultureKingMonster` | 祖玛教主 | 自有（未覆盖） |
| 51 | 2 | 1 | `MonsterObject`（Animal 初始化式） | 鸡 | 无覆写 |
| 53 | 2 | 2 | `AtMonster`（AnimalWolf） | 地狱犬、狼 | 已覆盖 |
| 106 | 2 | 1 | `GasDungMonster` | 粪虫 | `AtMonster.Run`（未覆盖 race） |

> **不可达说明**：`MonGen` 的 207 个名字映射出的 31 个 race 里，`215` 从未出现；在线 MySQL
> `mir2_data.monsters`（705 行）里也没有 `Race=215` 的行。其余不在上表的 race（`200/201/203/206/208/209/210/214/110/111/113/114/
> 117/118/120/130/150/…`）在本部署件的 `MonGen` 名字集里均无映射；`110/111`（门/墙）由**城堡系统**
> 另行放置，不经 `MonGen`。

---

## 3. 缺口与优先级

### 3.1 排序：真机可生成 × 有 RNG/帧差异

排序键：① 真机可生成条目数；② `Run` 体内/紧邻路径有 RNG 或帧；③ 行为差异量级。

> 本节排序轴记为 **prio-1/prio-2/prio-3**，与 §4.2 的实现期 **P0–P5** 区分（同号不同义）。

| 优先级 | race（条目） | 缺口 | RNG/帧 | 依据 |
|---|---|---|---|---|
| **prio-1** | 87/93/104（237） | `DualAxeMonster.Run`：5s 目标扫描、2 格回退、`Random(5)` | **RNG** | `DualAxeMonster.cs:84-137`；`ThornDarkMonster`/`ArcherMonster` 继承 |
| **prio-1** | 94（47） | `LightingZombi.Run`：索敌 + 2 格回退 + `LightingAttack` | **RNG** | `LightingZombi.cs:32-58` |
| **prio-1** | 96（120） | `ZilKinZombi.Run` 复活 + `AtMonster.Run` 索敌未覆盖 | 帧 `RM_ALIVE`；无 RNG | `ZilKinZombi.cs:35-47`、`ATMonster.cs:13-24` |
| **prio-1** | 103（98） | `BeeQueen.Run`：`SearchTarget` + 子蜂生成 | 帧 `RM_HIT`、`RM_ZEN_BEE`(500ms) + 生成抽 | `BeeQueen.cs:47-74` |
| **prio-2** | 101（338） | `ScultureMonster.Run`：石化融化 + 索敌 | 无 RNG/帧 | `ScultureMonster.cs:43-79` |
| **prio-2** | 105（279） | `GasMothMonster.Run`：`GasMothAttack` 索敌 | 攻击路径 `Random(3)` | `GasMothMonster.cs:17/24-35` |
| **prio-2** | 107（19） | `CentipedeKingMonster.Run` + `StickMonster.Run`：出土/缩地/索敌 | 无 | `CentipedeKingMonster.cs:91-143` |
| **prio-2** | 95（45） | `DigOutZombi.Run`：出土/索敌 | 无 | `DigOutZombi.cs:18-55` |
| **prio-2** | 92（18） | `CowKingMonster.Run`：瞬移/狂暴 + `AtMonster.Run` 索敌 | `MapRandomMove` 抽 | `CowKingMonster.cs:40-94` |
| **prio-2** | 116（8） | `SpiderHouseMonster.Run`：`SearchTarget` + 子蛛生成 | 帧 `RM_HIT`、`RM_ZEN_BEE`(500ms) + 生成抽 | `SpiderHouseMonster.cs:51-78` |
| **prio-2** | 115（4） | `BigHeartMonster.Run`：直接 `AttackTarget()` | 无 | `BigHeartMonster.cs:44-54` |
| **prio-2** | 52（13） | `ChickenDeer.Run`：逃跑/目标选择（含 `baseObject` 变量 bug） | 无 | `ChickenDeer.cs:13-63` |
| **prio-2** | 85（5） | `StickMonster.Run`：出土/索敌/缩地 | 无 | `StickMonster.cs:101-149` |
| **prio-2** | 102（3） | `ScultureKingMonster.Run`：石化融化/索敌/召唤 | `CallSlave` 抽 | `ScultureKingMonster.cs:61-115` |
| **prio-2** | 112（21+57） | `ArcherGuard.Run`：主动扫 PvP/怪并攻击 | 攻击帧 | `ArcherGuard.cs:39-91` |
| **prio-3** | 215（**0**） | `FireballMonster.Run` 全链 | **RNG + 3 帧 + 600ms 延迟** | §4；**不可达**，仅 GM/自定义库可注入 |
| **prio-3** | 110/111 | `CastleDoor.Run`/`WallStructure.Run`（城堡放置） | `RM_TURN`/`RM_DIGUP` | 需城堡场景，非 `MonGen` |
| **prio-3** | 113/114、200/201/203/206/208/209/210/214/117/120/130/118 | 未在部署 `MonGen` 出现 | 多数有 RNG/帧 | 先补数据可达性再谈 |

> **另一条独立缺口（与具体怪无关）**：`is_active_seek_race` 只列了 6 个 race，但
> `CreateMonster` 映射到 `AtMonster` 家族的 race 共 20 个：
> `{53,81,82,83,84,86,88,89,90,91,92,96,97,100,105,106,114,118,119,130}`。
> 其中**真机可生成**且当前**不在** Rust 门内的有
> `82/84/90/91/92/96/97/100/105/106/119`（11 个 race；按 §2.4 表合计 **567** 条 MonGen 条目：
> 10+6+25+18+18+120+58+0+279+2+31；其中 105 占 279、100 为 0）
> ⇒ 这些怪在 Rust 侧不会主动索敌（仅被攻击后由 `animal_struck` 取得目标）。
> 修法：把 `is_active_seek_race` 扩到「所有映射到 `AtMonster` 且自身 `Run` 不提前截断的 race」，
> 或把 `ATMonster.Run` 的索敌门拆成独立 `at_monster_run` 并在各派生类 `base.Run()` 位置调用；
> 后者与 §4 的「类体 + 基类链」结构一致，建议采用。

### 3.2 `FireballMonster` 的可达性

**结论：当前部署件不可达（content-unreachable）。**

证据链（只读）：
1. `CreateMonster` 只在 `case 215` 构造 `FireballMonster`（`WorldServer.MonGen.cs:725-727`）；
   race 来自 `MonsterList[名字].Race`（`ApplyMonsterAbility`，`WorldServer.MonGen.cs:882-887`）。
2. 部署 `MonGen.txt` 的 2827 条有效记录、207 个名字中，**没有**任何名字映射到 race 215；
   部署库（MySQL `mir2_data.monsters`）在线只读复核 705 行，`Race=215` **0 行**。
3. 全 `E:/MirServer/M2GameSvr/Envir` 的脚本面（`Market_Def`/`QuestDiary`/`Robot_def` 等）
   的 `MOBPLACE`/`MONGENEX` 共 30 个怪物名，**没有**火球类名字；全文检索 `火球`/`Fireball`
   只命中商品/书页/掉落文件，不命中怪物名。
4. 部署 `Data.mdb` 只有系统表与 `TBL_GABOARD`/`TBL_ITEMMARKET`，**没有 `monsters` 表**，不是怪物数据源；
   部署 GameSrv 的怪物行来自 MySQL `mir2_data`（`Server.conf` 的 `[DataBase] DbType=MySQL`，
   `MySqlDB.LoadMonsterDB` = `select * from monsters`），并已在线只读复核（705 行、`Race=215` 0 行）。
   若将来在线库出现 `Race=215` 的自定义行，可达性只经 **GM 命令按名生成** 或该行被 `MonGen` 引用——
   这两者都不在当前数据里。

⇒ **实现 Fireball 改变生产行为为 0（除非注入）**；其价值是闭合 `openmir2-spell-frame-bypass-paths`
的登记面与保证「一旦将来内容出现该怪，RNG/帧序已 1:1」。判级仍为 **R**（RNG + 时序 + 帧面）。

---

## 4. 实现设计（分期）

### 4.1 P1（最小第一步）：`FireballMonster.Run` 全链

#### 4.1.1 oracle 逐行语义（`FireballMonster.cs:14-62`）

```text
if (Death || Ghost) skip
if (TargetCret != null):
    if MagCanHitTarget(TargetCret.CurrX, TargetCret.CurrY, TargetCret):      // :20（传目标自身坐标）
      if IsProperTarget(TargetCret):                                        // :22
        if |TargetX-CurrX|<=8 && |TargetY-CurrY|<=8:                        // :24（用上一 tick 的 TargetX/Y）
          nPower = LoByte(DC) + Random(|HiByte(DC)-LoByte(DC)|+1);          // :26  ← 抽 #1
          if nPower > 0:
            pose = GetPoseCreate();                                         // :29  前方格
            if pose != null && IsProperTarget(pose) && AntiMagic > 0:       // :30
              nPower = pose.GetMagStruckDamage(this, nPower);               // :32  ← 抽 #2（pose 的 MAC）
              if nPower > 0:
                pose.StruckDamage(nPower);                                  // :35  ← 抽 #3（玩家耐久）
                if (now - SpellTick) > NextHitTime:                         // :36
                  SpellTick = now;
                  SendRefMsg(RM_SPELL, 48, tx, ty, 48, "");                 // :39
                  SendRefMsg(RM_MAGICFIRE, 0, MakeWord(2,48), MakeLong(tx,ty), targetId, ""); // :40
                  SendStruckDelayMsg(RM_DELAYMAGIC, nPower, MakeLong(tx,ty), 2, targetId, "", 600); // :41
    BreakHolySeizeMode();                                                   // :49（有目标即执行）
else: TargetCret = null
if (now-SearchEnemyTick>8000 || (now-SearchEnemyTick>1000 && TargetCret==null)):
    SearchEnemyTick = now; SearchTarget();                                  // :55-59
base.Run();                                                                 // :61
```

#### 4.1.2 关键事实（必须逐条复刻，不许「修 bug」）

1. **`MagCanHitTarget` 传的是目标自身坐标**：`n20 = 0`；第一轮 `GetNextDirection(x,y,x,y)`
   返回 `Down`（`SystemShare.cs:357` 的兜底），`GetNextPosition` 的 Down 支用 **`Width`** 判 y
   （`Envirnoment.cs:1110`），若前进一步则 `n1C=1>0` ⇒ **立刻 true**。
   实际上只在「目标下方一步不可前进（`y >= Width-1`）」或「该格 `GetCellInfo.success &&
   Attribute==LowWall`」时返回 false。**必须原样实现**（不要改成攻方到目标的视线）。
2. **`IsValidCell` 的「越界算合法」**：`GetCellInfo` 越界/`!Valid` ⇒ `success=false`
   （`Envirnoment.cs:322-335`），而 `IsValidCell` 只在 `success && Attribute==LowWall` 时 false
   （`:1422-1430`）⇒ **越界与无效格都算 valid**。
3. **距离门用 `TargetX/TargetY`，不是 `TargetCret.CurrX/CurrY`**；且 `TargetX` 初值 -1。
   正常索敌（`SearchTarget`）只置 `TargetCret`、不置 `TargetX/Y`；要等基类 `AttackTarget()` 的
   「不邻接」支 `SetTargetXy` 或 `GotoTargetXY` 才更新。⇒ **刚锁定目标的第一个 tick 常因
   `|TargetX-CurrX|>8` 跳过魔法块**；邻接目标的 `TargetX` 可能永远停在 -1（`AttackTarget` 邻接支
   不写 `TargetX/Y`）⇒ **邻接时可能永不施法**。这是 oracle 的真实行为。
4. **伤害打的是 `GetPoseCreate()`（前方一格）而不是 `TargetCret`**：`GetPoseCreate` =
   `GetFrontPosition` + `GetMovingObject(nX,nY,true)`（`BaseObject.cs:1253-1262`）；
   帧却引用 `TargetCret` 的坐标/id。⇒ 「伤害对象」与「帧目标」可能不同。
5. **`AntiMagic > 0` 是硬门**：正常 `MonGen` 生成的怪 `AntiMagic` 保持 `BaseObject` ctor 的 0
   （`BaseObject.cs:390`），只有 `RecalcAbilitys()` 会置 1（`BaseObject.Abil.cs:30`），而它只在
   **宝宝召唤**（`BaseObject.cs:1198`）与**复活**（`:2739`）等路径被调用。⇒ **普通生成的
   FireballMonster 永远不进入伤害/帧块**；但 **`nPower` 抽（#1）仍然发生**（在 `AntiMagic` 门之前）。
6. **`SpellTick` 与恢复累加器同名共用**（`BaseObject.cs:288` 的 `int SpellTick`）：
   - ctor `SpellTick = GetTickCount()`（`FireballMonster.cs:10`）；
   - `BaseObject.Run` 每 tick `SpellTick += recoveryTick`，且 `SpellTick >= SpellFillTime(800)`
     时 **归零**（`BaseObject.Base.cs:42/78-80`）；
   - 施法支把 `SpellTick = now`（`FireballMonster.cs:38`），同一 tick 的 `base.Run()` 立即把它
     归零 ⇒ **下一 tick 的 `(now - 0) > NextHitTime` 恒真**（只要 `now > 2000`）。
   ⇒ **施法冷却实际是「一 tick」而非 `NextHitTime`**；这是必须复刻的 bug。
7. **`BreakHolySeizeMode()` 无条件执行**（有目标即执行，`FireballMonster.cs:49`），
   即使伤害块全部跳过；它把 `HolySeize=false` 并发 `RM_CHANGENAMECOLOR`（`BaseObject.cs:730-733`）
   ⇒ **每 tick 一条名色帧**。
8. **两个 AI pass 与 `MagicMonster.AttackTarget`**：
   `FireballMonster.Run` 的 `base.Run()` 是 `MagicMonster.Run`，它跑完自己的 AI 后
   `base.Run()` 又进 `MonsterObject.Run`。两 pass 都通过虚分派调用
   `MagicMonster.AttackTarget`（**不造成物理伤害**，只刷新 `AttackTick/TargetFocusTick` 并返回 true）。
   `MagicMonster.Think` 无 `HolySeize` 早退；`MonsterObject.Think`（private）有。
   ⇒ 不能简单用现有 `monster_run` 一次替代；要按「MagicMonster pass → MonsterObject pass」两段实现。
9. **`GetMagStruckDamage` 的取数**（`BaseObject.Base.cs:812-821`）：
   `n14 = LoByte(MAC) + Random(|HiByte(MAC)-LoByte(MAC)|+1)`；
   `nDamage = (ushort)max(0, nDamage - n14)`；若 `LifeAttrib==LA_UNDEAD` **且 `baseObject != null`**
   加 **防御者自己的** `AddAbil.UndeadPower`（不是 `nDamage>0` 门；与 `GetHitStruckDamage` 加**攻击方**的不同）。
   注意：`RM_MAGSTRUCK` 泵传 `null`（`:624`）⇒ 该加项不发生；`RM_DELAYMAGIC` 传 `this`（`:717`）⇒ 发生。
10. **`StruckDamage` 的取数**（目标为玩家时）：`PlayObject.StruckDamage`（`:2954-3036`）
    先 `base.StruckDamage`（倍率 + `DamageHealth`，无抽），再 `Random(10)+5`（`:2957`），
    衣服位特判（不抽），再对每个 `Index>0` 的装备格 `Random(8)`（`:2998-3028`）。
    怪物目标只走 `BaseObject.StruckDamage`（`:827-857`，无抽）。

#### 4.1.3 600ms 延迟消息的精确语义

`SendStruckDelayMsg(RM_DELAYMAGIC, nPower, MakeLong(tx,ty), 2, targetId, "", 600)`
走 7 参重载（`BaseObject.Message.cs:211-229`）：入**怪物自己的** `MsgQueue`，
`DeliveryTime = now + 600`，`nParam3 = targetId`，优先级 `Lowest=255`。

到点后由**怪物自己的 `base.Run()` 消息泵**处理（`BaseObject.Base.cs:711-729`）：

```text
RM_DELAYMAGIC:
  nPower = wParam; (tx,ty) = LoWord/HiWord(nParam1); rage = nParam2;
  target = ActorMgr.Get(nParam3);
  if target != null && target.GetMagStruckDamage(this, nPower) > 0:   // ← 抽 #4（目标 MAC）
      SetTargetCreat(target);
      if target.Race >= Animal: nPower = Round(nPower / 1.2);
      if |tx-target.CurrX| <= rage && |ty-target.CurrY| <= rage:
          target.SendMsg(this, RM_MAGSTRUCK, 0, nPower, 0, 0);        // 入目标队列，ActorId=怪物
```

目标（通常是玩家）在自己的 `Run` 里处理 `RM_MAGSTRUCK`（`BaseObject.Base.cs:618-686`）：

```text
if wIdent == RM_MAGSTRUCK && Race >= Animal && !RushMode && Level < 50:
    WalkTick += 800 + Random(1000);               // ← 抽 #7（case 第一条语句，:622；在 :624 之前、if 之外）
nDamage = GetMagStruckDamage(null, nParam1);      // ← 抽 #5（目标 MAC 再来一次）
if nDamage > 0:
    StruckDamage(nDamage);                        // ← 抽 #6（玩家耐久）
    HealthSpellChanged();                         // → SM_HEALTHSPELLCHANGED(53)
    SendRefMsg(RM_STRUCK_MAG, nDamage, HP, MaxHP, processMsg.ActorId /*怪物*/);
    // 若 Race > Play 还会 SendMsg(RM_STRUCK,…) + 肉质扣减
```

`SendRefMsg(RM_STRUCK_MAG, …)` 的观察者支（`PlayObject.Message.cs:1562-1618`）：
`wParam>0`；`ActorId==自己` 时 `SetLastHiter`、`HealthTick=SpellTick=0`、`PerHealth--`/`PerSpell--`；
然后对自己与 ±12 内玩家发 `SM_STRUCK(…, Tag1=hitter, Tag2=1)`。

**抽取总账（一次成功施法、目标为在线玩家）**：
`#1 DC` → `#2 目标 MAC` → `#3 Random(10)+每格 Random(8)` →
（600ms 后）`#4 目标 MAC` → `#5 目标 MAC` → `#6 Random(10)+每格 Random(8)`。
若目标是宠物/怪：`#3/#6` 不发生；`RM_MAGSTRUCK` case 的 `#7 WalkTick` 抽（`:622`）位于
`#5`（`:624`）**之前**且在 `if nDamage > 0` **之外**，故抽数序是 **#4 → #7 → #5**
（不是 #5→#6→#7）；`#7` 只在 `wIdent == RM_MAGSTRUCK && Race >= Animal && !RushMode &&
Level < 50` 时发生（`Level >= 50` 的怪不抽）。

#### 4.1.4 需要新增/复用的世界接口

**新增字段（`crates/world/src/entity.rs`）**：
- `Entity.mac: i32`（`WAbil.MAC`；怪物来自 `MonsterInfo.mac`，玩家来自 `Wabil.mac`）；
- `Entity.anti_magic: u16`（`BaseObject.AntiMagic`；默认 0；`RecalcAbilitys` 等价路径置 1+add）；
- `AnimalState.delayed_msgs: Vec<DelayedMsg>`：
  `{ w_ident, w_param, n_param1, n_param2, n_param3, delivery_time }`（按入队序；
  处理时只取队首，未到点即停——复刻 `GetMessage` 的 `:334-355` 语义）；
- 复用 `Entity.spell_tick`（i32；见 4.1.2 第 6 条）；`seed_from_spawn` 对 race 215
  `spell_tick = now as i32`（ctor 赋值，**不抽数**；加法按 C# `int` 用 `wrapping_add`）；
- `Entity.view_range = 8` for race 215（`MagicMonster.cs:11`）。

**新增纯函数/helper（`crates/world/src/monai.rs` / `combat.rs` / `magic.rs`）**：
- `magic::get_next_position(sx, sy, dir, flag, width, height) -> Option<(i32,i32)>`：
  逐字复刻 `Envirnoment.cs:1097-1157`（含 Down 用 `Width`、UpRight 用 `Height` 等怪边界）；
- `monai::is_valid_cell(world, x, y) -> bool`：`!(success && attribute==LowWall)`。
  **实现注意**：不能用 `MapGrid::get_cell_info(..).1` 代替 C# 的 `success`——Rust 的 `.1`
  已经折叠了 `attribute==Walk`（等于 C# `CanWalk`），而 C# `success` 对 LowWall/HighWall 也为真。
  需新增按「越界/属性」直读的访问器（或 `cell_match + attribute`）；否则 **LowWall 格会被误判为
  不可达**，与本条的 ③ 分支相反。
- `monai::mag_can_hit_target(world, x, y, target_id) -> bool`：13 轮循环 + `next_direction` +
  `get_next_position` + `is_valid_cell`；
- `monai::get_pose_create(world, actor_id) -> Option<i32>`：`front_position` +
  `GetMovingObject(..., true)`（首个非 Ghost、`!Death`；CastleDoor 的 `HoldPlace` 分支登记不建模）；
- `combat::get_mag_struck_damage(rng, defender, n_damage, hitter_present) -> i32`
  （`hitter_present` 对位 oracle 的 `baseObject != null`；`RM_MAGSTRUCK` 泵传 false，`RM_DELAYMAGIC`/即时路径传 true）；
- `combat::struck_damage(world, target_id, n_damage, hiter_id, config)`：复用
  `struck_damage_multiplier` + `damage_health` + `struck_damage_durability`；
- `monai::break_holy_seize_mode(world, actor_id)`：置 `holy_seize=false` 并记一条
  `RM_CHANGENAMECOLOR` 事件（**无条件**，不判之前是否为 true）。

**新增 AI 入口（`crates/world/src/monai.rs`）**：
- `fireball_monster_run(world, id)`：4.1.1 的魔法块 + 索敌门；
- `magic_monster_run(world, id)`：`MagicMonster.Run` 的类体（含 `think_magic` 与
  `magic_attack_target`），在两个提前返回口调 `monster_run`；
- `monster_run` 增加内部 `attack_kind`（Normal/Magic）或 `monster_run_magic` 包装，
  使 `MonsterObject.Run` 的虚分派 `AttackTarget` 命中 `magic_attack_target`；
- `monster_ai_tick` 分派：`race == 215` ⇒ `fireball_monster_run`；其他保持原路。

**复用（已存在）**：`is_proper_target`、`search_target`、`set_target_creature`、
`wondering`/`goto_target_xy`/`walk_to`/`turn_to`、`damage_health`、`hutil::round`、
`combat::lo_byte/hi_byte`、`hutil::make_long_signed/unsigned`、`mir2_shared::rng::RandomSource`。

#### 4.1.5 对既有 `monai` 通用 Run 的影响边界

- `monster_run` 的默认路径（`attack_kind=Normal`）**逐字节不变**；race 215 不在
  `is_active_seek_race`，也不会被现有路径命中。
- `base_object_run_monster` 增加「队首延迟消息未到点即停」的处理；对空队列实体零行为差异。
- 新增字段默认值（`mac=0`、`anti_magic=0`）不改变任何既有读点（当前无读点）。
- `seed_from_spawn` 的 race 215 分支只影响 race 215。
- `apply_ability_to_entity` 增加 `e.mac = i32::from(wabil.mac)`：只喂新函数，不改现有 golden。
- 该片**不**改 `crates/protocol`、`crates/storage`。

#### 4.1.6 宿主接线（`bins/game-svr`）

1. `TickRecord` 新增（或在既有泛型帧记录上扩展）：
   - `monster_magic_frames: Vec<MonsterMagicFrame>`：
     `Spell { actor_id, x, y, effect, magic_id }` / `MagicFire { actor_id, x, y, effect_type,
     effect, target_id }` / `NameColor { actor_id }`；
   - `magic_struck: Vec<(victim_id, damage, hp_after, max_hp, hitter_id)>`（`mag=true`）。
2. `World::tick` 把 `pending_*` 转抄进 `TickRecord`（与 `pending_ai_attacks` 同法）。
3. 新 drain 函数（建议 `drain_monster_magic_downlink`）：
   - 对每帧按 `SendRefMsg` 收件人集合（`chat::hear_targets(..., SEND_REF_MSG_RANGE)`）灌
     `NetPriorityQueue`，优先级字节 = `RM_SPELL & 0xFF`(24) / `RM_MAGICFIRE & 0xFF`(136) /
     `RM_CHANGENAMECOLOR & 0xFF`(154)，再出队编码：
     `sm_spell(actor,x,y,48,48)`、`sm_magicfire(actor,x,y,2,48,target)`、
     `sm_changenamecolor(actor, MONSTER_NAME_COLOR)`；
   - `SM_SPELL` 排除施法者自身（`ActorId != ActorId` 守卫）；`SM_MAGICFIRE` **不**排除
     （`PlayObject.Message.cs:1788-1793` 无守卫；怪物无会话 ⇒ 实际同集合）。
   - `magic_struck` 走 `sm_struck(..., mag=true)`，并对在线玩家做与
     `drain_combat_downlink` 相同的会话 HP 同步/`on_struck` 清零；随后按需发 53。
4. **不在宿主实现延迟**：600ms 的到点判定与 #4/#5 抽数必须在 world 内完成（单线程、确定时钟）；
   宿主只发 world 已判定的事件。

#### 4.1.7 判级

- **本设计件**：L（纯文档）。
- **P1 实现片**：**R**。理由：① RNG 逐次流水（#1–#6）；② 时序（600ms/`SpellTick` 归零）；
  ③ 帧面（`SM_SPELL`/`SM_MAGICFIRE`/`SM_CHANGENAMECOLOR`/`SM_STRUCK` 的收件人、顺序、字段）。
  真机判据未绿时必须 `request_changes` 或 `approve` + `"window_required": true`。

### 4.2 分期

| 期 | 内容 | 判级 | 依赖 |
|---|---|---|---|
| **P0** | 本设计件（清册/可达性/接口/判据） | L | — |
| **P1** | `FireballMonster.Run` + `MagicMonster` 两 pass + 延迟队列 + 三帧 + `SM_STRUCK(mag)`；`Entity.mac/anti_magic` | **R** | 本件 §4.1 |
| **P2** | 通用延迟消息（`RM_ZEN_BEE` 复用同一队列）+ `BeeQueen`/`SpiderHouseMonster.Run` | **R** | P1 的队列 |
| **P3** | 真机可生成子类 `Run`：`DualAxe`/`ThornDark`/`ArcherMonster`、`LightingZombi`、`ZilKinZombi`、`Sculture`、`GasMoth`、`DigOutZombi`、`CowKing`、`CentipedeKing`、`StickMonster`、`ScultureKing`、`BigHeart`、`ChickenDeer`、`ArcherGuard`；并修 `is_active_seek_race` 缺口 | **R**（RNG/时序） | P1 的两 pass/接口 |
| **P4** | 不可达类（`Elf`、`Khazard`、`Green`、`Red`、`Fire`、`Explosion`、`SoccerBall`、`Tele`、`FrostTiger`、`Clone`、`ElectronicScolpion`、`MagicMonObject`、`BoneKing`、`Ron`）与城堡门/墙 | L（等价 + 抽样冒烟） | 数据可达性/城堡场景 |
| **P5** | 机器人 `AutoSpell` 旁路（`RobotPlayer.cs:1846/1856`） | 不立项（`RobotProcessor` 未移植） | 机器人里程碑 |

> P3 的结构建议：把「类体 + `base.Run()` 链」显式化——例如
> `at_monster_run(world,id)`（`ATMonster.Run` 的索敌门）与 `monster_object_run(world,id)`
> 分开；每个子类入口按 oracle 顺序调用。这样不会把「共用门」误当成「所有 race 都有」。

---

## 5. 机判验收 + 改坏必红

测试落点建议：`crates/world/src/monai.rs` 的 `#[cfg(test)]`（或 `crates/world/tests/`）。
「连续若干抽」统一用 `CountingRng`（同种子 `RandomNumber` + 记录每次 `random_below` 入参，
仓内已有同型：`combat.rs` 测试的 `CountingRng`）。集成用例用固定种子 + 固定 `world.clock_ms`
（建议 `100_000`，避开 `now <= NextHitTime` 的启动窗口）。

### A1 `fireball_power_draw_arg_and_value`

- **oracle**：`FireballMonster.cs:26`：`LoByte(DC) + Random(|HiByte(DC)-LoByte(DC)|+1)`。
- **夹具**：`dc = 0x0A14`（Lo=20、Hi=10，`|10-20|+1=11`）；脚本 RNG 使 `Random(11)=5`。
- **断言**：`CountingRng.below == [11]`；返回 `25`；再一次调用 ⇒ 第二个 `11`。
- **改坏必红**：把 `|Hi-Lo|+1` 改成 `Hi-Lo`（负数 panic）、改成 `Lo-Hi+1`、或把 `LoByte` 换成
  `HiByte` ⇒ 入参/结果断言红。

### A2 `get_mag_struck_damage_draws_mac_once_and_adds_defender_undead_power`

- **oracle**：`BaseObject.Base.cs:812-821`。
- **夹具**：防御者 `mac = 0x050A`（Lo=10、Hi=5 ⇒ 入参 6），脚本 RNG 给 `Random(6)=2`；
  `n = 30` ⇒ `n14=12` ⇒ `18`；再令 `life_attrib=1`、`undead_power=3`、**`hitter_present=true`** ⇒ `21`
  （`BaseObject.Base.cs:816` 还有 `baseObject != null` 门）；另测 `n=0` ⇒ `3`（只跳过 `nDamage>0`，
  不是跳过 `baseObject != null`）。
- **断言**：`below == [6]`（每次调用恰一抽）；`30→18`；`30→21`；`0→3`；`hitter_present=false` 时
  `30→18`、`0→0`（对位延迟泵 `RM_MAGSTRUCK` 的真实路径）。
- **改坏必红**：把 undead 加项挪进 `nDamage > 0` 门（第三断言红）；误用攻击方的 UndeadPower
  （第二/第三断言红）；漏掉 MAC 抽（`below` 空、`n14` 变 10）。

### A3 `fireball_cast_draw_order_dc_mac_struck_durability`

- **oracle**：`FireballMonster.cs:26/32/35` + `PlayObject.cs:2957/2998-3028`。
- **夹具**：`dc=0x0A14`、目标玩家 `mac=0x050A`、2 个 `Index>0` 的装备格（衣服 + 武器）；
  `anti_magic=1`、`pose=目标`、`spell_tick` 已过期。
- **断言（连续 5 抽）**：共享 `CountingRng` 依次调用
  `fireball_power` → `get_mag_struck_damage` → `struck_damage_durability`，
  `below == [11, 6, 10, 8, 8]`（DC、MAC、`Random(10)`、每格 `Random(8)`）。
  集成断言：目标 HP 减少恰为 `nPower`（经倍率后）；`magic_struck` 为空（即时伤害不出帧）。
- **改坏必红**：把 MAC 抽放到 `StruckDamage` 之后 ⇒ 次序红；漏掉 `Random(10)` ⇒ 长度/次序红；
  对每个格子不论 `Index>0` 都抽 ⇒ 多抽。

### A4 `delayed_magic_fires_only_at_600ms_and_draws_mac_twice`

- **oracle**：`BaseObject.Message.cs:222/339-342`（`now < DeliveryTime` 即重入队、返回 false）+
  `BaseObject.Base.cs:711-729` + `:624/627/629`。
- **夹具**：施法在 `t=100_000` 成功 ⇒ 队列项 `delivery=100_600`；目标为玩家。
- **断言**：
  1. `t=100_599` 调 `base_object_run_monster` ⇒ 队列项仍在、目标 HP 不变、`magic_struck` 空、
     零新抽；
  2. `t=100_600` ⇒ 恰一条 `magic_struck`；`CountingRng` 记录到 **#4 目标 MAC**、**#5 目标 MAC**、
     **#6 `Random(10)`+每格 `Random(8)`**；队列清空。
- **改坏必红**：把 `now < delivery` 改成 `<=`（599 就发）；把 600 改 500/700；只做 #4 不做 #5
  ⇒ 抽数与 HP 双红。

### A5 `anti_magic_zero_skips_damage_but_still_draws_dc_and_breaks_holy_seize`

- **oracle**：`FireballMonster.cs:26`（抽在门前）、`:30`（`AntiMagic > 0`）、`:49`
  （`BreakHolySeizeMode` 无条件）。
- **夹具**：普通生成态 `anti_magic=0`、有目标、`TargetX/Y` 合法、前方有玩家。
- **断言**：目标 HP 不变；无 `Spell/MagicFire` 帧、无 `magic_struck`；`below == [11]`（仅 DC 抽）；
  `holy_seize == false`；产生 1 条 `NameColor` 事件。
- **改坏必红**：删 `anti_magic > 0` 门 ⇒ HP/抽数红；把 `BreakHolySeizeMode` 挪进伤害块 ⇒
  `holy_seize`/`NameColor` 红；把 DC 抽挪到 `AntiMagic` 门后 ⇒ `below` 空。

### A6 `mag_can_hit_target_reproduces_target_coords_and_is_valid_cell_quirks`

- **oracle**：`BaseObject.cs:2059-2092`（传目标自身坐标）+ `Envirnoment.cs:1097-1157/1422-1430`。
- **夹具**：32×32 图；`x=10`。
  ① 目标 `(10,30)`：`Down` 可进 ⇒ true；
  ② 目标 `(10,31)`：`31 < 32-1` 为假 ⇒ false；
  ③ 目标 `(10,10)`、`(10,11)` 为 `LowWall` ⇒ false；
  ④ 目标 `(10,31)` 且 `(10,32)` 越界 ⇒ `IsValidCell` 视为 true，但因 ② 的 `GetNextPosition`
     返回 false 仍为 false（两个怪语义叠加）。
- **改坏必红**：把入口改成「攻方坐标 + 目标」的视线算法 ⇒ ②/③ 的期望翻转；把
  `IsValidCell` 改成「越界=非法」⇒ ④ 分支红。

### A7 `magic_monster_runs_second_pass_after_think_true`

- **oracle**：`FireballMonster.cs:61` → `MagicMonster.cs:185` → `MonsterObject.cs:255-258`。
- **夹具**：两个 actor 同格使 `Think` 的 `GetXYObjCount>=2` 置 `DupMode`；`think_tick` 过期；
  `CountingRng` 固定 `RandomByte(8)` 的方向，使第一次 `WalkTo` **成功**（`Think()` 返回 true）。
- **断言**：第一 pass 的 `base.Run()` 后，第二 pass（`MonsterObject.Run`）仍执行；最终坐标/攻击
  记录与「两次 pass」的金标准一致；抽数序 = 第一 pass 的 `RandomByte(8)` + 第二 pass 的
  `Wondering/Goto` 抽（若有）。
- **改坏必红**：只实现一个 AI pass（或 `Think` true 时直接 return）⇒ 坐标/抽数红。

### A8 `fireball_casts_every_tick_because_spell_tick_is_reset`

- **oracle**：`FireballMonster.cs:10/36-38` + `BaseObject.Base.cs:42/78-80`（`SpellFillTime=800`，
  `GameSvrConf.cs:1324`）。
- **夹具**：`clock=100_000` 起，连续两 tick（+200ms）；`anti_magic=1`、目标/前方对象稳定。
- **断言**：两 tick 各产生一对 `Spell+MagicFire`（**不是** `NextHitTime=2000` 冷却）；两 tick 各
  消耗一次 DC 抽（`below == [11, 11]`）；每 tick 的 `spell_tick` 在 `base_object_run_monster`
  后回到 0。
- **改坏必红**：把 `SpellTick` 拆成独立冷却字段并按 `NextHitTime` 门 ⇒ 第二 tick 无帧 ⇒ 红；
  不执行 `SpellTick` 的恢复归零 ⇒ 第二 tick 门不过。

> 以上 8 条覆盖「连续若干抽」（A3 5 抽、A4 跨 600ms 的 3+ 抽、A8 跨 tick 2 抽）与
> 「收件人/延迟/门」三类。真机判据（R 级）另需窗口内两侧同种子 `rng_trace` + 帧字节对拍；
> 窗口判据不因本件而放宽。

---

## 6. 未决项

1. **延迟消息队列（600ms）**
   - oracle 用 `PriorityQueue<SendMessage, byte>`；延时消息入队优先级 `Lowest=255`
     （`BaseObject.Message.cs:227/250`），`GetMessage` 只检查**队首**：未到点即重新入队并
     `return false`（`:334-355`），因此「队首未到点」会**阻塞**它后面同为 `Lowest` 的其它延时消息（普通消息优先级字节更小，已先出队）。
     .NET `PriorityQueue` 同优先级元素的出队序是 4 叉堆序（仓内 `msgqueue::NetPriorityQueue`
     已复刻）；**多个延时消息同优先级时的顺序**未在 oracle 文档化，属未决。
   - `RunTime` 节拍未在 Rust 建模：oracle 每只怪 `Run()` 受 `(now-RunTick)>RunTime(250)`
     门控（`WorldServer.MonGen.cs:280-282`），而 Rust `monster_ai_tick` 每 200ms tick 都跑
     ⇒ **600ms 到点判定可能比 oracle 早 0–200ms**。修法二选一：① 给 `Entity` 补 `run_tick`
     并复刻门；② 在 R 级窗口里把「到点 tick」作为判据显式对拍，先接受已知偏移。建议 ①。
2. **`GetMagStruckDamage` / `AntiMagic` 模型**
   - `Entity` 目前无 `mac`，`apply_ability_to_entity` 不写 MAC；玩家/怪物两侧都要补。
   - `AntiMagic` 正常生成恒 0（`BaseObject.cs:390`），只有 `RecalcAbilitys`（宝宝召唤
     `BaseObject.cs:1198`、复活 `:2739` 等）置 1。Rust 侧哪些路径算 `RecalcAbilitys`
     （`slave::summon_slave`/复活）必须**显式**写 `anti_magic=1`，否则与 oracle 的「普通生成
     不施法」一致；两类路径都要有用例钉住。
   - `GetMagStruckDamage` 的 undead 加项用**防御者自己的** `AddAbil.UndeadPower`，只受
     `baseObject != null` 门（`BaseObject.Base.cs:816-819`），不受 `nDamage>0` 门；`RM_MAGSTRUCK` 泵传
     `null` ⇒ 不加，`RM_DELAYMAGIC` 传 `this` ⇒ 加。不要复用物理版的「攻击方 undead_power + `damage>0` 门」。
3. **目标选择的全网语义**
   - `MagCanHitTarget` 传目标自身坐标 ⇒ 视线判定退化为「目标下方一格可前进且非 LowWall」；
     `IsValidCell` 越界算合法；`GetNextPosition` 的 Down 用 `Width`（`Envirnoment.cs:1110`）。
     实现时必须逐字复刻，不能「修正」。
   - `GetPoseCreate`（前方一格）与 `TargetCret` 可以不同；`GetMovingObject(...,true)` 对
     CastleDoor 的 `HoldPlace` 特判（`Envirnoment.cs:1217-1224`）未建模。
   - `TargetX/TargetY` 初值 -1、邻接 `AttackTarget` 不更新它 ⇒ 邻接目标可能永不施法（§4.1.2-3）。
   - `SendRefMsg` 的 `VisibleHumanList` **500ms 缓存**（`BaseObject.Message.cs:395-475`）：
     缓存命中支按 `<11` 复核、重算支按 ±12；Rust 现每 tick 现算 ±12 ⇒ 真机对拍可能差收件人。
4. **玩家目标的 `RM_MAGSTRUCK` 泵**
   - oracle 的延迟伤害最终入**玩家**的 `MsgQueue`，由玩家 `Run()` 的 `Operate(RM_MAGSTRUCK)`
     处理（`BaseObject.Base.cs:618-686`：`GetMagStruckDamage`/`StruckDamage`/`HealthSpellChanged`/
     `RM_STRUCK_MAG`）。
   - Rust 的 world tick 目前只驱动怪物 AI；玩家的 `BaseObject.Run` 等价物在宿主会话层
     （`recover`/命令路径），**没有通用的玩家消息泵**。P1 必须明确：
     - 方案 A（推荐）：在 `crates/world` 增加 `base_object_run_player`（消息泵 + 恢复 +
       `TargetCret` 清理），由宿主每 tick 对在线玩家调用；`RM_MAGSTRUCK` 入玩家
       `AnimalState.delayed_msgs`（或独立 `pending_magic_struck`），在泵里处理。
     - 方案 B：world 只记录「到点事件」，宿主在自己的玩家 tick 里处理并回帧；缺点：抽数
       在宿主侧（不再是 world 的确定性 RNG），**不推荐**。
   - 该泵的调用序（玩家 vs 怪物）会影响共享 RNG 的跨侧流水；R 级窗口必须固定顺序。
5. **两个 AI pass / 虚分派**
   - `MagicMonster.Run` + `MonsterObject.Run` 的双 pass、`MagicMonster.Think`（无 HolySeize 门）、
     `MagicMonster.AttackTarget`（不伤害、只刷 tick）都要显式建模；不能用现有 `monster_run`
     一次替代（§4.1.2-8）。
6. **`SpellTick` 双语义 / i32 时钟**
   - `SpellTick` 既是回蓝累加器又是施法冷却戳（§4.1.2-6）；复用 `Entity.spell_tick`
     是 1:1 的唯一简单方案。加法用 `wrapping_add`（C# `int` 溢出回绕）。
   - 真机 `GetTickCount()` 是 ~10^8 ms 量级；Rust 测试时钟若从 0 起，`now > NextHitTime` 的
     启动窗口不同 ⇒ 夹具统一 `clock_ms=100_000`，并登记「真实开机时刻」窗口项。
7. **`RM_CHANGENAMECOLOR` 的帧面**
   - `BreakHolySeizeMode` 每 tick（有目标）都 `RefNameColor`，不是「状态变化才发」；需按
     `SM_CHANGENAMECOLOR(656)` + `NetPriorityQueue` 优先级 154 出帧。
   - 与既有 `drain_slave_mutiny_downlink` 的 `MutinyFrame::NameColor` 同编码器，但**入队源不同**
     （怪物 AI vs 叛变宝宝），不能共用同一批入队逻辑。
8. **可达性与数据缺失**
   - `FireballMonster` 当前不可达（§3.2）；部署怪物库 = MySQL `mir2_data`（`Server.conf` 的
     `[DataBase] DbType=MySQL`，`MySqlDB.LoadMonsterDB` = `select * from monsters`），已在线只读
     复核：`monsters` 705 行、`Race=215` 0 行。`MonGen.txt` 另有 13 个名字/76 条记录在库中
     **不存在**（`GetMonRace` 返回 -1 ⇒ `CreateMonster` 返回 null ⇒ 出生失败），不是「快照漂移」。
     P1 的「生产行为不变」结论依赖该在线复核；若在线库出现 `Race=215` 行，需按该行名字/地图重新评估。
9. **与 `openmir2-spell-frame-bypass-paths` 的解冻条件对应**
   - 旁路 ②（怪物硬编码 `RM_SPELL(48)`）：**P1 实现并通过 R 级窗口后**才可勾销；本件只是设计。
   - 旁路 ①（机器人 `AutoSpell`，`RobotPlayer.cs:1846/1856`）：仍被「`RobotProcessor` 未移植」
     阻塞（宿主已登记「不建机器人会话」）⇒ 两条旁路**不能一起**判清。
   - 结论：**在本件与 P1 落地之前，任何「帧面缺口已全清」的结论都不成立**（与 runbook §2f #9 一致）。
10. **`view_range=8`**：`MagicMonster` ctor 置 8（`MagicMonster.cs:11`）；Rust 怪物缺省 5。
    AOI/`VisibleActors` 的窗口可能因此不同（尤其当该怪成为宝宝并主动 `SearchViewRange` 时）。
11. **`is_active_seek_race` 缺口**：见 §3.1 末段；与 Fireball 无关但同为「真机可生成 × 行为差异」，
    建议在 P3 一并修。
12. **未建模的通用分支**：`Mission`、`Master`/`SlaveRelax`、`RunAwayMode`（除 ChickenDeer/RunAway）、
    `CrazyMode`、`AnimalObject.Run` 的 HolySeize 到期、`CheckRoyalty` 的完整语义。Fireball 正常
    主为空 ⇒ 多数不可达；一旦接入宝宝系统需重新评估。

---

## 7. 残留物清单（§14.3）

- **仓内新增**：仅本件 `mir2-rs/tests/parity/m3/design-monster-run-overrides-20261017.md`。
- **仓内修改**：无。未动 `src/**`、`sql/**`、`MirClient/`、`crates/{protocol,storage}/**`。
- **worktree**：`/e/tmp/impl-monrun2`（detached `568b0a962`），仅用于修正本件与生成 patch。
- **仓外临时物**：`E:/tmp/run_overrides.txt`、`E:/tmp/x_run.py`（oracle 抽取脚本/中间物），
  `E:/tmp/monrun.patch`（原始交付 patch）、`E:/tmp/monrun-fix.patch`（本次修正 patch）。未改
  `E:/MirServer` 任何文件，未起进程/端口；MySQL 仅只读 SELECT，未写库。
- **未做（诚实声明）**：
  1. 未实现任何代码，未跑 `cargo fmt/clippy/test`（本件是纯界定件）；
  2. 未做真机对拍（P1 的 R 级窗口判据待 owner 排窗口）；
  3. race→名字映射基于部署 MySQL `mir2_data.monsters` 的在线只读查询（705 行、`Race=215` 0 行；
     仅 SELECT，未写库）+ 部署 `MonGen.txt` 名字交叉验证；13 个「MonGen 引用但库中不存在」的名字
     已在 §2.4 登记（出生失败，非快照漂移）；
  4. 未覆盖机器人 `AutoSpell` 的选技/触发集合（旁路 ①，归机器人里程碑）。
