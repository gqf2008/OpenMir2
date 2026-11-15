using System;
using OpenMir2;
using OpenMir2.Packets.ClientPackets;
using Serilog;

namespace BotSrv
{
    /// <summary>
    /// 卡 `openmir2-botsrv-decode-compat`：`SM_ABILITY(52)` / `SM_SENDUSEITEMS(621)` 解码兼容的**无栈牙**。
    ///
    /// 入口：`BotSrv.exe --selftest-decode-compat`（不启动服务/不连端口；PASS ⇒ exit 0，任一用例不符 ⇒ exit 1）。
    ///
    /// 覆盖（生产同一入口）：
    ///   ① `AbilityCodec`：58B client-58 / 40B oracle-40 选路；Level=22/23 边界；
    ///   ② 旧路径负例（钉死「为什么必须换」）：`EDCode.DecodeBuffer&lt;Ability&gt;` 对 58B Level≥23 必抛；
    ///      对 Level≤22 与 40B **不抛但解成全零**（静默测量错误）；
    ///   ③ **静默全零牙**：全零 `Ability` 记录必须被 `AbilityCodec` 拒绝，并经 `RunPacketGuard`
    ///      计进 `recv_packet_other` —— 不得默默接受；
    ///   ④ `ClientItemCodec.Decode` 对 `EDCode.EncodeBuffer(ClientItem)` 往返一致；
    ///      旧 `EDCode.DecodeBuffer&lt;ClientItem&gt;` 必抛 "not registered in this provider"；
    ///   ⑤ 端到端接线：旧分派经 `RunPacketGuard` ⇒ `recv_packet_other` +1；新分派 ⇒ 0；
    ///   ⑥ 元断言：注入次数 == `recv_packet_other` 增量（计数牙本身有牙，不是空过）。
    /// </summary>
    internal static class DecodeCompatSelfCheck
    {
        public static int Run()
        {
            // 旧路径负例会被 `RunPacketGuard` catch 并 `LogService.Error(ex)`；自检不起 AppServer，
            // 需要一个无 sink 的 logger（与 ProbeCoordsSelfCheck 同法）。
            LogService.Logger = new LoggerConfiguration().CreateLogger();

            int failed = 0;
            void Check(string name, bool ok, string detail = "")
            {
                if (!ok)
                {
                    failed++;
                }
                Console.WriteLine($"DECODECOMPAT {(ok ? "PASS" : "FAIL")} {name}{(detail.Length > 0 ? " " + detail : "")}");
            }

            BotErrorCensus.ResetForSelftest();
            long otherBefore = BotErrorCensus.Get(BotErrorCensus.RecvPacketOther);
            long otherInjections = 0;

            // ---------- ① 58B client-58：Level=22 / 23 / 50 正解 ----------
            foreach (ushort level in new ushort[] { 22, 23, 50 })
            {
                byte[] raw = Client58(level);
                string body = EDCode.EncodeBuffer(raw, raw.Length);
                Ability got = AbilityCodec.Decode(body);
                Check(
                    $"client58-level{level}-fields",
                    got.Level == level && got.AC == 0 && got.DC == 0x0101 && got.HP == 19 && got.MP == 15
                        && got.MaxHP == 19 && got.MaxMP == 15 && got.Exp == 0 && got.MaxExp == 10
                        && got.Weight == 14 && got.MaxWeight == 50 && got.MaxWearWeight == 15
                        && got.MaxHandWeight == 12,
                    $"L={got.Level} DC={got.DC} HP={got.HP}/{got.MaxHP} MP={got.MP}/{got.MaxMP} " +
                    $"Exp={got.Exp}/{got.MaxExp} W={got.Weight}/{got.MaxWeight} " +
                    $"MaxWear={got.MaxWearWeight} MaxHand={got.MaxHandWeight}");

                // 旧 MemoryPack 路径：level 23/50 必抛；level 22 不抛但全零。
                bool oldThrew = false;
                string oldMsg = "";
                Ability oldValue = null;
                try
                {
                    oldValue = EDCode.DecodeBuffer<Ability>(body);
                }
                catch (Exception ex)
                {
                    oldThrew = true;
                    oldMsg = ex.Message;
                }
                if (level >= 23)
                {
                    Check(
                        $"old-path-58-level{level}-throws",
                        oldThrew && oldMsg.Contains("header maked as " + level),
                        oldThrew ? oldMsg : "old path did not throw");
                }
                else
                {
                    // 旧路径在 Level<=22 时**不抛**，但解出的对象错位/全零（Level 与真实值不符）。
                    // 不能只断言 all-zero：Level=22 时可能解出部分非零垃圾（Exp/Weight=3840 等）。
                    bool wrong = !oldThrew && (oldValue == null || oldValue.Level != level || oldValue.HP != 19);
                    Check(
                        $"old-path-58-level{level}-silent-wrong",
                        wrong,
                        oldThrew
                            ? "unexpected throw: " + oldMsg
                            : $"L={oldValue?.Level} HP={oldValue?.HP} allZero={AbilityCodec.IsAllZero(oldValue)}");
                }
            }

            // ---------- ② 40B oracle-40 正解；旧路径静默全零 ----------
            {
                byte[] raw = Oracle40(1);
                string body = EDCode.EncodeBuffer(raw, raw.Length);
                Ability got = AbilityCodec.Decode(body);
                Check(
                    "oracle40-level1-fields",
                    got.Level == 1 && got.DC == 0x0101 && got.HP == 19 && got.MP == 15
                        && got.MaxHP == 19 && got.MaxMP == 15 && got.Exp == 0 && got.MaxExp == 10
                        && got.Weight == 14 && got.MaxWeight == 50 && got.MaxWearWeight == 15
                        && got.MaxHandWeight == 12,
                    $"L={got.Level} DC={got.DC} HP={got.HP}/{got.MaxHP} MP={got.MP}/{got.MaxMP} " +
                    $"Exp={got.Exp}/{got.MaxExp} W={got.Weight}/{got.MaxWeight}");

                bool oldThrew = false;
                Ability oldValue = null;
                try
                {
                    oldValue = EDCode.DecodeBuffer<Ability>(body);
                }
                catch
                {
                    oldThrew = true;
                }
                Check(
                    "old-path-oracle40-silent-all-zero",
                    !oldThrew && oldValue != null && AbilityCodec.IsAllZero(oldValue),
                    oldThrew ? "unexpected throw" : $"L={oldValue?.Level} HP={oldValue?.HP}");
            }

            // ---------- ③ 静默全零牙：不接受全零；且经 RunPacketGuard 计数 ----------
            {
                byte[] blank = new byte[AbilityCodec.ClientLayoutSize];
                bool threw = false;
                string msg = "";
                try
                {
                    AbilityCodec.DecodeBytes(blank);
                }
                catch (Exception ex)
                {
                    threw = true;
                    msg = ex.Message;
                }
                Check("blank-client58-rejected", threw && msg.Contains("all-zero"), msg);

                string blankBody = EDCode.EncodeBuffer(blank, blank.Length);
                ClientManager.RunPacketGuard(() => AbilityCodec.Decode(blankBody));
                otherInjections++;
                Check(
                    "blank-guard-counted",
                    BotErrorCensus.Get(BotErrorCensus.RecvPacketOther) == otherBefore + otherInjections,
                    $"{BotErrorCensus.Get(BotErrorCensus.RecvPacketOther)} want {otherBefore + otherInjections}");
            }

            // ---------- ④ 旧分派 → RunPacketGuard ⇒ recv_packet_other +1 ----------
            {
                string body50 = EDCode.EncodeBuffer(Client58(50), AbilityCodec.ClientLayoutSize);
                ClientManager.RunPacketGuard(() => EDCode.DecodeBuffer<Ability>(body50));
                otherInjections++;
                Check(
                    "old-ability-dispatch-guard-counted",
                    BotErrorCensus.Get(BotErrorCensus.RecvPacketOther) == otherBefore + otherInjections,
                    $"{BotErrorCensus.Get(BotErrorCensus.RecvPacketOther)} want {otherBefore + otherInjections}");

                ClientItem ci = new ClientItem();
                ci.Item.Name = "木剑";
                ci.Item.StdMode = 5;
                ci.Item.Shape = 1;
                ci.Item.Weight = 3;
                ci.Item.AniCount = 0;
                ci.MakeIndex = 12345;
                ci.Dura = 10;
                ci.DuraMax = 20;
                string ciBody = EDCode.EncodeBuffer(ci);

                // 旧 ClientItem 分派：必抛 "not registered in this provider"。
                bool oldItemThrew = false;
                string oldItemMsg = "";
                try
                {
                    EDCode.DecodeBuffer<ClientItem>(ciBody);
                }
                catch (Exception ex)
                {
                    oldItemThrew = true;
                    oldItemMsg = ex.Message;
                }
                Check(
                    "old-clientitem-dispatch-throws",
                    oldItemThrew && oldItemMsg.Contains("is not registered in this provider"),
                    oldItemThrew ? oldItemMsg : "old path did not throw");

                ClientManager.RunPacketGuard(() => EDCode.DecodeBuffer<ClientItem>(ciBody));
                otherInjections++;
                Check(
                    "old-clientitem-dispatch-guard-counted",
                    BotErrorCensus.Get(BotErrorCensus.RecvPacketOther) == otherBefore + otherInjections,
                    $"{BotErrorCensus.Get(BotErrorCensus.RecvPacketOther)} want {otherBefore + otherInjections}");

                // 新分派：124B 往返一致，不计数。
                ClientItem back = ClientItemCodec.Decode(ciBody);
                Check(
                    "new-clientitem-roundtrip",
                    back != null && back.Item.Name == "木剑" && back.Item.StdMode == 5
                        && back.MakeIndex == 12345 && back.Dura == 10 && back.DuraMax == 20,
                    back == null ? "null" : $"name={back.Item.Name} mode={back.Item.StdMode} mk={back.MakeIndex} dura={back.Dura}/{back.DuraMax}");
                ClientManager.RunPacketGuard(() =>
                {
                    ClientItem ok = ClientItemCodec.Decode(ciBody);
                    if (ok == null || ok.MakeIndex != 12345)
                    {
                        throw new InvalidOperationException("roundtrip");
                    }
                });
                Check(
                    "new-clientitem-dispatch-no-count",
                    BotErrorCensus.Get(BotErrorCensus.RecvPacketOther) == otherBefore + otherInjections,
                    $"{BotErrorCensus.Get(BotErrorCensus.RecvPacketOther)} want {otherBefore + otherInjections}");
            }

            // ---------- ⑤ 新 Ability 分派：经 guard 不计数 ----------
            {
                string body = EDCode.EncodeBuffer(Client58(50), AbilityCodec.ClientLayoutSize);
                ClientManager.RunPacketGuard(() =>
                {
                    Ability got = AbilityCodec.Decode(body);
                    if (got.Level != 50 || got.HP != 19)
                    {
                        throw new InvalidOperationException("ability roundtrip");
                    }
                });
                Check(
                    "new-ability-dispatch-no-count",
                    BotErrorCensus.Get(BotErrorCensus.RecvPacketOther) == otherBefore + otherInjections,
                    $"{BotErrorCensus.Get(BotErrorCensus.RecvPacketOther)} want {otherBefore + otherInjections}");
            }

            // ---------- ⑥ 元断言：注入次数非 0 且等于观测增量（计数牙不是空过） ----------
            Check(
                "meta-guard-count-wiring",
                otherInjections == 3 && BotErrorCensus.Get(BotErrorCensus.RecvPacketOther) == otherBefore + 3,
                $"injections={otherInjections} observed={BotErrorCensus.Get(BotErrorCensus.RecvPacketOther) - otherBefore}");

            Console.WriteLine($"DECODECOMPAT {(failed == 0 ? "ALL PASS" : "FAILED")} failures={failed}");
            return failed == 0 ? 0 : 1;
        }

