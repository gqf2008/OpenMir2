using System.Reflection;
using System.Text;
using ScriptSystem.Consts;
using ScriptSystem.Processings;
using SystemModule;

namespace CondCrashProbe;

/// <summary>
/// S2 最小夹具复现：`ConditionOfCheckRangeMonCount` 里 `questConditionInfo.sParam5[1]` 的
/// `String.get_Chars` 越界（`IndexOutOfRangeException`）。
///
/// 复现链（全部用生产代码，不自造样本）：
///   ① 解析器把条件命令名映射成 `CmdCode`：扫 `typeof(ConditionCode).GetFields()`，
///      `[ScriptDefName]` 命中记下**字段序号 i**，非特判命令取 `CmdCode = i - 1`（B-8 的位移）；
///   ② 参数按空白切 6 段，缺的留 `string.Empty`；
///   ③ `ConditionProcessingSys.Execute` 用 `_conditionMap[CmdCode]` 取处理器 —— 位移后
///      打到的是**前一个枚举成员**的处理器；
///   ④ 该处理器第 5 个参数取 `sParam5[1]`：参数不足 5 个 ⇒ 空串 ⇒ 索引 1 越界。
///
/// 用法：
///   CondCrashProbe --line "CHECKITEMADDVALUE 3 10 = 1"             # 单行复现
///   CondCrashProbe --script <path> --label @rw03                   # 真脚本 label 的 #IF 全跑一遍
///   CondCrashProbe --sweep <Envir 根目录>                          # 扫真实语料，统计暴露面
/// </summary>
internal static class Program
{
    private static readonly char[] TextSpitConst = { ' ', '\t' };

    private static async Task<int> Main(string[] args)
    {
        Console.OutputEncoding = Encoding.UTF8;
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);

        string line = null;
        string script = null;
        string label = null;
        string sweep = null;
        string dump = null;
        for (int i = 0; i < args.Length; i++)
        {
            switch (args[i])
            {
                case "--line": line = args[++i]; break;
                case "--script": script = args[++i]; break;
                case "--label": label = args[++i]; break;
                case "--sweep": sweep = args[++i]; break;
                case "--dump": dump = args[++i]; break;
            }
        }

        DumpAxis();

        if (line != null)
        {
            return AnalyzeLine(line) ? 0 : 1;
        }
        if (script != null)
        {
            return AnalyzeLabel(script, label ?? "@main") ? 0 : 1;
        }
        if (sweep != null)
        {
            Sweep(sweep, dump);
            return 0;
        }

