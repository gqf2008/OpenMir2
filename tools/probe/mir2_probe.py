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
  7. 进世界后先做**延迟段 dwell**（``--dwell-ms``，默认 1200ms，0 = 旧行为）：
     Rust 首个 SearchViewRange 在 enter_map+1000ms（crates/world/src/entity.rs 的
     PLAYER_SEARCH_TIME），dwell 不足会在延迟段帧（10/41/654/657/100…）到达之前
     CM_SOFTCLOSE 小退，对拍时被误判成「对侧独有」。
  8. 执行场景步骤（scenarios/README.md 的 DSL），全程落 ``frames.ndjson``。
  9. 收尾：默认发 CM_SOFTCLOSE(1009)（= BotSrv 的小退形状），再 drain 后断开。

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
import re
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
CM_TAKEONITEM = 1003
CM_SOFTCLOSE = 1009
CM_IDPASSWORD = 2001
CM_LOGINNOTICEOK = 1018
CM_SAY = 3030

SM_LOGON = 50
SM_TURN = 10
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
SM_MENU_OK = 767
SM_TAKEON_OK = 615
SM_TAKEON_FAIL = 616
SM_OUTOFCONNECTION = 528
SM_CHANGELIGHT = 654          # src/OpenMir2/Messages.cs（延迟段帧）
SM_CHARSTATUSCHANGED = 657    # src/OpenMir2/Messages.cs（延迟段帧）

CLIENT_VERSION_NUMBER = 1200409180  # src/OpenMir2/Grobal2.cs
HEARTBEAT = b"*"
# 进世界后的延迟段 dwell 默认值：Rust 首个 SearchViewRange 在 enter_map+1000ms
# （crates/world/src/entity.rs PLAYER_SEARCH_TIME），留 200ms 余量。
DEFAULT_DWELL_MS = 1200


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

    def wait_idents(self, idents, timeout_ms):
        """等 idents **全部**出现过（累计口径，含等待前已收到的帧）；超时返回差集。

        返回 (missing, waited_ms)：missing 为空 = 全部到齐。与 wait_for 的区别：
        wait_for 只认本步开始之后的第一条（任一命中），本方法不设 seq 下界、要求全到。
        """
        required = set(idents)
        t0 = time.time()
        deadline = t0 + timeout_ms / 1000.0
        while True:
            missing = sorted(required - {fr.ident for _, fr in self.frames})
            now = time.time()
            if not missing or now >= deadline:
                return missing, int((now - t0) * 1000)
            self.pump(min(100, max(1, int((deadline - now) * 1000))))


# ==========================================================================
# 场景 DSL
# ==========================================================================
STEP_META_KEYS = {"timeout_ms", "note", "delay_ms"}
ACTION_KEYS = {"send", "wait_for", "wait_frames", "wait_idents", "sleep",
               "say", "click_npc", "dlg_select"}


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
    # `computed` 是给 tools/probe/param_discovery.py 的**声明性注记**（场景声明 + 已推导量的算术），
    # 不是步骤 DSL：probe 不读它，无新 step 类型、不改 send/wait 语义。放行只是为了让带声明的场景
    # 仍能被 --check-scenario / 真跑加载（否则会被下面的 unknown key 拒绝）。
    _need_keys(data, "scenario", required=("name", "steps"),
               optional=("description", "params", "required", "computed",
                         "placement"))
    if not isinstance(data["name"], str) or not data["name"]:
        raise ScenarioError("scenario.name must be a non-empty string")
    params = data.get("params") or {}
    if not isinstance(params, dict):
        raise ScenarioError("scenario.params must be an object")
    if not isinstance(data["steps"], list) or not data["steps"]:
        raise ScenarioError("scenario.steps must be a non-empty list")
    req = data.get("required")
    if req is not None:
        if not isinstance(req, list):
            raise ScenarioError("scenario.required must be a list of var names")
        for name in req:
            if not isinstance(name, str) or not name:
                raise ScenarioError("scenario.required[] must be non-empty strings, got %r" % (name,))
        dup = sorted({n for n in req if req.count(n) > 1})
        if dup:
            raise ScenarioError("scenario.required has duplicates: %s" % dup)
    _check_placement(data.get("placement"))
    return data


def _check_placement(pl):
    """`scenario.placement` 的**形状**校验（本驱动器不做预置位，只负责别让坏声明溜过去）。

    预置位由 `tools/conformance/card_window.py` 在开窗前改**测试库**的
    `characters.MapName/CX/CY` 完成（卡 `openmir2-window-character-placement`）；
    这里只校验形状，并在声明存在时提示「本驱动器不应用它」——避免有人直接跑 probe
    就以为预置已经生效。
    """
    if pl is None:
        return
    _need_keys(pl, "scenario.placement", required=("map", "x", "y"), optional=("note",))
    if not isinstance(pl["map"], str) or not pl["map"]:
        raise ScenarioError("scenario.placement.map must be a non-empty string")
    if len(pl["map"]) > 10:
        raise ScenarioError("scenario.placement.map %r is longer than 10 chars "
                            "(characters.MapName is varchar(10))" % pl["map"])
    if not all(("a" <= c <= "z") or ("A" <= c <= "Z") or ("0" <= c <= "9") or c == "_"
               for c in pl["map"]):
        raise ScenarioError("scenario.placement.map %r must match [A-Za-z0-9_]{1,10} "
                            "(same rule as tools/conformance/card_window.py)" % pl["map"])
    _need_int(pl["x"], "scenario.placement.x", 0, 0xFFFF)
    _need_int(pl["y"], "scenario.placement.y", 0, 0xFFFF)
    if pl.get("note") is not None and not isinstance(pl["note"], str):
        raise ScenarioError("scenario.placement.note must be a string")
    print("note: scenario.placement 声明了 map=%s x=%s y=%s，但**本驱动器不预置位**"
          "（由 tools/conformance/card_window.py 在起栈前改测试库完成）"
          % (pl["map"], pl["x"], pl["y"]), file=sys.stderr)


