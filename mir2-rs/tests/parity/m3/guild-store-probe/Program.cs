// guild-store-probe —— 用 C# oracle 真身实测「行会文件加载/落盘」向量（M3⑧-3）。
//
// 为什么要探针：`crates/data` 的行会文件编解码必须与 oracle 逐字节 / 逐字段相同，而 oracle 这一段
// 有一堆**读源码极易漏掉**的行为：
//   * `StringList.SaveToFile` 先 `GetEncoding(file)` → `new FileStream(file, FileMode.Open)`
//     ⇒ **目标文件不存在时抛异常**，被 `GuildInfo.SaveGuildFile` / `GuildManager.SaveGuildList`
//     的 try/catch 吞掉 ⇒ **不会新建文件**（新行会的 `<名>.txt` 实际写不出去）。
//   * `GuildConf` 只 `Load()` 从不 `Save()` ⇒ `.ini` 五键**永远不落盘**；`.ini` 不存在时
//     ctor 里的 `Load()` 只 `File.Create` 一个**空文件**。
//   * `LoadGuildFile` 的行会战分支把剩余时长读进**上一行残留的 `s20`**（本段从不赋值）
//     ⇒ 加载出来的 `WarTime` 恒为 `0`。
//   * 联盟段 `//GuildAllList.Add(guild)` 被注释 ⇒ 加载出来的联盟表**恒空**（内容被丢弃）。
//   * 职名只在 `s24.Length > 30` 时才截到 `GuildRankNameLen`。
// 产物 = `vectors/guild_store_vectors.tsv`（group/key/value）+ `vectors/guild_file_bytes.tsv`
// （scenario/sha256/hex）。Rust 侧用例 `crates/data/tests/guild_store_vectors.rs` 逐字段核对。
//
// 用法（**在仓库根运行**）：
//   dotnet run --project mir2-rs/tests/parity/m3/guild-store-probe -c Release -- <向量目录>
//
// 只读纪律：只在向量目录下建 `work/` 临时子树；不连 MySQL、不写现网文件、不发起 socket。

using System;
using System.Collections.Generic;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using M2Server;
using M2Server.Guild;
using OpenMir2;
using OpenMir2.Data;
using OpenMir2.Packets.ClientPackets;
using Serilog;
using SystemModule;
using SystemModule.Actors;
using SystemModule.Castles;
using SystemModule.Data;
using SystemModule.Enums;
using SystemModule.Maps;
using SystemModule.SubSystem;

namespace GuildStoreProbe;

internal static class Program
{
    private static readonly List<string> Rows = new();
    private static readonly List<string> ByteRows = new();

    private static string _work;
    private static string _guildsDir;
    private static string _listFile;

    private static void Rec(string group, string key, object value)
    {
        Rows.Add($"{group}\t{key}\t{value}");
    }

    private static void RecBytes(string scenario, byte[] bytes)
    {
        ByteRows.Add($"{scenario}\t{Sha256Hex(bytes)}\t{Hex(bytes)}");
    }

    private static string Hex(byte[] b)
    {
        var sb = new StringBuilder(b.Length * 2);
        foreach (byte x in b)
        {
            sb.Append(x.ToString("x2"));
        }
        return sb.ToString();
    }

    private static string Sha256Hex(byte[] b)
    {
        using var h = SHA256.Create();
        byte[] d = h.ComputeHash(b);
        var sb = new StringBuilder(64);
        foreach (byte x in d)
        {
            sb.Append(x.ToString("x2"));
        }
        return sb.ToString();
    }

    private static byte[] Gbk(string s) => Encoding.GetEncoding("gb2312").GetBytes(s);

