using System;
using System.IO;
using NUnit.Framework;
using OpenMir2;

namespace GameSrvTest;

/// <summary>
/// 卡 `openmir2-gate-packlen-validation` 的负例牙：<b>包头声明的长度是不可信输入</b>。
///
/// 事故链：`packetLen` 直接取自网络包头 → 当 `MemoryCopy.BlockCopy` 的 count。修复前是越界读
/// （`Buffer.MemoryCopy` 无边界检查 ⇒ 进程级 AV）；`CheckRange` 上线后变成托管异常，但异常抛在
/// TouchSocket 的收包回调里 ⇒ 该连接停读；`int.MaxValue` 还会让 `new byte[]` OOM。
/// 正确做法：拷贝/切片**之前**显式校验，不通过就 WRN + 关闭连接，绝不进入拷贝。
///
/// 三层牙：
/// 1. **行为牙**：三条负例（&lt;0 / &gt;available / int.MaxValue）+ 一条正例，各自断言返回值与原因；
/// 2. **接线牙**：`GameGate/ClientThread.cs` 的 GM_DATA 分支必须先 `TryValidate` 再 `BlockCopy`，
///    且失败分支里有 `_clientSocket.Close()`；
/// 3. **接线牙（客户端封包）**：`GameGate/ClientSession.cs` 在任何切片之前检查 `BuffLen > Data.Length`。
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
    public void Accepts_LengthEqualToAvailable()
    {
        Assert.That(GatePacketLength.TryValidate(0, 0, out string reason), Is.True);
        Assert.That(reason, Is.Empty);
        Assert.That(GatePacketLength.TryValidate(20, 20, out reason), Is.True);
        Assert.That(reason, Is.Empty);
    }

    [Test]
    public void Rejects_NegativeLength_beforeAnyCopy()
    {
        Assert.That(GatePacketLength.TryValidate(-1, 1024, out string reason), Is.False);
        Assert.That(reason, Does.Contain("packetLen=-1"));
        Assert.That(reason, Does.Contain("< 0"));
        // `-int.MinValue` 在 unchecked 下仍是负数（C# 语义）：同样必须被拦。
        Assert.That(GatePacketLength.TryValidate(int.MinValue, 1024, out reason), Is.False);
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
    public void GameGate_EngineFrame_ValidatesBeforeBlockCopy()
    {
        string src = File.ReadAllText(Path.Combine(RepoRoot(), "src", "GameGate", "Services", "ClientThread.cs"));
        int caseIdx = src.IndexOf("case Grobal2.GM_DATA:", StringComparison.Ordinal);
        Assert.That(caseIdx, Is.GreaterThanOrEqualTo(0), "找不到 GM_DATA 分支");
        int validateIdx = src.IndexOf("GatePacketLength.TryValidate", caseIdx, StringComparison.Ordinal);
        int copyIdx = src.IndexOf("MemoryCopy.BlockCopy(data, 0, sendMsg.Buffer, 0, packetLen)", caseIdx, StringComparison.Ordinal);
        Assert.That(validateIdx, Is.GreaterThan(caseIdx), "GM_DATA 分支缺 TryValidate");
        Assert.That(copyIdx, Is.GreaterThan(validateIdx), "TryValidate 必须在 BlockCopy 之前");
        int closeIdx = src.IndexOf("_clientSocket.Close();", validateIdx, StringComparison.Ordinal);
        Assert.That(closeIdx, Is.GreaterThan(validateIdx), "校验失败必须关闭上游连接");
        Assert.That(closeIdx, Is.LessThan(copyIdx), "关闭连接必须在拷贝之前（失败即返回）");
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
}
