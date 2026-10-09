// D 线金标准生成器的桩类型。
// 命名与 C# 原代码一致，使 VerbatimCopies.cs 里的「逐字拷贝」可以不改动编译。
// HUtil32 的各静态方法为 src/OpenMir2/HUtil32.cs 的逐字拷贝。

using System;
using System.Collections.Generic;

namespace ParityGolden
{
    /// <summary>HUtil32 静态方法的逐字拷贝（只收 D 线用到的）。</summary>
    public static class HUtil32
    {
        // src/OpenMir2/HUtil32.cs:131
        public static int Round(double r)
        {
            return (int)Math.Round(r, MidpointRounding.AwayFromZero);
        }

        // src/OpenMir2/HUtil32.cs:669 / 674
        public static int _MIN(int n1, int n2)
        {
            return n1 < n2 ? n1 : n2;
        }

        public static int _MAX(int n1, int n2)
        {
            return n1 > n2 ? n1 : n2;
        }

        // src/OpenMir2/HUtil32.cs:60
        public static ushort MakeWord(ushort bLow, ushort bHigh)
        {
            return (ushort)(bLow | bHigh << 8);
        }

        // src/OpenMir2/HUtil32.cs:359
        public static int StrToInt(string str, int def)
        {
            return int.TryParse(str, out int result) ? result : def;
        }

        // src/OpenMir2/HUtil32.cs:391
        public static string GetValidStr3(string source, ref string dest, char[] dividerAry)
        {
            char[] div = new char[dividerAry.Length];
            for (int i = 0; i < dividerAry.Length; i++)
            {
                div[i] = dividerAry[i];
            }

            string[] ary = source.Split(div, 2, StringSplitOptions.RemoveEmptyEntries);
            dest = ary.Length > 0 ? ary[0] : string.Empty;
            return ary.Length > 1 ? ary[1] : string.Empty;
        }

        // src/OpenMir2/HUtil32.cs:488
        public static string ArrestStringEx(string source, string searchAfter, string arrestBefore, ref string arrestStr)
        {
            if (string.IsNullOrEmpty(source))
            {
                return string.Empty;
            }
            ReadOnlySpan<char> sourceSpan = source.AsSpan();
            int spanLen = sourceSpan.Length;
            string result = string.Empty;
            bool findData = false;
            if (spanLen >= 2)
            {
                if (source.StartsWith(searchAfter))
                {
                    sourceSpan = sourceSpan[1..spanLen];
                    findData = true;
                }
                else
                {
                    int n = sourceSpan.IndexOf(searchAfter, StringComparison.OrdinalIgnoreCase);
                    if (n > 0)
                    {
                        sourceSpan = sourceSpan.Slice(n + 1, spanLen - n - 1);
                        findData = true;
                    }
                }
            }
            if (findData)
            {
                int n = sourceSpan.IndexOf(arrestBefore, StringComparison.OrdinalIgnoreCase) + 1;
                if (n > 0)
                {
                    arrestStr = sourceSpan[..(n - 1)].ToString();
                    result = sourceSpan[(arrestStr.Length + 1)..].ToString();
                }
            }
            return result;
        }
    }

    /// <summary>src/OpenMir2/Data/StdItem.cs 子集（掉落路径字段）。</summary>
    public class StdItem
    {
        public string Name;
        public byte StdMode;
        public byte Shape;
        public ushort DuraMax;
    }

    /// <summary>src/OpenMir2/Packets/ClientPackets/ClientUserItem.cs 子集。</summary>
    public class UserItem
    {
        public int MakeIndex;
        public ushort Index;
        public ushort Dura;
        public ushort DuraMax;
        public byte[] Desc = new byte[14];
    }

    /// <summary>src/OpenMir2/Data/MonsterDropItem.cs。</summary>
    public struct MonsterDropItem
    {
        public int MaxPoint;
        public int SelPoint;
        public string ItemName;
        public int Count;
    }

    /// <summary>掉落对拍里的怪物对象（IMonsterActor 子集）。</summary>
    public class MonsterActorStub
    {
        public string ChrName;
        public int Gold;
        public List<UserItem> ItemList;
    }

    /// <summary>Ability 子集（RecalcLevelAbilitys 读写字段）。</summary>
    public class Ability
    {
        public byte Level;
        public ushort HP;
        public ushort MP;
        public ushort MaxHP;
        public ushort MaxMP;
        public ushort MaxWeight;
        public byte MaxWearWeight;
        public byte MaxHandWeight;
        public ushort DC;
        public ushort MC;
        public ushort SC;
        public ushort AC;
        public ushort MAC;
    }

