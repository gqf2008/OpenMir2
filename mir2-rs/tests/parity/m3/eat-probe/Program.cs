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
/// `SystemShare.ItemSystem` 的内存替身：
/// - `GetStdItem(ushort)` 返回注入的模板；
/// - `GetUpgradeStdItem` = **恒等拷贝**（有效物品 = 模板；Desc 升级不在本片作用域）；
/// - `GetStdItemName` 返回模板名。
/// </summary>
internal sealed class ProbeItemSystem : IItemSystem
{
    public readonly Dictionary<ushort, StdItem> Items = new();

    public StdItem GetStdItem(ushort nItemIdx)
        => Items.TryGetValue(nItemIdx, out var it) ? it : null;

    public string GetStdItemName(int itemIdx)
        => Items.TryGetValue((ushort)itemIdx, out var it) ? it.Name : string.Empty;

    public int GetUpgradeStdItem(StdItem stdItem, UserItem userItem, ref ClientItem clientItem)
    {
        if (clientItem == null)
        {
            clientItem = new ClientItem();
        }
        var d = clientItem.Item;
        d.Name = stdItem.Name;
        d.StdMode = stdItem.StdMode;
        d.Shape = stdItem.Shape;
        d.Weight = stdItem.Weight;
        d.AC = stdItem.AC;
        d.MAC = stdItem.MAC;
        return 0;
    }

