using NUnit.Framework;
using OpenMir2;
using OpenMir2.Packets.ClientPackets;
using OpenMir2.Packets.ServerPackets;

namespace GameSrvTest;

/// <summary>
/// 卡 `openmir2-db-write-stall` 的**前提牙**：多处拷贝把 `ServerMessage.PacketSize` /
/// `CommandMessage.Size` 当序列化长度用（GameGate/ClientSession、M2Server/PlayObject.Socket
/// 的常量 count 站点）。这只有在「序列化出多少字节 == 常量」时成立。
///
/// 一旦有人给这两个结构加字段而忘了同步常量，这些站点就会**静默截断/错位**；
/// 本测试让那种改动立刻变红。（常量站点已就地加注释指向本测试。）
/// </summary>
[TestFixture]
public class PacketLayoutConstantsTest
{
    [Test]
    public void ServerMessage_Serialized_Length_Equals_PacketSize()
    {
        var message = new ServerMessage
        {
            PacketCode = 0xAA9AAA9A,
            Socket = 7,
            SessionId = 9,
            Ident = 1000,
            SessionIndex = 3,
            PackLength = 42,
        };
        int len = SerializerUtil.Serialize(message).Length;
        Assert.That(len, Is.EqualTo(ServerMessage.PacketSize),
            "ServerMessage 的 MemoryPack 布局变了：常量 PacketSize 与序列化长度脱节，" +
            "所有用 PacketSize 当 count 的拷贝站点都会错位");
    }

    [Test]
    public void CommandMessage_Serialized_Length_Equals_Size()
    {
        var command = new CommandMessage
        {
            Recog = 1,
            Ident = 3011,
            Param = 2,
            Tag = 3,
            Series = 4,
        };
        int len = SerializerUtil.Serialize(command).Length;
        Assert.That(len, Is.EqualTo(CommandMessage.Size),
            "CommandMessage 的 MemoryPack 布局变了：常量 Size 与序列化长度脱节");
    }
}
