using System;
using System.IO;
using System.Linq;
using System.Runtime.CompilerServices;
using DBSrv.Services.Impl;
using NUnit.Framework;

namespace GameSrvTest;

/// <summary>
/// 卡 `openmir2-entry-load-reply-buckets` 的**可重复回归牙**（DBSrv 5100 取档面四桶）。
///
/// 两层：
/// 1. **行为牙**（`EntryTelemetry` 单测）：四桶各加各的、样本只放行前 5 条、汇总行四字段齐全；
/// 2. **接线牙**（读 `DataService.cs` 源码）：四个桶与两条样本在 `DataService` 里各只允许**一个**
///    调用点——桶实现改了行为牙会红，调用点被删/被挪走接线牙会红。
///
/// 为什么接线用源码断言：`DataService.ProcessServerMsg`/`LoadHumanRcd` 要真 socket 与真库才走得到，
/// 没有可单测的边界（与本仓 Rust 侧 `entry_telemetry_wiring` 同款取舍）。
/// needle 一律**运行时拼串**，避免把测试自身的字面量数进去。
/// </summary>
[TestFixture]
public class DBSrvEntryTelemetryTest
{
    private const string BucketErrorPrefix = "[EntryTelemetry]";

    [SetUp]
    public void Reset()
    {
        EntryTelemetry.ResetForTests();
    }

    // ---------- 1) 行为牙 ----------

    [Test]
    public void EachBucketIncrementsOnlyItself()
    {
        Assert.That(EntryTelemetry.LoadRequestSeen(), Is.EqualTo(1L));
        Assert.That(EntryTelemetry.LoadRespondedOk(), Is.EqualTo(1L));
        Assert.That(EntryTelemetry.LoadRespondedFail(), Is.EqualTo(1L));
        Assert.That(EntryTelemetry.LoadDroppedNoSession(), Is.EqualTo(1L));

        var line = EntryTelemetry.SummaryLine();
        Assert.That(line, Does.Contain("load_request_seen=1"), line);
        Assert.That(line, Does.Contain("load_responded_ok=1"), line);
        Assert.That(line, Does.Contain("load_responded_fail=1"), line);
        Assert.That(line, Does.Contain("load_dropped_no_session=1"), line);
    }

    /// <summary>四桶不得互相串：只动一个桶，另三个必须仍是 0（删/改任一桶的增量 ⇒ 本用例红）。</summary>
    [Test]
    public void BucketsAreIndependent()
    {
        EntryTelemetry.LoadDroppedNoSession();
        var line = EntryTelemetry.SummaryLine();
        Assert.That(line, Does.Contain("load_dropped_no_session=1"), line);
        Assert.That(line, Does.Contain("load_request_seen=0"), line);
        Assert.That(line, Does.Contain("load_responded_ok=0"), line);
        Assert.That(line, Does.Contain("load_responded_fail=0"), line);

        EntryTelemetry.ResetForTests();
        EntryTelemetry.LoadRespondedFail();
        line = EntryTelemetry.SummaryLine();
        Assert.That(line, Does.Contain("load_responded_fail=1"), line);
        Assert.That(line, Does.Contain("load_responded_ok=0"), line);
        Assert.That(line, Does.Contain("load_dropped_no_session=0"), line);
    }

    /// <summary>样本截断：前 SampleLimit 条 true，之后恒 false（改成 &lt;= 之外的比较 ⇒ 本用例红）。</summary>
    [Test]
    public void SampleCapsAtLimit()
    {
        for (var i = 1; i <= EntryTelemetry.SampleLimit; i++)
        {
            Assert.That(EntryTelemetry.SampleReplyFail(), Is.True, $"第 {i} 条应打样本");
            Assert.That(EntryTelemetry.SampleDroppedNoSession(), Is.True, $"第 {i} 条应打样本");
        }
        Assert.That(
            EntryTelemetry.SampleReplyFail(),
            Is.False,
            $"第 {EntryTelemetry.SampleLimit + 1} 条及以后不得打（截断牙）");
        Assert.That(
            EntryTelemetry.SampleDroppedNoSession(),
            Is.False,
            $"第 {EntryTelemetry.SampleLimit + 1} 条及以后不得打（截断牙）");
    }

    [Test]
    public void SummaryLineCarriesAllFourFields()
    {
        var line = EntryTelemetry.SummaryLine();
        Assert.That(line, Does.StartWith(BucketErrorPrefix), line);
        foreach (var key in new[]
                 {
                     "load_request_seen=", "load_responded_ok=",
                     "load_responded_fail=", "load_dropped_no_session=",
                 })
        {
            Assert.That(line, Does.Contain(key), $"汇总行缺字段 {key}: {line}");
        }
    }

    // ---------- 2) 接线牙（源码断言） ----------

