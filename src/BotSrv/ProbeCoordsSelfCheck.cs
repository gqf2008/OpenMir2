using System;
using Serilog;
using BotSrv.Player;
using OpenMir2;

namespace BotSrv
{
    /// <summary>
    /// 动作探针坐标跟随的自检（卡 `openmir2-botsrv-action-probe-turn-coords`）。
    ///
    /// 覆盖**生产同一实现**：
    ///   ① `ProbeCoords.TrySelfPosition`——哪些帧改本假人的服务端坐标（正例 + 负例）；
    ///   ② `RobotPlayer.ApplySelfMoveFrame`——把判定写进探针坐标（生产唯一应用点）；
    ///   ③ `RobotPlayer.MarkInWorld`——同一 actor/连接**再收一次** `SM_LOGON` 时落点要跟着刷新
    ///      （只刷新坐标，进世界计数仍只记一次）；
    ///   ④ `MarkInWorld` 的**身份守卫**（F3）——同一登录连接内收到**别的 actor** 的 `SM_LOGON`
    ///      必须被拒（坐标与 actor 不变、`logon_misdelivered` +1）；换新连接时允许合法重登并重新绑定。
    ///
    /// 入口：`BotSrv.exe --selftest-probecoords`（不启动服务/不连端口；PASS ⇒ exit 0，任一用例不符 ⇒ exit 1）。
    ///
    /// 改坏必红（本机实测，删除变异各自重建后 rc=1；FAIL 名单与卡面 patch 的变异表逐条一致）：
    ///   · 删 `TrySelfPosition` 的 `recog != myActorId` 守卫 ⇒ 3 条红：
    ///     `pure-other-actor-not-applied` / `pure-no-actor-not-applied` / `apply-other-actor-unchanged`；
    ///   · 删 `ApplySelfMoveFrame` 的 `_probeX/_probeY` 写回（保留计数）⇒ 2 条红：
    ///     `apply-self-backstep` / `apply-self-spacemove`；
    ///   · 把 `MarkInWorld` 的坐标刷新挪到 `_inWorldCounted` 门之后 ⇒ 2 条红：
    ///     `relogon-refresh` / `relogon-new-connection-rebinds`；
    ///   · 删 `MarkInWorld` 的身份守卫 ⇒ `misdelivered-actor-rejected` 红；
    ///   · 只删守卫里的 `LoadMetrics.LogonMisdelivered()`（保留拒绝）⇒ `misdelivered-actor-rejected` 红
    ///     （计数牙与拒绝牙分开验证）。
    /// </summary>
    internal static class ProbeCoordsSelfCheck
    {
        public static int Run()
        {
            // 生产应用点会走 `MainOutMessage`（Serilog）；自检不起 AppServer ⇒ 这里给一个**无 sink** 的
            // 最小 logger（与 QuestSeqSelfCheck 同法），否则 `LogService.Logger` 为 null 会 NRE。
            LogService.Logger = new LoggerConfiguration().CreateLogger();
            int failed = 0;

            void Check(string name, bool ok, string detail)
            {
                if (!ok) { failed++; }
                Console.WriteLine($"PROBECOORDS {(ok ? "PASS" : "FAIL")} {name} {detail}");
            }

            // ① 纯判定：服务端驱动位移的两类帧，只对本假人生效。
            var cases = new (string name, int ident, int recog, int p, int t, int self, bool want, int wx, int wy)[]
            {
                ("self-backstep", Messages.SM_BACKSTEP, 77, 287, 618, 77, true, 287, 618),
                ("self-spacemove-show", Messages.SM_SPACEMOVE_SHOW, 77, 650, 629, 77, true, 650, 629),
                ("self-spacemove-show2", Messages.SM_SPACEMOVE_SHOW2, 77, 651, 630, 77, true, 651, 630),
                ("other-actor-not-applied", Messages.SM_BACKSTEP, 78, 287, 618, 77, false, 0, 0),
                ("no-actor-not-applied", Messages.SM_BACKSTEP, 77, 287, 618, 0, false, 0, 0),
                ("turn-does-not-move", Messages.SM_TURN, 77, 287, 618, 77, false, 0, 0),
                ("walk-frame-not-ours", Messages.SM_WALK, 77, 287, 618, 77, false, 0, 0),
            };
            foreach (var c in cases)
            {
                bool got = ProbeCoords.TrySelfPosition(c.ident, c.recog, c.p, c.t, c.self, out int x, out int y);
                Check(
                    "pure-" + c.name,
                    got == c.want && x == c.wx && y == c.wy,
                    $"got={got} ({x},{y}) want={c.want} ({c.wx},{c.wy})");
            }

            // ② 生产应用点：判定的结果必须真的写进探针坐标。
            var robot = new RobotPlayer();
            robot.SetProbeForTest(1001, 10, 20);
            robot.ApplySelfMoveFrame(Messages.SM_BACKSTEP, 1001, 287, 618);
            Check("apply-self-backstep", robot.ProbeXForTest == 287 && robot.ProbeYForTest == 618,
                $"({robot.ProbeXForTest},{robot.ProbeYForTest}) want (287,618)");
            robot.ApplySelfMoveFrame(Messages.SM_SPACEMOVE_SHOW2, 1001, 650, 629);
            Check("apply-self-spacemove", robot.ProbeXForTest == 650 && robot.ProbeYForTest == 629,
                $"({robot.ProbeXForTest},{robot.ProbeYForTest}) want (650,629)");
            int beforeX = robot.ProbeXForTest;
            int beforeY = robot.ProbeYForTest;
            robot.ApplySelfMoveFrame(Messages.SM_BACKSTEP, 9999, 1, 2);   // 别人的推挤
            Check("apply-other-actor-unchanged", robot.ProbeXForTest == beforeX && robot.ProbeYForTest == beforeY,
                $"({robot.ProbeXForTest},{robot.ProbeYForTest}) want ({beforeX},{beforeY})");

            // ③ 同 actor 重发：同一连接再收一次 SM_LOGON ⇒ 探针落点跟到最新坐标。
            var again = new RobotPlayer();
            bool first = again.MarkInWorldOnConnectionForTest(2001, 297, 616, 7);
            bool second = again.MarkInWorldOnConnectionForTest(2001, 649, 629, 7);
            Check("relogon-refresh",
                first && second && again.MyActorIdForTest == 2001
                && again.ProbeXForTest == 649 && again.ProbeYForTest == 629,
                $"first=({297},{616}) now=({again.ProbeXForTest},{again.ProbeYForTest}) actor={again.MyActorIdForTest} want (649,629)");

            // ④ 身份守卫（F3）：同一连接内换 actor = 错投 ⇒ 拒绝 + 计数 +1 + 坐标/actor 不变。
            var mis = new RobotPlayer();
            long misBefore = LoadMetrics.LogonMisdeliveredCount;
            bool bind = mis.MarkInWorldOnConnectionForTest(4001, 10, 20, 11);
            bool wrong = mis.MarkInWorldOnConnectionForTest(4002, 99, 99, 11);
            Check("misdelivered-actor-rejected",
                bind && !wrong && mis.MyActorIdForTest == 4001
                && mis.ProbeXForTest == 10 && mis.ProbeYForTest == 20
                && LoadMetrics.LogonMisdeliveredCount == misBefore + 1,
                $"bind={bind} wrong={wrong} actor={mis.MyActorIdForTest} ({mis.ProbeXForTest},{mis.ProbeYForTest}) count={LoadMetrics.LogonMisdeliveredCount - misBefore} want reject+1");

            // ⑤ 新连接（ConnSeq 变化）= 合法重登 ⇒ 允许重新绑定到新 actor/落点。
            bool relog = mis.MarkInWorldOnConnectionForTest(4003, 30, 40, 12);
            Check("relogon-new-connection-rebinds",
                relog && mis.MyActorIdForTest == 4003
                && mis.ProbeXForTest == 30 && mis.ProbeYForTest == 40,
                $"relog={relog} actor={mis.MyActorIdForTest} ({mis.ProbeXForTest},{mis.ProbeYForTest}) want 4003 (30,40)");

            Console.WriteLine(failed == 0 ? "PROBECOORDS SELFTEST PASS" : $"PROBECOORDS SELFTEST FAIL ({failed})");
            return failed == 0 ? 0 : 1;
        }
    }
}
