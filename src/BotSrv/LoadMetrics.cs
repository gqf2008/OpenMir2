using System;
using System.Collections.Generic;
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
        private static long _internalErrors;
        private static long _internalErrorsLogged;
        private static long _probeSent;
        private static long _inWorld;

        // ==== 登录响应观测（C6 第 4 轮，T-8）：把「假人侧发了多少 / 收到哪些回包」变成可读数字 ====
        // 目的：tier-500/1000 登录卡在 ~350-390 时，先判「假人没发对包」还是「服务端没回/回错」，
        // 而不是靠 bots.log 里的中文行数反推（那条口径漏计了"发送时 socket 未连接"的假人）。
        private static long _loginBegin;         // 走到 SendLogin（= 日志「开始登陆」）
        private static long _loginSent;          // CM_IDPASSWORD 真正写进 socket 的次数
        private static long _loginSendClosed;    // 因 socket 未连接而**没发出去**的次数
        private static long _connAttempt;         // LoginScene 发起的 Connect 次数
        private static long _connOk;              // LoginScene 的 OnConnected 次数
        private static long _connErrRefused;
        private static long _connErrReset;
        private static long _connErrTimeout;
        private static long _connErrOther;
        private static long _recvChunks;          // 收到的原始字节块数（未组帧）
        private static long _recvChunkBytes;
        private static long _framesDecoded;       // 成功解出 CommandMessage 的帧数
        private static long _chunksWithoutHash;   // 不带 '#' 的块（组帧错位/残帧尾）
        private static long _partialFrames;       // 一次读里不是完整帧、被**静默丢弃**的残帧次数
        private static long _partialFrameBytes;   // 上面丢弃的字节数
        private static readonly Dictionary<int, long> IdentCounts = new Dictionary<int, long>();
        private const int MaxIdentKeys = 48;      // 直方图键上限（防异常输入把直方图撑爆）
        private static long _identOverflow;

        // ==== 登录连接级观测（C6 第 5 轮，T-11）：把「残余丢包」的方向证出来 ====
        // T-8 的 ident 直方图只能证明「服务端回的总数 < 假人数」，分不出这两种：
        //   (i)  服务端在这条 TCP 连接上**一个字节都没回**（rx_bytes == 0）⇒ 丢在上游（C# 栈）；
        //   (ii) 服务端回了、假人侧收包/解析丢了（rx_bytes > 0 但没解出 529）。
        // 所以按连接（本假人的登录 socket）记下：发出登录包的时刻 / 该连接的 rx 块数与字节数 /
        // 是否解出 SM_PASSOK_SELECTSERVER。另带一条自证对照：窗口内没应答的连接，在同一连接上**重发一次**
        // （`MIR2_BOT_LOGIN_RESEND_MS`，默认 0=关；第二次是否拿到应答直接区分「连接不可用」与「首次请求被服务端丢了」）。
        public sealed class LoginConn
        {
            public int ConnSeq;          // 本假人第几条登录 socket（同进程内单调递增）
            public long LoginTick;       // 发出 CM_IDPASSWORD 的时刻（Environment.TickCount）
            public int LoginSends;       // 本连接上发登录包的次数（正常 1，重发后 2）
            public long RxChunks;        // 本连接的原始读块数
            public long RxBytes;         // 本连接的原始字节数（0 ⇒ 服务端没在这条连接上写过任何东西）
            public bool Saw529;          // 解出过 SM_PASSOK_SELECTSERVER
            public int FirstRespIdent;   // 第一个解出的 ident（0 = 一个都没解出）
            public long FirstRespTick;
            public int LastIdent;
            public int ResendSends;
            public long ResendTick;
        }

        private static readonly Dictionary<string, LoginConn> LoginConns = new Dictionary<string, LoginConn>();
        private static string _loginConnPath;
        private static long _loginSendFailed;   // 想发但 socket.Send 抛异常（与「没连接」分开）
        private static long _loginResendWindowMs; // 0 = 关（默认，保持既有行为）

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
                _loginConnPath = Path.Combine(_outDir, "login_conns.ndjson");
                _loginResendWindowMs = ResolveLoginResendWindow();
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

        /// <summary>登录重发对照窗口（ms）。`MIR2_BOT_LOGIN_RESEND_MS`，默认 0=关（不开时假人行为与第 4 轮逐字节一致）。</summary>
        private static long ResolveLoginResendWindow()
        {
            string v = Environment.GetEnvironmentVariable("MIR2_BOT_LOGIN_RESEND_MS");
            if (!string.IsNullOrEmpty(v) && long.TryParse(v, out long ms) && ms > 0)
            {
                return ms;
            }
            return 0;
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

        /// <summary>动作探针真正把一条动作发出去了（进世界之后）。0 表示"探针没武装/没进世界"，
        /// 与"发了但收不到 ack"（tick_samples = 0）是两种完全不同的故障，必须分开看得见。</summary>
        public static void ProbeSent()
        {
            lock (Sync)
            {
                _probeSent++;
            }
        }

        /// <summary>这个假人收到了服务端的 SM_LOGON（= 真的进了世界，而不是"本地切到 PlayScene"）。
        /// 探针的武装条件就是它；它同时是 tick 样本覆盖率的分子（分母 = 档位人数）。</summary>
        public static void InWorld()
        {
            lock (Sync)
            {
                _inWorld++;
            }
        }

        /// <summary>一轮自动挂机里被兜住的异常（压测进程不许被单个假人打死；异常数进报告）。</summary>
        public static void InternalError()
        {
            lock (Sync)
            {
                _internalErrors++;
            }
        }

        /// <summary>是否需要打印这一条异常（前若干条打印，避免压测日志被刷爆）。返回 true 表示该打印。</summary>
        public static bool ShouldLogInternalError(int maxLogged = 20)
        {
            lock (Sync)
            {
                if (_internalErrorsLogged >= maxLogged)
                {
                    return false;
                }
                _internalErrorsLogged++;
                return true;
            }
        }

        public static long InternalErrors
        {
            get { lock (Sync) { return _internalErrors; } }
        }

        /// <summary>登录之后连接被断开/超时 = 掉线（与"连不上"分开计）。</summary>
        public static void ConnLost()
        {
            lock (Sync)
            {
                _connLost++;
            }
        }

        // ==== T-8 登录响应观测：计数入口（全部只读语义，不改变假人行为）====

        /// <summary>走到 SendLogin（日志「开始登陆」）——注意它**早于**真正写 socket。</summary>
        public static void LoginBegin()
        {
            lock (Sync) { _loginBegin++; }
        }

        /// <summary>CM_IDPASSWORD 真正写进了 socket（只有这样服务端才可能回包）。</summary>
        public static void LoginSent()
        {
            lock (Sync) { _loginSent++; }
        }

        /// <summary>想发登录包但 socket 未连接（= 发送被丢弃，与「服务端没回」是两回事）。</summary>
        public static void LoginSendClosed()
        {
            lock (Sync) { _loginSendClosed++; }
        }

        // ==== T-11 登录连接级观测：入口 ====

        /// <summary>即将在本连接上发登录包（建/更新本连接的记录，记下发出时刻）。</summary>
        public static void LoginConnStart(string key, int connSeq)
        {
            if (string.IsNullOrEmpty(key))
            {
                return;
            }
            lock (Sync)
            {
                if (!LoginConns.TryGetValue(key, out LoginConn c) || c == null)
                {
                    c = new LoginConn();
                    LoginConns[key] = c;
                }
                c.ConnSeq = connSeq;
                c.LoginTick = Environment.TickCount;
                c.LoginSends++;
            }
        }

        /// <summary>登录包的发送结果（socket.Send 是否真的没抛异常）。</summary>
        public static void LoginConnSent(string key, bool sent)
        {
            if (sent)
            {
                return;
            }
            lock (Sync)
            {
                _loginSendFailed++;
            }
        }

        /// <summary>本登录连接上收到一块原始字节（未组帧）。rx_bytes == 0 ⇒ 服务端没在这条连接上写过任何东西。</summary>
        public static void LoginConnRecv(string key, int bytes)
        {
            if (string.IsNullOrEmpty(key))
            {
                return;
            }
            lock (Sync)
            {
                if (LoginConns.TryGetValue(key, out LoginConn c) && c != null)
                {
                    c.RxChunks++;
                    c.RxBytes += bytes;
                }
            }
        }

        /// <summary>本登录连接上解出一帧（任何 ident）。</summary>
        public static void LoginConnIdent(string key, int ident)
        {
            if (string.IsNullOrEmpty(key))
            {
                return;
            }
            lock (Sync)
            {
                if (LoginConns.TryGetValue(key, out LoginConn c) && c != null)
                {
                    c.LastIdent = ident;
                    if (c.FirstRespIdent == 0)
                    {
                        c.FirstRespIdent = ident;
                        c.FirstRespTick = Environment.TickCount;
                    }
                    if (ident == OpenMir2.Messages.SM_PASSOK_SELECTSERVER)
                    {
                        c.Saw529 = true;
                    }
                }
            }
        }

        /// <summary>本连接是否到了「窗口内没拿到登录应答」而该重发一次对照包的时刻。</summary>
        public static bool LoginConnNeedsResend(string key, int nowTick)
        {
            long window = _loginResendWindowMs;
            if (window <= 0 || string.IsNullOrEmpty(key))
            {
                return false;
            }
            lock (Sync)
            {
                if (!LoginConns.TryGetValue(key, out LoginConn c) || c == null)
                {
                    return false;
                }
                if (c.Saw529 || c.ResendSends > 0)
                {
                    return false;
                }
                return (nowTick - c.LoginTick) >= window;
            }
        }

        /// <summary>重发了一次登录包（对照）。返回 false = 本次不该重发（调用方无需记）。</summary>
        public static void LoginConnResend(string key)
        {
            if (string.IsNullOrEmpty(key))
            {
                return;
            }
            lock (Sync)
            {
                if (LoginConns.TryGetValue(key, out LoginConn c) && c != null)
                {
                    c.ResendSends++;
                    c.ResendTick = Environment.TickCount;
                }
            }
        }

        public static void ConnectAttempt()
        {
            lock (Sync) { _connAttempt++; }
        }

        public static void ConnectOk()
        {
            lock (Sync) { _connOk++; }
        }

        public static void ConnectError(System.Net.Sockets.SocketError code)
        {
            lock (Sync)
            {
                switch (code)
                {
                    case System.Net.Sockets.SocketError.ConnectionRefused: _connErrRefused++; break;
                    case System.Net.Sockets.SocketError.ConnectionReset: _connErrReset++; break;
                    case System.Net.Sockets.SocketError.TimedOut: _connErrTimeout++; break;
                    default: _connErrOther++; break;
                }
            }
        }

        /// <summary>一次 socket 读（未组帧的原始块）。</summary>
        public static void RecvChunk(int bytes)
        {
            lock (Sync)
            {
                _recvChunks++;
                _recvChunkBytes += bytes;
            }
        }

        /// <summary>解出了一帧 CommandMessage（任何 ident）。</summary>
        public static void FrameDecoded()
        {
            lock (Sync) { _framesDecoded++; }
        }

        /// <summary>收到的块里连 '#' 都没有（组帧错位或残帧尾）。</summary>
        public static void ChunkWithoutHash()
        {
            lock (Sync) { _chunksWithoutHash++; }
        }

        /// <summary>
        /// 残帧：这一读里的数据不是完整帧（没有 '!'）——旧行为是**静默丢弃**（没有跨读缓存）。
        /// 帧被 TCP 切开时，前半截与后半截都会丢，服务端回了包但假人永远看不到。
        /// </summary>
        public static void PartialFrame(int bytes)
        {
            lock (Sync)
            {
                _partialFrames++;
                _partialFrameBytes += bytes;
            }
        }

        /// <summary>按 ident 计数（登录卡死时用来判「服务端回了别的包」还是「什么都没回」）。</summary>
        public static void IdentSeen(int ident)
        {
            lock (Sync)
            {
                if (IdentCounts.TryGetValue(ident, out long n))
                {
                    IdentCounts[ident] = n + 1;
                }
                else if (IdentCounts.Count < MaxIdentKeys)
                {
                    IdentCounts[ident] = 1;
                }
                else
                {
                    _identOverflow++;
                }
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
                    // T-11：连接级明细（整文件重写；1000 连接 ≈ 160KB，远小于 tick 直方图落盘量）
                    WriteLoginConnsLocked();
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

        /// <summary>T-11 连接级明细落盘（整文件重写，每 flush 一次）。调用方持锁。</summary>
        private static void WriteLoginConnsLocked()
        {
            if (string.IsNullOrEmpty(_loginConnPath))
            {
                return;
            }
            var sb = new StringBuilder(LoginConns.Count * 160 + 64);
            foreach (KeyValuePair<string, LoginConn> kv in LoginConns)
            {
                LoginConn c = kv.Value;
                if (c == null)
                {
                    continue;
                }
                sb.Append("{\"key\":\"").Append(kv.Key)
                  .Append("\",\"conn_seq\":").Append(c.ConnSeq)
                  .Append(",\"login_tick\":").Append(c.LoginTick)
                  .Append(",\"sends\":").Append(c.LoginSends)
                  .Append(",\"rx_chunks\":").Append(c.RxChunks)
                  .Append(",\"rx_bytes\":").Append(c.RxBytes)
                  .Append(",\"saw_529\":").Append(c.Saw529 ? "true" : "false")
                  .Append(",\"first_resp_ident\":").Append(c.FirstRespIdent)
                  .Append(",\"first_resp_tick\":").Append(c.FirstRespTick)
                  .Append(",\"last_ident\":").Append(c.LastIdent)
                  .Append(",\"resend_sends\":").Append(c.ResendSends)
                  .Append(",\"resend_tick\":").Append(c.ResendTick)
                  .Append("}\n");
            }
            try
            {
                File.WriteAllText(_loginConnPath, sb.ToString(), new UTF8Encoding(false));
            }
            catch
            {
                // 同 Flush：采集失败不能让被测进程崩
            }
        }

        /// <summary>T-11 连接级汇总（调用方持锁）：总数 / 没拿到 529 的 / 其中 rx_bytes==0 的 / 做过重发的 / 重发后成功。</summary>
        private static void LoginConnSummaryLocked(out int total, out int noResp, out int noRespRx0,
            out int resend, out int resendOk)
        {
            total = LoginConns.Count;
            noResp = 0;
            noRespRx0 = 0;
            resend = 0;
            resendOk = 0;
            foreach (KeyValuePair<string, LoginConn> kv in LoginConns)
            {
                LoginConn c = kv.Value;
                if (c == null)
                {
                    continue;
                }
                if (!c.Saw529)
                {
                    noResp++;
                    if (c.RxBytes == 0)
                    {
                        noRespRx0++;
                    }
                }
                if (c.ResendSends > 0)
                {
                    resend++;
                    if (c.Saw529)
                    {
                        resendOk++;
                    }
                }
            }
        }

        /// <summary>ident 直方图（按次数降序，取前 24 个键）——判「服务端回了什么」用。调用方持锁。</summary>
        private static string IdentCountsJson()
        {
            if (IdentCounts.Count == 0)
            {
                return "{}";
            }
            var top = new List<KeyValuePair<int, long>>(IdentCounts);
            top.Sort((a, b) => b.Value.CompareTo(a.Value));
            int take = top.Count > 24 ? 24 : top.Count;
            var sb = new StringBuilder(256);
            sb.Append('{');
            for (int i = 0; i < take; i++)
            {
                if (i > 0)
                {
                    sb.Append(',');
                }
                sb.Append('"').Append(top[i].Key).Append("\":").Append(top[i].Value);
            }
            sb.Append('}');
            return sb.ToString();
        }

        /// <summary>累计快照（下游按相邻两行差分得到时间窗内的分布）。</summary>
        private static string StatsLine()
        {
            LoginConnSummaryLocked(out int lcTotal, out int lcNoResp, out int lcNoRespRx0,
                out int lcResend, out int lcResendOk);
            var sb = new StringBuilder(1400);
            sb.Append("{\"t_ms\":").Append(Environment.TickCount)
              .Append(",\"uptime_ms\":").Append(Environment.TickCount - _startTickMs)
              .Append(",\"spawned\":").Append(_spawned)
              .Append(",\"login_ok\":").Append(_loginOk)
              .Append(",\"conn_refused\":").Append(_connRefused)
              .Append(",\"conn_lost\":").Append(_connLost)
              .Append(",\"internal_errors\":").Append(_internalErrors)
              .Append(",\"in_world\":").Append(_inWorld)
              .Append(",\"probe_sent\":").Append(_probeSent)
              .Append(",\"login_begin\":").Append(_loginBegin)
              .Append(",\"login_sent\":").Append(_loginSent)
              .Append(",\"login_send_closed\":").Append(_loginSendClosed)
              .Append(",\"login_send_failed\":").Append(_loginSendFailed)
              .Append(",\"login_conn_total\":").Append(lcTotal)
              .Append(",\"login_conn_no_resp\":").Append(lcNoResp)
              .Append(",\"login_conn_no_resp_rx0\":").Append(lcNoRespRx0)
              .Append(",\"login_conn_resend\":").Append(lcResend)
              .Append(",\"login_conn_resend_ok\":").Append(lcResendOk)
              .Append(",\"conn_attempt\":").Append(_connAttempt)
              .Append(",\"conn_ok\":").Append(_connOk)
              .Append(",\"conn_err_refused\":").Append(_connErrRefused)
              .Append(",\"conn_err_reset\":").Append(_connErrReset)
              .Append(",\"conn_err_timeout\":").Append(_connErrTimeout)
              .Append(",\"conn_err_other\":").Append(_connErrOther)
              .Append(",\"recv_chunks\":").Append(_recvChunks)
              .Append(",\"recv_chunk_bytes\":").Append(_recvChunkBytes)
              .Append(",\"frames_decoded\":").Append(_framesDecoded)
              .Append(",\"chunks_without_hash\":").Append(_chunksWithoutHash)
              .Append(",\"partial_frames\":").Append(_partialFrames)
              .Append(",\"partial_frame_bytes\":").Append(_partialFrameBytes)
              .Append(",\"ident_overflow\":").Append(_identOverflow)
              .Append(",\"ident_counts\":").Append(IdentCountsJson())
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
