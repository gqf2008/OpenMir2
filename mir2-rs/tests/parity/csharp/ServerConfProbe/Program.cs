using System;
using System.IO;

namespace ServerConfProbe
{
    /// <summary>
    /// Server.conf 解析对拍探针（卡 openmir2-magic-serverconf-batch-plan §6.7）。
    ///
    /// 只做一件事：对给定 ini 跑 oracle 的 `ServerConf.LoadConfig()`，把六个契约键的
    /// 解析值按 `key=value` 逐行打到 stdout。**不读 golden、不做别的判断**——
    /// 对拍与断言在 Rust 侧测试 `serverconf_parse_parity` 里做。
    ///
    /// 用法：`ServerConfProbe.exe <Server.conf>`；rc=0 成功，2 = 用法/文件错误。
    /// </summary>
    public static class Program
    {
        public static int Main(string[] args)
        {
            // 原版 `ConfigFile.Load()` 用 `Encoding.GetEncoding("GB2312")`（`ConfigFile.cs:263`）；
            // .NET Core 默认不带代码页 ⇒ 必须先注册（与 MagicGolden 同法）。
            System.Text.Encoding.RegisterProvider(System.Text.CodePagesEncodingProvider.Instance);
            if (args.Length != 1)
            {
                Console.Error.WriteLine("usage: ServerConfProbe <Server.conf>");
                return 2;
            }

            string path = args[0];
            if (!File.Exists(path))
            {
                Console.Error.WriteLine("missing conf: " + path);
                return 2;
            }

            new SystemModule.Conf.ServerConf(path).LoadConfig();
            var cfg = SystemModule.SystemShare.Config;

            Console.WriteLine("mag_turn_undead_level=" + cfg.MagTurnUndeadLevel);
            Console.WriteLine("mab_mabe_hit_rand_rate=" + cfg.MabMabeHitRandRate);
            Console.WriteLine("mab_mabe_hit_min_lv_limit=" + cfg.MabMabeHitMinLvLimit);
            Console.WriteLine("mab_mabe_hit_sucess_rate=" + cfg.MabMabeHitSucessRate);
            Console.WriteLine("mab_mabe_hit_mabe_time_rate=" + cfg.MabMabeHitMabeTimeRate);
            Console.WriteLine("amy_ounsul_point=" + cfg.AmyOunsulPoint);
            // 追加：`PetSlaveConfig`（`crates/data/src/pet_slave.rs`）读的四个 `[Setup]` 键
            // （`ServerConf.cs:531-534`；注意 oracle 缺陷：HP 率的**键**写作 `MagTammingTargetHPRate`）。
            Console.WriteLine("mag_tamming_level=" + cfg.MagTammingLevel);
            Console.WriteLine("mag_tamming_target_level=" + cfg.MagTammingTargetLevel);
            Console.WriteLine("mag_tamming_hp_rate=" + cfg.MagTammingHPRate);
            Console.WriteLine("mag_tamming_count=" + cfg.MagTammingCount);
            return 0;
        }
    }
}
