// eat-probe —— 用 C# oracle 真身实测「使用药品（CM_EAT）」向量。
//
// 为什么要有这个探针：`bins/game-svr/src/useitem.rs` 的接缝必须与 oracle **逐字段/逐字节相同**，
// 而 `PlayObject.ClientUseItems`（`PlayObject.Operate.cs:601`）的守卫链（`BoCanUseItem`/`Death`/
// `MakeIndex` 首命中即 break/`GetStdItem == null`/`StdMode` 白名单）与 `EatItems`（`PlayObject.Base.cs:2047`，
// StdMode 0 的 Shape 1/2/3/default 四条分支）靠读源码重推极易抄错。
// 这里 `new PlayObject()` + **反射调私有 handler**：
//   * 状态（`ItemList`/`WAbil`/`IncHealth`/`IncSpell`/`UserUnLockDurg`）来自运行中的 oracle 代码；
//   * **即时帧**（`SM_EAT_OK=635`/`SM_EAT_FAIL=636`）由替身 `INetChannel.AddGateBuffer` 截获
//     （`SendDefMessage` → `SendSocket` 立即写 gate buffer）；
//   * **延时通知**（`RM_HEALTHSPELLCHANGED=10052`/`RM_WEIGHTCHANGED=10115`/`RM_MENU_OK=10309`）
//     由 `SendMsg` 入 `MsgQueue`，探针反射按**出队序**读出（`PriorityQueue`，优先级 = `(byte)wIdent`）。
//
// 向量落 `mir2-rs/tests/parity/m3/fixtures/eat_vectors.json`，Rust 侧用例回读逐字段核对
//（改坏任一映射 ⇒ 用例必红）。
//
// 用法（**在仓库根运行**，源锚点用相对路径）：
//   dotnet run --project mir2-rs/tests/parity/m3/eat-probe -c Release -- \
//     mir2-rs/tests/parity/m3/fixtures/eat_vectors.json

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using M2Server;
using M2Server.Items;
using M2Server.Maps;
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
using SystemModule.Conf.Model;
using SystemModule.SubSystem;

namespace EatProbe;

/// <summary>
/// 物品表用**真身 `GameItemSystem`**（`SystemShare.ItemSystem`）：`GetStdItem` / `GetUpgradeStdItem`
/// / `CopyToUserItemFromName` 全走 oracle 同一份代码，避免"转写冒充 oracle"。探针只负责把模板
/// 按 id 稠密铺进 `StdItemList`（位置 i = id i+1，见 `Main`）。
/// </summary>
// (物品表直接用真身 `M2Server.Items.GameItemSystem`，见 `Main`：按 id 稠密铺 `StdItemList`。)

/// <summary>截获到的一条**即时**帧（`CommandMessage` 在 `ServerMessage` 头之后）。</summary>
internal sealed class CapFrame
{
    public int ident;
    public int recog;
    public int param;
    public int tag;
    public int series;
    public int len;
}

/// <summary>截获 `PlayObject` 真正发出的帧（`AddGateBuffer`）。</summary>
internal sealed class CaptureNetChannel : INetChannel
{
    public readonly List<CapFrame> Frames = new();

    public void AddGateBuffer(int gateIdx, byte[] senData)
    {
        if (senData == null || senData.Length < 32)
        {
            Frames.Add(new CapFrame { ident = -1, len = senData?.Length ?? 0 });
            return;
        }
        Frames.Add(new CapFrame
        {
            ident = BitConverter.ToUInt16(senData, 24),
            recog = BitConverter.ToInt32(senData, 20),
            param = BitConverter.ToUInt16(senData, 26),
            tag = BitConverter.ToUInt16(senData, 28),
            series = BitConverter.ToUInt16(senData, 30),
            len = senData.Length,
        });
    }

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

/// <summary>
/// `SystemShare.ManageNPC` 的最小替身。can_use=false 分支的 `SendMsg(SystemShare.ManageNPC, RM_MENU_OK, ..., ActorId, ...)`
/// 会读 `baseObject.ActorId`；探针只需要它非 null（通知的 `nParam1` 是显式传入的玩家 `ActorId`）。
/// </summary>
internal class NpcProxy : DispatchProxy
{
    public static INormNpc Create() => DispatchProxy.Create<INormNpc, NpcProxy>();

    /// <summary>`SystemShare.FunctionNPC` 的替身（`IMerchant`；`UseStdModeFunItem` 只判它非 null，
    /// 再调 `GotoLable` 做脚本派发——派发本身不在本接缝的观测面内，替身吞掉即可）。</summary>
    public static IMerchant CreateMerchant() => DispatchProxy.Create<IMerchant, NpcProxy>();

    protected override object Invoke(MethodInfo targetMethod, object[] args)
    {
        var rt = targetMethod.ReturnType;
        if (rt == typeof(void))
        {
            return null;
        }
        return rt.IsValueType ? Activator.CreateInstance(rt) : null;
    }
}

internal static class Program
{
    private sealed class Spec
    {
        public int MakeIndex;
        public ushort Index;
        public ushort Dura;
        public ushort DuraMax;
    }

    private sealed class Ctx
    {
        public bool canUseItem = true;
        public bool dead = false;
        public int actorId = 0x1234;
        public bool noDrug = false;
        public int hungerStatus = 0;
        /// <summary>`SystemShare.FunctionNPC != null`（`UseStdModeFunItem` 的唯一判据，`PlayObject.Base.cs:1072`）。</summary>
        public bool functionNpc = false;
    }

    private static UserItem ToUser(Spec s)
        => new UserItem { MakeIndex = s.MakeIndex, Index = s.Index, Dura = s.Dura, DuraMax = s.DuraMax };

    private static object ItemDto(UserItem u)
    {
        if (u == null)
        {
            return null;
        }
        var desc = new int[14];
        for (int i = 0; i < 14; i++)
        {
            desc[i] = u.Desc[i];
        }
        return new { make_index = u.MakeIndex, index = (int)u.Index, dura = (int)u.Dura, dura_max = (int)u.DuraMax, desc };
    }

