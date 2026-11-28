// readbook-probe —— IT-17a（ReadBook 读书 / 技能书）oracle 真身向量。
//
// oracle 路径（全部真身）：
//   `PlayObject.Message.cs:826 CM_EAT → ClientUseItems(nParam1, Msg)`（`PlayObject.Operate.cs:601`）
//   → `case 4:`（`:632-654`）→ `ReadBook(stdItem)`（`PlayObject.Base.cs:2171-2200`）：
//     ① `FindMagic(stdItem.Name)`（按**物品名**查魔法表）② 未学（`IsTrainingSkill`，`PlayObject.Attack.cs:340-351`）
//     ③ `Job==99 || Job==(byte)Job` ④ `Abil.Level >= TrainLevel[0]`
//     → `MagicList.Add(UserMagic{Level=0,Key=0,TranPoint=0})` → `RecalcAbilitys()` →
//       `SendAddMagic`（`MakeMessage(SM_ADDMAGIC,0,0,0,1)` + `EDCode(ClientMagic{Key,Level,CurTrain,Def})`）
//   尾段：`MagicArr[SKILL_ERGUM(12)]!=null && !UseThrusting ⇒ ThrustingOnOff(true)+SendSocket("+LNG")`；
//         `MagicArr[SKILL_BANWOL(25)]!=null && !UseHalfMoon ⇒ HalfMoonOnOff(true)+SendSocket("+WID")`；
//         `Dispose/RemoveAt` → `WeightChanged()`（RM_WEIGHTCHANGED）→ `SM_EAT_OK(635)`。
//   `RecalcAbilitys` 尾（`PlayObject.cs:~2874`）：`CharStatus=GetCharStatus(); StatusChanged();
//   SendUpdateMsg(RM_CHARSTATUSCHANGED, HitSpeed, CharStatus, …)`；`MagicArr` 由 MagicList 填（`:3681-3684`）。
//
// **原始串帧**：`SendSocket(string)` 是 `ServerMessage` 头（`PacketSize=20`，`Ident=GM_DATA`、
// `PackLength` 取负）+ 明文；本探针记录每条帧 `payload` = `senData[20..]` 的完整 hex（SM 帧的
// command header 也在这 12B 里），因此 `+LNG`/`+WID` 与普通帧同面逐字节可比。
//
// 用法（仓库根）：dotnet run --project mir2-rs/tests/parity/m3/readbook-probe -c Release -- <输出>

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
using SystemModule.SubSystem;

namespace ReadBookProbe;

/// <summary>截获的一条帧：`payload` = `ServerMessage` 头（20B）之后的完整 hex。</summary>
internal sealed class CapFrame
{
    public string payload;
    public int ident;   // 头 32B 可解析时填；原始串帧（payload 不是 12B 命令头）填 -1
    public int recog;
    public int param;
    public int tag;
    public int series;
    public int len;     // 整个 gate buffer 长度（含 20B 头）
}

internal sealed class CaptureNetChannel : INetChannel
{
    public readonly List<CapFrame> Frames = new();

    public void AddGateBuffer(int gateIdx, byte[] senData)
    {
        if (senData == null)
        {
            Frames.Add(new CapFrame { payload = "", ident = -1, len = 0 });
            return;
        }
        var payload = senData.Length > ServerMessage.PacketSize
            ? senData.Skip(ServerMessage.PacketSize).ToArray()
            : Array.Empty<byte>();
        var f = new CapFrame
        {
            payload = Convert.ToHexString(payload).ToLowerInvariant(),
            len = senData.Length,
            ident = -1,
        };
        // 普通 SM 帧：payload 以 12B CommandMessage 开头（recog@0, ident@4）。原始串帧（"+LNG"）不满足。
        if (payload.Length >= 12)
        {
            // 12B `CommandMessage`：recog@0(i32)、ident@4、param@6、tag@8、series@10。
            f.recog = BitConverter.ToInt32(payload, 0);
            f.ident = BitConverter.ToUInt16(payload, 4);
            f.param = BitConverter.ToUInt16(payload, 6);
            f.tag = BitConverter.ToUInt16(payload, 8);
            f.series = BitConverter.ToUInt16(payload, 10);
        }
        Frames.Add(f);
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

/// <summary>`System.Random` 记录包装（本卡无抽，但要**记录器自证 0 抽**，不是「没看到数」）。</summary>
internal sealed class RecordingRandom : Random
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

    public override int Next()
    {
        int v = base.Next();
        Draws.Add($"{Draws.Count + 1}|Next()|0|{v}|{CallSite()}");
        return v;
    }

    public override int Next(int maxValue)
    {
        int v = base.Next(maxValue);
        Draws.Add($"{Draws.Count + 1}|Next(max)|{maxValue}|{v}|{CallSite()}");
        return v;
    }
}

/// <summary>`SystemShare.WorldEngine` 的最小替身：只实现 `FindMagic(string|int)`；
/// 其余成员返回默认（`SystemShare.CastleMgr` 同款替身用于 `RecalcAbilitys` 的着色路径）。</summary>
internal class StubProxy : DispatchProxy
{
    public Dictionary<string, MagicInfo> ByName = new(StringComparer.OrdinalIgnoreCase);
    public Dictionary<int, MagicInfo> ById = new();
    public Dictionary<string, object> Objects = new();

