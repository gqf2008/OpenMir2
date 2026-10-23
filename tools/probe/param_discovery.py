#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""param_discovery.py — 对拍台的「参数发现」阶段（卡 openmir2-window-param-discovery）。

背景：15 个场景里 9 个声明了 `required` 运行时变量（NPC/商人 ActorId、物品 MakeIndex、自身坐标），
缺 `--var` 即加载期 `scenario-error`(rc=2)。本模块负责**从已录 s2c 帧里推导这些变量**；
推导不出来就 fail-closed（逐个点名缺哪个），**绝不用默认值/猜值**（与 DSL 的 `required` 语义一致）。

「推导」与「规则」严格分栏（返工后；依据与代价见 docs/param-discovery-design-20261023.md §2）：

| 变量 | 类别 | 来源 | oracle / 依据 |
|---|---|---|---|
| `self_x` / `self_y` | 推导 | `SM_LOGON(50)` 的 `param` / `tag` | `PlayObject.cs:1128-1131 SendLogon()` `MakeMessage(SM_LOGON, ActorId, CurrX, CurrY, …)` |
| `merchant_actor` | 推导 | `SM_MERCHANTSAY(643)` 的 `recog` | 643 是 NPC **应答点击**才发的面板（`NormNpc.Click → GotoLable(@main) → RM_MERCHANTSAY`），`PlayObject.Message.cs:1721-1724` |
| `npc_actor` | 优先推导，退化为**规则** | 有 643 就取 643 的 `recog`（实证）；否则取 `SM_TURN(10)` 首个 `recog != self` | 退化规则是**部署假设**（非帧语义）：点错 actor 时后续步骤**响亮超时**，不会静默通过 |
| `king_actor` | **规则**（部署假设） | `SM_MERCHANTSAY(643)` 的 `recog` | 643 只证明「该 actor 应答了点击」，**不证明是国王**（仓库里全部 643 录制都是赌场庄家）⇒ 记 `kind=rule`，不计入发现能力；点错则末步响亮超时 |
| `make_index` / `item_name` | 推导 | `SM_BAGITEMS(201)` 包体里**同一个物品槽**的 `MakeIndex` / `Name` | `PlayObject.Operate.cs:38-64 ClientQueryBagItems()`：`MakeMessage(SM_BAGITEMS, ActorId, 0, 0, ItemList.Count)`，包体 = 逐项 `EDCode.EncodeBuffer(ClientItem) + '/'`；记录**定长 124B**（`ClientItem.WritePacket`）：`Name`@0（1B 长度 + 14B 定长，GBK）、`StdMode`@15、`MakeIndex`@100（i32 LE）。**帧里没有 StdItem 的 `Index`**（客户端靠 Name 识别），所以「MakeIndex 取成 Index」在线上不可能发生 |
| `takeon_where` | 推导 | **同一物品槽**的 `StdMode` → `ItemLocation` | `M2Share.CheckUserItems`（`src/M2Server/M2Share.cs:376-458`）的逆表：StdMode 5/6→Weapon(1)、10/11→Dress(0)、15→Helmet(4)、19/20/21→Necklace(3)、22/23→Ringl(7)、24/25/26→ArmRingl(5)、28/29/30→RighThand(2)、51→Bujuk(9)、52/62→Boots(11)、53/63→Charm(12)、54/64→Belt(10)；同 StdMode 多槽时取**最小槽号**（24/25/26 只可能 ArmRingl 而非 ArmRingr/Bujuk） |
| `move_x` / `move_y` | **计算**（`kind=computed`） | 场景 `computed` 声明：`{derived_from: self_x, delta: N}` + 已推导量 | 目标格是**操作者的选择**而非帧事实 ⇒ 由「已推导量 + 场景显式 delta」确定性算出，**不是猜**；oracle `PlayObject.Attack.cs:1365 ClientWalkXY`（目标格为绝对坐标） |
| `target_*` / `hit_dir` | 不可推导 | 无来源帧 | fail-closed（可由操作者 `--var` 显式给）；`combat_hit` 的目标是**实体**，需一帧报出附近怪物位置（见 `docs/window-runbook-20261023.md` §4.5） |

**201 的「取哪个物品」规则（显式、确定、不随机）**：只看**首条** `SM_BAGITEMS(201)`（槽序 = oracle
`ItemList` 序），按场景 `required` 选**第一个**满足谓词的槽——谓词来自 oracle 的可操作性判定：

- `required` 含 `takeon_where`（`bag_equip`）⇒ **可装备**：`StdMode` 在 `M2Share.CheckUserItems` 的
  槽表里（否则 `CheckUserItems` 为假 ⇒ `SM_TAKEON_FAIL`，场景没有意义）；
- 其余（`eat_item`）⇒ **可食用**：`StdMode ∈ {0,1,2,3,4,31}`（`PlayObject.Operate.cs:601-676
  ClientUseItems` 的 `switch` 分支；其余 StdMode 恒 `SM_EAT_FAIL`），且按**固定价值次序**取
  `StdMode31 & AniCount!=0`（QFunction `@StdModeFunc<N>`，有脚本可判）→ `StdMode31 & AniCount==0`（解包）
  → 药（0/1/2/3）→ 书（4）；同档次序取最小槽。

`make_index` / `item_name` / `takeon_where` **必须来自同一个槽**（一次 derive 只选一次），否则三个 `--var`
指向不同物品，oracle 的「MakeIndex + 名称双匹配」必然落空。没有候选 ⇒ **missing**（逐个点名），
**不许**退回「随便取一个」或默认 MakeIndex。

「计算」（`computed`）是第三类，**与 `derived`/`rule` 分开计**：它不证明「发现能力已验证」，只证明
「该变量可由场景声明 + 已推导量确定性复算」。声明写在场景 JSON 顶层 `computed` 对象里：

```json
"required": ["move_x", "move_y"],
"computed": {
  "move_x": {"derived_from": "self_x", "delta": 1},
  "move_y": {"derived_from": "self_y", "delta": 0}
}
```

- `derived_from` 必须是**非 computed** 的变量名（不支持链式，避免环）；基准量即使不在 `required` 里也会先推导。
- `delta` 必须是整数（`bool` 不算）；**未声明 / 类型不对 ⇒ 场景坏掉（`DiscoveryError`，rc=2）**，不是「缺变量」。
- `computed` 的键必须出现在 `required` 里（声明了却不用 ⇒ 场景坏掉）。
- 基准量推不出 ⇒ 该变量**连锁 fail-closed**（missing 里同样点名）。
- 没有 `computed` 声明（例如单变量推导入口 `param_discovery.py <frames> <vars>`）⇒ 这类变量照旧 missing，**绝不猜 delta**。
- `mir2_probe` 不读 `computed`：它只是场景的**声明性注记**，不是步骤 DSL（无新 step 类型、不改 send/wait 语义），
  但顶层可选键里必须放行，否则带声明的场景会被 `unknown key(s)` 拒绝。
- **已知局限（无机器守卫）**：`derived_from` 的**语义正确性**机判不了——例如把 `move_x.derived_from`
  从 `self_x` 改成 `self_y`，`selftest` 仍绿、`check-all` 仍 rc=0（reviewer 自造牙证实）。接线卡必须在真机判据里
  补一条断言（如「目标格 = 起始格 + 声明的 delta」，且两侧一致）。

用法：

```sh
python -I -B tools/probe/param_discovery.py selftest                 # 构造级用例 + 牙（离线）

# 单次推导：输出 values/missing/argv，missing 非空 ⇒ rc=2
python -I -B tools/probe/param_discovery.py <frames.ndjson> npc_actor,self_x

# 逐场景：读每个场景的 required ⇒ 推导 ⇒ 拼 --var 跑 mir2_probe --check-scenario
python -I -B tools/probe/param_discovery.py check-all <frames.ndjson> [scenarios_dir]
python -I -B tools/probe/param_discovery.py check-all <frames> [dir] --allow-missing[=N]
```

`check-all` 的退出码（fail-closed 口径）：

- 默认：**任一**行非 0（`missing` 或 `error`）⇒ rc=2；
- `--allow-missing`：只放行本模块自己的 `missing` 行（不可推导已登记）；`error` 行（场景本身坏了）**仍 rc=2**；
- `--allow-missing=N`：放行的同时钉住 `missing` 行数必须恰好为 N，否则 rc=2（防止多坏了几条还悄悄放行）；
- **零场景判红**：`scenarios_dir` 不存在/不是目录、或目录里一个 `.json` 都没有 ⇒ rc=2（「检查了零个场景」绝不当绿）；
- **场景文件坏掉**（坏 JSON / 顶层非对象 / `required` 不是非空字符串数组 / `params` 非对象）⇒ 该行
  `cls=error`、`scenario-load: …`、整体 rc=2，`--allow-missing` 也不放行。

每行带分类（`cls`）：`derived`（required 全部由本模块**推导**，唯一算「发现能力已验证」）/
`rule`（required 里有变量来自退化**规则**，即部署假设，**不计入**发现能力）/
`no_required`（场景无需变量）/ `params_default`（**绿来自场景 params 硬编码默认值，不是发现值**）/
`missing`（本模块 fail-closed）/ `error`（场景校验或加载失败）。
"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # -I 下脚本目录不在 sys.path
import mir2frame  # noqa: E402  （EDCode 解码：201 包体是逐记录 EDCode 编码 + '/' 分隔）

SM_TURN = 10
SM_LOGON = 50
SM_MERCHANTSAY = 643
SM_BAGITEMS = 201

#: 变量 -> 推导规则（写进输出，便于复审复算）；`kind` 区分推导与规则。
RULES = {
    "self_x": {
        "kind": "derived", "ident": SM_LOGON, "field": "param",
        "rule": "SM_LOGON(50).param",
        "oracle": "src/M2Server/Player/PlayObject.cs:1128-1131 SendLogon() "
                  "MakeMessage(SM_LOGON, ActorId, CurrX, CurrY, MakeWord(Dir, Light))",
    },
    "self_y": {
        "kind": "derived", "ident": SM_LOGON, "field": "tag",
        "rule": "SM_LOGON(50).tag",
        "oracle": "src/M2Server/Player/PlayObject.cs:1128-1131 SendLogon()（同上，tag=CurrY）",
    },
    "merchant_actor": {
        "kind": "derived", "ident": SM_MERCHANTSAY, "field": "recog",
        "rule": "SM_MERCHANTSAY(643).recog",
        "oracle": "NormNpc.Click → GotoLable(@main) → RM_MERCHANTSAY"
                  "（src/M2Server/Npc/NormNpc.cs:68-73,1615；"
                  "src/M2Server/Player/PlayObject.Message.cs:1721-1724）",
    },
    "npc_actor": {
        "kind": "derived", "ident": SM_MERCHANTSAY, "field": "recog",
        "rule": "SM_MERCHANTSAY(643).recog（有 643 时）",
        "oracle": "同 merchant_actor；没有 643 时退化为 RULES_FALLBACK",
    },
    "king_actor": {
        "kind": "rule", "ident": SM_MERCHANTSAY, "field": "recog",
        "rule": "SM_MERCHANTSAY(643).recog（**部署假设**：发现 run 点中的是国王）",
        "oracle": "643 只证明「这个 actor 应答了本次点击」（src/M2Server/Npc/NormNpc.cs:68-73,1615；"
                  "src/M2Server/Player/PlayObject.Message.cs:1721-1724），**不证明它是国王**："
                  "仓库里所有 643 录制都是赌场庄家（〖庄〗…@kaiz），无一含 @@buildguildnow/国王名。"
                  "点错国王的后果是**响亮失败**（ClientMerchantDlgSelect → UserSelect → GotoLable 查不到 label"
                  " ⇒ 零帧 ⇒ 末步超时），不会静默通过。⇒ 与 npc_actor 的退化规则同栏（kind=rule），"
                  "不计入「发现能力已验证」",
    },
}

