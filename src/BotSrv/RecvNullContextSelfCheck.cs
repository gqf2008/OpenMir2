using System;
using BotSrv.Objects;
using BotSrv.Player;
using OpenMir2;
using OpenMir2.Packets.ClientPackets;
using Serilog;

namespace BotSrv
{
    /// <summary>
    /// 卡 `openmir2-botsrv-recv-null-residual`：软关后残留排队帧在 `MShare.MySelf == null`
    /// 上下文里的确定性离屏复现（**不启动服务/不连端口**）。
    ///
    /// 入口：`BotSrv.exe --selftest-recv-null-context`（PASS ⇒ exit 0，任一用例不符 ⇒ exit 1）。
    ///
    /// 生产触发链（真机 2026-11-16 10:42 窗口，归档 load-360-p1-verify-20261116）：
    ///   AppLogout/ClearActors 把本 bot 的 `MyActor` 置空；`ClientManager.ProcessOnePacket`
    ///   按包设置 `MShare.MySelf = client.MyActor`（= null）；而本 bot 的 `_inWorld` 仍为 true，
    ///   `DecodeMessagePacket` 照常进入世界分支 ⇒ 未加 null 门控的 `MShare.MySelf.RecogId`
    ///   解引用抛 NRE，被 RunPacketGuard 计成 `recv_packet_null`。
    ///
    /// 三组牙：
    ///   ① null 上下文：同一帧在 `MyActor == null` 时**不得**抛任何异常、不得计任何 recv 类别
    ///     （修复前 = NRE，自检红；修复后 = 跳过该分支的 SendMsg，自检绿）；
    ///   ② 非 null 路径等价：同一帧在 `MyActor != null` 时**必须**照旧产生该分支的 SendMsg
    ///     （Recog 不等 ⇒ 发；Recog 相等 ⇒ 不发）——证明门控没有把正常路径一起关掉；
    ///   ③ 夹具自证：`CharDesc` body 必须能被 `EDCode.DecodeBuffer<CharDesc>` 往返解回，
    ///     夹具坏 ⇒ 自检直接失败，不把「解码异常」当成「门控生效」。
    /// </summary>
    internal static class RecvNullContextSelfCheck
    {
        public static int Run()
        {
            // 生产解码/日志路径会走 LogService；自检不起 AppServer ⇒ 给一个无 sink 的最小 logger。
            LogService.Logger = new LoggerConfiguration().CreateLogger();
            int failed = 0;

            void Check(string name, bool ok, string detail)
            {
                if (!ok) { failed++; }
                Console.WriteLine($"RECVNULL {(ok ? "PASS" : "FAIL")} {name} {detail}");
            }

            // ---------- ③ 夹具自证：CharDesc body 往返 ----------
            string body;
            try
            {
                byte[] raw = SerializerUtil.Serialize(new CharDesc { Feature = 7, Status = 9 });
                body = EDCode.EncodeBuffer(raw, raw.Length);
                CharDesc rt = EDCode.DecodeBuffer<CharDesc>(body);
                bool ok = rt.Feature == 7 && rt.Status == 9;
                Check("fixture-chardesc-roundtrip", ok,
                    $"raw={raw.Length} enc_len={body.Length} rt=({rt.Feature},{rt.Status})");
                if (!ok)
                {
                    Console.WriteLine("RECVNULL SELFTEST FAIL (fixture 坏了，不能判定)");
                    return 1;
                }
            }
            catch (Exception ex)
            {
                Check("fixture-chardesc-roundtrip", false, ex.GetType().Name + ": " + ex.Message);
                Console.WriteLine("RECVNULL SELFTEST FAIL (fixture 抛异常)");
                return 1;
            }

            // 世界分支（真机 ident_counts 里这些 ident 都有流量；软关后都可能命中同一根因）。
            var cases = new (string name, ushort ident, int expectSendIdent)[]
            {
                ("walk", (ushort)Messages.SM_WALK, Messages.SM_WALK),
                ("run", (ushort)Messages.SM_RUN, Messages.SM_RUN),
                ("hit", (ushort)Messages.SM_HIT, Messages.SM_HIT),
                ("butch", (ushort)Messages.SM_BUTCH, Messages.SM_SITDOWN),
                ("sitdown", (ushort)Messages.SM_SITDOWN, Messages.SM_SITDOWN),
            };

            for (int ci = 0; ci < cases.Length; ci++)
            {
                (string name, ushort ident, int expectSendIdent) = cases[ci];
                int selfId = 4100 + ci * 10;
                int otherId = selfId + 1;

                // ---------- ① null 上下文：经 ClientManager 的 per-packet 上下文接线 ----------
                {
                    var bot = new RobotPlayer();
                    bot.MarkInWorldOnConnectionForTest(selfId, 10, 20, ci + 1);
                    // MyActor 保持 null（AppLogout/ClearActors 后的形态）。
                    var cm = new ClientManager();
                    string sid = "recvnull-" + name;
                    cm.AddClient(sid, bot);

                    long n0 = BotErrorCensus.Get(BotErrorCensus.RecvPacketNull);
                    long o0 = BotErrorCensus.Get(BotErrorCensus.RecvPacketOther);
                    long i0 = BotErrorCensus.Get(BotErrorCensus.RecvPacketIndex);
                    cm.ProcessOnePacket(new RecvicePacket
                    {
                        SessionId = sid,
                        ReviceData = Frame(ident, otherId, 0, 0, 0, body),
                    });
                    long dn = BotErrorCensus.Get(BotErrorCensus.RecvPacketNull) - n0;
                    long dother = BotErrorCensus.Get(BotErrorCensus.RecvPacketOther) - o0;
                    long dindex = BotErrorCensus.Get(BotErrorCensus.RecvPacketIndex) - i0;
                    Check("null-" + name + "-no-recv-error", dn == 0 && dother == 0 && dindex == 0,
                        $"delta null={dn} other={dother} index={dindex}");
                }

                // ---------- ② 直接调用：记录修复前异常类型与解引用行（红的证据） ----------
                {
                    var bot = new RobotPlayer();
                    bot.MarkInWorldOnConnectionForTest(selfId, 10, 20, ci + 1);
                    MShare.MySelf = null;
                    string outcome = "no-exception";
                    try
                    {
                        bot.ProcessPacket(Frame(ident, otherId, 0, 0, 0, body));
                    }
                    catch (Exception ex)
                    {
                        outcome = ex.GetType().Name + " @ " + OperateFrameLine(ex);
                    }
                    finally
                    {
                        MShare.MySelf = null;
                    }
                    Check("null-" + name + "-direct-no-throw", outcome == "no-exception", outcome);
                }

                // ---------- ③ 非 null 路径等价：Recog 不等 ⇒ 发；Recog 相等 ⇒ 不发 ----------
                {
                    var bot = new RobotPlayer();
                    bot.MyActor = new THumActor(bot) { RecogId = selfId };
                    bot.MarkInWorldOnConnectionForTest(selfId, 10, 20, ci + 1);
                    var cm = new ClientManager();
                    string sid = "recvnull-nonull-" + name;
                    cm.AddClient(sid, bot);

                    // 非自己帧需要场景里已有目标 actor（SM_HIT/SM_BUTCH/SM_SITDOWN 只对 FindActor 发）。
                    if (ident != Messages.SM_WALK && ident != Messages.SM_RUN)
                    {
                        bot.PlayScene.NewActor(otherId, 5, 6, 0, 0, 0);
                    }

                    // 不等 ⇒ 该分支的 SendMsg 必须发生。
                    cm.ProcessOnePacket(new RecvicePacket
                    {
                        SessionId = sid,
                        ReviceData = Frame(ident, otherId, 0, 0, 0, body),
                    });
                    Actor target = bot.PlayScene.FindActor(otherId);
                    bool sent = target != null && target.m_MsgList.Count == 1
                        && target.m_MsgList[0].Ident == expectSendIdent;
                    Check("nonull-" + name + "-other-sends", sent,
                        $"actor={(target != null)} msg={(target != null ? target.m_MsgList.Count : -1)} " +
                        $"ident={(target != null && target.m_MsgList.Count > 0 ? target.m_MsgList[0].Ident : -1)} " +
                        $"want={expectSendIdent}");

                    // 相等 ⇒ 该分支必须跳过（不建自己 actor、不发）。
                    int beforeCount = target != null ? target.m_MsgList.Count : 0;
                    cm.ProcessOnePacket(new RecvicePacket
                    {
                        SessionId = sid,
                        ReviceData = Frame(ident, selfId, 0, 0, 0, body),
                    });
                    Actor selfActor = bot.PlayScene.FindActor(selfId);
                    int afterCount = target != null ? target.m_MsgList.Count : 0;
                    Check("nonull-" + name + "-self-skips", selfActor == null && afterCount == beforeCount,
                        $"selfActor={(selfActor != null)} targetMsgs={beforeCount}->{afterCount}");
                }
            }

            Console.WriteLine(failed == 0 ? "RECVNULL SELFTEST PASS" : $"RECVNULL SELFTEST FAIL ({failed})");
            return failed == 0 ? 0 : 1;
        }

        /// <summary>组一条服务端→客户端的线上帧：`#` + 16 字符头 + 加密 CharDesc body + `!`。</summary>
        private static string Frame(ushort ident, int recog, int param, int tag, int series, string body)
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

        /// <summary>异常栈里第一条 `RobotPlayer.Operate.cs:line N`（修复前用来钉解引用行）。</summary>
        private static string OperateFrameLine(Exception ex)
        {
            string st = ex.StackTrace ?? string.Empty;
            foreach (string raw in st.Split('\n'))
            {
                string line = raw.Trim();
                if (line.Contains("RobotPlayer.Operate.cs:line"))
                {
                    int at = line.IndexOf(" in ", StringComparison.Ordinal);
                    return at >= 0 ? line[(at + 4)..] : line;
                }
            }
            return st.Trim();
        }
    }
}
