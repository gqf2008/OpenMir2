using System;
using System.IO;
using NUnit.Framework;
using OpenMir2;

namespace GameSrvTest;

/// <summary>
/// 卡 `openmir2-db-write-stall` 的回归牙。
///
/// 事故：`SendMessage` 用目的缓冲长度当**源**长度，`MemoryCopy.BlockCopy(dataBuff,0,data,0,data.Length)`
/// 从 6 字节包头数组越界读 `data.Length-6` 字节；内部是 `Buffer.MemoryCopy`（无边界检查），
/// 大回包跨出 GC 段 ⇒ 访问违例 ⇒ CLR 致命错误 `0x80131506` ⇒ 卡死在 `NtRaiseHardError`
/// 报告路径（进程活着、该连接线程永不返回）。
///
/// 三层牙：
/// 1. **行为牙**：`MemoryCopy.BlockCopy` 对越界必须抛 `ArgumentOutOfRangeException`
///    （把进程级致命降级为托管异常）；
/// 2. **接线牙**：九个 `SendMessage` 变体的第一段拷贝 count 必须取 `dataBuff.Length`；
/// 3. 变异（把某处改回 `data.Length`）会让 2 变红——设计上如此，勿"修测试"。
/// </summary>
[TestFixture]
public class MemoryCopyBoundsTest
{
    private static string RepoRoot()
    {
        var dir = new DirectoryInfo(TestContext.CurrentContext.TestDirectory);
        while (dir != null && !Directory.Exists(Path.Combine(dir.FullName, "src", "DBSrv")))
        {
            dir = dir.Parent;
        }
        Assert.That(dir, Is.Not.Null, "找不到仓库根（含 src/DBSrv）");
        return dir!.FullName;
    }

    /// <summary>九个同款 `SendMessage`：文件 → 相对路径。</summary>
    private static readonly string[] SendMessageFiles =
    {
        Path.Combine("src", "DBSrv", "Services", "Impl", "DataService.cs"),
        Path.Combine("src", "DBSrv", "Services", "Impl", "MarketService.cs"),
        Path.Combine("src", "DBSrv", "Services", "Impl", "UserService.cs"),
        Path.Combine("src", "GameSrv", "Services", "DataQueryServer.cs"),
        Path.Combine("src", "LoginGate", "Services", "ClientThread.cs"),
        Path.Combine("src", "LoginSrv", "Services", "ClientSession.cs"),
        Path.Combine("src", "LoginSrv", "Services", "LoginServer.cs"),
        Path.Combine("src", "Modules", "MarketSystem", "Services", "MarketService.cs"),
        Path.Combine("src", "SelGate", "Services", "ClientThread.cs"),
    };

    [Test]
    public void BlockCopy_Throws_When_Count_Exceeds_Source()
    {
        byte[] source = new byte[6];
        byte[] destination = new byte[64];
        Assert.Throws<ArgumentOutOfRangeException>(
            () => MemoryCopy.BlockCopy(source, 0, destination, 0, destination.Length));
    }

    [Test]
    public void BlockCopy_Throws_When_Offset_Plus_Count_Exceeds_Destination()
    {
        byte[] source = new byte[64];
        byte[] destination = new byte[8];
        Assert.Throws<ArgumentOutOfRangeException>(
            () => MemoryCopy.BlockCopy(source, 0, destination, 4, 8));
    }

    [Test]
    public void BlockCopy_Copies_Header_Then_Body()
    {
        byte[] header = { 1, 2, 3, 4, 5, 6 };
        byte[] body = { 7, 8, 9 };
        byte[] data = new byte[header.Length + body.Length];
        MemoryCopy.BlockCopy(header, 0, data, 0, header.Length);
        MemoryCopy.BlockCopy(body, 0, data, header.Length, body.Length);
        Assert.That(data, Is.EqualTo(new byte[] { 1, 2, 3, 4, 5, 6, 7, 8, 9 }));
    }

    /// <summary>
    /// 接线牙：**九个** `SendMessage` 的第一段拷贝必须用**源**长度（6 字节包头）。
    /// 运行时拼串，避免把测试自身字面量数进去。
    /// </summary>
    [Test]
    public void EverySendMessage_Copies_Header_With_Source_Length()
    {
        string bad = "MemoryCopy.BlockCopy(dataBuff, 0, data, 0, data" + ".Length);";
        string good = "MemoryCopy.BlockCopy(dataBuff, 0, data, 0, dataBuff" + ".Length);";
        string root = RepoRoot();
        foreach (string rel in SendMessageFiles)
        {
            string path = Path.Combine(root, rel);
            Assert.That(File.Exists(path), Is.True, $"缺文件 {rel}");
            string src = File.ReadAllText(path);
            Assert.That(src.Contains(bad), Is.False,
                $"{rel} 仍有越界读：count 取了目的长度（源只有包头长度）");
            Assert.That(src.Contains(good), Is.True,
                $"{rel} 必须用 dataBuff.Length 作为包头拷贝长度");
        }
    }
}
