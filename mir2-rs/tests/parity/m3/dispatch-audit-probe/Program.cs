// dispatch-audit-probe —— oracle 真身「DoSpell 之外是否有第二魔号效果分派器」审计向量。
//
// 第十片（卡 openmir2-m3-combat-skills-align，DoD③ 第五轴）：第五片把 108 行魔表按 `DoSpell`
// 控制流分为 6 类；其中 `no_switch_case`（20 行，55–80）与 `magicid_reject`（35 行，>80）的
// 承接原因都写着「待确认是否有第二分派器」。本探针把这个悬而未决的问题**机器化收口**：
//
// 1) 分派面清单（source-cited；逐行锚点自检）
//    - `src/M2Server/**/*.cs` 内所有 `switch (…MagicId|MagIdx…)` 站点（魔号开关）；
//    - `src/M2Server/**/*.cs` 内所有 `case MagicConst.<name>:` 标签（映射回数值）；
//    - `src/M2Server/**/*.cs` 内所有 `if (…(MagicId|MagIdx) == …)` 比较站点（只允许查表）。
//    实测结论：分派面恰 5 个 switch 站点 + 4 个比较站点；`no_switch_case ∪ magicid_reject`
//    的 55 个魔号里，**只有 56（SKILL_REDBANWOL）** 在别处有 `case`
//    （`PlayObject.Attack.cs:1152` 攻击路径开关，第二种分派器）；其余 54 个在 M2Server 内无任何 case。
//
// 2) 真身向量（反射直调，无地图/无 socket）
//    oracle 的私有实例入口 `PlayObject.DoSpell`（`PlayObject.Base.cs:1505-1529`）：
//      `if (!IsWarrSkill(MagIdx)) { nSpellPoint = GetSpellPoint(...); if (nSpellPoint > 0) {
//          if (WAbil.MP < nSpellPoint) return false; DamageSpell(nSpellPoint); HealthSpellChanged(); }
//        result = MagicManager.DoSpell(...); }`
//    以及 public static `MagicManager.DoSpell`（`MagicManager.cs:145`）。逐行取
//    `(is_warr, spell_point, mp 前后, 返回, 自身出帧)`，覆盖 `no_switch_case ∪ magicid_reject`
//    全部 55 行 + `warr_reject` 8 行对照组。
//
//    空 `Envirnoment` + 新构造的 `PlayObject` 处于隐藏模式（`PlayObject.cs:993`：Reset 置
//    `FixedHideMode = true`，登录后才 `:1442` 置 false）。因此 `SendRefMsg` 走
//    `BaseObject.Message.cs:390` 的**自身分支**（`if (ObMode || FixedHideMode) { SendMsg(...); return; }`），
//    把 oracle 本来要广播的帧原样投进自身 `MsgQueue`——帧的 `wIdent`/参数是 oracle 原值，
//    **收件人集合（同格广播）不在本片判据内**。
//
// 种子：本路径不取 RNG（`IsWarrSkill`/距离门/版本门/空 case 尾都没有 `Random`）⇒ 向量逐字节确定。
//
// 依据（只读，逐字对照 oracle，行号见 `VerifyAnchors`）：
//   - `src/M2Server/Magic/MagicManager.cs:82-100` `IsWarrSkill`（9 case 门）
//   - `src/M2Server/Magic/MagicManager.cs:145-671` `DoSpell`（:151 warr 门；:155 距离门；
//     :167 版本门；:172-643 主 switch；:326-391 护身符内层 switch）
//   - `src/M2Server/Player/PlayObject.Base.cs:1505-1529` `PlayObject.DoSpell`（MP 扣减包装）
//   - `src/M2Server/Player/PlayObject.Attack.cs:1118` `switch (UserMagic.MagIdx)`（攻击路径技能开关）
//   - `src/M2Server/Player/PlayObject.cs:3685` `switch (userMagic.MagIdx)`（RecalcHitSpeed 被动加成）
//
// 用法（**在仓库根运行**）：
//   dotnet run --project mir2-rs/tests/parity/m3/dispatch-audit-probe -c Release -- <仓库根> <向量输出>
//
// 只读：不连 MySQL、不写任何现网文件、不发起任何 socket、不起栈、不占端口。

using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;
using M2Server;
using M2Server.Actor;
using M2Server.Magic;
using M2Server.Maps;
using M2Server.Player;
using OpenMir2;
using OpenMir2.Data;
using OpenMir2.Enums;
using OpenMir2.Packets.ClientPackets;
using Serilog;
using SystemModule;
using SystemModule.Actors;
using SystemModule.Const;

