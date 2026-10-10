using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;
using OpenMir2;
using OpenMir2.Hosts;
using Serilog;
using System.Threading;
using System.Threading.Tasks;

namespace BotSrv
{
    public class AppServer : ServiceHost
    {
        // 注：上游 ServiceHost 改版后不再内置 HostBuilder（Builder/Host 属性已移除），
        // 这里由 AppServer 自己持有 IHost，并按各网关同样的方式装配 Serilog
        //（LogService.Logger 必须先赋值，否则 AppService 构造里第一行日志就 NRE）。
        // 2026-10-10 C 线修复。
        private IHost _host;

        public AppServer()
        {
        }

        public override void Initialize()
        {

        }

        public void ConfigureServices(IServiceCollection services)
        {
            services.Configure<RobotOptions>(Configuration.GetSection("BotPlay"));
            services.AddSingleton<ClientManager>();
            services.AddHostedService<AppService>();
        }

        private void ConfigureLogging(ILoggingBuilder logging)
        {
            logging.ClearProviders();
            logging.SetMinimumLevel(LogLevel.Debug);
            logging.AddSerilog(Log.Logger, dispose: false);
        }

        public override async Task StartAsync(CancellationToken cancellationToken)
        {
            Log.Logger = new LoggerConfiguration()
                .MinimumLevel.Debug()
                .WriteTo.Console(
                    outputTemplate: "{Timestamp:HH:mm:ss.fff} [{Level:u3}] {Message:lj}{NewLine}{Exception}")
                .CreateLogger();
            LogService.Logger = Log.Logger;

            _host = Host.CreateDefaultBuilder()
                .ConfigureLogging(ConfigureLogging)
                .ConfigureServices(ConfigureServices)
                .Build();
            await _host.StartAsync(cancellationToken);
            // 常驻：压测驱动脚本通过杀进程结束
            await _host.WaitForShutdownAsync(cancellationToken);
        }

        public override async Task StopAsync(CancellationToken cancellationToken)
        {
            if (_host != null)
            {
                await _host.StopAsync(cancellationToken);
            }
        }
    }
}
