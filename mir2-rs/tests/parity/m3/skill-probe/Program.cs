// skill-probe —— 用 C# oracle 真身实测「技能（前置于 MP / 效果威力 / 冷却）」向量。
//
// 输入是**真魔表**：解析 `sql/mir2_data.sql` 的 `INSERT INTO magics VALUES (…)`，
// 字段映射口径复用 my2-rs `load_magic_db`（= MySqlDB.LoadMagicDB）：
//   MagID/MagName/EffectType/Effect/Spell/Power/MaxPower/DefSpell/DefPower/DefMaxPower/Job/
//   NeedL1/L1Train/NeedL2/L2Train/NeedL3/L3Train/Delay/Descr
//   TrainLevel[0..2]=NeedL1..3，TrainLevel[3]=NeedL3（无 NeedL4），MaxTrain[3]=MaxTrain[2]，TrainLv=3。
//
// 对**每个已实装技能**产一条用例，逐项核四件事：
//   ① 施法前置：`!IsTrainingSkill && (Job==99||Job==char.Job) && Abil.Level >= TrainLevel[0]`（ReadBook 口径）；
//   ② MP 消耗：`GetSpellPoint = Round(Spell / 4.0 * (Level + 1)) + DefSpell`（Level 0..3）；
//   ③ 效果威力：oracle 的 `DoSpell` 各分支里的威力表达式（调用真身子函数，同种子）；
//   ④ 冷却：`Magic.DelayTime`（= `Delay` 列）与 `MagicAttackInterval = DelayTime + MagicHitIntervalTime`。
//
// "已实装" = 本片在 `crates/world/src/magic.rs` 里实现的那批（见 Implemented 表）。
// 其余魔表行逐条登记进 `skills_registry.json`（implemented=false + 承接说明），不许静默漏。
//
// 用法（**在仓库根运行**）：
//   dotnet run --project mir2-rs/tests/parity/m3/skill-probe -c Release -- <sql路径> <向量输出路径> <登记输出路径>
//
// 只读：不连 MySQL、不写任何现网文件、不发起任何 socket。

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
using M2Server.Player;
using OpenMir2;
using OpenMir2.Data;
using OpenMir2.Enums;
using OpenMir2.Packets.ClientPackets;
using Serilog;
using SystemModule;

namespace SkillProbe;

internal static class Program
{
    private const int RngSeed = 20261012;
    private static MethodInfo _getSpellPoint;

    /// <summary>魔表里每一列（按 `INSERT INTO magics VALUES (…)` 的位置）。</summary>
    private sealed class MagicRow
    {
        public int Idx;
        public ushort MagId;
        public string Name;
        public int EffectType;
        public int Effect;
        public ushort Spell;
        public ushort Power;
        public ushort MaxPower;
        public byte DefSpell;
        public byte DefPower;
        public byte DefMaxPower;
        public byte Job;
        public byte NeedL1;
        public int L1Train;
        public byte NeedL2;
        public int L2Train;
        public byte NeedL3;
        public int L3Train;
        public int Delay;
        public string Descr;
    }

    /// <summary>解析 `INSERT INTO \`magics\` VALUES (…);` 行（保持单引号内的逗号）。</summary>
    private static List<MagicRow> ParseMagics(string sqlPath)
    {
        var rows = new List<MagicRow>();
        foreach (string raw in File.ReadAllLines(sqlPath))
        {
            string line = raw.Trim();
            if (!line.StartsWith("INSERT INTO `magics` VALUES (", StringComparison.Ordinal))
            {
                continue;
            }
            int start = line.IndexOf('(');
            int end = line.LastIndexOf(')');
            string body = line.Substring(start + 1, end - start - 1);
            var fields = SplitSqlValues(body);
            if (fields.Count != 20)
            {
                throw new InvalidDataException($"magics 行字段数 {fields.Count} != 20: {line}");
            }
            rows.Add(new MagicRow
            {
                Idx = int.Parse(fields[0]),
                MagId = (ushort)int.Parse(fields[1]),
                Name = fields[2],
                EffectType = int.Parse(fields[3]),
                Effect = int.Parse(fields[4]),
                Spell = (ushort)int.Parse(fields[5]),
                Power = (ushort)int.Parse(fields[6]),
                MaxPower = (ushort)int.Parse(fields[7]),
                DefSpell = (byte)int.Parse(fields[8]),
                DefPower = (byte)int.Parse(fields[9]),
                DefMaxPower = (byte)int.Parse(fields[10]),
                Job = (byte)int.Parse(fields[11]),
                NeedL1 = (byte)int.Parse(fields[12]),
                L1Train = int.Parse(fields[13]),
                NeedL2 = (byte)int.Parse(fields[14]),
                L2Train = int.Parse(fields[15]),
                NeedL3 = (byte)int.Parse(fields[16]),
                L3Train = int.Parse(fields[17]),
                Delay = int.Parse(fields[18]),
                Descr = fields[19],
            });
        }
        return rows;
    }

