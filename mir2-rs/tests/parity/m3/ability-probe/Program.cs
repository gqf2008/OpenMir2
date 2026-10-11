// ability-probe —— 用 C# oracle 真身实测「装备集 + 基础 Abil ⇒ WAbil / SubAbility / feature」向量。
//
// 为什么要有这个探针：`bins/game-svr/src/ability.rs` 的接缝必须与 oracle **逐字段相同**，
// 而 `RecalcAbilitys` 的字段合成（byte 截断、word 高低字节相加与 255 夹取、HP/MP 保留、
// AntiMagic 基线 = 1）靠读源码重推极易抄错。这里直接 `new PlayObject()` + 真的
// `RecalcAbilitys()`，值由运行中的 oracle 代码产出；向量落
// `mir2-rs/tests/parity/m3/fixtures/ability_vectors.json`，Rust 侧用例回读并逐字段核对
//（改坏任一映射 ⇒ 用例必红）。
//
// 作用域（与整卡口径一致）：
//   * **有效物品 = 模板本身**：`GetUpgradeStdItem` 在探针里是**恒等拷贝**（只做
//     CopyItemToClientItem 的字段搬运），即 Desc 升级差量与各 StdMode 的 GetUpgradeStdItem
//     变换**不在本片作用域**（登记为未做）。Rust 侧收的也是"有效物品"，两侧同域。
//   * **不**覆盖：套装/特殊 Shape 的巨型 else-if 块、buff（`ActorBuffSystem`）、
//     技能对 HitPoint 的加成（MagicList 置空）、`ApplyItemParametersByJob` 的龙装分支。
//     这些在卡内登记（见 ability.rs 模块文档）。
//
// 只读：不连 MySQL、不写任何现网文件（`SystemShare.ItemSystem` 换成内存表）。
//
// 用法（**在仓库根运行**，源锚点用相对路径）：
//   dotnet run --project mir2-rs/tests/parity/m3/ability-probe -c Release -- \
//     mir2-rs/tests/parity/m3/fixtures/ability_vectors.json

using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using M2Server;
using M2Server.Player;
using OpenMir2;
using OpenMir2.Data;
using OpenMir2.Enums;
using OpenMir2.Packets.ClientPackets;
using Serilog;
using SystemModule;
using SystemModule.Conf.Model;
using SystemModule.SubSystem;

namespace AbilityProbe;

/// <summary>
/// `SystemShare.ItemSystem` 的内存替身：
/// - `GetStdItem(ushort)` 返回注入的模板；
/// - `GetUpgradeStdItem` = **恒等拷贝**（有效物品 = 模板；Desc 升级不在本片作用域）；
/// - `RealAttackSpeed` 照 oracle 公式（`GameItemSystem.cs:1140`）。
/// </summary>
internal sealed class ProbeItemSystem : IItemSystem
{
    public readonly Dictionary<ushort, StdItem> Items = new();

    public StdItem GetStdItem(ushort nItemIdx)
        => Items.TryGetValue(nItemIdx, out var it) ? it : null;

    public int GetUpgradeStdItem(StdItem stdItem, UserItem userItem, ref ClientItem clientItem)
    {
        if (clientItem == null)
        {
            clientItem = new ClientItem();
        }
        var d = clientItem.Item;
        // 等于 oracle `CopyItemToClientItem`（`GameItemSystem.cs:680`）的字段搬运；不做 Desc 差量、不做 StdMode 变换。
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

    /// <summary>`GameItemSystem.RealAttackSpeed`（`:1140`）：`w<=10 ? -w : w-10`。</summary>
    public int RealAttackSpeed(short wAtkSpd) => wAtkSpd <= 10 ? -wAtkSpd : wAtkSpd - 10;

    public void AddItem(StdItem stdItem) => throw new NotSupportedException();
    public void Clear() => throw new NotSupportedException();
    public int ItemCount => Items.Count;
    public string GetStdItemName(int itemIdx) => throw new NotSupportedException();
    public ushort GetStdItemIdx(string itemName) => throw new NotSupportedException();
    public bool CopyToUserItemFromName(string sItemName, ref UserItem item) => throw new NotSupportedException();
    public StdItem GetStdItem(string sItemName) => throw new NotSupportedException();
    public int GetStdItemWeight(int nItemIdx) => throw new NotSupportedException();
    public int GetUpgrade2(int x, int a) => throw new NotSupportedException();
    public void RandomSetUnknownItem(StdItem stdItem, UserItem pu) => throw new NotSupportedException();
    public void RandomUpgradeItem(StdItem stdItem, UserItem pu) => throw new NotSupportedException();
}

internal static class Program
{
    /// <summary>`MessageSettings.DEFHIT/DEFSPEED`（`src/Modules/SystemModule/MessageSettings.cs:10-11`）。</summary>
    private const ushort DefHit = 5;
    private const ushort DefSpeed = 15;

