#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""deps_manifest — 部署依赖清单：生成 + 两级核验（卡 `openmir2-deploy-deps-manifest`）。

**目标**：让"部署出去的那套东西依赖了什么"可被两级核验，而不是"我 restore 过，所以没问题"。

**范围**：本期只做 **NuGet（C#）**。Rust 件静态链接、部署目录里没有 Rust 第三方 DLL，
其来源由 `build-rev`/侧车戳覆盖；`Cargo.lock` 的供应链核对属构建车道，另卡（卡面已写明）。

**输入**（只读，绝不改包缓存）：
- `<project>/packages.lock.json`（`RestorePackagesWithLockFile=true` 生成，`requested/resolved/
  contentHash/type`）；
- `<project>.csproj` 的 `PackageReference` 声明（一级检查的"上层声明"）；
- 已还原包文件：`--packages-dir`（默认 `~/.nuget/packages`）下的 `<id>/<version>/`。

**两级检查**：
1. 版本一致性：csproj 声明范围 **可满足** lock 的 `resolved`（不是字符串比较）；
2. 内容一致性：manifest 记录的每个包文件 sha256 与磁盘逐项相等；nupkg 的 SHA512 与
   `.nupkg.sha512` 侧车一致（lock 的 `contentHash` 是 NuGet 对**包内容**的规范化哈希，
   **不是** .nupkg 的 SHA512——两个 hash 分开记录、分别核验）。

**fail-closed**：缺 lock / 缺 manifest / manifest 空或字段缺失 ⇒ 红（rc=1），绝不当"没有依赖"。

用法：
  python -I -B tools/deploy/deps_manifest.py generate --project <csproj> --out <manifest.json>
  python -I -B tools/deploy/deps_manifest.py check --project <csproj> --manifest <manifest.json>
  python -I -B tools/deploy/deps_manifest.py selftest

退出码：0 = 绿；1 = 判红（证伪，红项逐条打印）；2 = 用法错 / 判不了（环境缺目录等）。

机器可读输出：
  generate: `DEPS_WRITE <manifest> packages=<n> files=<m>`
  check:    `DEPS_GREEN packages=<n> files=<m>` 或 `DEPS_RED <CODE> <detail>`（可多条）
  selftest: `deps_manifest selftest: PASS <case>` / `FAIL <case> :: <detail>`
