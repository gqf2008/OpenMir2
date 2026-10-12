// residual-case-probe —— oracle 真身「DoSpell switch 内联 case」的端到端向量。
//
// 第十一片（卡 openmir2-m3-combat-skills-align，DoD③ 第六轴）：
//   * `no_effect_case` 6 行（43/49/51/52/53/54）：case 体无副作用 / 只置 boTrain，
//     第九片只把共享尾 source-cited；本片做 `DoSpell` 端到端真身向量。
//   * `context_required` 的内联 case 40（`SKILL_UNAMYOUNSUL`）：清三种状态计时器，
//     第七片只登记为 `<inline>`；本片做真身向量。
//
// 依据（只读，逐字对照 oracle，行号见 `VerifyAnchors`）：
//   - `src/M2Server/Magic/MagicManager.cs:145` `DoSpell`（分派与前置；:151 warr 门、:155 距离门、:167 版本门）
//   - `:495-523` case SKILL_UNAMYOUNSUL（内联效果：:504 Random(7) 门；:506/:511/:516 三种状态计时器置 1）
//   - `:555-556` case SKILL_43（`break`）；:607-642 49/51/52/53/54（只置 boTrain）
//   - `:644-670` 共享尾（:648 RM_MAGICFIRE；:659 训练门；:663 TrainSkill；:670 return true）
//   - `src/M2Server/Player/PlayObject.Base.cs:1505-1529` `PlayObject.DoSpell`（MP 扣减包装）
//
// 用法（**在仓库根运行**）：
//   dotnet run --project mir2-rs/tests/parity/m3/residual-case-probe -c Release -- <仓库根> <向量输出>
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
using M2Server;
using M2Server.Actor;
using M2Server.Magic;
using M2Server.Maps;
using M2Server.Player;
using OpenMir2;
using OpenMir2.Consts;
using OpenMir2.Data;
using OpenMir2.Enums;
using OpenMir2.Packets.ClientPackets;
using Serilog;
using SystemModule;
using SystemModule.Actors;

namespace ResidualCaseProbe;

internal static class Program
{
    private const int RngSeed = 20261012;

    /// <summary>`System.Random` 的记录包装：按 `ReplayRandom` 的行式格式（seq|kind|arg|result|site）记账。</summary>
    private sealed class RecordingRandom : Random
    {
        public readonly List<string> Draws = new();

        public RecordingRandom(int seed) : base(seed)
        {
        }

        private void Log(string kind, long arg, long value, string site)
        {
            Draws.Add($"{Draws.Count + 1}|{kind}|{arg}|{value}|{site}");
        }

        public override int Next()
        {
            int v = base.Next();
            Log("Next()", 0, v, "next");
            return v;
        }

        public override int Next(int maxValue)
        {
            int v = base.Next(maxValue);
            Log("Next(max)", maxValue, v, "max");
            return v;
        }

        public override int Next(int minValue, int maxValue)
        {
            int v = base.Next(minValue, maxValue);
            Log("Next(min,max)", ((long)minValue << 32) | (uint)maxValue, v, "range");
            return v;
        }
    }

    private static FieldInfo _rngField;

    private static void SeedRng(int seed)
    {
        RandomNumber.GetInstance();
        _rngField ??= typeof(RandomNumber).GetField("random", BindingFlags.NonPublic | BindingFlags.Static)
            ?? throw new InvalidOperationException("找不到 OpenMir2.RandomNumber.random 私有静态字段");
        _rngField.SetValue(null, new RecordingRandom(seed));
    }

    private static string Draws()
        => string.Join(";", ((RecordingRandom)_rngField.GetValue(null)).Draws);

    /// <summary>取第一条 `Next(max)` 记录的结果值；无取数返回 -1。</summary>
    private static int FirstMaxValue(string draws)
    {
        if (string.IsNullOrEmpty(draws))
        {
            return -1;
        }
        string[] parts = draws.Split(';')[0].Split('|');
        return int.Parse(parts[3], CultureInfo.InvariantCulture);
    }

