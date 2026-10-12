// struck-durability-probe —— 用 C# oracle 真身实测「受击耐久磨损（`PlayObject.StruckDamage`）」向量。
//
// 为什么要有这个探针（reviewer-5 对 B-1/B-2 片的 Minor ①）：
//   `PlayObject.StruckDamage`（`src/M2Server/Player/PlayObject.cs:2954`）在 `base.StruckDamage`
//   之后做三件事——① `:2957` 无条件抽一次 `Random(10)`（`nDam = Random(10)+5`）；
//   ② `:2965-2996` **衣服位（`ItemLocation.Dress = 0`）特判**（不抽数，只扣耐久，但**破零会先清
//   `Index`**）；③ `:2998-3039` 逐格循环（`Index > 0` 的每格各抽一次 `Random(8)`，破零同样清 `Index`）。
//   前两者都影响**取数次数**：衣服位在特判里破零 ⇒ 循环会跳过该格的 `Random(8)` ⇒ 全局 RNG 流错位。
//   旧 Rust 侧只建模了 ③（且用 `saturating_sub`），本探针把真身的**取数账本 + 前后状态**钉死。
//
// 做法（照 `proper-target-probe` / `combat-probe` 形态）：
//   * `new PlayObject()`（真身前构造，**之后**才注入种子——构造函数本身消耗随机数）；
//   * `SystemShare.ItemSystem` 用最小替身（`GetStdItem` 恒 null ⇒ `SendDelItems` 不落帧）；
//   * `M2Share.NetChannel` 用空替身；`Envir = null` ⇒ `FeatureChanged` 早退；
//   * 种子注入 = 反射覆盖 `OpenMir2.RandomNumber.random`（私有静态 `System.Random`）为
//     一个记账用的 `RecordingRandom`（与 D2 的 `RngSeedHook` 同一注入点）；
//   * 直调**真身** `PlayObject.StruckDamage(1)`，记 `Next(max)` 账本（`ReplayRandom` 行式格式）
//     与 `UseItems` 的前后状态。
//
// 产物 = mir2-rs/tests/parity/m3/struck-durability-probe/vectors/struck_durability_vectors.{json,tsv}
// Rust 侧 `crates/world/tests/struck_durability_vectors.rs` 用 `ReplayRandom` 回放逐字段核对。
//
// 用法（**在仓库根运行**）：
//   dotnet run --project mir2-rs/tests/parity/m3/struck-durability-probe -c Release -- <输出路径>
//
// 只读：不连 MySQL、不写任何现网文件、不发起任何 socket。

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using M2Server;
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

namespace StruckDurabilityProbe;

/// <summary>`SystemShare.ItemSystem` 的最小替身。
///
/// `GetStdItem` **必须返回非 null**：oracle 的 `PlayObject.StruckDamage` 在衣服位破零分支里
/// 直接读 `stdItem.NeedIdentify`（`PlayObject.cs:2974`，没有判空）⇒ 返回 null 会 NPE。
/// 返回 `NeedIdentify = 0` 的模板 ⇒ 跳过事件日志；后续 `SendDelItems` 走空帧替身。
/// 本探针不关心帧面，只钉取数账本与 `UseItems` 状态。</summary>
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