    public int RealAttackSpeed(short wAtkSpd) => wAtkSpd <= 10 ? -wAtkSpd : wAtkSpd - 10;
    public void AddItem(StdItem stdItem) => throw new NotSupportedException();
    public void Clear() => throw new NotSupportedException();
    public int ItemCount => Items.Count;
    public ushort GetStdItemIdx(string itemName) => throw new NotSupportedException();
    public bool CopyToUserItemFromName(string sItemName, ref UserItem item) => throw new NotSupportedException();
    public StdItem GetStdItem(string sItemName) => throw new NotSupportedException();
    public int GetStdItemWeight(int nItemIdx) => throw new NotSupportedException();
    public int GetUpgrade2(int x, int a) => throw new NotSupportedException();
    public void RandomSetUnknownItem(StdItem stdItem, UserItem pu) => throw new NotSupportedException();
    public void RandomUpgradeItem(StdItem stdItem, UserItem pu) => throw new NotSupportedException();
}

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

    /// <summary>按**出队序**（优先级 = `(byte)wIdent`）读出延时通知。</summary>
    private static List<object> Drain(object pq)
    {
        var list = new List<object>();
        var t = pq.GetType();
        var countProp = t.GetProperty("Count") ?? throw new MissingMemberException("PriorityQueue.Count");
        var dequeue = t.GetMethod("Dequeue") ?? throw new MissingMethodException("PriorityQueue.Dequeue");
        while ((int)countProp.GetValue(pq) > 0)
        {
            var m = dequeue.Invoke(pq, null);
            var mt = m.GetType();
            list.Add(new
            {
                ident = (int)mt.GetField("wIdent").GetValue(m),
                w_param = (int)mt.GetField("wParam").GetValue(m),
                n_param1 = (int)mt.GetField("nParam1").GetValue(m),
                n_param2 = (int)mt.GetField("nParam2").GetValue(m),
                n_param3 = (int)mt.GetField("nParam3").GetValue(m),
            });
        }
        return list;
    }

    private static object RunCase(ProbeItemSystem table, CaptureNetChannel net, string name,
        Ctx ctx, List<Spec> itemList, int requestMakeIndex, string requestName,
        Ability wabilPre, int incHealthPre, int incSpellPre, bool userUnlockPre)
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
        // `Ability` 是引用类型：`po.WAbil` 与 `wabilPre` 是同一对象，handler 会原地改。
        // 输入快照必须在 invoke **之前**取。
        var wabilBeforeDto = AbilityDto(wabilPre);
        mi.Invoke(po, new object[] { requestMakeIndex, requestName });
        SystemShare.FunctionNPC = null;

        var messages = Drain(GetMsgQueue(po));
        var immediate = net.Frames.ToList();

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
            item_list_before = beforeList,
            expect = new
            {
                immediate_frames = immediate
                    .Select(f => new { ident = f.ident, recog = f.recog, param = f.param, tag = f.tag, series = f.series })
                    .ToList(),
                notices = messages,
                wabil = AbilityDto(po.WAbil),
                inc_health = (int)po.IncHealth,
                inc_spell = (int)po.IncSpell,
                user_unlock_durg = po.UserUnLockDurg,
                hunger_status = po.HungerStatus,
                item_list = po.ItemList.Select(ItemDto).ToArray(),
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

    public static int Main(string[] args)
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        LogService.Logger = new LoggerConfiguration().CreateLogger();

        string outPath = args.Length > 0
            ? args[0]
            : Path.Combine(AppContext.BaseDirectory, "..", "..", "..", "..", "..", "fixtures", "eat_vectors.json");

        var table = new ProbeItemSystem();
        SystemShare.ItemSystem = table;
        var net = new CaptureNetChannel();
        M2Share.NetChannel = net;
        SystemShare.ManageNPC = NpcProxy.Create();

        // 模板：只覆盖 StdMode 0 药品分支读到的字段（StdMode/Shape/AC/MAC/Weight）。
        table.Items[2001] = Tpl("hp_mp_fixed", 0, 1, 5, ac: 20, mac: 10);   // Shape 1：固定量
        table.Items[2002] = Tpl("hp_mp_big", 0, 1, 5, ac: 200, mac: 200);    // Shape 1：溢出 MaxHP/MaxMP
        table.Items[2003] = Tpl("hp_mp_percent", 0, 3, 5, ac: 5, mac: 10);   // Shape 3：百分比（含 .5 进位）
        table.Items[2004] = Tpl("unlock", 0, 2, 5);                          // Shape 2：解锁
        table.Items[2005] = Tpl("inc_pool", 0, 0, 5, ac: 15, mac: 7);        // default：进 IncHealth/IncSpell 池
        table.Items[2006] = Tpl("inc_zero", 0, 0, 5);                        // default：AC/MAC 都为 0（两条 >0 守卫）
        table.Items[2007] = Tpl("other_mode", 5, 0, 8);                      // StdMode 5：非药品 ⇒ 不吃
        table.Items[2008] = Tpl("weighty", 0, 1, 9, ac: 1, mac: 1);          // 称重
        table.Items[2011] = Tpl("noop", 2, 0, 3);                             // StdMode 2：无效果（result=true）
        table.Items[2012] = Tpl("food_small", 1, 0, 2, duraMax: 200);        // StdMode 1：不跨档（不触发 RefMyStatus）
        table.Items[2013] = Tpl("food_recalc", 1, 0, 2, duraMax: 12000);     // StdMode 1：0→1200 跨档 ⇒ RefMyStatus
        table.Items[2014] = Tpl("food_clamp", 1, 0, 2, duraMax: 2000);       // StdMode 1：夹到 5000（不跨档）
        table.Items[2015] = Tpl("funitem_dispatch", 31, 1, 3, aniCount: 5);  // StdMode 31：AniCount!=0 ⇒ UseStdModeFunItem
        table.Items[2016] = Tpl("unbind_full", 31, 1, 3, aniCount: 0);        // StdMode 31：AniCount==0 ⇒ 解包（容量不足时不吃）
        // 2009 故意不注册 ⇒ 模板缺失分支

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
        cases.Add(RunCase(table, net, "eat_ok_stdmode1_hunger_recalc", new Ctx(),
            new List<Spec> { S(1, 2013) }, 1, "food_recalc",
            BaseAbil(hp: 50, mp: 40, maxHp: 100, maxMp: 80), 0, 0, false));
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

        var templateDtos = new Dictionary<string, object>();
        foreach (var kv in table.Items)
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
            "src/OpenMir2/Grobal2.cs",
        };
        string sourceAnchor = SourceAnchorSha256(oracleSourceFiles);
        string asmPath = typeof(M2Share).Assembly.Location;
        string asmSha = Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(asmPath))).ToLowerInvariant();

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
                "StdMode 2（无效果）已覆盖；StdMode 31 的 `UseStdModeFunItem`（AniCount!=0，只看 `SystemShare.FunctionNPC` 是否存在）与 AniCount==0 的容量不足分支已覆盖；StdMode 3/4 与 StdMode 31 的解包分支（`UnbindList`，容量足够时解包出 6 件）本片未实现，已登记（T-17）",
                "Envir.Flag.boNODRUG 分支（地图禁药）已覆盖：SysMsg(Red) 入 MsgQueue（RM_SYSMESSAGE=11004，颜色取自 Config.RedMsgFColor/RedMsgBColor），物品不消耗、背包不动、落 SM_EAT_FAIL",
            },
            templates = templateDtos,
            cases,
        };

        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outPath))!);
        File.WriteAllText(outPath, JsonSerializer.Serialize(doc, new JsonSerializerOptions { WriteIndented = true }) + "\n", new UTF8Encoding(false));
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
