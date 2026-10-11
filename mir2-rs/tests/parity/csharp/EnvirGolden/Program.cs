// M2 入图帧 envir/配置层输入的 C# 金标准探针（卡 openmir2-m2-envir-inputs）。
//
// 只读：不写 MySQL、不改任何现网文件（Server.conf 传夹具副本；MapInfo.txt 只读）。
//
// 用法：
//   EnvirGolden mapinfo <MapInfo.txt> <out.json>
//   EnvirGolden conf <Server.conf> <out.json>
//   EnvirGolden clientconf <out.json>
//
// mapinfo 的 section 解析循环**逐字拷贝**自 `src/GameSrv/Maps/Maps.cs:52-320`（LoadMapInfo 的
// 地图设置遍历 + AddMapInfo 的名/文件名拆分），只把 `SystemShare.MapMgr.AddMapInfo(...)` 这个
// 落地调用换成把结果收进列表（其余一行未改）。HUtil32 静态方法见 HUtil32Copies.cs。

using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using OpenMir2;
using OpenMir2.Common;
using OpenMir2.Data;
using OpenMir2.Packets.ClientPackets;

namespace EnvirGolden
{
    internal static class Program
    {
        private static readonly JsonSerializerOptions JsonOpts = new JsonSerializerOptions
        {
            // 紧凑输出：真实语料 322 条 section 的 JSON 会用上百 KB，留给机器逐字段比较；
            // 人能读的期望值表见 tests/parity/envir-2026-10-11/README.md。
            WriteIndented = false,
            Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
        };

        private static int Main(string[] args)
        {
            // .NET Core 默认不带 GB2312；OpenMir2 各进程启动时同样注册了这个 Provider。
            Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
            if (args.Length < 1)
            {
                Console.Error.WriteLine("mode required: mapinfo|conf|clientconf");
                return 2;
            }
            return args[0] switch
            {
                "mapinfo" => MapInfoMode(args[1], args[2]),
                "conf" => ConfMode(args[1], args[2]),
                "clientconf" => ClientConfMode(args[1]),
                _ => Fail($"unknown mode {args[0]}"),
            };
        }

        private static int Fail(string msg)
        {
            Console.Error.WriteLine(msg);
            return 2;
        }

        private static void WriteJson(string path, object value)
        {
            File.WriteAllText(path, JsonSerializer.Serialize(value, JsonOpts), new UTF8Encoding(false));
        }

        // ---------------------------------------------------------------- mapinfo

        /// <summary>`Maps.cs:397-415` 的 LoadSubMapInfo（只读；无 ConnectMapInfo 时不触发）。</summary>
        private static void LoadSubMapInfo(StringList loadList, string mapInfoDir, string sFileName)
        {
            // 源码是 `fileDir + sFileName` 的字符串拼接（没有分隔符）——照抄。
            string sFilePatchName = mapInfoDir + sFileName;
            if (File.Exists(sFilePatchName))
            {
                StringList loadMapList = new StringList();
                loadMapList.LoadFromFile(sFilePatchName);
                for (int i = 0; i < loadMapList.Count; i++)
                {
                    loadList.Add(loadMapList[i]);
                }
            }
        }

