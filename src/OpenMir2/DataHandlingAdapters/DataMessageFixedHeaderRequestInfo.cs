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

            if (_header.PackLength == int.MinValue)
            {
                // 卡 openmir2-gate-packlen-validation：`-int.MinValue` 在 unchecked 下仍是它自己（负值），
                // 负的 bodyLength 会把异常抛进 TouchSocket 的收包路径。这里**按 0 长度交帧**，
                // 由 `ProcessServerPacket` 的显式校验（WRN + 关闭连接）做处置，问题不越过本层。
                this.bodyLength = 0;
                return true;
            }
            this.bodyLength = _header.PackLength < 0 ? -_header.PackLength : _header.PackLength;
            return true;
        }

        public bool OnParsingBody(byte[] body)
        {
            this._message = body;
            return true;
        }
    }
}