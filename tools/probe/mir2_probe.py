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
                       build_s2c_frame, decode_bytes, decode_text, encode_bytes, encode_text,
                       pack_header)

TOOL = "tools/probe/mir2_probe.py"
SCHEMA_VERSION = 1

# ---- 消息号（src/OpenMir2/Messages.cs；只抄本驱动器用到的） ----
CM_QUERYBAGITEMS = 81
CM_NEWCHR = 101               # src/OpenMir2/Messages.cs:30（SelGate + DB UserService.NewChr）
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

SM_LOGON = 50                 # PlayObject.cs:1131 MakeMessage(SM_LOGON, ActorId, CurrX, CurrY, MakeWord(Dir,Light))
SM_NEWMAP = 51                # PlayObject.Message.cs:1680-1684（RM_LOGON 支）：body = Envir.MapFileName
SM_MAPDESCRIPTION = 54        # PlayObject.Base.cs:894（Envir.MapDesc）
SM_CHANGEMAP = 634            # 只在 **换图**（RM_CHANGEMAP）发：BaseObject.cs:1134 / PlayObject.cs:3778；
                              # 首登不换图 ⇒ 首包地图帧是 SM_NEWMAP(51)，不是 634
SM_TURN = 10
SM_ABILITY = 52
SM_SYSMESSAGE = 100
SM_QUERYCHR = 520
SM_QUERYCHR_FAIL = 527
SM_NEWCHR_SUCCESS = 521       # DB UserService.cs:652（MakeMessage(SM_NEWCHR_SUCCESS,0,…)）
SM_NEWCHR_FAIL = 522          # DB UserService.cs:656（MakeMessage(SM_NEWCHR_FAIL, nCode, …) ⇒ Recog=nCode）
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
SM_HIT = 14                   # SM_HIT：攻击者动作广播（recog=攻击者 ActorId）
SM_STRUCK = 31                # SM_STRUCK：受击广播（recog=被打对象的 ActorId；param=HP、tag=MaxHP、series=伤害）
CM_TURN = 3010                # src/OpenMir2/Messages.cs:120 → PlayObject.Message.cs:1218-1220 → ClientChangeDir
CM_HIT = 3014                 # Messages.cs:136 → PlayObject.Message.cs:1221-1230 → ClientHitXY（recog=**自身**坐标、tag=朝向）
SM_DEATH = 32                 # 可见性刷新时的死亡帧（PlayObject.Message.cs:1642-1646；来自 PlayObject.cs:3235，nParam3=0）
SM_NOWDEATH = 34              # **真死亡广播**（BaseObject.Base.cs:351 RM_DEATH nParam3=1 → PlayObject.Message.cs:1632-1634）
#: 八向朝向：`(dx, dy) -> Dir`（Direction.cs：Up=0/UpRight=1/Right=2/DownRight=3/Down=4/DownLeft=5/Left=6/UpLeft=7）。
DIRECTION_BY_DELTA = {(0, -1): 0, (1, -1): 1, (1, 0): 2, (1, 1): 3,
                      (0, 1): 4, (-1, 1): 5, (-1, 0): 6, (-1, -1): 7}

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

    def wait_for(self, idents, timeout_ms, after_seq, recog_filter=None):
        """等 seq > after_seq 的第一条 ident ∈ idents 且 recog_filter 通过的帧；超时返回 None。

        ``recog_filter``（可选）是 ``frame -> bool``：只看**来源**（帧头 ``recog``）匹配的帧。
        ``None`` = 旧语义（只按 ident 匹配，不筛来源）。场景 DSL 的 ``recog`` / ``recog_is``
        由 ``Run._recog_filter`` 编译成这个谓词（见 README「等待来源过滤」）。
        """
        wanted = set(idents)
        end = time.time() + timeout_ms / 1000.0
        while True:
            for seq, fr in self.frames:
                if (seq > after_seq and fr.ident in wanted
                        and (recog_filter is None or recog_filter(fr))):
                    return seq, fr
            remain = end - time.time()
            if remain <= 0:
                return None
            self.pump(min(100, int(remain * 1000)))

    def wait_frames(self, count, idents, timeout_ms, recog_filter=None):
        """等累计帧数 >= count 且 idents 全部出现过（含等待前已收到的帧）。

        ``recog_filter`` 只作用于「idents 是否都出现过」；``count`` 仍是连接累计帧数
        （不过滤），保持旧口径。
        """
        required = set(idents or [])
        end = time.time() + timeout_ms / 1000.0
        while True:
            seen = {fr.ident for _, fr in self.frames
                    if recog_filter is None or recog_filter(fr)}
            if len(self.frames) >= count and required <= seen:
                return True
            remain = end - time.time()
            if remain <= 0:
                return False
            self.pump(min(100, int(remain * 1000)))

    def wait_idents(self, idents, timeout_ms, recog_filter=None):
        """等 idents **全部**出现过（累计口径，含等待前已收到的帧）；超时返回差集。

        返回 (missing, waited_ms)：missing 为空 = 全部到齐。与 wait_for 的区别：
        wait_for 只认本步开始之后的第一条（任一命中），本方法不设 seq 下界、要求全到。
        """
        required = set(idents)
        t0 = time.time()
        deadline = t0 + timeout_ms / 1000.0
        while True:
            missing = sorted(required - {fr.ident for _, fr in self.frames
                                         if recog_filter is None or recog_filter(fr)})
            now = time.time()
            if not missing or now >= deadline:
                return missing, int((now - t0) * 1000)
            self.pump(min(100, max(1, int((deadline - now) * 1000))))


# ==========================================================================
# 场景 DSL
# ==========================================================================
STEP_META_KEYS = {"timeout_ms", "note", "delay_ms"}
ACTION_KEYS = {"send", "wait_for", "wait_frames", "wait_idents", "sleep",
               "say", "click_npc", "dlg_select", "dlg_select_any", "repeat", "newchr",
               "kill"}
# 场景顶层 `requires` 只认这些夹具值（卡 `openmir2-scenario-fixture-precondition`）：
# 目前只有 `magic`（characters_magic 预置）。未知值 fail-closed，避免「加了值却没生效」假绿。
REQUIRES_KNOWN = ("magic",)


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


def resolve_str_param(value, params, where):
    """把场景参数解析成**字符串**：字面量原样；是 params 名则取 params 里的字符串值。"""
    if not isinstance(value, str) or not value:
        raise ScenarioError("%s must be a non-empty string or params name, got %r"
                            % (where, value))
    if value in params:
        v = params[value]
        # `--var k=123` 会被 parse_vars 转成 int（数字串）——名字允许数字，转回字符串即可
        # （长度/字符门仍在调用方校验）。
        if isinstance(v, int) and not isinstance(v, bool):
            return str(v)
        if not isinstance(v, str) or not v:
            raise ScenarioError("%s: params[%r] must resolve to a non-empty string, got %r"
                                % (where, value, v))
        return v
    return value


# ---- NPC 按名/按坐标寻址（卡 openmir2-probe-npc-discovery）------------------------------
#
# `click_npc` 除 actor id / `auto:last_turn` 外新增两种**按可见对象**寻址：
#   * `name:<NPC 名>` —— 匹配 `SM_TURN(10)` 出现帧的名尾 `ShowName`；
#   * `pos:<x>,<y>`   —— 匹配该格的可见对象（`SM_TURN` 的 `Param=x` / `Tag=y`）。
# 两者都必须**唯一命中**：0 个或多于 1 个一律 StepFailure 并列出候选（fail-closed，
# 绝不「挑第一个」，也不回落到 `auto:last_turn`）。
#
# 名尾口径（oracle 同源）：`SM_TURN` 体 = `Encode(8B CharDesc)` [ + `Encode("ShowName/Color")` ]，
# 两段**各自** EDCode（`src/M2Server/Player/PlayObject.Message.cs:1549-1555`；
# Rust 侧 `mir2-rs/bins/game-svr/src/downlink.rs` 的 `turn_pair`）。`CM_TURN` 下行路径无名尾
# （`turn_pair_nameless`）⇒ 该对象 `name=None`，只能按 pos/actor 寻址。
SM_TURN_DESC_CHARS = 11   # Encode(8B CharDesc) = 11 字符（mir2frame.encode_bytes 的可逆长度）

# 移动帧族（服务端广播某对象移动）：对象**动过**的证据。值取自 `crates/protocol/src/messages.rs`。
SM_RUSH = 6
SM_RUSHKUNG = 7
SM_BACKSTEP = 9
SM_WALK = 11
SM_RUN = 13
SM_DISAPPEAR = 30
SM_HORSERUN = 5010
MOVE_IDENTS = frozenset({SM_RUSH, SM_RUSHKUNG, SM_BACKSTEP, SM_WALK, SM_RUN, SM_HORSERUN})

# 名尾 = `ShowName/Color`；颜色段必须是十进制（`$X/255`）。**最后一段不是颜色 ⇒ 整个名尾不可信**
# （宁可 `<无名>` 也不截断成前缀名；卡 `openmir2-probe-npc-addressing-followups` 的 N3）。
NAME_TAIL_RE = re.compile(r"^(?P<name>.*)/(?P<color>\d{1,3})$")


def actor_name_tail(frame):
    """`SM_TURN(10)` 体里的原始名尾文本（诊断用；无名尾 ⇒ None）。"""
    body = frame.body or b""
    if len(body) <= SM_TURN_DESC_CHARS:
        return None
    try:
        raw = decode_bytes(body[SM_TURN_DESC_CHARS:], strict=False)
    except FrameError:
        return None
    text = decode_text(raw, "gbk", "replace")
    return text or None


def actor_show_name(frame):
    """从 `SM_TURN(10)` 的体里取 `ShowName`；无名尾或名尾无颜色段 ⇒ None（不截断）。"""
    text = actor_name_tail(frame)
    if text is None:
        return None
    m = NAME_TAIL_RE.match(text)
    if m is None:
        return None
    name = m.group("name").strip()
    return name or None


def scan_actors(frames, self_actor):
    """扫一遍帧流，返回 `(rows, order)`：**当前可见**对象 + 每对象的动态证据。

    `frames` 是 `Conn.frames`（`[(seq, Frame), ...]`）。口径（**启发式，不是协议级类型判定**）：
      * `SM_TURN(10)` = 出现/刷新帧（`Recog=actor id`、`Param=x`、`Tag=y`、可选名尾）；
      * `MOVE_IDENTS` 任一 = 该对象**动过**（`moves += 1`，坐标取该帧 `Param/Tag`）；
      * `SM_DISAPPEAR(30)` = 该对象从**当前**可见集移除（再出现则重新加入）；
      * 自身（`self_actor`）永不入集。
    """
    rows = {}
    order = []
    for seq, fr in frames:
        if fr.recog == self_actor:
            continue
        if fr.ident == SM_TURN:
            cur = rows.get(fr.recog)
            if cur is None:
                rows[fr.recog] = {
                    "actor": fr.recog, "name": actor_show_name(fr), "tail": actor_name_tail(fr),
                    "x": fr.param, "y": fr.tag, "first_seq": seq, "last_seq": seq,
                    "moves": 0, "coords": {(fr.param, fr.tag)}, "disappeared": False,
                }
                order.append(fr.recog)
            else:
                cur["name"] = actor_show_name(fr) or cur["name"]
                cur["tail"] = actor_name_tail(fr) or cur["tail"]
                cur["x"], cur["y"], cur["last_seq"] = fr.param, fr.tag, seq
                cur["coords"].add((fr.param, fr.tag))
                cur["disappeared"] = False
        elif fr.ident in MOVE_IDENTS:
            cur = rows.get(fr.recog)
            if cur is not None:
                cur["moves"] += 1
                cur["coords"].add((fr.param, fr.tag))
                cur["x"], cur["y"], cur["last_seq"] = fr.param, fr.tag, seq
        elif fr.ident == SM_DISAPPEAR:
            cur = rows.get(fr.recog)
            if cur is not None:
                cur["disappeared"] = True
                cur["last_seq"] = seq
    return rows, order


def visible_actors(frames, self_actor):
    """**当前可见**对象清单（剔除已 `SM_DISAPPEAR` 的；按 actor id 去重）。"""
    rows, order = scan_actors(frames, self_actor)
    return [rows[a] for a in order if not rows[a]["disappeared"]]


def actor_static(row, snapshot_end_seq):
    """启发式「固定/静态对象」判定 ⇒ `(ok, why)`。

    **三条同时满足**（卡 `openmir2-probe-npc-addressing-followups` 的 N2；协议里没有对象类别字段，
    依据是 `SM_TURN` 体只有 `CharDesc + 名尾`）：
      1. 没有任何移动帧（`moves == 0`）；
      2. 坐标从未变过（出现帧只给过一个格子）；
      3. **首见于玩家登录后的第一份可见快照内**（`first_seq < snapshot_end_seq`，固定 NPC 随地图加载下发，
         怪是后来刷的）。
    """
    if row["moves"]:
        return False, "有移动帧 %d 条" % row["moves"]
    if len(row["coords"]) > 1:
        return False, "坐标变过 %d 处" % len(row["coords"])
    if snapshot_end_seq is None:
        return False, "初始快照界未知（snapshot_end_seq=None）"
    if row["first_seq"] > snapshot_end_seq:
        return False, "首见于 seq=%d，晚于初始快照界 seq=%d" % (row["first_seq"], snapshot_end_seq)
    return True, "静态（无移动帧、坐标不变、首见 seq=%d ≤ 快照界 seq=%d）" % (
        row["first_seq"], snapshot_end_seq)


def format_actors(rows, snapshot_end_seq=None):
    """候选清单（错误信息用）：name/(x,y)/actor + 首见 seq + 移动帧数 (+ 类别/原始名尾)。

    无名对象写 `<无名>` 并附**原始名尾**（N3：名尾没有颜色段时一眼可见，而不是被截断成前缀名）。
    """
    parts = []
    for r in rows:
        seg = "name=%s (%d,%d) actor=%d 首见=seq%d 移动帧=%d" % (
            r.get("name") or "<无名>", r["x"], r["y"], r["actor"],
            r.get("first_seq", -1), r.get("moves", 0))
        if not r.get("name") and r.get("tail"):
            seg += " 原始名尾=%r" % r["tail"][:40]
        if snapshot_end_seq is not None:
            ok, why = actor_static(r, snapshot_end_seq)
            seg += " 类别=%s（%s）" % ("静态" if ok else "活动", why)
        parts.append(seg)
    return "; ".join(parts) or "<无可见对象>"


# ---- 多会话：会话标注与收件人集合断言（卡 openmir2-multi-session-probe）-----------------
#
# 口径（卡上设计 comment bec70baf）：
# - 帧行 `conn` 列沿用旧值；多会话时标签为 `<session>.<hop>`（`s1.gamegate`/`s2.selgate`…），
#   单会话旧标签（`gamegate`/`selgate`/`login`）**不含 `.`** ⇒ `session_of` 归 `"main"`。
#   因此 `frames.ndjson` 旧字段逐字节不变，`compare_runs.py` 口径零改动。
# - 断言只吃「帧行 + 谓词」：不推算坐标、不假设 AOI；±N 边界由用例把期望集合直接给出。

RADIUS_CELLS = 12  # `SendRefMsgRange` 的 ±12（`SendRefMsg` 广播面；用例把期望集合喂进来）


def session_of(conn_label):
    """`<session>.<hop>` ⇒ 会话名；无 `.` 的旧标签 ⇒ `"main"`（单会话）。"""
    head, sep, _tail = (conn_label or "").partition(".")
    return head if sep else "main"


def session_label(session, hop):
    """多会话标签：`<session>.<hop>`（`hop` ∈ login/selgate/gamegate）。"""
    return "%s.%s" % (session, hop)


def within_radius(distance, radius=RADIUS_CELLS):
    """一维 `±N` 判定（含边界）。

    **注意（复审 M-1）**：oracle 的可见范围是**正方形**（`|dx|<=N && |dy|<=N`，
    `BaseObject.Message.cs:399-405`），不是欧氏半径；判「某格是否在视野内」请用
    [`within_square`]，不要把欧氏距离喂给本函数（(12,12) 的欧氏距离 16.97 会被误判为外）。
    """
    return abs(int(distance)) <= int(radius)


def within_square(dx, dy, radius=RADIUS_CELLS):
    """oracle 视野判定：`|dx| <= N && |dy| <= N`（端点在内；正方形，不是欧氏圆）。"""
    return abs(int(dx)) <= int(radius) and abs(int(dy)) <= int(radius)


def _row_matches(row, pred):
    for key in ("ident", "param", "tag", "series"):
        if key in pred and row.get(key) != pred[key]:
            return False
    if "body_text_contains" in pred:
        if pred["body_text_contains"] not in (row.get("body_text") or ""):
            return False
    t_from = pred.get("t_from")
    if t_from is not None and row.get("t_ms", 0) < t_from:
        return False
    t_to = pred.get("t_to")
    if t_to is not None and row.get("t_ms", 0) > t_to:
        return False
    return True


def recipients(rows, pred, sessions=None, statuses=None):
    """命中谓词的帧**分别由哪些会话收到**（会话名有序集合）。

    `sessions` 给了就只计这些会话；`statuses`（`{name: online|offline|closed}`）给了就
    只计 `online`（复审 I-1：连接状态必须能进判定，不能只靠帧是否存在）。
    """
    if sessions is not None and statuses is not None:
        raise ValueError(
            "recipients: sessions 与 statuses 只能给一个（同时给会让 statuses 被静默忽略）")
    if sessions is None and statuses is not None:
        sessions = online_sessions(statuses)
    allowed = set(sessions) if sessions is not None else None
    got = set()
    for row in rows:
        if not _row_matches(row, pred):
            continue
        name = session_of(row.get("conn", ""))
        # 复审 `bc2a5f2b` I-1：给了 statuses 就必须覆盖**所有**出现过的会话；否则未登记
        # 会话的帧会被静默过滤掉，「非 online 会话收到帧」永远观测不到。
        if statuses is not None and name not in statuses:
            raise ValueError(
                "recipients: 帧里的会话 %r 不在 statuses 里（%r）——无法判定 online 与否，"
                "拒绝静默剔除" % (name, sorted(statuses)))
        if allowed is not None and name not in allowed:
            continue
        got.add(name)
    return sorted(got)


def recipient_first_seqs(rows, pred, sessions=None):
    """每个会话**首帧**的 `seq`（用于「三会话都收到且次序一致」类判据）。"""
    allowed = set(sessions) if sessions is not None else None
    out = {}
    for row in rows:
        if not _row_matches(row, pred):
            continue
        name = session_of(row.get("conn", ""))
        if allowed is not None and name not in allowed:
            continue
        seq = row.get("seq")
        if name not in out or (seq is not None and seq < out[name]):
            out[name] = seq
    return out


def assert_recipients(expected, actual):
    """收件人集合断言：`missing`/`extra` 两列都报，绝不「有人收到就算过」。"""
    want, got = set(expected), set(actual)
    missing = sorted(want - got)
    extra = sorted(got - want)
    if missing or extra:
        raise AssertionError(
            "收件人集合不符：missing=%r extra=%r（expected=%r actual=%r）"
            % (missing, extra, sorted(want), sorted(got))
        )


def assert_recipients_with_status(statuses, expected, actual):
    """带连接状态的收件人断言（复审 I-1 的默认口径）。

    1. `expected` 里只保留 `online` 会话（非 online 的不计）；
    2. `actual` 里若出现非 online 会话收到帧 ⇒ 直接报「已断/未连会话仍收帧」（不许静默）。
    """
    unknown_expected = sorted(set(expected) - set(statuses))
    if unknown_expected:
        raise AssertionError(
            "expected 里有未登记连接状态的会话：%r（statuses 必须覆盖所有参与判定的会话，"
            "禁止静默从判据里删掉）" % unknown_expected)
    unknown_actual = sorted(set(actual) - set(statuses))
    if unknown_actual:
        raise AssertionError(
            "actual 里有未登记连接状态的会话：%r（来源不明，拒绝采信）" % unknown_actual)
    live = online_sessions(statuses)
    want = sorted(set(expected) & set(live))
    dead_received = sorted(set(actual) - set(live))
    if dead_received:
        raise AssertionError(
            "非 online 会话收到帧：%r（statuses=%r）" % (dead_received, statuses)
        )
    assert_recipients(want, actual)


def online_sessions(statuses):
    """`{name: "online"|"offline"|"closed"}` ⇒ 参与判定的会话（只有 `online` 计入）。"""
    return sorted(name for name, st in statuses.items() if st == "online")


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
                         "placement", "blocked", "requires", "inventory", "assets"))
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
    _check_blocked(data.get("blocked"))
    _check_requires(data.get("requires"))
    _check_inventory(data.get("inventory"))
    _check_assets(data.get("assets"))
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


