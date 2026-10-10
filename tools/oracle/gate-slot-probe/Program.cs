using System.Net;
using System.Net.Sockets;
using System.Reflection;
using System.Text;
using M2Server;
using M2Server.Net;
using M2Server.Net.TCP;
using OpenMir2;
using OpenMir2.Packets.ServerPackets;
using Serilog;
using SystemModule;

namespace GateSlotProbe;

/// <summary>
/// S1 缺陷① 守护探针：GameSvr 的网关槽位在「网关断开 → 重连」之后必须仍然指向**活着的那条连接**。
///
/// 操作序列（改前/改后跑同一份）：
///   ① 网关连接#1 → GM_OPEN(玩家A)     → 世界侧 AddGateBuffer(0,…) 必须到达连接#1
///   ② 断开连接#1（模拟网关抖动）
///   ③ 网关连接#2 → GM_OPEN(玩家B)     → 世界侧 AddGateBuffer(0,…) 必须到达连接#2
///   ④ 世界侧 SetGateUserList(0, 玩家B) → 必须不抛异常，且槽位 0 的 UserList 绑定到玩家B
///
/// 判据说明：世界侧一律按 playObject.GateIdx（= 网关自己声明的 ServiceId）寻址，
/// 而 GameSvr 的槽位数组按 TCP accept 序分配 —— 重连后两者若不重合，
/// 世界下发被静默丢弃（客户端黑屏）、SetGateUserList 抛 NullReferenceException（进世界死循环）。
/// </summary>
internal static class Program
{
    private const int UserSocketA = 41001;
    private const int UserSocketB = 41002;

    private static readonly List<(string Name, bool Ok, string Detail)> Verdicts = new();
    private static string _oracleDir = @"E:\MirServer\M2GameSvr";

    private static void Check(string name, bool ok, string detail)
    {
        Verdicts.Add((name, ok, detail));
        Console.WriteLine($"{(ok ? "GREEN" : "RED  ")}  {name}  [{detail}]");
    }

