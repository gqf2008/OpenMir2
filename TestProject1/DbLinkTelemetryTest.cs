using System;
using System.IO;
using System.Linq;
using System.Runtime.CompilerServices;
using DBSrv;
using DBSrv.Services.Impl;
using OpenMir2;
using OpenMir2.DataHandlingAdapters;
using NUnit.Framework;

namespace GameSrvTest;

/// <summary>
/// 卡 `openmir2-db-link-request-drop` 的回归牙：
/// 1. **适配器两道防线**：`GetInstance()` 每帧返回新对象；`OnParsingBody` 克隆传入缓冲；
/// 2. **6000 面六桶**：六桶互相独立、汇总行字段齐全；
/// 3. **接线**：六桶在 `DataService` 里各只有一个调用点，且在对应分支里；
/// 4. **关服幂等**：`UserService.Stop()` 连续调用两次不抛（原实现第二次 NRE）。
/// </summary>
[TestFixture]
public class DbLinkTelemetryTest
{
    // ---------- 1) 适配器两道防线 ----------

    private sealed class ProbeAdapter : PlayerDataFixedHeaderDataHandlingAdapter
    {
        public PlayerDataMessageFixedHeaderRequestInfo NewInstance()
        {
            return GetInstance();
        }
    }

    /// <summary>
    /// **工厂语义牙**：两次 `GetInstance()` 必须是**不同对象**。
    /// 把实现改回返回 static 单例 ⇒ 本用例红（帧边界会被下一帧覆盖）。
    /// </summary>
    [Test]
    public void GetInstanceReturnsDistinctObjects()
    {
        var adapter = new ProbeAdapter();
        var a = adapter.NewInstance();
        var b = adapter.NewInstance();
        Assert.That(ReferenceEquals(a, b), Is.False, "GetInstance 必须每帧返回新对象（工厂语义）");
    }

    /// <summary>
    /// **克隆牙**：解析后再修改传入 buffer，`Message` 内容不得改变。
    /// 把 `OnParsingBody` 改回直接存引用 ⇒ 本用例红。
    /// </summary>
    [Test]
    public void OnParsingBodyClonesIncomingBuffer()
    {
        var info = new PlayerDataMessageFixedHeaderRequestInfo();
        var buf = new byte[] { 1, 2, 3, 4 };
        Assert.That(info.OnParsingBody(buf), Is.True);
        buf[0] = 99;
        Assert.That(info.Message[0], Is.EqualTo((byte)1), "解析结果必须与传入 buffer 解耦（克隆）");
        Assert.That(ReferenceEquals(info.Message, buf), Is.False, "不得与上层缓冲共享引用");
    }

    // ---------- 2) 六桶行为 ----------

    [SetUp]
    public void Reset()
    {
        EntryTelemetry.ResetForTests();
    }

    [Test]
    public void SixDbCountersAreIndependentAndSummarised()
    {
        EntryTelemetry.DbFramesDecoded();
        EntryTelemetry.AddDbBytesRead(100);
        EntryTelemetry.DbParseError();
        EntryTelemetry.DbSignReject();
        EntryTelemetry.DbFailReply();
        EntryTelemetry.DbBadRequestInfo();

        var line = EntryTelemetry.SummaryLine();
        foreach (var key in new[]
                 {
                     "db_conn_bytes_read=100", "db_conn_frames_decoded=1", "db_conn_parse_error=1",
                     "db_conn_sign_reject=1", "db_conn_fail_reply=1", "db_conn_bad_request_info=1",
                     "db_conn_loadhumanrcd_decoded=0",
                 })
        {
            Assert.That(line, Does.Contain(key), $"汇总行缺/错 {key}: {line}");
        }
        // 取档四桶不得被 6000 面计数带偏
        Assert.That(line, Does.Contain("load_request_seen=0"), line);
        Assert.That(line, Does.Contain("load_responded_ok=0"), line);
    }

    // ---------- 3) 接线（源码断言） ----------

    private static string RepoRoot([CallerFilePath] string thisFile = "")
    {
        Assert.That(thisFile, Is.Not.Empty, "CallerFilePath 为空 ⇒ 接线牙无法定位源码");
        var dir = new DirectoryInfo(Path.GetDirectoryName(thisFile)!);
        while (dir != null)
        {
            if (File.Exists(Path.Combine(dir.FullName, "src", "DBSrv", "Services", "Impl", "DataService.cs")))
            {
                return dir.FullName;
            }
            dir = dir.Parent;
        }
        Assert.Fail("找不到仓库根（DataService.cs）");
        return string.Empty;
    }

