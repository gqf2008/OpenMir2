// 卡 openmir2-golden-fixture-drift（M51 装置参数化）：golden 夹具契约的公共实现。
//
// 唯一 schema（opc-07 2026-11-20，与 `2d` 的比较器共用；每支 case 必填、缺任一即红）：
//   rng_mode        : "cover_drain" | "freeze_at_dospell"
//   login_order     : true | false
//   reachability    : "reachable" | "unverified" | "unreachable"
//   anti_magic      : int
//   target_level    : int
//   target_hp       : int
//   target_max_hp   : int
//   derived         : { max_hp, max_mp, dc, ac, mac, max_exp }
//   construction    : { job, level, equipment[], skills[] }
//   scenario_note   : string               （login_order=false 必填）
//   legacy_artifact : true + legacy_reason （仅 freeze_at_dospell 自动写）
//
// 判据（任一不满足 ⇒ FixtureContractViolation，调用点 rc=2、不落盘）：
//   1) 缺/非法 rng_mode、reachability；缺 construction；2) login_order=false 缺 scenario_note；
//   3) login_order=true + rng_mode=freeze_at_dospell；4) reachability=unreachable 缺依据；
//   5) legacy_artifact 与 rng_mode 矛盾、freeze 缺 legacy_reason；
//   6) derived 必须等于 oracle 本人复算值（`RecalcLevelAbilitys()+RecalcAbilitys()`）——造假即红；
//   7) reachability=reachable 时，平铺 anti_magic/target_max_hp 必须等于该可达组合的复算值（手设值不得凭空）。
using System.Reflection;

namespace MagicGolden;

/// <summary>契约违规（调用方必须 rc=2、不落盘）。</summary>
public sealed class FixtureContractViolation : Exception
{
    public FixtureContractViolation(string message) : base(message) { }
}

public static class FixtureContract
{
    public static readonly string[] RngModes = { "cover_drain", "freeze_at_dospell" };
    public static readonly string[] Reachabilities = { "reachable", "unverified", "unreachable" };

    //  声明在 （）；对派生类 GetMethod 找非公有
    // 不搜基类声明 ⇒ 必须从  取。
    private static readonly MethodInfo GetLevelExpMethod = typeof(M2Server.Actor.BaseObject)
        .GetMethod("GetLevelExp", BindingFlags.NonPublic | BindingFlags.Static)
        ?? throw new InvalidOperationException("PlayObject.GetLevelExp 不存在（静态签名变了？）");

    private static readonly Lazy<int[]> NeedExps = new(LoadNeedExps);

    /// <summary>
    /// 生产路径的  读入是显式调用（ 的初始化调 ）；
    ///  的静态构造只建对象。离线装置同样要显式调一次，否则  全 0。
    /// </summary>
    private static readonly Lazy<bool> ExpConfLoaded = new(() =>
    {
        SystemModule.SystemShare.ExpConf.LoadConfig();
        return true;
    });

    /// <summary>
    /// 从**冻结表**  读  的完整数据源
    /// （来源  的 sha256/mtime 一并落盘）。路径可用  覆盖；
    /// 找不到/解析失败 ⇒ 契约违规（rc=2），不静默跳过。
    /// </summary>
    private static int[] LoadNeedExps()
    {
        string path = Environment.GetEnvironmentVariable("MAGICGOLDEN_EXPS_JSON");
        if (string.IsNullOrWhiteSpace(path))
        {
            var dir = new DirectoryInfo(AppContext.BaseDirectory);
            while (dir != null && !Directory.Exists(Path.Combine(dir.FullName, "mir2-rs", "tests", "parity", "fixtures")))
            {
                dir = dir.Parent;
            }
            if (dir != null)
            {
                path = Path.Combine(dir.FullName, "mir2-rs", "tests", "parity", "fixtures", "exps-need-exps.json");
            }
        }
        if (string.IsNullOrWhiteSpace(path) || !File.Exists(path))
        {
            throw new FixtureContractViolation("缺冻结表 exps-need-exps.json（或 MAGICGOLDEN_EXPS_JSON 指向不存在）");
        }
        System.Text.Json.JsonDocument doc;
        try
        {
            doc = System.Text.Json.JsonDocument.Parse(File.ReadAllText(path));
        }
        catch (Exception ex)
        {
            throw new FixtureContractViolation("exps-need-exps.json 解析失败：" + ex.Message);
        }
        using var _doc = doc;
        if (!doc.RootElement.TryGetProperty("need_exps", out var arr))
        {
            throw new FixtureContractViolation("exps-need-exps.json 缺 need_exps[]");
        }
        var list = new List<int>();
        foreach (var v in arr.EnumerateArray()) list.Add(v.GetInt32());
        return list.ToArray();
    }

    /// <summary>登录序派生值（用 oracle 本尊的两函数复算，不另写公式）。</summary>
    public sealed class Derived
    {
        public int MaxHp;
        public int MaxMp;
        public int Dc;
        public int Ac;
        public int Mac;
        public int MaxExp;
    }

    /// <summary>无装备/无技能加成的登录序角色复算。</summary>
    public static Derived Recompute(OpenMir2.Enums.PlayerJob job, byte level)
    {
        var p = new M2Server.Player.PlayObject
        {
            ChrName = "contract-scratch",
            ActorId = 1,
            CurrX = 10,
            CurrY = 10,
            Job = job,
        };
        p.Abil.Level = level;
        p.RecalcLevelAbilitys();
        p.RecalcAbilitys();
        _ = ExpConfLoaded.Value;
        var table = NeedExps.Value;
        int tableExp = level < table.Length ? table[level] : table[^1];
        int liveExp = (int)(GetLevelExpMethod.Invoke(null, new object[] { (int)level }) ?? 0);
        if (tableExp != liveExp)
        {
            throw new FixtureContractViolation(
                $"exps 冻结表与运行期 Exps.conf 不一致：冻结 need_exps[{level}]={tableExp}，live GetLevelExp({level})={liveExp}（先更新 exps-need-exps.json 并重生成受影响件）");
        }
        var maxExp = tableExp;
        return new Derived
        {
            MaxHp = p.WAbil.MaxHP,
            MaxMp = p.WAbil.MaxMP,
            Dc = p.WAbil.DC,
            Ac = p.WAbil.AC,
            Mac = p.WAbil.MAC,
            MaxExp = maxExp,
        };
    }

