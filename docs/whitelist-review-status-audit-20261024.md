# 白名单 `T-*` 的**复核状态**审计（`openmir2-whitelist-review-status-audit`）

- 卡：`openmir2-whitelist-review-status-audit`（tier=L）；分支 `card/whitelist-review-status-audit-69`
- **快照戳（本审计口径）**：被审 head `151a7e822` 的 `mir2-rs/tests/parity/whitelist.md`，**247** 个 `^| T-` 行。所有计数可用 §6 脚本复算；本审计**未改** whitelist 与任何卡片状态。
- **master 漂移说明**：`walgit/master = 2000e1cc5`（2026-10-24）时 whitelist 已增至 **249** 个 T 行（新增 `T-212`/`T-221`，`goal_gate --json` 的 `whitelist_t_rows=249`）。本审计的 245/224/247 只对 `151a7e822` 成立；新增行必须按 §6 脚本重算后再更新头条。
- **返工说明**：本版按 review `e7f30b33`（request_changes）重做 C-1～C-5：分类改为「行内 hex → `refs/collab` entry」的可复算口径并重算四档（§2/§3）、抽样逐条给 entry oid（§4）、订正 T-28/T-208（§4）、按「有登记 ≠ 已复核」拆 161 张口径（§5）、把分类脚本写进 §6（C-5）。

## 1. 先订正头条数字

| 口径 | 命令 | 结果 |
|---|---|---:|
| 含「待独立 principal 复核」的**所有行** | `grep -c '待独立 principal 复核' mir2-rs/tests/parity/whitelist.md` | **245** |
| 其中**属于 T-\* 登记表的行** | `grep '^\| T-' … \| grep -c '待独立 principal 复核'` | **224** |
| T-\* 行总数 | `grep -c '^\| T-' mir2-rs/tests/parity/whitelist.md` | **247** |

⇒ 卡面写的「**247 行里 245 行**带待复核字样」**不成立**：245 是**全文行数**，其中 **21 行是非 T 表行**（实测构成：`D-9`/`D-10`/`D-13` 3 行 + `B-16`/`B-116`/`B-117` 3 行 + `S4B-1`…`S4B-10` 10 行 + `M1-EXIT-1`/`M1-EXIT-2` 2 行 + 2 条引用块行 + 1 条续行 = **21**）。
正确的说法是：**247 行 T-\* 里 224 行（90.7%）带「待独立 principal 复核」。**

> 交叉验证：`python -I tools/goal/goal_gate.py --json` 在快照 `2000e1cc5` 报 `whitelist_t_rows=249`（不是 247），与本节的快照边界一致——头条数字必须带 T 行数一起引。

## 2. 可复算方法（行内 hex → `refs/collab` entry）

**为什么旧口径不可复算**：旧版把「含待复核字样」当短路条件、把「有 8–40 位 hex」直接判成「有引用，再人读」，既没有把 hex 解析成 entry，也没有把「字样」与「可核 review」当正交字段，导致 T-28（行内 `127d67b1` 是 reviewer-5 的 approve）、T-208（`bdddbeba` 是另一张卡的 issue）判错。新口径如下（§6 脚本即此实现）：

1. 取 `^| T-` 行（247 行）；对每行抽 8–40 位十六进制 token（正则 `(?<![0-9a-fA-F])[0-9a-fA-F]{8,40}(?![0-9a-fA-F])`）。
2. 对每个 token，在 `refs/collab/inbox/*` 的 **blob objectname**（40 位内容哈希）里按前缀匹配；**只**有 collab entry 的 blob oid 能被解析，commit oid（如 `505f8d16`/`376ea004`/`4d276812`）不在 `refs/collab` 里 ⇒ 记「不可核」。
3. 命中的 blob 用 `git cat-file -p <blob>` 解析 JSON，取 `kind` / `actor` / `id`（卡）/ `body.decision` / `body.verdict`；ref 路径 `refs/collab/inbox/<principal>/…` 交叉核对 actor。
4. 两个**正交**字段：`has_review_oid` = 至少一条解析到 `kind=review` 且 `actor` ≠ 本行作者；`has_phrase` = 行内是否含「待独立 principal 复核」。作者从行内「作者登记（`worker-N`）」/「作者 `openmir2-worker-N`」提取。
5. 分档（互斥、合计 247）：**① 有可核的非作者 approve** > **①′ 有可核 review（request_changes）但无 approve** > **② 仅字样** > **③ 无痕**。
6. `verified` 由 `walgit collab thread <卡>` 的 `verified` 字段给出（§4 抽样的全部条目均 `verified=true`）。

