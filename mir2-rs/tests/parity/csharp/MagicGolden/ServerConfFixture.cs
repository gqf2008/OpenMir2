// 卡 `openmir2-magic-serverconf-batch-plan` §3.1：**共享 ServerConf 装载入口**（只服务 magic 夹具）。
//
// 职责（限定，不扩）：
//   * 定位 `<BaseDirectory>/Server.conf`（`EnsureConfFiles` 已把部署配置拷进输出目录）；
//   * 缺文件 / sha256 不符 / 十键任一不等于部署期望值 ⇒ `FixtureContractViolation`（调用方 rc=2、不落盘）；
//   * `new ServerConf(path).LoadConfig()`（与生产同一解析器）；
//   * 返回十键快照，供 golden `Fixture.server_conf` 落盘。
//
// **不做**：不读 `String.conf` / `MessageSettings`（`YouPoisoned` 那套属「字符串配置同源」，
// 留在施毒批自己的装载里）；不读 golden；不做任何帧/行为判定。
//
// 注：`ConfigFile.ReadWriteInteger` 对**缺键**会 `WriteInteger` 写回**内存中的 ini 缓存**（不落磁盘，
// `src/OpenMir2/Common/ConfigFile.cs` 的 `ReadWriteInteger`/`ReadInteger`）——加载有内存副作用，
// 无需额外处理；本入口只在违反契约时抛错，不静默吃缺省。
//
// 十键口径（ASCII 拼写跟 oracle：`Sucess` 一个 c；键 `MagTammingTargetHPRate` 映射 C# 字段
// `MagTammingHPRate` 属 BUG-REPLICA）：
//
// **pin 是故意的 fail-closed**：任何人改部署 `Server.conf` 都会让 golden 生成失败。正确动作是
// 「同步更新 `PinnedServerConfSha256` 并重出相关 golden」，不是绕断言——否则下次改配置会被误当 bug。
// 断言顺序固定：**先 sha、后十键值**（"文件变了"与"值错了"排查动作完全不同，消息不能混）。

using System;
using System.Collections.Generic;
using System.IO;
using System.Security.Cryptography;

namespace MagicGolden;

public static class ServerConfFixture
{
    /// <summary>
    /// 生成源（`MAGICGOLDEN_CONF_DIR` 缺省 `E:\MirServer\M2GameSvr\Server.conf`）整文件身份。
    /// 该文件一改，golden 生成必须当场红——若换了生成源（如花名册副本），**更新此常量**，不得加旁路。
    /// </summary>
    public const string PinnedServerConfSha256 =
        "ce5e3ce98b9bb71cbb18e6be1501e20e05642fd3eeaaffd9326caef8d9a4c0e2";

    /// <summary>
    /// **非缺省对照臂**的冻结夹具身份（`mir2-rs/tests/parity/fixtures/serverconf/nondefault/Server.conf`，
    /// 由部署件逐字节复制后只改 `MabMabe*` 四键；卡 `magic-serverconf-nondefault-arm-plan`）。
    /// 同样 fail-closed：夹具件一改，生成当场红——正确动作是更新本常量并重出 `mabmabe-nondefault`。
    /// </summary>
    public const string PinnedNonDefaultSha256 =
        "cc59503c330cd9c41120f440e289a052d50e260f368889fc0e1f5b64be12376f";

    /// <summary>
    /// 十键口径的**唯一来源**：`(golden 键名, oracle `[Setup]` 键, 部署期望值)`。
    ///
    /// 一张表同时定义三个名字的映射：
    ///   * `GoldenKey` ⇒ golden `Fixture.server_conf` 的键名（`Snapshot.Values` 用它）；
    ///   * `IniKey`    ⇒ `Server.conf` 的 `[Setup]` 键名（`ReadValue` 用它查 `SystemShare.Config`）；
    ///   * `Expected`  ⇒ 部署期望值（断言用；**十键期望值只出现在这里**）。
    /// 例：`("mag_tamming_hp_rate", "MagTammingTargetHPRate", 100)` —— 键 ≠ 字段（BUG-REPLICA）。
    /// 加键/改期望值只改这张表 + `ReadValue` 的一个映射臂。
    /// </summary>
    /// <summary>
    /// 非缺省臂十键期望值：只把 `MabMabe*` 四键换成**第三值**（≠ 部署值、≠ ctor 缺省）：
    /// `HitRandRate 50`、`MinLvLimit 40`、`SucessRate 90`、`MabeTimeRate 7`。
    /// 其余六键保持部署口径（本支只针对 MG-48 的四个消费点）。`LoadOrThrow(nonDefault: true)`
    /// 会用本表断言，并做"第三值 ≠ 部署值 && ≠ ctor 缺省"的元断言。
    /// </summary>
    public static readonly (string GoldenKey, string IniKey, int Expected)[] NonDefaultKeys =
    {
        ("mag_turn_undead_level", "MagTurnUndeadLevel", 44),
        ("mab_mabe_hit_rand_rate", "MabMabeHitRandRate", 50),
        ("mab_mabe_hit_min_lv_limit", "MabMabeHitMinLvLimit", 40),
        ("mab_mabe_hit_sucess_rate", "MabMabeHitSucessRate", 90),
        ("mab_mabe_hit_mabe_time_rate", "MabMabeHitMabeTimeRate", 7),
        ("amy_ounsul_point", "AmyOunsulPoint", 15),
        ("mag_tamming_level", "MagTammingLevel", 40),
        ("mag_tamming_target_level", "MagTammingTargetLevel", 1),
        ("mag_tamming_hp_rate", "MagTammingTargetHPRate", 100),
        ("mag_tamming_count", "MagTammingCount", 5),
    };

