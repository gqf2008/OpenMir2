using System;
using System.Globalization;
using System.IO;
using System.Text;
using OpenMir2;

namespace BotSrv
{
    /// <summary>
    /// 卡 `openmir2-sm-logon-cross-session-delivery` 的**假人自报**（总监归因判据 step1）。
    ///
    /// 每个假人把「自己拿到的身份」按行写入 `MIR2_BOT_SELFREPORT` 指向的 NDJSON 文件：
    /// - 连接建立：chr / login_id / bot_session(GUID) / conn_seq / local_ep / remote_ep；
    /// - 收到 SM_LOGON：上述身份 + actor(Recog) / x(Param) / y(Tag)。
    ///
    /// 客户端线上协议里**看不到**网关分配的 SessionId，故本自报以
    /// 「本端 TCP 端点 + 连接序号 + 自己收到的 actor」作为自报身份；本端端点与网关
    /// `MISDELIVER_ACCEPT peer=...` 的 peer 一一对应，可与服务端 `socket`/`session_id` 对齐。
    ///
    /// 未设环境变量 ⇒ 完全空转（默认关，不影响压测）。写失败只在本进程内停写，不抛异常。
    /// </summary>
    public static class SelfReport
    {
        private static readonly object Sync = new object();
        private static readonly string Path =
            Environment.GetEnvironmentVariable("MIR2_BOT_SELFREPORT") ?? string.Empty;
        private static bool _broken;

        public static bool Enabled => Path.Length > 0;

        /// <summary>追加一行：<c>{"ts_ms":…,"boot_ms":…,"event":"…",&lt;fields&gt;}</c>。</summary>
        public static void Write(string ev, string jsonFields)
        {
            if (Path.Length == 0 || _broken)
            {
                return;
            }
            long ts = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
            long boot = HUtil32.GetTickCount();
            string line = "{\"ts_ms\":" + ts.ToString(CultureInfo.InvariantCulture)
                + ",\"boot_ms\":" + boot.ToString(CultureInfo.InvariantCulture)
                + ",\"event\":\"" + ev + "\""
                + (string.IsNullOrEmpty(jsonFields) ? string.Empty : "," + jsonFields)
                + "}";
            try
            {
                lock (Sync)
                {
                    File.AppendAllText(Path, line + "\n", new UTF8Encoding(false));
                }
            }
            catch
            {
                // 写失败 = 证据链断（本进程内停写）。必须可见：以前只置 _broken，报告里看不出来。
                BotErrorCensus.SelfReportWriteFailed();
                _broken = true;
            }
        }

        /// <summary>JSON 字符串值（最小转义；ChrName/端点都是 ASCII 或中文，够了）。</summary>
        public static string Q(string s)
        {
            if (string.IsNullOrEmpty(s))
            {
                return "\"\"";
            }
            var sb = new StringBuilder(s.Length + 2);
            sb.Append('"');
            foreach (char c in s)
            {
                switch (c)
                {
                    case '"': sb.Append("\\\""); break;
                    case '\\': sb.Append("\\\\"); break;
                    case '\n': sb.Append("\\n"); break;
                    case '\r': sb.Append("\\r"); break;
                    case '\t': sb.Append("\\t"); break;
                    default: sb.Append(c); break;
                }
            }
            sb.Append('"');
            return sb.ToString();
        }
    }
}
