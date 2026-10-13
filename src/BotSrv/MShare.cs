using BotSrv.Objects;
using OpenMir2;
using OpenMir2.Enums;
using OpenMir2.Packets.ClientPackets;
using System;
using System.Collections;
using System.Collections.Generic;
using System.Drawing;
using System.IO;
using System.Text;

namespace BotSrv
{
    public class MShare
    {
        public static string g_sGameIPaddr;
        public static int g_nGamePort;
        /// <summary>
        /// 固定建角参数（M3 夹具确定性，来自 RobotOptions；默认 false = 原随机行为）。
        /// 见 SelectChrScene.SelectChrCreateNewChr。
        /// </summary>
        public static bool g_boFixedNewChr = false;
        public static int g_nNewChrJob = 0;
        public static int g_nNewChrHair = 0;
        public static int g_nNewChrSex = 0;
        /// <summary>
        /// M3 T-14 夹具：进世界后按固定顺序发背包/装备动作（白名单 T-22）。
        /// 默认 false = 原行为（不新增任何发送）。见 RobotPlayer.BagEquipSequenceTick。
        /// </summary>
        public static bool g_boBagEquipSequence = false;
        public static int g_nBagEquipStepMs = 1000;
        public static int g_nBagEquipStartDelayMs = 3000;
        /// <summary>
        /// M3②③ 夹具：进世界后按固定攻击序列发 CM_HIT（白名单 T-22 扩展）。
        /// 默认 false = 原行为（不新增任何发送）。见 RobotPlayer.AttackSequenceTick。
        /// </summary>
        public static bool g_boAttackSequence = false;
        public static int g_nAttackCount = 120;
        public static int g_nAttackStepMs = 500;
        public static int g_nAttackStartDelayMs = 3000;
        public static int g_nAttackDir = 2;
        public static int g_nAttackRole = 0;
        /// <summary>
        /// M3②③ 夹具：attacker 的**精确** LoginId（= `LoginAccount + AttackRole`；空 = 未配置）。
        /// reviewer-5 `76e433f0` Minor 2：旧实现用 `LoginId.EndsWith(AttackRole)` 选 attacker，
        /// `-Bots&gt;10` 时序号 10/20… 也以 attacker 序号结尾 ⇒ 会被误武装成多个 attacker，
        /// 污染「第 k 次」对齐。改为与 LoginId 精确相等（见 IsAttackAttacker）。
        /// </summary>
        public static string g_sAttackRoleLoginId = string.Empty;

        /// <summary>M3②③ 夹具：LoginId 是否就是唯一 attacker。精确相等（非 EndsWith），
        /// 空配置/空入参一律 false。纯函数，供 `--selftest-attackrole` 与 ArmAttackSequence 共用。</summary>
        public static bool IsAttackAttacker(string loginId, string attackerLoginId)
        {
            return !string.IsNullOrEmpty(attackerLoginId)
                && string.Equals(loginId, attackerLoginId, StringComparison.Ordinal);
        }

        /// <summary>
        /// M3⑤ 夹具（卡 openmir2-m3-shop-fixture-botsrv；白名单 T-29 家族扩展）：
        /// 进世界后按固定顺序对同一 NPC/商人发开面板/买/卖/修理/存取动作。
        /// 默认 false = 原行为（不新增任何发送）。见 RobotPlayer.ShopSequenceTick。
        /// </summary>
        public static bool g_boShopSequence = false;
        public static int g_nShopStepMs = 1000;
        public static int g_nShopStartDelayMs = 3000;
        public static string g_sShopOps = "";
        public static string g_sShopNpcName = "";
        public static string g_sShopItemName = "";
        public static int g_nShopItemMake = 0;
        public static int g_nShopCount = 1;

        /// <summary>M3⑤ 夹具：解析逗号分隔的 op 串（去空白/小写/仅保留已知 op）。
        /// 纯函数（不依赖实例），供 `--selftest-shopseq` 与 ArmShopSequence 共用；
        /// 未知 op 一律丢弃（fail-closed：宁可少发，也不发不认识的动作）。</summary>
        public static string[] ParseShopOps(string raw)
        {
            var allowed = new List<string>
            {
                "sell", "buy", "query_sell_price", "query_repair_cost", "repair", "storage", "takeback"
            };
            var outList = new List<string>();
            if (string.IsNullOrEmpty(raw))
            {
                return outList.ToArray();
            }
            foreach (string part in raw.Split(','))
            {
                string op = part.Trim().ToLowerInvariant();
                if (op.Length == 0)
                {
                    continue;
                }
                if (allowed.Contains(op))
                {
                    outList.Add(op);
                }
            }
            return outList.ToArray();
        }

        /// <summary>M3⑤ 夹具：把本步收到的帧 JSON 串（`{"sm":..,"a":..,..}`）归类成失败分支。
        /// 纯函数（不依赖实例），供 `--selftest-shopseq` 与 RobotPlayer 共用。
        /// 口径：同一步取**第一个决定性帧**；钱不够/负重满/背包满/仓库满/取回满/无帧不可互相顶替。</summary>
        public static string ClassifyShopFrames(IList<string> frames)
        {
            if (frames == null)
            {
                return "fail_no_frame";
            }
            foreach (string fr in frames)
            {
                if (fr == null)
                {
                    continue;
                }
                if (fr.IndexOf("\"sm\":651", StringComparison.Ordinal) >= 0)
                {
                    if (fr.IndexOf("\"a\":3", StringComparison.Ordinal) >= 0) { return "fail_nomoney"; }
                    if (fr.IndexOf("\"a\":2", StringComparison.Ordinal) >= 0) { return "fail_overweight"; }
                    return "fail_buy_reject";
                }
                if (fr.IndexOf("\"sm\":649", StringComparison.Ordinal) >= 0) { return "fail_sell_reject"; }
                if (fr.IndexOf("\"sm\":670", StringComparison.Ordinal) >= 0) { return "fail_repair_reject"; }
                if (fr.IndexOf("\"sm\":702", StringComparison.Ordinal) >= 0) { return "fail_storage_full"; }
                if (fr.IndexOf("\"sm\":703", StringComparison.Ordinal) >= 0) { return "fail_storage_reject"; }
                if (fr.IndexOf("\"sm\":707", StringComparison.Ordinal) >= 0) { return "fail_bagfull"; }
                if (fr.IndexOf("\"sm\":706", StringComparison.Ordinal) >= 0) { return "fail_takeback_reject"; }
                if (fr.IndexOf("\"sm\":648", StringComparison.Ordinal) >= 0
                    || fr.IndexOf("\"sm\":650", StringComparison.Ordinal) >= 0
                    || fr.IndexOf("\"sm\":669", StringComparison.Ordinal) >= 0
                    || fr.IndexOf("\"sm\":701", StringComparison.Ordinal) >= 0
                    || fr.IndexOf("\"sm\":705", StringComparison.Ordinal) >= 0)
                {
                    return "ok";
                }
            }
            return frames.Count > 0 ? "frames_unclassified" : "fail_no_frame";
        }

        /// <summary>M3⑤ 夹具：从帧 647（卖价）/ 671（修理价）取价格。纯函数（不依赖实例）。</summary>
        public static int? ExtractShopPrice(IList<string> frames)
        {
            if (frames == null)
            {
                return null;
            }
            foreach (string fr in frames)
            {
                if (fr == null)
                {
                    continue;
                }
                if (fr.IndexOf("\"sm\":647", StringComparison.Ordinal) < 0
                    && fr.IndexOf("\"sm\":671", StringComparison.Ordinal) < 0)
                {
                    continue;
                }
                int idx = fr.IndexOf("\"a\":", StringComparison.Ordinal);
                if (idx < 0)
                {
                    continue;
                }
                int start = idx + 4;
                int end = start;
                while (end < fr.Length && (char.IsDigit(fr[end]) || fr[end] == '-'))
                {
                    end++;
                }
                if (end > start && int.TryParse(fr.Substring(start, end - start), out int val))
                {
                    return val;
                }
            }
            return null;
        }
        /// <summary>
        /// M3⑥ 夹具（卡 openmir2-m3-quest-fixture-botsrv；白名单 T-41 家族扩展）：
        /// 进世界后按固定步骤串「点任务 NPC / 选标签 / 核对说词」。
        /// 默认 false = 原行为（不新增任何发送/日志）。见 RobotPlayer.QuestSequenceTick。
        /// </summary>
        public static bool g_boQuestSequence = false;
        public static int g_nQuestStepMs = 1000;
        public static int g_nQuestStartDelayMs = 3000;
        public static string g_sQuestSteps = "";

        private static readonly string[] QuestAllowedOps =
        {
            "click_npc", "dialog_select", "take", "give", "set", "check"
        };

        /// <summary>M3⑥ 夹具：解析步骤串 `op|npc|label;op|npc|label;...`（保留顺序）。
        /// 纯函数（不依赖实例），供 `--selftest-questseq` 与 ArmQuestSequence 共用；
        /// 未知 op / 字段数 != 3 / 空 npc ⇒ 一律丢弃（fail-closed：宁可少发，也不发不认识的动作）。
        /// label 为空 ⇒ 补 `@main`（click_npc 的隐式标签）。</summary>
        public static string[] ParseQuestSteps(string raw)
        {
            var outList = new List<string>();
            if (string.IsNullOrEmpty(raw))
            {
                return outList.ToArray();
            }
            foreach (string part in raw.Split(';'))
            {
                string step = part.Trim();
                if (step.Length == 0)
                {
                    continue;
                }
                string[] f = step.Split('|');
                if (f.Length != 3)
                {
                    continue;
                }
                string op = f[0].Trim().ToLowerInvariant();
                string npc = f[1].Trim();
                string label = f[2].Trim();
                if (op.Length == 0 || npc.Length == 0)
                {
                    continue;
                }
                if (Array.IndexOf(QuestAllowedOps, op) < 0)
                {
                    continue;
                }
                if (label.Length == 0)
                {
                    label = "@main";
                }
                outList.Add(op + "|" + npc + "|" + label);
            }
            return outList.ToArray();
        }

