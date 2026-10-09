// 原 C# 函数的逐字拷贝（只适配外层签名/命名空间，函数体一行不改）。
// 每段标注来源文件与行号；若上游源码更新，这里必须同步。

using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using ParityGolden;

namespace ParityGolden
{
    /// <summary>
    /// src/GameSrv/Word/WorldServer.MonGen.cs 的掉落相关函数逐字拷贝。
    /// </summary>
    public static class WorldServerCopies
    {
        // src/GameSrv/Word/WorldServer.MonGen.cs:440 逐字拷贝。
        // 适配：原版从 MonsterList 按 ChrName 查 MonsterInfo，这里直接传入 itemList；
        // 循环体（含 itemName 只赋值一次的 bug）一行未动。
        public static void MonGetRandomItems(IList<MonsterDropItem> itemList, MonsterActorStub mon)
        {
            string itemName = string.Empty;
            if (itemList != null && itemList.Count > 0)
            {
                mon.ItemList = new List<UserItem>(itemList.Count);
                for (int i = 0; i < itemList.Count; i++)
                {
                    MonsterDropItem monItem = itemList[i];
                    if (M2Share.RandomNumber.Random(monItem.MaxPoint) <= monItem.SelPoint)
                    {
                        if (string.Compare(monItem.ItemName, Grobal2.StringGoldName, StringComparison.OrdinalIgnoreCase) == 0)
                        {
                            mon.Gold = mon.Gold + monItem.Count / 2 + M2Share.RandomNumber.Random(monItem.Count);
                        }
                        else
                        {
                            if (string.IsNullOrEmpty(itemName))
                            {
                                itemName = monItem.ItemName;
                            }

                            UserItem userItem = null;
                            if (SystemShare.ItemSystem.CopyToUserItemFromName(itemName, ref userItem))
                            {
                                userItem.Dura = (ushort)HUtil32.Round(userItem.DuraMax / 100.0 * (20 + M2Share.RandomNumber.Random(80)));
                                StdItem stdItem = SystemShare.ItemSystem.GetStdItem(userItem.Index);
                                if (stdItem == null)
                                {
                                    continue;
                                }

                                if (stdItem.StdMode > 0 && M2Share.RandomNumber.Random(SystemShare.Config.MonRandomAddValue) == 0) //极品掉落几率
                                {
                                    GameItemSystemCopies.RandomUpgradeItem(stdItem, userItem);
                                }
                                if (M2Share.StdModeMap.Contains(stdItem.StdMode))
                                {
                                    if (stdItem.Shape == 130 || stdItem.Shape == 131 || stdItem.Shape == 132)
                                    {
                                        GameItemSystemCopies.RandomSetUnknownItem(stdItem, userItem);
                                    }
                                }
                                mon.ItemList.Add(userItem);
                            }
                        }
                    }
                }
            }
        }

        // src/GameSrv/DB/LocalDB.cs:596 逐字拷贝（LoadMonitems）。
        // 适配：原版按怪物名拼 Envir 路径，这里直接传入文件路径；StringList 换为
        // File.ReadAllLines（GB2312，与 StringList.LoadFromFile 同编码），解析体一行未动。
        private static readonly char[] TextSplitConst = { ' ', '\t' };
        private static readonly char[] MonsterSpitConst = { ' ', '/', '\t' };

