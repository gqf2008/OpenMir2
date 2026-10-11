// feature-probe —— 用 C# oracle 真身实测「装备集 → feature 位」向量。
//
// 为什么要有这个探针：`bins/game-svr/src/feature.rs` 的接缝必须与 oracle **逐位相同**，
// 而位布局（`MakeWord` 的参数序、`MakeLong` 的高低字）靠读源码重推极易抄错。
// 这里直接 `new PlayObject()` + 真的 `GetFeature(null)`、真的 `M2Share.MakeHumanFeature`，
// 值由运行中的 oracle 代码产出；向量落 `mir2-rs/tests/parity/m3/fixtures/feature_vectors.json`，
// Rust 侧用例回读并逐位核对（改坏映射 ⇒ 用例必红）。
//
// 只读：不连 MySQL、不写任何现网文件（`SystemShare.ItemSystem` 换成内存表，
// 只用来给 `GetFeature` 提供 `StdItem.Shape` 输入）。
//
// 用法：
//   dotnet run --project mir2-rs/tests/parity/m3/feature-probe -c Release -- <out.json>

using System;
using System.Collections.Generic;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using M2Server;
using M2Server.Player;
using OpenMir2;
using OpenMir2.Data;
using OpenMir2.Enums;
using OpenMir2.Packets.ClientPackets;
using OpenMir2.Packets.ServerPackets;
using Serilog;
using SystemModule;
using SystemModule.SubSystem;

namespace FeatureProbe;

/// <summary>
/// `SystemShare.ItemSystem` 的内存替身：只实现 `GetStdItem(ushort)`，
/// 用来给 `PlayObject.GetFeature` 喂 `StdItem.Shape`（其余成员永不被本探针触达）。
/// </summary>
internal sealed class ShapeTable : IItemSystem
{
    public readonly Dictionary<int, byte> Shapes = new();

    public StdItem GetStdItem(ushort nItemIdx)
        => Shapes.TryGetValue(nItemIdx, out var shape) ? new StdItem { Shape = shape } : null;

    public void AddItem(StdItem stdItem) => throw new NotSupportedException();
    public void Clear() => Shapes.Clear();
    public int ItemCount => Shapes.Count;
    public string GetStdItemName(int itemIdx) => throw new NotSupportedException();
    public ushort GetStdItemIdx(string itemName) => throw new NotSupportedException();
    public bool CopyToUserItemFromName(string sItemName, ref UserItem item) => throw new NotSupportedException();
    public StdItem GetStdItem(string sItemName) => throw new NotSupportedException();
    public int GetStdItemWeight(int nItemIdx) => throw new NotSupportedException();
    public int GetUpgrade2(int x, int a) => throw new NotSupportedException();
    public int GetUpgradeStdItem(StdItem stdItem, UserItem userItem, ref ClientItem clientItem) => throw new NotSupportedException();
    public void RandomSetUnknownItem(StdItem stdItem, UserItem pu) => throw new NotSupportedException();
    public void RandomUpgradeItem(StdItem stdItem, UserItem pu) => throw new NotSupportedException();
    public int RealAttackSpeed(short wAtkSpd) => throw new NotSupportedException();
}

internal static class Program
{
    /// <summary>替身表里用来指向"穿在身上"的那个物品下标（数组 0 槽 = Dress，1 槽 = Weapon）。</summary>
    private const ushort DressItemIndex = 100;
    private const ushort WeaponItemIndex = 101;

    private static string Hex(int value) => "0x" + value.ToString("X8");

    private static int RunCase(ShapeTable table, int gender, byte hair, int? dressShape, int? weaponShape, uint dressIndex = DressItemIndex, uint weaponIndex = WeaponItemIndex)
    {
        var po = new PlayObject
        {
            UseItems = new UserItem[13],
            Gender = (PlayerGender)gender,
            Hair = hair,
        };
        if (dressShape.HasValue)
        {
            table.Shapes[(int)dressIndex] = (byte)dressShape.Value;
            po.UseItems[ItemLocation.Dress] = new UserItem { Index = (ushort)dressIndex };
        }
        if (weaponShape.HasValue)
        {
            table.Shapes[(int)weaponIndex] = (byte)weaponShape.Value;
            po.UseItems[ItemLocation.Weapon] = new UserItem { Index = (ushort)weaponIndex };
        }
        int feature = po.GetFeature(null);
        int featureToLong = po.GetFeatureToLong();
        if (feature != featureToLong)
        {
            throw new InvalidOperationException("GetFeatureToLong() != GetFeature(null) —— oracle 假设破裂");
        }
        return feature;
    }

    private static int RunFeatureEx(bool onHorse, byte horseType, byte dressEffType)
    {
        var po = new PlayObject { OnHorse = onHorse, HorseType = horseType, DressEffType = dressEffType };
        return po.GetFeatureEx();
    }