def _check_blocked(bl):
    """`scenario.blocked` 的**形状**校验：合法 ⇒ 放行（rc=0），结构非法 ⇒ ScenarioError（rc=2）。

    语义：场景可以声明自己被**封存**——形状合法、能加载、能过 `--check-scenario`，但**不可跑**。
    「为什么跑不了 / 缺什么夹具」必须写在**顶层机器可读字段**里，而不是只写在 `description`
    人话里（窗口批次读不到人话，本日已三次「文字写了但机判不拦」的假绿）。

    schema（顶层 `blocked`）：:

        "blocked": {"reason": "<非空 str>", "missing_fixture": "<str，可选>"}

    * `reason` 必填、非空字符串（缺/空/非字符串 ⇒ rc=2 点名 `scenario.blocked.reason`）；
    * `missing_fixture` 可选，给了就必须是**非空**字符串（空串按「没写」处理会掩盖信息，fail-closed）；
    * 未知键 fail-closed（与 `scenario.placement` 同口径）。

    消费侧：`tools/conformance/card_window.py` 在起栈前见到 `blocked` 一律 rc=2，不建 out、
    不起栈、不写 report —— 从产物层杜绝「跑了一份 BLOCKED 场景」。本驱动器只校验形状，
    `blocked` 场景的 `--check-scenario` 仍是 rc=0（它是形状合法的场景，只是不可跑）。
    """
    if bl is None:
        return
    _need_keys(bl, "scenario.blocked", required=("reason",), optional=("missing_fixture",))
    reason = bl["reason"]
    if not isinstance(reason, str) or not reason.strip():
        raise ScenarioError("scenario.blocked.reason must be a non-empty string, got %r"
                            % (reason,))
    mf = bl.get("missing_fixture")
    if mf is not None and (not isinstance(mf, str) or not mf.strip()):
        raise ScenarioError("scenario.blocked.missing_fixture must be a non-empty string "
                            "when present, got %r" % (mf,))


def _check_requires(rq):
    """`scenario.requires` 的**形状**校验（卡 `openmir2-scenario-fixture-precondition`）。

    语义：场景显式声明「跑我需要的 DB 夹具」（目前只认 `magic`）。**本驱动器不预置夹具、
    也没有 profile 概念**，所以只校验形状；「生效 profile 里有没有对应段」由
    `tools/conformance/card_window.py` 在**加载期**核（缺段 ⇒ rc=2 并点名，dry-run 与真跑同路径）。

    形状（fail-closed，与 `placement`/`blocked` 同口径）：
      * `requires` 必须是非空、无重复的非空字符串数组；
      * 只认 `magic`；未知值点名（拼错/未实现的值不许被静默忽略）。
    """
    if rq is None:
        return
    if (not isinstance(rq, list) or not rq
            or any(not isinstance(x, str) or not x for x in rq)):
        raise ScenarioError("scenario.requires must be a non-empty array of non-empty "
                            "strings, got %r" % (rq,))
    dup = sorted({x for x in rq if rq.count(x) > 1})
    if dup:
        raise ScenarioError("scenario.requires has duplicates: %s" % dup)
    unknown = [x for x in rq if x not in REQUIRES_KNOWN]
    if unknown:
        raise ScenarioError("scenario.requires has unknown fixture(s): %s "
                            "(only %s is implemented; fail-closed)"
                            % (unknown, list(REQUIRES_KNOWN)))


def _check_inventory(inv):
    """`scenario.inventory` 的**形状**校验（卡 `openmir2-inventory-fixture-reachability`）。

    语义：场景**自描述**背包夹具（要哪些物品、放哪个槽），内容全在场景里，不需要 profile 段；
    真正写入由 `tools/conformance/card_window.py` 的 `run_inventory` 在 restore 之后、起栈之前做
    （缺 `db.characters_db`/`char_name`/模板不存在/槽位被占 ⇒ 那边 rc=2 点名）。

    本驱动器只校验形状（与 `placement`/`blocked` 同口径 fail-closed）。
    """
    if inv is None:
        return
    required = ("index", "make_index", "position")
    optional = ("std_mode", "ani_count", "dura", "dura_max", "note")
    if not isinstance(inv, list) or not inv:
        raise ScenarioError("scenario.inventory must be a non-empty array, got %r" % (inv,))
    seen_pos, seen_make = set(), set()
    for i, item in enumerate(inv):
        where = "scenario.inventory[%d]" % i
        if not isinstance(item, dict):
            raise ScenarioError("%s must be an object, got %r" % (where, item))
        bad = set(item) - set(required) - set(optional)
        if bad:
            raise ScenarioError("%s has unknown key(s): %s" % (where, sorted(bad)))
        missing = [k for k in required if k not in item]
        if missing:
            raise ScenarioError("%s missing required key(s): %s" % (where, missing))
        for key in required + optional:
            v = item.get(key)
            if v is None or key == "note":
                continue
            if not isinstance(v, int) or isinstance(v, bool):
                raise ScenarioError("%s.%s must be an int, got %r" % (where, key, v))
            if v < 0 or v > 0xFFFFFFFF:
                raise ScenarioError("%s.%s out of range: %r" % (where, key, v))
        note = item.get("note")
        if note is not None and not isinstance(note, str):
            raise ScenarioError("%s.note must be a string, got %r" % (where, note))
        pos = int(item["position"])
        if pos > 45:
            raise ScenarioError("%s.position=%d out of range (0..45)" % (where, pos))
        if pos in seen_pos:
            raise ScenarioError("%s.position=%d duplicated" % (where, pos))
        seen_pos.add(pos)
        mk = int(item["make_index"])
        if mk <= 0:
            raise ScenarioError("%s.make_index must be > 0 (DBSvr skips 0)" % where)
        if mk in seen_make:
            raise ScenarioError("%s.make_index=%d duplicated" % (where, mk))
        seen_make.add(mk)


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


#: `recog_is` 只认这两值（相对**自身** ActorId 的来源分类）。
RECOG_IS_VALUES = ("self", "other")

#: 「页面」帧：说词 `SM_MERCHANTSAY(643)` / 对话框 `SM_MENU_OK(767)`——body 文本里可能带
#: `<文字/@label>` 链接。`dlg_select_any` 只在**当前页**的链接里选候选（fail-closed）。
PAGE_LINK_IDENTS = frozenset({643, 767})
_PAGE_LINK_RE = re.compile(r"<([^<>]*)/(@[A-Za-z0-9_]+)>")


def parse_page_links(text):
    """从一页文本抽 `<文字/@label>` 链接（出现序、去重）。"""
    out = []
    for m in _PAGE_LINK_RE.finditer(text or ""):
        lab = m.group(2)
        if lab not in out:
            out.append(lab)
    return out


def choose_link(candidates, links):
    """按候选顺序返回第一个出现在 `links` 里的候选（大小写不敏感）；全不中 ⇒ None。"""
    lower = {x.lower() for x in links}
    for c in candidates:
        if c.lower() in lower:
            return c
    return None


def _plan_recog_filter(val, where, params, out):
    """把等待动作里的可选来源过滤键编译进 plan（形状/互斥 fail-closed）。

    **两个键都不写 = 旧语义逐字节不变**（`plan` 里不会多出 recog/recog_is）：
      * ``recog``：int 或 params 名（加载期 `resolve_param` + i32 范围，与 `send.recog`
        同口径），按帧头 ``recog`` **精确匹配**（例：SM_STRUCK 的目标 ActorId）；
      * ``recog_is``：``"self"`` / ``"other"``，与 ``run.state["self_actor"]``（进世界
        ``SM_LOGON(50).recog``）比较；运行期还不知道自身 ActorId 时 **fail-closed**。
    两者**互斥**（同时写 = 想不清跟谁比）。
    """
    if not isinstance(val, dict):
        return
    has_recog = "recog" in val
    has_is = "recog_is" in val
    if has_recog and has_is:
        raise ScenarioError("%s: recog and recog_is are mutually exclusive (fail-closed)"
                            % where)
    if has_recog:
        out["recog"] = _need_int(resolve_param(val["recog"], params, where + ".recog"),
                                 where + ".recog", -0x80000000, 0x7FFFFFFF)
    if has_is:
        isv = val["recog_is"]
        if isv not in RECOG_IS_VALUES:
            raise ScenarioError("%s.recog_is must be one of %s, got %r (fail-closed)"
                                % (where, list(RECOG_IS_VALUES), isv))
        out["recog_is"] = isv


def plan_step(step, index, params, depth=0):
    """校验并把一步场景展开成可执行动作（不依赖运行期状态）。

    `depth` 只服务 `repeat` 的嵌套上限（≤3 层，fail-closed）。
    """
    where = "steps[%s]" % (index,)
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
    elif action == "kill":
        # 击杀步（卡 `openmir2-probe-kill-step`）：把活体夹具刷出的目标打到死亡。
        # 语义关键（别写反）：`CM_HIT(3014)` 的 **recog 是自身坐标**（PlayObject.Attack.cs:883
        # `nX == CurrX && nY == CurrY`），`tag` 是朝向；目标由 `GetPoseCreate()` 按**状态化
        # Dir** 取正前方格（PlayObject.Attack.cs:26）。所以本步先 `CM_TURN` 转身、再按同一
        # 朝向连击；`target_xy` 必须与 `self_pos` 八向相邻，否则加载期就拒（不猜）。
        _need_keys(val, where + ".kill", required=("target_xy",),
                   optional=("self_pos", "ident", "turn_ident", "max_swings",
                             "interval_ms", "wait_ident"))
        # review `ad28d643` F1：kill 的时长只由 max_swings×interval_ms 决定；通用
        # `timeout_ms` 在这里**不被读取**——静默收下会让人以为它生效（与 repeat 同口径：拒）。
        if timeout_ms is not None:
            raise ScenarioError(
                "%s.kill does not accept timeout_ms (duration = max_swings × interval_ms); "
                "删掉它或改用 max_swings/interval_ms" % where)
        out["target_xy"] = _need_int(
            resolve_param(val["target_xy"], params, where + ".kill.target_xy"),
            where + ".kill.target_xy", 0, 0xFFFFFFFF)
        raw_self = val.get("self_pos")
        if raw_self is None:
            if "self_pos" not in params:
                raise ScenarioError(
                    "%s.kill: 需要 self_pos（MakeLong(自身X,自身Y)）——声明在 params/required "
                    "里或在本步写 self_pos；没有它算不出朝向（CM_HIT 取目标用状态化 Dir）"
                    % where)
            raw_self = "self_pos"
        out["self_pos"] = _need_int(
            resolve_param(raw_self, params, where + ".kill.self_pos"),
            where + ".kill.self_pos", 0, 0xFFFFFFFF)
        out["ident"] = _need_int(resolve_param(val.get("ident", CM_HIT), params,
                                               where + ".kill.ident"),
                                 where + ".kill.ident", 0, 0xFFFF)
        out["turn_ident"] = _need_int(resolve_param(val.get("turn_ident", CM_TURN), params,
                                                    where + ".kill.turn_ident"),
                                      where + ".kill.turn_ident", 0, 0xFFFF)
        out["max_swings"] = _need_int(val.get("max_swings", 20),
                                      where + ".kill.max_swings", 1, 100)
        # 默认 1000ms：同 ident 连击之间 oracle 另有 `HitIntervalTime - HitSpeed*ItemSpeed`
        # （PlayObject.Attack.cs:850；部署 HitIntervalTime=850，Server.conf:325）——400ms 会
        # 被「攻击忙」节流吃掉一半挥击。允许覆盖，但下限仍是不同 ident 的 ActionIntervalTime=350。
        out["interval_ms"] = _need_int(val.get("interval_ms", 1000),
                                       where + ".kill.interval_ms", 350, 60000)
        wi = val.get("wait_ident", [SM_NOWDEATH, SM_DEATH])
        if isinstance(wi, int) and not isinstance(wi, bool):
            wi = [wi]
        if not isinstance(wi, list) or not wi:
            raise ScenarioError("%s.kill.wait_ident 必须是 ident 或非空 ident 数组" % where)
        out["wait_idents"] = [_need_int(x, where + ".kill.wait_ident[]") for x in wi]
        tx, ty = out["target_xy"] & 0xFFFF, (out["target_xy"] >> 16) & 0xFFFF
        sx, sy = out["self_pos"] & 0xFFFF, (out["self_pos"] >> 16) & 0xFFFF
        if (tx - sx, ty - sy) not in DIRECTION_BY_DELTA:
            raise ScenarioError(
                "%s.kill: target_xy (%d,%d) 与 self_pos (%d,%d) 不是八向相邻格——CM_HIT 只打 "
                "GetPoseCreate() 的正前方格（PlayObject.Attack.cs:26/883）；先把角色摆到目标邻格"
                % (where, tx, ty, sx, sy))
        out["recog"] = out["self_pos"]
        out["param"] = 0
        out["tag"] = DIRECTION_BY_DELTA[(tx - sx, ty - sy)]
    elif action == "say":
        if not isinstance(val, str):
            raise ScenarioError("%s.say must be a string" % where)
        out.update(ident=CM_SAY, recog=0, param=0, tag=0, series=0,
                   body=val.encode("gbk"))
    elif action == "click_npc":
        if val == "auto:last_turn":
            out.update(ident=CM_CLICKNPC, recog=0, param=0, tag=0, series=0, body=b"",
                       _auto_recog="last_turn")
        elif isinstance(val, str) and val.startswith("name:"):
            name = val[len("name:"):].strip()
            if not name:
                raise ScenarioError(
                    "%s.click_npc: `name:` 后必须跟 NPC 名（fail-closed）" % where)
            if "?allow_any" in name:
                raise ScenarioError(
                    "%s.click_npc: `allow_any` 只适用于 `pos:`（`name:%s` 不接受查询参数）"
                    % (where, name))
            out.update(ident=CM_CLICKNPC, recog=None, param=0, tag=0, series=0, body=b"",
                       _npc_selector={"kind": "name", "value": name})
        elif isinstance(val, str) and val.startswith("pos:"):
            spec = val[len("pos:"):]
            m = re.match(r"^(\d{1,5}),(\d{1,5})(?:\?allow_any=([01]))?$", spec)
            if m is None:
                raise ScenarioError(
                    "%s.click_npc: `pos:` 必须写成 pos:<x>,<y>（可选 `?allow_any=1`），"
                    "得到 %r" % (where, spec))
            x, y = int(m.group(1)), int(m.group(2))
            if x > 0xFFFF or y > 0xFFFF:
                raise ScenarioError(
                    "%s.click_npc: pos 坐标越界（0..65535）：%r" % (where, spec))
            out.update(ident=CM_CLICKNPC, recog=None, param=0, tag=0, series=0, body=b"",
                       _npc_selector={"kind": "pos", "x": x, "y": y,
                                      "allow_any": m.group(3) == "1"})
        elif isinstance(val, str) and ":" in val:
            raise ScenarioError(
                "%s.click_npc: 未知寻址写法 %r（只认 auto:last_turn / name:<名> / pos:<x>,<y>；"
                "其余按 params 名或数字）" % (where, val))
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
    elif action == "dlg_select_any":
        # `["@rw02","@rw01"]`：按顺序选**当前页实际存在**的第一个链接；全不存在 ⇒ 运行期
        # StepFailure 并打印当前页链接清单（fail-closed，不静默跳过）。
        if not isinstance(val, list) or not val:
            raise ScenarioError("%s.dlg_select_any must be a non-empty list of label strings"
                                % where)
        seen = set()
        for c in val:
            if not isinstance(c, str) or len(c) < 2 or not c.startswith("@"):
                raise ScenarioError(
                    "%s.dlg_select_any[] entries must be labels starting with '@' (got %r)"
                    % (where, c))
            if c.lower() in seen:
                raise ScenarioError(
                    "%s.dlg_select_any has duplicate candidate %r (ordered candidates must be "
                    "unique; fail-closed)" % (where, c))
            seen.add(c.lower())
        out.update(ident=CM_MERCHANTDLGSELECT, recog=None, param=0, tag=0, series=0,
                   body=b"", candidates=list(val), _needs_merchant_recog=True)
    elif action == "newchr":
        # `CM_NEWCHR(101)`：SelGate → DB `UserService.NewChr`（`:566-666`）。载荷
        # `account/name/hair/job/sex`（`BotSrv/.../SelectChrScene.cs:237-241` 的
        # `SendNewChr(uid,uname,shair,sjob,ssex)`）；account 运行期才知（登录账号）。
        # 本步只能在 steps[0] 且整个场景至多一次（进世界前由 `do_select_char` 执行）。
        _need_keys(val, where + ".newchr", required=("name",),
                   optional=("job", "hair", "sex", "on_exists"))
        name = resolve_str_param(val["name"], params, where + ".newchr.name").strip()
        if not name:
            raise ScenarioError("%s.newchr.name must not be blank" % where)
        if "/" in name or "\\" in name:
            raise ScenarioError("%s.newchr.name %r must not contain '/' or '\\' "
                                "(payload separator; fail-closed)" % (where, name))
        if len(name) < 3 or len(name) > 20:
            raise ScenarioError(
                "%s.newchr.name %r length %d out of range 3..20 chars "
                "(oracle DB `NewChr`: len<3 ⇒ nCode 0; characters.ChrName varchar(20))"
                % (where, name, len(name)))
        job = _need_int(resolve_param(val.get("job", 0), params, where + ".newchr.job"),
                        where + ".newchr.job", 0, 2)
        hair = _need_int(resolve_param(val.get("hair", 0), params, where + ".newchr.hair"),
                         where + ".newchr.hair", 0, 255)
        sex = _need_int(resolve_param(val.get("sex", 0), params, where + ".newchr.sex"),
                        where + ".newchr.sex", 0, 1)
        on_exists = val.get("on_exists", "reuse")
        if on_exists not in ("reuse", "fail"):
            raise ScenarioError("%s.newchr.on_exists must be 'reuse' or 'fail', got %r"
                                % (where, on_exists))
        out.update(ident=CM_NEWCHR, recog=0, param=0, tag=0, series=0, body=b"",
                   name=name, job=job, hair=hair, sex=sex, on_exists=on_exists)
    elif action == "repeat":
        _need_keys(val, where + ".repeat", required=("times", "steps"),
                   optional=("stop_on_select",))
        times = _need_int(val["times"], where + ".repeat.times", 1, 100)
        sub = val["steps"]
        if not isinstance(sub, list) or not sub:
            raise ScenarioError("%s.repeat.steps must be a non-empty list" % where)
        if depth >= 3:
            raise ScenarioError("%s.repeat: nesting depth > 3 refused (fail-closed)" % where)
        stop = val.get("stop_on_select")
        if stop is not None and (not isinstance(stop, str) or not stop):
            raise ScenarioError("%s.repeat.stop_on_select must be a non-empty string" % where)
        if timeout_ms is not None:
            raise ScenarioError(
                "%s.repeat.timeout_ms is not supported: timing is per inner step "
                "(give the wait_for steps their own timeout_ms); fail-closed" % where)
        subplans = [plan_step(x, "%s.%d" % (index, j), params, depth + 1)
                    for j, x in enumerate(sub)]
        if stop is not None:
            cands = set()
            for leaf in _iter_plan_leaves(subplans):
                if leaf["kind"] == "dlg_select_any":
                    cands.update(c.lower() for c in leaf["candidates"])
            if stop.lower() not in cands:
                raise ScenarioError(
                    "%s.repeat.stop_on_select %r is not one of the inner dlg_select_any "
                    "candidates %s (a typo would silently exhaust all attempts; fail-closed)"
                    % (where, stop, sorted(cands)))
        out.update(times=times, steps=subplans, stop_on_select=stop)
    elif action == "wait_for":
        # 旧形状（int / 非空 int 数组）原样保留；要来源过滤才写对象形状：
        #   {"idents": 31 | [31, 642], "recog": <int|params 名> | "recog_is": "self"|"other"}
        if isinstance(val, dict):
            _need_keys(val, where + ".wait_for", required=("idents",),
                       optional=("recog", "recog_is"))
            idents_val = val["idents"]
        else:
            idents_val = val
        if isinstance(idents_val, int) and not isinstance(idents_val, bool):
            idents = [_need_int(idents_val, where + ".wait_for")]
        elif isinstance(idents_val, list) and idents_val:
            idents = [_need_int(x, where + ".wait_for[]") for x in idents_val]
        else:
            raise ScenarioError("%s.wait_for must be an int, a non-empty list, or "
                                "{idents, recog|recog_is}" % where)
        out["idents"] = idents
        _plan_recog_filter(val, where + ".wait_for", params, out)
    elif action == "wait_frames":
        _need_keys(val, where + ".wait_frames", required=("count",),
                   optional=("idents", "timeout_ms", "recog", "recog_is"))
        out["count"] = _need_int(val["count"], where + ".wait_frames.count", 0, 1000000)
        ids = val.get("idents") or []
        if not isinstance(ids, list):
            raise ScenarioError("%s.wait_frames.idents must be a list" % where)
        out["idents"] = [_need_int(x, where + ".wait_frames.idents[]") for x in ids]
        if "timeout_ms" in val:
            out["timeout_ms"] = _need_int(val["timeout_ms"],
                                          where + ".wait_frames.timeout_ms", 0, 3600000)
        _plan_recog_filter(val, where + ".wait_frames", params, out)
    elif action == "wait_idents":
        _need_keys(val, where + ".wait_idents", required=("idents",),
                   optional=("timeout_ms", "recog", "recog_is"))
        ids = val["idents"]
        if not isinstance(ids, list) or not ids:
            raise ScenarioError("%s.wait_idents.idents must be a non-empty list" % where)
        out["idents"] = [_need_int(x, where + ".wait_idents.idents[]") for x in ids]
        if "timeout_ms" in val:
            out["timeout_ms"] = _need_int(val["timeout_ms"],
                                          where + ".wait_idents.timeout_ms", 0, 3600000)
        _plan_recog_filter(val, where + ".wait_idents", params, out)
    elif action == "sleep":
        if isinstance(val, int) and not isinstance(val, bool):
            out["ms"] = _need_int(val, where + ".sleep", 0, 3600000)
        else:
            _need_keys(val, where + ".sleep", required=("ms",))
            out["ms"] = _need_int(val["ms"], where + ".sleep.ms", 0, 3600000)
    return out


