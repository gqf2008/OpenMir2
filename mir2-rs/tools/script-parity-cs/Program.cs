// B 线脚本引擎对拍 harness（C# 侧）。
// 只读引用 ScriptSystem 参照实现，不修改任何参照代码。
// 口径与 mir2-script-tool parse-stats 完全一致：枚举 Envir 下全部 .txt（按相对路径小写排序），
// 逐文件调 LoadScriptFile(npc, patch, stem, boFlag)，聚合 ScriptList 结构与日志错误。
using System.Reflection;
using System.Text;
using OpenMir2;
using ScriptSystem;
using ScriptSystem.Consts;
using Serilog;
using Serilog.Core;
using Serilog.Events;
using SystemModule;
using SystemModule.Actors;

class CountingSink : ILogEventSink
{
    public int ScriptErrors;      // "脚本错误: "（条件/动作行解析失败）
    public int LoadFails;         // "script error, load fail:"（#CALL / #INCLUDE）
    public int FileNotFound;      // "Script file not found:"
    public int OtherErrors;
    public List<(string File, string Message)> ScriptErrorMessages = new();
    /// 全部 Error 级消息（run 模式的对拍口径：两侧错误文本需逐条一致）
    public List<string> AllErrors = new();
    public string CurrentFile = "";

    public void Emit(LogEvent logEvent)
    {
        if (logEvent.Level < LogEventLevel.Error) return;
        var msg = logEvent.RenderMessage();
        AllErrors.Add(msg);
        if (msg.StartsWith("脚本错误")) { ScriptErrors++; ScriptErrorMessages.Add((CurrentFile, msg)); }
        else if (msg.StartsWith("script error, load fail:")) LoadFails++;
        else if (msg.StartsWith("Script file not found:")) FileNotFound++;
        else OtherErrors++;
    }
}

// DispatchProxy 假 NPC：只实现解析器触达的成员，其余返回默认值。
class NpcProxy : DispatchProxy
{
    public List<ScriptInfo> Scripts = new();
    public Dictionary<string, object> Props = new();

    protected override object Invoke(MethodInfo targetMethod, object[] args)
    {
        var name = targetMethod.Name;
        if (name == "AddScript") { Scripts.Add((ScriptInfo)args[0]); Props["ScriptList"] = Scripts; return null; }
        if (name == "get_ScriptList") return Scripts;
        if (name.StartsWith("get_"))
        {
            var key = name[4..];
            if (Props.TryGetValue(key, out var v)) return v;
            var rt = targetMethod.ReturnType;
            if (rt == typeof(string)) return "";
            if (rt == typeof(int)) return 0;
            if (rt == typeof(bool)) return false;
            if (rt == typeof(short)) return (short)0;
            if (rt.IsGenericType && rt.GetGenericTypeDefinition() == typeof(IList<>))
            {
                var list = Activator.CreateInstance(typeof(List<>).MakeGenericType(rt.GetGenericArguments()[0]));
                Props[key] = list; // 缓存：同一属性多次 get 必须返回同一实例（C# 侧为真实字段）
                return list;
            }
            return rt.IsValueType ? Activator.CreateInstance(rt) : null;
        }
        if (name.StartsWith("set_")) { Props[name[4..]] = args[0]; return null; }
        return null;
    }

    public T Get<T>(string key, T def) => Props.TryGetValue(key, out var v) ? (T)v : def;
}


// 与 Rust crates/script/src/digest.rs 逐字节一致的 FNV-1a 64 结构摘要。
static class StructureDigest
{
    const ulong FnvOffset = 0xcbf29ce484222325UL;
    const ulong FnvPrime = 0x00000100000001b3UL;

    class Hasher
    {
        public ulong State = FnvOffset;
        public void Bytes(byte[] b) { foreach (var x in b) { State ^= x; State *= FnvPrime; } }
        public void Num(int n) => Bytes(Encoding.UTF8.GetBytes(n.ToString(System.Globalization.CultureInfo.InvariantCulture)));
        public void Str(string s) {
            var bytes = Encoding.UTF8.GetBytes(s ?? "");
            Bytes(Encoding.UTF8.GetBytes(bytes.Length.ToString(System.Globalization.CultureInfo.InvariantCulture)));
            Bytes(new byte[] { (byte)':' });
            Bytes(bytes);
        }
        public void Sep() => Bytes(new byte[] { (byte)'|' });
    }

