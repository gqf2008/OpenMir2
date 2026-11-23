#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""zone_frame_order_check.py — A6「同玩家跨类帧序」判据（第 4 步窗口装置）。

判据（写死，header-only；**不用 body**）：
- 窗口：`frames.ndjson` 中 `phase == "gamegate"` 的帧。
- 标签：
  * `ident == 100(SM_SYSMESSAGE)` 且 `recog == 该跑 P 的 actor` 且 `param != 65499` ⇒ `gm#k`
    （第 k 条 `@` 命令的回复）。登录公告同样 ident=100/recog=P，但 `param == 65499`（实测全量 264 帧
    均此值）⇒ 用 param 排除；GM 回复的 param 是颜色字（`MakeWord(f,b)`）。**注意**：绿字 GM 回复与登录公告同色词
    （`param=0xFFDB=65499`），无法按 param 区分 ⇒ 本场景固定用 `@时间`（Blue，部署色 251/252，
    `param` ≠ 65499）；将来换命令必须重新确认色词。
  * `frame_class == "server_ack_time"` 且 `body_text` 含 `+GD/` ⇒ `move`（移动命令的 ack）。
  * 其它帧忽略。
- 压缩连续重复标签（同一命令可产多帧）；**golden 与 test 的标签序列必须相等**。
- **逻辑支点**：golden（N=1 单区）里 `@`（Process，当场执行）与 move（Local）同 tick 按到达序执行
  ⇒ golden 的标签序列就是 A6 的期望序 `[gm#1, gm#2, move]`；test 必须与之相等。
- 判不了就不给绿（rc=2）：文件缺失/解析失败、任一侧 `gm` 帧数 != `--expected-gm`（默认 2）、
  `move` 帧数 != 1。rc=1 只在**两侧都可判**且标签序列不等时给。
- `--precheck`：只断言 P 侧收到 `--expected-gm` 条 `gm` 帧（第 4 步实装缺口探测）。零帧 ⇒ rc=2 且
  `VERDICT=IMPLEMENTATION_GAP`；不是期望值 ⇒ rc=2 `UNDECIDABLE`。**不许**用其它帧凑数、
  也不许把判据放宽成"有几帧算几帧"。