    /// <summary>
    /// 仓库根：从**本文件编译期路径**（`[CallerFilePath]`，实参不可显式传）向上找到含
    /// `src/DBSrv/Services/Impl/DataService.cs` 的目录。找不到 ⇒ 用例判红（不静默跳过）。
    /// </summary>
    private static string RepoRoot([CallerFilePath] string thisFile = "")
    {
        Assert.That(thisFile, Is.Not.Empty, "CallerFilePath 为空 ⇒ 接线牙无法定位源码，判红");
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
    public void DataServiceCallsEachBucketExactlyOnce()
    {
        var src = DataServiceSource();
        foreach (var bucket in new[] { "LoadRequestSeen", "LoadRespondedOk", "LoadRespondedFail", "LoadDroppedNoSession" })
        {
            var needle = string.Concat("EntryTelemetry.", bucket, "()");
            Assert.That(Count(src, needle), Is.EqualTo(1), $"{needle} 必须恰好一个调用点");
        }
        foreach (var sample in new[] { "SampleReplyFail", "SampleDroppedNoSession" })
        {
            var needle = string.Concat("EntryTelemetry.", sample, "()");
            Assert.That(Count(src, needle), Is.EqualTo(1), $"{needle} 必须恰好一个调用点（否则只有计数、没有样本）");
        }
    }

    /// <summary>静默丢弃那一桶必须挂在 `SessionID &lt;= 0` 的早退分支里（挪走 ⇒ 缺口又被算进「请求没到」）。</summary>
    [Test]
    public void DroppedNoSessionSitsInEarlyReturnBranch()
    {
        var src = DataServiceSource();
        var anchor = string.Concat("if (loadHumanPacket.SessionID <= ", "0)");
        var i = src.IndexOf(anchor, StringComparison.Ordinal);
        Assert.That(i, Is.GreaterThanOrEqualTo(0), "找不到 `SessionID <= 0` 早退分支");
        var seg = src.Substring(i, Math.Min(900, src.Length - i));
        var needle = string.Concat("EntryTelemetry.", "LoadDroppedNoSession()");
        Assert.That(seg, Does.Contain(needle), "load_dropped_no_session 必须在该早退分支里");
        Assert.That(seg, Does.Contain("return;"), "该分支必须仍原样早退（不改行为）");
    }

    /// <summary>失败码样本必须挂在 `recog=nCheckCode` 的失败回包之后。</summary>
    [Test]
    public void ReplyFailSampleSitsInFailureBranch()
    {
        var src = DataServiceSource();
        var needle = string.Concat("EntryTelemetry.", "SampleReplyFail()");
        var i = src.IndexOf(needle, StringComparison.Ordinal);
        Assert.That(i, Is.GreaterThanOrEqualTo(0), "找不到失败码样本调用点");
        var before = src.Substring(Math.Max(0, i - 700), Math.Min(700, i));
        Assert.That(
            before,
            Does.Contain("Messages.DBR_LOADHUMANRCD, nCheckCode"),
            "失败码样本必须紧跟 `DBR_LOADHUMANRCD, nCheckCode` 那条回包");
    }

    // ---------- 3) 终值与心跳（卡面验收增补） ----------

    /// <summary>心跳：不到间隔不打；到点但**值没变**也不打；值变了才打。</summary>
    [Test]
    public void HeartbeatRespectsIntervalAndChange()
    {
        EntryTelemetry.LoadRequestSeen();
        var first = EntryTelemetry.Heartbeat(100_000);
        Assert.That(first, Is.Not.Null, "第一次到点必须打（此前没有基线）");

        Assert.That(
            EntryTelemetry.Heartbeat(105_000),
            Is.Null,
            "间隔未到不得打（10s 心跳）");

        Assert.That(
            EntryTelemetry.Heartbeat(120_000),
            Is.Null,
            "到点但值没变不得打（去重）");

        EntryTelemetry.LoadDroppedNoSession();
        var changed = EntryTelemetry.Heartbeat(140_000);
        Assert.That(changed, Is.Not.Null, "值变了必须打");
        Assert.That(changed, Does.Contain("load_dropped_no_session=1"), changed);
    }

    /// <summary>
    /// **终值去重牙**：即使与最后一行心跳完全相同，`FinalLine()` 也必须返回一行。
    /// 把去重套到终值行上（`if (line == _lastSummary) return null;`）⇒ 本用例红。
    /// </summary>
    [Test]
    public void FinalLineAlwaysPrintsEvenIfUnchanged()
    {
        EntryTelemetry.LoadRespondedOk();
        var beat = EntryTelemetry.Heartbeat(200_000);
        Assert.That(beat, Is.Not.Null);

        var final1 = EntryTelemetry.FinalLine();
        var final2 = EntryTelemetry.FinalLine();
        Assert.That(final1, Is.Not.Null, "终值行不得被去重吞掉（即使与心跳完全相同）");
        Assert.That(final2, Is.Not.Null, "连续两次终值都必须打印（终值不去重）");
        Assert.That(final1, Does.Contain("final"), final1);
        Assert.That(final1, Does.Contain("load_responded_ok=1"), final1);
    }

    /// <summary>接线：心跳必须挂在 TimedService 的 1s 定时循环里；终值必须挂在进程退出路径上。</summary>
    [Test]
    public void HeartbeatAndFinalAreWired()
    {
        var root = RepoRoot();
        var timed = File.ReadAllText(Path.Combine(root, "src", "DBSrv", "TimedService.cs"));
        var program = File.ReadAllText(Path.Combine(root, "src", "DBSrv", "Program.cs"));

        var beatNeedle = string.Concat("EntryTelemetry.", "Heartbeat(");
        Assert.That(Count(timed, beatNeedle), Is.EqualTo(1), $"{beatNeedle} 必须恰好一个调用点（TimedService）");

        var finalNeedle = string.Concat("EntryTelemetry.", "FinalLine()");
        Assert.That(Count(program, finalNeedle), Is.EqualTo(1), $"{finalNeedle} 必须恰好一个调用点（Program 退出路径）");
    }
}
