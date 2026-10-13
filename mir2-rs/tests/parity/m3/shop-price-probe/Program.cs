// shop-price-probe —— oracle 真身实测「商店定价 / 修理」向量。
//
// 为什么要有这个探针：`crates/formula/src/shop.rs` 的接缝必须与 oracle **逐字段相同**，而
// `GetItemPrice`/`GetUserItemPrice`/`GetUserPrice`/`GetSellItemPrice` + 修理成本/副作用里
// 有若干**照搬的缺陷**（`nC != 4 || nC != 9` 恒真、StdMode 43 改写 DuraMax、整数截断取整等），
// 靠读源码重推极易抄错。这里 `new Merchant()` + **反射调私有方法**拿到真身读数：
//   - GetItemPrice(ushort)              Merchant.cs:347
//   - GetUserItemPrice(UserItem)        Merchant.cs:1130
//   - GetUserPrice(IPlayerActor, double) Merchant.cs:765
//   - GetSellItemPrice(double)          Merchant.cs:1358（private static）
//   - ClientQueryRepairCost(IPlayerActor, UserItem) Merchant.cs:1555
//   - ClientRepairItem(IPlayerActor, UserItem)      Merchant.cs:1599
// `IPlayerActor`/`IUserCastle` 用 DispatchProxy 动态替身（只实现被调用到的成员），
// `SystemShare.ItemSystem` 换成内存表；`ClientRepairItem` 走的 `M2Share.ScriptEngine` 换成记录替身。
//
// 产物 = mir2-rs/tests/parity/m3/shop-price-probe/vectors/shop_price_vectors.{json,tsv}
// Rust 侧用例 `crates/formula/tests/shop_price_vectors.rs` 回读 TSV 逐字段核对
//（改坏任一分支 ⇒ 用例必红）。
//
// 用法（**在仓库根运行**）：
//   dotnet run --project mir2-rs/tests/parity/m3/shop-price-probe -c Release -- \
//     mir2-rs/tests/parity/m3/shop-price-probe/vectors/shop_price_vectors.json
//
// 只读：不连 MySQL、不写任何现网文件、不发起任何 socket、不起栈、不占端口。

using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Reflection;
using System.Text;
using System.Text.Json;
using GameSrv.Npc;
using M2Server;
using OpenMir2;
using OpenMir2.Data;
using OpenMir2.Packets.ClientPackets;
using Serilog;
using SystemModule;
using SystemModule.Actors;
using SystemModule.Castles;
using SystemModule.Const;
using SystemModule.Data;
using SystemModule.SubSystem;
using ScriptSystem;

namespace ShopPriceProbe;

/// <summary>`SystemShare.ItemSystem` 的内存替身：按 index 返回注入的模板。</summary>
internal sealed class ProbeItemSystem : IItemSystem
{
    public readonly Dictionary<ushort, StdItem> Items = new();

    public void AddItem(StdItem stdItem) { }
    public void Clear() => Items.Clear();
    public int ItemCount => Items.Count;
    public string GetStdItemName(int itemIdx)
        => Items.TryGetValue((ushort)itemIdx, out var it) ? it.Name : string.Empty;
    public ushort GetStdItemIdx(string itemName) => 0;
    public bool CopyToUserItemFromName(string sItemName, ref UserItem item) => false;
    public StdItem GetStdItem(string sItemName)
        => Items.TryGetValue(1, out var it) ? it : null;
    public StdItem GetStdItem(ushort nItemIdx)
        => Items.TryGetValue(nItemIdx, out var it) ? it : null;
    public int GetStdItemWeight(int nItemIdx) => 0;
    public int GetUpgrade2(int x, int a) => a;
    public int GetUpgradeStdItem(StdItem stdItem, UserItem userItem, ref ClientItem clientItem)
    {
        if (clientItem == null)
        {
            clientItem = new ClientItem();
        }
        return 0;
    }
    public void RandomSetUnknownItem(StdItem stdItem, UserItem pu) { }
    public void RandomUpgradeItem(StdItem stdItem, UserItem pu) { }
    public int RealAttackSpeed(short wAtkSpd) => wAtkSpd <= 10 ? -wAtkSpd : wAtkSpd - 10;
}

/// <summary>`ClientRepairItem` 里的 `M2Share.ScriptEngine.GotoLable` 记录替身。</summary>
internal sealed class NullScriptEngine : IScriptEngine
{
    public readonly List<string> Gotos = new();
    public void GotoLable(IPlayerActor playerActor, int actorId, string sLabel, bool boExtJmp = false)
        => Gotos.Add(sLabel);
    public void GotoLable(INormNpc normNpc, IPlayerActor playerActor, string sLabel, bool boExtJmp = false)
        => Gotos.Add(sLabel);
}

