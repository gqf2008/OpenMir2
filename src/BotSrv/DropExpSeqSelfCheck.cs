using System;
using System.Collections.Generic;

namespace BotSrv
{
    /// <summary>
    /// M3④ 掉落/经验夹具的自检（白名单 T-22/T-29 家族扩展；卡 `openmir2-m3-drop-exp-fixture-botsrv`）。
    ///
    /// 覆盖的是**生产同一函数**：`MShare.IsDropExpAttacker`（attacker 精确选择）、
    /// `MShare.ClassifyDropExpFrames`（击杀窗口归类）、`MShare.ExtractDropExpTotal` /
    /// `MShare.ExtractDropExpDelta`（`SM_WINEXP(44)` 字段抽取，按原宽度不截断）、
    /// `MShare.BuildDropExpDropsJson` / `MShare.BuildDropExpRecord`（`drop_exp_raw.jsonl` 行组装，
    /// 必填字段 `k/gold/drops/exp_delta/exp_total/level`）。
    /// 改坏任一条 ⇒ 必 RED（`run_drop_exp_pair.ps1 -SelfCheckNoStack` 的 BotSrv 项按 rc 判红）。
    ///
    /// 入口：`BotSrv.exe --selftest-dropseq`（不启动任何服务/不连端口；PASS ⇒ exit 0，任一用例不符 ⇒ exit 1）。
    /// </summary>
    internal static class DropExpSeqSelfCheck
    {
        public static int Run()
        {
            int failed = 0;

            void Check(string name, bool ok, string detail)
            {
                if (!ok) { failed++; }
                Console.WriteLine($"DROPEXPSEQ {(ok ? "PASS" : "FAIL")} {name} {detail}");
            }

            // ① IsDropExpAttacker：精确相等（非 EndsWith；`-Bots>10` 时 10/20… 不得误命中）。
            var cases = new (string login, string role, bool want)[]
            {
                ("winplay0", "winplay0", true),
                ("winplay1", "winplay1", true),
                ("winplay10", "winplay0", false),
                ("winplay20", "winplay0", false),
                ("winplay1", "winplay10", false),
                ("winplay10", "winplay10", true),
                ("winplay0", "", false),
                ("", "winplay0", false),
            };
            foreach (var c in cases)
            {
                bool got = MShare.IsDropExpAttacker(c.login, c.role);
                Check("attacker-exact-" + c.login + "/" + c.role, got == c.want, $"got={got} want={c.want}");
            }

            // ② ClassifyDropExpFrames：44 ⇒ ok；无帧 ⇒ fail_no_frame；有帧无 44 ⇒ frames_unclassified。
            Check("branch-win-exp",
                MShare.ClassifyDropExpFrames(new List<string> { "{\"sm\":44,\"a\":120,\"b\":120,\"c\":0,\"d\":0}" }) == "ok",
                "sm=44");
            Check("branch-no-frame",
                MShare.ClassifyDropExpFrames(new List<string>()) == "fail_no_frame", "empty");
            Check("branch-null",
                MShare.ClassifyDropExpFrames(null) == "fail_no_frame", "null");
            Check("branch-only-itemshow",
                MShare.ClassifyDropExpFrames(new List<string> { "{\"sm\":610,\"a\":1,\"b\":17,\"c\":20,\"d\":0}" }) == "frames_unclassified",
                "only-610");
            Check("branch-44-priority",
                MShare.ClassifyDropExpFrames(new List<string> {
                    "{\"sm\":610,\"a\":1,\"b\":17,\"c\":20,\"d\":0}",
                    "{\"sm\":44,\"a\":120,\"b\":120,\"c\":0,\"d\":0}"
                }) == "ok",
                "610-then-44");
            // 数字前缀不得误命中（44 vs 610；`"sm":44` 子串口径会误判 440/441…）。
            Check("branch-no-sm-prefix-collision",
                MShare.ClassifyDropExpFrames(new List<string> { "{\"sm\":440,\"a\":1,\"b\":0,\"c\":0,\"d\":0}" }) == "frames_unclassified",
                "440");

            // ③ ExtractDropExpTotal / ExtractDropExpDelta：口径 = oracle `SM_WINEXP`
            //    （`a`=Recog 累计；`b`/`c` = Param/Tag 低/高 16 位 → `HUtil32.MakeLong`）。
            var win = new List<string> { "{\"sm\":610,\"a\":1,\"b\":17,\"c\":20,\"d\":0}", "{\"sm\":44,\"a\":120,\"b\":120,\"c\":0,\"d\":0}" };
            Check("total-basic", MShare.ExtractDropExpTotal(win) == 120, "got=" + MShare.ExtractDropExpTotal(win));
            Check("delta-basic", MShare.ExtractDropExpDelta(win) == 120, "got=" + MShare.ExtractDropExpDelta(win));
            Check("total-none-null", !MShare.ExtractDropExpTotal(new List<string> { "{\"sm\":610,\"a\":1,\"b\":0,\"c\":0,\"d\":0}" }).HasValue, "no-44");
            Check("delta-none-null", !MShare.ExtractDropExpDelta(new List<string>()).HasValue, "empty");
            // 原宽度：total 接近 int.MaxValue 不得截断。
            int bigTotal = 2000000000;
            var winBig = new List<string> { "{\"sm\":44,\"a\":" + bigTotal + ",\"b\":7,\"c\":0,\"d\":0}" };
            Check("total-not-truncated", MShare.ExtractDropExpTotal(winBig) == bigTotal,
                "got=" + MShare.ExtractDropExpTotal(winBig));
            // MakeLong 拼法：lo=0xffff(-1)、hi=0xffff(-1) ⇒ -1（有符号 32 位原宽度）。
            var winNeg = new List<string> { "{\"sm\":44,\"a\":0,\"b\":-1,\"c\":-1,\"d\":0}" };
            Check("delta-sign-makelong", MShare.ExtractDropExpDelta(winNeg) == -1,
                "got=" + MShare.ExtractDropExpDelta(winNeg));
            // lo=0x0100(256)、hi=0x0001(1) ⇒ (1<<16)|256 = 65792。
            var winHi = new List<string> { "{\"sm\":44,\"a\":0,\"b\":256,\"c\":1,\"d\":0}" };
            Check("delta-makelong-hi-lo", MShare.ExtractDropExpDelta(winHi) == 65792,
                "got=" + MShare.ExtractDropExpDelta(winHi));

            // ④ BuildDropExpDropsJson：空 ⇒ `[]`；字面 `<`/`>` ⇒ `\u003c`/`\u003e`（Serilog 模板分词缺陷）。
            Check("drops-empty", MShare.BuildDropExpDropsJson(new List<string>()) == "[]", "empty");
            Check("drops-null", MShare.BuildDropExpDropsJson(null) == "[]", "null");
            string dropsEsc = MShare.BuildDropExpDropsJson(new List<string> { "木剑", "<x>" });
            Check("drops-no-raw-angle",
                dropsEsc.IndexOf('<') < 0 && dropsEsc.IndexOf('>') < 0 && dropsEsc.Contains("\\u003c"),
                "got=" + dropsEsc);

            // ⑤ BuildDropExpRecord：必填字段齐全 + JSON 合法 + 大整数原宽度 + 与 drops 嵌套合法。
            string row = MShare.BuildDropExpRecord(3, "稻草人", 17, 20, 0, dropsEsc, bigTotal, bigTotal, 255);
            bool rowJsonOk = false;
            long rowExpTotal = -1, rowExpDelta = -1, rowLevel = -1;
            string rowMonster = null;
            try
            {
                using (var doc = System.Text.Json.JsonDocument.Parse(row))
                {
                    var root = doc.RootElement;
                    rowExpTotal = root.GetProperty("exp_total").GetInt64();
                    rowExpDelta = root.GetProperty("exp_delta").GetInt64();
                    rowLevel = root.GetProperty("level").GetInt64();
                    rowMonster = root.GetProperty("monster").GetString();
                    rowJsonOk = root.GetProperty("k").GetInt32() == 3
                        && root.GetProperty("gold").GetInt32() == 0
                        && root.GetProperty("drops").GetArrayLength() == 2;
                }
            }
            catch (System.Text.Json.JsonException)
            {
                rowJsonOk = false;
            }
            Check("record-json-ok", rowJsonOk, "json-ok=" + rowJsonOk + " row=" + row);
            Check("record-required-fields",
                rowExpTotal == bigTotal && rowExpDelta == bigTotal && rowLevel == 255 && rowMonster == "稻草人",
                $"total={rowExpTotal} delta={rowExpDelta} level={rowLevel} monster={rowMonster}");

            Console.WriteLine(failed == 0
                ? "DROPEXPSEQ SELFTEST GREEN"
                : $"DROPEXPSEQ SELFTEST RED ({failed} failed)");
            return failed == 0 ? 0 : 1;
        }
    }
}
