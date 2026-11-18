using DBSrv.Services.Impl;
using DBSrv.Storage;

namespace DBSrv
{
    public class TimedService : BackgroundService
    {
        private readonly PeriodicTimer _timer;
        private readonly UserService _userService;
        private readonly ClientSession _sessionService;
        private readonly DataService _dataService;
        private readonly MarketService _marketService;
        private readonly ICacheStorage _cacheStorage;
        private readonly IPlayDataStorage _playDataStorage;

        public TimedService(ICacheStorage cacheStorage, UserService userService, ClientSession session, DataService dataService, IPlayDataStorage playDataStorage, MarketService marketService)
        {
            _userService = userService;
            _sessionService = session;
            _dataService = dataService;
            _cacheStorage = cacheStorage;
            _playDataStorage = playDataStorage;
            _marketService = marketService;
            _timer = new PeriodicTimer(TimeSpan.FromSeconds(1));
        }

        protected override async Task ExecuteAsync(CancellationToken stoppingToken)
        {
            int currentTick = HUtil32.GetTickCount();
            int lastSocketTick = currentTick;
            int lastKeepTick = currentTick;
            int lastClearTick = currentTick;
            int syncSaveTick = currentTick;
            int marketPushTick = currentTick;
            int lastWatchStatTick = currentTick;
            long lastWatchStatSlow = 0;
            try
            {
                while (await _timer.WaitForNextTickAsync(stoppingToken))
                {
                    currentTick = HUtil32.GetTickCount();
                    // **入口遥测心跳**（卡 `openmir2-entry-load-reply-buckets`）：每 10s 且值有变化才打一行。
                    // 被强杀（Stop-Process）时没有优雅关服 ⇒ 靠这行兜底，最后一行最多旧 10s。
                    string telemetryLine = EntryTelemetry.Heartbeat(currentTick);
                    if (telemetryLine != null)
                    {
                        LogService.Info(telemetryLine);
                    }

                    // **在飞请求看门狗**（卡 `openmir2-db-write-stall`）：请在飞 > 1 s 的请求，
                    // 并每 5 s 汇总一行存储段计时/连接计数。纯旁路，不改业务。
                    string slowLine = RequestWatchdog.Check(currentTick);
                    if (slowLine != null)
                    {
                        LogService.Warn(slowLine);
                    }
                    // 汇总行频率：出现慢请求时 5 s 一行（诊断需要），否则 60 s 一行（常态不刷屏）。
                    long slowNow = RequestWatchdog.SlowCount();
                    long statInterval = slowNow != lastWatchStatSlow ? 5000 : 60000;
                    if (currentTick - lastWatchStatTick > statInterval)
                    {
                        lastWatchStatTick = HUtil32.GetTickCount();
                        lastWatchStatSlow = slowNow;
                        LogService.Info(RequestWatchdog.SummaryLine() + " " + StorageTiming.Describe());
                    }
                    if (currentTick - lastSocketTick > 10000)
                    {
                        lastSocketTick = HUtil32.GetTickCount();
                        _sessionService.Start();
                        //_sessionService.CheckConnection();
                    }
                    if (currentTick - lastKeepTick > 7000)
                    {
                        lastKeepTick = HUtil32.GetTickCount();
                        _sessionService.SendKeepAlivePacket(_userService.GetUserCount());
                    }
                    if (currentTick - lastClearTick > 10000)
                    {
                        lastClearTick = HUtil32.GetTickCount();
                    }
                    if (currentTick - syncSaveTick > 300000) //5分钟刷新一次缓存数据到数据库
                    {
                        syncSaveTick = HUtil32.GetTickCount();
                        ProcessCacheStorage();
                    }
                    //if (currentTick - marketPushTick > _config.PushMarketInterval) //自定义时间推送一次拍卖行数据到各个GameSrv
                    //{
                    //    marketPushTick = HUtil32.GetTickCount();
                    //    _marketService.PushMarketData();
                    //}
                }
            }
            catch (OperationCanceledException)
            {

            }
        }

        private void ProcessCacheStorage()
        {
            //从内存获取保存数据，刷新到数据库，减少数据库压力，和防止大量数据保存超时
            LogService.Info("同步玩家缓存数据.");
            using IEnumerator<CharacterDataInfo> playList = _cacheStorage.QueryCacheData();
            while (playList.MoveNext())
            {
                CharacterDataInfo play = playList.Current;
                if (play == null)
                {
                    continue;
                }
                if (_playDataStorage.Update(play.Header.Name, play))
                {
                    LogService.Debug($"{play.Header.Name}同步成功.");
                }
                else
                {
                    LogService.Debug($"{play.Header.Name}同步失败.");
                }
                _cacheStorage.Delete(play.Header.Name);//处理完从缓存删除
            }
            LogService.Info("同步玩家缓存数据完成.");
        }
    }
}