def _iter_plan_leaves(plans):
    """展开后的 plan 树 → 叶子动作（`repeat` 递归）。语义校验/方向检查都看叶子。"""
    for p in plans:
        if p["kind"] == "repeat":
            for leaf in _iter_plan_leaves(p["steps"]):
                yield leaf
        else:
            yield p


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
SEND_KINDS = frozenset({"send", "say", "click_npc", "dlg_select", "dlg_select_any",
                        "newchr", "kill"})
# 累计观察动作：等的是「累计出现过」而不是「本步之后的第一条回包」——驱动器的登录链
# （CM_IDPASSWORD/SELECTSERVER/QUERYCHR/SELCHR + GameGate 登录）与自动握手
# （CM_LOGINNOTICEOK / CM_QUERYBAGITEMS）本来就会发帧，所以纯观察场景（login_only）合法。
# `kill` 也归入累计观察（卡 `openmir2-probe-kill-step`）：它内部等死亡帧，
# 只含 kill 的场景本身就有「观察」语义，不需要再补一条空 wait。
CUMULATIVE_WAIT_KINDS = frozenset({"wait_idents", "wait_frames", "kill"})
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
    """收集场景里**按变量名**引用的字段：[(where, name)]（`repeat` 内部递归）。"""
    refs = []

    def walk(steps, prefix):
        for i, step in enumerate(steps or []):
            if not isinstance(step, dict):
                continue
            where = "%s[%d]" % (prefix, i)
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
            if (isinstance(v, str) and v != "auto:last_turn"
                    and not v.startswith(("name:", "pos:"))):
                refs.append((where + ".click_npc", v))
            v = step.get("dlg_select")
            if isinstance(v, dict) and isinstance(v.get("recog"), str):
                refs.append((where + ".dlg_select.recog", v["recog"]))
            for kind in ("wait_for", "wait_frames", "wait_idents"):
                v = step.get(kind)
                if isinstance(v, dict) and isinstance(v.get("recog"), str):
                    refs.append((where + "." + kind + ".recog", v["recog"]))
            nc = step.get("newchr")
            if isinstance(nc, dict):
                for key in ("name", "job", "hair", "sex"):
                    v = nc.get(key)
                    if isinstance(v, str):
                        refs.append((where + ".newchr." + key, v))
            rep = step.get("repeat")
            if isinstance(rep, dict) and isinstance(rep.get("steps"), list):
                walk(rep["steps"], where + ".repeat.steps")

    walk(data.get("steps") or [], "steps")
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
      R3 所有 send/wait_* 的 ident 必须存在于协议常量表**且方向正确**：发帧动作（send/say/
         click_npc/dlg_select）是 C2S ⇒ 必须 CM_*；观察动作（wait_for/wait_frames/wait_idents）
         等的是 S2C ⇒ 必须 SM_*（表外 ident 与方向反了都等于「永远发不出/等不到」）；
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
    # `newchr` 只能在 steps[0] 且至多一次（它在选角相位执行，之后的 waits 才有意义）。
    nc_idx = [i for i, p in enumerate(plans) if p["kind"] == "newchr"]
    if len(nc_idx) > 1:
        raise ScenarioError("scenario %r has %d newchr steps; at most one is allowed"
                            % (name, len(nc_idx)))
    if nc_idx and nc_idx[0] != 0:
        raise ScenarioError(
            "scenario %r: newchr must be steps[0] (it runs in the select-char phase, before "
            "enter-world; later ones would be unreachable)" % name)
    # 生效参数 = 场景 params + --var 覆盖（R5 用；R4 的「声明」口径刻意不含 --var）。
    eff_params = dict(data.get("params") or {})
    if params:
        eff_params.update(params)
    declared = (set((data.get("params") or {}).keys())
                | set(data.get("required") or [])
                | set((data.get("computed") or {}).keys()))

    # R2：wait_for 之前必须已有发帧步骤。先判这一步，坏样例拿到最具体的诊断。
    leaves = list(_iter_plan_leaves(plans))
    first_send = None
    for p in leaves:
        if p["kind"] in SEND_KINDS:
            if first_send is None:
                first_send = p["index"]
        elif p["kind"] == "wait_for" and first_send is None:
            raise ScenarioError(
                "steps[%s].wait_for %s: no send step precedes it — nothing in this scenario can "
                "produce that response (a scenario that only waits proves nothing; add one of "
                "%s before it)"
                % (p["index"], p["idents"], "/".join(sorted(SEND_KINDS))))

    kinds = [p["kind"] for p in leaves]

    # R1：至少一个发帧或累计观察步骤。
    if not any(k in SEND_KINDS or k in CUMULATIVE_WAIT_KINDS for k in kinds):
        raise ScenarioError(
            "scenario %r has no send step (%s) and no cumulative wait step (%s): "
            "steps contain only %s, so it neither triggers nor observes any frame"
            % (name, "/".join(sorted(SEND_KINDS)), "/".join(sorted(CUMULATIVE_WAIT_KINDS)),
               "/".join(sorted(set(kinds)))))

    # R3：ident 必须存在于协议常量表，**且方向必须对**。
    #   * 发帧动作（send/say/click_npc/dlg_select）是 C2S ⇒ 必须 CM_*；
    #   * 观察动作（wait_for/wait_frames/wait_idents）等的是 S2C ⇒ 必须 SM_*。
    # 并集口径的已知歧义：CM/SM 有 6 个**同值** ident（42/100/101/102/103/104），同一个号
    # 在两个命名空间里各有名字（例：100 = CM_QUERYCHR = SM_SYSMESSAGE），方向无法由号判定
    # ⇒ 两个方向都放行（集合与处理见 docs/probe-ident-direction-20261023.md；不是静默放过：
    # 代码在这里点名，docs 逐条列出）。ISM_* 不进表（解析器只认 `pub const (CM|SM)_`）。
    for p in leaves:
        if p["kind"] in SEND_KINDS:
            ident = p["ident"]
            if ident not in messages["CM"]:
                if ident in messages["SM"]:
                    raise ScenarioError(
                        "steps[%s].%s ident %d is %s, expected CM_* (%s are client→server "
                        "frames; an SM_* ident is server→client and can never be sent)"
                        % (p["index"], p["kind"], ident, messages["SM"][ident],
                           "/".join(sorted(SEND_KINDS))))
                raise ScenarioError(
                    "steps[%s].%s.ident %d is not a CM_*/SM_* constant in %s"
                    % (p["index"], p["kind"], ident, MESSAGES_RS_REL))
        elif p["kind"] in WAIT_KINDS:
            for ident in p["idents"]:
                if ident not in messages["SM"]:
                    if ident in messages["CM"]:
                        raise ScenarioError(
                            "steps[%s].%s ident %d is %s, expected SM_* (a wait can only match "
                            "server→client frames; a CM_* ident is client→server and can never "
                            "arrive)" % (p["index"], p["kind"], ident, messages["CM"][ident]))
                    raise ScenarioError(
                        "steps[%s].%s ident %d is not a CM_*/SM_* constant in %s "
                        "(typo? this wait can never match)"
                        % (p["index"], p["kind"], ident, MESSAGES_RS_REL))

    # R3b（kill，卡 `openmir2-probe-kill-step`）：`ident`/`turn_ident` 必须是 CM_*，
    # `wait_ident` 必须是 SM_*——与 R3 同口径，只是 kill 一步里同时出现三组 ident。
    for p in leaves:
        if p["kind"] != "kill":
            continue
        for key in ("ident", "turn_ident"):
            ident = p[key]
            if ident not in messages["CM"]:
                if ident in messages["SM"]:
                    raise ScenarioError(
                        "steps[%s].kill.%s ident %d is %s, expected CM_* (client→server)"
                        % (p["index"], key, ident, messages["SM"][ident]))
                raise ScenarioError(
                    "steps[%s].kill.%s ident %d is not a CM_*/SM_* constant in %s"
                    % (p["index"], key, ident, MESSAGES_RS_REL))
        for ident in p["wait_idents"]:
            if ident not in messages["SM"]:
                if ident in messages["CM"]:
                    raise ScenarioError(
                        "steps[%s].kill.wait_ident %d is %s, expected SM_* (a wait can only "
                        "match server→client frames)" % (p["index"], ident,
                                                         messages["CM"][ident]))
                raise ScenarioError(
                    "steps[%s].kill.wait_ident %d is not a CM_*/SM_* constant in %s"
                    % (p["index"], ident, MESSAGES_RS_REL))

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
    for p in leaves:
        if p["kind"] != "send":
            continue
        if direction_spec is None:
            direction_spec = load_direction_spec(messages=messages)
        hit_idents, dirs = direction_spec
        if p["ident"] not in hit_idents:
            continue
        eff_dir = p["tag"] & 0xFF          # oracle: (byte)Tag
        if eff_dir not in dirs:
            print("WARN: steps[%s].send.tag %d (ident %d, attack family) is not a valid Direction: "
                  "oracle takes nDir = (byte)Tag = %d, valid %s (source %s HIT_IDENTS/DIR_*). "
                  "真实客户端不会发送越界方向；该值**不影响本次命中**（取目标用的是状态化 Dir，"
                  "nDir 在取目标之后才写入），但会污染角色的状态 Dir，影响动作帧与**此后**的取目标。"
                  % (p["index"], p["tag"], p["ident"], eff_dir,
                     "%d..%d" % (min(dirs), max(dirs)), ATTACKWIRE_RS_REL), file=sys.stderr)

    # W：必须有等待步骤（N-5：只发不等证明不了任何东西）。
    if not any(k in WAIT_KINDS for k in kinds):
        raise ScenarioError(
            "scenario %r has no wait step: at least one of %s is required "
            "(a send-only scenario proves nothing)"
            % (name, sorted(WAIT_KINDS | {"kill"})))
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
        self.state = {"last_merchant_recog": None, "self_actor": None,
                      "last_turn_recog": None, "page": None,
                      "last_selected_label": None}
        self.gamegate = None
        self.conns = []
        self.dwell = None

    # ---- 自动握手（全在 GameGate 相位） ----
    def on_frame(self, conn, frame, seq):
        if conn.label != "gamegate":
            return
        if frame.ident == SM_MERCHANTSAY:
            self.state["last_merchant_recog"] = frame.recog
        # **W-LF-1 首包**：`SM_LOGON`（PlayObject.cs:1131）携带 x/y + MakeWord(Dir,Light)；
        # 地图面：`SM_NEWMAP(51)`（`PlayObject.Message.cs:1680-1684` 的 RM_LOGON 支）body =
        # `Envir.MapFileName`；`SM_MAPDESCRIPTION(54)` body = `Envir.MapDesc`。
        # **`SM_CHANGEMAP(634)` 只在换图（RM_CHANGEMAP）时发**，首登不换图 ⇒ 不能用 634 当首包地图
        # （review F1）。逻辑 MapName 不上线，见 docs/probe-newchr-20261025.md §2.3。
        if frame.ident == SM_LOGON and "first_pack" not in self.state:
            self.state["first_pack"] = {
                "seq": seq, "actor_id": frame.recog, "x": frame.param, "y": frame.tag,
                "series": frame.series, "dir": frame.series & 0xFF,
                "light": (frame.series >> 8) & 0xFF, "body_hex": frame.body.hex(),
                "map_file": None, "map_desc": None,
            }
        elif frame.ident == SM_MAPDESCRIPTION:
            fp = self.state.get("first_pack")
            if fp is not None and fp["map_desc"] is None:
                fp["map_desc"] = decode_text(frame.body_decoded, "gbk", "replace")
        elif frame.ident == SM_NEWMAP:
            fp = self.state.get("first_pack")
            if fp is not None and fp["map_file"] is None:
                fp["map_file"] = decode_text(frame.body_decoded, "gbk", "replace")
        if frame.ident in PAGE_LINK_IDENTS:
            # 记「当前页」：`dlg_select_any` 只在这一页的链接里选候选。文本按 GBK 宽松解码
            # （坏字节只影响链接抽取，不影响其它判据）。
            text = decode_text(frame.body_decoded, "gbk", "replace")
            self.state["page"] = {"ident": frame.ident, "seq": seq, "text": text,
                                  "links": parse_page_links(text)}
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

    def _resolve_npc_selector(self, conn, sel, idx):
        """`click_npc` 的 name:/pos: 寻址：唯一命中原样返回；0/多命中一律 StepFailure。

        `pos:` 追加**对象类别门**（N2）：只接受启发式的「固定/静态对象」（无移动帧、坐标不变、
        首见于初始快照），否则 fail-closed 并列出该格对象与类别；命中之前判定，**绝不先选中再报错**。
        `pos:<x>,<y>?allow_any=1` = 默认关闭的显式逃生阀（文档写清降级理由）。
        """
        snap = self.state.get("snapshot_end_seq")
        rows, order = scan_actors(conn.frames, self.state.get("self_actor"))
        actors = [rows[a] for a in order if not rows[a]["disappeared"]]
        allow_any = bool(sel.get("allow_any"))
        if sel["kind"] == "name":
            hits = [a for a in actors if a.get("name") == sel["value"]]
            what = "name=%s" % sel["value"]
        else:
            cell = [a for a in actors if a["x"] == sel["x"] and a["y"] == sel["y"]]
            what = "pos=(%d,%d)" % (sel["x"], sel["y"])
            static_cell = [a for a in cell if actor_static(a, snap)[0]]
            if cell and not static_cell and not allow_any:
                raise StepFailure(
                    "steps[%s].click_npc: %s 上的对象都**不是固定/静态形态**"
                    "（启发式：无移动帧 + 坐标不变 + 首见于初始快照；SM_TURN 体没有类别字段）"
                    "⇒ fail-closed，不选中：%s。确需按坐标选中活动对象请显式写 "
                    "`pos:%d,%d?allow_any=1`（默认关闭）"
                    % (idx, what, format_actors(cell, snap), sel["x"], sel["y"]))
            hits = cell if allow_any else static_cell
        if len(hits) == 1:
            ok, why = actor_static(hits[0], snap)
            hits[0]["_static_why"] = why if ok else "allow_any 放行（不是静态）"
            hits[0]["_allow_any"] = allow_any
            return hits[0]["actor"], hits[0]
        if not actors:
            raise StepFailure(
                "steps[%s].click_npc: 还没看到任何非己对象（无 SM_TURN(10)）⇒ 无法用 %s 寻址"
                "（fail-closed；先 wait_for 等到出现帧）" % (idx, what))
        if not hits:
            near = actors
            if sel["kind"] == "pos":
                near = [a for a in actors
                        if abs(a["x"] - sel["x"]) <= 3 and abs(a["y"] - sel["y"]) <= 3]
            raise StepFailure(
                "steps[%s].click_npc: 没有对象匹配 %s；%s%s"
                % (idx, what,
                   "该格邻近(±3)可见对象：" if (sel["kind"] == "pos" and near) else "可见对象：",
                   format_actors(near if near else actors, snap)))
        raise StepFailure(
            "steps[%s].click_npc: %s 命中 %d 个对象（歧义，fail-closed，不挑第一个）：%s"
            % (idx, what, len(hits), format_actors(hits, snap)))

    def _recog_filter(self, plan, idx):
        """把 plan 里的 ``recog`` / ``recog_is`` 编译成 ``frame -> bool``（None = 不过滤）。

        ``recog_is`` 依赖 ``self.state["self_actor"]``（``do_enter_world`` 从
        ``SM_LOGON(50).recog`` 学到）。**还不知道自身 ActorId 时不许猜**：直接
        ``ScenarioError``（退出码 2），否则 self/other 过滤会静默退化成恒真/恒假。
        """
        if plan.get("recog_is") is not None:
            self_actor = self.state.get("self_actor")
            if self_actor is None:
                raise ScenarioError(
                    "steps[%s]: recog_is=%r needs the self actor id (SM_LOGON(50).recog), "
                    "which has not been observed; refusing to guess (fail-closed)"
                    % (idx, plan["recog_is"]))
            want_self = plan["recog_is"] == "self"
            return lambda fr: (fr.recog == self_actor) == want_self
        if plan.get("recog") is not None:
            want = plan["recog"]
            return lambda fr: fr.recog == want
        return None

    def _execute_kill(self, conn, plan):
        """击杀步（卡 `openmir2-probe-kill-step`）：转身 → 连击 → 等死亡帧。

        三态失败可分（卡面验收②）：
          ① `no target frames at (x,y)` —— 整个窗口没有目标格的任何帧（`fixtures.monsters`
             未生效 / 落点不同格；也含「0 伤害打空」：SM_STRUCK 只在 wParam>0 时才发，
             PlayObject.Message.cs:1564）；
          ② `target alive after N swing(s)` —— 收到过目标格的活动帧（SM_TURN/SM_DEATH 带坐标）
             或带伤害的他人 SM_STRUCK，但没等到死亡帧；
          ③ 「死了但屠宰不接受」不在本步——由后续 `wait_for(33/637)` 超时表达（不同的步）。
        """
        idx = plan["index"]
        tx = plan["target_xy"] & 0xFFFF
        ty = (plan["target_xy"] >> 16) & 0xFFFF
        sx = plan["self_pos"] & 0xFFFF
        sy = (plan["self_pos"] >> 16) & 0xFFFF
        direction = DIRECTION_BY_DELTA.get((tx - sx, ty - sy))
        if direction is None:  # plan_step 已拒；这里再兜一次（fail-closed）
            raise ScenarioError(
                "steps[%s].kill: target (%d,%d) 与自身 (%d,%d) 非八向相邻"
                % (idx, tx, ty, sx, sy))
        interval = plan["interval_ms"]
        death = set(plan["wait_idents"])
        start_seq = conn.recorder.seq
        # 1) 转身：CM_TURN 的 tag=Dir、recog=自身坐标（ClientChangeDir 只在 x/y==自身时改 Dir）
        raw = conn.send(plan["self_pos"], plan["turn_ident"], 0, direction, 0, b"")
        self._log_send(conn, raw, {"index": idx, "kind": "kill.turn",
                                   "ident": plan["turn_ident"],
                                   "recog": plan["self_pos"], "tag": direction,
                                   "note": plan.get("note", "")},
                       now_ms() - self.t0_ms)
        conn.pump(interval)  # 不同 ident 之间要 >= ActionIntervalTime(350ms)
        swings = 0
        matched = None
        while swings < plan["max_swings"]:
            raw = conn.send(plan["self_pos"], plan["ident"], 0, direction, 0, b"")
            swings += 1
            self._log_send(conn, raw, {"index": idx, "kind": "kill.swing",
                                       "ident": plan["ident"],
                                       "recog": plan["self_pos"], "tag": direction,
                                       "swing": swings,
                                       "note": plan.get("note", "")},
                           now_ms() - self.t0_ms)
            before = conn.recorder.seq
            conn.pump(interval)
            for seq, fr in conn.frames:
                if seq > before and fr.ident in death and fr.param == tx and fr.tag == ty:
                    matched = (seq, fr)
                    break
            if matched:
                break
        rec = {"index": idx, "kind": "kill", "target_xy": plan["target_xy"],
               "self_pos": plan["self_pos"], "direction": direction,
               "swings": swings, "max_swings": plan["max_swings"],
               "interval_ms": interval, "wait_idents": sorted(death),
               "note": plan.get("note", "")}
        if matched:
            seq, fr = matched
            rec.update(result="dead", matched_seq=seq, matched_ident=fr.ident,
                       matched_recog=fr.recog)
            self.steps.append(rec)
            return
        window = [(s_, fr) for (s_, fr) in conn.frames if s_ > start_seq]
        self_actor = self.state.get("self_actor")
        struck = [(s_, fr.recog, fr.series) for (s_, fr) in window
                  if fr.ident == SM_STRUCK and fr.series > 0
                  and (self_actor is None or fr.recog != self_actor)]
        at_cell = [(s_, fr.ident, fr.recog) for (s_, fr) in window
                   if fr.param == tx and fr.tag == ty
                   and fr.ident in (SM_TURN, SM_DEATH, SM_NOWDEATH)]
        rec.update(result=("alive" if (struck or at_cell) else "no-target"),
                   struck=struck[:5], frames_at_cell=at_cell[:5])
        self.steps.append(rec)
        if struck or at_cell:
            raise StepFailure(
                "steps[%s].kill: target alive after %d swing(s) at (%d,%d) — no death frame "
                "among %s; damage-carrying SM_STRUCK(31)=%r; frames at cell(SM_TURN/DEATH)=%r"
                % (idx, swings, tx, ty, sorted(death), rec["struck"],
                   rec["frames_at_cell"]))
        raise StepFailure(
            "steps[%s].kill: no target frames at (%d,%d) after %d swing(s) — "
            "fixtures.monsters 未生效/落点不同格（或 0 伤害打空：SM_STRUCK 只在 wParam>0 "
            "时才发，PlayObject.Message.cs:1564）" % (idx, tx, ty, swings))

    def execute_step(self, conn, plan):
        idx = plan["index"]
        kind = plan["kind"]
        # 「玩家登录后的第一份可见快照」界：进世界 + dwell 之后、**第一个场景步**执行前的帧序号。
        # 固定 NPC 随地图加载下发（早于此刻），怪是后来刷的（晚于此刻）——`pos:` 的静态判定用它
        # （卡 `openmir2-probe-npc-addressing-followups` 的 N2；启发式，不是协议级类别）。
        if "snapshot_end_seq" not in self.state and conn is not None:
            self.state["snapshot_end_seq"] = getattr(conn.recorder, "seq", None)
        default_timeout = self.args.timeout_ms
        if kind == "kill":
            self._execute_kill(conn, plan)
            return
        if kind == "sleep":
            time.sleep(plan["ms"] / 1000.0)
            self.steps.append({"index": idx, "kind": "sleep", "ms": plan["ms"]})
            return
        if kind == "dlg_select_any":
            recog = plan.get("recog")
            if recog is None:
                recog = self.state["last_merchant_recog"]
            if recog is None:
                raise ScenarioError(
                    "steps[%s].dlg_select_any: no SM_MERCHANTSAY(643) seen yet; cannot infer "
                    "NPC recog (fail-closed)" % plan["index"])
            page = self.state.get("page")
            links = list(page["links"]) if page else []
            chosen = choose_link(plan["candidates"], links)
            if chosen is None:
                raise StepFailure(
                    "steps[%s].dlg_select_any: none of %r is a link on the current page "
                    "(page ident=%r seq=%r); page links=%r; page text=%r"
                    % (plan["index"], plan["candidates"],
                       page.get("ident") if page else None,
                       page.get("seq") if page else None,
                       links, page.get("text") if page else None))
            body = chosen.encode("gbk")
            raw = conn.send(recog, CM_MERCHANTDLGSELECT, 0, 0, 0, body)
            self._log_send(conn, raw, dict(plan, recog=recog, body=body),
                           now_ms() - self.t0_ms)
            self.state["last_selected_label"] = chosen
            self.steps.append({"index": plan["index"], "kind": "dlg_select_any",
                               "candidates": plan["candidates"], "links": links,
                               "chosen": chosen, "recog": recog,
                               "note": plan.get("note", "")})
            return
        if kind == "repeat":
            attempts = []
            for k in range(plan["times"]):
                self.state["last_selected_label"] = None
                for j, sub in enumerate(plan["steps"]):
                    # 内层 pacing 与顶层同口径（`delay_ms` 缺省取 `--between-ms`）；
                    # 整个 repeat 的第一步不额外延迟（顶层循环已在 repeat 之前等过）。
                    if not (k == 0 and j == 0):
                        delay = (sub["delay_ms"] if sub["delay_ms"] is not None
                                 else getattr(self.args, "between_ms", 0))
                        if delay:
                            time.sleep(delay / 1000.0)
                    self.execute_step(conn, sub)
                page = self.state.get("page")
                attempts.append({"attempt": k + 1,
                                 "links": list(page["links"]) if page else [],
                                 "selected": self.state.get("last_selected_label")})
                want = plan.get("stop_on_select")
                if want and self.state.get("last_selected_label") == want:
                    self.steps.append({"index": plan["index"], "kind": "repeat",
                                       "times": plan["times"], "stop_on_select": want,
                                       "attempts": attempts, "stopped": True,
                                       "note": plan.get("note", "")})
                    return
            raise StepFailure(
                "steps[%s].repeat: stop_on_select %r not reached in %d attempt(s); "
                "per-attempt pages (links/selected)=%r"
                % (plan["index"], plan.get("stop_on_select"), plan["times"], attempts))
        if kind == "send" or kind in ("say", "click_npc", "dlg_select"):
            recog = plan.get("recog")
            if kind == "click_npc" and plan.get("_auto_recog") == "last_turn":
                recog = self.state.get("last_turn_recog")
                if recog is None:
                    raise ScenarioError(
                        "steps[%s].click_npc: no non-self SM_TURN(10) seen yet; "
                        "cannot infer NPC recog (fail-closed)" % idx)
            if kind == "click_npc" and plan.get("_npc_selector"):
                recog, picked = self._resolve_npc_selector(conn, plan["_npc_selector"], idx)
                self.steps.append({"index": idx, "kind": "click_npc",
                                   "selector": plan["_npc_selector"], "actor": recog,
                                   "name": picked.get("name"), "x": picked["x"],
                                   "y": picked["y"], "tail": picked.get("tail"),
                                   "first_seq": picked.get("first_seq"),
                                   "moves": picked.get("moves"),
                                   "static_why": picked.get("_static_why"),
                                   "note": plan.get("note", "")})
            if kind == "dlg_select" and plan.get("_needs_merchant_recog"):
                recog = self.state["last_merchant_recog"]
                if recog is None:
                    raise ScenarioError(
                        "steps[%s].dlg_select: no SM_MERCHANTSAY(643) seen yet; "
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
            recog_filter = self._recog_filter(plan, idx)
            got = conn.wait_for(plan["idents"], timeout, after, recog_filter)
            rec = {"index": idx, "kind": "wait_for", "idents": plan["idents"],
                   "timeout_ms": timeout, "t_ms": t_start - self.t0_ms,
                   "waited_ms": now_ms() - t_start, "matched": got is not None,
                   "note": plan.get("note", "")}
            if recog_filter is not None:
                # 加性字段：只在用了来源过滤时出现（旧场景的 run.json 逐字节不变）
                rec["recog"] = plan.get("recog")
                rec["recog_is"] = plan.get("recog_is")
            if got is None:
                self.steps.append(rec)
                raise StepFailure("steps[%s]: timeout waiting for ident(s) %s after %.0fms"
                                  % (idx, plan["idents"], timeout))
            seq, fr = got
            rec.update(matched_seq=seq, matched_ident=fr.ident, matched_recog=fr.recog,
                       matched_body_hex=fr.body.hex())
            self.steps.append(rec)
            return
        if kind == "wait_frames":
            timeout = plan["timeout_ms"] if plan.get("timeout_ms") is not None else default_timeout
            t_start = now_ms()
            recog_filter = self._recog_filter(plan, idx)
            ok = conn.wait_frames(plan["count"], plan["idents"], timeout, recog_filter)
            seen = {fr.ident for _, fr in conn.frames
                    if recog_filter is None or recog_filter(fr)}
            rec = {"index": idx, "kind": "wait_frames", "count": plan["count"],
                   "idents": plan["idents"], "timeout_ms": timeout,
                   "t_ms": t_start - self.t0_ms, "waited_ms": now_ms() - t_start,
                   "total_frames": len(conn.frames),
                   "seen_idents": sorted(seen),
                   "matched": ok, "note": plan.get("note", "")}
            if recog_filter is not None:
                rec["recog"] = plan.get("recog")
                rec["recog_is"] = plan.get("recog_is")
            self.steps.append(rec)
            if not ok:
                raise StepFailure("steps[%s]: wait_frames count=%d idents=%s not met in %.0fms "
                                  "(have %d frames)"
                                  % (idx, plan["count"], plan["idents"], timeout,
                                     len(conn.frames)))
            return
        if kind == "wait_idents":
            timeout = plan["timeout_ms"] if plan["timeout_ms"] is not None else default_timeout
            t_start = now_ms()
            recog_filter = self._recog_filter(plan, idx)
            missing, waited = conn.wait_idents(plan["idents"], timeout, recog_filter)
            seen = sorted({fr.ident for _, fr in conn.frames
                           if recog_filter is None or recog_filter(fr)})
            rec = {"index": idx, "kind": "wait_idents", "idents": plan["idents"],
                   "timeout_ms": timeout, "t_ms": t_start - self.t0_ms,
                   "waited_ms": waited, "seen_idents": seen,
                   "missing_idents": missing, "matched": not missing,
                   "note": plan.get("note", "")}
            if recog_filter is not None:
                rec["recog"] = plan.get("recog")
                rec["recog_is"] = plan.get("recog_is")
            self.steps.append(rec)
            if missing:
                raise StepFailure(
                    "steps[%s]: timeout waiting for ident(s) %s after %.0fms "
                    "(missing %s; seen %s)"
                    % (idx, plan["idents"], timeout, missing, seen))
            return
        raise ScenarioError("steps[%s]: unhandled action %r" % (idx, kind))


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


def _query_chr(run, conn, args, session):
    """`CM_QUERYCHR` 一轮：返回角色列表（`SM_QUERYCHR_FAIL` ⇒ StepFailure）。"""
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
    return parse_char_list(_decode_body_text(fr, "SM_QUERYCHR"))


def _do_newchr(run, conn, args, session, plan, names):
    """执行 `newchr` 步（SelGate 相位）：`CM_NEWCHR(101)` → 成功/复用/显式失败。

    oracle 链：`BotSrv/.../SelectChrScene.cs:237-241` 的 `SendNewChr(uid,uname,shair,sjob,ssex)`
    ⇒ SelGate 透传 DB ⇒ `DBSrv UserService.NewChr`（`:566-666`）⇒
    `SM_NEWCHR_SUCCESS(521)` / `SM_NEWCHR_FAIL(522, Recog=nCode)`；
    成功时 Bot 侧 `RobotPlayer.Operate.cs:175-176` 会 **重新 `SendQueryChr()`**（本函数同样重查）。
    """
    name = plan["name"]
    if args.char_name and args.char_name.lower() != name.lower():
        raise ScenarioError(
            "--char-name %r conflicts with scenario newchr.name %r (fail-closed)"
            % (args.char_name, name))
    if not args.account:
        raise ScenarioError("newchr needs --account (payload starts with the login account)")
    body = newchr_payload(args.account, name, plan["hair"], plan["job"], plan["sex"])
    after = conn.recorder.seq
    _chain_send(run, conn, 0, CM_NEWCHR, body, "CM_NEWCHR")
    got = conn.wait_for([SM_NEWCHR_SUCCESS, SM_NEWCHR_FAIL, SM_OUTOFCONNECTION],
                        args.timeout_ms, after)
    frames = [(seq, fr) for (seq, fr) in conn.frames if seq > after]
    base = {"index": plan["index"], "kind": "newchr", "phase": "selgate",
            "name": name, "job": plan["job"], "hair": plan["hair"], "sex": plan["sex"],
            "note": plan.get("note", "")}
    if got is None:
        base["result"] = "no-reply"
        run.chain["newchr"] = base
        run.steps.append(base)
        raise StepFailure("newchr(name=%r): no SM_NEWCHR_SUCCESS/FAIL within %dms; "
                          "frames since newchr=%r"
                          % (name, args.timeout_ms,
                             [(s_, f_.ident, f_.recog) for (s_, f_) in frames]))
    seq, fr = got
    if fr.ident == SM_OUTOFCONNECTION:
        base["result"] = "disconnected"
        run.chain["newchr"] = base
        run.steps.append(base)
        raise StepFailure("newchr(name=%r): server returned SM_OUTOFCONNECTION(528)" % name)
    if fr.ident == SM_NEWCHR_SUCCESS:
        base["result"] = "created"
        run.chain["newchr"] = base
        run.steps.append(base)
        return name
    n_code = fr.recog
    base["n_code"] = n_code
    if newchr_should_reuse(plan["on_exists"], n_code, names, name):
        base["result"] = "reused"
        run.chain["newchr"] = base
        run.steps.append(base)
        return name
    base["result"] = "failed"
    run.chain["newchr"] = base
    run.steps.append(base)
    raise StepFailure("steps[%s].newchr(name=%r, on_exists=%r): %s"
                      % (plan["index"], name, plan["on_exists"],
                         newchr_fail_message(n_code, frames)))


def do_select_char(run, args, selgate_addr, session, plans=None):
    """SelGate 查询/选择角色，返回 (selgate_conn, char_name, gamegate_addr)。

    `plans` 里的 `newchr` 步在**这里**执行（进世界之前）；主步骤循环会跳过它。
    """
    conn = Conn("selgate", run.recorder)
    run.conns.append(conn)
    conn.phase = "selgate"
    sg_ip, sg_port = selgate_addr
    try:
        conn.connect(sg_ip, sg_port, args.timeout_ms / 1000.0)
    except OSError as ex:
        raise StepFailure("selgate %s:%d unreachable: %r" % (sg_ip, sg_port, ex))
    run.chain["selgate"] = conn.peer
    chars = _query_chr(run, conn, args, session)
    run.chain["char_list"] = chars
    names = [c["name"] for c in chars]
    nc_plan = next((p for p in (plans or []) if p["kind"] == "newchr"), None)
    if nc_plan is not None:
        char_name = _do_newchr(run, conn, args, session, nc_plan, names)
        # oracle `SM_NEWCHR_SUCCESS` ⇒ Bot 重新 QueryChr（新角色进列表）
        chars = _query_chr(run, conn, args, session)
        run.chain["char_list_after_newchr"] = chars
        names = [c["name"] for c in chars]
        if not any(n.lower() == char_name.lower() for n in names):
            raise StepFailure(
                "newchr(name=%r) 成功但刷新后的 SM_QUERYCHR 列表里没有该角色：%s（fail-closed）"
                % (char_name, names))
        if char_name not in names:
            char_name = next(n for n in names if n.lower() == char_name.lower())
    elif args.char_name:
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
    # 首包（W-LF-1）：dwell 之后再落 chain（`SM_NEWMAP/SM_MAPDESCRIPTION` 多在延迟段到达）。
    fp = dict(run.state.get("first_pack") or {})
    if fp:
        fp["map_name"] = None
        fp["map_name_note"] = ("客户端线上不含逻辑地图名：`map_file`=SM_NEWMAP(51) 的 MapFileName、"
                               "`map_desc`=SM_MAPDESCRIPTION 的 MapDesc；逻辑 MapName/出生点以 DB "
                               "`characters.MapName` 与 `!setup.txt` StartPoint 为准（W-LF-1）")
        run.chain["first_pack"] = fp
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
                    help="只加载+展开场景做校验（不连网）：rc=0 通过 / rc=2 场景错误。"
                         "顶层 `blocked`（schema {\"reason\": <非空 str>, "
                         "\"missing_fixture\": <str，可选>}）是**合法的形状**：合法 blocked 场景"
                         "仍 rc=0（只是不可跑，card_window 见到会 rc=2）；`blocked` 结构非法"
                         "（缺 reason / 类型错 / 未知键）⇒ rc=2 并点名 scenario.blocked.*")
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
        sg_conn, char_name, gg_addr = do_select_char(run, args, selgate_addr, session, plans)
        gg = do_enter_world(run, args, gg_addr, char_name, session)

        # BotSrv 在切场景后（延时 1000ms）关掉 Login/SelGate 连接；这里在进世界后关，
        # 顺序一致（GameGate 登录先完成），且不引入额外时序噪声。
        for c in (login_conn, sg_conn):
            c.close()
            run.chain.setdefault("closed", []).append(c.label)

        for i, plan in enumerate(plans):
            if plan["kind"] == "newchr":
                continue  # 已在选角相位执行
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