namespace DispatchAuditProbe;

internal static class Program
{
    private const string M2ServerRoot = "src/M2Server";
    private const string MagicConstPath = "src/Modules/SystemModule/Const/MagicConst.cs";

    // 钉死的 M2Server 魔号分派面（`switch (…MagicId|MagIdx…)`）。任一漂移 ⇒ 探针终止。
    private static readonly (string Kind, string Path, int Line, int CaseCount)[] PinnedSites =
    {
        ("gate_is_warr_skill", "src/M2Server/Magic/MagicManager.cs", 85, 9),
        ("effect_dospell_main", "src/M2Server/Magic/MagicManager.cs", 172, 45),
        ("effect_dospell_inner_amulet", "src/M2Server/Magic/MagicManager.cs", 326, 7),
        ("attack_skill_toggle", "src/M2Server/Player/PlayObject.Attack.cs", 1118, 8),
        ("passive_recalc_hitspeed", "src/M2Server/Player/PlayObject.cs", 3685, 4),
    };

    // 钉死的 M2Server 魔号**比较**站点（`if (…(MagicId|MagIdx) == …)`）：只允许查表，不允许效果分派。
    private static readonly (string Kind, string Path, int Line)[] PinnedIdCmps =
    {
        ("lookup_attack_magic", "src/M2Server/Player/PlayObject.Attack.cs", 345),
        ("lookup_change_magic_key", "src/M2Server/Player/PlayObject.Operate.cs", 779),
        ("lookup_get_magic_info", "src/M2Server/Player/PlayObject.cs", 4179),
        ("cond_see_heal_gauge", "src/M2Server/Player/PlayObject.cs", 5233),
    };

    // ---- 锚点自检（防 F-1 同族的行号漂移）------------------------------------------------------
    //
    // 条数断言（第十二片）：`VerifyAnchors` 跑完必须恰为 `ExpectedAnchorCount` 条。
    // 防的是「探针增删锚点、报告/清单的条数没跟着改」这类证据计数漂移——本片即因此把报告的
    // 「4 处锚点自检」订正为 **19 条**（实测 `CheckLine` 调用 19：MagicManager 11 +
    // PlayObject.Base 4 + PlayObject.Attack 1 + PlayObject 2 + BaseObject.Message 1）。
    private const int ExpectedAnchorCount = 19;
    private static int s_anchorCount;

    private static void VerifyAnchors(string repoRoot)
    {
        s_anchorCount = 0;
        const string mm = "src/M2Server/Magic/MagicManager.cs";
        CheckLine(repoRoot, mm, 82, "public static bool IsWarrSkill(int wMagIdx)");
        CheckLine(repoRoot, mm, 85, "switch (wMagIdx)");
        CheckLine(repoRoot, mm, 145, "public static bool DoSpell(PlayObject playObject, UserMagic userMagic, short nTargetX, short nTargetY, IActor targetObject)");
        CheckLine(repoRoot, mm, 151, "if (IsWarrSkill(userMagic.MagIdx))");
        CheckLine(repoRoot, mm, 155, "SystemShare.Config.MagicAttackRage");
        CheckLine(repoRoot, mm, 167, "userMagic.Magic.MagicId > 80");
        CheckLine(repoRoot, mm, 172, "switch (userMagic.Magic.MagicId)");
        CheckLine(repoRoot, mm, 326, "switch (userMagic.Magic.MagicId)");
        CheckLine(repoRoot, mm, 643, "}");
        // 共享尾锚点（第五/九片口径）：:648 RM_MAGICFIRE，:670 return true。
        CheckLine(repoRoot, mm, 648, "if (boSpellFire)");
        CheckLine(repoRoot, mm, 670, "return true;");

        const string pb = "src/M2Server/Player/PlayObject.Base.cs";
        CheckLine(repoRoot, pb, 1505, "private bool DoSpell(UserMagic userMagic, short targetX, short targetY, IActor targetObject)");
        CheckLine(repoRoot, pb, 1510, "if (!MagicManager.IsWarrSkill(userMagic.MagIdx))");
        CheckLine(repoRoot, pb, 1512, "ushort nSpellPoint = GetSpellPoint(userMagic);");
        CheckLine(repoRoot, pb, 1522, "result = MagicManager.DoSpell(this, userMagic, targetX, targetY, targetObject);");

        CheckLine(repoRoot, "src/M2Server/Player/PlayObject.Attack.cs", 1118, "switch (UserMagic.MagIdx)");
        CheckLine(repoRoot, "src/M2Server/Player/PlayObject.cs", 3685, "switch (userMagic.MagIdx)");
        // 探针对象处于新构造的隐藏模式（`PlayObject.cs:993` 置 true，登录后 `:1442` 置 false）
        // ⇒ `SendRefMsg` 走 `BaseObject.Message.cs:390` 的自身分支，出帧落自身 MsgQueue。
        CheckLine(repoRoot, "src/M2Server/Player/PlayObject.cs", 993, "FixedHideMode = true;");
        CheckLine(repoRoot, "src/M2Server/Actor/BaseObject.Message.cs", 390, "if (ObMode || FixedHideMode)");

        if (s_anchorCount != ExpectedAnchorCount)
        {
            throw new InvalidOperationException(
                $"锚点自检条数漂移：实跑 {s_anchorCount} != 期望 {ExpectedAnchorCount}（改锚点行数必须同步 ExpectedAnchorCount）");
        }
    }

