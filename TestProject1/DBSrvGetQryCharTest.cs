using System.Collections.Generic;
using System.Linq;
using DBSrv.Storage;
using DBSrv.Storage.Model;
using DBSrv.Storage.MySQL;
using MySqlConnector;
using NUnit.Framework;
using OpenMir2;
using Serilog;
using Serilog.Core;
using Serilog.Events;

namespace GameSrvTest;

/// <summary>
/// **卡 `openmir2-dbsrv-getqrychar-failopen` 的可重复回归牙（牙②④）**。
///
/// 钉的是存储层：`PlayDataStorage.GetQryChar` 在「查不到行」时**必须** `false`（不是 true+null），
/// 在「DB 异常」时必须打 **Error**（文案含「异常」/「打开连接失败」），两者在日志级别上可区分。
/// 用一次性临时库（`mir2_gq_test`，测试自建自删）+ Serilog 内存 sink 捕获日志。
///
/// 红检：把 `if (!found) { LogService.Warn(...) }` 删掉 ⇒ 牙②（内层 warn）红；
/// 把 `return found` 改回 `return true` ⇒ 牙（返回值）红；把异常分支的 Error 降成 Warn ⇒ 牙④红。
/// </summary>
[TestFixture]
[Explicit("需要本机 MySQL（root/空口令）；门禁以 --filter 显式跑。干净环境直接 dotnet test 不跑本 fixture 是设计，不是遗漏。")]
public class DBSrvGetQryCharTest
{
    // 库名带进程后缀：并发跑不互踩；固定名 DROP 会误删别人的库。
    private static readonly string Db = $"mir2_gq_test_{Environment.ProcessId}";
    /// 空库：`Open` 成功但 `characters` 表不存在 ⇒ 触发 `GetQryChar` 的 catch（牙④的真实路径）。
    private static readonly string DbNoTable = $"mir2_gq_test_{Environment.ProcessId}_nochr";
    private static string Cs => $"server=127.0.0.1;uid=root;pwd=;database={Db};";
    private static string CsNoTable => $"server=127.0.0.1;uid=root;pwd=;database={DbNoTable};";

    private sealed class CapturingSink : ILogEventSink
    {
        private readonly List<LogEvent> _events = new();

        public void Emit(LogEvent logEvent)
        {
            lock (_events)
            {
                _events.Add(logEvent);
            }
        }

        public void Clear()
        {
            lock (_events)
            {
                _events.Clear();
            }
        }

        public bool Has(LogEventLevel level, string token)
        {
            lock (_events)
            {
                return _events.Any(e => e.Level == level && e.RenderMessage().Contains(token));
            }
        }
    }

    private CapturingSink _sink = null!;

    [OneTimeSetUp]
    public void CreateTempDatabase()
    {
        using var conn = new MySqlConnection("server=127.0.0.1;uid=root;pwd=;");
        conn.Open();
        Exec(conn, $"DROP DATABASE IF EXISTS `{Db}`");
        Exec(conn, $"DROP DATABASE IF EXISTS `{DbNoTable}`");
        Exec(conn, $"CREATE DATABASE `{Db}` DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci");
        // 只建库、**不建 characters 表**：给牙④制造"Open 成功但查询抛异常"。
        Exec(conn, $"CREATE DATABASE `{DbNoTable}` DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci");
        Exec(conn, $"CREATE TABLE `{Db}`.`characters` (" +
                   "`Id` INT NOT NULL PRIMARY KEY, `ChrName` VARCHAR(32) NOT NULL, `Job` TINYINT NOT NULL, " +
                   "`Hair` TINYINT NOT NULL, `Sex` TINYINT NOT NULL, `Level` SMALLINT NOT NULL)");
    }

    [OneTimeTearDown]
    public void DropTempDatabase()
    {
        using var conn = new MySqlConnection("server=127.0.0.1;uid=root;pwd=;");
        conn.Open();
        Exec(conn, $"DROP DATABASE IF EXISTS `{Db}`");
        Exec(conn, $"DROP DATABASE IF EXISTS `{DbNoTable}`");
    }

    private Serilog.ILogger? _prevLogger;

    [SetUp]
    public void InstallSink()
    {
        _prevLogger = LogService.Logger;
        _sink = new CapturingSink();
        LogService.Logger = new LoggerConfiguration()
            .MinimumLevel.Verbose()
            .WriteTo.Sink(_sink)
            .CreateLogger();
    }

    [TearDown]
    public void RestoreLogger()
    {
        if (_prevLogger is not null)
        {
            LogService.Logger = _prevLogger;
        }
    }

    private static void Exec(MySqlConnection conn, string sql)
    {
        using var cmd = conn.CreateCommand();
        cmd.CommandText = sql;
        cmd.ExecuteNonQuery();
    }

