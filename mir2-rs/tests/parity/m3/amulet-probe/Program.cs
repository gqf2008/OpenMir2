// amulet-probe —— 用 C# oracle 真身实测「施法前置 · 武器（护身符）/ 战士技能」向量。
//
// 本片（M3 ②③ 离线第二格）把 DoD③ 的「施法前置」里**可离线、可真身调用**的两面做成逐字段向量：
//   ① 战士技能拒绝：`MagicManager.IsWarrSkill(int wMagIdx)`（`src/M2Server/Magic/MagicManager.cs:82-105`）
//       —— `DoSpell` 开头 `if (IsWarrSkill(userMagic.MagIdx)) return false;`（同文件 :151）。
//   ② 护身符（武器）前置：`MagicBase.CheckAmulet(playObject, byte nCount, byte nType, ref short Idx)`
//      （`src/M2Server/Magic/MagicBase.cs:47-105`），用**真 PlayObject**（真 `UseItems`）+ 真
//      `GameItemSystem`（`GetStdItem` 查表）驱动 oracle 本体，不是公式重抄。
//
// 另输出 oracle 配置常量 `SystemShare.Config.MagicAttackRage` / `MagicHitIntervalTime`，
// 供 Rust 侧 DoSpell 顶层门（`MagicManager.cs:155` 的距离门）引用。
//
// 两处入口均**不抽随机数**（不涉 RNG 账本），因此本片无 RNG 注入。
//
// 用法（**在仓库根运行**）：
//   dotnet run --project mir2-rs/tests/parity/m3/amulet-probe -c Release -- <向量输出路径>
//
// 只读：不连 MySQL、不写任何现网文件、不发起任何 socket。

using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using System.Text.Json;
using M2Server;
using M2Server.Actor;
using M2Server.Items;
using M2Server.Magic;
using M2Server.Player;
using OpenMir2;
using OpenMir2.Data;
using OpenMir2.Enums;
using OpenMir2.Packets.ClientPackets;
using Serilog;
using SystemModule;

namespace AmuletProbe;

internal static class Program
{
    /// <summary>护身符场景（一行为一条 oracle 调用）。索引 -1 = 槽位为 null，0 = 空槽。</summary>
    private sealed class AmuletCase
    {
        public string Id;
        public int Count;
        public int Type;
        public int AIdx;
        public int AStd;
        public int AShape;
        public int ADura;
        public int BIdx;
        public int BStd;
        public int BShape;
        public int BDura;
    }

    private static PlayObject NewCaster()
    {
        return new PlayObject
        {
            Race = ActorRace.Play,
            Job = (PlayerJob)1,
            Abil = new Ability { Level = 50, HP = 500, MP = 500, MaxHP = 500, MaxMP = 500 },
            AddAbil = new AddAbility(),
            MagicList = new List<UserMagic>(),
            ItemList = new List<UserItem>(),
            UseItems = new UserItem[13],
            IsRobot = false,
        };
    }

    /// <summary>把 oracle 的全局物品表装成 `GetStdItem(3)=arm`、`GetStdItem(4)=bujuk`。</summary>
    private static void InstallItems(int aStd, int aShape, int bStd, int bShape)
    {
        var sys = new GameItemSystem();
        for (int k = 1; k <= 4; k++)
        {
            var it = new StdItem { Name = $"amulet{k}", StdMode = 0, Shape = 0 };
            if (k == 3)
            {
                it.StdMode = (byte)aStd;
                it.Shape = (byte)aShape;
            }
            if (k == 4)
            {
                it.StdMode = (byte)bStd;
                it.Shape = (byte)bShape;
            }
            sys.AddItem(it);
        }
        SystemShare.ItemSystem = sys;
    }

    private static int WarrSkillSweepMax = 110;

    public static int Main(string[] args)
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        LogService.Logger = new LoggerConfiguration().CreateLogger();

        string repoRoot = args.Length > 0 ? args[0] : Directory.GetCurrentDirectory();
        string vecPath = args.Length > 1
            ? args[1]
            : Path.Combine(repoRoot, "mir2-rs", "tests", "parity", "m3", "amulet-probe", "vectors", "amulet_vectors.json");

