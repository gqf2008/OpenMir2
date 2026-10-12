// attack-dir-probe —— oracle 真身实测「玩家一次攻击（PlayObject.AttackDir）到底抽几次/什么次序」。
//
// owner 2026-10-12 19:20 裁决②③：Rust 攻击路径账本里 `attack_success` 站点**整段缺失**，
// 且「一次攻击该抽几次」此前没有真身向量。本探针直调真身
// `PlayObject.AttackDir(IActor targetObject, short wHitMode, byte nDir)`
// （`src/M2Server/Player/PlayObject.Attack.cs:25`，protected，反射调用），把每次 `System.Random`
// 调用记账成 `seq|kind|arg|result|site`（与 D2 `RngSeedHook` 同形），从而钉死：
//
//   `GetBaseAttackPoewr`（= `PlayObject.GetAttackPower` override，`Luck <= 0` 两抽）
//     → `_Attack`（命中门 `RandomByte(target.SpeedPoint)`；命中再 `GetHitStruckDamage`）
//     → `AttackSuccess`（`:225` `Random(5)`，**命中/躲闪都抽**）。
//
// `no_target_power_only` 一例复刻 `PlayObject.Attack.cs:73` 的**取目标之前**那一段
//（`AttackDir(null)` 会走 `GetPoseCreate()`，需要运行中的 `Envir`；本探针不构造世界，故只调
// `GetBaseAttackPoewr` 把这一段钉死；整体 `AttackDir(null)` 的行为由 C# 活栈 `rng-stream.tsv`
// 的 115/120 `[atk_power_a, atk_power_b]` 段兜底）。
//
// 产物 = mir2-rs/tests/parity/m3/attack-dir-probe/vectors/attack_dir_vectors.{json,tsv}
// Rust 侧用例 `crates/world/tests/attack_dir_vectors.rs` 用同一 seed 跑 `attack_dir` 逐击核对
// `(power, damage, hp_after, hit)`；draws 账本作为顺序/次数证据留档。
//
// 只读：不连 MySQL、不写任何现网文件、不发起任何 socket、不起栈、不占端口。

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
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
using SystemModule.SubSystem;

namespace AttackDirProbe;

/// <summary>SystemShare.ItemSystem 的最小替身（`StruckDamage` 破零分支会读 `NeedIdentify`）。</summary>
internal sealed class NullItemSystem : IItemSystem
{
    private static readonly StdItem Dummy = new StdItem { NeedIdentify = 0 };

    public void AddItem(StdItem stdItem) { }
    public void Clear() { }
    public int ItemCount => 0;
    public string GetStdItemName(int itemIdx) => string.Empty;
    public ushort GetStdItemIdx(string itemName) => 0;
    public bool CopyToUserItemFromName(string sItemName, ref UserItem item) => false;
    public StdItem GetStdItem(string sItemName) => Dummy;
    public StdItem GetStdItem(ushort nItemIdx) => Dummy;
    public int GetStdItemWeight(int nItemIdx) => 0;
    public int GetUpgrade2(int x, int a) => a;
    public int GetUpgradeStdItem(StdItem stdItem, UserItem userItem, ref ClientItem clientItem)
    {
        if (clientItem == null)
        {
            clientItem = new ClientItem();
        }
        var d = clientItem.Item;
        d.Name = string.Empty;
        d.StdMode = 0;
        return 0;
    }
    public void RandomSetUnknownItem(StdItem stdItem, UserItem pu) { }
    public void RandomUpgradeItem(StdItem stdItem, UserItem pu) { }
    public int RealAttackSpeed(short wAtkSpd) => wAtkSpd <= 10 ? -wAtkSpd : wAtkSpd - 10;
}