def _encode_send_body(val, where, params=None):
    given = [k for k in ("text", "raw_hex", "text_from") if k in val]
    if len(given) != 1:
        raise ScenarioError("%s needs exactly one of text/raw_hex/text_from, got %s"
                            % (where, sorted(given)))
    if "text_from" in val:
        name = val["text_from"]
        if not isinstance(name, str) or not name:
            raise ScenarioError("%s.text_from must be a non-empty param name" % where)
        if not params or name not in params:
            raise ScenarioError("%s.text_from: param %r is not defined (fail-closed)" % (where, name))
        text = params[name]
        text = text if isinstance(text, str) else str(text)
        if not text:
            raise ScenarioError("%s.text_from: param %r is empty" % (where, name))
        return text.encode("gbk")   # 原始 GBK；EDCode 由 build_c2s_frame 统一做（别再预编码）
    if "text" in val:
        text = val["text"]
        if not isinstance(text, str):
            raise ScenarioError("%s.text must be a string" % where)
        # 字面量：不与 params 做任何替换（要参数化文本请用 text_from）
        return text.encode("gbk")   # 原始 GBK；EDCode 由 build_c2s_frame 统一做
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
                   optional=("recog", "param", "tag", "series", "text", "raw_hex", "text_from"))
        out["ident"] = _need_int(val["ident"], where + ".send.ident")
        # recog/param/tag/series 与其它动作同口径：先 resolve_param（可写 params 名），
        # 再在 plan 期做范围校验（recog=i32，其余 u16）。否则要等到发送时 pack_header
        # 才抛 FrameError，被归类成 frame-parse(2) 而不是 scenario-error(2)。PRB-4。
        recog = val.get("recog", 0)
        out["recog"] = _need_int(resolve_param(recog, params, where + ".send.recog"),
                                 where + ".send.recog", -0x80000000, 0x7FFFFFFF)
        for name in ("param", "tag", "series"):
            out[name] = _need_int(
                resolve_param(val.get(name, 0), params, where + ".send." + name),
                where + ".send." + name)
        out["body"] = (_encode_send_body(val, where + ".send", params)
                       if ("text" in val or "raw_hex" in val or "text_from" in val) else b"")
    elif action == "say":
        if not isinstance(val, str):
            raise ScenarioError("%s.say must be a string" % where)
        out.update(ident=CM_SAY, recog=0, param=0, tag=0, series=0,
                   body=val.encode("gbk"))
    elif action == "click_npc":
        if val == "auto:last_turn":
            out.update(ident=CM_CLICKNPC, recog=0, param=0, tag=0, series=0, body=b"",
                       _auto_recog="last_turn")
        else:
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
        # recog 与 send.recog 同口径（PRB-4）：可以写 params 名，加载期就解析并校验 i32 范围；
        # 否则要等到发送时才抛 FrameError，被归类成 frame-parse(2) 而不是 scenario-error(2)。
        if recog is not None:
            recog = _need_int(resolve_param(recog, params, where + ".dlg_select.recog"),
                              where + ".dlg_select.recog", -0x80000000, 0x7FFFFFFF)
        out.update(ident=CM_MERCHANTDLGSELECT, recog=recog, param=0, tag=0, series=0,
                   body=label.encode("gbk"), _needs_merchant_recog=(recog is None))
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
    elif action == "wait_idents":
        _need_keys(val, where + ".wait_idents", required=("idents",),
                   optional=("timeout_ms",))
        ids = val["idents"]
        if not isinstance(ids, list) or not ids:
            raise ScenarioError("%s.wait_idents.idents must be a non-empty list" % where)
        out["idents"] = [_need_int(x, where + ".wait_idents.idents[]") for x in ids]
        if "timeout_ms" in val:
            out["timeout_ms"] = _need_int(val["timeout_ms"],
                                          where + ".wait_idents.timeout_ms", 0, 3600000)
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
    missing = [n for n in (data.get("required") or []) if n not in params]
    if missing:
        raise ScenarioError(
            "scenario %r requires --var %s (declared in \"required\"); refusing to send guessed values"
            % (data.get("name"), ", ".join("%s=<value>" % n for n in missing)))
    plans = [plan_step(s, i, params) for i, s in enumerate(data["steps"])]
    return plans, params


# ==========================================================================
# 场景语义校验（--check-scenario 的「牙」）
# ==========================================================================
# 背景：load_scenario + expand_scenario 只做**形状**校验（键名/类型/范围/required 变量/
# text_from 是否存在）。它对「这个场景能不能真跑」无能为力：删掉中间的 send 步（等一个
# 永远不会来的回包）、把 wait_for 写成协议表里不存在的 ident，曾经都是 rc=0 —— 审查者
# 无法拿工具本身证伪场景。check_scenario() 补上这层语义校验。
REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
MESSAGES_RS_REL = "mir2-rs/crates/protocol/src/messages.rs"
MESSAGES_RS = os.path.join(REPO_ROOT, *MESSAGES_RS_REL.split("/"))

# 发帧动作：在 GameGate 连接上真的写出 c2s 帧（say/click_npc/dlg_select 分别展开成
# CM_SAY / CM_CLICKNPC / CM_MERCHANTDLGSELECT）。`send` 只是其中最通用的一个。
SEND_KINDS = frozenset({"send", "say", "click_npc", "dlg_select"})
# 累计观察动作：等的是「累计出现过」而不是「本步之后的第一条回包」——驱动器的登录链
# （CM_IDPASSWORD/SELECTSERVER/QUERYCHR/SELCHR + GameGate 登录）与自动握手
# （CM_LOGINNOTICEOK / CM_QUERYBAGITEMS）本来就会发帧，所以纯观察场景（login_only）合法。
CUMULATIVE_WAIT_KINDS = frozenset({"wait_idents", "wait_frames"})
WAIT_KINDS = frozenset({"wait_for"}) | CUMULATIVE_WAIT_KINDS

# 只认「整行就是一个常量定义」：先按行剥 `//` 注释、再锚定 `pub const ... ;`。
# 这样注释掉的常量（`// removed: pub const SM_FAKE: u16 = 4095;`）、字符串里的同名文本
# 都不会进表（M4：否则 wait_for 4095 会假绿）。
_MSG_CONST_RE = re.compile(r"pub const (CM|SM)_([A-Z0-9_]+): u16 = (\d+);\s*$")


def load_message_constants(path=None):
    """解析 `mir2-rs/crates/protocol/src/messages.rs` -> {"CM": {ident: 名}, "SM": {...}}。

    不写死一张私有表：这张表由 `cargo run -p msgcodegen -- --check` 与 C# `Messages.cs`
    对拍，是「ident 是否真实存在」的唯一事实源。同值别名（如 CM_IDPASSWORD/CM_PASSWORD）
    与跨命名空间同值（CM_QUERYCHR == SM_SYSMESSAGE == 100）都合法，先出现的名字胜出。
    行注释（`// ...`）先剥掉再匹配：被注释掉的常量不算数（M4）。
    fail-closed：文件缺失或一条常量都解析不到 => ScenarioError。
    """
    path = path or MESSAGES_RS
    try:
        with open(path, "r", encoding="utf-8") as fh:
            text = fh.read()
    except OSError as ex:
        raise ScenarioError("cannot read protocol constant table %s: %s" % (path, ex))
    out = {"CM": {}, "SM": {}}
    for raw in text.splitlines():
        m = _MSG_CONST_RE.match(raw.split("//", 1)[0].strip())
        if not m:
            continue
        out[m.group(1)].setdefault(int(m.group(3)), "%s_%s" % (m.group(1), m.group(2)))
    if not out["CM"] or not out["SM"]:
        raise ScenarioError("protocol constant table %s parsed %d CM / %d SM constants "
                            "(parser drift?)" % (path, len(out["CM"]), len(out["SM"])))
    return out


# 攻击族的方向语义由 game-svr 的 `attackwire.rs` 承载（不写私有表）：
#   * `pub const HIT_IDENTS: [u16; 9]` —— PlayObject.Message.cs:1221-1230 的 9 条攻击 ident；
#   * `const DIR_*: u8 = 0..7` —— Enums/Direction.cs 的 8 个方向；
#   * 模块头 `nDir = (byte)Tag` —— 所以判据是 `(tag & 0xFF) in DIR_*`（截断后再判）。
ATTACKWIRE_RS_REL = "mir2-rs/bins/game-svr/src/attackwire.rs"
ATTACKWIRE_RS = os.path.join(REPO_ROOT, *ATTACKWIRE_RS_REL.split("/"))
_HIT_IDENTS_BLOCK_RE = re.compile(r"pub const HIT_IDENTS:[^=]*=\s*\[(.*?)\];", re.S)
_HIT_IDENT_NAME_RE = re.compile(r"messages::(CM_[A-Z0-9_]+)")
_DIR_CONST_RE = re.compile(r"^\s*const (DIR_[A-Z0-9_]+): u8 = (\d+);\s*$", re.M)