    private static object AbilityDto(Ability a) => new
    {
        level = a.Level,
        reserved1 = a.Reserved1,
        ac = a.AC,
        mac = a.MAC,
        dc = a.DC,
        mc = a.MC,
        sc = a.SC,
        hp = a.HP,
        mp = a.MP,
        max_hp = a.MaxHP,
        max_mp = a.MaxMP,
        reserved2 = a.Reserved2,
        exp_count = a.ExpCount,
        exp_max_count = a.ExpMaxCount,
        exp = a.Exp,
        max_exp = a.MaxExp,
        weight = a.Weight,
        max_weight = a.MaxWeight,
        wear_weight = a.WearWeight,
        max_wear_weight = a.MaxWearWeight,
        hand_weight = a.HandWeight,
        max_hand_weight = a.MaxHandWeight,
    };

    /// <summary>复制一份 `Ability`（`Abil` 与 `WAbil` 必须是不同对象：`RecalcAbilitys` 会 `WAbil = Abil.Clone()`）。</summary>
    private static Ability CloneAbility(Ability a) => new Ability
    {
        Level = a.Level,
        Reserved1 = a.Reserved1,
        AC = a.AC,
        MAC = a.MAC,
        DC = a.DC,
        MC = a.MC,
        SC = a.SC,
        HP = a.HP,
        MP = a.MP,
        MaxHP = a.MaxHP,
        MaxMP = a.MaxMP,
        Reserved2 = a.Reserved2,
        ExpCount = a.ExpCount,
        ExpMaxCount = a.ExpMaxCount,
        Exp = a.Exp,
        MaxExp = a.MaxExp,
        Weight = a.Weight,
        MaxWeight = a.MaxWeight,
        WearWeight = a.WearWeight,
        MaxWearWeight = a.MaxWearWeight,
        HandWeight = a.HandWeight,
        MaxHandWeight = a.MaxHandWeight,
    };

    private static Ability BaseAbil(ushort hp, ushort mp, ushort maxHp, ushort maxMp) => new Ability
    {
        Level = 1,
        Reserved1 = 0,
        AC = 0,
        MAC = 0,
        DC = 0x0101,
        MC = 0,
        SC = 0,
        HP = hp,
        MP = mp,
        MaxHP = maxHp,
        MaxMP = maxMp,
        Reserved2 = 0,
        ExpCount = 0,
        ExpMaxCount = 0,
        Exp = 0,
        MaxExp = 10,
        Weight = 0,
        MaxWeight = 50,
        MaxWearWeight = 15,
        MaxHandWeight = 12,
    };

    /// <summary>反射取 `ActorEntity.MsgQueue`（`protected readonly PriorityQueue&lt;SendMessage,byte&gt;`）。</summary>
    private static object GetMsgQueue(object po)
    {
        for (Type t = po.GetType(); t != null; t = t.BaseType)
        {
            var f = t.GetField("MsgQueue", BindingFlags.Instance | BindingFlags.NonPublic | BindingFlags.Public | BindingFlags.DeclaredOnly);
            if (f != null)
            {
                return f.GetValue(po);
            }
        }
        throw new MissingFieldException("ActorEntity.MsgQueue");
    }

    /// <summary>排空趟上限（记录面；超过即 `hitCap=true`，比较器必红）。</summary>
    private const int MaxDrainPasses = 8;

    /// <summary>排空**趟循环**（可单测的纯逻辑）：`hasWork` 为 false（起于空队列）⇒ `(0,false)`；
    /// 否则每趟调 `drainOnePass`，返回 `true` 表示本趟后队列又满（需下一趟，模拟投递重入），
    /// 最多 `maxPasses` 趟：某趟后不满 ⇒ `(pass+1,false)`；触顶仍满 ⇒ `(maxPasses,true)`（自然命中）。</summary>
    private static (int passes, bool hitCap) DrainPassLoop(
        int maxPasses, Func<bool> hasWork, Func<bool> drainOnePass)
    {
        if (!hasWork())
        {
            return (0, false);
        }
        for (int pass = 0; pass < maxPasses; pass++)
        {
            if (!drainOnePass())
            {
                return (pass + 1, false);
            }
        }
        return (maxPasses, true);
    }

    /// <summary>按**优先级升序出队**读出延时通知，抽空为止（上界 `MaxDrainPasses`）。
    /// `priority` 取自 `PriorityQueue.TryDequeue(out, out prio)`；`late` 取自 `SendMessage.LateDelivery`。
    /// **上界命中必报**（`hitCap=true` ⇒ 比较器红）。</summary>
    private static (List<object> messages, List<object> raw, int passes, bool hitCap) Drain(object pq)
    {
        var list = new List<object>();
        var raw = new List<object>();
        var t = pq.GetType();
        var countProp = t.GetProperty("Count") ?? throw new MissingMemberException("PriorityQueue.Count");
        var tryDequeue = t.GetMethod("TryDequeue") ?? throw new MissingMethodException("PriorityQueue.TryDequeue");
        var (passes, hitCap) = DrainPassLoop(
            MaxDrainPasses,
            () => (int)countProp.GetValue(pq) > 0,
            () =>
            {
                while ((int)countProp.GetValue(pq) > 0)
                {
                    var args = new object[] { null, null };
                    bool ok = (bool)tryDequeue.Invoke(pq, args);
                    if (!ok)
                    {
                        break;
                    }
                    var m = args[0];
                    raw.Add(m);
                    int prio = Convert.ToInt32(args[1]);
                    var mt = m.GetType();
                    list.Add(new
                    {
                        ident = (int)mt.GetField("wIdent").GetValue(m),
                        w_param = (int)mt.GetField("wParam").GetValue(m),
                        n_param1 = (int)mt.GetField("nParam1").GetValue(m),
                        n_param2 = (int)mt.GetField("nParam2").GetValue(m),
                        n_param3 = (int)mt.GetField("nParam3").GetValue(m),
                        priority = prio,
                        late = ((bool)mt.GetField("LateDelivery").GetValue(m)) ? 1 : 0,
                    });
                }
                return (int)countProp.GetValue(pq) > 0;
            });
        return (list, raw, passes, hitCap);
    }

