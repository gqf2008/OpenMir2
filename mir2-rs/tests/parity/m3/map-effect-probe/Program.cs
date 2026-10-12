// map-effect-probe —— oracle 真身实测「DoSpell 里 map-bound 技能效果」的向量 + 离线可行性三分类。
//
// 第十片（卡 openmir2-m3-combat-skills-align，DoD③）：
//   第七片把 10 条 `context_required` 二次分类后，其中 7 条（8/17/21/30/37/41/46）标成
//   `map_context`（「需真 Envir/可见集/召唤」）而 deferred。本片逐条判「能不能离线做」：
//     (a) 只需真 Envir/地图数据 ⇒ 探针离线可做，本片当场做成真身向量：
//         - 8 / 37 `MagPushArround`（`MagicManager.cs:24-52`）
//         - 21 `MagSaceMove`（`:876-891`）
//         - 46 `MagMakeClone`（`:1337-1341`）
//     (c) 需**真的召唤**怪物/她者（走 `SystemShare.WorldEngine.RegenMonsterByName`）：
//         - 17 `MagMakeSlave`（`:1308-1335`）、30 `MagMakeSinSuSlave`（`:1269-1306`）、
//           41 `MagMakeAngelSlave`（`:1343-1354`）—— 本片只登记「缺什么」，不冒充真身。
//
// 手法（只读真身，不连 MySQL、不发 socket、不起栈、不占端口）：
//   1. 在临时目录合成一份**真格式** `.map`（52B 头 + 12B/格，与 `Envirnoment.LoadMapData` 同格式），
//      交给真身 `Envirnoment.LoadMapData` 加载 ⇒ 真 `Envir`（`CanWalk`/`MoveToMovingObject`/`AddMapObject` 全真身）；
//   2. 搭真 `PlayObject` 施法者 + 真 `MonsterObject` 目标，按 oracle 语义填 `VisibleActors`；
//   3. 反射直调私有 static 入口；`System.Random` 覆盖为记录包装（`ReplayRandom` 行式账本）；
//   4. 21 的 `SystemShare.MapMgr` 用**最小 `IMapSystem` 替身**（只做 `FindMap` 注册表；效果路径
//      `MagSaceMove`→`MapBase.MapRandomMove`→`SpaceMove` 全真身）——探针里显式分栏，不混称。
//
// 用法（**在仓库根运行**）：
//   dotnet run --project mir2-rs/tests/parity/m3/map-effect-probe -c Release -- <仓库根> <向量输出> <登记输出>

using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Text;
using System.Text.Json;
using M2Server;
using M2Server.Actor;
using M2Server.Magic;
using M2Server.Monster;
using M2Server.Maps;
using M2Server.Player;
using OpenMir2;
using OpenMir2.Data;
using OpenMir2.Enums;
using OpenMir2.Packets.ClientPackets;
using Serilog;
using SystemModule;
using SystemModule.Actors;
using SystemModule.Data;
using SystemModule.Enums;
using SystemModule.Maps;
using SystemModule.SubSystem;

namespace MapEffectProbe;

internal static class Program
{
    private const int RngSeed = 20261012;
    private const string ProbeMapName = "probe_map";
    private const string ProbeMap2Name = "probe_map2";

    /// <summary>`System.Random` 的记录包装：按 `ReplayRandom` 的行式格式（seq|kind|arg|result|site）记账。</summary>
    private sealed class RecordingRandom : Random
    {
        public readonly List<string> Draws = new();
        public RecordingRandom(int seed) : base(seed) { }

        private void Log(string kind, long arg, long value, string site)
        {
            Draws.Add($"{Draws.Count + 1}|{kind}|{arg}|{value}|{site}");
        }

        public override int Next()
        {
            int v = base.Next();
            Log("Next()", 0, v, "next");
            return v;
        }

        public override int Next(int maxValue)
        {
            int v = base.Next(maxValue);
            Log("Next(max)", maxValue, v, "max");
            return v;
        }