    private static PlayObject NewCaster(byte level)
    {
        var po = new PlayObject
        {
            Race = ActorRace.Play,
            Job = PlayerJob.Wizard,
            Abil = new Ability { Level = level, HP = 500, MP = 500, MaxHP = 500, MaxMP = 500 },
            AddAbil = new AddAbility(),
            MagicList = new List<UserMagic>(),
            ItemList = new List<UserItem>(),
            UseItems = new UserItem[13],
            SlaveList = new List<IMonsterActor>(),
            IsRobot = false,
            Envir = new Envirnoment(),
            ActorId = 910040,
            SoftVersionDateEx = 1,
            ClientTick = 1,
            StatusTimeArr = new ushort[15],
        };
        po.WAbil = new Ability { HP = 500, MP = 500, MaxHP = 500, MaxMP = 500 };
        return po;
    }

    private static UserMagic NewMagic(ushort magicId, string name, byte level, ushort spell, byte trainLv0, byte trainLv)
    {
        var magic = new MagicInfo
        {
            MagicId = magicId,
            MagicName = name,
            TrainLv = trainLv,
            Job = 1,
            Spell = spell,
            DefSpell = 0,
            Effect = 1,
            EffectType = 1,
        };
        magic.TrainLevel = new byte[] { trainLv0, 9, 11, 0 };
        magic.MaxTrain = new[] { 100, 200, 300, 400 };
        return new UserMagic
        {
            Magic = magic,
            MagIdx = magicId,
            Level = level,
            TranPoint = 0,
            Key = (char)0,
        };
    }

    // ---- 锚点自检（防 F-1 同族的行号漂移）------------------------------------------------------
    //
    // 条数断言（第十二片）：`VerifyAnchors` 跑完必须恰为 `ExpectedAnchorCount` 条。
    // 防的是「探针增删锚点、报告/清单的条数没跟着改」这类证据计数漂移——本片即因此把
    // 报告的「26 个锚点」订正为 **28**（reviewer-5 `0b222072` 的 Minor）。改锚点行数必须同步改这里。
    private const int ExpectedAnchorCount = 28;
    private static int s_anchorCount;

