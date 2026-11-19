// quest-parity-probe — QT-01 `{Quest N}` 选择门可达性探针（卡 openmir2-quests-parity）。
//
// 判什么：oracle 的 `CheckGotoLableQuestStatus` 只在 `ScriptInfo.IsQuest` 为真时遍历 QuestInfo；
// 而 `IsQuest = true` 只出现在 `ScriptParsers.cs` 的 `if (scriptType == 1 && ...)` 块内，
// 该局部变量在 `LoadScriptFile` 内从未被赋 1。本探针用**真** `ScriptParsers.LoadScriptFile`
// 装载含 `{Quest` 头的脚本，先证明脚本真的装进来了，再断言 `IsQuest == false`。
//
// 防空转（QA 清单 Z3）：`LOADED` 三条证明（计数 +1 / 记录非空 / 已知标签命中）任一不成立即判 FAIL，
// 不许把「没装进来」当成「IsQuest 为假」的证据；0 用例 ⇒ rc=2。
//
// 用法（仓库根）：dotnet run --project mir2-rs/tests/parity/m3/quests/quest-parity-probe -c Release -- <产物目录>
// 只读：不连 MySQL、不写现网文件、不发 socket；脚本落探针自己的 bin/Envir/QuestProbe/ 与产物目录。

using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Text;
using M2Server.Npc;
using OpenMir2;
using ScriptSystem;
using Serilog;
using SystemModule;

namespace QuestParityProbe;

internal static class Program
{
    private const string Patch = "QuestProbe";

    private sealed class ProbeCase
    {
        public string Id;
        public string Name;
        public string KnownLabel;
        public string Text;
    }


    /// 判定「这条脚本是否证明死分支」：返回非空即 FAIL。正反两条路径共用同一判据。
    private static List<string> Why(ScriptInfo si, int records, bool labelHit)
    {
        var why = new List<string>();
        if (records <= 0) why.Add("RECORDS=0");
        if (!labelHit) why.Add("LABEL_MISS");
        if (si.IsQuest) why.Add("IS_QUEST_TRUE");
        if (si.QuestInfo != null) why.Add("QUEST_INFO_ALLOCATED");
        return why;
    }

    /// 反向牙：人为造一个 `IsQuest = true` 的 ScriptInfo 与一份空记录，判据必须报出来。
    /// 判据若退化成恒空（假绿），本模式 rc=1。
    private static int NegativeControl()
    {
        var siTrue = new ScriptInfo
        {
            IsQuest = true,
            QuestInfo = new ScriptQuestInfo[10],
            RecordList = new Dictionary<string, SayingRecord>(StringComparer.OrdinalIgnoreCase),
        };
        siTrue.RecordList["@main"] = new SayingRecord { sLabel = "@main" };
        var whyTrue = Why(siTrue, siTrue.RecordList.Count, true);

        var siEmpty = new ScriptInfo { IsQuest = false, RecordList = null };
        var whyEmpty = Why(siEmpty, 0, false);

        bool ok = whyTrue.Contains("IS_QUEST_TRUE")
                  && whyTrue.Contains("QUEST_INFO_ALLOCATED")
                  && whyEmpty.Contains("RECORDS=0")
                  && whyEmpty.Contains("LABEL_MISS");
        Console.WriteLine("NEGATIVE_CONTROL is_quest_true=>[{0}] empty=>[{1}] verdict={2}",
            string.Join("+", whyTrue), string.Join("+", whyEmpty), ok ? "RED(runtime)" : "GREEN_FALSE");
        return ok ? 0 : 1;
    }

