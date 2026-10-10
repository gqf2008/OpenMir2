// GB2312(cp936) / BOM 行为探针（B 线脚本引擎解码层的判据来源）。
//
// 用途：
//   dotnet run --project mir2-rs/tools/gbk-probe -- pairs <输出文件>
//       导出 .NET cp936 对全部双字节对（0x81..=0xFE × 0x00..=0xFF = 32256 条）的映射，
//       供 `script-tool gen-gbk-overrides` 生成 `crates/script/src/gbk_overrides.rs`；
//       同时该文件本身作为 `crates/script/tests/gbk_decode.rs` 的穷举判据（已入库）。
//   dotnet run --project mir2-rs/tools/gbk-probe -- bom
//       打印 BOM 判定与各分支非法序列的实测结果（StringList 真身，非推理）。
//
// 只读引用参照实现，不修改任何 C# 参照代码。
using System;
using System.IO;
using System.Text;
using OpenMir2.Common;

static class Program
{
    static int Main(string[] args)
    {
        if (args.Length < 1)
        {
            Console.Error.WriteLine("用法: gbk-probe pairs <out.txt> | gbk-probe bom");
            return 2;
        }
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        switch (args[0])
        {
            case "pairs":
                if (args.Length < 2) { Console.Error.WriteLine("pairs 需要输出路径"); return 2; }
                DumpPairs(args[1]);
                return 0;
            case "bom":
                DumpBom();
                return 0;
            default:
                Console.Error.WriteLine($"未知子命令: {args[0]}");
                return 2;
        }
    }

    /// 全部双字节对的 cp936 映射：`XXXX\tHEX[,HEX...]`（码点为空表示空串）。
    static void DumpPairs(string outPath)
    {
        var enc = Encoding.GetEncoding("gb2312");
        using var w = new StreamWriter(outPath, false, new UTF8Encoding(false));
        for (int l = 0x81; l <= 0xFE; l++)
        {
            for (int t = 0x00; t <= 0xFF; t++)
            {
                var s = enc.GetString(new byte[] { (byte)l, (byte)t });
                w.WriteLine($"{l:X2}{t:X2}\t{string.Join(",", Array.ConvertAll(s.ToCharArray(), c => ((int)c).ToString("X4")))}");
            }
        }
        Console.WriteLine($"已写出 {outPath}（126 × 256 = 32256 行）");
    }

    /// BOM 判定 + 各分支非法序列回退字符（经真实 StringList.LoadFromFile）。
    static void DumpBom()
    {
        Show("FF FE FF 41", new byte[] { 0xFF, 0xFE, 0xFF, 0x41 });          // → U+41FF（UTF-16LE）
        Show("FF FE 00 00 41 00", new byte[] { 0xFF, 0xFE, 0x00, 0x00, 0x41, 0x00 }); // → U+0000 U+0041
        Show("FE FF 00 41", new byte[] { 0xFE, 0xFF, 0x00, 0x41 });          // → U+0041（UTF-16BE）
        Show("00 00 FE FF 00 00 00 41", new byte[] { 0x00, 0x00, 0xFE, 0xFF, 0x00, 0x00, 0x00, 0x41 }); // → U+0041（UTF-32BE）
        Show("EF BB BF 41", new byte[] { 0xEF, 0xBB, 0xBF, 0x41 });          // → U+0041（UTF-8）
        Show("EF BB BF FF 41", new byte[] { 0xEF, 0xBB, 0xBF, 0xFF, 0x41 }); // → U+FFFD U+0041
        Show("EF BB BF ED A0 80", new byte[] { 0xEF, 0xBB, 0xBF, 0xED, 0xA0, 0x80 }); // → 三个 U+FFFD
        Show("FF FE 00 D8 41 00", new byte[] { 0xFF, 0xFE, 0x00, 0xD8, 0x41, 0x00 }); // → U+FFFD U+0041
        Show("81 20 41（gb2312）", new byte[] { 0x81, 0x20, 0x41 });          // → U+003F U+0041
        Show("A8 BF（gb2312）", new byte[] { 0xA8, 0xBF });                  // → U+E7C8（cp936 PUA）
        Show("80 63（gb2312）", new byte[] { 0x80, 0x63 });                  // → U+20AC U+0063
        Show("FF 41（gb2312）", new byte[] { 0xFF, 0x41 });                  // → U+F8F5 U+0041
    }

    static void Show(string name, byte[] bytes)
    {
        var dir = Path.Combine(AppContext.BaseDirectory, "gbk-probe-tmp");
        Directory.CreateDirectory(dir);
        var f = Path.Combine(dir, Guid.NewGuid().ToString("N") + ".txt");
        File.WriteAllBytes(f, bytes);
        var list = new StringList();
        list.LoadFromFile(f);
        Console.Write($"{name} ({BitConverter.ToString(bytes)}) →");
        for (int i = 0; i < list.Count; i++)
        {
            Console.Write($" 行{i}:");
            foreach (var c in list[i]) Console.Write($" U+{(int)c:X4}");
        }
        Console.WriteLine();
        File.Delete(f);
    }
}
