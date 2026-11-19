// npc-aoi-golden —— 卡 openmir2-npc-multiobserve-golden：NPC 多观察者 AOI 金标准生成器。
//
// 复用 MagicGolden 的记录面（`M2Share.NetChannel` 只记录实现 + 玩家唯一 GateIdx 归集 +
// 生产 `Operate` 编码路径），把施动者换成 oracle 的 NPC（`GameSrv.Npc.Merchant`）。
//
// 观察者集合（每例 5 类 + 隐藏 NPC 行）：
//   self    —— 与 NPC 同格（施动者本身位置的玩家）
//   near    —— 同图 +5 格（视野内）
//   edge    —— 同图恰好 +SendRefMsgRange(12) 格（边界含）
//   out     —— 同图 +13 格（边界外）
//   other   —— 另一张地图（0100.map）上的玩家
//   hidden  —— 注册在 ActorMgr、`IsHide=true`、**不进任何格 obj_list** 的隐藏 NPC（部署事实：
//              0.map (0,0) 是 HighWall ⇒ AddToMap 不执行）——AOI 侧必须不可见
//
// 帧集合：`RM_TURN` → `SM_TURN(10)` + `SM_FEATURECHANGED(41)`（出现/转向）；
//         `RM_HIT` → `SM_HIT(14)`；`RM_DISAPPEAR` → `SM_DISAPPEAR(30)`。
// 每接收者记录：`(ident,param,tag,series,tail)` 逐帧 + RM 账本（ident/wParam/p1..p3/msg/late/priority）
// + `VisibleActors`（SearchViewRange 结果，AOI 可见面）+ `DrainPasses`/`DrainPassesHitCap`。
//
// 只读：不连 MySQL、不写现网文件、不起 socket；地图取自部署件 `Map/*.map`（可用 NPCAOI_MAP_DIR 覆盖）。
//
// 用法（仓库根）：
//   dotnet run --project mir2-rs/tests/parity/csharp/NpcAoiGolden -c Release -- <out_dir>

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading;
using System.Threading.Tasks;
using GameSrv.Npc;
using M2Server;
using M2Server.Maps;
using M2Server.Net;
using M2Server.Player;
using OpenMir2;
using OpenMir2.Data;
using OpenMir2.Packets.ServerPackets;
using Serilog;
using SystemModule;
using SystemModule.Actors;
using SystemModule.Data;

namespace NpcAoiGolden;

internal static class Program
{
    private const int MaxDrainPasses = 8;

    private const int NpcId = 6002;
    private const int HiddenNpcId = 6001;
    private const int SelfId = 5001;
    private const int NearId = 5002;
    private const int EdgeId = 5003;
    private const int OutId = 5004;
    private const int OtherId = 5005;

    private static readonly JsonSerializerOptions JsonOpts = new()
    {
        WriteIndented = true,
        Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
    };

    private static int Main(string[] args)
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        LogService.Logger = new LoggerConfiguration().CreateLogger();
        // 生产 `GetChrColor` 会读 `SystemShare.CastleMgr`（`PlayObject.cs` 的 InCastleWarArea）；
        // 空表实现与"无城"等价（部署测试位不在任何城堡区）。
        SystemShare.CastleMgr = new M2Server.Castle.CastleManager();
        var mapDir = Environment.GetEnvironmentVariable("NPCAOI_MAP_DIR") ?? @"E:\MirServer\M2GameSvr\Map";
        if (!File.Exists(Path.Combine(mapDir, "0.map")))
        {
            Console.Error.WriteLine("MAP_MISSING " + mapDir);
            return 2;
        }
        var outDir = args.Length > 0 ? args[0] : Path.Combine(Path.GetTempPath(), "npc-aoi-golden");
        Directory.CreateDirectory(outDir);

