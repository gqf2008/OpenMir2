using System.Threading;

namespace DBSrv.Storage
{
    /// <summary>
    /// **存储段计时**（卡 `openmir2-db-write-stall`，opc-07 四段口径里的两段）。
    ///
    /// - `conn_acquire_ms`：`StorageContext.Open`（MySqlConnector 连接池借出/建连）耗时；
    /// - `query_exec_ms`：一次 `GetRecord` 内的全部 SQL 耗时；
    /// - `conn_opened/closed/inflight`：自维护连接计数，用于区分**连接泄漏**（inflight 只涨不落）
    ///   与**池耗尽**（两者都会让 Open 变慢/抛错）。
    ///
    /// 纯旁路静态计数，不参与任何判定。由 DBSrv 侧 `RequestWatchdog.SummaryLine()` 每 5 s 汇总。
    /// 注意：MySqlConnector 不暴露池内 idle/used 计数，这里的 inflight 是**本进程自己打开且未关闭**
    /// 的连接数（= 池里 used 的下界），不是池容量。
    /// </summary>
    public static class StorageTiming
    {
        private static long _openMsMax;
        private static long _queryMsMax;
        private static long _openCount;
        private static long _openFail;
        private static long _queryCount;
        private static long _connOpened;
        private static long _connClosed;
        private static long _connInflight;

        public static void RecordOpen(long ms, bool success)
        {
            Interlocked.Increment(ref _openCount);
            if (!success)
            {
                Interlocked.Increment(ref _openFail);
            }
            long cur = Interlocked.Read(ref _openMsMax);
            if (ms > cur)
            {
                Interlocked.Exchange(ref _openMsMax, ms);
            }
        }

        public static void RecordQuery(long ms)
        {
            Interlocked.Increment(ref _queryCount);
            long cur = Interlocked.Read(ref _queryMsMax);
            if (ms > cur)
            {
                Interlocked.Exchange(ref _queryMsMax, ms);
            }
        }

        public static void ConnOpened()
        {
            Interlocked.Increment(ref _connOpened);
            Interlocked.Increment(ref _connInflight);
        }

        public static void ConnClosed()
        {
            Interlocked.Increment(ref _connClosed);
            Interlocked.Decrement(ref _connInflight);
        }

        public static string Describe()
        {
            return "conn_acquire_ms_max=" + Interlocked.Read(ref _openMsMax)
                + " query_exec_ms_max=" + Interlocked.Read(ref _queryMsMax)
                + " opens=" + Interlocked.Read(ref _openCount)
                + " open_fail=" + Interlocked.Read(ref _openFail)
                + " queries=" + Interlocked.Read(ref _queryCount)
                + " conn_opened=" + Interlocked.Read(ref _connOpened)
                + " conn_closed=" + Interlocked.Read(ref _connClosed)
                + " conn_inflight=" + Interlocked.Read(ref _connInflight);
        }
    }
}
