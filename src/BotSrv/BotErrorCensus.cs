using System;
using System.Collections.Generic;
using System.Net.Sockets;
using System.Text;
using System.Threading;

namespace BotSrv
{
    /// <summary>
    /// 夹具错误普查（卡 `openmir2-botsrv-recv-thread-nre`，总监 2026-11-15 规则）。
    ///
    /// **为什么要有它**：`internal_errors` 以前只数了「我们看得见的那几类」。N1 窗口的收包线程
    /// ~3.95 万条 `[ERR] Object reference not set` 只进了 `LogService.Error`、没进 any 计数；
    /// 之后又发现 AOORE 根本没进任何计数。所以判据从「我知道的两类为 0」改成
    /// **「枚举出的每一类都有计数，且全部为 0 或已白名单登记」**。
    ///
    /// 设计约束：
    /// 1. 类别表是**封闭集合**：`Order` 里的名字同时是 `load_stats.ndjson` 的 JSON 键；
    ///    `tools/botload/load_report.py` 的 `ERROR_CENSUS_CATEGORIES` 必须逐字一致（报告会校验
    ///    缺键/多键，两边的枚举漂移都会判红）。
    /// 2. 每个运行时 catch/线程入口/事件处理都在这里落一个类别；`--selftest-error-census`
    ///    对每一类做「注入该错误 ⇒ 计数 +1」的牙，删任一计数方法都会让自检红。
    /// 3. **只有代码级故障**才计入 `internal_errors`（`feeds_internal=true` 的类别）；
    ///    网络/磁盘/主动关闭等运行期事件单独计数、单独报告，不污染 `internal_errors` 的语义。
    ///    `load_report.py` 会校验 `internal_errors == Σ(feeds_internal 类别)`，任何绕过
    ///    `LoadMetrics.InternalError(category)` 直接加 `_internalErrors` 的路径都会被这条等式抓出来。
    /// 4. 本类不负责「修复」；它只保证**错误面完整可见**。修复/护栏在各调用点（见文档表）。
    /// </summary>
    public static class BotErrorCensus
    {
        // ==== 错误类别（封闭集合；顺序 = 报告顺序）====
        // 收包线程（ClientManager.ProcessReviceMessage 的 per-packet catch）
        public const string RecvPacketNull = "recv_packet_null";
        public const string RecvPacketIndex = "recv_packet_index";
        public const string RecvPacketOther = "recv_packet_other";

        // 每假人一轮（ClientManager.SafeRun 的 catch）
        public const string RunRoundNull = "run_round_null";
        public const string RunRoundIndex = "run_round_index";
        public const string RunRoundOther = "run_round_other";

        // 整轮调度（AppService.Run 的 runThread）
        public const string RunLoopException = "run_loop_exception";

        // socket 事件处理（OnConnected/OnDisconnected/OnReceivedData/OnError 被调用点）
        public const string SocketHandlerException = "socket_handler_exception";

        // socket 系统调用（只计「不该发生」的 SocketError；reset/refused/timeout 等属运行期事件）
        public const string SocketRecvFault = "socket_recv_fault";
        public const string SocketSendFault = "socket_send_fault";
        public const string SocketConnectFault = "socket_connect_fault";

        // 证据/采集链（失败会静默丢证据，必须可见）
        public const string MetricFlushFailure = "metric_flush_failure";
        public const string SelfReportWriteFailure = "self_report_write_failure";
        public const string DropExpWriterOpenFailure = "drop_exp_writer_open_failure";
        public const string DropExpWriterWriteFailure = "drop_exp_writer_write_failure";

        // 解析/进程级
        public const string BagItemDecodeFailure = "bag_item_decode_failure";
        public const string UnhandledException = "unhandled_exception";
        public const string UnobservedTaskException = "unobserved_task_exception";
        public const string InternalErrorUnclassified = "internal_error_unclassified";

        // 运行期事件（operational；允许非 0，但必须出现在报告里）
        public const string TeardownRace = "teardown_race";