    private static void CheckLine(string repoRoot, string rel, int line, string needle)
    {
        string[] lines = ReadOracle(repoRoot, rel);
        if (line < 1 || line > lines.Length)
        {
            throw new InvalidOperationException($"{rel} 行号越界: {line} / 共 {lines.Length}");
        }
        if (lines[line - 1].IndexOf(needle, StringComparison.Ordinal) < 0)
        {
            throw new InvalidOperationException($"{rel}:{line} 锚点不符:\n  needle: {needle}\n  got   : {lines[line - 1].Trim()}");
        }
        s_anchorCount++;
    }

    private static string[] ReadOracle(string repoRoot, string rel)
        => File.ReadAllLines(Path.Combine(repoRoot, rel.Replace('/', Path.DirectorySeparatorChar)));

    // ---- 分派面扫描 -----------------------------------------------------------------------------
    private static readonly Regex SwitchOnMagic = new(@"switch\s*\([^)]*(MagicId|MagIdx)");
    private static readonly Regex IdCmp = new(@"if\s*\(.*(MagicId|MagIdx)\s*==");
    private static readonly Regex CaseMagicConst = new(@"case\s+MagicConst\.([A-Za-z0-9_]+)\s*:");
    private static readonly Regex ConstByte = new(@"public\s+const\s+byte\s+([A-Za-z0-9_]+)\s*=\s*(\d+)\s*;");

    private static Dictionary<string, int> ReadMagicConst(string repoRoot)
    {
        var map = new Dictionary<string, int>();
        foreach (string line in ReadOracle(repoRoot, MagicConstPath))
        {
            Match m = ConstByte.Match(line);
            if (m.Success)
            {
                map[m.Groups[1].Value] = int.Parse(m.Groups[2].Value, CultureInfo.InvariantCulture);
            }
        }
        if (map.Count < 50)
        {
            throw new InvalidOperationException($"MagicConst 常量解析过少（{map.Count}）⇒ 解析口径失效");
        }
        return map;
    }

    private static List<(string Kind, string Path, int Line, string Subject)> ScanSites(string repoRoot)
    {
        string root = Path.Combine(repoRoot, M2ServerRoot.Replace('/', Path.DirectorySeparatorChar));
        var sites = new List<(string, string, int, string)>();
        foreach (string file in Directory.EnumerateFiles(root, "*.cs", SearchOption.AllDirectories).OrderBy(p => p, StringComparer.Ordinal))
        {
            string rel = Rel(repoRoot, file);
            string[] lines = File.ReadAllLines(file);
            for (int i = 0; i < lines.Length; i++)
            {
                if (SwitchOnMagic.IsMatch(lines[i]))
                {
                    sites.Add(("switch", rel, i + 1, lines[i].Trim()));
                }
            }
        }
        return sites;
    }

