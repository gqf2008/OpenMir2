#!/usr/bin/env python3
"""从客户端整窗截图里求 NPC 对话框的**链接命中矩形与点击点**。

为什么需要：冻结客户端（`FState.pas:12097-12115`）给链接登记的命中矩形是
``Rect(lx+sX, ly, lx+sX+TextWidth(cmdstr), ly+14)``（``lx=30``、``ly=20+16*行号``），
**只覆盖尖括号内可见文本**；而字形像素带比这 14 px 高。肉眼按「字在哪」去点，
会落在矩形下沿之外——r7 的两点就分别低了 15/18 px（`docs/dice-link-diagnosis-20261022.md`）。

本工具改为**像素检测**：在对话框面板里找「黄字」（普通对话框的链接色），按行分带、
带内按列分 run，再用「带顶 + 7」求点击 y（不是带中心）。

用法::

    python tools/client/dlg_links.py shot.png                     # JSON: 每个 run 的矩形与点击点
    python tools/client/dlg_links.py shot.png --origin 3,26       # 面板原点（默认 3,26）
    python tools/client/dlg_links.py shot.png --region 0,0,440,210
    python tools/client/dlg_links.py shot.png --say-file say.txt  # 按序贴上 <文本/@参数>
    python tools/client/dlg_links.py --selftest

约定：
- 坐标一律输出**客户端客户区**坐标（= 窗口截图坐标 - origin），与 `mir_click.ps1` 同口径；
- 只报宽度 ≥ ``--min-width``（默认 12 px）的 run，且丢弃贴面板边缘的噪声（``--edge``）；
- 大对话框（`atlen>=15`）的链接是**灰白**色，黄掩码抓不到——本工具不猜，直接返回空，
  调用方按行距取点（见 `docs/window-round9-dice-chain-20261022.md` §2.1）。

自测：对 r7 的 `shot-dice5.png`（存在时）断言恰好三个链接矩形，与诊断文档逐像素一致。
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

try:
    import numpy as np
    from PIL import Image
except Exception as exc:  # pragma: no cover - 环境缺依赖时给出可读错误
    print(f"需要 PIL + numpy：{exc}", file=sys.stderr)
    raise SystemExit(2)

# 「黄字」掩码：R 高、G 较高、B 低，且 R≈G（排除纯红/纯白）。
YELLOW = dict(r_min=170, g_min=140, b_max=110, rg_max=80)


def runs_in_image(
    path: Path,
    origin: tuple[int, int] = (3, 26),
    region: tuple[int, int, int, int] | None = None,
    min_width: int = 12,
    edge: int = 8,
    band_gap: int = 3,
    run_gap: int = 10,
) -> list[dict]:
    img = np.asarray(Image.open(path).convert("RGB")).astype(int)
    ox, oy = origin
    x0, y0, x1, y1 = region if region else (0, 0, img.shape[1] - ox, img.shape[0] - oy)
    panel = img[oy + y0 : oy + y1, ox + x0 : ox + x1]
    r, g, b = panel[:, :, 0], panel[:, :, 1], panel[:, :, 2]
    mask = (
        (r > YELLOW["r_min"])
        & (g > YELLOW["g_min"])
        & (b < YELLOW["b_max"])
        & (abs(r - g) < YELLOW["rg_max"])
    )

    bands: list[list[int]] = []
    for y, count in enumerate(mask.sum(axis=1)):
        if count >= 3:
            if bands and y - bands[-1][-1] <= band_gap:
                bands[-1].append(y)
            else:
                bands.append([y])

    out: list[dict] = []
    for band in bands:
        columns = mask[band[0] : band[-1] + 1, :].any(axis=0)
        xs = np.where(columns)[0]
        runs: list[list[int]] = []
        for x in xs:
            if runs and x - runs[-1][-1] <= run_gap:
                runs[-1].append(int(x))
            else:
                runs.append([int(x)])
        for run in runs:
            left, right = run[0], run[-1]
            if right - left + 1 < min_width:
                continue
            if left <= edge or right >= (x1 - x0) - edge:
                continue
            out.append(
                {
                    "x0": x0 + left,
                    "x1": x0 + right,
                    "y_band0": y0 + band[0],
                    "y_band1": y0 + band[-1],
                    # 命中矩形高 14 px，起点 ly = 20+16k ⇒ 带顶 + 7 落在框内
                    "click_x": x0 + (left + right) // 2,
                    "click_y": y0 + band[0] + 7,
                }
            )
    return out


def attach_say(texts: list[str]) -> list[str]:
    """按序把说词里的链接（尖括号内含 ``/``）贴到 run 上，便于人读。"""
    links: list[str] = []
    for line in texts:
        for seg in line.split("\\"):
            start = 0
            while True:
                i = seg.find("<", start)
                if i < 0:
                    break
                j = seg.find(">", i + 1)
                if j < 0:
                    break
                tok = seg[i + 1 : j]
                if "/" in tok:
                    links.append(tok)
                start = j + 1
    return links


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description="客户端对话框：从截图求链接矩形与点击点")
    ap.add_argument("shot", nargs="?", help="整窗截图 PNG")
    ap.add_argument("--origin", default="3,26", help="客户区原点（窗口截图 - 该偏移 = client），默认 3,26")
    ap.add_argument(
        "--region",
        default="0,0,440,210",
        help="面板内搜索区域 x0,y0,x1,y1（client 坐标）；小对话框默认 0,0,440,210，大对话框按截图改",
    )
    ap.add_argument("--min-width", type=int, default=12)
    ap.add_argument("--edge", type=int, default=20)
    ap.add_argument("--say-file", default="", help="说词原文（GBK/UTF-8 均可），给 run 贴上 @参数")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()

    if not args.shot:
        ap.error("需要截图路径（或 --selftest）")
    origin = tuple(int(x) for x in args.origin.split(","))
    region = tuple(int(x) for x in args.region.split(",")) if args.region else None
    runs = runs_in_image(Path(args.shot), origin=origin, region=region, min_width=args.min_width, edge=args.edge)

    if args.say_file:
        raw = Path(args.say_file).read_bytes()
        for enc in ("utf-8-sig", "gbk"):
            try:
                text = raw.decode(enc)
                break
            except UnicodeDecodeError:
                continue
        else:
            text = raw.decode("gbk", "replace")
        for run, link in zip(runs, attach_say(text.splitlines())):
            run["link"] = link

    print(json.dumps({"shot": args.shot, "origin": list(origin), "runs": runs}, ensure_ascii=False, indent=2))
    return 0


def selftest() -> int:
    shot = Path("E:/tmp/window/r7/shot-dice5.png")
    if not shot.exists():
        print("SKIP：找不到 r7 截图（E:/tmp/window/r7/shot-dice5.png）")
        return 0
    runs = runs_in_image(shot, region=(0, 0, 440, 210))
    # 诊断文档 §3：@kaiz 43..107×68..82、@chouma 169..233×68..82、@huit 331..377×148..162
    want = [(43, 107, 68, 82), (169, 233, 68, 82), (331, 377, 148, 162)]
    got = [(r["x0"], r["x1"], r["click_x"], r["click_y"]) for r in runs]
    print("got ", got)
    if len(got) != 3:
        print(f"SELFTEST RED：期望 3 个链接，实得 {len(got)} 个")
        return 1
    for (x0, x1, cx, cy), (wx0, wx1, wy0, wy1) in zip(got, want):
        if (x0, x1) != (wx0, wx1) or not (wy0 <= cy <= wy1) or not (wx0 <= cx <= wx1):
            print(f"SELFTEST RED：{(x0, x1, cx, cy)} 不在期望矩形 {(wx0, wx1, wy0, wy1)} 内")
            return 1
    print("SELFTEST GREEN：三个链接矩形与诊断文档逐像素一致，点击点都落在命中框内")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