    private static List<string> SplitSqlValues(string body)
    {
        var outList = new List<string>();
        var sb = new StringBuilder();
        bool inQuote = false;
        for (int i = 0; i < body.Length; i++)
        {
            char c = body[i];
            if (inQuote)
            {
                if (c == '\'')
                {
                    if (i + 1 < body.Length && body[i + 1] == '\'')
                    {
                        sb.Append('\'');
                        i++;
                    }
                    else
                    {
                        inQuote = false;
                    }
                }
                else
                {
                    sb.Append(c);
                }
            }
            else if (c == '\'')
            {
                inQuote = true;
            }
            else if (c == ',')
            {
                outList.Add(sb.ToString().Trim());
                sb.Clear();
            }
            else
            {
                sb.Append(c);
            }
        }
        outList.Add(sb.ToString().Trim());
        return outList;
    }

    private static MagicInfo ToMagicInfo(MagicRow r)
    {
        var m = new MagicInfo
        {
            MagicId = r.MagId,
            MagicName = r.Name,
            EffectType = (byte)r.EffectType,
            Effect = (byte)r.Effect,
            Spell = r.Spell,
            Power = r.Power,
            MaxPower = r.MaxPower,
            TrainLv = 3,
            Job = r.Job,
            DelayTime = r.Delay,
            DefSpell = r.DefSpell,
            DefPower = r.DefPower,
            DefMaxPower = r.DefMaxPower,
            Desc = r.Descr,
        };
        m.TrainLevel[0] = r.NeedL1;
        m.TrainLevel[1] = r.NeedL2;
        m.TrainLevel[2] = r.NeedL3;
        m.TrainLevel[3] = r.NeedL3;
        m.MaxTrain[0] = r.L1Train;
        m.MaxTrain[1] = r.L2Train;
        m.MaxTrain[2] = r.L3Train;
        m.MaxTrain[3] = r.L3Train;
        return m;
    }

    private static UserMagic ToUserMagic(MagicInfo m, byte level)
        => new UserMagic { Magic = m, MagIdx = m.MagicId, Level = level, Key = (char)0, TranPoint = 0 };

    private static void SeedRng(int seed)
    {
        RandomNumber.GetInstance();
        FieldInfo field = typeof(RandomNumber).GetField("random", BindingFlags.NonPublic | BindingFlags.Static)
            ?? throw new InvalidOperationException("找不到 OpenMir2.RandomNumber.random 私有静态字段");
        field.SetValue(null, new Random(seed));
    }

    /// <summary>造一个探针施法者：职业按技能、等级 50、对应系别能力拉满。</summary>
    private static PlayObject NewCaster(int job)
    {
        var po = new PlayObject
        {
            Race = ActorRace.Play,
            Job = (PlayerJob)job,
            Abil = new Ability { Level = 50, HP = 500, MP = 500, MaxHP = 500, MaxMP = 500 },
            AddAbil = new AddAbility(),
            MagicList = new List<UserMagic>(),
            ItemList = new List<UserItem>(),
            UseItems = new UserItem[13],
            IsRobot = false,
        };
        // MC = 0x1E14（Lo=20 Hi=30），SC = 0x2832（Lo=50 Hi=40 → Δ 为负，覆盖 GetRPow 的 else 分支）
        po.WAbil = new Ability { MC = 0x1E14, SC = 0x2832, HP = 500, MP = 500, MaxHP = 500, MaxMP = 500 };
        return po;
    }

