using System.Runtime;
using System;
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
            // M3⑤（T-29 家族扩展）：商店夹具 op/分支/价格纯函数的无栈自检（不启动服务/不连端口）。
            if (args.Length > 0 && args[0] == "--selftest-shopseq")
            {
                System.Environment.Exit(ShopSeqSelfCheck.Run());
            }
            // M3⑧（T-46 家族扩展）：行会/攻城夹具 步解析/角色判定/分支归类 纯函数的无栈自检（不启动服务/不连端口）。
            if (args.Length > 0 && args[0] == "--selftest-guildcastle")
            {
                System.Environment.Exit(GuildCastleSeqSelfCheck.Run());
            }
            // M3⑨（T-45 家族扩展）：英雄/宠物夹具 步解析/角色判定/分支归类 纯函数的无栈自检（不启动服务/不连端口）。
            if (args.Length > 0 && args[0] == "--selftest-heropet")
            {
                System.Environment.Exit(HeroPetSeqSelfCheck.Run());
            }
            // M3⑥（T-41 家族扩展）：任务夹具 步解析/分支归类/说词抽取 纯函数的无栈自检（不启动服务/不连端口）。
            if (args.Length > 0 && args[0] == "--selftest-questseq")
            {
                System.Environment.Exit(QuestSeqSelfCheck.Run());
            }
            // M3④（T-22/T-29 家族扩展）：掉落/经验夹具 attacker 选择/击杀归类/字段抽取/行组装 纯函数的无栈自检（不启动服务/不连端口）。
            if (args.Length > 0 && args[0] == "--selftest-dropseq")
            {
                System.Environment.Exit(DropExpSeqSelfCheck.Run());
            }
            // M3⑦（T-43 家族扩展）：组队/交易夹具 步解析/角色判定/分支归类 纯函数的无栈自检（不启动服务/不连端口）。
            if (args.Length > 0 && args[0] == "--selftest-pairtrade")
            {
                System.Environment.Exit(PairTradeSeqSelfCheck.Run());
            }
            // M3⑩（T-44 家族扩展）：元宝寄售夹具 步解析/角色判定/分支归类 纯函数的无栈自检（不启动服务/不连端口）。
            if (args.Length > 0 && args[0] == "--selftest-selloff")
            {
                System.Environment.Exit(SellOffSeqSelfCheck.Run());
            }
            // M3 RNG 场景驱动器（T-59 家族扩展）：固定动作序列解析 / 固定怪 key / 场景描述件行组装
            // 纯函数的无栈自检（不启动服务/不连端口）。
            if (args.Length > 0 && args[0] == "--selftest-sceneseq")
            {
                System.Environment.Exit(SceneSeqSelfCheck.Run());
            }
            // 压测动作探针坐标口径（卡 `openmir2-botsrv-action-probe-turn-coords`）：服务端驱动位移后的
            // 探针坐标跟随 + 重入落点刷新（不启动服务/不连端口）。
            if (args.Length > 0 && args[0] == "--selftest-probecoords")
            {
                System.Environment.Exit(ProbeCoordsSelfCheck.Run());
            }
            // 场景空 Actor 健壮性（卡 `openmir2-botsrv-autoplay-nre`）：null 槽位跳过 + MySelf 快照
            // + 未知 actor 帧的显式跳过（照抄冻结 Delphi 客户端 PlayScn.pas:3670-3690 语义）。
            if (args.Length > 0 && args[0] == "--selftest-scene-null")
            {
                System.Environment.Exit(SceneGuardSelfCheck.Run());
            }
            // 卡 `openmir2-botsrv-recv-thread-nre`：错误类别普查的逐类牙（不启动服务/不连端口）。
            if (args.Length > 0 && args[0] == "--selftest-error-census")
            {
                System.Environment.Exit(ErrorCensusSelfCheck.Run());
            }
            // 卡 `openmir2-botsrv-decode-compat`：Ability 52 / ClientItem 621 解码兼容的牙
            // （不启动服务/不连端口；含静默全零拒绝与旧分派计数负例）。
            if (args.Length > 0 && args[0] == "--selftest-decode-compat")
            {
                System.Environment.Exit(DecodeCompatSelfCheck.Run());
            }
            // 卡 `openmir2-botsrv-shared-state-locks`：跨 bot 共享态（MySelf 线程本地 /
            // ActorList 细粒度锁）的牙（不启动服务/不连端口）。
            if (args.Length > 0 && args[0] == "--selftest-shared-state")
            {
                System.Environment.Exit(SharedStateSelfCheck.Run());
            }

            // 卡 `openmir2-botsrv-loginconns-buffer`：登录连接明细落盘复用缓冲 + 输出逐字节等价的牙
            // （不启动服务/不连端口；含旧实现复刻 diff=0、golden SHA256、缩容截断、单次分配量）。
            if (args.Length > 0 && args[0] == "--selftest-loadmetrics")
            {
                System.Environment.Exit(LoadMetricsSelfCheck.Run());
            }
            // 卡 `openmir2-botsrv-recv-null-residual`：软关后残留排队帧在 `MShare.MySelf == null`
            // 上下文里的确定性离屏复现（不启动服务/不连端口；红→绿牙 + 非 null 路径等价牙）。
            if (args.Length > 0 && args[0] == "--selftest-recv-null-context")
            {
                System.Environment.Exit(RecvNullContextSelfCheck.Run());
            }
            // 进程级兜底：线程入口/事件处理器之外的遗漏路径（例如 runThread 之外的线程）至少在
            // fatal.ndjson 留一行，并计入 unhandled_exception；未观察 Task 同理。
            AppDomain.CurrentDomain.UnhandledException += (sender, e) =>
                BotErrorCensus.OnUnhandledException(e.ExceptionObject as System.Exception);
            TaskScheduler.UnobservedTaskException += (sender, e) =>
            {
                BotErrorCensus.OnUnobservedTaskException(e.Exception);
                e.SetObserved();
            };
            GCSettings.LargeObjectHeapCompactionMode = GCLargeObjectHeapCompactionMode.CompactOnce;
            GCSettings.LatencyMode = GCSettings.IsServerGC ? GCLatencyMode.Batch : GCLatencyMode.Interactive;
            AppServer serviceRunner = new AppServer();
            await serviceRunner.StartAsync(CancellationToken.None);
        }
    }
}