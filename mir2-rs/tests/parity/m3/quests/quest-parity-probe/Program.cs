// quest-parity-probe — QT-01 `{Quest N}` 选择门可达性/门控判决探针（卡 openmir2-quests-parity）。
//
// 两件事：
//   1. 可达性：oracle 的 `CheckGotoLableQuestStatus` 只在 `ScriptInfo.IsQuest` 为真时遍历 QuestInfo；
//      而 `IsQuest = true` 只出现在 `ScriptParsers.cs` 的 `if (scriptType == 1 && ...)` 块内，
//      该局部变量在 `LoadScriptFile` 内从未被赋 1。本探针用**真** `ScriptParsers.LoadScriptFile`
//      装载夹具脚本，先证明脚本真的装进来了，再断言 `IsQuest == false`。
//   2. 门控判决（`--gate`）：反射调用 oracle 的私有静态 `CheckGotoLableQuestStatus`，
//      对「真实解析出的 ScriptInfo」与「人为把门臂装上（IsQuest=true）」各跑一遍真值表——
//      真实解析那份必须返回 true（不门控），装上臂那份必须能返回 false（判据可红，不是恒真）。
//
// 防空转（QA 清单 Z1/Z3）：0 用例 ⇒ rc=2；夹具缺失 ⇒ rc=2（PREREQ，Z2）；
// `LOADED` 三条证明（计数 +1 / 记录非空 / 已知标签命中）任一不成立即 FAIL，不许把「没装进来」当成证据。
//
// 用法（仓库根）：dotnet run --project mir2-rs/tests/parity/m3/quests/quest-parity-probe -c Release -- \
//                  <产物目录> [--fixtures <夹具目录>] [--gate] [--negative-control]
// 只读：不连 MySQL、不写现网文件、不发 socket；脚本只落探针自己的 bin/<EnvirDir>/<Patch>/ 与产物目录。

using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Reflection;
using System.Text;
using M2Server.Npc;
using M2Server.Player;
using OpenMir2;
using ScriptSystem;
using Serilog;
using SystemModule;
using SystemModule.Actors;

namespace QuestParityProbe;

internal static class Program
{
    private const string Patch = "QuestProbe";

    private sealed class ProbeCase
    {
        public string Id;
        public string Name;
        public string Fixture;
        public string KnownLabel;
    }

    private static readonly ProbeCase[] Cases =
    {
        new ProbeCase { Id = "QT01-A", Name = "quest_probe_a", Fixture = "quest_probe_a.txt", KnownLabel = "@other" },
        new ProbeCase { Id = "QT01-B", Name = "quest_probe_b", Fixture = "quest_probe_b.txt", KnownLabel = "@main" },
    };