/// <summary>
/// `IPlayerActor` 动态替身：只实现价格/修理路径真正用到的成员
/// （`ScriptLable`/`Gold`/`MyGuild`/`DecGold`/`SendMsg`），其余返回 default。
/// </summary>
internal class PlayerProxy : DispatchProxy
{
    public string ScriptLable = string.Empty;
    public int Gold;
    public IGuild MyGuild;
    public readonly List<(int ident, int p1, int p2, int p3)> Sends = new();

    protected override object Invoke(MethodInfo targetMethod, object[] args)
    {
        switch (targetMethod.Name)
        {
            case "get_ScriptLable":
                return ScriptLable;
            case "set_ScriptLable":
                ScriptLable = (string)args[0];
                return null;
            case "get_Gold":
                return Gold;
            case "set_Gold":
                Gold = (int)args[0];
                return null;
            case "get_MyGuild":
                return MyGuild;
            case "set_MyGuild":
                MyGuild = (IGuild)args[0];
                return null;
            case "DecGold":
            {
                int n = (int)args[0];
                if (Gold >= n)
                {
                    Gold -= n;
                    return true;
                }
                return false;
            }
            case "IncGold":
            {
                int n = (int)args[0];
                if (Gold + n <= SystemShare.Config.HumanMaxGold)
                {
                    Gold += n;
                    return true;
                }
                return false;
            }
            case "SendMsg":
            {
                ParameterInfo[] ps = targetMethod.GetParameters();
                int baseIdx = ps[0].ParameterType == typeof(int) ? 0 : 1;
                // args = [ident, wParam, nParam1, nParam2, nParam3, sMsg]
                Sends.Add(((int)args[baseIdx], (int)args[baseIdx + 2], (int)args[baseIdx + 3],
                    (int)args[baseIdx + 4]));
                return null;
            }
        }
        return Default(targetMethod.ReturnType);
    }

    private static object Default(Type t)
        => t == typeof(void) ? null : (t.IsValueType ? Activator.CreateInstance(t) : null);
}

/// <summary>`IUserCastle` 动态替身：只实现 `IsMasterGuild`（价格折扣用）。</summary>
internal class CastleProxy : DispatchProxy
{
    public bool IsMaster;

    protected override object Invoke(MethodInfo targetMethod, object[] args)
    {
        if (targetMethod.Name == "IsMasterGuild")
        {
            return IsMaster;
        }
        return targetMethod.ReturnType == typeof(void)
            ? null
            : (targetMethod.ReturnType.IsValueType ? Activator.CreateInstance(targetMethod.ReturnType) : null);
    }
}

internal static class Program
{
    private static string F(double v) => v.ToString("R", CultureInfo.InvariantCulture);

    private static readonly List<string> Tsv = new();
    private static readonly List<object> Cases = new();

    private static Merchant NewMerchant()
    {
        var m = new Merchant
        {
            ActorId = 9001,
            ChrName = "probe-merchant",
            PriceRate = 100,
            CastleMerchant = false,
        };
        return m;
    }

    private static void SetStdItem(ProbeItemSystem catalog, ushort index, byte stdMode, ushort stdDuraMax,
        int price)
    {
        catalog.Items[index] = new StdItem
        {
            Name = "probe-" + index,
            StdMode = stdMode,
            DuraMax = stdDuraMax,
            Price = price,
            Weight = 1,
        };
    }

    private static void AddItemPriceCase(string name, Func<Merchant, double> run, ushort index,
        bool listPresent, ushort listIndex, double listPrice, bool stdPresent, byte stdMode, int stdPrice,
        bool inTypeList, double expect)
    {
        Tsv.Add(string.Join("\t", "item_price", name, index.ToString(CultureInfo.InvariantCulture),
            listPresent ? "1" : "0", listIndex.ToString(CultureInfo.InvariantCulture), F(listPrice),
            stdPresent ? "1" : "0", stdMode.ToString(CultureInfo.InvariantCulture),
            stdPrice.ToString(CultureInfo.InvariantCulture), inTypeList ? "1" : "0", F(expect)));
        Cases.Add(new
        {
            family = "item_price",
            name,
            index = (int)index,
            list_present = listPresent,
            list_index = (int)listIndex,
            list_price = listPrice,
            std_present = stdPresent,
            std_mode = (int)stdMode,
            std_price = stdPrice,
            in_type_list = inTypeList,
            expect,
        });
    }

