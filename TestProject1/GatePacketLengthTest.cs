using System;
using System.IO;
using NUnit.Framework;
using OpenMir2;
using OpenMir2.Packets.ServerPackets;

namespace GameSrvTest;

/// <summary>
/// 卡 `openmir2-gate-packlen-validation` 的负例牙：<b>包头声明的长度是不可信输入</b>。
///
/// 事故链：`packetLen` 直接取自网络包头 → 当 `MemoryCopy.BlockCopy` 的 count。修复前是越界读
/// （`Buffer.MemoryCopy` 无边界检查 ⇒ 进程级 AV）；`CheckRange` 上线后变成托管异常，但异常抛在
/// TouchSocket 的收包回调里 ⇒ 该连接停读；`int.MaxValue` 还会让 `new byte[]` OOM。
///
/// **两层语义（本卡的核心）**：
/// - 帧层（`DataMessageFixedHeaderRequestInfo.OnParsingHeader`）**只能交帧**，没有"拒绝并关连接"
///   的出口 ⇒ 对无法表示的长度（`int.MinValue`）降级为 0 字节交帧，**不是放行**；
/// - 放行/拒绝的**唯一判定**在 `ClientThread.TryBuildServerSessionMessage`，与帧层共用
///   `GatePacketLength.TryMagnitude`（单一真源），因此两层不会对同一字段得出相反结论。
///
/// 注意协议语义：`PackLength` **负值是合法的**（"小包/已编码"，绝对值是长度）。"负 count"只可能
/// 来自 `int.MinValue`（`-MinValue` 在 unchecked 下仍是负数），这正是要拦的那一例。
/// </summary>
[TestFixture]
public class GatePacketLengthTest
{
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

    [Test]
    public void Magnitude_UsesSingleSourceOfTruth()
    {
        Assert.That(GatePacketLength.TryMagnitude(5, out int m), Is.True);
        Assert.That(m, Is.EqualTo(5));
        // 负字段是协议合法语义（小包/已编码）：幅值取反，不是错误。
        Assert.That(GatePacketLength.TryMagnitude(-255, out m), Is.True);
        Assert.That(m, Is.EqualTo(255));
        // 唯一无正值表示的值：`-int.MinValue` 在 unchecked 下仍是负数。
        Assert.That(GatePacketLength.TryMagnitude(int.MinValue, out m), Is.False);
    }

    [Test]
    public void Rejects_NegativeMagnitude_beforeAnyCopy()
    {
        // "负 count" 只可能来自 API 误用或 MinValue 传播：两条都必须拦。
        Assert.That(GatePacketLength.TryValidateMagnitude(-1, 1024, out string reason), Is.False);
        Assert.That(reason, Does.Contain("packetLen=-1"));
        Assert.That(GatePacketLength.TryValidateMagnitude(int.MinValue, 1024, out reason), Is.False);
        Assert.That(GatePacketLength.TryValidate(int.MinValue, 0, out reason), Is.False);
        Assert.That(reason, Does.Contain("int.MinValue"));
    }

    [Test]
    public void Accepts_LegalNegativePackLength()
    {
        // `PackLength=-5` ⇒ 幅值 5；available=10 ⇒ 合法（协议：已编码小包）。
        Assert.That(GatePacketLength.TryValidate(-5, 10, out string reason), Is.True);
        Assert.That(reason, Is.Empty);
        Assert.That(GatePacketLength.TryValidate(-5, 4, out reason), Is.False);
        Assert.That(reason, Does.Contain("packetLen=5"));
        Assert.That(reason, Does.Contain("available=4"));
    }

    [Test]
    public void Rejects_LengthGreaterThanAvailable()
    {
        Assert.That(GatePacketLength.TryValidate(21, 20, out string reason), Is.False);
        Assert.That(reason, Does.Contain("packetLen=21"));
        Assert.That(reason, Does.Contain("available=20"));
    }

    [Test]
    public void Rejects_IntMaxValue()
    {
        // `int.MaxValue` 不只是"越界读"，还会让 `new byte[packetLen]` OOM ⇒ 必须在分配前拦。
        Assert.That(GatePacketLength.TryValidate(int.MaxValue, 1 << 20, out string reason), Is.False);
        Assert.That(reason, Does.Contain("packetLen=2147483647"));
        Assert.That(reason, Does.Contain("available=1048576"));
    }

    [Test]
    public void Accepts_LengthEqualToAvailable()
    {
        Assert.That(GatePacketLength.TryValidate(0, 0, out string reason), Is.True);
        Assert.That(reason, Is.Empty);
        Assert.That(GatePacketLength.TryValidate(20, 20, out reason), Is.True);
        Assert.That(reason, Is.Empty);
    }

    // ---- 端到端（同一判定点）：帧 → 会话消息 ----

