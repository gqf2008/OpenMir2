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
        /// M3 预建角夹具（卡 openmir2-m3-attack-pair-fixture；白名单 T-29 扩展）。
        /// 默认 false = 原行为（建角后继续选角/进世界）。置 true 时，本假人在收到「已建新角色」的
        /// 角色列表后**不再发 CM_SELCHR**、停在选角界面（见 SelectChrScene.ClientGetReceiveChrs）。
        /// 用途：T-14/②③ 的**预热相**只建号；若预热假人进了世界，`run_bots` 的 Stop-Process 会被
        /// GameSvr 当成真实下线，走 `WorldServer.SaveHumanRcd`（异步，WorldServer.cs:1077-1091）把预热
        /// 末态写回、覆盖随后的 SQL pin ⇒ 这就是「首轮只 pin 1/2、12 轮从未到 attempts==2」的根因（竞态，
        /// 不是实现缺口）。默认关时 BotSrv 逐字节不变。
        /// </summary>
        public bool StopAfterCharCreated { get; set; }

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

        /// <summary>
        /// M3②③ 真机同种子对拍夹具（卡 openmir2-m3-attack-pair-fixture；白名单 T-22 扩展）。
        /// 默认 false = 原行为（不新增任何发送）。置 true 时，**assigned attacker**（`AttackRole`
        /// 指定的账号序号）在进世界后每隔 `AttackStepMs` 发一条 `CM_HIT`（Recog=MakeLong(自己的
        /// CurrX,CurrY)，Tag=`AttackDir`），共 `AttackCount` 步；受害者侧只收 `SM_STRUCK`。
        /// 每步/每帧落 `M3ATTSEQ {json}` 标记（供 attack_diff.py 对齐）。
        /// </summary>
        public bool AttackSequence { get; set; }
        /// <summary>仅 AttackSequence=true 时生效：攻击步数。默认 120（DoD ≥100）。</summary>
        public int AttackCount { get; set; } = 120;
        /// <summary>仅 AttackSequence=true 时生效：每步间隔（ms）。默认 500。</summary>
        public int AttackStepMs { get; set; } = 500;
        /// <summary>仅 AttackSequence=true 时生效：进世界到第一步的额外等待（ms）。默认 3000。</summary>
        public int AttackStartDelayMs { get; set; } = 3000;
        /// <summary>仅 AttackSequence=true 时生效：CM_HIT 的 Tag（方向）。默认 2=RIGHT（victim 在右邻格）。</summary>
        public int AttackDir { get; set; } = 2;
        /// <summary>仅 AttackSequence=true 时生效：哪个账号序号当 attacker（默认 0；其余只当 victim）。</summary>
        public int AttackRole { get; set; } = 0;

        /// <summary>
        /// M3⑤ 真机对拍夹具（卡 openmir2-m3-shop-fixture-botsrv；白名单 T-29 家族扩展）。
        /// 默认 false = 原行为（不新增任何发送）。置 true 时，本假人在收到自己的 SM_LOGON 后按
        /// `ShopOps` 给出的固定顺序对**同一个 NPC/商人**各发一条动作（开面板/买/卖/修理/存取），
        /// 逐 op 落 `M3SHOPSEQ {json}` 标记（send/frame/end），供 `npc_shop_diff.py` 对齐。
        /// 只作夹具驱动、只写本假人自己的 socket；默认关时 BotSrv 行为逐字节不变。
        /// </summary>
        public bool ShopSequence { get; set; }
        /// <summary>仅 ShopSequence=true 时生效：每步间隔（ms）。默认 1000。</summary>
        public int ShopStepMs { get; set; } = 1000;
        /// <summary>仅 ShopSequence=true 时生效：进世界到第一步的额外等待（ms）。默认 3000。</summary>
        public int ShopStartDelayMs { get; set; } = 3000;
        /// <summary>
        /// 仅 ShopSequence=true 时生效：逗号分隔的 op 序列。
        /// 允许值：sell / buy / query_sell_price / query_repair_cost / repair / storage / takeback。
        /// 顺序即发信顺序（每条 op = 一次交互）。
        /// </summary>
        public string ShopOps { get; set; } = "sell,buy,query_sell_price,query_repair_cost,repair,storage,takeback";
        /// <summary>仅 ShopSequence=true 时生效：目标 NPC 名（UserName；空 = 视图里第一个 NPC）。</summary>
        public string ShopNpcName { get; set; } = "";
        /// <summary>仅 ShopSequence=true 时生效：动作物品名（空 = 本假人背包第一件物品）。</summary>
        public string ShopItemName { get; set; } = "";
        /// <summary>仅 ShopSequence=true 时生效：动作物品 MakeIndex（0 = 只按名字取）。</summary>
        public int ShopItemMake { get; set; } = 0;
        /// <summary>仅 ShopSequence=true 时生效：卖/买/存取的数量（默认 1）。</summary>
        public int ShopCount { get; set; } = 1;

        /// <summary>
        /// M1③ 退出路径夹具（卡 openmir2-m1-dbsrv-6000-face；白名单 M1-EXIT）：默认 0 = 关
        /// （不新增任何发送）。&gt;0 时，本假人收到自己的 SM_LOGON（真进世界）后等这么久，
        /// 按 C# 客户端 AppLogout 的线上形状发一条 CM_SOFTCLOSE(1009)（= “小退”，服务端据此
        /// 走 PlayObject.BoSoftClose → MakeGhost → SaveHumanRcd 的存档路径）。只写本假人自己的
        /// socket，不读不写跨假人共享世界态（同 T-22 纪律）。见 RobotPlayer.SoftCloseTick。
        /// </summary>
        public int SoftCloseAfterWorldMs { get; set; }

        /// <summary>
        /// M1③ 退出路径夹具（白名单 M1-EXIT）：默认 false = 原行为（假人进世界后自动挂机游走）。
        /// true 时关掉自动挂机——挂机走位由客户端无种子 RNG 驱动，跨侧对拍不可复现；
        /// 退出路径的存档判据只关心“进世界 + 不动的初始状态”。见 ClientManager.RunAutoPlay。
        /// </summary>
        public bool DisableAutoPlay { get; set; }
    }
}