def load_direction_spec(path=None, messages=None):
    """从 `mir2-rs/bins/game-svr/src/attackwire.rs` 读 (攻击族 ident 集合, Direction 值域)。

    复用现成源（不写死私有表）：`HIT_IDENTS` 里的名字用 messages.rs 的「名→号」解析。
    fail-closed：文件缺失 / 找不到 HIT_IDENTS / 名字解析不了 / 方向不是 0..7 连续 8 个。
    """
    path = path or ATTACKWIRE_RS
    if messages is None:
        messages = load_message_constants()
    try:
        with open(path, "r", encoding="utf-8") as fh:
            text = fh.read()
    except OSError as ex:
        raise ScenarioError("cannot read attack-family table %s: %s" % (path, ex))
    block = _HIT_IDENTS_BLOCK_RE.search(text)
    if not block:
        raise ScenarioError("%s: cannot find `pub const HIT_IDENTS: [u16; N] = [...]` "
                            "(parser drift?)" % path)
    name_to_ident = {}
    for ns in ("CM", "SM"):
        for ident, name in messages[ns].items():
            name_to_ident[name] = ident
    idents = set()
    for name in _HIT_IDENT_NAME_RE.findall(block.group(1)):
        if name not in name_to_ident:
            raise ScenarioError("%s: HIT_IDENTS references %s, which is not in %s"
                                % (path, name, MESSAGES_RS_REL))
        idents.add(name_to_ident[name])
    if not idents:
        raise ScenarioError("%s: HIT_IDENTS parsed 0 idents (parser drift?)" % path)
    dirs = sorted(int(v) for _, v in _DIR_CONST_RE.findall(text))
    if dirs != list(range(8)):
        raise ScenarioError("%s: DIR_* constants parsed as %s, expected 0..7 (parser drift?)"
                            % (path, dirs))
    return idents, frozenset(dirs)


def _var_refs(data):
    """收集场景里**按变量名**引用的字段：[(where, name)]。

    与 plan_step 的解析点一一对应（send 的 recog/param/tag/series、click_npc、
    dlg_select.recog、send.text_from）。纯数字字面量（含 "0" 这种字符串）不算变量引用；
    click_npc 的 "auto:last_turn" 是动作级哨兵，也不是变量。
    """
    refs = []
    for i, step in enumerate(data.get("steps") or []):
        if not isinstance(step, dict):
            continue
        where = "steps[%d]" % i
        send = step.get("send")
        if isinstance(send, dict):
            for key in ("recog", "param", "tag", "series"):
                v = send.get(key)
                if isinstance(v, str):
                    refs.append((where + ".send." + key, v))
            v = send.get("text_from")
            if isinstance(v, str):
                refs.append((where + ".send.text_from", v))
        v = step.get("click_npc")
        if isinstance(v, str) and v != "auto:last_turn":
            refs.append((where + ".click_npc", v))
        v = step.get("dlg_select")
        if isinstance(v, dict) and isinstance(v.get("recog"), str):
            refs.append((where + ".dlg_select.recog", v["recog"]))
    return refs


#: 由 `scenario.placement` 派生、必须与它自洽的变量：变量名 -> (比较口径, 说明)。
#:   * `self_pos`：按 `MakeLong(X, Y) = (unsigned short)X | (Y << 16)` 打包（HUtil32.MakeLong
#:     口径，docs/HUtil32-parity-inventory-20261020.md 第 9 条），低 16 位 X、高 16 位 Y；
#:   * `self_x` / `self_y`：自身坐标，必须分别等于 placement.x / placement.y。
PLACEMENT_DERIVED_VARS = {
    "self_pos": ("packed", "MakeLong(X,Y)=(Y<<16)|X"),
    "self_x": ("x", "自身坐标 X"),
    "self_y": ("y", "自身坐标 Y"),
}


def _decode_self_pos(value):
    """MakeLong(X,Y) -> (x, y)：低 16 位 X、高 16 位 Y（HUtil32.MakeLong 口径）。"""
    return value & 0xFFFF, (value >> 16) & 0xFFFF


def _is_int_literal(text):
    try:
        int(text, 10)
        return True
    except ValueError:
        return False


