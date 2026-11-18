// 卡 `openmir2-db-write-stall` 的**确定性复现探针**（独立进程，不依赖 DBSrv）。
//
// 复刻事故形态：源是 6 字节数组，却按**目的**长度去读。
//   raw  —— 直接 Buffer.MemoryCopy（= 修复前 MemoryCopy.BlockCopy 的内部实现），
//           从 6 字节数组越界读 64 MB ⇒ 访问违例 ⇒ 进程被 CLR 终止（退出码非 0）。
//   safe —— 只拷源长度 6 字节（= 修复后调用点的形态）⇒ 正常退出 0。
//
// 用法：dotnet run -c Release --project tools/diagnose/memorycopy_oob_probe -- [raw|safe]
using System;

internal static class Program
{
    private static unsafe int Main(string[] args)
    {
        string mode = args.Length > 0 ? args[0] : "raw";
        byte[] src = new byte[6];
        int count = mode == "safe" ? src.Length : 64 * 1024 * 1024;
        byte[] dst = new byte[count];
        fixed (byte* s = src)
        fixed (byte* d = dst)
        {
            Buffer.MemoryCopy(s, d, count, count);
        }

        Console.WriteLine("COPY_OK mode=" + mode + " count=" + count);
        return 0;
    }
}