    private static void WriteGbk(string path, string s)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        File.WriteAllBytes(path, Gbk(s));
    }

    private static long SizeOf(string path) => File.Exists(path) ? new FileInfo(path).Length : -1;

    private static string GPath(string name) => Path.Combine(_guildsDir, name);

    private static int Main(string[] args)
    {
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        // `M2Share` 静态构造会经 `LogService` 打日志 ⇒ 先装空 sink。
        LogService.Logger = new LoggerConfiguration().CreateLogger();

        if (args.Length < 1)
        {
            Console.Error.WriteLine("usage: guild-store-probe <vectors-dir>");
            return 2;
        }
        string outDir = Path.GetFullPath(args[0]);
        string vecDir = Path.Combine(outDir, "vectors");
        Directory.CreateDirectory(vecDir);
        _work = Path.Combine(outDir, "work");
        if (Directory.Exists(_work))
        {
            Directory.Delete(_work, true);
        }
        _guildsDir = Path.Combine(_work, "GuildBase", "Guilds") + Path.DirectorySeparatorChar;
        _listFile = Path.Combine(_work, "GuildBase", "GuildList.txt");
        Directory.CreateDirectory(_guildsDir);

        SystemShare.Config.GuildDir = _guildsDir;
        SystemShare.Config.GuildFile = _listFile;
        M2Share.ServerIndex = 0;
        var stub = new GuildStub();
        SystemShare.GuildMgr = stub;
        SystemShare.WorldEngine = new WorldStub();

        Config();
        ScenarioA();
        ScenarioB();
        ScenarioC(stub);
        ScenarioD();
        ScenarioE();
        ScenarioF();
        ScenarioG();
        ScenarioH();

        File.WriteAllText(Path.Combine(vecDir, "guild_store_vectors.tsv"),
            "# group\tkey\tvalue（oracle 真身实测，GBK/逐字节口径）\n" + string.Join("\n", Rows) + "\n", new UTF8Encoding(false));
        File.WriteAllText(Path.Combine(vecDir, "guild_file_bytes.tsv"),
            "# scenario\tsha256\thex\n" + string.Join("\n", ByteRows) + "\n", new UTF8Encoding(false));
        Console.WriteLine($"guild-store-probe: wrote {Rows.Count} fields / {ByteRows.Count} byte vectors -> {vecDir}");
        return 0;
    }

    /// 段头文案与职名长度：oracle `ServerConf.LoadConfig` 对这四个段头**无读取点** ⇒ 恒为初值。
    private static void Config()
    {
        Rec("config", "guild_notice", SystemShare.Config.GuildNotice);
        Rec("config", "guild_war_header", SystemShare.Config.GuildWar);
        Rec("config", "guild_all", SystemShare.Config.GuildAll);
        Rec("config", "guild_member", SystemShare.Config.GuildMember);
        Rec("config", "guild_chief", SystemShare.Config.GuildChief);
        Rec("config", "guild_rank_name_len", SystemShare.Config.GuildRankNameLen);
        Rec("config", "guild_member_max_limit", SystemShare.Config.GuildMemberMaxLimit);
    }

    /// A：新建行会 → `SaveGuildInfoFile` 时 `<名>.txt` 尚不存在 ⇒ `StringList.SaveToFile` 抛错被吞
    /// ⇒ **不落盘**；`.ini` 由 ctor 的 `Load()` `File.Create` 成**空文件**。
    private static void ScenarioA()
    {
        const string name = "甲会";
        try
        {
            var g = new GuildInfo(name);
            bool ok = g.SetGuildInfo("甲会长");
            Rec("A_save_missing_target", "set_guild_info", ok);
            Rec("A_save_missing_target", "txt_exists_after_save", File.Exists(GPath(name + ".txt")));
            Rec("A_save_missing_target", "ini_exists_after_ctor", File.Exists(GPath(name + ".ini")));
            Rec("A_save_missing_target", "ini_size", SizeOf(GPath(name + ".ini")));
            Rec("A_save_missing_target", "rank_count", g.RankList.Count);
            Rec("A_save_missing_target", "rank_0_no", g.RankList[0].RankNo);
            Rec("A_save_missing_target", "rank_0_name", g.RankList[0].RankName);
            Rec("A_save_missing_target", "rank_0_member_0", g.RankList[0].MemberList[0].MemberName);
        }
        catch (Exception e)
        {
            Rec("A_save_missing_target", "error", e.GetType().Name + ":" + e.Message);
        }
    }

    /// B：预建**空** `<名>.txt` 后再存 ⇒ 落盘（逐字节向量）；`.ini` 仍为 0 字节（`GuildConf` 从不 `Save()`）。
    private static void ScenarioB()
    {
        const string name = "甲会";
        File.WriteAllBytes(GPath(name + ".txt"), Array.Empty<byte>());
        try
        {
            var g = new GuildInfo(name);
            g.NoticeList.Add("公告一");
            g.NoticeList.Add("公告二");
            g.RankList.Add(new GuildRank
            {
                RankNo = 99,
                RankName = "成员",
                MemberList = new List<GuildMember>
                {
                    new GuildMember { MemberName = "乙" },
                    new GuildMember { MemberName = "丙" }
                }
            });
            // 已到期行会战（n14 <= 0）⇒ oracle 跳过不写；顺带避开 GetTickCount 抖动。
            g.GuildWarList.Add(new WarGuild
            {
                Guild = null,
                WarTick = HUtil32.GetTickCount() - 100000,
                WarTime = 0
            });
            g.SaveGuildInfoFile();
            byte[] bytes = File.ReadAllBytes(GPath(name + ".txt"));
            RecBytes("B_save_precreated_txt", bytes);
            Rec("B_save_precreated_txt", "len", bytes.Length);
            Rec("B_save_precreated_txt", "ini_size", SizeOf(GPath(name + ".ini")));
        }
        catch (Exception e)
        {
            Rec("B_save_precreated_txt", "error", e.GetType().Name + ":" + e.Message);
        }
    }

    /// C：四段解析（`+`/`#`/`;`/空行）+ 三个 quirk（WarTime 恒 0 / 联盟被丢弃 / `.ini` 五键读侧）。
    private static void ScenarioC(GuildStub stub)
    {
        const string name = "加载会";
        try
        {
            var warTarget = new GuildInfo("乙会");
            stub.Register("乙会", warTarget);

            WriteGbk(GPath(name + ".ini"),
                "[Guild]\nGuildName=加载会\nBuildPoint=42\nAurae=7\nStability=3\nFlourishing=9\nChiefItemCount=5\n");
            WriteGbk(GPath(name + ".txt"),
                "; 分号注释行\n" +
                SystemShare.Config.GuildNotice + "\n+第一行公告\n+第二行公告\n" +
                " \n" +
                SystemShare.Config.GuildWar + "\n+乙会 123\n" +
                SystemShare.Config.GuildAll + "\n+丙会\n" +
                SystemShare.Config.GuildMember + "\n#1 掌门人\n+甲会长\n#99 成员\n+乙,丙\n");

            var g = new GuildInfo(name);
            bool ok = g.LoadGuild();
            Rec("C_load", "load_returns", ok);
            Rec("C_load", "build_point", g.BuildPoint);
            Rec("C_load", "aurae", g.Aurae);
            Rec("C_load", "stability", g.Stability);
            Rec("C_load", "flourishing", g.Flourishing);
            Rec("C_load", "chief_item_count", g.ChiefItemCount);
            Rec("C_load", "notices_count", g.NoticeList.Count);
            for (int i = 0; i < g.NoticeList.Count; i++)
            {
                Rec("C_load", $"notice_{i}", g.NoticeList[i]);
            }
            Rec("C_load", "war_count", g.GuildWarList.Count);
            if (g.GuildWarList.Count > 0)
            {
                Rec("C_load", "war_0_name", g.GuildWarList[0].Guild.GuildName);
                Rec("C_load", "war_0_time", g.GuildWarList[0].WarTime);
                Rec("C_load", "war_0_tick_nonzero", g.GuildWarList[0].WarTick != 0);
            }
            Rec("C_load", "ally_count", g.GuildAllList.Count);
            Rec("C_load", "rank_count", g.RankList.Count);
            for (int i = 0; i < g.RankList.Count; i++)
            {
                GuildRank r = g.RankList[i];
                Rec("C_load", $"rank_{i}_no", r.RankNo);
                Rec("C_load", $"rank_{i}_name", r.RankName);
                Rec("C_load", $"rank_{i}_member_count", r.MemberList.Count);
                for (int j = 0; j < r.MemberList.Count; j++)
                {
                    Rec("C_load", $"rank_{i}_member_{j}", r.MemberList[j].MemberName);
                }
            }

            var missing = new GuildInfo("无文件会");
            Rec("C_missing", "load_returns", missing.LoadGuild());
            Rec("C_missing", "ini_size", SizeOf(GPath("无文件会.ini")));
        }
        catch (Exception e)
        {
            Rec("C_load", "error", e.GetType().Name + ":" + e.Message);
        }
    }

    /// D：`GuildManager.LoadGuildInfo` —— 逐行读会名 + 反向删除加载失败者 + 回写 GuildList.txt。
    private static void ScenarioD()
    {
        try
        {
            M2Share.ServerIndex = 0;
            SystemShare.GuildMgr = new GuildManager();
            WriteGbk(_listFile, "甲会\n\n   \n加载会\n幽灵会\n");
            SystemShare.GuildMgr.LoadGuildInfo();
            Rec("D_load_guild_info", "find_甲会", SystemShare.GuildMgr.FindGuild("甲会") != null);
            Rec("D_load_guild_info", "find_加载会", SystemShare.GuildMgr.FindGuild("加载会") != null);
            Rec("D_load_guild_info", "find_幽灵会", SystemShare.GuildMgr.FindGuild("幽灵会") != null);
            Rec("D_load_guild_info", "ini_甲会_exists", File.Exists(GPath("甲会.ini")));
            Rec("D_load_guild_info", "ini_幽灵会_exists", File.Exists(GPath("幽灵会.ini")));
            Rec("D_load_guild_info", "ini_幽灵会_size", SizeOf(GPath("幽灵会.ini")));
            RecBytes("D_load_guild_info_list_rewritten", File.ReadAllBytes(_listFile));
        }
        catch (Exception e)
        {
            Rec("D_load_guild_info", "error", e.GetType().Name + ":" + e.Message);
        }
    }

    /// E：`M2Share.ServerIndex != 0` ⇒ `SaveGuildList` 直接 return（回写 no-op，文件保持原样）。
    private static void ScenarioE()
    {
        try
        {
            M2Share.ServerIndex = 1;
            SystemShare.GuildMgr = new GuildManager();
            WriteGbk(_listFile, "甲会\n幽灵会\n");
            SystemShare.GuildMgr.LoadGuildInfo();
            Rec("E_server_index_nonzero", "find_幽灵会", SystemShare.GuildMgr.FindGuild("幽灵会") != null);
            RecBytes("E_server_index_nonzero_list_unchanged", File.ReadAllBytes(_listFile));
        }
        catch (Exception e)
        {
            Rec("E_server_index_nonzero", "error", e.GetType().Name + ":" + e.Message);
        }
        finally
        {
            M2Share.ServerIndex = 0;
        }
    }

    /// F：`GuildFile` 不存在 ⇒ 记错误、会表为空、**不建文件**（fail-closed）。
    private static void ScenarioF()
    {
        try
        {
            SystemShare.GuildMgr = new GuildManager();
            if (File.Exists(_listFile))
            {
                File.Delete(_listFile);
            }
            SystemShare.GuildMgr.LoadGuildInfo();
            Rec("F_missing_list_file", "list_file_still_absent", !File.Exists(_listFile));
            Rec("F_missing_list_file", "find_甲会", SystemShare.GuildMgr.FindGuild("甲会") != null);
            Rec("F_missing_list_file", "member_of_甲会", SystemShare.GuildMgr.MemberOfGuild("甲会") != null);
        }
        catch (Exception e)
        {
            Rec("F_missing_list_file", "error", e.GetType().Name + ":" + e.Message);
        }
    }

    /// G：职名截断只在 `Length > 30` 时发生（截到 `GuildRankNameLen`）。
    private static void ScenarioG()
    {
        const string name = "截断会";
        try
        {
            string longName = new string('甲', 31);
            string shortName = new string('乙', 30);
            WriteGbk(GPath(name + ".txt"),
                SystemShare.Config.GuildMember + "\n#7 " + longName + "\n+某甲\n#8 " + shortName + "\n+某乙\n");
            var g = new GuildInfo(name);
            bool ok = g.LoadGuild();
            Rec("G_truncate", "load_returns", ok);
            Rec("G_truncate", "rank_count", g.RankList.Count);
            for (int i = 0; i < g.RankList.Count; i++)
            {
                Rec("G_truncate", $"rank_{i}_no", g.RankList[i].RankNo);
                Rec("G_truncate", $"rank_{i}_name_len", g.RankList[i].RankName.Length);
            }
        }
        catch (Exception e)
        {
            Rec("G_truncate", "error", e.GetType().Name + ":" + e.Message);
        }
    }

    /// H：`HUtil32.GetValidStr3(str, ref dest, char[])` 的切分口径（`Split(div, 2, RemoveEmptyEntries)`），
    /// 这是四段解析每一步都用的分词器，单独锁死。
    private static void ScenarioH()
    {
        string[] inputs =
        {
            "a b", "  a  b", "a,,b", ",,a,,b,,", "a", "", "a  ", "  ", "甲 乙 丙", "x  y,,z"
        };
        var split = new[] { ' ', ',' };
        for (int i = 0; i < inputs.Length; i++)
        {
            string s1c = string.Empty;
            string rest = HUtil32.GetValidStr3(inputs[i], ref s1c, split);
            Rec("H_get_valid_str3", $"case_{i}_in", inputs[i].Replace(" ", "<SP>"));
            Rec("H_get_valid_str3", $"case_{i}_dest", s1c.Replace(" ", "<SP>"));
            Rec("H_get_valid_str3", $"case_{i}_rest", rest.Replace(" ", "<SP>"));
        }
    }

    /// `IGuildSystem` 替身：只让 `FindGuild` 命中我们注册的行会（避免 `GuildManager.AddGuild` 的落盘副作用）。
    private sealed class GuildStub : IGuildSystem
    {
        private readonly Dictionary<string, IGuild> _map = new(StringComparer.Ordinal);

        public void Register(string name, IGuild g) => _map[name] = g;

        public bool AddGuild(string sGuildName, string sChief) => false;

        public void ClearGuildInf() { }

        public bool DelGuild(string sGuildName) => false;

        public IGuild FindGuild(string sGuildName) => _map.TryGetValue(sGuildName, out IGuild g) ? g : null;

        public void LoadGuildInfo() { }

        public IGuild MemberOfGuild(string sName) => null;

        public void Run() { }
    }

    /// `IWorldEngine` 替身：只让 `GetPlayObject` 返回 null（行会成员段的 `PlayObject` 回填）。
    private sealed class WorldStub : IWorldEngine
    {
        public void Initialize() => throw new NotImplementedException();

        public void InitializationMonsterThread() => throw new NotImplementedException();

        public void InitializeMonster() => throw new NotImplementedException();

        public IPlayerActor GetPlayObject(string chrName) => null;

        public IEnumerable<IPlayerActor> GetPlayObjects() => Array.Empty<IPlayerActor>();

        public int MonsterCount => 0;

        public int MagicCount => 0;

        public int OfflinePlayCount => 0;

        public int MonGenCount => 0;

        public void ProcessUserMessage(IPlayerActor playObject, CommandMessage defMsg, string buff) => throw new NotImplementedException();

        public void CryCry(short wIdent, IEnvirnoment pMap, int nX, int nY, int nWide, byte btFColor, byte btBColor, string sMsg) => throw new NotImplementedException();

        public void GetMapRageHuman(IEnvirnoment envir, int nRageX, int nRageY, int nRage, ref IList<IPlayerActor> list, bool botPlay = false) => throw new NotImplementedException();

        public int GetMapOfRangeHumanCount(IEnvirnoment envir, int nX, int nY, int nRange) => throw new NotImplementedException();

        public IActor RegenMonsterByName(string sMap, short nX, short nY, string sMonName) => throw new NotImplementedException();

        public void SendBroadCastMsg(string sMsg, MsgType msgType) => throw new NotImplementedException();

        public void SendBroadCastMsgExt(string sMsg, MsgType msgType) => throw new NotImplementedException();

        public IMerchant FindMerchant(int npcId) => throw new NotImplementedException();

        public INormNpc FindNpc(int npcId) => throw new NotImplementedException();

        public MagicInfo FindMagic(int nMagIdx) => throw new NotImplementedException();

        public MagicInfo FindMagic(string sMagicName) => throw new NotImplementedException();

        public IList<IMerchant> MerchantList { get; set; }

        public void AddMerchant(IMerchant merchant) => throw new NotImplementedException();

        public void AddQuestNpc(INormNpc normNpc) => throw new NotImplementedException();

        public IList<AdminInfo> AdminList { get; set; }

        public int PlayObjectCount => 0;

        public int OnlinePlayObject => 0;

        public int GetPlayExpireTime(string account) => throw new NotImplementedException();

        public void SetPlayExpireTime(string account, int expiredTime) => throw new NotImplementedException();

        public void AccountExpired(string account) => throw new NotImplementedException();

        public void SendServerGroupMsg(int nCode, int nServerIdx, string sMsg) => throw new NotImplementedException();

        public bool FindOtherServerUser(string mapName, ref int srvIdx) => throw new NotImplementedException();

        public int GetMonstersZenTime(int time) => throw new NotImplementedException();

        public int GetMapHuman(string mapName) => throw new NotImplementedException();

        public int GetMapMonster(IEnvirnoment envir, IList<IActor> list) => throw new NotImplementedException();

        public void AddMonGenList(MonGenInfo monGenInfo) => throw new NotImplementedException();

        public bool CheckMonGenInfoThreadMap(int threadId) => throw new NotImplementedException();

        public void AddMonGenInfoThreadMap(int threadId, MonGenInfo monGenInfo) => throw new NotImplementedException();

        public void CreateMonGenInfoThreadMap(int threadId, IList<MonGenInfo> monGenInfo) => throw new NotImplementedException();

        public void ClearMonsterList() => throw new NotImplementedException();

        public void AddMonsterList(MonsterInfo monsterInfo) => throw new NotImplementedException();

        public void SwitchMagicList() => throw new NotImplementedException();

        public void AddMagicList(MagicInfo magicInfo) => throw new NotImplementedException();

        public void Run() => throw new NotImplementedException();

        public void ProcessNpcs() => throw new NotImplementedException();

        public void ProcessMerchants() => throw new NotImplementedException();

        public void ProcessHumans() => throw new NotImplementedException();

        public void AddUserOpenInfo(UserOpenInfo UserOpenInfo) => throw new NotImplementedException();

        public void OpenDoor(IEnvirnoment envir, int nX, int nY) => throw new NotImplementedException();

        public void CloseDoor(IEnvirnoment envir, MapDoor door) => throw new NotImplementedException();
    }
}