    /// <summary>每个已实装技能的"效果威力"表达式类别（与 `crates/world/src/magic.rs` 一一对应）。</summary>
    private static string EffectKind(ushort magIdx) => magIdx switch
    {
        1 or 5 => "get_power_plus_mc",          // 火球术/大火球：GetAttackPower(GetPower(MPow)+Lo(MC), ΔMC+1)
        2 => "heal_get_power_plus_sc2",          // 治愈术：GetAttackPower(GetPower(MPow)+Lo(SC)*2, ΔSC*2+1)
        13 => "get_power_plus_sc",                 // 灵魂火符：GetAttackPower(GetPower(MPow)+Lo(SC), ΔSC+1)
        11 => "lightening_get_power_plus_mc",    // 雷电术：同上 + 不死 ×1.5
        6 => "amyounsul_power13_rpow",           // 施毒术：GetPower13(40|30)+GetRPow(SC)*2
        14 or 15 => "power13_60_sc10",           // 幽灵盾/神圣战甲术：GetAttackPower(GetPower13(60)+Lo(SC)*10, ΔSC+1)
        16 => "power13_40_rpow3",                // 困魔咒：GetPower13(40)+GetRPow(SC)*3
        18 or 19 => "power13_30_rpow3",          // 隐身术/集体隐身术：GetPower13(30)+GetRPow(SC)*3
        _ => "none",
    };

    private static bool IsImplemented(ushort magIdx) => EffectKind(magIdx) != "none";

    private sealed class EffectVal
    {
        public string Kind;
        public int Power = -1;
        public int VsUndead = -1;
        public int Green = -1;
        public int Red = -1;

        /// <summary>TSV 伴生件的 5 列（kind + 4 个数值，缺项为 -1）。</summary>
        public string Tsv => string.Join("\t", Kind, Power, VsUndead, Green, Red);

        public object Json => new
        {
            kind = Kind,
            power = Power,
            vs_undead = VsUndead,
            power_green = Green,
            power_red = Red,
        };
    }

    private static EffectVal ComputeEffect(PlayObject caster, UserMagic um)
    {
        ushort magIdx = um.MagIdx;
        // 每个技能独立子种子（两侧同序），避免跨技能取数顺序耦合。
        SeedRng(RngSeed ^ magIdx);
        switch (EffectKind(magIdx))
        {
            case "get_power_plus_mc":
            {
                int power = caster.GetAttackPower(
                    MagicBase.GetPower(MagicBase.MPow(um), um) + HUtil32.LoByte(caster.WAbil.MC),
                    HUtil32.HiByte(caster.WAbil.MC) - HUtil32.LoByte(caster.WAbil.MC) + 1);
                return new EffectVal { Kind = "damage", Power = power };
            }
            case "get_power_plus_sc":
            {
                int power = caster.GetAttackPower(
                    MagicBase.GetPower(MagicBase.MPow(um), um) + HUtil32.LoByte(caster.WAbil.SC),
                    HUtil32.HiByte(caster.WAbil.SC) - HUtil32.LoByte(caster.WAbil.SC) + 1);
                return new EffectVal { Kind = "damage", Power = power };
            }
            case "heal_get_power_plus_sc2":
            {
                int power = caster.GetAttackPower(
                    MagicBase.GetPower(MagicBase.MPow(um), um) + HUtil32.LoByte(caster.WAbil.SC) * 2,
                    (HUtil32.HiByte(caster.WAbil.SC) - HUtil32.LoByte(caster.WAbil.SC)) * 2 + 1);
                return new EffectVal { Kind = "heal", Power = power };
            }
            case "lightening_get_power_plus_mc":
            {
                int power = caster.GetAttackPower(
                    MagicBase.GetPower(MagicBase.MPow(um), um) + HUtil32.LoByte(caster.WAbil.MC),
                    HUtil32.HiByte(caster.WAbil.MC) - HUtil32.LoByte(caster.WAbil.MC) + 1);
                int vsUndead = HUtil32.Round(power * 1.5);
                return new EffectVal { Kind = "damage", Power = power, VsUndead = vsUndead };
            }
            case "amyounsul_power13_rpow":
            {
                int green = MagicBase.GetPower13(40, um) + MagicBase.GetRPow(caster.WAbil.SC) * 2;
                // 红毒/绿毒各取一次（oracle 按 StdItem.Shape 分支；这里两分支都算）
                SeedRng(RngSeed ^ magIdx);
                int red = MagicBase.GetPower13(30, um) + MagicBase.GetRPow(caster.WAbil.SC) * 2;
                return new EffectVal { Kind = "poison", Green = green, Red = red };
            }
            case "power13_60_sc10":
            {
                int power = caster.GetAttackPower(
                    MagicBase.GetPower13(60, um) + HUtil32.LoByte(caster.WAbil.SC) * 10,
                    HUtil32.HiByte(caster.WAbil.SC) - HUtil32.LoByte(caster.WAbil.SC) + 1);
                return new EffectVal { Kind = "buff_ac", Power = power };
            }
            case "power13_40_rpow3":
            {
                int power = MagicBase.GetPower13(40, um) + MagicBase.GetRPow(caster.WAbil.SC) * 3;
                return new EffectVal { Kind = "buff_holy", Power = power };
            }
            case "power13_30_rpow3":
            {
                int power = MagicBase.GetPower13(30, um) + MagicBase.GetRPow(caster.WAbil.SC) * 3;
                return new EffectVal { Kind = "buff_transparent", Power = power };
            }
            default:
                return null;
        }
    }

