// M2 世界/AOI 金标准：桩类型 + **逐字拷贝**的 oracle 函数体。
// 拷贝来源与行号标注在各函数上方；函数体一行未改（只把成员外壳换成桩）。

using System;
using System.Collections.Generic;

namespace WorldGolden
{
    /// <summary>虚拟时钟：C# 侧读 HUtil32.GetTickCount()，这里桩成由驱动推进的毫秒时钟。</summary>
    public static class VirtualClock
    {
        public static long Now;

        public static void Advance(long ms) => Now += ms;
    }

    /// <summary>桩：HUtil32.GetTickCount()（oracle 里返回墙钟毫秒）。</summary>
    public static class HUtil32
    {
        public static int GetTickCount() => (int)VirtualClock.Now;
    }

    public static class LogService
    {
        public static void Error(string s) { /* 桩：对拍不需要日志 */ }
        public static void Warn(string s) { }
    }

    /// <summary>src/Modules/SystemModule/Data/CellType.cs（逐字拷贝）。</summary>
    public enum CellType : byte
    {
        Event = 1,
        Play = 2,
        Item = 3,
        MapRoute = 4,
        MapEvent = 5,
        Door = 6,
        Roon = 7,
        Merchant = 8,
        Monster = 9,
        SavleMonster = 10,
        CastleDoor = 11
    }

    /// <summary>src/Modules/SystemModule/Data/CellObject.cs（逐字拷贝，去掉属性包装）。</summary>
    public struct CellObject
    {
        public int CellObjId;
        public CellType CellType;
        public bool ActorObject;
        public int AddTime;
    }

    public enum CellAttribute : byte
    {
        Walk = 0,
        HighWall = 1,
        LowWall = 2
    }

    /// <summary>src/Modules/SystemModule/Data/MapCellInfo.cs（逐字拷贝；ObjList 用 List 替 NativeList）。</summary>
    public class MapCellInfo
    {
        public List<CellObject> ObjList = new List<CellObject>();
        public CellAttribute Attribute = CellAttribute.Walk;

        public int Count => ObjList == null ? 0 : ObjList.Count;
        public bool Valid => Attribute == CellAttribute.Walk;
        public bool IsAvailable => ObjList?.Count > 0;

        public void Add(CellObject cell) => ObjList.Add(cell);
        public void Remove(int index) => ObjList.RemoveAt(index);
        public void Clear() => ObjList.Clear();
        public void SetAttribute(CellAttribute a) => Attribute = a;
    }

    /// <summary>src/Modules/SystemModule/Enums/VisibleFlag.cs。</summary>
    public enum VisibleFlag : byte
    {
        Hidden = 0,
        Invisible = 1,
        Show = 2
    }

    public class VisibleBaseObject
    {
        public ActorStub BaseObject;
        public VisibleFlag VisibleFlag;
    }

    /// <summary>ActorMgr 等价物：按 id 查对象（C# 侧是字典）。</summary>
    public static class ActorMgr
    {
        public static readonly Dictionary<int, ActorStub> Map = new Dictionary<int, ActorStub>();

        public static ActorStub Get(int id) => Map.TryGetValue(id, out ActorStub a) ? a : null;
        public static void Add(int id, ActorStub a) => Map[id] = a;
        public static void Clear() => Map.Clear();
    }

    /// <summary>
    /// BaseObject 的桩：字段名与 oracle 一致，函数体照抄。
    /// </summary>
    public class ActorStub
    {
        public int ActorId;
        public string ChrName = "";
        public int CurrX;
        public int CurrY;
        public byte ViewRange = 5; // BaseObject.cs:383
        public byte Race = 0;      // ActorRace.Play
        public ActorStub Master;   // C# 里是 IActor
        public bool Death;
        public bool Invisible;
        public bool Ghost;
        public bool FixedHideMode;
        public bool ObMode;
        public bool NastyMode;
        public bool WantRefMsg;
        public bool AddToMaped;
        public bool IsVisibleActive;
        public List<VisibleBaseObject> VisibleActors = new List<VisibleBaseObject>();
        public EnvirnomentStub Envir;

        public string MapName = "0";

        public string Format(string s, params object[] args) => string.Format(s, args);
        public void KickException() { /* 桩 */ }
        public void Dispose(VisibleBaseObject v) { /* 桩 */ }

        // ---------- src/M2Server/Actor/BaseObject.ViewRange.cs:12 逐字拷贝 ----------
        public virtual void UpdateVisibleGay(ActorStub baseObject)
        {
            bool boIsVisible = false;
            VisibleBaseObject visibleBaseObject;
            if ((baseObject.Race == 0) || (baseObject.Master != null))// 如果是人物或宝宝则置TRUE
            {
                IsVisibleActive = true;
            }
            for (int i = 0; i < VisibleActors.Count; i++)
            {
                visibleBaseObject = VisibleActors[i];
                if (visibleBaseObject.BaseObject == baseObject)
                {
                    visibleBaseObject.VisibleFlag = VisibleFlag.Invisible;
                    boIsVisible = true;
                    break;
                }
            }
            if (boIsVisible)
            {
                return;
            }
            visibleBaseObject = new VisibleBaseObject
            {
                VisibleFlag = VisibleFlag.Show,
                BaseObject = baseObject
            };
            VisibleActors.Add(visibleBaseObject);
        }