        /// <summary>MapInfoFlag 的 JSON 视图（键名与 Rust `MapInfoFlag` 字段一致）。</summary>
        private static object FlagJson(MapInfoFlag f) => new
        {
            safe_area = f.SafeArea,
            request_level = f.RequestLevel,
            need_seton_flag = f.NeedSetonFlag,
            need_on_off = f.NeedOnOff,
            music = f.Music,
            music_id = f.MusicId,
            bo_darkness = f.boDarkness,
            day_light = f.DayLight,
            fight_zone = f.FightZone,
            fight3_zone = f.Fight3Zone,
            bo_quiz = f.boQUIZ,
            bo_noreconnect = f.boNORECONNECT,
            s_no_reconnect_map = f.sNoReConnectMap,
            bo_exprate = f.boEXPRATE,
            exp_rate = f.ExpRate,
            bo_pkwinlevel = f.boPKWINLEVEL,
            n_pkwinlevel = f.nPKWINLEVEL,
            bo_pkwinexp = f.boPKWINEXP,
            n_pkwinexp = f.nPKWINEXP,
            bo_pklostlevel = f.boPKLOSTLEVEL,
            n_pklostlevel = f.nPKLOSTLEVEL,
            bo_pklostexp = f.boPKLOSTEXP,
            n_pklostexp = f.nPKLOSTEXP,
            bo_dechp = f.boDECHP,
            n_dechppoint = f.nDECHPPOINT,
            n_dechptime = f.nDECHPTIME,
            bo_inchp = f.boINCHP,
            n_inchppoint = f.nINCHPPOINT,
            n_inchptime = f.nINCHPTIME,
            bo_decgamegold = f.boDECGAMEGOLD,
            n_decgamegold = f.nDECGAMEGOLD,
            n_decgamegoldtime = f.nDECGAMEGOLDTIME,
            bo_decgamepoint = f.boDECGAMEPOINT,
            n_decgamepoint = f.nDECGAMEPOINT,
            n_decgamepointtime = f.nDECGAMEPOINTTIME,
            bo_incgamegold = f.boINCGAMEGOLD,
            n_incgamegold = f.nINCGAMEGOLD,
            n_incgamegoldtime = f.nINCGAMEGOLDTIME,
            bo_incgamepoint = f.boINCGAMEPOINT,
            n_incgamepoint = f.nINCGAMEPOINT,
            n_incgamepointtime = f.nINCGAMEPOINTTIME,
            run_human = f.RunHuman,
            run_mon = f.RunMon,
            bo_needhole = f.boNEEDHOLE,
            no_re_call = f.NoReCall,
            no_guild_re_call = f.NoGuildReCall,
            bo_nodearrecall = f.boNODEARRECALL,
            master_re_call = f.MasterReCall,
            bo_norandommove = f.boNORANDOMMOVE,
            bo_nodrug = f.boNODRUG,
            mine = f.Mine,
            bo_mine2 = f.boMINE2,
            bo_nopositionmove = f.boNOPOSITIONMOVE,
            no_drop_item = f.NoDropItem,
            no_throw_item = f.NoThrowItem,
            no_horse = f.NoHorse,
            bo_nochat = f.boNOCHAT,
            bo_killfunc = f.boKILLFUNC,
            n_killfuncno = f.nKILLFUNCNO,
            bo_nohumnomon = f.boNOHUMNOMON,
        };