    public static int Main(string[] args)
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        LogService.Logger = new LoggerConfiguration().CreateLogger();

        string repoRoot = args.Length > 0 ? args[0] : Directory.GetCurrentDirectory();
        string sqlPath = Path.Combine(repoRoot, "sql", "mir2_data.sql");
        string vecPath = args.Length > 1
            ? args[1]
            : Path.Combine(repoRoot, "mir2-rs", "tests", "parity", "m3", "skill-probe", "vectors", "skill_vectors.json");
        string regPath = args.Length > 2
            ? args[2]
            : Path.Combine(repoRoot, "mir2-rs", "tests", "parity", "m3", "skill-probe", "vectors", "skills_registry.json");

        _getSpellPoint = typeof(PlayObject).GetMethod("GetSpellPoint",
            BindingFlags.Static | BindingFlags.NonPublic)
            ?? throw new MissingMethodException("PlayObject.GetSpellPoint");

        var allRows = ParseMagics(sqlPath);

        var tsv = new List<string> { string.Join("\t", "seed", RngSeed, SystemShare.Config.MagicHitIntervalTime) };
        var cases = new List<object>();
        var registry = new List<object>();
        foreach (MagicRow row in allRows)
        {
            bool implemented = IsImplemented(row.MagId);
            string reason = implemented
                ? "本片已实装：施法前置 + MP 消耗 + 威力公式 + 冷却（见 crates/world/src/magic.rs）"
                : "未实装：需要地图/目标/护身符/召唤/状态等 oracle 上下文，承接 = 后续 M3 技能批次（本卡报告『未实现』清单）";
            registry.Add(new
            {
                magic_id = (int)row.MagId,
                name = row.Name,
                job = (int)row.Job,
                train_level0 = (int)row.NeedL1,
                delay_time = row.Delay,
                implemented,
                reason,
            });
            tsv.Add(string.Join("\t", "reg", row.MagId, row.Name, row.Job, row.NeedL1, row.Delay, implemented ? 1 : 0, reason));
            if (!implemented)
            {
                continue;
            }

            var magic = ToMagicInfo(row);
            var caster = NewCaster(row.Job == 99 ? 1 : row.Job);
            var um1 = ToUserMagic(magic, 1);

            var mpCost = new Dictionary<string, int>();
            for (byte lv = 0; lv <= 3; lv++)
            {
                var um = ToUserMagic(magic, lv);
                mpCost[lv.ToString(CultureInfo.InvariantCulture)] =
                    (ushort)_getSpellPoint.Invoke(null, new object[] { um });
            }

            bool jobOk = magic.Job == 99 || magic.Job == (byte)caster.Job;
            bool levelOk = caster.Abil.Level >= magic.TrainLevel[0];
            bool isTraining = caster.IsTrainingSkill(magic.MagicId);
            bool canReadBook = !isTraining && jobOk && levelOk;

            EffectVal effect = ComputeEffect(caster, um1);

            tsv.Add(string.Join("\t", "skill",
                magic.MagicId, magic.MagicName, magic.Job, magic.TrainLv,
                magic.TrainLevel[0], magic.TrainLevel[1], magic.TrainLevel[2], magic.TrainLevel[3],
                magic.MaxTrain[0], magic.MaxTrain[1], magic.MaxTrain[2], magic.MaxTrain[3],
                magic.Spell, magic.DefSpell, magic.Power, magic.MaxPower, magic.DefPower, magic.DefMaxPower,
                magic.DelayTime, magic.DelayTime + SystemShare.Config.MagicHitIntervalTime,
                mpCost["0"], mpCost["1"], mpCost["2"], mpCost["3"],
                (int)caster.Job, (int)caster.Abil.Level,
                jobOk ? 1 : 0, levelOk ? 1 : 0, isTraining ? 1 : 0, canReadBook ? 1 : 0,
                (int)caster.WAbil.MC, (int)caster.WAbil.SC, effect.Tsv));

            cases.Add(new
            {
                magic_id = (int)magic.MagicId,
                name = magic.MagicName,
                job = (int)magic.Job,
                train_lv = (int)magic.TrainLv,
                train_level = magic.TrainLevel.Select(x => (int)x).ToArray(),
                max_train = magic.MaxTrain,
                spell = (int)magic.Spell,
                def_spell = (int)magic.DefSpell,
                power = (int)magic.Power,
                max_power = (int)magic.MaxPower,
                def_power = (int)magic.DefPower,
                def_max_power = (int)magic.DefMaxPower,
                delay_time = magic.DelayTime,
                cooldown = new
                {
                    delay_time = magic.DelayTime,
                    magic_hit_interval_time = SystemShare.Config.MagicHitIntervalTime,
                    interval = magic.DelayTime + SystemShare.Config.MagicHitIntervalTime,
                },
                mp_cost = mpCost,
                prereq = new
                {
                    probe_job = (int)caster.Job,
                    probe_level = (int)caster.Abil.Level,
                    job_ok = jobOk,
                    level_ok = levelOk,
                    is_training_skill = isTraining,
                    can_read_book = canReadBook,
                },
                caster_abil = new
                {
                    mc = (int)caster.WAbil.MC,
                    sc = (int)caster.WAbil.SC,
                },
                effect = effect.Json,
            });
        }