    private static void AddUserItemPriceCase(string name, ProbeItemSystem catalog, Merchant m,
        MethodInfo fn, double basePrice, byte stdMode, ushort stdDuraMax, ushort dura, ushort duraMax,
        byte[] desc, double expect, ushort expectDuraMaxAfter)
    {
        var sb = new List<string> { "uip", name, catalog.Items.ContainsKey(100) ? "1" : "0", F(basePrice),
            stdMode.ToString(CultureInfo.InvariantCulture),
            stdDuraMax.ToString(CultureInfo.InvariantCulture), dura.ToString(CultureInfo.InvariantCulture),
            duraMax.ToString(CultureInfo.InvariantCulture) };
        for (int i = 0; i < 8; i++)
        {
            sb.Add(desc[i].ToString(CultureInfo.InvariantCulture));
        }
        sb.Add(F(expect));
        sb.Add(expectDuraMaxAfter.ToString(CultureInfo.InvariantCulture));
        Tsv.Add(string.Join("\t", sb));
        Cases.Add(new
        {
            family = "user_item_price",
            name,
            std_present = catalog.Items.ContainsKey(100),
            base_price = basePrice,
            std_mode = (int)stdMode,
            std_dura_max = (int)stdDuraMax,
            dura = (int)dura,
            dura_max = (int)duraMax,
            desc = desc,
            expect,
            expect_dura_max_after = (int)expectDuraMaxAfter,
        });
    }

    private static void AddUserPriceCase(string name, double nPrice, int priceRate, bool castleMerchant,
        bool castleMember, int memberRate, double expect)
    {
        Tsv.Add(string.Join("\t", "user_price", name, F(nPrice), priceRate.ToString(CultureInfo.InvariantCulture),
            castleMerchant ? "1" : "0", castleMember ? "1" : "0",
            memberRate.ToString(CultureInfo.InvariantCulture), F(expect)));
        Cases.Add(new
        {
            family = "user_price",
            name,
            n_price = nPrice,
            price_rate = priceRate,
            castle_merchant = castleMerchant,
            castle_member = castleMember,
            member_rate = memberRate,
            expect,
        });
    }

    private static void AddSellPriceCase(string name, double nPrice, double expect)
    {
        Tsv.Add(string.Join("\t", "sell_price", name, F(nPrice), F(expect)));
        Cases.Add(new { family = "sell_price", name, n_price = nPrice, expect });
    }

    private static void AddRepairCostCase(string name, double basePrice, int priceRate, ushort dura,
        ushort duraMax, byte stdMode, string label, bool isRepair, bool isSupRepair, int expectParam)
    {
        int labelKind = label == ScriptFlagConst.SuperRepair ? 1 : 0;
        Tsv.Add(string.Join("\t", "repair_cost", name, F(basePrice), priceRate.ToString(CultureInfo.InvariantCulture),
            dura.ToString(CultureInfo.InvariantCulture), duraMax.ToString(CultureInfo.InvariantCulture),
            stdMode.ToString(CultureInfo.InvariantCulture), labelKind.ToString(CultureInfo.InvariantCulture),
            isRepair ? "1" : "0", isSupRepair ? "1" : "0", expectParam.ToString(CultureInfo.InvariantCulture)));
        Cases.Add(new
        {
            family = "repair_cost",
            name,
            base_price = basePrice,
            price_rate = priceRate,
            dura = (int)dura,
            dura_max = (int)duraMax,
            std_mode = (int)stdMode,
            label_kind = labelKind,
            is_repair = isRepair,
            is_sup_repair = isSupRepair,
            expect_param = expectParam,
        });
    }

    private static void AddRepairItemCase(string name, double basePrice, int priceRate, ushort dura,
        ushort duraMax, byte stdMode, string label, bool isRepair, bool isSupRepair, int gold,
        string expectFrames, int expectGold, int expectDura, int expectDuraMax)
    {
        int labelKind = label == ScriptFlagConst.SuperRepair
            ? 1
            : (label == ScriptFlagConst.Superrepairfail ? 2 : 0);
        Tsv.Add(string.Join("\t", "repair_item", name, F(basePrice), priceRate.ToString(CultureInfo.InvariantCulture),
            dura.ToString(CultureInfo.InvariantCulture), duraMax.ToString(CultureInfo.InvariantCulture),
            stdMode.ToString(CultureInfo.InvariantCulture), labelKind.ToString(CultureInfo.InvariantCulture),
            isRepair ? "1" : "0", isSupRepair ? "1" : "0", gold.ToString(CultureInfo.InvariantCulture),
            expectFrames, expectGold.ToString(CultureInfo.InvariantCulture),
            expectDura.ToString(CultureInfo.InvariantCulture), expectDuraMax.ToString(CultureInfo.InvariantCulture)));
        Cases.Add(new
        {
            family = "repair_item",
            name,
            base_price = basePrice,
            price_rate = priceRate,
            dura = (int)dura,
            dura_max = (int)duraMax,
            std_mode = (int)stdMode,
            label_kind = labelKind,
            is_repair = isRepair,
            is_sup_repair = isSupRepair,
            gold,
            expect_frames = expectFrames,
            expect_gold = expectGold,
            expect_dura = (int)expectDura,
            expect_dura_max = (int)expectDuraMax,
        });
    }

