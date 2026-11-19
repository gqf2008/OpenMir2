// npc-golden —— NPC 线（card/npc-parity）NP-01 的 C# oracle 金标准生成器。
//
// 做法与 m3/eat-probe 同族：`new PlayObject()` + `new NormNpc()` + 真 `ScriptEngine`，
// **反射调私有** `PlayObject.ClientClickNpc`（`PlayObject.Base.cs:674`），然后把入队的 RM
// 按真实消息泵 `GetMessage` → `Operate`（`PlayObject.Message.cs:355-363`）映射成 SM。
//
// 记录面：
//   * rm[]：入 `MsgQueue` 的 RM（ident/wParam/nParam1..3/ActorId/LateDelivery/Msg）+ 优先级；
//   * sm[]：真实 `Operate` 下发的 SM 帧（ident/recog/param/tag/series/len）；
//   * state：LastNpc / ScriptCurrLable / ScriptGoBackLable / ScriptLable / 是否选中脚本；
//   * 负例：远距（>15）/ Death / 限频 ⇒ 零帧。
//
// 只读：脚本写临时目录；不连 MySQL、不碰现网文件、不起 socket。
//
// 用法（仓库根）：
//   dotnet run --project mir2-rs/tests/parity/csharp/NpcGolden -c Release -- \
//     mir2-rs/tests/parity/golden/npc/np01_click_main.json

using System;
using System.Collections;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Security.Cryptography;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using M2Server;
using M2Server.Maps;
using M2Server.Net;
using M2Server.Npc;
using M2Server.Player;
using GameSrv.Npc;
using OpenMir2;
using OpenMir2.Data;
using OpenMir2.Enums;
using OpenMir2.Packets.ClientPackets;
using OpenMir2.Packets.ServerPackets;
using ScriptSystem;
using Serilog;
using SystemModule;
using SystemModule.Actors;
using SystemModule.Data;
using SystemModule.SubSystem;

namespace NpcGolden;

/// <summary>`SystemShare.ItemSystem` 的最小替身（NPC 点击链不读物品表；只保证非 null 且不抛）。</summary>
internal sealed class ProbeItemSystem : IItemSystem
{
    public StdItem GetStdItem(ushort nItemIdx) => null;
    public string GetStdItemName(int itemIdx) => string.Empty;
    public int GetUpgradeStdItem(StdItem stdItem, UserItem userItem, ref ClientItem clientItem)
    {
        clientItem ??= new ClientItem();
        return 0;
    }
    public int RealAttackSpeed(short wAtkSpd) => wAtkSpd <= 10 ? -wAtkSpd : wAtkSpd - 10;
    public void AddItem(StdItem stdItem) => throw new NotSupportedException();
    public void Clear() => throw new NotSupportedException();
    public int ItemCount => 0;
    public ushort GetStdItemIdx(string itemName) => throw new NotSupportedException();
    public bool CopyToUserItemFromName(string sItemName, ref UserItem item) => throw new NotSupportedException();
    public StdItem GetStdItem(string sItemName) => throw new NotSupportedException();
    public int GetStdItemWeight(int nItemIdx) => throw new NotSupportedException();
    public int GetUpgrade2(int x, int a) => throw new NotSupportedException();
    public void RandomSetUnknownItem(StdItem stdItem, UserItem pu) => throw new NotSupportedException();
    public void RandomUpgradeItem(StdItem stdItem, UserItem pu) => throw new NotSupportedException();
}

internal sealed class CapFrame
{
    public int ident;
    public int recog;
    public int param;
    public int tag;
    public int series;
    public int len;
}

/// <summary>截获 `PlayObject` 真正发出的即时帧（`AddGateBuffer`）。</summary>
internal sealed class CaptureNetChannel : INetChannel
{
    public readonly List<CapFrame> Frames = new();

    public void AddGateBuffer(int gateIdx, byte[] senData)
    {
        if (senData == null || senData.Length < 32)
        {
            Frames.Add(new CapFrame { ident = -1, len = senData?.Length ?? 0 });
            return;
        }
        Frames.Add(new CapFrame
        {
            ident = BitConverter.ToUInt16(senData, 24),
            recog = BitConverter.ToInt32(senData, 20),
            param = BitConverter.ToUInt16(senData, 26),
            tag = BitConverter.ToUInt16(senData, 28),
            series = BitConverter.ToUInt16(senData, 30),
            len = senData.Length,
        });
    }