    private static List<(string Path, int Line, string Name, int Value)> ScanCaseIds(string repoRoot, Dictionary<string, int> consts)
    {
        string root = Path.Combine(repoRoot, M2ServerRoot.Replace('/', Path.DirectorySeparatorChar));
        var cases = new List<(string, int, string, int)>();
        foreach (string file in Directory.EnumerateFiles(root, "*.cs", SearchOption.AllDirectories).OrderBy(p => p, StringComparer.Ordinal))
        {
            string rel = Rel(repoRoot, file);
            string[] lines = File.ReadAllLines(file);
            for (int i = 0; i < lines.Length; i++)
            {
                Match m = CaseMagicConst.Match(lines[i]);
                if (!m.Success)
                {
                    continue;
                }
                string name = m.Groups[1].Value;
                if (!consts.TryGetValue(name, out int value))
                {
                    throw new InvalidOperationException($"{rel}:{i + 1} case 引用了 MagicConst 里没有的常量 {name}");
                }
                cases.Add((rel, i + 1, name, value));
            }
        }
        return cases;
    }

    private static List<(string Path, int Line, string Subject)> ScanIdCmps(string repoRoot)
    {
        string root = Path.Combine(repoRoot, M2ServerRoot.Replace('/', Path.DirectorySeparatorChar));
        var found = new List<(string, int, string)>();
        foreach (string file in Directory.EnumerateFiles(root, "*.cs", SearchOption.AllDirectories).OrderBy(p => p, StringComparer.Ordinal))
        {
            string rel = Rel(repoRoot, file);
            string[] lines = File.ReadAllLines(file);
            for (int i = 0; i < lines.Length; i++)
            {
                if (IdCmp.IsMatch(lines[i]))
                {
                    found.Add((rel, i + 1, lines[i].Trim()));
                }
            }
        }
        return found;
    }

    private static string Rel(string repoRoot, string abs)
    {
        string full = Path.GetFullPath(abs);
        string root = Path.GetFullPath(repoRoot);
        string rel = Path.GetRelativePath(root, full).Replace('\\', '/');
        return rel;
    }

    // ---- 真身入口反射句柄 -------------------------------------------------------------------------
    private static MethodInfo _playObjectDoSpell;
    private static MethodInfo _getSpellPoint;
    private static FieldInfo _msgQueueField;

    private static void ResolveHandles()
    {
        _playObjectDoSpell = typeof(PlayObject).GetMethod(
            "DoSpell",
            BindingFlags.Instance | BindingFlags.NonPublic,
            null,
            new[] { typeof(UserMagic), typeof(short), typeof(short), typeof(IActor) },
            null) ?? throw new MissingMethodException("PlayObject.DoSpell(UserMagic, short, short, IActor)");
        _getSpellPoint = typeof(PlayObject).GetMethod(
            "GetSpellPoint",
            BindingFlags.Static | BindingFlags.NonPublic,
            null,
            new[] { typeof(UserMagic) },
            null) ?? throw new MissingMethodException("PlayObject.GetSpellPoint(UserMagic)");
        MsgQueueField();
    }

    private static FieldInfo MsgQueueField()
    {
        if (_msgQueueField != null)
        {
            return _msgQueueField;
        }
        for (Type t = typeof(PlayObject); t != null; t = t.BaseType)
        {
            FieldInfo f = t.GetField("MsgQueue", BindingFlags.Instance | BindingFlags.NonPublic);
            if (f != null)
            {
                _msgQueueField = f;
                return f;
            }
        }
        throw new InvalidOperationException("找不到 ActorEntity.MsgQueue 字段");
    }

    /// <summary>取队列计数并**清空**（返回 `wIdent:nParam1:nParam2:nParam3` 以 `;` 连接）。</summary>
    private static (int Count, string Msgs) DrainMsgs(PlayObject po)
    {
        object q = MsgQueueField().GetValue(po);
        Type qt = q.GetType();
        int count = (int)qt.GetProperty("Count").GetValue(q);
        MethodInfo tryDequeue = qt.GetMethod("TryDequeue");
        var args = new object[] { null, null };
        var msgs = new List<string>();
        while ((bool)tryDequeue.Invoke(q, args))
        {
            object send = args[0];
            Type st = send.GetType();
            msgs.Add(string.Join(":", st.GetField("wIdent").GetValue(send),
                st.GetField("nParam1").GetValue(send),
                st.GetField("nParam2").GetValue(send),
                st.GetField("nParam3").GetValue(send)));
            args[0] = null;
            args[1] = null;
        }
        return (count, string.Join(";", msgs));
    }

