// 卡 openmir2-warrskill-entry：`ClientSpellXY` 入口的 C# golden 生成器（oracle 侧，只取证不改产品代码）。
//
// 为什么需要（卡面「背景」）：`MagicManager.DoSpell` 第一句就是战士门
// （`MagicManager.cs` 的 `IsWarrSkill`，命中 3/4/7/12/25/26/27/34/38），直调 `DoSpell` 对这九条
// 只能得到「零帧」——**那不是 oracle 的线上行为**。真实帧面由调用方
// `PlayObject.ClientSpellXY`（`src/M2Server/Player/PlayObject.Attack.cs`）产生：
//   * 3/4/7 走 `default` 支：`DoSpell` 返回 false ⇒ `SendRefMsg(RM_MAGICFIREFAIL)` ⇒
//     `SM_MAGICFIRE_FAIL(639)`（`PlayObject.Message.cs` 的 `RM_MAGICFIREFAIL` 支，无自守卫）；
//   * 12/25/26/27/34/38 各有 `case`（`SKILL_ERGUM`/`SKILL_BANWOL`/`SKILL_FIRESWORD`/
//     `SKILL_MOOTEBO`/`SKILL_CROSSMOON`/`SKILL_TWINBLADE`），**根本不调 `DoSpell`**。
//
// 本文件只做「构造与 `ClientSpellXY` 一致的调用上下文 → 跑 → 记账 → 落 golden」；
// `MagicGolden/Program.cs` 只加一条分派行（接口锁：不与 `card/magic-parity` 的 `fireboom` 分派行冲突）。
//
// 用法（仓库根）：
//   dotnet run --project mir2-rs/tests/parity/csharp/MagicGolden -c Release -- \
//       --case warrskill --out-dir mir2-rs/tests/parity/golden/magic
// 产物：`<out-dir>/warr-<magicId>.json`（9 份：3/4/7/12/25/26/27/34/38）。
// 环境变量：`MAGICGOLDEN_CONF_DIR`（默认 `E:\MirServer\M2GameSvr`）、`MAGICGOLDEN_MAP_DIR`（默认其 `Map`）。
//
// 记账口径（与既有用例一致）：`M2Share.NetChannel` 换成只记录实现（三处 `SendSocket` 重载都汇到
// `AddGateBuffer`）；排水「抽空为止」（上界 8，命中进产物）；等 1200ms 跨过自延迟。
// **不在 golden 内**：调用方在 `ClientSpellXY` 成功后发的 `+GD/<uptime>` 短包（墙钟，不可复现）；
// 以及 RM 的 `delivery`/`time` 类墙钟字段（`NotDue` 只记未到期条目的 ident/delivery）。
//
// 判据的边界（卡面第 2 条）：本入口**只**复现 `ClientSpellXY` 这一层；`CM_SPELL` 在
// `PlayObject.Message.cs` 的调用方尾部（`ActionTick`/`+GD`/`RM_MOVEFAIL`）不属本卡。

using System.Diagnostics;
using System.Reflection;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using OpenMir2;
using OpenMir2.Packets.ServerPackets;

namespace MagicGolden;

public static class ClientSpellXYEntry
{
    /// <summary>`PlayObject.ClientSpellXY`（private）：`(int wIdent, int nKey, short nTargetX, short nTargetY, IActor targetObject, bool boLateDelivery, ref int delayTime)`。</summary>
    private static readonly MethodInfo ClientSpellXYMethod = typeof(M2Server.Player.PlayObject)
        .GetMethod("ClientSpellXY", BindingFlags.NonPublic | BindingFlags.Instance)
        ?? throw new InvalidOperationException("PlayObject.ClientSpellXY 不存在（方法名/签名变了？先核 PlayObject.Attack.cs）");

    private static readonly string[] ConfNames = { "Server.conf", "String.conf", "Exps.conf", "Global.conf", "setting.conf" };

