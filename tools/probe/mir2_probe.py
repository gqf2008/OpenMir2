#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""mir2_probe.py — 无头协议对拍台的**驱动器**（模拟客户端）。

把同一串 CM_* 打给两套栈（Rust 实现 / C# oracle），把 s2c 帧按字段落盘，
数据库后态由另一条车道的比较器负责。本工具只做「发 + 收 + 落证据」，
不做判定、不连数据库、不起任何服务端/客户端/代理进程之外的东西。

链路（全部照 C# 参考实现，只读）：
  1. LoginGate：CM_IDPASSWORD(2001) 体 ``account/password``
     -> 期望 SM_PASSOK_SELECTSERVER(529)；SM_NEEDUPDATE_ACCOUNT(531) 视为**未覆盖**（退出码 2）。
     （src/BotSrv/Scenes/Scene/LoginScene.cs SendLogin）
  2. 同一连接：CM_SELECTSERVER(104) 体 ``serverName`` -> 期望 SM_SELECTSERVER_OK(530)
     体 ``selgateIp/selgatePort/session``。
  3. 连 SelGate：CM_QUERYCHR(100) 体 ``account/session`` -> 期望 SM_QUERYCHR(520)。
     （src/DBSrv/Services/Impl/UserService.cs QueryChr 用 GetValidStr3 取 account，余下按 int 当 session）
  4. 同一连接：CM_SELCHR(103) 体 ``account/charName`` -> 期望 SM_STARTPLAY(525)
     体 ``gameGateIp/gameGatePort``。（src/DBSrv/Services/Impl/UserService.cs SelectChr）
  5. 连 GameGate：发**无 12B 头**的旧式登录串
     ``**account/charName/session/<clientVersion>/<clientVersion>``
     （src/BotSrv/Player/RobotPlayer.cs SendRunLogin；版本号取 Grobal2.ClientVersionNumber）。
  6. 世界握手（照 src/BotSrv/Player/RobotPlayer.Operate.cs）：
     收到 SM_SENDNOTICE(658) -> 回 CM_LOGINNOTICEOK(1018)；
     收到 SM_LOGON(50)        -> 发 CM_QUERYBAGITEMS(81, recog=1)。
     心跳：服务端单字节 '*'，回 '*'。
  7. 执行场景步骤（scenarios/README.md 的 DSL），全程落 ``frames.ndjson``。
  8. 收尾：默认发 CM_SOFTCLOSE(1009)（= BotSrv 的小退形状），再 drain 后断开。

确定性约定（对拍必须逐字节一致）：
  * ``CM_LOGINNOTICEOK`` 的 ``recog`` 在 BotSrv 里取客户端 tick（1284 行），本驱动器固定 0。
    该字段服务端不读（只在 LoginSrv 的公告确认路径上被忽略），固定值才能让两次运行逐字节可比。
    登记为 **D-probe-1**（工具内偏差，非服务端行为偏差）。
  * 步骤之间的等待用 ``--between-ms`` 显式给出，不依赖墙钟抖动。

用法：
  python tools/probe/mir2_probe.py --login-gate 127.0.0.1:7000 \
      --account test01 --password test01 --server-name 热血传奇 \
      --char-name test01 --scenario tools/probe/scenarios/login_only.json \
      --out E:/tmp/probe-run-001
  python tools/probe/mir2_probe.py --selftest     # 离线：切帧+解析+落 ndjson 全链路

退出码：0 全绿；1 链路/场景失败（登录被拒、wait 超时、选角不存在等）；
        2 UNCLASSIFIED（帧解析失败、规则没覆盖、场景 DSL 非法、内部错误）。
