// oracle-vectors —— 用 C# 真身（EDCode/EncryptUtil/SerializerUtil）生成对拍向量。
//
// 产物：mir2-rs/tests/parity/vectors/oracle_vectors.jsonl
//   {"kind":"edcode","plain_hex":"..","encoded_hex":".."}
//   {"kind":"frame","dir":"c2s"|"s2c","seq":N,"raw_hex":"..","ident":N,"recog":N,"param":N,
//    "tag":N,"series":N,"body_len":N,"body_sha256":".."}
//   {"kind":"server-header","seq":N,"raw_hex":"..","packet_code":N,"socket":N,"session_id":N,
//    "ident":N,"session_index":N,"pack_length":N}
//
// 这是 M0 验收①②的静态判据来源（金标准抓包到位前的 oracle 层；抓包到位后 replay 直接吃抓包）。

using System;
using System.Collections.Generic;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using OpenMir2;
using OpenMir2.Packets.ClientPackets;
using OpenMir2.Packets.ServerPackets;

namespace OracleVectors;

public static class Program
{
    private static readonly Encoding Gb2312 = Encoding.GetEncoding("gb2312");

    private static string Hex(byte[] data) => Convert.ToHexString(data).ToLowerInvariant();

    private static string Sha256Hex(byte[] data) => Hex(SHA256.HashData(data));

