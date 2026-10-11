using BotSrv.Player;
using OpenMir2;
using OpenMir2.Packets.ClientPackets;
using System.Net;

namespace BotSrv.Scenes.Scene
{
    /// <summary>
    /// 登陆场景
    /// </summary>
    public class LoginScene : SceneBase
    {

        private readonly ScoketClient _clientSocket;
        private bool _loginSent;          // T-11：本连接上已发出登录包
        private bool _loginResendDone;    // T-11：对照用的重发已用过一次

        public LoginScene(RobotPlayer robotClient) : base(SceneType.Login, robotClient)
        {
            _clientSocket = new ScoketClient();
            _clientSocket.OnConnected += CSocketConnect;
            _clientSocket.OnDisconnected += CSocketDisconnect;
            _clientSocket.OnReceivedData += CSocketRead;
            _clientSocket.OnError += CSocketError;
        }

        public override void OpenScene()
        {
            _clientSocket.RemoteEndPoint = new IPEndPoint(IPAddress.Parse(MShare.g_sGameIPaddr), MShare.g_nGamePort);
            SetNotifyEvent(Login, 1000);
        }

        public void Login()
        {
            MaybeResendLogin();
            if (ConnectionStep == ConnectionStep.Connect)
            {
                if ((RobotClient.ConnectionStatus == ConnectionStatus.Failure) && (HUtil32.GetTickCount() > RobotClient.ConnectTick))
                {
                    RobotClient.ConnectTick = HUtil32.GetTickCount();
                    LoadMetrics.ConnectAttempt();
                    try
                    {
                        _clientSocket.Connect();
                        RobotClient.ConnectionStatus = ConnectionStatus.Connect;
                    }
                    catch
                    {
                        RobotClient.ConnectionStatus = ConnectionStatus.Failure;
                    }
                }
            }
        }

        public override void CloseScene()
        {
            SetNotifyEvent(CloseSocket, 1000);
        }

        public void PassWdFail()
        {
            ConnectionStep = ConnectionStep.Connect;
        }

        public override void PlayScene()
        {

        }

        /// <summary>
        /// 账号注册
        /// </summary>
        private void NewAccount()
        {
            ConnectionStep = ConnectionStep.NewAccount;
            SendNewAccount(RobotClient.LoginId, RobotClient.LoginPasswd);
        }

        /// <summary>
        /// 账号注册成功
        /// </summary>
        public void ClientNewIdSuccess()
        {
            SendLogin(RobotClient.LoginId, RobotClient.LoginPasswd);
        }

        private void SendNewAccount(string sAccount, string sPassword)
        {
            LogService.Info("创建帐号");
            ConnectionStep = ConnectionStep.NewAccount;
            UserEntry ue = new UserEntry();
            ue.Account = sAccount;
            ue.Password = sPassword;
            ue.UserName = sAccount;
            ue.SSNo = "650101-1455111";
            ue.Quiz = sAccount;
            ue.Answer = sAccount;
            ue.Phone = "";
            ue.EMail = "";
            UserEntryAdd ua = new UserEntryAdd();
            ua.Quiz2 = sAccount;
            ua.Answer2 = sAccount;
            ua.BirthDay = "1978/01/01";
            ua.MobilePhone = "";
            ua.Memo = "";
            ua.Memo2 = "";
            CommandMessage msg = Messages.MakeMessage(Messages.CM_ADDNEWUSER, 0, 0, 0, 0);
            // F3(a)（T-12）观测：注册包“发出/没发出去”分开计；服务端回没回看 `ident_counts[SM_NEWID_*]`。
            bool sent = SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeBuffer(ue) + EDCode.EncodeBuffer(ua));
            if (sent)
            {
                LoadMetrics.RegisterSent();
            }
            else
            {
                LoadMetrics.RegisterSendClosed();
            }
        }