def check_scenario(data, plans, params=None, messages=None):
    """`--check-scenario` 的语义校验（形状之外）。非法即抛 ScenarioError。

    已有形状校验（load_scenario/expand_scenario）：键名/类型/范围、恰好一个动作键、
    required 变量是否给全、text_from 是否在 params 里。本函数补「这场景真跑得起来吗」：

      R2 每个 wait_for 之前必须已有发帧步骤（否则它等的回包没人产生）；
      R1 场景至少要有一个能发帧（send/say/click_npc/dlg_select）或能累计观察
         （wait_idents/wait_frames）的步骤 —— 全是 sleep 的场景什么也证不了；
      R3 所有 send/wait_* 的 ident 必须存在于协议常量表（表外 ident = 永远发不出/等不到）；
      R4 变量引用必须能在场景声明的 required/params/computed 里找到（不许引用未声明变量）；
      R5 `scenario.placement` 与由位置派生的 params（如 self_pos=MakeLong(X,Y)）必须自洽
         （不一致 = 角色落在 A 点、期望值按 B 点算，真跑静默恒超时，card_window dry-run 也看不出来）；
      R6（**WARN 级，不改 rc**）攻击族（HIT_IDENTS 9 条）的 `send.tag` 按 oracle `(byte)Tag = nDir`
         截断后应是合法 Direction（0..7）。越界只打 stderr 警告：它**不影响本次命中**（取目标用
         状态化 Dir，nDir 之后才写入），但会污染状态 Dir（动作帧/此后取目标），且真实客户端不会发；
      W  至少要有一个等待步骤（原 main 里的 N-5 规则，收进来一起判）。

    R4 的「声明」口径刻意**不含 `--var`**：`--var` 是给已声明变量覆值的通道（README），
    不是声明通道 —— 否则把一个拼错的变量名用 `--var` 一喂，校验就绿了。
    """
    name = data.get("name")
    if messages is None:
        messages = load_message_constants()
    # 生效参数 = 场景 params + --var 覆盖（R5 用；R4 的「声明」口径刻意不含 --var）。
    eff_params = dict(data.get("params") or {})
    if params:
        eff_params.update(params)
    known = set(messages["CM"]) | set(messages["SM"])
    declared = (set((data.get("params") or {}).keys())
                | set(data.get("required") or [])
                | set((data.get("computed") or {}).keys()))

    # R2：wait_for 之前必须已有发帧步骤。先判这一步，坏样例拿到最具体的诊断。
    first_send = None
    for p in plans:
        if p["kind"] in SEND_KINDS:
            if first_send is None:
                first_send = p["index"]
        elif p["kind"] == "wait_for" and first_send is None:
            raise ScenarioError(
                "steps[%d].wait_for %s: no send step precedes it — nothing in this scenario can "
                "produce that response (a scenario that only waits proves nothing; add a "
                "send/say/click_npc/dlg_select before it)" % (p["index"], p["idents"]))

    kinds = [p["kind"] for p in plans]

    # R1：至少一个发帧或累计观察步骤。
    if not any(k in SEND_KINDS or k in CUMULATIVE_WAIT_KINDS for k in kinds):
        raise ScenarioError(
            "scenario %r has no send step (%s) and no cumulative wait step (%s): "
            "steps contain only %s, so it neither triggers nor observes any frame"
            % (name, "/".join(sorted(SEND_KINDS)), "/".join(sorted(CUMULATIVE_WAIT_KINDS)),
               "/".join(sorted(set(kinds)))))

    # R3：ident 必须存在于协议常量表。
    for p in plans:
        if p["kind"] == "send":
            if p["ident"] not in known:
                raise ScenarioError(
                    "steps[%d].send.ident %d is not a CM_*/SM_* constant in %s"
                    % (p["index"], p["ident"], MESSAGES_RS_REL))
        elif p["kind"] in WAIT_KINDS:
            for ident in p["idents"]:
                if ident not in known:
                    raise ScenarioError(
                        "steps[%d].%s ident %d is not a CM_*/SM_* constant in %s "
                        "(typo? this wait can never match)"
                        % (p["index"], p["kind"], ident, MESSAGES_RS_REL))

    # R4：变量引用必须在场景里声明过。
    for where, ref in _var_refs(data):
        if _is_int_literal(ref):
            continue
        if ref not in declared:
            raise ScenarioError(
                "%s references undeclared variable %r: declare it in required/params/computed "
                "(--var only overrides a declared name, it does not declare one)" % (where, ref))

    # R5：placement 与由位置派生的 params 必须自洽。取值与 plan_step 同口径走
    # resolve_param（**支持 params 名间接引用/链式**，否则 `"self_pos": "start_pos"` 会假红）。
    placement = data.get("placement")
    if isinstance(placement, dict):
        for var, (how_kind, how) in PLACEMENT_DERIVED_VARS.items():
            if var not in eff_params:
                continue
            value = resolve_param(eff_params[var], eff_params, "params." + var)
            if how_kind == "packed":
                x, y = _decode_self_pos(value)
                if (x, y) != (placement["x"], placement["y"]):
                    raise ScenarioError(
                        "scenario.placement map=%s x=%d y=%d disagrees with %s=%d "
                        "(decodes to x=%d y=%d via %s): the character would be placed at one point "
                        "while the scenario expects another (silent timeout at run time)"
                        % (placement["map"], placement["x"], placement["y"], var, value, x, y, how))
            elif value != placement[how_kind]:
                raise ScenarioError(
                    "scenario.placement map=%s x=%d y=%d disagrees with %s=%d (%s must equal "
                    "placement.%s): the character would be placed at one point while the scenario "
                    "expects another (silent timeout at run time)"
                    % (placement["map"], placement["x"], placement["y"], var, value, var, how_kind))

    # R6（**WARN 级，不改变 rc**）：攻击族 send.tag = (byte)wParam = 状态化朝向。
    # 机理（两侧一致，别写反）：oracle **先按状态化 Dir 取目标**，之后才写入 nDir ——
    #   C#：PlayObject.Attack.cs:26 GetPoseCreate()（状态 Dir）→ :75 AttackDir(...) →
    #       BaseObject.Attack.cs:12 `Dir = nDir;` → :13 `_Attack`（用已选好的目标）；
    #   Rust：bins/game-svr/src/lib.rs:8836-8848 `pose_dir = e.ai.dir` 取目标，随后 `e.ai.dir = dir`。
    # ⇒ 越界方向**不影响本击命中**（CM_TURN(tag=0)+CM_HIT(tag=99) 第一击照样命中并出 SM_STRUCK），
    #   它污染的是**此后**的状态 Dir（动作帧 + 下一次取目标）。所以这里只警告、不判死。
    direction_spec = None
    for p in plans:
        if p["kind"] != "send":
            continue
        if direction_spec is None:
            direction_spec = load_direction_spec(messages=messages)
        hit_idents, dirs = direction_spec
        if p["ident"] not in hit_idents:
            continue
        eff_dir = p["tag"] & 0xFF          # oracle: (byte)Tag
        if eff_dir not in dirs:
            print("WARN: steps[%d].send.tag %d (ident %d, attack family) is not a valid Direction: "
                  "oracle takes nDir = (byte)Tag = %d, valid %s (source %s HIT_IDENTS/DIR_*). "
                  "真实客户端不会发送越界方向；该值**不影响本次命中**（取目标用的是状态化 Dir，"
                  "nDir 在取目标之后才写入），但会污染角色的状态 Dir，影响动作帧与**此后**的取目标。"
                  % (p["index"], p["tag"], p["ident"], eff_dir,
                     "%d..%d" % (min(dirs), max(dirs)), ATTACKWIRE_RS_REL), file=sys.stderr)

    # W：必须有等待步骤（N-5：只发不等证明不了任何东西）。
    if not any(k in WAIT_KINDS for k in kinds):
        raise ScenarioError(
            "scenario %r has no wait step: at least one wait_for/wait_frames/wait_idents is required "
            "(a send-only scenario proves nothing)" % name)
    return True


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
        self.state = {"last_merchant_recog": None, "self_actor": None, "last_turn_recog": None}
        self.gamegate = None
        self.conns = []
        self.dwell = None

    # ---- 自动握手（全在 GameGate 相位） ----
    def on_frame(self, conn, frame, seq):
        if conn.label != "gamegate":
            return
        if frame.ident == SM_MERCHANTSAY:
            self.state["last_merchant_recog"] = frame.recog
        if frame.ident == SM_TURN and frame.recog != self.state.get("self_actor"):
            # 进世界后首个「非自己」的 actor（固定 NPC/怪）：click_npc 的 "auto:last_turn" 用它
            self.state["last_turn_recog"] = frame.recog
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
            if kind == "click_npc" and plan.get("_auto_recog") == "last_turn":
                recog = self.state.get("last_turn_recog")
                if recog is None:
                    raise ScenarioError(
                        "steps[%d].click_npc: no non-self SM_TURN(10) seen yet; "
                        "cannot infer NPC recog (fail-closed)" % idx)
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
        if kind == "wait_idents":
            timeout = plan["timeout_ms"] if plan["timeout_ms"] is not None else default_timeout
            t_start = now_ms()
            missing, waited = conn.wait_idents(plan["idents"], timeout)
            seen = sorted({fr.ident for _, fr in conn.frames})
            rec = {"index": idx, "kind": "wait_idents", "idents": plan["idents"],
                   "timeout_ms": timeout, "t_ms": t_start - self.t0_ms,
                   "waited_ms": waited, "seen_idents": seen,
                   "missing_idents": missing, "matched": not missing,
                   "note": plan.get("note", "")}
            self.steps.append(rec)
            if missing:
                raise StepFailure(
                    "steps[%d]: timeout waiting for ident(s) %s after %.0fms "
                    "(missing %s; seen %s)"
                    % (idx, plan["idents"], timeout, missing, seen))
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
    run.dwell = dwell_after_enter(conn,
                                  getattr(args, "dwell_ms", DEFAULT_DWELL_MS),
                                  getattr(args, "between_ms", 0))
    return conn


def dwell_after_enter(conn, dwell_ms, legacy_pump_ms=0):
    """进世界后的延迟段 dwell，返回落 run.json 的记录。

    ``dwell_ms > 0``：至少 ``dwell_ms`` 的收帧窗口（纯墙钟下限，不看 ident）；
    ``dwell_ms == 0``：旧行为 —— 只 ``pump(legacy_pump_ms)``（= ``--between-ms``）。
    为什么需要：Rust 首个 SearchViewRange 在 enter_map+1000ms
    （crates/world/src/entity.rs 的 PLAYER_SEARCH_TIME），进世界后立刻小退会把
    整段延迟帧（10/41/654/657/100…）丢掉，对拍时被误判成对侧独有。
    """
    requested = max(0, int(dwell_ms or 0))
    legacy = 0 if requested > 0 else max(0, int(legacy_pump_ms or 0))
    before = sorted({fr.ident for _, fr in conn.frames})
    t_start = now_ms()
    if requested > 0:
        conn.pump(requested)
    elif legacy > 0:
        conn.pump(legacy)
    after = sorted({fr.ident for _, fr in conn.frames})
    return {
        "requested_ms": requested,
        "legacy_pump_ms": legacy,
        "actual_ms": now_ms() - t_start,
        "idents_before": before,
        "idents_after": after,
        "new_idents": sorted(set(after) - set(before)),
        "frames_total": len(conn.frames),
    }


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
        "dwell": run.dwell,
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
                    help="收尾（CM_SOFTCLOSE 之后）的额外收帧窗口（默认 300）")
    ap.add_argument("--dwell-ms", type=int, default=DEFAULT_DWELL_MS,
                    help="进世界后的延迟段收帧窗口下限（毫秒，默认 %d，必须 >= 0）；"
                         "0 = 旧行为（只 pump --between-ms，会丢延迟段帧）；"
                         "负值 fail-closed（退出 2）"
                         % DEFAULT_DWELL_MS)
    ap.add_argument("--game-gate", help="覆盖 SM_STARTPLAY 给的 GameGate 地址 HOST:PORT")
    ap.add_argument("--client-version", type=int, default=CLIENT_VERSION_NUMBER,
                    help="GameGate 登录串里的客户端版本号（默认 Grobal2.ClientVersionNumber）")
    ap.add_argument("--var", action="append", default=[],
                    help="覆盖场景 params：NAME=VALUE（可重复）")
    ap.add_argument("--no-auto-handshake", action="store_true",
                    help="不按 BotSrv 自动回 1018/81（调试用；对拍不要开）")
    ap.add_argument("--finish", choices=("softclose", "close", "none"), default="softclose",
                    help="收尾方式（默认 softclose = CM_SOFTCLOSE 后断开）")
    ap.add_argument("--check-scenario", action="store_true",
                    help="只加载+展开场景做校验（不连网）：rc=0 通过 / rc=2 场景错误")
    ap.add_argument("--selftest", action="store_true", help="离线自测（不连网）")
    return ap


