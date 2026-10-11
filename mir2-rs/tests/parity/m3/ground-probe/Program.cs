// ground-probe —— 用 C# oracle 真身实测「拾取（CM_PICKUP）/ 丢弃（CM_DROPITEM）」向量。
//
// 为什么要有这个探针：`bins/game-svr/src/ground.rs` 的接缝必须与 oracle **逐字段/逐字节相同**，
// 而这一对 handler 的守卫链（`ClientPickUpItem` 的 `Dealing`/`GetItem`/`FloorItemCanPickUpTime`/
// `IsSelf`/`IsOfGroup`/金币分支/`IsEnoughBag`/`IsAddWeightAvailable`；`ClientDropItem` 的
// `InSafeDisableDrop`/`IsCanDrop`/`NoThrowItem`/空格名/`DealLastTick`/`ControlDropItem`/`DropItemDown`）
// 靠读源码重推极易抄错。这里 `new PlayObject()` + 反射调**完整的消息分支**：
//   - `PlayObject.Message.cs` 的 `case CM_DROPITEM/CM_PICKUP`（经 `Operate(ProcessMessage)` 真身派发）；
//   - `PlayObject.Base.cs:55` `ClientPickUpItem`、`PlayObject.Operate.cs:251` `ClientDropItem`；
//   - `MapItem`/`Envirnoment`/`M2Share.CellObjectMgr` 都是**真身**（地图 `CellArray` 由反射灌入一张
//     全 Walk、空格子的 64×64 网格；物品表 = 内存替身）。
//
// 截获：
//   * **即时帧**（`SM_DROPITEM_SUCCESS=600`/`SM_DROPITEM_FAIL=601`、`SM_ADDITEM=200`）由替身
//     `INetChannel.AddGateBuffer` 截获（`SendDefMessage`→`SendSocket` 立即写 gate buffer）；
//   * **延时通知**（`RM_ITEMHIDE=8085`/`RM_WEIGHTCHANGED=10115`/`RM_GOLDCHANGED=10136`/
//     `RM_MENU_OK=10309`/`RM_SYSMESSAGE=11004`）由 `SendMsg` 入 `MsgQueue`，探针反射按**出队序**
//     读出（`PriorityQueue`，优先级 = `(byte)wIdent`，**低值先出**——它不等于入队序）；
//   * 地图格子前后状态（`Envir.GetItem` + 全图物品计数）。
//
// 产物 = mir2-rs/tests/parity/m3/fixtures/ground_vectors.json（Rust 侧用例据此逐字段核对）。
//
// 只读：不连 MySQL、不写任何现网文件（SystemShare.ItemSystem 换成内存表）、不发起任何 socket。
//
// 用法（**在仓库根运行**，源锚点用相对路径）：
//   dotnet run --project mir2-rs/tests/parity/m3/ground-probe -c Release -- \
//     mir2-rs/tests/parity/m3/fixtures/ground_vectors.json

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
using OpenMir2.NativeList.Utils;
using OpenMir2.Packets.ClientPackets;
using OpenMir2.Packets.ServerPackets;
using Serilog;
using SystemModule;
using SystemModule.Actors;
using SystemModule.Data;
using SystemModule.SubSystem;

namespace GroundProbe;

/// <summary>
/// `SystemShare.ItemSystem` 的内存替身：
/// - `GetStdItem(ushort)` 返回注入的模板；
/// - `GetUpgradeStdItem` = **恒等拷贝**（有效物品 = 模板；Desc 升级不在本片作用域）；
/// - `GetStdItemName` / `GetStdItemWeight` 走模板。
/// </summary>
internal sealed class ProbeItemSystem : IItemSystem
{
    public readonly Dictionary<ushort, StdItem> Items = new();

    public StdItem GetStdItem(ushort nItemIdx)
        => Items.TryGetValue(nItemIdx, out var it) ? it : null;

    public string GetStdItemName(int itemIdx)
        => Items.TryGetValue((ushort)itemIdx, out var it) ? it.Name : string.Empty;

    public int GetStdItemWeight(int nItemIdx)
        => Items.TryGetValue((ushort)nItemIdx, out var it) ? it.Weight : 0;

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
/// `SystemShare.ManageNPC` 的最小替身。drop 守卫的 `SendMsg(SystemShare.ManageNPC, RM_MENU_OK, …, ActorId, …)`
/// 只有在 `baseObject.ActorId` 上取值，探针只需要它非 null。
/// </summary>
internal class NpcProxy : DispatchProxy
{
    public static INormNpc Create() => DispatchProxy.Create<INormNpc, NpcProxy>();

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

