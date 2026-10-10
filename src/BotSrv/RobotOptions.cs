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
    }
}