        /// <summary>M3⑥ 夹具：把本步收到的帧 JSON 串（`{"sm":..,"text":..}` / `{"sm":..,"a":..}`）
        /// 归类成 branch。纯函数（不依赖实例），唯一口径 = 本函数。
        /// 口径：无帧 ⇒ fail_no_frame；出现 `SM_MENU_OK(767)`（脚本 MESSAGEBOX）⇒ ok_messagebox；
        /// 出现 `SM_MERCHANTSAY(643)`（脚本说词）⇒ ok_say；其余 ⇒ frames_unclassified。
        /// 失败分支（等级不足/名单已列/物品不齐）由 643 说词文本 + 帧种类序列逐字节区分（比较器负责），
        /// 本字段只作粗归类，不承载强判据。</summary>
        public static string ClassifyQuestFrames(IList<string> frames)
        {
            if (frames == null)
            {
                return "fail_no_frame";
            }
            bool sawSay = false;
            foreach (string fr in frames)
            {
                if (fr == null)
                {
                    continue;
                }
                if (fr.IndexOf("\"sm\":643", StringComparison.Ordinal) >= 0)
                {
                    sawSay = true;
                    continue;
                }
                if (fr.IndexOf("\"sm\":767", StringComparison.Ordinal) >= 0)
                {
                    return "ok_messagebox";
                }
            }
            if (sawSay)
            {
                return "ok_say";
            }
            return frames.Count > 0 ? "frames_unclassified" : "fail_no_frame";
        }

        /// <summary>M3⑥ 夹具：取第一条 `sm=643` 说词帧的 `text` 值（无 ⇒ null）。纯函数（不依赖实例）。</summary>
        public static string ExtractQuestSay(IList<string> frames)
        {
            if (frames == null)
            {
                return null;
            }
            foreach (string fr in frames)
            {
                if (fr == null)
                {
                    continue;
                }
                if (fr.IndexOf("\"sm\":643", StringComparison.Ordinal) < 0)
                {
                    continue;
                }
                int idx = fr.IndexOf("\"text\":\"", StringComparison.Ordinal);
                if (idx < 0)
                {
                    continue;
                }
                int start = idx + 8;
                var sb = new StringBuilder();
                for (int i = start; i < fr.Length; i++)
                {
                    char c = fr[i];
                    if (c == '\\' && i + 1 < fr.Length)
                    {
                        char n = fr[i + 1];
                        if (n == '"' || n == '\\') { sb.Append(n); i++; continue; }
                        if (n == 'n') { sb.Append('\n'); i++; continue; }
                        if (n == 'r') { sb.Append('\r'); i++; continue; }
                        if (n == 't') { sb.Append('\t'); i++; continue; }
                    }
                    if (c == '"') { break; }
                    sb.Append(c);
                }
                return sb.ToString();
            }
            return null;
        }

        /// <summary>
        /// M3⑦ 夹具（卡 openmir2-m3-party-trade-fixture-botsrv；白名单 T-43 家族扩展）：
        /// 两个假人按固定步骤串发组队/交易动作。
        /// 默认 false = 原行为（不新增任何发送/日志）。见 RobotPlayer.PairTradeSequenceTick。
        /// </summary>
        public static bool g_boPairTradeSequence = false;
        public static int g_nPairTradeStepMs = 1000;
        public static int g_nPairTradeStartDelayMs = 3000;
        public static string g_sPairTradeSteps = "";
        public static int g_nPairTradeRole = 0;
        public static int g_nPairTradePeerRole = 1;
        /// <summary>actor 的精确 LoginId（= LoginAccount + PairTradeRole）。</summary>
        public static string g_sPairTradeActorLoginId = string.Empty;
        /// <summary>peer 的精确 LoginId（= LoginAccount + PairTradePeerRole），也是其角色名。</summary>
        public static string g_sPairTradePeerLoginId = string.Empty;

        private static readonly string[] PairTradeAllowedOps =
        {
            // 组队（PlayObject.Message.cs:914-935 / PlayObject.Operate.cs:787-909）
            "group_mode_on", "group_mode_off", "create_group", "add_member", "del_member",
            // 交易（PlayObject.Message.cs:946-960 / PlayObject.Operate.cs:911-1203）
            "deal_try", "deal_add_item", "deal_del_item", "deal_chg_gold", "deal_end", "deal_cancel",
            // 掉线中断（CM_SOFTCLOSE(1009)）
            "softclose"
        };

        private static readonly string[] PairTradeAllowedRoles = { "a", "b", "x" };

        /// <summary>M3⑦ 夹具：解析步骤串 `role|op|param;...`（保留顺序）。
        /// 纯函数（不依赖实例），供 `--selftest-pairtrade` 与 ArmPairTradeSequence 共用；
        /// 字段数 != 3 / 未知 role / 未知 op / 空角色 ⇒ 一律丢弃（fail-closed：宁可少发，也不发不认识的动作）。
        /// param 可为空（空 = 无参）。规范化输出 `role|op|param`。</summary>
        public static string[] ParsePairTradeSteps(string raw)
        {
            var outList = new List<string>();
            if (string.IsNullOrEmpty(raw))
            {
                return outList.ToArray();
            }
            foreach (string part in raw.Split(';'))
            {
                string step = part.Trim();
                if (step.Length == 0)
                {
                    continue;
                }
                string[] f = step.Split('|');
                if (f.Length != 3)
                {
                    continue;
                }
                string role = f[0].Trim().ToLowerInvariant();
                string op = f[1].Trim().ToLowerInvariant();
                string param = f[2].Trim();
                if (role.Length == 0 || op.Length == 0)
                {
                    continue;
                }
                if (Array.IndexOf(PairTradeAllowedRoles, role) < 0)
                {
                    continue;
                }
                if (Array.IndexOf(PairTradeAllowedOps, op) < 0)
                {
                    continue;
                }
                outList.Add(role + "|" + op + "|" + param);
            }
            return outList.ToArray();
        }

        /// <summary>M3⑦ 夹具：LoginId 是否就是本批的唯一 actor。精确相等（非 EndsWith）。纯函数。</summary>
        public static bool IsPairTradeActor(string loginId, string actorLoginId)
        {
            return !string.IsNullOrEmpty(actorLoginId)
                && string.Equals(loginId, actorLoginId, StringComparison.Ordinal);
        }

        /// <summary>M3⑦ 夹具：步骤 role（a/b/x；非法 ⇒ 空串）。纯函数。</summary>
        public static string PairTradeStepRole(string step)
        {
            if (string.IsNullOrEmpty(step)) { return string.Empty; }
            string[] f = step.Split('|');
            return f.Length == 3 ? f[0] : string.Empty;
        }

        /// <summary>M3⑦ 夹具：步骤 op（非法 ⇒ 空串）。纯函数。</summary>
        public static string PairTradeStepOp(string step)
        {
            if (string.IsNullOrEmpty(step)) { return string.Empty; }
            string[] f = step.Split('|');
            return f.Length == 3 ? f[1] : string.Empty;
        }

        /// <summary>M3⑦ 夹具：步骤 param（非法 ⇒ 空串）。纯函数。</summary>
        public static string PairTradeStepParam(string step)
        {
            if (string.IsNullOrEmpty(step)) { return string.Empty; }
            string[] f = step.Split('|');
            return f.Length == 3 ? f[2] : string.Empty;
        }

        /// <summary>M3⑦ 夹具：本步是否由当前角色执行（x=两者 / a=仅 actor / b=仅 peer；非法 role ⇒ false）。纯函数。</summary>
        public static bool PairTradeRunsForRole(string role, bool isActor)
        {
            if (role == "x") { return true; }
            if (role == "a") { return isActor; }
            if (role == "b") { return !isActor; }
            return false;
        }

        private static int? PairTradeFrameSm(string fr)
        {
            if (fr == null) { return null; }
            int idx = fr.IndexOf("\"sm\":", StringComparison.Ordinal);
            if (idx < 0) { return null; }
            int start = idx + 5;
            int end = start;
            while (end < fr.Length && (char.IsDigit(fr[end]) || fr[end] == '-')) { end++; }
            if (end <= start) { return null; }
            return int.TryParse(fr.Substring(start, end - start), out int v) ? v : (int?)null;
        }

        /// <summary>M3⑦ 夹具：把本步收到的帧 JSON 串归类成 branch。纯函数，唯一口径 = 本函数。
        /// 口径：无帧 ⇒ fail_no_frame；出现任一失败帧 ⇒ fail_reject；出现任一成功/信息帧 ⇒ ok；
        /// 其余 ⇒ frames_unclassified。失败码由完整帧种类序列（比较器）承载，本字段只作粗归类，不承载强判据。
        /// （按 reviewer 在 ⑤-6 M-1 的提醒：不靠 JSON 子串定位字段，统一解析 `"sm":` 数字。）</summary>
        public static string ClassifyPairTradeFrames(IList<string> frames)
        {
            if (frames == null)
            {
                return "fail_no_frame";
            }
            bool sawAny = false;
            bool sawFail = false;
            bool sawOk = false;
            foreach (string fr in frames)
            {
                if (fr == null) { continue; }
                int? sm = PairTradeFrameSm(fr);
                if (sm == null) { continue; }
                sawAny = true;
                switch (sm.Value)
                {
                    case 659: // SM_GROUPMODECHANGED
                    case 660: // SM_CREATEGROUP_OK
                    case 662: // SM_GROUPADDMEM_OK
                    case 663: // SM_GROUPDELMEM_OK
                    case 666: // SM_GROUPCANCEL
                    case 667: // SM_GROUPMEMBERS
                    case 673: // SM_DEALMENU
                    case 675: // SM_DEALADDITEM_OK
                    case 677: // SM_DEALDELITEM_OK
                    case 681: // SM_DEALCANCEL
                    case 682: // SM_DEALREMOTEADDITEM
                    case 683: // SM_DEALREMOTEDELITEM
                    case 684: // SM_DEALCHGGOLD_OK
                    case 686: // SM_DEALREMOTECHGGOLD
                    case 687: // SM_DEALSUCCESS
                    case 101: // SM_GROUPMESSAGE（组队广播文本）
                        sawOk = true;
                        break;
                    case 661: // SM_CREATEGROUP_FAIL
                    case 664: // SM_GROUPADDMEM_FAIL
                    case 665: // SM_GROUPDELMEM_FAIL
                    case 674: // SM_DEALTRY_FAIL
                    case 676: // SM_DEALADDITEM_FAIL
                    case 678: // SM_DEALDELITEM_FAIL
                    case 685: // SM_DEALCHGGOLD_FAIL
                        sawFail = true;
                        break;
                    default:
                        break;
                }
            }
            if (sawFail) { return "fail_reject"; }
            if (sawOk) { return "ok"; }
            return sawAny ? "frames_unclassified" : "fail_no_frame";
        }