    public static readonly (string GoldenKey, string IniKey, int Expected)[] Keys =
    {
        ("mag_turn_undead_level", "MagTurnUndeadLevel", 44),
        ("mab_mabe_hit_rand_rate", "MabMabeHitRandRate", 100),
        ("mab_mabe_hit_min_lv_limit", "MabMabeHitMinLvLimit", 10),
        ("mab_mabe_hit_sucess_rate", "MabMabeHitSucessRate", 21), // 一个 c（oracle 键与 Rust 字段同拼写）
        ("mab_mabe_hit_mabe_time_rate", "MabMabeHitMabeTimeRate", 20),
        ("amy_ounsul_point", "AmyOunsulPoint", 15),
        ("mag_tamming_level", "MagTammingLevel", 40),
        ("mag_tamming_target_level", "MagTammingTargetLevel", 1),
        ("mag_tamming_hp_rate", "MagTammingTargetHPRate", 100), // 键名；C# 字段是 MagTammingHPRate
        ("mag_tamming_count", "MagTammingCount", 5),
    };

    /// <summary>十键解析值快照；键名与 golden `Fixture.server_conf` 一致。</summary>
    public sealed class Snapshot
    {
        public Snapshot(IReadOnlyDictionary<string, int> values)
        {
            Values = values;
        }

        public IReadOnlyDictionary<string, int> Values { get; }
    }

    /// <summary>
    /// **MG-48（2026-11-26）**：配置变体表——每个变体是**完整 `Server.conf` 副本**（其余九键=部署值），
    /// 逐变体钉 sha256；`LoadVariantOrThrow` 断言 sha + 十键（被改的那一键 = 变体值）。
    /// 变体存在的唯一理由：`MabMabeHitRandRate` 的 **0/1**（§27 `Random(0)` 边界）与
    /// `MabMabeHitMabeTimeRate` 的时长面。
    /// </summary>
    public static readonly Dictionary<string, (string GoldenKey, int Value, string Sha256)> Variants = new()
    {
        ["mabmabe-rand0"] = ("mab_mabe_hit_rand_rate", 0, "0871d6482a02df6c385e834693080c7d9be27ce36e5d3f9f07fc0fd82b40915f"),
        ["mabmabe-rand1"] = ("mab_mabe_hit_rand_rate", 1, "787da312423edfec048b09fc73cfa7c07b9fad895288d8ddc3e4c02cce822578"),
        ["mabmabe-time10"] = ("mab_mabe_hit_mabe_time_rate", 10, "7421123b28a0f9cd742b02ca9d8b67938727eda1c5d060a74c80f9895885579f"),
    };

    /// <summary>装载变体 `Server.conf`（sha 钉死 + 十键断言；被改键 = 变体值）。</summary>
    public static Snapshot LoadVariantOrThrow(string caseName)
    {
        if (!Variants.TryGetValue(caseName, out var v))
        {
            throw new FixtureContractViolation($"[{caseName}] 不是已登记的配置变体");
        }
        string path = Path.Combine(AppContext.BaseDirectory, "Server.conf");
        if (!File.Exists(path))
        {
            throw new FixtureContractViolation($"[{caseName}] 缺 Server.conf：{path}");
        }
        string sha = FileSha256(path);
        if (!string.Equals(sha, v.Sha256, StringComparison.OrdinalIgnoreCase))
        {
            throw new FixtureContractViolation(
                $"[{caseName}] 变体 Server.conf sha256 与钉住值不符：got={sha} want={v.Sha256}");
        }
        new SystemModule.Conf.ServerConf(path).LoadConfig();
        var cfg = SystemModule.SystemShare.Config;
        var values = new Dictionary<string, int>(Keys.Length);
        foreach (var (goldenKey, iniKey, _) in Keys)
        {
            values[goldenKey] = ReadValue(cfg, iniKey);
        }
        foreach (var (goldenKey, _, expected) in Keys)
        {
            int want = goldenKey == v.GoldenKey ? v.Value : expected;
            if (values[goldenKey] != want)
            {
                throw new FixtureContractViolation($"[{caseName}] {goldenKey} 期望 {want}，实得 {values[goldenKey]}");
            }
        }
        return new Snapshot(values);
    }

