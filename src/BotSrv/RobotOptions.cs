namespace BotSrv
{
    public class RobotOptions
    {
        /// <summary>
        /// 服务器名称
        /// </summary>
        public string ServerName { get; set; }
        /// <summary>
        /// 游戏服务器IP地址
        /// </summary>
        public string Address { get; set; }
        /// <summary>
        /// 服务器端口号
        /// </summary>
        public int Port { get; set; }
        /// <summary>
        /// 账号前缀
        /// </summary>
        public string LoginAccount { get; set; }
        /// <summary>
        /// 同时登录人数
        /// </summary>
        public int ChrCount { get; set; }
        /// <summary>
        /// 登录总人数
        /// </summary>
        public int TotalChrCount { get; set; }
        /// <summary>
        /// 是否创建帐号
        /// </summary>
        public bool NewAccount { get; set; }

        /// <summary>
        /// 同一批假人的连接错峰间隔（毫秒/个）。默认 3000 = 原行为（第 i 个等 i*3s）。
        /// 压测档位（200/500/1000）要"尽快全部上线再测稳态"，把它调小（例如 20~50ms）；
        /// 报告里必须记录实际用的值（上线速率是压测口径的一部分）。
        /// </summary>
        public int ConnectStaggerMs { get; set; } = 3000;

        /// <summary>
        /// 固定建角参数（M3 跨侧对拍夹具确定性）。默认 false = 原行为：
        /// Hair/Job/Sex 走 RandomNumber.GetInstance()（无种子）随机。
        /// 置 true 时改用 NewChrJob/NewChrHair/NewChrSex，使 C# 侧与 Rust 侧跑同一输入序列。
        /// </summary>
        public bool FixedNewChr { get; set; }
        /// <summary>
        /// 固定建角职业（仅 FixedNewChr=true 时生效；对应 CM_NEWCHR 的 sJob 字节）。
        /// </summary>
        public int NewChrJob { get; set; }
        /// <summary>
        /// 固定建角发型（仅 FixedNewChr=true 时生效；对应 CM_NEWCHR 的 sHair 字节）。
        /// </summary>
        public int NewChrHair { get; set; }
        /// <summary>
        /// 固定建角性别（仅 FixedNewChr=true 时生效；对应 CM_NEWCHR 的 sSex 字节）。
        /// </summary>
        public int NewChrSex { get; set; }

        /// <summary>
        /// M3 跨侧对拍夹具（卡 openmir2-m3-bag-equip-alignment 的 T-14；白名单 T-22）。
        /// 默认 false = 原行为（不新增任何发送）。
        /// 置 true 时，本假人在收到自己的 SM_LOGON（服务端确认进世界）后，按**固定顺序**
        /// 各发一条背包/装备动作：拾取(CM_PICKUP) → 装备(CM_TAKEONITEM) → 卸下(CM_TAKEOFFITEM)
        /// → 丢弃(CM_DROPITEM) → 使用药品(CM_EAT)。参数从**本假人自己的客户端视图**取，
        /// 两侧服务端行为相同时即产生**同一串 CM_* 字节**，这正是 M3「两侧同一序列」的前提。
        /// 只作夹具驱动，不进 oracle 冻结面；默认关时 BotSrv 行为逐字节不变。
        /// </summary>
        public bool BagEquipSequence { get; set; }
        /// <summary>
        /// 固定背包/装备序列每步之间的间隔（ms），仅 BagEquipSequence=true 时生效。默认 1000。
        /// 每步（含第 0 步）之间都等这么久，给服务端回包与本假人视图更新留时间。
        /// </summary>
        public int BagEquipStepMs { get; set; } = 1000;
        /// <summary>
        /// 进世界到第一步之间的额外等待（ms），仅 BagEquipSequence=true 时生效。默认 3000：
        /// SM_LOGON 之后服务端还会推物品列表（SM_ADDITEM 等），要等本假人视图装载完再动。
        /// </summary>
        public int BagEquipStartDelayMs { get; set; } = 3000;
    }
}