        Console.WriteLine("用法：--line <脚本文本> | --script <路径> [--label @xx] | --sweep <Envir 目录>");
        return 2;
    }

    // ---------- 复刻解析器：命令名 → (字段序号, CmdCode, 处理器) ----------

    private static readonly Dictionary<string, int> DefMap = new Dictionary<string, int>(StringComparer.OrdinalIgnoreCase);

    static Program()
    {
        // 与 ScriptParsers 的构造逐行同构：序号 i 是 GetFields() 的下标（含 value__）
        FieldInfo[] fields = typeof(ConditionCode).GetFields();
        for (int i = 0; i < fields.Length; i++)
        {
            var attr = fields[i].GetCustomAttribute<ScriptDefName>();
            if (attr != null && !DefMap.ContainsKey(attr.CommandName))
            {
                DefMap.Add(attr.CommandName, i);
            }
        }
    }

    private static int ComputeCmdCode(string cmd)
    {
        if (!DefMap.TryGetValue(cmd, out int code))
        {
            return 0;
        }
        // ScriptParsers 的特判：CHECK / CHECKOPEN / CHECKUNIT 存原值，其余存 code-1
        if (code == (int)ConditionCode.CHECK || code == (int)ConditionCode.CHECKOPEN || code == (int)ConditionCode.CHECKUNIT)
        {
            return code;
        }
        return code - 1;
    }

    private static ConditionProcessingSys _axis;

    /// <summary>一张初始化好的注册表（`_conditionMap` 只有 Initialize() 之后才有内容）</summary>
    private static ConditionProcessingSys Axis => _axis ??= CreateInitialized();

    private static ConditionProcessingSys CreateInitialized()
    {
        var sys = new ConditionProcessingSys();
        sys.Initialize();
        return sys;
    }

    private static string HandlerName(int cmdCode)
    {
        // _conditionMap 是私有的 Dictionary<int, ScriptCondition>；取出来看**真实**派发目标
        FieldInfo mapField = typeof(ConditionProcessingSys).GetField("_conditionMap", BindingFlags.NonPublic | BindingFlags.Instance);
        var map = (System.Collections.IDictionary)mapField.GetValue(Axis);
        foreach (System.Collections.DictionaryEntry e in map)
        {
            if ((int)e.Key == cmdCode)
            {
                return ((Delegate)e.Value).Method.Name;
            }
        }
        return "(未注册)";
    }

    /// <summary>按解析器的切分规则造 QuestConditionInfo（本探针只用到 sParam1..6 与 CmdCode）。</summary>
    private static QuestConditionInfo Build(string cmd, string[] words)
    {
        var q = new QuestConditionInfo();
        string[] p = new string[7];
        for (int i = 0; i < 7; i++) { p[i] = i < words.Length ? words[i] : string.Empty; }
        q.sParam1 = p[1]; q.sParam2 = p[2]; q.sParam3 = p[3];
        q.sParam4 = p[4]; q.sParam5 = p[5]; q.sParam6 = p[6];
        q.CmdCode = ComputeCmdCode(cmd);
        return q;
    }

    /// <summary>只看不跑：算出这一行的派发去向与 sParam5 长度（用于扫全语料）。</summary>
    private static (string cmd, int cmdCode, string handler, int p5Len) Inspect(string text)
    {
        string[] words = text.Split(TextSpitConst, StringSplitOptions.RemoveEmptyEntries);
        if (words.Length == 0) { return (null, 0, null, 0); }
        string cmd = words[0].ToUpperInvariant();
        QuestConditionInfo q = Build(cmd, words);
        return (cmd, q.CmdCode, HandlerName(q.CmdCode), (q.sParam5 ?? string.Empty).Length);
    }

    // ---------- 三种运行模式 ----------

    private static bool AnalyzeLine(string text)
    {
        var (cmd, cmdCode, handler, p5Len) = Inspect(text);
        Console.WriteLine($"行           : {text.Trim()}");
        Console.WriteLine($"命令         : {cmd}（{(DefMap.ContainsKey(cmd) ? "已注册 [ScriptDefName]" : "未注册 ⇒ 解析器会记脚本错误")}）");
        Console.WriteLine($"CmdCode      : {cmdCode}（解析器存的就是这个值）");
        Console.WriteLine($"派发目标     : {handler}{(handler == cmd ? "" : "   ← 与命令名不同：B-8 位移")}");
        Console.WriteLine($"sParam5      : 长 {p5Len}（该处理器要取 [1]）");
        return Run(text, cmd, cmdCode, handler);
    }

    private static bool Run(string text, string cmd, int cmdCode, string handler)
    {
        string[] words = text.Split(TextSpitConst, StringSplitOptions.RemoveEmptyEntries);
        QuestConditionInfo q = Build(cmd, words);
        var sys = new ConditionProcessingSys();
        sys.Initialize();
        bool success = false;
        try
        {
            sys.Execute(null, null, q, ref success);
            Console.WriteLine($"结果         : 未抛异常（success={success}）");
            return true;
        }
        catch (Exception ex)
        {
            Console.WriteLine($"结果         : {ex.GetType().Name}: {ex.Message}");
            string frame = (ex.StackTrace ?? string.Empty).Split('\n').FirstOrDefault(l => l.Contains("ConditionProcessingSys.cs"));
            Console.WriteLine($"抛出点       : {frame?.Trim()}");
            Console.WriteLine("               （Release 下行号归属会落到方法体首行附近；越界语句本身是方法里第 5 个参数那句 `sParam5[1]`）");
            if (ex is NullReferenceException)
            {
                // 本探针传的是 null 的 normNpc / playerActor：NRE 很可能只是 stub 造成的，
                // 不能当成"生产也会崩"的证据 —— 只有参数形状决定的异常（越界/格式）才算。
                Console.WriteLine("               ⚠ 这是 NRE：很可能是本探针的空 stub 造成，**不可**据此判生产崩溃");
            }
            return false;
        }
    }

    private static bool AnalyzeLabel(string path, string label)
    {
        if (!File.Exists(path)) { Console.WriteLine($"找不到脚本: {path}"); return false; }
        string[] lines = File.ReadAllLines(path, Encoding.GetEncoding("gb2312"));
        // 段头是 [@rw03]，调用方传 @rw03 —— 两侧都归一化再比
        string want = label.Trim().Trim('[', ']');
        bool inLabel = false;
        int condIdx = -1, condCount = 0, crashed = 0, evaluated = 0;
        Console.WriteLine($"== label {want} @ {path}");
        for (int i = 0; i < lines.Length; i++)
        {
            string raw = lines[i].Trim();
            if (raw.StartsWith("[") && raw.EndsWith("]"))
            {
                inLabel = string.Equals(raw.Trim('[', ']'), want, StringComparison.OrdinalIgnoreCase);
                continue;
            }
            if (!inLabel || raw.Length == 0) { continue; }
            if (raw.StartsWith("#"))
            {
                condIdx = raw.Equals("#IF", StringComparison.OrdinalIgnoreCase) ? i : -1;
                condCount = 0;
                continue;
            }
            if (condIdx >= 0)
            {
                condCount++;
                evaluated++;
                Console.WriteLine($"--- 第 {i + 1} 行（#IF 块内第 {condCount} 条）");
                if (!AnalyzeLine(raw)) { crashed++; }
                Console.WriteLine();
            }
        }
        Console.WriteLine($"== 该 label 的 #IF 条件共 {evaluated} 条，其中 {crashed} 条抛异常");
        return crashed == 0;
    }

    private static void Sweep(string root, string dump)
    {
        string[] files = Directory.GetFiles(root, "*.txt", SearchOption.AllDirectories);
        int totalCond = 0, shifted = 0, toCrashHandler = 0, exposed = 0;
        var samples = new List<string>();
        var all = new List<string>();
        Encoding gbk = Encoding.GetEncoding("gb2312");
        foreach (string f in files)
        {
            foreach (string rawLine in File.ReadAllLines(f, gbk))
            {
                string t = rawLine.Trim();
                if (t.Length == 0 || t.StartsWith(";")) { continue; }
                string first = t.Split(TextSpitConst, StringSplitOptions.RemoveEmptyEntries).FirstOrDefault();
                if (first == null || !DefMap.ContainsKey(first.ToUpperInvariant())) { continue; }
                totalCond++;
                var (cmd, cmdCode, handler, p5Len) = Inspect(t);
                if (handler != cmd) { shifted++; }
                if (handler == nameof(ConditionProcessingSys) + "." || handler == "ConditionOfCheckRangeMonCount")
                {
                    toCrashHandler++;
                    if (p5Len < 2)
                    {
                        exposed++;
                        string rel = f.Substring(root.Length).TrimStart('\\', '/');
                        all.Add($"{rel}\t{t}");
                        if (samples.Count < 12)
                        {
                            samples.Add($"{Path.GetFileName(f)} :: {t}");
                        }
                    }
                }
            }
        }
        Console.WriteLine("== 全语料扫描（Envir 递归 *.txt）");
        Console.WriteLine($"语料文件数            : {files.Length}");
        Console.WriteLine($"条件命令行总数        : {totalCond}");
        Console.WriteLine($"派发被位移掉的行数    : {shifted}（B-8 位移面）");
        Console.WriteLine($"落到 ConditionOfCheckRangeMonCount 的行数 : {toCrashHandler}");
        Console.WriteLine($"其中 sParam5 不足 2 字符（**必抛**）的行数 : {exposed}");
        foreach (string s in samples) { Console.WriteLine($"    {s}"); }
        if (!string.IsNullOrEmpty(dump))
        {
            File.WriteAllLines(dump, all, new UTF8Encoding(false));
            Console.WriteLine($"完整清单（{all.Count} 行，制表符分隔：相对路径<TAB>脚本文本）已写入 : {dump}");
        }
    }

    private static void DumpAxis()
    {
        // 把「命令 → 字段序号 → CmdCode → 实际处理器」的表打出来，证明位移方向与落点
        foreach (string cmd in new[] { "CHECKRANGEMONCOUNT", "CHECKITEMADDVALUE", "CHECKINMAPRANGE", "CHECKLEVEL" })
        {
            int code = DefMap.TryGetValue(cmd, out int i) ? i : -1;
            int cmdCode = ComputeCmdCode(cmd);
            Console.WriteLine($"[轴] {cmd,-22} 字段序号={code,-4} CmdCode={cmdCode,-4} 处理器={HandlerName(cmdCode)}");
        }
        Console.WriteLine();
    }
}