    private sealed class MagicSpec
    {
        public int MagicId;
        public string MagicName;
        public string Arm; // "default" 或 "case <MagicConst 名>"
        public int Effect = 1;
        public int EffectType = 1;
        public int UserMagicLevel = 1;
        public int CasterLevel = 30;
        public int CasterMc = 15;
        public int CasterSc = 15;
        public int TargetLevel = 20;
        public int TargetMaxHp = 100;
        public int AntiMagic = 3;
        /// <summary>`MagicInfo.Spell`：`GetSpellPoint = Round(Spell/4*(Level+1)) + DefSpell` ⇒ 20/4*2 = 10。</summary>
        public int MagicSpell = 20;
        public int MagicDefSpell = 0;
        public SubCase[] Cases;
    }

    private sealed class SubCase
    {
        public string Label;
        public int Seed = 1;
        public int CasterMp = 100;
        /// <summary>传给 `ClientSpellXY` 的目标坐标。**27 的 case 把这个 X 当方向用**（`Dir=(byte)nTargetX`）。</summary>
        public int TargetX = 101;
        public int TargetY = 100;
        public bool PresetUseThrusting;
        public bool PresetUseHalfMoon;
        public bool PresetCrsHitkill;
        /// <summary>43：预设 `MBo43Kill=true` ⇒ 命中 on→off 支（`"+UCID"`）。</summary>
        public bool PresetMbo43Kill;
        /// <summary>26：预设 `LatestFireHitTick=now` ⇒ 命中 10s 门内的失败支（红字 + 不扣 MP）。</summary>
        public bool PresetFireHitBusy;
        /// <summary>
        /// **牙专用**（`--case warr-selftest`，裁决 C）：在「登录帧已丢弃、`ClientSpellXY` 尚未调用」
        /// 之间往施法者队列注入一条 `RM_CHARSTATUSCHANGED(10139)`（→ `SM_CHARSTATUSCHANGED(657)`）。
        /// 若有人把登录帧 `DrainRaw` 挪到调用之后，这条合成消息会被一并丢掉 ⇒ self-test 红。
        /// </summary>
        public bool InjectSpellWindowStatusRm;
    }

    /// <summary>10 条魔号的 case 分派（与 `ClientSpellXY` 的 switch 一一对应；56 为 oracle 越界崩溃路径，不入对拍）。</summary>
    private static readonly MagicSpec[] Specs =
    {
        // 3/4/7：default 支（DoSpell 被战士门挡下 ⇒ RM_MAGICFIREFAIL ⇒ 639）。
        new MagicSpec { MagicId = 3, MagicName = "SwordLong", Arm = "default",
            Cases = new[] { new SubCase { Label = "invoke" } } },
        new MagicSpec { MagicId = 4, MagicName = "SwordWide", Arm = "default",
            Cases = new[] { new SubCase { Label = "invoke" } } },
        new MagicSpec { MagicId = 7, MagicName = "SwordLong2", Arm = "default",
            Cases = new[] { new SubCase { Label = "invoke" } } },
        // 12 SKILL_ERGUM：ThrustingOnOff 开/关 + "+LNG"/"+ULNG"。
        new MagicSpec { MagicId = 12, MagicName = "Thrusting", Arm = "case SKILL_ERGUM",
            Cases = new[] { new SubCase { Label = "off2on" }, new SubCase { Label = "on2off", PresetUseThrusting = true } } },
        // 25 SKILL_BANWOL：HalfMoonOnOff 开/关 + "+WID"/"+UWID"。
        new MagicSpec { MagicId = 25, MagicName = "HalfMoon", Arm = "case SKILL_BANWOL",
            Cases = new[] { new SubCase { Label = "off2on" }, new SubCase { Label = "on2off", PresetUseHalfMoon = true } } },
        // 26 SKILL_FIRESWORD：AllowFireHitSkill（10s 门）+ GetSpellPoint/DamageSpell/HealthSpellChanged + "+FIR"。
        new MagicSpec { MagicId = 26, MagicName = "FireSword", Arm = "case SKILL_FIRESWORD",
            Cases = new[] { new SubCase { Label = "charge" }, new SubCase { Label = "busy", PresetFireHitBusy = true }, new SubCase { Label = "nomp", CasterMp = 5 } } },
        // 27 SKILL_MOOTEBO：Dir=(byte)nTargetX（oracle 怪行为，真实客户端传的是坐标）；
        //   3s 门 + GetSpellPoint/DamageSpell/HealthSpellChanged + DoMotaebo。
        new MagicSpec { MagicId = 27, MagicName = "MoteBo", Arm = "case SKILL_MOOTEBO",
            Cases = new[] { new SubCase { Label = "coordinate_dir" } } },
        // 34 SKILL_CROSSMOON：SkillCrsOnOff 开/关 + "+CRS"/"+UCRS"。
        new MagicSpec { MagicId = 34, MagicName = "CrossMoon", Arm = "case SKILL_CROSSMOON",
            Cases = new[] { new SubCase { Label = "off2on" }, new SubCase { Label = "on2off", PresetCrsHitkill = true } } },
        // 38 SKILL_TWINBLADE：AllowTwinHitSkill（恒真）+ GetSpellPoint/DamageSpell/HealthSpellChanged + "+TWN"。
        new MagicSpec { MagicId = 38, MagicName = "TwinBlade", Arm = "case SKILL_TWINBLADE",
            Cases = new[] { new SubCase { Label = "go" }, new SubCase { Label = "nomp", CasterMp = 5 } } },
        // 43 SKILL_43：Skill43OnOff 开/关 + "+CID"/"+UCID"。
        new MagicSpec { MagicId = 43, MagicName = "Skill43", Arm = "case SKILL_43",
            Cases = new[] { new SubCase { Label = "off2on" }, new SubCase { Label = "on2off", PresetMbo43Kill = true } } },
    };

