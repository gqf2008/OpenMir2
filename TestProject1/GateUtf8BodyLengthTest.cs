using System;
using System.IO;
using System.Text;
using NUnit.Framework;
using OpenMir2;

namespace GameSrvTest;

/// <summary>
/// 卡 `openmir2-gate-utf8-body-length` 的回归牙：网关控制帧的**长度一律取字节数**。
///
/// 历史缺陷：`SendServerMsg` 用 `data.Length`（字符数）开缓冲、当拷贝量、当 `PackLength`；
/// `HUtil32.GetBytes` 是 GB2312 编码 ⇒ 非 ASCII 正文（中文典型 2 字节/字符）会被**截断**，
/// 且包头声明长度与实际正文不符（Rust `game-gate` 一直是 `ip_bytes.len()` 字节口径）。
///
/// 三层牙：
/// 1. ASCII 基线：帧长/`PackLength`/正文与字节口径一致（旧实现同样通过，证明修复不破坏现状）；
/// 2. **中文负例**：帧长 == 20 + 字节数（**不是** 20 + 字符数），正文逐字节等于 GB2312 编码，
///    `PackLength == 字节数`——把长度换回 `data.Length` 即红；
/// 3. 接线牙：`BuildServerFrame`/`SendServerMsg` 内不得再出现 `data.Length` 当长度。
/// </summary>
[TestFixture]
public class GateUtf8BodyLengthTest
{
    private const int HeaderLen = 20; // ServerMessage.PacketSize

    /// <summary>
    /// .NET Core 默认不带 GB2312 码页表；各服务 `Program.cs` 都会注册，测试进程也要注册，
    /// 否则 `HUtil32.GetBytes` 抛 "gb2312 is not a supported encoding name"。
    /// </summary>
    [OneTimeSetUp]
    public void RegisterCodePages()
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
    }

    private static string RepoRoot()
    {
        var dir = new DirectoryInfo(TestContext.CurrentContext.TestDirectory);
        while (dir != null && !Directory.Exists(Path.Combine(dir.FullName, "src", "GameGate")))
        {
            dir = dir.Parent;
        }

        Assert.That(dir, Is.Not.Null, "找不到仓库根（src/GameGate）");
        return dir!.FullName;
    }

    private static int ReadPackLength(byte[] frame)
    {
        Assert.That(frame.Length, Is.GreaterThanOrEqualTo(HeaderLen));
        return BitConverter.ToInt32(frame, 16); // ServerMessage.PackLength 在 20B 头的 [16,20)
    }

    private static byte[] Body(byte[] frame)
    {
        byte[] body = new byte[frame.Length - HeaderLen];
        Array.Copy(frame, HeaderLen, body, 0, body.Length);
        return body;
    }

    [Test]
    public void EmptyBody_IsHeaderOnly()
    {
        byte[] frame = GameGate.Services.ClientThread.BuildServerFrame(Grobal2.GM_CLOSE, 1, 2, 0, string.Empty);
        Assert.That(frame.Length, Is.EqualTo(HeaderLen));
        Assert.That(ReadPackLength(frame), Is.EqualTo(0));
    }

    [Test]
    public void AsciiBody_MatchesByteLengthBaseline()
    {
        const string ip = "127.0.0.1"; // GM_OPEN 的真实正文形态
        byte[] frame = GameGate.Services.ClientThread.BuildServerFrame(Grobal2.GM_OPEN, 7, 9, 0, ip);
        byte[] token = Encoding.GetEncoding("gb2312").GetBytes(ip);
        Assert.That(frame.Length, Is.EqualTo(HeaderLen + token.Length));
        Assert.That(ReadPackLength(frame), Is.EqualTo(token.Length));
        Assert.That(Body(frame), Is.EqualTo(token));
    }

    [Test]
    public void ChineseBody_IsNotTruncated_AndPackLengthIsByteCount()
    {
        const string text = "中文角色"; // 4 字符 / GB2312 8 字节
        byte[] token = Encoding.GetEncoding("gb2312").GetBytes(text);
        Assert.That(token.Length, Is.GreaterThan(text.Length), "前置：GB2312 中文必须多于字符数");

        byte[] frame = GameGate.Services.ClientThread.BuildServerFrame(Grobal2.GM_OPEN, 3, 4, 0, text);

        // 正例：帧长与正文都按字节数。
        Assert.That(frame.Length, Is.EqualTo(HeaderLen + token.Length));
        Assert.That(Body(frame), Is.EqualTo(token), "正文必须逐字节等于 GB2312 编码（不得截断）");
        Assert.That(ReadPackLength(frame), Is.EqualTo(token.Length), "PackLength 必须是字节数");

        // **负例对照**：旧实现（按字符数）会得到这两个值；断言它们不成立。
        Assert.That(frame.Length, Is.Not.EqualTo(HeaderLen + text.Length), "帧长不得按字符数");
        Assert.That(ReadPackLength(frame), Is.Not.EqualTo(text.Length), "PackLength 不得按字符数");
    }

    [Test]
    public void FrameBuilder_UsesByteLengthNotCharLength()
    {
        string src = File.ReadAllText(Path.Combine(RepoRoot(), "src", "GameGate", "Services", "ClientThread.cs"));
        int start = src.IndexOf("public static byte[] BuildServerFrame", StringComparison.Ordinal);
        Assert.That(start, Is.GreaterThanOrEqualTo(0), "找不到 BuildServerFrame");
        int end = src.IndexOf("private void SendServerMsg", start, StringComparison.Ordinal);
        Assert.That(end, Is.GreaterThan(start), "找不到 SendServerMsg 起点");
        string region = src.Substring(start, end - start);
        Assert.That(region, Does.Contain("strBuff.Length"), "必须用字节数");
        // 传参名 `data` 与字符串 `data.Length` 都要消失（长度只取 strBuff.Length）。
        Assert.That(region.Contains("data.Length", StringComparison.Ordinal), Is.False, "不得再出现 data.Length 当长度");
        Assert.That(region.Contains("new byte[ServerMessage.PacketSize + data.Length]", StringComparison.Ordinal), Is.False);
    }
}
