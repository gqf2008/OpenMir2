using OpenMir2;
using OpenMir2.Packets.ClientPackets;
using System;
using System.Buffers.Binary;
using System.IO;

namespace BotSrv
{
    /// <summary>
    /// 卡 `openmir2-botsrv-decode-compat`：`SM_ABILITY(52)` 体的**原始 `TAbility` 布局**解码。
    ///
    /// 为什么不能再用 `EDCode.DecodeBuffer&lt;Ability&gt;`：那是 **MemoryPack**（22 成员）反序列化，
    /// 会把体首字节当对象头。线上体是冻结客户端的 58B `TAbility`（首字节 = `Level: Word` 低字节），
    /// 等级 &gt;22 时抛 `property count is 22 but binary's header maked as N`，等级 ≤22 时不抛但
    /// **静默解成全零**（测量有效性问题：假人 `WAbil` 一直是错的）。
    ///
    /// 两种合法布局（按 EDCode 解码后的**长度**选路，二者在线上互斥）：
    ///   · 58B = 冻结客户端 `TAbility`（`Source/Common/Grobal2.pas`，`HIGHHP=True`）——
    ///     `Level: Word`、`AC/MAC/DC/MC/SC: Integer`、`HP/MP/MaxHP/MaxMP: DWORD`、
    ///     `Exp/MaxExp: DWORD`、6 个 `Word`；生产默认（`MIR2_CLIENT_ABILITY_LAYOUT` 未设）。
    ///   · 40B = oracle `EncodeMessage(WAbil)` 去掉对象头后的 MemoryPack 成员序——
    ///     只有 `MIR2_CLIENT_ABILITY_LAYOUT=0` 的逐字节对拍栈才会发。
    ///
    /// 其它长度 ⇒ `InvalidDataException`（走 `recv_packet_other` 计数，不静默）。
    /// 全零结果 ⇒ **同样拒绝**：真实角色 `Level >= 1` 且 1 级就有非零 HP/Weight，
    /// 全零只可能来自错解；这正是旧路径在 `Level < 23` 时静默接受的那种形态。
    /// </summary>
    public static class AbilityCodec
    {
        /// <summary>冻结客户端 `TAbility`（`HIGHHP=True`）大小。</summary>
        public const int ClientLayoutSize = 58;

        /// <summary>oracle `EncodeMessage(WAbil)` 尾（MemoryPack 去对象头）大小。</summary>
        public const int OracleLayoutSize = 40;

        /// <summary>从 `SM_ABILITY` 的 EDCode 体（12B 命令头之后的字符串）解码。</summary>
        public static Ability Decode(string body)
        {
            if (string.IsNullOrEmpty(body))
            {
                throw new ArgumentNullException(nameof(body));
            }
            byte[] data = DecodeEdcode(body);
            return DecodeBytes(data);
        }

        /// <summary>从**已 EDCode 解码**的字节解码（自检/单测入口）。</summary>
        public static Ability DecodeBytes(byte[] data)
        {
            if (data == null)
            {
                throw new ArgumentNullException(nameof(data));
            }
            Ability abil;
            if (data.Length == ClientLayoutSize)
            {
                abil = DecodeClient58(data);
            }
            else if (data.Length == OracleLayoutSize)
            {
                abil = DecodeOracle40(data);
            }
            else
            {
                throw new InvalidDataException(
                    $"SM_ABILITY body length {data.Length} is neither client-58 ({ClientLayoutSize}) nor oracle-40 ({OracleLayoutSize}); refusing to guess");
            }
            if (IsAllZero(abil))
            {
                throw new InvalidDataException(
                    $"SM_ABILITY decoded to an all-zero Ability (body={data.Length}B): refusing a blank ability view (old MemoryPack path used to accept this silently for Level<23)");
            }
            return abil;
        }

        private static byte[] DecodeEdcode(string src)
        {
            byte[] bSrc = HUtil32.GetBytes(src);
            int nLen = 0;
            byte[] data = EncryptUtil.Decode(bSrc, bSrc.Length, ref nLen);
            if (nLen == data.Length)
            {
                return data;
            }
            // EncryptUtil.Decode 现在按 decodeLen 精确分配；保留裁剪以防未来实现变化。
            byte[] trimmed = new byte[nLen];
            Array.Copy(data, trimmed, nLen);
            return trimmed;
        }

