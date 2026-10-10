using BotSrv.Player;
using OpenMir2;
using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Channels;
using System.Threading.Tasks;

namespace BotSrv
{
    public class ClientManager
    {

        private int g_dwProcessTimeMin = 0;
        private int g_dwProcessTimeMax = 0;
        private int g_nPosition = 0;
        private int dwRunTick = 0;
        private int AutoRunTick = 0;
        private readonly ConcurrentDictionary<string, RobotPlayer> _clients;
        private readonly IList<RobotPlayer> _clientList;
        private readonly IList<AutoPlayRunTime> _autoList;
        private readonly Channel<RecvicePacket> _reviceQueue;

        public ClientManager()
        {
            _clientList = new List<RobotPlayer>();
            _autoList = new List<AutoPlayRunTime>();
            _clients = new ConcurrentDictionary<string, RobotPlayer>();
            _reviceQueue = Channel.CreateUnbounded<RecvicePacket>();
        }

        public Task Start(CancellationToken stoppingToken)
        {
            LogService.Info("消息处理线程启动...");
            return ProcessReviceMessage(stoppingToken);
        }

        public void Stop(CancellationToken stoppingToken)
        {

        }

        private async Task ProcessReviceMessage(CancellationToken stoppingToken)
        {
            while (await _reviceQueue.Reader.WaitToReadAsync(stoppingToken))
            {
                if (_reviceQueue.Reader.TryRead(out RecvicePacket message))
                {
                    try
                    {
                        if (_clients.TryGetValue(message.SessionId, out RobotPlayer client))
                        {
                            client.ProcessPacket(message.ReviceData);
                        }
                    }
                    catch (Exception ex)
                    {
                        LogService.Error(ex);
                    }
                }
            }
        }

        public void AddPacket(string sessionId, string reviceData)
        {
            RecvicePacket clientPacket = new RecvicePacket();
            clientPacket.SessionId = sessionId;
            clientPacket.ReviceData = reviceData;
            _reviceQueue.Writer.TryWrite(clientPacket);
        }

        public void AddClient(string sessionId, RobotPlayer objClient)
        {
            _autoList.Add(new AutoPlayRunTime()
            {
                SessionId = sessionId,
                RunTick = HUtil32.GetTickCount()
            });
            _clients.TryAdd(sessionId, objClient);
            _clientList.Add(objClient);
        }

        public void DelClient(string sessionId)
        {
            AutoPlayRunTime findSession = null;
            foreach (AutoPlayRunTime item in _autoList)
            {
                if (item.SessionId == sessionId)
                {
                    findSession = item;
                    break;
                }
            }
            if (findSession != null)
            {
                _autoList.Remove(findSession);
            }
            _clients.TryRemove(sessionId, out RobotPlayer robotClient);
            _clientList.Remove(robotClient);
            if (robotClient != null)
            {
                LogService.Info("机器人[{0}] 会话ID:{1}]掉线或断开链接.", robotClient.ChrName, sessionId);
            }
        }

        public void Run()
        {
            dwRunTick = HUtil32.GetTickCount();
            bool boProcessLimit = false;
            for (int i = g_nPosition; i < _clientList.Count; i++)
            {
                // **逐个假人**兜异常（不要把 try 包在整段循环外）：假人的挂机/移动层建立在跨假人共享的
                // MShare.* 上，N 个假人会互相踩（实测 500 档 internal_errors 一路涨到 1248+）。
                // 包在循环外时，一个假人抛异常就**中断整轮**——后面的假人永远轮不到（实测：500 档爬到
                // 474/500 后再不动、tick 采样冻结、统计也不再落盘）。逐个兜住 ⇒ 一个坏假人不拖住其余。
                SafeRun(_clientList[i]);
                if (((HUtil32.GetTickCount() - dwRunTick) > 20))
                {
                    g_nPosition = i;
                    boProcessLimit = true;
                    break;
                }
            }
            if (!boProcessLimit)
            {
                g_nPosition = 0;
            }
            g_dwProcessTimeMin = HUtil32.GetTickCount() - dwRunTick;
            if (g_dwProcessTimeMin > g_dwProcessTimeMax)
            {
                g_dwProcessTimeMax = g_dwProcessTimeMin;
            }
            RunAutoPlay();
        }

        private void RunAutoPlay()
        {
            AutoRunTick = HUtil32.GetTickCount();
            if (_autoList.Count > 0)
            {
                for (int i = 0; i < _autoList.Count; i++)
                {
                    if ((AutoRunTick - _autoList[i].RunTick) > 800)
                    {
                        _autoList[i].RunTick = HUtil32.GetTickCount();
                        // 压测：进图后挂机定时器会被停掉，这里确保它一直开着（否则假人静止、收不到 +GD）
                        // 修（C6 退回补做，2026-10-11）：原来错用 `_clientList[i]`——循环变量属于 `_autoList`，
                        // 两个列表长度/顺序并不一致（增删各自独立），N 大时 `_clientList[i]` 越界 ⇒
                        // `IndexOutOfRangeException`（1000 档实测 internal_errors 十万级），而且即便不越界也**打错了假人**。
                        // 改为按本条目自己的 SessionId 取回对应假人。
                        if (_clients.TryGetValue(_autoList[i].SessionId, out RobotPlayer autoRobot))
                        {
                            SafeRun(autoRobot, ensureAutoPlay: true);
                        }
                    }
                }
            }
        }

        /// <summary>跑一个假人的一轮（可带挂机重臂），异常就地兜住 + 计数 + 前若干条打日志。</summary>
        private static void SafeRun(RobotPlayer robot, bool ensureAutoPlay = false)
        {
            try
            {
                if (ensureAutoPlay)
                {
                    robot.EnsureAutoPlay();
                    robot.RunAutoPlay();
                }
                else
                {
                    robot.Run();
                }
            }
            catch (Exception ex)
            {
                LoadMetrics.InternalError();
                if (LoadMetrics.ShouldLogInternalError())
                {
                    LogService.Warn("假人[" + robot.LoginId + "]异常被兜住[" + LoadMetrics.InternalErrors + "]："
                        + ex.GetType().Name + ": " + ex.Message);
                }
            }
        }
    }

    public struct RecvicePacket
    {
        public string SessionId;
        public string ReviceData;
    }

    public class AutoPlayRunTime
    {
        public string SessionId;
        public int RunTick;
    }
}