        /// <summary>58B 冻结客户端 `TAbility`（`HIGHHP=True`）：字段值与 Rust 金标准 #12 同形。</summary>
        private static byte[] Client58(ushort level)
        {
            byte[] b = new byte[AbilityCodec.ClientLayoutSize];
            void W16(int off, ushort v)
            {
                b[off] = (byte)v;
                b[off + 1] = (byte)(v >> 8);
            }
            void W32(int off, uint v)
            {
                for (int i = 0; i < 4; i++)
                {
                    b[off + i] = (byte)(v >> (8 * i));
                }
            }
            W16(0, level);      // Level
            W32(2, 0);          // AC
            W32(6, 0);          // MAC
            W32(10, 0x0101);    // DC
            W32(14, 0);         // MC
            W32(18, 0);         // SC
            W32(22, 19);        // HP
            W32(26, 15);        // MP
            W32(30, 19);        // MaxHP
            W32(34, 15);        // MaxMP
            W32(38, 0);         // Exp
            W32(42, 10);        // MaxExp
            W16(46, 14);        // Weight
            W16(48, 50);        // MaxWeight
            W16(50, 0);         // WearWeight
            W16(52, 15);        // MaxWearWeight
            W16(54, 0);         // HandWeight
            W16(56, 12);        // MaxHandWeight
            return b;
        }

