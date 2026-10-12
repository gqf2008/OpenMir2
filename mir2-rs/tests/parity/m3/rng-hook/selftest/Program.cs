// M3②③ 活栈隔离口径 · C# 影子 hook 的离线自检（无 OpenMir2/oracle 依赖）。
//
// 覆盖：① MIR2_RNG_ISOLATE_ATTACK 解析语义（默认关 / 开 / 非法 fail-closed）；
//       ② attack_seed = seed + 0x9E37 的 i32 回绕公式；③ 分流站点清单齐备；
//       ④ 调用点路由：只有清单内 site 分流，attack_paralysis(:221) 与未登记 site 不分流。
//
// 退出码：0 = GREEN；1 = RED（任一条 FAIL）。
using System;

internal static class Program
{
    private static int _failed;

    private static void Check(bool ok, string name)
    {
        Console.WriteLine((ok ? "PASS " : "FAIL ") + name);
        if (!ok)
        {
            _failed++;
        }
    }

    private static int Main()
    {
        // ① 开关解析：空/0/false/off/no ⇒ 关；1/true/on/yes ⇒ 开；其它非空 ⇒ fail-closed。
        Check(StartupHook.TryParseIsolateAttack(null, out bool v) && v == false, "parse/null=off");
        Check(StartupHook.TryParseIsolateAttack("", out v) && v == false, "parse/empty=off");
        Check(StartupHook.TryParseIsolateAttack("0", out v) && v == false, "parse/0=off");
        Check(StartupHook.TryParseIsolateAttack("false", out v) && v == false, "parse/false=off");
        Check(StartupHook.TryParseIsolateAttack("OFF", out v) && v == false, "parse/OFF=off");
        Check(StartupHook.TryParseIsolateAttack("no", out v) && v == false, "parse/no=off");
        Check(StartupHook.TryParseIsolateAttack("1", out v) && v == true, "parse/1=on");
        Check(StartupHook.TryParseIsolateAttack("true", out v) && v == true, "parse/true=on");
        Check(StartupHook.TryParseIsolateAttack(" ON ", out v) && v == true, "parse/' ON '=on");
        Check(StartupHook.TryParseIsolateAttack("yes", out v) && v == true, "parse/yes=on");
        Check(!StartupHook.TryParseIsolateAttack("maybe", out _), "parse/maybe=fail-closed");
        Check(!StartupHook.TryParseIsolateAttack("2", out _), "parse/2=fail-closed");
        Check(StartupHook.TryParseIsolateAttack("on ", out v) && v, "parse/'on '=on");

        // ② attack_seed 公式（i32 回绕，与 Rust 侧同）。
        Check(StartupHook.AttackSeedFor(42) == unchecked(42 + 0x9E37), "seed/42");
        Check(StartupHook.AttackSeedFor(0) == 0x9E37, "seed/0");
        Check(StartupHook.AttackSeedFor(int.MaxValue) == unchecked(int.MaxValue + 0x9E37), "seed/int.MaxValue-wrap");
        Check(StartupHook.AttackSeedFor(-1) == unchecked(-1 + 0x9E37), "seed/-1");

        // ③ 分流站点清单：11 个 divert canon，逐条在册。
        string[] sites = StartupHook.DivertSitesForTest();
        Check(sites.Length == 11, "sites/count=11 (got " + sites.Length + ")");
        string[] expected =
        {
            "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2929",
            "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2934",
            "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2918",
            "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2924",
            "M2Server.Actor.BaseObject.GetAttackPower@BaseObject.Base.cs:892",
            "M2Server.Actor.BaseObject._Attack@BaseObject.Attack.cs:41",
            "M2Server.Actor.BaseObject.GetHitStruckDamage@BaseObject.Base.cs:792",
            "M2Server.Actor.BaseObject.GetHitStruckDamage@BaseObject.Base.cs:793",
            "M2Server.Player.PlayObject.StruckDamage@PlayObject.cs:2957",
            "M2Server.Player.PlayObject.StruckDamage@PlayObject.cs:3000",
            "M2Server.Player.PlayObject.AttackSuccess@PlayObject.Attack.cs:223",
        };
        foreach (string e in expected)
        {
            Check(Array.IndexOf(sites, e) >= 0, "sites/has " + e);
        }

        // ④ 路由：清单内分流；attack_paralysis(:221) 与未登记 site 不分流（divert=0）。
        Check(StartupHook.IsDivertSite("M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2929"), "route/atk_power_a");
        Check(StartupHook.IsDivertSite("M2Server.Actor.BaseObject._Attack@BaseObject.Attack.cs:41"), "route/hit_gate");
        Check(StartupHook.IsDivertSite("M2Server.Player.PlayObject.AttackSuccess@PlayObject.Attack.cs:223"), "route/attack_success");
        Check(!StartupHook.IsDivertSite("M2Server.Player.PlayObject.AttackSuccess@PlayObject.Attack.cs:221"), "route/attack_paralysis-not-diverted");
        Check(!StartupHook.IsDivertSite("M2Server.Player.PlayObject.StruckDamage@PlayObject.cs:9999"), "route/unknown-line-not-diverted");
        Check(!StartupHook.IsDivertSite("AnimalObject.Wondering@AnimalObject.cs:427"), "route/noise-not-diverted");
        Check(!StartupHook.IsDivertSite(null), "route/null-not-diverted");
        Check(!StartupHook.IsDivertSite(""), "route/empty-not-diverted");

        Console.WriteLine(_failed == 0 ? "SELFTEST GREEN" : ("SELFTEST RED failed=" + _failed));
        return _failed == 0 ? 0 : 1;
    }
}
