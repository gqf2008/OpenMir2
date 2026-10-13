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
            // M3 夹具确定性：固定建角参数（默认 false = 原随机行为，逐字节不变）
            MShare.g_boFixedNewChr = _options.FixedNewChr;
            MShare.g_nNewChrJob = _options.NewChrJob;
            MShare.g_nNewChrHair = _options.NewChrHair;
            MShare.g_nNewChrSex = _options.NewChrSex;
            // M3 T-14 夹具（白名单 T-22）：固定背包/装备序列；默认 false = 原行为
            MShare.g_boBagEquipSequence = _options.BagEquipSequence;
            MShare.g_nBagEquipStepMs = _options.BagEquipStepMs;
            MShare.g_nBagEquipStartDelayMs = _options.BagEquipStartDelayMs;
            // M3②③ 夹具（白名单 T-22 扩展）：固定攻击序列；默认 false = 原行为
            MShare.g_boAttackSequence = _options.AttackSequence;
            MShare.g_nAttackCount = _options.AttackCount;
            MShare.g_nAttackStepMs = _options.AttackStepMs;
            MShare.g_nAttackStartDelayMs = _options.AttackStartDelayMs;
            MShare.g_nAttackDir = _options.AttackDir;
            MShare.g_nAttackRole = _options.AttackRole;
            // M-2（reviewer-5 76e433f0）：attacker 判定改为与 LoginId 精确相等。
            // LoginId = LoginAccount + 序号（AppService.Run），故 attacker 的精确 LoginId = LoginAccount + AttackRole。
            MShare.g_sAttackRoleLoginId = string.Concat(_options.LoginAccount ?? string.Empty, _options.AttackRole);
            // M3⑤ 夹具（白名单 T-29 家族扩展）：固定 NPC/商店动作序列；默认 false = 原行为
            MShare.g_boShopSequence = _options.ShopSequence;
            MShare.g_nShopStepMs = _options.ShopStepMs;
            MShare.g_nShopStartDelayMs = _options.ShopStartDelayMs;
            MShare.g_sShopOps = _options.ShopOps ?? string.Empty;
            MShare.g_sShopNpcName = _options.ShopNpcName ?? string.Empty;
            MShare.g_sShopItemName = _options.ShopItemName ?? string.Empty;
            MShare.g_nShopItemMake = _options.ShopItemMake;
            MShare.g_nShopCount = _options.ShopCount;
            // M3⑧ 夹具（白名单 T-46 家族扩展）：固定行会/攻城动作序列；默认 false = 原行为
            MShare.g_boGuildCastleSequence = _options.GuildCastleSequence;
            MShare.g_nGuildCastleStepMs = _options.GuildCastleStepMs;
            MShare.g_nGuildCastleStartDelayMs = _options.GuildCastleStartDelayMs;
            MShare.g_sGuildCastleSteps = _options.GuildCastleSteps ?? string.Empty;
            MShare.g_nGuildCastleMasterRole = _options.GuildCastleMasterRole;
            MShare.g_nGuildCastlePeerRole = _options.GuildCastlePeerRole;
            MShare.g_sGuildCastleMasterLoginId = string.Concat(_options.LoginAccount ?? string.Empty, _options.GuildCastleMasterRole);
            MShare.g_sGuildCastlePeerLoginId = string.Concat(_options.LoginAccount ?? string.Empty, _options.GuildCastlePeerRole);
            // M1③ 退出路径夹具（白名单 M1-EXIT）：进世界后发 CM_SOFTCLOSE；默认 0 = 原行为
            MShare.g_nSoftCloseAfterWorldMs = _options.SoftCloseAfterWorldMs;
            MShare.g_boDisableAutoPlay = _options.DisableAutoPlay;
            // M3 预建角夹具（白名单 T-29 扩展）：预热相只建号不进世界；默认 false = 原行为
            MShare.g_boStopAfterCharCreated = _options.StopAfterCharCreated;
            runThread = new Thread(Run) { IsBackground = true };
            LogService.Info("服务器名称:{0} 服务器IP:{1} 服务器端口:{2}", _options.ServerName, _options.Address, _options.Port);
            LogService.Info("初始化机器人数量:{0} 登录机器人总数量:{1}", _options.ChrCount, _options.TotalChrCount);
            if (_options.FixedNewChr)
            {
                LogService.Info("建角参数固定: Job={0} Hair={1} Sex={2}", _options.NewChrJob, _options.NewChrHair, _options.NewChrSex);
            }
            if (_options.BagEquipSequence)
            {
                LogService.Info("固定背包/装备序列: 启用 起始等待={0}ms 步间隔={1}ms（拾取→装备→卸下→丢弃→用药；见 RobotPlayer.BagEquipSequenceTick）",
                    _options.BagEquipStartDelayMs, _options.BagEquipStepMs);
            }
            if (_options.AttackSequence)
            {
                LogService.Info("固定攻击序列: 启用 步数={0} 起始等待={1}ms 步间隔={2}ms dir={3} attacker序号={4}（见 RobotPlayer.AttackSequenceTick）",
                    _options.AttackCount, _options.AttackStartDelayMs, _options.AttackStepMs, _options.AttackDir, _options.AttackRole);
            }
            if (_options.SoftCloseAfterWorldMs > 0)
            {
                LogService.Info("退出路径夹具: 进世界后 {0}ms 发 CM_SOFTCLOSE(1009)（小退；见 RobotPlayer.SoftCloseTick）",
                    _options.SoftCloseAfterWorldMs);
            }
            if (_options.DisableAutoPlay)
            {
                LogService.Info("退出路径夹具: 已关闭自动挂机（DisableAutoPlay；见 ClientManager.RunAutoPlay）");
            }
            if (_options.StopAfterCharCreated)
            {
                LogService.Info("预建角夹具: 建角后停在选角界面、不进世界（StopAfterCharCreated；见 SelectChrScene.ClientGetReceiveChrs）");
            }
            if (_options.ShopSequence)
            {
                LogService.Info("固定 NPC/商店序列: 启用 ops={0} 起始等待={1}ms 步间隔={2}ms npc={3} item={4}（开面板/买/卖/修理/存取；见 RobotPlayer.ShopSequenceTick）",
                    _options.ShopOps, _options.ShopStartDelayMs, _options.ShopStepMs, _options.ShopNpcName, _options.ShopItemName);
            }
            if (_options.GuildCastleSequence)
            {
                LogService.Info("固定行会/攻城序列: 启用 steps={0} 起始等待={1}ms 步间隔={2}ms 会长序号={3} peer序号={4}（见 RobotPlayer.GuildCastleSequenceTick）",
                    _options.GuildCastleSteps, _options.GuildCastleStartDelayMs, _options.GuildCastleStepMs, _options.GuildCastleMasterRole, _options.GuildCastlePeerRole);
            }
        }

        public async Task StartAsync(CancellationToken stoppingToken)
        {
            LogService.Info("机器人服务启动...");
            LoadMetrics.Init(Environment.GetEnvironmentVariable("MIR2_BOT_OUT"));
            LogService.Info("压测采集: 输出目录={0} 连接错峰={1}ms/个 统计={2}",
                Environment.GetEnvironmentVariable("MIR2_BOT_OUT") ?? "(exe 同级/load_out.conf)",
                _options.ConnectStaggerMs, LoadMetrics.StatsPath);
            // 探针开关必须开机可见：否则"零样本"要先猜"是不是环境变量没传进去"（2026-10-11 踩过）
            LogService.Info("动作探针: 启用={0} 间隔={1}ms（武装条件 = 收到 SM_LOGON 才发 CM_TURN；见 RobotPlayer.ProbeActionTick）",
                RobotPlayer.ProbeEnabled, RobotPlayer.ProbeIntervalMs);
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
