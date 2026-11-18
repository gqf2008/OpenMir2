using MySqlConnector;
using NLog;
using OpenMir2;
using System;
using System.Data;
using System.Diagnostics;

namespace DBSrv.Storage.MySQL
{
    public class StorageContext : IDisposable
    {

        private readonly StorageOption _storageOption;
        private MySqlConnection? _connection;
        private MySqlTransaction? _transaction;
        private bool _openedByUs;

        public StorageContext(StorageOption storageOption)
        {
            _storageOption = storageOption;
        }

        public void Open(ref bool success)
        {
            _connection = new MySqlConnection(_storageOption.ConnectionString);
            long t0 = Stopwatch.GetTimestamp();
            try
            {
                _connection.Open();
                success = true;
            }
            catch (Exception e)
            {
                LogService.Error("打开数据库[MySql]失败.");
                LogService.Error(e.StackTrace);
                success = false;
            }
            finally
            {
                // 旁路计时（卡 `openmir2-db-write-stall`）：连接池借出/建连耗时；不改判定。
                StorageTiming.RecordOpen((Stopwatch.GetTimestamp() - t0) * 1000L / Stopwatch.Frequency, success);
                if (success)
                {
                    StorageTiming.ConnOpened();
                    _openedByUs = true;
                }
            }
        }

        public MySqlCommand CreateCommand()
        {
            return new MySqlCommand(connection: _connection, transaction: _transaction);
        }

        public MySqlConnection GetConnection()
        {
            return _connection;
        }

        public void BeginTransaction()
        {
            if (_transaction == null && _connection.State == ConnectionState.Open)
            {
                _transaction = _connection.BeginTransaction();
            }
            else
            {
                LogService.Warn("[警告] 获取MySQL链接事物失败.");
            }
        }

        public void Commit()
        {
            if (_transaction != null)
            {
                _transaction.Commit();
            }
        }

        public void RollBack()
        {
            if (_transaction == null)
            {
                _transaction.Rollback();
            }
        }

        public void Dispose()
        {
            if (_connection != null)
            {
                _connection.Close();
                _connection.Dispose();
                if (_openedByUs)
                {
                    _openedByUs = false;
                    StorageTiming.ConnClosed();
                }
            }
        }
    }
}