    private sealed class GroundSpec
    {
        public int ItemId;
        public string Name;
        public ushort Looks;
        public int Count;
        public int OfBaseObject;
        public int CanPickUpTickDelta; // 相对 now 的偏移（now + delta）
        public Spec UserItem;          // 金币为 null
    }

    private sealed class Ctx
    {
        public int actorId = 0x1234;
        public short currX = 32;
        public short currY = 32;
        public int groupOwner = 0;
        public int[] groupMemberActorIds = Array.Empty<int>();
        public bool dealing = false;
        public bool isCanDrop = true;
        // 相对 now 的偏移：真实 `DealLastTick = now + delta`（oracle 内部按真实 tick 判 `(tick-DealLastTick)>3000`）。
        // 默认 -1_000_000 ⇒ 差值远超 3000 ⇒ 扫描执行；`drop_fail_deal_tick_guard` 用 -1000 ⇒ 守卫跳过。
        public int dealLastTickDelta = -1000000;
        public bool inSafeDisableDrop = false;
        public bool safeArea = false;
        public bool noThrowItem = false;
        public bool controlDropItem = false;
        public int canDropPrice = 500;
        public int humanMaxGold = 10000000;
        public int floorItemCanPickUpTime = 2 * 60 * 1000;
        public byte dropItemRage = 3;
    }

    private static UserItem ToUser(Spec s)
        => s == null ? null : new UserItem { MakeIndex = s.MakeIndex, Index = s.Index, Dura = s.Dura, DuraMax = s.DuraMax };

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