    private static int Main(string[] args)
    {
        Console.OutputEncoding = Encoding.UTF8;
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        LogService.Logger = new LoggerConfiguration().CreateLogger();

        string outDir = args.Length > 0 ? Path.GetFullPath(args[0]) : Directory.GetCurrentDirectory();
        Directory.CreateDirectory(outDir);

        var cases = new[]
        {
            new ProbeCase
            {
                Id = "QT01-A",
                Name = "quest_probe_a",
                KnownLabel = "@other",
                Text = "{Quest 1}\n[@main]\n#SAY\nprobe main a\n[@other]\n#SAY\nprobe other a\n",
            },
            new ProbeCase
            {
                Id = "QT01-B",
                Name = "quest_probe_b",
                KnownLabel = "@main",
                Text = "{Quest 2}\n[@main]\n#IF\nCHECK [1] 1\n#ACT\nSET [2] 1\n#SAY\nprobe b\n",
            },
        };

        if (cases.Length == 0)
        {
            Console.Error.WriteLine("ZERO_CASES");
            return 2;
        }

        if (Array.IndexOf(args, "--negative-control") >= 0)
        {
            return NegativeControl();
        }

        var npc = new NormNpc();
        var parser = new ScriptParsers();
        var lines = new List<string>
        {
            "# case_id\tfile\tloaded_delta\trecords\tknown_label\tknown_label_hit\tis_quest\tquest_info_null\tquest_count\tverdict"
        };
        var failures = new List<string>();
        int passed = 0;

        string scriptsOut = Path.Combine(outDir, "scripts_applied");
        Directory.CreateDirectory(scriptsOut);

        foreach (ProbeCase c in cases)
        {
            string path = SystemShare.GetEnvirFilePath(Patch, c.Name + ".txt");
            Directory.CreateDirectory(Path.GetDirectoryName(path));
            string lfText = c.Text.Replace("\r\n", "\n");
            File.WriteAllText(path, lfText, new UTF8Encoding(false));
            File.WriteAllText(Path.Combine(scriptsOut, c.Name + ".txt"), lfText, new UTF8Encoding(false));

            int before = npc.ScriptList.Count;
            parser.LoadScriptFile(npc, Patch, c.Name, false);
            int after = npc.ScriptList.Count;
            int delta = after - before;

            if (delta != 1)
            {
                failures.Add(c.Id + ":LOAD_DELTA=" + delta.ToString(CultureInfo.InvariantCulture));
                lines.Add(string.Join("\t", c.Id, c.Name, delta.ToString(CultureInfo.InvariantCulture),
                    "-", c.KnownLabel, "0", "-", "-", "-", "FAIL_LOAD"));
                Console.WriteLine("FAIL {0} 装载失败：ScriptList {1}->{2}（脚本可能存在但没被解析成 ScriptInfo）", c.Id, before, after);
                continue;
            }

            ScriptInfo si = npc.ScriptList[after - 1];
            int records = si.RecordList == null ? -1 : si.RecordList.Count;
            bool labelHit = si.RecordList != null && si.RecordList.ContainsKey(c.KnownLabel);
            bool isQuest = si.IsQuest;
            bool questInfoNull = si.QuestInfo == null;

            Console.WriteLine("LOADED {0} file={1} records={2} known_label={3} hit={4} quest_count={5}",
                c.Id, c.Name, records, c.KnownLabel, labelHit ? 1 : 0, si.QuestCount);

            var why = Why(si, records, labelHit);

            string verdict = why.Count == 0 ? "PASS(dead_in_oracle)" : "FAIL(" + string.Join("+", why) + ")";
            if (why.Count == 0) passed++; else failures.Add(c.Id + ":" + string.Join("+", why));

            lines.Add(string.Join("\t", c.Id, c.Name, delta.ToString(CultureInfo.InvariantCulture),
                records.ToString(CultureInfo.InvariantCulture), c.KnownLabel, labelHit ? "1" : "0",
                isQuest ? "1" : "0", questInfoNull ? "1" : "0",
                si.QuestCount.ToString(CultureInfo.InvariantCulture), verdict));
        }

        string ledger = Path.Combine(outDir, "quest_gate_reachability.tsv");
        File.WriteAllText(ledger, string.Join("\n", lines) + "\n", new UTF8Encoding(false));

        Console.WriteLine("REACHABILITY cases={0} passed={1} failed={2}", cases.Length, passed, failures.Count);
        Console.WriteLine(ledger);
        if (failures.Count > 0)
        {
            Console.Error.WriteLine("FAIL_DETAIL " + string.Join("; ", failures));
            return 1;
        }
        return 0;
    }
}
