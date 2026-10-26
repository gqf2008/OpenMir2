#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""blocked_sources_check.py — blocked **两来源一致性 + 孤儿/证据**机判（只读；不起服务端、不占端口）。

两个来源：清单行 `run3-scenarios.json` 的 `blocked`（管批次排除）与场景 JSON
`tools/probe/scenarios/<name>.json` 的 `blocked`（管直接跑 card_window 的人工口径）。

判据（2026-10-25 扩容，卡 `openmir2-orphan-scenario-gate`）：
  1. **两来源一致 + 断言 token**（原有，一条未删）：run3 里两侧都 blocked 的场景，
     `BLOCKED_TOKENS` 的 token 必须两侧都出现；表项自带 `source` 锚与 `absent` 禁用词。
  2. **扫全部场景**（不只 run3 的行）：blocked 场景必须命中 (a) run3 行 + token，或
     (b) 场景内**结构化** `blocked.not_in_run3_reason`；两者皆无 ⇒ rc=1 点名（孤儿）。
  3. **结构化 blocked**：`missing`（旧别名 `missing_fixture`）+ `evidence[]`；
     `evidence[].kind ∈ {card, commit, path}` 是封闭枚举，value 机判
     （card 在账本 / commit 40hex 且对象存在 / path 在工作树存在）。
     `reason` 保持自由文本、**不参与判据**；纯文本 `not_in_run3_reason` ⇒ 判不了（rc=2）。
  4. **前提已落地/已结 ⇒ 证伪**：evidence 的 commit 已是 master 祖先、或 card 在账本里 closed，
     且没有**结构化** `still_blocked_because` ⇒ rc=1；still 自身也必须 `{reason, evidence}`。
     **已知边界（reviewer-21 F2，2026-10-25）**：card 证据只认 `status == closed`；
     **merged 但 in-progress 的卡不触发**（`openmir2-gm-target-state-write` 正是这种形态，
     p21_unequip 的过时理由曾躲在这个盲区里）。补法（follow-up 牙，不在本卡改判定逻辑）：
     card 证据应同时给 commit 指针，才能判「已合入」；届时应加一条「merged-but-in-progress ⇒ rc=1」的牙。
  5. **兼容基线**：既有 `{missing_fixture, reason}` 旧形态不迁移，不因缺新字段被判 UNCLASSIFIED
     （D9 承重牙把 7 个既有 blocked 场景喂进来验证这条）。

退出码（1 = 证伪，2 = 判不了；与 `compare_runs`/`goal_gate` 的 UNDECIDABLE=2 同口径）：
  0 = 全过（两来源一致 + 孤儿都有结构化理由 + 证据可解析且未落地 + --list 集合一致）；
  1 = 已证伪/违规（两来源不一致 / 缺 token / 孤儿无理由 / 证据指向已落地前提且无 excuse /
      --list 集合不符）；
  2 = 判不了或用法错（缺结构 / 证据不可解析 / 文件读不了 / run3 blocked 缺断言表条目）。

用法：
  python -I -B tools/window/blocked_sources_check.py check
  python -I -B tools/window/blocked_sources_check.py check --list-output E:/tmp/run3-list.txt
  python -I -B tools/window/blocked_sources_check.py selftest

`--list-output` = 先 `bash tools/window/run_batch.sh --list > <file>` 再喂进来：额外要求
**`--list` 标 YES 的场景集合 ==（run3 里）场景 JSON blocked 为真的集合**。
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

DEFAULT_RUN3 = os.path.join('tools', 'window', 'run3-scenarios.json')
DEFAULT_SCENES = os.path.join('tools', 'probe', 'scenarios')
REPO_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
#: `blocked.evidence[].kind` 的封闭枚举（2026-10-25 裁定）。
EVIDENCE_KINDS = ('card', 'commit', 'path')
#: 账本里视为「前提已结」的状态（命中且无 still_blocked_because ⇒ rc=1）。
CLOSED_STATUSES = ('closed',)
LIST_ROW_RE = re.compile(r'^\s*(\d+)\s+(\S+)\s+(\S+)\s+(YES|-)\s*(.*)$')

#: 每条 blocked 场景**必须**在两侧文案里同时出现的机器可核 token（确定性、无阈值、不要求逐字）。
#: 加场景必须同时加一条，并写清 source（audit 报告的章节锚）——否则 `check` rc=2。
BLOCKED_TOKENS = {
    'butch_two_draws': {
        'source': 'docs/blocked-scenarios-audit-20261025.md §1.2',
        'absent': ('未合',),
        'tokens': ('3081', '616', 'ProcessMonsterMultiThreadLimit'),
    },
    'castle_merchant_tax': {
        'source': 'docs/blocked-scenarios-audit-20261025.md §2.2/§2.3',
        'absent': ('未合',),
        'tokens': ('652', '原因未定位', '已证伪'),
    },
    'castle_merchant_tax_upgrade': {
        'source': 'docs/blocked-scenarios-audit-20261025.md §3.2/§3.3',
        'absent': ('未合',),
        'tokens': ('已证伪', 'NPC'),
    },
    # 2026-10-27（卡 `openmir2-npc-click-no-response-family`）：oracle 点 `Npcs.txt` type=1 的 `GuildOfficial`
    # 走 `FindMerchant` 的 `(IMerchant)actor` 硬转 ⇒ `InvalidCastException` ⇒ `PlayObject.Run` 吞掉 ⇒ 0 帧。
    # token 取「原因侧」三个词：换写法可以，但两侧文案必须同时说清「是类型门 + 是抛异常」。
    'king_dialog': {
        'source': 'docs/window-runbook-20261023.md §10.2',
        'absent': ('未合',),
        'tokens': ('GuildOfficial', 'IMerchant', 'InvalidCastException'),
    },
    'king_castle_war': {
        'source': 'docs/window-runbook-20261023.md §10.2',
        'absent': ('未合',),
        'tokens': ('GuildOfficial', 'IMerchant', 'InvalidCastException'),
    },
}


