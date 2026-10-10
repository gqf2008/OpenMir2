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
        string corpus = null;
        string axis = "C";
        bool audit = false;
        for (int i = 0; i < args.Length; i++)
        {
            switch (args[i])
            {
                case "--line": line = args[++i]; break;
                case "--script": script = args[++i]; break;
                case "--label": label = args[++i]; break;
                case "--sweep": sweep = args[++i]; break;
                case "--dump": dump = args[++i]; break;
                case "--verify-corpus": corpus = args[++i]; break;
                case "--axis": axis = args[++i]; break;
                case "--axis-audit": audit = true; break;
            }
        }

        if (audit)
        {
            AxisAudit();
            AxisInstanceCheck();
            return 0;
        }

        if (line == null && script == null && sweep == null && corpus == null)
        {
            Console.WriteLine("用法：--line <脚本文本> | --script <路径> [--label @xx] | --sweep <Envir 目录> [--dump f]");
            Console.WriteLine("      | --verify-corpus <dump文件> [--axis C|A]（S3 回归集） | --axis-audit");
            return 2;
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
        if (corpus != null)
        {
            VerifyCorpus(corpus, axis);
            return 0;
        }

        Console.WriteLine("用法：--line <脚本文本> | --script <路径> [--label @xx] | --sweep <Envir 目录>");
        return 2;
    }

    // ---------- 复刻解析器：命令名 → (字段序号, CmdCode, 处理器) ----------

    private static readonly Dictionary<string, int> DefMap = new Dictionary<string, int>(StringComparer.OrdinalIgnoreCase);
    private static readonly Dictionary<string, int> ActionDefMap = new Dictionary<string, int>(StringComparer.OrdinalIgnoreCase);

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
        FieldInfo[] aFields = typeof(ExecutionCode).GetFields();
        for (int i = 0; i < aFields.Length; i++)
        {
            var attr = aFields[i].GetCustomAttribute<ScriptDefName>();
            if (attr != null && !ActionDefMap.ContainsKey(attr.CommandName))
            {
                ActionDefMap.Add(attr.CommandName, i);
            }
        }
    }

    private static Dictionary<string, int> MapOf(string axis) => axis == "A" ? ActionDefMap : DefMap;

    private static bool IsActionSpecial(string cmd)
    {
        // 与 ScriptParsers 动作侧的特判一致（whitelist B-8 记载的五条）
        return cmd.Equals("Set", StringComparison.OrdinalIgnoreCase) || cmd.Equals("ReSet", StringComparison.OrdinalIgnoreCase)
            || cmd.Equals("SetOpen", StringComparison.OrdinalIgnoreCase) || cmd.Equals("SetUnit", StringComparison.OrdinalIgnoreCase)
            || cmd.Equals("ResetUnit", StringComparison.OrdinalIgnoreCase);
    }

    /// <summary>
    /// **现行解析器口径**（与 src/Modules/ScriptEngine/ScriptParsers.cs 同步；S3 起为「存字段序号」）。
    /// 探针复刻这条规则，所以它必须跟着解析器一起改 —— 否则"现状"一栏就失真了。
    /// </summary>
    private static int ParserCmdCode(string axis, string cmd) => MapOf(axis).TryGetValue(cmd, out int c) ? c : 0;

    private static int ParserCmdCode(string cmd) => ParserCmdCode("C", cmd);

    /// <summary>
    /// **S3 之前的口径**（`code - 1`；特判命令存原值）—— 保留它是为了两件事：
    /// ① 出「改前/改后」对照；② 描述 Rust 侧"默认复刻旧位移"时的影响面（whitelist T-4）。
    /// </summary>
    private static int LegacyShiftedCmdCode(string axis, string cmd)
    {
        if (!MapOf(axis).TryGetValue(cmd, out int code))
        {
            return 0;
        }
        bool special = axis == "A" ? IsActionSpecial(cmd)
            : (code == (int)ConditionCode.CHECK || code == (int)ConditionCode.CHECKOPEN || code == (int)ConditionCode.CHECKUNIT);
        return special ? code : code - 1;
    }

    private static int ComputeCmdCode(string cmd) => ParserCmdCode(cmd);

    private static ConditionProcessingSys _axis;
    private static ExecutionProcessingSys _axisExec;

    /// <summary>一张初始化好的注册表（`_conditionMap` 只有 Initialize() 之后才有内容）</summary>
    private static ConditionProcessingSys Axis => _axis ??= CreateInitialized();

    private static ConditionProcessingSys CreateInitialized()
    {
        var sys = new ConditionProcessingSys();
        sys.Initialize();
        return sys;
    }

    private static System.Collections.IDictionary MapFor(string axis)
    {
        if (axis == "A")
        {
            if (_axisExec == null) { _axisExec = new ExecutionProcessingSys(); _axisExec.Initialize(); }
            // 注意：动作侧这张表是 **static** 字段（条件侧是实例字段），反射标志要跟着变
            FieldInfo af = typeof(ExecutionProcessingSys).GetField("ProcessExecutionMessage",
                BindingFlags.NonPublic | BindingFlags.Static | BindingFlags.Instance);
            return (System.Collections.IDictionary)af.GetValue(_axisExec);
        }
        FieldInfo mapField = typeof(ConditionProcessingSys).GetField("_conditionMap", BindingFlags.NonPublic | BindingFlags.Instance);
        return (System.Collections.IDictionary)mapField.GetValue(Axis);
    }

    private static string HandlerName(int cmdCode) => HandlerName("C", cmdCode);

    private static string HandlerName(string axis, int cmdCode)
    {
        // 注册表是私有的 Dictionary<int, Delegate>；取出来看**真实**派发目标
        foreach (System.Collections.DictionaryEntry e in MapFor(axis))
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
        int legacyCode = LegacyShiftedCmdCode("C", cmd);
        Console.WriteLine($"行           : {text.Trim()}");
        Console.WriteLine($"命令         : {cmd}（{(DefMap.ContainsKey(cmd) ? "已注册 [ScriptDefName]" : "未注册 ⇒ 解析器会记脚本错误")}）");
        Console.WriteLine($"CmdCode      : {cmdCode}（现行解析器口径：存字段序号）");
        Console.WriteLine($"派发目标     : {handler}");
        Console.WriteLine($"sParam5      : 长 {p5Len}（该处理器要取 [1]）");
        if (legacyCode != cmdCode)
        {
            Console.WriteLine($"旧位移口径   : CmdCode={legacyCode} ⇒ {HandlerName("C", legacyCode)}（S3 之前 / Rust 现行；whitelist T-4）");
        }
        return Run(text, cmd, cmdCode, handler);
    }

    /// <summary>跑一次指定 CmdCode 的派发，返回异常类别（OOR=越界 / NRE=本探针空 stub / OK / OTHER:xxx）</summary>
    private static string RunAndClassify(string text, int cmdCode)
    {
        string[] words = text.Split(TextSpitConst, StringSplitOptions.RemoveEmptyEntries);
        QuestConditionInfo q = Build(words[0].ToUpperInvariant(), words);
        q.CmdCode = cmdCode;
        var sys = CreateInitialized();
        bool success = false;
        try
        {
            sys.Execute(null, null, q, ref success);
            return "OK";
        }
        catch (IndexOutOfRangeException) { return "OOR"; }
        catch (NullReferenceException) { return "NRE"; }
        catch (Exception ex) { return "OTHER:" + ex.GetType().Name; }
    }

    /// <summary>S3 回归集跑法：逐行对比「现状派发」与「修复口径派发」，并各自跑一遍</summary>
    private static void VerifyCorpus(string file, string axis)
    {
        if (!File.Exists(file)) { Console.WriteLine($"找不到语料文件: {file}"); return; }
        int n = 0, curOor = 0, fixOor = 0, dispatchChanged = 0;
        int fixOk = 0, fixNre = 0, fixOther = 0;
        var rows = new List<string>();
        foreach (string raw in File.ReadAllLines(file))
        {
            string[] parts = raw.Split('\t');
            if (parts.Length < 2) { continue; }
            string text = parts[1].Trim();
            string cmd = text.Split(TextSpitConst, StringSplitOptions.RemoveEmptyEntries).FirstOrDefault()?.ToUpperInvariant();
            if (cmd == null || !MapOf(axis).ContainsKey(cmd)) { continue; }
            n++;
            int legacy = LegacyShiftedCmdCode(axis, cmd);   // S3 之前 / Rust 现行
            int cur = ParserCmdCode(axis, cmd);              // 现行解析器口径
            if (legacy != cur) { dispatchChanged++; }
            string legacyRes = RunAndClassify(text, legacy);
            string curRes = RunAndClassify(text, cur);
            if (legacyRes == "OOR") { curOor++; }
            if (curRes == "OOR") { fixOor++; }
            if (curRes == "OK") { fixOk++; } else if (curRes == "NRE") { fixNre++; } else { fixOther++; }
            rows.Add($"{parts[0]}\t{text}\t旧位移:{HandlerName(axis, legacy)}({legacyRes})\t现行:{HandlerName(axis, cur)}({curRes})");
        }
        Console.WriteLine($"== 回归集 {file}（轴={(axis == "A" ? "动作" : "条件")}）");
        Console.WriteLine($"行数                          : {n}");
        Console.WriteLine($"两口径派发目标不同的行数      : {dispatchChanged}");
        Console.WriteLine($"旧位移口径下抛 IndexOutOfRange: {curOor}   ← S3 修复前的症状 / Rust 现行");
        Console.WriteLine($"现行口径下抛 IndexOutOfRange  : {fixOor}   ← **修复的验收线（应为 0）**");
        Console.WriteLine($"现行口径下：正常返回 {fixOk} / 探针空 stub 致 NRE（不可判）{fixNre} / 其它 {fixOther}");
        Console.WriteLine();
        Console.WriteLine("前 12 行明细（相对路径 | 脚本行 | 旧位移派发(结果) | 现行派发(结果)）：");
        foreach (string r in rows.Take(12)) { Console.WriteLine("    " + r); }
    }

    /// <summary>S3：证明「改执行器查表口径」是无效改法，并给出修复的正确落点</summary>
    private static void AxisAudit()
    {
        AuditOne("C", typeof(ConditionCode), "条件");
        AuditOne("A", typeof(ExecutionCode), "动作");
    }

    private static void AuditOne(string axis, Type enumType, string label)
    {
        FieldInfo[] fields = enumType.GetFields();
        System.Collections.IDictionary map = MapFor(axis);
        int members = 0, idxNeVal = 0, keyDiffer = 0, noKey = 0;
        foreach (FieldInfo f in fields)
        {
            if (f.GetCustomAttribute<ScriptDefName>() == null) { continue; }
            members++;
            int idx = Array.IndexOf(fields, f);
            int val = Convert.ToInt32(f.GetRawConstantValue());
            if (idx != val) { idxNeVal++; }
            bool hasIdx = map.Contains(idx), hasVal = map.Contains(val);
            if (!hasIdx || !hasVal) { noKey++; }
            else if (!Equals(map[idx], map[val])) { keyDiffer++; }
        }
        Console.WriteLine($"[{label}] 成员 {members}；字段序号≠枚举值 {idxNeVal}；两套键（字段序号 vs 枚举值）取值不同的成员 {keyDiffer}；缺键 {noKey}");
        Console.WriteLine($"        ⇒ 字段序号==枚举值 ⇒「把执行器查表改成按字段序号」与现状**完全等价**（键相同），改不动位移；");
        Console.WriteLine($"          且特判命令已存的是字段序号 ⇒ 只有在解析器侧把 code-1 改成 code 才能同时修好普通命令与特判命令。");
    }

    /// <summary>具体实例：修复口径下每个命令必须命中「它自己的」处理器</summary>
    private static void AxisInstanceCheck()
    {
        var expect = new (string axis, string cmd, string handler)[]
        {
            ("C", "CHECKITEMADDVALUE", "ConditionOfCheckItemAddValue"),
            ("C", "CHECKRANGEMONCOUNT", "ConditionOfCheckRangeMonCount"),
            ("C", "CHECKLEVEL", "ConditionOfCheckLevel"),
            ("A", "Give", "ActionOfGiveItem"),
            ("A", "Set", "ActionOfSet"),
        };
        Console.WriteLine("=== 现行口径下的命中检查（命令 → CmdCode → 处理器）");
        foreach (var (axis, cmd, handler) in expect)
        {
            int cur = ParserCmdCode(axis, cmd);
            string got = HandlerName(axis, cur);
            string tag = got == handler ? "OK  " : "FAIL";
            Console.WriteLine($"    {tag} {cmd,-22} CmdCode={cur,-4} 命中={got,-34} 期望={handler}");
        }
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
        int condLines = 0, actLines = 0;
        int condTwoRules = 0, actTwoRules = 0;          // 两口径给不同 CmdCode 的行数 = Rust 现行会错位的行数
        int toCrashHandler = 0, exposed = 0;            // 旧位移口径下落到崩溃处理器、且参数形状必抛
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
                if (first == null) { continue; }
                string cmd = first.ToUpperInvariant();
                bool isCond = DefMap.ContainsKey(cmd), isAct = ActionDefMap.ContainsKey(cmd);
                if (!isCond && !isAct) { continue; }
                string axis = isCond ? "C" : "A";
                int cur = ParserCmdCode(axis, cmd);
                int legacy = LegacyShiftedCmdCode(axis, cmd);
                // 注意：判"有没有被位移"要比 **两套口径算出的 CmdCode**（或两者命中的处理器名），
                // 不能拿处理器名去比命令名 —— 命名法不同，那种比法恒真（本探针早先就踩过，见报告 §5）
                bool shifted = cur != legacy;
                if (isCond) { condLines++; if (shifted) { condTwoRules++; } }
                else { actLines++; if (shifted) { actTwoRules++; } }
                if (axis == "C" && HandlerName("C", legacy) == "ConditionOfCheckRangeMonCount")
                {
                    toCrashHandler++;
                    var (_, _, _, p5Len) = Inspect(t);
                    if (p5Len < 2)
                    {
                        exposed++;
                        string rel = f.Substring(root.Length).TrimStart('\\', '/');
                        all.Add($"{rel}\t{t}");
                        if (samples.Count < 12) { samples.Add($"{Path.GetFileName(f)} :: {t}"); }
                    }
                }
            }
        }
        Console.WriteLine("== 全语料扫描（Envir 递归 *.txt）");
        Console.WriteLine($"语料文件数            : {files.Length}");
        Console.WriteLine($"条件命令行总数        : {condLines}");
        Console.WriteLine($"动作命令行总数        : {actLines}");
        Console.WriteLine($"两口径 CmdCode 不同的行数：条件 {condTwoRules} + 动作 {actTwoRules} = {condTwoRules + actTwoRules}   ← S3 改动的实际影响面 / Rust 现行（T-4）错位面");
        Console.WriteLine("（现行口径下「命中非自己处理器」的行数恒为 0 —— 现行取的就是本命令自己的字段序号；");
        Console.WriteLine("  「确实命中了名字对得上的处理器」由 --axis-audit 的实例检查逐条给出）");
        Console.WriteLine($"旧位移口径下落到 ConditionOfCheckRangeMonCount 的行数 : {toCrashHandler}");
        Console.WriteLine($"其中 sParam5 不足 2 字符（旧口径下**必抛**）的行数 : {exposed}   ← S3 的回归集");
        foreach (string s in samples) { Console.WriteLine($"    {s}"); }
        if (!string.IsNullOrEmpty(dump))
        {
            File.WriteAllLines(dump, all, new UTF8Encoding(false));
            Console.WriteLine($"完整清单（{all.Count} 行，制表符分隔：相对路径<TAB>脚本文本）已写入 : {dump}");
        }
    }

    private static void DumpAxis()
    {
        // 把「命令 → 字段序号 → 现行 CmdCode → 实际处理器」与「旧位移口径」并排打出来
        foreach (string cmd in new[] { "CHECKRANGEMONCOUNT", "CHECKITEMADDVALUE", "CHECKINMAPRANGE", "CHECKLEVEL" })
        {
            int idx = DefMap.TryGetValue(cmd, out int i) ? i : -1;
            int cur = ParserCmdCode("C", cmd);
            int legacy = LegacyShiftedCmdCode("C", cmd);
            Console.WriteLine($"[轴] {cmd,-22} 字段序号={idx,-4} 现行 CmdCode={cur,-4} → {HandlerName("C", cur),-32} 旧位移 CmdCode={legacy,-4} → {HandlerName("C", legacy)}");
        }
        Console.WriteLine();
    }
}
