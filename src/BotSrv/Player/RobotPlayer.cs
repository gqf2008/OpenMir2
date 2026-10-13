using BotSrv.Data;
using BotSrv.Maps;
using BotSrv.Objects;
using BotSrv.Scenes;
using BotSrv.Scenes.Scene;
using OpenMir2;
using OpenMir2.Enums;
using OpenMir2.Packets.ClientPackets;
using System;
using System.Collections;
using System.Collections.Generic;
using System.Drawing;

namespace BotSrv.Player
{
    public partial class RobotPlayer
    {
        public readonly string SessionId;
        public TimerAutoPlay TimerAutoPlay;
        public TimerAutoPlay TimerAutoMove;
        public static int LastestClickTime = 0;
        public static bool GMoveBusy = false;
        public bool GPathBusy = false;
        public static int GMoveStep = 0;
        public static int GMoveErr = 0;
        public static int GMoveErrTick = 0;
        public ScreenManager DScreen = null;
        public IntroScene IntroScene = null;
        public LoginScene LoginScene = null;
        public SelectChrScene SelectChrScene = null;
        public PlayScene PlayScene = null;
        public TMap Map = null;
        public static long GDwOverSpaceWarningTick = 0;
        private readonly TTimerCommand _timerCmd;
        private int _actionLockTime = 0;
        private readonly short _actionKey = 0;
        private CommandMessage _waitingMsg;
        private string _waitingStr = string.Empty;
        private readonly string _whisperName = string.Empty;
        private int _mDwProcUseMagicTick = 0;
        private readonly TTimerCommand TimerCmd;
        private readonly int ActionLockTime = 0;
        private CommandMessage WaitingMsg;
        private string WhisperName = string.Empty;
        private readonly int m_dwProcUseMagicTick = 0;
        public bool ActionFailLock = false;
        public int ActionFailLockTime = 0;
        public int LastHitTick = 0;
        public bool NewAccount = false;
        public string LoginId = string.Empty;
        public string LoginPasswd = string.Empty;
        public string ChrName = string.Empty;
        public int Certification = 0;
        public int MNEatRetIdx = 0;
        public bool ActionLock = false;
        public bool MBoSupplyItem = false;
        public int MDwDuraWarningTick = 0;
        public int DwIpTick = 0;
        public int DwhIpTick = 0;
        private readonly HeroActor _heroActor;
        public ScoketClient ClientSocket;
        public int ConnectTick = 0;
        public ConnectionStatus ConnectionStatus;

        public RobotPlayer()
        {
            SessionId = Guid.NewGuid().ToString("N");
            _heroActor = new HeroActor(this);
            MShare.AutoPathList = new List<FindMapNode>();
            DScreen = new ScreenManager(this);
            IntroScene = new IntroScene(this);
            LoginScene = new LoginScene(this);
            SelectChrScene = new SelectChrScene(this);
            PlayScene = new PlayScene(this);
            Map = new TMap(this);
            MShare.g_DropedItemList = new List<TDropItem>();
            MShare.g_MagicList = new List<ClientMagic>();
            MShare.g_FreeActorList = new List<Actor>();
            //EventMan = new TClEventManager();
            MShare.g_ChangeFaceReadyList = new ArrayList();
            MShare.g_SendSayList = new ArrayList();
            if (MShare.MySelf != null)
            {
                MShare.MySelf.SlaveObject.Clear();
                MShare.MySelf = null;
            }
            MShare.InitClientItems();
            MShare.g_DetectItemMineID = 0;
            MShare.LastMapMusic = -1;
            MShare.TargetX = -1;
            MShare.TargetY = -1;
            MShare.TargetCret = null;
            MShare.FocusCret = null;
            MShare.g_FocusItem = null;
            MShare.MagicTarget = null;
            MShare.g_nTestReceiveCount = 0;
            MShare.g_boServerChanging = false;
            MShare.g_boBagLoaded = false;
            MShare.g_boAutoDig = false;
            MShare.g_boAutoSit = false;
            MShare.g_dwLatestClientTime2 = 0;
            MShare.g_dwFirstClientTime = 0;
            MShare.g_dwFirstServerTime = 0;
            MShare.g_dwFirstClientTimerTime = 0;
            MShare.g_dwLatestClientTimerTime = 0;
            MShare.g_dwFirstClientGetTime = 0;
            MShare.g_dwLatestClientGetTime = 0;
            MShare.g_nTimeFakeDetectCount = 0;
            MShare.g_nTimeFakeDetectTimer = 0;
            MShare.g_nTimeFakeDetectSum = 0;
            MShare.g_nAreaStateValue = 0;
            MShare.ConnectionStep = ConnectionStep.Intro;
            MShare.SendLogin = false;
            MShare.ServerConnected = false;
            MShare.g_SoftClosed = false;
            ActionFailLock = false;
            MShare.g_boMapMoving = false;
            MShare.MapMovingWait = false;
            MShare.g_boCheckBadMapMode = false;
            MShare.g_boCheckSpeedHackDisplay = false;
            MShare.g_nDupSelection = 0;
            MShare.LastAttackTick = MShare.GetTickCount();
            MShare.LastMoveTick = MShare.GetTickCount();
            MShare.LatestSpellTick = MShare.GetTickCount();
            MShare.AutoPickupTick = MShare.GetTickCount();
            MShare.g_boItemMoving = false;
            MShare.g_boNextTimePowerHit = false;
            MShare.g_boCanLongHit = false;
            MShare.g_boCanWideHit = false;
            MShare.g_boCanCrsHit = false;
            MShare.g_boNextTimeFireHit = false;
            MShare.g_boCanSLonHit = false;
            MShare.g_boNextTimeTwinHit = false;
            MShare.g_boNextTimePursueHit = false;
            MShare.g_boNextTimeRushHit = false;
            MShare.g_boNextTimeSmiteHit = false;
            MShare.g_boNextTimeSmiteLongHit = false;
            MShare.g_boNextTimeSmiteLongHit3 = false;
            MShare.g_boNextTimeSmiteLongHit2 = false;
            MShare.g_boNextTimeSmiteWideHit = false;
            MShare.g_boNextTimeSmiteWideHit2 = false;
            MShare.g_boQueryPrice = false;
            MShare.g_sSellPriceStr = "";
            MShare.g_boAllowGroup = false;
            MShare.g_GroupMembers = new List<string>();
            MShare.LoadItemDesc();
            MShare.LoadItemFilter();
            ConnectionStatus = ConnectionStatus.Failure;
            for (int i = 0; i < BotConst.MAXX * 3; i++)
            {
                for (int j = 0; j < BotConst.MAXY * 3; j++)
                {
                    MShare.g_APPassEmpty[i, j] = 0xFF;
                }
            }
        }

        public void SocketEvents()
        {
            if (ClientSocket == null)
            {
                ClientSocket = new ScoketClient();
            }
            ClientSocket.OnConnected -= SocketConnect;
            ClientSocket.OnDisconnected -= SocketDisconnect;
            ClientSocket.OnReceivedData -= ReceivedData;
            ClientSocket.OnError -= SocketError;
            ClientSocket.OnConnected += SocketConnect;
            ClientSocket.OnDisconnected += SocketDisconnect;
            ClientSocket.OnReceivedData += ReceivedData;
            ClientSocket.OnError += SocketError;
            ClientSocket.Connect(MShare.RunServerAddr, MShare.RunServerPort);
        }

        #region Socket Events

        private void SocketConnect(object sender, DSCClientConnectedEventArgs e)
        {
            MShare.ServerConnected = true;
            if (MShare.ConnectionStep == ConnectionStep.Login)
            {
                DScreen.ChangeScene(SceneType.Login);
            }
            if (MShare.ConnectionStep == ConnectionStep.Play)
            {
                ClientSocket.IsConnected = true;
                SendRunLogin();
                if (!MShare.g_boServerChanging)
                {
                    BotHelper.ClearBag();
                }
                else
                {
                    ChangeServerClearGameVariables();
                }
            }
        }

        private void SocketDisconnect(object sender, DSCClientConnectedEventArgs e)
        {
            MShare.ServerConnected = false;
            if (MShare.g_SoftClosed)
            {
                MShare.g_SoftClosed = false;
                //ActiveCmdTimer(MShare.TTimerCommand.tcReSelConnect);
            }
            else if ((DScreen.CurrentScene == LoginScene) && !MShare.SendLogin)
            {
                MainOutMessage("游戏连接已关闭...");
            }
            if (DScreen.CurrentScene == PlayScene)
            {
                LoginOut();
                BotShare.ClientMgr.DelClient(SessionId);
            }
        }

        private void SocketError(object sender, DSCClientErrorEventArgs e)
        {
            // 防重入（2026-10-11 实测的爆栈根因）：`ScoketClient.SendText` 在已坏的 socket 上抛
            // SocketException 时会**同步**回调 OnError，而下面这条错误分支会去调 LoginOut() →
            // SendClientMessage() → SendSocket() → SendText() —— 于是在同一个调用栈里无限递归：
            //   SendText → SocketError → LoginOut → SendSocket → SendText → …
            // 实测 134 层后 stack overflow，**整个 BotSrv 进程直接死**（一档数据全废）。
            // 这里用重入闸门把它变成"一次错误、一条记录、bot 退场"。
            if (_sockErrorHandling)
            {
                return;
            }
            _sockErrorHandling = true;
            try
            {
            switch (e.ErrorCode)
            {
                case System.Net.Sockets.SocketError.ConnectionRefused:
                    MainOutWarnMessage($"游戏服务器[{ClientSocket.RemoteEndPoint}]拒绝链接...");
                    LoadMetrics.ConnRefused();
                    break;
                case System.Net.Sockets.SocketError.ConnectionReset:
                    MainOutWarnMessage($"游戏服务器[{ClientSocket.RemoteEndPoint}]关闭连接...");
                    LoadMetrics.ConnLost();      // 掉线（与"连不上"分开计）
                    break;
                case System.Net.Sockets.SocketError.TimedOut:
                    MainOutWarnMessage($"游戏服务器[{ClientSocket.RemoteEndPoint}]链接超时...");
                    LoadMetrics.ConnLost();
                    break;
                default:
                    LoadMetrics.ConnLost();
                    break;
            }
            // refused/reset 都是"这条连接已经不可用"：先摘掉 IsConnected，避免 LoginOut/探针继续往
            // 死 socket 上写（每写一次就再抛一次 SocketException，压测里就是噪声与无谓开销）。
            if ((e.ErrorCode == System.Net.Sockets.SocketError.ConnectionRefused) ||
                (e.ErrorCode == System.Net.Sockets.SocketError.ConnectionReset))
            {
                if (ClientSocket != null)
                {
                    ClientSocket.IsConnected = false;
                }
            }
            if (DScreen.CurrentScene == PlayScene)
            {
                LoginOut();
                BotShare.ClientMgr.DelClient(SessionId);
            }
            }
            finally
            {
                _sockErrorHandling = false;
            }
        }

        private void ReceivedData(object sender, DSCClientDataInEventArgs e)
        {
            string sData = HUtil32.GetString(e.Buff, 0, e.BuffLen);
            if (!string.IsNullOrEmpty(sData))
            {
                int n = sData.IndexOf("*", StringComparison.OrdinalIgnoreCase);
                if (n > 0)
                {
                    string data2 = sData[..(n - 1)];
                    sData = data2 + sData.Substring(n, sData.Length);
                    ClientSocket.SendBuffer(HUtil32.GetBytes(BotConst.Activebuf));
                }
                BotShare.ClientMgr.AddPacket(SessionId, sData);
            }
        }

        #endregion

        /// <summary>压测动作探针：见 ProbeActionTick()。</summary>
        private static readonly bool _probeEnabled =
            Environment.GetEnvironmentVariable("MIR2_BOT_ACTION_PROBE") == "1";
        private static readonly int _probeIntervalMs = ProbeIntervalFromEnv();
        private int _probeLastTick;
        private int _probeDir;
        /// <summary>服务端确认进世界（收到 SM_LOGON）——探针的武装条件。</summary>
        private volatile bool _inWorld;
        private bool _inWorldCounted;
        /// <summary>本假人自己的世界坐标（SM_LOGON 里服务端给的落点）。</summary>
        private int _probeX;
        private int _probeY;
        /// <summary>本假人自己的 ActorId（SM_LOGON 的 `Recog`）。
        /// M3②③ 夹具（T-29 扩展，owner 2026-10-12 17:35）：`M3ATTSEQ` 的 attack/struck 行都带 `my`
        /// = 本假人自身 ActorId。attack 行的 `my` 是 `attack_diff.py` 的施击者身份来源（缺它 ⇒
        /// 判据 fail-closed，**不**静默降级）；struck 行的 `my`（观察者自身 ActorId）供判据区分
        /// attacker 视角与 victim 视角（真机同一击会被两个假人各落一遍；去重靠事件键，
        /// 这个字段用于统计 `other_observer` 并佐证重复来源）。</summary>
        private int _myActorId;
        /// <summary>防 SocketError 重入（SendText 失败会**同步**回调 OnError，见 ScoketClient.SendText）。</summary>
        private bool _sockErrorHandling;

        public static bool ProbeEnabled => _probeEnabled;

        public static int ProbeIntervalMs => _probeIntervalMs;

        private static int ProbeIntervalFromEnv()
        {
            string v = Environment.GetEnvironmentVariable("MIR2_BOT_ACTION_PROBE_MS");
            if (!string.IsNullOrEmpty(v) && int.TryParse(v, out int ms) && ms > 0)
            {
                return ms;
            }
            return 1000;
        }

        /// <summary>压测用动作探针：进世界后每 ~1s 发一个 `CM_TURN`，用来拿服务端的 `#+GD/&lt;rtime&gt;!` ack
        /// （tick 服务时延口径的样本来源）。
        ///
        /// **为什么这样写**：BotSrv 的挂机/移动层建立在**跨假人共享的全局态**上（`MShare.MySelf` 等），
        /// N 个假人只有登录链路无状态 ⇒ 谁都动不了、tick 样本恒为 0（2026-10-10 实测）。
        /// 本探针刻意**只依赖本假人自己的东西**：自己的 socket（`SendSocket`）与自己实例上的场景状态
        /// （`DScreen.CurrentScene == PlayScene`），**不读不写 MShare 的世界态** ⇒ 每个假人各自产生动作。
        /// 线上形状照 C3 金标准的实测（`CM_TURN`：`Recog`=递增时间戳样值、`Param`=0、`Tag`=方向样值、`Series`=0）。</summary>
        private void ProbeActionTick()
        {
            // 武装条件 = **服务端确认进世界**（SM_LOGON ⇒ `_inWorld`）+ 本假人自己的 socket 活着。
            //
            // 不能用 `DScreen.CurrentScene == PlayScene` 做条件（2026-10-10/11 实测的零样本根因）：
            // 该标志在收到 SM_STARTPLAY 时就置上了，**早于网关登录握手**（`**account/chr/...`），
            // 于是探针的 `CM_TURN` 会抢在登录串之前写进同一条 socket，网关按"首包"解析它 ⇒
            // 解不出登录串 ⇒ `ClientLogin` 失败 ⇒ `Kick(1)` 重置连接（src/GameGate/Services/ClientSession.cs
            // "客户端登陆消息处理失败，剔除链接"）⇒ 既进不了世界，又触发 SocketError 递归（见下）。
            if (!_probeEnabled || !_inWorld || ClientSocket == null || !ClientSocket.IsConnected)
            {
                return;
            }
            int now = MShare.GetTickCount();
            if (now - _probeLastTick < _probeIntervalMs)
            {
                return;
            }
            _probeLastTick = now;
            _probeDir = (_probeDir + 1) & 7;
            // Recog 必须是 **MakeLong(X, Y)**，不是时间戳：服务端 WorldServer 对 CM_TURN
            // 走 `SendMsg(Ident, Tag, LoWord(Recog), HiWord(Recog), 0)`，于是
            //   nParam1 = X = LoWord(Recog)、nParam2 = Y = HiWord(Recog)、wParam = 方向 = Tag，
            // 而 PlayObject.ClientChangeDir 只有在 `nX == CurrX && nY == CurrY` 时才回
            // `#+GD/<rtime>!`（PlayObject.Operate.cs:310）。原先塞的是时间戳 ⇒ X/Y 恒不等于自身坐标
            // ⇒ 服务端每个探针动作都判 false ⇒ tick 样本恒为 0（2026-10-11 实测根因，两处：
            // 见本方法开头的握手竞态，及此处）。C3 金标准里 CM_WALK 的 Recog=0x026B011F 就是 X=287/Y=619 的坐标，不是时间戳。
            CommandMessage msg = Messages.MakeMessage(Messages.CM_TURN,
                HUtil32.MakeLong((short)_probeX, (short)_probeY), 0, _probeDir, 0);
            SendSocket(EDCode.EncodeMessage(msg));
            LoadMetrics.ProbeSent();
        }

        #region M3 T-14 固定背包/装备序列（夹具专用，默认关；白名单 T-22）

        /// <summary>下一步要发的背包/装备动作：-1=未武装；0..4=待发；5=已完成。</summary>
        private int _bagSeqStep = -1;
        /// <summary>下一步的到点时刻（MShare.GetTickCount），起步延迟与步间隔都用它。</summary>
        private int _bagSeqNextTick;

        // T-14 步骤参数**在 step=0 一次性快照**（见 SnapshotBagEquipSequence 的说明）：
        // 序列一旦快照就不再依赖「服务端回应 → 假人视图演化」，因此两侧发出去的 CM_* 字节
        // 只由「进入世界后的初始视图」决定 —— 这正是「同一序列」要的可复现性。
        private bool _bagSeqSnapped;
        private int _seqTakeOnSlot = -1;
        private int _seqTakeOnMake;
        private string _seqTakeOnName = "";
        private int _seqTakeOffSlot = -1;
        private int _seqTakeOffMake;
        private string _seqTakeOffName = "";
        private int _seqDropSlot = -1;
        private int _seqDropMake;
        private string _seqDropName = "";
        private int _seqEatSlot = -1;
        private int _seqEatMake;
        private int _seqEatStdMode;
        private string _seqEatName = "";

        /// <summary>收到本假人 SM_LOGON 后武装固定序列（`MShare.g_boBagEquipSequence` 关闭时是空操作）。
        /// 武装后**不做**任何即时发送：第一步还要等 `g_nBagEquipStartDelayMs`（默认 3000ms），
        /// 给服务端把物品列表推完、本假人视图装载完的时间。</summary>
        private void ArmBagEquipSequence()
        {
            if (!MShare.g_boBagEquipSequence)
            {
                return;
            }
            _bagSeqStep = 0;
            _bagSeqSnapped = false;
            _bagSeqNextTick = MShare.GetTickCount() + MShare.g_nBagEquipStartDelayMs;
            MainOutMessage($"M3BAGSEQ arm start_delay_ms={MShare.g_nBagEquipStartDelayMs} step_ms={MShare.g_nBagEquipStepMs}");
        }

        /// <summary>M3 T-14 固定序列（步进式）：
        ///   0 CM_PICKUP     —— 拾取当前格子上的地面物品
        ///   1 CM_TAKEONITEM —— 把背包里第一件可装备物品穿上（`BotHelper.GetTakeOnPosition` 定槽）
        ///   2 CM_TAKEOFFITEM—— 卸下**刚才穿上**的那件（Step 1 的落点；无 Step 1 才回落到初始视图的第一件已装备物）
        ///   3 CM_DROPITEM   —— 丢弃背包里第一件物品（初始视图 6.. 起）
        ///   4 CM_EAT        —— 吃掉初始视图里第一件药品（StdMode 0/1/2，**0..MaxBagItemcl 全槽**：
        ///                        BotSrv 的 `AddItemBag` 把 StdMode<=3 的消耗品放 0..4，旧实现只扫 6.. 永远找不到）
        ///
        /// **参数在 step=0 快照一次**，之后不再读「已被服务端回应改过的视图」。理由（本片实测）：
        /// 假人侧的 `g_WaitingUseItem`/`g_EatingItem` 与 `ClientItem` 解码在 T-14 之前从未被真实触发，
        /// 回应处理器里有多处 latent NRE ⇒ 逐步骤读活视图会让「两侧同一序列」依赖于那些未实现路径。
        /// 快照只依赖进入世界后服务端推来的初始背包视图（`SM_BAGITEMS`），这正是两侧应对齐的输入。
        /// 缺件时**只记 skip、不发送** —— 两侧同缺即同 skip，不引入差异。默认关时本方法空转。</summary>
        private void BagEquipSequenceTick()
        {
            if (!MShare.g_boBagEquipSequence || _bagSeqStep < 0 || _bagSeqStep > 4)
            {
                return;
            }
            if (!_inWorld || ClientSocket == null || !ClientSocket.IsConnected)
            {
                return;
            }
            int now = MShare.GetTickCount();
            if (now < _bagSeqNextTick)
            {
                return;
            }
            _bagSeqNextTick = now + MShare.g_nBagEquipStepMs;
            int step = _bagSeqStep;
            if (step == 0 && !_bagSeqSnapped)
            {
                SnapshotBagEquipSequence();
            }
            switch (step)
            {
                case 0:
                    MainOutMessage($"M3BAGSEQ step=0 op=CM_PICKUP x={_probeX} y={_probeY}");
                    SendPickup();
                    break;
                case 1:
                    BagSeqTakeOn();
                    break;
                case 2:
                    BagSeqTakeOff();
                    break;
                case 3:
                    BagSeqDrop();
                    break;
                case 4:
                    BagSeqEat();
                    break;
            }
            _bagSeqStep = step + 1;
            if (_bagSeqStep > 4)
            {
                MainOutMessage("M3BAGSEQ done");
            }
        }

        /// <summary>step=0 时把五步要用的 <b>MakeIndex/槽位/名字</b> 一次性钉住：
        /// 只读 `MShare.ItemArr`（服务端推来的初始背包视图）与 `MShare.UseItems`，
        /// 之后各步不再回头看任何会随服务端回应变化的视图。</summary>
        private void SnapshotBagEquipSequence()
        {
            _bagSeqSnapped = true;
            int bagView = 0;
            for (int i = 0; i < BotConst.MaxBagItemcl; i++)
            {
                if (MShare.ItemArr[i] != null && !string.IsNullOrEmpty(MShare.ItemArr[i].Item.Name))
                {
                    bagView++;
                }
            }

            // Step 1：背包里第一件可装备物品（6..）。
            for (int i = 6; i < BotConst.MaxBagItemcl; i++)
            {
                ClientItem it = MShare.ItemArr[i];
                if (it == null || string.IsNullOrEmpty(it.Item.Name) || it.Item.NeedIdentify >= 4)
                {
                    continue;
                }
                int where = BotHelper.GetTakeOnPosition(it, MShare.UseItems, true);
                if (where < 0)
                {
                    continue;
                }
                _seqTakeOnSlot = i;
                _seqTakeOnMake = it.MakeIndex;
                _seqTakeOnName = it.Item.Name;
                // Step 2 卸下的就是这一件（穿上后落在 where 槽）。
                _seqTakeOffSlot = where;
                _seqTakeOffMake = it.MakeIndex;
                _seqTakeOffName = it.Item.Name;
                break;
            }
            if (_seqTakeOnSlot < 0)
            {
                // 没有可穿的：才回落到初始视图里第一件已装备物（通常为空）。
                for (int w = 0; w < MShare.UseItems.Length; w++)
                {
                    ClientItem eq = MShare.UseItems[w];
                    if (eq == null || string.IsNullOrEmpty(eq.Item.Name))
                    {
                        continue;
                    }
                    _seqTakeOffSlot = w;
                    _seqTakeOffMake = eq.MakeIndex;
                    _seqTakeOffName = eq.Item.Name;
                    break;
                }
            }

            // Step 3：背包里第一件物品（6..）。
            for (int i = 6; i < BotConst.MaxBagItemcl; i++)
            {
                ClientItem it = MShare.ItemArr[i];
                if (it == null || string.IsNullOrEmpty(it.Item.Name))
                {
                    continue;
                }
                _seqDropSlot = i;
                _seqDropMake = it.MakeIndex;
                _seqDropName = it.Item.Name;
                break;
            }

            // Step 4：第一件药品（StdMode 0/1/2，全槽 0..）。
            for (int i = 0; i < BotConst.MaxBagItemcl; i++)
            {
                ClientItem it = MShare.ItemArr[i];
                if (it == null || string.IsNullOrEmpty(it.Item.Name))
                {
                    continue;
                }
                if (it.Item.StdMode != 0 && it.Item.StdMode != 1 && it.Item.StdMode != 2)
                {
                    continue;
                }
                _seqEatSlot = i;
                _seqEatMake = it.MakeIndex;
                _seqEatStdMode = it.Item.StdMode;
                _seqEatName = it.Item.Name;
                break;
            }

            MainOutMessage($"M3BAGSEQ snapshot bag_view={bagView} takeon={_seqTakeOnSlot} takeoff={_seqTakeOffSlot} drop={_seqDropSlot} eat={_seqEatSlot}");
        }

        /// <summary>步骤 1：背包里第一件可装备物品 → CM_TAKEONITEM(where, MakeIndex, name)。</summary>
        private void BagSeqTakeOn()
        {
            if (_seqTakeOnSlot < 0)
            {
                MainOutMessage("M3BAGSEQ step=1 op=CM_TAKEONITEM skip=no_equippable_bag_item");
                return;
            }
            // 让 SM_TAKEON_OK 处理器把该物品从背包挪到 UseItems[where]（同一对象引用）。
            if (MShare.g_WaitingUseItem != null)
            {
                MShare.g_WaitingUseItem.Index = _seqTakeOffSlot;
                MShare.g_WaitingUseItem.Item = MShare.ItemArr[_seqTakeOnSlot];
            }
            MainOutMessage($"M3BAGSEQ step=1 op=CM_TAKEONITEM bag={_seqTakeOnSlot} where={_seqTakeOffSlot} make={_seqTakeOnMake} name={_seqTakeOnName}");
            SendTakeOnItem(_seqTakeOffSlot, _seqTakeOnMake, _seqTakeOnName);
        }

        /// <summary>步骤 2：卸下 Step 1 穿上那一件 → CM_TAKEOFFITEM(where, MakeIndex, name)。</summary>
        private void BagSeqTakeOff()
        {
            if (_seqTakeOffSlot < 0)
            {
                MainOutMessage("M3BAGSEQ step=2 op=CM_TAKEOFFITEM skip=no_equipped_item");
                return;
            }
            if (MShare.g_WaitingUseItem != null)
            {
                MShare.g_WaitingUseItem.Index = _seqTakeOffSlot;
                MShare.g_WaitingUseItem.Item = _seqTakeOnSlot >= 0 ? MShare.ItemArr[_seqTakeOnSlot] : MShare.UseItems[_seqTakeOffSlot];
            }
            MainOutMessage($"M3BAGSEQ step=2 op=CM_TAKEOFFITEM where={_seqTakeOffSlot} make={_seqTakeOffMake} name={_seqTakeOffName}");
            SendTakeOffItem((byte)_seqTakeOffSlot, _seqTakeOffMake, _seqTakeOffName);
        }

        /// <summary>步骤 3：丢弃背包里第一件物品 → CM_DROPITEM(MakeIndex, 1, name)。</summary>
        private void BagSeqDrop()
        {
            if (_seqDropSlot < 0)
            {
                MainOutMessage("M3BAGSEQ step=3 op=CM_DROPITEM skip=no_bag_item");
                return;
            }
            MainOutMessage($"M3BAGSEQ step=3 op=CM_DROPITEM bag={_seqDropSlot} make={_seqDropMake} name={_seqDropName}");
            SendDropItem(_seqDropName, _seqDropMake, 1);
        }

        /// <summary>步骤 4：吃掉第一件药品（StdMode 0/1/2）→ CM_EAT(MakeIndex, 0, 0, StdMode)。</summary>
        private void BagSeqEat()
        {
            if (_seqEatSlot < 0)
            {
                MainOutMessage("M3BAGSEQ step=4 op=CM_EAT skip=no_drug_in_bag");
                return;
            }
            if (MShare.g_EatingItem != null)
            {
                MShare.g_EatingItem = MShare.ItemArr[_seqEatSlot];
            }
            MainOutMessage($"M3BAGSEQ step=4 op=CM_EAT bag={_seqEatSlot} make={_seqEatMake} stdmode={_seqEatStdMode} name={_seqEatName}");
            SendEat(_seqEatMake, _seqEatName, _seqEatStdMode);
        }

        #endregion

        #region M3②③ 固定攻击序列（夹具专用，默认关；白名单 T-22 扩展）

        /// <summary>下一步要发的攻击序号（-1=未武装；0..count-1=待发）。</summary>
        private int _attackSeqStep = -1;
        /// <summary>下一步的到点时刻（MShare.GetTickCount）。</summary>
        private int _attackSeqNextTick;

        /// <summary>收到本假人 SM_LOGON 后武装固定攻击序列（`MShare.g_boAttackSequence` 关闭或本账号
        /// 不是 attacker（`g_nAttackRole`）时空操作）。武装后不做即时发送：第一步等 `g_nAttackStartDelayMs`，
        /// 给服务端把 ``SM_LOGON`` / 场景推送完、本假人落点（`_probeX/_probeY`）确定的时间。</summary>
        private void ArmAttackSequence()
        {
            if (!MShare.g_boAttackSequence)
            {
                return;
            }
            // M-2（reviewer-5 76e433f0）：精确比较，不用 EndsWith（`-Bots>10` 时 10/20… 会误武装）。
            if (!MShare.IsAttackAttacker(LoginId, MShare.g_sAttackRoleLoginId))
            {
                return;
            }
            _attackSeqStep = 0;
            _attackSeqNextTick = MShare.GetTickCount() + MShare.g_nAttackStartDelayMs;
            MainOutMessage($"M3ATTSEQ {{\"role\":\"arm\",\"id\":\"{LoginId}\",\"count\":{MShare.g_nAttackCount},\"step_ms\":{MShare.g_nAttackStepMs},\"dir\":{MShare.g_nAttackDir}}}");
        }

        /// <summary>M3②③ 固定攻击序列：每 `g_nAttackStepMs` 发一条 `CM_HIT`——
        /// `Recog = MakeLong(自己的 CurrX, CurrY)`、`Tag = g_nAttackDir`（照 `ProbeActionTick` 的线上形状）；
        /// 共 `g_nAttackCount` 步，逐步落 `M3ATTSEQ {json}` 标记（`attack_diff.py` 据此按「第 k 次」对齐）。
        /// 只写本假人自己的 socket，不读不写跨假人共享世界态（同 T-22 纪律）。默认关时空转。</summary>
        private void AttackSequenceTick()
        {
            if (!MShare.g_boAttackSequence || _attackSeqStep < 0)
            {
                return;
            }
            if (!_inWorld || ClientSocket == null || !ClientSocket.IsConnected)
            {
                return;
            }
            if (_attackSeqStep >= MShare.g_nAttackCount)
            {
                return;
            }
            int now = MShare.GetTickCount();
            if (now < _attackSeqNextTick)
            {
                return;
            }
            _attackSeqNextTick = now + MShare.g_nAttackStepMs;
            int k = _attackSeqStep + 1;
            // M3②③ 逐击状态记录（owner 2026-10-12 23:50；T-29 扩展）：**发 CM_HIT 之前**落一条
            // `role=state`——把「C# 侧目标格 (18,20) 从 k=4 起为空」钉到具体成因（受害被 ghost/移走
            // 还是从未在格上）。m3cs `per_strike_requests.py jointable` 的权威输入。
            MainOutMessage("M3ATTSEQ " + M3AttackStateJson(k));
            MainOutMessage($"M3ATTSEQ {{\"role\":\"attack\",\"k\":{k},\"op\":\"CM_HIT\",\"x\":{_probeX},\"y\":{_probeY},\"dir\":{MShare.g_nAttackDir},\"my\":{_myActorId},\"id\":\"{LoginId}\"}}");
            CommandMessage msg = Messages.MakeMessage(Messages.CM_HIT,
                HUtil32.MakeLong((short)_probeX, (short)_probeY), 0, MShare.g_nAttackDir, 0);
            SendSocket(EDCode.EncodeMessage(msg));
            _attackSeqStep = k;
            if (_attackSeqStep >= MShare.g_nAttackCount)
            {
                MainOutMessage($"M3ATTSEQ {{\"role\":\"done\",\"id\":\"{LoginId}\",\"attacks\":{MShare.g_nAttackCount}}}");
            }
        }