    private static int RunMakeHumanFeature(byte raceImg, byte dress, byte weapon, byte hair)
    {
        int packed = M2Share.MakeHumanFeature(raceImg, dress, weapon, hair);
        // 顺带实测 HUtil32 的两个原语，钉住参数序（MakeWord(low, high) = high<<8|low）。
        int manual = HUtil32.MakeLong(HUtil32.MakeWord(raceImg, weapon), HUtil32.MakeWord(hair, dress));
        if (packed != manual)
        {
            throw new InvalidOperationException("MakeHumanFeature != MakeLong(MakeWord(raceImg,weapon), MakeWord(hair,dress))");
        }
        return packed;
    }

    public static int Main(string[] args)
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        // oracle 的静态构造（M2Share → ScriptParsers 等）会经 LogService 打日志；
        // 真实进程在启动时注入 Serilog logger，这里补上，否则 M2Share 静态构造 NRE。
        LogService.Logger = new LoggerConfiguration().CreateLogger();

        string outPath = args.Length > 0
            ? args[0]
            : Path.Combine(AppContext.BaseDirectory, "..", "..", "..", "..", "..", "fixtures", "feature_vectors.json");

        var table = new ShapeTable();
        SystemShare.ItemSystem = table;

        // ---- A. 端到端：角色状态（UseItems[Dress/Weapon] 的 StdItem.Shape + Gender + Hair）→ GetFeature(null) ----
        var cases = new List<object>();
        void Case(string name, int gender, byte hair, int? dress, int? weapon)
        {
            int feature = RunCase(table, gender, hair, dress, weapon);
            cases.Add(new
            {
                name,
                gender,
                hair,
                dress_shape = dress,
                weapon_shape = weapon,
                feature,
                feature_hex = Hex(feature),
            });
        }

        Case("naked_man_hair1", 0, 1, null, null);
        Case("naked_man_hair0", 0, 0, null, null);
        Case("naked_woman_hair1", 1, 1, null, null);
        Case("naked_woman_hair3", 1, 3, null, null);
        Case("dress_shape1_man", 0, 1, 1, null);
        Case("dress_shape2_woman", 1, 1, 2, null);
        Case("weapon_shape2_woman", 1, 1, null, 2);
        Case("weapon_shape5_man", 0, 1, null, 5);
        Case("dress1_weapon2_man", 0, 1, 1, 2);
        Case("dress2_weapon3_woman", 1, 1, 2, 3);
        // 边界：Shape*2 溢出 byte（(byte)(200*2)=144）
        Case("dress_shape200_man", 0, 1, 200, null);
        Case("weapon_shape200_man", 0, 1, null, 200);
        // 边界：Hair*2 + gender 溢出 byte
        Case("hair128_woman", 1, 128, null, null);
        Case("hair255_man", 0, 255, null, null);
        // 边界：Shape=127 ⇒ *2=254 不溢出
        Case("dress_shape127_man", 0, 1, 127, null);
        Case("weapon_shape127_woman", 1, 0, null, 127);

        // 未穿（Index=0）⇒ 忽略；stdItem 为 null ⇒ 忽略
        int naked = RunCase(table, 0, 1, null, null);
        var indexZero = new PlayObject
        {
            UseItems = new UserItem[13],
            Gender = PlayerGender.Man,
            Hair = 1,
        };
        indexZero.UseItems[ItemLocation.Dress] = new UserItem { Index = 0 };
        indexZero.UseItems[ItemLocation.Weapon] = new UserItem { Index = 0 };
        if (indexZero.GetFeature(null) != naked)
        {
            throw new InvalidOperationException("Index==0 的装备槽必须被 GetFeature 忽略");
        }
        var missingStdItem = new PlayObject
        {
            UseItems = new UserItem[13],
            Gender = PlayerGender.Man,
            Hair = 1,
        };
        missingStdItem.UseItems[ItemLocation.Dress] = new UserItem { Index = 999 }; // 替身表里没有 ⇒ GetStdItem 返回 null
        if (missingStdItem.GetFeature(null) != naked)
        {
            throw new InvalidOperationException("GetStdItem 返回 null 时必须忽略该槽");
        }

        // ---- B. 原语：MakeHumanFeature 的位布局（(raceImg,dress,weapon,hair) → int32） ----
        var packing = new List<object>();
        void Pack(byte raceImg, byte dress, byte weapon, byte hair)
        {
            int value = RunMakeHumanFeature(raceImg, dress, weapon, hair);
            packing.Add(new { race_img = raceImg, dress, weapon, hair, value, value_hex = Hex(value) });
        }
        Pack(0, 0, 0, 0);
        Pack(0, 2, 0, 4);
        Pack(0, 1, 1, 1);
        Pack(0, 5, 9, 3);
        Pack(0, 0xFF, 0xFF, 0xFF);
        Pack(0, 0x80, 0x00, 0x40);
        Pack(0, 0x00, 0x80, 0x00);
        Pack(7, 6, 5, 4);

