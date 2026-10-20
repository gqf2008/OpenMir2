#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""s2c_diff.py — M2 验收③「同序列出包 diff」的对账半段。

把 **Rust 侧** 的 `capture.jsonl`（由 `export_s2c.ps1` 用既有 segment_frames +
GoldenExport 流水线产出，schema 与金标准逐字段同口径）与 **M0 金标准**
`mir2-rs/tests/golden/c-line-baseline-c3-20261010.jsonl`（当前口径）按**入图序**配对，逐帧对拍。

**不新写一套对账器**：帧的**抽取/解码**由既有 `segment_frames.py` + `tools/capture/GoldenExport`
负责，帧的**自洽（decode→encode 逐字节 + 字段）**由既有 `mir2-rs/tools/replay` 负责（`export_s2c.ps1`
已调用）。本脚本只补仓库里唯一没有的那一环：**两条 capture.jsonl 的序列配对 + 归一化 + 差异报告**。

对拍口径（每条都必须成立，否则红）：
  * **同序**：两侧的 ident 序列（从 `--start-ident` 起，到 `--upto-ident` 为止）逐项相等；
  * **等长**：同序天然等长；另逐帧比 `body_len`；
  * **逐字节**：`raw_hex` 在**只归一化 Recog**（会话相关 ActorId）后必须逐字节相等。
    归一化由既有 `oracle_capture.py` 的 EDCode 实现完成（importlib 复用，不新写 EDCode）：
    解 12B 头 → 把 recog 置为同一值 → 重编码 → 与尾原样拼回。除 recog 外的任何字节差异都算红。
  * 另比 `ident/param/tag/series/body_sha256`（body_sha256 更是把**尾**钉死，与 recog 无关）。

**归一化清单**（必须显式写进报告）：本卡只允许 `recog`（=`CommandMessage.Recog` = oracle `ActorId`）。
若将来要加字段，必须登记到 `--whitelist`，且每条白名单必须带 `reason` + `reviewer`（否则脚本直接红）。

用法：
  python s2c_diff.py --rust <rust capture.jsonl> [--golden <golden capture.jsonl>] \
      [--port 7200] [--start-ident 658] [--upto-ident 46] [--whitelist whitelist.jsonl] \
      [--out report.json] [--max-diffs 20] \
      [--from-recog N] [--from-x N] [--from-y N] [--to-recog N]

  `--from-recog/--from-x/--from-y` 只用于**选起始帧**（默认 `--start-ident 10` 的
  `SM_TURN`：recog=ActorId、param=x、tag=y）；`--to-recog` 选截止帧（默认
  `--upto-ident 41` 的 `SM_FEATURECHANGED`），缺省沿用起始帧的 recog。
  这是 C3 GuardList 需要的「按 recog/坐标挑出卫士那一对」，**不改变**任何逐字节口径
  （选帧之后仍按 ident 序列 + 字段 + 归一化后的 raw_hex 逐字节比）。
  `--selftest` 不需要 capture，覆盖选帧过滤与 `norm_raw` 归一化。