**四档复算结果（与审查者独立复算逐号一致）**：

| 档 | 定义 | 行数 | T 号 |
|---|---|---:|---|
| ① | 行内 hex 解析到**非作者的 review approve** | **8** | `T-17`、`T-18`、`T-28`、`T-30`、`T-36`、`T-44`、`T-209`、`T-213` |
| ①′ | 行内 hex 解析到 **review（`request_changes`）**，但没有可核 approve | **8** | `T-6`、`T-7`、`T-192`、`T-196`、`T-205`、`T-206`、`T-219`、`T-222` |
| ② | 只有「待独立 principal 复核」字样，hex 不解析到 review | **212** | 其余全部；含 `T-227`…`T-262` 全部 36 行 |
| ③ | 既无该字样，也无任何可解析到 review 的 oid | **19** | `T-1`、`T-5`、`T-71`、`T-104`…`T-117`、`T-172`、`T-208` |

合计 **8 + 8 + 212 + 19 = 247** ✓（旧版 ①3/①′2/②224/③18 不成立；「只有 T-213 能追到 approve」也不成立。）

> **正交核对**：224 带字样 = 212 仅字样 + **12** 行「既有字样、又有可核 review」（T-17/T-18/T-30/T-36/T-44/T-192/T-196/T-205/T-206/T-209/T-219/T-222）。因此「字样」不能当短路条件，必须与 `has_review_oid` 分开统计。

## 3. 四档清单（可逐号复核）

**① 有可核的非作者 approve（8 行）**——行内 hex → entry：

| 行 | 行内 hex | entry（blob 前缀） | kind / decision | actor | 条目 `id`（卡） | 行作者 |
|---|---|---|---|---|---|---|
| `T-17` | `f069955a` | `f069955a` | review / approve | `openmir2-reviewer-6` | `openmir2-m3-bag-equip-alignment` | worker-4 |
| `T-18` | `33bc9ae4` | `33bc9ae4` | review / approve | `openmir2-reviewer-2` | `openmir2-m3-bag-equip-alignment` | worker-4 |
| `T-28` | `127d67b1` | `127d67b1` | review / approve | `openmir2-reviewer-5` | `openmir2-m3-recovery-tick` | worker-4 |
| `T-30` | `3a395d4c` + `bb5c858b` | `3a395d4c` / `bb5c858b` | review / approve（两票） | reviewer-5 / reviewer-6 | `openmir2-m3-rust-seed-injection` | worker-4 |
| `T-36` | `10aeeae0` | `10aeeae0` | review / approve | `openmir2-reviewer-5` | `openmir2-m3-mail-auction-align` | owner 裁决行（`1bd1361c`），无 worker 作者 |
| `T-44` | `7a575abe` | `7a575abe` | review / approve | `openmir2-reviewer-5` | `openmir2-m3-selloff-fixture-botsrv` | worker-5 |
| `T-209` | `8ab09811` | `8ab09811` | review / approve（`verdict=approve`） | `openmir2-reviewer-1` | `openmir2-gm-reachability-registry` | worker-60 |
| `T-213` | `f6509d9a` | `f6509d9a` | review / approve（`decision`+`verdict`） | `openmir2-reviewer-5` | `openmir2-script-action-var-text` | worker-68 |

> `T-36` 的 approve 是对「盘点报告片」的 `10aeeae0`，本行仍是 owner §6.0-3 裁决；它证明「有可核非作者 approve」，**不**等于本行已完成行级复核。其余 7 行 approve 的 actor 均为 reviewer，作者均为 `worker-4/5/60/68`（T-36 无 worker 作者），故均为**非作者**。

**①′ 有可核 review（`request_changes`），无 approve（8 行）**：

| 行 | 行内 hex | entry（blob 前缀） | decision / verdict | actor | 卡 | 行作者 |
|---|---|---|---|---|---|---|
| `T-6` | `a9e5ec0d` | `a9e5ec0d` | request_changes | `openmir2-reviewer-4` | `openmir2-c6-e2e-loadgate` | worker-5 |
| `T-7` | `a9e5ec0d` | `a9e5ec0d` | request_changes | `openmir2-reviewer-4` | `openmir2-c6-e2e-loadgate` | worker-5 |
| `T-192` | `1de72e17` | `1de72e17` | verdict=request_changes | `openmir2-reviewer-1` | `openmir2-castle-merchant-fixedhide-hit` | worker-29 |
| `T-196` | `2c5cb955` | `2c5cb955` | verdict=request_changes | `openmir2-reviewer-2` | `openmir2-gm-admin-mode-entity-seam` | worker-38 |
| `T-205` | `39262b65` | `39262b65` | verdict=request_changes | `openmir2-reviewer-9` | `openmir2-map-event-frames` | worker-49 |
| `T-206` | `8c4da05a` + `48eae6ce` | `8c4da05a` / `48eae6ce` | verdict=request_changes（两条） | `openmir2-reviewer-14` | `openmir2-sm-skeleton-wiring` | worker-53 |
| `T-219` | `f015343d` | `f015343d` | verdict=request_changes | `openmir2-reviewer-14` | `openmir2-gm-impl-startquest` | worker-24 |
| `T-222` | `24132993` | `24132993` | request_changes | `openmir2-reviewer-5` | `openmir2-script-dyn-var-host` | worker-24 |