        /// <summary>58B 冻结客户端 `TAbility`（`HIGHHP=True`）。</summary>
        private static Ability DecodeClient58(byte[] b)
        {
            return new Ability
            {
                Level = unchecked((byte)BinaryPrimitives.ReadUInt16LittleEndian(b.AsSpan(0))),
                AC = unchecked((ushort)BinaryPrimitives.ReadInt32LittleEndian(b.AsSpan(2))),
                MAC = unchecked((ushort)BinaryPrimitives.ReadInt32LittleEndian(b.AsSpan(6))),
                DC = unchecked((ushort)BinaryPrimitives.ReadInt32LittleEndian(b.AsSpan(10))),
                MC = unchecked((ushort)BinaryPrimitives.ReadInt32LittleEndian(b.AsSpan(14))),
                SC = unchecked((ushort)BinaryPrimitives.ReadInt32LittleEndian(b.AsSpan(18))),
                HP = unchecked((ushort)BinaryPrimitives.ReadUInt32LittleEndian(b.AsSpan(22))),
                MP = unchecked((ushort)BinaryPrimitives.ReadUInt32LittleEndian(b.AsSpan(26))),
                MaxHP = unchecked((ushort)BinaryPrimitives.ReadUInt32LittleEndian(b.AsSpan(30))),
                MaxMP = unchecked((ushort)BinaryPrimitives.ReadUInt32LittleEndian(b.AsSpan(34))),
                Exp = BinaryPrimitives.ReadInt32LittleEndian(b.AsSpan(38)),
                MaxExp = BinaryPrimitives.ReadInt32LittleEndian(b.AsSpan(42)),
                Weight = BinaryPrimitives.ReadUInt16LittleEndian(b.AsSpan(46)),
                MaxWeight = BinaryPrimitives.ReadUInt16LittleEndian(b.AsSpan(48)),
                WearWeight = unchecked((byte)BinaryPrimitives.ReadUInt16LittleEndian(b.AsSpan(50))),
                MaxWearWeight = unchecked((byte)BinaryPrimitives.ReadUInt16LittleEndian(b.AsSpan(52))),
                HandWeight = unchecked((byte)BinaryPrimitives.ReadUInt16LittleEndian(b.AsSpan(54))),
                MaxHandWeight = unchecked((byte)BinaryPrimitives.ReadUInt16LittleEndian(b.AsSpan(56))),
                // 客户端线上无这四个 C# 专有字段（oracle 的 Reserved1/Reserved2/ExpCount/ExpMaxCount）。
                Reserved1 = 0,
                Reserved2 = 0,
                ExpCount = 0,
                ExpMaxCount = 0,
            };
        }

        /// <summary>40B oracle `EncodeMessage` 尾：MemoryPack 成员序、无对象头。</summary>
        private static Ability DecodeOracle40(byte[] b)
        {
            return new Ability
            {
                Level = b[0],
                Reserved1 = b[1],
                AC = BinaryPrimitives.ReadUInt16LittleEndian(b.AsSpan(2)),
                MAC = BinaryPrimitives.ReadUInt16LittleEndian(b.AsSpan(4)),
                DC = BinaryPrimitives.ReadUInt16LittleEndian(b.AsSpan(6)),
                MC = BinaryPrimitives.ReadUInt16LittleEndian(b.AsSpan(8)),
                SC = BinaryPrimitives.ReadUInt16LittleEndian(b.AsSpan(10)),
                HP = BinaryPrimitives.ReadUInt16LittleEndian(b.AsSpan(12)),
                MP = BinaryPrimitives.ReadUInt16LittleEndian(b.AsSpan(14)),
                MaxHP = BinaryPrimitives.ReadUInt16LittleEndian(b.AsSpan(16)),
                MaxMP = BinaryPrimitives.ReadUInt16LittleEndian(b.AsSpan(18)),
                Reserved2 = BinaryPrimitives.ReadInt16LittleEndian(b.AsSpan(20)),
                ExpCount = b[22],
                ExpMaxCount = b[23],
                Exp = BinaryPrimitives.ReadInt32LittleEndian(b.AsSpan(24)),
                MaxExp = BinaryPrimitives.ReadInt32LittleEndian(b.AsSpan(28)),
                Weight = BinaryPrimitives.ReadUInt16LittleEndian(b.AsSpan(32)),
                MaxWeight = BinaryPrimitives.ReadUInt16LittleEndian(b.AsSpan(34)),
                WearWeight = b[36],
                MaxWearWeight = b[37],
                HandWeight = b[38],
                MaxHandWeight = b[39],
            };
        }

        /// <summary>全零 = 错解形态（真实角色 `Level >= 1` 且 1 级就有非零 HP/Weight）。</summary>
        public static bool IsAllZero(Ability a)
        {
            return a != null
                && a.Level == 0 && a.Reserved1 == 0
                && a.AC == 0 && a.MAC == 0 && a.DC == 0 && a.MC == 0 && a.SC == 0
                && a.HP == 0 && a.MP == 0 && a.MaxHP == 0 && a.MaxMP == 0
                && a.Reserved2 == 0 && a.ExpCount == 0 && a.ExpMaxCount == 0
                && a.Exp == 0 && a.MaxExp == 0
                && a.Weight == 0 && a.MaxWeight == 0
                && a.WearWeight == 0 && a.MaxWearWeight == 0
                && a.HandWeight == 0 && a.MaxHandWeight == 0;
        }
    }
}
