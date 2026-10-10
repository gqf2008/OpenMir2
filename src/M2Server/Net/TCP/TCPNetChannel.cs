using OpenMir2;
using OpenMir2.Common;
using OpenMir2.DataHandlingAdapters;
using OpenMir2.Packets.ClientPackets;
using OpenMir2.Packets.ServerPackets;
using System.Collections.Concurrent;
using System.Net;
using System.Net.Sockets;
using System.Threading.Channels;
using SystemModule;
using SystemModule.Actors;
using SystemModule.Enums;
using TouchSocket.Core;
using TouchSocket.Sockets;

namespace M2Server.Net.TCP
{
    public class TCPNetChannel : INetChannel
    {

        private readonly TcpService tcpService;
        private readonly object RunSocketSection;
        private readonly Channel<ReceiveData> _receiveQueue;
        private readonly ChannelMessageHandler[] _gameGates;
        /// <summary>
        /// 网关连接 SocketId -> 网关槽位下标。
        /// 槽位下标是**世界侧寻址用的**（`playObject.GateIdx` = 网关自己声明的 ServiceId，
        /// 经客户端认证串带回），因此不能按 TCP accept 序绑定：网关断线重连会拿到新的
        /// SocketId，accept 序一变，世界下发的数据就打到已断开的旧槽位上 —— 表现为
        /// 客户端进世界黑屏（下发被静默丢弃）且 `SetGateUserList` 每 tick 抛 NRE。
        /// </summary>
        private readonly ConcurrentDictionary<string, int> _gateSlotByConnection = new ConcurrentDictionary<string, int>();
        /// <summary>
        /// 网关地址白名单
        /// </summary>
        private readonly HashSet<long> Whitelist = new HashSet<long>();
        private CancellationToken _stoppingCancelReads;

        public TCPNetChannel()
        {
            LoadRunAddr();
            _receiveQueue = Channel.CreateUnbounded<ReceiveData>();
            _gameGates = new ChannelMessageHandler[20];
            tcpService = new TcpService();
            tcpService.Connected += Connecting;
            tcpService.Disconnected += Disconnected;
            tcpService.Received += Received;
            RunSocketSection = new object();
            _stoppingCancelReads = new CancellationToken();
        }

        private Task Received(SocketClient socketClient, ReceivedDataEventArgs e)
        {
            if (e.RequestInfo is not DataMessageFixedHeaderRequestInfo gateMessage)
            {
                return Task.CompletedTask;
            }

            // 槽位按"连接"解析，不再用 int.Parse(SocketId) - 1（那是 accept 序，重连后会错位）
            if (!_gateSlotByConnection.TryGetValue(socketClient.Id, out int gateId))
            {
                return Task.CompletedTask;
            }
            _receiveQueue.Writer.TryWrite(new ReceiveData()
            {
                Data = gateMessage.Message,
                Packet = gateMessage.Header,
                GateId = gateId
            });
            return Task.CompletedTask;
        }

        private Task Connecting(IClient client, ConnectedEventArgs e)
        {
            const string sKickGate = "服务器未就绪: {0}";
            const string sGateOpen = "游戏网关[{0}]已打开...";
            SocketClient clientSoc = (SocketClient)client;
            if (M2Share.StartReady)
            {
                if (Whitelist.Contains(HUtil32.IpToInt(clientSoc.ServiceIP)))
                {
                    ChannelGate gateInfo = new ChannelGate();
                    gateInfo.SendMsgCount = 0;
                    gateInfo.SendRemainCount = 0;
                    gateInfo.SendTick = HUtil32.GetTickCount();
                    gateInfo.SendMsgBytes = 0;
                    gateInfo.SendedMsgCount = 0;
                    gateInfo.BoUsed = true;
                    gateInfo.SocketId = clientSoc.Id;
                    gateInfo.Socket = clientSoc.MainSocket;
                    gateInfo.UserList = new List<SessionUser>();
                    gateInfo.UserCount = 0;
                    gateInfo.SendKeepAlive = false;
                    gateInfo.SendChecked = 0;
                    gateInfo.SendBlockCount = 0;
                    int gateIdx;
                    lock (RunSocketSection)
                    {
                        // 复用最小空闲槽位：槽位下标就是世界侧寻址用的网关编号，
                        // 网关重连（含前一条连接刚 CloseGate 释放槽位）必须落回同一个编号。
                        gateIdx = AllocGateSlot();
                        if (gateIdx >= 0)
                        {
                            _gameGates[gateIdx] = new ChannelMessageHandler(gateInfo);
                            _gateSlotByConnection[clientSoc.Id] = gateIdx;
                        }
                    }
                    if (gateIdx < 0)
                    {
                        clientSoc.Close();
                        LogService.Error("超过网关最大链接数量.关闭链接");
                        return Task.CompletedTask;
                    }
                    LogService.Info(string.Format(sGateOpen, clientSoc.MainSocket.RemoteEndPoint));
                }
                else
                {
                    clientSoc.Close();
                    LogService.Warn($"关闭非白名单网关地址链接. IP:{clientSoc.MainSocket.RemoteEndPoint}");
                }
            }
            else
            {
                LogService.Error(string.Format(sKickGate, clientSoc.MainSocket.RemoteEndPoint));
                clientSoc.Close();
            }
            return Task.CompletedTask;
        }

