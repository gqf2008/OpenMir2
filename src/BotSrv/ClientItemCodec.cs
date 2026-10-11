using OpenMir2;
using OpenMir2.Packets.ClientPackets;
using System.IO;

namespace BotSrv
{
    /// <summary>
    /// M3 T-14（T-22 夹具例外）：<see cref="ClientItem"/> 的**读取**实现。
    ///
    /// 背景（本片实测得出的根因）：`src/OpenMir2/Packets/ClientPackets/ClientItem.cs` 与
    /// `ClientStdItem.cs` 的 `ReadPacket` 都是 `throw new NotImplementedException()`
    /// —— 这对**服务端**没问题（它只 `EncodeBuffer` 写、从不读），但 BotSrv 作为**客户端**
    /// 解析 `SM_BAGITEMS`(201)/`SM_ADDITEM`(200) 时必须能读：
    /// `EDCode.DecodeClientBuffer<ClientItem>` 走 `ClientPacket.ToPacket` → `ReadPacket` 抛异常
    /// → `catch` 吞掉返回 `null` ⇒ 假人的背包视图**恒为空**，T-14 的
    /// 「拾取→装备→卸下→丢弃→用药」五步全部落 `skip`，跨侧 diff 也就无法反映任何背包行为
    /// （实测：`SM_BAGITEMS body_len=668` 而 `bag_view=0`）。
    ///
    /// **为什么不改 `src/OpenMir2`**：`OpenMir2.dll` 进 oracle 部署包（`E:\MirServer\M2GameSvr`）且是
    /// M3 RNG 基线 `pack_fingerprint` 的被哈希件之一 ⇒ 改它会改 oracle 二进制指纹。
    /// 因此把解码就地放在 BotSrv（客户端工具，不在 oracle 七进程内，同 T-1/T-5..T-12/T-22）。
    ///
    /// 字段序与 `ClientItem.WritePacket` **逐字段 1:1**（含 `WriteAsciiString` 的 1 字节长度 + 定长内容）。
    /// 仅当 `MShare.g_boBagEquipSequence` 打开时才被 `RobotPlayer.ClientGetAddItem` /
    /// `ClientGetBagItmes` 选用（默认关 ⇒ 原路径/字节序完全不变）。
    /// </summary>
    public static class ClientItemCodec
    {
        public static ClientItem Decode(string src)
        {
            if (string.IsNullOrEmpty(src))
            {
                return null;
            }

            byte[] bSrc = HUtil32.GetBytes(src);
            int nLen = 0;
            byte[] data = EncryptUtil.Decode(bSrc, bSrc.Length, ref nLen);
            using MemoryStream ms = new MemoryStream(data, 0, nLen);
            using BinaryReader r = new BinaryReader(ms);

            ClientItem ci = new ClientItem();
            ClientItem.ClientStdItem it = ci.Item;

            // 与 ClientItem.WritePacket 的写入顺序一一对应。
            it.Name = ReadAsciiString(r, ClientPacketConst.ItemNameLen);
            it.StdMode = r.ReadByte();
            it.Shape = r.ReadByte();
            it.Weight = r.ReadByte();
            it.AniCount = r.ReadByte();
            it.SpecialPwr = r.ReadSByte();
            it.ItemDesc = r.ReadByte();
            r.ReadByte();
            it.Looks = r.ReadUInt16();
            it.DuraMax = r.ReadUInt16();
            it.AC = r.ReadUInt16();
            it.MAC = r.ReadUInt16();
            it.DC = r.ReadUInt16();
            it.MC = r.ReadUInt16();
            it.SC = r.ReadUInt16();
            it.Need = r.ReadByte();
            it.NeedLevel = r.ReadByte();
            it.NeedIdentify = r.ReadByte();
            r.ReadByte();
            it.Price = r.ReadInt32();
            it.Stock = r.ReadInt32();
            it.AtkSpd = r.ReadByte();
            it.Agility = r.ReadByte();
            it.Accurate = r.ReadByte();
            it.MgAvoid = r.ReadByte();
            it.Strong = r.ReadByte();
            it.Undead = r.ReadByte();
            r.ReadByte();
            r.ReadByte();
            it.HpAdd = r.ReadInt32();
            it.MpAdd = r.ReadInt32();
            it.ExpAdd = r.ReadInt32();
            it.EffType1 = r.ReadByte();
            it.EffRate1 = r.ReadByte();
            it.EffValue1 = r.ReadByte();
            it.EffType2 = r.ReadByte();
            it.EffRate2 = r.ReadByte();
            it.EffValue2 = r.ReadByte();
            it.Slowdown = r.ReadByte();
            it.Tox = r.ReadByte();
            it.ToxAvoid = r.ReadByte();
            it.UniqueItem = r.ReadByte();
            it.OverlapItem = r.ReadByte();
            it.Light = r.ReadByte();
            it.ItemType = r.ReadByte();
            r.ReadByte();
            it.ItemSet = r.ReadUInt16();
            it.Reference = ReadAsciiString(r, ClientPacketConst.ItemNameLen);
            r.ReadByte();
            ci.MakeIndex = r.ReadInt32();
            ci.Dura = r.ReadUInt16();
            ci.DuraMax = r.ReadUInt16();
            ci.Desc = r.ReadBytes(14);
            r.ReadByte();
            r.ReadByte();

            if (string.IsNullOrEmpty(it.Name))
            {
                return null;
            }
            return ci;
        }

        /// <summary>对应 <c>BinaryWriterExtension.WriteAsciiString</c>：1 字节长度 + 定长内容。</summary>
        private static string ReadAsciiString(BinaryReader r, int defaultSize)
        {
            int len = r.ReadByte();
            byte[] buf = r.ReadBytes(defaultSize);
            int n = len <= defaultSize ? len : defaultSize;
            if (n > buf.Length)
            {
                n = buf.Length;
            }
            return HUtil32.GetString(buf, 0, n);
        }
    }
}
