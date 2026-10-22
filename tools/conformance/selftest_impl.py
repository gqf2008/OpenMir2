#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""selftest_impl.py — compare_runs.py 的离线自检（由 `compare_runs.py selftest` 调用）。

全离线：不连网、不连库、不起任何进程；只用**合成的两侧运行产物**（临时目录）验证：
  1. 全等 ⇒ 退出码 0；
  2. 改一个字节 ⇒ 1，且报告定位到 ident + 字节偏移；
  3. 规则没覆盖的 ident ⇒ 2（UNCLASSIFIED），显式 --allow-unclassified 才放行；
  4. 40/58 布局按语义字段比、同长度按字节比；
  5. 头部掩码、body 区间掩码、ignore（心跳）不会顺带吞掉真差异（「改坏必红」的牙）；
  6. 缺 body / 产物自相矛盾 / JSON 坏行 / 规则缺失 ⇒ 2（fail-closed）；
  7. 库快照逐列 diff（内联格式 + 跨库前缀归一）与「只有一侧有快照」⇒ 2。

自检自身的牙：每一项「改坏必红」都断言**具体退出码 + 具体定位字段/偏移**；如果比较器退化成
「永远 0」，这些断言会以 RED 收场。
"""
import hashlib
import json
import os
import shutil
import struct
import tempfile
import traceback


def _sha(b):
    return hashlib.sha256(b).hexdigest()


class Selftest(object):
    def __init__(self, api, here):
        self.api = api
        self.here = here
        self.root = tempfile.mkdtemp(prefix="conformance-selftest-")
        self.passed = []
        self.failed = []
        self.n = 0
        self.builtin_rules = self._load_builtin()

    # -- 基础设施 --
    def _load_builtin(self):
        path = self.api.DEFAULT_RULES
        if not os.path.exists(path):
            raise RuntimeError("缺少内置规则表：%s" % path)
        with open(path, "r", encoding="utf-8") as f:
            return json.load(f)

    def check(self, name, cond, detail=""):
        if cond:
            self.passed.append(name)
            print("PASS %s" % name)
        else:
            self.failed.append((name, detail))
            print("FAIL %s%s" % (name, ("  <- " + str(detail)[:400]) if detail else ""))

    def tmpdir(self, name):
        path = os.path.join(self.root, name)
        os.makedirs(path, exist_ok=True)
        return path

    def write_run(self, name, frames, run_meta=True, db=None, raw_lines=None):
        d = self.tmpdir(name)
        fpath = os.path.join(d, "frames.ndjson")
        with open(fpath, "w", encoding="utf-8", newline="\n") as f:
            if raw_lines is not None:
                for line in raw_lines:
                    f.write(line + "\n")
            else:
                for fr in frames:
                    f.write(json.dumps(fr, ensure_ascii=True, sort_keys=True) + "\n")
        if run_meta:
            with open(os.path.join(d, "run.json"), "w", encoding="utf-8", newline="\n") as f:
                json.dump({"argv": ["synth", name], "started_at": "2026-01-01T00:00:00",
                           "ended_at": "2026-01-01T00:00:10", "steps": []},
                          f, ensure_ascii=False, indent=2)
                f.write("\n")
        if db is not None:
            with open(os.path.join(d, "db_snapshot.json"), "w", encoding="utf-8",
                      newline="\n") as f:
                json.dump(db, f, ensure_ascii=False, indent=2, sort_keys=True)
                f.write("\n")
        return d

    def rules_file(self, name, data):
        path = os.path.join(self.root, name)
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            json.dump(data, f, ensure_ascii=False, indent=2, sort_keys=True)
            f.write("\n")
        return path

    def compare(self, oracle_dir, rust_dir, extra=()):
        self.n += 1
        out = os.path.join(self.root, "report-%02d.json" % self.n)
        argv = ["--oracle", oracle_dir, "--rust", rust_dir,
                "--out", out, "--quiet"] + list(extra)
        rc = self.api.main(argv)
        report = None
        if os.path.exists(out):
            with open(out, "r", encoding="utf-8") as f:
                report = json.load(f)
        return rc, report

    # -- 合成帧 --
    @staticmethod
    def frame(ident, body=b"", **heads):
        rec = {"ident": ident}
        rec.update(heads)
        if body is not None:
            rec["body_hex"] = body.hex()
            rec["body_len"] = len(body)
            rec["body_sha256"] = _sha(body)
        return rec

    def ability_body(self, layout_len, **overrides):
        lc = self.lc
        vals = {}
        for i, name in enumerate(lc.ABILITY_FIELDS):
            vals[name] = (i + 1) * 3
        vals.update(overrides)
        buf = bytearray(layout_len)
        for name in lc.ABILITY_FIELDS:
            fmt, off = lc.ABILITY_LAYOUTS[layout_len][name]
            struct.pack_into(fmt, buf, off, vals[name])
        return bytes(buf)

    def base_pair(self, conn_o="oracle-conn", conn_r="rust-conn"):
        """一段覆盖默认规则的消息流；两侧只有 recog/conn/t_ms/seq 与 52 布局不同。"""
        def side(conn, seq0, t0, recog, ability_len, body658=b"notice-text"):
            return [
                self.frame(50, b"", phase="login", seq=seq0 + 0, conn=conn, t_ms=t0 + 0,
                           recog=recog, param=10, tag=20, series=3),
                self.frame(51, b"\x01\x02", phase="login", seq=seq0 + 1, conn=conn,
                           t_ms=t0 + 1, recog=recog, param=100, tag=200, series=1),
                self.frame(52, self.ability_body(ability_len), phase="login", seq=seq0 + 2,
                           conn=conn, t_ms=t0 + 2, recog=777, param=0, tag=0, series=0),
                self.frame(46, b"", phase="day", seq=seq0 + 3, conn=conn, t_ms=t0 + 3,
                           recog=0, param=5, tag=2, series=0),
                self.frame(658, body658, phase="notice", seq=seq0 + 4, conn=conn,
                           t_ms=t0 + 4, recog=0, param=0, tag=0, series=0),
                self.frame(41, b"", phase="login", seq=seq0 + 5, conn=conn, t_ms=t0 + 5,
                           recog=recog, param=0, tag=0, series=0),
                self.frame(10, b"", phase="hb", seq=seq0 + 6, conn=conn, t_ms=t0 + 6,
                           recog=recog, param=0, tag=0, series=0),
            ]
        o = side(conn_o, 1, 1000, 123, 40)
        r = side(conn_r, 101, 5000, 456, 58)
        return o, r

    # -- 各类断言 --
    def run(self):
        self.lc = self.api.load_module("capture_login_compare", self.api.LOGIN_COMPARE)
        o, r = self.base_pair()
        self.t_baseline(o, r)
        self.t_byte_diff(o, r)
        self.t_unclassified(o, r)
        self.t_missing_rule(o, r)
        self.t_ability_semantics(o, r)
        self.t_head_mask(o, r)
        self.t_ignore(o, r)
        self.t_frame_count()
        self.t_missing_body(o)
        self.t_inconsistent(o)
        self.t_malformed(o)
        self.t_no_rules(o, r)
        self.t_body_ranges()
        self.t_rule_validation(o, r)
        self.t_db()
        return self.finish()

    def t_baseline(self, o, r):
        od = self.write_run("t01-o", o)
        rd = self.write_run("t01-r", r)
        rc, rep = self.compare(od, rd)
        self.check("baseline: 全等 => rc 0", rc == 0, "rc=%r" % rc)
        self.check("baseline: verdict PASS", rep is not None and rep.get("verdict") == "PASS",
                   rep and rep.get("verdict"))
        kinds = {w.get("kind") for w in (rep or {}).get("whitelisted", [])}
        self.check("baseline: 52 布局差走语义比较并登记 ability_layout",
                   "ability_layout" in kinds, sorted(kinds))
        self.check("baseline: recog/conn/t_ms/seq 掩码差异被显式登记",
                   "masked_head" in kinds, sorted(kinds))
        self.check("baseline: 10/41 心跳帧按规则剔除",
                   (rep or {}).get("frames", {}).get("ignored", {})
                   .get("oracle", {}).get("count", 0) >= 2,
                   (rep or {}).get("frames", {}).get("ignored"))

    def t_byte_diff(self, o, r):
        r2 = json.loads(json.dumps(r))
        # 定位 658 帧并改最后一个字节
        target = [fr for fr in r2 if fr["ident"] == 658][0]
        body = bytes.fromhex(target["body_hex"])
        assert len(body) > 0
        mutated = body[:-1] + bytes([body[-1] ^ 0xFF])
        target["body_hex"] = mutated.hex()
        target["body_sha256"] = _sha(mutated)
        target["body_len"] = len(mutated)
        od = self.write_run("t02-o", o)
        rd = self.write_run("t02-r", r2)
        rc, rep = self.compare(od, rd)
        self.check("改一个字节 => rc 1", rc == 1, "rc=%r" % rc)
        target_diff = None
        for d in (rep or {}).get("diffs", []):
            if d.get("source") == "frame" and d.get("ident") == 658:
                target_diff = d
                break
        self.check("改一个字节 => 报告定位到 ident=658 + 偏移",
                   target_diff is not None and target_diff.get("offset") == len(body) - 1,
                   target_diff)
        self.check("改一个字节 => 报告给出首处不同字节",
                   target_diff is not None
                   and target_diff.get("oracle_byte") == body[-1]
                   and target_diff.get("rust_byte") == mutated[-1],
                   target_diff)

    def t_unclassified(self, o, r):
        o2 = json.loads(json.dumps(o)) + [self.frame(9999, b"\xaa\xbb", phase="unknown")]
        r2 = json.loads(json.dumps(r)) + [self.frame(9999, b"\xaa\xbb", phase="unknown")]
        od = self.write_run("t03-o", o2)
        rd = self.write_run("t03-r", r2)
        rc, rep = self.compare(od, rd)
        self.check("未覆盖 ident => rc 2", rc == 2, "rc=%r" % rc)
        self.check("未覆盖 ident => 报告 UNCLASSIFIED 列出 9999",
                   (rep or {}).get("coverage", {}).get("unclassified_idents") == [9999],
                   (rep or {}).get("coverage"))
        rc2, rep2 = self.compare(od, rd, ["--allow-unclassified", "9999"])
        self.check("--allow-unclassified 9999 => rc 0 且仍登记放行",
                   rc2 == 0 and any(w.get("kind") == "allowed_unclassified"
                                    for w in (rep2 or {}).get("whitelisted", [])),
                   (rc2, (rep2 or {}).get("whitelisted")))

    def t_missing_rule(self, o, r):
        rules = json.loads(json.dumps(self.builtin_rules))
        rules["rules"] = [x for x in rules["rules"] if x.get("ident") != 658]
        rp = self.rules_file("t04-rules.json", rules)
        od = self.write_run("t04-o", o)
        rd = self.write_run("t04-r", r)
        rc, rep = self.compare(od, rd, ["--rules", rp])
        self.check("自定义规则缺 658 => rc 2 + UNCLASSIFIED",
                   rc == 2 and 658 in (rep or {}).get("coverage", {}).get("unclassified_idents", []),
                   (rc, (rep or {}).get("coverage")))

    def t_ability_semantics(self, o, r):
        r2 = json.loads(json.dumps(r))
        target = [fr for fr in r2 if fr["ident"] == 52][0]
        body = bytearray(self.ability_body(58, hp=9999))
        target["body_hex"] = bytes(body).hex()
        target["body_sha256"] = _sha(bytes(body))
        target["body_len"] = len(body)
        od = self.write_run("t05-o", o)
        rd = self.write_run("t05-r", r2)
        rc, rep = self.compare(od, rd)
        self.check("52 语义字段不同 => rc 1", rc == 1, "rc=%r" % rc)
        diff = None
        for d in (rep or {}).get("diffs", []):
            if d.get("field") == "52_semantic:hp":
                diff = d
                break
        self.check("52 语义差异定位到字段 hp（含两侧偏移）",
                   diff is not None and "oracle_offset" in diff and "rust_offset" in diff,
                   diff)

    def t_head_mask(self, o, r):
        r2 = json.loads(json.dumps(r))
        for fr in r2:
            if fr["ident"] == 46:
                fr["param"] = 8  # oracle 是 5；规则掩码 param/tag
        od = self.write_run("t06-o", o)
        rd = self.write_run("t06-r", r2)
        rc, rep = self.compare(od, rd)
        self.check("46 的 param 被掩码 => rc 0", rc == 0, "rc=%r" % rc)
        self.check("46 param 掩码差异显式登记",
                   any(w.get("ident") == 46 and w.get("field") == "param"
                       for w in (rep or {}).get("whitelisted", [])),
                   (rep or {}).get("whitelisted"))
        # 牙：掩码只掩头部，body 仍要逐字节比
        for fr in r2:
            if fr["ident"] == 46:
                fr["body_hex"] = "aa"
                fr["body_len"] = 1
                fr["body_sha256"] = _sha(b"\xaa")
        rd2 = self.write_run("t06-r2", r2)
        rc2, rep2 = self.compare(od, rd2)
        self.check("46 头部被掩码但 body 不同 => 仍然 rc 1",
                   rc2 == 1 and any(d.get("ident") == 46 and d.get("field") == "body_len"
                                    for d in (rep2 or {}).get("diffs", [])),
                   (rc2, (rep2 or {}).get("diffs")))

    def t_ignore(self, o, r):
        r2 = json.loads(json.dumps(r))
        r2.insert(3, self.frame(10, b"\x00", phase="hb-extra", seq=999, conn="rust-conn",
                                t_ms=42, recog=456))
        od = self.write_run("t07-o", o)
        rd = self.write_run("t07-r", r2)
        rc, rep = self.compare(od, rd)
        self.check("一侧多出心跳帧(10) => ignore 后 rc 0", rc == 0, "rc=%r" % rc)
        # 牙：多出的心跳不能顺带吞掉真差异
        for fr in r2:
            if fr["ident"] == 658:
                body = bytes.fromhex(fr["body_hex"])
                mutated = body[:-1] + bytes([body[-1] ^ 0x01])
                fr["body_hex"] = mutated.hex()
                fr["body_sha256"] = _sha(mutated)
                fr["body_len"] = len(mutated)
        rd2 = self.write_run("t07-r2", r2)
        rc2, rep2 = self.compare(od, rd2)
        self.check("心跳错位后 658 改字节 => 仍 rc 1 且偏移正确",
                   rc2 == 1 and any(d.get("ident") == 658 and d.get("offset") == len(body) - 1
                                    for d in (rep2 or {}).get("diffs", [])),
                   (rc2, (rep2 or {}).get("diffs")))

    def t_frame_count(self):
        o = [self.frame(50, b"", phase="login"), self.frame(658, b"x", phase="notice")]
        r = [self.frame(50, b"", phase="login")]
        od = self.write_run("t08-o", o)
        rd = self.write_run("t08-r", r)
        rc, rep = self.compare(od, rd)
        self.check("可比较帧数不同 => rc 1 + frame_count",
                   rc == 1 and any(d.get("kind") == "frame_count"
                                   for d in (rep or {}).get("diffs", [])),
                   (rc, (rep or {}).get("diffs")))

    def t_missing_body(self, o):
        o2 = [self.frame(50, b"", phase="login"),
              self.frame(658, None, phase="notice")]
        r2 = [self.frame(50, b"", phase="login"),
              self.frame(658, None, phase="notice")]
        od = self.write_run("t09-o", o2)
        rd = self.write_run("t09-r", r2)
        rc, rep = self.compare(od, rd)
        self.check("两侧都缺 body => rc 2 (body_unavailable)",
                   rc == 2 and any(u.get("kind") == "body_unavailable"
                                   for u in (rep or {}).get("undecidable", [])),
                   (rc, (rep or {}).get("undecidable")))

    def t_inconsistent(self, o):
        bad = self.frame(658, b"abc", phase="notice")
        bad["body_sha256"] = _sha(b"different")
        o2 = [self.frame(50, b"", phase="login"), bad]
        r2 = [self.frame(50, b"", phase="login"),
              self.frame(658, b"abc", phase="notice")]
        od = self.write_run("t10-o", o2)
        rd = self.write_run("t10-r", r2)
        rc, rep = self.compare(od, rd)
        self.check("body_sha256 与 body_hex 自相矛盾 => rc 2",
                   rc == 2 and any(u.get("kind") == "input_inconsistent"
                                   for u in (rep or {}).get("undecidable", [])),
                   (rc, (rep or {}).get("undecidable")))

    def t_malformed(self, o):
        od = self.write_run("t11-o", None, raw_lines=[
            json.dumps(self.frame(50, b"", phase="login")), "{not-json"])
        rd = self.write_run("t11-r", o)
        rc, rep = self.compare(od, rd)
        self.check("frames.ndjson 坏行 => rc 2", rc == 2,
                   (rc, (rep or {}).get("undecidable")))

    def t_no_rules(self, o, r):
        od = self.write_run("t12-o", o)
        rd = self.write_run("t12-r", r)
        missing = os.path.join(self.root, "no-such-rules.json")
        rc, rep = self.compare(od, rd, ["--rules", missing])
        self.check("规则表缺失 => rc 2（不静默当绿）", rc == 2,
                   (rc, (rep or {}).get("undecidable")))

    def t_body_ranges(self):
        rules = json.loads(json.dumps(self.builtin_rules))
        rules["rules"] = [x for x in rules["rules"] if x.get("ident") != 46]
        rules["rules"].append({"ident": 46, "action": "mask_body_ranges",
                               "ranges": [[0, 2]], "reason": "selftest: 前 2 字节按墙钟抖动",
                               "tier": "L", "approved_by": "selftest"})
        rp = self.rules_file("t13-rules.json", rules)
        o = [self.frame(46, b"\x01\x02\x03\x04", phase="day")]
        r = [self.frame(46, b"\x09\x09\x03\x04", phase="day")]
        od = self.write_run("t13-o", o)
        rd = self.write_run("t13-r", r)
        rc, rep = self.compare(od, rd, ["--rules", rp])
        self.check("body 区间掩码内不同 => rc 0 且显式登记",
                   rc == 0 and any(w.get("kind") == "mask_body_ranges"
                                   for w in (rep or {}).get("whitelisted", [])),
                   (rc, (rep or {}).get("whitelisted")))
        r2 = [self.frame(46, b"\x01\x02\x03\x05", phase="day")]
        rd2 = self.write_run("t13-r2", r2)
        rc2, rep2 = self.compare(od, rd2, ["--rules", rp])
        self.check("区间掩码外不同 => rc 1 且 offset=3",
                   rc2 == 1 and any(d.get("ident") == 46 and d.get("offset") == 3
                                    for d in (rep2 or {}).get("diffs", [])),
                   (rc2, (rep2 or {}).get("diffs")))

    def t_rule_validation(self, o, r):
        od = self.write_run("t14-o", o)
        rd = self.write_run("t14-r", r)
        dup = json.loads(json.dumps(self.builtin_rules))
        dup["rules"].append({"ident": 46, "action": "compare", "reason": "dup",
                             "tier": "L", "approved_by": "selftest"})
        rp = self.rules_file("t14-dup.json", dup)
        rc, rep = self.compare(od, rd, ["--rules", rp])
        self.check("规则表 ident 重复 => rc 2", rc == 2,
                   (rc, (rep or {}).get("undecidable")))
        notier = json.loads(json.dumps(self.builtin_rules))
        notier["rules"][0].pop("tier", None)
        rp2 = self.rules_file("t14-notier.json", notier)
        rc2, rep2 = self.compare(od, rd, ["--rules", rp2])
        self.check("规则缺 tier => rc 2", rc2 == 2,
                   (rc2, (rep2 or {}).get("undecidable")))

    def t_db(self):
        dp = self.api.load_module("dbsnap_pair", self.api.DBSNAP_PAIR)
        cols = list(dp.TABLE_SPECS["mir2_db.characters_ablity"]["columns"])

        def rows(level):
            row = ["0"] * len(cols)
            row[cols.index("PlayerId")] = "1"
            row[cols.index("Level")] = str(level)
            row[cols.index("ModifyTime")] = "2026-10-22 00:00:00"
            return [row]

        def snap(prefix, level):
            return {"format": "dbsnap-inline/1",
                    "tables": [{"table": prefix + ".characters_ablity",
                                "columns": cols, "rows": rows(level)}]}

        frames = [self.frame(50, b"", phase="login"), self.frame(658, b"x", phase="notice")]
        od = self.write_run("t15-o", frames, db=snap("mir2_test_db_oracle", 7))
        rd = self.write_run("t15-r", frames, db=snap("mir2_test_db_rust", 7))
        rc, rep = self.compare(od, rd)
        db = (rep or {}).get("db") or {}
        self.check("库快照全等（跨库前缀归一）=> rc 0 + state=compared",
                   rc == 0 and db.get("state") == "compared", (rc, db.get("state"), db.get("detail")))
        self.check("库快照比较表集非空",
                   (db.get("summary") or {}).get("tables", {}).get("mir2_db.characters_ablity") == 0,
                   (db.get("summary") or {}).get("tables"))

        rd2 = self.write_run("t15-r2", frames, db=snap("mir2_test_db_rust", 8))
        rc2, rep2 = self.compare(od, rd2)
        self.check("库字段 Level 7->8 => rc 1 + value 差异",
                   rc2 == 1 and any(d.get("source") == "db" and d.get("kind") == "value"
                                    for d in (rep2 or {}).get("diffs", [])),
                   (rc2, (rep2 or {}).get("diffs")))

        rd3 = self.write_run("t15-r3", frames)  # 无 db_snapshot
        rc3, rep3 = self.compare(od, rd3)
        self.check("只有一侧有库快照 => rc 2",
                   rc3 == 2 and ((rep3 or {}).get("db") or {}).get("state") == "one_sided",
                   (rc3, (rep3 or {}).get("db")))

        od4 = self.write_run("t15-o4", frames)
        rd4 = self.write_run("t15-r4", frames)
        rc4, rep4 = self.compare(od4, rd4)
        self.check("两侧都无库快照 => rc 0 + state=absent_both（显式登记未比较）",
                   rc4 == 0 and ((rep4 or {}).get("db") or {}).get("state") == "absent_both",
                   (rc4, (rep4 or {}).get("db")))

    def finish(self):
        total = len(self.passed) + len(self.failed)
        shutil.rmtree(self.root, ignore_errors=True)
        if self.failed:
            print("RED: compare_runs selftest %d/%d" % (len(self.passed), total))
            for name, detail in self.failed:
                print("  FAILED: %s <- %r" % (name, detail))
            return 1
        print("GREEN: compare_runs selftest %d/%d" % (len(self.passed), total))
        return 0


def run(api, here):
    try:
        return Selftest(api, here).run()
    except Exception:  # noqa: BLE001 - 自检自身崩了必须是 INCOMPLETE，不能假绿
        traceback.print_exc()
        print("RED: compare_runs selftest 未跑完（INCOMPLETE）")
        return 2