        /// <summary>类别表（顺序固定；与 load_report.py 的 ERROR_CENSUS_CATEGORIES 逐字一致）。</summary>
        private static readonly string[] Order =
        {
            RecvPacketNull, RecvPacketIndex, RecvPacketOther,
            RunRoundNull, RunRoundIndex, RunRoundOther,
            RunLoopException, SocketHandlerException,
            SocketRecvFault, SocketSendFault, SocketConnectFault,
            MetricFlushFailure, SelfReportWriteFailure,
            DropExpWriterOpenFailure, DropExpWriterWriteFailure,
            BagItemDecodeFailure, UnhandledException, UnobservedTaskException,
            InternalErrorUnclassified, TeardownRace,
        };

        /// <summary>计入 `internal_errors` 的类别（其余类别只作运行期观测）。</summary>
        private static readonly HashSet<string> FeedsInternalErrors = new HashSet<string>(StringComparer.Ordinal)
        {
            RecvPacketNull, RecvPacketIndex, RecvPacketOther,
            RunRoundNull, RunRoundIndex, RunRoundOther,
            RunLoopException, SocketHandlerException,
            BagItemDecodeFailure, UnhandledException, UnobservedTaskException,
            InternalErrorUnclassified,
        };

        private static readonly Dictionary<string, int> Index = BuildIndex();
        private static readonly long[] Counts = new long[Order.Length];
        private static readonly object Sync = new object();

        private static Dictionary<string, int> BuildIndex()
        {
            var d = new Dictionary<string, int>(Order.Length, StringComparer.Ordinal);
            for (int i = 0; i < Order.Length; i++)
            {
                d[Order[i]] = i;
            }
            return d;
        }

        /// <summary>全部类别（只读；自检/报告用）。</summary>
        public static IReadOnlyList<string> Categories => Order;

        /// <summary>该类别是否计入 internal_errors（自检/跨语言校验用）。</summary>
        public static bool IsInternalErrorCategory(string category)
        {
            return category != null && FeedsInternalErrors.Contains(category);
        }

        /// <summary>计数 +1。未知类别落到 `internal_error_unclassified`（拼错/漏登记 ⇒ 故障可见，不会静默）。</summary>
        public static void Count(string category)
        {
            int i;
            if (category == null || !Index.TryGetValue(category, out i))
            {
                i = Index[InternalErrorUnclassified];
            }
            lock (Sync)
            {
                Counts[i]++;
            }
        }

        /// <summary>读某类别的累计值（自检用）。</summary>
        public static long Get(string category)
        {
            int i;
            if (category == null || !Index.TryGetValue(category, out i))
            {
                return -1;
            }
            lock (Sync)
            {
                return Counts[i];
            }
        }

        /// <summary>自检用：清零全部类别（只在 `--selftest-error-census` 进程里调用）。</summary>
        internal static void ResetForSelftest()
        {
            lock (Sync)
            {
                for (int i = 0; i < Counts.Length; i++)
                {
                    Counts[i] = 0;
                }
            }
        }

        /// <summary>把异常归到 (null / index / other) 三桶之一——三个运行时 choke point 共用同一分类器。</summary>
        public static string ClassifyFault(Exception ex, string nullCategory, string indexCategory, string otherCategory)
        {
            if (ex is NullReferenceException)
            {
                return nullCategory;
            }
            if (ex is ArgumentOutOfRangeException || ex is IndexOutOfRangeException)
            {
                return indexCategory;
            }
            return otherCategory;
        }

        public static string RecvPacketCategory(Exception ex)
        {
            return ClassifyFault(ex, RecvPacketNull, RecvPacketIndex, RecvPacketOther);
        }

        public static string RunRoundCategory(Exception ex)
        {
            return ClassifyFault(ex, RunRoundNull, RunRoundIndex, RunRoundOther);
        }

        /// <summary>socket 事件处理器抛异常（fire-and-forget 边界；以前会直接打死收包线程/进程）。
        /// 生产路径统一走 `SocketShim.InvokeHandlerGuarded → LoadMetrics.InternalError`，本入口仅保留注释说明，
        /// 不再提供可直接调用的 public 方法（避免绕过 internal_errors 的陷阱；复审 minor）。</summary>
        // SocketHandlerException 类别的唯一入口 = ScoketClient.InvokeHandlerGuarded。