    public static int Main(string[] args)
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        LogService.Logger = new LoggerConfiguration().CreateLogger();
        var catalog = new ProbeItemSystem();
        SystemShare.ItemSystem = catalog;
        var engine = new NullScriptEngine();
        M2Share.ScriptEngine = engine;

        MethodInfo getItemPrice = typeof(Merchant).GetMethod("GetItemPrice",
            BindingFlags.Instance | BindingFlags.NonPublic, null, new[] { typeof(ushort) }, null)
            ?? throw new MissingMethodException("Merchant.GetItemPrice(ushort)");
        MethodInfo getUserItemPrice = typeof(Merchant).GetMethod("GetUserItemPrice",
            BindingFlags.Instance | BindingFlags.NonPublic, null, new[] { typeof(UserItem) }, null)
            ?? throw new MissingMethodException("Merchant.GetUserItemPrice(UserItem)");
        MethodInfo getUserPrice = typeof(Merchant).GetMethod("GetUserPrice",
            BindingFlags.Instance | BindingFlags.NonPublic, null, new[] { typeof(IPlayerActor), typeof(double) }, null)
            ?? throw new MissingMethodException("Merchant.GetUserPrice(IPlayerActor, double)");
        MethodInfo getSellItemPrice = typeof(Merchant).GetMethod("GetSellItemPrice",
            BindingFlags.Static | BindingFlags.NonPublic, null, new[] { typeof(double) }, null)
            ?? throw new MissingMethodException("Merchant.GetSellItemPrice(double)");
        MethodInfo queryRepairCost = typeof(Merchant).GetMethod("ClientQueryRepairCost",
            BindingFlags.Instance | BindingFlags.Public, null, new[] { typeof(IPlayerActor), typeof(UserItem) }, null)
            ?? throw new MissingMethodException("Merchant.ClientQueryRepairCost(IPlayerActor, UserItem)");
        MethodInfo repairItem = typeof(Merchant).GetMethod("ClientRepairItem",
            BindingFlags.Instance | BindingFlags.Public, null, new[] { typeof(IPlayerActor), typeof(UserItem) }, null)
            ?? throw new MissingMethodException("Merchant.ClientRepairItem(IPlayerActor, UserItem)");

        // ===== 1) GetItemPrice（Merchant.cs:347）=====
        {
            // 列表覆盖命中：走 ItemPriceList
            var m = NewMerchant();
            m.ItemPriceList = new List<ItemPrice> { new ItemPrice { wIndex = 5, nPrice = 1234.5 } };
            SetStdItem(catalog, 5, 0, 0, 999);
            double r = (double)getItemPrice.Invoke(m, new object[] { (ushort)5 });
            AddItemPriceCase("list_hit_overrides_std", _ => r, 5, true, 5, 1234.5, true, 0, 999, false, r);

            // 列表未命中 + StdMode 在 ItemTypeList ⇒ stdItem.Price
            m = NewMerchant();
            m.ItemPriceList = new List<ItemPrice>();
            m.ItemTypeList = new List<int> { 5, 6 };
            SetStdItem(catalog, 7, 5, 0, 777);
            r = (double)getItemPrice.Invoke(m, new object[] { (ushort)7 });
            AddItemPriceCase("std_fallback_type_hit", _ => r, 7, false, 0, 0, true, 5, 777, true, r);

            // 列表未命中 + StdMode 不在 ItemTypeList ⇒ -1
            m = NewMerchant();
            m.ItemPriceList = new List<ItemPrice>();
            m.ItemTypeList = new List<int> { 5, 6 };
            SetStdItem(catalog, 8, 3, 0, 777);
            r = (double)getItemPrice.Invoke(m, new object[] { (ushort)8 });
            AddItemPriceCase("std_fallback_type_miss", _ => r, 8, false, 0, 0, true, 3, 777, false, r);

            // 列表未命中 + 无 stdItem ⇒ -1
            m = NewMerchant();
            m.ItemPriceList = new List<ItemPrice>();
            m.ItemTypeList = new List<int> { 5 };
            r = (double)getItemPrice.Invoke(m, new object[] { (ushort)9 });
            AddItemPriceCase("std_missing", _ => r, 9, false, 0, 0, false, 0, 0, false, r);

            // 列表命中但价格为负 ⇒ 仍落回 stdItem（result<0 分支），且 break 生效
            m = NewMerchant();
            m.ItemPriceList = new List<ItemPrice> { new ItemPrice { wIndex = 10, nPrice = -5 } };
            m.ItemTypeList = new List<int> { 5 };
            SetStdItem(catalog, 10, 5, 0, 321);
            r = (double)getItemPrice.Invoke(m, new object[] { (ushort)10 });
            AddItemPriceCase("list_negative_falls_back", _ => r, 10, true, 10, -5, true, 5, 321, true, r);

            // 列表命中且价格为 0 ⇒ 不再落回，返回 0
            m = NewMerchant();
            m.ItemPriceList = new List<ItemPrice> { new ItemPrice { wIndex = 11, nPrice = 0 } };
            m.ItemTypeList = new List<int> { 5 };
            SetStdItem(catalog, 11, 5, 0, 321);
            r = (double)getItemPrice.Invoke(m, new object[] { (ushort)11 });
            AddItemPriceCase("list_zero_no_fallback", _ => r, 11, true, 11, 0, true, 5, 321, true, r);
        }