        /// <summary>
        /// M1③ 退出路径夹具（卡 openmir2-m1-dbsrv-6000-face；白名单 M1-EXIT）：
        /// 收到本假人 SM_LOGON 后等这么久，按客户端 AppLogout 的线上形状发一条
        /// CM_SOFTCLOSE(1009)（= “小退”）。默认 0 = 关（不新增任何发送）。
        /// 见 RobotPlayer.SoftCloseTick。
        /// </summary>
        public static int g_nSoftCloseAfterWorldMs = 0;
        /// <summary>
        /// M1③ 退出路径夹具：关掉自动挂机（默认 false = 原行为）。
        /// 挂机会让假人随机游走（未种子的客户端 RNG）⇒ 存档位置不可复现；跨侧对拍需要“静止”。
        /// 见 ClientManager.RunAutoPlay。
        /// </summary>
        public static bool g_boDisableAutoPlay = false;
        /// <summary>
        /// M3 预建角夹具（白名单 T-29 扩展）：建角后停在选角界面、不发 CM_SELCHR（默认 false = 原行为）。
        /// 见 SelectChrScene.ClientGetReceiveChrs；根因见 RobotOptions.StopAfterCharCreated。
        /// </summary>
        public static bool g_boStopAfterCharCreated = false;
        /// <summary>
        /// 是否自动捡取物品
        /// </summary>
        public static bool AutoPickUp = true;
        /// <summary>
        /// 是否自动捡取所有物品
        /// </summary>
        public static bool PickUpAll = false;
        public static TDropItem AutoPicupItem = null;
        public static TMovingItem[] g_TIItems = new TMovingItem[2];
        public static TMovingItem[] g_spItems = new TMovingItem[2];
        public static int g_IPointLessHintTick = 0;
        public static int g_MPLessHintTick = 0;
        public static int g_dwAutoTecTick = 0;
        public static int g_rtime = 0;
        public static bool g_boExchgPoison = false;
        public static bool g_boCheckTakeOffPoison = false;
        public static bool g_boTakeOnPos = true;
        public static bool g_boQueryDynCode = false;
        public static bool g_boQuerySelChar = false;
        public static int g_dwCheckCount = 0;
        public static string g_sBookLabel = "";
        public static int g_MaxExpFilter = 2000;
        public static bool g_boQueryExit = false;
        public static string SelChrAddr = string.Empty;
        public static int SelChrPort = 0;
        public static string RunServerAddr = string.Empty;
        public static int RunServerPort = 0;
        public static bool SendLogin = false;
        public static bool ServerConnected = false;
        public static bool g_SoftClosed = false;
        public static PlayerAction PlayerAction;
        public static ConnectionStep ConnectionStep;
        public static string MapTitle = string.Empty;
        public static int LastMapMusic = -1;
        public static ArrayList g_SendSayList = null;
        public static int g_SendSayListIdx = 0;
        public static IList<string> g_GroupMembers = null;
        public static ClientMagic[] MagicArr = new ClientMagic[255];
        public static IList<ClientMagic> g_MagicList = null;
        public static IList<TDropItem> g_DropedItemList = null;
        public static ArrayList g_ChangeFaceReadyList = null;
        public static IList<Actor> g_FreeActorList = null;
        public static NakedAbility g_BonusTick = default;
        public static NakedAbility g_BonusAbil = default;
        public static NakedAbility g_NakedAbil = default;
        public static NakedAbility g_BonusAbilChg = default;
        public static string g_sGuildName = string.Empty;
        public static string g_sGuildRankName = string.Empty;
        /// <summary>
        /// 最后魔法攻击时间
        /// </summary>
        public static int LatestJoinAttackTick = 0;
        /// <summary>
        /// 最后攻击时间(包括物理攻击及魔法攻击)
        /// </summary>
        public static int LastAttackTick = 0;
        /// <summary>
        /// 最后移动时间
        /// </summary>        
        public static int LastMoveTick = 0;
        /// <summary>
        /// 最后魔法攻击时间
        /// </summary>        
        public static int LatestSpellTick = 0;
        /// <summary>
        /// 最后列火攻击时间
        /// </summary>        
        public static int LatestFireHitTick = 0;
        /// <summary>
        /// 最后列火攻击时间
        /// </summary>        
        public static int LatestSLonHitTick = 0;
        public static int g_dwLatestTwinHitTick = 0;
        public static int g_dwLatestPursueHitTick = 0;
        public static int g_dwLatestRushHitTick = 0;
        public static int g_dwLatestSmiteHitTick = 0;
        public static int g_dwLatestSmiteLongHitTick = 0;
        public static int g_dwLatestSmiteLongHitTick2 = 0;
        public static int g_dwLatestSmiteLongHitTick3 = 0;
        public static int g_dwLatestSmiteWideHitTick = 0;
        public static int g_dwLatestSmiteWideHitTick2 = 0;
        public static int g_dwLatestRushRushTick = 0;
        public static int g_dwMagicDelayTime = 0;
        public static int g_dwMagicPKDelayTime = 0;
        public static short MouseCurrX = 0;
        public static short MouseCurrY = 0;
        public static ushort MouseX = 0;
        public static ushort MouseY = 0;
        public static short TargetX = 0;
        public static short TargetY = 0;
        public static Actor TargetCret = null;
        public static Actor FocusCret = null;
        public static Actor MagicTarget = null;
        public static MapLink g_APQueue = null;
        public static IList<FindMapNode> AutoPathList = null;
        public static ushort[,] g_APPass = null;
        public static ushort[,] g_APPassEmpty = new ushort[BotConst.MAXX * 3, BotConst.MAXY * 3];
        public static Actor AutoTagget = null;
        public static int g_APRunTick = 0;
        public static int g_APRunTick2 = 0;
        public static int g_nAPStatus = 0;
        public static bool AutoMove = false;
        public static int m_dwSpellTick = 0;
        public static int m_dwRecallTick = 0;
        public static int m_dwDoubluSCTick = 0;
        public static int m_dwPoisonTick = 0;
        public static int m_btMagPassTh = 0;
        public static int g_nTagCount = 0;
        public static int m_dwTargetFocusTick = 0;
        public static Dictionary<string, string> g_APPickUpList = null;
        /// <summary>
        /// 忽略攻击的怪物列表
        /// </summary>
        public static Dictionary<string, string> IgnoreMobList = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        public static Actor g_AttackTarget = null;
        public static int g_dwSearchEnemyTick = 0;
        public static byte g_nAPReLogon = 0;
        public static int g_nAPReLogonWaitTick = 0;
        public static int g_nAPReLogonWaitTime = 10 * 1000;
        public static int g_ApLastSelect = 0;
        public static int g_nOverAPZone = 0;
        public static int g_nOverAPZone2 = 0;
        public static bool g_APGoBack = false;
        public static bool g_APGoBack2 = false;
        public static int AutoStep = -1;
        public static int g_APStep2 = -1;
        public static Point[] MapPath;
        public static Point[] g_APMapPath2;
        public static Point AutoLastPoint;
        public static Point AutoLastPoint2;
        public static bool g_boMapMoving = false;
        public static bool MapMovingWait = false;
        public static bool g_boCheckBadMapMode = false;
        public static bool g_boCheckSpeedHackDisplay = false;
        public static int g_nStallX = 0;
        public static int g_nStallY = 0;
        public static int g_dwChangeGroupModeTick = 0;
        public static int g_dwDealActionTick = 0;
        public static int g_dwQueryMsgTick = 0;
        public static int g_nDupSelection = 0;
        public static bool g_boAllowGroup = false;
        // 人物信息相关
        public static int g_nMySpeedPoint = 0;
        // 敏捷
        public static int g_nMyHitPoint = 0;
        // 准确
        public static int g_nMyAntiPoison = 0;
        // 魔法躲避
        public static int g_nMyPoisonRecover = 0;
        // 中毒恢复
        public static int g_nMyHealthRecover = 0;
        // 体力恢复
        public static int g_nMySpellRecover = 0;
        // 魔法恢复
        public static int g_nMyAntiMagic = 0;
        // 魔法躲避
        public static int g_nMyHungryState = 0;
        // 饥饿状态
        public static int g_nMyIPowerRecover = 0;
        // 中毒恢复
        public static int g_nMyAddDamage = 0;
        public static int g_nMyDecDamage = 0;
        public static ushort g_wAvailIDDay = 0;
        public static ushort g_wAvailIDHour = 0;
        public static ushort g_wAvailIPDay = 0;
        public static ushort g_wAvailIPHour = 0;
        public static THumActor MySelf = null;
        public static int g_BuildAcusesStep = 0;
        public static TMovingItem[] g_BuildAcuses = new TMovingItem[8];
        /// <summary>
        /// 穿戴物品
        /// </summary>
        public static ClientItem[] UseItems = new ClientItem[14];
        /// <summary>
        /// 包裹物品
        /// </summary>
        public static ClientItem[] ItemArr = new ClientItem[BotConst.MaxBagItemcl];
        public static bool g_boBagLoaded = false;
        public static bool g_boServerChanging = false;
        public static int g_nTestReceiveCount = 0;
        public static int g_nSpellCount = 0;
        public static int g_nSpellFailCount = 0;
        public static int g_nFireCount = 0;
        public static ClientItem g_SellDlgItem = null;
        public static TMovingItem g_TakeBackItemWait = null;
        public static TMovingItem g_SellDlgItemSellWait = null;
        public static ClientItem g_DetectItem = null;
        public static int g_DetectItemMineID = 0;
        public static ClientItem g_DealDlgItem = null;
        public static bool g_boQueryPrice = false;
        public static int g_dwQueryPriceTime = 0;
        public static string g_sSellPriceStr = string.Empty;
        public static ClientItem[] g_DealItems = new ClientItem[10];
        public static ClientItem[] g_YbDealItems = new ClientItem[10];
        public static ClientItem[] g_DealRemoteItems = new ClientItem[20];
        public static int g_nDealGold = 0;
        public static int g_nDealRemoteGold = 0;
        public static string g_sDealWho = string.Empty;
        public static ClientItem g_MouseItem = null;
        public static ClientItem g_MouseStateItem = null;
        public static ClientItem g_MouseUserStateItem = null;
        public static ClientItem g_EatingItem = new ClientItem();
        public static bool g_boItemMoving = false;
        public static TMovingItem MovingItem = null;
        public static TMovingItem OpenBoxItem = null;
        public static TMovingItem g_WaitingUseItem = new TMovingItem { Item = new ClientItem() };
        public static TMovingItem g_WaitingStallItem = null;
        public static TMovingItem g_WaitingDetectItem = null;
        public static TDropItem g_FocusItem = null;
        public static TDropItem g_FocusItem2 = null;
        public static bool g_boOpenStallSystem = true;
        public static bool g_boAutoLongAttack = true;
        public static bool g_boAutoSay = true;
        public static bool g_boMutiHero = true;
        public static bool g_boSkill_114_MP = false;
        public static bool g_boSkill_68_MP = false;
        public static int g_nAreaStateValue = 0;
        public static int g_nRunReadyCount = 0;
        public static bool g_boForceNotViewFog = true;
        public static int g_dwEatTime = 0;
        public static int g_dwDizzyDelayStart = 0;
        public static int g_dwDizzyDelayTime = 0;
        public static bool g_boAutoDig = false;
        public static bool g_boAutoSit = false;
        public static int g_dwFirstServerTime = 0;
        public static int g_dwFirstClientTime = 0;
        public static int g_nTimeFakeDetectCount = 0;
        public static int g_dwLatestClientTime2 = 0;
        public static int g_dwFirstClientTimerTime = 0;
        public static int g_dwLatestClientTimerTime = 0;
        public static int g_dwFirstClientGetTime = 0;
        public static int g_dwLatestClientGetTime = 0;
        public static int g_nTimeFakeDetectSum = 0;
        public static int g_nTimeFakeDetectTimer = 0;
        public static int g_dwLastestClientGetTime = 0;
        /// <summary>
        /// 地面物品闪时间间隔
        /// </summary>
        public static int DropItemFlashTime = 5 * 1000;
        /// <summary>
        /// 攻击间隔时间间隔
        /// </summary>
        public static int HitTime = 1400;
        public static int ItemSpeed = 60;
        /// <summary>
        /// 魔法攻间隔时间
        /// </summary>
        public static int SpellTime = 500;
        public static bool OpenAutoPlay = true;
        public static bool g_boClientCanSet = true;
        public static int g_nEatIteminvTime = 200;
        public static bool g_boCanRunSafeZone = true;
        public static bool g_boCanRunHuman = true;
        public static bool g_boCanRunMon = true;
        public static bool g_boCanRunNpc = true;
        public static bool g_boCanRunAllInWarZone = false;
        public static bool g_boCanStartRun = true;
        // 是否允许免助跑
        public static bool g_boParalyCanRun = false;
        // 麻痹是否可以跑
        public static bool g_boParalyCanWalk = false;
        // 麻痹是否可以走
        public static bool g_boParalyCanHit = false;
        // 麻痹是否可以攻击
        public static bool g_boParalyCanSpell = false;
        // 麻痹是否可以魔法
        public static bool g_boShowRedHPLable = true;
        // 显示血条
        public static bool g_boShowHPNumber = true;
        // 显示血量数字
        public static bool g_boShowJobLevel = true;
        // 显示职业等级
        public static bool DuraAlert = true;
        // 物品持久警告
        public static bool MagicLock = true;
        // 魔法锁定
        public static bool SpeedRate = false;
        public static bool SpeedRateShow = false;
        // 外挂功能变量结束
        public static bool g_boQuickPickup = false;
        public static int AutoPickupTick = 0;
        /// <summary>
        /// 自动捡物品间隔
        /// </summary>
        public static int AutoPickupTime = 100;
        public static Actor MagicLockActor = null;
        public static bool g_boNextTimePowerHit = false;
        public static bool g_boCanLongHit = false;
        public static bool g_boCanWideHit = false;
        public static bool g_boCanCrsHit = false;
        public static bool g_boCanStnHit = false;
        public static bool g_boNextTimeFireHit = false;
        public static bool g_boNextTimeTwinHit = false;
        public static bool g_boNextTimePursueHit = false;
        public static bool g_boNextTimeRushHit = false;
        public static bool g_boNextTimeSmiteHit = false;
        public static bool g_boNextTimeSmiteLongHit = false;
        public static bool g_boNextTimeSmiteLongHit2 = false;
        public static bool g_boNextTimeSmiteLongHit3 = false;
        public static bool g_boNextTimeSmiteWideHit = false;
        public static bool g_boNextTimeSmiteWideHit2 = false;
        public static bool g_boCanSLonHit = false;
        public static bool g_boCanSquHit = false;
        public static bool[] g_gcGeneral = { true, true, false, true, true, true, false, true, false, true, true, true, true, false, false, true, true };
        public static bool[] g_gcProtect = { false, false, false, false, false, false, false, true, true, true, false, true };
        public static int[] g_gnProtectPercent = { 10, 10, 10, 10, 10, 10, 0, 88, 88, 88, 20, 00 };
        public static int[] g_gnProtectTime = { 4000, 4000, 4000, 4000, 4000, 4000, 4000, 4000, 4000, 1000, 1000, 1000 };
        public static bool[] g_gcTec = { true, true, true, true, true, true, false, false, false, false, false, false, false, true, false };
        public static int[] g_gnTecTime = { 0, 0, 0, 0, 0, 0, 0, 0, 4000, 0, 0, 0, 0, 0, 0 };
        public static bool[] g_gcAss = { false, false, false, false, false, false, true };
        public static int g_HitSpeedRate = 0;
        public static int g_MagSpeedRate = 0;
        public static int g_MoveSpeedRate = 0;