    public void AddGameGateQueue(int gateIdx, ServerMessage packet, byte[] data) { }
    public void CloseAllGate() { }
    public void CloseUser(int gateIdx, int nSocket) { }
    public void Initialize() { }
    public void KickUser(string account, int sessionId, int payMode) { }
    public void Run() { }
    public void SendOutConnectMsg(int gateIdx, int nSocket, ushort nGsIdx) { }
    public void SendServerStopMsg() { }
    public void SetGateUserList(int gateIdx, int nSocket, IPlayerActor playObject) { }
    public Task Start(CancellationToken cancellationToken = default) => Task.CompletedTask;
    public void Send(string connectId, byte[] buff) { }
    public void CloseGate(string connectionId, string endPoint) { }
    public Task StopAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;
}

/// <summary>`SystemShare.WorldEngine` 的最小替身：FindNpc 返回注入的 NPC，FindMerchant 返回 null。</summary>
internal class WorldProxy : DispatchProxy
{
    public object Npc;

    public static IWorldEngine Create(object npc)
    {
        var p = DispatchProxy.Create<IWorldEngine, WorldProxy>();
        ((WorldProxy)(object)p).Npc = npc;
        return p;
    }

    protected override object Invoke(MethodInfo targetMethod, object[] args)
    {
        if (targetMethod.Name == "FindNpc" || targetMethod.Name == "FindMerchant")
        {
            return Npc;
        }
        var rt = targetMethod.ReturnType;
        if (rt == typeof(void))
        {
            return null;
        }
        return rt.IsValueType ? Activator.CreateInstance(rt) : null;
    }
}

internal static class Program
{
    private static readonly JsonSerializerOptions JsonOpts = new()
    {
        WriteIndented = true,
        Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
    };

    private static object GetMsgQueue(object po)
    {
        for (Type t = po.GetType(); t != null; t = t.BaseType)
        {
            var f = t.GetField("MsgQueue", BindingFlags.Instance | BindingFlags.NonPublic | BindingFlags.Public | BindingFlags.DeclaredOnly);
            if (f != null)
            {
                return f.GetValue(po);
            }
        }
        throw new MissingFieldException("ActorEntity.MsgQueue");
    }

    /// <summary>`PriorityQueue&lt;SendMessage,byte&gt;.UnorderedItems` 的优先级（只用于记录/断言）。</summary>
    private static List<int> QueuePriorities(object pq)
    {
        var prop = pq.GetType().GetProperty("UnorderedItems")
            ?? throw new MissingMemberException("PriorityQueue.UnorderedItems");
        var items = (IEnumerable)prop.GetValue(pq);
        var list = new List<int>();
        foreach (var it in items)
        {
            var t = it.GetType();
            var pr = t.GetField("Item2").GetValue(it);
            list.Add(Convert.ToInt32(pr));
        }
        list.Sort();
        return list;
    }

    private static int _npcSeq = 5001;