    private static Ability BaseAbil() => new Ability
    {
        Level = 1,
        Reserved1 = 0,
        AC = 0,
        MAC = 0,
        DC = 0,
        MC = 0,
        SC = 0,
        HP = 50,
        MP = 40,
        MaxHP = 100,
        MaxMP = 80,
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

    /// <summary>
    /// 给真身 `Envirnoment` 灌一张 `w×h` 的**全 Walk、空格子**地图：`CellArray` 是私有属性，
    /// 用反射写 backing property；`MapCellInfo.ObjList` 必须非 null（`GetItemEx`/`AddItemToMap` 会读）。
    /// </summary>
    private static void SetupMap(Envirnoment env, short w, short h)
    {
        env.Width = w;
        env.Height = h;
        var arr = new MapCellInfo[w * h];
        for (int i = 0; i < arr.Length; i++)
        {
            arr[i] = new MapCellInfo { Attribute = CellAttribute.Walk, ObjList = new NativeList<CellObject>() };
        }
        var prop = typeof(Envirnoment).GetProperty("CellArray", BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingMemberException("Envirnoment.CellArray");
        prop.SetValue(env, arr);
    }

    private static int CountMapItems(Envirnoment env)
    {
        var prop = typeof(Envirnoment).GetProperty("CellArray", BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingMemberException("Envirnoment.CellArray");
        var arr = (MapCellInfo[])prop.GetValue(env);
        int total = 0;
        foreach (var cell in arr)
        {
            if (cell.ObjList != null)
            {
                total += cell.ObjList.Count;
            }
        }
        return total;
    }

    private static object GroundDto(MapItem m, bool present, int tickShift)
    {
        if (!present)
        {
            return null;
        }
        return new
        {
            item_id = m.ItemId,
            name = m.Name,
            looks = (int)m.Looks,
            ani_count = (int)m.AniCount,
            reserved = m.Reserved,
            count = m.Count,
            drop_base_object = m.DropBaseObject,
            of_base_object = m.OfBaseObject,
            can_pick_up_tick = m.CanPickUpTick + tickShift, // 平移到固定基点（见 RunCase 的 tickShift）
            user_item = ItemDto(m.UserItem),
        };
    }

    private static object RunCase(ProbeItemSystem table, CaptureNetChannel net, string name, string action,
        Ctx ctx, GroundSpec ground, List<Spec> itemList, Spec request, string requestName, int goldBefore)
    {
        net.Frames.Clear();

        SystemShare.Config.HumanMaxGold = ctx.humanMaxGold;
        SystemShare.Config.FloorItemCanPickUpTime = ctx.floorItemCanPickUpTime;
        SystemShare.Config.ControlDropItem = ctx.controlDropItem;
        SystemShare.Config.CanDropPrice = ctx.canDropPrice;
        SystemShare.Config.InSafeDisableDrop = ctx.inSafeDisableDrop;
        SystemShare.Config.DropItemRage = ctx.dropItemRage;
        SystemShare.Config.RedHomeMap = "probe";
        SystemShare.Config.RedHomeX = ctx.currX;
        SystemShare.Config.RedHomeY = ctx.currY;
        SystemShare.Config.SafeZoneSize = 10;

        int now = HUtil32.GetTickCount();
        // 夹具必须**逐字节可复现**：`HUtil32.GetTickCount()` 是「开机以来毫秒」，每次运行都不同。
        // 这里把「记录进 JSON 的 tick」整体平移到固定基点（真实 tick 仍用于驱动 oracle 判定），
        // 于是 `now_tick`/`can_pick_up_tick`/`deal_last_tick` 全是常量基点上的相对值，重跑 deep-equal。
        const int FixedBaseTick = 1_200_000;
        int tickShift = FixedBaseTick - now;

        var env = new Envirnoment { MapName = "probe", MapFileName = "probe" };
        SetupMap(env, 64, 64);
        env.Flag.SafeArea = ctx.safeArea;
        env.Flag.NoThrowItem = ctx.noThrowItem;

        if (ground != null)
        {
            var seed = new MapItem
            {
                ItemId = ground.ItemId,
                Name = ground.Name,
                Looks = ground.Looks,
                Count = ground.Count,
                OfBaseObject = ground.OfBaseObject,
                CanPickUpTick = now + ground.CanPickUpTickDelta,
                AniCount = 0,
                Reserved = 0,
                DropBaseObject = 0,
                UserItem = ToUser(ground.UserItem),
            };
            if (!env.AddItemToMap(ctx.currX, ctx.currY, seed))
            {
                throw new InvalidOperationException($"seed ground failed: {name}");
            }
        }

        if (ctx.groupOwner != 0)
        {
            // `ClientPickUpItemIsOfGroup` 会 `SystemShare.ActorMgr.Get(GroupOwner)` 取团长，再遍历其
            // `GroupMembers`。构造一个只承担该查询的团长演员，并显式注册到 ActorMgr（ActorId 在构造后
            // 才赋值，故必须补一次 Add 才能用新 ActorId 取到）。
            var groupOwnerPlay = new PlayObject { ActorId = ctx.groupOwner, GroupMembers = new List<IPlayerActor>() };
            foreach (var mid in ctx.groupMemberActorIds)
            {
                groupOwnerPlay.GroupMembers.Add(new PlayObject { ActorId = mid });
            }
            SystemShare.ActorMgr.Add(groupOwnerPlay);
        }

        var wabil = BaseAbil();
        var po = new PlayObject
        {
            Race = (byte)ActorRace.Play,
            ActorId = ctx.actorId,
            Envir = env,
            CurrX = ctx.currX,
            CurrY = ctx.currY,
            WAbil = wabil,
            ItemList = new List<UserItem>(),
            MagicList = new List<UserMagic>(),
            Gold = goldBefore,
            Dealing = ctx.dealing,
            IsCanDrop = ctx.isCanDrop,
            DealLastTick = now + ctx.dealLastTickDelta,
            GroupOwner = ctx.groupOwner,
            UserAccount = "probe",
            LoginIpAddr = "127.0.0.1",
            IsRobot = false,
            OffLineFlag = false,
            // PlayObject 构造器把 FixedHideMode 置 true（`PlayObject.cs:993`），那是「还没进世界」的
            // 中间态；生产里进世界后会复位（`:1442`）。探针显式复位，才能让 `SendRefMsg` 走
            // 「广播给附近玩家」这一条真实路径（否则它会走「只发自己」的隐身分支，凭空给自身排一条
            // RM_ITEMHIDE —— 那是探针夹具的伪影，不是生产语义）。
            FixedHideMode = false,
            ObMode = false,
        };
        foreach (var s in itemList)
        {
            po.ItemList.Add(ToUser(s));
        }

        MapItem before = default;
        bool groundBefore = env.GetItem(ctx.currX, ctx.currY, ref before);
        var beforeList = po.ItemList.Select(ItemDto).ToArray();
        var wabilBefore = AbilityDto(wabil);

        int ident = action == "pick_up" ? Messages.CM_PICKUP : Messages.CM_DROPITEM;
        var operate = typeof(PlayObject).GetMethod("Operate", BindingFlags.Instance | BindingFlags.NonPublic, null,
            new[] { typeof(ProcessMessage) }, null)
            ?? throw new MissingMethodException("PlayObject.Operate(ProcessMessage)");
        var msg = new ProcessMessage
        {
            wIdent = ident,
            wParam = 0,
            nParam1 = request.MakeIndex,
            nParam2 = ctx.currX,
            nParam3 = ctx.currY,
            ActorId = 0,
            LateDelivery = false,
            Msg = requestName,
        };
        operate.Invoke(po, new object[] { msg });

        var messages = Drain(GetMsgQueue(po));
        var immediate = net.Frames.ToList();

        MapItem after = default;
        bool groundAfter = env.GetItem(ctx.currX, ctx.currY, ref after);

        return new
        {
            name,
            action,
            ctx = new
            {
                actor_id = ctx.actorId,
                curr_x = (int)ctx.currX,
                curr_y = (int)ctx.currY,
                group_owner = ctx.groupOwner,
                group_member_actor_ids = ctx.groupMemberActorIds,
                dealing = ctx.dealing,
                is_can_drop = ctx.isCanDrop,
                deal_last_tick = FixedBaseTick + ctx.dealLastTickDelta,
                in_safe_disable_drop = ctx.inSafeDisableDrop,
                safe_area = ctx.safeArea,
                no_throw_item = ctx.noThrowItem,
                control_drop_item = ctx.controlDropItem,
                can_drop_price = ctx.canDropPrice,
                human_max_gold = ctx.humanMaxGold,
                floor_item_can_pick_up_time = ctx.floorItemCanPickUpTime,
                drop_item_rage = (int)ctx.dropItemRage,
                red_msg_f_color = (int)SystemShare.Config.RedMsgFColor,
                red_msg_b_color = (int)SystemShare.Config.RedMsgBColor,
            },
            now_tick = FixedBaseTick,
            ground_before = GroundDto(before, groundBefore, tickShift),
            item_list_before = beforeList,
            gold_before = goldBefore,
            wabil_before = wabilBefore,
            request = new { make_index = request.MakeIndex, name = requestName },
            expect = new
            {
                immediate_frames = immediate
                    .Select(f => new { ident = f.ident, recog = f.recog, param = f.param, tag = f.tag, series = f.series })
                    .ToList(),
                notices = messages,
                item_list = po.ItemList.Select(ItemDto).ToArray(),
                gold = po.Gold,
                wabil = AbilityDto(po.WAbil),
                ground_after = GroundDto(after, groundAfter, tickShift),
                map_item_count = CountMapItems(env),
            },
        };
    }

    private static StdItem Tpl(string name, byte stdMode, byte shape, byte weight, ushort ac = 0, ushort mac = 0,
        ushort duraMax = 0, int price = 100, ushort looks = 0, byte aniCount = 0)
        => new StdItem
        {
            Name = name,
            StdMode = stdMode,
            Shape = shape,
            Weight = weight,
            AC = ac,
            MAC = mac,
            DuraMax = duraMax,
            Price = price,
            Looks = looks,
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
            : Path.Combine(AppContext.BaseDirectory, "..", "..", "..", "..", "..", "fixtures", "ground_vectors.json");

        var table = new ProbeItemSystem();
        SystemShare.ItemSystem = table;
        var net = new CaptureNetChannel();
        M2Share.NetChannel = net;
        SystemShare.ManageNPC = NpcProxy.Create();

        // 模板：只覆盖本片作用域读到的字段（StdMode/Shape/Weight/Price/Looks/AniCount/Name）。
        table.Items[3001] = Tpl("ring", 23, 0, 3, price: 100, looks: 5000);          // 普通可拾取物（MakeIndex 主用）
        table.Items[3002] = Tpl("heavy_ring", 23, 0, 3, price: 100, looks: 5001);    // 同形，另一件
        table.Items[3003] = Tpl("expensive", 23, 0, 3, price: 900, looks: 5002);     // ControlDropItem 下不够便宜 ⇒ 仍走 DropItemDown
        table.Items[3004] = Tpl("cheap", 23, 0, 3, price: 10, looks: 5003);          // ControlDropItem 下便宜 ⇒ 直接移除
        table.Items[3006] = Tpl("very_heavy", 23, 0, 200, price: 100, looks: 5004);  // 超过 MaxWeight(50) ⇒ 拾取时负重不足
        table.Items[3007] = Tpl("durable_ring", 40, 0, 3, duraMax: 3000, price: 100, looks: 5005); // StdMode 40 ⇒ DropItemDown 耐久 -2000（u16 回绕）
        // 3005 故意不注册 ⇒ 模板缺失分支

        var cases = new List<object>();

        // ---------- 拾取 ----------
        // 1) 空地/无主物品：成功入包 + SM_ADDITEM + RM_ITEMHIDE + RM_WEIGHTCHANGED（出队序 RM_WEIGHTCHANGED 先）
        cases.Add(RunCase(table, net, "pickup_ok_item", "pick_up", new Ctx(),
            new GroundSpec { ItemId = 70001, Name = "ring", Looks = 5000, Count = 1, OfBaseObject = 0, CanPickUpTickDelta = 0, UserItem = S(41, 3001) },
            new List<Spec>(), S(0, 0), "", 1000));
        // 2) 自己的掉落物（OfBaseObject == ActorId）
        cases.Add(RunCase(table, net, "pickup_ok_self_owned", "pick_up", new Ctx(),
            new GroundSpec { ItemId = 70002, Name = "ring", Looks = 5000, Count = 1, OfBaseObject = 0x1234, CanPickUpTickDelta = 0, UserItem = S(42, 3001) },
            new List<Spec>(), S(0, 0), "", 1000));
        // 3) 超时后归属被清 0（tick - CanPickUpTick > FloorItemCanPickUpTime ⇒ OfBaseObject=0）
        cases.Add(RunCase(table, net, "pickup_ok_owner_reset_by_timeout", "pick_up", new Ctx(),
            new GroundSpec { ItemId = 70003, Name = "ring", Looks = 5000, Count = 1, OfBaseObject = 999, CanPickUpTickDelta = -(200 * 1000), UserItem = S(43, 3001) },
            new List<Spec>(), S(0, 0), "", 1000));
        // 4) 别人的掉落物且未超时 ⇒ CanotPickUpItem（RM_SYSMESSAGE）+ 不动地面/背包
        cases.Add(RunCase(table, net, "pickup_fail_owner_mismatch", "pick_up", new Ctx(),
            new GroundSpec { ItemId = 70004, Name = "ring", Looks = 5000, Count = 1, OfBaseObject = 999, CanPickUpTickDelta = 0, UserItem = S(44, 3001) },
            new List<Spec>(), S(0, 0), "", 1000));
        // 4b) 别人的掉落物但属于同组 ⇒ IsOfGroup 放行
        cases.Add(RunCase(table, net, "pickup_ok_group_member", "pick_up",
            new Ctx { groupOwner = 777, groupMemberActorIds = new[] { 888 } },
            new GroundSpec { ItemId = 70011, Name = "ring", Looks = 5000, Count = 1, OfBaseObject = 888, CanPickUpTickDelta = 0, UserItem = S(49, 3001) },
            new List<Spec>(), S(0, 0), "", 1000));
        // 5) Dealing ⇒ 直接 return（无帧无通知）
        cases.Add(RunCase(table, net, "pickup_fail_dealing", "pick_up", new Ctx { dealing = true },
            new GroundSpec { ItemId = 70005, Name = "ring", Looks = 5000, Count = 1, OfBaseObject = 0, CanPickUpTickDelta = 0, UserItem = S(45, 3001) },
            new List<Spec>(), S(0, 0), "", 1000));
        // 6) 背包满（46 件）⇒ IsEnoughBag false ⇒ 什么都不做，地面不动
        cases.Add(RunCase(table, net, "pickup_fail_bag_full", "pick_up", new Ctx(),
            new GroundSpec { ItemId = 70006, Name = "ring", Looks = 5000, Count = 1, OfBaseObject = 0, CanPickUpTickDelta = 0, UserItem = S(46, 3001) },
            Enumerable.Range(0, 46).Select(i => S(100 + i, 3001)).ToList(), S(0, 0), "", 1000));
        // 7) 超重（IsAddWeightAvailable false）⇒ DeleteFromMap 成功后又 AddItemToMap 放回，地面保留、背包不变
        cases.Add(RunCase(table, net, "pickup_fail_weight", "pick_up", new Ctx(),
            new GroundSpec { ItemId = 70007, Name = "very_heavy", Looks = 5004, Count = 1, OfBaseObject = 0, CanPickUpTickDelta = 0, UserItem = S(47, 3006) },
            new List<Spec>(), S(0, 0), "", 1000));
        // 8) 模板缺失 ⇒ `stdItem == null` 短路 ⇒ AddItemToMap 放回
        cases.Add(RunCase(table, net, "pickup_fail_template_missing", "pick_up", new Ctx(),
            new GroundSpec { ItemId = 70008, Name = "missing", Looks = 5000, Count = 1, OfBaseObject = 0, CanPickUpTickDelta = 0, UserItem = S(48, 3005) },
            new List<Spec>(), S(0, 0), "", 1000));
        // 9) 金币成功：DeleteFromMap + IncGold + RM_GOLDCHANGED（SendRefMsg 对自身不投递，无 RM_ITEMHIDE）
        cases.Add(RunCase(table, net, "pickup_gold_ok", "pick_up", new Ctx(),
            new GroundSpec { ItemId = 70009, Name = Grobal2.StringGoldName, Looks = 110, Count = 500, OfBaseObject = 0, CanPickUpTickDelta = 0, UserItem = null },
            new List<Spec>(), S(0, 0), "", 1000));
        // 10) 金币超上限（IncGold false）⇒ AddItemToMap 放回、金币不变、无通知
        cases.Add(RunCase(table, net, "pickup_gold_fail_max", "pick_up", new Ctx { humanMaxGold = 1200 },
            new GroundSpec { ItemId = 70010, Name = Grobal2.StringGoldName, Looks = 110, Count = 500, OfBaseObject = 0, CanPickUpTickDelta = 0, UserItem = null },
            new List<Spec>(), S(0, 0), "", 1000));
        // 11) 空格子 ⇒ GetItem false ⇒ 直接 return
        cases.Add(RunCase(table, net, "pickup_fail_no_ground", "pick_up", new Ctx(),
            null, new List<Spec>(), S(0, 0), "", 1000));

        // ---------- 丢弃 ----------
        // 12) 正常丢弃：oracle 从不给 MapItem.ItemId 赋值（=0）⇒ AddItemToMap 首行即 false ⇒ SM_DROPITEM_FAIL、背包不变
        cases.Add(RunCase(table, net, "drop_fail_item_id_zero", "drop_item", new Ctx(),
            null, new List<Spec> { S(51, 3001) }, S(51, 0), "ring", 1000));
        // 13) ControlDropItem + 便宜 ⇒ 不进地图、直接移除 + SM_DROPITEM_SUCCESS
        cases.Add(RunCase(table, net, "drop_ok_control_price", "drop_item", new Ctx { controlDropItem = true, canDropPrice = 500 },
            null, new List<Spec> { S(52, 3004), S(53, 3001) }, S(52, 0), "cheap", 1000));
        // 14) ControlDropItem + 不便宜（price >= CanDropPrice）⇒ 仍走 DropItemDown ⇒ FAIL、背包不变
        cases.Add(RunCase(table, net, "drop_fail_control_price_too_high", "drop_item", new Ctx { controlDropItem = true, canDropPrice = 500 },
            null, new List<Spec> { S(54, 3003) }, S(54, 0), "expensive", 1000));
        // 15) 安全区禁丢（InSafeDisableDrop + InSafeZone）⇒ RM_MENU_OK + FAIL
        cases.Add(RunCase(table, net, "drop_fail_safe_zone", "drop_item", new Ctx { inSafeDisableDrop = true, safeArea = true },
            null, new List<Spec> { S(55, 3001) }, S(55, 0), "ring", 1000));
        // 16) 地图 NoThrowItem ⇒ RM_MENU_OK + FAIL
        cases.Add(RunCase(table, net, "drop_fail_no_throw_item", "drop_item", new Ctx { noThrowItem = true },
            null, new List<Spec> { S(56, 3001) }, S(56, 0), "ring", 1000));
        // 17) IsCanDrop=false ⇒ RM_MENU_OK + FAIL
        cases.Add(RunCase(table, net, "drop_fail_is_can_drop_false", "drop_item", new Ctx { isCanDrop = false },
            null, new List<Spec> { S(57, 3001) }, S(57, 0), "ring", 1000));
        // 18) DealLastTick 太近（(now-DealLastTick)=1000 <= 3000）⇒ 整个扫描被跳过 ⇒ FAIL、背包不变。
        //     刻意用 ControlDropItem + 便宜物：若没有这条守卫，本用例会走「直接移除 + SUCCESS」⇒ 改坏守卫必红。
        cases.Add(RunCase(table, net, "drop_fail_deal_tick_guard", "drop_item",
            new Ctx { controlDropItem = true, canDropPrice = 500, dealLastTickDelta = -1000 },
            null, new List<Spec> { S(58, 3004) }, S(58, 0), "cheap", 1000));
        // 19) 名称带空格（GetValidStr3 折分）⇒ 折分后命中 ⇒ 仍因 ItemId=0 FAIL
        cases.Add(RunCase(table, net, "drop_fail_name_with_space", "drop_item", new Ctx(),
            null, new List<Spec> { S(59, 3001) }, S(59, 0), "ring #3", 1000));
        // 20) MakeIndex 不命中 ⇒ FAIL、背包不变
        cases.Add(RunCase(table, net, "drop_fail_makeindex_mismatch", "drop_item", new Ctx(),
            null, new List<Spec> { S(60, 3001) }, S(99, 0), "ring", 1000));
        // 21) StdMode 40 且 Dura >= 2000 ⇒ `(ushort)(Dura-2000)` 正常扣减（2500 ⇒ 500）
        cases.Add(RunCase(table, net, "drop_stdmode40_dura_normal", "drop_item", new Ctx(),
            null, new List<Spec> { S(61, 3007, dura: 2500, duraMax: 3000) }, S(61, 0), "durable_ring", 1000));
        // 22) StdMode 40 且 Dura < 2000 ⇒ `(ushort)(Dura-2000)` **按 u16 回绕**（10 ⇒ 63546），
        //     不是 clamp 到 0：`idura <= 0` 对 ushort 恒 false（`BaseObject.cs:541-548` 的死代码）。
        //     改回 `max(0, Dura-2000)` ⇒ 本用例必红。
        cases.Add(RunCase(table, net, "drop_stdmode40_dura_underflow", "drop_item", new Ctx(),
            null, new List<Spec> { S(62, 3007, dura: 10, duraMax: 3000) }, S(62, 0), "durable_ring", 1000));

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
                price = t.Price,
                looks = (int)t.Looks,
                ani_count = (int)t.AniCount,
                dura_max = t.DuraMax,
            };
        }

        var oracleSourceFiles = new[]
        {
            "src/M2Server/Player/PlayObject.Base.cs",
            "src/M2Server/Player/PlayObject.Operate.cs",
            "src/M2Server/Player/PlayObject.Message.cs",
            "src/M2Server/Player/PlayObject.Socket.cs",
            "src/M2Server/Actor/BaseObject.cs",
            "src/M2Server/Actor/BaseObject.Message.cs",
            "src/M2Server/Maps/Envirnoment.cs",
            "src/Modules/SystemModule/Data/MapItem.cs",
        };
        string sourceAnchor = SourceAnchorSha256(oracleSourceFiles);
        string asmPath = typeof(M2Share).Assembly.Location;
        string asmSha = Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(asmPath))).ToLowerInvariant();

