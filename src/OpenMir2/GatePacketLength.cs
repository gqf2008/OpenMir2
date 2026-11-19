namespace OpenMir2
{
    /// <summary>
    /// 网关/引擎帧头长度校验（卡 `openmir2-gate-packlen-validation`）。
    ///
    /// 背景：`packetLen` 直接取自网络包头，修复前被当作 `MemoryCopy.BlockCopy` 的 count。
    /// `MemoryCopy.CheckRange` 上线后越界从"进程级 AV"降级为托管异常——但异常抛在 TouchSocket
    /// 的收包回调里，该连接会停读；`new byte[packetLen]` 遇到 `int.MaxValue` 还会 OOM。
    /// **不可信输入不该走到抛异常这条路**：拷贝/切片前显式校验，不通过就记 WRN + 关闭连接。
    /// </summary>
    public static class GatePacketLength
    {
        /// <summary>
        /// 校验「包头声明的载荷长度」能否安全地当作 count/切片长度。
        /// </summary>
        /// <param name="packetLen">包头声明的长度（`ServerMessage.PackLength` 取绝对值后的值）。</param>
        /// <param name="available">实际可用字节数（`data.Length` / `Span.Length`）。</param>
        /// <param name="reason">失败原因（含原值，供 WRN 日志；通过时为空串）。</param>
        /// <returns>通过 ⇒ true；不通过 ⇒ false，且 <paramref name="reason"/> 非空。</returns>
        public static bool TryValidate(int packetLen, int available, out string reason)
        {
            if (packetLen < 0)
            {
                reason = $"packetLen={packetLen} < 0";
                return false;
            }
            if (packetLen > available)
            {
                reason = $"packetLen={packetLen} > available={available}";
                return false;
            }
            reason = string.Empty;
            return true;
        }
    }
}
