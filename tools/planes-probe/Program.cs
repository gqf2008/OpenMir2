// G11 跨服事实探针。见 README.md；结论落在 docs/g11-planes-design-20261108.md §5.1/§5.2。
using System.Net;
using System.Net.Sockets;
using System.Text;
using TouchSocket.Core;
using TouchSocket.Sockets;
using TcpClient = TouchSocket.Sockets.TcpClient;

internal sealed class Slot
{
    public Socket Socket;
    public string SocData = "";
    public string SocketId;
}

internal static class Probe
{
    private const int Port = 17780;
    private static readonly Slot[] srvArray = new Slot[10];

    private static async Task Main(string[] args)
    {
        var mode = args.Length > 0 ? args[0] : "ids";
        switch (mode)
        {
            case "ids": await Ids(); break;
            case "slots": await Slots(); break;
            case "listen": await Listen(); break;
            case "cross": await Cross(); break;
            default:
                Console.Error.WriteLine("用法: dotnet run -c Release -- [ids|slots|listen]");
                Environment.ExitCode = 2;
                break;
        }
    }

    /// SocketClient.Id 的分配：全局递增、断开不复用。
    private static async Task Ids()
    {
        var svc = new TcpService();
        svc.Connected += (c, e) =>
        {
            Console.WriteLine($"CONNECTED id={((SocketClient)c).Id}");
            return Task.CompletedTask;
        };
        svc.Disconnected += (c, e) =>
        {
            Console.WriteLine($"DISCONNECTED id={((SocketClient)c).Id}");
            return Task.CompletedTask;
        };
        svc.Received += (c, e) =>
        {
            var sc = (SocketClient)c;
            var s = Encoding.ASCII.GetString(e.ByteBlock.Buffer, 0, e.ByteBlock.Len);
            Console.WriteLine($"RECV id={sc.Id} data={s}");
            return Task.CompletedTask;
        };
        await Start(svc, Port);
        for (var k = 0; k < 4; k++)
        {
            await Ping($"client{k}");
        }
        await svc.StopAsync();
    }

    /// 复刻 oracle `PlanesServer` 的 srvArray 逻辑：Connecting 填空槽、Received 用 Id-1、Disconnected 按 SocketId 清。
    private static async Task Slots()
    {
        var svc = new TcpService();
        svc.Connected += (c, e) =>
        {
            var sc = (SocketClient)c;
            for (var i = 0; i < srvArray.Length; i++)
            {
                if (srvArray[i] == null)
                {
                    srvArray[i] = new Slot { Socket = sc.MainSocket, SocData = "", SocketId = sc.Id };
                    Console.WriteLine($"CONNECTED id={sc.Id} -> slot{i}");
                    break;
                }
            }
            return Task.CompletedTask;
        };
        svc.Disconnected += (c, e) =>
        {
            var sc = (SocketClient)c;
            for (var i = 0; i < srvArray.Length; i++)
            {
                if (srvArray[i] != null && srvArray[i].SocketId == sc.Id)
                {
                    srvArray[i] = null;
                    Console.WriteLine($"DISCONNECTED id={sc.Id} -> cleared slot{i}");
                    break;
                }
            }
            return Task.CompletedTask;
        };
        svc.Received += (c, e) =>
        {
            var sc = (SocketClient)c;
            var s = Encoding.ASCII.GetString(e.ByteBlock.Buffer, 0, e.ByteBlock.Len);
            try
            {
                var serverInfo = srvArray[int.Parse(sc.Id) - 1];
                serverInfo.SocData += s;
                Console.WriteLine($"RECV id={sc.Id} -> slot{int.Parse(sc.Id) - 1} SOC={serverInfo.SocData}");
            }
            catch (Exception ex)
            {
                Console.WriteLine($"RECV id={sc.Id} THREW {ex.GetType().Name}");
            }
            return Task.CompletedTask;
        };
        await Start(svc, Port);
        for (var k = 0; k < 3; k++)
        {
            await Ping($"c{k}");
            Console.WriteLine($"-- slots: {string.Join(",", srvArray.Select(x => x == null ? "-" : x.SocketId))}");
        }
        await svc.StopAsync();
    }