        private Task Disconnected(IClient client, DisconnectEventArgs e)
        {
            SocketClient socSocket = ((SocketClient)client);
            M2Share.NetChannel.CloseGate(socSocket.Id, socSocket.GetIPPort());
            return Task.CompletedTask;
        }

        public void Initialize()
        {
            TouchSocketConfig touchSocketConfig = new TouchSocketConfig();
            touchSocketConfig.SetListenIPHosts(new IPHost(IPAddress.Parse(SystemShare.Config.sGateAddr), SystemShare.Config.nGatePort))
                .SetTcpDataHandlingAdapter(() => new PacketFixedHeaderDataHandlingAdapter());
            tcpService.Setup(touchSocketConfig);
            LogService.Info("游戏网关初始化完成...");
        }

        public Task Start(CancellationToken cancellationToken = default)
        {
            tcpService.Start();
            LogService.Info($"游戏网关[{SystemShare.Config.sGateAddr}:{SystemShare.Config.nGatePort}]已启动...");
            return StartMessageThread(cancellationToken);
        }

        public async Task StopAsync(CancellationToken cancellationToken = default)
        {
            if (_receiveQueue.Reader.Count > 0)
            {
                await _receiveQueue.Reader.Completion;
            }

            _stoppingCancelReads = new CancellationToken(true);
            await tcpService.StopAsync();
        }

        private void LoadRunAddr()
        {
            string sFileName = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "!runaddr.txt");
            if (File.Exists(sFileName))
            {
                StringList runAddrList = new StringList();
                runAddrList.LoadFromFile(sFileName);
                M2Share.TrimStringList(runAddrList);
                for (int i = 0; i < runAddrList.Count; i++)
                {
                    if (!string.IsNullOrEmpty(runAddrList[i]))
                    {
                        Whitelist.Add(HUtil32.IpToInt(runAddrList[i]));
                    }
                }
                IPHostEntry localEntity = Dns.GetHostEntry(Dns.GetHostName());
                for (int i = 0; i < localEntity.AddressList.Length; i++)
                {
                    if (localEntity.AddressList[i].AddressFamily == AddressFamily.InterNetwork)
                    {
                        Whitelist.Add(HUtil32.IpToInt(localEntity.AddressList[i].ToString()));
                    }
                }
            }
        }

        /// <summary>
        /// 分配一个空闲网关槽位；全满返回 -1（调用方负责拒绝连接）。
        /// 调用方必须已持有 RunSocketSection。
        /// </summary>
        private int AllocGateSlot()
        {
            for (int i = 0; i < _gameGates.Length; i++)
            {
                if (_gameGates[i] == null)
                {
                    return i;
                }
            }
            return -1;
        }

        /// <summary>
        /// 世界侧按网关编号取处理器。
        /// gateIdx 来自客户端认证串里网关自报的编号，可能越界（脏数据）或已被释放，
        /// 这两种情况都必须安全丢弃 —— 世界线程（WorldServer.ProcessHumans 每 200ms 一轮）
        /// 吃到异常会连带丢掉整轮回调。
        /// </summary>
        private ChannelMessageHandler GetGate(int gateIdx)
        {
            if (gateIdx < 0 || gateIdx >= _gameGates.Length)
            {
                return null;
            }
            return _gameGates[gateIdx];
        }

        public void CloseUser(int gateIdx, int nSocket)
        {
            GetGate(gateIdx)?.CloseUser(nSocket);
        }