def _selftest_scenario_requires():
    """`scenario.requires` 形状校验（卡 `openmir2-scenario-fixture-precondition`）：

    * `["magic"]` 放行（夹具段由 `card_window.py` 按生效 profile 核，本驱动器不读）；
    * 未知值（含大小写/未来值）⇒ ScenarioError 并点名，不许静默不生效；
    * 非数组 / 空数组 / 非字符串元素 / 重复 ⇒ ScenarioError。
    """
    import tempfile
    good = {
        "name": "selftest_requires",
        "requires": ["magic"],
        "steps": [{"send": {"ident": CM_SAY, "text": "@时间"}},
                  {"wait_for": SM_SYSMESSAGE}],
    }
    fd, path = tempfile.mkstemp(suffix=".json")
    os.close(fd)
    try:
        def _load(doc):
            with open(path, "w", encoding="utf-8") as fh:
                json.dump(doc, fh, ensure_ascii=False)
            return load_scenario(path)

        assert _load(good).get("requires") == ["magic"]
        bad_cases = [
            ({"requires": "magic"}, "non-empty array"),
            ({"requires": []}, "non-empty array"),
            ({"requires": [1]}, "non-empty array"),
            ({"requires": ["magic", "magic"]}, "duplicates"),
            ({"requires": ["inventory"]}, "unknown fixture"),
            ({"requires": ["magic", "Magic"]}, "unknown fixture"),
        ]
        for patch, needle in bad_cases:
            err = None
            try:
                _load(dict(good, **patch))
            except ScenarioError as ex:
                err = str(ex)
            assert err is not None, "load_scenario accepted bad requires: %r" % (patch,)
            assert needle in err, (needle, err)
        return "requires 形状 6 项"
    finally:
        try:
            os.unlink(path)
        except OSError:
            pass


def _selftest_scenario_inventory():
    """`scenario.inventory` 形状校验（卡 `openmir2-inventory-fixture-reachability`）：
    合法声明放行；坏声明（缺键/未知键/槽位越界/重复槽位/重复 MakeIndex/非数组）⇒ ScenarioError。
    """
    import tempfile
    good = {
        "name": "selftest_inventory",
        "inventory": [{"index": 207, "make_index": 5001, "position": 40, "std_mode": 5},
                      {"index": 3, "make_index": 5002, "position": 41, "dura": 1}],
        "steps": [{"send": {"ident": CM_SAY, "text": "@时间"}},
                  {"wait_for": SM_SYSMESSAGE}],
    }
    fd, path = tempfile.mkstemp(suffix=".json")
    os.close(fd)
    try:
        def _load(doc):
            with open(path, "w", encoding="utf-8") as fh:
                json.dump(doc, fh, ensure_ascii=False)
            return load_scenario(path)

        assert len(_load(good)["inventory"]) == 2
        bad_cases = [
            ({"inventory": []}, "non-empty array"),
            ({"inventory": {"index": 1}}, "non-empty array"),
            ({"inventory": [{"index": 1, "position": 1}]}, "missing required"),
            ({"inventory": [{"index": 1, "make_index": 1, "position": 1, "zzz": 2}]},
             "unknown key"),
            ({"inventory": [{"index": 1, "make_index": 1, "position": 46}]}, "out of range"),
            ({"inventory": [{"index": 1, "make_index": 1, "position": 1},
                            {"index": 2, "make_index": 2, "position": 1}]}, "duplicated"),
            ({"inventory": [{"index": 1, "make_index": 0, "position": 1}]}, "make_index"),
            ({"inventory": [{"index": 1, "make_index": 5, "position": 1},
                            {"index": 2, "make_index": 5, "position": 2}]}, "duplicated"),
            ({"inventory": [{"index": 1, "make_index": 1, "position": 1,
                             "note": 123}]}, "note"),
        ]
        for body, needle in bad_cases:
            err = None
            try:
                _load(dict(good, **body))
            except ScenarioError as ex:
                err = str(ex)
            assert err is not None, "load_scenario accepted bad inventory: %r" % (body,)
            assert needle in err, (needle, err)
        return "inventory 形状 9 项（含 note 类型）"
    finally:
        try:
            os.unlink(path)
        except OSError:
            pass


