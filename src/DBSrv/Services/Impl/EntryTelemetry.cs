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

        // **6000 面（game-svr↔DBSrv 单连接）六桶**（卡 `openmir2-db-link-request-drop`）
        private static long _dbBytesRead;
        private static long _dbFramesDecoded;
        private static long _dbParseError;
        private static long _dbSignReject;
        private static long _dbFailReply;
        private static long _dbBadRequestInfo;

        private static long _requestSeen;
        private static long _respondedOk;
        private static long _respondedFail;
        private static long _droppedNoSession;
        private static long _sampleReplyFail;
        private static long _sampleDroppedNoSession;

        /// <summary>6000 面：解出帧的 **body 字节数**累计（原始 socket 字节不由 TouchSocket 暴露）。</summary>
        public static long AddDbBytesRead(long n) => Interlocked.Add(ref _dbBytesRead, n);

        /// <summary>6000 面：适配器解出并交给 `Received` 的帧数（= 帧解码成功）。</summary>
        public static long DbFramesDecoded() => Interlocked.Increment(ref _dbFramesDecoded);

        /// <summary>6000 面：解析失败（PacketCode 不符 / 反序列化异常）。</summary>
        public static long DbParseError() => Interlocked.Increment(ref _dbParseError);

        /// <summary>6000 面：签名不一致 ⇒ 关连接 的次数。</summary>
        public static long DbSignReject() => Interlocked.Increment(ref _dbSignReject);

        /// <summary>6000 面：`packetLen/queryId/Packet/Sign` 闸门拒绝（回 DBR_FAIL）的次数。</summary>
        public static long DbFailReply() => Interlocked.Increment(ref _dbFailReply);

        /// <summary>6000 面：`Received` 收到但 RequestInfo 不是本适配器类型（静默丢弃点）。</summary>
        public static long DbBadRequestInfo() => Interlocked.Increment(ref _dbBadRequestInfo);

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

        /// <summary>心跳间隔（毫秒）：值没变则不打；终值行**不走**这条去重。</summary>
        public const long HeartbeatIntervalMs = 10000;

        private static long _lastHeartbeatTick;
        private static string _lastSummary;

        /// <summary>
        /// 周期心跳：到点（<see cref="HeartbeatIntervalMs"/>）且**值有变化**才返回一行；
        /// 否则返回 <c>null</c>。被强杀时最后一行最多旧一个心跳周期。
        /// </summary>
        public static string Heartbeat(long nowTickMs)
        {
            if (nowTickMs - Interlocked.Read(ref _lastHeartbeatTick) < HeartbeatIntervalMs)
            {
                return null;
            }
            Interlocked.Exchange(ref _lastHeartbeatTick, nowTickMs);
            var line = SummaryLine();
            if (line == _lastSummary)
            {
                return null; // 值没变就不打（只作用于心跳）
            }
            _lastSummary = line;
            return line;
        }

        /// <summary>
        /// **关服终值**：无条件返回一行（带 `final` 标识）。
        /// **绝不去重**——即使与最后一行心跳完全相同也必须打印：
        /// 去重吞掉终值就会让整轮只剩百位快照（卡 `openmir2-entry-load-reply-buckets` 的验收增补）。
        /// </summary>
        public static string FinalLine()
        {
            return $"[EntryTelemetry] final {SummaryLine()}";
        }

        /// <summary>四桶一行汇总（字段名固定，供日志侧机判）。</summary>
        public static string SummaryLine()
        {
            return $"[EntryTelemetry] load_request_seen={Interlocked.Read(ref _requestSeen)} load_responded_ok={Interlocked.Read(ref _respondedOk)} load_responded_fail={Interlocked.Read(ref _respondedFail)} load_dropped_no_session={Interlocked.Read(ref _droppedNoSession)} db_conn_bytes_read={Interlocked.Read(ref _dbBytesRead)} db_conn_frames_decoded={Interlocked.Read(ref _dbFramesDecoded)} db_conn_parse_error={Interlocked.Read(ref _dbParseError)} db_conn_sign_reject={Interlocked.Read(ref _dbSignReject)} db_conn_fail_reply={Interlocked.Read(ref _dbFailReply)} db_conn_bad_request_info={Interlocked.Read(ref _dbBadRequestInfo)}";
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
            Interlocked.Exchange(ref _dbBytesRead, 0);
            Interlocked.Exchange(ref _dbFramesDecoded, 0);
            Interlocked.Exchange(ref _dbParseError, 0);
            Interlocked.Exchange(ref _dbSignReject, 0);
            Interlocked.Exchange(ref _dbFailReply, 0);
            Interlocked.Exchange(ref _dbBadRequestInfo, 0);
            Interlocked.Exchange(ref _lastHeartbeatTick, 0);
            _lastSummary = null;
        }
    }
}
