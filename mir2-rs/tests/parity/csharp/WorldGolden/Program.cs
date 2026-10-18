// M2 下半程金标准驱动：**会话进世界 + 掉落系统 +（本批新增）战斗结算**。
// 对齐 Rust `crate::world`；场景 `session`（无战斗）与 `combat`（含 `attack`）共用本驱动。
//
// 顺序契约（两侧写死）：
//   1. 每 tick：clock += 200 → 应用本 tick 的**会话操作**（enter/switch/softclose/reenter，立即生效）
//      → 入队本 tick 的**世界命令**（spawn/walk/kill/attack/leave，按场景顺序 FIFO）→ 处理队列
//      → 死亡结算（掉落落地 + `KillFunc` 发经验）→ 按插入顺序 AOI → 输出；
//      AOI 段：玩家（Race==Play）按 oracle `WorldServer.cs:663` 的
//      `(now - SearchTick) > SearchTime`（`SearchTime=1000`，`PlayObject.cs:900`）节流，
//      Ghost 玩家不扫（`:708-711`）；死亡走 `SearchViewRangeDeath`；非玩家保持每 tick 扫描
//      （对应 Rust 侧本片不动的怪/卫士/NPC/商人扫描节拍）。
//   2. 怪物掉落在地图**生成时**预算（`WorldServer.MonGen.cs:440` 的时机）；
//   3. 死亡时走 oracle `DieDropItems` 的掉落结算：`GetDropPosition` 只做格子扫描，
//      **不**把物品加进地图（`Envir.AddItemToMap` 首行 `MapItem.ItemId == 0` 门恒真 ⇒ 掉落恒不落地）。
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
        public bool ghost { get; set; }
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
                CellType = spec.race == 0 ? CellType.Play : CellType.Monster,
                Ghost = spec.ghost,
                Death = spec.death,
                Invisible = spec.invisible,
                FixedHideMode = spec.fixed_hide_mode,
                ObMode = spec.ob_mode,
                NastyMode = spec.nasty_mode,
                WantRefMsg = spec.want_ref_msg,
                Envir = envirs[0],
            };
            if (a.Race == 0)
            {
                // `PlayObject.cs:899-901`：ctor 覆盖 `SearchTime=1000`、`SearchTick=GetTickCount()`。
                // 静态实体在世界建立时构造（`VirtualClock.Now == 0`）。
                a.SearchTime = 1000;
                a.SearchTick = (int)VirtualClock.Now;
            }
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
                    case "ghost":
                        ApplyGhost(actors, c);
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
                            // C# `PlayObject.AttackDir`（`PlayObject.Attack.cs:73-77`）的子集：
                            // 先 `GetBaseAttackPoewr()`（消耗攻击力 RNG），再 `_Attack(nPower, target)`
                            // （消耗命中/护甲 RNG），最后 `AttackSuccess`（玩家攻方有目标 ⇒ 再抽一次
                            // `Random(5)`，`PlayObject.Attack.cs:225`）。技能与 `GetAttackPowerHit` 不在此子集。
                            int nPower = attackerObj.GetBaseAttackPoewr();
                            attackerObj._Attack(nPower, attackVictim);
                            if (attackerObj.Race == 0)
                            {
                                attackerObj.AttackSuccess();
                            }
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
            // 2.5) 命中者过期清理：照 `BaseObject.Base.cs:119-132` 的 Death/Ghost 那半条
            //（oracle 在命中者自己的 Run() 里清 LastHiter/ExpHitter；墙钟 30s/6s 两项不在本夹具）
            // 顺序：命令之后、死亡结算之前 —— 与 Rust 侧 `World::expire_hitters()` 同序。
            foreach (int id in order.ToList())
            {
                if (!actors.TryGetValue(id, out ActorStub a0))
                {
                    continue;
                }
                if (a0.LastHiter != null && (a0.LastHiter.Death || a0.LastHiter.Ghost))
                {
                    a0.LastHiter = null;
                }
                if (a0.ExpHitter != null && (a0.ExpHitter.Death || a0.ExpHitter.Ghost))
                {
                    a0.ExpHitter = null;
                }
            }
            // 3) 死亡结算（掉落**恒不落地** + `KillFunc` 发经验）
            foreach (int id in order.ToList())
            {
                if (!actors.TryGetValue(id, out ActorStub a) || a.WAbil.HP != 0 || a.DeathSettled)
                {
                    continue;
                }
                a.Death = true;
                a.DeathSettled = true;
                int mapId = envirOf[id];
                // oracle 掉落结算（`DieDropItems`，`BaseObject.cs:2450-2464`）：`DropItemDown`
                // （`:522-580`）/`DropGoldDown`（`:730-768`）先 `GetDropPosition`（`:459-524`，纯格子
                // 扫描、不消费 RNG），再 `Envir.AddItemToMap`（`Envirnoment.cs:88-130`）——而后者
                // 的**首行**是 `if (mapItem.ItemId == 0) return false;`，且全仓**无** `MapItem.ItemId`
                // 赋值点（`MapItem.cs:41` 被注释；`rg 'ItemId\s*=' src` 其余 4 处全是 `== 0` 守卫）
                // ⇒ **死亡掉落（物品 + 金币）恒不落地**，成功支的 `SendRefMsg(RM_ITEMSHOW,…)`
                // （`:570`/`:752`）与客户端 `SM_ITEMSHOW(610)` 恒不执行（同 T-19/T-25）。
                // 旧版探针用桩 `AddMapObject` 绕过该门 ⇒ 编码的是「探针意图」而非冻结 `src/` 行为；
                // 现按 oracle 建模：仍走格子扫描（保持调用序/无副作用），但**不加进地图**、`placed` 恒 0。
                int placed = 0;
                if (a.ItemList != null)
                {
                    foreach (ParityGolden.UserItem item in a.ItemList)
                    {
                        // 逐件走扫描（`item` 只用于计数，不落地）。
                        ActorStub.GetDropPosition(envirs[mapId], a.CurrX, a.CurrY, 3, out int _, out int _);
                    }
                }
                if (a.Gold > 0)
                {
                    ActorStub.GetDropPosition(envirs[mapId], a.CurrX, a.CurrY, 3, out _, out _);
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
                else if (a.Race == 0 && a.Ghost)
                {
                    // oracle 的 Ghost 玩家已从 `PlayObjectList` 移除、不再 `Run`
                    // （`WorldServer.cs:708-711`）⇒ 不做 `SearchViewRange`。
                }
                else if (a.Race != 0 || (VirtualClock.Now - a.SearchTick) > a.SearchTime)
                {
                    // `WorldServer.cs:663`：玩家严格大于 `SearchTime` 才扫；
                    // `playObject.SearchTick = HUtil32.GetTickCount();` 先刷新再扫。
                    if (a.Race == 0)
                    {
                        a.SearchTick = (int)VirtualClock.Now;
                    }
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
            CellType = CellType.Monster,
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

    /// <summary>把某个 actor 置为 Ghost（对应 oracle 里 `Ghost = true` 的状态，如隐身/消失）。</summary>
    private static void ApplyGhost(Dictionary<int, ActorStub> actors, CommandSpec c)
    {
        if (actors.TryGetValue(c.id, out ActorStub a))
        {
            a.Ghost = true;
        }
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
            CellType = ss.race == 0 ? CellType.Play : CellType.Monster,
            Envir = envir,
            HitPoint = (byte)ss.hit_point,
            SpeedPoint = (byte)ss.speed_point,
            Job = (byte)ss.job,
            Ghost = ss.ghost,
        };
        a.WAbil.Level = (byte)ss.level;
        a.WAbil.HP = (ushort)ss.hp;
        a.WAbil.MaxHP = (ushort)ss.max_hp;
        a.WAbil.DC = ss.dc;
        a.WAbil.AC = ss.ac;
        a.WAbil.Exp = ss.exp;
        a.WAbil.MaxExp = ss.max_exp;
        if (a.Race == 0)
        {
            // `PlayObject.cs:899-901`：ctor 覆盖 `SearchTime=1000`、`SearchTick=GetTickCount()`。
            // Rust 侧 `World::enter_map` 在 tick 推进**之前**调用（`common/mod.rs` 的顺序契约：
            // 会话操作先生效、`world.tick()` 才 `clock += 200`）⇒ 用推进前的时钟对齐。
            a.SearchTime = 1000;
            a.SearchTick = (int)VirtualClock.Now - 200;
        }
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