        /// <summary>M3②③ 逐击状态记录（owner 2026-10-12 23:50；T-29 扩展）：m3cs
        /// `per_strike_requests.py jointable` 的权威输入——每击在**发出 CM_HIT 之前**落一条
        /// `M3ATTSEQ {"role":"state",...}`：攻方 = 本假人自己的视图（`MShare.MySelf`），
        /// 受害 = **本假人视角下「前方格」上的首个可见活体**（= oracle `GetPoseCreate()` 的客户端
        /// 等价物；无 ⇒ `null`）。schema 按 m3cs 接口请求：`attacker`/`victim` 各
        /// `x/y/dir/ghost/death/hp` 六字段；另附 `fx/fy/vid/others` 诊断（m3cs 解析器忽略未知键）。
        ///
        /// 口径要点：① **发 CM_HIT 之前**取快照（= oracle `PlayObject.Attack.cs:26` 取目标那一刻的
        /// 客户端最近视图）；② 受害被 ghost/hide 时服务端向观察者发 `SM_GHOST/SM_HIDE/SM_DISAPPEAR`，
        /// 客户端 `PlayScene` 会 `DeleteActor` ⇒ 该对象**不在** `ActorList` ⇒ 这里自然读到 `null`
        /// （这正是「目标格为什么空」的直接证据）；③ `others` = 本假人视图里所有玩家族对象的
        /// 坐标/HP，用于区分「受害被移走」与「受害走开了」。只读本假人自己的视图，不读不写跨假人
        /// 共享世界态（同 T-29 纪律）。</summary>
        private string M3AttackStateJson(int k)
        {
            // ⚠ `MShare.MySelf` 是**进程级共享静态**（多假人同进程时被最后处理包的假人覆盖）⇒
            // 不能用来标识「本假人」。本记录只用**每假人字段**：`_probeX/_probeY`（= SM_LOGON 落点、
            // 也就是本假人 CM_HIT 里 Recog 的坐标，服务端要求与 CurrX/CurrY 相等才受理）、
            // `_myActorId`（SM_LOGON 的 Recog）、`MShare.g_nAttackDir`（夹具固定朝向），
            // 并在自己的 `PlayScene.ActorList` 里按 `_myActorId` 找自己的对象取 HP 等。
            short ax = (short)_probeX;
            short ay = (short)_probeY;
            int adir = MShare.g_nAttackDir;
            int adeath = 0;
            int ahp = 0;
            int amaxhp = 0;
            Actor selfActor = null;
            if (PlayScene != null && PlayScene.ActorList != null)
            {
                for (int i = 0; i < PlayScene.ActorList.Count; i++)
                {
                    Actor a = PlayScene.ActorList[i];
                    if (a != null && a.RecogId == _myActorId) { selfActor = a; break; }
                }
            }
            if (selfActor != null)
            {
                adeath = selfActor.Death ? 1 : 0;
                ahp = selfActor.Abil != null ? selfActor.Abil.HP : 0;
                amaxhp = selfActor.Abil != null ? selfActor.Abil.MaxHP : 0;
            }
            // 前方格：唯一口径 = oracle `BaseObject.GetFrontPosition` 的 8 向位移（用夹具固定朝向；
            // 与 oracle `AttackDir` 在 `BaseObject.AttackDir` 的 `Dir = nDir` 之前取目标一致）。
            short fx = ax;
            short fy = ay;
            switch (adir)
            {
                case 0: fy--; break;
                case 1: fx++; fy--; break;
                case 2: fx++; break;
                case 3: fx++; fy++; break;
                case 4: fy++; break;
                case 5: fx--; fy++; break;
                case 6: fx--; break;
                case 7: fx--; fy--; break;
            }
            Actor vic = null;
            var others = new System.Text.StringBuilder();
            if (PlayScene != null && PlayScene.ActorList != null)
            {
                for (int i = 0; i < PlayScene.ActorList.Count; i++)
                {
                    Actor a = PlayScene.ActorList[i];
                    if (a == null || a.RecogId == _myActorId)
                    {
                        continue;
                    }
                    if (vic == null && a.CurrX == fx && a.CurrY == fy && !a.Death)
                    {
                        vic = a;
                    }
                    if (a.Race == ActorRace.Play)
                    {
                        if (others.Length > 0) others.Append(',');
                        others.Append($"{{\"id\":{a.RecogId},\"x\":{a.CurrX},\"y\":{a.CurrY},\"dir\":{a.m_btDir},\"death\":{(a.Death ? 1 : 0)},\"hp\":{(a.Abil != null ? a.Abil.HP : 0)}}}");
                    }
                }
            }
            string vicJson = "null";
            int vid = 0;
            if (vic != null)
            {
                vid = vic.RecogId;
                int vhp = vic.Abil != null ? vic.Abil.HP : 0;
                int vmaxhp = vic.Abil != null ? vic.Abil.MaxHP : 0;
                vicJson = $"{{\"x\":{vic.CurrX},\"y\":{vic.CurrY},\"dir\":{vic.m_btDir},\"ghost\":0,\"death\":{(vic.Death ? 1 : 0)},\"hp\":{vhp},\"maxhp\":{vmaxhp},\"id\":{vic.RecogId},\"race\":{vic.Race}}}";
            }
            return $"{{\"role\":\"state\",\"k\":{k},\"attacker\":{{\"x\":{ax},\"y\":{ay},\"dir\":{adir},\"ghost\":0,\"death\":{adeath},\"hp\":{ahp},\"maxhp\":{amaxhp},\"id\":{_myActorId}}},\"victim\":{vicJson},\"fx\":{fx},\"fy\":{fy},\"vid\":{vid},\"others\":[{others}]}}";
        }

        #endregion

        #region M3⑤ 固定 NPC/商店动作序列（夹具专用，默认关；白名单 T-29 家族扩展）

        // 口径（卡 openmir2-m3-shop-fixture-botsrv）：收到本假人 SM_LOGON 后，按 `g_sShopOps` 的固定顺序
        // 对**本假人视图中**的同一个 NPC/商人各发一次动作（开面板 + 买/卖/修理/查询/存取），逐 op 落
        // `M3SHOPSEQ {json}` 的 send/frame/end/done 事件（供 `npc_shop_diff.py assemble` 合并成记录）。
        // 只读本假人自己的视图（`PlayScene.ActorList` / `MShare.ItemArr`）、只写本假人自己的 socket
        // （同 T-22/T-29 纪律）；默认关时本 region 全部空转、不新增任何发送与日志。

        private string[] _shopSeqOps;
        private int _shopSeqStep = -1;
        private int _shopSeqNextTick;
        private int _shopSeqPendingK = -1;
        private readonly List<string> _shopSeqFrames = new List<string>();
        private int _shopSeqNpcActor;
        private string _shopSeqItemName = string.Empty;
        private int _shopSeqItemMake;

        /// <summary>收到本假人 SM_LOGON 后武装固定商店序列（`g_boShopSequence` 关闭时是空操作）。</summary>
        private void ArmShopSequence()
        {
            if (!MShare.g_boShopSequence)
            {
                return;
            }
            _shopSeqOps = MShare.ParseShopOps(MShare.g_sShopOps);
            _shopSeqStep = 0;
            _shopSeqPendingK = -1;
            _shopSeqFrames.Clear();
            _shopSeqNextTick = MShare.GetTickCount() + MShare.g_nShopStartDelayMs;
            MainOutMessage("M3SHOPSEQ {\"phase\":\"arm\",\"ops\":" + ShopJsonArray(_shopSeqOps)
                + ",\"step_ms\":" + MShare.g_nShopStepMs
                + ",\"npc\":\"" + ShopEsc(MShare.g_sShopNpcName) + "\"}");
        }

        /// <summary>M3⑤ 固定商店序列（步进式）：每 `g_nShopStepMs` 处理一个 op；
        /// 上一步先收尾（`phase=end`，聚合本步收到的帧 + 金币/背包前后态 + branch），再发下一步；
        /// 全部发完发 `phase=done`。默认关时空转。</summary>
        private void ShopSequenceTick()
        {
            if (!MShare.g_boShopSequence || _shopSeqOps == null || _shopSeqStep < 0)
            {
                return;
            }
            if (!_inWorld || ClientSocket == null || !ClientSocket.IsConnected)
            {
                return;
            }
            int now = MShare.GetTickCount();
            if (now < _shopSeqNextTick)
            {
                return;
            }
            if (_shopSeqPendingK >= 0)
            {
                EmitShopEnd(_shopSeqPendingK);
                _shopSeqPendingK = -1;
            }
            if (_shopSeqStep >= _shopSeqOps.Length)
            {
                _shopSeqStep = -1;
                MainOutMessage("M3SHOPSEQ {\"phase\":\"done\",\"ops\":" + _shopSeqOps.Length + "}");
                return;
            }
            int k = _shopSeqStep;
            string op = _shopSeqOps[k];
            _shopSeqStep++;
            _shopSeqFrames.Clear();
            ResolveShopItem();
            EmitShopSend(k, op);
            SendShopOp(op);
            _shopSeqPendingK = k;
            _shopSeqNextTick = now + MShare.g_nShopStepMs;
        }

        private void EmitShopSend(int k, string op)
        {
            Actor npc = FindShopNpc();
            _shopSeqNpcActor = npc != null ? npc.RecogId : 0;
            string npcName = npc != null ? npc.UserName : string.Empty;
            int gold = ShopGold();
            MainOutMessage("M3SHOPSEQ {\"phase\":\"send\",\"k\":" + k
                + ",\"op\":\"" + op + "\",\"npc\":\"" + ShopEsc(npcName)
                + "\",\"npc_actor\":" + _shopSeqNpcActor
                + ",\"req\":" + ShopReqJson(op)
                + ",\"gold_before\":" + gold
                + ",\"bag_before\":" + ShopBagJson() + "}");
        }

        private void EmitShopEnd(int k)
        {
            MainOutMessage("M3SHOPSEQ {\"phase\":\"end\",\"k\":" + k
                + ",\"gold_after\":" + ShopGold()
                + ",\"bag_after\":" + ShopBagJson()
                + ShopPriceFromFrames()
                + ",\"branch\":\"" + ShopBranchFromFrames() + "\"}");
        }

        /// <summary>发本 op 的 CM_*（先开对应面板，再发动作）。npc 不在视图里 ⇒ 只记 skip，不发。</summary>
        private void SendShopOp(string op)
        {
            if (_shopSeqNpcActor == 0)
            {
                MainOutMessage("M3SHOPSEQ op=" + op + " skip=no_npc_in_view");
                return;
            }
            short count = (short)MShare.g_nShopCount;
            switch (op)
            {
                case "sell":
                    SendMerchantDlgSelect(_shopSeqNpcActor, "@sell");
                    SendSellItem(_shopSeqNpcActor, _shopSeqItemMake, _shopSeqItemName, count);
                    break;
                case "buy":
                    SendMerchantDlgSelect(_shopSeqNpcActor, "@buy");
                    SendBuyItem(_shopSeqNpcActor, _shopSeqItemMake, _shopSeqItemName, count);
                    break;
                case "query_sell_price":
                    SendMerchantDlgSelect(_shopSeqNpcActor, "@sell");
                    SendQueryPrice(_shopSeqNpcActor, _shopSeqItemMake, _shopSeqItemName);
                    break;
                case "query_repair_cost":
                    SendMerchantDlgSelect(_shopSeqNpcActor, "@repair");
                    SendQueryRepairCost(_shopSeqNpcActor, _shopSeqItemMake, _shopSeqItemName);
                    break;
                case "repair":
                    SendMerchantDlgSelect(_shopSeqNpcActor, "@repair");
                    SendRepairItem(_shopSeqNpcActor, _shopSeqItemMake, _shopSeqItemName);
                    break;
                case "storage":
                    SendMerchantDlgSelect(_shopSeqNpcActor, "@storage");
                    SendStorageItem(_shopSeqNpcActor, _shopSeqItemMake, _shopSeqItemName, count);
                    break;
                case "takeback":
                    SendMerchantDlgSelect(_shopSeqNpcActor, "@getback");
                    SendTakeBackStorageItem(_shopSeqNpcActor, _shopSeqItemMake, _shopSeqItemName, count);
                    break;
            }
        }

        /// <summary>从本假人自己的背包视图取动作物品（名字/MakeIndex）。
        /// 名字为空 ⇒ 取第一件；找不到（如刚卖掉/在货架上）⇒ 保留配置值（供买回用）。</summary>
        private void ResolveShopItem()
        {
            string name = MShare.g_sShopItemName;
            int make = MShare.g_nShopItemMake;
            if (MShare.ItemArr != null)
            {
                for (int i = 0; i < BotConst.MaxBagItemcl; i++)
                {
                    ClientItem it = MShare.ItemArr[i];
                    if (it == null || string.IsNullOrEmpty(it.Item.Name))
                    {
                        continue;
                    }
                    if (!string.IsNullOrEmpty(name) && !string.Equals(it.Item.Name, name, StringComparison.Ordinal))
                    {
                        continue;
                    }
                    if (make != 0 && it.MakeIndex != make)
                    {
                        continue;
                    }
                    _shopSeqItemName = it.Item.Name;
                    _shopSeqItemMake = it.MakeIndex;
                    return;
                }
            }
            _shopSeqItemName = name;
            _shopSeqItemMake = make;
        }

        /// <summary>本假人视图里按名（或第一个）找 NPC/商人。只用本假人自己的 `PlayScene.ActorList`。</summary>
        private Actor FindShopNpc()
        {
            if (PlayScene == null || PlayScene.ActorList == null)
            {
                return null;
            }
            string want = MShare.g_sShopNpcName;
            Actor fallback = null;
            for (int i = 0; i < PlayScene.ActorList.Count; i++)
            {
                Actor a = PlayScene.ActorList[i];
                if (a == null || a.RecogId == _myActorId)
                {
                    continue;
                }
                bool isNpc = a.Race == ActorRace.NPC || a.Race == ActorRace.Guard
                    || a.Race == ActorRace.PeaceNpc || a.Race == ActorRace.Merchant
                    || a.Race == ActorRace.Exercise;
                if (!isNpc)
                {
                    continue;
                }
                if (!string.IsNullOrEmpty(want))
                {
                    if (string.Equals(a.UserName, want, StringComparison.Ordinal))
                    {
                        return a;
                    }
                    continue;
                }
                if (fallback == null)
                {
                    fallback = a;
                }
            }
            return fallback;
        }

        /// <summary>本假人金币（自己的 actor；拿不到就回落 `MShare.MySelf`）。</summary>
        private int ShopGold()
        {
            if (PlayScene != null && PlayScene.ActorList != null)
            {
                for (int i = 0; i < PlayScene.ActorList.Count; i++)
                {
                    Actor a = PlayScene.ActorList[i];
                    if (a != null && a.RecogId == _myActorId)
                    {
                        return a.m_nGold;
                    }
                }
            }
            return MShare.MySelf != null ? MShare.MySelf.m_nGold : 0;
        }

        /// <summary>本假人背包多重集（稳定键串 `名字#MakeIndex`；空槽跳过）。</summary>
        private string ShopBagJson()
        {
            var parts = new List<string>();
            if (MShare.ItemArr != null)
            {
                for (int i = 0; i < BotConst.MaxBagItemcl; i++)
                {
                    ClientItem it = MShare.ItemArr[i];
                    if (it == null || string.IsNullOrEmpty(it.Item.Name))
                    {
                        continue;
                    }
                    parts.Add("\"" + ShopEsc(it.Item.Name) + "#" + it.MakeIndex + "\"");
                }
            }
            return "[" + string.Join(",", parts) + "]";
        }

        private string ShopReqJson(string op)
        {
            string s = "{\"item\":\"" + ShopEsc(_shopSeqItemName) + "\",\"n\":" + MShare.g_nShopCount
                + ",\"label\":\"" + op + "\"";
            if (_shopSeqItemMake != 0)
            {
                s += ",\"make\":" + _shopSeqItemMake;
            }
            return s + "}";
        }

        /// <summary>失败分支归因（唯一口径 = `MShare.ClassifyShopFrames`，与 `--selftest-shopseq` 共用）。</summary>
        private string ShopBranchFromFrames()
        {
            return MShare.ClassifyShopFrames(_shopSeqFrames);
        }

        /// <summary>从帧 647（卖价）/ 671（修理价）取价格，输出 `,"price":N` 或空串。
        /// 唯一口径 = `MShare.ExtractShopPrice`。</summary>
        private string ShopPriceFromFrames()
        {
            int? p = MShare.ExtractShopPrice(_shopSeqFrames);
            return p.HasValue ? ",\"price\":" + p.Value : string.Empty;
        }

        /// <summary>收帧钩子：SM_* 的商店帧在 `g_boShopSequence` 打开时落 `phase=frame` 事件。
        /// **只读不写世界态**（同 T-29 纪律）；默认关时空转。</summary>
        public void ShopOnFrame(int sm, int a, int b, int c, int d)
        {
            if (!MShare.g_boShopSequence || _shopSeqPendingK < 0)
            {
                return;
            }
            string fr = "{\"sm\":" + sm + ",\"a\":" + a + ",\"b\":" + b + ",\"c\":" + c + ",\"d\":" + d + "}";
            _shopSeqFrames.Add(fr);
            MainOutMessage("M3SHOPSEQ {\"phase\":\"frame\",\"k\":" + _shopSeqPendingK + ",\"frame\":" + fr + "}");
        }

        private static string ShopJsonArray(string[] ops)
        {
            var parts = new List<string>();
            if (ops != null)
            {
                foreach (string op in ops)
                {
                    parts.Add("\"" + ShopEsc(op) + "\"");
                }
            }
            return "[" + string.Join(",", parts) + "]";
        }

        private static string ShopEsc(string s)
        {
            if (string.IsNullOrEmpty(s))
            {
                return string.Empty;
            }
            return s.Replace("\\", "\\\\").Replace("\"", "\\\"");
        }

        #endregion

        #region M3⑥ 固定任务动作序列（夹具专用，默认关；白名单 T-41 家族扩展）

        // 口径（卡 openmir2-m3-quest-fixture-botsrv）：收到本假人 SM_LOGON 后，按 `g_sQuestSteps` 的
        // 固定步骤串（`op|npc|label`）「点任务 NPC（CM_CLICKNPC 1010）/ 选脚本标签（CM_MERCHANTDLGSELECT）/
        // 核对说词（SM_MERCHANTSAY 643 / SM_MENU_OK 767）」，逐 step 落 `M3QUESTSEQ {json}` 的
        // send/frame/end/done 事件（供 `quest_diff.py assemble` 合并成记录）。只读本假人自己的视图
        // （`PlayScene.ActorList` / `MShare.ItemArr`）、只写本假人自己的 socket（同 T-22/T-29 纪律）；
        // 默认关时本 region 全部空转、不新增任何发送与日志。

        private string[] _questSeqSteps;
        private int _questSeqStep = -1;
        private int _questSeqNextTick;
        private int _questSeqPendingK = -1;
        private readonly List<string> _questSeqFrames = new List<string>();
        private int _questSeqNpcActor;
        private string _questSeqOp = string.Empty;
        private string _questSeqNpcName = string.Empty;
        private string _questSeqLabel = string.Empty;

        /// <summary>收到本假人 SM_LOGON 后武装固定任务序列（`g_boQuestSequence` 关闭时是空操作）。</summary>
        private void ArmQuestSequence()
        {
            if (!MShare.g_boQuestSequence)
            {
                return;
            }
            _questSeqSteps = MShare.ParseQuestSteps(MShare.g_sQuestSteps);
            _questSeqStep = 0;
            _questSeqPendingK = -1;
            _questSeqFrames.Clear();
            _questSeqNextTick = MShare.GetTickCount() + MShare.g_nQuestStartDelayMs;
            MainOutMessage("M3QUESTSEQ {\"phase\":\"arm\",\"steps\":" + QuestJsonArray(_questSeqSteps)
                + ",\"step_ms\":" + MShare.g_nQuestStepMs + "}");
        }

        /// <summary>M3⑥ 固定任务序列（步进式）：每 `g_nQuestStepMs` 处理一步；上一步先收尾（`phase=end`，
        /// 聚合本步收到的帧 + 金币/背包前后态 + branch），再发下一步；全部发完发 `phase=done`。默认关时空转。</summary>
        private void QuestSequenceTick()
        {
            if (!MShare.g_boQuestSequence || _questSeqSteps == null || _questSeqStep < 0)
            {
                return;
            }
            if (!_inWorld || ClientSocket == null || !ClientSocket.IsConnected)
            {
                return;
            }
            int now = MShare.GetTickCount();
            if (now < _questSeqNextTick)
            {
                return;
            }
            if (_questSeqPendingK >= 0)
            {
                EmitQuestEnd(_questSeqPendingK);
                _questSeqPendingK = -1;
            }
            if (_questSeqStep >= _questSeqSteps.Length)
            {
                _questSeqStep = -1;
                MainOutMessage("M3QUESTSEQ {\"phase\":\"done\",\"steps\":" + _questSeqSteps.Length + "}");
                return;
            }
            int k = _questSeqStep;
            string step = _questSeqSteps[k];
            _questSeqStep++;
            _questSeqFrames.Clear();
            ResolveQuestStep(step);
            EmitQuestSend(k);
            SendQuestStep();
            _questSeqPendingK = k;
            _questSeqNextTick = now + MShare.g_nQuestStepMs;
        }

        /// <summary>解析一步 `op|npc|label`，并在本假人视图里找 NPC。</summary>
        private void ResolveQuestStep(string step)
        {
            string[] f = step.Split('|');
            _questSeqOp = f.Length == 3 ? f[0] : "click_npc";
            string want = f.Length >= 2 ? f[1] : string.Empty;
            _questSeqLabel = f.Length == 3 ? f[2] : "@main";
            Actor npc = FindQuestNpc(want);
            _questSeqNpcActor = npc != null ? npc.RecogId : 0;
            _questSeqNpcName = npc != null ? npc.UserName : want;
        }

        private void EmitQuestSend(int k)
        {
            MainOutMessage("M3QUESTSEQ {\"phase\":\"send\",\"k\":" + k
                + ",\"op\":\"" + QuestEsc(_questSeqOp)
                + "\",\"npc\":\"" + QuestEsc(_questSeqNpcName)
                + "\",\"npc_actor\":" + _questSeqNpcActor
                + ",\"map\":\"" + QuestEsc(QuestMapName()) + "\""
                + ",\"label\":\"" + QuestEsc(_questSeqLabel) + "\""
                + ",\"req\":{\"item\":\"\",\"n\":0}"
                + ",\"gold_before\":" + ShopGold()
                + ",\"bag_before\":" + ShopBagJson() + "}");
        }

        private void EmitQuestEnd(int k)
        {
            MainOutMessage("M3QUESTSEQ {\"phase\":\"end\",\"k\":" + k
                + ",\"gold_after\":" + ShopGold()
                + ",\"bag_after\":" + ShopBagJson()
                + ",\"branch\":\"" + MShare.ClassifyQuestFrames(_questSeqFrames) + "\"}");
        }

        /// <summary>发本步的 CM_*（click_npc ⇒ CM_CLICKNPC；dialog_select ⇒ CM_MERCHANTDLGSELECT + label）。
        /// npc 不在视图里 ⇒ 只记 skip，不发。take/give/set/check 本轮仅登记、不实发。</summary>
        private void SendQuestStep()
        {
            if (_questSeqNpcActor == 0)
            {
                MainOutMessage("M3QUESTSEQ skip=no_npc_in_view step=" + QuestEsc(_questSeqOp + "|" + _questSeqNpcName + "|" + _questSeqLabel));
                return;
            }
            switch (_questSeqOp)
            {
                case "click_npc":
                    SendClickNpc(_questSeqNpcActor);
                    break;
                case "dialog_select":
                    SendMerchantDlgSelect(_questSeqNpcActor, _questSeqLabel);
                    break;
                default:
                    MainOutMessage("M3QUESTSEQ skip=op_not_implemented step=" + QuestEsc(_questSeqOp));
                    break;
            }
        }