    private static int Main(string[] args)
    {
        Console.OutputEncoding = Encoding.UTF8;
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        LogService.Logger = new LoggerConfiguration().CreateLogger();

        string outDir = null;
        string fixturesDir = null;
        bool doGate = false;
        bool doActions = false;
        bool negativeControl = false;
        for (int i = 0; i < args.Length; i++)
        {
            if (args[i] == "--fixtures" && i + 1 < args.Length)
            {
                fixturesDir = Path.GetFullPath(args[++i]);
            }
            else if (args[i] == "--gate")
            {
                doGate = true;
            }
            else if (args[i] == "--actions")
            {
                doActions = true;
            }
            else if (args[i] == "--negative-control")
            {
                negativeControl = true;
            }
            else
            {
                outDir = Path.GetFullPath(args[i]);
            }
        }
        outDir ??= Directory.GetCurrentDirectory();
        fixturesDir ??= Path.GetFullPath(Path.Combine(
            Directory.GetCurrentDirectory(), "mir2-rs", "tests", "parity", "m3", "quests", "fixtures"));
        Directory.CreateDirectory(outDir);

        if (Cases.Length == 0)
        {
            Console.Error.WriteLine("ZERO_CASES");
            return 2;
        }

        if (negativeControl)
        {
            return NegativeControl();
        }

        if (doActions)
        {
            return RunActions(outDir);
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

        foreach (ProbeCase c in Cases)
        {
            string fixturePath = Path.Combine(fixturesDir, c.Fixture);
            if (!File.Exists(fixturePath))
            {
                Console.Error.WriteLine("PREREQ 夹具缺失: " + fixturePath);
                return 2;
            }
            string text = File.ReadAllText(fixturePath).Replace("\r\n", "\n");

            string path = SystemShare.GetEnvirFilePath(Patch, c.Name + ".txt");
            Directory.CreateDirectory(Path.GetDirectoryName(path));
            File.WriteAllText(path, text, new UTF8Encoding(false));
            File.WriteAllText(Path.Combine(scriptsOut, c.Name + ".txt"), text, new UTF8Encoding(false));

            int before = npc.ScriptList.Count;
            parser.LoadScriptFile(npc, Patch, c.Name, false);
            int after = npc.ScriptList.Count;
            int delta = after - before;

            if (delta != 1)
            {
                failures.Add(c.Id + ":LOAD_DELTA=" + delta.ToString(CultureInfo.InvariantCulture));
                lines.Add(string.Join("\t", c.Id, c.Fixture, delta.ToString(CultureInfo.InvariantCulture),
                    "-", c.KnownLabel, "0", "-", "-", "-", "FAIL_LOAD"));
                Console.WriteLine("FAIL {0} 装载失败：ScriptList {1}->{2}", c.Id, before, after);
                continue;
            }

            ScriptInfo si = npc.ScriptList[after - 1];
            int records = si.RecordList == null ? -1 : si.RecordList.Count;
            bool labelHit = si.RecordList != null && si.RecordList.ContainsKey(c.KnownLabel);
            bool isQuest = si.IsQuest;
            bool questInfoNull = si.QuestInfo == null;

            Console.WriteLine("LOADED {0} file={1} records={2} known_label={3} hit={4} quest_count={5}",
                c.Id, c.Fixture, records, c.KnownLabel, labelHit ? 1 : 0, si.QuestCount);

            var why = Why(si, records, labelHit);
            string verdict = why.Count == 0 ? "PASS(dead_in_oracle)" : "FAIL(" + string.Join("+", why) + ")";
            if (why.Count == 0) passed++; else failures.Add(c.Id + ":" + string.Join("+", why));

            lines.Add(string.Join("\t", c.Id, c.Fixture, delta.ToString(CultureInfo.InvariantCulture),
                records.ToString(CultureInfo.InvariantCulture), c.KnownLabel, labelHit ? "1" : "0",
                isQuest ? "1" : "0", questInfoNull ? "1" : "0",
                si.QuestCount.ToString(CultureInfo.InvariantCulture), verdict));
        }

        string ledger = Path.Combine(outDir, "quest_gate_reachability.tsv");
        File.WriteAllText(ledger, string.Join("\n", lines) + "\n", new UTF8Encoding(false));

        if (doGate)
        {
            int gateRc = RunGate(npc.ScriptList, outDir, failures);
            if (gateRc != 0)
            {
                return gateRc;
            }
        }

        Console.WriteLine("REACHABILITY cases={0} passed={1} failed={2}", Cases.Length, passed, failures.Count);
        Console.WriteLine(ledger);
        if (failures.Count > 0)
        {
            Console.Error.WriteLine("FAIL_DETAIL " + string.Join("; ", failures));
            return 1;
        }
        return 0;
    }

    private const string ActionPatch = "ActionProbe";

    private static string HexBytes(byte[] b) => Convert.ToHexString(b).ToLowerInvariant();

    private sealed class ActionOutcome
    {
        public int Parsed;
        public string Flag;
        public string Unit;
        public string Open;
        public string Exception = "-";
        public string ParsedParams = "-";
    }

    /// 跑一段 `#ACT` 正文：真解析器 + 真 `ScriptEngine.GotoLable` + 真 `PlayObject`。
    private static ActionOutcome RunOneBody(ScriptParsers parser, string id, string body, string seed)
    {
        string text = "[@main]\n#ACT\n" + body + "\n";
        string path = SystemShare.GetEnvirFilePath(ActionPatch, id + ".txt");
        Directory.CreateDirectory(Path.GetDirectoryName(path));
        File.WriteAllText(path, text, new UTF8Encoding(false));

        var npc = new NormNpc();
        var outcome = new ActionOutcome();
        var po = new PlayObject();
        if (!string.IsNullOrEmpty(seed))
        {
            foreach (string part in seed.Split(','))
            {
                po.SetQuestFlagStatus(int.Parse(part, CultureInfo.InvariantCulture), 1);
            }
        }
        try
        {
            parser.LoadScriptFile(npc, ActionPatch, id, false);
            if (npc.ScriptList.Count > 0
                && npc.ScriptList[0].RecordList.TryGetValue("@main", out SayingRecord rec))
            {
                var parts = new List<string>();
                foreach (SayingProcedure proc in rec.ProcedureList)
                {
                    outcome.Parsed += proc.ActionList.Count;
                    foreach (QuestActionInfo a in proc.ActionList)
                    {
                        parts.Add(string.Format(CultureInfo.InvariantCulture, "c{0}/n{1},{2}/s[{3}][{4}]",
                            a.nCmdCode, a.nParam1, a.nParam2, a.sParam1, a.sParam2));
                    }
                }
                outcome.ParsedParams = parts.Count == 0 ? "-" : string.Join("|", parts);
            }
            new ScriptEngine().GotoLable(npc, po, "@main");
        }
        catch (Exception ex)
        {
            outcome.Exception = ex.GetType().Name;
        }
        outcome.Flag = HexBytes(po.QuestFlag);
        outcome.Unit = HexBytes(po.QuestUnit);
        outcome.Open = HexBytes(po.QuestUnitOpen);
        return outcome;
    }

    /// 动作级对拍账本（QT-05..09）：**同一段脚本正文**两侧各跑一遍，比对最终三张位图。
    ///
    /// 参数门（oracle 的 bug，符号锚 `HUtil32.IsStringNumber`）：`!IsNullOrEmpty(s) || 正则`
    /// ⇒ **任何非空串都为真**，正则分支被短路；真门是**空参数**（缺参 ⇒ `nCMDCode = 0` ⇒ 整条动作丢弃）。
    /// 丢弃类用例另跑一遍「去掉末行」的前缀，断言三张位图**逐字节不变**（`unchanged=1`）。
    private static int RunActions(string outDir)
    {
        var cases = new (string Id, string Body, string Seed)[]
        {
            ("QT05-01", "SET [5] 1", ""),
            ("QT05-02", "SET [5] 1\nSET [5] 0", ""),
            ("QT05-03", "SET [5] 1\nSET [5] 2", ""),
            ("QT05-04", "SET [5] 1\nSET [5] -1", ""),
            ("QT05-D1", "SET [5] 1\nSET [6] 1", ""),
            ("QT05-D2", "SET [6] 1\nSET [5] 1", ""),
            ("QT05-D3", "SET [5] 1\nSET [5] 0\nSET [7] 1", ""),
            ("QT05-05", "SET [1025] 1", ""),
            ("QT05-06", "SET [0] 1", ""),
            ("QT05-07", "SET [-3] 1", ""),
            ("QT05-08", "SET [abc] 1", ""),
            ("QT05-09", "SET [5] 1\nSET [5] abc", ""),
            ("QT05-10", "SET [5]", ""),
            ("QT05-10S", "SET [5]", "5"),
            ("QT05-11", "SET [5] 1\nSET [5] \" \"", ""),
            ("QT05-12", "SET [5] 1\nSET [5]", ""),
            ("QT06-01", "SET [5] 1\nSET [6] 1\nSET [7] 1\nRESET [5] 3", ""),
            ("QT06-02", "SET [5] 1\nSET [6] 1\nRESET [5] abc", ""),
            ("QT06-03", "SET [5] 1\nRESET [5] 0", ""),
            ("QT06-04", "SET [5] 1\nSET [6] 1\nRESET [abc] 2", ""),
            ("QT06-05", "SET [5] 1\nRESET [5] -1", ""),
            ("QT06-06", "SET [5] 1\nRESET [5]", ""),
            ("QT06-06S", "RESET [5]", "5"),
            ("QT06-07", "SET [5] 1\nSET [6] 1\nRESET [5]", ""),
            ("QT07-01", "SETOPEN [5] 1", ""),
            ("QT07-02", "SETOPEN [5] 1\nSETOPEN [5] 0", ""),
            ("QT07-03", "SETOPEN [abc] 1", ""),
            ("QT08-01", "SETUNIT [5] 1", ""),
            ("QT08-02", "SETUNIT [5] 1\nSETUNIT [5] 0", ""),
            ("QT08-03", "SETUNIT [abc] 1", ""),
            ("QT09-01", "SETUNIT [5] 1\nSETUNIT [6] 1\nRESETUNIT [5] 2", ""),
            ("QT09-02", "SETUNIT [5] 1\nSETUNIT [6] 1\nRESETUNIT [5] abc", ""),
            ("QT09-03", "SETUNIT [5] 1\nRESETUNIT [5]", ""),
        };
        // 缺参 ⇒ 动作被丢弃：断言三张位图逐字节不变（不是「没看出变化」）。
        var dropCases = new HashSet<string> { "QT05-10", "QT05-12", "QT06-06", "QT06-07", "QT09-03" };

        var rows = new List<string>
        {
            "# case_id\tactions_parsed\tparsed_actions\tflag_hex\tunit_hex\tunit_open_hex\texception\tunchanged\tscript\tseed"
        };
        var parser = new ScriptParsers();
        int failed = 0;
        foreach ((string id, string body, string seed) in cases)
        {
            ActionOutcome o = RunOneBody(parser, id, body, seed);
            string unchanged = "-";
            if (!string.IsNullOrEmpty(seed))
            {
                // 预置位用例：与「只 seed、不跑正文」的初始态比，直接给出前/后是否变化
                ActionOutcome init = RunOneBody(parser, id + "-seedonly", "", seed);
                bool same = init.Flag == o.Flag && init.Unit == o.Unit && init.Open == o.Open;
                unchanged = same ? "1" : "0";
                Console.WriteLine("SEEDED {0} init={1} final={2} changed={3}", id, init.Flag[..8], o.Flag[..8], same ? "0" : "1");
            }
            else if (dropCases.Contains(id))
            {
                int cut = body.LastIndexOf('\n');
                string prefix = cut >= 0 ? body[..cut] : "";
                ActionOutcome p = RunOneBody(parser, id + "-prefix", prefix, seed);
                bool same = p.Flag == o.Flag && p.Unit == o.Unit && p.Open == o.Open;
                unchanged = same ? "1" : "0";
                if (!same)
                {
                    failed++;
                    Console.WriteLine("FAIL {0} 缺参动作改动了位图（prefix 与 full 不等）", id);
                }
            }
            rows.Add(string.Join("\t", id, o.Parsed.ToString(CultureInfo.InvariantCulture),
                o.ParsedParams, o.Flag, o.Unit, o.Open, o.Exception, unchanged, body.Replace("\n", "\\n"), string.IsNullOrEmpty(seed) ? "-" : seed));
        }

        string ledgerPath = Path.Combine(outDir, "quest_action_ledger.tsv");
        File.WriteAllText(ledgerPath, string.Join("\n", rows) + "\n", new UTF8Encoding(false));
        Console.WriteLine("ACTIONS cases={0} failed={1}", cases.Length, failed);
        Console.WriteLine(ledgerPath);
        return failed > 0 ? 1 : 0;
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

    /// 门控判决：反射调 oracle 私有静态 `ScriptEngine.CheckGotoLableQuestStatus`。
    /// 真实解析出的 ScriptInfo（IsQuest=false）必须返回 true（不门控）；
    /// 对照组把门臂装上（IsQuest=true + QuestInfo），真值表必须能返回 false（判据可红）。
    private static int RunGate(IList<ScriptInfo> parsed, string outDir, List<string> failures)
    {
        MethodInfo mi = typeof(ScriptEngine).GetMethod(
            "CheckGotoLableQuestStatus", BindingFlags.NonPublic | BindingFlags.Static);
        if (mi == null)
        {
            Console.Error.WriteLine("PREREQ 找不到 ScriptEngine.CheckGotoLableQuestStatus（oracle 结构变了？）");
            return 2;
        }

        var rows = new List<string>
        {
            "# case_id\tis_quest\tw_flag\tbt_value\tplayer_status\texpect\tgot\tverdict"
        };
        int n = 0;
        int bad = 0;

        void AddRow(string id, ScriptInfo si, int wFlag, byte btValue, int playerFlag, int playerStatus, bool expect)
        {
            GatePlayerProxy proxy = NewProxy(playerFlag, playerStatus);
            bool got = (bool)mi.Invoke(null, new object[] { proxy, si });
            bool ok = got == expect;
            if (!ok)
            {
                bad++;
            }
            rows.Add(string.Join("\t", id, si.IsQuest ? "1" : "0",
                wFlag.ToString(CultureInfo.InvariantCulture), btValue.ToString(CultureInfo.InvariantCulture),
                playerStatus.ToString(CultureInfo.InvariantCulture), expect ? "1" : "0", got ? "1" : "0",
                ok ? "PASS" : "FAIL"));
            n++;
        }

        if (parsed.Count > 0)
        {
            AddRow("QT01-G1", parsed[0], 0, 0, 5, 0, true);
        }
        AddRow("QT01-G2", Armed(5, 1), 5, 1, 5, 0, false);
        AddRow("QT01-G3", Armed(5, 0), 5, 0, 5, 0, true);
        AddRow("QT01-G4", Armed(5, 1), 5, 1, 5, 1, true);

        string path = Path.Combine(outDir, "quest_gate_decision.tsv");
        File.WriteAllText(path, string.Join("\n", rows) + "\n", new UTF8Encoding(false));
        Console.WriteLine("GATE rows={0} failed={1}", n, bad);
        Console.WriteLine(path);
        if (bad > 0)
        {
            failures.Add("GATE:" + bad.ToString(CultureInfo.InvariantCulture));
            return 1;
        }
        return 0;
    }

    private static GatePlayerProxy NewProxy(int flag, int status)
    {
        var player = DispatchProxy.Create<IPlayerActor, GatePlayerProxy>();
        var proxy = (GatePlayerProxy)player;
        proxy.Flag = flag;
        proxy.Status = status;
        return proxy;
    }

    /// 把平时不可达的门臂装上：IsQuest=true + QuestInfo[0] 指定期望值，其余组保持「放过」。
    private static ScriptInfo Armed(short wFlag, byte btValue)
    {
        var si = new ScriptInfo
        {
            IsQuest = true,
            QuestInfo = new ScriptQuestInfo[10],
            RecordList = new Dictionary<string, SayingRecord>(StringComparer.OrdinalIgnoreCase),
        };
        si.QuestInfo[0].wFlag = wFlag;
        si.QuestInfo[0].btValue = btValue;
        si.QuestInfo[0].nRandRage = 0;
        return si;
    }

    /// 反向牙：人为造一个 `IsQuest = true` 的 ScriptInfo 与一份空记录，判据必须报出来。
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
}

/// <summary>
/// `IPlayerActor` 的最小替身：只回答任务标记读取（门控判决只触及这一个取数口）。
/// </summary>
internal class GatePlayerProxy : DispatchProxy
{
    public int Flag;

    public int Status;

    protected override object Invoke(MethodInfo targetMethod, object[] args)
    {
        if (targetMethod.Name == "GetQuestFalgStatus")
        {
            // 只有被考察的 flag 返回给定状态；其余组（`QuestInfo[1..9]` 的 wFlag=0/btValue=0）
            // 必须返回 0，否则「全 10 组都要匹配」的判据会被无关组带偏。
            int flag = args.Length > 0 && args[0] is int f ? f : int.MinValue;
            return flag == Flag ? Status : 0;
        }
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