        // ===== 2) GetUserItemPrice（Merchant.cs:1130）=====
        void UipCase(string name, double basePrice, byte stdMode, ushort stdDuraMax, ushort dura,
            ushort duraMax, byte[] desc, bool stdPresent)
        {
            var m = NewMerchant();
            m.ItemPriceList = new List<ItemPrice> { new ItemPrice { wIndex = 100, nPrice = basePrice } };
            if (stdPresent)
            {
                SetStdItem(catalog, 100, stdMode, stdDuraMax, 0);
            }
            else
            {
                catalog.Items.Remove(100);
            }
            var u = new UserItem { Index = 100, Dura = dura, DuraMax = duraMax };
            for (int i = 0; i < 8; i++)
            {
                u.Desc[i] = desc[i];
            }
            double r = (double)getUserItemPrice.Invoke(m, new object[] { u });
            AddUserItemPriceCase(name, catalog, m, getUserItemPrice, basePrice, stdMode, stdDuraMax, dura,
                duraMax, desc, r, u.DuraMax);
        }

        byte[] ZeroDesc()
        {
            return new byte[14];
        }

        // itemPrice <= 0 ⇒ 原样返回
        UipCase("price_nonpositive", -1, 5, 1000, 10, 100, ZeroDesc(), true);
        // 无 stdItem ⇒ 原样
        UipCase("std_missing", 500, 5, 1000, 10, 100, ZeroDesc(), false);
        // StdMode <= 4 ⇒ 原样（不进折算）
        UipCase("stdmode_le4", 500, 4, 1000, 10, 100, ZeroDesc(), true);
        // stdItem.DuraMax == 0 ⇒ 原样
        UipCase("std_duramax_zero", 500, 5, 0, 10, 100, ZeroDesc(), true);
        // userItem.DuraMax == 0 ⇒ 原样
        UipCase("user_duramax_zero", 500, 5, 1000, 0, 0, ZeroDesc(), true);

        // StdMode 40（肉）：Dura <= DuraMax 分支
        UipCase("meat_dura_le", 200, 40, 1000, 600, 1000, ZeroDesc(), true);
        // StdMode 40：Dura > DuraMax 分支（负修正）
        UipCase("meat_dura_gt", 200, 40, 1000, 1200, 1000, ZeroDesc(), true);

        // StdMode 43：DuraMax < 10000 ⇒ 被改写成 10000，再折算
        UipCase("stdmode43_clamp", 300, 43, 1000, 500, 500, ZeroDesc(), true);
        // StdMode 43：Dura > DuraMax 分支
        UipCase("stdmode43_dura_gt", 300, 43, 1000, 900, 500, ZeroDesc(), true);

