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
        /// 原始包头字段 → 可安全当 count 的幅值（**单一真源**：帧层与判定层都用它）。
        ///
        /// 本协议的 `PackLength` **负值是合法语义**（"小包/已编码"，绝对值是长度），所以负字段
        /// 本身不是错误；唯一无正值表示的是 `int.MinValue`（`-MinValue` 在 unchecked 下仍是负数）
        /// ⇒ 返回 false，由调用方按非法帧处置。
        /// </summary>
        public static bool TryMagnitude(int rawPackLength, out int magnitude)
        {
            if (rawPackLength == int.MinValue)
            {
                magnitude = 0;
                return false;
            }
            magnitude = rawPackLength < 0 ? -rawPackLength : rawPackLength;
            return true;
        }

        /// <summary>校验**幅值**能否当 count（负值/越界 ⇒ false）。</summary>
        public static bool TryValidateMagnitude(int packetLen, int available, out string reason)
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

        /// <summary>生产调用点用的**一步校验**：原始字段 → 幅值 → 与可用长度比较。</summary>
        public static bool TryValidate(int rawPackLength, int available, out string reason)
        {
            if (!TryMagnitude(rawPackLength, out int packetLen))
            {
                reason = $"raw PackLength={rawPackLength} 无正值表示（int.MinValue）";
                return false;
            }
            return TryValidateMagnitude(packetLen, available, out reason);
        }
    }
}
