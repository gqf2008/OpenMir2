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
 13. 7 条规则被全局 recog 掩码吞掉语义字段（卡 openmir2-conformance-rules-recog-unmask）：
     54/5008/1106/752/811/766/213 的 Recog 都是语义值（非 ActorId）⇒ head_compare:["recog"]；
     改前仅 recog 不同 ⇒ 0（被吞）、改后 ⇒ 1 且 field=recog；另有「删 head_compare ⇒ 7 条全变 0」
     与「给 ActorId 类 42 硬加 head_compare ⇒ 跨栈必红」两颗牙。
 14. 202/203/642/709 四条规则（卡 openmir2-conformance-rules-updateitem-durachg）：
     202/203 单条 124B ClientItem 只掩 MakeIndex；642 Recog=Dura 窄掩、槽位/DuraMax 保留；
     709 无掩码全字节比。双向断言 + 牙①（删 642 ⇒ 整轮 rc 2 UNCLASSIFIED）
     + 牙②（窄掩码放宽成整帧 ignore ⇒ 保留字段差异被吞 ⇒ rc 0）。
 15. ident=100「@time 回显」的时间文本掩码（卡 openmir2-compare-time-noise-mask）：
     只差 `当前日期时间: YYYY-MM-DD HH:MM:SS` 的时间戳数字位 ⇒ 掩码生效 rc 0 且显式登记
     time_text_masked（不是静默 PASS）；时间位之外的真实差异（静态文案被改/明文长度变）
     ⇒ rc 1 且掩码不掩盖；掩码不越 ident（同形时间差在 658 仍 rc 1）；解码失败（非 EDCode
     线缆字节）⇒ 退回逐字节（fail-closed）；夹具与真机同形态（GBK 明文 → EDCode 线缆），
     并用真机同款 body_hex 钉住编码形态。
 15. ident=null 控制帧过滤（卡 openmir2-spell-cast-controlframe-filter）：真机 fixture
     （window-run-1 spell_cast，冻结副本 tools/conformance/fixtures/spell_cast/）两侧各
     2 条 +GD/server_ack_time 控制帧被输入层剔除并逐帧计数（oracle 行 31/34、rust 行 33/36），
     协议帧原样保留、整轮出 verdict；全控制帧流不崩溃（empty_run 兜底，--allow-empty 才放行）；
     牙：协议帧改字节仍 rc 1（过滤不吞真差异）、控制帧内容改坏不影响判定、ident 非法
     （负数/超 u16/字符串）与 frame_class=protocol 缺 ident 仍 fail-closed（过滤不外溢）。
 16. ident=600（SM_DROPITEM_SUCCESS）规则卡（openmir2-compare-ident600-registration）：
     pickup 链「丢→走→拾」的 600 等待帧登记为 action=compare 全字节比，并用
     head_compare:["recog"] 取回语义字段（Recog=MakeIndex，不是会话 ActorId）。冻结夹具
     tools/conformance/fixtures/dropitem_ack/（构造等价夹具，run2 未跑到 pickup）两侧 600
     帧 rc 0；牙：删 600 规则 ⇒ rc 2 UNCLASSIFIED(ident=600)、改坏 body 一字节 ⇒ rc 1 且
     定位 offset=0、改坏 recog ⇒ rc 1（证明全局 recog 掩码没吞掉 MakeIndex）。头部形态锚
     ground-probe 真机向量 mir2-rs/tests/parity/m3/fixtures/ground_vectors.json。
     夹具现含 600+601 两帧（601 见下条）。
 17. ident=601（SM_DROPITEM_FAIL）规则卡（openmir2-compare-ident601-registration）：
     600 同 send 点的失败支（PlayObject.Message.cs:808），同范式 action=compare 全字节比 +
     head_compare:["recog"]（Recog=MakeIndex）。冻结夹具 dropitem_ack/ 新增 601 case
     （真机向量 drop_fail_item_id_zero 的 request make_index=51/name="ring"，body=EDCode(ring)），
     两侧 601 帧 rc 0、compared=2；牙：删 601 规则 ⇒ rc 2 UNCLASSIFIED(ident=601，600 不被
     连带)、改坏 body 一字节 ⇒ rc 1 定位 offset=0/oracle 0x72 vs rust 0x73、改坏 recog ⇒
     rc 1 field=recog oracle=51/rust=52；600 的牙改为在含两帧的完整夹具上做，变异不牵连
     另一 ident。
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
                json.dump({"argv": ["synth", name, "--char-name", "winplay0"],
                           "started_at": "2026-01-01T00:00:00",
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
        self.t_struck31_rules()
        self.t_pending_review_scope()
        self.t_struck31_diff()
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
        self.t_time_text_mask()
        self.t_recog_unmask_rules()
        self.t_recog_unmask_diff()
        self.t_recog_unmask_actorid_abuse()
        self.t_updateitem_durachg_rules()
        self.t_updateitem_durachg_diff()
        self.t_updateitem_durachg_teeth()
        self.t_control_frames()
        self.t_dropitem_ack()
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
    @staticmethod
    def _approved_ok(meta):
        """approved_by 是否为本裁定的正式批准（卡 openmir2-rules-pending-adjudication）。

        断言必须同时看到：① 不是 PENDING；② coordinator 复核标记（日期）；
        ③ 裁定 decision oid；④ 保留原草稿人（draft: ...）。缺一条即视为未批准。
        """
        ab = str((meta or {}).get("approved_by") or "")
        return (ab.strip().upper().startswith("PENDING") is False
                and "coordinator review 2026-10-25" in ab
                and "decision=062972b6581e9fe4fd15370bdbbb4f8a4e3758ce" in ab
                and "draft:" in ab
                and ab.split("draft:", 1)[1].strip(" )").strip() != "")

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
            # 卡 openmir2-rules-pending-adjudication：这些 ident 已按 Leader 裁定转正
            # ⇒ 断言改瞄为「正式批准格式」（不是删掉 PENDING 检查，而是要求更多字段）。
            self.check("ident=%d 规则带 reason/tier 且 approved_by 为正式批准（coordinator + decision + 草稿人）" % ident,
                       all(str(meta.get(k, "")).strip()
                           for k in ("reason", "tier", "approved_by"))
                       and self._approved_ok(meta),
                       {k: meta.get(k) for k in ("tier", "approved_by")})
        # 裁定「保留 PENDING」的三条（10/14/50）：必须仍是 PENDING，且 reason 写明原因。
        for ident in (10, 14, 50):
            r = rs.by_ident.get(ident) or {}
            ab = str(r.get("approved_by") or "")
            reason = str(r.get("reason") or "")
            self.check("ident=%d 保留 PENDING（Leader 裁定 keep_pending）且 reason 写明原因" % ident,
                       ab.strip().upper().startswith("PENDING")
                       and len(reason.strip()) >= 20,
                       {"approved_by": ab[:40], "reason_len": len(reason)})

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
            self.check("ident=%d 带 reason/tier 且 approved_by 为正式批准（coordinator + decision + 草稿人）" % ident,
                       all(str(meta.get(k, "")).strip()
                           for k in ("reason", "tier", "approved_by"))
                       and self._approved_ok(meta),
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

    # ------------------------------------------------------------------
    # 批次 4（卡 openmir2-conformance-rules-recog-unmask）
    # 这 7 条 action=compare 但此前 head_mask/head_compare 都是 None，而 global
    # 第一条是 mask_fields:["recog"] ⇒ Recog 上的语义值根本没参与比较；
    # 213/766 无 body、其余头部全 0 ⇒ 该帧实际「什么都不比」。
    # 逐条定性（oracle 实参）：7/7 都是语义值、0 条 ActorId、0 条判不准。
    #   Recog 载荷与 oracle file:line 见 rules.json 各条 reason 的「Recog=」段。
    # ------------------------------------------------------------------
    RECOG_UNMASK = (
        # ident, Recog 载荷, oracle 值, rust 值（两侧故意不同）
        (54, "nMusicid/地图音乐号", 7, 8),
        (5008, "GameGold/元宝数量", 100, 200),
        (1106, "ClientFile1_CRC/版本CRC1", 0x1234, 0x5678),
        (752, "AntiMagic/魔法躲避", 3, 4),
        (811, "BonusPoint/加点数", 10, 11),
        (766, "n8/区域状态位", 0, 5),
        (213, "AttatckMode/攻击模式", 0, 2),
    )

    def _recog_pair(self, ident, body, recog_o, recog_r, name,
                    param_o=7, param_r=7, rules_path=None):
        heads = dict(phase="recog", seq=900, t_ms=1, conn="o",
                     param=param_o, tag=8, series=9)
        fo = self.frame(ident, body, **dict(heads, recog=recog_o))
        fr = self.frame(ident, body,
                        **dict(heads, seq=901, t_ms=2, conn="r",
                               param=param_r, recog=recog_r))
        return self._one_pair(name, fo, fr, rules_path=rules_path)

    def t_recog_unmask_rules(self):
        rs = self.api.RuleSet(self.builtin_rules, "builtin", "0" * 64)
        self.check("recog 批次前提：global 仍以 mask_fields 掩 recog（不掩就不需要本卡）",
                   "recog" in rs.global_masks, sorted(rs.global_masks))
        for ident, label, _, _ in self.RECOG_UNMASK:
            rule = rs.by_ident.get(ident) or {}
            self.check("ident=%d（%s）的 recog 已取回比较（head_compare=[recog]）"
                       % (ident, label), rule.get("head_compare") == ["recog"], rule)
            self.check("ident=%d 仍是 compare，且不另掩 body/其它头部（只取回 recog）" % ident,
                       rule.get("action") == "compare" and not rule.get("head_mask")
                       and not rule.get("ranges"), rule)
        self.check("反滥用不变式：ident=42（Recog=ActorId）不加 head_compare（会话作用域必须保持被掩）",
                   not (rs.by_ident.get(42) or {}).get("head_compare"),
                   rs.by_ident.get(42))

    def t_recog_unmask_diff(self):
        bodies = {54: b"\x0a", 5008: b"Gold\rPoint", 811: b"a/b/c"}
        # 牙①：一次性删掉这 7 条的 head_compare，作为「改前」对照（临时副本）
        pre = json.loads(json.dumps(self.builtin_rules))
        want = {ident for ident, _, _, _ in self.RECOG_UNMASK}
        for rule in pre["rules"]:
            if rule.get("ident") in want:
                rule.pop("head_compare", None)
        pre_path = self.rules_file("rules-recog-pre.json", pre)
        pre_rc = {}
        for ident, label, r_o, r_r in self.RECOG_UNMASK:
            body = bodies.get(ident, b"")
            # 改后：内置规则必须咬 recog
            rc, rep = self._recog_pair(ident, body, r_o, r_r, "ru-%d-after" % ident)
            self.check("ident=%d（%s）改后仅 recog 不同 => rc 1 且 field=recog"
                       % (ident, label),
                       rc == 1 and any(d.get("ident") == ident and d.get("field") == "recog"
                                       for d in (rep or {}).get("diffs", [])),
                       (rc, (rep or {}).get("diffs")))
            # 改前：同一对在删掉 head_compare 的副本上必须 rc 0（证明被全局掩码吞掉）
            rc0, rep0 = self._recog_pair(ident, body, r_o, r_r, "ru-%d-pre" % ident,
                                         rules_path=pre_path)
            pre_rc[ident] = rc0
            self.check("ident=%d（%s）改前（删 head_compare）同一对 => rc 0（recog 被吞）"
                       % (ident, label), rc0 == 0, (rc0, (rep0 or {}).get("diffs")))
            # 不外溢：只改 param（recog 相同）⇒ 改后仍 rc 1 field=param
            rc2, rep2 = self._recog_pair(ident, body, r_o, r_o, "ru-%d-param" % ident,
                                         param_r=8)
            self.check("ident=%d 取回 recog 后 param 仍逐值比（改 param => rc 1 field=param）"
                       % ident,
                       rc2 == 1 and any(d.get("ident") == ident and d.get("field") == "param"
                                        for d in (rep2 or {}).get("diffs", [])),
                       (rc2, (rep2 or {}).get("diffs")))
        self.check("牙①：删掉这 7 条的 head_compare 后，7 对 recog 差异全部 rc 0"
                   "（上面 7 条「改后 rc 1」断言会全红）",
                   all(v == 0 for v in pre_rc.values()), pre_rc)

    def t_recog_unmask_actorid_abuse(self):
        # 牙②：反例——把 head_compare:["recog"] 硬加到 ActorId 类（42 SM_USERNAME）。
        # 跨栈真实形态：C# HUtil32.Sequence() 实测 255025409 / Rust alloc_id 275
        #（docs/window-run-sheet-20261021.md:732）。这种取回会把必然差异判成 FAIL。
        abuse = json.loads(json.dumps(self.builtin_rules))
        for rule in abuse["rules"]:
            if rule.get("ident") == 42:
                rule["head_compare"] = ["recog"]
        abuse_path = self.rules_file("rules-recog-abuse-42.json", abuse)
        rc, rep = self._recog_pair(42, b"Hero", 255025409, 275, "ru-42-abuse",
                                   rules_path=abuse_path)
        self.check("牙②：给 ActorId 类（42）硬加 head_compare => 跨栈 recog 差异必红"
                   "（rc 1 field=recog）——所以不能滥用",
                   rc == 1 and any(d.get("ident") == 42 and d.get("field") == "recog"
                                   for d in (rep or {}).get("diffs", [])),
                   (rc, (rep or {}).get("diffs")))
        rc0, rep0 = self._recog_pair(42, b"Hero", 255025409, 275, "ru-42-real")
        self.check("对照：内置规则下同一对 ActorId 差异 => rc 0（全局 recog 掩码按设计生效）",
                   rc0 == 0, (rc0, (rep0 or {}).get("diffs")))

    # ------------------------------------------------------------------
    # 批次 3（卡 openmir2-conformance-rule-sm-struck-31；review 926d1b67 返工）
    # recog=受击者 ActorId（会话作用域 ⇒ 走全局 ActorId 掩码，I-1）
    # param=受击者当前 HP（伤害的结果）/ tag=MaxHP / series=本次伤害 nDmg
    # body=EDCode(MessageBodyWL{Feature,Status,攻击者,Mag?})：16B 明文 ⇒ 22B 线缆，
    # Tag1（攻击者 ActorId）的编码足迹 = 线缆 [10,16)（I-2，窄区间掩码）。
    # 未掩区（tag/series/body 其余字节）仍逐字节比。
    # ------------------------------------------------------------------
    STRUCK31_FEATURE = 0x0A0B0C0D
    STRUCK31_STATUS = 0x11223344
    STRUCK31_HITTER = 275

    def _struck31_body(self, feature=None, status=None, hitter=None, mag=0):
        """按 oracle 的 MessageBodyWL 布局造 22B 线缆 body（EDCode(16B 明文)）。"""
        wl = struct.pack("<iiii",
                         self.STRUCK31_FEATURE if feature is None else feature,
                         self.STRUCK31_STATUS if status is None else status,
                         self.STRUCK31_HITTER if hitter is None else hitter,
                         mag)
        return self.lc.encode(wl)

    def _struck31(self, side, seq, body=None, **over):
        heads = dict(recog=500, param=37, tag=40, series=12)
        heads.update(over)
        return self.frame(31, self._struck31_body() if body is None else body,
                          phase="combat", seq=seq,
                          t_ms=1000 if side == "o" else 1001,
                          conn="o" if side == "o" else "r", **heads)

    def t_pending_review_scope(self):
        """卡 `openmir2-rules-pending-adjudication` ④（Leader 裁定 2026-10-25）：

        `rules_pending_review` 是**保守主标志**——规则文件里只要还有任何 PENDING
        （含本卡保留的 10/14/50）就是 True。**为什么必须保守**：读者用这个标志判断
        「这份报告的判据是否全部审过」；若改成「本次涉及才 True」，一份藏着未审规则的
        报告会显示 False，正是要堵的假绿。本次影响面单列在 `pending_review_used`。

        * 只用已批准规则（520）⇒ 主标志 **True**（文件里仍有 10/14/50）+ `pending_review_used` 为空；
        * 出现保留 PENDING 的 ident（10）⇒ 主标志 True + `pending_review_used` 命中该条；
        * `pending_review`（全量清单）仍保留三条，供审计。
        """
        def mk(side, ident, body):
            # 全局掩码字段（t_ms/conn/seq）两侧给不同值 + 必填头部齐 ⇒ 两侧都缺字段的
            # UNDECIDABLE 不会掩盖本测试要看的 rules_pending_review 语义。
            return self.frame(ident, body, recog=1, param=2, tag=3, series=0,
                              t_ms=1000 if side == "o" else 1001,
                              conn="o" if side == "o" else "r", seq=1,
                              phase="gamegate")

        od = self.write_run("t-pr-scope-o", [mk("o", 520, b"1/2")])
        rd = self.write_run("t-pr-scope-r", [mk("r", 520, b"1/2")])
        rc, rep = self.compare(od, rd)
        rs = (rep or {}).get("rules") or {}
        # Leader 裁定：主标志是**保守闸门**——文件里还有 10/14/50 PENDING ⇒ 恒 true；
        # 「本次影响面」降到 pending_review_used（520 不涉及任何 PENDING => 空）。
        self.check("只碰已批准规则（520）=> rules_pending_review 仍为 True（保守闸门）"
                   "且 pending_review_used 为空",
                   rc == 0 and rs.get("rules_pending_review") is True
                   and (rs.get("pending_review_used") or []) == [],
                   (rc, rs.get("rules_pending_review"), rs.get("pending_review_used")))
        self.check("全量 pending_review 清单仍保留 10/14/50（审计用，不受本次涉及与否影响）",
                   {10, 14, 50} == {int(x.split("ident=[")[1].rstrip("]"))
                                    for x in (rs.get("pending_review") or [])
                                    if "ident=[" in x},
                   rs.get("pending_review"))
        od2 = self.write_run("t-pr-scope2-o", [mk("o", 10, b"")])
        rd2 = self.write_run("t-pr-scope2-r", [mk("r", 10, b"")])
        rc2, rep2 = self.compare(od2, rd2)
        rs2 = (rep2 or {}).get("rules") or {}
        self.check("出现保留 PENDING 的 ident=10 => 主标志 True 且 used 命中该条",
                   rc2 == 0 and rs2.get("rules_pending_review") is True
                   and any("ident=[10]" in x for x in (rs2.get("pending_review_used") or [])),
                   (rc2, rs2.get("rules_pending_review"),
                    rs2.get("pending_review_used")))

    def t_struck31_rules(self):
        rs = self.api.RuleSet(self.builtin_rules, "builtin", "0" * 64)
        rule = rs.by_ident.get(31)
        self.check("SM_STRUCK(31) 已登记且 action=mask_body_ranges（头部窄掩 + body 窄区间）",
                   rule is not None and rule["action"] == "mask_body_ranges",
                   rule and rule.get("action"))
        rule = rule or {}
        self.check("31 的头部掩码只有 param(HP)，且不再 head_compare 取回 recog（review I-1）",
                   rule.get("head_mask") == ["param"] and not rule.get("head_compare"),
                   (rule.get("head_mask"), rule.get("head_compare")))
        self.check("31 的 body 窄区间 = [[10,16)]（线缆空间，只覆盖 Tag1=攻击者 ActorId 足迹）",
                   rule.get("ranges") == [[10, 16]], rule.get("ranges"))
        self.check("31 的 reason/tier 齐全且 approved_by 为正式批准（coordinator + decision + 草稿人）",
                   all(str(rule.get(k, "")).strip() for k in ("reason", "tier", "approved_by"))
                   and self._approved_ok(rule),
                   {k: rule.get(k) for k in ("tier", "approved_by")})
        masks = rs.mask_for(31)
        self.check("31 的有效头部掩码 = {param + 全局 recog/t_ms/conn/seq}（tag/series 不掩）",
                   set(masks) == {"param", "recog", "t_ms", "conn", "seq"},
                   sorted(masks))
        reason = rule.get("reason") or ""
        self.check("31 的 reason 写明 recog 走全局 ActorId 口径 / param 在 series 已比时冗余 / Tag1 区间",
                   "全局 ActorId 口径" in reason and "受击前 HP" in reason
                   and "Tag1" in reason and "线缆" in reason, reason[:200])

    def t_struck31_diff(self):
        # ① 仅 param(HP) 不同 => rc 0，差异显式登记 masked_head(param)
        rc, rep = self._one_pair("31-hp", self._struck31("o", 840),
                                 self._struck31("r", 841, param=29))
        self.check("31 仅 param(HP) 不同 => rc 0 且登记 masked_head(param)",
                   rc == 0 and any(w.get("ident") == 31 and w.get("field") == "param"
                                   and w.get("kind") == "masked_head"
                                   for w in (rep or {}).get("whitelisted", [])),
                   (rc, (rep or {}).get("whitelisted"), (rep or {}).get("diffs")))
        # ② tag(MaxHP) 不同 => rc 1
        rc, rep = self._one_pair("31-maxhp", self._struck31("o", 842),
                                 self._struck31("r", 843, tag=41))
        self.check("31 的 tag(MaxHP) 不同 => rc 1 且定位 field=tag",
                   rc == 1 and any(d.get("ident") == 31 and d.get("field") == "tag"
                                   for d in (rep or {}).get("diffs", [])),
                   (rc, (rep or {}).get("diffs")))
        # ③ recog(受击者 ActorId) 不同 => rc 0 + masked_head(recog)：全局 ActorId 口径（review I-1）
        rc, rep = self._one_pair("31-actor", self._struck31("o", 844, recog=255025409),
                                 self._struck31("r", 845, recog=275))
        self.check("31 的 recog(会话 ActorId) 不同 => rc 0 且登记 masked_head(recog)（全局口径）",
                   rc == 0 and any(w.get("ident") == 31 and w.get("field") == "recog"
                                   and w.get("kind") == "masked_head"
                                   for w in (rep or {}).get("whitelisted", [])),
                   (rc, (rep or {}).get("whitelisted"), (rep or {}).get("diffs")))
        # ④ series(nDmg) 不同 => rc 1
        rc, rep = self._one_pair("31-dmg", self._struck31("o", 846),
                                 self._struck31("r", 847, series=13))
        self.check("31 的 series(本次伤害) 不同 => rc 1 且定位 field=series",
                   rc == 1 and any(d.get("ident") == 31 and d.get("field") == "series"
                                   for d in (rep or {}).get("diffs", [])),
                   (rc, (rep or {}).get("diffs")))
        # ⑤ body 的 Tag1(攻击者 ActorId) 不同 => rc 0 + mask_body_ranges（首处偏移落在 [10,16)）
        rc, rep = self._one_pair(
            "31-body-tag1", self._struck31("o", 848),
            self._struck31("r", 849,
                           body=self._struck31_body(hitter=self.STRUCK31_HITTER + 1)))
        body_mask = [w for w in (rep or {}).get("whitelisted", [])
                     if w.get("ident") == 31 and w.get("kind") == "mask_body_ranges"]
        self.check("31 的 body Tag1(攻击者 ActorId) 不同 => rc 0 且登记 mask_body_ranges"
                   "（first_offset 在 [10,16)）",
                   rc == 0 and bool(body_mask)
                   and 10 <= (body_mask[0].get("first_offset") or -1) < 16,
                   (rc, body_mask, (rep or {}).get("diffs")))
        # ⑥ body 未掩区 Param1(Feature) 不同 => rc 1 offset=0（窄区间不外溢）
        rc, rep = self._one_pair(
            "31-body-feature", self._struck31("o", 850),
            self._struck31("r", 851,
                           body=self._struck31_body(feature=self.STRUCK31_FEATURE + 1)))
        self.check("31 的 body Param1(Feature) 不同 => rc 1 offset=0（掩码不外溢）",
                   rc == 1 and any(d.get("ident") == 31 and d.get("offset") == 0
                                   for d in (rep or {}).get("diffs", [])),
                   (rc, (rep or {}).get("diffs")))
        # ⑦ body 未掩区 Tag2(Mag) 不同 => rc 1 offset=16
        rc, rep = self._one_pair("31-body-mag", self._struck31("o", 852),
                                 self._struck31("r", 853,
                                                body=self._struck31_body(mag=1)))
        self.check("31 的 body Tag2(Mag) 不同 => rc 1 offset=16（掩码不外溢）",
                   rc == 1 and any(d.get("ident") == 31 and d.get("offset") == 16
                                   for d in (rep or {}).get("diffs", [])),
                   (rc, (rep or {}).get("diffs")))
        # ⑧ 已知过掩边界：CharStatus 明文[6] bits2,3 与 Tag1[8] 共用线缆[11] ⇒ 被掩（rc 0）
        rc, rep = self._one_pair(
            "31-body-status-over", self._struck31("o", 854),
            self._struck31("r", 855,
                           body=self._struck31_body(status=self.STRUCK31_STATUS ^ 0x00040000)))
        body_mask2 = [w for w in (rep or {}).get("whitelisted", [])
                      if w.get("ident") == 31 and w.get("kind") == "mask_body_ranges"]
        self.check("31 的 CharStatus 明文[6] bits2,3（与 Tag1 共用线缆[11]）=> rc 0 + first_offset=11"
                   "（reason 已登记的过掩边界）",
                   rc == 0 and bool(body_mask2) and body_mask2[0].get("first_offset") == 11,
                   (rc, body_mask2, (rep or {}).get("diffs")))
        # ⑨ 过掩不外溢：CharStatus 明文[4] bit0 ⇒ 线缆[5] 未被掩，必须 rc 1
        rc, rep = self._one_pair(
            "31-body-status", self._struck31("o", 856),
            self._struck31("r", 857,
                           body=self._struck31_body(status=self.STRUCK31_STATUS ^ 0x1)))
        self.check("31 的 CharStatus 明文[4] bit0（线缆[5]，未掩）=> rc 1 offset=5",
                   rc == 1 and any(d.get("ident") == 31 and d.get("offset") == 5
                                   for d in (rep or {}).get("diffs", [])),
                   (rc, (rep or {}).get("diffs")))
        # ⑩ param 的掩码不吞 series：两者同时不同 ⇒ rc 1 field=series（review M-3 的冗余论证）
        rc, rep = self._one_pair("31-param-series", self._struck31("o", 858),
                                 self._struck31("r", 859, param=29, series=13))
        self.check("31 param+series 同时不同 => rc 1 field=series（param 掩码不吞 series）",
                   rc == 1 and any(d.get("ident") == 31 and d.get("field") == "series"
                                   for d in (rep or {}).get("diffs", [])),
                   (rc, (rep or {}).get("diffs")))

        # 牙①：去掉 ident=31 规则 => 用例① 必须变 rc 2（UNCLASSIFIED）
        no31 = json.loads(json.dumps(self.builtin_rules))
        no31["rules"] = [r for r in no31["rules"] if r.get("ident") != 31]
        path_no31 = self.rules_file("rules-no31.json", no31)
        rc, rep = self._one_pair("31-tooth-norule", self._struck31("o", 860),
                                 self._struck31("r", 861, param=29),
                                 rules_path=path_no31)
        self.check("牙①：去掉 31 规则 => 用例① 变 rc 2 且 coverage.unclassified_idents=[31]",
                   rc == 2 and 31 in ((rep or {}).get("coverage") or {})
                   .get("unclassified_idents", []),
                   (rc, (rep or {}).get("coverage"), (rep or {}).get("undecidable")))

        # 牙②：放宽成整帧 ignore（temp 副本，approved_by 非 PENDING）=> tag 差异被吞成 rc 0
        ign = json.loads(json.dumps(self.builtin_rules))
        for r in ign["rules"]:
            if r.get("ident") == 31:
                for k in ("head_mask", "head_compare", "ranges"):
                    r.pop(k, None)
                r["action"] = "ignore"
                r["approved_by"] = "selftest tooth: approved (non-pending)"
                r["reason"] = "selftest 变异：窄掩码 → 整帧 ignore（证明 ignore 会吞掉 tag 差异）"
        path_ign = self.rules_file("rules-31-ignore.json", ign)

        def tail41(side, seq):
            return self.frame(41, b"", phase="combat", seq=seq,
                              t_ms=1000 if side == "o" else 1001,
                              conn="o" if side == "o" else "r",
                              recog=500, param=0, tag=0, series=0)

        od = self.write_run("t22-o-31-tooth-ignore",
                            [self._struck31("o", 862), tail41("o", 864)])
        rd = self.write_run("t22-r-31-tooth-ignore",
                            [self._struck31("r", 863, tag=41), tail41("r", 865)])
        rc, rep = self.compare(od, rd, ["--rules", path_ign])
        ign_report = ((rep or {}).get("frames") or {}).get("ignored") or {}
        self.check("牙②：31 放宽成整帧 ignore => 用例②(tag) 被吞成 rc 0（两侧各剔 1 帧）",
                   rc == 0
                   and (ign_report.get("oracle") or {}).get("count") == 1
                   and (ign_report.get("rust") or {}).get("count") == 1
                   and not any(d.get("ident") == 31 for d in (rep or {}).get("diffs", [])),
                   (rc, ign_report, (rep or {}).get("diffs"),
                    (rep or {}).get("undecidable")))

        # 牙③（review I-1 的正向牙）：把 recog 重新加回 head_compare ⇒ 同一对「仅 recog 不同」的帧
        # 由 rc 0（真规则：掩码生效）变 rc 1 field=recog。真规则下 tag/series/body 未掩区仍 rc 1
        #（见 ②/④/⑥/⑦），证明 rc 0 来自掩码生效，而不是比较器失明。
        recog_cmp = json.loads(json.dumps(self.builtin_rules))
        for r in recog_cmp["rules"]:
            if r.get("ident") == 31:
                r["head_compare"] = ["recog"]
        path_recog = self.rules_file("rules-31-headcompare-recog.json", recog_cmp)
        rc, rep = self._one_pair("31-tooth-recog",
                                 self._struck31("o", 866, recog=255025409),
                                 self._struck31("r", 867, recog=275),
                                 rules_path=path_recog)
        self.check("牙③：把 recog 加回 head_compare => 同一对变 rc 1 field=recog"
                   "（对照：真规则下为 rc 0 + masked_head(recog)）",
                   rc == 1 and any(d.get("ident") == 31 and d.get("field") == "recog"
                                   for d in (rep or {}).get("diffs", [])),
                   (rc, (rep or {}).get("diffs")))

        # 牙④：把 body ranges 放宽成 [[0,null]]（整段掩）=> Feature 差异被吞成 rc 0
        wide = json.loads(json.dumps(self.builtin_rules))
        for r in wide["rules"]:
            if r.get("ident") == 31:
                r["ranges"] = [[0, None]]
        path_wide = self.rules_file("rules-31-wide-ranges.json", wide)
        rc, rep = self._one_pair(
            "31-tooth-wide", self._struck31("o", 868),
            self._struck31("r", 869,
                           body=self._struck31_body(feature=self.STRUCK31_FEATURE + 1)),
            rules_path=path_wide)
        self.check("牙④：body ranges 放宽成 [[0,null]] => Feature 差异被吞成 rc 0（窄区间不外溢）",
                   rc == 0 and any(w.get("ident") == 31 and w.get("kind") == "mask_body_ranges"
                                   for w in (rep or {}).get("whitelisted", [])),
                   (rc, (rep or {}).get("whitelisted"), (rep or {}).get("diffs")))

    # ------------------------------------------------------------------
    # 批次 5（卡 openmir2-conformance-rules-updateitem-durachg）
    # 202 SM_DELITEM / 203 SM_UPDATEITEM：与 200/201 同构的单条 124B ClientItem
    #   （Recog=ActorId 走全局掩码；Param/Tag=0；Series=1），只掩 MakeIndex@100。
    # 642 SM_DURACHANGE：Recog=Dura（伤害/RNG 扣除，跨进程不可复现）/ Param=槽 /
    #   Tag,Series=DuraMax 低高 ⇒ head_mask:[recog]，其余三字段逐值比。
    # 709 SM_DELITEMS：Recog=Param=Tag=0、Series=条数、body=「名/makeindex/」文本
    #   ⇒ 不加掩码，全字段逐字节比（含 MakeIndex：删除清单来自 DB 持久物品）。
    # ------------------------------------------------------------------
    BAG_ITEM_IDENTS = (202, 203)

    def t_updateitem_durachg_rules(self):
        rs = self.api.RuleSet(self.builtin_rules, "builtin", "0" * 64)
        for ident in self.BAG_ITEM_IDENTS:
            rule = rs.by_ident.get(ident) or {}
            self.check("ident=%d 已登记为 bagitems_records（单条 124B ClientItem）" % ident,
                       rule.get("action") == "bagitems_records", rule)
            self.check("ident=%d 只掩 MakeIndex@100（record_mask=[MakeIndex]）" % ident,
                       rule.get("record_mask") == ["MakeIndex"], rule)
        r642 = rs.by_ident.get(642) or {}
        self.check("ident=642 用 compare + head_mask=[recog]（Dura 窄掩）",
                   r642.get("action") == "compare" and r642.get("head_mask") == ["recog"],
                   r642)
        self.check("ident=642 不掩 param/tag/series（槽位与 DuraMax 保留比较）",
                   not r642.get("head_compare") and not r642.get("ranges"), r642)
        r709 = rs.by_ident.get(709) or {}
        self.check("ident=709 用 compare 且不加任何掩码（名称/条数/MakeIndex 全比）",
                   r709.get("action") == "compare" and not r709.get("head_mask")
                   and not r709.get("head_compare") and not r709.get("ranges"), r709)

    def t_updateitem_durachg_diff(self):
        heads = dict(phase="bag", seq=800, t_ms=1, conn="o", recog=5, param=0, tag=0,
                     series=1)
        # 202/203：单条记录；MakeIndex 掩 => rc0；Name / series 保留 => rc1
        for ident in self.BAG_ITEM_IDENTS:
            rec_a = self._client_item(make_index=1000)
            rec_b = self._client_item(make_index=2000)
            rc, rep = self._one_pair("bp-%d-mk" % ident,
                                     self.frame(ident, self.lc.encode(rec_a), **heads),
                                     self.frame(ident, self.lc.encode(rec_b),
                                                **dict(heads, seq=801, conn="r")))
            self.check("ident=%d 只有 MakeIndex 不同 => rc 0 且登记 bagitems_masked_field"
                       % ident,
                       rc == 0 and any(w.get("ident") == ident
                                       and w.get("kind") == "bagitems_masked_field"
                                       for w in (rep or {}).get("whitelisted", [])),
                       (rc, (rep or {}).get("whitelisted"), (rep or {}).get("diffs")))
            rec_name = self._client_item(name=b"\xcd\xc0\xc1\xfb", make_index=1000)
            rc, rep = self._one_pair("bp-%d-name" % ident,
                                     self.frame(ident, self.lc.encode(rec_a), **heads),
                                     self.frame(ident, self.lc.encode(rec_name),
                                                **dict(heads, seq=801, conn="r")))
            self.check("ident=%d Name 改一个字节 => rc 1（MakeIndex 掩码不外溢）" % ident,
                       rc == 1 and any(d.get("ident") == ident
                                       and d.get("kind") == "bagitems_record"
                                       for d in (rep or {}).get("diffs", [])),
                       (rc, (rep or {}).get("diffs")))
            rc, rep = self._one_pair("bp-%d-series" % ident,
                                     self.frame(ident, self.lc.encode(rec_a), **heads),
                                     self.frame(ident, self.lc.encode(rec_a),
                                                **dict(heads, seq=801, conn="r", series=2)))
            self.check("ident=%d series(常量 1) 不同 => rc 1 field=series" % ident,
                       rc == 1 and any(d.get("ident") == ident and d.get("field") == "series"
                                       for d in (rep or {}).get("diffs", [])),
                       (rc, (rep or {}).get("diffs")))

        # 642：recog(Dura) 掩 => rc0 + masked_head；param/tag/series 保留 => rc1
        h642 = dict(phase="dur", seq=810, t_ms=1, conn="o",
                    recog=1000, param=1, tag=500, series=0)
        rc, rep = self._one_pair("642-recog",
                                 self.frame(642, b"", **h642),
                                 self.frame(642, b"", **dict(h642, seq=811, conn="r",
                                                             recog=900)))
        self.check("ident=642 Dura(recog) 不同 => rc 0 且登记 masked_head",
                   rc == 0 and any(w.get("ident") == 642 and w.get("field") == "recog"
                                   and w.get("kind") == "masked_head"
                                   for w in (rep or {}).get("whitelisted", [])),
                   (rc, (rep or {}).get("whitelisted"), (rep or {}).get("diffs")))
        for field, val in (("param", 2), ("tag", 501), ("series", 1)):
            rc, rep = self._one_pair("642-%s" % field,
                                     self.frame(642, b"", **h642),
                                     self.frame(642, b"", **dict(h642, seq=811, conn="r",
                                                                 **{field: val})))
            self.check("ident=642 %s(保留字段) 不同 => rc 1 field=%s" % (field, field),
                       rc == 1 and any(d.get("ident") == 642 and d.get("field") == field
                                       for d in (rep or {}).get("diffs", [])),
                       (rc, (rep or {}).get("diffs")))

        # 709：无掩码 => 全字节比（条数/名称/MakeIndex 都要咬）
        body_a = self.lc.encode(b"BlackIron/70000001/")
        h709 = dict(phase="del", seq=820, t_ms=1, conn="o",
                    recog=0, param=0, tag=0, series=1)
        rc, rep = self._one_pair("709-same", self.frame(709, body_a, **h709),
                                 self.frame(709, body_a, **dict(h709, seq=821, conn="r")))
        self.check("ident=709 两侧相同 => rc 0", rc == 0, (rc, (rep or {}).get("diffs")))
        rc, rep = self._one_pair("709-series", self.frame(709, body_a, **h709),
                                 self.frame(709, body_a, **dict(h709, seq=821, conn="r",
                                                                series=2)))
        self.check("ident=709 Series(条数) 不同 => rc 1 field=series",
                   rc == 1 and any(d.get("ident") == 709 and d.get("field") == "series"
                                   for d in (rep or {}).get("diffs", [])),
                   (rc, (rep or {}).get("diffs")))
        rc, rep = self._one_pair("709-name", self.frame(709, body_a, **h709),
                                 self.frame(709, self.lc.encode(b"BlackIrox/70000001/"),
                                            **dict(h709, seq=821, conn="r")))
        self.check("ident=709 名称串改一字节 => rc 1（body 逐字节比）",
                   rc == 1 and any(d.get("ident") == 709 and d.get("field") == "body"
                                   for d in (rep or {}).get("diffs", [])),
                   (rc, (rep or {}).get("diffs")))
        rc, rep = self._one_pair("709-mi",
                                 self.frame(709, body_a, **h709),
                                 self.frame(709, self.lc.encode(b"BlackIron/70000002/"),
                                            **dict(h709, seq=821, conn="r")))
        self.check("ident=709 MakeIndex 也不同 => rc 1（本规则有意不放宽）",
                   rc == 1 and any(d.get("ident") == 709 and d.get("field") == "body"
                                   for d in (rep or {}).get("diffs", [])),
                   (rc, (rep or {}).get("diffs")))

    def t_updateitem_durachg_teeth(self):
        h41 = dict(phase="hb", seq=830, t_ms=1, conn="o",
                   recog=0, param=0, tag=0, series=0)
        h642 = dict(phase="dur", seq=831, t_ms=2, conn="o",
                    recog=1000, param=1, tag=500, series=0)
        # 牙①：删掉 642 规则 => 含该帧的整轮 UNDECIDABLE（UNCLASSIFIED，rc=2）
        no642 = json.loads(json.dumps(self.builtin_rules))
        no642["rules"] = [r for r in no642["rules"] if r.get("ident") != 642]
        no642_path = self.rules_file("rules-no-642.json", no642)
        od = self.write_run("t5-no642-o", [self.frame(41, b"", **h41),
                                           self.frame(642, b"", **h642)])
        rd = self.write_run("t5-no642-r", [self.frame(41, b"", **dict(h41, seq=840, conn="r")),
                                           self.frame(642, b"", **dict(h642, seq=841, conn="r"))])
        rc, rep = self.compare(od, rd, ["--rules", no642_path])
        self.check("牙①：删掉 642 规则 => 含该帧的整轮 rc 2（UNCLASSIFIED，窗口必 UNDECIDABLE）",
                   rc == 2 and any(u.get("kind") == "unclassified_ident"
                                   and u.get("ident") == 642
                                   for u in (rep or {}).get("undecidable", [])),
                   (rc, (rep or {}).get("undecidable")))
        # 牙②：把窄掩码放宽成整帧 ignore => 保留字段差异被吞 => rc 0
        wide = json.loads(json.dumps(self.builtin_rules))
        for r in wide["rules"]:
            if r.get("ident") in (642, 203):
                r["action"] = "ignore"
                r.pop("head_mask", None)
                r.pop("record_mask", None)
                r["approved_by"] = "selftest: approved (non-pending)"
                r["reason"] = "selftest 变异：窄掩码 → 整帧 ignore（证明保留字段被判据吞掉）"
        wide_path = self.rules_file("rules-wide-ignore.json", wide)
        od = self.write_run("t5-wide642-o", [self.frame(41, b"", **h41),
                                             self.frame(642, b"", **h642)])
        rd = self.write_run("t5-wide642-r", [self.frame(41, b"", **dict(h41, seq=840, conn="r")),
                                             self.frame(642, b"", **dict(h642, seq=841, conn="r",
                                                                         param=9))])
        rc, rep = self.compare(od, rd, ["--rules", wide_path])
        self.check("牙②：642 放宽成整帧 ignore => 槽位(param) 差异被吞 => rc 0（真规则 rc 1）",
                   rc == 0, (rc, (rep or {}).get("diffs")))
        rec_a = self._client_item(make_index=1000)
        rec_name = self._client_item(name=b"\xcd\xc0\xc1\xfb", make_index=1000)
        h203 = dict(phase="bag", seq=850, t_ms=1, conn="o",
                    recog=5, param=0, tag=0, series=1)
        od = self.write_run("t5-wide203-o", [self.frame(41, b"", **h41),
                                             self.frame(203, self.lc.encode(rec_a), **h203)])
        rd = self.write_run("t5-wide203-r", [self.frame(41, b"", **dict(h41, seq=840, conn="r")),
                                             self.frame(203, self.lc.encode(rec_name),
                                                        **dict(h203, seq=851, conn="r"))])
        rc, rep = self.compare(od, rd, ["--rules", wide_path])
        self.check("牙②：203 放宽成整帧 ignore => Name 差异被吞 => rc 0（真规则 rc 1）",
                   rc == 0, (rc, (rep or {}).get("diffs")))

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
        # 卡 openmir2-ignore-rule-count-compare：PENDING ignore 的兜底从「一剔就整轮
        # UNDECIDABLE」升级成「每侧每条规则计数 + 形态」——两侧同量**且**同形才记为
        # 「已计数」放行；计数或形态对不上仍红，并点名差额（保护机制没有取消，只是升级）。
        def pending_rules(name="t17-rules.json"):
            rules = json.loads(json.dumps(self.builtin_rules))
            rules["rules"] = [x for x in rules["rules"] if x.get("ident") != 658]
            rules["rules"].append({"ident": 658, "action": "ignore",
                                   "reason": "selftest: PENDING 的 ignore 需要两侧同量同形才放行",
                                   "tier": "L", "approved_by": "PENDING selftest"})
            return self.rules_file(name, rules)

        rp = pending_rules()
        od = self.write_run("t17-o", o)
        rd = self.write_run("t17-r", r)
        rc, rep = self.compare(od, rd, ["--rules", rp])
        frames_rep = (rep or {}).get("frames") or {}
        row = [x for x in (frames_rep.get("ignored_rule_census") or [])
               if x.get("ident") == 658]
        self.check("牙a 两侧各 1 帧 PENDING ignore 且计数/形态相同 => rc 0 且记为已计数",
                   rc == 0 and bool(row) and row[0]["counts_equal"] and row[0]["shape_equal"]
                   and frames_rep.get("ignored_ident_rules_counted") == [658]
                   and (frames_rep.get("ignored_counts") or {}).get("oracle", {})
                   .get("658") == 1
                   and (frames_rep.get("ignored_counts") or {}).get("rust", {})
                   .get("658") == 1
                   and not any(u.get("kind") == "pending_ignore_rule"
                               for u in (rep or {}).get("undecidable", [])),
                   (rc, frames_rep.get("ignored_rule_census"),
                    (rep or {}).get("undecidable")))

        f658 = [fr for fr in o if fr["ident"] == 658][0]
        od2 = self.write_run("t17-o2", o + [dict(f658, seq=2099, t_ms=990099)])
        rc, rep = self.compare(od2, rd, ["--rules", rp])
        self.check("牙b oracle 侧多 1 帧同 ident => rc 2 且点名 oracle=2/rust=1 差 +1",
                   rc == 2 and any(u.get("kind") == "pending_ignore_rule"
                                   and u.get("ident") == 658
                                   and u.get("oracle") == 2 and u.get("rust") == 1
                                   and u.get("delta") == 1
                                   for u in (rep or {}).get("undecidable", [])),
                   (rc, (rep or {}).get("undecidable")))

        rd3 = self.write_run("t17-r3", [fr for fr in r if fr["ident"] != 658])
        rc, rep = self.compare(od, rd3, ["--rules", rp])
        # F3（reviewer-20）：单侧命中时另一侧也要**有键补 0**，否则读 ignored_counts 会 KeyError。
        counts3 = ((rep or {}).get("frames") or {}).get("ignored_counts") or {}
        self.check("牙c rust 侧 0 帧 => rc 2 且点名 oracle=1/rust=0 差 +1（不许静默放过）；"
                   "ignored_counts 两侧都有键（rust 侧补 0）",
                   rc == 2 and any(u.get("kind") == "pending_ignore_rule"
                                   and u.get("ident") == 658
                                   and u.get("oracle") == 1 and u.get("rust") == 0
                                   and u.get("delta") == 1
                                   for u in (rep or {}).get("undecidable", []))
                   and counts3.get("oracle", {}).get("658") == 1
                   and counts3.get("rust", {}).get("658") == 0,
                   (rc, (rep or {}).get("undecidable"), counts3))

        o4 = [dict(fr, param=9) if fr["ident"] == 658 else fr for fr in o]
        od4 = self.write_run("t17-o4", o4)
        rc, rep = self.compare(od4, rd, ["--rules", rp])
        mism = [u for u in (rep or {}).get("undecidable", [])
                if u.get("kind") == "pending_ignore_rule"]
        self.check("牙d 计数相等但形态不同（param）=> rc 2 且 shape_equal=false",
                   rc == 2 and bool(mism) and mism[0].get("counts_equal") is True
                   and mism[0].get("shape_equal") is False
                   and mism[0].get("shapes", {}).get("oracle")
                   and mism[0].get("shapes", {}).get("rust"),
                   (rc, mism))

        # 牙e（reviewer-20 F6）：两侧各 3 帧、总数相同、**分布互换** {1×2,2×1} vs {1×1,2×2}
        #   —— 只比 distinct 集合（set）会放过，多重集必须红。
        base658 = [fr for fr in o if fr["ident"] == 658][0]

        def three(params, seq0):
            return [dict(base658, param=p, seq=seq0 + i, t_ms=100000 + i)
                    for i, p in enumerate(params)]

        o5 = [fr for fr in o if fr["ident"] != 658] + three([1, 1, 2], 3001)
        r5 = [fr for fr in r if fr["ident"] != 658] + three([1, 2, 2], 3101)
        od5 = self.write_run("t17-o5", o5)
        rd5 = self.write_run("t17-r5", r5)
        rc, rep = self.compare(od5, rd5, ["--rules", rp])
        mism5 = [u for u in (rep or {}).get("undecidable", [])
                 if u.get("kind") == "pending_ignore_rule"]
        self.check("牙e 两侧同 3 帧但形态分布互换（多重集 vs 集合）=> rc 2 且 shape_equal=false",
                   rc == 2 and bool(mism5) and mism5[0].get("counts_equal") is True
                   and mism5[0].get("shape_equal") is False,
                   (rc, mism5))

        # 牙f（reviewer-20 F2）：计数/头部形态相同但 **body 不同** => 必须红
        #   （只在 head 上比形态会漏掉这一整类）。
        o6 = [dict(fr, body_hex=b"AAA".hex(), body_len=3,
                   body_sha256=_sha(b"AAA")) if fr["ident"] == 658 else fr
              for fr in o]
        od6 = self.write_run("t17-o6", o6)
        rc, rep = self.compare(od6, rd, ["--rules", rp])
        mism6 = [u for u in (rep or {}).get("undecidable", [])
                 if u.get("kind") == "pending_ignore_rule"]
        self.check("牙f 计数与头部形态相同但 body 不同 => rc 2（形态含 body_sha256）",
                   rc == 2 and bool(mism6) and mism6[0].get("counts_equal") is True
                   and mism6[0].get("shape_equal") is False,
                   (rc, mism6))
        # F7（follow-up）：报告与报错串必须**如实**列出形态含 body_sha256，
        # 否则「体不同」会被读成「param/tag/series 不同」（而三项完全相同）。
        self.check("牙g 形态字段如实报 body_sha256（shape_fields 含 body_sha256、"
                   "undecidable.detail 也点名）",
                   bool(mism6)
                   and "body_sha256" in (mism6[0].get("shape_fields") or [])
                   and "body_sha256" in (mism6[0].get("detail") or ""),
                   (mism6[:1],))

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

    # -- ident=100「@time 回显」的时间文本掩码（卡 openmir2-compare-time-noise-mask）--
    #: 真机 @time 回显的线缆 body（E:/tmp/window/run1/gm_time idx30 csharp，
    #: 明文 = `当前日期时间: 2026-10-24 13:46:43` GBK → EDCode）。
    REAL_TIME_WIRE = ("4141675740547559565a625541404458625c5a636062566b5d61585f5d"
                      "62545f5c615b6b6260566262605b62")

    def enc_gbk_wire(self, text):
        m2f = self.api.load_module("mir2frame", self.api.MIR2FRAME)
        return m2f.encode_bytes(text.encode("gbk"))

    def t_time_text_mask(self):
        m2f = self.api.load_module("mir2frame", self.api.MIR2FRAME)
        ts_o = self.enc_gbk_wire("当前日期时间: 2026-10-24 13:46:43")
        ts_r = self.enc_gbk_wire("当前日期时间: 2026-10-24 13:47:57")
        plain_o = "当前日期时间: 2026-10-24 13:46:43".encode("gbk")
        self.check("time_mask ⓪ 夹具往返：GBK 明文 EDCode 编解码无损（等长 33B 明文）",
                   m2f.decode_bytes(ts_o, strict=True) == plain_o
                   and len(plain_o) == 33, plain_o)
        self.check("time_mask ⓪ 夹具与真机 @time 回显同一编码形态（逐字节同 hex）",
                   ts_o.hex() == self.REAL_TIME_WIRE, ts_o.hex())

        def run_case(tag, ident, obody, rbody, extra=()):
            of = [self.frame(ident, obody, phase="gamegate", seq=1, conn="o",
                             t_ms=1000, recog=5555, param=64763, tag=0, series=1)]
            rf = [self.frame(ident, rbody, phase="gamegate", seq=2, conn="r",
                             t_ms=2000, recog=6666, param=64763, tag=0, series=1)]
            od = self.write_run("t25-%s-o" % tag, of)
            rd = self.write_run("t25-%s-r" % tag, rf)
            return self.compare(od, rd, list(extra))

        # ① 有时间差（时分秒位，真机 gm_time/gm_batch 的噪声形态）⇒ 掩码生效 rc 0
        rc, rep = run_case("timediff", 100, ts_o, ts_r)
        wl = [w for w in (rep or {}).get("whitelisted", [])
              if w.get("kind") == "time_text_masked"]
        mr = [m for m in (rep or {}).get("masked_ranges", [])
              if m.get("space") == "plain_time_text"]
        self.check("time_mask ① 只差时间文本位 => rc 0（掩码生效）", rc == 0,
                   (rc, (rep or {}).get("diffs"), (rep or {}).get("undecidable")))
        self.check("time_mask ① 显式登记 time_text_masked：diff_bytes=3、"
                   "明文时间区间 [[14,33]]（不是静默 PASS）",
                   len(wl) == 1 and wl[0].get("ident") == 100
                   and wl[0].get("diff_bytes") == 3
                   and wl[0].get("oracle_time_spans") == [[14, 33]]
                   and wl[0].get("rust_time_spans") == [[14, 33]], wl)
        self.check("time_mask ① masked_ranges 登记 plain_time_text 掩码命中",
                   len(mr) == 1 and mr[0].get("oracle_spans") == [[14, 33]], mr)

        # ② 掩码生效、但时间位**之外**有真差异（时间戳后的备注文案不同，等长）⇒ 掩码不掩盖
        suf_o = self.enc_gbk_wire("当前日期时间: 2026-10-24 13:46:43（偏快1秒）")
        suf_r = self.enc_gbk_wire("当前日期时间: 2026-10-24 13:46:44（偏慢9秒）")
        rc, rep = run_case("suffix", 100, suf_o, suf_r)
        kinds = sorted({d.get("kind") for d in (rep or {}).get("diffs", [])
                        if d.get("ident") == 100})
        wl2 = [w for w in (rep or {}).get("whitelisted", [])
               if w.get("kind") == "time_text_masked"]
        self.check("time_mask ② 时间位之外的尾部真差异 => rc 1（掩码不掩盖）",
                   rc == 1 and kinds == ["multiset_extra", "multiset_missing"], (rc, kinds))
        self.check("time_mask ② 该差异不得登记成 time_text_masked（掩码只免时间位）",
                   not wl2, wl2)

        def rules_100_compare():
            rules = json.loads(json.dumps(self.builtin_rules))
            rules["rules"] = [r for r in rules["rules"] if r.get("ident") != 100]
            rules["rules"].append({"ident": 100, "action": "compare",
                                   "reason": "selftest：ident=100 改逐字节比，"
                                             "钉时间掩码的「不掩盖/定位」路径",
                                   "tier": "L", "approved_by": "selftest"})
            return self.rules_file("t25-rules-100cmp.json", rules)

        rp100 = rules_100_compare()
        # ②b 同 ② 夹具、ident=100 逐字节规则 ⇒ compare_bodies 的掩码路径：
        #    diff 定位在**明文** offset=37（备注「快/慢」首字节；占位符之后的真差异），
        #    并带时间区间证据 [[14,33]]
        rc, rep = run_case("suffixcmp", 100, suf_o, suf_r, ["--rules", rp100])
        bd = [d for d in (rep or {}).get("diffs", []) if d.get("kind") == "body"]
        self.check("time_mask ②b 逐字节规则下掩码后真差异 => rc 1 且 space=plain offset=37",
                   rc == 1 and len(bd) == 1 and bd[0].get("space") == "plain"
                   and bd[0].get("offset") == 37
                   and bd[0].get("oracle_time_spans") == [[14, 33]], (rc, bd))
        # ②c 静态文案前缀被改（间→刻）⇒ 掩码不命中前缀 ⇒ 退回逐字节 => rc 1，定位线缆 offset=13
        #    （明文[10]=0xBC(间首字节) → 线缆[4*(10//3)+10%3]=13，掩码未掩前缀）
        pre_o = self.enc_gbk_wire("当前日期时间: 2026-10-24 13:46:43")
        pre_r = self.enc_gbk_wire("当前日期时刻: 2026-10-24 13:46:43")
        rc, rep = run_case("prefixcmp", 100, pre_o, pre_r, ["--rules", rp100])
        bd = [d for d in (rep or {}).get("diffs", []) if d.get("kind") == "body"]
        self.check("time_mask ②c 前缀静态文案被改 => rc 1（前缀照比，掩码不命中）"
                   "且定位线缆 offset=13",
                   rc == 1 and len(bd) == 1 and bd[0].get("offset") == 13
                   and bd[0].get("space", "wire") != "plain", (rc, bd))
        # ③ 明文长度不同（一侧备注多一段）⇒ body_len diff（掩码不掩盖长度差）
        len_r = self.enc_gbk_wire("当前日期时间: 2026-10-24 13:46:44（偏慢9秒）!")
        rc, rep = run_case("lenmask", 100, suf_o, len_r, ["--rules", rp100])
        bl = [d for d in (rep or {}).get("diffs", []) if d.get("kind") == "body_len"]
        self.check("time_mask ③ 掩码后明文长度仍不同 => rc 1 + body_len（space=plain）",
                   rc == 1 and len(bl) == 1 and bl[0].get("space") == "plain"
                   and bl[0].get("oracle") == 44 and bl[0].get("rust") == 45, (rc, bl))

        # ④ 牙：掩码不越 ident —— 同形时间差放到 ident=658（action=compare）⇒ 仍 rc 1，
        #    定位到与真机证据相同的线缆 offset=38，且不得出现 time_text_masked
        rc, rep = run_case("nocross", 658, ts_o, ts_r)
        bd = [d for d in (rep or {}).get("diffs", []) if d.get("kind") == "body"]
        wl3 = [w for w in (rep or {}).get("whitelisted", [])
               if w.get("kind") == "time_text_masked"]
        self.check("time_mask ④ 掩码不越 ident（658 同形时间差 => rc 1）",
                   rc == 1 and len(bd) == 1 and bd[0].get("offset") == 38 and not wl3,
                   (rc, bd, wl3))

        # ⑤ 牙：body 不是合法 EDCode 线缆字节（明文直写）⇒ 解码失败 ⇒ 退回逐字节（fail-closed）
        raw_o = "当前日期时间: 2026-10-24 13:46:43".encode("gbk")
        raw_r = "当前日期时间: 2026-10-24 13:47:57".encode("gbk")
        rc, rep = run_case("notwire", 100, raw_o, raw_r)
        kinds = sorted({d.get("kind") for d in (rep or {}).get("diffs", [])
                        if d.get("ident") == 100})
        wl4 = [w for w in (rep or {}).get("whitelisted", [])
               if w.get("kind") == "time_text_masked"]
        self.check("time_mask ⑤ 非线缆字节（解码失败）=> rc 1，不掩码不登记 time_text_masked",
                   rc == 1 and kinds == ["multiset_extra", "multiset_missing"] and not wl4,
                   (rc, kinds, wl4))
    # ------------------------------------------------------------------
    # 批次 6（卡 openmir2-spell-cast-controlframe-filter）：ident=null 控制帧过滤。
    # 真机来源：window-run-1 spell_cast 双栈运行（2026-10-24）；冻结副本在
    # tools/conformance/fixtures/spell_cast/{csharp,rust}/frames.ndjson。oracle 第 31 行
    # 的 GameGate `+GD/<rtime>` 时间应答控制帧（frame_class=server_ack_time，ident=null）
    # 曾让 load_stream fail-closed（「缺合法 ident」）⇒ 整轮 UNDECIDABLE。
    # ------------------------------------------------------------------
    #: 真实控制帧原样行（oracle 文件行 31：10 位 tick）
    REAL_CONTROL_FRAME_10 = (
        '{"seq": 31, "t_ms": 4507, "phase": "gamegate", "conn": "gamegate", "ident": null,'
        ' "recog": null, "param": null, "tag": null, "series": null, "body_len": 13,'
        ' "body_hex": "2b47442f353638383330333930",'
        ' "body_sha256": "524f429f783e3f8ddeaaf32b327dce084bd01881efdce64d7adc4dc43a619178",'
        ' "frame_class": "server_ack_time", "tail": "!", "raw_len": 15,'
        ' "body_text": "+GD/568830390"}')
    #: 真实控制帧原样行（rust 文件行 33：13 位 tick，同族不同长度的样本）
    REAL_CONTROL_FRAME_13 = (
        '{"seq": 33, "t_ms": 2241, "phase": "gamegate", "conn": "gamegate", "ident": null,'
        ' "recog": null, "param": null, "tag": null, "series": null, "body_len": 17,'
        ' "body_hex": "2b47442f31373932383231343737393132",'
        ' "body_sha256": "4aeeafeecf363c23b6956aa0ef69ab2e9d4773a22ae8f91b3c9bf1575e9d8a48",'
        ' "frame_class": "server_ack_time", "tail": "!", "raw_len": 19,'
        ' "body_text": "+GD/1792821477912"}')

    @classmethod
    def _mixed_control_lines(cls, control_lines, frames):
        """控制帧与协议帧**交错**排（不聚在头尾）：证明过滤按内容判、不看位置。"""
        proto = [json.dumps(fr, ensure_ascii=True, sort_keys=True) for fr in frames]
        out = []
        for i in range(max(len(control_lines), len(proto))):
            if i < len(control_lines):
                out.append(control_lines[i])
            if i < len(proto):
                out.append(proto[i])
        return out

    @staticmethod
    def _filtered(rep, side):
        """report["frames"]["filtered_control_frames"] 里某一侧的台账（缺侧给空）。"""
        fcf = ((rep or {}).get("frames") or {}).get("filtered_control_frames") or {}
        return fcf.get(side) or {}

    def t_control_frames(self):
        ctl10, ctl13 = self.REAL_CONTROL_FRAME_10, self.REAL_CONTROL_FRAME_13
        o, r = self.base_pair()

        def run_case(tag, o_lines, r_lines, extra=()):
            od = self.write_run("t25-%s-o" % tag, None, raw_lines=o_lines)
            rd = self.write_run("t25-%s-r" % tag, None, raw_lines=r_lines)
            return self.compare(od, rd, list(extra))

        o_lines = self._mixed_control_lines([ctl10, ctl13], o)
        r_lines = self._mixed_control_lines([], r)

        # ① 控制帧被滤 + 台账：两侧等价 ⇒ rc 0；oracle 侧 2 条控制帧逐帧登记
        #   （行 1/3、frame_class、seq），rust 侧 0 条；协议帧 7 条全部参与比较。
        rc, rep = run_case("filter", o_lines, r_lines)
        self.check("控制帧 ① 混入控制帧后其余全等 => rc 0", rc == 0,
                   (rc, (rep or {}).get("undecidable"), (rep or {}).get("diffs")))
        fo, fr_ = self._filtered(rep, "oracle"), self._filtered(rep, "rust")
        self.check("控制帧 ① oracle 侧滤 2 条并逐帧留痕（行 1/3，server_ack_time）",
                   fo.get("count") == 2
                   and [f.get("line") for f in fo.get("frames") or []] == [1, 3]
                   and all(f.get("frame_class") == "server_ack_time"
                           for f in fo.get("frames") or []),
                   fo)
        self.check("控制帧 ① rust 侧 0 条被滤", fr_.get("count") == 0, fr_)
        self.check("控制帧 ① 协议帧 7 条全部保留参与比较（oracle_total 不含控制帧）",
                   ((rep or {}).get("frames", {}).get("oracle_total") == 7
                    and (rep or {}).get("frames", {}).get("compared") == 7),
                   (rep or {}).get("frames"))
        self.check("控制帧 ① 终端摘要有过滤计数行（不静默）",
                   any("filtered(控制帧" in ln for ln in
                       self.api.human_summary(rep).splitlines()),
                   self.api.human_summary(rep))

        # ② 正常帧保留（牙）：同一混入形态下协议帧改一个字节 => 仍 rc 1 且定位
        #   （过滤绝不吞真差异）。
        r_bad = json.loads(json.dumps(r))
        target = [fr for fr in r_bad if fr["ident"] == 658][0]
        body = bytes.fromhex(target["body_hex"])
        mutated = body[:-1] + bytes([body[-1] ^ 0xFF])
        target["body_hex"] = mutated.hex()
        target["body_len"] = len(mutated)
        target["body_sha256"] = _sha(mutated)
        rc, rep = run_case("keep-proto", o_lines,
                           self._mixed_control_lines([], r_bad))
        self.check("控制帧 ② 混入控制帧 + 协议帧改一字节 => rc 1 且定位 658 偏移",
                   rc == 1 and any(d.get("ident") == 658
                                   and d.get("offset") == len(body) - 1
                                   for d in (rep or {}).get("diffs", [])),
                   (rc, (rep or {}).get("diffs")))
        # ②b 控制帧内容改坏（hex 改坏、与声明的 sha 不再自洽）不影响判定：
        #   控制帧根本不进判定，只留台账（登记的是驱动器声明值，不另做校验）。
        bad_ctl = json.loads(ctl13)
        bad_ctl["body_hex"] = "ff" * 8
        rc, rep = run_case("ctl-content",
                           self._mixed_control_lines(
                               [ctl10, json.dumps(bad_ctl, ensure_ascii=True)], o),
                           self._mixed_control_lines([], r))
        self.check("控制帧 ②b 控制帧内容改坏仍被滤 => rc 0（不进判定，台账照记）",
                   rc == 0 and self._filtered(rep, "oracle").get("count") == 2,
                   (rc, (rep or {}).get("diffs"), self._filtered(rep, "oracle")))

        # ③ 全控制帧流不崩溃：两侧只有控制帧 => kept 0/0 走 empty_run 兜底（rc 2），
        #   --allow-empty 才放行；全程无异常（异常会让 selftest 整体 INCOMPLETE）。
        only_ctl = [ctl10, ctl13]
        rc, rep = run_case("all-ctl", only_ctl, list(only_ctl))
        self.check("控制帧 ③ 全控制帧流 => rc 2 (empty_run)，不崩溃",
                   rc == 2 and any(u.get("kind") == "empty_run"
                                   for u in (rep or {}).get("undecidable", [])),
                   (rc, (rep or {}).get("undecidable")))
        self.check("控制帧 ③ 全控制帧流两侧各滤 2 条（台账照记）",
                   self._filtered(rep, "oracle").get("count") == 2
                   and self._filtered(rep, "rust").get("count") == 2,
                   (self._filtered(rep, "oracle"), self._filtered(rep, "rust")))
        rc2, rep2 = run_case("all-ctl-allow", only_ctl, list(only_ctl),
                             ["--allow-empty"])
        self.check("控制帧 ③ --allow-empty => rc 0 且登记 allowed_empty_run",
                   rc2 == 0 and any(w.get("kind") == "allowed_empty_run"
                                    for w in (rep2 or {}).get("whitelisted", [])),
                   (rc2, (rep2 or {}).get("whitelisted")))

        # ④ 红线牙（过滤不外溢）：ident **存在但非法**（负数/超 u16/字符串）一律仍
        #   fail-closed（「缺合法 ident」），不滤、不崩、整份输入被拒（report 无 frames 台账
        #   ——输入层拒绝是整体拒绝，不下账）。
        for tag, bad_ident in (("neg", -1), ("over", 70000), ("str", "53")):
            obad = json.loads(json.dumps(o))
            obad[0]["ident"] = bad_ident
            rc, rep = run_case("bad-%s" % tag,
                               self._mixed_control_lines([], obad),
                               self._mixed_control_lines([], r))
            u0 = ((rep or {}).get("undecidable") or [{}])[0]
            self.check("控制帧 ④ ident 非法(%s=%r) => rc 2 fail-closed（不滤）"
                       % (tag, bad_ident),
                       rc == 2 and u0.get("source") == "input"
                       and "缺合法 ident" in str(u0.get("detail")),
                       (rc, u0))
        # ④b frame_class=protocol 却缺 ident = 驱动器自相矛盾的产物 ⇒ 同样 fail-closed
        #   （过滤绝不扩大到任何声明为协议帧的行）。
        proto_null = json.loads(ctl10)
        proto_null["frame_class"] = "protocol"
        rc, rep = run_case(
            "bad-proto-null",
            self._mixed_control_lines([json.dumps(proto_null, ensure_ascii=True)], o),
            self._mixed_control_lines([], r))
        u0 = ((rep or {}).get("undecidable") or [{}])[0]
        self.check("控制帧 ④b frame_class=protocol 却缺 ident => rc 2（自相矛盾不滤）",
                   rc == 2 and u0.get("source") == "input"
                   and "自相矛盾" in str(u0.get("detail")),
                   (rc, u0))

        # ⑤ 真机冻结 fixture（tools/conformance/fixtures/spell_cast/）：修复前必红
        #   （input/Undecidable 缺合法 ident）；修复后必须出确定性 verdict：
        #   两侧各滤 2 条控制帧（oracle 行 31/34、rust 行 33/36）、34 条协议帧全部比较。
        #   注：钉死 rc/diffs/undecidable 是**冻结 fixture + 冻结 rules.json** 下的确定性
        #   结论。ident 211（SM_SENDMYMAGIC）已随规则卡 openmir2-compare-ident211-
        #   classification 入表（action=compare 全字节比，见 ⑥）；残余 rc=1 是已知真差异
        #   ident 53 ×3（该红仍红，归 SM_HEALTHSPELLCHANGED 卡；与 baseline 钉
        #   ignored_ident_rules==[14] 同理，规则再变时随该规则卡更新这里）。
        fix_o = os.path.join(self.api.REPO_ROOT, "tools", "conformance", "fixtures",
                             "spell_cast", "csharp")
        fix_r = os.path.join(self.api.REPO_ROOT, "tools", "conformance", "fixtures",
                             "spell_cast", "rust")
        self.check("控制帧 ⑤ 冻结 fixture 双侧 frames.ndjson 在库",
                   os.path.exists(os.path.join(fix_o, "frames.ndjson"))
                   and os.path.exists(os.path.join(fix_r, "frames.ndjson")),
                   (fix_o, fix_r))
        rc, rep = self.compare(fix_o, fix_r, ["--ignore-idents", "14"])
        fo, fr_ = self._filtered(rep, "oracle"), self._filtered(rep, "rust")
        self.check("控制帧 ⑤ 冻结 fixture 不再 input/Undecidable（出 verdict）",
                   rc in (0, 1, 2) and rep is not None
                   and not any(u.get("source") == "input"
                               for u in (rep or {}).get("undecidable", [])),
                   (rc, (rep or {}).get("undecidable")))
        self.check("控制帧 ⑤ oracle 滤 2 条（行 31/34）、rust 滤 2 条（行 33/36）",
                   fo.get("count") == 2
                   and [f.get("line") for f in fo.get("frames") or []] == [31, 34]
                   and fr_.get("count") == 2
                   and [f.get("line") for f in fr_.get("frames") or []] == [33, 36],
                   (fo, fr_))
        self.check("控制帧 ⑤ 34 条协议帧全部比较（oracle_total=rust_total=compared=34）",
                   ((rep or {}).get("frames", {}).get("oracle_total") == 34
                    and (rep or {}).get("frames", {}).get("rust_total") == 34
                    and (rep or {}).get("frames", {}).get("compared") == 34),
                   (rep or {}).get("frames"))
        # ⑥ 211 规则卡（openmir2-compare-ident211-classification）：211 = SM_SENDMYMAGIC
        #   （Messages.cs:225，发送点 PlayObject.Base.cs:1052-1070 SendUseMagic，series=
        #   MagicList.Count，body=逐条 EDCode(ClientMagic 84B)+'/'）已按**全字节比**入表
        #   ——不登记 ignore/掩码：body 是魔法库静态定义 + 玩家持久化状态，无墙钟/会话成分。
        #   冻结 fixture 上该帧从唯一残余 UNDECIDABLE 转为已比（两侧 seq18 同形 ⇒ 0 diff），
        #   而已知真差异 ident 53 ×3 该红仍红。
        und = (rep or {}).get("undecidable", [])
        diffs = (rep or {}).get("diffs", []) or []
        rule211 = [x for x in ((rep or {}).get("coverage", {}) or {})
                   .get("ident_rules") or [] if x.get("ident") == 211]
        d211 = [d for d in diffs if d.get("ident") == 211]
        self.check("控制帧 ⑥ ident 211 已入规则表（action=compare 全字节比）",
                   rc == 1 and len(rule211) == 1
                   and rule211[0].get("action") == "compare",
                   (rc, rule211))
        self.check("控制帧 ⑥ 211 帧已比且 0 diff：UNDECIDABLE/unclassified 清零"
                   "（两侧 seq18 body_sha256 同形）",
                   not d211 and not und
                   and not ((rep or {}).get("coverage", {}) or {})
                   .get("unclassified_idents"),
                   (d211, und,
                    ((rep or {}).get("coverage", {}) or {}).get("unclassified_idents")))
        self.check("控制帧 ⑥ 残余 diffs = 已知真差异 ident 53 ×3（该红仍红，不许被 211 "
                   "登记掩掉）",
                   len(diffs) == 3 and all(d.get("ident") == 53 for d in diffs),
                   [(d.get("ident"), d.get("field"), d.get("frame_index"))
                    for d in diffs])
        # ⑦ 53 真差异分诊卡（openmir2-compare-ident53-diff-triage，tier=R）：残余 3 处
        #   ident 53 diff 是**显式登记的真差异**，逐处钉死 anatomy——
        #   (frame_index 30/32, body_len, oracle 0 vs rust 16) ×2 = D-9 owner 裁决的
        #   HIGHHP 家族故意偏离（mir2-rs/tests/parity/whitelist.md D-9：冻结客户端
        #   HIGHHP=True 要 12B TCharDesc 尾；window-run-1 的 pin
        #   MIR2_CLIENT_ABILITY_LAYOUT=0 在起栈链丢失，game-svr.out.log 自证行
        #   「layout=client-58（<unset>）」）⇒ rules.json 53 条目 fail-closed 该红仍红，
        #   修属窗口工具链（pin 转发），不许登记成掩码/ignore 消红；
        #   (frame_index 32, head tag, oracle 13 vs rust 15) = MP 投影/实体双份实现 bug
        #   （成因已静态定位，review bab63106 F1：lib.rs:10426 wrapper 只扣会话投影
        #   sg.state.wabil.mp 不镜像 entity.mp + lib.rs:18499 P21 每 tick 无条件
        #   sg.state.wabil.mp = e.mp 刷回，entity.mp 无生产写点恒 17 ⇒ cast1 17→15 发 15
        #   → 刷回 17 → cast2 17→15 发 15 → 落库 17，与夹具 15/15/17 逐位吻合；
        #   基线 MP=17 双侧证实：dbsnap + 入图日志；oracle 17→15→13 与 stamina
        #   SpellFillTime=800 模型逐位吻合）⇒ 修复触数值+持久化归 R，MP 镜像修复另卡，
        #   窗口复拍验证。逐字节解剖表与证据链见
        #   docs/compare-ident53-diff-triage-20261024.md；fixture/rules 任一漂移或
        #   掩码化都会让本断言红。修复卡落地并重新冻结 fixture 后，按 ⑤/⑥ 同款义务
        #   更新这里。tag 差异已由本卡（openmir2-mp-session-projection-clobber）修复
        #   （扣减镜像进 entity.mp，P21 刷回不再覆盖），窗口复拍验证 13/13 后刷新 fixture。
        anatomy = sorted(
            (d.get("frame_index"), d.get("kind"), d.get("field"),
             d.get("oracle"), d.get("rust"))
            for d in diffs)
        self.check("控制帧 ⑦ 残余 3 处 ident 53 diff anatomy 逐处钉死"
                   "（body_len 0/16 ×2 + head tag 13/15 ×1，R 级显式登记）",
                   anatomy == [
                       (30, "body_len", "body_len", 0, 16),
                       (32, "body_len", "body_len", 0, 16),
                       (32, "head", "tag", 13, 15),
                   ],
                   anatomy)
    # ------------------------------------------------------------------
    # 批次 7（卡 openmir2-compare-ident600-registration，tier=L）：ident 600 =
    # SM_DROPITEM_SUCCESS 入规则表（action=compare 全字节比 + head_compare recog）。
    # 批次 8（卡 openmir2-compare-ident601-registration，tier=L）：ident 601 =
    # SM_DROPITEM_FAIL（同一条 case CM_DROPITEM 的失败支，:808）同范式登记。
    # 冻结输入 tools/conformance/fixtures/dropitem_ack/ 是**构造等价夹具**：
    # 600 = pickup_item 的 600 等待帧（window-run-2 未跑到 pickup），头部形态由
    # ground-probe 真机向量 drop_ok_control_price 锚定；601 = 真机向量
    # drop_fail_item_id_zero 的请求参数（make_index=51 / name="ring"）与头部形态复刻，
    # body 按 EDCode 算法由 request.name 推出。
    # ------------------------------------------------------------------
    #: EDCode(GBK("木剑")) = 6B：明文 c4bebda3 → 线缆 54424d443f3f。
    FIXTURE_600_BODY_HEX = "54424d443f3f"
    FIXTURE_600_BODY_SHA = (
        "e8c33c3d97209b6302fde94d6a4987e728b83bcc509096071dbb86067460d746")
    #: EDCode(GBK("ring")) = 6B：明文 72696e67 → 线缆 726d3e796f3e。
    FIXTURE_601_BODY_HEX = "726d3e796f3e"
    FIXTURE_601_BODY_SHA = (
        "9500c41cd99da38613aa6c516f404cc2fb09191b2c4c13c41b91a7d1c227d1eb")

    def _fixture_frames(self, side):
        path = os.path.join(self.api.REPO_ROOT, "tools", "conformance", "fixtures",
                            "dropitem_ack", side, "frames.ndjson")
        with open(path, "r", encoding="utf-8") as f:
            return [json.loads(ln) for ln in f if ln.strip()]

    def _fixture_ident_frames(self, side, ident):
        return [r for r in self._fixture_frames(side) if r.get("ident") == ident]

    def _fixture_600_frames(self, side):
        return self._fixture_ident_frames(side, 600)

    def _fixture_601_frames(self, side):
        return self._fixture_ident_frames(side, 601)

    def _mutate_rust_fixture(self, ident, mutate):
        """取 rust 侧**整份夹具**（600+601），只改指定 ident 的那一行。

        牙必须在含两帧的完整夹具上做：若只写单帧，对侧多一帧会被判成缺帧，
        「改坏必红」的红就不再指向被改的那一处。
        """
        rows = json.loads(json.dumps(self._fixture_frames("rust")))
        targets = [r for r in rows if r.get("ident") == ident]
        assert len(targets) == 1, (ident, len(targets))
        mutate(targets[0])
        return rows

    def _ground_vector(self, name):
        gv_path = os.path.join(self.api.REPO_ROOT, "mir2-rs", "tests", "parity", "m3",
                               "fixtures", "ground_vectors.json")
        if not os.path.exists(gv_path):
            return None
        with open(gv_path, "r", encoding="utf-8") as f:
            gv = json.load(f)
        return {c.get("name"): c for c in (gv or {}).get("cases", [])}.get(name)

    def t_dropitem_ack(self):
        fix = os.path.join(self.api.REPO_ROOT, "tools", "conformance", "fixtures",
                           "dropitem_ack")
        fo, fr = os.path.join(fix, "csharp"), os.path.join(fix, "rust")

        # ① 冻结夹具在库，且两侧逐字节一致：600 的头部/body 是确定性构造，
        #   夹具钉死 body 字节与头部语义（recog=MakeIndex=5001）。
        ok_rows = True
        for side in ("csharp", "rust"):
            rows = self._fixture_600_frames(side)
            ok_rows = ok_rows and len(rows) == 1
            if len(rows) != 1:
                continue
            r = rows[0]
            ok_rows = ok_rows and (
                r.get("ident") == 600 and r.get("recog") == 5001
                and r.get("param") == 0 and r.get("tag") == 0 and r.get("series") == 0
                and r.get("body_len") == 6
                and r.get("body_hex") == self.FIXTURE_600_BODY_HEX
                and r.get("body_sha256") == self.FIXTURE_600_BODY_SHA
                and r.get("frame_class") == "protocol")
        self.check("600 ① 冻结夹具在库：两侧各 1 帧，头部 recog=5001(MakeIndex)/param=tag="
                   "series=0、body=EDCode(GBK(木剑))=54424d443f3f 且 sha256 钉死",
                   ok_rows, (fo, fr))

        # ② oracle 真机向量锚（ground-probe 实测）：600 的头部形态 = ident 600 /
        #   recog=MakeIndex / param=tag=series=0；同 send 点的 601 同形。
        #   本断言把「构造夹具」与真实 oracle 产物绑在一起——向量漂移则本断言红。
        ok_case = self._ground_vector("drop_ok_control_price")
        fl_case = self._ground_vector("drop_fail_item_id_zero")
        ok_v = (ok_case or {}).get("expect", {}).get("immediate_frames")
        fl_v = (fl_case or {}).get("expect", {}).get("immediate_frames")
        self.check("600 ② 头部形态锚 ground-probe 真机向量：drop_ok_control_price ⇒ "
                   "[{600,recog=MakeIndex,0,0,0}]，drop_fail_item_id_zero ⇒ 601 同形",
                   ok_v == [{"ident": 600, "recog": 52, "param": 0, "tag": 0,
                             "series": 0}]
                   and fl_v == [{"ident": 601, "recog": 51, "param": 0, "tag": 0,
                                 "series": 0}],
                   (ok_v, fl_v))

        # ③ 规则表登记：600 = action=compare（全字节比、无掩码），
        #   head_compare:["recog"] 取回 MakeIndex（recog 不是会话 ActorId）。
        rule600 = None
        for r in self.builtin_rules.get("rules", []):
            if r.get("ident") == 600:
                rule600 = r
                break
        self.check("600 ③ 规则表：action=compare 全字节比 + head_compare=[recog]"
                   "（不 ignore、不掩码；recog=MakeIndex 必须取回比较）",
                   bool(rule600) and rule600.get("action") == "compare"
                   and rule600.get("head_compare") == ["recog"]
                   and not rule600.get("head_mask")
                   and not rule600.get("ranges"), rule600)

        # ④ 冻结夹具比较：600 从 UNCLASSIFIED 转为已比 ⇒ rc 0、compared=2
        #   （600+601 两帧）、0 diff / 0 undecidable / 0 unclassified。
        rc, rep = self.compare(fo, fr)
        cov = (rep or {}).get("coverage", {}) or {}
        frames = (rep or {}).get("frames", {}) or {}
        ident_rules = cov.get("ident_rules") or []
        self.check("600 ④ 冻结夹具 ⇒ rc 0 且 600 已比（compared=2，0 diff/undecidable/"
                   "unclassified；coverage 里 600 action=compare）",
                   rc == 0 and frames.get("compared") == 2
                   and not ((rep or {}).get("diffs") or [])
                   and not ((rep or {}).get("undecidable") or [])
                   and not (cov.get("unclassified_idents") or [])
                   and any(x.get("ident") == 600 and x.get("action") == "compare"
                           for x in ident_rules),
                   (rc, frames, (rep or {}).get("diffs"),
                    (rep or {}).get("undecidable"), cov.get("unclassified_idents")))

        # ⑤ 牙A（删规则）：无 600 条目 ⇒ 整轮 rc 2 UNDECIDABLE
        #   （unclassified_ident 只有 ident=600；601 已登记、不得被连带判）。
        no600 = json.loads(json.dumps(self.builtin_rules))
        no600["rules"] = [r for r in no600["rules"] if r.get("ident") != 600]
        no600_path = self.rules_file("rules-no-600.json", no600)
        rc, rep = self.compare(fo, fr, ["--rules", no600_path])
        uc600 = [u for u in (rep or {}).get("undecidable") or []
                 if u.get("kind") == "unclassified_ident"]
        self.check("600 ⑤ 牙A：删 600 规则 ⇒ rc 2（UNDECIDABLE，unclassified_ident "
                   "ident=600 复现；601 仍已比，不被连带）",
                   rc == 2 and [u.get("ident") for u in uc600] == [600]
                   and not ((rep or {}).get("diffs") or []),
                   (rc, (rep or {}).get("undecidable")))

        # ⑥ 牙B（改坏 body 一字节）：rust 侧 600 帧首字节 0x54→0x55，sha256 同步重算
        #   ⇒ rc 1，diff 精确到 ident=600 / kind=body / offset=0 / 0x54 vs 0x55。
        def flip600(rec):
            wire = bytearray.fromhex(rec["body_hex"])
            wire[0] ^= 0x01
            rec["body_hex"] = bytes(wire).hex()
            rec["body_sha256"] = _sha(bytes(wire))
            rec["body_len"] = len(wire)
        bad_d = self.write_run("t600-badbody-r", self._mutate_rust_fixture(600, flip600))
        rc, rep = self.compare(fo, bad_d)
        all_d = (rep or {}).get("diffs") or []
        bd = [d for d in all_d if d.get("ident") == 600]
        self.check("600 ⑥ 牙B：改坏 600 body 一字节 ⇒ rc 1 且定位 "
                   "kind=body/offset=0/oracle 0x54 vs rust 0x55（601 不受牵连）",
                   rc == 1 and len(all_d) == 1 and len(bd) == 1
                   and bd[0].get("kind") == "body" and bd[0].get("offset") == 0
                   and bd[0].get("oracle_byte") == 0x54
                   and bd[0].get("rust_byte") == 0x55, (rc, bd))

        # ⑦ 牙C（recog 取回）：rust 侧 recog 5001→5002 ⇒ 必须 rc 1 field=recog。
        #   若 head_compare 失效，全局 recog 掩码会把该差异静默吞掉（rc 0）⇒ 本断言红。
        def recog600(rec):
            rec["recog"] = 5002
        bad2_d = self.write_run("t600-badrecog-r",
                                self._mutate_rust_fixture(600, recog600))
        rc, rep = self.compare(fo, bad2_d)
        all_d = (rep or {}).get("diffs") or []
        hd = [d for d in all_d if d.get("ident") == 600]
        self.check("600 ⑦ 牙C：改坏 recog(MakeIndex) ⇒ rc 1 且 field=recog"
                   "（head_compare 生效，全局 recog 掩码不吞语义字段）",
                   rc == 1 and len(all_d) == 1 and len(hd) == 1
                   and hd[0].get("field") == "recog"
                   and hd[0].get("oracle") == 5001 and hd[0].get("rust") == 5002,
                   (rc, hd))

        # ================= ident 601 = SM_DROPITEM_FAIL（批次 8） =================
        # ⑧ 夹具：601 case 在库、两侧一致，头部/body 与真机向量的请求参数对齐。
        ok601 = True
        for side in ("csharp", "rust"):
            rows = self._fixture_601_frames(side)
            ok601 = ok601 and len(rows) == 1
            if len(rows) != 1:
                continue
            r = rows[0]
            ok601 = ok601 and (
                r.get("ident") == 601 and r.get("recog") == 51
                and r.get("param") == 0 and r.get("tag") == 0 and r.get("series") == 0
                and r.get("body_len") == 6
                and r.get("body_hex") == self.FIXTURE_601_BODY_HEX
                and r.get("body_sha256") == self.FIXTURE_601_BODY_SHA
                and r.get("body_text") == "ring"
                and r.get("raw_len") == 17 + 6 + 1
                and r.get("frame_class") == "protocol")
        self.check("601 ① 冻结夹具在库：两侧各 1 帧，头部 ident=601/recog=51(MakeIndex)/"
                   "param=tag=series=0、body=EDCode(GBK(ring))=726d3e796f3e 且 sha256 钉死",
                   ok601, (fo, fr))

        # ⑨ 向量绑定：601 夹具的 recog 来自真机向量 drop_fail_item_id_zero 的请求
        #   make_index（51），头部逐字段等于向量 immediate_frames 的唯一一帧。
        req = (fl_case or {}).get("request")
        self.check("601 ② 夹具锚真机向量：drop_fail_item_id_zero.request={make_index:51,"
                   "name:'ring'}，immediate_frames=[{601,recog=51,0,0,0}]，与夹具 "
                   "recog=51/body=EDCode(ring) 对齐",
                   req == {"make_index": 51, "name": "ring"}
                   and fl_v == [{"ident": 601, "recog": 51, "param": 0, "tag": 0,
                                 "series": 0}]
                   and [r.get("recog") for r in self._fixture_601_frames("csharp")]
                   == [51], (req, fl_v))

        # ⑩ 规则表登记：601 = action=compare 全字节比 + head_compare=["recog"]。
        rule601 = None
        for r in self.builtin_rules.get("rules", []):
            if r.get("ident") == 601:
                rule601 = r
                break
        self.check("601 ③ 规则表：action=compare 全字节比 + head_compare=[recog]"
                   "（不 ignore、不掩码、无 ranges；recog=MakeIndex 必须取回比较）",
                   bool(rule601) and rule601.get("action") == "compare"
                   and rule601.get("head_compare") == ["recog"]
                   and rule601.get("tier") == "L"
                   and not rule601.get("head_mask")
                   and not rule601.get("ranges"), rule601)

        # ⑪ 601 由 UNCLASSIFIED 转为已比：coverage 里 601 action=compare，
        #   同一轮里 600/601 都被计数（compared=2），无未分类。
        rc, rep = self.compare(fo, fr)
        cov = (rep or {}).get("coverage", {}) or {}
        ident_rules = cov.get("ident_rules") or []
        self.check("601 ④ 冻结夹具 ⇒ 601 已比（coverage ident=601 action=compare；"
                   "compared=2；0 diff/undecidable/unclassified）",
                   rc == 0
                   and any(x.get("ident") == 601 and x.get("action") == "compare"
                           for x in ident_rules)
                   and ((rep or {}).get("frames") or {}).get("compared") == 2
                   and not ((rep or {}).get("diffs") or [])
                   and not ((rep or {}).get("undecidable") or [])
                   and not (cov.get("unclassified_idents") or []),
                   (rc, ident_rules, (rep or {}).get("frames"),
                    cov.get("unclassified_idents")))

        # ⑫ 牙A（删 601 规则）：rc 2，unclassified_ident 只有 601（600 不被连带）。
        no601 = json.loads(json.dumps(self.builtin_rules))
        no601["rules"] = [r for r in no601["rules"] if r.get("ident") != 601]
        no601_path = self.rules_file("rules-no-601.json", no601)
        rc, rep = self.compare(fo, fr, ["--rules", no601_path])
        uc601 = [u for u in (rep or {}).get("undecidable") or []
                 if u.get("kind") == "unclassified_ident"]
        self.check("601 ⑤ 牙A：删 601 规则 ⇒ rc 2（UNDECIDABLE，unclassified_ident "
                   "ident=601 复现；600 仍已比，不被连带）",
                   rc == 2 and [u.get("ident") for u in uc601] == [601]
                   and not ((rep or {}).get("diffs") or []),
                   (rc, (rep or {}).get("undecidable")))

        # ⑬ 牙B（改坏 601 body 一字节）：首字节 0x72→0x73 ⇒ rc 1，
        #   diff 精确到 ident=601 / kind=body / offset=0 / 0x72 vs 0x73。
        def flip601(rec):
            wire = bytearray.fromhex(rec["body_hex"])
            wire[0] ^= 0x01
            rec["body_hex"] = bytes(wire).hex()
            rec["body_sha256"] = _sha(bytes(wire))
            rec["body_len"] = len(wire)
        bad601_d = self.write_run("t601-badbody-r",
                                  self._mutate_rust_fixture(601, flip601))
        rc, rep = self.compare(fo, bad601_d)
        all_d = (rep or {}).get("diffs") or []
        bd601 = [d for d in all_d if d.get("ident") == 601]
        self.check("601 ⑥ 牙B：改坏 601 body 一字节 ⇒ rc 1 且定位 "
                   "kind=body/offset=0/oracle 0x72 vs rust 0x73（600 不受牵连）",
                   rc == 1 and len(all_d) == 1 and len(bd601) == 1
                   and bd601[0].get("kind") == "body"
                   and bd601[0].get("offset") == 0
                   and bd601[0].get("oracle_byte") == 0x72
                   and bd601[0].get("rust_byte") == 0x73, (rc, bd601))

        # ⑭ 牙C（recog 取回）：rust 侧 601 recog 51→52 ⇒ rc 1 field=recog。
        def recog601(rec):
            rec["recog"] = 52
        bad601b_d = self.write_run("t601-badrecog-r",
                                   self._mutate_rust_fixture(601, recog601))
        rc, rep = self.compare(fo, bad601b_d)
        all_d = (rep or {}).get("diffs") or []
        hd601 = [d for d in all_d if d.get("ident") == 601]
        self.check("601 ⑦ 牙C：改坏 601 recog(MakeIndex) ⇒ rc 1 且 field=recog "
                   "oracle=51/rust=52（head_compare 生效，全局 recog 掩码不吞语义字段）",
                   rc == 1 and len(all_d) == 1 and len(hd601) == 1
                   and hd601[0].get("field") == "recog"
                   and hd601[0].get("oracle") == 51
                   and hd601[0].get("rust") == 52, (rc, hd601))

    def t_db(self):
        dp = self.api.load_module("dbsnap_pair", self.api.DBSNAP_PAIR)
        cols = list(dp.TABLE_SPECS["mir2_db.characters_ablity"]["columns"])
        # 列集必须与登记表一致（否则 `db_schema` 结构漂移先红，测不到见证本身）。
        char_cols = list(dp.TABLE_SPECS["mir2_db.characters"]["columns"])
        i_id, i_name, i_upd = (char_cols.index("Id"), char_cols.index("ChrName"),
                               char_cols.index("LASTUPDATE"))

        def char_row(cid, name, mt):
            r = ["0"] * len(char_cols)
            r[i_id], r[i_name], r[i_upd] = cid, name, mt
            return r

        def rows(level):
            row = ["0"] * len(cols)
            row[cols.index("PlayerId")] = "1"
            row[cols.index("Level")] = str(level)
            # 卡 openmir2-dbsrv-session-timezone：ModifyTime 登记成 plausible ⇒ 夹具必须
            # 写「贴近本次运行墙钟」的值（write_run 的 run.json ended_at = 2026-01-01T00:00:10）。
            row[cols.index("ModifyTime")] = "2026-01-01 00:00:05"
            return [row]

        def rows_utc(level):
            r = rows(level)
            r[0][cols.index("ModifyTime")] = "2025-12-31 16:00:05"   # 早 8h = UTC 落库形态
            return r

        def snap(prefix, level, mt="2026-01-01 00:00:05", char="winplay0"):
            return {"format": "dbsnap-inline/1",
                    "tables": [{"table": prefix + ".characters", "columns": char_cols,
                                "rows": [char_row("1", char, mt)]},
                               {"table": prefix + ".characters_ablity",
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

        # ---- 时间列见证（卡 openmir2-dbsrv-session-timezone / follow-up `...-hardening`）----
        # 判据：**本角色行**在运行窗口 [start − lead, end + tol] 内有写入；不能只看表内最大值
        # （别人的行 / 上一次运行的行会误绿），也不能逐行判（历史行永远 stale）。
        def snap_tz(prefix, mt, extra_chars=(), char_name="winplay0", char_id="1", level=7):
            """characters（本角色 1 行 + 可选的「别人」行）+ characters_ablity（本角色 1 行）。"""
            chars = [char_row(char_id, char_name, mt)] + [list(r) for r in extra_chars]
            r = rows(level)
            r[0][cols.index("PlayerId")] = char_id
            r[0][cols.index("ModifyTime")] = mt
            return {"format": "dbsnap-inline/1",
                    "tables": [{"table": prefix + ".characters", "columns": char_cols,
                                "rows": chars},
                               {"table": prefix + ".characters_ablity", "columns": cols,
                                "rows": r}]}

        def with_clock(d, start, end, char="winplay0"):
            path = os.path.join(d, "run.json")
            argv = ["probe"] + (["--char-name", char] if char else [])
            with open(path, "w", encoding="utf-8", newline="\n") as f:
                json.dump({"argv": argv, "started_at": start, "ended_at": end}, f)
                f.write("\n")
            return d

        def tz_pair(tag, mt_o, mt_r, start="2026-01-01T00:00:00", end="2026-01-01T00:00:10",
                    extra_chars=(), **kw):
            ex = [char_row(*r) for r in extra_chars]
            od_ = self.write_run("tz-%s-o" % tag, frames,
                                 db=snap_tz("mir2_test_db", mt_o, extra_chars=ex, **kw))
            rd_ = self.write_run("tz-%s-r" % tag, frames,
                                 db=snap_tz("mir2_test_db_r", mt_r, extra_chars=ex, **kw))
            with_clock(od_, start, end)
            with_clock(rd_, start, end)
            return self.compare(od_, rd_, ["--allow-missing-fields"])

        def rules_with(db_patch, name):
            data = json.loads(json.dumps(self.builtin_rules))
            data["db"] = dict(data.get("db") or {})
            data["db"].update(db_patch)
            return self.rules_file(name, data)

        # 基线：两侧都在窗口内写 ⇒ rc 0
        rc_tz, rep_tz = tz_pair("base", "2026-01-01 00:00:05", "2026-01-01 00:00:05")
        self.check("牙TZ-a 本角色行在窗口内写（两侧）=> rc 0", rc_tz == 0,
                   (rc_tz, (rep_tz or {}).get("diffs")))

        # 8h 偏移（TZ-1 形态）⇒ stale
        rc_tz, rep_tz = tz_pair("utc", "2026-01-01 00:00:05", "2025-12-31 16:00:05")
        self.check("牙TZ-b rust 写 UTC（早 8h）=> rc 1 time_witness_stale",
                   rc_tz == 1 and any(d.get("source") == "db"
                                      and d.get("kind") == "time_witness_stale"
                                      for d in (rep_tz or {}).get("diffs", [])),
                   (rc_tz, (rep_tz or {}).get("diffs")))

        # G2：历史行（2022）+ 窗口内一行 ⇒ 绿（证明不是逐行判）
        rc_tz, rep_tz = tz_pair("hist", "2026-01-01 00:00:05", "2026-01-01 00:00:05",
                                extra_chars=[["9", "someone", "2022-12-22 00:00:02"]])
        self.check("牙TZ-g 历史行(2022) + 窗口内本角色行 => rc 0（逐行判会多一条 stale）",
                   rc_tz == 0, (rc_tz, (rep_tz or {}).get("diffs")))

        # F1-A：5 分钟前**别人**写的行 + 本角色行停在旧值 ⇒ 必须红（原来的 max-only 会误绿）
        rc_tz, rep_tz = tz_pair("f1a", "2022-12-22 00:00:02", "2022-12-22 00:00:02",
                                start="2026-01-01T00:05:00", end="2026-01-01T00:05:10",
                                extra_chars=[["9", "someone", "2026-01-01 00:04:30"]])
        self.check("牙TZ-h F1-A 只别人 5 分钟前写过、本角色没写 => rc 1（不许误绿）",
                   rc_tz == 1 and any(d.get("source") == "db"
                                      and d.get("kind") == "time_witness_stale"
                                      for d in (rep_tz or {}).get("diffs", [])),
                   (rc_tz, (rep_tz or {}).get("diffs")))

        # F1-C：本角色行是「上一次运行」写的（比 start 早 300s）⇒ 必须红（下界 slack 只有 lead）
        rc_tz, rep_tz = tz_pair("f1c", "2026-01-01 00:00:00", "2026-01-01 00:00:00",
                                start="2026-01-01T00:05:00", end="2026-01-01T00:05:10")
        self.check("牙TZ-i F1-C 本角色行写于 start 前 300s => rc 1（下界 slack=lead=30s）",
                   rc_tz == 1 and any(d.get("source") == "db"
                                      and d.get("kind") == "time_witness_stale"
                                      for d in (rep_tz or {}).get("diffs", [])),
                   (rc_tz, (rep_tz or {}).get("diffs")))

        # F1-B：本角色写了、且表里有更晚的「别人」行 ⇒ 仍绿（不许误红）
        # F1-B：本次写了（在窗口内），但**同一角色**还有一行更晚的（窗口外）⇒ 仍绿。
        # 「只看最大值」的写法会把这一行误判成 stale（窗口外）⇒ 用 any-in-window 才不会误红。
        rc_tz, rep_tz = tz_pair("f1b", "2026-01-01 00:00:05", "2026-01-01 00:00:05",
                                start="2026-01-01T00:00:00", end="2026-01-01T00:00:10",
                                extra_chars=[["9", "winplay0", "2026-06-01 00:00:00"]])
        self.check("牙TZ-j F1-B 本角色窗口内写过 + 同行更晚的一行（窗口外）=> rc 0（不许误红）",
                   rc_tz == 0, (rc_tz, (rep_tz or {}).get("diffs")))

        # 620s 长场景：运行**开头**写（距结束 620s）⇒ 绿（窗口口径，不是「距 end」）
        rc_tz, rep_tz = tz_pair("win", "2026-01-01 00:00:00", "2026-01-01 00:00:00",
                                start="2026-01-01T00:00:00", end="2026-01-01T00:10:20")
        self.check("牙TZ-e 620s 长场景：运行开头写的时间列 => rc 0（判据是窗口）",
                   rc_tz == 0, (rc_tz, (rep_tz or {}).get("diffs")))

        # 窗口下界：比 start 早 660s ⇒ 红
        rc_tz, rep_tz = tz_pair("early", "2025-12-31 23:49:00", "2025-12-31 23:49:00",
                                start="2026-01-01T00:00:00", end="2026-01-01T00:10:20")
        self.check("牙TZ-f 比 start 早 660s => rc 1 time_witness_stale", rc_tz == 1,
                   (rc_tz, (rep_tz or {}).get("diffs")))

        # 未登记 ⇒ rc 2；presence 显式放行 ⇒ rc 0；缺墙钟 ⇒ rc 2；缺键（G1）⇒ rc 2
        rp_unreg = rules_with({"time_columns": {}}, "tz-rules-unregistered.json")
        od_u = self.write_run("tz-un-o", frames, db=snap_tz("mir2_test_db", "2026-01-01 00:00:05"))
        rd_u = self.write_run("tz-un-r", frames, db=snap_tz("mir2_test_db_r", "2026-01-01 00:00:05"))
        with_clock(od_u, "2026-01-01T00:00:00", "2026-01-01T00:00:10")
        with_clock(rd_u, "2026-01-01T00:00:00", "2026-01-01T00:00:10")
        rc_un, rep_un = self.compare(od_u, rd_u, ["--allow-missing-fields", "--rules", rp_unreg])
        self.check("牙TZ-k 时间列未登记 => rc 2 time_witness_unregistered",
                   rc_un == 2 and any(u.get("kind") == "db_time_witness_unregistered"
                                      for u in (rep_un or {}).get("undecidable", [])),
                   (rc_un, (rep_un or {}).get("undecidable")))

        rp_pres = rules_with({"time_columns": {
            "mir2_db.characters": {"CREATEDATE": {"mode": "presence", "why": "selftest"},
                                   "LASTUPDATE": {"mode": "presence", "why": "selftest"}},
            "mir2_db.characters_ablity": {"ModifyTime": {"mode": "presence", "why": "selftest"}},
            "mir2_db.characters_indexes": {},
            "mir2_account.account": {}}}, "tz-rules-presence.json")
        rc_pr, rep_pr = self.compare(od_u, rd_u, ["--allow-missing-fields", "--rules", rp_pres])
        self.check("牙TZ-c 登记 presence => rc 0（放行是显式登记的结果）", rc_pr == 0,
                   (rc_pr, (rep_pr or {}).get("diffs")))

        rd_noclk = self.write_run("tz-noclk-r", frames,
                                  run_meta=False, db=snap_tz("mir2_test_db_r", "2026-01-01 00:00:05"))
        rc_nc, rep_nc = self.compare(od_u, rd_noclk, ["--allow-missing-fields"])
        self.check("牙TZ-d plausible 但缺 run.json 墙钟 => rc 2 time_witness_no_clock",
                   rc_nc == 2 and any(u.get("kind") == "db_time_witness_no_clock"
                                      for u in (rep_nc or {}).get("undecidable", [])),
                   (rc_nc, (rep_nc or {}).get("undecidable")))

        rules_no_key = json.loads(json.dumps(self.builtin_rules))
        rules_no_key["db"] = {k: v for k, v in (rules_no_key.get("db") or {}).items()
                              if k != "time_columns"}
        rp_nokey = self.rules_file("tz-rules-no-key.json", rules_no_key)
        rc_nk, rep_nk = self.compare(od_u, rd_u, ["--allow-missing-fields", "--rules", rp_nokey])
        self.check("牙TZ-l G1：db.time_columns 缺键 => rc 2（不许静默关掉见证后 PASS）",
                   rc_nk == 2, (rc_nk, (rep_nk or {}).get("verdict")))

        # 名称未声明（无 --char-name）⇒ unscope ⇒ 判不了（fail-closed）
        od_ns = self.write_run("tz-noscope-o", frames, db=snap_tz("mir2_test_db", "2026-01-01 00:00:05"))
        rd_ns = self.write_run("tz-noscope-r", frames, db=snap_tz("mir2_test_db_r", "2026-01-01 00:00:05"))
        with_clock(od_ns, "2026-01-01T00:00:00", "2026-01-01T00:00:10", char=None)
        with_clock(rd_ns, "2026-01-01T00:00:00", "2026-01-01T00:00:10", char=None)
        rc_ns, rep_ns = self.compare(od_ns, rd_ns, ["--allow-missing-fields"])
        self.check("牙TZ-m 缺 --char-name 无法限定到本角色行 => rc 2 time_witness_unscoped",
                   rc_ns == 2 and any(u.get("kind") == "db_time_witness_unscoped"
                                      for u in (rep_ns or {}).get("undecidable", [])),
                   (rc_ns, (rep_ns or {}).get("undecidable")))

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