        // StdMode >4 一般分支：Desc[0..7] 累加，n14>0 ⇒ itemPrice/5*n14
        {
            byte[] desc = ZeroDesc();
            desc[0] = 3;
            desc[1] = 4;
            desc[2] = 5;
            UipCase("generic_desc_sum", 500, 15, 1000, 400, 500, desc, true);
        }
        // StdMode >4：n14 == 0 ⇒ 不乘
        UipCase("generic_desc_zero", 500, 15, 1000, 400, 500, ZeroDesc(), true);
        // StdMode 5：Desc[6] <= 10 ⇒ 普通累加
        {
            byte[] desc = ZeroDesc();
            desc[6] = 4;
            desc[0] = 2;
            UipCase("stdmode5_desc6_normal", 500, 5, 1000, 400, 500, desc, true);
        }
        // StdMode 5：Desc[6] > 10 ⇒ (Desc[6]-10)*2 计入（恒真守卫的活口）
        {
            byte[] desc = ZeroDesc();
            desc[6] = 13;
            desc[0] = 2;
            UipCase("stdmode5_desc6_double", 500, 5, 1000, 400, 500, desc, true);
        }
        // StdMode 6：同理
        {
            byte[] desc = ZeroDesc();
            desc[6] = 20;
            desc[7] = 1;
            UipCase("stdmode6_desc6_double", 700, 6, 2000, 1500, 2000, desc, true);
        }
        // 一般分支：stdItem.DuraMax 与 userItem.DuraMax 不同（两个 DuraMax 口径不同）
        {
            byte[] desc = ZeroDesc();
            desc[0] = 1;
            UipCase("generic_two_duramax", 500, 19, 800, 300, 600, desc, true);
        }

        // ===== 3) GetUserPrice（Merchant.cs:765）=====
        {
            var player = DispatchProxy.Create<IPlayerActor, PlayerProxy>();

            // 非城堡商人：Round(nPrice/100*PriceRate)
            var m = NewMerchant();
            m.CastleMerchant = false;
            m.PriceRate = 100;
            double r = (int)getUserPrice.Invoke(m, new object[] { player, 101.0 });
            AddUserPriceCase("plain_rate100", 101.0, 100, false, false, 80, r);

            m = NewMerchant();
            m.PriceRate = 150;
            r = (int)getUserPrice.Invoke(m, new object[] { player, 101.0 });
            AddUserPriceCase("plain_rate150", 101.0, 150, false, false, 80, r);

            // 半整数取整（AwayFromZero）
            m = NewMerchant();
            m.PriceRate = 100;
            r = (int)getUserPrice.Invoke(m, new object[] { player, 1234.5 });
            AddUserPriceCase("plain_half", 1234.5, 100, false, false, 80, r);

            m = NewMerchant();
            m.PriceRate = 33;
            r = (int)getUserPrice.Invoke(m, new object[] { player, 1234.5 });
            AddUserPriceCase("plain_rate33", 1234.5, 33, false, false, 80, r);

            // 城堡商人 + 非成员：走 PriceRate
            m = NewMerchant();
            m.CastleMerchant = true;
            m.Castle = DispatchProxy.Create<IUserCastle, CastleProxy>();
            ((CastleProxy)(object)m.Castle).IsMaster = false;
            m.PriceRate = 100;
            r = (int)getUserPrice.Invoke(m, new object[] { player, 250.0 });
            AddUserPriceCase("castle_nonmember", 250.0, 100, true, false, 80, r);

            // 城堡商人 + 成员：n14 = max(60, Round(PriceRate * memberRate/100))
            m = NewMerchant();
            m.CastleMerchant = true;
            m.Castle = DispatchProxy.Create<IUserCastle, CastleProxy>();
            ((CastleProxy)(object)m.Castle).IsMaster = true;
            m.PriceRate = 100;
            SystemShare.Config.CastleMemberPriceRate = 80;
            r = (int)getUserPrice.Invoke(m, new object[] { player, 250.0 });
            AddUserPriceCase("castle_member_80", 250.0, 100, true, true, 80, r);

            // 城堡成员且折扣 < 60 ⇒ 钳到 60
            m = NewMerchant();
            m.CastleMerchant = true;
            m.Castle = DispatchProxy.Create<IUserCastle, CastleProxy>();
            ((CastleProxy)(object)m.Castle).IsMaster = true;
            m.PriceRate = 50;
            SystemShare.Config.CastleMemberPriceRate = 80;
            r = (int)getUserPrice.Invoke(m, new object[] { player, 250.0 });
            AddUserPriceCase("castle_member_clamp60", 250.0, 50, true, true, 80, r);

            // 城堡商人但 Castle == null ⇒ 走 PriceRate
            m = NewMerchant();
            m.CastleMerchant = true;
            m.Castle = null;
            m.PriceRate = 120;
            r = (int)getUserPrice.Invoke(m, new object[] { player, 333.0 });
            AddUserPriceCase("castle_merchant_null_castle", 333.0, 120, true, false, 80, r);

            // 还原默认折扣率，避免污染后续
            SystemShare.Config.CastleMemberPriceRate = 80;
        }