    private const string CLICK_SCRIPT =
        "[@main]\n老人：你需要什么？\\ <打听/@ask> <离开/@exit>";
    private const string DIALOG_SCRIPT = CLICK_SCRIPT + "\n\n[@ask]\n#SAY\n打听结果：一切正常\\ <返回/@back>";
    private const string MERCHANT_SCRIPT =
        "E:/MirServer/M2GameSvr/Envir/Market_Def/封魔谷/恶魔修理师-d2008.txt";
    private const string SMITH_SCRIPT =
        "E:/MirServer/M2GameSvr/Envir/Market_Def/任务NPC/打铁-0.txt";
    private const string QFUNCTION_SCRIPT =
        "E:/MirServer/M2GameSvr/Envir/Market_Def/QFunction-0.txt";
    private const string EXTJMP_FIXTURE =
        "mir2-rs/tests/parity/golden/npc/fixtures/np05_extjmp_synthetic.txt";
    private static readonly string[] CLICK_ORACLE_SOURCE =
    {
        "src/M2Server/Player/PlayObject.Base.cs:674 (ClientClickNpc)",
        "src/M2Server/Npc/NormNpc.cs:68 (Click -> GotoLable @main)",
        "src/Modules/ScriptEngine/ScriptEngine.cs:141 (GotoLable)",
        "src/Modules/ScriptEngine/ScriptEngine.cs:987 (GotoLableSendMerChantSayMsg)",
        "src/M2Server/Player/PlayObject.Message.cs:1702 (RM_MERCHANTSAY -> SM_MERCHANTSAY)",
    };
    private static readonly string[] DIALOG_ORACLE_SOURCE =
    {
        "src/M2Server/Player/PlayObject.Operate.cs:114 (ClientMerchantDlgSelect)",
        "src/M2Server/Npc/NormNpc.cs:1633 (NormNpc.UserSelect: label ledger only)",
        "src/GameSrv/Npc/Merchant.cs:789 (Merchant.UserSelect -> GotoLable / @back)",
        "src/Modules/ScriptEngine/ScriptEngine.cs:141 (GotoLable)",
        "src/Modules/ScriptEngine/ScriptEngine.cs:987 (GotoLableSendMerChantSayMsg)",
        "src/M2Server/Player/PlayObject.Message.cs:1702 (RM_MERCHANTSAY -> SM_MERCHANTSAY)",
    };
    private static Dictionary<string, object> BuildCtx(CaseSpec spec)
    {
        var ctx = new Dictionary<string, object>
        {
            ["player_x"] = spec.PlayerX,
            ["player_y"] = spec.PlayerY,
            ["npc_x"] = spec.NpcX,
            ["npc_y"] = spec.NpcY,
            ["death"] = spec.Death,
            ["rate_limited"] = spec.ClickNpcTime != 0 && spec.ClickNpcTimeConfig > 0,
            ["click_npc_time_config"] = spec.ClickNpcTimeConfig,
        };
        if (spec.Kind != "click")
        {
            ctx["kind"] = spec.Kind;
            ctx["label"] = spec.Label;
            ctx["labels"] = spec.Labels;
        }
        return ctx;
    }
    private static Dictionary<string, object> BuildState(PlayObject po, string kind, int lastNpcId)
    {
        var st = new Dictionary<string, object>
        {
            ["last_npc_is_npc"] = po.LastNpc == lastNpcId,
            ["script_curr_lable"] = po.ScriptCurrLable ?? string.Empty,
            ["script_go_back_lable"] = po.ScriptGoBackLable ?? string.Empty,
            ["script_lable"] = po.ScriptLable ?? string.Empty,
            ["script_selected"] = po.Script != null,
        };
        if (kind == "act_first")
        {
            st["mnm6"] = po.MNMval[6];
            st["flag106"] = (po.QuestFlag[13] & 64) != 0;
            st["game_gold"] = po.GameGold;
        }
        if (kind == "direct_goto")
        {
            st["game_gold"] = po.GameGold;
            st["send_msg_flag"] = po.BoSendMsgFlag;
        }
        if (kind.StartsWith("dlg", StringComparison.Ordinal))
        {
            st["can_jmp_labels"] = po.CanJmpScriptLableMap.Keys.Cast<string>()
                .OrderBy(x => x, StringComparer.OrdinalIgnoreCase).ToArray();
        }
        return st;
    }

    private sealed class CaseSpec
    {
        public string Name;
        public int PlayerX = 10;
        public int PlayerY = 10;
        public int NpcX = 10;
        public int NpcY = 10;
        public bool Death;
        public string Kind = "click";
        public string Label = "";
        public string[] Labels;
        public string ScriptName = "老人";
        public string ScriptSourcePath;
        public string MerchantScriptRel;
        public string ScriptText2;
        public int Mnm6;
        public int GameGold;
        public int Quest106;
        public string ScriptName2 = "国王";
        public string ScriptText = CLICK_SCRIPT;
        public int ClickNpcTime = 0;
        public int ClickNpcTimeConfig = 0;
    }