    [Test]
    public void EngineFrame_MinValue_IsRejectedByTheProcessor()
    {
        var header = MakeHeader(int.MinValue);
        bool ok = GameGate.Services.ClientThread.TryBuildServerSessionMessage(
            header, ReadOnlySpan<byte>.Empty, out _, out string reason);
        Assert.That(ok, Is.False, "MinValue 帧必须被判定点拒绝（而不是当成 0 长度空帧投递）");
        Assert.That(reason, Does.Contain("int.MinValue"));
    }

    [Test]
    public void EngineFrame_Oversize_IsRejectedByTheProcessor()
    {
        var header = MakeHeader(21);
        bool ok = GameGate.Services.ClientThread.TryBuildServerSessionMessage(
            header, new byte[20], out _, out string reason);
        Assert.That(ok, Is.False);
        Assert.That(reason, Does.Contain("packetLen=21"));
        Assert.That(reason, Does.Contain("available=20"));
    }

    [Test]
    public void EngineFrame_Valid_IsAcceptedAndCopiedWhole()
    {
        var header = MakeHeader(5);
        byte[] data = { 1, 2, 3, 4, 5 };
        bool ok = GameGate.Services.ClientThread.TryBuildServerSessionMessage(
            header, data, out GameGate.ServerSessionMessage message, out string reason);
        Assert.That(ok, Is.True);
        Assert.That(reason, Is.Empty);
        Assert.That(message.SessionId, Is.EqualTo(header.SessionId));
        Assert.That(message.Buffer, Is.EqualTo(data));
    }

    // ---- 接线牙 ----

    /// <summary>
    /// GM_DATA 分支必须走唯一判定点，且失败时**先关连接、后返回**；分支内不得再有裸 `new byte[`/拷贝。
    /// 删掉判定或把关闭挪走 ⇒ 本用例红。
    /// </summary>
    [Test]
    public void GameGate_EngineFrame_UsesDecisionPointAndClosesOnFailure()
    {
        string src = File.ReadAllText(Path.Combine(RepoRoot(), "src", "GameGate", "Services", "ClientThread.cs"));
        int caseIdx = src.IndexOf("case Grobal2.GM_DATA:", StringComparison.Ordinal);
        Assert.That(caseIdx, Is.GreaterThanOrEqualTo(0), "找不到 GM_DATA 分支");
        int nextCase = src.IndexOf("case Messages.GM_TEST:", caseIdx, StringComparison.Ordinal);
        Assert.That(nextCase, Is.GreaterThan(caseIdx), "找不到 GM_DATA 分支结尾");
        string branch = src.Substring(caseIdx, nextCase - caseIdx);

        int callIdx = branch.IndexOf("TryBuildServerSessionMessage", StringComparison.Ordinal);
        int closeIdx = branch.IndexOf("_clientSocket.Close();", StringComparison.Ordinal);
        Assert.That(callIdx, Is.GreaterThanOrEqualTo(0), "GM_DATA 分支必须调用唯一判定点");
        Assert.That(closeIdx, Is.GreaterThan(callIdx), "判定失败必须关闭上游连接");
        Assert.That(branch.IndexOf("new byte[", StringComparison.Ordinal), Is.EqualTo(-1), "分支内不得再分配帧缓冲");
        Assert.That(branch.IndexOf("MemoryCopy.BlockCopy", StringComparison.Ordinal), Is.EqualTo(-1), "分支内不得再直接拷贝");
    }

    [Test]
    public void GameGate_ClientFrame_ChecksDeclaredLengthBeforeSlices()
    {
        string src = File.ReadAllText(Path.Combine(RepoRoot(), "src", "GameGate", "Services", "ClientSession.cs"));
        int methodIdx = src.IndexOf("public unsafe void ProcessSessionPacket", StringComparison.Ordinal);
        Assert.That(methodIdx, Is.GreaterThanOrEqualTo(0), "找不到 ProcessSessionPacket");
        int checkIdx = src.IndexOf("packetLen > destinationSpan.Length", methodIdx, StringComparison.Ordinal);
        int firstSlice = src.IndexOf("destinationSpan[..", methodIdx, StringComparison.Ordinal);
        Assert.That(checkIdx, Is.GreaterThan(methodIdx), "缺 BuffLen > Data.Length 校验");
        Assert.That(firstSlice, Is.GreaterThan(checkIdx), "长度校验必须在任何切片之前");
    }

    private static ServerMessage MakeHeader(int packLength)
    {
        return new ServerMessage
        {
            PacketCode = Grobal2.PacketCode,
            Socket = 0,
            SessionId = 7,
            Ident = (ushort)Grobal2.GM_DATA,
            SessionIndex = 0,
            PackLength = packLength
        };
    }
}