class UsageError(Exception):
    pass


def load_run3(path):
    try:
        doc = json.load(open(path, encoding='utf-8'))
    except OSError as ex:
        raise UsageError('读不到清单 %s：%s' % (path, ex))
    except ValueError as ex:
        raise UsageError('清单 %s 不是合法 JSON：%s' % (path, ex))
    rows = doc.get('scenarios') if isinstance(doc, dict) else doc
    if not isinstance(rows, list) or not rows:
        raise UsageError('清单 %s 里没有 scenarios 数组' % path)
    for i, r in enumerate(rows):
        if not isinstance(r, dict) or not isinstance(r.get('name'), str) or not r['name']:
            raise UsageError('清单第 %d 行缺 name（fail-closed）' % (i + 1))
    return rows


def load_scene(scenes_dir, name):
    path = os.path.join(scenes_dir, name + '.json')
    try:
        return json.load(open(path, encoding='utf-8'))
    except OSError:
        raise UsageError('清单行 %r 找不到场景 JSON：%s（fail-closed，不猜）' % (name, path))
    except ValueError as ex:
        raise UsageError('场景 JSON %s 不是合法 JSON：%s' % (path, ex))


def row_blocked(row):
    """清单行 blocked：非空字符串/非空对象才算真。"""
    b = row.get('blocked')
    if isinstance(b, str):
        return bool(b.strip())
    if isinstance(b, dict):
        return bool(b)
    return bool(b)


def scene_blocked(scene):
    """场景 JSON blocked：必须是非空对象。"""
    return isinstance(scene.get('blocked'), dict) and bool(scene['blocked'])


def token_present(text, tok):
    """token 是否出现在文案里。

    **数字 token 用词边界**（`(?<![0-9.,])616(?![0-9.,])`）：相邻字符是数字（或小数点/千分位 `.`/`,`）
    就算**缺失** ⇒ `616` 不被 `6160`/`616.5`/`1,616` 命中，`652` 不被 `1652`、`3081` 不被 `30810`；
    字母/单位/标点/中文都算命中（`≈616s` 命中）。非数字 token 保持**子串**语义
    （长标识符里出现即算，如 `ProcessMonsterMultiThreadLimit`）。
    """
    if tok.isdigit():
        return re.search(r"(?<![0-9.,])%s(?![0-9.,])" % re.escape(tok), text) is not None
    return tok in text


def scene_text(scene):
    """场景 JSON 里参与 token/禁用词判定的文本：**description + reason + missing_fixture**。

    `description` 必须纳入（卡 `openmir2-run3-stale-card-status-json` 的 N1）：上一张卡的陈旧文案
    恰好写在 description 里，只扫 blocked 字段会漏。
    """
    b = scene.get('blocked') or {}
    return '%s\n%s\n%s' % (scene.get('description') or '',
                             b.get('reason') or '', b.get('missing_fixture') or '')


def scene_reason_ok(scene):
    b = scene.get('blocked')
    if not isinstance(b, dict):
        return False
    reason = b.get('reason')
    return isinstance(reason, str) and bool(reason.strip())


def validate_tokens_table(tokens):
    if not isinstance(tokens, dict) or not tokens:
        raise UsageError('断言 token 表为空（fail-closed）')
    for name, entry in tokens.items():
        if not isinstance(entry, dict):
            raise UsageError('断言表 %r 必须是对象' % name)
        toks = entry.get('tokens')
        src = entry.get('source')
        if not isinstance(toks, (list, tuple)) or not toks or any(
                not isinstance(t, str) or not t for t in toks):
            raise UsageError('断言表 %r.tokens 必须是非空字符串数组' % name)
        if not isinstance(src, str) or not src.strip():
            raise UsageError('断言表 %r 缺 source 锚（表也要可追溯）' % name)
        absent = entry.get('absent')
        if not isinstance(absent, (list, tuple)) or not absent or any(
                not isinstance(w, str) or not w for w in absent):
            raise UsageError(
                '断言表 %r 缺 absent（禁用词，如 未合/尚未合）——必须显式登记，'
                '不得因缺省而漏（fail-closed）' % name)



def scene_docs(scenes_dir):
    """读场景目录里全部 *.json，返回 {name: doc}（fail-closed：读不了/坏 JSON ⇒ UsageError）。"""
    try:
        names = sorted(os.listdir(scenes_dir))
    except OSError as ex:
        raise UsageError('读不到场景目录 %s：%s' % (scenes_dir, ex))
    docs = {}
    for fn in names:
        if not fn.endswith('.json'):
            continue
        path = os.path.join(scenes_dir, fn)
        try:
            docs[fn[:-5]] = json.load(open(path, encoding='utf-8'))
        except OSError as ex:
            raise UsageError('读不到场景 JSON %s：%s' % (path, ex))
        except ValueError as ex:
            raise UsageError('场景 JSON %s 不是合法 JSON：%s' % (path, ex))
    if not docs:
        raise UsageError('场景目录 %s 里没有任何 *.json（fail-closed）' % scenes_dir)
    return docs


def blocked_missing(b):
    """`missing` 的合法旧别名是 `missing_fixture`（2026-10-25 裁定）；两者都没有 ⇒ None。"""
    for key in ('missing', 'missing_fixture'):
        v = b.get(key)
        if isinstance(v, str) and v.strip():
            return v.strip()
    return None


def _nonempty_str(v):
    return isinstance(v, str) and bool(v.strip())


def _git(repo, *args):
    try:
        return subprocess.run(['git', '-C', repo, *args], capture_output=True, text=True,
                              stdin=subprocess.DEVNULL, timeout=120)
    except (OSError, subprocess.TimeoutExpired):
        return None