    private static object RunCase(CaseSpec spec, string tmp, Envirnoment envir, CaptureNetChannel net)
    {
        net.Frames.Clear();
        var npcId = _npcSeq++;
        var lastNpcId = npcId;
        var scriptText = spec.ScriptText;
        var enc = new UTF8Encoding(true);
        var scriptBytes = enc.GetPreamble().Concat(enc.GetBytes(scriptText)).ToArray();
        string fixtureSource = null;
        string scriptSha = null;
        if (spec.ScriptSourcePath != null)
        {
            var raw = File.ReadAllBytes(spec.ScriptSourcePath);
            // 真实语料是 GBK；入仓 synthetic 夹具是 UTF-8（无 BOM）⇒ 自动识别后按 GBK 写入。
            var gbk = Encoding.GetEncoding("gb2312");
            try
            {
                var strict = Encoding.GetEncoding(
                    "gb2312",
                    EncoderFallback.ExceptionFallback,
                    DecoderFallback.ExceptionFallback);
                var _ = strict.GetString(raw);
                scriptBytes = raw;
            }
            catch (DecoderFallbackException)
            {
                scriptBytes = gbk.GetBytes(Encoding.UTF8.GetString(raw));
            }
            fixtureSource = spec.ScriptSourcePath;
            scriptSha = Convert.ToHexString(SHA256.HashData(raw)).ToLowerInvariant();
        }
        File.WriteAllBytes(Path.Combine(tmp, "Npc_def", spec.ScriptName + ".txt"), scriptBytes);
        scriptSha ??= Convert.ToHexString(SHA256.HashData(scriptBytes)).ToLowerInvariant();
        SystemShare.Config.ClickNpcTime = spec.ClickNpcTimeConfig;

        // 走 oracle `FindMerchant(...) ?? FindNpc(...)` 的 **FindNpc 分支**（`Npcs.txt` 之外的
        // `AddQuestNpc` 型 NPC）；Rust 侧固定 NPC type0 为 `Merchant`，其 `Click` 覆写 = `base.Click`
        // （`Merchant.cs:1104`），点击面同形。
        var npc = new Merchant
        {
            ActorId = npcId,
            ChrName = "老人",
            Envir = envir,
            CurrX = (short)spec.NpcX,
            CurrY = (short)spec.NpcY,
            MapName = "0",
        };
        if (spec.MerchantScriptRel != null)
        {
            var mdPath = Path.Combine(tmp, "Market_Def", spec.MerchantScriptRel + ".txt");
            Directory.CreateDirectory(Path.GetDirectoryName(mdPath)!);
            File.WriteAllBytes(mdPath, scriptBytes);
            new ScriptParsers().LoadScriptFile(npc, "Market_Def", spec.MerchantScriptRel, true);
        }
        else
        {
            new ScriptParsers().LoadScript(npc, "Npc_def", spec.ScriptName);
        }
        SystemShare.ActorMgr.CleanObject();
        SystemShare.ActorMgr.Add(npc);
        SystemShare.WorldEngine = WorldProxy.Create(npc);

        var po = new PlayObject
        {
            Race = (byte)ActorRace.Play,
            ActorId = 0x1234,
            ChrName = "probe",
            Envir = envir,
            CurrX = (short)spec.PlayerX,
            CurrY = (short)spec.PlayerY,
            MapName = "0",
            IsCanDeal = true,
            Death = spec.Death,
            ItemList = new List<UserItem>(),
            MagicList = new List<UserMagic>(),
        };
        po.ClickNpcTime = spec.ClickNpcTime;
        if (spec.Mnm6 != 0) { po.MNMval[6] = spec.Mnm6; }
        if (spec.GameGold != 0) { po.GameGold = spec.GameGold; }
        if (spec.Quest106 != 0) { po.QuestFlag[13] |= 64; }

