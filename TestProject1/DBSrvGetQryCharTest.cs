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
public class DBSrvGetQryCharTest
{
    private const string Db = "mir2_gq_test";
    private static string Cs => $"server=127.0.0.1;uid=root;pwd=;database={Db};";

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
        Exec(conn, $"CREATE DATABASE `{Db}` DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci");
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
    }

    [SetUp]
    public void InstallSink()
    {
        _sink = new CapturingSink();
        LogService.Logger = new LoggerConfiguration()
            .MinimumLevel.Verbose()
            .WriteTo.Sink(_sink)
            .CreateLogger();
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
    public void BadConnection_LogsError_NotWarn()
    {
        // 端口不可达 ⇒ 连接失败路径（与「无行」必须可区分）
        var bad = new PlayDataStorage(new StorageOption
        {
            DBType = "MySQL",
            ConnectionString = "server=127.0.0.1;port=13306;uid=root;pwd=;database=mir2_gq_test;Connection Timeout=1;",
        });
        QueryChr? rcd = null;
        var ok = bad.GetQryChar(1, ref rcd);
        Assert.That(ok, Is.False, "连不上库必须 fail-closed（不得伪装成有记录）");
        var hasError = _sink.Has(LogEventLevel.Error, "GetQryChar 异常") ||
                       _sink.Has(LogEventLevel.Error, "打开连接失败");
        Assert.That(hasError, Is.True, "异常/连接失败必须是 Error 级且文案可区分（牙④）");
        Assert.That(
            _sink.Has(LogEventLevel.Warning, "GetQryChar 无行"),
            Is.False,
            "异常路径不得伪装成「无行」的 warn");
    }
}
