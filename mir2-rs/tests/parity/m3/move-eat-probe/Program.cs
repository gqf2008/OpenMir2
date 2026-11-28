// move-eat-probe —— 物品线 move 组（IT-09 回城卷 / IT-10 随机传送 / IT-11 地牢逃脱）的 oracle 真身向量。
//
// 判定面见 `docs/issues/openmir2-items-parity.md` §9.2b（终态 MapName/X/Y + Rng 逐次 + 目标点分支）。
// 本探针与 eat-probe 的分工：
//   * eat-probe 覆盖 `use_item` 纯决策面（帧/通知/背包迁移），Envir 是空图，**不覆盖位置搬运**；
//   * 本探针覆盖 `EatItems → EatUseItems → BaseObjectMove` 的移动面：真 `Envirnoment`
//     （合成真格式 `.map`，52B 头 + 12B/格）、真 `SystemShare.MapMgr` 替身（只做 FindMap 注册表）、
//     真 `SystemShare.ActorMgr`、真 `PlayObject.ClientUseItems`（反射调私有）。
//
// 记录面（每 case）：
//   - 即时帧：`SendDefMessage` 直接写 gate buffer（`INetChannel.AddGateBuffer` 截获）⇒ 635/636；
//   - 延时通知：`MsgQueue`（PriorityQueue，priority=(byte)wIdent）按出队序读出（含 priority/late）；
//   - pump：把 RM_SPACEMOVE_FIRE(8062) / RM_WEIGHTCHANGED(10115) 逐条喂 oracle `Operate`
//     ⇒ 玩家可见的 `SM_SPACEMOVE_HIDE(800)` / `SM_WEIGHTCHANGED(622)`（**按线上序**：635 直发在前，
//     这两条由队列泵出在后）。633/634/801（CLEAROBJECTS/CHANGEMAP/SPACEMOVE_SHOW）留队列不 pump：
//     Rust 宿主由 world tick 的 `drain_map_change` 出，不在本卡对拍面（卡面登记）；
//   - 终态 `MapName/CurrX/CurrY`、`ItemList`、`WAbil.Weight`；
//   - `System.Random` 逐次账本（`RandomNumber.random` 私有静态字段替换为录制包装，**固定种子**）。
//
// **种子时点**：`new PlayObject()` 自身有 5 抽（卡 openmir2-player-ctor-rng-order）⇒ 必须先建对象、
// 后 `SeedRng(seed)`，再调 `ClientUseItems`——这样账本只含本次移动的抽数，与 Rust 宿主
// （直接构造实体、`world.rng = RandomNumber::with_seed(seed)` 后调 `handle_eat_cmd`）逐次对齐。
//
// **home 三元组来源（重要）**：oracle IT-09/11 读的是 `PlayObject.HomeMap/HomeX/HomeY` 字段，
// 该字段由登录（`PlayObject.cs:1199`）与每 3s `CheckDupObjTick`（`PlayObject.Message.cs:266`）的
// `GetStartPoint()`（`PlayObject.Base.cs:704-721`）按**当前地图**在 `StartPointList` 里取
// （`PvpLevel>=2` 走 `RedHome*`）——**不是** `[Setup] HomeMap/HomeX/HomeY` 常量。
// 本探针离线复现的是「字段已经由那条链路填好之后」的吃物品行为 ⇒ golden 的 `ctx.home/red_home`
// 直接记录喂进去的值；真实链路里它们来自 login + 3s GetStartPoint。
//
// 用法（**在仓库根运行**）：
//   dotnet run --project mir2-rs/tests/parity/m3/move-eat-probe -c Release -- <输出路径>

using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using M2Server;
using M2Server.Items;
using M2Server.Maps;
using M2Server.Net;
using M2Server.Player;
using OpenMir2;
using OpenMir2.Data;
using OpenMir2.Enums;
using OpenMir2.Packets.ClientPackets;
using OpenMir2.Packets.ServerPackets;
using Serilog;
using SystemModule;
using SystemModule.Actors;
using SystemModule.Data;
using SystemModule.Maps;
using SystemModule.SubSystem;

namespace MoveEatProbe;

/// <summary>截获到的一条帧（`CommandMessage` 在 `ServerMessage` 头之后）。</summary>
internal sealed class CapFrame
{
    public int ident;
    public int recog;
    public int param;
    public int tag;
    public int series;
    public int len;
    /// <summary>32B 头之后的载荷（hex；634 的 MapFileName / 801 的 CharDesc 用）。</summary>
    public string tail;
}

/// <summary>截获 `PlayObject` 真正发出的帧（`M2Share.NetChannel.AddGateBuffer`）。</summary>
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
            tail = senData.Length > 32 ? Convert.ToHexString(senData, 32, senData.Length - 32).ToLowerInvariant() : "",
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
    public System.Threading.Tasks.Task Start(System.Threading.CancellationToken cancellationToken = default)
        => System.Threading.Tasks.Task.CompletedTask;
    public void Send(string connectId, byte[] buff) { }
    public void CloseGate(string connectionId, string endPoint) { }
    public System.Threading.Tasks.Task StopAsync(System.Threading.CancellationToken cancellationToken = default)
        => System.Threading.Tasks.Task.CompletedTask;
}

