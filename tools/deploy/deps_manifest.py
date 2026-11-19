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
3. **部署目录（`--deploy-dir`，可选）**：把 manifest 里每个包的**运行时资产**
   （`project.assets.json` 的 `runtime`/`native` 列表，按 TFM 已选定）按 basename 映射到
   `<deploy-dir>/<name>`：缺失 ⇒ `MISSING_DEPLOYED`；sha 不符 ⇒ `DEPLOYED_SHA_MISMATCH`；
   两个**不同包**映射到同一文件名且 sha 不同 ⇒ `AMBIGUOUS_MAPPING`；部署目录里多余的 DLL
   只 `WARN_EXTRA`（自家件由 `stamp-binaries` 管）。目录缺失/为空 ⇒ 红。

**fail-closed**：缺 lock / 缺 manifest / manifest 空或字段缺失 ⇒ 红（rc=1），绝不当"没有依赖"。

**构建口径是显式判据（卡 `openmir2-deps-build-config-pinning`）**：输出目录一律按
`--build-config Release|Debug`（缺省取 manifest 里记录的 `build_config`）**确定性解析**
（`bin/<config>/` 或 `bin/<config>/<tfm>/`），**不再按 mtime 探测"最新目录"**——
曾经因为 manifest 在 Debug 口径生成、而部署走 Release，主仓库一次 Release 构建就让
`BUILD_OUTPUT_MISMATCH` 变红、整栈起不来。manifest 的 `build_config` 缺失/非法、
`--build-config` 与 manifest 不一致、或按配置解析不到输出目录 ⇒ 一律红（fail-closed）。

用法：
  python -I -B tools/deploy/deps_manifest.py generate --projects <a;b;c> --out <manifest.json>
      [--build-config Release|Debug] [--exceptions <exceptions.json>]
  python -I -B tools/deploy/deps_manifest.py check --projects <a;b;c> --manifest <manifest.json>
      [--build-config Release|Debug] [--deploy-dir <目录>]
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
import io
import json
import os
import re
import shutil
import sys
import tempfile

MANIFEST_VERSION = 1
# 构建配置是**显式口径**：输出目录按它确定性解析（不看 mtime）。
BUILD_CONFIGS = ("Release", "Debug")
REQUIRED_PKG_FIELDS = ("name", "requested", "resolved", "source", "toolchain", "content_hash",
                       "nupkg_sha512", "files", "runtime_assets", "shipped_assets")


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


def load_assets_runtime(proj_dir):
    """从 `obj/project.assets.json` 取每个包的运行时资产（runtime+native，已按 TFM 选定）。

    返回 `(map, err)`：`map[name] = [relpath,...]`（去重排序）；缺文件/不可解析 ⇒ err。
    """
    path = os.path.join(proj_dir, "obj", "project.assets.json")
    if not os.path.isfile(path):
        return None, "MISSING_ASSETS %s" % path
    try:
        with open(path, "r", encoding="utf-8-sig") as f:
            data = json.load(f)
    except (OSError, ValueError) as exc:
        return None, "BAD_ASSETS %s (%s)" % (path, exc)
    out = {}
    targets = data.get("targets") or {}
    for _tfm, libs in targets.items():
        for key, entry in (libs or {}).items():
            if not isinstance(entry, dict):
                continue
            name = key.split("/", 1)[0]
            rels = set(out.get(name, []))
            for group in ("runtime", "native"):
                for rel in (entry.get(group) or {}):
                    if rel.rsplit("/", 1)[-1] == "_._":
                        continue  # NuGet 占位符：不是真实资产
                    rels.add(rel)
            out[name] = sorted(rels)
    return out, ""


def load_exceptions(proj_dir, explicit_path):
    """读取**显式例外**（只压制 `MISSING_DEPLOYED`，逐条必须带理由）。

    来源：`--exceptions <path>`，否则自动找主项目目录下的 `deps.exceptions.json`（可不存在）。
    格式：`[{"pkg": "...", "version": "...", "reason": "..."}]`；理由为空 ⇒ 判红（不能空口豁免）。
    """
    path = explicit_path or os.path.join(proj_dir, "deps.exceptions.json")
    if not explicit_path and not os.path.isfile(path):
        return [], ""
    if not os.path.isfile(path):
        return None, "MISSING_EXCEPTIONS %s" % path
    try:
        with open(path, "r", encoding="utf-8-sig") as f:
            data = json.load(f)
    except (OSError, ValueError) as exc:
        return None, "BAD_EXCEPTIONS %s (%s)" % (path, exc)
    if not isinstance(data, list):
        return None, "BAD_EXCEPTIONS %s（顶层必须是数组）" % path
    out = []
    for item in data:
        if not isinstance(item, dict):
            return None, "BAD_EXCEPTIONS %s（条目必须是对象）" % path
        pkg = item.get("pkg")
        ver = item.get("version")
        reason = (item.get("reason") or "").strip()
        if not pkg or not ver:
            return None, "BAD_EXCEPTIONS %s（缺 pkg/version）" % path
        if not reason:
            return None, "BAD_EXCEPTIONS %s（%s@%s 理由为空，禁止空口豁免）" % (path, pkg, ver)
        out.append({"pkg": pkg, "version": ver, "reason": reason})
    return out, ""


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


