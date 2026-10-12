// proper-target-probe —— 用 C# oracle 真身实测「攻击合法性（`IsProperTarget`）」与 PvP 攻击序列向量。
//
// 为什么要有这个探针：
//   1. F-K：`crates/world/src/combat.rs` 旧实现只放行「人打怪/怪打人」，人打人恒 false；
//      oracle 的门是**虚函数** `IsProperTarget`，玩家攻方按 `AttatckMode` 分派（默认 HAM_ALL
//      ⇒ 允许 PvP），再经安全区 `IsProtectTarget`、主人-召唤者分支。靠读源码重推易错，
//      本探针把真身判定链逐个边界调出来产向量。
//   2. R-1：跨侧逐 k 比对需要「除攻击路径外零 RNG 消费者」的场景。本探针直接构造两个
//      `PlayObject`（不经地图、无怪 AI、无 socket），用同一注入种子跑 N 次 `_Attack`，
//      产出**同侧可复现、跨侧可比**的逐 k (power/damage/hp) 序列。
//
// 做法（照 `combat-probe` / `ground-probe` 形态）：
//   * `new PlayObject()`（攻方/守方）+ `new MonsterObject()`（基版路径对照）；
//   * 直调 `PlayObject.IsProperTarget`（public override）/ `BaseObject.IsProperTarget`；
//   * 反射调 internal `BaseObject.GetBaseAttackPoewr` / `BaseObject._Attack`；
//   * 种子注入：反射覆盖 `OpenMir2.RandomNumber.random`（私有静态 `System.Random`）——
//     与 D2 的 `RngSeedHook` 同一注入点。
//
// 产物 = mir2-rs/tests/parity/m3/proper-target-probe/vectors/proper_target_vectors.{json,tsv}
// Rust 侧用例 `crates/world/tests/combat_skill_vectors.rs` 逐字段核对。
//
// 用法（**在仓库根运行**）：
//   dotnet run --project mir2-rs/tests/parity/m3/proper-target-probe -c Release -- <输出路径>
//
// 只读：不连 MySQL、不写任何现网文件、不发起任何 socket。

using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using M2Server;
using M2Server.Actor;
using M2Server.Maps;
using M2Server.Monster;
using M2Server.Net;
using M2Server.Player;
using OpenMir2;
using OpenMir2.Data;
using OpenMir2.Enums;
using OpenMir2.Packets.ClientPackets;
using OpenMir2.Packets.ServerPackets;
using Serilog;
using SystemModule;
using SystemModule.Actors;

namespace ProperTargetProbe;

/// <summary>截获 `AddGateBuffer` 的替身（本探针不消费帧，只为满足 `M2Share.NetChannel` 非空）。</summary>
internal sealed class NullNetChannel : INetChannel
{
    public void AddGateBuffer(int gateIdx, byte[] senData) { }
    public void AddGameGateQueue(int gateIdx, ServerMessage packet, byte[] data) { }
    public void CloseAllGate() { }
    public void CloseUser(int gateIdx, int nSocket) { }
    public void Initialize() { }
    public void KickUser(string account, int sessionId, int payMode) { }
    public void Run() { }
    public void SendOutConnectMsg(int gateIdx, int nSocket, ushort nGsIdx) { }
    public void SendServerStopMsg() { }
    public void SetGateUserList(int gateIdx, int nSocket, IPlayerActor playObject) { }
    public Task Start(CancellationToken cancellationToken = default) => Task.CompletedTask;
    public void Send(string connectId, byte[] buff) { }
    public void CloseGate(string connectionId, string endPoint) { }
    public Task StopAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;
}

internal static class Program
{
    private const int RngSeed = 20261012;
    private static MethodInfo _baseAttackPower;
    private static MethodInfo _attack;
    private static readonly List<string> Tsv = new();

    /// <summary>把 oracle 的随机源换成固定种子（与 D2 `RngSeedHook` 同一注入点）。</summary>
    private static void SeedRng(int seed)
    {
        RandomNumber.GetInstance();
        FieldInfo field = typeof(RandomNumber).GetField("random", BindingFlags.NonPublic | BindingFlags.Static)
            ?? throw new InvalidOperationException("找不到 OpenMir2.RandomNumber.random 私有静态字段");
        field.SetValue(null, new Random(seed));
    }