# ---------------------------------------------------------------------------
# 资产声明（assets）的**形状**校验。通道真身（读声明 -> UPDATE 测试库 -> 指纹/读回
# 断言）在 `tools/conformance/card_window.py`（卡 `openmir2-window-gold-declaration-
# channel`）；本驱动器不写库，只做**同口径形状校验**（卡 `openmir2-probe-assets-key-
# delta`，Gold 通道卡 review f7a4acc9bf 登记的 follow-up），使带 assets 声明的场景
# 能被 `--check-scenario` / 真跑加载（否则顶层 unknown key 直接 rc=2，通道端到端不可用）。
# **同口径 = 同形输入同判**：白名单/值域/note 的规则两处必须同步改，下面三个常量与
# card_window.py 的 ASSETS_COLUMNS/ASSETS_MAX/ASSETS_NOTE_KEY 逐一对应。
# ---------------------------------------------------------------------------
#: 资产声明白名单列（事实源 = `tools/conformance/card_window.py` 的 `ASSETS_COLUMNS`；
#: 2026-10-25 起含 `GamePoint`）。运行时显式同步，`--selftest` 的可执行同源校验见
#: `_card_window_assets_columns()`（漂移即红）。
ASSETS_COLUMNS = ("Gold", "GamePoint")
#: 声明值上限：characters.Gold/GamePoint 都是 int(11) 有符号（= card_window.py ASSETS_MAX）。
ASSETS_MAX = 0x7FFFFFFF
#: 声明里的备注键（不参与 UPDATE，只进证据；= card_window.py ASSETS_NOTE_KEY）。
ASSETS_NOTE_KEY = "note"


def _check_assets(assets):
    """`scenario.assets` 的**形状**校验：合法 ⇒ 放行（rc=0），非法 ⇒ ScenarioError（rc=2）。

    与 `tools/conformance/card_window.py` 的 `scenario_assets()` **同口径**（fail-closed，
    与 `placement`/`blocked`/`requires`/`inventory` 一致）：

    * `assets` 缺省（无此键 / null）⇒ 放行；给了就必须是**非空对象**（列→目标值）；
    * 键只允许白名单列（`Gold` / `GamePoint`，与 card_window 单一来源同源）+ `note`，
      未知列点名拒绝（拼错/未声明列不许静默）；
    * `note` 给了就必须是字符串；
    * 值必须是 JSON 整数（bool/浮点/字符串/null 一律拒）、0..0x7FFFFFFF
      （characters.Gold 是 int(11) 有符号；负数/超 int32 一律拒）；
    * 只有 `note`、没声明任何白名单列 ⇒ 拒（无意义声明）。
    """
    if assets is None:
        return
    if not isinstance(assets, dict) or not assets:
        raise ScenarioError("scenario.assets must be a non-empty object (column -> value), "
                            "got %r" % (assets,))
    bad = set(assets) - set(ASSETS_COLUMNS) - {ASSETS_NOTE_KEY}
    if bad:
        raise ScenarioError("scenario.assets has non-whitelisted column(s) %s "
                            "(fail-closed; whitelist=%s + note key %r; same rule as "
                            "tools/conformance/card_window.py scenario_assets)"
                            % (sorted(bad), list(ASSETS_COLUMNS), ASSETS_NOTE_KEY))
    note = assets.get(ASSETS_NOTE_KEY)
    if note is not None and not isinstance(note, str):
        raise ScenarioError("scenario.assets.note must be a string, got %r" % (note,))
    declared = 0
    for col in ASSETS_COLUMNS:
        if col not in assets:
            continue
        v = assets[col]
        if not isinstance(v, int) or isinstance(v, bool):
            raise ScenarioError("scenario.assets.%s must be a JSON int (got %r: %s; "
                                "string/float/bool/null are all rejected)"
                                % (col, v, type(v).__name__))
        if v < 0 or v > ASSETS_MAX:
            raise ScenarioError("scenario.assets.%s=%r out of range [0,%d] "
                                "(characters.%s is int(11) signed; negative / >int32 "
                                "are all rejected)" % (col, v, ASSETS_MAX, col))
        declared += 1
    if not declared:
        raise ScenarioError("scenario.assets has only %r and no declared column(s) %s "
                            "(meaningless declaration, rejected)"
                            % (ASSETS_NOTE_KEY, list(ASSETS_COLUMNS)))
    print("note: scenario.assets 声明了 %s，但**本驱动器不写库**"
          "（由 tools/conformance/card_window.py 在 restore 后、起栈前应用）"
          % sorted(k for k in assets if k != ASSETS_NOTE_KEY), file=sys.stderr)


def _card_window_assets_columns():
    """懒加载 `tools/conformance/card_window.py` 的资产白名单常量（**只在 selftest** 调用）。

    探针运行时不 import 它（脚本文件、无包结构）；此函数把「单一来源」变成**可执行**的
    一致性校验：三常量任一漂移，`--selftest` 的 `scenario-assets` 即红。
    """
    import importlib.util
    from pathlib import Path

    path = Path(__file__).resolve().parent.parent / "conformance" / "card_window.py"
    spec = importlib.util.spec_from_file_location("_card_window_assets_sync", str(path))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return (tuple(mod.ASSETS_COLUMNS), mod.ASSETS_MAX, mod.ASSETS_NOTE_KEY)


def _selftest_scenario_assets():
    """`scenario.assets` 形状校验（卡 `openmir2-probe-assets-key-delta` + 白名单对齐卡
    `openmir2-probe-assets-whitelist`）：

    * **同源校验**：本文件的 `ASSETS_COLUMNS/ASSETS_MAX/ASSETS_NOTE_KEY` 必须与
      `tools/conformance/card_window.py` 逐项相同（漂移即红）；
    * 正例：`{"Gold": 10000}`、`{"GamePoint": 9}`（GAMEPOINT 窗口前置）与两者同声明放行；
      边界 0 与 0x7FFFFFFF 放行；缺省放行；
    * 负例：非白名单列（含 `Foo`）/ 空对象 / note-only ⇒ 拒并点名（fail-closed）；
    * 值口径：bool/浮点/字符串/null/负数/超 int32 一律拒（与 card_window.scenario_assets
      同口径——同形输入同判）。
    """
    import tempfile
    good = {
        "name": "selftest_assets",
        "assets": {"Gold": 10000, "note": "upgrade_retrieve 前置 Gold>=10000"},
        "steps": [{"send": {"ident": CM_SAY, "text": "@时间"}},
                  {"wait_for": SM_SYSMESSAGE}],
    }
    fd, path = tempfile.mkstemp(suffix=".json")
    os.close(fd)
    try:
        def _load(doc):
            with open(path, "w", encoding="utf-8") as fh:
                json.dump(doc, fh, ensure_ascii=False)
            return load_scenario(path)

        # 同源校验（单一来源 = card_window.py）。
        cw_cols, cw_max, cw_note = _card_window_assets_columns()
        assert tuple(ASSETS_COLUMNS) == cw_cols, (
            "ASSETS_COLUMNS drift: probe=%r card_window=%r" % (ASSETS_COLUMNS, cw_cols))
        assert ASSETS_MAX == cw_max, "ASSETS_MAX drift: %r vs %r" % (ASSETS_MAX, cw_max)
        assert ASSETS_NOTE_KEY == cw_note, "ASSETS_NOTE_KEY drift"
        assert "GamePoint" in cw_cols, "card_window 白名单必须含 GamePoint（P0 卡）"

        assert _load(good)["assets"]["Gold"] == 10000
        assert _load(dict(good, assets={"Gold": 0}))["assets"]["Gold"] == 0
        assert _load(dict(good, assets={"Gold": 0x7FFFFFFF}))["assets"]["Gold"] == 0x7FFFFFFF
        # 正例②：GamePoint（`真爱-G002 @9hua` 的 `CHECKGAMEPOINT > 8` 前置）。
        assert _load(dict(good, assets={"GamePoint": 9}))["assets"]["GamePoint"] == 9
        assert _load(dict(good, assets={"Gold": 1, "GamePoint": 9}))["assets"] == {
            "Gold": 1, "GamePoint": 9}
        assert _load(dict(good, assets={"GamePoint": 0}))["assets"]["GamePoint"] == 0
        assert "assets" not in _load({k: v for k, v in good.items() if k != "assets"})
        bad_cases = [
            # 负例①（本卡判据）：未知列 `Foo` 必须 fail-closed 点名。
            ({"assets": {"Foo": 1}}, "non-whitelisted"),
            ({"assets": {"GamePoint": "9"}}, "must be a JSON int"),
            ({"assets": {"GamePoint": -1}}, "out of range"),
            ({"assets": {"GamePoint": True}}, "must be a JSON int"),
            ({"assets": {}}, "non-empty object"),
            ({"assets": []}, "non-empty object"),
            ({"assets": {"Credit": 100}}, "non-whitelisted"),
            ({"assets": {"Gold": 1, "Level": 2}}, "non-whitelisted"),
            ({"assets": {"note": "only note"}}, "no declared column"),
            ({"assets": {"Gold": "10000"}}, "must be a JSON int"),
            ({"assets": {"Gold": 1.5}}, "must be a JSON int"),
            ({"assets": {"Gold": True}}, "must be a JSON int"),
            ({"assets": {"Gold": None}}, "must be a JSON int"),
            ({"assets": {"Gold": -1}}, "out of range"),
            ({"assets": {"Gold": 0x80000000}}, "out of range"),
            ({"assets": {"Gold": 1, "note": 123}}, "note"),
        ]
        for patch, needle in bad_cases:
            err = None
            try:
                _load(dict(good, **patch))
            except ScenarioError as ex:
                err = str(ex)
            assert err is not None, "load_scenario accepted bad assets: %r" % (patch,)
            assert needle in err, (needle, err)
        return ("assets 形状 %d 项（7 正 + %d 负；含 GamePoint 正例、Foo 未知列、"
                "bool/浮点/字符串/null/负数/超 int32 拒绝；三常量与 card_window 同源）"
                % (7, len(bad_cases)))
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
    # ②c **R3-direction**：方向写反必须红（SM 号当 send / CM 号当 wait）。
    #     这两条也是「禁用方向校验」这个变异的牙：只留旧并集口径时，它们会被接受 ⇒ 本 check 红。
    expect_red(lambda d: d["steps"][0]["send"].__setitem__("ident", 4),
               "send ident 4 is SM_41, expected CM_*")
    expect_red(lambda d: d["steps"][1].__setitem__("wait_for", 80),
               "wait_for ident 80 is CM_QUERYUSERNAME, expected SM_*")
    # ②d 并集歧义（同号在 CM 与 SM 都有名字）两个方向都放行：100 = CM_QUERYCHR = SM_SYSMESSAGE。
    #     没有这一条的话，下次有人「顺手」把歧义号判死会把合法场景误伤。
    amb_wait = clone(base)
    amb_wait["steps"][1]["wait_for"] = 100
    check_scenario(amb_wait, expand_scenario(clone(amb_wait), dict(ok_vars))[0],
                   ok_vars, messages)
    amb_send = clone(base)
    amb_send["steps"][0]["send"]["ident"] = 100
    check_scenario(amb_send, expand_scenario(clone(amb_send), dict(ok_vars))[0],
                   ok_vars, messages)
    # ②e say / click_npc / dlg_select 这三种 C2S 动作同样走方向校验（正向：合法常量必须绿）
    say_case = {"name": "selftest_say", "steps": [{"say": "@时间"}, {"wait_for": 100}]}
    check_scenario(say_case, expand_scenario(clone(say_case))[0], None, messages)
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

    # M3：R6 取表 fail-closed 的五种 drift 都要在 selftest 里被钉住
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

    # BLOCKED：顶层 `blocked` 是**合法的形状**（rc=0，只是不可跑）；结构非法才 rc=2。
    # 牙：① 合法 blocked 必须放行（否则所有封存场景都会被 --check-scenario 判死）；
    #     ② 缺 reason / reason 非字符串 / 未知键 / 空串 必须点名 scenario.blocked.*
    import shutil
    bl_dir = tempfile.mkdtemp(prefix="pd-blocked-")

    def _load_doc(name, doc):
        path = os.path.join(bl_dir, name + ".json")
        with open(path, "w", encoding="utf-8") as fh:
            json.dump(doc, fh, ensure_ascii=False)
        return load_scenario(path)

    try:
        good_blocked = clone(base)
        good_blocked["blocked"] = {"reason": "夹具不存在：AC 30..59 无解",
                                   "missing_fixture": "combat_strike 夹具矩阵"}
        gb = _load_doc("good", good_blocked)
        check_scenario(gb, expand_scenario(gb, dict(ok_vars))[0], ok_vars, messages)
        only_reason = clone(base)
        only_reason["blocked"] = {"reason": "只写理由，不写 missing_fixture"}
        orr = _load_doc("only-reason", only_reason)   # missing_fixture 可选
        check_scenario(orr, expand_scenario(orr, dict(ok_vars))[0], ok_vars, messages)

        for i, (label, blocked, needle) in enumerate((
            ("缺 reason", {"missing_fixture": "x"}, "scenario.blocked missing key(s): ['reason']"),
            ("reason 非字符串", {"reason": 123},
             "scenario.blocked.reason must be a non-empty string"),
            ("reason 空串", {"reason": "   "}, "scenario.blocked.reason must be a non-empty string"),
            ("blocked 非对象", "yes", "scenario.blocked must be an object"),
            ("未知键", {"reason": "x", "note": "y"}, "scenario.blocked has unknown key(s)"),
            ("missing_fixture 空串", {"reason": "x", "missing_fixture": ""},
             "scenario.blocked.missing_fixture must be a non-empty string"),
        )):
            d = clone(base)
            d["blocked"] = blocked
            try:
                _load_doc("bad%d" % i, d)
            except ScenarioError as ex:
                assert needle in str(ex), (label, needle, str(ex))
            else:
                raise AssertionError("bad blocked accepted (%s) — fail-closed broken" % label)
    finally:
        shutil.rmtree(bl_dir, ignore_errors=True)

    # 防误伤：login_only（纯累计观察）与 dice_chain（params 默认值）必须绿
    for name in ("login_only.json", "dice_chain.json"):
        d = load_scenario(os.path.join(SCENARIO_DIR, name))
        check_scenario(d, expand_scenario(d)[0], None, messages)
    return "R1-R5 硬牙（含 R3 方向：send=CM_*/wait=SM_*）+ R6 tag WARN + 取表 drift 五例 + blocked 形状（6 坏例）+ 不误伤"


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


def _selftest_wait_recog_filter():
    """合成帧流（不连网）：同 ident、来源 recog=自己 vs 他人 ⇒ 过滤只认匹配的来源。

    覆盖：① 旧语义（不带过滤）两条都算命中（兼容性牙）；② ``recog_is=self/other``；
    ③ ``recog`` 精确值（含 params 名加载期解析）；④ wait_idents/wait_frames 的过滤；
    ⑤ 互斥/非法 recog_is/未知键在**加载期** rc=2；⑥ 运行期未知 self_actor 时 fail-closed。
    """
    import shutil
    import tempfile
    tmp = tempfile.mkdtemp(prefix="mir2probe-recog-")
    try:
        self_actor, other_actor = 6930, 119
        frames = [
            build_s2c_frame(self_actor, SM_STRUCK, 1, 2, 0, b"\x00\x00"),
            build_s2c_frame(other_actor, SM_STRUCK, 3, 4, 0, b"\x00\x00"),
            build_s2c_frame(other_actor, SM_HIT, 0, 0, 0, b""),
        ]
        args = argparse.Namespace(out=tmp, no_auto_handshake=True, timeout_ms=8000)
        run = Run(args)
        # do_enter_world 从 SM_LOGON(50).recog 学到自身 ActorId（这里直接喂状态）
        run.state["self_actor"] = self_actor
        conn = Conn("gamegate", run.recorder, on_frame=run.on_frame)
        conn.phase = "gamegate"
        conn.sock = _FakeSock(frames)
        conn.pump(100)
        assert [f.ident for _, f in conn.frames] == [SM_STRUCK, SM_STRUCK, SM_HIT], conn.frames
        seq_self = conn.frames[0][0]
        seq_other = conn.frames[1][0]

        # ① 旧语义：不带过滤 ⇒ 同 ident 的两条都算命中（先到先得）——兼容性判据。
        got = conn.wait_for([SM_STRUCK], 30, 0)
        assert got is not None and got[0] == seq_self, got
        got = conn.wait_for([SM_STRUCK], 30, seq_self)
        assert got is not None and got[0] == seq_other, got

        # ② recog_is=other / self：只认来源匹配的帧（同 ident 两条的区分点）。
        got = conn.wait_for([SM_STRUCK], 30, 0, lambda fr: fr.recog != self_actor)
        assert got is not None and got[0] == seq_other, got
        got = conn.wait_for([SM_STRUCK], 30, 0, lambda fr: fr.recog == self_actor)
        assert got is not None and got[0] == seq_self, got
        # 精确 recog=OTHER 同口径
        got = conn.wait_for([SM_STRUCK], 30, 0, lambda fr: fr.recog == other_actor)
        assert got is not None and got[0] == seq_other, got

        # ③ 加载期编译：dict 形状 / params 名 / 互斥与非法值 fail-closed。
        plan = plan_step({"wait_for": {"idents": SM_STRUCK, "recog_is": "other"}}, 0, {})
        assert plan["idents"] == [SM_STRUCK] and plan["recog_is"] == "other", plan
        assert "recog" not in plan, plan
        plan = plan_step({"wait_for": {"idents": [SM_STRUCK], "recog": "want"}}, 0,
                         {"want": other_actor})
        assert plan["recog"] == other_actor and "recog_is" not in plan, plan
        plan = plan_step({"wait_idents": {"idents": [SM_STRUCK], "recog_is": "self"}}, 0, {})
        assert plan["recog_is"] == "self", plan
        plan = plan_step({"wait_frames": {"count": 3, "recog": other_actor}}, 0, {})
        assert plan["count"] == 3 and plan["recog"] == other_actor, plan
        for bad in ({"idents": SM_STRUCK, "recog": other_actor, "recog_is": "self"},
                    {"idents": SM_STRUCK, "recog_is": "me"},
                    {"idents": SM_STRUCK, "bogus": 1},
                    {"recog_is": "self"}):
            try:
                plan_step({"wait_for": bad}, 0, {})
            except ScenarioError:
                continue
            raise AssertionError("bad wait_for accepted: %r" % (bad,))

        # ④ wait_idents / wait_frames 的过滤只作用于 ident 覆盖判定（count 仍是总帧数）。
        missing, _ = conn.wait_idents([SM_STRUCK], 20, lambda fr: fr.recog == other_actor)
        assert missing == [], missing
        missing, _ = conn.wait_idents([SM_HIT], 20, lambda fr: fr.recog == self_actor)
        assert missing == [SM_HIT], missing
        assert conn.wait_frames(3, [SM_STRUCK], 20, lambda fr: fr.recog == other_actor) is True
        assert conn.wait_frames(3, [SM_HIT], 20, lambda fr: fr.recog == self_actor) is False

        # ⑤ 运行期还不知道自身 ActorId ⇒ recog_is 必须 fail-closed（不许猜）。
        run2 = Run(args)
        conn2 = Conn("gamegate", run2.recorder, on_frame=run2.on_frame)
        conn2.phase = "gamegate"
        plan = plan_step({"wait_for": {"idents": SM_STRUCK, "recog_is": "other"}}, 0, {})
        try:
            run2.execute_step(conn2, plan)
        except ScenarioError as ex:
            assert "self actor" in str(ex), str(ex)
        else:
            raise AssertionError("recog_is with unknown self actor was accepted (fail-closed broken)")

        # ⑥ 完整 execute_step：过滤后 matched_recog 必须是「他人」那帧；
        #    同一串帧**不带过滤**必须命中第一条（自己那帧），且旧 run.json 字段不多不少。
        run3 = Run(args)
        run3.state["self_actor"] = self_actor
        conn3 = Conn("gamegate", run3.recorder, on_frame=run3.on_frame)
        conn3.phase = "gamegate"
        conn3.sock = _FakeSock(frames)
        plan = plan_step({"wait_for": {"idents": SM_STRUCK, "recog_is": "other"},
                          "timeout_ms": 200}, 0, {})
        run3.execute_step(conn3, plan)
        step = run3.steps[-1]
        assert step["matched"] and step["matched_recog"] == other_actor, step
        assert step["recog"] is None and step["recog_is"] == "other", step

        run4 = Run(args)
        conn4 = Conn("gamegate", run4.recorder, on_frame=run4.on_frame)
        conn4.phase = "gamegate"
        conn4.sock = _FakeSock(frames)
        plan = plan_step({"wait_for": SM_STRUCK, "timeout_ms": 200}, 0, {})
        run4.execute_step(conn4, plan)
        step4 = run4.steps[-1]
        assert step4["matched_recog"] == self_actor, step4
        assert "recog_is" not in step4 and "recog" not in step4, step4
        return "合成帧 3 条：self/other 过滤各认其源；无过滤两者都命中"
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