        // 得到地图文件名称自定义路径
        public static string GetMapDirAndName(string sFileName)
        {
            string result;
            if (File.Exists(BotConst.MAPDIRNAME + sFileName + ".map"))
            {
                result = BotConst.MAPDIRNAME + sFileName + ".map";
            }
            else
            {
                result = BotConst.MAPDIRNAME + sFileName + ".map";
            }
            return result;
        }

        public static int GetTickCount()
        {
            return HUtil32.GetTickCount();
        }

        public static bool IsDetectItem(int idx)
        {
            return idx == BotConst.DETECT_MIIDX_OFFSET;
        }

        public static bool IsBagItem(int idx)
        {
            return idx >= 6 && idx <= Grobal2.MaxBagItem - 1;
        }

        public static bool IsEquItem(int idx)
        {
            if (idx < 0)
            {
                int sel = -(idx + 1);
                return sel >= 0 && sel <= 13;
            }
            return false;
        }

        public static bool IsStorageItem(int idx)
        {
            return (idx >= BotConst.SAVE_MIIDX_OFFSET) && (idx < BotConst.SAVE_MIIDX_OFFSET + 46);
        }

        public static bool IsStallItem(int idx)
        {
            return (idx >= BotConst.STALL_MIIDX_OFFSET) && (idx < BotConst.STALL_MIIDX_OFFSET + 10);
        }

        public static void CheckSpeedCount(int Count)
        {
            g_dwCheckCount++;
            if (g_dwCheckCount > Count)
            {
                g_dwCheckCount = 0;
            }
        }

        public static bool IsPersentHP(int nMin, int nMax)
        {
            bool result = false;
            if (nMax != 0)
            {
                result = HUtil32.Round(nMin / nMax * 100) < g_gnProtectPercent[0];
            }
            return result;
        }

        public static bool IsPersentMP(int nMin, int nMax)
        {
            bool result = false;
            if (nMax != 0)
            {
                result = HUtil32.Round(nMin / nMax * 100) < g_gnProtectPercent[1];
            }
            return result;
        }

        public static bool IsPersentSpc(int nMin, int nMax)
        {
            bool result = false;
            if (nMax != 0)
            {
                result = HUtil32.Round(nMin / nMax * 100) < g_gnProtectPercent[3];
            }
            return result;
        }

        public static bool IsPersentBook(int nMin, int nMax)
        {
            bool result = false;
            if (nMax != 0)
            {
                result = HUtil32.Round(nMin / nMax * 100) < g_gnProtectPercent[5];
            }
            return result;
        }

        public static string GetJobName(int nJob)
        {
            string result;
            switch (nJob)
            {
                case 0:
                    result = BotConst.WarriorName;
                    break;
                case 1:
                    result = BotConst.WizardName;
                    break;
                case 2:
                    result = BotConst.TaoistName;
                    break;
                default:
                    result = BotConst.UnKnowName;
                    break;
            }
            return result;
        }

        public static string GetItemType(TItemType ItemType)
        {
            string result = string.Empty;
            switch (ItemType)
            {
                case TItemType.i_HPDurg:
                    result = "金创药";
                    break;
                case TItemType.i_MPDurg:
                    result = "魔法药";
                    break;
                case TItemType.i_HPMPDurg:
                    result = "高级药";
                    break;
                case TItemType.i_OtherDurg:
                    result = "其它药品";
                    break;
                case TItemType.i_Weapon:
                    result = "武器";
                    break;
                case TItemType.i_Dress:
                    result = "衣服";
                    break;
                case TItemType.i_Helmet:
                    result = "头盔";
                    break;
                case TItemType.i_Necklace:
                    result = "项链";
                    break;
                case TItemType.i_Armring:
                    result = "手镯";
                    break;
                case TItemType.i_Ring:
                    result = "戒指";
                    break;
                case TItemType.i_Belt:
                    result = "腰带";
                    break;
                case TItemType.i_Boots:
                    result = "鞋子";
                    break;
                case TItemType.i_Charm:
                    result = "宝石";
                    break;
                case TItemType.i_Book:
                    result = "技能书";
                    break;
                case TItemType.i_PosionDurg:
                    result = "毒药";
                    break;
                case TItemType.i_UseItem:
                    result = "消耗品";
                    break;
                case TItemType.i_Scroll:
                    result = "卷类";
                    break;
                case TItemType.i_Stone:
                    result = "矿石";
                    break;
                case TItemType.i_Gold:
                    result = "金币";
                    break;
                case TItemType.i_Other:
                    result = "其它";
                    break;
            }
            return result;
        }

        public static bool GetItemShowFilter(string sItemName)
        {
            return BotConst.g_ShowItemList.ContainsKey(sItemName);
        }