    /// <summary>非安全区的 `Envir`（`InSafeZone()` 读 `Envir.Flag.SafeArea`）。</summary>
    private static Envirnoment NonSafeEnvir() =>
        new Envirnoment { MapName = "probe", MapFileName = "probe", Flag = new MapInfoFlag { SafeArea = false } };

    private static PlayObject NewPlayer(int actorId, string name, byte race, int dc, byte hitPoint, byte speedPoint, ushort ac, bool safeZone)
    {
        var po = new PlayObject
        {
            Race = race,
            ActorId = actorId,
            ChrName = name,
            Job = PlayerJob.Warrior,
            Abil = new Ability { Level = 1, HP = 100, MP = 100, MaxHP = 100, MaxMP = 100 },
            AddAbil = new AddAbility(),
            // `Envir == null` ⇒ `InSafeZone() = true`（安全区）；要「非安全」必须给一个 SafeArea=false 的 Envir。
            Envir = safeZone ? null : NonSafeEnvir(),
        };
        po.HitPoint = hitPoint;
        po.SpeedPoint = speedPoint;
        po.WAbil = new Ability { DC = (ushort)dc, AC = ac, HP = po.Abil.HP, MP = 100, MaxHP = po.Abil.MaxHP, MaxMP = 100 };
        return po;
    }

    private sealed class ProperCase
    {
        public string Name;
        public bool AttackerIsPlay = true;
        public byte AttackerRace = ActorRace.Play;
        public byte TargetRace = ActorRace.Play;
        public byte AttackMode = 0; // HAM_ALL
        public bool AttackerSafe;
        public bool TargetSafe;
        public bool TargetAdmin;
        public bool TargetStone;
        public bool TargetHasMaster;
        public bool MasterIsAttacker;
        public bool DearIsTarget;
        public int AttackerPk;
        public int TargetPk;
        public bool AttackerNasty;
    }

    private static object RunProperCase(ProperCase c)
    {
        // 攻方：玩家（PlayObject.IsProperTarget）或怪（BaseObject.IsProperTarget）。
        BaseObject attacker;
        if (c.AttackerIsPlay)
        {
            var po = NewPlayer(101, "attacker", c.AttackerRace, 0x1E14, 20, 12, 0, c.AttackerSafe);
            po.AttatckMode = (AttackMode)c.AttackMode;
            po.PkPoint = c.AttackerPk;
            po.NastyMode = c.AttackerNasty;
            attacker = po;
        }
        else
        {
            var mo = new MonsterObject { Race = c.AttackerRace, ActorId = 101, ChrName = "probe_mon" };
            mo.NastyMode = c.AttackerNasty;
            mo.WAbil = new Ability();
            attacker = mo;
        }

        // 守方。
        var target = NewPlayer(202, "target", c.TargetRace, 0, 20, 12, 0, c.TargetSafe);
        target.AdminMode = c.TargetAdmin;
        target.StoneMode = c.TargetStone;
        target.PkPoint = c.TargetPk;
        if (c.TargetHasMaster)
        {
            target.Master = c.MasterIsAttacker ? attacker : NewPlayer(303, "othermaster", ActorRace.Play, 0, 20, 12, 0, false);
        }
        if (c.DearIsTarget && attacker is PlayObject patk)
        {
            patk.DearHuman = target;
        }

        bool expected = attacker.IsProperTarget(target);
        Tsv.Add(string.Join("\t", "pt", c.Name,
            c.AttackerIsPlay ? 1 : 0,
            c.AttackerRace, c.TargetRace, c.AttackMode,
            c.AttackerSafe ? 1 : 0, c.TargetSafe ? 1 : 0,
            c.TargetAdmin ? 1 : 0, c.TargetStone ? 1 : 0,
            c.TargetHasMaster ? 1 : 0, c.MasterIsAttacker ? 1 : 0, c.DearIsTarget ? 1 : 0,
            c.AttackerPk, c.TargetPk, c.AttackerNasty ? 1 : 0,
            expected ? 1 : 0));
        return new
        {
            name = c.Name,
            attacker_is_play = c.AttackerIsPlay,
            attacker_race = (int)c.AttackerRace,
            target_race = (int)c.TargetRace,
            attack_mode = (int)c.AttackMode,
            attacker_safe = c.AttackerSafe,
            target_safe = c.TargetSafe,
            target_admin = c.TargetAdmin,
            target_stone = c.TargetStone,
            target_has_master = c.TargetHasMaster,
            master_is_attacker = c.MasterIsAttacker,
            dear_is_target = c.DearIsTarget,
            attacker_pk = c.AttackerPk,
            target_pk = c.TargetPk,
            expected,
        };
    }