    /// <summary>src/OpenMir2/Enums/PlayerJob.cs。</summary>
    public enum PlayerJob : byte
    {
        Warrior = 0,
        Wizard = 1,
        Taoist = 2,
    }

    /// <summary>GameSvrConf 子集 + 默认值（GameSvrConf.cs:1888-1894 等）。</summary>
    public class GameConfig
    {
        public byte MonRandomAddValue = 10;
        public int[] NeedExps = new int[1000]; // Grobal2.MaxChangeLevel
        // 默认值与 GameSvrConf.cs:1828-1839 一致（缺键时生效）
        public int LimitExpLevel = 1000;
        public int LimitExpValue = 1;
        public int KillMonExpMultiple = 1;
        public bool HighLevelKillMonFixExp;
        public bool HighLevelGroupFixExp = true;
        public bool UseFixExp = true;
        public bool MonDelHptoExp;
        public int BaseExp = 100000000;
        public int AddExp = 1000000;
        public int MonHptoExpLevel = 100;
        public int MonHptoExpmax = 1;

        public int nLevelValueOfTaosHP = 6;
        public double nLevelValueOfTaosHPRate = 2.5;
        public int nLevelValueOfTaosMP = 8;
        public int nLevelValueOfWizardHP = 15;
        public double nLevelValueOfWizardHPRate = 1.8;
        public int nLevelValueOfWarrHP = 4;
        public double nLevelValueOfWarrHPRate = 4.5;

        public int ItemNumber;
    }

    /// <summary>ItemSystem 子集（掉落路径）：名单 + 拷贝 + 随机极品。</summary>
    public class ItemSystemStub
    {
        public readonly List<StdItem> StdItemList = new();

        // src/M2Server/Items/GameItemSystem.cs:113 逐字拷贝
        public bool CopyToUserItemFromName(string sItemName, ref UserItem item)
        {
            if (string.IsNullOrEmpty(sItemName))
            {
                return false;
            }

            for (int i = 0; i < StdItemList.Count; i++)
            {
                StdItem stdItem = StdItemList[i];
                if (!stdItem.Name.Equals(sItemName, StringComparison.OrdinalIgnoreCase))
                {
                    continue;
                }

                if (item == null)
                {
                    item = new UserItem();
                }

                item.Index = (ushort)(i + 1);
                item.MakeIndex = M2Share.GetItemNumber();
                item.Dura = stdItem.DuraMax;
                item.DuraMax = stdItem.DuraMax;
                return true;
            }
            return false;
        }

        // src/M2Server/Items/GameItemSystem.cs:20 逐字拷贝
        public StdItem GetStdItem(ushort nItemIdx)
        {
            StdItem result = null;
            nItemIdx -= 1;
            if (nItemIdx >= 0 && StdItemList.Count > nItemIdx)
            {
                result = StdItemList[nItemIdx];
                if (string.IsNullOrEmpty(result.Name))
                {
                    result = null;
                }
            }
            return result;
        }
    }

    public static class SystemShare
    {
        public static readonly GameConfig Config = new();
        public static readonly ItemSystemStub ItemSystem = new();
        // src/Modules/SystemModule/SystemShare.cs:170
        public static int[] OldNeedExps = new int[1000];
    }

    public static class Grobal2
    {
        // src/OpenMir2/Grobal2.cs:11
        public const string StringGoldName = "金币";
    }

    public static class M2Share
    {
        public static OpenMir2.RandomNumber RandomNumber = OpenMir2.RandomNumber.GetInstance();
        // src/M2Server/M2Share.cs:188
        public static readonly HashSet<byte> StdModeMap = new HashSet<byte>() { 15, 19, 20, 21, 22, 23, 24, 26 };

        // src/M2Server/M2Share.cs:547 逐字拷贝。
        // 注意：返回值含 GetTickCount()（墙钟），两次运行即不同——
        // 掉落对拍因此不含 MakeIndex 字段（见 tests/parity/README.md）。
        public static int GetItemNumber()
        {
            SystemShare.Config.ItemNumber++;
            if (SystemShare.Config.ItemNumber > int.MaxValue / 2 - 1)
            {
                SystemShare.Config.ItemNumber = 1;
            }
            return SystemShare.Config.ItemNumber + Environment.TickCount;
        }
    }
}
