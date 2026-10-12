using System;

namespace BotSrv
{
    /// <summary>
    /// M3②③ 攻击夹具的 attacker 选择自检（白名单 T-29 同族扩展；清 reviewer-5 `76e433f0` Minor 2）。
    ///
    /// 背景：旧实现 `ArmAttackSequence` 用 `LoginId.EndsWith(g_nAttackRole)` 选 attacker；
    /// `-Bots&gt;10` 时序号 10/20… 也以 attacker 序号结尾 ⇒ 被误武装成多个 attacker，污染
    /// `attack_diff.py` 的「第 k 次」对齐。现改为 `MShare.IsAttackAttacker` 的**精确相等**。
    ///
    /// 入口：`BotSrv.exe --selftest-attackrole`（不启动任何服务/不连端口；PASS ⇒ exit 0，
    /// 任一用例不符 ⇒ exit 1）。`run_attack_pair.ps1 -SelfCheckNoStack` 调用它做「改坏必红」的
    /// **生产函数级**红检（把相等改回 EndsWith ⇒ 必 RED）。
    /// </summary>
    internal static class AttackRoleSelfCheck
    {
        public static int Run()
        {
            // (loginId, attackerLoginId, want)
            var cases = new (string login, string role, bool want)[]
            {
                ("winplay0", "winplay0", true),    // 正常：唯一 attacker
                ("winplay1", "winplay1", true),    // 序号 >0 的 attacker 也精确命中
                ("winplay10", "winplay0", false),  // -Bots>10 回归：旧 EndsWith('0') 会误判 true
                ("winplay20", "winplay0", false),  // 同上
                ("winplay1", "winplay10", false),  // 精确反向：1 不得命中 10
                ("winplay10", "winplay10", true),  // 精确命中 10
                ("winplay0", "", false),           // 空配置 = 未武装
                ("", "winplay0", false),           // 空 LoginId 永不命中
            };

            int failed = 0;
            foreach (var c in cases)
            {
                bool got = MShare.IsAttackAttacker(c.login, c.role);
                bool ok = got == c.want;
                if (!ok) { failed++; }
                Console.WriteLine($"ATTACKROLE {(ok ? "PASS" : "FAIL")} login='{c.login}' attacker='{c.role}' got={got} want={c.want}");
            }
            Console.WriteLine(failed == 0
                ? "ATTACKROLE SELFTEST GREEN"
                : $"ATTACKROLE SELFTEST RED ({failed} failed)");
            return failed == 0 ? 0 : 1;
        }
    }
}