#: `computed` 类变量的 oracle（写进输出便于复审复算）；未列出的走通用文案。
COMPUTED_ORACLES = {
    "move_x": "src/M2Server/Player/PlayObject.Attack.cs:1365-1428 ClientWalkXY(wIdent, nX, nY)："
              "CM_WALK(3011) 的 nParam1/nParam2 是**绝对**目标格（PlayObject.Message.cs:1059-1060）",
    "move_y": "同上（nParam1=X → recog，nParam2=Y → param）",
}

#: `computed` 声明允许且必需的字段（多/少都算场景坏掉，避免拼错静默失效）。
COMPUTED_DECL_FIELDS = ("derived_from", "delta")

#: `npc_actor` 的退化规则：**规则（部署假设）而非推导**。点错 actor ⇒ 后续步骤响亮超时。
RULES_FALLBACK = {
    "npc_actor": {
        "kind": "rule", "ident": SM_TURN, "field": "recog",
        "rule": "SM_TURN(10) 首个 recog != self（部署假设）",
        "oracle": "src/M2Server/Player/PlayObject.Message.cs:1542 "
                  "MakeMessage(SM_TURN, ActorId, …)（只说明『该 actor 转了身』，不说明它是什么）",
    },
}

#: ---- SM_BAGITEMS(201) 包体布局（oracle 定长记录，见模块 docstring 的表）----
#: `ClientItem.WritePacket`（src/OpenMir2/Packets/ClientPackets/ClientItem.cs）逐字段定长 124B：
#: Name = WriteAsciiString(ItemNameLen=14) ⇒ 1B 长度 + 14B 内容 = 15B。
BAG_RECORD_LEN = 124
BAG_NAME_LEN = 14
BAG_RECORD_NAME_OFF = 0
BAG_RECORD_STDMODE_OFF = 1 + BAG_NAME_LEN   # 15
BAG_RECORD_ANICOUNT_OFF = BAG_RECORD_STDMODE_OFF + 3   # 18（StdMode,Shape,Weight,AniCount）
BAG_RECORD_MAKEINDEX_OFF = 100              # 见 docstring：Name…Reference(15B)…0x00 之后的 i32

#: StdMode -> ItemLocation 槽号（`M2Share.CheckUserItems` 的**逆表**；同 StdMode 命中多槽取最小槽号，
#: 例：24/25/26 同时可进 ArmRingl(5)/ArmRingr(6)（25 还可进 Bujuk(9)）⇒ 一律取 5）。
STD_MODE_TO_WHERE = {
    5: 1, 6: 1,          # Weapon
    10: 0, 11: 0,        # Dress
    15: 4,               # Helmet
    19: 3, 20: 3, 21: 3, # Necklace
    22: 7, 23: 7,        # Ringl（Ringr 同表，取小）
    24: 5, 25: 5, 26: 5, # ArmRingl（ArmRingr=6 / Bujuk=9 同表，取小）
    28: 2, 29: 2, 30: 2, # RighThand
    51: 9,               # Bujuk
    52: 11, 62: 11,      # Boots
    53: 12, 63: 12,      # Charm
    54: 10, 64: 10,      # Belt
}

#: `PlayObject.Operate.cs:601-676 ClientUseItems` 的可食用 StdMode 分支（其余恒 SM_EAT_FAIL）。
#: 选择时**不是**简单取首个：按 `_is_script_bundle > _is_bundle > _is_potion > _is_book` 的固定次序
#: （StdMode31/AniCount!=0 的 QFunction 包优先——那是有脚本可判的包）。
BAG_USE_MODES = frozenset((0, 1, 2, 3, 4, 31))

#: 201 类变量：字段 + 规则文案 + oracle 出处（写进输出，便于复审复算）。
BAG_VARS = {
    "make_index": {
        "field": "make_index", "wire_field": "MakeIndex",
        "rule": "SM_BAGITEMS(201) 首条帧里第 %d 槽的 MakeIndex（select=%s）",
        "oracle": "src/M2Server/Player/PlayObject.Operate.cs:38-64 ClientQueryBagItems()"
                  " → EDCode.EncodeBuffer(ClientItem)，ClientItem.WritePacket 的 MakeIndex@100(i32 LE)",
    },
    "item_name": {
        "field": "name", "wire_field": "Name",
        "rule": "SM_BAGITEMS(201) 首条帧里第 %d 槽的 Name（select=%s）",
        "oracle": "同上：ClientItem.Item.Name = CustomItemSystem.GetItemName(userItem)"
                  "（与 ClientTakeOnItems/ClientUseItems 匹配用的名字同源）⇒ **帧里就有**，不需查 StdItems 表",
    },
    "takeon_where": {
        "field": "std_mode", "wire_field": "StdMode",
        "rule": "SM_BAGITEMS(201) 首条帧里第 %d 槽的 StdMode=%d → ItemLocation 槽号（select=%s）",
        "oracle": "src/M2Server/M2Share.cs:376-458 CheckUserItems(btWhere, StdItem) 的逆表；"
                  "btWhere 即 CM_TAKEONITEM 的 nParam2"
                  "（src/M2Server/Player/PlayObject.Message.cs:820-821）",
    },
}

#: 可推导变量「这次为什么没推出来」的说明（single-derive 输出的 reasons）
REASONS = {
    "merchant_actor": "需 SM_MERCHANTSAY(643)：本 run 帧里没有 643（登录期收帧不含点击应答；"
                      "discovery run 必须点一次 NPC）",
    "npc_actor": "帧里既无 SM_MERCHANTSAY(643) 也无非自身的 SM_TURN(10)",
    "king_actor": "需 SM_MERCHANTSAY(643)：本 run 帧里没有 643（discovery run 必须点**国王**）",
    "move_x": "computed 类：由场景 computed.derived_from 指定的基准量 + delta 计算；"
              "基准量推不出或场景未声明 computed ⇒ 连锁 fail-closed",
    "move_y": "同上（computed 类；基准量推不出或未声明 ⇒ 连锁 fail-closed）",
    "make_index": "需 SM_BAGITEMS(201)：本 run 帧里没有 201（oracle 只在背包非空时发 201，"
                  "见 PlayObject.Operate.cs:58-62；也可能没有可装备/可食用的候选槽）",
    "item_name": "同上（SM_BAGITEMS(201) 的 Name 字段；不需查 StdItems 表）",
    "takeon_where": "同上（由 201 的 StdMode 经 M2Share.CheckUserItems 逆表映射到 ItemLocation）",
}

#: 明示「不可推导」的变量，命中即 missing（有据可查，避免有人以为漏实现）
UNDERIVABLE = {
    "target_x": "目标实体**未知**：位置本身有来源（`SM_TURN(10).param/tag` 报的是转身者自己的坐标），"
                "缺的是「哪一个实体是目标 + 它的对象类型（怪物/NPC）」——这两项当前没有任何帧给出",
    "target_y": "同上（缺「目标实体是谁 + 对象类型」，不是缺坐标来源）",
    "hit_dir": "无来源帧（需操作者显式 --var）；即使有目标坐标，SM_TURN(10).param/tag 也只报"
               "**转身者**自己的位置，且不区分怪物/NPC",
}


class DiscoveryError(Exception):
    """输入坏掉（坏行/非对象/字段非整数）⇒ 必须是干净的 rc=2，不是 traceback。"""


def load_frames(path):
    """读 frames.ndjson；坏行/非对象/读不开一律 DiscoveryError（带 file:lineno）。"""
    frames = []
    try:
        with open(path, "r", encoding="utf-8") as fh:
            for lineno, line in enumerate(fh, 1):
                line = line.strip()
                if not line:
                    continue
                try:
                    obj = json.loads(line)
                except ValueError as ex:
                    raise DiscoveryError("%s:%d: invalid JSON: %s" % (path, lineno, ex))
                if not isinstance(obj, dict):
                    raise DiscoveryError("%s:%d: frame must be a JSON object, got %s"
                                         % (path, lineno, type(obj).__name__))
                frames.append(obj)
    except OSError as ex:
        raise DiscoveryError("%s: cannot read frames: %s" % (path, ex))
    return frames


def _first(frames, ident, skip_recog=None):
    for frame in frames:
        if frame.get("ident") != ident:
            continue
        if skip_recog is not None and frame.get("recog") == skip_recog:
            continue
        return frame
    return None


def _value(frame, var, rule, field):
    """取字段并转 int；缺失 ⇒ None（调用方记 missing），非法值 ⇒ DiscoveryError。"""
    if frame is None:
        return None
    raw = frame.get(field)
    if raw is None:
        return None
    try:
        return int(raw)
    except (TypeError, ValueError):
        raise DiscoveryError("%s: %s.%s is not an integer: %r"
                             % (var, rule["rule"], field, raw))