        // ---- C. GetFeatureEx：MakeWord(OnHorse ? HorseType : 0, DressEffType) ----
        var featureEx = new List<object>();
        void Ex(string name, bool onHorse, byte horseType, byte dressEffType)
        {
            int value = RunFeatureEx(onHorse, horseType, dressEffType);
            featureEx.Add(new { name, on_horse = onHorse, horse_type = horseType, dress_eff_type = dressEffType, value, value_hex = "0x" + value.ToString("X4") });
        }
        Ex("off_horse_dress_eff0", false, 0, 0);
        Ex("off_horse_type7", false, 7, 0);
        Ex("on_horse7_eff0", true, 7, 0);
        Ex("on_horse7_eff3", true, 7, 3);
        Ex("off_horse_eff9", false, 5, 9);
        Ex("on_horse255_eff255", true, 255, 255);

        string asmPath = typeof(M2Share).Assembly.Location;
        string asmName = Path.GetFileName(asmPath);
        string asmSha = Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(asmPath))).ToLowerInvariant();

        // O-2（reviewer-4 2026-10-11）：`oracle_assembly_sha256` 是**构建产物** hash，随编译器/
        // SDK/环境变化，不能当可复现锚点。校验锚点改为**源码**锚点（下面是参与本向量语义的
        // oracle 源文件清单；顺序即拼接序）。
        var oracleSourceFiles = new[]
        {
            "src/M2Server/Player/PlayObject.cs",
            "src/M2Server/Player/CharacterObject.cs",
            "src/M2Server/Actor/BaseObject.cs",
            "src/M2Server/M2Share.cs",
            "src/OpenMir2/HUtil32.cs",
        };
        string sourceAnchor = SourceAnchorSha256(oracleSourceFiles);

        var doc = new
        {
            generator = "mir2-rs/tests/parity/m3/feature-probe",
            oracle_assembly = asmName,
            oracle_assembly_sha256 = asmSha,
            oracle_assembly_sha256_note = "生成时 M2Server.dll 构建产物 hash（随编译器/SDK/环境变化）；仅作溯源，不参与向量校验——校验锚点是 oracle_source_sha256",
            oracle_source_sha256 = sourceAnchor,
            oracle_source_sha256_files = oracleSourceFiles,
            oracle_source = new[]
            {
                "src/M2Server/Player/PlayObject.cs:3393 (GetFeature)",
                "src/M2Server/Player/PlayObject.cs:1128-1147 (SendLogon/SM_LOGON/SM_FEATURECHANGED)",
                "src/M2Server/M2Share.cs:1965 (MakeHumanFeature)",
                "src/M2Server/Player/CharacterObject.cs:146 (GetFeatureEx)",
                "src/M2Server/Actor/BaseObject.cs:1487 (GetFeatureToLong)",
            },
            notes = new[]
            {
                "feature 位布局（LSB→MSB）= [raceImg:8][weapon:8][hair:8][dress:8]",
                "featureEx 位布局（LSB→MSB）= [horse:8][dressEff:8]",
                "GetFeature 只在 UseItems[Dress/Weapon].Index>0 且 GetStdItem 非 null 时计入 Shape；(byte)(Shape*2) 与 byte 加法均按 byte 截断",
            },
            cases,
            packing,
            feature_ex = featureEx,
        };

        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outPath))!);
        File.WriteAllText(outPath, JsonSerializer.Serialize(doc, new JsonSerializerOptions { WriteIndented = true }) + "\n", new UTF8Encoding(false));
        Console.WriteLine($"WROTE {Path.GetFullPath(outPath)}");
        Console.WriteLine($"ORACLE_ASSEMBLY {asmPath}");
        Console.WriteLine($"ORACLE_ASSEMBLY_SHA256 {asmSha}");
        Console.WriteLine($"ORACLE_SOURCE_SHA256 {sourceAnchor}");
        Console.WriteLine($"CASES {cases.Count} PACKING {packing.Count} FEATURE_EX {featureEx.Count}");
        return 0;
    }

    /// <summary>
    /// oracle 源锚点：按清单顺序把「相对路径 + 文件字节」喂进 sha256（UTF-8，小写 hex）。
    /// 为什么不用 M2Server.dll 的 hash：那是构建产物（随编译器/SDK/环境变化，不可复现），
    /// 不能当可复现锚点；在「内容冻结」（设计文档 §1 硬约束②）下源码才是稳定锚点。
    /// 需在仓库根运行（cwd = 仓库根），清单是相对路径。
    /// </summary>
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