        // ---- ① 战士技能真身表（oracle: MagicManager.IsWarrSkill）-------------------------------
        var warr = new List<object>();
        var tsv = new List<string>
        {
            string.Join("\t", "meta",
                "magic_attack_rage", SystemShare.Config.MagicAttackRage,
                "magic_hit_interval_time", SystemShare.Config.MagicHitIntervalTime,
                "warr_skill_sweep_max", WarrSkillSweepMax),
        };
        var warrIdx = new List<int>();
        for (int magIdx = 0; magIdx <= WarrSkillSweepMax; magIdx++)
        {
            bool isWarr = MagicManager.IsWarrSkill(magIdx);
            warr.Add(new { mag_idx = magIdx, is_warr_skill = isWarr });
            tsv.Add(string.Join("\t", "warr", magIdx, isWarr ? 1 : 0));
            if (isWarr)
            {
                warrIdx.Add(magIdx);
            }
        }

        // ---- ② 护身符前置真身场景矩阵（oracle: MagicBase.CheckAmulet）-------------------------
        var scenarios = new List<AmuletCase>
        {
            // 空 / null 槽
            new() { Id = "none_empty", Count = 1, Type = 1, AIdx = 0, BIdx = 0 },
            new() { Id = "none_null", Count = 1, Type = 1, AIdx = -1, BIdx = -1 },
            // type1 命中 / 形状 / stdmode 门
            new() { Id = "t1_arm_ok", Count = 1, Type = 1, AIdx = 3, AStd = 25, AShape = 5, ADura = 100 },
            new() { Id = "t1_arm_dura_half_rounds_up", Count = 1, Type = 1, AIdx = 3, AStd = 25, AShape = 5, ADura = 50 },
            new() { Id = "t1_arm_dura_below", Count = 1, Type = 1, AIdx = 3, AStd = 25, AShape = 5, ADura = 49 },
            new() { Id = "t1_arm_shape_miss", Count = 1, Type = 1, AIdx = 3, AStd = 25, AShape = 4, ADura = 100 },
            new() { Id = "t1_arm_stdmode_miss", Count = 1, Type = 1, AIdx = 3, AStd = 24, AShape = 5, ADura = 100 },
            new() { Id = "t1_arm_stdmode_other", Count = 1, Type = 1, AIdx = 3, AStd = 0, AShape = 5, ADura = 100 },
            // type1 count 2 / count 5 的持久门
            new() { Id = "t1_arm_c2_dura149_miss", Count = 2, Type = 1, AIdx = 3, AStd = 25, AShape = 5, ADura = 149 },
            new() { Id = "t1_arm_c2_dura150_ok", Count = 2, Type = 1, AIdx = 3, AStd = 25, AShape = 5, ADura = 150 },
            new() { Id = "t1_arm_c5_dura449_miss", Count = 5, Type = 1, AIdx = 3, AStd = 25, AShape = 5, ADura = 449 },
            new() { Id = "t1_arm_c5_dura450_ok", Count = 5, Type = 1, AIdx = 3, AStd = 25, AShape = 5, ADura = 450 },
            new() { Id = "t1_arm_c5_dura500_ok", Count = 5, Type = 1, AIdx = 3, AStd = 25, AShape = 5, ADura = 500 },
            // bujuk 兜底 / 优先级
            new() { Id = "t1_bujuk_fallback", Count = 1, Type = 1, AIdx = 3, AStd = 25, AShape = 4, ADura = 100, BIdx = 4, BStd = 25, BShape = 5, BDura = 100 },
            new() { Id = "t1_arm_priority_over_bujuk", Count = 1, Type = 1, AIdx = 3, AStd = 25, AShape = 5, ADura = 100, BIdx = 4, BStd = 25, BShape = 5, BDura = 100 },
            new() { Id = "t1_arm_null_bujuk_ok", Count = 1, Type = 1, AIdx = -1, BIdx = 4, BStd = 25, BShape = 5, BDura = 100 },
            new() { Id = "t1_bujuk_stdmode_miss", Count = 1, Type = 1, AIdx = -1, BIdx = 4, BStd = 24, BShape = 5, BDura = 100 },
            new() { Id = "t1_bujuk_index_zero", Count = 1, Type = 1, AIdx = -1, BIdx = 0, BStd = 25, BShape = 5, BDura = 100 },
            // type2 形状门（Shape <= 2）
            new() { Id = "t2_arm_shape2_ok", Count = 1, Type = 2, AIdx = 3, AStd = 25, AShape = 2, ADura = 100 },
            new() { Id = "t2_arm_shape0_ok", Count = 1, Type = 2, AIdx = 3, AStd = 25, AShape = 0, ADura = 100 },
            new() { Id = "t2_arm_shape3_miss", Count = 1, Type = 2, AIdx = 3, AStd = 25, AShape = 3, ADura = 100 },
            new() { Id = "t2_bujuk_ok", Count = 1, Type = 2, AIdx = -1, BIdx = 4, BStd = 25, BShape = 1, BDura = 100 },
            // 形状跨类型不匹配 + 未知 nType
            new() { Id = "cross_type1_shape5_asked_type2_miss", Count = 1, Type = 2, AIdx = 3, AStd = 25, AShape = 5, ADura = 100 },
            new() { Id = "unknown_type3_miss", Count = 1, Type = 3, AIdx = 3, AStd = 25, AShape = 5, ADura = 100 },
        };