def resolve_output_dir(proj_dir, config):
    """按**显式构建配置**解析输出目录（确定性；不看 mtime、不猜"最新"）。

    两种布局：`bin/<config>/<tfm>/`（SDK 默认）与 `bin/<config>/`（`AppendTargetFrameworkToOutputPath=false`）。
    判据：`bin/<config>` 本身含 DLL ⇒ 就是它；否则要求**恰好一个**子目录含 DLL；
    其它情况返回 None，由调用方按 fail-closed 判红（绝不"挑一个看起来新的"）。

    为什么不能只看 assets 的 runtime 列表：`System.Text.Json` 等在 net8.0 共享框架里，
    assets 仍列为运行时资产但**不会复制到输出目录**；`_._` 是占位符。装运集必须来自构建输出。
    """
    base = os.path.join(proj_dir, "bin", config)
    if not os.path.isdir(base):
        return None

    def has_dll(d):
        try:
            return any(fn.lower().endswith(".dll") and os.path.isfile(os.path.join(d, fn))
                       for fn in os.listdir(d))
        except OSError:
            return False

    if has_dll(base):
        return base
    subs = [os.path.join(base, name) for name in sorted(os.listdir(base))
            if os.path.isdir(os.path.join(base, name))]
    subs = [d for d in subs if has_dll(d)]
    return subs[0] if len(subs) == 1 else None


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

def split_projects(args):
    raw = getattr(args, "projects", None)
    if raw:
        parts = [p.strip() for p in re.split(r"[;,]", raw) if p.strip()]
    else:
        single = getattr(args, "project", None)
        parts = [single] if single else []
    return [os.path.abspath(p) for p in parts]


