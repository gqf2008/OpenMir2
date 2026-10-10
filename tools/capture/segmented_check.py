#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""segmented_check.py — C4 第三种布局（`BodyLayout::Segmented`）的段边界自检。

**为什么单独做这个自检**：GoldenExport 的分段切法要"改错段边界必红"才算判据，
但 A 线 A4 的表项落地之前，源码里派生不出 segmented 条目 —— 于是这里用**真实帧 + 冻结契约**
先把导出器这一段验穿：拿 C3 裸 dump 里的两帧真实分段帧，
按契约现造一张表导出，断言 body_len / 段数 / 逐段长度，与 A 线给的期望值逐一对上；
再把 seg_len 改错一格，断言导出器**非 0 退出**且 JSONL 里出现 `segmented(mismatch:…)`。
等 A4 表项落地后，本脚本自动改走"真表"（见 --table 逻辑），断言不变。

契约（A 线冻结）：
  811 = SM_ADJUST_BONUS  : seg_len=20, sep=b'/', trailing_sep=false, count=Fixed(3)   ⇒ body 3×20 = 60
  201 = SM_BAGITEMS      : seg_len=124, sep=b'/', trailing_sep=true,  count=HeaderSeries(=头里的 series=4)
                                                                                    ⇒ body 4×124 = 496
  体 = 各段明文按序拼接、不含分隔符。

用法：
  python tools/capture/segmented_check.py                     # 默认：派生真表 → 必要时补 synth → 导出 → 断言
  python tools/capture/segmented_check.py --exe <GoldenExport.exe> --c3-session <dump dir> --out <capture.jsonl>