        public static void LoadUserConfig(string sUserName)
        {
            //FileStream ini;
            //FileStream ini2;
            //string sFileName;
            //ArrayList Strings;
            //int i;
            //int no;
            //string sn;
            //string so;
            //sFileName = ".\\Config\\" + g_sServerName + "." + sUserName + ".Set";
            //ini = new FileStream(sFileName);
            //// base
            //g_gcGeneral[0] = ini.ReadBool("Basic", "ShowActorName", g_gcGeneral[0]);
            //g_gcGeneral[1] = ini.ReadBool("Basic", "DuraWarning", g_gcGeneral[1]);
            //g_gcGeneral[2] = ini.ReadBool("Basic", "AutoAttack", g_gcGeneral[2]);
            //g_gcGeneral[3] = ini.ReadBool("Basic", "ShowExpFilter", g_gcGeneral[3]);
            //g_MaxExpFilter = ini.ReadInteger("Basic", "ShowExpFilterMax", g_MaxExpFilter);
            //g_gcGeneral[4] = ini.ReadBool("Basic", "ShowDropItems", g_gcGeneral[4]);
            //g_gcGeneral[5] = ini.ReadBool("Basic", "ShowDropItemsFilter", g_gcGeneral[5]);
            //g_gcGeneral[6] = ini.ReadBool("Basic", "ShowHumanWing", g_gcGeneral[6]);
            //g_boAutoPickUp = ini.ReadBool("Basic", "AutoPickUp", g_boAutoPickUp);
            //g_gcGeneral[7] = ini.ReadBool("Basic", "AutoPickUpFilter", g_gcGeneral[7]);
            //g_boPickUpAll = ini.ReadBool("Basic", "PickUpAllItem", g_boPickUpAll);
            //g_gcGeneral[8] = ini.ReadBool("Basic", "HideDeathBody", g_gcGeneral[8]);
            //g_gcGeneral[9] = ini.ReadBool("Basic", "AutoFixItem", g_gcGeneral[9]);
            //g_gcGeneral[10] = ini.ReadBool("Basic", "ShakeScreen", g_gcGeneral[10]);
            //g_gcGeneral[13] = ini.ReadBool("Basic", "StruckShow", g_gcGeneral[13]);
            //g_gcGeneral[15] = ini.ReadBool("Basic", "HideStruck", g_gcGeneral[15]);
            //g_gcGeneral[14] = ini.ReadBool("Basic", "CompareItems", g_gcGeneral[14]);
            //ini2 = new FileStream(".\\lscfg.ini");
            //g_gcGeneral[11] = ini2.ReadBool("Setup", "EffectSound", g_gcGeneral[11]);
            //g_gcGeneral[12] = ini2.ReadBool("Setup", "EffectBKGSound", g_gcGeneral[12]);
            //g_lWavMaxVol = ini2.ReadInteger("Setup", "EffectSoundLevel", g_lWavMaxVol);
            //ini2.Free;
            //g_HitSpeedRate = ini.ReadInteger("Basic", "HitSpeedRate", g_HitSpeedRate);
            //g_MagSpeedRate = ini.ReadInteger("Basic", "MagSpeedRate", g_MagSpeedRate);
            //g_MoveSpeedRate = ini.ReadInteger("Basic", "MoveSpeedRate", g_MoveSpeedRate);
            //// Protect
            //g_gcProtect[0] = ini.ReadBool("Protect", "RenewHPIsAuto", g_gcProtect[0]);
            //g_gcProtect[1] = ini.ReadBool("Protect", "RenewMPIsAuto", g_gcProtect[1]);
            //g_gcProtect[3] = ini.ReadBool("Protect", "RenewSpecialIsAuto", g_gcProtect[3]);
            //g_gcProtect[5] = ini.ReadBool("Protect", "RenewBookIsAuto", g_gcProtect[5]);
            //g_gcProtect[7] = ini.ReadBool("Protect", "HeroRenewHPIsAuto", g_gcProtect[7]);
            //g_gcProtect[8] = ini.ReadBool("Protect", "HeroRenewMPIsAuto", g_gcProtect[8]);
            //g_gcProtect[9] = ini.ReadBool("Protect", "HeroRenewSpecialIsAuto", g_gcProtect[9]);
            //g_gcProtect[10] = ini.ReadBool("Protect", "HeroSidestep", g_gcProtect[10]);
            //g_gcProtect[11] = ini.ReadBool("Protect", "RenewSpecialIsAuto_MP", g_gcProtect[11]);
            //g_gnProtectTime[0] = ini.ReadInteger("Protect", "RenewHPTime", g_gnProtectTime[0]);
            //g_gnProtectTime[1] = ini.ReadInteger("Protect", "RenewMPTime", g_gnProtectTime[1]);
            //g_gnProtectTime[3] = ini.ReadInteger("Protect", "RenewSpecialTime", g_gnProtectTime[3]);
            //g_gnProtectTime[5] = ini.ReadInteger("Protect", "RenewBookTime", g_gnProtectTime[5]);
            //g_gnProtectTime[7] = ini.ReadInteger("Protect", "HeroRenewHPTime", g_gnProtectTime[7]);
            //g_gnProtectTime[8] = ini.ReadInteger("Protect", "HeroRenewMPTime", g_gnProtectTime[8]);
            //g_gnProtectTime[9] = ini.ReadInteger("Protect", "HeroRenewSpecialTime", g_gnProtectTime[9]);
            //g_gnProtectPercent[0] = ini.ReadInteger("Protect", "RenewHPPercent", g_gnProtectPercent[0]);
            //g_gnProtectPercent[1] = ini.ReadInteger("Protect", "RenewMPPercent", g_gnProtectPercent[1]);
            //g_gnProtectPercent[3] = ini.ReadInteger("Protect", "RenewSpecialPercent", g_gnProtectPercent[3]);
            //g_gnProtectPercent[7] = ini.ReadInteger("Protect", "HeroRenewHPPercent", g_gnProtectPercent[7]);
            //g_gnProtectPercent[8] = ini.ReadInteger("Protect", "HeroRenewMPPercent", g_gnProtectPercent[8]);
            //g_gnProtectPercent[9] = ini.ReadInteger("Protect", "HeroRenewSpecialPercent", g_gnProtectPercent[9]);
            //g_gnProtectPercent[10] = ini.ReadInteger("Protect", "HeroPerSidestep", g_gnProtectPercent[10]);
            //g_gnProtectPercent[5] = ini.ReadInteger("Protect", "RenewBookPercent", g_gnProtectPercent[5]);
            //g_gnProtectPercent[6] = ini.ReadInteger("Protect", "RenewBookNowBookIndex", g_gnProtectPercent[6]);
            //ClMain.frmMain.SendClientMessage(Messages.CM_HEROSIDESTEP, HUtil32.MakeLong(((int)g_gcProtect[10]), g_gnProtectPercent[10]), 0, 0, 0);
            //g_gcTec[0] = ini.ReadBool("Tec", "SmartLongHit", g_gcTec[0]);
            //g_gcTec[10] = ini.ReadBool("Tec", "SmartLongHit2", g_gcTec[10]);
            //g_gcTec[11] = ini.ReadBool("Tec", "SmartSLongHit", g_gcTec[11]);
            //g_gcTec[1] = ini.ReadBool("Tec", "SmartWideHit", g_gcTec[1]);
            //g_gcTec[2] = ini.ReadBool("Tec", "SmartFireHit", g_gcTec[2]);
            //g_gcTec[3] = ini.ReadBool("Tec", "SmartPureHit", g_gcTec[3]);
            //g_gcTec[4] = ini.ReadBool("Tec", "SmartShield", g_gcTec[4]);
            //g_gcTec[5] = ini.ReadBool("Tec", "SmartShieldHero", g_gcTec[5]);
            //g_gcTec[6] = ini.ReadBool("Tec", "SmartTransparence", g_gcTec[6]);
            //g_gcTec[9] = ini.ReadBool("Tec", "SmartThunderHit", g_gcTec[9]);
            //g_gcTec[7] = ini.ReadBool("AutoPractice", "PracticeIsAuto", g_gcTec[7]);
            //g_gnTecTime[8] = ini.ReadInteger("AutoPractice", "PracticeTime", g_gnTecTime[8]);
            //g_gnTecPracticeKey = ini.ReadInteger("AutoPractice", "PracticeKey", g_gnTecPracticeKey);
            //g_gcTec[12] = ini.ReadBool("Tec", "HeroSeriesSkillFilter", g_gcTec[12]);
            //g_gcTec[13] = ini.ReadBool("Tec", "SLongHit", g_gcTec[13]);
            //g_gcTec[14] = ini.ReadBool("Tec", "SmartGoMagic", g_gcTec[14]);
            //ClMain.frmMain.SendClientMessage(Messages.CM_HEROSERIESSKILLCONFIG, HUtil32.MakeLong(((int)g_gcTec[12]), 0), 0, 0, 0);
            //g_gcHotkey[0] = ini.ReadBool("Hotkey", "UseHotkey", g_gcHotkey[0]);
            //FrmDlg.DEHeroCallHero.SetOfHotKey(ini.ReadInteger("Hotkey", "HeroCallHero", 0));
            //FrmDlg.DEHeroSetAttackState.SetOfHotKey(ini.ReadInteger("Hotkey", "HeroSetAttackState", 0));
            //FrmDlg.DEHeroSetGuard.SetOfHotKey(ini.ReadInteger("Hotkey", "HeroSetGuard", 0));
            //FrmDlg.DEHeroSetTarget.SetOfHotKey(ini.ReadInteger("Hotkey", "HeroSetTarget", 0));
            //FrmDlg.DEHeroUnionHit.SetOfHotKey(ini.ReadInteger("Hotkey", "HeroUnionHit", 0));
            //FrmDlg.DESwitchAttackMode.SetOfHotKey(ini.ReadInteger("Hotkey", "SwitchAttackMode", 0));
            //FrmDlg.DESwitchMiniMap.SetOfHotKey(ini.ReadInteger("Hotkey", "SwitchMiniMap", 0));
            //FrmDlg.DxEditSSkill.SetOfHotKey(ini.ReadInteger("Hotkey", "SerieSkill", 0));
            //g_ShowItemList.LoadFromFile(".\\Data\\Filter.dat");
            //// ============================================================================
            //// g_gcAss[0] := ini.ReadBool('Ass', '0', g_gcAss[0]);
            //g_gcAss[1] = ini.ReadBool("Ass", "1", g_gcAss[1]);
            //g_gcAss[2] = ini.ReadBool("Ass", "2", g_gcAss[2]);
            //g_gcAss[3] = ini.ReadBool("Ass", "3", g_gcAss[3]);
            //g_gcAss[4] = ini.ReadBool("Ass", "4", g_gcAss[4]);
            //g_gcAss[5] = ini.ReadBool("Ass", "5", g_gcAss[5]);
            //g_gcAss[6] = ini.ReadBool("Ass", "6", g_gcAss[6]);
            //g_APPickUpList.Clear();
            //g_APMobList.Clear();
            //Strings = new ArrayList();
            //if (g_gcAss[5])
            //{
            //    sFileName = ".\\Config\\" + g_sServerName + "." + sUserName + ".ItemFilterEx.txt";
            //    if (File.Exists(sFileName))
            //    {
            //        Strings.LoadFromFile(sFileName);
            //    }
            //    else
            //    {
            //        Strings.SaveToFile(sFileName);
            //    }
            //    for (i = 0; i < Strings.Count; i++)
            //    {
            //        if ((Strings[i] == "") || (Strings[i][1] == ";"))
            //        {
            //            continue;
            //        }
            //        so = HGEGUI.Units.HGEGUI.GetValidStr3(Strings[i], ref sn, new string[] { ",", " ", "\t" });
            //        no = ((int)so != "");
            //        g_APPickUpList.Add(sn, ((no) as Object));
            //    }
            //}
            //if (g_gcAss[6])
            //{
            //    sFileName = ".\\Config\\" + g_sServerName + "." + sUserName + ".MonFilter.txt";
            //    if (File.Exists(sFileName))
            //    {
            //        Strings.LoadFromFile(sFileName);
            //    }
            //    else
            //    {
            //        Strings.SaveToFile(sFileName);
            //    }
            //    for (i = 0; i < Strings.Count; i++)
            //    {
            //        if ((Strings[i] == "") || (Strings[i][1] == ";"))
            //        {
            //            continue;
            //        }
            //        // , nil
            //        g_APMobList.Add(Strings[i]);
            //    }
            //}
        }