        /// <summary>40B oracle `EncodeMessage` 尾（MemoryPack 成员序、无对象头）。</summary>
        private static byte[] Oracle40(byte level)
        {
            byte[] b = new byte[AbilityCodec.OracleLayoutSize];
            void W16(int off, ushort v)
            {
                b[off] = (byte)v;
                b[off + 1] = (byte)(v >> 8);
            }
            void W32(int off, uint v)
            {
                for (int i = 0; i < 4; i++)
                {
                    b[off + i] = (byte)(v >> (8 * i));
                }
            }
            b[0] = level;       // Level
            b[1] = 0;           // Reserved1
            W16(2, 0);          // AC
            W16(4, 0);          // MAC
            W16(6, 0x0101);     // DC
            W16(8, 0);          // MC
            W16(10, 0);         // SC
            W16(12, 19);        // HP
            W16(14, 15);        // MP
            W16(16, 19);        // MaxHP
            W16(18, 15);        // MaxMP
            W16(20, 0);         // Reserved2
            b[22] = 0;          // ExpCount
            b[23] = 0;          // ExpMaxCount
            W32(24, 0);         // Exp
            W32(28, 10);        // MaxExp
            W16(32, 14);        // Weight
            W16(34, 50);        // MaxWeight
            b[36] = 0;          // WearWeight
            b[37] = 15;         // MaxWearWeight
            b[38] = 0;          // HandWeight
            b[39] = 12;         // MaxHandWeight
            return b;
        }
    }
}