        private void SendLogin(string uid, string passwd)
        {
            // T-8 登录响应观测：「开始登陆」这条日志**早于**真正写 socket（SendSocket 在未连接时只打一句
            // 警告就返回），所以日志行数不等于发出的登录包数。这里把两件事分开计数。
            LoadMetrics.LoginBegin();
            LogService.Info("开始登陆");
            RobotClient.LoginId = uid;
            RobotClient.LoginPasswd = passwd;
            // T-11：按**本连接**记下发出时刻与发送结果（残余丢包方向定位的锚点）
            LoadMetrics.LoginConnStart(RobotClient.SessionId, _clientSocket.ConnSeq);
            CommandMessage msg = Messages.MakeMessage(Messages.CM_IDPASSWORD, 0, 0, 0, 0);
            bool sent = SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(uid + "/" + passwd));
            LoadMetrics.LoginConnSent(RobotClient.SessionId, sent);
            if (sent)
            {
                LoadMetrics.LoginSent();
                _loginSent = true;
            }
            else
            {
                LoadMetrics.LoginSendClosed();
            }
            MShare.SendLogin = true;
        }

        /// <summary>
        /// T-11 对照：登录包发出后 `MIR2_BOT_LOGIN_RESEND_MS` 窗口内没拿到任何应答时，在**同一条连接**上
        /// 重发一次登录包。它只回答一个问题——「这条连接第二次请求能不能拿到应答」。
        /// 窗口未配置（默认 0）时整段不执行，假人行为与 C6 第 4 轮逐字节一致。
        /// </summary>
        private void MaybeResendLogin()
        {
            if (!_loginSent || _loginResendDone)
            {
                return;
            }
            if (!LoadMetrics.LoginConnNeedsResend(RobotClient.SessionId, HUtil32.GetTickCount()))
            {
                return;
            }
            _loginResendDone = true;
            CommandMessage msg = Messages.MakeMessage(Messages.CM_IDPASSWORD, 0, 0, 0, 0);
            bool sent = SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(RobotClient.LoginId + "/" + RobotClient.LoginPasswd));
            LoadMetrics.LoginConnResend(RobotClient.SessionId);
            LogService.Warn($"T-11 登录重发对照（{RobotClient.LoginId}）：sent={sent}");
        }

        public void ClientGetPasswordOk(CommandMessage msg, string sBody)
        {
            MShare.g_wAvailIDDay = HUtil32.LoWord(msg.Recog);
            MShare.g_wAvailIDHour = HUtil32.HiWord(msg.Recog);
            MShare.g_wAvailIPDay = msg.Param;
            MShare.g_wAvailIPHour = msg.Tag;
            if ((MShare.g_wAvailIDHour % 60) > 0)
            {
                LogService.Info("个人帐户的期限: 剩余 " + (MShare.g_wAvailIDHour / 60) + " 小时 " + (MShare.g_wAvailIDHour % 60) + " 分钟.");
            }
            else if (MShare.g_wAvailIDHour > 0)
            {
                LogService.Info("个人帐户的期限: 剩余 " + MShare.g_wAvailIDHour + " 分钟.");
            }
            else
            {
                LogService.Info("帐号登录成功！");
            }
            // 压测计数：登录成功（**放在 if/else 之外**——上面那条日志只在"无期限提示"分支打印，
            // 拿日志行计数会漏；这里三个分支都算登录成功）
            LoadMetrics.LoginOk();
            string sServerName = string.Empty;
            string sText = EDCode.DeCodeString(sBody);
            HUtil32.GetValidStr3(sText, ref sServerName, "/");
            ClientGetSelectServer();
            SendSelectServer(sServerName);
        }

        public void ClientGetSelectServer()
        {

        }

        private void SendSelectServer(string svname)
        {
            LogService.Info($"选择服务器：{svname}");
            ConnectionStep = ConnectionStep.SelServer;
            CommandMessage defMsg = Messages.MakeMessage(Messages.CM_SELECTSERVER, 0, 0, 0, 0);
            SendSocket(EDCode.EncodeMessage(defMsg) + EDCode.EncodeString(svname));
        }

        /// <summary>
        /// 登陆成功
        /// </summary>
        public void ClientGetPasswdSuccess(string body)
        {
            string runaddr = string.Empty;
            string runport = string.Empty;
            string certifystr = string.Empty;
            string str = EDCode.DeCodeString(body);
            str = HUtil32.GetValidStr3(str, ref runaddr, HUtil32.Backslash);
            str = HUtil32.GetValidStr3(str, ref runport, HUtil32.Backslash);
            str = HUtil32.GetValidStr3(str, ref certifystr, HUtil32.Backslash);
            RobotClient.Certification = HUtil32.StrToInt(certifystr, 0);
            MShare.SelChrAddr = runaddr;
            MShare.SelChrPort = HUtil32.StrToInt(runport, 0);
            ConnectionStep = ConnectionStep.QueryChr;
        }

        private bool SendSocket(string sendstr)
        {
            if (_clientSocket.IsConnected)
            {
                return _clientSocket.SendText($"#1{sendstr}!");
            }
            LogService.Warn($"Socket Close: {_clientSocket.RemoteEndPoint}");
            return false;
        }

        private void CloseSocket()
        {
            _clientSocket.Disconnect();//断开登录网关链接
            LogService.Info("主动断开");
        }

        #region Socket Events

        private void CSocketConnect(object sender, DSCClientConnectedEventArgs e)
        {
            LoadMetrics.ConnectOk();
            MShare.ServerConnected = true;
            if (ConnectionStep == ConnectionStep.Connect)
            {
                if (RobotClient.NewAccount)
                {
                    SetNotifyEvent(NewAccount, 3000);
                }
                else
                {
                    ClientNewIdSuccess();
                }
            }
            if (MShare.ConnectionStep == ConnectionStep.Login)
            {
                RobotClient.DScreen.ChangeScene(SceneType.Login);
            }
        }

        private void CSocketDisconnect(object sender, DSCClientConnectedEventArgs e)
        {
            MShare.ServerConnected = false;
            if (MShare.g_SoftClosed)
            {
                MShare.g_SoftClosed = false;
            }
            else if ((RobotClient.DScreen.CurrentScene == RobotClient.LoginScene) && !MShare.SendLogin)
            {
                LogService.Info("游戏连接已关闭...");
            }
        }

        private void CSocketError(object sender, DSCClientErrorEventArgs e)
        {
            LoadMetrics.ConnectError(e.ErrorCode);
            switch (e.ErrorCode)
            {
                case System.Net.Sockets.SocketError.ConnectionRefused:
                    LogService.Warn($"游戏服务器[{_clientSocket.RemoteEndPoint}]拒绝链接...");
                    break;
                case System.Net.Sockets.SocketError.ConnectionReset:
                    LogService.Warn($"游戏服务器[{_clientSocket.RemoteEndPoint}]关闭连接...");
                    break;
                case System.Net.Sockets.SocketError.TimedOut:
                    LogService.Warn($"游戏服务器[{_clientSocket.RemoteEndPoint}]链接超时...");
                    break;
            }
        }

        private void CSocketRead(object sender, DSCClientDataInEventArgs e)
        {
            // T-11：本连接收到多少原始字节（0 ⇒ 服务端没在这条 TCP 连接上回过任何东西）
            LoadMetrics.LoginConnRecv(RobotClient.SessionId, e.BuffLen);
            string sData = HUtil32.GetString(e.Buff, 0, e.BuffLen);
            if (!string.IsNullOrEmpty(sData))
            {
                BotShare.ClientMgr.AddPacket(RobotClient.SessionId, sData);
            }
        }

        #endregion
    }
}