    public static (string Hash, long Scripts, long Records, long Procedures, long Conditions, long Actions, long ElseActions) Of(NpcProxy npc)
    {
        var scripts = npc.Scripts;
        var h = new Hasher();
        long scriptCount = 0, records = 0, procedures = 0, conditions = 0, actions = 0, elseActions = 0;
        foreach (var s in scripts)
        {
            scriptCount++;
            h.Bytes(new byte[] { (byte)'S' }); h.Sep();
            h.Num(s.QuestCount); h.Sep();
            h.Num(s.IsQuest ? 1 : 0); h.Sep();
            foreach (var rec in s.RecordList.Values)
            {
                h.Bytes(new byte[] { (byte)'R' }); h.Sep();
                h.Str(rec.sLabel); h.Sep();
                h.Num(rec.boExtJmp ? 1 : 0); h.Sep();
                records++;
                foreach (var proc in rec.ProcedureList)
                {
                    h.Bytes(new byte[] { (byte)'P' }); h.Sep();
                    h.Str(proc.sSayMsg); h.Sep();
                    h.Str(proc.sElseSayMsg); h.Sep();
                    procedures++;
                    foreach (var c in proc.ConditionList) { Condition(h, c); conditions++; }
                    foreach (var a in proc.ActionList) { Action(h, a, false); actions++; }
                    foreach (var a in proc.ElseActionList) { Action(h, a, true); elseActions++; }
                }
            }
        }
        // ── 商家段（与 Rust digest.rs 的 hash_merchant 逐字节一致） ──
        h.Bytes(new byte[] { (byte)'M' }); h.Sep();
        h.Num((int)npc.Get<object>("PriceRate", 0));
        h.Sep();
        var itemTypes = npc.Get<System.Collections.Generic.IList<int>>("ItemTypeList", new System.Collections.Generic.List<int>());
        h.Num(itemTypes.Count); h.Sep();
        foreach (var v in itemTypes) { h.Num(v); h.Sep(); }
        bool[] flags = {
            npc.Get<bool>("IsBuy", false), npc.Get<bool>("IsSell", false), npc.Get<bool>("IsMakeDrug", false),
            npc.Get<bool>("IsPrices", false), npc.Get<bool>("IsStorage", false), npc.Get<bool>("IsGetback", false),
            npc.Get<bool>("IsUpgradenow", false), npc.Get<bool>("IsGetBackupgnow", false), npc.Get<bool>("IsRepair", false),
            npc.Get<bool>("IsSupRepair", false), npc.Get<bool>("IsSendMsg", false), npc.Get<bool>("IsUseItemName", false),
            npc.Get<bool>("IsOffLineMsg", false), npc.Get<bool>("IsYbDeal", false),
        };
        foreach (var f in flags) h.Bytes(new byte[] { (byte)(f ? '1' : '0') });
        h.Sep();
        var goodsList = npc.Get<System.Collections.Generic.IList<SystemModule.Data.Goods>>("RefillGoodsList", new System.Collections.Generic.List<SystemModule.Data.Goods>());
        h.Num(goodsList.Count); h.Sep();
        foreach (var g in goodsList) { h.Str(g.ItemName); h.Sep(); h.Num(g.Count); h.Sep(); h.Num(g.RefillTime); h.Sep(); }
        return ($"{h.State:x16}", scriptCount, records, procedures, conditions, actions, elseActions);
    }

    static void Condition(Hasher h, QuestConditionInfo c)
    {
        h.Bytes(new byte[] { (byte)'C' }); h.Sep();
        h.Num(c.CmdCode); h.Sep();
        foreach (var p in new[] { c.sParam1, c.sParam2, c.sParam3, c.sParam4, c.sParam5, c.sParam6 }) { h.Str(p); h.Sep(); }
        foreach (var n in new[] { c.nParam1, c.nParam2, c.nParam3, c.nParam4, c.nParam5, c.nParam6 }) { h.Num(n); h.Sep(); }
        h.Str(c.sParam7); h.Sep();
        h.Num(c.nParam7); h.Sep();
        h.Str(c.sOpName); h.Sep();
        h.Str(c.sOpHName); h.Sep();
    }

    static void Action(Hasher h, QuestActionInfo a, bool isElse)
    {
        h.Bytes(new byte[] { (byte)(isElse ? 'E' : 'A') }); h.Sep();
        h.Num(a.nCmdCode); h.Sep();
        foreach (var p in new[] { a.sParam1, a.sParam2, a.sParam3, a.sParam4, a.sParam5, a.sParam6 }) { h.Str(p); h.Sep(); }
        foreach (var n in new[] { a.nParam1, a.nParam2, a.nParam3, a.nParam4, a.nParam5, a.nParam6 }) { h.Num(n); h.Sep(); }
        h.Str(a.sOpName); h.Sep();
        h.Str(a.sOpHName); h.Sep();
    }
}

