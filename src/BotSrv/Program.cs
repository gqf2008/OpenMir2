using System.Runtime;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

namespace BotSrv
{
    internal class Program
    {
        private static async Task Main(string[] args)
        {
            Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
            // M-2（reviewer-5 76e433f0）：attacker 精确选择的无栈自检入口（不启动服务/不连端口）。
            if (args.Length > 0 && args[0] == "--selftest-attackrole")
            {
                System.Environment.Exit(AttackRoleSelfCheck.Run());
            }
            GCSettings.LargeObjectHeapCompactionMode = GCLargeObjectHeapCompactionMode.CompactOnce;
            GCSettings.LatencyMode = GCSettings.IsServerGC ? GCLatencyMode.Batch : GCLatencyMode.Interactive;
            AppServer serviceRunner = new AppServer();
            await serviceRunner.StartAsync(CancellationToken.None);
        }
    }
}