    /// <summary>本探针显式钉住的 `Config.BonusAbilofXxx`（`GameSvrConf.cs:1413-1415` 的默认值，逐字段抄）。</summary>
    private static NakedAbility TickWarr() => new() { DC = 17, MC = 20, SC = 20, AC = 20, MAC = 20, HP = 1, MP = 3, Hit = 20, Speed = 35, Reserved = 0 };
    private static NakedAbility TickWizard() => new() { DC = 17, MC = 25, SC = 30, AC = 20, MAC = 15, HP = 2, MP = 1, Hit = 25, Speed = 35, Reserved = 0 };
    private static NakedAbility TickTaos() => new() { DC = 20, MC = 30, SC = 17, AC = 20, MAC = 15, HP = 2, MP = 1, Hit = 30, Speed = 30, Reserved = 0 };

    private static StdItem Tpl(byte stdMode, byte shape, byte weight, ushort ac = 0, ushort mac = 0,
        ushort dc = 0, ushort mc = 0, ushort sc = 0, int hpAdd = 0, int mpAdd = 0, byte agility = 0,
        byte accurate = 0, byte mgAvoid = 0, byte toxAvoid = 0, byte atkSpd = 0, byte slowdown = 0,
        byte tox = 0, sbyte specialPwr = 0, byte effType1 = 0, byte effRate1 = 0, byte effValue1 = 0,
        byte effType2 = 0, byte effRate2 = 0, byte effValue2 = 0, byte light = 0)
    {
        return new StdItem
        {
            Name = "t",
            StdMode = stdMode,
            Shape = shape,
            Weight = weight,
            AC = ac,
            MAC = mac,
            DC = dc,
            MC = mc,
            SC = sc,
            HpAdd = hpAdd,
            MpAdd = mpAdd,
            Agility = agility,
            Accurate = accurate,
            MgAvoid = mgAvoid,
            ToxAvoid = toxAvoid,
            AtkSpd = atkSpd,
            Slowdown = slowdown,
            Tox = tox,
            SpecialPwr = specialPwr,
            EffType1 = effType1,
            EffRate1 = effRate1,
            EffValue1 = effValue1,
            EffType2 = effType2,
            EffRate2 = effRate2,
            EffValue2 = effValue2,
            Light = light,
        };
    }

    private static Ability BaseAbil()
    {
        // 与 M0 金标准 SM_ABILITY（`tests/golden/session-20261010-c2-hook` ident=52）解出的
        // WAbil 对齐：level=1 / DC=0x0101 / HP=MP=15 / MaxHP=19 / MaxExp=10 / MaxWeight=50 /
        // MaxWearWeight=15 / MaxHandWeight=12。裸装时 AddAbil=0 ⇒ WAbil 应逐字段等于本 Abil。
        return new Ability
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
    }

    private static NakedAbility ZeroBonus() => new() { DC = 0, MC = 0, SC = 0, AC = 0, MAC = 0, HP = 0, MP = 0, Hit = 0, Speed = 0, Reserved = 0 };