    /// 跨槽形态：id-1 指向的槽被**另一个**连接占用时，数据会追加到别人的缓冲。
    /// 序列：连 A(id1→slot0) B(id2→slot1) C(id3→slot2) → 断 A → 连 D(id4→slot0) E(id5→slot3)
    /// → D 发送 ⇒ `srvArray[4-1]` = slot3（属于 E）⇒ APPEND。
    private static async Task Cross()
    {
        var svc = new TcpService();
        svc.Connected += (c, e) =>
        {
            var sc = (SocketClient)c;
            for (var i = 0; i < srvArray.Length; i++)
            {
                if (srvArray[i] == null)
                {
                    srvArray[i] = new Slot { Socket = sc.MainSocket, SocData = "", SocketId = sc.Id };
                    Console.WriteLine($"CONNECTED id={sc.Id} -> slot{i}");
                    break;
                }
            }
            return Task.CompletedTask;
        };
        svc.Disconnected += (c, e) =>
        {
            var sc = (SocketClient)c;
            for (var i = 0; i < srvArray.Length; i++)
            {
                if (srvArray[i] != null && srvArray[i].SocketId == sc.Id)
                {
                    srvArray[i] = null;
                    Console.WriteLine($"DISCONNECTED id={sc.Id} -> cleared slot{i}");
                    break;
                }
            }
            return Task.CompletedTask;
        };
        svc.Received += (c, e) =>
        {
            var sc = (SocketClient)c;
            var s = Encoding.ASCII.GetString(e.ByteBlock.Buffer, 0, e.ByteBlock.Len);
            try
            {
                var idx = int.Parse(sc.Id) - 1;
                var serverInfo = srvArray[idx];
                serverInfo.SocData += s;
                Console.WriteLine($"RECV id={sc.Id} -> slot{idx} owner={serverInfo.SocketId} SOC={serverInfo.SocData}");
            }
            catch (Exception ex)
            {
                Console.WriteLine($"RECV id={sc.Id} THREW {ex.GetType().Name}");
            }
            return Task.CompletedTask;
        };
        await Start(svc, Port);

        var a = await ConnectClient();          // id1 -> slot0
        var b = await ConnectClient();          // id2 -> slot1
        var c3 = await ConnectClient();         // id3 -> slot2
        a.Close();
        await Task.Delay(300);
        var d = await ConnectClient();          // id4 -> slot0（第一个空槽）
        var e5 = await ConnectClient();         // id5 -> slot3
        await Task.Delay(200);
        d.Send("(1/2/from4)");
        await Task.Delay(300);
        Console.WriteLine($"-- slots: {string.Join(",", srvArray.Select(x => x == null ? "-" : x.SocketId))}");

        b.Close(); c3.Close(); d.Close(); e5.Close();
        await Task.Delay(200);
        await svc.StopAsync();
    }

    private static async Task<TcpClient> ConnectClient()
    {
        var cli = new TcpClient();
        var cfg = new TouchSocketConfig();
        cfg.SetRemoteIPHost(new IPHost(IPAddress.Parse("127.0.0.1"), Port));
        cli.Setup(cfg);
        await cli.ConnectAsync();
        await Task.Delay(150);
        return cli;
    }

    /// 只 Setup vs Setup+Start：oracle 的 `StartPlanesServer()` 属于前者。
    private static async Task Listen()
    {
        var svc = new TcpService();
        svc.Connected += (c, e) =>
        {
            Console.WriteLine($"CONNECTED id={((SocketClient)c).Id}");
            return Task.CompletedTask;
        };
        var cfg = new TouchSocketConfig();
        cfg.SetListenIPHosts(new[] { new IPHost(IPAddress.Parse("127.0.0.1"), Port) });
        svc.Setup(cfg);
        Console.WriteLine($"after Setup: ServerState={svc.ServerState}");
        await Task.Delay(300);
        await TryConnect("Setup 之后");
        await svc.StartAsync();
        Console.WriteLine($"after Start: ServerState={svc.ServerState}");
        await Task.Delay(300);
        await TryConnect("Start 之后");
        await svc.StopAsync();
    }

    private static async Task Start(TcpService svc, int port)
    {
        var cfg = new TouchSocketConfig();
        cfg.SetListenIPHosts(new[] { new IPHost(IPAddress.Parse("127.0.0.1"), port) });
        svc.Setup(cfg);
        await svc.StartAsync();
        Console.WriteLine("server started");
    }

    private static async Task Ping(string tag)
    {
        var cli = new TcpClient();
        var cfg = new TouchSocketConfig();
        cfg.SetRemoteIPHost(new IPHost(IPAddress.Parse("127.0.0.1"), Port));
        cli.Setup(cfg);
        await cli.ConnectAsync();
        cli.Send($"(1/2/{tag})");
        await Task.Delay(200);
        cli.Close();
        await Task.Delay(200);
    }

    private static async Task TryConnect(string when)
    {
        var cli = new TcpClient();
        var cfg = new TouchSocketConfig();
        cfg.SetRemoteIPHost(new IPHost(IPAddress.Parse("127.0.0.1"), Port));
        cli.Setup(cfg);
        try
        {
            await cli.ConnectAsync(1000);
            Console.WriteLine($"{when}: client connected");
            cli.Close();
        }
        catch (Exception ex)
        {
            Console.WriteLine($"{when}: client THREW {ex.GetType().Name}");
        }
    }
}
