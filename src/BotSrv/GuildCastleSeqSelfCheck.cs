using System;
using System.Collections.Generic;

namespace BotSrv
{
    /// <summary>
    /// M3⑧ 公会/攻城（guild / castle）夹具的自检（白名单 T-46 家族扩展；卡 `openmir2-m3-guild-castle-fixture-botsrv`）。
    ///
    /// 覆盖的是**生产同一函数**：`MShare.ParseGuildCastleSteps`（步骤串解析）、
    /// `MShare.IsGuildCastleMaster`（会长精确判定）、`MShare.GuildCastleRunsForRole`（角色过滤）、
    /// `MShare.GuildCastleStepRole/Op/Param`（分解）、`MShare.GuildCastleFrameSm`（帧 id 解析）、
    /// `MShare.ClassifyGuildCastleFrames`（分支归类）。
    /// 改坏任一条 ⇒ 必 RED（`run_guild_castle_pair.ps1 -SelfCheckNoStack` 第 ⑭ 项按 rc 判红）。
    ///
    /// 入口：`BotSrv.exe --selftest-guildcastle`（不启动任何服务/不连端口；PASS ⇒ exit 0，任一用例不符 ⇒ exit 1）。
    /// </summary>
    internal static class GuildCastleSeqSelfCheck
    {
        public static int Run()
        {
            int failed = 0;

            void Check(string name, bool ok, string detail)
            {
                if (!ok) { failed++; }
                Console.WriteLine($"GUILDCSEQ {(ok ? "PASS" : "FAIL")} {name} {detail}");
            }

            // ① ParseGuildCastleSteps：顺序保留 / 空白与大小写归一 / 未知 role/op 丢弃 / 字段数不对丢弃 / param 可空
            string[] parsed = MShare.ParseGuildCastleSteps(
                " x | open_dlg | ; A | HOME | ; z|ally|g ; a|frobnicate|x ; open_dlg ; b|wait| ; b|add_member|@peer ");
            Check("parse-order-and-filter",
                parsed.Length == 4
                && parsed[0] == "x|open_dlg|"
                && parsed[1] == "a|home|"
                && parsed[2] == "b|wait|"
                && parsed[3] == "b|add_member|@peer",
                "got=[" + string.Join(",", parsed) + "]");
            Check("parse-empty", MShare.ParseGuildCastleSteps("").Length == 0, "empty");
            Check("parse-null", MShare.ParseGuildCastleSteps(null).Length == 0, "null");
            Check("parse-unknown-role-dropped",
                MShare.ParseGuildCastleSteps("z|ally|g").Length == 0, "role-z");
            Check("parse-unknown-op-dropped",
                MShare.ParseGuildCastleSteps("a|frobnicate|x").Length == 0, "op-frobnicate");
            Check("parse-two-field-dropped",
                MShare.ParseGuildCastleSteps("a|open_dlg").Length == 0, "two-field");
            Check("parse-all-ops",
                MShare.ParseGuildCastleSteps(
                    "a|open_dlg|;a|home|;a|memberlist|;a|add_member|@peer;a|del_member|@peer;" +
                    "a|update_notice|x;a|update_rank|x;a|ally|g;a|break_ally|g;" +
                    "a|build_guild|@@buildguildnow;a|guild_war|@@guildwar;a|castle_apply|@requestcastlewarnow0;" +
                    "a|open_door|@openmaindoor;a|close_door|@closemaindoor;a|repair_door|@repairdoor;" +
                    "a|repair_wall|@repairwall;x|observe|;x|wait|1000;x|softclose|").Length == 19,
                "count-19");

            // ② IsGuildCastleMaster：精确相等（非 EndsWith）
            Check("master-exact", MShare.IsGuildCastleMaster("m3gc0", "m3gc0"), "exact");
            Check("master-not-ends", !MShare.IsGuildCastleMaster("m3gc10", "m3gc0"), "ends-with");
            Check("master-empty-config", !MShare.IsGuildCastleMaster("m3gc0", ""), "empty-cfg");
            Check("master-empty-login", !MShare.IsGuildCastleMaster("", "m3gc0"), "empty-login");

            // ③ GuildCastleRunsForRole：x=both / a=master / b=peer / 非法=false
            Check("runs-x", MShare.GuildCastleRunsForRole("x", true) && MShare.GuildCastleRunsForRole("x", false), "x");
            Check("runs-a", MShare.GuildCastleRunsForRole("a", true) && !MShare.GuildCastleRunsForRole("a", false), "a");
            Check("runs-b", !MShare.GuildCastleRunsForRole("b", true) && MShare.GuildCastleRunsForRole("b", false), "b");
            Check("runs-invalid", !MShare.GuildCastleRunsForRole("z", true) && !MShare.GuildCastleRunsForRole("", false), "z/empty");

            // ④ GuildCastleStepRole/Op/Param 分解
            Check("split-role", MShare.GuildCastleStepRole("a|open_dlg|") == "a", "role");
            Check("split-op", MShare.GuildCastleStepOp("a|open_dlg|") == "open_dlg", "op");
            Check("split-param", MShare.GuildCastleStepParam("b|ally|盟重省") == "盟重省", "param");
            Check("split-bad", MShare.GuildCastleStepRole("x") == "", "bad");

            // ⑤ GuildCastleFrameSm：解析 `"sm":N`（含负数），缺/非法 ⇒ null
            Check("framesm-ok", MShare.GuildCastleFrameSm("{\"sm\":753,\"a\":0}") == 753, "753");
            Check("framesm-neg", MShare.GuildCastleFrameSm("{\"sm\":-1,\"a\":0}") == -1, "neg");
            Check("framesm-missing", MShare.GuildCastleFrameSm("{\"a\":0}") == null, "missing");
            Check("framesm-nonint", MShare.GuildCastleFrameSm("{\"sm\":x,\"a\":0}") == null, "nonint");
            Check("framesm-empty", MShare.GuildCastleFrameSm(null) == null && MShare.GuildCastleFrameSm("") == null, "null/empty");

            // ⑥ ClassifyGuildCastleFrames：帧种类 → branch（失败优先；不靠 JSON 子串误判）
            Check("branch-no-frame",
                MShare.ClassifyGuildCastleFrames(new List<string>()) == "fail_no_frame", "empty");
            Check("branch-null",
                MShare.ClassifyGuildCastleFrames(null) == "fail_no_frame", "null");
            Check("branch-ok-dlg",
                MShare.ClassifyGuildCastleFrames(new List<string> { "{\"sm\":753,\"a\":0}" }) == "ok", "753");
            Check("branch-ok-memberlist",
                MShare.ClassifyGuildCastleFrames(new List<string> { "{\"sm\":756,\"a\":0}" }) == "ok", "756");
            Check("branch-ok-addmember",
                MShare.ClassifyGuildCastleFrames(new List<string> { "{\"sm\":757,\"a\":0}" }) == "ok", "757");
            Check("branch-ok-build",
                MShare.ClassifyGuildCastleFrames(new List<string> { "{\"sm\":762,\"a\":0}" }) == "ok", "762");
            Check("branch-ok-ally",
                MShare.ClassifyGuildCastleFrames(new List<string> { "{\"sm\":768,\"a\":0}" }) == "ok", "768");
            Check("branch-fail-dlgfail",
                MShare.ClassifyGuildCastleFrames(new List<string> { "{\"sm\":754,\"a\":0}" }) == "fail_reject", "754");
            Check("branch-fail-addmember",
                MShare.ClassifyGuildCastleFrames(new List<string> { "{\"sm\":758,\"a\":3}" }) == "fail_reject", "758");
            Check("branch-fail-build",
                MShare.ClassifyGuildCastleFrames(new List<string> { "{\"sm\":763,\"a\":-2}" }) == "fail_reject", "763");
            Check("branch-fail-rmbuild",
                MShare.ClassifyGuildCastleFrames(new List<string> { "{\"sm\":10304,\"a\":-1}" }) == "fail_reject", "10304");
            Check("branch-ok-rmbuildok",
                MShare.ClassifyGuildCastleFrames(new List<string> { "{\"sm\":10303,\"a\":0}" }) == "ok", "10303");
            Check("branch-ok-menu",
                MShare.ClassifyGuildCastleFrames(new List<string> { "{\"sm\":767,\"a\":0,\"text\":\"[成功]\"}" }) == "ok", "767-ok");
            Check("branch-fail-menu",
                MShare.ClassifyGuildCastleFrames(new List<string> { "{\"sm\":767,\"a\":0,\"text\":\"[错误:] 行会不存在\"}" }) == "fail_reject", "767-fail");
            Check("branch-unclassified",
                MShare.ClassifyGuildCastleFrames(new List<string> { "{\"sm\":999,\"a\":0}" }) == "frames_unclassified", "999");
            // 失败优先于成功（同一步里先收成功再收失败 ⇒ 仍判 fail_reject）
            Check("branch-fail-priority",
                MShare.ClassifyGuildCastleFrames(new List<string> {
                    "{\"sm\":757,\"a\":0}",
                    "{\"sm\":758,\"a\":2}"
                }) == "fail_reject", "ok-then-fail");
            // 数字前缀不得误命中（753 vs 7530；762 vs 7620）
            Check("branch-no-prefix-collision",
                MShare.ClassifyGuildCastleFrames(new List<string> { "{\"sm\":7530,\"a\":0}" }) == "frames_unclassified",
                "7530");

            Console.WriteLine(failed == 0
                ? "GUILDCSEQ SELFTEST GREEN"
                : $"GUILDCSEQ SELFTEST RED ({failed} failed)");
            return failed == 0 ? 0 : 1;
        }
    }
}
