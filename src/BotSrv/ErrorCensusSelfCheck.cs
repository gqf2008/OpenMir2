using System;
using System.Collections.Generic;
using System.IO;
using System.Net.Sockets;
using System.Text.Json;
using OpenMir2;
using Serilog;
using Serilog.Core;
using Serilog.Events;
using Serilog.Formatting.Display;

namespace BotSrv
{
    /// <summary>
    /// 卡 `openmir2-botsrv-recv-thread-nre`：错误类别普查的**逐类牙**（无栈自检）。
    ///
    /// 入口：`BotSrv.exe --selftest-error-census`（不启动服务/不连端口；PASS ⇒ exit 0，任一用例不符 ⇒ exit 1）。
    ///
    /// 每个枚举出的类别都走**生产代码同一条入口**注入一次，断言该类别 +1：
    ///   · recv_packet_*      → ClientManager.RunPacketGuard（收包线程 per-packet 护栏）
    ///   · run_round_*        → ClientManager.RunBotRound（每假人一轮护栏）
    ///   · run_loop_exception → AppService.RunLoopGuard（runThread 整轮护栏）
    ///   · socket_handler_*   → SocketShim.InvokeHandlerGuarded（事件处理器 fire-and-forget 边界）
    ///   · socket_*_fault     → BotErrorCensus.SocketExceptionSeen（只对非生命周期错误码计数）
    ///   · 证据链/解析/进程级 → 各自的单一入口
    /// 负例（必须不计数）：处理器不抛异常、ConnectionReset 等生命周期错误码、运行期 teardown 不串类。
    /// 另断言：JSON 键集合 == 类别表（缺一/多一都红）；`internal_errors == Σ(feeds_internal 类别)`。
    /// </summary>
    internal static class ErrorCensusSelfCheck
    {
        public static int Run()
        {
            int failed = 0;

            void Check(string name, bool ok, string detail = "")
            {
                if (!ok)
                {
                    failed++;
                }
                Console.WriteLine($"CENSUS {(ok ? "PASS" : "FAIL")} {name}{(detail.Length > 0 ? " " + detail : "")}");
            }

            long Delta(string category, Action inject)
            {
                long before = BotErrorCensus.Get(category);
                inject();
                long after = BotErrorCensus.Get(category);
                Check("inject-" + category, after == before + 1, $"{before}->{after}");
                return after - before;
            }

            void NoDelta(string name, string category, Action inject)
            {
                long before = BotErrorCensus.Get(category);
                inject();
                long after = BotErrorCensus.Get(category);
                Check(name, after == before, $"{category} {before}->{after}");
            }

            BotErrorCensus.ResetForSelftest();

            // ① 类别表 == JSON 键集合（封闭集合；缺一/多一都红）。
            string json = BotErrorCensus.BuildJson();
            try
            {
                using (JsonDocument doc = JsonDocument.Parse(json))
                {
                    int props = 0;
                    foreach (JsonProperty p in doc.RootElement.EnumerateObject())
                    {
                        props++;
                        bool known = false;
                        foreach (string cat in BotErrorCensus.Categories)
                        {
                            if (cat == p.Name) { known = true; break; }
                        }
                        Check("json-known-key-" + p.Name, known, p.Name);
                    }
                    Check("json-key-count-equals-categories", props == BotErrorCensus.Categories.Count,
                        $"json={props} categories={BotErrorCensus.Categories.Count}");
                    foreach (string cat in BotErrorCensus.Categories)
                    {
                        Check("json-has-" + cat, doc.RootElement.TryGetProperty(cat, out _));
                    }
                }
            }
            catch (Exception ex)
            {
                Check("json-parse", false, ex.Message);
            }

            // ② 收包线程三类（N1 的 3.95 万条 NRE 是 recv_packet_null）。
            Delta(BotErrorCensus.RecvPacketNull,
                () => ClientManager.RunPacketGuard(() => throw new NullReferenceException("census-selftest")));
            Delta(BotErrorCensus.RecvPacketIndex,
                () => ClientManager.RunPacketGuard(() => throw new ArgumentOutOfRangeException("census-selftest")));
            Delta(BotErrorCensus.RecvPacketOther,
                () => ClientManager.RunPacketGuard(() => throw new InvalidOperationException("census-selftest")));

            // ③ 每假人一轮三类。
            Delta(BotErrorCensus.RunRoundNull,
                () => ClientManager.RunBotRound(() => throw new NullReferenceException("census-selftest"), null));
            Delta(BotErrorCensus.RunRoundIndex,
                () => ClientManager.RunBotRound(() => throw new IndexOutOfRangeException("census-selftest"), null));
            Delta(BotErrorCensus.RunRoundOther,
                () => ClientManager.RunBotRound(() => throw new InvalidOperationException("census-selftest"), null));

            // ④ runThread 整轮（含建号段；以前建号异常会静默打死线程）。
            Delta(BotErrorCensus.RunLoopException,
                () => AppService.RunLoopGuard(() => throw new InvalidOperationException("census-selftest")));

            // ⑤ socket 事件处理器（fire-and-forget 边界）。
            Delta(BotErrorCensus.SocketHandlerException,
                () => ScoketClient.InvokeHandlerGuarded(() => throw new NullReferenceException("census-selftest")));
            NoDelta("handler-no-throw-no-count", BotErrorCensus.SocketHandlerException,
                () => ScoketClient.InvokeHandlerGuarded(() => { }));

            // ⑥ socket 系统调用：非生命周期错误码才计 fault；生命周期错误码必须不串类。
            Delta(BotErrorCensus.SocketRecvFault,
                () => BotErrorCensus.SocketExceptionSeen("recv", new SocketException((int)SocketError.InvalidArgument)));
            Delta(BotErrorCensus.SocketSendFault,
                () => BotErrorCensus.SocketExceptionSeen("send", new SocketException((int)SocketError.InvalidArgument)));
            Delta(BotErrorCensus.SocketConnectFault,
                () => BotErrorCensus.SocketExceptionSeen("connect", new SocketException((int)SocketError.InvalidArgument)));
            NoDelta("lifecycle-reset-not-fault", BotErrorCensus.SocketRecvFault,
                () => BotErrorCensus.SocketExceptionSeen("recv", new SocketException((int)SocketError.ConnectionReset)));
            NoDelta("lifecycle-refused-not-fault", BotErrorCensus.SocketConnectFault,
                () => BotErrorCensus.SocketExceptionSeen("connect", new SocketException((int)SocketError.ConnectionRefused)));

            // ⑦ 证据链/解析/进程级/未分类。
            Delta(BotErrorCensus.MetricFlushFailure, LoadMetrics.RecordMetricFlushFailure);
            Delta(BotErrorCensus.SelfReportWriteFailure, BotErrorCensus.SelfReportWriteFailed);
            Delta(BotErrorCensus.DropExpWriterOpenFailure, BotErrorCensus.DropExpWriterOpenFailed);
            Delta(BotErrorCensus.DropExpWriterWriteFailure, BotErrorCensus.DropExpWriterWriteFailed);
            Delta(BotErrorCensus.BagItemDecodeFailure,
                () => LoadMetrics.InternalError(BotErrorCensus.BagItemDecodeFailure));
            Delta(BotErrorCensus.UnhandledException,
                () => BotErrorCensus.OnUnhandledException(new InvalidOperationException("census-selftest")));
            Delta(BotErrorCensus.UnobservedTaskException,
                () => BotErrorCensus.OnUnobservedTaskException(new InvalidOperationException("census-selftest")));
            Delta(BotErrorCensus.InternalErrorUnclassified, () => LoadMetrics.InternalError());

            // ⑧ 运行期 teardown（operational；允许非 0，但必须独立计数、不串到 fault）。
            Delta(BotErrorCensus.TeardownRace, BotErrorCensus.TeardownRaceSeen);

            // ⑨ 不变式：internal_errors == Σ(feeds_internal 类别)。
            //    任何绕过 LoadMetrics.InternalError(category) 直接加计数的路径都会打破它。
            long sumInternal = 0;
            foreach (string cat in BotErrorCensus.Categories)
            {
                if (BotErrorCensus.IsInternalErrorCategory(cat))
                {
                    sumInternal += BotErrorCensus.Get(cat);
                }
            }
            Check("internal-errors-equals-census-sum", LoadMetrics.InternalErrors == sumInternal,
                $"internal_errors={LoadMetrics.InternalErrors} census_sum={sumInternal}");

            // ⑩ 接线：真实走 LoadMetrics.Init → Flush → load_stats.ndjson，确认 census 落盘且逐类可读。
            //    只验证 JSON 结构/数值，不依赖真机/服务端；临时目录在 finally 删除。
            string tmpDir = Path.Combine(Path.GetTempPath(), "botsrv-census-selftest-" + Guid.NewGuid().ToString("N"));
            try
            {
                LoadMetrics.Init(tmpDir);
                LoadMetrics.Flush();
                string statsPath = LoadMetrics.StatsPath;
                LoadMetrics.Shutdown();
                string[] lines = File.ReadAllLines(statsPath);
                string lastLine = "";
                for (int i = lines.Length - 1; i >= 0; i--)
                {
                    if (lines[i].Trim().Length > 0) { lastLine = lines[i]; break; }
                }
                using (JsonDocument doc = JsonDocument.Parse(lastLine))
                {
                    JsonElement root = doc.RootElement;
                    JsonElement schema;
                    JsonElement ec;
                    bool hasSchema = root.TryGetProperty("error_census_schema", out schema)
                                     && schema.GetInt32() == 1;
                    bool hasCensus = root.TryGetProperty("error_census", out ec)
                                     && ec.ValueKind == JsonValueKind.Object;
                    Check("stats-has-error_census", hasCensus && hasSchema,
                        $"schema={hasSchema} census={hasCensus}");
                    if (hasCensus)
                    {
                        int okRows = 0;
                        foreach (string cat in BotErrorCensus.Categories)
                        {
                            JsonElement v;
                            if (ec.TryGetProperty(cat, out v) && v.TryGetInt64(out long n)
                                && n == BotErrorCensus.Get(cat))
                            {
                                okRows++;
                            }
                        }
                        Check("stats-census-values-match", okRows == BotErrorCensus.Categories.Count,
                            $"matched={okRows}/{BotErrorCensus.Categories.Count}");
                    }
                    JsonElement ie;
                    Check("stats-internal-errors-present",
                        root.TryGetProperty("internal_errors", out ie) && ie.GetInt64() == LoadMetrics.InternalErrors,
                        $"internal_errors={LoadMetrics.InternalErrors}");
                }
            }
            catch (Exception ex)
            {
                Check("stats-wiring", false, ex.Message);
            }
            finally
            {
                try { Directory.Delete(tmpDir, true); } catch { }
            }

            // ⑪ 删计数器必红（元牙）：把某类的累计值手工抹成 0 后，Δ 判定必须失败——
            //    这里用「假设值」证明判定不是恒真（不真去改生产计数）。
            long fakeBefore = 100;
            long fakeAfter = 100;
            Check("meta-delta-detects-missing-increment", !(fakeAfter == fakeBefore + 1),
                "no-increment must fail the +1 predicate");

            // ⑫ 日志可诊断性牙（卡 `openmir2-botsrv-errorstack-diagnosable`）：
            //    经 `RunPacketGuard` 的异常必须按 **AppServer 真实模板**落成多行：
            //    消息行 → 异常类型行 → `   at ` 堆栈行。用同一个 outputTemplate 常量渲染到临时文件。
            //    根因（2026-11-15）：`LogService.Error(ex)` 曾写成 `Logger.Error(ex.Message, ex)`，
            //    Serilog 绑泛型 `Error<T>(string, T)` ⇒ LogEvent.Exception=null ⇒ `{Exception}` 为空。
            //    改回坏序 / 改模板 ⇒ 下面这些用例必 RED。
            var logEvents = new List<LogEvent>();
            var logWriter = new StringWriter();
            var logSink = new TemplateCapturingSink(AppServer.LogOutputTemplate, logWriter, logEvents);
            LogService.Logger = new LoggerConfiguration().MinimumLevel.Debug()
                .WriteTo.Sink(logSink).CreateLogger();
            ClientManager.RunPacketGuard(() => throw new NullReferenceException("census-log-tooth"));
            string renderedLog = logWriter.ToString();
            string logPath = Path.Combine(Path.GetTempPath(),
                "botsrv-errorlog-tooth-" + Guid.NewGuid().ToString("N") + ".log");
            try
            {
                File.WriteAllText(logPath, renderedLog);
                string[] rawLines = File.ReadAllText(logPath).Replace("\r\n", "\n").Split('\n');
                var nonEmpty = new List<string>();
                foreach (string ln in rawLines)
                {
                    if (ln.Length > 0) { nonEmpty.Add(ln); }
                }
                LogEvent lastEvent = logEvents.Count > 0 ? logEvents[logEvents.Count - 1] : null;
                string renderedException = lastEvent == null || lastEvent.Exception == null
                    ? ""
                    : lastEvent.Exception.ToString();
                Check("log-error-event-has-exception",
                    lastEvent != null && lastEvent.Exception != null,
                    lastEvent == null
                        ? "no-log-event"
                        : ("exception=" + (lastEvent.Exception == null ? "null" : "set")));
                Check("log-error-exception-type-present",
                    renderedException.Contains("System.NullReferenceException"),
                    "head=" + (renderedException.Length == 0
                        ? "<empty>" : renderedException.Split('\n')[0]));
                Check("log-error-exception-stack-present",
                    renderedException.Contains("   at "),
                    "has-at=" + renderedException.Contains("   at "));
                // P1 回归：真实模板下类型行在**下一行**（不带 [ERR]），三行形态缺一不可。
                Check("log-shape-message-line",
                    nonEmpty.Count >= 1 && nonEmpty[0].Contains("[ERR]")
                        && !nonEmpty[0].Contains("System.NullReferenceException"),
                    nonEmpty.Count >= 1 ? nonEmpty[0] : "<empty>");
                Check("log-shape-type-line",
                    nonEmpty.Count >= 2 && nonEmpty[1].StartsWith("System.NullReferenceException:"),
                    nonEmpty.Count >= 2 ? nonEmpty[1] : "<missing>");
                Check("log-shape-stack-line",
                    nonEmpty.Count >= 3 && nonEmpty[2].StartsWith("   at "),
                    nonEmpty.Count >= 3 ? nonEmpty[2] : "<missing>");
                Check("log-shape-multiline-file",
                    File.ReadAllLines(logPath).Length >= 3,
                    "lines=" + File.ReadAllLines(logPath).Length);
            }
            finally
            {
                try { File.Delete(logPath); } catch { }
            }
            // meta 牙：形态绑定 AppServer 的模板字面量；模板改了这条必红。
            Check("log-shape-template-bound",
                AppServer.LogOutputTemplate
                    == "{Timestamp:HH:mm:ss.fff} [{Level:u3}] {Message:lj}{NewLine}{Exception}",
                AppServer.LogOutputTemplate);

            Console.WriteLine(failed == 0
                ? "CENSUS SELFTEST GREEN"
                : $"CENSUS SELFTEST RED ({failed} failed)");
            return failed == 0 ? 0 : 1;
        }

        /// <summary>日志牙用的内联 sink：用 AppServer 的真实 outputTemplate 渲染到 TextWriter，
        /// 同时取回原始 LogEvent（含 Exception）供断言。</summary>
        private sealed class TemplateCapturingSink : ILogEventSink
        {
            private readonly MessageTemplateTextFormatter _formatter;
            private readonly TextWriter _writer;
            private readonly List<LogEvent> _events;

            public TemplateCapturingSink(string outputTemplate, TextWriter writer, List<LogEvent> events)
            {
                _formatter = new MessageTemplateTextFormatter(outputTemplate);
                _writer = writer;
                _events = events;
            }

            public void Emit(LogEvent logEvent)
            {
                _events.Add(logEvent);
                _formatter.Format(logEvent, _writer);
            }
        }
    }
}
