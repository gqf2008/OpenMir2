// M2 下半程金标准驱动：**会话进世界 + 掉落系统 +（本批新增）战斗结算**。
// 对齐 Rust `crate::world`；场景 `session`（无战斗）与 `combat`（含 `attack`）共用本驱动。
//
// 顺序契约（两侧写死）：
//   1. 每 tick：clock += 200 → 应用本 tick 的**会话操作**（enter/switch/softclose/reenter，立即生效）
//      → 入队本 tick 的**世界命令**（spawn/walk/kill/attack/leave，按场景顺序 FIFO）→ 处理队列
//      → 死亡结算（掉落落地 + `KillFunc` 发经验）→ 按插入顺序 AOI → 输出；
//   2. 怪物掉落在地图**生成时**预算（`WorldServer.MonGen.cs:440` 的时机）；
//   3. 死亡时用 `GetDropPosition` 扫描序把物品落到空格子（金币也占一格）。
//   4. `attack`：`GetBaseAttackPoewr()` → `_Attack` → 若 `target.Race > ActorRace.Play` 则
//      `target.SetLastHiter(攻击方)`（折叠 `RM_STRUCK` 消息，见 `StubsCombat.cs::_Attack` 注释）。
//
// 随机源：`ParityGolden.M2Share.RandomNumber`（真实 RandomNumber.cs，反射注入种子）。

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Text;
using System.Text.Json;
using WorldGolden;

internal static class Program
{
    private sealed class MapSpec
    {
        public int width { get; set; }
        public int height { get; set; }
    }

    private sealed class SessionSpec
    {
        public string chr { get; set; }
        public int id { get; set; }
        public int map { get; set; }
        public int x { get; set; }
        public int y { get; set; }
        public int level { get; set; } = 10;
        public int hp { get; set; } = 200;
        public int max_hp { get; set; } = 200;
        public int dc { get; set; }
        public int ac { get; set; }
        public int hit_point { get; set; }
        public int speed_point { get; set; }
        public int job { get; set; }
        public int exp { get; set; }
        public int max_exp { get; set; } = 1000;
        public int view_range { get; set; } = 5;
        public int race { get; set; }
    }

    private sealed class MonsterSpec
    {
        public int id { get; set; }
        public string name { get; set; }
        public int map { get; set; }
        public int x { get; set; }
        public int y { get; set; }
        public int level { get; set; } = 1;
        public int exp { get; set; }
        public int hp { get; set; } = 10;
        public int dc { get; set; } = 1;
        public int ac { get; set; }
        public int undead { get; set; }
        // 命中判定两侧：缺省 = BaseObject 构造的 5 / 15（BaseObject.cs:386-387）
        public int hit_point { get; set; } = 5;
        public int speed_point { get; set; } = 15;
        public List<DropSpec> drop { get; set; } = new List<DropSpec>();
    }

    private sealed class DropSpec
    {
        public int sel_point { get; set; }
        public int max_point { get; set; }
        public string item_name { get; set; }
        public int count { get; set; } = 1;
    }

    private sealed class ItemSpec
    {
        public string name { get; set; }
        public int std_mode { get; set; }
        public int shape { get; set; }
        public int dura_max { get; set; }
    }

    private sealed class CommandSpec
    {
        public int tick { get; set; }
        public string kind { get; set; }
        public int id { get; set; }
        public int target { get; set; }
        public int dx { get; set; }
        public int dy { get; set; }
        public string chr { get; set; }
        public int map { get; set; }
        public int x { get; set; }
        public int y { get; set; }
    }

    private sealed class EntitySpec
    {
        public int id { get; set; }
        public int x { get; set; }
        public int y { get; set; }
        public int view_range { get; set; } = 5;
        public int race { get; set; }
        public bool ghost { get; set; }
        public bool death { get; set; }
        public bool invisible { get; set; }
        public bool fixed_hide_mode { get; set; }
        public bool ob_mode { get; set; }
        public int master { get; set; } = -1;
        public bool nasty_mode { get; set; }
        public bool want_ref_msg { get; set; }
    }

