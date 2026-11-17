using OpenMir2.Packets.ServerPackets;
using System.Runtime.InteropServices;
using TouchSocket.Core;

namespace OpenMir2.DataHandlingAdapters
{
    public class PlayerDataFixedHeaderDataHandlingAdapter : CustomFixedHeaderDataHandlingAdapter<PlayerDataMessageFixedHeaderRequestInfo>
    {
        /// <summary>
        /// 接口实现，指示固定包头长度
        /// </summary>
        public override int HeaderLength => 6;

        // 工厂语义：每次取实例都必须是新对象（卡 `openmir2-db-link-request-drop`）——
        // 原先返回 static 单例，`_header/_bodyLength/_message` 会被下一帧覆盖 ⇒ 帧边界串台。

        /// <summary>
        /// 获取新实例
        /// </summary>
        /// <returns></returns>
        protected override PlayerDataMessageFixedHeaderRequestInfo GetInstance()
        {
            return new PlayerDataMessageFixedHeaderRequestInfo();
        }
    }

    public class PlayerDataMessageFixedHeaderRequestInfo : IFixedHeaderRequestInfo
    {
        private int bodyLength;
        public int BodyLength => bodyLength;
        private ServerDataPacket _header;
        public ServerDataPacket Header => _header;
        private byte[] _message;
        public byte[] Message => _message;

        public bool OnParsingHeader(byte[] header)
        {
            if (!MemoryMarshal.TryRead(header, out _header))
            {
                return false;
            }

            this.bodyLength = _header.PacketLen;
            return true;
        }

        public bool OnParsingBody(byte[] body)
        {
            // 拷贝：即使上层复用传入数组，回调拿到的也是独立副本（第二道防线）。
            this._message = body.Length == 0 ? System.Array.Empty<byte>() : (byte[])body.Clone();
            return true;
        }
    }
}