    /// <summary>
    /// `C# DBSrv.GetAbilGetRecord`（`PlayDataStorage.Query.cs:268-309`）产出的**原始记录 Abil**：
    /// 它读 `Level/HP/MP/DC/MC/SC/Exp/MaxExp/Weight/MaxWeight/WearWeight/MaxWearWeight/HandWeight/MaxHandWeight`，
    /// 但**不读** `characters_ablity.MaxHP/MAxMP` ⇒ 未登录前 `MaxHP=MaxMP=0`；
    /// 且 `HP`/`MP` 列的高 16 位被塞进 `AC`/`MAC`（`HUtil32.HiWord`）。
    /// 登录时 `RecalcLevelAbilitys()` 会用等级公式覆盖这些派生字段。
    /// </summary>
    private static Ability RawRecordAbil() => new()
    {
        Level = 1,
        Reserved1 = 0,
        AC = 0,
        MAC = 0,
        DC = 0,
        MC = 0,
        SC = 0,
        HP = 19,
        MP = 15,
        MaxHP = 0,
        MaxMP = 0,
        Reserved2 = 0,
        ExpCount = 0,
        ExpMaxCount = 0,
        Exp = 0,
        MaxExp = 10,
        Weight = 0,
        MaxWeight = 0,
        WearWeight = 0,
        MaxWearWeight = 0,
        HandWeight = 0,
        MaxHandWeight = 0,
    };

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

    private sealed record EquipSpec(int Slot, ushort Index, ushort Dura);

    /// <summary>`CharacterObject.PoisonRecover/HealthRecover/SpellRecover` 是 `protected`，外部读要用反射。</summary>
    private static ushort ProtectedRecover(PlayObject po, string name)
    {
        var prop = po.GetType().GetProperty(name, BindingFlags.Instance | BindingFlags.NonPublic);
        if (prop == null)
        {
            throw new MissingMemberException($"CharacterObject.{name}");
        }
        return (ushort)prop.GetValue(po);
    }

    private static object RunCase(ProbeItemSystem table, string name, PlayerJob job, Ability abil,
        NakedAbility bonus, EquipSpec[] items, ushort[] bag, byte gender, byte hair,
        ushort? curHp = null, ushort? curMp = null)
    {
        var po = new PlayObject
        {
            Job = job,
            // `RecalcLevelAbilitys()` 会直接改写 `Abil`（`Abil` 是 class）⇒ 探针存进 `po` 的必须是
            // 副本，夹具里的 `abil` 保持**原始记录输入**（否则 `login_record_*` 的 `MaxHP=0` 会被抹掉）。
            Abil = abil.Clone(),
            BonusAbil = bonus,
            UseItems = new UserItem[13],
            ItemList = new List<UserItem>(),
            Gender = (PlayerGender)gender,
            Hair = hair,
            MagicList = new List<UserMagic>(),
        };
        foreach (var s in items)
        {
            po.UseItems[s.Slot] = new UserItem { Index = s.Index, Dura = s.Dura };
        }
        foreach (var idx in bag)
        {
            po.ItemList.Add(new UserItem { Index = idx, Dura = 1 });
        }
        // `RecalcAbilitys` 顶部 `ushort wOldHP = WAbil.HP;` 保留的是**调用前**的当前 HP/MP
        //（`BaseObject.Abil.cs:19-23`）——真机里这是角色当前血量/蓝量，不是 Abil.MaxHP。
        po.WAbil.HP = curHp ?? abil.HP;
        po.WAbil.MP = curMp ?? abil.MP;

        // 登录/升级时 oracle 的调用序（`WorldServer.cs:351` → `PlayObject.cs:1308`）：
        // `AbilCopyToWAbil()` → `RecalcLevelAbilitys()` → `RecalcAbilitys()`。
        // `C# DBSrv.GetAbilGetRecord`（`PlayDataStorage.Query.cs:268-309`）不读 `MaxHP/MAxMP`
        // ⇒ 记录里它们是 0，必须由 `RecalcLevelAbilitys()` 按等级公式补（owner brief §14）。
        po.RecalcLevelAbilitys();
        po.RecalcAbilitys();

        var sub = new
        {
            anti_magic = po.AntiMagic,
            hit_point = po.HitPoint,
            speed_point = po.SpeedPoint,
            anti_poison = po.AntiPoison,
            poison_recover = ProtectedRecover(po, "PoisonRecover"),
            health_recover = ProtectedRecover(po, "HealthRecover"),
            spell_recover = ProtectedRecover(po, "SpellRecover"),
        };
        int feature = po.GetFeature(null);
        if (feature != po.GetFeatureToLong())
        {
            throw new InvalidOperationException($"{name}: GetFeatureToLong() != GetFeature(null)");
        }

        return new
        {
            name,
            job = (int)job,
            gender,
            hair,
            abil = AbilityDto(abil),
            bonus_abil = new { dc = bonus.DC, mc = bonus.MC, sc = bonus.SC, ac = bonus.AC, mac = bonus.MAC, hp = bonus.HP, mp = bonus.MP, hit = bonus.Hit, speed = bonus.Speed, reserved = bonus.Reserved },
            items = Array.ConvertAll(items, s => new { slot = s.Slot, index = s.Index, dura = s.Dura }),
            bag = Array.ConvertAll(bag, i => (int)i),
            cur_hp = po.WAbil.HP,
            cur_mp = po.WAbil.MP,
            expect_wabil = AbilityDto(po.WAbil),
            expect_sub = sub,
            expect_feature = feature,
        };
    }