/// <summary>空 `INetChannel`（本探针不消费帧；只为满足 `M2Share.NetChannel` 非空）。</summary>
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

    /// <summary>`System.Random` 的记录包装：按 `ReplayRandom` 行式格式（`seq|kind|arg|result|site`）记账。</summary>
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

    /// <summary>构造一个真 `PlayObject` 守方（`Envir = null`；构造后再注入种子）。</summary>
    private static PlayObject NewTarget()
    {
        var po = new PlayObject
        {
            Race = ActorRace.Play,
            ActorId = 202,
            ChrName = "victim",
            Job = PlayerJob.Warrior,
            Abil = new Ability { Level = 1, HP = 30000, MP = 100, MaxHP = 30000, MaxMP = 100 },
            AddAbil = new AddAbility(),
            Envir = null,
        };
        po.WAbil = new Ability { HP = 30000, MaxHP = 30000, MP = 100, MaxMP = 100 };
        po.UseItems = new UserItem[13];
        return po;
    }

    /// <summary>校准：同 seed 下 `StruckDamage` 的第一次取数就是 `Random(10)` ⇒ `nDam = 结果 + 5`。</summary>
    private static int CalibrateNDam(int seed)
    {
        var po = NewTarget();
        SeedRng(seed);
        po.StruckDamage(1);
        string first = Draws()[0].Split('|')[3];
        return int.Parse(first) + 5;
    }

    /// <summary>在 `1..20000` 内找一个 seed，使「slot1 已装备、slot0 空」时第二次取数
    /// （即逐格循环的第一次 `Random(8)`）恰为 0 ⇒ 能构造「循环里破零」的真身用例。</summary>
    private static int FindLoopBreakSeed()
    {
        for (int seed = 1; seed <= 20000; seed++)
        {
            var po = NewTarget();
            po.UseItems[1] = new UserItem { Index = 101, Dura = 1000, DuraMax = 1000 };
            SeedRng(seed);
            po.StruckDamage(1);
            var draws = Draws();
            if (draws.Count >= 2 && draws[1].Split('|')[1] == "Next(max)" && draws[1].Split('|')[2] == "8"
                && draws[1].Split('|')[3] == "0")
            {
                return seed;
            }
        }
        throw new InvalidOperationException("未找到逐格 Random(8)==0 的种子");
    }

    private sealed class SlotSpec
    {
        public int Slot;
        public ushort Index;
        /// <summary>`null` = 空位；否则 `Dura = nDam + Delta`（校准后计算）。</summary>
        public int? Delta;
    }

    private static object RunCase(string name, int seed, IReadOnlyList<SlotSpec> specs)
    {
        int nDam = CalibrateNDam(seed);
        var po = NewTarget();
        foreach (var s in specs)
        {
            if (s.Delta is null)
            {
                continue;
            }
            ushort dura = (ushort)(nDam + s.Delta.Value);
            po.UseItems[s.Slot] = new UserItem { Index = s.Index, Dura = dura, DuraMax = dura };
        }

        SeedRng(seed); // 与校准同一 seed ⇒ 第一次取数（nDam）逐字节相同
        string inState = State(po);
        po.StruckDamage(1);
        string outState = State(po);
        string draws = string.Join(";", Draws());

        return new
        {
            name,
            seed,
            n_dam = nDam,
            draws,
            items_in = inState,
            items_out = outState,
        };
    }

    /// <summary>把「仍装备（`Index > 0`）」的格子编成 `slot:index:dura;...`（按 slot 升序）。</summary>
    private static string State(PlayObject po)
    {
        var parts = new List<string>();
        for (int i = 0; i < po.UseItems.Length; i++)
        {
            UserItem it = po.UseItems[i];
            if (it != null && it.Index > 0)
            {
                parts.Add($"{i}:{it.Index}:{it.Dura}");
            }
        }
        return string.Join(";", parts);
    }

    public static int Main(string[] args)
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        LogService.Logger = new LoggerConfiguration().CreateLogger();
        M2Share.NetChannel = new NullNetChannel();
        SystemShare.ItemSystem = new NullItemSystem();

        string outPath = args.Length > 0
            ? args[0]
            : Path.Combine(AppContext.BaseDirectory, "..", "..", "..", "..", "..",
                "struck-durability-probe", "vectors", "struck_durability_vectors.json");

        var cases = new List<object>
        {
            // 衣服位 Dura == nDam ⇒ 特判里破零、清 Index ⇒ 同一击逐格循环跳过 slot0 的 Random(8)。
            RunCase("dress_break", 20261012, new[]
            {
                new SlotSpec { Slot = 0, Index = 100, Delta = 0 },
                new SlotSpec { Slot = 1, Index = 101, Delta = 10000 },
                new SlotSpec { Slot = 7, Index = 107, Delta = 10000 },
            }),
            // 衣服位 Dura < nDam ⇒ unchecked ushort 回绕成大正数、不破零 ⇒ 循环照抽 slot0 的 Random(8)。
            RunCase("dress_wrap", 20261013, new[]
            {
                new SlotSpec { Slot = 0, Index = 100, Delta = -2 },
                new SlotSpec { Slot = 1, Index = 101, Delta = 10000 },
            }),
            // 衣服位 Dura > nDam ⇒ 正常扣耐久，不破零 ⇒ 循环照抽 slot0。
            RunCase("dress_normal", 20261014, new[]
            {
                new SlotSpec { Slot = 0, Index = 100, Delta = 1000 },
                new SlotSpec { Slot = 1, Index = 101, Delta = 10000 },
            }),
            // 衣服位空 ⇒ 特判整段跳过；只走逐格循环。
            RunCase("dress_empty", 20261015, new[]
            {
                new SlotSpec { Slot = 1, Index = 101, Delta = 0 },
            }),
            // 循环里破零：`Random(8)==0` 且 `Dura == nDam` ⇒ 清 slot1 的 Index（影响后续攻击取数）。
            RunCase("loop_break", FindLoopBreakSeed(), new[]
            {
                new SlotSpec { Slot = 1, Index = 101, Delta = 0 },
            }),
        };

        var payload = new
        {
            generated_by = "mir2-rs/tests/parity/m3/struck-durability-probe",
            oracle = new
            {
                entry = "M2Server.Player.PlayObject.StruckDamage (public override, 直调)",
                source = "src/M2Server/Player/PlayObject.cs:2954-3036（nDam=:2957 / 衣服位特判=:2965-2996 / 逐格循环=:2998-3039）",
                rng = "OpenMir2.RandomNumber.random (私有静态 System.Random) 反射覆盖为 RecordingRandom(seed)",
                item_system = "SystemShare.ItemSystem = NullItemSystem（GetStdItem 恒 null ⇒ SendDelItems 早退）",
            },
            cases,
        };

        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outPath)));
        string json = JsonSerializer.Serialize(payload, new JsonSerializerOptions { WriteIndented = true });
        File.WriteAllText(outPath, json + Environment.NewLine, new UTF8Encoding(false));

        var tsv = new List<string> { "kind\tname\tseed\tn_dam\tdraws\titems_in\titems_out" };
        foreach (dynamic c in cases)
        {
            tsv.Add($"cloth\t{c.name}\t{c.seed}\t{c.n_dam}\t{c.draws}\t{c.items_in}\t{c.items_out}");
        }
        string tsvPath = Path.ChangeExtension(outPath, ".tsv");
        File.WriteAllText(tsvPath, string.Join("\n", tsv) + "\n", new UTF8Encoding(false));

        Console.WriteLine($"struck-durability-probe: wrote {Path.GetFullPath(outPath)} ({cases.Count} cases) + {Path.GetFileName(tsvPath)}");
        return 0;
    }
}