        // ==== 单类别入口（生产 catch 与自检共用同一入口；删掉入口里的 Count ⇒ 自检必红）====
        public static void SocketConnectFailed() { Count(SocketConnectFault); }
        public static void MetricFlushFailed() { Count(MetricFlushFailure); }
        public static void SelfReportWriteFailed() { Count(SelfReportWriteFailure); }
        public static void DropExpWriterOpenFailed() { Count(DropExpWriterOpenFailure); }
        public static void DropExpWriterWriteFailed() { Count(DropExpWriterWriteFailure); }
        public static void BagItemDecodeFailed() { Count(BagItemDecodeFailure); }

        /// <summary>主动 Disconnect 之后仍撞到已释放的 socket（收包/发送线程），属运行期竞态，单独观测。</summary>
        public static void TeardownRaceSeen()
        {
            Count(TeardownRace);
        }

        /// <summary>
        /// socket 系统调用异常分类：`refused/reset/timeout/shutdown/not-connected` 等是连接生命周期的
        /// 正常运行期事件（另有 `conn_err_*` / `conn_lost` 计数），不当故障；其余 SocketError 才计 fault。
        /// </summary>
        public static void SocketExceptionSeen(string operation, SocketException ex)
        {
            if (ex == null)
            {
                return;
            }
            if (IsOperationalSocketError(ex.SocketErrorCode))
            {
                return;
            }
            if (operation == "recv")
            {
                Count(SocketRecvFault);
            }
            else if (operation == "send")
            {
                Count(SocketSendFault);
            }
            else
            {
                Count(SocketConnectFault);
            }
        }

        /// <summary>连接生命周期内的预期错误码（不计 fault）。</summary>
        public static bool IsOperationalSocketError(SocketError code)
        {
            switch (code)
            {
                case SocketError.ConnectionRefused:
                case SocketError.ConnectionReset:
                case SocketError.ConnectionAborted:
                case SocketError.TimedOut:
                case SocketError.Shutdown:
                case SocketError.NetworkReset:
                case SocketError.NetworkDown:
                case SocketError.HostUnreachable:
                case SocketError.HostDown:
                case SocketError.NotConnected:
                case SocketError.Disconnecting:
                case SocketError.Interrupted:
                case SocketError.OperationAborted:
                    return true;
                default:
                    return false;
            }
        }

        /// <summary>AppDomain.UnhandledException：进程级故障，先计数并尽力落一行 fatal 记录。</summary>
        public static void OnUnhandledException(Exception ex)
        {
            LoadMetrics.InternalError(UnhandledException);
            LoadMetrics.WriteFatalMarker(UnhandledException, ex);
        }

        /// <summary>TaskScheduler.UnobservedTaskException：fire-and-forget 任务的异常。</summary>
        public static void OnUnobservedTaskException(Exception ex)
        {
            LoadMetrics.InternalError(UnobservedTaskException);
            LoadMetrics.WriteFatalMarker(UnobservedTaskException, ex);
        }

        /// <summary>护栏里的日志必须“安全”：日志初始化失败/写失败不得把护栏本身打断
        /// （自检进程没有 LogService 初始化，生产里磁盘也可能出问题）。</summary>
        public static void SafeLog(Action log)
        {
            try
            {
                log();
            }
            catch
            {
                // 日志失败不改变错误计数与进程存活。
            }
        }

        /// <summary>累计快照（顺序固定；直接拼进 load_stats.ndjson 的 `error_census`）。</summary>
        public static string BuildJson()
        {
            lock (Sync)
            {
                var sb = new StringBuilder(Order.Length * 34 + 8);
                sb.Append('{');
                for (int i = 0; i < Order.Length; i++)
                {
                    if (i > 0)
                    {
                        sb.Append(',');
                    }
                    sb.Append('"').Append(Order[i]).Append("\":").Append(Counts[i]);
                }
                sb.Append('}');
                return sb.ToString();
            }
        }
    }
}
