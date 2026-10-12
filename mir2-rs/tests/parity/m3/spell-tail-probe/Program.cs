// spell-tail-probe —— oracle 真身实测「DoSpell 共享尾 + 技能熟练度/升级」的向量。
//
// 第九片（卡 openmir2-m3-combat-skills-align，DoD③ 第四轴）：第五片把 108 行魔表按 DoSpell
// 控制流分成 6 类，其中两类**效果体为空**：
//   * `no_effect_case`（6 行）：case 体是 `break`（43 `SKILL_43`）或只置 `boTrain = true`
//     （49 `SKILL_49` / 51 / 52 / 53 / 54）；
//   * `no_switch_case`（20 行）：`DoSpell` 的 switch 里根本没有这些魔号的 case。
// 它们**唯一的运行时可观测行为**都在共享尾（`src/M2Server/Magic/MagicManager.cs:644-670`）：
//   :644-647 `if (boSpellFail) return false;`
//   :648-658 `if (boSpellFire) SendRefMsg(RM_MAGICFIRE, …)`
//   :659-669 `if (userMagic.Level < 3 && boTrain) { if (TrainLevel[Level] <= Abil.Level) {
//              TrainSkill(userMagic, Random(3)+1); if (!CheckMagicLevelUp(userMagic))
//              SendSelfDelayMsg(RM_MAGIC_LVEXP, …); } }`
//   :670     `return true;`
//
// 本探针把尾逻辑调用的三个 oracle 入口在**无地图/无 socket** 的独立对象上直调：
//   * `PlayObject.TrainSkill`（`PlayObject.cs:4393-4400`，public）——`FastTrain` 时 ×3；
//   * `PlayObject.CheckMagicLevelUp`（`PlayObject.cs:5198-5226`，public）——升级点扣减/升级/帧/看护法量；
//   * `PlayObject.CheckSkillProficiency`（`PlayObject.Attack.cs:200-213`，private）——攻击路径同段逻辑。
//
// 依据（只读，逐字对照 oracle，行号见 `VerifyAnchors`）：
//   - `src/M2Server/Player/PlayObject.cs:4393-4400` `TrainSkill`
//   - `src/M2Server/Player/PlayObject.cs:5198-5226` `CheckMagicLevelUp`（`:5219-5222` 是死代码）
//   - `src/M2Server/Player/PlayObject.Attack.cs:200-213` `CheckSkillProficiency`
//   - `src/M2Server/Magic/MagicManager.cs:644-670` `DoSpell` 共享尾（内联块，source-cited）
//
// 用法（**在仓库根运行**）：
//   dotnet run --project mir2-rs/tests/parity/m3/spell-tail-probe -c Release -- <仓库根> <向量输出>
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
using M2Server.Player;
using OpenMir2;
using OpenMir2.Data;
using OpenMir2.Enums;
using OpenMir2.Packets.ClientPackets;
using Serilog;
using SystemModule;
using SystemModule.Actors;
using SystemModule.Const;

namespace SpellTailProbe;

internal static class Program
{
    private const int RngSeed = 20261012;

    /// <summary>`System.Random` 的记录包装：按 `ReplayRandom` 的行式格式（seq|kind|arg|result|site）记账。</summary>
    private sealed class RecordingRandom : Random
    {
        public readonly List<string> Draws = new();
        public RecordingRandom(int seed) : base(seed) { }

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

    // ---- 锚点自检（防 F-1 同族的行号漂移）------------------------------------------------------
    private static void VerifyAnchors(string repoRoot)
    {
        CheckMethod(repoRoot, "src/M2Server/Player/PlayObject.cs", 4393, 4400, "TrainSkill(");
        CheckMethod(repoRoot, "src/M2Server/Player/PlayObject.cs", 5198, 5226, "CheckMagicLevelUp(");
        CheckMethod(repoRoot, "src/M2Server/Player/PlayObject.Attack.cs", 200, 213, "CheckSkillProficiency(");

        // DoSpell 共享尾是内联块（不是独立方法）⇒ 逐行锚点。
        const string mm = "src/M2Server/Magic/MagicManager.cs";
        CheckLine(repoRoot, mm, 644, "if (boSpellFail)");
        CheckLine(repoRoot, mm, 646, "return false;");
        CheckLine(repoRoot, mm, 648, "if (boSpellFire)");
        CheckLine(repoRoot, mm, 659, "if (userMagic.Level < 3 && boTrain)");
        CheckLine(repoRoot, mm, 661, "userMagic.Magic.TrainLevel[userMagic.Level] <= playObject.Abil.Level");
        CheckLine(repoRoot, mm, 663, "playObject.TrainSkill(userMagic, M2Share.RandomNumber.Random(3) + 1)");
        CheckLine(repoRoot, mm, 664, "playObject.CheckMagicLevelUp(userMagic)");
        CheckLine(repoRoot, mm, 666, "playObject.SendSelfDelayMsg(Messages.RM_MAGIC_LVEXP");
        CheckLine(repoRoot, mm, 670, "return true;");
    }