    public static T Create<T>(Action<T> init = null) where T : class
    {
        var p = DispatchProxy.Create<T, StubProxy>();
        init?.Invoke(p);
        return p;
    }

    protected override object Invoke(MethodInfo targetMethod, object[] args)
    {
        if (targetMethod.Name == "FindMagic" && args != null && args.Length == 1)
        {
            return args[0] switch
            {
                string name => ByName.TryGetValue(name, out var m) ? m : null,
                int id => ById.TryGetValue(id, out var m) ? m : null,
                _ => null,
            };
        }
        var rt = targetMethod.ReturnType;
        if (rt == typeof(void))
        {
            return null;
        }
        return rt.IsValueType ? Activator.CreateInstance(rt) : null;
    }

    public void SetObjects(Dictionary<string, object> objects) => Objects = objects;
}

internal static class Program
{
    private sealed class Ctx
    {
        public string Name;
        public ushort Index;
        /// <summary>玩家 `Abil.Level`（等级门 ④ 用基础 `Abil.Level`）。</summary>
        public int Level = 20;
        public byte Job; // PlayerJob.Warrior = 0
        /// <summary>已学技能 id 列表（`MagicList` 预置；`IsTrainingSkill` 只看 MagIdx）。</summary>
        public int[] Known = Array.Empty<int>();
        public string Note;
        public int Seed = 20261120;
        /// <summary>**IT-17 输入注入批**：会话预挂 buff（`ActorBuffSystem.AddBuff`，list 序 = 位序）。</summary>
        public (int type, int dura, int stateval)[] PreBuffs = Array.Empty<(int, int, int)>();
        /// <summary>**IT-17 输入注入批**：会话预挂武器（`UseItems[Weapon]`；MAC 高字节 = RealAttackSpeed 源）。</summary>
        public (ushort index, ushort dura)? Weapon;
        /// <summary>**IT-17 weight 点亮（① 档，2026-11-28）**：额外背包件（读掉主书后仍在包）⇒
        /// `WeightChanged` 重算 `Weight>0` ⇒ 622 的 Recog/校验和覆盖非零和（IT-16 同源字段复用）。</summary>
        public ushort[] ExtraBag = Array.Empty<ushort>();
    }

    private static UserItem ToUser(ushort index)
        => new UserItem { MakeIndex = 1, Index = index, Dura = 10, DuraMax = 20 };

    private static object ItemDto(UserItem u)
        => u == null ? null : new { make_index = u.MakeIndex, index = (int)u.Index, dura = (int)u.Dura, dura_max = (int)u.DuraMax };

    private static object MagicDto(MagicInfo m) => new
    {
        magic_id = (int)m.MagicId,
        name = m.MagicName,
        effect_type = (int)m.EffectType,
        effect = (int)m.Effect,
        spell = (int)m.Spell,
        power = (int)m.Power,
        max_power = (int)m.MaxPower,
        train_level = m.TrainLevel.Select(x => (int)x).ToArray(),
        max_train = m.MaxTrain,
        train_lv = (int)m.TrainLv,
        job = (int)m.Job,
        delay_time = m.DelayTime,
        def_spell = (int)m.DefSpell,
        def_power = (int)m.DefPower,
        def_max_power = (int)m.DefMaxPower,
        desc = m.Desc ?? "",
    };