def cmd_generate(args):
    proj_paths = split_projects(args)
    if not proj_paths:
        emit("DEPS_RED MISSING_PROJECTS（--project 或 --projects 必给）")
        return 2
    exceptions, eerr = load_exceptions(os.path.dirname(proj_paths[0]),
                                       getattr(args, "exceptions", None))
    if exceptions is None:
        emit("DEPS_RED %s" % eerr)
        return 1
    projects_rel = []
    output_dirs = {}
    configs = set()
    merged = {}
    assets_union = {}
    shipped_union = {}
    out_arg = getattr(args, "output_dir", None)
    cfg_arg = getattr(args, "build_config", None)
    if cfg_arg is not None and cfg_arg not in BUILD_CONFIGS:
        emit("DEPS_RED BAD_BUILD_CONFIG --build-config=%r（只接受 %s）"
             % (cfg_arg, "/".join(BUILD_CONFIGS)))
        return 2
    loaded = []
    for proj in proj_paths:
        proj_dir = os.path.dirname(proj)
        if not os.path.isfile(proj):
            emit("DEPS_RED MISSING_PROJECT %s" % proj)
            return 1
        lock, lock_path = load_lock(proj_dir)
        if lock is None:
            emit("DEPS_RED MISSING_LOCK %s" % lock_path)
            return 1
        pkgs, err = lock_packages(lock)
        if pkgs is None:
            emit("DEPS_RED %s" % err)
            return 1
        assets, aerr = load_assets_runtime(proj_dir)
        if assets is None:
            emit("DEPS_RED %s" % aerr)
            return 1
        if out_arg:
            output_dir = os.path.abspath(out_arg)
            config = ""
            parts = output_dir.replace("\\", "/").split("/")
            if "bin" in parts:
                idx = len(parts) - 1 - parts[::-1].index("bin")
                if idx + 1 < len(parts):
                    config = parts[idx + 1]
            if cfg_arg is not None:
                if config in BUILD_CONFIGS and config != cfg_arg:
                    emit("DEPS_RED BUILD_CONFIG_CONFLICT --output-dir 隐含 %s，--build-config=%s"
                         % (config, cfg_arg))
                    return 2
                config = cfg_arg
        else:
            # 显式口径：--build-config 优先；否则按固定优先级（Release → Debug）取**存在的**，
            # 只用存在性、不看 mtime。
            config = cfg_arg or next(
                (c for c in BUILD_CONFIGS if resolve_output_dir(proj_dir, c) is not None),
                "")
            output_dir = resolve_output_dir(proj_dir, config) if config else None
            if not output_dir:
                emit("DEPS_RED MISSING_OUTPUT_DIR %s bin/%s（按显式配置解析不到输出目录；"
                     "先构建再 generate，或用 --build-config 指定）" % (proj, config or "?"))
                return 2
        configs.add(config)
        projects_rel.append(proj)
        output_dirs[proj] = output_dir or ""
        out_files = set()
        if output_dir and os.path.isdir(output_dir):
            out_files = {fn for fn in os.listdir(output_dir)
                         if os.path.isfile(os.path.join(output_dir, fn))}
        loaded.append((proj, pkgs, assets, out_files))
        for name, meta in pkgs.items():
            # 部署集由**主项目**（第一个）的还原图决定；后续项目只"补缺"（典型：插件的 Direct 包）。
            # 同名但版本不同时保留主项目的版本（运行时按主项目输出装配），不判冲突。
            if name not in merged:
                merged[name] = meta
            assets_union.setdefault(name, set()).update(assets.get(name, []))
            for rel in assets.get(name, []):
                if rel.rsplit("/", 1)[-1] in out_files:
                    shipped_union.setdefault(name, set()).add(rel)
    if len(configs) > 1:
        emit("DEPS_RED BUILD_CONFIG_CONFLICT %s" % ",".join(sorted(configs)))
        return 1
    build_config = sorted(configs)[0]
    if build_config not in BUILD_CONFIGS:
        emit("DEPS_RED BAD_BUILD_CONFIG 无法从项目布局确定构建配置（%r）；请用 --build-config %s"
             % (build_config, "|".join(BUILD_CONFIGS)))
        return 2
    pkgs = merged
    assets = {k: sorted(v) for k, v in assets_union.items()}
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
            "runtime_assets": assets.get(name, []),
            "shipped_assets": sorted(shipped_union.get(name, set())),
        })

    manifest = {
        "manifest_version": MANIFEST_VERSION,
        "projects": projects_rel,
        "build_config": build_config,
        "output_dirs": output_dirs,
        "exceptions": exceptions,
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


SELF_BUILT = {
    "dbsrv.exe", "dbsrv.dll", "dbsrv.storage.dll", "dbsrv.storage.mysql.dll",
    "dbsrv.storage.mongodb.dll", "openmir2.dll", "loginsrv.exe", "loginsrv.dll",
}


def cmd_check(args):
    reds = []
    proj_paths = split_projects(args)
    manifest_path = os.path.abspath(args.manifest)
    manifest = None
    if not os.path.isfile(manifest_path):
        _fail(reds, "MISSING_MANIFEST", manifest_path)
    else:
        try:
            with open(manifest_path, "r", encoding="utf-8") as f:
                manifest = json.load(f)
        except (OSError, ValueError) as exc:
            _fail(reds, "BAD_MANIFEST", "%s (%s)" % (manifest_path, exc))
    if isinstance(manifest, dict):
        if manifest.get("manifest_version") != MANIFEST_VERSION:
            _fail(reds, "BAD_MANIFEST_VERSION", "manifest_version=%r" % manifest.get("manifest_version"))
        for field in ("projects", "build_config", "output_dirs", "exceptions"):
            if field not in manifest:
                _fail(reds, "MISSING_FIELD", "<top>." + field)
        excs = manifest.get("exceptions")
        if not isinstance(excs, list):
            _fail(reds, "BAD_EXCEPTIONS", "exceptions 必须是数组")
            excs = []
        else:
            for e in excs:
                if (not isinstance(e, dict) or not e.get("pkg") or not e.get("version")
                        or not (e.get("reason") or "").strip()):
                    _fail(reds, "BAD_EXCEPTIONS", "条目缺 pkg/version/reason")
        manifest["_exceptions_valid"] = isinstance(excs, list)
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
        if proj_paths:
            have = {os.path.abspath(x) for x in (manifest.get("projects") or [])}
            want = set(proj_paths)
            if have != want:
                _fail(reds, "PROJECT_SET_MISMATCH",
                      "manifest=%s args=%s" % (sorted(have), sorted(want)))
        cfg_manifest = manifest.get("build_config")
        cli_cfg = getattr(args, "build_config", None)
        if cli_cfg is not None and cli_cfg not in BUILD_CONFIGS:
            _fail(reds, "BAD_BUILD_CONFIG",
                  "--build-config=%r（只接受 %s）" % (cli_cfg, "/".join(BUILD_CONFIGS)))
        elif cfg_manifest not in BUILD_CONFIGS:
            # fail-closed：manifest 没写清口径（缺失/空/"unknown"）就不猜。
            _fail(reds, "BAD_BUILD_CONFIG",
                  "manifest build_config=%r（必须是 %s；含糊一律红）"
                  % (cfg_manifest, "/".join(BUILD_CONFIGS)))
        elif cli_cfg is not None and cli_cfg != cfg_manifest:
            _fail(reds, "BUILD_CONFIG_CONFLICT",
                  "manifest=%s args=%s" % (cfg_manifest, cli_cfg))
        else:
            use_cfg = cli_cfg or cfg_manifest
            for proj, recorded in (manifest.get("output_dirs") or {}).items():
                resolved = resolve_output_dir(os.path.dirname(proj), use_cfg)
                if resolved is None:
                    _fail(reds, "MISSING_OUTPUT_DIR",
                          "%s bin/%s（按 %s 口径解析不到输出目录）" % (proj, use_cfg, use_cfg))
                    continue
                if recorded and os.path.abspath(resolved) != os.path.abspath(recorded):
                    _fail(reds, "BUILD_OUTPUT_MISMATCH",
                          "%s manifest=%s now=%s（口径 %s）"
                          % (proj, recorded, resolved, use_cfg))
    elif manifest is not None:
        _fail(reds, "BAD_MANIFEST", "顶层不是对象")
        manifest = None

    # 一级：每个项目的 csproj 声明 vs 其 lock 的 resolved
    level1_projects = proj_paths or [os.path.abspath(x) for x in
                                     ((manifest or {}).get("projects") or [])]
    for proj in level1_projects:
        proj_dir = os.path.dirname(proj)
        lock, lock_path = load_lock(proj_dir)
        if lock is None:
            _fail(reds, "MISSING_LOCK", lock_path)
            continue
        pkgs_lock, err = lock_packages(lock)
        if pkgs_lock is None:
            _fail(reds, "BAD_LOCK", "%s %s" % (proj, err))
            continue
        refs = parse_csproj_refs(proj)
        direct = {n for n, m in pkgs_lock.items() if m.get("type") == "Direct"}
        for name in sorted(set(refs) - set(pkgs_lock)):
            _fail(reds, "NOT_IN_LOCK", "%s %s（csproj 声明但 lock 缺失）" % (proj, name))
        for name in sorted(direct - set(refs)):
            _fail(reds, "LOCK_DIRECT_NOT_DECLARED", "%s %s（lock Direct 但 csproj 未声明）" % (proj, name))
        for name, rng in sorted(refs.items()):
            meta = pkgs_lock.get(name)
            if meta is None:
                continue
            ok, why = range_satisfied(meta["resolved"], rng)
            if not ok:
                _fail(reds, "RANGE_UNSATISFIED", "%s %s range=%s resolved=%s（%s）"
                      % (proj, name, rng, meta["resolved"], why))

    # 二级：manifest vs 已还原包
    packages_dir = os.path.expanduser(args.packages_dir)
    if manifest is not None:
        for p in manifest.get("packages") or []:
            if not isinstance(p, dict) or "name" not in p or "resolved" not in p:
                continue
            name, version = p["name"], p["resolved"]
            if not all(field in p for field in REQUIRED_PKG_FIELDS):
                continue
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

    # 三级（可选）：部署目录映射
    deploy_dir = getattr(args, "deploy_dir", None)
    if deploy_dir:
        ddir = os.path.abspath(deploy_dir)
        deployed = []
        if not os.path.isdir(ddir):
            _fail(reds, "MISSING_DEPLOY_DIR", ddir)
        else:
            try:
                deployed = [fn for fn in os.listdir(ddir)
                            if os.path.isfile(os.path.join(ddir, fn))]
            except OSError as exc:
                _fail(reds, "BAD_DEPLOY_DIR", "%s (%s)" % (ddir, exc))
            if not deployed:
                _fail(reds, "EMPTY_DEPLOY_DIR", ddir)
        deploy_red_start = len(reds)
        if manifest is not None:
            claims = {}
            ship_total = 0
            for p in manifest.get("packages") or []:
                if not isinstance(p, dict) or not all(f in p for f in REQUIRED_PKG_FIELDS):
                    continue
                for rel in p.get("shipped_assets") or []:
                    base = rel.rsplit("/", 1)[-1]
                    claims.setdefault(base, []).append(
                        (p["name"], p["resolved"], rel, p["files"].get(rel)))
            mapped = set(claims)
            for base in sorted(claims):
                entries = claims[base]
                shas = {e[3] for e in entries}
                owners = {(e[0], e[1]) for e in entries}
                ship_total += 1
                if len(owners) > 1 and len(shas) > 1:
                    _fail(reds, "AMBIGUOUS_MAPPING", "%s candidates=%s" % (
                        base, ",".join("%s@%s" % kv for kv in sorted(owners))))
                    continue
                expected = sorted(sh for sh in shas if sh)[0] if any(shas) else None
                if expected is None:
                    _fail(reds, "MISSING_ASSET_HASH", base)
                    continue
                target = os.path.join(ddir, base)
                if not os.path.isfile(target):
                    _fail(reds, "MISSING_DEPLOYED",
                          "%s/%s %s" % (entries[0][0], entries[0][1], base))
                    continue
                actual = sha256_file(target)
                if actual != expected:
                    _fail(reds, "DEPLOYED_SHA_MISMATCH",
                          "%s/%s %s expected=%s actual=%s"
                          % (entries[0][0], entries[0][1], base, expected, actual))
            if ship_total == 0:
                _fail(reds, "MISSING_SHIP_SET",
                      "manifest 无 shipped_assets（生成时缺构建输出目录？）")
            for fn in sorted(deployed):
                if not fn.lower().endswith(".dll"):
                    continue
                if fn in mapped or fn.lower() in SELF_BUILT:
                    continue
                _fail(reds, "UNCOVERED_DEPLOYED", fn)
            # 显式例外只压制 MISSING_DEPLOYED（逐条带理由）；其余红原样保留。
            exceptions = [e for e in (manifest.get("exceptions") or [])
                          if isinstance(e, dict) and e.get("pkg") and e.get("version")
                          and (e.get("reason") or "").strip()]
            used = set()
            kept = []
            for idx, line in enumerate(reds):
                if idx < deploy_red_start or not line.startswith("DEPS_RED MISSING_DEPLOYED "):
                    kept.append(line)
                    continue
                detail = line[len("DEPS_RED MISSING_DEPLOYED "):]
                key = detail.split(" ", 1)[0]
                pkg, _, ver = key.rpartition("/")
                match = next((e for e in exceptions
                              if e["pkg"] == pkg and e["version"] == ver), None)
                if match is None:
                    kept.append(line)
                else:
                    used.add((match["pkg"], match["version"]))
            reds[:] = kept
            for e in exceptions:
                line = "DEPS_EXCEPTION %s/%s %s" % (e["pkg"], e["version"], e["reason"])
                if (e["pkg"], e["version"]) in used:
                    emit(line)
                else:
                    emit("DEPS_EXCEPTION_UNUSED %s/%s %s" % (e["pkg"], e["version"], e["reason"]))

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
            # 部署映射用的运行时资产 + 冲突样本（两包各带一个 Shared.dll，内容不同）
            for name, payload in (("Alpha", b"shared-alpha"), ("Beta", b"shared-beta")):
                with open(os.path.join(feed, name.lower(), "1.0.0" if name == "Alpha" else "2.0.0",
                                       "lib", "net8.0", "Shared.dll"), "wb") as f:
                    f.write(payload)
            os.makedirs(os.path.join(proj, "obj"), exist_ok=True)
            assets = {"targets": {"net8.0": {
                "Alpha/1.0.0": {"runtime": {"lib/net8.0/Alpha.dll": {}}},
                "Beta/2.0.0": {"runtime": {"lib/net8.0/Beta.dll": {}}},
            }}}
            with open(os.path.join(proj, "obj", "project.assets.json"), "w", encoding="utf-8") as f:
                json.dump(assets, f)
            out = os.path.join(proj, "bin", "Debug", "net8.0")
            os.makedirs(out, exist_ok=True)
            shutil.copy(os.path.join(feed, "alpha", "1.0.0", "lib", "net8.0", "Alpha.dll"),
                        os.path.join(out, "Alpha.dll"))
            shutil.copy(os.path.join(feed, "beta", "2.0.0", "lib", "net8.0", "Beta.dll"),
                        os.path.join(out, "Beta.dll"))
            return proj, feed

        def run(argv):
            ns = argparse.Namespace(**argv)
            if ns.cmd == "generate":
                return cmd_generate(ns)
            return cmd_check(ns)

        def run_out(argv):
            """跑一次并捕获 stdout：牙必须钉**诊断码**（否则别的红也能让 rc=1 冒充）。"""
            buf = io.StringIO()
            old_stdout = sys.stdout
            sys.stdout = buf
            try:
                rc = run(argv)
            finally:
                sys.stdout = old_stdout
            return rc, buf.getvalue()

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

        # Case 8：--deploy-dir 正例（部署件与 manifest 一致 ⇒ 绿；多余 DLL 只 WARN_EXTRA）
        proj4, feed4 = make_fixture("deploy")
        man4 = os.path.join(tmp, "deploy", "m.json")
        run({"cmd": "generate", "project": os.path.join(proj4, "App.csproj"), "out": man4,
             "packages_dir": feed4})
        ddir = os.path.join(tmp, "deploy", "out")
        os.makedirs(ddir, exist_ok=True)
        shutil.copy(os.path.join(feed4, "alpha", "1.0.0", "lib", "net8.0", "Alpha.dll"),
                    os.path.join(ddir, "Alpha.dll"))
        shutil.copy(os.path.join(feed4, "beta", "2.0.0", "lib", "net8.0", "Beta.dll"),
                    os.path.join(ddir, "Beta.dll"))
        with open(os.path.join(ddir, "DBSrv.dll"), "wb") as f:
            f.write(b"ours")
        rc = run({"cmd": "check", "project": os.path.join(proj4, "App.csproj"),
                  "manifest": man4, "packages_dir": feed4, "deploy_dir": ddir})
        sassert(rc == 0, "deploydir: matching deploy dir green (self-built extra ignored)", "rc=%d" % rc)

        # 牙①：翻部署件末字节 ⇒ DEPLOYED_SHA_MISMATCH
        with open(os.path.join(ddir, "Alpha.dll"), "r+b") as f:
            f.seek(-1, os.SEEK_END)
            last = f.read(1)
            f.seek(-1, os.SEEK_END)
            f.write(bytes([(last[0] + 1) % 256]))
        rc = run({"cmd": "check", "project": os.path.join(proj4, "App.csproj"),
                  "manifest": man4, "packages_dir": feed4, "deploy_dir": ddir})
        sassert(rc == 1, "tooth1: deployed byte flip red", "rc=%d" % rc)

        # 牙②：删一个部署件 ⇒ MISSING_DEPLOYED
        os.remove(os.path.join(ddir, "Beta.dll"))
        rc = run({"cmd": "check", "project": os.path.join(proj4, "App.csproj"),
                  "manifest": man4, "packages_dir": feed4, "deploy_dir": ddir})
        sassert(rc == 1, "tooth2: missing deployed file red", "rc=%d" % rc)

        # 牙③：两包映射到同名 DLL 且 sha 不同 ⇒ AMBIGUOUS_MAPPING
        proj5, feed5 = make_fixture("ambiguous")
        man5 = os.path.join(tmp, "ambiguous", "m.json")
        run({"cmd": "generate", "project": os.path.join(proj5, "App.csproj"), "out": man5,
             "packages_dir": feed5})
        data5 = json.load(open(man5, encoding="utf-8"))
        for pkg in data5["packages"]:
            pkg["runtime_assets"] = ["lib/net8.0/Shared.dll"]
            pkg["shipped_assets"] = ["lib/net8.0/Shared.dll"]
        json.dump(data5, open(man5, "w", encoding="utf-8"))
        adir = os.path.join(tmp, "ambiguous", "out")
        os.makedirs(adir, exist_ok=True)
        shutil.copy(os.path.join(feed5, "alpha", "1.0.0", "lib", "net8.0", "Shared.dll"),
                    os.path.join(adir, "Shared.dll"))
        rc = run({"cmd": "check", "project": os.path.join(proj5, "App.csproj"),
                  "manifest": man5, "packages_dir": feed5, "deploy_dir": adir})
        sassert(rc == 1, "tooth3: ambiguous mapping red", "rc=%d" % rc)

        # 牙④：部署目录里出现不在 manifest、也不在自家件白名单的第三方 DLL ⇒ UNCOVERED_DEPLOYED
        ddir2 = os.path.join(tmp, "deploy", "out2")
        os.makedirs(ddir2, exist_ok=True)
        shutil.copy(os.path.join(feed4, "alpha", "1.0.0", "lib", "net8.0", "Alpha.dll"),
                    os.path.join(ddir2, "Alpha.dll"))
        shutil.copy(os.path.join(feed4, "beta", "2.0.0", "lib", "net8.0", "Beta.dll"),
                    os.path.join(ddir2, "Beta.dll"))
        with open(os.path.join(ddir2, "Stray.Third.Party.dll"), "wb") as f:
            f.write(b"stray")
        rc = run({"cmd": "check", "project": os.path.join(proj4, "App.csproj"),
                  "manifest": man4, "packages_dir": feed4, "deploy_dir": ddir2})
        sassert(rc == 1, "tooth4: uncovered third-party dll red", "rc=%d" % rc)

        # 记录的输出目录与当前探测不一致 ⇒ BUILD_OUTPUT_MISMATCH
        bad_out = os.path.join(tmp, "deploy", "badout.json")
        data6 = json.load(open(man4, encoding="utf-8"))
        for k in list(data6.get("output_dirs") or {}):
            data6["output_dirs"][k] = os.path.join(tmp, "deploy", "elsewhere")
        json.dump(data6, open(bad_out, "w", encoding="utf-8"))
        rc = run({"cmd": "check", "project": os.path.join(proj4, "App.csproj"),
                  "manifest": bad_out, "packages_dir": feed4})
        sassert(rc == 1, "output-dir mismatch red", "rc=%d" % rc)

        # 卡 deps-build-config-pinning 牙①：--build-config 与 manifest 的口径冲突 ⇒ 红（不"迁就"参数）
        rc, out = run_out({"cmd": "check", "project": os.path.join(proj4, "App.csproj"),
                           "manifest": man4, "packages_dir": feed4, "build_config": "Release"})
        sassert(rc == 1 and "BUILD_CONFIG_CONFLICT" in out,
                "buildcfg: cli config conflict red", "rc=%d out=%r" % (rc, out[:160]))

        # 牙②（反 mtime 探测）：Debug 口径 manifest + 另建一个**更晚**的 Release 输出目录 ⇒ 仍按 Debug 判、绿。
        # 旧实现按 mtime 挑"最新" ⇒ 这里会 BUILD_OUTPUT_MISMATCH；删掉 mtime 探测后必须不再受影响。
        proj7, feed7 = make_fixture("mtime")
        man7 = os.path.join(tmp, "mtime", "m.json")
        run({"cmd": "generate", "project": os.path.join(proj7, "App.csproj"), "out": man7,
             "packages_dir": feed7})
        data7 = json.load(open(man7, encoding="utf-8"))
        rel_out = os.path.join(proj7, "bin", "Release", "net8.0")
        os.makedirs(rel_out, exist_ok=True)
        shutil.copy(os.path.join(feed7, "alpha", "1.0.0", "lib", "net8.0", "Alpha.dll"),
                    os.path.join(rel_out, "Alpha.dll"))
        dbg_dir = os.path.join(proj7, "bin", "Debug")
        rel_base = os.path.join(proj7, "bin", "Release")
        tick = os.path.getmtime(dbg_dir) + 3600
        os.utime(rel_out, (tick, tick))
        os.utime(rel_base, (tick, tick))
        newer = os.path.getmtime(rel_base) > os.path.getmtime(dbg_dir)
        rc, out = run_out({"cmd": "check", "project": os.path.join(proj7, "App.csproj"),
                           "manifest": man7, "packages_dir": feed7})
        sassert(newer and data7.get("build_config") == "Debug" and rc == 0
                and "BUILD_OUTPUT_MISMATCH" not in out,
                "buildcfg: no mtime probing (newer Release must not flip)",
                "newer=%s cfg=%r rc=%d out=%r"
                % (newer, data7.get("build_config"), rc, out[:160]))

        # 牙③（fail-closed）：manifest 的 build_config 缺失/空/非法 ⇒ 红（不猜默认）
        for idx, bad_cfg in enumerate(("", "unknown", None)):
            bad = os.path.join(tmp, "mtime", "bad_cfg_%d.json" % idx)
            d = json.load(open(man7, encoding="utf-8"))
            d["build_config"] = bad_cfg
            json.dump(d, open(bad, "w", encoding="utf-8"))
            rc, out = run_out({"cmd": "check", "project": os.path.join(proj7, "App.csproj"),
                               "manifest": bad, "packages_dir": feed7})
            sassert(rc == 1 and "BAD_BUILD_CONFIG" in out,
                    "buildcfg: bad/absent build_config red (%r)" % (bad_cfg,),
                    "rc=%d out=%r" % (rc, out[:160]))

        # 牙④：口径是 Release 但该目录不存在 ⇒ MISSING_OUTPUT_DIR（绝不静默回落 Debug）
        bad2 = os.path.join(tmp, "deploy", "release_only.json")
        d2 = json.load(open(man4, encoding="utf-8"))
        d2["build_config"] = "Release"
        json.dump(d2, open(bad2, "w", encoding="utf-8"))
        rc, out = run_out({"cmd": "check", "project": os.path.join(proj4, "App.csproj"),
                           "manifest": bad2, "packages_dir": feed4})
        sassert(rc == 1 and "MISSING_OUTPUT_DIR" in out,
                "buildcfg: missing configured output dir red",
                "rc=%d out=%r" % (rc, out[:160]))

        # 牙⑤（生成侧）：两个目录都在时，--build-config Debug 仍按 Debug 写 manifest
        proj8, feed8 = make_fixture("both")
        os.makedirs(os.path.join(proj8, "bin", "Release", "net8.0"), exist_ok=True)
        shutil.copy(os.path.join(feed8, "alpha", "1.0.0", "lib", "net8.0", "Alpha.dll"),
                    os.path.join(proj8, "bin", "Release", "net8.0", "Alpha.dll"))
        man8 = os.path.join(tmp, "both", "m.json")
        run({"cmd": "generate", "project": os.path.join(proj8, "App.csproj"), "out": man8,
             "packages_dir": feed8, "build_config": "Debug"})
        d8 = json.load(open(man8, encoding="utf-8"))
        rc = run({"cmd": "check", "project": os.path.join(proj8, "App.csproj"),
                  "manifest": man8, "packages_dir": feed8, "build_config": "Debug"})
        sassert(d8.get("build_config") == "Debug" and rc == 0,
                "buildcfg: generate honors explicit Debug",
                "cfg=%r rc=%d" % (d8.get("build_config"), rc))

        # 显式例外：被例外覆盖的缺失件 ⇒ 绿 + DEPS_EXCEPTION（例外必须逐条有理由）
        exc = os.path.join(tmp, "deploy", "exc.json")
        with open(exc, "w", encoding="utf-8") as f:
            json.dump([{"pkg": "Beta", "version": "2.0.0",
                        "reason": "selftest: declared but zero API usage"}], f)
        manE = os.path.join(tmp, "deploy", "mE.json")
        rc = run({"cmd": "generate", "project": os.path.join(proj4, "App.csproj"), "out": manE,
                  "packages_dir": feed4, "exceptions": exc})
        sassert(rc == 0, "exception: generate with exceptions rc=0", "rc=%d" % rc)
        ddirE = os.path.join(tmp, "deploy", "outE")
        os.makedirs(ddirE, exist_ok=True)
        shutil.copy(os.path.join(feed4, "alpha", "1.0.0", "lib", "net8.0", "Alpha.dll"),
                    os.path.join(ddirE, "Alpha.dll"))
        rc = run({"cmd": "check", "project": os.path.join(proj4, "App.csproj"),
                  "manifest": manE, "packages_dir": feed4, "deploy_dir": ddirE})
        sassert(rc == 0, "exception: missing-but-excepted green", "rc=%d" % rc)

        # 例外理由为空 ⇒ 生成阶段红（禁止空口豁免）
        bad_exc = os.path.join(tmp, "deploy", "bad_exc.json")
        with open(bad_exc, "w", encoding="utf-8") as f:
            json.dump([{"pkg": "Beta", "version": "2.0.0", "reason": "  "}], f)
        rc = run({"cmd": "generate", "project": os.path.join(proj4, "App.csproj"),
                  "out": os.path.join(tmp, "deploy", "mBad.json"),
                  "packages_dir": feed4, "exceptions": bad_exc})
        sassert(rc == 1, "exception: empty reason red", "rc=%d" % rc)

        # fail-closed：部署目录缺失 / 为空
        rc = run({"cmd": "check", "project": os.path.join(proj4, "App.csproj"),
                  "manifest": man4, "packages_dir": feed4,
                  "deploy_dir": os.path.join(tmp, "deploy", "nope")})
        sassert(rc == 1, "deploydir: missing dir red", "rc=%d" % rc)
        empty_dir = os.path.join(tmp, "deploy", "empty")
        os.makedirs(empty_dir, exist_ok=True)
        rc = run({"cmd": "check", "project": os.path.join(proj4, "App.csproj"),
                  "manifest": man4, "packages_dir": feed4, "deploy_dir": empty_dir})
        sassert(rc == 1, "deploydir: empty dir red", "rc=%d" % rc)

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
    g.add_argument("--project", required=False)
    g.add_argument("--projects", default=None, help="多个项目用 ; 或 , 分隔（并集 manifest）")
    g.add_argument("--out", required=True)
    g.add_argument("--packages-dir", default=os.path.join("~", ".nuget", "packages"))
    g.add_argument("--output-dir", default=None)
    g.add_argument("--exceptions", default=None)
    g.add_argument("--build-config", choices=list(BUILD_CONFIGS), default=None,
                   help="输出目录口径（缺省按 Release → Debug 取**存在者**，只看存在性、不看 mtime）")
    c = sub.add_parser("check")
    c.add_argument("--project", required=False)
    c.add_argument("--projects", default=None)
    c.add_argument("--manifest", required=True)
    c.add_argument("--packages-dir", default=os.path.join("~", ".nuget", "packages"))
    c.add_argument("--deploy-dir", default=None)
    c.add_argument("--build-config", choices=list(BUILD_CONFIGS), default=None,
                   help="显式口径；缺省用 manifest 的 build_config（含糊即红，绝不按 mtime 猜）")
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