    private static string DataServiceSource()
    {
        return File.ReadAllText(Path.Combine(RepoRoot(), "src", "DBSrv", "Services", "Impl", "DataService.cs"));
    }

    private static int Count(string haystack, string needle)
    {
        var n = 0;
        var i = 0;
        while ((i = haystack.IndexOf(needle, i, StringComparison.Ordinal)) >= 0)
        {
            n++;
            i += needle.Length;
        }
        return n;
    }

    [Test]
    public void DataServiceCallsEachDbCounterExactlyOnceInItsBranch()
    {
        var src = DataServiceSource();
        foreach (var c in new[] { "DbFramesDecoded", "AddDbBytesRead", "DbParseError", "DbSignReject", "DbFailReply", "DbBadRequestInfo", "DbLoadHumanRcdDecoded" })
        {
            var needle = string.Concat("EntryTelemetry.", c, "(");
            Assert.That(Count(src, needle), Is.EqualTo(1), $"{needle} 必须恰好一个调用点");
        }

        // 分支定位：RequestInfo 类型不符 / PacketCode 不符 / 签名拒绝 / 闸门拒绝
        AssertBranch(src, "PlayerDataMessageFixedHeaderRequestInfo fixedHeader", "DbBadRequestInfo", "RequestInfo 类型不符分支");
        AssertBranch(src, "Grobal2.PacketCode", "DbParseError", "PacketCode 校验分支");
        AssertBranch(src, "client.Close()", "DbSignReject", "签名拒绝（关连接）分支");
        AssertBranch(src, "Messages.DBR_FAIL", "DbFailReply", "闸门拒绝回 DBR_FAIL 分支");
    }

    private static void AssertBranch(string src, string anchor, string counter, string what)
    {
        var i = src.IndexOf(anchor, StringComparison.Ordinal);
        Assert.That(i, Is.GreaterThanOrEqualTo(0), $"{what}：找不到锚点 {anchor}");
        var from = Math.Max(0, i - 600);
        var seg = src.Substring(from, Math.Min(1200, src.Length - from));
        Assert.That(seg, Does.Contain(string.Concat("EntryTelemetry.", counter, "(")), $"{counter} 必须落在{what}里");
    }

    /// <summary>
    /// **6000 面关服幂等 + 零连接安全**（卡 `openmir2-entry-gap-buckets`）：`DataService.Stop()` 连调两次
    /// 不抛；从未起服（零连接）时也不抛。原实现直接把 `_serverSocket.Stop()` 交出去，
    /// TouchSocket 的 `ReleaseAll()` 在并发枚举连接集合时抛 `Collection was modified`（出现在 `final` 之后）。
    /// </summary>
    [Test]
    public void DataServiceStopIsIdempotentAndSafeWithoutClients()
    {
        // Stop 的兜底分支会写 WRN ⇒ 需要一个可用 logger（生产由启动流程装配）
        var prevLogger = LogService.Logger;
        LogService.Logger = new Serilog.LoggerConfiguration().MinimumLevel.Verbose().CreateLogger();
        try
        {
            var svc = new DataService(new DBSrv.Conf.SettingsModel(), null!, null!, null!);
            Assert.DoesNotThrow(() => svc.Stop(), "零连接时 Stop 不得抛");
            Assert.DoesNotThrow(() => svc.Stop(), "第二次 Stop 必须幂等");
        }
        finally
        {
            LogService.Logger = prevLogger;
        }
    }

    // ---------- 4) 关服幂等 ----------

    /// <summary>
    /// **幂等牙**：`Stop()` 连续两次不抛。原实现第二次调用会在 `gateInfo.UserList` 已置 null 后解引用 ⇒ NRE，
    /// 打断 teardown（关服窗口的 CLR 错误由此诱发，见卡 `openmir2-db-link-request-drop` 的 B 支）。
    /// </summary>
    [Test]
    public void UserServiceStopIsIdempotent()
    {
        var svc = new UserService(null!, null!, null!, null!);
        var gates = (SelGateInfo[])svc.GetGates;
        gates[0] = new SelGateInfo { UserList = new System.Collections.Generic.List<SessionUserInfo> { null! } };

        Assert.DoesNotThrow(() => svc.Stop(), "第一次 Stop 必须不抛");
        Assert.DoesNotThrow(() => svc.Stop(), "第二次 Stop 必须幂等（原实现 NRE）");
        Assert.That(gates[0].UserList, Is.Null, "Stop 后 UserList 应为 null（行为不变）");
    }
}
