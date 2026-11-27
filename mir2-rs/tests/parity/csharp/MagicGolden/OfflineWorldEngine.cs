// 卡 openmir2-magic-parity §6 / 卡 openmir2-mg24-slave-design：
// 离线 harness 的最小 IWorldEngine 替身——只为 17/41/42 的 `MakeSlave` 服务。
//
// **只有 `RegenMonsterByName` 是真实现**；其余 40 条声明一律 `NotImplementedException`，
// 目的不是"能用"，而是"**一旦某任务路径用了未实现的成员，立刻炸**"（比静默返回 null 安全）。
// 若将来发现某成员被间接调用（如 `ActorMgr`/地图相关），把它从 throw 改成"最小真值"并在卡里点名。
//
// 配置不在此处读取：`Skeleton*/Dragon*/Angel` 由 `SystemShare.Config`（部署 `Server.conf`）提供，
// 调用方 `MakeSlave` 传入的 `sMonName` 即最终名字。
using SystemModule;
using SystemModule.Actors;
using SystemModule.Data;
using SystemModule.Enums;
using SystemModule.Maps;
using SystemModule.SubSystem;
using OpenMir2.Data;
using OpenMir2.Packets.ClientPackets;

namespace MagicGolden
{
    public sealed class OfflineWorldEngine : IWorldEngine
    {
        private static System.NotImplementedException Unused(string name) =>
            new System.NotImplementedException("offline harness: unused IWorldEngine member " + name);

        /// <summary>
        /// **唯一真实现**：返回一个可用的 `MonsterObject`（`MakeSlave` 会强转为 `MonsterObject`）。
        /// 只设置定位/身份；`Abil.Level` 与 `WAbil.HP/MaxHP` 由 `MakeSlave` 之后写入。
        /// </summary>
        /// <summary>调用计数：cap 支断言"未生成实体"的观测面（0 次 ⇒ 没有新实体）。</summary>
        public static int RegenCalls;

        public IActor RegenMonsterByName(string sMap, short nX, short nY, string sMonName)
        {
            RegenCalls++;

            // **MG-24（撤 DEV-MG-24-RNGCOUNT 登记）**：按 race 构造 **oracle 真子类**——ctor 链
            // 自然执行生产抽（WhiteSkeleton → AtMonster 的 `SearchTime = Random(1500)+1500`，
            // `AtMonster.cs:10`），与 oracle 生产 CreateMonster 同构；构造链零 World/表副作用
            // （仅字段写入 + GetTickCount/RandomNumber，探针已核）。
            var __raceEnv = System.Environment.GetEnvironmentVariable("MAGICGOLDEN_STUB_MONSTER_RACE");
            var __race = (!string.IsNullOrWhiteSpace(__raceEnv) && byte.TryParse(__raceEnv, out var __rr)) ? __rr : (byte)100;
            M2Server.Monster.MonsterObject mon = __race switch
            {
                100 => new M2Server.Monster.Monsters.WhiteSkeleton(),
                _ => new M2Server.Monster.MonsterObject(),
            };
            mon.ChrName = sMonName;
            mon.CurrX = nX;
            mon.CurrY = nY;
            // **与生产 `MonsterTableSpawner::spawn_monster` 同源语义**：生产按 `MonsterInfo.hp` 同时写
            // `hp` 与 `max_hp`（⇒ `hp == max_hp`），`MakeSlave` 的半补因此是 no-op。
            // 离线 harness 没有怪物表，HP 由 `MAGICGOLDEN_STUB_MONSTER_HP`（缺省 17）给定，
            // **并原值写进 golden 的 `stub_monster_hp`**，让"这个数来自替身常量"可被看见。
            ushort stubHp = 17;
            var env = System.Environment.GetEnvironmentVariable("MAGICGOLDEN_STUB_MONSTER_HP");
            if (!string.IsNullOrWhiteSpace(env) && ushort.TryParse(env, out var v))
            {
                stubHp = v;
            }
            // **Race = 100（WhiteSkeleton，真身）**：`apply_slave_level_abilitys` 的 White/Elf 分支
            // 会先抬高 `max_hp`（`chp = hp + round(hp*0.4)*slaveExpLevel`）再走半补 ⇒ 真实面是 hp≠max_hp。
            // SINSU 神兽传 113（ElfMonster 族 ctor 无额外抽）。
            mon.Race = __race;
            // **Abil 来自"怪物信息"**：生产里 `MonsterInfo.hp` 写进 `Abil`，`RecalcAbilitys` 再克隆到 `WAbil`。
            // 替身必须**显式设 `Abil.MaxHP`**（否则取 `MonsterObject` ctor 缺省 15，与真怪无关）——
            // `ApplySlaveLevelAbilitys` 的 `chp` 基数读的正是 `Abil.MaxHP`。
            mon.Abil.MaxHP = stubHp;
            mon.Abil.HP = stubHp;
            mon.WAbil.HP = stubHp;
            mon.WAbil.MaxHP = stubHp;
            // **MG-24**：生产 CreateMonster 的两处 spawner 点抽（`WorldServer.MonGen.cs:740/:745`）
            // —— `cert.Dir = RandomByte(8)`（无条件）与 `Random(100) < CoolEyeCode`（**抽无条件、
            // 比对条件**：CoolEyeCode=0 也照抽、恒不置位）。只对齐 RNG 流，不在替身落 Dir/CoolEye
            // 状态（帧面不涉）。
            mon.Dir = M2Server.M2Share.RandomNumber.RandomByte(8);
            if (M2Server.M2Share.RandomNumber.Random(100) < mon.CoolEyeCode)
            {
                mon.CoolEye = true;
            }
            return mon;
        }