    private static void CheckMethod(string repoRoot, string rel, int first, int last, string needle)
    {
        string[] lines = ReadOracle(repoRoot, rel);
        if (first < 1 || last > lines.Length)
        {
            throw new InvalidOperationException($"{rel} 行号越界: {first}-{last} / 共 {lines.Length}");
        }
        if (lines[first - 1].IndexOf(needle, StringComparison.Ordinal) < 0)
        {
            throw new InvalidOperationException($"{rel}:{first} 首行不含 {needle}: {lines[first - 1].Trim()}");
        }
        if (lines[last - 1].Trim() != "}")
        {
            throw new InvalidOperationException($"{rel}:{last} 末行不是 }}: {lines[last - 1].Trim()}");
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
    }

    private static string[] ReadOracle(string repoRoot, string rel)
        => File.ReadAllLines(Path.Combine(repoRoot, rel.Replace('/', Path.DirectorySeparatorChar)));

    // ---- 真身入口反射句柄 -------------------------------------------------------------------------
    private static MethodInfo _checkSkillProficiency;
    private static FieldInfo _msgQueueField;

    private static (PlayObject Po, UserMagic Um) NewPair(byte abilLevel, byte initLevel, int initTp, MagicInfo magic)
    {
        var po = new PlayObject
        {
            Race = ActorRace.Play,
            Job = PlayerJob.Warrior,
            Abil = new Ability { Level = abilLevel, HP = 500, MP = 500, MaxHP = 500, MaxMP = 500 },
            AddAbil = new AddAbility(),
            MagicList = new List<UserMagic>(),
            ItemList = new List<UserItem>(),
            UseItems = new UserItem[13],
            IsRobot = false,
            AbilSeeHealGauge = false,
        };
        po.WAbil = new Ability { DC = 0x1E14, HP = 500, MP = 500, MaxHP = 500, MaxMP = 500 };
        var um = new UserMagic
        {
            Magic = magic,
            MagIdx = magic.MagicId,
            Level = initLevel,
            TranPoint = initTp,
            Key = (char)0,
        };
        po.MagicList.Add(um);
        return (po, um);
    }

    private static MagicInfo NewMagic(ushort id, string name, byte trainLv, byte[] trainLevel, int[] maxTrain)
    {
        var m = new MagicInfo
        {
            MagicId = id,
            MagicName = name,
            TrainLv = trainLv,
            Job = 0,
        };
        m.TrainLevel = trainLevel ?? new byte[4];
        m.MaxTrain = maxTrain ?? new int[4];
        return m;
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

    private static string MagicCols(MagicInfo m)
        => string.Join("\t", m.MagicId, m.TrainLv,
            m.MaxTrain[0], m.MaxTrain[1], m.MaxTrain[2], m.MaxTrain[3],
            m.TrainLevel[0], m.TrainLevel[1], m.TrainLevel[2], m.TrainLevel[3]);

    // ---- 尾部行为登记：no_effect_case / no_switch_case 的唯一效果路径 -----------------------------
    //
    // `tail_effect`（本片新增的第四轴）：
    //   `case_train`           = case 体只置 `boTrain = true`（49/51/52/53/54）⇒ 唯一效果是 :659 训练尾
    //   `case_break_no_train`  = case 体 `break`（43）⇒ 连训练都没有，只有 :648 RM_MAGICFIRE
    //   `no_case_no_train`     = switch 无 case（55-80）⇒ 同 43，只有 :648 RM_MAGICFIRE
    private static string TailEffect(ushort id, string dospellClass)
    {
        if (dospellClass == "no_switch_case")
        {
            return "no_case_no_train";
        }
        return id == 43 ? "case_break_no_train" : "case_train";
    }

    private static readonly Dictionary<ushort, string> TailAnchor = new()
    {
        [43] = "MagicManager.cs:555-556（case SKILL_43: break;）",
        [49] = "MagicManager.cs:607-609（case SKILL_49: boTrain = true; break;）",
        [51] = "MagicManager.cs:631-633（case SKILL_51: boTrain = true; break;）",
        [52] = "MagicManager.cs:634-636（case SKILL_52: boTrain = true; break;）",
        [53] = "MagicManager.cs:637-639（case SKILL_53: boTrain = true; break;）",
        [54] = "MagicManager.cs:640-642（case SKILL_54: boTrain = true; break;）",
    };

    /// <summary>从 5th 片已入库的 `skill_vectors.tsv` 读全部 `reg` 行（保持装表顺序）。</summary>
    private static List<(int RowIndex, ushort Id, string Name, string Class)> ReadRegistry(string repoRoot)
    {
        string path = Path.Combine(repoRoot, "mir2-rs", "tests", "parity", "m3", "skill-probe",
            "vectors", "skill_vectors.tsv");
        var rows = new List<(int, ushort, string, string)>();
        int idx = 0;
        foreach (string line in File.ReadAllLines(path))
        {
            if (!line.StartsWith("reg\t", StringComparison.Ordinal))
            {
                continue;
            }
            idx++;
            string[] f = line.Split('\t');
            rows.Add((idx, ushort.Parse(f[1], CultureInfo.InvariantCulture), f[2], f[8]));
        }
        return rows;
    }

    // ---- 用例定义 ---------------------------------------------------------------------------------
    private sealed record TrainCase(string Name, bool FastTrain, int NTranPoint, int InitTp);

    private sealed record ChkLvCase(string Name, MagicInfo Magic, byte InitLevel, int InitTp);

    private sealed record ProfCase(string Name, bool FastTrain, byte AbilLevel, MagicInfo Magic,
        byte InitLevel, int InitTp);

    private static void RunTrain(List<object> cases, List<string> tsv, TrainCase def)
    {
        (PlayObject po, UserMagic um) = NewPair(40, 0, def.InitTp, NewMagic(1, "火球术", 3, new byte[4], new int[4]));
        po.FastTrain = def.FastTrain;
        po.TrainSkill(um, def.NTranPoint);
        tsv.Add(string.Join("\t", "train", def.Name, def.FastTrain ? 1 : 0, def.NTranPoint, def.InitTp, um.TranPoint));
        cases.Add(new
        {
            name = def.Name,
            fast_train = def.FastTrain,
            n_tran_point = def.NTranPoint,
            init_tp = def.InitTp,
            out_tp = um.TranPoint,
        });
    }

    private static void RunChkLv(List<object> cases, List<string> tsv, ChkLvCase def)
    {
        (PlayObject po, UserMagic um) = NewPair(40, def.InitLevel, def.InitTp, def.Magic);
        bool result = po.CheckMagicLevelUp(um);
        (int count, string msgs) = DrainMsgs(po);
        tsv.Add(string.Join("\t", "chklv", def.Name, MagicCols(def.Magic), def.InitLevel, def.InitTp,
            result ? 1 : 0, um.Level, um.TranPoint, po.AbilSeeHealGauge ? 1 : 0, msgs));
        cases.Add(new
        {
            name = def.Name,
            magic_id = (int)def.Magic.MagicId,
            train_lv = (int)def.Magic.TrainLv,
            max_train = def.Magic.MaxTrain,
            train_level = def.Magic.TrainLevel,
            init_level = (int)def.InitLevel,
            init_tp = def.InitTp,
            out_result = result,
            out_level = (int)um.Level,
            out_tp = um.TranPoint,
            see_heal_gauge = po.AbilSeeHealGauge,
            msg_count = count,
            msg = msgs,
        });
    }

    private static void RunProf(List<object> cases, List<string> tsv, ProfCase def)
    {
        (PlayObject po, UserMagic um) = NewPair(def.AbilLevel, def.InitLevel, def.InitTp, def.Magic);
        po.FastTrain = def.FastTrain;
        // 攻击路径 `AttackSuccess` 的调用方**先判** `attackMagic.Level < 3`，再抽 `Random(3)+1`
        // （与 DoSpell 尾部相反顺序）；Level >= 3 时调用方根本不该抽/调。
        // 用 `RandomNumber.GetInstance()` 而非 `M2Share.RandomNumber`——两者共用同一私有静态
        // `System.Random` 字段，但后者会触发 `M2Share` 静态构造（起 NetChannel 等），本探针不起栈。
        SeedRng(RngSeed ^ def.Magic.MagicId ^ def.InitLevel);
        int nTranPoint;
        string draws;
        if (def.InitLevel < 3)
        {
            nTranPoint = RandomNumber.GetInstance().Random(3) + 1;
            draws = Draws();
        }
        else
        {
            nTranPoint = 0;
            draws = string.Empty;
        }
        _checkSkillProficiency.Invoke(po, new object[] { nTranPoint, um });
        (int count, string msgs) = DrainMsgs(po);
        tsv.Add(string.Join("\t", "prof", def.Name, def.FastTrain ? 1 : 0, def.AbilLevel, MagicCols(def.Magic),
            def.InitLevel, def.InitTp, nTranPoint, um.TranPoint, um.Level,
            po.AbilSeeHealGauge ? 1 : 0, msgs, draws));
        cases.Add(new
        {
            name = def.Name,
            fast_train = def.FastTrain,
            abil_level = (int)def.AbilLevel,
            magic_id = (int)def.Magic.MagicId,
            train_lv = (int)def.Magic.TrainLv,
            max_train = def.Magic.MaxTrain,
            train_level = def.Magic.TrainLevel,
            init_level = (int)def.InitLevel,
            init_tp = def.InitTp,
            n_tran_point = nTranPoint,
            out_level = (int)um.Level,
            out_tp = um.TranPoint,
            see_heal_gauge = po.AbilSeeHealGauge,
            msg_count = count,
            msg = msgs,
            draws,
        });
    }

    public static int Main(string[] args)
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        LogService.Logger = new LoggerConfiguration().CreateLogger();

        string repoRoot = args.Length > 0 ? args[0] : ".";
        string outPath = args.Length > 1
            ? args[1]
            : Path.Combine("mir2-rs", "tests", "parity", "m3", "spell-tail-probe", "vectors", "spell_tail_vectors.json");

        VerifyAnchors(repoRoot);

        _checkSkillProficiency = typeof(PlayObject).GetMethod("CheckSkillProficiency",
            BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingMethodException("PlayObject.CheckSkillProficiency");
        MsgQueueField();

        // 技能 A：火球术 1，TrainLv=3，MaxTrain=[100,200,300,400]，TrainLevel=[7,9,11,0]
        MagicInfo fb = NewMagic(1, "火球术", 3, new byte[] { 7, 9, 11, 0 }, new[] { 100, 200, 300, 400 });
        // 技能 B：心灵启示 28（`CheckSeeHealGauge` 只对 28 生效）
        MagicInfo showHp = NewMagic(28, "心灵启示", 3, new byte[] { 7, 9, 11, 0 }, new[] { 100, 200, 300, 400 });
        // 技能 C：TrainLv=1（`TrainLv < Level` 的 nLevel=0 分支）
        MagicInfo lowLv = NewMagic(50, "冰焰", 1, new byte[] { 5, 0, 0, 0 }, new[] { 80, 160, 240, 320 });

        var trainCases = new List<TrainCase>
        {
            new("train_plain_1", false, 1, 100),
            new("train_plain_3", false, 3, 100),
            new("train_fast_1", true, 1, 100),
            new("train_fast_3", true, 3, 100),
            new("train_fast_0", true, 0, 100),
        };

        var chklvCases = new List<ChkLvCase>
        {
            new("chklv_below", fb, 0, 99),
            new("chklv_exact_lv0", fb, 0, 100),
            new("chklv_lv1_partial", fb, 1, 250),
            new("chklv_one_level_only", fb, 0, 500),
            new("chklv_maxlevel3", fb, 3, 9999),
            new("chklv_level4_gate", fb, 4, 9999),
            new("chklv_trainlv_lt_level", lowLv, 2, 9999),
            new("chklv_showhp_lv1_to_2", showHp, 1, 200),
            new("chklv_showhp_lv0_to_1", showHp, 0, 100),
        };

        var profCases = new List<ProfCase>
        {
            new("prof_gate_level3", false, 40, fb, 3, 0),
            new("prof_gate_charlevel_low", false, 6, fb, 0, 0),
            new("prof_train_no_levelup", false, 40, fb, 0, 0),
            new("prof_train_levelup", false, 40, fb, 0, 99),
            new("prof_fast_train_levelup", true, 40, fb, 0, 97),
            new("prof_showhp_see_gauge", false, 40, showHp, 1, 199),
            new("prof_lowtrainlv", false, 40, lowLv, 1, 79),
        };

        var tsv = new List<string>
        {
            string.Join("\t", "cfg", RngSeed),
        };
        var trainObjs = new List<object>();
        foreach (TrainCase c in trainCases)
        {
            RunTrain(trainObjs, tsv, c);
        }
        var chklvObjs = new List<object>();
        foreach (ChkLvCase c in chklvCases)
        {
            RunChkLv(chklvObjs, tsv, c);
        }
        var profObjs = new List<object>();
        foreach (ProfCase c in profCases)
        {
            RunProf(profObjs, tsv, c);
        }

        // 第四轴登记：no_effect_case 6 + no_switch_case 20 = 26 行，逐行附 tail_effect。
        var reg = ReadRegistry(repoRoot);
        var reachObjs = new List<object>();
        int reachCount = 0;
        foreach ((int rowIndex, ushort id, string name, string cls) in reg)
        {
            if (cls != "no_effect_case" && cls != "no_switch_case")
            {
                continue;
            }
            string effect = TailEffect(id, cls);
            string anchor = TailAnchor.TryGetValue(id, out string a)
                ? a
                : "MagicManager.cs:172-641 的 switch 无此魔号 case（⇒ :648 RM_MAGICFIRE，无训练）";
            tsv.Add(string.Join("\t", "reach", rowIndex, id, name, cls, effect, anchor));
            reachObjs.Add(new
            {
                row_index = rowIndex,
                magic_id = (int)id,
                name,
                dospell_class = cls,
                tail_effect = effect,
                anchor,
            });
            reachCount++;
        }
        if (reachCount != 26)
        {
            throw new InvalidOperationException($"第四轴登记行数应为 26（6+20），实得 {reachCount}");
        }

        var payload = new
        {
            generated_by = "mir2-rs/tests/parity/m3/spell-tail-probe",
            oracle = new
            {
                train_skill = "M2Server.Player.PlayObject.TrainSkill (public, 直调)",
                check_magic_level_up = "M2Server.Player.PlayObject.CheckMagicLevelUp (public, 直调)",
                check_skill_proficiency = "M2Server.Player.PlayObject.CheckSkillProficiency (private, 反射调用)",
                source = "src/M2Server/Player/PlayObject.cs:4393-4400 / :5198-5226；PlayObject.Attack.cs:200-213；MagicManager.cs:644-670",
                rng = "OpenMir2.RandomNumber.GetInstance().Random(3)（私有静态 System.Random 反射覆盖为 RecordingRandom(seed)）",
                note = "DoSpell 共享尾是内联块 ⇒ 只 source-cited（锚点见 VerifyAnchors），本探针不冒充其真身向量。",
            },
            seed = RngSeed,
            train_cases = trainObjs,
            chklv_cases = chklvObjs,
            prof_cases = profObjs,
            reach_registry = reachObjs,
        };

        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outPath)));
        string json = JsonSerializer.Serialize(payload, new JsonSerializerOptions { WriteIndented = true });
        File.WriteAllText(outPath, json + Environment.NewLine, new UTF8Encoding(false));
        string tsvPath = Path.ChangeExtension(outPath, ".tsv");
        File.WriteAllText(tsvPath, string.Join("\n", tsv) + "\n", new UTF8Encoding(false));
        Console.WriteLine(
            $"spell-tail-probe: wrote {Path.GetFullPath(outPath)} ({trainObjs.Count} train + {chklvObjs.Count} chklv + {profObjs.Count} prof + {reachObjs.Count} reach) + {Path.GetFileName(tsvPath)}");
        return 0;
    }
}
