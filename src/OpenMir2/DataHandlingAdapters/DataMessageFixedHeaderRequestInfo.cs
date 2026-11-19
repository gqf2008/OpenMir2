using OpenMir2.Packets.ServerPackets;
using System.Runtime.InteropServices;
using TouchSocket.Core;

namespace OpenMir2.DataHandlingAdapters
{
    public class PacketFixedHeaderDataHandlingAdapter : CustomFixedHeaderDataHandlingAdapter<DataMessageFixedHeaderRequestInfo>
    {
        /// <summary>
        /// 接口实现，指示固定包头长度
        /// </summary>
        public override int HeaderLength => 20;

        private static readonly DataMessageFixedHeaderRequestInfo instance = new DataMessageFixedHeaderRequestInfo();

        private static DataMessageFixedHeaderRequestInfo Instance => instance;

        /// <summary>
        /// 获取新实例
        /// </summary>
        /// <returns></returns>
        protected override DataMessageFixedHeaderRequestInfo GetInstance()
        {
            return Instance;
        }
    }

    public class DataMessageFixedHeaderRequestInfo : IFixedHeaderRequestInfo
    {
        private int bodyLength;
        public int BodyLength => bodyLength;
        private ServerMessage _header;
        public ServerMessage Header => _header;
        private byte[] _message;
        public byte[] Message => _message;

        public bool OnParsingHeader(byte[] header)
        {
            if (!MemoryMarshal.TryRead(header, out _header))
            {
                return false;
            }

            // 卡 openmir2-gate-packlen-validation：帧层**只能交帧**（没有"拒绝并关连接"的出口），
            // 所以这里把"无法表示的长度"降级为 0 字节交帧——**不是放行**：放行/拒绝的唯一判定在
            // 下游 `ClientThread.TryBuildServerSessionMessage`，两层共用 `GatePacketLength.TryMagnitude`。
            if (!GatePacketLength.TryMagnitude(_header.PackLength, out this.bodyLength))
            {
                this.bodyLength = 0;
            }
            return true;
        }

        public bool OnParsingBody(byte[] body)
        {
            this._message = body;
            return true;
        }
    }
}