def _check_required(args):
    missing = []
    for name in ("login_gate", "account", "password", "server_name", "scenario", "out"):
        if not getattr(args, name):
            missing.append("--" + name.replace("_", "-"))
    if missing:
        raise ProbeError("missing required argument(s): %s" % ", ".join(missing))
    if getattr(args, "dwell_ms", 0) < 0:
        raise ProbeError("--dwell-ms must be >= 0 (0 = 旧行为), got %d" % args.dwell_ms)


def main(argv=None):
    raw_argv = list(sys.argv) if argv is None else ["mir2_probe.py"] + list(argv)
    args = build_argparser().parse_args(argv)
    args._argv = raw_argv
    if args.selftest:
        return run_selftest()
    if args.check_scenario:
        if not args.scenario:
            print("mir2_probe: --check-scenario needs --scenario", file=sys.stderr)
            return 2
        try:
            data = load_scenario(args.scenario)
            plans, params = expand_scenario(data, parse_vars(args.var))
            check_scenario(data, plans, params)
        except ScenarioError as ex:
            print("scenario-error: %s" % ex, file=sys.stderr)
            return 2
        print("scenario OK: %s (%d steps, params=%s)"
              % (data["name"], len(plans), ",".join("%s=%s" % (k, v) for k, v in sorted(params.items())) or "-"))
        return 0

    # 参数校验先于 Run/Recorder 创建：缺参数时还没有 --out 可落 run.json，
    # 按 README「退出码」退 2（fail-closed）；绝不能走到 run=None 的 AttributeError
    #（那是未捕获异常，退出码会变成 1）。PRB-2。
    try:
        _check_required(args)
    except ProbeError as ex:
        print("mir2_probe: %s" % ex, file=sys.stderr)
        return 2
    try:
        run = Run(args)
    except OSError as ex:
        print("mir2_probe: cannot create evidence dir %r: %r" % (args.out, ex),
              file=sys.stderr)
        return 2

    scenario = None
    params = {}
    error = None
    try:
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
        run.exit_code, run.exit_reason = 2, "scenario-error"
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

    # 明文动作帧必须**被分类**而不是解析失败（PRB-1/PRB-3）：fixture 里的 +GD ack 是
    # Rust 13 位 epoch-ms 形状；C# 的 <=10 位形状由 mir2frame selftest 的
    # plain-csharp-shapes-red 牙单独钉住，这里钉住「条数」不许漂。
    if expected_plain:
        plains = [f for f in frames if f.frame_class == "server_ack_time"]
        assert len(plains) == expected_plain, (
            "%s: server_ack_time count %d != %d" % (source, len(plains), expected_plain))
        assert all(f.plain and f.ident is None for f in plains), (
            "%s: server_ack_time frame is not plain" % source)
        assert all(f.body.startswith(b"+GD/") for f in plains), (
            "%s: server_ack_time body does not start with +GD/" % source)

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
    assert len(lo_plans) == 1 and lo_plans[0]["kind"] == "wait_idents", lo_plans
    # 牙：等待集合必须含延迟段帧 654/657/100（退回「固定帧数就小退」这条会红）。
    assert set(lo_plans[0]["idents"]) == {
        SM_LOGON, SM_ABILITY, SM_SENDNOTICE,
        SM_CHANGELIGHT, SM_CHARSTATUSCHANGED, SM_SYSMESSAGE}, lo_plans[0]

    gm = load_scenario(paths["gm_time.json"])
    gm_plans, _ = expand_scenario(gm)
    assert gm_plans[0]["kind"] == "send" and gm_plans[0]["ident"] == CM_SAY, gm_plans[0]
    assert gm_plans[0]["body"] == "@时间".encode("gbk"), gm_plans[0]["body"]
    assert gm_plans[1]["kind"] == "wait_for", gm_plans[1]
    assert gm_plans[1]["idents"] == [SM_SYSMESSAGE], gm_plans[1]

    dice = load_scenario(paths["dice_chain.json"])
    dice_plans, dice_params = expand_scenario(dice)
    kinds = [p["kind"] for p in dice_plans]
    assert kinds == ["click_npc", "wait_for", "dlg_select", "wait_for",
                     "dlg_select", "wait_for", "dlg_select", "wait_for"], kinds
    assert dice_plans[0]["recog"] == dice_params["npc_actor"], dice_plans[0]
    # 末步是分支相关：持筹码包回 643、无包 #ELSEACT MESSAGEBOX 回 767（R-3）
    for p in dice_plans[1:-1:2]:
        assert p["idents"] == [SM_MERCHANTSAY], (p["index"], p["idents"])
    assert dice_plans[-1]["idents"] == [SM_MERCHANTSAY, SM_MENU_OK], dice_plans[-1]
    for p in dice_plans[2::2]:
        assert p["_needs_merchant_recog"] is True, (p["index"], p.get("_needs_merchant_recog"))
    bodies = [p["body"] for p in dice_plans[2::2]]
    assert bodies == [x.encode("gbk") for x in ("@kaiz", "@ya5", "@ya52")], bodies

    # fail-closed 负向：未知键 / text+raw_hex 同给 / 坏 hex / 解析不了的助记
    for bad_step in (
        {"send": {"ident": 3030, "text": "x"}, "bogus": 1},
        {"send": {"ident": 3030, "text": "x", "raw_hex": "00"}},
        {"send": {"ident": 3030, "raw_hex": "zz"}},
        {"click_npc": "no_such_param"},
        {"wait_frames": {"count": 1, "extra": 2}},
        {"wait_idents": {"idents": []}},
        {"wait_idents": {"idents": [SM_CHANGELIGHT], "bogus": 1}},
        {"wait_idents": {"idents": [70000]}},
    ):
        try:
            plan_step(bad_step, 0, {})
        except ScenarioError:
            continue
        raise AssertionError("bad step accepted: %r" % (bad_step,))

    # PRB-4：send 的 recog/param/tag/series 也走 resolve_param，且 plan 期就校验范围，
    # 错误必须落在 scenario-error(2)，不能拖到发送时变成 frame-parse。
    sp = plan_step({"send": {"ident": 1010, "recog": "npc_actor",
                             "param": "one", "tag": 5, "series": "one"}},
                   0, {"npc_actor": 119, "one": 1})
    assert (sp["recog"], sp["param"], sp["tag"], sp["series"]) == (119, 1, 5, 1), sp
    for bad in ({"send": {"ident": 1010, "param": 70000}},
                {"send": {"ident": 1010, "tag": -1}},
                {"send": {"ident": 1010, "recog": "no_such_param"}},
                {"send": {"ident": 1010, "recog": 0x80000000}}):
        try:
            plan_step(bad, 0, {})
        except ScenarioError:
            continue
        raise AssertionError("bad send accepted: %r" % (bad,))
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