- 自测：`--self-selftest`（合成夹具、临时目录；不读仓库状态）。
"""
import argparse, json, os, sys

GM_IDENT = 100
ACK_CLASS = "server_ack_time"
ACK_NEEDLE = "+GD/"
NOTICE_PARAM = 65499  # 登录公告的 ident=100 用 param=65499（实测 264 帧全此值）；GM 回复用颜色字


def load_frames(path):
    """读 frames.ndjson；返回 list 或 None（缺失/坏 JSON）。"""
    if not os.path.isfile(path):
        return None
    out = []
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                out.append(json.loads(line))
    except (OSError, ValueError):
        return None
    return out


def label_frames(frames, actor):
    """给 gamegate 窗口内的帧打标签；返回统计与压缩后的标签序列。"""
    tags = []
    gm_rows = []
    move_rows = []
    for fr in frames:
        if fr.get("phase") != "gamegate":
            continue
        if (fr.get("ident") == GM_IDENT and fr.get("recog") == actor
                and fr.get("param") != NOTICE_PARAM):
            gm_rows.append(fr)
            tags.append("gm#%d" % len(gm_rows))
        elif (fr.get("frame_class") == ACK_CLASS
              and ACK_NEEDLE in (fr.get("body_text") or "")):
            move_rows.append(fr)
            tags.append("move")
    comp = []
    for t in tags:
        if not comp or comp[-1] != t:
            comp.append(t)
    return {"tags": comp, "gm_frames": len(gm_rows), "move_frames": len(move_rows),
            "gm_rows": gm_rows, "move_rows": move_rows}


def _decide(res, expected_gm):
    if res["gm_frames"] != expected_gm:
        return 2, "gm_frames=%d(expected %d)" % (res["gm_frames"], expected_gm)
    if res["move_frames"] != 1:
        return 2, "move_frames=%d(expected 1)" % res["move_frames"]
    return 0, ""


def _row_brief(fr):
    return {k: fr.get(k) for k in ("seq", "t_ms", "ident", "recog", "param", "tag", "series")}


def analyze_order(g_frames, t_frames, golden_actor, test_actor, expected_gm):
    """返回 (rc, report)。rc=0 相等；1 首差；2 判不了。"""
    gr = label_frames(g_frames, golden_actor)
    tr = label_frames(t_frames, test_actor)
    rg, why_g = _decide(gr, expected_gm)
    rt, why_t = _decide(tr, expected_gm)
    rep = {"golden_tags": gr["tags"], "test_tags": tr["tags"],
           "golden_gm": gr["gm_frames"], "test_gm": tr["gm_frames"],
           "golden_move": gr["move_frames"], "test_move": tr["move_frames"]}
    if rg or rt:
        rep["error"] = "golden: %s; test: %s" % (why_g or "ok", why_t or "ok")
        return 2, rep
    if gr["tags"] == tr["tags"]:
        rep["verdict"] = "OK"
        return 0, rep
    pairs = [(i, x, y) for i, (x, y) in enumerate(zip(gr["tags"], tr["tags"])) if x != y]
    idx = pairs[0][0] if pairs else min(len(gr["tags"]), len(tr["tags"]))
    rep["verdict"] = "DIFF"
    rep["first_diff_index"] = idx
    lo = max(0, idx - 3)
    rep["golden_window"] = gr["tags"][lo:idx + 4]
    rep["test_window"] = tr["tags"][lo:idx + 4]
    all_rows_g = [("gm#%d" % i, r) for i, r in enumerate(gr["gm_rows"], 1)] + [("move", gr["move_rows"][0])]
    all_rows_t = [("gm#%d" % i, r) for i, r in enumerate(tr["gm_rows"], 1)] + [("move", tr["move_rows"][0])]
    rep["golden_frames"] = [[tag, _row_brief(fr)] for tag, fr in all_rows_g]
    rep["test_frames"] = [[tag, _row_brief(fr)] for tag, fr in all_rows_t]
    return 1, rep


def analyze_precheck(frames, actor, expected_gm):
    """返回 (rc, report)。零帧 = IMPLEMENTATION_GAP。"""
    res = label_frames(frames, actor)
    n = res["gm_frames"]
    rep = {"gm_frames": n, "expected": expected_gm, "move_frames": res["move_frames"]}
    if n == expected_gm:
        rep["verdict"] = "OK"
        return 0, rep
    rep["verdict"] = "IMPLEMENTATION_GAP" if n == 0 else "UNDECIDABLE"
    return 2, rep


def _selftest():
    checks = []

    def ck(name, cond, detail=""):
        checks.append((name, bool(cond), detail))

    def f_gm(actor, seq=1, recog=None):
        return {"seq": seq, "t_ms": seq, "phase": "gamegate", "conn": "gamegate",
                "ident": GM_IDENT, "recog": actor if recog is None else recog,
                "param": 4452, "tag": 0, "series": 1, "frame_class": "protocol",
                "body_text": "当前日期时间: 2026-11-23 18:00:00"}

    def f_notice(seq=0):
        return {"seq": seq, "t_ms": seq, "phase": "gamegate", "conn": "gamegate",
                "ident": GM_IDENT, "recog": 0, "param": 65499, "tag": 0, "series": 1,
                "frame_class": "protocol", "body_text": "欢迎进入"}

    def f_move(seq=2):
        return {"seq": seq, "t_ms": seq, "phase": "gamegate", "conn": "gamegate",
                "ident": None, "recog": None, "param": None, "tag": None, "series": None,
                "frame_class": ACK_CLASS, "body_text": "+GD/123", "body_hex": "2b47442f313233"}

    def f_login(seq=-1):
        return {"seq": seq, "t_ms": seq, "phase": "login", "conn": "login",
                "ident": 529, "recog": 0, "body_text": "热血传奇/Idle/"}

    A = 4194326
    base = [f_notice(), f_gm(A, 3), f_gm(A, 4), f_move(5)]
    rc, rep = analyze_order(base, base, A, A, 2)
    ck("order_equal_green", rc == 0 and rep["golden_tags"] == ["gm#1", "gm#2", "move"],
       "rc=%d tags=%s" % (rc, rep.get("golden_tags")))
    swapped = [f_notice(), f_move(3), f_gm(A, 4), f_gm(A, 5)]
    rc, rep = analyze_order(base, swapped, A, A, 2)
    ck("order_swap_red", rc == 1 and rep.get("first_diff_index") == 0,
       "rc=%d idx=%s" % (rc, rep.get("first_diff_index")))
    missing = [f_notice(), f_gm(A, 3), f_move(4)]
    ck("order_missing_gm_rc2", analyze_order(base, missing, A, A, 2)[0] == 2, "")
    ck("order_empty_rc2", analyze_order([], [], A, A, 2)[0] == 2, "")
    ck("precheck_two_ok", analyze_precheck(base, A, 2)[0] == 0, "")
    rc, rep = analyze_precheck([f_notice(), f_move(2)], A, 2)
    ck("precheck_zero_gap", rc == 2 and rep["verdict"] == "IMPLEMENTATION_GAP",
       "rc=%d verdict=%s" % (rc, rep.get("verdict")))
    one = [f_notice(), f_gm(A, 3)]
    rc, rep = analyze_precheck(one, A, 2)
    ck("precheck_one_undecidable", rc == 2 and rep["verdict"] == "UNDECIDABLE",
       "rc=%d verdict=%s" % (rc, rep.get("verdict")))
    ck("login_notice_not_gm", label_frames([f_notice(), f_login()], A)["gm_frames"] == 0, "")
    # **判据陷阱牙**：公告与 GM 回复同 ident=100、同 recog=P，只差 param=65499。
    # 去掉 param 过滤时这里会数成 4 条 ⇒ 本条必须红。
    trap = [f_notice(1), f_notice(2), f_gm(A, 3), f_gm(A, 4), f_move(5)]
    rc, rep = analyze_order(base, trap, A, A, 2)
    ck("notice_param_65499_excluded",
       rc == 0 and rep["test_gm"] == 2 and rep["test_tags"] == ["gm#1", "gm#2", "move"],
       "rc=%d gm=%s tags=%s" % (rc, rep.get("test_gm"), rep.get("test_tags")))
    noise = [f_login(), f_notice(), f_gm(A, 3),
             {"seq": 9, "t_ms": 9, "phase": "gamegate", "conn": "gamegate",
              "ident": 11, "recog": A, "param": 329, "tag": 268, "series": 6,
              "frame_class": "protocol", "body_text": ""},
             f_gm(A, 4), f_move(5)]
    rc, rep = analyze_order(base, noise, A, A, 2)
    ck("noise_frames_ignored",
       rc == 0 and rep["test_tags"] == ["gm#1", "gm#2", "move"],
       "rc=%d tags=%s" % (rc, rep.get("test_tags")))

    expected = 10
    ok = all(c for _, c, _ in checks) and len(checks) == expected
    for name, cond, detail in checks:
        print("SELFTEST %s %s %s" % (name, "ok" if cond else "FAIL", detail))
    print("SELFTEST_META checks=%d expect=%d" % (len(checks), expected))
    if not ok:
        print("SELFTEST_FAIL")
        return 1
    print("SELFTEST_OK")
    return 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--golden-dir")
    ap.add_argument("--test-dir")
    ap.add_argument("--golden-actor", type=int, default=None,
                    help="golden 跑 P 的 actor id（角色归一后的原始 recog）")
    ap.add_argument("--test-actor", type=int, default=None)
    ap.add_argument("--expected-gm", type=int, default=2)
    ap.add_argument("--precheck-dir", help="只跑 precheck：断言该目录 frames.ndjson 里的 gm 帧数")
    ap.add_argument("--actor", type=int, default=None, help="precheck 用 P 的 actor id")
    ap.add_argument("--self-selftest", action="store_true")
    a = ap.parse_args()
    if a.self_selftest:
        sys.exit(_selftest())
    if a.expected_gm < 1:
        print(json.dumps({"error": "--expected-gm 必须 >= 1"}, ensure_ascii=False))
        sys.exit(2)
    if a.precheck_dir:
        if a.actor is None:
            print(json.dumps({"error": "--precheck-dir 需要 --actor"}, ensure_ascii=False))
            sys.exit(2)
        frames = load_frames(os.path.join(a.precheck_dir, "frames.ndjson"))
        if frames is None:
            print(json.dumps({"error": "frames.ndjson 缺失/解析失败",
                              "dir": a.precheck_dir}, ensure_ascii=False))
            sys.exit(2)
        rc, rep = analyze_precheck(frames, a.actor, a.expected_gm)
        rep["mode"] = "precheck"
        print("A6_PRECHECK " + json.dumps(rep, ensure_ascii=False))
        sys.exit(rc)
    if not (a.golden_dir and a.test_dir
            and a.golden_actor is not None and a.test_actor is not None):
        print(json.dumps({"error": "--golden-dir/--test-dir/--golden-actor/--test-actor 必填"},
                         ensure_ascii=False))
        sys.exit(2)
    gf = load_frames(os.path.join(a.golden_dir, "frames.ndjson"))
    tf = load_frames(os.path.join(a.test_dir, "frames.ndjson"))
    if gf is None or tf is None:
        print(json.dumps({"error": "frames.ndjson 缺失/解析失败",
                          "golden_ok": gf is not None, "test_ok": tf is not None},
                         ensure_ascii=False))
        sys.exit(2)
    rc, rep = analyze_order(gf, tf, a.golden_actor, a.test_actor, a.expected_gm)
    rep["mode"] = "order"
    print("A6_ORDER " + json.dumps(rep, ensure_ascii=False))
    sys.exit(rc)


if __name__ == "__main__":
    main()
