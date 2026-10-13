using System;
using System.Collections.Generic;

namespace BotSrv
{
    /// <summary>
    /// M3⑨ 英雄/宠物（宝宝/下属 slave）夹具的自检（白名单 T-45 家族扩展；卡 `openmir2-m3-hero-pet-fixture-botsrv`）。
    ///
    /// 覆盖的是**生产同一函数**：`MShare.ParseHeroPetSteps`（步骤串解析）、
    /// `MShare.IsHeroPetMaster`（主人精确判定）、`MShare.HeroPetRunsForRole`（角色过滤）、
    /// `MShare.HeroPetStepRole/Op/Param`（分解）、`MShare.HeroPetFrameSm`（帧 id 解析）、
    /// `MShare.ClassifyHeroPetFrames`（分支归类）。
    /// 改坏任一条 ⇒ 必 RED（`run_hero_pet_pair.ps1 -SelfCheckNoStack` 第 ⑭ 项按 rc 判红）。
    ///
    /// 入口：`BotSrv.exe --selftest-heropet`（不启动任何服务/不连端口；PASS ⇒ exit 0，任一用例不符 ⇒ exit 1）。
    /// </summary>
    internal static class HeroPetSeqSelfCheck
    {
        public static int Run()
        {
            int failed = 0;

            void Check(string name, bool ok, string detail)
            {
                if (!ok) { failed++; }
                Console.WriteLine($"HEROPETSEQ {(ok ? "PASS" : "FAIL")} {name} {detail}");
            }

            // ① ParseHeroPetSteps：顺序保留 / 空白与大小写归一 / 未知 role/op 丢弃 / 字段数不对丢弃 / param 可空
            string[] parsed = MShare.ParseHeroPetSteps(
                " x | summon |17 ; A|OBSERVE| ; z|attack|2 ; a|frobnicate|x ; summon ; b|attack|2 ; b|wait| ");
            Check("parse-order-and-filter",
                parsed.Length == 4
                && parsed[0] == "x|summon|17"
                && parsed[1] == "a|observe|"
                && parsed[2] == "b|attack|2"
                && parsed[3] == "b|wait|",
                "got=[" + string.Join(",", parsed) + "]");
            Check("parse-empty", MShare.ParseHeroPetSteps("").Length == 0, "empty");
            Check("parse-null", MShare.ParseHeroPetSteps(null).Length == 0, "null");
            Check("parse-unknown-role-dropped",
                MShare.ParseHeroPetSteps("z|attack|2").Length == 0, "role-z");
            Check("parse-unknown-op-dropped",
                MShare.ParseHeroPetSteps("a|frobnicate|x").Length == 0, "op-frobnicate");
            Check("parse-two-field-dropped",
                MShare.ParseHeroPetSteps("a|summon").Length == 0, "two-field");
            Check("parse-all-five",
                MShare.ParseHeroPetSteps(
                    "a|summon|17;b|summon|30;a|attack|2;x|observe|;x|wait|1000;x|softclose|").Length == 6,
                "count-6");

            // ② IsHeroPetMaster：精确相等（非 EndsWith）
            Check("master-exact", MShare.IsHeroPetMaster("m3hp0", "m3hp0"), "exact");
            Check("master-not-ends", !MShare.IsHeroPetMaster("m3hp10", "m3hp0"), "ends-with");
            Check("master-empty-config", !MShare.IsHeroPetMaster("m3hp0", ""), "empty-cfg");
            Check("master-empty-login", !MShare.IsHeroPetMaster("", "m3hp0"), "empty-login");

            // ③ HeroPetRunsForRole：x=both / a=master / b=peer / 非法=false
            Check("runs-x", MShare.HeroPetRunsForRole("x", true) && MShare.HeroPetRunsForRole("x", false), "x");
            Check("runs-a", MShare.HeroPetRunsForRole("a", true) && !MShare.HeroPetRunsForRole("a", false), "a");
            Check("runs-b", !MShare.HeroPetRunsForRole("b", true) && MShare.HeroPetRunsForRole("b", false), "b");
            Check("runs-invalid", !MShare.HeroPetRunsForRole("z", true) && !MShare.HeroPetRunsForRole("", false), "z/empty");

            // ④ HeroPetStepRole/Op/Param 分解
            Check("split-role", MShare.HeroPetStepRole("a|summon|17") == "a", "role");
            Check("split-op", MShare.HeroPetStepOp("a|summon|17") == "summon", "op");
            Check("split-param", MShare.HeroPetStepParam("b|wait|1000") == "1000", "param");
            Check("split-bad", MShare.HeroPetStepRole("x") == "", "bad");

            // ⑤ HeroPetFrameSm：解析 `"sm":N`（含负数），缺/非法 ⇒ null
            Check("framesm-ok", MShare.HeroPetFrameSm("{\"sm\":656,\"a\":0}") == 656, "656");
            Check("framesm-neg", MShare.HeroPetFrameSm("{\"sm\":-1,\"a\":0}") == -1, "neg");
            Check("framesm-missing", MShare.HeroPetFrameSm("{\"a\":0}") == null, "missing");
            Check("framesm-nonint", MShare.HeroPetFrameSm("{\"sm\":x,\"a\":0}") == null, "nonint");
            Check("framesm-empty", MShare.HeroPetFrameSm(null) == null && MShare.HeroPetFrameSm("") == null, "null/empty");

            // ⑥ ClassifyHeroPetFrames：帧种类 → branch（失败文案优先；不靠 JSON 子串误判）
            Check("branch-no-frame",
                MShare.ClassifyHeroPetFrames(new List<string>()) == "fail_no_frame", "empty");
            Check("branch-null",
                MShare.ClassifyHeroPetFrames(null) == "fail_no_frame", "null");
            Check("branch-ok-spell",
                MShare.ClassifyHeroPetFrames(new List<string> { "{\"sm\":17,\"a\":0}" }) == "ok", "17");
            Check("branch-ok-death",
                MShare.ClassifyHeroPetFrames(new List<string> { "{\"sm\":32,\"a\":0}" }) == "ok", "32");
            Check("branch-ok-winexp",
                MShare.ClassifyHeroPetFrames(new List<string> { "{\"sm\":44,\"a\":0}" }) == "ok", "44");
            Check("branch-ok-namecolor",
                MShare.ClassifyHeroPetFrames(new List<string> { "{\"sm\":656,\"a\":0}" }) == "ok", "656");
            Check("branch-ok-menu",
                MShare.ClassifyHeroPetFrames(new List<string> { "{\"sm\":767,\"a\":0,\"text\":\"[成功]\"}" }) == "ok", "767-ok");
            Check("branch-fail-menu",
                MShare.ClassifyHeroPetFrames(new List<string> { "{\"sm\":767,\"a\":0,\"text\":\"[错误:] 召唤失败\"}" }) == "fail_reject", "767-fail");
            Check("branch-unclassified",
                MShare.ClassifyHeroPetFrames(new List<string> { "{\"sm\":999,\"a\":0}" }) == "frames_unclassified", "999");
            // 失败文案优先于成功帧（同一步里先收 [成功] 再收 [错误] ⇒ 仍判 fail_reject）
            Check("branch-fail-priority",
                MShare.ClassifyHeroPetFrames(new List<string> {
                    "{\"sm\":767,\"a\":0,\"text\":\"[成功]\"}",
                    "{\"sm\":767,\"a\":0,\"text\":\"[错误:] 召唤失败\"}"
                }) == "fail_reject", "ok-then-fail");
            // 数字前缀不得误命中（17 vs 170；656 vs 6560）
            Check("branch-no-prefix-collision",
                MShare.ClassifyHeroPetFrames(new List<string> { "{\"sm\":170,\"a\":0}" }) == "frames_unclassified",
                "170");

            Console.WriteLine(failed == 0
                ? "HEROPETSEQ SELFTEST GREEN"
                : $"HEROPETSEQ SELFTEST RED ({failed} failed)");
            return failed == 0 ? 0 : 1;
        }
    }
}
