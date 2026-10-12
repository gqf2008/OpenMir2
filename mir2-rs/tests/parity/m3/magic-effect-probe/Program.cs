// magic-effect-probe —— oracle 真身实测「DoSpell 里 context_required 技能效果」的向量。
//
// 第七片（卡 openmir2-m3-combat-skills-align，DoD③）：
//   ① 把真魔表每一行的「效果入口」按可达性二次分类（在第五片的 DoSpell 分派类别之上再加一轴）：
//      implemented / precondition_reject / mag_manager_method / dospell_inline / no_effect / no_dispatch，
//      并对有独立方法的行标注能否被**真身反射调用**（probeability）。
//   ② 对其中「有独立可调方法且不依赖地图/可见集/召唤」的两条——32 圣言术 `MagTurnUndead`
//      与 20 诱惑之光 `MagTamming`——用真身反射直调，产出**同种子取数账本 + 可观测输出**向量。
//
// 依据（只读，逐字对照 oracle）：
//   - `src/M2Server/Magic/MagicManager.cs:145` `DoSpell`（分派与前置）
//   - `:827` `MagTurnUndead`、`:704` `MagTamming`（本片两条真身向量）
//   - `:24` `MagPushArround`、`:1308` `MagMakeSlave`、`:876` `MagSaceMove`、`:1269` `MagMakeSinSuSlave`、
//     `:1343` `MagMakeAngelSlave`、`:1337` `MagMakeClone`（需地图/可见集/召唤 ⇒ 本片只登记可达性）
//
// 用法（**在仓库根运行**）：
//   dotnet run --project mir2-rs/tests/parity/m3/magic-effect-probe -c Release -- <仓库根> <向量输出> <登记输出>
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
using M2Server.Monster;
using M2Server.Player;
using OpenMir2;
using OpenMir2.Data;
using OpenMir2.Enums;
using OpenMir2.Packets.ClientPackets;
using Serilog;
using SystemModule;
using SystemModule.Actors;