def _selftest_scenario_required():
    """`required` 元数据 + `send.text` 走 params 的口径（F1/F2 的回归）。"""
    import tempfile
    base = {
        "name": "selftest_required",
        "description": "required 元数据 + send.text params",
        "required": ["make_index", "item_name"],
        "steps": [
            {"send": {"ident": CM_TAKEONITEM, "recog": "make_index", "param": 1, "text_from": "item_name"}},
            {"wait_for": [SM_TAKEON_OK, SM_TAKEON_FAIL]},
        ],
    }
    fd, path = tempfile.mkstemp(suffix=".json")
    os.close(fd)
    try:
        with open(path, "w", encoding="utf-8") as fh:
            json.dump(base, fh, ensure_ascii=False)
        data = load_scenario(path)
        # 1) 缺 required ⇒ 加载期即报错（与 CLI rc=2 同一条路径），并点名缺的是哪个变量
        try:
            expand_scenario(data, {})
        except ScenarioError as ex:
            assert "--var make_index" in str(ex), str(ex)
            assert "item_name" in str(ex), str(ex)
        else:
            raise AssertionError("expand_scenario accepted missing required vars")
        # 2) 给全 ⇒ 展开成功，text_from 解析成参数值（GBK+EDCode）
        plans, _ = expand_scenario(data, {"make_index": 42, "item_name": "屠龙"})
        assert plans[0]["recog"] == 42, plans[0]
        assert plans[0]["body"] == "屠龙".encode("gbk"), plans[0]["body"]
        assert plans[1]["idents"] == [SM_TAKEON_OK, SM_TAKEON_FAIL], plans[1]
        # 3) text 不是 params 名时保持字面量
        lit = dict(base, required=[], steps=[{"send": {"ident": CM_SAY, "text": "@时间"}}])
        with open(path, "w", encoding="utf-8") as fh:
            json.dump(lit, fh, ensure_ascii=False)
        plans, _ = expand_scenario(load_scenario(path), {"@时间": "不该命中"})
        assert plans[0]["body"] == "@时间".encode("gbk"), plans[0]["body"]
        # 3b) 只有 send、没有任何 wait 的场景必须被 --check-scenario 拒绝（N-5）
        sendonly = {"name": "sendonly", "steps": [{"send": {"ident": CM_SAY, "text": "x"}}]}
        with open(path, "w", encoding="utf-8") as fh:
            json.dump(sendonly, fh, ensure_ascii=False)
        data2 = load_scenario(path)
        plans2, _ = expand_scenario(data2)
        assert [pl["kind"] for pl in plans2] == ["send"], plans2
        # 4) required 形状非法 / 未知顶层键仍 fail-closed
        for bad in (dict(base, required="make_index"), dict(base, required=[1]),
                    dict(base, bogus=1)):
            with open(path, "w", encoding="utf-8") as fh:
                json.dump(bad, fh, ensure_ascii=False)
            try:
                load_scenario(path)
            except ScenarioError:
                pass
            else:
                raise AssertionError("load_scenario accepted bad scenario: %r" % sorted(bad))
        return "required/text-params 4 项"
    finally:
        try:
            os.unlink(path)
        except OSError:
            pass


def _write_tmp_rs(text):
    """selftest 用：写一个临时 .rs 文件（假 messages.rs / attackwire.rs），返回路径。"""
    import tempfile
    fd, path = tempfile.mkstemp(suffix=".rs")
    os.close(fd)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(text)
    return path