def commit_exists(repo, sha):
    if not isinstance(sha, str) or not re.fullmatch(r'[0-9a-f]{40}', sha):
        return False
    p = _git(repo, 'cat-file', '-e', sha + '^{commit}')
    return p is not None and p.returncode == 0


def commit_landed(repo, sha, master='master'):
    """sha 是否已经是本地 master 的祖先（= 该前提已落地）。"""
    if not commit_exists(repo, sha):
        return False
    p = _git(repo, 'merge-base', '--is-ancestor', sha, master)
    return p is not None and p.returncode == 0


def load_ledger(repo):
    """一次 `walgit collab board --format json` ⇒ {card_id: status}；不可用返回 None。"""
    try:
        p = subprocess.run(['walgit', 'collab', 'board', '--format', 'json', '--repo', repo],
                           capture_output=True, text=True, stdin=subprocess.DEVNULL,
                           timeout=120)
    except (OSError, subprocess.TimeoutExpired):
        return None
    if p.returncode != 0:
        return None
    try:
        doc = json.loads(p.stdout)
    except ValueError:
        return None
    out = {}
    for col in doc.get('columns', []):
        for card in col.get('cards', []):
            cid = card.get('id')
            if isinstance(cid, str):
                out[cid] = card.get('status')
    return out


def _check_evidence(items, where, repo, ledger, commit_check, landed_check,
                    landed_rule=True):
    """返回 (undecidable, problems)。证据形状/可解析性属「判不了」（rc=2），
    证据指向已落地前提且无 still_blocked_because 属「证伪」（rc=1）。"""
    und, vio = [], []
    if not isinstance(items, list) or not items:
        und.append('%s：evidence 必须是非空数组（缺结构）' % where)
        return und, vio
    for i, ev in enumerate(items):
        tag = '%s.evidence[%d]' % (where, i)
        if not isinstance(ev, dict):
            und.append('%s：必须是 {kind, value} 对象（缺结构）' % tag)
            continue
        kind, value = ev.get('kind'), ev.get('value')
        if kind not in EVIDENCE_KINDS:
            und.append('%s：kind=%r 不在封闭枚举 %s（缺结构）'
                       % (tag, kind, list(EVIDENCE_KINDS)))
            continue
        if not _nonempty_str(value):
            und.append('%s：value 必须非空字符串（缺结构）' % tag)
            continue
        if kind == 'commit':
            if not commit_check(value):
                und.append('%s：commit %s 不是 40hex 或对象不存在（判不了）' % (tag, value))
                continue
            if landed_rule and landed_check(value):
                vio.append('%s：commit %s 已是 master 祖先（前提已落地）；'
                           '确需继续封存请给 still_blocked_because（结构化）' % (tag, value))
        elif kind == 'path':
            if not os.path.exists(os.path.join(repo, value)):
                und.append('%s：path %s 在工作树不存在（判不了）' % (tag, value))
        else:  # card
            if ledger is None:
                und.append('%s：card %s 无法查账本（walgit 不可用；判不了）' % (tag, value))
            elif value not in ledger:
                und.append('%s：card %s 不在账本（判不了）' % (tag, value))
            elif (ledger.get(value) or '') in CLOSED_STATUSES:
                vio.append('%s：card %s 已 closed（前提已结）；'
                           '确需继续封存请给 still_blocked_because（结构化）' % (tag, value))
    return und, vio


def _check_structured_reason(value, where, repo, ledger, commit_check, landed_check,
                            landed_rule=True):
    """`not_in_run3_reason` / `still_blocked_because` 的公共形状：{reason, evidence[]}。

    `still_blocked_because` 自己的证据**不再**套「已落地 ⇒ 红」（否则永远自相矛盾），
    由调用方传 `landed_rule=False`。
    """
    if not isinstance(value, dict):
        return (['%s：必须是 {reason, evidence} 对象（缺结构；纯文本判不了）' % where], [])
    und, vio = [], []
    if not _nonempty_str(value.get('reason')):
        und.append('%s：reason 必须非空字符串（缺结构）' % where)
    e_und, e_vio = _check_evidence(value.get('evidence'), where, repo, ledger,
                                   commit_check, landed_check, landed_rule=landed_rule)
    return und + e_und, vio + e_vio


def parse_list_output(path):
    """解析 `run_batch.sh --list`：返回 (rows{n:name}, blocked_names set)。"""
    try:
        text = open(path, encoding='utf-8', errors='replace').read()
    except OSError as ex:
        raise UsageError('读不到 --list 输出 %s：%s' % (path, ex))
    rows = {}
    blocked = set()
    for line in text.splitlines():
        m = LIST_ROW_RE.match(line)
        if not m:
            continue
        n, name, _prof, yesno = int(m.group(1)), m.group(2), m.group(3), m.group(4)
        rows[n] = name
        if yesno == 'YES':
            blocked.add(name)
    if not rows:
        raise UsageError('--list 输出里没有解析到任何场景行（fail-closed）：%s' % path)
    return rows, blocked


