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
// M3②③ 活栈隔离口径（owner 2026-10-12 21:15，§6.0-3 owner 签字）：
//   MIR2_RNG_ISOLATE_ATTACK=<开关>  空/0/false/off/no ⇒ 关（大小写不敏感）；
//                                   1/true/on/yes ⇒ 开；其它非空 ⇒ fail-closed。
//     * 开启时，命中 `divert-sites.txt`（= tests/parity/m3/rng-isolate-attack/attack_sites.tsv
//       的 side=csharp, divert=1 行）的调用点，取数委托给一条**独立的 Random(attack_seed)**；
//       其余消费者（怪物 AI/掉落/商人/刷新/构造期…）仍走原全局流。
//     * attack_seed = unchecked(MIR2_RNG_SEED + 0x9E37)（与 Rust 侧同公式）。
//     * 关闭时行为与现在**逐字节相同**（专用流不构造、不被取数）。
//   MIR2_RNG_ISOLATE_SITES=<路径>  静态分流站点 dump 落盘路径（每行一个 Type.Method@file:line，
//                                  即 divert-sites.txt 原样字节；供 check_predicates.py 机判）。
//                                  未设 ⇒ 落在 MIR2_RNG_LOG 同目录的 rng-isolate-sites.txt。
//   MIR2_RNG_ATTACK_LOG=<路径>     可选：只记**分流到专用流**的取数（专用流流水，夹具记其 sha256）。
//   MIR2_RNG_STATE_LOG=<路径>      可选（owner 2026-10-12 23:50）：在每个**玩家攻击起点**
//                                  （`PlayObject.GetAttackPower@PlayObject.cs:2929`，即每个玩家单元首抽）
//                                  把服务端权威 `SystemShare.ActorMgr` 里全部**玩家族**对象的
//                                  `id/name/x/y/dir/ghost/death/hp/maxhp/map` 落成一 JSON 数组一行
//                                  （逐次、与玩家单元同序）。用于把「目标格为何为空」钉到服务端的
//                                  `Ghost/Death/map` 而非客户端视图。未设 ⇒ 零行为变化。
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
using System.Collections;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Reflection;
using System.Text;

internal static class StartupHook
{
    private static readonly object Sync = new object();
    private static StreamWriter _log;
    private static StreamWriter _attackLog;
    private static StreamWriter _stateLog;
    private static long _seq;
    private static long _stateSeq;
    private static bool _logSite;
    private static bool _isolateAttack;

    /// <summary>玩家攻击起点站点（每个玩家攻击单元的首抽；oracle `PlayObject.GetAttackPower`，
    /// `Luck<=0` 分支第一抽）。服务端状态 dump 与玩家单元同序落盘。</summary>
    internal const string AttackStartSite =
        "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2929";

    /// <summary>attack_seed = MIR2_RNG_SEED + 0x9E37（i32 回绕，与 Rust 侧同公式）。</summary>
    internal const int AttackSeedDelta = 0x9E37;

    /// <summary>静态分流站点清单（嵌入 divert-sites.txt）；路由与 dump 的唯一事实源。</summary>
    private static string[] _divertSites;
    private static string _divertSitesRaw;

    /// <summary>attack_seed 公式（i32 回绕）——供自检/夹具与 Rust 侧同口径复算。</summary>
    internal static int AttackSeedFor(int seed)
    {
        return unchecked(seed + AttackSeedDelta);
    }

    /// <summary>分流站点清单（自检用）：嵌入 divert-sites.txt 去注释/空行后的站点数组。</summary>
    internal static string[] DivertSitesForTest()
    {
        string raw;
        return LoadDivertSites(out raw);
    }

    /// <summary>调用点是否属于分流站点（精确匹配清单；null/未登记 ⇒ false）。</summary>
    internal static bool IsDivertSite(string site)
    {
        if (string.IsNullOrEmpty(site))
        {
            return false;
        }
        string raw;
        foreach (string s in LoadDivertSites(out raw))
        {
            if (string.Equals(s, site, StringComparison.Ordinal))
            {
                return true;
            }
        }
        return false;
    }

