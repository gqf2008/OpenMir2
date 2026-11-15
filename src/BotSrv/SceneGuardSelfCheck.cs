using System;
using System.Reflection;
using Serilog;
using BotSrv.Objects;
using BotSrv.Player;
using BotSrv.Scenes.Scene;
using OpenMir2;

namespace BotSrv
{
    /// <summary>
    /// 场景空 Actor 健壮性自检（卡 `openmir2-botsrv-autoplay-nre`）。
    ///
    /// 归因（2026-11-15 总监裁定）：PlayScene.BeginScene 的 NRE 是**夹具侧**问题——
    /// 真实 Delphi 客户端 `PlayScn.pas:1125-1176` 的 ActorList 只在 UI 线程读写（不会出现 null），
    /// 且对「帧引用不存在的 actor」显式跳过（`PlayScn.pas:3670-3690`：创建不出 ⇒ `if Actor = nil then Exit`）。
    /// BotSrv 的收包线程与 run 线程并发增删 `ActorList`、共享 `MShare.MySelf`，才会出现空 Actor。
    ///
    /// 覆盖：
    ///   ① 帧引用不存在的 actor：SM_FEATURECHANGED 必须静默跳过（不新增 actor、不抛）；
    ///      SM_TURN 等位置帧按 NewActor 语义补建（等价 `PlayScn.pas:3670-3672`）。
    ///   ② ActorList 的 null 槽位：BeginScene 跳过 + 计数，不抛 NRE，其余 actor 照常处理。
    ///   ③ 循环中途 `MShare.MySelf` 被置 null：BeginScene 用轮首快照，不抛（真实客户端单线程语义）。
    ///
    /// 入口：`BotSrv.exe --selftest-scene-null`（不启动服务/不连端口）。
    /// 改坏必红：删 BeginScene 的 null 槽位守卫 ⇒ ② 红；把快照改回 `MShare.MySelf` ⇒ ③ 红；
    /// 删 `PlayScene.SendMsg` 的 `if (actor == null) return;` ⇒ ① 红；
    /// 删删除点的「下标/身份核对」⇒ ④ 红（会误删下一个 actor 或抛 AOORE）。
    /// </summary>
    internal static class SceneGuardSelfCheck
    {
        private sealed class TestActor : Actor
        {
            public int Runs;
            public bool NullSelfOnRun;
            public PlayScene SceneRef;
            public bool RemoveSelfOnRun;
            public TestActor(RobotPlayer robot) : base(robot) { }
            public override void RunFrameAction(int frame) { }
            public override void Run()
            {
                Runs++;
                if (NullSelfOnRun)
                {
                    MShare.MySelf = null;
                }
                if (RemoveSelfOnRun && SceneRef != null)
                {
                    SceneRef.ActorList.Remove(this);
                }
            }
        }

        private static void SetMoveTimeNow(PlayScene scene)
        {
            FieldInfo f = typeof(PlayScene).GetField("m_dwMoveTime", BindingFlags.NonPublic | BindingFlags.Instance);
            if (f != null)
            {
                f.SetValue(scene, HUtil32.GetTickCount());
            }
        }

