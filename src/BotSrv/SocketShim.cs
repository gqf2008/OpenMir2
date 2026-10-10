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

        public IPEndPoint RemoteEndPoint { get; set; }
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
                OnError?.Invoke(this, new DSCClientErrorEventArgs(ex.SocketErrorCode));
                return;
            }
            _connected = true;
            _recvThread = new Thread(RecvLoop) { IsBackground = true, Name = "ScoketClient-recv" };
            _recvThread.Start();
            OnConnected?.Invoke(this, DSCClientConnectedEventArgs.Instance);
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
                    OnReceivedData?.Invoke(this, new DSCClientDataInEventArgs(data, n));
                }
            }
            catch (SocketException ex)
            {
                if (_connected) OnError?.Invoke(this, new DSCClientErrorEventArgs(ex.SocketErrorCode));
            }
            catch (ObjectDisposedException) { /* Disconnect 主动关 */ }
            _connected = false;
            OnDisconnected?.Invoke(this, DSCClientConnectedEventArgs.Instance);
        }

        public void SendText(string text)
        {
            if (!_connected) return;
            byte[] data = HUtil32.GetBytes(text);
            try { _socket.Send(data); }
            catch (SocketException ex) { OnError?.Invoke(this, new DSCClientErrorEventArgs(ex.SocketErrorCode)); }
        }

        public void SendBuffer(byte[] data)
        {
            if (!_connected) return;
            try { _socket.Send(data); }
            catch (SocketException ex) { OnError?.Invoke(this, new DSCClientErrorEventArgs(ex.SocketErrorCode)); }
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