    /// <summary>R-1 零 RNG 消费者场景：两个真 `PlayObject`（非安全区）同种子跑 N 次 `_Attack`。</summary>
    private static object RunPvpSequence(string name, int attackerSkillCount, int attacks)
    {
        var attacker = NewPlayer(101, "attacker", ActorRace.Play, 0x1E14, 20, 12, 0, false);
        var target = NewPlayer(202, "victim", ActorRace.Play, 0, 20, 20, 0x0A05, false);
        // 只改 HP/MaxHP，保留 `WAbil.AC`（不要整体替换 `WAbil`，否则护甲归 0）。
        target.WAbil.HP = 30000;
        target.WAbil.MaxHP = 30000;
        // 构造之后再注入种子：`PlayObject` 构造函数本身消耗随机数（见 combat-probe 注释）。
        SeedRng(RngSeed);

        var list = new List<object>();
        int hits = 0;
        int dodges = 0;
        int attackedCount = 0;
        long damageSum = 0;
        for (int i = 0; i < attacks; i++)
        {
            int power = (int)_baseAttackPower.Invoke(attacker, null);
            ushort hpBefore = target.WAbil.HP;
            bool ok = (bool)_attack.Invoke(attacker, new object[] { power, target });
            ushort hpAfter = target.WAbil.HP;
            int damage = hpBefore - hpAfter;
            if (ok)
            {
                attackedCount++;
            }
            if (damage > 0)
            {
                hits++;
            }
            else
            {
                dodges++;
            }
            damageSum += damage;
            list.Add(new { i, power, damage, hp_after = hpAfter, hit = damage > 0 });
            Tsv.Add(string.Join("\t", "pvp", name, i, power, damage, hpAfter, damage > 0 ? 1 : 0));
        }
        Tsv.Add(string.Join("\t", "pvpstats", name, hits, dodges, attackedCount, damageSum));
        return new
        {
            name,
            attacker = new { race = (int)attacker.Race, dc = 0x1E14, hit_point = 20, skill_count = attackerSkillCount },
            target = new { race = (int)target.Race, ac = 0x0A05, hp = 30000, hit_point = 20, speed_point = 20 },
            seed = RngSeed,
            attacks,
            sequence = list,
            stats = new { hits, dodges, attacked = attackedCount, damage_sum = damageSum },
        };
    }

    public static int Main(string[] args)
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        LogService.Logger = new LoggerConfiguration().CreateLogger();
        M2Share.NetChannel = new NullNetChannel();

        string outPath = args.Length > 0
            ? args[0]
            : Path.Combine(AppContext.BaseDirectory, "..", "..", "..", "..", "..",
                "proper-target-probe", "vectors", "proper_target_vectors.json");