        private static int MapInfoMode(string path, string outPath)
        {
            var loadList = new StringList();
            loadList.LoadFromFile(path);

            string sFlag = string.Empty;
            string sCommand = string.Empty;
            string sLine = string.Empty;
            string sReConnectMap = string.Empty;

            // `Maps.cs:38-51`：第一遍扫 ConnectMapInfo，把子文件内容追加到表尾。
            int count = 0;
            while (true)
            {
                if (count >= loadList.Count)
                {
                    break;
                }
                if (HUtil32.CompareLStr("ConnectMapInfo", loadList[count]))
                {
                    string sMapInfoFile = HUtil32.GetValidStr3(loadList[count], ref sFlag, new[] { ' ', '\t' });
                    loadList.RemoveAt(count);
                    if (!string.IsNullOrEmpty(sMapInfoFile))
                    {
                        LoadSubMapInfo(loadList, Path.GetDirectoryName(Path.GetFullPath(path)) + Path.DirectorySeparatorChar, sMapInfoFile);
                    }
                }
                count++;
            }

            var sections = new List<object>();
            string sMapName;
            for (int i = 0; i < loadList.Count; i++)
            {
                sFlag = loadList[i];
                if (!string.IsNullOrEmpty(sFlag) && sFlag[0] == '[')
                {
                    sMapName = string.Empty;
                    MapInfoFlag MapFlag = new MapInfoFlag
                    {
                        SafeArea = false
                    };
                    sFlag = HUtil32.ArrestStringEx(sFlag, "[", "]", ref sMapName);
                    string sMapDesc = HUtil32.GetValidStrCap(sMapName, ref sMapName, HUtil32.Separator);
                    if (!string.IsNullOrEmpty(sMapDesc) && sMapDesc[0] == '\"')
                    {
                        HUtil32.ArrestStringEx(sMapDesc, "\"", "\"", ref sMapDesc);
                    }
                    string s4C = HUtil32.GetValidStr3(sMapDesc, ref sMapDesc, HUtil32.Separator).Trim();
                    byte nServerIndex = (byte)HUtil32.StrToInt(s4C, 0);
                    if (string.IsNullOrEmpty(sMapName))
                    {
                        continue;
                    }
                    MapFlag.RequestLevel = 1;
                    MapFlag.SafeArea = false;
                    MapFlag.NeedSetonFlag = -1;
                    MapFlag.NeedOnOff = -1;
                    MapFlag.MusicId = -1;
                    while (true)
                    {
                        if (string.IsNullOrEmpty(sFlag))
                        {
                            break;
                        }
                        sFlag = HUtil32.GetValidStr3(sFlag, ref sCommand, HUtil32.Separator);
                        if (string.IsNullOrEmpty(sCommand))
                        {
                            break;
                        }
                        if (sCommand.Equals("SAFE", StringComparison.OrdinalIgnoreCase))
                        {
                            MapFlag.SafeArea = true;
                            continue;
                        }
                        if (string.Compare(sCommand, "DARK", StringComparison.OrdinalIgnoreCase) == 0)
                        {
                            MapFlag.boDarkness = true;
                            continue;
                        }
                        if (string.Compare(sCommand, "FIGHT", StringComparison.OrdinalIgnoreCase) == 0)
                        {
                            MapFlag.FightZone = true;
                            continue;
                        }
                        if (string.Compare(sCommand, "FIGHT3", StringComparison.OrdinalIgnoreCase) == 0)
                        {
                            MapFlag.Fight3Zone = true;
                            continue;
                        }
                        if (string.Compare(sCommand, "DAY", StringComparison.OrdinalIgnoreCase) == 0)
                        {
                            MapFlag.DayLight = true;
                            continue;
                        }
                        if (string.Compare(sCommand, "QUIZ", StringComparison.OrdinalIgnoreCase) == 0)
                        {
                            MapFlag.boQUIZ = true;
                            continue;
                        }
                        if (HUtil32.CompareLStr(sCommand, "NORECONNECT"))
                        {
                            MapFlag.boNORECONNECT = true;
                            HUtil32.ArrestStringEx(sCommand, "(", ")", ref sReConnectMap);
                            MapFlag.sNoReConnectMap = sReConnectMap;
                            if (string.IsNullOrEmpty(MapFlag.sNoReConnectMap))
                            {
                            }
                            continue;
                        }
                        if (HUtil32.CompareLStr(sCommand, "CHECKQUEST"))
                        {
                            HUtil32.ArrestStringEx(sCommand, "(", ")", ref sLine);
                            // QuestNPC = LoadMapQuest(sLine);  // 探针不建 NPC，仅保持控制流
                            continue;
                        }
                        if (HUtil32.CompareLStr(sCommand, "NEEDSET_ON"))
                        {
                            MapFlag.NeedOnOff = 1;
                            HUtil32.ArrestStringEx(sCommand, "(", ")", ref sLine);
                            MapFlag.NeedSetonFlag = HUtil32.StrToInt(sLine, -1);
                            continue;
                        }
                        if (HUtil32.CompareLStr(sCommand, "NEEDSET_OFF"))
                        {
                            MapFlag.NeedOnOff = 0;
                            HUtil32.ArrestStringEx(sCommand, "(", ")", ref sLine);
                            MapFlag.NeedSetonFlag = HUtil32.StrToInt(sLine, -1);
                            continue;
                        }
                        if (HUtil32.CompareLStr(sCommand, "MUSIC"))
                        {
                            MapFlag.Music = true;
                            HUtil32.ArrestStringEx(sCommand, "(", ")", ref sLine);
                            MapFlag.MusicId = HUtil32.StrToInt(sLine, -1);
                            continue;
                        }
                        if (HUtil32.CompareLStr(sCommand, "EXPRATE"))
                        {
                            MapFlag.boEXPRATE = true;
                            HUtil32.ArrestStringEx(sCommand, "(", ")", ref sLine);
                            MapFlag.ExpRate = HUtil32.StrToInt(sLine, -1);
                            continue;
                        }
                        if (HUtil32.CompareLStr(sCommand, "PKWINLEVEL"))
                        {
                            MapFlag.boPKWINLEVEL = true;
                            HUtil32.ArrestStringEx(sCommand, "(", ")", ref sLine);
                            MapFlag.nPKWINLEVEL = HUtil32.StrToInt(sLine, -1);
                            continue;
                        }
                        if (HUtil32.CompareLStr(sCommand, "PKWINEXP"))
                        {
                            MapFlag.boPKWINEXP = true;
                            HUtil32.ArrestStringEx(sCommand, "(", ")", ref sLine);
                            MapFlag.nPKWINEXP = HUtil32.StrToInt(sLine, -1);
                            continue;
                        }
                        if (HUtil32.CompareLStr(sCommand, "PKLOSTLEVEL"))
                        {
                            MapFlag.boPKLOSTLEVEL = true;
                            HUtil32.ArrestStringEx(sCommand, "(", ")", ref sLine);
                            MapFlag.nPKLOSTLEVEL = HUtil32.StrToInt(sLine, -1);
                            continue;
                        }
                        if (HUtil32.CompareLStr(sCommand, "PKLOSTEXP"))
                        {
                            MapFlag.boPKLOSTEXP = true;
                            HUtil32.ArrestStringEx(sCommand, "(", ")", ref sLine);
                            MapFlag.nPKLOSTEXP = HUtil32.StrToInt(sLine, -1);
                            continue;
                        }
                        if (HUtil32.CompareLStr(sCommand, "DECHP"))
                        {
                            MapFlag.boDECHP = true;
                            HUtil32.ArrestStringEx(sCommand, "(", ")", ref sLine);
                            MapFlag.nDECHPPOINT = HUtil32.StrToInt(HUtil32.GetValidStr3(sLine, ref sLine, HUtil32.Backslash), -1);
                            MapFlag.nDECHPTIME = HUtil32.StrToInt(sLine, -1);
                            continue;
                        }
                        if (HUtil32.CompareLStr(sCommand, "INCHP"))
                        {
                            MapFlag.boINCHP = true;
                            HUtil32.ArrestStringEx(sCommand, "(", ")", ref sLine);
                            MapFlag.nINCHPPOINT = HUtil32.StrToInt(HUtil32.GetValidStr3(sLine, ref sLine, HUtil32.Backslash), -1);
                            MapFlag.nINCHPTIME = HUtil32.StrToInt(sLine, -1);
                            continue;
                        }
                        if (HUtil32.CompareLStr(sCommand, "DECGAMEGOLD"))
                        {
                            MapFlag.boDECGAMEGOLD = true;
                            HUtil32.ArrestStringEx(sCommand, "(", ")", ref sLine);
                            MapFlag.nDECGAMEGOLD = HUtil32.StrToInt(HUtil32.GetValidStr3(sLine, ref sLine, HUtil32.Backslash), -1);
                            MapFlag.nDECGAMEGOLDTIME = HUtil32.StrToInt(sLine, -1);
                            continue;
                        }
                        if (HUtil32.CompareLStr(sCommand, "DECGAMEPOINT"))
                        {
                            MapFlag.boDECGAMEPOINT = true;
                            HUtil32.ArrestStringEx(sCommand, "(", ")", ref sLine);
                            MapFlag.nDECGAMEPOINT = HUtil32.StrToInt(HUtil32.GetValidStr3(sLine, ref sLine, HUtil32.Backslash), -1);
                            MapFlag.nDECGAMEPOINTTIME = HUtil32.StrToInt(sLine, -1);
                            continue;
                        }
                        if (HUtil32.CompareLStr(sCommand, "INCGAMEGOLD"))
                        {
                            MapFlag.boINCGAMEGOLD = true;
                            HUtil32.ArrestStringEx(sCommand, "(", ")", ref sLine);
                            MapFlag.nINCGAMEGOLD = HUtil32.StrToInt(HUtil32.GetValidStr3(sLine, ref sLine, HUtil32.Backslash), -1);
                            MapFlag.nINCGAMEGOLDTIME = HUtil32.StrToInt(sLine, -1);
                            continue;
                        }
                        if (HUtil32.CompareLStr(sCommand, "INCGAMEPOINT"))
                        {
                            MapFlag.boINCGAMEPOINT = true;
                            HUtil32.ArrestStringEx(sCommand, "(", ")", ref sLine);
                            MapFlag.nINCGAMEPOINT = HUtil32.StrToInt(HUtil32.GetValidStr3(sLine, ref sLine, HUtil32.Backslash), -1);
                            MapFlag.nINCGAMEPOINTTIME = HUtil32.StrToInt(sLine, -1);
                            continue;
                        }
                        if (sCommand.Equals("RUNHUMAN", StringComparison.OrdinalIgnoreCase))
                        {
                            MapFlag.RunHuman = true;
                            continue;
                        }
                        if (sCommand.Equals("RUNMON", StringComparison.OrdinalIgnoreCase))
                        {
                            MapFlag.RunMon = true;
                            continue;
                        }
                        if (sCommand.Equals("NEEDHOLE", StringComparison.OrdinalIgnoreCase))
                        {
                            MapFlag.boNEEDHOLE = true;
                            continue;
                        }
                        if (sCommand.Equals("NORECALL", StringComparison.OrdinalIgnoreCase))
                        {
                            MapFlag.NoReCall = true;
                            continue;
                        }
                        if (sCommand.Equals("NOGUILDRECALL", StringComparison.OrdinalIgnoreCase))
                        {
                            MapFlag.NoGuildReCall = true;
                            continue;
                        }
                        if (sCommand.Equals("NODEARRECALL", StringComparison.OrdinalIgnoreCase))
                        {
                            MapFlag.boNODEARRECALL = true;
                            continue;
                        }
                        if (sCommand.Equals("NOMASTERRECALL", StringComparison.OrdinalIgnoreCase))
                        {
                            MapFlag.MasterReCall = true;
                            continue;
                        }
                        if (sCommand.Equals("NORANDOMMOVE", StringComparison.OrdinalIgnoreCase))
                        {
                            MapFlag.boNORANDOMMOVE = true;
                            continue;
                        }
                        if (sCommand.Equals("NODRUG", StringComparison.OrdinalIgnoreCase))
                        {
                            MapFlag.boNODRUG = true;
                            continue;
                        }
                        if (sCommand.Equals("MINE", StringComparison.OrdinalIgnoreCase))
                        {
                            MapFlag.Mine = true;
                            continue;
                        }
                        if (sCommand.Equals("MINE2", StringComparison.OrdinalIgnoreCase))
                        {
                            MapFlag.boMINE2 = true;
                            continue;
                        }
                        if (sCommand.Equals("NOTHROWITEM", StringComparison.OrdinalIgnoreCase))
                        {
                            MapFlag.NoThrowItem = true;
                            continue;
                        }
                        if (sCommand.Equals("NODROPITEM", StringComparison.OrdinalIgnoreCase))
                        {
                            MapFlag.NoDropItem = true;
                            continue;
                        }
                        if (sCommand.Equals("NOPOSITIONMOVE", StringComparison.OrdinalIgnoreCase))
                        {
                            MapFlag.boNOPOSITIONMOVE = true;
                            continue;
                        }
                        if (sCommand.Equals("NOHORSE", StringComparison.OrdinalIgnoreCase))
                        {
                            MapFlag.boNOPOSITIONMOVE = true;
                            continue;
                        }
                        if (sCommand.Equals("NOCHAT", StringComparison.OrdinalIgnoreCase))
                        {
                            MapFlag.boNOCHAT = true;
                            continue;
                        }
                        if (HUtil32.CompareLStr(sCommand, "KILLFUNC"))
                        {
                            MapFlag.boKILLFUNC = true;
                            HUtil32.ArrestStringEx(sCommand, "(", ")", ref sLine);
                            MapFlag.nKILLFUNCNO = HUtil32.StrToInt(sLine, -1);
                            continue;
                        }
                        if (HUtil32.CompareLStr(sCommand, "NOHUMNOMON"))
                        {
                            // 有人才开始刷怪
                            MapFlag.boNOHUMNOMON = true;
                            continue;
                        }
                        if (sCommand[0] == 'L')
                        {
                            MapFlag.RequestLevel = HUtil32.StrToInt(sCommand[1..], 1);
                        }
                    }

                    // `MapManager.AddMapInfo`（MapManager.cs:64-96）的名/文件名拆分。
                    string sMapFileName = string.Empty;
                    string sTempName = sMapName;
                    if (sTempName.IndexOf('|') > -1)
                    {
                        sMapFileName = HUtil32.GetValidStr3(sTempName, ref sMapName, '|');
                    }
                    else
                    {
                        sTempName = HUtil32.ArrestStringEx(sTempName, "<", ">", ref sMapFileName);
                        if (string.IsNullOrEmpty(sMapFileName))
                        {
                            sMapFileName = sMapName;
                        }
                        else
                        {
                            sMapName = sTempName;
                        }
                    }

                    sections.Add(new
                    {
                        line = i + 1,
                        map_name = sMapName,
                        map_file_name = sMapFileName,
                        map_desc = sMapDesc,
                        server_index = nServerIndex,
                        flag = FlagJson(MapFlag),
                    });
                }
            }

            WriteJson(outPath, new
            {
                file = path,
                sections,
            });
            Console.WriteLine($"sections={sections.Count}");
            return 0;
        }