    public static void Initialize()
    {
        string seedText = Environment.GetEnvironmentVariable("MIR2_RNG_SEED");
        string isolateText = Environment.GetEnvironmentVariable("MIR2_RNG_ISOLATE_ATTACK");
        if (!TryParseIsolateAttack(isolateText, out bool isolate))
        {
            Fail($"MIR2_RNG_ISOLATE_ATTACK 取值非法: '{isolateText}'（合法：空/0/false/off/no 或 1/true/on/yes）");
        }
        if (string.IsNullOrEmpty(seedText))
        {
            if (isolate)
            {
                Fail("MIR2_RNG_ISOLATE_ATTACK 开启但没有 MIR2_RNG_SEED（无种子即无专用流）");
            }
            return; // 未启用：oracle 行为一字不改
        }
        if (!int.TryParse(seedText, out int seed))
        {
            Fail($"MIR2_RNG_SEED 不是合法 int: {seedText}");
        }
        _isolateAttack = isolate;
        int attackSeed = AttackSeedFor(seed);

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

            // 隔离口径：加载静态分流站点清单（嵌入资源）并落盘 dump，构造专用流。
            Random attackRandom = null;
            HashSet<string> divertSet = null;
            string sitesPath = null;
            if (_isolateAttack)
            {
                _divertSites = LoadDivertSites(out _divertSitesRaw);
                divertSet = new HashSet<string>(_divertSites, StringComparer.Ordinal);
                attackRandom = new Random(attackSeed);

                sitesPath = Environment.GetEnvironmentVariable("MIR2_RNG_ISOLATE_SITES");
                if (string.IsNullOrEmpty(sitesPath))
                {
                    sitesPath = !string.IsNullOrEmpty(logPath)
                        ? Path.Combine(Path.GetDirectoryName(Path.GetFullPath(logPath)) ?? ".", "rng-isolate-sites.txt")
                        : Path.Combine(Path.GetTempPath(), "rng-isolate-sites-" + Process.GetCurrentProcess().Id + ".txt");
                }
                // 原样落盘嵌入清单（dump 与仓库内 divert-sites.txt 逐字节相同，便于 hash 溯源）。
                File.WriteAllText(sitesPath, _divertSitesRaw, new UTF8Encoding(false));
                Console.WriteLine($"[RngSeedHook] ISOLATE_ATTACK sites={_divertSites.Length} dump={sitesPath} attack_seed={attackSeed}");

                string attackLogPath = Environment.GetEnvironmentVariable("MIR2_RNG_ATTACK_LOG");
                if (!string.IsNullOrEmpty(attackLogPath))
                {
                    _attackLog = new StreamWriter(
                        new FileStream(attackLogPath, FileMode.Create, FileAccess.Write, FileShare.ReadWrite),
                        new UTF8Encoding(false));
                    _attackLog.AutoFlush = true;
                }
            }

            if (_stateLog == null)
            {
                string statePath = Environment.GetEnvironmentVariable("MIR2_RNG_STATE_LOG");
                if (!string.IsNullOrEmpty(statePath))
                {
                    _stateLog = new StreamWriter(
                        new FileStream(statePath, FileMode.Create, FileAccess.Write, FileShare.ReadWrite),
                        new UTF8Encoding(false));
                    _stateLog.AutoFlush = true;
                    Console.WriteLine($"[RngSeedHook] SERVER_STATE_LOG {statePath}");
                }
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
            var recording = new RecordingRandom(seed, attackRandom, divertSet, _isolateAttack || _logSite || _stateLog != null, Record);
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
                if (_isolateAttack)
                {
                    // 与 Rust 侧 rng_trace.rs 同形的口径行：让流水自证「这次跑的是隔离口径」。
                    // 末段附 dump 路径（无 tab，既有的 site 解析器按前缀跳过，不污染 site 列表）。
                    _log.WriteLine("ISOLATE_ATTACK " + attackSeed + " isolate_sites=" + sitesPath);
                }
                _log.Flush();
            }
            Console.WriteLine($"[RngSeedHook] 已注入种子 seed={seed} log={logPath ?? "(none)"} 参照序列={sb}");
        }
        catch (Exception ex)
        {
            Fail($"RngSeedHook 注入异常: {ex}");
        }
    }

    /// <summary>
    /// 解析 MIR2_RNG_ISOLATE_ATTACK：null/空/0/false/off/no ⇒ 关；1/true/on/yes ⇒ 开；其它非空 ⇒ 非法。
    /// </summary>
    internal static bool TryParseIsolateAttack(string value, out bool isolate)
    {
        isolate = false;
        if (string.IsNullOrEmpty(value))
        {
            return true;
        }
        switch (value.Trim().ToLowerInvariant())
        {
            case "0":
            case "false":
            case "off":
            case "no":
                return true;
            case "1":
            case "true":
            case "on":
            case "yes":
                isolate = true;
                return true;
            default:
                return false;
        }
    }

    /// <summary>读取嵌入的 divert-sites.txt（原样字符串 + 去注释/空行后的站点数组）。</summary>
    private static string[] LoadDivertSites(out string raw)
    {
        Assembly asm = typeof(StartupHook).Assembly;
        string resName = null;
        foreach (string n in asm.GetManifestResourceNames())
        {
            if (n.EndsWith("divert-sites.txt", StringComparison.Ordinal))
            {
                resName = n;
                break;
            }
        }
        if (resName == null)
        {
            Fail("找不到嵌入资源 divert-sites.txt（静态分流站点清单）");
        }
        using (Stream s = asm.GetManifestResourceStream(resName))
        using (var sr = new StreamReader(s, Encoding.UTF8, true))
        {
            raw = sr.ReadToEnd();
        }
        var list = new List<string>();
        foreach (string line in raw.Replace("\r\n", "\n").Split('\n'))
        {
            string t = line.Trim();
            if (t.Length == 0 || t.StartsWith("#", StringComparison.Ordinal))
            {
                continue;
            }
            list.Add(t);
        }
        return list.ToArray();
    }

    private static void Record(string kind, long arg, long result, string site, bool diverted)
    {
        // 服务端权威状态 dump（owner 2026-10-12 23:50）：每个玩家攻击起点一行（与玩家单元同序）。
        if (_stateLog != null && site != null && string.Equals(site, AttackStartSite, StringComparison.Ordinal))
        {
            DumpActors();
        }
        if (diverted && _attackLog != null)
        {
            lock (Sync)
            {
                _attackLog.WriteLine($"{kind}\t{arg}\t{result}\t{site}");
            }
        }
        if (_log == null)
        {
            return;
        }
        lock (Sync)
        {
            _seq++;
            if (_logSite)
            {
                _log.WriteLine($"{_seq}\t{kind}\t{arg}\t{result}\t{site}");
            }
            else
            {
                _log.WriteLine($"{_seq}\t{kind}\t{arg}\t{result}");
            }
        }
    }

    /// <summary>取第一个「非本钩子、非 RandomNumber 包装层」的调用帧（游戏侧调用点）。</summary>
    internal static string Site()
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

    /// <summary>
    /// 把服务端权威 `SystemShare.ActorMgr` 里全部**玩家族**（`Race==0`）对象的
    /// `id/name/x/y/dir/ghost/death/hp/maxhp/map` 落成一行 JSON 数组，前缀 `SERVER_STATE <seq>`：
    /// 用于判定目标格（服务端）上是否还站着一个 `!Ghost && !Death` 的对象。
    /// 全程反射（影子程序集不静态依赖 SystemModule/M2Server），失败只记一行不抛。
    /// </summary>
    private static void DumpActors()
    {
        try
        {
            Assembly sys = Assembly.Load("SystemModule");
            Type ss = sys.GetType("SystemModule.SystemShare", throwOnError: true);
            object mgr = ss.GetProperty("ActorMgr", BindingFlags.Public | BindingFlags.Static)?.GetValue(null);
            if (mgr == null)
            {
                lock (Sync) { _stateSeq++; _stateLog.WriteLine($"SERVER_STATE {_stateSeq} NO_ACTORMGR"); }
                return;
            }
            object map = mgr.GetType().GetField("_actorsMap", BindingFlags.NonPublic | BindingFlags.Instance)?.GetValue(mgr);
            if (map == null)
            {
                lock (Sync) { _stateSeq++; _stateLog.WriteLine($"SERVER_STATE {_stateSeq} NO_ACTORSMAP"); }
                return;
            }
            object values = map.GetType().GetProperty("Values")?.GetValue(map);
            if (values == null)
            {
                lock (Sync) { _stateSeq++; _stateLog.WriteLine($"SERVER_STATE {_stateSeq} NO_VALUES"); }
                return;
            }
            var sb = new StringBuilder();
            sb.Append("[");
            bool first = true;
            foreach (object a in (IEnumerable)values)
            {
                if (a == null)
                {
                    continue;
                }
                Type t = a.GetType();
                object race = GetProp(t, a, "Race");
                if (race == null || Convert.ToInt32(race) != 0)
                {
                    continue; // 只看玩家族（Race==0 == ActorRace.Play）
                }
                object wabil = GetProp(t, a, "WAbil");
                object hp = wabil == null ? null : GetProp(wabil.GetType(), wabil, "HP");
                object maxhp = wabil == null ? null : GetProp(wabil.GetType(), wabil, "MaxHP");
                string name = Convert.ToString(GetProp(t, a, "ChrName") ?? "");
                string mapName = Convert.ToString(GetProp(t, a, "MapName") ?? "");
                bool ghost = Convert.ToBoolean(GetProp(t, a, "Ghost") ?? false);
                bool death = Convert.ToBoolean(GetProp(t, a, "Death") ?? false);
                if (!first)
                {
                    sb.Append(',');
                }
                first = false;
                object id = GetProp(t, a, "ActorId");
                object x = GetProp(t, a, "CurrX");
                object y = GetProp(t, a, "CurrY");
                object dir = GetProp(t, a, "Dir");
                sb.Append("{\"id\":" + id + ",\"name\":\"" + name + "\",\"x\":" + x + ",\"y\":" + y + ",\"dir\":" + dir + ",\"ghost\":" + (ghost ? 1 : 0) + ",\"death\":" + (death ? 1 : 0) + ",\"hp\":" + hp + ",\"maxhp\":" + maxhp + ",\"map\":\"" + mapName + "\"}");
            }
            sb.Append("]");
            lock (Sync)
            {
                _stateSeq++;
                _stateLog.WriteLine($"SERVER_STATE {_stateSeq} {sb}");
            }
        }
        catch (Exception ex)
        {
            lock (Sync)
            {
                _stateSeq++;
                _stateLog.WriteLine($"SERVER_STATE {_stateSeq} DUMP_ERROR {ex.GetType().Name}: {ex.Message}");
            }
        }
    }

    private static object GetProp(Type t, object o, string name)
    {
        try
        {
            return t.GetProperty(name, BindingFlags.Public | BindingFlags.Instance)?.GetValue(o);
        }
        catch
        {
            return null;
        }
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

/// <summary>记录型 System.Random：委托给带种子的基类实例，逐调用落盘；隔离时按调用点分流专用流。</summary>
internal sealed class RecordingRandom : Random
{
    private readonly Random _inner;
    private readonly Random _attack;             // null ⇒ 未隔离（全部走 _inner）
    private readonly HashSet<string> _divert;    // null ⇒ 未隔离
    private readonly bool _needSite;             // 隔离或记 site 时才有必要取调用点
    private readonly Action<string, long, long, string, bool> _record;

    public RecordingRandom(int seed, Random attack, HashSet<string> divert, bool needSite,
                           Action<string, long, long, string, bool> record)
    {
        _inner = new Random(seed);
        _attack = attack;
        _divert = divert;
        _needSite = needSite;
        _record = record;
    }

    private Random Route(out string site, out bool diverted)
    {
        site = _needSite ? StartupHook.Site() : null;
        diverted = _attack != null && site != null && _divert.Contains(site);
        return diverted ? _attack : _inner;
    }

    public override int Next()
    {
        Random r = Route(out string site, out bool diverted);
        int v = r.Next();
        _record("Next()", 0, v, site, diverted);
        return v;
    }

    public override int Next(int maxValue)
    {
        Random r = Route(out string site, out bool diverted);
        int v = r.Next(maxValue);
        _record("Next(max)", maxValue, v, site, diverted);
        return v;
    }

    public override int Next(int minValue, int maxValue)
    {
        Random r = Route(out string site, out bool diverted);
        int v = r.Next(minValue, maxValue);
        _record("Next(min,max)", ((long)minValue << 32) | (uint)maxValue, v, site, diverted);
        return v;
    }

    public override double NextDouble()
    {
        Random r = Route(out string site, out bool diverted);
        double v = r.NextDouble();
        _record("NextDouble()", 0, BitConverter.DoubleToInt64Bits(v), site, diverted);
        return v;
    }
}