        /// <summary>发 CM_CLICKNPC(1010)（= 点 NPC；服务端 `ClientClickNpc` → `GotoLable("@main")`）。
        /// 夹具用；默认关时不会被调用。</summary>
        private void SendClickNpc(int actorId)
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_CLICKNPC, actorId, 0, 0, 0);
            SendSocket(EDCode.EncodeMessage(msg));
        }

        /// <summary>本假人视图里按名（或第一个）找 NPC/商人。只用本假人自己的 `PlayScene.ActorList`。</summary>
        private Actor FindQuestNpc(string want)
        {
            if (PlayScene == null || PlayScene.ActorList == null)
            {
                return null;
            }
            Actor fallback = null;
            for (int i = 0; i < PlayScene.ActorList.Count; i++)
            {
                Actor a = PlayScene.ActorList[i];
                if (a == null || a.RecogId == _myActorId)
                {
                    continue;
                }
                bool isNpc = a.Race == ActorRace.NPC || a.Race == ActorRace.Guard
                    || a.Race == ActorRace.PeaceNpc || a.Race == ActorRace.Merchant
                    || a.Race == ActorRace.Exercise;
                if (!isNpc)
                {
                    continue;
                }
                if (!string.IsNullOrEmpty(want))
                {
                    if (string.Equals(a.UserName, want, StringComparison.Ordinal))
                    {
                        return a;
                    }
                    continue;
                }
                if (fallback == null)
                {
                    fallback = a;
                }
            }
            return fallback;
        }

        private string QuestMapName()
        {
            return MShare.MapTitle ?? string.Empty;
        }

        /// <summary>收帧钩子（单一收包点）：`SM_MERCHANTSAY(643)` / `SM_MENU_OK(767)` / `SM_DLGMSG(772)`
        /// 在 `g_boQuestSequence` 打开且本步待收尾时落 `phase=frame` 事件（含说词文本）。
        /// **只读不写世界态**（同 T-29 纪律）；默认关时空转。</summary>
        public void QuestOnServerMessage(int sm, int merchant, int face, int tag, int series, string body)
        {
            if (!MShare.g_boQuestSequence || _questSeqPendingK < 0)
            {
                return;
            }
            if (sm != 643 && sm != 767 && sm != 772)
            {
                return;
            }
            string text = string.IsNullOrEmpty(body) ? string.Empty : EDCode.DeCodeString(body);
            string fr = "{\"sm\":" + sm + ",\"merchant\":" + merchant + ",\"face\":" + face
                + ",\"tag\":" + tag + ",\"series\":" + series
                + ",\"text\":\"" + QuestEsc(text) + "\"}";
            _questSeqFrames.Add(fr);
            MainOutMessage("M3QUESTSEQ {\"phase\":\"frame\",\"k\":" + _questSeqPendingK + ",\"frame\":" + fr + "}");
        }

        private static string QuestJsonArray(string[] steps)
        {
            var parts = new List<string>();
            if (steps != null)
            {
                foreach (string s in steps)
                {
                    parts.Add("\"" + QuestEsc(s) + "\"");
                }
            }
            return "[" + string.Join(",", parts) + "]";
        }

        private static string QuestEsc(string s)
        {
            if (string.IsNullOrEmpty(s))
            {
                return string.Empty;
            }
            return s.Replace("\\", "\\\\").Replace("\"", "\\\"")
                .Replace("\r", "\\r").Replace("\n", "\\n").Replace("\t", "\\t");
        }

        #endregion

        #region M3⑦ 组队/交易动作序列夹具（默认关；白名单 T-43 家族扩展）

        private string[] _ptSeqSteps;
        private int _ptSeqStep = -1;
        private int _ptSeqNextTick;
        private int _ptSeqPendingK = -1;
        private readonly List<string> _ptSeqFrames = new List<string>();
        private string _ptSeqOp = string.Empty;
        private string _ptSeqRole = string.Empty;
        private string _ptSeqParam = string.Empty;
        private bool _ptSeqRan;

        /// <summary>收到本假人 SM_LOGON 后武装固定组队/交易序列（`g_boPairTradeSequence` 关闭时是空操作）。</summary>
        private void ArmPairTradeSequence()
        {
            if (!MShare.g_boPairTradeSequence)
            {
                return;
            }
            _ptSeqSteps = MShare.ParsePairTradeSteps(MShare.g_sPairTradeSteps);
            _ptSeqStep = 0;
            _ptSeqPendingK = -1;
            _ptSeqFrames.Clear();
            _ptSeqNextTick = MShare.GetTickCount() + MShare.g_nPairTradeStartDelayMs;
            MainOutMessage("M3PAIRTRADESEQ {\"phase\":\"arm\",\"bot\":\"" + QuestEsc(LoginId)
                + "\",\"role\":\"" + (MShare.IsPairTradeActor(LoginId, MShare.g_sPairTradeActorLoginId) ? "a" : "b")
                + "\",\"steps\":" + QuestJsonArray(_ptSeqSteps)
                + ",\"step_ms\":" + MShare.g_nPairTradeStepMs + "}");
        }

        /// <summary>M3⑦ 固定组队/交易序列（步进式）：每 `g_nPairTradeStepMs` 处理一步；上一步先收尾
        /// （`phase=end`，聚合本步收到的帧 + 金币/背包前后态 + branch），再发下一步；全部发完发 `phase=done`。
        /// **两个**假人都跑同一串步骤；角色不符的步只记 `ran=false` + `skip_other_role`（k 仍前进，保证两侧同 k）。</summary>
        private void PairTradeSequenceTick()
        {
            if (!MShare.g_boPairTradeSequence || _ptSeqSteps == null || _ptSeqStep < 0)
            {
                return;
            }
            if (!_inWorld || ClientSocket == null || !ClientSocket.IsConnected)
            {
                return;
            }
            int now = MShare.GetTickCount();
            if (now < _ptSeqNextTick)
            {
                return;
            }
            if (_ptSeqPendingK >= 0)
            {
                EmitPairTradeEnd(_ptSeqPendingK);
                _ptSeqPendingK = -1;
            }
            if (_ptSeqStep >= _ptSeqSteps.Length)
            {
                _ptSeqStep = -1;
                MainOutMessage("M3PAIRTRADESEQ {\"phase\":\"done\",\"bot\":\"" + QuestEsc(LoginId)
                    + "\",\"steps\":" + _ptSeqSteps.Length + "}");
                return;
            }
            int k = _ptSeqStep;
            string step = _ptSeqSteps[k];
            _ptSeqStep++;
            _ptSeqFrames.Clear();
            _ptSeqOp = MShare.PairTradeStepOp(step);
            _ptSeqRole = MShare.PairTradeStepRole(step);
            _ptSeqParam = MShare.PairTradeStepParam(step);
            bool isActor = MShare.IsPairTradeActor(LoginId, MShare.g_sPairTradeActorLoginId);
            _ptSeqRan = MShare.PairTradeRunsForRole(_ptSeqRole, isActor);
            EmitPairTradeSend(k);
            SendPairTradeStep();
            _ptSeqPendingK = k;
            _ptSeqNextTick = now + MShare.g_nPairTradeStepMs;
        }

        private void EmitPairTradeSend(int k)
        {
            MainOutMessage("M3PAIRTRADESEQ {\"phase\":\"send\",\"bot\":\"" + QuestEsc(LoginId)
                + "\",\"k\":" + k
                + ",\"role\":\"" + QuestEsc(_ptSeqRole)
                + "\",\"op\":\"" + QuestEsc(_ptSeqOp)
                + "\",\"ran\":" + (_ptSeqRan ? "true" : "false")
                + ",\"param\":\"" + QuestEsc(ResolvePairTradeParam(_ptSeqParam)) + "\""
                + ",\"map\":\"" + QuestEsc(QuestMapName()) + "\""
                + ",\"gold_before\":" + ShopGold()
                + ",\"bag_before\":" + ShopBagJson() + "}");
        }

        private void EmitPairTradeEnd(int k)
        {
            string branch = _ptSeqRan ? MShare.ClassifyPairTradeFrames(_ptSeqFrames) : "skip_other_role";
            MainOutMessage("M3PAIRTRADESEQ {\"phase\":\"end\",\"bot\":\"" + QuestEsc(LoginId)
                + "\",\"k\":" + k
                + ",\"gold_after\":" + ShopGold()
                + ",\"bag_after\":" + ShopBagJson()
                + ",\"branch\":\"" + branch + "\"}");
        }

        /// <summary>M3⑦：把步骤参数里的 `peer` 记号换成**对手**角色名（LoginAccount+PeerRole）。
        /// 其余参数原样返回。纯函数式（不改状态）。</summary>
        private string ResolvePairTradeParam(string param)
        {
            if (string.Equals(param, "peer", StringComparison.OrdinalIgnoreCase))
            {
                return MShare.g_sPairTradePeerLoginId;
            }
            return param ?? string.Empty;
        }

        /// <summary>本假人背包里取动作物品（名字/MakeIndex）；名字为空 ⇒ 第一件。找不到 ⇒ null。</summary>
        private ClientItem FindPairTradeItem(string name, int make)
        {
            if (MShare.ItemArr == null)
            {
                return null;
            }
            for (int i = 0; i < BotConst.MaxBagItemcl; i++)
            {
                ClientItem it = MShare.ItemArr[i];
                if (it == null || string.IsNullOrEmpty(it.Item.Name))
                {
                    continue;
                }
                if (!string.IsNullOrEmpty(name) && !string.Equals(it.Item.Name, name, StringComparison.Ordinal))
                {
                    continue;
                }
                if (make != 0 && it.MakeIndex != make)
                {
                    continue;
                }
                return it;
            }
            return null;
        }

        /// <summary>发本步的 CM_*（角色不符 ⇒ 不发；op 未知 ⇒ 只记 skip）。</summary>
        private void SendPairTradeStep()
        {
            if (!_ptSeqRan)
            {
                return;
            }
            string param = ResolvePairTradeParam(_ptSeqParam);
            switch (_ptSeqOp)
            {
                case "group_mode_on":
                    SendGroupMode(true);
                    break;
                case "group_mode_off":
                    SendGroupMode(false);
                    break;
                case "create_group":
                    SendCreateGroup(param);
                    break;
                case "add_member":
                    SendAddGroupMember(param);
                    break;
                case "del_member":
                    SendDelGroupMember(param);
                    break;
                case "deal_try":
                    SendDealTry();
                    break;
                case "deal_add_item":
                    {
                        ClientItem it = FindPairTradeItem(string.Empty, 0);
                        if (it != null) { SendAddDealItem(it); }
                        else { MainOutMessage("M3PAIRTRADESEQ skip=no_item_in_bag op=deal_add_item"); }
                    }
                    break;
                case "deal_del_item":
                    {
                        ClientItem it = FindPairTradeItem(string.Empty, 0);
                        if (it != null) { SendDelDealItem(it); }
                        else { MainOutMessage("M3PAIRTRADESEQ skip=no_item_in_bag op=deal_del_item"); }
                    }
                    break;
                case "deal_chg_gold":
                    {
                        int gold = 0;
                        int.TryParse(param, out gold);
                        SendChangeDealGold(gold);
                    }
                    break;
                case "deal_end":
                    SendDealEnd();
                    break;
                case "deal_cancel":
                    SendCancelDeal();
                    break;
                case "softclose":
                    SendClientMessage(Messages.CM_SOFTCLOSE, 0, 0, 0, 0);
                    break;
                default:
                    MainOutMessage("M3PAIRTRADESEQ skip=op_not_implemented step=" + QuestEsc(_ptSeqOp));
                    break;
            }
        }

        /// <summary>收帧钩子（单一收包点）：组队/交易相关 SM_* 在 `g_boPairTradeSequence` 打开且本步待收尾时
        /// 落 `phase=frame` 事件。**只读不写世界态**（同 T-29/T-43 纪律）；默认关时空转。</summary>
        public void PairTradeOnServerMessage(int sm, int merchant, int face, int tag, int series, string body)
        {
            if (!MShare.g_boPairTradeSequence || _ptSeqPendingK < 0)
            {
                return;
            }
            switch (sm)
            {
                case 101:   // SM_GROUPMESSAGE
                case 659:   // SM_GROUPMODECHANGED
                case 660: case 661:
                case 662: case 663: case 664: case 665:
                case 666: case 667:
                case 673: case 674:
                case 675: case 676: case 677: case 678:
                case 681: case 682: case 683:
                case 684: case 685: case 686: case 687:
                    break;
                default:
                    return;
            }
            string text = sm == 101 && !string.IsNullOrEmpty(body) ? EDCode.DeCodeString(body) : string.Empty;
            string fr = "{\"sm\":" + sm + ",\"a\":" + merchant + ",\"b\":" + face
                + ",\"c\":" + tag + ",\"d\":" + series
                + ",\"text\":\"" + QuestEsc(text) + "\"}";
            _ptSeqFrames.Add(fr);
            MainOutMessage("M3PAIRTRADESEQ {\"phase\":\"frame\",\"bot\":\"" + QuestEsc(LoginId)
                + "\",\"k\":" + _ptSeqPendingK + ",\"frame\":" + fr + "}");
        }

        #endregion

        #region M3⑩ 元宝寄售动作序列夹具（默认关；白名单 T-44 家族扩展）

        private string[] _soSeqSteps;
        private int _soSeqStep = -1;
        private int _soSeqNextTick;
        private int _soSeqPendingK = -1;
        private readonly List<string> _soSeqFrames = new List<string>();
        private string _soSeqOp = string.Empty;
        private string _soSeqRole = string.Empty;
        private string _soSeqParam = string.Empty;
        private bool _soSeqRan;

        /// <summary>收到本假人 SM_LOGON 后武装固定元宝寄售序列（`g_boSellOffSequence` 关闭时是空操作）。</summary>
        private void ArmSellOffSequence()
        {
            if (!MShare.g_boSellOffSequence)
            {
                return;
            }
            _soSeqSteps = MShare.ParseSellOffSteps(MShare.g_sSellOffSteps);
            _soSeqStep = 0;
            _soSeqPendingK = -1;
            _soSeqFrames.Clear();
            _soSeqNextTick = MShare.GetTickCount() + MShare.g_nSellOffStartDelayMs;
            MainOutMessage("M3SELLOFFSEQ {\"phase\":\"arm\",\"bot\":\"" + QuestEsc(LoginId)
                + "\",\"role\":\"" + (MShare.IsSellOffSeller(LoginId, MShare.g_sSellOffSellerLoginId) ? "a" : "b")
                + "\",\"steps\":" + QuestJsonArray(_soSeqSteps)
                + ",\"step_ms\":" + MShare.g_nSellOffStepMs + "}");
        }

        /// <summary>M3⑩ 固定元宝寄售序列（步进式）：每 `g_nSellOffStepMs` 处理一步；上一步先收尾
        /// （`phase=end`，聚合本步收到的帧 + 元宝/金币/背包前后态 + branch），再发下一步；全部发完发 `phase=done`。
        /// **两个**假人（卖方/买方）都跑同一串步骤；角色不符的步只记 `ran=false` + `skip_other_role`（k 仍前进）。</summary>
        private void SellOffSequenceTick()
        {
            if (!MShare.g_boSellOffSequence || _soSeqSteps == null || _soSeqStep < 0)
            {
                return;
            }
            if (!_inWorld || ClientSocket == null || !ClientSocket.IsConnected)
            {
                return;
            }
            int now = MShare.GetTickCount();
            if (now < _soSeqNextTick)
            {
                return;
            }
            if (_soSeqPendingK >= 0)
            {
                EmitSellOffEnd(_soSeqPendingK);
                _soSeqPendingK = -1;
            }
            if (_soSeqStep >= _soSeqSteps.Length)
            {
                _soSeqStep = -1;
                MainOutMessage("M3SELLOFFSEQ {\"phase\":\"done\",\"bot\":\"" + QuestEsc(LoginId)
                    + "\",\"steps\":" + _soSeqSteps.Length + "}");
                return;
            }
            int k = _soSeqStep;
            string step = _soSeqSteps[k];
            _soSeqStep++;
            _soSeqFrames.Clear();
            _soSeqOp = MShare.SellOffStepOp(step);
            _soSeqRole = MShare.SellOffStepRole(step);
            _soSeqParam = MShare.SellOffStepParam(step);
            bool isSeller = MShare.IsSellOffSeller(LoginId, MShare.g_sSellOffSellerLoginId);
            _soSeqRan = MShare.SellOffRunsForRole(_soSeqRole, isSeller);
            EmitSellOffSend(k);
            SendSellOffStep();
            _soSeqPendingK = k;
            _soSeqNextTick = now + MShare.g_nSellOffStepMs;
        }

        private void EmitSellOffSend(int k)
        {
            MainOutMessage("M3SELLOFFSEQ {\"phase\":\"send\",\"bot\":\"" + QuestEsc(LoginId)
                + "\",\"k\":" + k
                + ",\"role\":\"" + QuestEsc(_soSeqRole)
                + "\",\"op\":\"" + QuestEsc(_soSeqOp)
                + "\",\"ran\":" + (_soSeqRan ? "true" : "false")
                + ",\"param\":\"" + QuestEsc(ResolveSellOffParam()) + "\""
                + ",\"map\":\"" + QuestEsc(QuestMapName()) + "\""
                + ",\"gamegold_before\":" + SellOffGameGold()
                + ",\"gold_before\":" + ShopGold()
                + ",\"bag_before\":" + ShopBagJson() + "}");
        }

        private void EmitSellOffEnd(int k)
        {
            string branch = _soSeqRan ? MShare.ClassifySellOffFrames(_soSeqFrames) : "skip_other_role";
            MainOutMessage("M3SELLOFFSEQ {\"phase\":\"end\",\"bot\":\"" + QuestEsc(LoginId)
                + "\",\"k\":" + k
                + ",\"gamegold_after\":" + SellOffGameGold()
                + ",\"gold_after\":" + ShopGold()
                + ",\"bag_after\":" + ShopBagJson()
                + ",\"branch\":\"" + branch + "\"}");
        }

        /// <summary>本假人元宝（GameGold；`SM_GAMEGOLDNAME` 回包写入 `MShare.MySelf.m_nGameGold`）。</summary>
        private int SellOffGameGold()
        {
            return MShare.MySelf != null ? MShare.MySelf.m_nGameGold : 0;
        }

        /// <summary>M3⑩：把步骤参数里的 `peer` 记号换成**对方**角色名；`seller_end` 接受 `<tok>/<gold>`
        /// （tok=peer 买方 / self 自己 / 字面名）并返回规范化 `<buyer>/<gold>`。纯函数式（不改状态）。</summary>
        private string ResolveSellOffParam()
        {
            string raw = _soSeqParam ?? string.Empty;
            bool isSeller = MShare.IsSellOffSeller(LoginId, MShare.g_sSellOffSellerLoginId);
            if (_soSeqOp == "seller_end")
            {
                int gold = 1000;
                string tok = "peer";
                int slash = raw.IndexOf('/');
                if (slash >= 0)
                {
                    tok = raw.Substring(0, slash).Trim();
                    if (!int.TryParse(raw.Substring(slash + 1).Trim(), out gold) || gold <= 0)
                    {
                        gold = 1000;
                    }
                }
                else if (!int.TryParse(raw, out gold) || gold <= 0)
                {
                    gold = 1000;
                }
                string buyer = string.Equals(tok, "self", StringComparison.OrdinalIgnoreCase)
                    ? LoginId : MShare.g_sSellOffBuyerLoginId;
                return buyer + "/" + gold;
            }
            if (string.Equals(raw, "peer", StringComparison.OrdinalIgnoreCase))
            {
                return isSeller ? MShare.g_sSellOffBuyerLoginId : MShare.g_sSellOffSellerLoginId;
            }
            return raw;
        }

        /// <summary>本假人背包里取动作物品（名字/MakeIndex）；名字为空 ⇒ 第一件。找不到 ⇒ null。</summary>
        private ClientItem FindSellOffItem(string name, int make)
        {
            if (MShare.ItemArr == null)
            {
                return null;
            }
            for (int i = 0; i < BotConst.MaxBagItemcl; i++)
            {
                ClientItem it = MShare.ItemArr[i];
                if (it == null || string.IsNullOrEmpty(it.Item.Name))
                {
                    continue;
                }
                if (!string.IsNullOrEmpty(name) && !string.Equals(it.Item.Name, name, StringComparison.Ordinal))
                {
                    continue;
                }
                if (make != 0 && it.MakeIndex != make)
                {
                    continue;
                }
                return it;
            }
            return null;
        }

        /// <summary>发本步的 CM_*（角色不符 ⇒ 不发；op 未知 ⇒ 只记 skip）。</summary>
        private void SendSellOffStep()
        {
            if (!_soSeqRan)
            {
                return;
            }
            string raw = _soSeqParam ?? string.Empty;
            switch (_soSeqOp)
            {
                case "seller_add_item":
                case "seller_del_item":
                    {
                        ClientItem it = FindSellOffItem(string.Empty, 0);
                        if (it != null)
                        {
                            int cm = _soSeqOp == "seller_add_item"
                                ? Messages.CM_SELLOFFADDITEM : Messages.CM_SELLOFFDELITEM;
                            CommandMessage msg = Messages.MakeMessage(cm, it.MakeIndex, 0, 0, it.Dura);
                            SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(it.Item.Name));
                        }
                        else
                        {
                            MainOutMessage("M3SELLOFFSEQ skip=no_item_in_bag op=" + QuestEsc(_soSeqOp));
                        }
                    }
                    break;
                case "seller_end":
                    {
                        string norm = ResolveSellOffParam();
                        int slash = norm.IndexOf('/');
                        string buyer = slash >= 0 ? norm.Substring(0, slash) : MShare.g_sSellOffBuyerLoginId;
                        int gold = 1000;
                        if (slash >= 0) { int.TryParse(norm.Substring(slash + 1), out gold); }
                        CommandMessage msg = Messages.MakeMessage(Messages.CM_SELLOFFEND, gold, 0, 0, 0);
                        SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(buyer));
                    }
                    break;
                case "seller_cancel":
                    SendClientMessage(Messages.CM_SELLOFFCANCEL, 0, 0, 0, 0);
                    break;
                case "seller_cancel_ing":
                    SendClientMessage(Messages.CM_CANCELSELLOFFITEMING, 0, 0, 0, 0);
                    break;
                case "buyer_buy_cancel":
                    {
                        CommandMessage msg = Messages.MakeMessage(Messages.CM_SELLOFFBUYCANCEL, 0, 0, 0, 0);
                        SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(MShare.g_sSellOffSellerLoginId));
                    }
                    break;
                case "buyer_buy":
                    {
                        CommandMessage msg = Messages.MakeMessage(Messages.CM_SELLOFFBUY, 0, 0, 0, 0);
                        SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(MShare.g_sSellOffSellerLoginId));
                    }
                    break;
                case "softclose":
                    SendClientMessage(Messages.CM_SOFTCLOSE, 0, 0, 0, 0);
                    break;
                default:
                    MainOutMessage("M3SELLOFFSEQ skip=op_not_implemented step=" + QuestEsc(_soSeqOp));
                    break;
            }
        }

        /// <summary>收帧钩子（单一收包点）：元宝寄售相关 SM_* 在 `g_boSellOffSequence` 打开且本步待收尾时
        /// 落 `phase=frame` 事件。**只读不写世界态**（同 T-29/T-44 纪律）；默认关时空转。</summary>
        public void SellOffOnServerMessage(int sm, int merchant, int face, int tag, int series, string body)
        {
            if (!MShare.g_boSellOffSequence || _soSeqPendingK < 0)
            {
                return;
            }
            switch (sm)
            {
                case 767:     // SM_MENU_OK（[成功]/[错误] 文案）
                case 23000:   // SM_SENDDEALOFFFORM
                case 23003: case 23005:   // SM_SELLOFFADDITEM_OK / SM_SellOffADDITEM_FAIL
                case 23008: case 23010:   // SM_SELLOFFDELITEM_OK / _FAIL
                case 23014:   // SM_SellOffCANCEL
                case 23016: case 23018:   // SM_SELLOFFEND_OK / _FAIL
                case 23021: case 23023:   // SM_QUERYYBSELL / SM_QUERYYBDEAL
                case 23027:   // SM_SELLOFFBUY_OK
                    break;
                default:
                    return;
            }
            string text = sm == 767 && !string.IsNullOrEmpty(body) ? EDCode.DeCodeString(body) : string.Empty;
            string fr = "{\"sm\":" + sm + ",\"a\":" + merchant + ",\"b\":" + face
                + ",\"c\":" + tag + ",\"d\":" + series
                + ",\"text\":\"" + QuestEsc(text) + "\"}";
            _soSeqFrames.Add(fr);
            MainOutMessage("M3SELLOFFSEQ {\"phase\":\"frame\",\"bot\":\"" + QuestEsc(LoginId)
                + "\",\"k\":" + _soSeqPendingK + ",\"frame\":" + fr + "}");
        }

        #endregion

        #region M3⑧ 公会/攻城（guild / castle）动作序列夹具（默认关；白名单 T-46 家族扩展）

        // 口径（卡 openmir2-m3-guild-castle-fixture-botsrv）：收到本假人 SM_LOGON 后，按 `g_sGuildCastleSteps`
        // 的固定顺序发 行会面板/成员/封号/公告/联盟（现成 CM_GUILD* 原语）+ 行会 NPC 标签选择（CM_MERCHANTDLGSELECT），
        // 逐步落 `M3GUILDCSEQ {json}` 的 arm/send/frame/end/done 事件（供 `guild_castle_diff.py assemble`）。
        // 只读本假人自己的视图（`MShare.MySelf` / `PlayScene.ActorList` / `MShare.ItemArr` / `MShare.g_sGuildName`）、
        // 只写本假人自己的 socket（同 T-22/T-29/T-41/T-45 纪律）；默认关时本 region 全部空转、不新增任何发送与日志。

        private string[] _gcSeqSteps;
        private int _gcSeqStep = -1;
        private int _gcSeqNextTick;
        private int _gcSeqPendingK = -1;
        private readonly List<string> _gcSeqFrames = new List<string>();
        private string _gcSeqOp = string.Empty;
        private string _gcSeqRole = string.Empty;
        private string _gcSeqParam = string.Empty;
        private bool _gcSeqRan;
        private int _gcSeqNpcActor;

        /// <summary>收到本假人 SM_LOGON 后武装固定行会/攻城序列（`g_boGuildCastleSequence` 关闭时是空操作）。</summary>
        private void ArmGuildCastleSequence()
        {
            if (!MShare.g_boGuildCastleSequence)
            {
                return;
            }
            _gcSeqSteps = MShare.ParseGuildCastleSteps(MShare.g_sGuildCastleSteps);
            _gcSeqStep = 0;
            _gcSeqPendingK = -1;
            _gcSeqFrames.Clear();
            _gcSeqNextTick = MShare.GetTickCount() + MShare.g_nGuildCastleStartDelayMs;
            MainOutMessage("M3GUILDCSEQ {\"phase\":\"arm\",\"bot\":\"" + ShopEsc(LoginId)
                + "\",\"role\":\"" + (MShare.IsGuildCastleMaster(LoginId, MShare.g_sGuildCastleMasterLoginId) ? "a" : "b")
                + "\",\"steps\":" + ShopJsonArray(_gcSeqSteps)
                + ",\"step_ms\":" + MShare.g_nGuildCastleStepMs + "}");
        }

        /// <summary>M3⑧ 固定行会/攻城序列（步进式）：每 `g_nGuildCastleStepMs` 处理一步；上一步先收尾
        /// （`phase=end`，聚合本步收到的帧 + 行会名/封号/金币/背包前后态 + branch），再发下一步；
        /// 全部发完发 `phase=done`。**两个**假人（会长/peer）都跑同一串步骤；角色不符的步只记 `ran=false`
        /// + `skip_other_role`（k 仍前进）。默认关时空转。</summary>
        private void GuildCastleSequenceTick()
        {
            if (!MShare.g_boGuildCastleSequence || _gcSeqSteps == null || _gcSeqStep < 0)
            {
                return;
            }
            if (!_inWorld || ClientSocket == null || !ClientSocket.IsConnected)
            {
                return;
            }
            int now = MShare.GetTickCount();
            if (now < _gcSeqNextTick)
            {
                return;
            }
            if (_gcSeqPendingK >= 0)
            {
                EmitGuildCastleEnd(_gcSeqPendingK);
                _gcSeqPendingK = -1;
            }
            if (_gcSeqStep >= _gcSeqSteps.Length)
            {
                _gcSeqStep = -1;
                MainOutMessage("M3GUILDCSEQ {\"phase\":\"done\",\"bot\":\"" + ShopEsc(LoginId)
                    + "\",\"steps\":" + _gcSeqSteps.Length + "}");
                return;
            }
            int k = _gcSeqStep;
            string step = _gcSeqSteps[k];
            _gcSeqStep++;
            _gcSeqFrames.Clear();
            _gcSeqOp = MShare.GuildCastleStepOp(step);
            _gcSeqRole = MShare.GuildCastleStepRole(step);
            _gcSeqParam = MShare.GuildCastleStepParam(step);
            bool isMaster = MShare.IsGuildCastleMaster(LoginId, MShare.g_sGuildCastleMasterLoginId);
            _gcSeqRan = MShare.GuildCastleRunsForRole(_gcSeqRole, isMaster);
            Actor npc = FindShopNpc();
            _gcSeqNpcActor = npc != null ? npc.RecogId : 0;
            EmitGuildCastleSend(k);
            SendGuildCastleStep();
            _gcSeqPendingK = k;
            int waitMs = MShare.g_nGuildCastleStepMs;
            if (_gcSeqOp == "wait" && int.TryParse(_gcSeqParam, out int w) && w > waitMs)
            {
                waitMs = w;
            }
            _gcSeqNextTick = now + waitMs;
        }

        private void EmitGuildCastleSend(int k)
        {
            MainOutMessage("M3GUILDCSEQ {\"phase\":\"send\",\"bot\":\"" + ShopEsc(LoginId)
                + "\",\"k\":" + k
                + ",\"role\":\"" + ShopEsc(_gcSeqRole)
                + "\",\"op\":\"" + ShopEsc(_gcSeqOp)
                + "\",\"ran\":" + (_gcSeqRan ? "true" : "false")
                + ",\"param\":\"" + ShopEsc(_gcSeqParam) + "\""
                + ",\"map\":\"" + ShopEsc(MShare.MapTitle) + "\""
                + ",\"guild_name\":\"" + ShopEsc(GuildCastleName()) + "\""
                + ",\"guild_rank_name\":\"" + ShopEsc(GuildCastleRankName()) + "\""
                + ",\"gold_before\":" + ShopGold()
                + ",\"bag_before\":" + ShopBagJson() + "}");
        }

        private void EmitGuildCastleEnd(int k)
        {
            string branch = _gcSeqRan ? MShare.ClassifyGuildCastleFrames(_gcSeqFrames) : "skip_other_role";
            // observe/wait 本来就不发 CM_*、也不期待帧 ⇒ 收尾固定 ok（否则空帧列表会被误判 fail_no_frame）。
            if (_gcSeqRan && (_gcSeqOp == "observe" || _gcSeqOp == "wait"))
            {
                branch = "ok";
            }
            MainOutMessage("M3GUILDCSEQ {\"phase\":\"end\",\"bot\":\"" + ShopEsc(LoginId)
                + "\",\"k\":" + k
                + ",\"guild_name\":\"" + ShopEsc(GuildCastleName()) + "\""
                + ",\"guild_rank_name\":\"" + ShopEsc(GuildCastleRankName()) + "\""
                + ",\"gold_after\":" + ShopGold()
                + ",\"bag_after\":" + ShopBagJson()
                + ",\"branch\":\"" + branch + "\"}");
        }

        /// <summary>发本步的 CM_*（角色不符 ⇒ 不发；NPC 不在视图 ⇒ 只记 skip）。</summary>
        private void SendGuildCastleStep()
        {
            if (!_gcSeqRan)
            {
                return;
            }
            switch (_gcSeqOp)
            {
                case "open_dlg":
                    SendGuildDlg();
                    break;
                case "home":
                    SendGuildHome();
                    break;
                case "memberlist":
                    SendGuildMemberList();
                    break;
                case "add_member":
                    SendGuildAddMem(ResolveGuildCastleWho(_gcSeqParam));
                    break;
                case "del_member":
                    SendGuildDelMem(ResolveGuildCastleWho(_gcSeqParam));
                    break;
                case "update_notice":
                    SendGuildUpdateNotice(_gcSeqParam);
                    break;
                case "update_rank":
                    SendGuildUpdateGrade(_gcSeqParam);
                    break;
                case "ally":
                    SendGuildAlly(_gcSeqParam);
                    break;
                case "break_ally":
                    SendGuildBreakAlly(_gcSeqParam);
                    break;
                case "build_guild":
                case "guild_war":
                case "castle_apply":
                case "open_door":
                case "close_door":
                case "repair_door":
                case "repair_wall":
                    if (_gcSeqNpcActor == 0 || string.IsNullOrEmpty(_gcSeqParam))
                    {
                        MainOutMessage("M3GUILDCSEQ skip=no_npc_in_view op=" + ShopEsc(_gcSeqOp));
                        break;
                    }
                    // 行会 NPC 脚本标签：`@@buildguildnow` / `@@guildwar` / `@requestcastlewarnowN` /
                    // `@openmaindoor` / `@closemaindoor` / `@repairdoor` / `@repairwall`（GuildOfficial/CastleOfficial）。
                    SendMerchantDlgSelect(_gcSeqNpcActor, _gcSeqParam);
                    break;
                case "observe":
                case "wait":
                    break;
                case "softclose":
                    SendClientMessage(Messages.CM_SOFTCLOSE, 0, 0, 0, 0);
                    break;
                default:
                    MainOutMessage("M3GUILDCSEQ skip=op_not_implemented step=" + ShopEsc(_gcSeqOp));
                    break;
            }
        }

        /// <summary>param 里的夹具占位符：`@peer` / `@master` 用两侧一致的 LoginId 展开（仅夹具配置，不读跨假人世界态）。</summary>
        private string ResolveGuildCastleWho(string param)
        {
            if (param == "@peer") { return MShare.g_sGuildCastlePeerLoginId; }
            if (param == "@master") { return MShare.g_sGuildCastleMasterLoginId; }
            return param;
        }

        /// <summary>M3⑧ 夹具最小驱动新增：CM_GUILDALLY(1044) + EncodeString(行会名)（PlayObject.Operate.cs:1465 读 body 为名）。</summary>
        public void SendGuildAlly(string guildName)
        {
            if (!string.IsNullOrEmpty(guildName))
            {
                CommandMessage msg = Messages.MakeMessage(Messages.CM_GUILDALLY, 0, 0, 0, 0);
                SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(guildName));
            }
        }

        /// <summary>M3⑧ 夹具最小驱动新增：CM_GUILDBREAKALLY(1045) + EncodeString(行会名)（PlayObject.Operate.cs:1525 读 body 为名）。</summary>
        public void SendGuildBreakAlly(string guildName)
        {
            if (!string.IsNullOrEmpty(guildName))
            {
                CommandMessage msg = Messages.MakeMessage(Messages.CM_GUILDBREAKALLY, 0, 0, 0, 0);
                SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(guildName));
            }
        }

        /// <summary>本假人行会名（`SM_CHANGEGUILDNAME(750)` 回包写入 `MShare.g_sGuildName`）。</summary>
        private string GuildCastleName()
        {
            return MShare.g_sGuildName ?? string.Empty;
        }

        /// <summary>本假人行会封号名（`SM_CHANGEGUILDNAME(750)` 回包写入 `MShare.g_sGuildRankName`）。</summary>
        private string GuildCastleRankName()
        {
            return MShare.g_sGuildRankName ?? string.Empty;
        }

        /// <summary>收帧钩子（单一收包点）：行会面板/成员/封号/建会/联盟/换名/菜单文案帧在
        /// `g_boGuildCastleSequence` 打开且本步待收尾时落 `phase=frame` 事件。**只读不写世界态**
        /// （同 T-29/T-41/T-45 纪律）；默认关时空转。</summary>
        public void GuildCastleOnServerMessage(int sm, int a, int b, int c, int d, string body)
        {
            if (!MShare.g_boGuildCastleSequence || _gcSeqPendingK < 0)
            {
                return;
            }
            switch (sm)
            {
                case 750:   // SM_CHANGEGUILDNAME
                case 753:   // SM_OPENGUILDDLG
                case 754:   // SM_OPENGUILDDLG_FAIL
                case 756:   // SM_SENDGUILDMEMBERLIST
                case 757:   // SM_GUILDADDMEMBER_OK
                case 758:   // SM_GUILDADDMEMBER_FAIL
                case 759:   // SM_GUILDDELMEMBER_OK
                case 760:   // SM_GUILDDELMEMBER_FAIL
                case 761:   // SM_GUILDRANKUPDATE_FAIL
                case 762:   // SM_BUILDGUILD_OK
                case 763:   // SM_BUILDGUILD_FAIL
                case 767:   // SM_MENU_OK
                case 768:   // SM_GUILDMAKEALLY_OK
                case 769:   // SM_GUILDMAKEALLY_FAIL
                case 770:   // SM_GUILDBREAKALLY_OK
                case 771:   // SM_GUILDBREAKALLY_FAIL
                case 10303: // RM_BUILDGUILD_OK
                case 10304: // RM_BUILDGUILD_FAIL
                    break;
                default:
                    return;
            }
            bool isText = sm == 750 || sm == 753 || sm == 756 || sm == 767;
            string text = isText && !string.IsNullOrEmpty(body) ? EDCode.DeCodeString(body) : string.Empty;
            string fr = "{\"sm\":" + sm + ",\"a\":" + a + ",\"b\":" + b
                + ",\"c\":" + c + ",\"d\":" + d
                + ",\"text\":\"" + ShopEsc(text) + "\"}";
            _gcSeqFrames.Add(fr);
            MainOutMessage("M3GUILDCSEQ {\"phase\":\"frame\",\"bot\":\"" + ShopEsc(LoginId)
                + "\",\"k\":" + _gcSeqPendingK + ",\"frame\":" + fr + "}");
        }

        #endregion


        #region M1③ 退出路径夹具：进世界后发 CM_SOFTCLOSE（小退；默认关；白名单 M1-EXIT）

        /// <summary>发送时刻（MShare.GetTickCount），-1=未武装。</summary>
        private int _softCloseAtTick = -1;
        /// <summary>是否已发出（只发一次）。</summary>
        private bool _softCloseSent;

        /// <summary>收到本假人 SM_LOGON 后武装（`MShare.g_nSoftCloseAfterWorldMs` &lt;= 0 时是空操作）。</summary>
        private void ArmSoftClose()
        {
            if (MShare.g_nSoftCloseAfterWorldMs <= 0)
            {
                return;
            }
            _softCloseAtTick = MShare.GetTickCount() + MShare.g_nSoftCloseAfterWorldMs;
            MainOutMessage($"M1SOFTCLOSE arm after_ms={MShare.g_nSoftCloseAfterWorldMs}");
        }

        /// <summary>到点后发一条 CM_SOFTCLOSE(1009)——线上形状照 C# 客户端 `AppLogout()` 的
        /// `SendClientMessage(CM_SOFTCLOSE, 0, 0, 0, 0)`；只写本假人自己的 socket，
        /// 不读不写跨假人共享世界态（同 T-22 纪律）。默认关时空转。</summary>
        private void SoftCloseTick()
        {
            if (MShare.g_nSoftCloseAfterWorldMs <= 0 || _softCloseSent || _softCloseAtTick < 0)
            {
                return;
            }
            if (!_inWorld || ClientSocket == null || !ClientSocket.IsConnected)
            {
                return;
            }
            if (MShare.GetTickCount() < _softCloseAtTick)
            {
                return;
            }
            _softCloseSent = true;
            SendClientMessage(Messages.CM_SOFTCLOSE, 0, 0, 0, 0);
            MainOutMessage("M1SOFTCLOSE sent ident=1009");
        }

        #endregion

        /// <summary>收到 SM_LOGON = 服务端确认本假人已进世界（探针武装 + tick 覆盖率分子），幂等。
        /// `x`/`y` = 该消息里服务端给本假人的落点（PlayScene 的 SM_LOGON 分支就用它们 NewActor）。</summary>
        public void MarkInWorld(int actorId, int x, int y)
        {
            if (_inWorldCounted)
            {
                return;
            }
            _inWorldCounted = true;
            _myActorId = actorId;
            _probeX = x;
            _probeY = y;
            _inWorld = true;
            ArmBagEquipSequence();
            ArmAttackSequence();
            ArmShopSequence();
            ArmQuestSequence();
            ArmPairTradeSequence();
            ArmSellOffSequence();
            ArmGuildCastleSequence();
            ArmSoftClose();
            if (LoadMetrics.ShouldSuppressInWorld(LoginId))
            {
                // 牙齿钩子（T-12）：本假人照常进世界，但**不计** in_world（抹掉“进世界证据”）。
                LoadMetrics.InWorldSuppressed();
                return;
            }
            LoadMetrics.InWorld();
        }

        /// <summary>本会话是否已收到服务端对本连接的 `SM_LOGON`（= 真的进了世界）。
        /// F4（T-12）后，挂机/在线判据只看它，不看跨假人共享的 `MShare` 世界态。</summary>
        public bool InWorld => _inWorld;

        public void Run()
        {
            if (DScreen.CurrentScene == null)
            {
                DScreen.ChangeScene(SceneType.Login);
            }
            else
            {
                if (DScreen.CurrentScene == LoginScene)
                {
                    LoginScene.Login();
                }
                DScreen.CurrentScene.DoNotifyEvent();
                ProbeActionTick();
                BagEquipSequenceTick();
                AttackSequenceTick();
                ShopSequenceTick();
                QuestSequenceTick();
                PairTradeSequenceTick();
                SellOffSequenceTick();
                GuildCastleSequenceTick();
                SoftCloseTick();
                if (DScreen.CurrentScene == PlayScene)
                {
                    ProcessActionMessages();
                    if (MShare.MySelf != null)
                    {
                        PlayScene.BeginScene();
                    }
                }
            }
        }

        private void AppLogout()
        {
            if (MShare.g_boQueryExit)
            {
                return;
            }
            MShare.g_boQueryExit = true;
            try
            {
                SendClientMessage(Messages.CM_SOFTCLOSE, 0, 0, 0, 0);
                CloseAllWindows();
                PlayScene.ClearActors();
                MShare.g_SoftClosed = true;
                //ActiveCmdTimer(MShare.TTimerCommand.tcSoftClose);
                MShare.SaveItemFilter();
            }
            finally
            {
                MShare.g_boQueryExit = false;
            }
        }

        private void LoginOut()
        {
            MainOutMessage("退出游戏.");
            SendClientMessage(Messages.CM_SOFTCLOSE, 0, 0, 0, 0);
        }

        public void AppExit()
        {
            if (MShare.g_boQueryExit)
            {
                return;
            }
            MShare.g_boQueryExit = true;
            try
            {
                SendClientMessage(Messages.CM_SOFTCLOSE, 0, 0, 0, 0);
                MShare.SaveItemFilter();
            }
            finally
            {
                MShare.g_boQueryExit = false;
            }
        }

        private void ProcessMagic()
        {
            short nSx;
            short nSy;
            int tdir;
            int targid;
            short targx;
            short targy;
            //TUseMagicInfo pmag;
            if ((PlayScene.ProcMagic.NTargetX < 0) || (MShare.MySelf == null))
            {
                return;
            }
            if (MShare.GetTickCount() - PlayScene.ProcMagic.DwTick > 5000)
            {
                PlayScene.ProcMagic.DwTick = MShare.GetTickCount();
                PlayScene.ProcMagic.NTargetX = -1;
                return;
            }
            if (MShare.GetTickCount() - _mDwProcUseMagicTick > 28)
            {
                _mDwProcUseMagicTick = MShare.GetTickCount();
                if (PlayScene.ProcMagic.FUnLockMagic)
                {
                    targx = PlayScene.ProcMagic.NTargetX;
                    targy = PlayScene.ProcMagic.NTargetY;
                }
                else if ((PlayScene.ProcMagic.XTarget != null) && !PlayScene.ProcMagic.XTarget.Death)
                {
                    targid = PlayScene.ProcMagic.XTarget.RecogId;
                    targx = PlayScene.ProcMagic.XTarget.CurrX;
                    targy = PlayScene.ProcMagic.XTarget.CurrY;
                }
                else
                {
                    PlayScene.ProcMagic.NTargetX = -1;
                    return;
                }
                nSx = (short)Math.Abs(MShare.MySelf.CurrX - targx);
                nSy = (short)Math.Abs(MShare.MySelf.CurrY - targy);
                if ((nSx <= BotConst.MagicRange) && (nSy <= BotConst.MagicRange))
                {
                    if (PlayScene.ProcMagic.FContinue || (CanNextAction() && ServerAcceptNextAction()))
                    {
                        MShare.LatestSpellTick = MShare.GetTickCount();
                        tdir = BotHelper.GetFlyDirection(MShare.MySelf.CurrX, MShare.MySelf.CurrY, targx, targy);
                        //pmag = new TUseMagicInfo();
                        //pmag.EffectNumber = g_PlayScene.ProcMagic.XMagic.Def.btEffect;
                        //pmag.MagicSerial = g_PlayScene.ProcMagic.XMagic.Def.wMagicID;
                        //pmag.ServerMagicCode = 0;
                        MShare.g_dwMagicDelayTime = 200 + PlayScene.ProcMagic.XMagic.Def.DelayTime;
                        MShare.g_dwMagicPKDelayTime = 0;
                        if (MShare.MagicTarget != null)
                        {
                            if (MShare.MagicTarget.Race == 0)
                            {
                                MShare.g_dwMagicPKDelayTime = 300 + RandomNumber.GetInstance().Random(1100);
                            }
                        }
                        //MShare.g_MySelf.SendMsg(Messages.CM_SPELL, targx, targy, tdir, pmag, targid, "", 0);
                        PlayScene.ProcMagic.NTargetX = -1;
                    }
                }
                else
                {
                    MShare.PlayerAction = PlayerAction.Run;
                    MShare.TargetX = targx;
                    MShare.TargetY = targy;
                }
            }
        }

        private void ProcessKeyMessages()
        {
            if (_actionKey == 0)
            {
                return;
            }
            if ((MShare.MySelf != null) && MShare.MySelf.StallMgr.OnSale)
            {
                return;
            }
        }

        private void ProcessActionMessages()
        {
            if (MShare.MySelf == null)
            {
                return;
            }
            if ((MShare.TargetX >= 0) && CanNextAction() && ServerAcceptNextAction())
            {
                if (MShare.OpenAutoPlay && (MShare.MapPath != null) && (MShare.AutoStep >= 0) && (0 < MShare.MapPath.Length))
                {
                    if ((Math.Abs(MShare.MapPath[MShare.AutoStep].X - MShare.MySelf.CurrX) <= 3) && (Math.Abs(MShare.MapPath[MShare.AutoStep].X - MShare.MySelf.CurrY) <= 3))
                    {
                        if (MShare.MapPath.Length >= 2)// 3点以上
                        {
                            if (MShare.AutoStep >= MShare.MapPath.Length) // 当前点在终点...
                            {
                                // 终点 <-> 起点 距离过远...
                                if ((Math.Abs(MShare.MapPath[MShare.MapPath.Length].X - MShare.MapPath[0].X) >= 36) || (Math.Abs(MShare.MapPath[MShare.MapPath.Length].X - MShare.MapPath[0].X) >= 36))
                                {
                                    MShare.g_APGoBack = true; // 原路返回
                                    MShare.AutoLastPoint = MShare.MapPath[MShare.AutoStep];
                                    MShare.AutoStep -= 1;
                                }
                                else
                                {
                                    MShare.g_APGoBack = false; // 循环到起点...
                                    MShare.AutoLastPoint = MShare.MapPath[MShare.AutoStep];
                                    MShare.AutoStep = 0;
                                }
                            }
                            else
                            {
                                if (MShare.g_APGoBack) // 原路返回
                                {
                                    MShare.AutoLastPoint = MShare.MapPath[MShare.AutoStep];
                                    MShare.AutoStep -= 1;
                                    if (MShare.AutoStep <= 0)// 已回到起点
                                    {
                                        MShare.AutoStep = 0;
                                        MShare.g_APGoBack = false;
                                    }
                                }
                                else
                                {
                                    MShare.AutoLastPoint = MShare.MapPath[MShare.AutoStep]; // 循环...
                                    MShare.AutoStep++;
                                }
                            }
                        }
                        else
                        {
                            // 2点,循环...
                            MShare.AutoLastPoint = MShare.MapPath[MShare.AutoStep];
                            MShare.AutoStep++;
                            if (MShare.AutoStep > MShare.MapPath.Length)
                            {
                                MShare.AutoStep = 0;
                            }
                        }
                    }
                }
                if ((MShare.TargetX != MShare.MySelf.CurrX) || (MShare.TargetY != MShare.MySelf.CurrY))
                {
                    if ((MShare.MySelf.m_nTagX > 0) && (MShare.MySelf.m_nTagY > 0))
                    {
                        if (GMoveBusy)
                        {
                            if (MShare.GetTickCount() - GMoveErrTick > 60)
                            {
                                GMoveErrTick = MShare.GetTickCount();
                                GMoveErr++;
                            }
                        }
                        else
                        {
                            GMoveErr = 0;
                        }
                        if (GMoveErr > 10)
                        {
                            GMoveErr = 0;
                            GMoveBusy = false;
                            TimerAutoMove.Enabled = false;
                            if ((MShare.MySelf.m_nTagX > 0) && (MShare.MySelf.m_nTagY > 0))
                            {
                                if (!GPathBusy)
                                {
                                    GPathBusy = true;
                                    try
                                    {
                                        Map.ReLoadMapData();
                                        PathMap.g_MapPath = Map.FindPath(MShare.MySelf.CurrX, MShare.MySelf.CurrY, MShare.MySelf.m_nTagX, MShare.MySelf.m_nTagY, 0);
                                        if (PathMap.g_MapPath != null)
                                        {
                                            GMoveStep = 1;
                                            TimerAutoMove.Enabled = true;
                                        }
                                        else
                                        {
                                            MShare.MySelf.m_nTagX = 0;
                                            MShare.MySelf.m_nTagY = 0;
                                            ScreenManager.AddChatBoardString("自动移动出错，停止移动");
                                        }
                                    }
                                    finally
                                    {
                                        GPathBusy = false;
                                    }
                                }
                            }
                        }
                    }
                TTTT:
                    short mx = MShare.MySelf.CurrX;
                    short my = MShare.MySelf.CurrY;
                    short dx = MShare.TargetX;
                    short dy = MShare.TargetY;
                    byte ndir = BotHelper.GetNextDirection(mx, my, dx, dy);
                    int crun;
                    byte mdir;
                    switch (MShare.PlayerAction)
                    {
                        case PlayerAction.Walk:
                        LB_WALK:
                            crun = MShare.MySelf.CanWalk();
                            if (IsUnLockAction() && (crun > 0))
                            {
                                BotHelper.GetNextPosXy(ndir, ref mx, ref my);
                                bool bostop = false;
                                if (!PlayScene.CanWalk(mx, my))
                                {
                                    if (MShare.OpenAutoPlay && MShare.AutoMove && (MShare.AutoPathList.Count > 0))
                                    {
                                        _heroActor.InitQueue2();
                                        MShare.TargetX = -1;
                                    }
                                    bool bowalk = false;
                                    byte adir = 0;
                                    if (!bowalk)
                                    {
                                        mx = MShare.MySelf.CurrX;
                                        my = MShare.MySelf.CurrY;
                                        BotHelper.GetNextPosXy(ndir, ref mx, ref my);
                                        if (CheckDoorAction(mx, my))
                                        {
                                            bostop = true;
                                        }
                                    }
                                    if (!bostop && (PlayScene.CrashMan(mx, my) || !Map.CanMove(mx, my)))
                                    {
                                        mx = MShare.MySelf.CurrX;
                                        my = MShare.MySelf.CurrY;
                                        adir = BotHelper.PrivDir(ndir);
                                        BotHelper.GetNextPosXy(adir, ref mx, ref my);
                                        if (!Map.CanMove(mx, my))
                                        {
                                            mx = MShare.MySelf.CurrX;
                                            my = MShare.MySelf.CurrY;
                                            adir = BotHelper.NextDir(ndir);
                                            BotHelper.GetNextPosXy(adir, ref mx, ref my);
                                            if (Map.CanMove(mx, my))
                                            {
                                                bowalk = true;
                                            }
                                        }
                                        else
                                        {
                                            bowalk = true;
                                        }
                                    }
                                    if (bowalk)
                                    {
                                        MShare.MySelf.UpdateMsg(Messages.CM_WALK, (ushort)mx, (ushort)my, adir, 0, 0, "", 0);
                                        MShare.LastMoveTick = MShare.GetTickCount();
                                        if (MShare.g_nOverAPZone > 0)
                                        {
                                            MShare.g_nOverAPZone -= 1;
                                        }
                                    }
                                    else
                                    {
                                        mdir = BotHelper.GetNextDirection(MShare.MySelf.CurrX, MShare.MySelf.CurrY, dx, dy);
                                        if (mdir != MShare.MySelf.m_btDir)
                                        {
                                            MShare.MySelf.SendMsg(Messages.CM_TURN, (ushort)MShare.MySelf.CurrX, (ushort)MShare.MySelf.CurrY, mdir, 0, 0, "", 0);
                                        }
                                        MShare.TargetX = -1;
                                    }
                                }
                                else
                                {
                                    MShare.MySelf.UpdateMsg(Messages.CM_WALK, (ushort)mx, (ushort)my, ndir, 0, 0, "", 0);
                                    MShare.LastMoveTick = MShare.GetTickCount();
                                }
                            }
                            else
                            {
                                MShare.TargetX = -1;
                            }
                            break;
                        case PlayerAction.Run: // 免助跑
                            if (MShare.g_boCanStartRun || (MShare.g_nRunReadyCount >= 1))
                            {
                                crun = MShare.MySelf.CanRun();// 骑马开始
                                if ((MShare.MySelf.m_btHorse != 0) && (BotHelper.GetDistance(mx, my, dx, dy) >= 3) && (crun > 0) && IsUnLockAction())
                                {
                                    BotHelper.GetNextHorseRunXy(ndir, ref mx, ref my);
                                    if (PlayScene.CanRun(MShare.MySelf.CurrX, MShare.MySelf.CurrY, mx, my))
                                    {
                                        MShare.MySelf.UpdateMsg(Messages.CM_HORSERUN, (ushort)mx, (ushort)my, ndir, 0, 0, "", 0);
                                        MShare.LastMoveTick = MShare.GetTickCount();
                                        if (MShare.g_nOverAPZone > 0)
                                        {
                                            MShare.g_nOverAPZone -= 1;
                                        }
                                    }
                                    else
                                    {
                                        // 如果跑失败则跳回去走
                                        MShare.PlayerAction = PlayerAction.Walk;
                                        goto TTTT;
                                    }
                                }
                                else
                                {
                                    if ((BotHelper.GetDistance(mx, my, dx, dy) >= 2) && (crun > 0))
                                    {
                                        if (IsUnLockAction())
                                        {
                                            BotHelper.GetNextRunXy(ndir, ref mx, ref my);
                                            if (PlayScene.CanRun(MShare.MySelf.CurrX, MShare.MySelf.CurrY, mx, my))
                                            {
                                                MShare.MySelf.UpdateMsg(Messages.CM_RUN, (ushort)mx, (ushort)my, ndir, 0, 0, "", 0);
                                                MShare.LastMoveTick = MShare.GetTickCount();
                                                if (MShare.g_nOverAPZone > 0)
                                                {
                                                    MShare.g_nOverAPZone -= 1;
                                                }
                                            }
                                            else
                                            {
                                                // 如果跑失败则跳回去走
                                                MShare.PlayerAction = PlayerAction.Walk;
                                                goto TTTT;
                                            }
                                        }
                                        else
                                        {
                                            MShare.TargetX = -1;
                                        }
                                    }
                                    else
                                    {
                                        mdir = BotHelper.GetNextDirection(MShare.MySelf.CurrX, MShare.MySelf.CurrY, dx, dy);
                                        if (mdir != MShare.MySelf.m_btDir)
                                        {
                                            MShare.MySelf.SendMsg(Messages.CM_TURN, (ushort)MShare.MySelf.CurrX, (ushort)MShare.MySelf.CurrY, mdir, 0, 0, "", 0);
                                        }
                                        MShare.TargetX = -1;
                                        goto LB_WALK;
                                    }
                                }
                            }
                            else
                            {
                                MShare.g_nRunReadyCount++;
                                goto LB_WALK;
                            }
                            break;
                    }
                }
                else if (MShare.OpenAutoPlay && MShare.AutoMove && (MShare.AutoPicupItem != null))
                {
                    SendPickup();
                    MainOutMessage("拾取物品");
                    if (MShare.AutoMove && (MShare.AutoPathList.Count > 0))
                    {
                        _heroActor.InitQueue2();
                        MShare.TargetX = -1;
                    }
                }
            }
            MShare.TargetX = -1;
            if (MShare.MySelf.RealActionMsg.Ident > 0)
            {
                if (MShare.MySelf.RealActionMsg.Ident == Messages.CM_SPELL)
                {
                    SendSpellMsg(MShare.MySelf.RealActionMsg.Ident, (ushort)MShare.MySelf.RealActionMsg.X, (ushort)MShare.MySelf.RealActionMsg.Y, MShare.MySelf.RealActionMsg.Dir, MShare.MySelf.RealActionMsg.State);
                }
                else
                {
                    SendActMsg(MShare.MySelf.RealActionMsg.Ident, (ushort)MShare.MySelf.RealActionMsg.X, (ushort)MShare.MySelf.RealActionMsg.Y, MShare.MySelf.RealActionMsg.Dir);
                }
                MShare.MySelf.RealActionMsg.Ident = 0;
                if (MShare.g_nStallX != -1)
                {
                    if ((Math.Abs(MShare.g_nStallX - MShare.MySelf.CurrX) >= 8) || (Math.Abs(MShare.g_nStallY - MShare.MySelf.CurrY) >= 8))
                    {
                        MShare.g_nStallX = -1;
                    }
                }
            }
        }

        /// <summary>压测用：**确保**自动挂机开着（OpenAutoPlay 是 toggle，压测里不能盲调）。
        /// 背景：进图时 PlayScene 的 SM_NEWMAP 分支会把 TimerAutoPlay 停掉（"地图跳转，停止自动挂机"），
        /// 之后假人就一直站着不动 ⇒ 没有动作 ⇒ 收不到 `+GD` ack ⇒ tick 样本为 0（2026-10-10 实测）。
        /// 负载门禁需要假人持续产生动作，所以每轮挂机循环前把它重新打开。</summary>
        public void EnsureAutoPlay()
        {
            if (MShare.MySelf == null)
            {
                return;
            }
            // F4（T-12）：只对**本会话已收到 SM_LOGON**（`_inWorld`）的假人重臂挂机。
            // 旧行为只看跨假人共享的 `MShare.MySelf`：只要任意一个假人进了世界，其余未进世界的假人
            // 也会被臂上挂机并写在共享态上 ⇒ 按 bot 名记的挂机日志不能当作“该假人进世界”的证据。
            // `MIR2_BOT_LEGACY_SHARED_AUTOPLAY=1` 时保留旧行为，仅用于产出“改前”对照数字。
            if (!_inWorld && !LoadMetrics.LegacySharedAutoPlay)
            {
                return;
            }
            MShare.g_gcAss[0] = true;
            if (TimerAutoPlay == null)
            {
                TimerAutoPlay = new TimerAutoPlay();
            }
            TimerAutoPlay.Enabled = true;
        }

        public void OpenAutoPlay()
        {
            if (MShare.MySelf == null)
            {
                return;
            }
            MShare.g_gcAss[0] = !MShare.g_gcAss[0];
            if (TimerAutoPlay == null)
            {
                TimerAutoPlay = new TimerAutoPlay();
            }
            TimerAutoPlay.Enabled = MShare.g_gcAss[0];
            if (TimerAutoPlay.Enabled)
            {
                MShare.AutoTagget = null;
                MShare.AutoPicupItem = null;
                MShare.g_nAPStatus = -1;
                MShare.TargetX = -1;
                MShare.g_APGoBack = false;
                ScreenManager.AddChatBoardString("开始自动挂机...");
                SaveWayPoint();
                if (MShare.MapPath != null)
                {
                    MShare.AutoStep = 0;
                    MShare.AutoLastPoint.X = -1;
                    GetNearPoint();
                }
            }
            else
            {
                ScreenManager.AddChatBoardString("停止自动挂机...");
            }
            return;
        }

        public ClientMagic GetMagicByKey(char key)
        {
            ClientMagic result = null;
            for (int i = 0; i < MShare.g_MagicList.Count; i++)
            {
                ClientMagic pm = MShare.g_MagicList[i];
                if (pm.Key == key)
                {
                    result = pm;
                    break;
                }
            }
            return result;
        }

        public void UseMagic(int tx, int ty, ClientMagic pcm, bool boReacll = false, bool boContinue = false)
        {
            bool boSeriesSkill;
            int defSpellSpend;
            int tdir;
            int targx = 0;
            int targy = 0;
            //TUseMagicInfo pmag;
            bool fUnLockMagic;
            if ((MShare.MySelf != null))
            {
                return;
            }
            if (pcm == null)
            {
                return;
            }
            if (pcm.Def.MagicId == 0)
            {
                return;
            }
            short spellSpend = (short)(HUtil32.Round(pcm.Def.Spell / (pcm.Def.TrainLv + 1) * (pcm.Level + 1)) + pcm.Def.DefSpell);
            if (pcm.Def.MagicId == 114)
            {
                if (MShare.g_boSkill_114_MP)
                {
                    boSeriesSkill = false;
                }
                else
                {
                    boSeriesSkill = true;
                }
            }
            else if (new ArrayList(new int[] { 68, 78 }).Contains(pcm.Def.MagicId))
            {
                if (MShare.g_boSkill_68_MP)
                {
                    boSeriesSkill = false;
                }
                else
                {
                    boSeriesSkill = true;
                }
            }
            else
            {
                boSeriesSkill = pcm.Def.MagicId >= 100 && pcm.Def.MagicId <= 111;
            }
            if (boSeriesSkill)
            {
                defSpellSpend = MShare.MySelf.m_nIPower;
            }
            else
            {
                defSpellSpend = MShare.MySelf.Abil.MP;
            }
            if (spellSpend <= defSpellSpend)
            {
                if (pcm.Def.EffectType == 0)
                {
                    if (new ArrayList(new int[] { 68, 78 }).Contains(pcm.Def.MagicId))
                    {
                        boContinue = true;
                        goto labSpell;
                    }
                    switch (pcm.Def.MagicId)
                    {
                        case 26 when MShare.g_boNextTimeFireHit || (MShare.GetTickCount() - MShare.LatestFireHitTick <= 10 * 1000): // 烈火时间间隔
                        case 66 when MShare.g_boCanSLonHit || (MShare.GetTickCount() - MShare.LatestSLonHitTick <= 8 * 1000):
                        case 43 when MShare.g_boNextTimeTwinHit || (MShare.GetTickCount() - MShare.g_dwLatestTwinHitTick <= 15 * 1000):
                        case 56 when MShare.g_boNextTimePursueHit || (MShare.GetTickCount() - MShare.g_dwLatestPursueHitTick <= 10 * 1000): // 野蛮时间间隔
                        case 27 when MShare.GetTickCount() - MShare.g_dwLatestRushRushTick <= 3 * 1000:
                            return;
                        case 100 when boContinue || (CanNextAction() && ServerAcceptNextAction() && CanNextHit()):
                            break;
                        case 100:
                        case 101 when MShare.g_boNextTimeSmiteHit || (MShare.GetTickCount() - MShare.g_dwLatestSmiteHitTick <= 1 * 100):
                        case 102 when MShare.g_boNextTimeSmiteLongHit || (MShare.GetTickCount() - MShare.g_dwLatestSmiteLongHitTick <= 1 * 100):
                        case 103 when MShare.g_boNextTimeSmiteWideHit || (MShare.GetTickCount() - MShare.g_dwLatestSmiteWideHitTick <= 1 * 100):
                        case 113 when MShare.g_boNextTimeSmiteLongHit2 || (MShare.GetTickCount() - MShare.g_dwLatestSmiteLongHitTick2 <= 10 * 1000):
                        case 114 when MShare.g_boNextTimeSmiteWideHit2 || (MShare.GetTickCount() - MShare.g_dwLatestSmiteWideHitTick2 <= 2 * 1000):
                        case 115 when MShare.g_boNextTimeSmiteLongHit3 || (MShare.GetTickCount() - MShare.g_dwLatestSmiteLongHitTick3 <= 2 * 1000):
                            return;
                    }
                    if (MShare.SpeedRate)
                    {
                        if (boContinue || (MShare.GetTickCount() - MShare.LatestSpellTick > MShare.SpellTime - ((long)MShare.g_MagSpeedRate) * 20))
                        {
                            MShare.LatestSpellTick = MShare.GetTickCount();
                            MShare.g_dwMagicDelayTime = 0;
                            SendSpellMsg(Messages.CM_SPELL, MShare.MySelf.m_btDir, 0, pcm.Def.MagicId, 0, false);
                        }
                    }
                    else
                    {
                        if (boContinue || (MShare.GetTickCount() - MShare.LatestSpellTick > MShare.SpellTime))
                        {
                            MShare.LatestSpellTick = MShare.GetTickCount();
                            MShare.g_dwMagicDelayTime = 0;
                            SendSpellMsg(Messages.CM_SPELL, MShare.MySelf.m_btDir, 0, pcm.Def.MagicId, 0, false);
                        }
                    }
                }
            labSpell:
                fUnLockMagic = new ArrayList(new[] { 2, 9, 10, 14, 21, 33, 37, 41, 46, 50, 58, 70, 72, 75 }).Contains(pcm.Def.MagicId);
                if (fUnLockMagic)
                {
                    MShare.MagicTarget = MShare.FocusCret;
                }
                else
                {
                    if (MShare.MagicLock && PlayScene.IsValidActor(MShare.FocusCret) && !MShare.FocusCret.Death)
                    {
                        MShare.MagicLockActor = MShare.FocusCret;
                    }
                    MShare.MagicTarget = MShare.MagicLockActor;
                }
                if (MShare.MagicTarget != null)
                {
                    if (!MShare.MagicLock || MShare.MagicTarget.Death || (MShare.MagicTarget.Race == ActorRace.Merchant) || !PlayScene.IsValidActor(MShare.MagicTarget))
                    {
                        MShare.MagicTarget = null;
                        MShare.MagicLockActor = null;
                    }
                }
                if ((MShare.MagicTarget != null) && (MShare.MagicTarget is THumActor))
                {
                    if (((THumActor)MShare.MagicTarget).StallMgr.OnSale)
                    {
                        MShare.MagicTarget = null;
                        MShare.MagicLockActor = null;
                    }
                }
                SmartChangePoison(pcm);
                int targid;
                if (MShare.MagicTarget == null)
                {
                    if (boReacll)
                    {
                        targx = tx;
                        targy = ty;
                    }
                    targid = 0;
                }
                else
                {
                    if (!boReacll)
                    {
                        targx = MShare.MagicTarget.CurrX;
                        targy = MShare.MagicTarget.CurrY;
                    }
                    else
                    {
                        targx = tx;
                        targy = ty;
                    }
                    targid = MShare.MagicTarget.RecogId;
                }
                if ((Math.Abs(MShare.MySelf.CurrX - targx) > BotConst.MagicRange) || (Math.Abs(MShare.MySelf.CurrY - targy) > BotConst.MagicRange))
                {
                    if (MShare.g_gcTec[14] && (fUnLockMagic || (targid != 0)))
                    {
                        PlayScene.ProcMagic.NTargetX = (short)targx;
                        PlayScene.ProcMagic.NTargetY = (short)targy;
                        PlayScene.ProcMagic.XMagic = pcm;
                        PlayScene.ProcMagic.XTarget = MShare.MagicLockActor;
                        PlayScene.ProcMagic.FReacll = boReacll;
                        PlayScene.ProcMagic.FContinue = boContinue;
                        PlayScene.ProcMagic.FUnLockMagic = fUnLockMagic;
                        PlayScene.ProcMagic.DwTick = MShare.GetTickCount();
                    }
                    else
                    {
                        if (MShare.GetTickCount() - GDwOverSpaceWarningTick > 1000)
                        {
                            GDwOverSpaceWarningTick = MShare.GetTickCount();
                            DScreen.AddSysMsg("目标太远了，施展魔法失败！！！");
                        }
                        PlayScene.ProcMagic.NTargetX = -1;
                    }
                    return;
                }
                PlayScene.ProcMagic.NTargetX = -1;
                if (boContinue || (CanNextAction() && ServerAcceptNextAction()))
                {
                    MShare.LatestSpellTick = MShare.GetTickCount();
                    tdir = BotHelper.GetFlyDirection(MShare.MySelf.CurrX, MShare.MySelf.CurrY, targx, targy);
                    TUseMagicInfo pmag = new TUseMagicInfo();
                    pmag.EffectNumber = pcm.Def.Effect;
                    pmag.MagicSerial = pcm.Def.MagicId;
                    pmag.ServerMagicCode = 0;
                    MShare.g_dwMagicDelayTime = 200 + pcm.Def.DelayTime;
                    MShare.g_dwMagicPKDelayTime = 0;
                    if (MShare.MagicTarget != null)
                    {
                        if (MShare.MagicTarget.Race == 0)
                        {
                            MShare.g_dwMagicPKDelayTime = 300 + RandomNumber.GetInstance().Random(1100);
                        }
                    }
                    //MShare.g_MySelf.SendMsg(Messages.CM_SPELL, targx, targy, tdir, pmag, targid, "", 0);
                }
            }
            else
            {
                if (boSeriesSkill)
                {
                    if (MShare.GetTickCount() - MShare.g_IPointLessHintTick > 5000)
                    {
                        MShare.g_IPointLessHintTick = MShare.GetTickCount();
                        DScreen.AddSysMsg($"需要 {spellSpend} 内力值才能释放 {pcm.Def.MagicName}");
                    }
                }
                else if (MShare.GetTickCount() - MShare.g_MPLessHintTick > 1000)
                {
                    MShare.g_MPLessHintTick = MShare.GetTickCount();
                    DScreen.AddSysMsg($"需要 {spellSpend} 魔法值才能释放 {pcm.Def.MagicName}");
                }
            }
        }

        private void UseMagicSpell(int who, int effnum, int targetx, int targety, int magicId)
        {
            Actor actor = PlayScene.FindActor(who);
            if (actor != null)
            {
                byte adir = BotHelper.GetFlyDirection(actor.CurrX, actor.CurrY, targetx, targety);
                TUseMagicInfo useMagic = new TUseMagicInfo();
                useMagic.EffectNumber = effnum % 255;
                useMagic.ServerMagicCode = 0;
                useMagic.MagicSerial = magicId % 300;
                //Actor.SendMsg(Messages.SM_SPELL, effnum / 255, magic_id / 300, adir, UseMagic, 0, "", 0);
                MShare.g_nSpellCount++;
            }
            else
            {
                MShare.g_nSpellFailCount++;
            }
        }

        private void UseMagicFire(int who, int efftype, int effnum, int targetx, int targety, int target, int maglv)
        {
            Actor actor = PlayScene.FindActor(who);
            if (actor != null)
            {
                actor.SendMsg(Messages.SM_MAGICFIRE, (ushort)(short)target, (ushort)(short)efftype, effnum, targetx, targety, maglv.ToString(), 0);
                if (MShare.g_nFireCount < MShare.g_nSpellCount)
                {
                    MShare.g_nFireCount++;
                }
            }
            MShare.MagicTarget = null;
        }

        private void UseMagicFireFail(int who)
        {
            Actor actor = PlayScene.FindActor(who);
            if (actor != null)
            {
                actor.SendMsg(Messages.SM_MAGICFIRE_FAIL, 0, 0, 0, 0, 0, "", 0);
            }
            MShare.MagicTarget = null;
        }

        public void ActorAutoEat(THumActor actor)
        {
            if (!actor.Death)
            {
                ActorCheckHealth(false);
                if (MShare.g_EatingItem.Item.Name == "")
                {
                    if (MShare.IsPersentSpc(actor.Abil.HP, actor.Abil.MaxHP))
                    {
                        ActorCheckHealth(true);
                    }
                }
            }
        }

        public void ActorCheckHealth(bool bNeedSp)
        {
            int nCount = 0;
            int hidx = -1;
            int midx = -1;
            int sidx = -1;
            int bidx = -1;
            int uhidx = -1;
            int umidx = -1;
            int usidx = -1;
            int ubidx = -1;
            int maxHp = int.MaxValue / 2 - 1;
            int maxMp = int.MaxValue / 2 - 1;
            int maxSp = int.MaxValue / 2 - 1;
            for (int i = Grobal2.MaxBagItem - (1 + 0); i >= 0; i--)
            {
                if ((MShare.ItemArr[i].Item.Name != "") && (MShare.ItemArr[i].Item.NeedIdentify < 4))
                {
                    switch (MShare.ItemArr[i].Item.StdMode)
                    {
                        case 00:
                            switch (MShare.ItemArr[i].Item.Shape)
                            {
                                case 0: // 普通药
                                    if (MShare.g_gcProtect[0] && (MShare.ItemArr[i].Item.AC > 0) && (MShare.ItemArr[i].Item.AC < maxHp))
                                    {
                                        maxHp = MShare.ItemArr[i].Item.AC;
                                        hidx = i;
                                    }
                                    if (MShare.g_gcProtect[1] && (MShare.ItemArr[i].Item.MAC > 0) && (MShare.ItemArr[i].Item.MAC < maxMp))
                                    {
                                        maxMp = MShare.ItemArr[i].Item.MAC;
                                        midx = i;
                                    }
                                    break;
                                case 1: // 速效药
                                    if (MShare.g_gcProtect[3] && (MShare.ItemArr[i].Item.AC > 0) && (MShare.ItemArr[i].Item.AC < maxSp))
                                    {
                                        maxSp = MShare.ItemArr[i].Item.AC;
                                        sidx = i;
                                    }
                                    break;
                            }
                            break;
                        case 2:
                        case 3:
                            if (MShare.g_gcProtect[5])
                            {
                                if (String.Compare(MShare.ItemArr[i].Item.Name, BotConst.g_sRenewBooks[MShare.g_gnProtectPercent[6]], StringComparison.OrdinalIgnoreCase) == 0)
                                {
                                    bidx = i;
                                }
                            }
                            break;
                        case 31:
                            switch (MShare.ItemArr[i].Item.AniCount)
                            {
                                case 1:
                                    if (MShare.g_gcProtect[0])
                                    {
                                        uhidx = i;
                                    }
                                    break;
                                case 2:
                                    if (MShare.g_gcProtect[1])
                                    {
                                        umidx = i;
                                    }
                                    break;
                                case 3:
                                    if (MShare.g_gcProtect[3])
                                    {
                                        usidx = i;
                                    }
                                    break;
                                default:
                                    if (MShare.g_gcProtect[5] && (string.Compare(MShare.ItemArr[i].Item.Name, BotConst.g_sRenewBooks[MShare.g_gnProtectPercent[6]] + "包", StringComparison.OrdinalIgnoreCase) == 0))
                                    {
                                        ubidx = i;
                                    }
                                    break;
                            }
                            break;
                    }
                }
                else
                {
                    nCount++;
                }
            }
            bool bHint = false;
            bool bEatSp = false;
            bool bEatOk = false;
            if (MShare.GetTickCount() - MShare.MySelf.m_dwMsgHint > 15 * 1000)
            {
                MShare.MySelf.m_dwMsgHint = MShare.GetTickCount();
                bHint = true;
            }
            if (!bNeedSp)
            {
                if (MShare.g_gcProtect[0] && MShare.IsPersentHP(MShare.MySelf.Abil.HP, MShare.MySelf.Abil.MaxHP))
                {
                    if (MShare.GetTickCount() - MShare.MySelf.m_dwHealthHP > MShare.g_gnProtectTime[0])
                    {
                        MShare.MySelf.m_dwHealthHP = MShare.GetTickCount();
                        if (hidx > -1)
                        {
                            EatItem(hidx);
                            bEatOk = true;
                        }
                        else if ((nCount > 4) && (uhidx > -1))
                        {
                            EatItem(uhidx);
                            bEatOk = true;
                        }
                        else
                        {
                            bEatSp = true;
                            if (bHint)
                            {
                                ScreenManager.AddChatBoardString("你的金创药已经用完！");
                            }
                            bEatOk = false;
                        }
                    }
                }
            }
            if (!bNeedSp)
            {
                if (MShare.g_gcProtect[1] && MShare.IsPersentMP(MShare.MySelf.Abil.MP, MShare.MySelf.Abil.MaxMP))
                {
                    if (MShare.GetTickCount() - MShare.MySelf.m_dwHealthMP > MShare.g_gnProtectTime[1])
                    {
                        MShare.MySelf.m_dwHealthMP = MShare.GetTickCount();
                        if (midx > -1)
                        {
                            EatItem(midx);
                            bEatOk = true;
                        }
                        else if ((nCount > 4) && (umidx > -1))
                        {
                            EatItem(umidx);
                            bEatOk = true;
                        }
                        else
                        {
                            if (MShare.g_gcProtect[11])
                            {
                                bEatSp = true;
                            }
                            if (bHint)
                            {
                                ScreenManager.AddChatBoardString("你的魔法药已经用完！");
                            }
                            bEatOk = false;
                        }
                    }
                }
            }
            if (!bEatOk)
            {
                if (MShare.g_gcProtect[3] && (bNeedSp || bEatSp || (MShare.g_gcProtect[11] && MShare.IsPersentSpc(MShare.MySelf.Abil.MP, MShare.MySelf.Abil.MaxMP))))
                {
                    if (MShare.GetTickCount() - MShare.MySelf.m_dwHealthSP > MShare.g_gnProtectTime[3])
                    {
                        MShare.MySelf.m_dwHealthSP = MShare.GetTickCount();
                        if (sidx > -1)
                        {
                            EatItem(sidx);
                        }
                        else if ((nCount > 4) && (usidx > -1))
                        {
                            EatItem(usidx);
                        }
                        else if (bHint)
                        {
                            ScreenManager.AddChatBoardString("你的特殊药品已经用完！");
                        }
                    }
                }
            }
            if (MShare.g_gcProtect[5] && MShare.IsPersentBook(MShare.MySelf.Abil.HP, MShare.MySelf.Abil.MaxHP))
            {
                if (MShare.GetTickCount() - MShare.MySelf.m_dwHealthBK > MShare.g_gnProtectTime[5])
                {
                    MShare.MySelf.m_dwHealthBK = MShare.GetTickCount();
                    if (bidx > -1)
                    {
                        EatItem(bidx);
                    }
                    else if ((nCount > 4) && (ubidx > -1))
                    {
                        EatItem(ubidx);
                    }
                    else if (bHint)
                    {
                        ScreenManager.AddChatBoardString("你的" + BotConst.g_sRenewBooks[MShare.g_gnProtectPercent[6]] + "已经用完！");
                    }
                }
            }
        }

        private void AutoSupplyBeltItem(int nType, int idx, string sItem)
        {
            if (idx >= 0 && idx <= 5 && (sItem != ""))
            {
                if (MShare.ItemArr[idx].Item.Name == "")
                {
                    for (int i = BotConst.MaxBagItemcl - 1; i >= 6; i--)
                    {
                        if (MShare.ItemArr[i].Item.Name == sItem)
                        {
                            MShare.ItemArr[idx] = MShare.ItemArr[i];
                            MShare.ItemArr[i].Item.Name = "";
                            return;
                        }
                    }
                    AutoUnBindItem(nType, sItem);
                }
            }
        }

        private void AutoSupplyBagItem(int nType, string sItem)
        {
            for (int i = BotConst.MaxBagItemcl - 1; i >= 6; i--)
            {
                if (MShare.ItemArr[i].Item.Name == sItem)
                {
                    return;
                }
            }
            AutoUnBindItem(nType, sItem);
        }

        /// <summary>
        /// 自动解包物品
        /// </summary>
        private void AutoUnBindItem(int nType, string sItem)
        {
            if ((!string.IsNullOrEmpty(sItem)) && (nType != 0))
            {
                bool boIsUnBindItem = false;
                for (int i = 0; i < BotConst.g_UnBindItems.Length; i++)
                {
                    if (sItem == BotConst.g_UnBindItems[i])
                    {
                        boIsUnBindItem = true;
                        break;
                    }
                }
                if (!boIsUnBindItem)
                {
                    return;
                }
                int n = 0;
                bool boUnBindAble = false;
                for (int i = 0; i < BotConst.MaxBagItemcl - 1 - 6; i++)
                {
                    if (MShare.ItemArr[i].Item.Name == "")
                    {
                        n++;
                        if (n >= 5)
                        {
                            boUnBindAble = true;
                            break;
                        }
                    }
                }
                if (!boUnBindAble)
                {
                    return;
                }
                int idx = -1;
                for (int i = BotConst.MaxBagItemcl - 1; i >= 6; i--)
                {
                    if (MShare.ItemArr[i].Item.StdMode == 31)
                    {
                        if (MShare.ItemArr[i].Item.Name != "")
                        {
                            if (MShare.ItemArr[i].Item.Shape == nType)
                            {
                                idx = i;
                                break;
                            }
                        }
                    }
                }
                if (idx > -1)
                {
                    SendEat(MShare.ItemArr[idx].MakeIndex, "", MShare.ItemArr[idx].Item.StdMode);
                    if (MShare.ItemArr[idx].Dura > 1)
                    {
                        MShare.ItemArr[idx].Dura = (ushort)(MShare.ItemArr[idx].Dura - 1);
                        MShare.g_EatingItem = MShare.ItemArr[idx];
                        MNEatRetIdx = -1;
                    }
                    else
                    {
                        MShare.ItemArr[idx].Dura = (ushort)(MShare.ItemArr[idx].Dura - 1);
                        MShare.g_EatingItem = MShare.ItemArr[idx];
                        MShare.ItemArr[idx].Item.Name = "";
                        MNEatRetIdx = -1;
                    }
                }
            }
        }

        private bool EatItemName(string str)
        {
            if ((str == "小退") && (MShare.MySelf.HiterCode > 0))
            {
                AppLogout();
                return false;
            }
            if ((str == "大退") && (MShare.MySelf.HiterCode > 0))
            {
                return false;
            }
            for (int i = 0; i < BotConst.MaxBagItemcl; i++)
            {
                if ((MShare.ItemArr[i].Item.Name == str) && (MShare.ItemArr[i].Item.NeedIdentify < 4))
                {
                    EatItem(i);
                    return true;
                }
            }
            return false;
        }

        private void EatItem(int idx)
        {
            int i;
            bool eatable = false;
            bool takeon = false;
            int where = -1;
            if (idx >= 0 && idx <= BotConst.MaxBagItemcl - 1)
            {
                if ((MShare.g_EatingItem.Item.Name != "") && (MShare.GetTickCount() - MShare.g_dwEatTime > 5 * 1000))
                {
                    MShare.g_EatingItem.Item.Name = "";
                }
                if ((MShare.g_EatingItem.Item.Name == "") && (MShare.ItemArr[idx].Item.Name != "") && (MShare.ItemArr[idx].Item.NeedIdentify < 4))
                {
                    if ((MShare.ItemArr[idx].Item.StdMode <= 3) || (MShare.ItemArr[idx].Item.StdMode == 31))
                    {
                        if (MShare.ItemArr[idx].Dura > 1)
                        {
                            MShare.ItemArr[idx].Dura = (ushort)(MShare.ItemArr[idx].Dura - 1);
                            MShare.g_EatingItem = MShare.ItemArr[idx];
                            MShare.ItemArr[idx].Item.Name = "";
                            eatable = true;
                        }
                        else
                        {
                            MShare.g_EatingItem = MShare.ItemArr[idx];
                            MShare.ItemArr[idx].Item.Name = "";
                            eatable = true;
                        }
                    }
                    else
                    {
                    lab1:
                        if ((MShare.ItemArr[idx].Item.StdMode == 46) && MShare.ItemArr[idx].Item.Shape >= 2 && MShare.ItemArr[idx].Item.Shape <= 6)
                        {
                            //if (!MShare.g_RareBoxWindow.m_boKeyAvail && (MShare.g_OpenBoxItem.Item.Item.Name == "") && !FrmDlg.DWBoxBKGnd.Visible)
                            //{
                            //    MShare.g_OpenBoxItem.Index = idx;
                            //    MShare.g_OpenBoxItem.Item = MShare.g_ItemArr[idx];
                            //    MShare.g_ItemArr[idx].Item.Name = "";
                            //    //FrmDlg.DWBoxBKGnd.Visible = true;
                            //}
                            return;
                        }
                        if ((MShare.ItemArr[idx].Item.StdMode == 41) && new ArrayList(new int[] { 10, 30 }).Contains(MShare.ItemArr[idx].Item.Shape) && (MShare.g_BuildAcusesStep != 1))
                        {
                            for (i = 0; i < 7; i++)
                            {
                                if (MShare.g_BuildAcuses[i].Item.Item.Name == "")
                                {
                                    if ((MShare.ItemArr[idx].Item.Shape >= 30 && MShare.ItemArr[idx].Item.Shape <= 34 && i >= 5 && i <= 7) || (MShare.ItemArr[idx].Item.Shape >= 10 && MShare.ItemArr[idx].Item.Shape <= 14 && i >= 0 && i <= 4))
                                    {
                                        break;
                                    }
                                }
                            }
                            if (i >= 0 && i <= 7)
                            {
                                MShare.g_boItemMoving = true;
                                MShare.MovingItem.Index = idx;
                                MShare.MovingItem.Item = MShare.ItemArr[idx];
                                MShare.ItemArr[idx].Item.Name = "";
                            }
                            return;
                        }
                        where = BotHelper.GetTakeOnPosition(MShare.ItemArr[idx], MShare.UseItems, true);
                        if (where >= 0 && where <= 13)
                        {
                            takeon = true;
                            MShare.g_EatingItem = MShare.ItemArr[idx];
                            MShare.ItemArr[idx].Item.Name = "";
                        }
                    }
                }
            }
            else if ((idx == -1) && MShare.g_boItemMoving)
            {
                if ((MShare.MovingItem.Item.Item.StdMode <= 4) || (MShare.MovingItem.Item.Item.StdMode == 31) && (MShare.MovingItem.Item.Item.NeedIdentify < 4))
                {
                    if (((MShare.MovingItem.Item.Item.StdMode <= 3) || (MShare.MovingItem.Item.Item.StdMode == 31)) && (MShare.MovingItem.Item.Dura > 1))
                    {
                        MShare.MovingItem.Item.Dura = (ushort)(MShare.MovingItem.Item.Dura - 1);
                        MShare.g_boItemMoving = false;
                        MShare.g_EatingItem = MShare.MovingItem.Item;
                        MShare.MovingItem.Item.Item.Name = "";
                    }
                    else
                    {
                        MShare.g_boItemMoving = false;
                        MShare.g_EatingItem = MShare.MovingItem.Item;
                        MShare.MovingItem.Item.Item.Name = "";
                    }
                    if ((MShare.g_EatingItem.Item.StdMode == 4) && (MShare.g_EatingItem.Item.Shape < 50))
                    {
                        MainOutMessage($"练习{MShare.g_EatingItem.Item.Name}技能");
                        BotHelper.AddItemBag(MShare.g_EatingItem);
                        return;
                    }
                    idx = MNEatRetIdx;
                    eatable = true;
                }
                else
                {
                lab2:
                    if ((MShare.MovingItem.Item.Item.StdMode == 46) && MShare.MovingItem.Item.Item.Shape >= 2 && MShare.MovingItem.Item.Item.Shape <= 6)
                    {
                        //if (!MShare.g_RareBoxWindow.m_boKeyAvail && (MShare.g_OpenBoxItem.Item.Item.Name == "") && !FrmDlg.DWBoxBKGnd.Visible)
                        //{
                        //    MShare.g_OpenBoxItem.Index = m_nEatRetIdx;
                        //    MShare.g_OpenBoxItem.Item = MShare.g_MovingItem.Item;
                        //    MShare.g_boItemMoving = false;
                        //    MShare.g_MovingItem.Item.Item.Name = "";
                        //}
                        return;
                    }
                    if ((MShare.MovingItem.Item.Item.StdMode == 41) && new ArrayList(new int[] { 10, 30 }).Contains(MShare.MovingItem.Item.Item.Shape) && (MShare.g_BuildAcusesStep != 1))
                    {
                        for (i = 0; i < 7; i++)
                        {
                            if (MShare.g_BuildAcuses[i].Item.Item.Name == "")
                            {
                                if ((MShare.MovingItem.Item.Item.Shape >= 30 && MShare.MovingItem.Item.Item.Shape <= 34 && i >= 5 && i <= 7) || (MShare.MovingItem.Item.Item.Shape >= 10 && MShare.MovingItem.Item.Item.Shape <= 14 && i >= 0 && i <= 4))
                                {
                                    break;
                                }
                            }
                        }
                        return;
                    }
                    where = BotHelper.GetTakeOnPosition(MShare.MovingItem.Item, MShare.UseItems, true);
                    if (where >= 0 && where <= 13)
                    {
                        takeon = true;
                        MShare.g_boItemMoving = false;
                        MShare.g_EatingItem = MShare.MovingItem.Item;
                        MShare.MovingItem.Item.Item.Name = "";
                        idx = MNEatRetIdx;
                    }
                }
            }
            if (eatable)
            {
                MNEatRetIdx = idx;
                MBoSupplyItem = true;
                MShare.g_dwEatTime = MShare.GetTickCount();
                SendEat(MShare.g_EatingItem.MakeIndex, MShare.g_EatingItem.Item.Name, MShare.g_EatingItem.Item.StdMode);
            }
            else if (takeon)
            {
                MNEatRetIdx = idx;
                MShare.g_dwEatTime = MShare.GetTickCount();
                MShare.g_WaitingUseItem.Item = MShare.g_EatingItem;
                MShare.g_WaitingUseItem.Index = where;
                SendTakeOnItem(where, MShare.g_EatingItem.MakeIndex, MShare.g_EatingItem.Item.Name);
                MShare.g_EatingItem.Item.Name = "";
            }
        }

        private bool TargetInSwordLongAttackRange(int ndir)
        {
            if (MShare.g_gcTec[0])
            {
                return true;
            }
            short nX = 0;
            short nY = 0;
            BotHelper.GetFrontPosition(MShare.MySelf.CurrX, MShare.MySelf.CurrY, ndir, ref nX, ref nY);
            BotHelper.GetFrontPosition(nX, nY, ndir, ref nX, ref nY);
            if ((Math.Abs(MShare.MySelf.CurrX - nX) == 2) || (Math.Abs(MShare.MySelf.CurrY - nY) == 2))
            {
                Actor actor = PlayScene.FindActorXY(nX, nY);
                if (actor != null)
                {
                    if (!actor.Death)
                    {
                        return true;
                    }
                }
            }
            return false;
        }

        private static bool TargetInSwordLongAttackRange2(int sx, int sy, int dx, int dy)
        {
            if ((Math.Abs(sx - dx) == 2) && (Math.Abs(sy - dy) == 0))
            {
                return true;
            }
            if ((Math.Abs(sx - dx) == 0) && (Math.Abs(sy - dy) == 2))
            {
                return true;
            }
            if ((Math.Abs(sx - dx) == 2) && (Math.Abs(sy - dy) == 2))
            {
                return true;
            }
            return false;
        }

        private bool TargetInSwordWideAttackRange(int ndir)
        {
            short nX = 0;
            short nY = 0;
            short rx = 0;
            short ry = 0;
            bool result = false;
            BotHelper.GetFrontPosition(MShare.MySelf.CurrX, MShare.MySelf.CurrY, ndir, ref nX, ref nY);
            Actor actor = PlayScene.FindActorXY(nX, nY);
            int mdir = (ndir + 1) % 8;
            BotHelper.GetFrontPosition(MShare.MySelf.CurrX, MShare.MySelf.CurrY, mdir, ref rx, ref ry);
            Actor ractor = PlayScene.FindActorXY(rx, ry);
            if (ractor == null)
            {
                mdir = (ndir + 2) % 8;
                BotHelper.GetFrontPosition(MShare.MySelf.CurrX, MShare.MySelf.CurrY, mdir, ref rx, ref ry);
                ractor = PlayScene.FindActorXY(rx, ry);
            }
            if (ractor == null)
            {
                mdir = (ndir + 7) % 8;
                BotHelper.GetFrontPosition(MShare.MySelf.CurrX, MShare.MySelf.CurrY, mdir, ref rx, ref ry);
                ractor = PlayScene.FindActorXY(rx, ry);
            }
            if ((actor != null) && (ractor != null))
            {
                if (!actor.Death && !ractor.Death)
                {
                    result = true;
                }
            }
            return result;
        }

        public bool TargetInSwordLongAttackRangeX(int ndir)
        {
            short nX = 0;
            short nY = 0;
            short nC = 1;
            bool result = false;
            while (true)
            {
                if (GetNextPosition(MShare.MySelf.CurrX, MShare.MySelf.CurrY, ndir, nC, ref nX, ref nY))
                {
                    Actor actor = PlayScene.FindActorXY(nX, nY);
                    if ((actor != null) && !actor.Death)
                    {
                        result = true;
                        break;
                    }
                }
                nC++;
                if (nC >= 5)
                {
                    break;
                }
            }
            return result;
        }

        public bool TargetInSwordLongAttackRangeA(int ndir)
        {
            short nX = 0;
            short nY = 0;
            Actor actor;
            bool result = false;
            short nC = 1;
            while (true)
            {
                if (GetNextPosition(MShare.MySelf.CurrX, MShare.MySelf.CurrY, ndir, nC, ref nX, ref nY))
                {
                    actor = PlayScene.FindActorXY(nX, nY);
                    if ((actor != null) && !actor.Death)
                    {
                        result = true;
                        break;
                    }
                }
                nC++;
                if (nC >= 4)
                {
                    break;
                }
            }
            return result;
        }

        public bool TargetInSwordCrsAttackRange(int ndir)
        {
            short nX = 0;
            short nY = 0;
            short rx = 0;
            short ry = 0;
            bool result = false;
            BotHelper.GetFrontPosition(MShare.MySelf.CurrX, MShare.MySelf.CurrY, ndir, ref nX, ref nY);
            Actor actor = PlayScene.FindActorXY(nX, nY);
            int mdir = (ndir + 1) % 8;
            BotHelper.GetFrontPosition(MShare.MySelf.CurrX, MShare.MySelf.CurrY, mdir, ref rx, ref ry);
            Actor ractor = PlayScene.FindActorXY(rx, ry);
            if (ractor == null)
            {
                mdir = (ndir + 2) % 8;
                BotHelper.GetFrontPosition(MShare.MySelf.CurrX, MShare.MySelf.CurrY, mdir, ref rx, ref ry);
                ractor = PlayScene.FindActorXY(rx, ry);
            }
            if (ractor == null)
            {
                mdir = (ndir + 7) % 8;
                BotHelper.GetFrontPosition(MShare.MySelf.CurrX, MShare.MySelf.CurrY, mdir, ref rx, ref ry);
                ractor = PlayScene.FindActorXY(rx, ry);
            }
            if ((actor != null) && (ractor != null))
            {
                if (!actor.Death && !ractor.Death)
                {
                    result = true;
                }
            }
            return result;
        }

        public bool AttackTarget(Actor target)
        {
            bool result = false;
            // 守卫（2026-10-10 压测实测）：MShare.MySelf 是**跨假人共享**的全局态，地图切换/进图窗口里
            // 会是 null；原来直接解引用 ⇒ NullReferenceException 把整个 BotSrv 打崩（200 假人档实测）。
            if (MShare.MySelf == null || target == null)
            {
                return false;
            }
            int nHitMsg = Messages.CM_HIT;
            if (MShare.UseItems[ItemLocation.Weapon] != null && MShare.UseItems[ItemLocation.Weapon].Item.StdMode == 6)
            {
                nHitMsg = Messages.CM_HEAVYHIT;
            }
            byte tdir = BotHelper.GetNextDirection(MShare.MySelf.CurrX, MShare.MySelf.CurrY, target.CurrX, target.CurrY);
            if ((Math.Abs(MShare.MySelf.CurrX - target.CurrX) <= 1) && (Math.Abs(MShare.MySelf.CurrY - target.CurrY) <= 1) && (!target.Death))
            {
                if (TimerAutoPlay.Enabled)
                {
                    MShare.AutoMove = false;
                    if (MShare.AutoTagget != null)
                    {
                        MainOutMessage($"怪物目标：{MShare.AutoTagget.UserName} ({MShare.AutoTagget.CurrX},{MShare.AutoTagget.CurrY}) 正在使用普通攻击");
                    }
                }
                if (CanNextAction() && ServerAcceptNextAction())
                {
                    if (CanNextHit())
                    {
                        if (MShare.g_boNextTimeFireHit && (MShare.MySelf.Abil.MP >= 7))
                        {
                            MShare.g_boNextTimeFireHit = false;
                            nHitMsg = Messages.CM_FIREHIT;
                        }
                        else if (MShare.g_boNextTimePowerHit)
                        {
                            MShare.g_boNextTimePowerHit = false;
                            nHitMsg = Messages.CM_POWERHIT;
                        }
                        else if ((MShare.MySelf.Abil.MP >= 3) && (MShare.g_boCanWideHit || (MShare.g_gcTec[1] && (GetMagicById(25) != null) && TargetInSwordWideAttackRange(tdir))))
                        {
                            nHitMsg = Messages.CM_WIDEHIT;
                        }
                        else if (MShare.g_boCanCrsHit && (MShare.MySelf.Abil.MP >= 6))
                        {
                            nHitMsg = Messages.CM_CRSHIT;
                        }
                        else if (MShare.g_boCanLongHit && TargetInSwordLongAttackRange(tdir))
                        {
                            nHitMsg = Messages.CM_LONGHIT;
                        }
                        MShare.MySelf.SendMsg(nHitMsg, (ushort)MShare.MySelf.CurrX, (ushort)MShare.MySelf.CurrY, tdir, 0, 0, "", 0);
                    }
                }
                result = true;
                MShare.LastAttackTick = MShare.GetTickCount();
            }
            else
            {
                if (MShare.g_boCanLongHit && (MShare.MySelf.Job == 0) && (!target.Death) && MShare.g_boAutoLongAttack && MShare.g_gcTec[10] && (MShare.MagicArr[12] != null) && TargetInSwordLongAttackRange2(MShare.MySelf.CurrX, MShare.MySelf.CurrY, target.CurrX, target.CurrY))
                {
                    if (CanNextAction() && ServerAcceptNextAction() && CanNextHit())
                    {
                        nHitMsg = Messages.CM_LONGHIT;
                        MShare.MySelf.SendMsg(nHitMsg, (ushort)MShare.MySelf.CurrX, (ushort)MShare.MySelf.CurrY, tdir, 0, 0, "", 0);
                        MShare.LastAttackTick = MShare.GetTickCount();
                    }
                    else if (MShare.g_boAutoLongAttack && MShare.g_gcTec[10] && TimerAutoPlay.Enabled)// 走刺杀位
                    {
                        return true;
                    }
                }
                else
                {
                    short dx = MShare.MySelf.CurrX;
                    short dy = MShare.MySelf.CurrY;
                    if ((MShare.MySelf.Job == 0) && MShare.g_boAutoLongAttack && MShare.g_gcTec[10] && (MShare.MagicArr[12] != null))
                    {
                        BotHelper.GetNextHitPosition(target.CurrX, target.CurrY, ref dx, ref dy);
                        if (!PlayScene.CanWalk(dx, dy))
                        {
                            BotHelper.GetBackPosition(target.CurrX, target.CurrY, tdir, ref dx, ref dy);
                        }
                    }
                    else
                    {
                        BotHelper.GetBackPosition(target.CurrX, target.CurrY, tdir, ref dx, ref dy);
                    }
                    MShare.TargetX = dx;
                    MShare.TargetY = dy;
                    MShare.PlayerAction = PlayerAction.Run;
                }
                if (TimerAutoPlay.Enabled)
                {
                    MShare.AutoMove = true;
                    MainOutMessage($"跑向目标怪物：{target.UserName} ({target.CurrX},{target.CurrY}) ");
                }
            }
            return result;
        }

        private bool CheckDoorAction(int dx, int dy)
        {
            bool result = false;
            int door = Map.GetDoor(dx, dy);
            if (door > 0)
            {
                if (!Map.IsDoorOpen(dx, dy))
                {
                    SendClientMessage(Messages.CM_OPENDOOR, door, dx, dy, 0);
                    result = true;
                }
            }
            return result;
        }

        public void MouseTimerTimer(object sender, EventArgs e1)
        {
            int ii;
            int fixidx;
            Actor target;
            if ((MShare.g_gcGeneral[1] || MShare.g_gcGeneral[9]) && (MShare.GetTickCount() - MDwDuraWarningTick > 60 * 1000))
            {
                MDwDuraWarningTick = MShare.GetTickCount();
                if ((MShare.MySelf != null) && !MShare.MySelf.Death)
                {
                    for (int i = MShare.UseItems.Length; i > 0; i--)
                    {
                        if (MShare.UseItems[i].Item.Name != "")
                        {
                            if (MShare.UseItems[i].Item.StdMode == 7 || MShare.UseItems[i].Item.StdMode == 25)
                            {
                                continue;
                            }
                            if (MShare.UseItems[i].Dura < 1500)
                            {
                                if (MShare.g_gcGeneral[1])
                                {
                                    ScreenManager.AddChatBoardString($"你的[{MShare.UseItems[i].Item.Name}]持久已到底限，请及时修理！");
                                }
                                if (MShare.g_gcGeneral[9])
                                {
                                    fixidx = -1;
                                    for (ii = Grobal2.MaxBagItem - (1 + 0); ii >= 0; ii--)
                                    {
                                        if ((MShare.ItemArr[ii].Item.NeedIdentify < 4) && (MShare.ItemArr[ii].Item.Name != "") && (MShare.ItemArr[ii].Item.StdMode == 2) && (MShare.ItemArr[ii].Item.Shape == 9) && (MShare.ItemArr[ii].Dura > 0))
                                        {
                                            fixidx = ii;
                                            break;
                                        }
                                    }
                                    if (fixidx > -1)
                                    {
                                        EatItem(fixidx);
                                    }
                                    else
                                    {
                                        ScreenManager.AddChatBoardString($"你的{MShare.UseItems[i].Item.Name}已经用完，请及时补充！");
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if ((MShare.MySelf != null) && !MShare.MySelf.Death && (MShare.MySelf.m_nIPowerLvl > 5) && (MShare.MySelf.m_nIPower < 30) && (MShare.GetTickCount() - DwIpTick > 30 * 1000))
            {
                DwIpTick = MShare.GetTickCount();
                fixidx = -1;
                for (ii = Grobal2.MaxBagItem - (1 + 0); ii >= 0; ii--)
                {
                    if ((MShare.ItemArr[ii].Item.NeedIdentify < 4) && (MShare.ItemArr[ii].Item.Name != "") && (MShare.ItemArr[ii].Item.StdMode == 2) && (MShare.ItemArr[ii].Item.Shape == 13) && (MShare.ItemArr[ii].DuraMax > 0))
                    {
                        fixidx = ii;
                        break;
                    }
                }
                if (fixidx > -1)
                {
                    EatItem(fixidx);
                }
            }
            if (MShare.TargetCret != null)
            {
                if (_actionKey > 0)
                {
                    ProcessKeyMessages();
                }
                else
                {
                    MShare.TargetCret = null;
                }
            }
            if ((MShare.MySelf != null) && (MShare.g_boAutoDig || MShare.g_boAutoSit))
            {
                if (CanNextAction() && ServerAcceptNextAction() && (MShare.g_boAutoSit || CanNextHit()))
                {
                    if (MShare.g_boAutoDig)
                    {
                        MShare.MySelf.SendMsg(Messages.CM_HIT + 1, (ushort)MShare.MySelf.CurrX, (ushort)MShare.MySelf.CurrY, MShare.MySelf.m_btDir, 0, 0, "", 0);
                    }
                    if (MShare.g_boAutoSit)
                    {
                        target = PlayScene.ButchAnimal(MShare.MouseCurrX, MShare.MouseCurrY);
                        if (target != null)
                        {
                            ii = BotHelper.GetNextDirection(MShare.MySelf.CurrX, MShare.MySelf.CurrY, MShare.MouseCurrX, MShare.MouseCurrY);
                            SendButchAnimal(MShare.MouseCurrX, MShare.MouseCurrY, ii, target.RecogId);
                            MShare.MySelf.SendMsg(Messages.CM_SITDOWN, (ushort)MShare.MySelf.CurrX, (ushort)MShare.MySelf.CurrY, ii, 0, 0, "", 0);
                        }
                        else
                        {
                            ii = BotHelper.GetNextDirection(MShare.MySelf.CurrX, MShare.MySelf.CurrY, MShare.MouseCurrX, MShare.MouseCurrY);
                            SendButchAnimal(MShare.MouseCurrX, MShare.MouseCurrY, ii, MShare.g_DetectItemMineID);
                            MShare.MySelf.SendMsg(Messages.CM_SITDOWN, (ushort)MShare.MySelf.CurrX, (ushort)MShare.MySelf.CurrY, ii, 0, 0, "", 0);
                        }
                    }
                }
            }
            if (MShare.AutoPickUp && (MShare.MySelf != null) && (MShare.GetTickCount() - MShare.AutoPickupTick) > MShare.AutoPickupTime)// 动自捡取
            {
                MShare.AutoPickupTick = MShare.GetTickCount();
                AutoPickUpItem();
            }
        }

        private void AutoPickUpItem()
        {
            if (ServerAcceptNextAction())
            {
                TDropItem dropItem = PlayScene.GetXyDropItems(MShare.MySelf.CurrX, MShare.MySelf.CurrY);
                if (dropItem != null)
                {
                    if (MShare.PickUpAll || dropItem.boPickUp)
                    {
                        SendPickup();
                    }
                }
            }
        }

        public void WaitMsgTimerTimer(object sender, EventArgs e1)
        {
            if (MShare.MySelf == null)
            {
                return;
            }
            if (MShare.MySelf.ActionFinished())
            {
                //WaitMsgTimer.Enabled = false;
                switch (_waitingMsg.Ident)
                {
                    case Messages.SM_CHANGEMAP:
                        MShare.MapMovingWait = false;
                        MShare.g_boMapMoving = false;
                        if (MShare.g_nStallX != -1)
                        {
                            MShare.g_nStallX = -1;
                        }
                        ClearDropItems();
                        PlayScene.CleanObjects();
                        MShare.MapTitle = "";
                        PlayScene.SendMsg(Messages.SM_CHANGEMAP, 0, _waitingMsg.Param, _waitingMsg.Tag, (byte)_waitingMsg.Series, 0, 0, _waitingStr);
                        MShare.MySelf.CleanCharMapSetting((short)_waitingMsg.Param, (short)_waitingMsg.Tag);
                        MShare.TargetX = -1;
                        MShare.TargetCret = null;
                        MShare.FocusCret = null;
                        break;
                }
            }
        }

        public void ActiveCmdTimer(TTimerCommand cmd)
        {
            //TimerCmd = cmd;
            //CmdTimer.Enabled = true;
        }

        public void CmdTimerTimer(object sender, EventArgs e1)
        {
            //CmdTimer.Enabled = false;
            //CmdTimer.Interval = 500;
            //switch (TimerCmd)
            //{
            //    case MShare.TTimerCommand.tcSoftClose:
            //        CSocket.Socket.Close;
            //        while (true)
            //        {
            //            if (!CSocket.Socket.Connected)
            //            {
            //                CmdTimer.Interval = 100;
            //                ActiveCmdTimer(MShare.TTimerCommand.tcReSelConnect);
            //                break;
            //            }
            //            if (Application.Terminated)
            //            {
            //                break;
            //            }
            //            WaitAndPass(10);
            //        }
            //        break;
            //    case MShare.TTimerCommand.tcReSelConnect:
            //        ResetGameVariables();
            //        this.Active = false;
            //        while (true)
            //        {
            //            if (!CSocket.Socket.Connected)
            //            {
            //                DScreen.ChangeScene(SceneType.stSelectChr);
            //                if (!MShare.g_boDoFadeOut && !MShare.g_boDoFadeIn)
            //                {
            //                    MShare.g_boDoFadeIn = true;
            //                    MShare.g_nFadeIndex = 0;
            //                }
            //                MShare.g_ConnectionStep = TConnectionStep.cnsReSelChr;
            //                MShare.g_boQuerySelChar = true;
            //                CSocket.Address = MShare.g_sSelChrAddr;
            //                CSocket.Port = MShare.g_nSelChrPort;
            //                this.Active = true;
            //                break;
            //            }
            //            if (Application.Terminated)
            //            {
            //                break;
            //            }
            //            WaitAndPass(10);
            //        }
            //        break;
            //    case MShare.TTimerCommand.tcFastQueryChr:
            //        SendQueryChr();
            //        break;
            //}
        }

        private void CloseAllWindows()
        {
            //SaveWayPoint();
            MShare.g_gcAss[0] = false;
            MShare.AutoTagget = null;
            MShare.AutoPicupItem = null;
            MShare.g_nAPStatus = -1;
            MShare.TargetX = -1;
            MShare.g_boCanRunSafeZone = true;
            MShare.g_nEatIteminvTime = 200;
            MShare.g_SendSayListIdx = 0;
            MShare.g_SendSayList.Clear();
            if (MShare.g_nStallX != -1)
            {
                MShare.g_nStallX = -1;
            }
            MShare.g_boItemMoving = false;
        }

        private void ClearDropItems()
        {
            for (int i = 0; i < MShare.g_DropedItemList.Count; i++)
            {
                MShare.g_DropedItemList[i] = null;
            }
            MShare.g_DropedItemList.Clear();
        }

        private void ResetGameVariables()
        {
            CloseAllWindows();
            ClearDropItems();
            //if (MShare.g_RareBoxWindow != null)
            //{
            //    MShare.g_RareBoxWindow.Initialize();
            //}
            //for (i = Low(FrmDlg.m_MissionList); i <= High(FrmDlg.m_MissionList); i++)
            //{
            //    List = FrmDlg.m_MissionList[i];
            //    for (ii = 0; ii < List.Count; ii++)
            //    {
            //        this.Dispose((TClientMission)List[ii]);
            //    }
            //    List.Clear();
            //}
            //for (i = 0; i < MShare.g_MagicList.Count; i++)
            //{
            //    this.Dispose((TClientMagic)MShare.g_MagicList[i]);
            //}
            //MShare.g_MagicList.Clear();
            //for (i = 0; i < MShare.g_IPMagicList.Count; i++)
            //{
            //    this.Dispose((TClientMagic)MShare.g_IPMagicList[i]);
            //}
            //MShare.g_IPMagicList.Clear();
            //for (i = 0; i < MShare.g_HeroMagicList.Count; i++)
            //{
            //    this.Dispose((TClientMagic)MShare.g_HeroMagicList[i]);
            //}
            //MShare.g_HeroMagicList.Clear();
            //for (i = 0; i < MShare.g_HeroIPMagicList.Count; i++)
            //{
            //    this.Dispose((TClientMagic)MShare.g_HeroIPMagicList[i]);
            //}
            //MShare.g_HeroIPMagicList.Clear();
            //for (i = MShare.g_ShopListArr.GetLowerBound(0); i <= MShare.g_ShopListArr..Length; i++)
            //{
            //    List = MShare.g_ShopListArr[i];
            //    for (ii = 0; ii < List.Count; ii++)
            //    {
            //        this.Dispose((TShopItem)List[ii]);
            //    }
            //    List.Clear();
            //}
            MShare.g_boItemMoving = false;
            MShare.g_DetectItem.Item.Name = "";
            MShare.g_WaitingUseItem.Item.Item.Name = "";
            MShare.g_WaitingStallItem.Item.Item.Name = "";
            MShare.g_WaitingDetectItem.Item.Item.Name = "";
            MShare.OpenBoxItem.Item.Item.Name = "";
            MShare.g_EatingItem.Item.Name = "";
            MShare.LastMapMusic = -1;
            MShare.TargetX = -1;
            MShare.TargetCret = null;
            MShare.FocusCret = null;
            MShare.MagicTarget = null;
            ActionLock = false;
            MBoSupplyItem = false;
            MNEatRetIdx = -1;
            MShare.g_GroupMembers.Clear();
            MShare.g_sGuildRankName = "";
            MShare.g_sGuildName = "";
            MShare.g_boMapMoving = false;
            //WaitMsgTimer.Enabled = false;
            MShare.MapMovingWait = false;
            MShare.g_boNextTimePowerHit = false;
            MShare.g_boCanLongHit = false;
            MShare.g_boCanWideHit = false;
            MShare.g_boCanCrsHit = false;
            MShare.g_boCanSquHit = false;
            MShare.g_boNextTimeFireHit = false;
            MShare.g_boCanSLonHit = false;
            MShare.g_boNextTimeTwinHit = false;
            MShare.g_boNextTimePursueHit = false;
            MShare.g_boNextTimeSmiteHit = false;
            MShare.g_boNextTimeRushHit = false;
            MShare.g_boNextTimeSmiteLongHit = false;
            MShare.g_boNextTimeSmiteLongHit3 = false;
            MShare.g_boNextTimeSmiteLongHit2 = false;
            MShare.g_boNextTimeSmiteWideHit = false;
            MShare.g_boNextTimeSmiteWideHit2 = false;
            MShare.InitClientItems();
            MShare.g_DetectItemMineID = 0;
            PlayScene.ClearActors();
            ClearDropItems();
            //EventMan.ClearEvents();
            PlayScene.CleanObjects();
            //MaketSystem.Units.MaketSystem.g_Market.Clear();
        }

        private void ChangeServerClearGameVariables()
        {
            CloseAllWindows();
            ClearDropItems();
            MShare.g_boItemMoving = false;
            MShare.g_DetectItem.Item.Name = "";
            MShare.g_WaitingUseItem.Item.Item.Name = "";
            MShare.g_WaitingStallItem.Item.Item.Name = "";
            MShare.g_WaitingDetectItem.Item.Item.Name = "";
            MShare.OpenBoxItem.Item.Item.Name = "";
            MShare.g_EatingItem.Item.Name = "";
            MShare.LastMapMusic = -1;
            MShare.TargetX = -1;
            MShare.TargetCret = null;
            MShare.FocusCret = null;
            MShare.MagicTarget = null;
            ActionLock = false;
            MBoSupplyItem = false;
            MNEatRetIdx = -1;
            MShare.g_GroupMembers.Clear();
            MShare.g_sGuildRankName = "";
            MShare.g_sGuildName = "";
            MShare.g_boMapMoving = false;
            //WaitMsgTimer.Enabled = false;
            MShare.MapMovingWait = false;
            MShare.g_boNextTimePowerHit = false;
            MShare.g_boCanLongHit = false;
            MShare.g_boCanWideHit = false;
            MShare.g_boCanCrsHit = false;
            MShare.g_boCanSquHit = false;
            MShare.g_boNextTimeFireHit = false;
            MShare.g_boCanSLonHit = false;
            MShare.g_boNextTimeTwinHit = false;
            MShare.g_boNextTimePursueHit = false;
            MShare.g_boNextTimeSmiteHit = false;
            MShare.g_boNextTimeRushHit = false;
            MShare.g_boNextTimeSmiteLongHit = false;
            MShare.g_boNextTimeSmiteLongHit3 = false;
            MShare.g_boNextTimeSmiteLongHit2 = false;
            MShare.g_boNextTimeSmiteWideHit = false;
            MShare.g_boNextTimeSmiteWideHit2 = false;
            ClearDropItems();
            //EventMan.ClearEvents();
            PlayScene.CleanObjects();
        }

        private void SendSocket(string sendstr)
        {
            if (ClientSocket.IsConnected)
            {
                ClientSocket.SendText($"#1{sendstr}!");
            }
            else
            {
                MainOutMessage($"socket close {ClientSocket.RemoteEndPoint}");
            }
        }

        public void CloseSocket()
        {
            if (ClientSocket != null)
            {
                ClientSocket.Disconnect();
            }
        }

        private void SendClientMessage(int msg, int recog, int param, int tag, int series)
        {
            CommandMessage dMsg = Messages.MakeMessage(msg, recog, param, tag, series);
            SendSocket(EDCode.EncodeMessage(dMsg));
        }

        /// <summary>
        /// 发送登录消息
        /// </summary>
        private void SendRunLogin()
        {
            MainOutMessage("进入游戏");
            DScreen.CurrentScene.ConnectionStep = ConnectionStep.Play;
            string sSendMsg = $"**{LoginId}/{ChrName}/{Certification}/{Grobal2.ClientVersionNumber}/{Grobal2.ClientVersionNumber}";
            SendSocket(EDCode.EncodeString(sSendMsg));
        }

        public void SendSay(string str)
        {
            string sx = string.Empty;
            const string sam = "/move";
            if (!string.IsNullOrEmpty(str))
            {
                if (HUtil32.CompareLStr(str, sam))
                {
                    string param = str[sam.Length..];
                    if (!string.IsNullOrEmpty(param))
                    {
                        var sy = HUtil32.GetValidStr3(param, ref sx, new string[] { " ", ":", ",", "\t" });
                        if ((sx != "") && (sy != ""))
                        {
                            short x = Convert.ToInt16(sx);
                            short y = Convert.ToInt16(sy);
                            if ((x > 0) && (y > 0))
                            {
                                MShare.MySelf.m_nTagX = x;
                                MShare.MySelf.m_nTagY = y;
                                if (!GPathBusy)
                                {
                                    GPathBusy = true;
                                    try
                                    {
                                        Map.LoadMapData();
                                        PathMap.g_MapPath = Map.FindPath(MShare.MySelf.CurrX, MShare.MySelf.CurrY, MShare.MySelf.m_nTagX, MShare.MySelf.m_nTagY, 0);
                                        if (PathMap.g_MapPath != null)
                                        {
                                            GMoveStep = 1;
                                            //TimerAutoMove.Enabled = true;
                                            ScreenManager.AddChatBoardString($"自动移动至坐标({MShare.MySelf.m_nTagX}:{MShare.MySelf.m_nTagY})，点击鼠标任意键停止……");
                                        }
                                        else
                                        {
                                            //TimerAutoMove.Enabled = false;
                                            ScreenManager.AddChatBoardString($"自动移动坐标点({MShare.MySelf.m_nTagX}:{MShare.MySelf.m_nTagY})不可到达");
                                            MShare.MySelf.m_nTagX = 0;
                                            MShare.MySelf.m_nTagY = 0;
                                        }
                                    }
                                    finally
                                    {
                                        GPathBusy = false;
                                    }
                                }
                            }
                        }
                    }
                    return;
                }
                CommandMessage msg = Messages.MakeMessage(Messages.CM_SAY, 0, 0, 0, 0);
                SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(str));
                if (str[0] == '/')
                {
                    ScreenManager.AddChatBoardString(str);
                    HUtil32.GetValidStr3(str[1..], ref WhisperName, " ");
                }
            }
        }

        /// <summary>
        /// 发送角色动作消息（走路 攻击等）
        /// </summary>
        private void SendActMsg(int ident, ushort x, ushort y, int dir)
        {
            CommandMessage msg = Messages.MakeMessage(ident, HUtil32.MakeLong(x, y), 0, dir, 0);
            SendSocket(EDCode.EncodeMessage(msg));
            ActionLock = true;
            _actionLockTime = MShare.GetTickCount();
        }

        private void SendSpellMsg(int ident, ushort x, ushort y, int dir, int target, bool bLock = false)
        {
            CommandMessage msg = Messages.MakeMessage(ident, HUtil32.MakeLong(x, y), HUtil32.LoWord(target), dir, HUtil32.HiWord(target));
            SendSocket(EDCode.EncodeMessage(msg));
            if (!bLock)
            {
                return;
            }
            ActionLock = true;
            _actionLockTime = MShare.GetTickCount();
        }

        public void SendQueryUserName(int targetid, int x, int y)
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_QUERYUSERNAME, targetid, x, y, 0);
            SendSocket(EDCode.EncodeMessage(msg));
        }

        public void SendDropItem(string name, int itemserverindex, int dropcnt)
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_DROPITEM, itemserverindex, dropcnt, 0, 0);
            SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(name));
        }

        private void SendPickup()
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_PICKUP, 0, MShare.MySelf.CurrX, MShare.MySelf.CurrY, 0);
            SendSocket(EDCode.EncodeMessage(msg));
        }

        private void SendTakeOnItem(int where, int itmindex, string itmname)
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_TAKEONITEM, itmindex, where, 0, 0);
            SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(itmname));
        }

        public void SendTakeOffItem(byte where, int itmindex, string itmname)
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_TAKEOFFITEM, itmindex, where, 0, 0);
            SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(itmname));
        }

        private void SendEat(int itmindex, string itmname, int nUnBindItem)
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_EAT, itmindex, 0, 0, nUnBindItem);
            SendSocket(EDCode.EncodeMessage(msg));
        }

        private void SendButchAnimal(int x, int y, int dir, int actorid)
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_BUTCH, actorid, x, y, dir);
            SendSocket(EDCode.EncodeMessage(msg));
        }

        public void SendMagicKeyChange(int magid, char keych)
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_MAGICKEYCHANGE, magid, (byte)keych, 0, 0);
            SendSocket(EDCode.EncodeMessage(msg));
        }

        private void SendMerchantDlgSelect(int merchant, string rstr)
        {
            const string sam = "@_automove ";
            int x;
            int y;
            string sx = string.Empty;
            string sy = string.Empty;
            if (rstr.Length >= 2)
            {
                if (HUtil32.CompareLStr(rstr, sam))
                {
                    string param = rstr.Substring(sam.Length + 1 - 1, rstr.Length - sam.Length);
                    if (param != "")
                    {
                        param = HUtil32.GetValidStr3(param, ref sx, new string[] { " ", ":", ",", "\t" });
                        var sM = HUtil32.GetValidStr3(param, ref sy, new string[] { " ", ":", ",", "\t" });

                        if ((sx != "") && (sy != ""))
                        {
                            if ((sM != "") && (string.Compare(MShare.MapTitle, sM, StringComparison.OrdinalIgnoreCase) != 0))// 自动移动
                            {
                                ScreenManager.AddChatBoardString($"到达 {sM} 之后才能使用自动走路");
                                return;
                            }
                            x = Convert.ToInt32(sx);
                            y = Convert.ToInt32(sy);
                            if ((x > 0) && (y > 0))
                            {
                                MShare.MySelf.m_nTagX = (short)x;
                                MShare.MySelf.m_nTagY = (short)y;
                                if (!GPathBusy)
                                {
                                    GPathBusy = true;
                                    try
                                    {
                                        Map.LoadMapData();
                                        PathMap.g_MapPath = Map.FindPath(MShare.MySelf.CurrX, MShare.MySelf.CurrY, MShare.MySelf.m_nTagX, MShare.MySelf.m_nTagY, 0);
                                        if (PathMap.g_MapPath != null)
                                        {
                                            GMoveStep = 1;
                                            TimerAutoMove.Enabled = true;
                                            ScreenManager.AddChatBoardString($"自动移动至坐标({MShare.MySelf.m_nTagX}:{MShare.MySelf.m_nTagY})，点击鼠标任意键停止……");
                                        }
                                        else
                                        {
                                            TimerAutoMove.Enabled = false;
                                            ScreenManager.AddChatBoardString($"自动移动坐标点({MShare.MySelf.m_nTagX}:{MShare.MySelf.m_nTagY})不可到达");
                                            MShare.MySelf.m_nTagX = 0;
                                            MShare.MySelf.m_nTagY = 0;
                                        }
                                    }
                                    finally
                                    {
                                        GPathBusy = false;
                                    }
                                }
                            }
                        }
                    }
                    return;
                }
            }
            CommandMessage msg = Messages.MakeMessage(Messages.CM_MERCHANTDLGSELECT, merchant, 0, 0, 0);
            SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(rstr));
        }

        public void SendQueryPrice(int merchant, int itemindex, string itemname)
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_MERCHANTQUERYSELLPRICE, merchant, HUtil32.LoWord(itemindex), HUtil32.HiWord(itemindex), 0);
            SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(itemname));
        }

        public void SendQueryRepairCost(int merchant, int itemindex, string itemname)
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_MERCHANTQUERYREPAIRCOST, merchant, HUtil32.LoWord(itemindex), HUtil32.HiWord(itemindex), 0);
            SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(itemname));
        }

        public void SendSellItem(int merchant, int itemindex, string itemname, short count)
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_USERSELLITEM, merchant, HUtil32.LoWord(itemindex), HUtil32.HiWord(itemindex), count);
            SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(itemname));
        }

        public void SendRepairItem(int merchant, int itemindex, string itemname)
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_USERREPAIRITEM, merchant, HUtil32.LoWord(itemindex), HUtil32.HiWord(itemindex), 0);
            SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(itemname));
        }

        public void SendStorageItem(int merchant, int itemindex, string itemname, short count)
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_USERSTORAGEITEM, merchant, HUtil32.LoWord(itemindex), HUtil32.HiWord(itemindex), count);
            SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(itemname));
        }

        public void SendGetDetailItem(int merchant, int menuindex, string itemname)
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_USERGETDETAILITEM, merchant, menuindex, 0, 0);
            SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(itemname));
        }

        public void SendBuyItem(int merchant, int itemserverindex, string itemname, short conut)
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_USERBUYITEM, merchant, HUtil32.LoWord(itemserverindex), HUtil32.HiWord(itemserverindex), conut);
            SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(itemname));
        }

        public void SendTakeBackStorageItem(int merchant, int itemserverindex, string itemname, short count)
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_USERTAKEBACKSTORAGEITEM, merchant, HUtil32.LoWord(itemserverindex), HUtil32.HiWord(itemserverindex), count);
            SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(itemname));
        }

        public void SendMakeDrugItem(int merchant, string itemname)
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_USERMAKEDRUGITEM, merchant, 0, 0, 0);
            SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(itemname));
        }

        public void SendDropGold(int dropgold)
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_DROPGOLD, dropgold, 0, 0, 0);
            SendSocket(EDCode.EncodeMessage(msg));
        }

        public void SendGroupMode(bool onoff)
        {
            CommandMessage msg;
            if (onoff)
            {
                msg = Messages.MakeMessage(Messages.CM_GROUPMODE, 0, 1, 0, 0);
            }
            else
            {
                msg = Messages.MakeMessage(Messages.CM_GROUPMODE, 0, 0, 0, 0);
            }
            SendSocket(EDCode.EncodeMessage(msg));
        }

        public void SendCreateGroup(string withwho)
        {
            if (withwho != "")
            {
                CommandMessage msg = Messages.MakeMessage(Messages.CM_CREATEGROUP, 0, 0, 0, 0);
                SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(withwho));
            }
        }

        public void SendWantMiniMap()
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_WANTMINIMAP, 0, 0, 0, 0);
            SendSocket(EDCode.EncodeMessage(msg));
        }

        public void SendGuildDlg()
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_OPENGUILDDLG, 0, 0, 0, 0);
            SendSocket(EDCode.EncodeMessage(msg));
        }

        public void SendDealTry()
        {
            string who = string.Empty;
            CommandMessage msg = Messages.MakeMessage(Messages.CM_DEALTRY, 0, 0, 0, 0);
            SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(who));
        }

        public void SendCancelDeal()
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_DEALCANCEL, 0, 0, 0, 0);
            SendSocket(EDCode.EncodeMessage(msg));
        }

        public void SendAddDealItem(ClientItem ci)
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_DEALADDITEM, ci.MakeIndex, 0, 0, ci.Dura);
            SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(ci.Item.Name));
        }

        public void SendDelDealItem(ClientItem ci)
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_DEALDELITEM, ci.MakeIndex, 0, 0, ci.Dura);
            SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(ci.Item.Name));
        }

        public void SendChangeDealGold(int gold)
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_DEALCHGGOLD, gold, 0, 0, 0);
            SendSocket(EDCode.EncodeMessage(msg));
        }

        public void SendDealEnd()
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_DEALEND, 0, 0, 0, 0);
            SendSocket(EDCode.EncodeMessage(msg));
        }

        public void SendAddGroupMember(string withwho)
        {
            if (withwho != "")
            {
                CommandMessage msg = Messages.MakeMessage(Messages.CM_ADDGROUPMEMBER, 0, 0, 0, 0);
                SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(withwho));
            }
        }

        public void SendDelGroupMember(string withwho)
        {
            if (withwho != "")
            {
                CommandMessage msg = Messages.MakeMessage(Messages.CM_DELGROUPMEMBER, 0, 0, 0, 0);
                SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(withwho));
            }
        }

        public void SendGuildHome()
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_GUILDHOME, 0, 0, 0, 0);
            SendSocket(EDCode.EncodeMessage(msg));
        }

        private void SendGuildMemberList()
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_GUILDMEMBERLIST, 0, 0, 0, 0);
            SendSocket(EDCode.EncodeMessage(msg));
        }

        public void SendGuildAddMem(string who)
        {
            if (who.Trim() != "")
            {
                CommandMessage msg = Messages.MakeMessage(Messages.CM_GUILDADDMEMBER, 0, 0, 0, 0);
                SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(who));
            }
        }

        public void SendGuildDelMem(string who)
        {
            if (who.Trim() != "")
            {
                CommandMessage msg = Messages.MakeMessage(Messages.CM_GUILDDELMEMBER, 0, 0, 0, 0);
                SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(who));
            }
        }

        public void SendGuildUpdateNotice(string notices)
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_GUILDUPDATENOTICE, 0, 0, 0, 0);
            SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(notices));
        }

        public void SendGuildUpdateGrade(string rankinfo)
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_GUILDUPDATERANKINFO, 0, 0, 0, 0);
            SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeString(rankinfo));
        }

        public void SendSpeedHackUser()
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_SPEEDHACKUSER, 0, 0, 0, 0);
            SendSocket(EDCode.EncodeMessage(msg));
        }

        public void SendAdjustBonus(int remain, NakedAbility babil)
        {
            CommandMessage msg = Messages.MakeMessage(Messages.CM_ADJUST_BONUS, remain, 0, 0, 0);
            SendSocket(EDCode.EncodeMessage(msg) + EDCode.EncodeMessage(babil));
        }

        public bool ServerAcceptNextAction()
        {
            if ((MShare.MySelf != null) && MShare.MySelf.StallMgr.OnSale)
            {
                return false;
            }
            bool result = true;
            if (ActionLock)
            {
                if ((MShare.GetTickCount() - _actionLockTime) > 5 * 1000)
                {
                    ActionLock = false;
                }
                result = false;
            }
            return result;
        }

        public bool CanNextAction()
        {
            if ((MShare.MySelf != null) && MShare.MySelf.StallMgr.OnSale)
            {
                return false;
            }
            if (!MShare.MySelf.m_boUseCboLib && MShare.MySelf.IsIdle() && ((MShare.MySelf.m_nState & 0x04000000) == 0) && ((MShare.MySelf.m_nState & 0x02000000) == 0)
                && (MShare.GetTickCount() - MShare.g_dwDizzyDelayStart > MShare.g_dwDizzyDelayTime))
            {
                return true;
            }
            return false;
        }

        /// <summary>
        /// 是否可以攻击，控制攻击速度
        /// </summary>
        /// <returns></returns>
        private bool CanNextHit(bool settime = false)
        {
            bool result;
            int nextHitTime;
            if ((MShare.MySelf != null) && MShare.MySelf.StallMgr.OnSale)
            {
                return false;
            }
            int levelFastTime = HUtil32._MIN(370, MShare.MySelf.Abil.Level * 14);
            levelFastTime = HUtil32._MIN(800, levelFastTime + MShare.MySelf.HitSpeed * MShare.ItemSpeed);
            if (MShare.SpeedRate)
            {
                if (MShare.MySelf.m_boAttackSlow)
                {
                    nextHitTime = MShare.HitTime - levelFastTime + 1500 - MShare.g_HitSpeedRate * 20; // 腕力超过时，减慢攻击速度
                }
                else
                {
                    nextHitTime = MShare.HitTime - levelFastTime - MShare.g_HitSpeedRate * 20;
                }
            }
            else
            {
                if (MShare.MySelf.m_boAttackSlow)
                {
                    nextHitTime = MShare.HitTime - levelFastTime + 1500;
                }
                else
                {
                    nextHitTime = MShare.HitTime - levelFastTime;
                }
            }
            if (nextHitTime < 0)
            {
                nextHitTime = 0;
            }
            if (MShare.GetTickCount() - LastHitTick > nextHitTime)
            {
                if (settime)
                {
                    LastHitTick = MShare.GetTickCount();
                }
                result = true;
            }
            else
            {
                result = false;
            }
            return result;
        }

        private void ActionFailed()
        {
            MShare.TargetX = -1;
            MShare.TargetY = -1;
            MShare.MySelf.m_boUseCboLib = false;
            ActionFailLock = true;
            ActionFailLockTime = MShare.GetTickCount();
            MShare.MySelf.MoveFail();
        }

        private bool IsUnLockAction()
        {
            bool result;
            if (ActionFailLock)
            {
                if ((MShare.GetTickCount() - ActionFailLockTime) >= 1000)// 如果操作被锁定，则在指定时间后解锁
                {
                    ActionFailLock = false;
                }
            }
            if (ActionFailLock || MShare.g_boMapMoving || MShare.g_boServerChanging)
            {
                result = false;
            }
            else
            {
                result = true;
            }
            return result;
        }

        private static bool IsGroupMember(string uname)
        {
            return MShare.g_GroupMembers.IndexOf(uname) >= 0; ;
        }

        private void CheckSpeedHack(long rtime)
        {
            return;
        }

        private void RecalcAutoMovePath()
        {
            if ((MShare.MySelf.m_nTagX > 0) && (MShare.MySelf.m_nTagY > 0))
            {
                if (!GPathBusy)
                {
                    GPathBusy = true;
                    try
                    {
                        Map.ReLoadMapData();
                        PathMap.g_MapPath = Map.FindPath(MShare.MySelf.CurrX, MShare.MySelf.CurrY, MShare.MySelf.m_nTagX, MShare.MySelf.m_nTagY, 0);
                        if (PathMap.g_MapPath != null)
                        {
                            GMoveStep = 1;
                            TimerAutoMove.Enabled = true;
                        }
                        else
                        {
                            MShare.MySelf.m_nTagX = 0;
                            MShare.MySelf.m_nTagY = 0;
                            TimerAutoMove.Enabled = false;
                            ScreenManager.AddChatBoardString($"自动移动目标({MShare.MySelf.m_nTagX}:{MShare.MySelf.m_nTagY})被占据，不可到达");
                        }
                    }
                    finally
                    {
                        GPathBusy = false;
                    }
                }
            }
        }

        public string DecodeMessagePacket_ExtractUserName(string line)
        {
            string uname = string.Empty;
            HUtil32.GetValidStr3(line, ref line, new string[] { "(", "!", "*", "/", ")" });
            HUtil32.GetValidStr3(line, ref uname, new string[] { " ", "=", ":" });
            if (uname != "")
            {
                if ((uname[0] == '/') || (uname[0] == '(') || (uname[0] == ' ') || (uname[0] == '['))
                {
                    uname = "";
                }
            }
            return uname;
        }

        private void ClientGetNeedUpdateAccount(string body)
        {
            //TUserEntry ue = EDcode.DecodeBuffer<TUserEntry>(body);
            //LoginScene.UpdateAccountInfos(ue);
        }

        private void ClientGetMapDescription(CommandMessage msg, string sBody)
        {
            sBody = EDCode.DeCodeString(sBody);
            string sTitle = sBody;
            MShare.MapTitle = sTitle;
            LoadWayPoint();
            if (!MShare.g_gcGeneral[11])
            {
                MShare.LastMapMusic = msg.Recog;
            }
            else
            {
                if (msg.Recog == -1)
                {
                    MShare.LastMapMusic = -1;
                }
                if (MShare.LastMapMusic != msg.Recog)
                {
                    MShare.LastMapMusic = msg.Recog;
                }
            }
        }

        private void ClientGetGameGoldName(CommandMessage msg, string sBody)
        {
            //var sData = string.Empty;
            //if (sBody != "")
            //{
            //    sBody = EDCode.DeCodeString(sBody);
            //    sBody = HUtil32.GetValidStr3(sBody, ref sData, new char[] { '\r' });
            //    //BotConst.GameGoldName = sData;
            //    //BotConst.GamePointName = sBody;
            //}
            MShare.MySelf.m_nGameGold = msg.Recog;
            MShare.MySelf.m_nGamePoint = HUtil32.MakeLong(msg.Param, msg.Tag);
        }

        private void ClientGetAdjustBonus(int bonus, string body)
        {
            //string str1;
            //string Str2;
            //string str3;
            //MShare.g_nBonusPoint = bonus;
            //body = HUtil32.GetValidStr3(body, ref str1, HUtil32.Backslash);
            //str3 = HUtil32.GetValidStr3(body, ref Str2, HUtil32.Backslash);
            //EDcode.DecodeBuffer(str1, MShare.g_BonusTick);
            //EDcode.DecodeBuffer(Str2, MShare.g_BonusAbil);
            //EDcode.DecodeBuffer(str3, MShare.g_NakedAbil);
        }

        private void ClientGetAddItem(int hint, string body)
        {
            if (!string.IsNullOrEmpty(body))
            {
                ClientItem cu = MShare.g_boBagEquipSequence ? ClientItemCodec.Decode(body) : EDCode.DecodeBuffer<ClientItem>(body);
                BotHelper.AddItemBag(cu);
                if (hint != 0)
                {
                    DScreen.AddSysMsg(cu.Item.Name + " 被发现");
                }
            }
        }

        private void ClientGetUpdateItem(string body)
        {
            if (!string.IsNullOrEmpty(body))
            {
                ClientItem cu = MShare.g_boBagEquipSequence ? ClientItemCodec.Decode(body) : EDCode.DecodeBuffer<ClientItem>(body);
                BotHelper.UpdateItemBag(cu);
                for (int i = 0; i < MShare.UseItems.Length; i++)
                {
                    if ((MShare.UseItems[i].Item.Name == cu.Item.Name) && (MShare.UseItems[i].MakeIndex == cu.MakeIndex))
                    {
                        MShare.UseItems[i] = cu;
                    }
                }
                if ((MShare.g_SellDlgItem.Item.Name != "") && (MShare.g_SellDlgItem.MakeIndex == cu.MakeIndex))
                {
                    MShare.g_SellDlgItem = cu;
                }
                for (int i = 0; i < 1; i++)
                {
                    if ((MShare.g_TIItems[i].Item.MakeIndex == cu.MakeIndex) && (MShare.g_TIItems[i].Item.Item.Name != ""))
                    {
                        MShare.g_TIItems[i].Item = cu;
                        if (i == 0)
                        {
                            MShare.GetTIHintString1(1, MShare.g_TIItems[0].Item);
                        }
                    }
                }
                MShare.AutoPutOntiBooks();
                for (int i = 0; i < 1; i++)
                {
                    if ((MShare.g_spItems[i].Item.MakeIndex == cu.MakeIndex) && (MShare.g_spItems[i].Item.Item.Name != ""))
                    {
                        MShare.g_spItems[i].Item = cu;
                    }
                }
            }
        }

        private void ClientGetDelItem(string body)
        {
            if (!string.IsNullOrEmpty(body))
            {
                ClientItem cu = MShare.g_boBagEquipSequence ? ClientItemCodec.Decode(body) : EDCode.DecodeBuffer<ClientItem>(body);
                BotHelper.DelItemBag(cu.Item.Name, cu.MakeIndex);
                for (int i = 0; i < MShare.UseItems.Length; i++)
                {
                    if ((MShare.UseItems[i].Item.Name == cu.Item.Name) && (MShare.UseItems[i].MakeIndex == cu.MakeIndex))
                    {
                        MShare.UseItems[i].Item.Name = "";
                    }
                }
                for (int i = 0; i < 1; i++)
                {
                    if (MShare.g_TIItems[i].Item.MakeIndex == cu.MakeIndex)
                    {
                        MShare.g_TIItems[i].Item.Item.Name = "";
                        if (i == 0)
                        {
                            MShare.GetTIHintString1(0);
                        }
                    }
                }
                for (int i = 0; i < 1; i++)
                {
                    if (MShare.g_spItems[i].Item.MakeIndex == cu.MakeIndex)
                    {
                        MShare.g_spItems[i].Item.Item.Name = "";
                    }
                }
            }
        }

        private void ClientGetDelItems(string body, ushort wOnlyBag)
        {
            int iindex;
            string str = string.Empty;
            string iname = string.Empty;
            ClientItem cu = new ClientItem();
            body = EDCode.DeCodeString(body);
            while (!string.IsNullOrEmpty(body))
            {
                body = HUtil32.GetValidStr3(body, ref str, HUtil32.Backslash);
                if ((!string.IsNullOrEmpty(iname)) && (!string.IsNullOrEmpty(str)))
                {
                    iindex = HUtil32.StrToInt(str, 0);
                    BotHelper.DelItemBag(iname, iindex);
                    if (wOnlyBag == 0)
                    {
                        for (int i = 0; i < MShare.UseItems.Length; i++)
                        {
                            if ((MShare.UseItems[i].Item.Name == iname) && (MShare.UseItems[i].MakeIndex == iindex))
                            {
                                MShare.UseItems[i].Item.Name = "";
                                break;
                            }
                        }
                    }
                    for (int i = 0; i < 1; i++)
                    {
                        if (MShare.g_TIItems[i].Item.MakeIndex == cu.MakeIndex)
                        {
                            MShare.g_TIItems[i].Item.Item.Name = "";
                            if (i == 0)
                            {
                                MShare.GetTIHintString1(0);
                            }
                        }
                    }
                    for (int i = 0; i < 1; i++)
                    {
                        if (MShare.g_spItems[i].Item.MakeIndex == cu.MakeIndex)
                        {
                            MShare.g_spItems[i].Item.Item.Name = "";
                        }
                    }
                }
                else
                {
                    break;
                }
            }
        }

        public bool ClientGetBagItmes_CompareItemArr(ClientItem[] itemSaveArr)
        {
            bool flag = true;
            for (int i = 0; i < BotConst.MaxBagItemcl; i++)
            {
                if (itemSaveArr[i].Item.Name != "")
                {
                    flag = false;
                    for (int j = 0; j < BotConst.MaxBagItemcl; j++)
                    {
                        if ((MShare.ItemArr[j].Item.Name == itemSaveArr[i].Item.Name) && (MShare.ItemArr[j].MakeIndex == itemSaveArr[i].MakeIndex))
                        {
                            if ((MShare.ItemArr[j].Dura == itemSaveArr[i].Dura) && (MShare.ItemArr[j].DuraMax == itemSaveArr[i].DuraMax))
                            {
                                flag = true;
                            }
                            break;
                        }
                    }
                    if (!flag)
                    {
                        break;
                    }
                }
            }
            if (flag)
            {
                for (int i = 0; i < BotConst.MaxBagItemcl; i++)
                {
                    if (MShare.ItemArr[i].Item.Name != "")
                    {
                        flag = false;
                        for (int j = 0; j < BotConst.MaxBagItemcl; j++)
                        {
                            if ((MShare.ItemArr[i].Item.Name == itemSaveArr[j].Item.Name) && (MShare.ItemArr[i].MakeIndex == itemSaveArr[j].MakeIndex))
                            {
                                if ((MShare.ItemArr[i].Dura == itemSaveArr[j].Dura) && (MShare.ItemArr[i].DuraMax == itemSaveArr[j].DuraMax))
                                {
                                    flag = true;
                                }
                                break;
                            }
                        }
                        if (!flag)
                        {
                            break;
                        }
                    }
                }
            }
            bool result = flag;
            return result;
        }

        private void ClientGetBagItmes(string body)
        {
            string Str = string.Empty;
            // M3 T-14 诊断：-BagEquipSequence 时把收到的包体长度与装载后的背包视图规模落日志，
            // 否则「序列全 skip」既可能是服务端没推物品、也可能是假人视图没装载，无法判红。
            int bagBodyLen = string.IsNullOrEmpty(body) ? 0 : body.Length;
            //var ItemSaveArr = new ClientItem[BotConst.MaxBagItemcl];
            //FillChar(MShare.g_RefineItems, sizeof(TMovingItem) * 3, '\0');
            //FillChar(MShare.g_BuildAcuses, sizeof(MShare.g_BuildAcuses), '\0');
            //FillChar(MShare.g_ItemArr * BotConst.MaxBagItemCL, '\0');
            //FillChar(MShare.g_TIItems, sizeof(MShare.g_TIItems), '\0');
            //FillChar(MShare.g_spItems, sizeof(MShare.g_spItems), '\0');
            if (MShare.MovingItem != null)
            {
                if ((MShare.MovingItem.Item.Item.Name != "") && MShare.IsBagItem(MShare.MovingItem.Index))
                {
                    MShare.MovingItem.Item.Item.Name = "";
                    MShare.g_boItemMoving = false;
                }
            }
            while (true)
            {
                if (string.IsNullOrEmpty(body))
                {
                    break;
                }
                body = HUtil32.GetValidStr3(body, ref Str, HUtil32.Backslash);
                try
                {
                    ClientItem cu = MShare.g_boBagEquipSequence ? ClientItemCodec.Decode(Str) : EDCode.DecodeClientBuffer<ClientItem>(Str);
                    BotHelper.AddItemBag(cu);
                }
                catch (Exception ex)
                {
                    MainOutMessage($"M3BAGSEQ bag item decode ERR: {ex}");
                }
            }
            //ClFunc.Loadbagsdat(".\\Config\\" + MShare.g_sServerName + "." + m_sChrName + ".itm-plus", ItemSaveArr);
            //if (ClientGetBagItmes_CompareItemArr())
            //{
            //    Move(ItemSaveArr, MShare.g_ItemArr * BotConst.MaxBagItemCL);
            //}
            BotHelper.ArrangeItembag();
            MShare.g_boBagLoaded = true;
            if (MShare.g_boBagEquipSequence)
            {
                int nBagView = 0;
                for (int vi = 0; vi < BotConst.MaxBagItemcl; vi++)
                {
                    if (MShare.ItemArr[vi] != null && !string.IsNullOrEmpty(MShare.ItemArr[vi].Item.Name))
                    {
                        nBagView++;
                    }
                }
                MainOutMessage($"M3BAGSEQ SM_BAGITEMS body_len={bagBodyLen} bag_view={nBagView}");
            }
            if (MShare.MySelf != null)
            {
                if (!MShare.MySelf.StallMgr.OnSale)
                {
                    for (int i = 0; i < 9; i++)
                    {
                        if (MShare.MySelf.StallMgr.mBlock.Items[i] == null)
                        {
                            continue;
                        }
                        if (MShare.MySelf.StallMgr.mBlock.Items[i].Item.Name != "")
                        {
                            BotHelper.UpdateBagStallItem(MShare.MySelf.StallMgr.mBlock.Items[i], 4);
                        }
                    }
                }
                else
                {
                    for (int i = 0; i < 9; i++)
                    {
                        if (MShare.MySelf.StallMgr.mBlock.Items[i] == null)
                        {
                            continue;
                        }
                        if (MShare.MySelf.StallMgr.mBlock.Items[i].Item.Name != "")
                        {
                            BotHelper.UpdateBagStallItem(MShare.MySelf.StallMgr.mBlock.Items[i], 5);
                        }
                    }
                }
            }
            if (MShare.OpenAutoPlay && (MShare.g_nAPReLogon == 4))
            {
                MShare.g_nAPReLogon = 0;
                MShare.g_nOverAPZone = MShare.g_nOverAPZone2;
                MShare.g_APGoBack = MShare.g_APGoBack2;
                if (MShare.g_APMapPath2 != null)
                {
                    MShare.MapPath = new Point[MShare.g_APMapPath2.Length + 1];
                    for (int k = 0; k <= MShare.g_APMapPath2.Length; k++)
                    {
                        MShare.MapPath[k] = MShare.g_APMapPath2[k];
                    }
                }
                MShare.AutoLastPoint = MShare.AutoLastPoint2;
                MShare.AutoStep = MShare.g_APStep2;
                MShare.g_gcAss[0] = true;
                MShare.AutoTagget = null;
                MShare.AutoPicupItem = null;
                MShare.g_nAPStatus = -1;
                MShare.TargetX = -1;
                MShare.g_APGoBack2 = false;
                MShare.g_APMapPath2 = null;
                GetNearPoint();
                TimerAutoPlay.Enabled = MShare.g_gcAss[0];
                ScreenManager.AddChatBoardString("开始自动挂机...");
                SaveWayPoint();
            }
        }

        private void ClientGetDropItemFail(string iname, int sindex)
        {
            ClientItem pc = BotHelper.GetDropItem(iname, sindex);
            if (pc != null)
            {
                BotHelper.AddItemBag(pc);
                BotHelper.DelDropItem(iname, sindex);
            }
        }

        private void ClientGetShowItem(int itemid, short x, short y, int looks, string itmname)
        {
            for (int i = 0; i < MShare.g_DropedItemList.Count; i++)
            {
                if (MShare.g_DropedItemList[i].id == itemid)
                {
                    return;
                }
            }
            TDropItem dropItem = new TDropItem();
            dropItem.id = itemid;
            dropItem.X = x;
            dropItem.Y = y;
            dropItem.looks = looks;
            dropItem.Name = itmname;
            dropItem.Width = itmname.Length;
            dropItem.Height = itmname.Length;
            HUtil32.GetValidStr3(dropItem.Name, ref itmname, "\\");
            dropItem.FlashTime = MShare.GetTickCount() - RandomNumber.GetInstance().Random(3000);
            dropItem.BoFlash = false;
            dropItem.boNonSuch = false;
            dropItem.boShowName = BotConst.g_ShowItemList.ContainsKey(itmname);
            dropItem.boPickUp = dropItem.boShowName;
            if (MShare.g_gcAss[5])
            {
                dropItem.boNonSuch = false;
                dropItem.boPickUp = false;
                dropItem.boShowName = false;
                //i = MShare.g_APPickUpList.IndexOf(itmname);
                //if (i >= 0)
                //{
                //    DropItem.boNonSuch = ((int)MShare.g_APPickUpList.Values[i]) != 0;
                //    DropItem.boPickUp = true;
                //    if (!DropItem.boNonSuch)
                //    {
                //        DropItem.boShowName = true;
                //    }
                //}
            }
            else
            {
                //P = (TCItemRule)MShare.g_ItemsFilter_All.GetValues(itmname);
                //if (P != null)
                //{
                //    DropItem.boNonSuch = P.rare;
                //    DropItem.boPickUp = P.pick;
                //    DropItem.boShowName = P.Show;
                //}
            }
            MShare.g_DropedItemList.Add(dropItem);
        }

        private void ClientGetHideItem(int itemid, int x, int y)
        {
            TDropItem dropItem;
            for (int i = 0; i < MShare.g_DropedItemList.Count; i++)
            {
                dropItem = MShare.g_DropedItemList[i];
                if (dropItem.id == itemid)
                {
                    MShare.g_DropedItemList.RemoveAt(i);
                    break;
                }
            }
        }

        private void ClientGetSendUseItems(string body)
        {
            int index;
            string str = string.Empty;
            string data = string.Empty;
            while (true)
            {
                if (string.IsNullOrEmpty(body))
                {
                    break;
                }
                body = HUtil32.GetValidStr3(body, ref str, HUtil32.Backslash);
                body = HUtil32.GetValidStr3(body, ref data, HUtil32.Backslash);
                index = HUtil32.StrToInt(str, -1);
                if (index >= 0 && index <= 13)
                {
                    ClientItem cu = EDCode.DecodeBuffer<ClientItem>(data);
                    MShare.UseItems[index] = cu;
                }
            }
        }

        public int ClientGetAddMagic_ListSortCompareLevel(object item1, object item2)
        {
            int result = 1;
            if (((ClientMagic)item1).Def.TrainLevel[0] < ((ClientMagic)item2).Def.TrainLevel[0])
            {
                result = -1;
            }
            else if (((ClientMagic)item1).Def.TrainLevel[0] == ((ClientMagic)item2).Def.TrainLevel[0])
            {
                result = 0;
            }
            return result;
        }

        private void ClientGetAddMagic(string body)
        {
            ClientMagic pcm = EDCode.DecodeBuffer<ClientMagic>(body);
            MShare.MagicArr[pcm.Def.MagicId] = pcm;
            MShare.g_MagicList.Add(pcm);
            for (int i = 0; i < MShare.g_MagicList.Count; i++)
            {
                if (MShare.g_MagicList[i].Def.MagicId == 67)
                {
                    //MShare.g_MagicList.Move(i, 0);
                    break;
                }
            }
        }

        private void ClientGetDelMagic(int magid, int btclass)
        {
            for (int i = MShare.g_MagicList.Count - 1; i >= 0; i--)
            {
                if (MShare.g_MagicList[i].Def.MagicId == magid)
                {
                    MShare.g_MagicList[i] = null;
                    MShare.g_MagicList.RemoveAt(i);
                    break;
                }
            }
            MShare.MagicArr[magid] = null;
        }

        public int ClientConvertMagic_ListSortCompareLevel(object item1, object item2)
        {
            int result = 1;
            if (((ClientMagic)item1).Def.TrainLevel[0] < ((ClientMagic)item2).Def.TrainLevel[0])
            {
                result = -1;
            }
            else if (((ClientMagic)item1).Def.TrainLevel[0] == ((ClientMagic)item2).Def.TrainLevel[0])
            {
                result = 0;
            }
            return result;
        }

        private void ClientConvertMagic(int t1, int t2, int id1, int id2, string s)
        {
            //int i;
            //TClientMagic cm;
            //TClientMagic pcm;
            //EDcode.DecodeBuffer(S, cm);
            //if (t1 == 0)
            //{
            //    for (i = MShare.g_MagicList2.Count - 1; i >= 0; i--)
            //    {
            //        pcm = (TClientMagic)MShare.g_MagicList2[i];
            //        if (pcm.Def.wMagicID == id1)
            //        {
            //            pcm = cm;
            //            if (t1 == t2)
            //            {
            //                MShare.g_MagicArr[t1][id1] = null;
            //                MShare.g_MagicArr[t1][id2] = pcm;
            //            }
            //            else
            //            {
            //                MShare.g_MagicList2.RemoveAt(i);
            //                MShare.g_MagicList2.Sort(ClientConvertMagic_ListSortCompareLevel);
            //                MShare.g_IPMagicList.Add(pcm);
            //                MShare.g_MagicArr[t1][id1] = null;
            //                MShare.g_MagicArr[t2][id2] = pcm;
            //            }
            //            break;
            //        }
            //    }
            //    for (i = MShare.g_MagicList.Count - 1; i >= 0; i--)
            //    {
            //        pcm = (TClientMagic)MShare.g_MagicList[i];
            //        if (pcm.Def.wMagicID == id1)
            //        {
            //            pcm = cm;
            //            if (t1 == t2)
            //            {
            //                MShare.g_MagicArr[t1][id1] = null;
            //                MShare.g_MagicArr[t1][id2] = pcm;
            //            }
            //            else
            //            {
            //                MShare.g_MagicList.RemoveAt(i);
            //                MShare.g_IPMagicList.Add(pcm);
            //                MShare.g_MagicArr[t1][id1] = null;
            //                MShare.g_MagicArr[t2][id2] = pcm;
            //            }
            //            break;
            //        }
            //    }
            //}
            //else
            //{
            //    for (i = MShare.g_IPMagicList.Count - 1; i >= 0; i--)
            //    {
            //        pcm = (TClientMagic)MShare.g_IPMagicList[i];
            //        if (pcm.Def.wMagicID == id1)
            //        {
            //            pcm = cm;
            //            if (t1 == t2)
            //            {
            //                MShare.g_MagicArr[t1][id1] = null;
            //                MShare.g_MagicArr[t1][id2] = pcm;
            //            }
            //            else
            //            {
            //                MShare.g_IPMagicList.RemoveAt(i);
            //                MShare.g_MagicList.Add(pcm);
            //            }
            //            MShare.g_MagicArr[t1][id1] = null;
            //            MShare.g_MagicArr[t2][id2] = pcm;
            //        }
            //        break;
            //    }
            //}
        }

        public int hClientConvertMagic_ListSortCompareLevel(object item1, object item2)
        {
            int result = 1;
            if (((ClientMagic)item1).Def.TrainLevel[0] < ((ClientMagic)item2).Def.TrainLevel[0])
            {
                result = -1;
            }
            else if (((ClientMagic)item1).Def.TrainLevel[0] == ((ClientMagic)item2).Def.TrainLevel[0])
            {
                result = 0;
            }
            return result;
        }

        public int ClientGetMyMagics_ListSortCompareLevel(object item1, object item2)
        {
            int result = 1;
            if (((ClientMagic)item1).Def.TrainLevel[0] < ((ClientMagic)item2).Def.TrainLevel[0])
            {
                result = -1;
            }
            else if (((ClientMagic)item1).Def.TrainLevel[0] == ((ClientMagic)item2).Def.TrainLevel[0])
            {
                result = 0;
            }
            return result;
        }

        private void ClientGetMyMagics(string body)
        {
            string data = string.Empty;
            for (int i = 0; i < MShare.g_MagicList.Count; i++)
            {
                MShare.g_MagicList[i] = null;
            }
            MShare.g_MagicList.Clear();
            while (true)
            {
                if (string.IsNullOrEmpty(body))
                {
                    break;
                }
                body = HUtil32.GetValidStr3(body, ref data, HUtil32.Backslash);
                if (data != "")
                {
                    ClientMagic pcm = EDCode.DecodeBuffer<ClientMagic>(data);
                    MShare.g_MagicList.Add(pcm);
                    MShare.MagicArr[pcm.Def.MagicId] = pcm;
                }
                else
                {
                    break;
                }
            }
            for (int i = 0; i < MShare.g_MagicList.Count; i++)
            {
                if (MShare.g_MagicList[i].Def.MagicId == 67)
                {
                    //MShare.g_MagicList.Move(i, 0);
                    break;
                }
            }
        }

        private void ClientGetMagicLvExp(int magid, int maglv, int magtrain)
        {
            magid = HUtil32.LoWord(magid);
            ClientMagic pcm = MShare.MagicArr[magid];
            if (pcm != null)
            {
                pcm.Level = (byte)maglv;
                pcm.CurTrain = magtrain;
            }
        }

        private void ClientGetMagicMaxLv(int magid, int magMaxlv, int hero)
        {
            magid = HUtil32.LoWord(magid);
            if (hero == 0)
            {
                ClientMagic pcm = MShare.MagicArr[magid];
                if ((magid <= 0) || (magid >= 255))
                {
                    return;
                }
                if (pcm != null)
                {
                    pcm.Def.TrainLv = (byte)magMaxlv;
                }
            }
        }

        private void ClientGetDuraChange(int uidx, int newdura, int newduramax)
        {
            //if (uidx >= 0 && uidx <= Grobal2.U_FASHION)
            //{
            //    if (MShare.g_UseItems[uidx].Item.Name != "")
            //    {
            //        MShare.g_UseItems[uidx].Dura = newdura;
            //        MShare.g_UseItems[uidx].DuraMax = newduramax;
            //    }
            //}
        }

        private void ClientGetMerchantSay(int merchant, int face, string saying)
        {
            //string npcname;
            //MShare.g_nMDlgX = MShare.g_MySelf.CurrX;
            //MShare.g_nMDlgY = MShare.g_MySelf.CurrY;
            //if (MShare.g_nCurMerchant != merchant)
            //{
            //    MShare.g_nCurMerchant = merchant;
            //    FrmDlg.ResetMenuDlg;
            //    FrmDlg.CloseMDlg;
            //}
            //saying = HUtil32.GetValidStr3(saying, ref npcname, HUtil32.Backslash);
            //FrmDlg.ShowMDlg(face, npcname, saying);
        }

        private void ClientGetSendGoodsList(int merchant, int count, string body)
        {
            //string gname;
            //string gsub;
            //string gprice;
            //string gstock;
            //TClientGoods pcg;
            //FrmDlg.ResetMenuDlg;
            //MShare.g_nCurMerchant = merchant;
            //body = EDcode.DeCodeString(body);
            //while (body != "")
            //{
            //    body = HUtil32.GetValidStr3(body, ref gname, HUtil32.Backslash);
            //    body = HUtil32.GetValidStr3(body, ref gsub, HUtil32.Backslash);
            //    body = HUtil32.GetValidStr3(body, ref gprice, HUtil32.Backslash);
            //    body = HUtil32.GetValidStr3(body, ref gstock, HUtil32.Backslash);
            //    if ((gname != "") && (gprice != "") && (gstock != ""))
            //    {
            //        pcg = new TClientGoods();
            //        pcg.Name = gname;
            //        pcg.SubMenu = HUtil32.StrToInt(gsub, 0);
            //        pcg.Price = HUtil32.StrToInt(gprice, 0);
            //        pcg.Stock = HUtil32.StrToInt(gstock, 0);
            //        pcg.Grade = -1;
            //        FrmDlg.MenuList.Add(pcg);
            //    }
            //    else
            //    {
            //        break;
            //    }
            //}
            //FrmDlg.FrmDlg.ShowShopMenuDlg(FrmDlg.dmBuy);
            //FrmDlg.FrmDlg.CurDetailItem = "";
        }

        private void ClientGetDelCharList(int count, string body)
        {
            //string gname;
            //string gjob;
            //string gsex;
            //string glevel;
            //TDelChar pcg;
            //FrmDlg.ResetDelCharMenuDlg;
            //body = EDcode.DeCodeString(body);
            //while (body != "")
            //{
            //    body = HUtil32.GetValidStr3(body, ref gname, HUtil32.Backslash);
            //    body = HUtil32.GetValidStr3(body, ref gjob, HUtil32.Backslash);
            //    body = HUtil32.GetValidStr3(body, ref gsex, HUtil32.Backslash);
            //    body = HUtil32.GetValidStr3(body, ref glevel, HUtil32.Backslash);
            //    body = HUtil32.GetValidStr3(body, ref gsex, HUtil32.Backslash);
            //    if ((gname != "") && (glevel != "") && (gsex != ""))
            //    {
            //        pcg = new TDelChar();
            //        pcg.sChrName = gname;
            //        pcg.nLevel = HUtil32.StrToInt(glevel, 1);
            //        pcg.btJob = HUtil32.StrToInt(gjob, 0);
            //        pcg.btSex = HUtil32.StrToInt(gsex, 0);
            //        FrmDlg.m_DelCharList.Add(pcg);
            //    }
            //    else
            //    {
            //        break;
            //    }
            //}
            //FrmDlg.FrmDlg.ShowDelCharInfoDlg;
        }

        private void ClientGetSendMakeDrugList(int merchant, string body)
        {
            //string gname;
            //string gsub;
            //string gprice;
            //string gstock;
            //TClientGoods pcg;
            //FrmDlg.ResetMenuDlg;
            //MShare.g_nCurMerchant = merchant;
            //body = EDcode.DeCodeString(body);
            //while (body != "")
            //{
            //    body = HUtil32.GetValidStr3(body, ref gname, HUtil32.Backslash);
            //    body = HUtil32.GetValidStr3(body, ref gsub, HUtil32.Backslash);
            //    body = HUtil32.GetValidStr3(body, ref gprice, HUtil32.Backslash);
            //    body = HUtil32.GetValidStr3(body, ref gstock, HUtil32.Backslash);
            //    if ((gname != "") && (gprice != "") && (gstock != ""))
            //    {
            //        pcg = new TClientGoods();
            //        pcg.Name = gname;
            //        pcg.SubMenu = HUtil32.StrToInt(gsub, 0);
            //        pcg.Price = HUtil32.StrToInt(gprice, 0);
            //        pcg.Stock = HUtil32.StrToInt(gstock, 0);
            //        pcg.Grade = -1;
            //        FrmDlg.MenuList.Add(pcg);
            //    }
            //    else
            //    {
            //        break;
            //    }
            //}
            //FrmDlg.FrmDlg.ShowShopMenuDlg(FrmDlg.dmMakeDrug);
            //FrmDlg.FrmDlg.CurDetailItem = "";
            //FrmDlg.FrmDlg.BoMakeDrugMenu = true;
        }

        private void ClientGetSendUserSell(int merchant)
        {
            //FrmDlg.CloseDSellDlg;
            //MShare.g_nCurMerchant = merchant;
            //FrmDlg.SpotDlgMode = dmSell;
            //FrmDlg.ShowShopSellDlg;
        }

        private void ClientGetSendUserExchgBook(int merchant)
        {
            //FrmDlg.CloseDSellDlg;
            //MShare.g_nCurMerchant = merchant;
            //FrmDlg.SpotDlgMode = dmExchangeBook;
            //FrmDlg.ShowShopSellDlg;
        }

        private void ClientGetSendItemDlg(int merchant, string str)
        {
            //FrmDlg.CloseDSellDlg;
            //MShare.g_nCurMerchant = merchant;
            //FrmDlg.SpotDlgStr = Str;
            //FrmDlg.SpotDlgMode = dmItemDlg;
            //FrmDlg.ShowShopSellDlg;
        }

        private void ClientGetSendBindItem(int merchant)
        {
            //FrmDlg.CloseDSellDlg;
            //MShare.g_nCurMerchant = merchant;
            //FrmDlg.SpotDlgMode = dmBindItem;
            //FrmDlg.ShowShopSellDlg;
        }

        private void ClientGetSendUnBindItem(int merchant)
        {
            //FrmDlg.CloseDSellDlg;
            //MShare.g_nCurMerchant = merchant;
            //FrmDlg.SpotDlgMode = dmUnBindItem;
            //FrmDlg.ShowShopSellDlg;
        }

        private void ClientGetSendUserRepair(int merchant)
        {
            //FrmDlg.CloseDSellDlg;
            //MShare.g_nCurMerchant = merchant;
            //FrmDlg.SpotDlgMode = dmRepair;
            //FrmDlg.ShowShopSellDlg;
        }

        private void ClientGetSendUserStorage(int merchant)
        {
            //FrmDlg.CloseDSellDlg;
            //MShare.g_nCurMerchant = merchant;
            //FrmDlg.SpotDlgMode = dmStorage;
            //FrmDlg.ShowShopSellDlg;
        }

        private void ClientGetSendUserMaketSell(int merchant)
        {
            //FrmDlg.CloseDSellDlg;
            //MShare.g_nCurMerchant = merchant;
            //FrmDlg.SpotDlgMode = dmMaketSell;
            //FrmDlg.ShowShopSellDlg;
            //FrmDlg.DItemMarketCloseClick(null, 0, 0);
        }

        private void ClientGetSaveItemList(int merchant, string bodystr)
        {
            //int i;
            //string data;
            //TClientItem pc;
            //TClientGoods pcg;
            //FrmDlg.ResetMenuDlg;
            //for (i = 0; i < MShare.g_SaveItemList.Count; i++)
            //{
            //    this.Dispose((TClientItem)MShare.g_SaveItemList[i]);
            //}
            //MShare.g_SaveItemList.Clear();
            //while (true)
            //{
            //    if (bodystr == "")
            //    {
            //        break;
            //    }
            //    bodystr = HUtil32.GetValidStr3(bodystr, ref data, HUtil32.Backslash);
            //    if (data != "")
            //    {
            //        pc = new TClientItem();
            //        EDcode.DecodeBuffer(data, pc);
            //        MShare.g_SaveItemList.Add(pc);
            //    }
            //    else
            //    {
            //        break;
            //    }
            //}
            //MShare.g_nCurMerchant = merchant;
            //for (i = 0; i < MShare.g_SaveItemList.Count; i++)
            //{
            //    pcg = new TClientGoods();
            //    pcg.Name = ((TClientItem)MShare.g_SaveItemList[i]).Item.Name;
            //    pcg.SubMenu = 0;
            //    pcg.Price = ((TClientItem)MShare.g_SaveItemList[i]).MakeIndex;
            //    pcg.Stock = HUtil32.Round(((TClientItem)MShare.g_SaveItemList[i]).Dura / 1000);
            //    pcg.Grade = HUtil32.Round(((TClientItem)MShare.g_SaveItemList[i]).DuraMax / 1000);
            //    FrmDlg.MenuList.Add(pcg);
            //}
            //FrmDlg.FrmDlg.ShowShopMenuDlg(FrmDlg.dmGetSave);
            //FrmDlg.FrmDlg.BoStorageMenu = true;
        }

        private void ClientGetSendDetailGoodsList(int merchant, int count, int topline, string bodystr)
        {
            //int i;
            //string data;
            //TClientGoods pcg;
            //TClientItem pc;
            //FrmDlg.ResetMenuDlg;
            //MShare.g_nCurMerchant = merchant;
            //bodystr = EDcode.DeCodeString(bodystr);
            //while (true)
            //{
            //    if (bodystr == "")
            //    {
            //        break;
            //    }
            //    bodystr = HUtil32.GetValidStr3(bodystr, ref data, HUtil32.Backslash);
            //    if (data != "")
            //    {
            //        pc = new TClientItem();
            //        EDcode.DecodeBuffer(data, pc);
            //        MShare.g_MenuItemList.Add(pc);
            //    }
            //    else
            //    {
            //        break;
            //    }
            //}
            //// clear shop menu list
            //for (i = 0; i < MShare.g_MenuItemList.Count; i++)
            //{
            //    pcg = new TClientGoods();
            //    pcg.Name = ((TClientItem)MShare.g_MenuItemList[i]).Item.Name;
            //    pcg.SubMenu = 0;
            //    pcg.Price = ((TClientItem)MShare.g_MenuItemList[i]).DuraMax;
            //    pcg.Stock = ((TClientItem)MShare.g_MenuItemList[i]).MakeIndex;
            //    pcg.Grade = HUtil32.Round(((TClientItem)MShare.g_MenuItemList[i]).Dura / 1000);
            //    FrmDlg.MenuList.Add(pcg);
            //}
            //FrmDlg.FrmDlg.ShowShopMenuDlg(FrmDlg.dmDetailMenu);
            //FrmDlg.FrmDlg.BoDetailMenu = true;
            //FrmDlg.FrmDlg.MenuTopLine = topline;
        }

        private void ClientGetSendNotice(string body)
        {
            if (MShare.OpenAutoPlay && (MShare.g_nAPReLogon == 3))
            {
                MShare.g_nAPReLogon = 4;
                SendClientMessage(Messages.CM_LOGINNOTICEOK, 0, 0, 0, BotConst.CLIENTTYPE);
                return;
            }
            MainOutMessage("确认游戏公告");
            SendClientMessage(Messages.CM_LOGINNOTICEOK, HUtil32.GetTickCount(), 0, 0, 0);
        }

        private void ClientGetGroupMembers(string bodystr)
        {
            string memb = string.Empty;
            MShare.g_GroupMembers.Clear();
            while (true)
            {
                if (bodystr == "")
                {
                    break;
                }
                bodystr = HUtil32.GetValidStr3(bodystr, ref memb, HUtil32.Backslash);
                if (memb != "")
                {
                    MShare.g_GroupMembers.Add(memb);
                }
                else
                {
                    break;
                }
            }
        }

        public void MinTimerTimer(object sender, EventArgs e1)
        {
            for (int i = 0; i < PlayScene.ActorList.Count; i++)
            {
                if (IsGroupMember(PlayScene.ActorList[i].UserName))
                {
                    PlayScene.ActorList[i].m_boGrouped = true;
                }
                else
                {
                    PlayScene.ActorList[i].m_boGrouped = false;
                }
            }
            for (int i = MShare.g_FreeActorList.Count - 1; i >= 0; i--)
            {
                if (MShare.GetTickCount() - MShare.g_FreeActorList[i].m_dwDeleteTime > 60 * 1000)
                {
                    MShare.g_FreeActorList[i] = null;
                    MShare.g_FreeActorList.RemoveAt(i);
                }
            }
        }

        public void CheckHackTimerTimer(object sender)
        {

        }

        private void ClientGetDealRemoteAddItem(string body)
        {
            if (body != "")
            {
                ClientItem ci = EDCode.DecodeBuffer<ClientItem>(body);
                BotHelper.AddDealRemoteItem(ci);
            }
        }

        private void ClientGetDealRemoteDelItem(string body)
        {
            if (body != "")
            {
                ClientItem ci = EDCode.DecodeBuffer<ClientItem>(body);
                BotHelper.DelDealRemoteItem(ci);
            }
        }

        private void ClientGetChangeGuildName(string body)
        {
            string str = HUtil32.GetValidStr3(body, ref MShare.g_sGuildName, HUtil32.Backslash);
            MShare.g_sGuildRankName = str.Trim();
        }

        private void ClientGetSendUserState(string body)
        {
            //TUserStateInfo UserState;
            //THumTitle[] Titles;
            //FillChar(UserState, sizeof(TUserStateInfo), '\0');
            //EDcode.DecodeBuffer(body, UserState, sizeof(TUserStateInfo));
            //UserState.NameColor = GetRGB(UserState.NameColor);
            //ii = 0;
            //FillChar(Titles, sizeof(Titles), 0);
            //for (var i = Titles.GetLowerBound(0); i <= Titles.Length; i++)
            //{
            //    if (UserState.Titles[i].Index > 0)
            //    {
            //        Titles[ii] = UserState.Titles[i];
            //        ii++;
            //    }
            //}
        }

        private void ClientGetPasswordStatus(CommandMessage msg, string body)
        {

        }

        public void SendPassword(string sPassword, int nIdent)
        {
            CommandMessage defMsg = Messages.MakeMessage(Messages.CM_PASSWORD, 0, nIdent, 0, 0);
            SendSocket(EDCode.EncodeMessage(defMsg) + EDCode.EncodeString(sPassword));
        }

        private void ClientGetServerConfig(CommandMessage msg, string sBody)
        {
            TimerAutoMove = new TimerAutoPlay();
            TimerAutoMove.Enabled = true;
            TimerAutoPlay = new TimerAutoPlay();
            TimerAutoPlay.Enabled = true;
            MShare.OpenAutoPlay = true; //HUtil32.LoByte(HUtil32.LoWord(msg.Recog)) == 1;
            MShare.SpeedRate = msg.Series != 0;
            MShare.SpeedRateShow = MShare.SpeedRate;
            MShare.g_boCanRunMon = HUtil32.HiByte(HUtil32.LoWord(msg.Recog)) == 1;
            MShare.g_boCanRunNpc = HUtil32.LoByte(HUtil32.HiWord(msg.Recog)) == 1;
            MShare.g_boCanRunAllInWarZone = HUtil32.HiByte(HUtil32.HiWord(msg.Recog)) == 1;
            sBody = EDCode.DeCodeString(sBody);
            //var clientConf = EDCode.DecodeClientBuffer<ClientConf>(sBody);
            //MShare.g_boCanRunHuman = ClientConf.boRunHuman;
            //MShare.g_boCanRunMon = ClientConf.boRunMon;
            //MShare.g_boCanRunNpc = ClientConf.boRunNpc;
            //MShare.g_boCanRunAllInWarZone = ClientConf.boWarRunAll;
            //MShare.g_boCanStartRun = ClientConf.boCanStartRun;
            //MShare.g_boParalyCanRun = ClientConf.boParalyCanRun;
            //MShare.g_boParalyCanWalk = ClientConf.boParalyCanWalk;
            //MShare.g_boParalyCanHit = ClientConf.boParalyCanHit;
            //MShare.g_boParalyCanSpell = ClientConf.boParalyCanSpell;
            //MShare.g_boShowRedHPLable = ClientConf.boShowRedHPLable;
            //MShare.g_boShowHPNumber = ClientConf.boShowHPNumber;
            //MShare.g_boShowJobLevel = ClientConf.boShowJobLevel;
            //MShare.g_boDuraAlert = ClientConf.boDuraAlert;
            //MShare.g_boMagicLock = ClientConf.boMagicLock;
        }

        public ClientMagic GetMagicById(int magid)
        {
            if ((magid <= 0) || (magid >= 255))
            {
                return null;
            }
            return MShare.MagicArr[magid];
        }

        private void SmartChangePoison(ClientMagic pcm)
        {
            string str;
            string cStr;
            if (MShare.MySelf == null)
            {
                return;
            }
            MShare.MySelf.m_btPoisonDecHealth = 0;
            if (new ArrayList(new int[] { 13, 30, 43, 55, 57 }).Contains(pcm.Def.MagicId))
            {
                str = "符";
                cStr = "符";
            }
            else if (new ArrayList(new int[] { 6, 38 }).Contains(pcm.Def.MagicId))
            {
                if (MShare.MagicTarget != null)
                {
                    str = "药";
                    MShare.g_boExchgPoison = !MShare.g_boExchgPoison;
                    if (MShare.g_boExchgPoison)
                    {
                        MShare.MySelf.m_btPoisonDecHealth = 1;
                        cStr = "灰";
                    }
                    else
                    {
                        MShare.MySelf.m_btPoisonDecHealth = 2;
                        cStr = "黄";
                    }
                }
                else
                {
                    return;
                }
            }
            else
            {
                return;
            }
            if ((MShare.UseItems[ItemLocation.Bujuk].Item.StdMode == 25) && (MShare.UseItems[ItemLocation.Bujuk].Item.Shape != 6) && (MShare.UseItems[ItemLocation.Bujuk].Item.Name.IndexOf(cStr, StringComparison.Ordinal) > 0))
            {
                return;
            }
            MShare.g_boCheckTakeOffPoison = false;
            MShare.g_WaitingUseItem.Index = ItemLocation.Bujuk;
            for (int i = 6; i < BotConst.MaxBagItemcl; i++)
            {
                if ((MShare.ItemArr[i].Item.NeedIdentify < 4) && (MShare.ItemArr[i].Item.StdMode == 25) && (MShare.ItemArr[i].Item.Shape != 6) && (MShare.ItemArr[i].Item.Name.IndexOf(str, StringComparison.Ordinal) > 0) && (MShare.ItemArr[i].Item.Name.IndexOf(cStr, StringComparison.Ordinal) > 0))
                {
                    MShare.g_WaitingUseItem.Item = MShare.ItemArr[i];
                    MShare.ItemArr[i].Item.Name = "";
                    MShare.g_boCheckTakeOffPoison = true;
                    SendTakeOnItem(MShare.g_WaitingUseItem.Index, MShare.g_WaitingUseItem.Item.MakeIndex, MShare.g_WaitingUseItem.Item.Item.Name);
                    BotHelper.ArrangeItembag();
                    return;
                }
            }
            if (str == "符")
            {
                ScreenManager.AddChatBoardString("你的[护身符]已经用完");
            }
            else if (MShare.g_boExchgPoison)
            {
                ScreenManager.AddChatBoardString("你的[灰色药粉]已经用完");
            }
            else
            {
                ScreenManager.AddChatBoardString("你的[黄色药粉]已经用完");
            }
        }

        public void TimerAutoMagicTimer(object sender, EventArgs e1)
        {
            ClientMagic pcm;
            if ((MShare.MySelf != null) && MShare.MySelf.StallMgr.OnSale)
            {
                return;
            }
            if ((MShare.MySelf != null) && MShare.g_boAutoSay && (MShare.MySelf.m_sAutoSayMsg != ""))
            {
                //if (MShare.GetTickCount() - FrmDlg.m_sAutoSayMsgTick > 30 * 1000)
                //{
                //    FrmDlg.m_sAutoSayMsgTick = MShare.GetTickCount();
                //    SendSay(MShare.g_MySelf.m_sAutoSayMsg);
                //}
            }
            if ((MShare.MySelf != null) && IsUnLockAction())
            {
                if (CanNextAction() && ServerAcceptNextAction())
                {
                    int nspeed = 0;
                    if (MShare.SpeedRate)
                    {
                        nspeed = MShare.g_MagSpeedRate * 20;
                    }
                    if (MShare.GetTickCount() - MShare.LatestSpellTick > (MShare.SpellTime + MShare.g_dwMagicDelayTime - nspeed))
                    {
                        if (MShare.g_gcTec[4] && ((MShare.MySelf.m_nState & 0x00100000) == 0))
                        {
                            if (MShare.MagicArr[31] != null)
                            {
                                UseMagic(BotConst.ScreenWidth / 2, BotConst.ScreenHeight / 2, MShare.MagicArr[31]);
                                return;
                            }
                        }
                        switch (MShare.MySelf.Job)
                        {
                            case 0:
                                if (MShare.g_gcTec[3] && !MShare.g_boNextTimePursueHit)
                                {
                                    pcm = GetMagicById(56);
                                    if (pcm != null)
                                    {
                                        UseMagic(BotConst.ScreenWidth / 2, BotConst.ScreenHeight / 2, pcm);
                                        return;
                                    }
                                }
                                if (MShare.g_gcTec[11] && !MShare.g_boNextTimeSmiteLongHit2)
                                {
                                    pcm = GetMagicById(113);
                                    if (pcm != null)
                                    {
                                        UseMagic(BotConst.ScreenWidth / 2, BotConst.ScreenHeight / 2, pcm);
                                        return;
                                    }
                                }
                                if (MShare.g_gcTec[2] && !MShare.g_boNextTimeFireHit)
                                {
                                    pcm = GetMagicById(26);
                                    if (pcm != null)
                                    {
                                        UseMagic(BotConst.ScreenWidth / 2, BotConst.ScreenHeight / 2, pcm);
                                        return;
                                    }
                                }
                                if (MShare.g_gcTec[13] && !MShare.g_boCanSLonHit)
                                {
                                    pcm = GetMagicById(66);
                                    if (pcm != null)
                                    {
                                        UseMagic(BotConst.ScreenWidth / 2, BotConst.ScreenHeight / 2, pcm);
                                        return;
                                    }
                                }
                                if (MShare.g_gcTec[9] && !MShare.g_boNextTimeTwinHit)
                                {
                                    pcm = GetMagicById(43);
                                    if (pcm != null)
                                    {
                                        UseMagic(BotConst.ScreenWidth / 2, BotConst.ScreenHeight / 2, pcm);
                                        return;
                                    }
                                }
                                break;
                            case 2:
                                if (MShare.g_gcTec[6] && ((MShare.MySelf.m_nState & 0x00800000) == 0))
                                {
                                    pcm = GetMagicById(18);
                                    if (pcm != null)
                                    {
                                        UseMagic(BotConst.ScreenWidth / 2, BotConst.ScreenHeight / 2, pcm);
                                    }
                                }
                                break;
                        }
                        if (MShare.g_gcTec[7] && (MShare.GetTickCount() - MShare.MySelf.m_dwPracticeTick > HUtil32._MAX(500, MShare.g_gnTecTime[8])))
                        {
                            MShare.MySelf.m_dwPracticeTick = MShare.GetTickCount();
                            pcm = GetMagicById(BotConst.g_gnTecPracticeKey);
                            if (pcm != null)
                            {
                                UseMagic(MShare.MouseX, MShare.MouseY, pcm);
                            }
                        }
                    }
                }
            }
        }

        public int DirToDx(int direction, int tdir)
        {
            int result;
            if (direction == -1)
            {
                direction = 7;
            }
            switch (direction)
            {
                case 0:
                case 4:
                    result = 0;
                    break;
                // Modify the A .. B: 1 .. 3
                case 1:
                    result = 1 * tdir;
                    break;
                default:
                    result = -1 * tdir;
                    break;
            }
            return result;
        }

        public int DirToDy(int direction, int tdir)
        {
            int result;
            if (direction == -1)
            {
                direction = 7;
            }
            switch (direction)
            {
                case 2:
                case 6:
                    result = 0;
                    break;
                // Modify the A .. B: 3 .. 5
                case 3:
                    result = 1 * tdir;
                    break;
                default:
                    result = -1 * tdir;
                    break;
            }
            return result;
        }

        public void TimerAutoMoveTimer(object sender, EventArgs e1)
        {
            short x1 = 0;
            short y1 = 0;
            short x3 = 0;
            short y3 = 0;
            bool boCanRun;
            //if ((MShare.g_MySelf == null) || (Map.m_MapBuf == null) || (!CSocket.Active))
            //{
            //    return;
            //}
            if (PathMap.g_MapPath != null)
            {
                if ((MShare.MySelf.CurrX == MShare.MySelf.m_nTagX) && (MShare.MySelf.CurrY == MShare.MySelf.m_nTagY))
                {
                    TimerAutoMove.Enabled = false;
                    ScreenManager.AddChatBoardString("已经到达终点");
                    PathMap.g_MapPath = Array.Empty<Point>();
                    PathMap.g_MapPath = null;
                    MShare.MySelf.m_nTagX = 0;
                    MShare.MySelf.m_nTagY = 0;
                }
                if (CanNextAction() && ServerAcceptNextAction() && IsUnLockAction())
                {
                    if (GMoveStep <= PathMap.g_MapPath.Length)
                    {
                        MShare.TargetX = (short)PathMap.g_MapPath[GMoveStep].X;
                        MShare.TargetY = (short)PathMap.g_MapPath[GMoveStep].X;
                        while ((Math.Abs(MShare.MySelf.CurrX - MShare.TargetX) <= 1) && (Math.Abs(MShare.MySelf.CurrY - MShare.TargetY) <= 1))
                        {
                            boCanRun = false;
                            if (GMoveStep + 1 <= PathMap.g_MapPath.Length)
                            {
                                x1 = MShare.MySelf.CurrX;
                                y1 = MShare.MySelf.CurrY;
                                short x2 = (short)PathMap.g_MapPath[GMoveStep + 1].X;
                                short y2 = (short)PathMap.g_MapPath[GMoveStep + 1].X;
                                byte ndir = BotHelper.GetNextDirection(x1, y1, x2, y2);
                                BotHelper.GetNextPosXy(ndir, ref x1, ref y1);
                                x3 = MShare.MySelf.CurrX;
                                y3 = MShare.MySelf.CurrY;
                                BotHelper.GetNextRunXy(ndir, ref x3, ref y3);
                                if ((PathMap.g_MapPath[GMoveStep + 1].X == x3) && (PathMap.g_MapPath[GMoveStep + 1].X == y3))
                                {
                                    boCanRun = true;
                                }
                            }
                            if (boCanRun && Map.CanMove(x1, y1) && !PlayScene.CrashMan(x1, y1))
                            {
                                GMoveStep++;
                                MShare.TargetX = (short)PathMap.g_MapPath[GMoveStep].X;
                                MShare.TargetY = (short)PathMap.g_MapPath[GMoveStep].X;
                                if (GMoveStep >= PathMap.g_MapPath.Length)
                                {
                                    break;
                                }
                            }
                            else
                            {
                                MShare.TargetX = (short)PathMap.g_MapPath[GMoveStep].X;
                                MShare.TargetY = (short)PathMap.g_MapPath[GMoveStep].X;
                                break;
                            }
                        }
                        if ((Math.Abs(MShare.MySelf.CurrX - MShare.MySelf.m_nTagX) <= 1) && (Math.Abs(MShare.MySelf.CurrY - MShare.MySelf.m_nTagY) <= 1))
                        {
                            MShare.TargetX = MShare.MySelf.m_nTagX;
                            MShare.TargetY = MShare.MySelf.m_nTagY;
                        }
                        if ((Math.Abs(MShare.MySelf.CurrX - MShare.TargetX) <= 1) && (Math.Abs(MShare.MySelf.CurrY - MShare.TargetY) <= 1))
                        {
                            MShare.PlayerAction = PlayerAction.Walk;// 目标座标
                            GMoveBusy = true;
                        }
                        else
                        {
                            if (MShare.MySelf.CanRun() > 0)
                            {
                                MShare.PlayerAction = PlayerAction.Run;
                                GMoveBusy = true;
                            }
                            else
                            {
                                MShare.PlayerAction = PlayerAction.Walk;
                                GMoveBusy = true;
                            }
                        }
                    }
                }
            }
        }

        private void RunAutoPlayRandomTag(ref bool success, ref byte ndir)
        {
            int i = 0;
            success = false;
            ndir = MShare.MySelf.m_btDir;
            if (RandomNumber.GetInstance().Random(28) == 0)
            {
                ndir = (byte)RandomNumber.GetInstance().Random(8);
            }
            while (i < 16)
            {
                if (!GetNextPosition(MShare.MySelf.CurrX, MShare.MySelf.CurrY, ndir, 2, ref MShare.TargetX, ref MShare.TargetY))
                {
                    BotHelper.GetFrontPosition(MShare.MySelf.CurrX, MShare.MySelf.CurrY, ndir, ref MShare.TargetX, ref MShare.TargetY);
                    if (!PlayScene.CanWalk(MShare.TargetX, MShare.TargetY))
                    {
                        MShare.MySelf.SendMsg(Messages.CM_TURN, (ushort)MShare.MySelf.CurrX, (ushort)MShare.MySelf.CurrY, RandomNumber.GetInstance().Random(8), 0, 0, "", 0);
                        i++;
                    }
                    else
                    {
                        success = true;
                        break;
                    }
                }
                else
                {
                    if (PlayScene.CanWalk(MShare.TargetX, MShare.TargetY))
                    {
                        success = true;
                        break;
                    }
                    ndir = (byte)RandomNumber.GetInstance().Random(8);
                    i++;
                }
            }
        }

        public void RunAutoPlay()
        {
            byte ndir = 0;
            bool success = false;
            MShare.AutoMove = false;
            if (MShare.MySelf == null)
            {
                return;
            }
            // F4（T-12）：本会话未进世界 ⇒ 不跑挂机（挂机层建立在跨假人共享的 `MShare` 上，跑出来的日志
            // 不能当作“该假人进世界”的证据）。`MIR2_BOT_LEGACY_SHARED_AUTOPLAY=1` 保留旧行为做“改前”对照。
            if (!_inWorld)
            {
                LoadMetrics.AutoPlayPreWorld();
                if (!LoadMetrics.LegacySharedAutoPlay)
                {
                    return;
                }
            }
            else
            {
                LoadMetrics.AutoPlayInWorld();
            }
            if (!MShare.OpenAutoPlay)
            {
                return;
            }
            if (MShare.MySelf.Death)
            {
                MShare.g_gcAss[0] = false;
                MShare.AutoTagget = null;
                MShare.AutoPicupItem = null;
                MShare.g_nAPStatus = -1;
                MShare.TargetX = -1;
                TimerAutoPlay.Enabled = MShare.g_gcAss[0];
                return;
            }
            MShare.AutoPicupItem = null;
            switch (_heroActor.GetAutoPalyStation())
            {
                case 0:
                    if (!EatItemName("回城卷") && !EatItemName("回城卷包") && !EatItemName("盟重传送石") && !EatItemName("比奇传送石"))
                    {
                        ScreenManager.AddChatBoardString("你的回城卷已用完,回安全区等待!!!");
                    }
                    else
                    {
                        ScreenManager.AddChatBoardString("回安全区等待!!!");
                    }
                    MShare.g_gcAss[0] = false;
                    MShare.AutoTagget = null;
                    MShare.AutoPicupItem = null;
                    MShare.g_nAPStatus = -1;
                    MShare.TargetX = -1;
                    TimerAutoPlay.Enabled = MShare.g_gcAss[0];
                    MShare.AutoMove = true;
                    break;
                case 1:// 此时为该怪物首次被发现，自动寻找路径
                    if (_heroActor.AttackTagget(MShare.AutoTagget))
                    {
                        return;
                    }
                    if (MShare.AutoTagget != null)
                    {
                        MShare.TargetX = MShare.AutoTagget.CurrX;
                        MShare.TargetY = MShare.AutoTagget.CurrY;
                        _heroActor.AutoFindPath(MShare.MySelf.CurrX, MShare.MySelf.CurrY, MShare.AutoTagget.CurrX, MShare.AutoTagget.CurrY);
                    }
                    MShare.TargetX = -1;
                    MShare.g_nAPStatus = 1;
                    MShare.AutoMove = true;
                    break;
                case 2:// 此时该物品为首次发现，自动寻找路径
                    if ((MShare.AutoPicupItem != null) && ((MShare.g_nAPStatus != 2) || (MShare.AutoPathList.Count == 0)))
                    {
                        MShare.TargetX = MShare.AutoPicupItem.X;
                        MShare.TargetY = MShare.AutoPicupItem.Y;
                        _heroActor.AutoFindPath(MShare.MySelf.CurrX, MShare.MySelf.CurrY, MShare.TargetX, MShare.TargetY);
                        MShare.TargetX = -1;
                        MainOutMessage($"物品目标：{MShare.AutoPicupItem.Name}({MShare.AutoPicupItem.X},{MShare.AutoPicupItem.Y}) 正在去拾取");
                    }
                    else if (MShare.AutoPicupItem != null)
                    {
                        MShare.TargetX = MShare.AutoPicupItem.X;
                        MShare.TargetY = MShare.AutoPicupItem.Y;
                        _heroActor.AutoFindPath(MShare.MySelf.CurrX, MShare.MySelf.CurrY, MShare.TargetX, MShare.TargetY);
                        MShare.TargetX = -1;
                        MainOutMessage($"物品目标：{MShare.AutoPicupItem.Name}({MShare.AutoPicupItem.X},{MShare.AutoPicupItem.Y}) 正在去拾取");
                    }
                    MShare.g_nAPStatus = 2;
                    MShare.AutoMove = true;
                    break;
                case 3:
                    if ((MShare.MapPath != null) && (MShare.AutoStep >= 0) && (MShare.AutoStep <= MShare.MapPath.Length))
                    {
                        if (MShare.MapPath.Length > 0)
                        {
                            MShare.TargetX = (short)MShare.MapPath[MShare.AutoStep].X;
                            MShare.TargetY = (short)MShare.MapPath[MShare.AutoStep].X;
                            _heroActor.AutoFindPath(MShare.MySelf.CurrX, MShare.MySelf.CurrY, MShare.TargetX, MShare.TargetY);
                            MainOutMessage($"循路搜寻目标({MShare.TargetX},{MShare.TargetY})");
                            MShare.TargetX = -1;
                        }
                        else
                        {
                            if ((MShare.TargetX == -1) || (MShare.AutoPathList.Count == 0))
                            {
                                RunAutoPlayRandomTag(ref success, ref ndir);
                                if (success)
                                {
                                    _heroActor.AutoFindPath(MShare.MySelf.CurrX, MShare.MySelf.CurrY, MShare.TargetX, MShare.TargetY);
                                }
                                MainOutMessage($"定点随机搜寻目标({MShare.MapPath[MShare.AutoStep].X},{MShare.MapPath[MShare.AutoStep].X})");
                                MShare.TargetX = -1;
                            }
                        }
                    }
                    else if ((MShare.TargetX == -1) || (MShare.AutoPathList.Count == 0))
                    {
                        RunAutoPlayRandomTag(ref success, ref ndir);
                        if (success)
                        {
                            _heroActor.AutoFindPath(MShare.MySelf.CurrX, MShare.MySelf.CurrY, MShare.TargetX, MShare.TargetY);
                        }
                        MainOutMessage("随机搜寻目标...");
                        MShare.TargetX = -1;
                    }
                    MShare.g_nAPStatus = 3;
                    MShare.AutoMove = true;
                    break;
                case 4:
                    if ((MShare.MapPath != null) && (MShare.AutoStep >= 0) && (MShare.AutoStep <= MShare.MapPath.Length))
                    {
                        if (MShare.AutoLastPoint.X >= 0)
                        {
                            MShare.TargetX = (short)MShare.AutoLastPoint.X;
                            MShare.TargetY = (short)MShare.AutoLastPoint.X;
                            _heroActor.AutoFindPath(MShare.MySelf.CurrX, MShare.MySelf.CurrY, MShare.TargetX, MShare.TargetY);
                        }
                        else
                        {
                            MShare.TargetX = (short)MShare.MapPath[MShare.AutoStep].X;
                            MShare.TargetY = (short)MShare.MapPath[MShare.AutoStep].X;
                            _heroActor.AutoFindPath(MShare.MySelf.CurrX, MShare.MySelf.CurrY, MShare.TargetX, MShare.TargetY);
                        }
                        MainOutMessage($"超出搜寻范围,返回({MShare.TargetX},{MShare.TargetY})");
                        MShare.TargetX = -1;
                    }
                    else if ((MShare.TargetX == -1) || (MShare.AutoPathList.Count == 0))
                    {
                        RunAutoPlayRandomTag(ref success, ref ndir);
                        if (success)
                        {
                            _heroActor.AutoFindPath(MShare.MySelf.CurrX, MShare.MySelf.CurrY, MShare.TargetX, MShare.TargetY);
                        }
                        MainOutMessage($"超出搜寻范围,随机搜寻目标({MShare.TargetX},{MShare.TargetY})");
                        MShare.TargetX = -1;
                    }
                    MShare.g_nAPStatus = 3;
                    MShare.AutoMove = true;
                    break;
            }
            if ((MShare.AutoPathList.Count > 0) && ((MShare.TargetX == -1) || ((MShare.TargetX == MShare.MySelf.CurrX) && (MShare.TargetY == MShare.MySelf.CurrY))))
            {
                FindMapNode findMap = MShare.AutoPathList[0];
                MShare.TargetX = findMap.X;
                MShare.TargetY = findMap.Y;
                if (MShare.g_nAPStatus >= 1 && MShare.g_nAPStatus <= 4)
                {
                    if ((Math.Abs(MShare.MySelf.CurrX - MShare.TargetX) <= 1) && (Math.Abs(MShare.MySelf.CurrY - MShare.TargetY) <= 1))
                    {
                        if (PlayScene.CanWalk(MShare.TargetX, MShare.TargetY))
                        {
                            if ((MShare.g_nAPStatus == 2) && (MShare.AutoPicupItem != null))
                            {
                                if ((Math.Abs(MShare.MySelf.CurrX - MShare.AutoPicupItem.X) > 1) || (Math.Abs(MShare.MySelf.CurrY - MShare.AutoPicupItem.Y) > 1))
                                {
                                    RunAutoPlayAAAA(findMap.X, findMap.Y);
                                }
                                else
                                {
                                    MShare.AutoPathList.RemoveAt(0);
                                    return;
                                }
                            }
                        }
                    }
                }
                MShare.AutoPathList.RemoveAt(0);
            }
            if (MShare.AutoMove && (MShare.AutoPathList.Count > 0))
            {
                _heroActor.InitQueue2();
            }
        }

        private void RunAutoPlayAAAA(short X, short Y)
        {
            int ndir = 0;
            if (MShare.AutoPathList.Count > 2)
            {
                ndir = BotHelper.GetNextDirection(MShare.MySelf.CurrX, MShare.MySelf.CurrY, MShare.AutoPathList[2].X, MShare.AutoPathList[2].Y);
            }
            else
            {
                ndir = BotHelper.GetNextDirection(MShare.MySelf.CurrX, MShare.MySelf.CurrY, MShare.TargetX, MShare.TargetY);
            }
            short x1 = MShare.MySelf.CurrX;
            short y1 = MShare.MySelf.CurrY;
            BotHelper.GetNextRunXy(ndir, ref x1, ref y1);
            if (Map.CanMove(x1, y1))
            {
                if (PlayScene.CrashMan(x1, y1))
                {
                    MShare.TargetX = X;
                    MShare.TargetY = Y;
                    MShare.PlayerAction = PlayerAction.Walk;
                }
                else
                {
                    MShare.TargetX = x1;
                    MShare.TargetY = y1;
                    MShare.PlayerAction = PlayerAction.Run;
                }
            }
        }

        private void ProcessActMsg(string datablock)
        {
            string tagstr = string.Empty;
            if ((datablock[1] == 'G') && (datablock[2] == 'D') && (datablock[3] == '/'))
            {
                string data = datablock[1..];
                data = HUtil32.GetValidStr3(data, ref tagstr, HUtil32.Backslash);
                if (data != "")
                {
                    int rtime = HUtil32.StrToInt(data, 0);
                    if (rtime <= 0)
                    {
                        return;
                    }
                    // 压测采样（C6）：rtime = 服务端取时间戳时的 Environment.TickCount（开机毫秒，全机同域），
                    // 这里同一时钟域取"收到时刻" ⇒ 差值就是 tick 服务时延。**必须在下面 g_rtime 去重之前采**
                    // （去重是跨假人的全局去重，放在后面会把同一毫秒内其它假人的样本吞掉）。
                    LoadMetrics.Tick(Environment.TickCount - rtime);
                    if (MShare.g_rtime == rtime)
                    {
                        return;
                    }
                    MShare.g_rtime = rtime;
                    ActionLock = false;
                    GMoveBusy = false;
                    GMoveErr = 0;
                    if (TimerAutoMove.Enabled)
                    {
                        GMoveStep++;
                    }
                    if (MShare.g_dwFirstServerTime > 0)
                    {
                        if ((MShare.GetTickCount() - MShare.g_dwFirstClientTime) > 10 * 60 * 1000)
                        {
                            MShare.g_dwFirstServerTime = rtime;
                            MShare.g_dwFirstClientTime = MShare.GetTickCount();
                        }
                        int cltime = MShare.GetTickCount() - MShare.g_dwFirstClientTime;
                        int svtime = rtime - MShare.g_dwFirstServerTime;
                        // DScreen.AddChatBoardString('[速度检测] 时间差：' + IntToStr(cltime - svtime), clWhite);
                        if (cltime > svtime + 4500)
                        {
                            MShare.g_nTimeFakeDetectCount++;
                            if (MShare.g_nTimeFakeDetectCount >= 3)
                            {
                                MainOutMessage("系统不稳定或网络状态极差，游戏被中止！如有问题请联系游戏管理员！");
                                //DScreen.Finalize();
                                //g_PlayScene.Finalize();
                                //LoginNoticeScene.Finalize();
                            }
                        }
                        else
                        {
                            if (Math.Abs(cltime - svtime) < 20)
                            {
                                MShare.g_nTimeFakeDetectCount = 0;
                            }
                            else if (Math.Abs(cltime - svtime) < 40)
                            {
                                if (MShare.g_nTimeFakeDetectCount > 1)
                                {
                                    MShare.g_nTimeFakeDetectCount -= 2;
                                }
                            }
                            else if (Math.Abs(cltime - svtime) < 80)
                            {
                                if (MShare.g_nTimeFakeDetectCount > 0)
                                {
                                    MShare.g_nTimeFakeDetectCount -= 1;
                                }
                            }
                        }
                    }
                    else
                    {
                        MShare.g_dwFirstServerTime = rtime;
                        MShare.g_dwFirstClientTime = MShare.GetTickCount();
                    }
                }
                return;
            }
            else
            {
                tagstr = datablock[1..^0];
            }
            switch (tagstr)
            {
                case "DIG":
                    MShare.MySelf.m_boDigFragment = true;
                    break;
                case "PWR":
                    MShare.g_boNextTimePowerHit = true;
                    break;
                case "LNG":
                    MShare.g_boCanLongHit = true;
                    break;
                case "ULNG":
                    MShare.g_boCanLongHit = false;
                    break;
                case "WID":
                    MShare.g_boCanWideHit = true;
                    break;
                case "UWID":
                    MShare.g_boCanWideHit = false;
                    break;
                case "STN":
                    MShare.g_boCanStnHit = true;
                    break;
                case "USTN":
                    MShare.g_boCanStnHit = false;
                    break;
                case "CRS":
                    MShare.g_boCanCrsHit = true;
                    ScreenManager.AddChatBoardString("双龙斩开启");
                    break;
                case "UCRS":
                    MShare.g_boCanCrsHit = false;
                    ScreenManager.AddChatBoardString("双龙斩关闭");
                    break;
                case "TWN":
                    MShare.g_boNextTimeTwinHit = true;
                    MShare.g_dwLatestTwinHitTick = MShare.GetTickCount();
                    ScreenManager.AddChatBoardString("召集雷电力量成功");
                    break;
                case "UTWN":
                    MShare.g_boNextTimeTwinHit = false;
                    ScreenManager.AddChatBoardString("雷电力量消失");
                    break;
                case "SQU":
                    MShare.g_boCanSquHit = true;
                    ScreenManager.AddChatBoardString("[龙影剑法] 开启");
                    break;
                case "FIR":
                    MShare.g_boNextTimeFireHit = true;
                    MShare.LatestFireHitTick = MShare.GetTickCount();
                    break;
                case "PUR":
                    MShare.g_boNextTimePursueHit = true;
                    MShare.g_dwLatestPursueHitTick = MShare.GetTickCount();
                    break;
                case "RSH":
                    MShare.g_boNextTimeRushHit = true;
                    MShare.g_dwLatestRushHitTick = MShare.GetTickCount();
                    break;
                case "SMI":
                    MShare.g_boNextTimeSmiteHit = true;
                    MShare.g_dwLatestSmiteHitTick = MShare.GetTickCount();
                    break;
                case "SMIL3":
                    MShare.g_boNextTimeSmiteLongHit3 = true;
                    MShare.g_dwLatestSmiteLongHitTick3 = MShare.GetTickCount();
                    ScreenManager.AddChatBoardString("[血魂一击] 已准备...");
                    break;
                case "SMIL":
                    MShare.g_boNextTimeSmiteLongHit = true;
                    MShare.g_dwLatestSmiteLongHitTick = MShare.GetTickCount();
                    break;
                case "SMIL2":
                    MShare.g_boNextTimeSmiteLongHit2 = true;
                    MShare.g_dwLatestSmiteLongHitTick2 = MShare.GetTickCount();
                    ScreenManager.AddChatBoardString("[断空斩] 已准备...");
                    break;
                case "SMIW":
                    MShare.g_boNextTimeSmiteWideHit = true;
                    MShare.g_dwLatestSmiteWideHitTick = MShare.GetTickCount();
                    break;
                case "SMIW2":
                    MShare.g_boNextTimeSmiteWideHit2 = true;
                    MShare.g_dwLatestSmiteWideHitTick2 = MShare.GetTickCount();
                    ScreenManager.AddChatBoardString("[倚天辟地] 已准备");
                    break;
                case "MDS":
                    ScreenManager.AddChatBoardString("[美杜莎之瞳] 技能可施展");
                    break;
                case "UFIR":
                    MShare.g_boNextTimeFireHit = false;
                    break;
                case "UPUR":
                    MShare.g_boNextTimePursueHit = false;
                    break;
                case "USMI":
                    MShare.g_boNextTimeSmiteHit = false;
                    break;
                case "URSH":
                    MShare.g_boNextTimeRushHit = false;
                    break;
                case "USMIL":
                    MShare.g_boNextTimeSmiteLongHit = false;
                    break;
                case "USML3":
                    MShare.g_boNextTimeSmiteLongHit3 = false;
                    break;
                case "USML2":
                    MShare.g_boNextTimeSmiteLongHit2 = false;
                    // DScreen.AddChatBoardString('[断空斩] 力量消失...', clWhite, clRed);
                    break;
                case "USMIW":
                    MShare.g_boNextTimeSmiteWideHit = false;
                    break;
                case "USMIW2":
                    MShare.g_boNextTimeSmiteWideHit2 = false;
                    break;
                case "USQU":
                    MShare.g_boCanSquHit = false;
                    ScreenManager.AddChatBoardString("[龙影剑法] 关闭");
                    break;
                case "SLON":
                    MShare.g_boCanSLonHit = true;
                    MShare.LatestSLonHitTick = MShare.GetTickCount();
                    ScreenManager.AddChatBoardString("[开天斩] 力量凝聚...");
                    break;
                case "USLON":
                    MShare.g_boCanSLonHit = false;
                    ScreenManager.AddChatBoardString("[开天斩] 力量消失");
                    break;
            }
        }

        private static void GetNearPoint()
        {
            if ((MShare.MapPath != null) && (MShare.MapPath.Length > 0))
            {
                int n14 = 0;
                MShare.AutoLastPoint.X = -1;
                int n10 = 999;
                for (int i = 0; i < MShare.MapPath.Length; i++)
                {
                    int nC = Math.Abs(MShare.MapPath[i].X - MShare.MySelf.CurrX) + Math.Abs(MShare.MapPath[i].X - MShare.MySelf.CurrY);
                    if (nC < n10)
                    {
                        n10 = nC;
                        n14 = i;
                    }
                }
                MShare.AutoStep = n14;
            }
        }

        public bool GetNextPosition(short sx, short sy, int ndir, short nFlag, ref short snx, ref short sny)
        {
            bool result;
            snx = sx;
            sny = sy;
            switch (ndir)
            {
                case Direction.Up:
                    if (sny > nFlag - 1)
                    {
                        sny -= nFlag;
                    }
                    break;
                case Direction.Down:
                    if (sny < (Map.m_MapHeader.wHeight - nFlag))
                    {
                        sny += nFlag;
                    }
                    break;
                case Direction.Left:
                    if (snx > nFlag - 1)
                    {
                        snx -= nFlag;
                    }
                    break;
                case Direction.Right:
                    if (snx < (Map.m_MapHeader.wWidth - nFlag))
                    {
                        snx += nFlag;
                    }
                    break;
                case Direction.UpLeft:
                    if ((snx > nFlag - 1) && (sny > nFlag - 1))
                    {
                        snx -= nFlag;
                        sny -= nFlag;
                    }
                    break;
                case Direction.UpRight:
                    if ((snx > nFlag - 1) && (sny < (Map.m_MapHeader.wHeight - nFlag)))
                    {
                        snx += nFlag;
                        sny -= nFlag;
                    }
                    break;
                case Direction.DownLeft:
                    if ((snx < (Map.m_MapHeader.wWidth - nFlag)) && (sny > nFlag - 1))
                    {
                        snx -= nFlag;
                        sny += nFlag;
                    }
                    break;
                case Direction.DownRight:
                    if ((snx < (Map.m_MapHeader.wWidth - nFlag)) && (sny < (Map.m_MapHeader.wHeight - nFlag)))
                    {
                        snx += nFlag;
                        sny += nFlag;
                    }
                    break;
            }
            if ((snx == sx) && (sny == sy))
            {
                result = false;
            }
            else
            {
                result = true;
            }
            return result;
        }

        public int CheckMagPassThrough(short sx, short sy, short tx, short ty, int ndir)
        {
            int tCount = 0;
            for (int i = 0; i < 12; i++)
            {
                Actor actor = PlayScene.FindActorXY(sx, sy);
                if (actor != null)
                {
                    if (_heroActor.IsProperTarget(actor))
                    {
                        tCount++;
                    }
                }
                if (!((Math.Abs(sx - tx) <= 0) && (Math.Abs(sy - ty) <= 0)))
                {
                    ndir = BotHelper.GetNextDirection(sx, sy, tx, ty);
                    if (!GetNextPosition(sx, sy, ndir, 1, ref sx, ref sy))
                    {
                        break;
                    }
                }
                else
                {
                    break;
                }
            }
            return tCount;
        }

        public bool GetAdvPosition(Actor targetCret, ref short nX, ref short nY)
        {
            bool result = false;
            nX = MShare.MySelf.CurrX;
            nY = MShare.MySelf.CurrY;
            byte btDir = BotHelper.GetNextDirection(MShare.MySelf.CurrX, MShare.MySelf.CurrY, targetCret.CurrX, targetCret.CurrY);
            THumActor wvar1 = MShare.MySelf;
            switch (btDir)
            {
                case Direction.Up:
                    if (RandomNumber.GetInstance().Random(2) == 0)
                    {
                        nY += 2;
                        nX += 2;
                    }
                    else
                    {
                        nY += 2;
                        nX -= 2;
                    }
                    if (!PlayScene.CanWalk(nX, nY))
                    {
                        nY = (short)(wvar1.CurrY + 2);
                    }
                    break;
                case Direction.Down:
                    if (RandomNumber.GetInstance().Random(2) == 0)
                    {
                        nY -= 2;
                        nX += 2;
                    }
                    else
                    {
                        nY -= 2;
                        nX -= 2;
                    }
                    if (!PlayScene.CanWalk(nX, nY))
                    {
                        nY = (short)(wvar1.CurrY - 2);
                    }
                    break;
                case Direction.Left:
                    if (RandomNumber.GetInstance().Random(2) == 0)
                    {
                        nX += 2;
                        nY += 2;
                    }
                    else
                    {
                        nX += 2;
                        nY -= 2;
                    }
                    if (!PlayScene.CanWalk(nX, nY))
                    {
                        nX = (short)(wvar1.CurrX + 2);
                    }
                    break;
                case Direction.Right:
                    if (RandomNumber.GetInstance().Random(2) == 0)
                    {
                        nX -= 2;
                        nY += 2;
                    }
                    else
                    {
                        nX -= 2;
                        nY -= 2;
                    }
                    if (!PlayScene.CanWalk(nX, nY))
                    {
                        nX = (short)(wvar1.CurrX - 2);
                    }
                    break;
                case Direction.UpLeft:
                    if (RandomNumber.GetInstance().Random(2) == 0)
                    {
                        nX += 2;
                    }
                    else
                    {
                        nY += 2;
                        nY -= 1;
                    }
                    if (!PlayScene.CanWalk(nX, nY))
                    {
                        nX = (short)(wvar1.CurrX + 2);
                        nY = (short)(wvar1.CurrY + 2);
                    }
                    break;
                case Direction.UpRight:
                    if (RandomNumber.GetInstance().Random(2) == 0)
                    {
                        nY += 2;
                        nY -= 1;
                    }
                    else
                    {
                        nX -= 2;
                        nX++;
                    }
                    if (!PlayScene.CanWalk(nX, nY))
                    {
                        nX = (short)(wvar1.CurrX - 2);
                        nY = (short)(wvar1.CurrY + 2);
                    }
                    break;
                case Direction.DownLeft:
                    if (RandomNumber.GetInstance().Random(2) == 0)
                    {
                        nX += 2;
                    }
                    else
                    {
                        nY -= 2;
                    }
                    if (!PlayScene.CanWalk(nX, nY))
                    {
                        nX = (short)(wvar1.CurrX + 2);
                        nY = (short)(wvar1.CurrY - 2);
                    }
                    break;
                case Direction.DownRight:
                    if (RandomNumber.GetInstance().Random(2) == 0)
                    {
                        nX -= 2;
                    }
                    else
                    {
                        nY -= 2;
                    }
                    if (!PlayScene.CanWalk(nX, nY))
                    {
                        nX = (short)(wvar1.CurrX - 2);
                        nY = (short)(wvar1.CurrY - 2);
                    }
                    break;
            }
            return result;
        }

        public static void SaveWayPoint()
        {
            //int i;
            //string S;
            //FileStream ini;
            //if (MShare.g_APMapPath != null)
            //{
            //    try
            //    {
            //        ini = new FileStream(".\\Config\\" + MShare.g_sServerName + "." + MShare.g_MySelf.m_sUserName + ".WayPoint.txt");
            //        S = "";
            //        for (i = MShare.g_APMapPath.GetLowerBound(0); i <= MShare.g_APMapPath.Length; i++)
            //        {
            //            S = S + string.Format("%d,%d ", new int[] { MShare.g_APMapPath[i].X, MShare.g_APMapPath[i].X });
            //        }
            //        ini.WriteString(MShare.g_sMapTitle, "WayPoint", S);
            //        ini.Free;
            //    }
            //    catch
            //    {
            //    }
            //}
            //else
            //{
            //    if (MShare.g_MySelf != null)
            //    {
            //        try
            //        {
            //            if (!Directory.Exists(".\\Config\\"))
            //            {
            //                Directory.CreateDirectory(".\\Config\\");
            //            }
            //            ini = new FileStream(".\\Config\\" + MShare.g_sServerName + "." + MShare.g_MySelf.m_sUserName + ".WayPoint.txt");
            //            ini.WriteString(MShare.g_sMapTitle, "WayPoint", "");
            //            ini.Free;
            //        }
            //        catch
            //        {
            //        }
            //    }
            //}
        }

        public static void LoadWayPoint()
        {
            //string X;
            //string Y;
            //string S;
            //string ss;
            //FileStream ini;
            //MShare.g_APMapPath = null;
            //ini = new FileStream(".\\Config\\" + MShare.g_sServerName + "." + MShare.g_MySelf.m_sUserName + ".WayPoint.txt");
            //S = ini.ReadString(MShare.g_sMapTitle, "WayPoint", "");
            //while (true)
            //{
            //    if (S == "")
            //    {
            //        break;
            //    }
            //    S = HUtil32.GetValidStr3(S, ref ss, new string[] { " " });
            //    if (ss != "")
            //    {
            //        Y = HUtil32.GetValidStr3(ss, ref X, new string[] { "," });
            //        if (MShare.g_APMapPath == null)
            //        {
            //            MShare.g_APMapPath = new Point[1];
            //            MShare.g_APMapPath[0].X = Convert.ToInt32(X);
            //            MShare.g_APMapPath[0].X = Convert.ToInt32(Y);
            //        }
            //        else
            //        {
            //            MShare.g_APMapPath = new Point[MShare.g_APMapPath.Length + 2];
            //            MShare.g_APMapPath[MShare.g_APMapPath.Length].X = Convert.ToInt32(X);
            //            MShare.g_APMapPath[MShare.g_APMapPath.Length].X = Convert.ToInt32(Y);
            //        }
            //    }
            //}
        }

        public int GetMagicLv(Actor actor, int magid)
        {
            if (actor == null)
            {
                return 0;
            }
            if ((magid <= 0) || (magid >= 255))
            {
                return 0;
            }
            return MShare.MagicArr[magid] != null ? MShare.MagicArr[magid].Level : 0;
        }

        private void MainOutMessage(string msg)
        {
            LogService.Info($"机器人:[{ChrName}] {msg}");
        }

        private void MainOutErrorMessage(string msg)
        {
            LogService.Error($"机器人:[{ChrName}] {msg}");
        }

        private void MainOutWarnMessage(string msg)
        {
            LogService.Warn($"机器人:[{ChrName}] {msg}");
        }
    }
}