退出码：0 = GREEN；非 0 = 有一条断言变红（每条都打印原因）。
"""
import argparse
import json
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))

# A 线冻结契约（也是本自检的期望值来源）
FROZEN = {
    811: {"kind": "segmented", "seg_len": 20, "sep": 47, "trailing_sep": False,
          "count_kind": "fixed", "count": 3, "name": "SM_ADJUST_BONUS"},
    201: {"kind": "segmented", "seg_len": 124, "sep": 47, "trailing_sep": True,
          "count_kind": "header_series", "name": "SM_BAGITEMS"},
}
EXPECT = {811: (60, [20, 20, 20]), 201: (496, [124, 124, 124, 124])}

# A 线冻结的 Rust 形状（原样抄，供派生器形状自检：证明"表项落地后能派生出来"，而不是"我以为能"）
FIXTURE_FRAME_RS = '''// 仅供派生器形状自检：与 A 线冻结契约同形（crates/protocol/src/frame.rs）
pub enum SegCount { Fixed(usize), HeaderSeries }

pub enum BodyLayout {
    Single,
    StructThenRest { struct_len: usize },
    Segmented { seg_len: usize, sep: u8, trailing_sep: bool, count: SegCount },
}

pub const STATUS_GOOD_PREFIX: &[u8] = b"+GD/";

const HEAD_BLOCK: usize = 16;

pub fn body_layout(ident: u16) -> BodyLayout {
    match ident {
        messages::SM_RUSH | messages::SM_RUSHKUNG => { BodyLayout::StructThenRest { struct_len: 8 } }
        messages::SM_ADJUST_BONUS => { BodyLayout::Segmented { seg_len: 20, sep: b'/', trailing_sep: false, count: SegCount::Fixed(3) } }
        messages::SM_BAGITEMS => { BodyLayout::Segmented { seg_len: 124, sep: b'/', trailing_sep: true, count: SegCount::HeaderSeries } }
        _ => BodyLayout::Single,
    }
}
'''

FIXTURE_MESSAGES_RS = '''pub const SM_RUSH: u16 = 6;
pub const SM_RUSHKUNG: u16 = 7;
pub const SM_ADJUST_BONUS: u16 = 811;
pub const SM_BAGITEMS: u16 = 201;
'''

FAILS = []


def fail(msg):
    FAILS.append(msg)
    print("RED: " + msg)


def check_layout_derivation(tmp):
    """步骤 0（派生侧形状自检）：拿 A 线冻结形状的 fixture 验 `dump_body_layout.py` 抓得对；
    再把字段名改一处，验它**报错**而不是静默少一档（免得表项落地后导出一张缺项的错表）。"""
    fx = os.path.join(tmp, "fixture")
    os.makedirs(fx, exist_ok=True)
    frame_rs = os.path.join(fx, "frame.rs")
    msgs_rs = os.path.join(fx, "messages.rs")
    open(frame_rs, "w", encoding="utf-8", newline="\n").write(FIXTURE_FRAME_RS)
    open(msgs_rs, "w", encoding="utf-8", newline="\n").write(FIXTURE_MESSAGES_RS)
    out = os.path.join(fx, "layout.json")

    def run(frame_path):
        return subprocess.run([sys.executable, os.path.join(HERE, "dump_body_layout.py"),
                               "--frame-rs", frame_path, "--constants", msgs_rs, "--out", out],
                              capture_output=True, text=True)

    p = run(frame_rs)
    if p.returncode != 0:
        fail("派生器抓不住冻结形状的表项：" + (p.stdout + p.stderr).strip()[-300:])
        return
    got = json.load(open(out, encoding="utf-8"))["layouts"]
    for ident, want in FROZEN.items():
        entry = dict(want)
        entry.setdefault("count_kind", "fixed")
        actual = got.get(str(ident))
        if not actual or actual.get("kind") != "segmented":
            fail(f"派生器把 ident={ident} 抓成了 {actual}")
            continue
        for k in ("seg_len", "sep", "trailing_sep", "count_kind", "count"):
            if k in entry and actual.get(k) != entry[k]:
                fail(f"派生器 ident={ident} 的 {k} = {actual.get(k)}（应为 {entry[k]}）")
    if all(got.get(str(i), {}).get("kind") == "segmented" for i in FROZEN):
        print("  PASS 0a 派生器按冻结形状抓出 %s"
              % ", ".join(f"{i}={got[str(i)]['seg_len']}B/{got[str(i)]['count_kind']}" for i in FROZEN))

    # 改名必红：字段名一变，派生器必须报错（不许静默少一档）
    bad_rs = os.path.join(fx, "frame_bad.rs")
    open(bad_rs, "w", encoding="utf-8", newline="\n").write(
        FIXTURE_FRAME_RS.replace("seg_len:", "segment_len:", 2))
    p2 = run(bad_rs)
    if p2.returncode == 0:
        fail("字段改名后派生器仍然成功（静默少一档 = 会导出缺项的错表）")
    elif "解析失败" not in (p2.stdout + p2.stderr):
        fail("字段改名后派生器报错了但错误信息里没有「解析失败」：" + (p2.stdout + p2.stderr)[-200:])
    else:
        print("  PASS 0b 字段改名 ⇒ 派生器报错（不静默）")


def derive_real_table(out_json):
    """从协议源码派生真表（走 dump_body_layout.py，唯一真值在 frame.rs）。"""
    cmd = [sys.executable, os.path.join(HERE, "dump_body_layout.py"),
           "--frame-rs", os.path.join(REPO, "mir2-rs", "crates", "protocol", "src", "frame.rs"),
           "--constants", os.path.join(REPO, "mir2-rs", "crates", "protocol", "src", "messages.rs"),
           "--out", out_json]
    p = subprocess.run(cmd, capture_output=True, text=True)
    if p.returncode != 0:
        fail("派生真表失败：" + (p.stdout + p.stderr).strip())
        return None
    return json.load(open(out_json, encoding="utf-8"))


def with_frozen(table, seg_len_override=None):
    """把冻结契约两项并进表（真表若已含其中一项，以真表为准；不一致就报红）。"""
    t = json.loads(json.dumps(table))
    for ident, spec in FROZEN.items():
        key = str(ident)
        if key in t["layouts"]:
            real = t["layouts"][key]
            if real.get("kind") != "segmented":
                fail(f"真表把 {ident} 归成了 {real.get('kind')}，与冻结契约（segmented）不一致")
            else:
                print(f"  [info] {ident} 真表已有 segmented 条目，用真表：{real}")
            continue
        t["layouts"][key] = dict(spec)
    if seg_len_override is not None:
        ident, bad = seg_len_override
        t["layouts"][str(ident)] = dict(t["layouts"][str(ident)], seg_len=bad)
    return t


def run_export(exe, session, out, layout, extra=()):
    cmd = [exe, "--session", session, "--out", out, "--layout", layout, *extra]
    p = subprocess.run(cmd, capture_output=True, text=True)
    return p.returncode, p.stdout + p.stderr


def load(path):
    return [json.loads(l) for l in open(path, encoding="utf-8") if l.strip()]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--exe", default=os.path.join(HERE, "GoldenExport", "bin", "Debug", "net8.0", "GoldenExport.exe"))
    ap.add_argument("--c3-session", default=os.path.join(REPO, "tests", "golden", "session-20261010-c3-hook", "proxy"))
    ap.add_argument("--out", default=None)
    args = ap.parse_args()

    if not os.path.exists(args.exe):
        print("先构建导出器：dotnet build tools/capture/GoldenExport/GoldenExport.csproj")
        return 2
    tmp = tempfile.mkdtemp(prefix="segcheck-")
    check_layout_derivation(tmp)          # 步骤 0：派生侧形状自检（表项还没落地也能先验）
    real_json = os.path.join(tmp, "layout_real.json")
    table = derive_real_table(real_json)
    if table is None:
        return 1

    # ---- 1) 正常路径：契约表导出，逐帧断言 ----
    synth = os.path.join(tmp, "layout_synth.json")
    json.dump(with_frozen(table), open(synth, "w", encoding="utf-8", newline="\n"),
              ensure_ascii=False, indent=2, sort_keys=True)
    out1 = args.out or os.path.join(tmp, "cap1.jsonl")
    rc, log = run_export(args.exe, args.c3_session, out1, synth, extra=("--verify-roundtrip",))
    print("  导出(契约表) rc=%d：%s" % (rc, [l for l in log.splitlines() if l.startswith(("LAYOUT", "FRAMES"))]))
    if rc != 0:
        fail(f"契约表导出退出码 {rc}（应为 0）\n{log}")
    if "roundtrip_fail=0" not in log:
        fail("契约表下 段回编佐证 未全过（--verify-roundtrip）：" +
             str([l for l in log.splitlines() if l.startswith("FRAMES")]))

    rows = load(out1) if os.path.exists(out1) else []
    for ident, (want_len, want_segs) in EXPECT.items():
        hit = [r for r in rows if r.get("ident") == ident]
        if not hit:
            fail(f"导出结果里没有 ident={ident} 的帧")
            continue
        for r in hit:
            got_len = r.get("body_len")
            got_segs = [s["len"] for s in r.get("body_segments", [])]
            got_kind = r.get("layout")
            if got_kind != "segmented":
                fail(f"ident={ident} layout={got_kind}（应为 segmented）")
            if (got_len, got_segs) != (want_len, want_segs):
                fail(f"ident={ident} body_len/segs = {got_len}/{got_segs}（应为 {want_len}/{want_segs}）")
            else:
                print(f"  PASS ident={ident} layout=segmented body_len={got_len} segs={got_segs}")

    # ---- 2) 线上结构独立佐证：分隔符确实落在"段边界"上（不靠解码器自证） ----
    # 811：rest = enc(20) + '/' + enc(20) + '/' + enc(20)，两个 '/' 应落在 27 和 55 位
    # （A-8 证据：payload 99 = 16 头 + 27 + '/' + 27 + '/' + 27）
    cap = os.path.join(REPO, "mir2-rs", "tests", "golden", "c-line-baseline-c3-20261010.jsonl")
    if os.path.exists(cap):
        gold = [r for r in load(cap) if r.get("ident") == 811]
        if gold:
            raw = bytes.fromhex(gold[0]["raw_hex"]) if "raw_hex" in gold[0] else None
            # 注册金标准里没有 raw_hex 时用编码长度关系旁证：enc(20)=27、enc(124)=166（由导出器断言间接覆盖）
            if raw:
                body = raw[1:raw.rindex(b"!")]
                payload = body[16:] if len(body) > 16 else b""
                seps = [i for i, b in enumerate(payload) if b == 47]
                if seps != [27, 55]:
                    fail(f"811 线上分隔符位置 {seps}（应为 [27, 55]）")

    # ---- 3) 改坏必红：seg_len 改错一格，导出器必须非 0 且留下 mismatch 标记 ----
    bad = os.path.join(tmp, "layout_bad.json")
    json.dump(with_frozen(table, seg_len_override=(811, 19)),
              open(bad, "w", encoding="utf-8", newline="\n"),
              ensure_ascii=False, indent=2, sort_keys=True)
    out2 = os.path.join(tmp, "cap_bad.jsonl")
    rc2, log2 = run_export(args.exe, args.c3_session, out2, bad)
    print("  导出(错表 seg_len=19) rc=%d" % rc2)
    if rc2 == 0:
        fail("seg_len 改错后导出器仍返回 0 —— 改错段边界不红")
    if "seg_mismatch=0" in log2:
        fail("seg_len 改错后 seg_mismatch 计数仍为 0")
    bad_rows = load(out2) if os.path.exists(out2) else []
    marked = [r for r in bad_rows if r.get("ident") == 811 and str(r.get("layout", "")).startswith("segmented(mismatch")]
    if not marked:
        fail("seg_len 改错后 811 没有被打上 segmented(mismatch:…) 标记")
    else:
        print("  PASS 错表：rc=%d，811 → %s" % (rc2, marked[0]["layout"]))

    # ---- 4) 确定性：同一 dump + 同一张表两次导出逐字节相同 ----
    out3 = os.path.join(tmp, "cap2.jsonl")
    run_export(args.exe, args.c3_session, out3, synth, extra=("--verify-roundtrip",))
    if open(out1, "rb").read() != open(out3, "rb").read():
        fail("同一 dump + 同一张表两次导出结果不同（确定性被破坏）")
    else:
        print("  PASS 同一 dump 两次导出逐字节相同")

    if FAILS:
        print("RED: segmented_check %d 条断言变红" % len(FAILS))
        return 1
    print("GREEN: segmented_check 全过（含改错必红、确定性）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
