// 卡 openmir2-magic-parity：C# 侧魔法对拍金标准生成器（骨架）。
//
// 用法：
//   dotnet run --project mir2-rs/tests/parity/csharp/MagicGolden -c Release -- --rng-selftest
//
// 设计要点（与卡面第 2 节一致）：
//  * **不改生产代码**：`M2Share.RandomNumber` 是 `public static readonly RandomNumber`，
//    无法重新赋值；但 `OpenMir2.RandomNumber` 内部用**私有静态字段 `random`**（`Random` 实例）
//    产生所有取值。本程序通过反射把该字段替换为 `LoggingRandom : Random`（固定种子、逐次记账），
//    从而做到「同输入 ⇒ 同一 RNG 序列」且**不引入任何测试专用分支到生产类**。
//  * **还原方式**：替换是进程内的；进程退出即还原。同进程跑多用例时，每例前 `Install(seed)`
//    重建账本、例后 `Restore()` 把原 `Random` 实例写回。
//  * 判据（卡面）：帧逐字节 + RNG 抽取次数与逐次取值全等 ⇒ 分支可标 `verified`。

using System.Reflection;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using OpenMir2;
using OpenMir2.Packets.ServerPackets;

namespace MagicGolden;

/// <summary>包装真实 <see cref="Random"/> 的记账器：所有抽取按调用序进账本。</summary>
public sealed class LoggingRandom : Random
{
    public List<long> Draws { get; } = new();
    /// <summary>本 case 期间该包装层的累计抽取数（`Restore` 不会清零；每次 `Install` 新实例从 0 起）。</summary>
    public long TotalDraws { get; private set; }

    public LoggingRandom(int seed) : base(seed) { }

    public override int Next()
    {
        int v = base.Next();
        Draws.Add(v);
        TotalDraws++;
        return v;
    }

    public override int Next(int maxValue)
    {
        int v = base.Next(maxValue);
        Draws.Add(v);
        TotalDraws++;
        return v;
    }

    public override int Next(int minValue, int maxValue)
    {
        int v = base.Next(minValue, maxValue);
        Draws.Add(v);
        TotalDraws++;
        return v;
    }

    protected override double Sample()
    {
        double v = base.Sample();
        return v;
    }
}

/// <summary>
/// **测试接缝（时钟源）**：镜像 [`RngSeam`]——反射设置生产 `OpenMir2.HUtil32` 的**私有静态**字段 `_tickOverride`（`int?`），
/// 使 `HUtil32.GetTickCount()` 在读虚拟钟与真实 `Environment.TickCount` 间切换。默认 `null` ⇒ 生产逐字节等价；仅装置可设。
/// </summary>
public static class ClockSeam
{
    private static readonly FieldInfo TickField =
        typeof(OpenMir2.HUtil32).GetField("_tickOverride", BindingFlags.NonPublic | BindingFlags.Static)
        ?? throw new InvalidOperationException("HUtil32._tickOverride 私有静态字段不存在（原版类结构变了？）");

    public static void Set(int virtualNowMs) => TickField.SetValue(null, virtualNowMs);
    public static void Clear() => TickField.SetValue(null, null);
    public static int? Capture() => (int?)TickField.GetValue(null);
    public static int VNow;
    public static void Begin(int baseMs) { VNow = baseMs; TickField.SetValue(null, VNow); }
    public static void Advance(int ms) { VNow += ms; TickField.SetValue(null, VNow); }
}

public static class RngSeam
{
    private static readonly FieldInfo RandomField =
        typeof(RandomNumber).GetField("random", BindingFlags.NonPublic | BindingFlags.Static)
        ?? throw new InvalidOperationException("RandomNumber.random 私有静态字段不存在（原版类结构变了？）");

    public static LoggingRandom Install(int seed)
    {
        // 先让单例初始化（其构造会把 random = new Random()），再替换。
        _ = RandomNumber.GetInstance();
        var logging = new LoggingRandom(seed);
        RandomField.SetValue(null, logging);
        _current = logging;
        return logging;
    }

    public static void Restore(Random original) => RandomField.SetValue(null, original);

    public static Random Capture() => (Random)RandomField.GetValue(null);

    private static LoggingRandom _current;

    /// <summary>当前安装的包装层（`Install` 设置；`Restore` 不清空引用 ⇒ 计数可回读）。</summary>
    public static long TotalDraws => _current?.TotalDraws ?? 0;
}

public static class Program
{
    public static int Main(string[] args)
    {
        // GB2312：原版 ConfigFile.Load 用 Encoding.GetEncoding("GB2312") 读 conf（`ConfigFile.cs:263`）。
        System.Text.Encoding.RegisterProvider(System.Text.CodePagesEncodingProvider.Instance);
        // 原版日志门面是 public static ILogger（`src/OpenMir2/LogService.cs:9`）；离线进程给一个无 sink 的实例，
        // 避免错误路径（如地图缺失）因 Logger 为 null 抛 NRE 掩盖真实原因。
        OpenMir2.LogService.Logger = new Serilog.LoggerConfiguration().CreateLogger();
        // 冻结配置必须**早于**任何  静态初始化（否则 Exps.conf 的 NeedExps 读成全 0，
        // 登录序  与冻结表复算全部失真）。缺源时留给各模式自己报 PREREQ。
        EnsureConfFiles(out _, out _);
        // 帧编码路径会读城堡/攻城区（`PlayObject.GetChrColor` → `SystemShare.CastleMgr.InCastleWarArea`）；
        // 离线进程给一个空城堡管理器（`CastleManager` 参数构造，城堡列表为空 ⇒ 恒返回 null）。
        if (SystemModule.SystemShare.CastleMgr == null)
        {
            SystemModule.SystemShare.CastleMgr = new M2Server.Castle.CastleManager();
        }
        // 16 的 `MagMakeHolyCurtain` 会 `SystemShare.EventMgr.AddEvent(...)`；离线进程给空事件表
        // （`EventManager` 只维护列表，不启线程）。缺失时会 NullReference ⇒ 该支无法生成。
        if (SystemModule.SystemShare.EventMgr == null)
        {
            SystemModule.SystemShare.EventMgr = new M2Server.Event.EventManager();
        }
        // 17/41/42 的 `MakeSlave` 需要 `WorldEngine.RegenMonsterByName`；离线进程从未初始化它 ⇒
        // 装上最小替身（仅该方法真实现，其余 throw；见 OfflineWorldEngine.cs 与卡 §6）。
        if (SystemModule.SystemShare.WorldEngine == null)
        {
            SystemModule.SystemShare.WorldEngine = new OfflineWorldEngine();
        }
        // **MG-36**：`BaseObject.InSafeZone(env,x,y)` 在 `SafeArea=false` 时也会遍历
        // `M2Share.StartPointList`（生产由 StartPoint.txt 装载；离线进程为 null ⇒ NRE）。
        // 离线替身 = **空表**（没有安全出生点）⇒ 安全区只由地图 Flag 决定；该边界登记在卡 §2。
        if (M2Server.M2Share.StartPointList == null)
        {
            M2Server.M2Share.StartPointList = new System.Collections.Generic.List<OpenMir2.Data.StartPoint>();
        }
        try
        {
        if (args.Contains("--rng-selftest"))
        {
            return RngSelfTest();
        }
        if (args.Contains("--clock-seam-selftest"))
        {
            return ClockSeamSelfTest();
        }
        if (args.Contains("--config-selftest"))
        {
            return ConfigSelfTest();
        }
        if (args.Contains("--conf-selftest"))
        {
            return ConfFilesSelfTest();
        }
        if (args.Contains("--case") && args.Length > Array.IndexOf(args, "--case") + 1)
        {
            var name = args[Array.IndexOf(args, "--case") + 1];
            if (name == "fixture-selftest") return FixtureSelfTest();
            if (name == "runmagic-input-selftest") return RunMagicInputSelfTest();
            if (name == "antimagic-selftest") return AntiMagicSelfTest();
            if (name == "fireball")
            {
                string fbMode = ArgValue(args, "--fireball-rng-mode") ?? "cover_drain";
                string fbLogin = ArgValue(args, "--fireball-login-order") ?? "true";
                return FireballCase(fbMode, !string.Equals(fbLogin, "false", StringComparison.OrdinalIgnoreCase));
            }
            if (name == "fireball-selftest") return FireballSelfTest();
            if (name == "warrskill") return ClientSpellXYEntry.Run();
            if (name == "warr-selftest") return ClientSpellXYEntry.SelfTest();
            // **MG-19（2026-11-26，批 B）**：`Race>=Animal` 与 `LifeAttrib==LA_UNDEAD` 两臂的
            // 示范——`monster`（非不死动物目标：无 1.5x、boTrain=true）与 `monster_undead`
            // （不死动物目标：`nPower=Round(nPower*1.5)`、boTrain=true）。怪物 `AntiMagic=1`
            // 与门槛同值，hit 种子在怪身上仍是「命中」段。
            if (name == "lightning11") return AntiMagicBatch(new MagicSpec { Key = "lightning-11", MagicId = 11, MagicName = "Lightening" , CasterMcNoClamp = HUtil32.MakeWord(5, 10), StepDrainMs = 100, ExtraCases = new[] { new ExtraCaseSpec { Label = "monster", Monsters = new[] { new MonsterSpec { X = 101, Y = 100, Race = 50, Hp = 100, LifeAttrib = 0, AntiMagic = 1 } }, TargetMonster = 1, TargetMonsterUndead = false, UseHitSeed = true }, new ExtraCaseSpec { Label = "monster_undead", Monsters = new[] { new MonsterSpec { X = 101, Y = 100, Race = 80, Hp = 100, LifeAttrib = 1, AntiMagic = 1 } }, TargetMonster = 1, UseHitSeed = true, IdBase = 4200 } } });
            // **MG-54（2026-11-26，批 B）**：与 MG-19 同实现臂（`case 11|45` 逐字相同）——
            // `monster`（非不死：无 1.5x）+ `monster_undead`（不死：`Round(nPower*1.5)`）两臂。
            if (name == "lightning45") return AntiMagicBatch(new MagicSpec { Key = "lightning-45", MagicId = 45, MagicName = "Lightening45" , CasterMcNoClamp = HUtil32.MakeWord(5, 10), StepDrainMs = 100, ExtraCases = new[] { new ExtraCaseSpec { Label = "monster", Monsters = new[] { new MonsterSpec { X = 101, Y = 100, Race = 50, Hp = 100, LifeAttrib = 0, AntiMagic = 1 } }, TargetMonster = 1, TargetMonsterUndead = false, UseHitSeed = true }, new ExtraCaseSpec { Label = "monster_undead", Monsters = new[] { new MonsterSpec { X = 101, Y = 100, Race = 80, Hp = 100, LifeAttrib = 1, AntiMagic = 1 } }, TargetMonster = 1, UseHitSeed = true, IdBase = 4200 } } });
            // 卡 openmir2-spell-tail-delay-audit：以下 10 支的 640 在 oracle 走共享尾
            // `MagicManager.cs:666 SendSelfDelayMsg(RM_MAGIC_LVEXP,…,1000)` ⇒ 必须**时间忠实排水**
            // （`StepDrainMs=100`）。否则「Sleep(WaitMs) 后一次排空」按 per-actor pass 序把 640
            // 排在 600ms 的 31/53/656 之前（MG-17 同类把伪影固化的失效模式）。
            if (name == "healing") return HealingBatch();
            // **MG-12/40 `tick_cap`（2026-11-26）**：一次施法 `nParam1 > 300` ⇒ `RM_MAGHEALING`
            // 累计走 **else 支**（置 300）。用 B16 Arm A 的五字段把 `nPower` 抬到 ~800
            // （`MagicManager` 私有 `MPow`/`GetPower`：`GetPower = Round(MPow/(TrainLv+1)*(Level+1))`
            // + DefPower + R(DefMaxPower-DefPower)`；本臂 Level=1 ⇒ ×2）。
            if (name == "healing-nonzero") return NonzeroHealingBatch();
            // **MG-27（2026-11-26，批 B）**：`monster` 支——非不死动物目标。
            // 13 的 `SendSelfDelayMsg` 是 **1200ms**、伤害走 **SC**（`HUtil32.LoByte(WAbil.SC)` 起）；
            // `Race>=Animal ⇒ boTrain`（训练尾 640/10123）。无 LA_UNDEAD 分支 ⇒ 单额外 case。
            if (name == "firecharm") return AntiMagicBatch(new MagicSpec { Key = "firecharm", MagicId = 13, MagicName = "FireCharm", ArmRingAmulet = true , CasterScNoClamp = HUtil32.MakeWord(5, 10), StepDrainMs = 100, ExtraCases = new[] { new ExtraCaseSpec { Label = "monster", Monsters = new[] { new MonsterSpec { X = 101, Y = 100, Race = 50, Hp = 100, LifeAttrib = 0, AntiMagic = 1 } }, TargetMonster = 1, TargetMonsterUndead = false, UseHitSeed = true, FaithfulPump = true } } });
            // **MG-53（2026-11-26，批 B）**：`Race>=Animal` 只影响 `boTrain`，而本 stem 的
            // `UserMagicLevel=3` 把训练门关掉 ⇒ 需要**三支结构**：既有三支（玩家@lv3）不动；
            // `player_lv0`（玩家@lv0，对照）与 `monster_lv0`（怪@lv0，处理）逐项比，
            // 差值才是 `Race>=Animal` 的（boTrain/训练尾）效应。`Random(Level)` 在 lv0 下变
            // `Random(0)`（§27 边界），两支持同等级 ⇒ 边界效应由"对照 vs 处理"分离。
            if (name == "hbfireball") return AntiMagicBatch(new MagicSpec { Key = "hbfireball", MagicId = 44, MagicName = "HbFireBall", UserMagicLevel = 3 , CasterMcNoClamp = HUtil32.MakeWord(5, 10), StepDrainMs = 100, ExtraCases = new[] { new ExtraCaseSpec { Label = "player_lv0", Monsters = Array.Empty<MonsterSpec>(), TargetMonster = 0, UserMagicLevelOverride = 0, UseHitSeed = true }, new ExtraCaseSpec { Label = "monster_lv0", Monsters = new[] { new MonsterSpec { X = 101, Y = 100, Race = 50, Hp = 100, LifeAttrib = 0, AntiMagic = 1 } }, TargetMonster = 1, TargetMonsterUndead = false, UserMagicLevelOverride = 0, UseHitSeed = true, IdBase = 4200 } } });
            // **MG-11（2026-11-26）**：额外 `monster` 支——非不死动物目标（`Race>=Animal` ⇒ `boTrain`，
            // 训练尾 640/10123 面）；怪物 `AntiMagic=1` 让 hit 种子仍落在「命中」段。
            if (name == "fireball5") return AntiMagicBatch(new MagicSpec { Key = "fireball-5", MagicId = 5, MagicName = "FireBall2" , CasterMcNoClamp = HUtil32.MakeWord(5, 10), StepDrainMs = 100, ExtraCases = new[] { new ExtraCaseSpec { Label = "monster", Monsters = new[] { new MonsterSpec { X = 101, Y = 100, Race = 50, Hp = 100, LifeAttrib = 0, AntiMagic = 1 } }, TargetMonster = 1, TargetMonsterUndead = false, UseHitSeed = true } } });
            if (name == "hangmajinbub") return BehaviorBatch(new MagicSpec { Key = "hangmajinbub", MagicId = 14, MagicName = "HangMaJinBub", ArmRingAmulet = true , HitSpeed = 7, CasterScNoClamp = HUtil32.MakeWord(5, 10), StepDrainMs = 100 });
            if (name == "dejiwonho") return BehaviorBatch(new MagicSpec { Key = "dejiwonho", MagicId = 15, MagicName = "DeJiWonHo", ArmRingAmulet = true , HitSpeed = 7, CasterScNoClamp = HUtil32.MakeWord(5, 10), StepDrainMs = 100 });
            if (name == "shield") return BehaviorBatch(new MagicSpec { Key = "shield", MagicId = 31, MagicName = "Shield" , HitSpeed = 7, StepDrainMs = 100 });
            if (name == "showhp") return ShowHpBatch(new MagicSpec { Key = "showhp", MagicId = 28, MagicName = "ShowHp", StepDrainMs = 100 });
            // **MG-55（46 分身术）**：`MagMakeClone` 只造克隆体（无配置/事件系统依赖）。训练前提钉死
            // (`level=0 / TrainLv=3 / MaxTrain[0]=1000`)，`Gender=1/Hair=7` 让 feature 面非退化。
            // **MG-36（22 火墙）**：`MagMakeFireCross` 五格 + 安全区 + 预占去重。非退化 MC
            // （`MakeWord(5,10)`）让 damage/时间都有抽数；训练前提同 46（640 走 1000ms 支）。
            // **MG-34（20 诱惑之光）**：七支分支表；配置十键由 ServerConfFixture 记录。
            if (name == "tamming")
            {
                return TammingBatch(new MagicSpec { Key = "tamming", MagicId = 20, MagicName = "Tamming" });
            }
            if (name == "firecross")
            {
                return FireCrossBatch(new MagicSpec
                {
                    Key = "firecross",
                    MagicId = 22,
                    MagicName = "FireCross",
                    UserMagicLevel = 0,
                    MagicTrainLv = 3,
                    MagicMaxTrain0 = 1000,
                    CasterMc = HUtil32.MakeWord(5, 10),
                    StepDrainMs = 100,
                    SkillTail = true,
                });
            }
            if (name == "clone")
            {
                return CloneBatch(new MagicSpec
                {
                    Key = "clone",
                    MagicId = 46,
                    MagicName = "Clone",
                    UserMagicLevel = 0,
                    MagicTrainLv = 3,
                    MagicMaxTrain0 = 1000,
                    CasterDir = 0,
                    CasterGender = 1,
                    CasterHair = 7,
                    PreSpellFaceTarget = true,
                    // 640 在 +1000ms ⇒ 必须步进排水（与 49/51 等同口径）。
                    StepDrainMs = 100,
                    SkillTail = true,
                });
            }
            // **MG-32（2026-11-26，批 B）**：`MagMakePrivateTransparent` 的
            // `Race>=Animal && TargetCret == baseObject` 分支——怪 `TargetCret` 指向施法者：
            // `monster_clear`（邻格，`Random(2)==0` ⇒ 清空）/ `monster_keep`（邻格，抽 ≠0 ⇒ 保留）/
            // `monster_far`（距离>1 ⇒ 短路清空、**不抽**）。术后 `TargetCret` 落盘进比较面。
            if (name == "cloak") return BehaviorBatch(new MagicSpec { Key = "cloak", MagicId = 18, MagicName = "Cloak", ArmRingAmulet = true , HitSpeed = 7, ExtraCases = new[] { new ExtraCaseSpec { Label = "monster_clear", Monsters = new[] { new MonsterSpec { X = 101, Y = 100, Race = 50, Hp = 100, LifeAttrib = 0, TargetCretCaster = true } }, SeedOverride = 1, IdBase = 4100 }, new ExtraCaseSpec { Label = "monster_keep", Monsters = new[] { new MonsterSpec { X = 101, Y = 100, Race = 50, Hp = 100, LifeAttrib = 0, TargetCretCaster = true } }, SeedOverride = 3, IdBase = 4200 }, new ExtraCaseSpec { Label = "monster_far", Monsters = new[] { new MonsterSpec { X = 105, Y = 100, Race = 50, Hp = 100, LifeAttrib = 0, TargetCretCaster = true } }, SeedOverride = 1, IdBase = 4300 } } });
            if (name == "bighealing") return BigHealingBatch(new MagicSpec { Key = "bighealing", MagicId = 29, MagicName = "BigHealing", CasterScNoClamp = HUtil32.MakeWord(5, 10), StepDrainMs = 100 });
            if (name == "bigcloak") return BehaviorBatch(new MagicSpec { Key = "bigcloak", MagicId = 19, MagicName = "BigCloak", ArmRingAmulet = true, StepDrainMs = 100 , HitSpeed = 7 });
            if (name == "fireboom") return BehaviorBatch(new MagicSpec { Key = "fireboom", MagicId = 23, MagicName = "FireBoom" , CasterMcNoClamp = HUtil32.MakeWord(5, 10), StepDrainMs = 100 });
            // `SKILL_47`（MagicManager.cs `case MagicConst.SKILL_47`）：与 23 同 helper（FireBoomRage），只有魔号不同。
            if (name == "fireboom47") return BehaviorBatch(new MagicSpec { Key = "fireboom47", MagicId = 47, MagicName = "FireBoom47" , CasterMcNoClamp = HUtil32.MakeWord(5, 10), StepDrainMs = 100 });
            // `SKILL_LIGHTFLOWER(24)`（`case MagicConst.SKILL_LIGHTFLOWER` → `MagElecBlizzard(ElecBlizzardRange)`）。
            if (name == "lightflower") return BehaviorBatch(new MagicSpec { Key = "lightflower", MagicId = 24, MagicName = "LightFlower" , CasterMcNoClamp = HUtil32.MakeWord(20, 30), StepDrainMs = 100, ExtraCases = new[] { new ExtraCaseSpec { Label = "undead", Monsters = new[] { new MonsterSpec { X = 101, Y = 100, Race = 80, Hp = 100, LifeAttrib = 1, Animal = true, OperateStruck = true } } } } });
            // `SKILL_UNAMYOUNSUL(40)`（部署行「十方斩」；`case MagicConst.SKILL_UNAMYOUNSUL` 清三态计时器）。
            if (name == "unamyounsul") return UnamyounsulBatch(new MagicSpec { Key = "unamyounsul", MagicId = 40, MagicName = "UnAmYoUnSul" });
            // `SKILL_HOLYSHIELD(16)`（`case MagicConst.SKILL_HOLYSHIELD` → `MagMakeHolyCurtain`）：
            // v1 被动怪物 —— 只判"对象选择"（谁收到 `RM_MAKEHOLYSEIZEMODE`）。
            // `SKILL_SKELLETON(17)`：MakeSlave → WorldEngine 替身；两支 = 正常召唤 / 数量上限。
            if (name == "skeleton") return SkeletonBatch(new MagicSpec { Key = "skeleton", MagicId = 17, MagicName = "Skeleton", ArmRingAmulet = true, Reachability = "unverified", ScenarioNote = "登录序+护身符；WorldEngine 为离线替身（仅 RegenMonsterByName）。SCOPE：本支验的是『给定怪物之后的 adopt 行为』（MakeSlave 的 Master/IsSlave/levels/Recalc/半补/RefNameColor/SlaveList），不是造怪；造怪抽数因离线缺 MonsterInfo（掉落表 MonGetRandomItems、CoolEyeCode）不可复刻 ⇒ Rng 与下游训练面（640/RM10123 p3）不在对拍范围；golden 的 Rng 是替身不抽造怪时的数，不代表生产抽数" });
            // **MG-42（41 SKILL_ANGEL）**：`case SKILL_ANGEL`（`MagicManager.cs:462-473`）→
            // `MagMakeAngelSlave`（`:1343-1354`）→ `MakeSlave(Config.Angel="精灵", level, level, 1, …)`。
            // **SCOPE 写死**：stub 验证（`OfflineWorldEngine.RegenMonsterByName` 是离线替身）+
            // 生产数据缺失（冻结 `sql/mir2_data.sql` 的 `monsters` 无 `精灵` 行 ⇒ `GetMonRace` 返 -1
            // ⇒ `CreateMonster` 无 default ⇒ `MakeSlave` null ⇒ 41 生产恒失败；卡
            // `openmir2-angel-summon-monster-missing`）。**不得**把本支绿灯说成"生产可召唤"。
            if (name == "angel") return AngelBatch(new MagicSpec { Key = "angel", MagicId = 41, MagicName = "Angel", ArmRingAmulet = true, Reachability = "unverified", ScenarioNote = "STUB 验证 + 生产数据缺失：WorldEngine 为离线替身（仅 RegenMonsterByName），验的是『给定怪物之后的 adopt 行为』；生产 `monsters` 无 `精灵` 行 ⇒ MakeSlave 恒 null（卡 openmir2-angel-summon-monster-missing），本 golden 不证明生产可召唤；造怪抽数不可复刻 ⇒ Rng/640-tag 不在对拍范围" });
            if (name == "sinsu") return SinsuBatch(new MagicSpec { Key = "sinsu", MagicId = 30, MagicName = "SinSu", ArmRingAmulet = true, Reachability = "unverified", ScenarioNote = "SINSU 神兽=race113；验 MakeSlave adopt + cap_hit 回召(RecallSlave 移 slave)；仅参数化 race/stub，不验龙名变体(数据缺口)" });
            if (name == "holyshield") return HolyShieldBatch(new MagicSpec { Key = "holyshield", MagicId = 16, MagicName = "HolyShield", ArmRingAmulet = true });
            // 49/51/52/53/54：共享尾 B 类（case 体只有 `boTrain = true;`）——本批先做 49。
            // 夹具前提：`UserMagicLevel<3`（训练门）、`TrainLv=3`（升级门非退化）、`MaxTrain[0]=1000`（不升级 ⇒ 1000ms 支）。
            // `EffectType=2` 与 `Effect=1` 不同 ⇒ `MakeWord(EffectType, Effect)=0x0102`，
            // 把「低字节=EffectType / 高字节=Effect」的打包变成**可判别**（两者都取 1 时换位不可见）。
            // 共享尾 B 类五号（MG-58/60/61/62/63）：`SkillTailBatch` 同形参数化。
            // `EffectType=2`（Effect 仍 1）⇒ `MakeWord(EffectType, Effect)=0x0102`，把「低字节=EffectType」
            // 的打包变成**可判别**（两者都取 1 时换位不可见）。
            // `UserMagicLevel=0` ⇒ `nLevel=0`，前提断言核的就是 `CheckMagicLevelUp` 实际读的那一项。
            if (name is "skill49" or "skill51" or "skill52" or "skill53" or "skill54")
            {
                int sid = int.Parse(name.Substring(5));
                return SkillTailBatch(new MagicSpec { Key = name, MagicId = sid, MagicName = "Skill" + sid, EffectType = 2, UserMagicLevel = 0, MagicTrainLv = 3, MagicMaxTrain0 = 1000, SkillTail = true });
            }
            // 39/42/48：同在 `CheckAmulet(1,1)` 组（`MagicManager.cs:315-325`），需护身符。
            if (name == "groupdeding") return GroupDeDingBatch(new MagicSpec { Key = "groupdeding", MagicId = 39, MagicName = "GroupDeDing", ArmRingAmulet = true });
            if (name == "uenhancer") return UenhancerBatch(new MagicSpec { Key = "uenhancer", MagicId = 36, MagicName = "UEnhancer", ArmRingAmulet = true , HitSpeed = 7 });
            if (name == "grouplightening") return BehaviorBatch(new MagicSpec { Key = "grouplightening", MagicId = 42, MagicName = "GroupLightening", ArmRingAmulet = true, StepDrainMs = 100 });
            // **推挤批（MG-16 抗拒火环 8 / MG-57 气功波 37）**：`MagPushArround`，需要 `VisibleActors`
            // 回填（`MakeVisible`）。两支 oracle 函数体逐字相同，装置同形。
            // **B16 Arm A（非零值魔表对照臂）**：五字段取 `sql/mir2_data.sql` 的**生效行**
            // （MagID 重复时 `FindMagic` 取最先读到的行；本批 1/11/24 无重复）。
            // `--nonzero-control`：同 label/seed 把五字段全置 0，产物只供差异清单（不落仓）。
            if (name is "fireball-nonzero" or "lightning11-nonzero" or "lightflower-nonzero")
            {
                bool isControl = args.Contains("--nonzero-control");
                int p = 0, mp = 0, dp = 0, dmp = 0;
                if (name == "fireball-nonzero") { p = 8; mp = 8; dp = 2; dmp = 2; }
                else if (name == "lightning11-nonzero") { p = 14; mp = 28; dp = 10; dmp = 10; }
                else { p = 10; mp = 30; dp = 10; dmp = 30; }
                var spec = new MagicSpec
                {
                    Key = name,
                    MagicId = name == "fireball-nonzero" ? 1 : name == "lightning11-nonzero" ? 11 : 24,
                    MagicName = name,
                    MagicTrainLv = isControl ? 0 : 3,
                    NonzeroArm = true,
                    MagicPower = isControl ? 0 : p,
                    MagicMaxPower = isControl ? 0 : mp,
                    MagicDefPower = isControl ? 0 : dp,
                    MagicDefMaxPower = isControl ? 0 : dmp,
                    CasterMcNoClamp = name == "lightflower-nonzero" ? HUtil32.MakeWord(20, 30) : HUtil32.MakeWord(5, 10),
                    // 三支统一时间忠实排水：600ms 交付帧与 1000ms/800ms 训练尾的 `due_ms_rel`
                    // 必须与 Rust 步进交付同口径（否则 golden=0 / rust=600 是记录面伪影）。
                    StepDrainMs = 100,
                };
                return name == "lightflower-nonzero" ? BehaviorBatch(spec) : AntiMagicBatch(spec);
            }
                        if (name == "firewind")
            {
                // **MG-57 P2（2026-11-26）**：MG-57 的未验面逐条成臂——
                // ①8 向 ×1 格（每向一个受害者位）、②对角 2 格、③多目标、④被挡两施法（同目标连续两 tick）、
                // ⑤`RM_PUSH` 收件人边界 4 档（首扫 ≤12 / 复用 <11）。
                // 所有臂走**真 `0.map`**（`real_map`，逐 case）+ 生产 AOI（`production_aoi`，逐 case）
                // + 缓存冷断言（`push_cache_cold`）；既有 s1/s5/s13 三项全缺省 ⇒ 逐字节不变。
                // seed 1 = `Random(2)=0` ⇒ 推 1 格；seed 13 = 1 ⇒ 推 2 格（与既有 s1/s13 同口径）。
                return BehaviorBatch(new MagicSpec
                {
                    Key = "firewind",
                    MagicId = 8,
                    MagicName = "FireWind",
                    MakeVisible = true,
                    ExtraCases = new[] {
                        // ①8 向（受害者 = target，caster 恒 (100,100)；obs_in 缺省 (105,100) ⇒ Δ>1 不被推）
                        new ExtraCaseSpec { Label = "dir_e", TargetXy = new[] { 101, 100 }, SeedOverride = 1, IdBase = 4100, RealMap = true, ProductionAoi = true, PushCacheProbe = true, ObserveDirAfter = true },
                        new ExtraCaseSpec { Label = "dir_w", TargetXy = new[] { 99, 100 }, SeedOverride = 1, IdBase = 4200, RealMap = true, ProductionAoi = true, PushCacheProbe = true, ObserveDirAfter = true },
                        new ExtraCaseSpec { Label = "dir_n", TargetXy = new[] { 100, 99 }, SeedOverride = 1, IdBase = 4300, RealMap = true, ProductionAoi = true, PushCacheProbe = true, ObserveDirAfter = true },
                        new ExtraCaseSpec { Label = "dir_s", TargetXy = new[] { 100, 101 }, SeedOverride = 1, IdBase = 4400, RealMap = true, ProductionAoi = true, PushCacheProbe = true, ObserveDirAfter = true },
                        new ExtraCaseSpec { Label = "dir_ne", TargetXy = new[] { 101, 99 }, SeedOverride = 1, IdBase = 4500, RealMap = true, ProductionAoi = true, PushCacheProbe = true, ObserveDirAfter = true },
                        new ExtraCaseSpec { Label = "dir_nw", TargetXy = new[] { 99, 99 }, SeedOverride = 1, IdBase = 4600, RealMap = true, ProductionAoi = true, PushCacheProbe = true, ObserveDirAfter = true },
                        new ExtraCaseSpec { Label = "dir_se", TargetXy = new[] { 101, 101 }, SeedOverride = 1, IdBase = 4700, RealMap = true, ProductionAoi = true, PushCacheProbe = true, ObserveDirAfter = true },
                        new ExtraCaseSpec { Label = "dir_sw", TargetXy = new[] { 99, 101 }, SeedOverride = 1, IdBase = 4800, RealMap = true, ProductionAoi = true, PushCacheProbe = true, ObserveDirAfter = true },
                        // ②对角 2 格（seed 13 ⇒ `Random(2)=1`）
                        new ExtraCaseSpec { Label = "diag_ne2", TargetXy = new[] { 101, 99 }, SeedOverride = 13, IdBase = 4900, RealMap = true, ProductionAoi = true, PushCacheProbe = true },
                        // ③多目标：target 在东、obs_in 挪到南邻格 ⇒ 同一施法推两个 Play 受害者
                        new ExtraCaseSpec { Label = "multi", TargetXy = new[] { 101, 100 }, ObsInXy = new[] { 100, 101 }, SeedOverride = 1, IdBase = 5000, RealMap = true, ProductionAoi = true, PushCacheProbe = true },
                        // ④被挡两施法：caster (99,103)/target (100,103)，东侧 (101,103) 是 `0.map` 的墙 ⇒
                        // 两次施法都 `CharPushed` 失败（零 `9` 帧），但 `Random(20)`/`Random(2)` 照抽。
                        new ExtraCaseSpec { Label = "blocked2", CasterXy = new[] { 99, 103 }, TargetXy = new[] { 100, 103 }, SeedOverride = 1, IdBase = 5100, RealMap = true, ProductionAoi = true, PushCacheProbe = true, SecondCastAtMs = 100, StepDrainMsOverride = 100, UserMagicLevelOverride = 3 /* 训练门关 ⇒ 两次 640 的 param 相同，等键序不进判据 */, ObserveDirAfter = true },
                        // ⑤收件人边界（受害者东推两格 ⇒ 格1 (102,100) 首扫、格2 (103,100) 复用）：
                        // 首扫 |Δ|≤12 含、复用 |Δ|<11 含；obs 距受害者格1/格2 分别为：
                        // a: obs_in(113)→11/10（两次都含）、obs_out(114)→12/11（首扫含 / 复用不含——钉 `<11`）
                        // b: obs_in(114)→12/11、obs_out(115)→13/12（13 两档都不含）
                        // 观察者都放在 caster 的 Δ≥13 处（caster 的 `RM_SPELL` 首扫 ±12 之外）⇒ 它们只
                        // 经受害者的 `RM_PUSH` 面，不和施法者自身的 `SendRefMsg` 缓存边界纠缠。
                        new ExtraCaseSpec { Label = "bound_a", ObsInXy = new[] { 113, 100 }, ObsOutXy = new[] { 114, 100 }, SeedOverride = 13, IdBase = 5200, RealMap = true, ProductionAoi = true, PushCacheProbe = true },
                        new ExtraCaseSpec { Label = "bound_b", ObsInXy = new[] { 114, 100 }, ObsOutXy = new[] { 115, 100 }, SeedOverride = 13, IdBase = 5300, RealMap = true, ProductionAoi = true, PushCacheProbe = true },
                    },
                });
            }
            if (name == "energyrepulsor") return BehaviorBatch(new MagicSpec { Key = "energyrepulsor", MagicId = 37, MagicName = "EnergyRepulsor", MakeVisible = true });
            // **MG-35（21 瞬息移动）**：`MapRandomMove→SpaceMove` 走 `FindMap(HomeMap)` ⇒
            // `EnableMaps`（env.MapName/ServerIndex）+ `HomeMap="0"`。观测面是 `RM_SPACEMOVE_SHOW2(807)`
            // 的 `param/tag`（落点），不新增任何观测字段。
            if (name == "spacemove") return BehaviorBatch(new MagicSpec { Key = "spacemove", MagicId = 21, MagicName = "SpaceMove", EnableMaps = true, HomeMap = "0" });
            // **MG-44（32 圣言术）/ MG-48（50 魔爆）**：ServerConf 十键**分支级**装载（走共享入口，
            // 不自己再读 `Server.conf`）。32 的目标是**怪物**（`TargetMonster` 1 基），目标等级
            // 43/44/45 三档钉住部署阈值 44 的严格小于口径；50 保持**玩家目标**（oracle 只有
            // `Race == Play` 才发 STONE 中毒帧，套 `TargetMonster` 会丢该分支）。
            if (name == "turnundead")
            {
                return TurnUndeadBatch(new MagicSpec
                {
                    Key = "turnundead",
                    MagicId = 32,
                    MagicName = "TurnUndead",
                    // `:842 Random(2)+(Level-1) > target.Level` 对 43..45 两抽都真（0+59>45、1+59>45）
                    // ⇒ 比较面只由 `:844` 的等级门决定。
                    CasterLevel = 60,
                    TargetMonster = 1,
                    Monsters = new[] { new MonsterSpec { X = 101, Y = 100, Race = 50, Level = 43, Hp = 300, LifeAttrib = 1 } },
                    // `Struck` 之后的训练尾 640 在 +1000ms ⇒ 时间忠实排水。
                    StepDrainMs = 100,
                    // **MG-44（2026-11-26，批 B）**：六臂——
                    // A `miss_842`（caster 20 vs 怪 43 ⇒ `R(2)+19 > 43` 假：抽 `[R300, R2]`、无 R100）；
                    // B `non_undead`（`LifeAttrib=0` ⇒ `:830` 零抽）；C `super_man`（`SuperMan` ⇒ 零抽）；
                    // D `multi`（两只不死怪：目标 43 + 旁观 45，旁观零影响）；
                    // E `player_target`（玩家 `LifeAttrib=0` ⇒ 零抽，非 AnimalObject 面）；
                    // F `run_away`（怪 `Master=caster` ⇒ `IsProperTarget(caster)=false` ⇒ Struck 不设
                    //   `TargetCret` ⇒ `:835-840` RunAway(10s)；seed 取 lv43 的即死 seed ⇒ HP 0 证明被 Struck）。
                    // 每项 `ObserveState=true` 的支落 `caster.TargetCret` 与 `mon.RunAwayMode/Time`。
                    TargetMonsterAllowSuperMan = true,
                    ExtraCases = new[] {
                        new ExtraCaseSpec { Label = "miss_842", Monsters = new[] { new MonsterSpec { X = 101, Y = 100, Race = 50, Level = 43, Hp = 300, LifeAttrib = 1 } }, TargetMonster = 1, CasterLevelOverride = 20, SeedOverride = 1, IdBase = 4100 },
                        new ExtraCaseSpec { Label = "non_undead", Monsters = new[] { new MonsterSpec { X = 101, Y = 100, Race = 50, Level = 43, Hp = 300, LifeAttrib = 0 } }, TargetMonster = 1, TargetMonsterUndead = false, SeedOverride = 1, IdBase = 4200 },
                        new ExtraCaseSpec { Label = "super_man", Monsters = new[] { new MonsterSpec { X = 101, Y = 100, Race = 50, Level = 43, Hp = 300, LifeAttrib = 1, SuperMan = true } }, TargetMonster = 1, SeedOverride = 1, IdBase = 4300 },
                        new ExtraCaseSpec { Label = "multi", Monsters = new[] { new MonsterSpec { X = 101, Y = 100, Race = 50, Level = 43, Hp = 300, LifeAttrib = 1 }, new MonsterSpec { X = 102, Y = 100, Race = 50, Level = 45, Hp = 300, LifeAttrib = 1 } }, TargetMonster = 1, SeedOverride = 1, ObserveState = true, IdBase = 4400 },
                        new ExtraCaseSpec { Label = "player_target", Monsters = Array.Empty<MonsterSpec>(), TargetMonster = 0, SeedOverride = 1, IdBase = 4500 },
                        new ExtraCaseSpec { Label = "run_away", Monsters = new[] { new MonsterSpec { X = 101, Y = 100, Race = 50, Level = 43, Hp = 300, LifeAttrib = 1, MasterCaster = true } }, TargetMonster = 1, SeedOverride = 2 /* = lv43 即死 seed（R100 命中 ⇒ HP 0，证明被 Struck） */, ObserveState = true, IdBase = 4600 },
                    },
                    ServerConf = ServerConfFixture.LoadOrThrow(name),
                });
            }
            if (name == "mabmabe")
        {
            return BehaviorBatch(new MagicSpec
            {
                Key = "mabmabe",
                MagicId = 50,
                MagicName = "MabMabe",
                // 伤害区间非退化（`GetAttackPower(Lo(MC), Hi(MC)-Lo(MC)+1)` 抽 `Random(6)`）。
                CasterMcNoClamp = HUtil32.MakeWord(5, 10),
                // RM_DELAYMAGIC 600ms + 训练尾 1000ms ⇒ 步进排水。
                StepDrainMs = 100,
                ServerConf = ServerConfFixture.LoadOrThrow(name),
            });
        }
        // **MG-48 非缺省对照臂**（卡 `magic-serverconf-nondefault-arm-plan`）：独立冻结夹具
            // `tests/parity/fixtures/serverconf/nondefault/Server.conf`（四键 50/40/90/7，均 ≠ 部署值
            // 100/10/21/20 且 ≠ ctor 缺省），让「读配置」与「吃缺省」在**行为面**可区分。
            if (name == "mabmabe-nondefault")
            {
                // `--nondefault-control`：**差分对照**用（同一 label/seed、部署配置），产物只供
                // "两支输出必须真的不同"的证据，不落仓、不进比较器。
                bool isControl = args.Contains("--nondefault-control");
                if (isControl)
                {
                    // 用默认入口按 sha 把**部署件**同步回来（修复后必需：上一步可能刚写过夹具件）。
                    if (!EnsureConfFiles(out _, out var ctrlMissing))
                    {
                        throw new FixtureContractViolation("对照臂缺部署配置：" + string.Join(",", ctrlMissing));
                    }
                }
                else
                {
                    string ndConfDir = FindNonDefaultConfDir();
                    // 只同步本支需要的夹具（**不**改 env；否则会影响同进程其它 case 的默认入口）。
                    EnsureConfFilesFrom(ndConfDir, AppContext.BaseDirectory, out var ndCopied, out var ndMissing);
                    if (ndMissing.Count > 0)
                    {
                        throw new FixtureContractViolation("非缺省夹具缺文件：" + string.Join(",", ndMissing));
                    }
                    Console.Error.WriteLine("NONDEFAULT_CONF_COPIED " + string.Join(",", ndCopied));
                }
                return NonDefaultArmBatch(new MagicSpec
                {
                    Key = "mabmabe-nondefault",
                    MagicId = 50,
                    MagicName = "MabMabe",
                    // 刻意**不设** CasterMcNoClamp：本支 label 不属于 s5/s13/hit ⇒ 不命中 MC 变体，
                    // 第 3 抽固定 `Random(1)`（取数序见卡 §2.1/§2.6）。
                    StepDrainMs = 100,
                    ServerConf = ServerConfFixture.LoadOrThrow(name, nonDefault: !isControl),
                }, expectNonDefault: !isControl);
            }
            // **MG-48 配置变体（2026-11-26）**：完整 `Server.conf` 副本（其余九键=部署），逐变体钉 sha。
            // `rand0/rand1`：`MabMabeHitRandRate` 的 §27 边界（`Random(0)` 是否推进由 golden 记录）；
            // `time10`：`MabMabeHitMabeTimeRate=10` 的 STONE 时长面。
            if (name is "mabmabe-rand0" or "mabmabe-rand1" or "mabmabe-time10")
            {
                string vDir = FindConfVariantDir(name.Substring("mabmabe-".Length));
                EnsureConfFilesFrom(vDir, AppContext.BaseDirectory, out var vCopied, out var vMissing);
                if (vMissing.Count > 0)
                {
                    throw new FixtureContractViolation($"配置变体 {name} 缺文件：" + string.Join(",", vMissing));
                }
                Console.Error.WriteLine("VARIANT_CONF_COPIED " + name + " " + string.Join(",", vCopied));
                return BehaviorBatch(new MagicSpec
                {
                    Key = name,
                    MagicId = 50,
                    MagicName = "MabMabe",
                    CasterMcNoClamp = HUtil32.MakeWord(5, 10),
                    StepDrainMs = 100,
                    ServerConf = ServerConfFixture.LoadVariantOrThrow(name),
                    // **MG-48**：`time10` 变体的**成功臂**（与部署 `time_rate=20` 对照看 STONE 时长）
                    // 暂不落：受 B17/`openmir2-refmsg-queue-model` 的等键帧序缺口阻塞，
                    // 见 `docs/issues/openmir2-refmsg-queue-model.md`「MG-48 最小复现」节。
                });
            }
            // **施毒系（MG-13/14/15/50/64/65）**：护身符 `Shape<=2`（`CheckAmulet(1,2)`）+ 目标
            // `AntiPoison` + `AmyOunsulPoint` 三旋钮。`AmyOunsulPoint` **必须显式装载**：不装载会落
            // ctor 缺省 **10**（`GameSvrConf.cs` 的 `AmyOunsulPoint = 10`，**不是 0、不除零**），
            // 但部署是 **15** ⇒ 不装载就会**功率数值错**且与部署不同源（卡 §53）。断言 `!= 15` 即 rc=2。
            // **作用域**：本 `--case` 分支级装载（不全局），且本生成器**一次进程只处理一个 `--case`**
            // （CLI 只取一个 argv），静态 `SystemShare.Config` 不会泄漏到别的 case 族。
            if (name is "amyounsul" or "amyounsul-shape2" or "groupamyounsul" or "groupamyounsul-shape2")
            {
                // **显式装载 ServerConf（共享入口）**：`ServerConfFixture.LoadOrThrow` 先断生成源 sha、
                // 再断言十键 == 部署值（含 `AmyOunsulPoint==15`）；缺文件/漂移/任何键吃缺省 ⇒
                // `FixtureContractViolation`（rc=2）。施毒批不再写内联装载，避免两套口径。
                _ = ServerConfFixture.LoadOrThrow(name);
                // **`[String] YouPoisoned` 必须真的读进 `MessageSettings`**：`StringConf` 的 ctor 只做
                // `Load()`（解析进缓存），`LoadString()` 才是把值写进 `MessageSettings` 的那一步——生产
                // 由 `GameShare.cs:91` 调用，本离线装置必须显式调；不调就会静默用源码默认「中毒了!!!」，
                // 而部署值（`String.conf:31`）是「您中毒了!!!」⇒ 金标准记的值 ≠ 实际会用的值（卡 §53）。
                SystemModule.SystemShare.StringConf.LoadString();
                if (string.IsNullOrEmpty(SystemModule.MessageSettings.YouPoisoned))
                {
                    throw new FixtureContractViolation($"[{name}] [String] YouPoisoned 未装载（空）");
                }
            }
            if (name == "amyounsul") return BehaviorBatch(new MagicSpec { Key = "amyounsul", MagicId = 6, MagicName = "AmYoUnSul", ArmRingAmulet = true, HitSpeed = 7, AmuletShape = 1, CasterScNoClamp = HUtil32.MakeWord(5, 10), StepDrainMs = 100, FrameOrderScope = "multi_victim_same_tick" });
            if (name == "amyounsul-shape2") return BehaviorBatch(new MagicSpec { Key = "amyounsul-shape2", MagicId = 6, MagicName = "AmYoUnSul", ArmRingAmulet = true, HitSpeed = 7, AmuletShape = 2, CasterScNoClamp = HUtil32.MakeWord(5, 10), StepDrainMs = 100, FrameOrderScope = "multi_victim_same_tick" });
            if (name == "groupamyounsul") return BehaviorBatch(new MagicSpec { Key = "groupamyounsul", MagicId = 48, MagicName = "GroupAmyounsul", ArmRingAmulet = true, HitSpeed = 7, AmuletShape = 1, StepDrainMs = 100, FrameOrderScope = "multi_victim_same_tick", ExtraCases = new[] {
                // **MG-50（2026-11-26）**：范围边界——obs_in 在 click (101,100) 的 Chebyshev 距离 4。
                // `MAX(1, Lv)` 方框：Lv1 不含（无 8037）、Lv4 含（有 8037）。
                new ExtraCaseSpec { Label = "lvl1", UserMagicLevelOverride = 1, SeedOverride = 1, IdBase = 6100 },
                new ExtraCaseSpec { Label = "lvl4", UserMagicLevelOverride = 4, SeedOverride = 1, IdBase = 6200 },
                // **MG-50**：多目标 + 护身符耗尽——3 只怪在方框内（x 升序扫描：m1→m2→m3），
                // `Dura=100` ⇒ 只 **1 次** `CheckAmulet` 通过 ⇒ 仅第一个合格目标有 `8037` **且**置
                // `TargetCret`；其余既无 `8037`、也不置 `TargetCret`（`SetTargetCreat` 在
                // `if (CheckAmulet)` 内、骰外）。
                // 正臂：玩家目标**移出方框**（(110,100)）⇒ 扫描首个合格目标是 mon0 ⇒ 拿到 `8037` 且 `TargetCret` 落它；
                // mon1/mon2 无护身符 ⇒ 零 `8037`、且不置 `TargetCret`。
                new ExtraCaseSpec { Label = "multi_amulet_monster_first", Monsters = new[] { new MonsterSpec { X = 101, Y = 100, Race = 50, Level = 10, Hp = 100, LifeAttrib = 0, AntiMagic = 0 }, new MonsterSpec { X = 102, Y = 100, Race = 50, Level = 10, Hp = 100, LifeAttrib = 0, AntiMagic = 0 }, new MonsterSpec { X = 103, Y = 100, Race = 50, Level = 10, Hp = 100, LifeAttrib = 0, AntiMagic = 0 } }, TargetMonster = 1, TargetXy = new[] { 110, 100 }, UserMagicLevelOverride = 3, AmuletDura = 100, SeedOverride = 1, ObserveState = true, TargetMonsterUndead = false, IdBase = 6300 },
                // 反臂：玩家目标与 mon0 同格 (101,100) 且 `ObjList` 里先于怪 ⇒ 唯一一次护身符被玩家夺走 ⇒
                // 三只怪零 `8037`、`TargetCret` 落在玩家目标。
                new ExtraCaseSpec { Label = "multi_amulet_player_first", Monsters = new[] { new MonsterSpec { X = 101, Y = 100, Race = 50, Level = 10, Hp = 100, LifeAttrib = 0, AntiMagic = 0 }, new MonsterSpec { X = 102, Y = 100, Race = 50, Level = 10, Hp = 100, LifeAttrib = 0, AntiMagic = 0 }, new MonsterSpec { X = 103, Y = 100, Race = 50, Level = 10, Hp = 100, LifeAttrib = 0, AntiMagic = 0 } }, TargetMonster = 1, UserMagicLevelOverride = 3, AmuletDura = 100, SeedOverride = 1, ObserveState = true, TargetMonsterUndead = false, IdBase = 6400 },
            } });
            // **MG-50（2026-11-26）**：护身符 `Shape=0` 的 fall-through——`switch` 无 default ⇒ 不投毒
            // （无 `8037`），但骰过且 race 合格 ⇒ `result` **仍 true**（`:929-932` 在 switch 之外）⇒ 训练尾 640 在。
            if (name == "groupamyounsul-shape0") return BehaviorBatch(new MagicSpec { Key = "groupamyounsul-shape0", MagicId = 48, MagicName = "GroupAmyounsul", ArmRingAmulet = true, HitSpeed = 7, AmuletShape = 0, StepDrainMs = 100, FrameOrderScope = "multi_victim_same_tick" });
            if (name == "groupamyounsul-shape2") return BehaviorBatch(new MagicSpec { Key = "groupamyounsul-shape2", MagicId = 48, MagicName = "GroupAmyounsul", ArmRingAmulet = true, HitSpeed = 7, AmuletShape = 2, StepDrainMs = 100, FrameOrderScope = "multi_victim_same_tick" });
            // **MG-47（35 `SKILL_WINDTEBO`）**：`GetPoseCreate` 取**前格**实体 ⇒ 装置用 `EnableMaps`
            // 设 `caster.Dir`（朝点击方向 = 东）⇒ 前格 (101,100) 的 target 成为推挤目标。
            // 判据：golden 必须 ≥1 条 `SM_BACKSTEP(9)`（退化守卫；零条即具名红）。
            if (name == "windtebo") return BehaviorBatch(new MagicSpec { Key = "windtebo", MagicId = 35, MagicName = "WindTebo", EnableMaps = true });
            if (name == "snowwind") return BehaviorBatch(new MagicSpec { Key = "snowwind", MagicId = 33, MagicName = "SnowWind" , CasterMcNoClamp = HUtil32.MakeWord(5, 10), StepDrainMs = 100 });
            // 9 `SKILL_FIRE`（`MagicManager.cs:265-274`）：沿方向穿透 **5 格**（`MagPassThroughMagic`，
            // `BaseObject.cs:896-931`）。摆位理由（变异牙）：
            //   · 施法者 (100,100) → 点击 (101,100) ⇒ 方向东，检查格 x=101..105；
            //   · x=101 是玩家 target（AntiMagic=夹具值），x=102/103/104 放 3 只怪，
            //     **AntiMagic = {1, 9, 1} 互异** ⇒ `Random(10) >= AntiMagic` 让"必中/必不中/随 seed"三态并存；
            //   · 刀①：删骰门抽数 ⇒ Rng 计数/序列变；刀②：AntiMagic 恒 0 ⇒ 命中集合变（8044 条数）；
            //     刀③：穿透步数 5→8 ⇒ 第 5 格之外的对象进入（本夹具在 x=106 放一只，便于该刀变红）。
            //   · 禁止"空线"：没有怪时骰门零抽、RM 零条 ⇒ 退化数据，生成后自查会拦。
            // 10 疾光电影与 9 同形（`MagicManager.cs:277-289`）：`MagPassThroughMagic(..., 8, true)`
            // ⇒ 穿透 8 格 + `undeadAttack=true`（**逐击** `Round(pow*1.5)` 复合）。装置同形，
            // 仅魔号/名字不同；两者共用本支 spec。
            if (name is "fire" or "skill10") return BehaviorBatch(new MagicSpec
            {
                Key = name, MagicId = name == "fire" ? 9 : 10,
                MagicName = name == "fire" ? "Fire" : "ShootLighten",
                CasterMcNoClamp = HUtil32.MakeWord(5, 10),
                // 场景里 obs_in(105,100) 落在穿透线上，与 target 同刻受击 ⇒ 帧序用多重集
                // （产品批量 flush vs oracle 逐 actor；产品卡修复后删此标记并恢复严格按序）。
                FrameOrderScope = "multi_victim_same_tick",
                // **`due_ms_rel` 的建模约定（写进 golden `scenario_note`）**：施法批（17/638）= 0
                // 是**建模约定**——oracle 实际在下一个 `Run()` 处理（≈100ms），那是投递调度语义、
                // 不是生成时刻语义；本字段**不作为行为判据**，只用于区分"施法批 vs 交付批"与
                // 交付帧的先后（640=1000ms、31/53/656=600ms）。
                ScenarioNote = "登录序（UserLogon→RecalcLevelAbilitys+RecalcAbilitys；target/caster 同标准，HP=公式 MaxHP）；"
                    + "帧序用步进排水 StepDrainMs=100；due_ms_rel 施法批=0 是建模约定（oracle 实际在下一个 Run 处理，≈100ms），不作为行为判据",
                // **时间忠实排水**：oracle 的 640 在 +1000ms、穿透受击 31 在 +600ms ⇒ 真实次序
                // 31 在前、640 在后。默认口径是"Sleep(WaitMs) 后一次排空"，多 pass 的 per-actor
                // 顺序会把 caster 的 640 排在 pass1、target 的 31 广播排在 pass2（与时间序相反）。
                // 步进排水按到期时刻处理，golden 才与生产（逐 tick 交付）同序。
                StepDrainMs = 100,
                Monsters = new[]
                {
                    new MonsterSpec { X = 102, Y = 100, Race = 50, Level = 1, Hp = 300, AntiMagic = 1 },
                    new MonsterSpec { X = 103, Y = 100, Race = 50, Level = 1, Hp = 300, AntiMagic = 9 },
                    new MonsterSpec { X = 104, Y = 100, Race = 50, Level = 1, Hp = 300, AntiMagic = 1 },
                    new MonsterSpec { X = 106, Y = 100, Race = 50, Level = 1, Hp = 300, AntiMagic = 1 },
                },
            });
            Console.Error.WriteLine("未知用例: " + name);
            return 2;
        }
        }
        catch (FixtureContractViolation ex)
        {
            Console.Error.WriteLine("RED FIXTURE_CONTRACT " + ex.Message);
            return 2;
        }
        Console.Error.WriteLine("用法: MagicGolden --rng-selftest   （魔法用例生成器在卡 openmir2-magic-parity 后续提交）");
        return 2;
    }

    private static readonly string[] ConfNames = { "Server.conf", "String.conf", "Exps.conf", "Global.conf", "setting.conf" };

    /// <summary>
    /// 部署配置 `Server.conf` 的 sha256（金标准夹具里落它的来源证明：范围类魔法的 range 值
    /// 必须与**部署配置**同源，卡 §31）。优先取本进程工作目录里 `EnsureConfFiles` 拷进来的副本。
    /// </summary>
    private static string ServerConfSha256()
    {
        var copied = Path.Combine(AppContext.BaseDirectory, "Server.conf");
        var src = Environment.GetEnvironmentVariable("MAGICGOLDEN_CONF_DIR") ?? @"E:\MirServer\M2GameSvr";
        var path = File.Exists(copied) ? copied : Path.Combine(src, "Server.conf");
        using var sha = System.Security.Cryptography.SHA256.Create();
        using var fs = File.OpenRead(path);
        return Convert.ToHexString(sha.ComputeHash(fs)).ToLowerInvariant();
    }

    /// <summary>把冻结部署的 5 个配置文件拷进输出目录（缺哪份拷哪份）；只读源，不改进生产文件。</summary>
    private static bool EnsureConfFiles(out List<string> copied, out List<string> missing)
    {
        string baseDir = AppContext.BaseDirectory;
        string confDir = Environment.GetEnvironmentVariable("MAGICGOLDEN_CONF_DIR")
                         ?? @"E:\MirServer\M2GameSvr";
        return EnsureConfFilesFrom(confDir, baseDir, out copied, out missing);
    }

    /// <summary>
    /// 把 `confDir` 的 5 份冻结配置同步到 `baseDir`：**按内容身份**（sha256）比较，
    /// 不同才拷贝。只读源，不改任何生产文件。
    ///
    /// **为什么不是"存在即跳过"**（卡 `openmir2-ensureconffiles-stale-copy`）：
    /// 只判 `File.Exists(dst)` 会让"换 conf 目录/改源内容"静默沿用上一次的副本，
    /// 任何"换配置的对照臂"都会变成假绿；`ServerConfSha256()` 又优先读副本 ⇒ golden 会
    /// **自洽地**记录错的来源。
    ///
    /// `copied` = 本轮真正写过的文件名（同源重复运行 ⇒ 空）；`missing` = 源不存在的路径。
    /// </summary>
    private static bool EnsureConfFilesFrom(string confDir, string baseDir,
        out List<string> copied, out List<string> missing)
    {
        copied = new List<string>();
        missing = new List<string>();
        foreach (var n in ConfNames)
        {
            var src = Path.Combine(confDir, n);
            var dst = Path.Combine(baseDir, n);
            if (!File.Exists(src)) { missing.Add(src); continue; }
            // 0 字节也算"内容不同"（sha 不同），无须单列。
            if (!File.Exists(dst) || FileSha256(dst) != FileSha256(src))
            {
                File.Copy(src, dst, overwrite: true);
                copied.Add(n);
            }
        }
        return missing.Count == 0;
    }

    private static string FileSha256(string path)
    {
        using var sha = System.Security.Cryptography.SHA256.Create();
        using var fs = File.OpenRead(path);
        return Convert.ToHexString(sha.ComputeHash(fs));
    }

    /// <summary>
    /// `--conf-selftest`：`EnsureConfFilesFrom` 的**身份**语义三把牙（卡
    /// `openmir2-ensureconffiles-stale-copy` §6）。只用 `Path.GetTempPath()` 下的临时目录
    /// （A/B 源 + 临时 base），**不碰部署目录、不改任何生产文件**。
    /// </summary>
    private static int ConfFilesSelfTest()
    {
        string root = Path.Combine(Path.GetTempPath(),
            "magicgolden-conf-selftest-" + Guid.NewGuid().ToString("N"));
        string dirA = Path.Combine(root, "A");
        string dirB = Path.Combine(root, "B");
        string baseDir = Path.Combine(root, "base");
        try
        {
            Directory.CreateDirectory(dirA);
            Directory.CreateDirectory(dirB);
            Directory.CreateDirectory(baseDir);
            foreach (var n in ConfNames)
            {
                File.WriteAllText(Path.Combine(dirA, n), "A:" + n);
                File.WriteAllText(Path.Combine(dirB, n), "B:" + n);
            }
            int failures = 0;
            void Check(string name, bool cond, string detail)
            {
                if (cond) { Console.Error.WriteLine("CONF_SELFTEST OK " + name); return; }
                Console.Error.WriteLine("CONF_SELFTEST FAIL " + name + " :: " + detail);
                failures++;
            }
            string DstSha() => FileSha256(Path.Combine(baseDir, "Server.conf"));

            // ① 空 base + 源 A ⇒ 5 份全拷
            bool okA = EnsureConfFilesFrom(dirA, baseDir, out var copied1, out _);
            Check("copy-from-A", okA && copied1.Count == ConfNames.Length
                    && DstSha() == FileSha256(Path.Combine(dirA, "Server.conf")),
                $"copied={copied1.Count}");

            // ② 换源 B（同名不同内容）⇒ 必须重拷（旧实现"存在即跳过"会静默留在 A）
            bool okB = EnsureConfFilesFrom(dirB, baseDir, out var copied2, out _);
            Check("switch-to-B-recopies", okB && copied2.Count == ConfNames.Length
                    && DstSha() == FileSha256(Path.Combine(dirB, "Server.conf")),
                $"copied={copied2.Count}");

            // ③ 源 B 就地改内容 ⇒ 必须重拷 Server.conf
            File.WriteAllText(Path.Combine(dirB, "Server.conf"), "B2:" + "Server.conf");
            bool okB2 = EnsureConfFilesFrom(dirB, baseDir, out var copied3, out _);
            Check("content-change-recopies", okB2 && copied3.Count == 1 && copied3[0] == "Server.conf"
                    && DstSha() == FileSha256(Path.Combine(dirB, "Server.conf")),
                $"copied=[{string.Join(",", copied3)}]");

            // ④ 同源连跑 ⇒ 不重拷（防"每次都覆盖"这种相反的错误）
            bool okSame = EnsureConfFilesFrom(dirB, baseDir, out var copied4, out _);
            Check("same-source-no-copy", okSame && copied4.Count == 0,
                $"copied=[{string.Join(",", copied4)}]");

            return failures == 0 ? 0 : 2;
        }
        finally
        {
            try { Directory.Delete(root, recursive: true); } catch { /* 临时目录清理失败不影响判据 */ }
        }
    }

    /// <summary>
    /// Slice 1 自检：`SystemShare` 静态构造会从 `AppContext.BaseDirectory` 读 5 个配置文件
    /// （`src/Modules/SystemModule/SystemShare.cs:183-199`）。本方法把冻结部署里的 conf 拷进
    /// 输出目录（缺哪份拷哪份），然后读 `SystemShare.Config.MagicAttackRage` 等值，
    /// 证明魔法路径依赖的配置面在离线进程里可用。**只读冻结配置，不修改任何生产文件。**
    /// </summary>
    private static int ConfigSelfTest()
    {
        string baseDir = AppContext.BaseDirectory;
        string confDir = Environment.GetEnvironmentVariable("MAGICGOLDEN_CONF_DIR")
                         ?? @"E:\MirServer\M2GameSvr";
        string[] names = { "Server.conf", "String.conf", "Exps.conf", "Global.conf", "setting.conf" };
        var copied = new List<string>();
        var missing = new List<string>();
        foreach (var n in names)
        {
            var src = Path.Combine(confDir, n);
            var dst = Path.Combine(baseDir, n);
            if (!File.Exists(src)) { missing.Add(src); continue; }
            if (!File.Exists(dst))
            {
                File.Copy(src, dst);
                copied.Add(n);
            }
        }
        if (missing.Count > 0)
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        // 牙 A（opc-07 2026-11-20）：**0 字节副本必须被重拷**。预置 0 字节 Exps.conf ⇒
        // EnsureConfFiles 后文件必须非空、且 Level20 可读出（否则以后同类缺载还会漏过去）。
        string expsDst = Path.Combine(baseDir, "Exps.conf");
        if (File.Exists(expsDst))
        {
            File.WriteAllBytes(expsDst, Array.Empty<byte>());
            EnsureConfFiles(out _, out _);
            var text = File.ReadAllText(expsDst);
            bool reCopied = new FileInfo(expsDst).Length > 0 && text.Contains("Level20=");
            Console.Error.WriteLine((reCopied ? "ZERO_CONF_RECOPY OK" : "ZERO_CONF_RECOPY FAILED"));
            if (!reCopied)
            {
                return 1;
            }
        }
        // 生产路径在  初始化显式调 ； 静态构造不调。
        SystemModule.SystemShare.ExpConf.LoadConfig();
        var cfg = SystemModule.SystemShare.Config;
        // 牙 B：Exps 必须真的读进来（此前 `--config-selftest` 从不读 Exps ⇒ 缺载也能绿）。
        int needExp20 = cfg.NeedExps.Length > 20 ? cfg.NeedExps[20] : -1;
        if (cfg.NeedExps.Length <= 20 || needExp20 != 6000)
        {
            Console.Error.WriteLine("CONF_EXPS_FAILED need_exps_len=" + cfg.NeedExps.Length + " level20=" + needExp20);
            return 1;
        }
        Console.Error.WriteLine("CONF_EXPS_OK level20=" + needExp20);
        var outObj = new
        {
            base_dir = baseDir,
            conf_dir = confDir,
            copied = copied,
            magic_attack_rage = cfg.MagicAttackRage,
            send_ref_msg_range = cfg.SendRefMsgRange,
            need_exps_level20 = needExp20,
            need_exps_len = cfg.NeedExps.Length,
            fireboom_rage = cfg.FireBoomRage,
            items = cfg.ItemNumber,
        };
        Console.Out.WriteLine(JsonSerializer.Serialize(outObj, new JsonSerializerOptions { WriteIndented = true }));
        bool ok = cfg.MagicAttackRage > 0 && cfg.SendRefMsgRange > 0;
        Console.Error.WriteLine(ok ? "CONFIG_SEAM_SELFTEST OK" : "CONFIG_SEAM_SELFTEST FAILED");
        return ok ? 0 : 1;
    }

    /// <summary>
    /// 装置骨架自检：
    ///  1) 替换点可用（反射找到私有字段并注入）；
    ///  2) 固定种子 ⇒ 逐次抽取序列可复现（两次安装序列相同）；
    ///  3) 不同种子 ⇒ 序列不同（证明不是恒等/没接上）；
    ///  4) 还原后字段回到原实例。
    /// </summary>
    /// <summary>
    /// **接缝行为牙**：证明 `HUtil32.GetTickCount` 的时钟 seam「默认惰性、设置后固定」。默认 null ⇒ 随真实时间前进；
    /// 设置后 ⇒ 立即返回该值、不随真实时间变；Clear ⇒ 还原。
    /// </summary>
    private static int ClockSeamSelfTest()
    {
        int fails = 0;
        void Check(bool ok, string what) { Console.WriteLine((ok ? "PASS " : "FAIL ") + what); if (!ok) fails++; }
        ClockSeam.Clear();
        int a = HUtil32.GetTickCount();
        Thread.Sleep(90);
        int b = HUtil32.GetTickCount();
        int delta = b - a;
        Console.WriteLine($"[seam@default] a={a} b={b} delta={delta}ms (expect ~90)");
        Check(delta >= 60 && delta < 500, "default null: 随真实时间前进(delta~sleep)");
        Check(b >= a, "default null: 单调不减");
        ClockSeam.Set(123456789);
        int c = HUtil32.GetTickCount();
        Thread.Sleep(60);
        int d = HUtil32.GetTickCount();
        Console.WriteLine($"[seam@set] c={c} d={d} (expect both 123456789)");
        Check(c == 123456789, "set: 立即返回注入值");
        Check(d == 123456789, "set: 不随真实时间变");
        ClockSeam.Clear();
        int e = HUtil32.GetTickCount();
        Console.WriteLine($"[seam@cleared] e={e} (expect != 123456789)");
        Check(e != 123456789, "clear: 还原为真实时间");
        Console.WriteLine(fails == 0 ? "CLOCK_SEAM_SELFTEST_OK" : "CLOCK_SEAM_SELFTEST_FAILED fails=" + fails);
        return fails == 0 ? 0 : 1;
    }

    private static int RngSelfTest()
    {
        var rn = RandomNumber.GetInstance();

        var original = RngSeam.Capture();
        var a = RngSeam.Install(12345);
        var seqA = new List<int>();
        for (int i = 0; i < 8; i++) seqA.Add(rn.Random(100));
        var drawsA = a.Draws.ToArray();
        RngSeam.Restore(original);

        var b = RngSeam.Install(12345);
        var seqB = new List<int>();
        for (int i = 0; i < 8; i++) seqB.Add(rn.Random(100));
        var drawsB = b.Draws.ToArray();
        RngSeam.Restore(original);

        var c = RngSeam.Install(999);
        var seqC = new List<int>();
        for (int i = 0; i < 8; i++) seqC.Add(rn.Random(100));
        RngSeam.Restore(original);

        bool sameSeedSameSeq = seqA.SequenceEqual(seqB) && drawsA.SequenceEqual(drawsB);
        bool diffSeedDiffSeq = !seqA.SequenceEqual(seqC);
        bool restored = ReferenceEquals(RngSeam.Capture(), original);
        bool ledgerMatches = seqA.Count == drawsA.Length;

        var result = new
        {
            seam = "OpenMir2.RandomNumber.random",
            seed = 12345,
            seq = seqA,
            draws = drawsA,
            same_seed_same_seq = sameSeedSameSeq,
            diff_seed_diff_seq = diffSeedDiffSeq,
            restored = restored,
            ledger_matches = ledgerMatches,
        };
        Console.Out.WriteLine(JsonSerializer.Serialize(result, new JsonSerializerOptions { WriteIndented = true }));

        bool ok = sameSeedSameSeq && diffSeedDiffSeq && restored && ledgerMatches;
        Console.Error.WriteLine(ok ? "RNG_SEAM_SELFTEST OK" : "RNG_SEAM_SELFTEST FAILED");
        return ok ? 0 : 1;
    }

    /// <summary>
    /// Slice 3 用例：`1/5 火球`（`MagicManager.cs:174-203`）语义层对拍（C# 侧）。
    /// 三支：未命中（首抽 &lt; AntiMagic）、边界（首抽 == AntiMagic）、命中（首抽 &gt; AntiMagic）。
    /// 种子由扫描确定并**写进产物**，Rust 侧按同一 JSON 复现。
    /// </summary>
    private static string ArgValue(string[] args, string key)
    {
        int i = Array.IndexOf(args, key);
        return i >= 0 && i + 1 < args.Length ? args[i + 1] : null;
    }

    /// <summary>
    /// `--case antimagic-selftest`（B 的牙）：登录序下生效 AntiMagic = 1（见 `LoginOrderAntiMagic`），
    /// 断言三支首抽分别为 0 / 1 / &gt;=2，且每支 `Fixture.seed_reason` 写明首抽/种子/门槛；
    /// 另钉住 A 的守卫（`EnsureCoverAccounting` 伪造 3 &lt; 5 ⇒ 违规；相等或 freeze 口径 ⇒ 放行）、
    /// 分支判定（miss 支首抽 == 门槛 ⇒ 违规）与门槛断言（实测 3 != 门槛 1 ⇒ 违规）。
    /// </summary>
    private static int AntiMagicSelfTest()
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        int failures = 0;
        void Check(string name, bool ok, string detail)
        {
            Console.Error.WriteLine("ANTIMAGIC_SELFTEST " + (ok ? "PASS " : "FAIL ") + name + " :: " + detail);
            if (!ok) failures++;
        }
        var spec = new MagicSpec { Key = "lightning-11", MagicId = 11, MagicName = "Lightening" };
        var cases = RunAntiMagicTriplet(spec);
        if (cases == null)
        {
            Console.Error.WriteLine("SEED_SCAN_FAILED");
            return 1;
        }
        long First(FireballResult r) => r.Rng.Count > 0 ? r.Rng[0] : -1;
        Check("miss-first-draw", First(cases[0]) == 0, "first=" + First(cases[0]));
        Check("edge-first-draw", First(cases[1]) == 1, "first=" + First(cases[1]));
        Check("hit-first-draw", First(cases[2]) >= 2, "first=" + First(cases[2]));
        for (int i = 0; i < cases.Length; i++)
        {
            var fx = cases[i].Fixture;
            int anti = (int)fx.GetType().GetProperty("anti_magic").GetValue(fx);
            string reason = (string)fx.GetType().GetProperty("seed_reason").GetValue(fx);
            long total = Convert.ToInt64(fx.GetType().GetProperty("rng_total_draws").GetValue(fx));
            long recorded = Convert.ToInt64(fx.GetType().GetProperty("rng_recorded").GetValue(fx));
            Check("effective-anti#" + cases[i].Label, anti == LoginOrderAntiMagic, "fixture.anti_magic=" + anti);
            Check("seed-reason#" + cases[i].Label, !string.IsNullOrEmpty(reason) && reason.Contains("门槛 anti_magic=1"), "reason=" + reason);
            Check("cover-accounting#" + cases[i].Label, total == recorded, "total=" + total + " recorded=" + recorded);
        }
        Check("guard-bite", ThrowsFixtureViolation(() => EnsureCoverAccounting("cover_drain", 3, 5, "selftest")), "cover_drain 3 != 5 应违规");
        Check("guard-pass-equal", !ThrowsFixtureViolation(() => EnsureCoverAccounting("cover_drain", 5, 5, "selftest")), "相等应放行");
        Check("guard-freeze-legacy", !ThrowsFixtureViolation(() => EnsureCoverAccounting("freeze_at_dospell", 3, 5, "selftest")), "freeze 只记不判");
        Check("branch-bite", ThrowsFixtureViolation(() => CheckAntiMagicBranch("selftest", new long[] { 1 }, 14, new AntiMagicBranchExpect { Kind = "miss", Threshold = 1 })), "miss 首抽=门槛 应违规");
        Check("threshold-bite", ThrowsFixtureViolation(() => CheckBranchThreshold("selftest", 3, new AntiMagicBranchExpect { Kind = "hit", Threshold = 1 })), "实测 3 != 门槛 1 应违规");
        Console.Error.WriteLine(failures == 0 ? "ANTIMAGIC_SELFTEST OK" : ("ANTIMAGIC_SELFTEST RED failures=" + failures));
        return failures == 0 ? 0 : 1;
    }

    private static bool ThrowsFixtureViolation(Action a)
    {
        try { a(); return false; }
        catch (FixtureContractViolation) { return true; }
    }

    /// <summary>
    /// `--case fireball-selftest`：牙①（有排水抽的 cover vs freeze 可区分）+ 第 8 道门两例
    /// （设了 MaxExp ⇒ 绿 / 不设 ⇒ rc=2）。
    /// </summary>
    private static int FireballSelfTest()
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        int failures = 0;
        // 牙①：同一 case（hit，seed 13）在两种 seam 下的 Rng 对照。
        // freeze 只与 login_order=false 合法（门 3）；两模式都在**同一合成夹具**（anti=3）上跑，
        // 差异只来自 seam：cover 含排水期抽取、freeze 不含。
        var cover = RunFireball("selftest", 13, 3, 7000, false, "cover_drain");
        var freeze = RunFireball("selftest", 13, 3, 8000, false, "freeze_at_dospell");
        // 探针（牙①）：注入一次「排水前」抽取。cover 必须记到（+1），freeze 不得记到（0 增量）。
        var coverProbe = RunFireball("selftest-probe", 13, 3, 9000, false, "cover_drain", probeDrainDraw: true);
        var freezeProbe = RunFireball("selftest-probe", 13, 3, 9100, false, "freeze_at_dospell", probeDrainDraw: true);
        bool probeCoverCaught = coverProbe.Rng.Count == cover.Rng.Count + 1;
        bool probeFreezeMissed = freezeProbe.Rng.Count == freeze.Rng.Count;
        Console.Error.WriteLine("FIREBALL_SEAM_PROBE cover=" + cover.Rng.Count + "->" + coverProbe.Rng.Count
            + " freeze=" + freeze.Rng.Count + "->" + freezeProbe.Rng.Count
            + " cover_caught=" + probeCoverCaught + " freeze_missed=" + probeFreezeMissed);
        if (!probeCoverCaught || !probeFreezeMissed)
        {
            Console.Error.WriteLine("FIREBALL_SELFTEST FAIL seam-probe");
            failures++;
        }
        else
        {
            Console.Error.WriteLine("FIREBALL_SELFTEST PASS seam-probe (cover catches drain-window draw; freeze does not)");
        }
        bool distinguishable = cover.Rng.Count != freeze.Rng.Count
            || !cover.Rng.SequenceEqual(freeze.Rng);
        Console.Error.WriteLine("FIREBALL_SEAM_DIFF cover_len=" + cover.Rng.Count + " freeze_len=" + freeze.Rng.Count
            + " cover=" + string.Join(",", cover.Rng) + " freeze=" + string.Join(",", freeze.Rng)
            + (distinguishable ? " DISTINGUISHABLE" : " IDENTICAL"));
        if (!distinguishable)
        {
            // 火球自身无排水抽：两模式相同属**结构性**结果（解释见卡面），不再据此判红；
            // seam 是否真覆盖排水由下面的注入探针证明。
            Console.Error.WriteLine("FIREBALL_SEAM_DIFF_INFO identical-by-construction (fireball draws only at cast; see probe)");
        }
        // MaxExp 门两例
        var derived = FixtureContract.Recompute(OpenMir2.Enums.PlayerJob.Warrior, 20);
        FixtureContract.FixtureInput Make(int? maxExp) => new()
        {
            Label = "selftest",
            RngMode = "cover_drain",
            LoginOrder = true,
            Reachability = "reachable",
            AntiMagic = 1,
            TargetLevel = 20,
            TargetHp = derived.MaxHp,
            TargetMaxHp = derived.MaxHp,
            Derived = derived,
            Construction = new FixtureContract.Construction { Job = OpenMir2.Enums.PlayerJob.Warrior, Level = 20, Equipment = new List<string>(), Skills = new List<string> { "FireBall" } },
            ScenarioNote = "",
            ActualMaxExp = maxExp,
        };
        try
        {
            FixtureContract.Validate(Make(derived.MaxExp));
            Console.Error.WriteLine("FIREBALL_SELFTEST PASS maxexp-set-green (expect=" + derived.MaxExp + ")");
        }
        catch (FixtureContractViolation ex)
        {
            Console.Error.WriteLine("FIREBALL_SELFTEST FAIL maxexp-set-green :: " + ex.Message);
            failures++;
        }
        try
        {
            FixtureContract.Validate(Make(50));
            Console.Error.WriteLine("FIREBALL_SELFTEST FAIL maxexp-unset-red :: 期望 rc=2 的输入通过了校验");
            failures++;
        }
        catch (FixtureContractViolation)
        {
            Console.Error.WriteLine("FIREBALL_SELFTEST PASS maxexp-unset-red (rc=2)");
        }
        Console.Error.WriteLine(failures == 0 ? "FIREBALL_SELFTEST OK" : ("FIREBALL_SELFTEST RED failures=" + failures));
        return failures == 0 ? 0 : 1;
    }

    private static int FireballCase(string rngMode, bool loginOrder)
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        const int antiMagic = 3;
        // 种子门槛取**生效值**：登录序里 `RecalcAbilitys()` 把 AntiMagic 置 1（构造期预置的 3 被覆盖）；
        // 用 3 选种子会让三支首抽（2/3/5）在门槛 1 下全是 hit（miss/edge 丢失）。
        int effectiveAnti = loginOrder ? LoginOrderAntiMagic : antiMagic;
        int seedMiss = FindSeedForFirstDraw(10, effectiveAnti - 1);
        int seedEdge = FindSeedForFirstDraw(10, effectiveAnti);
        int seedHit = FindSeedForFirstDraw(10, effectiveAnti + 2);
        if (seedMiss < 0 || seedEdge < 0 || seedHit < 0)
        {
            Console.Error.WriteLine("SEED_SCAN_FAILED");
            return 1;
        }
        var cases = new[]
        {
            RunFireball("miss", seedMiss, antiMagic, 1000, loginOrder, rngMode, branchExpect: new AntiMagicBranchExpect { Kind = "miss", Threshold = effectiveAnti }),
            RunFireball("edge", seedEdge, antiMagic, 2000, loginOrder, rngMode, branchExpect: new AntiMagicBranchExpect { Kind = "edge", Threshold = effectiveAnti }),
            RunFireball("hit", seedHit, antiMagic, 3000, loginOrder, rngMode, branchExpect: new AntiMagicBranchExpect { Kind = "hit", Threshold = effectiveAnti }, mc: HUtil32.MakeWord(5, 10)),
        };
        var outObj = new { magic_id = 1, anti_magic = antiMagic, cases };
        var json = JsonSerializer.Serialize(outObj, new JsonSerializerOptions { WriteIndented = true });
        var outIdx = Array.IndexOf(Environment.GetCommandLineArgs(), "--out");
        if (outIdx > 0 && outIdx + 1 < Environment.GetCommandLineArgs().Length)
        {
            var outPath = Environment.GetCommandLineArgs()[outIdx + 1];
            Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outPath))!);
            File.WriteAllText(outPath, json);
            Console.Error.WriteLine("GOLDEN_WRITTEN " + outPath);
        }
        Console.Out.WriteLine(json);
        Console.Error.WriteLine("FIREBALL_CASES " + string.Join(", ", cases.Select(c => c.Label + "[rng=" + c.Rng.Count + "; " + string.Join(" ", c.Receivers.Select(r => r.Who + ":rm" + r.Rm.Count + "/sm" + r.Sent.Count)) + "]")));
        return 0;
    }

    /// <summary>扫描种子：找到首个 `Random(mod)` 抽取等于 want 的最小种子（1..200000）。</summary>
    private static int FindSeedForFirstDraw(int mod, int want)
    {
        var rn = RandomNumber.GetInstance();
        var original = RngSeam.Capture();
        for (int seed = 1; seed <= 200000; seed++)
        {
            RngSeam.Install(seed);
            int v = rn.Random(mod);
            RngSeam.Restore(original);
            if (v == want) return seed;
        }
        return -1;
    }

    /// <summary>
    /// 生效 AntiMagic（种子门槛）：`RunMagicCase`/`RunFireball` 的登录序分支**恒**跑
    /// `RecalcLevelAbilitys(); RecalcAbilitys();`（`PlayObject.cs:1308-1309`），而
    /// `RecalcAbilitys()` 无条件 `AntiMagic = 1` 再 `+= AddAbil.AntiMagic`（`BaseObject.Abil.cs`）；
    /// 本装置 `Equipment` 恒空 ⇒ 生效值 = 1。构造期预置的 `MagicSpec.AntiMagic`（缺省 3）会被登录序
    /// **覆盖**，不能当种子门槛——否则 miss/edge/hit 三支退化为同一支（首抽 >=1 全过）。
    /// 该预测值在每次生成时与实测 `target.AntiMagic` 断言一致（见 `CheckBranchThreshold`）。
    /// </summary>
    private const int LoginOrderAntiMagic = 1;

    /// <summary>
    /// 分支语义（骰 `Random(10) >= AntiMagic` 才通过；11/45/13/44 与火球同式）：
    /// miss = 首抽 &lt; 门槛、edge = 首抽 == 门槛、hit = 首抽 &gt; 门槛。
    /// </summary>
    internal sealed class AntiMagicBranchExpect
    {
        public string Kind;
        public int Threshold;
    }

    private static void CheckBranchThreshold(string where, int actualAntiMagic, AntiMagicBranchExpect expect)
    {
        if (expect == null) return;
        if (actualAntiMagic != expect.Threshold)
        {
            throw new FixtureContractViolation(
                $"[{where}] 种子门槛与生效 AntiMagic 不符：门槛={expect.Threshold} 实测 target.AntiMagic={actualAntiMagic}"
                + "（登录序 RecalcAbilitys 会把 AntiMagic 置 1；改装备/改登录序后必须重新推导门槛）");
        }
    }

    private static string BuildSeedReason(long firstDraw, int seed, AntiMagicBranchExpect expect)
        => $"首抽={firstDraw}(seed {seed})，门槛 anti_magic={expect.Threshold}（骰 Random(10) >= 门槛 才通过；{expect.Kind}）";

    private static void CheckAntiMagicBranch(string where, IReadOnlyList<long> draws, int seed, AntiMagicBranchExpect expect)
    {
        if (expect == null) return;
        if (draws.Count == 0)
        {
            throw new FixtureContractViolation($"[{where}] 该支无任何抽取，无法判定 {expect.Kind} 分支（种子 seam 失效？）");
        }
        long first = draws[0];
        bool ok = expect.Kind switch
        {
            "miss" => first < expect.Threshold,
            "edge" => first == expect.Threshold,
            "hit" => first > expect.Threshold,
            _ => throw new FixtureContractViolation($"[{where}] 未知分支 kind={expect.Kind}"),
        };
        if (!ok)
        {
            throw new FixtureContractViolation(
                $"[{where}] 分支与首抽不符：kind={expect.Kind} 首抽={first}(seed {seed}) 门槛={expect.Threshold}"
                + "（三支必须分别落在 小于 / 等于 / 大于 门槛 三段）");
        }
    }

    /// <summary>
    /// `cover_drain` 记账完整性：`Rng`（排水结束后的快照）必须等于该 seam 实例累计抽取数
    /// （`rng_total_draws`）。捕获点若被挪到排水之前，两者不等 ⇒ 生成失败（不是静默产出短账本）。
    /// `freeze_at_dospell` 是 legacy 口径（本来漏记排水抽取），只记两值不判等。
    /// </summary>
    internal static void EnsureCoverAccounting(string rngMode, long recorded, long totalDraws, string where)
    {
        if (rngMode != "cover_drain") return;
        if (recorded != totalDraws)
        {
            throw new FixtureContractViolation(
                $"[{where}] cover_drain 记账不完整：Rng.Count={recorded} != rng_total_draws={totalDraws}");
        }
    }

    /// <summary>用给定种子跑一次火球，返回账本（各接收者的 RM 与真实 SM 帧 + RNG + 关键状态）。</summary>
    private static FireballResult RunFireball(string label, int seed, int antiMagic, int idBase, bool loginOrder, string rngMode, bool probeDrainDraw = false, AntiMagicBranchExpect branchExpect = null, int mc = 15)
    {
        ClockSeam.Begin(1000000);
        string mapDir = Environment.GetEnvironmentVariable("MAGICGOLDEN_MAP_DIR") ?? @"E:\MirServer\M2GameSvr\Map";
        var env = new M2Server.Maps.Envirnoment();
        if (!env.LoadMapData(Path.Combine(mapDir, "0.map")))
        {
            throw new InvalidOperationException("MAP_LOAD_FAILED " + mapDir);
        }
        // 卡 openmir2-golden-harness-mapmgr：设 MapMgr（FindMap 返回本 case 已加载的 env），供 RecallSlave→SpaceMove。
        // 无条件设 MapMgr 安全：其它 spec 不触达 FindMap（否则原 null 会 NPE），且 skeleton/fireball/fire 结构 diff ⊆ drift；EnableMaps 门控：env.MapName/ServerIndex + 预置 slave 的 RecordingMonster 构造 + caster.Dir（均 SINSU）。
        SystemModule.SystemShare.MapMgr = new OfflineMapSystem(env);
        var caster = new RecordingPlayObject { ChrName = "golden-caster", ActorId = idBase + 1, CurrX = 100, CurrY = 100, SoftVersionDateEx = 20240101, ClientTick = 5 };
        caster.Abil.Level = 30;
        caster.WAbil.MC = (ushort)mc;
        var target = new RecordingPlayObject { ChrName = "golden-target", ActorId = idBase + 2, CurrX = 101, CurrY = 100, SoftVersionDateEx = 20240101, ClientTick = 5, AntiMagic = (ushort)antiMagic };
        target.Abil.Level = 20;
        int actualMaxExp = 50;
        if (loginOrder)
        {
            // 登录序：`RecalcLevelAbilitys()+RecalcAbilitys()`（`UserLogon` 同序），并按冻结表显式设
            // `Abil.MaxExp = GetLevelExp(Level)`——不设会被契约第 8 道门判 rc=2（不留 ctor 50）。
            target.RecalcLevelAbilitys();
            // 派生值经  取（内部先  并读冻结表；直接读
            //  会因未 LoadConfig 而得 0）。
            var levelDerived = FixtureContract.Recompute(OpenMir2.Enums.PlayerJob.Warrior, (byte)target.Abil.Level);
            target.Abil.MaxExp = levelDerived.MaxExp;
            actualMaxExp = target.Abil.MaxExp;
            target.RecalcAbilitys();
            target.WAbil.HP = target.WAbil.MaxHP; // 登录后满血（ 保留旧 HP=0，需在之后置）
            target.DrainRaw();
        }
        else
        {
            // 历史合成场景（`AntiMagic=3`/`MaxHP=100`）：保持旧夹具（legacy），不设 MaxExp。
            target.WAbil.MaxHP = 100;
            target.WAbil.HP = 100;
            target.Abil.MaxHP = 100;
        }
        // **输入**语义：`Fixture.target_hp` = 施法前 HP（驱动用它设初始值）；施法后值走结果顶层
        // `TargetHp`（只作断言、不作输入）。
        int targetHpInput = target.WAbil.HP;
        var obsIn = new RecordingPlayObject { ChrName = "golden-obs-in", ActorId = idBase + 3, CurrX = 105, CurrY = 100, SoftVersionDateEx = 20240101, ClientTick = 5 };
        var obsOut = new RecordingPlayObject { ChrName = "golden-obs-out", ActorId = idBase + 4, CurrX = 140, CurrY = 100, SoftVersionDateEx = 20240101, ClientTick = 5 };
        // **观察者是真实玩家**：初始化 HP/MaxHP=100（与 Rust rig 的 `new_player_entity` 默认同源）。
        // 不初始化会让"旁观者自己受击/治疗"的帧（53/31）带 0/0，与实现侧不可比（MG-17 D1）。
        obsIn.WAbil.HP = 100; obsIn.WAbil.MaxHP = 100;
        obsOut.WAbil.HP = 100; obsOut.WAbil.MaxHP = 100;
        // **记录面补全（卡 openmir2-magic-parity A 口径）**：所有下行最终都汇到
        // `M2Share.NetChannel.AddGateBuffer(GateIdx, bytes)`——把静态通道换成只记录不发送的实现，
        // 就能同时捕获双参 virtual `SendSocket(msg, tail)`、私有单参 `SendSocket(msg)`
        // （`SendDefMessage` 走这条）与 `SendSocket(string)` 动作短包。**生产类零改动**。
        // 各玩家给唯一 GateIdx（= ActorId），用于把缓冲归属到接收者。
        var recorder = new RecordingNetChannel();
        M2Server.M2Share.NetChannel = recorder;
        foreach (var p in new[] { caster, target, obsIn, obsOut })
        {
            p.GateIdx = p.ActorId;
            // 世界内正常玩家：不隐身（PlayObject 构造后 FixedHideMode 可能为 true，见卡面"洁癖区"）。
            p.FixedHideMode = false;
            p.ObMode = false;
            p.Envir = env;
            SystemModule.SystemShare.ActorMgr.Add(p);
            bool added = env.AddMapObject(p.CurrX, p.CurrY, SystemModule.Data.CellType.Play, p.ActorId, p);
            if (Environment.GetEnvironmentVariable("MAGICGOLDEN_DIAG") == "1")
            {
                var cell = env.GetCellInfo(p.CurrX, p.CurrY, out bool cellOk);
                Console.Error.WriteLine("DIAG add=" + added + " who=" + p.ChrName + " cellOk=" + cellOk + " cellCount=" + cell.Count + " available=" + cell.IsAvailable + " match=" + env.CellMatch(p.CurrX, p.CurrY));
            }
        }

        var original = RngSeam.Capture();
        var rng = RngSeam.Install(seed);
        var um = new OpenMir2.Packets.ClientPackets.UserMagic
        {
            MagIdx = 1,
            Level = 1,
            TranPoint = 0,
            Magic = new OpenMir2.Data.MagicInfo { MagicId = 1, MagicName = "FireBall", Effect = 1, EffectType = 1 },
        };
        long castTick = HUtil32.GetTickCount();
        bool ok = M2Server.Magic.MagicManager.DoSpell(caster, um, target.CurrX, target.CurrY, target);
        List<long> draws = new();
        if (rngMode == "freeze_at_dospell")
        {
            draws = rng.Draws.ToList();
            RngSeam.Restore(original);
        }
        if (Environment.GetEnvironmentVariable("MAGICGOLDEN_DIAG") == "1")
        {
            Console.Error.WriteLine("DIAG visible=" + string.Join(",", caster.VisibleHumanList) + " envW=" + env.Width + " envH=" + env.Height
                + " get1001=" + (SystemModule.SystemShare.ActorMgr.Get(idBase + 1) != null)
                + " get1003=" + (SystemModule.SystemShare.ActorMgr.Get(idBase + 3) != null)
                + " get1004=" + (SystemModule.SystemShare.ActorMgr.Get(idBase + 4) != null)
                + " obmode=" + caster.ObMode + " fixedhide=" + caster.FixedHideMode + " ghost=" + caster.Ghost);
        }

        // 延时路径：RM_DELAYMAGIC 到期后才走 BaseObject.Base.cs:711（伤害 → RM_MAGSTRUCK）。
        // 真机语义是后续 tick；这里等价地等到到期（600ms）再排水。
        //
        // **排水窗口 = 抽空为止（opc-07 裁定 2026-11-19）**：旧实现固定两趟是**记录面伪影**——
        // RM 由 `Operate` 触发下一条（如 `RM_STRUCK_MAG` ⇒ `SetPkFlag` ⇒ `RM_CHANGENAMECOLOR`），
        // 只跑两趟会把「已入队但尚未排水」的条目从账本里漏掉；真机上它们在后续 tick 都会投给
        // 所有可见者。因此循环到某一趟**没排到任何消息**为止。
        //
        // 上界 `MaxDrainPasses = 8`：防自发消息无限增生（本用例链正常 ≤4 趟）。
        // **上界命中必须可见**（`DrainPassesHitCap` 进产物 + stderr），不得静默截断。
        const int MaxDrainPasses = 8;
        int hpBefore = target.WAbil.HP;
        // **step-drain**：按 StepDrainMs=100 步进到 700ms，逐消息设 `CurrentDueMs`（同 `RunMagicCase` 口径）。
        // 使延迟父消息的 non-late 子帧在"到达那步"记 `due=stepMs`（累计 elapsed），与 Rust 100ms pump 同步量化的 step 一致。
        if (probeDrainDraw)
        {
            _ = M2Server.M2Share.RandomNumber.Random(1000);
        }
        var receivers = new List<ReceiverResult>();
        var pending = new List<object>();
        var byWho = new (string, RecordingPlayObject)[] { ("caster", caster), ("target", target), ("obs_in", obsIn), ("obs_out", obsOut) };
        var rmAcc = new Dictionary<string, List<object>>();
        foreach (var (who, _) in byWho) rmAcc[who] = new List<object>();
        int drainPasses = 0;
        bool drainPassesHitCap = false;
        const int StepDrainMs = 100;
        const int WaitMs = 700;
        void DrainOnce(bool[] operatedAny, long stepMs)
        {
            foreach (var (who, obj) in byWho)
            {
                var raw = obj.DrainRaw();
                int now = HUtil32.GetTickCount();
                int stopAt = raw.Count;
                for (int i = 0; i < raw.Count; i++)
                {
                    var (m, prio) = raw[i];
                    if (!m.LateDelivery || m.DeliveryTime <= now)
                    {
                        rmAcc[who].Add(new
                        {
                            ident = m.wIdent, wParam = m.wParam, p1 = m.nParam1, p2 = m.nParam2, p3 = m.nParam3,
                            msg = m.Buff ?? "", late = m.LateDelivery, delivery = m.DeliveryTime, priority = prio,
                        });
                        RecordingNetChannel.CurrentDueMs = m.LateDelivery ? Math.Max(0, m.DeliveryTime - castTick) : stepMs;
                        obj.OperatePublic(m);
                        RecordingNetChannel.CurrentDueMs = 0;
                        operatedAny[0] = true;
                    }
                    else { stopAt = i; break; }
                }
                for (int i = stopAt; i < raw.Count; i++) obj.PushBack(raw[i].Msg, raw[i].Prio);
            }
        }
        for (int pass = 0; pass < MaxDrainPasses; pass++)
        {
            var operatedAny = new[] { false };
            DrainOnce(operatedAny, 0);
            if (!operatedAny[0]) break;
            if (pass == MaxDrainPasses - 1) drainPassesHitCap = true;
        }
        int elapsed = 0;
        while (elapsed < WaitMs)
        {
            int step = Math.Min(StepDrainMs, WaitMs - elapsed);
            ClockSeam.Advance(step); elapsed += step;
            int stepPasses = 0;
            for (int pass = 0; pass < MaxDrainPasses; pass++)
            {
                var operatedAny = new[] { false };
                DrainOnce(operatedAny, elapsed);
                stepPasses = pass + 1;
                if (!operatedAny[0]) break;
                if (pass == MaxDrainPasses - 1) drainPassesHitCap = true;
            }
            drainPasses = Math.Max(drainPasses, stepPasses);
        }
        foreach (var (who, obj) in byWho)
        {
            foreach (var (m, prio) in obj.DrainRaw())
                pending.Add(new { who, ident = m.wIdent, delivery = m.DeliveryTime, now = HUtil32.GetTickCount() });
        }
        Console.Error.WriteLine($"DRAIN_PASSES {label} passes={drainPasses} hit_cap={drainPassesHitCap}");
        // **cover 的 draws 与 Restore 必须在排水循环之后**（`rng_mode=cover_drain` 的真实含义：
        // 排水阶段的抽取也进账本）；freeze 已在 `DoSpell` 紧后收尾（legacy）。
        if (rngMode != "freeze_at_dospell")
        {
            draws = rng.Draws.ToList();
            RngSeam.Restore(original);
        }
        long totalDraws = rng.TotalDraws;
        EnsureCoverAccounting(rngMode, draws.Count, totalDraws, "fireball/" + label);
        CheckBranchThreshold("fireball/" + label, target.AntiMagic, branchExpect);
        CheckAntiMagicBranch("fireball/" + label, draws, seed, branchExpect);
        string seedReasonText = branchExpect != null && draws.Count > 0
            ? BuildSeedReason(draws[0], seed, branchExpect)
            : null;
        int hpAfter = target.WAbil.HP;
        foreach (var (who, obj) in byWho)
        {
            receivers.Add(new ReceiverResult { Who = who, Rm = rmAcc[who], Sent = ParseFrames(recorder.Buffers, obj.ActorId) });
        }

        // —— 契约（M52.1）：同一 `FixtureContract` 七道门 + 第 8 道门（MaxExp）。
        var contract = new FixtureContract.FixtureInput
        {
            Label = "fireball/" + label,
            RngMode = rngMode,
            LoginOrder = loginOrder,
            Reachability = loginOrder ? "reachable" : "unverified",
            AntiMagic = target.AntiMagic,
            TargetLevel = target.Abil.Level,
            TargetHp = targetHpInput,
            TargetMaxHp = target.WAbil.MaxHP,
            Derived = FixtureContract.Recompute(OpenMir2.Enums.PlayerJob.Warrior, (byte)target.Abil.Level),
            Construction = new FixtureContract.Construction
            {
                Job = OpenMir2.Enums.PlayerJob.Warrior,
                Level = (byte)target.Abil.Level,
                Equipment = new List<string>(),
                Skills = new List<string> { "FireBall" },
            },
            ScenarioNote = loginOrder ? "" : "历史合成场景（AntiMagic=3/MaxHP=100；登录流程产不出，留档）",
            LegacyArtifact = rngMode == "freeze_at_dospell",
            LegacyReason = rngMode == "freeze_at_dospell" ? "rng ledger incomplete (drain-stage draws omitted)" : null,
            ActualMaxExp = loginOrder ? actualMaxExp : null,
        };
        FixtureContract.Validate(contract);
        return new FireballResult
        {
            Fixture = new
            {
                // recog 的归属对照：31 帧的 `recog` = 受击者 ActorId（`SM` 头 4 字节）。
                caster_actor_id = caster.ActorId,
                target_actor_id = target.ActorId,
                caster_mc = (int)caster.WAbil.MC,
                caster_sc = (int)caster.WAbil.SC,
                rng_mode = rngMode,
                login_order = loginOrder,
                reachability = contract.Reachability,
                anti_magic = (int)target.AntiMagic,
                target_level = (int)target.Abil.Level,
                target_hp = targetHpInput,
                target_max_hp = (int)target.WAbil.MaxHP,
                derived = new
                {
                    max_hp = contract.Derived.MaxHp,
                    max_mp = contract.Derived.MaxMp,
                    dc = contract.Derived.Dc,
                    ac = contract.Derived.Ac,
                    mac = contract.Derived.Mac,
                    max_exp = contract.Derived.MaxExp,
                },
                construction = new
                {
                    job = "Warrior",
                    level = (int)target.Abil.Level,
                    equipment = Array.Empty<string>(),
                    skills = new[] { "FireBall" },
                },
                scenario_note = contract.ScenarioNote,
                legacy_artifact = contract.LegacyArtifact,
                legacy_reason = contract.LegacyReason,
                seed_reason = seedReasonText,
                rng_total_draws = totalDraws,
                rng_recorded = draws.Count,
            },
            Label = label,
            Seed = seed,
            DrainPasses = drainPasses,
            DrainPassesHitCap = drainPassesHitCap,
            SpellReturn = ok,
            Receivers = receivers,
            Rng = draws,
            TargetHp = hpAfter,
            TargetHpBefore = hpBefore,
            NotDue = pending,
            CasterMp = caster.WAbil.MP,
        };
    }

/// <summary>
/// 把某接收者的通道缓冲解析成与旧 `Sent` 同形的账本项。
///
/// 缓冲 = 20B `ServerMessage` 头 + 载荷：
/// * `PackLength >= 0` ⇒ 载荷 = 12B `CommandMessage` + `tail`（GB2312 文本，EDCode 字符在 ASCII 区）；
/// * `PackLength < 0` ⇒ `SendSocket(string)` 动作短包，载荷就是该字符串（记 `ident=0` 便于显形）。
/// </summary>
private static List<object> ParseFrames(List<(int GateIdx, byte[] Data, long Due)> buffers, int gateIdx)
{
    var list = new List<object>();
    foreach (var (g, data, due) in buffers)
    {
        if (g != gateIdx) continue;
        int packLength = BitConverter.ToInt32(data, 16);
        if (packLength < 0)
        {
            string action = Encoding.GetEncoding("GB2312").GetString(data, ServerMessage.PacketSize, data.Length - ServerMessage.PacketSize);
            list.Add(new { ident = 0, recog = 0, param = 0, tag = 0, series = 0, due_ms_rel = due, tail = action });
            continue;
        }
        int off = ServerMessage.PacketSize;
        // 头 4 字节 = `Recog`（`CommandMessage` 的 `recog`；此前 ParseFrames 跳过未记）。
        int recog = BitConverter.ToInt32(data, off);
        ushort ident = BitConverter.ToUInt16(data, off + 4);
        ushort param = BitConverter.ToUInt16(data, off + 6);
        ushort tag = BitConverter.ToUInt16(data, off + 8);
        ushort series = BitConverter.ToUInt16(data, off + 10);
        string tail = data.Length > off + 12
            ? Encoding.GetEncoding("GB2312").GetString(data, off + 12, data.Length - off - 12)
            : "";
        list.Add(new { ident, recog, param, tag, series, due_ms_rel = due, tail });
    }
    return list;
}

/// <summary>
/// **MG-55（46 分身）**：与 [`ParseFrames`] **同一 wire 布局**的类型化解码（只给 Clone 块用）。
/// [ParseFrames] 保持匿名类型输出一字不变（既有 golden 重新生成时字段顺序/名字零漂移）。
/// </summary>
private static List<(ushort ident, int recog, ushort param, ushort tag, ushort series, long due, string tail)> DecodeFramesForGate(
    List<(int GateIdx, byte[] Data, long Due)> buffers, int gateIdx)
{
    var list = new List<(ushort, int, ushort, ushort, ushort, long, string)>();
    foreach (var (g, data, due) in buffers)
    {
        if (g != gateIdx) continue;
        int packLength = BitConverter.ToInt32(data, 16);
        if (packLength < 0)
        {
            string action = Encoding.GetEncoding("GB2312").GetString(data, ServerMessage.PacketSize, data.Length - ServerMessage.PacketSize);
            list.Add((0, 0, 0, 0, 0, due, action));
            continue;
        }
        int off = ServerMessage.PacketSize;
        int recog = BitConverter.ToInt32(data, off);
        ushort ident = BitConverter.ToUInt16(data, off + 4);
        ushort param = BitConverter.ToUInt16(data, off + 6);
        ushort tag = BitConverter.ToUInt16(data, off + 8);
        ushort series = BitConverter.ToUInt16(data, off + 10);
        string tail = data.Length > off + 12
            ? Encoding.GetEncoding("GB2312").GetString(data, off + 12, data.Length - off - 12)
            : "";
        list.Add((ident, recog, param, tag, series, due, tail));
    }
    return list;
}

/// **MG-36**：反射读实例字段（沿基类链；`MapEvent.ContinueTime` 是 **private readonly**）。
private static int ReadInstanceInt(object obj, string fieldName)
{
    for (var t = obj.GetType(); t != null; t = t.BaseType)
    {
        var f = t.GetField(fieldName, BindingFlags.Instance | BindingFlags.NonPublic | BindingFlags.Public);
        if (f != null)
        {
            return Convert.ToInt32(f.GetValue(obj));
        }
    }
    throw new FixtureContractViolation($"反射读 {obj.GetType().Name}.{fieldName} 失败（字段不存在）");
}

    // ======================================================================
    // 下一批 4 分支（11/45 雷电、2 治愈、13 火符、44 灭天火）：C# golden only
    // ======================================================================
    //
    // 与火球同口径：golden 几何（caster 100,100 / target 101,100 / obs_in 105,100 /
    // obs_out 140,100）、ActorId 段 1001../2001../3001..、`M2Share.NetChannel` 旁路记录
    // （三处 SendSocket 重载全捕获）、排水「抽空为止」（上界 8，命中进产物）。
    // 每 case 落 `Fixture`（魔法/施法者/目标输入），Rust 侧按它复现——不手抄数字。

    /// <summary>一支魔法的夹具输入（C# 侧真源；同时写进产物）。</summary>
    private sealed class MagicSpec
    {
        public string Key;
        public int MagicId;
        public string MagicName;
        public int Effect = 1;
        public int EffectType = 1;
        public int UserMagicLevel = 1;
        public int CasterLevel = 30;
        public int CasterMc = 15;
        public int CasterSc = 15;
        public int TargetLevel = 20;
        public int TargetMaxHp = 100;
        public int AntiMagic = 3;
        public bool ArmRingAmulet;
        /// <summary>
        /// 护身符 `StdItem.Shape` 旋钮（`ArmRingAmulet=true` 时生效；缺省 5 = `CheckAmulet(...,1,nType=1)`
        /// 那一组）。施毒 6/48 走 `nType=2`，要求 **`Shape <= 2`**（`MagicBase.cs:65-70`）。
        /// </summary>
        public int AmuletShape = 5;
        /// <summary>
        /// 目标 `AntiPoison` 旋钮（施毒骰 `Random(AntiPoison + 7) <= 6`，`MagicManager.cs:235/:914`）；
        /// 缺省 0（= ctor 缺省）⇒ 既有件零变化。
        /// </summary>
        public int TargetAntiPoison;
        /// <summary>
        /// 施法者 `HitPoint`（命中门 `Random(target.SpeedPoint) >= HitPoint`）；`-1` = 不设
        /// （保持 `BaseObject` ctor 缺省 5）。卡 §39：`0` = 门恒真 ⇒ 全 miss；`5` = `Random(0)=0 < 5` 必中。
        /// </summary>
        public int CasterHitPoint = -1;
        /// <summary>
        /// 施法者 `WAbil.DC` 原始字（Lo/Hi 字节，`HUtil32.MakeWord(lo,hi)`）；`-1` = 不设（保持缺省 0）。
        /// 卡 §39：`MagGroupDeDing` 的 `GetAttackPower(Lo(DC), Hi(DC)-Lo(DC))` 读它。
        /// </summary>
        public int CasterDC = -1;
        /// <summary>
        /// 施法者 `AttatckMode`（`OpenMir2.Enums.AttackMode`）；`-1` = 不设（保持 ctor 缺省 0 = `HAM_ALL`）。
        /// 卡 §39：置 `HAM_PEACE` ⇒ 只有 `Race >= Animal`（怪物）进伤害链，玩家 target 不取随机数。
        /// </summary>
        public int CasterAttackMode = -1;
        /// <summary>
        /// 卡 36：施法者也按**登录序**派生 `Abil`（只调 `RecalcLevelAbilitys()`，**不**调
        /// `RecalcAbilitys()`）——让 `AttPowerUp→RecalcAbilitys` 的输入 `Abil` 等级自洽，
        /// 同时不改夹具设的 `WAbil.SC`（`nTime` 的 `Lo(SC)*10` 输入）。缺省 false ⇒ 既有件零变化。
        /// </summary>
        public bool CasterLoginOrderAbil;
        /// <summary>卡 36：施法者 `CharStatusEx`（非 0 夹具使 `GetCharStatus()` 可证伪；缺省 0 ⇒ 既有件零变化）。</summary>
        public int CasterCharStatusEx;
        /// <summary>
        /// **MG-55（46 分身）**：施法者 `Dir` 输入（`-1` = 不设 ⇒ 既有 stem 零变化）。
        /// 生产读点 = `PlayCloneObject` ctor 的 `GetBackDir(playObject.Dir)`（`PlayCloneObject.cs:12`）；
        /// 注意生产 `ClientSpellXY` 的 default 支会在 `DoSpell` **之前**转向点击坐标
        /// （`PlayObject.Attack.cs:1269`）⇒ clone 侧真正用的是**转向后**的方向，见
        /// `PreSpellFaceTarget` 与 `Clone` 块里的 `caster_dir`。
        /// </summary>
        public int CasterDir = -1;
        /// <summary>**MG-55**：施法者 `Gender`（`PlayerGender`；`-1` = 不设）。克隆体 ctor 复制它（`PlayCloneObject.cs:14`）。</summary>
        public int CasterGender = -1;
        /// <summary>**MG-55**：施法者 `Hair`（`-1` = 不设）。克隆体 ctor 复制它（`PlayCloneObject.cs:15`）。</summary>
        public int CasterHair = -1;
        /// <summary>
        /// **MG-55**：施法前按生产 `default` 支把 `Dir` 转向 `(doX,doY)`（`PlayObject.Attack.cs:1269`）。
        /// 装置直调 `DoSpell` 不走命令层，不照做会让 `clone.dir` 与产品 `handle_spell_cmd` 路径不可比。
        /// 缺省 false ⇒ 既有 stem 零变化。
        /// </summary>
        public bool PreSpellFaceTarget;
        /// <summary>
        /// **MG-36（22 火墙）**：安全区夹具。true ⇒ 本支把 `env.Flag.SafeArea` 置 true 并临时打开
        /// `Config.DisableInSafeZoneFireCross`（oracle `MagicManager.cs:1066-1070` 的门），
        /// 施法后还原配置。缺省 false ⇒ 既有 stem 零变化。
        /// </summary>
        public bool FireCrossSafeZone;
        /// <summary>
        /// **MG-36（22 火墙）**：施法前预置事件的格。`new FireBurnEvent(..., Visible:true)` 的 ctor
        /// 只把事件挂进格子（`Envir.AddMapEvent`）、**不进** `EventMgr.Events` ⇒ 用来造
        /// `GetEvent(x,y) != null` 的去重支。缺省空 ⇒ 既有 stem 零变化。
        /// </summary>
        public (int X, int Y)[] PreEventCells = Array.Empty<(int, int)>();
        /// <summary>**收件人 HitSpeed 旋钮**（657 `series` 取收件人自己的 HitSpeed）；缺省 0 = 不动。</summary>
        public int HitSpeed;
        /// <summary>
        /// **帧序比较口径标记**（写进 `Fixture.frame_order_scope`）：空 = 严格按序（默认）。
        /// 目前唯一合法值 `multi_victim_same_tick`：多受害者同刻延迟受击时，产品是批量 flush、
        /// oracle 是逐 actor `Run()`（见 `docs/issues/openmir2-b4-multivictim-strike-order.md`），
        /// 比较器在该标记下把 `Sent` 改为**每接收者多重集**比较。**产品卡修复后必须删掉该标记。**
        /// </summary>
        public string FrameOrderScope = "";

        /// <summary>
        /// **非退化 MC 变体**（伤害面点亮）：>0 时对"后两个 damage case"（`s5`/`s13`/`hit`）覆写
        /// `caster.WAbil.MC`，其余 case 仍用 `CasterMc`（缺省 15 ⇒ `Hi<Lo` 钳制支对照）。
        /// 取值建议 `MakeWord(5,10)`（Lo=5,Hi=10）⇒ 抽 `Random(6)`，伤害随 seed 分叉。
        /// </summary>
        public int CasterMcNoClamp;
        /// <summary>
        /// **非退化 SC 变体**（治疗/火魅/DefenceUp 面点亮）：&gt;0 时对 `s5`/`s13`/`hit` 覆写
        /// `caster.WAbil.SC`，其余 case 仍用 `CasterSc`（缺省 15 ⇒ `Hi&lt;Lo` 钳制支对照）。
        /// 与 MC 变体同一逐 case 机制；oracle 里用 SC 的支（13/2/29/14/15）才该设它。
        /// </summary>
        public int CasterScNoClamp;
        /// <summary>`MagBigHealing` 的「多朋友」支要求施法者自身也 HP&lt;MaxHP（`0` = 不设）。</summary>
        public int CasterHp;
        public int CasterMaxHp;
        /// <summary>
        /// **步进排水**（`0` = 旧口径：单次 `Sleep(WaitMs)` 后一次排空）。&gt;0 时按该步长睡眠、
        /// 只在**到期时刻**处理消息（未到期按原优先级放回队列）——等价真机每 tick 的 `Run()`
        /// 到点处理。19 的 800ms `RM_TRANSPARENT`（→657）与 1000ms `RM_MAGIC_LVEXP`（→640）
        /// 同在 prio 255 队列里：单次 `Sleep` 后按 4 叉堆序会把 640 排到 657 前面（记录面伪影），
        /// 步进排水才与真机/Rust 分步交付同序。
        /// </summary>
        public int StepDrainMs;
        /// <summary>
        /// 排水前的等待（ms）。必须 > 该支最长自延迟，否则到点消息只进 `NotDue`
        /// （`now/delivery` 是墙钟、不可比拼），到点产生的帧会缺失。
        /// 默认 1200 覆盖 8/13/44 的自延迟；28 的 `RM_DOOPENHEALTH` = 1500 ⇒ 1600（卡 §25(a)）。
        /// 该值写进 golden 的 `Fixture.wait_ms`，Rust 侧推进量取同值。
        /// </summary>
        public int WaitMs = 1200;
        /// <summary>契约（M51）：是否走登录序重算（true = target/caster 同标准；默认 true）。</summary>
        public bool LoginOrder = true;
        /// <summary>契约（M51）：`cover_drain`（默认，现行行为）| `freeze_at_dospell`（仅 legacy）。</summary>
        public string RngMode = "cover_drain";
        /// <summary>契约（M51）：`reachable` | `unverified` | `unreachable`（无默认，逐 case 显式）。</summary>
        public string Reachability = "reachable";
        /// <summary>契约（M51）：`construction` 的 Job（缺省战士）。</summary>
        public OpenMir2.Enums.PlayerJob Job = OpenMir2.Enums.PlayerJob.Warrior;
        /// <summary>`construction.equipment`（本批一律空：无装备登录序）。</summary>
        public string[] Equipment = Array.Empty<string>();
        /// <summary>
        /// <summary>
        /// **预置 slave 数量**（17 数量上限支）：施法前向 caster `SlaveList` 塞这么多占位召唤物，
        /// 使 `SlaveList.Count >= nMaxMob` ⇒ `MakeSlave` 必返 null。
        /// </summary>
        public int PreSlaveCount;
        /// <summary>预置 slave 的 `ChrName`；缺省 `pre-slave`+索引保持 17 既有；SINSU 设为精确 `神兽`使 `RecallSlave` 能命中。</summary>
        public string PreSlaveName = "pre-slave";
        /// <summary>临时：为 SINSU 设 MapMgr 可用（env.MapName + ServerIndex），供 RecallSlave→SpaceMove；缺省 false ⇒ 其它 golden 零变化。</summary>
        public bool EnableMaps;
        /// <summary>
        /// **卡 magic-push-batch（MG-16/57）**：`MagPushArround` 遍历 `playObject.VisibleActors`
        /// （`MagicManager.cs:27`），而离线装置不跑 AOI（`SearchViewRange`/`UpdateVisibleGay` 都不调）
        /// ⇒ 表恒空、循环零次。置 true 时在建对象后对每只目标调 `caster.UpdateVisibleGay(...)`。
        /// **缺省 false ⇒ 其它 stem 的收件人集合零变化。**
        /// </summary>
        public bool MakeVisible;
        /// <summary>
        /// **MG-35（21 瞬息移动）**：`PlayObject.HomeMap`（`MapRandomMove→SpaceMove→FindMap(HomeMap)`）。
        /// 离线 ctor 不跑初始化 ⇒ 缺省 null（不设）；需要时置 "0" 并同时 `EnableMaps = true`。
        /// </summary>
        public string HomeMap;
        /// **非玩家对象夹具**（动物/怪物）：默认空 = 既有件零变化。每只的
        /// `(x,y,race,level,hp,life_attrib,master,target_cret)` 既进 `Fixture.monsters[]`，也决定世界里的实物。
        /// </summary>
        public MonsterSpec[] Monsters = Array.Empty<MonsterSpec>();
        /// <summary>
        /// **MG-44（32 圣言术）**：把 `DoSpell` 的目标从玩家换成 `Monsters` 的第 N 只（**1 基**；0 = 现状）。
        /// 圣言要求目标是 `AnimalObject` ⇒ 生成阶段即断言 `race>=Animal`、`life_attrib=LA_UNDEAD`、`!SuperMan`，
        /// 不满足直接 `FixtureContractViolation`（不等比较器给"帧数不等"这种无法归因的红）。
        /// 缺省 0 ⇒ 既有 stem 的 golden 字节不变。
        /// </summary>
        public int TargetMonster;
        /// <summary>
        /// **MG-38 后续批（2026-11-26）**：`TargetMonster` 的目标是否**不死**（缺省 true）。
        /// true ⇒ 装置按圣言三条件（`Race>=Animal` / `LifeAttrib==LA_UNDEAD` / `!SuperMan`）校验；
        /// false ⇒ **只放宽 `LifeAttrib==LA_UNDEAD`**（`Race>=Animal`、`!SuperMan` 仍是本支前置，
        /// 因为非不死分支（5/11/13/44/45 的 `Race>=Animal` 伤害面）同样要求动物目标）。
        /// 只在 `TargetMonster>0` 且为 false 时把 `Fixture.target_monster_undead=false` 落盘；
        /// 缺键 = true（旧 golden 零变化）。
        /// </summary>
        public bool TargetMonsterUndead = true;
        /// <summary>
        /// **B17 的可控落点（2026-11-26）**：该 case **以生产泵语义（含 t=0 泵）对齐**——
        /// 每 tick 只泵一次，`TryPeek` 队首；未到期 ⇒ `TryDequeue` + `Lowest` 重入队 +
        /// **结束本 actor 本次泵**；施法当刻（t=0）也泵一次（投 due=0 的 cast 帧并旋转等键队列）。
        /// 缺省 false ⇒ 既有 `DrainRaw`（先取全部、再逐条判到期）口径，**既有件零变化**。
        /// 由 `ExtraCaseSpec.FaithfulPump` 逐 case 临时置位；推广到更多 stem 属 B17 全量处置（另一张卡）。
        /// **根因口径（2026-11-26 实测更正）**：640 due 1000 vs 1100 不是"排水顺序"，而是**等键旋转次数**
        /// （装置 t=0 泵一次、驱动从 t=100 开始 ⇒ 少转一次）；驱动读 `Fixture.faithful_pump` 补 t=0 泵后对齐。
        /// 真实生产 tick ≈250–300ms 的 parity 属 B23。
        /// </summary>
        public bool FaithfulPump;
        /// <summary>
        /// **MG-48（2026-11-26）**：`OperateStruck` 怪物**按步结算**（而不是整个排水窗口结束后
        /// 一次性结算）。生产里怪物自己的 `Run()` 每 tick 泵自己的队列 ⇒ 延迟受击（`RM_MAGSTRUCK`
        /// 600ms）到点的**那一步**就结算、随即向玩家广播 `RM_STRUCK_MAG/CHANGENAMECOLOR`。
        /// 缺省 false ⇒ 既有「窗口结束后统一结算」口径，旧件零变化。
        /// </summary>
        public bool OperatePerStep;
        /// <summary>
        /// **MG-57（2026-11-26）**：第二次同形施法在步进到该 ms 时发生（`null` = 只施法一次）。
        /// 生产：客户端第二次 `CM_SPELL` 在后续 tick 的消息循环里被处理 ⇒ `DoSpell` 在**该步排水之前**
        /// 再跑一次（同一 `UserMagic`/坐标/目标）。落 `Fixture.second_cast_at_ms`，驱动按同一步镜像。
        /// </summary>
        public int? SecondCastAtMs;
        /// <summary>
        /// **MG-57 / B29**：本 stem 的 rig 用真 `0.map`（`Fixture.real_map`）；C# 装置本来就载真图，
        /// 本开关只影响 Rust rig 的地图基准。
        /// </summary>
        public bool RealMap;
        /// <summary>
        /// **MG-57 / ③**：本 stem 的 rig/装置用生产 AOI（`Fixture.production_aoi`）。
        /// </summary>
        public bool ProductionAoi;
        /// <summary>
        /// **MG-57 P2（2026-11-26）**：四个对象的坐标覆盖（`null` = 既有缺省 (100,100)/(101,100)/
        /// (105,100)/(140,100)）。用于方向臂 / 多目标 / 收件人范围边界臂；装置据此建对象并**记录实际值**。
        /// </summary>
        public int[] CasterXy;
        public int[] TargetXy;
        public int[] ObsInXy;
        public int[] ObsOutXy;
        /// <summary>
        /// **MG-57 P2**：断言受害者的 `SendRefMsg` 缓存**冷**（`VisibleHumanList` 空 ⇒ 首次推挤必走
        /// 首扫路径），并落 `Fixture.push_cache_cold=true`；用于把"首扫 vs 复用"两条规则的入口钉死。
        /// </summary>
        public bool PushCacheProbe;
        /// <summary>**MG-57 P2**：记录受害者术后 `Dir`（`TargetDirAfter`，仅该 case 落盘）。</summary>
        public bool ObserveDirAfter;
        /// <summary>**MG-12/40**：恢复 tick 的虚拟毫秒列表（见 `ExtraCaseSpec.HealTicksMs`）。</summary>
        public int[] HealTicksMs;
        /// <summary>
        /// **MG-12/40**：恢复 tick 臂的**施法者 `WAbil.Level`**（`null` = 不动；生产登录后至少 1 级，
        /// 装置离线对象 `WAbil.Level` 可能是未初始化的 0 ⇒ 逐 case 显式给）。
        /// </summary>
        public int? HealCasterWabilLevel;
        /// <summary>
        /// **MG-12/40**：恢复 tick 也跑**施法者**（29 的 `both` 支：caster/target 各自累计/各自排干）；
        /// 轨迹同时记 `hp_caster/inc_healing_caster`。缺省 false ⇒ 只跑 target。
        /// </summary>
        public bool HealTicksBoth;
        /// <summary>
        /// **MG-44（2026-11-26）**：允许 `TargetMonster` 的 `SuperMan` 前置被**显式测试**（负路径）。
        /// 缺省 false ⇒ 既有圣言三条件门槛不变；置 true 只在"有意验证 `:830` 负路径"的 stem。
        /// </summary>
        public bool TargetMonsterAllowSuperMan;
        /// <summary>
        /// **MG-48（2026-11-26）**：允许怪的 `Rm` 出现**同 ident 多条**（成功臂的两条
        /// `RM_DELAYMAGIC(8021)`：nPower/3 与 MSD）。缺省 false ⇒ 既有「同种 RM 恰 1 条」守卫不变。
        /// </summary>
        public bool AllowDuplicateMonsterRm;
        /// <summary>
        /// **MG-44（2026-11-26）**：术后状态观测列（见 `ExtraCaseSpec.ObserveState`）。
        /// 由 extra 循环临时置位；缺省 false ⇒ 既有件零变化。
        /// </summary>
        public bool ObserveState;
        /// <summary>
        /// **MG-44/48**：本支装载的 ServerConf 十键快照（`ServerConfFixture.LoadOrThrow`）。
        /// 非 null ⇒ golden `Fixture.server_conf` 落盘；缺省 null ⇒ 既有 stem 字节不变。
        /// 键序与 `ServerConfFixture.Keys` 一致（Rust `SERVER_CONF_KEYS` 同序）。
        /// </summary>
        public ServerConfFixture.Snapshot ServerConf;
        /// <summary>
        /// **三态计时器输入**（`DECHEALTH/DAMAGEARMOR/STONE` 的初始值）。`null` = 本支不涉及状态分支；
        /// **走到状态分支的用例必须显式给**（不靠缺省 0 静默通过）。非零初值取 ≥2 且互不相同，
        /// 否则"置 1"前后同值、观测不到。
        /// </summary>
        public int[] PoisonStates;
        /// <summary>`construction.skills`（本批写魔号名）。</summary>
        public string[] Skills = Array.Empty<string>();
        /// <summary>
        /// **共享尾（49/51/52/53/54）批专用**：`MagicInfo.TrainLv` / `MaxTrain[0]` 的夹具值。
        /// 缺省 0/0 = 既有行为（`CheckMagicLevelUp` 因 `TrainLv(0) > Level` 为假而不升级，
        /// 与既有 golden 一致）。本批取 `TrainLv=3, MaxTrain[0]=1000` ⇒ 未升级 ⇒ 640 走 **1000ms** 支。
        /// </summary>
        public int MagicTrainLv;
        public int MagicMaxTrain0;
        /// <summary>
        /// **非零值魔表对照臂（B16 Arm A）**：`MagicInfo` 五字段部署真值（`sql/mir2_data.sql` 生效行；
        /// `LoadMagicDB` 对重复 MagID 取最先读到的行）。`NonzeroArm=true` 时夹具落
        /// `magic_power/magic_max_power/magic_def_power/magic_def_max_power/magic_train_lv`（非零才写，
        /// 缺键=0）；旧 stem 全 0 且 `NonzeroArm=false` ⇒ 生成器与 golden 逐字节不变。
        /// </summary>
        public bool NonzeroArm;
        /// <summary>
        /// **MG-50（2026-11-26）**：护身符 `Dura`（`null` = 既有 600）。`CheckAmulet` 消耗
        /// `Round(Dura/100)` 次 ⇒ 该值决定"每目标独立扣一次"能持续到第几个目标（48 的多目标臂用）。
        /// 落 `Fixture.amulet_dura`（实际值）并断言。
        /// </summary>
        public int? AmuletDura;
        public int MagicPower;
        public int MagicMaxPower;
        public int MagicDefPower;
        public int MagicDefMaxPower;
        /// <summary>为 true ⇒ `EmitGolden` 的 case 里落 `Train` 块（本批专用；缺省零变化）。</summary>
        public bool SkillTail;
        /// <summary>
        /// **额外 case 列表（2026-11-26 泛化）**：在既有三支之外继续跑；每项自带 label / 同场怪物 /
        /// 目标怪物（1 基）/ 目标是否不死 / seed 口径。缺省空 ⇒ 既有件零变化。
        /// `BehaviorBatch` 与 `AntiMagicBatch` 共用同一实现（不再各自一套单字段）。
        /// </summary>
        public ExtraCaseSpec[] ExtraCases = Array.Empty<ExtraCaseSpec>();
        /// <summary>
        /// true ⇒ 本支是**升级变体**（MG-58/60/61/62/63 的 800ms 支）：令 `MaxTrain[nLevel] &lt;=
        /// TrainSkill 后的 TranPoint` ⇒ `CheckMagicLevelUp` 成功、`Level++`、640 走 **800ms**
        /// （`PlayObject.cs:5210-5216`）。与未升级三支同 seed，便于逐帧对照。
        /// </summary>
        public bool TailUpgrade;
        /// <summary>`scenario_note`（`login_order=false` 必填）。</summary>
        public string ScenarioNote = "";
        /// <summary>`unreachable` 时的依据（门 4）。</summary>
        public string UnreachableEvidence = "";
    }

    /// <summary>
    /// 29 群体治愈术：三案按行为分支取（不是 miss/edge/hit）——
    /// `heal`（目标 50/224！登录序 MaxHP=224）、`full`（224/224 ⇒ 无 8034）、
    /// `both`（施法者 50/100 + 目标 50/224 ⇒ **两个** proper friend 各一条 8034，覆盖 (d) 的
    /// 多朋友枚举序/条数）。`UserMagic.Level=1` ⇒ `AbilSeeHealGauge=false` ⇒ **1103 分支不覆盖**
    /// （卡面显式登记）。
    /// </summary>
    private static int BigHealingBatch(MagicSpec spec)
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        var heal = RunMagicCase(spec, "heal", 1, 1000, 50, false);
        var full = RunMagicCase(spec, "full", 5, 2000, 224, false);
        var bothSpec = new MagicSpec
        {
            Key = spec.Key,
            MagicId = spec.MagicId,
            MagicName = spec.MagicName,
            Effect = spec.Effect,
            EffectType = spec.EffectType,
            UserMagicLevel = spec.UserMagicLevel,
            CasterLevel = spec.CasterLevel,
            CasterMc = spec.CasterMc,
            CasterSc = spec.CasterSc,
            CasterScNoClamp = spec.CasterScNoClamp,
            TargetLevel = spec.TargetLevel,
            TargetMaxHp = spec.TargetMaxHp,
            AntiMagic = spec.AntiMagic,
            ArmRingAmulet = spec.ArmRingAmulet,
            WaitMs = spec.WaitMs,
            StepDrainMs = spec.StepDrainMs,
            CasterHp = 50,
            CasterMaxHp = 100,
        };
        var both = RunMagicCase(bothSpec, "both", 13, 3000, 50, false);
        // **MG-12/40**：29 的差异点——两目标（caster/target）**各自累计、各自排干**；两条 HP 轨迹。
        var tickSpec = new MagicSpec
        {
            Key = bothSpec.Key,
            MagicId = bothSpec.MagicId,
            MagicName = bothSpec.MagicName,
            Effect = bothSpec.Effect,
            EffectType = bothSpec.EffectType,
            UserMagicLevel = bothSpec.UserMagicLevel,
            CasterLevel = bothSpec.CasterLevel,
            CasterMc = bothSpec.CasterMc,
            CasterSc = bothSpec.CasterSc,
            CasterScNoClamp = bothSpec.CasterScNoClamp,
            TargetLevel = bothSpec.TargetLevel,
            TargetMaxHp = bothSpec.TargetMaxHp,
            AntiMagic = bothSpec.AntiMagic,
            ArmRingAmulet = bothSpec.ArmRingAmulet,
            WaitMs = bothSpec.WaitMs,
            StepDrainMs = bothSpec.StepDrainMs,
            CasterHp = 50,
            CasterMaxHp = 100,
            HealTicksMs = new[] { 1300, 1900 },
            HealTicksBoth = true,
            HealCasterWabilLevel = 1,
        };
        var tickBoth = RunMagicCase(tickSpec, "big_tick_two", 13, 3100, 50, false);
        return EmitGolden(spec, new[] { heal, full, both, tickBoth });
    }

    /// <summary>
    /// 28 心灵启示：门是 `Random(6) <= userMagic.Level + 3`（`MagicManager.cs:434`，**不是**
    /// AntiMagic 骰）。按同一物理意义取三支：miss = 首抽 5（>4）、edge = 首抽 4（==门）、
    /// hit = 首抽 3（<门）。`WaitMs=1600` 跨过 1500ms 的 `RM_DOOPENHEALTH` 自延迟（卡 §25(a)）。
    /// </summary>
    /// <summary>
    /// **MG-44（32 圣言术）**：目标等级 **43/44/45** 三档（部署 `MagTurnUndeadLevel=44` 的严格
    /// 小于口径：43 过 `:844` 等级门、44/45 不过门）。三档共用同一只不死怪（仅 `Level` 不同），
    /// 各一个 label ⇒ 同一份 golden 里可直接对照三档。
    ///
    /// 前置门（`:842 Random(2)+(casterLevel-1) > targetLevel`）由 `CasterLevel=60` 在两抽下都为真
    /// （0+59>45、1+59>45）⇒ 比较面只由等级门决定；`:847 Random(100) < …` 是**有条件下游随机**，
    /// 位置在等级门之后（只在等级门内才抽），所以 44/45 档的抽数天然少一次。
    /// </summary>
    private static int TurnUndeadBatch(MagicSpec spec)
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        var baseMonster = spec.Monsters[0];
        // lv43 必须真的走到即死（否则"43 过门、44/45 不过门"退化成只差一次抽数）：
        // 按 `Struck` 的 `Random(300)` → `:842 Random(2)` → `:847 Random(100)` 的**实际取数序**
        // 扫 seed。44/45 不进入 `:847`（等级门在算术前），seed 只需固定。
        int seedKill = FindSeedForTurnUndeadKill(43, spec.CasterLevel, spec.UserMagicLevel, 44);
        if (seedKill < 0)
        {
            throw new FixtureContractViolation("[turnundead] 找不到让 lv43 即死的 seed（200000 内）");
        }
        var cases = new[]
        {
            (Label: "lv43", Level: 43, Seed: seedKill, Base: 1000),
            (Label: "lv44", Level: 44, Seed: 5, Base: 2000),
            (Label: "lv45", Level: 45, Seed: 13, Base: 3000),
        };
        var results = new List<FireballResult>();
        for (int i = 0; i < cases.Length; i++)
        {
            // 只换 `Level`（其余输入逐字段照抄首只）——三档之间只有等级门变量不同。
            spec.Monsters = new[]
            {
                new MonsterSpec
                {
                    X = baseMonster.X,
                    Y = baseMonster.Y,
                    Race = baseMonster.Race,
                    Level = cases[i].Level,
                    Hp = baseMonster.Hp,
                    LifeAttrib = baseMonster.LifeAttrib,
                    SpeedPoint = baseMonster.SpeedPoint,
                    Death = baseMonster.Death,
                    AC = baseMonster.AC,
                    AntiMagic = baseMonster.AntiMagic,
                },
            };
            results.Add(RunMagicCase(spec, cases[i].Label, cases[i].Seed, cases[i].Base, spec.TargetMaxHp, false));
        }
        // **MG-44（2026-11-26）**：可选额外 case（`ExtraCases`）——miss 臂 / 两条负路径 / 多怪 /
        // 玩家目标 / run_away。每项临时替换 `Monsters/TargetMonster/TargetMonsterUndead/
        // CasterLevel/ObserveState`，做完立即还原；缺省空 ⇒ 既有三支零变化。
        foreach (var extra in spec.ExtraCases)
        {
            var savedMonsters = spec.Monsters;
            var savedTargetMonster = spec.TargetMonster;
            var savedTargetMonsterUndead = spec.TargetMonsterUndead;
            var savedCasterLevel = spec.CasterLevel;
            var savedObserveState = spec.ObserveState;
            spec.Monsters = extra.Monsters;
            spec.TargetMonster = extra.TargetMonster;
            spec.TargetMonsterUndead = extra.TargetMonsterUndead;
            if (extra.CasterLevelOverride.HasValue)
            {
                spec.CasterLevel = extra.CasterLevelOverride.Value;
            }
            spec.ObserveState = extra.ObserveState;
            results.Add(RunMagicCase(spec, extra.Label, extra.SeedOverride ?? 1, extra.IdBase, spec.TargetMaxHp, false));
            spec.Monsters = savedMonsters;
            spec.TargetMonster = savedTargetMonster;
            spec.TargetMonsterUndead = savedTargetMonsterUndead;
            spec.CasterLevel = savedCasterLevel;
            spec.ObserveState = savedObserveState;
        }
        return EmitGolden(spec, results.ToArray());
    }

    /// <summary>
    /// 找让 `MagTurnUndead` 走完即死臂的 seed：按 oracle 的**实际取数序**
    /// `Struck` 的 `Random(300)`（`Animal`）→ `:842 Random(2)` → `:844` 等级门 → `:847 Random(100)`。
    /// 只模拟取数，不跑整场（无 Sleep）。找不到返回 -1。
    /// </summary>
    private static int FindSeedForTurnUndeadKill(int targetLevel, int casterLevel, int magicLevel, int cfgLevel)
    {
        var rn = RandomNumber.GetInstance();
        var original = RngSeam.Capture();
        try
        {
            for (int seed = 1; seed <= 200000; seed++)
            {
                RngSeam.Install(seed);
                rn.Random(300); // Struck：Animal 的 MeatQuality 抽
                int d842 = rn.Random(2);
                if (d842 + (casterLevel - 1) > targetLevel && targetLevel < cfgLevel)
                {
                    int d847 = rn.Random(100);
                    int bound = (magicLevel << 3) - magicLevel + 15 + (casterLevel - targetLevel);
                    if (d847 < bound)
                    {
                        return seed;
                    }
                }
                RngSeam.Restore(original);
            }
        }
        finally
        {
            RngSeam.Restore(original);
        }
        return -1;
    }

    /// <summary>
    /// **MG-55（46 分身术）**：`MagMakeClone`（`MagicManager.cs:1337`）⇒ `new PlayCloneObject`
    /// （`PlayCloneObject.cs:5-23`）。判据面 = 克隆体静态字段 + 收件人 `SM_TURN(10)` /
    /// `SM_FEATURECHANGED(41)`（真身 `RM_TURN` handler 产物）+ 训练尾 640 的存在性。
    ///
    /// **前提（fail-closed，不满足 ⇒ rc=2）**：
    /// - `UserMagicLevel=0` 且 `TrainLv=3 > 0`（`CheckMagicLevelUp` 的下标是 `nLevel=0`）；
    /// - `MaxTrain[0] &gt; 4`（`TranPoint=0 + Random(3)+1 ≤ 4` ⇒ 必不升级 ⇒ 640 走 1000ms 支）；
    /// - `CasterDir/CasterGender/CasterHair` 必须显式给（不靠缺省 ⇒ feature 面不退化）；
    /// - `PreSpellFaceTarget=true`：生产 `ClientSpellXY` 的 default 支在 `DoSpell` 前转向点击坐标
    ///   （`PlayObject.Attack.cs:1269`）——不照做 `clone.dir` 与产品路径不同源。
    /// </summary>
    /// <summary>MG-34 七个分支（只用于生成期选 seed；与 `MagTamming` 的取数序逐字对应）。</summary>
    private enum TammingBranch
    {
        Other,
        SelfMaster,
        AdoptOk,
        LevelGateFail,
        ElseSend,
        OuterTrue,
        OuterFalse,
        Crazy,
    }

    private static TammingBranch SimulateTamming(
        int seed,
        int magicLevel,
        int casterLevel,
        bool masterIsCaster,
        int targetLevel,
        int maxHp,
        int cfgTargetLevel,
        int cfgLevel,
        int cfgCount,
        int hpRate,
        bool noTame,
        bool undead,
        int slaveCount)
    {
        var r = new Random(seed);
        if (r.Next(4 - magicLevel) != 0)
        {
            return r.Next(2) == 0 ? TammingBranch.OuterTrue : TammingBranch.OuterFalse;
        }
        if (masterIsCaster)
        {
            return TammingBranch.SelfMaster;
        }
        if (r.Next(2) != 0)
        {
            return TammingBranch.ElseSend;
        }
        if (targetLevel > casterLevel + 2)
        {
            return TammingBranch.LevelGateFail;
        }
        if (r.Next(3) != 0)
        {
            if (!undead && r.Next(20) == 0)
            {
                return TammingBranch.Crazy;
            }
            return TammingBranch.Other;
        }
        if (r.Next(casterLevel + 20 + magicLevel * 5) <= targetLevel + cfgTargetLevel)
        {
            if (!undead && r.Next(20) == 0)
            {
                return TammingBranch.Crazy;
            }
            return TammingBranch.Other;
        }
        if (!noTame && !undead && targetLevel < cfgLevel && slaveCount < cfgCount)
        {
            int n14 = maxHp / hpRate;
            n14 = n14 <= 2 ? 2 : n14 * 2;
            if (r.Next(n14) == 0)
            {
                return TammingBranch.AdoptOk;
            }
            return r.Next(14) == 0 ? TammingBranch.Other : TammingBranch.Other;
        }
        if (undead && r.Next(20) == 0)
        {
            return TammingBranch.Other;
        }
        return TammingBranch.Other;
    }

    private static int FindTammingSeed(
        TammingBranch want,
        int magicLevel,
        int casterLevel,
        bool masterIsCaster,
        int targetLevel,
        int maxHp,
        int cfgTargetLevel,
        int cfgLevel,
        int cfgCount,
        int hpRate,
        bool noTame,
        bool undead,
        int slaveCount)
    {
        for (int seed = 1; seed <= 200_000; seed++)
        {
            if (SimulateTamming(
                    seed, magicLevel, casterLevel, masterIsCaster, targetLevel, maxHp,
                    cfgTargetLevel, cfgLevel, cfgCount, hpRate, noTame, undead, slaveCount)
                == want)
            {
                return seed;
            }
        }
        throw new FixtureContractViolation($"[tamming] 200000 内找不到分支 {want} 的 seed");
    }

    /// <summary>
    /// **MG-34（20 诱惑之光 `MagTamming`）**：七支覆盖卡 §3.3 的分支表。
    /// 配置十键由 `ServerConfFixture.LoadOrThrow` 记录（部署 40/1/100/5）；配置不在 `World`，
    /// 驱动侧用 `Fixture.server_conf` 构造 `PetSlaveConfig`。
    /// **不建模/不比较**：`CanReAlive` 清除后效与 `MonGen.ActiveCount--` 的 Rust 侧落值（golden 记录）。
    /// </summary>
    private static int TammingBatch(MagicSpec spec)
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        var serverConf = ServerConfFixture.LoadOrThrow("tamming");
        int cfgTargetLevel = serverConf.Values["mag_tamming_target_level"];
        int cfgLevel = serverConf.Values["mag_tamming_level"];
        int cfgCount = serverConf.Values["mag_tamming_count"];
        int hpRate = serverConf.Values["mag_tamming_hp_rate"];
        const int casterLevel = 30;
        const int magicLevel = 0; // 训练前提：level=0 ⇒ nLevel=0、MaxTrain[0]=1000 不升级

        void SetMonster(int level, bool masterCaster, bool canReAlive, int monGenActive, bool undead)
        {
            spec.Monsters = new[]
            {
                new MonsterSpec
                {
                    Name = "golden-mon0",
                    X = 102,
                    Y = 100,
                    Race = 50,
                    Level = level,
                    Hp = 600,
                    LifeAttrib = undead ? 1 : 0,
                    SpeedPoint = 10,
                    Death = false,
                    AC = 0,
                    AntiMagic = 3,
                    NoTame = false,
                    Animal = false,
                    CanReAlive = canReAlive,
                    MonGenActive = monGenActive,
                    WalkSpeed = 2000,
                    NextHitTime = 3000,
                    MasterCaster = masterCaster,
                },
            };
        }

        int SeedFor(TammingBranch b, int level, bool masterCaster, bool undead)
        {
            return FindTammingSeed(
                b, magicLevel, casterLevel, masterCaster, level, 600,
                cfgTargetLevel, cfgLevel, cfgCount, hpRate, noTame: false, undead, slaveCount: 0);
        }

        var results = new FireballResult[7];
        spec.ServerConf = serverConf;
        spec.TargetMonster = 1;
        spec.CasterLevel = casterLevel;
        spec.UserMagicLevel = magicLevel;
        spec.MagicTrainLv = 3;
        spec.MagicMaxTrain0 = 1000;
        spec.SkillTail = true;
        spec.StepDrainMs = 100;

        SetMonster(20, true, false, -1, false);
        results[0] = RunMagicCase(spec, "self_master", SeedFor(TammingBranch.SelfMaster, 20, true, false), 1000, spec.TargetMaxHp, false);
        SetMonster(20, false, true, 1, false);
        results[1] = RunMagicCase(spec, "adopt_ok", SeedFor(TammingBranch.AdoptOk, 20, false, false), 2000, spec.TargetMaxHp, false);
        SetMonster(40, false, false, -1, false);
        results[2] = RunMagicCase(spec, "level_gate_fail", SeedFor(TammingBranch.LevelGateFail, 40, false, false), 3000, spec.TargetMaxHp, false);
        SetMonster(20, false, false, -1, false);
        results[3] = RunMagicCase(spec, "branch_else", SeedFor(TammingBranch.ElseSend, 20, false, false), 4000, spec.TargetMaxHp, false);
        SetMonster(20, false, false, -1, false);
        results[4] = RunMagicCase(spec, "outer_fail_true", SeedFor(TammingBranch.OuterTrue, 20, false, false), 5000, spec.TargetMaxHp, false);
        SetMonster(20, false, false, -1, false);
        results[5] = RunMagicCase(spec, "outer_fail_false", SeedFor(TammingBranch.OuterFalse, 20, false, false), 6000, spec.TargetMaxHp, false);
        SetMonster(20, false, false, -1, false);
        results[6] = RunMagicCase(spec, "crazy", SeedFor(TammingBranch.Crazy, 20, false, false), 7000, spec.TargetMaxHp, false);
        foreach (var r in results)
        {
            if (r.DrainPassesHitCap)
            {
                Console.Error.WriteLine(
                    $"TAMMING_DRAIN_CAP {spec.Key}/{r.Label} passes={r.DrainPasses}（账本可能不完整 ⇒ 拒绝落盘）");
                return 2;
            }
        }
        return EmitGolden(spec, results);
    }

    /// <summary>
    /// **MG-36（22 火墙 `MagMakeFireCross`）**：三支——
    /// `base`（五格全登记）、`pre_center`（施法前预置中心格事件 ⇒ `GetEvent` 去重，4 条且缺中心）、
    /// `safe_zone`（`DisableInSafeZoneFireCross` + `env.Flag.SafeArea` ⇒ 0 条 + 红字，且无 640）。
    /// 逐 tick 的 3000ms 伤害不在本卡（事件进程 `EventProcessor` 不启动）。
    /// </summary>
    private static int FireCrossBatch(MagicSpec spec)
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        var results = new FireballResult[3];
        spec.PreEventCells = Array.Empty<(int, int)>();
        spec.FireCrossSafeZone = false;
        results[0] = RunMagicCase(spec, "base", 22, 1000, spec.TargetMaxHp, false);
        spec.PreEventCells = new[] { (101, 100) };
        results[1] = RunMagicCase(spec, "pre_center", 23, 2000, spec.TargetMaxHp, false);
        spec.PreEventCells = Array.Empty<(int, int)>();
        spec.FireCrossSafeZone = true;
        results[2] = RunMagicCase(spec, "safe_zone", 24, 3000, spec.TargetMaxHp, false);
        spec.FireCrossSafeZone = false;
        foreach (var r in results)
        {
            if (r.DrainPassesHitCap)
            {
                Console.Error.WriteLine(
                    $"FIRECROSS_DRAIN_CAP {spec.Key}/{r.Label} passes={r.DrainPasses}（账本可能不完整 ⇒ 拒绝落盘）");
                return 2;
            }
        }
        return EmitGolden(spec, results);
    }

    private static int CloneBatch(MagicSpec spec)
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        if (spec.UserMagicLevel >= 3 || spec.MagicTrainLv <= spec.UserMagicLevel || spec.MagicMaxTrain0 <= 4)
        {
            throw new FixtureContractViolation(
                $"[{spec.Key}] 训练前提不成立：level={spec.UserMagicLevel} train_lv={spec.MagicTrainLv} "
                + $"max_train0={spec.MagicMaxTrain0}（要求 level<3、train_lv>level、max_train0>4）");
        }
        if (spec.CasterDir < 0 || spec.CasterGender < 0 || spec.CasterHair < 0)
        {
            throw new FixtureContractViolation(
                $"[{spec.Key}] CasterDir/CasterGender/CasterHair 必须显式给出（当前 {spec.CasterDir}/{spec.CasterGender}/{spec.CasterHair}）");
        }
        if (!spec.PreSpellFaceTarget)
        {
            throw new FixtureContractViolation(
                $"[{spec.Key}] PreSpellFaceTarget 必须为 true（生产 default 支在 DoSpell 前转向点击坐标）");
        }
        // 单支：`label=clone`、seed=46、ActorId 段 1000（沿用其它 magic batch 的分段口径）。
        var result = RunMagicCase(spec, "clone", 46, 1000, spec.TargetMaxHp, false);
        if (result.DrainPassesHitCap)
        {
            Console.Error.WriteLine(
                $"CLONE_DRAIN_CAP {spec.Key} passes={result.DrainPasses}（账本可能不完整 ⇒ 拒绝落盘）");
            return 2;
        }
        return EmitGolden(spec, new[] { result });
    }

    private static int ShowHpBatch(MagicSpec spec)
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        int seedMiss = FindSeedForFirstDraw(6, 5);
        int seedEdge = FindSeedForFirstDraw(6, 4);
        int seedHit = FindSeedForFirstDraw(6, 3);
        if (seedMiss < 0 || seedEdge < 0 || seedHit < 0)
        {
            Console.Error.WriteLine("SEED_SCAN_FAILED");
            return 1;
        }
        spec.WaitMs = 1600;
        var cases = new[]
        {
            RunMagicCase(spec, "miss", seedMiss, 1000, spec.TargetMaxHp, false),
            RunMagicCase(spec, "edge", seedEdge, 2000, spec.TargetMaxHp, false),
            RunMagicCase(spec, "hit", seedHit, 3000, spec.TargetMaxHp, false),
        };
        return EmitGolden(spec, cases);
    }

    /// <summary>有 AntiMagic 骰的支（11/45/13/44）：scan 三 seed ⇒ miss/edge/hit。</summary>
    private static int AntiMagicBatch(MagicSpec spec)
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        var cases = RunAntiMagicTriplet(spec);
        if (cases == null)
        {
            Console.Error.WriteLine("SEED_SCAN_FAILED");
            return 1;
        }
        return EmitGolden(spec, cases);
    }

    /// <summary>
    /// 有 AntiMagic 骰的支（11/45/13/44/火球系）三支：门槛取**生效 AntiMagic**（登录序后 = 1，
    /// 见 `LoginOrderAntiMagic`），逐支带 `AntiMagicBranchExpect` ⇒ 生成期断言首抽落段，
    /// 退化（三支同一支）会让生成失败而不是静默产出重复用例。
    /// </summary>
    private static FireballResult[] RunAntiMagicTriplet(MagicSpec spec)
    {
        int threshold = LoginOrderAntiMagic;
        int seedMiss = FindSeedForFirstDraw(10, threshold - 1);
        int seedEdge = FindSeedForFirstDraw(10, threshold);
        int seedHit = FindSeedForFirstDraw(10, threshold + 2);
        if (seedMiss < 0 || seedEdge < 0 || seedHit < 0)
        {
            return null;
        }
        var cases = new List<FireballResult>
        {
            RunMagicCase(spec, "miss", seedMiss, 1000, spec.TargetMaxHp, false, new AntiMagicBranchExpect { Kind = "miss", Threshold = threshold }),
            RunMagicCase(spec, "edge", seedEdge, 2000, spec.TargetMaxHp, false, new AntiMagicBranchExpect { Kind = "edge", Threshold = threshold }),
            RunMagicCase(spec, "hit", seedHit, 3000, spec.TargetMaxHp, false, new AntiMagicBranchExpect { Kind = "hit", Threshold = threshold }),
        };
        // **2026-11-26 泛化**：`ExtraCases` 每项追加一支（缺省空 ⇒ 既有三支零变化）。
        // `UseHitSeed` ⇒ 用本批 hit 种子（首抽 > 门槛，走到伤害/训练尾面），否则 seed=1；
        // 每项临时替换 `Monsters/TargetMonster/TargetMonsterUndead`，做完立即还原。
        foreach (var extra in spec.ExtraCases)
        {
            var savedMonsters = spec.Monsters;
            var savedTargetMonster = spec.TargetMonster;
            var savedTargetMonsterUndead = spec.TargetMonsterUndead;
            var savedFaithfulPump = spec.FaithfulPump;
            var savedOperatePerStep = spec.OperatePerStep;
            var savedUserMagicLevel = spec.UserMagicLevel;
            var savedCasterLevel = spec.CasterLevel;
            var savedObserveState = spec.ObserveState;
            spec.Monsters = extra.Monsters;
            spec.TargetMonster = extra.TargetMonster;
            spec.TargetMonsterUndead = extra.TargetMonsterUndead;
            spec.FaithfulPump = extra.FaithfulPump;
            spec.OperatePerStep = extra.OperatePerStep;
            if (extra.UserMagicLevelOverride.HasValue)
            {
                spec.UserMagicLevel = extra.UserMagicLevelOverride.Value;
            }
            if (extra.CasterLevelOverride.HasValue)
            {
                spec.CasterLevel = extra.CasterLevelOverride.Value;
            }
            spec.ObserveState = extra.ObserveState;
            var extraSeed = extra.SeedOverride ?? (extra.UseHitSeed ? seedHit : 1);
            var extraExpect = extra.UseHitSeed
                ? new AntiMagicBranchExpect { Kind = "hit", Threshold = threshold }
                : null;
            cases.Add(RunMagicCase(spec, extra.Label, extraSeed, extra.IdBase, spec.TargetMaxHp, false, extraExpect));
            spec.Monsters = savedMonsters;
            spec.TargetMonster = savedTargetMonster;
            spec.TargetMonsterUndead = savedTargetMonsterUndead;
            spec.FaithfulPump = savedFaithfulPump;
            spec.OperatePerStep = savedOperatePerStep;
            spec.UserMagicLevel = savedUserMagicLevel;
            spec.CasterLevel = savedCasterLevel;
            spec.ObserveState = savedObserveState;
        }
        return cases.ToArray();
    }

    /// <summary>
    /// 2 治愈：**没有 AntiMagic 骰**，三支按行为分支取（不是 miss/edge/hit）——
    /// `heal`（目标受伤 ⇒ RM_MAGHELING）、`full`（目标满血 ⇒ 无 RM_MAGHELING）、
    /// `self`（targetObject=null ⇒ 自疗，坐标取施法者）。seed 固定 1/5/13。
    /// </summary>
    /// **MG-12/40 `tick_cap`（2026-11-26）**：封顶本体的**单次施法**臂——`nPower≈801` ⇒
    /// `(IncHealing + n) < 300` 为假 ⇒ else 支直接置 300。判据：轨迹首行 `inc_healing=300`
    /// （未封顶会 ~801）、每 tick `TargetHp +5`（不是 +800）。三抽（生成期核对）：
    /// `R(MaxPower-Power)=R(1)`、`R(DefMaxPower-DefPower)=R(1)`（`Random(0)` 照抽，§27）、
    /// `GetAttackPower` 的 `R((Hi-Lo)*2+2)=R(2)`。
    private static int NonzeroHealingBatch()
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        var spec = new MagicSpec
        {
            Key = "healing-nonzero",
            MagicId = 2,
            MagicName = "Healing",
            NonzeroArm = true,
            MagicPower = 400,
            MagicMaxPower = 401,
            MagicDefPower = 1,
            MagicDefMaxPower = 2,
            MagicTrainLv = 0,
            UserMagicLevel = 1,
            CasterSc = 0,
            CasterMc = 0,
            StepDrainMs = 100,
            HealTicksMs = new[] { 1300, 1600, 1900, 2300 },
        };
        var cap = RunMagicCase(spec, "cap", 1, 1000, 50, false);
        return EmitGolden(spec, new[] { cap });
    }

    private static int HealingBatch()
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        var spec = new MagicSpec { Key = "healing", MagicId = 2, MagicName = "Healing", CasterScNoClamp = HUtil32.MakeWord(5, 10), StepDrainMs = 100 };
        // `full` = 目标**整血**（`DoSpell` 的门是 `targetObject.WAbil.HP < targetObject.WAbil.MaxHP`，
        // 见 `MagicConst.SKILL_HEALLING` 支）：输入 HP 必须**由等级派生**，不能写字面量——改 `TargetLevel` 时
        // 字面量会让「满血」静默变成「未满血」，治疗分支又悄悄回来（12 号 2026-11-20 的回归）。
        int fullHp = FixtureContract.Recompute(spec.Job, (byte)spec.TargetLevel).MaxHp;
        var cases = new List<FireballResult>
        {
            RunMagicCase(spec, "heal", 1, 1000, 50, false),
            RunMagicCase(spec, "full", 5, 2000, fullHp, false),
            RunMagicCase(spec, "self", 13, 3000, 50, true),
        };
        // 生成期自检（在 `EmitGolden` 之前 ⇒ 失败即 rc=2、不落盘）。门槛由 **spec 等级现算**，
        // 不复用上面的 `fullHp`——否则「传什么就比什么」，门恒真（负例证明见卡 §25）。
        EnsureFullHpBranch("healing/full", cases[1].Fixture, spec);
        // **MG-12/40（2026-11-26）**：恢复 tick 臂——判据 = `TargetHp`（真加血，每次排干 +`PerHealing=5`）。
        // tick 列表是**相对施法的虚拟毫秒**（排水窗口 = `StepDrainMs` 到 `WaitMs`，故从 1300 起）；
        // `tick_gate_edge_*` 用两个 Level 钉 `dw = 600 - min(400, Level*10)`；`tick_lag_rearm` 钉
        // `IncHealthSpellTick = now + min(200, …)`。`tick_hp_clamp` 同时证明 `HP==MaxHP ⇒ IncHealing=0`
        // （222+15 末位 tick 后 inc 必须为 0，不是剩 10）。
        var tickCases = new List<FireballResult>();
        void TickCase(string label, int seed, int idBase, int hp, int level, int[] ticks)
        {
            var savedTicks = spec.HealTicksMs;
            var savedLevel = spec.TargetLevel;
            spec.HealTicksMs = ticks;
            spec.TargetLevel = level;
            tickCases.Add(RunMagicCase(spec, label, seed, idBase, hp, false));
            spec.HealTicksMs = savedTicks;
            spec.TargetLevel = savedLevel;
        }
        int hp1 = FixtureContract.Recompute(spec.Job, 1).MaxHp - 10;
        TickCase("tick_hp_3", 1, 6100, 50, 20, new[] { 1300, 1600, 1900, 2000, 2300 });
        TickCase("tick_hp_clamp", 1, 6200, 222, 20, new[] { 1400 });
        TickCase("tick_gate_edge_20", 1, 6300, 50, 20, new[] { 1300, 1600, 1899, 1900 });
        TickCase("tick_gate_edge_1", 1, 6400, hp1, 1, new[] { 1300, 1899, 1900, 2090 });
        TickCase("tick_lag_rearm", 1, 6500, 50, 20, new[] { 1300, 1600, 1900 });
        cases.AddRange(tickCases);
        return EmitGolden(spec, cases.ToArray());
    }

    /// <summary>
    /// `full` 支必须**整血**构造：`Fixture.target_hp` 要等于该 spec 等级派生的 `MaxHP`（不是字面量）。
    /// 不等 ⇒ `FixtureContractViolation`（顶层打 `RED FIXTURE_CONTRACT`、rc=2），且调用点在
    /// `EmitGolden` 之前 ⇒ 不落盘。防的失效模式：改了 `TargetLevel` 或手改 HP，让「满血」静默变成
    /// 「未满血」、治疗分支悄悄回来（12 号 2026-11-20 的回归）。
    /// </summary>
    private static void EnsureFullHpBranch(string where, object fixture, MagicSpec spec)
    {
        int derivedMaxHp = FixtureContract.Recompute(spec.Job, (byte)spec.TargetLevel).MaxHp;
        int hp = (int)fixture.GetType().GetProperty("target_hp").GetValue(fixture);
        int maxHp = (int)fixture.GetType().GetProperty("target_max_hp").GetValue(fixture);
        if (hp != derivedMaxHp)
        {
            throw new FixtureContractViolation(
                $"[{where}] full 支必须整血构造：Fixture.target_hp={hp} != derived.max_hp={derivedMaxHp}"
                + "（Fixture.target_max_hp=" + maxHp + "；输入 HP 必须由等级派生、不得写字面量，"
                + "否则「满血不治疗」分支会静默消失）");
        }
    }

    /// <summary>
    /// **无 AntiMagic 骰**的支（14/15/31）：按注释的**同一行为**取三 seed（1/5/13）——
    /// 不存在 miss/edge/hit 语义（不构造假骰）。标签 = `s1` / `s5` / `s13`。
    /// </summary>
    private static int BehaviorBatch(MagicSpec spec)
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        var cases = new List<FireballResult>
        {
            RunMagicCase(spec, "s1", 1, 1000, spec.TargetMaxHp, false),
            RunMagicCase(spec, "s5", 5, 2000, spec.TargetMaxHp, false),
            RunMagicCase(spec, "s13", 13, 3000, spec.TargetMaxHp, false),
        };
        // **2026-11-26 泛化**：`ExtraCases` 每项追加一支（缺省空 ⇒ 既有三支零变化）。
        // 本批无 AntiMagic 骰 ⇒ seed 固定 1、ActorId 段尾 4100（便于与 s1 逐帧对照）。
        foreach (var extra in spec.ExtraCases)
        {
            var savedMonsters = spec.Monsters;
            var savedTargetMonster = spec.TargetMonster;
            var savedTargetMonsterUndead = spec.TargetMonsterUndead;
            var savedCasterLevel = spec.CasterLevel;
            var savedObserveState = spec.ObserveState;
            var savedFaithfulPump = spec.FaithfulPump;
            var savedOperatePerStep = spec.OperatePerStep;
            var savedSecondCastAtMs = spec.SecondCastAtMs;
            var savedRealMap = spec.RealMap;
            var savedProductionAoi = spec.ProductionAoi;
            var savedCasterXy = spec.CasterXy;
            var savedTargetXy = spec.TargetXy;
            var savedObsInXy = spec.ObsInXy;
            var savedObsOutXy = spec.ObsOutXy;
            var savedPushCacheProbe = spec.PushCacheProbe;
            var savedStepDrainMs = spec.StepDrainMs;
            var savedUserMagicLevel = spec.UserMagicLevel;
            var savedObserveDirAfter = spec.ObserveDirAfter;
            var savedHealTicksMs = spec.HealTicksMs;
            var savedTargetLevel = spec.TargetLevel;
            var savedUserMagicLevel48 = spec.UserMagicLevel;
            var savedAmuletDura = spec.AmuletDura;
            spec.Monsters = extra.Monsters;
            spec.TargetMonster = extra.TargetMonster;
            spec.TargetMonsterUndead = extra.TargetMonsterUndead;
            if (extra.CasterLevelOverride.HasValue)
            {
                spec.CasterLevel = extra.CasterLevelOverride.Value;
            }
            spec.ObserveState = extra.ObserveState;
            // **MG-48（2026-11-26）**：B17 生产泵语义与「怪按步结算」必须逐 case 可选
            // （既有三支零变化）。
            spec.FaithfulPump = extra.FaithfulPump;
            spec.OperatePerStep = extra.OperatePerStep;
            spec.SecondCastAtMs = extra.SecondCastAtMs;
            spec.RealMap = extra.RealMap;
            spec.ProductionAoi = extra.ProductionAoi;
            spec.CasterXy = extra.CasterXy;
            spec.TargetXy = extra.TargetXy;
            spec.ObsInXy = extra.ObsInXy;
            spec.ObsOutXy = extra.ObsOutXy;
            spec.PushCacheProbe = extra.PushCacheProbe;
            spec.ObserveDirAfter = extra.ObserveDirAfter;
            spec.HealTicksMs = extra.HealTicksMs;
            if (extra.TargetLevelOverride.HasValue)
            {
                spec.TargetLevel = extra.TargetLevelOverride.Value;
            }
            // **MG-50（2026-11-26）**：48 的范围门读 `userMagic.Level`（`MAX(1, Level)` 方框）
            // ⇒ 逐 case 覆盖（`null` = stem 值；既有三支零变化）。
            if (extra.UserMagicLevelOverride.HasValue)
            {
                spec.UserMagicLevel = extra.UserMagicLevelOverride.Value;
            }
            spec.AmuletDura = extra.AmuletDura;
            if (extra.StepDrainMsOverride.HasValue)
            {
                spec.StepDrainMs = extra.StepDrainMsOverride.Value;
            }
            if (extra.UserMagicLevelOverride.HasValue)
            {
                spec.UserMagicLevel = extra.UserMagicLevelOverride.Value;
            }
            cases.Add(RunMagicCase(spec, extra.Label, extra.SeedOverride ?? 1, extra.IdBase, extra.TargetHp ?? spec.TargetMaxHp, false));
            spec.Monsters = savedMonsters;
            spec.TargetMonster = savedTargetMonster;
            spec.TargetMonsterUndead = savedTargetMonsterUndead;
            spec.CasterLevel = savedCasterLevel;
            spec.ObserveState = savedObserveState;
            spec.FaithfulPump = savedFaithfulPump;
            spec.OperatePerStep = savedOperatePerStep;
            spec.SecondCastAtMs = savedSecondCastAtMs;
            spec.RealMap = savedRealMap;
            spec.ProductionAoi = savedProductionAoi;
            spec.CasterXy = savedCasterXy;
            spec.TargetXy = savedTargetXy;
            spec.ObsInXy = savedObsInXy;
            spec.ObsOutXy = savedObsOutXy;
            spec.PushCacheProbe = savedPushCacheProbe;
            spec.StepDrainMs = savedStepDrainMs;
            spec.UserMagicLevel = savedUserMagicLevel;
            spec.ObserveDirAfter = savedObserveDirAfter;
            spec.HealTicksMs = savedHealTicksMs;
            spec.TargetLevel = savedTargetLevel;
            spec.UserMagicLevel = savedUserMagicLevel48;
            spec.AmuletDura = savedAmuletDura;
        }
        return EmitGolden(spec, cases.ToArray());
    }

    /// <summary>把实跑状态装进契约输入（平铺值取 target 的实时字段）。</summary>
    private static FixtureContract.FixtureInput BuildContractInput(MagicSpec spec, RecordingPlayObject target)
    {
        return new FixtureContract.FixtureInput
        {
            Label = spec.Key,
            RngMode = spec.RngMode,
            LoginOrder = spec.LoginOrder,
            Reachability = spec.Reachability,
            AntiMagic = target.AntiMagic,
            TargetLevel = target.Abil.Level,
            TargetHp = target.WAbil.HP,
            TargetMaxHp = target.WAbil.MaxHP,
            Derived = FixtureContract.Recompute(spec.Job, (byte)spec.TargetLevel),
            Construction = new FixtureContract.Construction
            {
                Job = spec.Job,
                Level = (byte)spec.TargetLevel,
                Equipment = spec.Equipment.ToList(),
                Skills = spec.Skills.Length == 0 ? new List<string> { spec.MagicName } : spec.Skills.ToList(),
            },
            ScenarioNote = spec.ScenarioNote,
            UnreachableEvidence = spec.UnreachableEvidence,
            LegacyArtifact = spec.RngMode == "freeze_at_dospell",
            LegacyReason = spec.RngMode == "freeze_at_dospell" ? "rng ledger incomplete (drain-stage draws omitted)" : null,
        };
    }

    /// <summary>
    /// **MC 变体判定**（取数序开关）：`RunMagicCase` 里的 `useMcVariant` 是**按 label 名**判的；
    /// 抽出共享函数供自检与扫描器使用。非缺省臂必须断言 `false`——`Next(7)` 对约 1/7 的种子
    /// 也会返回 0，只靠"记录值 `Rng[2]==0`"是概率牙（卡 §2.6 三层）。
    /// </summary>
    private static bool IsMcVariantLabel(string label) =>
        label == "s5" || label == "s13" || label == "hit";

    /// <summary>非缺省臂的三个翻转目标（互斥：见卡 §2.3）。</summary>
    private enum NonDefaultArm { Hit, RandRateFlip, SucessFlip }

    /// <summary>
    /// 从 `AppContext.BaseDirectory` 向上找 `<repo>/mir2-rs/tests/parity/fixtures/serverconf/nondefault`。
    /// 找不到即契约违规（不静默回落部署目录——那会生成一支假绿）。
    /// </summary>
    private static string FindNonDefaultConfDir()
    {
        var dir = new DirectoryInfo(AppContext.BaseDirectory);
        for (int i = 0; i < 10 && dir != null; i++)
        {
            var cand = Path.Combine(dir.FullName, "mir2-rs", "tests", "parity", "fixtures", "serverconf", "nondefault");
            if (File.Exists(Path.Combine(cand, "Server.conf"))) return cand;
            dir = dir.Parent;
        }
        throw new FixtureContractViolation("找不到非缺省夹具目录 tests/parity/fixtures/serverconf/nondefault");
    }

    /// <summary>**MG-48**：按变体名找 `tests/parity/fixtures/serverconf/&lt;name&gt;` 目录。</summary>
    private static string FindConfVariantDir(string name)
    {
        var dir = new DirectoryInfo(AppContext.BaseDirectory);
        while (dir != null)
        {
            var cand = Path.Combine(dir.FullName, "mir2-rs", "tests", "parity", "fixtures", "serverconf", name);
            if (Directory.Exists(cand))
            {
                return cand;
            }
            dir = dir.Parent;
        }
        throw new FixtureContractViolation($"找不到配置变体目录 tests/parity/fixtures/serverconf/{name}");
    }

    /// <summary>按 oracle 实际取数序取"吃缺省实现"的两道门抽（门①=100、门②=21）。</summary>
    private static (int C100, int D21) DefaultGateDraws(int seed)
    {
        var rn = RandomNumber.GetInstance();
        var original = RngSeam.Capture();
        try
        {
            RngSeam.Install(seed);
            rn.Random(0); rn.Random(0); rn.Random(1); rn.Random(10); // MPow / GetPower / GetAttackPower ×2
            rn.Random(10); rn.Random(2);                              // AntiMagic 门 / Random(2)
            return (rn.Random(100), rn.Random(21));
        }
        finally
        {
            RngSeam.Restore(original);
        }
    }

    /// <summary>
    /// 扫 seed：判**两支合取**（本支 + 吃缺省），不是只看本支。取数序与
    /// `DefaultGateDraws`/装置实测一致（卡 §2.1）。找不到返回 -1。
    /// </summary>
    private static int FindSeedForNonDefaultArm(NonDefaultArm arm)
    {
        var rn = RandomNumber.GetInstance();
        var original = RngSeam.Capture();
        try
        {
            for (int seed = 1; seed <= 200000; seed++)
            {
                RngSeam.Install(seed);
                rn.Random(0); rn.Random(0); rn.Random(1); rn.Random(10);
                int a = rn.Random(10);
                rn.Random(2);
                int c50 = rn.Random(50);
                int d90 = rn.Random(90);
                RngSeam.Restore(original);
                if (a < 3) continue;
                var (c100, d21) = DefaultGateDraws(seed);
                bool armG1 = c50 < 40;
                bool armG2 = d90 < 6;
                bool ok = arm switch
                {
                    // Hit 支要求**两支都成功**（缺省也过两道门）⇒ 差异只来自 STONE 时长/模数，
                    // 不来自"谁进了成功臂"（后者由另两支覆盖）。
                    NonDefaultArm.Hit => armG1 && armG2 && c100 < 32 && d21 < 6,
                    NonDefaultArm.RandRateFlip => armG1 && armG2 && c100 >= 32,
                    NonDefaultArm.SucessFlip => armG1 && c100 < 32 && !armG2 && d21 < 6,
                    _ => false,
                };
                if (ok) return seed;
            }
        }
        finally
        {
            RngSeam.Restore(original);
        }
        return -1;
    }

    /// <summary>
    /// 记录值自检（卡 §2.6 第 2 层）+ 翻转谓词的**事后重算**（不是只在扫描期成立）：
    /// `Rng[4]`=AntiMagic、`Rng[5]`=Random(2)、`Rng[6]`=门①、`Rng[7]`=门②（0 基）。
    /// </summary>
    private static void AssertNonDefaultFlip(string label, NonDefaultArm arm, int seed, IReadOnlyList<long> rng)
    {
        if (rng.Count < 7)
        {
            throw new FixtureContractViolation($"[{label}] Rng 少于 7 抽（实际 {rng.Count}）");
        }
        if (rng[2] != 0)
        {
            throw new FixtureContractViolation(
                $"[{label}] Rng[2]={rng[2]} 非 0 ⇒ label 被判成 MC 变体（第 3 抽应为 Random(1)）");
        }
        if (rng[4] < 3)
        {
            throw new FixtureContractViolation($"[{label}] AntiMagic 门未过：Rng[4]={rng[4]}");
        }
        bool g1 = rng[6] < 40;
        bool g2 = g1 && rng.Count > 7 && rng[7] < 6;
        var (c100, d21) = DefaultGateDraws(seed);
        switch (arm)
        {
            case NonDefaultArm.Hit:
                if (!(g1 && g2 && c100 < 32 && d21 < 6))
                {
                    throw new FixtureContractViolation(
                        $"[{label}] 期望两支都成功（arm 门①={g1} 门②={g2}；缺省 c100={c100} 需<32 d21={d21} 需<6）");
                }
                break;
            case NonDefaultArm.RandRateFlip:
                if (!(g1 && g2 && c100 >= 32))
                {
                    throw new FixtureContractViolation(
                        $"[{label}] randrate flip 不成立：arm g1={g1} g2={g2}，缺省 c100={c100}（需 >=32）");
                }
                break;
            case NonDefaultArm.SucessFlip:
                if (!(g1 && c100 < 32 && !g2 && d21 < 6))
                {
                    throw new FixtureContractViolation(
                        $"[{label}] sucess flip 不成立：arm g1={g1} g2={g2}，缺省 c100={c100}（需 <32） d21={d21}（需 <6）");
                }
                break;
        }
    }

    /// <summary>
    /// **MG-48 非缺省对照臂**：三支各一个翻转目标（`Hit`/`RandRateFlip`/`SucessFlip`），
    /// seed 由扫描给出，落盘前逐支做记录值自检（卡 §2.6）。**不**再调默认 `EnsureConfFiles`
    /// ——它会把部署件按 sha 比较后覆盖回来（这正是修复后的预期行为，本支必须避免）。
    /// </summary>
    private static int NonDefaultArmBatch(MagicSpec spec, bool expectNonDefault)
    {
        foreach (var n in ConfNames)
        {
            if (!File.Exists(Path.Combine(AppContext.BaseDirectory, n)))
            {
                Console.Error.WriteLine("CONF_MISSING " + n);
                return 2;
            }
        }
        var arms = new[]
        {
            (Label: "nondef_hit", Arm: NonDefaultArm.Hit, Base: 1000),
            (Label: "nondef_randrate_flip", Arm: NonDefaultArm.RandRateFlip, Base: 2000),
            (Label: "nondef_sucess_flip", Arm: NonDefaultArm.SucessFlip, Base: 3000),
        };
        var results = new FireballResult[arms.Length];
        for (int i = 0; i < arms.Length; i++)
        {
            string label = arms[i].Label;
            if (IsMcVariantLabel(label))
            {
                throw new FixtureContractViolation($"[{label}] 非缺省臂 label 不得命中 MC 变体集合");
            }
            int seed = FindSeedForNonDefaultArm(arms[i].Arm);
            if (seed < 0)
            {
                throw new FixtureContractViolation($"[{label}] 找不到满足翻转合取式的 seed（200000 内）");
            }
            var r = RunMagicCase(spec, label, seed, arms[i].Base, spec.TargetMaxHp, false);
            if (expectNonDefault)
            {
                AssertNonDefaultFlip(label, arms[i].Arm, seed, r.Rng);
            }
            else if (r.Rng.Count > 2 && r.Rng[2] != 0)
            {
                throw new FixtureContractViolation($"[{label}] Rng[2]={r.Rng[2]} 非 0 ⇒ label 被判成 MC 变体");
            }
            results[i] = r;
        }
        return EmitGolden(spec, results);
    }

    /// <summary>
    /// `--case fixture-selftest`：七道门逐条（每条都要 rc=2 的违规输入）+ 一个合法输入。
    /// </summary>
    private static int FixtureSelfTest()
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        int failures = 0;
        void ExpectRed(string name, Action<FixtureContract.FixtureInput> mutate)
        {
            var ok = BaseInput();
            mutate(ok);
            try
            {
                FixtureContract.Validate(ok);
                Console.Error.WriteLine($"FIXTURE_SELFTEST FAIL {name} :: 期望 rc=2 的输入通过了校验");
                failures++;
            }
            catch (FixtureContractViolation)
            {
                Console.Error.WriteLine($"FIXTURE_SELFTEST PASS {name} (rc=2)");
            }
        }

        // 合法输入：登录序 L20 无装备（reachable）。
        var legal = BaseInput();
        FixtureContract.Validate(legal);
        Console.Error.WriteLine("FIXTURE_SELFTEST PASS legal-login-order");

        ExpectRed("missing-rng_mode", f => f.RngMode = "");
        ExpectRed("missing-reachability", f => f.Reachability = "");
        ExpectRed("missing-construction", f => f.Construction = null);
        ExpectRed("illegal-login+freeze", f => f.RngMode = "freeze_at_dospell");
        ExpectRed("login-false-no-note", f =>
        {
            f.LoginOrder = false;
            f.ScenarioNote = "";
        });
        ExpectRed("unreachable-no-evidence", f => f.Reachability = "unreachable");
        ExpectRed("legacy-mismatch", f => f.LegacyArtifact = true);
        ExpectRed("freeze-no-reason", f =>
        {
            f.LoginOrder = false;
            f.ScenarioNote = "synthetic";
            f.RngMode = "freeze_at_dospell";
            f.LegacyReason = "";
        });
        ExpectRed("derived-forged", f => f.Derived.MaxHp += 1);
        ExpectRed("reachable-flat-mismatch", f => f.AntiMagic = 3);

        Console.Error.WriteLine(failures == 0 ? "FIXTURE_SELFTEST OK" : ("FIXTURE_SELFTEST RED failures=" + failures));
        return failures == 0 ? 0 : 1;
    }

    private static FixtureContract.FixtureInput BaseInput()
    {
        var derived = FixtureContract.Recompute(OpenMir2.Enums.PlayerJob.Warrior, 20);
        return new FixtureContract.FixtureInput
        {
            Label = "selftest",
            RngMode = "cover_drain",
            LoginOrder = true,
            Reachability = "reachable",
            AntiMagic = 1,
            TargetLevel = 20,
            TargetHp = derived.MaxHp,
            TargetMaxHp = derived.MaxHp,
            Derived = derived,
            Construction = new FixtureContract.Construction
            {
                Job = OpenMir2.Enums.PlayerJob.Warrior,
                Level = 20,
                Equipment = new List<string>(),
                Skills = new List<string> { "FireBall" },
            },
            ScenarioNote = "",
        };
    }

    /// <summary>
    /// `--case runmagic-input-selftest`（M57 前序的牙）：人造「术后 ≠ 输入」场景（29 的 `heal`：
    /// target 输入 HP=50，治疗术后 HP 上升），断言 `Fixture.target_hp` 仍是**输入 50**、术后值只在
    /// 结果顶层 `TargetHp`。若有人把捕获挪到排水后（术后取值），本用例必红。
    /// </summary>
    private static int RunMagicInputSelfTest()
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        // 11 雷电 hit 支（seed 13）：输入 targetHp=100，术后被打击到 85 ⇒ 术后 ≠ 输入（实测）。
        var spec = new MagicSpec { Key = "lightning-11", MagicId = 11, MagicName = "Lightening" };
        var res = RunMagicCase(spec, "hit", 13, 1000, 100, false);
        var fx = res.Fixture;
        int fxHp = (int)fx.GetType().GetProperty("target_hp").GetValue(fx);
        int fxMax = (int)fx.GetType().GetProperty("target_max_hp").GetValue(fx);
        int fxAnti = (int)fx.GetType().GetProperty("anti_magic").GetValue(fx);
        bool postDiffers = res.TargetHp != 100;
        bool ok = fxHp == 100 && postDiffers;
        Console.Error.WriteLine("RUNMAGIC_INPUT_SELFTEST " + (ok ? "OK" : "FAIL")
            + " fixture.target_hp=" + fxHp + " fixture.target_max_hp=" + fxMax + " fixture.anti_magic=" + fxAnti
            + " post.TargetHp=" + res.TargetHp + " post_differs=" + postDiffers);
        return ok ? 0 : 1;
    }

    /// <summary>
    /// `--case holyshield`：16 的对象选择四支（v1 被动怪物，卡见 openmir2-monster-actors-v2 与 §36）。
    ///
    /// 门（`MagMakeHolyCurtain`，`MagicManager.cs` 的 helper）：`Race >= Animal
    /// &amp;&amp; Random(4) + (caster.Level - 1) > target.Level &amp;&amp; Master == null`。
    /// caster.Level=30 ⇒ **level=1 必中、level=200 必不中**（不依赖 seed 抽值，选择结果确定）。
    /// 四支：两怪全中 / 等级挡 / Master 挡（自指）/ 两怪部分中。
    /// </summary>
    private static int HolyShieldBatch(MagicSpec spec)
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        MonsterSpec Mon(int x, int y, int level, int master = 0) =>
            new MonsterSpec { X = x, Y = y, Race = 50, Level = level, Hp = 300, Master = master };
        spec.Monsters = new[] { Mon(102, 100, 1), Mon(101, 101, 1) };
        var c1 = RunMagicCase(spec, "mon_hit_two", 1, 1000, spec.TargetMaxHp, false);
        spec.Monsters = new[] { Mon(102, 100, 200) };
        var c2 = RunMagicCase(spec, "mon_blocked_level", 1, 2000, spec.TargetMaxHp, false);
        spec.Monsters = new[] { Mon(102, 100, 1, master: 1) };
        var c3 = RunMagicCase(spec, "mon_blocked_master", 1, 3000, spec.TargetMaxHp, false);
        spec.Monsters = new[] { Mon(102, 100, 1), Mon(101, 101, 200) };
        var c4 = RunMagicCase(spec, "mon_partial", 1, 4000, spec.TargetMaxHp, false);
        return EmitGolden(spec, new[] { c1, c2, c3, c4 });
    }

    /// <summary>
    /// `--case unamyounsul`：40 的四支分支矩阵（卡 §34）。
    ///
    /// 门 = `Random(7) - (UserMagicLevel + 1) &lt; 0`（Level=1 ⇒ 首抽 &lt; 2 为开）；
    /// 非零初值取 **5/6/7 互不相同**（"置 1"前后可辨）。四支：
    /// **边界（§26）**：断言只到 DoSpell 时刻——`StatusTimeArr` 归 1 不等于"解毒生效"（清除发生在 Run）。
    /// `gate_open_all`（开+全非零 ⇒ 全归 1）、`gate_closed_all`（关+全非零 ⇒ 不动）、
    /// `gate_open_zero`（开+全零 ⇒ 不动）、`gate_open_mixed`（开+混合 ⇒ 仅非零项归 1）。
    /// 术后值由 `RunMagicCase` 内按"门结果+术前值"独立推导并断言（不拿实现输出自证）。
    /// </summary>
    private static int UnamyounsulBatch(MagicSpec spec)
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        int seedOpen = FindSeedForFirstDraw(7, 0);
        int seedClosed = FindSeedForFirstDraw(7, 3);
        if (seedOpen < 0 || seedClosed < 0)
        {
            Console.Error.WriteLine("SEED_SCAN_FAILED");
            return 1;
        }
        int[] nonzero = { 5, 6, 7 };
        int[] zero = { 0, 0, 0 };
        int[] mixed = { 5, 0, 7 };
        spec.PoisonStates = nonzero;
        var c1 = RunMagicCase(spec, "gate_open_all", seedOpen, 1000, spec.TargetMaxHp, false);
        var c2 = RunMagicCase(spec, "gate_closed_all", seedClosed, 2000, spec.TargetMaxHp, false);
        spec.PoisonStates = zero;
        var c3 = RunMagicCase(spec, "gate_open_zero", seedOpen, 3000, spec.TargetMaxHp, false);
        spec.PoisonStates = mixed;
        var c4 = RunMagicCase(spec, "gate_open_mixed", seedOpen, 4000, spec.TargetMaxHp, false);
        return EmitGolden(spec, new[] { c1, c2, c3, c4 });
    }

    /// <summary>
    /// 把扩展输入挂到 `Fixture`（`status_time_arr` / `monsters`）：只为显式给出这些输入的支出现，
    /// 缺省 ⇒ 原样返回 ⇒ 既有件**零变化**。
    /// </summary>
    private static object AttachStatusTimeArr(object fixture, object status, object monsters = null,
        int? casterHitPoint = null, int? casterDc = null, int? casterAttackMode = null, int? casterSc = null,
        int? casterCharStatusEx = null, int? targetMonster = null, ServerConfFixture.Snapshot serverConf = null,
        bool? targetMonsterUndead = null, bool? faithfulPump = null,
        int? casterDir = null, int? casterGender = null, int? casterHair = null,
        bool? preSpellFaceTarget = null, int? magicTrainLv = null, int? magicMaxTrain0 = null,
        bool? fastTrain = null, bool? fireCrossSafeZone = null, object preEventCells = null,
        int? magicPower = null, int? magicMaxPower = null, int? magicDefPower = null,
        int? magicDefMaxPower = null, int? secondCastAtMs = null, bool? realMap = null,
        bool? productionAoi = null, bool? pushCacheCold = null, bool? observeDirAfter = null,
        int? targetDirBefore = null, int[] healTicksMs = null, int? healTickBase = null,
        int? healLevelTarget = null, int? healLevelCaster = null, bool specHealBoth = false,
        int? amuletDura = null)
    {
        if (status == null && monsters == null && casterHitPoint == null && casterDc == null
            && casterAttackMode == null && casterSc == null && casterCharStatusEx == null
            && targetMonster == null && targetMonsterUndead == null && faithfulPump == null && serverConf == null
            && casterDir == null && casterGender == null && casterHair == null
            && preSpellFaceTarget == null && magicTrainLv == null && magicMaxTrain0 == null
            && fastTrain == null && fireCrossSafeZone == null && preEventCells == null
            && magicPower == null && magicMaxPower == null && magicDefPower == null
            && magicDefMaxPower == null && secondCastAtMs == null && realMap == null
            && productionAoi == null && pushCacheCold == null && observeDirAfter == null
            && targetDirBefore == null && healTicksMs == null && healTickBase == null
            && healLevelTarget == null && healLevelCaster == null && !specHealBoth
            && amuletDura == null)
        {
            return fixture;
        }
        var node = JsonSerializer.SerializeToNode(fixture)!.AsObject();
        if (status != null)
        {
            node["status_time_arr"] = JsonSerializer.SerializeToNode(status);
        }
        if (monsters != null)
        {
            node["monsters"] = JsonSerializer.SerializeToNode(monsters);
        }
        // 卡 §39：施法者夹具输入（仅在被显式给出时落盘；既有件零变化）。
        if (casterHitPoint.HasValue) node["caster_hit_point"] = casterHitPoint.Value;
        if (casterDc.HasValue) node["caster_dc"] = casterDc.Value;
        if (casterAttackMode.HasValue) node["caster_attack_mode"] = casterAttackMode.Value;
        // 卡 36：`nTime = GetAttackPower(GetPower13(60)+Lo(SC)*10, Hi(SC)-Lo(SC)+1)` 的**输入**必须可复算。
        if (casterSc.HasValue)
        {
            node["caster_sc_raw"] = casterSc.Value;
            node["caster_sc_lo"] = casterSc.Value & 0xFF;
            node["caster_sc_hi"] = (casterSc.Value >> 8) & 0xFF;
        }
        if (casterCharStatusEx.HasValue) node["caster_char_status_ex"] = casterCharStatusEx.Value;
        if (targetMonster.HasValue) node["target_monster"] = targetMonster.Value;
        // **MG-38 后续批**：只在显式声明「非不死目标」时落盘（false）；缺键 = true ⇒ 旧 golden 零变化。
        if (targetMonsterUndead.HasValue) node["target_monster_undead"] = targetMonsterUndead.Value;
        // **B17（2026-11-26）**：语义 = **该 case 以生产泵语义（含 t=0 泵）对齐**；
        // 只在 opt-in 的 case 落盘（true）；缺键 = false ⇒ 旧 golden 零变化。
        if (faithfulPump.HasValue) node["faithful_pump"] = faithfulPump.Value;
        // **MG-57（2026-11-26）**：条件键（缺省不落盘 ⇒ 既有 golden 逐字节不变）。
        if (secondCastAtMs.HasValue) node["second_cast_at_ms"] = secondCastAtMs.Value;
        if (realMap.HasValue) node["real_map"] = realMap.Value;
        if (productionAoi.HasValue) node["production_aoi"] = productionAoi.Value;
        if (pushCacheCold.HasValue) node["push_cache_cold"] = pushCacheCold.Value;
        if (observeDirAfter.HasValue) node["observe_dir_after"] = observeDirAfter.Value;
        if (targetDirBefore.HasValue) node["target_dir_before"] = targetDirBefore.Value;
        if (healTicksMs != null)
        {
            node["heal_ticks_ms"] = JsonSerializer.SerializeToNode(healTicksMs);
        }
        if (healTickBase.HasValue) node["inc_health_spell_tick_base"] = healTickBase.Value;
        if (healLevelTarget.HasValue) node["heal_level_target"] = healLevelTarget.Value;
        if (healTicksMs != null && specHealBoth) node["heal_ticks_both"] = true;
        if (healLevelCaster.HasValue) node["heal_level_caster"] = healLevelCaster.Value;
        if (amuletDura.HasValue) node["amulet_dura"] = amuletDura.Value;
        if (serverConf != null)
        {
            // 十键快照（`Dictionary` 插入序 = `Keys` 表序 = Rust `SERVER_CONF_KEYS`）。
            node["server_conf"] = JsonSerializer.SerializeToNode(serverConf.Values);
        }
        // **MG-55（46 分身）**：分身夹具输入（只有 CloneBatch 传；其它 stem 键不出现 ⇒ 零变化）。
        if (casterDir.HasValue) node["caster_dir"] = casterDir.Value;
        if (casterGender.HasValue) node["caster_gender"] = casterGender.Value;
        if (casterHair.HasValue) node["caster_hair"] = casterHair.Value;
        if (preSpellFaceTarget.HasValue) node["pre_spell_face_target"] = preSpellFaceTarget.Value;
        if (magicTrainLv.HasValue) node["magic_train_lv"] = magicTrainLv.Value;
        if (magicMaxTrain0.HasValue) node["magic_max_train0"] = magicMaxTrain0.Value;
        // B16 Arm A：五字段（非零才写；控制臂四键缺省、训练键写 0）。
        if (magicPower.HasValue) node["magic_power"] = magicPower.Value;
        if (magicMaxPower.HasValue) node["magic_max_power"] = magicMaxPower.Value;
        if (magicDefPower.HasValue) node["magic_def_power"] = magicDefPower.Value;
        if (magicDefMaxPower.HasValue) node["magic_def_max_power"] = magicDefMaxPower.Value;
        if (fastTrain.HasValue) node["fast_train"] = fastTrain.Value;
        if (fireCrossSafeZone.HasValue) node["firecross_safe_zone"] = fireCrossSafeZone.Value;
        if (preEventCells != null) node["pre_event_cells"] = JsonSerializer.SerializeToNode(preEventCells);
        return node;
    }

    /// <summary>
    /// `--case groupdeding`：39 `SKILL_GROUPDEDING`（`MagGroupDeDing`，`MagicManager.cs:942-978`）
    /// 四支（卡 §39，甲方案，opc-19 2026-11-20 定稿）。
    ///
    /// 施法者夹具（三件必要输入，均落进 golden 的 `Fixture`）：
    /// - `AttatckMode = HAM_PEACE`：只有 `Race >= Animal`（怪物）进伤害链；玩家 target 仍收
    ///   `RM_10205`（该发送在 `IsProperTarget` 块之外）但不取任何随机数 ⇒ 抽值序列只由怪物构成，
    ///   与卡面「怪物顺序显式固定、逐只记抽值」的口径一致。
    /// - `WAbil.DC = MakeWord(20,20)`：`GetAttackPower(Lo, Hi-Lo)` 的随机区间为 0 ⇒ `nPower` 恒 = 20。
    /// - `HitPoint` 逐支给（`0` = 命中门恒真 ⇒ 全 miss；`5` = `Random(0)=0 < 5` 必中）。
    ///
    /// **四支的 `CasterDC` 统一为 `MakeWord(20,20)`（区间=0 ⇒ `nPower` 恒 20、伤害常数）**：
    /// 这是为隔离"命中门 / 护甲衰减"的简化，**代价是本批不覆盖 `GetAttackPower` 的区间抽**。
    /// 为补这一覆盖，另加第五支 `dc_range`（`MakeWord(20,30)`，区间=10）单列，不塞进四支。
    ///
    /// 五支（怪物顺序显式固定 = `GetMapBaseObjects` 收集序：x 升序→y 升序）：
    /// - `all_miss_hp0`：M1(101,101,S=0)、M2(102,100,S=1)，HitPoint=0 ⇒ 门恒真，全场 miss；
    /// - `hit_ac_split`：M1(S=0,AC=0)、M2(S=0,AC=MakeWord(5,5))，**除 AC 外全同**；两者必中，
    ///   M1 无衰减（damage=20），M2 经护甲衰减（damage=20-(5..9)）；
    /// - `miss_by_draw`：M1(S=0) 必中、M2(S=10) 经 seed 选到 `Random(10) >= 5`（唯一扫 seed 的一支）；
    /// - `dead_excluded`：单只 `Death=true` 的怪在 `GetMapBaseObjects` 收集期被过滤 ⇒ 零抽、无 RM；
    /// - `dc_range`：M1(S=0,AC=0)，`CasterDC=MakeWord(20,30)` ⇒ `Random(11)` 进伤害（nPower=20..30）。
    /// </summary>
    private static int GroupDeDingBatch(MagicSpec spec)
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        spec.CasterAttackMode = (int)OpenMir2.Enums.AttackMode.HAM_PEACE;
        MonsterSpec Mon(int x, int y, int speed, int ac = 0, bool death = false) =>
            new MonsterSpec { X = x, Y = y, Race = 50, Level = 1, Hp = 300, SpeedPoint = speed, AC = ac, Death = death };

        int dcFlat = HUtil32.MakeWord(20, 20);   // 区间 0：伤害常数 20（隔离命中门/护甲）
        int dcRange = HUtil32.MakeWord(20, 30);  // 区间 10：覆盖 GetAttackPower 的区间抽

        var c1 = RunGroupDeDingArm(spec, "all_miss_hp0", 0, dcFlat, 1000, 0,
            new[] { Mon(101, 101, 0), Mon(102, 100, 1) });

        var c2 = RunGroupDeDingArm(spec, "hit_ac_split", 5, dcFlat, 2000, 5,
            new[] { Mon(101, 101, 0, ac: 0), Mon(102, 100, 0, ac: HUtil32.MakeWord(5, 5)) });

        int missSeed = FindDeDingMissByDrawSeed();
        if (missSeed < 0)
        {
            Console.Error.WriteLine("SEED_SCAN_FAILED groupdeding/miss_by_draw");
            return 2;
        }
        var c3 = RunGroupDeDingArm(spec, "miss_by_draw", 5, dcFlat, 3000, missSeed,
            new[] { Mon(101, 101, 0), Mon(102, 100, 10) });

        var c4 = RunGroupDeDingArm(spec, "dead_excluded", 5, dcFlat, 4000, 5,
            new[] { Mon(101, 101, 0, death: true) });

        var c5 = RunGroupDeDingArm(spec, "dc_range", 5, dcRange, 5000, 7,
            new[] { Mon(101, 101, 0, ac: 0) });

        return EmitGolden(spec, new[] { c1, c2, c3, c4, c5 });
    }

    private static FireballResult RunGroupDeDingArm(
        MagicSpec spec, string label, int hitPoint, int casterDc, int idBase, int seed, MonsterSpec[] monsters)
    {
        spec.CasterHitPoint = hitPoint;
        spec.CasterDC = casterDc;
        spec.Monsters = monsters;
        return RunMagicCase(spec, label, seed, idBase, spec.TargetMaxHp, false);
    }

    /// <summary>
    /// `miss_by_draw` 的 seed：M2(S=10) 的命中门抽 **≥ 5**（`Random(10) >= HitPoint=5` ⇒ miss）。
    /// 抽序（HAM_PEACE ⇒ 只有两只怪进伤害链）：M1 攻击 2 抽 + 命中门 `Random(0)` 1 抽 + 护甲
    /// `Random(1)` 1 抽；M2 攻击 2 抽 + 命中门 `Random(10)` 1 抽。按 `LoggingRandom` 的逐次 `Next(n)`
    /// 复现（`Random(0)` 也占位），与真跑同序即可复现筛选。
    /// </summary>
    private static int FindDeDingMissByDrawSeed()
    {
        for (int seed = 1; seed < 200000; seed++)
        {
            var r = new Random(seed);
            r.Next(1); r.Next(10); r.Next(0); // M1 攻击 ×2 + 命中门 Random(0)
            r.Next(1);                        // M1 护甲 Random(1)
            r.Next(1); r.Next(10);            // M2 攻击 ×2
            if (r.Next(10) >= 5) return seed; // M2 命中门
        }
        return -1;
    }

    /// <summary>
    /// `--case uenhancer`：36 `SKILL_UENHANCER`（无极真气，`MagicManager.cs:610-630`）。
    ///
    /// 目标归一（`:612-617`）：`targetObject == null ⇒ target = self`（本批用 `targetNull=true`），
    /// 效果 **`AttPowerUp` 落在施法者自己**：`ExtraAbil[0]/Times[0]` + `SysMsg` + `RecalcAbilitys`
    /// + `SendMsg(RM_ABILITY)` —— 全部在 DoSpell 内**施法者侧同步**完成 ⇒ 可升 verified。
    /// **scope**：只证 DoSpell 时刻的 `ExtraAbil[0]/Times[0]` 与帧；施法者 `Abil` 已按登录序对齐
    /// （`CasterLoginOrderAbil`：只 `RecalcLevelAbilitys()`，保留 `WAbil.SC=15` 供 `nTime`）。
    /// 门：`IsProperFriend(target)` → `CheckAmulet(1,1)`（`ArmRingAmulet=true`）。
    /// </summary>
    private static int UenhancerBatch(MagicSpec spec)
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        spec.CasterLoginOrderAbil = true;
        spec.CasterCharStatusEx = 2; // 非 0 夹具：GetCharStatus()=2，使 ④ 读取点可证伪
        spec.CasterHp = 500;
        spec.CasterMaxHp = 500;
        var cases = new[]
        {
            RunMagicCase(spec, "s1", 1, 1000, spec.TargetMaxHp, true),
            RunMagicCase(spec, "s5", 5, 2000, spec.TargetMaxHp, true),
            RunMagicCase(spec, "s13", 13, 3000, spec.TargetMaxHp, true),
        };
        return EmitGolden(spec, cases);
    }

    /// <summary>`--case skeleton`：17 两支（正常召唤 / 数量上限），MasterRoyaltyTick 墙钟不记不比（§36）。</summary>
    private static int SkeletonBatch(MagicSpec spec)
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        spec.PreSlaveCount = 0;
        var c1 = RunMagicCase(spec, "summon_ok", 1, 1000, spec.TargetMaxHp, false);
        spec.PreSlaveCount = Math.Max(1, SystemModule.SystemShare.Config.SkeletonCount);
        var c2 = RunMagicCase(spec, "cap_hit", 1, 2000, spec.TargetMaxHp, false);
        return EmitGolden(spec, new[] { c1, c2 });
    }
    /// <summary>
    /// 共享尾 B 类（49/51/52/53/54）三支 s1/s5/s13：case 体只有 `boTrain = true;`，
    /// 观测面 = 638 + 1000ms 后的 640 + 训练面（Level/TranPoint/TrainLv/MaxTrain）。
    /// 生成期断言（不满足即 rc=2、不落盘）：每支 `Rng.len()==1`（只有 `:663` 的 `Random(3)+1` 一抽）。
    /// </summary>
    private static int SkillTailBatch(MagicSpec spec)
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        var cases = new List<FireballResult>
        {
            RunMagicCase(spec, "s1", 1, 1000, spec.TargetMaxHp, false),
            RunMagicCase(spec, "s5", 5, 2000, spec.TargetMaxHp, false),
            RunMagicCase(spec, "s13", 13, 3000, spec.TargetMaxHp, false),
        };
        // **升级变体 s1_up**（MG-58/60/61/62/63 的 800ms 支）：`MaxTrain[0]=1 <= TranPoint(0)+(Random(3)+1)`
        // ⇒ `CheckMagicLevelUp` 成功 ⇒ `TranPoint -= MaxTrain`、`Level 0→1`、`UpdateDelayMsg(…, 800)`
        // （`PlayObject.cs:5210-5216`）。与 `s1` 同 seed=1/ActorId 段尾（4100）⇒ 抽数与帧可逐项对照。
        // 输入记录在逐 case 的 `Train` 块（`max_train0`/`n_level`/`level_before/after`）；
        // `EmitGolden` 的 fixture_spec 不落这两键，故跑完立即还原，避免影响其它支的语义。
        spec.TailUpgrade = true;
        spec.MagicMaxTrain0 = 1;
        cases.Add(RunMagicCase(spec, "s1_up", 1, 4100, spec.TargetMaxHp, false));
        spec.TailUpgrade = false;
        spec.MagicMaxTrain0 = 1000;
        foreach (var c in cases)
        {
            if (c.Rng.Count != 1)
            {
                throw new FixtureContractViolation(
                    $"[{spec.Key}/{c.Label}] 训练尾应恰 1 抽（Random(3)+1），实得 {c.Rng.Count}：{string.Join(",", c.Rng)}");
            }
        }
        return EmitGolden(spec, cases.ToArray());
    }

    /// <summary>
    /// `--case angel`：41 `SKILL_ANGEL` 两支（summon_ok / cap_hit），镜像 17 的 skeleton 批次。
    ///
    /// cap 门：`MagMakeAngelSlave`（`MagicManager.cs:1346-1349`）调
    /// `MakeSlave(Config.Angel, level, level, **1**, DwRoyaltySec)` ⇒ `nMaxMob = 1`；
    /// 预置 1 个已存在奴隶即令 `SlaveList.Count < 1` 门失败（与 `SkeletonCount` 无关）。
    /// </summary>
    private static int AngelBatch(MagicSpec spec)
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        spec.PreSlaveCount = 0;
        var c1 = RunMagicCase(spec, "summon_ok", 1, 1000, spec.TargetMaxHp, false);
        spec.PreSlaveCount = 1; // `MakeSlave(..., nMaxMob = 1)`
        var c2 = RunMagicCase(spec, "cap_hit", 1, 2000, spec.TargetMaxHp, false);
        return EmitGolden(spec, new[] { c1, c2 });
    }

    private static int SinsuBatch(MagicSpec spec)
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        // SINSU 神兽 = race 113（ELF_MONSTER）⇒ apply_slave_level_abilitys 第一支(0.3)。
        spec.EnableMaps = true; // cap_hit 回召需 SpaceMove：给 env 一个 MapName/ServerIndex
        Environment.SetEnvironmentVariable("MAGICGOLDEN_STUB_MONSTER_RACE", "113");
        // 预置 slave 用精确名 `神兽`（RecallSlave 只认 ChrName==Config.Dragon|Dragon1）。
        spec.PreSlaveName = SystemModule.SystemShare.Config.Dragon;
        spec.PreSlaveCount = 0;
        var c1 = RunMagicCase(spec, "summon_ok", 1, 1000, spec.TargetMaxHp, false);
        // cap_hit：预置 slave（精确名 神兽，使 RecallSlave 命中）令 MakeSlave 门失败 ⇒ RecallSlave 移它。
        spec.PreSlaveCount = Math.Max(1, SystemModule.SystemShare.Config.DragonCount);
        var c2 = RunMagicCase(spec, "cap_hit", 1, 2000, spec.TargetMaxHp, false);
        Environment.SetEnvironmentVariable("MAGICGOLDEN_STUB_MONSTER_RACE", null);
        return EmitGolden(spec, new[] { c1, c2 });
    }
    private static int EmitGolden(MagicSpec spec, FireballResult[] cases)
    {
        var outObj = new
        {
            magic_id = spec.MagicId,
            anti_magic = spec.AntiMagic,
            fixture_spec = new
            {
                magic_name = spec.MagicName,
                effect = spec.Effect,
                effect_type = spec.EffectType,
                user_magic_level = spec.UserMagicLevel,
                caster_level = spec.CasterLevel,
                caster_mc = spec.CasterMc,
                caster_sc = spec.CasterSc,
                target_level = spec.TargetLevel,
                target_max_hp = spec.TargetMaxHp,
                arm_ring_amulet = spec.ArmRingAmulet,
            },
            cases,
        };
        var json = JsonSerializer.Serialize(outObj, new JsonSerializerOptions { WriteIndented = true });
        // **卡 magic-push-batch-impl**：记录这三支的 `SoftVersionDateEx` **生效值**。
        // 它决定 `11029 SM_SERVERCONFIG` 发不发（`SendServerConfig` 在 `SoftVersionDateEx == 0` 时
        // 直接 return，`src/M2Server/Player/PlayObject.Base.cs:941-971`）；装置四个对象都设 20240101。
        // **只对这三支写入**，其它 stem 的 Fixture 一字不改 —— 不加 `MagicSpec` 旋钮，
        // 避免"缺省写不写"这件事改变既有 stem 的行为面（记录 ≠ 输入）。
        if (spec.Key == "firewind" || spec.Key == "energyrepulsor" || spec.Key == "spacemove")
        {
            var node = System.Text.Json.Nodes.JsonNode.Parse(json)!;
            foreach (var c in node["cases"]!.AsArray())
            {
                c!["Fixture"]!["soft_version_date_ex"] = 20240101;
            }
            // **MG-35（21）还要记"图身份"**：落点由 `Random(Width-nEgdey-1)+nEgdey` 决定
            // （`MapRandomMove` 的 `SpaceMoveGetRandXY`），**图的宽高直接决定落点**。
            // 不记图身份的话，换一张尺寸不同的 `0.map` 就会得到不同 golden 而没人知道为什么。
            // 与 Rust 驱动**读同一个环境变量**（`MAGICGOLDEN_MAP_DIR`），并由驱动侧核 sha256/size
            // ⇒ "两侧读的是同一张图"由**同一输入 + 同一校验**保证，不是巧合。
            // **MG-57 / B29**：`spec.RealMap` 的 stem 也要记图身份（rig 走真 0.map 时按 size/sha 钉）；
            // 只记 `map_file_sha256/map_file_size`，`map_dir/map_file/map_bright` 仍只属 21。
            // **MG-57 P2**：`RealMap` 可能是**逐 case**（ExtraCaseSpec）给的 ⇒ 本块对这三个 stem 总是进，
            // 内部按**该 case 的 Fixture.real_map** 决定是否写图身份；其它 stem 不进（既有件零变化）。
            if (spec.Key == "spacemove" || spec.Key == "firewind" || spec.Key == "energyrepulsor")
            {
                var mapDir = Environment.GetEnvironmentVariable("MAGICGOLDEN_MAP_DIR") ?? @"E:\MirServer\M2GameSvr\Map";
                var mapPath = Path.Combine(mapDir, "0.map");
                if (!Directory.Exists(mapDir)) throw new InvalidOperationException("MAP_DIR_MISSING " + mapDir);
                if (!File.Exists(mapPath)) throw new InvalidOperationException("MAP_FILE_MISSING " + mapPath);
                var bytesMap = File.ReadAllBytes(mapPath);
                var sha = Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(bytesMap)).ToLowerInvariant();
                foreach (var c in node["cases"]!.AsArray())
                {
                    if (spec.Key == "spacemove")
                    {
                        c!["Fixture"]!["map_dir"] = mapDir;
                        c!["Fixture"]!["map_file"] = "0.map";
                    }
                    bool caseRealMap = c!["Fixture"]!["real_map"]?.GetValue<bool>() == true;
                    if (spec.Key == "spacemove" || caseRealMap)
                    {
                        c!["Fixture"]!["map_file_sha256"] = sha;
                        c!["Fixture"]!["map_file_size"] = bytesMap.LongLength;
                    }
                }
                // **`634.series` 的时刻输入（卡 magic-push-batch-impl）**：`SM_CHANGEMAP(634).Series`
                // = `PlayObject.DayBright()`，其输入是 `M2Share.GameTime`（`PlayObject.Base.cs:244-269`：
                // `1→0`、`3→1`、其它 `→2`；`Envir.Flag.DayLight/boDarkness` 另有覆盖）。
                // 真机由 `SystemProcessor` 每 50ms 用 `DateTime.Now.Hour` 刷新它；**本装置不跑
                // SystemProcessor** ⇒ 生效值恒为静态默认 `0` ⇒ `DayBright()==2`（golden 的 634.series）。
                // 不记这个值的话，Rust 驱动只能读真实本地小时：`hour∈{4,11,15,20,23}` 之外
                // `game_time_from_hour` 给 0/2 ⇒ `DayBright()` 会变成 0，与本 golden 的 2 不同
                // （当天 09:xx 实测必红）。记录的是**实际用了的值**，不是重算的意图值。
                var bright = (long)M2Server.M2Share.GameTime;
                var brightCases = spec.Key == "spacemove"
                    ? node["cases"]!.AsArray()
                    : new System.Text.Json.Nodes.JsonArray();
                foreach (var c in brightCases)
                {
                    // **键名语义**：`map_bright` 存的是 `M2Share.GameTime` 的**原值**（0..3），
                    // **不是** `DayBright()` 的返回值 —— 0 对应 `DayBright()==2`（1→0、3→1、其它→2）。
                    // 后来人若"顺手修正"成 `DayBright()` 的结果（当前是 2），Rust 驱动注入 2 后
                    // 得到 series=2，看似等价，但 device 再改 GameTime 时会与记录值脱钩。
                    c!["Fixture"]!["map_bright"] = bright;
                    // 无墙钟来源（固定静态默认）⇒ `-1` 表示"不是真实本地小时"，仅作来源标注。
                    c!["Fixture"]!["map_bright_hour"] = -1;
                }
            }
            json = node.ToJsonString(new JsonSerializerOptions { WriteIndented = true });
        }
        var argv = Environment.GetCommandLineArgs();
        var outIdx = Array.IndexOf(argv, "--out");
        if (outIdx > 0 && outIdx + 1 < argv.Length)
        {
            var outPath = argv[outIdx + 1];
            Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outPath))!);
            File.WriteAllText(outPath, json);
            Console.Error.WriteLine("GOLDEN_WRITTEN " + outPath);
        }
        Console.Out.WriteLine(json);
        Console.Error.WriteLine(spec.Key + "_CASES " + string.Join(", ", cases.Select(c => c.Label + "[rng=" + c.Rng.Count + ",passes=" + c.DrainPasses + ",cap=" + c.DrainPassesHitCap + "; " + string.Join(" ", c.Receivers.Select(r => r.Who + ":rm" + r.Rm.Count + "/sm" + r.Sent.Count)) + "]")));
        return 0;
    }

    /// <summary>按 `MagicSpec` 构造 `MagicInfo`（`TrainLv`/`MaxTrain` 缺省 0 = 既有行为）。</summary>
    private static OpenMir2.Data.MagicInfo BuildMagicInfo(MagicSpec spec)
    {
        var mi = new OpenMir2.Data.MagicInfo
        {
            MagicId = (ushort)spec.MagicId,
            MagicName = spec.MagicName,
            Effect = (byte)spec.Effect,
            EffectType = (byte)spec.EffectType,
        };
        // B16 Arm A：五字段直接写入（旧 stem 全 0 ⇒ 与 ctor 缺省相同；golden 侧非零才落键）。
        mi.Power = (ushort)spec.MagicPower;
        mi.MaxPower = (ushort)spec.MagicMaxPower;
        mi.DefPower = (byte)spec.MagicDefPower;
        mi.DefMaxPower = (byte)spec.MagicDefMaxPower;
        if (spec.MagicTrainLv != 0 || spec.MagicMaxTrain0 != 0)
        {
            mi.TrainLv = (byte)spec.MagicTrainLv;
            mi.MaxTrain[0] = spec.MagicMaxTrain0;
        }
        return mi;
    }

    /// <summary>跑一支魔法（`DoSpell` 直调），返回账本。`targetNull=true` ⇒ 目标对象为 null（治愈 self 支）。</summary>
        /// <summary>
    /// **MG-12/40**：调 `PlayObject.Run()`（跑 oracle 恢复段）前的**惰性前提**——`Run()` 的其它段
    /// 必须全部静止，否则本批就不是"只跑恢复段"（挡路先报 opc-27，**不许改 oracle**）。
    /// </summary>
    private static void AssertRunInert(RecordingPlayObject p, string where)
    {
        var problems = new List<string>();
        if (p.Dealing) problems.Add("Dealing");
        if (SystemModule.SystemShare.Config.PayMentMode == 3) problems.Add("PayMentMode==3");
        if (p.FireHitSkill) problems.Add("FireHitSkill");
        if (p.TwinHitSkill) problems.Add("TwinHitSkill");
        if (p.IsTimeRecall) problems.Add("IsTimeRecall");
        if (p.IncSpell != 0) problems.Add("IncSpell=" + p.IncSpell);
        if (p.IncHealth != 0) problems.Add("IncHealth=" + p.IncHealth);
        if (p.MsgQueueCount != 0) problems.Add("MsgQueue=" + p.MsgQueueCount);
        if (p.Death) problems.Add("Death");
        if (problems.Count > 0)
        {
            throw new FixtureContractViolation(
                "[" + where + "] Run() 非惰性段活跃：" + string.Join(", ", problems)
                + "（先报 opc-27，不许改 oracle）");
        }
    }

private static FireballResult RunMagicCase(
        MagicSpec spec, string label, int seed, int idBase, int targetHp, bool targetNull,
        AntiMagicBranchExpect branchExpect = null)
    {
        ClockSeam.Begin(1000000);
        string mapDir = Environment.GetEnvironmentVariable("MAGICGOLDEN_MAP_DIR") ?? @"E:\MirServer\M2GameSvr\Map";
        var env = new M2Server.Maps.Envirnoment();
        if (!env.LoadMapData(Path.Combine(mapDir, "0.map")))
        {
            throw new InvalidOperationException("MAP_LOAD_FAILED " + mapDir);
        }
        // 卡 openmir2-golden-harness-mapmgr：设 MapMgr（FindMap 返回本 case 已加载的 env），供 RecallSlave→SpaceMove。
        // 无条件设 MapMgr 安全：其它 spec 不触达 FindMap（否则原 null 会 NPE），且 skeleton/fireball/fire 结构 diff ⊆ drift；EnableMaps 门控：env.MapName/ServerIndex + 预置 slave 的 RecordingMonster 构造 + caster.Dir（均 SINSU）。
        SystemModule.SystemShare.MapMgr = new OfflineMapSystem(env);
        if (spec.EnableMaps)
        {
            env.MapName = "0";
            env.ServerIndex = M2Server.M2Share.ServerIndex;
            // **`Envir.MapFileName`（卡 magic-push-batch-impl）**：oracle `RM_CHANGEMAP` 的尾是
            // `SendMsg(RM_CHANGEMAP, 0,0,0,0, MapFileName)`（`BaseObject.cs:1134`），生产由
            // `MapManager.AddMapInfo` 从 `[名|文件]` 取（`GameSrv/Maps/MapManager.cs:66-87`）。
            // 装置原先不设 ⇒ 尾恒空串，与生产（"0" ⇒ EDCode ``?`）不忠实；Rust 侧按注册表
            // 给 `map_file_name_of` ⇒ 两侧恒不等。这里按生产口径补齐。
            env.MapFileName = "0";
        }
        var caster = new RecordingPlayObject { ChrName = "golden-caster", ActorId = idBase + 1, CurrX = 100, CurrY = 100, SoftVersionDateEx = 20240101, ClientTick = 5 };
        // **MG-57 P2**：坐标覆盖（缺省 null ⇒ 既有值；建对象后、`AddMapObject` 之前生效）。
        if (spec.CasterXy != null) { caster.CurrX = (short)spec.CasterXy[0]; caster.CurrY = (short)spec.CasterXy[1]; }
        caster.Abil.Level = (byte)spec.CasterLevel;
        // **MG-55（46 分身）**：`Dir/Gender/Hair` 夹具输入（`-1` = 不设 ⇒ 既有件零变化）。
        if (spec.CasterDir >= 0) caster.Dir = (byte)spec.CasterDir;
        if (spec.CasterGender >= 0) caster.Gender = (OpenMir2.Enums.PlayerGender)spec.CasterGender;
        if (spec.CasterHair >= 0) caster.Hair = (byte)spec.CasterHair;
        bool useMcVariant = spec.CasterMcNoClamp > 0 && IsMcVariantLabel(label);
        // SC 变体的逐 case 标签：伤害/火魅/DefenceUp 的 `hit`/`s5`/`s13` + 治疗面的 `self`/`both`。
        bool useScVariant = spec.CasterScNoClamp > 0
            && (label == "s5" || label == "s13" || label == "hit" || label == "self" || label == "both" || label == "heal");
        caster.WAbil.MC = (ushort)(useMcVariant ? spec.CasterMcNoClamp : spec.CasterMc);
        caster.WAbil.SC = (ushort)(useScVariant ? spec.CasterScNoClamp : spec.CasterSc);
        // 卡 §39：命中门 / 攻击力 / 攻击模式的夹具输入（缺省 -1 ⇒ 既有件零变化）。
        if (spec.CasterHitPoint >= 0) caster.HitPoint = (byte)spec.CasterHitPoint;
        if (spec.CasterDC >= 0) caster.WAbil.DC = (ushort)spec.CasterDC;
        if (spec.CasterAttackMode >= 0) caster.AttatckMode = (OpenMir2.Enums.AttackMode)spec.CasterAttackMode;
        // 卡 36：施法者 Abil 按等级派生（只 RecalcLevelAbilitys，不碰 WAbil）。
        if (!string.IsNullOrEmpty(spec.HomeMap)) caster.HomeMap = spec.HomeMap;
        if (spec.CasterLoginOrderAbil) caster.RecalcLevelAbilitys();
        // **B21 语义声明**：`caster_sc`/`caster_mc` 是**施法输入**——`WAbil.SC/MC` 在 `DoSpell` 之前的
        // 取值，供 `nTime = GetAttackPower(GetPower13(60) + Lo(SC)*10, Hi(SC)-Lo(SC)+1)`。
        // **不是事后态声明**：生产 `AttPowerUp → RecalcAbilitys()`（`WAbil = Abil.Clone()`，
        // `BaseObject.Abil.cs:27`）会按 `Abil` 重物化（Warrior 等级派生 ⇒ SC/MC = 0），两者**本就不同**。
        // **错误修法（反例，2026-11-21 实测）**：把输入"物化"进 `Abil.SC/MC` ⇒ oracle 事后态变成 15，
        // `SM_ABILITY(52)` 的 `WAbil` 尾 MC/SC 字节 0→15（帧翻转），而 Rust `recalc_session_ability`
        // → `level_ability_base` 每次按等级覆盖 ⇒ comparator 的 `Sent[*].tail` 严格比较**必红**。
        // 语义门只守反方向：`Abil` 自登录序派生后**不得再被改写**（防再造同一错误修法）。
        int loginAbilSc = caster.Abil.SC;
        int loginAbilMc = caster.Abil.MC;
        if (spec.CasterCharStatusEx != 0) caster.CharStatusEx = spec.CasterCharStatusEx;
        if (spec.CasterHp > 0)
        {
            // 29 的「多朋友」支：施法者自身也要 `HP < MaxHP` 才被 `MagBigHealing` 选中。
            caster.WAbil.HP = (ushort)spec.CasterHp;
            caster.WAbil.MaxHP = (ushort)spec.CasterMaxHp;
        }
        var target = new RecordingPlayObject { ChrName = "golden-target", ActorId = idBase + 2, CurrX = 101, CurrY = 100, SoftVersionDateEx = 20240101, ClientTick = 5, AntiMagic = (ushort)spec.AntiMagic };
        if (spec.TargetXy != null) { target.CurrX = (short)spec.TargetXy[0]; target.CurrY = (short)spec.TargetXy[1]; }
        target.Abil.Level = (byte)spec.TargetLevel;
        // **真实登录序**（`PlayObject.cs:1308-1309`：`RecalcLevelAbilitys(); RecalcAbilitys();`）：
        // 先按 Job+Level 把 `Abil` 的等级派生字段（MaxHP/MaxMP/MaxWeight/DC/MC/SC/AC/MAC）写成
        // 自洽值，后续 `RecalcAbilitys()`（`BaseObject.Abil.cs:27` `WAbil = Abil.Clone()`）才有
        // 语义。此前的夹具只设 `Abil.Level`/`Abil.MaxHP`，等价于「等级 20 但 Abil 仍是 ctor
        // 缺省」，`RecalcAbilitys()` 克隆出的是不自洽的 WAbil（MaxHP=100/DC=1），Rust 侧
        // `recalc_session_ability`（= 上述两函数的合并实现，见其文档注释）无法等价复现。
        // `Abil.MaxExp` 保持 ctor 缺省 50：登录后的 `Abil.MaxExp = GetLevelExp(...)` 属数据面，
        // 不在本对拍夹具的建模范围（Rust 夹具同置 max_exp=50）。
        target.RecalcLevelAbilitys();
        target.WAbil.HP = (ushort)targetHp;
        // 登录序的最后一步（`PlayObject.cs:1308-1309`：`RecalcLevelAbilitys(); RecalcAbilitys();`）：
        // 由 `RecalcAbilitys()` 把 `Abil` 物化到 `WAbil`（`MaxHP/MaxMP/DC/...` 与登录后的
        // 客户端视图一致）。**必须**在 `NetChannel` 安装/排水循环之前做，并清掉它产生的
        // `RM_CHARSTATUSCHANGED`（登录帧不属于施法窗口）——28 的 `SM_OPENHEALTH(1100)` 直接读
        // `WAbil.HP/MaxHP`（`PlayObject.Message.cs:2009-2011`），少了这一步 MaxHP 会是 ctor 的 0。
        target.RecalcAbilitys();
        target.DrainRaw();
        // **施毒骰输入**：`AntiPoison` 在登录序 recalc 之后再写（recalc 会把 `Abil` 物化到 `WAbil`，
        // 但 `AntiPoison` 是 `BaseObject` 的独立字段；先写会被 recalc 的累加覆盖）。
        if (spec.TargetAntiPoison > 0)
        {
            target.AntiPoison = (byte)spec.TargetAntiPoison;
        }
        // **三态计时器输入**（状态/辅助族，卡 §34）：只有显式给了 `PoisonStates` 的支才写；
        // 写入是纯状态赋值（不入队、不产生帧），且在 RNG/DoSpell 之前 ⇒ 不影响既有支。
        if (spec.PoisonStates != null)
        {
            if (spec.PoisonStates.Length != 3)
            {
                throw new FixtureContractViolation(
                    $"[{spec.Key}] PoisonStates 必须是 3 元（DECHEALTH/DAMAGEARMOR/STONE）");
            }
            target.StatusTimeArr[OpenMir2.Consts.PoisonState.DECHEALTH] = (ushort)spec.PoisonStates[0];
            target.StatusTimeArr[OpenMir2.Consts.PoisonState.DAMAGEARMOR] = (ushort)spec.PoisonStates[1];
            target.StatusTimeArr[OpenMir2.Consts.PoisonState.STONE] = (ushort)spec.PoisonStates[2];
        }
        int[] statusBefore = new[]
        {
            (int)target.StatusTimeArr[OpenMir2.Consts.PoisonState.DECHEALTH],
            (int)target.StatusTimeArr[OpenMir2.Consts.PoisonState.DAMAGEARMOR],
            (int)target.StatusTimeArr[OpenMir2.Consts.PoisonState.STONE],
        };
        // **驱动输入（输入/输出语义）**：这四个值在「target 构造完成、RNG/DoSpell 之前」捕获并写入
        // `Fixture`——`2d` 的驱动用它们设 target 初始状态。术后状态（如受击后的 HP）只留在结果顶层
        // `TargetHp`，**不得**回写成 `Fixture` 输入（否则驱动会从术后值起手）。
        int inputTargetLevel = target.Abil.Level;
        int inputTargetHp = target.WAbil.HP;
        int inputTargetMaxHp = target.WAbil.MaxHP;
        int inputAntiMagic = target.AntiMagic;
        int inputAntiPoison = target.AntiPoison;
        CheckBranchThreshold("runmagic/" + spec.Key + "/" + label, inputAntiMagic, branchExpect);
        var obsIn = new RecordingPlayObject { ChrName = "golden-obs-in", ActorId = idBase + 3, CurrX = 105, CurrY = 100, SoftVersionDateEx = 20240101, ClientTick = 5 };
        var obsOut = new RecordingPlayObject { ChrName = "golden-obs-out", ActorId = idBase + 4, CurrX = 140, CurrY = 100, SoftVersionDateEx = 20240101, ClientTick = 5 };
        if (spec.ObsInXy != null) { obsIn.CurrX = (short)spec.ObsInXy[0]; obsIn.CurrY = (short)spec.ObsInXy[1]; }
        if (spec.ObsOutXy != null) { obsOut.CurrX = (short)spec.ObsOutXy[0]; obsOut.CurrY = (short)spec.ObsOutXy[1]; }
        // **MG-57 P2**：夹具坐标 = **施法前**实际值（推挤会移动受害者 ⇒ 事后读 `CurrX/Y` 会失真）。
        int fixtureCasterX = caster.CurrX, fixtureCasterY = caster.CurrY;
        int fixtureTargetX = target.CurrX, fixtureTargetY = target.CurrY;
        int fixtureObsInX = obsIn.CurrX, fixtureObsInY = obsIn.CurrY;
        int fixtureObsOutX = obsOut.CurrX, fixtureObsOutY = obsOut.CurrY;
        // **MG-57 P2**：受害者**施法前** `Dir`（`CharPushed` 的 `result==0` 会还原到它；
        // oracle 的玩家 ctor 缺省 `Dir=4`，Rust 实体缺省 0 ⇒ 观察支必须把该输入也钉进 golden）。
        int fixtureTargetDirBefore = target.Dir;
        // **观察者是真实玩家**：初始化 HP/MaxHP=100（与 Rust rig 的 `new_player_entity` 默认同源）。
        // 不初始化会让"旁观者自己受击/治疗"的帧（53/31）带 0/0，与实现侧不可比（MG-17 D1）。
        obsIn.WAbil.HP = 100; obsIn.WAbil.MaxHP = 100;
        obsOut.WAbil.HP = 100; obsOut.WAbil.MaxHP = 100;
        if (targetNull)
        {
            // 治愈 self 支：`DoSpell` 首句把 targetObject 置成 playObject，读取的是施法者 WAbil。
            caster.WAbil.MaxHP = (ushort)spec.TargetMaxHp;
            caster.WAbil.HP = (ushort)targetHp;
        }
        int amuletDuraInitial = -1;
        if (spec.ArmRingAmulet)
        {
            // `CheckAmulet`（`MagicBase.cs:47`）读 `SystemShare.ItemSystem` 的 StdMode/Shape：
            // 离线进程给最小物品系统 + ArmRingl 护身符（Dura=600 ⇒ Round(600/100)=6 ≥ 1）。
            SystemModule.SystemShare.ItemSystem = new M2Server.Items.GameItemSystem();
            SystemModule.SystemShare.ItemSystem.AddItem(new OpenMir2.Data.StdItem
            {
                Name = "护身符",
                StdMode = 25,
                Shape = (byte)spec.AmuletShape,
            });
            caster.UseItems[OpenMir2.Enums.ItemLocation.ArmRingl] = new OpenMir2.Packets.ClientPackets.UserItem
            {
                MakeIndex = 1,
                Index = 1,
                Dura = (ushort)(spec.AmuletDura ?? 600),
                DuraMax = 1000,
            };
            // **MG-50**：护身符 `Dura` 的**初始**值（`UseAmulet` 会在施法里扣减/销毁，事后读不可用）。
            amuletDuraInitial = caster.UseItems[OpenMir2.Enums.ItemLocation.ArmRingl].Dura;
            if (spec.AmuletDura.HasValue && amuletDuraInitial != spec.AmuletDura.Value)
            {
                throw new FixtureContractViolation(
                    "[" + spec.Key + "] amulet_dura=" + amuletDuraInitial + " != 声明的 " + spec.AmuletDura.Value);
            }
        }
        var recorder = new RecordingNetChannel();
        M2Server.M2Share.NetChannel = recorder;
        foreach (var p in new[] { caster, target, obsIn, obsOut })
        {
            p.GateIdx = p.ActorId;
            p.FixedHideMode = false;
            p.ObMode = false;
            p.Envir = env;
            SystemModule.SystemShare.ActorMgr.Add(p);
            env.AddMapObject(p.CurrX, p.CurrY, SystemModule.Data.CellType.Play, p.ActorId, p);
        }
        if (spec.HitSpeed > 0)
        {
            foreach (var p in new[] { caster, target, obsIn })
            {
                p.HitSpeed = (ushort)spec.HitSpeed; // `CharacterObject.HitSpeed`（657 series 的来源）
            }
        }
        // **非玩家对象（v1 被动）**：进世界并参与所有对象类型门/筛选；**不 Operate**
        // ⇒ 只记录"它们收到了哪些 RM"（对象选择面）。效果面由目标自己 Operate 时产生，属 v2。
        var monsters = new List<RecordingMonster>();
        foreach (var ms in spec.Monsters)
        {
            var mo = new RecordingMonster
            {
                ChrName = "golden-mon" + monsters.Count,
                ActorId = idBase + 10 + monsters.Count,
                CurrX = (short)ms.X,
                CurrY = (short)ms.Y,
            };
            mo.Race = (byte)ms.Race;
            mo.Abil.Level = (byte)ms.Level;
            mo.WAbil.HP = (ushort)ms.Hp;
            mo.WAbil.MaxHP = (ushort)ms.Hp;
            mo.LifeAttrib = (byte)ms.LifeAttrib;
            mo.SuperMan = ms.SuperMan;
            // 卡 §39：命中门 `Random(SpeedPoint)` / 护甲 `WAbil.AC` / 收集期 `Death` 过滤。
            mo.SpeedPoint = (byte)ms.SpeedPoint;
            mo.WAbil.AC = (ushort)ms.AC;
            mo.AntiMagic = (ushort)ms.AntiMagic;
            mo.Death = ms.Death;
            // **MG-34**：诱惑夹具字段（缺省零变化：NoTame/Animal/CanReAlive 均 ctor 缺省 false）。
            mo.NoTame = ms.NoTame;
            mo.Animal = ms.Animal;
            mo.CanReAlive = ms.CanReAlive;
            if (ms.WalkSpeed >= 0) mo.WalkSpeed = ms.WalkSpeed;
            if (ms.NextHitTime >= 0) mo.NextHitTime = ms.NextHitTime;
            if (ms.MonGenActive >= 0)
            {
                mo.MonGen = new SystemModule.Data.MonGenInfo { ActiveCount = (ushort)ms.MonGenActive };
            }
            mo.FixedHideMode = false;
            mo.ObMode = false;
            mo.Envir = env;
            SystemModule.SystemShare.ActorMgr.Add(mo);
            env.AddMapObject(mo.CurrX, mo.CurrY, SystemModule.Data.CellType.Monster, mo.ActorId, mo);
            monsters.Add(mo);
        }
        // `Master`/`TargetCret` 用**本列表的 1 基序号**解析（0 = 无）；越界即配置错误。
        foreach (var (ms, i) in spec.Monsters.Select((v, i) => (v, i)))
        {
            if (ms.MasterCaster)
            {
                monsters[i].Master = caster;
            }
            else if (ms.Master != 0)
            {
                if (ms.Master < 1 || ms.Master > monsters.Count)
                {
                    throw new FixtureContractViolation($"[{spec.Key}] MonsterSpec.Master={ms.Master} 超出 {monsters.Count} 只");
                }
                monsters[i].Master = monsters[ms.Master - 1];
            }
            if (ms.TargetCret != 0)
            {
                if (ms.TargetCret < 1 || ms.TargetCret > monsters.Count)
                {
                    throw new FixtureContractViolation($"[{spec.Key}] MonsterSpec.TargetCret={ms.TargetCret} 超出 {monsters.Count} 只");
                }
                monsters[i].TargetCret = monsters[ms.TargetCret - 1];
            }
            else if (ms.TargetCretCaster)
            {
                // **MG-32**：`TargetCret == baseObject` 分支；golden 侧记 sentinel -1。
                monsters[i].TargetCret = caster;
            }
        }

        // **MG-44**：可选"怪物目标"（`TargetMonster` 1 基；0 = 玩家目标 ⇒ 既有件零变化）。
        SystemModule.Actors.IActor spellTarget = target;
        RecordingMonster targetMonsterObj = null;
        int? monsterHpBefore = null;
        int? monsterHpAfter = null;
        if (spec.TargetMonster > 0)
        {
            if (spec.TargetMonster > monsters.Count)
            {
                throw new FixtureContractViolation($"[{spec.Key}] TargetMonster={spec.TargetMonster} 越界（怪物数 {monsters.Count}）");
            }
            targetMonsterObj = monsters[spec.TargetMonster - 1];
            // `MagTurnUndead`（`MagicManager.cs:830-834`）：`!SuperMan`、`LifeAttrib==LA_UNDEAD`、
            // `Race>=Animal` —— 三条缺一不可。**20 诱惑**的派发门不同（`!target.Animal`，
            // `MagicManager.cs:395`）⇒ 20 跳过圣言三条件、且**不得**把 Animal 强置 true。
            if (spec.MagicId != 20)
            {
                // `TargetMonsterUndead=false` 只放宽 `LifeAttrib == LA_UNDEAD` 一条；
                // `Race >= Animal` 与 `!SuperMan` 仍是前置（非不死分支的伤害面同样要求动物目标）。
                if (targetMonsterObj.Race < OpenMir2.Enums.ActorRace.Animal
                    || (spec.TargetMonsterUndead && targetMonsterObj.LifeAttrib != Grobal2.LA_UNDEAD)
                    || (targetMonsterObj.SuperMan && !spec.TargetMonsterAllowSuperMan))
                {
                    throw new FixtureContractViolation(
                        $"[{spec.Key}] TargetMonster 不满足圣言条件：race={targetMonsterObj.Race} "
                        + $"life_attrib={targetMonsterObj.LifeAttrib} super_man={targetMonsterObj.SuperMan}");
                }
            }
            // **`Animal = true`（生产形态）**：`RecordingMonster` 直接 new 出来，`BaseObject.Animal`
            // 保持 ctor 缺省 false；生产怪物由 MonsterInfo 装载时会设 true，而 Rust `place_monsters`
            // 恒写 `ai.animal = true`。不在这条分支补上，`AnimalObject.Struck` 的
            // `if (Animal) MeatQuality -= Random(300)` **少一抽**，整条 RNG 流错位（实测 lv43）。
            // 只在 `TargetMonster > 0` 分支做 ⇒ 其它 stem 零变化。
            if (spec.MagicId != 20)
            {
                targetMonsterObj.Animal = true;
            }
            spellTarget = targetMonsterObj;
            monsterHpBefore = targetMonsterObj.WAbil.HP;
        }

        // **卡 magic-push-batch（MG-16/57）**：`MagPushArround` 遍历 `playObject.VisibleActors`
        // （`MagicManager.cs:27`）。离线装置不跑 AOI ⇒ 表恒空 ⇒ 循环零次。这里按 `spec.MakeVisible`
        // 显式回填（`PlayObject.UpdateVisibleGay`：不存在则 `VisibleActors.Add(VisibleFlag.Show)`）。
        // **只在 MakeVisible 的 spec 里做** ⇒ 其它 stem 零变化（收件人集合不变）。
        if (spec.MakeVisible)
        {
            // **MG-57 P2**：按**格扫描序**（x 升序、同 x 再 y 升序）逐个 `UpdateVisibleGay`——
            // 生产 `SearchViewRange` 就是这样填 `VisibleActors` 的，`MagPushArround` 的遍历序
            // 直接决定多目标的抽数/帧序。既有摆位（target x=101 在 obs_in x=105 之前）排序后不变
            // ⇒ 既有 golden 逐字节不变。
            var visibleOrder = new System.Collections.Generic.List<SystemModule.Actors.IActor>
            {
                target,
                obsIn,
            };
            foreach (var mo in monsters)
            {
                visibleOrder.Add(mo);
            }
            visibleOrder.Sort((a, b) =>
            {
                int c = a.CurrX.CompareTo(b.CurrX);
                return c != 0 ? c : a.CurrY.CompareTo(b.CurrY);
            });
            foreach (var obj in visibleOrder)
            {
                caster.UpdateVisibleGay(obj);
            }
        }

        // 卡 §39：怪物 HP 前值（施法/RNG 之前的纯观测，进台账；只对 `groupdeding` 落盘）。
        var monHpBefore = monsters.Select(m => (int)m.WAbil.HP).ToArray();
        // —— 夹具契约（M51）：construction/derived 由 oracle 复算，七道门任一不过 ⇒ FixtureContractViolation（rc=2）。
        var contract = BuildContractInput(spec, target);
        FixtureContract.Validate(contract);

        // **17 夹具：预置 slave + 施法前快照**（cap 支靠预置触发；regen 计数证明是否真生成实体）。
        if (spec.PreSlaveCount > 0)
        {
            caster.SlaveList ??= new System.Collections.Generic.List<SystemModule.Actors.IMonsterActor>();
            for (int k = 0; k < spec.PreSlaveCount; k++)
            {
                // exact-name（SINSU 神兽）才能被 RecallSlave 命中（它只认 ChrName==Config.Dragon|Dragon1）。
                string psName = (spec.PreSlaveName == "pre-slave") ? (spec.PreSlaveName + k) : spec.PreSlaveName;
                if (spec.EnableMaps)
                {
                    // 卡 mapmgr(a)：预置 slave 用 RecordingMonster 镜像/同构 target 构造（harness 手填，非生产 RegenMonsterByName；
                    // 比裸对象完整，避免排水 RM_SPACEMOVE_SHOW 的 GetFeature NPE）。仅 SINSU 走此支。
                    var ps = new RecordingMonster { ChrName = psName, ActorId = idBase + 50 + k, CurrX = 0, CurrY = 0 };
                    ps.Race = 113; ps.Abil.Level = 1;
                    ps.WAbil.HP = 17; ps.WAbil.MaxHP = 17; ps.WAbil.AC = 0;
                    ps.LifeAttrib = 0; ps.SpeedPoint = 0; ps.AntiMagic = 0;
                    ps.Death = false; ps.FixedHideMode = false; ps.ObMode = false;
                    ps.Envir = env;
                    SystemModule.SystemShare.ActorMgr.Add(ps);
                    env.AddMapObject(ps.CurrX, ps.CurrY, SystemModule.Data.CellType.Monster, ps.ActorId, ps);
                    caster.SlaveList.Add(ps);
                }
                else
                {
                    var ps = new M2Server.Monster.MonsterObject { ChrName = psName, ActorId = idBase + 50 + k };
                    ps.CurrX = 0; ps.CurrY = 0; ps.Envir = caster.Envir; // 旧路径（skeleton/fire 等零变化）
                    caster.SlaveList.Add(ps);
                }
            }
        }
        var preSlaveXyBefore = (caster.SlaveList ?? new System.Collections.Generic.List<SystemModule.Actors.IMonsterActor>())
            .Take(spec.PreSlaveCount)
            .Select(x => (object)new { name = x.ChrName, x = (int)x.CurrX, y = (int)x.CurrY }).ToList();
        int slaveCountBefore = caster.SlaveList?.Count ?? 0;
        int regenBefore = OfflineWorldEngine.RegenCalls;
        var original = RngSeam.Capture();
        var rng = RngSeam.Install(seed);
        var um = new OpenMir2.Packets.ClientPackets.UserMagic
        {
            MagIdx = (ushort)spec.MagicId,
            Level = (byte)spec.UserMagicLevel,
            TranPoint = 0,
            Magic = BuildMagicInfo(spec),
        };
        short doX = targetNull ? caster.CurrX : spellTarget.CurrX;
        short doY = targetNull ? caster.CurrY : spellTarget.CurrY;
        // 卡 mapmgr：SINSU cap_hit 回召依赖 RecallSlave→GetFrontPosition 用 caster.Dir；生产施法前 caster 面向施法目标，
        // 故对 EnableMaps 支把 caster 面向设成施法方向（faithful），使回召落点与 Rust(按施法方向)一致。
        if (spec.EnableMaps) caster.Dir = M2Server.M2Share.GetNextDirection(caster.CurrX, caster.CurrY, doX, doY);
        // **MG-55（46 分身）**：生产 `ClientSpellXY` 的 default 支在 `DoSpell` **之前**把
        // `playObject.Dir` 转向点击坐标（`PlayObject.Attack.cs:1269`）；装置直调 `DoSpell`
        // ⇒ 显式照做，`clone.dir` 才与产品路径同源。
        if (spec.PreSpellFaceTarget) caster.Dir = M2Server.M2Share.GetNextDirection(caster.CurrX, caster.CurrY, doX, doY);
        // **MG-36（22 火墙）**：安全区输入（夹具面）与预置事件（GetEvent 去重面）。
        // 二者都必须在 RngSeam.Install **之前**（均不抽随机数；预置走 oracle ctor，
        // 只落格子、不进 EventMgr.Events）。
        bool savedDisableSafeZone = SystemModule.SystemShare.Config.DisableInSafeZoneFireCross;
        if (spec.MagicId == 22 && spec.FireCrossSafeZone)
        {
            env.Flag.SafeArea = true;
            SystemModule.SystemShare.Config.DisableInSafeZoneFireCross = true;
        }
        if (spec.MagicId == 22)
        {
            var cellSet = new HashSet<(int, int)> { (doX, doY - 1), (doX - 1, doY), (doX, doY), (doX + 1, doY), (doX, doY + 1) };
            foreach (var cell in spec.PreEventCells)
            {
                if (!cellSet.Contains(cell))
                {
                    throw new FixtureContractViolation(
                        $"[{spec.Key}/{label}] PreEventCells ({cell.X},{cell.Y}) 不在五格 {string.Join(" ", cellSet)} 内");
                }
                _ = new SystemModule.MagicEvent.Events.FireBurnEvent(
                    caster, (short)cell.X, (short)cell.Y, Grobal2.ET_FIRE, 1, 0);
            }
        }
        bool inSafeZoneAtCast = spec.MagicId == 22 && caster.InSafeZone(env, doX, doY);
        // **MG-34**：20 的 `MagTamming` 读/写 `playObject.SlaveList`（`:725/:779`）；离线 ctor 不建表。
        if (spec.MagicId == 20)
        {
            caster.SlaveList ??= new System.Collections.Generic.List<SystemModule.Actors.IMonsterActor>();
        }
        int trainTpBefore = um.TranPoint;
        int trainLevelBefore = um.Level;
        // `Train` 块要用的前提值（`CheckMagicLevelUp` 的实际下标），在 `SkillTail` 块里赋值。
        int trainNLevel = 0;
        int trainMtUsed = 0;
        // **施法时刻 t0**：`due_ms_rel = DeliveryTime − t0`（调度器口径）。必须取在 DoSpell **之前**——
        // 取在之后会让延迟帧的 due 变成 `1000 − DoSpell 耗时`（Sleep 过冲下落到 900 一类），
        // 那是测量伪影、不是排期。t0 与 `SendSelfDelayMsg` 的 `GetTickCount()` 同源（同一时钟）。
        long castTick = HUtil32.GetTickCount();
        // B21：施法者重算计数清零——只统计本支 `DoSpell` 窗口（覆写见 `RecordingPlayObject`）。
        caster.RecalcAbilitysCalls = 0;
        // B21：记录**施法输入**（`DoSpell` 之前）。golden 的 `caster_sc`/`caster_mc` 与
        // `caster_sc_raw/lo/hi` 都取它；事后 `WAbil` 可能被 `RecalcAbilitys()` 重物化，不得当输入用。
        int casterInputMc = caster.WAbil.MC;
        int casterInputSc = caster.WAbil.SC;
        // **MG-57 P2**：`PushCacheProbe` 把"首扫入口"钉死——受害者 `VisibleHumanList` 必须为空
        // （缓存冷），否则第一次 `CharPushed` 会走"复用（<11）"路径，边界臂的首扫档无法解释。
        if (spec.PushCacheProbe && spellTarget != null && spellTarget.VisibleHumanList.Count != 0)
        {
            throw new FixtureContractViolation(
                $"[{spec.Key}/{label}] PushCacheProbe：受害者 SendRefMsg 缓存非冷（VisibleHumanList.Count={spellTarget.VisibleHumanList.Count}）");
        }
        bool ok = M2Server.Magic.MagicManager.DoSpell(caster, um, doX, doY, targetNull ? null : spellTarget);
        if (spec.MagicId == 22 && spec.FireCrossSafeZone)
        {
            SystemModule.SystemShare.Config.DisableInSafeZoneFireCross = savedDisableSafeZone;
        }
        List<long> draws = new();
        if (spec.RngMode == "freeze_at_dospell")
        {
            // legacy 口径：漏记排水阶段抽取（只作审计件；`legacy_artifact=true`）。
            draws = rng.Draws.ToList();
            RngSeam.Restore(original);
        }
        // **`Rng` = DoSpell + 排水的全部抽取**（种子的 seam 直到排水结束才还原）：
        // 23 的 `RM_MAGSTRUCK` 是**即时**投递（`MagBigExplosion` 直接 `SendMsg`），它的
        // `R(1000)`/MSD/STD 抽发生在排水的 `Operate` 里；若像旧口径那样在 DoSpell 后就还原，
        // 这些抽会落到未播种的全局 RNG ⇒ 金标准不可复现、Rust 侧（施法段内同步结算）无法对齐。
        // 既有 golden 不受影响（未重生成）。

        // 与火球同口径：排水抽空为止（上界 8），上界命中可见。
        // 等到**最长**的自延迟到期：治愈的 RM_MAGHEALING=800ms、共享尾 RM_MAGIC_LVEXP=1000ms、
        // 44 的 RM_DELAYPUSHED=600ms ⇒ 1200ms（否则 `NotDue` 会带墙钟字段、两次跑不一致）。
        // 28 的 `RM_DOOPENHEALTH`=1500ms ⇒ 该支 `WaitMs=1600`（卡 §25(a)：必须跨过自延迟，
        // 否则 `MakeOpenHealth`/`SM_OPENHEALTH(1100)` 不会发生）。
        const int MaxDrainPasses = 8;
        int hpBefore = target.WAbil.HP;
        var pending = new List<object>();
        var byWho = new (string, RecordingPlayObject)[] { ("caster", caster), ("target", target), ("obs_in", obsIn), ("obs_out", obsOut) };
        var rmAcc = new Dictionary<string, List<object>>();
        foreach (var (who, _) in byWho) rmAcc[who] = new List<object>();
        // 帧 `due_ms_rel` 的基准 = **DoSpell 之前**捕获的 `castTick`（见上方 t0 注释）。
        int drainPasses = 0;
        bool drainPassesHitCap = false;
        // **MG-38（2026-11-26）**：`MonsterSpec.OperateStruck` 的怪物早期操作结果（在 `if` 内填充）。
        Dictionary<int, List<(SystemModule.Data.SendMessage Msg, byte Prio)>> monOperated = null;
        Dictionary<int, int> monHpBeforeOperate = null;
        if (spec.SecondCastAtMs.HasValue && spec.StepDrainMs <= 0)
        {
            throw new FixtureContractViolation(
                $"[{spec.Key}/{label}] SecondCastAtMs 需要 StepDrainMs > 0（非步进支没有\"步\"可施第二次法，"
                + "静默忽略会把'没有第二次施法'的 golden 当成功）");
        }
        if (spec.StepDrainMs > 0)
        {
            // **步进排水**（见 `MagicSpec.StepDrainMs` 说明）：每步只处理**已到期**的消息，
            // 未到期的按原优先级放回队列；RM 账本按消息对象去重（同一消息跨步只记一次）。
            void DrainOnce(bool[] operatedAny, long stepMs)
            {
                foreach (var (who, obj) in byWho)
                {
                    // **B17（2026-11-26）**：opt-in 的生产泵语义——`TryPeek` 队首，未到期
                    // ⇒ `Lowest` 重入队 + 结束本 actor 本次泵（不越过未到期队首）。
                    if (spec.FaithfulPump)
                    {
                        while (true)
                        {
                            var got = obj.FaithfulPumpOne(HUtil32.GetTickCount());
                            if (got == null) break;
                            var (m, prio) = got.Value;
                            rmAcc[who].Add(new
                            {
                                ident = m.wIdent, wParam = m.wParam, p1 = m.nParam1, p2 = m.nParam2, p3 = m.nParam3,
                                msg = m.Buff ?? "", late = m.LateDelivery, delivery = m.DeliveryTime, priority = prio,
                            });
                            RecordingNetChannel.CurrentDueMs = m.LateDelivery ? Math.Max(0, m.DeliveryTime - castTick) : stepMs;
                            obj.OperatePublic(m);
                            RecordingNetChannel.CurrentDueMs = 0;
                            operatedAny[0] = true;
                        }
                        continue;
                    }
                    var raw = obj.DrainRaw();
                    int now = HUtil32.GetTickCount();
                    int stopAt = raw.Count;
                    for (int i = 0; i < raw.Count; i++)
                    {
                        var (m, prio) = raw[i];
                        if (!m.LateDelivery || m.DeliveryTime <= now)
                        {
                            // RM 账本 = 全部「成功入队并被处理」的消息（每条只记一次）。
                            rmAcc[who].Add(new
                            {
                                ident = m.wIdent,
                                wParam = m.wParam,
                                p1 = m.nParam1,
                                p2 = m.nParam2,
                                p3 = m.nParam3,
                                msg = m.Buff ?? "",
                                late = m.LateDelivery,
                                delivery = m.DeliveryTime,
                                priority = prio,
                            });
                            // 延迟消息 = 其到期时刻；**非延迟消息 = 当前步进时刻**（它在本次
                            // Operate 里被处理，客户端可见时刻就是这一步），与 Rust 台账的
                            // `due_ms_rel = elapsed` 同口径。
                            RecordingNetChannel.CurrentDueMs =
                                m.LateDelivery ? Math.Max(0, m.DeliveryTime - castTick) : stepMs;
                            obj.OperatePublic(m);
                            RecordingNetChannel.CurrentDueMs = 0;
                            operatedAny[0] = true;
                        }
                        else
                        {
                            // **B17 对齐生产**：遇第一条未到期即停（`GetMessage`：`Enqueue(Lowest)` + `return false`）。
                            stopAt = i;
                            break;
                        }
                    }
                    for (int i = stopAt; i < raw.Count; i++) obj.PushBack(raw[i].Msg, raw[i].Prio);
                }
            }
            // **施法批（t=0）**：与 Rust 台账"施法段 drain（elapsed=0）"同口径 ⇒ 帧 `due_ms_rel=0`。
            // **排到空（多趟）**：`SendMsg` 是 `LateDelivery=false` 的即时消息链（受害者的
            // `RM_MAGSTRUCK` → `RM_STRUCK` 扇出），t=0 全部到期；单趟会把链式后继推到 100ms 步
            // （`due=100`），既与 Rust 同拍出帧（`due=0`）不一致，也与"施法批 due=0"的建模约定矛盾。
            // 与下面的步进循环同语义：每步排到空为止（上界同 `MaxDrainPasses`，命中记 cap）。
            for (int pass = 0; pass < MaxDrainPasses; pass++)
            {
                var operatedAny = new[] { false };
                DrainOnce(operatedAny, 0);
                // **B17 FaithfulPump**：生产每 tick 只泵一次（泵内自己循环到「未到期队首」为止）；
                // 再跑一轮只会**多旋转一次堆**、改变等键出队序（2026-11-26 实测：640 due 1000 vs 1100）。
                if (spec.FaithfulPump) break;
                if (!operatedAny[0]) break;
                if (pass == MaxDrainPasses - 1) drainPassesHitCap = true;
            }
            // **MG-48（2026-11-26）**：`OperatePerStep` 支——`OperateStruck` 怪在**每步**结算
            // 自己的队列（生产：怪物 `Run()` 每 tick 泵一次；延迟受击 `RM_MAGSTRUCK` 到点的
            // 那一步就结算、随即 `SendRefMsg(RM_STRUCK_MAG)`/`RefNameColor` 向玩家出帧）。
            // 与缺省路径（窗口结束后一次性结算、due=0）**互斥**；帧的 `CurrentDueMs` 取本步
            // `elapsed`，与同一步玩家帧同口径。缺省支零变化。
            void OperateStruckPerStep(long stepMs)
            {
                if (monsters.Count == 0) return;
                monOperated ??= new Dictionary<int, List<(SystemModule.Data.SendMessage Msg, byte Prio)>>();
                monHpBeforeOperate ??= new Dictionary<int, int>();
                for (int mi = 0; mi < monsters.Count; mi++)
                {
                    if (!spec.Monsters[mi].OperateStruck) continue;
                    if (!monHpBeforeOperate.ContainsKey(mi))
                    {
                        monHpBeforeOperate[mi] = monsters[mi].WAbil.HP;
                    }
                    if (!monOperated.TryGetValue(mi, out var mlist))
                    {
                        mlist = new List<(SystemModule.Data.SendMessage Msg, byte Prio)>();
                        monOperated[mi] = mlist;
                    }
                    for (int pass = 0; pass < MaxDrainPasses; pass++)
                    {
                        var raw = monsters[mi].DrainRaw();
                        if (raw.Count == 0) break;
                        foreach (var (m, prio) in raw)
                        {
                            mlist.Add((m, prio));
                            RecordingNetChannel.CurrentDueMs =
                                m.LateDelivery ? Math.Max(0, m.DeliveryTime - castTick) : stepMs;
                            monsters[mi].OperatePublic(m);
                            RecordingNetChannel.CurrentDueMs = 0;
                        }
                    }
                }
                // 玩家侧再排到空（怪物 RefMsg 的收件人）。
                for (int pass = 0; pass < MaxDrainPasses; pass++)
                {
                    var operatedAny = new[] { false };
                    DrainOnce(operatedAny, stepMs);
                    if (!operatedAny[0]) break;
                }
            }
            int elapsed = 0;
            while (elapsed < spec.WaitMs)
            {
                int step = Math.Min(spec.StepDrainMs, spec.WaitMs - elapsed);
                ClockSeam.Advance(step);
                elapsed += step;
                // **MG-57**：第二次施法——生产里第二次 `CM_SPELL` 在该 tick 的消息循环内被处理 ⇒
                // 在**本步排水之前**再跑一次 `DoSpell`；帧的 `CurrentDueMs` 取本步 `elapsed`
                // （与 `DrainOnce` 的非延迟消息同口径），否则会落 0。
                if (spec.SecondCastAtMs.HasValue && spec.SecondCastAtMs.Value == elapsed)
                {
                    // 第二次施法按**目标当前格**点击（真实客户端每次 `CM_SPELL` 带当时的坐标；
                    // 复用首施法的 `doX/doY` 会让"已移动目标 + 陈旧坐标"这种非生产形态进 golden）。
                    short scx = targetNull ? caster.CurrX : spellTarget.CurrX;
                    short scy = targetNull ? caster.CurrY : spellTarget.CurrY;
                    RecordingNetChannel.CurrentDueMs = elapsed;
                    _ = M2Server.Magic.MagicManager.DoSpell(caster, um, scx, scy, targetNull ? null : spellTarget);
                    RecordingNetChannel.CurrentDueMs = 0;
                }
                int stepPasses = 0;
                for (int pass = 0; pass < MaxDrainPasses; pass++)
                {
                    var operatedAny = new[] { false };
                    DrainOnce(operatedAny, elapsed);
                    stepPasses = pass + 1;
                    // **B17 FaithfulPump**：每 tick 只泵一次（理由同上，避免多余的堆旋转）。
                    if (spec.FaithfulPump) break;
                    if (!operatedAny[0]) break;
                    if (pass == MaxDrainPasses - 1) drainPassesHitCap = true;
                }
                drainPasses = Math.Max(drainPasses, stepPasses);
                if (spec.OperatePerStep) OperateStruckPerStep(elapsed);
            }
            // **MG-38（2026-11-26）**：`OperateStruck` 的怪物在**玩家步进排水之后、RNG 快照之前**
            // 按生产泵语义结算自己收到的 RM（`BaseObject.Base.cs:618-686`：`R(1000)`/MSD/
            // `StruckDamage`/`SendRefMsg(RM_STRUCK_MAG)`/`SendMsg(RM_STRUCK)`）。随后把玩家侧
            // 再排一轮——怪物的 `SendRefMsg`（每格重扫）与 `BreakHolySeizeMode→RefNameColor`
            // 会把 8103/656 投给旁观者。**只有 opt-in 的 case 走这里**；其余 stem 零变化。
            if (!spec.OperatePerStep && monsters.Count > 0 && spec.Monsters.Any(m => m.OperateStruck))
            {
                monOperated = new Dictionary<int, List<(SystemModule.Data.SendMessage Msg, byte Prio)>>();
                monHpBeforeOperate = new Dictionary<int, int>();
                for (int mi = 0; mi < monsters.Count; mi++)
                {
                    if (!spec.Monsters[mi].OperateStruck) continue;
                    monHpBeforeOperate[mi] = monsters[mi].WAbil.HP;
                    var mlist = new List<(SystemModule.Data.SendMessage Msg, byte Prio)>();
                    for (int pass = 0; pass < MaxDrainPasses; pass++)
                    {
                        var raw = monsters[mi].DrainRaw();
                        if (raw.Count == 0) break;
                        foreach (var (m, prio) in raw)
                        {
                            mlist.Add((m, prio));
                            monsters[mi].OperatePublic(m);
                        }
                    }
                    monOperated[mi] = mlist;
                }
                // 玩家侧再排到空（怪物 RefMsg 的收件人）。
                for (int pass = 0; pass < MaxDrainPasses; pass++)
                {
                    var operatedAny = new[] { false };
                    DrainOnce(operatedAny, 0);
                    if (!operatedAny[0]) break;
                }
            }
            // 兜底：窗口结束时仍未到期的消息记入 `NotDue`（不执行、不再入队）——正常情况下为空。
            foreach (var (who, obj) in byWho)
            {
                foreach (var (m, prio) in obj.DrainRaw())
                {
                    pending.Add(new
                    {
                        who,
                        ident = m.wIdent,
                        delivery = m.DeliveryTime,
                        now = HUtil32.GetTickCount(),
                    });
                }
            }
        }
        else
        {
            ClockSeam.Advance(spec.WaitMs);
            for (int pass = 0; pass < MaxDrainPasses; pass++)
            {
                bool drainedAny = false;
                foreach (var (who, obj) in byWho)
                {
                    var raw = obj.DrainRaw();
                    if (raw.Count > 0) drainedAny = true;
                    var rm = raw.Select(m => (object)new
                    {
                        ident = m.Msg.wIdent,
                        wParam = m.Msg.wParam,
                        p1 = m.Msg.nParam1,
                        p2 = m.Msg.nParam2,
                        p3 = m.Msg.nParam3,
                        msg = m.Msg.Buff ?? "",
                        late = m.Msg.LateDelivery,
                        delivery = m.Msg.DeliveryTime,
                        priority = m.Prio,
                    }).ToList();
                    int now = HUtil32.GetTickCount();
                    foreach (var m in raw)
                    {
                        if (!m.Msg.LateDelivery || m.Msg.DeliveryTime <= now)
                        {
                            RecordingNetChannel.CurrentDueMs =
                                m.Msg.LateDelivery ? Math.Max(0, m.Msg.DeliveryTime - castTick) : 0;
                            obj.OperatePublic(m.Msg);
                            RecordingNetChannel.CurrentDueMs = 0;
                        }
                        else pending.Add(new { who, ident = m.Msg.wIdent, delivery = m.Msg.DeliveryTime, now });
                    }
                    rmAcc[who].AddRange(rm);
                }
                drainPasses = pass + 1;
                if (!drainedAny) break;
                if (pass == MaxDrainPasses - 1) drainPassesHitCap = true;
            }
        }
        if (spec.SkillTail)
        {
            // `CheckMagicLevelUp`（`PlayObject.cs:5200-5209`）用的下标是**升级前** Level 推出的 `nLevel`
            // （Level<4 且 TrainLv>=Level 时 = Level，否则 0）——**不是恒 [0]**，也不该用升级后的 Level
            // （升级支执行后 `um.Level` 已 +1 ⇒ 会核到没被用的那一项，断言变永真）。
            int nLevel = (trainLevelBefore < 4 && um.Magic.TrainLv >= trainLevelBefore) ? trainLevelBefore : 0;
            int mtUsed = (um.Magic.MaxTrain != null && um.Magic.MaxTrain.Length > nLevel) ? um.Magic.MaxTrain[nLevel] : 0;
            trainNLevel = nLevel;
            trainMtUsed = mtUsed;
            if (spec.TailUpgrade)
            {
                // 800ms 升级支的前提（`PlayObject.cs:5210-5216`）：门 `MaxTrain[nLevel] <= TranPoint`
                // 成立 ⇒ `TranPoint -= MaxTrain`、`Level++`。升级后 `TranPoint = 前值 + 抽点 − MaxTrain`，
                // 故 `TranPoint >= 0` 是「门确实成立」的等价观测（不重算、不拿实现输出自证）。
                if (!(um.Magic.TrainLv > trainLevelBefore))
                {
                    throw new FixtureContractViolation(
                        $"[{spec.Key}/{label}] 升级支前提失败：TrainLv={um.Magic.TrainLv} 必须 > 升级前 Level={trainLevelBefore}");
                }
                if (um.Level != trainLevelBefore + 1)
                {
                    throw new FixtureContractViolation(
                        $"[{spec.Key}/{label}] 升级支前提失败：Level {trainLevelBefore} -> {um.Level}（应恰好 +1；"
                        + $"否则 CheckMagicLevelUp 未走升级臂，max_train0={mtUsed}）");
                }
                if (um.TranPoint < 0)
                {
                    throw new FixtureContractViolation(
                        $"[{spec.Key}/{label}] 升级支前提失败：升级后 TranPoint={um.TranPoint} < 0 ⇒ MaxTrain 超过门值");
                }
            }
            else
            {
                if (!(mtUsed > um.TranPoint))
                {
                    throw new FixtureContractViolation(
                        $"[{spec.Key}/{label}] 夹具前提失败：MaxTrain[nLevel={nLevel}]={mtUsed} 必须 > TranPoint={um.TranPoint}"
                        + "（否则 CheckMagicLevelUp 成功 ⇒ 640 走 800ms 升级支，不在本批 scope）");
                }
                if (!(um.Magic.TrainLv > um.Level))
                {
                    throw new FixtureContractViolation(
                        $"[{spec.Key}/{label}] 夹具前提失败：TrainLv={um.Magic.TrainLv} 必须 > Level={um.Level}"
                        + "（否则升级门因 TrainLv 而恒假，前提断言会变成永真）");
                }
                if (um.Level != trainLevelBefore)
                {
                    throw new FixtureContractViolation(
                        $"[{spec.Key}/{label}] 意外升级：Level {trainLevelBefore} -> {um.Level}（本支应为未升级 1000ms）");
                }
            }
        }
        Console.Error.WriteLine($"DRAIN_PASSES {spec.Key}/{label} passes={drainPasses} hit_cap={drainPassesHitCap}");
        // 排水后、`RngSeam.Restore` 前取累计数（`Restore` 不清零该实例计数，但按约定顺序取值）。
        long totalDraws = rng.TotalDraws;
        if (spec.RngMode != "freeze_at_dospell")
        {
            draws = rng.Draws.ToList();
            RngSeam.Restore(original);
        }
        EnsureCoverAccounting(spec.RngMode, draws.Count, totalDraws, "runmagic/" + spec.Key + "/" + label);
        CheckAntiMagicBranch("runmagic/" + spec.Key + "/" + label, draws, seed, branchExpect);
        string seedReasonText = branchExpect != null && draws.Count > 0
            ? BuildSeedReason(draws[0], seed, branchExpect)
            : null;
        // **MG-12/40**：恢复 tick——把虚拟钟设到 fixture 给的绝对 ms，对**被治疗者**调一次生产
        // `Run()`（oracle `:70-135`），逐 tick 记 `{ms, hp, inc_healing}`。无 wall-clock、无 Sleep。
        List<object> healTicks = null;
        int healTickBase = target.IncHealthSpellTickPublic;
        // **MG-12/40**：恢复门读的是 `WAbil.Level`（不是 `Abil.Level`）；逐支记录，驱动据此对齐
        // 会话 `wabil.level`（否则 `dw = 600 - min(400, Level*10)` 两侧不同）。
        if (spec.HealTicksMs != null && spec.HealCasterWabilLevel.HasValue)
        {
            caster.WAbil.Level = (byte)spec.HealCasterWabilLevel.Value;
        }
        int healLevelTarget = target.WAbil.Level;
        int healLevelCaster = caster.WAbil.Level;
        if (spec.HealTicksMs != null && spec.HealCasterWabilLevel.HasValue
            && healLevelCaster != spec.HealCasterWabilLevel.Value)
        {
            throw new FixtureContractViolation(
                "[" + spec.Key + "/" + label + "] heal_level_caster=" + healLevelCaster
                + " != 声明的 " + spec.HealCasterWabilLevel.Value + "（恢复门会读错 dw）");
        }
        if (spec.HealTicksMs != null && spec.HealTicksMs.Length > 0)
        {
            healTicks = new List<object>();
            foreach (var offset in spec.HealTicksMs)
            {
                // `HealTicksMs` 是**相对施法时刻的偏移**（与 `due_ms_rel` 同口径）；绝对虚拟时刻 =
                // `castTick + offset`（castTick 在 `DoSpell` 之前取，同一虚拟钟）。
                long when = castTick + offset;
                ClockSeam.Set((int)when);
                AssertRunInert(target, spec.Key + "/" + label + "@" + offset);
                target.Run();
                object row;
                if (spec.HealTicksBoth)
                {
                    AssertRunInert(caster, spec.Key + "/" + label + "/caster@" + offset);
                    caster.Run();
                    row = new
                    {
                        ms = offset,
                        hp = (int)target.WAbil.HP,
                        inc_healing = (int)target.IncHealing,
                        hp_caster = (int)caster.WAbil.HP,
                        inc_healing_caster = (int)caster.IncHealing,
                    };
                }
                else
                {
                    row = new
                    {
                        ms = offset,
                        hp = (int)target.WAbil.HP,
                        inc_healing = (int)target.IncHealing,
                    };
                }
                healTicks.Add(row);
            }
        }
        int hpAfter = target.WAbil.HP;
        if (targetMonsterObj != null) monsterHpAfter = targetMonsterObj.WAbil.HP;
        // 卡 36：`AttPowerUp` 同步写 `ExtraAbil[0]`（目标=self ⇒ 施法者）⇒ 直接观测，非自证。
        int? casterExtra0 = spec.CasterLoginOrderAbil ? (int)caster.ExtraAbil[0] : (int?)null;
        // **B21 语义门（fail-closed ⇒ rc=2、不落 golden）**：`CasterLoginOrderAbil` stem 若在 `DoSpell`
        // 内对施法者触发过 `RecalcAbilitys()`（**运行期计数**，不靠静态清单 ⇒ 新增来源自动覆盖；
        // 当前唯一来源：`SKILL_UENHANCER`(36) 的 `AttPowerUp`，`MagicManager.cs:625`，target=None
        // ⇒ target=self），只断言**反方向**：`Abil` 自登录序派生之后不得再被改写；
        // **不得**断言事后态 `WAbil == 输入`（等级派生本来就允许不同，见上方 B21 说明）。
        if (spec.CasterLoginOrderAbil && caster.RecalcAbilitysCalls > 0)
        {
            FixtureContract.ValidateCasterAbilitySemantics(
                $"runmagic/{spec.Key}/{label}", loginAbilSc, loginAbilMc, caster.Abil.SC, caster.Abil.MC);
        }
        // 卡 §39：怪物 HP 后值（施法段同步 `StruckDamage` 的结果观测；不喂回驱动输入）。
        var monHpAfter = monsters.Select(m => (int)m.WAbil.HP).ToArray();
        // **三态计时器的术后值必须由"门结果 + 术前值"独立推出**（不拿实现输出自证）：
        // 门开 ⇒ 非零项置 1、零项不动；门关 ⇒ 一律不动。首抽即门骰（本支此前零抽）。
        int[] statusAfter = new[]
        {
            (int)target.StatusTimeArr[OpenMir2.Consts.PoisonState.DECHEALTH],
            (int)target.StatusTimeArr[OpenMir2.Consts.PoisonState.DAMAGEARMOR],
            (int)target.StatusTimeArr[OpenMir2.Consts.PoisonState.STONE],
        };
        if (spec.PoisonStates != null)
        {
            bool gateOpen = draws.Count > 0 && draws[0] - (spec.UserMagicLevel + 1) < 0;
            int[] expectAfter = new int[3];
            for (int i = 0; i < 3; i++)
            {
                expectAfter[i] = gateOpen && statusBefore[i] != 0 ? 1 : statusBefore[i];
            }
            if (!statusAfter.SequenceEqual(expectAfter))
            {
                throw new FixtureContractViolation(
                    $"[runmagic/{spec.Key}/{label}] status_time_arr 术后值不可由门结果推导："
                    + $"before=[{string.Join(",", statusBefore)}] after=[{string.Join(",", statusAfter)}] "
                    + $"gate_open={gateOpen}（期望 after=[{string.Join(",", expectAfter)}]）");
            }
        }
        var receivers = new List<ReceiverResult>();
        foreach (var (who, obj) in byWho)
        {
            var rr = new ReceiverResult { Who = who, Rm = rmAcc[who], Sent = ParseFrames(recorder.Buffers, obj.ActorId) };
            // **MG-44（2026-11-26）**：`ObserveState` 时落 `caster.TargetCret`（`:841 SetTargetCreat` 面；
            // `0` = 无目标）。缺省不落盘 ⇒ 旧 golden 零变化。
            if (spec.ObserveState && who == "caster")
            {
                rr.TargetCret = caster.TargetCret == null ? 0 : caster.TargetCret.ActorId;
            }
            receivers.Add(rr);
        }
        // **§39 逐目标抽值**（仅 `groupdeding`）：按 objectList 收集序走查 `draws`，记录每只怪的
        // **命中门**抽值与序号（`Random(0)` 也占位）。死怪在收集期被过滤 ⇒ 无条目（`GateDraw=null`）。
        // 走查同时是一条自洽牙：抽数必须恰好耗尽 `draws`，否则夹具假设（HAM_PEACE / DC 区间 0）已被破坏。
        Dictionary<int, (int GateIdx, long GateVal, bool Hit)> monGate = null;
        if (spec.Key == "groupdeding" && spec.CasterHitPoint >= 0)
        {
            monGate = new Dictionary<int, (int, long, bool)>();
            int di = 0;
            int hits = 0;
            var order = spec.Monsters.Select((m, idx) => (m, idx))
                .Where(t => !t.m.Death)
                .OrderBy(t => t.m.X).ThenBy(t => t.m.Y);
            foreach (var (m, idx) in order)
            {
                if (di + 2 >= draws.Count) throw new FixtureContractViolation($"[{spec.Key}/{label}] 抽值走查越界（mon{idx} 攻击段）");
                di += 2; // PlayObject.GetAttackPower：Random(power+1)[区间0] + Luck 门 Random(10)
                int gateIdx = di;
                if (di >= draws.Count) throw new FixtureContractViolation($"[{spec.Key}/{label}] 抽值走查越界（mon{idx} 命中门）");
                long gate = draws[gateIdx];
                di += 1; // Random(SpeedPoint)
                bool hit = gate < spec.CasterHitPoint;
                if (hit)
                {
                    hits++;
                    int acLo = m.AC & 0xFF, acHi = (m.AC >> 8) & 0xFF;
                    if (di >= draws.Count) throw new FixtureContractViolation($"[{spec.Key}/{label}] 抽值走查越界（mon{idx} 护甲骰）");
                    long r = draws[di]; // Random(|hi-lo|+1)
                    di += 1;
                    if (acLo + r > 0) di += 1; // Random(nRnd)
                }
                monGate[idx] = (gateIdx, gate, hit);
            }
            // 共享尾训练抽（`MagicManager.cs:659-666`）：`boTrain`（有任一非死怪 ⇒ 至少一只
            // proper 动物）且 `Level<3`、`TrainLevel[Level] <= caster.Abil.Level`（默认 0<=30）
            // ⇒ `TrainSkill(..., Random(3)+1)` 恰一次抽，效果体之后。
            if (spec.Monsters.Any(m => !m.Death)) di += 1;
            // 交付段（200ms RM_DELAYMAGIC ⇒ 目标侧 `GetMagStruckDamage`）：每次命中/可投递的
            // RM_DELAYMAGIC 抽一次 `Random(|HiMac-LoMac|+1)`（MAC=0 ⇒ `Random(1)`）。
            // 这些抽在 `draws` 的**尾部**（做法与 `Rng` 记 cast+drain 同源）；逐目标走查须计入。
            di += hits;
            if (di != draws.Count)
            {
                throw new FixtureContractViolation(
                    $"[{spec.Key}/{label}] 逐目标抽值走查未耗尽：consumed={di} recorded={draws.Count}");
            }
        }
        // **v1 被动怪物**：只把它收到的 RM 记进台账（对象选择面）；不 Operate ⇒ `Sent` 恒空。
        // **MG-38（2026-11-26）**：`MonsterSpec.OperateStruck=true` 的 case 改为**像真怪一样**
        // 逐条 `Operate`（生产泵语义；上界同 `MaxDrainPasses`），并把**本次操作过的每条 RM**
        // 与操作产生的新 RM 一并记进 `Rm`（顺序 = 出队/产生序）。
        foreach (var (mo, i) in monsters.Select((v, i) => (v, i)))
        {
            // `OperateStruck` 的怪：RM 列 = **早期已操作过的消息序列**（含操作产生的新 RM）；
            // 其余只记队列里收到的 RM（v1 被动壳）。
            List<(SystemModule.Data.SendMessage Msg, byte Prio)> operatedList = null;
            bool operatedStruck = monOperated != null && monOperated.TryGetValue(i, out operatedList);
            var rm = operatedStruck
                ? operatedList.Select(m => (object)new
                {
                    ident = m.Msg.wIdent,
                    wParam = m.Msg.wParam,
                    p1 = m.Msg.nParam1,
                    p2 = m.Msg.nParam2,
                    p3 = m.Msg.nParam3,
                    msg = m.Msg.Buff ?? "",
                    late = m.Msg.LateDelivery,
                    delivery = m.Msg.DeliveryTime,
                    priority = m.Prio,
                }).ToList()
                : mo.DrainRaw().Select(m => (object)new
                {
                    ident = m.Msg.wIdent,
                    wParam = m.Msg.wParam,
                    p1 = m.Msg.nParam1,
                    p2 = m.Msg.nParam2,
                    p3 = m.Msg.nParam3,
                    msg = m.Msg.Buff ?? "",
                    late = m.Msg.LateDelivery,
                    delivery = m.Msg.DeliveryTime,
                    priority = m.Prio,
                }).ToList();
            // **不重复投递**：同一只怪同一种 RM 恰好 1 条（口径见卡 §36；v1 判据 = 收件人集合）。
            var idents = rm.Select(o => o.GetType().GetProperty("ident").GetValue(o)).ToList();
            // **MG-48**：成功臂的怪会有**两条** `RM_DELAYMAGIC(8021)`（nPower/3 与 MSD）——
            // `AllowDuplicateMonsterRm` 显式置位的 stem 才允许；其余 stem 的「恰 1 条」守卫不变。
            if (!spec.AllowDuplicateMonsterRm && idents.Count != idents.Distinct().Count())
            {
                throw new FixtureContractViolation(
                    $"[{spec.Key}/{label}] mon{i} 收到重复 RM：{string.Join(",", idents)}（同种 RM 必须恰 1 条）");
            }
            int? gateIdxOut = null; long? gateValOut = null; bool? gateHitOut = null;
            if (monGate != null && monGate.TryGetValue(i, out var gv))
            {
                gateIdxOut = gv.GateIdx; gateValOut = gv.GateVal; gateHitOut = gv.Hit;
            }
            int? targetCretAfter = spec.Monsters[i].TargetCretCaster
                ? (mo.TargetCret == null ? 0 : mo.TargetCret.ActorId)
                : (int?)null;
            receivers.Add(new ReceiverResult
            {
                Who = "mon" + i,
                Rm = rm,
                Sent = new List<object>(),
                TargetCret = targetCretAfter,
                // **MG-44（2026-11-26）**：`ObserveState` 时落 `RunAwayMode/RunAwayTime`
                // （`:835-840` 的 10s 逃跑面）；缺省不落盘 ⇒ 旧 golden 零变化。
                RunAwayMode = spec.ObserveState ? mo.RunAwayMode : (bool?)null,
                RunAwayTime = spec.ObserveState ? mo.RunAwayTime : (int?)null,
                HpBefore = spec.Key == "groupdeding" ? monHpBefore[i]
                    : operatedStruck ? monHpBeforeOperate[i]
                    : spec.ObserveState ? monHpBefore[i] : (int?)null,
                HpAfter = spec.Key == "groupdeding" ? monHpAfter[i]
                    : operatedStruck ? (int)mo.WAbil.HP
                    : spec.ObserveState ? monHpAfter[i] : (int?)null,
                GateIndex = gateIdxOut,
                GateDraw = gateValOut,
                GateHit = gateHitOut,
            });
        }
        // **MG-55（46 分身）**：克隆体静态面 + 逐收件人 `SM_TURN(10)`/`SM_FEATURECHANGED(41)`。
        // 两帧由真身收件人侧 `RM_TURN` handler 产生（`PlayObject.Message.cs:1524-1559`）⇒ 不新增
        // 记录机制，只按 ident 过滤 `recorder.Buffers`。`recog`（= 克隆体 ActorId）**记原始值**：
        // ActorId 是进程级自增（`ActorEntity.cs:20`），两侧数值不可比 ⇒ 比较器按各自的
        // `Clone.actor_id` 归一化后再比帧；此处不相对化（相对化后无法识别哪些帧属于克隆体）。
        object cloneObs = null;
        if (spec.MagicId == 46)
        {
            IList<SystemModule.Actors.IActor> cellObjects = new List<SystemModule.Actors.IActor>();
            env.GetBaseObjects(caster.CurrX, caster.CurrY, true, ref cellObjects);
            var clones = cellObjects.OfType<M2Server.Player.PlayCloneObject>()
                .Where(o => o.ChrName == "Clone").ToList();
            if (clones.Count != 1)
            {
                throw new FixtureContractViolation(
                    $"[{spec.Key}/{label}] 46 施法后同格克隆体数 {clones.Count} ≠ 1（同格对象 {cellObjects.Count} 个）");
            }
            var cloneObj = clones[0];
            // **B20 关系契约**：oracle actor-id 是 GUID 派生（非确定），不进 golden；但生成期必须
            // 断言运行内唯一性/稳定性的一面：clone id 与四个夹具 actor 的固定 id 互不相同。
            foreach (var other in new[] { caster.ActorId, target.ActorId, obsIn.ActorId, obsOut.ActorId })
            {
                if (cloneObj.ActorId == other)
                {
                    throw new FixtureContractViolation(
                        $"[{spec.Key}/{label}] 克隆体 ActorId {cloneObj.ActorId} 与夹具 actor 撞 id");
                }
            }
            var cloneReceivers = new List<object>();
            foreach (var (who, obj) in byWho)
            {
                var frames = DecodeFramesForGate(recorder.Buffers, obj.ActorId)
                    .Where(f => f.ident == 10 || f.ident == 41) // SM_TURN=10 / SM_FEATURECHANGED=41
                    .Select(f => (object)new
                    {
                        ident = (int)f.ident,
                        // **原始 recog**（= 克隆体 ActorId）：C# 的 ActorId 由**进程级**自增分配
                        // （`ActorEntity.cs:20`），绝对值跨栈不可比 ⇒ 比较器按两侧各自的
                        // `Clone.actor_id` 把它归一化后再逐帧比；此处不相对化（相对化后无法识别）。
                        recog = f.recog,
                        param = (int)f.param,
                        tag = (int)f.tag,
                        series = (int)f.series,
                        due_ms_rel = f.due,
                        tail = f.tail,
                    })
                    .ToList();
                cloneReceivers.Add(new { who, frames });
            }
            cloneObs = new
            {
                name = cloneObj.ChrName,
                // **B20**：oracle actor-id 由 `ActorMgr.GetNextIdentity()`（GUID/随机池）分配，
                // 跨运行非确定 ⇒ **绝对值不入 golden**。关系由比较器按本侧 id 断（同侧一致性），
                // `clone_ordinal` 是确定性可比的创建序（本场景恒 0）。
                clone_ordinal = 0,
                clone_count = clones.Count,
                actor_id_policy = "oracle_guid_nondeterministic",
                x = (int)cloneObj.CurrX,
                y = (int)cloneObj.CurrY,
                dir = (int)cloneObj.Dir,
                gender = (int)cloneObj.Gender,
                hair = (int)cloneObj.Hair,
                // 有效施法方向（转向后）；`dir == (caster_dir + 4) % 8` 是 GetBackDir 的推导面。
                caster_dir = (int)caster.Dir,
                caster_gender = (int)caster.Gender,
                caster_hair = (int)caster.Hair,
                // **克隆体在建时处于隐藏态**（`PlayObject` ctor `FixedHideMode = true`，
                // `PlayObject.cs:993`；clones 不登录 ⇒ 无 `:1442` 的复位）⇒ ctor 尾的
                // `SendRefMsg(RM_TURN)` 走"只发给自己"支（`BaseObject.Message.cs:390-394`），
                // 不产生任何收件人帧。记录该值让"零帧"有可解释的输入根因。
                fixed_hide_mode = cloneObj.FixedHideMode,
                ob_mode = cloneObj.ObMode,
                // 训练尾 640 的存在性（`SpellReturn == has_640` 由比较器判，驱动/装置不自证）。
                has_640 = DecodeFramesForGate(recorder.Buffers, caster.ActorId).Any(f => f.ident == 640),
                receivers = cloneReceivers,
            };
        }
        // **MG-36（22 火墙）**：事件表快照（只记 `EventType/x/y/ContinueTime/Damage`；不记
        // `RunTick`/时间戳——`GetTickCount` 派生）。只取 `Envirnoment == env` 的事件
        // （同一 Batch 的前一个 case 会留下别的 env 的事件）。契约 fail-closed：
        // 预占格 ⊆ 五格、无重复；事件数 == 5 − 预占数且缺格恰为预占格；安全区支 == 0 且有红字。
        object fireCrossObs = null;
        if (spec.MagicId == 22)
        {
            var pre = spec.PreEventCells.Distinct().ToList();
            if (pre.Count != spec.PreEventCells.Length)
            {
                throw new FixtureContractViolation($"[{spec.Key}/{label}] PreEventCells 有重复格");
            }
            var evSnap = SystemModule.SystemShare.EventMgr.Events
                .Where(e => ReferenceEquals(e.Envirnoment, env))
                .Select(e => new
                {
                    x = (int)e.nX,
                    y = (int)e.nY,
                    type = e.EventType,
                    damage = e.Damage,
                    continue_ms = ReadInstanceInt(e, "ContinueTime"),
                })
                .OrderBy(e => e.x).ThenBy(e => e.y)
                .ToList();
            int want = (spec.FireCrossSafeZone ? 0 : 5 - pre.Count);
            if (evSnap.Count != want)
            {
                throw new FixtureContractViolation(
                    $"[{spec.Key}/{label}] 事件数 {evSnap.Count} ≠ 期望 {want}（预占 {pre.Count}）");
            }
            if (evSnap.Any(e => e.type != Grobal2.ET_FIRE || e.continue_ms <= 0))
            {
                throw new FixtureContractViolation($"[{spec.Key}/{label}] 事件字段退化（type!=ET_FIRE 或 continue_ms<=0）");
            }
            bool redSysMsg = false;
            string redText = "";
            foreach (var rm in rmAcc["caster"])
            {
                var t = rm.GetType();
                if ((int)t.GetProperty("ident").GetValue(rm) == 11004)
                {
                    redSysMsg = true;
                    redText = (string)t.GetProperty("msg").GetValue(rm) ?? "";
                    break;
                }
            }
            if (spec.FireCrossSafeZone && !redSysMsg)
            {
                throw new FixtureContractViolation($"[{spec.Key}/{label}] 安全区支必须出现红字 RM_SYSMESSAGE(11004)");
            }
            bool has640 = DecodeFramesForGate(recorder.Buffers, caster.ActorId).Any(f => f.ident == 640);
            fireCrossObs = new
            {
                events = evSnap,
                pre_event_cells = pre.Select(c => new[] { c.X, c.Y }).ToList(),
                safe_zone_flag = env.Flag.SafeArea,
                disable_in_safe_zone_fire_cross = spec.FireCrossSafeZone,
                in_safe_zone_at_cast = inSafeZoneAtCast,
                red_sysmsg = redSysMsg,
                red_sysmsg_text = redText,
                has_640 = has640,
            };
        }
        // **MG-34（20 诱惑）**：术后怪物面 + caster SlaveList + 目标收到的 60005。
        // **不建模/不比较**：`CanReAlive` 的清除后效与 `MonGen.ActiveCount--` 的 Rust 侧落值（Rust 无字段）。
        object tammingObs = null;
        if (spec.MagicId == 20)
        {
            var monSnap = new List<object>();
            for (int i = 0; i < monsters.Count; i++)
            {
                var m = monsters[i];
                string master = null;
                if (m.Master != null)
                {
                    if (ReferenceEquals(m.Master, caster))
                    {
                        master = "caster";
                    }
                    else
                    {
                        int mi = monsters.FindIndex(x => ReferenceEquals(x, m.Master));
                        master = mi >= 0 ? "mon" + mi : "other";
                    }
                }
                monSnap.Add(new
                {
                    name = m.ChrName,
                    master,
                    hp = (int)m.WAbil.HP,
                    max_hp = (int)m.WAbil.MaxHP,
                    walk_speed = m.WalkSpeed,
                    next_hit_time = m.NextHitTime,
                    can_re_alive = m.CanReAlive,
                    no_tame = m.NoTame,
                    animal = m.Animal,
                    slave_make_level = (int)m.SlaveMakeLevel,
                    mongen_active = m.MonGen == null ? (int?)null : (int)m.MonGen.ActiveCount,
                    crazy_mode = m.CrazyMode,
                    crazy_interval_ms = ReadInstanceInt(m, "CrazyModeInterval"),
                });
            }
            var slaveRel = (caster.SlaveList ?? new System.Collections.Generic.List<SystemModule.Actors.IMonsterActor>())
                .Select(s => monsters.FindIndex(x => ReferenceEquals(x, s)))
                .ToList();
            var rm60005 = new List<object>();
            foreach (var r in receivers)
            {
                if (!r.Who.StartsWith("mon"))
                {
                    continue;
                }
                foreach (var o in r.Rm)
                {
                    var t = o.GetType();
                    if ((int)t.GetProperty("ident").GetValue(o) == 60005)
                    {
                        rm60005.Add(new
                        {
                            who = r.Who,
                            w_param = (int)t.GetProperty("wParam").GetValue(o),
                        });
                    }
                }
            }
            tammingObs = new
            {
                spell_return = ok,
                has_640 = DecodeFramesForGate(recorder.Buffers, caster.ActorId).Any(f => f.ident == 640),
                rm_makeholyseize = rm60005,
                slave_list_rel = slaveRel,
                slave_count_after = caster.SlaveList?.Count ?? 0,
                monsters_after = monSnap,
            };
        }
        return new FireballResult
        {
            Fixture = AttachStatusTimeArr(new
            {
                magic_id = spec.MagicId,
                magic_name = spec.MagicName,
                // MG-34：`GetShowName` 的主人名与 `GetChrColor` 的 SlaveColor 槽（驱动输入）。
                caster_name = spec.MagicId == 20 ? caster.ChrName : null,
                slave_colors = spec.MagicId == 20
                    ? SystemModule.SystemShare.Config.SlaveColor.Select(b => (int)b).ToArray()
                    : null,
                effect = spec.Effect,
                effect_type = spec.EffectType,
                user_magic_level = spec.UserMagicLevel,
                caster_level = spec.CasterLevel,
                caster_actor_id = caster.ActorId,
                target_actor_id = target.ActorId,
                // B21：施法**输入**（`DoSpell` 之前的 `WAbil.SC/MC`），不是事后态。
                caster_mc = casterInputMc,
                caster_sc = casterInputSc,
                rng_mode = spec.RngMode,
                login_order = spec.LoginOrder,
                reachability = spec.Reachability,
                target_level = inputTargetLevel,
                target_hp = inputTargetHp,
                target_max_hp = inputTargetMaxHp,
                anti_magic = inputAntiMagic,
                target_anti_poison = inputAntiPoison,
                amulet_shape = spec.ArmRingAmulet ? spec.AmuletShape : 0,
                amy_ounsul_point = SystemModule.SystemShare.Config.AmyOunsulPoint,
                derived = new
                {
                    max_hp = contract.Derived.MaxHp,
                    max_mp = contract.Derived.MaxMp,
                    dc = contract.Derived.Dc,
                    ac = contract.Derived.Ac,
                    mac = contract.Derived.Mac,
                    max_exp = contract.Derived.MaxExp,
                },
                construction = new
                {
                    job = spec.Job.ToString(),
                    level = (int)spec.TargetLevel,
                    equipment = spec.Equipment,
                    skills = spec.Skills.Length == 0 ? new[] { spec.MagicName } : spec.Skills,
                },
                scenario_note = string.IsNullOrWhiteSpace(spec.ScenarioNote)
                    ? (spec.LoginOrder
                        ? "登录序（UserLogon→RecalcLevelAbilitys+RecalcAbilitys；target/caster 同标准，HP=公式 MaxHP）"
                        : spec.ScenarioNote)
                    : spec.ScenarioNote,
                legacy_artifact = spec.RngMode == "freeze_at_dospell",
                legacy_reason = spec.RngMode == "freeze_at_dospell" ? "rng ledger incomplete (drain-stage draws omitted)" : null,
                seed_reason = seedReasonText,
                rng_total_draws = totalDraws,
                rng_recorded = draws.Count,
                target_null = targetNull,
                arm_ring_amulet = spec.ArmRingAmulet,
                wait_ms = spec.WaitMs,
                caster_hp = spec.CasterHp,
                caster_max_hp = spec.CasterMaxHp,
                // 范围类魔法（22/23/33/24/47）的 range 值与其部署配置来源（卡 §31）。
                snow_wind_range = SystemModule.SystemShare.Config.SnowWindRange,
                fire_boom_rage = SystemModule.SystemShare.Config.FireBoomRage,
                elec_blizzard_range = SystemModule.SystemShare.Config.ElecBlizzardRange,
                server_conf_sha256 = ServerConfSha256(),
                // **MG-57 P2**：坐标**实际值**（缺省臂 = 旧常量 ⇒ 既有 golden 逐字节不变）。
                caster_xy = new[] { fixtureCasterX, fixtureCasterY },
                target_xy = new[] { fixtureTargetX, fixtureTargetY },
                obs_in_xy = new[] { fixtureObsInX, fixtureObsInY },
                obs_out_xy = new[] { fixtureObsOutX, fixtureObsOutY },
                hit_speed = spec.HitSpeed,
                // **施毒 `SysMsg(YouPoisoned)` 载荷来源（MG-13/14/15/50/64/65）**：`BaseObject.MakePosion`
                // 的 `SysMsg(..., MsgColor.Red, MsgType.Hint)` 以前/背景色 = `[Setup] RedMsgFColor/RedMsgBColor`
                // （`ServerConf.cs`）与文本 = `[String] YouPoisoned`（`StringConf.cs:76`）出 100 帧。
                // 驱动**必须**用这三个值，不许硬编码 0/空串（卡 §53：记录端 = 实际使用值）。
                red_msg_f_color = SystemModule.SystemShare.Config.RedMsgFColor,
                red_msg_b_color = SystemModule.SystemShare.Config.RedMsgBColor,
                you_poisoned = SystemModule.MessageSettings.YouPoisoned,
                frame_order_scope = string.IsNullOrEmpty(spec.FrameOrderScope) ? null : spec.FrameOrderScope,
                sockets = new[] { 911, 912, 913, 914 },
            }, spec.PoisonStates == null
                ? null
                : (object)new
                {
                    dechealth = statusBefore[0],
                    damagearmor = statusBefore[1],
                    stone = statusBefore[2],
                },
                spec.Monsters.Length == 0
                ? null
                // **MG-44（2026-11-26）**：改为有序字典——`super_man` **只在 true 时出现**
                // （条件键；旧的带怪 golden 不受影响，逐字节相等）。键序与原匿名对象一致。
                : (object)spec.Monsters.Select(m =>
                {
                    var d = new Dictionary<string, object>
                    {
                        // MG-34：名称是 `GetShowName`/`RM_USERNAME` 尾的输入（仅 20 记录）。
                        ["name"] = spec.MagicId == 20 ? m.Name : null,
                        ["x"] = m.X,
                        ["y"] = m.Y,
                        ["race"] = m.Race,
                        ["level"] = m.Level,
                        ["hp"] = m.Hp,
                        ["life_attrib"] = m.LifeAttrib,
                        ["master"] = m.Master,
                        ["target_cret"] = m.TargetCretCaster ? -1 : m.TargetCret,
                        ["speed_point"] = m.SpeedPoint,
                        ["death"] = m.Death,
                        ["ac"] = m.AC,
                        ["anti_magic"] = m.AntiMagic,
                        // **MG-34**：诱惑夹具字段（旧 stem 均为缺省值，零语义变化）。
                        ["no_tame"] = m.NoTame,
                        ["animal"] = m.Animal,
                        ["mongen_active"] = m.MonGenActive,
                        ["can_re_alive"] = m.CanReAlive,
                        ["walk_speed"] = m.WalkSpeed,
                        ["next_hit_time"] = m.NextHitTime,
                        ["master_caster"] = m.MasterCaster,
                    };
                    if (m.SuperMan)
                    {
                        d["super_man"] = true;
                    }
                    return d;
                }).ToArray(),
                spec.CasterHitPoint >= 0 ? (int?)spec.CasterHitPoint : null,
                spec.CasterDC >= 0 ? (int?)spec.CasterDC : null,
                spec.CasterAttackMode >= 0 ? (int?)spec.CasterAttackMode : null,
                spec.CasterLoginOrderAbil ? (int?)casterInputSc : null,
                spec.CasterCharStatusEx != 0 ? (int?)spec.CasterCharStatusEx : null,
                spec.TargetMonster > 0 ? (int?)spec.TargetMonster : null,
                spec.ServerConf,
                targetMonsterUndead: spec.TargetMonster > 0 && !spec.TargetMonsterUndead ? (bool?)false : null,
                faithfulPump: spec.FaithfulPump ? (bool?)true : null,
                secondCastAtMs: spec.SecondCastAtMs,
                realMap: spec.RealMap ? (bool?)true : null,
                productionAoi: spec.ProductionAoi ? (bool?)true : null,
                pushCacheCold: spec.PushCacheProbe ? (bool?)true : null,
                observeDirAfter: spec.ObserveDirAfter ? (bool?)true : null,
                targetDirBefore: spec.ObserveDirAfter ? (int?)fixtureTargetDirBefore : null,
                healTicksMs: spec.HealTicksMs,
                // 恢复门时钟的**初值**（相对 castTick 的偏移；驱动 `Recovery::new(cast + base)`）。
                healTickBase: spec.HealTicksMs != null ? (int?)(healTickBase - (int)castTick) : null,
                healLevelTarget: spec.HealTicksMs != null ? (int?)healLevelTarget : null,
                healLevelCaster: spec.HealTicksMs != null ? (int?)healLevelCaster : null,
                amuletDura: spec.AmuletDura.HasValue ? (int?)amuletDuraInitial : null,
                specHealBoth: spec.HealTicksBoth,
                // **MG-55（46 分身）**：夹具输入快照（其它 stem 恒 null ⇒ 键不落盘）。
                casterDir: spec.MagicId == 46 ? spec.CasterDir : (int?)null,
                casterGender: spec.MagicId == 46 ? spec.CasterGender : (int?)null,
                casterHair: spec.MagicId == 46 ? spec.CasterHair : (int?)null,
                preSpellFaceTarget: spec.MagicId == 46 ? (bool?)spec.PreSpellFaceTarget : null,
                magicTrainLv: (spec.MagicId == 46 || spec.MagicId == 22 || spec.NonzeroArm) ? (int?)spec.MagicTrainLv : null,
                magicMaxTrain0: (spec.MagicId == 46 || spec.MagicId == 22) ? (int?)spec.MagicMaxTrain0 : null,
                magicPower: (spec.NonzeroArm && spec.MagicPower != 0) ? (int?)spec.MagicPower : null,
                magicMaxPower: (spec.NonzeroArm && spec.MagicMaxPower != 0) ? (int?)spec.MagicMaxPower : null,
                magicDefPower: (spec.NonzeroArm && spec.MagicDefPower != 0) ? (int?)spec.MagicDefPower : null,
                magicDefMaxPower: (spec.NonzeroArm && spec.MagicDefMaxPower != 0) ? (int?)spec.MagicDefMaxPower : null,
                // `PlayObject.FastTrain` 缺省 false（`PlayObject.cs:2023`），装置从不置 true
                // （只有快速修炼项链 Shape 才会置）；显式记录，比较器断言为 false。
                fastTrain: spec.MagicId == 46 ? (bool?)false : null,
                fireCrossSafeZone: spec.MagicId == 22 ? (bool?)spec.FireCrossSafeZone : null,
                preEventCells: spec.MagicId == 22
                    ? (object)spec.PreEventCells.Select(c => new[] { c.X, c.Y }).ToArray()
                    : null),
            Label = label,
            Seed = seed,
            DrainPasses = drainPasses,
            DrainPassesHitCap = drainPassesHitCap,
            SpellReturn = ok,
            Receivers = receivers,
            Rng = draws,
            TargetHp = hpAfter,
            TargetDirAfter = spec.ObserveDirAfter ? (int?)((int)target.Dir) : null,
            HealTicks = healTicks,
            TargetHpBefore = hpBefore,
            TargetMonsterHpBefore = monsterHpBefore,
            TargetMonsterHpAfter = monsterHpAfter,
            NotDue = pending,
            CasterMp = caster.WAbil.MP,
            CasterExtraAbil0 = casterExtra0,
            Train = !spec.SkillTail
                ? null
                : (object)new
                {
                    level_before = trainLevelBefore,
                    level_after = (int)um.Level,
                    tran_point_before = trainTpBefore,
                    tran_point_after = um.TranPoint,
                    train_lv = (int)um.Magic.TrainLv,
                    max_train0 = (um.Magic.MaxTrain != null && um.Magic.MaxTrain.Length > 0) ? um.Magic.MaxTrain[0] : 0,
                    n_level = trainNLevel,
                    max_train_at_level = trainMtUsed,
                    train_level_at_level = (int)um.Magic.TrainLevel[um.Level],
                },
            Skeleton = spec.MagicId != 17
                ? null
                : (object)new
                {
                    // 替身怪物 HP 常量（生产 MonsterInfo.hp 的占位；见 OfflineWorldEngine）。
                    // 两个输入都显式设定、且可见（取代误导性的 stub_monster_hp）：
                    stub_wabil_hp = 17,
                    stub_abil_max_hp = 17,
                    stub_monster_race = 100,
                    pre_slave_count = spec.PreSlaveCount,
                    slave_count_before = slaveCountBefore,
                    slave_count_after = caster.SlaveList?.Count ?? 0,
                    regen_calls = OfflineWorldEngine.RegenCalls - regenBefore,
                    // MasterRoyaltyTick 是墙钟派生 → 不记录不比较（§36）
                    new_slaves = (caster.SlaveList ?? new System.Collections.Generic.List<SystemModule.Actors.IMonsterActor>())
                        .Skip(slaveCountBefore)
                        .Select(x => (object)new
                        {
                            name = x.ChrName,
                            level = (int)x.Abil.Level,
                            hp = (int)x.WAbil.HP,
                            max_hp = (int)x.WAbil.MaxHP,
                            master_is_caster = ReferenceEquals(x.Master, caster),
                        })
                        .ToList(),
                },
            SinSu = spec.MagicId != 30
                ? null
                : (object)new
                {
                    stub_wabil_hp = 17,
                    stub_abil_max_hp = 17,
                    stub_monster_race = 113,
                    pre_slave_count = spec.PreSlaveCount,
                    pre_slave_xy_before = preSlaveXyBefore,
                    pre_slave_xy_after = (caster.SlaveList ?? new System.Collections.Generic.List<SystemModule.Actors.IMonsterActor>())
                        .Take(spec.PreSlaveCount)
                        .Select(x => (object)new { name = x.ChrName, x = (int)x.CurrX, y = (int)x.CurrY })
                        .ToList(),
                    // 预置 slave 从 (0,0) 移到施法者面前即 moved=true；未命中同名（空操作分界）则 false。
                    recall_moved = (caster.SlaveList ?? new System.Collections.Generic.List<SystemModule.Actors.IMonsterActor>())
                        .Take(spec.PreSlaveCount).Any(x => x.CurrX != 0 || x.CurrY != 0),
                    slave_count_before = slaveCountBefore,
                    slave_count_after = caster.SlaveList?.Count ?? 0,
                    regen_calls = OfflineWorldEngine.RegenCalls - regenBefore,
                    new_slaves = (caster.SlaveList ?? new System.Collections.Generic.List<SystemModule.Actors.IMonsterActor>())
                        .Skip(slaveCountBefore)
                        .Select(x => (object)new
                        {
                            name = x.ChrName,
                            level = (int)x.Abil.Level,
                            hp = (int)x.WAbil.HP,
                            max_hp = (int)x.WAbil.MaxHP,
                            master_is_caster = ReferenceEquals(x.Master, caster),
                        })
                        .ToList(),
                },
            Angel = spec.MagicId != 41
                ? null
                : (object)new
                {
                    // 与 Skeleton 同源：替身常量的可见值（生产 MonsterInfo.hp 的占位）。
                    stub_wabil_hp = 17,
                    stub_abil_max_hp = 17,
                    stub_monster_race = 100,
                    pre_slave_count = spec.PreSlaveCount,
                    slave_count_before = slaveCountBefore,
                    slave_count_after = caster.SlaveList?.Count ?? 0,
                    regen_calls = OfflineWorldEngine.RegenCalls - regenBefore,
                    new_slaves = (caster.SlaveList ?? new System.Collections.Generic.List<SystemModule.Actors.IMonsterActor>())
                        .Skip(slaveCountBefore)
                        .Select(x => (object)new
                        {
                            name = x.ChrName,
                            level = (int)x.Abil.Level,
                            hp = (int)x.WAbil.HP,
                            max_hp = (int)x.WAbil.MaxHP,
                            master_is_caster = ReferenceEquals(x.Master, caster),
                        })
                        .ToList(),
                },
            Clone = cloneObs,
            FireCross = fireCrossObs,
            Tamming = tammingObs,
            StatusTimeArr = spec.PoisonStates == null
                ? null
                : (object)new
                {
                    dechealth = statusAfter[0],
                    damagearmor = statusAfter[1],
                    stone = statusAfter[2],
                },
        };
    }

    /// <summary>非玩家对象（动物/怪物）夹具输入。</summary>
    /// <summary>
    /// **额外 case 描述（2026-11-26）**：`MagicSpec.ExtraCases` 的元素。
    /// </summary>
    private sealed class ExtraCaseSpec
    {
        /// <summary>额外 case 的标签（如 `undead` / `monster` / `monster_undead`）。</summary>
        public string Label;
        /// <summary>同场怪物（空 ⇒ 无）。只在本 case 生效，做完还原。</summary>
        public MonsterSpec[] Monsters = Array.Empty<MonsterSpec>();
        /// <summary>施法目标 = 本列表第几只怪（**1 基**；0 = 既有的玩家目标）。</summary>
        public int TargetMonster;
        /// <summary>`TargetMonster` 是否不死（缺省 true；只在 `TargetMonster>0` 时生效）。</summary>
        public bool TargetMonsterUndead = true;
        /// <summary>
        /// seed 口径：true ⇒ 用本批的 **hit 种子**（`AntiMagicBatch`；首抽越过门槛），
        /// false ⇒ 固定 1（`BehaviorBatch` 旧口径，`lightflower/undead` 即此）。
        /// </summary>
        public bool UseHitSeed;
        /// <summary>
        /// **B17**：本 case 是否**以生产泵语义（含 t=0 泵）对齐**（见 `MagicSpec.FaithfulPump`）——
        /// 「遇未到期队首即停」+ 每 tick 只泵一次 + 施法当刻泵一次。典型场景：同一 caster 队列里
        /// 同时有 640@1000 与 1200ms 的 `RM_DELAYMAGIC`（firecharm/monster）。
        /// 落 `Fixture.faithful_pump=true`，驱动据此在 t=0 泵一次（否则等键旋转次数差 1、640 漂到 1100）。
        /// </summary>
        public bool FaithfulPump;
        /// <summary>
        /// **MG-48（2026-11-26）**：本 case 的 `OperateStruck` 怪按**每步**结算（见
        /// `MagicSpec.OperatePerStep`）。缺省 false ⇒ 既有口径（窗口结束后统一结算）。
        /// </summary>
        public bool OperatePerStep;
        /// <summary>
        /// **MG-57（2026-11-26）**：本 case 的第二次施法时刻（见 `MagicSpec.SecondCastAtMs`）。
        /// </summary>
        public int? SecondCastAtMs;
        /// <summary>
        /// **MG-57**：本 case 的 rig 用真 `0.map` / 生产 AOI（见 `MagicSpec.RealMap` / `ProductionAoi`）。
        /// </summary>
        public bool RealMap;
        public bool ProductionAoi;
        /// <summary>
        /// **MG-57 P2（2026-11-26）**：本 case 的四个对象坐标覆盖（见 `MagicSpec.CasterXy`）。
        /// </summary>
        public int[] CasterXy;
        public int[] TargetXy;
        public int[] ObsInXy;
        public int[] ObsOutXy;
        /// <summary>**MG-57 P2**：本 case 断言受害者 `SendRefMsg` 缓存冷（见 `MagicSpec.PushCacheProbe`）。</summary>
        public bool PushCacheProbe;
        /// <summary>**MG-57 P2**：本 case 记录受害者术后 `Dir`（见 `MagicSpec.ObserveDirAfter`）。</summary>
        public bool ObserveDirAfter;
        /// <summary>
        /// **MG-12/40（2026-11-26）**：恢复 tick 的**虚拟毫秒列表**（绝对虚拟时刻；null = 不跑 tick）。
        /// 装置在每个时刻 `ClockSeam.Set(ms)` 后对**被治疗者**调一次生产 `PlayObject.Run()`
        /// （oracle `:70-135` 恢复段），记录 `{ms, hp, inc_healing}` 轨迹。
        /// </summary>
        public int[] HealTicksMs;
        /// <summary>**MG-12/40**：本 case 的**被治疗者**（target）等级覆盖（恢复门 `dw` 读它）。</summary>
        public int? TargetLevelOverride;
        /// <summary>**MG-12/40**：本 case 的 target **初始 HP**（`null` = 用 stem 的 `TargetMaxHp`）。</summary>
        public int? TargetHp;
        /// <summary>**MG-50**：本 case 的护身符 `Dura`（见 `MagicSpec.AmuletDura`）。</summary>
        public int? AmuletDura;
        /// <summary>
        /// **MG-57 P2**：本 case 的 `StepDrainMs` 覆盖（`null` = 用 stem 值）。第二次施法需要
        /// "步"（非步进支没有 `elapsed` 推进）⇒ 只给该 case 打开步进，既有三支保持非步进。
        /// </summary>
        public int? StepDrainMsOverride;
        /// <summary>
        /// **MG-53（2026-11-26）**：本 case 的 `UserMagicLevel` 覆盖（`null` = 用 stem 值）。
        /// 用途：同一 stem 既有支是 `UserMagicLevel=3`（训练门关），要观测 `Race>=Animal ⇒ boTrain`
        /// 就必须令训练门开（`< 3`）——但那样会同时改变等级轴，故必须配一个**同等级的玩家对照支**
        /// （`player_lv0`）与怪目标支（`monster_lv0`）逐项比，差值才是 `Race>=Animal` 的效应。
        /// </summary>
        public int? UserMagicLevelOverride;
        /// <summary>
        /// **MG-32（2026-11-26）**：本 case 的 seed 覆盖（`null` = 既有口径：AntiMagic 批看
        /// `UseHitSeed`，BehaviorBatch 恒 1）。用于覆盖同一判据的两个分支（如 `Random(2)==0` 的 0/1）。
        /// </summary>
        public int? SeedOverride;
        /// <summary>
        /// **MG-44（2026-11-26）**：本 case 的 `CasterLevel` 覆盖（`null` = 用 stem 值）。
        /// 用于 `:842 Random(2)+(casterLv-1) > targetLv` 的 **miss 臂**（低等级施法者）。
        /// </summary>
        public int? CasterLevelOverride;
        /// <summary>
        /// **MG-44（2026-11-26）**：本 case 是否落**术后状态观测列**（§26）：
        /// `caster.TargetCret`（`:841 SetTargetCreat` 面）与 `mon*.RunAwayMode/RunAwayTime`
        /// （`:835-840` 面）。缺省 false ⇒ 旧 golden 零变化。
        /// </summary>
        public bool ObserveState;
        /// <summary>
        /// 该额外 case 的 **ActorId 段基**（缺省 4100，沿用旧口径）。**多个额外 case 必须互异**：
        /// `SystemShare.ActorMgr` 是**进程级静态**，同一 run 内两个 case 用同一段会让
        /// `SendRefMsg` 的格子扫描取到上一 case 的旧对象（2026-11-26 实测：`monster_undead`
        /// 的 10008/10120/638 全丢，落到上一 case 的队列）。
        /// </summary>
        public int IdBase = 4100;
    }

    private sealed class MonsterSpec
    {
        public int X;
        public int Y;
        /// <summary>`Race`（`ActorRace.Animal` 起的值即"动物/怪物"）。</summary>
        public int Race = 50;
        public int Level = 1;
        public int Hp = 300;
        /// <summary>`LifeAttrib`（`Grobal2.LA_UNDEAD` = 不死系）。</summary>
        public int LifeAttrib;
        /// <summary>`Master` 指向本列表第几只（**1 基**；`0` = 无主）。</summary>
        public int Master;
        /// <summary>`TargetCret` 指向本列表第几只（**1 基**；`0` = 无）。</summary>
        public int TargetCret;
        /// <summary>
        /// **MG-32（2026-11-26）**：`TargetCret` 指向**施法者**（`MagMakePrivateTransparent` 的
        /// `Race>=Animal && TargetCret == baseObject` 分支）。现有 `TargetCret` 只能指本列表第几只怪；
        /// 本开关把绑定改为 caster。golden 的 `target_cret` 落 **-1**（sentinel；Rust 识别后绑定 caster）。
        /// 只在显式置位的 case 生效。
        /// </summary>
        public bool TargetCretCaster;
        /// <summary>
        /// **MG-44（2026-11-26）**：`SuperMan`（`MagTurnUndead :830` 的第一条负路径）。
        /// 缺省 false；只在显式置位的 case 生效。
        /// </summary>
        public bool SuperMan;
        /// <summary>`SpeedPoint`（命中门 `Random(SpeedPoint)`）；缺省 15 = `BaseObject` ctor 缺省。卡 §39。</summary>
        public int SpeedPoint = 15;
        /// <summary>死怪：`GetMapBaseObjects` 收集期即被过滤（不进 objectList）。卡 §39 `dead_excluded`。</summary>
        public bool Death;
        /// <summary>`WAbil.AC` 原始字（Lo/Hi 字节，`HUtil32.MakeWord(lo,hi)`）；缺省 0 = 无护甲。卡 §39。</summary>
        public int AC;
        /// <summary>
        /// `AntiMagic`（穿透类 `MagPassThroughMagic` 的骰门 `Random(10) >= AntiMagic`）；
        /// 缺省 0 = 必中。**MG-17/18 用互异值在同一个 case 里造出命中/未命中两支**，
        /// 否则三条 seed 全落同一支、变异无从变红。
        /// </summary>
        public int AntiMagic;
        /// <summary>**MG-34**：`NoTame`（`WorldServer.MonGen.cs:911` 由 `MonsterInfo.boTame` 置；本夹具显式）。</summary>
        public bool NoTame;
        /// <summary>**MG-34**：`Animal`（`MagicManager.cs:395` 的 `!targetObject.Animal` 门）。</summary>
        public bool Animal;
        /// <summary>
        /// **MG-38（2026-11-26）**：让这只怪在装置里**像真怪一样结算**它收到的 `RM_MAGSTRUCK`
        /// （`BaseObject.Base.cs:618-686`：`R(1000)` walk / `GetMagStruckDamage` / `StruckDamage` /
        /// `SendRefMsg(RM_STRUCK_MAG)` / `SendMsg(RM_STRUCK)` / `AnimalObject.Struck` 的 `R(300)`）。
        /// **缺省 false ＝ 既有的 v1 被动壳**（只记 RM、不 Operate，B2 盲区）⇒ 其它 stem 零变化。
        /// 只给「本支要验怪物受击效果面」的 case 打开（当前仅 `lightflower/undead`）。
        /// 打开时必须同时 `Animal = true`（生产形态；Rust `place_monsters` 恒置 animal=true）。
        /// </summary>
        public bool OperateStruck;
        /// <summary>**MG-34**：`MonGen.ActiveCount`（`-1` = `MonGen=null`；`>=0` = 建 `MonGenInfo{ActiveCount=v}`）。</summary>
        public int MonGenActive = -1;
        /// <summary>**MG-34**：`CanReAlive`（`MagicManager.cs:745` 的收服支前置）。缺省 false = ctor 缺省。</summary>
        public bool CanReAlive;
        /// <summary>**MG-34**：`WalkSpeed`（`-1` = 不设）。</summary>
        public int WalkSpeed = -1;
        /// <summary>**MG-34**：`NextHitTime`（`-1` = 不设）。</summary>
        public int NextHitTime = -1;
        /// <summary>**MG-34**：`Master = 施法者`（1 基 `Master` 之外的快捷写法；给 self_master 支）。</summary>
        public bool MasterCaster;
        /// <summary>**MG-34**：怪物 `ChrName`（`GetShowName` 输入；仅 20 落 Fixture）。</summary>
        public string Name;
    }

    private static readonly string[] Unused = Array.Empty<string>();

}

/// <summary>
/// **记录面补全**：`M2Share.NetChannel` 的只记录实现（`INetChannel.AddGateBuffer` 是全部下行
/// 的唯一汇合点，见 `PlayObject.Socket.cs:70/98/128` 三处 SendSocket 重载）。
/// 生产类零改动；`Runner`/网络线程相关成员在离线进程里不会被调用，空实现即可。
/// </summary>
public sealed class RecordingNetChannel : M2Server.Net.INetChannel
{
    public List<(int GateIdx, byte[] Data, long Due)> Buffers { get; } = new();

    /// <summary>
    /// 当前正在处理的消息的**相对到期时刻**（ms，相对施法时刻；立即发送 = 0）。
    /// 排水循环在 `OperatePublic` 前置（延迟消息 = `DeliveryTime - castTick`），帧捕获时随帧记录。
    /// 与 Rust 台账的 `due_ms_rel` 同口径、可跨栈比较（**不得用墙钟绝对值**）。
    /// </summary>
    public static long CurrentDueMs;

    public void AddGateBuffer(int gateIdx, byte[] senData) => Buffers.Add((gateIdx, senData, CurrentDueMs));

    public void AddGameGateQueue(int gateIdx, ServerMessage packet, byte[] data) => Buffers.Add((gateIdx, data, CurrentDueMs));

    public void CloseAllGate() { }
    public void CloseUser(int gateIdx, int nSocket) { }
    public void Initialize() { }
    public void KickUser(string account, int sessionId, int payMode) { }
    public void Run() { }
    public void SendOutConnectMsg(int gateIdx, int nSocket, ushort nGsIdx) { }
    public void SendServerStopMsg() { }
    public void SetGateUserList(int gateIdx, int nSocket, SystemModule.Actors.IPlayerActor playObject) { }
    public Task Start(CancellationToken cancellationToken = default) => Task.CompletedTask;
    public void Send(string connectId, byte[] buff) { }
    public void CloseGate(string connectionId, string endPoint) { }
    public Task StopAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;
}

/// <summary>
/// 只记录不发送的玩家对象：
///  * 下行帧**不再在这里**记录：`M2Share.NetChannel` 换成 [`RecordingNetChannel`] 后，
///    三处 `SendSocket` 重载（含 `SendDefMessage` 走的私有单参版）都汇到同一 `AddGateBuffer`，
///    记录面由 [`ParseFrames`] 统一解析（**修复旧版只捕获双参 virtual 的缺口**）；
///  * `Operate(ProcessMessage)` 在 `PlayObject.Message.cs:768` 是 protected override —— 暴露一个
///    公共包装，用生产编码路径处理自己队列里的 RM 消息；
///  * `MsgQueue`（`ActorEntity.cs:16`，protected）用于抽 RM 账本。
/// **生产类零改动**。
/// </summary>
/// <summary>
/// **被动**怪物记录壳（v1）：只暴露 `DrainRaw` 让金标准记录"它收到了哪些 RM"；
/// **不提供 `Operate`** —— 效果面（状态置位/伤害/诱捕）属 v2（卡 openmir2-monster-actors-v2）。
/// </summary>
public sealed class RecordingMonster : M2Server.Monster.MonsterObject
{
    /// <summary>
    /// **MG-38（2026-11-26，`MonsterSpec.OperateStruck`）**：用生产 `Operate` 处理一条 RM
    /// （`BaseObject.Base.cs` 的消息处理器；与 `RecordingPlayObject.OperatePublic` 同法）。
    /// 只在 `OperateStruck=true` 的 case 里调用；缺省路径不碰（v1 被动壳零变化）。
    /// </summary>
    public void OperatePublic(SystemModule.Data.SendMessage m)
    {
        var msg = new OpenMir2.Data.ProcessMessage
        {
            ActorId = m.ActorId,
            wIdent = m.wIdent,
            wParam = m.wParam,
            nParam1 = m.nParam1,
            nParam2 = m.nParam2,
            nParam3 = m.nParam3,
            LateDelivery = m.LateDelivery,
            Msg = m.Buff ?? "",
        };
        Operate(msg);
    }

    public List<(SystemModule.Data.SendMessage Msg, byte Prio)> DrainRaw()
    {
        var list = new List<(SystemModule.Data.SendMessage, byte)>();
        while (MsgQueue.TryDequeue(out var m, out var prio)) list.Add((m, prio));
        return list;
    }
}

public sealed class RecordingPlayObject : M2Server.Player.PlayObject
{
    /// <summary>**MG-12/40**：恢复门时钟的只读观测（`CharacterObject.IncHealthSpellTick`，protected）。</summary>
    public int IncHealthSpellTickPublic => IncHealthSpellTick;

    /// <summary>**MG-12/40**：消息队列只读计数（`Run()` 惰性前提：无待处理消息）。</summary>
    public int MsgQueueCount => MsgQueue.Count;
    /// <summary>
    /// B21：本对象被 `RecalcAbilitys()` 的次数。装置用它**运行期**判定「本支 `DoSpell` 是否把
    /// 施法者重算过」（`WAbil = Abil.Clone()`），取代易漏登记的静态来源清单；调用方在 `DoSpell`
    /// 前清零。覆写只计数 + 走基类，不改行为。
    /// </summary>
    public int RecalcAbilitysCalls;

    public override void RecalcAbilitys()
    {
        RecalcAbilitysCalls++;
        base.RecalcAbilitys();
    }

    /// <summary>
    /// 抽 RM 账本：连同**入队优先级**一起取（`priority` 必须进比较面——金标准要与
    /// 比较器逐字段对齐；来源见 `BaseObject.Message.cs:181`（`Enqueue(..., (byte)wIdent)`）与
    /// `:204`（`SendSelfDelayMsg` 的 `(byte)MessagePriority.Lowest`））。
    /// </summary>
    public List<(SystemModule.Data.SendMessage Msg, byte Prio)> DrainRaw()
    {
        var list = new List<(SystemModule.Data.SendMessage, byte)>();
        while (MsgQueue.TryDequeue(out var m, out var prio)) list.Add((m, prio));
        return list;
    }

    /// <summary>
    /// **B17（2026-11-26）**：生产 `BaseObject.GetMessage` 语义的单步泵——
    /// `TryPeek` 队首；`DeliveryTime > 0 && now < DeliveryTime` ⇒ `TryDequeue` + 以
    /// `MessagePriority.Lowest` 重入队并**结束本 actor 本次泵**（返回 null 表示"停"）；
    /// 到期 ⇒ `TryDequeue` 返回该消息；队列空也返回 null。与 `DrainRaw`（一次取全部）不同。
    /// </summary>
    public (SystemModule.Data.SendMessage Msg, byte Prio)? FaithfulPumpOne(int now)
    {
        if (!MsgQueue.TryPeek(out var head, out _))
        {
            return null;
        }
        if (head.DeliveryTime > 0 && now < head.DeliveryTime)
        {
            if (MsgQueue.TryDequeue(out var notDue, out _))
            {
                MsgQueue.Enqueue(notDue, (byte)OpenMir2.Enums.MessagePriority.Lowest);
            }
            return null;
        }
        if (MsgQueue.TryDequeue(out var due, out var prio))
        {
            return (due, prio);
        }
        return null;
    }

    /// <summary>把**未到期**的消息按原优先级放回队列（步进排水用；`MsgQueue` 是 4 叉堆）。</summary>
    public void PushBack(SystemModule.Data.SendMessage m, byte prio) => MsgQueue.Enqueue(m, prio);

    /// <summary>用生产编码路径处理一条 RM（`PlayObject.Message.cs:768` 的 Operate）。</summary>
    public void OperatePublic(SystemModule.Data.SendMessage m)
    {
        var msg = new OpenMir2.Data.ProcessMessage
        {
            ActorId = m.ActorId,
            wIdent = m.wIdent,
            wParam = m.wParam,
            nParam1 = m.nParam1,
            nParam2 = m.nParam2,
            nParam3 = m.nParam3,
            LateDelivery = m.LateDelivery,
            Msg = m.Buff ?? "",
        };
        Operate(msg);
    }
}

/// <summary>单个接收者的账本：RM（进队）+ 真实 SM 帧（经生产编码后从 SendSocket 出来）。</summary>
public sealed class ReceiverResult
{
    public string Who { get; set; }
    public List<object> Rm { get; set; }
    public List<object> Sent { get; set; }
    /// <summary>卡 §39（仅 `groupdeding` 的 `mon{i}`）：怪物 HP 前/后值（施法段同步结算的观测）。</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public int? HpBefore { get; set; }
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public int? HpAfter { get; set; }
    /// <summary>
    /// **MG-32（2026-11-26）**：`MonsterSpec.TargetCretCaster` 支的**术后** `TargetCret`
    /// （`0` = 已被清空；否则 = 指向的 ActorId）。只有声明该开关的 case 落盘（旧件零变化）。
    /// </summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public int? TargetCret { get; set; }
    /// <summary>
    /// **MG-44（2026-11-26）**：`MagTurnUndead :835-840` 的逃跑面（`RunAwayMode`/`RunAwayTime`）。
    /// 只有 `ObserveState` 的 case 落盘（旧件零变化）。
    /// </summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public bool? RunAwayMode { get; set; }
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public int? RunAwayTime { get; set; }
    /// <summary>卡 §39：命中门 `Random(SpeedPoint)` 在 `Rng` 序列中的序号与抽值，及是否命中。</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public int? GateIndex { get; set; }
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public long? GateDraw { get; set; }
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public bool? GateHit { get; set; }
}

/// <summary>火球单支结果（三支由 `--case fireball` 输出一份 JSON）。</summary>
public sealed class FireballResult
{
    /// <summary>
    /// **MG-12/40**：恢复 tick 轨迹 `[{ms, hp, inc_healing}]`（只有 `HealTicksMs` 的 case 落盘）。
    /// `hp` = 该 tick 后的 `WAbil.HP`（真加血），`inc_healing` 只作辅助列。
    /// </summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public List<object> HealTicks { get; set; }
    /// 该支的夹具输入（逐字段写进产物，Rust 侧直接读它复现，**禁止手抄**）。
    /// `null` 时**不落盘**（保持火球既有 golden 的 schema 不变）。
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public object Fixture { get; set; }
    public string Label { get; set; }
    /// <summary>
    /// **MG-57 P2**：受害者（玩家目标）**术后 `Dir`**（`CharPushed` 收尾：`result>0` ⇒ 回向、
    /// `result==0` ⇒ 初值）。只有声明 `ObserveDirAfter` 的 case 落盘（旧件零变化）。
    /// 用途：把「`Dir` 用 `nDir` 而非 `nBackDir`」这类变异变成可见红。
    /// </summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public int? TargetDirAfter { get; set; }
    /// <summary>MG-44：`TargetMonster>0` 时目标怪物的术前/术后 HP（null ⇒ 不落盘，旧件零变化）。</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public int? TargetMonsterHpBefore { get; set; }
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public int? TargetMonsterHpAfter { get; set; }
    /// 实际排水趟数（`DrainRaw` 到空为止；正常 ≤4）。
    public int DrainPasses { get; set; }
    /// 上界 `MaxDrainPasses` 命中（= 可能仍有未排干消息；不得当作完整账本）。
    public bool DrainPassesHitCap { get; set; }
    public int Seed { get; set; }
    public bool SpellReturn { get; set; }
    public List<ReceiverResult> Receivers { get; set; }
    public List<long> Rng { get; set; }
    public int TargetHp { get; set; }
    public int TargetHpBefore { get; set; }
    public List<object> NotDue { get; set; }
    public int CasterMp { get; set; }
    /// <summary>卡 36：`AttPowerUp` 写入的 `ExtraAbil[0]`（= `nPower`）；仅 `uenhancer` 落盘。</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public int? CasterExtraAbil0 { get; set; }
    /// <summary>17 SKELLETON 观测（仅该魔号；null 不落盘）：预置数/前后 slave 数/regen 次数/新增实体。</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public object Skeleton { get; set; }
    /// <summary>30 SINSU 观测（仅该魔号；null 不落盘）：召唤 adopt + cap_hit 回召（pre-slave xy before/after + recall_moved）。</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public object SinSu { get; set; }
    /// <summary>41 `SKILL_ANGEL` 的 adopt 观测块（`MagicId == 41` 时非 null；镜像 `Skeleton`）。</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public object Angel { get; set; }
    /// <summary>**MG-55**：46 分身的克隆体静态面 + 逐收件人 10/41 帧（仅该魔号；null 不落盘）。</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public object Clone { get; set; }
    /// <summary>**MG-36**：22 火墙的事件表快照 + 安全区/预占输入（仅该魔号；null 不落盘）。</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public object FireCross { get; set; }
    /// <summary>**MG-34**：20 诱惑的术后怪物面 / SlaveList / 60005 投递（仅该魔号；null 不落盘）。</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public object Tamming { get; set; }
    /// <summary>共享尾（49/51/52/53/54）训练面（`SkillTail=true` 才落盘）：Level/TranPoint/TrainLv/MaxTrain[0]。</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public object Train { get; set; }
    /// <summary>三态计时器术后值（只有显式给了 `PoisonStates` 的支才有；`null` 时不落盘）。</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public object StatusTimeArr { get; set; }
}
