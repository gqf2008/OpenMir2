using System.Diagnostics;
using System.Threading;

namespace DBSrv.Services.Impl
{
    /// <summary>
    /// **在飞请求看门狗**（卡 `openmir2-db-write-stall`，opc-07 / botfleet 规格）。
    ///
    /// 背景：tt2/tt3 两轮 5000 登录压测里，DBSrv 的 6000 面数据连接在
    /// `load_request_seen=2642 / 3383` 处**静默停读**——进程活着、CPU 0、525 线路照常。
    /// 现场 `dotnet-stack` / 全堆转储抓到收包线程停在
    /// `DataService.LoadHumanRcd` 的 `SendRequest(connectionId, queryId, responsePack, loadHumData)`
    /// （`DataService.cs:252`），栈顶是内联的 `Buffer._Memmove`，**既没有 MySqlConnector 帧
    /// 也没有 TouchSocket 发送帧**，一次采样分不清「应答序列化」与「socket 发送」。
    ///
    /// 本看门狗是**纯旁路**：请求进入时打时间戳、逐段更新阶段码，`TimedService` 每秒检查一次，
    /// 在飞超过 <see cref="SlowThresholdMs"/> 就打一行 `[EntryWatchdog] slow_request ...`，
    /// 直接给出「卡在哪个阶段 / 回包多大」。不改任何判定与回包。
    ///
    /// 线程模型：数据连接是**单线程串行处理**（TouchSocket 收包回调内同步执行），
    /// 因此只保留**一个**在飞槽位；若将来改成 worker 线程池，需要改成按线程/请求的槽位表。
    /// </summary>
    public static class RequestWatchdog
    {
        public const int StageIdle = 0;
        public const int StageDeserialize = 1;
        public const int StageSessionCheck = 2;
        public const int StageIndex = 3;
        public const int StageCacheGet = 4;
        public const int StageStorageGet = 5;
        public const int StageSerializeReply = 6;
        public const int StageSend = 7;

        /// <summary>超过多少毫秒算「慢请求」（正常单请求 &lt; 10 ms 量级）。</summary>
        public const long SlowThresholdMs = 1000;

        /// <summary>同一请求最多每 5 s 打一行，避免停写时每秒刷屏。</summary>
        private const long WarnIntervalTicks = 5000;

        private static long _startTs; // Stopwatch 时间戳；0 = 空闲
        private static int _stage = StageIdle;
        private static long _replyBytes;
        private static string _context = string.Empty;
        private static long _lastWarnTick;
        private static long _slowCount;
        private static long _maxElapsedMs;

        /// <summary>请求开始（阶段 1 = 反序列化请求体）。</summary>
        public static void Begin(int queryId, string ident)
        {
            Volatile.Write(ref _replyBytes, -1);
            _context = "queryId=" + queryId + " ident=" + ident + " chr=-";
            Volatile.Write(ref _stage, StageDeserialize);
            Volatile.Write(ref _startTs, Stopwatch.GetTimestamp());
        }

        /// <summary>补充上下文字段（取档请求解出 `ChrName` 后调用）。</summary>
        /// <remarks>仪表**不得**影响业务路径：任何异常形态都退化为「追加」，绝不抛。</remarks>
        public static void NoteChr(string chrName)
        {
            if (string.IsNullOrEmpty(chrName))
            {
                return;
            }
            string ctx = _context;
            int i = ctx.LastIndexOf(" chr=", System.StringComparison.Ordinal);
            _context = i < 0 ? ctx + " chr=" + chrName : ctx.Substring(0, i) + " chr=" + chrName;
        }

        /// <summary>回包体字节数（进入发送段前调用；-1 = 未走到该段）。</summary>
        public static void NoteReplyBytes(long bytes)
        {
            Volatile.Write(ref _replyBytes, bytes);
        }

        public static void Stage(int stage)
        {
            Volatile.Write(ref _stage, stage);
        }

        /// <summary>请求结束（成功/失败/异常都必须在 finally 里调用）。</summary>
        public static void End()
        {
            Volatile.Write(ref _startTs, 0L);
            Volatile.Write(ref _stage, StageIdle);
        }

        public static string IdentName(int ident)
        {
            if (ident == OpenMir2.Messages.DB_LOADHUMANRCD)
            {
                return "load";
            }
            if (ident == OpenMir2.Messages.DB_SAVEHUMANRCD ||
                ident == OpenMir2.Messages.DB_SAVEHUMANRCDEX)
            {
                return "save";
            }
            return "other:" + ident;
        }

        public static string StageName(int stage)
        {
            switch (stage)
            {
                case StageDeserialize: return "deserialize";
                case StageSessionCheck: return "session_check";
                case StageIndex: return "index";
                case StageCacheGet: return "cache_get";
                case StageStorageGet: return "storage_get";
                case StageSerializeReply: return "serialize_reply";
                case StageSend: return "send";
                default: return "idle";
            }
        }

        /// <summary>每秒由 `TimedService` 调用：在飞且超阈值就返回一行（否则 null）。</summary>
        public static string Check(long nowTick)
        {
            long start = Volatile.Read(ref _startTs);
            if (start == 0)
            {
                return null;
            }
            long elapsedMs = (Stopwatch.GetTimestamp() - start) * 1000L / Stopwatch.Frequency;
            if (elapsedMs < SlowThresholdMs)
            {
                return null;
            }
            long last = Volatile.Read(ref _lastWarnTick);
            if (last != 0 && nowTick - last < WarnIntervalTicks)
            {
                return null;
            }
            Volatile.Write(ref _lastWarnTick, nowTick);
            long n = Interlocked.Increment(ref _slowCount);
            long prevMax = Interlocked.Read(ref _maxElapsedMs);
            if (elapsedMs > prevMax)
            {
                Interlocked.Exchange(ref _maxElapsedMs, elapsedMs);
            }
            return "[EntryWatchdog] slow_request #" + n + " " + _context
                + " stage=" + StageName(Volatile.Read(ref _stage))
                + " elapsed_ms=" + elapsedMs
                + " reply_bytes=" + Volatile.Read(ref _replyBytes);
        }

        /// <summary>累计慢请求数（供定时器决定汇总行的打印频率）。</summary>
        public static long SlowCount()
        {
            return Interlocked.Read(ref _slowCount);
        }

        /// <summary>累计汇总（`TimedService` 每 5 s 打一行）。</summary>
        public static string SummaryLine()
        {
            return "[EntryWatchdog] watchdog_stats slow_requests=" + Interlocked.Read(ref _slowCount)
                + " slow_max_ms=" + Interlocked.Read(ref _maxElapsedMs)
                + " in_flight=" + (Volatile.Read(ref _startTs) == 0 ? 0 : 1)
                + " stage=" + StageName(Volatile.Read(ref _stage));
        }
    }
}