        public static void LoadItemDesc()
        {
            //if (File.Exists(fItemDesc))
            //{
            //    temp = new ArrayList();
            //    temp.LoadFromFile(fItemDesc);
            //    for (i = 0; i < temp.Count; i++)
            //    {
            //        if (temp[i] == "")
            //        {
            //            continue;
            //        }
            //        desc = HGEGUI.Units.HGEGUI.GetValidStr3(temp[i], ref Name, new string[] { "=" });
            //        desc = desc.Replace("\\", "");
            //        ps = new string();
            //        ps = desc;
            //        if ((Name != "") && (desc != ""))
            //        {
            //            // g_ItemDesc.Put(name, TObject(ps));
            //            g_ItemDesc.Add(Name, ps as object);
            //        }
            //    }
            //    temp.Free;
            //}
        }

        public static int GetLevelColor(byte iLevel)
        {
            int result;
            switch (iLevel)
            {
                case 0:
                    result = 0x00FFFFFF;
                    break;
                case 1:
                    result = 0x004AD663;
                    break;
                case 2:
                    result = 0x00E9A000;
                    break;
                case 3:
                    result = 0x00FF35B1;
                    break;
                case 4:
                    result = 0x000061EB;
                    break;
                case 5:
                    result = 0x005CF4FF;
                    break;
                case 15:
                    result = Color.Gray.ToArgb();
                    break;
                default:
                    result = 0x005CF4FF;
                    break;
            }
            return result;
        }

        public static void LoadItemFilter()
        {
            //int i;
            //int n;
            //string s;
            //string s0;
            //string s1;
            //string s2;
            //string s3;
            //string s4;
            //string fn;
            //ArrayList ls;
            //TCItemRule p;
            //TCItemRule p2;
            //fn = ".\\Data\\lsDefaultItemFilter.txt";
            //if (File.Exists(fn))
            //{
            //    ls = new ArrayList();
            //    ls.LoadFromFile(fn);
            //    for (i = 0; i < ls.Count; i++)
            //    {
            //        s = ls[i];
            //        if (s == "")
            //        {
            //            continue;
            //        }
            //        s = HGEGUI.Units.HGEGUI.GetValidStr3(s, ref s0, new string[] { "," });
            //        s = HGEGUI.Units.HGEGUI.GetValidStr3(s, ref s1, new string[] { "," });
            //        s = HGEGUI.Units.HGEGUI.GetValidStr3(s, ref s2, new string[] { "," });
            //        s = HGEGUI.Units.HGEGUI.GetValidStr3(s, ref s3, new string[] { "," });
            //        s = HGEGUI.Units.HGEGUI.GetValidStr3(s, ref s4, new string[] { "," });
            //        p = new TCItemRule();
            //        p.Name = s0;
            //        p.rare = s2 == "1";
            //        p.pick = s3 == "1";
            //        p.Show = s4 == "1";
            //        g_ItemsFilter_All.Put(s0, p as object);
            //        p2 = new TCItemRule();
            //        p2 = p;
            //        g_ItemsFilter_All_Def.Put(s0, p2 as object);
            //        n = Convert.ToInt32(s1);
            //        switch (n)
            //        {
            //            case 0:
            //                g_ItemsFilter_Dress.Add(s0, p as object);
            //                break;
            //            case 1:
            //                g_ItemsFilter_Weapon.Add(s0, p as object);
            //                break;
            //            case 2:
            //                g_ItemsFilter_Headgear.Add(s0, p as object);
            //                break;
            //            case 3:
            //                g_ItemsFilter_Drug.Add(s0, p as object);
            //                break;
            //            default:
            //                g_ItemsFilter_Other.Add(s0, p as object);
            //                break;
            //                // 服装
            //        }
            //    }
            //    ls.Free;
            //}
        }

        public static void LoadItemFilter2()
        {
            //int i;
            //string s;
            //string s0;
            //string s2;
            //string s3;
            //string s4;
            //string fn;
            //ArrayList ls;
            //TCItemRule p;
            //TCItemRule p2;
            //bool b2;
            //bool b3;
            //bool b4;
            //fn = ".\\Config\\" + g_sServerName + "." + ClMain.frmMain.m_sChrName + ".ItemFilter.txt";
            //// DScreen.AddChatBoardString(fn, clWhite, clBlue);
            //if (File.Exists(fn))
            //{
            //    // DScreen.AddChatBoardString('1', clWhite, clBlue);
            //    ls = new ArrayList();
            //    ls.LoadFromFile(fn);
            //    for (i = 0; i < ls.Count; i++)
            //    {
            //        s = ls[i];
            //        if (s == "")
            //        {
            //            continue;
            //        }
            //        s = HGEGUI.Units.HGEGUI.GetValidStr3(s, ref s0, new string[] { "," });
            //        s = HGEGUI.Units.HGEGUI.GetValidStr3(s, ref s2, new string[] { "," });
            //        s = HGEGUI.Units.HGEGUI.GetValidStr3(s, ref s3, new string[] { "," });
            //        s = HGEGUI.Units.HGEGUI.GetValidStr3(s, ref s4, new string[] { "," });
            //        p = ((TCItemRule)(g_ItemsFilter_All_Def.GetValues(s0)));
            //        if (p != null)
            //        {
            //            // DScreen.AddChatBoardString('2', clWhite, clBlue);
            //            b2 = s2 == "1";
            //            b3 = s3 == "1";
            //            b4 = s4 == "1";
            //            if ((b2 != p.rare) || (b3 != p.pick) || (b4 != p.Show))
            //            {
            //                // DScreen.AddChatBoardString('3', clWhite, clBlue);
            //                p2 = ((TCItemRule)(g_ItemsFilter_All.GetValues(s0)));
            //                if (p2 != null)
            //                {
            //                    // DScreen.AddChatBoardString('4', clWhite, clBlue);
            //                    p2.rare = b2;
            //                    p2.pick = b3;
            //                    p2.Show = b4;
            //                }
            //            }
            //        }
            //    }
            //    ls.Free;
            //}
        }

        public static void SaveItemFilter()
        {
            // 退出增量保存
            //int i;
            //ArrayList ls;
            //TCItemRule p;
            //TCItemRule p2;
            //string fn;
            //fn = ".\\Config\\" + g_sServerName + "." + ClMain.frmMain.m_sChrName + ".ItemFilter.txt";
            //ls = new ArrayList();
            //for (i = 0; i < g_ItemsFilter_All.Count; i++)
            //{
            //    p = ((TCItemRule)(g_ItemsFilter_All.GetValues(g_ItemsFilter_All.Keys[i])));
            //    p2 = ((TCItemRule)(g_ItemsFilter_All_Def.GetValues(g_ItemsFilter_All_Def.Keys[i])));
            //    if (p.Name == p2.Name)
            //    {
            //        if ((p.rare != p2.rare) || (p.pick != p2.pick) || (p.Show != p2.Show))
            //        {
            //            ls.Add(string.Format("%s,%d,%d,%d", new byte[] { p.Name, ((byte)p.rare), ((byte)p.pick), ((byte)p.Show) }));
            //        }
            //    }
            //}
            //if (ls.Count > 0)
            //{
            //    ls.SaveToFile(fn);
            //}
            //ls.Free;
        }

        public static int GetItemWhere(ClientItem clientItem)
        {
            int result = -1;
            if (clientItem.Item.Name == "")
            {
                return result;
            }
            switch (clientItem.Item.StdMode)
            {
                case 10:
                case 11:
                    result = ItemLocation.Dress;
                    break;
                case 5:
                case 6:
                    result = ItemLocation.Weapon;
                    break;
                case 30:
                    result = ItemLocation.RighThand;
                    break;
                case 19:
                case 20:
                case 21:
                    result = ItemLocation.Necklace;
                    break;
                case 15:
                    result = ItemLocation.Helmet;
                    break;
                case 16:
                    break;
                case 24:
                case 26:
                    result = ItemLocation.ArmRingl;
                    break;
                case 22:
                case 23:
                    result = ItemLocation.Ringl;
                    break;
                case 25:
                    result = ItemLocation.Bujuk;
                    break;
                case 27:
                    result = ItemLocation.Belt;
                    break;
                case 28:
                    result = ItemLocation.Boots;
                    break;
                case 7:
                case 29:
                    result = ItemLocation.Charm;
                    break;
            }
            return result;
        }

        public static bool GetSecretAbil(ClientItem CurrMouseItem)
        {
            bool result = false;
            //if (!new ArrayList(new int[] { 5, 6, 10, 15, 26 }).Contains(CurrMous.Item.Item.StdMode))
            //{
            //    return result;
            //}
            return result;
        }

        public static void InitClientItems()
        {
            //FillChar(g_MagicArr);            
            //FillChar(g_TakeBackItemWait);           
            //FillChar(g_UseItems);           
            //FillChar(g_ItemArr);           
            //FillChar(g_RefineItems);     
            //FillChar(g_BuildAcuses);     
            //FillChar(g_DetectItem);    
            //FillChar(g_TIItems);        
            //FillChar(g_spItems);     
            //FillChar(g_ItemArr);        
            //FillChar(g_HeroItemArr);  
            //FillChar(g_DealItems);       
            //FillChar(g_YbDealItems);         
            //FillChar(g_DealRemoteItems);
        }

