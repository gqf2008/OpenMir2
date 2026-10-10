using System;
using System.Globalization;
using System.IO;
using System.Text;
using System.Threading;

namespace BotSrv
{
    /// <summary>
    /// 压测指标采集（C6 载荷门禁的"测量仪"）—— 假人进程自己量，不靠改 oracle、不靠抓包代理。
    ///
    /// **为什么由假人来量**：tick 服务时延的口径是"线上可观测"的 ——
    /// 服务端在每个被接受的客户端动作上回一帧明文 `#+GD/&lt;rtime&gt;!`，其中 `rtime` =
    /// `M2Share.GetGoodTick` = `HUtil32.GetTickCount()` = `Environment.TickCount`（**开机毫秒，全机同域**）。
    /// 假人收到该帧时也在同一时钟域取一次 ⇒
    ///     tick 服务时延 = 收到时刻 − 帧里带的 rtime
    /// 即"服务端取时间戳 → 客户端收完这一帧"的耗时（本机环回，无网络时延），它直接反映
    /// 世界线程/发送队列被拖住的程度。**同一个假人（同一份 BotSrv）也能跑 Rust 侧** ⇒ 两侧口径天然一致
    /// （M2 的 p99-baseline 就用它对齐）。
    ///
    /// 采集方式：内存直方图（1ms 桶，0..200ms + 溢出桶）累计写 `load_stats.ndjson`（每 5s 一行，累计值，
    /// 下游按相邻两行做差分就能得到"每个时间窗"的 P50/P90/P99）；`MIR2_BOT_TICK_SAMPLES=1` 时另落
    /// 逐样本文件（小档位精确核对用）。**退出不依赖优雅关闭**（驱动会 Stop-Process），所以是持续落盘。
    ///
    /// 时钟粒度提醒：`Environment.TickCount` 在 Windows 上粒度约 15.6ms ⇒ 单样本有 ±16ms 量化，
    /// 报告里要把它当"测量下限"写出来（Rust 侧若用更细的时钟，该字段语义仍按同口径声明）。
    /// </summary>
    public static class LoadMetrics
    {
        private const int MaxBucketMs = 200;                 // 直方图 0..200ms，1ms 桶
        private const int BucketCount = MaxBucketMs + 1;     // 索引 0..200
        private const int OverflowIndex = BucketCount;       // >200ms 的溢出桶

        private static readonly object Sync = new object();
        private static readonly long[] Hist = new long[BucketCount + 1];

        private static long _spawned;
        private static long _loginOk;
        private static long _connRefused;
        private static long _connLost;
        private static long _tickSamples;
        private static long _tickSumMs;
        private static long _tickMaxMs;

        private static string _outDir;
        private static string _statsPath;
        private static StreamWriter _stats;
        private static StreamWriter _samples;
        private static Thread _flusher;
        private static volatile bool _running;
        private static long _startTickMs;
        private static int _flushIntervalMs = 5000;

        public static string StatsPath => _statsPath;

        /// <summary>由 AppService 启动时调用。输出目录优先级：参数 &gt; 环境变量 MIR2_BOT_OUT &gt; exe 同级。</summary>
        public static void Init(string outDir)
        {
            lock (Sync)
            {
                if (_running)
                {
                    return;
                }
                _outDir = ResolveOutDir(outDir);
                Directory.CreateDirectory(_outDir);
                _statsPath = Path.Combine(_outDir, "load_stats.ndjson");
                _stats = NewWriter(_statsPath);
                if (Environment.GetEnvironmentVariable("MIR2_BOT_TICK_SAMPLES") == "1")
                {
                    _samples = NewWriter(Path.Combine(_outDir, "tick_samples.ndjson"));
                }
                _startTickMs = Environment.TickCount;
                _running = true;
                WriteLineLocked(_stats, StatsLine());
                _stats.Flush();
            }
            _flusher = new Thread(FlushLoop) { IsBackground = true, Name = "LoadMetricsFlush" };
            _flusher.Start();
        }

