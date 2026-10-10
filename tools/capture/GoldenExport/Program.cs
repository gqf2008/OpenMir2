using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using OpenMir2;

namespace Mir2Tools.Capture
{
    /// <summary>
    /// 把 mir2_proxy 的会话 dump 导出为 tests/golden 契约 JSONL（每行一帧）。
    /// 帧界（实测线上字节 + C# 网关源码核对）：
    ///   c2s: '#' + 编码块 + '!'
    ///   s2c: '#' + 编码块 + '!' + '$'
    ///   裸 '*' (0x2A) 为心跳，记 kind=heartbeat。
    /// 帧头 12B（CommandMessage）：Recog i32le / Ident/Param/Tag/Series u16le，
    /// 其余为明文 body（GB2312 原始字节，直接 hash，不转码）。
    /// </summary>
    internal static class Program
    {
        private sealed class ChunkEvent
        {
            public int Conn;
            public string Dir = "";
            public int Port;
            public long Off;
            public int N;
            public long TsMs;
        }

        private sealed class Frame
        {
            public int Conn;
            public int Port;
            public string Dir = "";
            public long TsMs;
            public long Off;
            public byte[] Raw = Array.Empty<byte>();   // 含定界符的完整线上帧
            public bool Heartbeat;
        }

        private static int Main(string[] args)
        {
            string session = null;
            string outFile = null;
            bool dumpPlain = false;
            for (int i = 0; i < args.Length; i++)
            {
                if (args[i] == "--session" && i + 1 < args.Length) session = args[++i];
                else if (args[i] == "--out" && i + 1 < args.Length) outFile = args[++i];
                else if (args[i] == "--dump-plain") dumpPlain = true;   // 调试：打印每帧明文（latin1 逐字节）
            }
            if (session == null || outFile == null)
            {
                Console.Error.WriteLine("usage: GoldenExport --session <proxy dump dir> --out <capture.jsonl> [--dump-plain]");
                return 2;
            }
            Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);   // GB2312

            var chunksPath = Path.Combine(session, "chunks.ndjson");
            var events = new List<ChunkEvent>();
            var connPort = new Dictionary<int, int>();
            foreach (var line in File.ReadLines(chunksPath))
            {
                using var doc = JsonDocument.Parse(line);
                var root = doc.RootElement;
                var kind = root.GetProperty("event").GetString();
                if (kind == "open")
                {
                    connPort[root.GetProperty("conn").GetInt32()] = root.GetProperty("port").GetInt32();
                }
                else if (kind == "data")
                {
                    events.Add(new ChunkEvent
                    {
                        Conn = root.GetProperty("conn").GetInt32(),
                        Dir = root.GetProperty("dir").GetString(),
                        Port = root.GetProperty("port").GetInt32(),
                        Off = root.GetProperty("off").GetInt64(),
                        N = root.GetProperty("n").GetInt32(),
                        TsMs = root.GetProperty("t_wall_ms").GetInt64(),
                    });
                }
            }

            var frames = new List<Frame>();
            foreach (var binPath in Directory.GetFiles(session, "conn-*.bin"))
            {
                var name = Path.GetFileNameWithoutExtension(binPath); // conn-0001.c2s
                var parts = name.Split('.');
                int conn = int.Parse(parts[0].Substring(5));
                string dir = parts[1];
                byte[] stream = File.ReadAllBytes(binPath);
                var connEvents = events.Where(e => e.Conn == conn && e.Dir == dir)
                                       .OrderBy(e => e.Off).ToList();
                foreach (var f in Segment(stream))
                {
                    frames.Add(new Frame
                    {
                        Conn = conn,
                        Dir = dir,
                        Port = connPort.TryGetValue(conn, out var p) ? p : 0,
                        Off = f.off,
                        Raw = f.raw,
                        Heartbeat = f.heartbeat,
                        TsMs = TsOfOffset(connEvents, f.off),
                    });
                }
            }

