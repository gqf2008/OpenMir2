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
    public string CurrentFile = "";

    public void Emit(LogEvent logEvent)
    {
        if (logEvent.Level < LogEventLevel.Error) return;
        var msg = logEvent.RenderMessage();
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
        if (args.Length < 1) { Console.Error.WriteLine("usage: script-parity-cs <Envir目录> [out.json]"); return 2; }
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

        foreach (var file in files)
        {
            var rel = Path.GetRelativePath(envir, file).Replace('\\', '/');
            var parent = (Path.GetDirectoryName(rel) ?? "").Replace('\\', '/');
            var stem = Path.GetFileNameWithoutExtension(file);
            var boFlag = rel.Split('/').Any(seg => seg.Equals("Market_Def", StringComparison.OrdinalIgnoreCase));
            sink.CurrentFile = rel;
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
        return 0;
    }
}
