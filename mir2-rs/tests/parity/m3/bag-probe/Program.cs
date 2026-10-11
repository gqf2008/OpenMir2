// bag-probe —— 用 C# oracle 真身实测「装备（CM_TAKEONITEM）/ 卸下（CM_TAKEOFFITEM）」向量。
//
// 为什么要有这个探针：`bins/game-svr/src/bag.rs` 的接缝必须与 oracle **逐字段/逐字节相同**，
// 而 `ClientTakeOnItems`/`ClientTakeOffItems` 的守卫链（性别/腕力/负重/Need 分支/Desc[7]/
// ItemDesc&2&4/InDisableTakeOffList）与错误码（n18/n10 ∈ {0,-1,-2,-3,-4,1}）靠读源码重推极易抄错。
// 这里 `new PlayObject()` + **反射调私有 handler**，并用替身 `INetChannel` 截获真实出帧：
//   * 状态（UseItems/ItemList/重算后的 WAbil）来自运行中的 oracle 代码；
//   * 出帧（SM_TAKEON_OK/FAIL=615/616、SM_TAKEOFF_OK/FAIL=619/620、SM_ADDITEM=200、SM_UPDATEITEM=203）
//     来自 `PlayObject.SendDefMessage`/`SendSocket` 真正写进 `M2Share.NetChannel.AddGateBuffer` 的字节。
// 向量落 `mir2-rs/tests/parity/m3/fixtures/bag_vectors.json`，Rust 侧用例回读逐字段核对
//（改坏任一映射 ⇒ 用例必红）。
//
// 用法（**在仓库根运行**，源锚点用相对路径）：
//   dotnet run --project mir2-rs/tests/parity/m3/bag-probe -c Release -- \
//     mir2-rs/tests/parity/m3/fixtures/bag_vectors.json

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

namespace BagProbe;

/// <summary>
/// `SystemShare.ItemSystem` 的内存替身：
/// - `GetStdItem(ushort)` 返回注入的模板；
/// - `GetUpgradeStdItem` = **恒等拷贝**（有效物品 = 模板；Desc 升级不在本片作用域）；
/// - `GetStdItemName` 返回模板名（`CustomItemSystem.GetItemName` 的非自定义分支要它）。
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
        d.AniCount = stdItem.AniCount;
        d.SpecialPwr = stdItem.SpecialPwr;
        d.ItemDesc = stdItem.ItemDesc;
        d.Looks = stdItem.Looks;
        d.DuraMax = stdItem.DuraMax;
        d.AC = stdItem.AC;
        d.MAC = stdItem.MAC;
        d.DC = stdItem.DC;
        d.MC = stdItem.MC;
        d.SC = stdItem.SC;
        d.Need = stdItem.Need;
        d.NeedLevel = stdItem.NeedLevel;
        d.NeedIdentify = stdItem.NeedIdentify;
        d.Price = stdItem.Price;
        d.Stock = stdItem.Stock;
        d.AtkSpd = stdItem.AtkSpd;
        d.Agility = stdItem.Agility;
        d.Accurate = stdItem.Accurate;
        d.MgAvoid = stdItem.MgAvoid;
        d.Strong = stdItem.Strong;
        d.Undead = stdItem.Undead;
        d.HpAdd = stdItem.HpAdd;
        d.MpAdd = stdItem.MpAdd;
        d.ExpAdd = stdItem.ExpAdd;
        d.EffType1 = stdItem.EffType1;
        d.EffRate1 = stdItem.EffRate1;
        d.EffValue1 = stdItem.EffValue1;
        d.EffType2 = stdItem.EffType2;
        d.EffRate2 = stdItem.EffRate2;
        d.EffValue2 = stdItem.EffValue2;
        d.Slowdown = stdItem.Slowdown;
        d.Tox = stdItem.Tox;
        d.ToxAvoid = stdItem.ToxAvoid;
        d.UniqueItem = stdItem.UniqueItem;
        d.OverlapItem = stdItem.OverlapItem;
        d.Light = stdItem.Light;
        d.ItemType = stdItem.ItemType;
        d.ItemSet = stdItem.ItemSet;
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

/// <summary>截获到的一条帧（`CommandMessage` 在 `ServerMessage` 头之后）。</summary>
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
        // 线上格式：ServerMessage 头 20 字节 + CommandMessage 12 字节（`PlayObject.Socket.cs:77`）。
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

internal static class Program
{
    private const ushort DefHit = 5;
    private const ushort DefSpeed = 15;

