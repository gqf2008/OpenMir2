// B2 效果对拍：C# 侧脚本执行器（`script-parity-cs run ...`）。
//
// 目标：同一份脚本 + 同一初始状态 ⇒ 两侧产出**同一份效果日志**（给物/扣钱/发消息/传送…）。
// 本文件只读引用 ScriptSystem 参照实现；玩家/NPC/物品系统均为 harness 侧 mock（DispatchProxy
// + 内存表），不改动任何 C# 参照代码。
//
// 用法：
//   script-parity-cs run <脚本文件> <label> [--gold N] [--level N] [--items name:count,...]
//
// 输出（stdout，规范化一行一效果，供 diff）：
//   msg <ident> <wParam> <nParam1> <nParam2> <nParam3> <文本>
//   gold <delta>
//   additem <名> <件数>
//   delitem <名> <件数>
using System.Reflection;
using System.Text;
using OpenMir2;
using OpenMir2.Data;
using OpenMir2.Packets.ClientPackets;
using ScriptSystem;
using SystemModule;
using SystemModule.Actors;
using SystemModule.Data;
using SystemModule.SubSystem;

namespace ScriptParityCs;

/// 内存物品系统：只实现脚本用到的查询（表由 --items 夹具给定）。
class FixtureItemSystem : DispatchProxy
{
    public Dictionary<string, ushort> ByName = new(StringComparer.OrdinalIgnoreCase);
    public Dictionary<ushort, string> ByIdx = new();
    public Dictionary<ushort, int> Weight = new();

    public void Add(string name, ushort idx, int weight)
    {
        ByName[name] = idx;
        ByIdx[idx] = name;
        Weight[idx] = weight;
    }

    protected override object Invoke(MethodInfo m, object[] args)
    {
        switch (m.Name)
        {
            case "GetStdItemIdx":
                return ByName.TryGetValue((string)args[0], out var foundIdx) ? foundIdx : (ushort)0;
            case "GetStdItemName":
                return ByIdx.TryGetValue((ushort)args[0], out var itemName) ? itemName : "";
            case "GetStdItem":
                {
                    var obj = args[0];
                    ushort itemIdx = obj is string s
                        ? (ByName.TryGetValue(s, out var v) ? v : (ushort)0)
                        : Convert.ToUInt16(obj);
                    if (!ByIdx.ContainsKey(itemIdx)) return null;
                    return new StdItem
                    {
                        Name = ByIdx[itemIdx],
                        Weight = (byte)Weight[itemIdx],
                        Price = 100,
                        Stock = 100,
                    };
                }
            case "CopyToUserItemFromName":
                {
                    var name = (string)args[0];
                    if (!ByName.TryGetValue(name, out var copyIdx)) return false;
                    var item = new UserItem { Index = copyIdx, MakeIndex = ++_makeIndex };
                    args[1] = item; // ref 参数
                    return true;
                }
            default:
                {
                    var rt = m.ReturnType;
                    if (rt == typeof(bool)) return true;
                    if (rt == typeof(int)) return 0;
                    if (rt == typeof(string)) return "";
                    if (rt == typeof(ushort)) return (ushort)0;
                    if (rt.IsValueType) return Activator.CreateInstance(rt);
                    return null;
                }
        }
    }

    private int _makeIndex;
}

/// 记录副作用的玩家 mock。
class RunPlayer : DispatchProxy
{
    public List<string> Log = new();
    public int GoldValue;
    public int Level = 10;
    public List<UserItem> Items = new();
    public Dictionary<ushort, string> ItemNames;
    public Dictionary<short, byte> Flags = new();
    public UserItem[] UseItemsStub = Enumerable.Range(0, 12).Select(_ => new UserItem()).ToArray();

    static short ToShort(object o) => (short)Math.Clamp(Convert.ToInt32(o), short.MinValue, short.MaxValue);

    public static string Esc(string s) => (s ?? "").Replace("\r", "\\r").Replace("\n", "\\n");

    public UserItem FindItem(string name)
    {
        foreach (var it in Items)
            if (ItemNames.TryGetValue(it.Index, out var n) && n.Equals(name, StringComparison.OrdinalIgnoreCase))
                return it;
        return null;
    }