        // ===== 4) GetSellItemPrice（Merchant.cs:1358）=====
        foreach (double p in new double[] { 100.0, 101.0, 99.0, 1.0, 0.0, 2.5, 0.5, -3.0 })
        {
            double r = (int)getSellItemPrice.Invoke(null, new object[] { p });
            AddSellPriceCase("p" + F(p).Replace("-", "m").Replace(".", "_"), p, r);
        }

        // ===== 5) ClientQueryRepairCost（Merchant.cs:1555）=====
        void RcCase(string name, double basePrice, int priceRate, ushort dura, ushort duraMax, byte stdMode,
            string label, bool isRepair, bool isSupRepair)
        {
            var m = NewMerchant();
            m.PriceRate = priceRate;
            m.IsRepair = isRepair;
            m.IsSupRepair = isSupRepair;
            m.ItemPriceList = new List<ItemPrice> { new ItemPrice { wIndex = 200, nPrice = basePrice } };
            SetStdItem(catalog, 200, stdMode, 1000, 0);
            var player = (PlayerProxy)(object)DispatchProxy.Create<IPlayerActor, PlayerProxy>();
            player.ScriptLable = label;
            var u = new UserItem { Index = 200, Dura = dura, DuraMax = duraMax };
            _ = queryRepairCost.Invoke(m, new object[] { player, u });
            int param = player.Sends.Count > 0 ? player.Sends[0].p1 : int.MinValue;
            AddRepairCostCase(name, basePrice, priceRate, dura, duraMax, stdMode, label, isRepair, isSupRepair,
                param);
        }

        // 普通修理可用：round((nPrice/3.0)/DuraMax*(DuraMax-Dura))
        RcCase("plain_ok", 900, 100, 400, 1000, 5, "", true, false);
        // 普通修理可用（StdMode<=4 ⇒ GetUserItemPrice 原样返回，隔离修理公式本身）
        RcCase("plain_ok_stdmode0", 900, 100, 400, 1000, 0, "", true, false);
        // 半整数耐久取整口径
        RcCase("round_half", 900, 100, 435, 1000, 0, "", true, false);
        // 普通修理不可用 ⇒ -1
        RcCase("plain_not_supported", 900, 100, 400, 1000, 5, "", false, false);
        // 特殊修理可用 ⇒ * SuperRepairPriceRate(3)
        RcCase("super_ok", 900, 100, 400, 1000, 5, ScriptFlagConst.SuperRepair, true, true);
        // 特殊修理不可用 ⇒ -1
        RcCase("super_not_supported", 900, 100, 400, 1000, 5, ScriptFlagConst.SuperRepair, true, false);
        // DuraMax <= Dura ⇒ -1（未损耗）
        RcCase("no_damage", 900, 100, 1000, 1000, 5, "", true, false);
        // 未损耗但 Dura > DuraMax（回绕语义）⇒ 仍 -1（DuraMax > Dura 为假）
        RcCase("dura_gt_duramax", 900, 100, 1200, 1000, 5, "", true, false);
        // PriceRate != 100
        RcCase("rate150", 900, 150, 100, 1000, 5, "", true, false);
        // StdMode 43 在查询成本路径**不**被拒（只有 ClientRepairItem 拒）
        RcCase("stdmode43_query", 900, 100, 400, 1000, 43, "", true, false);

        // ===== 6) ClientRepairItem（Merchant.cs:1599）=====
        void RiCase(string name, double basePrice, int priceRate, ushort dura, ushort duraMax, byte stdMode,
            string label, bool isRepair, bool isSupRepair, int gold)
        {
            var m = NewMerchant();
            m.PriceRate = priceRate;
            m.IsRepair = isRepair;
            m.IsSupRepair = isSupRepair;
            m.ItemPriceList = new List<ItemPrice> { new ItemPrice { wIndex = 300, nPrice = basePrice } };
            SetStdItem(catalog, 300, stdMode, 1000, 0);
            var player = (PlayerProxy)(object)DispatchProxy.Create<IPlayerActor, PlayerProxy>();
            player.ScriptLable = label;
            player.Gold = gold;
            var u = new UserItem { Index = 300, Dura = dura, DuraMax = duraMax };
            int before = engine.Gotos.Count;
            _ = repairItem.Invoke(m, new object[] { player, u });
            string frames;
            if (player.Sends.Count == 0)
            {
                frames = "NONE";
            }
            else if (player.Sends[0].ident == Messages.RM_USERREPAIRITEM_OK)
            {
                var s = player.Sends[0];
                frames = $"OK,{s.p1},{s.p2},{s.p3}";
            }
            else
            {
                frames = "FAIL";
            }
            AddRepairItemCase(name, basePrice, priceRate, dura, duraMax, stdMode, label, isRepair, isSupRepair,
                gold, frames, player.Gold, u.Dura, u.DuraMax);
        }

