// 卡 openmir2-golden-harness-mapmgr：离线 harness 的最小 IMapSystem 替身。
//
// **只有 `FindMap` 是真实现**（返回 harness 已 LoadMapData 的那个 Envirnoment）；其余 9 成员一律
// `NotImplementedException`（触达即炸，比静默假值安全，同 OfflineWorldEngine 约定）。
// 明确不做：不建图 / 不载 MapInfo.txt / 不刷怪 / 不触发地图事件 / 无城门安全区。
// 边界：SpaceMove 过 FindMap 后还会 DeleteFromMap/AddMapObject/SpaceMoveGetRandXY/SendMsg ——
// 替身只保 FindMap 非 null；对象入图/落点由驱动侧（pre-slave 设 Envir=env）保证。
using System.Collections.Generic;
using SystemModule.Actors;
using SystemModule.Maps;
using SystemModule.SubSystem;
using OpenMir2.Data;

namespace MagicGolden
{
    public sealed class OfflineMapSystem : IMapSystem
    {
        private readonly IEnvirnoment _env;
        public OfflineMapSystem(IEnvirnoment env) { _env = env; }
        private static System.NotImplementedException Unused(string n) =>
            new System.NotImplementedException("offline harness: unused IMapSystem member " + n);

        /// <summary>唯一真实现：名字匹配已加载 env 的 MapName（同一实例传入）⇒ 返回 env；否则 null（oracle「找不到=null」语义，不静默给假图）。</summary>
        public IEnvirnoment FindMap(string sMapName) =>
            (sMapName != null && string.Equals(sMapName, _env.MapName, System.StringComparison.OrdinalIgnoreCase))
                ? _env
                : null;

        public IList<IEnvirnoment> Maps => throw Unused(nameof(Maps));
        public void AddMapInfo(string sMapName, string sMapDesc, byte nServerNumber, MapInfoFlag mapFlag, IMerchant questNpc) => throw Unused(nameof(AddMapInfo));
        public bool AddMapRoute(string sSMapNo, int nSMapX, int nSMapY, string sDMapNo, int nDMapX, int nDMapY) => throw Unused(nameof(AddMapRoute));
        public IList<IEnvirnoment> GetDoorMapList() => throw Unused(nameof(GetDoorMapList));
        public IEnvirnoment GetMapInfo(int nServerIdx, string sMapName) => throw Unused(nameof(GetMapInfo));
        public int GetMapOfServerIndex(string sMapName) => throw Unused(nameof(GetMapOfServerIndex));
        public IList<IEnvirnoment> GetMineMaps() => throw Unused(nameof(GetMineMaps));
        public void LoadMapDoor() => throw Unused(nameof(LoadMapDoor));
        public void MakeSafePkZone() => throw Unused(nameof(MakeSafePkZone));
    }
}