        public static int Run()
        {
            LogService.Logger = new LoggerConfiguration().CreateLogger();
            int failed = 0;
            void Check(string name, bool ok, string detail)
            {
                if (!ok) { failed++; }
                Console.WriteLine($"SCENEGUARD {(ok ? "PASS" : "FAIL")} {name} {detail}");
            }

            var robot = new RobotPlayer();
            PlayScene scene = robot.PlayScene;
            var self = new THumActor(robot);
            self.CurrX = 100; self.CurrY = 100; self.m_nRx = 100; self.m_nRy = 100;
            MShare.MySelf = self;

            // ① 帧引用不存在的 actor：非位置帧静默跳过；位置帧按 NewActor 语义补建。
            bool threw = false;
            try
            {
                scene.SendMsg(Messages.SM_FEATURECHANGED, 987654, 0, 0, 0, 123, 0, "", 0);
            }
            catch (Exception e)
            {
                threw = true;
                Console.WriteLine("   featurechanged threw: " + e.GetType().Name);
            }
            Check("unknown-frame-featurechanged-skipped",
                !threw && scene.FindActor(987654) == null,
                $"threw={threw} found={scene.FindActor(987654) != null} want skip");

            threw = false;
            try
            {
                scene.SendMsg(Messages.SM_TURN, 987655, 101, 102, 0, 0, 0, "", 0);
            }
            catch (Exception e)
            {
                threw = true;
                Console.WriteLine("   turn threw: " + e.GetType().Name);
            }
            Actor created = scene.FindActor(987655);
            Check("unknown-frame-turn-creates",
                !threw && created != null && created.CurrX == 101 && created.CurrY == 102,
                $"threw={threw} found={created != null}");

            // ② null 槽位：跳过 + 计数，其余 actor 照常处理，不抛。
            scene.ActorList.Clear();
            var valid = new TestActor(robot) { CurrX = 100, CurrY = 100, m_nRx = 100, m_nRy = 100, RecogId = 987656 };
            scene.ActorList.Add(valid);
            scene.ActorList.Add(null);
            long before = LoadMetrics.SceneNullActorSkippedCount;
            threw = false;
            try
            {
                SetMoveTimeNow(scene);
                scene.BeginScene();
            }
            catch (Exception e)
            {
                threw = true;
                Console.WriteLine("   beginscene threw: " + e.GetType().Name + " @ " + e.StackTrace);
            }
            Check("null-actor-slot-skipped",
                !threw && LoadMetrics.SceneNullActorSkippedCount == before + 1 && valid.Runs == 1,
                $"threw={threw} skipped+={LoadMetrics.SceneNullActorSkippedCount - before} runs={valid.Runs} want skip+1 runs=1");

            // ③ 循环中途 MySelf 变 null：轮首快照保证不抛（真实客户端单线程语义）。
            scene.ActorList.Clear();
            var nuller = new TestActor(robot) { CurrX = 100, CurrY = 100, m_nRx = 100, m_nRy = 100, RecogId = 987657, NullSelfOnRun = true };
            scene.ActorList.Add(nuller);
            MShare.MySelf = self;
            threw = false;
            try
            {
                SetMoveTimeNow(scene);
                scene.BeginScene();
            }
            catch (Exception e)
            {
                threw = true;
                Console.WriteLine("   self-null threw: " + e.GetType().Name + " @ " + e.StackTrace);
            }
            Check("self-null-during-run-no-crash",
                !threw && nuller.Runs == 1,
                $"threw={threw} runs={nuller.Runs} want no-throw runs=1");

            // ④ 删除点并发一致性：Run() 自己把 actor 从列表移除（模拟收包线程/同线程重入），
            //    旧代码会 RemoveAt(i) 误删下一个 actor（或下标越界抛 AOORE）；修复后跳过并计数。
            scene.ActorList.Clear();
            MShare.MySelf = self;
            var keep = new TestActor(robot) { CurrX = 100, CurrY = 100, m_nRx = 100, m_nRy = 100, RecogId = 987658 };
            var remover = new TestActor(robot) { CurrX = 200, CurrY = 200, m_nRx = 200, m_nRy = 200, RecogId = 987659, SceneRef = scene, RemoveSelfOnRun = true };
            scene.ActorList.Add(remover);
            scene.ActorList.Add(keep);
            long rb = LoadMetrics.SceneListRaceSkippedCount;
            threw = false;
            try
            {
                SetMoveTimeNow(scene);
                scene.BeginScene();
            }
            catch (Exception e)
            {
                threw = true;
                Console.WriteLine("   list-race-1 threw: " + e.GetType().Name + " @ " + e.StackTrace);
            }
            Check("list-race-index-changed-skipped",
                !threw && LoadMetrics.SceneListRaceSkippedCount == rb + 1
                && scene.ActorList.Count == 1 && scene.ActorList[0] == keep,
                $"threw={threw} skipped+={LoadMetrics.SceneListRaceSkippedCount - rb} count={scene.ActorList.Count} keep={(scene.ActorList.Count == 1 && scene.ActorList[0] == keep)}");

            scene.ActorList.Clear();
            MShare.MySelf = self;
            var keep2 = new TestActor(robot) { CurrX = 100, CurrY = 100, m_nRx = 100, m_nRy = 100, RecogId = 987660 };
            var remover2 = new TestActor(robot) { CurrX = 200, CurrY = 200, m_nRx = 200, m_nRy = 200, RecogId = 987661, SceneRef = scene, RemoveSelfOnRun = true };
            scene.ActorList.Add(keep2);
            scene.ActorList.Add(remover2);
            long rb2 = LoadMetrics.SceneListRaceSkippedCount;
            threw = false;
            try
            {
                SetMoveTimeNow(scene);
                scene.BeginScene();
            }
            catch (Exception e)
            {
                threw = true;
                Console.WriteLine("   list-race-2 threw: " + e.GetType().Name + " @ " + e.StackTrace);
            }
            Check("list-race-index-out-of-range-skipped",
                !threw && LoadMetrics.SceneListRaceSkippedCount == rb2 + 1
                && scene.ActorList.Count == 1 && scene.ActorList[0] == keep2,
                $"threw={threw} skipped+={LoadMetrics.SceneListRaceSkippedCount - rb2} count={scene.ActorList.Count}");

            MShare.MySelf = self;
            scene.ActorList.Clear();
            MShare.MySelf = null;
            Console.WriteLine(failed == 0 ? "SCENEGUARD SELFTEST PASS" : $"SCENEGUARD SELFTEST FAIL ({failed})");
            return failed == 0 ? 0 : 1;
        }
    }
}