    public static int Main(string[] args)
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        LogService.Logger = new LoggerConfiguration().CreateLogger();

        string outPath = args.Length > 0
            ? args[0]
            : Path.Combine(AppContext.BaseDirectory, "..", "..", "..", "..", "..", "fixtures", "ability_vectors.json");

        var table = new ProbeItemSystem();
        SystemShare.ItemSystem = table;
        // 显式钉住 bonusTick（RecalcHitSpeed / RecalcAdjusBonus 的除数）；与 GameSvrConf 默认值逐字段相同。
        SystemShare.Config.BonusAbilofWarr = TickWarr();
        SystemShare.Config.BonusAbilofWizard = TickWizard();
        SystemShare.Config.BonusAbilofTaos = TickTaos();

        // ---- 模板表（只覆盖本片作用域内会被 ApplyItemParameters 读到的字段） ----
        // index 1000+：裸装基线；1101.. 各种 StdMode 的普通物品。
        table.Items[1101] = Tpl(stdMode: 5, shape: 0, weight: 60, dc: 0x050A, mac: 0, sc: 0, specialPwr: 3);      // 武器
        table.Items[1102] = Tpl(stdMode: 10, shape: 0, weight: 30, ac: 0x0406, mac: 0x0203, mc: 0x0102, sc: 0x0101,
            hpAdd: 20, mpAdd: 10, agility: 2, mgAvoid: 4, toxAvoid: 3, effType1: 5, effRate1: 2, effValue1: 3);  // 衣服
        table.Items[1103] = Tpl(stdMode: 15, shape: 0, weight: 8, ac: 0x0203, mac: 0x0101, accurate: 2, mgAvoid: 1, toxAvoid: 2); // 头盔
        table.Items[1104] = Tpl(stdMode: 19, shape: 0, weight: 5, ac: 0x0203, mac: 0x0405, accurate: 1, atkSpd: 2, slowdown: 1, tox: 1); // 项链
        table.Items[1105] = Tpl(stdMode: 20, shape: 0, weight: 3, ac: 0x0304, mac: 0x0506, mgAvoid: 2, atkSpd: 1); // 戒指（mode 20）
        table.Items[1106] = Tpl(stdMode: 21, shape: 0, weight: 3, ac: 0x0202, mac: 0x0303, accurate: 1, mgAvoid: 1, atkSpd: 1); // 宝石（mode 21）
        table.Items[1107] = Tpl(stdMode: 22, shape: 0, weight: 4, ac: 0x0102, mac: 0x0101, accurate: 2, hpAdd: 5, atkSpd: 1); // 手镯（mode 22）
        table.Items[1108] = Tpl(stdMode: 23, shape: 0, weight: 4, ac: 0x0303, mac: 0x0202, atkSpd: 2);              // 手镯（mode 23）
        table.Items[1109] = Tpl(stdMode: 24, shape: 0, weight: 2, ac: 0x0401, mac: 0x0200, specialPwr: 2);          // 符（mode 24）
        table.Items[1110] = Tpl(stdMode: 26, shape: 0, weight: 3, ac: 0x0203, mac: 0x0101, accurate: 1, agility: 2, mpAdd: 7, specialPwr: 4); // mode 26
        table.Items[1111] = Tpl(stdMode: 52, shape: 0, weight: 6, ac: 0x0101, mac: 0x0101, agility: 3,
            effType1: 1, effValue1: 5, effType2: 2, effValue2: 4);                                                // 腰带（mode 52）
        table.Items[1112] = Tpl(stdMode: 54, shape: 0, weight: 7, ac: 0x0101, mac: 0x0101, accurate: 2, agility: 1,
            toxAvoid: 4, effType1: 4, effValue1: 6);                                                              // 鞋（mode 54）
        table.Items[1113] = Tpl(stdMode: 53, shape: 0, weight: 1, hpAdd: 30, mpAdd: 20);                           // mode 53
        table.Items[1114] = Tpl(stdMode: 99, shape: 0, weight: 2, ac: 0x0202, mac: 0x0303);                        // default 分支
        table.Items[1115] = Tpl(stdMode: 10, shape: 0, weight: 100, light: 3);                                     // 仅称重/亮度
        table.Items[1201] = Tpl(stdMode: 7, shape: 0, weight: 1);                                                  // 背包物品（称重）
        table.Items[1202] = Tpl(stdMode: 7, shape: 0, weight: 2);

