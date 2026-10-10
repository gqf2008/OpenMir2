// D2 线 · RNG 同种子注入（.NET Startup Hook）
//
// 用途：把 C# oracle 进程（GameSvr 等）的随机源变成「同种子 + 可记录」，
// 从而让 M3 的「同一操作序列 → 两侧掉落/经验 diff=0」成为可判定对拍，
// 而不是「两次运行本来就不同，只能目测手感」（白名单 D-3）。
//
// **不触碰冻结基线**：本程序集只放进「影子副本」目录，通过环境变量启用：
//   DOTNET_STARTUP_HOOKS=<影子目录>\RngSeedHook.dll
//   MIR2_RNG_SEED=<int>   固定种子（不设则完全不动 oracle 行为）
//   MIR2_RNG_LOG=<路径>   调用流水落盘：序号 \t 调用形态 \t 参数 \t 结果
//   MIR2_RNG_SITE=1       流水追加第 5 列「调用点 文件:行」（依赖 PDB）
//
// 实现要点：
// 1. 反射拿到 `OpenMir2.RandomNumber` 的私有静态字段 `random`（System.Random），
//    换成 `RecordingRandom`（继承 System.Random，逐调用委托给**带种子的基类实例**
//    并记录流水）。System.Random 的 Next()/Next(int)/Next(int,int)/NextDouble()/Sample()
//    都是 virtual，子类能完整拦截取数调用。
// 2. 先调 `RandomNumber.GetInstance()` 建好单例，再覆盖其 `random` 字段，
//    GetInstance 的双检锁不被破坏（后续调用沿用注入的实例）。
// 3. 钩子失败必须**显式非零退出**，不许静默降级——否则「以为注入了种子、
//    其实没有」会污染整批对拍结论。
//
// 注意：.NET 启动钩子要求「全局命名空间下恰名为 StartupHook 的类」，
// 带命名空间会报 TypeLoadException（2026-10-10 实测）。

using System;
using System.Diagnostics;
using System.IO;
using System.Reflection;
using System.Text;

internal static class StartupHook
{
    private static readonly object Sync = new object();
    private static StreamWriter _log;
    private static long _seq;
    private static bool _logSite;

    public static void Initialize()
    {
        string seedText = Environment.GetEnvironmentVariable("MIR2_RNG_SEED");
        if (string.IsNullOrEmpty(seedText))
        {
            return; // 未启用：oracle 行为一字不改
        }
        if (!int.TryParse(seedText, out int seed))
        {
            Fail($"MIR2_RNG_SEED 不是合法 int: {seedText}");
        }
        string logPath = Environment.GetEnvironmentVariable("MIR2_RNG_LOG");
        try
        {
            _logSite = Environment.GetEnvironmentVariable("MIR2_RNG_SITE") == "1";
            if (!string.IsNullOrEmpty(logPath))
            {
                _log = new StreamWriter(
                    new FileStream(logPath, FileMode.Create, FileAccess.Write, FileShare.ReadWrite),
                    new UTF8Encoding(false));
                _log.AutoFlush = true;
            }

            // 1. 先建单例（此时 oracle 自己的 new Random() 被换掉）
            Assembly openMir2 = Assembly.Load("OpenMir2");
            Type rnType = openMir2.GetType("OpenMir2.RandomNumber", throwOnError: true);
            object singleton = rnType
                .GetMethod("GetInstance", BindingFlags.Public | BindingFlags.Static)
                .Invoke(null, null);

            // 2. 覆盖私有静态字段 random
            FieldInfo randomField = rnType.GetField("random", BindingFlags.NonPublic | BindingFlags.Static);
            if (randomField == null)
            {
                Fail("找不到 OpenMir2.RandomNumber.random 私有静态字段（被 trim？）");
            }
            var recording = new RecordingRandom(seed, Record);
            randomField.SetValue(null, recording);

            // 3. 自证：字段确已被替换，并写出该种子的参照序列（不消耗游戏流）
            var probe = (Random)randomField.GetValue(null);
            if (!ReferenceEquals(probe, recording))
            {
                Fail("注入失败：字段未被替换");
            }
            if (singleton == null)
            {
                Fail("GetInstance 返回 null");
            }
            var reference = new Random(seed);
            var sb = new StringBuilder("SEED_PROBE");
            for (int i = 0; i < 8; i++)
            {
                sb.Append(' ').Append(reference.Next());
            }
            if (_log != null)
            {
                _log.WriteLine("1\tINJECT_OK\t" + seed + "\t0");
                _log.WriteLine(sb.ToString());
                _log.Flush();
            }
            Console.WriteLine($"[RngSeedHook] 已注入种子 seed={seed} log={logPath ?? "(none)"} 参照序列={sb}");
        }
        catch (Exception ex)
        {
            Fail($"RngSeedHook 注入异常: {ex}");
        }
    }

    private static void Record(long seqIgnored, string kind, long arg, long result)
    {
        if (_log == null)
        {
            return;
        }
        lock (Sync)
        {
            _seq++;
            if (_logSite)
            {
                _log.WriteLine($"{_seq}\t{kind}\t{arg}\t{result}\t{Site()}");
            }
            else
            {
                _log.WriteLine($"{_seq}\t{kind}\t{arg}\t{result}");
            }
        }
    }

    /// <summary>取第一个「非本钩子、非 RandomNumber 包装层」的调用帧（游戏侧调用点）。</summary>
    private static string Site()
    {
        try
        {
            var st = new StackTrace(2, true);
            for (int i = 0; i < st.FrameCount; i++)
            {
                StackFrame f = st.GetFrame(i);
                MethodBase m = f?.GetMethod();
                if (m == null)
                {
                    continue;
                }
                string asm = m.DeclaringType?.Assembly?.GetName().Name ?? "";
                if (asm == "RngSeedHook" || asm == "OpenMir2")
                {
                    continue;
                }
                string file = Path.GetFileName(f.GetFileName());
                return $"{m.DeclaringType?.FullName}.{m.Name}@{file}:{f.GetFileLineNumber()}";
            }
        }
        catch
        {
            // 调用点记录失败不影响数值对拍
        }
        return "?";
    }

    private static void Fail(string message)
    {
        Console.Error.WriteLine($"[RngSeedHook][FATAL] {message}");
        if (_log != null)
        {
            _log.WriteLine($"FATAL\t{message}");
            _log.Flush();
        }
        Environment.Exit(97);
    }
}

/// <summary>记录型 System.Random：委托给带种子的基类实例，逐调用落盘。</summary>
internal sealed class RecordingRandom : Random
{
    private readonly Random _inner;
    private readonly Action<long, string, long, long> _record;

    public RecordingRandom(int seed, Action<long, string, long, long> record)
    {
        _inner = new Random(seed);
        _record = record;
    }

    public override int Next()
    {
        int v = _inner.Next();
        _record(0, "Next()", 0, v);
        return v;
    }

    public override int Next(int maxValue)
    {
        int v = _inner.Next(maxValue);
        _record(0, "Next(max)", maxValue, v);
        return v;
    }

    public override int Next(int minValue, int maxValue)
    {
        int v = _inner.Next(minValue, maxValue);
        _record(0, "Next(min,max)", ((long)minValue << 32) | (uint)maxValue, v);
        return v;
    }

    public override double NextDouble()
    {
        double v = _inner.NextDouble();
        _record(0, "NextDouble()", 0, BitConverter.DoubleToInt64Bits(v));
        return v;
    }
}
