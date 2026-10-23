#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""selftest_impl.py — compare_runs.py 的离线自检（由 `compare_runs.py selftest` 调用）。

全离线：不连网、不连库、不起任何进程；只用**合成的两侧运行产物**（临时目录）验证：
  1. 全等 ⇒ 退出码 0；
  2. 改一个字节 ⇒ 1，且报告定位到 ident + 字节偏移；
  3. 规则没覆盖的 ident ⇒ 2（UNCLASSIFIED），显式 --allow-unclassified 才放行；
  4. 40/58 布局按语义字段比、同长度按字节比；驱动器线缆长度对 (54,78) 先 EDCode
     归一成 (40,58) 再比，未登记长度对 ⇒ 2（fail-closed）；
  5. 头部掩码、body 区间掩码、显式剔除（--ignore-idents）不会顺带吞掉真差异（「改坏必红」的牙）；
  6. 缺 body / 产物自相矛盾 / JSON 坏行 / 规则缺失 ⇒ 2（fail-closed）；
  7. 库快照逐列 diff（内联格式 + 跨库前缀归一）与「只有一侧有快照」⇒ 2；
  8. 两侧 0 帧（empty_run）/ 两侧都缺同一头部字段 / PENDING 的 ignore 真剔了帧 ⇒ 2，
     只有显式 --allow-empty / --allow-missing-fields 才降级放行；
  9. 10/41 是真实协议帧（SM_TURN / SM_FEATURECHANGED），默认 compare，不再当心跳剔除；
 10. 首跑缺口补齐的 7 条规则（100/520/525/529/530/654/657）逐个「相同 ⇒ 0、改一个字节 ⇒ 1
     且定位」；520 的 recog（=nChrCount）被取回比较、530 的 recog 走掩码而 body 不掩；
 11. 52 线缆 (54,78)/(78,78) 的归一比较与写坏必红；
 12. multiset_by_tick（同 ident 连续段的多重集比较）：顺序颠倒 ⇒ 0 且登记 masked_order；
     少一条 ⇒ 1 且给出缺失帧；集合相同/内容互换 ⇒ 0；多塞一条 41 ⇒ 1（不吞非多重集 ident）；
     t_ms 不是判定边界（1ms 抖动不得报缺帧，tick_split 只作证据）；跨别的 ident 绝不合并
     （I-1）；任一帧缺 t_ms ⇒ 2 且点名侧/帧/seq（I-2）；块数不同只报 block_align（I-3）；
     phase 进键（M-1）；tick_tolerance_ms 坏值 ⇒ 2。

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
        self.t_new_idents_rules()
        self.t_new_idents_diff(o, r)
        self.t_batch2_rules()
        self.t_batch2_diff()
        self.t_ability_wire(o, r)
        self.t_ability_unknown_pair(o, r)
        self.t_head_mask(o, r)
        self.t_ignore(o, r)
        self.t_real_frames_10_41(o, r)
        self.t_pending_ignore(o, r)
        self.t_masked_missing(o, r)
        self.t_empty_run()
        self.t_missing_head_both()
        self.t_frame_count()
        self.t_missing_body(o)
        self.t_inconsistent(o)
        self.t_malformed(o)
        self.t_no_rules(o, r)
        self.t_body_ranges()
        self.t_rule_validation(o, r)
        self.t_multiset_by_tick(o, r)
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
        frames_rep = (rep or {}).get("frames") or {}
        # 默认表里唯一的 ignore 是 ident 14（固定 NPC 受击帧 = 随机支，跨进程不可复现，
        # 见 rules.json 的 reason）；本用例不含 14 ⇒ 不得有任何帧被 ignore 规则剔掉。
        self.check("baseline: 默认表只有 ident=14 是 ignore，且本用例 0 帧被剔（10/41 仍参与比较）",
                   frames_rep.get("ignored_ident_rules") == [14]
                   and frames_rep.get("ignored", {}).get("oracle", {}).get("count", 0) == 0
                   and frames_rep.get("ignored", {}).get("rust", {}).get("count", 0) == 0,
                   frames_rep)

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
        rc2, rep2 = self.compare(od, rd, ["--allow-unclassified", "9999",
                                          "--allow-missing-fields"])
        self.check("--allow-unclassified 9999 => rc 0 且仍登记放行",
                   rc2 == 0 and any(w.get("kind") == "allowed_unclassified"
                                    for w in (rep2 or {}).get("whitelisted", [])),
                   (rc2, (rep2 or {}).get("whitelisted")))
        rc3, rep3 = self.compare(od, rd, ["--ignore-idents", "9999"])
        self.check("--ignore-idents 9999 => 未覆盖 ident 被剔除，rc 0 且登记 cli_ignore",
                   rc3 == 0 and any(w.get("kind") == "cli_ignore"
                                    for w in (rep3 or {}).get("whitelisted", [])),
                   (rc3, (rep3 or {}).get("whitelisted")))

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

    # -- 首次双栈对拍暴露的 8 条规则缺口（100/520/525/529/530/654/657 + 52 线缆布局对）--
    # 530 = SM_SELECTSERVER_OK：body 末段是 LoginSrv 进程内 SessionID（跨进程必然不同）
    # ⇒ action=mask_body_ranges（只掩偏移 16..end，ip/端口仍逐字节比）；其余 6 条 compare。
    NEW_IDENTS = ((100, "multiset_by_tick"), (520, "compare"), (525, "compare"),
                  (529, "compare"), (530, "mask_body_ranges"), (654, "compare"),
                  (657, "compare"))

    def t_new_idents_rules(self):
        rs = self.api.RuleSet(self.builtin_rules, "builtin", "0" * 64)
        for ident, want_action in self.NEW_IDENTS:
            rule = rs.by_ident.get(ident)
            self.check("内置规则表覆盖 ident=%d（action=%s）" % (ident, want_action),
                       rule is not None and rule["action"] == want_action,
                       rule and rule.get("action"))
            meta = rule or {}
            self.check("ident=%d 规则带 reason/tier/approved_by=PENDING" % ident,
                       all(str(meta.get(k, "")).strip()
                           for k in ("reason", "tier", "approved_by"))
                       and str(meta.get("approved_by", "")).upper().startswith("PENDING"),
                       {k: meta.get(k) for k in ("tier", "approved_by")})
        self.check("520 的 recog 从全局掩码取回比较（=nChrCount，协议内容）",
                   rs.by_ident[520].get("head_compare") == ["recog"],
                   rs.by_ident[520].get("head_compare"))
        self.check("529/530/525/100/654/657 不额外掩码**头部**（只靠全局 recog/t_ms；530 的 body 区间掩码由它自己的规则独立登记）",
                   all(not rs.by_ident[i].get("head_mask")
                       and not rs.by_ident[i].get("head_compare")
                       for i in (100, 525, 529, 530, 654, 657)),
                   [(i, rs.by_ident[i].get("head_mask")) for i in (100, 525, 529, 530, 654, 657)])

    def t_new_idents_diff(self, o, r):
        """每个新 ident 都真的参与判定：相同 ⇒ 0；改一个字节 ⇒ 1 且定位到该 ident。"""
        base_o = json.loads(json.dumps(o))
        base_r = json.loads(json.dumps(r))
        for ident, action in self.NEW_IDENTS:
            body = bytes([0x41 + (ident % 26)]) * 4  # 合成非空 body：只验规则参与判定
            heads = dict(phase="new", conn="x", recog=3, param=11, tag=22, series=33)
            fo = self.frame(ident, body, seq=900, t_ms=7, **heads)
            fr = self.frame(ident, body, seq=901, t_ms=8, **heads)
            od = self.write_run("t21-o-%d" % ident, base_o + [fo])
            rd = self.write_run("t21-r-%d" % ident, base_r + [fr])
            rc, rep = self.compare(od, rd)
            self.check("ident=%d 两侧相同 => rc 0（规则已覆盖，不判 UNCLASSIFIED）" % ident,
                       rc == 0,
                       (rc, (rep or {}).get("undecidable"),
                        (rep or {}).get("coverage", {}).get("unclassified_idents")))
            mut = json.loads(json.dumps(fr))
            mutated = body[:-1] + bytes([body[-1] ^ 0xFF])
            mut["body_hex"] = mutated.hex()
            mut["body_len"] = len(mutated)
            mut["body_sha256"] = _sha(mutated)
            rd2 = self.write_run("t21-r2-%d" % ident, base_r + [mut])
            rc2, rep2 = self.compare(od, rd2)
            if action == "multiset_by_tick":
                # 改 body ⇒ 多重集身份变了：必须 rc 1，并以「缺失/多余」定位到该帧
                self.check("ident=%d 改一个字节 => rc 1 且按多重集定位缺失/多余帧" % ident,
                           rc2 == 1 and any(d.get("ident") == ident
                                            and d.get("kind") in ("multiset_missing",
                                                                  "multiset_extra")
                                            for d in (rep2 or {}).get("diffs", [])),
                           (rc2, (rep2 or {}).get("diffs")))
            else:
                self.check("ident=%d 改一个字节 => rc 1 且定位到 offset=%d"
                           % (ident, len(body) - 1),
                           rc2 == 1 and any(d.get("ident") == ident
                                            and d.get("offset") == len(body) - 1
                                            for d in (rep2 or {}).get("diffs", [])),
                           (rc2, (rep2 or {}).get("diffs")))
        # 520 的 recog 必须被比（nChrCount）；530 的 recog 必须仍被全局掩码（会话/游戏时长）
        fo520 = self.frame(520, b"NAME/1/0/0/30/", phase="selgate", seq=500, t_ms=1, conn="o",
                           recog=1, param=0, tag=1, series=0)
        fr520 = self.frame(520, b"NAME/1/0/0/30/", phase="selgate", seq=501, t_ms=2, conn="r",
                           recog=2, param=0, tag=1, series=0)
        od = self.write_run("t21-o-520r", [fo520])
        rd = self.write_run("t21-r-520r", [fr520])
        rc, rep = self.compare(od, rd)
        self.check("520 的 recog(nChrCount) 不同 => rc 1（head_compare 生效）",
                   rc == 1 and any(d.get("ident") == 520 and d.get("field") == "recog"
                                   for d in (rep or {}).get("diffs", [])),
                   (rc, (rep or {}).get("diffs")))
        # 530 的 body 在线上是 **EDCode 编码后的线缆字节**，夹具必须同形——否则「掩码偏移是线缆空间」
        # 这件事根本咬不到（reviewer-2 F1：早先用未编码 ASCII，`[[16,null]]` 掩掉了端口后三位还全绿）。
        wire530 = self.lc.encode(b"127.0.0.1/17100/2")
        fo530 = self.frame(530, wire530, phase="login", seq=600, t_ms=1, conn="o",
                           recog=2, param=0, tag=0, series=0)
        fr530 = self.frame(530, wire530, phase="login", seq=601, t_ms=2, conn="r",
                           recog=9, param=0, tag=0, series=0)
        od = self.write_run("t21-o-530r", [fo530])
        rd = self.write_run("t21-r-530r", [fr530])
        rc, rep = self.compare(od, rd)
        self.check("530 的 recog 走全局掩码 => rc 0 且登记 masked_head",
                   rc == 0 and any(w.get("ident") == 530 and w.get("field") == "recog"
                                   and w.get("kind") == "masked_head"
                                   for w in (rep or {}).get("whitelisted", [])),
                   (rc, (rep or {}).get("whitelisted")))
        # 530 的**线缆** [20,end) = 明文「最后一个 `/` + SessionID」（+ 尾组余字节）⇒ 只有 SessionID
        # 不同时必须 rc 0 且登记；明文 ip/端口的每个数字在线缆上更靠前，改任一位必红。两条互为咬。
        def _wire(plain):
            return self.lc.encode(plain)

        def _cmp_530(tag, oracle_bytes, rust_bytes):
            fo = self.frame(530, oracle_bytes, phase="login", seq=602, t_ms=1, conn="o",
                            recog=2, param=0, tag=0, series=0)
            fr = self.frame(530, rust_bytes, phase="login", seq=603, t_ms=2, conn="r",
                            recog=9, param=0, tag=0, series=0)
            od = self.write_run("t21-o-%s" % tag, [fo])
            rd = self.write_run("t21-r-%s" % tag, [fr])
            return self.compare(od, rd)

        rc, rep = _cmp_530("530sid", _wire(b"127.0.0.1/17100/2"), _wire(b"127.0.0.1/17100/9"))
        self.check("530 的 SessionID 不同 => rc 0 且登记 mask_body_ranges（末段是进程内计数）",
                   rc == 0 and any(w.get("ident") == 530
                                   and w.get("kind") == "mask_body_ranges"
                                   for w in (rep or {}).get("whitelisted", [])),
                   (rc, (rep or {}).get("whitelisted"), (rep or {}).get("diffs")))
        # 端口：逐位咬（第 1/3/5 位各改一次）+ ip 改一位。旧规则 [[16,null]] 会让前三条假绿。
        port_muts = (b"17200", b"17150", b"17101")
        for i, pm in enumerate(port_muts):
            rc, rep = _cmp_530("530port%d" % i, _wire(b"127.0.0.1/17100/2"),
                               _wire(b"127.0.0.1/" + pm + b"/2"))
            self.check("530 的端口改 %s => rc 1（线缆掩码不得吞掉端口）" % pm.decode(),
                       rc == 1 and any(d.get("ident") == 530 and d.get("kind") == "body"
                                       for d in (rep or {}).get("diffs", [])),
                       (rc, (rep or {}).get("diffs")))
        rc, rep = _cmp_530("530ip", _wire(b"127.0.0.1/17100/2"), _wire(b"127.0.0.2/17100/2"))
        self.check("530 的 ip 改一位 => rc 1（掩码不得吞掉 ip）",
                   rc == 1 and any(d.get("ident") == 530 and d.get("kind") == "body"
                                   for d in (rep or {}).get("diffs", [])),
                   (rc, (rep or {}).get("diffs")))

    # ------------------------------------------------------------------
    # 批次 2（卡 openmir2-conformance-rules-idents）：场景 wait_for 用到、
    # 但规则表未登记的 13 个 ident（11/13/200/201/615/616/635/636/643/644/645/763/767）。
    # ------------------------------------------------------------------
    @staticmethod
    def _client_item(name=b"\xcd\xc0\xc1\xfa", std_mode=5, make_index=123456, ani=0):
        """构造一条 124B `ClientItem` 记录（偏移逐字段对齐 ClientItem.WritePacket）。"""
        b = bytearray()
        b.append(len(name))
        b += name.ljust(14, b"\x00")            # Name@0 = 1B 长度 + 14B 定长
        b.append(std_mode)                       # @15
        b += bytes([0, 0, ani, 0, 0, 0])         # @16-21 Shape/Weight/AniCount/SpecialPwr/ItemDesc/0
        b += b"\x00" * 2                         # @22 Looks
        b += b"\x00" * 2                         # @24 DuraMax
        b += b"\x00" * 10                        # @26 AC MAC DC MC SC
        b += b"\x00" * 4                         # @36 Need/NeedLevel/NeedIdentify/0
        b += b"\x00" * 8                         # @40 Price/Stock
        b += b"\x00" * 8                         # @48 AtkSpd..Undead+0+0
        b += b"\x00" * 12                        # @56 HpAdd/MpAdd/ExpAdd
        b += b"\x00" * 14                        # @68 Eff..ItemType+0
        b += b"\x00" * 2                         # @82 ItemSet
        b.append(0)
        b += b"\x00" * 14                        # @84 Reference (1B 长度 + 14B)
        b.append(0)                              # @99
        b += int(make_index).to_bytes(4, "little", signed=True)   # @100
        b += b"\x00" * 4                         # @104 Dura / @106 DuraMax
        b += b"\x00" * 14                        # @108 Desc
        b += b"\x00" * 2                         # @122 @123
        assert len(b) == 124, len(b)
        return bytes(b)

    def _bag_body(self, records, trailing=True):
        parts = [self.lc.encode(rec) for rec in records]
        return b"/".join(parts) + (b"/" if trailing else b"")

    def _one_pair(self, tag, fr_o, fr_r, rules_path=None):
        od = self.write_run("t22-o-%s" % tag, [fr_o])
        rd = self.write_run("t22-r-%s" % tag, [fr_r])
        extra = ["--rules", rules_path] if rules_path else []
        return self.compare(od, rd, extra)

    def t_batch2_rules(self):
        rs = self.api.RuleSet(self.builtin_rules, "builtin", "0" * 64)
        want = {11: "compare", 13: "compare", 200: "bagitems_records",
                201: "bagitems_records", 615: "compare", 616: "compare", 635: "compare",
                636: "compare", 643: "compare", 644: "compare", 645: "compare",
                763: "compare", 767: "compare"}
        for ident in sorted(want):
            rule = rs.by_ident.get(ident)
            self.check("批次2 覆盖 ident=%d（action=%s）" % (ident, want[ident]),
                       rule is not None and rule["action"] == want[ident],
                       rule and rule.get("action"))
            meta = rule or {}
            self.check("ident=%d 带 reason/tier/approved_by=PENDING" % ident,
                       all(str(meta.get(k, "")).strip()
                           for k in ("reason", "tier", "approved_by"))
                       and str(meta.get("approved_by", "")).upper().startswith("PENDING"),
                       {k: meta.get(k) for k in ("tier", "approved_by")})
        self.check("616/763 的 recog(失败码) 用 head_compare 取回（全局 recog 掩码会永远吞掉失败码）",
                   rs.by_ident[616].get("head_compare") == ["recog"]
                   and rs.by_ident[763].get("head_compare") == ["recog"],
                   (rs.by_ident[616].get("head_compare"),
                    rs.by_ident[763].get("head_compare")))
        self.check("615 的 recog/param（外观 long + FeatureEx）取回比较",
                   rs.by_ident[615].get("head_compare") == ["recog", "param"],
                   rs.by_ident[615].get("head_compare"))
        self.check("11/13 掩 series（单客户端只收得到**怪物**走位，Dir=RandomByte(8) 墙钟随机）",
                   rs.by_ident[11].get("head_mask") == ["series"]
                   and rs.by_ident[13].get("head_mask") == ["series"],
                   (rs.by_ident[11].get("head_mask"), rs.by_ident[13].get("head_mask")))
        self.check("11/13 的 reason 写清「单客户端自过滤 + 怪物 RandomByte(8) + 第二观察者才可判」",
                   all(k in rs.by_ident[i]["reason"]
                       for i in (11, 13)
                       for k in ("ActorId != ActorId", "RandomByte(8)", "第二观察者")),
                   rs.by_ident[11]["reason"][:200])
        self.check("200/201 用记录级比较，且只掩 MakeIndex",
                   rs.by_ident[200].get("record_mask") == ["MakeIndex"]
                   and rs.by_ident[201].get("record_mask") == ["MakeIndex"],
                   (rs.by_ident[200].get("record_mask"),
                    rs.by_ident[201].get("record_mask")))
        self.check("既有规则未被放宽（10 head_mask=[series] / 14 ignore / 52 ability_layout / "
                   "100 multiset_by_tick / 530 mask_body_ranges）",
                   rs.by_ident[10].get("head_mask") == ["series"]
                   and rs.by_ident[14]["action"] == "ignore"
                   and rs.by_ident[52]["action"] == "ability_layout"
                   and rs.by_ident[100]["action"] == "multiset_by_tick"
                   and rs.by_ident[530]["action"] == "mask_body_ranges",
                   {i: rs.by_ident[i]["action"] for i in (10, 14, 52, 100, 530)})

    def t_batch2_diff(self):
        heads_o = dict(phase="b2", seq=700, t_ms=1, conn="o", recog=77, param=3, tag=4,
                       series=5)
        heads_r = dict(heads_o, seq=701, t_ms=2, conn="r", recog=78)

        # 1) 简单 compare 帧：相同 ⇒ 0；改 body（无 body 的改 param）⇒ 1 且定位到该 ident
        simple = [(643, b"King/@main"), (767, b"@continue"), (644, b""),
                  (645, b"Sword/0/100/5/"), (635, b""), (636, b""),
                  (11, b"\x01\x00\x02\x00"), (13, b"\x01\x00\x02\x00")]
        for ident, body in simple:
            rc, rep = self._one_pair("%d-same" % ident,
                                     self.frame(ident, body, **heads_o),
                                     self.frame(ident, body, **heads_r))
            self.check("ident=%d 两侧相同 => rc 0（不再 UNCLASSIFIED）" % ident, rc == 0,
                       (rc, (rep or {}).get("undecidable")))
            if body:
                mut = body[:-1] + bytes([body[-1] ^ 0xFF])
                rc, rep = self._one_pair("%d-mut" % ident,
                                         self.frame(ident, body, **heads_o),
                                         self.frame(ident, mut, **heads_r))
                self.check("ident=%d 改 body 一个字节 => rc 1 且定位到该 ident" % ident,
                           rc == 1 and any(d.get("ident") == ident for d in
                                           (rep or {}).get("diffs", [])),
                           (rc, (rep or {}).get("diffs")))
            else:
                rc, rep = self._one_pair("%d-param" % ident,
                                         self.frame(ident, body, **heads_o),
                                         self.frame(ident, body,
                                                    **dict(heads_r, param=9)))
                self.check("ident=%d（无 body）param 不同 => rc 1" % ident,
                           rc == 1 and any(d.get("ident") == ident
                                           and d.get("field") == "param"
                                           for d in (rep or {}).get("diffs", [])),
                           (rc, (rep or {}).get("diffs")))

        # 1b) 11/13 的 series（MakeWord(Dir, Light)）不同 ⇒ rc 0 且登记 masked_head；
        #     body/param 仍必须咬（掩码只掩 series，不外溢）
        for ident in (11, 13):
            rc, rep = self._one_pair("%d-series" % ident,
                                     self.frame(ident, b"\x01\x00\x02\x00", **heads_o),
                                     self.frame(ident, b"\x01\x00\x02\x00",
                                                **dict(heads_r, series=6)))
            self.check("ident=%d series(Dir/Light) 不同 => rc 0 且登记 masked_head" % ident,
                       rc == 0 and any(w.get("ident") == ident and w.get("field") == "series"
                                       and w.get("kind") == "masked_head"
                                       for w in (rep or {}).get("whitelisted", [])),
                       (rc, (rep or {}).get("whitelisted"), (rep or {}).get("diffs")))
            rc, rep = self._one_pair("%d-tag" % ident,
                                     self.frame(ident, b"\x01\x00\x02\x00", **heads_o),
                                     self.frame(ident, b"\x01\x00\x02\x00",
                                                **dict(heads_r, tag=99)))
            self.check("ident=%d tag(落点 y) 不同 => rc 1（series 掩码不外溢）" % ident,
                       rc == 1 and any(d.get("ident") == ident and d.get("field") == "tag"
                                       for d in (rep or {}).get("diffs", [])),
                       (rc, (rep or {}).get("diffs")))

        # 2) 失败码取回：616/763 的 recog 必须真的被比（否则全局掩码会吞掉）
        for ident, bad in ((616, -4), (763, 2)):
            rc, rep = self._one_pair("%d-code" % ident,
                                     self.frame(ident, b"", **dict(heads_o, recog=-1)),
                                     self.frame(ident, b"", **dict(heads_r, recog=bad)))
            self.check("ident=%d 的 recog(失败码) 不同 => rc 1（head_compare 生效）" % ident,
                       rc == 1 and any(d.get("ident") == ident and d.get("field") == "recog"
                                       for d in (rep or {}).get("diffs", [])),
                       (rc, (rep or {}).get("diffs")))
        rc, rep = self._one_pair("615-param",
                                 self.frame(615, b"", **dict(heads_o, param=11)),
                                 self.frame(615, b"", **dict(heads_r, param=12)))
        self.check("615 的 param(FeatureEx) 不同 => rc 1（head_compare 生效）",
                   rc == 1 and any(d.get("ident") == 615 and d.get("field") == "param"
                                   for d in (rep or {}).get("diffs", [])),
                   (rc, (rep or {}).get("diffs")))

        # 3) 200/201 记录级比较：只掩 MakeIndex，其余任何字节都咬
        rec_a = self._client_item(make_index=1000)
        rec_b = self._client_item(make_index=2000)
        for ident in (200, 201):
            body_o = self._bag_body([rec_a], trailing=(ident == 201))
            body_r = self._bag_body([rec_b], trailing=(ident == 201))
            rc, rep = self._one_pair("%d-mk" % ident,
                                     self.frame(ident, body_o, **dict(heads_o, series=1)),
                                     self.frame(ident, body_r, **dict(heads_r, series=1)))
            self.check("ident=%d 只有 MakeIndex 不同 => rc 0 且登记 bagitems_masked_field"
                       % ident,
                       rc == 0 and any(w.get("ident") == ident
                                       and w.get("kind") == "bagitems_masked_field"
                                       for w in (rep or {}).get("whitelisted", [])),
                       (rc, (rep or {}).get("whitelisted"), (rep or {}).get("diffs")))
            # Name 一个字节不同 ⇒ 必红（掩码只掩 MakeIndex，不许连带吞掉别的字段）
            rec_name = self._client_item(name=b"\xcd\xc0\xc1\xfb", make_index=1000)
            body_r2 = self._bag_body([rec_name], trailing=(ident == 201))
            rc, rep = self._one_pair("%d-name" % ident,
                                     self.frame(ident, body_o, **dict(heads_o, series=1)),
                                     self.frame(ident, body_r2, **dict(heads_r, series=1)))
            self.check("ident=%d Name 改一个字节 => rc 1（MakeIndex 掩码不外溢）" % ident,
                       rc == 1 and any(d.get("ident") == ident
                                       and d.get("kind") == "bagitems_record"
                                       and d.get("slot") == 0
                                       for d in (rep or {}).get("diffs", [])),
                       (rc, (rep or {}).get("diffs")))
            # 条数不同 ⇒ 必红
            body_r3 = self._bag_body([rec_a, rec_a], trailing=(ident == 201))
            rc, rep = self._one_pair("%d-count" % ident,
                                     self.frame(ident, body_o, **dict(heads_o, series=1)),
                                     self.frame(ident, body_r3, **dict(heads_r, series=2)))
            self.check("ident=%d 记录条数不同 => rc 1（bagitems_count）" % ident,
                       rc == 1 and any(d.get("ident") == ident
                                       and d.get("kind") == "bagitems_count"
                                       for d in (rep or {}).get("diffs", [])),
                       (rc, (rep or {}).get("diffs")))
        # 解不开的记录 ⇒ UNDECIDABLE（不按通过处理）
        rc, rep = self._one_pair("201-bad",
                                 self.frame(201, self._bag_body([rec_a]),
                                            **dict(heads_o, series=1)),
                                 self.frame(201, b"%%%%", **dict(heads_r, series=1)))
        self.check("201 记录解不开 => rc 2（bagitems_decode，不猜）",
                   rc == 2 and any(u.get("ident") == 201
                                   and u.get("kind") == "bagitems_decode"
                                   for u in (rep or {}).get("undecidable", [])),
                   (rc, (rep or {}).get("undecidable")))

        # 4) 反向牙：把 ignore 的 ident 14 改回 compare ⇒ 同一对产物必须由 0 变 1
        #    （证明 ignore 是真在剔帧，不是「反正没差异」；也证明规则表是判定的载荷）
        f41 = self.frame(41, b"\x01\x02", **heads_o)
        f14 = self.frame(14, b"The merchant is hit", **dict(heads_o, seq=702))
        body_o = [f41, f14]
        body_r = [f41]
        od = self.write_run("t22-ign-o", body_o)
        rd = self.write_run("t22-ign-r", body_r)
        # 内置 14 的 approved_by 是 PENDING ⇒ 真剔帧时按既有口径判 UNDECIDABLE（不是放行）。
        # 牙要证明的是「ignore 与 compare 的判定差」，所以用**已批准**的副本消除 PENDING 兜底。
        rules1 = json.loads(json.dumps(self.builtin_rules))
        for rule in rules1["rules"]:
            if rule.get("ident") == 14:
                rule["approved_by"] = "selftest: approved (non-pending)"
        path1 = self.rules_file("rules-14-ignore.json", rules1)
        rc, rep = self.compare(od, rd)
        self.check("忽略牙：内置 14 是 PENDING ignore ⇒ 真剔帧时 rc 2（pending_ignore_rule 兜底）",
                   rc == 2 and any(u.get("kind") == "pending_ignore_rule"
                                   for u in (rep or {}).get("undecidable", [])),
                   (rc, (rep or {}).get("undecidable")))
        rc, rep = self.compare(od, rd, ["--rules", path1])
        self.check("忽略牙：已批准 ignore 下多出的 ident=14 => rc 0 且 ignored 记 1 条",
                   rc == 0 and ((rep or {}).get("frames", {}).get("ignored", {})
                                .get("oracle", {}).get("idents") == [14]),
                   (rc, (rep or {}).get("frames"), (rep or {}).get("diffs")))
        rules2 = json.loads(json.dumps(self.builtin_rules))
        for rule in rules2["rules"]:
            if rule.get("ident") == 14:
                rule["action"] = "compare"
                rule["approved_by"] = "selftest: approved (non-pending)"
                rule.pop("reason", None)
                rule["reason"] = "selftest 变异：ignore → compare（证明 ignore 在剔帧）"
        path2 = self.rules_file("rules-14-compare.json", rules2)
        rc, rep = self.compare(od, rd, ["--rules", path2])
        self.check("忽略牙：同一条 14 改回 compare => rc 1（多出的帧立刻暴露）",
                   rc == 1, (rc, (rep or {}).get("diffs")))

    def t_ability_wire(self, o, r):
        """首次真机对拍实测的 52 线缆长度对 (54, 78)：EDCode 归一后按 40/58 语义比。"""
        lc = self.lc
        wire40 = lc.encode(self.ability_body(40))
        wire58 = lc.encode(self.ability_body(58))
        self.check("selftest 前置：encoded_len(40)=54 / encoded_len(58)=78",
                   len(wire40) == 54 and len(wire58) == 78, (len(wire40), len(wire58)))

        def with_wire(frames, body):
            out = json.loads(json.dumps(frames))
            for fr in out:
                if fr["ident"] == 52:
                    fr["body_hex"] = body.hex()
                    fr["body_len"] = len(body)
                    fr["body_sha256"] = _sha(body)
            return out

        o2 = with_wire(o, wire40)
        r2 = with_wire(r, wire58)
        od = self.write_run("t22-o", o2)
        rd = self.write_run("t22-r", r2)
        rc, rep = self.compare(od, rd)
        self.check("52 线缆 (54,78) => EDCode 归一后语义全等 => rc 0",
                   rc == 0, (rc, (rep or {}).get("undecidable")))
        wl = [w for w in (rep or {}).get("whitelisted", [])
              if w.get("kind") == "ability_layout"]
        self.check("52 线缆布局差登记 ability_layout（wire 54/78 + wire_normalized）",
                   bool(wl) and wl[0].get("wire_normalized") is True
                   and wl[0].get("oracle_wire_len") == 54
                   and wl[0].get("rust_wire_len") == 78
                   and wl[0].get("oracle_len") == 40 and wl[0].get("rust_len") == 58,
                   wl)
        # 牙：线缆形态下改 hp 一个语义字段，必须在明文空间定位
        r3 = with_wire(r, lc.encode(self.ability_body(58, hp=9999)))
        rd3 = self.write_run("t22-r3", r3)
        rc3, rep3 = self.compare(od, rd3)
        self.check("52 线缆改 hp => rc 1 且字段 52_semantic:hp（space=plain）",
                   rc3 == 1 and any(d.get("field") == "52_semantic:hp"
                                    and d.get("space") == "plain"
                                    for d in (rep3 or {}).get("diffs", [])),
                   (rc3, (rep3 or {}).get("diffs")))
        # 牙：两侧同一明文布局（58/58）时按明文逐字节比；改一字节必须红，偏移在明文空间
        o58 = with_wire(o, wire58)
        r58 = with_wire(r, wire58)
        od58 = self.write_run("t22-o58", o58)
        rd58 = self.write_run("t22-r58", r58)
        rc5, rep5 = self.compare(od58, rd58)
        self.check("52 两侧同为 58B 线缆（78,78）=> 明文逐字节相等 => rc 0 且登记 wire 归一",
                   rc5 == 0 and any(w.get("kind") == "ability_layout_wire"
                                    for w in (rep5 or {}).get("whitelisted", [])),
                   (rc5, (rep5 or {}).get("whitelisted"), (rep5 or {}).get("undecidable")))
        bad58 = bytearray(self.ability_body(58))
        bad58[46] ^= 0xFF  # weight 低字节
        r4 = with_wire(r, lc.encode(bytes(bad58)))
        rd4 = self.write_run("t22-r4", r4)
        rc4, rep4 = self.compare(od58, rd4)
        self.check("52 线缆归一后同布局改一字节 => rc 1 且偏移=46（明文空间，按字节比）",
                   rc4 == 1 and any(d.get("ident") == 52 and d.get("kind") == "body"
                                    and d.get("offset") == 46 and d.get("space") == "plain"
                                    for d in (rep4 or {}).get("diffs", [])),
                   (rc4, (rep4 or {}).get("diffs")))

    def t_ability_unknown_pair(self, o, r):
        """未登记的 52 长度对 => rc 2（fail-closed，新对必须附来源再登记）。"""
        def with_body(frames, body):
            out = json.loads(json.dumps(frames))
            for fr in out:
                if fr["ident"] == 52:
                    fr["body_hex"] = body.hex()
                    fr["body_len"] = len(body)
                    fr["body_sha256"] = _sha(body)
            return out

        o2 = with_body(o, b"\x00" * 41)
        r2 = with_body(r, b"\x00" * 41)
        od = self.write_run("t23-o", o2)
        rd = self.write_run("t23-r", r2)
        rc, rep = self.compare(od, rd)
        self.check("52 未登记长度对(41,41) => rc 2 (ability_layout_unknown)",
                   rc == 2 and any(u.get("kind") == "ability_layout_unknown"
                                   for u in (rep or {}).get("undecidable", [])),
                   (rc, (rep or {}).get("undecidable")))

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
        self.check("一侧多出 10 帧且未显式忽略 => rc 1（10 是真实帧，默认比）",
                   rc == 1 and any(d.get("kind") in ("ident", "block_align")
                                   for d in (rep or {}).get("diffs", [])),
                   (rc, (rep or {}).get("diffs")))
        rc_i, rep_i = self.compare(od, rd, ["--ignore-idents", "10,41"])
        self.check("--ignore-idents 10,41 => 多出的 10 帧被剔除，rc 0 且登记 cli_ignore",
                   rc_i == 0 and any(w.get("kind") == "cli_ignore"
                                     for w in (rep_i or {}).get("whitelisted", [])),
                   (rc_i, (rep_i or {}).get("whitelisted")))
        # 牙：显式剔除的帧不能顺带吞掉真差异
        for fr in r2:
            if fr["ident"] == 658:
                body = bytes.fromhex(fr["body_hex"])
                mutated = body[:-1] + bytes([body[-1] ^ 0x01])
                fr["body_hex"] = mutated.hex()
                fr["body_sha256"] = _sha(mutated)
                fr["body_len"] = len(mutated)
        rd2 = self.write_run("t07-r2", r2)
        rc2, rep2 = self.compare(od, rd2, ["--ignore-idents", "10,41"])
        self.check("显式剔除 10/41 后 658 改字节 => 仍 rc 1 且偏移正确",
                   rc2 == 1 and any(d.get("ident") == 658 and d.get("offset") == len(body) - 1
                                    for d in (rep2 or {}).get("diffs", [])),
                   (rc2, (rep2 or {}).get("diffs")))

    def t_real_frames_10_41(self, o, r):
        # 41 的 param 不同 => 默认必须红（证明 10/41 不再是心跳）
        r2 = json.loads(json.dumps(r))
        for fr in r2:
            if fr["ident"] == 41:
                fr["param"] = 7  # oracle 是 0
        od = self.write_run("t16-o", o)
        rd = self.write_run("t16-r", r2)
        rc, rep = self.compare(od, rd)
        self.check("41 的 param 不同 => rc 1（10/41 默认 compare）",
                   rc == 1 and any(d.get("ident") == 41 and d.get("field") == "param"
                                   for d in (rep or {}).get("diffs", [])),
                   (rc, (rep or {}).get("diffs")))
        rs = self.api.RuleSet(self.builtin_rules, "builtin", "0" * 64)
        self.check("内置规则表 10/41 的 action 是 compare（不是 ignore）",
                   rs.by_ident[10]["action"] == "compare"
                   and rs.by_ident[41]["action"] == "compare",
                   [(i, rs.by_ident[i]["action"]) for i in (10, 41)])

    def t_pending_ignore(self, o, r):
        rules = json.loads(json.dumps(self.builtin_rules))
        rules["rules"] = [x for x in rules["rules"] if x.get("ident") != 658]
        rules["rules"].append({"ident": 658, "action": "ignore",
                               "reason": "selftest: PENDING 的 ignore 不许直接判绿",
                               "tier": "L", "approved_by": "PENDING selftest"})
        rp = self.rules_file("t17-rules.json", rules)
        od = self.write_run("t17-o", o)
        rd = self.write_run("t17-r", r)
        rc, rep = self.compare(od, rd, ["--rules", rp])
        self.check("PENDING ignore 真的剔了帧 => rc 2 (pending_ignore_rule)",
                   rc == 2 and any(u.get("kind") == "pending_ignore_rule"
                                   for u in (rep or {}).get("undecidable", [])),
                   (rc, (rep or {}).get("undecidable")))

    def t_masked_missing(self, o, r):
        # 46 的 param 被规则掩码；只给一侧 => 不能静默跳过（CH-4）
        r2 = json.loads(json.dumps(r))
        for fr in r2:
            if fr["ident"] == 46:
                fr.pop("param", None)
        od = self.write_run("t18-o", o)
        rd = self.write_run("t18-r", r2)
        rc, rep = self.compare(od, rd)
        self.check("被掩码字段只有一侧给了 => rc 2 (missing_field)",
                   rc == 2 and any(u.get("kind") == "missing_field"
                                   and u.get("field") == "param"
                                   and u.get("masked") is True
                                   for u in (rep or {}).get("undecidable", [])),
                   (rc, (rep or {}).get("undecidable")))

    def t_empty_run(self):
        od = self.write_run("t19-o", [])
        rd = self.write_run("t19-r", [])
        rc, rep = self.compare(od, rd)
        self.check("两侧 0 帧 => rc 2 (empty_run)",
                   rc == 2 and any(u.get("kind") == "empty_run"
                                   for u in (rep or {}).get("undecidable", [])),
                   (rc, (rep or {}).get("undecidable")))
        rc2, rep2 = self.compare(od, rd, ["--allow-empty"])
        self.check("--allow-empty => 空跑 rc 0 且显式登记放行",
                   rc2 == 0 and any(w.get("kind") == "allowed_empty_run"
                                    for w in (rep2 or {}).get("whitelisted", [])),
                   (rc2, (rep2 or {}).get("whitelisted")))

    def t_missing_head_both(self):
        o = [self.frame(50, b"", phase="login")]
        r = [self.frame(50, b"", phase="login")]
        od = self.write_run("t20-o", o)
        rd = self.write_run("t20-r", r)
        rc, rep = self.compare(od, rd)
        self.check("两侧都缺同一头部字段 => rc 2（默认判不了）",
                   rc == 2 and any(u.get("kind") == "missing_field_both_sides"
                                   for u in (rep or {}).get("undecidable", [])),
                   (rc, (rep or {}).get("undecidable")))
        rc2, rep2 = self.compare(od, rd, ["--allow-missing-fields"])
        self.check("--allow-missing-fields => rc 0 且显式登记未比较字段",
                   rc2 == 0 and any(w.get("kind") == "allowed_missing_fields"
                                    for w in (rep2 or {}).get("whitelisted", [])),
                   (rc2, (rep2 or {}).get("whitelisted")))

    def t_frame_count(self):
        o = [self.frame(50, b"", phase="login"), self.frame(658, b"x", phase="notice")]
        r = [self.frame(50, b"", phase="login")]
        od = self.write_run("t08-o", o)
        rd = self.write_run("t08-r", r)
        rc, rep = self.compare(od, rd, ["--allow-missing-fields"])
        self.check("块数不同（尾部多一帧）=> rc 1 + block_align",
                   rc == 1 and any(d.get("kind") == "block_align"
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
        rc, rep = self.compare(od, rd, ["--rules", rp, "--allow-missing-fields"])
        self.check("body 区间掩码内不同 => rc 0 且显式登记",
                   rc == 0 and any(w.get("kind") == "mask_body_ranges"
                                   for w in (rep or {}).get("whitelisted", [])),
                   (rc, (rep or {}).get("whitelisted")))
        r2 = [self.frame(46, b"\x01\x02\x03\x05", phase="day")]
        rd2 = self.write_run("t13-r2", r2)
        rc2, rep2 = self.compare(od, rd2, ["--rules", rp, "--allow-missing-fields"])
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
        # multiset_by_tick 的坏规则必须 fail-closed（rc 2，不带着错规则去比）
        badtol = json.loads(json.dumps(self.builtin_rules))
        for rl in badtol["rules"]:
            if rl.get("ident") == 100:
                rl["tick_tolerance_ms"] = -1
        rp3 = self.rules_file("t14-badtol.json", badtol)
        rc3, rep3 = self.compare(od, rd, ["--rules", rp3])
        self.check("multiset_by_tick 的 tick_tolerance_ms<0 => rc 2 (InputError)",
                   rc3 == 2 and any(u.get("source") == "input"
                                    for u in (rep3 or {}).get("undecidable", [])),
                   (rc3, (rep3 or {}).get("undecidable")))
        misplace = json.loads(json.dumps(self.builtin_rules))
        for rl in misplace["rules"]:
            if rl.get("ident") == 46:
                rl["tick_tolerance_ms"] = 5
        rp4 = self.rules_file("t14-misplace-tol.json", misplace)
        rc4, rep4 = self.compare(od, rd, ["--rules", rp4])
        self.check("tick_tolerance_ms 用在非 multiset 动作 => rc 2 (InputError)",
                   rc4 == 2 and any(u.get("source") == "input"
                                    for u in (rep4 or {}).get("undecidable", [])),
                   (rc4, (rep4 or {}).get("undecidable")))

    # -- multiset_by_tick：同 ident 连续段的多重集比较（对拍降噪 v2）--
    BODIES_100 = (b"welcome-static", b"anti-cheat-static",
                  b"service-qq-static", b"attack-mode-static")

    def f100(self, conn, seq, body, t_ms, phase="gamegate"):
        return self.frame(100, body, phase=phase, seq=seq, conn=conn, t_ms=t_ms,
                          recog=5555, param=65499, tag=0, series=1)

    def ms_frames(self, conn, t0, order, seq0=100):
        """造一段 ident=100 的登录系统提示（同一 t_ms）。"""
        return [self.f100(conn, seq0 + k, body, t0) for k, body in enumerate(order)]

    def t_multiset_by_tick(self, o, r):
        A, B, C = self.BODIES_100[:3]
        rs = self.api.RuleSet(self.builtin_rules, "builtin", "0" * 64)
        multi = sorted(i for i, rl in rs.by_ident.items()
                       if rl["action"] == "multiset_by_tick")
        self.check("内置规则表只有 ident=100 用 multiset_by_tick（不扩散到 10/41/14）",
                   multi == [100], multi)
        self.check("ident=100 的 tick_tolerance_ms=0（仅标注窗口，不作判定边界）",
                   rs.by_ident[100].get("tick_tolerance_ms") == 0,
                   rs.by_ident[100].get("tick_tolerance_ms"))

        def run_case(tag, oframes, rframes, extra=()):
            od = self.write_run("t24-%s-o" % tag, oframes)
            rd = self.write_run("t24-%s-r" % tag, rframes)
            return self.compare(od, rd, list(extra))

        # ① 两条 100 顺序颠倒 => rc 0 且登记 masked_order
        rc, rep = run_case("order", self.ms_frames("o", 1000, [A, B]),
                           self.ms_frames("r", 1000, [B, A]))
        self.check("multiset ① 两条 100 顺序颠倒 => rc 0", rc == 0,
                   (rc, (rep or {}).get("diffs"), (rep or {}).get("undecidable")))
        mo = (rep or {}).get("masked_order") or []
        self.check("multiset ① 顺序颠倒 => 登记 masked_order（ident=100，非静默 PASS）",
                   len(mo) == 1 and mo[0].get("ident") == 100
                   and mo[0].get("kind") == "masked_order"
                   and mo[0]["oracle_order"][0]["body_prefix_hex"] == A[:16].hex()
                   and mo[0]["rust_order"][0]["body_prefix_hex"] == B[:16].hex(),
                   mo)

        # ② 少一条 100 => rc 1（缺失帧具体到 body 前 16B）
        rc, rep = run_case("missing", self.ms_frames("o", 1000, [A, B]),
                           self.ms_frames("r", 1000, [A]))
        md = [d for d in (rep or {}).get("diffs", [])
              if d.get("kind") == "multiset_missing"]
        self.check("multiset ② 少一条 100 => rc 1", rc == 1,
                   (rc, (rep or {}).get("diffs")))
        self.check("multiset ② 缺失帧给出 ident/关键字段/body 前 16 字节",
                   len(md) == 1 and md[0].get("ident") == 100
                   and md[0]["frame"].get("param") == 65499
                   and md[0]["frame"].get("series") == 1
                   and md[0]["frame"].get("body_prefix_hex") == B[:16].hex()
                   and md[0].get("count") == 1, md)

        # ③ 三条 100 内容互换/轮换（集合相同、位置不同）=> rc 0 + masked_order
        rc, rep = run_case("rotate", self.ms_frames("o", 1000, [A, B, C]),
                           self.ms_frames("r", 1000, [B, C, A]))
        self.check("multiset ③ 两条 100 内容互换/轮换（集合相同）=> rc 0", rc == 0,
                   (rc, (rep or {}).get("diffs")))
        self.check("multiset ③ 轮换仍登记 masked_order",
                   len((rep or {}).get("masked_order") or []) == 1,
                   (rep or {}).get("masked_order"))

        # ④ 塞一条多余的 41 => rc 1（机制不会吞非多重集 ident）
        rf = self.ms_frames("r", 1000, [A, B])
        rf.insert(1, self.frame(41, b"", phase="gamegate", seq=999, conn="r",
                                t_ms=1000, recog=5555, param=0, tag=0, series=0))
        rc, rep = run_case("extra41", self.ms_frames("o", 1000, [A, B]), rf)
        blob = json.dumps((rep or {}).get("diffs", []), ensure_ascii=True)
        self.check("multiset ④ 多塞一条 41 => rc 1（非多重集 ident 不被吞）",
                   rc == 1 and "41" in blob, (rc, (rep or {}).get("diffs")))

        # ⑤ 条数相同但一条 body 不同 => rc 1，缺失+多余各有具体帧
        rc, rep = run_case("changed", self.ms_frames("o", 1000, [A, B]),
                           self.ms_frames("r", 1000, [A, b"tampered-static"]))
        kinds = sorted({d.get("kind") for d in (rep or {}).get("diffs", [])
                        if d.get("ident") == 100})
        self.check("multiset ⑤ 一条 body 不同 => rc 1 且缺失/多余各一条",
                   rc == 1 and kinds == ["multiset_extra", "multiset_missing"], kinds)

        # ⑥ reviewer-2 I-3：t_ms 不是判定边界（同一帧集 + 1ms 抖动不得报缺帧）
        o6 = [self.f100("o", 1, A, 1000), self.f100("o", 2, B, 1000)]
        r6 = [self.f100("r", 3, B, 1000), self.f100("r", 4, A, 1003)]
        rc, rep = run_case("i3", o6, r6)
        self.check("multiset ⑥(I-3) 同帧集 + 1ms 抖动 + 顺序不同 => rc 0（不误报缺帧）",
                   rc == 0, (rc, (rep or {}).get("diffs"), (rep or {}).get("undecidable")))
        mo = (rep or {}).get("masked_order") or []
        self.check("multiset ⑥(I-3) 登记 masked_order 且 tick_split=True（窗口 1 vs 2）",
                   len(mo) == 1 and mo[0].get("tick_split") is True
                   and len(mo[0].get("oracle_windows") or []) == 1
                   and len(mo[0].get("rust_windows") or []) == 2, mo)
        r6b = [self.f100("r", 5, A, 1000), self.f100("r", 6, B, 1001)]
        rc, rep = run_case("i3same", o6, r6b)
        self.check("multiset ⑥(I-3) 同序 + 1ms 抖动 => rc 0 且无缺帧/多余 diff（与 base 一致）",
                   rc == 0 and not (rep or {}).get("diffs"),
                   (rc, (rep or {}).get("diffs")))
        rules = json.loads(json.dumps(self.builtin_rules))
        for rl in rules["rules"]:
            if rl.get("ident") == 100:
                rl["tick_tolerance_ms"] = 5
        rp = self.rules_file("t24-rules-tol5.json", rules)
        rc, rep = run_case("i3tol5", o6, r6, ["--rules", rp])
        mo = (rep or {}).get("masked_order") or []
        self.check("multiset ⑥ 容差 5ms ⇒ 两侧各 1 窗口 => tick_split=False",
                   rc == 0 and len(mo) == 1 and mo[0].get("tick_split") is False,
                   (rc, mo))

        # ⑦ reviewer-2 I-2：缺 t_ms 一律 rc 2（含运行中的**后续**帧），点名侧/帧/seq
        o7 = [self.f100("o", 10, A, 1000),
              self.frame(100, B, phase="gamegate", seq=11, conn="o",
                         recog=5555, param=65499, tag=0, series=1)]
        r7 = [self.f100("r", 12, A, 1000), self.f100("r", 13, B, 1000)]
        rc, rep = run_case("i2", o7, r7)
        mt = [u for u in (rep or {}).get("undecidable", [])
              if u.get("kind") == "multiset_tick_missing"]
        self.check("multiset ⑦(I-2) 后续帧缺 t_ms => rc 2 (multiset_tick_missing)",
                   rc == 2 and len(mt) == 1, (rc, (rep or {}).get("undecidable")))
        self.check("multiset ⑦(I-2) 错误信息点名 oracle 侧 + frame_index + seq",
                   len(mt) == 1 and mt[0].get("side") == "oracle"
                   and mt[0].get("frame_index") == 1 and mt[0].get("seq") == 11, mt)
        o7b = self.ms_frames("o", 1000, [A, B])
        for fr in o7b:
            fr.pop("t_ms", None)
        rc, rep = run_case("notick", o7b, self.ms_frames("r", 1000, [A, B]))
        self.check("multiset ⑦ 整侧缺 t_ms => rc 2",
                   rc == 2 and any(u.get("kind") == "multiset_tick_missing"
                                   for u in (rep or {}).get("undecidable", [])),
                   (rc, (rep or {}).get("undecidable")))

        # ⑧ reviewer-2 I-1：跨过别的 ident 绝不合块（[100A,42,100B] vs [100B,42,100A]）
        def f42(conn, seq):
            return self.frame(42, b"", phase="gamegate", seq=seq, conn=conn,
                              t_ms=1000, recog=5555, param=0, tag=0, series=0)

        o8 = [self.f100("o", 20, A, 1000), f42("o", 21), self.f100("o", 22, B, 1000)]
        r8_bad = [self.f100("r", 23, B, 1000), f42("r", 24), self.f100("r", 25, A, 1000)]
        rc, rep = run_case("i1bad", o8, r8_bad)
        self.check("multiset ⑧(I-1) [100A,42,100B] vs [100B,42,100A] => rc 1"
                   "（不得跨 42 合并）", rc == 1, (rc, (rep or {}).get("diffs")))
        self.check("multiset ⑧(I-1) 该夹具不得登记 masked_order（跨块重排不算顺序抖动）",
                   not (rep or {}).get("masked_order"), (rep or {}).get("masked_order"))
        r8_ok = [self.f100("r", 26, A, 1000), f42("r", 27), self.f100("r", 28, B, 1000)]
        rc, rep = run_case("i1ok", o8, r8_ok)
        self.check("multiset ⑧(I-1) 对照：同序 => rc 0 且无 masked_order",
                   rc == 0 and not (rep or {}).get("masked_order"),
                   (rc, (rep or {}).get("masked_order")))

        # ⑨ reviewer-2 M-1：phase 进键（重复 body、phase 不同、顺序互换 => 0 + masked_order）
        o9 = [self.f100("o", 30, A, 1000, phase="login"),
              self.f100("o", 31, A, 1000, phase="gamegate")]
        r9 = [self.f100("r", 32, A, 1000, phase="gamegate"),
              self.f100("r", 33, A, 1000, phase="login")]
        rc, rep = run_case("m1", o9, r9)
        self.check("multiset ⑨(M-1) 同 body、phase 不同、顺序互换 => rc 0 + masked_order",
                   rc == 0 and len((rep or {}).get("masked_order") or []) == 1,
                   (rc, (rep or {}).get("diffs"), (rep or {}).get("masked_order")))
        r9b = [self.f100("r", 34, A, 1000, phase="login"),
               self.f100("r", 35, A, 1000, phase="login")]
        rc, rep = run_case("m1diff", o9, r9b)
        self.check("multiset ⑨(M-1) phase 真不同（集合不同）=> rc 1 缺失/多余",
                   rc == 1 and {"multiset_missing", "multiset_extra"}
                   <= {d.get("kind") for d in (rep or {}).get("diffs", [])},
                   (rc, (rep or {}).get("diffs")))

        # ⑩ reviewer-2 I-3①：块数不同但帧数相同 => block_align，绝不输出「帧数 2 vs 2 不同」
        o10 = [self.f100("o", 40, A, 1000), self.f100("o", 41, B, 1000)]
        r10 = [self.f100("r", 42, A, 1000), f42("r", 43)]
        rc, rep = run_case("blockalign", o10, r10)
        ba = [d for d in (rep or {}).get("diffs", []) if d.get("kind") == "block_align"]
        self.check("multiset ⑩(I-3) 块数不同但帧数相同 => rc 1 + block_align 且两侧帧数都=2",
                   rc == 1 and len(ba) == 1 and ba[0].get("oracle_frames") == 2
                   and ba[0].get("rust_frames") == 2, (rc, ba))

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
        rc, rep = self.compare(od, rd, ["--allow-missing-fields"])
        db = (rep or {}).get("db") or {}
        self.check("库快照全等（跨库前缀归一）=> rc 0 + state=compared",
                   rc == 0 and db.get("state") == "compared", (rc, db.get("state"), db.get("detail")))
        self.check("库快照比较表集非空",
                   (db.get("summary") or {}).get("tables", {}).get("mir2_db.characters_ablity") == 0,
                   (db.get("summary") or {}).get("tables"))

        rd2 = self.write_run("t15-r2", frames, db=snap("mir2_test_db_rust", 8))
        rc2, rep2 = self.compare(od, rd2, ["--allow-missing-fields"])
        self.check("库字段 Level 7->8 => rc 1 + value 差异",
                   rc2 == 1 and any(d.get("source") == "db" and d.get("kind") == "value"
                                    for d in (rep2 or {}).get("diffs", [])),
                   (rc2, (rep2 or {}).get("diffs")))

        rd3 = self.write_run("t15-r3", frames)  # 无 db_snapshot
        rc3, rep3 = self.compare(od, rd3, ["--allow-missing-fields"])
        self.check("只有一侧有库快照 => rc 2",
                   rc3 == 2 and ((rep3 or {}).get("db") or {}).get("state") == "one_sided",
                   (rc3, (rep3 or {}).get("db")))

        od4 = self.write_run("t15-o4", frames)
        rd4 = self.write_run("t15-r4", frames)
        rc4, rep4 = self.compare(od4, rd4, ["--allow-missing-fields"])
        self.check("两侧都无库快照 => rc 0 + state=absent_both（显式登记未比较）",
                   rc4 == 0 and ((rep4 or {}).get("db") or {}).get("state") == "absent_both",
                   (rc4, (rep4 or {}).get("db")))

        rc5, rep5 = self.compare(od4, rd4, ["--allow-missing-fields", "--require-db"])
        self.check("两侧都无库快照 + --require-db => rc 2 (db_snapshot_absent)",
                   rc5 == 2 and any(u.get("kind") == "db_snapshot_absent"
                                    for u in (rep5 or {}).get("undecidable", [])),
                   (rc5, (rep5 or {}).get("undecidable")))

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
