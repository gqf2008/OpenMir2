using System;
using System.Net;
using System.Net.Sockets;
using System.Threading;
using OpenMir2;

namespace BotSrv
{
    // ==== BotSrv 修复 shim（C 线工具修复，2026-10-10）====
    // 上游「调整项目结构」(44c33e9b) 后 OpenMir2 库里的 ScoketClient / DSCClient*EventArgs
    // 被移除，BotSrv 三个文件（Player/RobotPlayer.cs、Scenes/Scene/LoginScene.cs、
    // Scenes/Scene/SelectChrScene.cs）仍在引用，导致 BotSrv 无法编译。
    // 这里按原 API 表面用 System.Net.Sockets.Socket 重写（每连接一条后台接收线程，
    // 原始字节块原样上抛——组帧由上层 RobotPlayer.ProcessPacket 负责）。

    public sealed class DSCClientConnectedEventArgs : EventArgs
    {
        public static readonly DSCClientConnectedEventArgs Instance = new DSCClientConnectedEventArgs();
    }

    public sealed class DSCClientErrorEventArgs : EventArgs
    {
        public SocketError ErrorCode { get; }
        public DSCClientErrorEventArgs(SocketError errorCode) { ErrorCode = errorCode; }
    }

    public sealed class DSCClientDataInEventArgs : EventArgs
    {
        public byte[] Buff { get; }
        public int BuffLen { get; }
        public DSCClientDataInEventArgs(byte[] buff, int buffLen) { Buff = buff; BuffLen = buffLen; }
    }

    public sealed class ScoketClient : IDisposable
    {
        private Socket _socket;
        private Thread _recvThread;
        private volatile bool _connected;
        private static int _connSeqSeed;

        /// <summary>T-11：本进程内第几条登录连接（单调递增）——用来把「同一条 TCP 连接」的观测归到一起。</summary>
        public int ConnSeq { get; private set; }

        public IPEndPoint RemoteEndPoint { get; set; }

        /// <summary>卡 openmir2-sm-logon-cross-session-delivery：本连接的本端端点
        /// （与网关 accept 的 peer 一一对应，用作三方对齐键）。连接前为 null。</summary>
        public IPEndPoint LocalEndPoint { get; private set; }
        public bool IsConnected { get => _connected; set => _connected = value; }

        public event EventHandler<DSCClientConnectedEventArgs> OnConnected;
        public event EventHandler<DSCClientConnectedEventArgs> OnDisconnected;
        public event EventHandler<DSCClientDataInEventArgs> OnReceivedData;
        public event EventHandler<DSCClientErrorEventArgs> OnError;

        public void Connect(string host, int port)
        {
            RemoteEndPoint = new IPEndPoint(IPAddress.Parse(host), port);
            Connect();
        }

        public void Connect()
        {
            if (_connected) return;
            _socket = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp) { NoDelay = true };
            try
            {
                _socket.Connect(RemoteEndPoint);
            }
            catch (SocketException ex)
            {
                BotErrorCensus.SocketExceptionSeen("connect", ex);
                // 复审 minor：Connect 阶段的 OnError 处理器也走统一护栏，
                // 避免它是 7 处事件调用点里唯一未归类的路径。
                InvokeHandlerGuarded(() => OnError?.Invoke(this, new DSCClientErrorEventArgs(ex.SocketErrorCode)));
                return;
            }
            ConnSeq = Interlocked.Increment(ref _connSeqSeed);
            LocalEndPoint = _socket.LocalEndPoint as IPEndPoint;
            _connected = true;
            _recvThread = new Thread(RecvLoop) { IsBackground = true, Name = "ScoketClient-recv" };
            _recvThread.Start();
            InvokeHandlerGuarded(() => OnConnected?.Invoke(this, DSCClientConnectedEventArgs.Instance));
        }

        private void RecvLoop()
        {
            var buf = new byte[65536];
            try
            {
                while (_connected)
                {
                    int n = _socket.Receive(buf);
                    if (n <= 0) break;
                    var data = new byte[n];
                    Buffer.BlockCopy(buf, 0, data, 0, n);
                    // 收包线程是 fire-and-forget 边界：处理器异常以前会**静默打死整条线程**（.NET 的
                    // 未处理线程异常会终止进程）。现在计数为 socket_handler_exception 并继续收包。
                    InvokeHandlerGuarded(() => OnReceivedData?.Invoke(this, new DSCClientDataInEventArgs(data, n)));
                }
            }
            catch (SocketException ex)
            {
                BotErrorCensus.SocketExceptionSeen("recv", ex);
                if (_connected) InvokeHandlerGuarded(() => OnError?.Invoke(this, new DSCClientErrorEventArgs(ex.SocketErrorCode)));
            }
            catch (ObjectDisposedException)
            {
                // Disconnect 主动关：运行期竞态，单独观测（不是故障）。
                BotErrorCensus.TeardownRaceSeen();
            }
            catch (Exception ex)
            {
                // 兜底：任何非 socket 异常都不得静默杀死收包线程。
                BotErrorCensus.Count(BotErrorCensus.SocketRecvFault);
                LoadMetrics.WriteFatalMarker("socket_recv_unexpected", ex);
            }
            _connected = false;
            InvokeHandlerGuarded(() => OnDisconnected?.Invoke(this, DSCClientConnectedEventArgs.Instance));
        }

        /// <summary>socket 事件处理器的统一护栏（自检注入本方法证明 +1）。</summary>
        internal static void InvokeHandlerGuarded(Action invoke)
        {
            try
            {
                invoke();
            }
            catch (Exception ex)
            {
                LoadMetrics.InternalError(BotErrorCensus.SocketHandlerException);
                if (LoadMetrics.ShouldLogInternalError())
                {
                    BotErrorCensus.SafeLog(() => LogService.Warn("socket 事件处理器异常被兜住：" + ex));
                }
            }
        }

        /// <summary>返回 true = socket.Send 未抛异常（小包在已连接 TCP 上写成功 ⇒ 已交给本机内核 / 对端可靠可达）。</summary>
        public bool SendText(string text)
        {
            if (!_connected) return false;
            byte[] data = HUtil32.GetBytes(text);
            try { _socket.Send(data); return true; }
            catch (SocketException ex)
            {
                BotErrorCensus.SocketExceptionSeen("send", ex);
                InvokeHandlerGuarded(() => OnError?.Invoke(this, new DSCClientErrorEventArgs(ex.SocketErrorCode)));
                return false;
            }
            catch (ObjectDisposedException)
            {
                BotErrorCensus.TeardownRaceSeen();
                return false;
            }
        }

        public void SendBuffer(byte[] data)
        {
            if (!_connected) return;
            try { _socket.Send(data); }
            catch (SocketException ex)
            {
                BotErrorCensus.SocketExceptionSeen("send", ex);
                InvokeHandlerGuarded(() => OnError?.Invoke(this, new DSCClientErrorEventArgs(ex.SocketErrorCode)));
            }
            catch (ObjectDisposedException)
            {
                BotErrorCensus.TeardownRaceSeen();
            }
        }

        public void Disconnect()
        {
            _connected = false;
            try { _socket?.Shutdown(SocketShutdown.Both); } catch { /* 已关 */ }
            try { _socket?.Close(); } catch { /* 已关 */ }
        }

        public void Dispose() => Disconnect();
    }
}