    /// <summary>
    /// 装载 `&lt;BaseDirectory&gt;/Server.conf` 并断言十键 == 部署期望值。
    /// 任何违约抛 <see cref="FixtureContractViolation"/>（`caseName` 只用于错误定位）。
    /// </summary>
    public static Snapshot LoadOrThrow(string caseName, bool nonDefault = false)
    {
        var keys = nonDefault ? NonDefaultKeys : Keys;
        string pinnedSha = nonDefault ? PinnedNonDefaultSha256 : PinnedServerConfSha256;

        string path = Path.Combine(AppContext.BaseDirectory, "Server.conf");
        if (!File.Exists(path))
        {
            throw new FixtureContractViolation($"[{caseName}] 缺 Server.conf：{path}");
        }

        string sha = FileSha256(path);
        if (!string.Equals(sha, pinnedSha, StringComparison.OrdinalIgnoreCase))
        {
            throw new FixtureContractViolation(
                $"[{caseName}] Server.conf sha256 与钉住值不符：got={sha} want={pinnedSha}"
                + "（生成源一改必须更新常量，不得绕过断言）");
        }

        new SystemModule.Conf.ServerConf(path).LoadConfig();
        var cfg = SystemModule.SystemShare.Config;

        var values = new Dictionary<string, int>(keys.Length);
        foreach (var (goldenKey, iniKey, _) in keys)
        {
            values[goldenKey] = ReadValue(cfg, iniKey);
        }

        // 顺序固定：sha 已在上面先判完；这里只报「值不符」（消息里不含 sha 字段）。
        foreach (var (goldenKey, _, expected) in keys)
        {
            int got = values[goldenKey];
            if (got != expected)
            {
                throw new FixtureContractViolation(
                    $"[{caseName}] {goldenKey} 期望 {expected}，实得 {got}"
                    + "——缺键/空值/非数字会回落缺省，本入口不许吃缺省");
            }
        }

        if (nonDefault) AssertThirdValues(caseName);
        return new Snapshot(values);
    }

    /// <summary>
    /// 元断言（卡 §1.1/§6）：非缺省臂改名的那四键必须 **≠ 部署值 && ≠ ctor 缺省**，
    /// 否则"非缺省臂"退化成复用部署值或复用缺省值，判据整体失效。
    /// ctor 缺省来自 `GameSvrConf` 构造函数（`MagTurnUndeadLevel=50` 等；四键缺省 = 100/10/21/20）。
    /// </summary>
    private static void AssertThirdValues(string caseName)
    {
        var deploy = Keys.ToDictionary(k => k.GoldenKey, k => k.Expected);
        var ctor = new Dictionary<string, int>(StringComparer.Ordinal)
        {
            ["mab_mabe_hit_rand_rate"] = 100,
            ["mab_mabe_hit_min_lv_limit"] = 10,
            ["mab_mabe_hit_sucess_rate"] = 21,
            ["mab_mabe_hit_mabe_time_rate"] = 20,
        };
        foreach (var (goldenKey, _, expected) in NonDefaultKeys)
        {
            if (!ctor.TryGetValue(goldenKey, out int ctorDefault)) continue;
            if (expected == deploy[goldenKey] || expected == ctorDefault)
            {
                throw new FixtureContractViolation(
                    $"[{caseName}] {goldenKey}={expected} 必须 ≠ 部署值 {deploy[goldenKey]} 且 ≠ ctor 缺省 {ctorDefault}"
                    + "（否则非缺省臂没有判别力）");
            }
        }
    }

    /// <summary>
    /// oracle `[Setup]` 键 → `SystemShare.Config` 当前值。键名与字段名的全部差异都集中在这里
    /// （如 `MagTammingTargetHPRate` ⇒ `MagTammingHPRate`）；未知键直接违约，防止
    /// `Keys` 加了行却忘了接字段。
    /// </summary>
    private static int ReadValue(SystemModule.Conf.Model.GameSvrConf cfg, string iniKey) => iniKey switch
    {
        "MagTurnUndeadLevel" => cfg.MagTurnUndeadLevel,
        "MabMabeHitRandRate" => cfg.MabMabeHitRandRate,
        "MabMabeHitMinLvLimit" => cfg.MabMabeHitMinLvLimit,
        "MabMabeHitSucessRate" => cfg.MabMabeHitSucessRate,
        "MabMabeHitMabeTimeRate" => cfg.MabMabeHitMabeTimeRate,
        "AmyOunsulPoint" => cfg.AmyOunsulPoint,
        "MagTammingLevel" => cfg.MagTammingLevel,
        "MagTammingTargetLevel" => cfg.MagTammingTargetLevel,
        // BUG-REPLICA：oracle 键写作 `MagTammingTargetHPRate`，字段是 `MagTammingHPRate`。
        "MagTammingTargetHPRate" => cfg.MagTammingHPRate,
        // 字节型：显式转 int（别依赖隐式提升；将来字段若改成 ReadWriteInt 也不影响这里的断言口径）。
        "MagTammingCount" => (int)cfg.MagTammingCount,
        _ => throw new FixtureContractViolation("ServerConfFixture.Keys 含未映射 ini 键：" + iniKey),
    };

    private static string FileSha256(string path)
    {
        using var sha = SHA256.Create();
        using var fs = File.OpenRead(path);
        return Convert.ToHexString(sha.ComputeHash(fs)).ToLowerInvariant();
    }
}