    private sealed class Scenario
    {
        public List<MapSpec> maps { get; set; } = new List<MapSpec>();
        public int width { get; set; }
        public int height { get; set; }
        public int ticks { get; set; }
        public bool extended { get; set; }
        public int mon_random_add_value { get; set; } = 40;
        // `Config.HighLevelKillMonFixExp`（现网 Exps.conf = 1）；场景可覆写以钉住 `CalcGetExp` 的调用口径
        public bool high_level_kill_mon_fix_exp { get; set; } = true;
        // `Config.NeedExps`；不填时全 0（本场景不触发升级，故不索引）
        public List<int> need_exps { get; set; }
        public List<ItemSpec> item_catalog { get; set; } = new List<ItemSpec>();
        public List<SessionSpec> sessions { get; set; } = new List<SessionSpec>();
        public List<MonsterSpec> monsters { get; set; } = new List<MonsterSpec>();
        public List<EntitySpec> entities { get; set; } = new List<EntitySpec>();
        public List<CommandSpec> commands { get; set; } = new List<CommandSpec>();
    }

    private sealed class QueuedCommand
    {
        public string kind;
        public int id;
        public int target;
        public int dx;
        public int dy;
        public MonsterSpec monster;
    }

    private static int Main(string[] args)
    {
        if (args.Length < 1)
        {
            Console.Error.WriteLine("usage: WorldGolden <scenario-name> [out-path]");
            return 2;
        }
        string worldDir = ResolveWorldDir();
        string scenarioPath = Path.Combine(worldDir, "scenarios",
            args[0].EndsWith(".json") ? args[0] : args[0] + ".json");
        string outPath = args.Length >= 2
            ? args[1]
            : Path.Combine(worldDir, "golden", Path.GetFileNameWithoutExtension(scenarioPath) + ".txt");
        var scenario = JsonSerializer.Deserialize<Scenario>(File.ReadAllText(scenarioPath));

        SeedRandomNumber(42);
        ParityGolden.SystemShare.Config.MonRandomAddValue = (byte)scenario.mon_random_add_value;
        CombatConfigStub.HighLevelKillMonFixExp = scenario.high_level_kill_mon_fix_exp;
        int[] needExps = scenario.need_exps != null
            ? scenario.need_exps.ToArray()
            : new int[1000];
        foreach (ItemSpec it in scenario.item_catalog)
        {
            ParityGolden.SystemShare.ItemSystem.StdItemList.Add(new ParityGolden.StdItem
            {
                Name = it.name,
                StdMode = (byte)it.std_mode,
                Shape = (byte)it.shape,
                DuraMax = (ushort)it.dura_max
            });
        }

        var sizes = scenario.maps.Count > 0
            ? scenario.maps.Select(m => (m.width, m.height)).ToList()
            : new List<(int, int)> { (scenario.width, scenario.height) };
        var envirs = sizes.Select(s => new EnvirnomentStub(s.Item1, s.Item2)).ToList();

        VirtualClock.Now = 0;
        ActorMgr.Clear();
        var order = new List<int>();
        var actors = new Dictionary<int, ActorStub>();
        var envirOf = new Dictionary<int, int>();
        var queue = new Queue<QueuedCommand>();
        var sessionSpecs = scenario.sessions.ToDictionary(s => s.chr, s => s);

        // basic 场景的静态实体（进图由 enter 命令驱动）
        foreach (EntitySpec spec in scenario.entities)
        {
            var a = new ActorStub
            {
                ActorId = spec.id,
                ChrName = "e" + spec.id,
                CurrX = spec.x,
                CurrY = spec.y,
                ViewRange = (byte)spec.view_range,
                Race = (byte)spec.race,
                Ghost = spec.ghost,
                Death = spec.death,
                Invisible = spec.invisible,
                FixedHideMode = spec.fixed_hide_mode,
                ObMode = spec.ob_mode,
                NastyMode = spec.nasty_mode,
                WantRefMsg = spec.want_ref_msg,
                Envir = envirs[0],
            };
            a.WAbil.HP = 100;
            a.WAbil.MaxHP = 100;
            RegisterActor(actors, envirOf, order, a, 0);
            queue.Enqueue(new QueuedCommand { kind = "enter", id = spec.id });
        }
        foreach (EntitySpec spec in scenario.entities)
        {
            if (spec.master >= 0 && actors.TryGetValue(spec.master, out ActorStub master))
            {
                actors[spec.id].Master = master;
            }
        }

        var sb = new StringBuilder();
        for (int tick = 1; tick <= scenario.ticks; tick++)
        {
            VirtualClock.Advance(200);
            // 1) 会话操作（立即生效；与 Rust 的 enter_map/switch_map/soft_close/enter_map 对应）
            foreach (CommandSpec c in scenario.commands.Where(c => c.tick == tick))
            {
                switch (c.kind)
                {
                    case "enter":
                        ApplyEnter(envirs, actors, envirOf, order, sessionSpecs, c);
                        break;
                    case "switch":
                        ApplySwitch(envirs, actors, envirOf, sessionSpecs, c);
                        break;
                    case "softclose":
                        ApplySoftClose(envirs, actors, envirOf, order, sessionSpecs, c.chr);
                        break;
                    case "reenter":
                        ApplyReenter(envirs, actors, envirOf, order, sessionSpecs, c);
                        break;
                    case "spawn":
                        queue.Enqueue(new QueuedCommand
                        {
                            kind = "spawn",
                            monster = scenario.monsters.First(m => m.id == c.id)
                        });
                        break;
                    case "walk":
                    case "kill":
                    case "attack":
                    case "leave":
                        queue.Enqueue(new QueuedCommand { kind = c.kind, id = c.id, target = c.target, dx = c.dx, dy = c.dy });
                        break;
                }
            }
            // 2) 世界命令 FIFO
            var tickDeaths = new List<(int id, int placed, int gold, int killer, int exp)>();
            var tickAttacks = new List<(int attacker, int target, int power, int hpAfter)>();
            while (queue.Count > 0)
            {
                QueuedCommand cmd = queue.Dequeue();
                switch (cmd.kind)
                {
                    case "enter":
                        if (actors.TryGetValue(cmd.id, out ActorStub ea))
                        {
                            int mapId = envirOf[cmd.id];
                            envirs[mapId].AddMapObject(ea.CurrX, ea.CurrY, CellType.Play, cmd.id, ea);
                        }
                        break;
                    case "spawn":
                        SpawnMonster(envirs, actors, envirOf, order, cmd.monster);
                        break;
                    case "walk":
                        TryStep(envirs, envirOf, actors, cmd.id, cmd.dx, cmd.dy);
                        break;
                    case "kill":
                        if (actors.TryGetValue(cmd.id, out ActorStub victim))
                        {
                            victim.WAbil.HP = 0;
                        }
                        break;
                    case "attack":
                        if (actors.TryGetValue(cmd.id, out ActorStub attackerObj)
                            && actors.TryGetValue(cmd.target, out ActorStub attackVictim))
                        {
                            // C# `PlayObject.AttackDir`（`PlayObject.Attack.cs:73-75`）的子集：
                            // 先 `GetBaseAttackPoewr()`（消耗攻击力 RNG），再 `_Attack(nPower, target)`
                            // （消耗命中/护甲 RNG）。技能与 `GetAttackPowerHit` 不在此子集。
                            int nPower = attackerObj.GetBaseAttackPoewr();
                            attackerObj._Attack(nPower, attackVictim);
                            // `BaseObject.Attack.cs:61-64` 的 `SendMsg(RM_STRUCK)` 条件：
                            // `targetObject.Race > ActorRace.Play` ⇒ 目标下一次 Run 里 `SetLastHiter(攻击方)`
                            if (attackVictim.Race > 0)
                            {
                                attackVictim.SetLastHiter(attackerObj);
                            }
                            tickAttacks.Add((cmd.id, cmd.target, nPower, attackVictim.WAbil.HP));
                        }
                        break;
                    case "leave":
                        LeaveWorld(envirs, envirOf, actors, order, cmd.id);
                        break;
                }
            }
            // 3) 死亡结算（掉落落地 + `KillFunc` 发经验）
            foreach (int id in order.ToList())
            {
                if (!actors.TryGetValue(id, out ActorStub a) || a.WAbil.HP != 0 || a.DeathSettled)
                {
                    continue;
                }
                a.Death = true;
                a.DeathSettled = true;
                int mapId = envirOf[id];
                int placed = 0;
                if (a.ItemList != null)
                {
                    foreach (ParityGolden.UserItem item in a.ItemList)
                    {
                        if (ActorStub.GetDropPosition(envirs[mapId], a.CurrX, a.CurrY, 3, out int px, out int py))
                        {
                            envirs[mapId].AddMapObject(px, py, CellType.Item, item.Index, a);
                            placed++;
                        }
                    }
                }
                if (a.Gold > 0 && ActorStub.GetDropPosition(envirs[mapId], a.CurrX, a.CurrY, 3, out int gx, out int gy))
                {
                    envirs[mapId].AddMapObject(gx, gy, CellType.Item, id, a);
                    placed++;
                }
                // `BaseObject.cs:2429 KillFunc`：经验发给 `ExpHitter`（**第一个**命中者），不是 `LastHiter`；
                // 子集只做「归属者是玩家本人」这一支（宝宝击杀走 `ExpHitter.Master`，不在本骨架）。
                int killer = -1;
                int gained = 0;
                if (a.ExpHitter != null && a.ExpHitter.Race == 0)
                {
                    killer = a.ExpHitter.ActorId;
                    // `KillTargetTrigger`（`PlayObject.cs:1632-1653`）：
                    // `monsterExp = CalcGetExp(WAbil.Level /*玩家自己*/, fightExp)` → `GainExp(monsterExp)`
                    int monsterExp = a.ExpHitter.CalcGetExp(a.ExpHitter.WAbil.Level, a.FightExp);
                    gained = a.ExpHitter.GainExp(monsterExp, needExps);
                }
                tickDeaths.Add((id, placed, a.Gold, killer, gained));
            }
            // 4) AOI（按插入顺序）
            foreach (int id in order.ToList())
            {
                if (!actors.TryGetValue(id, out ActorStub a))
                {
                    continue;
                }
                if (a.Death)
                {
                    a.SearchViewRangeDeath();
                }
                else
                {
                    a.SearchViewRange();
                }
            }
            // 5) 输出
            sb.Append("tick ").Append(tick).Append(" clock ").Append(VirtualClock.Now).Append('\n');
            foreach (int id in order.Where(actors.ContainsKey).OrderBy(i => i))
            {
                ActorStub a = actors[id];
                sb.Append("ent ").Append(a.ActorId).Append(' ').Append(envirOf[id]).Append(' ')
                    .Append(a.CurrX).Append(' ').Append(a.CurrY).Append(' ')
                    .Append(a.WAbil.HP).Append(' ').Append(a.WAbil.Level).Append(' ').Append(a.WAbil.Exp)
                    .Append(" vis");
                foreach (VisibleBaseObject v in a.VisibleActors)
                {
                    sb.Append(' ').Append(v.BaseObject.ActorId).Append(':').Append((int)v.VisibleFlag);
                }
                sb.Append('\n');
            }
            if (scenario.extended)
            {
                foreach (var (attacker, target, power, hpAfter) in tickAttacks)
                {
                    sb.Append("atk ").Append(attacker).Append(' ').Append(target).Append(' ')
                        .Append(power).Append(' ').Append(hpAfter).Append('\n');
                }
                foreach (var d in tickDeaths)
                {
                    sb.Append("death ").Append(d.id).Append(' ').Append(d.placed).Append(' ')
                        .Append(d.gold).Append(' ').Append(d.killer).Append(' ').Append(d.exp).Append('\n');
                }
                foreach (var (map, x, y, obj) in FloorItems(envirs))
                {
                    sb.Append("floor ").Append(map).Append(' ').Append(x).Append(' ').Append(y).Append(' ').Append(obj).Append('\n');
                }
            }
        }
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outPath)));
        File.WriteAllText(outPath, sb.ToString());
        Console.WriteLine($"world golden: {actors.Count} 实体 × {scenario.ticks} tick -> {outPath}");
        return 0;
    }

    private static void RegisterActor(Dictionary<int, ActorStub> actors, Dictionary<int, int> envirOf,
        List<int> order, ActorStub a, int mapId)
    {
        actors[a.ActorId] = a;
        ActorMgr.Add(a.ActorId, a);
        envirOf[a.ActorId] = mapId;
        order.Add(a.ActorId);
    }

    private static void SpawnMonster(List<EnvirnomentStub> envirs, Dictionary<int, ActorStub> actors,
        Dictionary<int, int> envirOf, List<int> order, MonsterSpec ms)
    {
        // 掉落预算（`WorldServer.MonGen.cs:440` 的时机）：RNG 在这里消耗
        IList<ParityGolden.MonsterDropItem> list = ms.drop
            .Select(d => new ParityGolden.MonsterDropItem
            {
                SelPoint = d.sel_point,
                MaxPoint = d.max_point,
                ItemName = d.item_name,
                Count = d.count
            })
            .ToList();
        var mon = new ParityGolden.MonsterActorStub();
        ParityGolden.WorldServerCopies.MonGetRandomItems(list, mon);

        var a = new ActorStub
        {
            ActorId = ms.id,
            ChrName = ms.name,
            CurrX = ms.x,
            CurrY = ms.y,
            Race = 80,
            Envir = envirs[ms.map],
            ItemList = mon.ItemList ?? new List<ParityGolden.UserItem>(),
            Gold = mon.Gold,
        };
        a.WAbil.Level = (byte)ms.level;
        a.WAbil.HP = (ushort)ms.hp;
        a.WAbil.MaxHP = (ushort)ms.hp;
        a.WAbil.DC = ms.dc;
        a.WAbil.AC = ms.ac;
        a.LifeAttrib = (byte)ms.undead;
        a.HitPoint = (byte)ms.hit_point;
        a.SpeedPoint = (byte)ms.speed_point;
        a.FightExp = ms.exp; // `WorldServer.MonGen.cs:893`: monsterActor.FightExp = monster.Exp
        RegisterActor(actors, envirOf, order, a, ms.map);
        envirs[ms.map].AddMapObject(ms.x, ms.y, CellType.Play, ms.id, a);
    }

    private static void ApplyEnter(List<EnvirnomentStub> envirs, Dictionary<int, ActorStub> actors,
        Dictionary<int, int> envirOf, List<int> order, Dictionary<string, SessionSpec> specs, CommandSpec c)
    {
        SessionSpec ss = specs[c.chr];
        var a = SessionActor(ss, envirs[ss.map], ss.id, ss.x, ss.y);
        RegisterActor(actors, envirOf, order, a, ss.map);
        envirs[ss.map].AddMapObject(ss.x, ss.y, CellType.Play, ss.id, a);
    }

    private static void ApplySwitch(List<EnvirnomentStub> envirs, Dictionary<int, ActorStub> actors,
        Dictionary<int, int> envirOf, Dictionary<string, SessionSpec> specs, CommandSpec c)
    {
        SessionSpec ss = specs[c.chr];
        ActorStub a = actors.Values.FirstOrDefault(x => x.ChrName == c.chr);
        if (a == null)
        {
            return;
        }
        int old = envirOf[a.ActorId];
        RemoveFromCell(envirs[old], a);
        a.CurrX = c.x;
        a.CurrY = c.y;
        a.VisibleActors.Clear();
        a.IsVisibleActive = false;
        a.AddToMaped = false;
        a.Envir = envirs[c.map];
        envirOf[a.ActorId] = c.map;
        envirs[c.map].AddMapObject(c.x, c.y, CellType.Play, a.ActorId, a);
        ss.map = c.map;
    }

    private static void ApplySoftClose(List<EnvirnomentStub> envirs, Dictionary<int, ActorStub> actors,
        Dictionary<int, int> envirOf, List<int> order, Dictionary<string, SessionSpec> specs, string chr)
    {
        ActorStub a = actors.Values.FirstOrDefault(x => x.ChrName == chr);
        if (a == null)
        {
            return;
        }
        int mapId = envirOf[a.ActorId];
        RemoveFromCell(envirs[mapId], a);
        actors.Remove(a.ActorId);
        ActorMgr.Map.Remove(a.ActorId);
        envirOf.Remove(a.ActorId);
        order.Remove(a.ActorId);
    }

    private static void ApplyReenter(List<EnvirnomentStub> envirs, Dictionary<int, ActorStub> actors,
        Dictionary<int, int> envirOf, List<int> order, Dictionary<string, SessionSpec> specs, CommandSpec c)
    {
        SessionSpec ss = specs[c.chr];
        var a = SessionActor(ss, envirs[ss.map], c.id, c.x, c.y);
        RegisterActor(actors, envirOf, order, a, ss.map);
        envirs[ss.map].AddMapObject(c.x, c.y, CellType.Play, c.id, a);
    }

    private static ActorStub SessionActor(SessionSpec ss, EnvirnomentStub envir, int id, int x, int y)
    {
        var a = new ActorStub
        {
            ActorId = id,
            ChrName = ss.chr,
            CurrX = x,
            CurrY = y,
            ViewRange = (byte)ss.view_range,
            Race = (byte)ss.race,
            Envir = envir,
            HitPoint = (byte)ss.hit_point,
            SpeedPoint = (byte)ss.speed_point,
            Job = (byte)ss.job,
        };
        a.WAbil.Level = (byte)ss.level;
        a.WAbil.HP = (ushort)ss.hp;
        a.WAbil.MaxHP = (ushort)ss.max_hp;
        a.WAbil.DC = ss.dc;
        a.WAbil.AC = ss.ac;
        a.WAbil.Exp = ss.exp;
        a.WAbil.MaxExp = ss.max_exp;
        return a;
    }

    private static void RemoveFromCell(EnvirnomentStub envir, ActorStub a)
    {
        MapCellInfo cell = envir.GetCellInfo(a.CurrX, a.CurrY, out bool ok);
        if (!ok)
        {
            return;
        }
        for (int i = cell.ObjList.Count - 1; i >= 0; i--)
        {
            if (cell.ObjList[i].ActorObject && cell.ObjList[i].CellObjId == a.ActorId)
            {
                cell.Remove(i);
            }
        }
        if (cell.Count == 0)
        {
            cell.Clear();
        }
    }

    private static List<(int map, int x, int y, int obj)> FloorItems(List<EnvirnomentStub> envirs)
    {
        var outList = new List<(int, int, int, int)>();
        for (int m = 0; m < envirs.Count; m++)
        {
            for (int x = 0; x < envirs[m].Width; x++)
            {
                for (int y = 0; y < envirs[m].Height; y++)
                {
                    MapCellInfo cell = envirs[m].GetCellInfo(x, y, out bool ok);
                    if (!ok)
                    {
                        continue;
                    }
                    foreach (CellObject o in cell.ObjList)
                    {
                        if (o.CellType == CellType.Item)
                        {
                            outList.Add((m, x, y, o.CellObjId));
                        }
                    }
                }
            }
        }
        outList.Sort();
        return outList;
    }

    private static void TryStep(List<EnvirnomentStub> envirs, Dictionary<int, int> envirOf,
        Dictionary<int, ActorStub> actors, int id, int dx, int dy)
    {
        if (!actors.TryGetValue(id, out ActorStub a))
        {
            return;
        }
        int tx = a.CurrX + dx;
        int ty = a.CurrY + dy;
        EnvirnomentStub envir = envirs[envirOf[id]];
        if (!envir.CellMatch(tx, ty))
        {
            return;
        }
        if (envir.MoveToMovingObject(a.CurrX, a.CurrY, a, tx, ty, false))
        {
            a.CurrX = tx;
            a.CurrY = ty;
        }
    }

    private static void LeaveWorld(List<EnvirnomentStub> envirs, Dictionary<int, int> envirOf,
        Dictionary<int, ActorStub> actors, List<int> order, int id)
    {
        if (!actors.TryGetValue(id, out ActorStub a))
        {
            return;
        }
        RemoveFromCell(envirs[envirOf[id]], a);
        actors.Remove(id);
        ActorMgr.Map.Remove(id);
        envirOf.Remove(id);
        order.Remove(id);
    }

    private static void SeedRandomNumber(int seed)
    {
        var inst = OpenMir2.RandomNumber.GetInstance();
        var field = typeof(OpenMir2.RandomNumber).GetField("random", BindingFlags.NonPublic | BindingFlags.Static);
        field.SetValue(null, new Random(seed));
        _ = inst;
    }

    private static string ResolveWorldDir()
    {
        var dir = new DirectoryInfo(AppContext.BaseDirectory);
        while (dir != null)
        {
            string candidate = Path.Combine(dir.FullName, "world");
            if (Directory.Exists(Path.Combine(candidate, "scenarios")))
            {
                return candidate;
            }
            dir = dir.Parent;
        }
        throw new DirectoryNotFoundException("找不到 tests/parity/world/scenarios");
    }
}