        public static void LoadMonitems(string monFileName, ref IList<MonsterDropItem> itemList)
        {
            string sData = string.Empty;
            if (File.Exists(monFileName))
            {
                if (itemList != null)
                {
                    for (int i = 0; i < itemList.Count; i++)
                    {
                        itemList[i] = default;
                    }
                    itemList.Clear();
                }
                if (itemList == null)
                {
                    itemList = new List<MonsterDropItem>();
                }
                string[] loadList = File.ReadAllLines(monFileName, Encoding.GetEncoding("gb2312"));
                for (int i = 0; i < loadList.Length; i++)
                {
                    string s28 = loadList[i];
                    if (!string.IsNullOrEmpty(s28) && s28[0] != ';')
                    {
                        s28 = HUtil32.GetValidStr3(s28, ref sData, MonsterSpitConst);
                        int n18 = HUtil32.StrToInt(sData, -1);
                        s28 = HUtil32.GetValidStr3(s28, ref sData, MonsterSpitConst);
                        int n1C = HUtil32.StrToInt(sData, -1);
                        s28 = HUtil32.GetValidStr3(s28, ref sData, TextSplitConst);
                        if (!string.IsNullOrEmpty(sData))
                        {
                            if (sData[0] == '\"')
                            {
                                HUtil32.ArrestStringEx(sData, "\"", "\"", ref sData);
                            }
                        }
                        string itemName = sData;
                        s28 = HUtil32.GetValidStr3(s28, ref sData, TextSplitConst);
                        int itemCount = HUtil32.StrToInt(sData, 1);
                        if (n18 > 0 && n1C > 0 && !string.IsNullOrEmpty(itemName))
                        {
                            MonsterDropItem monItem = new MonsterDropItem
                            {
                                SelPoint = n18 - 1,
                                MaxPoint = n1C,
                                ItemName = itemName,
                                Count = itemCount
                            };
                            itemList.Add(monItem);
                        }
                    }
                }
            }
        }
    }

    /// <summary>
    /// src/M2Server/Items/GameItemSystem.cs 的极品加成函数逐字拷贝。
    /// </summary>
    public static class GameItemSystemCopies
    {
        // GameItemSystem.cs:142
        private static int GetUpgrade(int count, int ran)
        {
            int result = 0;
            for (int i = 0; i < count; i++)
            {
                if (M2Share.RandomNumber.Random(ran) == 0)
                {
                    result = result + 1;
                }
                else
                {
                    break;
                }
            }
            return result;
        }

        // GameItemSystem.cs:182
        private static void UpgradeRandomWeapon(UserItem pu)
        {
            int up = GetUpgrade(12, 15);
            if (M2Share.RandomNumber.Random(15) == 0)
            {
                pu.Desc[0] = (byte)(1 + up);//DC
            }
            up = GetUpgrade(12, 15);
            if (M2Share.RandomNumber.Random(20) == 0)
            {
                int incp = (1 + up) / 3;
                if (incp > 0)
                {
                    if (M2Share.RandomNumber.Random(3) != 0)
                    {
                        pu.Desc[6] = (byte)incp;
                    }
                    else
                    {
                        pu.Desc[6] = (byte)(10 + incp);
                    }
                }
            }
            up = GetUpgrade(12, 15);
            if (M2Share.RandomNumber.Random(15) == 0)
            {
                pu.Desc[1] = (byte)(1 + up);//MC
            }
            up = GetUpgrade(12, 15);
            if (M2Share.RandomNumber.Random(15) == 0)
            {
                pu.Desc[2] = (byte)(1 + up);//SC
            }
            up = GetUpgrade(12, 15);
            if (M2Share.RandomNumber.Random(24) == 0)
            {
                pu.Desc[5] = (byte)(1 + up / 2);
            }
            up = GetUpgrade(12, 12);
            if (M2Share.RandomNumber.Random(3) < 2)
            {
                int n = (1 + up) * 2000;
                pu.DuraMax = (ushort)HUtil32._MIN(65000, pu.DuraMax + n);
                pu.Dura = (ushort)HUtil32._MIN(65000, pu.Dura + n);
            }
            up = GetUpgrade(12, 15);
            if (M2Share.RandomNumber.Random(10) == 0)
            {
                pu.Desc[7] = (byte)(1 + up / 2);
            }
        }