    public static int Main(string[] args)
    {
        // gb2312 需要 CodePages provider（与服务端进程启动时的注册一致）
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        string outPath = args.Length > 0
            ? args[0]
            : Path.Combine(AppContext.BaseDirectory, "..", "..", "..", "..", "vectors", "oracle_vectors.jsonl");
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outPath))!);

        var lines = new List<string>();

        // ---- 0. 前置断言：MemoryPack 对 CommandMessage 的序列化必须恒为 12 字节原始布局 ----
        byte[] probe = SerializerUtil.Serialize(new CommandMessage { Recog = 0x11223344, Ident = 3011, Param = 10, Tag = 20, Series = 3 });
        if (probe.Length != CommandMessage.Size)
        {
            Console.Error.WriteLine($"FATAL: CommandMessage 序列化长度 {probe.Length} != 12（MemoryPack 布局假设破裂）");
            return 2;
        }
        // 布局断言：Recog 在 offset 0（小端）
        if (probe[0] != 0x44 || probe[1] != 0x33 || probe[2] != 0x22 || probe[3] != 0x11)
        {
            Console.Error.WriteLine("FATAL: CommandMessage 字段布局不是 Recog@0 小端");
            return 2;
        }

        // ---- 1. EDCode 纯编解码向量 ----
        // 1a. 全长度扫：0..=64 的确定性伪随机内容
        var rng = new Random(20261010);
        for (int len = 0; len <= 64; len++)
        {
            byte[] plain = new byte[len];
            rng.NextBytes(plain);
            AddEdcode(lines, plain);
        }
        // 1b. 全字节值 0..255（打满 XOR/分组的所有角落）
        byte[] allBytes = new byte[256];
        for (int i = 0; i < 256; i++) allBytes[i] = (byte)i;
        AddEdcode(lines, allBytes);
        // 1c. GB2312 中文样本（线上文本的真实形态）
        foreach (string s in new[] { "热血传奇", "登录成功，欢迎来到游戏。", "比奇省", " mir2test/mir2pass ", "!@#$%^&*()_+", "" })
        {
            AddEdcode(lines, Gb2312.GetBytes(s));
        }
        // 1d. 1000 条随机长度随机内容
        for (int i = 0; i < 1000; i++)
        {
            byte[] plain = new byte[rng.Next(0, 300)];
            rng.NextBytes(plain);
            AddEdcode(lines, plain);
        }

        // ---- 2. 帧向量（走真实公开路径 EDCode.EncodeMessage / EncodeString）----
        int seq = 0;
        // 覆盖八阶段的典型包号 + 边界值
        (int ident, int recog, int param, int tag, int series, string bodyGbk)[] frameCases =
        {
            // 登录
            (Messages.CM_IDPASSWORD, 0, 0, 0, 0, "mir2test/mir2pass"),
            (Messages.CM_PROTOCOL, 0, 0, 0, 0, ""),
            (Messages.SM_PASSOK_SELECTSERVER, 0, 0, 0, 0, "1/热血传奇/127.0.0.1/7100"),
            // 选服
            (Messages.CM_SELECTSERVER, 0, 0, 0, 0, "热血传奇"),
            (Messages.SM_SELECTSERVER_OK, 0, 0, 0, 0, "127.0.0.1/7100"),
            // 选角
            (Messages.CM_QUERYCHR, 0, 0, 0, 0, "mir2test"),
            (Messages.SM_QUERYCHR, 0, 2, 0, 0, "aaa/1/0/0/30/bbb/1/1/0/25"),
            // 建角
            (Messages.CM_NEWCHR, 0, 0, 0, 1, "ccc/0/1"),
            (Messages.SM_NEWCHR_SUCCESS, 0, 0, 0, 0, "ccc"),
            // 进世界
            (Messages.CM_SELCHR, 0, 0, 0, 0, "aaa"),
            (Messages.SM_STARTPLAY, 0, 0, 0, 0, "127.0.0.1/7200/7201"),
            (Messages.SM_LOGON, 1001, 300, 300, 0, ""),
            (Messages.SM_NEWMAP, 1001, 0, 0, 0, "0/比奇省/300/300"),
            // 移动
            (Messages.CM_WALK, 0, 301, 300, 2, ""),
            (Messages.SM_WALK, 1001, 301, 300, 2, ""),
            (Messages.CM_RUN, 0, 302, 300, 2, ""),
            // 攻击
            (Messages.CM_HIT, 0, 301, 299, 2, ""),
            (Messages.SM_HIT, 1001, 301, 299, 2, ""),
            (Messages.SM_STRUCK, 1002, 35, 0, 0, ""),
            // 小退
            (Messages.CM_SOFTCLOSE, 0, 0, 0, 0, ""),
            (Messages.SM_OUTOFCONNECTION, 0, 0, 0, 0, ""),
            // 边界值：recog 负数/极大值（截断行为锚定；包号本身必须是表内真值，否则触发"未实现"判据）
            (Messages.SM_EXCHGTAKEON_OK, int.MaxValue, 65535, 65535, 65535, ""),
            (Messages.SM_EXCHGTAKEON_FAIL, -1, 0, 0, 0, ""),
        };
        foreach (var (ident, recog, param, tag, series, bodyGbk) in frameCases)
        {
            AddFrame(lines, ref seq, "c2s", ident, recog, param, tag, series, Gb2312.GetBytes(bodyGbk));
            AddFrame(lines, ref seq, "s2c", ident, recog, param, tag, series, Gb2312.GetBytes(bodyGbk));
        }

        // ---- 3. 内部帧头 ServerMessage（20B）向量 ----
        var sm = new ServerMessage
        {
            PacketCode = Grobal2.PacketCode,
            Socket = 12345,
            SessionId = 7,
            Ident = Grobal2.GM_DATA,
            SessionIndex = 3,
            PackLength = -48, // 负值：预编码载荷
        };
        byte[] smBytes = SerializerUtil.Serialize(sm);
        if (smBytes.Length != ServerMessage.PacketSize)
        {
            Console.Error.WriteLine($"FATAL: ServerMessage 序列化长度 {smBytes.Length} != 20");
            return 2;
        }
        lines.Add($"{{\"kind\":\"server-header\",\"seq\":{seq++},\"raw_hex\":\"{Hex(smBytes)}\"," +
                  $"\"packet_code\":{sm.PacketCode},\"socket\":{sm.Socket},\"session_id\":{sm.SessionId}," +
                  $"\"ident\":{sm.Ident},\"session_index\":{sm.SessionIndex},\"pack_length\":{sm.PackLength}}}");

        File.WriteAllLines(outPath, lines);

        // 证据：行数 + 文件 hash
        byte[] fileBytes = File.ReadAllBytes(outPath);
        Console.WriteLine($"vectors: {lines.Count} lines -> {outPath}");
        Console.WriteLine($"sha256: {Sha256Hex(fileBytes)}");
        return 0;
    }

    private static void AddEdcode(List<string> lines, byte[] plain)
    {
        // 真实公开路径：EncryptUtil.Encode → Decode（即 EDCode.EncodeString/DecodeBuff 的底层）
        byte[] encBuf = new byte[plain.Length * 2 + 8];
        int encLen = EncryptUtil.Encode(plain, plain.Length, encBuf);
        byte[] encoded = encBuf[..encLen];
        int decLen = 0;
        byte[] decoded = EncryptUtil.Decode(encoded, encoded.Length, ref decLen);
        if (decoded.Length != plain.Length || Hex(decoded) != Hex(plain))
        {
            throw new InvalidOperationException("C# 侧 EDCode 往返不一致——oracle 自身破裂，中止");
        }
        lines.Add($"{{\"kind\":\"edcode\",\"plain_hex\":\"{Hex(plain)}\",\"encoded_hex\":\"{Hex(encoded)}\"}}");
    }

    private static void AddFrame(List<string> lines, ref int seq, string dir, int ident, int recog, int param, int tag, int series, byte[] body)
    {
        CommandMessage cmd = Messages.MakeMessage(ident, recog, param, tag, series);
        // 线上路径：头 12B 与体分别编码后拼接
        string encHead = EDCode.EncodeMessage(cmd);              // C# 真身：Encode(Serialize(cmd), 12)
        string encBody = body.Length > 0 ? EDCode.EncodeString(Gb2312.GetString(body)) : string.Empty;
        byte[] headBytes = Gb2312.GetBytes(encHead);             // 编码产物恒为 ASCII，gb2312 往返恒等
        byte[] bodyBytes = Gb2312.GetBytes(encBody);
        byte[] frameBytes;
        if (dir == "c2s")
        {
            // Delphi 客户端：Format('#1%s!', [EncodeMessage(msg) + EncodeString(body)])
            frameBytes = new byte[2 + headBytes.Length + bodyBytes.Length + 1];
            frameBytes[0] = (byte)'#';
            frameBytes[1] = (byte)'1';
            Array.Copy(headBytes, 0, frameBytes, 2, headBytes.Length);
            Array.Copy(bodyBytes, 0, frameBytes, 2 + headBytes.Length, bodyBytes.Length);
            frameBytes[^1] = (byte)'!';
        }
        else
        {
            // 网关→客户端：'#' + EDCode(头) + 已编码体 + '!'
            frameBytes = new byte[1 + headBytes.Length + bodyBytes.Length + 1];
            frameBytes[0] = (byte)'#';
            Array.Copy(headBytes, 0, frameBytes, 1, headBytes.Length);
            Array.Copy(bodyBytes, 0, frameBytes, 1 + headBytes.Length, bodyBytes.Length);
            frameBytes[^1] = (byte)'!';
        }
        lines.Add($"{{\"kind\":\"frame\",\"dir\":\"{dir}\",\"seq\":{seq++},\"raw_hex\":\"{Hex(frameBytes)}\"," +
                  $"\"ident\":{cmd.Ident},\"recog\":{cmd.Recog},\"param\":{cmd.Param},\"tag\":{cmd.Tag}," +
                  $"\"series\":{cmd.Series},\"body_len\":{body.Length},\"body_sha256\":\"{Sha256Hex(body)}\"}}");
    }
}
