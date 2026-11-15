using Serilog;
using System;

namespace OpenMir2
{
    public static class LogService
    {
        public static ILogger Logger;

        public static void Info(string message)
        {
            Logger.Information(message);
        }

        public static void Info(string message, params object[] args)
        {
            Logger.Information(message, args);
        }

        public static void Error(string message)
        {
            Logger.Error(message);
        }

        public static void Error(string message, Exception ex)
        {
            Logger.Error(ex, message);
        }

        /// <summary>
        /// 单参异常重载：**必须**写成 `Logger.Error(ex, message)`（真正带异常的重载）。
        /// Serilog 的重载陷阱：`Logger.Error(someMessage, ex)` 会被泛型重载
        /// `Error&lt;T&gt;(string, T)` 静默接走，`LogEvent.Exception` 为 null，
        /// 输出模板里的 `{Exception}` 就渲染为空——**只丢类型与堆栈，不报任何错**。
        /// 2026-11-15 全仓审计（`src/**/*.cs`，排除注释行）：`LogService.Error(` 调用
        /// 共 393 处，其中单参传 Exception 的 42 处曾全部走坏重载（BotSrv 1 / DBSrv 4 /
        /// GameSrv 7 / LoginGate 1 / LoginSrv 18 / M2Server 3 / MapSrv 1 / Modules 3），
        /// 坏序只有本文件原 :32 一处；本文件 :27 的 `Error(ex, message)` 与
        /// `LogDataSrv/AppService.cs:84` 是正确序。新增调用一律用 `Error(message, ex)`
        /// 形式或本重载，禁止把消息写在异常前面。
        /// </summary>
        public static void Error(Exception ex)
        {
            // 正确序：exception 在前、messageTemplate 在后。写成 Logger.Error(ex.Message, ex)
            // 会被 Serilog 当成 (template, propertyValue) ⇒ LogEvent.Exception=null。
            Logger.Error(ex, ex.Message);
        }

        public static void Debug(string message)
        {
            Logger.Debug(message);
        }

        public static void Debug(string message, params object[] args)
        {
            Logger.Debug(message, args);
        }

        public static void Warn(string message)
        {
            Logger.Warning(message);
        }

        public static void Fatal(string message)
        {
            Logger.Fatal(message);
        }
    }
}