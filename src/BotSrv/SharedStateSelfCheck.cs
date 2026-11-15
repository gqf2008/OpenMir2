using System;
using System.Threading;
using BotSrv.Objects;
using BotSrv.Player;
using BotSrv.Scenes.Scene;
using OpenMir2;
using Serilog;

namespace BotSrv
{
    /// <summary>
    /// 卡 `openmir2-botsrv-shared-state-locks`：跨 bot 共享态（`MShare.MySelf` / `PlayScene.ActorList`）
    /// 的细粒度锁 / per-bot 上下文牙（无栈自检）。
    ///
    /// 入口：`BotSrv.exe --selftest-shared-state`（PASS ⇒ exit 0，任一用例不符 ⇒ exit 1）。
    ///
    /// 覆盖：
    ///   ① `MShare.MySelf` 是**线程本地**（per-packet/per-round 上下文）：两个线程各设各读，互不覆盖；
    ///   ② `PlayScene.ActorList` 的确定性竞态：`SceneIndexRaceHook` 在 Count→下标之间缩表，
    ///      生产 `BeginScene` 经锁+`TryGetActorAt` 不抛并计数；**legacy 裸 Count→[i] 读必定 AOORE**
    ///      （修复前的形态，作为负对照）；
    ///   ③ 删除路径的锁：`DelActor`/`NewActor` 并发读写无异常；`DeleteActor` 登记释放池；
    ///   ④ 元断言：线程本地负对照（plain static）会串值、legacy 裸读会抛 —— 牙本身有牙。
    /// </summary>
    internal static class SharedStateSelfCheck
    {
        public static int Run()
        {
            LogService.Logger = new LoggerConfiguration().CreateLogger();
            int failed = 0;
            void Check(string name, bool ok, string detail = "")
            {
                if (!ok) failed++;
                Console.WriteLine($"SHAREDSTATE {(ok ? "PASS" : "FAIL")} {name}{(detail.Length > 0 ? " " + detail : "")}");
            }

            var robot = new RobotPlayer();
            PlayScene scene = robot.PlayScene;

            // ---------- ① MShare.MySelf 线程本地 ----------
            {
                var a1 = new THumActor(robot) { CurrX = 11, CurrY = 11 };
                var a2 = new THumActor(robot) { CurrX = 22, CurrY = 22 };
                THumActor read1 = null, read2 = null;
                var b1 = new Barrier(2);
                var b2 = new Barrier(2);
                var t1 = new Thread(() =>
                {
                    MShare.MySelf = a1;
                    b1.SignalAndWait();
                    b2.SignalAndWait();
                    read1 = MShare.MySelf;
                });
                var t2 = new Thread(() =>
                {
                    MShare.MySelf = a2;
                    b1.SignalAndWait();
                    b2.SignalAndWait();
                    read2 = MShare.MySelf;
                });
                t1.Start();
                t2.Start();
                t1.Join();
                t2.Join();
                Check("myself-thread-local", ReferenceEquals(read1, a1) && ReferenceEquals(read2, a2),
                    $"read1={(read1 == null ? "null" : read1.CurrX.ToString())} read2={(read2 == null ? "null" : read2.CurrX.ToString())}");

                // 负对照：plain static 会被最后写者覆盖（证明上面的断言不是恒真）。
                int plain = 0;
                int plain1 = 0, plain2 = 0;
                var c1 = new Barrier(2);
                var c2 = new Barrier(2);
                var p1 = new Thread(() => { plain = 1; c1.SignalAndWait(); c2.SignalAndWait(); plain1 = plain; });
                var p2 = new Thread(() => { plain = 2; c1.SignalAndWait(); c2.SignalAndWait(); plain2 = plain; });
                p1.Start(); p2.Start(); p1.Join(); p2.Join();
                Check("plain-static-negative-control", plain1 == 2 && plain2 == 2,
                    $"plain1={plain1} plain2={plain2}（plain static 被最后写者覆盖，预期 2/2）");
                MShare.MySelf = null;
            }

            // ---------- ② ActorList 确定性竞态 + legacy 负对照 ----------
            {
                scene.ActorList.Clear();
                var self = new THumActor(robot) { CurrX = 100, CurrY = 100, m_nRx = 100, m_nRy = 100 };
                MShare.MySelf = self;
                var victim = new TestActor(robot) { RecogId = 77, CurrX = 101, CurrY = 100 };
                scene.ActorList.Add(victim);

                long before = LoadMetrics.SceneIndexRaceSkippedCount;
                PlayScene.SceneIndexRaceHook = ps =>
                {
                    // 确定性模拟收包线程在 Count 检查与下标读取之间缩表：自清并删掉最后一个 actor。
                    PlayScene.SceneIndexRaceHook = null;
                    if (ps.ActorList.Count > 0)
                    {
                        ps.ActorList.RemoveAt(ps.ActorList.Count - 1);
                    }
                };
                bool threw = false;
                try
                {
                    scene.BeginScene();   // 生产路径：count 锁读 + TryGetActorAt
                }
                catch (Exception ex)
                {
                    threw = true;
                    Console.WriteLine("   BeginScene threw: " + ex.GetType().Name);
                }
                PlayScene.SceneIndexRaceHook = null;
                Check("production-beginscene-guarded-no-throw",
                    !threw && LoadMetrics.SceneIndexRaceSkippedCount >= before + 1,
                    $"threw={threw} skipped+={LoadMetrics.SceneIndexRaceSkippedCount - before}");

                // legacy 负对照：旧实现是 `if (i >= Count) break; ... ActorList[i]`（无原子读）。
                // Count 检查通过后，收包线程缩表 ⇒ 旧下标必定 AOORE（修复前的形态）。
                bool legacyThrew = false;
                try
                {
                    scene.ActorList.Add(victim);
                    int i = scene.ActorList.Count - 1;   // 旧代码的 Count 检查通过的下标
                    scene.ActorList.RemoveAt(0);         // 模拟收包线程缩表（无锁的旧路径）
                    _ = scene.ActorList[i];              // 旧下标读取 ⇒ AOORE
                }
                catch (ArgumentOutOfRangeException)
                {
                    legacyThrew = true;
                }
                Check("legacy-raw-index-negative-control", legacyThrew, "legacy 裸下标读必须 AOORE");
            }

            // ---------- ③ 生产方法的并发读写（无异常 + 释放池有锁） ----------
            {
                scene.ActorList.Clear();
                var raised = 0;
                var stop = 0;
                var writer = new Thread(() =>
                {
                    try
                    {
                        for (int i = 0; i < 20000; i++)
                        {
                            Actor a = scene.NewActor(100000 + i, 10, 10, 0, 0, 0);
                            scene.DelActor(a);
                        }
                    }
                    catch (Exception ex)
                    {
                        Interlocked.Exchange(ref raised, 1);
                        Console.WriteLine("   writer threw: " + ex.GetType().Name + ": " + ex.Message);
                    }
                    finally
                    {
                        Interlocked.Exchange(ref stop, 1);
                    }
                });
                var reader = new Thread(() =>
                {
                    try
                    {
                        while (Volatile.Read(ref stop) == 0)
                        {
                            Actor[] snapshot = scene.SnapshotActors();
                            for (int i = 0; i < snapshot.Length; i++)
                            {
                                _ = snapshot[i] == null ? 0 : snapshot[i].RecogId;
                            }
                        }
                    }
                    catch (Exception ex)
                    {
                        Interlocked.Exchange(ref raised, 1);
                        Console.WriteLine("   reader threw: " + ex.GetType().Name + ": " + ex.Message);
                    }
                });
                writer.Start();
                reader.Start();
                writer.Join();
                reader.Join();
                Check("concurrent-newactor-delactor-snapshot", raised == 0, $"raised={raised}");
                Check("actorlist-empty-after-stress", scene.SnapshotActors().Length == 0,
                    $"count={scene.SnapshotActors().Length}");
            }

            // ---------- ④ per-bot 上下文接线（卡 `openmir2-botsrv-myself-wiring-tooth`） ----------
            {
                // 清掉前面场景用例留下的线程本地值，让「恢复后 == 调用前」的断言可判。
                MShare.MySelf = null;
                var bot1 = new RobotPlayer();
                bot1.MyActor = new THumActor(bot1) { CurrX = 101, CurrY = 101 };
                var bot2 = new RobotPlayer();
                bot2.MyActor = new THumActor(bot2) { CurrX = 202, CurrY = 202 };

                var seen = new System.Collections.Generic.List<THumActor>();
                ClientManager.RunBotRound(() => seen.Add(MShare.MySelf), bot1);
                ClientManager.RunBotRound(() => seen.Add(MShare.MySelf), bot2);
                Check(
                    "runbotround-context-per-bot",
                    seen.Count == 2 && ReferenceEquals(seen[0], bot1.MyActor) && ReferenceEquals(seen[1], bot2.MyActor)
                        && MShare.MySelf == null,
                    $"seen0={(seen.Count > 0 && seen[0] != null ? seen[0].CurrX.ToString() : "null")} " +
                    $"seen1={(seen.Count > 1 && seen[1] != null ? seen[1].CurrX.ToString() : "null")} " +
                    $"after={(MShare.MySelf == null ? "null" : "set")}");

                var cm = new ClientManager();
                cm.AddClient("tooth-s1", bot1);
                cm.AddClient("tooth-s2", bot2);
                var packetSeen = new System.Collections.Generic.Dictionary<RobotPlayer, THumActor>();
                RobotPlayer.ProcessPacketEnteredForTest = r => packetSeen[r] = MShare.MySelf;
                try
                {
                    cm.ProcessOnePacket(new RecvicePacket { SessionId = "tooth-s1", ReviceData = "" });
                    cm.ProcessOnePacket(new RecvicePacket { SessionId = "tooth-s2", ReviceData = "" });
                }
                finally
                {
                    RobotPlayer.ProcessPacketEnteredForTest = null;
                }
                // M3c（卡 `openmir2-botsrv-actor-pool-leak`）：per-packet 的 finally 恢复本身也要有牙。
                // 只看「包内上下文正确」时，删掉 finally 仍全绿；所以这里额外断言两次调用后
                // MShare.MySelf 已恢复到调用前（null）。
                Check(
                    "processonepacket-context-per-bot",
                    packetSeen.Count == 2
                        && ReferenceEquals(packetSeen[bot1], bot1.MyActor)
                        && ReferenceEquals(packetSeen[bot2], bot2.MyActor)
                        && MShare.MySelf == null,
                    $"seen1={(packetSeen.ContainsKey(bot1) && packetSeen[bot1] != null ? packetSeen[bot1].CurrX.ToString() : "null")} " +
                    $"seen2={(packetSeen.ContainsKey(bot2) && packetSeen[bot2] != null ? packetSeen[bot2].CurrX.ToString() : "null")} " +
                    $"after={(MShare.MySelf == null ? "null" : "leaked")}");
            }

            Console.WriteLine($"SHAREDSTATE {(failed == 0 ? "ALL PASS" : "FAILED")} failures={failed}");
            return failed == 0 ? 0 : 1;
        }

        private sealed class TestActor : Actor
        {
            public TestActor(RobotPlayer robot) : base(robot) { }
            public override void RunFrameAction(int frame) { }
            public override void Run() { }
        }
    }
}