        private static string ResolveOutDir(string explicitDir)
        {
            if (!string.IsNullOrEmpty(explicitDir))
            {
                return explicitDir;
            }
            string env = Environment.GetEnvironmentVariable("MIR2_BOT_OUT");
            if (!string.IsNullOrEmpty(env))
            {
                return env;
            }
            // 兜底：exe 同级放 load_out.conf（一行目录）——环境变量在注入/托管启动下不一定继承
            string conf = Path.Combine(AppContext.BaseDirectory, "load_out.conf");
            if (File.Exists(conf))
            {
                string line = File.ReadAllText(conf, Encoding.UTF8).Trim();
                if (line.Length > 0)
                {
                    return line;
                }
            }
            return Path.Combine(AppContext.BaseDirectory, "load-metrics");
        }

        private static StreamWriter NewWriter(string path)
        {
            var fs = new FileStream(path, FileMode.Create, FileAccess.Write, FileShare.ReadWrite);
            return new StreamWriter(fs, new UTF8Encoding(false)) { AutoFlush = false };
        }

        public static void Spawned()
        {
            lock (Sync)
            {
                _spawned++;
            }
        }

        public static void LoginOk()
        {
            lock (Sync)
            {
                _loginOk++;
            }
        }

        public static void ConnRefused()
        {
            lock (Sync)
            {
                _connRefused++;
            }
        }

        /// <summary>登录之后连接被断开/超时 = 掉线（与"连不上"分开计）。</summary>
        public static void ConnLost()
        {
            lock (Sync)
            {
                _connLost++;
            }
        }

        /// <summary>一条 tick 服务时延样本（ms）。rtime 为空/非正时调用方不该调。</summary>
        public static void Tick(int latencyMs)
        {
            if (latencyMs < 0)
            {
                latencyMs = 0;      // 负值只会来自时钟粒度/回绕，钳到 0 而不是丢样本
            }
            lock (Sync)
            {
                _tickSamples++;
                _tickSumMs += latencyMs;
                if (latencyMs > _tickMaxMs)
                {
                    _tickMaxMs = latencyMs;
                }
                if (latencyMs <= MaxBucketMs)
                {
                    Hist[latencyMs]++;
                }
                else
                {
                    Hist[OverflowIndex]++;
                }
                if (_samples != null)
                {
                    WriteLineLocked(_samples, latencyMs.ToString(CultureInfo.InvariantCulture));
                }
            }
        }

        private static void FlushLoop()
        {
            while (_running)
            {
                Thread.Sleep(_flushIntervalMs);
                Flush();
            }
        }

        public static void Flush()
        {
            try
            {
                lock (Sync)
                {
                    if (_stats == null)
                    {
                        return;
                    }
                    WriteLineLocked(_stats, StatsLine());
                    _stats.Flush();
                    _samples?.Flush();
                }
            }
            catch
            {
                // 采集失败不能让被测进程崩（压测期间磁盘/句柄异常都算环境问题，由驱动侧可见）
            }
        }

        public static void Shutdown()
        {
            Flush();
            lock (Sync)
            {
                _running = false;
                _stats?.Flush();
                _stats?.Dispose();
                _samples?.Flush();
                _samples?.Dispose();
                _stats = null;
                _samples = null;
            }
        }

        private static void WriteLineLocked(StreamWriter w, string line)
        {
            w.Write(line);
            w.Write('\n');
        }

        /// <summary>累计快照（下游按相邻两行差分得到时间窗内的分布）。</summary>
        private static string StatsLine()
        {
            var sb = new StringBuilder(1024);
            sb.Append("{\"t_ms\":").Append(Environment.TickCount)
              .Append(",\"uptime_ms\":").Append(Environment.TickCount - _startTickMs)
              .Append(",\"spawned\":").Append(_spawned)
              .Append(",\"login_ok\":").Append(_loginOk)
              .Append(",\"conn_refused\":").Append(_connRefused)
              .Append(",\"conn_lost\":").Append(_connLost)
              .Append(",\"tick_samples\":").Append(_tickSamples)
              .Append(",\"tick_sum_ms\":").Append(_tickSumMs)
              .Append(",\"tick_max_ms\":").Append(_tickMaxMs)
              .Append(",\"hist_bucket_ms\":1,\"hist\":[");
            for (int i = 0; i < Hist.Length; i++)
            {
                if (i > 0)
                {
                    sb.Append(',');
                }
                sb.Append(Hist[i]);
            }
            sb.Append("]}");
            return sb.ToString();
        }
    }
}
