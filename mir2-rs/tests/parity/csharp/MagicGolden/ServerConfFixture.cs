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
    /// 十键口径的**唯一来源**：`(golden 键名, oracle `[Setup]` 键, 部署期望值)`。
    ///
    /// 一张表同时定义三个名字的映射：
    ///   * `GoldenKey` ⇒ golden `Fixture.server_conf` 的键名（`Snapshot.Values` 用它）；
    ///   * `IniKey`    ⇒ `Server.conf` 的 `[Setup]` 键名（`ReadValue` 用它查 `SystemShare.Config`）；
    ///   * `Expected`  ⇒ 部署期望值（断言用；**十键期望值只出现在这里**）。
    /// 例：`("mag_tamming_hp_rate", "MagTammingTargetHPRate", 100)` —— 键 ≠ 字段（BUG-REPLICA）。
    /// 加键/改期望值只改这张表 + `ReadValue` 的一个映射臂。
    /// </summary>
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
    /// 装载 `&lt;BaseDirectory&gt;/Server.conf` 并断言十键 == 部署期望值。
    /// 任何违约抛 <see cref="FixtureContractViolation"/>（`caseName` 只用于错误定位）。
    /// </summary>
    public static Snapshot LoadOrThrow(string caseName)
    {
        string path = Path.Combine(AppContext.BaseDirectory, "Server.conf");
        if (!File.Exists(path))
        {
            throw new FixtureContractViolation($"[{caseName}] 缺 Server.conf：{path}");
        }

        string sha = FileSha256(path);
        if (!string.Equals(sha, PinnedServerConfSha256, StringComparison.OrdinalIgnoreCase))
        {
            throw new FixtureContractViolation(
                $"[{caseName}] Server.conf sha256 与钉住值不符：got={sha} want={PinnedServerConfSha256}"
                + "（生成源一改必须更新常量，不得绕过断言）");
        }

        new SystemModule.Conf.ServerConf(path).LoadConfig();
        var cfg = SystemModule.SystemShare.Config;

        var values = new Dictionary<string, int>(Keys.Length);
        foreach (var (goldenKey, iniKey, _) in Keys)
        {
            values[goldenKey] = ReadValue(cfg, iniKey);
        }

        // 顺序固定：sha 已在上面先判完；这里只报「值不符」（消息里不含 sha 字段）。
        foreach (var (goldenKey, _, expected) in Keys)
        {
            int got = values[goldenKey];
            if (got != expected)
            {
                throw new FixtureContractViolation(
                    $"[{caseName}] {goldenKey} 期望 {expected}（部署值），实得 {got}"
                    + "——缺键/空值/非数字会回落缺省，本入口不许吃缺省");
            }
        }

        return new Snapshot(values);
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
