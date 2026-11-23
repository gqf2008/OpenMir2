using System;
using BotSrv.Data;
using BotSrv.Objects;
using BotSrv.Player;
using BotSrv.Scenes;
using OpenMir2;
using OpenMir2.Packets.ClientPackets;
using OpenMir2.Packets.ServerPackets;
using Serilog;

namespace BotSrv
{
    /// <summary>
    /// 卡 `openmir2-botsrv-recv-null-leftworld-gate`：类级门禁（per-bot `_leftWorld`）的离线牙。
    ///
    /// 入口：`BotSrv.exe --selftest-leftworld-gate`（PASS ⇒ exit 0；任一用例不符 ⇒ exit 1）。
    /// **不启动服务/不连端口。**
    ///
    /// 用例（Leader 2026-11-23 定稿）：
    ///   L1 置位 + `MySelf == null` ⇒ 世界帧被丢、census 不涨、无副作用
    ///   L2 置位 + `MySelf` 非 null ⇒ 同 L1（证明门禁键在标记、不在 null）
    ///   L3 对照（不置位）⇒ null 仍由最小修的分支门控挡住；非 null 走原路径（防掩蔽）
    ///   L4 复位点 1：`ChangeScene(Login)` 后同帧重新被处理
    ///   L5 复位点 2：`MarkInWorld` 的 `_inWorldCounted` 早退**之前**复位（重登场景）
    ///   L6 `relog_on_struck` 接线牙：`SM_STRUCK` 自己受击 → 计数 +1、`ClearActors` 生效
    ///   L7a `'+'` 动作包在门禁之前（`#+GD/12345!` ⇒ `g_rtime` 落盘）
    ///   L7b `g_boMapMoving` 换图帧在门禁之前（`SM_CHANGEMAP` ⇒ `MapMovingWait`）
    ///   L7c 为注释级位置约束（见 `RobotPlayer.Operate.cs` 门禁处），无运行用例。
    /// </summary>
    internal static class LeftWorldGateSelfCheck
    {
        public static int Run()
        {
            LogService.Logger = new LoggerConfiguration().CreateLogger();
            int failed = 0;

            void Check(string name, bool ok, string detail)
            {
                if (!ok) { failed++; }
                Console.WriteLine($"LEFTWORLD {(ok ? "PASS" : "FAIL")} {name} {detail}");
            }

            long Nre() => BotErrorCensus.Get(BotErrorCensus.RecvPacketNull);
            long Oth() => BotErrorCensus.Get(BotErrorCensus.RecvPacketOther);
            long Idx() => BotErrorCensus.Get(BotErrorCensus.RecvPacketIndex);

            string descBody;
            try
            {
                byte[] raw = SerializerUtil.Serialize(new CharDesc { Feature = 7, Status = 9 });
                descBody = EDCode.EncodeBuffer(raw, raw.Length);
                CharDesc rt = EDCode.DecodeBuffer<CharDesc>(descBody);
                bool ok = rt.Feature == 7 && rt.Status == 9;
                Check("fixture-chardesc-roundtrip", ok, $"rt=({rt.Feature},{rt.Status})");
                if (!ok) { Console.WriteLine("LEFTWORLD SELFTEST FAIL (fixture 坏了，不能判定)"); return 1; }
            }
            catch (Exception ex)
            {
                Check("fixture-chardesc-roundtrip", false, ex.GetType().Name + ": " + ex.Message);
                Console.WriteLine("LEFTWORLD SELFTEST FAIL (fixture 抛异常)");
                return 1;
            }

            RobotPlayer NewInWorldBot(int selfId)
            {
                var bot = new RobotPlayer();
                bot.PlayScene.NewActor(selfId, 10, 20, 0, 0, 0);
                // 与生产 `PlayScene` 的 SM_LOGON 分支同形：MyActor = 场景里的那个实例（引用相等）。
                bot.MyActor = (THumActor)bot.PlayScene.FindActor(selfId);
                bot.MarkInWorldOnConnectionForTest(selfId, 10, 20, 1);
                return bot;
            }

            string Frame(ushort ident, int recog, int param, int tag, int series, string body)
            {
                CommandMessage msg = new CommandMessage
                {
                    Recog = recog,
                    Ident = ident,
                    Param = (ushort)param,
                    Tag = (ushort)tag,
                    Series = (ushort)series,
                };
                return "#" + EDCode.EncodeMessage(msg) + body + "!";
            }

            // ---------- L1 置位 + MySelf=null：世界帧被整段丢弃 ----------
            {
                int selfId = 6100, otherId = 6101;
                var bot = new RobotPlayer();
                bot.MarkInWorldOnConnectionForTest(selfId, 10, 20, 1); // MyActor 保持 null（软关后形态）
                bot.PlayScene.NewActor(otherId, 5, 6, 0, 0, 0);
                var cm = new ClientManager();
                string sid = "lw1";
                cm.AddClient(sid, bot);
                bot.SetLeftWorldForTest(true);
                MShare.TargetX = 7;
                MShare.TargetY = 9;
                long n0 = Nre(), o0 = Oth(), i0 = Idx();
                cm.ProcessOnePacket(new RecvicePacket
                {
                    SessionId = sid,
                    ReviceData = Frame((ushort)Messages.SM_MOVEFAIL, otherId, 0, 0, 0, descBody),
                });
                Actor other = bot.PlayScene.FindActor(otherId);
                int msgs = other == null ? -1 : other.m_MsgList.Count;
                bool ok = Nre() - n0 == 0 && Oth() - o0 == 0 && Idx() - i0 == 0
                    && MShare.TargetX == 7 && MShare.TargetY == 9 && msgs == 0;
                Check("L1-set-drops-frame-null", ok,
                    $"census=({Nre() - n0},{Oth() - o0},{Idx() - i0}) tx={MShare.TargetX} ty={MShare.TargetY} msgs={msgs}");
                MShare.TargetX = -1;
                MShare.TargetY = -1;
            }

            // ---------- L2 置位 + MySelf 非 null：同 L1（门禁键在标记） ----------
            {
                int selfId = 6200, otherId = 6201;
                var bot = new RobotPlayer();
                bot.MyActor = new THumActor(bot) { RecogId = selfId };
                bot.MarkInWorldOnConnectionForTest(selfId, 10, 20, 1);
                bot.PlayScene.NewActor(otherId, 5, 6, 0, 0, 0);
                var cm = new ClientManager();
                string sid = "lw2";
                cm.AddClient(sid, bot);
                bot.SetLeftWorldForTest(true);
                MShare.TargetX = 7;
                MShare.TargetY = 9;
                long n0 = Nre(), o0 = Oth(), i0 = Idx();
                cm.ProcessOnePacket(new RecvicePacket
                {
                    SessionId = sid,
                    ReviceData = Frame((ushort)Messages.SM_MOVEFAIL, otherId, 0, 0, 0, descBody),
                });
                Actor other = bot.PlayScene.FindActor(otherId);
                int msgs = other == null ? -1 : other.m_MsgList.Count;
                bool ok = Nre() - n0 == 0 && Oth() - o0 == 0 && Idx() - i0 == 0
                    && MShare.TargetX == 7 && MShare.TargetY == 9 && msgs == 0;
                Check("L2-set-drops-frame-nonnull", ok,
                    $"census=({Nre() - n0},{Oth() - o0},{Idx() - i0}) tx={MShare.TargetX} ty={MShare.TargetY} msgs={msgs}");
                MShare.TargetX = -1;
                MShare.TargetY = -1;
            }

            // ---------- L3 对照：不置位时逐行等价（并证明分支牙未被类级门禁掩蔽） ----------
            {
                int selfId = 6300, otherId = 6301;
                // a) 不置位 + MySelf=null ⇒ 仍由 SM_MOVEFAIL 分支门控挡住
                var bot = new RobotPlayer();
                bot.MarkInWorldOnConnectionForTest(selfId, 10, 20, 1);
                bot.PlayScene.NewActor(otherId, 5, 6, 0, 0, 0);
                var cm = new ClientManager();
                string sid = "lw3a";
                cm.AddClient(sid, bot);
                MShare.TargetX = 7;
                MShare.TargetY = 9;
                long n0 = Nre(), o0 = Oth(), i0 = Idx();
                cm.ProcessOnePacket(new RecvicePacket
                {
                    SessionId = sid,
                    ReviceData = Frame((ushort)Messages.SM_MOVEFAIL, otherId, 0, 0, 0, descBody),
                });
                Actor other = bot.PlayScene.FindActor(otherId);
                int msgs = other == null ? -1 : other.m_MsgList.Count;
                Check("L3a-unset-null-branch-gate-still-works",
                    Nre() - n0 == 0 && Oth() - o0 == 0 && Idx() - i0 == 0 && MShare.TargetX == 7 && msgs == 0,
                    $"census=({Nre() - n0},{Oth() - o0},{Idx() - i0}) tx={MShare.TargetX} msgs={msgs}");

                // b) 不置位 + MySelf 非 null ⇒ 原路径（副作用 + SM_TURN）
                var bot2 = NewInWorldBot(selfId);
                bot2.PlayScene.NewActor(otherId, 5, 6, 0, 0, 0);
                var cm2 = new ClientManager();
                string sid2 = "lw3b";
                cm2.AddClient(sid2, bot2);
                MShare.TargetX = 7;
                MShare.TargetY = 9;
                bot2.MyActor.m_boUseCboLib = true;
                n0 = Nre();
                cm2.ProcessOnePacket(new RecvicePacket
                {
                    SessionId = sid2,
                    ReviceData = Frame((ushort)Messages.SM_MOVEFAIL, otherId, 0, 0, 0, descBody),
                });
                Actor other2 = bot2.PlayScene.FindActor(otherId);
                bool sent = other2 != null && other2.m_MsgList.Count >= 1
                    && other2.m_MsgList[0].Ident == Messages.SM_TURN;
                Check("L3b-unset-preserves-branch",
                    MShare.TargetX == -1 && MShare.TargetY == -1 && !bot2.MyActor.m_boUseCboLib && sent && Nre() - n0 == 0,
                    $"tx={MShare.TargetX} ty={MShare.TargetY} cbolib={bot2.MyActor.m_boUseCboLib} sent={sent} dnre={Nre() - n0}");
                MShare.TargetX = -1;
                MShare.TargetY = -1;
            }

            // ---------- L4 复位点 1：ChangeScene(Login) ----------
            {
                int selfId = 6400, otherId = 6401;
                var bot = NewInWorldBot(selfId);
                bot.PlayScene.NewActor(otherId, 5, 6, 0, 0, 0);
                var cm = new ClientManager();
                string sid = "lw4";
                cm.AddClient(sid, bot);
                bot.SetLeftWorldForTest(true);
                MShare.TargetX = 7;
                MShare.TargetY = 9;
                cm.ProcessOnePacket(new RecvicePacket
                {
                    SessionId = sid,
                    ReviceData = Frame((ushort)Messages.SM_MOVEFAIL, otherId, 0, 0, 0, descBody),
                });
                bool blocked = MShare.TargetX == 7 && MShare.TargetY == 9;
                // `LoginScene.OpenScene` 会 `IPAddress.Parse(MShare.g_sGameIPaddr)`——自检不起栈，
                // 这里临时给一个回环地址再还原（`SetNotifyEvent` 只是登记，没有 Run 循环不会触发）。
                string savedIp = MShare.g_sGameIPaddr;
                int savedPort = MShare.g_nGamePort;
                MShare.g_sGameIPaddr = "127.0.0.1";
                MShare.g_nGamePort = 1;
                bot.DScreen.ChangeScene(SceneType.Login);
                MShare.g_sGameIPaddr = savedIp;
                MShare.g_nGamePort = savedPort;
                MShare.TargetX = 7;
                MShare.TargetY = 9;
                cm.ProcessOnePacket(new RecvicePacket
                {
                    SessionId = sid,
                    ReviceData = Frame((ushort)Messages.SM_MOVEFAIL, otherId, 0, 0, 0, descBody),
                });
                bool resumed = MShare.TargetX == -1;
                Check("L4-reset-on-scene-change", blocked && resumed, $"blocked={blocked} resumed={resumed}");
                MShare.TargetX = -1;
                MShare.TargetY = -1;
            }

            // ---------- L5 复位点 2：MarkInWorld 的 _inWorldCounted 早退之前 ----------
            {
                int selfId = 6500, otherId = 6501;
                var bot = NewInWorldBot(selfId); // 第一次 SM_LOGON ⇒ _inWorldCounted=true
                bot.PlayScene.NewActor(otherId, 5, 6, 0, 0, 0);
                var cm = new ClientManager();
                string sid = "lw5";
                cm.AddClient(sid, bot);
                bot.SetLeftWorldForTest(true);
                // 同 actor / 同 conn 的第二次 SM_LOGON：必然走 `if (_inWorldCounted) return true;` 早退。
                bool second = bot.MarkInWorldOnConnectionForTest(selfId, 10, 20, 1);
                MShare.TargetX = 7;
                MShare.TargetY = 9;
                cm.ProcessOnePacket(new RecvicePacket
                {
                    SessionId = sid,
                    ReviceData = Frame((ushort)Messages.SM_MOVEFAIL, otherId, 0, 0, 0, descBody),
                });
                Check("L5-reset-before-inworldcounted-earlyreturn", second && MShare.TargetX == -1,
                    $"secondLogon={second} tx={MShare.TargetX}");
                MShare.TargetX = -1;
                MShare.TargetY = -1;
            }

            // ---------- L6 relog_on_struck 接线牙 ----------
            {
                int selfId = 6600, otherId = 6601;
                var bot = NewInWorldBot(selfId);
                bot.PlayScene.NewActor(otherId, 5, 6, 0, 0, 0); // 攻击者 race=0 ⇒ 玩家
                bot.ClientSocket = new ScoketClient();          // 未连接：SendSocket 只走日志分支
                bot.TimerAutoPlay = new TimerAutoPlay { Enabled = true };
                MShare.OpenAutoPlay = true;
                MShare.g_boQueryExit = false;
                MShare.g_nAPReLogon = 0;
                var cm = new ClientManager();
                string sid = "lw6";
                cm.AddClient(sid, bot);
                byte[] rawWl = SerializerUtil.Serialize(new MessageBodyWL { Tag1 = otherId });
                string wlBody = EDCode.EncodeBuffer(rawWl, rawWl.Length);
                long r0 = LoadMetrics.GetRelogOnStruck();
                long n0 = Nre(), o0 = Oth(), i0 = Idx();
                cm.ProcessOnePacket(new RecvicePacket
                {
                    SessionId = sid,
                    ReviceData = Frame((ushort)Messages.SM_STRUCK, selfId, 3, 9, 1, wlBody),
                });
                long dr = LoadMetrics.GetRelogOnStruck() - r0;
                bool ok = dr == 1 && bot.MyActor == null && MShare.g_nAPReLogon == 1
                    && Nre() - n0 == 0 && Oth() - o0 == 0 && Idx() - i0 == 0;
                Check("L6-relog-on-struck-counts-and-clears-actor", ok,
                    $"drelog={dr} myActorNull={(bot.MyActor == null)} aplogon={MShare.g_nAPReLogon} census=({Nre() - n0},{Oth() - o0},{Idx() - i0})");

                // L6b：`AppLogout` 必须同时**置位门禁**——软关后的下一条世界帧要被整段丢弃。
                // 探针用 `SM_LEVELUP`（`MShare.MySelf.Abil.Level` 解引用，**当前无** null 门控）：
                // 若 AppLogout 忘了置位，这一帧会 NRE 并计入 census。
                // （`SM_TURN`/`SM_WINEXP`/`SM_ABILITY` 不能用——它们已有 null 门控，探针会假绿。）
                long n1 = Nre(), o1 = Oth(), i1 = Idx();
                cm.ProcessOnePacket(new RecvicePacket
                {
                    SessionId = sid,
                    ReviceData = Frame((ushort)Messages.SM_LEVELUP, otherId, 0, 0, 0, descBody),
                });
                Check("L6b-applogout-sets-leftworld",
                    Nre() - n1 == 0 && Oth() - o1 == 0 && Idx() - i1 == 0,
                    $"census=({Nre() - n1},{Oth() - o1},{Idx() - i1})");
                MShare.g_nAPReLogon = 0;
                MShare.g_boQueryExit = false;
            }

            // ---------- L7a 动作包（'+' 前缀）在门禁之前 ----------
            {
                var bot = new RobotPlayer();
                bot.TimerAutoMove = new TimerAutoPlay(); // ProcessActMsg 的 `#+GD/` 分支会读 TimerAutoMove.Enabled
                bot.SetLeftWorldForTest(true);
                MShare.g_rtime = 0;
                bot.ProcessPacket("#+GD/12345!");
                Check("L7a-act-packet-not-swallowed", MShare.g_rtime == 12345, $"g_rtime={MShare.g_rtime}");
                MShare.g_rtime = 0;
            }

            // ---------- L7b 换图帧（g_boMapMoving 块）在门禁之前 ----------
            {
                var bot = new RobotPlayer();
                bot.SetLeftWorldForTest(true);
                MShare.g_boMapMoving = true;
                MShare.MapMovingWait = false;
                bot.ProcessPacket(Frame((ushort)Messages.SM_CHANGEMAP, 0, 0, 0, 0, EDCode.EncodeString("1")));
                Check("L7b-changemap-not-swallowed", MShare.MapMovingWait, $"mapMovingWait={MShare.MapMovingWait}");
                MShare.g_boMapMoving = false;
                MShare.MapMovingWait = false;
            }

            Console.WriteLine(failed == 0 ? "LEFTWORLD SELFTEST PASS" : $"LEFTWORLD SELFTEST FAIL ({failed})");
            return failed == 0 ? 0 : 1;
        }
    }
}
