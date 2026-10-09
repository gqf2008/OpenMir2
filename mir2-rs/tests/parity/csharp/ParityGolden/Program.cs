// D 线金标准生成器。
// 用法：
//   ParityGolden rng <seed> <out.txt>
//   ParityGolden exp <Exps.conf> <out.txt>
//   ParityGolden drop <MonItems.txt> <items.json> <seed> <kills> <monRandomAddValue> <out.txt>
//   ParityGolden levelabil <out.txt>
//
// 只读：不写 MySQL、不改任何现网文件（Exps.conf 应传夹具副本路径）。

using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Text;
using System.Text.Json;
using OpenMir2.Common;
using ParityGolden;

internal static class Program
{
    private static int Main(string[] args)
    {
        // .NET Core 默认不带 GB2312；OpenMir2 各进程启动时同样注册了这个 Provider
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        if (args.Length < 1)
        {
            Console.Error.WriteLine("mode required: rng|exp|drop|levelabil");
            return 2;
        }
        switch (args[0])
        {
            case "rng":
                return RngMode(int.Parse(args[1]), args[2]);
            case "exp":
                return ExpMode(args[1], args[2]);
            case "drop":
                return DropMode(args[1], args[2], int.Parse(args[3]), int.Parse(args[4]), byte.Parse(args[5]), args[6]);
            case "levelabil":
                return LevelAbilMode(args[1]);
            default:
                Console.Error.WriteLine($"unknown mode {args[0]}");
                return 2;
        }
    }

    /// <summary>把固定种子注入链接进来的真实 RandomNumber（覆盖其私有静态 Random 字段）。</summary>
    private static void SeedRandomNumber(int seed)
    {
        var inst = OpenMir2.RandomNumber.GetInstance(); // 确保单例已建
        var field = typeof(OpenMir2.RandomNumber).GetField("random", BindingFlags.NonPublic | BindingFlags.Static);
        if (field == null) throw new InvalidOperationException("RandomNumber.random field not found");
        field.SetValue(null, new Random(seed));
        _ = inst;
    }

    /// <summary>
    /// RNG 序列：混合调用 RandomNumber 全部取数方法。
    /// 调用脚本必须与 Rust 侧 tests/parity/tests/rng_parity.rs 逐条一致。
    /// </summary>
    private static int RngMode(int seed, string outPath)
    {
        SeedRandomNumber(seed);
        var rn = OpenMir2.RandomNumber.GetInstance();
        var sb = new StringBuilder();
        for (int i = 0; i < 500; i++)
        {
            sb.Append(rn.Random()).Append('\n');                    // Random()
            sb.Append(rn.Random(100)).Append('\n');                 // Random(int value)
            sb.Append(rn.Random(10, 1000)).Append('\n');            // Random(int min, int max)
            sb.Append(rn.GetRandomNumber(1, 6)).Append('\n');       // GetRandomNumber(min, max) 含上界
            sb.Append(rn.RandomByte(200)).Append('\n');             // RandomByte
            if (i % 10 == 0)
            {
                sb.Append(rn.GenerateRandomNumber(8)).Append('\n'); // 字符串
            }
            if (i % 25 == 0)
            {
                IList<int> src = new List<int> { 1, 2, 3, 4, 5, 6, 7, 8, 9, 10 };
                var picked = rn.RandomSelect(src, 3);
                sb.Append(string.Join(',', picked)).Append('\n');
            }
        }
        File.WriteAllText(outPath, sb.ToString());
        Console.WriteLine($"rng seed={seed} lines written -> {outPath}");
        return 0;
    }

    /// <summary>用真实 ConfigFile 读 Exps.conf，复刻 ExpsConf.LoadConfig，输出 NeedExps[0..999] 与标量。</summary>
    private sealed class ExpsReader : ConfigFile
    {
        public ExpsReader(string fileName) : base(fileName)
        {
            Load();
        }
    }