/// <summary>空 INetChannel（本探针不消费帧；只为满足 `M2Share.NetChannel` 非空）。</summary>
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
    private static FieldInfo _rngFieldCache;

    /// <summary>`System.Random` 的记录包装：`seq|kind|arg|result|site`（与 D2 钩子同形）。</summary>
    private sealed class RecordingRandom : Random
    {
        public readonly List<string> Draws = new();
        public RecordingRandom(int seed) : base(seed) { }

        private void Log(string kind, long arg, long value)
        {
            Draws.Add($"{Draws.Count + 1}|{kind}|{arg}|{value}|{Site()}");
        }

        public override int Next()
        {
            int v = base.Next();
            Log("Next()", 0, v);
            return v;
        }

        public override int Next(int maxValue)
        {
            int v = base.Next(maxValue);
            Log("Next(max)", maxValue, v);
            return v;
        }

        public override int Next(int minValue, int maxValue)
        {
            int v = base.Next(minValue, maxValue);
            Log("Next(min,max)", ((long)minValue << 32) | (uint)maxValue, v);
            return v;
        }
    }

    /// <summary>取第一个「非本探针、非 OpenMir2 包装层」的调用帧（游戏侧调用点）。</summary>
    private static string Site()
    {
        try
        {
            var st = new StackTrace(2, true);
            for (int i = 0; i < st.FrameCount; i++)
            {
                StackFrame f = st.GetFrame(i);
                MethodBase m = f?.GetMethod();
                if (m == null)
                {
                    continue;
                }
                string asm = m.DeclaringType?.Assembly?.GetName().Name ?? "";
                if (asm == "attack-dir-probe" || asm == "OpenMir2")
                {
                    continue;
                }
                string file = Path.GetFileName(f.GetFileName());
                return $"{m.DeclaringType?.FullName}.{m.Name}@{file}:{f.GetFileLineNumber()}";
            }
        }
        catch
        {
            // 调用点记录失败不影响数值对拍
        }
        return "?";
    }

    private static void SeedRng(int seed)
    {
        RandomNumber.GetInstance();
        FieldInfo field = typeof(RandomNumber).GetField("random", BindingFlags.NonPublic | BindingFlags.Static)
            ?? throw new InvalidOperationException("找不到 OpenMir2.RandomNumber.random 私有静态字段");
        field.SetValue(null, new RecordingRandom(seed));
        _rngFieldCache = field;
    }

    private static IReadOnlyList<string> Draws()
        => ((RecordingRandom)_rngFieldCache.GetValue(null)).Draws;

    private static PlayObject NewAttacker(int dc, byte hitPoint)
    {
        var po = new PlayObject
        {
            Race = ActorRace.Play,
            ActorId = 101,
            ChrName = "attacker",
            Job = PlayerJob.Warrior,
            IsRobot = true, // 跳过 AttackSuccess 的 MonDelHptoExp→GainExp 支（该支无 RNG，但避免副作用）
            Abil = new Ability { Level = 1, HP = 100, MP = 100, MaxHP = 100, MaxMP = 100 },
            AddAbil = new AddAbility(),
            Envir = null,
        };
        po.HitPoint = hitPoint;
        po.WAbil = new Ability { DC = (ushort)dc, HP = 100, MP = 100, MaxHP = 100, MaxMP = 100 };
        po.UseItems = new UserItem[13];
        return po;
    }

    private static MonsterObject NewTarget(ushort ac, ushort hp, byte hitPoint, byte speedPoint)
    {
        var mo = new MonsterObject
        {
            Race = ActorRace.Monster,
            ChrName = "monster",
            ActorId = 900001,
        };
        mo.HitPoint = hitPoint;
        mo.SpeedPoint = speedPoint;
        mo.WAbil = new Ability { AC = ac, HP = hp, MaxHP = hp };
        mo.AddAbil = new AddAbility();
        return mo;
    }

    private sealed class CaseDef
    {
        public string Name;
        public int Dc;
        public byte AttackerHitPoint;
        public ushort TargetAc;
        public byte TargetSpeedPoint;
        public int Attacks = 3;
        /// <summary>`true` = 只复刻 `AttackDir` 取目标之前的 `GetBaseAttackPoewr()` 段（无目标）。</summary>
        public bool PowerOnly;
    }

    private static readonly List<string> Tsv = new();

    private static object RunCase(CaseDef def, MethodInfo attackDir, MethodInfo baseAttackPower)
    {
        var attacker = NewAttacker(def.Dc, def.AttackerHitPoint);
        var target = NewTarget(def.TargetAc, 60000, 10, def.TargetSpeedPoint);
        SeedRng(RngSeed); // 构造之后注种子（与 combat-probe / WorldGolden 同口径）

        var attacks = new List<object>();
        var ledger = new List<string>();
        Tsv.Add(string.Join("\t", "case", def.Name, def.Dc, def.AttackerHitPoint, def.TargetAc,
            def.TargetSpeedPoint, def.PowerOnly ? 1 : 0, 0)); // 占位，最后回填 draws 数
        int caseRow = Tsv.Count - 1;
        for (int i = 0; i < def.Attacks; i++)
        {
            ushort hpBefore = target.WAbil.HP;
            int power;
            bool attacked;
            if (def.PowerOnly)
            {
                power = (int)baseAttackPower.Invoke(attacker, null);
                attacked = false;
            }
            else
            {
                // `AttackDir(targetObject, wHitMode=0, nDir=0)`：真身完整口径。
                // 注意 `PlayObject.AttackDir` 返回 void，观察不到内部 `_Attack` 的返回值 ⇒
                // `hit` 以「本击是否真的扣了 HP」定义（命中且护甲未减到 0）。
                power = 0;
                _ = attackDir.Invoke(attacker, new object[] { target, (short)0, (byte)0 });
                attacked = false;
            }
            ushort hpAfter = target.WAbil.HP;
            attacked = hpBefore - hpAfter > 0;
            attacks.Add(new
            {
                i,
                power,
                damage = hpBefore - hpAfter,
                hp_after = hpAfter,
                hit = attacked,
            });
            Tsv.Add(string.Join("\t", "atk", def.Name, i, power, hpBefore - hpAfter, hpAfter, attacked ? 1 : 0));
        }
        ledger.AddRange(Draws());
        foreach (string line in ledger)
        {
            Tsv.Add("draw\t" + def.Name + "\t" + line);
        }
        // 回填 case 行的 draw 数
        Tsv[caseRow] = string.Join("\t", "case", def.Name, def.Dc, def.AttackerHitPoint, def.TargetAc,
            def.TargetSpeedPoint, def.PowerOnly ? 1 : 0, ledger.Count);
        return new
        {
            name = def.Name,
            dc = def.Dc,
            attacker_hit_point = (int)def.AttackerHitPoint,
            target = new { ac = (int)def.TargetAc, speed_point = (int)def.TargetSpeedPoint, hit_point = 10, hp = 60000 },
            power_only = def.PowerOnly,
            attacks,
            draws = string.Join(";", ledger),
        };
    }

    private const int RngSeed = 20261012;

    public static int Main(string[] args)
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        LogService.Logger = new LoggerConfiguration().CreateLogger();
        M2Share.NetChannel = new NullNetChannel();
        SystemShare.ItemSystem = new NullItemSystem();

        string outPath = args.Length > 0
            ? args[0]
            : Path.Combine(AppContext.BaseDirectory, "..", "..", "..", "..", "..",
                "attack-dir-probe", "vectors", "attack_dir_vectors.json");

        MethodInfo attackDir = typeof(PlayObject).GetMethod("AttackDir",
            BindingFlags.Instance | BindingFlags.NonPublic,
            null, new[] { typeof(IActor), typeof(short), typeof(byte) }, null)
            ?? throw new MissingMethodException("PlayObject.AttackDir(IActor, short, byte)");
        MethodInfo baseAttackPower = typeof(BaseObject).GetMethod("GetBaseAttackPoewr",
            BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingMethodException("BaseObject.GetBaseAttackPoewr");

        var cases = new List<object>
        {
            RunCase(new CaseDef
            {
                Name = "no_target_power_only",
                Dc = 0x1E14,           // Lo=20 Hi=30 ⇒ power ∈ [20,30]
                AttackerHitPoint = 20,
                PowerOnly = true,
            }, attackDir, baseAttackPower),
            RunCase(new CaseDef
            {
                Name = "target_hit_ac0",
                Dc = 0x1E14,
                AttackerHitPoint = 255, // 255 >= RandomByte(1)==0 ⇒ 必中
                TargetAc = 0,           // 护甲 1 抽
                TargetSpeedPoint = 1,
            }, attackDir, baseAttackPower),
            RunCase(new CaseDef
            {
                Name = "target_hit_ac2",
                Dc = 0x1E14,
                AttackerHitPoint = 255,
                TargetAc = 0x0A05,      // Lo=5 Hi=10 ⇒ 护甲 2 抽
                TargetSpeedPoint = 1,
            }, attackDir, baseAttackPower),
            RunCase(new CaseDef
            {
                Name = "target_miss",
                Dc = 0x1E14,
                AttackerHitPoint = 0,   // 0 < RandomByte(254) ⇒ 基本必躲
                TargetAc = 0,
                TargetSpeedPoint = 254,
            }, attackDir, baseAttackPower),
        };

        var payload = new
        {
            generated_by = "mir2-rs/tests/parity/m3/attack-dir-probe",
            oracle = new
            {
                entry = "M2Server.Player.PlayObject.AttackDir(IActor, short, byte) (protected, 反射调用)",
                source = "src/M2Server/Player/PlayObject.Attack.cs:25-77（GetBaseAttackPoewr=:73 / AttackSuccess=:77,:225）" +
                         " + src/M2Server/Actor/BaseObject.Attack.cs:30-64（_Attack/命中门 :41）",
                rng = "OpenMir2.RandomNumber.random (私有静态 System.Random) 反射覆盖为 RecordingRandom(seed)",
            },
            seed = RngSeed,
            cases,
        };

        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outPath)));
        string json = JsonSerializer.Serialize(payload, new JsonSerializerOptions { WriteIndented = true });
        File.WriteAllText(outPath, json + Environment.NewLine, new UTF8Encoding(false));
        File.WriteAllText(Path.ChangeExtension(outPath, ".tsv"), string.Join("\n", Tsv) + "\n",
            new UTF8Encoding(false));
        Console.WriteLine($"attack-dir-probe: wrote {Path.GetFullPath(outPath)} ({cases.Count} cases) + .tsv");
        return 0;
    }
}