def _selftest_scenario_validation():
    """`--check-scenario` 的语义牙：R1/R2/R3/R4，外加「纯观察场景不误伤」。

    卡面要求的三条坏样例：① 删 send 步 ② wait_for 写成表外的 4095 ③ 引用未声明变量；
    另加 ④ wait_for 排在 send 之前、⑤ 全 sleep 场景，以及 login_only/dice_chain 仍绿的断言。
    """
    messages = load_message_constants()
    base = {
        "name": "selftest_validation",
        "required": ["target_x", "target_y", "hit_dir"],
        "steps": [
            {"send": {"ident": 3014, "recog": "target_x",
                      "param": "target_y", "tag": "hit_dir"}},
            {"wait_for": 14},
        ],
    }
    ok_vars = {"target_x": 30, "target_y": 32, "hit_dir": 4}

    def clone(obj):
        return json.loads(json.dumps(obj))

    def expect_red(mut, needle):
        d = clone(base)
        mut(d)
        try:
            plans, _ = expand_scenario(d, dict(ok_vars))
        except ScenarioError as ex:
            msg = str(ex)
        else:
            try:
                check_scenario(d, plans, None, messages)
            except ScenarioError as ex:
                msg = str(ex)
            else:
                raise AssertionError("bad scenario accepted (tooth broken): %r" % (needle,))
        assert needle in msg, (needle, msg)
        return msg

    # 好样例（与 combat_hit 同形）必须绿
    check_scenario(clone(base), expand_scenario(clone(base), dict(ok_vars))[0],
                   ok_vars, messages)

    # ① 删掉 send 步 ⇒ wait_for 之前没有发帧步骤（R2 点名）
    expect_red(lambda d: d.__setitem__("steps", [s for s in d["steps"] if "send" not in s]),
               "no send step precedes")
    # ② wait_for 写成协议表里不存在的 ident ⇒ R3 点名 4095
    expect_red(lambda d: d["steps"][1].__setitem__("wait_for", 4095),
               "ident 4095 is not a CM_*/SM_* constant")
    # ③ 引用未声明变量（拼错）⇒ 加载期就红
    expect_red(lambda d: d["steps"][0]["send"].__setitem__("recog", "no_such_var"),
               "no_such_var")
    # ③b 只在 --var 里出现、场景从未声明 ⇒ R4 仍然红（--var 是覆值通道，不是声明通道）
    expect_red(lambda d: d.__setitem__("required", ["target_y", "hit_dir"]),
               "undeclared variable 'target_x'")
    # ④ wait_for 排在 send 之前 ⇒ R2
    expect_red(lambda d: d.__setitem__("steps", [d["steps"][1], d["steps"][0]]),
               "no send step precedes")
    # ②b send.ident 也必须在协议表里（R3 的 send 分支；M2：单独禁用该分支必须红）
    expect_red(lambda d: d["steps"][0]["send"].__setitem__("ident", 4095),
               "send.ident 4095 is not a CM_*/SM_* constant")
    # ⑤ 全是 sleep ⇒ R1（既不发帧也不观察）
    expect_red(lambda d: d.__setitem__("steps", [{"sleep": 1}, {"sleep": 2}]),
               "has no send step")

    # M4：常量表先剥 `//` 注释再匹配 —— 注释掉的常量不得进表（否则 wait_for 假绿）
    import tempfile
    fake_rs = _write_tmp_rs("// removed: pub const SM_FAKE: u16 = 4095;\n"
                            "pub const SM_REAL: u16 = 50;   // 行尾注释内的 pub const SM_X: u16 = 51;\n"
                            "pub const CM_OTHER: u16 = 3014;\n")
    try:
        tbl = load_message_constants(fake_rs)
        assert 4095 not in tbl["SM"] and 4095 not in tbl["CM"], tbl   # 整行注释
        assert 51 not in tbl["SM"], tbl                                # 行尾注释
        assert tbl["SM"] == {50: "SM_REAL"} and tbl["CM"] == {3014: "CM_OTHER"}, tbl
    finally:
        os.unlink(fake_rs)

    # R5：placement 与 params.self_pos 必须自洽（牙：placement.y 改 1 ⇒ 红）
    placed = {
        "name": "selftest_placement",
        "params": {"self_pos": 15204608},                 # MakeLong(X=256, Y=232)
        "placement": {"map": "4", "x": 256, "y": 232},
        "steps": [{"send": {"ident": 3010, "recog": "self_pos"}}, {"wait_for": 10}],
    }
    check_scenario(clone(placed), expand_scenario(clone(placed))[0], None, messages)
    bad_pl = clone(placed)
    bad_pl["placement"]["y"] = 233
    try:
        check_scenario(bad_pl, expand_scenario(clone(bad_pl))[0], None, messages)
    except ScenarioError as ex:
        msg = str(ex)
        assert "disagrees with self_pos" in msg and "x=256 y=232" in msg, msg
    else:
        raise AssertionError("placement/self_pos mismatch accepted (R5 tooth broken)")
    # --var 覆成不一致的值也必须红（生效参数口径）
    try:
        check_scenario(clone(placed), expand_scenario(clone(placed), {"self_pos": 15204609})[0],
                       {"self_pos": 15204609}, messages)
    except ScenarioError as ex:
        assert "disagrees with self_pos" in str(ex), str(ex)
    else:
        raise AssertionError("--var self_pos mismatch accepted (R5 tooth broken)")

    # M1：self_x / self_y 也必须与 placement 自洽
    placed_xy = {
        "name": "selftest_placement_xy",
        "params": {"self_x": 256, "self_y": 232},
        "placement": {"map": "4", "x": 256, "y": 232},
        "steps": [{"send": {"ident": 3011, "recog": "self_x", "param": "self_y"}},
                  {"wait_for": 11}],
    }
    check_scenario(clone(placed_xy), expand_scenario(clone(placed_xy))[0], None, messages)
    for var, bad in (("self_x", 257), ("self_y", 233)):
        d = clone(placed_xy)
        d["params"][var] = bad
        try:
            check_scenario(d, expand_scenario(clone(d))[0], None, messages)
        except ScenarioError as ex:
            assert ("disagrees with %s" % var) in str(ex), str(ex)
        else:
            raise AssertionError("%s mismatch accepted (R5 tooth broken)" % var)

    # M2：派生变量写成**间接 params 名**（链式）不得假红；链到错值仍要红
    indirect = {
        "name": "selftest_indirect",
        "params": {"self_pos": "start_pos", "start_pos": 15204608},
        "placement": {"map": "4", "x": 256, "y": 232},
        "steps": [{"send": {"ident": 3010, "recog": "self_pos"}}, {"wait_for": 10}],
    }
    check_scenario(clone(indirect), expand_scenario(clone(indirect))[0], None, messages)
    indirect_bad = clone(indirect)
    indirect_bad["params"]["start_pos"] = 15204609
    try:
        check_scenario(indirect_bad, expand_scenario(clone(indirect_bad))[0], None, messages)
    except ScenarioError as ex:
        assert "disagrees with self_pos" in str(ex), str(ex)
    else:
        raise AssertionError("indirect self_pos mismatch accepted")

    # R6（WARN 级）：攻击族 tag=(byte)Direction 越界 ⇒ stderr 警告，**不判死**。
    # 机理：oracle 先按**状态化** Dir 取目标（PlayObject.Attack.cs:26 / lib.rs:8836-8848），
    # nDir 之后才写入 ⇒ CM_TURN(tag=0)+CM_HIT(tag=99) 的第一击照样命中，不能 rc=2。
    import contextlib
    import io as _io
    hit_idents, dirs = load_direction_spec(messages=messages)
    assert len(hit_idents) == 9 and dirs == frozenset(range(8)), (hit_idents, dirs)
    assert 3014 in hit_idents, hit_idents          # CM_HIT

    def stderr_of(data):
        """展开 + 校验，返回 stderr 文本（不抛 = 通过；抛则冒泡）。"""
        plans, _ = expand_scenario(clone(data))
        buf = _io.StringIO()
        with contextlib.redirect_stderr(buf):
            check_scenario(clone(data), plans, None, messages)
        return buf.getvalue()

    turn_hit = {
        "name": "selftest_turn_hit",
        "steps": [{"send": {"ident": 3010, "recog": 1, "param": 1, "tag": 0}},
                  {"send": {"ident": 3014, "recog": 1, "param": 1, "tag": 99}},
                  {"wait_for": 31}],
    }
    warn = stderr_of(turn_hit)       # 不得抛：oracle 第一击仍命中
    assert "WARN" in warn and "tag 99" in warn, warn
    assert "不影响本次命中" in warn and "状态 Dir" in warn, warn
    assert "never hits" not in warn and "only time out" not in warn, warn
    ok_dir = clone(turn_hit)
    ok_dir["steps"][1]["send"]["tag"] = 7
    assert stderr_of(ok_dir) == "", "合法 Direction (tag=7) 不该有警告"
    # 非攻击族不受 R6 约束（不误伤）：CM_SAY 的 tag 没有方向语义
    say_scen = {"name": "selftest_say", "steps": [{"send": {"ident": 3030, "tag": 99}},
                                                  {"wait_for": 100}]}
    assert stderr_of(say_scen) == "", "非攻击族 ident 不该有方向警告"

    # M3：R6 取表 fail-closed 的四种 drift 都要在 selftest 里被钉住
    _NL = "\n"
    dir_consts = "".join("const DIR_X%d: u8 = %d;\n" % (i, i) for i in range(8))
    good_aw = ("pub const HIT_IDENTS: [u16; 1] = [\n    messages::CM_HIT,\n];\n" + dir_consts)
    drift_cases = [
        ("missing-file", None, "cannot read attack-family table"),
        ("no-hit-idents", "pub const NOPE: u16 = 1;\n" + dir_consts,
         "cannot find `pub const HIT_IDENTS"),
        ("unknown-name", "pub const HIT_IDENTS: [u16; 1] = [messages::CM_NOPE];\n" + dir_consts,
         "references CM_NOPE"),
        ("short-direction-set",
         "pub const HIT_IDENTS: [u16; 1] = [messages::CM_HIT];\n"
         + "".join("const DIR_X%d: u8 = %d;\n" % (i, i) for i in range(7)),
         "expected 0..7"),
        # 多出一个越界方向（0..7+99）：任何「放宽成包含」的变异都必须被这条钉住
        ("extra-direction",
         "pub const HIT_IDENTS: [u16; 1] = [messages::CM_HIT];" + _NL + dir_consts
         + "const DIR_XBOGUS: u8 = 99;" + _NL,
         "expected 0..7"),
    ]
    assert load_direction_spec(_write_tmp_rs(good_aw), messages)[0] == {3014}, "good fake"
    for label, text, needle in drift_cases:
        path = os.path.join(tempfile.mkdtemp(prefix="pd-dir-"), "no-such.rs") if text is None \
            else _write_tmp_rs(text)
        try:
            load_direction_spec(path, messages)
        except ScenarioError as ex:
            assert needle in str(ex), (label, needle, str(ex))
        else:
            raise AssertionError("direction-table drift %r accepted (fail-closed broken)" % label)

    # 防误伤：login_only（纯累计观察）与 dice_chain（params 默认值）必须绿
    for name in ("login_only.json", "dice_chain.json"):
        d = load_scenario(os.path.join(SCENARIO_DIR, name))
        check_scenario(d, expand_scenario(d)[0], None, messages)
    return "R1-R5 硬牙 + R6 方向 WARN + 取表 drift 四例 + 不误伤"