def check(run3_path, scenes_dir, list_output=None, tokens=None, repo=None,
          ledger=None, commit_check=None, landed_check=None):
    """返回 (problems, undecidable, info)。

    * problems   = 已证伪/违规 ⇒ rc=1（两来源不一致、缺 token、孤儿无理由、
                   证据指向已落地前提且无 still_blocked_because）；
    * undecidable = 判不了（缺结构 / 证据不可解析）⇒ rc=2；
    * UsageError ⇒ rc=2（用法/文件不可读/未登记的 run3 blocked 缺 token 条目）。
    """
    tokens = BLOCKED_TOKENS if tokens is None else tokens
    validate_tokens_table(tokens)
    repo = REPO_ROOT if repo is None else repo
    if commit_check is None:
        commit_check = lambda sha: commit_exists(repo, sha)
    if landed_check is None:
        landed_check = lambda sha: commit_landed(repo, sha)
    ledger = load_ledger(repo) if ledger is None else ledger
    rows = load_run3(run3_path)
    scenes = scene_docs(scenes_dir)
    rows_by_name = {r['name']: r for r in rows}
    problems, und = [], []
    info = {'rows': len(rows), 'scenes': len(scenes), 'blocked_run3': [],
            'blocked_scene': [], 'orphans': []}
    for row in rows:
        name = row['name']
        if name not in scenes:
            raise UsageError('清单行 %r 找不到场景 JSON：%s（fail-closed，不猜）'
                             % (name, os.path.join(scenes_dir, name + '.json')))
        scene = scenes[name]
        rb = row_blocked(row)
        sb = scene_blocked(scene)
        if rb:
            info['blocked_run3'].append(name)
        if sb:
            info['blocked_scene'].append(name)
        if rb != sb:
            problems.append(
                '%s：两来源不一致（清单行 blocked=%s，场景 JSON blocked=%s）'
                % (name, '真' if rb else '假', '真' if sb else '假'))
            continue
        if not rb:
            continue
        if not isinstance(row.get('blocked'), str) or not row['blocked'].strip():
            problems.append('%s：清单行 blocked 为真但不是非空字符串' % name)
        if not scene_reason_ok(scene):
            problems.append('%s：场景 JSON blocked 缺非空 reason' % name)
            continue
        entry = tokens.get(name)
        if entry is None:
            raise UsageError(
                '%s：清单与场景都 blocked，但**没有断言表条目**（fail-closed：新 blocked 场景必须在 '
                'blocked_sources_check.BLOCKED_TOKENS 登记 tokens + source，不得绕过检查）' % name)
        row_text = row.get('blocked') or ''
        stext = scene_text(scene)
        for word in entry['absent']:
            where = []
            if word in row_text:
                where.append('清单行')
            if word in stext:
                where.append('场景 JSON')
            if where:
                problems.append(
                    '%s：%s 文案含禁用词 %r（已关闭/已合入的卡不得写成未合；断言表 source=%s）'
                    % (name, '+'.join(where), word, entry['source']))
        for tok in entry['tokens']:
            where = []
            if not token_present(row_text, tok):
                where.append('清单行')
            if not token_present(stext, tok):
                where.append('场景 JSON')
            if where:
                problems.append(
                    '%s：断言 token %r 未出现在 %s（断言表 source=%s；两侧文案都必须含齐）'
                    % (name, tok, '+'.join(where), entry['source']))

    # --- 2026-10-25 扩容：扫**全部**场景（不只 run3 的行）---
    for name in sorted(scenes):
        scene = scenes[name]
        if not scene_blocked(scene):
            continue
        b = scene['blocked']
        new_style = any(k in b for k in ('missing', 'evidence', 'not_in_run3_reason',
                                         'still_blocked_because'))
        if new_style and blocked_missing(b) is None:
            und.append('%s：blocked 缺 missing/missing_fixture（缺结构）' % name)
        # 给了结构化 still_blocked_because ⇒ 允许「前提已落地/已结」继续封存（excuse）。
        has_still = 'still_blocked_because' in b
        if new_style and 'evidence' in b:
            e_und, e_vio = _check_evidence(b.get('evidence'), name, repo, ledger,
                                           commit_check, landed_check,
                                           landed_rule=not has_still)
            und += e_und
            problems += e_vio
        if new_style and 'still_blocked_because' in b:
            s_und, s_vio = _check_structured_reason(
                b.get('still_blocked_because'), name + '.still_blocked_because',
                repo, ledger, commit_check, landed_check, landed_rule=False)
            und += s_und
            problems += s_vio
        if name not in rows_by_name:
            info['orphans'].append(name)
            reason = b.get('not_in_run3_reason')
            if reason is None:
                problems.append(
                    '%s：blocked 且不在 run3，且没有 not_in_run3_reason（孤儿；'
                    '补结构化理由或进清单，不得无声存在）' % name)
            else:
                r_und, r_vio = _check_structured_reason(
                    reason, name + '.not_in_run3_reason', repo, ledger,
                    commit_check, landed_check, landed_rule=not has_still)
                und += r_und
                problems += r_vio

    # 断言表只约束「在 run3 里也 blocked」的场景（孤儿由 D2 管，不要求 token 表条目）
    must_register = {n for n in info['blocked_scene'] if n in rows_by_name}
    unregistered = sorted(must_register - set(tokens))
    if unregistered:
        raise UsageError('以下 blocked 场景没有断言表条目：%s（fail-closed）' % unregistered)
    if list_output:
        lrows, lblocked = parse_list_output(list_output)
        listed = set(lrows.values())
        names = {r['name'] for r in rows}
        if listed != names:
            problems.append('--list 的场景集合与清单不同：只在 --list=%s，只在清单=%s'
                            % (sorted(listed - names), sorted(names - listed)))
        scene_set = {n for n in info['blocked_scene'] if n in rows_by_name}
        if lblocked != scene_set:
            problems.append('--list 的 blocked 集合 != 场景 JSON 的 blocked 集合：'
                            '只在 --list=%s，只在场景 JSON=%s'
                            % (sorted(lblocked - scene_set), sorted(scene_set - lblocked)))
    info['blocked_scene'] = [n for n in sorted(scenes) if scene_blocked(scenes[n])]
    info['undecidable'] = len(und)
    info['problems'] = len(problems)
    return problems, und, info