**③ 无痕（19 行）**：`T-1`、`T-5`、`T-71`、`T-104`、`T-105`、`T-106`、`T-107`、`T-108`、`T-109`、`T-110`、`T-111`、`T-112`、`T-113`、`T-114`、`T-115`、`T-116`、`T-117`、`T-172`、`T-208`。
其中 `T-208` 的 `bdddbeba` **不是**「查不到的 oid」——它是 `openmir2-orchestrator` 在卡 `openmir2-script-sendmsg-host` 上的 **`kind=issue`** 条目（`verified=true`），不是 review，故本行仍归「无痕」。

**② 仅字样（212 行）**：其余全部；其中 `T-227`…`T-262` 恰 **36** 行且**全部**带「待独立 principal 复核」（审查者已核实，本节复算一致），历史部分 176 行（211 − 8 − 8 − 19）。

## 4. 抽样核（8 行，逐条给 review/issue oid）

| 行 | 行内 hex | 解析出的 entry | 结论 | verified |
|---|---|---|---|---|
| `T-17` | `f069955a` | review approve，`openmir2-reviewer-6`，卡 `openmir2-m3-bag-equip-alignment` | **①**（非作者 approve） | true |
| `T-28` | `127d67b1` | review approve，`openmir2-reviewer-5`，卡 `openmir2-m3-recovery-tick` | **①**（不是旧版说的「无 reviewer 字样/只是 blob」） | true |
| `T-30` | `3a395d4c` + `bb5c858b` | review approve（reviewer-5 / reviewer-6），卡 `openmir2-m3-rust-seed-injection` | **①**（两票独立 approve） | true |
| `T-36` | `10aeeae0` | review approve，`openmir2-reviewer-5`，卡 `openmir2-m3-mail-auction-align`（另 `1bd1361c` = owner comment） | **①**；approve 指向报告片，不等于本行行级复核 | true |
| `T-6` | `a9e5ec0d` | review `request_changes`，`openmir2-reviewer-4`，卡 `openmir2-c6-e2e-loadgate` | **①′**；该卡另有 reviewer-3 approve `d3583dc7`，但**行内未引用** | true |
| `T-44` | `7a575abe` | review approve，`openmir2-reviewer-5`，卡 `openmir2-m3-selloff-fixture-botsrv` | **①** | true |
| `T-213` | `f6509d9a` | review approve（`decision=approve`、`verdict=approve`），`openmir2-reviewer-5`，卡 `openmir2-script-action-var-text` | **①** | true |
| `T-208` | `bdddbeba` | **issue**，`openmir2-orchestrator`，卡 `openmir2-script-sendmsg-host` | **③**（不是 review；行文本身写「承接卡 `openmir2-script-sendmsg-host`（issue `bdddbeba`）」） | true |

> 复跑：`walgit collab thread <卡>` 查 `verified`；`git cat-file -p <blob 前缀>` 查 `kind/actor/id/body.decision`。

## 5. 与「登记型 PASS 161 张」的对应（C-4）

`python -I tools/goal/goal_gate.py --json` 在快照 `2000e1cc5`（2026-10-24）的条件② detail 原文：

> `merged 未 closed 但已凭登记 PASS：161 张（卫生清单，见 evidence.merged_open_hygiene；登记型 PASS ≠ 已有真机证据）`

按本审计口径拆开两个可计数口径：

| 口径 | 计数 | 含义 |
|---|---:|---|
| **有登记**（登记型 PASS） | **161 张** | `goal_gate` 的卫生清单口径；**不等于**已复核 |
| **有行级 review** | **16 行**（8 approve + 8 request_changes） | 行内 hex 可解析到 `kind=review` |
| **无行级复核** | **231 行**（212 仅字样 + 19 无痕） | 未能在行内追到 review entry |