    public sealed class Construction
    {
        public OpenMir2.Enums.PlayerJob Job;
        public byte Level;
        public List<string> Equipment = new();
        public List<string> Skills = new();
    }

    public sealed class FixtureInput
    {
        public string Label;
        public string RngMode;
        public bool LoginOrder;
        public string Reachability;
        public int? AntiMagic;
        public int? TargetLevel;
        public int? TargetHp;
        public int? TargetMaxHp;
        public Derived Derived;
        public Construction Construction;
        public string ScenarioNote;
        public string UnreachableEvidence;
        public bool LegacyArtifact;
        public string LegacyReason;
    }

    /// <summary>四道硬门 + 三条自洽门；违规抛 <see cref="FixtureContractViolation"/>（调用方 rc=2）。</summary>
    public static void Validate(FixtureInput f)
    {
        if (string.IsNullOrWhiteSpace(f.RngMode) || Array.IndexOf(RngModes, f.RngMode) < 0)
        {
            throw new FixtureContractViolation($"[{f.Label}] rng_mode 缺失或非法：{f.RngMode ?? "<null>"}（cover_drain|freeze_at_dospell）");
        }
        if (string.IsNullOrWhiteSpace(f.Reachability) || Array.IndexOf(Reachabilities, f.Reachability) < 0)
        {
            throw new FixtureContractViolation($"[{f.Label}] reachability 缺失或非法：{f.Reachability ?? "<null>"}（reachable|unverified|unreachable）");
        }
        if (f.Construction is null || f.Derived is null)
        {
            throw new FixtureContractViolation($"[{f.Label}] 缺 construction 或 derived（构造路径必须可机检）");
        }
        if (f.LoginOrder && f.RngMode == "freeze_at_dospell")
        {
            throw new FixtureContractViolation($"[{f.Label}] 非法组合：login_order=true + rng_mode=freeze_at_dospell");
        }
        if (!f.LoginOrder && string.IsNullOrWhiteSpace(f.ScenarioNote))
        {
            throw new FixtureContractViolation($"[{f.Label}] login_order=false 缺 scenario_note（手设时刻：构造后、任何 Operate 前）");
        }
        if (f.Reachability == "unreachable" && string.IsNullOrWhiteSpace(f.UnreachableEvidence))
        {
            throw new FixtureContractViolation($"[{f.Label}] reachability=unreachable 但缺不可达依据（unreachable_evidence 为空）");
        }
        if (f.LegacyArtifact && f.RngMode != "freeze_at_dospell")
        {
            throw new FixtureContractViolation($"[{f.Label}] legacy_artifact=true 但 rng_mode={f.RngMode}（只允许 freeze_at_dospell）");
        }
        if (f.RngMode == "freeze_at_dospell" && string.IsNullOrWhiteSpace(f.LegacyReason))
        {
            throw new FixtureContractViolation($"[{f.Label}] rng_mode=freeze_at_dospell 必须写 legacy_reason");
        }
        if (f.AntiMagic is null || f.TargetLevel is null || f.TargetHp is null || f.TargetMaxHp is null)
        {
            throw new FixtureContractViolation($"[{f.Label}] 平铺字段缺失（anti_magic/target_level/target_hp/target_max_hp 四项必填）");
        }
        // 第 6 道门：derived 必须等于 oracle 复算值（造假即红）。
        var oracle = Recompute(f.Construction.Job, f.Construction.Level);
        if (f.Derived.MaxHp != oracle.MaxHp || f.Derived.MaxMp != oracle.MaxMp
            || f.Derived.Dc != oracle.Dc || f.Derived.Ac != oracle.Ac || f.Derived.Mac != oracle.Mac
            || f.Derived.MaxExp != oracle.MaxExp)
        {
            throw new FixtureContractViolation(
                $"[{f.Label}] derived 与 oracle 复算不等：derived=max_hp/{f.Derived.MaxHp} max_mp/{f.Derived.MaxMp} dc/{f.Derived.Dc} ac/{f.Derived.Ac} mac/{f.Derived.Mac} max_exp/{f.Derived.MaxExp}；"
                + $"oracle({f.Construction.Job} L{f.Construction.Level})=max_hp/{oracle.MaxHp} max_mp/{oracle.MaxMp} dc/{oracle.Dc} ac/{oracle.Ac} mac/{oracle.Mac} max_exp/{oracle.MaxExp}");
        }
        // 第 7 道门：reachable 时平铺值必须等于该可达组合的复算值（手设值不得凭空）。
        if (f.Reachability == "reachable")
        {
            if (f.TargetLevel != f.Construction.Level || f.TargetMaxHp != oracle.MaxHp || f.AntiMagic != 1)
            {
                throw new FixtureContractViolation(
                    $"[{f.Label}] 平铺值不可达：target_level={f.TargetLevel}/target_max_hp={f.TargetMaxHp}/anti_magic={f.AntiMagic}；"
                    + $"该组合（{f.Construction.Job} L{f.Construction.Level}，无装备）复算 target_level={f.Construction.Level}/max_hp={oracle.MaxHp}/anti_magic=1");
            }
        }
    }
}
