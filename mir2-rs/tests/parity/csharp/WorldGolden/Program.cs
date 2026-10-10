// M2 世界/AOI 金标准驱动：读场景 JSON，按与 Rust 侧 crate::world **同一顺序**驱动，
// 输出规范化文本（每 tick 每实体的 位置 + 视野列表）。
//
// 顺序契约（两侧必须一致，写死在各自实现里）：
//   1. 实体按场景数组顺序插入（= 槽位顺序 = AOI 处理顺序）；
//   2. 每个实体插入时入队一条 Enter；
//   3. 每 tick：clock += 200 → 追加本 tick 的命令（按场景顺序）→ FIFO 处理队列 → 按插入顺序对每个实体做 AOI；
//   4. Walk：界内检查 → 目标格阻挡者（!Ghost && !Death && !FixedHideMode && !ObMode）→ MoveToMovingObject → 更新坐标；
//   5. Leave：从格子摘除 + 从 ActorMgr 移除（Rust 侧同时从仓库移除）。
//
// 用法：WorldGolden <scenario.json> <out.txt>

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.Json;
using WorldGolden;

internal static class Program
{
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

    private sealed class CommandSpec
    {
        public int tick { get; set; }
        public string kind { get; set; }
        public int id { get; set; }
        public int dx { get; set; }
        public int dy { get; set; }
    }

    private sealed class Scenario
    {
        public int width { get; set; }
        public int height { get; set; }
        public int ticks { get; set; }
        public List<EntitySpec> entities { get; set; } = new List<EntitySpec>();
        public List<CommandSpec> commands { get; set; } = new List<CommandSpec>();
    }

    private sealed class QueuedCommand
    {
        public string kind;
        public int id;
        public int dx;
        public int dy;
    }

    private static int Main(string[] args)
    {
        if (args.Length < 1)
        {
            Console.Error.WriteLine("usage: WorldGolden <scenario-name> [out-path]");
            return 2;
        }
        // 路径自解析：从程序集位置向上找 tests/parity/world（免去 cwd 依赖）
        string worldDir = ResolveWorldDir();
        string scenarioPath = Path.Combine(worldDir, "scenarios", args[0].EndsWith(".json") ? args[0] : args[0] + ".json");
        string outPath = args.Length >= 2 ? args[1] : Path.Combine(worldDir, "golden", Path.GetFileNameWithoutExtension(scenarioPath) + ".txt");
        var scenario = JsonSerializer.Deserialize<Scenario>(File.ReadAllText(scenarioPath));
        var envir = new EnvirnomentStub(scenario.width, scenario.height);
        var order = new List<int>();          // 插入顺序（= AOI 处理顺序）
        var actors = new Dictionary<int, ActorStub>();
        var queue = new Queue<QueuedCommand>();

        VirtualClock.Now = 0;
        ActorMgr.Clear();

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
                Envir = envir
            };
            actors[spec.id] = a;
            ActorMgr.Add(spec.id, a);
            order.Add(spec.id);
            queue.Enqueue(new QueuedCommand { kind = "enter", id = spec.id });
        }
        // Master 必须在全部实体建好后再接（场景里 master 可以指向后面的实体）
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
            foreach (CommandSpec c in scenario.commands.Where(c => c.tick == tick))
            {
                queue.Enqueue(new QueuedCommand { kind = c.kind, id = c.id, dx = c.dx, dy = c.dy });
            }
            while (queue.Count > 0)
            {
                QueuedCommand cmd = queue.Dequeue();
                switch (cmd.kind)
                {
                    case "enter":
                        if (actors.TryGetValue(cmd.id, out ActorStub enterActor))
                        {
                            envir.AddMapObject(enterActor.CurrX, enterActor.CurrY, CellType.Play, cmd.id, enterActor);
                        }
                        break;
                    case "leave":
                        LeaveWorld(envir, actors, order, cmd.id);
                        break;
                    case "walk":
                        TryStep(envir, actors, cmd.id, cmd.dx, cmd.dy);
                        break;
                }
            }
            // AOI：按插入顺序（仍存在的实体）
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

            sb.Append("tick ").Append(tick).Append(" clock ").Append(VirtualClock.Now).Append('\n');
            foreach (int id in order.Where(actors.ContainsKey).OrderBy(i => i))
            {
                ActorStub a = actors[id];
                sb.Append("ent ").Append(a.ActorId).Append(' ').Append(a.CurrX).Append(' ').Append(a.CurrY).Append(" vis");
                foreach (VisibleBaseObject v in a.VisibleActors)
                {
                    sb.Append(' ').Append(v.BaseObject.ActorId).Append(':').Append((int)v.VisibleFlag);
                }
                sb.Append('\n');
            }
        }
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outPath)));
        File.WriteAllText(outPath, sb.ToString());
        Console.WriteLine($"world golden: {scenario.entities.Count} 实体 × {scenario.ticks} tick -> {outPath}");
        return 0;
    }

    /// <summary>从程序集位置向上找 `tests/parity/world`（对 cwd 不敏感）。</summary>
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
        throw new DirectoryNotFoundException("找不到 tests/parity/world/scenarios（程序集位置: " + AppContext.BaseDirectory + "）");
    }

    /// <summary>对应 Rust `World::try_step`（同样顺序：界内 → 阻挡者 → MoveToMovingObject → 更新坐标）。</summary>
    private static void TryStep(EnvirnomentStub envir, Dictionary<int, ActorStub> actors, int id, int dx, int dy)
    {
        if (!actors.TryGetValue(id, out ActorStub a))
        {
            return;
        }
        int tx = a.CurrX + dx;
        int ty = a.CurrY + dy;
        if (!envir.CellMatch(tx, ty))
        {
            return;
        }
        bool moved = envir.MoveToMovingObject(a.CurrX, a.CurrY, a, tx, ty, false);
        if (moved)
        {
            a.CurrX = tx;
            a.CurrY = ty;
        }
    }

    /// <summary>对应 Rust `EntityStore::leave_world`（从格子摘除 + 从仓库/ActorMgr 移除）。</summary>
    private static void LeaveWorld(EnvirnomentStub envir, Dictionary<int, ActorStub> actors, List<int> order, int id)
    {
        if (!actors.TryGetValue(id, out ActorStub a))
        {
            return;
        }
        MapCellInfo cell = envir.GetCellInfo(a.CurrX, a.CurrY, out bool success);
        if (success)
        {
            for (int i = cell.ObjList.Count - 1; i >= 0; i--)
            {
                if (cell.ObjList[i].ActorObject && cell.ObjList[i].CellObjId == id)
                {
                    cell.Remove(i);
                }
            }
            if (cell.Count == 0)
            {
                cell.Clear();
            }
        }
        actors.Remove(id);
        ActorMgr.Map.Remove(id);
        order.Remove(id);
    }
}