    private static async Task<int> Main(string[] args)
    {
        int port = 15000;
        int settle = 700;
        for (int i = 0; i < args.Length; i++)
        {
            if (args[i] == "--port" && i + 1 < args.Length) { port = int.Parse(args[++i]); }
            else if (args[i] == "--settle" && i + 1 < args.Length) { settle = int.Parse(args[++i]); }
            else if (args[i] == "--oracle-dir" && i + 1 < args.Length) { _oracleDir = args[++i]; }
        }

        Console.OutputEncoding = Encoding.UTF8;
        // 与 GameSrv 启动同款：配置/脚本是 GB2312(cp936)，不注册 provider 连 SystemShare 都构造不出来
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        SeedBaseDirectory();
        // LogService.Logger 必须先赋值：SystemShare / M2Share 的静态构造里都会打日志
        LogService.Logger = new LoggerConfiguration().MinimumLevel.Debug()
            .WriteTo.Console(outputTemplate: "    [log {Level:u3}] {Message:lj}{NewLine}")
            .CreateLogger();

        SystemShare.Config.sGateAddr = "127.0.0.1";
        SystemShare.Config.nGatePort = port;
        M2Share.StartReady = true;
        var net = (TCPNetChannel)M2Share.NetChannel;
        net.Initialize();
        await net.Start(CancellationToken.None);
        await Task.Delay(settle);
        Console.WriteLine($"== S1 缺陷① 探针：GameSvr 网关监听 127.0.0.1:{port}；世界侧按 GateIdx=0 寻址");

        Console.WriteLine("== 步骤 1：网关连接#1 + GM_OPEN(玩家A)");
        var link1 = new GateLink();
        await link1.ConnectAsync(port);
        await Task.Delay(settle);
        link1.SendOpen(UserSocketA, 1, "127.0.0.1");
        await Task.Delay(settle);
        DumpSlots(net, "步骤1 后");

        byte[] marker1 = Encoding.ASCII.GetBytes("S1-MARKER-LINK1-0000000000");
        net.AddGateBuffer(0, marker1);
        await Task.Delay(settle);
        Check("连接#1 活着时，世界→网关(槽0) 下发到达", link1.ReceivedContains(marker1), $"收到 {link1.ReceivedLength}B");

        Console.WriteLine("== 步骤 2：断开连接#1（模拟网关连接抖动）");
        link1.Dispose();
        await Task.Delay(settle);
        DumpSlots(net, "步骤2 后");

        Console.WriteLine("== 步骤 3：网关连接#2 + GM_OPEN(玩家B)");
        var link2 = new GateLink();
        await link2.ConnectAsync(port);
        await Task.Delay(settle);
        link2.SendOpen(UserSocketB, 2, "127.0.0.1");
        await Task.Delay(settle);
        DumpSlots(net, "步骤3 后");

        byte[] marker2 = Encoding.ASCII.GetBytes("S1-MARKER-LINK2-1111111111");
        net.AddGateBuffer(0, marker2);
        await Task.Delay(settle);
        Check("网关重连后，世界→网关(槽0) 下发到达连接#2", link2.ReceivedContains(marker2), $"收到 {link2.ReceivedLength}B");

        Console.WriteLine("== 步骤 4：世界侧 SetGateUserList(0, 玩家B)（进世界路径）");
        bool ok4;
        string detail4;
        try
        {
            net.SetGateUserList(0, UserSocketB, null);
            ok4 = true;
            detail4 = "未抛异常";
        }
        catch (Exception ex)
        {
            ok4 = false;
            detail4 = ex.GetType().Name + ": " + ex.Message;
        }
        Check("SetGateUserList(0,…) 不抛异常（WorldServer.ProcessHumans 每 tick 都调它）", ok4, detail4);

        SlotState slot0 = InspectSlot(net, 0);
        bool bound = slot0.HandlerPresent && slot0.BoUsed && slot0.SocketPresent && slot0.UserListPresent
                     && slot0.UserSockets.Contains(UserSocketB);
        Check("槽位0 仍绑定活连接且 UserList 里有玩家B", bound, slot0.Describe());

        Console.WriteLine();
        DumpSlots(net, "结束");

        Console.WriteLine();
        int red = Verdicts.Count(v => !v.Ok);
        Console.WriteLine($"==== GateSlotProbe: {Verdicts.Count - red} GREEN / {red} RED ====");
        link2.Dispose();
        try { await net.StopAsync(CancellationToken.None); } catch { }
        return red == 0 ? 0 : 1;
    }

    /// <summary>
    /// 把 oracle 部署目录里的配置文件铺到本进程 BaseDirectory：
    /// M2Share / SystemShare 的静态构造会从 AppContext.BaseDirectory 读 *.conf，
    /// 探针不能依赖「恰好有个完整部署目录当工作目录」。
    /// </summary>
    private static void SeedBaseDirectory()
    {
        string baseDir = AppContext.BaseDirectory;
        File.WriteAllText(Path.Combine(baseDir, "!runaddr.txt"), "127.0.0.1" + Environment.NewLine);
        if (!Directory.Exists(_oracleDir))
        {
            Console.WriteLine($"    [warn] oracle 目录不存在，跳过配置铺设: {_oracleDir}");
            return;
        }
        foreach (string pattern in new[] { "*.conf", "*.txt", "*.ini" })
        {
            foreach (string src in Directory.GetFiles(_oracleDir, pattern, SearchOption.TopDirectoryOnly))
            {
                try { File.Copy(src, Path.Combine(baseDir, Path.GetFileName(src)), true); } catch { }
            }
        }
    }

    // ---- 反射探针：只读地看槽位内部状态，作为「改了哪一点」的原始证据 ----

