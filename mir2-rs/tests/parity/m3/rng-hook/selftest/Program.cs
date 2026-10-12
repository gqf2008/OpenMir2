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

        // ⑤ F-N1：专用流按「被测量攻方线程」收窄（怪物 MobThread 的取数不进玩家专用流）。
        //    牙齿：去掉线程过滤（只看调用点）⇒ 下面 4 条非玩家/异线程断言必红。
        const string playerStart = "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2929";
        const string playerStartLuck = "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2918";
        const string hitGate = "M2Server.Actor.BaseObject._Attack@BaseObject.Attack.cs:41";
        const string baseAtk = "M2Server.Actor.BaseObject.GetAttackPower@BaseObject.Base.cs:892";
        Check(StartupHook.IsPlayerAttackStartSite(playerStart), "entity/player-start-2929");
        Check(StartupHook.IsPlayerAttackStartSite(playerStartLuck), "entity/player-start-2918");
        Check(!StartupHook.IsPlayerAttackStartSite(hitGate), "entity/hit-gate-not-start");
        Check(!StartupHook.IsPlayerAttackStartSite(baseAtk), "entity/base-not-start");
        Check(!StartupHook.IsPlayerAttackStartSite(null), "entity/null-not-start");

        StartupHook.ResetAttackThreadForTest();
        Check(StartupHook.AttackThreadIdForTest() == -1, "entity/unclaimed-init=-1");
        // 未认领前：任何分流站点都不进专用流（避免起跑前被别图怪污染）。
        Check(!StartupHook.RouteToAttackStream(baseAtk, true, 7), "entity/base-before-player-not-diverted");
        Check(!StartupHook.RouteToAttackStream(hitGate, true, 7), "entity/shared-before-player-not-diverted");
        // 玩家首抽认领线程 5（自身必须进专用流）。
        Check(StartupHook.RouteToAttackStream(playerStart, true, 5), "entity/player-start-diverted");
        Check(StartupHook.AttackThreadIdForTest() == 5, "entity/claimed-thread=5");
        // 同线程的共享/后续站点进专用流。
        Check(StartupHook.RouteToAttackStream(hitGate, true, 5), "entity/shared-same-thread-diverted");
        Check(StartupHook.RouteToAttackStream("M2Server.Player.PlayObject.AttackSuccess@PlayObject.Attack.cs:223", true, 5), "entity/succ-same-thread-diverted");
        // 异线程（怪物 MobThread id=7）的共享/非玩家站点一律不走专用流。
        Check(!StartupHook.RouteToAttackStream(hitGate, true, 7), "entity/shared-other-thread-not-diverted");
        Check(!StartupHook.RouteToAttackStream(baseAtk, true, 7), "entity/base-other-thread-not-diverted");
        // 下一击可能换线程：新首抽重新认领（ThreadPool 复用）。
        Check(StartupHook.RouteToAttackStream(playerStartLuck, true, 9), "entity/reclaim-thread-9");
        Check(StartupHook.AttackThreadIdForTest() == 9, "entity/reclaimed-thread=9");
        Check(!StartupHook.RouteToAttackStream(hitGate, true, 5), "entity/old-thread-no-longer-diverted");
        // 非分流站点在任何线程都不进专用流。
        Check(!StartupHook.RouteToAttackStream("M2Server.Player.PlayObject.AttackSuccess@PlayObject.Attack.cs:221", false, 9), "entity/non-divert-site-never");
        StartupHook.ResetAttackThreadForTest();

        Console.WriteLine(_failed == 0 ? "SELFTEST GREEN" : ("SELFTEST RED failed=" + _failed));
        return _failed == 0 ? 0 : 1;
    }
}
