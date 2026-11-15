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
    ///   ③ 循环中途 `MShare.MySelf` 被置 null：BeginScene 用轮首快照，不抛（真实客户端单线程语义）；
    ///   ④ 删除点被并发改动（自删/下标前移）：跳过且不重复归属 g_FreeActorList；
    ///   ⑤ 轮内 `ActorList[i]` 读取前被并发缩表：跳过 + 计数，不抛 AOORE（F1）；
    ///   ⑥a 删除点条件读取 `ActorList[i]` 前被并发缩表（F1c）；⑥b 身份核对后、RemoveAt 前
    ///      被缩表（F1b）：都必须计数跳过、不登记 FreeActorList、不双归属；
    ///   ⑦ ActorDied 的删除/插入循环遇并发缩表：计数跳过，不抛 AOORE；
    ///   ⑧ RemoveAt 失败后 actor 仍在列表：不再 Insert/Add（不重复插入，P3-2）；
    ///   ⑨ ActorDied 插入循环遇 null 槽位：跳过并计数，不解引用（P3-1）。
    ///
    /// 归因精度（F3）：隔离注入 null 槽位复现为 NRE@121、ActorList=null 为 NRE@116；
    /// 归档里的 :120 是**并发索引读取竞争**的签名，不是 null 槽位的直接证据。
    ///
    /// 入口：`BotSrv.exe --selftest-scene-null`（不启动服务/不连端口）。
    /// 改坏必红：删 BeginScene 的 null 槽位守卫 ⇒ ② 红；把快照改回 `MShare.MySelf` ⇒ ③ 红；
    /// 删 `PlayScene.SendMsg` 的 `if (actor == null) return;` ⇒ ① 红；
    /// 删删除点的「下标/身份核对」⇒ ④ 红（会误删/双归属）；
    /// 把 FreeActorList 登记挪回核对之前 ⇒ ④c 红（双归属）；
    /// 删轮内读取的 try/catch ⇒ ⑤ 红（AOORE）；删删除路径 try/catch ⇒ ⑥ 红（AOORE）；
    /// 把 FreeActorList 登记挪到 RemoveAt 之前 ⇒ ⑥b 红（双归属）；
    /// 删删除路径外层 try/catch ⇒ ⑥a/⑥b 红（AOORE）；删 ActorDied 的 try/catch ⇒ ⑦ 红；
    /// 删「仍存在则不再插入」判断 ⇒ ⑧ 红（重复插入）；删插入循环 null 判断 ⇒ ⑨ 红（NRE）。
    /// </summary>
    internal static class SceneGuardSelfCheck
    {
        private sealed class TestActor : Actor
        {
            public int Runs;
            public bool NullSelfOnRun;
            public PlayScene SceneRef;
            public bool RemoveSelfOnRun;
            public Actor RemoveTargetOnRun;
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
                if (RemoveTargetOnRun != null && SceneRef != null)
                {
                    SceneRef.ActorList.Remove(RemoveTargetOnRun);
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
                && scene.ActorList.Count == 1 && scene.ActorList[0] == keep
                && !MShare.g_FreeActorList.Contains(remover),
                $"threw={threw} skipped+={LoadMetrics.SceneListRaceSkippedCount - rb} count={scene.ActorList.Count} free={MShare.g_FreeActorList.Contains(remover)}");

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

            // ④c 下标前移：前面元素被删 ⇒ actor 仍在列表里，旧代码会把它登记进 free-list（双归属）。
            scene.ActorList.Clear();
            MShare.MySelf = self;
            var shifted = new TestActor(robot) { CurrX = 100, CurrY = 100, m_nRx = 100, m_nRy = 100, RecogId = 987662 };
            var pusher = new TestActor(robot) { CurrX = 200, CurrY = 200, m_nRx = 200, m_nRy = 200, RecogId = 987663, SceneRef = scene, RemoveTargetOnRun = shifted };
            scene.ActorList.Add(shifted);
            scene.ActorList.Add(pusher);
            long rb3 = LoadMetrics.SceneListRaceSkippedCount;
            threw = false;
            try
            {
                SetMoveTimeNow(scene);
                scene.BeginScene();
            }
            catch (Exception e)
            {
                threw = true;
                Console.WriteLine("   list-race-3 threw: " + e.GetType().Name + " @ " + e.StackTrace);
            }
            Check("list-race-shift-skip-no-double-claim",
                !threw && LoadMetrics.SceneListRaceSkippedCount == rb3 + 1
                && scene.ActorList.Count == 1 && scene.ActorList[0] == pusher
                && !MShare.g_FreeActorList.Contains(pusher),
                $"threw={threw} skipped+={LoadMetrics.SceneListRaceSkippedCount - rb3} count={scene.ActorList.Count} still={scene.ActorList.Contains(pusher)} free={MShare.g_FreeActorList.Contains(pusher)}");

            // ⑤ 轮内索引读取（F1）：Count 检查通过后、下标读取前列表被并发缩短 ⇒ 必须跳过并计数，
            //    不能抛 AOORE。用 SceneIndexRaceHook 做确定性注入（生产为 null = 零行为变化）。
            scene.ActorList.Clear();
            MShare.MySelf = self;
            scene.ActorList.Add(new TestActor(robot) { CurrX = 100, CurrY = 100, m_nRx = 100, m_nRy = 100, RecogId = 987664 });
            long ib = LoadMetrics.SceneIndexRaceSkippedCount;
            PlayScene.SceneIndexRaceHook = sc =>
            {
                PlayScene.SceneIndexRaceHook = null;
                sc.ActorList.RemoveAt(sc.ActorList.Count - 1);
            };
            threw = false;
            try
            {
                SetMoveTimeNow(scene);
                scene.BeginScene();
            }
            catch (Exception e)
            {
                threw = true;
                Console.WriteLine("   index-race threw: " + e.GetType().Name + " @ " + e.StackTrace);
            }
            finally
            {
                PlayScene.SceneIndexRaceHook = null;
            }
            Check("index-read-race-skipped",
                !threw && LoadMetrics.SceneIndexRaceSkippedCount == ib + 1,
                $"threw={threw} skipped+={LoadMetrics.SceneIndexRaceSkippedCount - ib} want skip+1");

            // ⑥a 删除路径条件读取（F1c）：`i<Count` 通过后、`ActorList[i]` 之前被缩表 ⇒
            //    条件读取本身要 AOORE；必须计数跳过、不登记 Free。
            scene.ActorList.Clear();
            MShare.MySelf = self;
            var doomed = new TestActor(robot) { CurrX = 200, CurrY = 200, m_nRx = 200, m_nRy = 200, RecogId = 987665 };
            scene.ActorList.Add(doomed);
            long db = LoadMetrics.SceneListRaceSkippedCount;
            PlayScene.SceneRemoveRaceHook = sc =>
            {
                PlayScene.SceneRemoveRaceHook = null;
                sc.ActorList.RemoveAt(sc.ActorList.Count - 1);
            };
            threw = false;
            try
            {
                SetMoveTimeNow(scene);
                scene.BeginScene();
            }
            catch (Exception e)
            {
                threw = true;
                Console.WriteLine("   remove-condition threw: " + e.GetType().Name + " @ " + e.StackTrace);
            }
            finally
            {
                PlayScene.SceneRemoveRaceHook = null;
            }
            Check("remove-condition-read-race-skipped",
                !threw && LoadMetrics.SceneListRaceSkippedCount == db + 1
                && scene.ActorList.Count == 0 && !MShare.g_FreeActorList.Contains(doomed),
                $"threw={threw} skipped+={LoadMetrics.SceneListRaceSkippedCount - db} count={scene.ActorList.Count} free={MShare.g_FreeActorList.Contains(doomed)}");

            // ⑥b RemoveAt 窗口（F1b）：身份核对通过后、RemoveAt 之前被缩表 ⇒ AOORE 计数跳过，
            //    且 Free 登记必须在 RemoveAt 成功之后（不得双归属）。
            scene.ActorList.Clear();
            MShare.MySelf = self;
            var doomed2 = new TestActor(robot) { CurrX = 200, CurrY = 200, m_nRx = 200, m_nRy = 200, RecogId = 987667 };
            scene.ActorList.Add(doomed2);
            long db2 = LoadMetrics.SceneListRaceSkippedCount;
            PlayScene.SceneRemoveAtRaceHook = sc =>
            {
                PlayScene.SceneRemoveAtRaceHook = null;
                sc.ActorList.RemoveAt(sc.ActorList.Count - 1);
            };
            threw = false;
            try
            {
                SetMoveTimeNow(scene);
                scene.BeginScene();
            }
            catch (Exception e)
            {
                threw = true;
                Console.WriteLine("   remove-at threw: " + e.GetType().Name + " @ " + e.StackTrace);
            }
            finally
            {
                PlayScene.SceneRemoveAtRaceHook = null;
            }
            Check("remove-at-race-skipped",
                !threw && LoadMetrics.SceneListRaceSkippedCount == db2 + 1
                && scene.ActorList.Count == 0 && !MShare.g_FreeActorList.Contains(doomed2),
                $"threw={threw} skipped+={LoadMetrics.SceneListRaceSkippedCount - db2} count={scene.ActorList.Count} free={MShare.g_FreeActorList.Contains(doomed2)}");

            // ⑦ ActorDied「顺带核」：BeginScene 的 ProcMsg→ActorDied 在 run 线程执行，收包线程可能
            //    并发缩表；删除/插入失败必须计数跳过、不抛 AOORE。
            scene.ActorList.Clear();
            MShare.MySelf = self;
            var dying = new TestActor(robot) { CurrX = 100, CurrY = 100, m_nRx = 100, m_nRy = 100, RecogId = 987666 };
            scene.ActorList.Add(dying);
            long ab = LoadMetrics.SceneListRaceSkippedCount;
            PlayScene.SceneActorDiedRaceHook = sc =>
            {
                PlayScene.SceneActorDiedRaceHook = null;
                sc.ActorList.RemoveAt(sc.ActorList.Count - 1);
            };
            threw = false;
            try
            {
                scene.ActorDied(dying);
            }
            catch (Exception e)
            {
                threw = true;
                Console.WriteLine("   actordied-race threw: " + e.GetType().Name + " @ " + e.StackTrace);
            }
            finally
            {
                PlayScene.SceneActorDiedRaceHook = null;
            }
            Check("actordied-race-skipped",
                !threw && LoadMetrics.SceneListRaceSkippedCount == ab + 1
                && scene.ActorList.Count == 1 && scene.ActorList[0] == dying,
                $"threw={threw} skipped+={LoadMetrics.SceneListRaceSkippedCount - ab} count={scene.ActorList.Count}");

            // ⑧ RemoveAt 失败后 actor 仍在列表 ⇒ 不得再 Insert/Add 造成重复（P3-2）。
            scene.ActorList.Clear();
            var preActor = new TestActor(robot) { CurrX = 100, CurrY = 100, RecogId = 987668 };
            var dup = new TestActor(robot) { CurrX = 100, CurrY = 100, RecogId = 987669 };
            scene.ActorList.Add(preActor);
            scene.ActorList.Add(dup);
            long db3 = LoadMetrics.SceneListRaceSkippedCount;
            PlayScene.SceneActorDiedRaceHook = sc =>
            {
                PlayScene.SceneActorDiedRaceHook = null;
                sc.ActorList.RemoveAt(0);   // 前移：dup 移到 0，RemoveAt(1) 将 AOORE，dup 仍在列表
            };
            threw = false;
            try
            {
                scene.ActorDied(dup);
            }
            catch (Exception e)
            {
                threw = true;
                Console.WriteLine("   actordied-dup threw: " + e.GetType().Name + " @ " + e.StackTrace);
            }
            finally
            {
                PlayScene.SceneActorDiedRaceHook = null;
            }
            Check("actordied-no-duplicate-when-still-present",
                !threw && LoadMetrics.SceneListRaceSkippedCount == db3 + 1
                && scene.ActorList.Count == 1 && scene.ActorList[0] == dup,
                $"threw={threw} skipped+={LoadMetrics.SceneListRaceSkippedCount - db3} count={scene.ActorList.Count}");

            // ⑨ ActorDied 插入循环遇 null 槽位（P3-1）：不得解引用 null，跳过并计数。
            scene.ActorList.Clear();
            var other = new TestActor(robot) { CurrX = 100, CurrY = 100, RecogId = 987670 };
            var absent = new TestActor(robot) { CurrX = 100, CurrY = 100, RecogId = 987671 };
            scene.ActorList.Add(null);
            scene.ActorList.Add(other);
            long nb = LoadMetrics.SceneNullActorSkippedCount;
            threw = false;
            try
            {
                scene.ActorDied(absent);
            }
            catch (Exception e)
            {
                threw = true;
                Console.WriteLine("   actordied-null threw: " + e.GetType().Name + " @ " + e.StackTrace);
            }
            Check("actordied-null-slot-skipped",
                !threw && LoadMetrics.SceneNullActorSkippedCount == nb + 1
                && scene.ActorList.Count == 3 && scene.ActorList.Contains(absent),
                $"threw={threw} null+={LoadMetrics.SceneNullActorSkippedCount - nb} count={scene.ActorList.Count}");

            MShare.MySelf = self;
            scene.ActorList.Clear();
            MShare.MySelf = null;
            Console.WriteLine(failed == 0 ? "SCENEGUARD SELFTEST PASS" : $"SCENEGUARD SELFTEST FAIL ({failed})");
            return failed == 0 ? 0 : 1;
        }
    }
}
