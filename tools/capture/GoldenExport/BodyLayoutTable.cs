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
    ///   体按布局分：Single = 整段编码；StructThenRest{n} = enc(n 字节) + enc(其余)。
    /// 明文动作帧（payload 首字符 = act 前缀）不走编码，整段即明文体。
    /// </summary>
    internal sealed class BodyLayoutTable
    {
        public int HeadBlock { get; private set; } = 16;
        public char ActPrefix { get; private set; } = '+';

        /// <summary>ident → struct_len（StructThenRest 家族）。</summary>
        private readonly Dictionary<ushort, int> _structThenRest = new();
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
                if (v.GetProperty("kind").GetString() == "struct_then_rest")
                {
                    t._structThenRest[ident] = v.GetProperty("struct_len").GetInt32();
                    if (v.TryGetProperty("name", out var n))
                    {
                        t._names[ident] = n.GetString();
                    }
                }
            }
            if (t._structThenRest.Count == 0)
            {
                throw new InvalidDataException("布局表里没有任何 struct_then_rest 条目：" + jsonPath);
            }
            return t;
        }

        public string DescribeStructEntries()
            => string.Join(",", _structThenRest.OrderBy(kv => kv.Key)
                   .Select(kv => $"{kv.Key}{( _names.TryGetValue(kv.Key, out var n) ? "(" + n + ")" : "")}={kv.Value}"));

        /// <summary>把线上体（已在头之外）按布局切成若干段并各自解码。</summary>
        public static (List<byte[]> Segments, string Kind) SplitBody(byte[] rest, ushort ident, BodyLayoutTable table)
        {
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
    }
}