        public void KickUser(string sAccount, int sessionId, int payMode)
        {
            const string sExceptionMsg = "[Exception] TRunSocket::KickUser";
            const string sKickUserMsg = "当前登录帐号正在其它位置登录，本机已被强行离线!!!";
            const string accountExpiredMsg = "账号付费时间已到,本机已被强行离线,请充值后再继续进行游戏!";
            try
            {
                for (int i = 0; i < _gameGates.Length; i++)
                {
                    if (_gameGates[i] == null)
                    {
                        continue;
                    }
                    ChannelGate gateInfo = _gameGates[i].GateInfo;
                    if (gateInfo.BoUsed && gateInfo.Socket != null && gateInfo.UserList != null)
                    {
                        HUtil32.EnterCriticalSection(RunSocketSection);
                        try
                        {
                            for (int j = 0; j < gateInfo.UserList.Count; j++)
                            {
                                SessionUser gateUserInfo = gateInfo.UserList[j];
                                if (gateUserInfo == null)
                                {
                                    continue;
                                }
                                if (string.Compare(gateUserInfo.Account, sAccount, StringComparison.OrdinalIgnoreCase) == 0 || (gateUserInfo.SessionID == sessionId))
                                {
                                    if (gateUserInfo.FrontEngine != null)
                                    {
                                        gateUserInfo.FrontEngine.DeleteHuman(i, gateUserInfo.Socket);
                                    }
                                    if (gateUserInfo.PlayObject != null)
                                    {
                                        if (payMode == 0)
                                        {
                                            gateUserInfo.PlayObject.SysMsg(sKickUserMsg, MsgColor.Red, MsgType.Hint);
                                        }
                                        else
                                        {
                                            gateUserInfo.PlayObject.SysMsg(accountExpiredMsg, MsgColor.Red, MsgType.Hint);
                                        }
                                        gateUserInfo.PlayObject.BoEmergencyClose = true;
                                        gateUserInfo.PlayObject.BoSoftClose = true;
                                    }
                                    gateInfo.UserList[j] = null;
                                    gateInfo.UserCount -= 1;
                                    break;
                                }
                            }
                        }
                        finally
                        {
                            HUtil32.LeaveCriticalSection(RunSocketSection);
                        }
                    }
                }
            }
            catch (Exception e)
            {
                LogService.Error(sExceptionMsg);
                LogService.Error(e.Message);
            }
        }

        public void CloseAllGate()
        {
            for (int i = 0; i < _gameGates.Length; i++)
            {
                ChannelMessageHandler handler = _gameGates[i];
                if (handler?.GateInfo?.Socket == null)
                {
                    continue;
                }
                handler.GateInfo.Socket.Close();
            }
        }

        public static void CloseErrGate(Socket socket)
        {
            if (socket.Connected)
            {
                socket.Close();
            }
        }

        public void CloseGate(string connectionId, string endPoint)
        {
            const string sGateClose = "游戏网关[{0}]已关闭...";
            HUtil32.EnterCriticalSection(RunSocketSection);
            try
            {
                for (int i = 0; i < _gameGates.Length; i++)
                {
                    if (_gameGates[i] == null)
                    {
                        continue;
                    }
                    if (_gameGates[i].ConnectionId == connectionId)
                    {
                        ChannelGate gateInfo = _gameGates[i].GateInfo;
                        if (gateInfo.Socket == null)
                        {
                            LogService.Error("Socket异常，无需关闭");
                        }
                        else
                        {
                            for (int j = 0; j < gateInfo.UserList.Count; j++)
                            {
                                SessionUser gateUser = gateInfo.UserList[j];
                                if (gateUser != null)
                                {
                                    if (gateUser.PlayObject != null)
                                    {
                                        gateUser.PlayObject.BoEmergencyClose = true;
                                        if (!gateUser.PlayObject.BoReconnection)
                                        {
                                            M2Share.Authentication.SendHumanLogOutMsg(gateUser.Account, gateUser.SessionID);
                                        }
                                    }
                                    gateInfo.UserList[j] = null;
                                }
                            }
                            LogService.Error(string.Format(sGateClose, endPoint));
                        }
                        gateInfo.UserList = null;
                        gateInfo.BoUsed = false;
                        gateInfo.Socket = null;
                        _gameGates[i].Stop();
                        // 释放槽位：重连的网关按「最小空闲槽」重新分配，落回自己声明的编号，
                        // 世界侧（playObject.GateIdx）才找得到这条活连接。
                        _gameGates[i] = null;
                        _gateSlotByConnection.TryRemove(connectionId, out _);
                        break;
                    }
                }
            }
            finally
            {
                HUtil32.LeaveCriticalSection(RunSocketSection);
            }
        }

        public void SendServerStopMsg()
        {
            if (_gameGates.Length > 0)
            {
                for (int i = 0; i < _gameGates.Length; i++) //循环网关发送消息游戏引擎停止服务消息
                {
                    if (_gameGates[i] == null)
                    {
                        continue;
                    }
                    ChannelGate gateInfo = _gameGates[i].GateInfo;
                    if (gateInfo.Socket != null && gateInfo.Socket.Connected)
                    {
                        _gameGates[i].SendCheck(Grobal2.GM_STOP);
                    }
                }
            }
        }