def validate_computed(computed, required, where="computed"):
    """校验场景的 `computed` 声明；坏声明 ⇒ DiscoveryError（**场景坏掉**，不是「缺变量」）。

    口径（见模块 docstring）：
      - `computed` 必须是对象；键非空字符串，且**必须在 `required` 里**；
      - 每条声明必须恰好有 `derived_from`（非空字符串）与 `delta`（int，`bool` 不算）；
      - `derived_from` 不得指向另一个 `computed` 变量（不支持链式，避免环）。
    返回规整后的 {var: {derived_from, delta}}。
    """
    if computed is None:
        return {}
    if not isinstance(computed, dict):
        raise DiscoveryError("%s: scenario.computed must be an object, got %r"
                             % (where, computed))
    req = set(required or [])
    out = {}
    for var, decl in computed.items():
        if not isinstance(var, str) or not var:
            raise DiscoveryError("%s: scenario.computed key must be a non-empty string, got %r"
                                 % (where, var))
        if var not in req:
            raise DiscoveryError("%s: scenario.computed.%s is not declared in required %s"
                                 "（声明了却不用 = 场景坏掉）" % (where, var, sorted(req)))
        if not isinstance(decl, dict):
            raise DiscoveryError("%s: scenario.computed.%s must be an object, got %r"
                                 % (where, var, decl))
        missing = [k for k in COMPUTED_DECL_FIELDS if k not in decl]
        unknown = sorted(set(decl) - set(COMPUTED_DECL_FIELDS))
        if missing:
            raise DiscoveryError("%s: scenario.computed.%s missing %s（computed 必须显式声明 delta）"
                                 % (where, var, missing))
        if unknown:
            raise DiscoveryError("%s: scenario.computed.%s has unknown key(s) %s (fail-closed)"
                                 % (where, var, unknown))
        base = decl["derived_from"]
        if not isinstance(base, str) or not base:
            raise DiscoveryError("%s: scenario.computed.%s.derived_from must be a non-empty string,"
                                 " got %r" % (where, var, base))
        if base in computed:
            raise DiscoveryError("%s: scenario.computed.%s.derived_from=%r 指向另一个 computed 变量"
                                 "（不支持链式，避免环）" % (where, var, base))
        delta = decl["delta"]
        if not isinstance(delta, int) or isinstance(delta, bool):
            raise DiscoveryError("%s: scenario.computed.%s.delta must be an int, got %r"
                                 % (where, var, delta))
        out[var] = {"derived_from": base, "delta": delta}
    return out


def _bag_record(raw, slot):
    """把一条 124B ClientItem 记录解成 {slot,name,std_mode,make_index}；坏字段 ⇒ DiscoveryError。"""
    name_len = raw[BAG_RECORD_NAME_OFF]
    if name_len > BAG_NAME_LEN:
        raise DiscoveryError("SM_BAGITEMS(201): slot %d name length %d > %d（定长 14）"
                             % (slot, name_len, BAG_NAME_LEN))
    name_bytes = raw[BAG_RECORD_NAME_OFF + 1:BAG_RECORD_NAME_OFF + 1 + name_len]
    try:
        name = name_bytes.decode("gbk")
    except UnicodeDecodeError as ex:
        raise DiscoveryError("SM_BAGITEMS(201): slot %d Name is not valid GBK: %s" % (slot, ex))
    if not name:
        raise DiscoveryError("SM_BAGITEMS(201): slot %d has empty Name（空槽不该出现在包体里）" % slot)
    return {
        "slot": slot,
        "name": name,
        "std_mode": raw[BAG_RECORD_STDMODE_OFF],
        "ani_count": raw[BAG_RECORD_ANICOUNT_OFF],
        "make_index": int.from_bytes(
            raw[BAG_RECORD_MAKEINDEX_OFF:BAG_RECORD_MAKEINDEX_OFF + 4], "little", signed=True),
    }


def _parse_bagitems(frame):
    """首条 SM_BAGITEMS(201) 的包体 -> 有序物品槽（槽序 = oracle ItemList 序）。

    口径（oracle `ClientQueryBagItems`）：包体 = 逐项 `EDCode.EncodeBuffer(ClientItem)` + `'/'`。
    坏数据一律 DiscoveryError（fail-closed）：不许「跳过坏槽继续取」。
    """
    raw_hex = frame.get("body_hex")
    if not isinstance(raw_hex, str):
        raise DiscoveryError("SM_BAGITEMS(201).body_hex missing (seq=%r)" % (frame.get("seq"),))
    if not raw_hex:
        # 空包体：只有 header series=0 才合法（空背包）；**不是** error，由调用方记 missing。
        series = frame.get("series")
        if isinstance(series, int) and not isinstance(series, bool) and series != 0:
            raise DiscoveryError("SM_BAGITEMS(201): body 为空但 header series=%d（fail-closed）"
                                 % series)
        return []
    try:
        body = bytes.fromhex(raw_hex)
    except ValueError as ex:
        raise DiscoveryError("SM_BAGITEMS(201).body_hex is not valid hex: %s" % ex)
    segments = body.split(b"/")
    if segments and segments[-1] == b"":            # oracle 每条记录后都追加一个 '/'
        segments.pop()
    records = []
    for i, seg in enumerate(segments):
        if not seg:
            raise DiscoveryError("SM_BAGITEMS(201): empty segment at slot %d（'/' 连写）" % i)
        try:
            raw = mir2frame.decode_bytes(seg, strict=True)
        except mir2frame.FrameError as ex:
            raise DiscoveryError("SM_BAGITEMS(201): slot %d EDCode decode failed: %s" % (i, ex))
        if len(raw) != BAG_RECORD_LEN:
            raise DiscoveryError(
                "SM_BAGITEMS(201): slot %d decoded to %d bytes, expected %d"
                "（ClientItem.WritePacket 定长 124B；布局变了就 fail-closed）"
                % (i, len(raw), BAG_RECORD_LEN))
        records.append(_bag_record(raw, i))
    series = frame.get("series")
    if isinstance(series, int) and not isinstance(series, bool) and series != len(records):
        # 帧头 series = ItemList.Count；与包体槽数不符说明包体不完整（oracle 会跳过 StdItem 缺失的项）
        raise DiscoveryError("SM_BAGITEMS(201): header series=%d 与包体槽数 %d 不符（fail-closed）"
                             % (series, len(records)))
    return records


def _is_script_bundle(rec):
    """StdMode 31 且 AniCount != 0：`ClientUseItems` 走 `UseStdModeFunItem` ⇒ 先跑
    `@StdModeFunc<AniCount>` 脚本再移物品（exec-seams ② 路的判据包）。"""
    return rec["std_mode"] == 31 and rec["ani_count"] != 0


def _is_bundle(rec):
    """StdMode 31 且 AniCount == 0：解包成 6 件（`ClientUseItemsGetUnBindItems`）。"""
    return rec["std_mode"] == 31 and rec["ani_count"] == 0


def _is_potion(rec):
    return rec["std_mode"] in (0, 1, 2, 3)      # ClientUseItems: 药


def _is_book(rec):
    return rec["std_mode"] == 4                 # ClientUseItems: 书（ReadBook）


def _bag_select(records, required):
    """确定性地选**一个**物品槽：`required` 含 takeon_where ⇒ 首个可装备槽；
    否则按固定价值次序取首个可食用槽（QFunction 包 → 解包 → 药 → 书，见下方注释）。

    返回 (slot_index, record, mode)；无候选 ⇒ (None, None, mode)。**绝不随机、绝不取默认值**。
    """
    mode = "equip" if "takeon_where" in (required or []) else "use"
    if mode == "equip":
        for idx, rec in enumerate(records):
            if rec["std_mode"] in STD_MODE_TO_WHERE:
                return idx, rec, mode
        return None, None, mode
    # use：按 oracle 的**分支价值**排序（固定次序、确定性；同档内取槽序最小者）：
    #   1) StdMode 31 且 AniCount != 0 ⇒ QFunction `@StdModeFunc<AniCount>` 有脚本
    #      （`PlayObject.Base.cs:1072 UseStdModeFunItem`，调用点 `Operate.cs:668`；exec-seams ② 路要的正是这种包）
    #   2) StdMode 31 且 AniCount == 0 ⇒ 解包物品（ClientUseItemsGetUnBindItems）
    #   3) StdMode 0/1/2/3（药）  4) StdMode 4（书）
    for pred in (_is_script_bundle, _is_bundle, _is_potion, _is_book):
        for idx, rec in enumerate(records):
            if pred(rec):
                return idx, rec, mode
    return None, None, mode


def _derive_bag_var(var, bag_pick, field_override=None):
    """201 类变量（make_index / item_name / takeon_where）都来自**同一个**被选中的槽。"""
    if bag_pick is None:
        return None, "no-frame"
    pick, reason = bag_pick()
    if pick is None:
        return None, reason
    slot, rec, mode = pick
    meta = BAG_VARS[var]
    field = field_override or meta["field"]
    if field == "name":
        value = rec["name"]
    elif field == "make_index":
        value = rec["make_index"]
    elif field == "std_mode":
        value = STD_MODE_TO_WHERE.get(rec["std_mode"]) if var == "takeon_where" else rec["std_mode"]
        if value is None:                          # 只在牙（field_override）路径可达
            return None, "no-candidate"
    else:
        raise DiscoveryError("%s: unknown bag field %r（牙只允许 name/std_mode/make_index）"
                             % (var, field))
    if var == "takeon_where":
        rule = meta["rule"] % (slot, rec["std_mode"], mode)
        wire_field = "StdMode→ItemLocation"
    else:
        rule = meta["rule"] % (slot, mode)
        wire_field = meta["wire_field"]
    return {"value": value, "kind": "derived", "rule": rule, "oracle": meta["oracle"],
            "source": {"ident": SM_BAGITEMS, "field": wire_field,
                       "seq": None, "slot": slot, "std_mode": rec["std_mode"],
                       "select": mode}}, None


def _derive_from_rules(var, login, said, turned, overrides, bag_pick=None):
    """按 RULES / RULES_FALLBACK / SM_BAGITEMS(201) 推导单个变量；推不出返回 (None, reason)。"""
    if var in BAG_VARS:
        return _derive_bag_var(var, bag_pick, overrides.get(var))
    if var in UNDERIVABLE:
        return None, "underivable"
    if var == "npc_actor" and said is None:
        rule = RULES_FALLBACK["npc_actor"]
        frame = turned
    else:
        rule = RULES.get(var)
        if rule is None:                           # 规则表里没有 ⇒ fail-closed（不猜）
            return None, "no-rule"
        frame = {SM_LOGON: login, SM_MERCHANTSAY: said}.get(rule["ident"])
    field = overrides.get(var, rule["field"])
    value = _value(frame, var, rule, field)
    if value is None:
        return None, "no-value"
    return {"value": value, "kind": rule["kind"], "rule": rule["rule"],
            "oracle": rule["oracle"],
            "source": {"ident": frame.get("ident"), "field": field,
                       "seq": frame.get("seq")}}, None