#: oracle `DBSrv UserService.NewChr` 的 nCode 语义（`:566-666`；`SM_NEWCHR_FAIL.Recog`）。
NEWCHR_FAIL_CODES = {
    0: "角色名含非法字符/长度<3/多余字段（NewChr 解析门）",
    2: "角色名已被使用（Index>=0 或 CheckDenyChrName/Add 失败）",
    3: "该账号已有两个角色（ChrCountOfAccount >= 2）",
    4: "创建角色数据失败（NewChrData 落库失败，已回滚索引）",
}


def newchr_payload(account, name, hair, job, sex):
    """`CM_NEWCHR` 载荷：`account/name/hair/job/sex`。

    oracle 同序：`BotSrv/.../SelectChrScene.cs:237-241` `SendNewChr(uid,uname,shair,sjob,ssex)` 拼
    `uid + "/" + uname + "/" + shair + "/" + sjob + "/" + ssex`；DB `NewChr` 按同序
    `GetValidStr3(…, Backslash)` 收 5 段（`UserService.cs:572-576`）。**hair 在 job 之前**。
    """
    return ("%s/%s/%d/%d/%d" % (account, name, hair, job, sex)).encode("gbk")


def newchr_fail_message(n_code, frames):
    """`SM_NEWCHR_FAIL` 的显式失败串：nCode 语义 + 服务端返回帧清单（不静默继续）。"""
    meaning = NEWCHR_FAIL_CODES.get(n_code, "未知错误码")
    fl = []
    for (seq, fr) in frames:
        text = decode_text(fr.body_decoded, "gbk", "replace")
        fl.append("seq=%s ident=%s recog=%s body=%r" % (seq, fr.ident, fr.recog, text))
    return ("newchr: server returned SM_NEWCHR_FAIL(%d) Recog(nCode)=%r ⇒ %s; "
            "server frames since newchr: %s"
            % (SM_NEWCHR_FAIL, n_code, meaning, fl or "<none>"))


def newchr_should_reuse(on_exists, n_code, names, target):
    """重名时是否复用：策略=reuse 且服务端 nCode=2 且目标名已在角色列表里。"""
    return (on_exists == "reuse" and n_code == 2
            and any(n.lower() == target.lower() for n in names))


def _selftest_dlg_select_any():
    """`dlg_select_any`（卡 `openmir2-probe-conditional-dlg-select`）：

    * 纯函数：`parse_page_links`（`<文字/@label>` 抽取，出现序去重）与 `choose_link`
      （候选顺序优先、大小写不敏感、全不中 ⇒ None）；
    * 形状：候选列表必须非空/都是 `@label`/不重复；`repeat` 的 times 1..100、steps 非空、
      `stop_on_select` 字符串、嵌套 ≤3；
    * 语义：合成场景（repeat 内 send/select/wait）`check_scenario` 通过；wait 在 send 之前 ⇒ 拒；
    * 叶子展开：`_iter_plan_leaves` 把 repeat 内层动作纳入方向/顺序校验。
    """
    # ① 页面链接抽取与择一
    page_text = ("高级武士：“我出的是<石头> 你赢了！”\\ \\ "
                 "<打听[命运之刃]的下落/@rw02>\\ \\ <放弃任务/@exit>")
    links = parse_page_links(page_text)
    assert links == ["@rw02", "@exit"], links
    assert parse_page_links("<再来/@rw01><石头/@st>") == ["@rw01", "@st"]
    # 先钉「候选顺序压过链接顺序」（变异：按链接顺序 ⇒ 本行先红，便于归因）
    assert choose_link(["@rw02", "@rw01"], ["@rw01", "@rw02"]) == "@rw02",         "候选顺序压过链接顺序（反序链接下仍选 @rw02）"
    assert choose_link(["@rw02", "@rw01"], links) == "@rw02"
    assert choose_link(["@rw03", "@rw01"], links) is None
    assert choose_link(["@RW02"], links) == "@RW02", "大小写不敏感"
    assert choose_link(["@rw01", "@rw02"], links) == "@rw02", "按候选顺序，不是链接顺序"

    # ② 形状：合法
    def plan1(step):
        return plan_step(step, 0, {})

    p = plan1({"dlg_select_any": ["@rw02", "@rw01"]})
    assert p["kind"] == "dlg_select_any" and p["candidates"] == ["@rw02", "@rw01"]
    assert p["ident"] == CM_MERCHANTDLGSELECT and p["_needs_merchant_recog"] is True
    rep = plan1({"repeat": {"times": 2, "stop_on_select": "@rw02",
                            "steps": [{"dlg_select": "@st"},
                                      {"wait_for": 643},
                                      {"dlg_select_any": ["@rw02", "@rw01"]},
                                      {"wait_for": 643}]}})
    assert rep["kind"] == "repeat" and rep["times"] == 2
    assert [x["kind"] for x in rep["steps"]] == ["dlg_select", "wait_for",
                                                 "dlg_select_any", "wait_for"]
    assert [x["index"] for x in rep["steps"]] == ["0.0", "0.1", "0.2", "0.3"]
    assert [x["kind"] for x in _iter_plan_leaves([rep])] == [
        "dlg_select", "wait_for", "dlg_select_any", "wait_for"]

    # ③ 形状：非法（fail-closed）
    bad = [
        ({"dlg_select_any": []}, "non-empty list"),
        ({"dlg_select_any": "x"}, "non-empty list"),
        ({"dlg_select_any": ["x"]}, "starting with '@'"),
        ({"dlg_select_any": ["@a", "@A"]}, "duplicate"),
        ({"dlg_select_any": [1]}, "starting with '@'"),
        ({"repeat": {"times": 0, "steps": [{"dlg_select": "@st"}]}}, "out of range"),
        ({"repeat": {"times": 101, "steps": [{"dlg_select": "@st"}]}}, "out of range"),
        ({"repeat": {"times": 1, "steps": []}}, "non-empty list"),
        ({"repeat": {"times": 1, "steps": [{"dlg_select": "@st"}], "stop_on_select": 1}},
         "non-empty string"),
        ({"repeat": {"times": 1, "steps": [{"dlg_select": "@st"}], "bogus": 1}},
         "unknown key"),
        ({"repeat": {"times": 1, "steps": [{"dlg_select": "@st"}]}, "timeout_ms": 100},
         "timeout_ms is not supported"),
        ({"repeat": {"times": 1, "stop_on_select": "@typo",
                     "steps": [{"dlg_select_any": ["@rw02"]}]}}, "not one of the inner"),
    ]
    for step, needle in bad:
        err = None
        try:
            plan1(step)
        except ScenarioError as ex:
            err = str(ex)
        assert err is not None, "plan_step accepted bad step: %r" % (step,)
        assert needle in err, (needle, err)

    # 嵌套深度 >3
    deep = {"dlg_select": "@st"}
    for _ in range(4):
        deep = {"repeat": {"times": 1, "steps": [deep]}}
    err = None
    try:
        plan1(deep)
    except ScenarioError as ex:
        err = str(ex)
    assert err is not None and "depth" in err, err

    # ④ 语义：合成场景通过；repeat 内「wait 先于 send」被 R2 拒
    messages = load_message_constants()
    good_doc = {
        "name": "selftest_dlg_any_semantics",
        "steps": [
            {"click_npc": "auto:last_turn"},
            {"wait_for": 643},
            {"repeat": {"times": 2, "stop_on_select": "@rw02", "steps": [
                {"dlg_select_any": ["@rw02", "@rw01"]},
                {"wait_for": 643},
            ]}},
            {"wait_for": 643},
        ],
    }
    plans, _ = expand_scenario(good_doc, {"npc_actor": 1})
    check_scenario(good_doc, plans, None, messages)
    bad_doc = {
        "name": "selftest_dlg_any_bad_order",
        "steps": [
            {"repeat": {"times": 1, "steps": [{"wait_for": 643}]}},
            {"click_npc": "auto:last_turn"},
        ],
    }
    plans_bad, _ = expand_scenario(bad_doc, {"npc_actor": 1})
    err = None
    try:
        check_scenario(bad_doc, plans_bad, None, messages)
    except ScenarioError as ex:
        err = str(ex)
    assert err is not None and "no send step precedes" in err, err
    # ⑤ 运行期（合成帧，不连网）：正例选中 + 负例打印链接清单 + repeat 终止/超限
    import shutil  # noqa: F401
    import tempfile

    def page(ident, text):
        return build_s2c_frame(ident, SM_MERCHANTSAY, 0, 0, 0, text.encode("gbk"))

    def mkrun(chunks):
        tmpd = tempfile.mkdtemp(prefix="mir2probe-dlgany-")
        args = argparse.Namespace(out=tmpd, no_auto_handshake=True, timeout_ms=8000)
        run = Run(args)
        conn = Conn("gamegate", run.recorder, on_frame=run.on_frame)
        conn.phase = "gamegate"
        conn.sock = _FakeSock(chunks)
        return run, conn, tmpd

    win = page(119, "你赢了 <打听[命运之刃]的下落/@rw02> <放弃任务/@exit>")
    tie = page(119, "平局 <再来/@rw01>")
    lose = page(119, "输了 <放弃任务/@exit>")
    p_any = plan_step({"dlg_select_any": ["@rw02", "@rw01"]}, 0, {})
    run, conn, tmpd = mkrun([win])
    try:
        conn.pump(100)
        assert run.state["page"]["links"] == ["@rw02", "@exit"], run.state["page"]
        run.execute_step(conn, p_any)
        assert run.state["last_selected_label"] == "@rw02"
        sent = conn.sock.sent[-1]
        # 线上 body 是 EDCode 编码后的标签（build_s2c_frame/c2s 同口径）⇒ 断言编码片段在场。
        assert encode_text("@rw02") in sent, sent
        assert run.steps[-1]["chosen"] == "@rw02"
    finally:
        shutil.rmtree(tmpd, ignore_errors=True)

    run, conn, tmpd = mkrun([lose])
    try:
        conn.pump(100)
        err = None
        try:
            run.execute_step(conn, p_any)
        except StepFailure as ex:
            err = str(ex)
        assert err is not None, "dlg_select_any accepted a page without any candidate"
        assert "none of" in err and "@exit" in err, err
    finally:
        shutil.rmtree(tmpd, ignore_errors=True)

    # repeat：按「发送什么就回什么页面」的脚本化 socket 驱动（不预 pump，避免一次 pump
    # 把后续页也吃掉——真实服务端也只在收到选择后才发下一页）。
    class _ScriptedSock:
        def __init__(self):
            self.chunks = []
            self.sent = []
            self.script = []

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
            for needle, frame in self.script:
                if encode_text(needle) in data:
                    self.chunks.append(frame)
                    break

        def close(self):
            pass

    def mkrun_scripted(script):
        tmpd = tempfile.mkdtemp(prefix="mir2probe-dlgany-")
        args = argparse.Namespace(out=tmpd, no_auto_handshake=True, timeout_ms=8000)
        run = Run(args)
        conn = Conn("gamegate", run.recorder, on_frame=run.on_frame)
        conn.phase = "gamegate"
        sock = _ScriptedSock()
        sock.script = script
        conn.sock = sock
        run.state["last_merchant_recog"] = 119
        run.state["page"] = {"ident": 643, "seq": 0, "text": "<石头/@st>",
                             "links": parse_page_links("<石头/@st>")}
        return run, conn, sock, tmpd

    rw02_page = page(119, "@rw03 挑战 <挑战命运教主/@rw03>")
    rep_plan = plan_step({"repeat": {"times": 4, "stop_on_select": "@rw02",
                                     "steps": [{"dlg_select": "@st"}, {"wait_for": 643},
                                               {"dlg_select_any": ["@rw02", "@rw01"]},
                                               {"wait_for": 643}]}}, 0, {})
    run, conn, sock, tmpd = mkrun_scripted([("@st", win), ("@rw02", rw02_page)])
    try:
        run.execute_step(conn, rep_plan)
        rec = run.steps[-1]
        assert rec["kind"] == "repeat" and rec["stopped"] is True, rec
        assert len(rec["attempts"]) == 1 and rec["attempts"][0]["selected"] == "@rw02", rec
    finally:
        shutil.rmtree(tmpd, ignore_errors=True)

    # repeat **不得吞子步失败**：@st 后只回输页（links=['@exit']）⇒ 显式失败且只发 1 帧
    run, conn, sock, tmpd = mkrun_scripted([("@st", lose)])
    try:
        err = None
        try:
            run.execute_step(conn, rep_plan)
        except StepFailure as ex:
            err = str(ex)
        assert err is not None and "none of" in err and "@exit" in err, err
        assert len(sock.sent) == 1, (
            "repeat 吞掉了子步失败并空转：sent=%d" % len(sock.sent))
    finally:
        shutil.rmtree(tmpd, ignore_errors=True)

    # repeat 超限：每次 @st 给平局页、每次 @rw01 也给平局页 ⇒ 2 轮后仍非 @rw02 ⇒ fail
    rep_plan2 = plan_step({"repeat": {"times": 2, "stop_on_select": "@rw02",
                                      "steps": [{"dlg_select": "@st"}, {"wait_for": 643},
                                                {"dlg_select_any": ["@rw02", "@rw01"]},
                                                {"wait_for": 643}]}}, 0, {})
    run, conn, sock, tmpd = mkrun_scripted([("@st", tie), ("@rw01", tie)])
    try:
        err = None
        try:
            run.execute_step(conn, rep_plan2)
        except StepFailure as ex:
            err = str(ex)
        assert err is not None and "not reached" in err and "attempt" in err, err
        assert "@rw01" in err, err
    finally:
        shutil.rmtree(tmpd, ignore_errors=True)
    return ("parse/choose 8 项；形状 %d 坏例；repeat 叶子 0.0-0.3；"
            "语义正/反；运行期 选中/清单打印/终止/超限/子步失败不吞 5 例" % len(bad))


def _selftest_newchr():
    """`newchr` 步（卡 `openmir2-probe-newchr-step`）：

    * 形状：name 字面量/params 名、job 0..2、hair 0..255、sex 0..1、`on_exists` 枚举；
      blank/长度 3..20/含 `/` 或 `\\`/未知键 ⇒ ScenarioError；
    * 位置：`newchr` 必须 steps[0] 且至多一次（check_scenario）；
    * 纯逻辑：`newchr_should_reuse` 四象限、`newchr_fail_message` 带 nCode 语义与帧清单；
    * 运行期（假 socket）：成功；重名 + reuse ⇒ reused；重名 + fail ⇒ 显式失败带帧；
      code=3（已有两个角色）不接受 reuse；
    * **S-2 钉**：只有 `dlg_select_any`+wait 的场景必须过 check_scenario（删 SEND_KINDS 即 R1 红）；
    * **B1 钉**：没有 643 时 `dlg_select_any` 必须 ScenarioError（不许 recog=0 猜测）。
    """
    messages = load_message_constants()

    # ① 形状：合法（字面量 + params 名）
    p1 = plan_step({"newchr": {"name": "abc", "job": 1, "hair": 3, "sex": 1}}, 0, {})
    assert p1["kind"] == "newchr" and p1["ident"] == CM_NEWCHR and p1["recog"] == 0
    assert (p1["name"], p1["job"], p1["hair"], p1["sex"], p1["on_exists"]) == \
        ("abc", 1, 3, 1, "reuse")
    p2 = plan_step({"newchr": {"name": "chrname", "job": "job", "hair": "hair", "sex": "sex"}},
                   0, {"chrname": "英雄A", "job": 2, "hair": 5, "sex": 0})
    assert (p2["name"], p2["job"], p2["hair"], p2["sex"]) == ("英雄A", 2, 5, 0)

    # ② 形状：非法（fail-closed）
    bad = [
        ({"newchr": {"job": 0}}, "missing key"),
        ({"newchr": {"name": "   "}}, "not be blank"),
        ({"newchr": {"name": "ab"}}, "length 2 out of range"),
        ({"newchr": {"name": "x" * 21}}, "length 21 out of range"),
        ({"newchr": {"name": "a/b"}}, "must not contain"),
        ({"newchr": {"name": "abc", "job": 3}}, "out of range"),
        ({"newchr": {"name": "abc", "hair": 256}}, "out of range"),
        ({"newchr": {"name": "abc", "sex": 2}}, "out of range"),
        ({"newchr": {"name": "abc", "on_exists": "ignore"}}, "on_exists"),
        ({"newchr": {"name": "abc", "bogus": 1}}, "unknown key"),
    ]
    for step, needle in bad:
        err = None
        try:
            plan_step(step, 0, {})
        except ScenarioError as ex:
            err = str(ex)
        assert err is not None, "plan_step accepted bad newchr: %r" % (step,)
        assert needle in err, (needle, err)

    # ③ 位置：必须 steps[0] 且至多一次
    def check_doc(steps):
        doc = {"name": "selftest_newchr_doc", "required": ["newchr_name"], "steps": steps}
        plans, _ = expand_scenario(doc, {"newchr_name": "abcdef"})
        check_scenario(doc, plans, None, messages)

    ok_steps = [
        {"newchr": {"name": "newchr_name"}},
        {"wait_idents": {"idents": [SM_LOGON], "timeout_ms": 100}},
    ]
    check_doc(ok_steps)
    for bad_steps, needle in [
        ([{"send": {"ident": CM_SAY, "text": "x"}}, {"newchr": {"name": "abcdef"}}],
         "must be steps[0]"),
        ([{"newchr": {"name": "abcdef"}}, {"newchr": {"name": "abcdef2"}},
          {"wait_for": SM_LOGON}], "at most one"),
    ]:
        err = None
        try:
            check_doc(bad_steps)
        except ScenarioError as ex:
            err = str(ex)
        assert err is not None and needle in err, (needle, err)

    # ④ 纯逻辑
    assert newchr_should_reuse("reuse", 2, ["abc", "def"], "ABC") is True
    assert newchr_should_reuse("fail", 2, ["abc"], "abc") is False
    assert newchr_should_reuse("reuse", 3, ["abc"], "abc") is False
    assert newchr_should_reuse("reuse", 2, ["def"], "abc") is False
    msg = newchr_fail_message(2, [])
    assert "SM_NEWCHR_FAIL(522)" in msg and "已被使用" in msg, msg

    # ⑤ 运行期（假 socket）：成功 / 重名 reuse / 重名 fail / code=3
    class _Sock:
        def __init__(self, reply):
            self.chunks = []
            self.sent = []
            self.reply = reply
            self.fired = False

        def recv(self, n):
            if not self.chunks:
                raise socket.timeout()
            return self.chunks.pop(0)

        def sendall(self, data):
            self.sent.append(bytes(data))
            if not self.fired and self.reply is not None:
                self.fired = True
                self.chunks.append(self.reply)

        def close(self):
            pass

    def mk(reply):
        import tempfile
        tmpd = tempfile.mkdtemp(prefix="mir2probe-newchr-")
        args = argparse.Namespace(out=tmpd, no_auto_handshake=True, timeout_ms=800,
                                  account="ua", char_name=None)
        run = Run(args)
        conn = Conn("selgate", run.recorder)
        conn.phase = "selgate"
        sock = _Sock(reply)
        conn.sock = sock
        return run, conn, tmpd

    import shutil

    def reply(frame):
        return build_s2c_frame(0, frame, 0, 0, 0, b"")

    plan = plan_step({"newchr": {"name": "abcdef", "job": 0, "hair": 0, "sex": 0}}, 0, {})
    run, conn, tmpd = mk(reply(SM_NEWCHR_SUCCESS))
    try:
        assert _do_newchr(run, conn, run.args, "sess", plan, []) == "abcdef"
        assert run.chain["newchr"]["result"] == "created"
        assert len(conn.sock.sent) == 1
    finally:
        shutil.rmtree(tmpd, ignore_errors=True)

    run, conn, tmpd = mk(reply(SM_NEWCHR_FAIL))
    try:
        # `SM_NEWCHR_FAIL` 的 nCode 在 recog 上（MakeMessage(SM_NEWCHR_FAIL, nCode, …)）
        conn.sock.reply = build_s2c_frame(2, SM_NEWCHR_FAIL, 0, 0, 0, b"")
        assert _do_newchr(run, conn, run.args, "sess", plan, ["abcdef"]) == "abcdef"
        assert run.chain["newchr"]["result"] == "reused"
    finally:
        shutil.rmtree(tmpd, ignore_errors=True)

    run, conn, tmpd = mk(build_s2c_frame(2, SM_NEWCHR_FAIL, 0, 0, 0, b""))
    plan_fail = plan_step({"newchr": {"name": "abcdef", "on_exists": "fail"}}, 0, {})
    try:
        err = None
        try:
            _do_newchr(run, conn, run.args, "sess", plan_fail, ["abcdef"])
        except StepFailure as ex:
            err = str(ex)
        assert err is not None, "duplicate+fail was not rejected"
        assert "SM_NEWCHR_FAIL(522)" in err and "已被使用" in err, err
        assert "server frames since newchr" in err, err
        assert run.chain["newchr"]["result"] == "failed"
    finally:
        shutil.rmtree(tmpd, ignore_errors=True)

    run, conn, tmpd = mk(build_s2c_frame(3, SM_NEWCHR_FAIL, 0, 0, 0, b""))
    try:
        err = None
        try:
            _do_newchr(run, conn, run.args, "sess", plan, ["abcdef"])
        except StepFailure as ex:
            err = str(ex)
        assert err is not None and "两个角色" in err, err
    finally:
        shutil.rmtree(tmpd, ignore_errors=True)

    # ⑥ S-2：只有 dlg_select_any + wait 的场景必须过（删 SEND_KINDS 即 R1 红）
    s2_doc = {"name": "selftest_send_kinds_dlg_any", "steps": [
        {"click_npc": "auto:last_turn"},
        {"wait_for": 643},
        {"dlg_select_any": ["@rw02", "@rw01"]},
        {"wait_for": 643},
    ]}
    s2_plans, _ = expand_scenario(s2_doc, {"npc_actor": 1})
    check_scenario(s2_doc, s2_plans, None, messages)
    only_doc = {"name": "selftest_send_kinds_only", "steps": [
        {"dlg_select_any": ["@rw02"]},
        {"wait_for": 643},
    ]}
    only_plans, _ = expand_scenario(only_doc, {})
    check_scenario(only_doc, only_plans, None, messages)  # 删 SEND_KINDS 里的 dlg_select_any ⇒ R1 红

    # ⑦ B1：没有 643 时 dlg_select_any 必须 fail-closed（不许 recog=0）
    import tempfile
    tmpd = tempfile.mkdtemp(prefix="mir2probe-b1-")
    try:
        args = argparse.Namespace(out=tmpd, no_auto_handshake=True, timeout_ms=200)
        run = Run(args)
        conn = Conn("gamegate", run.recorder, on_frame=run.on_frame)
        conn.phase = "gamegate"
        conn.sock = _FakeSock([])
        err = None
        try:
            run.execute_step(conn, plan_step({"dlg_select_any": ["@rw02"]}, 0, {}))
        except ScenarioError as ex:
            err = str(ex)
        assert err is not None and "no SM_MERCHANTSAY(643) seen yet" in err, err
    finally:
        shutil.rmtree(tmpd, ignore_errors=True)

    # ⑧ F2：newchr 载荷顺序（account/name/**hair**/job/sex——hair 在 job 前）
    assert newchr_payload("ua", "abcdef", 5, 2, 1) == b"ua/abcdef/5/2/1"
    run, conn, tmpd = mk(reply(SM_NEWCHR_SUCCESS))
    try:
        _do_newchr(run, conn, run.args, "sess", plan, [])
        sent = conn.sock.sent[-1]
        assert encode_text("ua/abcdef/0/0/0") in sent, sent  # plan 默认 hair=0/job=0/sex=0
    finally:
        shutil.rmtree(tmpd, ignore_errors=True)

    # ⑨ F2：first_pack 三条路径（SM_LOGON 位拆 dir/light、SM_NEWMAP(51) 取 map_file、
    #     SM_MAPDESCRIPTION(54) 取 map_desc）。**刻意在 51 前塞一条 634（body=WRONG）**：
    #     若实现回退成用 634 ⇒ map_file=WRONG ⇒ 本断言红（review F1 的构造级钉）。
    import tempfile
    tmpd = tempfile.mkdtemp(prefix="mir2probe-fp-")
    try:
        series = 3 | (5 << 8)  # MakeWord(Dir=3, Light=5)
        frames = [
            build_s2c_frame(7, SM_LOGON, 10, 20, series, b""),
            build_s2c_frame(0, SM_CHANGEMAP, 0, 0, 0, b"WRONG"),
            build_s2c_frame(0, SM_NEWMAP, 10, 20, 0, b"0114"),
            build_s2c_frame(0, SM_MAPDESCRIPTION, 0, 0, 0, "比奇省".encode("gbk")),
        ]
        args = argparse.Namespace(out=tmpd, no_auto_handshake=True, timeout_ms=200)
        run = Run(args)
        conn = Conn("gamegate", run.recorder, on_frame=run.on_frame)
        conn.phase = "gamegate"
        conn.sock = _FakeSock(frames)
        conn.pump(100)
        fp = run.state["first_pack"]
        assert fp["map_file"] == "0114", fp        # 来自 SM_NEWMAP(51)，不是 634
        assert fp["map_desc"] == "比奇省", fp
        assert (fp["x"], fp["y"]) == (10, 20), fp
        assert (fp["dir"], fp["light"]) == (3, 5), fp   # series&0xFF=Dir、>>8=Light
        assert fp["actor_id"] == 7 and fp["series"] == series, fp
    finally:
        shutil.rmtree(tmpd, ignore_errors=True)

    return ("形状 2 正 + %d 坏例；位置 2 坏例；reuse 4 象限；运行期 created/reused/fail(code2)/code3 4 例；"
            "S-2 与 B1 各 1 钉；F2 载荷序 + first_pack(51/位拆) 各 1" % len(bad))