class _TimedSock:
    """selftest 用：按墙钟延迟吐出预录字节（不占端口、不连网）。

    chunks = [(delay_s, bytes)]，delay_s 相对本 sock 创建时刻；未到期抛 socket.timeout，
    让 Conn.pump 的超时轮询继续（与真 socket 的 50ms 轮询同形状）。
    """

    def __init__(self, chunks):
        self.t0 = time.monotonic()
        self.chunks = [(float(d), bytes(b)) for d, b in chunks]
        self.sent = []

    def recv(self, n):
        now = time.monotonic()
        if not self.chunks:
            time.sleep(0.001)
            raise socket.timeout()
        due, data = self.chunks[0]
        if now < self.t0 + due:
            time.sleep(min(self.t0 + due - now, 0.05))
            raise socket.timeout()
        self.chunks.pop(0)
        if len(data) > n:
            self.chunks.insert(0, (0.0, data[n:]))
            data = data[:n]
        return data

    def sendall(self, data):
        self.sent.append(bytes(data))

    def close(self):
        pass


def _delayed_frames():
    """进世界握手段（early=SM_LOGON）与延迟段（late=10/654/657/100）。"""
    early = build_s2c_frame(6930, SM_LOGON, 5, 9, 0, bytes(16))
    late = b"".join(build_s2c_frame(*f) for f in (
        (119, 10, 8, 10, 0, b"2\x00\x0c\x00\x00\x00\x00\x00"),
        (0, SM_CHANGELIGHT, 0, 0, 0, b""),
        (0, SM_CHARSTATUSCHANGED, 1, 0, 0, b""),
        (6930, SM_SYSMESSAGE, 0, 0, 0, encode_text("welcome")),
    ))
    return early, late


def _selftest_dwell():
    """dwell 默认值 + 正例 + 牙：dwell>0 收齐延迟段帧，dwell=0（旧行为）丢帧。"""
    import shutil
    import tempfile
    tmp = tempfile.mkdtemp(prefix="mir2probe-dwell-")
    try:
        early, late = _delayed_frames()
        base = ["--login-gate", "h:1", "--account", "a", "--password", "p",
                "--server-name", "s", "--scenario", "x", "--out", tmp]
        parsed = build_argparser().parse_args(base)
        assert parsed.dwell_ms == DEFAULT_DWELL_MS == 1200, parsed.dwell_ms
        parsed0 = build_argparser().parse_args(base + ["--dwell-ms", "0"])
        assert parsed0.dwell_ms == 0, parsed0.dwell_ms
        neg = build_argparser().parse_args(base + ["--dwell-ms", "-1"])
        try:
            _check_required(neg)
        except ProbeError:
            pass
        else:
            raise AssertionError("negative --dwell-ms accepted (must fail-closed, rc=2)")

        # 正例：dwell=300ms => 150ms 后才到的延迟段帧（654/657/100）收齐
        args = argparse.Namespace(out=tmp, no_auto_handshake=False, timeout_ms=8000)
        run = Run(args)
        conn = Conn("gamegate", run.recorder, on_frame=run.on_frame)
        conn.phase = "gamegate"
        conn.sock = _TimedSock([(0.0, early), (0.15, late)])
        conn.pump(20)  # 进世界握手：只到 SM_LOGON
        before = {f.ident for _, f in conn.frames}
        assert SM_LOGON in before and SM_CHANGELIGHT not in before, before
        rec = dwell_after_enter(conn, 300, 200)
        assert rec["requested_ms"] == 300 and rec["legacy_pump_ms"] == 0, rec
        assert rec["actual_ms"] >= 300, rec
        want = {SM_CHANGELIGHT, SM_CHARSTATUSCHANGED, SM_SYSMESSAGE}
        assert want <= set(rec["idents_after"]), rec
        assert want <= set(rec["new_idents"]), rec
        run.dwell = rec
        # 执行路径：到齐 => 步骤记录 matched；缺 ident => StepFailure（退出码 1 的路径）
        plan = plan_step({"wait_idents": {"idents": [SM_CHANGELIGHT, SM_SYSMESSAGE],
                                          "timeout_ms": 100}}, 0, {})
        run.execute_step(conn, plan)
        step = run.steps[-1]
        assert step["kind"] == "wait_idents" and step["matched"], step
        assert step["missing_idents"] == [], step
        assert {SM_CHANGELIGHT, SM_SYSMESSAGE} <= set(step["seen_idents"]), step
        plan_bad = plan_step({"wait_idents": {"idents": [SM_STARTPLAY],
                                              "timeout_ms": 10}}, 1, {})
        try:
            run.execute_step(conn, plan_bad)
        except StepFailure as ex:
            assert ("missing [%d]" % SM_STARTPLAY) in str(ex), str(ex)
        else:
            raise AssertionError("wait_idents accepted a missing ident")
        assert run.steps[-1]["kind"] == "wait_idents", run.steps[-1]
        assert run.steps[-1]["missing_idents"] == [SM_STARTPLAY], run.steps[-1]
        payload = build_run_json(run, args, None, {}, None)
        assert payload["dwell"]["requested_ms"] == 300, payload.get("dwell")
        assert SM_CHANGELIGHT in payload["dwell"]["idents_after"], payload.get("dwell")

        # 牙（反例）：dwell=0 且无 legacy pump => 同一串延迟段帧收不到。
        # 与上面的默认值断言合起来钉住「dwell 必须 > 0」：把 --dwell-ms 默认改回 0、
        # 或让 dwell_after_enter 不再 pump，默认值断言/正例即红（已实测 tooth1 红）。
        tmp0 = os.path.join(tmp, "dwell0")
        os.makedirs(tmp0, exist_ok=True)
        args0 = argparse.Namespace(out=tmp0, no_auto_handshake=False, timeout_ms=8000)
        run0 = Run(args0)
        conn0 = Conn("gamegate", run0.recorder, on_frame=run0.on_frame)
        conn0.phase = "gamegate"
        conn0.sock = _TimedSock([(0.0, early), (0.15, late)])
        conn0.pump(20)
        rec0 = dwell_after_enter(conn0, 0, 0)
        seen0 = set(rec0["idents_after"])
        assert rec0["requested_ms"] == 0, rec0
        assert SM_CHANGELIGHT not in seen0 and SM_SYSMESSAGE not in seen0, seen0
        assert rec0["actual_ms"] < 300, rec0
        return "dwell 300ms 收齐 654/657/100；dwell=0 丢帧（牙），默认 %dms" % DEFAULT_DWELL_MS
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


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
    check("scenario-required", _selftest_scenario_required)
    check("scenario-validation", _selftest_scenario_validation)
    check("helpers", _selftest_helpers)
    check("conn-semantics", _selftest_conn_semantics)
    check("dwell-after-enter", _selftest_dwell)
    if fails:
        print("mir2_probe selftest: FAIL")
        for f in fails:
            print("  -", f)
        return 2
    print("mir2_probe selftest: OK (9 checks; fixtures replayed at 1/2/3/5/7/11/13/17/64-byte chunks)")
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
        # wait_idents 累计口径且要求全到：已收的立即满足，缺的按 timeout 返回差集
        missing, _ = conn.wait_idents([SM_SENDNOTICE, SM_SYSMESSAGE], 50)
        assert missing == [], missing
        missing, waited = conn.wait_idents([SM_STARTPLAY], 30)
        assert missing == [SM_STARTPLAY] and waited >= 30, (missing, waited)
        # 部分命中：不能把「任一命中」当成功 —— missing 必须保留差集，且必须等满 timeout
        # （把 wait_idents 退化成 any-match 则这里必红：missing 变 [] 或 waited 变 ~0）
        missing, waited = conn.wait_idents([SM_SENDNOTICE, SM_STARTPLAY], 30)
        assert missing == [SM_STARTPLAY], missing
        assert waited >= 30, waited
        run.recorder.close()
        rows = [json.loads(line) for line in open(run.recorder.path, encoding="utf-8")]
        assert len(rows) == 4 and rows[0]["frame_class"] == "protocol"
        return "%d frames, 2 auto replies" % len(rows)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