        var doc = new Dictionary<string, object>
        {
            ["generator"] = "mir2-rs/tests/parity/csharp/NpcAoiGolden",
            ["oracle_source"] = new[]
            {
                "src/M2Server/Actor/BaseObject.Message.cs:383 (SendRefMsg: ±SendRefMsgRange 扫格 → Race==Play)",
                "src/GameSrv/Npc/Merchant.cs:132/140 (RM_TURN/RM_HIT)",
                "src/M2Server/Player/PlayObject.Message.cs:1436 (RM_HIT -> SM_HIT)",
                "src/M2Server/Player/PlayObject.Message.cs:1542/1558 (RM_TURN -> SM_TURN + SM_FEATURECHANGED)",
                "src/GameSrv/Word/WorldServer.Merchant.cs:47 (AddtoMapSuccess && !IsHide)",
            },
            ["max_drain_passes"] = MaxDrainPasses,
            ["excluded_wallclock_fields"] = new[] { "delivery", "send_ref_msg_tick" },
            ["cases"] = new List<object> { RunCase(mapDir) },
        };
        var path = Path.Combine(outDir, "smoke.json");
        File.WriteAllText(path, JsonSerializer.Serialize(doc, JsonOpts), new UTF8Encoding(false));
        Console.WriteLine("OK " + path);
        return 0;
    }

    /// <summary>在 [x0..x0+span] 里找首个可走格（`CanWalk(...,true)`），返回 (x,y)。</summary>
    private static (short x, short y) FirstWalkable(Envirnoment env, short x0, short y0, int span)
    {
        // 先扫给定窗口，扫不到就整图扫（异图观察者只需一个可走格）。
        for (var attempt = 0; attempt < 2; attempt++)
        {
            var (sx, sy, ex, ey) = attempt == 0
                ? (x0, y0, (short)(x0 + span), (short)(y0 + span))
                : ((short)0, (short)0, env.Width, env.Height);
            if (TryScan(env, sx, sy, ex, ey, out var hit))
            {
                return hit;
            }
        }
        throw new InvalidOperationException("NO_WALKABLE_CELL");
    }

    private static bool TryScan(Envirnoment env, short sx, short sy, short ex, short ey, out (short x, short y) hit)
    {
        hit = default;
        for (short x = sx; x < ex; x++)
        {
            for (short y = sy; y < ey; y++)
            {
                if (env.CellMatch(x, y) && env.CanWalk(x, y, true))
                {
                    hit = (x, y);
                    return true;
                }
            }
        }
        return false;
    }

    /// <summary>
    /// 找一条同 y 的水平带：基格 + 偏移 `{0,5,12,13}` **全部可走**（观察者几何必须是行为输入，
    /// 不能一边超界/落墙）。
    /// </summary>
    private static (short x, short y) FirstStrip(Envirnoment env, short x0, short y0, int span)
    {
        foreach (short dx in new short[] { 0, 5, 12, 13 })
        {
            if (env.Width <= x0 + span + 13)
            {
                throw new InvalidOperationException("MAP_TOO_NARROW");
            }
        }
        for (short x = x0; x < x0 + span; x++)
        {
            for (short y = y0; y < y0 + span; y++)
            {
                var ok = true;
                foreach (var off in new short[] { 0, 5, 12, 13 })
                {
                    var (cx, cy) = ((short)(x + off), y);
                    if (!env.CellMatch(cx, cy) || !env.CanWalk(cx, cy, true))
                    {
                        ok = false;
                        break;
                    }
                }
                if (ok)
                {
                    return (x, y);
                }
            }
        }
        throw new InvalidOperationException("NO_WALKABLE_STRIP");
    }

    private static object RunCase(string mapDir)
    {
        var env = new Envirnoment();
        if (!env.LoadMapData(Path.Combine(mapDir, "0.map")))
        {
            throw new InvalidOperationException("MAP_LOAD_FAILED 0.map");
        }
        var envOther = new Envirnoment();
        if (!envOther.LoadMapData(Path.Combine(mapDir, "0100.map")))
        {
            throw new InvalidOperationException("MAP_LOAD_FAILED 0100.map");
        }
        var (nx, ny) = FirstStrip(env, 300, 300, 40);

        var recorder = new RecordingNetChannel();
        M2Share.NetChannel = recorder;

        RecordingPlayObject MakeObserver(int actorId, string name, Envirnoment e, short x, short y)
        {
            var p = new RecordingPlayObject
            {
                ChrName = name,
                ActorId = actorId,
                CurrX = x,
                CurrY = y,
                SoftVersionDateEx = 20240101,
                ClientTick = 5,
            };
            p.GateIdx = actorId;
            p.FixedHideMode = false;
            p.ObMode = false;
            p.Envir = e;
            SystemShare.ActorMgr.Add(p);
            if (!e.AddMapObject(x, y, CellType.Play, actorId, p))
            {
                throw new InvalidOperationException("ADD_MAP_OBJECT_FAILED " + name);
            }
            return p;
        }

        var self = MakeObserver(SelfId, "golden-self", env, nx, ny);
        var near = MakeObserver(NearId, "golden-near", env, (short)(nx + 5), ny);
        var edge = MakeObserver(EdgeId, "golden-edge", env, (short)(nx + 12), ny);
        var outOf = MakeObserver(OutId, "golden-out", env, (short)(nx + 13), ny);
        var (ox, oy) = FirstWalkable(envOther, 100, 100, 20);
        var other = MakeObserver(OtherId, "golden-other", envOther, ox, oy);

        // 施动 NPC（`new Merchant`：`LocalDB.QMangeNpc` 的形态）。
        var npc = new Merchant
        {
            ChrName = "QManage",
            ActorId = NpcId,
            CurrX = nx,
            CurrY = ny,
            Envir = env,
            MapName = "0",
        };
        SystemShare.ActorMgr.Add(npc);
        if (!env.AddMapObject(nx, ny, CellType.Merchant, NpcId, npc))
        {
            throw new InvalidOperationException("ADD_NPC_FAILED");
        }

        // 隐藏 NPC：注册进 ActorMgr、`IsHide=true`、**不进格**（部署件 (0,0) 是 HighWall）。
        var hidden = new Merchant
        {
            ChrName = "RobotManage",
            ActorId = HiddenNpcId,
            CurrX = 0,
            CurrY = 0,
            Envir = env,
            MapName = "0",
            IsHide = true,
        };
        SystemShare.ActorMgr.Add(hidden);

        // 施动次序与 oracle 的发帧点同形：出现（AddToMap 的 RM_TURN）、转向、受击、消失。
        npc.SendRefMsg(Messages.RM_TURN, 4, nx, ny, 0, "");
        npc.SendRefMsg(Messages.RM_TURN, 6, nx, ny, 0, "");
        npc.SendRefMsg(Messages.RM_HIT, 4, nx, ny, 0, "");
        npc.SendRefMsg(Messages.RM_DISAPPEAR, 0, 0, 0, 0, "");

        // 每个观察者做一次 SearchViewRange（AOI 可见面），再按生产 `Operate` 排干队列。
        var observers = new (string role, RecordingPlayObject po, Envirnoment env, string map)[]
        {
            ("self", self, env, "0"),
            ("near", near, env, "0"),
            ("edge", edge, env, "0"),
            ("out", outOf, env, "0"),
            ("other_map", other, envOther, "0100"),
        };
        var rows = new List<object>();
        foreach (var (role, po, e, map) in observers)
        {
            if (Environment.GetEnvironmentVariable("NPCAOI_SKIP_SCAN") != "1") po.SearchViewRange();
            var visible = po.VisibleActors
                .Select(v => v.BaseObject)
                .Where(b => b != null)
                .Select(b => b.ActorId)
                .OrderBy(id => id)
                .ToArray();
            var (rm, passes, hitCap) = DrainReceiver(po);
            rows.Add(new
            {
                role,
                actor_id = po.ActorId,
                x = po.CurrX,
                y = po.CurrY,
                map,
                gate = po.GateIdx,
                visible_actors = visible,
                hidden_visible = visible.Contains(HiddenNpcId),
                drain_passes = passes,
                drain_hit_cap = hitCap,
                rm,
                frames = ParseFrames(recorder.Buffers, po.GateIdx),
            });
        }

        return new
        {
            name = "smoke_turn_hit_disappear",
            npc = new { actor_id = NpcId, chr_name = npc.ChrName, x = nx, y = ny, is_hide = false },
            hidden_npc = new
            {
                actor_id = HiddenNpcId,
                chr_name = hidden.ChrName,
                x = hidden.CurrX,
                y = hidden.CurrY,
                is_hide = true,
                on_map = false,
            },
            send_ref_msg_range = 12,
            observers = rows,
        };
    }

    /// <summary>「抽空为止」：反复抽 RM → 生产 `Operate`，直到队列空或命中上限。</summary>
    private static (List<object> rm, int passes, bool hitCap) DrainReceiver(RecordingPlayObject po)
    {
        var all = new List<object>();
        var passes = 0;
        var hitCap = false;
        while (true)
        {
            var batch = po.DrainRaw();
            if (batch.Count == 0)
            {
                break;
            }
            if (passes >= MaxDrainPasses)
            {
                hitCap = true;
                break;
            }
            passes++;
            foreach (var (m, prio) in batch)
            {
                all.Add(new
                {
                    ident = m.wIdent,
                    w_param = m.wParam,
                    p1 = m.nParam1,
                    p2 = m.nParam2,
                    p3 = m.nParam3,
                    msg = m.Buff ?? "",
                    late = m.LateDelivery,
                    delivery = m.LateDelivery ? m.DeliveryTime : 0,
                    priority = prio,
                });
                po.OperatePublic(m);
            }
        }
        return (all, passes, hitCap);
    }

    /// <summary>缓冲 = 20B `ServerMessage` 头 + 载荷；逐帧解码（同 MagicGolden 口径）。</summary>
    private static List<object> ParseFrames(List<(int GateIdx, byte[] Data)> buffers, int gateIdx)
    {
        var list = new List<object>();
        foreach (var (g, data) in buffers)
        {
            if (g != gateIdx)
            {
                continue;
            }
            int packLength = BitConverter.ToInt32(data, 16);
            if (packLength < 0)
            {
                string action = Encoding.GetEncoding("GB2312").GetString(
                    data, ServerMessage.PacketSize, data.Length - ServerMessage.PacketSize);
                list.Add(new { ident = 0, param = 0, tag = 0, series = 0, tail = action });
                continue;
            }
            int off = ServerMessage.PacketSize;
            ushort ident = BitConverter.ToUInt16(data, off + 4);
            ushort param = BitConverter.ToUInt16(data, off + 6);
            ushort tag = BitConverter.ToUInt16(data, off + 8);
            ushort series = BitConverter.ToUInt16(data, off + 10);
            string tail = data.Length > off + 12
                ? Encoding.GetEncoding("GB2312").GetString(data, off + 12, data.Length - off - 12)
                : "";
            list.Add(new { ident, param, tag, series, tail });
        }
        return list;
    }
}