            // 全局 seq：按时间戳（同帧按连接/偏移稳定排序）
            frames = frames.OrderBy(f => f.TsMs).ThenBy(f => f.Conn).ThenBy(f => f.Off).ToList();

            int decoded = 0, decodeErrors = 0, heartbeats = 0;
            using (var w = new StreamWriter(outFile, false, new UTF8Encoding(false)))
            {
                w.NewLine = "\n";   // 契约 JSONL 一律 LF（CRLF 会让入库后 blob 与工作区 sha256 不一致）
                int seq = 0;
                foreach (var f in frames)
                {
                    seq++;
                    if (f.Heartbeat)
                    {
                        w.WriteLine($"{{\"kind\":\"heartbeat\",\"dir\":\"{f.Dir}\",\"seq\":{seq},\"ts_ms\":{f.TsMs}}}");
                        heartbeats++;
                        continue;
                    }
                    var row = new Dictionary<string, object>
                    {
                        ["kind"] = "frame",
                        ["dir"] = f.Dir,
                        ["seq"] = seq,
                        ["ts_ms"] = f.TsMs,
                        ["port"] = f.Port,
                        ["hop"] = HopOfPort(f.Port),
                        ["conn"] = f.Conn,
                        ["raw_hex"] = ToHex(f.Raw),
                    };
                    // 帧尾独立期望（A 线 replay 靠它把「改帧尾」变成可判伪）：
                    // 各跳默认尾 c2s='!'、s2c 见 ServerFrameTail：login 跳经 LoginSrv 中继为 '!$'，
                    // sel/game 跳为 '!'；仅当实测尾与默认不符时才写 "tail" 显式覆盖
                    // （例：login 跳上由 LoginGate 自身产生的帧是 '!'）。
                    string actualTail = TailOf(f.Raw);
                    string defaultTail = DefaultTail(HopOfPort(f.Port), f.Dir);
                    if (actualTail != defaultTail)
                    {
                        row["tail"] = actualTail;
                    }
                    // 剥定界符：c2s 帧头是字面量 "#1"（C# 网关收包统一跳 2 字节，
                    // 见 LoginGate/ClientSession.cs `Body[2..^1]`、GameGate 同），
                    // s2c 帧头是 "#"；尾部剥 '!'/“!$”。
                    int start = 0;
                    if (f.Dir == "c2s" && f.Raw.Length > 1 && f.Raw[0] == (byte)'#' && f.Raw[1] == (byte)'1') start = 2;
                    else if (f.Raw.Length > 0 && f.Raw[0] == (byte)'#') start = 1;
                    int end = f.Raw.Length;
                    while (end > start && (f.Raw[end - 1] == (byte)'!' || f.Raw[end - 1] == (byte)'$')) end--;
                    byte[] blob = f.Raw[start..end];
                    try
                    {
                        int decLen = 0;
                        byte[] plain = EncryptUtil.Decode(blob, blob.Length, ref decLen);
                        if (dumpPlain)
                        {
                            Console.WriteLine($"  seq={seq} {f.Dir} blob={Encoding.ASCII.GetString(blob)} plain({decLen})={BitConverter.ToString(plain, 0, decLen)}");
                            Console.WriteLine($"         text={Encoding.GetEncoding("GB2312").GetString(plain, 0, decLen)}");
                        }
                        // 纯字符串帧：进 GameGate 的首个上行包是 "**账号/角色/..." 登录串
                        // （C# 侧走 ClientLogin → DeCodeString，不当 12B 头解）
                        bool isStringFrame = decLen >= 2 && plain[0] == (byte)'*' && plain[1] == (byte)'*';
                        if (isStringFrame)
                        {
                            row["string_frame"] = true;
                            row["body_len"] = decLen;
                            row["body_sha256"] = Convert.ToHexString(SHA256.HashData(plain.AsSpan(0, decLen))).ToLowerInvariant();
                            decoded++;
                        }
                        else if (decLen >= 12)
                        {
                            row["recog"] = BitConverter.ToInt32(plain, 0);
                            row["ident"] = BitConverter.ToUInt16(plain, 4);
                            row["param"] = BitConverter.ToUInt16(plain, 6);
                            row["tag"] = BitConverter.ToUInt16(plain, 8);
                            row["series"] = BitConverter.ToUInt16(plain, 10);
                            int bodyLen = decLen - 12;
                            row["body_len"] = bodyLen;
                            row["body_sha256"] = Convert.ToHexString(SHA256.HashData(plain.AsSpan(12, bodyLen))).ToLowerInvariant();
                            decoded++;
                        }
                        else
                        {
                            // 短帧（如纯应答）：没有 12B 头，明文整体当 body
                            row["body_len"] = decLen;
                            row["body_sha256"] = Convert.ToHexString(SHA256.HashData(plain.AsSpan(0, decLen))).ToLowerInvariant();
                            row["short_frame"] = true;
                            decoded++;
                        }
                    }
                    catch (Exception ex)
                    {
                        row["error"] = "decode";
                        row["error_detail"] = ex.GetType().Name;
                        decodeErrors++;
                    }
                    w.WriteLine(JsonSerializer.Serialize(row));
                }
            }

