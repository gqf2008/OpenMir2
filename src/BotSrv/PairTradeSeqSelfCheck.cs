using System;
using System.Collections.Generic;

namespace BotSrv
{
    /// <summary>
    /// M3⑦ 组队/交易夹具的自检（白名单 T-43 家族扩展；卡 `openmir2-m3-party-trade-fixture-botsrv`）。
    ///
    /// 覆盖的是**生产同一函数**：`MShare.ParsePairTradeSteps`（步骤串解析）、
    /// `MShare.IsPairTradeActor`（actor 精确判定）、`MShare.PairTradeRunsForRole`（角色过滤）、
    /// `MShare.PairTradeStepRole/Op/Param`（分解）、`MShare.ClassifyPairTradeFrames`（分支归类）。
    /// 改坏任一条 ⇒ 必 RED（`run_party_trade_pair.ps1 -SelfCheckNoStack` 第 ⑩ 项按 rc 判红）。
    ///
    /// 入口：`BotSrv.exe --selftest-pairtrade`（不启动任何服务/不连端口；PASS ⇒ exit 0，任一用例不符 ⇒ exit 1）。
    /// </summary>
    internal static class PairTradeSeqSelfCheck
    {
        public static int Run()
        {
            int failed = 0;

            void Check(string name, bool ok, string detail)
            {
                if (!ok) { failed++; }
                Console.WriteLine($"PAIRTRADESEQ {(ok ? "PASS" : "FAIL")} {name} {detail}");
            }

            // ① ParsePairTradeSteps：顺序保留 / 空白与大小写归一 / 未知 role/op 丢弃 / 字段数不对丢弃 / param 可空
            string[] parsed = MShare.ParsePairTradeSteps(
                " x | group_mode_on | ; A|CREATE_GROUP|peer ; z|deal_try| ; a|frobnicate|x ; deal_end ; b|deal_chg_gold|-5 ; b|deal_chg_gold|0 ");
            Check("parse-order-and-filter",
                parsed.Length == 4
                && parsed[0] == "x|group_mode_on|"
                && parsed[1] == "a|create_group|peer"
                && parsed[2] == "b|deal_chg_gold|-5"
                && parsed[3] == "b|deal_chg_gold|0",
                "got=[" + string.Join(",", parsed) + "]");
            Check("parse-empty", MShare.ParsePairTradeSteps("").Length == 0, "empty");
            Check("parse-null", MShare.ParsePairTradeSteps(null).Length == 0, "null");
            Check("parse-unknown-role-dropped",
                MShare.ParsePairTradeSteps("z|deal_try|").Length == 0, "role-z");
            Check("parse-unknown-op-dropped",
                MShare.ParsePairTradeSteps("a|frobnicate|x").Length == 0, "op-frobnicate");
            Check("parse-two-field-dropped",
                MShare.ParsePairTradeSteps("a|deal_end").Length == 0, "two-field");
            Check("parse-all-eighteen",
                MShare.ParsePairTradeSteps(
                    "x|group_mode_on|;x|group_mode_off|;a|create_group|peer;a|add_member|peer;a|del_member|peer;" +
                    "a|deal_try|;a|deal_add_item|;a|deal_del_item|;a|deal_chg_gold|10;a|deal_end|;a|deal_cancel|;a|softclose|").Length == 12,
                "count-12");

            // ② IsPairTradeActor：精确相等（非 EndsWith）
            Check("actor-exact", MShare.IsPairTradeActor("m3p0", "m3p0"), "exact");
            Check("actor-not-ends", !MShare.IsPairTradeActor("m3p10", "m3p0"), "ends-with");
            Check("actor-empty-config", !MShare.IsPairTradeActor("m3p0", ""), "empty-cfg");
            Check("actor-empty-login", !MShare.IsPairTradeActor("", "m3p0"), "empty-login");

            // ③ PairTradeRunsForRole：x=both / a=actor / b=peer / 非法=false
            Check("runs-x", MShare.PairTradeRunsForRole("x", true) && MShare.PairTradeRunsForRole("x", false), "x");
            Check("runs-a", MShare.PairTradeRunsForRole("a", true) && !MShare.PairTradeRunsForRole("a", false), "a");
            Check("runs-b", !MShare.PairTradeRunsForRole("b", true) && MShare.PairTradeRunsForRole("b", false), "b");
            Check("runs-invalid", !MShare.PairTradeRunsForRole("z", true) && !MShare.PairTradeRunsForRole("", false), "z/empty");

            // ④ PairTradeStepRole/Op/Param 分解
            Check("split-role", MShare.PairTradeStepRole("a|create_group|peer") == "a", "role");
            Check("split-op", MShare.PairTradeStepOp("a|create_group|peer") == "create_group", "op");
            Check("split-param", MShare.PairTradeStepParam("b|deal_chg_gold|-5") == "-5", "param");
            Check("split-bad", MShare.PairTradeStepRole("x") == "", "bad");

            // ⑤ ClassifyPairTradeFrames：帧种类 → branch（失败优先；不靠 JSON 子串误判）
            Check("branch-no-frame",
                MShare.ClassifyPairTradeFrames(new List<string>()) == "fail_no_frame", "empty");
            Check("branch-null",
                MShare.ClassifyPairTradeFrames(null) == "fail_no_frame", "null");
            Check("branch-ok-mode",
                MShare.ClassifyPairTradeFrames(new List<string> { "{\"sm\":659,\"a\":1}" }) == "ok", "659");
            Check("branch-ok-create",
                MShare.ClassifyPairTradeFrames(new List<string> { "{\"sm\":660,\"a\":0}" }) == "ok", "660");
            Check("branch-ok-members",
                MShare.ClassifyPairTradeFrames(new List<string> { "{\"sm\":667,\"a\":0}" }) == "ok", "667");
            Check("branch-fail-reject",
                MShare.ClassifyPairTradeFrames(new List<string> { "{\"sm\":661,\"a\":-3}" }) == "fail_reject", "661");
            Check("branch-fail-deal",
                MShare.ClassifyPairTradeFrames(new List<string> { "{\"sm\":676,\"a\":0}" }) == "fail_reject", "676");
            Check("branch-unclassified",
                MShare.ClassifyPairTradeFrames(new List<string> { "{\"sm\":999,\"a\":0}" }) == "frames_unclassified", "999");
            // 失败帧优先于成功帧（同一步里先收 660 再收 661 ⇒ 仍判 fail_reject）
            Check("branch-fail-priority",
                MShare.ClassifyPairTradeFrames(new List<string> {
                    "{\"sm\":660,\"a\":0}",
                    "{\"sm\":661,\"a\":-3}"
                }) == "fail_reject", "660-then-661");
            // 数字前缀不得误命中（661 vs 6610；旧式 IndexOf("\"sm\":661") 子串口径会误判）
            Check("branch-no-prefix-collision",
                MShare.ClassifyPairTradeFrames(new List<string> { "{\"sm\":6610,\"a\":-3}" }) == "frames_unclassified",
                "6610");

            Console.WriteLine(failed == 0
                ? "PAIRTRADESEQ SELFTEST GREEN"
                : $"PAIRTRADESEQ SELFTEST RED ({failed} failed)");
            return failed == 0 ? 0 : 1;
        }
    }
}