        // -------------------------------------------------------------------- conf

        /// <summary>
        /// `src/Modules/SystemModule/Conf/Model/GameSvrConf.cs` 里相关字段的**初值**（逐字抄）。
        /// 读取点：`src/Modules/SystemModule/Conf/ServerConf.cs:108-109/116-118/165-224`。
        /// </summary>
        private static (NakedAbility warr, NakedAbility wizard, NakedAbility taos) BonusDefaults() => (
            new NakedAbility { DC = 17, MC = 20, SC = 20, AC = 20, MAC = 20, HP = 1, MP = 3, Hit = 20, Speed = 35, Reserved = 0 },
            new NakedAbility { DC = 17, MC = 25, SC = 30, AC = 20, MAC = 15, HP = 2, MP = 1, Hit = 25, Speed = 35, Reserved = 0 },
            new NakedAbility { DC = 20, MC = 30, SC = 17, AC = 20, MAC = 15, HP = 2, MP = 1, Hit = 30, Speed = 30, Reserved = 0 }
        );

        private static (NakedAbility warr, NakedAbility wizard, NakedAbility taos) NakedDefaults() => (
            new NakedAbility { DC = 512, MC = 2560, SC = 20, AC = 768, MAC = 1280, HP = 0, MP = 0, Hit = 0, Speed = 0, Reserved = 0 },
            new NakedAbility { DC = 512, MC = 512, SC = 2560, AC = 1280, MAC = 768, HP = 0, MP = 0, Hit = 5, Speed = 0, Reserved = 0 },
            new NakedAbility { DC = 20, MC = 30, SC = 17, AC = 20, MAC = 15, HP = 2, MP = 1, Hit = 30, Speed = 30, Reserved = 0 }
        );