        // 普通修理工可用：DuraMax -= (DuraMax-Dura)/RepairItemDecDura; Dura = DuraMax
        RiCase("plain_ok", 900, 100, 400, 1000, 5, "", true, false, 100000);
        // 普通修理（StdMode<=4 ⇒ 价格不被耐久折算）
        RiCase("plain_ok_stdmode0", 900, 100, 400, 1000, 0, "", true, false, 100000);
        // 普通修理：耐久差不能被 RepairItemDecDura 整除 ⇒ C# 整数截断
        RiCase("trunc_dec", 900, 100, 14, 1000, 0, "", true, false, 100000);
        // 普通修理不支持 ⇒ 早退，无帧
        RiCase("plain_unsupported_early", 900, 100, 400, 1000, 5, "", false, false, 100000);
        // 特殊修理可用：Dura = DuraMax，价格 * SuperRepairPriceRate
        RiCase("super_ok", 900, 100, 400, 1000, 5, ScriptFlagConst.SuperRepair, true, true, 100000);
        // 特殊修理不支持（脚本处于 @s_repair 但 IsSupRepair=false）⇒ 早退，无帧
        RiCase("super_unsupported_early", 900, 100, 400, 1000, 5, ScriptFlagConst.SuperRepair, true, false, 100000);
        // @Superrepairfail ⇒ 失败帧
        RiCase("superrepairfail", 900, 100, 400, 1000, 5, ScriptFlagConst.Superrepairfail, true, true, 100000);
        // 未损耗 ⇒ 失败帧
        RiCase("no_damage", 900, 100, 1000, 1000, 5, "", true, false, 100000);
        // StdMode 43 ⇒ 失败帧
        RiCase("stdmode43_reject", 900, 100, 400, 1000, 43, "", true, false, 100000);
        // 金币不足 ⇒ 失败帧（DecGold 假）
        RiCase("not_enough_gold", 900, 100, 400, 1000, 5, "", true, false, 0);
        // PriceRate != 100
        RiCase("rate150", 900, 150, 100, 1000, 5, "", true, false, 100000);

        string outPath = args.Length > 0
            ? args[0]
            : Path.Combine(AppContext.BaseDirectory, "..", "..", "..", "..", "..",
                "shop-price-probe", "vectors", "shop_price_vectors.json");

        var payload = new
        {
            generated_by = "mir2-rs/tests/parity/m3/shop-price-probe",
            oracle = new
            {
                assembly = "GameSrv (src/GameSrv/GameSrv.csproj) — 真身反射",
                entry = new[]
                {
                    "Merchant.GetItemPrice(ushort) Merchant.cs:347",
                    "Merchant.GetUserItemPrice(UserItem) Merchant.cs:1130",
                    "Merchant.GetUserPrice(IPlayerActor, double) Merchant.cs:765",
                    "Merchant.GetSellItemPrice(double) Merchant.cs:1358 (static)",
                    "Merchant.ClientQueryRepairCost(IPlayerActor, UserItem) Merchant.cs:1555",
                    "Merchant.ClientRepairItem(IPlayerActor, UserItem) Merchant.cs:1599",
                },
                config = $"SuperRepairPriceRate={SystemShare.Config.SuperRepairPriceRate} " +
                         $"RepairItemDecDura={SystemShare.Config.RepairItemDecDura} " +
                         $"CastleMemberPriceRate={SystemShare.Config.CastleMemberPriceRate} " +
                         $"HumanMaxGold={SystemShare.Config.HumanMaxGold}",
            },
            cases = Cases,
        };

        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outPath)));
        string json = JsonSerializer.Serialize(payload, new JsonSerializerOptions { WriteIndented = true });
        // Windows 上 System.Text.Json 的缩进换行可能为 CRLF；仓库口径要求 LF ⇒ 归一后落盘。
        json = json.Replace("\r\n", "\n");
        File.WriteAllText(outPath, json + "\n", new UTF8Encoding(false));
        File.WriteAllText(Path.ChangeExtension(outPath, ".tsv"), string.Join("\n", Tsv) + "\n",
            new UTF8Encoding(false));
        Console.WriteLine($"shop-price-probe: wrote {Path.GetFullPath(outPath)} ({Cases.Count} cases) + .tsv");
        return 0;
    }
}