            Console.WriteLine($"FRAMES={frames.Count} decoded={decoded} heartbeat={heartbeats} decode_error={decodeErrors}");
            Console.WriteLine($"OUT={outFile}");
            return decodeErrors > 0 ? 3 : 0;   // 有解不开的帧必须可见，不许静默
        }

        /// <summary>代理监听端口 → 跳名（A 线 replay 按 hop 给帧尾独立期望）。</summary>
        private static string HopOfPort(int port) => port switch
        {
            7000 => "login",
            7100 => "sel",
            7200 => "game",
            _ => "unknown",
        };

        /// <summary>帧尾实测值："!$" / "!"（取末尾 '$' 或 '!'）。</summary>
        private static string TailOf(byte[] raw)
        {
            if (raw.Length >= 2 && raw[^1] == (byte)'$' && raw[^2] == (byte)'!') return "!$";
            if (raw.Length >= 1 && raw[^1] == (byte)'!') return "!";
            return "";
        }

        /// <summary>各跳默认帧尾：c2s 一律 '!'；s2c 仅 login 跳经 LoginSrv 中继为 '!$'。</summary>
        private static string DefaultTail(string hop, string dir)
            => (dir == "s2c" && hop == "login") ? "!$" : "!";

        /// <summary>按帧界切流：'#' 开头、'!' 收尾（s2c 附带 '$'），裸 '*' 为心跳。</summary>
        private static IEnumerable<(long off, byte[] raw, bool heartbeat)> Segment(byte[] s)
        {
            long i = 0;
            while (i < s.Length)
            {
                if (s[i] == (byte)'*')
                {
                    yield return (i, new[] { s[i] }, true);
                    i++;
                    continue;
                }
                long startOff = i;
                // 找 '!' 收尾
                while (i < s.Length && s[i] != (byte)'!') i++;
                if (i < s.Length) i++;                       // 含 '!'
                if (i < s.Length && s[i] == (byte)'$') i++;  // s2c 尾 '$'
                yield return (startOff, s[(int)startOff..(int)i], false);
            }
        }

        private static long TsOfOffset(List<ChunkEvent> events, long off)
        {
            // 帧起始字节落在哪个 data 块里，就用哪个块的墙钟
            foreach (var e in events)
            {
                if (off >= e.Off && off < e.Off + e.N) return e.TsMs;
            }
            return events.Count > 0 ? events[^1].TsMs : 0;
        }

        private static string ToHex(byte[] b) => Convert.ToHexString(b).ToLowerInvariant();
    }
}