        // ---------- src/M2Server/Actor/BaseObject.ViewRange.cs:57 逐字拷贝 ----------
        public virtual void SearchViewRange()
        {
            const string sExceptionMsg = "[Exception] TBaseObject::SearchViewRange {0} {1} {2} {3} {4}";
            if (Envir == null)
            {
                LogService.Error("SearchViewRange nil PEnvir");
                return;
            }
            int n24 = 0;
            IsVisibleActive = false;// 先置为FALSE
            for (int i = 0; i < VisibleActors.Count; i++)
            {
                VisibleActors[i].VisibleFlag = VisibleFlag.Hidden;
            }
            short nStartX = (short)(CurrX - ViewRange);
            short nEndX = (short)(CurrX + ViewRange);
            short nStartY = (short)(CurrY - ViewRange);
            short nEndY = (short)(CurrY + ViewRange);
            try
            {
                for (short nX = nStartX; nX <= nEndX; nX++)
                {
                    for (short nY = nStartY; nY <= nEndY; nY++)
                    {
                        MapCellInfo cellInfo = Envir.GetCellInfo(nX, nY, out bool cellSuccess);
                        if (cellSuccess && cellInfo.IsAvailable)
                        {
                            n24 = 1;
                            int nIdx = 0;
                            while (true)
                            {
                                if (cellInfo.Count <= nIdx)
                                {
                                    break;
                                }
                                CellObject cellObject = cellInfo.ObjList[nIdx];
                                if (cellObject.ActorObject)
                                {
                                    if ((HUtil32.GetTickCount() - cellObject.AddTime) >= 60 * 1000)
                                    {
                                        cellInfo.Remove(nIdx);
                                        if (cellInfo.Count > 0)
                                        {
                                            continue;
                                        }
                                        cellInfo.Clear();
                                        break;
                                    }
                                    ActorStub baseObject = ActorMgr.Get(cellObject.CellObjId);
                                    if (baseObject != null)
                                    {
                                        if (!baseObject.Death && !baseObject.Invisible)
                                        {
                                            if (!baseObject.Ghost && !baseObject.FixedHideMode && !baseObject.ObMode)
                                            {
                                                if ((Master != null) || NastyMode || WantRefMsg || ((baseObject.Master != null) && (Math.Abs(baseObject.CurrX - CurrX) <= 3) && (Math.Abs(baseObject.CurrY - CurrY) <= 3)) || (baseObject.Race == 0))
                                                {
                                                    UpdateVisibleGay(baseObject);
                                                }
                                            }
                                        }
                                    }
                                }
                                nIdx++;
                            }
                        }
                    }
                }
            }
            catch (Exception e)
            {
                LogService.Error(Format(sExceptionMsg, n24, ChrName, MapName, CurrX, CurrY));
                LogService.Error(e.Message);
                KickException();
            }
            n24 = 2;
            try
            {
                int n18 = 0;
                while (true)
                {
                    if (VisibleActors.Count <= n18)
                    {
                        break;
                    }
                    VisibleBaseObject visibleBaseObject = VisibleActors[n18];
                    if (visibleBaseObject.VisibleFlag == VisibleFlag.Hidden)
                    {
                        VisibleActors.RemoveAt(n18);
                        Dispose(visibleBaseObject);
                        continue;
                    }
                    n18++;
                }
            }
            catch
            {
                LogService.Error(Format(sExceptionMsg, n24, ChrName, MapName, CurrX, CurrY));
                KickException();
            }
        }

        // ---------- src/M2Server/Actor/BaseObject.ViewRange.cs:159 逐字拷贝 ----------
        public void SearchViewRangeDeath()
        {
            if (Envir == null)
            {
                return;
            }
            if (VisibleActors.Count <= 0)
            {
                return;
            }
            IsVisibleActive = false;
            for (int i = 0; i < VisibleActors.Count; i++)
            {
                if (VisibleActors[i] == null)
                {
                    continue;
                }
                VisibleActors[i].VisibleFlag = VisibleFlag.Hidden;
            }
            short nStartX = (short)(CurrX - ViewRange);
            short nEndX = (short)(CurrX + ViewRange);
            short nStartY = (short)(CurrY - ViewRange);
            short nEndY = (short)(CurrY + ViewRange);
            for (short nX = nStartX; nX <= nEndX; nX++)
            {
                for (short nY = nStartY; nY <= nEndY; nY++)
                {
                    MapCellInfo cellInfo = Envir.GetCellInfo(nX, nY, out bool cellSuccess);
                    if (cellSuccess && cellInfo.IsAvailable)
                    {
                        for (int i = 0; i < cellInfo.ObjList.Count; i++)
                        {
                            CellObject cellObject = cellInfo.ObjList[i];
                            if (cellObject.ActorObject)
                            {
                                if ((HUtil32.GetTickCount() - cellObject.AddTime) >= 60 * 1000)
                                {
                                    cellInfo.Remove(i);
                                    if (cellInfo.Count > 0)
                                    {
                                        continue;
                                    }
                                    cellInfo.Clear();
                                    break;
                                }
                            }
                            if ((cellObject.CellType == CellType.Item) && !Death && (Race > 80))
                            {
                                if ((HUtil32.GetTickCount() - cellObject.AddTime) > 5 * 60 * 1000)
                                {
                                    cellInfo.Remove(i);
                                    if (cellInfo.Count > 0)
                                    {
                                        continue;
                                    }
                                    cellInfo.Clear();
                                }
                            }
                        }
                    }
                }
            }
            VisibleActors.Clear();
        }
    }

