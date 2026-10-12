#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""SHA256SUMS.txt 通用复核器（卡 `openmir2-evidence-hash-convention`，issue 915ea881）。

## 为什么要它

同一个坑在本仓库**已出现四次**：工作树字节（`core.autocrlf=true` 下是 CRLF）与
**仓库字节（Git blob 内容）**不同，而 `SHA256SUMS.txt` 这类证据清单到底按哪种字节记，
此前从没写成约定：

1. C6 **F9-1**：`tests/loadgate/**` 的 `artifacts.sha256.json` 按盘上 CRLF 记 ⇒ 全新检出 18/45 假 mismatch；
2. M4 **N-1**：`tests/parity/evidence/M4/**` 声明 LF(blob) 而裸 `sha256sum` 对不上；
3. M2 会话生命周期 **N-1**：`tools/m2-acceptance/evidence/session-lifecycle-{rerun,selftest}-20261012/SHA256SUMS.txt`
   按盘上 CRLF 记 ⇒ 对仓库字节复核 8/11、5/6 mismatch；
4. M2 envir **-2026-10-10**：`parse-stats-cs.json` 记的是 CRLF 值（该文件 `text eol=lf`，仓库字节是 LF）——
   连 `--disk-bytes` 都对不上，因为检出时已被归一成 LF。

## 口径（唯一权威定义，见 `docs/Rust重写设计文档.md` §6.0-4）

`SHA256SUMS.txt` 里的 hash **一律按「仓库字节」记**：

    SHA-256( Git blob 内容 )

对 `text` 文件（含 `text eol=lf`、`auto`、未指定 + `core.autocrlf=true`）它就是
**先 CRLF→LF 归一后的 sha256**；对 `-text` 冻结文件（见 `.gitattributes`）它就是**盘上原始字节**；
对**仓外输入**（如 `E:/MirServer/**` 的真实文件，没有 blob）按**盘上原始字节**记。

本工具默认按这个口径复核（用 `git check-attr` 逐文件解析 `text` 属性，不自己猜）。
`--disk-bytes` 切到「盘上原始字节」口径，用于**声明了 `.gitattributes` 的 `* -text`（冻结字节）目录**——
那种目录里 `sha256sum -c SHA256SUMS.txt` 才能在任何检出上通过。切错口径 = 红，这是本工具的牙齿之一。

## 命令

    # 复核单个清单 / 目录（默认 = 仓库字节口径）
    python -I tools/evidence-hash/verify_sha256sums.py tools/m2-acceptance/evidence/xxx
    python -I tools/evidence-hash/verify_sha256sums.py --disk-bytes tests/loadgate

    # 把清单**显式修正**到仓库字节口径（改动证据后 / 修历史遗留时用；不自动跑）
    python -I tools/evidence-hash/verify_sha256sums.py write tools/m2-acceptance/evidence/xxx

    # 全仓逐目录对账（③）：给出 件数 / 两种口径命中数 / 结论
    python -I tools/evidence-hash/verify_sha256sums.py reconcile --md tools/evidence-hash/RECONCILE.md \
        --json tools/evidence-hash/RECONCILE.json

    # 牙齿（改 hash / 多件 / 少件 / 口径切换 各必红）
    python -I tools/evidence-hash/verify_sha256sums.py selftest

退出码：`0` = 绿；`1` = 红（有 mismatch / 缺件 / 未声明的盘上字节口径）；
`2` = 用法或前置错误（目标/清单不存在、无法解析、仓库解析失败）。

设计原则（照 `mir2-rs/tests/parity/M4/evidence_hashes.py`）：
- **不静默通过**：清单不存在、路径缺失、`git` 不可用一律非零退出；
- **口径只有一个**：默认仓库字节；切口径要显式 `--disk-bytes`；
- **多件/少件是牙齿**：`--check-extra` 打开后目录里未列入清单的文件也判红；
- **EOL 陷阱是牙齿（2026-10-12 追加，同族坑第 4 次）**：对清单里每个**受跟踪、会被 git 行尾归一**的
  文本文件，若**清单记的是工作树原始字节（CRLF）**而仓库字节（LF 归一 / git blob）是另一个值 ⇒
  直接判红并点名。这类清单在作者自己的工作树里可能「自洽」（尤其 `--disk-bytes` 时）却在**仓库字节口径下
  不可复现**（m3cs 片：`verify GREEN 75/75` 在钉住 head 上实为 `RED 74/75`）。**不许靠清单自洽拿到假绿。**