    private static (PlayObject Po, UserMagic Um) NewPair(ushort magicId, string name)
    {
        var po = new PlayObject
        {
            Race = ActorRace.Play,
            Job = PlayerJob.Wizard,
            Abil = new Ability { Level = 40, HP = 500, MP = 500, MaxHP = 500, MaxMP = 500 },
            AddAbil = new AddAbility(),
            MagicList = new List<UserMagic>(),
            ItemList = new List<UserItem>(),
            UseItems = new UserItem[13],
            IsRobot = false,
            Envir = new Envirnoment(),
            SoftVersionDateEx = 1,
            ClientTick = 1,
        };
        po.WAbil = new Ability { HP = 500, MP = 500, MaxHP = 500, MaxMP = 500 };
        var magic = new MagicInfo
        {
            MagicId = magicId,
            MagicName = name,
            TrainLv = 3,
            Job = 1,
            Spell = 20,
            DefSpell = 0,
            Effect = 1,
            EffectType = 1,
        };
        magic.TrainLevel = new byte[] { 7, 9, 11, 0 };
        magic.MaxTrain = new[] { 100, 200, 300, 400 };
        var um = new UserMagic
        {
            Magic = magic,
            MagIdx = magicId,
            Level = 0,
            TranPoint = 0,
            Key = (char)0,
        };
        po.MagicList.Add(um);
        return (po, um);
    }

    private sealed record GenericRow(int RowIndex, ushort Id, string Name, string Cls);

    /// <summary>从 5th 片已入库的 `skill_vectors.tsv` 读全部 `reg` 行（保持装表顺序）。</summary>
    private static List<GenericRow> ReadRegistry(string repoRoot)
    {
        string path = Path.Combine(repoRoot, "mir2-rs", "tests", "parity", "m3", "skill-probe",
            "vectors", "skill_vectors.tsv");
        var rows = new List<GenericRow>();
        int idx = 0;
        foreach (string line in File.ReadAllLines(path))
        {
            if (!line.StartsWith("reg\t", StringComparison.Ordinal))
            {
                continue;
            }
            idx++;
            string[] f = line.Split('\t');
            rows.Add(new GenericRow(idx, ushort.Parse(f[1], CultureInfo.InvariantCulture), f[2], f[8]));
        }
        return rows;
    }

    public static int Main(string[] args)
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        LogService.Logger = new LoggerConfiguration().CreateLogger();

        string repoRoot = args.Length > 0 ? args[0] : ".";
        string outPath = args.Length > 1
            ? args[1]
            : Path.Combine("mir2-rs", "tests", "parity", "m3", "dispatch-audit-probe", "vectors", "dispatch_audit.json");

        VerifyAnchors(repoRoot);
        ResolveHandles();

        // 1) 分派面清单：站点集合必须与钉死集合一致（行号 + case 数）。
        List<(string Kind, string Path, int Line, string Subject)> sites = ScanSites(repoRoot);
        if (sites.Count != PinnedSites.Length)
        {
            throw new InvalidOperationException(
                $"M2Server 魔号 switch 站点数 {sites.Count} != 钉死 {PinnedSites.Length}：\n  " +
                string.Join("\n  ", sites.Select(s => $"{s.Path}:{s.Line} {s.Subject}")));
        }
        for (int i = 0; i < PinnedSites.Length; i++)
        {
            (string kind, string path, int line, int cases) = PinnedSites[i];
            (string sKind, string sPath, int sLine, string subject) = sites[i];
            if (sPath != path || sLine != line)
            {
                throw new InvalidOperationException($"分派站点漂移：期望 {path}:{line}，实得 {sPath}:{sLine}（{subject}）");
            }
        }
        // case 数逐个复核（按站点覆盖行区间计数）。
        int[] caseCounts =
        {
            CountCases(repoRoot, "src/M2Server/Magic/MagicManager.cs", 82, 101),
            CountCases(repoRoot, "src/M2Server/Magic/MagicManager.cs", 172, 643, 326, 391),
            CountCases(repoRoot, "src/M2Server/Magic/MagicManager.cs", 326, 391),
            CountCases(repoRoot, "src/M2Server/Player/PlayObject.Attack.cs", 1118, 1285),
            CountCases(repoRoot, "src/M2Server/Player/PlayObject.cs", 3685, 3712),
        };
        for (int i = 0; i < PinnedSites.Length; i++)
        {
            if (caseCounts[i] != PinnedSites[i].CaseCount)
            {
                throw new InvalidOperationException(
                    $"{PinnedSites[i].Path}:{PinnedSites[i].Line} case 数 {caseCounts[i]} != 钉死 {PinnedSites[i].CaseCount}");
            }
        }