        // GameItemSystem.cs:234
        private static void UpgradeRandomDress(UserItem pu)
        {
            int up = GetUpgrade(6, 15);
            if (M2Share.RandomNumber.Random(30) == 0)
            {
                pu.Desc[0] = (byte)(1 + up);// AC
            }
            up = GetUpgrade(6, 15);
            if (M2Share.RandomNumber.Random(30) == 0)
            {
                pu.Desc[1] = (byte)(1 + up); // MAC
            }
            up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(40) == 0)
            {
                pu.Desc[2] = (byte)(1 + up);// DC
            }
            up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(40) == 0)
            {
                pu.Desc[3] = (byte)(1 + up); // MC
            }
            up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(40) == 0)
            {
                pu.Desc[4] = (byte)(1 + up);// SC
            }
            up = GetUpgrade(6, 10);
            if (M2Share.RandomNumber.Random(8) < 6)
            {
                int n = (1 + up) * 2000;
                pu.DuraMax = (ushort)HUtil32._MIN(65000, pu.DuraMax + n);
                pu.Dura = (ushort)HUtil32._MIN(65000, pu.Dura + n);
            }
        }

        // GameItemSystem.cs:270
        private static void UpgradeRandomNecklace(UserItem pu)
        {
            int up = GetUpgrade(6, 30);
            if (M2Share.RandomNumber.Random(60) == 0)
            {
                pu.Desc[0] = (byte)(1 + up);// AC(HIT)
            }
            up = GetUpgrade(6, 30);
            if (M2Share.RandomNumber.Random(60) == 0)
            {
                pu.Desc[1] = (byte)(1 + up);// MAC(SPEED)
            }
            up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(30) == 0)
            {
                pu.Desc[2] = (byte)(1 + up);// DC
            }
            up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(30) == 0)
            {
                pu.Desc[3] = (byte)(1 + up);// MC
            }
            up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(30) == 0)
            {
                pu.Desc[4] = (byte)(1 + up);// SC
            }
            up = GetUpgrade(6, 12);
            if (M2Share.RandomNumber.Random(20) < 15)
            {
                int n = (1 + up) * 1000;
                pu.DuraMax = (ushort)HUtil32._MIN(65000, pu.DuraMax + n);
                pu.Dura = (ushort)HUtil32._MIN(65000, pu.Dura + n);
            }
        }

        // GameItemSystem.cs:306
        private static void UpgradeRandomBarcelet(UserItem pu)
        {
            int up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(20) == 0)
            {
                pu.Desc[0] = (byte)(1 + up);// AC
            }
            up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(20) == 0)
            {
                pu.Desc[1] = (byte)(1 + up);// MAC
            }
            up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(30) == 0)
            {
                pu.Desc[2] = (byte)(1 + up);// DC
            }
            up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(30) == 0)
            {
                pu.Desc[3] = (byte)(1 + up);// MC
            }
            up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(30) == 0)
            {
                pu.Desc[4] = (byte)(1 + up);// SC
            }
            up = GetUpgrade(6, 12);
            if (M2Share.RandomNumber.Random(20) < 15)
            {
                int n = (1 + up) * 1000;
                pu.DuraMax = (ushort)HUtil32._MIN(65000, pu.DuraMax + n);
                pu.Dura = (ushort)HUtil32._MIN(65000, pu.Dura + n);
            }
        }

        // GameItemSystem.cs:342
        private static void UpgradeRandomNecklace19(UserItem pu)
        {
            int up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(40) == 0)
            {
                pu.Desc[0] = (byte)(1 + up);
            }
            up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(40) == 0)
            {
                pu.Desc[1] = (byte)(1 + up);
            }
            up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(30) == 0)
            {
                pu.Desc[2] = (byte)(1 + up); // DC
            }
            up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(30) == 0)
            {
                pu.Desc[3] = (byte)(1 + up);// MC
            }
            up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(30) == 0)
            {
                pu.Desc[4] = (byte)(1 + up);// SC
            }
            up = GetUpgrade(6, 10);
            if (M2Share.RandomNumber.Random(4) < 3)
            {
                int n = (1 + up) * 1000;
                pu.DuraMax = (ushort)HUtil32._MIN(65000, pu.DuraMax + n);
                pu.Dura = (ushort)HUtil32._MIN(65000, pu.Dura + n);
            }
        }

        // GameItemSystem.cs:378
        private static void UpgradeRandomRings(UserItem pu)
        {
            int up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(30) == 0)
            {
                pu.Desc[2] = (byte)(1 + up); // DC
            }
            up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(30) == 0)
            {
                pu.Desc[3] = (byte)(1 + up);// MC
            }
            up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(30) == 0)
            {
                pu.Desc[4] = (byte)(1 + up);// SC
            }
            up = GetUpgrade(6, 12);
            if (M2Share.RandomNumber.Random(4) < 3)
            {
                int n = (1 + up) * 1000;
                pu.DuraMax = (ushort)HUtil32._MIN(65000, pu.DuraMax + n);
                pu.Dura = (ushort)HUtil32._MIN(65000, pu.Dura + n);
            }
        }

        // GameItemSystem.cs:404
        private static void UpgradeRandomRings23(UserItem pu)
        {
            int up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(40) == 0)
            {
                pu.Desc[0] = (byte)(1 + up);
            }
            up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(40) == 0)
            {
                pu.Desc[1] = (byte)(1 + up);
            }
            up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(30) == 0)
            {
                pu.Desc[2] = (byte)(1 + up);// DC
            }
            up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(30) == 0)
            {
                pu.Desc[3] = (byte)(1 + up);// MC
            }
            up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(30) == 0)
            {
                pu.Desc[4] = (byte)(1 + up); // SC
            }
            up = GetUpgrade(6, 12);
            if (M2Share.RandomNumber.Random(4) < 3)
            {
                int n = (1 + up) * 1000;
                pu.DuraMax = (ushort)HUtil32._MIN(65000, pu.DuraMax + n);
                pu.Dura = (ushort)HUtil32._MIN(65000, pu.Dura + n);
            }
        }

        // GameItemSystem.cs:440
        private static void UpgradeRandomHelmet(UserItem pu)
        {
            int up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(40) == 0)
            {
                pu.Desc[0] = (byte)(1 + up);// AC
            }
            up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(30) == 0)
            {
                pu.Desc[1] = (byte)(1 + up);// MAC
            }
            up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(30) == 0)
            {
                pu.Desc[2] = (byte)(1 + up);// DC
            }
            up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(30) == 0)
            {
                pu.Desc[3] = (byte)(1 + up);// MC
            }
            up = GetUpgrade(6, 20);
            if (M2Share.RandomNumber.Random(30) == 0)
            {
                pu.Desc[4] = (byte)(1 + up);// SC
            }
            up = GetUpgrade(6, 12);
            if (M2Share.RandomNumber.Random(4) < 3)
            {
                int n = (1 + up) * 1000;
                pu.DuraMax = (ushort)HUtil32._MIN(65000, pu.DuraMax + n);
                pu.Dura = (ushort)HUtil32._MIN(65000, pu.Dura + n);
            }
        }

        // GameItemSystem.cs:476
        private static void RandomSetUnknownHelmet(UserItem pu)
        {
            int up = GetUpgrade(4, 3) + GetUpgrade(4, 8) + GetUpgrade(4, 20);
            if (up > 0)
            {
                pu.Desc[0] = (byte)up;// AC
            }
            int sum = up;
            up = GetUpgrade(4, 3) + GetUpgrade(4, 8) + GetUpgrade(4, 20);
            if (up > 0)
            {
                pu.Desc[1] = (byte)up;// MAC
            }
            sum = sum + up;
            up = GetUpgrade(3, 15) + GetUpgrade(3, 30);
            if (up > 0)
            {
                pu.Desc[2] = (byte)up;// DC
            }
            sum = sum + up;
            up = GetUpgrade(3, 15) + GetUpgrade(3, 30);
            if (up > 0)
            {
                pu.Desc[3] = (byte)up;// MC
            }
            sum = sum + up;
            up = GetUpgrade(3, 15) + GetUpgrade(3, 30);
            if (up > 0)
            {
                pu.Desc[4] = (byte)up;// SC
            }
            sum = sum + up;
            up = GetUpgrade(6, 30);
            if (up > 0)
            {
                int n = (1 + up) * 1000;
                pu.DuraMax = (ushort)HUtil32._MIN(65000, pu.DuraMax + n);
                pu.Dura = (ushort)HUtil32._MIN(65000, pu.Dura + n);
            }
            if (M2Share.RandomNumber.Random(30) == 0)
            {
                pu.Desc[7] = 1;
            }
            pu.Desc[8] = 1;
            if (sum >= 3)
            {
                if (pu.Desc[0] >= 5)
                {
                    pu.Desc[5] = 1;
                    pu.Desc[6] = (byte)(25 + pu.Desc[0] * 3);
                    return;
                }
                if (pu.Desc[2] >= 2)
                {
                    pu.Desc[5] = 1;
                    pu.Desc[6] = (byte)(35 + pu.Desc[2] * 4);
                    return;
                }
                if (pu.Desc[3] >= 2)
                {
                    pu.Desc[5] = 2;
                    pu.Desc[6] = (byte)(18 + pu.Desc[3] * 2);
                    return;
                }
                if (pu.Desc[4] >= 2)
                {
                    pu.Desc[5] = 3;
                    pu.Desc[6] = (byte)(18 + pu.Desc[4] * 2);
                    return;
                }
                pu.Desc[6] = (byte)(18 + sum * 2);
            }
        }

        // GameItemSystem.cs:550
        private static void RandomSetUnknownRing(UserItem pu)
        {
            int up = GetUpgrade(3, 4) + GetUpgrade(3, 8) + GetUpgrade(6, 20);
            if (up > 0)
            {
                pu.Desc[2] = (byte)up;// DC
            }
            int sum = up;
            up = GetUpgrade(3, 4) + GetUpgrade(3, 8) + GetUpgrade(6, 20);
            if (up > 0)
            {
                pu.Desc[3] = (byte)up;// MC
            }
            sum = sum + up;
            up = GetUpgrade(3, 4) + GetUpgrade(3, 8) + GetUpgrade(6, 20);
            if (up > 0)
            {
                pu.Desc[4] = (byte)up;// SC
            }
            sum = sum + up;
            up = GetUpgrade(6, 30);
            if (up > 0)
            {
                int n = (1 + up) * 1000;
                pu.DuraMax = (ushort)HUtil32._MIN(65000, pu.DuraMax + n);
                pu.Dura = (ushort)HUtil32._MIN(65000, pu.Dura + n);
            }
            if (M2Share.RandomNumber.Random(30) == 0)
            {
                pu.Desc[7] = 1;
            }
            pu.Desc[8] = 1;
            if (sum >= 3)
            {
                if (pu.Desc[2] >= 3)
                {
                    pu.Desc[5] = 1;
                    pu.Desc[6] = (byte)(25 + pu.Desc[2] * 3);
                    return;
                }
                if (pu.Desc[3] >= 3)
                {
                    pu.Desc[5] = 2;
                    pu.Desc[6] = (byte)(18 + pu.Desc[3] * 2);
                    return;
                }
                if (pu.Desc[4] >= 3)
                {
                    pu.Desc[5] = 3;
                    pu.Desc[6] = (byte)(18 + pu.Desc[4] * 2);
                    return;
                }
                pu.Desc[6] = (byte)(18 + sum * 2);
            }
        }

        // GameItemSystem.cs:606
        private static void RandomSetUnknownBracelet(UserItem pu)
        {
            int up = GetUpgrade(3, 5) + GetUpgrade(5, 20);
            if (up > 0)
            {
                pu.Desc[0] = (byte)up; // AC
            }
            int sum = up;
            up = GetUpgrade(3, 5) + GetUpgrade(5, 20);
            if (up > 0)
            {
                pu.Desc[1] = (byte)up;// MAC
            }
            sum = sum + up;
            up = GetUpgrade(3, 15) + GetUpgrade(5, 30);
            if (up > 0)
            {
                pu.Desc[2] = (byte)up;// DC
            }
            sum = sum + up;
            up = GetUpgrade(3, 15) + GetUpgrade(5, 30);
            if (up > 0)
            {
                pu.Desc[3] = (byte)up;// MC
            }
            sum = sum + up;
            up = GetUpgrade(3, 15) + GetUpgrade(5, 30);
            if (up > 0)
            {
                pu.Desc[4] = (byte)up;// SC
            }
            sum = sum + up;
            up = GetUpgrade(6, 30);
            if (up > 0)
            {
                int n = (1 + up) * 1000;
                pu.DuraMax = (ushort)HUtil32._MIN(65000, pu.DuraMax + n);
                pu.Dura = (ushort)HUtil32._MIN(65000, pu.Dura + n);
            }
            if (M2Share.RandomNumber.Random(30) == 0)
            {
                pu.Desc[7] = 1;
            }
            pu.Desc[8] = 1;
            if (sum >= 2)
            {
                if (pu.Desc[0] >= 3)
                {
                    pu.Desc[5] = 1;
                    pu.Desc[6] = (byte)(25 + pu.Desc[0] * 3);
                    return;
                }
                if (pu.Desc[2] >= 2)
                {
                    pu.Desc[5] = 1;
                    pu.Desc[6] = (byte)(30 + pu.Desc[2] * 3);
                    return;
                }
                if (pu.Desc[3] >= 2)
                {
                    pu.Desc[5] = 2;
                    pu.Desc[6] = (byte)(20 + pu.Desc[3] * 2);
                    return;
                }
                if (pu.Desc[4] >= 2)
                {
                    pu.Desc[5] = 3;
                    pu.Desc[6] = (byte)(20 + pu.Desc[4] * 2);
                    return;
                }
                pu.Desc[6] = (byte)(18 + sum * 2);
            }
        }

        // GameItemSystem.cs:1184
        public static void RandomUpgradeItem(StdItem stdItem, UserItem pu)
        {
            if (stdItem != null)
            {
                switch (stdItem.StdMode)
                {
                    case 5:
                    case 6:
                        UpgradeRandomWeapon(pu);
                        break;
                    case 10:
                    case 11:
                        UpgradeRandomDress(pu);
                        break;
                    case 19:
                        UpgradeRandomNecklace19(pu);
                        break;
                    case 20:
                    case 21:
                    case 24:
                        UpgradeRandomNecklace(pu);
                        break;
                    case 26:
                        UpgradeRandomBarcelet(pu);
                        break;
                    case 22:
                        UpgradeRandomRings(pu);
                        break;
                    case 23:
                        UpgradeRandomRings23(pu);
                        break;
                    case 15:
                        UpgradeRandomHelmet(pu);
                        break;
                }
            }
        }

        // GameItemSystem.cs:1222
        public static void RandomSetUnknownItem(StdItem stdItem, UserItem pu)
        {
            if (stdItem != null)
            {
                switch (stdItem.StdMode)
                {
                    case 15:
                        RandomSetUnknownHelmet(pu);
                        break;
                    case 22:
                    case 23:
                        RandomSetUnknownRing(pu);
                        break;
                    case 24:
                    case 26:
                        RandomSetUnknownBracelet(pu);
                        break;
                }
            }
        }
    }

    /// <summary>
    /// src/M2Server/Player/PlayObject.cs:5254（RecalcLevelAbilitys）逐字拷贝。
    /// 适配：Job 从实例属性改为入参；函数体（含道士 MaxHandWeight 判断/赋值
    /// 除数不一致的 bug）一行未动。
    /// </summary>
    public static class PlayObjectCopies
    {
        public static void RecalcLevelAbilitys(PlayerJob job, Ability abil)
        {
            int n;
            byte nLevel = abil.Level;
            switch (job)
            {
                case PlayerJob.Taoist:
                    abil.MaxHP = (ushort)HUtil32._MIN(ushort.MaxValue, 14 + HUtil32.Round((nLevel / (double)SystemShare.Config.nLevelValueOfTaosHP + SystemShare.Config.nLevelValueOfTaosHPRate) * nLevel));
                    abil.MaxMP = (ushort)HUtil32._MIN(ushort.MaxValue, 13 + HUtil32.Round(nLevel / (double)SystemShare.Config.nLevelValueOfTaosMP * 2.2 * nLevel));
                    abil.MaxWeight = (ushort)(50 + HUtil32.Round(nLevel / 4.0 * nLevel));
                    abil.MaxWearWeight = (byte)HUtil32._MIN(byte.MaxValue, (15 + HUtil32.Round(nLevel / 50.0 * nLevel)));
                    if ((12 + HUtil32.Round(abil.Level / 13.0 * abil.Level)) > 255)
                    {
                        abil.MaxHandWeight = byte.MaxValue;
                    }
                    else
                    {
                        abil.MaxHandWeight = (byte)(12 + HUtil32.Round(nLevel / 42.0 * nLevel));
                    }
                    n = nLevel / 7;
                    abil.DC = HUtil32.MakeWord((ushort)HUtil32._MAX(n - 1, 0), (ushort)HUtil32._MAX(1, n));
                    abil.MC = 0;
                    abil.SC = HUtil32.MakeWord((ushort)HUtil32._MAX(n - 1, 0), (ushort)HUtil32._MAX(1, n));
                    abil.AC = 0;
                    n = HUtil32.Round(nLevel / 6.0);
                    abil.MAC = HUtil32.MakeWord((ushort)(n / 2), (ushort)(n + 1));
                    break;
                case PlayerJob.Wizard:
                    abil.MaxHP = (ushort)HUtil32._MIN(ushort.MaxValue, 14 + HUtil32.Round(((nLevel / (double)SystemShare.Config.nLevelValueOfWizardHP) + SystemShare.Config.nLevelValueOfWizardHPRate) * nLevel));
                    abil.MaxMP = (ushort)HUtil32._MIN(ushort.MaxValue, 13 + HUtil32.Round(((nLevel / (double)5) + 2) * 2.2 * nLevel));
                    abil.MaxWeight = (ushort)(50 + HUtil32.Round(nLevel / 5.0 * nLevel));
                    abil.MaxWearWeight = (byte)HUtil32._MIN(byte.MaxValue, 15 + HUtil32.Round(nLevel / 100.0 * nLevel));
                    abil.MaxHandWeight = (byte)(12 + HUtil32.Round(nLevel / 90.0 * nLevel));
                    n = nLevel / 7;
                    abil.DC = HUtil32.MakeWord((ushort)HUtil32._MAX(n - 1, 0), (ushort)HUtil32._MAX(1, n));
                    abil.MC = HUtil32.MakeWord((ushort)HUtil32._MAX(n - 1, 0), (ushort)HUtil32._MAX(1, n));
                    abil.SC = 0;
                    abil.AC = 0;
                    abil.MAC = 0;
                    break;
                case PlayerJob.Warrior:
                    abil.MaxHP = (ushort)HUtil32._MIN(ushort.MaxValue, 14 + HUtil32.Round(((nLevel / (double)SystemShare.Config.nLevelValueOfWarrHP) + SystemShare.Config.nLevelValueOfWarrHPRate + (nLevel / (double)20)) * nLevel));
                    abil.MaxMP = (ushort)HUtil32._MIN(ushort.MaxValue, 11 + HUtil32.Round(nLevel * 3.5));
                    abil.MaxWeight = (ushort)(50 + HUtil32.Round(nLevel / 3.0 * nLevel));
                    abil.MaxWearWeight = (byte)HUtil32._MIN(byte.MaxValue, (15 + HUtil32.Round(nLevel / 20.0 * nLevel)));
                    abil.MaxHandWeight = (byte)(12 + HUtil32.Round(nLevel / 13.0 * nLevel));
                    abil.DC = HUtil32.MakeWord((ushort)HUtil32._MAX(nLevel / 5 - 1, 1), (ushort)HUtil32._MAX(1, nLevel / 5));
                    abil.SC = 0;
                    abil.MC = 0;
                    abil.AC = HUtil32.MakeWord(0, (ushort)(nLevel / 7));
                    abil.MAC = 0;
                    break;
                default:
                    break;
            }
            if (abil.HP > abil.MaxHP)
            {
                abil.HP = abil.MaxHP;
            }
            if (abil.MP > abil.MaxMP)
            {
                abil.MP = abil.MaxMP;
            }
        }
    }
}