    protected override object Invoke(MethodInfo m, object[] args)
    {
        var name = m.Name;
        switch (name)
        {
            case "SendMsg":
                {
                    var a = args;
                    // 两个重载：带 IActor 时参数整体后移一位
                    int off = a.Length >= 6 && a[0] is IActor ? 1 : 0;
                    var text = a.Length > off + 5 ? a[off + 5] as string : "";
                    Log.Add($"msg {a[off]} {a[off + 1]} {a[off + 2]} {a[off + 3]} {a[off + 4]} {Esc(text)}");
                    return null;
                }
            case "IncGold":
                {
                    var v = (int)args[0];
                    GoldValue += v;
                    Log.Add($"gold {v}");
                    return true;
                }
            case "DecGold":
                {
                    var v = (int)args[0];
                    GoldValue -= v;
                    Log.Add($"gold {-v}");
                    return true;
                }
            case "SendAddItem":
                {
                    var it = (UserItem)args[0];
                    Items.Add(it);
                    Log.Add($"additem {ItemNames.GetValueOrDefault(it.Index, "?")} 1");
                    return null;
                }
            case "SendDelItems":
                {
                    var it = (UserItem)args[0];
                    Items.Remove(it);
                    Log.Add($"delitem {ItemNames.GetValueOrDefault(it.Index, "?")} 1");
                    return null;
                }
            case "SetQuestUnitStatus":
                Log.Add($"unitstatus {Convert.ToInt32(args[0])} {Convert.ToInt32(args[1])}");
                return null;
            case "get_Gold":
                return GoldValue;
            case "set_Gold":
                GoldValue = (int)args[0];
                return null;
            case "get_ItemList":
                return Items;
            case "get_UseItems":
                // 10 个空佩戴位（Index=0）：C# 的 TakeWItem 会遍历并跳过
                return UseItemsStub;
            case "get_Abil":
                return new Ability { Level = (byte)Level };
            case "get_ActorId":
                return 4242;
            case "get_ChrName":
                return "测试玩家";
            case "get_MapName":
                return "0";
            case "get_CurrX":
                return (short)300;
            case "get_CurrY":
                return (short)300;
            case "IsEnoughBag":
                return true;
            case "GetQuestFalgStatus":
                return Flags.TryGetValue(Convert.ToInt16(args[0]), out var f) ? f : (byte)0;
            case "SetQuestFlagStatus":
                Flags[ToShort(args[0])] = (byte)Math.Clamp(Convert.ToInt32(args[1]), 0, 255);
                return null;
            case "CheckItemCount":
                {
                    var it = FindItem((string)args[0]);
                    if (args.Length > 1 && args[1] != null) args[1] = it != null ? 1 : 0; // ref nCount
                    return it;
                }
            default:
                {
                    if (name.StartsWith("set_")) return null;
                    if (name.StartsWith("get_"))
                    {
                        var rt = m.ReturnType;
                        if (rt == typeof(string)) return "";
                        if (rt == typeof(int)) return 0;
                        if (rt == typeof(bool)) return false;
                        if (rt == typeof(byte)) return (byte)0;
                        if (rt == typeof(short)) return (short)0;
                        if (rt.IsValueType) return Activator.CreateInstance(rt);
                        return null;
                    }
                    return null;
                }
        }
    }
}

class NpcRunProxy : DispatchProxy
{
    public List<ScriptInfo> Scripts = new();
    public string Says = "";

    protected override object Invoke(MethodInfo m, object[] args)
    {
        switch (m.Name)
        {
            case "AddScript":
                Scripts.Add((ScriptInfo)args[0]);
                return null;
            case "get_ScriptList":
                return Scripts;
            case "get_ActorId":
                return 7;
            case "get_ChrName":
                return "测试NPC";
            case "get_MapName":
                return "0";
            case "get_CurrX":
                return (short)100;
            case "get_CurrY":
                return (short)200;
            case "SendMsg":
                return null;
            case "GetLineVariableText":
                return args[1];
            default:
                {
                    if (m.Name.StartsWith("get_"))
                    {
                        var rt = m.ReturnType;
                        if (rt == typeof(string)) return "";
                        if (rt.IsValueType) return Activator.CreateInstance(rt);
                        return null;
                    }
                    return null;
                }
        }
    }
}