def derive(frames, required, field_map=None, computed=None):
    """按规则推导 required 变量。返回 (values, missing)；不做任何兜底。

    `field_map` 只用于牙（把某个变量故意指到别的字段）；正常调用不传。
    `computed` 是场景 `computed` 声明（见 `validate_computed`）；不传 ⇒ 这类变量照旧 missing，
    **绝不猜 delta**（单变量推导入口就是这条路径）。

    两遍：先按 `required` 顺序推导非 computed 变量，再按 `required` 顺序算 computed
    （基准量即使不在 `required` 里也会先推导）；`missing` 始终按 `required` 顺序输出。
    """
    values, missing = {}, []
    decls = validate_computed(computed, required)
    login = _first(frames, SM_LOGON)
    self_actor = login.get("recog") if login else None
    said = _first(frames, SM_MERCHANTSAY)          # 真的应答过点击的 NPC（实证来源）
    turned = None
    if self_actor is not None:
        turned = _first(frames, SM_TURN, skip_recog=self_actor)  # 退化规则来源

    overrides = dict(field_map or {})
    memo = {}

    # 201 上下文：**惰性**解析（没有变量需要它时，坏 201 不许影响别的场景），整次 derive 只选一次槽，
    # 保证 make_index / item_name / takeon_where 指向**同一个**物品。
    bag_state = {}

    def bag_pick():
        if "pick" in bag_state:
            return bag_state["pick"]
        frame = _first(frames, SM_BAGITEMS)
        if frame is None:
            result = (None, "no-frame")
        else:
            bag_state["frame"] = frame
            records = _parse_bagitems(frame)
            if not records:
                result = (None, "empty")
            else:
                idx, rec, mode = _bag_select(records, required)
                result = ((idx, rec, mode), None) if rec is not None \
                    else (None, "no-candidate:" + mode)
        bag_state["pick"] = result
        return result

    def base(var):
        """推导任意变量（含不在 required 里的基准量）；同一次 derive 内只算一次。"""
        if var not in memo:
            memo[var] = _derive_from_rules(var, login, said, turned, overrides, bag_pick)
        return memo[var]

    entries = {}
    for var in required:                           # 第一遍：非 computed
        if var in decls:
            continue
        entry, _ = base(var)
        entries[var] = entry
    for var in required:                           # 第二遍：computed（依已推导量）
        decl = decls.get(var)
        if decl is None:
            continue
        bentry, _ = base(decl["derived_from"])
        if bentry is None:                         # 基准量推不出 ⇒ 连锁 fail-closed
            entries[var] = None
            continue
        entries[var] = {
            "value": bentry["value"] + decl["delta"],
            "kind": "computed",
            "rule": "%s %+d（场景 computed 声明）" % (decl["derived_from"], decl["delta"]),
            "oracle": COMPUTED_ORACLES.get(
                var,
                "场景 JSON 的 computed 声明 + 已推导量（确定性算术，非猜值）"),
            "source": {"kind": "computed",
                       "computed_from": decl["derived_from"],
                       "delta": decl["delta"],
                       "base_value": bentry["value"],
                       "base_source": bentry["source"]},
        }
    bag_frame = bag_state.get("frame")
    for var in required:                           # 输出顺序 = required 顺序
        entry = entries.get(var)
        if entry is not None and entry.get("source", {}).get("ident") == SM_BAGITEMS:
            entry["source"]["seq"] = bag_frame.get("seq") if bag_frame else None
        if entry is None:
            missing.append(var)
        else:
            values[var] = entries[var]
    return values, missing


def render_vars(values):
    """-> ['--var', 'k=v', ...]（按变量名排序，保证确定性）。

    值可以是 int（坐标/ActorId/槽号）或 str（`item_name`）；两者都按 `NAME=VALUE` 原样交给
    `mir2_probe --var`（纯数字的物品名会被 probe 读回 int，`text_from` 仍按 str 处理，语义不变）。
    """
    out = []
    for key in sorted(values):
        out += ["--var", "%s=%s" % (key, values[key]["value"])]
    return out


#: 分类；`params_default` = 绿来自场景 params 硬编码默认值，**不是**发现值；
#: `rule` = required 里有变量来自退化规则（部署假设），**不计入**「发现能力已验证」；
#: `computed` = required 里有变量来自场景 `computed` 声明（场景声明 + 已推导量的算术），
#: **同样不计入**「发现能力已验证」（另立一栏，绝不混算成 derived）。
CLS_DERIVED = "derived"
CLS_RULE = "rule"
CLS_COMPUTED = "computed"
CLS_NO_REQUIRED = "no_required"
CLS_PARAMS_DEFAULT = "params_default"
CLS_MISSING = "missing"
CLS_ERROR = "error"


def _load_scenario(path):
    """读并校验场景文件；坏 JSON / 非对象 / required 不是非空字符串数组 ⇒ DiscoveryError。

    场景文件是 CLI 可指定的输入（可传自定义 scenarios_dir），与 frames 坏行同口径：
    必须是干净的 rc=2，不许 traceback / rc=1，也不许把 `required: "npc_actor"` 按字符拆开。
    """
    try:
        with open(path, "r", encoding="utf-8") as fh:
            doc = json.load(fh)
    except (OSError, ValueError) as ex:
        raise DiscoveryError("%s: cannot load scenario: %s" % (path, ex))
    if not isinstance(doc, dict):
        raise DiscoveryError("%s: scenario must be a JSON object, got %s"
                             % (path, type(doc).__name__))
    required = doc.get("required")
    if required is not None:
        if not isinstance(required, list) or not all(
                isinstance(item, str) and item for item in required):
            raise DiscoveryError("%s: scenario.required must be a list of non-empty strings, got %r"
                                 % (path, required))
    params = doc.get("params")
    if params is not None and not isinstance(params, dict):
        raise DiscoveryError("%s: scenario.params must be an object, got %r" % (path, params))
    # computed 声明在加载期就校验：坏声明 ⇒ 场景坏掉（cls=error），不许拖到 derive 才炸/静默缺变量
    validate_computed(doc.get("computed"), required, path)
    return doc


def check_all(frames_path, scenarios_dir):
    """对每个场景：从 frames 推导它的 required ⇒ 拼 --var 跑 `mir2_probe --check-scenario`。

    返回 [(scenario, rc, detail, cls)]；rc 口径：0=通过 / 2=发现不出必需变量或场景校验失败。

    fail-closed 两处：`scenarios_dir` 不是目录 ⇒ DiscoveryError（rc=2）；
    单个场景文件坏掉 ⇒ 该行 cls=error（整体 rc=2，`--allow-missing` 也不放行）。
    「一个场景都没检查」绝不当绿——由 check_all_main 对空 rows 判 rc=2。
    """
    import glob
    import subprocess

    if not os.path.isdir(scenarios_dir):
        raise DiscoveryError("%s: scenarios dir does not exist or is not a directory"
                             % scenarios_dir)
    frames = load_frames(frames_path)
    here = os.path.dirname(os.path.abspath(__file__))
    probe = os.path.join(here, "mir2_probe.py")
    rows = []
    for path in sorted(glob.glob(os.path.join(scenarios_dir, "*.json"))):
        name = os.path.basename(path)
        try:
            scenario = _load_scenario(path)
        except DiscoveryError as ex:
            rows.append((name, 2, "scenario-load: %s" % ex, CLS_ERROR))
            continue
        required = scenario.get("required") or []
        params = scenario.get("params") or {}
        values, missing = derive(frames, required, computed=scenario.get("computed"))
        if missing:
            # 本模块自己兜住 DSL 的 required：这一行必须是 missing（不许交给 mir2_probe 崩）
            rows.append((name, 2, "missing: %s" % ",".join(missing), CLS_MISSING))
            continue
        cmd = [sys.executable, "-I", "-B", probe, "--check-scenario", "--scenario", path]
        cmd += render_vars(values)
        proc = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8")
        detail = (proc.stdout or proc.stderr or "").strip().splitlines()
        detail = detail[-1] if detail else ""
        if proc.returncode == 0:
            if required:
                # 推导/规则/计算三栏：只要有变量来自退化规则，这一行就不算「发现能力已验证」；
                # computed 是「场景声明 + 已推导量的算术」，同样不计入 derived。
                # 优先级 rule > computed > derived（越不确定越靠前）。
                kinds = set(v["kind"] for v in values.values())
                if "rule" in kinds:
                    cls = CLS_RULE
                elif "computed" in kinds:
                    cls = CLS_COMPUTED
                else:
                    cls = CLS_DERIVED
            elif params:
                cls = CLS_PARAMS_DEFAULT
            else:
                cls = CLS_NO_REQUIRED
        else:
            cls = CLS_ERROR
        rows.append((name, proc.returncode, detail, cls))
    return rows


def _parse_check_all_args(args):
    """-> (positional, allow, expect_missing)；用法错误时返回 (None, None, msg)。"""
    allow, expect, rest = False, None, []
    i = 0
    while i < len(args):
        arg = args[i]
        if arg == "--allow-missing":
            allow = True
            if i + 1 < len(args) and args[i + 1].isdigit():
                expect = int(args[i + 1])
                i += 1
        elif arg.startswith("--allow-missing="):
            allow = True
            value = arg.split("=", 1)[1]
            if not value.isdigit():
                return None, None, "--allow-missing=N needs an integer, got %r" % value
            expect = int(value)
        elif arg in ("-h", "--help"):
            return None, None, None
        elif arg.startswith("-") and arg != "-":
            return None, None, "unknown option %r" % arg
        else:
            rest.append(arg)
        i += 1
    return rest, allow, expect


def check_all_main(args):
    parsed, allow, expect = _parse_check_all_args(args)
    if parsed is None:
        if expect is not None:                       # 用法错误
            print("param_discovery: %s" % expect, file=sys.stderr)
            return 2
        print(__doc__)                               # --help
        return 0
    if not parsed:
        print("usage: param_discovery.py check-all <frames.ndjson> [scenarios_dir] "
              "[--allow-missing[=N]]", file=sys.stderr)
        return 2
    scen = parsed[1] if len(parsed) > 1 else os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "scenarios")
    try:
        rows = check_all(parsed[0], scen)
    except DiscoveryError as ex:
        print("param_discovery: %s" % ex, file=sys.stderr)
        return 2
    if not rows:
        # 「一个场景都没检查」绝不当绿（与 goal_gate 缺 counts、recon 唯一 tracked 同族）
        print("param_discovery: no scenarios found under %s — 零场景判绿是 fail-open，rc=2"
              % scen, file=sys.stderr)
        return 2
    for name, rc, detail, cls in rows:
        print("%-28s rc=%d  %-14s %s" % (name, rc, cls, detail[:100]))
    bad = [r for r in rows if r[1] != 0]
    missing = [r for r in rows if r[3] == CLS_MISSING]
    errors = [r for r in rows if r[3] == CLS_ERROR]
    print("SUMMARY scenarios=%d ok=%d rc2=%d derived=%d rule=%d computed=%d no_required=%d "
          "params_default=%d missing=%d error=%d"
          % (len(rows), len(rows) - len(bad), len(bad),
             sum(1 for r in rows if r[3] == CLS_DERIVED),
             sum(1 for r in rows if r[3] == CLS_RULE),
             sum(1 for r in rows if r[3] == CLS_COMPUTED),
             sum(1 for r in rows if r[3] == CLS_NO_REQUIRED),
             sum(1 for r in rows if r[3] == CLS_PARAMS_DEFAULT),
             len(missing), len(errors)))
    if errors:
        print("param_discovery: %d scenario error(s) — 场景本身坏了，--allow-missing 也不放行"
              % len(errors), file=sys.stderr)
        return 2
    if missing and not allow:
        print("param_discovery: %d scenario(s) fail-closed on missing vars "
              "（登记后可 --allow-missing=N 放行）" % len(missing), file=sys.stderr)
        return 2
    if allow and expect is not None and len(missing) != expect:
        print("param_discovery: expected %d missing scenario(s), got %d — 不可推导清单变了"
              % (expect, len(missing)), file=sys.stderr)
        return 2
    return 0