        public override int Next(int minValue, int maxValue)
        {
            int v = base.Next(minValue, maxValue);
            Log("Next(min,max)", ((long)minValue << 32) | (uint)maxValue, v, "range");
            return v;
        }
    }

    private static FieldInfo _rngField;

    private static void SeedRng(int seed)
    {
        RandomNumber.GetInstance();
        _rngField ??= typeof(RandomNumber).GetField("random", BindingFlags.NonPublic | BindingFlags.Static)
            ?? throw new InvalidOperationException("找不到 OpenMir2.RandomNumber.random 私有静态字段");
        _rngField.SetValue(null, new RecordingRandom(seed));
    }

    private static string Draws()
        => string.Join(";", ((RecordingRandom)_rngField.GetValue(null)).Draws);

    // ---- 真身对象 ---------------------------------------------------------------------------------

    private static PlayObject NewCaster(byte level)
    {
        var po = new PlayObject
        {
            Race = ActorRace.Play,
            Job = PlayerJob.Warrior,
            Abil = new Ability { Level = level, HP = 500, MP = 500, MaxHP = 500, MaxMP = 500 },
            AddAbil = new AddAbility(),
            MagicList = new List<UserMagic>(),
            ItemList = new List<UserItem>(),
            UseItems = new UserItem[13],
            SlaveList = new List<IMonsterActor>(),
            IsRobot = false,
            AttatckMode = 0, // HAM_ALL（新角色 PlayObject.cs:908）
            IsTimeRecall = true, // 便于观测 MagSaceMove 的 IsTimeRecall=false 分支
        };
        po.WAbil = new Ability { DC = 0x1E14, MC = 0x1E14, SC = 0x2832, HP = 500, MP = 500, MaxHP = 500, MaxMP = 500 };
        po.HitPoint = 20;
        return po;
    }

    private static MonsterObject NewMonster(byte level, int actorId, short x, short y)
    {
        var mo = new MonsterObject();
        mo.Race = ActorRace.Monster;
        mo.ChrName = "map_probe_target";
        mo.ActorId = actorId;
        mo.LifeAttrib = 0;
        mo.Abil = new Ability { Level = level, HP = 500, MP = 0, MaxHP = 500, MaxMP = 0 };
        mo.WAbil = new Ability { AC = 5, HP = 500, MaxHP = 500 };
        mo.AddAbil = new AddAbility();
        mo.CurrX = x;
        mo.CurrY = y;
        mo.Dir = Direction.Down;
        return mo;
    }