    private static object RunCase(GameItemSystem table, CaptureNetChannel net, string name,
        Ctx ctx, List<Spec> itemList, int requestMakeIndex, string requestName,
        Ability wabilPre, int incHealthPre, int incSpellPre, bool userUnlockPre, bool pump = false)
    {
        net.Frames.Clear();

        var po = new PlayObject
        {
            Race = (byte)ActorRace.Play,
            ActorId = ctx.actorId,
            BoCanUseItem = ctx.canUseItem,
            Death = ctx.dead,
            Envir = new Envirnoment(),
            WAbil = wabilPre,
            ItemList = new List<UserItem>(),
            MagicList = new List<UserMagic>(),
            IncHealth = (ushort)incHealthPre,
            IncSpell = (ushort)incSpellPre,
            UserUnLockDurg = userUnlockPre,
            Abil = CloneAbility(wabilPre),
            UserAccount = "probe",
            LoginIpAddr = "127.0.0.1",
            IsRobot = false,
            OffLineFlag = false,
        };
        po.HungerStatus = ctx.hungerStatus;
        foreach (var s in itemList)
        {
            po.ItemList.Add(ToUser(s));
        }
        po.Envir.Flag.boNODRUG = ctx.noDrug;
        SystemShare.FunctionNPC = ctx.functionNpc ? NpcProxy.CreateMerchant() : null;

        var mi = typeof(PlayObject).GetMethod("ClientUseItems", BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingMethodException("PlayObject.ClientUseItems");
        var beforeList = po.ItemList.Select(ItemDto).ToArray();
        var beforeMakeIndexes = new HashSet<int>(po.ItemList.Select(u => u.MakeIndex));
        // `Ability` 是引用类型：`po.WAbil` 与 `wabilPre` 是同一对象，handler 会原地改。
        // 输入快照必须在 invoke **之前**取。
        var wabilBeforeDto = AbilityDto(wabilPre);
        // IT-08：**基础 `PlayObject.Abil.Exp`**（`GetExp` 只改它，不改 `WAbil.Exp`）与 `BodyLuck`/`BodyLuckLevel` 前值。
        var abilExpBefore = po.Abil.Exp;
        var bodyLuckBefore = po.BodyLuck;
        var bodyLuckLevelBefore = po.BodyLuckLevel;
        mi.Invoke(po, new object[] { requestMakeIndex, requestName });
        SystemShare.FunctionNPC = null;

        var (messages, rawMessages, drainPasses, drainHitCap) = Drain(GetMsgQueue(po));
        var immediate = net.Frames.ToList();
        // IT-08 升级支（b 缩小版）：仅对 `RM_WINEXP`/`RM_LEVELUP`/`RM_HEALTHSPELLCHANGED` 三条自足 RM 逐条走 oracle `Operate`
        // ⇒ 玩家可见的 SM 帧（SM_LEVELUP/SM_ABILITY/SM_SUBABILITY/SM_HEALTHSPELLCHANGED）落 golden。这三条 handler 只读
        // PlayObject 字段 + SendSocket，不需 Envir/Map/定时；若实测某条抛错（需 Envir）则本探针直接失败（不造假 Envir）。
        var pumpedFrames = new List<object>();
        if (pump)
        {
            var pumpSet = new HashSet<int> { 8075, 8086, 10052, 10115 };
            // `RM_HEALTHSPELLCHANGED` 的 handler 用 `baseObject.WAbil`（`Operate` 里 `baseObject=ActorMgr.Get(ActorId)`）
            // ⇒ 把本 po 登进生产 `SystemShare.ActorMgr`（不是假 Envir），其 ActorId 须唯一（升级 case 用 7777）。
            SystemShare.ActorMgr.Add(po);
            var expected = rawMessages
                .Select(m => (int)m.GetType().GetField("wIdent").GetValue(m))
                .Where(id => pumpSet.Contains(id))
                .ToList();
            var pumpIdents = new List<int>();
            var operate = typeof(PlayObject).GetMethod("Operate",
                System.Reflection.BindingFlags.Instance | System.Reflection.BindingFlags.NonPublic)
                ?? throw new MissingMethodException("PlayObject.Operate");
            foreach (var m in rawMessages)
            {
                var mt = m.GetType();
                int wid = (int)mt.GetField("wIdent").GetValue(m);
                if (!pumpSet.Contains(wid))
                {
                    continue;
                }
                pumpIdents.Add(wid);
                var pm = new ProcessMessage
                {
                    wIdent = wid,
                    wParam = (int)mt.GetField("wParam").GetValue(m),
                    nParam1 = (int)mt.GetField("nParam1").GetValue(m),
                    nParam2 = (int)mt.GetField("nParam2").GetValue(m),
                    nParam3 = (int)mt.GetField("nParam3").GetValue(m),
                    ActorId = (int)mt.GetField("ActorId").GetValue(m),
                    LateDelivery = (bool)mt.GetField("LateDelivery").GetValue(m),
                    Msg = (string)mt.GetField("Buff").GetValue(m),
                };
                operate.Invoke(po, new object[] { pm });
            }
            // 约束②：pump 的 ident 序列 == 队列里这三条 RM 的 ident 序列（防漏喂/多喂）。
            if (!pumpIdents.SequenceEqual(expected))
            {
                throw new InvalidOperationException(
                    $"pump idents {string.Join(",", pumpIdents)} != queue的这三条 {string.Join(",", expected)}");
            }
            pumpedFrames = net.Frames.Skip(immediate.Count)
                .Select(f => (object)new { ident = f.ident, recog = f.recog, param = f.param, tag = f.tag, series = f.series })
                .ToList();
        }
        else if (rawMessages.Any(m => ((int)m.GetType().GetField("wIdent").GetValue(m)) is 8086))
        {
            // 约束④：旧例（不 pump）若队列里出现 RM_LEVELUP 却没 pump，说明该例被误标——直接失败（不静默漏 SM）。
            throw new InvalidOperationException("非 pump 用例出现 RM_LEVELUP 但未开 pump（升级帧会漏）");
        }

        return new
        {
            name,
            ctx = new
            {
                can_use_item = ctx.canUseItem,
                dead = ctx.dead,
                actor_id = ctx.actorId,
                no_drug = ctx.noDrug,
                function_npc_present = ctx.functionNpc,
                red_msg_fcolor = (int)SystemShare.Config.RedMsgFColor,
                red_msg_bcolor = (int)SystemShare.Config.RedMsgBColor,
            },
            request = new { make_index = requestMakeIndex, name = requestName },
            wabil_before = wabilBeforeDto,
            inc_health_before = incHealthPre,
            inc_spell_before = incSpellPre,
            user_unlock_durg_before = userUnlockPre,
            hunger_status_before = ctx.hungerStatus,
            abil_exp_before = abilExpBefore,
            item_list_before = beforeList,
            expect = new
            {
                immediate_frames = immediate
                    .Select(f => new { ident = f.ident, recog = f.recog, param = f.param, tag = f.tag, series = f.series })
                    .ToList(),
                notices = messages,
                // IT-18：解包新建的物品（`ItemList` 中不在 `beforeList` 里的项，按生成序）。
                // `make_index` 含 `Environment.TickCount`（墙钟）⇒ 比较器排除绝对值（见 items-parity §3）。
                added_items = po.ItemList
                    .Where(it => !beforeMakeIndexes.Contains(it.MakeIndex))
                    .Select(ItemDto).ToArray(),
                wabil = AbilityDto(po.WAbil),
                // IT-08：基础 `Abil.Exp`（`GetExp` 落点）与 `BodyLuck`/`BodyLuckLevel` 后值（**追加字段**，旧键不变）。
                abil_exp = po.Abil.Exp,
                body_luck_before = bodyLuckBefore,
                body_luck = po.BodyLuck,
                body_luck_level_before = bodyLuckLevelBefore,
                body_luck_level = po.BodyLuckLevel,
                // IT-08 升级支：本 case 是否走宿主升级下行（pump）；pumped = 经 oracle `Operate` 出的玩家可见 SM 帧。
                // 非 pump 用例 host_levelup=false、pumped=[]（纯追加）。
                host_levelup = pump,
                pumped = pumpedFrames,
                inc_health = (int)po.IncHealth,
                inc_spell = (int)po.IncSpell,
                user_unlock_durg = po.UserUnLockDurg,
                hunger_status = po.HungerStatus,
                item_list = po.ItemList.Select(ItemDto).ToArray(),
                // 抽空为止：趟数 + 是否命中上界（命中 ⇒ 比较器必红，不得当完整账本）。
                drain = new { passes = drainPasses, max_passes = 8, hit_cap = drainHitCap },
            },
        };
    }

    private static StdItem Tpl(string name, byte stdMode, byte shape, byte weight, ushort ac = 0, ushort mac = 0, ushort duraMax = 0, byte aniCount = 0)
        => new StdItem
        {
            Name = name,
            StdMode = stdMode,
            Shape = shape,
            Weight = weight,
            AC = ac,
            MAC = mac,
            DuraMax = duraMax,
            AniCount = aniCount,
        };

    private static Spec S(int makeIndex, ushort index, ushort dura = 10, ushort duraMax = 20)
        => new Spec { MakeIndex = makeIndex, Index = index, Dura = dura, DuraMax = duraMax };

    /// <summary>把冻结部署的 `Exps.conf`/`Server.conf` 拷进 `AppContext.BaseDirectory`（`SystemShare` 静态构造读它）；
    /// 已在则跳过。IT-08 升级支前置：让 `GetLevelExp`/`RecalcLevelAbilitys` 用**生产**配置而非缺省。</summary>
    private static bool EnsureConfFiles(out string info)
    {
        string confDir = Environment.GetEnvironmentVariable("EATPROBE_CONF_DIR") ?? @"E:\MirServer\M2GameSvr";
        string baseDir = AppContext.BaseDirectory;
        var missing = new System.Collections.Generic.List<string>();
        foreach (var n in new[] { "Server.conf", "Exps.conf" })
        {
            var src = Path.Combine(confDir, n);
            var dst = Path.Combine(baseDir, n);
            if (!File.Exists(src)) { missing.Add(src); continue; }
            File.Copy(src, dst, overwrite: true); // 总是覆盖：防 bin 目录里残留的空 conf 让"未载入"假绿。
        }
        var exps = Path.Combine(baseDir, "Exps.conf");
        var srv = Path.Combine(baseDir, "Server.conf");
        info = $"confDir={confDir} exps_sha={Sha256Hex(exps)} srv_sha={Sha256Hex(srv)} missing={string.Join(",", missing)}";
        return missing.Count == 0;
    }

    private static string Sha256Hex(string path)
    {
        if (!File.Exists(path)) return "<missing>";
        using var sha = SHA256.Create();
        return Convert.ToHexString(sha.ComputeHash(File.ReadAllBytes(path))).ToLowerInvariant();
    }

    public static int Main(string[] args)
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        // IT-08 升级支前置（opc-07 批准）：把生产 `Exps.conf`/`Server.conf` 拷进 `AppContext.BaseDirectory`，
        // 使 `SystemShare.Config`（静态构造读该目录）拿到**生产** `NeedExps`/`nLevelValueOf*`；否则 golden 用缺省值=假。
        if (!EnsureConfFiles(out string confInfo))
        {
            Console.Error.WriteLine("EATPROBE_CONF_MISSING " + confInfo);
            return 2;
        }
        Console.Error.WriteLine("EATPROBE_CONF " + confInfo);
        LogService.Logger = new LoggerConfiguration().CreateLogger();
        // 复刻生产 `GameSrv/GameShare.LoadConfig()`：把复制的 conf 真正灌进 `SystemShare.Config`（否则 Config 停在
        // `new GameSvrConf()` 缺省、`NeedExps` 全 0）。只做配置装载，不连库/不建表/不起服务。
        SystemShare.ServerConf.LoadConfig();
        SystemShare.ExpConf.LoadConfig();
        SystemShare.GlobalConf.LoadConfig();
        SystemShare.GameSetting.LoadConfig();

        // D3 自测：排空趟循环的「自然命中」——假队列每趟都重满，跑到 MaxDrainPasses ⇒ hitCap 必真；
        // 一趟即空 ⇒ hitCap 假、passes=1；起于空队列 ⇒ passes=0。不进夹具、不依赖探针记录。
        if (args.Length > 0 && args[0] == "--drain-selftest")
        {
            var (pFull, cFull) = DrainPassLoop(MaxDrainPasses, () => true, () => true); // 每趟重满
            var (pOne, cOne) = DrainPassLoop(MaxDrainPasses, () => true, () => false); // 一趟即空
            var (pNone, cNone) = DrainPassLoop(MaxDrainPasses, () => false, () => true); // 起于空
            bool ok = cFull && pFull == MaxDrainPasses && !cOne && pOne == 1 && !cNone && pNone == 0;
            Console.WriteLine(
                $"DRAIN_SELFTEST ok={ok} always_full(passes={pFull},hitCap={cFull}) "
                + $"one_pass(passes={pOne},hitCap={cOne}) none(passes={pNone},hitCap={cNone}) "
                + $"maxPasses={MaxDrainPasses}");
            return ok ? 0 : 1;
        }

        string outPath = args.Length > 0
            ? args[0]
            : Path.Combine(AppContext.BaseDirectory, "..", "..", "..", "..", "..", "fixtures", "eat_vectors.json");

        var templates = new Dictionary<ushort, StdItem>();
        var net = new CaptureNetChannel();
        M2Share.NetChannel = net;
        SystemShare.ManageNPC = NpcProxy.Create();

        // 模板：只覆盖 StdMode 0 药品分支读到的字段（StdMode/Shape/AC/MAC/Weight）。
        templates[2001] = Tpl("hp_mp_fixed", 0, 1, 5, ac: 20, mac: 10);   // Shape 1：固定量
        templates[2002] = Tpl("hp_mp_big", 0, 1, 5, ac: 200, mac: 200);    // Shape 1：溢出 MaxHP/MaxMP
        templates[2003] = Tpl("hp_mp_percent", 0, 3, 5, ac: 5, mac: 10);   // Shape 3：百分比（含 .5 进位）
        templates[2004] = Tpl("unlock", 0, 2, 5);                          // Shape 2：解锁
        templates[2005] = Tpl("inc_pool", 0, 0, 5, ac: 15, mac: 7);        // default：进 IncHealth/IncSpell 池
        templates[2006] = Tpl("inc_zero", 0, 0, 5);                        // default：AC/MAC 都为 0（两条 >0 守卫）
        templates[2007] = Tpl("other_mode", 5, 0, 8);                      // StdMode 5：非药品 ⇒ 不吃
        templates[2008] = Tpl("weighty", 0, 1, 9, ac: 1, mac: 1);          // 称重
        templates[2011] = Tpl("noop", 2, 0, 3);                             // StdMode 2：无效果（result=true）
        templates[2012] = Tpl("food_small", 1, 0, 2, duraMax: 200);        // StdMode 1：不跨档（不触发 RefMyStatus）
        templates[2013] = Tpl("food_recalc", 1, 0, 2, duraMax: 12000);     // StdMode 1：0→1200 跨档 ⇒ RefMyStatus
        templates[2014] = Tpl("food_clamp", 1, 0, 2, duraMax: 2000);       // StdMode 1：夹到 5000（不跨档）
        templates[2015] = Tpl("funitem_dispatch", 31, 1, 3, aniCount: 5);  // StdMode 31：AniCount!=0 ⇒ UseStdModeFunItem
        templates[2016] = Tpl("unbind_full", 31, 1, 3, aniCount: 0);        // StdMode 31：AniCount==0 ⇒ 解包（容量不足时不吃）
        templates[2017] = Tpl("unbind_item", 0, 0, 4, duraMax: 7);          // IT-18 解包产物（6 件）
        templates[2018] = Tpl("exp_book", 3, 13, 1, duraMax: 100);          // IT-08 StdMode3/Shape13 经验书（GetExp）
        // 2009 故意不注册 ⇒ 模板缺失分支（稠密表该位置放空名占位 ⇒ `GetStdItem` 返回 null）

        // **用真身 `GameItemSystem`**：`GetStdItem`/`GetUpgradeStdItem`/`CopyToUserItemFromName`
        // 全走 oracle 同一份代码。`StdItemList` 按 id 稠密铺：位置 i = id i+1，空位放空名占位
        // （`GetStdItem` 对空名返回 null；`CopyToUserItemFromName` 的 `stdItem.Name` 为空串、不 NRE）。
        var table = new GameItemSystem();
        ushort maxId = templates.Keys.Max();
        for (ushort id = 1; id <= maxId; id++)
        {
            table.AddItem(templates.TryGetValue(id, out var t) ? t : new StdItem { Name = string.Empty });
        }
        SystemShare.ItemSystem = table;

        // `M2Share.UnbindList`（`M2Share.cs:85`，默认 null）：`StdItem.Shape → 解包物名`。
        // 只挂 Shape 1（2016 的 Shape）⇒ 成功解包出 6 件 2017；其余 Shape 未命中 ⇒ 空名 ⇒ 0 件。
        M2Share.UnbindList = new Dictionary<int, string> { [1] = "unbind_item" };

        var cases = new List<object>();

        cases.Add(RunCase(table, net, "eat_ok_shape1_fixed", new Ctx(),
            new List<Spec> { S(1, 2001) }, 1, "hp_mp_fixed",
            BaseAbil(hp: 50, mp: 40, maxHp: 100, maxMp: 80), 0, 0, false));
        cases.Add(RunCase(table, net, "eat_ok_shape1_clamp", new Ctx(),
            new List<Spec> { S(1, 2002) }, 1, "hp_mp_big",
            BaseAbil(hp: 50, mp: 40, maxHp: 100, maxMp: 80), 0, 0, false));
        cases.Add(RunCase(table, net, "eat_ok_shape3_percent", new Ctx(),
            new List<Spec> { S(1, 2003) }, 1, "hp_mp_percent",
            BaseAbil(hp: 50, mp: 40, maxHp: 101, maxMp: 55), 0, 0, false));
        cases.Add(RunCase(table, net, "eat_ok_shape2_unlock", new Ctx(),
            new List<Spec> { S(1, 2004) }, 1, "unlock",
            BaseAbil(hp: 50, mp: 40, maxHp: 100, maxMp: 80), 0, 0, false));
        cases.Add(RunCase(table, net, "eat_ok_shape0_inc_pool", new Ctx(),
            new List<Spec> { S(1, 2005) }, 1, "inc_pool",
            BaseAbil(hp: 50, mp: 40, maxHp: 100, maxMp: 80), 3, 4, false));
        cases.Add(RunCase(table, net, "eat_ok_shape0_inc_zero", new Ctx(),
            new List<Spec> { S(1, 2006) }, 1, "inc_zero",
            BaseAbil(hp: 50, mp: 40, maxHp: 100, maxMp: 80), 3, 4, false));
        cases.Add(RunCase(table, net, "eat_ok_weight_from_remaining_bag", new Ctx(),
            new List<Spec> { S(7, 2008), S(1, 2001), S(8, 2007) }, 1, "hp_mp_fixed",
            BaseAbil(hp: 50, mp: 40, maxHp: 100, maxMp: 80), 0, 0, false));
        cases.Add(RunCase(table, net, "eat_ok_first_makeindex_wins", new Ctx(),
            new List<Spec> { S(1, 2001), S(1, 2003) }, 1, "hp_mp_fixed",
            BaseAbil(hp: 50, mp: 40, maxHp: 100, maxMp: 80), 0, 0, false));
        cases.Add(RunCase(table, net, "eat_fail_not_found", new Ctx(),
            new List<Spec> { S(1, 2001) }, 99, "hp_mp_fixed",
            BaseAbil(hp: 50, mp: 40, maxHp: 100, maxMp: 80), 0, 0, false));
        cases.Add(RunCase(table, net, "eat_fail_stdmode_other", new Ctx(),
            new List<Spec> { S(1, 2007) }, 1, "other_mode",
            BaseAbil(hp: 50, mp: 40, maxHp: 100, maxMp: 80), 0, 0, false));
        cases.Add(RunCase(table, net, "eat_fail_template_missing", new Ctx(),
            new List<Spec> { S(1, 2009) }, 1, "missing",
            BaseAbil(hp: 50, mp: 40, maxHp: 100, maxMp: 80), 0, 0, false));
        cases.Add(RunCase(table, net, "eat_fail_can_use_item_false", new Ctx { canUseItem = false },
            new List<Spec> { S(1, 2001) }, 1, "hp_mp_fixed",
            BaseAbil(hp: 50, mp: 40, maxHp: 100, maxMp: 80), 0, 0, false));
        cases.Add(RunCase(table, net, "eat_fail_dead", new Ctx { dead = true },
            new List<Spec> { S(1, 2001) }, 1, "hp_mp_fixed",
            BaseAbil(hp: 50, mp: 40, maxHp: 100, maxMp: 80), 0, 0, false));
        cases.Add(RunCase(table, net, "eat_ok_stdmode2_noop", new Ctx(),
            new List<Spec> { S(1, 2011) }, 1, "noop",
            BaseAbil(hp: 50, mp: 40, maxHp: 100, maxMp: 80), 0, 0, false));
        cases.Add(RunCase(table, net, "eat_ok_stdmode1_hunger_no_recalc", new Ctx(),
            new List<Spec> { S(1, 2012) }, 1, "food_small",
            BaseAbil(hp: 50, mp: 40, maxHp: 100, maxMp: 80), 0, 0, false));
        // 跨档 ⇒ `RefMyStatus()` ⇒ `RecalcAbilitys()`。运行时 `Abil` 已是**登录时的等级派生值**
        //（`PlayObject.cs:1308` 先跑 `RecalcLevelAbilitys()`）⇒ 这里用 level=1 战士的 19/15；
        // 且 `HP/MP <= MaxHP/MaxMP`（oracle 登录路径的 Play 分类钳位保证），否则会触发
        // `PlayObject.RecalcAbilitys` 尚未建模的 `WAbil.HP > MaxHP ⇒ HP=MaxHP` 尾钳。
        cases.Add(RunCase(table, net, "eat_ok_stdmode1_hunger_recalc", new Ctx(),
            new List<Spec> { S(1, 2013) }, 1, "food_recalc",
            BaseAbil(hp: 15, mp: 15, maxHp: 19, maxMp: 15), 0, 0, false));
        cases.Add(RunCase(table, net, "eat_ok_stdmode1_hunger_clamp", new Ctx { hungerStatus = 4950 },
            new List<Spec> { S(1, 2014) }, 1, "food_clamp",
            BaseAbil(hp: 50, mp: 40, maxHp: 100, maxMp: 80), 0, 0, false));
        cases.Add(RunCase(table, net, "eat_fail_nodrug_stdmode1", new Ctx { noDrug = true },
            new List<Spec> { S(1, 2013) }, 1, "food_recalc",
            BaseAbil(hp: 50, mp: 40, maxHp: 100, maxMp: 80), 0, 0, false));
        cases.Add(RunCase(table, net, "eat_fail_nodrug_no_consume", new Ctx { noDrug = true },
            new List<Spec> { S(1, 2001) }, 1, "hp_mp_fixed",
            BaseAbil(hp: 50, mp: 40, maxHp: 100, maxMp: 80), 0, 0, false));
        // StdMode 31：`UseStdModeFunItem`（`PlayObject.Base.cs:1072`）= `SystemShare.FunctionNPC != null`。
        // 存在 ⇒ `GotoLable("@StdModeFunc" + AniCount)` 后返回 true ⇒ 吃 + SM_EAT_OK + RM_WEIGHTCHANGED。
        cases.Add(RunCase(table, net, "eat_ok_stdmode31_funitem", new Ctx { functionNpc = true },
            new List<Spec> { S(1, 2015) }, 1, "funitem_dispatch",
            BaseAbil(hp: 50, mp: 40, maxHp: 100, maxMp: 80), 0, 0, false));
        // FunctionNPC 缺失 ⇒ `UseStdModeFunItem` 返回 false ⇒ 不吃。
        cases.Add(RunCase(table, net, "eat_fail_stdmode31_no_functionnpc", new Ctx { functionNpc = false },
            new List<Spec> { S(1, 2015) }, 1, "funitem_dispatch",
            BaseAbil(hp: 50, mp: 40, maxHp: 100, maxMp: 80), 0, 0, false));
        // StdMode 31 / AniCount==0：解包需 `ItemList.Count + 6 - 1 <= Grobal2.MaxBagItem`（46）。
        // 背包 42 件 ⇒ 42+5=47 > 46 ⇒ 容量守卫失败（未到 `ClientUseItemsGetUnBindItems`）⇒ 不吃。
        var unbindFullBag = new List<Spec> { S(1, 2016) };
        for (int i = 0; i < 41; i++)
        {
            unbindFullBag.Add(S(200 + i, 0));
        }
        cases.Add(RunCase(table, net, "eat_fail_stdmode31_unbind_bag_full", new Ctx(),
            unbindFullBag, 1, "unbind_full",
            BaseAbil(hp: 50, mp: 40, maxHp: 100, maxMp: 80), 0, 0, false));
        // IT-18 成功路径：背包仅 1 件解包件（2016，Shape=1/AniCount=0）⇒ 1+6-1=6 <= 46 ⇒
        // Dispose+RemoveAt 后 `ClientUseItemsGetUnBindItems("unbind_item", 6)` 建 6 件 2017，
        // 每件 `SendAddItem` ⇒ 6 条 SM_ADDITEM(200) 即时帧，再 SM_EAT_OK + RM_WEIGHTCHANGED。
        cases.Add(RunCase(table, net, "eat_ok_stdmode31_unbind_success", new Ctx(),
            new List<Spec> { S(1, 2016) }, 1, "unbind_full",
            BaseAbil(hp: 50, mp: 40, maxHp: 100, maxMp: 80), 0, 0, false));
        // IT-08：StdMode3/Shape13 `GetExp(DuraMax=100)` **非升级支**（`MaxExp=10000` ⇒ `0+100<10000` 不升级）。
        // 观测新增 `abil_exp`/`body_luck*`（追加字段）；`WAbil.Exp` 保持陈旧（GetExp 只改 Abil）。
        var expBookAbil = BaseAbil(hp: 50, mp: 40, maxHp: 100, maxMp: 80);
        expBookAbil.MaxExp = 10000;
        cases.Add(RunCase(table, net, "eat_ok_stdmode3_shape13_exp_nonlevelup", new Ctx(),
            new List<Spec> { S(1, 2018) }, 1, "exp_book", expBookAbil, 0, 0, false));
        // IT-08 **升级支**：Exp=9990/MaxExp=10000/DuraMax=100 ⇒ 9990+100>=10000 升级；pump 三条 RM
        // （RM_WINEXP/RM_LEVELUP/RM_HEALTHSPELLCHANGED）⇒ golden 带玩家可见 SM 三帧。需生产 Exps.conf（已由 EnsureConfFiles 载入）。
        var expUpAbil = BaseAbil(hp: 50, mp: 40, maxHp: 100, maxMp: 80);
        expUpAbil.MaxExp = 10000;
        expUpAbil.Exp = 9990;
        expUpAbil.Level = 5;
        cases.Add(RunCase(table, net, "eat_ok_stdmode3_shape13_exp_levelup", new Ctx { actorId = 7777 },
            new List<Spec> { S(1, 2018) }, 1, "exp_book", expUpAbil, 0, 0, false, pump: true));

        var templateDtos = new Dictionary<string, object>();
        foreach (var kv in templates)
        {
            var t = kv.Value;
            templateDtos[kv.Key.ToString()] = new
            {
                name = t.Name,
                std_mode = t.StdMode,
                shape = t.Shape,
                weight = t.Weight,
                ac = t.AC,
                mac = t.MAC,
                dura_max = t.DuraMax,
                ani_count = t.AniCount,
            };
        }

        var oracleSourceFiles = new[]
        {
            "src/M2Server/Player/PlayObject.Operate.cs",
            "src/M2Server/Player/PlayObject.Base.cs",
            "src/M2Server/Actor/BaseObject.cs",
            "src/M2Server/Actor/BaseObject.Message.cs",
            "src/M2Server/Player/PlayObject.Socket.cs",
            "src/Modules/SystemModule/Data/SendMessage.cs",
            "src/M2Server/Items/GameItemSystem.cs",
            "src/OpenMir2/Grobal2.cs",
        };
        string sourceAnchor = SourceAnchorSha256(oracleSourceFiles);
        string asmPath = typeof(M2Share).Assembly.Location;
        string asmSha = Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(asmPath))).ToLowerInvariant();

