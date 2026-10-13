// quest-state-probe —— 用 C# oracle 真身实测「玩家任务态位读写」与「CHECKOPEN/CHECKUNIT」向量。
//
// 为什么要有这个探针：`crates/script` 的 `QuestBitmap` 必须与 oracle 的位语义逐字节相同
// （`nFlag -= 1; idx = nFlag/8; bit = 128 >> (nFlag%8)`、越界 no-op、按位清写），而
// `ConditionOfCheckOpen` 起始写的是 `success = false`（同族的 `ConditionCheckUnit` 写 `true`）
// ⇒ CHECKOPEN **恒假**。靠读源码重推极易漏掉这类不对称。本探针直接 `new PlayObject()` +
// 真 `ConditionProcessingSys`，把边界向量产出来。
//
// 产物 = mir2-rs/tests/parity/m3/quest-state-probe/vectors/quest_state_vectors.{json,tsv}
//        + quest_condition_vectors.{json,tsv}
// Rust 侧用例 `crates/script/tests/quest_state_vectors.rs` 逐字段核对（改坏必红）。
//
// 用法（**在仓库根运行**）：
//   dotnet run --project mir2-rs/tests/parity/m3/quest-state-probe -c Release -- <向量目录>
//
// 只读：不连 MySQL、不写任何现网文件、不发起任何 socket。

using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Text;
using System.Text.Json;
using M2Server.Player;
using OpenMir2;
using ScriptSystem.Consts;
using ScriptSystem.Processings;
using Serilog;
using SystemModule;
using SystemModule.Actors;

namespace QuestStateProbe;

internal static class Program
{
    /// 三组任务位图（oracle 同形复制三遍：QuestFlag / QuestUnit / QuestUnitOpen）。
    private static readonly string[] ArrayNames = { "quest_flag", "quest_unit", "quest_unit_open" };

    /// 位读写场景：同一串 op 依次作用在一张全零位图上。
    /// `op = set` 调对应 SetXxxStatus；`op = read` 只读。每步都 dump 原始 128 字节。
    private static readonly (string Op, int Flag, int Value)[] BitOps =
    {
        ("read", 1, 0),
        ("set", 1, 1),
        ("read", 1, 0),
        ("read", 2, 0),
        ("set", 8, 1),
        ("set", 9, 1),
        ("set", 31, 1),
        ("set", 32, 1),
        ("set", 33, 1),
        ("set", 107, 1),
        ("read", 107, 0),
        ("set", 107, 0),
        ("read", 107, 0),
        ("read", 8, 0),   // 清 107 不影响 8/9
        ("read", 9, 0),
        ("set", 255, 1),
        ("set", 256, 1),
        ("read", 255, 0),
        ("read", 256, 0),
        ("set", 1024, 1),
        ("read", 1024, 0),
        ("set", 1025, 1),
        ("read", 1025, 0),
        ("set", 0, 1),
        ("read", 0, 0),
        ("set", -3, 1),
        ("read", -3, 0),
        ("set", 8, 0),
        ("read", 8, 0),
        ("read", 9, 0),   // 仍是 1
        ("set", 1, 0),
        ("read", 1, 0),
    };

    private sealed class BitRow
    {
        public string array { get; set; }
        public string op { get; set; }
        public int flag { get; set; }
        public int value { get; set; }
        public int get { get; set; }
        public string bytes { get; set; }
    }

    private sealed class CondRow
    {
        public string handler { get; set; }
        public int flag { get; set; }
        public int value { get; set; }
        public int status { get; set; }
        public bool success { get; set; }
    }

    private static int Main(string[] args)
    {
        Console.OutputEncoding = Encoding.UTF8;
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        // `M2Share` 静态构造会写日志（`LogService.Info`）⇒ 先装一个空 sink。
        LogService.Logger = new LoggerConfiguration().CreateLogger();
        string outDir = args.Length > 0 ? args[0] : ".";
        Directory.CreateDirectory(outDir);

        var bitRows = new List<BitRow>();
        foreach (string arr in ArrayNames)
        {
            var po = new PlayObject();
            foreach (var (op, flag, value) in BitOps)
            {
                if (op == "set")
                {
                    SetStatus(po, arr, flag, value);
                }
                bitRows.Add(new BitRow
                {
                    array = arr,
                    op = op,
                    flag = flag,
                    value = value,
                    get = GetStatus(po, arr, flag),
                    bytes = Hex(GetBytes(po, arr)),
                });
            }
        }

        var condRows = BuildConditionRows();

        string jsonPath = Path.Combine(outDir, "quest_state_vectors.json");
        var opts = new JsonSerializerOptions { WriteIndented = true };
        WriteLf(jsonPath, JsonSerializer.Serialize(new
        {
            source = "src/M2Server/Player/PlayObject.cs:3966-4110; src/Modules/ScriptEngine/Processings/ConditionProcessingSys.cs:199/4104",
            bit_ops = bitRows,
            condition_rows = condRows,
        }, opts));

        WriteLf(Path.Combine(outDir, "quest_state_vectors.tsv"),
            string.Join("\n", BuildTsv(bitRows)) + "\n");
        WriteLf(Path.Combine(outDir, "quest_condition_vectors.tsv"),
            string.Join("\n", BuildCondTsv(condRows)) + "\n");

        Console.WriteLine($"bit rows      : {bitRows.Count}");
        Console.WriteLine($"condition rows: {condRows.Count}");
        Console.WriteLine(jsonPath);
        return 0;
    }

