// combat-probe —— 用 C# oracle 真身实测「战斗结算（_Attack）」向量。
//
// 为什么要有这个探针：`crates/world/src/combat.rs` 的 `attack()` 必须与 oracle
// **同种子 / 同取数次数 / 同数值**。攻击力、命中骰、护甲减免三条 RNG 的顺序与次数，
// 以及 `StruckDamage` 的职业倍率、`DamageHealth` 的扣血分支，靠读源码重推极易抄错。
//
// 做法（照 `bag-probe` 形态）：
//   * `new PlayObject()`（攻方）+ `new MonsterObject()`（守方）—— 状态来自运行中的 oracle 代码；
//   * 反射调 internal 入口 `BaseObject.GetBaseAttackPoewr` / `BaseObject._Attack`；
//   * 种子注入：反射覆盖 `OpenMir2.RandomNumber.random`（私有静态 `System.Random`）——
//     与 D2 的 `RngSeedHook` 同一注入点，保证两侧同一取数序列。
//
// 产物 = `mir2-rs/tests/parity/m3/combat-probe/vectors/combat_vectors.json`，
// Rust 侧用例 `crates/world/tests/combat_skill_vectors.rs` 逐字段核对。
//
// 用法（**在仓库根运行**）：
//   dotnet run --project mir2-rs/tests/parity/m3/combat-probe -c Release -- <输出路径>
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