    private sealed record SlotState(
        int Index, bool HandlerPresent, bool BoUsed, bool SocketPresent, bool UserListPresent,
        string SocketId, IReadOnlyList<int> UserSockets)
    {
        public string Describe()
        {
            if (!HandlerPresent) { return "handler=null"; }
            string socket = SocketPresent ? "有" : "null";
            string userList = UserListPresent ? "有" : "null";
            return $"handler=有 BoUsed={BoUsed} Socket={socket} UserList={userList} SocketId={SocketId} users=[{string.Join(",", UserSockets)}]";
        }
    }

    private static SlotState InspectSlot(TCPNetChannel net, int index)
    {
        FieldInfo gatesField = typeof(TCPNetChannel).GetField("_gameGates", BindingFlags.NonPublic | BindingFlags.Instance)!;
        Array gates = (Array)gatesField.GetValue(net)!;
        object handler = index < gates.Length ? gates.GetValue(index) : null;
        if (handler == null)
        {
            return new SlotState(index, false, false, false, false, "", Array.Empty<int>());
        }
        var gate = (ChannelGate)handler.GetType().GetProperty("GateInfo")!.GetValue(handler)!;
        var users = new List<int>();
        if (gate.UserList != null)
        {
            foreach (SessionUser u in gate.UserList)
            {
                if (u != null) { users.Add(u.Socket); }
            }
        }
        return new SlotState(index, true, gate.BoUsed, gate.Socket != null, gate.UserList != null, gate.SocketId ?? "", users);
    }

    private static void DumpSlots(TCPNetChannel net, string when)
    {
        for (int i = 0; i < 20; i++)
        {
            SlotState s = InspectSlot(net, i);
            if (s.HandlerPresent)
            {
                Console.WriteLine($"    slot[{i}] {s.Describe()}   <- {when}");
            }
        }
    }

    /// <summary>冒充 GameGate 的一条网关连接（只发 GM_OPEN，收世界侧下发）。</summary>
    private sealed class GateLink : IDisposable
    {
        private readonly TcpClient _tcp = new();
        private readonly List<byte> _rx = new();

        public int ReceivedLength { get { lock (_rx) { return _rx.Count; } } }

        public async Task ConnectAsync(int port)
        {
            await _tcp.ConnectAsync(IPAddress.Loopback, port);
            _ = Task.Run(ReadLoopAsync);
        }

        private async Task ReadLoopAsync()
        {
            byte[] buf = new byte[8192];
            try
            {
                while (true)
                {
                    int n = await _tcp.GetStream().ReadAsync(buf);
                    if (n <= 0) { break; }
                    lock (_rx) { _rx.AddRange(buf.AsSpan(0, n).ToArray()); }
                }
            }
            catch
            {
                // 连接被关闭：正常收尾
            }
        }

        public bool ReceivedContains(byte[] needle)
        {
            lock (_rx)
            {
                for (int i = 0; i + needle.Length <= _rx.Count; i++)
                {
                    bool hit = true;
                    for (int j = 0; j < needle.Length; j++)
                    {
                        if (_rx[i + j] != needle[j]) { hit = false; break; }
                    }
                    if (hit) { return true; }
                }
                return false;
            }
        }

        /// <summary>GM_OPEN：与 GameGate 的 ClientThread.UserEnter 同构（定长头 + IP 字符串体）。</summary>
        public void SendOpen(int userSocket, ushort sessionId, string ipaddr)
        {
            byte[] body = Encoding.ASCII.GetBytes(ipaddr);
            var header = new ServerMessage
            {
                PacketCode = Grobal2.PacketCode,
                Socket = userSocket,
                SessionId = sessionId,
                Ident = Grobal2.GM_OPEN,
                SessionIndex = 0,
                PackLength = body.Length
            };
            byte[] head = SerializerUtil.Serialize(header);
            byte[] frame = new byte[head.Length + body.Length];
            Buffer.BlockCopy(head, 0, frame, 0, head.Length);
            Buffer.BlockCopy(body, 0, frame, head.Length, body.Length);
            NetworkStream stream = _tcp.GetStream();
            stream.Write(frame, 0, frame.Length);
            stream.Flush();
        }

        public void Dispose()
        {
            try { _tcp.Close(); } catch { }
        }
    }
}
