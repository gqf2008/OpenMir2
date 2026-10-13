using System;
using System.Collections.Generic;

namespace BotSrv
{
    /// <summary>
    /// M3⑩ 元宝寄售夹具的自检（白名单 T-44 家族扩展；卡 `openmir2-m3-selloff-fixture-botsrv`）。
    ///
    /// 覆盖的是**生产同一函数**：`MShare.ParseSellOffSteps`（步骤串解析）、
    /// `MShare.IsSellOffSeller`（卖方精确判定）、`MShare.SellOffRunsForRole`（角色过滤）、
    /// `MShare.SellOffStepRole/Op/Param`（分解）、`MShare.ClassifySellOffFrames`（分支归类）。
    /// 改坏任一条 ⇒ 必 RED（`run_selloff_pair.ps1 -SelfCheckNoStack` 第 ⑩ 项按 rc 判红）。
    ///
    /// 入口：`BotSrv.exe --selftest-selloff`（不启动任何服务/不连端口；PASS ⇒ exit 0，任一用例不符 ⇒ exit 1）。
    /// </summary>
    internal static class SellOffSeqSelfCheck
    {
        public static int Run()
        {
            int failed = 0;

            void Check(string name, bool ok, string detail)
            {
                if (!ok) { failed++; }
                Console.WriteLine($"SELLOFFSEQ {(ok ? "PASS" : "FAIL")} {name} {detail}");
            }

            // ① ParseSellOffSteps：顺序保留 / 空白与大小写归一 / 未知 role/op 丢弃 / 字段数不对丢弃 / param 可空
            string[] parsed = MShare.ParseSellOffSteps(
                " x | seller_add_item | ; A|SELLER_END|peer/1000 ; z|buyer_buy| ; a|frobnicate|x ; selloff_end ; b|buyer_buy|peer ; b|buyer_buy_cancel| ");
            Check("parse-order-and-filter",
                parsed.Length == 4
                && parsed[0] == "x|seller_add_item|"
                && parsed[1] == "a|seller_end|peer/1000"
                && parsed[2] == "b|buyer_buy|peer"
                && parsed[3] == "b|buyer_buy_cancel|",
                "got=[" + string.Join(",", parsed) + "]");
            Check("parse-empty", MShare.ParseSellOffSteps("").Length == 0, "empty");
            Check("parse-null", MShare.ParseSellOffSteps(null).Length == 0, "null");
            Check("parse-unknown-role-dropped",
                MShare.ParseSellOffSteps("z|buyer_buy|").Length == 0, "role-z");
            Check("parse-unknown-op-dropped",
                MShare.ParseSellOffSteps("a|frobnicate|x").Length == 0, "op-frobnicate");
            Check("parse-two-field-dropped",
                MShare.ParseSellOffSteps("a|seller_end").Length == 0, "two-field");
            Check("parse-all-eight",
                MShare.ParseSellOffSteps(
                    "x|seller_add_item|;a|seller_del_item|;a|seller_cancel|;a|seller_end|peer/1;" +
                    "a|seller_cancel_ing|;b|buyer_buy_cancel|peer;b|buyer_buy|peer;x|softclose|").Length == 8,
                "count-8");

            // ② IsSellOffSeller：精确相等（非 EndsWith）
            Check("seller-exact", MShare.IsSellOffSeller("m3s0", "m3s0"), "exact");
            Check("seller-not-ends", !MShare.IsSellOffSeller("m3s10", "m3s0"), "ends-with");
            Check("seller-empty-config", !MShare.IsSellOffSeller("m3s0", ""), "empty-cfg");
            Check("seller-empty-login", !MShare.IsSellOffSeller("", "m3s0"), "empty-login");

            // ③ SellOffRunsForRole：x=both / a=seller / b=buyer / 非法=false
            Check("runs-x", MShare.SellOffRunsForRole("x", true) && MShare.SellOffRunsForRole("x", false), "x");
            Check("runs-a", MShare.SellOffRunsForRole("a", true) && !MShare.SellOffRunsForRole("a", false), "a");
            Check("runs-b", !MShare.SellOffRunsForRole("b", true) && MShare.SellOffRunsForRole("b", false), "b");
            Check("runs-invalid", !MShare.SellOffRunsForRole("z", true) && !MShare.SellOffRunsForRole("", false), "z/empty");

            // ④ SellOffStepRole/Op/Param 分解
            Check("split-role", MShare.SellOffStepRole("a|seller_end|peer/1000") == "a", "role");
            Check("split-op", MShare.SellOffStepOp("a|seller_end|peer/1000") == "seller_end", "op");
            Check("split-param", MShare.SellOffStepParam("b|buyer_buy|peer") == "peer", "param");
            Check("split-bad", MShare.SellOffStepRole("x") == "", "bad");

            // ⑤ ClassifySellOffFrames：帧种类 → branch（失败优先；不靠 JSON 子串误判）
            Check("branch-no-frame",
                MShare.ClassifySellOffFrames(new List<string>()) == "fail_no_frame", "empty");
            Check("branch-null",
                MShare.ClassifySellOffFrames(null) == "fail_no_frame", "null");
            Check("branch-ok-additem",
                MShare.ClassifySellOffFrames(new List<string> { "{\"sm\":23003,\"a\":0}" }) == "ok", "23003");
            Check("branch-ok-end",
                MShare.ClassifySellOffFrames(new List<string> { "{\"sm\":23016,\"a\":0}" }) == "ok", "23016");
            Check("branch-ok-buy",
                MShare.ClassifySellOffFrames(new List<string> { "{\"sm\":23027,\"a\":0}" }) == "ok", "23027");
            Check("branch-ok-menu",
                MShare.ClassifySellOffFrames(new List<string> { "{\"sm\":767,\"a\":0}" }) == "ok", "767");
            Check("branch-fail-end",
                MShare.ClassifySellOffFrames(new List<string> { "{\"sm\":23018,\"a\":0}" }) == "fail_reject", "23018");
            Check("branch-fail-additem",
                MShare.ClassifySellOffFrames(new List<string> { "{\"sm\":23005,\"a\":0}" }) == "fail_reject", "23005");
            Check("branch-unclassified",
                MShare.ClassifySellOffFrames(new List<string> { "{\"sm\":999,\"a\":0}" }) == "frames_unclassified", "999");
            // 失败帧优先于成功帧（同一步里先收 23016 再收 23018 ⇒ 仍判 fail_reject）
            Check("branch-fail-priority",
                MShare.ClassifySellOffFrames(new List<string> {
                    "{\"sm\":23016,\"a\":0}",
                    "{\"sm\":23018,\"a\":0}"
                }) == "fail_reject", "23016-then-23018");
            // 数字前缀不得误命中（23005 vs 230050）
            Check("branch-no-prefix-collision",
                MShare.ClassifySellOffFrames(new List<string> { "{\"sm\":230050,\"a\":0}" }) == "frames_unclassified",
                "230050");

            Console.WriteLine(failed == 0
                ? "SELLOFFSEQ SELFTEST GREEN"
                : $"SELLOFFSEQ SELFTEST RED ({failed} failed)");
            return failed == 0 ? 0 : 1;
        }
    }
}