/// <summary>`M2Share.NetChannel` 只记录实现：三处 `SendSocket` 重载的唯一汇合点。</summary>
internal sealed class RecordingNetChannel : INetChannel
{
    public List<(int GateIdx, byte[] Data)> Buffers { get; } = new();

    public void AddGateBuffer(int gateIdx, byte[] senData) => Buffers.Add((gateIdx, senData));
    public void AddGameGateQueue(int gateIdx, ServerMessage packet, byte[] data) => Buffers.Add((gateIdx, data));
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

/// <summary>只记录不发送的玩家：暴露 `MsgQueue` 抽取与生产 `Operate`。</summary>
internal sealed class RecordingPlayObject : PlayObject
{
    public List<(SendMessage Msg, byte Prio)> DrainRaw()
    {
        var list = new List<(SendMessage, byte)>();
        while (MsgQueue.TryDequeue(out var m, out var prio))
        {
            list.Add((m, prio));
        }
        return list;
    }

    public void OperatePublic(SendMessage m)
    {
        var msg = new ProcessMessage
        {
            ActorId = m.ActorId,
            wIdent = m.wIdent,
            wParam = m.wParam,
            nParam1 = m.nParam1,
            nParam2 = m.nParam2,
            nParam3 = m.nParam3,
            LateDelivery = m.LateDelivery,
            Msg = m.Buff ?? "",
        };
        Operate(msg);
    }
}
