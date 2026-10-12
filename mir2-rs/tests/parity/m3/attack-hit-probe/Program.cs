// attack-hit-probe —— oracle 真身实测「DoSpell 之外的战士近战技能路径」的向量。
//
// 第八片（卡 openmir2-m3-combat-skills-align，DoD③）：第五片的 `skill-probe` 把真魔表里
// `IsWarrSkill` 命中的 8 行（3 基本剑术 / 4 精神力战法 / 7 攻杀剑术 / 12 刺杀剑术 /
// 25 半月弯刀 / 26 烈火剑法 / 27 野蛮冲撞 / 34 解毒术[`MagicConst.SKILL_CROSSMOON`]）
// 登记为 `warr_reject`，reason 写的是「DoSpell 前置即 return；承接 = 战士技能走 ClientHit/Attack 路径」。
// 那条路径此前**没有真身向量**——本探针把它补上，且只挑**能独立直调**的两个入口：
//
//   * `PlayObject.RecalcHitSpeed`（`src/M2Server/Player/PlayObject.cs:3659-3713`，protected void）
//       ——被动技能效果：基本剑术(3)/精神力战法(4)/攻杀剑术(7) 加 `HitPoint`、
//         攻杀剑术(7) 设 `HitPlus`+`AttackSkillCount`+抽 `RandomByte`、烈火剑法(26) 设 `HitDouble`。
//   * `PlayObject.GetAttackPowerHit`（`src/M2Server/Player/PlayObject.Attack.cs:579-625`，protected int）
//       ——主动倍率：mode 3(+攻杀剑术 PowerHit) `nPower += HitPlus`；
//         mode 7(FireHitSkill)/mode 9(TwinHitSkill) `nPower = (ushort)(nPower + Round(nPower/100*HitDouble*10))`；
//         目标为 null 时两个 Hit 分支只清标志、不改威力、不置 canHit。
//
// 其余 4 行（12/25/27/34）体内走 `Envir`/地图目标集（`SwordLongAttack`/`SwordWideAttack`/
// `CrsWideAttack`/`DoMotaebo`）⇒ 本片**只登记可达性 + source-cited**，不冒充真身向量。
//
// 依据（只读，逐字对照 oracle）：
//   - `src/M2Server/Player/PlayObject.cs:3659-3713` `RecalcHitSpeed`
//   - `src/M2Server/Player/PlayObject.Attack.cs:579-625` `GetAttackPowerHit`
//   - `src/M2Server/Player/PlayObject.Attack.cs:84-138` `GetHitMode`（读 MagicArr，本片不产向量）
//   - `src/M2Server/Player/PlayObject.Attack.cs:141-199` `SkillAttackDamage`（尾调需地图）
//   - `src/Modules/SystemModule/MessageSettings.cs:10-11` `DEFHIT=5` / `DEFSPEED=15`
//
// 用法（**在仓库根运行**）：
//   dotnet run --project mir2-rs/tests/parity/m3/attack-hit-probe -c Release -- <仓库根> <向量输出>
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
using M2Server.Monster;
using M2Server.Player;
using OpenMir2;
using OpenMir2.Data;
using OpenMir2.Enums;
using OpenMir2.Packets.ClientPackets;
using Serilog;
using SystemModule;
using SystemModule.Actors;