# --------------------------------------------------------------------------
# 构造级用例（≥6）+ 牙（≥2），全部离线
# --------------------------------------------------------------------------
def _f(seq, ident, recog=0, param=0, tag=0):
    return {"seq": seq, "ident": ident, "recog": recog, "param": param, "tag": tag}


def _fixture(with_npc=True, with_login=True, merchant_say=False):
    """`merchant_say=True` 模拟「点过并拿到 643」的 run；默认只有登录期收帧。"""
    frames = []
    if with_login:
        frames.append(_f(1, SM_LOGON, recog=191, param=7, tag=12))
    if with_npc:
        frames.append(_f(2, SM_TURN, recog=191, param=0, tag=0))   # 自己转身：必须排除
        frames.append(_f(3, SM_TURN, recog=119, param=8, tag=10))  # 目标 NPC
    if merchant_say:
        frames.append(_f(4, SM_MERCHANTSAY, recog=119, param=0, tag=0))
    return frames


#: ---- 201 合成帧夹具（构造级：布局逐字段照 ClientItem.WritePacket，见模块 docstring）----
def _bag_record_bytes(name="屠龙", std_mode=5, make_index=123456, ani_count=0):
    """按 `ClientItem.WritePacket` 的定长 124B 布局拼一条记录（未 EDCode 编码）。"""
    import struct
    name_bytes = name.encode("gbk")
    assert len(name_bytes) <= BAG_NAME_LEN, name
    b = bytearray()
    b.append(len(name_bytes))
    b += name_bytes.ljust(BAG_NAME_LEN, b"\x00")          # Name: 1B 长度 + 14B 定长
    b.append(std_mode)                                    # @15 StdMode
    b += bytes([0, 0, ani_count])                         # @16 Shape @17 Weight @18 AniCount
    b += bytes([0, 0, 0])                                 # @19 SpecialPwr @20 ItemDesc @21 0
    b += struct.pack("<H", 0)                             # @22 Looks
    b += struct.pack("<H", 0)                             # @24 DuraMax
    b += struct.pack("<HHHHH", 0, 0, 0, 0, 0)             # @26 AC MAC DC MC SC
    b += bytes([0] * 4)                                   # @36 Need/NeedLevel/NeedIdentify/0
    b += struct.pack("<ii", 0, 0)                         # @40 Price Stock
    b += bytes([0] * 8)                                   # @48 AtkSpd..Undead+2
    b += struct.pack("<iii", 0, 0, 0)                     # @56 HpAdd MpAdd ExpAdd
    b += bytes([0] * 14)                                  # @68 Eff..ItemType+0
    b += struct.pack("<H", 0)                             # @82 ItemSet
    b.append(0)
    b += b"".ljust(BAG_NAME_LEN, b"\x00")                 # @84 Reference: 1B 长度 + 14B
    b.append(0)                                           # @99
    b += struct.pack("<i", make_index)                    # @100 MakeIndex (i32 LE)
    b += struct.pack("<HH", 1000, 1000)                   # @104 Dura @106 DuraMax
    b += bytes(14)                                        # @108 Desc[14]
    b += bytes([0, 0])                                    # @122 @123
    assert len(b) == BAG_RECORD_LEN, len(b)
    return bytes(b)


def _bag_frame(records, seq=9, series=None, body=None):
    """一条 SM_BAGITEMS(201) 合成帧；`body` 可显式覆盖（坏数据牙用）。"""
    payload = b"".join(mir2frame.encode_bytes(r) + b"/" for r in records)
    if body is not None:
        payload = body
    return {"seq": seq, "ident": SM_BAGITEMS, "recog": 191, "param": 0, "tag": 0,
            "series": len(records) if series is None else series,
            "body_hex": payload.hex()}


def _bag_fixture(with_201=True):
    """登录帧 + 一个两物品背包：槽 0 = 金创药(StdMode 0, MakeIndex 9001)、
    槽 1 = 屠龙(StdMode 5, MakeIndex 123456)。用具/可食用分别命中不同槽（用来钉住选择规则）。"""
    frames = _fixture()
    if with_201:
        frames.append(_bag_frame([_bag_record_bytes("金创药", 0, 9001),
                                  _bag_record_bytes("屠龙", 5, 123456)]))
    return frames


def _tmp_check_all(scenarios, frames=None):
    """写临时 frames + 场景目录，返回 check_all 的行（供用例/牙断言行为）。"""
    import tempfile
    tmp = tempfile.mkdtemp(prefix="pd-checkall-")
    frames_path = os.path.join(tmp, "frames.ndjson")
    with open(frames_path, "w", encoding="utf-8") as fh:
        for frame in (frames if frames is not None else _fixture(merchant_say=True)):
            print(json.dumps(frame), file=fh)
    for name, doc in scenarios.items():
        with open(os.path.join(tmp, name), "w", encoding="utf-8") as fh:
            json.dump(doc, fh)
    return dict((r[0], r) for r in check_all(frames_path, tmp))


def _write_tmp_frames(frames):
    """写临时 frames.ndjson，返回路径（坏数据/check-all 接线用）。"""
    import tempfile
    tmp = tempfile.mkdtemp(prefix="pd-frames-")
    path = os.path.join(tmp, "frames.ndjson")
    with open(path, "w", encoding="utf-8") as fh:
        for frame in frames:
            print(json.dumps(frame), file=fh)
    return path


def _quiet_check_all_main(argv):
    """跑 check_all_main 但不刷屏（用例/牙只关心 rc）。"""
    import contextlib
    import io
    with contextlib.redirect_stdout(io.StringIO()), \
            contextlib.redirect_stderr(io.StringIO()):
        return check_all_main(argv)