# ---------------------------------------------------------------- selftest
def selftest():
    ok = fail = 0

    def chk(cond, name):
        nonlocal ok, fail
        if cond:
            ok += 1
            print('PASS %s' % name)
        else:
            fail += 1
            print('FAIL %s' % name)

    tmp = tempfile.mkdtemp(prefix='blocked-sources-selftest.')
    try:
        scenes = os.path.join(tmp, 'scenarios')
        os.makedirs(scenes)
        run3 = os.path.join(tmp, 'run3.json')
        table = {'a': {'source': 'audit §1', 'absent': ('未合',),
                       'tokens': ('TOK1', 'TOK2')}}
        row_b = {'name': 'a', 'blocked': '清单：TOK1 与 TOK2 都在'}
        row_ok = {'name': 'b'}

        def write(rows, scene_block, reason='场景：TOK1 TOK2 都在', description=None):
            for fn in os.listdir(scenes):   # 新扫描面会读整个目录：先清掉上一用例的残留
                os.remove(os.path.join(scenes, fn))
            json.dump({'format': 'run3/1', 'scenarios': rows},
                      open(run3, 'w', encoding='utf-8'), ensure_ascii=False)
            for row in rows:
                name = row['name']
                sdoc = {'name': name, 'steps': []}
                if description is not None:
                    sdoc['description'] = description
                if name in scene_block:
                    sdoc['blocked'] = {'reason': reason, 'missing_fixture': '夹具'}
                json.dump(sdoc, open(os.path.join(scenes, name + '.json'), 'w',
                                     encoding='utf-8'), ensure_ascii=False)

        def run(rows, scene_block, reason='场景：TOK1 TOK2 都在', table_=None, extra_rows=None,
                list_path=None, description=None, ledger=None, commit_check=None,
                landed_check=None, repo=None):
            write(rows, scene_block, reason, description)
            try:
                probs, und, _info = check(
                    run3, scenes, list_path, table_ if table_ is not None else table,
                    repo=repo if repo is not None else tmp,
                    ledger={} if ledger is None else ledger,
                    commit_check=commit_check if commit_check is not None else (lambda s: True),
                    landed_check=landed_check if landed_check is not None else (lambda s: False))
                return probs, und
            except UsageError as ex:
                return ('USAGE', str(ex))

        # 牙 a：两处都 blocked，**只把场景 JSON 文案换成完全不同内容** ⇒ 必红（reviewer-20 的复现）
        probs, _ = run([row_b, row_ok], {'a'}, reason='此场景其实可跑（DRIFT 演示）')
        chk(any("'TOK1'" in p and '场景 JSON' in p for p in probs),
            '牙a：两处都 blocked 但场景文案完全不同 ⇒ 红（原假绿）')
        # 牙 b：只删清单行 ⇒ 红
        probs, _ = run([{'name': 'a'}, row_ok], {'a'})
        chk(any('a' in p and '不一致' in p for p in probs), '牙b：只删清单行 ⇒ 红')
        # 牙 c：只删场景 JSON ⇒ 红
        probs, _ = run([row_b, row_ok], set())
        chk(any('a' in p and '不一致' in p for p in probs), '牙c：只删场景 JSON ⇒ 红')
        # 牙 d：两处都删 ⇒ 绿
        probs, _ = run([row_ok, {'name': 'c'}], set())
        chk(probs == [], '牙d：两处都不 blocked ⇒ 绿')
        # 牙 e：新增 blocked 场景但表里没有 ⇒ rc=2 并点名
        out = run([row_b, {'name': 'c', 'blocked': '清单：TOK1 TOK2'}], {'a', 'c'})
        chk(isinstance(out, tuple) and out[0] == 'USAGE' and 'c' in out[1],
            '牙e：未登记的 blocked 场景 ⇒ rc=2 点名')
        # 牙 f：表里的 token 从两侧文案里都删掉（表不改）⇒ 红（自一致性）
        table2 = {'a': {'source': 'audit §1', 'absent': ('未合',),
                        'tokens': ('TOK1', 'GONE')}}
        probs, _ = run([{'name': 'a', 'blocked': '清单：TOK1 在'}, row_ok], {'a'},
                       reason='场景：TOK1 在', table_=table2)
        chk(any("'GONE'" in p for p in probs), '牙f：表 token 与两侧文案不一致 ⇒ 红')
        # 牙 g：数字 token 词边界（616 不应被 6160 命中）
        table3 = {'a': {'source': 'audit §1', 'absent': ('未合',),
                        'tokens': ('616', 'TOKX')}}
        probs, _ = run([{'name': 'a', 'blocked': '清单：TOKX 在，数 6160'}, row_ok], {'a'},
                       reason='场景：TOKX 在，数 6160', table_=table3)
        chk(any("'616'" in p for p in probs), '牙g：token 616 不被 6160 命中 ⇒ 红')
        probs, _ = run([{'name': 'a', 'blocked': '清单：TOKX 在，数 616'}, row_ok], {'a'},
                       reason='场景：TOKX 在，数 616', table_=table3)
        chk(probs == [], '牙g-对照：token 616 正常出现 ⇒ 绿')
        # 牙 h：3081 vs 30810
        table4 = {'a': {'source': 'audit §1', 'absent': ('未合',),
                        'tokens': ('3081',)}}
        probs, _ = run([{'name': 'a', 'blocked': '清单：30810'}, row_ok], {'a'},
                       reason='场景：30810', table_=table4)
        chk(any("'3081'" in p for p in probs), '牙h：token 3081 不被 30810 命中 ⇒ 红')
        # 牙 i：652 vs 1652
        table5 = {'a': {'source': 'audit §1', 'absent': ('未合',),
                        'tokens': ('652',)}}
        probs, _ = run([{'name': 'a', 'blocked': '清单：1652'}, row_ok], {'a'},
                       reason='场景：1652', table_=table5)
        chk(any("'652'" in p for p in probs), '牙i：token 652 不被 1652 命中 ⇒ 红')
        # 牙 g2：小数点/千分位不是边界（616.5 / 1,616 都不算命中）
        probs, _ = run([{'name': 'a', 'blocked': '清单：TOKX 在，数 616.5'}, row_ok], {'a'},
                       reason='场景：TOKX 在，数 616.5', table_=table3)
        chk(any("'616'" in p for p in probs), '牙g2：616 不被 616.5 命中 ⇒ 红')
        probs, _ = run([{'name': 'a', 'blocked': '清单：TOKX 在，数 1,616'}, row_ok], {'a'},
                       reason='场景：TOKX 在，数 1,616', table_=table3)
        chk(any("'616'" in p for p in probs), '牙g3：616 不被 1,616 命中 ⇒ 红')
        probs, _ = run([{'name': 'a', 'blocked': '清单：TOKX 在，≈616s'}, row_ok], {'a'},
                       reason='场景：TOKX 在，≈616s', table_=table3)
        chk(probs == [], '牙g4-对照：≈616s（字母边界）仍命中 ⇒ 绿')
        # 非数字 token 仍是子串语义：含在长标识符里也算
        table6 = {'a': {'source': 'audit §1', 'absent': ('未合',),
                        'tokens': ('ProcessMonsterMultiThreadLimit',)}}
        probs, _ = run([{'name': 'a', 'blocked': '清单：x ProcessMonsterMultiThreadLimit x'},
                        row_ok], {'a'},
                       reason='场景：x ProcessMonsterMultiThreadLimit x', table_=table6)
        chk(probs == [], '非数字 token 维持子串语义 ⇒ 绿')
        # 牙 j：禁用词「未合」出现在清单行 ⇒ 红
        probs, _ = run([{'name': 'a', 'blocked': '清单：TOK1 TOK2，卡未合'}, row_ok], {'a'})
        chk(any('未合' in p and '清单行' in p for p in probs),
            '牙j：清单行含禁用词「未合」⇒ 红')
        # 牙 k：禁用词只出现在场景 JSON ⇒ 红
        probs, _ = run([{'name': 'a', 'blocked': '清单：TOK1 TOK2'}, row_ok], {'a'},
                       reason='场景：TOK1 TOK2，卡未合')
        chk(any('未合' in p and '场景 JSON' in p for p in probs),
            '牙k：场景 JSON 含禁用词「未合」⇒ 红')
        # 牙 k2：禁用词只出现在 **description** ⇒ 红（N1：只扫 blocked 会漏）
        probs, _ = run([{'name': 'a', 'blocked': '清单：TOK1 TOK2'}, row_ok], {'a'},
                       description='描述：这张卡未合，先别用')
        chk(any('未合' in p and '场景 JSON' in p for p in probs),
            '牙k2：description 含禁用词「未合」⇒ 红')
        # 牙 l：合法文案（已合入 + 尚未接入 run3）⇒ 绿
        probs, _ = run([{'name': 'a', 'blocked': '清单：TOK1 TOK2，已合入 c5ec908e1，尚未接入 run3'},
                        row_ok], {'a'},
                       reason='场景：TOK1 TOK2，已合入 c5ec908e1，尚未接入 run3')
        chk(probs == [], '牙l：合法文案（已合入/尚未接入）⇒ 绿')
        # 牙 m：表项缺 absent ⇒ rc=2（不得因缺省而漏）
        out = run([row_b, row_ok], {'a'},
                  table_={'a': {'source': 'audit §1', 'tokens': ('TOK1', 'TOK2')}})
        chk(isinstance(out, tuple) and out[0] == 'USAGE' and 'absent' in out[1],
            '牙m：表项缺 absent ⇒ rc=2')
        # 表项缺 source ⇒ rc=2
        out = run([row_b, row_ok], {'a'},
                  table_={'a': {'tokens': ('TOK1',)}})
        chk(isinstance(out, tuple) and out[0] == 'USAGE' and 'source' in out[1],
            '断言表项缺 source ⇒ rc=2')
        # 场景 JSON blocked 缺 reason ⇒ 红
        probs, _ = run([{'name': 'a', 'blocked': 'TOK1 TOK2'}], set())
        sdoc = json.load(open(os.path.join(scenes, 'a.json'), encoding='utf-8'))
        sdoc['blocked'] = {'missing_fixture': 'x'}
        json.dump(sdoc, open(os.path.join(scenes, 'a.json'), 'w', encoding='utf-8'),
                  ensure_ascii=False)
        try:
            probs, _und, _info = check(run3, scenes, None, table)
        except UsageError as ex:  # 缺 reason 时仍应给出 problem 而不是异常：这里要红
            probs = ['USAGE ' + str(ex)]
        chk(any('reason' in p for p in probs), '场景 JSON blocked 缺 reason ⇒ 红')
        # 缺场景 JSON ⇒ rc=2
        write([{'name': 'a', 'blocked': 'x'}], set())
        try:
            check(run3, os.path.join(tmp, 'nope'), None, table)
            chk(False, '缺场景 JSON ⇒ rc=2')
        except UsageError:
            chk(True, '缺场景 JSON ⇒ rc=2')
        # --list 输出：YES 集合 != 场景 JSON blocked 集合 ⇒ 红
        write([row_ok], set())
        lop = os.path.join(tmp, 'list.txt')
        open(lop, 'w', encoding='utf-8').write(
            '  1  b            alt           YES     \n场景数 1（blocked 1 ⇒ 入批 0）\n')
        out = run([row_ok], set(), list_path=lop)
        probs = out[0] if isinstance(out, tuple) else out
        chk(any('--list' in p and 'blocked' in p for p in probs),
            '--list 的 YES 集合 != 场景 JSON blocked ⇒ 红')
        open(lop, 'w', encoding='utf-8').write(
            '  1  b            alt           -       \n场景数 1（blocked 0 ⇒ 入批 1）\n')
        probs, _ = run([row_ok], set(), list_path=lop)
        chk(probs == [], '--list 与场景 JSON 都不 blocked ⇒ 绿')
        # ---- 2026-10-25 扩容：孤儿（D2）/ 结构化 blocked（D3′）/ 证据落地（D4′）/ 基线（D9）----
        os.makedirs(os.path.join(tmp, 'ev'), exist_ok=True)
        with open(os.path.join(tmp, 'ev', 'doc.md'), 'w', encoding='utf-8') as fh:
            fh.write('evidence\n')

        def orphan(blocked, name='orphan1', ledger=None, commit_check=None,
                   landed_check=None):
            """只留 row_ok + 一个孤儿场景，返回 (problems, undecidable)。"""
            for fn in os.listdir(scenes):
                os.remove(os.path.join(scenes, fn))
            json.dump({'format': 'run3/1', 'scenarios': [row_ok]},
                      open(run3, 'w', encoding='utf-8'), ensure_ascii=False)
            for nm, doc in (('b', {'name': 'b', 'steps': []}),
                            (name, {'name': name, 'steps': [], 'blocked': blocked})):
                json.dump(doc, open(os.path.join(scenes, nm + '.json'), 'w',
                                    encoding='utf-8'), ensure_ascii=False)
            try:
                probs, und, _i = check(
                    run3, scenes, None, table, repo=tmp,
                    ledger={} if ledger is None else ledger,
                    commit_check=commit_check if commit_check is not None else (lambda s: True),
                    landed_check=landed_check if landed_check is not None else (lambda s: False))
                return probs, und
            except UsageError as ex:
                return ('USAGE', str(ex))

        def structured():
            return {'reason': '为什么仍不进 run3', 'evidence': [
                {'kind': 'path', 'value': 'ev/doc.md'}]}

        # 牙 n1：孤儿无理由（两个不同实例）⇒ DRIFT 点名（rc=1），且不许是 UNCLASSIFIED
        for nm in ('orphanA', 'orphanB'):
            probs, und = orphan({'missing_fixture': 'x', 'reason': 'r'}, name=nm)
            chk(isinstance(probs, list)
                and any('not_in_run3_reason' in p and nm in p for p in probs)
                and und == [],
                '牙n1：孤儿无理由（%s）⇒ DRIFT 点名、无 UNCLASSIFIED' % nm)
        # 牙 n2：纯文本 reason（两个实例）⇒ UNCLASSIFIED（rc=2，判不了），不是 rc=1
        for nm in ('orphanC', 'orphanD'):
            probs, und = orphan({'missing_fixture': 'x', 'reason': 'r',
                                 'not_in_run3_reason': '就是跑不了'}, name=nm)
            chk(probs == [] and any('缺结构' in u or '纯文本' in u for u in und),
                '牙n2：not_in_run3_reason 纯文本（%s）⇒ UNCLASSIFIED' % nm)
        # 牙 n3：结构化 reason + 可解析 path 证据（两个实例）⇒ 绿
        for nm in ('orphanE', 'orphanF'):
            probs, und = orphan({'missing_fixture': 'x', 'reason': 'r',
                                 'not_in_run3_reason': structured()}, name=nm)
            chk(probs == [] and und == [],
                '牙n3：结构化 reason + path 证据（%s）⇒ 绿' % nm)
        # 牙 n4：commit 已是 master 祖先 ⇒ DRIFT；补结构化 still_blocked_because ⇒ 绿
        sha = 'a' * 40
        landed = {'not_in_run3_reason': {'reason': 'r', 'evidence': [
            {'kind': 'commit', 'value': sha}]}}
        probs, und = orphan({'missing_fixture': 'x', 'reason': 'r', **landed},
                            name='orphanG', landed_check=lambda s: True)
        chk(any('已落地' in p or 'master 祖先' in p for p in probs) and und == [],
            '牙n4：证据 commit 已落地 ⇒ DRIFT（证伪）')
        probs, und = orphan({'missing_fixture': 'x', 'reason': 'r', **landed,
                             'still_blocked_because': structured()},
                            name='orphanH', landed_check=lambda s: True)
        chk(probs == [] and und == [], '牙n4b：补 still_blocked_because ⇒ 绿')
        # 牙 n5：card 已 closed ⇒ DRIFT；在办 ⇒ 绿
        card_ev = {'not_in_run3_reason': {'reason': 'r', 'evidence': [
            {'kind': 'card', 'value': 'card-1'}]}}
        probs, und = orphan({'missing_fixture': 'x', 'reason': 'r', **card_ev},
                            name='orphanI', ledger={'card-1': 'closed'})
        chk(any('closed' in p for p in probs) and und == [], '牙n5：card 已 closed ⇒ DRIFT')
        probs, und = orphan({'missing_fixture': 'x', 'reason': 'r', **card_ev},
                            name='orphanJ', ledger={'card-1': 'in-progress'})
        chk(probs == [] and und == [], '牙n5b：card 在办 ⇒ 绿')
        # 牙 n6：新形态缺 missing/missing_fixture ⇒ UNCLASSIFIED
        probs, und = orphan({'reason': 'r', 'evidence': [
            {'kind': 'path', 'value': 'ev/doc.md'}]}, name='orphanK')
        chk(any('missing' in u for u in und),
            '牙n6：新形态缺 missing/missing_fixture ⇒ UNCLASSIFIED')
        # 牙 n7：evidence.kind 不在封闭枚举 ⇒ UNCLASSIFIED
        probs, und = orphan({'missing_fixture': 'x', 'reason': 'r',
                             'not_in_run3_reason': {'reason': 'r', 'evidence': [
                                 {'kind': 'file', 'value': 'ev/doc.md'}]}}, name='orphanL')
        chk(any('kind' in u for u in und),
            '牙n7：evidence.kind 不在封闭枚举 ⇒ UNCLASSIFIED')
        # 牙 n8：evidence 为空 ⇒ UNCLASSIFIED
        probs, und = orphan({'missing_fixture': 'x', 'reason': 'r',
                             'not_in_run3_reason': {'reason': 'r', 'evidence': []}},
                            name='orphanM')
        chk(any('evidence' in u for u in und), '牙n8：evidence 为空 ⇒ UNCLASSIFIED')

        # 牙 n9（D9 承重）：把仓库里**当前全部** blocked 场景喂进去——
        #   去掉新增的 not_in_run3_reason 后，不得因缺结构被批量 UNCLASSIFIED（孤儿按 D2 判红）；
        #   原样（已补结构化理由）⇒ 全绿。
        #   **计数不写死**：2026-10-27 新增两条 blocked（king_dialog/king_castle_war，卡
        #   openmir2-npc-click-no-response-family）时，旧断言 `== 7` 让整颗牙失效——改成与喂进来的
        #   集合逐名比对，条数随真实清单增长而增长（牙的语义不变：**每一个**都要被识别）。
        real_scenes = os.path.join(REPO_ROOT, DEFAULT_SCENES)
        real_run3 = os.path.join(REPO_ROOT, DEFAULT_RUN3)
        real_rows = load_run3(real_run3)
        base = os.path.join(tmp, 'base9')
        os.makedirs(base, exist_ok=True)
        blocked_names = []
        docs = {}
        for row in real_rows:
            nm = row['name']
            doc = json.load(open(os.path.join(real_scenes, nm + '.json'), encoding='utf-8'))
            if 'blocked' in doc:
                blocked_names.append(nm)
                docs[nm] = doc
        for fn in sorted(os.listdir(real_scenes)):
            if not fn.endswith('.json'):
                continue
            nm = fn[:-5]
            if nm in docs:
                continue
            d = json.load(open(os.path.join(real_scenes, fn), encoding='utf-8'))
            if 'blocked' in d:
                blocked_names.append(nm)
                docs[nm] = d
        rows9 = [r for r in real_rows if r['name'] in docs]
        run3_9 = os.path.join(tmp, 'run3-9.json')
        json.dump({'format': 'run3/1', 'scenarios': rows9},
                  open(run3_9, 'w', encoding='utf-8'), ensure_ascii=False)
        stripped = {}
        for nm, d in docs.items():
            d2 = json.loads(json.dumps(d, ensure_ascii=False))
            if isinstance(d2.get('blocked'), dict):
                d2['blocked'].pop('not_in_run3_reason', None)
            stripped[nm] = d2
        base_a = os.path.join(base, 'stripped')
        os.makedirs(base_a, exist_ok=True)
        for nm, d in stripped.items():
            json.dump(d, open(os.path.join(base_a, nm + '.json'), 'w',
                              encoding='utf-8'), ensure_ascii=False)
        probs9a, und9a, info9a = check(
            run3_9, base_a, None, BLOCKED_TOKENS, repo=REPO_ROOT, ledger={},
            commit_check=lambda s: True, landed_check=lambda s: False)
        chk(sorted(info9a['blocked_scene']) == sorted(docs) and not und9a
            and sorted(info9a['orphans']) == sorted(
                set(blocked_names) - {r['name'] for r in rows9})
            and any('not_in_run3_reason' in p for p in probs9a),
            '牙n9（D9）：%d 个既有 blocked 场景（抹掉新理由）⇒ 无 UNCLASSIFIED，'
            '孤儿按 D2 判红（%d 个）'
            % (len(docs), len(info9a['orphans'])))
        base_b = os.path.join(base, 'asis')
        os.makedirs(base_b, exist_ok=True)
        for nm, d in docs.items():
            json.dump(d, open(os.path.join(base_b, nm + '.json'), 'w',
                              encoding='utf-8'), ensure_ascii=False)
        probs9b, und9b, _i9b = check(
            run3_9, base_b, None, BLOCKED_TOKENS, repo=REPO_ROOT,
            ledger={'openmir2-gm-target-state-write': 'in-progress'},
            commit_check=lambda s: True, landed_check=lambda s: False)
        chk(probs9b == [] and und9b == [],
            '牙n9b（D9）：原样（结构化理由齐）⇒ 全绿')

    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    print('%s: blocked_sources_check selftest %d/%d'
          % ('GREEN' if not fail else 'RED', ok, ok + fail))
    return 0 if not fail else 1