    // ---- 合成真格式 .map（52B 头 + 12B/格，列主序，与 Envirnoment.LoadMapData 一致）------------------

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
                ushort wBk = 0x8000; // 高墙 ⇒ CellAttribute.HighWall（不可走）
                ushort wMid = 0;
                ushort wFr = 0;
                if (!walls.Contains((nW, nH)))
                {
                    wBk = 0;
                }
                bw.Write(wBk);
                bw.Write(wMid);
                bw.Write(wFr);
                bw.Write((byte)0); // doorIndex
                bw.Write((byte)0); // doorOffset
                bw.Write((byte)0); // aniFrame
                bw.Write((byte)0); // aniTick
                bw.Write((byte)0); // area
                bw.Write((byte)0); // light
            }
        }
    }

    private static Envirnoment NewMap(string path, string mapName, short width, short height, ISet<(short, short)> walls)
    {
        WriteMap(path, width, height, walls);
        var envir = new Envirnoment();
        if (!envir.LoadMapData(path))
        {
            throw new InvalidDataException($"真身 LoadMapData 失败: {path}");
        }
        envir.MapName = mapName;
        envir.MapFileName = Path.GetFileName(path);
        envir.ServerIndex = 0;
        return envir;
    }

    /// <summary>最小 `IMapSystem` 替身：只实现 `FindMap` 注册表查找（其余为确证的 no-op）。
    /// 效果路径 `MagSaceMove`→`MapBase.MapRandomMove`→`SpaceMove` 全部走真身 M2Server 代码。</summary>
    private sealed class ProbeMapSystem : IMapSystem
    {
        private readonly Dictionary<string, IEnvirnoment> _maps = new(StringComparer.OrdinalIgnoreCase);

        public void Register(IEnvirnoment envir) => _maps[envir.MapName] = envir;

        public IList<IEnvirnoment> Maps => _maps.Values.ToList();
        public void AddMapInfo(string sMapName, string sMapDesc, byte nServerNumber, MapInfoFlag mapFlag, IMerchant questNpc) { }
        public bool AddMapRoute(string sSMapNo, int nSMapX, int nSMapY, string sDMapNo, int nDMapX, int nDMapY) => false;
        public IEnvirnoment FindMap(string sMapName) => _maps.TryGetValue(sMapName ?? string.Empty, out IEnvirnoment e) ? e : null;
        public IList<IEnvirnoment> GetDoorMapList() => new List<IEnvirnoment>();
        public IEnvirnoment GetMapInfo(int nServerIdx, string sMapName) => FindMap(sMapName);
        public int GetMapOfServerIndex(string sMapName) => 0;
        public IList<IEnvirnoment> GetMineMaps() => new List<IEnvirnoment>();
        public void LoadMapDoor() { }
        public void MakeSafePkZone() { }
    }

    // ---- 锚点自检（防 F-1 同族的行号漂移）----------------------------------------------------------

    private sealed record Anchor(string Entry, int Start, int End);

    private static readonly Dictionary<string, Anchor> Anchors = new()
    {
        ["MagPushArround"] = new Anchor("MagPushArround", 24, 52),
        ["MagSaceMove"] = new Anchor("MagSaceMove", 876, 891),
        ["MagMakeClone"] = new Anchor("MagMakeClone", 1337, 1341),
        ["MagMakeSlave"] = new Anchor("MagMakeSlave", 1308, 1335),
        ["MagMakeSinSuSlave"] = new Anchor("MagMakeSinSuSlave", 1269, 1306),
        ["MagMakeAngelSlave"] = new Anchor("MagMakeAngelSlave", 1343, 1354),
    };

    private static void VerifyAnchors(string repoRoot)
    {
        string path = Path.Combine(repoRoot, "src", "M2Server", "Magic", "MagicManager.cs");
        string[] lines = File.ReadAllLines(path);
        foreach (Anchor a in Anchors.Values)
        {
            string startLine = lines[a.Start - 1];
            string endLine = lines[a.End - 1];
            if (!startLine.Contains(a.Entry + "("))
            {
                throw new InvalidDataException($"锚点首行不含 {a.Entry}( ：{a.Entry}:{a.Start}-{a.End}\n{startLine}");
            }
            if (endLine.Trim() != "}")
            {
                throw new InvalidDataException($"锚点末行不是 }} ：{a.Entry}:{a.Start}-{a.End}\n{endLine}");
            }
            if (a.End < lines.Length)
            {
                string next = lines[a.End].Trim();
                if (next != string.Empty && next != "}")
                {
                    throw new InvalidDataException($"锚点末行下一行非空非 }} （行号少算/多算？）：{a.Entry}:{a.Start}-{a.End}\n{lines[a.End]}");
                }
            }
        }
    }

    // ---- 主流程 -----------------------------------------------------------------------------------

    private static (MethodInfo Push, MethodInfo Sace, MethodInfo Clone, MethodInfo MakeSlave,
        MethodInfo MakeSinSuSlave, MethodInfo MakeAngelSlave) Methods()
    {
        MethodInfo Get(string name) => typeof(MagicManager)
            .GetMethod(name, BindingFlags.NonPublic | BindingFlags.Static)
            ?? throw new MissingMethodException("MagicManager." + name);
        return (Get("MagPushArround"), Get("MagSaceMove"), Get("MagMakeClone"), Get("MagMakeSlave"),
            Get("MagMakeSinSuSlave"), Get("MagMakeAngelSlave"));
    }

    public static int Main(string[] args)
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        LogService.Logger = new LoggerConfiguration().CreateLogger();
        M2Share.ServerIndex = 0;

        string repoRoot = args.Length > 0 ? args[0] : Directory.GetCurrentDirectory();
        string outDir = Path.Combine(repoRoot, "mir2-rs", "tests", "parity", "m3", "map-effect-probe", "vectors");
        string vecPath = args.Length > 1 ? args[1] : Path.Combine(outDir, "map_effect_vectors.json");
        string regPath = args.Length > 2 ? args[2] : Path.Combine(outDir, "context_reachability.tsv");

        VerifyAnchors(repoRoot);
        (MethodInfo push, MethodInfo sace, MethodInfo clone, MethodInfo makeSlave,
            MethodInfo makeSinSuSlave, MethodInfo makeAngelSlave) = Methods();

        string tmp = Path.Combine(Path.GetTempPath(), "m3-map-effect-probe");
        Directory.CreateDirectory(tmp);
        try
        {
            const short W = 60;
            const short H = 60;

            // 地图 A：全可走（push 的位移不受阻挡）。
            var envirA = NewMap(Path.Combine(tmp, "probe_map.map"), ProbeMapName, W, H, new HashSet<(short, short)>());
            // 地图 B：默认全可走 + 指定墙（push 的阻挡分支 / sace 的落点）。
            var wallsB = new HashSet<(short, short)> { (33, 30), (32, 29), (30, 27), (34, 30) };
            var envirB = NewMap(Path.Combine(tmp, "probe_map2.map"), ProbeMap2Name, W, H, wallsB);
            // 地图 C：sace 的「换到另一张图」目标（与 A 不同对象）。
            var envirC = NewMap(Path.Combine(tmp, "probe_map3.map"), "probe_map3", W, H, new HashSet<(short, short)>());

            var mapSys = new ProbeMapSystem();
            mapSys.Register(envirA);
            mapSys.Register(envirB);
            mapSys.Register(envirC);
            SystemShare.MapMgr = mapSys;

            var cases = new List<object>();
            var vecTsv = new List<string> { string.Join("\t", "seed", RngSeed) };

            // ---- MagPushArround（8 / 37）真身向量 --------------------------------------------------
            // 目标规格：id,x,y,level,death,stick,proper （proper 由 oracle `IsProperTarget` 实测后记录）
            var pushCases = new (string Name, int MagicId, short Width, short Height, ISet<(short, short)> Walls,
                short Cx, short Cy, byte CasterLevel, int PushLevel, string Targets, int Seeds)[]
            {
                ("p_all_dir", 8, W, H, new HashSet<(short, short)>(), 30, 30, 50, 3,
                    "1,31,30,10,0,0;2,29,30,10,0,0;3,30,29,10,0,0;4,30,31,10,0,0;5,31,31,10,0,0;6,31,29,10,0,0;7,29,31,10,0,0;8,29,29,10,0,0", 12),
                ("p_wall_block", 8, W, H, wallsB, 30, 30, 50, 3,
                    "1,31,30,10,0,0;2,29,30,10,0,0;3,30,29,10,0,0;4,30,31,10,0,0", 12),
                ("p_gates", 37, W, H, new HashSet<(short, short)>(), 30, 30, 50, 1,
                    "1,31,30,10,0,0;2,29,30,60,0,0;3,30,29,10,1,0;4,30,31,10,0,1;5,35,30,10,0,0;6,31,30,10,0,0", 16),
                ("p_self_id", 8, W, H, new HashSet<(short, short)>(), 30, 30, 50, 3,
                    "1,31,30,10,0,0;2,29,30,10,0,0", 16),
            };
            foreach (var cfg in pushCases)
            {
                for (int k = 0; k < cfg.Seeds; k++)
                {
                    int seed = RngSeed ^ (0x5000 + cfg.MagicId * 4096 + cfg.CasterLevel * 8 + k);
                    Envirnoment envir = cfg.Walls.Count == 0
                        ? (cfg.Name == "p_wall_block" ? envirB : envirA)
                        : envirB;
                    var caster = NewCaster(cfg.CasterLevel);
                    caster.ActorId = 930000 + k; // 固定 ActorId（否则 Guid 分配污染向量可复现性）
                    caster.Envir = envir;
                    caster.CurrX = cfg.Cx;
                    caster.CurrY = cfg.Cy;
                    caster.Dir = Direction.Down;
                    var targets = new List<MonsterObject>();
                    var targetSpecs = new List<string>();
                    foreach (string spec in cfg.Targets.Split(';', StringSplitOptions.RemoveEmptyEntries))
                    {
                        string[] f = spec.Split(',');
                        int id = int.Parse(f[0], CultureInfo.InvariantCulture);
                        short tx = short.Parse(f[1], CultureInfo.InvariantCulture);
                        short ty = short.Parse(f[2], CultureInfo.InvariantCulture);
                        byte lv = byte.Parse(f[3], CultureInfo.InvariantCulture);
                        bool dead = f[4] == "1";
                        bool stick = f[5] == "1";
                        var mo = NewMonster(lv, 920000 + id, tx, ty);
                        mo.Envir = envir;
                        mo.Death = dead;
                        mo.StickMode = stick;
                        // p_self_id 场景：把目标 ActorId 设成与施法者相同 ⇒ oracle IsProperTarget 直接 false。
                        if (cfg.Name == "p_self_id" && id == 2)
                        {
                            mo.ActorId = caster.ActorId;
                        }
                        targets.Add(mo);
                        caster.VisibleActors.Add(new VisibleBaseObject { BaseObject = mo, VisibleFlag = VisibleFlag.Show });
                        targetSpecs.Add($"{mo.ActorId},{tx},{ty},{lv},{(dead ? 1 : 0)},{(stick ? 1 : 0)},{mo.Dir}");
                    }
                    SeedRng(seed);
                    int result = (int)push.Invoke(null, new object[] { caster, cfg.PushLevel });
                    string draws = Draws();
                    var outcomes = new List<string>();
                    for (int i = 0; i < targets.Count; i++)
                    {
                        MonsterObject mo = targets[i];
                        bool proper = caster.IsProperTarget(mo);
                        outcomes.Add($"{mo.CurrX},{mo.CurrY},{mo.Dir}");
                        targetSpecs[i] = targetSpecs[i] + "," + (proper ? 1 : 0);
                    }
                    string targetSpec = string.Join(";", targetSpecs);
                    string wallSpec = string.Join(";", cfg.Walls.Select(w => $"{w.Item1},{w.Item2}"));
                    string outcomeSpec = string.Join(";", outcomes);
                    vecTsv.Add(string.Join("\t", "push", cfg.Name, seed, cfg.MagicId, caster.ActorId,
                        cfg.Cx, cfg.Cy, cfg.CasterLevel, cfg.PushLevel, cfg.Width, cfg.Height,
                        wallSpec, targetSpec, draws, result, outcomeSpec));
                    cases.Add(new
                    {
                        kind = "push_arround",
                        case_name = cfg.Name,
                        magic_id = cfg.MagicId,
                        seed,
                        map = new { width = (int)cfg.Width, height = (int)cfg.Height, walls = wallSpec },
                        caster = new { actor_id = caster.ActorId, x = (int)cfg.Cx, y = (int)cfg.Cy, level = (int)cfg.CasterLevel, push_level = cfg.PushLevel },
                        targets = targetSpec,
                        draws,
                        result,
                        outcomes = outcomeSpec,
                    });
                }
            }

            // ---- MagSaceMove（21）真身向量 ---------------------------------------------------------
            // 场景：0 = 同图同对象（IsTimeRecall 不变）；1 = 换到 probe_map3（IsTimeRecall=false）；
            //       2 = HomeMap 找不到；3 = 门未过（由种子决定）。
            var saceScenarios = new (int Kind, string HomeMap, byte Level)[] { (0, ProbeMapName, 3), (1, "probe_map3", 3), (2, "no_such_map", 3), (0, ProbeMapName, 0) };
            for (int si = 0; si < saceScenarios.Length; si++)
            {
                var sc = saceScenarios[si];
                for (int k = 0; k < 24; k++)
                {
                    int seed = RngSeed ^ (0x7A00 + si * 1024 + k);
                    var caster = NewCaster(50);
                    caster.ActorId = 940000 + si * 100 + k; // 固定 ActorId（可复现性）
                    caster.Envir = envirA;
                    caster.CurrX = 30;
                    caster.CurrY = 30;
                    caster.HomeMap = sc.HomeMap;
                    caster.IsTimeRecall = true;
                    envirA.AddMapObject(caster.CurrX, caster.CurrY, CellType.Play, caster.ActorId, caster);
                    SeedRng(seed);
                    bool result = (bool)sace.Invoke(null, new object[] { caster, sc.Level });
                    string draws = Draws();
                    string kindName = sc.Kind switch { 0 => "same_map", 1 => "other_map", _ => "map_missing" };
                    vecTsv.Add(string.Join("\t", "sace", $"{kindName}_l{sc.Level}", seed, sc.Level,
                        sc.Kind == 2 ? 0 : 1, sc.Kind == 0 ? 1 : 0, (int)W, (int)H,
                        caster.CurrX, caster.CurrY, draws, result ? 1 : 0, caster.SpaceMoved ? 1 : 0,
                        caster.IsTimeRecall ? 1 : 0, caster.Envir.MapName));
                    cases.Add(new
                    {
                        kind = "sace_move",
                        case_name = $"{kindName}_l{sc.Level}",
                        magic_id = 21,
                        seed,
                        level = (int)sc.Level,
                        map_found = sc.Kind != 2,
                        same_envir = sc.Kind == 0,
                        width = (int)W,
                        height = (int)H,
                        x = (int)caster.CurrX,
                        y = (int)caster.CurrY,
                        draws,
                        result,
                        space_moved = caster.SpaceMoved,
                        is_time_recall = caster.IsTimeRecall,
                        map_name = caster.Envir.MapName,
                    });
                }
            }

            // ---- MagMakeClone（46）真身向量 --------------------------------------------------------
            // `MagMakeClone` 体 = `new PlayCloneObject(playObject); return true;`（MagicManager.cs:1337-1341）。
            // ① 反射直调 `MagMakeClone` 拿返回值 + 地图格新增（证明 wrapper 真身）；
            // ② 再**直构** `PlayCloneObject`（= wrapper 的全部效果代码）拿逐字段观测。
            for (int d = 0; d < 8; d++)
            {
                int seed = RngSeed ^ (0x4600 + d);
                var envir = NewMap(Path.Combine(tmp, "clone_map.map"), "clone_map", 20, 20, new HashSet<(short, short)>());
                var caster = NewCaster(50);
                caster.ActorId = 950000 + d; // 固定 ActorId（可复现性）
                caster.Envir = envir;
                caster.CurrX = 10;
                caster.CurrY = 10;
                caster.Dir = (byte)d;
                int before = envir.GetCellInfo(10, 10, out bool _).Count;
                SeedRng(seed);
                bool magicResult = (bool)clone.Invoke(null, new object[] { caster, null });
                int after = envir.GetCellInfo(10, 10, out bool _).Count;
                var cloneObj = new PlayCloneObject(caster);
                vecTsv.Add(string.Join("\t", "clone", $"dir{d}", seed, d, 10, 10,
                    magicResult ? 1 : 0, after - before, cloneObj.ChrName, cloneObj.CurrX, cloneObj.CurrY,
                    cloneObj.Dir, ReferenceEquals(cloneObj.Envir, envir) ? 1 : 0));
                cases.Add(new
                {
                    kind = "make_clone",
                    case_name = $"dir{d}",
                    magic_id = 46,
                    seed,
                    caster_dir = d,
                    x = 10,
                    y = 10,
                    magic_result = magicResult,
                    created = after - before,
                    name = cloneObj.ChrName,
                    clone_x = (int)cloneObj.CurrX,
                    clone_y = (int)cloneObj.CurrY,
                    clone_dir = cloneObj.Dir,
                    same_envir = ReferenceEquals(cloneObj.Envir, envir),
                });
                envir.Dispose();
            }

            // ---- 7 行离线可行性三分类（row_class / missing_prereq）--------------------------------
            var reachRows = new List<string>
            {
                "row\tmagic_id\tname\teffect_entry\toracle_anchor\trow_class\tmissing_prereq",
                "row\t8\t抗拒火环\tMagPushArround\tMagicManager.cs:24-52\ta_offline_done\t-",
                "row\t37\t雷神之怒\tMagPushArround\tMagicManager.cs:24-52\ta_offline_done\t-",
                "row\t21\t瞬息移动\tMagSaceMove\tMagicManager.cs:876-891\ta_offline_done\t探针替身 SystemShare.MapMgr（只 FindMap 字典，见探针头注）",
                "row\t46\t分身术\tMagMakeClone\tMagicManager.cs:1337-1341\ta_offline_done\t-",
                "row\t17\t召唤骷髅\tMagMakeSlave\tMagicManager.cs:1308-1335\tc_real_summon\tBaseObject.cs:1185 SystemShare.WorldEngine.RegenMonsterByName（GameSrv 宿主实现，M2Server 探针不可达）+ MonsterInfo 配置",
                "row\t30\t召唤神兽\tMagMakeSinSuSlave\tMagicManager.cs:1269-1306\tc_real_summon\tBaseObject.cs:1185 SystemShare.WorldEngine.RegenMonsterByName（GameSrv 宿主实现，M2Server 探针不可达）+ SystemShare.Config.Dragon/DragonArray 配置",
                "row\t41\t狮子吼\tMagMakeAngelSlave\tMagicManager.cs:1343-1354\tc_real_summon\tBaseObject.cs:1185 SystemShare.WorldEngine.RegenMonsterByName（GameSrv 宿主实现，M2Server 探针不可达）+ SystemShare.Config.Angel 配置",
            };

            Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(vecPath)));
            var opts = new JsonSerializerOptions { WriteIndented = true };
            var payload = new
            {
                generated_by = "mir2-rs/tests/parity/m3/map-effect-probe",
                oracle = new
                {
                    mag_push_arround = "M2Server.Magic.MagicManager.MagPushArround（src/M2Server/Magic/MagicManager.cs:24-52，private static，反射调用）",
                    mag_sace_move = "M2Server.Magic.MagicManager.MagSaceMove（:876-891）",
                    mag_make_clone = "M2Server.Magic.MagicManager.MagMakeClone（:1337-1341）",
                    map = "合成真格式 .map（12B/格）→ 真身 Envirnoment.LoadMapData（src/M2Server/Maps/Envirnoment.cs:903）",
                    rng = "OpenMir2.RandomNumber.random 覆盖为 RecordingRandom(seed)",
                    map_mgr = "21：SystemShare.MapMgr = 探针最小 IMapSystem 替身（只 FindMap）；效果路径全真身",
                },
                seed = RngSeed,
                cases,
            };
            File.WriteAllText(vecPath, JsonSerializer.Serialize(payload, opts) + Environment.NewLine, new UTF8Encoding(false));
            File.WriteAllText(Path.ChangeExtension(vecPath, ".tsv"), string.Join("\n", vecTsv) + "\n", new UTF8Encoding(false));
            File.WriteAllText(regPath, string.Join("\n", reachRows) + "\n", new UTF8Encoding(false));
            Console.WriteLine($"map-effect-probe: wrote {vecPath} ({cases.Count} cases) / {regPath} ({reachRows.Count - 1} rows)");
            return 0;
        }
        finally
        {
            try { Directory.Delete(tmp, true); } catch { /* 临时目录清理失败不影响结果 */ }
        }
    }
}