def selftest():
    fails = []

    def case(name, fn):
        try:
            fn()
            print("  [ok] %s" % name)
        except AssertionError as ex:
            fails.append("%s: %s" % (name, ex))

    def c1():
        v, m = derive(_fixture(merchant_say=True),
                      ["self_x", "self_y", "npc_actor", "merchant_actor"])
        assert not m, m
        assert v["self_x"]["value"] == 7 and v["self_y"]["value"] == 12, v
        assert v["npc_actor"]["value"] == 119 and v["merchant_actor"]["value"] == 119, v
        # 有 643 时 npc_actor 走推导（同 merchant），来源是 643 那帧
        assert v["npc_actor"]["kind"] == "derived", v
        assert v["merchant_actor"]["source"] == {"ident": SM_MERCHANTSAY, "field": "recog",
                                                 "seq": 4}, v
        assert v["self_x"]["source"]["ident"] == SM_LOGON, v
        assert render_vars(v) == ["--var", "merchant_actor=119", "--var", "npc_actor=119",
                                  "--var", "self_x=7", "--var", "self_y=12"], render_vars(v)

    def c2():
        v, m = derive(_fixture(with_npc=False), ["npc_actor"])
        assert m == ["npc_actor"] and not v, (v, m)

    def c3():
        v, m = derive(_fixture(with_login=False), ["self_x", "self_y"])
        assert m == ["self_x", "self_y"] and not v, (v, m)

    def c4():
        # 只有自己的 SM_TURN 时不得把自己当 NPC
        v, m = derive(_fixture(with_npc=False, with_login=True), ["npc_actor"])
        assert m == ["npc_actor"], (v, m)

    def c5():
        # 无 SM_BAGITEMS(201) ⇒ make_index/item_name 恒 missing（201 规则只在帧里有时生效）
        v, m = derive(_fixture(), ["make_index", "item_name"])
        assert m == ["make_index", "item_name"] and not v, (v, m)

    def c6():
        # 确定性：同夹具两次跑同结果
        a = derive(_fixture(), ["self_x", "npc_actor"])
        b = derive(_fixture(), ["self_x", "npc_actor"])
        assert a == b, (a, b)

    def c7():
        # 混入不可推导变量时，可推导的仍给出、缺的逐个点名
        v, m = derive(_fixture(), ["self_x", "king_actor", "target_x"])
        assert v["self_x"]["value"] == 7, v
        assert m == ["king_actor", "target_x"], m

    def c8():
        # check-all 接线口径：可推导场景 rc=0；缺变量场景 rc=2 且**由本模块**报 missing:
        rows = _tmp_check_all({
            "ok.json": {"name": "ok_scen", "required": ["self_x"],
                        "steps": [{"wait_for": 50, "timeout_ms": 10}]},
            "bad.json": {"name": "bad_scen", "required": ["make_index"],
                         "steps": [{"wait_for": 50, "timeout_ms": 10}]},
        })
        assert rows["ok.json"][1] == 0, rows["ok.json"]
        assert rows["ok.json"][3] == CLS_DERIVED, rows["ok.json"]
        assert rows["bad.json"][1] == 2, rows["bad.json"]
        assert rows["bad.json"][3] == CLS_MISSING, rows["bad.json"]
        assert rows["bad.json"][2].startswith("missing:"), rows["bad.json"]

    def c9():
        # 分类诚实：只有 params 默认值的场景（dice_chain 形态）算 params_default，不算发现绿
        rows = _tmp_check_all({
            "diceish.json": {"name": "diceish", "params": {"npc_actor": 119},
                             "steps": [{"wait_for": 50, "timeout_ms": 10}]},
            "plain.json": {"name": "plain",
                           "steps": [{"wait_for": 50, "timeout_ms": 10}]},
        })
        assert rows["diceish.json"][1] == 0 and rows["diceish.json"][3] == CLS_PARAMS_DEFAULT, \
            rows["diceish.json"]
        assert rows["plain.json"][3] == CLS_NO_REQUIRED, rows["plain.json"]

    def c10():
        # --allow-missing：只放行 missing；坏场景(error)仍 rc=2；钉住的 N 不符也 rc=2
        import tempfile
        tmp = tempfile.mkdtemp(prefix="pd-allowmiss-")
        frames_path = os.path.join(tmp, "frames.ndjson")
        with open(frames_path, "w", encoding="utf-8") as fh:
            for frame in _fixture():
                print(json.dumps(frame), file=fh)
        scen = os.path.join(tmp, "scen")
        os.makedirs(scen)
        docs = {
            "ok.json": {"name": "ok", "required": ["self_x"],
                        "steps": [{"wait_for": 50, "timeout_ms": 10}]},
            "miss.json": {"name": "miss", "required": ["make_index"],
                          "steps": [{"wait_for": 50, "timeout_ms": 10}]},
            "broken.json": {"name": "broken", "required": ["self_x"],
                            "steps": [{"no_such_step": 1}]},
        }
        for name, doc in docs.items():
            with open(os.path.join(scen, name), "w", encoding="utf-8") as fh:
                json.dump(doc, fh)
        base = [frames_path, scen]                       # check_all_main 的参数不含 "check-all"

        def quiet(argv):
            return _quiet_check_all_main(argv)

        assert quiet(base) == 2, "默认必须 fail-closed"
        assert quiet(base + ["--allow-missing"]) == 2, "error 行不许被放行"
        assert quiet(base + ["--allow-missing=2"]) == 2, "实际 missing=1，钉 2 必须红"
        os.remove(os.path.join(scen, "broken.json"))     # 去掉坏场景后
        assert quiet(base + ["--allow-missing=1"]) == 0, "登记 1 条缺失应放行"
        assert quiet(base + ["--allow-missing=2"]) == 2, "钉住的 N 不符必须红"

    def c11():
        # 坏行必须干净 rc=2（不能 traceback / rc=1）
        import tempfile
        tmp = tempfile.mkdtemp(prefix="pd-badline-")
        bad = os.path.join(tmp, "frames.ndjson")
        with open(bad, "w", encoding="utf-8") as fh:
            fh.write('{"seq": 1, "ident": 50}\n')
            fh.write('{"seq": 2, "ident": 10, oops\n')
        try:
            load_frames(bad)
        except DiscoveryError as ex:
            assert "%s:2" % bad in str(ex), ex
        else:
            raise AssertionError("坏行必须抛 DiscoveryError")

    def c13():
        # computed 正例：值 = 已推导量 + 场景声明 delta；kind/source 与 derived/rule 分开
        frames = _fixture()                         # SM_LOGON recog=191 param=7 tag=12
        decls = {"move_x": {"derived_from": "self_x", "delta": 1},
                 "move_y": {"derived_from": "self_y", "delta": 0}}
        v, m = derive(frames, ["move_x", "move_y"], computed=decls)
        assert not m, m
        assert v["move_x"]["value"] == 8 and v["move_y"]["value"] == 12, v
        assert v["move_x"]["kind"] == "computed", v
        assert v["move_x"]["source"]["kind"] == "computed", v
        assert v["move_x"]["source"]["computed_from"] == "self_x", v
        assert v["move_x"]["source"]["delta"] == 1, v
        assert v["move_x"]["source"]["base_value"] == 7, v
        # 基准量的来源帧要能一路追到 SM_LOGON
        assert v["move_x"]["source"]["base_source"] == {"ident": SM_LOGON, "field": "param",
                                                        "seq": 1}, v
        assert render_vars(v) == ["--var", "move_x=8", "--var", "move_y=12"], render_vars(v)

    def c14():
        # delta 未声明 ⇒ fail-closed（场景坏掉，不是「缺变量」）：derive 与 check-all 两条路径都红
        frames = _fixture()
        try:
            derive(frames, ["move_x"], computed={"move_x": {"derived_from": "self_x"}})
        except DiscoveryError as ex:
            assert "delta" in str(ex), ex
        else:
            raise AssertionError("computed 未声明 delta 必须抛 DiscoveryError")
        rows = _tmp_check_all({
            "nodelta.json": {"name": "nodelta", "required": ["move_x"],
                             "computed": {"move_x": {"derived_from": "self_x"}},
                             "steps": [{"wait_for": 50, "timeout_ms": 10}]},
        })
        row = rows["nodelta.json"]
        assert row[1] == 2 and row[3] == CLS_ERROR, row
        assert row[2].startswith("scenario-load:"), row

    def c15():
        # 基准量推不出 ⇒ 连锁 fail-closed（missing，不是 error；也不许拿 0 顶替）
        frames = _fixture(with_login=False)         # 没有 SM_LOGON ⇒ self_x/self_y 推不出
        decls = {"move_x": {"derived_from": "self_x", "delta": 1},
                 "move_y": {"derived_from": "self_y", "delta": 0}}
        v, m = derive(frames, ["move_x", "move_y"], computed=decls)
        assert v == {} and m == ["move_x", "move_y"], (v, m)
        rows = _tmp_check_all({
            "chain.json": {"name": "chain", "required": ["move_x", "move_y"],
                           "computed": decls,
                           "steps": [{"wait_for": 50, "timeout_ms": 10}]},
        }, frames=frames)
        row = rows["chain.json"]
        assert row[1] == 2 and row[3] == CLS_MISSING, row
        assert row[2] == "missing: move_x,move_y", row

    def c16():
        # kind 分类：computed 行必须是 cls=computed，**不得**混算成 derived；
        # 含退化规则的仍优先记 rule（优先级 rule > computed > derived）。
        rows = _tmp_check_all({
            "mv.json": {"name": "mv", "required": ["move_x"],
                        "computed": {"move_x": {"derived_from": "self_x", "delta": 1}},
                        "steps": [{"wait_for": 50, "timeout_ms": 10}]},
        })
        assert rows["mv.json"][1] == 0, rows["mv.json"]
        assert rows["mv.json"][3] == CLS_COMPUTED, rows["mv.json"]
        assert rows["mv.json"][3] != CLS_DERIVED, rows["mv.json"]

    def c17():
        # computed 的边界声明一律场景坏掉：键不在 required / 未知键 / delta 是 bool / 链式
        frames = _fixture()
        bad = [
            {"x": {"derived_from": "self_x", "delta": 1}},                        # 不在 required
            {"self_x": {"derived_from": "self_y", "delta": 1, "note": "x"}},      # 未知键
            {"move_x": {"derived_from": "self_x", "delta": True}},                # bool 不是 int
            {"move_x": {"derived_from": "move_y", "delta": 1},
             "move_y": {"derived_from": "self_y", "delta": 0}},                    # 链式
        ]
        for decl in bad:
            req = ["move_x"] if "move_x" in decl else ["self_x"]
            try:
                derive(frames, req, computed=decl)
            except DiscoveryError:
                pass
            else:
                raise AssertionError("坏 computed 声明必须抛 DiscoveryError：%r" % (decl,))

    def c18():
        # king_actor 归 rule（部署假设），不是 derived：643 不证明「这个 actor 是国王」
        # （仓库全部 643 录制都是赌场庄家）。有 643 ⇒ 给值但 kind=rule、场景 cls=rule；
        # 无 643 ⇒ missing（fail-closed）。
        v, m = derive(_fixture(merchant_say=True), ["king_actor"])
        assert not m and v["king_actor"]["kind"] == "rule", (v, m)
        assert v["king_actor"]["value"] == 119, v
        v, m = derive(_fixture(), ["king_actor"])
        assert m == ["king_actor"] and not v, (v, m)
        rows = _tmp_check_all({
            "king.json": {"name": "king", "required": ["king_actor"],
                          "steps": [{"wait_for": 643, "timeout_ms": 10}]},
        }, frames=_fixture(merchant_say=True))
        assert rows["king.json"][1] == 0, rows["king.json"]
        assert rows["king.json"][3] == CLS_RULE, rows["king.json"]
        assert rows["king.json"][3] != CLS_DERIVED, "king_actor 不得计入 derived"

    def tooth1():
        # 牙①：把 self_x 错取成 tag ⇒ 值变成 12，与真值 7 不同（用例会红）
        good, _ = derive(_fixture(), ["self_x"])
        bad, _ = derive(_fixture(), ["self_x"], field_map={"self_x": "tag"})
        assert good["self_x"]["value"] == 7, good
        assert bad["self_x"]["value"] == 12, bad
        assert bad["self_x"]["value"] != good["self_x"]["value"]

    def tooth2():
        # 牙②：check_all 必须**自己**在 missing 时拦下（cls=missing、detail 以 missing: 开头），
        # 而不是静默交给 mir2_probe 兜底。把 check_all 的 `if missing:` 改成 `if False:` ⇒
        # 这一行会变成 cls=error / detail 以 scenario-error: 开头 ⇒ 下面断言红（reviewer 的变异 M2）。
        rows = _tmp_check_all({
            "only.json": {"name": "only", "required": ["make_index"],
                          "steps": [{"wait_for": 50, "timeout_ms": 10}]},
        })
        row = rows["only.json"]
        assert row[3] == CLS_MISSING, "check_all 没自己拦 missing（cls=%s）" % row[3]
        assert row[2].startswith("missing:"), "detail 不是本模块的 %r" % row[2]

    def tooth3():
        # 牙③（reviewer blocking-1 的回归牙）：只有「第三方 actor 转身」、没有 643 的帧里，
        # merchant_actor 必须 fail-closed——任意第三方 actor 不得被当商人（旧规则此处 rc=0）。
        frames = [_f(1, SM_LOGON, recog=191, param=7, tag=12),
                  _f(2, SM_TURN, recog=191, param=0, tag=0),
                  _f(3, SM_TURN, recog=777, param=1, tag=1)]
        v, m = derive(frames, ["merchant_actor"])
        assert m == ["merchant_actor"] and not v, (v, m)
        rows = _tmp_check_all({
            "merchantish.json": {"name": "merchantish", "required": ["merchant_actor"],
                                 "steps": [{"wait_for": 643, "timeout_ms": 10}]},
        }, frames=frames)
        assert rows["merchantish.json"][1] == 2, rows["merchantish.json"]
        assert rows["merchantish.json"][3] == CLS_MISSING, rows["merchantish.json"]

    def c12():
        # 单场景推导路径的空判据 fail-open：一个变量都没要求 ⇒ rc=2（不许「什么都没推」判绿）。
        # 空 frames ✅ 已由「required 全部 missing ⇒ rc=2」兜住；这条补的是 required 本身为空。
        import tempfile
        tmp = tempfile.mkdtemp(prefix="pd-novars-")
        frames_path = os.path.join(tmp, "frames.ndjson")
        with open(frames_path, "w", encoding="utf-8") as fh:
            for frame in _fixture():
                print(json.dumps(frame), file=fh)
        import contextlib
        import io

        def quiet(argv):
            with contextlib.redirect_stdout(io.StringIO()), \
                    contextlib.redirect_stderr(io.StringIO()):
                return main(argv)

        assert quiet([frames_path, ""]) == 2, "空 --var 列表必须 rc=2"
        assert quiet([frames_path, " , "]) == 2, "只有分隔符/空白的 --var 列表必须 rc=2"
        # 空 frames + 有 required ⇒ missing ⇒ rc=2（回归：这条以前也是对的）
        empty = os.path.join(tmp, "empty.ndjson")
        open(empty, "w").close()
        assert quiet([empty, "self_x"]) == 2, "空 frames + required 必须 rc=2"

    def tooth4():
        # 牙④（reviewer C-1）：零场景判绿是 fail-open。
        # 变异：删掉 check_all_main 里 `if not rows: return 2` 与 check_all 的 isdir 检查 ⇒ 红。
        import tempfile
        tmp = tempfile.mkdtemp(prefix="pd-zeroscen-")
        frames_path = os.path.join(tmp, "frames.ndjson")
        with open(frames_path, "w", encoding="utf-8") as fh:
            for frame in _fixture():
                print(json.dumps(frame), file=fh)
        empty_dir = os.path.join(tmp, "empty-scenarios")
        os.makedirs(empty_dir)
        missing_dir = os.path.join(tmp, "no-such-dir")
        assert _quiet_check_all_main([frames_path, empty_dir]) == 2, "空目录（零场景）必须 rc=2"
        assert _quiet_check_all_main([frames_path, missing_dir]) == 2, "不存在的目录必须 rc=2"
        assert _quiet_check_all_main([frames_path, empty_dir, "--allow-missing"]) == 2, \
            "--allow-missing 也不许放行零场景"
        assert _quiet_check_all_main([frames_path, missing_dir, "--allow-missing=0"]) == 2, \
            "--allow-missing=0 也不许放行零场景"

    def tooth5():
        # 牙⑤（reviewer C-2）：场景文件坏掉必须干净落 cls=error / rc=2。
        # 变异：去掉 _load_scenario 的 try/except 与类型校验 ⇒ traceback rc=1 / detail 被按字符拆开 ⇒ 红。
        import tempfile
        tmp = tempfile.mkdtemp(prefix="pd-badscen-")
        frames_path = os.path.join(tmp, "frames.ndjson")
        with open(frames_path, "w", encoding="utf-8") as fh:
            for frame in _fixture():
                print(json.dumps(frame), file=fh)
        scen = os.path.join(tmp, "scen")
        os.makedirs(scen)
        cases = {
            "badjson.json": '{"name": "x", "steps": [\n',
            "array.json": '[1, 2, 3]\n',
            "reqstr.json": '{"name": "x", "required": "npc_actor", '
                           '"steps": [{"wait_for": 643, "timeout_ms": 10}]}\n',
            "reqint.json": '{"name": "x", "required": [123], '
                           '"steps": [{"wait_for": 643, "timeout_ms": 10}]}\n',
        }
        for name, text in cases.items():
            with open(os.path.join(scen, name), "w", encoding="utf-8") as fh:
                fh.write(text)
        rows = dict((r[0], r) for r in check_all(frames_path, scen))
        for name in cases:
            assert name in rows, rows
            assert rows[name][1] == 2, (name, rows[name])
            assert rows[name][3] == CLS_ERROR, (name, rows[name])
            assert rows[name][2].startswith("scenario-load:"), (name, rows[name])
        assert _quiet_check_all_main([frames_path, scen]) == 2, "坏场景整体必须 rc=2"
        assert _quiet_check_all_main([frames_path, scen, "--allow-missing"]) == 2, \
            "--allow-missing 不许放行 error 行"

    def tooth6():
        # 牙⑥（reviewer A-1）：required 里有任一变量来自退化规则 ⇒ cls=rule，不计入 derived。
        # 变异：check_all 里一律 cls=CLS_DERIVED ⇒ 本断言红。
        rows = _tmp_check_all({
            "npcish.json": {"name": "npcish", "required": ["npc_actor"],
                            "steps": [{"wait_for": 643, "timeout_ms": 10}]},
        }, frames=_fixture())                        # 无 643 ⇒ npc_actor 走 SM_TURN 规则
        assert rows["npcish.json"][1] == 0, rows["npcish.json"]
        assert rows["npcish.json"][3] == CLS_RULE, rows["npcish.json"]
        rows = _tmp_check_all({                      # 有 643 ⇒ npc_actor 走推导
            "npcish.json": {"name": "npcish", "required": ["npc_actor"],
                            "steps": [{"wait_for": 643, "timeout_ms": 10}]},
        }, frames=_fixture(merchant_say=True))
        assert rows["npcish.json"][3] == CLS_DERIVED, rows["npcish.json"]

    def tooth7():
        # 牙⑦：把 computed 的 delta **应用方向弄反**（base + delta → base - delta）⇒ 红。
        # 夹具 self_x=7、delta=+1 ⇒ 正确值 8；方向反了会得到 6。下面交叉断言两条都钉住，
        # 同时用负 delta（-2）钉住「方向」不是「绝对值」：正确值 5，反了是 9。
        frames = _fixture()
        v, _ = derive(frames, ["move_x"], computed={"move_x": {"derived_from": "self_x",
                                                               "delta": 1}})
        w, _ = derive(frames, ["move_x"], computed={"move_x": {"derived_from": "self_x",
                                                               "delta": -2}})
        assert v["move_x"]["value"] == 8, v
        assert v["move_x"]["value"] != 6, "delta 应用方向反了（得到 base-delta）"
        assert w["move_x"]["value"] == 5, w
        assert w["move_x"]["value"] != 9, "负 delta 方向反了"
        assert v["move_x"]["source"]["delta"] == 1, v
        assert w["move_x"]["source"]["delta"] == -2, w

    def tooth8():
        # 牙⑧：把 computed **混算成 derived** ⇒ 红。
        # 变异：check_all 的分类改成 `cls = CLS_DERIVED if values else ...`（不看 kind）⇒ 下面断言红。
        rows = _tmp_check_all({
            "mv.json": {"name": "mv", "required": ["move_x"],
                        "computed": {"move_x": {"derived_from": "self_x", "delta": 1}},
                        "steps": [{"wait_for": 50, "timeout_ms": 10}]},
        })
        row = rows["mv.json"]
        assert row[1] == 0, row
        assert row[3] == CLS_COMPUTED, "computed 行必须单独归栏（cls=%s）" % row[3]
        assert row[3] != CLS_DERIVED, "computed 被混算成 derived（计入『发现能力已验证』）"

    def c19():
        # 201 正例：多物品槽 ⇒ **同一个槽**给三个变量；用具/可食用分别命中不同槽
        frames = _bag_fixture()
        v, m = derive(frames, ["make_index", "item_name"])
        assert not m, m
        assert v["make_index"]["value"] == 9001, v          # 首个可食用槽（金创药）
        assert v["item_name"]["value"] == "金创药", v
        assert v["make_index"]["kind"] == "derived", v
        assert v["make_index"]["source"]["ident"] == SM_BAGITEMS, v
        assert v["make_index"]["source"]["slot"] == 0, v
        assert v["make_index"]["source"]["select"] == "use", v
        assert v["item_name"]["source"]["slot"] == 0, v     # 与 make_index 同槽
        assert render_vars(v) == ["--var", "item_name=金创药", "--var", "make_index=9001"], \
            render_vars(v)

        v2, m2 = derive(frames, ["make_index", "item_name", "takeon_where"])
        assert not m2, m2
        assert v2["make_index"]["value"] == 123456, v2      # 首个**可装备**槽（屠龙）
        assert v2["item_name"]["value"] == "屠龙", v2
        assert v2["takeon_where"]["value"] == 1, v2         # StdMode 5 → Weapon(1)
        assert v2["takeon_where"]["source"]["slot"] == 1, v2
        assert v2["takeon_where"]["source"]["std_mode"] == 5, v2
        assert v2["make_index"]["source"]["slot"] == v2["item_name"]["source"]["slot"] \
            == v2["takeon_where"]["source"]["slot"] == 1
        assert render_vars(v2) == ["--var", "item_name=屠龙", "--var", "make_index=123456",
                                   "--var", "takeon_where=1"], render_vars(v2)

    def c20():
        # 帧里没有 201 ⇒ fail-closed（missing，不给默认 MakeIndex）
        v, m = derive(_bag_fixture(with_201=False), ["make_index", "item_name"])
        assert m == ["make_index", "item_name"] and not v, (v, m)

    def c21():
        # 201 存在但**目标槽为空** ⇒ missing（不是 error）：空包体 series=0 / 没有可装备槽
        empty = [_f(1, SM_LOGON, recog=191, param=7, tag=12), _bag_frame([], series=0)]
        v, m = derive(empty, ["make_index", "item_name"])
        assert m == ["make_index", "item_name"] and not v, (v, m)
        # 只有一把剑：eat_item 的可食用槽为空 ⇒ missing；bag_equip 仍可用
        sword_only = [_f(1, SM_LOGON, recog=191, param=7, tag=12),
                      _bag_frame([_bag_record_bytes("屠龙", 5, 123456)])]
        v, m = derive(sword_only, ["make_index", "item_name"])
        assert m == ["make_index", "item_name"] and not v, (v, m)
        v, m = derive(sword_only, ["make_index", "item_name", "takeon_where"])
        assert not m and v["takeon_where"]["value"] == 1, (v, m)

    def c22():
        # 多物品取定值：同一帧 5 次结果逐字节相同；顺序取**首个**候选槽（不是最后一个）
        frames = _bag_fixture()
        runs = [derive(frames, ["make_index", "item_name", "takeon_where"]) for _ in range(5)]
        assert all(r == runs[0] for r in runs), runs
        # 三件装备：取第 0 件（首个可装备槽），不是最后一件
        three = [_f(1, SM_LOGON, recog=191, param=7, tag=12),
                 _bag_frame([_bag_record_bytes("布衣(男)", 10, 111),
                             _bag_record_bytes("铁剑", 5, 222),
                             _bag_record_bytes("金创药", 0, 333)])]
        v, _ = derive(three, ["make_index", "item_name", "takeon_where"])
        assert v["make_index"]["value"] == 111, v            # 第 0 槽（Dress）而非第 1/第 2
        assert v["takeon_where"]["value"] == 0, v            # StdMode 10 → Dress(0)
        assert v["make_index"]["source"]["slot"] == 0, v

    def c23():
        # 坏数据 ⇒ DiscoveryError（fail-closed，不许「跳过坏槽继续取」）
        cases = [
            [_bag_frame([_bag_record_bytes("屠龙", 5, 1)],
                        body=mir2frame.encode_bytes(_bag_record_bytes("屠龙", 5, 1)[:100]))],
            [_bag_frame([_bag_record_bytes("屠龙", 5, 1)], series=3)],       # 槽数不符
            [_bag_frame([_bag_record_bytes("屠龙", 5, 1)], body=b"%%%%")],   # EDCode 越界
        ]
        for frames in cases:
            try:
                derive(frames, ["make_index"])
            except DiscoveryError:
                pass
            else:
                raise AssertionError("坏 201 包体必须 DiscoveryError：%r" % (frames,))
        assert _quiet_check_all_main([_write_tmp_frames(cases[1]),
                                      os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                                   "scenarios")]) == 2, \
            "坏 201 ⇒ check-all 整体 rc=2"

    def c25():
        # eat 的选择**不是**「首个可食用」：StdMode31 且 AniCount!=0 的 QFunction 包优先
        # （exec-seams ② 路要的判据包正是这种；`PlayObject.Base.cs:1072 UseStdModeFunItem`）。
        frames = [_f(1, SM_LOGON, recog=191, param=7, tag=12),
                  _bag_frame([_bag_record_bytes("金创药", 0, 9001),
                              _bag_record_bytes("书", 4, 9002),
                              _bag_record_bytes("捆装包(空)", 31, 9003, ani_count=0),
                              _bag_record_bytes("捆装包15", 31, 9004, ani_count=15)])]
        v, m = derive(frames, ["make_index", "item_name"])
        assert not m, m
        assert v["make_index"]["value"] == 9004, v          # 第 3 槽（StdMode31/AniCount15）
        assert v["item_name"]["value"] == "捆装包15", v
        assert v["make_index"]["source"]["slot"] == 3, v
        # 没有 31 ⇒ 药先于书
        frames2 = [_f(1, SM_LOGON, recog=191, param=7, tag=12),
                   _bag_frame([_bag_record_bytes("书", 4, 9101),
                               _bag_record_bytes("金创药", 0, 9102)])]
        v, _ = derive(frames2, ["make_index"])
        assert v["make_index"]["value"] == 9102, v
        # StdMode31/AniCount==0（解包）先于药
        frames3 = [_f(1, SM_LOGON, recog=191, param=7, tag=12),
                   _bag_frame([_bag_record_bytes("金创药", 0, 9201),
                               _bag_record_bytes("捆装包(空)", 31, 9202, ani_count=0)])]
        v, _ = derive(frames3, ["make_index"])
        assert v["make_index"]["value"] == 9202, v

    def c24():
        # 接线：真实场景 bag_equip / eat_item 在合成 201 帧上 rc=0，且归 derived（不是 rule/computed）
        here = os.path.dirname(os.path.abspath(__file__))
        scen_dir = os.path.join(here, "scenarios")
        docs = {}
        for name in ("bag_equip.json", "eat_item.json"):
            with open(os.path.join(scen_dir, name), encoding="utf-8") as fh:
                docs[name] = json.load(fh)
        rows = _tmp_check_all(docs, frames=_bag_fixture())
        for name in docs:
            assert rows[name][1] == 0, rows[name]
            assert rows[name][3] == CLS_DERIVED, rows[name]
        assert "make_index=123456" in rows["bag_equip.json"][2], rows["bag_equip.json"]
        assert "takeon_where=1" in rows["bag_equip.json"][2], rows["bag_equip.json"]
        assert "make_index=9001" in rows["eat_item.json"][2], rows["eat_item.json"]

    case("① 643+自身 ⇒ 值与来源正确（含 --var 渲染）", c1)
    case("② 缺 NPC ⇒ fail-closed", c2)
    case("③ 缺自身坐标 ⇒ fail-closed", c3)
    case("④ 自己的 SM_TURN 不得当 NPC", c4)
    case("⑤ 无 201 ⇒ make_index/item_name 恒 missing", c5)
    case("⑥ 同夹具两次跑同结果（确定性）", c6)
    case("⑦ 混合：可推导的给、不可推导的逐个点名", c7)
    case("⑧ check-all：可推导 rc=0 / 不可推导 rc=2 且报 missing:", c8)
    case("⑨ check-all 分类：params 默认值不算发现绿", c9)
    case("⑩ --allow-missing：只放行 missing，error 与钉数不符仍 rc=2", c10)
    case("⑪ 坏行 ⇒ 干净 rc=2（DiscoveryError）", c11)
    case("⑫ 单场景推导：空 --var 列表 / 空 frames ⇒ rc=2（空判据不算通过）", c12)
    case("⑬ computed 正例：值 = 已推导量 + 场景 delta（kind/source 分栏）", c13)
    case("⑭ computed 未声明 delta ⇒ 场景坏掉（derive 与 check-all 双路径 rc=2）", c14)
    case("⑮ 基准量推不出 ⇒ computed 连锁 fail-closed（missing，不拿 0 顶替）", c15)
    case("⑯ kind 分类：computed 行 cls=computed，不混算成 derived", c16)
    case("⑰ computed 边界声明（不在 required/未知键/bool delta/链式）⇒ 场景坏掉", c17)
    case("⑱ king_actor 归 rule（643 不证明是国王），cls=rule 不计 derived", c18)
    case("⑲ 201 正例：多槽时同槽给 make_index/item_name/takeon_where", c19)
    case("⑳ 无 201 ⇒ fail-closed（不给默认 MakeIndex）", c20)
    case("㉑ 201 存在但目标槽为空 ⇒ missing（不是 error）", c21)
    case("㉒ 多物品取定值：5 次同结果 + 取首个候选槽", c22)
    case("㉓ 坏 201 包体（长度/series/EDCode）⇒ DiscoveryError + check-all rc=2", c23)
    case("㉔ 接线：bag_equip/eat_item 在合成 201 上 rc=0 且 cls=derived", c24)
    case("㉕ eat 优先级：StdMode31/AniCount!=0 的 QFunction 包先于药/书", c25)
    def tooth9():
        # 牙⑨：把 make_index 取到**别的字段**（StdMode；线上没有 StdItem.Index，最接近的混淆字段）
        # ⇒ 值得 5/10 而不是 123456/111 ⇒ 红。变异：BAG_VARS["make_index"]["field"] 改成 std_mode
        # 或偏移 BAG_RECORD_MAKEINDEX_OFF 挪到 15 ⇒ 本断言红。
        frames = _bag_fixture()
        good, _ = derive(frames, ["make_index", "item_name", "takeon_where"])
        bad, _ = derive(frames, ["make_index", "item_name", "takeon_where"],
                        field_map={"make_index": "std_mode"})
        assert good["make_index"]["value"] == 123456, good
        assert bad["make_index"]["value"] == 5, bad
        assert bad["make_index"]["value"] != good["make_index"]["value"], bad
        # 同一物品的 name/takeon_where 不受影响（三变量必须同槽，错取只该表现为值错、
        # 不许把 name 也悄悄换槽）
        assert bad["item_name"]["source"]["slot"] == good["item_name"]["source"]["slot"]

    def tooth10():
        # 牙⑩：「取哪个物品」改成随机 ⇒ 同一帧两次 derive 出现不同值 ⇒ 红。
        # 变异：_bag_select 里 `return records[0]...` 换成 random.choice(candidates) ⇒ 本断言红。
        # 候选 ≥3 时 40 次全同的概率 (1/3)^39 ≈ 0（不是「靠运气」的阈值，是统计上不可能）。
        import random
        frames = [_f(1, SM_LOGON, recog=191, param=7, tag=12),
                  _bag_frame([_bag_record_bytes("甲", 10, 111),
                              _bag_record_bytes("乙", 11, 222),
                              _bag_record_bytes("丙", 15, 333)])]
        original = globals()["_bag_select"]

        def random_select(records, required):
            mode = "equip" if "takeon_where" in (required or []) else "use"
            allowed = STD_MODE_TO_WHERE if mode == "equip" else BAG_USE_MODES
            cand = [(i, r) for i, r in enumerate(records) if r["std_mode"] in allowed]
            idx, rec = random.choice(cand)
            return idx, rec, mode

        seen = set()
        try:
            globals()["_bag_select"] = random_select
            for _ in range(40):
                v, m = derive(frames, ["make_index", "item_name", "takeon_where"])
                assert not m, m
                seen.add(v["make_index"]["value"])
        finally:
            globals()["_bag_select"] = original
        assert len(seen) > 1, ("随机取物品后 40 次仍恒同值 —— 选择规则不可复现，用例断言必须红",
                               seen)
        # 恢复确定性实现后必须恒为首候选（第 0 槽 甲/111）
        v, _ = derive(frames, ["make_index", "item_name", "takeon_where"])
        assert v["make_index"]["value"] == 111, v

    def tooth11():
        # 牙⑪：takeon_where 取成 MakeIndex（而不是 StdMode→ItemLocation 逆表）⇒ 值不是合法槽号 ⇒ 红。
        frames = _bag_fixture()
        good, _ = derive(frames, ["make_index", "item_name", "takeon_where"])
        bad, _ = derive(frames, ["make_index", "item_name", "takeon_where"],
                        field_map={"takeon_where": "make_index"})
        assert good["takeon_where"]["value"] == 1, good
        assert bad["takeon_where"]["value"] == 123456, bad
        assert bad["takeon_where"]["value"] not in STD_MODE_TO_WHERE.values(), bad

    case("牙① self_x 错取 tag ⇒ 值与真值不同", tooth1)
    case("牙② check-all 静默吞 missing（if missing:→if False:）⇒ 行为断言红", tooth2)
    case("牙③ 第三方 actor 无 643 ⇒ merchant_actor fail-closed", tooth3)
    case("牙④ 零场景（缺目录/空目录/--allow-missing）⇒ rc=2", tooth4)
    case("牙⑤ 场景文件坏掉（坏 JSON/数组/required 非字符串数组）⇒ cls=error rc=2", tooth5)
    case("牙⑥ required 含退化规则变量 ⇒ cls=rule（不计 derived）", tooth6)
    case("牙⑦ computed delta 应用方向弄反（+ → -）⇒ 值断言红", tooth7)
    case("牙⑧ computed 混算成 derived ⇒ 分类断言红", tooth8)
    case("牙⑨ make_index 错取 StdMode ⇒ 值断言红（三变量仍同槽）", tooth9)
    case("牙⑩ 取物品改成随机 ⇒ 40 次出现多值、断言红", tooth10)
    case("牙⑪ takeon_where 取成 MakeIndex（绕过 StdMode 逆表）⇒ 红", tooth11)
    if fails:
        print("param_discovery selftest: FAIL")
        for item in fails:
            print("  -", item)
        return 1
    print("param_discovery selftest: OK (25 cases + 11 teeth)")
    return 0