        var amulet = new List<object>();
        foreach (AmuletCase c in scenarios)
        {
            InstallItems(c.AStd, c.AShape, c.BStd, c.BShape);
            PlayObject po = NewCaster();
            po.UseItems[ItemLocation.ArmRingl] = c.AIdx < 0
                ? null
                : new UserItem { Index = (ushort)c.AIdx, Dura = (ushort)c.ADura };
            po.UseItems[ItemLocation.Bujuk] = c.BIdx < 0
                ? null
                : new UserItem { Index = (ushort)c.BIdx, Dura = (ushort)c.BDura };

            short idx = 0;
            bool ok = MagicBase.CheckAmulet(po, (byte)c.Count, (byte)c.Type, ref idx);
            if (!ok)
            {
                idx = -1;
            }

            amulet.Add(new
            {
                id = c.Id,
                count = c.Count,
                amulet_type = c.Type,
                armringl = new { index = c.AIdx, std_mode = c.AStd, shape = c.AShape, dura = c.ADura },
                bujuk = new { index = c.BIdx, std_mode = c.BStd, shape = c.BShape, dura = c.BDura },
                ok,
                result_idx = idx,
            });
            tsv.Add(string.Join("\t", "amulet",
                c.Id, c.Count, c.Type,
                c.AIdx, c.AStd, c.AShape, c.ADura,
                c.BIdx, c.BStd, c.BShape, c.BDura,
                ok ? 1 : 0, idx));
        }

        var payload = new
        {
            generated_by = "mir2-rs/tests/parity/m3/amulet-probe",
            oracle = new
            {
                warr_skill_entry = "M2Server.Magic.MagicManager.IsWarrSkill(int)（public static，MagicManager.cs:82-105）",
                check_amulet_entry = "M2Server.Magic.MagicBase.CheckAmulet(PlayObject, byte, byte, ref short)（public static，MagicBase.cs:47-105）",
                items = "真 GameItemSystem（GetStdItem 查表）+ 真 PlayObject.UseItems[ArmRingl=5 / Bujuk=9]",
                config = "SystemShare.Config.MagicAttackRage / MagicHitIntervalTime（oracle 配置默认值）",
                rng = "无（两处入口均不抽随机数）",
            },
            magic_attack_rage = (int)SystemShare.Config.MagicAttackRage,
            magic_hit_interval_time = SystemShare.Config.MagicHitIntervalTime,
            warr_skill_sweep_max = WarrSkillSweepMax,
            warr_skill_indices = warrIdx,
            warr_skill_sweep = warr,
            amulet_cases = amulet,
        };

        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(vecPath)));
        var opts = new JsonSerializerOptions { WriteIndented = true };
        File.WriteAllText(vecPath, JsonSerializer.Serialize(payload, opts) + Environment.NewLine, new UTF8Encoding(false));
        // TSV 伴生件（Rust 用例用）：行式、零依赖（`crates/world` 守卫禁 serde_json）。
        File.WriteAllText(Path.ChangeExtension(vecPath, ".tsv"), string.Join("\n", tsv) + "\n", new UTF8Encoding(false));
        Console.WriteLine($"amulet-probe: wrote {vecPath}（warr={warrIdx.Count} 个魔号 / amulet={scenarios.Count} 条）");
        return 0;
    }
}