        var cases = new List<object>();
        void Case(string name, PlayerJob job, Ability abil, NakedAbility bonus, EquipSpec[] items, ushort[] bag, byte gender = 0, byte hair = 1, ushort? curHp = null, ushort? curMp = null)
            => cases.Add(RunCase(table, name, job, abil, bonus, items, bag, gender, hair, curHp, curMp));

        // A. 裸装基线：复现 M0 金标准 52/752（level=1 / DC=0x0101 / MaxHP=19 / AntiMagic=1 / Hit=5 / Speed=15）
        // 背包 14 件重量 1 的物品 ⇒ WAbil.Weight=14，逐字段复现金标准 52（HP=MP=15 由 cur_hp/cur_mp 传入）。
        Case("naked_warr_golden", PlayerJob.Warrior, BaseAbil(), ZeroBonus(), Array.Empty<EquipSpec>(),
            new ushort[] { 1201, 1201, 1201, 1201, 1201, 1201, 1201, 1201, 1201, 1201, 1201, 1201, 1201, 1201 },
            curHp: 15, curMp: 15);
        Case("naked_wizard", PlayerJob.Wizard, BaseAbil(), ZeroBonus(), Array.Empty<EquipSpec>(), Array.Empty<ushort>());
        Case("naked_taos", PlayerJob.Taoist, BaseAbil(), ZeroBonus(), Array.Empty<EquipSpec>(), Array.Empty<ushort>());

        // A2. 登录原始记录（`DBSrv.GetAbilGetRecord` 不读 `MaxHP/MAxMP` ⇒ 记录里为 0）：
        // `RecalcLevelAbilitys()` 必须按等级公式补出 `MaxHP/MaxMP/DC/MaxWeight/...`（owner brief §14；
        // 真机 ③ 发现 `52` 的 `max_hp/max_mp=0`）。三职业各一条 ⇒ 覆盖等级派生公式的三个分支。
        Case("login_record_warr", PlayerJob.Warrior, RawRecordAbil(), ZeroBonus(), Array.Empty<EquipSpec>(), Array.Empty<ushort>());
        Case("login_record_wizard", PlayerJob.Wizard, RawRecordAbil(), ZeroBonus(), Array.Empty<EquipSpec>(), Array.Empty<ushort>());
        Case("login_record_taos", PlayerJob.Taoist, RawRecordAbil(), ZeroBonus(), Array.Empty<EquipSpec>(), Array.Empty<ushort>());
        // 带 BonusAbil（非 0）⇒ HitPoint/SpeedPoint 走 `DEFHIT + Hit/tick.Hit`
        var bonusWarr = ZeroBonus();
        bonusWarr.Hit = 40;
        bonusWarr.Speed = 70;
        Case("bonus_warr", PlayerJob.Warrior, BaseAbil(), bonusWarr, Array.Empty<EquipSpec>(), Array.Empty<ushort>());