    private static List<CondRow> BuildConditionRows()
    {
        var rows = new List<CondRow>();
        foreach (int code in new[] { (int)ConditionCode.CHECKUNIT, (int)ConditionCode.CHECKOPEN })
        {
            string handler = code == (int)ConditionCode.CHECKUNIT ? "ConditionCheckUnit" : "ConditionOfCheckOpen";
            foreach (int status in new[] { 0, 1 })
            {
                foreach (int value in new[] { 0, 1 })
                {
                    var sys = new ConditionProcessingSys();
                    sys.Initialize();
                    var info = new QuestConditionInfo
                    {
                        CmdCode = code,
                        sParam1 = "5",
                        sParam2 = value.ToString(),
                    };
                    var player = (IPlayerActor)DispatchProxy.Create<IPlayerActor, QuestPlayerProxy>();
                    var proxy = (QuestPlayerProxy)player;
                    proxy.UnitStatus = status;
                    bool success = true;   // 对齐 ScriptEngine.GotoLableQuestCheckCondition 的初值
                    sys.Execute(null, player, info, ref success);
                    rows.Add(new CondRow
                    {
                        handler = handler,
                        flag = 5,
                        value = value,
                        status = status,
                        success = success,
                    });
                }
            }
        }
        return rows;
    }

    private static string[] BuildTsv(List<BitRow> rows)
    {
        var lines = new List<string> { "# array\top\tflag\tvalue\tget\tbytes_hex" };
        foreach (var r in rows)
        {
            lines.Add($"{r.array}\t{r.op}\t{r.flag}\t{r.value}\t{r.get}\t{r.bytes}");
        }
        return lines.ToArray();
    }

    private static string[] BuildCondTsv(List<CondRow> rows)
    {
        var lines = new List<string> { "# handler\tflag\tvalue\tstatus\tsuccess" };
        foreach (var r in rows)
        {
            lines.Add($"{r.handler}\t{r.flag}\t{r.value}\t{r.status}\t{(r.success ? 1 : 0)}");
        }
        return lines.ToArray();
    }

    private static int GetStatus(PlayObject po, string arr, int flag) => arr switch
    {
        "quest_flag" => po.GetQuestFalgStatus(flag),
        "quest_unit" => po.GetQuestUnitStatus(flag),
        "quest_unit_open" => po.GetQuestUnitOpenStatus(flag),
        _ => throw new ArgumentOutOfRangeException(nameof(arr), arr, null),
    };

    private static void SetStatus(PlayObject po, string arr, int flag, int value)
    {
        switch (arr)
        {
            case "quest_flag": po.SetQuestFlagStatus(flag, value); break;
            case "quest_unit": po.SetQuestUnitStatus(flag, value); break;
            case "quest_unit_open": po.SetQuestUnitOpenStatus(flag, value); break;
            default: throw new ArgumentOutOfRangeException(nameof(arr), arr, null);
        }
    }

    private static byte[] GetBytes(PlayObject po, string arr) => arr switch
    {
        "quest_flag" => po.QuestFlag,
        "quest_unit" => po.QuestUnit,
        "quest_unit_open" => po.QuestUnitOpen,
        _ => throw new ArgumentOutOfRangeException(nameof(arr), arr, null),
    };

    private static string Hex(byte[] bytes) => Convert.ToHexString(bytes).ToLowerInvariant();

    /// <summary>按仓库口径写 **LF**（生产者侧 EOL 门：`verify_sha256sums.py eolcheck` 要求盘上 LF）。</summary>
    private static void WriteLf(string path, string text)
        => File.WriteAllText(path, text.Replace("\r\n", "\n"), new UTF8Encoding(false));
}

/// <summary>
/// `IPlayerActor` 的最小替身：只回答两个任务态取数口（其余方法不会被这两个条件触及）。
/// </summary>
internal class QuestPlayerProxy : DispatchProxy
{
    public int UnitStatus;

    protected override object Invoke(MethodInfo targetMethod, object[] args)
    {
        switch (targetMethod.Name)
        {
            case "GetQuestUnitStatus":
            case "GetQuestUnitOpenStatus":
                return UnitStatus;
            default:
                Type rt = targetMethod.ReturnType;
                if (rt == typeof(int)) return 0;
                if (rt == typeof(byte)) return (byte)0;
                if (rt == typeof(short)) return (short)0;
                if (rt == typeof(bool)) return false;
                if (rt == typeof(string)) return "";
                if (rt.IsValueType) return Activator.CreateInstance(rt);
                return null;
        }
    }
}
