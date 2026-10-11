// Server.conf 读取探针：`ConfigFile` 是链接进来的**真实实现**，这里只按
// `src/Modules/SystemModule/Conf/ServerConf.cs` 的**同一批读取点**（节/键/默认值/类型）
// 把值读出来，不动任何写回逻辑（本探针只读；Server.conf 传夹具副本）。

using OpenMir2.Common;

namespace EnvirGolden
{
    /// <summary>`src/Modules/SystemModule/Conf/ServerConf.cs` 的读取子集。</summary>
    public sealed class ProbeConf : ConfigFile
    {
        public ProbeConf(string fileName) : base(fileName)
        {
            Load();
        }

        // ServerConf.cs:108-109（ReadWriteString）
        public string S(string section, string key, string def) => ReadWriteString(section, key, def);

        // ServerConf.cs:172-173（ReadWriteByte / ReadWriteUInt16）
        public byte U8(string section, string key, byte def) => ReadWriteByte(section, key, def);

        public ushort U16(string section, string key, ushort def) => ReadWriteUInt16(section, key, def);

        // ServerConf.cs:173（ReadWriteInteger）
        public int I(string section, string key, int def) => ReadWriteInteger(section, key, def);
    }
}
