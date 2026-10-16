using System;
using System.Collections.Generic;

namespace BotSrv
{
    /// <summary>
    /// M3 RNG 场景驱动器（卡 `openmir2-m3-rng-scene-driver`）的 BotSrv 侧自检
    /// （白名单 **T-59** 家族扩展；默认关，不新增任何发送/日志/文件）。
    ///
    /// 覆盖的是**生产同一函数**：`MShare.ParseSceneSteps`（固定动作序列解析）、
    /// `MShare.SceneStepOp/SceneStepParam`（分解）、`MShare.SceneMonsterKey`（固定怪规范化键）、
    /// `MShare.BuildSceneMonsterEntry` / `MShare.BuildSceneMonstersJson`（`monsters` 数组组装）、
    /// `MShare.BuildSceneDescriptorLine`（场景描述件一行 JSON，canon `m3-rng-scene-descriptor-v1`）。
    /// 改坏任一条 ⇒ 必 RED（`rng-scene/run_rng_scene_pair.ps1 -SelfCheckNoStack` / 承接卡窗口项按 rc 判红）。
    ///
    /// 入口：`BotSrv.exe --selftest-sceneseq`（不启动任何服务/不连端口；PASS ⇒ exit 0，任一用例不符 ⇒ exit 1）。
    /// </summary>
    internal static class SceneSeqSelfCheck
    {
        public static int Run()
        {
            int failed = 0;

            void Check(string name, bool ok, string detail)
            {
                if (!ok) { failed++; }
                Console.WriteLine($"SCENESEQ {(ok ? "PASS" : "FAIL")} {name} {detail}");
            }

            // ① ParseSceneSteps：顺序保留 / 空白与大小写归一 / 未知 op 丢弃 / 字段数不对丢弃 / param 可空。
            //    注意 param 内**不得含 `|`**（切分键；含则整步按字段数不对丢弃 —— fail-closed）。
            string[] parsed = MShare.ParseSceneSteps(
                " pin_monster |稻草人@18,20,0 ; IDLE|1000 ; z|frobnicate|x ; pin_monster ; player_pose|17,20,2 ; idle| ");
            Check("parse-order-and-filter",
                parsed.Length == 4
                && parsed[0] == "pin_monster|稻草人@18,20,0"
                && parsed[1] == "idle|1000"
                && parsed[2] == "player_pose|17,20,2"
                && parsed[3] == "idle|",
                "got=[" + string.Join(",", parsed) + "]");
            Check("parse-empty", MShare.ParseSceneSteps("").Length == 0, "empty");
            Check("parse-null", MShare.ParseSceneSteps(null).Length == 0, "null");
            Check("parse-unknown-op-dropped", MShare.ParseSceneSteps("z|frobnicate|x").Length == 0, "op-frobnicate");
            Check("parse-two-field-dropped", MShare.ParseSceneSteps("pin_monster").Length == 0, "one-field");
            Check("parse-three-field-dropped", MShare.ParseSceneSteps("a|b|c").Length == 0, "three-field");
            Check("parse-all-ops",
                MShare.ParseSceneSteps("pin_monster|a;player_pose|b;idle|c").Length == 3, "count-3");

            // ② SceneStepOp/Param 分解（坏串 ⇒ 空）。
            Check("split-op", MShare.SceneStepOp("pin_monster|稻草人") == "pin_monster", "op");
            Check("split-param", MShare.SceneStepParam("idle|600") == "600", "param");
            Check("split-param-missing", MShare.SceneStepParam("idle") == "", "missing-param");
            Check("split-bad", MShare.SceneStepOp("") == "" && MShare.SceneStepOp(null) == "", "empty/null");

            // ③ SceneMonsterKey：`name@x,y,dir`（两侧场景描述件 monsters 多重集的键）。
            Check("monster-key", MShare.SceneMonsterKey("稻草人", 18, 20, 0) == "稻草人@18,20,0", "key");
            Check("monster-key-null-name", MShare.SceneMonsterKey(null, 18, 20, 0) == "@18,20,0", "null-name");
            Check("monster-key-dir-distinct",
                MShare.SceneMonsterKey("稻草人", 18, 20, 0) != MShare.SceneMonsterKey("稻草人", 18, 20, 1), "dir");

            // ④ BuildSceneMonsterEntry / BuildSceneMonstersJson：JSON 合法 + 空 ⇒ `[]` + 名字转义。
            string one = MShare.BuildSceneMonsterEntry("稻草人", 18, 20, 0, 1);
            bool oneOk = false;
            try
            {
                using (var doc = System.Text.Json.JsonDocument.Parse(one))
                {
                    var r = doc.RootElement;
                    oneOk = r.GetProperty("name").GetString() == "稻草人"
                        && r.GetProperty("x").GetInt32() == 18
                        && r.GetProperty("y").GetInt32() == 20
                        && r.GetProperty("dir").GetInt32() == 0
                        && r.GetProperty("count").GetInt32() == 1;
                }
            }
            catch (System.Text.Json.JsonException) { oneOk = false; }
            Check("entry-json-ok", oneOk, "one=" + one);
            Check("monsters-empty", MShare.BuildSceneMonstersJson(new List<string>()) == "[]", "empty");
            Check("monsters-null", MShare.BuildSceneMonstersJson(null) == "[]", "null");
            string two = MShare.BuildSceneMonstersJson(new List<string> {
                MShare.BuildSceneMonsterEntry("稻草人", 18, 20, 0, 1),
                MShare.BuildSceneMonsterEntry("鸡", 19, 20, 0, 1)
            });
            bool twoOk = false;
            try
            {
                using (var doc = System.Text.Json.JsonDocument.Parse(two))
                {
                    twoOk = doc.RootElement.GetArrayLength() == 2
                        && doc.RootElement[1].GetProperty("name").GetString() == "鸡";
                }
            }
            catch (System.Text.Json.JsonException) { twoOk = false; }
            Check("monsters-multi-ok", twoOk, "two=" + two);
            // 字面 `<`/`>` 不得原样进 JSON（Serilog 模板分词缺陷，同 DropExpEsc 口径）。
            string esc = MShare.BuildSceneMonstersJson(new List<string> {
                MShare.BuildSceneMonsterEntry("<x>", 1, 2, 0, 1)
            });
            Check("monsters-no-raw-angle", esc.IndexOf('<') < 0 && esc.IndexOf('>') < 0 && esc.Contains("\\u003c"), "esc=" + esc);

            // ⑤ BuildSceneDescriptorLine：一行 JSON，字段齐全 + canon/side/seed/pose/monsters 合法。
            //    注意 `monsters` 必须是**数组** JSON（`BuildSceneMonstersJson` 的产物），不是单个条目对象。
            string monArr = MShare.BuildSceneMonstersJson(new List<string> { one });
            string desc = MShare.BuildSceneDescriptorLine("csharp", "0137", 42, 17, 20, 2, monArr);
            bool descOk = false;
            try
            {
                using (var doc = System.Text.Json.JsonDocument.Parse(desc))
                {
                    var r = doc.RootElement;
                    descOk = r.GetProperty("canon").GetString() == "m3-rng-scene-descriptor-v1"
                        && r.GetProperty("side").GetString() == "csharp"
                        && r.GetProperty("map").GetString() == "0137"
                        && r.GetProperty("seed").GetInt32() == 42
                        && r.GetProperty("start_pose").GetProperty("x").GetInt32() == 17
                        && r.GetProperty("start_pose").GetProperty("y").GetInt32() == 20
                        && r.GetProperty("start_pose").GetProperty("dir").GetInt32() == 2
                        && r.GetProperty("mongen_rows").GetArrayLength() == 0
                        && r.GetProperty("monsters").GetArrayLength() == 1;
                }
            }
            catch (System.Text.Json.JsonException) { descOk = false; }
            Check("descriptor-line-ok", descOk, "desc=" + desc);
            // 空 monsters ⇒ `[]`（不是 null/缺字段）。
            string descEmpty = MShare.BuildSceneDescriptorLine("rust", "0137", 42, 17, 20, 2, "");
            bool descEmptyOk = descEmpty.Contains("\"monsters\":[]") && descEmpty.Contains("\"side\":\"rust\"");
            Check("descriptor-empty-monsters", descEmptyOk, "desc=" + descEmpty);
            // map 是**字符串**（`"0137"` 与 `137` 不得被当成同一个图）。
            Check("descriptor-map-is-string",
                MShare.BuildSceneDescriptorLine("rust", "0137", 42, 17, 20, 2, "").Contains("\"map\":\"0137\""), "map");

            // ⑥ 默认关：固定场景序列**默认不得生效**（不新增发送/日志/文件）。
            Check("default-off", MShare.g_boSceneSequence == false, "g_boSceneSequence=" + MShare.g_boSceneSequence);

            Console.WriteLine(failed == 0
                ? "SCENESEQ SELFTEST GREEN"
                : $"SCENESEQ SELFTEST RED ({failed} failed)");
            return failed == 0 ? 0 : 1;
        }
    }
}