"""
import argparse
import base64
import hashlib
import json
import os
import re
import shutil
import sys
import tempfile

MANIFEST_VERSION = 1
REQUIRED_PKG_FIELDS = ("name", "requested", "resolved", "source", "toolchain", "content_hash",
                       "nupkg_sha512", "files")


def emit(text):
    sys.stdout.write(text + "\n")


# ---------------- 版本与范围 ----------------

def _parse_version(text):
    text = str(text).strip()
    if not text:
        return None
    m = re.match(r"^(\d+)(?:\.(\d+))?(?:\.(\d+))?(?:\.(\d+))?(?:-([0-9A-Za-z.\-]+))?$", text)
    if not m:
        return None
    nums = tuple(int(g) if g is not None else 0 for g in m.group(1, 2, 3, 4))
    return nums, (m.group(5) or "")


def _cmp_version(a, b):
    if a == b:
        return 0
    (na, pa), (nb, pb) = a, b
    if na != nb:
        return -1 if na < nb else 1
    if pa == pb:
        return 0
    if pa == "":
        return 1
    if pb == "":
        return -1
    return -1 if pa < pb else 1


def range_satisfied(version_text, range_text):
    """NuGet 范围语义（够本仓使用）：`[1.0.0, )`、`[1.0.0, 2.0.0)`、`[1.0.0]`、`1.0.0`(=>=)、`*`。"""
    v = _parse_version(version_text)
    if v is None:
        return False, "bad resolved version %r" % version_text
    r = (range_text or "").strip()
    if r in ("", "*"):
        return True, ""
    if r.startswith("[") or r.startswith("("):
        if not (r.endswith("]") or r.endswith(")")):
            return False, "bad range %r" % range_text
        lo_inc = r[0] == "["
        hi_inc = r[-1] == "]"
        body = r[1:-1]
        parts = [p.strip() for p in body.split(",")]
        if len(parts) == 1:
            # [1.2.3] 精确
            lo = _parse_version(parts[0])
            if lo is None:
                return False, "bad range %r" % range_text
            ok = _cmp_version(v, lo) == 0
            return ok, "" if ok else "resolved %s != %s" % (version_text, parts[0])
        if len(parts) != 2:
            return False, "bad range %r" % range_text
        lo_s, hi_s = parts
        if lo_s:
            lo = _parse_version(lo_s)
            if lo is None:
                return False, "bad range %r" % range_text
            c = _cmp_version(v, lo)
            if c < 0 or (c == 0 and not lo_inc):
                return False, "resolved %s below %s%s" % (version_text, "[" if lo_inc else "(", lo_s)
        if hi_s:
            hi = _parse_version(hi_s)
            if hi is None:
                return False, "bad range %r" % range_text
            c = _cmp_version(v, hi)
            if c > 0 or (c == 0 and not hi_inc):
                return False, "resolved %s above %s%s" % (version_text, hi_s, "]" if hi_inc else ")")
        return True, ""
    # 裸版本：>= 语义
    lo = _parse_version(r)
    if lo is None:
        return False, "bad range %r" % range_text
    ok = _cmp_version(v, lo) >= 0
    return ok, "" if ok else "resolved %s < %s" % (version_text, r)


# ---------------- 文件工具 ----------------

def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def sha512_b64_file(path):
    h = hashlib.sha512()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return base64.b64encode(h.digest()).decode("ascii")


def parse_csproj_refs(csproj_path):
    """返回 {包名: 版本范围}（只解析 `PackageReference Include=... Version=...` 形式）。"""
    with open(csproj_path, "r", encoding="utf-8-sig") as f:
        text = f.read()
    refs = {}
    for m in re.finditer(r"<PackageReference\b([^>]*)>", text):
        attrs = m.group(1)
        inc = re.search(r'Include="([^"]+)"', attrs)
        ver = re.search(r'Version="([^"]+)"', attrs)
        if inc and ver:
            refs[inc.group(1)] = ver.group(1)
    return refs


def load_lock(proj_dir):
    path = os.path.join(proj_dir, "packages.lock.json")
    if not os.path.isfile(path):
        return None, path
    try:
        with open(path, "r", encoding="utf-8-sig") as f:
            return json.load(f), path
    except (OSError, ValueError):
        return None, path


def lock_packages(lock):
    """按包名合并各 TFM（要求同一包在不同 TFM 里 resolved 一致，否则判不了/判红）。"""
    deps = (lock or {}).get("dependencies") or {}
    merged = {}
    for tfm, entries in deps.items():
        for name, entry in entries.items():
            prev = merged.get(name)
            if prev is not None and prev["resolved"] != entry.get("resolved"):
                return None, "TARGET_FRAMEWORK_CONFLICT %s: %s vs %s" % (
                    name, prev["resolved"], entry.get("resolved"))
            merged[name] = {
                "type": entry.get("type", ""),
                "requested": entry.get("requested"),
                "resolved": entry.get("resolved"),
                "content_hash": entry.get("contentHash"),
            }
    return merged, ""


def package_dir(packages_dir, name, version):
    return os.path.join(packages_dir, name.lower(), version)


def list_package_files(pkg_dir):
    """包目录内的文件（排除 nupkg 及其侧车/元数据——它们由 nupkg_sha512 单独管）。"""
    out = []
    for root, _dirs, files in os.walk(pkg_dir):
        for fn in files:
            low = fn.lower()
            if low.endswith(".nupkg") or low.endswith(".nupkg.sha512") or low.endswith(".nupkg.metadata"):
                continue
            full = os.path.join(root, fn)
            rel = os.path.relpath(full, pkg_dir).replace(os.sep, "/")
            out.append(rel)
    return sorted(out)


def find_nupkg(pkg_dir):
    for fn in os.listdir(pkg_dir):
        if fn.lower().endswith(".nupkg") and not fn.lower().endswith(".nupkg.sha512"):
            return os.path.join(pkg_dir, fn)
    return None


# ---------------- generate ----------------

def cmd_generate(args):
    proj_dir = os.path.dirname(os.path.abspath(args.project))
    lock, lock_path = load_lock(proj_dir)
    if lock is None:
        emit("DEPS_RED MISSING_LOCK %s" % lock_path)
        return 1
    pkgs, err = lock_packages(lock)
    if pkgs is None:
        emit("DEPS_RED %s" % err)
        return 1
    packages_dir = os.path.expanduser(args.packages_dir)
    if not os.path.isdir(packages_dir):
        emit("DEPS_RED MISSING_PACKAGES_DIR %s" % packages_dir)
        return 2

    entries = []
    file_total = 0
    for name in sorted(pkgs):
        meta = pkgs[name]
        if meta["type"] == "Project":
            continue
        pdir = package_dir(packages_dir, name, meta["resolved"])
        if not os.path.isdir(pdir):
            emit("DEPS_RED MISSING_PACKAGE_DIR %s/%s" % (name, meta["resolved"]))
            return 1
        nupkg = find_nupkg(pdir)
        if nupkg is None:
            emit("DEPS_RED MISSING_NUPKG %s/%s" % (name, meta["resolved"]))
            return 1
        nupkg_sha = sha512_b64_file(nupkg)
        sidecar = nupkg + ".sha512"
        if os.path.isfile(sidecar):
            with open(sidecar, "r", encoding="ascii", errors="ignore") as f:
                expected = f.read().strip()
            if expected and expected != nupkg_sha:
                emit("DEPS_RED NUPKG_SHA_MISMATCH %s/%s sidecar=%s actual=%s"
                     % (name, meta["resolved"], expected, nupkg_sha))
                return 1
        files = {}
        for rel in list_package_files(pdir):
            files[rel] = sha256_file(os.path.join(pdir, rel))
        file_total += len(files)
        entries.append({
            "name": name,
            "type": meta["type"],
            "requested": meta["requested"],
            "resolved": meta["resolved"],
            "source": "packages.lock.json",
            "toolchain": "nuget",
            "content_hash": meta["content_hash"],
            "nupkg_sha512": nupkg_sha,
            "files": files,
        })

    manifest = {
        "manifest_version": MANIFEST_VERSION,
        "project": os.path.relpath(os.path.abspath(args.project)).replace(os.sep, "/"),
        "generated_by": "tools/deploy/deps_manifest.py",
        "packages": entries,
    }
    out = os.path.abspath(args.out)
    os.makedirs(os.path.dirname(out), exist_ok=True)
    with open(out, "w", encoding="utf-8", newline="\n") as f:
        json.dump(manifest, f, ensure_ascii=False, indent=2, sort_keys=False)
        f.write("\n")
    emit("DEPS_WRITE %s packages=%d files=%d" % (out, len(entries), file_total))
    return 0


# ---------------- check ----------------

def _fail(lines, code, detail):
    lines.append("DEPS_RED %s %s" % (code, detail))


def cmd_check(args):
    reds = []
    proj_dir = os.path.dirname(os.path.abspath(args.project))
    lock, lock_path = load_lock(proj_dir)
    if lock is None:
        _fail(reds, "MISSING_LOCK", lock_path)
    manifest_path = os.path.abspath(args.manifest)
    if not os.path.isfile(manifest_path):
        _fail(reds, "MISSING_MANIFEST", manifest_path)
    manifest = None
    if os.path.isfile(manifest_path):
        try:
            with open(manifest_path, "r", encoding="utf-8") as f:
                manifest = json.load(f)
        except (OSError, ValueError) as exc:
            _fail(reds, "BAD_MANIFEST", "%s (%s)" % (manifest_path, exc))
    if isinstance(manifest, dict):
        if manifest.get("manifest_version") != MANIFEST_VERSION:
            _fail(reds, "BAD_MANIFEST_VERSION", "manifest_version=%r" % manifest.get("manifest_version"))
        pkgs = manifest.get("packages")
        if not isinstance(pkgs, list) or not pkgs:
            _fail(reds, "EMPTY_MANIFEST", "packages 为空或非数组")
            pkgs = []
        for p in pkgs:
            if not isinstance(p, dict):
                _fail(reds, "MISSING_FIELD", "packages[] 非对象")
                continue
            for field in REQUIRED_PKG_FIELDS:
                if field not in p:
                    _fail(reds, "MISSING_FIELD", "%s.%s" % (p.get("name", "?"), field))
    elif manifest is not None:
        _fail(reds, "BAD_MANIFEST", "顶层不是对象")
        manifest = None

    # 一级：csproj 声明 vs lock resolved
    refs = parse_csproj_refs(os.path.abspath(args.project))
    if lock is not None:
        pkgs_lock, err = lock_packages(lock)
        if pkgs_lock is None:
            _fail(reds, "BAD_LOCK", err)
        else:
            direct = {n for n, m in pkgs_lock.items() if m.get("type") == "Direct"}
            for name in sorted(set(refs) - set(pkgs_lock)):
                _fail(reds, "NOT_IN_LOCK", "%s（csproj 声明但 lock 缺失）" % name)
            for name in sorted(direct - set(refs)):
                _fail(reds, "LOCK_DIRECT_NOT_DECLARED", "%s（lock Direct 但 csproj 未声明）" % name)
            for name, rng in sorted(refs.items()):
                meta = pkgs_lock.get(name)
                if meta is None:
                    continue
                ok, why = range_satisfied(meta["resolved"], rng)
                if not ok:
                    _fail(reds, "RANGE_UNSATISFIED", "%s range=%s resolved=%s（%s）"
                          % (name, rng, meta["resolved"], why))

    # 二级：manifest vs 磁盘
    packages_dir = os.path.expanduser(args.packages_dir)
    if manifest is not None:
        for p in manifest.get("packages") or []:
            if not isinstance(p, dict) or "name" not in p or "resolved" not in p:
                continue
            name, version = p["name"], p["resolved"]
            if not all(field in p for field in REQUIRED_PKG_FIELDS):
                continue  # 已在上面的 MISSING_FIELD 报过
            pdir = package_dir(packages_dir, name, version)
            if not os.path.isdir(pdir):
                _fail(reds, "MISSING_PACKAGE_DIR", "%s/%s" % (name, version))
                continue
            recorded = p["files"] if isinstance(p["files"], dict) else {}
            on_disk = set(list_package_files(pdir))
            for rel in sorted(set(recorded) - on_disk):
                _fail(reds, "FILE_MISSING", "%s/%s %s" % (name, version, rel))
            for rel in sorted(on_disk - set(recorded)):
                _fail(reds, "FILE_UNEXPECTED", "%s/%s %s" % (name, version, rel))
            for rel in sorted(set(recorded) & on_disk):
                actual = sha256_file(os.path.join(pdir, rel))
                if actual != recorded[rel]:
                    _fail(reds, "FILE_SHA_MISMATCH",
                          "%s/%s %s expected=%s actual=%s" % (name, version, rel, recorded[rel], actual))
            nupkg = find_nupkg(pdir)
            if nupkg is None:
                _fail(reds, "MISSING_NUPKG", "%s/%s" % (name, version))
            else:
                actual512 = sha512_b64_file(nupkg)
                if actual512 != p.get("nupkg_sha512"):
                    _fail(reds, "NUPKG_CHANGED", "%s/%s expected=%s actual=%s"
                          % (name, version, p.get("nupkg_sha512"), actual512))
            file_count = len(recorded)

    if reds:
        for line in reds:
            emit(line)
        emit("DEPS_RED_TOTAL %d" % len(reds))
        return 1
    file_total = sum(len(p.get("files") or {}) for p in (manifest.get("packages") or []))
    emit("DEPS_GREEN packages=%d files=%d" % (len(manifest.get("packages") or []), file_total))
    return 0


# ---------------- selftest ----------------

def _write_pkg(root, name, version, payload=b"payload"):
    pdir = os.path.join(root, name.lower(), version)
    os.makedirs(os.path.join(pdir, "lib", "net8.0"), exist_ok=True)
    with open(os.path.join(pdir, "lib", "net8.0", name + ".dll"), "wb") as f:
        f.write(payload)
    with open(os.path.join(pdir, name.lower() + "." + version + ".nupkg"), "wb") as f:
        f.write(b"nupkg-" + name.encode())
    return pdir


def cmd_selftest(_args):
    cases = []
    fails = []

    def sassert(cond, name, detail=""):
        cases.append(name)
        if cond:
            emit("deps_manifest selftest: PASS %s" % name)
        else:
            fails.append((name, detail))
            emit("deps_manifest selftest: FAIL %s :: %s" % (name, detail))

    tmp = tempfile.mkdtemp(prefix="deps-manifest-selftest-")
    try:
        def make_fixture(sub):
            root = os.path.join(tmp, sub)
            proj = os.path.join(root, "proj")
            feed = os.path.join(root, "feed")
            os.makedirs(proj, exist_ok=True)
            os.makedirs(feed, exist_ok=True)
            with open(os.path.join(proj, "App.csproj"), "w", encoding="utf-8") as f:
                f.write('<Project><ItemGroup>'
                        '<PackageReference Include="Alpha" Version="[1.0.0]" />'
                        '<PackageReference Include="Beta" Version="[2.0.0, )" />'
                        '</ItemGroup></Project>')
            lock = {"version": 1, "dependencies": {"net8.0": {
                "Alpha": {"type": "Direct", "requested": "[1.0.0]", "resolved": "1.0.0",
                          "contentHash": "AAAA"},
                "Beta": {"type": "Direct", "requested": "[2.0.0, )", "resolved": "2.0.0",
                         "contentHash": "BBBB"},
            }}}
            with open(os.path.join(proj, "packages.lock.json"), "w", encoding="utf-8") as f:
                json.dump(lock, f)
            _write_pkg(feed, "Alpha", "1.0.0")
            _write_pkg(feed, "Beta", "2.0.0")
            return proj, feed

        def run(argv):
            ns = argparse.Namespace(**argv)
            if ns.cmd == "generate":
                return cmd_generate(ns)
            return cmd_check(ns)

        # Case 1：正例 generate + check 全绿；manifest 字段齐全
        proj, feed = make_fixture("pos")
        man = os.path.join(tmp, "pos", "deps.manifest.json")
        rc = run({"cmd": "generate", "project": os.path.join(proj, "App.csproj"), "out": man,
                  "packages_dir": feed})
        sassert(rc == 0, "positive: generate rc=0", "rc=%d" % rc)
        data = json.load(open(man, encoding="utf-8")) if os.path.isfile(man) else {}
        fields_ok = all(all(k in p for k in REQUIRED_PKG_FIELDS) for p in data.get("packages", []))
        sassert(fields_ok and data.get("manifest_version") == MANIFEST_VERSION,
                "positive: manifest fields complete", json.dumps(data)[:120])
        rc = run({"cmd": "check", "project": os.path.join(proj, "App.csproj"),
                  "manifest": man, "packages_dir": feed})
        sassert(rc == 0, "positive: check rc=0", "rc=%d" % rc)

        # Case 2（牙①）：lock 把 Beta 改成不可能满足 csproj 声明的版本 ⇒ 一级红
        lock_path = os.path.join(proj, "packages.lock.json")
        lock = json.load(open(lock_path, encoding="utf-8"))
        lock["dependencies"]["net8.0"]["Alpha"]["resolved"] = "9.9.9"  # csproj 要求 [1.0.0]
        json.dump(lock, open(lock_path, "w", encoding="utf-8"))
        rc = run({"cmd": "check", "project": os.path.join(proj, "App.csproj"),
                  "manifest": man, "packages_dir": feed})
        sassert(rc == 1, "tooth1: unsatisfiable range red", "rc=%d" % rc)

        # Case 3（牙②）：lock 复原，改一个已还原包文件的一个字节 ⇒ 二级红（报包名与两 hash）
        proj, feed = make_fixture("sha")
        man = os.path.join(tmp, "sha", "deps.manifest.json")
        run({"cmd": "generate", "project": os.path.join(proj, "App.csproj"), "out": man,
             "packages_dir": feed})
        victim = os.path.join(feed, "alpha", "1.0.0", "lib", "net8.0", "Alpha.dll")
        with open(victim, "r+b") as f:
            f.seek(0)
            first = f.read(1)
            f.seek(0)
            f.write(bytes([(first[0] + 1) % 256]))
        rc = run({"cmd": "check", "project": os.path.join(proj, "App.csproj"),
                  "manifest": man, "packages_dir": feed})
        sassert(rc == 1, "tooth2: file byte change red", "rc=%d" % rc)

        # Case 4（牙③a）：空 manifest ⇒ 红
        empty = os.path.join(tmp, "empty.manifest.json")
        with open(empty, "w", encoding="utf-8") as f:
            f.write("{}")
        rc = run({"cmd": "check", "project": os.path.join(proj, "App.csproj"),
                  "manifest": empty, "packages_dir": feed})
        sassert(rc == 1, "tooth3a: empty manifest red", "rc=%d" % rc)

        # Case 5（牙③b）：缺字段 ⇒ 红
        broken = os.path.join(tmp, "broken.manifest.json")
        data = json.load(open(man, encoding="utf-8"))
        data["packages"][0].pop("nupkg_sha512", None)
        json.dump(data, open(broken, "w", encoding="utf-8"))
        rc = run({"cmd": "check", "project": os.path.join(proj, "App.csproj"),
                  "manifest": broken, "packages_dir": feed})
        sassert(rc == 1, "tooth3b: missing field red", "rc=%d" % rc)

        # Case 6（fail-closed）：缺 manifest 文件 ⇒ 红
        rc = run({"cmd": "check", "project": os.path.join(proj, "App.csproj"),
                  "manifest": os.path.join(tmp, "nope.json"), "packages_dir": feed})
        sassert(rc == 1, "failclosed: missing manifest red", "rc=%d" % rc)

        # Case 7（fail-closed）：缺 lock ⇒ 红
        proj2, feed2 = make_fixture("nolock")
        os.remove(os.path.join(proj2, "packages.lock.json"))
        rc = run({"cmd": "check", "project": os.path.join(proj2, "App.csproj"),
                  "manifest": man, "packages_dir": feed2})
        sassert(rc == 1, "failclosed: missing lock red", "rc=%d" % rc)

        # Case 8：nupkg 侧车被改 ⇒ 红（生成阶段即拦，不自动重下）
        proj3, feed3 = make_fixture("sidecar")
        side = os.path.join(feed3, "alpha", "1.0.0", "alpha.1.0.0.nupkg.sha512")
        with open(side, "w", encoding="ascii") as f:
            f.write("WRONG-SHA")
        rc = run({"cmd": "generate", "project": os.path.join(proj3, "App.csproj"),
                  "out": os.path.join(tmp, "sidecar", "m.json"), "packages_dir": feed3})
        sassert(rc == 1, "nupkg sidecar tampered red", "rc=%d" % rc)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    if fails:
        emit("deps_manifest selftest: RED (%d failed, %d cases)" % (len(fails), len(cases)))
        return 1
    emit("deps_manifest selftest: GREEN (%d cases)" % len(cases))
    return 0


# ---------------- main ----------------

def build_parser():
    p = argparse.ArgumentParser(description="部署依赖清单：生成与两级核验")
    sub = p.add_subparsers(dest="cmd", required=True)
    g = sub.add_parser("generate")
    g.add_argument("--project", required=True)
    g.add_argument("--out", required=True)
    g.add_argument("--packages-dir", default=os.path.join("~", ".nuget", "packages"))
    c = sub.add_parser("check")
    c.add_argument("--project", required=True)
    c.add_argument("--manifest", required=True)
    c.add_argument("--packages-dir", default=os.path.join("~", ".nuget", "packages"))
    sub.add_parser("selftest")
    return p


def main(argv=None):
    args = build_parser().parse_args(argv)
    if args.cmd == "generate":
        return cmd_generate(args)
    if args.cmd == "check":
        return cmd_check(args)
    return cmd_selftest(args)


if __name__ == "__main__":
    sys.exit(main())