/// <summary>`SystemShare.CastleMgr` 的最小替身：`GetChrColor`（`PlayObject.cs:3613`）在
/// `SM_SPACEMOVE_SHOW` 的收件人着色里调 `InCastleWarArea`；离线探针无城堡 ⇒ 返回 null（无城堡区域）。</summary>
internal class StubProxy : DispatchProxy
{
    public static T Create<T>() where T : class => DispatchProxy.Create<T, StubProxy>();

    protected override object Invoke(MethodInfo targetMethod, object[] args)
    {
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
    /// <summary>`System.Random` 的记录包装：行式账本 `seq|kind|arg|value|site`。
    /// `site` = **oracle 代码调用点**（栈上第一个非 `RecordingRandom`/`Random`/`RandomNumber` 的方法，
    /// 如 `BaseObject.MapRandomMove`）——「抽了几次、从哪抽」都由记录器自己证，不从结果反推。</summary>
    private sealed class RecordingRandom : Random
    {
        public readonly List<string> Draws = new();
        public RecordingRandom(int seed) : base(seed) { }

        private static string CallSite()
        {
            var frames = new System.Diagnostics.StackTrace(false).GetFrames();
            if (frames != null)
            {
                foreach (var f in frames)
                {
                    var m = f.GetMethod();
                    if (m == null)
                    {
                        continue;
                    }
                    var t = m.DeclaringType?.Name ?? string.Empty;
                    if (t is "RecordingRandom" or "Random" or "RandomNumber")
                    {
                        continue;
                    }
                    return t + "." + m.Name;
                }
            }
            return string.Empty;
        }

        private void Log(string kind, long arg, long value, string site)
        {
            Draws.Add($"{Draws.Count + 1}|{kind}|{arg}|{value}|{site}");
        }

        public override int Next()
        {
            int v = base.Next();
            Log("Next()", 0, v, CallSite());
            return v;
        }

        public override int Next(int maxValue)
        {
            int v = base.Next(maxValue);
            Log("Next(max)", maxValue, v, CallSite());
            return v;
        }

        public override int Next(int minValue, int maxValue)
        {
            int v = base.Next(minValue, maxValue);
            Log("Next(min,max)", ((long)minValue << 32) | (uint)maxValue, v, CallSite());
            return v;
        }
    }

    private static FieldInfo _rngField;

    /// <summary>把 `RandomNumber.random`（私有静态）换成固定种子录制包装（同 map-effect-probe 手法）。</summary>
    private static void SeedRng(int seed)
    {
        RandomNumber.GetInstance();
        _rngField ??= typeof(RandomNumber).GetField("random", BindingFlags.NonPublic | BindingFlags.Static)
            ?? throw new InvalidOperationException("找不到 OpenMir2.RandomNumber.random 私有静态字段");
        _rngField.SetValue(null, new RecordingRandom(seed));
    }

    private static List<object> Draws()
        => ((RecordingRandom)_rngField.GetValue(null)).Draws
            .Select(line =>
            {
                var parts = line.Split('|');
                return (object)new
                {
                    seq = int.Parse(parts[0], CultureInfo.InvariantCulture),
                    kind = parts[1],
                    arg = long.Parse(parts[2], CultureInfo.InvariantCulture),
                    value = long.Parse(parts[3], CultureInfo.InvariantCulture),
                    site = parts[4],
                };
            })
            .ToList();

    /// <summary>最小 `IMapSystem` 替身：只实现 `FindMap` 注册表（其余 no-op）。
    /// 效果路径 `BaseObjectMove→MapRandomMove/SpaceMove` 全部走真身 M2Server 代码。</summary>
    private sealed class ProbeMapSystem : IMapSystem
    {
        private readonly Dictionary<string, IEnvirnoment> _maps = new(StringComparer.OrdinalIgnoreCase);

        public void Register(IEnvirnoment envir) => _maps[envir.MapName] = envir;

        public IList<IEnvirnoment> Maps => _maps.Values.ToList();
        public void AddMapInfo(string sMapName, string sMapDesc, byte nServerNumber, MapInfoFlag mapFlag, IMerchant questNpc) { }
        public bool AddMapRoute(string sSMapNo, int nSMapX, int nSMapY, string sDMapNo, int nDMapX, int nDMapY) => false;
        public IEnvirnoment FindMap(string sMapName)
            => _maps.TryGetValue(sMapName ?? string.Empty, out IEnvirnoment e) ? e : null;
        public IList<IEnvirnoment> GetDoorMapList() => new List<IEnvirnoment>();
        public IEnvirnoment GetMapInfo(int nServerIdx, string sMapName) => FindMap(sMapName);
        public int GetMapOfServerIndex(string sMapName) => 0;
        public IList<IEnvirnoment> GetMineMaps() => new List<IEnvirnoment>();
        public void LoadMapDoor() { }
        public void MakeSafePkZone() { }
    }

    // ---- 合成真格式 .map（52B 头 + 12B/格，列主序；与 Envirnoment.LoadMapData 一致）------------------

    private static void WriteMap(string path, short width, short height, ISet<(short X, short Y)> walls)
    {
        using var fs = new FileStream(path, FileMode.Create, FileAccess.Write);
        using var bw = new BinaryWriter(fs);
        var head = new byte[52];
        head[0] = (byte)(width & 0xFF);
        head[1] = (byte)((width >> 8) & 0xFF);
        head[2] = (byte)(height & 0xFF);
        head[3] = (byte)((height >> 8) & 0xFF);
        bw.Write(head);
        for (short nW = 0; nW < width; nW++)
        {
            for (short nH = 0; nH < height; nH++)
            {
                ushort wBk = walls.Contains((nW, nH)) ? (ushort)0x8000 : (ushort)0;
                bw.Write(wBk);
                bw.Write((ushort)0);
                bw.Write((ushort)0);
                bw.Write((byte)0);
                bw.Write((byte)0);
                bw.Write((byte)0);
                bw.Write((byte)0);
                bw.Write((byte)0);
                bw.Write((byte)0);
            }
        }
    }

    /// <summary>`MapFileName` = **MapInfo 的取图文件名（文件主干，不带 `.map`）**——oracle
    /// `MapManager.AddMapInfo`（`src/GameSrv/Maps/MapManager.cs:66-96`）把 `sMapFileName`（`[名|文件]`
    /// 的 `|` 右半，或 `<...>`，缺省 = 逻辑名）赋给 `Envirnoment.MapFileName`，**不是**物理文件名
    /// （`sMapFileName + ".map"` 才是加载路径）。本夹具的逻辑名 == 文件主干。</summary>
    private static Envirnoment NewMap(string path, string mapName, string mapFileStem)
    {
        var envir = new Envirnoment();
        if (!envir.LoadMapData(path))
        {
            throw new InvalidDataException($"真身 LoadMapData 失败: {path}");
        }
        envir.MapName = mapName;
        envir.MapFileName = mapFileStem;
        envir.ServerIndex = 0;
        return envir;
    }

    // ---- 反射：MsgQueue / Operate ----------------------------------------------------------------

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

    private const int MaxDrainPasses = 8;

    private static (List<object> messages, List<object> raw, int passes, bool hitCap) Drain(object pq)
    {
        var list = new List<object>();
        var raw = new List<object>();
        var t = pq.GetType();
        var countProp = t.GetProperty("Count") ?? throw new MissingMemberException("PriorityQueue.Count");
        var tryDequeue = t.GetMethod("TryDequeue") ?? throw new MissingMethodException("PriorityQueue.TryDequeue");
        var passes = 0;
        var hitCap = false;
        if ((int)countProp.GetValue(pq) > 0)
        {
            for (var pass = 0; pass < MaxDrainPasses; pass++)
            {
                while ((int)countProp.GetValue(pq) > 0)
                {
                    var args = new object[] { null, null };
                    if (!(bool)tryDequeue.Invoke(pq, args))
                    {
                        break;
                    }
                    var m = args[0];
                    raw.Add(m);
                    int prio = Convert.ToInt32(args[1]);
                    var mt = m.GetType();
                    list.Add(new
                    {
                        ident = (int)mt.GetField("wIdent").GetValue(m),
                        w_param = (int)mt.GetField("wParam").GetValue(m),
                        n_param1 = (int)mt.GetField("nParam1").GetValue(m),
                        n_param2 = (int)mt.GetField("nParam2").GetValue(m),
                        n_param3 = (int)mt.GetField("nParam3").GetValue(m),
                        priority = prio,
                        late = ((bool)mt.GetField("LateDelivery").GetValue(m)) ? 1 : 0,
                    });
                }
                passes = pass + 1;
                hitCap = (int)countProp.GetValue(pq) > 0;
                if (!hitCap)
                {
                    break;
                }
            }
        }
        return (list, raw, passes, hitCap);
    }

    // ---- 用例 -----------------------------------------------------------------------------------

    private sealed class Ctx
    {
        public string Name;
        public ushort Index;
        public int MakeIndex = 1;
        /// <summary>`PlayObject.HomeMap/HomeX/HomeY`（登录 + 3s GetStartPoint 之后的字段值）。</summary>
        public string HomeMap;
        public short HomeX;
        public short HomeY;
        /// <summary>当前 `Envir.MapName`（`BaseObjectMove(MapName,0,0)` 的源）。</summary>
        public string CurMap = "move_map";
        public short CurX = 10;
        public short CurY = 10;
        /// <summary>`PlayObject.PkPoint`（`PvpLevel() = (byte)(PkPoint/100)`）。</summary>
        public int PkPoint;
        /// <summary>`Envir.Flag.boNORANDOMMOVE`（IT-10 的两支）。</summary>
        public bool NoRandom;
        public int Seed = 20261120;
        /// <summary>目标点是否**精确**（`sX!=0 && sY!=0` ⇒ SpaceMove 精确点、0 抽）；仅自检/ctx 用。</summary>
        public bool Precise;
        /// <summary>`SystemShare.Config.RedHome*`（逐例可换值 ⇒ 证明实现读快照而不是写死）。</summary>
        public string RedMap = "red_map";
        public short RedX = 20;
        public short RedY = 30;
        /// <summary>用例说明（写进 golden ctx；说明输入来源与「误用会怎样」的反例期望）。</summary>
        public string Note;
    }

    private static UserItem ToUser(Ctx c)
        => new UserItem { MakeIndex = c.MakeIndex, Index = c.Index, Dura = 10, DuraMax = 20 };

    private static object ItemDto(UserItem u)
    {
        if (u == null)
        {
            return null;
        }
        var desc = new int[14];
        for (int i = 0; i < 14; i++)
        {
            desc[i] = u.Desc[i];
        }
        return new { make_index = u.MakeIndex, index = (int)u.Index, dura = (int)u.Dura, dura_max = (int)u.DuraMax, desc };
    }

    private static object RunCase(GameItemSystem table, CaptureNetChannel net, string tmpDir,
        string mapFileMove, string mapFileHome, string mapFileRed, Ctx c)
    {
        // 每 case 独立 Envir 实例（避免上一 case 的对象留在格子里污染 ±12 扫描/换图）。
        var envirCur = NewMap(Path.Combine(tmpDir, mapFileMove), c.CurMap, Path.GetFileNameWithoutExtension(mapFileMove));
        var envirHome = NewMap(Path.Combine(tmpDir, mapFileHome), "home_map", Path.GetFileNameWithoutExtension(mapFileHome));
        var envirRed = NewMap(Path.Combine(tmpDir, mapFileRed), c.RedMap, Path.GetFileNameWithoutExtension(mapFileRed));
        // 夹具字段点亮（① 档 IT-09/10/11，2026-11-28）：`Envir.MapDesc` 非空且**逐图不同**——
        // 54 尾 = EDCode(GBK(落地目标图的 MapDesc))（`PlayObject.Base.cs:887-895`），逐图不同才能
        // 证明尾取自**目标图** Envir 而非常量；4 字与冻结样例（`[jiaren|0150 家人回归]`）同长，纯合成文案。
        envirCur.MapDesc = "移动之图";
        envirHome.MapDesc = "归家之图";
        envirRed.MapDesc = "红名之图";
        // 夹具字段点亮：`DARK`（boDarkness=true）⇒ `DayBright()=1`（`PlayObject.Base.cs:250-253`，
        // DARK 臂在 Bright/墙钟之前 ⇒ 两侧确定值、不依赖探针墙钟/日相；冻结 MapInfo.txt 有
        // 4 张 DARK 图 ⇒ 生产可达，非 MUSIC 式不可观测）。
        envirCur.Flag.DayLight = false;
        envirCur.Flag.boDarkness = true;
        envirHome.Flag.DayLight = false;
        envirHome.Flag.boDarkness = true;
        envirRed.Flag.DayLight = false;
        envirRed.Flag.boDarkness = true;
        var mapSys = new ProbeMapSystem();
        mapSys.Register(envirCur);
        mapSys.Register(envirHome);
        mapSys.Register(envirRed);
        SystemShare.MapMgr = mapSys;

        var po = new PlayObject
        {
            Race = ActorRace.Play,
            ActorId = 930000 + (int)c.Index,
            ChrName = "move_probe",
            IsRobot = false,
            OffLineFlag = false,
            Envir = envirCur,
            MapName = c.CurMap,
            CurrX = c.CurX,
            CurrY = c.CurY,
            HomeMap = c.HomeMap,
            HomeX = c.HomeX,
            HomeY = c.HomeY,
            PkPoint = c.PkPoint,
            // 801 `Series` 低字节旋钮（`BaseObject.cs:378` ctor `Dir=4` ⇒ 旋钮改 5）：oracle
            // `SpaceMove` SendRefMsg(RM_SPACEMOVE_SHOW, Dir, …) 取**实体活值**，非 ctor 常量。
            Dir = 5,
            CellType = CellType.Play,
            ItemList = new List<UserItem>(),
            MagicList = new List<UserMagic>(),
            Abil = new Ability { Level = 20, HP = 100, MP = 100, MaxHP = 100, MaxMP = 100 },
            WAbil = new Ability { Level = 20, HP = 100, MP = 100, MaxHP = 100, MaxMP = 100 },
        };
        po.ItemList.Add(ToUser(c));
        envirCur.Flag.boNORANDOMMOVE = c.NoRandom;
        SystemShare.Config.RedHomeMap = c.RedMap;
        SystemShare.Config.RedHomeX = c.RedX;
        SystemShare.Config.RedHomeY = c.RedY;
        if (!envirCur.AddMapObject(c.CurX, c.CurY, CellType.Play, po.ActorId, po))
        {
            throw new InvalidOperationException($"AddMapObject 失败（{c.Name}）");
        }
        SystemShare.ActorMgr.Add(po);

        net.Frames.Clear();
        // `SM_WEIGHTCHANGED(622)` 的三字段前值（`WeightChanged` 只改 Weight；Wear/Hand 由夹具决定）。
        var wabilBefore = new Ability
        {
            Weight = po.WAbil.Weight,
            WearWeight = po.WAbil.WearWeight,
            HandWeight = po.WAbil.HandWeight,
        };
        // **建对象之后**才注入种子（`new PlayObject()` 自身 5 抽不进本账本）。
        SeedRng(c.Seed);

        var mi = typeof(PlayObject).GetMethod("ClientUseItems", BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingMethodException("PlayObject.ClientUseItems");
        mi.Invoke(po, new object[] { c.MakeIndex, "move_probe_item" });

        var (messages, rawMessages, drainPasses, drainHitCap) = Drain(GetMsgQueue(po));
        var immediate = net.Frames.ToList();

        // pump：按**排空序**驱动 move 面全部五条 RM（oracle 消息泵的真实出队序）：
        // 8062 FIRE → 800、10115 WEIGHTCHANGED → 622、8097 CLEAROBJECTS → 633、
        // 8098 CHANGEMAP → 634(+MapDescription/ServerConfig 连带帧)、8100 SPACEMOVE_SHOW → 801。
        var pumpSet = new HashSet<int> { 8062, 10115, 8097, 8098, 8100 };
        var operate = typeof(PlayObject).GetMethod("Operate", BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingMethodException("PlayObject.Operate");
        var pumpIdents = new List<int>();
        foreach (var m in rawMessages)
        {
            var mt = m.GetType();
            int wid = (int)mt.GetField("wIdent").GetValue(m);
            if (!pumpSet.Contains(wid))
            {
                continue;
            }
            pumpIdents.Add(wid);
            var pm = new ProcessMessage
            {
                wIdent = wid,
                wParam = (int)mt.GetField("wParam").GetValue(m),
                nParam1 = (int)mt.GetField("nParam1").GetValue(m),
                nParam2 = (int)mt.GetField("nParam2").GetValue(m),
                nParam3 = (int)mt.GetField("nParam3").GetValue(m),
                ActorId = (int)mt.GetField("ActorId").GetValue(m),
                LateDelivery = (bool)mt.GetField("LateDelivery").GetValue(m),
                Msg = (string)mt.GetField("Buff").GetValue(m),
            };
            operate.Invoke(po, new object[] { pm });
        }
        var pumped = net.Frames.Skip(immediate.Count)
            .Select(f => (object)new { ident = f.ident, recog = f.recog, param = f.param, tag = f.tag, series = f.series, tail = f.tail })
            .ToList();

        return new
        {
            name = c.Name,
            ctx = new
            {
                make_index = c.MakeIndex,
                index = (int)c.Index,
                // oracle `PlayObject.ActorId`（线上帧的 Recog；Rust 侧 actor id 由引擎分配 ⇒ 比较时按此映射）。
                actor_id = po.ActorId,
                // `WAbil.Weight/WearWeight/HandWeight` 前值（`SM_WEIGHTCHANGED(622)` 的 Recog/Param/Tag）。
                wabil_before = new
                {
                    weight = (int)wabilBefore.Weight,
                    wear_weight = (int)wabilBefore.WearWeight,
                    hand_weight = (int)wabilBefore.HandWeight,
                },
                current = new { map = c.CurMap, x = (int)c.CurX, y = (int)c.CurY },
                home = new { map = c.HomeMap, x = (int)c.HomeX, y = (int)c.HomeY },
                red_home = new { map = c.RedMap, x = (int)c.RedX, y = (int)c.RedY },
                pvp_level = (byte)(c.PkPoint / 100),
                precise = c.Precise,
                note = c.Note,
                norandom = c.NoRandom,
                seed = c.Seed,
                // 离线夹具的地图几何（两侧必须同尺寸/同可走格；本批全可走）。
                map_sizes = new { move_map = new[] { 60, 60 }, home_map = new[] { 60, 60 }, red_map = new[] { 60, 60 } },
                // 夹具地图 Flag：DARK（boDarkness=true ⇒ DayBright=1，两侧确定）；norandom 见 `norandom` 字段。
                map_day_light = false,
                map_darkness = true,
                // 逐图非空 MapDesc（54 尾的来源；Rust rig 从本键读同值建 registry）。
                map_desc = new { move_map = "移动之图", home_map = "归家之图", red_map = "红名之图" },
                // 801 `Series = MakeWord(Dir,Light)` 的成立前提（实体 `Dir=5` 旋钮、ctor 常量 4 已弃；
                // `Light=0` 为 ctor 值；夹具地图 `boDarkness=true` ⇒ 634 `Series`/`DayBright()=1`）。
                presentation = new { dir = 5, light = 0, day_light = false },
                // 634 尾 = `EDCode(Envir.MapFileName)`；夹具 `MapFileName` = 取图文件主干（= 逻辑名）。
                map_file_name = c.CurMap,
            },
            expect = new
            {
                immediate_frames = immediate
                    .Select(f => new { ident = f.ident, recog = f.recog, param = f.param, tag = f.tag, series = f.series, tail = f.tail })
                    .ToList(),
                notices = messages,
                pumped,
                pump_idents = pumpIdents,
                final = new { map = po.MapName, envir = po.Envir?.MapName, x = (int)po.CurrX, y = (int)po.CurrY },
                item_list = po.ItemList.Select(ItemDto).ToArray(),
                weight = (int)po.WAbil.Weight,
                rng = Draws(),
                // 记录器自证的抽数（与 `rng.len()` 恒等；禁随机支必须为 0——「没抽」不是「没看到」）。
                rng_draw_count = ((RecordingRandom)_rngField.GetValue(null)).Draws.Count,
                drain = new { passes = drainPasses, max_passes = MaxDrainPasses, hit_cap = drainHitCap },
            },
        };
    }

    private static StdItem Tpl(string name, byte stdMode, byte shape, byte weight)
        => new StdItem { Name = name, StdMode = stdMode, Shape = shape, Weight = weight };

    internal static string SourceAnchorSha256(IEnumerable<string> files)
    {
        using var sha = SHA256.Create();
        using var ms = new MemoryStream();
        foreach (var rel in files)
        {
            if (!File.Exists(rel))
            {
                throw new FileNotFoundException($"oracle 源锚点文件缺失（请在仓库根运行）：{rel}");
            }
            ms.Write(Encoding.UTF8.GetBytes(rel + "\n"));
            ms.Write(File.ReadAllBytes(rel));
            ms.WriteByte((byte)'\n');
        }
        return Convert.ToHexString(sha.ComputeHash(ms.ToArray())).ToLowerInvariant();
    }

    private static string Sha256Hex(string path)
    {
        if (!File.Exists(path)) return "<missing>";
        using var sha = SHA256.Create();
        return Convert.ToHexString(sha.ComputeHash(File.ReadAllBytes(path))).ToLowerInvariant();
    }

    public static int Main(string[] args)
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        LogService.Logger = new LoggerConfiguration().CreateLogger();
        M2Share.ServerIndex = 0;

        string outPath = args.Length > 0
            ? args[0]
            : Path.Combine(AppContext.BaseDirectory, "..", "..", "..", "..", "..", "fixtures", "move_eat_vectors.json");

        var net = new CaptureNetChannel();
        M2Share.NetChannel = net;
        SystemShare.CastleMgr = StubProxy.Create<ICastleSystem>();

        // **真身 GameItemSystem**：GetStdItem/RecalcBagWeight 全走 oracle 同一份代码（按 id 稠密铺表）。
        var templates = new Dictionary<ushort, StdItem>
        {
            [3001] = Tpl("town_scroll", 3, 1, 5),       // IT-09 Shape1 回城卷
            [3002] = Tpl("random_scroll", 3, 2, 5),     // IT-10 Shape2 随机传送
            [3003] = Tpl("dungeon_scroll", 3, 3, 5),    // IT-11 Shape3 地牢逃脱
        };
        var table = new GameItemSystem();
        ushort maxId = templates.Keys.Max();
        for (ushort id = 1; id <= maxId; id++)
        {
            table.AddItem(templates.TryGetValue(id, out var t) ? t : new StdItem { Name = string.Empty });
        }
        SystemShare.ItemSystem = table;

        string tmp = Path.Combine(Path.GetTempPath(), "m3-move-eat-probe");
        Directory.CreateDirectory(tmp);
        string mapFileMove = "move_map.map";
        string mapFileHome = "home_map.map";
        string mapFileRed = "red_map.map";
        WriteMap(Path.Combine(tmp, mapFileMove), 60, 60, new HashSet<(short, short)>());
        WriteMap(Path.Combine(tmp, mapFileHome), 60, 60, new HashSet<(short, short)>());
        WriteMap(Path.Combine(tmp, mapFileRed), 60, 60, new HashSet<(short, short)>());

        var cases = new List<object>
        {
            // IT-09：Shape1 回城卷 —— HomeMap=home_map / HomeX/HomeY=0 ⇒ `BaseObjectMove(HomeMap,0,0)`
            // ⇒ `MapRandomMove`（两次抽：`Random(W-21)`/`Random(H-21)`，60x60 ⇒ Next(39) ×2）。
            RunCase(table, net, tmp, mapFileMove, mapFileHome, mapFileRed, new Ctx
            {
                Name = "eat_ok_shape1_town_scroll_home_random",
                Index = 3001,
                HomeMap = "home_map",
                HomeX = 0,
                HomeY = 0,
                Seed = 20261120,
            }),
            // IT-09 第二支：HomeMap 空 ⇒ `BaseObjectMove` 的 `string.IsNullOrEmpty(sMap) ⇒ sMap = MapName`
            // ⇒ 在当前图（move_map）随机落点（同一 `Random(39)` ×2，但目标图变了 ⇒ 落点语义可辨）。
            RunCase(table, net, tmp, mapFileMove, mapFileHome, mapFileRed, new Ctx
            {
                Name = "eat_ok_shape1_town_scroll_empty_home_falls_back_to_current_map",
                Index = 3001,
                HomeMap = "",
                HomeX = 0,
                HomeY = 0,
                Seed = 20261121,
            }),
            // IT-10 未置位支：Shape2 `if (!Envir.Flag.boNORANDOMMOVE)` ⇒ FIRE + `BaseObjectMove(MapName,0,0)`
            // ⇒ 当前图随机点（两次抽 `Next(39)`）；Home/RedHome 都不参与。
            RunCase(table, net, tmp, mapFileMove, mapFileHome, mapFileRed, new Ctx
            {
                Name = "eat_ok_shape2_random_scroll_current_map",
                Index = 3002,
                HomeMap = "home_map",
                HomeX = 0,
                HomeY = 0,
                Seed = 20261122,
            }),
            // IT-10 置位支：`boNORANDOMMOVE` ⇒ **不吃**（EatUseItems 返回 false）⇒ 只 `SM_EAT_FAIL(636)`；
            // 无 FIRE/无移动/无消耗/RNG 0 抽。
            RunCase(table, net, tmp, mapFileMove, mapFileHome, mapFileRed, new Ctx
            {
                Name = "eat_fail_shape2_norandommap_no_eat",
                Index = 3002,
                HomeMap = "home_map",
                HomeX = 0,
                HomeY = 0,
                NoRandom = true,
                Seed = 20261123,
            }),
            // IT-11 PvpLevel<2 支：`BaseObjectMove(HomeMap,HomeX,HomeY)`，两轴非 0 ⇒ **精确点、0 抽**。
            RunCase(table, net, tmp, mapFileMove, mapFileHome, mapFileRed, new Ctx
            {
                Name = "eat_ok_shape3_dungeon_home_exact",
                Index = 3003,
                HomeMap = "home_map",
                HomeX = 5,
                HomeY = 6,
                Seed = 20261124,
                Precise = true,
            }),
            // IT-11 PvpLevel>=2 支：`BaseObjectMove(Config.RedHomeMap, RedHomeX, RedHomeY)`（20/30 非 0）⇒ 精确点。
            RunCase(table, net, tmp, mapFileMove, mapFileHome, mapFileRed, new Ctx
            {
                Name = "eat_ok_shape3_dungeon_red_home_exact",
                Index = 3003,
                HomeMap = "home_map",
                HomeX = 5,
                HomeY = 6,
                PkPoint = 200, // PvpLevel() = 200/100 = 2
                Seed = 20261125,
                Precise = true,
            }),
            // IT-11 同一 `PvpLevel>=2` 支的**另一组 RedHome 值**：证明实现从快照读（不是写死 20/30）。
            RunCase(table, net, tmp, mapFileMove, mapFileHome, mapFileRed, new Ctx
            {
                Name = "eat_ok_shape3_dungeon_red_home_alt_values",
                Index = 3003,
                HomeMap = "home_map",
                HomeX = 5,
                HomeY = 6,
                PkPoint = 200,
                RedX = 25,
                RedY = 35,
                Seed = 20261127,
                Precise = true,
                Note = "home=(home_map,5,6) 是登录时 PvpLevel<2 捕获的字段快照；red=(red_map,25,35) 是"
                    + "登录时 SystemShare.Config.RedHome* 的快照。吃时 PvpLevel=2 ⇒ 必须用 red；若实现误用"
                    + "登录期的 home（或写死 20/30），终态会是 home_map(5,6) / red_map(20,30)。",
            }),
            // IT-11 第四支（`sX != 0 && sY != 0` 的 **&& 语义**）：只缺一维（HomeX=0、HomeY=6）
            // ⇒ `MapRandomMove(HomeMap)`；若实现写成 `||` 会走精确点 (0,6)（0 抽）。
            RunCase(table, net, tmp, mapFileMove, mapFileHome, mapFileRed, new Ctx
            {
                Name = "eat_ok_shape3_home_x_zero_y_nonzero_random",
                Index = 3003,
                HomeMap = "home_map",
                HomeX = 0,
                HomeY = 6,
                Seed = 20261128,
                Precise = false,
                Note = "只缺一维：oracle `if (sX != 0 && sY != 0)` ⇒ 随机点；`||` 实现会落到精确点 (0,6) 且 0 抽。",
            }),
            // IT-11 第三支（`BaseObjectMove` 的坐标 0 门）：HomeX/HomeY=0 ⇒ `MapRandomMove(HomeMap)`，两抽。
            RunCase(table, net, tmp, mapFileMove, mapFileHome, mapFileRed, new Ctx
            {
                Name = "eat_ok_shape3_dungeon_home_coords_zero_random",
                Index = 3003,
                HomeMap = "home_map",
                HomeX = 0,
                HomeY = 0,
                Seed = 20261126,
            }),
        };

        // 自检（夹具前提/实现不符 ⇒ rc=2，不落盘）：
        foreach (var c in cases.Select(x => (dynamic)x))
        {
            var expect = c.expect;
            var im = ((IEnumerable<object>)expect.immediate_frames).Cast<dynamic>().ToList();
            var pumped = ((IEnumerable<object>)expect.pumped).Cast<dynamic>().ToList();
            var pumpIds = pumped.Select(p => (int)p.ident).ToList();
            var rngCount = ((IEnumerable<object>)expect.rng).Cast<object>().Count();
            var items = ((IEnumerable<object>)expect.item_list).Cast<object>().Count();
            // 分两族自检：**禁随机支 = 只 636、0 抽、0 帧、物品保留**；其余 = 吃 + 全套 move 帧 + 2 抽。
            var norandom = c.ctx.norandom is bool b && b;
            if (norandom)
            {
                if (im.Count != 1 || im[^1].ident != 636 || pumped.Count != 0 || rngCount != 0 || items != 1)
                {
                    Console.Error.WriteLine(
                        $"PROBE_SELFTEST_FAIL case={c.name} 禁随机支期望 immediate=[636]/pumped=[]/rng=0/items=1，"
                        + $"实得 im=[{string.Join(",", im.Select(f => (int)f.ident))}] pumped=[{string.Join(",", pumpIds)}] rng={rngCount} items={items}");
                    return 2;
                }
                continue;
            }
            if (im.Count == 0 || im[^1].ident != 635)
            {
                Console.Error.WriteLine($"PROBE_SELFTEST_FAIL case={c.name} 最后一条即时帧不是 SM_EAT_OK(635)");
                return 2;
            }
            if (pumped.Count < 2 || pumpIds[0] != 800 || pumpIds[1] != 622
                || !pumpIds.Contains(633) || !pumpIds.Contains(634) || !pumpIds.Contains(801))
            {
                Console.Error.WriteLine(
                    $"PROBE_SELFTEST_FAIL case={c.name} pumped 期望 800/622 起头且含 633/634/801，实得 [{string.Join(",", pumpIds)}]");
                return 2;
            }
            var precise = c.ctx.precise is bool pr && pr;
            var wantDraws = precise ? 0 : 2;
            if (rngCount != wantDraws)
            {
                Console.Error.WriteLine(
                    $"PROBE_SELFTEST_FAIL case={c.name} 期望 {wantDraws} 抽（precise={precise}），实得 {rngCount}");
                return 2;
            }
            if (items != 0)
            {
                Console.Error.WriteLine($"PROBE_SELFTEST_FAIL case={c.name} 吃完后背包应空");
                return 2;
            }
        }

        var oracleSourceFiles = new[]
        {
            "src/M2Server/Player/PlayObject.Base.cs",   // EatItems :2047 / EatUseItems :2228 / BaseObjectMove :2312
            "src/M2Server/Player/PlayObject.Operate.cs", // ClientUseItems :601
            "src/M2Server/Actor/BaseObject.cs",          // SpaceMove :1104 / MapRandomMove :1214 / SpaceMoveGetRandXY :1040
            "src/M2Server/Actor/BaseObject.Message.cs",  // SendRefMsg :383 / RM_SPACEMOVE_FIRE :1955
            "src/M2Server/Player/PlayObject.Message.cs", // Operate :768 / RM_WEIGHTCHANGED :1759
        };
        var doc = new
        {
            generator = "mir2-rs/tests/parity/m3/move-eat-probe",
            oracle_source_sha256 = SourceAnchorSha256(oracleSourceFiles),
            oracle_source_sha256_files = oracleSourceFiles,
            oracle_source = new[]
            {
                "src/M2Server/Player/PlayObject.Base.cs:2163 (EatItems StdMode3 default -> EatUseItems)",
                "src/M2Server/Player/PlayObject.Base.cs:2228 (EatUseItems case1/2/3)",
                "src/M2Server/Player/PlayObject.Base.cs:2312 (BaseObjectMove: sX/sY 任一为 0 -> MapRandomMove)",
                "src/M2Server/Actor/BaseObject.cs:1104 (SpaceMove: RM_CLEAROBJECTS/CHANGEMAP/SPACEMOVE_SHOW + SpaceMoveGetRandXY)",
                "src/M2Server/Actor/BaseObject.cs:1214 (MapRandomMove: Random(Width-nEgdey-1)/Random(Height-nEgdey-1))",
                "src/M2Server/Player/PlayObject.Operate.cs:601 (ClientUseItems: WeightChanged + SM_EAT_OK/FAIL)",
                "src/M2Server/Actor/BaseObject.Message.cs:383 (SendRefMsg ±SendRefMsgRange，含自己)",
                "src/M2Server/Player/PlayObject.Message.cs:1955 (RM_SPACEMOVE_FIRE -> SM_SPACEMOVE_HIDE=800)",
                "src/M2Server/Player/PlayObject.Message.cs:1759 (RM_WEIGHTCHANGED -> SM_WEIGHTCHANGED=622)",
            },
            notes = new[]
            {
                "home/red_home 是 PlayObject.HomeMap/HomeX/HomeY 与 Config.RedHome* 的**字段值**；真实链路里 home 来自 login + 3s CheckDupObjTick 的 GetStartPoint()（按当前地图查 StartPointList，PvpLevel>=2 走 RedHome），不是 [Setup] 常量。",
                "种子在 new PlayObject()（自身 5 抽）之后注入 ⇒ 账本只含本次 ClientUseItems 的抽数；两侧同 seed 的 legacy System.Random 序列逐次对齐。",
                "pumped 只含 SM_SPACEMOVE_HIDE(800)/SM_WEIGHTCHANGED(622)：633/634/801 换图簇由 Rust 宿主 world tick 的 drain_map_change 出，不在本对拍面（卡面登记）。",
                "MapRandomMove 的两抽参数按**目标图**宽高（nEgdey：H<150 且 >=30 ⇒ 20 ⇒ Random(60-20-1)=Next(39)）。",
                "落点可走 ⇒ SpaceMoveGetRandXY 0 抽；本批夹具全可走（60x60 全零格）。",
            },
            scenario_note = "夹具契约：三图 60x60 全可走；MapName == MapFileName 主干（无 [名|文件] 覆盖）；"
                + "地图 Flag.boDarkness=true（DayBright=1，DARK 臂）；逐图非空 MapDesc（54 尾，见 ctx.map_desc）；"
                + "实体 Dir=5 / Light=0 / Feature=0 / Status=0。"
                + "改动任一前提都要同时改 golden 的 ctx（presentation/map_desc/map_file_name/map_sizes）。",
            map_geometry = new { width = 60, height = 60, walkable = "all" },
            templates = templates.ToDictionary(
                kv => kv.Key.ToString(CultureInfo.InvariantCulture),
                kv => new { name = kv.Value.Name, std_mode = (int)kv.Value.StdMode, shape = (int)kv.Value.Shape, weight = (int)kv.Value.Weight }),
            cases,
        };

        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outPath))!);
        // 落盘统一 LF（仓库约定；CRLF 会被 `git diff --check` 判 trailing whitespace）。
        string json = JsonSerializer.Serialize(doc, new JsonSerializerOptions { WriteIndented = true }).Replace("\r\n", "\n");
        File.WriteAllText(outPath, json + "\n", new UTF8Encoding(false));
        Console.WriteLine($"WROTE {Path.GetFullPath(outPath)}");
        Console.WriteLine($"TEMPLATES {templates.Count} CASES {cases.Count}");
        Console.WriteLine("MOVEEATPROBE_OK");
        return 0;
    }
}
