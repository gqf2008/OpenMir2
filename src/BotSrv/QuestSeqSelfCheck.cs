using System;
using System.Collections.Generic;

namespace BotSrv
{
    /// <summary>
    /// M3⑥ 任务夹具的自检（白名单 T-41 家族扩展；卡 `openmir2-m3-quest-fixture-botsrv`）。
    ///
    /// 覆盖的是**生产同一函数**：`MShare.ParseQuestSteps`（步骤串解析）、
    /// `MShare.ClassifyQuestFrames`（分支归类）、`MShare.ExtractQuestSay`（说词抽取）。
    /// 改坏任一条 ⇒ 必 RED（`run_quest_pair.ps1 -SelfCheckNoStack` 第 ⑩ 项按 rc 判红）。
    ///
    /// 入口：`BotSrv.exe --selftest-questseq`（不启动任何服务/不连端口；PASS ⇒ exit 0，任一用例不符 ⇒ exit 1）。
    /// </summary>
    internal static class QuestSeqSelfCheck
    {
        public static int Run()
        {
            int failed = 0;

            void Check(string name, bool ok, string detail)
            {
                if (!ok) { failed++; }
                Console.WriteLine($"QUESTSEQ {(ok ? "PASS" : "FAIL")} {name} {detail}");
            }

            // ① ParseQuestSteps：顺序保留 / 空白与大小写归一 / 未知 op 丢弃 / 字段数不对丢弃 / 空 npc 丢弃
            string[] parsed = MShare.ParseQuestSteps(" click_npc | 任务管理 | @main ; DIALOG_SELECT|任务管理|@rw06 ; teleport|x|@y ; dialog_select|任务管理 ; dialog_select||@main ; dialog_select|任务管理| ");
            Check("parse-order-and-filter",
                parsed.Length == 3
                && parsed[0] == "click_npc|任务管理|@main"
                && parsed[1] == "dialog_select|任务管理|@rw06"
                && parsed[2] == "dialog_select|任务管理|@main",
                "got=[" + string.Join(",", parsed) + "]");
            Check("parse-empty", MShare.ParseQuestSteps("").Length == 0, "empty");
            Check("parse-null", MShare.ParseQuestSteps(null).Length == 0, "null");
            string[] allOps = MShare.ParseQuestSteps("click_npc|a|@m;dialog_select|a|@x;take|a|@x;give|a|@x;set|a|@x;check|a|@x");
            Check("parse-all-six", allOps.Length == 6, "count=" + allOps.Length);
            // 改坏必红：把未知 op 也保留（旧式 `AllowAny` 行为）⇒ 本条必 FAIL
            Check("parse-unknown-dropped",
                MShare.ParseQuestSteps("frobnicate|npc|@x").Length == 0, "unknown-kept?");
            Check("parse-label-default-main",
                MShare.ParseQuestSteps("click_npc|npc|")[0] == "click_npc|npc|@main", "label-default");

            // ② ClassifyQuestFrames：帧种类 → branch（互不顶替）
            Check("branch-say",
                MShare.ClassifyQuestFrames(new List<string> { "{\"sm\":643,\"text\":\"任务管理/确定开始吗\"}" }) == "ok_say",
                "643");
            Check("branch-messagebox",
                MShare.ClassifyQuestFrames(new List<string> { "{\"sm\":767,\"a\":0,\"text\":\"苍月岛的首饰店老板正在找人帮忙\"}" }) == "ok_messagebox",
                "767");
            Check("branch-no-frame",
                MShare.ClassifyQuestFrames(new List<string>()) == "fail_no_frame",
                "empty");
            Check("branch-null",
                MShare.ClassifyQuestFrames(null) == "fail_no_frame",
                "null");
            Check("branch-unclassified",
                MShare.ClassifyQuestFrames(new List<string> { "{\"sm\":999,\"a\":0}" }) == "frames_unclassified",
                "999");
            // messagebox 优先于 say（脚本 close+messagebox 的组合步）
            Check("branch-messagebox-priority",
                MShare.ClassifyQuestFrames(new List<string> {
                    "{\"sm\":643,\"text\":\"任务管理/x\"}",
                    "{\"sm\":767,\"a\":0,\"text\":\"y\"}"
                }) == "ok_messagebox",
                "767-after-643");

            // ③ ExtractQuestSay：首条 643 帧的 text；转义（\" \\ \n）；无 643 ⇒ null
            string say = MShare.ExtractQuestSay(new List<string> { "{\"sm\":643,\"text\":\"任务管理/确定开始[神秘洞口]任务吗？\"}" });
            Check("say-extract", say == "任务管理/确定开始[神秘洞口]任务吗？", "got=" + say);
            string sayEsc = MShare.ExtractQuestSay(new List<string> { "{\"sm\":643,\"text\":\"a\\\"b\\\\c\"}" });
            Check("say-extract-escapes", sayEsc == "a\"b\\c", "got=" + sayEsc);
            string sayNone = MShare.ExtractQuestSay(new List<string> { "{\"sm\":767,\"a\":0,\"text\":\"x\"}" });
            Check("say-none-null", sayNone == null, "got=" + (sayNone ?? "null"));
            string sayFirst = MShare.ExtractQuestSay(new List<string> {
                "{\"sm\":767,\"a\":0,\"text\":\"x\"}",
                "{\"sm\":643,\"text\":\"first\"}"
            });
            Check("say-first-643", sayFirst == "first", "got=" + sayFirst);

            Console.WriteLine(failed == 0
                ? "QUESTSEQ SELFTEST GREEN"
                : $"QUESTSEQ SELFTEST RED ({failed} failed)");
            return failed == 0 ? 0 : 1;
        }
    }
}