退出码：0 = 0 差异（或全部落在白名单）；1 = 有未登记差异（红）；2 = 前置未满足（文件缺失/序列为空）。
"""
import argparse
import importlib.util
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
DEFAULT_GOLDEN = os.path.join(
    REPO, "mir2-rs", "tests", "golden", "c-line-baseline-c3-20261010.jsonl"
)
# 注意：`tests/golden/session-20261010-c3-hook/capture.jsonl` 是 **A-9 旧口径**（811 等多段帧被当成
# single、body_len/body_sha256 与分段解码不一致）。本仓“当前口径”的金标准是 A 线登记、
# `golden_fresh_check.py` 保活的 `mir2-rs/tests/golden/c-line-baseline-c3-*.jsonl`
# （sha256=f34a6d11…，与当前 `segment_frames.py --layout` 重导件逐字节相同）。
# 因此默认拿它做金标准；若硬要拿旧 session capture.jsonl 对，811 会出一对 A-9 假红（已登记在
# `mir2-rs/tests/parity/whitelist.md` A-9），需自行白名单。
# 复用既有 EDCode（LoginSrv 金标准采集器里的 C# EncryptUtil 移植），不新写一套。
_EDCODE_PATH = os.path.join(REPO, "mir2-rs", "tests", "parity", "loginsrv", "oracle_capture.py")


def _load_edcode():
    if not os.path.exists(_EDCODE_PATH):
        print("PREREQ_MISSING: 找不到既有 EDCode 实现 %s" % _EDCODE_PATH, file=sys.stderr)
        sys.exit(2)
    spec = importlib.util.spec_from_file_location("_oracle_capture_edcode", _EDCODE_PATH)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)  # __main__ 有 guard，不会跑采集主体
    return mod


ED = _load_edcode()

# 归一化字段清单（本卡只允许 recog；写进报告，禁止静默扩大）。
NORMALIZATION = {
    "recog": "CommandMessage.Recog = oracle ActorId（ActorMgr.GetNextIdentity() 的会话相关随机正数）"
}


def load_frames(path):
    rows = []
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                r = json.loads(line)
            except json.JSONDecodeError:
                continue
            if r.get("kind") in ("heartbeat", "internal", "edcode"):
                continue
            rows.append(r)
    return rows


def select(rows, port, direction):
    out = [r for r in rows if r.get("port") == port and r.get("dir") == direction]
    out.sort(key=lambda r: r.get("seq", 0))
    return out


def _recog_eq(a, b):
    """recog 是带符号 i32（线上 ActorId 恒正，但归一化后可能以 0 出现）。按 32 位无符号比。"""
    return ((a or 0) & 0xFFFFFFFF) == ((b or 0) & 0xFFFFFFFF)


def frame_matches(r, ident, recog=None, x=None, y=None):
    """选帧谓词：ident 必须相等；recog/param/tag 仅在显式给出时参与。"""
    if r.get("ident") != ident:
        return False
    if recog is not None and not _recog_eq(r.get("recog", 0), recog):
        return False
    if x is not None and r.get("param") != x:
        return False
    if y is not None and r.get("tag") != y:
        return False
    return True


def trim(rows, start_ident, upto_ident, from_recog=None, from_x=None, from_y=None,
         to_recog=None):
    """从 start_ident 的第一次**匹配**起，到 upto_ident 的第一次**匹配**（含）为止。

    过滤参数只用于选帧，不改变逐字节口径（C3：按 recog/坐标挑出特定 actor 的 10→41 对）：
      from_recog/from_x/from_y = 起始帧（SM_TURN(10)）的 recog/x(param)/y(tag)；
      to_recog = 截止帧（SM_FEATURECHANGED(41)）的 recog，缺省沿用起始帧的 recog
      （若起始帧带了 x/y，则截止帧自动按**那个 actor** 匹配，避免配到旁人的 41）。
    """
    start = None
    for i, r in enumerate(rows):
        if frame_matches(r, start_ident, from_recog, from_x, from_y):
            start = i
            break
    if start is None:
        flt = []
        if from_recog is not None:
            flt.append("recog=%s" % from_recog)
        if from_x is not None:
            flt.append("x=%s" % from_x)
        if from_y is not None:
            flt.append("y=%s" % from_y)
        return None, ("序列里找不到起始 ident=%s%s" %
                      (start_ident, ("（过滤 %s）" % " ".join(flt)) if flt else ""))
    start_recog = from_recog
    if start_recog is None and (from_x is not None or from_y is not None):
        start_recog = rows[start].get("recog", 0)
    rows = rows[start:]
    end = len(rows)
    if upto_ident is not None:
        tr = to_recog if to_recog is not None else start_recog
        j = None
        for i, r in enumerate(rows):
            if frame_matches(r, upto_ident, tr):
                j = i
                break
        if j is None:
            return None, ("序列里找不到截止 ident=%s%s" %
                          (upto_ident, ("（recog=%s）" % tr) if tr is not None else ""))
        end = j + 1
    return rows[:end], None


def norm_raw(raw_hex):
    """`#`+Encode(头12B)+尾+`!` 的 s2c 帧：只把 recog 归一化，其余逐字节保留。"""
    if not raw_hex:
        return raw_hex
    raw = bytes.fromhex(raw_hex)
    if len(raw) < 18 or raw[0:1] != b"#" or raw[-1:] not in (b"!",):
        return raw_hex  # 非 12B 头帧（字符串/act 帧）不做头归一化，原字节直接比
    tail, head, rest = ED.split_client_frame(raw)
    fields = ED.head_fields(head)
    norm_head = bytearray(head)
    norm_head[0:4] = (0).to_bytes(4, "little", signed=True)
    out = b"#" + ED.edcode_encode(bytes(norm_head)) + rest + tail
    return out.hex()


def load_whitelist(path):
    if not path:
        return []
    entries = []
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            e = json.loads(line)
            if not e.get("reason") or not e.get("reviewer"):
                print(
                    "RED: 白名单条目缺 reason/reviewer（§6.0-3：差异要么 0 要么逐条登记）: %s" % line,
                    file=sys.stderr,
                )
                sys.exit(1)
            entries.append(e)
    return entries


def whitelisted(entries, ident, field):
    for e in entries:
        if e.get("ident") in ("*", ident) and e.get("field") in ("*", field):
            return e
    return None


def main():
    ap = argparse.ArgumentParser(description="M2 验收③：Rust s2c 序列 vs M0 金标准")
    ap.add_argument("--rust", default="")
    ap.add_argument("--golden", default=DEFAULT_GOLDEN)
    ap.add_argument("--port", type=int, default=7200)
    ap.add_argument("--dir", default="s2c")
    ap.add_argument("--start-ident", type=int, default=658)
    # owner §22 裁定：入图序 = 658→46（15 条自帧）；10 SM_TURN 是 AOI 帧，不属入图序（单列判）。
    ap.add_argument("--upto-ident", type=int, default=46)
    ap.add_argument("--from-recog", type=int, default=None,
                    help="起始帧 recog（ActorId）过滤；用于 C3 按 actor 挑 10→41 对")
    ap.add_argument("--from-x", type=int, default=None,
                    help="起始帧 param（SM_TURN 的 x）过滤；与 --from-y 同给")
    ap.add_argument("--from-y", type=int, default=None,
                    help="起始帧 tag（SM_TURN 的 y）过滤；与 --from-x 同给")
    ap.add_argument("--to-recog", type=int, default=None,
                    help="截止帧 recog 过滤；缺省沿用起始帧 recog")
    ap.add_argument("--whitelist", default="")
    ap.add_argument("--out", default="")
    ap.add_argument("--max-diffs", type=int, default=20)
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()

    if args.selftest:
        return selftest()
    if not args.rust:
        print("PREREQ_MISSING: 必须给 --rust <capture.jsonl>（或 --selftest）", file=sys.stderr)
        return 2
    if (args.from_x is None) != (args.from_y is None):
        print("PREREQ_MISSING: --from-x/--from-y 必须成对给出（坐标过滤）", file=sys.stderr)
        return 2
    for p in (args.rust, args.golden):
        if not os.path.exists(p):
            print("PREREQ_MISSING: 找不到 %s（先跑 export_s2c.ps1 / 确认金标准在库）" % p, file=sys.stderr)
            return 2

    g_all = select(load_frames(args.golden), args.port, args.dir)
    r_all = select(load_frames(args.rust), args.port, args.dir)
    if not r_all:
        print("PREREQ_MISSING: Rust 侧 capture.jsonl 里没有 port=%d dir=%s 的帧" % (args.port, args.dir), file=sys.stderr)
        return 2
    g, err = trim(g_all, args.start_ident, args.upto_ident, args.from_recog,
                  args.from_x, args.from_y, args.to_recog)
    if err:
        print("PREREQ_MISSING(golden): %s" % err, file=sys.stderr)
        return 2
    r, err = trim(r_all, args.start_ident, args.upto_ident, args.from_recog,
                  args.from_x, args.from_y, args.to_recog)
    if err:
        print("RED(rust): %s" % err, file=sys.stderr)
        return 1

    wl = load_whitelist(args.whitelist)
    diffs = []
    whitelisted_hits = []

    def add(ident, field, gold, rust, kind=""):
        w = whitelisted(wl, ident, field)
        item = {"ident": ident, "field": field, "kind": kind,
                "golden": str(gold)[:160], "rust": str(rust)[:160]}
        if w is not None:
            item["whitelist"] = {"reason": w["reason"], "reviewer": w["reviewer"]}
            whitelisted_hits.append(item)
        else:
            diffs.append(item)

    # ① 同序
    g_idents = [x.get("ident") for x in g]
    r_idents = [x.get("ident") for x in r]
    if g_idents != r_idents:
        for i in range(max(len(g_idents), len(r_idents))):
            gi = g_idents[i] if i < len(g_idents) else None
            ri = r_idents[i] if i < len(r_idents) else None
            if gi != ri:
                add(gi if gi is not None else ri, "ident", gi, ri, kind="order")
                break  # 第一处分叉
    # ② 逐帧逐字段
    for i in range(min(len(g), len(r))):
        gi, ri = g[i], r[i]
        ident = gi.get("ident")
        for field in ("ident", "param", "tag", "series", "body_len", "body_sha256"):
            if gi.get(field) != ri.get(field):
                add(ident, field, gi.get(field), ri.get(field))
        gn, rn = norm_raw(gi.get("raw_hex", "")), norm_raw(ri.get("raw_hex", ""))
        if gn != rn:
            add(ident, "raw_hex", gn, rn, kind="byte")
        if len(diffs) >= args.max_diffs:
            break

    report = {
        "tool": "m2-acceptance/s2c_diff.py",
        "golden": args.golden,
        "rust": args.rust,
        "port": args.port,
        "dir": args.dir,
        "start_ident": args.start_ident,
        "upto_ident": args.upto_ident,
        "filters": {"from_recog": args.from_recog, "from_x": args.from_x,
                    "from_y": args.from_y, "to_recog": args.to_recog},
        "normalization": NORMALIZATION,
        "golden_frames": len(g),
        "rust_frames": len(r),
        "golden_idents": g_idents,
        "rust_idents": r_idents,
        "diff_count": len(diffs),
        "whitelisted_count": len(whitelisted_hits),
        "diffs": diffs,
        "whitelisted": whitelisted_hits,
        "pass": len(diffs) == 0,
    }
    if args.out:
        with open(args.out, "w", encoding="utf-8", newline="\n") as f:
            json.dump(report, f, ensure_ascii=False, indent=1)

    print("NORMALIZATION %s" % json.dumps(NORMALIZATION, ensure_ascii=False))
    print("FILTERS from_recog=%s from_x=%s from_y=%s to_recog=%s" %
          (args.from_recog, args.from_x, args.from_y, args.to_recog))
    print("GOLDEN idents(%d): %s" % (len(g_idents), g_idents))
    print("RUST   idents(%d): %s" % (len(r_idents), r_idents))
    for d in diffs:
        print("DIFF ident=%s field=%s golden=%s rust=%s" % (d["ident"], d["field"], d["golden"], d["rust"]))
    for w in whitelisted_hits:
        print("WHITELISTED ident=%s field=%s reviewer=%s" % (w["ident"], w["field"], w["whitelist"]["reviewer"]))
    if args.out:
        print("REPORT %s" % args.out)

    if diffs:
        print("RED: %d 处未登记差异（同序列 diff != 0）" % len(diffs))
        return 1
    print("GREEN: 同序列逐字节 diff = 0（归一化仅 %s）" % ",".join(NORMALIZATION))
    return 0


def _st_frame(ident, recog=0, param=0, tag=0, seq=0, raw_hex=""):
    return {"kind": "frame", "dir": "s2c", "port": 7200, "seq": seq, "ident": ident,
            "recog": recog, "param": param, "tag": tag, "series": 0,
            "body_len": 0, "body_sha256": "", "raw_hex": raw_hex}


def selftest():
    """选帧过滤的改坏必红自检：不依赖任何外部 capture。"""
    ok = True

    def expect(name, got, want):
        nonlocal ok
        good = got == want
        ok = ok and good
        print("%-52s %s (rc=%d, want %d)" % (name, "OK" if good else "FAIL", got, want))

    def rc_of(rows, **kw):
        sid = kw.pop("start_ident", 10)
        uid = kw.pop("upto_ident", 41)
        _out, err = trim(rows, sid, uid, **kw)
        return 0 if err is None else 1

    # 同屏先有别人（guard 在第二对）：不加过滤必须取第一对；按 recog/坐标必须取第二对。
    rows = [
        _st_frame(10, recog=0x11, param=100, tag=200, seq=0),
        _st_frame(41, recog=0x11, seq=1),
        _st_frame(10, recog=0x22, param=291, tag=666, seq=2),
        _st_frame(41, recog=0x22, seq=3),
    ]
    first, err = trim(rows, 10, 41)
    expect("无过滤：取第一对（旧行为不变）",
           0 if err is None and first[0]["recog"] == 0x11 else 1, 0)
    guard, err = trim(rows, 10, 41, from_recog=0x22)
    expect("--from-recog 0x22：取卫士那一对",
           0 if err is None and [f["ident"] for f in guard] == [10, 41]
           and guard[0]["recog"] == 0x22 else 1, 0)
    guard2, err = trim(rows, 10, 41, from_x=291, from_y=666)
    expect("--from-x/--from-y：按坐标取卫士那一对",
           0 if err is None and [f["ident"] for f in guard2] == [10, 41]
           and guard2[0]["recog"] == 0x22 else 1, 0)
    expect("--from-recog 与坐标不符 ⇒ 找不到（fail-closed）",
           rc_of(rows, from_recog=0x22, from_x=100, from_y=200), 1)
    expect("--from-recog 不存在 ⇒ 找不到（fail-closed）",
           rc_of(rows, from_recog=0x99), 1)
    # 截止帧必须配同一 actor：中间插了旁人的 41，也不能拿它当截止。
    rows2 = [
        _st_frame(10, recog=0x22, param=291, tag=666, seq=0),
        _st_frame(41, recog=0x99, seq=1),
        _st_frame(41, recog=0x22, seq=2),
    ]
    got, err = trim(rows2, 10, 41, from_recog=0x22)
    expect("截止 41 按同一 recog 匹配（不吃旁人的 41）",
           0 if err is None and [f["recog"] for f in got] == [0x22, 0x99, 0x22] else 1, 0)
    # 显式 --to-recog 必须真正生效：起始 recog=A 的 41 在前，截止只能落在 recog=B 的 41。
    # （把 trim 的 `to_recog if ... else start_recog` 改成只用 start_recog 时，本条必红。）
    rows_t = [
        _st_frame(10, recog=0x11, seq=0),
        _st_frame(41, recog=0x11, seq=1),
        _st_frame(41, recog=0x22, seq=2),
    ]
    got_t, err = trim(rows_t, 10, 41, from_recog=0x11, to_recog=0x22)
    expect("显式 --to-recog=B 截止（不吃起始 recog=A 的 41）",
           0 if err is None and [f["recog"] for f in got_t] == [0x11, 0x11, 0x22] else 1, 0)
    # 负 recog 的 32 位比较（捕获里 i32 可能显示为负）。
    rows3 = [_st_frame(10, recog=-1, param=1, tag=2, seq=0),
             _st_frame(41, recog=-1, seq=1)]
    expect("负 recog 按 32 位无符号匹配",
           0 if trim(rows3, 10, 41, from_recog=0xFFFFFFFF)[1] is None else 1, 0)
    # 过滤不改变逐字节口径：两条只差 recog 的帧，norm_raw 后必须相等。
    f1 = ED.client_frame(10, b"\x01\x02\x03", recog=0x11, param=291, tag=666).hex()
    f2 = ED.client_frame(10, b"\x01\x02\x03", recog=0x22, param=291, tag=666).hex()
    expect("norm_raw 只归一化 recog（过滤参数不参与字节比较）",
           0 if norm_raw(f1) == norm_raw(f2) and norm_raw(f1) != f1 else 1, 0)
    print("SELFTEST %s" % ("GREEN" if ok else "RED"))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