namespace AttackHitProbe;

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

    // ---- 可达性登记：warr_reject 8 行在 DoSpell 之外的入口 -----------------------------------------
    //
    // `combat_entry`：
    //   `RecalcHitSpeed` / `GetAttackPowerHit`  可被真身反射在无地图/无 socket 的独立对象上直调（本片产向量）
    //   `SkillAttackDamage` / `DoMotaebo`       入口存在，但尾调走 Envir / 地图目标集 ⇒ source-cited
    // `probeability`：`probeable`（本片实测） / `map_context`（不可独立调）
    private sealed record Reach(ushort Id, string Name, string Entry, string Probeability, string Anchor);

    private static readonly Reach[] ReachRows =
    {
        new(3, "基本剑术", "RecalcHitSpeed", "probeable", "PlayObject.cs:3659-3713（:3687 SKILL_ONESWORD 分支）"),
        new(4, "精神力战法", "RecalcHitSpeed", "probeable", "PlayObject.cs:3659-3713（:3693 SKILL_ILKWANG 分支）"),
        new(7, "攻杀剑术", "RecalcHitSpeed+GetAttackPowerHit", "probeable", "PlayObject.cs:3659-3713（:3698 SKILL_YEDO）+ PlayObject.Attack.cs:579-625（mode 3）"),
        new(12, "刺杀剑术", "SkillAttackDamage", "map_context", "PlayObject.Attack.cs:141-199（mode 4 ⇒ SwordLongAttack ⇒ Envir）"),
        new(25, "半月弯刀", "SkillAttackDamage", "map_context", "PlayObject.Attack.cs:141-199（mode 5 ⇒ SwordWideAttack ⇒ Envir）"),
        new(26, "烈火剑法", "RecalcHitSpeed+GetAttackPowerHit", "probeable", "PlayObject.cs:3659-3713（:3708 SKILL_FIRESWORD）+ PlayObject.Attack.cs:579-625（mode 7）"),
        new(27, "野蛮冲撞", "DoMotaebo", "map_context", "PlayObject.Attack.cs:1187 DoMotaebo（⇒ Envir 位移）"),
        new(34, "解毒术", "SkillAttackDamage", "map_context", "PlayObject.Attack.cs:141-199（mode 8 ⇒ CrsWideAttack ⇒ Envir；`MagicConst.SKILL_CROSSMOON=34`）"),
    };

    /// <summary>
    /// 锚点自检（防 F-1 同族的行号漂移）：逐条读 oracle 源文件核对
    /// ① 首行必须含 `方法名(`；② 末行 trim 后必须是 `}`。不符 ⇒ 抛异常终止。
    /// </summary>
    private static void VerifyAnchors(string repoRoot)
    {
        Check(repoRoot, "src/M2Server/Player/PlayObject.cs", 3659, 3713, "RecalcHitSpeed(");
        Check(repoRoot, "src/M2Server/Player/PlayObject.Attack.cs", 579, 625, "GetAttackPowerHit(");
    }

    private static void Check(string repoRoot, string rel, int first, int last, string needle)
    {
        string[] lines = File.ReadAllLines(Path.Combine(repoRoot, rel.Replace('/', Path.DirectorySeparatorChar)));
        if (first < 1 || last > lines.Length)
        {
            throw new InvalidOperationException($"{rel} 行号越界: {first}-{last} / 共 {lines.Length}");
        }
        string head = lines[first - 1];
        string tail = lines[last - 1];
        if (head.IndexOf(needle, StringComparison.Ordinal) < 0)
        {
            throw new InvalidOperationException($"{rel}:{first} 首行不含 {needle}: {head.Trim()}");
        }
        if (tail.Trim() != "}")
        {
            throw new InvalidOperationException($"{rel}:{last} 末行不是 }}: {tail.Trim()}");
        }
    }

    /// <summary>从 5th 片已入库的 `skill_vectors.tsv` 读 `warr_reject` 行（id+名），与探针登记对账。</summary>
    private static List<(ushort Id, string Name)> ReadWarrReject(string repoRoot)
    {
        string path = Path.Combine(repoRoot, "mir2-rs", "tests", "parity", "m3", "skill-probe",
            "vectors", "skill_vectors.tsv");
        var rows = new List<(ushort, string)>();
        foreach (string line in File.ReadAllLines(path))
        {
            if (!line.StartsWith("reg\t", StringComparison.Ordinal))
            {
                continue;
            }
            string[] f = line.Split('\t');
            if (f.Length >= 9 && f[8] == "warr_reject")
            {
                rows.Add((ushort.Parse(f[1], CultureInfo.InvariantCulture), f[2]));
            }
        }
        return rows;
    }

    // ---- 真身入口反射句柄 -------------------------------------------------------------------------
    private static MethodInfo _recalcHitSpeed;
    private static MethodInfo _getAttackPowerHit;

    private static PlayObject NewCaster(PlayerJob job, byte bonusHit, int bonusSpeed,
        byte initHitPoint, byte initSpeedPoint, IEnumerable<(ushort MagIdx, byte Level)> magics)
    {
        var po = new PlayObject
        {
            Race = ActorRace.Play,
            Job = job,
            Abil = new Ability { Level = 40, HP = 500, MP = 500, MaxHP = 500, MaxMP = 500 },
            BonusAbil = new NakedAbility { Hit = bonusHit, Speed = bonusSpeed },
            AddAbil = new AddAbility(),
            MagicList = new List<UserMagic>(),
            ItemList = new List<UserItem>(),
            UseItems = new UserItem[13],
            SlaveList = new List<IMonsterActor>(),
            IsRobot = false,
        };
        po.HitPoint = initHitPoint;
        po.SpeedPoint = initSpeedPoint;
        foreach ((ushort idx, byte lv) in magics)
        {
            po.MagicList.Add(new UserMagic { MagIdx = idx, Level = lv });
        }
        po.WAbil = new Ability { DC = 0x1E14, HP = 500, MP = 500, MaxHP = 500, MaxMP = 500 };
        return po;
    }

    private sealed record RecalcCase(
        string Name,
        PlayerJob Job,
        byte BonusHit,
        int BonusSpeed,
        byte InitHitPoint,
        byte InitSpeedPoint,
        (ushort Id, byte Level)[] Magics);

    private static string MagicSpec((ushort Id, byte Level)[] magics)
        => string.Join(";", magics.Select(m => $"{m.Id}:{m.Level}"));

    /// <summary>oracle 按 `Job` 选 `Config.BonusAbilof{Job}`；把实取的 tick 记进向量（Rust 侧不再猜）。</summary>
    private static (byte Hit, int Speed) TickFor(PlayerJob job) => job switch
    {
        PlayerJob.Warrior => (SystemShare.Config.BonusAbilofWarr.Hit, SystemShare.Config.BonusAbilofWarr.Speed),
        PlayerJob.Wizard => (SystemShare.Config.BonusAbilofWizard.Hit, SystemShare.Config.BonusAbilofWizard.Speed),
        _ => (SystemShare.Config.BonusAbilofTaos.Hit, SystemShare.Config.BonusAbilofTaos.Speed),
    };

    private static void RunRecalc(
        List<object> cases, List<string> tsv, RecalcCase def)
    {
        PlayObject po = NewCaster(def.Job, def.BonusHit, def.BonusSpeed, def.InitHitPoint, def.InitSpeedPoint, def.Magics);
        (byte tickHit, int tickSpeed) = TickFor(def.Job);
        // 种子注入必须在对象构造之后（构造函数本身消耗随机数）。
        SeedRng(RngSeed);
        _recalcHitSpeed.Invoke(po, null);
        string draws = Draws();
        tsv.Add(string.Join("\t", "recalc", def.Name, (int)def.Job, def.BonusHit, def.BonusSpeed,
            tickHit, tickSpeed, def.InitHitPoint, def.InitSpeedPoint, MagicSpec(def.Magics),
            po.HitPoint, po.SpeedPoint, po.HitPlus, po.HitDouble, po.AttackSkillCount,
            po.AttackSkillPointCount, draws));
        cases.Add(new
        {
            name = def.Name,
            job = (int)def.Job,
            bonus_hit = (int)def.BonusHit,
            bonus_speed = def.BonusSpeed,
            tick_hit = (int)tickHit,
            tick_speed = tickSpeed,
            init_hit_point = (int)def.InitHitPoint,
            init_speed_point = (int)def.InitSpeedPoint,
            magics = def.Magics.Select(m => new { id = (int)m.Id, level = (int)m.Level }).ToArray(),
            hit_point = (int)po.HitPoint,
            speed_point = (int)po.SpeedPoint,
            hit_plus = (int)po.HitPlus,
            hit_double = (int)po.HitDouble,
            attack_skill_count = (int)po.AttackSkillCount,
            attack_skill_point_count = (int)po.AttackSkillPointCount,
            draws,
        });
    }

    private sealed record AphCase(string Name, short HitMode, int Power, bool HasTarget,
        ushort HitPlus, ushort HitDouble, bool PowerHit, bool FireHit, bool TwinHit);

    private static void RunAph(List<object> cases, List<string> tsv, AphCase def)
    {
        PlayObject po = NewCaster(PlayerJob.Warrior, 20, 35, 0, 0, Array.Empty<(ushort, byte)>());
        po.HitPlus = def.HitPlus;
        po.HitDouble = def.HitDouble;
        po.PowerHit = def.PowerHit;
        po.FireHitSkill = def.FireHit;
        po.TwinHitSkill = def.TwinHit;
        IActor target = def.HasTarget ? new MonsterObject() : null;
        object[] args = { def.HitMode, def.Power, target, false };
        int result = (int)_getAttackPowerHit.Invoke(po, args);
        bool canHit = (bool)args[3];
        tsv.Add(string.Join("\t", "aph", def.Name, def.HitMode, def.Power, def.HasTarget ? 1 : 0,
            def.HitPlus, def.HitDouble, def.PowerHit ? 1 : 0, def.FireHit ? 1 : 0, def.TwinHit ? 1 : 0,
            result, canHit ? 1 : 0, po.PowerHit ? 1 : 0, po.FireHitSkill ? 1 : 0, po.TwinHitSkill ? 1 : 0));
        cases.Add(new
        {
            name = def.Name,
            hit_mode = (int)def.HitMode,
            power = def.Power,
            has_target = def.HasTarget,
            hit_plus = (int)def.HitPlus,
            hit_double = (int)def.HitDouble,
            power_hit = def.PowerHit,
            fire_hit = def.FireHit,
            twin_hit = def.TwinHit,
            out_power = result,
            can_hit = canHit,
            post_power_hit = po.PowerHit,
            post_fire_hit = po.FireHitSkill,
            post_twin_hit = po.TwinHitSkill,
        });
    }

    public static int Main(string[] args)
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        LogService.Logger = new LoggerConfiguration().CreateLogger();

        string repoRoot = args.Length > 0 ? args[0] : ".";
        string outPath = args.Length > 1
            ? args[1]
            : Path.Combine("mir2-rs", "tests", "parity", "m3", "attack-hit-probe", "vectors", "attack_hit_vectors.json");

        VerifyAnchors(repoRoot);

        _recalcHitSpeed = typeof(PlayObject).GetMethod("RecalcHitSpeed",
            BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingMethodException("PlayObject.RecalcHitSpeed");
        _getAttackPowerHit = typeof(PlayObject).GetMethod("GetAttackPowerHit",
            BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingMethodException("PlayObject.GetAttackPowerHit");

        // 固定 BonusAbilofWarr/Wizard/Taos（记录进 cfg 行，Rust 侧同值重建）。
        SystemShare.Config.BonusAbilofWarr = new NakedAbility { Hit = 20, Speed = 35 };
        SystemShare.Config.BonusAbilofWizard = new NakedAbility { Hit = 25, Speed = 35 };
        SystemShare.Config.BonusAbilofTaos = new NakedAbility { Hit = 30, Speed = 30 };

        var recalcCases = new List<RecalcCase>
        {
            new("warr_naked", PlayerJob.Warrior, 40, 70, 0, 0, Array.Empty<(ushort, byte)>()),
            new("warr_basic3", PlayerJob.Warrior, 40, 70, 0, 0, new[] { ((ushort)3, (byte)3) }),
            new("warr_ilkwang2", PlayerJob.Warrior, 40, 70, 0, 0, new[] { ((ushort)4, (byte)2) }),
            new("warr_yedo_lv1", PlayerJob.Warrior, 40, 70, 0, 0, new[] { ((ushort)7, (byte)1) }),
            new("warr_yedo_lv3", PlayerJob.Warrior, 40, 70, 0, 0, new[] { ((ushort)7, (byte)3) }),
            new("warr_firesword_lv3", PlayerJob.Warrior, 40, 70, 0, 0, new[] { ((ushort)26, (byte)3) }),
            new("warr_all_four", PlayerJob.Warrior, 40, 70, 0, 0,
                new[] { ((ushort)3, (byte)3), ((ushort)4, (byte)2), ((ushort)7, (byte)2), ((ushort)26, (byte)1) }),
            new("wizard_hit", PlayerJob.Wizard, 50, 70, 0, 0, Array.Empty<(ushort, byte)>()),
            new("taos_speed", PlayerJob.Taoist, 50, 60, 0, 0, Array.Empty<(ushort, byte)>()),
            new("taos_init_hp_kept", PlayerJob.Taoist, 50, 60, 7, 9, Array.Empty<(ushort, byte)>()),
        };

        var aphCases = new List<AphCase>();
        foreach (bool hasTarget in new[] { true, false })
        {
            string t = hasTarget ? "t" : "n";
            aphCases.Add(new AphCase($"mode3_powerhit_{t}", 3, 25, hasTarget, 9, 0, true, false, false));
            aphCases.Add(new AphCase($"mode3_nopowerhit_{t}", 3, 25, hasTarget, 9, 0, false, false, false));
            aphCases.Add(new AphCase($"mode7_fire_low_{t}", 7, 25, hasTarget, 0, 8, false, true, false));
            aphCases.Add(new AphCase($"mode7_fire_high_{t}", 7, 250, hasTarget, 0, 8, false, true, false));
            aphCases.Add(new AphCase($"mode7_nofire_{t}", 7, 250, hasTarget, 0, 8, false, false, false));
            aphCases.Add(new AphCase($"mode9_twin_high_{t}", 9, 333, hasTarget, 0, 12, false, false, true));
            aphCases.Add(new AphCase($"mode9_notwin_{t}", 9, 333, hasTarget, 0, 12, false, false, false));
            aphCases.Add(new AphCase($"mode0_default_{t}", 0, 77, hasTarget, 5, 6, true, true, true));
        }

        var tsv = new List<string>
        {
            string.Join("\t", "cfg", RngSeed, 20, 35, 25, 35, 30, 30),
        };
        var recalcObjs = new List<object>();
        foreach (RecalcCase c in recalcCases)
        {
            RunRecalc(recalcObjs, tsv, c);
        }
        var aphObjs = new List<object>();
        foreach (AphCase c in aphCases)
        {
            RunAph(aphObjs, tsv, c);
        }

        // 可达性登记：以已入库的 warr_reject 集合为准，逐条附上本片的 combat_entry。
        List<(ushort Id, string Name)> warr = ReadWarrReject(repoRoot);
        ushort[] expectedIds = ReachRows.Select(r => r.Id).ToArray();
        ushort[] actualIds = warr.Select(w => w.Id).OrderBy(x => x).ToArray();
        if (!expectedIds.OrderBy(x => x).SequenceEqual(actualIds))
        {
            throw new InvalidOperationException(
                $"warr_reject 集合与本片登记不一致: 期望 [{string.Join(",", expectedIds.OrderBy(x => x))}] 实得 [{string.Join(",", actualIds)}]");
        }
        foreach (Reach r in ReachRows)
        {
            string name = warr.First(w => w.Id == r.Id).Name;
            if (name != r.Name)
            {
                throw new InvalidOperationException($"魔表行 {r.Id} 名字不符: 登记 {r.Name} / 实得 {name}");
            }
            tsv.Add(string.Join("\t", "reach", r.Id, name, r.Entry, r.Probeability, r.Anchor));
        }

        var payload = new
        {
            generated_by = "mir2-rs/tests/parity/m3/attack-hit-probe",
            oracle = new
            {
                recalc_entry = "M2Server.Player.PlayObject.RecalcHitSpeed (protected, 反射调用)",
                aph_entry = "M2Server.Player.PlayObject.GetAttackPowerHit (protected, 反射调用)",
                source = "src/M2Server/Player/PlayObject.cs:3659-3713 + PlayObject.Attack.cs:579-625",
                rng = "OpenMir2.RandomNumber.random (私有静态 System.Random) 反射覆盖为 RecordingRandom(seed)",
                constants = "MessageSettings.DEFHIT=5 / DEFSPEED=15；BonusAbilofWarr{Hit=20,Speed=35} / Wizard{Hit=25,Speed=35} / Taos{Speed=30}",
            },
            seed = RngSeed,
            recalc_cases = recalcObjs,
            aph_cases = aphObjs,
            reach_registry = ReachRows.Select(r => new
            {
                magic_id = (int)r.Id,
                name = r.Name,
                combat_entry = r.Entry,
                probeability = r.Probeability,
                anchor = r.Anchor,
            }).ToArray(),
        };

        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outPath)));
        string json = JsonSerializer.Serialize(payload, new JsonSerializerOptions { WriteIndented = true });
        File.WriteAllText(outPath, json + Environment.NewLine, new UTF8Encoding(false));
        string tsvPath = Path.ChangeExtension(outPath, ".tsv");
        File.WriteAllText(tsvPath, string.Join("\n", tsv) + "\n", new UTF8Encoding(false));
        Console.WriteLine(
            $"attack-hit-probe: wrote {Path.GetFullPath(outPath)} ({recalcObjs.Count} recalc + {aphObjs.Count} aph + {ReachRows.Length} reach) + {Path.GetFileName(tsvPath)}");
        return 0;
    }
}