    private static int ExpMode(string confPath, string outPath)
    {
        var conf = new ExpsReader(confPath);
        var cfg = SystemShare.Config;

        // src/Modules/SystemModule/Conf/ExpsConf.cs:13 LoadConfig 逐条对应
        cfg.LimitExpLevel = conf.ReadWriteInteger("Exp", "LimitExpLevel", cfg.LimitExpLevel);
        cfg.LimitExpValue = conf.ReadWriteInteger("Exp", "LimitExpValue", cfg.LimitExpValue);
        cfg.KillMonExpMultiple = conf.ReadWriteInteger("Exp", "KillMonExpMultiple", cfg.KillMonExpMultiple);
        cfg.HighLevelKillMonFixExp = conf.ReadWriteBool("Exp", "HighLevelKillMonFixExp", cfg.HighLevelKillMonFixExp);
        cfg.HighLevelGroupFixExp = conf.ReadWriteBool("Exp", "HighLevelGroupFixExp", cfg.HighLevelGroupFixExp);

        for (int i = 0; i < cfg.NeedExps.Length; i++)
        {
            string LoadString = conf.ReadWriteString("Exp", "Level" + i, "");
            int LoadInteger = HUtil32.StrToInt(LoadString, 0);
            if (LoadInteger == 0)
            {
                int oldNeedExp = SystemShare.OldNeedExps[i];
                if (oldNeedExp <= 0)
                {
                    oldNeedExp = conf.ReadWriteInteger("Exp", "Level" + i, 0);
                    cfg.NeedExps[i] = oldNeedExp;
                }
                else
                {
                    cfg.NeedExps[i] = oldNeedExp;
                }
            }
            else
            {
                cfg.NeedExps[i] = LoadInteger;
            }
        }

        cfg.UseFixExp = conf.ReadWriteBool("Exp", "UseFixExp", cfg.UseFixExp);
        cfg.MonDelHptoExp = conf.ReadWriteBool("Exp", "MonDelHptoExp", cfg.MonDelHptoExp);
        cfg.BaseExp = conf.ReadWriteInteger("Exp", "BaseExp", cfg.BaseExp);
        cfg.AddExp = conf.ReadWriteInteger("Exp", "AddExp", cfg.AddExp);
        cfg.MonHptoExpLevel = conf.ReadWriteInteger("Exp", "MonHptoExpLevel", cfg.MonHptoExpLevel);
        cfg.MonHptoExpmax = conf.ReadWriteInteger("Exp", "MonHptoExpmax", cfg.MonHptoExpmax);

        var sb = new StringBuilder();
        for (int i = 0; i < cfg.NeedExps.Length; i++)
        {
            sb.Append(cfg.NeedExps[i]).Append('\n');
        }
        sb.Append("LimitExpLevel=").Append(cfg.LimitExpLevel).Append('\n');
        sb.Append("LimitExpValue=").Append(cfg.LimitExpValue).Append('\n');
        sb.Append("KillMonExpMultiple=").Append(cfg.KillMonExpMultiple).Append('\n');
        sb.Append("HighLevelKillMonFixExp=").Append(cfg.HighLevelKillMonFixExp).Append('\n');
        sb.Append("HighLevelGroupFixExp=").Append(cfg.HighLevelGroupFixExp).Append('\n');
        sb.Append("UseFixExp=").Append(cfg.UseFixExp).Append('\n');
        sb.Append("MonDelHptoExp=").Append(cfg.MonDelHptoExp).Append('\n');
        sb.Append("BaseExp=").Append(cfg.BaseExp).Append('\n');
        sb.Append("AddExp=").Append(cfg.AddExp).Append('\n');
        sb.Append("MonHptoExpLevel=").Append(cfg.MonHptoExpLevel).Append('\n');
        sb.Append("MonHptoExpmax=").Append(cfg.MonHptoExpmax).Append('\n');
        File.WriteAllText(outPath, sb.ToString());
        Console.WriteLine($"exp: 1000 need-exps + scalars -> {outPath}");
        return 0;
    }

    private sealed class ItemJson
    {
        public string name { get; set; }
        public byte std_mode { get; set; }
        public byte shape { get; set; }
        public ushort dura_max { get; set; }
    }

    /// <summary>掉落：同一掉落表 + 同一物品表，固定种子跑 kills 次击杀。</summary>
    private static int DropMode(string monItemsPath, string itemsJsonPath, int seed, int kills, byte monRandomAddValue, string outPath)
    {
        SeedRandomNumber(seed);
        SystemShare.Config.MonRandomAddValue = monRandomAddValue;

        var items = JsonSerializer.Deserialize<List<ItemJson>>(File.ReadAllText(itemsJsonPath));
        foreach (var ij in items)
        {
            SystemShare.ItemSystem.StdItemList.Add(new StdItem
            {
                Name = ij.name,
                StdMode = ij.std_mode,
                Shape = ij.shape,
                DuraMax = ij.dura_max,
            });
        }

        IList<MonsterDropItem> itemList = null;
        WorldServerCopies.LoadMonitems(monItemsPath, ref itemList);

        var sb = new StringBuilder();
        for (int k = 1; k <= kills; k++)
        {
            var mon = new MonsterActorStub();
            WorldServerCopies.MonGetRandomItems(itemList, mon);
            sb.Append("KILL ").Append(k).Append(" G ").Append(mon.Gold)
                .Append(" N ").Append(mon.ItemList?.Count ?? 0).Append('\n');
            if (mon.ItemList != null)
            {
                foreach (var it in mon.ItemList)
                {
                    // MakeIndex 含墙钟（M2Share.GetItemNumber = 计数器 + TickCount），不进对拍
                    sb.Append("I ").Append(it.Index).Append(' ').Append(it.Dura).Append(' ')
                        .Append(it.DuraMax);
                    foreach (var b in it.Desc)
                    {
                        sb.Append(' ').Append(b);
                    }
                    sb.Append('\n');
                }
            }
        }
        File.WriteAllText(outPath, sb.ToString());
        Console.WriteLine($"drop: {kills} kills, seed={seed}, monRandomAddValue={monRandomAddValue} -> {outPath}");
        return 0;
    }

    /// <summary>三职业 0..=255 级 RecalcLevelAbilitys 全表。</summary>
    private static int LevelAbilMode(string outPath)
    {
        var sb = new StringBuilder();
        foreach (PlayerJob job in new[] { PlayerJob.Warrior, PlayerJob.Wizard, PlayerJob.Taoist })
        {
            for (int lv = 0; lv <= 255; lv++)
            {
                var abil = new Ability { Level = (byte)lv };
                PlayObjectCopies.RecalcLevelAbilitys(job, abil);
                sb.Append((int)job).Append(' ').Append(lv).Append(' ')
                    .Append(abil.MaxHP).Append(' ').Append(abil.MaxMP).Append(' ')
                    .Append(abil.MaxWeight).Append(' ').Append(abil.MaxWearWeight).Append(' ')
                    .Append(abil.MaxHandWeight).Append(' ')
                    .Append(abil.DC).Append(' ').Append(abil.MC).Append(' ')
                    .Append(abil.SC).Append(' ').Append(abil.AC).Append(' ').Append(abil.MAC)
                    .Append('\n');
            }
        }
        File.WriteAllText(outPath, sb.ToString());
        Console.WriteLine($"levelabil: 3 jobs x 256 levels -> {outPath}");
        return 0;
    }
}