def _selftest_kill():
    """`kill` 步（卡 `openmir2-probe-kill-step`）：

    * schema：默认 ident=CM_HIT(3014)/turn_ident=CM_TURN(3010)/interval 1000/max_swings 20/
      wait_ident=[SM_NOWDEATH(34), SM_DEATH(32)]；未知键/越界/缺 self_pos/非相邻 ⇒ ScenarioError；
    * 方向：tag = 自身→目标的八向 Dir（Direction.cs 口径）；
    * check_scenario：ident/turn_ident 必须 CM_*、wait_ident 必须 SM_*；
    * 运行期三态：死了（死亡帧带 (x,y)）/打了没死（他人 SM_STRUCK 带伤害或目标格活动帧）/
      没刷出来（窗口内目标格零帧）——错误串各不相同。
    """
    import argparse as _argparse
    import tempfile as _tempfile
    import shutil as _shutil

    checks = []
    self_xy = (330 | (330 << 16))
    tgt_xy = (330 | (331 << 16))          # 正下方一格 ⇒ Dir=4 (Down)

    # ① schema 正例与默认值
    plan = plan_step({"kill": {"target_xy": tgt_xy}}, 0, {"self_pos": self_xy})
    checks.append((plan["ident"] == CM_HIT and plan["turn_ident"] == CM_TURN
                   and plan["max_swings"] == 20 and plan["interval_ms"] == 1000
                   and plan["wait_idents"] == [SM_NOWDEATH, SM_DEATH]
                   and plan["tag"] == 4 and plan["recog"] == self_xy,
                   "kill 默认值：ident=3014/turn=3010/interval=1000/max=20/wait=[34,32]/tag=Down(4)"))
    plan2 = plan_step({"kill": {"target_xy": tgt_xy, "wait_ident": SM_NOWDEATH,
                                "max_swings": 5, "interval_ms": 350}},
                      0, {"self_pos": self_xy})
    checks.append((plan2["wait_idents"] == [SM_NOWDEATH] and plan2["max_swings"] == 5
                   and plan2["interval_ms"] == 350,
                   "kill 显式参数（wait_ident 单值归一成数组、max_swings/interval 可覆盖）"))

    # 八个方向
    ok_dirs = True
    for dxy, want in (((0, -1), 0), ((1, -1), 1), ((1, 0), 2), ((1, 1), 3),
                      ((0, 1), 4), ((-1, 1), 5), ((-1, 0), 6), ((-1, -1), 7)):
        tx = (330 + dxy[0]) | ((330 + dxy[1]) << 16)
        p = plan_step({"kill": {"target_xy": tx}}, 0, {"self_pos": self_xy})
        ok_dirs = ok_dirs and p["tag"] == want
    checks.append((ok_dirs, "kill 八向 Dir 映射与 Direction.cs 一致（0..7）"))

    # ② schema 反例
    bad_cases = [
        ({"kill": {}}, {}, "missing key"),
        ({"kill": {"target_xy": tgt_xy, "bogus": 1}}, {"self_pos": self_xy}, "unknown key"),
        ({"kill": {"target_xy": tgt_xy, "max_swings": 0}}, {"self_pos": self_xy},
         "out of range"),
        ({"kill": {"target_xy": tgt_xy, "max_swings": 101}}, {"self_pos": self_xy},
         "out of range"),
        ({"kill": {"target_xy": tgt_xy, "interval_ms": 349}}, {"self_pos": self_xy},
         "out of range"),
        ({"kill": {"target_xy": tgt_xy}}, {}, "self_pos"),
        ({"kill": {"target_xy": tgt_xy}, "timeout_ms": 5000}, {"self_pos": self_xy},
         "does not accept timeout_ms"),
        ({"kill": {"target_xy": (330 + 3) | (330 << 16)}}, {"self_pos": self_xy},
         "八向相邻"),
    ]
    bad_ok = True
    detail = ""
    for step, params, needle in bad_cases:
        try:
            plan_step(step, 0, params)
            bad_ok = False
            detail = "accepted %r" % (step,)
            break
        except ScenarioError as ex:
            if needle not in str(ex):
                bad_ok = False
                detail = "%s not in %s" % (needle, ex)
                break
    checks.append((bad_ok, "kill 坏例全拒（缺 target_xy/未知键/越界/缺 self_pos/非相邻）%s"
                   % ("" if bad_ok else detail)))

    # ③ check_scenario 方向校验
    msgs = load_message_constants()

    def check_kill(doc, params):
        plans, _ = expand_scenario(doc, params)
        check_scenario(doc, plans, params, msgs)

    good = {"name": "selftest_kill_good", "steps": [
        {"kill": {"target_xy": tgt_xy}}, {"wait_for": 32}]}
    try:
        check_kill(good, {"self_pos": self_xy})
        good_ok = True
    except ScenarioError:
        good_ok = False
    checks.append((good_ok, "check_scenario：合法 kill + wait_for(32) 通过"))
    bad_waits = []
    ident_err = ""
    for doc in (
            {"name": "selftest_kill_wait_cm", "steps": [
                {"kill": {"target_xy": tgt_xy, "wait_ident": CM_TURN}}]},
            {"name": "selftest_kill_ident_sm", "steps": [
                {"kill": {"target_xy": tgt_xy, "ident": SM_STRUCK}}]},
            # turn_ident 只由 R3b 校验（通用 R3 只看 kill 的 ident）——这条专钉 R3b
            {"name": "selftest_kill_turn_sm", "steps": [
                {"kill": {"target_xy": tgt_xy, "turn_ident": SM_STRUCK}}]},
    ):
        try:
            check_kill(doc, {"self_pos": self_xy})
            bad_waits.append("accepted %s" % doc["name"])
        except ScenarioError as ex:
            ident_err += str(ex)
            if "expected CM_*" not in str(ex) and "expected SM_*" not in str(ex):
                bad_waits.append("%s: unexpected %s" % (doc["name"], ex))
    # R3 的候选列表必须**动态**取自 SEND_KINDS（review `e9731a5e5` F3 第二半）：
    # 断言完整候选串出现，而不是只看「kill」这个词（它也会出现在 steps[0].kill 前缀里）。
    want_kinds = "/".join(sorted(SEND_KINDS))
    checks.append((not bad_waits and want_kinds in ident_err,
                   "check_scenario 拒 wait_ident=CM_* 与 ident=SM_*（%s；R3 候选串动态=%r）"
                   % ("；".join(bad_waits) or "三条都拒", want_kinds)))

    # ④ 运行期三态（假 socket：每次 sendall 触发一次脚本回帧）
    class _KSock(object):
        def __init__(self, replies):
            self.replies = list(replies)   # 每个元素：该次 sendall 后要推入的帧列表
            self.chunks = []
            self.sent = []

        def recv(self, n):
            if not self.chunks:
                raise socket.timeout()
            return self.chunks.pop(0)

        def sendall(self, data):
            self.sent.append(bytes(data))
            if self.replies:
                self.chunks.extend(self.replies.pop(0))

        def close(self):
            pass

    def mk(replies):
        tmpd = _tempfile.mkdtemp(prefix="mir2probe-kill-")
        args = _argparse.Namespace(out=tmpd, no_auto_handshake=True, timeout_ms=800,
                                   account="ua", char_name=None)
        run = Run(args)
        run.state["self_actor"] = 700
        conn = Conn("gamegate", run.recorder, on_frame=run.on_frame)
        conn.phase = "gamegate"
        conn.sock = _KSock(replies)
        return run, conn, tmpd

    def struck(actor=800, dmg=9):
        return build_s2c_frame(actor, SM_STRUCK, 100, 200, dmg, b"")

    def death(ident=SM_NOWDEATH, x=330, y=331, actor=800):
        return build_s2c_frame(actor, ident, x, y, 4, b"")

    plan3 = plan_step({"kill": {"target_xy": tgt_xy, "max_swings": 6,
                                "interval_ms": 350}}, 0, {"self_pos": self_xy})
    # 第 1 次 sendall 是转身（不推帧），之后每击一帧；第 3 击附带死亡帧
    run, conn, tmpd = mk([[], [struck()], [struck()], [struck(), death()]])
    try:
        run.execute_step(conn, plan3)
        rec = run.steps[-1]
        checks.append((rec["kind"] == "kill" and rec["result"] == "dead"
                       and rec["swings"] == 3 and rec.get("matched_ident") == SM_NOWDEATH
                       and rec["direction"] == 4,
                       "kill 运行期：第 3 击收到 SM_NOWDEATH(34)@(330,331) ⇒ result=dead（swings=3）"))
    finally:
        _shutil.rmtree(tmpd, ignore_errors=True)

    run, conn, tmpd = mk([[struck()] for _ in range(8)])
    try:
        err = None
        try:
            run.execute_step(conn, plan3)
        except StepFailure as ex:
            err = str(ex)
        checks.append((err is not None and "target alive after 6 swing(s)" in err
                       and "SM_STRUCK(31)" in err,
                       "kill 运行期：有伤害帧但没死 ⇒ target alive after 6 swing(s)（含帧摘要）"))
    finally:
        _shutil.rmtree(tmpd, ignore_errors=True)

    # F1（review `b0a7797b0`）：死亡帧的**坐标谓词**必须承重——同 ident 但在别的格子
    # 不能算击杀成功。变异「去掉 fr.param==tx and fr.tag==ty」时本条必须红。
    wrong = [[struck()] for _ in range(8)]
    wrong[3].append(death(x=331, y=331))
    run, conn, tmpd = mk(wrong)
    try:
        err = None
        try:
            run.execute_step(conn, plan3)
        except StepFailure as ex:
            err = str(ex)
        checks.append((err is not None and "target alive after 6 swing(s)" in err,
                       "kill 运行期：死亡帧在**别的格子**（331,331）不算成功 ⇒ target alive"
                       "（钉住坐标谓词）"))
    finally:
        _shutil.rmtree(tmpd, ignore_errors=True)

    run, conn, tmpd = mk([])
    try:
        err = None
        try:
            run.execute_step(conn, plan3)
        except StepFailure as ex:
            err = str(ex)
        checks.append((err is not None and "no target frames at (330,331)" in err,
                       "kill 运行期：目标格零帧 ⇒ no target frames（与 alive 错误串不同）"))
    finally:
        _shutil.rmtree(tmpd, ignore_errors=True)

    # F2（review `b0a7797b0`）：转身/挥击的**tag=Dir 与 recog=自身坐标**必须被钉
    run, conn, tmpd = mk([[], [struck()], [struck()], [struck(), death()]])
    calls = []
    orig_send = conn.send

    def _rec_send(recog, ident, param=0, tag=0, series=0, body=b""):
        calls.append((recog, ident, param, tag, series))
        return orig_send(recog, ident, param, tag, series, body)

    conn.send = _rec_send
    try:
        run.execute_step(conn, plan3)
        checks.append((calls[:2] == [(self_xy, CM_TURN, 0, 4, 0),
                                     (self_xy, CM_HIT, 0, 4, 0)],
                       "kill 运行期：前两帧 = (自身坐标, CM_TURN, tag=4) 与 "
                       "(自身坐标, CM_HIT, tag=4)（钉住 recog 与朝向）"))
    finally:
        _shutil.rmtree(tmpd, ignore_errors=True)

    # kill 属累计观察：只含 kill 的场景不应被「has no wait step」拒（review info）
    only_kill = {"name": "selftest_kill_only", "steps": [
        {"kill": {"target_xy": tgt_xy}}]}
    try:
        check_kill(only_kill, {"self_pos": self_xy})
        only_ok = True
    except ScenarioError:
        # review `ee452351` N1：任何拒绝都算失败（旧写法 "no wait step" not in msg 是恒真面）
        only_ok = False
    checks.append((only_ok, "check_scenario：只含 kill 的场景必须**不被拒**（任何 ScenarioError 都算红）"))

    # N2（review `ee452351`）：R2 的 no-send 文案必须有断言——只 wait 的场景，
    # 错误串里要出现完整候选（含 kill）。变异回旧硬编码串 ⇒ 红。
    only_wait = {"name": "selftest_r2_msg", "steps": [{"wait_for": SM_DEATH}]}
    r2_err = None
    try:
        check_kill(only_wait, {"self_pos": self_xy})
    except ScenarioError as ex:
        r2_err = str(ex)
    checks.append((r2_err is not None
                   and "/".join(sorted(SEND_KINDS)) in r2_err,
                   "R2 no-send 错误串点名完整候选（%r）"
                   % "/".join(sorted(SEND_KINDS))))

    for cond, msg in checks:
        assert cond, msg
    return "schema 正/反例 + 八向 + check 方向 3 例 + 运行期 dead/alive/no-target 3 态"