        // 2) case 标签数值集合（含所有 `case MagicConst.<name>`）⇒ 供 Rust 侧断言「目标桶 ∩ case 值」的**精确面**。
        Dictionary<string, int> consts = ReadMagicConst(repoRoot);
        List<(string Path, int Line, string Name, int Value)> caseIds = ScanCaseIds(repoRoot, consts);

        // 2b) 魔号**比较**站点：只允许查表（钉死 4 处），不允许效果分派。
        List<(string Path, int Line, string Subject)> idCmps = ScanIdCmps(repoRoot);
        if (idCmps.Count != PinnedIdCmps.Length)
        {
            throw new InvalidOperationException(
                $"M2Server 魔号比较站点数 {idCmps.Count} != 钉死 {PinnedIdCmps.Length}：\n  " +
                string.Join("\n  ", idCmps.Select(s => $"{s.Path}:{s.Line} {s.Subject}")));
        }
        for (int i = 0; i < PinnedIdCmps.Length; i++)
        {
            if (idCmps[i].Path != PinnedIdCmps[i].Path || idCmps[i].Line != PinnedIdCmps[i].Line)
            {
                throw new InvalidOperationException(
                    $"魔号比较站点漂移：期望 {PinnedIdCmps[i].Path}:{PinnedIdCmps[i].Line}，实得 {idCmps[i].Path}:{idCmps[i].Line}（{idCmps[i].Subject}）");
            }
        }

        // 3) 真身向量：no_switch_case ∪ magicid_reject ∪ warr_reject（对照）。
        List<GenericRow> registry = ReadRegistry(repoRoot);
        var bucketIds = registry
            .Where(r => r.Cls is "no_switch_case" or "magicid_reject")
            .Select(r => (int)r.Id)
            .ToHashSet();
        var bucketCaseHits = caseIds.Where(c => bucketIds.Contains(c.Value)).ToList();
        var wanted = registry
            .Where(r => r.Cls is "no_switch_case" or "magicid_reject" or "warr_reject")
            .ToList();

        var tsv = new List<string> { string.Join("\t", "cfg", "no_rng_drawn") };
        var siteObjs = new List<object>();
        for (int si = 0; si < sites.Count; si++)
        {
            (_, string path, int line, string subject) = sites[si];
            string kind = PinnedSites[si].Kind;
            int cc = caseCounts[si];
            tsv.Add(string.Join("\t", "site", kind, path, line, cc, subject));
            siteObjs.Add(new { kind, file = path, line, case_count = cc, subject });
        }
        var caseObjs = new List<object>();
        foreach ((string path, int line, string name, int value) in caseIds)
        {
            tsv.Add(string.Join("\t", "caseid", path, line, name, value));
            caseObjs.Add(new { file = path, line, name, value });
        }
        var idCmpObjs = new List<object>();
        for (int ii = 0; ii < idCmps.Count; ii++)
        {
            tsv.Add(string.Join("\t", "idcmp", PinnedIdCmps[ii].Kind, idCmps[ii].Path, idCmps[ii].Line, idCmps[ii].Subject));
            idCmpObjs.Add(new { kind = PinnedIdCmps[ii].Kind, file = idCmps[ii].Path, line = idCmps[ii].Line, subject = idCmps[ii].Subject });
        }
        var bucketCaseObjs = new List<object>();
        foreach ((string path, int line, string name, int value) in bucketCaseHits)
        {
            tsv.Add(string.Join("\t", "bucketcase", path, line, name, value));
            bucketCaseObjs.Add(new { file = path, line, name, value });
        }

