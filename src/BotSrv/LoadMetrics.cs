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

        // ==== 进世界后的挂机轮次（C6/M5 假人侧证据完整性 T-12）：F4 的"改前/改后"证据 ====
        // F4（reviewer-3 `a4c8091be9`）：`RunAutoPlay` 对**尚未进世界**的假人也执行、且作用在**跨假人共享**的
        // `MShare` 世界态上 ⇒ 按 bot 名记的挂机日志不能当作"该假人进世界"的证据。
        //   · `_autoPlayInWorld`        = 本会话已进世界（收到自己的 SM_LOGON）的挂机轮次（修复后只有它）
        //   · `_autoPlayPreWorld`       = 本会话**没有** SM_LOGON 却仍跑挂机的轮次（仅 `MIR2_BOT_LEGACY_SHARED_AUTOPLAY=1` 的对照组能产出）
        //   · `_autoPlaySkippedPreWorld`= 修复后挂机循环**因本会话未进世界而跳过**的次数
        // 判据：同一批假人，`legacy` 组 `autoPlayPreWorld>0`（共享态挂机日志与真实进世界数不符），修复组 `autoPlayPreWorld==0`。
        private static long _autoPlayInWorld;
        private static long _autoPlayPreWorld;
        private static long _autoPlaySkippedPreWorld;

        // ==== 牙齿测试钩子（T-12）：显式抹掉某个假人的"进世界证据" ====
        // `MIR2_BOT_SUPPRESS_INWORLD=<0 基序号>`：LoginId 尾部数字命中该序号的假人**照常进世界**，但**不计**
        // `in_world`（= 证据被抹掉）。用于证明"in_world/在线判据有牙齿"（抹掉一条 ⇒ 判据必红）。默认 -1=关。
        private static long _inWorldSuppressed;

        // ==== F3(a) 观测：CM_ADDNEWUSER 的"发出/没发出去" ====
        // bot-smoke 第三条假人"`CM_ADDNEWUSER` 后无应答"需要一个可读数字：把"注册包写进 socket"与
        // "socket 未连接而丢弃"分开；服务端回没回看既有的 `ident_counts[SM_NEWID_*]`。
        private static long _registerSent;
        private static long _registerSendClosed;

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
        private static long _actFrames;          // 收到的动作短包（datablock[0]=='+'）次数
        private static long _goodTickFrames;     // 其中前缀为 "+GD/" 的次数（ack 已到达 BotSrv，先于解析）
        private static string _actFirstPayload = "";
        private static long _probePosFollowed;   // 探针坐标跟随服务端位移的次数（推挤/空间移动）
        private static long _logonMisdelivered;  // 同一会话收到**别的 actor** 的 SM_LOGON 并拒绝的次数（错投观测）
        private static long _sceneNullActorSkipped; // BeginScene 跳过的 null 槽位次数（夹具健壮性）
        private static long _sceneListRaceSkipped; // BeginScene 因列表被并发改动而跳过的删除次数
        private static long _sceneIndexRaceSkipped; // BeginScene 轮内下标读取被并发缩表跳过的次数
        private static long _sceneRounds;          // BeginScene 实际进入 ActorList 迭代的轮次（0.5% 上界的分母）
        private static long _sceneActorEjected;    // actor 被移出 ActorList 并立即解除引用的累计次数（泄漏修复观测）

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

        // 卡 `openmir2-botsrv-loginconns-buffer`：跨 flush 复用的落盘缓冲（稳态零大块分配）。
        // 只在 Sync 锁内使用；初值小，首次按峰值增长一次后复用（不再是每次 count*160+64 的新 char[]）。
        private static readonly StringBuilder LoginConnsText = new StringBuilder(1024);
        private static readonly Encoding LoginConnsEncoding = new UTF8Encoding(false);
        private static readonly Encoder LoginConnsUtf8 = LoginConnsEncoding.GetEncoder();
        private static byte[] _loginConnBytes = new byte[1024];

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

        /// <summary>收到一条动作短包（`datablock[0]=='+'`，如 `+GD/`）。用于区分「ack 到了 BotSrv
        /// 但解析/记账坏了」与「ack 根本没到 BotSrv」——两者都表现为 samples=0。</summary>
        public static void ActFrameSeen()
        {
            lock (Sync)
            {
                _actFrames++;
            }
        }

        /// <summary>探针坐标跟随了一次服务端位移（`SM_BACKSTEP`/`SM_SPACEMOVE_SHOW*`）。</summary>
        public static void ProbePosFollowed()
        {
            lock (Sync)
            {
                _probePosFollowed++;
            }
        }

        /// <summary>同一会话已绑定 actor 之后，又收到**别的 actor** 的 `SM_LOGON` 并被身份守卫拒绝。
        /// 只作观测：跨连接错投的根因已由 D-18（卡 `openmir2-sm-logon-cross-session-delivery`，
        /// merge `a5869cd6f`，closed）在网关 `SessionId` 撞号层修复，本计数让夹具侧不再静默接受。
        /// **口径（F12②）**：这是「同连接 actor 冲突被拒次数」，**不等于错投帧数**——错投帧先到时会
        /// 拒后到的正确帧（+1）；只有错投帧、没有正确帧时反而不计数。
        /// **禁止把 `logon_misdelivered=0` 读成「无错投」**。</summary>
        public static void LogonMisdelivered()
        {
            lock (Sync)
            {
                _logonMisdelivered++;
            }
        }

        /// <summary>自检/诊断读取用：被身份守卫拒绝的错投 `SM_LOGON` 累计次数。</summary>
        public static long LogonMisdeliveredCount
        {
            get { lock (Sync) { return _logonMisdelivered; } }
        }

        /// <summary>记下进程内第一条动作短包的原文（诊断 rtime 时钟口径用）。</summary>
        public static void ActFrameSample(string payload)
        {
            lock (Sync)
            {
                if (_actFirstPayload.Length == 0) _actFirstPayload = payload ?? "";
            }
        }

        /// <summary>动作短包中前缀为 `+GD/` 的那一类（服务端 tick ack）。必须在解析分支之前计入。</summary>
        public static void GoodTickSeen()
        {
            lock (Sync)
            {
                _goodTickFrames++;
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

        /// <summary>BeginScene 本轮遇到 `ActorList` 的 null 槽位并跳过（卡 `openmir2-botsrv-autoplay-nre`）。
        /// 真实 Delphi 客户端 `m_ActorList` 只在 UI 线程读写、不会出现 null；BotSrv 的收包线程与 run
        /// 线程并发增删才会造成瞬时 null。计数只作观测：修复后应恒 0（不为它写白名单豁免）。</summary>
        public static void SceneNullActorSkipped()
        {
            lock (Sync)
            {
                _sceneNullActorSkipped++;
            }
        }

        /// <summary>自检读取用：BeginScene 跳过的 null 槽位累计次数。</summary>
        public static long SceneNullActorSkippedCount
        {
            get { lock (Sync) { return _sceneNullActorSkipped; } }
        }

        /// <summary>BeginScene 的删除点发现槽位已被并发改动（下标失效/已换人）而跳过删除
        /// （卡 `openmir2-botsrv-autoplay-nre`）。修复后应恒 0；不写白名单豁免。</summary>
        public static void SceneListRaceSkipped()
        {
            lock (Sync)
            {
                _sceneListRaceSkipped++;
            }
        }

        public static long SceneListRaceSkippedCount
        {
            get { lock (Sync) { return _sceneListRaceSkipped; } }
        }

        /// <summary>BeginScene 轮内下标读取时列表已被并发缩短（F1；复审 6a669aba）。本轮结束并计数，
        /// 不抛 ArgumentOutOfRangeException。修复后应恒 0；不写白名单豁免。</summary>
        public static void SceneIndexRaceSkipped()
        {
            lock (Sync)
            {
                _sceneIndexRaceSkipped++;
            }
        }

        public static long SceneIndexRaceSkippedCount
        {
            get { lock (Sync) { return _sceneIndexRaceSkipped; } }
        }

        /// <summary>
        /// BeginScene 实际进入 `ActorList` 迭代的一轮（`MShare.MySelf == null` 的早退不算）。
        /// 它是总监裁决的 null-skip 上界分母：**稳态窗口内 `scene_null_actor_skipped` 轮次占比 &lt;0.5%**。
        /// 只计数，不改变任何行为。
        /// </summary>
        public static void SceneRoundStarted()
        {
            lock (Sync)
            {
                _sceneRounds++;
            }
        }

        public static long SceneRoundsCount
        {
            get { lock (Sync) { return _sceneRounds; } }
        }

        /// <summary>actor 被移出 ActorList 并立即解除引用（卡 `openmir2-botsrv-actor-pool-leak`）。
        /// AOI 驱逐 / DeleteActor / DelActor 三个出口共用；自由池已移除，因此本计数不含任何保留对象。
        /// 只作观测：它让「驱逐发生过」与「驱逐后不再留存」在压测产物里可分辨。</summary>
        public static void SceneActorEjected()
        {
            lock (Sync)
            {
                _sceneActorEjected++;
            }
        }

        public static long SceneActorEjectedCount
        {
            get { lock (Sync) { return _sceneActorEjected; } }
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

        // ==== F4：挂机轮次按"本会话是否进世界"分账（T-12）====

        /// <summary>本会话已进世界的假人跑了一轮挂机（修复后的正常路径）。</summary>
        public static void AutoPlayInWorld()
        {
            lock (Sync) { _autoPlayInWorld++; }
        }

        /// <summary>本会话**尚未**进世界的假人仍跑了一轮挂机（F4 缺陷的"改前"证据；仅 legacy 对照组）。</summary>
        public static void AutoPlayPreWorld()
        {
            lock (Sync) { _autoPlayPreWorld++; }
        }

        /// <summary>挂机循环因"本会话未进世界"跳过了一个假人（修复后的行为）。</summary>
        public static void AutoPlaySkippedPreWorld()
        {
            lock (Sync) { _autoPlaySkippedPreWorld++; }
        }

        /// <summary>牙齿钩子（T-12）：本假人的进世界证据被显式抑制，不计入 `in_world`。</summary>
        public static void InWorldSuppressed()
        {
            lock (Sync) { _inWorldSuppressed++; }
        }

        // ==== F3(a)：注册包发出/丢弃观测（T-12）====

        /// <summary>CM_ADDNEWUSER 真正写进了 socket（只有这样服务端才可能回 SM_NEWID_*）。</summary>
        public static void RegisterSent()
        {
            lock (Sync) { _registerSent++; }
        }

        /// <summary>想发注册包但 socket 未连接（= 发送被丢弃，与"服务端没回"是两回事）。</summary>
        public static void RegisterSendClosed()
        {
            lock (Sync) { _registerSendClosed++; }
        }

        // ==== F4：legacy 对照组开关 + 牙齿抑制序号（T-12；默认都关，正常门禁行为与 T-11 逐字节一致）====

        /// <summary>`MIR2_BOT_LEGACY_SHARED_AUTOPLAY=1` 时恢复"只看共享 `MShare.MySelf`"的旧挂机门
        /// （用于产出 F4 的"改前"对照数字）。默认 false。</summary>
        private static readonly bool _legacySharedAutoPlay =
            Environment.GetEnvironmentVariable("MIR2_BOT_LEGACY_SHARED_AUTOPLAY") == "1";

        public static bool LegacySharedAutoPlay => _legacySharedAutoPlay;

        /// <summary>`MIR2_BOT_SUPPRESS_INWORLD=<0 基序号>`：抹掉该假人的进世界证据；-1=关。</summary>
        private static readonly int _suppressInWorldIndex = ResolveSuppressInWorldIndex();

        private static int ResolveSuppressInWorldIndex()
        {
            string v = Environment.GetEnvironmentVariable("MIR2_BOT_SUPPRESS_INWORLD");
            if (!string.IsNullOrEmpty(v) && int.TryParse(v, out int i) && i >= 0)
            {
                return i;
            }
            return -1;
        }

        /// <summary>本假人（LoginId 尾部数字 = 序号）的进世界证据是否被显式抑制。</summary>
        public static bool ShouldSuppressInWorld(string loginId)
        {
            if (_suppressInWorldIndex < 0 || string.IsNullOrEmpty(loginId))
            {
                return false;
            }
            int end = loginId.Length;
            int start = end;
            while (start > 0 && loginId[start - 1] >= '0' && loginId[start - 1] <= '9')
            {
                start--;
            }
            return start < end && int.TryParse(loginId.Substring(start), out int idx) && idx == _suppressInWorldIndex;
        }

        /// <summary>
        /// 一轮自动挂机里被兜住的异常（压测进程不许被单个假人打死；异常数进报告）。
        /// **必须带类别**：`internal_errors` 是 Σ(计入该字段的错误类别)，
        /// load_report 会校验 `internal_errors == Σ(error_census 的 feeds_internal 类别)`；
        /// 无类别调用落到 `internal_error_unclassified`（可见的故障，不会静默）。
        /// </summary>
        public static void InternalError(string category)
        {
            lock (Sync)
            {
                _internalErrors++;
                // 同一临界区内累加 census：StatsLine（持 Sync）读两者，避免 flush 快照读到
                // internal_errors=N 而 census_sum=N-1 的瞬时对账不平（复审 minor）。
                // 锁顺序固定 Sync → BotErrorCensus.Sync；BotErrorCensus 不回取 Sync，无死锁。
                BotErrorCensus.Count(category);
            }
        }

        /// <summary>未分类兜底：任何新增/漏改的 catch 若还走无参入口，会在 census 里以该类别露出。</summary>
        public static void InternalError()
        {
            InternalError(BotErrorCensus.InternalErrorUnclassified);
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
                // 采集失败不能让被测进程崩（压测期间磁盘/句柄异常都算环境问题，由驱动侧可见）。
                // 但**不能静默**：证据链断了要让报告看得见（卡 openmir2-botsrv-recv-thread-nre）。
                RecordMetricFlushFailure();
            }
        }

        /// <summary>统计落盘失败（Flush / login_conns 重写）。census-only：这是环境/证据链问题，
        /// 不计入 `internal_errors`（`internal_errors` 只表示代码级故障）。</summary>
        public static void RecordMetricFlushFailure()
        {
            BotErrorCensus.MetricFlushFailed();
        }

        /// <summary>进程级故障（AppDomain/未观察 Task）尽力落一行到 fatal.ndjson；写不了就放弃（不能让
        /// 异常处理再抛一个异常）。</summary>
        public static void WriteFatalMarker(string category, Exception ex)
        {
            try
            {
                string dir = _outDir;
                if (string.IsNullOrEmpty(dir))
                {
                    return;
                }
                Directory.CreateDirectory(dir);
                string type = ex == null ? "" : ex.GetType().FullName;
                string msg = ex == null ? "" : ex.Message;
                string line = "{\"t_ms\":" + Environment.TickCount
                    + ",\"category\":\"" + JsonEsc(category)
                    + "\",\"exception\":\"" + JsonEsc(type)
                    + "\",\"message\":\"" + JsonEsc(msg) + "\"}\n";
                File.AppendAllText(Path.Combine(dir, "fatal.ndjson"), line, new UTF8Encoding(false));
                Flush();
            }
            catch
            {
                // 尽力而为：崩溃路径上不允许二次抛出。
            }
        }

        private static string JsonEsc(string s)
        {
            if (string.IsNullOrEmpty(s))
            {
                return "";
            }
            var sb = new StringBuilder(s.Length + 8);
            foreach (char c in s)
            {
                switch (c)
                {
                    case '"': sb.Append("\\\""); break;
                    case '\\': sb.Append("\\\\"); break;
                    case '\n': sb.Append("\\n"); break;
                    case '\r': sb.Append("\\r"); break;
                    case '\t': sb.Append("\\t"); break;
                    default:
                        if (c < ' ')
                        {
                            sb.Append("\\u").Append(((int)c).ToString("x4", CultureInfo.InvariantCulture));
                        }
                        else
                        {
                            sb.Append(c);
                        }
                        break;
                }
            }
            return sb.ToString();
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

        /// <summary>T-11 连接级明细落盘（整文件重写，每 flush 一次）。调用方持锁。
        ///
        /// 卡 `openmir2-botsrv-loginconns-buffer`：旧实现每次 flush 都 `new StringBuilder(count*160+64)`
        /// 再 `ToString()` + `File.WriteAllText` —— 500 档下约 160KB + 234KB / 5s 的确定性 LOH 垃圾。
        /// 现改为**跨 flush 复用**：
        ///   · `LoginConnsText` 用 `Clear()` 复用同一 char 缓冲，稳态不再分配大块文本；
        ///   · UTF-8 直接编码进复用的 `_loginConnBytes`（逐 chunk，无 `ToString()`/`GetBytes(string)`）；
        ///   · 文件仍每次 `FileMode.Create` **全量重写** ⇒ 落盘字节与旧实现逐字节相同
        ///     （`--selftest-loadmetrics` 用旧实现复刻做 diff=0 断言并钉 golden hash）。
        /// 不变量：Flush 周期（5s）、字段、字段顺序、行序、换行、UTF-8 无 BOM 全部不变；不是增量写、不降频。
        /// </summary>
        private static void WriteLoginConnsLocked()
        {
            if (string.IsNullOrEmpty(_loginConnPath))
            {
                return;
            }
            StringBuilder sb = LoginConnsText;
            sb.Clear();
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
            EnsureLoginConnBytesCapacity(sb.Length);
            int bytes = EncodeLoginConnsUtf8(sb, _loginConnBytes);
            try
            {
                using (var fs = new FileStream(_loginConnPath, FileMode.Create, FileAccess.Write, FileShare.Read))
                {
                    fs.Write(_loginConnBytes, 0, bytes);
                }
            }
            catch
            {
                // 同 Flush：采集失败不能让被测进程崩。但必须可见（证据链断了也要能判红）。
                RecordMetricFlushFailure();
            }
        }

        /// <summary>按 UTF-8 最坏情况给复用缓冲定容（P1：**不得先测量字节数**）。
        ///
        /// 教训：`Encoder.GetByteCount(span, flush:false)` 逐 chunk 的计数与 stateful
        /// `GetBytes(span, dst, flush:false)` 对「代理对被 `GetChunks()` 切口拆开」的处理不一致
        /// （实测 `StringBuilder(1)` + `U+D83D` + `U+DE00` + 40×'b'：measure=43 / actual=44），
        /// 按测量值定容会在 flush 时抛 `ArgumentException`（被 Flush 的 catch 吞成
        /// `RecordMetricFlushFailure`，该次不重写）。这里改用 `Encoding.GetMaxByteCount(chars)` 上界：
        /// 缓冲按峰值只增长一次，稳态仍是零分配；写盘字节由 stateful `GetBytes` 的返回值决定。</summary>
        private static void EnsureLoginConnBytesCapacity(int charCount)
        {
            int need = LoginConnsEncoding.GetMaxByteCount(charCount);
            if (_loginConnBytes.Length < need)
            {
                _loginConnBytes = new byte[need];
            }
        }

        /// <summary>把登录明细按 UTF-8（无 BOM）写进复用缓冲，返回写入字节数。`dst` 必须按
        /// `EnsureLoginConnBytesCapacity` 的上界定容（stateful `GetBytes` 负责跨 chunk 代理对）。</summary>
        private static int EncodeLoginConnsUtf8(StringBuilder sb, byte[] dst)
        {
            Encoder enc = LoginConnsUtf8;
            enc.Reset();
            int off = 0;
            foreach (ReadOnlyMemory<char> chunk in sb.GetChunks())
            {
                off += enc.GetBytes(chunk.Span, dst.AsSpan(off), flush: false);
            }
            off += enc.GetBytes(ReadOnlySpan<char>.Empty, dst.AsSpan(off), flush: true);
            return off;
        }

        // ==== 自检接缝（卡 `openmir2-botsrv-loginconns-buffer`；只供 `--selftest-loadmetrics`，不参与运行时路径）====

        /// <summary>自检专用：只设置 login_conns 输出路径，不启动 flusher、不改其它运行时状态。</summary>
        internal static void SelfCheckSetLoginConnPath(string path)
        {
            lock (Sync) { _loginConnPath = path; }
        }

        /// <summary>自检专用：清空连接表与复用缓冲（模拟全新进程的初始态，让分配量可重复测量）。</summary>
        internal static void SelfCheckResetLoginConns()
        {
            lock (Sync)
            {
                LoginConns.Clear();
                LoginConnsText.Clear();
                LoginConnsUtf8.Reset();
                _loginConnBytes = new byte[1024];
            }
        }

        /// <summary>自检专用：直接写入一条连接记录（字段语义与 LoginConnStart 建立的对象一致）。</summary>
        internal static void SelfCheckAddLoginConn(string key, LoginConn c)
        {
            lock (Sync) { LoginConns[key] = c; }
        }

        /// <summary>自检专用：读当前连接表（供「旧实现复刻」在同一枚举序上产出对照字节）。</summary>
        internal static Dictionary<string, LoginConn> SelfCheckLoginConns => LoginConns;

        /// <summary>自检专用：用与 `WriteLoginConnsLocked` **同一组**定容/编码函数处理任意 StringBuilder
        /// （跨 chunk 代理对用例需要自造 `StringBuilder` 容量以控制 `GetChunks()` 切口）。</summary>
        internal static byte[] SelfCheckEncodeText(StringBuilder sb)
        {
            lock (Sync)
            {
                EnsureLoginConnBytesCapacity(sb.Length);
                int n = EncodeLoginConnsUtf8(sb, _loginConnBytes);
                byte[] exact = new byte[n];
                Array.Copy(_loginConnBytes, exact, n);
                return exact;
            }
        }

        /// <summary>自检专用：把复用字节缓冲缩到指定大小。跨切口用例必须先缩，模拟「首次为某个
        /// 文本增长」的定容状态；否则前面的大用例已把缓冲撑大，会掩盖定容缺陷（假绿）。</summary>
        internal static void SelfCheckSetLoginConnBytesBufferSize(int size)
        {
            lock (Sync) { _loginConnBytes = new byte[size < 0 ? 0 : size]; }
        }

        /// <summary>自检专用：当前复用字节缓冲长度（元断言用）。</summary>
        internal static int SelfCheckLoginConnBytesBufferLength => _loginConnBytes.Length;

        /// <summary>自检专用：在调用线程上执行一次真实落盘（与 Flush 相同的 Sync 锁语义），便于测分配量。</summary>
        internal static void SelfCheckWriteLoginConns()
        {
            lock (Sync) { WriteLoginConnsLocked(); }
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
              .Append(",\"scene_null_actor_skipped\":").Append(_sceneNullActorSkipped)
              .Append(",\"scene_list_race_skipped\":").Append(_sceneListRaceSkipped)
              .Append(",\"scene_index_race_skipped\":").Append(_sceneIndexRaceSkipped)
              .Append(",\"scene_rounds\":").Append(_sceneRounds)
              .Append(",\"scene_actor_ejected\":").Append(_sceneActorEjected)
              // 卡 openmir2-botsrv-recv-thread-nre：错误类别普查（封闭集合，逐类计数）。
              // load_report 对缺键/多键/类别非 0 都 fail-closed；schema 变更必须同步改两边。
              .Append(",\"error_census_schema\":1")
              .Append(",\"error_census\":").Append(BotErrorCensus.BuildJson())
              .Append(",\"in_world\":").Append(_inWorld)
              .Append(",\"in_world_suppressed\":").Append(_inWorldSuppressed)
              .Append(",\"autoplay_in_world\":").Append(_autoPlayInWorld)
              .Append(",\"autoplay_pre_world\":").Append(_autoPlayPreWorld)
              .Append(",\"autoplay_skipped_pre_world\":").Append(_autoPlaySkippedPreWorld)
              .Append(",\"register_sent\":").Append(_registerSent)
              .Append(",\"register_send_closed\":").Append(_registerSendClosed)
              .Append(",\"probe_sent\":").Append(_probeSent)
              .Append(",\"probe_pos_updates\":").Append(_probePosFollowed)
              .Append(",\"logon_misdelivered\":").Append(_logonMisdelivered)
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
              .Append(",\"act_frames\":").Append(_actFrames)
              .Append(",\"good_tick_frames\":").Append(_goodTickFrames)
              .Append(",\"act_first\":\"").Append(_actFirstPayload).Append("\"")
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