        _baseAttackPower = typeof(BaseObject).GetMethod("GetBaseAttackPoewr",
            BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingMethodException("BaseObject.GetBaseAttackPoewr");
        _attack = typeof(BaseObject).GetMethod("_Attack",
            BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingMethodException("BaseObject._Attack");

        Tsv.Add(string.Join("\t", "seed", RngSeed));

        var properCases = new List<ProperCase>
        {
            // F-K 核心：HAM_ALL 允许 PvP；安全区任一在 ⇒ 拒。
            new ProperCase { Name = "pvp_ham_all_nosafe" },
            new ProperCase { Name = "pvp_ham_all_atk_safe", AttackerSafe = true },
            new ProperCase { Name = "pvp_ham_all_tgt_safe", TargetSafe = true },
            // 攻击模式。
            new ProperCase { Name = "pvp_ham_peace", AttackMode = (byte)AttackMode.HAM_PEACE },
            new ProperCase { Name = "pvp_ham_dear_stranger", AttackMode = (byte)AttackMode.HAM_DEAR },
            new ProperCase { Name = "pvp_ham_dear_spouse", AttackMode = (byte)AttackMode.HAM_DEAR, DearIsTarget = true },
            new ProperCase { Name = "pvp_ham_master", AttackMode = (byte)AttackMode.HAM_MASTER },
            new ProperCase { Name = "pvp_ham_group", AttackMode = (byte)AttackMode.HAM_GROUP },
            new ProperCase { Name = "pvp_ham_guild", AttackMode = (byte)AttackMode.HAM_GUILD },
            new ProperCase { Name = "pvp_ham_pk_white_white", AttackMode = (byte)AttackMode.HAM_PKATTACK },
            new ProperCase { Name = "pvp_ham_pk_white_red", AttackMode = (byte)AttackMode.HAM_PKATTACK, TargetPk = 250 },
            new ProperCase { Name = "pvp_ham_pk_red_white", AttackMode = (byte)AttackMode.HAM_PKATTACK, AttackerPk = 250 },
            new ProperCase { Name = "pvp_ham_pk_red_red", AttackMode = (byte)AttackMode.HAM_PKATTACK, AttackerPk = 250, TargetPk = 250 },
            // 种族 / 召唤物（自己宝宝 vs 对方宝宝）。
            new ProperCase { Name = "npc_ham_all", TargetRace = ActorRace.NPC },
            new ProperCase { Name = "animal_ham_all", TargetRace = ActorRace.AnimalChicken },
            new ProperCase { Name = "monster_ham_all", TargetRace = ActorRace.Monster },
            new ProperCase { Name = "monster_ham_peace", TargetRace = ActorRace.Monster, AttackMode = (byte)AttackMode.HAM_PEACE },
            new ProperCase { Name = "own_pet_ham_all", TargetRace = ActorRace.PlayClone, TargetHasMaster = true, MasterIsAttacker = true },
            new ProperCase { Name = "own_pet_ham_peace", TargetRace = ActorRace.PlayClone, AttackMode = (byte)AttackMode.HAM_PEACE, TargetHasMaster = true, MasterIsAttacker = true },
            new ProperCase { Name = "other_pet_ham_all", TargetRace = ActorRace.PlayClone, TargetHasMaster = true, MasterIsAttacker = false },
            // 基版（非玩家攻方）路径：管理员/石化不可被攻击。
            new ProperCase { Name = "mon_attacker_admin_target", AttackerIsPlay = false, AttackerRace = ActorRace.Monster, TargetAdmin = true },
            new ProperCase { Name = "mon_attacker_stone_target", AttackerIsPlay = false, AttackerRace = ActorRace.Monster, TargetStone = true },
            new ProperCase { Name = "mon_attacker_vs_player", AttackerIsPlay = false, AttackerRace = ActorRace.Monster },
            new ProperCase { Name = "mon_attacker_vs_npc", AttackerIsPlay = false, AttackerRace = ActorRace.Monster, TargetRace = ActorRace.NPC },
        };

        var properResults = new List<object>();
        foreach (var c in properCases)
        {
            properResults.Add(RunProperCase(c));
        }

        var pvp = RunPvpSequence("pvp_ham_all_100", 0, 100);

        var payload = new
        {
            generated_by = "mir2-rs/tests/parity/m3/proper-target-probe",
            oracle = new
            {
                entry = "M2Server.Player.PlayObject.IsProperTarget (public override) / M2Server.Actor.BaseObject.IsProperTarget",
                attack = "M2Server.Actor.BaseObject._Attack (internal, 反射调用) + GetBaseAttackPoewr",
                source = "src/M2Server/Player/PlayObject.cs:1929 + PlayObject.Attack.cs:627 + BaseObject.Base.cs:360,377,470 + BaseObject.cs:1777",
                rng = "OpenMir2.RandomNumber.random (私有静态 System.Random) 反射覆盖为 new Random(seed)",
            },
            seed = RngSeed,
            proper_target = properResults,
            pvp_sequence = pvp,
        };

        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outPath)));
        string json = JsonSerializer.Serialize(payload, new JsonSerializerOptions { WriteIndented = true });
        File.WriteAllText(outPath, json + Environment.NewLine, new UTF8Encoding(false));
        string tsvPath = Path.ChangeExtension(outPath, ".tsv");
        File.WriteAllText(tsvPath, string.Join("\n", Tsv) + "\n", new UTF8Encoding(false));
        Console.WriteLine($"proper-target-probe: wrote {Path.GetFullPath(outPath)} ({properCases.Count} proper cases + 1 pvp seq) + {Path.GetFileName(tsvPath)}");
        return 0;
    }
}