    private static MagicInfo MkMagic(ushort id, string name, byte job, byte lvl0)
        => new MagicInfo
        {
            MagicId = id,
            MagicName = name,
            EffectType = 1,
            Effect = 8,
            Spell = 10,
            Power = 10,
            MaxPower = 10,
            TrainLevel = new byte[] { lvl0, (byte)(lvl0 + 1), (byte)(lvl0 + 2), 0 },
            MaxTrain = new int[] { 100, 200, 300, 0 },
            TrainLv = 3,
            Job = job,
            DelayTime = 0,
            DefSpell = 0,
            DefPower = 0,
            DefMaxPower = 0,
            Desc = "",
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

    private const int MaxDrainPasses = 8;

    /// <summary>把排空的原始 RM 消息逐条泵进 `Operate`（出帧走 NetChannel 捕获）；返回 pump 序的 `wIdent`。</summary>
    private static List<int> PumpRaw(object po, MethodInfo operate, List<object> raw)
    {
        var idents = new List<int>();
        foreach (var m in raw)
        {
            var mt = m.GetType();
            int wid = (int)mt.GetField("wIdent").GetValue(m);
            idents.Add(wid);
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
        return idents;
    }

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

    private static object RunCase(GameItemSystem table, CaptureNetChannel net, string name, Ctx c)
    {
        var magic = ((StubProxy)(object)SystemShare.WorldEngine).ByName;
        var po = new PlayObject
        {
            Race = ActorRace.Play,
            ActorId = 940000 + (int)c.Index,
            ChrName = "book_probe",
            IsRobot = false,
            OffLineFlag = false,
            Envir = new Envirnoment(),
            Job = (PlayerJob)c.Job,
            ItemList = new List<UserItem>(),
            MagicList = new List<UserMagic>(),
            Abil = new Ability { Level = (byte)c.Level },
        };
        po.WAbil = new Ability();
        foreach (var id in c.Known)
        {
            var def = magic.Values.FirstOrDefault(m => m.MagicId == id) ?? new MagicInfo { MagicId = (ushort)id };
            po.MagicList.Add(new UserMagic { Magic = def, MagIdx = (ushort)id, Key = (char)0, Level = 0, TranPoint = 0 });
        }
        po.ItemList.Add(ToUser(c.Index));
        // **IT-17 weight 点亮（① 档，2026-11-28）**：额外背包件（读掉主书后仍在包）⇒
        // `WeightChanged` 重算 `Weight>0` ⇒ 622 的 Recog/校验和覆盖非零和。
        foreach (var extraIdx in c.ExtraBag)
        {
            po.ItemList.Add(new UserItem { MakeIndex = 2, Index = extraIdx, Dura = 10, DuraMax = 20 });
        }
        // RecalcAbilitys 会按等级重算基础字段（真身）；Abil 先置等级即可。
        po.Abil = new Ability { Level = (byte)c.Level };
        po.WAbil = new Ability { Level = (byte)c.Level };

        // **IT-17 输入注入批（会话输入 = 登录态）**：
        //   ① 武器：`UseItems[Weapon]`（StdMode 5，MAC 高字节经 `GetUpgradeStdItem` Desc=0 同值回环后
        //      喂 `RealAttackSpeed` ⇒ `AddAbil.HitSpeed`，`PlayObject.cs:4419`）；Recalc 后
        //      `HitSpeed = AddAbil.HitSpeed`（`:2899`）。
        //   ② buff：`ActorBuffSystem.AddBuff` **只改状态不发帧不入队**（`ActorBuffSystem.cs:100-159`）；
        //      list 序 = 位序（`GetBuffStatus` `:282-296`，`OR(0x80000000>>i)`）。
        if (c.Weapon is { } wp)
        {
            po.UseItems = new UserItem[13];
            po.UseItems[ItemLocation.Weapon] = new UserItem
            {
                MakeIndex = 2,
                Index = wp.index,
                Dura = wp.dura,
                DuraMax = 20,
            };
        }
        foreach (var pb in c.PreBuffs)
        {
            M2Share.ActorBuffSystem.AddBuff(po, (BuffType)pb.type, pb.dura, pb.stateval);
        }
        // **会话预 Recalc（登录态）**：oracle 的 `CharStatus`/`HitSpeed` 字段既有值由此产生（失败例
        // 不再跑 Recalc ⇒ 两字段保持预 Recalc 值）。产生的 RM 逐条排空并泵出帧后**丢弃**（净捕获
        // 只从本 case 的 `ClientUseItems` 起）。权重三字段 base 每轮清零（`BaseObject.Abil.cs:27-30`）
        // ⇒ 两次 Recalc 不累积，case 内 Recalc 与单轮语义同值。
        po.RecalcAbilitys();
        var operate = typeof(PlayObject).GetMethod("Operate", BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingMethodException("PlayObject.Operate");
        var (preNotices, preRaw, _, _) = Drain(GetMsgQueue(po));
        _ = preNotices;
        _ = PumpRaw(po, operate, preRaw);
        net.Frames.Clear(); // 预帧丢弃点：本 case 的帧面从这里起。

        // **IT-17 weight 点亮**：case 起点背包快照（Rust rig 按本键建同形背包）。
        var bagBefore = po.ItemList.Select(u => new { make_index = (int)u.MakeIndex, index = (int)u.Index }).ToArray();
        var wabilBefore = new Ability
        {
            Weight = po.WAbil.Weight,
            WearWeight = po.WAbil.WearWeight,
            HandWeight = po.WAbil.HandWeight,
        };
        // **建对象之后**才注入种子（`new PlayObject()` 自身 5 抽不进账本）；预 Recalc 在注入**之前**
        // 跑 ⇒ 记录器的 0 抽自证严格是 case 范围（`ClientUseItems` 起算）。
        RandomNumber.GetInstance();
        typeof(RandomNumber)
            .GetField("random", BindingFlags.NonPublic | BindingFlags.Static)
            .SetValue(null, new RecordingRandom(c.Seed));
        var mi = typeof(PlayObject).GetMethod("ClientUseItems", BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingMethodException("PlayObject.ClientUseItems");
        var magicBefore = po.MagicList.Select(u => (int)u.MagIdx).ToArray();
        var magArrBefore = Enumerable.Range(0, po.MagicArr.Length).Where(i => po.MagicArr[i] != null).ToArray();
        mi.Invoke(po, new object[] { 1, "book" });

        var (messages, rawMessages, drainPasses, drainHitCap) = Drain(GetMsgQueue(po));
        var immediate = net.Frames.ToList();

        var pumpIdents = PumpRaw(po, operate, rawMessages);
        var pumped = net.Frames.Skip(immediate.Count).ToList();

        object FrameDto(CapFrame f) => new
        {
            ident = f.ident,
            recog = f.recog,
            param = f.param,
            tag = f.tag,
            series = f.series,
            payload = f.payload,
            len = f.len,
        };

        return new
        {
            name,
            ctx = new
            {
                index = (int)c.Index,
                actor_id = po.ActorId,
                level = c.Level,
                job = (int)c.Job,
                known = c.Known,
                note = c.Note,
                seed = c.Seed,
                // **IT-17 输入注入批**：会话输入键（Rust 侧按同键装配同状态：world.buffs.add_buff +
                // use_items[SLOT_WEAPON] + recalc_session_ability）。
                prebuffs = c.PreBuffs.Select(pb => new { type = pb.type, dura = pb.dura, state_val = pb.stateval }).ToArray(),
                weapon = c.Weapon is { } w2
                    ? new { index = (int)w2.index, dura = (int)w2.dura, make_index = 2 }
                    : null,
                wabil_before = new
                {
                    weight = (int)wabilBefore.Weight,
                    wear_weight = (int)wabilBefore.WearWeight,
                    hand_weight = (int)wabilBefore.HandWeight,
                },
                // case 起点背包（**条件键**：只在 ExtraBag 例出现 ⇒ 既有例零变化；Rust rig
                // 有键按袋建、无键走既有单件路径）。
                bag = c.ExtraBag.Length > 0 ? bagBefore : null,
                // 这本书名对应的**魔法表条目**（尾 = ClientMagic.Def 的来源，逐字段钉住）。
                magic = magic.TryGetValue(BookName(table, c.Index), out var bookDef) ? MagicDto(bookDef) : null,
                templates = new[] { new { index = (int)c.Index, name = BookName(table, c.Index) } },
            },
            expect = new
            {
                immediate_frames = immediate.Select(FrameDto).ToList(),
                notices = messages,
                pumped = pumped.Select(FrameDto).ToList(),
                pump_idents = pumpIdents,
                magic_list_before = magicBefore,
                magic_arr_before = magArrBefore,
                magic_list = po.MagicList.Select(u => new
                {
                    mag_idx = (int)u.MagIdx,
                    level = (int)u.Level,
                    key = (int)u.Key,
                    train_point = u.TranPoint,
                }).ToArray(),
                magic_arr = Enumerable.Range(0, po.MagicArr.Length)
                    .Where(i => po.MagicArr[i] != null)
                    .Select(i => new { id = i, level = (int)po.MagicArr[i].Level })
                    .ToArray(),
                use_thrusting = po.UseThrusting,
                use_half_moon = po.UseHalfMoon,
                char_status = po.CharStatus,
                hit_speed = (int)po.HitSpeed,
                abil_level = (int)po.Abil.Level,
                item_list = po.ItemList.Select(ItemDto).ToArray(),
                weight = (int)po.WAbil.Weight,
                // IT-17a：ReadBook 路径**无抽**；记录器自证 0 抽（不是「没有字段」）。
                rng_draw_count = ((RecordingRandom)typeof(RandomNumber)
                    .GetField("random", BindingFlags.NonPublic | BindingFlags.Static)
                    .GetValue(null)).Draws.Count,
                rng = ((RecordingRandom)typeof(RandomNumber)
                    .GetField("random", BindingFlags.NonPublic | BindingFlags.Static)
                    .GetValue(null)).Draws
                    .Select(line =>
                    {
                        var parts = line.Split('|');
                        return new { seq = int.Parse(parts[0], CultureInfo.InvariantCulture), kind = parts[1], arg = long.Parse(parts[2], CultureInfo.InvariantCulture), value = long.Parse(parts[3], CultureInfo.InvariantCulture), site = parts[4] };
                    })
                    .ToArray(),
                drain = new { passes = drainPasses, max_passes = MaxDrainPasses, hit_cap = drainHitCap },
            },
        };
    }

    private static readonly Dictionary<ushort, string> BookNames = new()
    {
        [3001] = "刺杀剑术",
        [3002] = "半月弯刀",
        [3003] = "不存在技能",
        [3004] = "火球术",
        [3005] = "烈火剑法",
        [3006] = "高级刺杀",
        [3007] = "边界技能",
    };

    private static string BookName(GameItemSystem table, ushort idx)
    {
        var tpl = table.GetStdItem(idx);
        return tpl?.Name ?? (BookNames.TryGetValue(idx, out var n) ? n : "");
    }

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

    public static int Main(string[] args)
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        LogService.Logger = new LoggerConfiguration().CreateLogger();
        M2Share.ServerIndex = 0;

        string outPath = args.Length > 0
            ? args[0]
            : Path.Combine(AppContext.BaseDirectory, "..", "..", "..", "..", "..", "fixtures", "readbook_vectors.json");

        var net = new CaptureNetChannel();
        M2Share.NetChannel = net;

        // 魔法表替身：按名字/ID 都能查（`FindMagic(string)` 是 ReadBook 的入口）。
        var magics = new Dictionary<string, MagicInfo>(StringComparer.OrdinalIgnoreCase)
        {
            ["刺杀剑术"] = MkMagic(12, "刺杀剑术", 0, 10),
            ["半月弯刀"] = MkMagic(25, "半月弯刀", 0, 15),
            ["火球术"] = MkMagic(1, "火球术", 1, 7),
            ["烈火剑法"] = MkMagic(26, "烈火剑法", 0, 40),
            ["高级刺杀"] = MkMagic(46, "高级刺杀", 0, 30),
            // 等级门边界：`Abil.Level(20) >= TrainLevel[0](20)` ⇒ **恰好可学**（`>=` 的边界牙）。
            ["边界技能"] = MkMagic(49, "边界技能", 0, 20),
        };
        var engine = DispatchProxy.Create<IWorldEngine, StubProxy>();
        var sp = (StubProxy)(object)engine;
        sp.ByName = magics;
        sp.ById = magics.Values.ToDictionary(m => (int)m.MagicId, m => m);
        SystemShare.WorldEngine = engine;
        SystemShare.CastleMgr = DispatchProxy.Create<ICastleSystem, StubProxy>();

        var templates = new Dictionary<ushort, StdItem>();
        foreach (var (idx, name) in BookNames)
        {
            templates[idx] = new StdItem { Name = name, StdMode = 4, Shape = 0, Weight = 5 };
        }
        // **IT-17 输入注入批**：武器模板（挂 `UseItems[Weapon]`）。`MAC` 高字节 12 ⇒
        // `RealAttackSpeed(12)=+2`（`GetUpgradeStdItem` Desc 全 0 时同值回环）；`Weight=7` 进 622 的
        // `HandWeight`；`Dura=10>0` 走 `ApplyItemParameters` 臂（`:2107` 的 Dura==0 臂只加重量不加速度）。
        templates[5001] = new StdItem { Name = "铁剑", StdMode = 5, Shape = 0, Weight = 7, MAC = 0x0C00, DuraMax = 20 };
        var table = new GameItemSystem();
        ushort maxId = templates.Keys.Max();
        for (ushort id = 1; id <= maxId; id++)
        {
            table.AddItem(templates.TryGetValue(id, out var t) ? t : new StdItem { Name = string.Empty });
        }
        SystemShare.ItemSystem = table;

        // **IT-17 输入注入批**：全 8 例同会话输入（预挂铁剑 5001 + Defense buff），先 RecalcAbilitys
        // 一次（登录态）再跑 case ⇒ 状态面（hit_speed/char_status 字段）8 例全对齐；banwol 例两条
        // buff（Defense+CanNotRun ⇒ 位 31|30）钉 `>>i` 位移公式。
        var preBuffs = new (int, int, int)[] { (2, 60000, 1) };
        var preBuffsDouble = new (int, int, int)[] { (2, 60000, 1), (3, 60000, 1) };
        var weapon = ((ushort)5001, (ushort)10);

        var cases = new List<object>
        {
            RunCase(table, net, "eat_ok_book_ergum_learns_and_thrusting", new Ctx
            {
                Name = "eat_ok_book_ergum_learns_and_thrusting",
                Index = 3001,
                Level = 20,
                PreBuffs = preBuffs,
                Weapon = weapon,
                Note = "刺杀剑术(id12, Job0, TrainLevel[0]=10)：学习成功 ⇒ SM_ADDMAGIC + `+LNG` 原始串帧 + 635",
            }),
            RunCase(table, net, "eat_ok_book_ergum_second_book_keeps_weight", new Ctx
            {
                Name = "eat_ok_book_ergum_second_book_keeps_weight",
                Index = 3001,
                Level = 20,
                PreBuffs = preBuffs,
                Weapon = weapon,
                ExtraBag = new ushort[] { 3002 },
                Note = "IT-17 weight 点亮：包内第二本书（半月弯刀 3002）读后保留 ⇒ WeightChanged 重算 Weight=5≠0 ⇒ 622 Recog=5、校验和覆盖 12（书5+手7）；学习面与 ergum 例同支",
            }),
            RunCase(table, net, "eat_ok_book_banwol_learns_and_halfmoon", new Ctx
            {
                Name = "eat_ok_book_banwol_learns_and_halfmoon",
                Index = 3002,
                Level = 20,
                PreBuffs = preBuffsDouble,
                Weapon = weapon,
                Note = "半月弯刀(id25, Job0, TL[0]=15)：成功 ⇒ SM_ADDMAGIC + `+WID` + 635；双 buff ⇒ 657 tag=0xC000（位 31|30）",
            }),
            RunCase(table, net, "eat_fail_book_magic_not_in_table", new Ctx
            {
                Name = "eat_fail_book_magic_not_in_table",
                Index = 3003,
                Level = 20,
                PreBuffs = preBuffs,
                Weapon = weapon,
                Note = "门①：魔法表里没有该物品名 ⇒ 636、物品保留",
            }),
            RunCase(table, net, "eat_fail_book_already_learned", new Ctx
            {
                Name = "eat_fail_book_already_learned",
                Index = 3001,
                Level = 20,
                Known = new[] { 12 },
                PreBuffs = preBuffs,
                Weapon = weapon,
                Note = "门②：MagicList 已有 MagIdx=12（`IsTrainingSkill`）⇒ 636",
            }),
            RunCase(table, net, "eat_fail_book_job_mismatch", new Ctx
            {
                Name = "eat_fail_book_job_mismatch",
                Index = 3004,
                Level = 20,
                Job = 0,
                PreBuffs = preBuffs,
                Weapon = weapon,
                Note = "门③：火球术 Job=1（法师），玩家战士 ⇒ 636",
            }),
            RunCase(table, net, "eat_fail_book_level_too_low", new Ctx
            {
                Name = "eat_fail_book_level_too_low",
                Index = 3005,
                Level = 20,
                PreBuffs = preBuffs,
                Weapon = weapon,
                Note = "门④：烈火剑法 TL[0]=40 > 等级 20 ⇒ 636",
            }),
            RunCase(table, net, "eat_ok_book_level_boundary_equal", new Ctx
            {
                Name = "eat_ok_book_level_boundary_equal",
                Index = 3007,
                Level = 20,
                PreBuffs = preBuffs,
                Weapon = weapon,
                Note = "等级门边界：Abil.Level == TrainLevel[0] == 20 ⇒ 可学（`>=`，不是 `>`）",
            }),
            RunCase(table, net, "eat_fail_book_already_learned_and_level_low", new Ctx
            {
                Name = "eat_fail_book_already_learned_and_level_low",
                Index = 3006,
                Level = 20,
                Known = new[] { 46 },
                PreBuffs = preBuffs,
                Weapon = weapon,
                Note = "门②③④ 同时不满足：已学(46) 且 TL[0]=30>20 ⇒ 636；两条失败路径帧相同 ⇒ 顺序不可观测（只证各自可达）",
            }),
        };

        // 自检：成功例必须恰有一条 SM_ADDMAGIC(210)、一条 635、且无 636；失败例只一条 636 且 MagicList 不变。
        foreach (var c in cases.Select(x => (dynamic)x))
        {
            var im = ((IEnumerable<object>)c.expect.immediate_frames).Cast<dynamic>().ToList();
            var ids = im.Select(f => (int)f.ident).ToList();
            var mlBefore = ((Array)c.expect.magic_list_before).Length;
            var mlAfter = ((Array)c.expect.magic_list).Length;
            var ok = ids.Contains(210) && ids.Contains(635) && !ids.Contains(636) && mlAfter == mlBefore + 1;
            var fail = ids.Count == 1 && ids[0] == 636 && mlAfter == mlBefore;
            var draws = (int)c.expect.rng_draw_count;
            if (draws != 0)
            {
                Console.Error.WriteLine($"PROBE_SELFTEST_FAIL case={c.name} ReadBook 路径应 0 抽，记录器实得 {draws}");
                return 2;
            }
            if (!(ok || fail))
            {
                Console.Error.WriteLine($"PROBE_SELFTEST_FAIL case={c.name} ids=[{string.Join(",", ids)}] ml {mlBefore}->{mlAfter}");
                return 2;
            }
        }

        var oracleSourceFiles = new[]
        {
            "src/M2Server/Player/PlayObject.Operate.cs",
            "src/M2Server/Player/PlayObject.Base.cs",
            "src/M2Server/Player/PlayObject.Attack.cs",
            "src/M2Server/Player/PlayObject.cs",
            "src/M2Server/Player/PlayObject.Message.cs",
            "src/M2Server/Actor/BaseObject.Message.cs",
            // IT-17 输入注入批：buff 状态源 + base Recalc（AddAbil/权重重置、HitSpeed 减半位置）。
            "src/M2Server/ActorBuffSystem.cs",
            "src/M2Server/Actor/BaseObject.Abil.cs",
        };
        var doc = new
        {
            generator = "mir2-rs/tests/parity/m3/readbook-probe",
            oracle_source_sha256 = SourceAnchorSha256(oracleSourceFiles),
            oracle_source_sha256_files = oracleSourceFiles,
            oracle_source = new[]
            {
                "src/M2Server/Player/PlayObject.Operate.cs:601 (ClientUseItems) / :632-654 (case 4 书)",
                "src/M2Server/Player/PlayObject.Base.cs:2171-2200 (ReadBook: 按物品名 FindMagic + 四道门)",
                "src/M2Server/Player/PlayObject.Attack.cs:340-351 (IsTrainingSkill = MagicList 已有 MagIdx)",
                "src/M2Server/Player/PlayObject.Base.cs:2205-2216 (SendAddMagic: SM_ADDMAGIC 0/0/0/1 + EDCode(ClientMagic))",
                "src/M2Server/Player/PlayObject.cs:3681-3684 (RecalcAbilitys 用 MagicList 填 MagicArr)",
                "src/M2Server/Player/PlayObject.cs:~2874 (CharStatus/StatusChanged/SendUpdateMsg RM_CHARSTATUSCHANGED)",
                "src/M2Server/Player/PlayObject.Socket.cs:48-71 (SendSocket(string): ServerMessage 头 + 明文, PackLength 取负)",
                // IT-17 输入注入批锚点：
                "src/M2Server/Player/PlayObject.Message.cs:1845-1848 (RM→SM_CHARSTATUSCHANGED: param=LoWord(CharStatus), tag=HiWord(CharStatus), series=this.HitSpeed 泵时实时字段)",
                "src/M2Server/Player/PlayObject.cs:4419 (武器 StdMode 5/6: AddAbil.HitSpeed += RealAttackSpeed(HiByte(MAC))) / :2899 (HitSpeed = AddAbil.HitSpeed)",
                "src/M2Server/ActorBuffSystem.cs:100-159 (AddBuff 只改状态不发帧) / :282-296 (GetBuffStatus 位公式 OR(0x80000000>>i), StateTick>0 置位)",
                "src/M2Server/Actor/BaseObject.Abil.cs:15-63 (base.RecalcAbilitys: AddAbil 重置 + 权重三字段清零 + HitSpeed 减半 clamp15 在物品循环前)",
            },
            notes = new[]
            {
                "帧面 payload = gate buffer 去掉 20B ServerMessage 头后的完整 hex；普通 SM 帧的 12B command 头也在其中，`+LNG`/`+WID` 是明文原始串帧（ident 解析为 -1）。",
                "MagicInfo 逐字段记在 ctx.magic（ClientMagic.Def 的来源）；Rust 侧必须用同一组值。",
                "RecalcAbilitys 的**技能被动**（HitPoint/SpeedPoint/DC）不在本卡范围（IT-17b）；本批选无被动的技能，CharStatus/HitSpeed 记在 expect 但不作为 17a 的主判据。",
                // IT-17 输入注入批：
                "IT-17 批：全 8 例会话预挂铁剑 5001（MAC_hi=12 ⇒ HitSpeed=+2，Weight=7）+ Defense buff（banwol 例两条 ⇒ CharStatus=0xC0000000）并**先 RecalcAbilitys 一次**（登录态）；预帧排空泵出后丢弃，帧面只含 case。",
                "657 三字段语义（PlayObject.Message.cs:1845-1848）：param=LoWord(CharStatus)、tag=HiWord(CharStatus)、series=this.HitSpeed（泵时实时字段，非 RM wParam——发射点 :2876 的 wParam 是重置后旧值 0，转换点不读它）。两侧同判据：tag==HiWord(OR(0x80000000>>i))、series==hit_speed、param==LoWord(…)=0。",
                "wabil_before 是预 Recalc 后、case 前的快照：weight=RecalcBagWeight（书仍在包，=5）、hand=武器 Weight=7；权重三字段 base 每轮清零（BaseObject.Abil.cs:27-30）⇒ case 内 Recalc 与单轮语义同值，622 的 tag=HandWeight、recog=Weight（WeightChanged 在移书后重算 ⇒ 泵时 weight=0）。",
            },
            magic_table = magics.Values.Select(MagicDto).ToArray(),
            templates = templates.OrderBy(kv => kv.Key).Select(kv => new
            {
                index = (int)kv.Key,
                name = kv.Value.Name,
                std_mode = (int)kv.Value.StdMode,
                shape = (int)kv.Value.Shape,
                weight = (int)kv.Value.Weight,
                // IT-17 批新增键：RealAttackSpeed 的源字节在 MAC 高字节（`PlayObject.cs:4419`）；书类恒 0。
                mac = (int)kv.Value.MAC,
            }).ToArray(),
            cases,
        };

        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outPath))!);
        string json = JsonSerializer.Serialize(doc, new JsonSerializerOptions { WriteIndented = true }).Replace("\r\n", "\n");
        File.WriteAllText(outPath, json + "\n", new UTF8Encoding(false));
        Console.WriteLine($"WROTE {Path.GetFullPath(outPath)}");
        Console.WriteLine($"CASES {cases.Count}");
        Console.WriteLine("READBOOKPROBE_OK");
        return 0;
    }
}