"""

from __future__ import annotations

import argparse
import fnmatch
import hashlib
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

MANIFEST_NAME = "SHA256SUMS.txt"
LINE_RE = re.compile(r"^([0-9a-fA-F]{64})([ \t]+\*?)(.*)$")
ABS_RE = re.compile(r"^([A-Za-z]:[\\/]|\\\\|/)")


# --------------------------------------------------------------------------- #
# 仓库 / 口径
# --------------------------------------------------------------------------- #
def find_repo(explicit: str | None) -> Path:
    if explicit:
        return Path(explicit).resolve()
    here = Path(__file__).resolve().parents[2]  # tools/evidence-hash/x.py -> 仓库根
    for cand in (here, Path.cwd()):
        try:
            out = subprocess.run(
                ["git", "-C", str(cand), "rev-parse", "--show-toplevel"],
                capture_output=True, text=True, check=True,
            )
            return Path(out.stdout.strip()).resolve()
        except (subprocess.CalledProcessError, OSError):
            continue
    return here


def inside_repo(path: Path, repo: Path) -> bool:
    try:
        path.resolve().relative_to(repo.resolve())
        return True
    except ValueError:
        return False


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def is_binary(data: bytes) -> bool:
    # 与 Git 的启发式一致：前 8000 字节里有 NUL 即按二进制处理（不做行尾归一）
    return b"\x00" in data[:8000]


def normalize_lf(data: bytes) -> bytes:
    return data.replace(b"\r\n", b"\n")


def check_attr_text(repo: Path, rel_paths: list[str]) -> dict[str, str]:
    """批量解析 `text` 属性 → {仓库相对 posix 路径: 'unset'|'set'|'auto'|'unspecified'|...}。"""
    out: dict[str, str] = {}
    for p in rel_paths:
        out[p] = "unspecified"
    if not rel_paths:
        return out
    try:
        proc = subprocess.run(
            ["git", "-C", str(repo), "check-attr", "-z", "text", "--", *rel_paths],
            capture_output=True, check=True,
        )
    except (subprocess.CalledProcessError, OSError) as exc:  # pragma: no cover
        raise RuntimeError(f"git check-attr 失败：{exc}") from exc
    parts = proc.stdout.split(b"\x00")
    # 三元组：path \0 attr \0 value \0
    for i in range(0, len(parts) - 2, 3):
        path = parts[i].decode("utf-8", "surrogateescape")
        value = parts[i + 2].decode("utf-8", "surrogateescape")
        if path:
            out[path] = value or "unspecified"
    return out


def resolve_entry(base: Path, name: str) -> Path:
    """清单里的条目 → 文件路径。支持绝对路径（含 Windows 盘符）与相对路径。"""
    if ABS_RE.match(name):
        return Path(name)
    return (base / name).resolve()


def blob_bytes(repo: Path, rel: str) -> bytes | None:
    """取某路径在仓库里的 blob 原始字节（HEAD 优先，其次 index）；未跟踪返回 None。"""
    for rev in (f"HEAD:{rel}", f":{rel}"):
        try:
            proc = subprocess.run(["git", "-C", str(repo), "cat-file", "blob", rev],
                                  capture_output=True, check=True)
        except (subprocess.CalledProcessError, OSError):
            continue
        return proc.stdout
    return None


def find_eol_traps(entries: list[tuple[str, str]], base: Path, repo: Path,
                   attrs: dict[str, str]) -> list[dict]:
    """EOL 陷阱（fail-closed，2026-10-12；同族坑第 4 次）。

    对清单里**在仓库内、会被 git 做行尾归一**（`text` 属性非 `-text`、且非二进制）的每个文件：

      - 盘上原始字节 / 该文件的 git blob 原始字节，同一存储内容在 EOL 上与仓库口径（LF 归一）不一致，**且**
      - 清单里记的正是**原始字节**（盘上的 CRLF，或 blob 自己就带 CRLF）的 sha256，而不是 LF 归一后的值

      ⇒ 这条清单只在作者自己的工作树/提交物里自洽（`--disk-bytes` 更是直接判绿），在**仓库字节口径 /
      干净检出**上不可复现 ⇒ **判红并点名**。修法：`write` 改写成仓库字节口径；若确实要按盘上字节记，
      目录必须声明 `.gitattributes * -text` 冻结字节（那时 `attr == unset`，本条不适用，也不该适用）。

    `-text` 冻结文件与仓外输入**不适用**本条（按盘上字节就是它们声明的口径）。
    真实先例（第 4 次）：m3cs 的 `evidence-rng-isolate-attack-twophase-20261012`——`window/00-selfcheck-nostack.txt`
    的 blob 自己就带 CRLF（`a9ca0b7c…`），清单也记了该值，而 `mir2-rs/.gitattributes` 的 `*.txt text eol=lf`
    在仓库口径下是 LF 归一值 ⇒ 该片声明的 `verify GREEN 75/75` 在钉住 head 上实为 `RED 74/75`。
    """
    traps: list[dict] = []
    for want, name in entries:
        path = resolve_entry(base, name)
        if not path.is_file():
            continue
        if not inside_repo(path, repo):
            continue  # 仓外输入：按盘上字节是声明口径
        try:
            rel = path.resolve().relative_to(repo.resolve()).as_posix()
        except ValueError:  # pragma: no cover - inside_repo 已保证
            continue
        attr = attrs.get(rel)
        if attr == "unset":
            continue  # `-text` 冻结：盘上字节 == 仓库字节，按盘上记是对的
        data = path.read_bytes()
        if is_binary(data):
            continue  # 二进制不做行尾归一
        h_lf = sha256_bytes(normalize_lf(data))
        if want == h_lf:
            continue  # 清单就是仓库口径，无陷阱
        h_raw = sha256_bytes(data)
        h_blob: str | None = None
        b = blob_bytes(repo, rel)
        if b is not None:
            h_blob = sha256_bytes(b)
        source = None
        if h_raw != h_lf and want == h_raw:
            source = "工作树原始字节(CRLF)"
        elif h_blob is not None and h_blob != h_lf and want == h_blob:
            source = "git blob 原始字节(含 CRLF)"
        if source is not None:
            traps.append({"path": str(path), "listed": want, "disk_raw": h_raw,
                          "repo_lf": h_lf, "blob_raw": h_blob, "source": source,
                          "attr": attr})
    return traps


def hash_path(path: Path, mode: str, repo: Path, rel: str | None, attr_state: str | None) -> str:
    data = path.read_bytes()
    if mode == "disk":
        return sha256_bytes(data)
    # mode == "repo"：仓库字节 = git blob 内容
    if not inside_repo(path, repo):
        return sha256_bytes(data)  # 仓外输入：没有 blob，按盘上字节
    if attr_state == "unset":
        return sha256_bytes(data)  # `-text` 冻结
    if attr_state in (None, "unspecified", "auto") and is_binary(data):
        return sha256_bytes(data)
    return sha256_bytes(normalize_lf(data))


# --------------------------------------------------------------------------- #
# 清单解析 / 复核
# --------------------------------------------------------------------------- #
def parse_manifest(manifest: Path) -> list[tuple[str, str]]:
    """→ [(hash, 相对/绝对条目名)]，保序。"""
    entries: list[tuple[str, str]] = []
    for lineno, raw in enumerate(manifest.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
        line = raw.rstrip("\r")
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        m = LINE_RE.match(line)
        if not m:
            raise ValueError(f"{manifest}:{lineno} 无法解析：{line!r}")
        entries.append((m.group(1).lower(), m.group(3).strip()))
    return entries


def verify_dir(base: Path, repo: Path, mode: str, check_extra: bool,
               excludes: list[str]) -> dict:
    manifest = base / MANIFEST_NAME
    if not manifest.is_file():
        return {"dir": str(base), "manifest": str(manifest), "prereq": f"缺清单 {manifest}"}
    entries = parse_manifest(manifest)

    # 解析 + 收集属性
    rel_for_attr: list[str] = []
    tmp: list[tuple[str, Path, str | None]] = []  # (hash, path, rel)
    for want, name in entries:
        path = resolve_entry(base, name)
        rel = None
        if inside_repo(path, repo):
            try:
                rel = path.resolve().relative_to(repo.resolve()).as_posix()
            except ValueError:
                rel = None
        if rel is not None:
            rel_for_attr.append(rel)
        tmp.append((want, path, rel))

    attrs = check_attr_text(repo, rel_for_attr)

    mismatch, missing = [], []
    ok = 0
    for want, path, rel in tmp:
        if not path.is_file():
            missing.append(str(path))
            continue
        got = hash_path(path, mode, repo, rel, attrs.get(rel) if rel else None)
        if got == want:
            ok += 1
        else:
            mismatch.append({"path": str(path), "listed": want, "computed": got,
                             "attr": attrs.get(rel) if rel else "outside"})

    # EOL 陷阱：与 mode 无关——`--disk-bytes` 也不能拿「清单自洽」换假绿。
    eol_traps = find_eol_traps(entries, base, repo, attrs)

    extra: list[str] = []
    listed_names = {name.replace("\\", "/").lstrip("./") for _, name in entries}
    if check_extra:
        for p in sorted(base.rglob("*")):
            if not p.is_file():
                continue
            rel = p.relative_to(base).as_posix()
            if rel == MANIFEST_NAME or rel in listed_names:
                continue
            if any(fnmatch.fnmatch(rel, pat) or fnmatch.fnmatch(p.name, pat) for pat in excludes):
                continue
            extra.append(rel)

    green = not mismatch and not missing and not extra and not eol_traps
    return {
        "dir": str(base),
        "manifest": str(manifest),
        "mode": mode,
        "listed": len(entries),
        "ok": ok,
        "mismatch": mismatch,
        "missing": missing,
        "extra": extra,
        "eol_trap": eol_traps,
        "green": green,
    }


def print_result(res: dict) -> None:
    if res.get("prereq"):
        print(f"RED  {res['prereq']}")
        return
    tag = "GREEN" if res["green"] else "RED  "
    print(f"{tag} {res['manifest']}  [{res['mode']} 口径]  OK {res['ok']}/{res['listed']}")
    for m in res["mismatch"]:
        print(f"       mismatch: {m['path']}\n         清单 {m['listed']}\n         实算 {m['computed']}"
              f"  (text attr={m['attr']})")
    for t in res.get("eol_trap", []):
        print(f"       EOL 陷阱: {t['path']}\n         清单按{t.get('source', '原始字节')}记 {t['listed']}\n"
              f"         仓库口径(LF 归一/git blob)应为 {t['repo_lf']}  (text attr={t['attr']})\n"
              f"         ⇒ 该清单在工作树/提交物自洽、在仓库字节口径/干净检出不可复现（用 `write` 改写）")
    for p in res["missing"]:
        print(f"       缺件: {p}")
    for p in res["extra"]:
        print(f"       多件: {p}")


# --------------------------------------------------------------------------- #
# 命令：verify
# --------------------------------------------------------------------------- #
def cmd_verify(args) -> int:
    repo = find_repo(args.repo)
    targets = [Path(t) for t in args.targets]
    if not targets:
        print("用法错误：verify 需要一个或多个 SHA256SUMS.txt 或目录", file=sys.stderr)
        return 2
    mode = "disk" if args.disk_bytes else "repo"
    results = []
    for t in targets:
        if not t.exists():
            print(f"RED  前置缺失：目标不存在 {t}")
            results.append({"prereq": f"目标不存在 {t}"})
            continue
        base = t if t.is_dir() else t.parent
        if not base.is_dir():
            print(f"RED  前置缺失：目录不存在 {base}")
            results.append({"prereq": f"目录不存在 {base}"})
            continue
        res = verify_dir(base, repo, mode, args.check_extra, args.exclude)
        results.append(res)
        print_result(res)
    if args.json:
        Path(args.json).write_text(json.dumps(results, ensure_ascii=False, indent=2) + "\n",
                                   encoding="utf-8", newline="\n")
    bad = [r for r in results if r.get("prereq") or not r.get("green")]
    if any(r.get("prereq") for r in results) and not any(
            r.get("prereq") is None for r in results):
        return 2
    return 1 if bad else 0


# --------------------------------------------------------------------------- #
# 命令：reconcile（③ 逐目录对账）
# --------------------------------------------------------------------------- #
def find_manifests(roots: list[Path]) -> list[Path]:
    seen, out = set(), []
    for root in roots:
        if not root.is_dir():
            continue
        for p in sorted(root.rglob(MANIFEST_NAME)):
            if ".git" in p.parts:
                continue
            rp = p.resolve()
            if rp in seen:
                continue
            seen.add(rp)
            out.append(p)
    return out


def classify(res_repo: dict, res_disk: dict, freeze_all: bool) -> dict:
    listed = res_repo.get("listed", 0)
    missing = res_repo.get("missing", [])
    repo_ok = res_repo.get("ok", 0)
    disk_ok = res_disk.get("ok", 0)
    traps = res_repo.get("eol_trap", [])
    if traps:
        names = ", ".join(Path(t["path"]).name for t in traps[:3])
        return {"verdict": "RED",
                "why": f"EOL 陷阱 {len(traps)} 件（清单按工作树 CRLF 记，仓库口径为 LF）：{names}"}
    if missing:
        return {"verdict": "RED", "why": f"清单引用文件缺失 {len(missing)} 个"}
    if repo_ok == listed and disk_ok == listed:
        return {"verdict": "GREEN", "why": "仓库字节 == 盘上字节（两种口径一致）"}
    if repo_ok == listed:
        return {"verdict": "GREEN", "why": "仓库字节口径（盘上 CRLF ≠ blob，复核器默认口径）"}
    if disk_ok == listed:
        if freeze_all:
            return {"verdict": "GREEN", "why": "盘上字节口径且整目录已声明 -text 冻结字节"}
        return {"verdict": "RED", "why": "盘上字节口径但未声明整目录 .gitattributes * -text"}
    return {"verdict": "RED", "why": f"混合/漂移：仓库字节命中 {repo_ok}/{listed}、盘上字节命中 {disk_ok}/{listed}"}


def cmd_reconcile(args) -> int:
    repo = find_repo(args.repo)
    roots = [Path(r) for r in (args.targets or [str(repo)])]
    manifests = find_manifests(roots)
    if not manifests:
        print("RED  未找到任何 SHA256SUMS.txt", file=sys.stderr)
        return 2

    rows = []
    for mf in manifests:
        base = mf.parent
        r_repo = verify_dir(base, repo, "repo", False, args.exclude)
        r_disk = verify_dir(base, repo, "disk", False, args.exclude)
        if r_repo.get("prereq"):
            rows.append({"manifest": str(mf), "verdict": "RED", "why": r_repo["prereq"]})
            continue
        entries = parse_manifest(mf)
        external = [n for _, n in entries if not inside_repo(resolve_entry(base, n), repo)]
        inrepo = []
        for _, name in entries:
            path = resolve_entry(base, name)
            if inside_repo(path, repo):
                inrepo.append(path.resolve().relative_to(repo.resolve()).as_posix())
        attrs = check_attr_text(repo, inrepo)
        freeze_all = bool(inrepo) and all(attrs.get(r) == "unset" for r in inrepo)
        cls = classify(r_repo, r_disk, freeze_all)
        # 目录内未列入清单的文件（信息列，不作判据）
        listed_names = {n.replace("\\", "/").lstrip("./") for _, n in entries}
        extra = sorted(
            p.relative_to(base).as_posix() for p in base.rglob("*")
            if p.is_file() and p.relative_to(base).as_posix() != MANIFEST_NAME
            and p.relative_to(base).as_posix() not in listed_names
        )
        rows.append({
            "manifest": str(mf.relative_to(repo)).replace("\\", "/"),
            "dir": str(base.relative_to(repo)).replace("\\", "/"),
            "listed": r_repo["listed"],
            "repo_ok": r_repo["ok"],
            "disk_ok": r_disk["ok"],
            "missing": len(r_repo["missing"]),
            "extra": extra,
            "external": external,
            "gitattributes": (base / ".gitattributes").is_file(),
            "freeze_all": freeze_all,
            "verdict": cls["verdict"],
            "why": cls["why"],
        })

    # 人类可读表
    header = f"{'目录':<72} {'件':>3} {'仓库':>5} {'盘上':>5} {'缺':>3}  结论"
    print(header)
    print("-" * len(header))
    for r in rows:
        print(f"{r['dir']:<72} {r['listed']:>3} {r['repo_ok']:>5} {r['disk_ok']:>5} "
              f"{r.get('missing', 0):>3}  [{r['verdict']}] {r['why']}")
        if r.get("extra"):
            print(f"{'':<72} 未列入(信息，非判据): {', '.join(r['extra'])}")
        if r.get("external"):
            print(f"{'':<72} 仓外输入(按盘上字节): {', '.join(r['external'])}")

    bad = [r for r in rows if r["verdict"] != "GREEN"]
    print()
    print(f"reconcile: {len(rows) - len(bad)}/{len(rows)} GREEN"
          + (f"；RED {len(bad)}" if bad else ""))

    if args.json:
        Path(args.json).write_text(json.dumps(rows, ensure_ascii=False, indent=2) + "\n",
                                   encoding="utf-8", newline="\n")
    if args.md:
        write_md(Path(args.md), rows)
    return 1 if bad else 0


def write_md(path: Path, rows: list[dict]) -> None:
    lines = [
        "# 全仓 `SHA256SUMS.txt` 逐目录对账（③）",
        "",
        "> 生成：`python -I tools/evidence-hash/verify_sha256sums.py reconcile "
        "--md tools/evidence-hash/RECONCILE.md --json tools/evidence-hash/RECONCILE.json`",
        "> 口径：仓库字节（Git blob 内容；`text` 文件 = LF 归一，`-text` = 原始字节，仓外输入 = 盘上字节）。",
        "> 「仓库」列 = 按仓库字节命中的件数，「盘上」列 = 按盘上原始字节命中的件数。",
        "",
        "| 目录 | 件数 | 仓库字节 | 盘上字节 | 缺件 | 全目录冻结(`* -text`) | 结论 | 说明 |",
        "| --- | --- | --- | --- | --- | --- | --- | --- |",
    ]
    for r in rows:
        lines.append(
            f"| `{r['dir']}` | {r['listed']} | {r['repo_ok']} | {r['disk_ok']} | "
            f"{r.get('missing', 0)} | {'是' if r.get('freeze_all') else '否'} | "
            f"**{r['verdict']}** | {r['why']} |"
        )
    lines.append("")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")


# --------------------------------------------------------------------------- #
# 命令：write（显式修正清单到仓库字节口径）
# --------------------------------------------------------------------------- #
def cmd_write(args) -> int:
    repo = find_repo(args.repo)
    targets = [Path(t) for t in args.targets]
    if not targets:
        print("用法错误：write 需要一个或多个目录", file=sys.stderr)
        return 2
    mode = "disk" if args.disk_bytes else "repo"
    changed, failed = 0, 0
    for base in targets:
        manifest = base / MANIFEST_NAME
        if not manifest.is_file():
            print(f"RED  缺清单 {manifest}")
            failed += 1
            continue
        out_lines = []
        for raw in manifest.read_text(encoding="utf-8", errors="replace").splitlines():
            m = LINE_RE.match(raw)
            if not m or raw.lstrip().startswith("#"):
                out_lines.append(raw)
                continue
            prefix, name = m.group(2), m.group(3).rstrip("\r")
            path = resolve_entry(base, name)
            if not path.is_file():
                print(f"RED  {manifest}: 清单引用的文件不存在 {name}")
                failed += 1
                out_lines.append(raw)
                continue
            rel = None
            if inside_repo(path, repo):
                rel = path.resolve().relative_to(repo.resolve()).as_posix()
            attrs = check_attr_text(repo, [rel]) if rel else {}
            got = hash_path(path, mode, repo, rel, attrs.get(rel))
            if got != m.group(1).lower():
                changed += 1
            out_lines.append(f"{got}{prefix}{name}")
        manifest.write_text("\n".join(out_lines) + "\n", encoding="utf-8", newline="\n")
        print(f"写入 {manifest}（{len(out_lines)} 行，{mode} 口径）")
    if failed:
        return 2
    print(f"GREEN: write 完成（{changed} 行 hash 变更，mode={mode}）")
    return 0


# --------------------------------------------------------------------------- #
# 命令：selftest
# --------------------------------------------------------------------------- #
def _selftest_write_manifest(d: Path, items: list[tuple[str, str]], style_bin: int = 0) -> Path:
    mf = d / MANIFEST_NAME
    out = []
    for i, (h, name) in enumerate(items):
        marker = "*" if i < style_bin else ""
        out.append(f"{h} {marker}{name}")
    mf.write_text("\n".join(out) + "\n", encoding="utf-8", newline="\n")
    return mf


def cmd_selftest(_args) -> int:
    tmp = Path(tempfile.mkdtemp(prefix="ehc-selftest-"))
    fails = 0

    def report(ok: bool, name: str, detail: str = "") -> None:
        nonlocal fails
        if not ok:
            fails += 1
        print(f"{'PASS' if ok else 'RED '} {name}" + (f"  {detail}" if detail else ""))

    try:
        repo = tmp / "repo"
        (repo / "evidence").mkdir(parents=True)
        subprocess.run(["git", "init", "-q", str(repo)], check=True)
        subprocess.run(["git", "-C", str(repo), "config", "core.autocrlf", "true"], check=True)
        subprocess.run(["git", "-C", str(repo), "config", "user.email", "selftest@example.invalid"], check=True)
        subprocess.run(["git", "-C", str(repo), "config", "user.name", "selftest"], check=True)
        (repo / ".gitattributes").write_text("*.txt text eol=lf\n", encoding="utf-8", newline="\n")

        ev = repo / "evidence"
        crlf = b"line1\r\nline2\r\n"
        (ev / "crlf.txt").write_bytes(crlf)          # CRLF 工作树 + text 属性 ⇒ 仓库字节 = LF
        (ev / "frozen.bin").write_bytes(crlf)        # `-text` 冻结 ⇒ 仓库字节 = 原始
        ga = ev / ".gitattributes"
        ga.write_text("frozen.bin -text\n", encoding="utf-8", newline="\n")

        h_lf = sha256_bytes(normalize_lf(crlf))
        h_raw = sha256_bytes(crlf)

        # 0) 口径自检：两个文件在两种口径下的期望值不同
        got_lf = hash_path(ev / "crlf.txt", "repo", repo, "evidence/crlf.txt", "set")
        got_frozen = hash_path(ev / "frozen.bin", "repo", repo, "evidence/frozen.bin", "unset")
        report(got_lf == h_lf and got_frozen == h_raw and h_lf != h_raw,
               "0/11 口径：text 文件按 LF 归一、-text 文件按原始字节（两者不同）")

        # 1) 仓库字节口径（默认）→ 绿
        _selftest_write_manifest(ev, [(h_lf, "crlf.txt"), (h_raw, "frozen.bin")], style_bin=2)
        r_repo = verify_dir(ev, repo, "repo", False, [])
        report(r_repo["green"], "1/11 仓库字节口径：清单与仓库字节一致 → 绿",
               f"OK {r_repo['ok']}/{r_repo['listed']}")

        # 2) 改 1 位 hash → 必红
        _selftest_write_manifest(ev, [("0" * 64, "crlf.txt"), (h_raw, "frozen.bin")], style_bin=2)
        r_bad = verify_dir(ev, repo, "repo", False, [])
        report(not r_bad["green"] and any("crlf.txt" in m["path"] for m in r_bad["mismatch"]),
               "2/11 改 1 位 hash → 必红")

        # 3) 多件（--check-extra）→ 必红
        _selftest_write_manifest(ev, [(h_lf, "crlf.txt"), (h_raw, "frozen.bin")], style_bin=2)
        (ev / "ghost.txt").write_bytes(b"x\n")
        r_extra = verify_dir(ev, repo, "repo", True, [])
        report(not r_extra["green"] and "ghost.txt" in r_extra["extra"],
               "3/11 多件（--check-extra）→ 必红")
        (ev / "ghost.txt").unlink()

        # 4) 少件（清单引用但不存在的文件）→ 必红
        _selftest_write_manifest(ev, [(h_lf, "crlf.txt"), (h_raw, "missing.bin")], style_bin=2)
        r_missing = verify_dir(ev, repo, "repo", False, [])
        report(not r_missing["green"] and len(r_missing["missing"]) == 1,
               "4/11 少件（清单引用缺失文件）→ 必红")

        # 5) 口径只有一个：text 文件盘上 CRLF 时，仓库口径(LF)绿、--disk-bytes 红（盘上字节对不上 LF 清单）
        _selftest_write_manifest(ev, [(h_lf, "crlf.txt"), (h_raw, "frozen.bin")], style_bin=2)
        r_correct = verify_dir(ev, repo, "repo", False, [])
        r_disk = verify_dir(ev, repo, "disk", False, [])
        report(r_correct["green"] and not r_disk["green"],
               "5/11 口径只有一个：text 文件盘上 CRLF ⇒ 默认仓库口径(LF)绿、--disk-bytes 红")

        # 6) 声明 `* -text` 的目录：盘上恢复为原始字节后两种口径一致 → 绿
        (ev / "crlf.txt").write_bytes(crlf)
        ga.write_text("* -text\n", encoding="utf-8", newline="\n")
        _selftest_write_manifest(ev, [(h_raw, "crlf.txt"), (h_raw, "frozen.bin")], style_bin=2)
        r_decl_repo = verify_dir(ev, repo, "repo", False, [])
        r_decl_disk = verify_dir(ev, repo, "disk", False, [])
        report(r_decl_repo["green"] and r_decl_disk["green"],
               "6/11 已声明 * -text：仓库字节 == 盘上字节，两种口径都绿")

        # 7..10) EOL 陷阱（2026-10-12，同族坑第 4 次）：
        #   先建一个「受跟踪 blob = LF」的文件，再把盘上改回 CRLF。
        ga.write_text("frozen.bin -text\n", encoding="utf-8", newline="\n")
        (ev / "crlf.txt").write_bytes(b"line1\nline2\n")
        subprocess.run(["git", "-C", str(repo), "add", "-A"], check=True)
        subprocess.run(["git", "-C", str(repo), "commit", "-q", "-m", "selftest base"], check=True)
        (ev / "crlf.txt").write_bytes(crlf)          # 盘上 CRLF，blob 仍为 LF

        # 7) 清单按盘上 CRLF 记 ⇒ 必须红且点名（默认口径与 --disk-bytes 都要红，不许自洽假绿）
        _selftest_write_manifest(ev, [(h_raw, "crlf.txt"), (h_raw, "frozen.bin")], style_bin=2)
        r_trap = verify_dir(ev, repo, "repo", False, [])
        r_trap_disk = verify_dir(ev, repo, "disk", False, [])
        trapped = any("crlf.txt" in t["path"] for t in r_trap.get("eol_trap", []))
        report(not r_trap["green"] and trapped and not r_trap_disk["green"],
               "7/11 EOL 陷阱(盘上 CRLF / blob LF)：清单按盘上字节记 ⇒ 必红且点名（--disk-bytes 也红）",
               f"trap={len(r_trap.get('eol_trap', []))} disk_rc_trap={not r_trap_disk['green']}")

        # 8) EOL 陷阱(blob 自己也带 CRLF，即 m3cs 真实先例)：即使盘上已是 LF，记 blob 原始字节也必红
        (ev / "crlf.txt").write_bytes(b"line1\nline2\n")
        crlf_oid = subprocess.run(
            ["git", "-C", str(repo), "hash-object", "-w", "--stdin", "--no-filters"],
            input=crlf, capture_output=True, check=True).stdout.strip().decode()
        subprocess.run(["git", "-C", str(repo), "update-index", "--add", "--cacheinfo",
                        f"100644,{crlf_oid},evidence/blobcrlf.txt"], check=True)
        (ev / "blobcrlf.txt").write_bytes(b"line1\nline2\n")   # 盘上 LF
        subprocess.run(["git", "-C", str(repo), "commit", "-q", "-m", "blob crlf"], check=True)
        _selftest_write_manifest(
            ev, [(h_raw, "blobcrlf.txt"), (h_lf, "crlf.txt"), (h_raw, "frozen.bin")], style_bin=2)
        r_trap_blob = verify_dir(ev, repo, "repo", False, [])
        blob_trapped = any("blobcrlf.txt" in t["path"] and "blob" in t.get("source", "")
                           for t in r_trap_blob.get("eol_trap", []))
        report(not r_trap_blob["green"] and blob_trapped,
               "8/11 EOL 陷阱(blob 含 CRLF)：盘上 LF 但清单按 blob 原始字节记 ⇒ 必红且点名",
               f"trap={len(r_trap_blob.get('eol_trap', []))}")

        # 9) 不误伤：盘上 CRLF 但清单按仓库口径(LF)记 ⇒ 绿（现存 72 个目录就是这个形态）
        (ev / "crlf.txt").write_bytes(crlf)
        _selftest_write_manifest(ev, [(h_lf, "crlf.txt"), (h_raw, "frozen.bin")], style_bin=2)
        r_correct_crlf_disk = verify_dir(ev, repo, "repo", False, [])
        report(r_correct_crlf_disk["green"] and not r_correct_crlf_disk.get("eol_trap"),
               "9/11 不误伤：盘上 CRLF 但清单按仓库口径(LF)记 ⇒ 绿")

        # 10) 正确的 LF 样例：盘上 LF + 清单 LF ⇒ 绿
        (ev / "crlf.txt").write_bytes(b"line1\nline2\n")
        _selftest_write_manifest(ev, [(h_lf, "crlf.txt"), (h_raw, "frozen.bin")], style_bin=2)
        r_correct_lf = verify_dir(ev, repo, "repo", False, [])
        report(r_correct_lf["green"], "10/11 正确的 LF 样例（盘上 LF + 清单 LF）⇒ 绿")

        if fails:
            print(f"RED: verify_sha256sums selftest {fails} 项不过", file=sys.stderr)
            return 2
        print("GREEN: verify_sha256sums selftest 11/11"
              "（口径 + 改 hash / 多件 / 少件 / 口径切换 / 冻结声明 / EOL 陷阱×2 / 不误伤）")
        return 0
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


# --------------------------------------------------------------------------- #
def main() -> int:
    ap = argparse.ArgumentParser(
        prog="verify_sha256sums.py",
        description="SHA256SUMS.txt 通用复核器（默认仓库字节口径 = Git blob 内容）")
    ap.add_argument("cmd", nargs="?", default="verify",
                    help="verify（默认）/ reconcile / selftest")
    ap.add_argument("targets", nargs="*", help="verify：清单/目录；reconcile：扫描根（默认仓库根）")
    ap.add_argument("--repo", default=None, help="仓库根（默认自动探测）")
    ap.add_argument("--disk-bytes", action="store_true",
                    help="切到盘上原始字节口径（仅用于声明了 .gitattributes * -text 的目录）")
    ap.add_argument("--check-extra", action="store_true",
                    help="目录里未列入清单的文件也判红（多件牙齿）")
    ap.add_argument("--exclude", action="append", default=[],
                    help="--check-extra 时排除的 glob（可重复）")
    ap.add_argument("--json", default=None)
    ap.add_argument("--md", default=None)
    # 允许省略 `verify` 子命令：第一个位置参数不是已知命令时，按 verify 处理
    argv = sys.argv[1:]
    if not argv or argv[0] not in ("verify", "reconcile", "selftest", "write"):
        argv = ["verify"] + argv
    args = ap.parse_args(argv)

    if args.cmd == "selftest":
        return cmd_selftest(args)
    if args.cmd == "reconcile":
        return cmd_reconcile(args)
    if args.cmd == "write":
        return cmd_write(args)
    return cmd_verify(args)


if __name__ == "__main__":
    raise SystemExit(main())