⇒ 「登记型 PASS 161 张」**只能按「有登记」计，不能按「已复核」计**。B/C 方向可执行，但 B 的工作量应写成「**212 行仅字样 + 19 行无痕 = 231 行**」，优先给 8 行已有 approve 的补引用；不要把 161 当成 reviewed 数。

## 6. 复算脚本（可直接从仓库重放，C-5）

前置：`git fetch walgit '+refs/collab/inbox/*:refs/collab/inbox/*'`（或本 worktree 的 `refs/collab` 已同步）。在仓库根目录运行：

```sh
python -I - <<'PY'
import subprocess, json, re

def git(*a): return subprocess.run(["git"]+list(a), capture_output=True).stdout

refs=[]
for ln in git("for-each-ref","--format=%(refname) %(objecttype) %(objectname)","refs/collab").decode().splitlines():
    ref, otype, oid = ln.split()
    if otype == "blob": refs.append((ref, oid))

p=subprocess.Popen(["git","cat-file","--batch"], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
out,_=p.communicate("".join(o+"\n" for _,o in refs).encode())
entries={}; pos=0
for ref,oid in refs:
    nl=out.find(b"\n",pos); size=int(out[pos:nl].split()[2])
    body=out[nl+1:nl+1+size]; pos=nl+2+size
    try: e=json.loads(body.decode("utf-8"))
    except Exception: e=None
    entries.setdefault(oid,[]).append((ref,e))

def resolve(tok):
    return [(oid,ref,e) for oid,lst in entries.items() if oid.startswith(tok.lower())
            for ref,e in lst if isinstance(e,dict)]

hexre=re.compile(r"(?<![0-9a-fA-F])[0-9a-fA-F]{8,40}(?![0-9a-fA-F])")
buckets={"approve":[],"review":[],"phrase":[],"none":[]}
rows=[l for l in open("mir2-rs/tests/parity/whitelist.md", encoding="utf-8") if l.startswith("| T-")]
for l in rows:
    tid=l[2:].split("|")[0].strip()
    m=re.search(r"作者登记（`?(?:openmir2-)?worker-\d+", l) or re.search(r"作者 `(openmir2-worker-\d+)", l)
    author=m.group(0).split("（")[-1].strip("`") if m else None
    if author and author.startswith("worker-"): author="openmir2-"+author
    found=[e for t in hexre.findall(l) for _,_,e in resolve(t)]
    app=[e for e in found if e.get("kind")=="review" and e.get("actor")!=author
         and ((e.get("body") or {}).get("decision")=="approve" or (e.get("body") or {}).get("verdict")=="approve")]
    rev=[e for e in found if e.get("kind")=="review"]
    if app: buckets["approve"].append(tid)
    elif rev: buckets["review"].append(tid)
    elif "待独立 principal 复核" in l: buckets["phrase"].append(tid)
    else: buckets["none"].append(tid)

for k in ("approve","review","phrase","none"):
    print(k, len(buckets[k]), buckets[k] if k!="phrase" else "(list omitted)")
print("sum", sum(len(v) for v in buckets.values()))
PY
```

期望输出（快照 `151a7e822`，247 行）：`approve 8 / review 8 / phrase 212 / none 19 / sum 247`，且 ①/①′/③ 的 T 号与 §3 逐号一致。头条三条 `grep` 命令仍可复算 245/224/247。

## 7. 建议（工作量按新口径）

- **已可引用（8 行）**：`T-17/T-18/T-28/T-30/T-36/T-44/T-209/T-213` 行内 hex 已解析到非作者 approve，只需把 entry oid 补成「复核引用」格式即可计入 reviewed。
- **先解阻塞（8 行）**：`T-6/T-7/T-192/T-196/T-205/T-206/T-219/T-222` 行内是 `request_changes`；先把对应卡的返工/复审做完，再引用新的 approve。
- **需补复核（231 行 = 212 仅字样 + 19 无痕）**：B 的工作量按 231 行计；其中 `T-227`…`T-262` 共 36 行可整批补（四批刚被 reviewer 看过，只差引用），历史 195 行按批抽样裁定。
- **口径（C）**：把「登记 + `reviewed: <oid>`」拆成两个字段——登记型 PASS 只表示「有登记」，`reviewed` 才表示「已复核」；161 张登记型 PASS 不应再隐含已复核。

## 8. 残留物

- 本版把分类脚本内联到 §6，**仓库内可独立重放**；不再依赖 `/e/tmp/wl_audit.py`。
- 本次审计的工作产物仅有本文件（写集 `docs/`）；未改 `whitelist.md`、工具或他人状态。