    /// <summary>分派入口（`Program.cs` 的 `--case warrskill` 一行调用这里）。读 `--out-dir`。</summary>
    public static int Run()
    {
        var argv = Environment.GetCommandLineArgs();
        string outDir = null;
        int idx = Array.IndexOf(argv, "--out-dir");
        if (idx > 0 && idx + 1 < argv.Length) outDir = argv[idx + 1];
        if (string.IsNullOrWhiteSpace(outDir))
        {
            Console.Error.WriteLine("WARR_USAGE 需要 --out-dir <dir>（例：mir2-rs/tests/parity/golden/magic）");
            return 2;
        }
        outDir = Path.GetFullPath(outDir);
        Directory.CreateDirectory(outDir);

        if (!EnsureConfFiles(out var confHashes, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        string mapDir = Environment.GetEnvironmentVariable("MAGICGOLDEN_MAP_DIR") ?? @"E:\MirServer\M2GameSvr\Map";
        string mapPath = Path.Combine(mapDir, "0.map");
        if (!File.Exists(mapPath))
        {
            Console.Error.WriteLine("MAP_MISSING " + mapPath);
            return 2;
        }
        var config = new
        {
            conf_dir = confHashes.Dir,
            files = confHashes.Files,
            map = new { path = mapPath, sha256 = Sha256Hex(File.ReadAllBytes(mapPath)) },
            repo_head = TryRepoHead(),
        };

        foreach (var spec in Specs)
        {
            var cases = new List<FireballResult>();
            int idBase = 1000 * spec.MagicId;
            for (int i = 0; i < spec.Cases.Length; i++)
            {
                cases.Add(RunOne(spec, spec.Cases[i], idBase + 100 * (i + 1), mapPath));
            }
            var doc = new
            {
                magic_id = spec.MagicId,
                anti_magic = spec.AntiMagic,
                entry = new
                {
                    name = "ClientSpellXY",
                    arm = spec.Arm,
                    source = "src/M2Server/Player/PlayObject.Attack.cs::ClientSpellXY",
                },
                config,
                fixture_spec = new
                {
                    magic_name = spec.MagicName,
                    effect = spec.Effect,
                    effect_type = spec.EffectType,
                    user_magic_level = spec.UserMagicLevel,
                    caster_level = spec.CasterLevel,
                    caster_mc = spec.CasterMc,
                    caster_sc = spec.CasterSc,
                    target_level = spec.TargetLevel,
                    target_max_hp = spec.TargetMaxHp,
                    magic_spell = spec.MagicSpell,
                    magic_def_spell = spec.MagicDefSpell,
                    spell_point = SpellPoint(spec),
                    // 43：`Skill43OnOff` 的文案是**源码默认**（`StringConf` 不读 `RedHalfMoon*`、
                    // 部署无该键）⇒ 记进 Fixture，供比较器断言"实际 100 尾 == Fixture == 字面量"，
                    // 防默认串被改而 golden 静默漂移（同类：施毒批的 `you_poisoned`）。
                    toggle_text_on = spec.MagicId == 43 ? SystemModule.MessageSettings.RedHalfMoonOn : (string)null,
                    toggle_text_off = spec.MagicId == 43 ? SystemModule.MessageSettings.RedHalfMoonOff : (string)null,
                },
                cases,
            };
            string path = Path.Combine(outDir, "warr-" + spec.MagicId + ".json");
            // 落盘一律 LF（Windows 上缩进 JSON 可能带 CRLF；仓库金标准用 LF，避免 diff --check 逐行红）。
            string json = JsonSerializer.Serialize(doc, new JsonSerializerOptions { WriteIndented = true })
                .Replace("\r\n", "\n");
            File.WriteAllText(path, json, new UTF8Encoding(false));
            Console.Error.WriteLine("GOLDEN_WRITTEN " + path);
            Console.Out.WriteLine("WARR_" + spec.MagicId + " cases=" + cases.Count + " "
                + string.Join(" | ", cases.Select(c => c.Label
                    + "[ret=" + c.SpellReturn + "; rng=" + c.Rng.Count + "; passes=" + c.DrainPasses + "; cap=" + c.DrainPassesHitCap
                    + "; " + string.Join(" ", c.Receivers.Select(r => r.Who + ":rm" + r.Rm.Count + "/sm" + r.Sent.Count)) + "]")));
        }
        return 0;
    }

    /// <summary>
    /// 裁决 C 的牙（`--case warr-selftest`）：
    /// (a) 登录序确实会在施法者队列里留下 `RM_CHARSTATUSCHANGED(10139)`（证明"丢弃"不是空动作）；
    /// (b) 施法窗口内的同一条 RM **必须**变成 `SM_CHARSTATUSCHANGED(657)` 进 golden——
    ///     走的是 RunOne 同一条排水路径，注入点在「丢弃之后、调用之前」。
    /// 红检：把 `caster.DrainRaw()` 挪到调用之后 ⇒ (b) 失败。
    /// </summary>
    public static int SelfTest()
    {
        if (!EnsureConfFiles(out _, out var missing))
        {
            Console.Error.WriteLine("CONF_MISSING " + string.Join(",", missing));
            return 2;
        }
        string mapDir = Environment.GetEnvironmentVariable("MAGICGOLDEN_MAP_DIR") ?? @"E:\MirServer\M2GameSvr\Map";
        string mapPath = Path.Combine(mapDir, "0.map");
        if (!File.Exists(mapPath))
        {
            Console.Error.WriteLine("MAP_MISSING " + mapPath);
            return 2;
        }

        // (a) 登录帧存在性
        var c = new RecordingPlayObject { ChrName = "selftest-caster", ActorId = 1, CurrX = 10, CurrY = 10, SoftVersionDateEx = 20240101, ClientTick = 5 };
        c.Abil.Level = 30;
        c.RecalcLevelAbilitys();
        c.RecalcAbilitys();
        var loginFrames = c.DrainRaw();
        bool hasLogin10139 = loginFrames.Any(m => m.Msg.wIdent == (int)OpenMir2.Messages.RM_CHARSTATUSCHANGED);

        // (b) 施法窗口内的 657 必须进 golden
        var spec = new MagicSpec
        {
            MagicId = 12,
            MagicName = "Thrusting",
            Arm = "case SKILL_ERGUM",
            Cases = new[] { new SubCase { Label = "spell_window_657", InjectSpellWindowStatusRm = true } },
        };
        var res = RunOne(spec, spec.Cases[0], 9001, mapPath);
        bool hasSpell657 = res.Receivers.Any(r => HasIdent(r.Sent, 657));

        // (c) 登录期的 657 **不得**进 golden：同一套 RunOne（不注入）跑一遍，断言 657 不存在。
        //     红检：删掉 `caster.DrainRaw()` ⇒ 登录帧留下 ⇒ (c) 失败。
        var cleanSpec = new MagicSpec
        {
            MagicId = 12,
            MagicName = "Thrusting",
            Arm = "case SKILL_ERGUM",
            Cases = new[] { new SubCase { Label = "off2on" } },
        };
        var clean = RunOne(cleanSpec, cleanSpec.Cases[0], 9101, mapPath);
        bool noLogin657 = !clean.Receivers.Any(r => HasIdent(r.Sent, 657));

        bool ok = hasLogin10139 && hasSpell657 && noLogin657;
        Console.Error.WriteLine(("WARR_SELFTEST " + (ok ? "OK" : "FAIL")
            + " login_10139=" + hasLogin10139 + " spell_window_657=" + hasSpell657 + " no_login_657=" + noLogin657));
        return ok ? 0 : 1;
    }

    private static bool HasIdent(IEnumerable<object> frames, int ident)
    {
        foreach (var f in frames)
        {
            var prop = f.GetType().GetProperty("ident");
            if (prop != null && Convert.ToInt32(prop.GetValue(f), System.Globalization.CultureInfo.InvariantCulture) == ident)
            {
                return true;
            }
        }
        return false;
    }

    /// <summary>GetSpellPoint 的纯函数复算（只用于 golden 自述，判据仍以 oracle 运行为准）。</summary>
    private static int SpellPoint(MagicSpec s)
    {
        return (int)Math.Round(s.MagicSpell / 4.0 * (s.UserMagicLevel + 1)) + s.MagicDefSpell;
    }

    private static FireballResult RunOne(MagicSpec spec, SubCase sc, int idBase, string mapPath)
    {
        var env = new M2Server.Maps.Envirnoment();
        if (!env.LoadMapData(mapPath))
        {
            throw new InvalidOperationException("MAP_LOAD_FAILED " + mapPath);
        }
        var caster = new RecordingPlayObject
        {
            ChrName = "golden-caster",
            ActorId = idBase + 1,
            CurrX = 100,
            CurrY = 100,
            SoftVersionDateEx = 20240101,
            ClientTick = 5,
        };
        caster.Abil.Level = (byte)spec.CasterLevel;
        caster.WAbil.MC = (ushort)spec.CasterMc;
        caster.WAbil.SC = (ushort)spec.CasterSc;
        caster.WAbil.MP = (ushort)sc.CasterMp;
        // `ClientSpellXY` 的第一道门：`IsCanSpell`（`PlayObject.cs` 的自动属性，ctor 缺省 false）。
        caster.IsCanSpell = true;
        // 裁决 A（opc-07 2026-11-20）：caster 与 target **同标准**——走真人登录序
        // （`PlayObject.UserLogon` 的 `RecalcLevelAbilitys(); RecalcAbilitys();`），
        // 并把 HP 置为公式 `MaxHP`（否则 SM_HEALTHSPELLCHANGED(53) 会带装置伪影 MaxHP=0）。
        // 注意 `RecalcAbilitys` 保留重算前的旧 MP（`wOldMP`），所以上面的 CasterMp 仍然生效。
        caster.RecalcLevelAbilitys();
        caster.RecalcAbilitys();
        caster.WAbil.HP = caster.WAbil.MaxHP;
        // 登录帧不属于施法窗口：`RecalcAbilitys` 的 `StatusChanged()` 会在施法者队列里留
        // `RM_CHARSTATUSCHANGED(10139)`（→ `SM_CHARSTATUSCHANGED(657)`），与 target 的
        // `DrainRaw()` 同口径——**丢弃原始消息、不 Operate**，否则登录伪影会混入 golden。
        caster.DrainRaw();

        var um = new OpenMir2.Packets.ClientPackets.UserMagic
        {
            MagIdx = (ushort)spec.MagicId,
            Level = (byte)spec.UserMagicLevel,
            TranPoint = 0,
            Magic = new OpenMir2.Data.MagicInfo
            {
                MagicId = (ushort)spec.MagicId,
                MagicName = spec.MagicName,
                Effect = (byte)spec.Effect,
                EffectType = (byte)spec.EffectType,
                Spell = (ushort)spec.MagicSpell,
                DefSpell = (byte)spec.MagicDefSpell,
                TrainLv = 1,
                TrainLevel = new byte[3],
                MaxTrain = new int[3],
            },
        };
        // `GetMagicInfo(nKey)` 按 `Magic.MagicId` 扫 `MagicList`；case 臂另读 `MagicArr[MagIdx]`。
        caster.MagicList.Add(um);
        caster.MagicArr[spec.MagicId] = um;

        if (sc.PresetUseThrusting) caster.UseThrusting = true;
        if (sc.PresetUseHalfMoon) caster.UseHalfMoon = true;
        if (sc.PresetCrsHitkill) caster.CrsHitkill = true;
        if (sc.PresetMbo43Kill) caster.MBo43Kill = true;
        if (sc.PresetFireHitBusy) caster.LatestFireHitTick = HUtil32.GetTickCount();

        var target = new RecordingPlayObject
        {
            ChrName = "golden-target",
            ActorId = idBase + 2,
            CurrX = 101,
            CurrY = 100,
            SoftVersionDateEx = 20240101,
            ClientTick = 5,
            AntiMagic = (ushort)spec.AntiMagic,
        };
        target.Abil.Level = (byte)spec.TargetLevel;
        target.RecalcLevelAbilitys();
        target.WAbil.HP = (ushort)spec.TargetMaxHp;
        target.RecalcAbilitys();
        target.DrainRaw();
        var obsIn = new RecordingPlayObject { ChrName = "golden-obs-in", ActorId = idBase + 3, CurrX = 105, CurrY = 100, SoftVersionDateEx = 20240101, ClientTick = 5 };
        var obsOut = new RecordingPlayObject { ChrName = "golden-obs-out", ActorId = idBase + 4, CurrX = 140, CurrY = 100, SoftVersionDateEx = 20240101, ClientTick = 5 };

        var recorder = new RecordingNetChannel();
        M2Server.M2Share.NetChannel = recorder;
        foreach (var p in new[] { caster, target, obsIn, obsOut })
        {
            p.GateIdx = p.ActorId;
            p.FixedHideMode = false;
            p.ObMode = false;
            p.Envir = env;
            SystemModule.SystemShare.ActorMgr.Add(p);
            env.AddMapObject(p.CurrX, p.CurrY, SystemModule.Data.CellType.Play, p.ActorId, p);
        }

        if (sc.InjectSpellWindowStatusRm)
        {
            // 牙（裁决 C）：施法窗口内的 657 必须能进 golden——注入点刻意在「登录帧丢弃之后、
            // `ClientSpellXY` 调用之前」；把丢弃挪后即吞掉它（self-test 会红）。
            caster.SendMsg(caster, (int)OpenMir2.Messages.RM_CHARSTATUSCHANGED, 0, 0, 0, 0, "");
        }
        int mpBefore = caster.WAbil.MP;
        int hpBefore = target.WAbil.HP;
        var original = RngSeam.Capture();
        var rng = RngSeam.Install(sc.Seed);
        // **真正的调用面**：与 `PlayObject.Message.cs` 的 `case Messages.CM_SPELL` 同一组实参
        // （`wIdent=CM_SPELL, nKey=MagicId, 坐标, 目标= ActorMgr.Get(nParam3), boLateDelivery`）。
        // 只差调用方尾部（`ActionTick` / `+GD` / `RM_MOVEFAIL`），见文件头「判据的边界」。
        var args = new object[]
        {
            (int)OpenMir2.Messages.CM_SPELL, spec.MagicId, (short)sc.TargetX, (short)sc.TargetY,
            (object)target, false, 0,
        };
        bool entryReturn = (bool)ClientSpellXYMethod.Invoke(caster, args);
        int delayTime = (int)args[6];
        byte dir = caster.Dir;
        // **`rng_mode=cover_drain`**（统一契约，opc-07 2026-11-20）：RNG seam 保持到排水结束，
        // `draws` 在排水后取——排水阶段的真实抽取同样进账本（oracle 行为）。
        // 本 9 条在排水阶段无抽取，故数值与旧口径相同；语义按契约写。

        // 排水：抽空为止（上界 8，命中进产物）；1200ms 跨过 `RM_MAGIC_LVEXP` 的 1000ms 自延迟。
        const int MaxDrainPasses = 8;
        const int WaitMs = 1200;
        var pending = new List<object>();
        var byWho = new (string, RecordingPlayObject)[] { ("caster", caster), ("target", target), ("obs_in", obsIn), ("obs_out", obsOut) };
        var rmAcc = new Dictionary<string, List<object>>();
        foreach (var (who, _) in byWho) rmAcc[who] = new List<object>();
        int drainPasses = 0;
        bool drainPassesHitCap = false;
        Thread.Sleep(WaitMs);
        for (int pass = 0; pass < MaxDrainPasses; pass++)
        {
            bool drainedAny = false;
            foreach (var (who, obj) in byWho)
            {
                var raw = obj.DrainRaw();
                if (raw.Count > 0) drainedAny = true;
                var rm = raw.Select(m => (object)new
                {
                    ident = m.Msg.wIdent,
                    wParam = m.Msg.wParam,
                    p1 = m.Msg.nParam1,
                    p2 = m.Msg.nParam2,
                    p3 = m.Msg.nParam3,
                    msg = m.Msg.Buff ?? "",
                    late = m.Msg.LateDelivery,
                    delivery = m.Msg.DeliveryTime,
                    priority = m.Prio,
                }).ToList();
                int now = HUtil32.GetTickCount();
                foreach (var m in raw)
                {
                    if (!m.Msg.LateDelivery || m.Msg.DeliveryTime <= now) obj.OperatePublic(m.Msg);
                    else pending.Add(new { who, ident = m.Msg.wIdent, delivery = m.Msg.DeliveryTime, now });
                }
                rmAcc[who].AddRange(rm);
            }
            drainPasses = pass + 1;
            if (!drainedAny) break;
            if (pass == MaxDrainPasses - 1) drainPassesHitCap = true;
        }
        Console.Error.WriteLine($"DRAIN_PASSES warr/{spec.MagicId}/{sc.Label} passes={drainPasses} hit_cap={drainPassesHitCap}");
        var draws = rng.Draws.ToList();
        RngSeam.Restore(original);

        var receivers = new List<ReceiverResult>();
        foreach (var (who, obj) in byWho)
        {
            receivers.Add(new ReceiverResult { Who = who, Rm = rmAcc[who], Sent = ParseFrames(recorder.Buffers, obj.ActorId) });
        }
        return new FireballResult
        {
            Fixture = new
            {
                label = sc.Label,
                magic_id = spec.MagicId,
                magic_name = spec.MagicName,
                arm = spec.Arm,
                // 统一字段契约（opc-07 2026-11-20）：比较器读 rng_mode，缺键即红；
                // 本入口一律 cover_drain + 登录序角色 + reachable。
                rng_mode = "cover_drain",
                login_order = true,
                reachability = "reachable",
                user_magic_level = spec.UserMagicLevel,
                caster_level = spec.CasterLevel,
                caster_mc = spec.CasterMc,
                caster_sc = spec.CasterSc,
                caster_mp = sc.CasterMp,
                magic_spell = spec.MagicSpell,
                magic_def_spell = spec.MagicDefSpell,
                target_level = spec.TargetLevel,
                target_max_hp = spec.TargetMaxHp,
                anti_magic = spec.AntiMagic,
                seed = sc.Seed,
                entry_args = new
                {
                    w_ident = OpenMir2.Messages.CM_SPELL,
                    n_key = spec.MagicId,
                    n_target_x = sc.TargetX,
                    n_target_y = sc.TargetY,
                    bo_late_delivery = false,
                },
                preset = new
                {
                    use_thrusting = sc.PresetUseThrusting,
                    use_half_moon = sc.PresetUseHalfMoon,
                    crs_hitkill = sc.PresetCrsHitkill,
                    fire_hit_busy = sc.PresetFireHitBusy,
                    mbo43_kill = sc.PresetMbo43Kill,
                },
                entry_return = entryReturn,
                delay_time = delayTime,
                dir = (int)dir,
                wait_ms = WaitMs,
                caster_xy = new[] { 100, 100 },
                target_xy = new[] { 101, 100 },
                obs_in_xy = new[] { 105, 100 },
                obs_out_xy = new[] { 140, 100 },
            },
            Label = sc.Label,
            Seed = sc.Seed,
            DrainPasses = drainPasses,
            DrainPassesHitCap = drainPassesHitCap,
            SpellReturn = entryReturn,
            Receivers = receivers,
            Rng = draws,
            TargetHp = target.WAbil.HP,
            TargetHpBefore = hpBefore,
            NotDue = pending,
            CasterMp = caster.WAbil.MP,
        };
    }

    /// <summary>
    /// 与 `Program.cs::ParseFrames` 同一解析口径（该函数为 private，接口锁下不能改它；
    /// 复制一份并在卡面登记「同一实现的第二份」）。
    /// 缓冲 = 20B `ServerMessage` 头 + 载荷：`PackLength >= 0` ⇒ 12B `CommandMessage` + tail；
    /// `PackLength < 0` ⇒ `SendSocket(string)` 动作短包（记 `ident=0`）。
    /// </summary>
    private static List<object> ParseFrames(List<(int GateIdx, byte[] Data, long Due)> buffers, int gateIdx)
    {
        var list = new List<object>();
        // `Due`（相对到期毫秒）本域不使用（warrskill golden 无 `due_ms_rel` 键，保持输出不变）。
        foreach (var (g, data, _) in buffers)
        {
            if (g != gateIdx) continue;
            int packLength = BitConverter.ToInt32(data, 16);
            if (packLength < 0)
            {
                string action = Encoding.GetEncoding("GB2312").GetString(data, ServerMessage.PacketSize, data.Length - ServerMessage.PacketSize);
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

    /// <summary>把冻结部署的 5 个配置文件拷进输出目录（缺哪份拷哪份），并返回逐文件 sha256。</summary>
    private static bool EnsureConfFiles(out (string Dir, Dictionary<string, string> Files) hashes, out List<string> missing)
    {
        string baseDir = AppContext.BaseDirectory;
        string confDir = Environment.GetEnvironmentVariable("MAGICGOLDEN_CONF_DIR") ?? @"E:\MirServer\M2GameSvr";
        hashes = (confDir, new Dictionary<string, string>());
        missing = new List<string>();
        foreach (var n in ConfNames)
        {
            var src = Path.Combine(confDir, n);
            var dst = Path.Combine(baseDir, n);
            if (!File.Exists(src)) { missing.Add(src); continue; }
            if (!File.Exists(dst)) File.Copy(src, dst);
            hashes.Files[n] = Sha256Hex(File.ReadAllBytes(src));
        }
        return missing.Count == 0;
    }

    private static string Sha256Hex(byte[] bytes)
    {
        using var sha = SHA256.Create();
        return string.Concat(sha.ComputeHash(bytes).Select(b => b.ToString("x2")));
    }

    /// <summary>尽力取 oracle 仓库 head（信息性字段；取不到记 null，不冒充）。</summary>
    private static string TryRepoHead()
    {
        foreach (var cwd in new[] { Environment.CurrentDirectory, AppContext.BaseDirectory })
        {
            try
            {
                var psi = new ProcessStartInfo("git", "-C \"" + cwd + "\" rev-parse HEAD")
                {
                    UseShellExecute = false,
                    RedirectStandardOutput = true,
                    RedirectStandardError = true,
                    CreateNoWindow = true,
                };
                using var p = Process.Start(psi);
                if (p == null) continue;
                string outp = p.StandardOutput.ReadToEnd().Trim();
                p.WaitForExit(5000);
                if (p.ExitCode == 0 && outp.Length == 40) return outp;
            }
            catch
            {
                // 取不到就不记录（信息性字段，不是判据）。
            }
        }
        return null;
    }
}
