namespace DBSrv
{
    internal class Program
    {
        private static async Task Main(string[] args)
        {
            Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
            AppServer serviceRunner = new AppServer();
            CancellationTokenSource cts = new CancellationTokenSource();
            cts.Token.Register(() => _ = serviceRunner.StopAsync(cts.Token));
            // 监听 Ctrl+C 事件
            Console.CancelKeyPress += (sender, e) =>
            {
                Console.WriteLine("Ctrl+C pressed");
                cts.Cancel();
                // 阻止其他处理程序处理此事件，以及默认的操作（终止程序）
                e.Cancel = true;
            };
            try
            {
                await serviceRunner.StartAsync(cts.Token);
            }
            finally
            {
                // **关服终值**（卡 `openmir2-entry-load-reply-buckets`）：无条件打印一次，**不参与去重**。
                // 去重只能作用于周期心跳；终值被吞掉会让整轮只剩百位快照。
                LogService.Info(Services.Impl.EntryTelemetry.FinalLine());
            }
        }
    }
}