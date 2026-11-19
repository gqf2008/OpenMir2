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
        try
        {
        if (args.Contains("--rng-selftest"))
        {
            return RngSelfTest();
        }
        if (args.Contains("--config-selftest"))
        {
            return ConfigSelfTest();
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
            if (name == "lightning11") return AntiMagicBatch(new MagicSpec { Key = "lightning-11", MagicId = 11, MagicName = "Lightening" });
            if (name == "lightning45") return AntiMagicBatch(new MagicSpec { Key = "lightning-45", MagicId = 45, MagicName = "Lightening45" });
            if (name == "healing") return HealingBatch();
            if (name == "firecharm") return AntiMagicBatch(new MagicSpec { Key = "firecharm", MagicId = 13, MagicName = "FireCharm", ArmRingAmulet = true });
            if (name == "hbfireball") return AntiMagicBatch(new MagicSpec { Key = "hbfireball", MagicId = 44, MagicName = "HbFireBall", UserMagicLevel = 3 });
            if (name == "fireball5") return AntiMagicBatch(new MagicSpec { Key = "fireball-5", MagicId = 5, MagicName = "FireBall2" });
            if (name == "hangmajinbub") return BehaviorBatch(new MagicSpec { Key = "hangmajinbub", MagicId = 14, MagicName = "HangMaJinBub", ArmRingAmulet = true });
            if (name == "dejiwonho") return BehaviorBatch(new MagicSpec { Key = "dejiwonho", MagicId = 15, MagicName = "DeJiWonHo", ArmRingAmulet = true });
            if (name == "shield") return BehaviorBatch(new MagicSpec { Key = "shield", MagicId = 31, MagicName = "Shield" });
            if (name == "showhp") return ShowHpBatch(new MagicSpec { Key = "showhp", MagicId = 28, MagicName = "ShowHp" });
            if (name == "cloak") return BehaviorBatch(new MagicSpec { Key = "cloak", MagicId = 18, MagicName = "Cloak", ArmRingAmulet = true });
            if (name == "bighealing") return BigHealingBatch(new MagicSpec { Key = "bighealing", MagicId = 29, MagicName = "BigHealing" });
            if (name == "bigcloak") return BehaviorBatch(new MagicSpec { Key = "bigcloak", MagicId = 19, MagicName = "BigCloak", ArmRingAmulet = true, StepDrainMs = 100 });
            if (name == "fireboom") return BehaviorBatch(new MagicSpec { Key = "fireboom", MagicId = 23, MagicName = "FireBoom" });
            if (name == "snowwind") return BehaviorBatch(new MagicSpec { Key = "snowwind", MagicId = 33, MagicName = "SnowWind" });
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
        copied = new List<string>();
        missing = new List<string>();
        foreach (var n in ConfNames)
        {
            var src = Path.Combine(confDir, n);
            var dst = Path.Combine(baseDir, n);
            if (!File.Exists(src)) { missing.Add(src); continue; }
            // 冻结配置必须非空：0 字节的旧副本（构建产物/读回写坏）会让  全 0 ⇒ 重拷。
            if (!File.Exists(dst) || new FileInfo(dst).Length == 0) { File.Copy(src, dst, overwrite: true); copied.Add(n); }
        }
        return missing.Count == 0;
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
            RunFireball("hit", seedHit, antiMagic, 3000, loginOrder, rngMode, branchExpect: new AntiMagicBranchExpect { Kind = "hit", Threshold = effectiveAnti }),
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
    private static FireballResult RunFireball(string label, int seed, int antiMagic, int idBase, bool loginOrder, string rngMode, bool probeDrainDraw = false, AntiMagicBranchExpect branchExpect = null)
    {
        string mapDir = Environment.GetEnvironmentVariable("MAGICGOLDEN_MAP_DIR") ?? @"E:\MirServer\M2GameSvr\Map";
        var env = new M2Server.Maps.Envirnoment();
        if (!env.LoadMapData(Path.Combine(mapDir, "0.map")))
        {
            throw new InvalidOperationException("MAP_LOAD_FAILED " + mapDir);
        }
        var caster = new RecordingPlayObject { ChrName = "golden-caster", ActorId = idBase + 1, CurrX = 100, CurrY = 100, SoftVersionDateEx = 20240101, ClientTick = 5 };
        caster.Abil.Level = 30;
        caster.WAbil.MC = 15;
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
        Thread.Sleep(700);
        if (probeDrainDraw)
        {
            // 自测探针（不进 golden）：**在真实排水阶段**（`Thread.Sleep` 之后、排水循环之前）抽一次。
            // cover 下 seam 仍安装 ⇒ 入账；freeze 下 seam 已在 DoSpell 后还原 ⇒ 这次抽不被记录。
            _ = M2Server.M2Share.RandomNumber.Random(1000);
        }
        var receivers = new List<ReceiverResult>();
        var pending = new List<object>();
        var byWho = new (string, RecordingPlayObject)[] { ("caster", caster), ("target", target), ("obs_in", obsIn), ("obs_out", obsOut) };
        var rmAcc = new Dictionary<string, List<object>>();
        foreach (var (who, _) in byWho) rmAcc[who] = new List<object>();
        int drainPasses = 0;
        bool drainPassesHitCap = false;
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
                if (!m.Msg.LateDelivery || m.Msg.DeliveryTime <= now) obj.OperatePublic(m.Msg);
                else pending.Add(new { who, ident = m.Msg.wIdent, delivery = m.Msg.DeliveryTime, now });
            }
            rmAcc[who].AddRange(rm);
        }
        drainPasses = pass + 1;
        if (!drainedAny) break;
        if (pass == MaxDrainPasses - 1) drainPassesHitCap = true;
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
private static List<object> ParseFrames(List<(int GateIdx, byte[] Data)> buffers, int gateIdx)
{
    var list = new List<object>();
    foreach (var (g, data) in buffers)
    {
        if (g != gateIdx) continue;
        int packLength = BitConverter.ToInt32(data, 16);
        if (packLength < 0)
        {
            string action = Encoding.GetEncoding("GB2312").GetString(data, ServerMessage.PacketSize, data.Length - ServerMessage.PacketSize);
            list.Add(new { ident = 0, param = 0, tag = 0, series = 0, tail = action });
            continue;
        }
        int off = ServerMessage.PacketSize;
        ushort ident = BitConverter.ToUInt16(data, off + 4);
        ushort param = BitConverter.ToUInt16(data, off + 6);
        ushort tag = BitConverter.ToUInt16(data, off + 8);
        ushort series = BitConverter.ToUInt16(data, off + 10);
        string tail = data.Length > off + 12
            ? Encoding.GetEncoding("GB2312").GetString(data, off + 12, data.Length - off - 12)
            : "";
        list.Add(new { ident, param, tag, series, tail });
    }
    return list;
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
        /// <summary>`construction.skills`（本批写魔号名）。</summary>
        public string[] Skills = Array.Empty<string>();
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
            TargetLevel = spec.TargetLevel,
            TargetMaxHp = spec.TargetMaxHp,
            AntiMagic = spec.AntiMagic,
            ArmRingAmulet = spec.ArmRingAmulet,
            WaitMs = spec.WaitMs,
            CasterHp = 50,
            CasterMaxHp = 100,
        };
        var both = RunMagicCase(bothSpec, "both", 13, 3000, 50, false);
        return EmitGolden(spec, new[] { heal, full, both });
    }

    /// <summary>
    /// 28 心灵启示：门是 `Random(6) <= userMagic.Level + 3`（`MagicManager.cs:434`，**不是**
    /// AntiMagic 骰）。按同一物理意义取三支：miss = 首抽 5（>4）、edge = 首抽 4（==门）、
    /// hit = 首抽 3（<门）。`WaitMs=1600` 跨过 1500ms 的 `RM_DOOPENHEALTH` 自延迟（卡 §25(a)）。
    /// </summary>
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
        return new[]
        {
            RunMagicCase(spec, "miss", seedMiss, 1000, spec.TargetMaxHp, false, new AntiMagicBranchExpect { Kind = "miss", Threshold = threshold }),
            RunMagicCase(spec, "edge", seedEdge, 2000, spec.TargetMaxHp, false, new AntiMagicBranchExpect { Kind = "edge", Threshold = threshold }),
            RunMagicCase(spec, "hit", seedHit, 3000, spec.TargetMaxHp, false, new AntiMagicBranchExpect { Kind = "hit", Threshold = threshold }),
        };
    }

    /// <summary>
    /// 2 治愈：**没有 AntiMagic 骰**，三支按行为分支取（不是 miss/edge/hit）——
    /// `heal`（目标受伤 ⇒ RM_MAGHELING）、`full`（目标满血 ⇒ 无 RM_MAGHELING）、
    /// `self`（targetObject=null ⇒ 自疗，坐标取施法者）。seed 固定 1/5/13。
    /// </summary>
    private static int HealingBatch()
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        var spec = new MagicSpec { Key = "healing", MagicId = 2, MagicName = "Healing" };
        var cases = new[]
        {
            RunMagicCase(spec, "heal", 1, 1000, 50, false),
            RunMagicCase(spec, "full", 5, 2000, 224, false),
            RunMagicCase(spec, "self", 13, 3000, 50, true),
        };
        return EmitGolden(spec, cases);
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
        var cases = new[]
        {
            RunMagicCase(spec, "s1", 1, 1000, spec.TargetMaxHp, false),
            RunMagicCase(spec, "s5", 5, 2000, spec.TargetMaxHp, false),
            RunMagicCase(spec, "s13", 13, 3000, spec.TargetMaxHp, false),
        };
        return EmitGolden(spec, cases);
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

    /// <summary>跑一支魔法（`DoSpell` 直调），返回账本。`targetNull=true` ⇒ 目标对象为 null（治愈 self 支）。</summary>
    private static FireballResult RunMagicCase(
        MagicSpec spec, string label, int seed, int idBase, int targetHp, bool targetNull,
        AntiMagicBranchExpect branchExpect = null)
    {
        string mapDir = Environment.GetEnvironmentVariable("MAGICGOLDEN_MAP_DIR") ?? @"E:\MirServer\M2GameSvr\Map";
        var env = new M2Server.Maps.Envirnoment();
        if (!env.LoadMapData(Path.Combine(mapDir, "0.map")))
        {
            throw new InvalidOperationException("MAP_LOAD_FAILED " + mapDir);
        }
        var caster = new RecordingPlayObject { ChrName = "golden-caster", ActorId = idBase + 1, CurrX = 100, CurrY = 100, SoftVersionDateEx = 20240101, ClientTick = 5 };
        caster.Abil.Level = (byte)spec.CasterLevel;
        caster.WAbil.MC = (ushort)spec.CasterMc;
        caster.WAbil.SC = (ushort)spec.CasterSc;
        if (spec.CasterHp > 0)
        {
            // 29 的「多朋友」支：施法者自身也要 `HP < MaxHP` 才被 `MagBigHealing` 选中。
            caster.WAbil.HP = (ushort)spec.CasterHp;
            caster.WAbil.MaxHP = (ushort)spec.CasterMaxHp;
        }
        var target = new RecordingPlayObject { ChrName = "golden-target", ActorId = idBase + 2, CurrX = 101, CurrY = 100, SoftVersionDateEx = 20240101, ClientTick = 5, AntiMagic = (ushort)spec.AntiMagic };
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
        // **驱动输入（输入/输出语义）**：这四个值在「target 构造完成、RNG/DoSpell 之前」捕获并写入
        // `Fixture`——`2d` 的驱动用它们设 target 初始状态。术后状态（如受击后的 HP）只留在结果顶层
        // `TargetHp`，**不得**回写成 `Fixture` 输入（否则驱动会从术后值起手）。
        int inputTargetLevel = target.Abil.Level;
        int inputTargetHp = target.WAbil.HP;
        int inputTargetMaxHp = target.WAbil.MaxHP;
        int inputAntiMagic = target.AntiMagic;
        CheckBranchThreshold("runmagic/" + spec.Key + "/" + label, inputAntiMagic, branchExpect);
        var obsIn = new RecordingPlayObject { ChrName = "golden-obs-in", ActorId = idBase + 3, CurrX = 105, CurrY = 100, SoftVersionDateEx = 20240101, ClientTick = 5 };
        var obsOut = new RecordingPlayObject { ChrName = "golden-obs-out", ActorId = idBase + 4, CurrX = 140, CurrY = 100, SoftVersionDateEx = 20240101, ClientTick = 5 };
        if (targetNull)
        {
            // 治愈 self 支：`DoSpell` 首句把 targetObject 置成 playObject，读取的是施法者 WAbil。
            caster.WAbil.MaxHP = (ushort)spec.TargetMaxHp;
            caster.WAbil.HP = (ushort)targetHp;
        }
        if (spec.ArmRingAmulet)
        {
            // `CheckAmulet`（`MagicBase.cs:47`）读 `SystemShare.ItemSystem` 的 StdMode/Shape：
            // 离线进程给最小物品系统 + ArmRingl 护身符（Dura=600 ⇒ Round(600/100)=6 ≥ 1）。
            SystemModule.SystemShare.ItemSystem = new M2Server.Items.GameItemSystem();
            SystemModule.SystemShare.ItemSystem.AddItem(new OpenMir2.Data.StdItem
            {
                Name = "护身符",
                StdMode = 25,
                Shape = 5,
            });
            caster.UseItems[OpenMir2.Enums.ItemLocation.ArmRingl] = new OpenMir2.Packets.ClientPackets.UserItem
            {
                MakeIndex = 1,
                Index = 1,
                Dura = 600,
                DuraMax = 1000,
            };
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

        // —— 夹具契约（M51）：construction/derived 由 oracle 复算，七道门任一不过 ⇒ FixtureContractViolation（rc=2）。
        var contract = BuildContractInput(spec, target);
        FixtureContract.Validate(contract);

        var original = RngSeam.Capture();
        var rng = RngSeam.Install(seed);
        var um = new OpenMir2.Packets.ClientPackets.UserMagic
        {
            MagIdx = (ushort)spec.MagicId,
            Level = (byte)spec.UserMagicLevel,
            TranPoint = 0,
            Magic = new OpenMir2.Data.MagicInfo
            {
                MagicId = (ushort)spec.MagicId,
                MagicName = spec.MagicName,
                Effect = (byte)spec.Effect,
                EffectType = (byte)spec.EffectType,
            },
        };
        short doX = targetNull ? caster.CurrX : target.CurrX;
        short doY = targetNull ? caster.CurrY : target.CurrY;
        bool ok = M2Server.Magic.MagicManager.DoSpell(caster, um, doX, doY, targetNull ? null : target);
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
        int drainPasses = 0;
        bool drainPassesHitCap = false;
        if (spec.StepDrainMs > 0)
        {
            // **步进排水**（见 `MagicSpec.StepDrainMs` 说明）：每步只处理**已到期**的消息，
            // 未到期的按原优先级放回队列；RM 账本按消息对象去重（同一消息跨步只记一次）。
            void DrainOnce(bool[] operatedAny)
            {
                foreach (var (who, obj) in byWho)
                {
                    var raw = obj.DrainRaw();
                    var later = new List<(SystemModule.Data.SendMessage Msg, byte Prio)>();
                    foreach (var (m, prio) in raw)
                    {
                        int now = HUtil32.GetTickCount();
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
                            obj.OperatePublic(m);
                            operatedAny[0] = true;
                        }
                        else
                        {
                            later.Add((m, prio));
                        }
                    }
                    foreach (var (m, prio) in later) obj.PushBack(m, prio);
                }
            }
            int elapsed = 0;
            while (elapsed < spec.WaitMs)
            {
                int step = Math.Min(spec.StepDrainMs, spec.WaitMs - elapsed);
                Thread.Sleep(step);
                elapsed += step;
                int stepPasses = 0;
                for (int pass = 0; pass < MaxDrainPasses; pass++)
                {
                    var operatedAny = new[] { false };
                    DrainOnce(operatedAny);
                    stepPasses = pass + 1;
                    if (!operatedAny[0]) break;
                    if (pass == MaxDrainPasses - 1) drainPassesHitCap = true;
                }
                drainPasses = Math.Max(drainPasses, stepPasses);
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
            Thread.Sleep(spec.WaitMs);
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
                        if (!m.Msg.LateDelivery || m.Msg.DeliveryTime <= now) obj.OperatePublic(m.Msg);
                        else pending.Add(new { who, ident = m.Msg.wIdent, delivery = m.Msg.DeliveryTime, now });
                    }
                    rmAcc[who].AddRange(rm);
                }
                drainPasses = pass + 1;
                if (!drainedAny) break;
                if (pass == MaxDrainPasses - 1) drainPassesHitCap = true;
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
        int hpAfter = target.WAbil.HP;
        var receivers = new List<ReceiverResult>();
        foreach (var (who, obj) in byWho)
        {
            receivers.Add(new ReceiverResult { Who = who, Rm = rmAcc[who], Sent = ParseFrames(recorder.Buffers, obj.ActorId) });
        }
        return new FireballResult
        {
            Fixture = new
            {
                magic_id = spec.MagicId,
                magic_name = spec.MagicName,
                effect = spec.Effect,
                effect_type = spec.EffectType,
                user_magic_level = spec.UserMagicLevel,
                caster_level = spec.CasterLevel,
                caster_mc = spec.CasterMc,
                caster_sc = spec.CasterSc,
                rng_mode = spec.RngMode,
                login_order = spec.LoginOrder,
                reachability = spec.Reachability,
                target_level = inputTargetLevel,
                target_hp = inputTargetHp,
                target_max_hp = inputTargetMaxHp,
                anti_magic = inputAntiMagic,
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
                caster_xy = new[] { 100, 100 },
                target_xy = new[] { 101, 100 },
                obs_in_xy = new[] { 105, 100 },
                obs_out_xy = new[] { 140, 100 },
                sockets = new[] { 911, 912, 913, 914 },
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

    private static readonly string[] Unused = Array.Empty<string>();

}

/// <summary>
/// **记录面补全**：`M2Share.NetChannel` 的只记录实现（`INetChannel.AddGateBuffer` 是全部下行
/// 的唯一汇合点，见 `PlayObject.Socket.cs:70/98/128` 三处 SendSocket 重载）。
/// 生产类零改动；`Runner`/网络线程相关成员在离线进程里不会被调用，空实现即可。
/// </summary>
public sealed class RecordingNetChannel : M2Server.Net.INetChannel
{
    public List<(int GateIdx, byte[] Data)> Buffers { get; } = new();

    public void AddGateBuffer(int gateIdx, byte[] senData) => Buffers.Add((gateIdx, senData));

    public void AddGameGateQueue(int gateIdx, ServerMessage packet, byte[] data) => Buffers.Add((gateIdx, data));

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
public sealed class RecordingPlayObject : M2Server.Player.PlayObject
{

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
}

/// <summary>火球单支结果（三支由 `--case fireball` 输出一份 JSON）。</summary>
public sealed class FireballResult
{
    /// 该支的夹具输入（逐字段写进产物，Rust 侧直接读它复现，**禁止手抄**）。
    /// `null` 时**不落盘**（保持火球既有 golden 的 schema 不变）。
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public object Fixture { get; set; }
    public string Label { get; set; }
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
}