def main(argv=None):
    argv = list(sys.argv[1:] if argv is None else argv)
    if not argv:
        print(__doc__)
        return 2
    try:
        if argv[0] == "selftest":
            return selftest()
        if argv[0] == "check-all":
            return check_all_main(argv[1:])
        if len(argv) < 2:
            sys.stderr.write("usage: param_discovery.py selftest | <frames.ndjson> "
                             "<var1,var2,...> | check-all <frames> [dir] [--allow-missing[=N]]\n")
            return 2
        frames = load_frames(argv[0])
        required = [x.strip() for x in argv[1].split(",") if x.strip()]
        if not required:
            # 空判据不算通过：一个变量都没要求 ⇒ 无从「发现」，退回 usage 口径 rc=2
            sys.stderr.write("param_discovery: no variables requested (empty --var list) — "
                             "空判据不算通过\n")
            return 2
        values, missing = derive(frames, required)
        print(json.dumps({"values": values, "missing": missing,
                          "reasons": dict((v, REASONS.get(v) or UNDERIVABLE.get(v)
                                           or "规则表无此变量（不猜）") for v in missing),
                          "argv": render_vars(values)}, ensure_ascii=False, indent=1))
        return 0 if not missing else 2
    except DiscoveryError as ex:
        sys.stderr.write("param_discovery: %s\n" % ex)
        return 2


if __name__ == "__main__":
    sys.exit(main())