        // B. 单件装备：覆盖 ApplyItemParameters 的每个 StdMode 分支
        Case("weapon_mode5", PlayerJob.Warrior, BaseAbil(), ZeroBonus(), new[] { new EquipSpec(ItemLocation.Weapon, 1101, 10) }, Array.Empty<ushort>());
        Case("dress_mode10", PlayerJob.Warrior, BaseAbil(), ZeroBonus(), new[] { new EquipSpec(ItemLocation.Dress, 1102, 10) }, Array.Empty<ushort>());
        Case("helmet_mode15", PlayerJob.Warrior, BaseAbil(), ZeroBonus(), new[] { new EquipSpec(ItemLocation.Helmet, 1103, 10) }, Array.Empty<ushort>());
        Case("necklace_mode19", PlayerJob.Warrior, BaseAbil(), ZeroBonus(), new[] { new EquipSpec(ItemLocation.Necklace, 1104, 10) }, Array.Empty<ushort>());
        Case("ring_mode20", PlayerJob.Warrior, BaseAbil(), ZeroBonus(), new[] { new EquipSpec(ItemLocation.Ringl, 1105, 10) }, Array.Empty<ushort>());
        Case("charm_mode21", PlayerJob.Warrior, BaseAbil(), ZeroBonus(), new[] { new EquipSpec(ItemLocation.Charm, 1106, 10) }, Array.Empty<ushort>());
        Case("bracelet_mode22", PlayerJob.Warrior, BaseAbil(), ZeroBonus(), new[] { new EquipSpec(ItemLocation.ArmRingl, 1107, 10) }, Array.Empty<ushort>());
        Case("bracelet_mode23", PlayerJob.Warrior, BaseAbil(), ZeroBonus(), new[] { new EquipSpec(ItemLocation.ArmRingr, 1108, 10) }, Array.Empty<ushort>());
        Case("bujuk_mode24", PlayerJob.Warrior, BaseAbil(), ZeroBonus(), new[] { new EquipSpec(ItemLocation.Bujuk, 1109, 10) }, Array.Empty<ushort>());
        Case("mode26", PlayerJob.Warrior, BaseAbil(), ZeroBonus(), new[] { new EquipSpec(ItemLocation.ArmRingr, 1110, 10) }, Array.Empty<ushort>());
        Case("belt_mode52", PlayerJob.Warrior, BaseAbil(), ZeroBonus(), new[] { new EquipSpec(ItemLocation.Belt, 1111, 10) }, Array.Empty<ushort>());
        Case("boots_mode54", PlayerJob.Warrior, BaseAbil(), ZeroBonus(), new[] { new EquipSpec(ItemLocation.Boots, 1112, 10) }, Array.Empty<ushort>());
        Case("mode53", PlayerJob.Warrior, BaseAbil(), ZeroBonus(), new[] { new EquipSpec(ItemLocation.Charm, 1113, 10) }, Array.Empty<ushort>());
        Case("mode_default", PlayerJob.Warrior, BaseAbil(), ZeroBonus(), new[] { new EquipSpec(ItemLocation.Charm, 1114, 10) }, Array.Empty<ushort>());

        // C. Dura==0 只称重；Dura>0 同时称重
        Case("durability_zero_weighs_only", PlayerJob.Warrior, BaseAbil(), ZeroBonus(), new[] { new EquipSpec(ItemLocation.Weapon, 1101, 0) }, Array.Empty<ushort>());

        // D. 满装（多槽同时生效；非套装普通物品）
        Case("full_plain_set", PlayerJob.Warrior, BaseAbil(), ZeroBonus(), new[]
        {
            new EquipSpec(ItemLocation.Dress, 1102, 10),
            new EquipSpec(ItemLocation.Weapon, 1101, 10),
            new EquipSpec(ItemLocation.Necklace, 1104, 10),
            new EquipSpec(ItemLocation.Helmet, 1103, 10),
            new EquipSpec(ItemLocation.ArmRingl, 1107, 10),
            new EquipSpec(ItemLocation.ArmRingr, 1108, 10),
            new EquipSpec(ItemLocation.Ringl, 1105, 10),
            new EquipSpec(ItemLocation.Ringr, 1106, 10),
            new EquipSpec(ItemLocation.Belt, 1111, 10),
            new EquipSpec(ItemLocation.Boots, 1112, 10),
        }, new ushort[] { 1201, 1202 });

        // E. 空 Index / 缺模板的槽必须被忽略
        Case("index_zero_ignored", PlayerJob.Warrior, BaseAbil(), ZeroBonus(), new[] { new EquipSpec(ItemLocation.Weapon, 0, 10) }, Array.Empty<ushort>());
        Case("missing_template_ignored", PlayerJob.Warrior, BaseAbil(), ZeroBonus(), new[] { new EquipSpec(ItemLocation.Weapon, 65000, 10) }, Array.Empty<ushort>());