    /// <summary>
    /// Envirnoment 的桩：CellArray / Width / Height 与 GetCellInfo / AddMapObject / MoveToMovingObject
    /// 的**格子簿记部分**照抄 `src/M2Server/Maps/Envirnoment.cs`（行号见注释）。
    /// </summary>
    public class EnvirnomentStub
    {
        public int Width;
        public int Height;
        public MapCellInfo[] CellArray;

        public EnvirnomentStub(int width, int height)
        {
            Width = width;
            Height = height;
            CellArray = new MapCellInfo[width * height];
            for (int i = 0; i < CellArray.Length; i++)
            {
                CellArray[i] = new MapCellInfo();
            }
        }

        // Envirnoment.cs:305 逐字拷贝
        public bool CellMatch(int nX, int nY)
        {
            return nX >= 0 && nX < Width && nY >= 0 && nY < Height;
        }

        // Envirnoment.cs:322 逐字拷贝（列主序 x * Height + y；失败返回 CellArray[0]）
        public MapCellInfo GetCellInfo(int nX, int nY, out bool success)
        {
            if (nX >= 0 && nX < Width && nY >= 0 && nY < Height)
            {
                MapCellInfo cellInfo = CellArray[nX * Height + nY];
                if (cellInfo.Valid)
                {
                    success = true;
                    return cellInfo;
                }
            }
            success = false;
            return CellArray[0];
        }

        // Envirnoment.cs:252 逐字拷贝（去掉 AddObject/M2Share 簿记，仅保留格子追加）
        public bool AddMapObject(int nX, int nY, CellType cellType, int cellId, ActorStub mapObject)
        {
            if (mapObject == null)
            {
                return false;
            }
            if (!CellMatch(nX, nY))
            {
                return false;
            }
            bool result = false;
            MapCellInfo cellInfo = GetCellInfo(nX, nY, out bool cellSuccess);
            if (cellSuccess && cellInfo.Valid)
            {
                CellObject cellObject = new CellObject
                {
                    CellType = cellType,
                    CellObjId = cellId,
                    AddTime = HUtil32.GetTickCount()
                };
                if (!mapObject.AddToMaped)
                {
                    mapObject.AddToMaped = true;
                }
                cellObject.ActorObject = true;
                cellInfo.Add(cellObject);
                result = true;
            }
            return result;
        }

        // Envirnoment.cs:333 逐字拷贝（格子簿记部分；门对象分支本骨架不含）
        public bool MoveToMovingObject(int nCx, int nCy, ActorStub cert, int nX, int nY, bool boFlag)
        {
            if (!CellMatch(nX, nY))
            {
                return false;
            }
            bool canMove = true;
            bool result = false;
            MapCellInfo cellInfo = GetCellInfo(nX, nY, out bool cellSuccess);
            if (!boFlag && cellSuccess)
            {
                if (cellInfo.Valid && cellInfo.IsAvailable)
                {
                    for (int i = 0; i < cellInfo.ObjList.Count; i++)
                    {
                        CellObject cellObject = cellInfo.ObjList[i];
                        if (cellObject.ActorObject)
                        {
                            ActorStub baseObject = ActorMgr.Get(cellObject.CellObjId);
                            if (baseObject != null)
                            {
                                if (!baseObject.Ghost && !baseObject.Death && !baseObject.FixedHideMode && !baseObject.ObMode)
                                {
                                    canMove = false;
                                    break;
                                }
                            }
                        }
                    }
                }
                else
                {
                    canMove = true;
                }
            }
            if (canMove && cellInfo.Valid)
            {
                MapCellInfo oldCellInfo = GetCellInfo(nCx, nCy, out bool oldSuccess);
                if (oldSuccess && oldCellInfo.IsAvailable)
                {
                    for (int i = 0; i < oldCellInfo.ObjList.Count; i++)
                    {
                        CellObject moveObject = oldCellInfo.ObjList[i];
                        if (moveObject.CellObjId == cert.ActorId && moveObject.ActorObject)
                        {
                            oldCellInfo.Remove(i);
                            if (oldCellInfo.Count > 0)
                            {
                                continue;
                            }
                            oldCellInfo.Clear();
                            break;
                        }
                    }
                }
                AddMapObject(nX, nY, CellType.Play, cert.ActorId, cert);
                result = true;
            }
            return result;
        }
    }
}
