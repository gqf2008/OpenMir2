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
// 第五片（本轮）：
//   * 39（`SKILL_GROUPDEDING`→`MagGroupDeDing`，数据行名「乌龟跑步」）的**逐目标群体物理伤害**：
//     `GetAttackPower(Lo(DC), Hi(DC)-Lo(DC))` + 闪避门 `Random(target.SpeedPoint) >= HitPoint`
//     + `target.GetHitStruckDamage`（真身子函数，同种子；目标为真 `MonsterObject`）。
//   * `DoSpell` **全表分派分类**（`dospell_class`）：按 oracle 控制流把每一行归为
//     warr_reject / magicid_reject / implemented / context_required / no_effect_case / no_switch_case，
//     写进 registry JSON 与 `reg` TSV 行（Rust 侧 `dospell_classes_are_fully_registered` 逐类钉行数）。
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
using M2Server.Monster;
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
        // DC = 0x1E14（Lo=20 Hi=30），MC = 0x1E14（Lo=20 Hi=30），SC = 0x2832（Lo=50 Hi=40 → Δ 为负，覆盖 GetRPow 的 else 分支）
        po.WAbil = new Ability { DC = 0x1E14, MC = 0x1E14, SC = 0x2832, HP = 500, MP = 500, MaxHP = 500, MaxMP = 500 };
        // 准确（命中判定攻方一侧）：39（`MagGroupDeDing`）的闪避门 = `Random(target.SpeedPoint) >= playObject.HitPoint`。
        po.HitPoint = 20;
        return po;
    }

    /// <summary>
    /// 造一个探针目标怪：39（`MagGroupDeDing`）的逐目标结算需要真目标（`SpeedPoint`/`AC`/`LifeAttrib`）。
    /// 与 `combat-probe` 同形；构造本对象的 RNG 消耗发生在 `SeedRng` **之前**，不入取数账本。
    /// </summary>
    private static MonsterObject NewTarget(ushort ac, ushort hp, byte hitPoint, byte speedPoint, byte lifeAttrib)
    {
        var mo = new MonsterObject();
        mo.Race = ActorRace.Monster;
        mo.ChrName = "skill_probe_target";
        mo.ActorId = 910001;
        mo.HitPoint = hitPoint;
        mo.SpeedPoint = speedPoint;
        mo.LifeAttrib = lifeAttrib;
        mo.WAbil = new Ability { AC = ac, HP = hp, MaxHP = hp };
        mo.AddAbil = new AddAbility();
        return mo;
    }

    /// <summary>每个已实装技能的"效果威力"表达式类别（与 `crates/world/src/magic.rs` 一一对应）。</summary>
    private static string EffectKind(ushort magIdx) => magIdx switch
    {
        1 or 5 or 9 or 10 or 23 or 24 or 33 or 44 or 47 or 50 => "get_power_plus_mc",
        // 火球术/大火球/地狱火/疾光电影/爆裂火焰/地狱雷光/冰咆哮/地狱烈焰/冰焰
        // +44（`SKILL_44` → `MagHbFireBall`，逐目标同式）：
        // GetAttackPower(GetPower(MPow)+Lo(MC), ΔMC+1)
        2 or 29 => "heal_get_power_plus_sc2",     // 治愈术/群体治疗术：GetAttackPower(GetPower(MPow)+Lo(SC)*2, ΔSC*2+1)
        13 => "get_power_plus_sc",                 // 灵魂火符：GetAttackPower(GetPower(MPow)+Lo(SC), ΔSC+1)
        11 or 42 or 45 => "lightening_get_power_plus_mc", // 雷电术/灭天火/42（`SKILL_GROUPLIGHTENING`→`MagGroupLightening`）：同上 + 不死 ×1.5
        22 => "fire_wall",                        // 火墙：power + time=GetPower(10)+GetRPow(MC)>>1
        6 or 48 => "amyounsul_power13_rpow",     // 施毒术/48（`SKILL_GROUPAMYOUNSUL`→`MagGroupAmyounsul`，逐目标同式）：GetPower13(40|30)+GetRPow(SC)*2
        14 or 15 => "power13_60_sc10",           // 幽灵盾/神圣战甲术：GetAttackPower(GetPower13(60)+Lo(SC)*10, ΔSC+1)
        16 => "power13_40_rpow3",                // 困魔咒：GetPower13(40)+GetRPow(SC)*3
        18 or 19 => "power13_30_rpow3",          // 隐身术/集体隐身术：GetPower13(30)+GetRPow(SC)*3
        28 => "show_hp",                          // 心灵启示：interval=GetPower13(GetRPow(SC)*2+30)*1000
        31 => "shield",                           // 魔法盾：GetPower(GetRPow(MC)+15)
        36 => "uenhancer",                        // 无极真气分支(数据行=火焰冰)：Level+1+Random(Level) / GetPower13(60)+Lo(SC)*10
        39 => "group_melee",                      // 乌龟跑步/十方斩族（`SKILL_GROUPDEDING`→`MagGroupDeDing`）：逐目标 DC 物理伤害 + 闪避门 + 护甲减免
        _ => "none",
    };

    // ---- DoSpell 的**全表分派分类**（DoD③「未实装逐条登记，不许静默漏」的机器可判口径）----------
    //
    // oracle 控制流（`src/M2Server/Magic/MagicManager.cs::DoSpell`）：
    //   :151  if (IsWarrSkill(userMagic.MagIdx)) return false;                     → warr_reject
    //   :167  if (SoftVersionDateEx==0 && ClientTick==0 || MagicId > 80) return false; → magicid_reject
    //   :172  switch (userMagic.Magic.MagicId) { … }                               → 每条 case 的类别
    // 其中 case 体只需**地图/目标/可见集/召唤/状态**的归 context_required；case 体只有
    // `break` 或只置 `boTrain` 的归 no_effect_case；没有 case 的归 no_switch_case。
    private static readonly HashSet<ushort> WarrSkillIds = new()
    {
        3, 4, 7, 12, 25, 26, 27, 34, 38, // MagicConst.SKILL_ONESWORD..TWINBLADE（`MagicManager.cs:82-105`）
    };

    /// <summary>`DoSpell` 的 `switch` case 魔号全集（逐条抄 `MagicManager.cs:172-640`）。</summary>
    private static readonly HashSet<ushort> DospellSwitchCases = new()
    {
        1, 2, 5, 6, 8, 9, 10, 11, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
        28, 29, 30, 31, 32, 33, 35, 36, 37, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49,
        50, 51, 52, 53, 54,
    };

    /// <summary>case 体无副作用（`break` 或只置 `boTrain`）的魔号：43 `break`；49/51/52/53/54 只 `boTrain=true`。</summary>
    private static readonly HashSet<ushort> NoEffectCaseIds = new() { 43, 49, 51, 52, 53, 54 };

    private static string DospellClass(ushort magId)
    {
        if (WarrSkillIds.Contains(magId))
        {
            return "warr_reject";
        }
        if (magId > 80)
        {
            return "magicid_reject";
        }
        if (IsImplemented(magId))
        {
            return "implemented";
        }
        if (DospellSwitchCases.Contains(magId))
        {
            return NoEffectCaseIds.Contains(magId) ? "no_effect_case" : "context_required";
        }
        return "no_switch_case";
    }

    /// <summary>每个分派类别的承接说明（DoD③「未实装逐条登记并给承接」）。</summary>
    private static string DospellClassReason(string dospellClass) => dospellClass switch
    {
        "warr_reject" => "DoSpell 前置：IsWarrSkill(MagIdx) ⇒ return false（MagicManager.cs:82-105,151）；承接 = 战士技能走 ClientHit/Attack 路径（M3 攻击接线）",
        "magicid_reject" => "DoSpell 前置：MagicId > 80 ⇒ return false（MagicManager.cs:167）；承接 = 需先确认 oracle 是否另有分派器（本表内无 case，DoSpell 不可达）",
        "no_effect_case" => "DoSpell case 体无副作用（43 `break`；49/51/52/53/54 只置 boTrain）；承接 = 无行为可对拍",
        "no_switch_case" => "DoSpell switch 无此魔号 case ⇒ 只走通用 RM_MAGICFIRE/train（MagicManager.cs:172-640 无分支）；承接 = 待确认是否有第二分派器",
        "context_required" => "DoSpell 有 case，但效果需要地图/目标/可见集/召唤/状态等上下文（如 MagPushArround/MagSaceMove/MagTurnUndead/MagMakeSlave/MagGroupDeDing 的遍历与施加）；承接 = M3 攻击/魔法接线后的批次",
        _ => "未分类：登记器缺陷",
    };

    private static bool IsImplemented(ushort magIdx) => EffectKind(magIdx) != "none";

    private sealed class EffectVal
    {
        public string Kind;
        public int Power = -1;
        public int VsUndead = -1;
        public int Green = -1;
        public int Red = -1;
        public int Time = -1;
        public int Interval = -1;
        public int MeleeDamage = -1;

        /// <summary>TSV 伴生件的 8 列（kind + 7 个数值，缺项为 -1）。</summary>
        public string Tsv => string.Join("\t", Kind, Power, VsUndead, Green, Red, Time, Interval, MeleeDamage);

        public object Json => new
        {
            kind = Kind,
            power = Power,
            vs_undead = VsUndead,
            power_green = Green,
            power_red = Red,
            time = Time,
            interval = Interval,
            melee_damage = MeleeDamage,
        };
    }

    private static EffectVal ComputeEffect(PlayObject caster, UserMagic um, MonsterObject target)
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
            case "fire_wall":
            {
                // MagicManager.cs:412-413（`SKILL_EARTHFIRE` case）—— 实参从左到右：power（MPow→GetPower→GetAttackPower）→ GetPower(10) → GetRPow(MC)
                int power = caster.GetAttackPower(
                    MagicBase.GetPower(MagicBase.MPow(um), um) + HUtil32.LoByte(caster.WAbil.MC),
                    HUtil32.HiByte(caster.WAbil.MC) - HUtil32.LoByte(caster.WAbil.MC) + 1);
                int time = MagicBase.GetPower(10, um) + (MagicBase.GetRPow(caster.WAbil.MC) >> 1);
                return new EffectVal { Kind = "fire_wall", Power = power, Time = time };
            }
            case "show_hp":
            {
                // MagicManager.cs:437（`SKILL_SHOWHP` case）—— GetRPow(SC) 先于 GetPower13
                int interval = MagicBase.GetPower13(MagicBase.GetRPow(caster.WAbil.SC) * 2 + 30, um) * 1000;
                return new EffectVal { Kind = "show_hp", Interval = interval };
            }
            case "shield":
            {
                // MagicManager.cs:475（`SKILL_SHIELD` case）—— GetRPow(MC) 先于 GetPower
                int power = MagicBase.GetPower(MagicBase.GetRPow(caster.WAbil.MC) + 15, um);
                return new EffectVal { Kind = "shield", Power = power };
            }
            case "uenhancer":
            {
                // MagicManager.cs:625-626（`SKILL_UENHANCER` case）—— Random(Level) 先于 time；time 式同 power13_60_sc10
                int power = (ushort)(um.Level + 1 + M2Share.RandomNumber.Random(um.Level));
                int time = caster.GetAttackPower(
                    MagicBase.GetPower13(60, um) + HUtil32.LoByte(caster.WAbil.SC) * 10,
                    HUtil32.HiByte(caster.WAbil.SC) - HUtil32.LoByte(caster.WAbil.SC) + 1);
                return new EffectVal { Kind = "uenhancer", Power = power, Time = time };
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
            case "group_melee":
            {
                // MagicManager.cs:960-968（`MagGroupDeDing` 的循环体，取数顺序与 oracle 逐字一致）：
                //   int nPower = playObject.GetAttackPower(LoByte(DC), HiByte(DC) - LoByte(DC));
                //   if (Random(baseObject.SpeedPoint) >= playObject.HitPoint) nPower = 0;
                //   if (nPower > 0) nPower = baseObject.GetHitStruckDamage(playObject, nPower);
                // 注意：oracle 这里**无条件**抽一次 `Random(SpeedPoint)`（先算 power），Rust 侧同序。
                int power = caster.GetAttackPower(
                    HUtil32.LoByte(caster.WAbil.DC),
                    HUtil32.HiByte(caster.WAbil.DC) - HUtil32.LoByte(caster.WAbil.DC));
                if (M2Share.RandomNumber.Random(target.SpeedPoint) >= caster.HitPoint)
                {
                    power = 0;
                }
                int damage = 0;
                if (power > 0)
                {
                    damage = target.GetHitStruckDamage(caster, power);
                }
                return new EffectVal { Kind = "group_melee", Power = power, MeleeDamage = damage };
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
        // DoSpell 的施法者是 PlayObject，其 GetAttackPower override 的取数次数取决于下列四项。
        // 向量把这四项一并交给 Rust 侧，保证“威力 + 其后的取数”整体对齐（否则火墙的 time 会错位）。
        var seedCaster = NewCaster(1);

        var tsv = new List<string>
        {
            string.Join("\t", "seed", RngSeed, SystemShare.Config.MagicHitIntervalTime,
                (int)seedCaster.Luck, seedCaster.PowerRate, seedCaster.BoPowerItem ? 1 : 0,
                (int)seedCaster.PowerItem)
        };
        var cases = new List<object>();
        var registry = new List<object>();
        foreach (MagicRow row in allRows)
        {
            bool implemented = IsImplemented(row.MagId);
            string dospellClass = DospellClass(row.MagId);
            string reason = implemented
                ? "本片已实装：施法前置 + MP 消耗 + 效果威力 + 冷却（见 crates/world/src/magic.rs）"
                : DospellClassReason(dospellClass);
            registry.Add(new
            {
                magic_id = (int)row.MagId,
                name = row.Name,
                job = (int)row.Job,
                train_level0 = (int)row.NeedL1,
                delay_time = row.Delay,
                implemented,
                dospell_class = dospellClass,
                reason,
            });
            tsv.Add(string.Join("\t", "reg", row.MagId, row.Name, row.Job, row.NeedL1, row.Delay, implemented ? 1 : 0, reason, dospellClass));
            if (!implemented)
            {
                continue;
            }

            var magic = ToMagicInfo(row);
            var caster = NewCaster(row.Job == 99 ? 1 : row.Job);
            var um1 = ToUserMagic(magic, 1);
            // 39 的逐目标结算需要真目标；构造在 SeedRng 之前（构造消耗不入取数账本）。
            MonsterObject effectTarget = EffectKind(magic.MagicId) == "group_melee"
                ? NewTarget(0x0A05, 60000, 20, 40, 0)
                : null;

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

            EffectVal effect = ComputeEffect(caster, um1, effectTarget);

            tsv.Add(string.Join("\t", "skill",
                magic.MagicId, magic.MagicName, magic.Job, magic.TrainLv,
                magic.TrainLevel[0], magic.TrainLevel[1], magic.TrainLevel[2], magic.TrainLevel[3],
                magic.MaxTrain[0], magic.MaxTrain[1], magic.MaxTrain[2], magic.MaxTrain[3],
                magic.Spell, magic.DefSpell, magic.Power, magic.MaxPower, magic.DefPower, magic.DefMaxPower,
                magic.DelayTime, magic.DelayTime + SystemShare.Config.MagicHitIntervalTime,
                mpCost["0"], mpCost["1"], mpCost["2"], mpCost["3"],
                (int)caster.Job, (int)caster.Abil.Level,
                jobOk ? 1 : 0, levelOk ? 1 : 0, isTraining ? 1 : 0, canReadBook ? 1 : 0,
                (int)caster.WAbil.MC, (int)caster.WAbil.SC, effect.Tsv,
                (int)caster.WAbil.DC, (int)caster.HitPoint,
                effectTarget == null ? 0 : (int)effectTarget.SpeedPoint,
                effectTarget == null ? 0 : (int)effectTarget.WAbil.AC,
                effectTarget == null ? 0 : (int)effectTarget.LifeAttrib));

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
                    dc = (int)caster.WAbil.DC,
                    mc = (int)caster.WAbil.MC,
                    sc = (int)caster.WAbil.SC,
                    hit_point = (int)caster.HitPoint,
                },
                effect_target = effectTarget == null
                    ? null
                    : new
                    {
                        speed_point = (int)effectTarget.SpeedPoint,
                        ac = (int)effectTarget.WAbil.AC,
                        life_attrib = (int)effectTarget.LifeAttrib,
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
                attack_power = "PlayObject.GetAttackPower override（PlayObject.cs:2909）；向量附 luck/power_rate/bo_power_item/power_item",
                dospell_class = "DoSpell 全表分派分类：warr_reject(:151) / magicid_reject(:167) / implemented / no_effect_case / context_required / no_switch_case（switch 见 :172-640）",
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