    private static void VerifyAnchors(string repoRoot)
    {
        s_anchorCount = 0;
        const string mm = "src/M2Server/Magic/MagicManager.cs";
        CheckLine(repoRoot, mm, 82, "public static bool IsWarrSkill(int wMagIdx)");
        CheckLine(repoRoot, mm, 145, "public static bool DoSpell(PlayObject playObject, UserMagic userMagic, short nTargetX, short nTargetY, IActor targetObject)");
        CheckLine(repoRoot, mm, 151, "if (IsWarrSkill(userMagic.MagIdx))");
        CheckLine(repoRoot, mm, 167, "userMagic.Magic.MagicId > 80");
        CheckLine(repoRoot, mm, 172, "switch (userMagic.Magic.MagicId)");
        // 40：内联效果体。
        CheckLine(repoRoot, mm, 495, "case MagicConst.SKILL_UNAMYOUNSUL:");
        CheckLine(repoRoot, mm, 502, "playObject.IsProperFriend(targetObject)");
        CheckLine(repoRoot, mm, 504, "M2Share.RandomNumber.Random(7) - (userMagic.Level + 1) < 0");
        CheckLine(repoRoot, mm, 506, "StatusTimeArr[PoisonState.DECHEALTH] != 0");
        CheckLine(repoRoot, mm, 511, "StatusTimeArr[PoisonState.DAMAGEARMOR] != 0");
        CheckLine(repoRoot, mm, 516, "StatusTimeArr[PoisonState.STONE] != 0");
        CheckLine(repoRoot, mm, 523, "break;");
        // no_effect_case：43 与 49/51/52/53/54。
        CheckLine(repoRoot, mm, 555, "case MagicConst.SKILL_43:");
        CheckLine(repoRoot, mm, 556, "break;");
        CheckLine(repoRoot, mm, 607, "case MagicConst.SKILL_49:");
        CheckLine(repoRoot, mm, 631, "case MagicConst.SKILL_51:");
        CheckLine(repoRoot, mm, 634, "case MagicConst.SKILL_52:");
        CheckLine(repoRoot, mm, 637, "case MagicConst.SKILL_53:");
        CheckLine(repoRoot, mm, 640, "case MagicConst.SKILL_54:");
        // 共享尾。
        CheckLine(repoRoot, mm, 648, "if (boSpellFire)");
        CheckLine(repoRoot, mm, 659, "if (userMagic.Level < 3 && boTrain)");
        CheckLine(repoRoot, mm, 661, "userMagic.Magic.TrainLevel[userMagic.Level] <= playObject.Abil.Level");
        CheckLine(repoRoot, mm, 663, "playObject.TrainSkill(userMagic, M2Share.RandomNumber.Random(3) + 1);");
        CheckLine(repoRoot, mm, 670, "return true;");

        const string pb = "src/M2Server/Player/PlayObject.Base.cs";
        CheckLine(repoRoot, pb, 1505, "private bool DoSpell(UserMagic userMagic, short targetX, short targetY, IActor targetObject)");
        CheckLine(repoRoot, pb, 1512, "ushort nSpellPoint = GetSpellPoint(userMagic);");
        CheckLine(repoRoot, pb, 1522, "result = MagicManager.DoSpell(this, userMagic, targetX, targetY, targetObject);");
        CheckLine(repoRoot, pb, 1369, "protected static ushort GetSpellPoint(UserMagic userMagic)");

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
            BindingFlags.Static | BindingFlags.NonPublic | BindingFlags.Public,
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

    private sealed record RunResult(
        bool Result,
        int SpellPoint,
        int MpBefore,
        int MpAfter,
        string Draws,
        ushort D0Before, ushort A0Before, ushort S0Before,
        ushort D0After, ushort A0After, ushort S0After,
        int TranBefore, int TranAfter,
        byte LevelBefore, byte LevelAfter,
        int MsgCount, string Msgs, string Exception);

    private static RunResult Run(ushort magicId, string name, byte umLevel, byte abilLevel, byte trainLv0, int seed,
        ushort d0, ushort a0, ushort s0)
    {
        PlayObject po = NewCaster(abilLevel);
        UserMagic um = NewMagic(magicId, name, umLevel, 20, trainLv0, 3);
        po.StatusTimeArr[PoisonState.DECHEALTH] = d0;
        po.StatusTimeArr[PoisonState.DAMAGEARMOR] = a0;
        po.StatusTimeArr[PoisonState.STONE] = s0;
        int spellPoint = (int)(ushort)_getSpellPoint.Invoke(null, new object[] { um });
        int mpBefore = po.WAbil.MP;
        int tranBefore = um.TranPoint;
        byte levelBefore = um.Level;
        string exception = string.Empty;
        bool result;
        SeedRng(seed);
        try
        {
            result = (bool)_playObjectDoSpell.Invoke(po, new object[] { um, (short)0, (short)0, null });
        }
        catch (Exception ex)
        {
            result = false;
            exception = ex.GetType().Name;
        }
        string draws = Draws();
        int mpAfter = po.WAbil.MP;
        int tranAfter = um.TranPoint;
        byte levelAfter = um.Level;
        (int msgCount, string msgs) = DrainMsgs(po);
        return new RunResult(result, spellPoint, mpBefore, mpAfter, draws,
            d0, a0, s0,
            po.StatusTimeArr[PoisonState.DECHEALTH], po.StatusTimeArr[PoisonState.DAMAGEARMOR], po.StatusTimeArr[PoisonState.STONE],
            tranBefore, tranAfter, levelBefore, levelAfter, msgCount, msgs, exception);
    }

    private static void AddRow(List<object> rows, List<string> tsv, string kind, ushort magicId, string name,
        byte umLevel, byte abilLevel, byte trainLv0, int seed, RunResult r)
    {
        tsv.Add(string.Join("\t", "resid", kind, magicId, name, umLevel, abilLevel, trainLv0, seed,
            $"{r.D0Before},{r.A0Before},{r.S0Before}",
            r.MpBefore, r.SpellPoint, r.MpAfter, r.Result ? 1 : 0, r.Draws,
            $"{r.D0After},{r.A0After},{r.S0After}",
            r.TranBefore, r.TranAfter, r.LevelBefore, r.LevelAfter, r.MsgCount, r.Msgs, r.Exception));
        rows.Add(new
        {
            kind,
            magic_id = (int)magicId,
            name,
            um_level = (int)umLevel,
            abil_level = (int)abilLevel,
            train_lv0 = (int)trainLv0,
            seed,
            status_before = new { dechealth = (int)r.D0Before, damagearmor = (int)r.A0Before, stone = (int)r.S0Before },
            mp_before = r.MpBefore,
            spell_point = r.SpellPoint,
            mp_after = r.MpAfter,
            result = r.Result,
            draws = r.Draws,
            first_draw = FirstMaxValue(r.Draws),
            status_after = new { dechealth = (int)r.D0After, damagearmor = (int)r.A0After, stone = (int)r.S0After },
            tran_point_before = r.TranBefore,
            tran_point_after = r.TranAfter,
            magic_level_before = (int)r.LevelBefore,
            magic_level_after = (int)r.LevelAfter,
            msg_count = r.MsgCount,
            msg = r.Msgs,
            exception = r.Exception,
        });
    }

    public static int Main(string[] args)
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        LogService.Logger = new LoggerConfiguration().CreateLogger();

        string repoRoot = args.Length > 0 ? args[0] : ".";
        string outPath = args.Length > 1
            ? args[1]
            : Path.Combine("mir2-rs", "tests", "parity", "m3", "residual-case-probe", "vectors", "residual_case.json");

        VerifyAnchors(repoRoot);
        ResolveHandles();

        var rows = new List<object>();
        var tsv = new List<string> { string.Join("\t", "cfg", RngSeed) };

        // ---- 40 SKILL_UNAMYOUNSUL：内联效果，两态（清成 / 未清成）-------------------------------
        // 两态由 `Random(7) - (Level+1) < 0` 决定（本片 um_level=0 ⇒ 需 Random(7)==0 才清）。
        // 用固定搜索找出首两个种子，保证向量稳定且覆盖两态。
        RunResult clear = null;
        RunResult noClear = null;
        int clearSeed = 0;
        int noClearSeed = 0;
        for (int k = 0; k < 2000 && (clear == null || noClear == null); k++)
        {
            int seed = RngSeed ^ (0x4000 + k);
            RunResult r = Run(40, "十方斩", 0, 40, 7, seed, 50, 60, 70);
            if (r.Draws.Length == 0)
            {
                continue;
            }
            if (FirstMaxValue(r.Draws) == 0)
            {
                if (clear == null)
                {
                    clear = r;
                    clearSeed = seed;
                }
            }
            else if (noClear == null)
            {
                noClear = r;
                noClearSeed = seed;
            }
        }
        if (clear == null || noClear == null)
        {
            throw new InvalidOperationException($"40 的 Random(7) 门两态未同时命中（clear={clear != null} noClear={noClear != null}）");
        }
        AddRow(rows, tsv, "inline_effect_cleared", 40, "十方斩", 0, 40, 7, clearSeed, clear);
        AddRow(rows, tsv, "inline_effect_not_cleared", 40, "十方斩", 0, 40, 7, noClearSeed, noClear);

        // ---- no_effect_case 43：case break ⇒ 永不训练 ------------------------------------------
        AddRow(rows, tsv, "no_effect_break", 43, "开天斩", 0, 40, 7, RngSeed ^ 0x1001, Run(43, "开天斩", 0, 40, 7, RngSeed ^ 0x1001, 0, 0, 0));
        AddRow(rows, tsv, "no_effect_break", 43, "开天斩", 0, 6, 7, RngSeed ^ 0x1002, Run(43, "开天斩", 0, 6, 7, RngSeed ^ 0x1002, 0, 0, 0));

        // ---- no_effect_case 49/51/52/53/54：只置 boTrain ⇒ 唯一效果是共享训练尾 ----------------
        var trainRows = new (ushort Id, string Name, int SeedBase)[]
        {
            (49, "云寂术", 0x2001),
            (51, "月魂风暴", 0x2101),
            (52, "神之诅咒", 0x2201),
            (53, "嗜血魔咒(保留)", 0x2301),
            (54, "召妖降魔", 0x2401),
        };
        foreach ((ushort id, string name, int seedBase) in trainRows)
        {
            // 训练门通过：Level=0 < 3 且 TrainLevel[0]=7 <= Abil.Level=40 ⇒ 抽 Random(3)+1 并加点。
            AddRow(rows, tsv, "no_effect_train", id, name, 0, 40, 7, RngSeed ^ seedBase,
                Run(id, name, 0, 40, 7, RngSeed ^ seedBase, 0, 0, 0));
            // 训练门失败：Abil.Level=6 < TrainLevel[0]=7 ⇒ 不抽、不加点。
            AddRow(rows, tsv, "no_effect_train", id, name, 0, 6, 7, RngSeed ^ (seedBase + 0x10),
                Run(id, name, 0, 6, 7, RngSeed ^ (seedBase + 0x10), 0, 0, 0));
        }
        // 额外一行覆盖 `Level < 3` 门：Lv=0 才训练，Lv=3 不训练（即便 Abil.Level 够）。
        AddRow(rows, tsv, "no_effect_train_level3", 49, "云寂术", 3, 40, 7, RngSeed ^ 0x2F01,
            Run(49, "云寂术", 3, 40, 7, RngSeed ^ 0x2F01, 0, 0, 0));

        var payload = new
        {
            generated_by = "mir2-rs/tests/parity/m3/residual-case-probe",
            oracle = new
            {
                play_object_dospell = "M2Server.Player.PlayObject.DoSpell(UserMagic, short, short, IActor)（private，反射直调；含 GetSpellPoint 扣 MP）",
                source = "MagicManager.cs:145-671（:495-523 40 内联效果；:555-556 43 break；:607-642 49/51/52/53/54 boTrain；:644-670 共享尾）；PlayObject.Base.cs:1505-1529",
                envir = "空 Envirnoment（无地图/无 socket）+ 新构造 PlayObject 隐藏模式（PlayObject.cs:993）⇒ SendRefMsg 走 BaseObject.Message.cs:390 自身分支，帧落自身 MsgQueue；收件人集合不在本片判据内",
                rng = "OpenMir2.RandomNumber.random 反射覆盖为 RecordingRandom(seed)：记录每次 Next/Next(max)/Next(min,max) 的实参/返回值",
                note = "本片只覆盖 40 与该 6 行 no_effect_case；context_required 其余 8 行（8/17/21/30/37/41/46 需地图/可见集/召唤；另 20/32 已由第七片覆盖）不在本片",
            },
            seed = RngSeed,
            rows,
        };

        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outPath)));
        File.WriteAllText(outPath, JsonSerializer.Serialize(payload, new JsonSerializerOptions { WriteIndented = true }) + Environment.NewLine, new UTF8Encoding(false));
        string tsvPath = Path.ChangeExtension(outPath, ".tsv");
        File.WriteAllText(tsvPath, string.Join("\n", tsv) + "\n", new UTF8Encoding(false));
        Console.WriteLine($"residual-case-probe: wrote {Path.GetFullPath(outPath)} ({rows.Count} rows) + {Path.GetFileName(tsvPath)}");
        return 0;
    }
}