static class Program
{
    static string Esc(string s)
    {
        var sb = new StringBuilder(s.Length + 8);
        foreach (var c in s)
        {
            switch (c)
            {
                case '"': sb.Append("\\\""); break;
                case '\\': sb.Append("\\\\"); break;
                case '\n': sb.Append("\\n"); break;
                case '\r': sb.Append("\\r"); break;
                case '\t': sb.Append("\\t"); break;
                default:
                    if (c < 0x20) sb.Append($"\\u{(int)c:x4}");
                    else sb.Append(c);
                    break;
            }
        }
        return sb.ToString();
    }

    static int Main(string[] args)
    {
        if (args.Length < 1) { Console.Error.WriteLine("usage: script-parity-cs <Envir目录> [out.json] | run <脚本> <label> [参数]"); return 2; }
        if (args[0] == "run")
        {
            var runSink = new CountingSink();
            LogService.Logger = new LoggerConfiguration().MinimumLevel.Verbose().WriteTo.Sink(runSink).CreateLogger();
            return ScriptParityCs.RunMode.Run(args, runSink);
        }
        var envir = Path.GetFullPath(args[0]);
        var outJson = args.Length > 1 ? args[1] : null;

        var sink = new CountingSink();
        LogService.Logger = new LoggerConfiguration().MinimumLevel.Verbose().WriteTo.Sink(sink).CreateLogger();
        // 与各服务 Program.cs 相同：注册 gb2312 代码页
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        // BasePath 为 harness 自身目录；EnvirDir 用绝对路径让 Path.Combine 直接命中目标
        SystemShare.Config.EnvirDir = envir;
        // 固定随机种子，让重名 label 改名可复现（反射私有静态字段，不改参照代码）
        var rngField = typeof(RandomNumber).GetField("random", BindingFlags.NonPublic | BindingFlags.Static);
        rngField?.SetValue(null, new Random(42));

        var files = Directory.GetFiles(envir, "*.txt", SearchOption.AllDirectories)
            .OrderBy(f => Path.GetRelativePath(envir, f).Replace('\\', '/'), StringComparer.OrdinalIgnoreCase)
            .ToList();

        int loaded = 0, missing = 0, panics = 0;
        long scripts = 0, records = 0, procedures = 0, conditions = 0, actions = 0, elseActions = 0;
        long goods = 0, itemTypeEntries = 0;
        var panicFiles = new List<(string File, string Message)>();
        var digests = new SortedDictionary<string, (string Hash, long S, long R, long P, long C, long A, long E)>(StringComparer.Ordinal);
        var textDigests = new SortedDictionary<string, string>(StringComparer.Ordinal);

        foreach (var file in files)
        {
            var rel = Path.GetRelativePath(envir, file).Replace('\\', '/');
            var parent = (Path.GetDirectoryName(rel) ?? "").Replace('\\', '/');
            var stem = Path.GetFileNameWithoutExtension(file);
            var boFlag = rel.Split('/').Any(seg => seg.Equals("Market_Def", StringComparison.OrdinalIgnoreCase));
            sink.CurrentFile = rel;
            // 逐文件原文摘要（StringList.Text 形态：LoadFromFile 的 ReadLine + AppendLine 复拼）
            {
                var tl = new OpenMir2.Common.StringList();
                tl.LoadFromFile(file);
                ulong th = 0xcbf29ce484222325UL;
                foreach (var b in Encoding.UTF8.GetBytes(tl.Text ?? "")) { th ^= b; th *= 0x00000100000001b3UL; }
                textDigests[rel] = $"{th:x16}";
            }
            var npc = DispatchProxy.Create<IMerchant, NpcProxy>() as NpcProxy;
            long beforeErr = sink.ScriptErrors + sink.LoadFails + sink.FileNotFound + sink.OtherErrors;
            try
            {
                new ScriptParsers().LoadScriptFile((IMerchant)npc, parent, stem, boFlag);
            }
            catch (Exception ex)
            {
                panics++;
                panicFiles.Add((rel, ex.GetType().Name + ": " + ex.Message));
                digests[rel] = ("PANIC", 0, 0, 0, 0, 0, 0);
                continue; // 整文件计数丢弃（与 Rust 侧 Err 口径一致）
            }
            loaded++;
            var scriptList = npc.Scripts;
            scripts += scriptList.Count;
            foreach (var sc in scriptList)
            {
                records += sc.RecordList.Count;
                foreach (var rec in sc.RecordList.Values)
                {
                    procedures += rec.ProcedureList.Count;
                    foreach (var p in rec.ProcedureList)
                    {
                        conditions += p.ConditionList.Count;
                        actions += p.ActionList.Count;
                        elseActions += p.ElseActionList.Count;
                    }
                }
            }
            var d = StructureDigest.Of(npc);
            digests[rel] = (d.Hash, d.Scripts, d.Records, d.Procedures, d.Conditions, d.Actions, d.ElseActions);
            goods += npc.Get<System.Collections.IList>("RefillGoodsList", new List<object>()).Count;
            itemTypeEntries += npc.Get<System.Collections.IList>("ItemTypeList", new List<object>()).Count;
            _ = beforeErr;
        }

        var jsonOut = new StringBuilder();
        jsonOut.AppendLine("{");
        jsonOut.AppendLine($"  \"files\": {files.Count}, \"loaded\": {loaded}, \"missing\": {missing}, \"panics\": {panics},");
        jsonOut.AppendLine("  \"stats\": {");
        jsonOut.AppendLine($"    \"scripts\": {scripts}, \"records\": {records}, \"procedures\": {procedures},");
        jsonOut.AppendLine($"    \"conditions\": {conditions}, \"actions\": {actions}, \"else_actions\": {elseActions},");
        jsonOut.AppendLine($"    \"goods\": {goods}, \"item_type_entries\": {itemTypeEntries},");
        jsonOut.AppendLine($"    \"parse_errors\": {sink.ScriptErrors}, \"load_failures\": {sink.LoadFails}, \"file_not_found\": {sink.FileNotFound}, \"other_errors\": {sink.OtherErrors}");
        jsonOut.AppendLine("  },");
        jsonOut.AppendLine("  \"structure_digest\": {");
        int di = 0;
        foreach (var kv in digests)
        {
            var comma = ++di == digests.Count ? "" : ",";
            jsonOut.AppendLine($"    \"{Esc(kv.Key)}\": {{\"hash\": \"{kv.Value.Hash}\", \"s\": {kv.Value.S}, \"r\": {kv.Value.R}, \"p\": {kv.Value.P}, \"c\": {kv.Value.C}, \"a\": {kv.Value.A}, \"e\": {kv.Value.E}}}{comma}");
        }
        jsonOut.AppendLine("  },");
        jsonOut.AppendLine("  \"text_digest\": {");
        int ti = 0;
        foreach (var kv in textDigests)
        {
            var comma = ++ti == textDigests.Count ? "" : ",";
            jsonOut.AppendLine($"    \"{Esc(kv.Key)}\": \"{kv.Value}\"{comma}");
        }
        jsonOut.AppendLine("  },");
        jsonOut.AppendLine("  \"errors\": [");
        for (int i = 0; i < sink.ScriptErrorMessages.Count; i++)
        {
            var (f, m) = sink.ScriptErrorMessages[i];
            var comma = i + 1 == sink.ScriptErrorMessages.Count ? "" : ",";
            jsonOut.AppendLine($"    {{\"file\": \"{Esc(f)}\", \"message\": \"{Esc(m)}\"}}{comma}");
        }
        jsonOut.AppendLine("  ],");
        jsonOut.AppendLine("  \"panics_detail\": [");
        for (int i = 0; i < panicFiles.Count; i++)
        {
            var comma = i + 1 == panicFiles.Count ? "" : ",";
            jsonOut.AppendLine($"    {{\"file\": \"{Esc(panicFiles[i].File)}\", \"message\": \"{Esc(panicFiles[i].Message)}\"}}{comma}");
        }
        jsonOut.AppendLine("  ]");
        jsonOut.AppendLine("}");
        if (outJson != null) File.WriteAllText(outJson, jsonOut.ToString());

        Console.WriteLine($"Envir: {envir}");
        Console.WriteLine($"文件: {files.Count} 加载: {loaded} 缺失: {missing} 异常: {panics}");
        Console.WriteLine($"脚本 {scripts} 标签 {records} 过程 {procedures} 条件 {conditions} 动作 {actions} 否则动作 {elseActions}");
        Console.WriteLine($"商品 {goods} 物品类型项 {itemTypeEntries}");
        Console.WriteLine($"解析错误: {sink.ScriptErrors} 加载失败: {sink.LoadFails} 文件未找到: {sink.FileNotFound} 其他错误: {sink.OtherErrors}");
        foreach (var (f, m) in panicFiles) Console.WriteLine($"  [Panic] {f}: {m}");
        // 退出码：有解析异常（未被捕获的越界/重复键）→ 非零，便于 CI/脚本判断
        return panics > 0 ? 1 : 0;
    }
}