static class RunMode
{
    public static int Run(string[] args, CountingSink sink)
    {
        // run <脚本文件> <label> [--gold N] [--level N] [--items name:idx:weight,...] [--player-items name:count,...]
        if (args.Length < 3)
        {
            Console.Error.WriteLine("usage: run <脚本文件> <label> [--gold N] [--level N] [--items 名:序号:重量,...] [--player-items 名:件数,...]");
            return 2;
        }
        var scriptPath = Path.GetFullPath(args[1]);
        var label = args[2];
        int gold = 1000, level = 10;
        var itemTable = new List<(string, ushort, int)>();
        var playerItems = new List<(string, int)>();
        for (int i = 3; i + 1 < args.Length; i += 2)
        {
            switch (args[i])
            {
                case "--gold": gold = int.Parse(args[i + 1]); break;
                case "--level": level = int.Parse(args[i + 1]); break;
                case "--items":
                    foreach (var spec in args[i + 1].Split(',', StringSplitOptions.RemoveEmptyEntries))
                    {
                        var p = spec.Split(':');
                        itemTable.Add((p[0], ushort.Parse(p[1]), p.Length > 2 ? int.Parse(p[2]) : 1));
                    }
                    break;
                case "--player-items":
                    foreach (var spec in args[i + 1].Split(',', StringSplitOptions.RemoveEmptyEntries))
                    {
                        var p = spec.Split(':');
                        playerItems.Add((p[0], p.Length > 1 ? int.Parse(p[1]) : 1));
                    }
                    break;
                default:
                    Console.Error.WriteLine($"未知参数: {args[i]}");
                    return 2;
            }
        }

        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        LogService.Logger = new Serilog.LoggerConfiguration().WriteTo.Sink(sink).CreateLogger();
        var itemProxy = DispatchProxy.Create<IItemSystem, FixtureItemSystem>();
        var itemSystem = (FixtureItemSystem)(object)itemProxy;
        foreach (var (n, idx, w) in itemTable) itemSystem.Add(n, idx, w);
        SystemShare.ItemSystem = itemProxy;

        var npc = DispatchProxy.Create<INormNpc, NpcRunProxy>() as NpcRunProxy;
        var parsers = new ScriptParsers();
        parsers.LoadScriptFile((INormNpc)npc, Path.GetDirectoryName(scriptPath) ?? "", Path.GetFileNameWithoutExtension(scriptPath), true);

        var player = DispatchProxy.Create<IPlayerActor, RunPlayer>() as RunPlayer;
        player.GoldValue = gold;
        player.Level = level;
        player.ItemNames = itemSystem.ByIdx;
        foreach (var (n, cnt) in playerItems)
        {
            if (itemSystem.ByName.TryGetValue(n, out var idx))
                for (int k = 0; k < cnt; k++)
                    player.Items.Add(new UserItem { Index = idx, MakeIndex = 1000 + player.Items.Count });
        }

        var engine = new ScriptEngine();
        try
        {
            engine.GotoLable((INormNpc)npc, (IPlayerActor)player, label, false);
        }
        catch (Exception ex)
        {
            // 参照实现崩溃也是可观测结果：输出已收集日志 + EX 标记行（供两侧对拍）
            foreach (var line in player.Log) Console.WriteLine(line);
            Console.WriteLine($"EX {ex.GetType().Name}");
            return 0;
        }

        foreach (var line in player.Log) Console.WriteLine(line);
        // C# 侧全部 Error 级消息也纳入日志：两侧错误文本需逐条一致
        foreach (var msg in sink.AllErrors)
        {
            Console.WriteLine($"err {msg}");
        }
        Console.WriteLine($"#gold={player.GoldValue}");
        Console.WriteLine($"#items={string.Join(",", player.Items.Select(i => itemSystem.ByIdx.GetValueOrDefault(i.Index, "?")))}");
        return 0;
    }
}