        // IT-08 升级支：证"生产 Exps.conf 真被载入"（Level6=80，来自部署 Exps.conf），否则缺省值会污染升级 golden。
        int needExps6 = SystemShare.Config.NeedExps[6];
        if (needExps6 != 80)
            throw new InvalidOperationException(
                $"NeedExps[6]={needExps6} != 80（生产 Exps.conf 未按预期载入；head={string.Join(",", SystemShare.Config.NeedExps.Take(8))}）");
        var doc = new
        {
            generator = "mir2-rs/tests/parity/m3/eat-probe",
            oracle_assembly = Path.GetFileName(asmPath),
            oracle_assembly_sha256 = asmSha,
            oracle_assembly_sha256_note = "生成时 M2Server.dll 构建产物 hash（随编译器/SDK/环境变化）；仅作溯源，不参与向量校验——校验锚点是 oracle_source_sha256",
            oracle_source_sha256 = sourceAnchor,
            oracle_source_sha256_files = oracleSourceFiles,
            oracle_source = new[]
            {
                "src/M2Server/Player/PlayObject.Message.cs:826 (CM_EAT -> ClientUseItems)",
                "src/M2Server/Player/PlayObject.Operate.cs:601 (ClientUseItems)",
                "src/M2Server/Player/PlayObject.Base.cs:2047 (EatItems; StdMode 0/1/2)",
                "src/M2Server/Player/PlayObject.Base.cs:1072 (UseStdModeFunItem; StdMode 31 AniCount!=0)",
                "src/OpenMir2/Grobal2.cs:62 (MaxBagItem = 46; StdMode 31 AniCount==0 容量守卫)",
                "src/M2Server/Player/PlayObject.Base.cs:288 (RefMyStatus = RecalcAbilitys + RM_MYSTATUS)",
                "src/M2Server/Player/PlayObject.Base.cs:1183 (GetMyStatus = min(HungerStatus/1000, 4))",
                "src/M2Server/Actor/BaseObject.cs:952 (IncHealthSpell)",
                "src/M2Server/Actor/BaseObject.cs:700 (HealthSpellChanged)",
                "src/M2Server/Actor/BaseObject.cs:1771 (WeightChanged) / :1356 (RecalcBagWeight)",
                "src/M2Server/Player/PlayObject.Socket.cs:17 (SendDefMessage)",
            },
            notes = new[]
            {
                "有效物品 = 模板（探针 GetUpgradeStdItem 是恒等拷贝；Desc 升级/StdMode 变换不在本片作用域）",
                "即时帧 = SendDefMessage/SendSocket 直接写 gate buffer（SM_EAT_OK=635 / SM_EAT_FAIL=636）",
                "延时通知 = SendMsg 入 MsgQueue（PriorityQueue，优先级 = (byte)wIdent），探针按出队序读；真实协议上它晚于即时帧由消息泵下发",
                "StdMode 2（无效果）已覆盖；StdMode 31 的 `UseStdModeFunItem`（AniCount!=0）、AniCount==0 的容量不足分支、以及 AniCount==0 的解包成功（`UnbindList[Shape]` 命中 ⇒ 6 件 `CopyToUserItemFromName` + 6 条 SM_ADDITEM）均已覆盖；StdMode 3/4 仍在 T-17。解包产物 `make_index` 含 Environment.TickCount（墙钟）⇒ 比较器排除绝对值。",
                "Envir.Flag.boNODRUG 分支（地图禁药）已覆盖：SysMsg(Red) 入 MsgQueue（RM_SYSMESSAGE=11004，颜色取自 Config.RedMsgFColor/RedMsgBColor），物品不消耗、背包不动、落 SM_EAT_FAIL",
            },
            unbind = M2Share.UnbindList.ToDictionary(kv => kv.Key.ToString(), kv => kv.Value),
            // IT-08 升级支：配置文件身份钉进 golden（路径 + sha256；配置一变换 ⇒ 该 golden 失效）。
            config = new
            {
                exps_conf = Path.Combine(AppContext.BaseDirectory, "Exps.conf"),
                exps_conf_sha256 = Sha256Hex(Path.Combine(AppContext.BaseDirectory, "Exps.conf")),
                server_conf_sha256 = Sha256Hex(Path.Combine(AppContext.BaseDirectory, "Server.conf")),
                need_exps_head = SystemShare.Config.NeedExps.Take(8).ToArray(),
                need_exps_6 = SystemShare.Config.NeedExps[6],
            },
            templates = templateDtos,
            cases,
        };

        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outPath))!);
        // 落盘统一 LF：Windows 上 `JsonSerializer` 缩进为 CRLF，仓库约定 LF，
        // 否则 `git diff --check` 会逐行判 trailing whitespace。
        string json = JsonSerializer.Serialize(doc, new JsonSerializerOptions { WriteIndented = true }).Replace("\r\n", "\n");
        File.WriteAllText(outPath, json + "\n", new UTF8Encoding(false));
        Console.WriteLine($"WROTE {Path.GetFullPath(outPath)}");
        Console.WriteLine($"ORACLE_SOURCE_SHA256 {sourceAnchor}");
        Console.WriteLine($"TEMPLATES {templateDtos.Count} CASES {cases.Count}");
        return 0;
    }

    internal static string SourceAnchorSha256(IEnumerable<string> files)
    {
        using var sha = SHA256.Create();
        using var ms = new MemoryStream();
        foreach (var rel in files)
        {
            if (!File.Exists(rel))
            {
                throw new FileNotFoundException($"oracle 源锚点文件缺失（请在仓库根运行）：{rel}");
            }
            ms.Write(Encoding.UTF8.GetBytes(rel + "\n"));
            ms.Write(File.ReadAllBytes(rel));
            ms.WriteByte((byte)'\n');
        }
        return Convert.ToHexString(sha.ComputeHash(ms.ToArray())).ToLowerInvariant();
    }
}