        public static byte GetTIHintString1(int idx, ClientItem ci, string iname)
        {
            byte result = 0;
            //switch (idx)
            //{
            //    case 0:
            //        g_tiHintStr1 = "我收藏天下的奇珍异宝，走南闯北几十年了，各种神器见过不少，把你要鉴定的装备放在桌子上吧！";
            //        FrmDlg.DBTIbtn1.btnState = tdisable;
            //        FrmDlg.DBTIbtn1.Caption = "普通鉴定";
            //        FrmDlg.DBTIbtn2.btnState = tdisable;
            //        FrmDlg.DBTIbtn2.Caption = "高级鉴定";
            //        break;
            //    case 1:
            //        if ((ci == null) || (ci.s.Name == ""))
            //        {
            //            return result;
            //        }
            //        if (ci.Item.Eva.EvaTimesMax == 0)
            //        {
            //            g_tiHintStr1 = "标志了不可鉴定的物品我是鉴定不了的，你换一个吧。";
            //            FrmDlg.DBTIbtn1.btnState = tdisable;
            //            FrmDlg.DBTIbtn1.Caption = "普通鉴定";
            //            FrmDlg.DBTIbtn2.btnState = tdisable;
            //            FrmDlg.DBTIbtn2.Caption = "高级鉴定";
            //            return result;
            //        }
            //        if (ci.Item.Eva.EvaTimes < ci.Item.Eva.EvaTimesMax)
            //        {
            //            if (FrmDlg.DWTI.tag == 1)
            //            {
            //                switch (ci.Item.Eva.EvaTimes)
            //                {
            //                    case 0:
            //                        g_tiHintStr1 = "第一次鉴定我需要一个一级鉴定卷轴，你快去收集一个吧！";
            //                        FrmDlg.DBTIbtn1.btnState = tnor;
            //                        FrmDlg.DBTIbtn1.Caption = "普通一鉴";
            //                        FrmDlg.DBTIbtn2.btnState = tnor;
            //                        FrmDlg.DBTIbtn2.Caption = "高级一鉴";
            //                        break;
            //                    case 1:
            //                        g_tiHintStr1 = "第二次鉴定我需要一个二级鉴定卷轴，你快去收集一个吧！";
            //                        FrmDlg.DBTIbtn1.btnState = tnor;
            //                        FrmDlg.DBTIbtn1.Caption = "普通二鉴";
            //                        FrmDlg.DBTIbtn2.btnState = tnor;
            //                        FrmDlg.DBTIbtn2.Caption = "高级二鉴";
            //                        break;
            //                    case 2:
            //                        g_tiHintStr1 = "第三次鉴定我需要一个三级鉴定卷轴，你快去收集一个吧！";
            //                        FrmDlg.DBTIbtn1.btnState = tnor;
            //                        FrmDlg.DBTIbtn1.Caption = "普通三鉴";
            //                        FrmDlg.DBTIbtn2.btnState = tnor;
            //                        FrmDlg.DBTIbtn2.Caption = "高级三鉴";
            //                        break;
            //                    default:
            //                        g_tiHintStr1 = "我需要一个三级鉴定卷轴来鉴定你这个装备。";
            //                        FrmDlg.DBTIbtn1.btnState = tnor;
            //                        FrmDlg.DBTIbtn1.Caption = "普通三鉴";
            //                        FrmDlg.DBTIbtn2.btnState = tnor;
            //                        FrmDlg.DBTIbtn2.Caption = "高级三鉴";
            //                        break;
            //                }
            //            }
            //            else if (FrmDlg.DWTI.tag == 2)
            //            {
            //                FrmDlg.DBTIbtn1.btnState = tnor;
            //                FrmDlg.DBTIbtn1.Caption = "更换";
            //            }
            //            result = ci.Item.Eva.EvaTimes;
            //        }
            //        else
            //        {
            //            g_tiHintStr1 = string.Format("你的这件%s已经不能再鉴定了。", new string[] { ci.s.Name });
            //            FrmDlg.DBTIbtn1.btnState = tdisable;
            //            FrmDlg.DBTIbtn1.Caption = "普通鉴定";
            //            FrmDlg.DBTIbtn2.btnState = tdisable;
            //            FrmDlg.DBTIbtn2.Caption = "高级鉴定";
            //        }
            //        break;
            //    case 2:
            //        g_tiHintStr1 = string.Format("借助卷轴的力量，我已经帮你发现了你这%s的潜能。", new string[] { iname });
            //        break;
            //    case 3:
            //        g_tiHintStr1 = string.Format("借助卷轴的力量，我已经帮你发现了你这%s的神秘潜能。", new string[] { iname });
            //        break;
            //    case 4:
            //        g_tiHintStr1 = string.Format("这%s虽然没能发现更大的潜能，但是他拥有感应其他宝物存在的特殊能力。", new string[] { iname });
            //        break;
            //    case 5:
            //        g_tiHintStr1 = string.Format("我并没能从你的这个%s上发现更多的潜能。你不要沮丧，我会给你额外的补偿。", new string[] { iname });
            //        break;
            //    case 6:
            //        g_tiHintStr1 = string.Format("我并没能从你的这个%s上发现更多的潜能。", new string[] { iname });
            //        break;
            //    case 7:
            //        g_tiHintStr1 = string.Format("我并没能从你的这个%s上发现更多的潜能。你的宝物已经不可鉴定。", new string[] { iname });
            //        break;
            //    case 8:
            //        g_tiHintStr1 = "你缺少宝物或者卷轴。";
            //        break;
            //    case 9:
            //        g_tiHintStr1 = string.Format("恭喜你的宝物被鉴定为主宰装备，你获得了%s。", new string[] { iname });
            //        break;
            //    case 10:
            //        g_tiHintStr1 = "待鉴物品错误或不存在！";
            //        break;
            //    case 11:
            //        g_tiHintStr1 = string.Format("你的这件%s不可以鉴定！", new string[] { iname });
            //        break;
            //    case 12:
            //        FrmDlg.DBTIbtn1.btnState = tdisable;
            //        FrmDlg.DBTIbtn2.btnState = tdisable;
            //        g_tiHintStr1 = string.Format("以我目前的能力，%s只能先鉴定到这里了。", new string[] { iname });
            //        break;
            //    case 30:
            //        g_tiHintStr1 = "鉴定卷轴错误或不存在！";
            //        break;
            //    case 31:
            //        g_tiHintStr1 = string.Format("我需要一个%s级鉴定卷轴，你的卷轴不符合要求！", new string[] { iname });
            //        break;
            //    case 32:
            //        g_tiHintStr1 = string.Format("高级鉴定失败，你的%s消失了！", new string[] { iname });
            //        break;
            //    case 33:
            //        g_tiHintStr1 = string.Format("服务器没有%s的数据，高级鉴定失败！", new string[] { iname });
            //        break;
            //}
            return result;
        }

        public static byte GetTIHintString1(int idx)
        {
            return GetTIHintString1(idx, null);
        }

        public static byte GetTIHintString1(int idx, ClientItem ci)
        {
            return GetTIHintString1(idx, ci, "");
        }

        public static byte GetTIHintString2(int idx, ClientItem ci, string iname)
        {
            byte result = 0;
            /*g_tiHintStr1 = "";
            switch (idx)
            {
                case 0:
                    g_tiHintStr1 = "如果你不喜欢已经鉴定过了的宝物，你可以把他给我，我平素最爱收藏各种宝物，我会给你一个一模一样的没鉴定过的装备作为补偿。";
                    FrmDlg.DBTIbtn1.btnState = tdisable;
                    break;
                case 1:
                    g_tiHintStr1 = string.Format("这个%s，看上去不错，我这里正好有没有鉴定过的各种%s你可以挑一把，要换的话，你要给我一个幸运符。", new string[] { ci.s.Name, ci.s.Name });
                    FrmDlg.DBTIbtn1.btnState = tnor;
                    break;
                case 2:
                    g_tiHintStr1 = string.Format("我已经给了你一把没鉴定过的%s，跟你原来的%s没鉴定过之前是一模一样的！", new string[] { iname, iname });
                    break;
                case 3:
                    g_tiHintStr1 = "缺少宝物或材料。";
                    break;
                case 4:
                    g_tiHintStr1 = string.Format("你的这件%s并没有鉴定过。", new string[] { iname });
                    break;
                case 5:
                    g_tiHintStr1 = "材料不符合，请放入幸运符。";
                    break;
                case 6:
                    g_tiHintStr1 = "该物品框只能放鉴定过的宝物，你的东西不符合，我已经将它放回你的包裹了。";
                    break;
                case 7:
                    g_tiHintStr1 = "该物品框只能放幸运符，你的东西不符合，我已经将它放回你的包裹了。";
                    break;
                case 8:
                    g_tiHintStr1 = "宝物更换失败。";
                    break;
            }*/
            return result;
        }

        public static byte GetTIHintString2(int idx)
        {
            return GetTIHintString2(idx, null);
        }

        public static byte GetTIHintString2(int idx, ClientItem ci)
        {
            return GetTIHintString2(idx, ci, "");
        }

        public static byte GetSpHintString1(int idx, ClientItem ci, string iname)
        {
            byte result = 0;
            /*g_spHintStr1 = "";
            switch (idx)
            {
                case 0:
                    g_spHintStr1 = "你可以跟别人购买神秘卷轴，也可以自己制作神秘卷轴来解读宝物的神秘属性。";
                    break;
                case 1:
                    g_spHintStr1 = "这次解读不幸失败，解读幸运值、神秘卷轴" + "的等级和熟练度过低可能导致解读失败，不" + "要失望，再接再厉吧。";
                    break;
                case 2:
                    g_spHintStr1 = "找不到鉴定物品或卷轴";
                    break;
                case 3:
                    FrmDlg.DBSP.btnState = tdisable;
                    g_spHintStr1 = "没有可鉴定的神秘属性";
                    break;
                case 4:
                    g_spHintStr1 = "装备不符合神秘解读要求";
                    break;
                case 5:
                    g_spHintStr1 = "卷轴类型不符合";
                    break;
                case 6:
                    g_spHintStr1 = "卷轴等级不符合";
                    break;
                case 7:
                    g_spHintStr1 = "借助神秘卷轴的帮助，已经帮你解读出了一个神秘属性";
                    break;
                case 10:
                    g_spHintStr1 = "神秘卷轴制作成功。";
                    break;
                case 11:
                    g_spHintStr1 = "这次制作不幸的失败了，可能是因为你的神" + "秘解读技能等级还不够高，或者是你制作的" + "卷轴等级太高了";
                    break;
                case 12:
                    g_spHintStr1 = "找不到羊皮卷。";
                    break;
                case 13:
                    g_spHintStr1 = "请放入羊皮卷。";
                    break;
                case 14:
                    g_spHintStr1 = "精力值不够。";
                    break;
                case 15:
                    g_spHintStr1 = "没有解读技能，制作失败。";
                    break;
            }*/
            return result;
        }

        public static byte GetSpHintString1(int idx)
        {
            return GetSpHintString1(idx, null);
        }

        public static byte GetSpHintString1(int idx, ClientItem ci)
        {
            return GetSpHintString1(idx, ci, "");
        }

        public static byte GetSpHintString2(int idx, ClientItem ci, string iname)
        {
            byte result = 0;
            switch (idx)
            {
                case 0:
                    //g_spHintStr1 = "你可以把你对鉴宝的心得还有你的鉴定经验写在神秘卷轴上，这样的话，就可以帮助更多人解读神秘属性。";
                    break;
            }
            return result;
        }