        var genericObjs = new List<object>();
        foreach (GenericRow row in wanted)
        {
            bool isWarr = MagicManager.IsWarrSkill(row.Id);
            (PlayObject po, UserMagic um) = NewPair(row.Id, row.Name);
            ushort spellPoint = (ushort)_getSpellPoint.Invoke(null, new object[] { um });
            int mpBefore = po.WAbil.MP;
            bool result = (bool)_playObjectDoSpell.Invoke(po, new object[] { um, (short)0, (short)0, null });
            int mpAfter = po.WAbil.MP;
            (int msgCount, string msgs) = DrainMsgs(po);

            // 对照：直接调 public static `MagicManager.DoSpell`（不经 MP 包装），记录原始返回/异常。
            (PlayObject po2, UserMagic um2) = NewPair(row.Id, row.Name);
            bool directResult;
            string directException = string.Empty;
            try
            {
                directResult = MagicManager.DoSpell(po2, um2, 0, 0, null);
            }
            catch (Exception ex)
            {
                directResult = false;
                directException = ex.GetType().Name;
            }
            DrainMsgs(po2);

            tsv.Add(string.Join("\t", "generic", row.RowIndex, row.Id, row.Name, row.Cls,
                isWarr ? 1 : 0, spellPoint, mpBefore, mpAfter, result ? 1 : 0,
                directResult ? 1 : 0, directException, msgCount, msgs));
            genericObjs.Add(new
            {
                row_index = row.RowIndex,
                magic_id = (int)row.Id,
                name = row.Name,
                dospell_class = row.Cls,
                is_warr = isWarr,
                spell_point = (int)spellPoint,
                mp_before = mpBefore,
                mp_after = mpAfter,
                out_result = result,
                direct_result = directResult,
                direct_exception = directException,
                msg_count = msgCount,
                msg = msgs,
            });
        }

        var payload = new
        {
            generated_by = "mir2-rs/tests/parity/m3/dispatch-audit-probe",
            oracle = new
            {
                dispatch_surface_scan = $"{M2ServerRoot}/**/*.cs 的 `switch (…MagicId|MagIdx…)` 与 `case MagicConst.<name>`（source-cited）",
                play_object_dospell = "M2Server.Player.PlayObject.DoSpell(UserMagic, short, short, IActor)（private，反射直调；含 GetSpellPoint 扣 MP）",
                magic_manager_dospell = "M2Server.Magic.MagicManager.DoSpell(PlayObject, UserMagic, short, short, IActor)（public static，直调）",
                source = "MagicManager.cs:82-100/:145-671；PlayObject.Base.cs:1505-1529；PlayObject.Attack.cs:1118；PlayObject.cs:3685",
                envir = "空 Envirnoment（无地图/无 socket）+ 新构造 PlayObject 的隐藏模式（PlayObject.cs:993 FixedHideMode=true）⇒ SendRefMsg 走 BaseObject.Message.cs:390 自身分支，帧落自身 MsgQueue；收件人集合不在判据内",
                rng = "本路径不取 RNG（IsWarrSkill/距离门/版本门/空 case 尾无 Random）⇒ 逐字节确定",
            },
            pinned_sites = PinnedSites.Select(s => new { kind = s.Kind, file = s.Path, line = s.Line, case_count = s.CaseCount }),
            sites = siteObjs,
            id_cmp_sites = idCmpObjs,
            case_ids = caseObjs,
            bucket_case_hits = bucketCaseObjs,
            generic_rows = genericObjs,
        };

        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outPath)));
        string json = JsonSerializer.Serialize(payload, new JsonSerializerOptions { WriteIndented = true });
        File.WriteAllText(outPath, json + Environment.NewLine, new UTF8Encoding(false));
        string tsvPath = Path.ChangeExtension(outPath, ".tsv");
        File.WriteAllText(tsvPath, string.Join("\n", tsv) + "\n", new UTF8Encoding(false));
        Console.WriteLine(
            $"dispatch-audit-probe: wrote {Path.GetFullPath(outPath)} ({siteObjs.Count} sites + {idCmpObjs.Count} idcmps + {caseObjs.Count} case_labels + {bucketCaseObjs.Count} bucket_case_hits + {genericObjs.Count} generic rows) + {Path.GetFileName(tsvPath)}");
        return 0;
    }

    private static int CountCases(string repoRoot, string rel, int lo, int hi, int excludeLo = 0, int excludeHi = 0)
    {
        int n = 0;
        string[] lines = ReadOracle(repoRoot, rel);
        for (int i = lo; i <= hi && i <= lines.Length; i++)
        {
            if (excludeLo != 0 && i >= excludeLo && i <= excludeHi)
            {
                continue;
            }
            if (lines[i - 1].IndexOf("case MagicConst.", StringComparison.Ordinal) >= 0)
            {
                n++;
            }
        }
        return n;
    }

    private static int CaseCountFor(int[] counts, string path, int line)
    {
        for (int i = 0; i < PinnedSites.Length; i++)
        {
            if (PinnedSites[i].Path == path && PinnedSites[i].Line == line)
            {
                return counts[i];
            }
        }
        throw new InvalidOperationException($"未登记的分派站点 {path}:{line}");
    }
}