        private static NakedAbility ReadBonus(ProbeConf c, string stem, NakedAbility def) => new NakedAbility
        {
            DC = c.U16("Setup", stem + "DC", def.DC),
            MC = c.U16("Setup", stem + "MC", def.MC),
            SC = c.U16("Setup", stem + "SC", def.SC),
            AC = c.U16("Setup", stem + "AC", def.AC),
            MAC = c.U16("Setup", stem + "MAC", def.MAC),
            HP = c.U16("Setup", stem + "HP", def.HP),
            MP = c.U16("Setup", stem + "MP", def.MP),
            Hit = c.U8("Setup", stem + "Hit", def.Hit),
            Speed = c.I("Setup", stem + "Speed", def.Speed),
            Reserved = c.U8("Setup", stem + "X2", def.Reserved),
        };

        private static object NakedJson(NakedAbility a) => new
        {
            dc = a.DC,
            mc = a.MC,
            sc = a.SC,
            ac = a.AC,
            mac = a.MAC,
            hp = a.HP,
            mp = a.MP,
            hit = a.Hit,
            speed = a.Speed,
            reserved = a.Reserved,
        };

        private static int ConfMode(string path, string outPath)
        {
            var c = new ProbeConf(path);
            var (bw, bwz, bt) = BonusDefaults();
            var (nw, nwz, nt) = NakedDefaults();
            var outObj = new
            {
                file = path,
                // ServerConf.cs:108-109（默认值 GameSvrConf.cs:1320-1321）
                game_gold_name = c.S("Names", "GameGold", "元宝"),
                game_point_name = c.S("Names", "GamePoint", "游戏点"),
                // ServerConf.cs:116-118（默认值 GameSvrConf.cs:1298-1300）
                client_file1 = c.S("Setup", "ClientFile1", "mir.1"),
                client_file2 = c.S("Setup", "ClientFile2", "mir.Dat"),
                client_file3 = c.S("Setup", "ClientFile3", "mir.3"),
                // `ClientFile*_CRC` 在 oracle 里只有声明（GameSvrConf.cs:1148-1150）与读取
                // （PlayObject.Message.cs:1690），**全仓无任何赋值点** ⇒ 恒为 int 默认 0。
                client_file1_crc = 0,
                client_file2_crc = 0,
                client_file3_crc = 0,
                // ServerConf.cs:165-224
                bonus_abil = new
                {
                    warr = NakedJson(ReadBonus(c, "BonusAbilofWarr", bw)),
                    wizard = NakedJson(ReadBonus(c, "BonusAbilofWizard", bwz)),
                    taos = NakedJson(ReadBonus(c, "BonusAbilofTaos", bt)),
                },
                naked_abil = new
                {
                    warr = NakedJson(ReadBonus(c, "NakedAbilofWarr", nw)),
                    wizard = NakedJson(ReadBonus(c, "NakedAbilofWizard", nwz)),
                    taos = NakedJson(ReadBonus(c, "NakedAbilofTaos", nt)),
                },
            };
            WriteJson(outPath, outObj);
            Console.WriteLine($"conf ok: gold={outObj.game_gold_name} point={outObj.game_point_name}");
            return 0;
        }