        public static byte GetSpHintString2(int idx)
        {
            return GetSpHintString2(idx, null);
        }

        public static byte GetSpHintString2(int idx, ClientItem ci)
        {
            return GetSpHintString2(idx, ci, "");
        }

        public static void AutoPutOntiBooks()
        {
            //if ((g_TIItems[0].Item.Item.Name != "") && (g_TIItems[0].Item.Item.Eva.EvaTimesMax > 0) && (g_TIItems[0].Item.Item.Eva.EvaTimes < g_TIItems[0].Item.Item.Eva.EvaTimesMax) && ((g_TIItems[1].Item.Item.Name == "") || (g_TIItems[1].Item.Item.StdMode != 56) || !(g_TIItems[1].Item.Item.Shape >= 1 && g_TIItems[1].Item.Item.Shape <= 3) || (g_TIItems[1].Item.Item.Shape != g_TIItems[0].Item.Item.Eva.EvaTimes + 1)))
            //{
            //    for (i = MaxBagItemCL - 1; i >= 6; i--)
            //    {
            //        if ((g_ItemArr[i].Item.Name != "") && (g_ItemArr[i].Item.StdMode == 56) && (g_ItemArr[i].Item.Shape == g_TIItems[0].Item.Eva.EvaTimes + 1))
            //        {
            //            if (g_TIItems[1].Item.Item.Name != "")
            //            {
            //                cu = g_TIItems[1].Item;
            //                g_TIItems[1].Item = g_ItemArr[i];
            //                g_TIItems[1].Index = i;
            //                g_ItemArr[i] = cu;
            //            }
            //            else
            //            {
            //                g_TIItems[1].Item = g_ItemArr[i];
            //                g_TIItems[1].Index = i;
            //                g_ItemArr[i].Item.Name = "";
            //            }
            //            break;
            //        }
            //    }
            //}
        }

        public static void AutoPutOntiSecretBooks()
        {
            //if (FrmDlg.DWSP.Visible && (FrmDlg.DWSP.tag == 1) && (g_spItems[0].Item.Item.Name != "") && (g_spItems[0].Item.Item.Eva.EvaTimesMax > 0) && ((g_spItems[1].Item.Item.Name == "") || (g_spItems[1].Item.Item.StdMode != 56) || (g_spItems[1].Item.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             s.Shape != 0)))
            //{
            //    for (i = MaxBagItemCL - 1; i >= 6; i--)
            //    {
            //        if ((g_ItemArr[i].Item.Name != "") && (g_ItemArr[i].Item.StdMode == 56) && (g_ItemArr[i].Item.Shape == 0))
            //        {
            //            if (g_spItems[1].Item.Item.Name != "")
            //            {
            //                cu = g_spItems[1].Item;
            //                g_spItems[1].Item = g_ItemArr[i];
            //                g_spItems[1].Index = i;
            //                g_ItemArr[i] = cu;
            //            }
            //            else
            //            {
            //                g_spItems[1].Item = g_ItemArr[i];
            //                g_spItems[1].Index = i;
            //                g_ItemArr[i].Item.Name = "";
            //            }
            //            break;
            //        }
            //    }
            //}
        }

        public static void AutoPutOntiCharms()
        {
            //if ((g_TIItems[0].Item.Item.Name != "") && (g_TIItems[0].Item.Item.Eva.EvaTimesMax > 0) && (g_TIItems[0].Item.Item.Eva.EvaTimes > 0) && ((g_TIItems[1].Item.Item.Name == "") || (g_TIItems[1].Item.Item.StdMode != 41) || (g_TIItems[1].Item.Item.Shape != 30)))
            //{
            //    for (i = MaxBagItemCL - 1; i >= 6; i--)
            //    {
            //        if ((g_ItemArr[i].Item.Name != "") && (g_ItemArr[i].Item.StdMode == 41) && (g_ItemArr[i].Item.Shape == 30))
            //        {
            //            if (g_TIItems[1].Item.Item.Name != "")
            //            {
            //                cu = g_TIItems[1].Item;
            //                g_TIItems[1].Item = g_ItemArr[i];
            //                g_TIItems[1].Index = i;
            //                g_ItemArr[i] = cu;
            //            }
            //            else
            //            {
            //                g_TIItems[1].Item = g_ItemArr[i];
            //                g_TIItems[1].Index = i;
            //                g_ItemArr[i].Item.Name = "";
            //            }
            //            break;
            //        }
            //    }
            //}
        }

        public static bool GetSuiteAbil(int idx, int Shape, ref byte[] sa)
        {
            bool result = false;
            //FillChar(sa);           
            //switch (idx)
            //{
            //    case 1:
            //        result = true;
            //        for (i = TtSuiteAbil.GetLowerBound(0); i <= TtSuiteAbil..Length(0); i++ )
            //        {
            //            if ((g_UseItems[i].s.Name != "") && ((g_UseItems[i].s.Shape == Shape) || (g_UseItems[i].s.AniCount == Shape)))
            //            {
            //                sa[i] = 1;
            //            }
            //        }
            //        break;
            //    case 2:
            //        result = true;
            //        for (i = Grobal2.byte.GetLowerBound(0); i <= Grobal2.byte..Length(0); i++ )
            //        {
            //            if ((g_HeroUseItems[i].s.Name != "") && ((g_HeroUseItems[i].s.Shape == Shape) || (g_HeroUseItems[i].s.AniCount == Shape)))
            //            {
            //                sa[i] = 1;
            //            }
            //        }
            //        break;
            //    case 3:
            //        result = true;
            //        for (i = Grobal2.byte.GetLowerBound(0); i <= Grobal2.byte..Length(0); i++ )
            //        {
            //            if ((UserState1.UseItems[i].s.Name != "") && ((UserState1.UseItems[i].s.Shape == Shape) || (UserState1.UseItems[i].s.AniCount == Shape)))
            //            {
            //                sa[i] = 1;
            //            }
            //        }
            //        break;
            //}
            return result;
        }

        public static bool IsInMyRange(Actor Act)
        {
            bool result = false;
            if ((Act == null) || (MySelf == null))
            {
                return result;
            }
            if ((Math.Abs(Act.CurrX - MySelf.CurrX) <= (BotConst.TileMapOffSetX - 2)) && (Math.Abs(Act.CurrY - MySelf.CurrY) <= (BotConst.TileMapOffSetY - 1)))
            {
                result = true;
            }
            return result;
        }

        public static bool IsItemInMyRange(int X, int Y)
        {
            bool result = false;
            if (MySelf == null)
            {
                return result;
            }
            if ((Math.Abs(X - MySelf.CurrX) <= HUtil32._MIN(24, BotConst.TileMapOffSetX + 9)) && (Math.Abs(Y - MySelf.CurrY) <= HUtil32._MIN(24, BotConst.TileMapOffSetY + 10)))
            {
                result = true;
            }
            return result;
        }
    }

    public struct TVaInfo
    {
        public string cap;
        public Rectangle[] pt1;
        public Rectangle[] pt2;
        public string[] str1;
        public string[] Hint;
    }

    public class FindMapNode
    {
        public short X;
        public short Y;
    }

    public class MapTree
    {
        public int H;
        public short X;
        public short Y;
        public byte Dir;
        public MapTree Father;
    }

    public class MapLink
    {
        public MapTree Node;
        public int F;
        public MapLink Next;
    }

    public struct TVirusSign
    {
        public int Offset;
        public string CodeSign;
    }

    public class TMovingItem
    {
        public int Index;
        public ClientItem Item;
    }

    public struct TCleintBox
    {
        public int Index;
        public ClientItem Item;
    }

    public enum MagicType
    {
        mtReady,
        mtFly,
        mtExplosion,
        mtFlyAxe,
        mtFireWind,
        mtFireGun,
        mtLightingThunder,
        mtThunder,
        mtExploBujauk,
        mtBujaukGroundEffect,
        mtKyulKai,
        mtFlyArrow,
        mtFlyBug,
        mtGroundEffect,
        mtThuderEx,
        mtFireBall,
        mtFlyBolt,
        mtRedThunder,
        mtRedGroundThunder,
        mtLava,
        mtSpurt,
        mtFlyStick,
        mtFlyStick2
    }

    public struct TShowItem
    {
        public string sItemName;
        public TItemType ItemType;
        public bool boAutoPickup;
        public bool boShowName;
        public int nFColor;
        public int nBColor;
    }
    public struct TMapDescInfo
    {
        public string szMapTitle;
        public string szPlaceName;
        public int nPointX;
        public int nPointY;
        public Color nColor;
        public int nFullMap;
    }

    public struct TSeriesSkill
    {
        public byte wMagid;
        public byte nStep;
        public bool bSpell;
    }

    public struct TTempSeriesSkillA
    {
        public ClientMagic pm;
        public bool bo;
    }

    public enum TTimerCommand
    {
        tcSoftClose,
        tcReSelConnect,
        tcFastQueryChr,
        tcQueryItemPrice
    }

    public enum PlayerAction
    {
        Walk,
        Run,
        HorseRun,
        Hit,
        Spell,
        Sitdown
    }

    public enum ConnectionStep
    {
        Connect,
        NewAccount,
        QueryServer,
        SelServer,
        Intro,
        Login,
        NewChr,
        QueryChr,
        SelChr,
        ReSelChr,
        Play
    }

    public enum ConnectionStatus
    {
        Success,
        Connect,
        Failure,
    }

    public enum TItemType
    {
        i_HPDurg,
        i_MPDurg,
        i_HPMPDurg,
        i_OtherDurg,
        i_Weapon,
        i_Dress,
        i_Helmet,
        i_Necklace,
        i_Armring,
        i_Ring,
        i_Belt,
        i_Boots,
        i_Charm,
        i_Book,
        i_PosionDurg,
        i_UseItem,
        i_Scroll,
        i_Stone,
        i_Gold,
        i_Other
    }

    public record struct TChrMsg
    {
        public int Ident;
        public short X;
        public short Y;
        public int Dir;
        public int State;
        public int Feature;
        public int Saying;
        public int Sound;
        public int dwDelay;
    }

    public class TDropItem
    {
        public short X;
        public short Y;
        public int id;
        public int looks;
        public string Name;
        public int Width;
        public int Height;
        public int FlashTime;
        public int FlashStepTime;
        public int FlashStep;
        public bool BoFlash;
        public bool boNonSuch;
        public bool boPickUp;
        public bool boShowName;
    }

    public class TUseMagicInfo
    {
        public int ServerMagicCode;
        public int MagicSerial;
        public int Target;
        public MagicType EffectType;
        public int EffectNumber;
        public int targx;
        public int targy;
        public bool Recusion;
        public int AniTime;
    }
}