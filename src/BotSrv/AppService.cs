using BotSrv.Player;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Options;
using OpenMir2;
using System;
using System.Threading;
using System.Threading.Tasks;

namespace BotSrv
{
    public class AppService : IHostedService
    {
        private readonly RobotOptions _options;
        /// <summary>
        /// 登录序号
        /// </summary>
        private static int g_nLoginIndex;
        /// <summary>
        /// 登录间隔
        /// </summary>
        private static long g_dwLogonTick;
        private readonly Thread runThread;

        public AppService(IOptions<RobotOptions> options, ClientManager clientManager)
        {
            BotShare.ClientMgr = clientManager;
            _options = options.Value;
            _options.ChrCount = HUtil32._MIN(_options.ChrCount, _options.TotalChrCount);
            g_dwLogonTick = HUtil32.GetTickCount() - 1000 * _options.ChrCount;
            MShare.g_sGameIPaddr = _options.Address;
            MShare.g_nGamePort = _options.Port;
            runThread = new Thread(Run) { IsBackground = true };
            LogService.Info("服务器名称:{0} 服务器IP:{1} 服务器端口:{2}", _options.ServerName, _options.Address, _options.Port);
            LogService.Info("初始化机器人数量:{0} 登录机器人总数量:{1}", _options.ChrCount, _options.TotalChrCount);
        }

        public async Task StartAsync(CancellationToken stoppingToken)
        {
            LogService.Info("机器人服务启动...");
            LoadMetrics.Init(Environment.GetEnvironmentVariable("MIR2_BOT_OUT"));
            LogService.Info("压测采集: 输出目录={0} 连接错峰={1}ms/个 统计={2}",
                Environment.GetEnvironmentVariable("MIR2_BOT_OUT") ?? "(exe 同级/load_out.conf)",
                _options.ConnectStaggerMs, LoadMetrics.StatsPath);
            runThread.Start();
            await BotShare.ClientMgr.Start(stoppingToken);
        }

        public Task StopAsync(CancellationToken cancellationToken)
        {
            LogService.Info("机器人服务停止...");
            LoadMetrics.Shutdown();     // 落最后一行统计（驱动 Stop-Process 时来不及，故这里是尽力而为）
            BotShare.ClientMgr.Stop(cancellationToken);
            return Task.CompletedTask;
        }

        private void Run()
        {
            LogService.Debug("工作线程开始执行...");
            while (true)
            {
                if (_options.TotalChrCount > 0)
                {
                    if (((HUtil32.GetTickCount() - g_dwLogonTick) > 1000 * _options.ChrCount))
                    {
                        g_dwLogonTick = HUtil32.GetTickCount();
                        if (_options.TotalChrCount >= _options.ChrCount)
                        {
                            _options.TotalChrCount -= _options.ChrCount;
                        }
                        else
                        {
                            _options.TotalChrCount = 0;
                        }
                        for (int i = 0; i < _options.ChrCount; i++)
                        {
                            RobotPlayer playClient = new RobotPlayer();
                            playClient.NewAccount = _options.NewAccount;
                            playClient.LoginId = string.Concat(_options.LoginAccount, g_nLoginIndex);
                            playClient.LoginPasswd = playClient.LoginId;
                            playClient.ChrName = playClient.LoginId;
                            playClient.ConnectTick = HUtil32.GetTickCount() + (i + 1) * _options.ConnectStaggerMs;
                            BotShare.ClientMgr.AddClient(playClient.SessionId, playClient);
                            LoadMetrics.Spawned();
                            g_nLoginIndex++;
                        }
                    }
                }
                try
                {
                    BotShare.ClientMgr.Run();
                }
                catch (Exception ex)
                {
                    // 压测进程不许被单个假人的异常打死（2026-10-10 实测：自动挂机里 MShare 共享态竞态
                    // 出 NullReference/IndexOutOfRange，整个 1000 假人进程直接退出 ⇒ 那一档数据全废）。
                    // 兜住并计数（进 load_stats.ndjson 的 internal_errors），前若干条打日志便于定位。
                    LoadMetrics.InternalError();
                    if (LoadMetrics.ShouldLogInternalError())
                    {
                        LogService.Warn("压测兜住的异常[" + LoadMetrics.InternalErrors + "]：" + ex.GetType().Name + ": " + ex.Message);
                    }
                }
                Thread.Sleep(TimeSpan.FromMilliseconds(50));
            }
        }
    }
}