        var payload = new
        {
            generated_by = "mir2-rs/tests/parity/m3/skill-probe",
            oracle = new
            {
                input = "sql/mir2_data.sql 的 magics 行（口径 = MySqlDB.LoadMagicDB）",
                formulas = "M2Server.Magic.MagicBase.{MPow,GetPower,GetPower13,GetRPow} + PlayObject.GetAttackPower",
                mp_cost_entry = "M2Server.Player.PlayObject.GetSpellPoint (protected static, 反射调用)",
                prereq_entry = "M2Server.Player.PlayObject.IsTrainingSkill + ReadBook 口径（PlayObject.Base.cs:2170-2196）",
                rng = "OpenMir2.RandomNumber.random 反射覆盖为 new Random(seed ^ magic_id)",
            },
            seed = RngSeed,
            magic_hit_interval_time = SystemShare.Config.MagicHitIntervalTime,
            cases,
        };

        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(vecPath)));
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(regPath)));
        var opts = new JsonSerializerOptions { WriteIndented = true };
        File.WriteAllText(vecPath, JsonSerializer.Serialize(payload, opts) + Environment.NewLine, new UTF8Encoding(false));
        File.WriteAllText(regPath, JsonSerializer.Serialize(new
        {
            generated_by = "mir2-rs/tests/parity/m3/skill-probe",
            total_magic_rows = allRows.Count,
            implemented = cases.Count,
            skills = registry,
        }, opts) + Environment.NewLine, new UTF8Encoding(false));
        // TSV 伴生件（Rust 用例用）：行式、零依赖（`crates/world` 守卫禁 serde_json）。
        File.WriteAllText(Path.ChangeExtension(vecPath, ".tsv"), string.Join("\n", tsv) + "\n", new UTF8Encoding(false));
        Console.WriteLine($"skill-probe: wrote {vecPath} ({cases.Count} implemented) / {regPath} ({allRows.Count} rows)");
        return 0;
    }
}