    private static NakedAbility TickWarr() => new() { DC = 17, MC = 20, SC = 20, AC = 20, MAC = 20, HP = 1, MP = 3, Hit = 20, Speed = 35, Reserved = 0 };
    private static NakedAbility TickWizard() => new() { DC = 17, MC = 25, SC = 30, AC = 20, MAC = 15, HP = 2, MP = 1, Hit = 25, Speed = 35, Reserved = 0 };
    private static NakedAbility TickTaos() => new() { DC = 20, MC = 30, SC = 17, AC = 20, MAC = 15, HP = 2, MP = 1, Hit = 30, Speed = 30, Reserved = 0 };

    private static StdItem Tpl(string name, byte stdMode, byte shape, byte weight, byte itemDesc = 0,
        byte need = 0, byte needLevel = 0)
        => new StdItem
        {
            Name = name,
            StdMode = stdMode,
            Shape = shape,
            Weight = weight,
            ItemDesc = itemDesc,
            Need = need,
            NeedLevel = needLevel,
        };

    private static Ability BaseAbil() => new Ability
    {
        Level = 1,
        Reserved1 = 0,
        AC = 0,
        MAC = 0,
        DC = 0x0101,
        MC = 0,
        SC = 0,
        HP = 15,
        MP = 15,
        MaxHP = 19,
        MaxMP = 15,
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

    private static NakedAbility ZeroBonus() => new() { DC = 0, MC = 0, SC = 0, AC = 0, MAC = 0, HP = 0, MP = 0, Hit = 0, Speed = 0, Reserved = 0 };

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

    private sealed class Spec
    {
        public int MakeIndex;
        public ushort Index;
        public ushort Dura;
        public ushort DuraMax;
        public byte[] Desc = new byte[14];
    }

    private sealed class Ctx
    {
        public int job = 0;
        public int gender = 0;
        public int hair = 1;
        public int abilLevel = 1;
        public int reLevel = 0;
        public int creditPoint = 0;
        public int memberType = 0;
        public int memberLevel = 0;
        public bool hasGuild = false;
        public int guildRankNo = 0;
        public bool isCastleMember = false;
        public bool userUnlockDurg = false;
        public bool dealing = false;
        public List<int> disableKeys = new();
        public bool onHorse = false;
        public int horseType = 0;
        public int dressEffType = 0;
    }

    private static UserItem ToUser(Spec s)
    {
        var u = new UserItem { MakeIndex = s.MakeIndex, Index = s.Index, Dura = s.Dura, DuraMax = s.DuraMax };
        Array.Copy(s.Desc, u.Desc, 14);
        return u;
    }

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

    private static object RunCase(ProbeItemSystem table, CaptureNetChannel net, string name, string action,
        Ctx ctx, Spec[] useItems, List<Spec> itemList, Spec request, string requestName)
    {
        net.Frames.Clear();
        M2Share.DisableTakeOffList = new Dictionary<int, string>();
        foreach (var k in ctx.disableKeys)
        {
            M2Share.DisableTakeOffList[k] = "probe";
        }

        var abil = BaseAbil();
        var bonus = ZeroBonus();
        var po = new PlayObject
        {
            Job = (PlayerJob)ctx.job,
            Gender = (PlayerGender)ctx.gender,
            Hair = (byte)ctx.hair,
            Abil = abil,
            BonusAbil = bonus,
            UseItems = new UserItem[13],
            ItemList = new List<UserItem>(),
            MagicList = new List<UserMagic>(),
            ReLevel = (byte)ctx.reLevel,
            CreditPoint = (byte)ctx.creditPoint,
            MemberType = ctx.memberType,
            MemberLevel = (byte)ctx.memberLevel,
            GuildRankNo = (short)ctx.guildRankNo,
            UserUnLockDurg = ctx.userUnlockDurg,
            Dealing = ctx.dealing,
            UserAccount = "probe",
            LoginIpAddr = "127.0.0.1",
            IsRobot = false,
            OffLineFlag = false,
        };
        for (int i = 0; i < 13; i++)
        {
            po.UseItems[i] = useItems[i] == null ? null : ToUser(useItems[i]);
        }
        foreach (var s in itemList)
        {
            po.ItemList.Add(ToUser(s));
        }
        var wabilPre = BaseAbil();
        wabilPre.HP = abil.HP;
        wabilPre.MP = abil.MP;
        po.WAbil = wabilPre;

        string methodName = action == "take_on" ? "ClientTakeOnItems" : "ClientTakeOffItems";
        var mi = typeof(PlayObject).GetMethod(methodName, BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingMethodException($"PlayObject.{methodName}");
        // 输入快照必须在 invoke 之前取（handler 会原地改 UseItems/ItemList）。
        var beforeUse = po.UseItems.Select(ItemDto).ToArray();
        var beforeList = po.ItemList.Select(ItemDto).ToArray();
        mi.Invoke(po, new object[] { (byte)request.Index, request.MakeIndex, requestName });

        var frames = net.Frames.ToList();
        var modeled = frames.Where(f => f.ident == 615 || f.ident == 616 || f.ident == 619 || f.ident == 620)
            .Select(f => new { ident = f.ident, recog = f.recog, param = f.param, tag = f.tag, series = f.series })
            .ToList();
        int code = 0;
        if (action == "take_on")
        {
            // oracle：成功 `n18=1`（只发 OK）；失败 `n18 ∈ {0,-1,-4}`（发 FAIL，Recog=n18）。
            code = modeled.Any(f => f.ident == 615) ? 1 : (modeled.Count > 0 ? modeled[modeled.Count - 1].recog : 0);
        }
        else
        {
            // oracle：take-off 成功分支**不设** `n10=1`（`Operate.cs:540-556`）⇒ 落回 `FailExit`
            // 再发一条 `SM_TAKEOFF_FAIL(Recog=0)`；故最终 `n10` = 620 帧的 `Recog`。
            code = modeled.Where(f => f.ident == 620).Select(f => f.recog).LastOrDefault();
        }

        return new
        {
            name,
            action,
            ctx = new
            {
                job = ctx.job,
                gender = ctx.gender,
                hair = ctx.hair,
                abil_level = ctx.abilLevel,
                re_level = ctx.reLevel,
                credit_point = ctx.creditPoint,
                member_type = ctx.memberType,
                member_level = ctx.memberLevel,
                has_guild = ctx.hasGuild,
                guild_rank_no = ctx.guildRankNo,
                is_castle_member = ctx.isCastleMember,
                user_unlock_durg = ctx.userUnlockDurg,
                dealing = ctx.dealing,
                disable_take_off_keys = ctx.disableKeys,
                on_horse = ctx.onHorse,
                horse_type = ctx.horseType,
                dress_eff_type = ctx.dressEffType,
            },
            wabil = AbilityDto(wabilPre),
            abil = AbilityDto(abil),
            bonus_abil = new { dc = bonus.DC, mc = bonus.MC, sc = bonus.SC, ac = bonus.AC, mac = bonus.MAC, hp = bonus.HP, mp = bonus.MP, hit = bonus.Hit, speed = bonus.Speed, reserved = bonus.Reserved },
            use_items = beforeUse,
            item_list = beforeList,
            request = new { slot = (int)request.Index, make_index = request.MakeIndex, name = requestName },
            expect = new
            {
                code,
                frames = modeled,
                has_add_item = frames.Any(f => f.ident == 200),
                has_update_item = frames.Any(f => f.ident == 203),
                use_items = po.UseItems.Select(ItemDto).ToArray(),
                item_list = po.ItemList.Select(ItemDto).ToArray(),
                wabil = AbilityDto(po.WAbil),
            },
        };
    }

    private static byte[] Desc(int idx, byte value)
    {
        var d = new byte[14];
        d[idx] = value;
        return d;
    }

    private static Spec S(int makeIndex, ushort index, ushort dura = 10, ushort duraMax = 20, byte[] desc = null)
        => new() { MakeIndex = makeIndex, Index = index, Dura = dura, DuraMax = duraMax, Desc = desc ?? new byte[14] };

    private static Spec[] Empty() => new Spec[13];

    private static Spec[] With(Spec[] arr, int slot, Spec s)
    {
        arr[slot] = s;
        return arr;
    }

    public static int Main(string[] args)
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        LogService.Logger = new LoggerConfiguration().CreateLogger();

        string outPath = args.Length > 0
            ? args[0]
            : Path.Combine(AppContext.BaseDirectory, "..", "..", "..", "..", "..", "fixtures", "bag_vectors.json");

        var table = new ProbeItemSystem();
        SystemShare.ItemSystem = table;
        var net = new CaptureNetChannel();
        M2Share.NetChannel = net;
        M2Share.ItemBindAccount = new List<ItemBind>();
        M2Share.ItemBindIPaddr = new List<ItemBind>();
        M2Share.ItemBindChrName = new List<ItemBind>();
        SystemShare.Config.BonusAbilofWarr = TickWarr();
        SystemShare.Config.BonusAbilofWizard = TickWizard();
        SystemShare.Config.BonusAbilofTaos = TickTaos();

        // 模板：只覆盖本片作用域读到的字段（StdMode/Shape/Weight/ItemDesc/Need/NeedLevel/Name）。
        table.Items[1101] = Tpl("w", 5, 0, 10);          // 武器
        table.Items[1102] = Tpl("w2", 5, 0, 8);          // 另一把武器（换装）
        table.Items[1103] = Tpl("dress_m", 10, 0, 10);   // 男衣
        table.Items[1104] = Tpl("dress_f", 11, 0, 10);   // 女衣
        table.Items[1105] = Tpl("helmet", 15, 0, 8);     // 头盔（StdModeMap 命中，Desc[7] 守卫用）
        table.Items[1106] = Tpl("heavy", 5, 0, 200);     // 超腕力武器
        table.Items[1107] = Tpl("need50", 5, 0, 10, need: 0, needLevel: 50);
        table.Items[1109] = Tpl("itemdesc2", 5, 0, 10, itemDesc: 2);
        table.Items[1110] = Tpl("itemdesc4", 5, 0, 10, itemDesc: 4);
        table.Items[1111] = Tpl("disabled", 5, 0, 10);

        var cases = new List<object>();

        // ---- take_on ----
        cases.Add(RunCase(table, net, "takeon_ok_empty_weapon", "take_on", new Ctx(),
            Empty(), new List<Spec> { S(1, 1101) }, S(1, 1), "w"));
        cases.Add(RunCase(table, net, "takeon_ok_swap_weapon", "take_on", new Ctx(),
            With(Empty(), 1, S(9, 1102)), new List<Spec> { S(1, 1101) }, S(1, 1), "w"));
        cases.Add(RunCase(table, net, "takeon_fail_wrong_slot_mode", "take_on", new Ctx(),
            Empty(), new List<Spec> { S(1, 1103) }, S(1, 1), "dress_m"));
        cases.Add(RunCase(table, net, "takeon_fail_name_mismatch", "take_on", new Ctx(),
            Empty(), new List<Spec> { S(1, 1101) }, S(1, 1), "not-the-name"));
        cases.Add(RunCase(table, net, "takeon_fail_need_level", "take_on", new Ctx(),
            Empty(), new List<Spec> { S(1, 1107) }, S(1, 1), "need50"));
        cases.Add(RunCase(table, net, "takeon_fail_hand_weight", "take_on", new Ctx(),
            Empty(), new List<Spec> { S(1, 1106) }, S(1, 1), "heavy"));
        cases.Add(RunCase(table, net, "takeon_fail_occupied_desc7", "take_on", new Ctx(),
            With(Empty(), 4, S(9, 1105, desc: Desc(7, 1))), new List<Spec> { S(1, 1105) }, S(1, 4), "helmet"));
        cases.Add(RunCase(table, net, "takeon_fail_occupied_itemdesc2", "take_on", new Ctx(),
            With(Empty(), 1, S(9, 1109)), new List<Spec> { S(1, 1101) }, S(1, 1), "w"));
        cases.Add(RunCase(table, net, "takeon_fail_occupied_itemdesc4", "take_on", new Ctx(),
            With(Empty(), 1, S(9, 1110)), new List<Spec> { S(1, 1101) }, S(1, 1), "w"));
        cases.Add(RunCase(table, net, "takeon_fail_disable_takeoff", "take_on",
            new Ctx { disableKeys = new List<int> { 1111 - 1 } },
            With(Empty(), 1, S(9, 1111)), new List<Spec> { S(1, 1101) }, S(1, 1), "w"));
        cases.Add(RunCase(table, net, "takeon_fail_gender", "take_on", new Ctx { gender = 0 },
            Empty(), new List<Spec> { S(1, 1104) }, S(1, 0), "dress_f"));
        cases.Add(RunCase(table, net, "takeon_ok_dress", "take_on", new Ctx { gender = 0 },
            Empty(), new List<Spec> { S(1, 1103) }, S(1, 0), "dress_m"));
        cases.Add(RunCase(table, net, "takeon_fail_bad_slot_13", "take_on", new Ctx(),
            Empty(), new List<Spec> { S(1, 1101) }, S(1, 13), "w"));

        // ---- take_off ----
        cases.Add(RunCase(table, net, "takeoff_ok", "take_off", new Ctx(),
            With(Empty(), 1, S(9, 1101)), new List<Spec>(), S(9, 1), "w"));
        cases.Add(RunCase(table, net, "takeoff_fail_empty_slot", "take_off", new Ctx(),
            With(Empty(), 1, S(0, 0)), new List<Spec>(), S(9, 1), "w"));
        cases.Add(RunCase(table, net, "takeoff_fail_wrong_makeindex", "take_off", new Ctx(),
            With(Empty(), 1, S(9, 1101)), new List<Spec>(), S(7, 1), "w"));
        cases.Add(RunCase(table, net, "takeoff_fail_name_mismatch", "take_off", new Ctx(),
            With(Empty(), 1, S(9, 1101)), new List<Spec>(), S(9, 1), "other"));
        cases.Add(RunCase(table, net, "takeoff_fail_desc7", "take_off", new Ctx(),
            With(Empty(), 4, S(9, 1105, desc: Desc(7, 1))), new List<Spec>(), S(9, 4), "helmet"));
        cases.Add(RunCase(table, net, "takeoff_fail_bag_full", "take_off", new Ctx(),
            With(Empty(), 1, S(9, 1101)),
            Enumerable.Range(0, 46).Select(i => S(100 + i, 1101)).ToList(), S(9, 1), "w"));
        cases.Add(RunCase(table, net, "takeoff_ok_bag_45", "take_off", new Ctx(),
            With(Empty(), 1, S(9, 1101)),
            Enumerable.Range(0, 45).Select(i => S(100 + i, 1101)).ToList(), S(9, 1), "w"));
        cases.Add(RunCase(table, net, "takeoff_fail_dealing", "take_off", new Ctx { dealing = true },
            With(Empty(), 1, S(9, 1101)), new List<Spec>(), S(9, 1), "w"));

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
                item_desc = t.ItemDesc,
                need = t.Need,
                need_level = t.NeedLevel,
            };
        }