# ---------------------------------------------------------------- main
def main(argv=None):
    ap = argparse.ArgumentParser(description='run3 blocked 两来源一致性机判（只读）')
    ap.add_argument('cmd', choices=('check', 'selftest'))
    ap.add_argument('--run3', default=DEFAULT_RUN3)
    ap.add_argument('--scenarios-dir', default=DEFAULT_SCENES)
    ap.add_argument('--list-output', help='`run_batch.sh --list` 的输出文件（可选）')
    args = ap.parse_args(argv)
    if args.cmd == 'selftest':
        return selftest()
    try:
        problems, undecidable, info = check(args.run3, args.scenarios_dir, args.list_output)
    except UsageError as ex:
        print('blocked_sources_check: %s' % ex, file=sys.stderr)
        return 2
    for u in undecidable:
        print('UNCLASSIFIED  %s' % u)
    for p in problems:
        print('DRIFT  %s' % p)
    print('清单 %s：%d 行，blocked %d；场景 %d 个，blocked %d，孤儿 %d'
          % (args.run3, info['rows'], len(info['blocked_run3']), info['scenes'],
             len(info['blocked_scene']), len(info['orphans'])))
    if problems:
        print('blocked_sources_check: DRIFT %d 处（另有 %d 处判不了）'
              % (len(problems), len(undecidable)))
        return 1
    if undecidable:
        print('blocked_sources_check: UNCLASSIFIED %d 处（判不了）' % len(undecidable))
        return 2
    print('blocked_sources_check: OK（两来源一致 + 孤儿/证据/--list 全过）')
    return 0


if __name__ == '__main__':
    sys.exit(main())
