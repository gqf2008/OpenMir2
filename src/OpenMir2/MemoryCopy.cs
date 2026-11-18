using System;

namespace OpenMir2
{
    public static class MemoryCopy
    {
        /// <summary>
        /// 边界检查（卡 `openmir2-db-write-stall`）：本类内部是 `Buffer.MemoryCopy`——**零边界检查**。
        /// 越界读会踩到未映射页 ⇒ 访问违例 ⇒ CLR 致命错误 `0x80131506`（随后卡死在
        /// `NtRaiseHardError` 的报告路径上，进程还活着、该线程永不返回）。
        /// 生产事故：`SendMessage` 用 `data.Length` 当源长度，从 6 字节包头数组越界读，
        /// 5000 人压测里两次把 DBSrv 的取档连接打死。
        /// 这里把「越界」从进程级 AV 降级为托管异常；调用点仍必须传正确长度。
        /// </summary>
        /// <remarks>
        /// **能力边界**：只有源/目的**都有长度**的重载（byte[] / Span）能查两端；
        /// `nint` 与 `void*` 重载<b>只能查目的端</b>——源端是否够长由调用方负责。
        /// 别因为「有检查」就以为安全。
        /// </remarks>
        private static void CheckRange(int offset, int count, int length, string paramName)
        {
            if (offset < 0 || count < 0 || (long)offset + count > length)
            {
                throw new ArgumentOutOfRangeException(paramName,
                    $"offset={offset} count={count} length={length}（MemoryCopy 无边界检查，越界读会致命）");
            }
        }

        public static void ArrayCopy(byte[] source, int srcOff, byte[] destination, int dstOff, int count)
        {
            Array.Copy(source, srcOff, destination, dstOff, count);
        }

        public static unsafe void BlockCopy(nint source, int srcOff, Span<byte> destination, int dstOff, int count)
        {
            CheckRange(dstOff, count, destination.Length, nameof(destination));
            Span<byte> destinationSpan = new Span<byte>(source.ToPointer(), count);
            fixed (byte* src = &destinationSpan[srcOff])
            {
                fixed (byte* dest = &destination[dstOff])
                {
                    Buffer.MemoryCopy(
                        source: src, //要复制的字节的地址
                        destination: dest, //目标地址
                        destinationSizeInBytes: count, //目标内存块中可用的字节数
                        sourceBytesToCopy: count //要复制的字节数
                    );
                }
            }
        }

        public static unsafe void BlockCopy(ReadOnlySpan<byte> source, int srcOff, void* destination, int dstOff, int count)
        {
            CheckRange(srcOff, count, source.Length, nameof(source));
            unsafe
            {
                fixed (byte* src = &source[srcOff])
                {
                    Buffer.MemoryCopy(
                        source: src, //要复制的字节的地址
                        destination: destination, //目标地址
                        destinationSizeInBytes: count, //目标内存块中可用的字节数
                        sourceBytesToCopy: count //要复制的字节数
                    );
                }
            }
        }

        public static void BlockCopy(byte[] source, int srcOff, Span<byte> destination, int dstOff, int count)
        {
            CheckRange(srcOff, count, source.Length, nameof(source));
            CheckRange(dstOff, count, destination.Length, nameof(destination));
            unsafe
            {
                fixed (byte* src = &source[srcOff])
                {
                    fixed (byte* dest = &destination[dstOff])
                    {
                        Buffer.MemoryCopy(
                            source: src, //要复制的字节的地址
                            destination: dest, //目标地址
                            destinationSizeInBytes: count, //目标内存块中可用的字节数
                            sourceBytesToCopy: count //要复制的字节数
                        );
                    }
                }
            }
        }

        public static void BlockCopy(Span<byte> source, int srcOff, Span<byte> destination, int dstOff, int count)
        {
            unsafe
            {
                fixed (byte* src = &source[srcOff])
                {
                    fixed (byte* dest = &destination[dstOff])
                    {
                        Buffer.MemoryCopy(
                            source: src, //要复制的字节的地址
                            destination: dest, //目标地址
                            destinationSizeInBytes: count, //目标内存块中可用的字节数
                            sourceBytesToCopy: count //要复制的字节数
                        );
                    }
                }
            }
        }

        public static void BlockCopy(ReadOnlySpan<byte> source, int srcOff, Span<byte> destination, int dstOff, int count)
        {
            CheckRange(srcOff, count, source.Length, nameof(source));
            CheckRange(dstOff, count, destination.Length, nameof(destination));
            unsafe
            {
                fixed (byte* src = &source[srcOff])
                {
                    fixed (byte* dest = &destination[dstOff])
                    {
                        Buffer.MemoryCopy(
                            source: src, //要复制的字节的地址
                            destination: dest, //目标地址
                            destinationSizeInBytes: count, //目标内存块中可用的字节数
                            sourceBytesToCopy: count //要复制的字节数
                        );
                    }
                }
            }
        }
    }
}