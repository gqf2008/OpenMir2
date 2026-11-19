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
        if (targetMethod.Name == "FindNpc")
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

    private sealed class CaseSpec
    {
        public string Name;
        public int PlayerX = 10;
        public int PlayerY = 10;
        public int NpcX = 10;
        public int NpcY = 10;
        public bool Death;
        public int ClickNpcTime = 0;
        public int ClickNpcTimeConfig = 0;
    }

    private static object RunCase(CaseSpec spec, string scriptPath, Envirnoment envir, CaptureNetChannel net)
    {
        net.Frames.Clear();
        SystemShare.Config.ClickNpcTime = spec.ClickNpcTimeConfig;

        // 走 oracle `FindMerchant(...) ?? FindNpc(...)` 的 **FindNpc 分支**（`Npcs.txt` 之外的
        // `AddQuestNpc` 型 NPC）；Rust 侧固定 NPC type0 为 `Merchant`，其 `Click` 覆写 = `base.Click`
        // （`Merchant.cs:1104`），点击面同形。
        var npc = new NormNpc
        {
            ActorId = 5001,
            ChrName = "老人",
            Envir = envir,
            CurrX = (short)spec.NpcX,
            CurrY = (short)spec.NpcY,
            MapName = "0",
        };
        new ScriptParsers().LoadScript(npc, "Npc_def", "老人");
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

        var click = typeof(PlayObject).GetMethod("ClientClickNpc", BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingMethodException("PlayObject.ClientClickNpc");
        click.Invoke(po, new object[] { 5001 });

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
            recog_is_npc = f.recog == 5001,
            param = f.param,
            tag = f.tag,
            series = f.series,
            len = f.len,
        }).ToArray();

        return new
        {
            name = spec.Name,
            ctx = new
            {
                player_x = spec.PlayerX,
                player_y = spec.PlayerY,
                npc_x = spec.NpcX,
                npc_y = spec.NpcY,
                death = spec.Death,
                click_npc_time = spec.ClickNpcTime,
                click_npc_time_config = spec.ClickNpcTimeConfig,
            },
            rm = rms,
            rm_priorities = prioritiesBefore,
            max_drain_passes = 64,
            drain_passes = rms.Count,
            drain_hit_cap = pqCount > 0 || rms.Count >= 64,
            sm,
            state = new
            {
                last_npc_is_npc = po.LastNpc == 5001,
                script_curr_lable = po.ScriptCurrLable ?? string.Empty,
                script_go_back_lable = po.ScriptGoBackLable ?? string.Empty,
                script_lable = po.ScriptLable ?? string.Empty,
                script_selected = po.Script != null,
            },
        };
    }

    private static int Main(string[] args)
    {
        if (args.Length < 1)
        {
            Console.Error.WriteLine("usage: npc-golden <out.json>");
            return 2;
        }
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        LogService.Logger = new LoggerConfiguration().CreateLogger();

        var tmp = Path.Combine(Path.GetTempPath(), "npc-golden-" + Environment.ProcessId);
        Directory.CreateDirectory(Path.Combine(tmp, "Npc_def"));
        // StringList 按 UTF-8 BOM 识别；内容与 Rust 侧夹具同源。
        File.WriteAllText(Path.Combine(tmp, "Npc_def", "老人.txt"), "[@main]\n老人：你需要什么？\\ <打听/@ask> <离开/@exit>\r\n", new UTF8Encoding(true));
        SystemShare.Config.EnvirDir = tmp;

        var net = new CaptureNetChannel();
        M2Share.NetChannel = net;
        SystemShare.ItemSystem = new ProbeItemSystem();
        M2Share.ScriptEngine = new ScriptEngine();

        var envir = new Envirnoment();
        var cases = new List<object>
        {
            RunCase(new CaseSpec { Name = "click_main_ok" }, null, envir, net),
            RunCase(new CaseSpec { Name = "click_main_far", NpcX = 40, NpcY = 40 }, null, envir, net),
            RunCase(new CaseSpec { Name = "click_main_dead", Death = true }, null, envir, net),
            RunCase(new CaseSpec
            {
                Name = "click_main_rate_limited",
                ClickNpcTime = HUtil32.GetTickCount(),
                ClickNpcTimeConfig = 1000,
            }, null, envir, net),
        };

        var doc = new
        {
            generator = "mir2-rs/tests/parity/csharp/NpcGolden",
            oracle_source = new[]
            {
                "src/M2Server/Player/PlayObject.Base.cs:674 (ClientClickNpc)",
                "src/M2Server/Npc/NormNpc.cs:68 (Click -> GotoLable @main)",
                "src/Modules/ScriptEngine/ScriptEngine.cs:141 (GotoLable)",
                "src/Modules/ScriptEngine/ScriptEngine.cs:987 (GotoLableSendMerChantSayMsg)",
                "src/M2Server/Player/PlayObject.Message.cs:1702 (RM_MERCHANTSAY -> SM_MERCHANTSAY)",
            },
            script = "[@main]\n老人：你需要什么？\\ <打听/@ask> <离开/@exit>",
            max_drain_passes = 64,
            cases,
        };

        var outPath = args[0];
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outPath))!);
        File.WriteAllText(outPath, JsonSerializer.Serialize(doc, JsonOpts), new UTF8Encoding(false));
        Directory.Delete(tmp, true);
        Console.WriteLine("OK " + outPath);
        return 0;
    }
}
