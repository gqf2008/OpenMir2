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
using System.Text.Json;
using OpenMir2;

namespace MagicGolden;

/// <summary>包装真实 <see cref="Random"/> 的记账器：所有抽取按调用序进账本。</summary>
public sealed class LoggingRandom : Random
{
    public List<long> Draws { get; } = new();

    public LoggingRandom(int seed) : base(seed) { }

    public override int Next()
    {
        int v = base.Next();
        Draws.Add(v);
        return v;
    }

    public override int Next(int maxValue)
    {
        int v = base.Next(maxValue);
        Draws.Add(v);
        return v;
    }

    public override int Next(int minValue, int maxValue)
    {
        int v = base.Next(minValue, maxValue);
        Draws.Add(v);
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
        return logging;
    }

    public static void Restore(Random original) => RandomField.SetValue(null, original);

    public static Random Capture() => (Random)RandomField.GetValue(null);
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
            if (name == "fireball") return FireballCase();
            Console.Error.WriteLine("未知用例: " + name);
            return 2;
        }
        Console.Error.WriteLine("用法: MagicGolden --rng-selftest   （魔法用例生成器在卡 openmir2-magic-parity 后续提交）");
        return 2;
    }

    private static readonly string[] ConfNames = { "Server.conf", "String.conf", "Exps.conf", "Global.conf", "setting.conf" };

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
            if (!File.Exists(dst)) { File.Copy(src, dst); copied.Add(n); }
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
        var cfg = SystemModule.SystemShare.Config;
        var outObj = new
        {
            base_dir = baseDir,
            conf_dir = confDir,
            copied = copied,
            magic_attack_rage = cfg.MagicAttackRage,
            send_ref_msg_range = cfg.SendRefMsgRange,
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
    private static int FireballCase()
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        const int antiMagic = 3;
        int seedMiss = FindSeedForFirstDraw(10, antiMagic - 1);
        int seedEdge = FindSeedForFirstDraw(10, antiMagic);
        int seedHit = FindSeedForFirstDraw(10, antiMagic + 2);
        if (seedMiss < 0 || seedEdge < 0 || seedHit < 0)
        {
            Console.Error.WriteLine("SEED_SCAN_FAILED");
            return 1;
        }
        var cases = new[]
        {
            RunFireball("miss", seedMiss, antiMagic, 1000),
            RunFireball("edge", seedEdge, antiMagic, 2000),
            RunFireball("hit", seedHit, antiMagic, 3000),
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

    /// <summary>用给定种子跑一次火球，返回账本（各接收者的 RM 与真实 SM 帧 + RNG + 关键状态）。</summary>
    private static FireballResult RunFireball(string label, int seed, int antiMagic, int idBase)
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
        target.WAbil.MaxHP = 100;
        target.Abil.MaxHP = 100;
        var obsIn = new RecordingPlayObject { ChrName = "golden-obs-in", ActorId = idBase + 3, CurrX = 105, CurrY = 100, SoftVersionDateEx = 20240101, ClientTick = 5 };
        var obsOut = new RecordingPlayObject { ChrName = "golden-obs-out", ActorId = idBase + 4, CurrX = 140, CurrY = 100, SoftVersionDateEx = 20240101, ClientTick = 5 };
        foreach (var p in new[] { caster, target, obsIn, obsOut })
        {
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
        if (Environment.GetEnvironmentVariable("MAGICGOLDEN_DIAG") == "1")
        {
            Console.Error.WriteLine("DIAG visible=" + string.Join(",", caster.VisibleHumanList) + " envW=" + env.Width + " envH=" + env.Height
                + " get1001=" + (SystemModule.SystemShare.ActorMgr.Get(idBase + 1) != null)
                + " get1003=" + (SystemModule.SystemShare.ActorMgr.Get(idBase + 3) != null)
                + " get1004=" + (SystemModule.SystemShare.ActorMgr.Get(idBase + 4) != null)
                + " obmode=" + caster.ObMode + " fixedhide=" + caster.FixedHideMode + " ghost=" + caster.Ghost);
        }
        var draws = rng.Draws.ToList();
        RngSeam.Restore(original);

        var receivers = new List<ReceiverResult>();
        foreach (var (who, obj) in new (string, RecordingPlayObject)[] { ("caster", caster), ("target", target), ("obs_in", obsIn), ("obs_out", obsOut) })
        {
            var raw = obj.DrainRaw();
            var rm = raw.Select(m => (object)new
            {
                ident = m.wIdent,
                wParam = m.wParam,
                p1 = m.nParam1,
                p2 = m.nParam2,
                p3 = m.nParam3,
                msg = m.Buff ?? "",
                late = m.LateDelivery,
                delivery = m.DeliveryTime,
            }).ToList();
            int now = HUtil32.GetTickCount();
            foreach (var m in raw)
            {
                if (!m.LateDelivery || m.DeliveryTime <= now) obj.OperatePublic(m);
            }
            receivers.Add(new ReceiverResult { Who = who, Rm = rm, Sent = obj.Sent.ToList() });
        }

        return new FireballResult
        {
            Label = label,
            Seed = seed,
            SpellReturn = ok,
            Receivers = receivers,
            Rng = draws,
            TargetHp = target.WAbil.HP,
            CasterMp = caster.WAbil.MP,
        };
    }

    private static readonly string[] Unused = Array.Empty<string>();

}

/// <summary>
/// 只记录不发送的玩家对象：
///  * `SendSocket(CommandMessage, string)` 在 `PlayObject.Socket.cs:101` 是 **virtual** —— 覆写它
///    就拿到**真正要发出去的 SM 帧**（在 `PlayObject.Message.cs` 的 RM→SM 编码与自守卫之后）；
///  * `Operate(ProcessMessage)` 在 `PlayObject.Message.cs:768` 是 protected override —— 暴露一个
///    公共包装，用生产编码路径处理自己队列里的 RM 消息；
///  * `MsgQueue`（`ActorEntity.cs:16`，protected）用于抽 RM 账本。
/// **生产类零改动**。
/// </summary>
public sealed class RecordingPlayObject : M2Server.Player.PlayObject
{
    public List<object> Sent { get; } = new();

    public override void SendSocket(OpenMir2.Packets.ClientPackets.CommandMessage defMsg, string sMsg)
    {
        Sent.Add(new
        {
            ident = defMsg.Ident,
            param = defMsg.Param,
            tag = defMsg.Tag,
            series = defMsg.Series,
            tail = sMsg ?? "",
        });
    }

    public List<SystemModule.Data.SendMessage> DrainRaw()
    {
        var list = new List<SystemModule.Data.SendMessage>();
        while (MsgQueue.TryDequeue(out var m, out _)) list.Add(m);
        return list;
    }

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
    public string Label { get; set; }
    public int Seed { get; set; }
    public bool SpellReturn { get; set; }
    public List<ReceiverResult> Receivers { get; set; }
    public List<long> Rng { get; set; }
    public int TargetHp { get; set; }
    public int CasterMp { get; set; }
}
