#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""entryrefs — 从 collab 条目里取 `refs.head` / `refs.base`：**取不到就报错**，绝不返回空串。

卡：`openmir2-refs-head-guard`（L）。背景是「恒真谓词」的原型：

    body.head 取不到 ⇒ 空串 ⇒ "anything".startswith("") 恒真 ⇒ 守卫恒通过、且看不出异常

thread 视图（`walgit collab thread <id>`）里每条是::

    {"entry": {"actor", "body", "id", "kind", "parent", "sig", "ts", "version",
               "refs": {"base": "<40-hex>", "head": "<40-hex>"}},   # patch 才有 refs
     "oid": "<blob>", "principal": "...", "verified": true}

**patch 的 base/head 在 `entry.refs`，不在 `entry.body`。** 本模块把「读对字段 + 校验形状」
收敛到一处：缺失 / 空串 / 非 40 位小写 hex 一律 `EntryRefError`（调用方必须当错误处理）。

用法：
    from entryrefs import require_head, require_base, head_matches
    head = require_head(item)                  # 取不到 ⇒ 抛 EntryRefError
    if not head_matches(item, current_head):   # 相等比较；current_head 也必须是 40-hex
        ...

自检：`python -I -B tools/collab/entryrefs.py --selftest`
"""
import argparse
import json
import os
import re
import sys

OID_RE = re.compile(r"^[0-9a-f]{40}$")
REF_KEYS = ("base", "head")


class EntryRefError(Exception):
    """取不到合法的 `refs.<key>`（读错字段/缺字段/空串）——**判不了**，不许静默放行。"""


def entry_of(item):
    """thread 视图的一条 → `entry` dict。兼容「已拆包的 entry 本身」。形状不对 ⇒ 报错。"""
    if isinstance(item, dict):
        inner = item.get("entry")
        if isinstance(inner, dict):
            return inner
        if item.get("kind"):
            return item
    raise EntryRefError("条目形状不对（既不是 {entry: ...} 包装，也不是 entry 本身）：%r"
                        % (str(item)[:120],))


def require_oid(item, key, what=None):
    """取 `refs.<key>` 并校验是 40 位小写 hex；任何异常形状都抛 `EntryRefError`。"""
    if key not in REF_KEYS:
        raise EntryRefError("只支持 refs.base / refs.head（收到 %r）" % (key,))
    entry = entry_of(item)
    refs = entry.get("refs")
    if not isinstance(refs, dict):
        raise EntryRefError(
            "缺 refs.%s：条目没有 refs 对象（thread 视图里 patch 的 base/head 在 entry.refs，"
            "不在 body；用 body.%s 会取到空串 ⇒ startswith('') 恒真）" % (key, key))
    val = refs.get(key)
    if val is None:
        raise EntryRefError("缺 refs.%s：refs 里没有 %r 键（现有键：%s）"
                            % (key, key, sorted(refs)))
    if not isinstance(val, str):
        raise EntryRefError("refs.%s 类型不对（%s）：必须是 40 位小写 hex 字符串"
                            % (key, type(val).__name__))
    if val == "":
        raise EntryRefError("refs.%s 是空串：空串会让任何 startswith('') 恒真 ⇒ 必须显式报错"
                            % key)
    if not OID_RE.match(val):
        raise EntryRefError("refs.%s 不是 40 位小写 hex：%r" % (key, val[:48]))
    return val


def require_head(item, what=None):
    return require_oid(item, "head", what)


def require_base(item, what=None):
    return require_oid(item, "base", what)


def head_matches(item, current_head, what=None):
    """`refs.head == current_head`——**相等**比较（不是前缀/startswith）。

    `current_head` 同样必须是 40 位小写 hex：空串 / None 一律报错，
    否则 `"" == ""` 会把「两边都没取到」蒙成匹配。
    """
    head = require_head(item, what)
    if not isinstance(current_head, str) or not OID_RE.match(current_head):
        raise EntryRefError("current_head 不是 40 位小写 hex：%r"
                            % (str(current_head)[:48],))
    return head == current_head


def body_oid_keys(item):
    """返回 body 里出现过的 base/head 键（**反模式探测**，用于 --lint；不改任何东西）。"""
    entry = entry_of(item)
    body = entry.get("body")
    if not isinstance(body, dict):
        return []
    return [k for k in REF_KEYS if k in body]


# --------------------------------------------------------------------------- selftest
def selftest():
    ok = fail = 0

    def chk(cond, name):
        nonlocal ok, fail
        if cond:
            ok += 1
            print("PASS " + name)
        else:
            fail += 1
            print("FAIL " + name)

    good = "a" * 40
    other = "b" * 40

    def wrap(entry):
        return {"entry": entry, "oid": "f" * 40, "principal": "p", "verified": True}

    # 正例
    item = wrap({"kind": "patch", "refs": {"base": good, "head": other}})
    chk(require_head(item) == other and require_base(item) == good,
        "正例：refs.head/base 取得到")
    chk(head_matches(item, other) is True and head_matches(item, good) is False,
        "正例：head_matches 是相等比较（不同 head ⇒ False，不是前缀）")

    # 牙 ①：没有 refs.head（refs 缺失 / 键缺失 / None）——必须**点名缺 refs.head**
    for label, entry in (("refs 缺失", {"kind": "patch"}),
                         ("refs 里没 head", {"kind": "patch", "refs": {"base": good}}),
                         ("head 为 None", {"kind": "patch", "refs": {"base": good, "head": None}})):
        try:
            require_head(wrap(entry))
            got = "no-raise"
        except EntryRefError as ex:
            got = "ok" if "缺 refs.head" in str(ex) else "wrong:%s" % str(ex)[:60]
        chk(got == "ok", "牙① %s ⇒ EntryRefError 且点名「缺 refs.head」（%r）" % (label, got))

    # 牙 ②：refs.head 是空串——必须**点名空串**（与牙①的消息可区分）
    try:
        require_head(wrap({"kind": "patch", "refs": {"base": good, "head": ""}}))
        got2 = "no-raise"
    except EntryRefError as ex:
        msg = str(ex)
        got2 = "ok" if ("空串" in msg and "缺 refs.head" not in msg) else "wrong:%s" % msg[:60]
    chk(got2 == "ok", "牙② refs.head='' ⇒ EntryRefError 且点名「空串」（不冒充「缺」；%r）" % (got2,))

    # 牙 ③：非 40-hex / 大写 / 短 —— 一律报错
    for label, bad in (("短 oid", "abc"), ("大写", "A" * 40), ("非 hex", "z" * 40)):
        try:
            require_head(wrap({"kind": "patch", "refs": {"head": bad}}))
            got3 = "no-raise"
        except EntryRefError as ex:
            got3 = "ok" if "不是 40 位" in str(ex) else "wrong:%s" % str(ex)[:60]
        chk(got3 == "ok", "牙③ %s ⇒ EntryRefError（%r）" % (label, got3))

    # 牙 ④：反模式——body 里放 head（reviewer-21 的病）⇒ 读 refs 会报错，而 startswith('') 恒真
    body_only = wrap({"kind": "patch", "body": {"head": ""}})
    try:
        require_head(body_only)
        got4 = "no-raise"
    except EntryRefError as ex:
        got4 = "ok" if "缺 refs.head" in str(ex) else "wrong"
    chk(got4 == "ok", "牙④ body 里放 head ⇒ 读 refs 时报「缺 refs.head」（不返回空串）")
    chk("anything".startswith("") is True,
        "牙④b 反模式本身：startswith('') 恒真（这就是「取空串 ⇒ 守卫恒通过」）")

    # 牙 ⑤：current_head 退化值不许蒙过（'' / None / 短）——`'' == ''` 是最隐蔽的一支
    for label, cur in (("空串", ""), ("None", None), ("短", "dead")):
        try:
            head_matches(wrap({"kind": "patch", "refs": {"head": other}}), cur)
            got5 = "no-raise"
        except EntryRefError:
            got5 = "ok"
        chk(got5 == "ok", "牙⑤ current_head=%s ⇒ 报错（不让 '' == '' 蒙成匹配）" % label)

    # 牙 ⑥：形状不对 / 未知 key
    for label, item_bad in (("非 dict", "not-a-dict"), ("空 dict", {})):
        try:
            require_head(item_bad)
            got6 = "no-raise"
        except EntryRefError:
            got6 = "ok"
        chk(got6 == "ok", "牙⑥ %s ⇒ 报错（形状 fail-closed）" % label)
    try:
        require_oid(wrap({"kind": "patch", "refs": {"head": other}}), "tip")
        got6b = "no-raise"
    except EntryRefError:
        got6b = "ok"
    chk(got6b == "ok", "牙⑥b 只支持 base/head 两个 key ⇒ 其它 key 报错")

    # 牙 ⑦：body_oid_keys 反模式探测（--lint 用）
    chk(body_oid_keys(body_only) == ["head"] and body_oid_keys(item) == [],
        "牙⑦ body_oid_keys 能点出 body 里的 head/base（lint 用）")

    print("%s: entryrefs selftest %d/%d" % ("GREEN" if not fail else "RED", ok, ok + fail))
    return 0 if not fail else 1


def main(argv=None):
    ap = argparse.ArgumentParser(description="collab 条目 refs.head/refs.base 取值（fail-closed）")
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("--lint", default=None,
                    help="对 `walgit collab thread` 的 JSON 输出做反模式扫描（body 里出现 base/head）")
    args = ap.parse_args(argv)
    if args.selftest:
        return selftest()
    if args.lint:
        with open(args.lint, encoding="utf-8", errors="replace") as fh:
            data = json.load(fh)
        bad = 0
        for item in data:
            keys = body_oid_keys(item)
            if keys:
                bad += 1
                entry = entry_of(item)
                print("BODY_OID %s %r" % (entry.get("id"), keys))
        print("lint：%d 条里 %d 条在 body 里放了 base/head" % (len(data), bad))
        return 0
    ap.print_help()
    return 2


if __name__ == "__main__":
    sys.exit(main())