namespace CombatProbe;

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
    private static MethodInfo _baseAttackPower;
    private static MethodInfo _attack;

    /// <summary>把 oracle 的随机源换成固定种子（与 D2 `RngSeedHook` 同一注入点）。</summary>
    private static void SeedRng(int seed)
    {
        RandomNumber.GetInstance();
        FieldInfo field = typeof(RandomNumber).GetField("random", BindingFlags.NonPublic | BindingFlags.Static)
            ?? throw new InvalidOperationException("找不到 OpenMir2.RandomNumber.random 私有静态字段");
        field.SetValue(null, new Random(seed));
    }

    private static PlayObject NewAttacker(int dc, byte hitPoint, byte undeadPower)
    {
        var po = new PlayObject
        {
            Race = ActorRace.Play,
            Job = PlayerJob.Warrior,
            Abil = new Ability { Level = 10, HP = 100, MP = 100, MaxHP = 100, MaxMP = 100 },
            AddAbil = new AddAbility { UndeadPower = undeadPower },
            IsRobot = false,
        };
        po.HitPoint = hitPoint;
        po.WAbil = new Ability { DC = (ushort)dc, HP = 100, MP = 100, MaxHP = 100, MaxMP = 100 };
        return po;
    }

    private static MonsterObject NewTarget(ushort ac, ushort hp, byte hitPoint, byte speedPoint, byte lifeAttrib)
    {
        var mo = new MonsterObject();
        mo.Race = ActorRace.Monster;
        mo.ChrName = "probe_monster";
        mo.ActorId = 900001;
        mo.HitPoint = hitPoint;
        mo.SpeedPoint = speedPoint;
        mo.LifeAttrib = lifeAttrib;
        mo.WAbil = new Ability { AC = ac, HP = hp, MaxHP = hp };
        mo.AddAbil = new AddAbility();
        return mo;
    }

    private sealed class CaseDef
    {
        public string Name;
        public int Dc;
        public byte AttackerHitPoint;
        public byte AttackerUndeadPower;
        public ushort TargetAc;
        public ushort TargetHp;
        public byte TargetHitPoint;
        public byte TargetSpeedPoint;
        public byte TargetLifeAttrib;
        public int Attacks;
    }

    private static readonly List<string> Tsv = new();

    private static object RunCase(CaseDef def)
    {
        var attacker = NewAttacker(def.Dc, def.AttackerHitPoint, def.AttackerUndeadPower);
        var target = NewTarget(def.TargetAc, def.TargetHp, def.TargetHitPoint, def.TargetSpeedPoint, def.TargetLifeAttrib);
        // 种子注入必须在**对象构造之后**：`PlayObject`/`MonsterObject` 构造函数本身会消耗随机数
        // （实测 5+5 次），若先注入会把构造消耗算进攻击序列，Rust 侧无法复现。
        // 每个 case 前重新注入 ⇒ case 之间互不影响（Rust 侧用 `RandomNumber::with_seed` 同序重建）。
        SeedRng(RngSeed);

        Tsv.Add(string.Join("\t", "case", def.Name,
            def.Dc, def.AttackerHitPoint, def.AttackerUndeadPower, attacker.Luck, attacker.PowerRate,
            attacker.BoPowerItem ? 1 : 0, attacker.PowerItem,
            def.TargetAc, def.TargetHp, def.TargetHitPoint, def.TargetSpeedPoint, def.TargetLifeAttrib, def.Attacks));
        List<object> attacks = new();
        int hits = 0;
        int dodges = 0;
        int attacked = 0;
        long damageSum = 0;
        for (int i = 0; i < def.Attacks; i++)
        {
            int power = (int)_baseAttackPower.Invoke(attacker, null);
            ushort hpBefore = target.WAbil.HP;
            bool ok = (bool)_attack.Invoke(attacker, new object[] { power, target });
            ushort hpAfter = target.WAbil.HP;
            int damage = hpBefore - hpAfter;
            if (ok)
            {
                attacked++;
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
            attacks.Add(new
            {
                i,
                power,
                damage,
                hp_after = hpAfter,
                attacked = ok,
            });
            Tsv.Add(string.Join("\t", "atk", def.Name, i, power, damage, hpAfter, ok ? 1 : 0));
        }

        Tsv.Add(string.Join("\t", "stats", def.Name, hits, dodges, attacked, damageSum));

        return new
        {
            name = def.Name,
            attacker = new
            {
                race = (int)attacker.Race,
                job = (int)attacker.Job,
                dc = def.Dc,
                dc_lo = (int)((byte)def.Dc),
                dc_hi = (int)((byte)(def.Dc >> 8)),
                hit_point = (int)def.AttackerHitPoint,
                undead_power = (int)def.AttackerUndeadPower,
                // `PlayObject.GetAttackPower` override 的输入（默认：Luck=0、PowerRate=100）。
                luck = attacker.Luck,
                power_rate = attacker.PowerRate,
                bo_power_item = attacker.BoPowerItem,
                power_item = attacker.PowerItem,
            },
            target = new
            {
                race = (int)target.Race,
                ac = (int)def.TargetAc,
                ac_lo = (int)((byte)def.TargetAc),
                ac_hi = (int)((byte)(def.TargetAc >> 8)),
                hp = (int)def.TargetHp,
                hit_point = (int)def.TargetHitPoint,
                speed_point = (int)def.TargetSpeedPoint,
                life_attrib = (int)def.TargetLifeAttrib,
            },
            attacks,
            stats = new
            {
                n = def.Attacks,
                hits,
                dodges,
                attacked,
                damage_sum = damageSum,
            },
        };
    }

    private const int RngSeed = 20261012;

    public static int Main(string[] args)
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        LogService.Logger = new LoggerConfiguration().CreateLogger();
        M2Share.NetChannel = new NullNetChannel();

        string outPath = args.Length > 0
            ? args[0]
            : Path.Combine(AppContext.BaseDirectory, "..", "..", "..", "..", "..",
                "combat-probe", "vectors", "combat_vectors.json");

        _baseAttackPower = typeof(BaseObject).GetMethod("GetBaseAttackPoewr",
            BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingMethodException("BaseObject.GetBaseAttackPoewr");
        _attack = typeof(BaseObject).GetMethod("_Attack",
            BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingMethodException("BaseObject._Attack");

        Tsv.Add(string.Join("\t", "seed", RngSeed));
        var cases = new List<object>
        {
            RunCase(new CaseDef
            {
                Name = "dodge_mix_100",
                Dc = 0x1E14,           // Lo=20 Hi=30 ⇒ power ∈ [20,30]
                AttackerHitPoint = 20,
                AttackerUndeadPower = 0,
                TargetAc = 0x0A05,     // Lo=5 Hi=10 ⇒ 护甲两条 RNG 分支都走
                TargetHp = 60000,
                TargetHitPoint = 20,
                TargetSpeedPoint = 40, // 20/40 ⇒ 约 50% 闪避
                TargetLifeAttrib = 0,
                Attacks = 100,
            }),
            RunCase(new CaseDef
            {
                Name = "no_dodge_100",
                Dc = 0x1E14,
                AttackerHitPoint = 20,
                AttackerUndeadPower = 0,
                TargetAc = 5,          // Lo=5 Hi=0 ⇒ |Δ|+1 = 6
                TargetHp = 60000,
                TargetHitPoint = 20,
                TargetSpeedPoint = 1,  // RandomByte(1)=0 ⇒ 永不闪避
                TargetLifeAttrib = 0,
                Attacks = 100,
            }),
            RunCase(new CaseDef
            {
                Name = "undead_30",
                Dc = 0x1E14,
                AttackerHitPoint = 20,
                AttackerUndeadPower = 7, // 不死系加成
                TargetAc = 5,
                TargetHp = 60000,
                TargetHitPoint = 20,
                TargetSpeedPoint = 12,
                TargetLifeAttrib = 1,    // LA_UNDEAD
                Attacks = 30,
            }),
            RunCase(new CaseDef
            {
                Name = "stats_1000",
                Dc = 0x1E14,
                AttackerHitPoint = 20,
                AttackerUndeadPower = 0,
                TargetAc = 5,
                TargetHp = 60000,
                TargetHitPoint = 20,
                TargetSpeedPoint = 12,   // 20/12 ⇒ 必中（20 >= 12）
                TargetLifeAttrib = 0,
                Attacks = 1000,
            }),
        };

        var payload = new
        {
            generated_by = "mir2-rs/tests/parity/m3/combat-probe",
            oracle = new
            {
                entry = "M2Server.Actor.BaseObject._Attack (internal, 反射调用)",
                power_entry = "M2Server.Actor.BaseObject.GetBaseAttackPoewr (internal, 反射调用)",
                source = "src/M2Server/Actor/BaseObject.Attack.cs:25,30 + BaseObject.Base.cs:789,827 + BaseObject.cs:775",
                rng = "OpenMir2.RandomNumber.random (私有静态 System.Random) 反射覆盖为 new Random(seed)",
            },
            seed = RngSeed,
            config = new
            {
                warr_mon = SystemShare.Config.WarrMon,
                wizard_mon = SystemShare.Config.WizardMon,
                taos_mon = SystemShare.Config.TaosMon,
                mon_hum = SystemShare.Config.MonHum,
            },
            cases,
        };

        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outPath)));
        string json = JsonSerializer.Serialize(payload, new JsonSerializerOptions { WriteIndented = true });
        File.WriteAllText(outPath, json + Environment.NewLine, new UTF8Encoding(false));
        // TSV 伴生件（Rust 用例用）：行式、零依赖。
        string tsvPath = Path.ChangeExtension(outPath, ".tsv");
        File.WriteAllText(tsvPath, string.Join("\n", Tsv) + "\n", new UTF8Encoding(false));
        Console.WriteLine($"combat-probe: wrote {Path.GetFullPath(outPath)} ({cases.Count} cases) + {Path.GetFileName(tsvPath)}");
        return 0;
    }
}