        var click = typeof(PlayObject).GetMethod("ClientClickNpc", BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingMethodException("PlayObject.ClientClickNpc");
        if (spec.Kind == "direct_goto")
        {
            M2Share.ScriptEngine.GotoLable(po, npcId, spec.Label, false);
        }
        else
        {
            click.Invoke(po, new object[] { npcId });
        }

        if (spec.Kind == "click_switch")
        {
            // 第一次点击的帧按真实消息泵排空丢弃，只记第二次点击（LastNpc/Script 切换面）。
            var preQ = GetMsgQueue(po);
            var preG = typeof(PlayObject).GetMethod("GetMessage", BindingFlags.Instance | BindingFlags.NonPublic | BindingFlags.Public,
                null, new[] { typeof(int), typeof(ProcessMessage).MakeByRefType() }, null);
            var preO = typeof(PlayObject).GetMethod("Operate", BindingFlags.Instance | BindingFlags.NonPublic | BindingFlags.Public,
                null, new[] { typeof(ProcessMessage) }, null);
            var preB = new object[] { HUtil32.GetTickCount(), default(ProcessMessage) };
            while ((bool)preG.Invoke(po, preB))
            {
                var pmS = (ProcessMessage)preB[1];
                if (!(bool)preO.Invoke(po, new object[] { pmS })) break;
                preB[0] = HUtil32.GetTickCount();
                preB[1] = default(ProcessMessage);
            }
            net.Frames.Clear();
            var npc2Id = _npcSeq++;
            var npc2 = new Merchant
            {
                ActorId = npc2Id,
                ChrName = spec.ScriptName2,
                Envir = envir,
                CurrX = (short)spec.NpcX,
                CurrY = (short)spec.NpcY,
                MapName = "0",
            };
            var text2 = spec.ScriptText2 ?? "[@main]\n王：第二页";
            var enc2 = new UTF8Encoding(true);
            File.WriteAllBytes(Path.Combine(tmp, "Npc_def", spec.ScriptName2 + ".txt"), enc2.GetPreamble().Concat(enc2.GetBytes(text2)).ToArray());
            new ScriptParsers().LoadScript(npc2, "Npc_def", spec.ScriptName2);
            SystemShare.ActorMgr.Add(npc2);
            SystemShare.WorldEngine = WorldProxy.Create(npc2);
            po.ClickNpcTime = 0;
            click.Invoke(po, new object[] { npc2Id });
            lastNpcId = npc2Id;
        }

        // NP-02：先点击打开对话（上面的 Click），再把点击产生的帧排空丢弃，然后选择标签。
        if (spec.Kind == "dlg_select" || spec.Kind == "dlg_sequence")
        {
            var pre = GetMsgQueue(po);
            var preGet = typeof(PlayObject).GetMethod("GetMessage", BindingFlags.Instance | BindingFlags.NonPublic | BindingFlags.Public,
                null, new[] { typeof(int), typeof(ProcessMessage).MakeByRefType() }, null);
            var preOp = typeof(PlayObject).GetMethod("Operate", BindingFlags.Instance | BindingFlags.NonPublic | BindingFlags.Public,
                null, new[] { typeof(ProcessMessage) }, null);
            var box = new object[] { HUtil32.GetTickCount(), default(ProcessMessage) };
            while ((bool)preGet.Invoke(po, box))
            {
                var pm0 = (ProcessMessage)box[1];
                if (!(bool)preOp.Invoke(po, new object[] { pm0 })) break;
                box[0] = HUtil32.GetTickCount();
                box[1] = default(ProcessMessage);
            }
            var dlg = typeof(PlayObject).GetMethod("ClientMerchantDlgSelect", BindingFlags.Instance | BindingFlags.NonPublic)
                ?? throw new MissingMethodException("PlayObject.ClientMerchantDlgSelect");
            var labels = spec.Kind == "dlg_sequence" ? spec.Labels : new[] { spec.Label };
            for (var li = 0; li < labels.Length; li++)
            {
                net.Frames.Clear();
                dlg.Invoke(po, new object[] { npcId, labels[li] });
                if (li == labels.Length - 1) { continue; } // 末步留给下面的主消息泵记录
                // 中间步：按真实消息泵排空（保留状态副作用），只记录最后一步的帧。
                var mid = GetMsgQueue(po);
                var midGet = typeof(PlayObject).GetMethod("GetMessage", BindingFlags.Instance | BindingFlags.NonPublic | BindingFlags.Public,
                    null, new[] { typeof(int), typeof(ProcessMessage).MakeByRefType() }, null);
                var midOp = typeof(PlayObject).GetMethod("Operate", BindingFlags.Instance | BindingFlags.NonPublic | BindingFlags.Public,
                    null, new[] { typeof(ProcessMessage) }, null);
                var midBox = new object[] { HUtil32.GetTickCount(), default(ProcessMessage) };
                while ((bool)midGet.Invoke(po, midBox))
                {
                    var pm1 = (ProcessMessage)midBox[1];
                    if (!(bool)midOp.Invoke(po, new object[] { pm1 })) break;
                    midBox[0] = HUtil32.GetTickCount();
                    midBox[1] = default(ProcessMessage);
                }
                net.Frames.Clear();
            }
        }

        // 真实消息泵：GetMessage 按优先级出队 → Operate 映射成 SM（`PlayObject.Message.cs:355-363`）。
        var pq = GetMsgQueue(po);
        var prioritiesBefore = QueuePriorities(pq);
        var flags = BindingFlags.Instance | BindingFlags.NonPublic | BindingFlags.Public;
        var getMessage = typeof(PlayObject).GetMethod(
            "GetMessage",
            flags,
            null,
            new[] { typeof(int), typeof(ProcessMessage).MakeByRefType() },
            null) ?? throw new MissingMethodException("PlayObject.GetMessage(int, ref ProcessMessage)");
        var operate = typeof(PlayObject).GetMethod(
            "Operate",
            flags,
            null,
            new[] { typeof(ProcessMessage) },
            null) ?? throw new MissingMethodException("PlayObject.Operate(ProcessMessage)");
        var rms = new List<object>();
        var boxed = new object[] { HUtil32.GetTickCount(), default(ProcessMessage) };
        while ((bool)getMessage.Invoke(po, boxed))
        {
            var pm = (ProcessMessage)boxed[1];
            rms.Add(new
            {
                order = rms.Count,
                ident = pm.wIdent,
                w_param = pm.wParam,
                n_param1 = pm.nParam1,
                n_param2 = pm.nParam2,
                n_param3 = pm.nParam3,
                actor_id = pm.ActorId,
                late = pm.LateDelivery,
                msg = pm.Msg,
            });
            var oa = new object[] { pm };
            if (!(bool)operate.Invoke(po, oa))
            {
                break;
            }
            boxed[0] = HUtil32.GetTickCount();
            boxed[1] = default(ProcessMessage);
        }

        var pqCount = (int)(pq.GetType().GetProperty("Count")?.GetValue(pq) ?? 0);
        var sm = net.Frames.Select((f, i) => new
        {
            order = i,
            ident = f.ident,
            recog_is_npc = f.recog == lastNpcId,
            param = f.param,
            tag = f.tag,
            series = f.series,
            len = f.len,
        }).ToArray();

        return new
        {
            name = spec.Name,
            fixture = fixtureSource == null ? null : new { source_path = fixtureSource, sha256 = scriptSha },
            script_name = spec.ScriptName,
            script_sha256 = scriptSha,
            ctx = BuildCtx(spec),
            rm = rms,
            rm_priorities = prioritiesBefore,
            max_drain_passes = 64,
            drain_passes = rms.Count,
            drain_hit_cap = pqCount > 0 || rms.Count >= 64,
            sm,
            state = BuildState(po, spec.Kind, lastNpcId),
        };
    }