    private static void InsertRow(int id, string name, byte job, byte hair, byte sex, ushort level)
    {
        using var conn = new MySqlConnection(Cs);
        conn.Open();
        using var cmd = conn.CreateCommand();
        cmd.CommandText = $"INSERT INTO `characters` (Id, ChrName, Job, Hair, Sex, Level) VALUES (@id,@n,@j,@h,@s,@l)";
        cmd.Parameters.AddWithValue("@id", id);
        cmd.Parameters.AddWithValue("@n", name);
        cmd.Parameters.AddWithValue("@j", job);
        cmd.Parameters.AddWithValue("@h", hair);
        cmd.Parameters.AddWithValue("@s", sex);
        cmd.Parameters.AddWithValue("@l", level);
        cmd.ExecuteNonQuery();
    }

    private static void DeleteRow(int id)
    {
        using var conn = new MySqlConnection(Cs);
        conn.Open();
        using var cmd = conn.CreateCommand();
        cmd.CommandText = "DELETE FROM `characters` WHERE Id=@id";
        cmd.Parameters.AddWithValue("@id", id);
        cmd.ExecuteNonQuery();
    }

    private static PlayDataStorage NewStorage() =>
        new(new StorageOption { DBType = "MySQL", ConnectionString = Cs });

    [Test]
    public void MissingRow_ReturnsFalse_DoesNotThrow_AndLogsInnerWarn()
    {
        DeleteRow(2);
        QueryChr? rcd = null;
        var ok = NewStorage().GetQryChar(2, ref rcd);
        Assert.That(ok, Is.False, "查不到行必须 false（修前是 true + null 记录 ⇒ 调用方 NRE）");
        Assert.That(rcd, Is.Null, "查不到行不得伪造记录");
        Assert.That(
            _sink.Has(LogEventLevel.Warning, "GetQryChar 无行 id=2"),
            Is.True,
            "内层 warn 必须存在（牙②：删掉这条 warn ⇒ 本断言红）");
    }

    [Test]
    public void PresentRow_ReturnsTrue_WithFields()
    {
        InsertRow(1, "hero", 1, 2, 0, 33);
        QueryChr? rcd = null;
        var ok = NewStorage().GetQryChar(1, ref rcd);
        Assert.That(ok, Is.True, "有行必须 true");
        Assert.That(rcd, Is.Not.Null);
        Assert.That(rcd!.Name, Is.EqualTo("hero"));
        Assert.That(rcd.Job, Is.EqualTo((byte)1));
        Assert.That(rcd.Hair, Is.EqualTo((byte)2));
        Assert.That(rcd.Sex, Is.EqualTo((byte)0));
        Assert.That(rcd.Level, Is.EqualTo((ushort)33));
    }

    [Test]
    public void OpenFailure_LogsOpeningError_Exactly_NotWarn()
    {
        // 端口不可达 ⇒ `DBContext.Open` 自己吞掉异常并把 success 置 false（DBContext.cs:20-35）
        // ⇒ 走 `GetQryChar` 的「打开连接失败」支。**这条只钉该支，文案精确匹配，不用「或」。**
        var bad = new PlayDataStorage(new StorageOption
        {
            DBType = "MySQL",
            ConnectionString = "server=127.0.0.1;port=13306;uid=root;pwd=;database=" + Db + ";Connection Timeout=1;",
        });
        QueryChr? rcd = null;
        var ok = bad.GetQryChar(1, ref rcd);
        Assert.That(ok, Is.False, "连不上库必须 fail-closed");
        Assert.That(
            _sink.Has(LogEventLevel.Error, "GetQryChar 打开连接失败"),
            Is.True,
            "必须是「打开连接失败」这一支（精确文案）");
        Assert.That(
            _sink.Has(LogEventLevel.Warning, "GetQryChar 无行"),
            Is.False,
            "打开失败不得伪装成「无行」的 warn");
    }

    [Test]
    public void QueryException_LogsExceptionError_Exactly_NotWarn()
    {
        // **牙④的真实路径**：`Open` 成功（库存在）但 `characters` 表不存在 ⇒ `ExecuteReader` 抛
        // ⇒ 进入 `GetQryChar` 的 catch ⇒ `Error($"[QueryChr] GetQryChar 异常 id=…", ex)`。
        // 删掉 Query.cs 里 catch 的这行 Error ⇒ 本用例必红（变异已重放）。
        var noTable = new PlayDataStorage(new StorageOption
        {
            DBType = "MySQL",
            ConnectionString = CsNoTable,
        });
        QueryChr? rcd = null;
        var ok = noTable.GetQryChar(1, ref rcd);
        Assert.That(ok, Is.False, "查询异常必须 fail-closed");
        Assert.That(
            _sink.Has(LogEventLevel.Error, "GetQryChar 异常"),
            Is.True,
            "异常路径必须是 Error 且文案含「异常」（精确文案，不用「或」）");
        Assert.That(
            _sink.Has(LogEventLevel.Warning, "GetQryChar 无行"),
            Is.False,
            "异常不得伪装成「无行」的 warn");
    }
}