"""
import argparse
import hashlib
import json
import os
import socket
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from mir2frame import (FrameError, FrameSplitter, build_c2s_frame, build_legacy_c2s_frame,
                       build_s2c_frame, decode_text, encode_text)

TOOL = "tools/probe/mir2_probe.py"
SCHEMA_VERSION = 1

# ---- 消息号（src/OpenMir2/Messages.cs；只抄本驱动器用到的） ----
CM_QUERYBAGITEMS = 81
CM_QUERYCHR = 100
CM_SELCHR = 103
CM_SELECTSERVER = 104
CM_CLICKNPC = 1010
CM_MERCHANTDLGSELECT = 1011
CM_SOFTCLOSE = 1009
CM_IDPASSWORD = 2001
CM_LOGINNOTICEOK = 1018
CM_SAY = 3030

SM_LOGON = 50
SM_ABILITY = 52
SM_SYSMESSAGE = 100
SM_QUERYCHR = 520
SM_QUERYCHR_FAIL = 527
SM_STARTPLAY = 525
SM_STARTFAIL = 526
SM_PASSOK_SELECTSERVER = 529
SM_SELECTSERVER_OK = 530
SM_NEEDUPDATE_ACCOUNT = 531
SM_SENDNOTICE = 658
SM_MERCHANTSAY = 643
SM_OUTOFCONNECTION = 528

CLIENT_VERSION_NUMBER = 1200409180  # src/OpenMir2/Grobal2.cs
HEARTBEAT = b"*"


class ProbeError(Exception):
    """驱动器无法继续（内部/协议/配置）。退出码 2。"""


class ScenarioError(ProbeError):
    """场景 DSL 非法 / 规则没覆盖。退出码 2。"""


class StepFailure(Exception):
    """链路或场景步骤失败（可分类的业务失败）。退出码 1。"""


def now_ms():
    return int(time.time() * 1000)


def iso_wall(ms=None):
    t = time.time() if ms is None else ms / 1000.0
    return time.strftime("%Y-%m-%dT%H:%M:%S%z", time.localtime(t))


def parse_hostport(spec, what):
    if not isinstance(spec, str) or ":" not in spec:
        raise ProbeError("%s must be HOST:PORT, got %r" % (what, spec))
    host, _, port_s = spec.rpartition(":")
    if not host:
        raise ProbeError("%s has empty host: %r" % (what, spec))
    try:
        port = int(port_s)
    except ValueError:
        raise ProbeError("%s port is not an int: %r" % (what, spec))
    if not (0 < port < 65536):
        raise ProbeError("%s port out of range: %d" % (what, port))
    return host, port


def sha256_hex(data):
    return hashlib.sha256(data).hexdigest()


class Recorder:
    """把 s2c 帧流式写成 frames.ndjson（一行一帧，字段固定）。"""

    FIELDS = ("seq", "t_ms", "phase", "conn", "ident", "recog", "param", "tag",
              "series", "body_len", "body_hex", "body_sha256")

    def __init__(self, out_dir, t0_ms):
        self.out_dir = out_dir
        os.makedirs(out_dir, exist_ok=True)
        self.path = os.path.join(out_dir, "frames.ndjson")
        self._fh = open(self.path, "w", encoding="utf-8", buffering=1)
        self.t0_ms = t0_ms
        self.seq = 0
        self.counts = {}
        self.ident_hist = {}

    def write_frame(self, phase, conn_label, frame):
        self.seq += 1
        row = {
            "seq": self.seq,
            "t_ms": now_ms() - self.t0_ms,
            "phase": phase,
            "conn": conn_label,
            "ident": frame.ident,
            "recog": frame.recog,
            "param": frame.param,
            "tag": frame.tag,
            "series": frame.series,
            "body_len": len(frame.body),
            "body_hex": frame.body.hex(),
            "body_sha256": sha256_hex(frame.body),
            "frame_class": frame.frame_class,
            "tail": frame.tail.decode("ascii"),
            "raw_len": len(frame.raw),
            "body_text": decode_text(frame.body_decoded, "gbk", "replace"),
        }
        self._fh.write(json.dumps(row, ensure_ascii=False) + "\n")
        self.counts[conn_label] = self.counts.get(conn_label, 0) + 1
        key = ("%s/%s" % (conn_label, frame.ident) if frame.ident is not None
               else "%s/plain:%s" % (conn_label, frame.frame_class))
        self.ident_hist[key] = self.ident_hist.get(key, 0) + 1
        return row

    def close(self):
        try:
            self._fh.close()
        except Exception:  # noqa: BLE001
            pass


class Conn:
    """一条客户端连接：发帧、增量收帧、记证据。"""

    def __init__(self, label, recorder, on_frame=None):
        self.label = label
        self.recorder = recorder
        self.on_frame = on_frame
        self.sock = None
        self.splitter = FrameSplitter()
        self.frames = []          # [(seq, Frame)]
        self.phase = "init"
        self.heartbeats = 0
        self.garbage = []
        self.closed = False
        self.peer = None

    # ---- 生命周期 ----
    def connect(self, host, port, timeout_s):
        self.sock = socket.create_connection((host, port), timeout_s)
        self.sock.settimeout(0.05)   # 小超时用于轮询墙钟，不是业务超时
        self.peer = "%s:%d" % (host, port)

    def close(self):
        if self.sock is not None:
            try:
                self.sock.close()
            except OSError:
                pass
        self.sock = None
        self.closed = True

    # ---- 发送 ----
    def send_raw(self, data):
        if self.sock is None:
            raise ProbeError("send on closed conn %s" % self.label)
        self.sock.sendall(data)
        return data

    def send(self, recog, ident, param=0, tag=0, series=0, body=b""):
        frame = build_c2s_frame(recog, ident, param, tag, series, body)
        self.send_raw(frame)
        return frame

    def send_legacy(self, body):
        frame = build_legacy_c2s_frame(body)
        self.send_raw(frame)
        return frame

    # ---- 接收 ----
    def _handle_event(self, ev):
        if ev.kind == "heartbeat":
            self.heartbeats += 1
            try:
                self.send_raw(HEARTBEAT)
            except OSError:
                pass
            return None
        if ev.kind == "garbage":
            self.garbage.append(ev.data)
            raise ProbeError("conn %s: %d unclassified byte(s) outside frames: %s"
                             % (self.label, len(ev.data), ev.data[:32].hex()))
        frame = ev.frame
        row = self.recorder.write_frame(self.phase, self.label, frame)
        self.frames.append((row["seq"], frame))
        if self.on_frame is not None:
            self.on_frame(self, frame, row["seq"])
        return frame

    def pump(self, duration_ms):
        """读最多 duration_ms 的入站数据；返回本次新收的帧。"""
        end = time.time() + max(0, duration_ms) / 1000.0
        got = []
        while True:
            remain = end - time.time()
            if remain <= 0:
                break
            if self.sock is None:
                break
            try:
                data = self.sock.recv(65536)
            except socket.timeout:
                continue
            except OSError as ex:
                raise StepFailure("conn %s: recv failed: %r" % (self.label, ex))
            if not data:
                self.closed = True
                break
            for ev in self.splitter.feed(data):
                fr = self._handle_event(ev)
                if fr is not None:
                    got.append(fr)
        return got

    def next_seq(self):
        return self.recorder.seq + 1

    def wait_for(self, idents, timeout_ms, after_seq):
        """等 seq > after_seq 的第一条 ident ∈ idents 的帧；超时返回 None。"""
        wanted = set(idents)
        end = time.time() + timeout_ms / 1000.0
        while True:
            for seq, fr in self.frames:
                if seq > after_seq and fr.ident in wanted:
                    return seq, fr
            remain = end - time.time()
            if remain <= 0:
                return None
            self.pump(min(100, int(remain * 1000)))

    def wait_frames(self, count, idents, timeout_ms):
        """等累计帧数 >= count 且 idents 全部出现过（含等待前已收到的帧）。"""
        required = set(idents or [])
        end = time.time() + timeout_ms / 1000.0
        while True:
            seen = {fr.ident for _, fr in self.frames}
            if len(self.frames) >= count and required <= seen:
                return True
            remain = end - time.time()
            if remain <= 0:
                return False
            self.pump(min(100, int(remain * 1000)))


# ==========================================================================
# 场景 DSL
# ==========================================================================
STEP_META_KEYS = {"timeout_ms", "note", "delay_ms"}
ACTION_KEYS = {"send", "wait_for", "wait_frames", "sleep", "say", "click_npc", "dlg_select"}


def _need_int(value, where, lo=0, hi=0xFFFF):
    if not isinstance(value, int) or isinstance(value, bool):
        raise ScenarioError("%s must be an int, got %r" % (where, value))
    if value < lo or value > hi:
        raise ScenarioError("%s out of range [%d,%d]: %r" % (where, lo, hi, value))
    return value


def _need_keys(obj, where, required=(), optional=()):
    if not isinstance(obj, dict):
        raise ScenarioError("%s must be an object, got %r" % (where, type(obj).__name__))
    keys = set(obj)
    missing = set(required) - keys
    unknown = keys - set(required) - set(optional)
    if missing:
        raise ScenarioError("%s missing key(s): %s" % (where, sorted(missing)))
    if unknown:
        raise ScenarioError("%s has unknown key(s): %s (fail-closed)" % (where, sorted(unknown)))


def resolve_param(value, params, where):
    """把场景参数解析成 int：int 原样；str 先查 params，再按十进制数字。"""
    if isinstance(value, int) and not isinstance(value, bool):
        return value
    if isinstance(value, str):
        if value in params:
            return resolve_param(params[value], params, "params.%s" % value)
        try:
            return int(value, 10)
        except ValueError:
            raise ScenarioError("%s: cannot resolve %r (not a params name or int)" % (where, value))
    raise ScenarioError("%s must be int or params name, got %r" % (where, value))


def load_scenario(path):
    try:
        with open(path, "r", encoding="utf-8") as fh:
            data = json.load(fh)
    except OSError as ex:
        raise ScenarioError("cannot read scenario %s: %s" % (path, ex))
    except ValueError as ex:
        raise ScenarioError("scenario %s is not valid JSON: %s" % (path, ex))
    _need_keys(data, "scenario", required=("name", "steps"),
               optional=("description", "params"))
    if not isinstance(data["name"], str) or not data["name"]:
        raise ScenarioError("scenario.name must be a non-empty string")
    params = data.get("params") or {}
    if not isinstance(params, dict):
        raise ScenarioError("scenario.params must be an object")
    if not isinstance(data["steps"], list) or not data["steps"]:
        raise ScenarioError("scenario.steps must be a non-empty list")
    return data


def _encode_send_body(val, where):
    if "text" in val:
        text = val["text"]
        if not isinstance(text, str):
            raise ScenarioError("%s.text must be a string" % where)
        return encode_text(text, "gbk")
    raw = val["raw_hex"]
    if not isinstance(raw, str):
        raise ScenarioError("%s.raw_hex must be a hex string" % where)
    try:
        return bytes.fromhex(raw)
    except ValueError as ex:
        raise ScenarioError("%s.raw_hex is not valid hex: %s" % (where, ex))


def plan_step(step, index, params):
    """校验并把一步场景展开成可执行动作（不依赖运行期状态）。"""
    where = "steps[%d]" % index
    if not isinstance(step, dict):
        raise ScenarioError("%s must be an object" % where)
    actions = [k for k in step if k in ACTION_KEYS]
    if len(actions) != 1:
        raise ScenarioError("%s must have exactly one action key %s, got %s"
                            % (where, sorted(ACTION_KEYS), sorted(actions)))
    unknown = set(step) - ACTION_KEYS - STEP_META_KEYS
    if unknown:
        raise ScenarioError("%s has unknown key(s): %s (fail-closed)" % (where, sorted(unknown)))
    action = actions[0]
    timeout_ms = step.get("timeout_ms")
    if timeout_ms is not None:
        timeout_ms = _need_int(timeout_ms, where + ".timeout_ms", 0, 3600000)
    delay_ms = step.get("delay_ms")
    if delay_ms is not None:
        delay_ms = _need_int(delay_ms, where + ".delay_ms", 0, 3600000)
    note = step.get("note", "")
    if not isinstance(note, str):
        raise ScenarioError("%s.note must be a string" % where)
    out = {"index": index, "kind": action, "timeout_ms": timeout_ms,
           "delay_ms": delay_ms, "note": note}

    val = step[action]
    if action == "send":
        _need_keys(val, where + ".send", required=("ident",),
                   optional=("recog", "param", "tag", "series", "text", "raw_hex"))
        if "text" in val and "raw_hex" in val:
            raise ScenarioError("%s.send got both text and raw_hex" % where)
        out["ident"] = _need_int(val["ident"], where + ".send.ident")
        out["recog"] = val.get("recog", 0)
        out["param"] = val.get("param", 0)
        out["tag"] = val.get("tag", 0)
        out["series"] = val.get("series", 0)
        out["body"] = _encode_send_body(val, where + ".send") if ("text" in val or "raw_hex" in val) else b""
    elif action == "say":
        if not isinstance(val, str):
            raise ScenarioError("%s.say must be a string" % where)
        out.update(ident=CM_SAY, recog=0, param=0, tag=0, series=0,
                   body=encode_text(val, "gbk"))
    elif action == "click_npc":
        out.update(ident=CM_CLICKNPC, recog=resolve_param(val, params, where + ".click_npc"),
                   param=0, tag=0, series=0, body=b"")
    elif action == "dlg_select":
        if isinstance(val, str):
            label, recog = val, None
        else:
            _need_keys(val, where + ".dlg_select", required=("label",), optional=("recog",))
            label = val["label"]
            recog = val.get("recog")
        if not isinstance(label, str) or not label:
            raise ScenarioError("%s.dlg_select label must be a non-empty string" % where)
        out.update(ident=CM_MERCHANTDLGSELECT, recog=recog, param=0, tag=0, series=0,
                   body=encode_text(label, "gbk"), _needs_merchant_recog=(recog is None))
    elif action == "wait_for":
        if isinstance(val, int) and not isinstance(val, bool):
            idents = [_need_int(val, where + ".wait_for")]
        elif isinstance(val, list) and val:
            idents = [_need_int(x, where + ".wait_for[]") for x in val]
        else:
            raise ScenarioError("%s.wait_for must be an int or a non-empty list" % where)
        out["idents"] = idents
    elif action == "wait_frames":
        _need_keys(val, where + ".wait_frames", required=("count",),
                   optional=("idents", "timeout_ms"))
        out["count"] = _need_int(val["count"], where + ".wait_frames.count", 0, 1000000)
        ids = val.get("idents") or []
        if not isinstance(ids, list):
            raise ScenarioError("%s.wait_frames.idents must be a list" % where)
        out["idents"] = [_need_int(x, where + ".wait_frames.idents[]") for x in ids]
        if "timeout_ms" in val:
            out["timeout_ms"] = _need_int(val["timeout_ms"],
                                          where + ".wait_frames.timeout_ms", 0, 3600000)
    elif action == "sleep":
        if isinstance(val, int) and not isinstance(val, bool):
            out["ms"] = _need_int(val, where + ".sleep", 0, 3600000)
        else:
            _need_keys(val, where + ".sleep", required=("ms",))
            out["ms"] = _need_int(val["ms"], where + ".sleep.ms", 0, 3600000)
    return out


def expand_scenario(data, params_override=None):
    """校验整个场景，返回展开后的动作列表（运行期只差 dlg_select 的 recog）。"""
    params = dict(data.get("params") or {})
    if params_override:
        for k, v in params_override.items():
            params[k] = v
    plans = [plan_step(s, i, params) for i, s in enumerate(data["steps"])]
    return plans, params


# --------------------------------------------------------------------------
# 选角/网关地址解析（照 C# 的字段语义）
# --------------------------------------------------------------------------
def parse_char_list(body_text):
    """SM_QUERYCHR 体 -> [dict]；'*' 前缀表示上次选择的角色。"""
    fields = body_text.split("/")
    out = []
    for k in range(0, len(fields) - 4, 5):
        name = fields[k]
        if not name:
            continue
        out.append({
            "name": name[1:] if name.startswith("*") else name,
            "job": fields[k + 1], "hair": fields[k + 2],
            "level": fields[k + 3], "sex": fields[k + 4],
            "selected": name.startswith("*"),
        })
    return out


def first_char_name(body_text):
    chars = parse_char_list(body_text)
    return chars[0]["name"] if chars else None


def gamegate_login_body(account, char_name, session, version=CLIENT_VERSION_NUMBER):
    """BotSrv SendRunLogin 的明文字节（编码由 build_legacy_c2s_frame 做）。"""
    return ("**%s/%s/%s/%d/%d" % (account, char_name, session, version, version)).encode("gbk")


# ==========================================================================
# 运行器
# ==========================================================================
class Run:
    def __init__(self, args):
        self.args = args
        self.t0_ms = now_ms()
        self.start_wall = iso_wall(self.t0_ms)
        self.recorder = Recorder(args.out, self.t0_ms)
        self.steps = []
        self.auto = []
        self.unclassified = []
        self.exit_code = 0
        self.exit_reason = "completed"
        self.chain = {}
        self.frames_by_conn = {}
        self.ident_hist = {}
        self.state = {"last_merchant_recog": None, "self_actor": None}
        self.gamegate = None
        self.conns = []

    # ---- 自动握手（全在 GameGate 相位） ----
    def on_frame(self, conn, frame, seq):
        if conn.label != "gamegate":
            return
        if frame.ident == SM_MERCHANTSAY:
            self.state["last_merchant_recog"] = frame.recog
        if self.args.no_auto_handshake:
            return
        if frame.ident == SM_SENDNOTICE:
            raw = conn.send(0, CM_LOGINNOTICEOK)
            self.auto.append({"reason": "SM_SENDNOTICE->CM_LOGINNOTICEOK", "t_ms": now_ms() - self.t0_ms,
                              "ident": CM_LOGINNOTICEOK, "recog": 0, "frame_hex": raw.hex()})
        elif frame.ident == SM_LOGON:
            raw = conn.send(1, CM_QUERYBAGITEMS)
            self.auto.append({"reason": "SM_LOGON->CM_QUERYBAGITEMS", "t_ms": now_ms() - self.t0_ms,
                              "ident": CM_QUERYBAGITEMS, "recog": 1, "frame_hex": raw.hex()})

    def _log_send(self, conn, raw, plan, t_ms):
        self.steps.append({
            "index": plan["index"], "kind": "send", "t_ms": t_ms,
            "ident": plan.get("ident"), "recog": plan.get("recog"),
            "param": plan.get("param"), "tag": plan.get("tag"),
            "series": plan.get("series"), "body_len": len(plan.get("body") or b""),
            "body_hex": (plan.get("body") or b"").hex(),
            "frame_hex": raw.hex(), "frame_sha256": sha256_hex(raw),
            "note": plan.get("note", ""),
        })

    def execute_step(self, conn, plan):
        idx = plan["index"]
        kind = plan["kind"]
        default_timeout = self.args.timeout_ms
        if kind == "sleep":
            time.sleep(plan["ms"] / 1000.0)
            self.steps.append({"index": idx, "kind": "sleep", "ms": plan["ms"]})
            return
        if kind == "send" or kind in ("say", "click_npc", "dlg_select"):
            recog = plan.get("recog")
            if kind == "dlg_select" and plan.get("_needs_merchant_recog"):
                recog = self.state["last_merchant_recog"]
                if recog is None:
                    raise ScenarioError(
                        "steps[%d].dlg_select: no SM_MERCHANTSAY(643) seen yet; "
                        "cannot infer NPC recog (fail-closed)" % idx)
            raw = conn.send(recog, plan["ident"], plan.get("param", 0),
                            plan.get("tag", 0), plan.get("series", 0),
                            plan.get("body") or b"")
            self._log_send(conn, raw, dict(plan, recog=recog), now_ms() - self.t0_ms)
            return
        if kind == "wait_for":
            after = conn.recorder.seq
            t_start = now_ms()
            timeout = plan["timeout_ms"] if plan["timeout_ms"] is not None else default_timeout
            got = conn.wait_for(plan["idents"], timeout, after)
            rec = {"index": idx, "kind": "wait_for", "idents": plan["idents"],
                   "timeout_ms": timeout, "t_ms": t_start - self.t0_ms,
                   "waited_ms": now_ms() - t_start, "matched": got is not None,
                   "note": plan.get("note", "")}
            if got is None:
                self.steps.append(rec)
                raise StepFailure("steps[%d]: timeout waiting for ident(s) %s after %.0fms"
                                  % (idx, plan["idents"], timeout))
            seq, fr = got
            rec.update(matched_seq=seq, matched_ident=fr.ident, matched_recog=fr.recog,
                       matched_body_hex=fr.body.hex())
            self.steps.append(rec)
            return
        if kind == "wait_frames":
            timeout = plan["timeout_ms"] if plan.get("timeout_ms") is not None else default_timeout
            t_start = now_ms()
            ok = conn.wait_frames(plan["count"], plan["idents"], timeout)
            rec = {"index": idx, "kind": "wait_frames", "count": plan["count"],
                   "idents": plan["idents"], "timeout_ms": timeout,
                   "t_ms": t_start - self.t0_ms, "waited_ms": now_ms() - t_start,
                   "total_frames": len(conn.frames),
                   "seen_idents": sorted({fr.ident for _, fr in conn.frames}),
                   "matched": ok, "note": plan.get("note", "")}
            self.steps.append(rec)
            if not ok:
                raise StepFailure("steps[%d]: wait_frames count=%d idents=%s not met in %.0fms "
                                  "(have %d frames)"
                                  % (idx, plan["count"], plan["idents"], timeout,
                                     len(conn.frames)))
            return
        raise ScenarioError("steps[%d]: unhandled action %r" % (idx, kind))


def _chain_send(run, conn, recog, ident, body=b"", note=""):
    raw = conn.send(recog, ident, body=body)
    run.chain.setdefault("sends", []).append({
        "t_ms": now_ms() - run.t0_ms, "conn": conn.label, "ident": ident,
        "recog": recog, "body_len": len(body), "body_hex": bytes(body).hex(),
        "frame_hex": raw.hex(), "frame_sha256": sha256_hex(raw), "note": note,
    })
    return raw


def _decode_body_text(frame, what):
    try:
        return decode_text(frame.body_decoded, "gbk", "strict")
    except UnicodeDecodeError as ex:
        raise ProbeError("%s: body is not valid GBK: %s" % (what, ex))


def do_login_gate(run, args):
    """登录 + 选服，返回 (login_conn, selgate_addr, session)。"""
    conn = Conn("login", run.recorder)
    run.conns.append(conn)
    conn.phase = "login"
    host, port = parse_hostport(args.login_gate, "--login-gate")
    try:
        conn.connect(host, port, args.timeout_ms / 1000.0)
    except OSError as ex:
        raise StepFailure("login gate %s unreachable: %r" % (args.login_gate, ex))
    run.chain["login_gate"] = conn.peer
    body = ("%s/%s" % (args.account, args.password)).encode("gbk")
    after = conn.recorder.seq
    _chain_send(run, conn, 0, CM_IDPASSWORD, body, "CM_IDPASSWORD")
    got = conn.wait_for([SM_PASSOK_SELECTSERVER, SM_NEEDUPDATE_ACCOUNT],
                         args.timeout_ms, after)
    if got is None:
        raise StepFailure("login: no SM_PASSOK_SELECTSERVER(%d) within %dms"
                          % (SM_PASSOK_SELECTSERVER, args.timeout_ms))
    seq, fr = got
    run.chain["login_result"] = {"ident": fr.ident, "recog": fr.recog, "seq": seq}
    if fr.ident == SM_NEEDUPDATE_ACCOUNT:
        raise ProbeError("login: server demands SM_NEEDUPDATE_ACCOUNT(531); "
                         "password-update flow is not modelled (fail-closed)")

    # CM_SELECTSERVER
    after = conn.recorder.seq
    _chain_send(run, conn, 0, CM_SELECTSERVER, args.server_name.encode("gbk"), "CM_SELECTSERVER")
    got = conn.wait_for([SM_SELECTSERVER_OK], args.timeout_ms, after)
    if got is None:
        raise StepFailure("select-server: no SM_SELECTSERVER_OK(530) within %dms"
                          % args.timeout_ms)
    seq, fr = got
    text = _decode_body_text(fr, "SM_SELECTSERVER_OK")
    parts = text.split("/")
    if len(parts) < 3:
        raise ProbeError("SM_SELECTSERVER_OK body needs ip/port/session, got %r" % text)
    sg_ip, sg_port_s, session = parts[0], parts[1], parts[2]
    try:
        sg_port = int(sg_port_s)
    except ValueError:
        raise ProbeError("SM_SELECTSERVER_OK port is not an int: %r" % sg_port_s)
    if not (0 < sg_port < 65536):
        raise ProbeError("SM_SELECTSERVER_OK port out of range: %r" % sg_port_s)
    run.chain["select_server"] = {"body": text, "selgate": "%s:%d" % (sg_ip, sg_port),
                                  "session": session}
    if sg_ip in ("", "0.0.0.0"):
        sg_ip = host
        run.chain["select_server"]["host_substituted"] = host
    return conn, (sg_ip, sg_port), session


def do_select_char(run, args, selgate_addr, session):
    """SelGate 查询/选择角色，返回 (selgate_conn, char_name, gamegate_addr)。"""
    conn = Conn("selgate", run.recorder)
    run.conns.append(conn)
    conn.phase = "selgate"
    sg_ip, sg_port = selgate_addr
    try:
        conn.connect(sg_ip, sg_port, args.timeout_ms / 1000.0)
    except OSError as ex:
        raise StepFailure("selgate %s:%d unreachable: %r" % (sg_ip, sg_port, ex))
    run.chain["selgate"] = conn.peer
    after = conn.recorder.seq
    _chain_send(run, conn, 0, CM_QUERYCHR,
                ("%s/%s" % (args.account, session)).encode("gbk"), "CM_QUERYCHR")
    got = conn.wait_for([SM_QUERYCHR, SM_QUERYCHR_FAIL], args.timeout_ms, after)
    if got is None:
        raise StepFailure("query-chr: no SM_QUERYCHR(520) within %dms" % args.timeout_ms)
    seq, fr = got
    if fr.ident == SM_QUERYCHR_FAIL:
        raise StepFailure("query-chr: server returned SM_QUERYCHR_FAIL(527) "
                          "(session not accepted)")
    chars = parse_char_list(_decode_body_text(fr, "SM_QUERYCHR"))
    run.chain["char_list"] = chars
    names = [c["name"] for c in chars]
    if args.char_name:
        if args.char_name not in names:
            raise ProbeError("char %r not in server list %s (fail-closed)"
                             % (args.char_name, names))
        char_name = args.char_name
    else:
        if not names:
            raise StepFailure("query-chr: account has no characters")
        char_name = names[0]
    run.chain["char_name"] = char_name

    after = conn.recorder.seq
    _chain_send(run, conn, 0, CM_SELCHR,
                ("%s/%s" % (args.account, char_name)).encode("gbk"), "CM_SELCHR")
    got = conn.wait_for([SM_STARTPLAY, SM_STARTFAIL, SM_OUTOFCONNECTION],
                         args.timeout_ms, after)
    if got is None:
        raise StepFailure("select-chr: no SM_STARTPLAY(525) within %dms" % args.timeout_ms)
    seq, fr = got
    if fr.ident == SM_STARTFAIL:
        raise StepFailure("select-chr: server returned SM_STARTFAIL(526) (server full)")
    if fr.ident == SM_OUTOFCONNECTION:
        raise StepFailure("select-chr: server returned SM_OUTOFCONNECTION(528)")
    text = _decode_body_text(fr, "SM_STARTPLAY")
    parts = text.split("/")
    if len(parts) < 2:
        raise ProbeError("SM_STARTPLAY body needs ip/port, got %r" % text)
    gg_ip, gg_port_s = parts[0], parts[1]
    try:
        gg_port = int(gg_port_s)
    except ValueError:
        raise ProbeError("SM_STARTPLAY port is not an int: %r" % gg_port_s)
    if not (0 < gg_port < 65536):
        raise ProbeError("SM_STARTPLAY port out of range: %r" % gg_port_s)
    if gg_ip in ("", "0.0.0.0"):
        gg_ip = sg_ip
        run.chain["host_substituted_gamegate"] = gg_ip
    run.chain["gamegate"] = {"from": "SM_STARTPLAY", "addr": "%s:%d" % (gg_ip, gg_port)}
    return conn, char_name, (gg_ip, gg_port)


def do_enter_world(run, args, gamegate_addr, char_name, session):
    """GameGate 登录 + 等 SM_LOGON；返回 gamegate Conn。"""
    conn = Conn("gamegate", run.recorder, on_frame=run.on_frame)
    run.conns.append(conn)
    conn.phase = "gamegate"
    if args.game_gate:
        gg_ip, gg_port = parse_hostport(args.game_gate, "--game-gate")
        run.chain["gamegate"] = {"from": "argv", "addr": "%s:%d" % (gg_ip, gg_port)}
    else:
        gg_ip, gg_port = gamegate_addr
    try:
        conn.connect(gg_ip, gg_port, args.timeout_ms / 1000.0)
    except OSError as ex:
        raise StepFailure("gamegate %s:%d unreachable: %r" % (gg_ip, gg_port, ex))
    run.gamegate = conn
    body = gamegate_login_body(args.account, char_name, session, args.client_version)
    raw = conn.send_legacy(body)
    run.chain.setdefault("sends", []).append({
        "t_ms": now_ms() - run.t0_ms, "conn": conn.label, "legacy": True,
        "ident": None, "recog": None, "body_len": len(body), "body_hex": body.hex(),
        "frame_hex": raw.hex(), "frame_sha256": sha256_hex(raw),
        "note": "GameGate legacy ** login (no 12B header)",
    })
    after = conn.recorder.seq
    got = conn.wait_for([SM_LOGON, SM_OUTOFCONNECTION], args.timeout_ms, after)
    if got is None:
        raise StepFailure("enter-world: no SM_LOGON(%d) within %dms" % (SM_LOGON, args.timeout_ms))
    seq, fr = got
    if fr.ident == SM_OUTOFCONNECTION:
        raise StepFailure("enter-world: server returned SM_OUTOFCONNECTION(528)")
    run.chain["world_entered"] = {
        "seq": seq, "actor_id": fr.recog, "x": fr.param, "y": fr.tag,
        "series": fr.series, "body_hex": fr.body.hex(),
    }
    run.state["self_actor"] = fr.recog
    conn.pump(args.between_ms)  # 让紧随其后的 51/52/658 有机会进来（不强制）
    return conn


def do_finish(run, args, conn):
    mode = args.finish
    if mode == "softclose":
        try:
            _chain_send(run, conn, 0, CM_SOFTCLOSE, b"", "CM_SOFTCLOSE (小退，正常断开)")
        except OSError as ex:
            raise StepFailure("finish: softclose send failed: %r" % ex)
    if mode != "none" and args.drain_ms > 0:
        try:
            conn.pump(args.drain_ms)
        except ProbeError:
            raise
        except StepFailure:
            pass


def build_run_json(run, args, scenario, params, error):
    frames_total = run.recorder.seq
    by_conn = dict(run.recorder.counts)
    hist = dict(run.recorder.ident_hist)
    return {
        "tool": TOOL,
        "schema_version": SCHEMA_VERSION,
        "argv": masked_argv(getattr(args, "_argv", sys.argv)),
        "env": {
            "platform": sys.platform,
            "python": sys.version.split()[0],
            "cwd": os.getcwd(),
        },
        "start_wall": run.start_wall,
        "end_wall": iso_wall(),
        "wall_ms": now_ms() - run.t0_ms,
        "scenario": {
            "file": os.path.abspath(scenario["path"]) if scenario else None,
            "name": scenario["data"]["name"] if scenario else None,
            "params": params or {},
        },
        "chain": run.chain,
        "steps": run.steps,
        "auto": run.auto,
        "frames_total": frames_total,
        "frames_by_conn": by_conn,
        "ident_hist": hist,
        "unclassified": run.unclassified,
        "exit": {"code": run.exit_code, "reason": run.exit_reason,
                 "error": error},
    }


def masked_argv(argv):
    out = []
    hide = False
    for a in argv:
        if hide:
            out.append("***")
            hide = False
            continue
        if a == "--password":
            out.append(a)
            hide = True
            continue
        if a.startswith("--password="):
            out.append("--password=***")
            continue
        out.append(a)
    return out


# ==========================================================================
# CLI
# ==========================================================================
def parse_vars(items):
    out = {}
    for item in items or []:
        if "=" not in item:
            raise ScenarioError("--var must be NAME=VALUE, got %r" % item)
        k, _, v = item.partition("=")
        if not k:
            raise ScenarioError("--var has empty name: %r" % item)
        try:
            out[k] = int(v, 10)
        except ValueError:
            out[k] = v
    return out


def build_argparser():
    ap = argparse.ArgumentParser(
        prog="mir2_probe.py",
        description="Mir2 无头协议对拍驱动器（模拟客户端）")
    ap.add_argument("--login-gate", help="LoginGate HOST:PORT（如 127.0.0.1:7000）")
    ap.add_argument("--account")
    ap.add_argument("--password")
    ap.add_argument("--server-name", help="CM_SELECTSERVER 的服务器名（GBK 文本）")
    ap.add_argument("--char-name", help="选角；缺省取角色列表第一条")
    ap.add_argument("--scenario", help="场景 JSON 文件")
    ap.add_argument("--out", help="证据输出目录（frames.ndjson / run.json）")
    ap.add_argument("--timeout-ms", type=int, default=8000,
                    help="每次等待的默认超时（默认 8000）")
    ap.add_argument("--between-ms", type=int, default=200,
                    help="场景步骤之间的默认间隔（默认 200）")
    ap.add_argument("--drain-ms", type=int, default=300,
                    help="收尾/进世界后的额外收帧窗口（默认 300）")
    ap.add_argument("--game-gate", help="覆盖 SM_STARTPLAY 给的 GameGate 地址 HOST:PORT")
    ap.add_argument("--client-version", type=int, default=CLIENT_VERSION_NUMBER,
                    help="GameGate 登录串里的客户端版本号（默认 Grobal2.ClientVersionNumber）")
    ap.add_argument("--var", action="append", default=[],
                    help="覆盖场景 params：NAME=VALUE（可重复）")
    ap.add_argument("--no-auto-handshake", action="store_true",
                    help="不按 BotSrv 自动回 1018/81（调试用；对拍不要开）")
    ap.add_argument("--finish", choices=("softclose", "close", "none"), default="softclose",
                    help="收尾方式（默认 softclose = CM_SOFTCLOSE 后断开）")
    ap.add_argument("--selftest", action="store_true", help="离线自测（不连网）")
    return ap


def _check_required(args):
    missing = []
    for name in ("login_gate", "account", "password", "server_name", "scenario", "out"):
        if not getattr(args, name):
            missing.append("--" + name.replace("_", "-"))
    if missing:
        raise ProbeError("missing required argument(s): %s" % ", ".join(missing))


def main(argv=None):
    raw_argv = list(sys.argv) if argv is None else ["mir2_probe.py"] + list(argv)
    args = build_argparser().parse_args(argv)
    args._argv = raw_argv
    if args.selftest:
        return run_selftest()

    run = None
    scenario = None
    params = {}
    error = None
    try:
        _check_required(args)
        run = Run(args)
        data = load_scenario(args.scenario)
        plans, params = expand_scenario(data, parse_vars(args.var))
        scenario = {"path": args.scenario, "data": data}

        login_conn, selgate_addr, session = do_login_gate(run, args)
        sg_conn, char_name, gg_addr = do_select_char(run, args, selgate_addr, session)
        gg = do_enter_world(run, args, gg_addr, char_name, session)

        # BotSrv 在切场景后（延时 1000ms）关掉 Login/SelGate 连接；这里在进世界后关，
        # 顺序一致（GameGate 登录先完成），且不引入额外时序噪声。
        for c in (login_conn, sg_conn):
            c.close()
            run.chain.setdefault("closed", []).append(c.label)

        for i, plan in enumerate(plans):
            if i > 0:
                delay = plan["delay_ms"] if plan["delay_ms"] is not None else args.between_ms
                if delay:
                    time.sleep(delay / 1000.0)
            run.execute_step(gg, plan)
        do_finish(run, args, gg)
        run.exit_code, run.exit_reason = 0, "completed"
    except ScenarioError as ex:
        error = str(ex)
        if run:
            run.exit_code, run.exit_reason = 2, "scenario-error"
        else:
            print("mir2_probe: ScenarioError: %s" % ex, file=sys.stderr)
            return 2
    except FrameError as ex:
        error = str(ex)
        run.exit_code, run.exit_reason = 2, "frame-parse"
        run.unclassified.append({"kind": "frame", "detail": str(ex)})
    except ProbeError as ex:
        error = str(ex)
        run.exit_code, run.exit_reason = 2, "protocol/unclassified"
        run.unclassified.append({"kind": "probe", "detail": str(ex)})
    except StepFailure as ex:
        error = str(ex)
        run.exit_code, run.exit_reason = 1, "step-failed"
    except OSError as ex:
        error = repr(ex)
        run.exit_code, run.exit_reason = 1, "network"
    except Exception as ex:  # noqa: BLE001
        error = "%s: %s" % (type(ex).__name__, ex)
        run.exit_code, run.exit_reason = 2, "internal"
    finally:
        if run is not None:
            for c in run.conns:
                c.close()
            run.recorder.close()
            payload = build_run_json(run, args, scenario, params, error)
            os.makedirs(args.out, exist_ok=True)
            path = os.path.join(args.out, "run.json")
            with open(path, "w", encoding="utf-8") as fh:
                json.dump(payload, fh, ensure_ascii=False, indent=1)
            print("mir2_probe: exit=%d reason=%s frames=%d out=%s"
                  % (run.exit_code, run.exit_reason, run.recorder.seq, args.out))
            if error:
                print("  error: %s" % error, file=sys.stderr)
    return run.exit_code



# ==========================================================================
# 离线自测
# ==========================================================================
FIXTURE_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "fixtures", "r9")
SCENARIO_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "scenarios")


def _replay_bytes(data, chunks):
    """按给定 chunk 大小循环喂入，返回 (frames, heartbeats)；任何垃圾都算失败。"""
    sp = FrameSplitter()
    frames = []
    heartbeats = 0
    i = 0
    k = 0
    while i < len(data):
        n = chunks[k % len(chunks)]
        k += 1
        for ev in sp.feed(data[i:i + n]):
            if ev.kind == "heartbeat":
                heartbeats += 1
            elif ev.kind == "garbage":
                raise ProbeError("selftest: garbage bytes in fixture: %s" % ev.data[:16].hex())
            else:
                frames.append(ev.frame)
        i += n
    sp.eof()
    return frames, heartbeats


def _synthetic_stream():
    frames = [
        (2000, SM_SENDNOTICE, 0, 0, 0, b"notice"),
        (6930, 51, 5, 9, 0, b"B013"),
        (6930, SM_LOGON, 5, 9, 0, bytes(16)),
        (6930, 41, 0, 2, 0, b""),
        (0, SM_ABILITY, 25345, 0, 0, bytes(58)),
        (6930, 100, 65499, 0, 1, "欢迎".encode("gbk")),
    ]
    for i in range(9):
        frames.append((119, 10, 8, 10, i, b"2\x00\x0c\x00\x00\x00\x00\x00"))
    return b"".join(build_s2c_frame(*f) for f in frames), len(frames)


def _selftest_replay_fixture(name, expected_count, required, count_643=0, expected_plain=0):
    path = os.path.join(FIXTURE_DIR, name)
    if os.path.exists(path):
        with open(path, "rb") as fh:
            data = fh.read()
        source = path
    else:
        data, expected_count = _synthetic_stream()
        source = "<synthetic>"
    frames, heartbeats = _replay_bytes(data, [1, 2, 3, 5, 7, 11, 13, 17, 64])
    assert len(frames) == expected_count, "%s: %d frames != %d" % (source, len(frames), expected_count)
    idents = [f.ident for f in frames]
    for want in required:
        assert want in idents, "%s: required ident %d missing" % (source, want)
    if count_643:
        assert idents.count(SM_MERCHANTSAY) == count_643, \
            "%s: SM_MERCHANTSAY count %d != %d" % (source, idents.count(SM_MERCHANTSAY), count_643)

    import tempfile
    import shutil
    tmp = tempfile.mkdtemp(prefix="mir2probe-selftest-")
    try:
        rec = Recorder(tmp, now_ms())
        for f in frames:
            rec.write_frame("gamegate", "gamegate", f)
        rec.close()
        rows = []
        with open(rec.path, "r", encoding="utf-8") as fh:
            for line in fh:
                rows.append(json.loads(line))
        assert len(rows) == len(frames)
        last_seq = 0
        last_t = -1
        for row, fr in zip(rows, frames):
            for key in Recorder.FIELDS:
                assert key in row, "%s: ndjson row missing %s" % (source, key)
            assert row["seq"] > last_seq, "%s: seq not increasing" % source
            assert row["t_ms"] >= last_t, "%s: t_ms went backwards" % source
            last_seq, last_t = row["seq"], row["t_ms"]
            body = bytes.fromhex(row["body_hex"])
            assert len(body) == row["body_len"], "%s: body_len mismatch" % source
            assert sha256_hex(body) == row["body_sha256"], "%s: body_sha256 mismatch" % source
            assert row["ident"] == fr.ident and row["recog"] == fr.recog
            assert row["param"] == fr.param and row["tag"] == fr.tag
            assert row["series"] == fr.series
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    return (len(frames), heartbeats, source)


def _selftest_tamper():
    """改坏必红：fixture 里把一个载荷字节改成 0x00，切帧必须报错。"""
    path = os.path.join(FIXTURE_DIR, "conn-0001.s2c.bin")
    if os.path.exists(path):
        with open(path, "rb") as fh:
            data = bytearray(fh.read())
    else:
        data = bytearray(_synthetic_stream()[0])
    # 改 12B 头的第一段（'#' 后第 4 个字节）；body 允许原样分隔符，只有头是必编区。
    assert data[0:1] == b"#" and len(data) > 20, "selftest: fixture does not start with a frame"
    data[5] = 0x00
    sp = FrameSplitter()
    try:
        sp.feed(bytes(data))
    except FrameError:
        return
    raise AssertionError("tampered fixture was accepted (selftest tooth is broken)")


def _selftest_scenarios():
    paths = {n: os.path.join(SCENARIO_DIR, n)
             for n in ("login_only.json", "gm_time.json", "dice_chain.json")}
    for p in paths.values():
        assert os.path.exists(p), "missing scenario %s" % p
    lo = load_scenario(paths["login_only.json"])
    lo_plans, _ = expand_scenario(lo)
    assert len(lo_plans) == 1 and lo_plans[0]["kind"] == "wait_frames"
    assert lo_plans[0]["count"] == 15
    assert set(lo_plans[0]["idents"]) == {SM_LOGON, SM_ABILITY, SM_SENDNOTICE}

    gm = load_scenario(paths["gm_time.json"])
    gm_plans, _ = expand_scenario(gm)
    assert gm_plans[0]["kind"] == "send" and gm_plans[0]["ident"] == CM_SAY
    assert gm_plans[0]["body"] == encode_text("@时间", "gbk")
    assert gm_plans[1]["kind"] == "wait_for" and gm_plans[1]["idents"] == [SM_SYSMESSAGE]

    dice = load_scenario(paths["dice_chain.json"])
    dice_plans, dice_params = expand_scenario(dice)
    kinds = [p["kind"] for p in dice_plans]
    assert kinds == ["click_npc", "wait_for", "dlg_select", "wait_for",
                     "dlg_select", "wait_for", "dlg_select", "wait_for"], kinds
    assert dice_plans[0]["recog"] == dice_params["npc_actor"]
    for p in dice_plans[1::2]:
        assert p["idents"] == [SM_MERCHANTSAY]
    for p in dice_plans[2::2]:
        assert p["_needs_merchant_recog"] is True
    bodies = [p["body"] for p in dice_plans[2::2]]
    assert bodies == [encode_text(x, "gbk") for x in ("@kaiz", "@ya5", "@ya52")], bodies

    # fail-closed 负向：未知键 / text+raw_hex 同给 / 坏 hex / 解析不了的助记
    for bad_step in (
        {"send": {"ident": 3030, "text": "x"}, "bogus": 1},
        {"send": {"ident": 3030, "text": "x", "raw_hex": "00"}},
        {"send": {"ident": 3030, "raw_hex": "zz"}},
        {"click_npc": "no_such_param"},
        {"wait_frames": {"count": 1, "extra": 2}},
    ):
        try:
            plan_step(bad_step, 0, {})
        except ScenarioError:
            continue
        raise AssertionError("bad step accepted: %r" % (bad_step,))
    return len(paths)


def _selftest_helpers():
    body = gamegate_login_body("acct", "hero", "777")
    assert body == b"**acct/hero/777/1200409180/1200409180", body
    text = "*甲/0/1/22/0/乙/1/2/3/1/"
    chars = parse_char_list(text)
    assert [c["name"] for c in chars] == ["甲", "乙"], chars
    assert chars[0]["selected"] and not chars[1]["selected"]
    assert first_char_name(text) == "甲"
    assert parse_hostport("127.0.0.1:7000", "x") == ("127.0.0.1", 7000)
    for bad in ("127.0.0.1", ":7000", "h:0", "h:70000", "h:abc"):
        try:
            parse_hostport(bad, "x")
        except ProbeError:
            continue
        raise AssertionError("parse_hostport accepted %r" % bad)
    assert masked_argv(["p", "--password", "s3cret"]) == ["p", "--password", "***"]
    assert masked_argv(["p", "--password=s3cret"]) == ["p", "--password=***"]


def run_selftest():
    fails = []

    def check(name, fn):
        try:
            detail = fn()
            print("  [ok] %-24s %s" % (name, detail if detail is not None else ""))
        except AssertionError as ex:
            fails.append("%s: %s" % (name, ex))
        except Exception as ex:  # noqa: BLE001
            fails.append("%s: unexpected %s: %s" % (name, type(ex).__name__, ex))

    print("mir2_probe selftest (offline, no sockets):")
    check("fixture conn-0001.s2c", lambda: _selftest_replay_fixture(
        "conn-0001.s2c.bin", 44, [SM_LOGON, SM_ABILITY, SM_SENDNOTICE], expected_plain=2))
    check("fixture conn-0002.s2c", lambda: _selftest_replay_fixture(
        "conn-0002.s2c.bin", 106, [SM_LOGON, SM_ABILITY, SM_SENDNOTICE], count_643=4,
        expected_plain=6))
    check("tamper-red", _selftest_tamper)
    check("scenario-dsl", _selftest_scenarios)
    check("helpers", _selftest_helpers)
    check("conn-semantics", _selftest_conn_semantics)
    if fails:
        print("mir2_probe selftest: FAIL")
        for f in fails:
            print("  -", f)
        return 2
    print("mir2_probe selftest: OK (6 checks; fixtures replayed at 1/2/3/5/7/11/13/17/64-byte chunks)")
    return 0



class _FakeSock:
    """selftest 用：按块吐出预录字节，sendall 只记录（不占端口、不连网）。"""

    def __init__(self, chunks):
        self.chunks = list(chunks)
        self.sent = []

    def recv(self, n):
        if not self.chunks:
            raise socket.timeout()
        chunk = self.chunks.pop(0)
        if len(chunk) > n:
            self.chunks.insert(0, chunk[n:])
            chunk = chunk[:n]
        return chunk

    def sendall(self, data):
        self.sent.append(bytes(data))

    def close(self):
        pass


def _selftest_conn_semantics():
    import shutil
    import tempfile
    tmp = tempfile.mkdtemp(prefix="mir2probe-conn-")
    try:
        args = argparse.Namespace(out=tmp, no_auto_handshake=False)
        run = Run(args)
        frames = [
            build_s2c_frame(0, SM_SENDNOTICE, body=encode_text("notice")),
            build_s2c_frame(6930, SM_LOGON, 5, 9, 0, bytes(16)),
            build_s2c_frame(119, SM_MERCHANTSAY, 0, 0, 0, encode_text("@main")),
            build_s2c_frame(6930, SM_SYSMESSAGE, 1, 0, 0, encode_text("hi")),
        ]
        conn = Conn("gamegate", run.recorder, on_frame=run.on_frame)
        conn.phase = "gamegate"
        conn.sock = _FakeSock(frames)
        got = conn.pump(500)
        assert len(got) == 4, "pump got %d frames" % len(got)
        # 自动握手：658 -> 1018，50 -> 81（各一条）
        assert len(run.auto) == 2, run.auto
        auto_idents = [a["ident"] for a in run.auto]
        assert auto_idents == [CM_LOGINNOTICEOK, CM_QUERYBAGITEMS], auto_idents
        assert len(conn.sock.sent) == 2
        # 643 的 recog 被记住
        assert run.state["last_merchant_recog"] == 119
        # wait_for 只认 after_seq 之后的帧：第一条 100 在 after_seq 之前，不应命中
        first_100_seq = [s for s, f in conn.frames if f.ident == SM_SYSMESSAGE][0]
        assert conn.wait_for([SM_SYSMESSAGE], 50, first_100_seq) is None
        assert conn.wait_for([SM_SYSMESSAGE], 50, first_100_seq - 1) is not None
        # wait_frames 是累计口径（含等待前已收的帧）
        assert conn.wait_frames(4, [SM_SENDNOTICE, SM_LOGON], 50) is True
        assert conn.wait_frames(5, [SM_SENDNOTICE], 50) is False
        run.recorder.close()
        rows = [json.loads(line) for line in open(run.recorder.path, encoding="utf-8")]
        assert len(rows) == 4 and rows[0]["frame_class"] == "protocol"
        return "%d frames, 2 auto replies" % len(rows)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