def _selftest_npc_discovery():
    """卡 openmir2-probe-npc-discovery：`click_npc` 的 name:/pos: 寻址（4 牙，构造级）。

    * schema：name:/pos: 展开成 `_npc_selector`（recog=None）；未知前缀/空名/坏坐标 ⇒
      ScenarioError；`check_scenario` 不把 name:/pos: 当 params 名（否则 R2 误报缺参）；
    * 运行期（合成 SM_TURN 出现帧，不连网、不占端口）：
      a. 同名两处 ⇒ StepFailure 且列出两处；b. 坐标格空 ⇒ StepFailure 且给邻近对象；
      c. 名字不存在 ⇒ StepFailure（**不回落 auto:last_turn**，且一帧都不发）；
      d. 唯一匹配 ⇒ 真的发出 `CM_CLICKNPC`，recog=该 actor id；
      另加同格多对象（pos 歧义）一例。
    """
    import argparse as _argparse
    import shutil as _shutil
    import tempfile as _tempfile

    def turn_frame(actor, x, y, name=None):
        """oracle 同形出现帧：'#' + Encode(12B 头) + Encode(8B CharDesc) [+ Encode("名/色")] + '!'。"""
        body = encode_bytes(bytes(8))            # Encode(8B CharDesc) = 11 字符
        if name is not None:
            body += encode_text(name + "/255")   # Encode("ShowName/Color")，两段各自编码
        return b"#" + encode_bytes(pack_header(actor, SM_TURN, x, y, 0)) + body + b"!"

    # ① schema 正例
    p_name = plan_step({"click_npc": "name:沙巴克城堡管理"}, 0, {})
    p_pos = plan_step({"click_npc": "pos:7,16"}, 0, {})
    assert p_name["_npc_selector"] == {"kind": "name", "value": "沙巴克城堡管理"}, p_name
    assert p_pos["_npc_selector"] == {"kind": "pos", "x": 7, "y": 16,
                                   "allow_any": False}, p_pos
    assert p_name["ident"] == CM_CLICKNPC and p_name["recog"] is None, p_name
    # ② schema 反例（fail-closed）
    bad = [({"click_npc": "name:"}, "必须跟 NPC 名"),
           ({"click_npc": "name:   "}, "必须跟 NPC 名"),
           ({"click_npc": "pos:7"}, "pos:<x>,<y>"),
           ({"click_npc": "pos:-1,2"}, "pos:<x>,<y>"),
           ({"click_npc": "pos:7,70000"}, "越界"),
           ({"click_npc": "auto:lastturn"}, "未知寻址写法"),
           ({"click_npc": "name:商人?allow_any=1"}, "allow_any` 只适用于 `pos:")]
    for step, needle in bad:
        err = None
        try:
            plan_step(step, 0, {})
        except ScenarioError as ex:
            err = str(ex)
        assert err is not None, "accepted bad click_npc: %r" % (step,)
        assert needle in err, (needle, err)
    # ③ check_scenario：name:/pos: 不是 params 名，不得报缺参
    messages = load_message_constants()
    doc = {"name": "selftest_npc_discovery", "steps": [
        {"click_npc": "name:沙巴克城堡管理"}, {"wait_for": 643},
        {"click_npc": "pos:7,16"}, {"wait_for": 643}]}
    plans, _ = expand_scenario(doc, {})
    check_scenario(doc, plans, None, messages)

    def run_with(frames):
        tmpd = _tempfile.mkdtemp(prefix="mir2probe-npc-")
        args = _argparse.Namespace(out=tmpd, no_auto_handshake=True, timeout_ms=200)
        run = Run(args)
        conn = Conn("gamegate", run.recorder, on_frame=run.on_frame)
        conn.phase = "gamegate"
        conn.sock = _FakeSock(list(frames))
        conn.pump(100)
        return run, conn, tmpd

    def click(run, conn, spec):
        run.execute_step(conn, plan_step({"click_npc": spec}, 0, {}))

    # 牙 a：同名两处 ⇒ 红且列出两处
    run, conn, tmpd = run_with([turn_frame(101, 7, 16, "守卫"),
                                turn_frame(102, 9, 16, "守卫")])
    try:
        err = None
        try:
            click(run, conn, "name:守卫")
        except StepFailure as ex:
            err = str(ex)
        assert err is not None, "同名两处未被拒（会挑第一个）"
        assert "歧义" in err and "actor=101" in err and "actor=102" in err, err
        assert "(7,16)" in err and "(9,16)" in err, err
        assert conn.sock.sent == [], "歧义却发了帧：%r" % (conn.sock.sent,)
    finally:
        _shutil.rmtree(tmpd, ignore_errors=True)

    # 牙 b：坐标格空 ⇒ 红且给出该格邻近可见对象
    run, conn, tmpd = run_with([turn_frame(201, 7, 16, "商人")])
    try:
        err = None
        try:
            click(run, conn, "pos:8,17")
        except StepFailure as ex:
            err = str(ex)
        assert err is not None, "空格坐标未被拒"
        assert "邻近" in err and "actor=201" in err, err
        assert conn.sock.sent == [], "空坐标却发了帧：%r" % (conn.sock.sent,)
    finally:
        _shutil.rmtree(tmpd, ignore_errors=True)

    # 牙 c：名字不存在 ⇒ 红（**不许**静默回落到 auto:last_turn）
    run, conn, tmpd = run_with([turn_frame(201, 7, 16, "商人")])
    try:
        err = None
        try:
            click(run, conn, "name:不存在的NPC")
        except StepFailure as ex:
            err = str(ex)
        assert err is not None, "不存在的名字未被拒"
        assert "没有对象匹配" in err and "actor=201" in err, err
        assert "last_turn" not in err, err
        assert conn.sock.sent == [], "名字不存在却发了帧：%r" % (conn.sock.sent,)
    finally:
        _shutil.rmtree(tmpd, ignore_errors=True)

    # 牙 d：唯一匹配 ⇒ 真的发出 CM_CLICKNPC，recog = 该 actor id
    run, conn, tmpd = run_with([turn_frame(301, 7, 16, "沙巴克城堡管理")])
    try:
        click(run, conn, "name:沙巴克城堡管理")
        assert len(conn.sock.sent) == 1, conn.sock.sent
        assert encode_bytes(pack_header(301, CM_CLICKNPC)) in conn.sock.sent[0], conn.sock.sent
        step = [x for x in run.steps if x.get("kind") == "click_npc"][-1]
        assert step["actor"] == 301 and step["name"] == "沙巴克城堡管理", step
        assert (step["x"], step["y"]) == (7, 16), step
    finally:
        _shutil.rmtree(tmpd, ignore_errors=True)

    # 另：pos 上多个对象 ⇒ 同样红（fail-closed，不挑「第一个」）
    run, conn, tmpd = run_with([turn_frame(401, 7, 16, "甲"),
                                turn_frame(402, 7, 16, "乙")])
    try:
        err = None
        try:
            click(run, conn, "pos:7,16")
        except StepFailure as ex:
            err = str(ex)
        assert err is not None, "同格多对象未被拒"
        assert "actor=401" in err and "actor=402" in err and "歧义" in err, err
        assert conn.sock.sent == [], "歧义却发了帧：%r" % (conn.sock.sent,)
    finally:
        _shutil.rmtree(tmpd, ignore_errors=True)

    # ---- N2（卡 openmir2-probe-npc-addressing-followups）：pos: 的对象类别门 ----
    # 造一个「动过」的对象：先出现，再一条移动帧
    def moved_frame(actor, x, y):
        return build_s2c_frame(actor, SM_WALK, x, y, 0, b"")

    run, conn, tmpd = run_with([turn_frame(601, 7, 16, "野怪"),
                                moved_frame(601, 8, 16)])
    try:
        err = None
        try:
            click(run, conn, "pos:8,16")
        except StepFailure as ex:
            err = str(ex)
        assert err is not None, "活动对象被 pos: 直接命中（N2 未修）"
        assert "不是固定/静态形态" in err and "移动帧" in err, err
        assert "类别=活动" in err and "allow_any=1" in err, err
        assert conn.sock.sent == [], "活动对象却发了帧：%r" % (conn.sock.sent,)
    finally:
        _shutil.rmtree(tmpd, ignore_errors=True)

    # 逃生阀：显式 ?allow_any=1 才按坐标选中活动对象（默认关闭）
    run, conn, tmpd = run_with([turn_frame(601, 7, 16, "野怪"),
                                moved_frame(601, 8, 16)])
    try:
        click(run, conn, "pos:8,16?allow_any=1")
        assert len(conn.sock.sent) == 1, conn.sock.sent
        assert encode_bytes(pack_header(601, CM_CLICKNPC)) in conn.sock.sent[0], conn.sock.sent
        step = [x for x in run.steps if x.get("kind") == "click_npc"][-1]
        assert step["actor"] == 601 and step["selector"]["allow_any"] is True, step
    finally:
        _shutil.rmtree(tmpd, ignore_errors=True)

    # 「首见于初始快照之后」的对象同样不是静态：先执行一步钉住快照界，再让对象出现
    run, conn, tmpd = run_with([])
    try:
        run.execute_step(conn, plan_step({"sleep": 1}, 0, {}))
        assert "snapshot_end_seq" in run.state
        conn.sock.chunks.append(turn_frame(701, 7, 16, "后来刷的"))
        conn.pump(100)
        err = None
        try:
            click(run, conn, "pos:7,16")
        except StepFailure as ex:
            err = str(ex)
        assert err is not None, "晚于初始快照的对象被 pos: 命中"
        assert "不是固定/静态形态" in err and "晚于初始快照" in err, err
    finally:
        _shutil.rmtree(tmpd, ignore_errors=True)

    # ---- N1：自身过滤的牙（name: 与 pos: 各一颗；去掉 self 过滤 ⇒ 变绿 ⇒ 必须红）----
    # 自身 actor=900 在 (7,16)、名字「守卫」；NPC=901 在 (9,16)、名字「商人」
    run, conn, tmpd = run_with([turn_frame(900, 7, 16, "守卫"),
                                turn_frame(901, 9, 16, "商人")])
    try:
        run.state["self_actor"] = 900
        err = None
        try:
            click(run, conn, "name:守卫")
        except StepFailure as ex:
            err = str(ex)
        assert err is not None, "把自己当成按名候选（N1 未修）"
        assert "actor=900" in err or "没有对象匹配" in err, err
        assert conn.sock.sent == [], "按名命中自己却发了帧：%r" % (conn.sock.sent,)
    finally:
        _shutil.rmtree(tmpd, ignore_errors=True)

    run, conn, tmpd = run_with([turn_frame(900, 7, 16, "守卫"),
                                turn_frame(901, 9, 16, "商人")])
    try:
        run.state["self_actor"] = 900
        err = None
        try:
            click(run, conn, "pos:7,16")
        except StepFailure as ex:
            err = str(ex)
        assert err is not None, "把自己当成按坐标候选（N1 pos: 未修）"
        assert "没有对象匹配" in err, err
        assert conn.sock.sent == [], "按坐标命中自己却发了帧：%r" % (conn.sock.sent,)
    finally:
        _shutil.rmtree(tmpd, ignore_errors=True)

    # ---- N3：名字含 '/' 的边界 ----
    # 正确：名尾是 "GM/测试/255" ⇒ 名字 = "GM/测试"（按最后一个 '/' 切，且最后一段是颜色）
    run, conn, tmpd = run_with([turn_frame(801, 7, 16, "GM/测试")])
    try:
        click(run, conn, "name:GM/测试")
        step = [x for x in run.steps if x.get("kind") == "click_npc"][-1]
        assert step["actor"] == 801 and step["name"] == "GM/测试", step
    finally:
        _shutil.rmtree(tmpd, ignore_errors=True)

    # 名尾没有颜色段（"A/B"）⇒ `<无名>` + 原始名尾，不得被截断成 "A"
    run, conn, tmpd = run_with([])
    try:
        body = encode_bytes(bytes(8)) + encode_text("A/B")
        conn.sock.chunks.append(b"#" + encode_bytes(pack_header(802, SM_TURN, 7, 16, 0))
                                + body + b"!")
        conn.pump(100)
        err = None
        try:
            click(run, conn, "name:A")
        except StepFailure as ex:
            err = str(ex)
        assert err is not None, "无名尾被截断成前缀名（N3 未修）"
        assert "没有对象匹配" in err and "<无名>" in err and "原始名尾" in err, err
        assert conn.sock.sent == [], "把无名对象当成 A 却发了帧：%r" % (conn.sock.sent,)
    finally:
        _shutil.rmtree(tmpd, ignore_errors=True)

    # ---- 静态判据的三颗**不可替代**牙（卡 openmir2-probe-npc-teeth）----
    # 牙 i：同格 SM_RUSH(6) —— 坐标没变，但**有移动帧** ⇒ 必须是「活动」（咬 `moves`）
    run, conn, tmpd = run_with([turn_frame(911, 7, 16, "怪"),
                                build_s2c_frame(911, SM_RUSH, 7, 16, 0, b"")])
    try:
        err = None
        try:
            click(run, conn, "pos:7,16")
        except StepFailure as ex:
            err = str(ex)
        assert err is not None, "同格 SM_RUSH 被判成静态（moves 判据失效）"
        assert "不是固定/静态形态" in err and "移动帧 1 条" in err, err
    finally:
        _shutil.rmtree(tmpd, ignore_errors=True)

    # 牙 j：两条 SM_TURN 报不同坐标、**无移动帧** ⇒ 必须是「活动」（咬 `coords`）
    run, conn, tmpd = run_with([turn_frame(912, 7, 16, "怪"),
                                turn_frame(912, 8, 16, "怪")])
    try:
        err = None
        try:
            click(run, conn, "pos:8,16")
        except StepFailure as ex:
            err = str(ex)
        assert err is not None, "坐标变过仍被判成静态（coords 判据失效）"
        assert "不是固定/静态形态" in err and "坐标变过" in err, err
    finally:
        _shutil.rmtree(tmpd, ignore_errors=True)

    # 牙 k：出现 → SM_DISAPPEAR → **再出现** ⇒ 再现后能命中（咬「再出现重加」）
    run, conn, tmpd = run_with([turn_frame(913, 7, 16, "商人"),
                                build_s2c_frame(913, SM_DISAPPEAR, 0, 0, 0, b""),
                                turn_frame(913, 7, 16, "商人")])
    try:
        click(run, conn, "name:商人")
        assert len(conn.sock.sent) == 1, conn.sock.sent
        assert encode_bytes(pack_header(913, CM_CLICKNPC)) in conn.sock.sent[0], conn.sock.sent
    finally:
        _shutil.rmtree(tmpd, ignore_errors=True)

    # ---- 第 4 颗牙：消失后**再来移动帧**不得复活（卡 openmir2-probe-npc-doc-teeth-followups 的 F2）----
    # 正确行为：移动帧只更新「动过」证据，**不**清 disappeared；否则加一行 `cur["disappeared"] = False`
    # 到移动分支，作者 selftest 会全绿。
    run, conn, tmpd = run_with([turn_frame(921, 7, 16, "商人"),
                                build_s2c_frame(921, SM_DISAPPEAR, 0, 0, 0, b""),
                                build_s2c_frame(921, SM_WALK, 7, 16, 0, b"")])
    try:
        err = None
        try:
            click(run, conn, "name:商人")
        except StepFailure as ex:
            err = str(ex)
        assert err is not None, "消失后的一帧移动把对象复活了（移动分支不得清 disappeared）"
        assert conn.sock.sent == [], "对已消失对象发了帧：%r" % (conn.sock.sent,)
    finally:
        _shutil.rmtree(tmpd, ignore_errors=True)

    # ---- SM_DISAPPEAR(30)：已消失对象不再是候选 ----
    run, conn, tmpd = run_with([turn_frame(901, 7, 16, "商人"),
                                build_s2c_frame(901, SM_DISAPPEAR, 0, 0, 0, b"")])
    try:
        err = None
        try:
            click(run, conn, "name:商人")
        except StepFailure as ex:
            err = str(ex)
        assert err is not None, "已消失对象仍被当成候选（DISAPPEAR 未剔除）"
        assert conn.sock.sent == [], "点已消失对象却发了帧：%r" % (conn.sock.sent,)
    finally:
        _shutil.rmtree(tmpd, ignore_errors=True)

    return ("name:/pos: schema 2 正 6 反；牙 a/b/c/d + 同格多对象；"
            "N2 活动对象/allow_any/晚于初始快照 3 例；N1 name:/pos: 自身过滤 2 例；"
            "N3 名含/与无名尾 2 例；DISAPPEAR 剔除 1 例；"
            "静态三颗独立牙（同格 SM_RUSH / 双 TURN 换格 / 消失再出现）+ name?allow_any 加载期拒")


def run_selftest():
    fails = []
    checked = [0]

    def check(name, fn):
        checked[0] += 1
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
    check("scenario-requires", _selftest_scenario_requires)
    check("scenario-inventory", _selftest_scenario_inventory)
    check("scenario-assets", _selftest_scenario_assets)
    check("dsl-dlg-select-any", _selftest_dlg_select_any)
    check("dsl-newchr", _selftest_newchr)
    check("dsl-kill", _selftest_kill)
    check("dsl-npc-discovery", _selftest_npc_discovery)
    check("scenario-validation", _selftest_scenario_validation)
    check("helpers", _selftest_helpers)
    check("conn-semantics", _selftest_conn_semantics)
    check("multi-session", _selftest_multi_session)
    check("dwell-after-enter", _selftest_dwell)
    check("wait-recog-filter", _selftest_wait_recog_filter)
    if fails:
        print("mir2_probe selftest: FAIL")
        for f in fails:
            print("  -", f)
        return 2
    print("mir2_probe selftest: OK (%d checks; fixtures replayed at 1/2/3/5/7/11/13/17/64-byte chunks)"
          % checked[0])
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


def _selftest_multi_session():
    """卡 openmir2-multi-session-probe：会话标注 + 收件人集合断言（5 用例 + 2 牙）。

    帧行用手搓的最小集合（字段与 `Recorder.write_frame` 同形），不连服务端。
    """
    def row(seq, conn, ident, t_ms=0, body_text=""):
        return {"seq": seq, "t_ms": t_ms, "conn": conn, "ident": ident,
                "param": 0, "tag": 0, "series": 0, "body_text": body_text}

    pred = {"ident": 657}  # SM_CHARSTATUSCHANGED（广播类）

    # ① 两会话都在同图两格内 ⇒ 都收到
    rows = [row(1, session_label("s1", "gamegate"), 657), row(2, session_label("s2", "gamegate"), 657)]
    assert_recipients(["s1", "s2"], recipients(rows, pred))

    # ② 一格超出 ±12 ⇒ 只近的收到（远的那条帧根本不存在）；期望含远者必须报 missing
    rows_far = [row(1, session_label("s1", "gamegate"), 657)]
    assert_recipients(["s1"], recipients(rows_far, pred))
    try:
        assert_recipients(["s1", "s2"], recipients(rows_far, pred))
    except AssertionError as ex:
        assert "missing=['s2']" in str(ex), str(ex)
    else:
        raise AssertionError("超界一格时不该把远会话算进收件人")

    # ③ 三会话同格 ⇒ 三个都收到且首帧次序一致
    rows3 = [row(1, session_label("s1", "gamegate"), 657),
             row(2, session_label("s2", "gamegate"), 657),
             row(3, session_label("s3", "gamegate"), 657)]
    assert_recipients(["s1", "s2", "s3"], recipients(rows3, pred))
    seqs = recipient_first_seqs(rows3, pred)
    assert seqs == {"s1": 1, "s2": 2, "s3": 3}, seqs

    # ④ 一个会话不在线 ⇒ 不参与判定
    online = online_sessions({"s1": "online", "s2": "online", "s3": "offline"})
    assert online == ["s1", "s2"], online
    assert_recipients(["s1", "s2"], recipients(rows3, pred, sessions=online))

    # ⑤ 会话断开后不再收：断线会话从**参与判定的集合**里剔除（不是靠帧恰好不存在）
    rows5 = [row(1, session_label("s1", "gamegate"), 657, t_ms=10),
             row(2, session_label("s2", "gamegate"), 657, t_ms=20),
             row(3, session_label("s1", "gamegate"), 657, t_ms=150),
             row(4, session_label("s2", "gamegate"), 657, t_ms=500)]
    after = dict(pred, t_from=100)
    alive = online_sessions({"s1": "online", "s2": "closed"})
    assert alive == ["s1"], alive
    assert_recipients(["s1"], recipients(rows5, after, sessions=alive))
    # 反证：若把已断开的 s2 也算进来，它的 500ms 帧会污染期望 ⇒ 必须被集合剔除挡住
    assert_recipients(["s1", "s2"], recipients(rows5, after))

    # 牙①：把断言放宽成「有人收到就算过」⇒ ② 的 missing 抓不到 ⇒ 必须红
    def _lax(expected, actual):
        if not actual:
            raise AssertionError("没人收到")
    try:
        _lax(["s1", "s2"], recipients(rows_far, pred))
    except AssertionError:
        raise AssertionError("牙①失效：放宽断言反而报错")
    # 放宽版放过了「缺 s2」——严格版必须拒绝（上面 ② 已证），这里再钉一次差异
    assert_recipients(["s1"], recipients(rows_far, pred))

    # 牙②：±12 改 ±11 ⇒ 第 12 格的会话被判成收不到（期望集合随之变化）
    assert within_radius(12) and within_radius(-12)
    assert not within_radius(12, radius=11) and within_radius(11, radius=11)

    # ⑥ 复审 I-1：`statuses` 进判定——非 online 会话不参与
    st = {"s1": "online", "s2": "offline", "s3": "closed"}
    assert recipients(rows3, pred, statuses=st) == ["s1"], "statuses 给了 ⇒ 只计 online"
    # ⑦ 复审 I-1：带状态的断言口径
    assert_recipients_with_status(st, ["s1", "s2"], ["s1"])  # s2 offline 不计入 expected
    try:
        assert_recipients_with_status(st, ["s1"], ["s1", "s2"])  # s2 已 offline 却收到帧
    except AssertionError as ex:
        assert "非 online 会话收到帧" in str(ex), str(ex)
    else:
        raise AssertionError("已断会话收到帧必须报错（I-1）")
    # 牙（I-1）：不带状态的旧断言会放过「expected 里的 offline 会话」——新断言不会
    try:
        assert_recipients(["s1", "s2"], ["s1"])
    except AssertionError:
        pass
    # ⑧ 复审 M-1：视野是正方形（端点在内），不是欧氏半径
    assert within_square(12, 12) and within_square(-12, 12)
    assert not within_square(13, 0) and not within_square(0, -13)
    assert within_radius(12) and not within_radius(13)
    # ⑨ 复审 `bc2a5f2b` I-1：expected 里的未登记会话必须报错（不是静默剔除）
    try:
        assert_recipients_with_status(st, ["s1", "s9"], ["s1"])
    except AssertionError as ex:
        assert "未登记连接状态" in str(ex) and "s9" in str(ex), str(ex)
    else:
        raise AssertionError("expected 里拼错/未登记的会话被静默剔除（I-1 假绿）")
    # ⑩ sessions 与 statuses 同时给 ⇒ 报错（不许静默让 statuses 失效）
    try:
        recipients(rows3, pred, sessions=["s2"], statuses={"s1": "online"})
    except ValueError as ex:
        assert "只能给一个" in str(ex), str(ex)
    else:
        raise AssertionError("sessions+statuses 同时给被静默接受（M-1 分叉）")
    # ⑪ statuses 未覆盖帧里出现过的会话 ⇒ 报错（不许静默过滤）
    try:
        recipients(rows3, pred, statuses={"s1": "online"})
    except ValueError as ex:
        assert "不在 statuses 里" in str(ex), str(ex)
    else:
        raise AssertionError("未登记会话的帧被静默剔除")
    # 单会话回归：旧标签不含 `.` ⇒ 归 "main"
    rows_main = [row(1, "gamegate", 657), row(2, "selgate", 657)]
    assert recipients(rows_main, pred) == ["main"]


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
