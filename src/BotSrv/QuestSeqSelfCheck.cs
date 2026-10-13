using System;
using System.Collections.Generic;
using OpenMir2;
using Serilog;
using Serilog.Core;
using Serilog.Events;

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

            // ④ JSON 转义：字面 `<`/`>` → `\u003c`/`\u003e`（规避 Serilog 消息模板分词的丢括号缺陷；
            //    真机缺陷 defect_3：C# k=0 任务 frame 载荷 `{`=2/`}`=1，json.loads 失败）。
            const string angleText = "任务管理/<任务名称>/@rw01";
            string escapedQuest = BotSrv.Player.RobotPlayer.QuestEsc(angleText);
            Check("esc-angle-quest-no-raw",
                escapedQuest.IndexOf('<') < 0 && escapedQuest.IndexOf('>') < 0
                && escapedQuest.Contains("\\u003c") && escapedQuest.Contains("\\u003e"),
                "got=" + escapedQuest);
            string escapedShop = BotSrv.Player.RobotPlayer.ShopEsc(angleText);
            Check("esc-angle-shop-no-raw",
                escapedShop.IndexOf('<') < 0 && escapedShop.IndexOf('>') < 0
                && escapedShop.Contains("\\u003c") && escapedShop.Contains("\\u003e"),
                "got=" + escapedShop);
            // 往返：用转义后的文本造 k=0 frame（嵌套 `{}` + 尖括号）⇒ 合法 JSON 且用真正的
            //     JSON 解析器解码后说词还原（比较器侧走 json.loads，与此一致）。
            string angleFrame = "{\"sm\":643,\"merchant\":1,\"face\":0,\"tag\":0,\"series\":1,\"text\":\""
                + escapedQuest + "\"}";
            string decodedText = null;
            bool frameJsonOk = false;
            try
            {
                using (var doc = System.Text.Json.JsonDocument.Parse(angleFrame))
                {
                    decodedText = doc.RootElement.GetProperty("text").GetString();
                }
                frameJsonOk = true;
            }
            catch (System.Text.Json.JsonException)
            {
                frameJsonOk = false;
            }
            Check("esc-angle-frame-decode",
                frameJsonOk && decodedText == angleText,
                "json-ok=" + frameJsonOk + " decoded=" + (decodedText ?? "null"));

            // ④b 根因回归（走真实 Serilog 通路，用内联 sink 取回 RenderMessage）：
            //     固定载荷用**生产 escaper** 造；把转义改回去（不转义尖括号）⇒ Serilog 会丢末尾 `}`
            //     ⇒ `fixedCaptured != fixedPayload` ⇒ 本条必 RED。老路径的丢失同时打印作证。
            var captured = new List<string>();
            LogService.Logger = new LoggerConfiguration().MinimumLevel.Debug()
                .WriteTo.Sink(new CollectingSink(captured)).CreateLogger();
            LogService.Info("{\"phase\":\"frame\",\"k\":0,\"frame\":{\"sm\":643,\"text\":\"<x>\"}}");
            string rawCaptured = captured[captured.Count - 1];
            string fixedPayload = "{\"phase\":\"frame\",\"k\":0,\"frame\":{\"sm\":643,\"text\":\""
                + BotSrv.Player.RobotPlayer.QuestEsc("<x>") + "\"}}";
            LogService.Info(fixedPayload);
            string fixedCaptured = captured[captured.Count - 1];
            Check("serilog-path-payload-intact",
                fixedCaptured == fixedPayload,
                "escaped-payload-via-serilog got=" + fixedCaptured);
            Console.WriteLine("QUESTSEQ INFO serilog-raw-path-mangled="
                + (rawCaptured != "{\"phase\":\"frame\",\"k\":0,\"frame\":{\"sm\":643,\"text\":\"<x>\"}}")
                + " raw=" + rawCaptured);

            // ④c reviewer-5 M-1（根因修法落在**日志出口**，不再只靠逐字段转义）：
            //     `RobotPlayer.SerilogTemplateLiteral` 把整条消息当字面渲染，`MainOutMessage` 已改走它。
            //     这样含嵌套 `{}` + 字面 `<`/`>` 的 M3*SEQ 载荷逐字节不变，且**新增字段自动安全**
            //     （reviewer-5 M-2 随之消解）。改坏必红：把出口的 `SerilogTemplateLiteral` 去掉
            //     ⇒ 下面的 raw-vs-literal 对照必挂（triggerRaw != triggerPayload）。
            Check("logexit-brace-escape",
                BotSrv.Player.RobotPlayer.SerilogTemplateLiteral("a{b}c{}") == "a{{b}}c{{}}",
                "got=" + BotSrv.Player.RobotPlayer.SerilogTemplateLiteral("a{b}c{}"));
            // 行为保持：正常标记（无 `<`/`>` 触发词）出口修前/修后渲染逐字节相同（不是顺手指改渲染）。
            const string normalPayload = "机器人:[bot1] M3QUESTSEQ {\"phase\":\"frame\",\"k\":0,\"frame\":{\"sm\":643,\"text\":\"任务管理/确定开始[神秘洞口]任务吗？\"}}";
            LogService.Info(normalPayload);
            string normalRaw = captured[captured.Count - 1];
            LogService.Info(BotSrv.Player.RobotPlayer.SerilogTemplateLiteral(normalPayload));
            string normalLiteral = captured[captured.Count - 1];
            Check("logexit-normal-marker-unchanged",
                normalLiteral == normalPayload && normalRaw == normalPayload,
                "literal-ok=" + (normalLiteral == normalPayload) + " raw-ok=" + (normalRaw == normalPayload));
            // 缺陷触发载荷（嵌套 `{}` + 字面 `<>`）：raw 出口被 Serilog 吞末尾 `}`，出口修法逐字节保留。
            const string triggerPayload = "机器人:[bot1] M3QUESTSEQ {\"phase\":\"frame\",\"k\":0,\"frame\":{\"sm\":643,\"text\":\"<x>\"}}";
            LogService.Info(triggerPayload);
            string triggerRaw = captured[captured.Count - 1];
            LogService.Info(BotSrv.Player.RobotPlayer.SerilogTemplateLiteral(triggerPayload));
            string triggerLiteral = captured[captured.Count - 1];
            Check("logexit-payload-intact",
                triggerLiteral == triggerPayload && triggerRaw != triggerPayload,
                "literal-ok=" + (triggerLiteral == triggerPayload) + " raw-mangled=" + (triggerRaw != triggerPayload));
            Console.WriteLine("QUESTSEQ INFO logexit raw-bytes=" + triggerRaw.Length
                + " literal-bytes=" + triggerLiteral.Length + " payload-bytes=" + triggerPayload.Length);

            Console.WriteLine(failed == 0
                ? "QUESTSEQ SELFTEST GREEN"
                : $"QUESTSEQ SELFTEST RED ({failed} failed)");
            return failed == 0 ? 0 : 1;
        }

        /// <summary>无栈自检用的内联 Serilog sink：取回事件渲染后的 Message。</summary>
        private sealed class CollectingSink : ILogEventSink
        {
            private readonly List<string> _items;

            public CollectingSink(List<string> items)
            {
                _items = items;
            }

            public void Emit(LogEvent logEvent)
            {
                _items.Add(logEvent.RenderMessage());
            }
        }
    }
}