        // -------------------------------------------------------------- clientconf

        /// <summary>
        /// `EDCode.EncodeMessage(SystemShare.Config.ClientConf)` 的**序列化原文**（MemoryPack，
        /// 走 `SerializerUtil.Serialize`：`MemoryPackSerializer.Serialize(x, Utf16)`）。
        /// 默认值逐字抄自 `src/Modules/SystemModule/Conf/Model/GameSvrConf.cs:1866-1878`。
        /// </summary>
        private static int ClientConfMode(string outPath)
        {
            var conf = new ClientConf()
            {
                boGameAssist = true,
                boWhisperRecord = true,
                boMaketSystem = true,
                boNoFog = false,
                boStallSystem = true,
                boShowHpBar = true,
                boShowHpNumber = true,
                boNoStruck = false,
                boFastMove = false,
                boShowGameShop = true
            };
            byte[] data = MemoryPack.MemoryPackSerializer.Serialize(conf, MemoryPack.MemoryPackSerializerOptions.Utf16);
            // `EDCode.EncodeMessage<T>`（EDcode.cs:211-226）：`data[1..]`（丢掉 MemoryPack 的成员数字节）
            // 才是 `EncryptUtil.Encode(data, data.Length, encBuf, pos: 1)` 实际编码的那段。
            byte[] body = data[1..];
            int[] bodyInts = Array.ConvertAll(body, b => (int)b);
            int[] fullInts = Array.ConvertAll(data, b => (int)b);
            WriteJson(outPath, new
            {
                client_conf = bodyInts,
                len = body.Length,
                memorypack_full = fullInts,
                memorypack_full_len = data.Length,
            });
            Console.WriteLine($"clientconf len={body.Length} hex={Convert.ToHexString(body).ToLowerInvariant()}");
            return 0;
        }
    }
}