namespace MagicEffectProbe;

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

    private static PlayObject NewCaster(int job, int level)
    {
        var po = new PlayObject
        {
            Race = ActorRace.Play,
            Job = (PlayerJob)job,
            Abil = new Ability { Level = (byte)level, HP = 500, MP = 500, MaxHP = 500, MaxMP = 500 },
            AddAbil = new AddAbility(),
            MagicList = new List<UserMagic>(),
            ItemList = new List<UserItem>(),
            UseItems = new UserItem[13],
            SlaveList = new List<IMonsterActor>(),
            IsRobot = false,
        };
        po.WAbil = new Ability { DC = 0x1E14, MC = 0x1E14, SC = 0x2832, HP = 500, MP = 500, MaxHP = 500, MaxMP = 500 };
        po.HitPoint = 20;
        return po;
    }

    private static MonsterObject NewMonster(byte lifeAttrib, bool superMan, bool animal, byte level, bool noTame, bool canReAlive, ushort maxHp = 500)
    {
        var mo = new MonsterObject();
        mo.Race = ActorRace.Monster;
        mo.ChrName = "effect_probe_target";
        mo.ActorId = 910001;
        mo.LifeAttrib = lifeAttrib;
        mo.SuperMan = superMan;
        mo.Animal = animal;
        mo.NoTame = noTame;
        mo.CanReAlive = canReAlive;
        mo.MeatQuality = 30000;
        mo.Abil = new Ability { Level = level, HP = maxHp, MP = 0, MaxHP = maxHp, MaxMP = 0 };
        mo.WAbil = new Ability { AC = 5, HP = maxHp, MaxHP = maxHp };
        mo.AddAbil = new AddAbility();
        return mo;
    }

    // ---- 效果入口的「可达性」二次分类（在 skill-probe 的 dospell_class 之上再加一轴）----------------
    //
    // 口径：
    // - `implemented`         rows with implemented=true in skills_registry.json
    // - `precondition_reject` warr_reject / magicid_reject：DoSpell 前置就 return，无效果体可达
    // - `mag_manager_method`  case 体是**独立可调的 MagicManager 私有方法**（点名 + 行号）
    // - `dospell_inline`      case 体**内联**在 DoSpell switch 里（含内联子 switch），无独立入口
    // - `no_effect`           case 体无副作用（`break` / 只置 boTrain）
    // - `no_dispatch`         switch 无此魔号 case（走通用 RM_MAGICFIRE 尾）
    //
    // `probeability`：
    // - `probeable`     本片已用反射在**无地图/无 socket** 的独立对象上直调成功（真身向量）
    // - `map_context`   方法存在，但体内需要 Envir / 地图对象集 / 召唤 / 可见集 ⇒ 本片不可独立调
    // - `inline`        无独立入口
    // - `reject`        前置拒绝
    // - `none`          无效果
    private sealed record Reach(string Entry, string Anchor, string Probeability);

    private static readonly Dictionary<ushort, Reach> CtxReach = new()
    {
        [8] = new Reach("MagPushArround", "MagicManager.cs:24-52", "map_context"),   // 抗拒火环（走 CharPushed ⇒ Envir）
        [17] = new Reach("MagMakeSlave", "MagicManager.cs:1308-1335", "map_context"), // 召唤骷髅（刷怪 ⇒ Envir）
        [20] = new Reach("MagTamming", "MagicManager.cs:704-825", "probeable"),       // 诱惑之光
        [21] = new Reach("MagSaceMove", "MagicManager.cs:876-891", "map_context"),    // 瞬息移动（MapRandomMove ⇒ Envir）
        [30] = new Reach("MagMakeSinSuSlave", "MagicManager.cs:1269-1306", "map_context"),
        [32] = new Reach("MagTurnUndead", "MagicManager.cs:827-856", "probeable"),    // 圣言术
        [37] = new Reach("MagPushArround", "MagicManager.cs:24-52", "map_context"),   // 雷神之怒（同 MagPushArround 入口）
        [40] = new Reach("<inline>", "MagicManager.cs:495-523", "inline"),            // 十方斩（内联清状态）
        [41] = new Reach("MagMakeAngelSlave", "MagicManager.cs:1343-1354", "map_context"),
        [46] = new Reach("MagMakeClone", "MagicManager.cs:1337-1341", "map_context"), // 分身术（PlayCloneObject ⇒ Envir）
    };

    /// <summary>
    /// 锚点自检（防 F-1 同族的行号漂移）：`mag_manager_method` 的 `<c>文件:首行-末行</c>` 一律约定为
    /// **函数首行（含签名）到函数结尾 `}` 行**，不含其后空行。逐条读 oracle 源文件核对：
    /// ① 首行必须含 `方法名(`；② 末行 trim 后必须是 `}`；③ 末行的下一行必须是空行或 `}`。
    /// 任一不符 ⇒ 抛异常终止（探针宁可不出向量，也不出带错行号的向量）。
    /// </summary>
    private static void VerifyAnchors(string repoRoot)
    {
        string path = Path.Combine(repoRoot, "src", "M2Server", "Magic", "MagicManager.cs");
        string[] lines = File.ReadAllLines(path);
        foreach (Reach r in CtxReach.Values)
        {
            if (r.Entry == "<inline>")
            {
                continue;
            }
            // 形如 MagicManager.cs:START-END（本文件的相对路径部分）
            string span = r.Anchor.Substring(r.Anchor.IndexOf(':') + 1);
            string[] parts = span.Split('-');
            int start = int.Parse(parts[0], CultureInfo.InvariantCulture);
            int end = int.Parse(parts[1], CultureInfo.InvariantCulture);
            string startLine = lines[start - 1];
            string endLine = lines[end - 1];
            if (!startLine.Contains(r.Entry + "("))
            {
                throw new InvalidDataException($"锚点首行不含 {r.Entry}( ：{r.Anchor}\n{startLine}");
            }
            if (endLine.Trim() != "}")
            {
                throw new InvalidDataException($"锚点末行不是 }} ：{r.Anchor}\n{endLine}");
            }
            if (end < lines.Length)
            {
                string next = lines[end].Trim();
                if (next != string.Empty && next != "}")
                {
                    throw new InvalidDataException($"锚点末行下一行非空非 }} （行号少算/多算？）：{r.Anchor}\n{lines[end]}");
                }
            }
        }
    }

    private static Reach Classify(string dospellClass, ushort magicId, bool implemented)
    {
        if (implemented)
        {
            return new Reach("<implemented>", "crates/world/src/magic.rs", "implemented");
        }
        return dospellClass switch
        {
            "warr_reject" => new Reach("IsWarrSkill+DoSpell", "MagicManager.cs:82-100,151", "reject"),
            "magicid_reject" => new Reach("DoSpell", "MagicManager.cs:167", "reject"),
            "no_effect_case" => new Reach("<inline break>", "MagicManager.cs:555-640", "none"),
            "no_switch_case" => new Reach("<none>", "MagicManager.cs:172-640 switch 无分支", "none"),
            "context_required" => CtxReach.TryGetValue(magicId, out Reach r)
                ? r
                : throw new InvalidDataException($"context_required 魔号 {magicId} 未登记效果入口"),
            var other => throw new InvalidDataException($"未登记的分派类别 {other}"),
        };
    }

    private sealed record RegRow(ushort MagicId, string Name, string DospellClass, bool Implemented);

    private static List<RegRow> ReadRegistry(string repoRoot)
    {
        string path = Path.Combine(repoRoot, "mir2-rs", "tests", "parity", "m3", "skill-probe", "vectors", "skills_registry.json");
        using JsonDocument doc = JsonDocument.Parse(File.ReadAllText(path));
        var rows = new List<RegRow>();
        foreach (JsonElement s in doc.RootElement.GetProperty("skills").EnumerateArray())
        {
            rows.Add(new RegRow(
                (ushort)s.GetProperty("magic_id").GetInt32(),
                s.GetProperty("name").GetString(),
                s.GetProperty("dospell_class").GetString(),
                s.GetProperty("implemented").GetBoolean()));
        }
        return rows;
    }

    private static (MethodInfo TurnUndead, MethodInfo Tamming) Methods()
    {
        MethodInfo turnUndead = typeof(MagicManager).GetMethod("MagTurnUndead",
            BindingFlags.NonPublic | BindingFlags.Static)
            ?? throw new MissingMethodException("MagicManager.MagTurnUndead");
        MethodInfo tamming = typeof(MagicManager).GetMethod("MagTamming",
            BindingFlags.NonPublic | BindingFlags.Static)
            ?? throw new MissingMethodException("MagicManager.MagTamming");
        return (turnUndead, tamming);
    }

    public static int Main(string[] args)
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        LogService.Logger = new LoggerConfiguration().CreateLogger();

        string repoRoot = args.Length > 0 ? args[0] : Directory.GetCurrentDirectory();
        string vecPath = args.Length > 1
            ? args[1]
            : Path.Combine(repoRoot, "mir2-rs", "tests", "parity", "m3", "magic-effect-probe", "vectors", "effect_vectors.json");
        string regPath = args.Length > 2
            ? args[2]
            : Path.Combine(repoRoot, "mir2-rs", "tests", "parity", "m3", "magic-effect-probe", "vectors", "effect_registry.json");

        (MethodInfo turnUndead, MethodInfo tamming) = Methods();
        VerifyAnchors(repoRoot);

        var regRows = ReadRegistry(repoRoot);
        int cfgTurnUndeadLevel = SystemShare.Config.MagTurnUndeadLevel;
        int cfgTammingTargetLevel = SystemShare.Config.MagTammingTargetLevel;
        int cfgTammingLevel = SystemShare.Config.MagTammingLevel;
        int cfgTammingCount = SystemShare.Config.MagTammingCount;
        int cfgTammingHpRate = SystemShare.Config.MagTammingHPRate;

        // ---- 可达性登记（108 行；effect_entry + possible probeability）--------------------------------
        var registry = new List<object>();
        var regTsv = new List<string> { string.Join("\t", "cfg",
            cfgTurnUndeadLevel, cfgTammingTargetLevel, cfgTammingLevel, cfgTammingCount, cfgTammingHpRate) };
        foreach (RegRow row in regRows)
        {
            Reach reach = Classify(row.DospellClass, row.MagicId, row.Implemented);
            registry.Add(new
            {
                magic_id = (int)row.MagicId,
                name = row.Name,
                dospell_class = row.DospellClass,
                implemented = row.Implemented,
                effect_entry = reach.Entry,
                oracle_anchor = reach.Anchor,
                probeability = reach.Probeability,
            });
            regTsv.Add(string.Join("\t", "entry",
                row.MagicId, row.Name, row.DospellClass, row.Implemented ? 1 : 0,
                reach.Entry, reach.Anchor, reach.Probeability));
        }

        var cases = new List<object>();
        var vecTsv = new List<string> { string.Join("\t", "seed", RngSeed) };

        // ---- MagTurnUndead（32 圣言术）真身向量 --------------------------------------------------------
        var turnConfigs = new (int CastLevel, byte TargetLevel, byte Life, bool SuperMan, bool Animal, byte MagicLevel, int Seeds)[]
        {
            (50, 20, 1, false, true, 0, 6),   // 可杀：ml=0 阈值低
            (50, 20, 1, false, true, 3, 6),   // 可杀：ml=3 阈值高（与上一行同种子对照）
            (50, 60, 1, false, true, 3, 4),   // 目标等级 60 ≥ MagTurnUndeadLevel(50) ⇒ 不杀
            (50, 20, 0, false, true, 3, 2),   // 非不死 ⇒ 前置 return
            (50, 20, 1, true, true, 3, 2),    // SuperMan ⇒ 前置 return
            (30, 25, 1, false, true, 3, 4),   // 等级差 5
            (50, 20, 1, false, false, 3, 4),  // Animal=false ⇒ Struck 不抽 Random(300)
        };
        foreach (var cfg in turnConfigs)
        {
            for (int k = 0; k < cfg.Seeds; k++)
            {
                int seed = RngSeed ^ (0x3200 + (cfg.CastLevel * 100 + cfg.TargetLevel * 10 + cfg.Life) * 16 + k);
                var caster = NewCaster(1, cfg.CastLevel);
                var target = NewMonster(cfg.Life, cfg.SuperMan, cfg.Animal, cfg.TargetLevel, false, true);
                SeedRng(seed);
                bool result = (bool)turnUndead.Invoke(null, new object[] { caster, target, (short)0, (short)0, cfg.MagicLevel });
                string draws = Draws();
                vecTsv.Add(string.Join("\t", "turn_undead",
                    seed, cfg.CastLevel, (int)cfg.TargetLevel, (int)cfg.Life, cfg.SuperMan ? 1 : 0, cfg.Animal ? 1 : 0,
                    (int)cfg.MagicLevel, cfgTurnUndeadLevel,
                    draws, result ? 1 : 0, (int)target.WAbil.HP,
                    ReferenceEquals(target.TargetCret, caster) ? 1 : 0, target.RunAwayMode ? 1 : 0,
                    ReferenceEquals(caster.TargetCret, target) ? 1 : 0));
                cases.Add(new
                {
                    kind = "turn_undead",
                    seed,
                    caster_level = cfg.CastLevel,
                    target_level = (int)cfg.TargetLevel,
                    life_attrib = (int)cfg.Life,
                    super_man = cfg.SuperMan,
                    animal = cfg.Animal,
                    magic_level = (int)cfg.MagicLevel,
                    cfg_turn_undead_level = cfgTurnUndeadLevel,
                    draws,
                    result,
                    target_hp = (int)target.WAbil.HP,
                    target_target_is_caster = ReferenceEquals(target.TargetCret, caster),
                    target_runaway = target.RunAwayMode,
                    caster_target_is_target = ReferenceEquals(caster.TargetCret, target),
                });
            }
        }

        // ---- MagTamming（20 诱惑之光）真身向量 --------------------------------------------------------
        var tammingConfigs = new (byte TargetLevel, byte Life, bool MasterIsCaster, bool NoTame, int WalkSpeed, ushort MaxHp, bool OtherMaster, byte MagicLevel, int Seeds)[]
        {
            (10, 0, false, false, 0, 500, false, 0, 4),    // 低等级目标、ml=0
            (10, 0, false, false, 0, 500, false, 3, 4),    // 低等级目标、ml=3（Random(1) 恒 0 ⇒ 进第一层）
            (40, 0, false, false, 3000, 100, false, 3, 24), // 高等级目标 + WalkSpeed/NextHitTime 上限分支（受奴后走 1500-200-? 封顶）
            (10, 1, false, false, 0, 500, false, 3, 3),    // 不死系（LA_UNDEAD）
            (10, 0, true, false, 0, 500, false, 3, 3),     // target.Master == caster ⇒ 早分支
            (10, 0, false, true, 0, 500, false, 3, 3),     // NoTame=true
            (10, 0, false, false, 0, 100, false, 3, 24),   // MaxHP=100 ⇒ n14=2 ⇒ 覆盖受奴分支（MasterRoyaltyTick 含 GetTickCount，只核稳定输出）
            (10, 0, false, false, 0, 100, true, 3, 64),    // 已有别的 Master ⇒ 覆盖 HP/10 分支（命中率≈6%，多给种子）
        };
        for (int ci = 0; ci < tammingConfigs.Length; ci++)
        {
            var cfg = tammingConfigs[ci];
            for (int k = 0; k < cfg.Seeds; k++)
            {
                int seed = RngSeed ^ (0x7400 + ci * 4096 + (cfg.TargetLevel * 10 + cfg.Life) * 16 + k);
                var caster = NewCaster(2, 50);
                var target = NewMonster(cfg.Life, false, false, cfg.TargetLevel, cfg.NoTame, true, cfg.MaxHp);
                // 预置一个非 null 的 TargetCret，让「进入诱惑分支时被清空」成为**可判别**的输出
                // （不预置的话构造后本来就是 null，cleared 恒真）。
                target.TargetCret = caster;
                if (cfg.MasterIsCaster)
                {
                    target.Master = caster;
                    caster.SlaveList.Add(target);
                }
                else if (cfg.OtherMaster)
                {
                    target.Master = NewMonster(0, false, false, 30, false, true);
                }
                int inWalk = target.WalkSpeed;
                int inNext = target.NextHitTime;
                if (cfg.WalkSpeed > 0)
                {
                    target.WalkSpeed = cfg.WalkSpeed;
                    target.NextHitTime = cfg.WalkSpeed;
                    inWalk = cfg.WalkSpeed;
                    inNext = cfg.WalkSpeed;
                }
                SeedRng(seed);
                bool result = (bool)tamming.Invoke(null, new object[] { caster, target, (short)0, (short)0, cfg.MagicLevel });
                string draws = Draws();
                vecTsv.Add(string.Join("\t", "tamming",
                    seed, 50, (int)cfg.TargetLevel, (int)target.Race,
                    (int)cfg.Life, cfg.NoTame ? 1 : 0, cfg.MasterIsCaster ? 1 : 0, cfg.OtherMaster ? 1 : 0, caster.SlaveList.Count,
                    (int)cfg.MaxHp, (int)cfg.MagicLevel, cfgTammingTargetLevel, cfgTammingLevel, cfgTammingCount, cfgTammingHpRate,
                    inWalk, inNext,
                    draws, result ? 1 : 0, (int)target.WAbil.HP,
                    target.TargetCret == null ? 1 : 0, ReferenceEquals(target.Master, caster) ? 1 : 0,
                    target.WalkSpeed, target.NextHitTime, target.CrazyMode ? 1 : 0));
                cases.Add(new
                {
                    kind = "tamming",
                    seed,
                    caster_level = 50,
                    target_level = (int)cfg.TargetLevel,
                    target_race = (int)target.Race,
                    life_attrib = (int)cfg.Life,
                    no_tame = cfg.NoTame,
                    master_is_caster = cfg.MasterIsCaster,
                    other_master = cfg.OtherMaster,
                    slave_count = caster.SlaveList.Count,
                    target_abil_max_hp = (int)cfg.MaxHp,
                    magic_level = (int)cfg.MagicLevel,
                    cfg_tamming_target_level = cfgTammingTargetLevel,
                    cfg_tamming_level = cfgTammingLevel,
                    cfg_tamming_count = cfgTammingCount,
                    cfg_tamming_hp_rate = cfgTammingHpRate,
                    draws,
                    result,
                    target_hp = (int)target.WAbil.HP,
                    target_target_cleared = target.TargetCret == null,
                    target_master_is_caster = ReferenceEquals(target.Master, caster),
                    walk_speed = target.WalkSpeed,
                    next_hit_time = target.NextHitTime,
                    crazy_mode = target.CrazyMode,
                });
            }
        }

        var payload = new
        {
            generated_by = "mir2-rs/tests/parity/m3/magic-effect-probe",
            oracle = new
            {
                mag_turn_undead = "M2Server.Magic.MagicManager.MagTurnUndead（src/M2Server/Magic/MagicManager.cs:827，private static，反射调用）",
                mag_tamming = "M2Server.Magic.MagicManager.MagTamming（src/M2Server/Magic/MagicManager.cs:704，private static，反射调用）",
                rng = "OpenMir2.RandomNumber.random 反射覆盖为 RecordingRandom(seed)：记录每次 Next/Next(max)/Next(min,max) 的实参/返回值",
                config = $"SystemShare.Config：MagTurnUndeadLevel={cfgTurnUndeadLevel} / MagTammingTargetLevel={cfgTammingTargetLevel} / MagTammingLevel={cfgTammingLevel} / MagTammingCount={cfgTammingCount} / MagTammingHPRate={cfgTammingHpRate}",
            },
            seed = RngSeed,
            cases,
        };

        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(vecPath)));
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(regPath)));
        var opts = new JsonSerializerOptions { WriteIndented = true };
        File.WriteAllText(vecPath, JsonSerializer.Serialize(payload, opts) + Environment.NewLine, new UTF8Encoding(false));
        File.WriteAllText(Path.ChangeExtension(vecPath, ".tsv"), string.Join("\n", vecTsv) + "\n", new UTF8Encoding(false));
        File.WriteAllText(regPath, JsonSerializer.Serialize(new
        {
            generated_by = "mir2-rs/tests/parity/m3/magic-effect-probe",
            source = "skill-probe/vectors/skills_registry.json（dospell_class 口径不变）+ 本片可达性二次分类",
            total_magic_rows = regRows.Count,
            entries = registry,
        }, opts) + Environment.NewLine, new UTF8Encoding(false));
        File.WriteAllText(Path.ChangeExtension(regPath, ".tsv"), string.Join("\n", regTsv) + "\n", new UTF8Encoding(false));
        Console.WriteLine($"magic-effect-probe: wrote {vecPath} ({cases.Count} cases) / {regPath} ({regRows.Count} rows)");
        return 0;
    }
}