        var oracleSourceFiles = new[]
        {
            "src/M2Server/Player/PlayObject.Operate.cs",
            "src/M2Server/Player/PlayObject.Base.cs",
            "src/M2Server/Player/PlayObject.Socket.cs",
            "src/M2Server/Actor/BaseObject.cs",
            "src/M2Server/Items/CustomItemSystem.cs",
            "src/M2Server/M2Share.cs",
        };
        string sourceAnchor = SourceAnchorSha256(oracleSourceFiles);
        string asmPath = typeof(M2Share).Assembly.Location;
        string asmSha = Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(asmPath))).ToLowerInvariant();

        var doc = new
        {
            generator = "mir2-rs/tests/parity/m3/bag-probe",
            oracle_assembly = Path.GetFileName(asmPath),
            oracle_assembly_sha256 = asmSha,
            oracle_assembly_sha256_note = "生成时 M2Server.dll 构建产物 hash（随编译器/SDK/环境变化）；仅作溯源，不参与向量校验——校验锚点是 oracle_source_sha256",
            oracle_source_sha256 = sourceAnchor,
            oracle_source_sha256_files = oracleSourceFiles,
            oracle_source = new[]
            {
                "src/M2Server/Player/PlayObject.Operate.cs:377 (ClientTakeOnItems)",
                "src/M2Server/Player/PlayObject.Operate.cs:499 (ClientTakeOffItems)",
                "src/M2Server/Player/PlayObject.Base.cs:1730 (CheckTakeOnItems)",
                "src/M2Server/Player/PlayObject.Base.cs:2026 (GetUserItemWeitht)",
                "src/M2Server/Actor/BaseObject.cs:1241 (AddItemToBag)",
                "src/M2Server/M2Share.cs:376 (CheckUserItems)",
                "src/M2Server/M2Share.cs:1482 (InDisableTakeOffList)",
            },
            defaults = new
            {
                def_hit = DefHit,
                def_speed = DefSpeed,
                bonus_tick_warr = new { dc = 17, mc = 20, sc = 20, ac = 20, mac = 20, hp = 1, mp = 3, hit = 20, speed = 35, reserved = 0 },
                bonus_tick_wizard = new { dc = 17, mc = 25, sc = 30, ac = 20, mac = 15, hp = 2, mp = 1, hit = 25, speed = 35, reserved = 0 },
                bonus_tick_taos = new { dc = 20, mc = 30, sc = 17, ac = 20, mac = 15, hp = 2, mp = 1, hit = 30, speed = 30, reserved = 0 },
            },
            notes = new[]
            {
                "有效物品 = 模板（探针 GetUpgradeStdItem 是恒等拷贝；Desc 升级/StdMode 变换不在本片作用域）",
                "CheckItemBindUse 恒 true（ItemBindAccount/ItemBindIPaddr 两张表为空，两侧同域）",
                "InDisableTakeOffList 命中时 oracle 不设错误码 ⇒ FAIL 帧 Recog=0（不是 -4）",
                "出帧由替身 INetChannel.AddGateBuffer 截获（ServerMessage 20B 头 + CommandMessage 12B）",
                "StdItem.NeedLevel 是 byte ⇒ LoByte/HiByte 语义下 Hi 恒 0（10-13/40-44/81/82 的 Hi 比较恒真）",
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