    private static int Main(string[] args)
    {
        if (args.Length < 1)
        {
            Console.Error.WriteLine("usage: npc-golden <out_dir>");
            return 2;
        }
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        LogService.Logger = new LoggerConfiguration().CreateLogger();

        var tmp = Path.Combine(Path.GetTempPath(), "npc-golden-" + Environment.ProcessId);
        Directory.CreateDirectory(Path.Combine(tmp, "Npc_def"));
        // StringList 按 UTF-8 BOM 识别；内容与 Rust 侧夹具同源。
        SystemShare.Config.EnvirDir = tmp;

        var net = new CaptureNetChannel();
        M2Share.NetChannel = net;
        SystemShare.ItemSystem = new ProbeItemSystem();
        M2Share.ScriptEngine = new ScriptEngine();

        var envir = new Envirnoment();
        var clickCases = new List<object>
        {
            RunCase(new CaseSpec { Name = "click_main_ok" }, tmp, envir, net),
            RunCase(new CaseSpec { Name = "click_main_far", NpcX = 40, NpcY = 40 }, tmp, envir, net),
            RunCase(new CaseSpec { Name = "click_main_dead", Death = true }, tmp, envir, net),
            RunCase(new CaseSpec
            {
                Name = "click_main_rate_limited",
                ClickNpcTime = HUtil32.GetTickCount(),
                ClickNpcTimeConfig = 1000,
            }, tmp, envir, net),
        };
        clickCases.Add(RunCase(new CaseSpec
        {
            Name = "click_no_main_fallback",
            ScriptText = "[@greet]\nhi\r\n",
        }, tmp, envir, net));
        var dlgCases = new List<object>
        {
            RunCase(new CaseSpec { Name = "dlg_ask_ok", Kind = "dlg_select", Label = "@ask", ScriptText = DIALOG_SCRIPT }, tmp, envir, net),
            RunCase(new CaseSpec { Name = "dlg_back_to_main", Kind = "dlg_select", Label = "@back", ScriptText = DIALOG_SCRIPT }, tmp, envir, net),
            RunCase(new CaseSpec { Name = "dlg_unknown_label", Kind = "dlg_select", Label = "@nope", ScriptText = DIALOG_SCRIPT }, tmp, envir, net),
            RunCase(new CaseSpec { Name = "dlg_ask_back", Kind = "dlg_sequence", Labels = new[] { "@ask", "@back" }, ScriptText = DIALOG_SCRIPT }, tmp, envir, net),
        };
        var np03Cases = new List<object>
        {
            RunCase(new CaseSpec { Name = "menu_main", MerchantScriptRel = "封魔谷/恶魔修理师-d2008", ScriptSourcePath = MERCHANT_SCRIPT }, tmp, envir, net),
            RunCase(new CaseSpec { Name = "menu_buy", Kind = "dlg_select", Label = "@buy", MerchantScriptRel = "封魔谷/恶魔修理师-d2008", ScriptSourcePath = MERCHANT_SCRIPT }, tmp, envir, net),
            RunCase(new CaseSpec { Name = "menu_sell", Kind = "dlg_select", Label = "@sell", MerchantScriptRel = "封魔谷/恶魔修理师-d2008", ScriptSourcePath = MERCHANT_SCRIPT }, tmp, envir, net),
            RunCase(new CaseSpec { Name = "menu_storage", Kind = "dlg_sequence", Labels = new[] { "@cangku", "@storage" }, MerchantScriptRel = "封魔谷/恶魔修理师-d2008", ScriptSourcePath = MERCHANT_SCRIPT }, tmp, envir, net),
        };
        var np04Cases = new List<object>
        {
            RunCase(new CaseSpec { Name = "click_switch", Kind = "click_switch", ScriptName = "老人", ScriptText2 = "[@main]\n王：第二页" }, tmp, envir, net),
            RunCase(new CaseSpec { Name = "act_usercmd6_first_only", Kind = "direct_goto", Label = "@UserCmd6", ScriptName = "老人", ScriptSourcePath = QFUNCTION_SCRIPT, GameGold = 10 }, tmp, envir, net),
        };
        var np05Cases = new List<object>
        {
            RunCase(new CaseSpec { Name = "extjmp_allowed", Kind = "dlg_select", Label = "@sub", ScriptName = "老人", ScriptSourcePath = EXTJMP_FIXTURE }, tmp, envir, net),
            RunCase(new CaseSpec { Name = "extjmp_blocked", Kind = "dlg_select", Label = "@plain", ScriptName = "老人", ScriptSourcePath = EXTJMP_FIXTURE }, tmp, envir, net),
        };

        var outDir = args[0];
        Directory.CreateDirectory(outDir);
        WriteDoc(Path.Combine(outDir, "np01_click_main.json"), CLICK_SCRIPT, clickCases, CLICK_ORACLE_SOURCE);
        WriteDoc(Path.Combine(outDir, "np02_dialog_select.json"), DIALOG_SCRIPT, dlgCases, DIALOG_ORACLE_SOURCE);
        WriteDoc(Path.Combine(outDir, "np03_merchant_menu.json"), DIALOG_SCRIPT, np03Cases, DIALOG_ORACLE_SOURCE);
        WriteDoc(Path.Combine(outDir, "np04_main_selection.json"), CLICK_SCRIPT, np04Cases, CLICK_ORACLE_SOURCE);
        WriteDoc(Path.Combine(outDir, "np05_extjmp.json"), DIALOG_SCRIPT, np05Cases, DIALOG_ORACLE_SOURCE);
        Directory.Delete(tmp, true);
        return 0;
    }

    private static void WriteDoc(string path, string script, List<object> cases, string[] oracleSource)
    {
        var doc = new
        {
            generator = "mir2-rs/tests/parity/csharp/NpcGolden",
            oracle_source = oracleSource,
            script,
            max_drain_passes = 64,
            excluded_wallclock_fields = new[] { "ctx.click_npc_time" },
            cases,
        };
        File.WriteAllText(path, JsonSerializer.Serialize(doc, JsonOpts), new UTF8Encoding(false));
        Console.WriteLine("OK " + path);
    }
}
