using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json;
using OpenMir2;

namespace Mir2Tools.Capture
{
    /// <summary>
    /// 体分段布局表（C4 / A-9）—— **唯一真值在协议层**（`mir2-rs/crates/protocol/src/frame.rs`
    /// 的 `body_layout()`），本类只是消费者：读 `dump_body_layout.py` 从那份源码派生出的 JSON，
    /// 绝不在这里再抄一份表（口径分叉是本项目反复栽过的坑）。
    ///
    /// 口径与 A 线 `decode_server_payload` 一致：
    ///   s2c payload = enc(12B 头，恒 16 字符) + 体；
    ///   体按布局分三种：
    ///     Single            = 整段编码；
    ///     StructThenRest{n} = enc(n 字节) + enc(其余)；
    ///     Segmented         = **按线上字面量分隔符连接的 N 段**，每段各自编码
    ///                         （811 固定 3 段、无尾分隔；201 每件物品一段、有尾分隔、段数取头里的 series）。
    /// 明文动作帧（payload 首字符 = act 前缀）不走编码，整段即明文体。
    ///
    /// **改错段边界必红**：Segmented 的切法不靠"猜分隔符"（编码字表里也有 `/`，盲切会切错），
    /// 而是**用表给的 seg_len 逐段算编码长度**去取；任一处对不上（段长、分隔符、尾分隔、总长）
    /// 都不硬编，而是给出 `segmented(mismatch:…)` 标记并让导出器退出码非 0 ⇒ 坏表立刻可见。
    /// </summary>
    internal sealed class BodyLayoutTable
    {
        public int HeadBlock { get; private set; } = 16;
        public char ActPrefix { get; private set; } = '+';

        private sealed class Segmented
        {
            public int SegLen;
            public byte Sep;
            public bool TrailingSep;
            public bool FixedCount;
            public int Count;           // 仅 FixedCount 有时有意义
        }

        private readonly Dictionary<ushort, int> _structThenRest = new();
        private readonly Dictionary<ushort, Segmented> _segmented = new();
        /// <summary>ident → 消息名（仅用于人读的日志）。</summary>
        private readonly Dictionary<ushort, string> _names = new();

        public static BodyLayoutTable Load(string jsonPath)
        {
            if (string.IsNullOrEmpty(jsonPath))
            {
                return null;    // 未提供布局表：调用方回落到旧行为（整段解）
            }
            if (!File.Exists(jsonPath))
            {
                throw new FileNotFoundException("布局表文件不存在：" + jsonPath +
                    "（用 tools/capture/dump_body_layout.py 从协议源码派生）");
            }
            var doc = JsonDocument.Parse(File.ReadAllText(jsonPath));
            var root = doc.RootElement;
            var t = new BodyLayoutTable
            {
                HeadBlock = root.GetProperty("head_block").GetInt32(),
                ActPrefix = root.GetProperty("act_prefix").GetString()[0],
            };
            foreach (var prop in root.GetProperty("layouts").EnumerateObject())
            {
                ushort ident = ushort.Parse(prop.Name);
                var v = prop.Value;
                string kind = v.GetProperty("kind").GetString();
                switch (kind)
                {
                    case "struct_then_rest":
                        t._structThenRest[ident] = v.GetProperty("struct_len").GetInt32();
                        break;
                    case "segmented":
                        string countKind = v.GetProperty("count_kind").GetString();
                        var s = new Segmented
                        {
                            SegLen = v.GetProperty("seg_len").GetInt32(),
                            Sep = (byte)v.GetProperty("sep").GetInt32(),
                            TrailingSep = v.GetProperty("trailing_sep").GetBoolean(),
                            FixedCount = countKind == "fixed",
                        };
                        if (s.FixedCount)
                        {
                            s.Count = v.GetProperty("count").GetInt32();
                        }
                        else if (countKind != "header_series")
                        {
                            throw new InvalidDataException("布局表 count_kind 不认识：" + countKind +
                                "（ident=" + ident + "）");
                        }
                        t._segmented[ident] = s;
                        break;
                    default:
                        // 表里出现本导出器不认识的形态：**报错而不是忽略**（忽略 = 静默少一档，
                        // 导出结果看着"正常"却和真值不同，正是本项目栽过的坑）
                        throw new InvalidDataException("布局表 kind 不认识：" + kind + "（ident=" + ident +
                            "）—— 协议层新增了布局形态，先扩导出器再导出");
                }
                if (v.TryGetProperty("name", out var n))
                {
                    t._names[ident] = n.GetString();
                }
            }
            if (t._structThenRest.Count == 0 && t._segmented.Count == 0)
            {
                throw new InvalidDataException("布局表里没有任何分段条目：" + jsonPath);
            }
            return t;
        }

        public string DescribeStructEntries()
            => string.Join(",", _structThenRest.OrderBy(kv => kv.Key)
                   .Select(kv => $"{kv.Key}{(_names.TryGetValue(kv.Key, out var n) ? "(" + n + ")" : "")}={kv.Value}"));

        public string DescribeSegmentedEntries()
            => string.Join(",", _segmented.OrderBy(kv => kv.Key)
                   .Select(kv => $"{kv.Key}{(_names.TryGetValue(kv.Key, out var n) ? "(" + n + ")" : "")}=" +
                                 $"seg(seg_len={kv.Value.SegLen},sep={(char)kv.Value.Sep}," +
                                 $"trailing={kv.Value.TrailingSep}," +
                                 (kv.Value.FixedCount ? "fixed=" + kv.Value.Count : "header_series") + ")"));