        var doc = new
        {
            generator = "mir2-rs/tests/parity/m3/ground-probe",
            oracle_assembly = Path.GetFileName(asmPath),
            oracle_assembly_sha256 = asmSha,
            oracle_assembly_sha256_note = "生成时 M2Server.dll 构建产物 hash（随编译器/SDK/环境变化）；仅作溯源，不参与向量校验——校验锚点是 oracle_source_sha256",
            oracle_source_sha256 = sourceAnchor,
            oracle_source_sha256_files = oracleSourceFiles,
            oracle_source = new[]
            {
                "src/M2Server/Player/PlayObject.Message.cs:801 (CM_DROPITEM -> ClientDropItem -> SM_DROPITEM_SUCCESS/FAIL)",
                "src/M2Server/Player/PlayObject.Message.cs:811 (CM_PICKUP -> ClientPickUpItem; 需 CurrX==nParam2 && CurrY==nParam3)",
                "src/M2Server/Player/PlayObject.Base.cs:55 (ClientPickUpItem)",
                "src/M2Server/Player/PlayObject.Operate.cs:251 (ClientDropItem)",
                "src/M2Server/Player/PlayObject.Base.cs:24/38/168/183/193/198 (IsSelf/IsOfGroup/IncGold/IsEnoughBag/IsAddWeightAvailable/SendAddItem)",
                "src/M2Server/Actor/BaseObject.cs:529 (DropItemDown; :541-548 StdMode 40 耐久 u16 回绕) / :470 (GetDropPosition) / :1241 (AddItemToBag) / :1356 (RecalcBagWeight) / :1771 (WeightChanged)",
                "src/M2Server/Maps/Envirnoment.cs:88 (AddItemToMap) / :635 (DeleteFromMap) / :705 (GetItem) / :723 (GetItemEx)",
                "src/Modules/SystemModule/Data/MapItem.cs (MapItem.ItemId 从无赋值点 ⇒ 恒 0)",
            },
            notes = new[]
            {
                "即时帧 = SendDefMessage/SendSocket 直接写 gate buffer（SM_DROPITEM_SUCCESS=600 / SM_DROPITEM_FAIL=601 / SM_ADDITEM=200）",
                "延时通知 = SendMsg 入 MsgQueue（PriorityQueue，优先级 = (byte)wIdent，低值先出）；探针按出队序读，Rust 侧亦按出队序产出",
                "**oracle 现状（实测）**：全仓无任何 `MapItem.ItemId = …` 赋值点 ⇒ `DropItemDown` 生成的 MapItem.ItemId 恒 0 ⇒ `Envir.AddItemToMap` 首行 `if (mapItem.ItemId == 0) return false;` 恒命中 ⇒ 正常 `CM_DROPITEM` 恒 `SM_DROPITEM_FAIL`（背包不动）。这是 oracle 的既有行为，Rust 1:1 复刻（不是本片引入的差异）",
                "拾取的金币分支 `SendRefMsg(RM_ITEMHIDE, …)` 只广播给**附近玩家**（不含自己）⇒ 探针里自身 MsgQueue 只看到 `RM_GOLDCHANGED`",
                "`pickup_ok_group_member` 覆盖 `ClientPickUpItemIsOfGroup` 放行：探针把 GroupOwner 注册进 ActorMgr（ActorId 在构造后才赋值，需补 Add），其 `GroupMembers` 含目标 OfBaseObject",
                "夹具记录的所有 tick（now_tick/can_pick_up_tick/deal_last_tick）都平移到固定基点 `1_200_000`；真实 tick 只用于驱动 oracle 判定 ⇒ 重跑探针产物逐字节相同",
                "`drop_fail_deal_tick_guard` 用 `ControlDropItem + 便宜物 + DealLastTick 距今 1000ms`：守卫跳过 ⇒ FAIL；若守卫被改坏/删掉 ⇒ 会变成 SUCCESS，用例必红",
                "**StdMode 40 的耐久扣减是 u16 回绕**（`BaseObject.cs:541-548`：`ushort idura = (ushort)(Dura-2000)`；后面的 `if (idura <= 0) idura = 0;` 对 ushort 恒 false ⇒ 死代码）：Dura=2500⇒500（`drop_stdmode40_dura_normal`），Dura=10⇒63546（`drop_stdmode40_dura_underflow`）。该变更在丢弃失败时也留在 ItemList（`ClientDropItem` 与 `DropItemDown` 共享同一 UserItem 引用）⇒ 可由夹具观测；改回 `max(0, Dura-2000)` ⇒ underflow 用例必红",
                "`DropItemDown` 的 `itemOfCreat` 传 0（`PlayObject.Operate.cs:251`：`DropItemDown(userItem, dropWide, false, 0, ActorId)`）⇒ `MapItem.OfBaseObject=0`、`DropBaseObject=ActorId`；因 T-19（ItemId 恒 0、AddItemToMap 恒 false）当前不可观测",
                "本片未覆盖/未建模：`DropItemDown` 的 StdMode 45 随机外观（`GetRandomLook`，Rust 未建模）、`GetDropPosition` 的散落选格在「地图格子确有占位」时的真实分布（本片地图恒空 ⇒ 恒落当前格）、`SendRefMsg(RM_ITEMSHOW/RM_ITEMHIDE)` 对附近玩家的**广播**（替身 net 只看自身即时帧 + 自身 MsgQueue）——承接见 whitelist T-15/T-19",
                "有效物品 = 模板（探针 GetUpgradeStdItem 是恒等拷贝；Desc 升级不在本片作用域）",
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
