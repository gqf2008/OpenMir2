using System;
using System.Collections.Generic;

namespace BotSrv
{
    /// <summary>
    /// M3⑤ 商店夹具的自检（白名单 T-29 家族扩展；卡 `openmir2-m3-shop-fixture-botsrv`）。
    ///
    /// 覆盖的是**生产同一函数**：`MShare.ParseShopOps`（op 串解析）、
    /// `MShare.ClassifyShopFrames`（失败分支归因）、`MShare.ExtractShopPrice`（价格抽取）。
    /// 改坏任一条 ⇒ 必 RED（`run_npc_shop_pair.ps1 -SelfCheckNoStack` 第 ⑩ 项按 rc 判红）。
    ///
    /// 入口：`BotSrv.exe --selftest-shopseq`（不启动任何服务/不连端口；PASS ⇒ exit 0，任一用例不符 ⇒ exit 1）。
    /// </summary>
    internal static class ShopSeqSelfCheck
    {
        public static int Run()
        {
            int failed = 0;

            void Check(string name, bool ok, string detail)
            {
                if (!ok) { failed++; }
                Console.WriteLine($"SHOPSEQ {(ok ? "PASS" : "FAIL")} {name} {detail}");
            }

            // ① ParseShopOps：顺序保留 / 空白与大小写归一 / 未知 op 丢弃 / 空 ⇒ 空
            string[] parsed = MShare.ParseShopOps(" sell , BUY , teleport ,storage ");
            Check("parse-order-and-filter",
                parsed.Length == 3 && parsed[0] == "sell" && parsed[1] == "buy" && parsed[2] == "storage",
                "got=[" + string.Join(",", parsed) + "]");
            Check("parse-empty", MShare.ParseShopOps("").Length == 0, "empty");
            Check("parse-null", MShare.ParseShopOps(null).Length == 0, "null");
            string[] allOps = MShare.ParseShopOps("sell,buy,query_sell_price,query_repair_cost,repair,storage,takeback");
            Check("parse-all-seven", allOps.Length == 7, "count=" + allOps.Length);
            // 改坏必红：把未知 op 也保留（旧式 `AllowAny` 行为）⇒ 本条必 FAIL
            Check("parse-unknown-dropped",
                MShare.ParseShopOps("frobnicate").Length == 0, "unknown-kept?");

            // ② ClassifyShopFrames：每个失败码互不相同（不可互相顶替）
            Check("branch-nomoney",
                MShare.ClassifyShopFrames(new List<string> { "{\"sm\":651,\"a\":3,\"b\":0,\"c\":0,\"d\":0}" }) == "fail_nomoney",
                "651.a=3");
            Check("branch-overweight",
                MShare.ClassifyShopFrames(new List<string> { "{\"sm\":651,\"a\":2,\"b\":0,\"c\":0,\"d\":0}" }) == "fail_overweight",
                "651.a=2");
            Check("branch-sell-reject",
                MShare.ClassifyShopFrames(new List<string> { "{\"sm\":649,\"a\":0,\"b\":0,\"c\":0,\"d\":0}" }) == "fail_sell_reject",
                "649");
            Check("branch-storage-full",
                MShare.ClassifyShopFrames(new List<string> { "{\"sm\":702,\"a\":0,\"b\":0,\"c\":0,\"d\":0}" }) == "fail_storage_full",
                "702");
            Check("branch-bagfull",
                MShare.ClassifyShopFrames(new List<string> { "{\"sm\":707,\"a\":0,\"b\":0,\"c\":0,\"d\":0}" }) == "fail_bagfull",
                "707");
            Check("branch-ok-buy",
                MShare.ClassifyShopFrames(new List<string> { "{\"sm\":650,\"a\":0,\"b\":0,\"c\":0,\"d\":0}" }) == "ok",
                "650");
            Check("branch-no-frame",
                MShare.ClassifyShopFrames(new List<string>()) == "fail_no_frame",
                "empty");
            Check("branch-null",
                MShare.ClassifyShopFrames(null) == "fail_no_frame",
                "null");
            // 第一个决定性帧：nomoney 在前 ⇒ 即使后面有成功帧也归失败（决定性优先）
            Check("branch-first-decisive",
                MShare.ClassifyShopFrames(new List<string> {
                    "{\"sm\":651,\"a\":3,\"b\":0,\"c\":0,\"d\":0}",
                    "{\"sm\":648,\"a\":1,\"b\":0,\"c\":0,\"d\":0}"
                }) == "fail_nomoney",
                "first-decisive");
            Check("branch-unclassified",
                MShare.ClassifyShopFrames(new List<string> { "{\"sm\":999,\"a\":0,\"b\":0,\"c\":0,\"d\":0}" }) == "frames_unclassified",
                "999");

            // ③ ExtractShopPrice：647（卖价）/ 671（修理价）取 a；无帧 ⇒ null（不是 0）
            int? pSell = MShare.ExtractShopPrice(new List<string> { "{\"sm\":647,\"a\":123,\"b\":0,\"c\":0,\"d\":0}" });
            Check("price-sell", pSell == 123, "got=" + (pSell.HasValue ? pSell.Value.ToString() : "null"));
            int? pRepair = MShare.ExtractShopPrice(new List<string> { "{\"sm\":671,\"a\":45,\"b\":0,\"c\":0,\"d\":0}" });
            Check("price-repair", pRepair == 45, "got=" + (pRepair.HasValue ? pRepair.Value.ToString() : "null"));
            int? pNeg = MShare.ExtractShopPrice(new List<string> { "{\"sm\":647,\"a\":-1,\"b\":0,\"c\":0,\"d\":0}" });
            Check("price-negative", pNeg == -1, "got=" + (pNeg.HasValue ? pNeg.Value.ToString() : "null"));
            int? pNone = MShare.ExtractShopPrice(new List<string> { "{\"sm\":648,\"a\":9,\"b\":0,\"c\":0,\"d\":0}" });
            Check("price-none-null", !pNone.HasValue, "got=" + (pNone.HasValue ? pNone.Value.ToString() : "null"));

            // ④ reviewer-6 M-1 收口：`sm`/`a` 改**顶层 JSON 键解析**（旧子串口径的误判面）。
            // 以下 5 条在旧子串实现下必 RED：`a` 前缀（30 命中 3）、`sm` 前缀（6510 命中 651）、
            // 嵌套同名键（内层 651/3 误命中外层 648）、空白（`"sm": 651` 不含 `"sm":651`）、
            // 卖价帧空白（`"sm": 647` 不含 `"sm":647`）。
            Check("branch-field-reordered",
                MShare.ClassifyShopFrames(new List<string> { "{\"a\":3,\"d\":0,\"c\":0,\"b\":0,\"sm\":651}" }) == "fail_nomoney",
                "reordered");
            Check("branch-a-not-prefix",
                MShare.ClassifyShopFrames(new List<string> { "{\"sm\":651,\"a\":30,\"b\":0,\"c\":0,\"d\":0}" }) == "fail_buy_reject",
                "a=30-not-nomoney");
            Check("branch-nested-key-ignored",
                MShare.ClassifyShopFrames(new List<string> { "{\"sm\":648,\"a\":1,\"x\":{\"a\":3,\"sm\":651}}" }) == "ok",
                "nested-ignored");
            Check("branch-sm-prefix-not-match",
                MShare.ClassifyShopFrames(new List<string> { "{\"sm\":6510,\"a\":3,\"b\":0,\"c\":0,\"d\":0}" }) == "frames_unclassified",
                "6510-not-651");
            Check("branch-whitespace",
                MShare.ClassifyShopFrames(new List<string> { "{ \"sm\" : 651 , \"a\" : 3 }" }) == "fail_nomoney",
                "whitespace");
            Check("price-field-reordered",
                MShare.ExtractShopPrice(new List<string> { "{\"a\":77,\"sm\":671}" }) == 77,
                "reordered");
            Check("price-whitespace",
                MShare.ExtractShopPrice(new List<string> { "{ \"sm\" : 647 , \"a\" : 99 }" }) == 99,
                "whitespace");

            Console.WriteLine(failed == 0
                ? "SHOPSEQ SELFTEST GREEN"
                : $"SHOPSEQ SELFTEST RED ({failed} failed)");
            return failed == 0 ? 0 : 1;
        }
    }
}