        public void SendOutConnectMsg(int gateIdx, int nSocket, ushort nGsIdx)
        {
            CommandMessage defMsg = Messages.MakeMessage(Messages.SM_OUTOFCONNECTION, 0, 0, 0, 0);
            ServerMessage msgHeader = new ServerMessage();
            msgHeader.PacketCode = Grobal2.PacketCode;
            msgHeader.Socket = nSocket;
            msgHeader.SessionId = nGsIdx;
            msgHeader.Ident = Grobal2.GM_DATA;
            msgHeader.PackLength = CommandMessage.Size;
            ClientOutMessage outMessage = new ClientOutMessage();
            outMessage.MessagePacket = msgHeader;
            outMessage.CommandPacket = defMsg;
            LogService.Info("待实现");
            //AddGateBuffer(gateIdx, SerializerUtil.Serialize(outMessage));
        }

        /// <summary>
        /// 设置用户对应网关编号
        /// </summary>
        public void SetGateUserList(int gateIdx, int nSocket, IPlayerActor playObject)
        {
            GetGate(gateIdx)?.SetGateUserList(nSocket, playObject);
        }

        public void Run()
        {
            if (M2Share.StartReady)
            {
                if (_gameGates.Length > 0)
                {
                    for (int i = 0; i < _gameGates.Length; i++)
                    {
                        if (_gameGates[i] == null)
                        {
                            continue;
                        }
                        ChannelGate gateInfo = _gameGates[i].GateInfo;
                        if (gateInfo.Socket != null && gateInfo.Socket.Connected)
                        {
                            if (HUtil32.GetTickCount() - gateInfo.SendTick >= 1000)
                            {
                                gateInfo.SendTick = HUtil32.GetTickCount();
                                gateInfo.SendMsgBytes = gateInfo.SendBytesCount;
                                gateInfo.SendedMsgCount = gateInfo.SendCount;
                                gateInfo.SendBytesCount = 0;
                                gateInfo.SendCount = 0;
                            }
                            if (gateInfo.SendKeepAlive)
                            {
                                gateInfo.SendKeepAlive = false;
                                _gameGates[i].SendCheck(Grobal2.GM_CHECKSERVER);
                            }
                        }
                    }
                }
            }
        }

        private static void SendGateTestMsg(int nIndex)
        {
            CommandMessage defMsg = new CommandMessage();
            ServerMessage msgHdr = new ServerMessage
            {
                PacketCode = Grobal2.PacketCode,
                Socket = 0,
                Ident = Messages.GM_TEST,
                PackLength = 100
            };
            int nLen = msgHdr.PackLength + ServerMessage.PacketSize;
            ClientOutMessage clientOutMessage = new ClientOutMessage();
            clientOutMessage.CommandPacket = defMsg;
            clientOutMessage.MessagePacket = msgHdr;
            M2Share.NetChannel.AddGateBuffer(nIndex, SerializerUtil.Serialize(clientOutMessage));
        }

        /// <summary>
        /// 添加到网关发送队列
        /// M2Server.>GameGate
        /// </summary>
        /// <returns></returns>
        public void AddGateBuffer(int gateIdx, byte[] senData)
        {
            GetGate(gateIdx)?.ProcessBufferSend(senData);
        }

        public void Send(string connectId, byte[] buff)
        {
            tcpService.Send(connectId, buff);
        }

        /// <summary>
        /// 收到GameGate发来的消息并添加到GameSvr消息队列
        /// </summary>
        public void AddGameGateQueue(int gateIdx, ServerMessage packet, byte[] data)
        {
            _receiveQueue.Writer.TryWrite(new ReceiveData()
            {
                GateId = gateIdx,
                Packet = packet,
                Data = data
            });
        }

        /// <summary>
        /// 处理GameGate消息
        /// </summary>
        private Task StartMessageThread(CancellationToken cancellationToken)
        {
            return Task.Factory.StartNew(async () =>
            {
                while (await _receiveQueue.Reader.WaitToReadAsync(_stoppingCancelReads))
                {
                    while (_receiveQueue.Reader.TryRead(out ReceiveData message))
                    {
                        // 槽位可能在入队之后被 CloseGate 释放，空槽直接丢弃；
                        // 这里抛异常会让整个收包线程退出（所有网关一起哑掉）
                        GetGate(message.GateId)?.ProcessDataBuffer(message.Packet, message.Data);
                    }
                }
            }, cancellationToken);
        }
    }

    public struct ReceiveData
    {
        public ServerMessage Packet;
        public byte[] Data;
        public int GateId;
    }
}