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
        // 帧编码路径会读城堡/攻城区（`PlayObject.GetChrColor` → `SystemShare.CastleMgr.InCastleWarArea`）；
        // 离线进程给一个空城堡管理器（`CastleManager` 参数构造，城堡列表为空 ⇒ 恒返回 null）。
        if (SystemModule.SystemShare.CastleMgr == null)
        {
            SystemModule.SystemShare.CastleMgr = new M2Server.Castle.CastleManager();
        }
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
        target.WAbil.HP = 100;   // 关键：不给 HP 则伤害变化不可观测（账本里 0→0）
        target.Abil.MaxHP = 100;
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
        int hpAfter = target.WAbil.HP;
        foreach (var (who, obj) in byWho)
        {
            receivers.Add(new ReceiverResult { Who = who, Rm = rmAcc[who], Sent = ParseFrames(recorder.Buffers, obj.ActorId) });
        }

        return new FireballResult
        {
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
        int seedMiss = FindSeedForFirstDraw(10, spec.AntiMagic - 1);
        int seedEdge = FindSeedForFirstDraw(10, spec.AntiMagic);
        int seedHit = FindSeedForFirstDraw(10, spec.AntiMagic + 2);
        if (seedMiss < 0 || seedEdge < 0 || seedHit < 0)
        {
            Console.Error.WriteLine("SEED_SCAN_FAILED");
            return 1;
        }
        var cases = new[]
        {
            RunMagicCase(spec, "miss", seedMiss, 1000, spec.TargetMaxHp, false),
            RunMagicCase(spec, "edge", seedEdge, 2000, spec.TargetMaxHp, false),
            RunMagicCase(spec, "hit", seedHit, 3000, spec.TargetMaxHp, false),
        };
        return EmitGolden(spec, cases);
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
            RunMagicCase(spec, "full", 5, 2000, 100, false),
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
        MagicSpec spec, string label, int seed, int idBase, int targetHp, bool targetNull)
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
        var draws = rng.Draws.ToList();
        RngSeam.Restore(original);
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
                target_level = spec.TargetLevel,
                target_hp = targetHp,
                target_max_hp = spec.TargetMaxHp,
                anti_magic = spec.AntiMagic,
                target_null = targetNull,
                arm_ring_amulet = spec.ArmRingAmulet,
                wait_ms = spec.WaitMs,
                caster_hp = spec.CasterHp,
                caster_max_hp = spec.CasterMaxHp,
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