        /// <summary>把线上体（已在头之外）按布局切成若干段并各自解码。
        /// <paramref name="series"/> 只有 Segmented+header_series 用得上（段数取帧头 series）。</summary>
        public static (List<byte[]> Segments, string Kind) SplitBody(
            byte[] rest, ushort ident, int series, BodyLayoutTable table)
        {
            if (table != null && table._segmented.TryGetValue(ident, out var seg))
            {
                return SplitSegmented(rest, seg, series);
            }
            if (table != null && table._structThenRest.TryGetValue(ident, out int structLen))
            {
                int firstLen = EncodedLen(structLen);
                if (rest.Length <= firstLen)
                {
                    // 短于首段：C# 侧 EncodePacket 之后没拼第二段（frame.rs 同口径）
                    return (new List<byte[]> { DecodeAll(rest) }, "struct_then_rest(short)");
                }
                var segs = new List<byte[]>
                {
                    DecodeAll(rest.AsSpan(0, firstLen).ToArray()),
                    DecodeAll(rest.AsSpan(firstLen).ToArray()),
                };
                return (segs, "struct_then_rest");
            }
            return (new List<byte[]> { DecodeAll(rest) }, "single");
        }

        /// <summary>Segmented 的切法：**按表算长度逐段取**，绝不"按分隔符盲切"
        /// （编码字表里也可能出现分隔符字节，盲切会切错且悄无声息）。
        /// 任何一处对不上 ⇒ 返回 `segmented(mismatch:…)`（导出器据此非 0 退出）。</summary>
        private static (List<byte[]> Segments, string Kind) SplitSegmented(byte[] rest, Segmented seg, int series)
        {
            int count = seg.FixedCount ? seg.Count : series;   // header_series：段数由帧头 series 给
            if (count <= 0)
            {
                return Mismatch(rest, "count=" + count);
            }
            int segEncLen = EncodedLen(seg.SegLen);
            if (segEncLen <= 0)
            {
                return Mismatch(rest, "seg_len=" + seg.SegLen);
            }
            var segs = new List<byte[]>(count);
            int pos = 0;
            for (int i = 0; i < count; i++)
            {
                if (pos + segEncLen > rest.Length)
                {
                    return Mismatch(rest, $"truncated@{i}");
                }
                var raw = rest.AsSpan(pos, segEncLen).ToArray();
                int dl = 0;
                byte[] plain;
                try
                {
                    plain = EncryptUtil.Decode(raw, raw.Length, ref dl);
                }
                catch (Exception ex)
                {
                    return Mismatch(rest, $"decode@{i}:{ex.GetType().Name}");
                }
                if (dl != seg.SegLen)
                {
                    return Mismatch(rest, $"seglen@{i}={dl}(want {seg.SegLen})");
                }
                segs.Add(plain);
                pos += segEncLen;
                bool needSep = i != count - 1 || seg.TrailingSep;   // 中间必有分隔符；尾分隔符按表
                if (needSep)
                {
                    if (pos >= rest.Length || rest[pos] != seg.Sep)
                    {
                        return Mismatch(rest, $"sep@{i}");
                    }
                    pos++;
                }
            }
            if (pos != rest.Length)
            {
                return Mismatch(rest, $"tail={rest.Length - pos}");
            }
            return (segs, "segmented");
        }

        private static (List<byte[]> Segments, string Kind) Mismatch(byte[] rest, string why)
            => (new List<byte[]> { SafeDecode(rest) }, "segmented(mismatch:" + why + ")");

        /// <summary>字节级回编佐证（--verify-roundtrip）：把切出来的段各自**再编码**、
        /// 按表给的分隔符/尾分隔符拼回去，必须逐字节等于线上原体。
        /// 这是比"解码器自证"硬的一步 —— 段边界选错一处就拼不回原字节。
        /// 非 segmented 的帧恒真（不适用）。</summary>
        public static bool VerifySegmentedRoundtrip(
            BodyLayoutTable table, ushort ident, byte[] rest, List<byte[]> segs, string kind)
        {
            if (table == null || !table._segmented.TryGetValue(ident, out var s))
            {
                return true;
            }
            if (!kind.StartsWith("segmented", StringComparison.Ordinal) || kind.Contains("mismatch"))
            {
                return false;   // 切都切不对，谈不上回编
            }
            var ms = new MemoryStream();
            for (int i = 0; i < segs.Count; i++)
            {
                var dst = new byte[segs[i].Length * 2 + 16];
                int n = EncryptUtil.Encode(segs[i], segs[i].Length, dst);
                ms.Write(dst, 0, n);
                if (i != segs.Count - 1 || s.TrailingSep)
                {
                    ms.WriteByte(s.Sep);
                }
            }
            var got = ms.ToArray();
            return got.Length == rest.Length && got.AsSpan().SequenceEqual(rest);
        }

        /// <summary>n 字节明文编码后的字符数（用仓库自带 oracle 实测，不另写公式）。</summary>
        private static int EncodedLen(int n)
        {
            if (n <= 0)
            {
                return 0;
            }
            var src = new byte[n];
            var dst = new byte[n * 2 + 16];
            return EncryptUtil.Encode(src, n, dst);
        }

        private static byte[] DecodeAll(byte[] buf)
        {
            int len = 0;
            return EncryptUtil.Decode(buf, buf.Length, ref len);
        }

        private static byte[] SafeDecode(byte[] buf)
        {
            try
            {
                return DecodeAll(buf);
            }
            catch
            {
                return buf;     // 判为 mismatch 的帧不因解码异常丢掉整行
            }
        }
    }
}