        // F. feature 接缝（Dress/Weapon Index → Shape → feature）与 getfeature 同域
        table.Items[1301] = Tpl(stdMode: 10, shape: 5, weight: 10);   // 衣服 Shape=5
        table.Items[1302] = Tpl(stdMode: 5, shape: 7, weight: 10);    // 武器 Shape=7
        Case("feature_dress5_weapon7", PlayerJob.Warrior, BaseAbil(), ZeroBonus(), new[]
        {
            new EquipSpec(ItemLocation.Dress, 1301, 10),
            new EquipSpec(ItemLocation.Weapon, 1302, 10),
        }, Array.Empty<ushort>(), gender: 1, hair: 3);

        // 模板表落盘（cases 只引用 index）
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
                ani_count = t.AniCount,
                special_pwr = t.SpecialPwr,
                ac = t.AC,
                mac = t.MAC,
                dc = t.DC,
                mc = t.MC,
                sc = t.SC,
                atk_spd = t.AtkSpd,
                agility = t.Agility,
                accurate = t.Accurate,
                mg_avoid = t.MgAvoid,
                hp_add = t.HpAdd,
                mp_add = t.MpAdd,
                eff_type1 = t.EffType1,
                eff_rate1 = t.EffRate1,
                eff_value1 = t.EffValue1,
                eff_type2 = t.EffType2,
                eff_rate2 = t.EffRate2,
                eff_value2 = t.EffValue2,
                slowdown = t.Slowdown,
                tox = t.Tox,
                tox_avoid = t.ToxAvoid,
                light = t.Light,
            };
        }

        string asmPath = typeof(M2Share).Assembly.Location;
        string asmSha = Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(asmPath))).ToLowerInvariant();
        var oracleSourceFiles = new[]
        {
            "src/M2Server/Player/PlayObject.cs",
            "src/M2Server/Actor/BaseObject.Abil.cs",
            "src/M2Server/Actor/BaseObject.cs",
            "src/M2Server/Items/GameItemSystem.cs",
            "src/Modules/SystemModule/MessageSettings.cs",
            "src/OpenMir2/HUtil32.cs",
        };
        string sourceAnchor = SourceAnchorSha256(oracleSourceFiles);

        var doc = new
        {
            generator = "mir2-rs/tests/parity/m3/ability-probe",
            oracle_assembly = Path.GetFileName(asmPath),
            oracle_assembly_sha256 = asmSha,
            oracle_assembly_sha256_note = "生成时 M2Server.dll 构建产物 hash（随编译器/SDK/环境变化）；仅作溯源，不参与向量校验——校验锚点是 oracle_source_sha256",
            oracle_source_sha256 = sourceAnchor,
            oracle_source_sha256_files = oracleSourceFiles,
            oracle_source = new[]
            {
                "src/M2Server/Player/PlayObject.cs:2008 (RecalcAbilitys)",
                "src/M2Server/Player/PlayObject.cs:5254 (RecalcLevelAbilitys)",
                "src/M2Server/Player/PlayObject.cs:3647 (RecalcHitSpeed)",
                "src/M2Server/Actor/BaseObject.Abil.cs:15 (BaseObject.RecalcAbilitys)",
                "src/M2Server/Actor/BaseObject.cs:1356 (RecalcBagWeight)",
                "src/M2Server/Player/PlayObject.cs:4407 (ApplyItemParameters)",
                "src/M2Server/Player/PlayObject.cs:4610 (ApplyItemParametersEx)",
                "src/M2Server/Player/PlayObject.cs:3393 (GetFeature)",
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
                "SubAbility.anti_magic 基线 = 1（BaseObject.Abil.cs:31），anti_poison/poison_recover/health_recover/spell_recover 基线 = 0",
                "hit_point = DEFHIT + BonusAbil.Hit/tick.Hit + AddAbil.HIT（Taoist 分支不重置 HitPoint）",
                "speed_point = DEFSPEED + BonusAbil.Speed/tick.Speed (+3 Taoist) + AddAbil.SPEED",
                "WAbil.AC/MAC/DC/MC/SC = word 相加（高字节 _MIN(255, …)）；WAbil.Weight = RecalcBagWeight()",
                "未覆盖：套装 else-if 块 / buff / MagicList 技能加成 / ApplyItemParametersByJob 龙装分支",
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
