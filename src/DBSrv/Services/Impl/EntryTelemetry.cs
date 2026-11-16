using System.Threading;

namespace DBSrv.Services.Impl
{
    /// <summary>
    /// **入口遥测 · DBSrv 5100 取档面四桶**（卡 `openmir2-entry-load-reply-buckets`）。
    ///
    /// 目的：把「GameSvr 发了取档请求、却没进入世界」的缺口分成
    /// 「请求没到 DBSrv」「到了但静默丢弃」「回了失败码」三类。四个桶：
    /// - <see cref="LoadRequestSeen"/>：`DB_LOADHUMANRCD` 到达 5100 面的请求数；
    /// - <see cref="LoadRespondedOk"/>：回 `DBR_LOADHUMANRCD` 且 `recog=1`（成功））；
    /// - <see cref="LoadRespondedFail"/>：回 `DBR_LOADHUMANRCD` 且 `recog=nCheckCode&lt;0`（失败码）；
    /// - <see cref="LoadDroppedNoSession"/>：`LoadHumanRcd` 里 `SessionID &lt;= 0` 时的**静默丢弃**
    ///   （原实现直接 `return`：不回包、不打日志 ⇒ 上游永远等不到应答）。
    ///
    /// **纯旁路计数**：不改任何判定/回包；独立成类是为了让四桶可单测、可变异
    /// （`DataService` 里只允许各调用一次，接线由测试侧的源码断言钉住）。
    /// </summary>
    public static class EntryTelemetry
    {
        /// <summary>每类样本最多打印几条（前 N 条原样字段；之后只计数）。</summary>
        public const long SampleLimit = 5;

        private static long _requestSeen;
        private static long _respondedOk;
        private static long _respondedFail;
        private static long _droppedNoSession;
        private static long _sampleReplyFail;
        private static long _sampleDroppedNoSession;

        /// <summary>请求到达 +1，返回累计值（调用方用返回值判「每 100 个打一行」）。</summary>
        public static long LoadRequestSeen()
        {
            return Interlocked.Increment(ref _requestSeen);
        }

        /// <summary>成功回包 +1。</summary>
        public static long LoadRespondedOk()
        {
            return Interlocked.Increment(ref _respondedOk);
        }

        /// <summary>失败码回包 +1。</summary>
        public static long LoadRespondedFail()
        {
            return Interlocked.Increment(ref _respondedFail);
        }

        /// <summary>`SessionID &lt;= 0` 静默丢弃 +1。</summary>
        public static long LoadDroppedNoSession()
        {
            return Interlocked.Increment(ref _droppedNoSession);
        }

        /// <summary>失败码样本是否该打：前 <see cref="SampleLimit"/> 条为 true，之后恒 false。</summary>
        public static bool SampleReplyFail()
        {
            return Interlocked.Increment(ref _sampleReplyFail) <= SampleLimit;
        }

        /// <summary>静默丢弃样本是否该打：前 <see cref="SampleLimit"/> 条为 true，之后恒 false。</summary>
        public static bool SampleDroppedNoSession()
        {
            return Interlocked.Increment(ref _sampleDroppedNoSession) <= SampleLimit;
        }

        /// <summary>四桶一行汇总（字段名固定，供日志侧机判）。</summary>
        public static string SummaryLine()
        {
            return $"[EntryTelemetry] load_request_seen={Interlocked.Read(ref _requestSeen)} load_responded_ok={Interlocked.Read(ref _respondedOk)} load_responded_fail={Interlocked.Read(ref _respondedFail)} load_dropped_no_session={Interlocked.Read(ref _droppedNoSession)}";
        }

        /// <summary>测试专用：清零全部计数（生产路径不调用）。</summary>
        public static void ResetForTests()
        {
            Interlocked.Exchange(ref _requestSeen, 0);
            Interlocked.Exchange(ref _respondedOk, 0);
            Interlocked.Exchange(ref _respondedFail, 0);
            Interlocked.Exchange(ref _droppedNoSession, 0);
            Interlocked.Exchange(ref _sampleReplyFail, 0);
            Interlocked.Exchange(ref _sampleDroppedNoSession, 0);
        }
    }
}