        // —— 以下为"读得到就返回最小真值"的只读成员（常被外围代码顺手调用）——
        public IPlayerActor GetPlayObject(string chrName) => null;
        public System.Collections.Generic.IEnumerable<IPlayerActor> GetPlayObjects() =>
            System.Array.Empty<IPlayerActor>();
        public int MonsterCount => 0;
        public int MagicCount => 0;
        public int OfflinePlayCount => 0;
        public int MonGenCount => 0;
        public int PlayObjectCount => 0;
        public int OnlinePlayObject => 0;
        public int GetMapOfRangeHumanCount(IEnvirnoment envir, int nX, int nY, int nRange) => 0;
        public int GetMapHuman(string mapName) => 0;
        public int GetMonstersZenTime(int time) => time;
        public IMerchant FindMerchant(int npcId) => null;
        public INormNpc FindNpc(int npcId) => null;
        public MagicInfo FindMagic(int nMagIdx) => null;
        public MagicInfo FindMagic(string sMagicName) => null;
        public bool FindOtherServerUser(string mapName, ref int srvIdx) => false;
        public int GetPlayExpireTime(string account) => 0;

        // —— 其余：一律炸（用到即暴露）——
        public void Initialize() => throw Unused(nameof(Initialize));
        public void InitializationMonsterThread() => throw Unused(nameof(InitializationMonsterThread));
        public void InitializeMonster() => throw Unused(nameof(InitializeMonster));
        public void ProcessUserMessage(IPlayerActor playObject, CommandMessage defMsg, string buff) => throw Unused(nameof(ProcessUserMessage));
        public void CryCry(short wIdent, IEnvirnoment pMap, int nX, int nY, int nWide, byte btFColor, byte btBColor, string sMsg) => throw Unused(nameof(CryCry));
        public void GetMapRageHuman(IEnvirnoment envir, int nRageX, int nRageY, int nRage, ref System.Collections.Generic.IList<IPlayerActor> list, bool botPlay = false) => throw Unused(nameof(GetMapRageHuman));
        public void SendBroadCastMsg(string sMsg, MsgType msgType) => throw Unused(nameof(SendBroadCastMsg));
        public void SendBroadCastMsgExt(string sMsg, MsgType msgType) => throw Unused(nameof(SendBroadCastMsgExt));
        public System.Collections.Generic.IList<IMerchant> MerchantList { get; set; } = new System.Collections.Generic.List<IMerchant>();
        public void AddMerchant(IMerchant merchant) => throw Unused(nameof(AddMerchant));
        public void AddQuestNpc(INormNpc normNpc) => throw Unused(nameof(AddQuestNpc));
        public System.Collections.Generic.IList<AdminInfo> AdminList { get; set; } = new System.Collections.Generic.List<AdminInfo>();
        public void SetPlayExpireTime(string account, int expiredTime) => throw Unused(nameof(SetPlayExpireTime));
        public void AccountExpired(string account) => throw Unused(nameof(AccountExpired));
        public void SendServerGroupMsg(int nCode, int nServerIdx, string sMsg) => throw Unused(nameof(SendServerGroupMsg));
        public int GetMapMonster(IEnvirnoment envir, System.Collections.Generic.IList<IActor> list) => throw Unused(nameof(GetMapMonster));
        public void AddMonGenList(MonGenInfo monGenInfo) => throw Unused(nameof(AddMonGenList));
        public bool CheckMonGenInfoThreadMap(int threadId) => throw Unused(nameof(CheckMonGenInfoThreadMap));
        public void AddMonGenInfoThreadMap(int threadId, MonGenInfo monGenInfo) => throw Unused(nameof(AddMonGenInfoThreadMap));
        public void CreateMonGenInfoThreadMap(int threadId, System.Collections.Generic.IList<MonGenInfo> monGenInfo) => throw Unused(nameof(CreateMonGenInfoThreadMap));
        public void ClearMonsterList() => throw Unused(nameof(ClearMonsterList));
        public void AddMonsterList(MonsterInfo monsterInfo) => throw Unused(nameof(AddMonsterList));
        public void SwitchMagicList() => throw Unused(nameof(SwitchMagicList));
        public void AddMagicList(MagicInfo magicInfo) => throw Unused(nameof(AddMagicList));
        public void Run() => throw Unused(nameof(Run));
        public void ProcessNpcs() => throw Unused(nameof(ProcessNpcs));
        public void ProcessMerchants() => throw Unused(nameof(ProcessMerchants));
        public void ProcessHumans() => throw Unused(nameof(ProcessHumans));
        public void AddUserOpenInfo(UserOpenInfo UserOpenInfo) => throw Unused(nameof(AddUserOpenInfo));
        public void OpenDoor(IEnvirnoment envir, int nX, int nY) => throw Unused(nameof(OpenDoor));
        public void CloseDoor(IEnvirnoment envir, MapDoor door) => throw Unused(nameof(CloseDoor));
    }
}
