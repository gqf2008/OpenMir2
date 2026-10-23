# 需要 owner 决策/授权的事项（2026-10-24 刷新）

> **快照戳**：`master = 55a8e7fca`（2026-10-24，返工 v2；此前两版分别基于 `8dcb8c8ca` / `7bb48239a`）。本文件取代 `docs/needs-human-20261023-night.md`
> （后者保留为只读历史；里面多条结论已过期，见各节「旧结论订正」）。
> 所有数字都可用节内命令现算；引用别处数字的均标了来源。**本文件只列循环内无权自决的事**。
> 判级口径按 AGENTS.md「判据分级」；本卡为文档类改动，`tier=L`。

## 0. 现在只需做 5 件事

| # | 一句话 | 预计成本 | 不做的后果 |
|---:|---|---|---|
| 1 | **窗口授权**：批一轮真机窗口（或指定车道去跑），先跑 60 秒 `preflight`，再按 §1.2 逐场景跑 | 一轮 ≈ 240 GB 磁盘写入 + 数十分钟（`docs/window-runbook-20261023.md:42`）；前置见 §1 | 条件② 的 **123** 张 `window_required`（其中未合并 2）证据永远拿不到；`merged 未 closed` 的 **172** 张只能继续「登记型 PASS」挂着 |
| 2 | **`rules.json` 裁定**：47 条 PENDING = **44 条可批 / 3 条需返工或 owner 明确裁定**（旧文档的 33/10 已不成立） | 1 次评审 + 1 个提交；全批要同步改 **22 条** selftest 断言（否则 240/262 红），只解 ident 14 只需改 **1 条**（261/262） | 每轮窗口报告都带 `rules_pending_review=true`，不能当 R 级已复核证据；两侧一旦出现 `SM_HIT(14)` 整轮判 `UNDECIDABLE(rc=2)` |
| 3 | **`T-161..T-222` 复核口径**：58 行里只有 2 行有可核的非作者 approve | 一句话定口径（§4 给 3 个选项）；逐号复核则至少看 5 个文件 + 8 张卡 | 条件② 的 whitelist 支永远停在「待复核」；`goal_gate` 卫生清单只能按「有登记」记账，不能按「已复核」 |
| 4 | **三张卡的替代判据**：`run-gap-checkdupobj` / `disappear-recipients-gridscan` / `enter-map-cell-rejected-vs-oracle` 各等一句话 | 每张 1 句话（§3 给了每张的两个选项） | 三张永久挂 `in-progress`，`goal_gate` 的「需人工 10 张」常驻 |
| 5 | **私有 C# 栈去留**：`E:/tmp/conformance/csharp-root` 是否长期保留（低优先级） | 保留=0；归档/退役需先安排重建 | 不表态=默认保留，占 `E:/tmp/conformance` 共 **699 MB** |

**从 owner 清单里可以关闭的两条**（详见 §5）：`@upgradenow/@getbackupgnow` 已实现（`35c039c8d`）；
`DoDamageWeapon/642` 已合入 master `8dcb8c8ca`（复审 `dbcf22f8e`，R 卡窗口未跑）——两者都**不再需要 owner 一句话**。

> 另有一条**不是 owner 的事**：`openmir2-ability-weaponstrong` 已随 `d38acf25b` 合入 master（tier=R），
> `goal_gate` 里现为 `state=pass / category=window_required / merged=true`（不是 FAIL，也不再是
> `window_required_unmerged`）；该卡按 R 级规则保持 `in-progress` 等真机窗口，归入 §1 的窗口排期。

---

## 1. 真机窗口（唯一能推进条件② 的钥匙）

### 现状

- 循环硬约束是「不起服务端、不占端口」，窗口只能由被授权的人/车道跑。
- **条件② 当前是 `need_human`，FAIL 卡 0，三者皆无 0**（本卡在 master `55a8e7fca` 实跑，原文见 §1.5）。
  `openmir2-ability-weaponstrong` 现为 `state=pass / category=window_required / merged=true`（已合入 `d38acf25b`，
  R 卡等真机窗口），**不是 FAIL**。
- **这次「假 FAIL」的因果要记下来（本卡第一版就踩了）**：Leader 建 `openmir2-ability-weaponstrong` 卡时 issue body
  只写了 `tier=R`、**没写 `window_required`** ⇒ `goal_gate` 把它归入「三者皆无」⇒ 条件② 从 `need_human` 掉成 `fail`；
  交付后 Leader 用签名 comment `c7f43850`（`verified=true`，`body.window_required=true`）补登记，条件② 才修回 `need_human`。
  `goal_gate` 扫**任意 kind** 的 `body.window_required`，一条 comment 即可消除「三者皆无」。
  快照：`master=55a8e7fca`，`generated_ts=1792791977`；R 卡 198 张、`window_required=123`（其中未合并 2）、
  `merged 未 closed 但已凭登记 PASS=172`、`whitelist_t_rows=255`。
  （R 卡计数与「未开工/未合并」随 collab 看板漂移：本卡窗口内新开出 `openmir2-weaponstrong-dura-zero-skip`
  （open/待认领，不计缺口），使 R 卡 197→198、未开工 0→1；复算一律以命令现取为准。）
- 工具已就绪：`preflight.py` 已合入 master（最新 commit `deea8d3d0`，`git merge-base --is-ancestor deea8d3d0 master` 为真）；
  `docs/window-batch-plan-20261023.md` §7.1 里「preflight 在办（未合）」**已过期**。
- 场景：`tools/probe/scenarios/` 现 20 个，2 个仍 `blocked`（`combat_hit` / `combat_strike`）；
  `spell_cast` 已解封（master `7bb48239a`）但因 Rust 侧缺魔法帧仍不在可跑集合内，`equip_struck_save` 因 `642`
  合入（`8dcb8c8ca`）解锁。**「今天能跑」的集合不在本文件定义**——逐行枚举见 §1.2 引用的 batch-plan §7.4。

### 证据

- 卡 `openmir2-spell-cast-unblock` 已 `closed`，master `7bb48239a` 是其合并提交（`git log --oneline -1`）。
- `python -I -B tools/conformance/preflight.py --selftest` → `preflight selftest: OK (13 checks)`，rc=0（本卡实跑）。
- `python -I -B tools/conformance/compare_runs.py selftest` → `GREEN: compare_runs selftest 262/262`（本卡实跑）。
- 场景 `blocked` 计数：`grep -l '"blocked"' tools/probe/scenarios/*.json` → 仅 `combat_hit.json`、`combat_strike.json`。
- 内置 `alt` profile 无 `magic` 段：`card_window.py:290-338`（`BUILTIN_PROFILES["alt"]`，本快照行号）；
  场景 `spell_cast` 自 `cecc3fe41` 起声明 `"requires": ["magic"]`，缺夹具即 fail-closed。

### 还差什么

1. owner 授权（或指派车道）开窗；2. 开窗前 60 秒自查必须 rc=0；3. 逐场景产物归档 + 独立 reviewer 复核；
4. R 级卡窗口后**仍不 closed**（保持 `in-progress` 并在 `work` 写明窗口项）——这是 AGENTS.md 的硬规则。

### 1.5 条件② 原文（本卡在 `55a8e7fca` 实跑，`generated_ts=1792791977`；逐字复制，未删节）

```sh
python -I -B tools/goal/goal_gate.py --json
```

```
overall = need_human（需人工）
conditions[1].id = 2
conditions[1].state = need_human
conditions[1].detail = R 卡 198 张：已完成 11 / window_required 123（其中未合并 2） /
  whitelist 48 / 已裁定登记 3 / comment 证据 0 / 三者皆无 0 /
  merged 未 closed 且无登记(需人工) 5 / 未开工或未合并 1（不计缺口）/ 其它需人工 5；
  需人工 10 张（按 last_status_ts 最旧优先，见 JSON need_items）；
  merged 未 closed 但已凭登记 PASS：172 张（卫生清单，见 evidence.merged_open_hygiene；登记型 PASS ≠ 已有真机证据）
fail_items = []          # FAIL 卡 0
conditions[1].evidence.whitelist_t_rows = 255
```

- 「**未开工或未合并 1（不计缺口）**」与「**其它需人工 5**」是两行：前者**不计入**「需人工 10 张」（10 = 5 + 5）；
  这 1 张是本次快照窗口内新开的 `openmir2-weaponstrong-dura-zero-skip`（`board_status=open`，无 patch/merge_result）。
- `openmir2-ability-weaponstrong` 在 `evidence.r_cards` 里是
  `state=pass / category=window_required / merged=true / window_required=true / strong_t=["T-266"] / weak_t=["T-141","T-267"] / board_status=in-progress`
  ——**不是 FAIL**（已随 `d38acf25b` 合入，R 卡等窗口）。它当初被修回 PASS 靠的是 Leader 的签名 comment `c7f43850`（`body.window_required=true`）。

### owner 的选项（一句话）

- **A**：批一轮窗口，按 §1.2 引用的 batch-plan §7.4 枚举（**10 行 / 15 个场景**，`@ a42171d04`）逐场景跑——成本与前置见下表；
- **B**：只批最小验证集（`login_only` + `upgrade_retrieve` + `equip_struck_save`，三者均在上述枚举内），其余下一轮；
- **C**：暂不开窗，先做不占窗口的清理项（§2 `rules.json` 最小解阻 / §4 `T-161..T-222` 复核口径 / §3 三张卡的替代判据）；
  `openmir2-ability-weaponstrong` 的合入阻塞已解除（`d38acf25b`），不再是开窗前置。

**不要**把「开不开窗」交给执行者判断——这是 owner 的决定，本文件只给成本与前置。

### 1.1 第①步：60 秒自查（可直接执行）

```sh
cd E:/Users/gxh/Documents/GitHub/OpenMir2     # 或任意 master=55a8e7fca 的检出
python -I -B tools/conformance/preflight.py --profile alt
```

它检查 5 件事（`tools/conformance/preflight.py:1-24`）：

| 步骤 | 检查内容 | FAIL 的含义 |
|---|---|---|
| `docs` | runbook 里有 `probe_timeout_ms`、没有 profile 的 `"timeout_ms"`（旧键会被静默忽略） | 文档口径漂了，先按 runbook §0.5 第 0 步改回 |
| `binary` | 委托 `tools/m2-acceptance/start_mir2_server.ps1 -WhatIf`，解析 `SERVER_EXE/SHA256/BUILD`；必须 `Release` 且 sha256 与本地重算一致 | Rust 二进制不是 release 或被换过；`cargo build --release -p mir2-server` |
| `ports` | 从 profile 的 `sides[*].ports` 逐个 connect 探测，要求全空（不写死端口表） | 上一轮栈没停干净，先停栈 |
| `profile` | `probe_timeout_ms` 为正整数；`require_pid_binaries` 的侧必须有 `pid_file` | 补 profile 键 |
| 可选 | `--with-dry-run` / `--with-selftest` 才委托 `card_window`（默认不跑） | 同对应命令 |

rc=0 = `PREFLIGHT PASS`；rc=2 = `PREFLIGHT FAIL`（FAIL 行自带修复建议）。**只读，不起栈、不连库。**

### 1.2 第②步：逐场景窗口命令

前置（只设一次）：

```sh
export CONFORMANCE_PASSWORD='<窗口测试账号密码>'     # PowerShell: $env:CONFORMANCE_PASSWORD='...'
```

**今天就能跑**：**本文件不自行计数**，一律引用 `docs/window-batch-plan-20261023.md` §7.4（枚举出处 `a42171d04`，
该文件此后未改；master `55a8e7fca` 上逐行核对为 **10 行 / 15 个场景**）。下面按该表原样列出：

```sh
for s in login_only pickup_item move_walk upgrade_retrieve equip_struck_save \
         dice_chain dice_chain_auto npc_click npc_click_auto \
         king_dialog merchant_dialog npc_script_chain king_castle_war \
         gm_time gm_batch; do
  python -I -B tools/conformance/card_window.py \
    --scenario tools/probe/scenarios/$s.json \
    --out E:/tmp/conformance/runs/20261024/$s \
    --profile alt
done
```

- 带 `required` 的场景（`npc_click*` / `merchant_dialog` / `king_dialog` / `npc_script_chain` / `move_walk` / `pickup_item` / `upgrade_retrieve`）
  会**自动先跑 discovery**；推不出参数即 fail-closed（不会拿空参数硬跑）。
- `upgrade_retrieve` 的夹具三件套缺一必红：**武器在手、金币 ≥ 10000、背包有黑铁矿**（场景 note 原文）。
- `equip_struck_save` 现在可跑：`642` 已合入 master `8dcb8c8ca`；本卡实测
  `python -I -B tools/conformance/card_window.py --scenario tools/probe/scenarios/equip_struck_save.json --out E:/tmp/nh24-equip-dry --profile alt --dry-run` → rc=0、
  `python -I -B tools/probe/mir2_probe.py --scenario tools/probe/scenarios/equip_struck_save.json --check-scenario` → `scenario OK` rc=0
  （`--check-scenario` 是开关，场景必须另给 `--scenario`；写成 `--check-scenario <file>` 会 `unrecognized arguments`）。
  摆位 map `0103` (19,19)、目标正上方 (19,18) 商店 NPC「卫家店」（场景自带 `placement`）。
  注意：场景 `description` 已随 `8dcb8c8ca` 改写为「Rust 已接这条磨损帧」，与 master 代码一致（`combat.rs` 有 `attack_success_weapon_wear`）；上位旧说法只作历史对照。
- R 卡记账用 `--post-comment --card <卡 id>`（一次 run 可对多张卡各发一条 comment；卡 id 见 batch-plan §3 的「场景」列）。

**`spell_cast` 不在可跑枚举内（batch-plan §7.4 末尾「仍不可跑」，本文件不另立计数）**

batch-plan 的排除理由是：Rust 侧魔法落点 + 第二观察者仍未接 ⇒ 真机必超时。场景自 `cecc3fe41` 起声明
`"requires": ["magic"]`，**默认 `alt` profile 现在直接 rc=2 fail-closed**（不再是静默假绿）：

```sh
# 默认 alt 没有 magic 段 → 明确拒绝（本卡在 master 55a8e7fca 实跑）
python -I -B tools/conformance/card_window.py --scenario tools/probe/scenarios/spell_cast.json \
  --out E:/tmp/conformance/runs/20261024/spell_cast --profile alt --dry-run
# → rc=2「card_window.py: 场景 `spell_cast` 要求 `magic` 夹具，但 profile `alt` 未声明 `magic` 段…」

# 生成带 magic 的 profile（不改仓库）后 dry-run 才会打印两侧各一行「魔法夹具… magic_ids=1」
python -I -B -c "import json,sys;sys.path.insert(0,'tools/conformance');import card_window as c;p=json.loads(json.dumps(c.BUILTIN_PROFILES['alt']));p['magic']=[{'magic_id':1,'level':0,'use_key':'1','curr_train':0}];json.dump(p,open('E:/tmp/conformance/spell-profile.json','w',encoding='utf-8'),ensure_ascii=False,indent=1)"
python -I -B tools/conformance/card_window.py --scenario tools/probe/scenarios/spell_cast.json \
  --out E:/tmp/conformance/runs/20261024/spell_cast --profile E:/tmp/conformance/spell-profile.json --dry-run
```

> **订正（2026-10-24，本卡 v2）**：上一版写「`--profile alt --dry-run` 同样 rc=0、输出里没有魔法夹具行 = 假绿」；
> 在 `cecc3fe41`（场景 `requires` 前置 fail-closed）之后**已不成立**——现在缺夹具是 rc=2 明确拒绝，不是静默跳过。
> 以本卡实跑为准：alt → rc=2；带 magic profile → rc=0，两侧各一行 `magic_ids=1`。
> 真跑若等不到 `SM_MAGICFIRE(638)`/`SM_MAGICFIRE_FAIL(639)`，按 `UNDECIDABLE` 记录，**不要改判据**。

**仍不可跑（别塞进本轮）**：

| 场景 | 原因 | 什么时候能跑 |
|---|---|---|
| `combat_hit` / `combat_strike` | 场景自身 `blocked`；`card_window` 见到 `blocked` 即 rc=2、不建 out、不起栈 | 需第二会话/同格怪夹具（`combat_hit`）、满足 DC/HitPoint 的配装或 `wait_for` recog 过滤（`combat_strike`） |
| `bag_equip` / `eat_item` | 背包夹具 `inventory` 已进 master（`bb4317a01`），但这两个场景**没有**声明 `inventory`，`required` 又要从 201 帧发现非空背包 | 场景补 `inventory` 声明后（另卡） |
| `spell_cast` | Rust 侧魔法落点 + 第二观察者未接（场景已解封且 `requires:["magic"]`）；见上一节 | 见 batch-plan §7.3/§7.4 |
| `castle_zone` | 仓库内无该场景文件（不可达登记） | 双行会 DB + 私有根 `AttackSabukWall.txt` + 多会话探针 |

### 1.3 第③步：证据落点与复核

- 每次 run 落 `<out>/`：`report.json`（verdict/rc）、`<side>/frames.ndjson`、`<side>/db/`（库快照）、
  用了 magic 时还有 `<side>/magic.json`（`declared/before/after/sql`）。
- 复核人：**与作者不同的 principal**，在**钉住的 commit** 上重跑同一命令并核 `report.json` 的 verdict、
  帧数、二进制 sha256；不要采信执行者的口头结论（AGENTS.md §1-4）。
- 记账：R 卡在卡上 `--post-comment`，状态保持 `in-progress`（R 级不 closed）；
  面向人的汇总按 AGENTS.md 写进 `docs/`，不要只留在 thread body。

### 1.4 第④步：失败时的停栈/回滚口径

```sh
# 1) 停两侧栈（与 alt profile 的 stop 命令一致）
powershell -NoProfile -ExecutionPolicy Bypass -File E:/tmp/conformance/stop-rust.ps1
powershell -NoProfile -ExecutionPolicy Bypass -Command "Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -like 'E:\tmp\conformance\csharp-root\*' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }"

# 2) 回滚两侧测试库（只碰 mir2_test_*；绝不碰生产库，也**不要停 mysqld**）
bash E:/tmp/conformance/restore-baseline.sh
bash E:/tmp/conformance/restore-baseline-r.sh

# 3) 确认端口已空、可以重来
python -I -B tools/conformance/preflight.py --profile alt
```

- `report.json` 为 `UNDECIDABLE(rc=2)`：**记为判不了**，把原因（缺产物/UNCLASSIFIED/pending_ignore_rule…）原样记卡，
  不许降低判据或改场景来凑绿。
- 禁 `--allow-production-db`（`csharp-default` profile 默认拒绝，指向生产库 `mir2_db`）。
- 失败 run 的 out 目录保留（证据），标「invalid」，不要当成 PASS 的输入再复用。

### 旧结论订正

- 旧文档说「条件② 已转 `need_human`、FAIL 卡 0」→ **这个结论现在成立**（`need_human` / FAIL 0 / 三者皆无 0）。
  本卡第一版曾写「现在 overall=fail、唯一 FAIL 卡 `openmir2-ability-weaponstrong`」，那是**假 FAIL**：
  建卡 issue body 缺 `window_required` → 被归「三者皆无」→ 条件② 掉成 `fail`；Leader 补签名 comment `c7f43850`
  后修回。**教训：判定条件② 前先看 `body.window_required` 是否落在任意一条签名条目上，不要只读 issue 标题/正文。**
- 旧文档把 `spell_cast` 归为「已解封但真机跑不了」→ 场景确已解封，但仍在 batch-plan §7.4 的「仍不可跑」，
  **不在本轮 15 个场景内**：Rust 侧魔法落点 + 第二观察者未接，真机必超时。另：缺 `magic` 夹具现在自 `cecc3fe41`
  起是 rc=2 fail-closed（`"requires": ["magic"]`），**不是**上一版说的静默假绿。
- 旧文档的「`preflight.py` 未合、按手工清单自查」→ `preflight.py` 已在 master（`deea8d3d0`）。

---

## 2. `rules.json` 的 47 条 PENDING

### 现状（现算，不抄旧数）

命令（离线，任意 master 检出）：

```sh
python -I -B -c "import json;d=json.load(open('tools/conformance/rules.json',encoding='utf-8'));rows=[('global[%d]'%i,r) for i,r in enumerate(d.get('global',[]))]+[('rules[%d]'%i,r) for i,r in enumerate(d.get('rules',[]))]+([('db',d['db'])] if bool(d.get('db')) else []);p=[(w,r.get('ident','-'),r.get('action'),r.get('tier')) for w,r in rows if str(r.get('approved_by','')).upper().startswith('PENDING')];print('PENDING',len(p),'/',len(rows))"
```

| 口径 | 现算值 | 说明 |
|---|---:|---|
| PENDING 总数 | **47** | `global` 4 + `rules` 42 + `db` 1（旧文档 43 = 4+38+1，已过期） |
| **可批（证据充分）** | **44** | 含新补的 `202/203/642/709` 四条 |
| **需先返工/owner 明确裁定** | **3** | ident **14**（`ignore`）、**10**、**50** |
| 其中 `tier=R` 的规则 | 6 | 10 / 14 / 200 / 201 / 202 / 203 |
| 草稿人分组 | worker-4 28、worker-70 13、worker-66 4、worker-68 1、worker-26 1 | 同一命令的变体 |
| 文件指纹 | `sha256=193aa7b7c010828b33f437f40f904d759cd21774c29db60176e9be8bd1bf478d` | 引用时带指纹 |

> 本卡在 master `55a8e7fca` 复算：仍 **47/47**，`sha256=193aa7b7…` 未变（`rules.json` 自上一版快照起未被改动）。

**为什么从 33/10 变成 44/3**：旧文档列的 10 条「需证据」里，7 条（54/5008/1106/752/811/766/213）
已按 `b42f20a28`（merge `card/rules-recog-unmask`）补上 `head_compare:["recog"]`（本卡逐条核 `rules.json` 确有该键）；同时新增 `202/203/642/709` 四条
（补规则包 `docs/conformance-rules-202-203-642-709-20261023.md`）。剩 3 条：

| ident | action | 缺什么 | 后果 |
|---:|---|---|---|
| **14** | `ignore`（整帧剔除） | reviewer-2 在 `46e479fc`/`2f4b311f` 明写「`approved_by` 必须保持 PENDING，直到 `openmir2-castle-merchant-fixedhide-hit` 落地或工具加计数/形态比较」。该卡代码已合 `0119e47b9`，但 **R 级窗口⑧未跑** | **唯一硬阻塞**：只要本轮两侧剔出任何 `SM_HIT(14)`，整轮 `UNDECIDABLE(rc=2, pending_ignore_rule)` |
| **10** | `compare + head_mask:["series"]` | 掩整个 `MakeWord(Dir,Light)`；玩家原地转身的 Dir 因此无帧级判据（reason 已按 reviewer-2 F7 写实，但**没有追踪 T 行**） | 不 rc=2；只是证据等级不足（报告带 `rules_pending_review`） |
| **50** | `mask_fields:["series"]` | reason 只论证了 Light 是墙钟，**没论证 Dir**；登录朝向是确定值却被一并掩掉 | 同上 |

> ident 14 单独说清：它**不是**「证据不够」——RNG 支证据充分；卡点是 reviewer 已签的**保留条件**
> 与「合并算不算落地」两种读法。owner 必须先裁定口径：① 认可「合并 + T-192 构造级」即算落地 ⇒ 可批；
> ② 要求真机窗口 ⇒ 先排窗口。在此之前**不要 de-PENDING**。

### 证据：批准会触及哪些 selftest 断言，不改会怎么红

- 当前：`python -I -B tools/conformance/compare_runs.py selftest` → **GREEN 262/262**（本卡实跑）。
- 实测（在仓库外的临时副本上把 47 条 `approved_by` 全改成非 PENDING，再跑同一 selftest）：
  **`RED: compare_runs selftest 240/262`** —— 恰好 **22 条**断言红。
  复跑方法：`cp -r tools E:/tmp/nh24-sim/` → 改副本 `tools/conformance/rules.json` 的 `approved_by` → 在副本里跑 selftest。
- 22 条断言的分布（`tools/conformance/selftest_impl.py`）：
  - 7 条：`NEW_IDENTS` 逐 ident 的「带 reason/tier/approved_by=PENDING」（`:327`，覆盖 100/520/525/529/530/654/657）；
  - 13 条：批次 2 逐 ident 同型断言（`:494`，覆盖 11/13/200/201/615/616/635/636/643/644/645/763/767）；
  - 1 条：`31 的 reason/tier/approved_by=PENDING 齐全`（`:830`）；
  - 1 条：`内置 14 是 PENDING ignore ⇒ 真剔帧时 rc 2`（`:667`）。
- **只批 ident 14**（口径 A）实测：`RED: compare_runs selftest 261/262`（只红 `:667` 那一条）。
- 新补的 `202/203/642/709` **不动**上述断言（全批模拟里这四条的牙仍 PASS）。

### owner 的选项

| 选项 | 内容 | 成本 | 不做的后果 |
|---|---|---|---|
| **A 最小解阻** | 只批 ident 14（先裁定其保留条件） | 改 1 条断言（261/262）+ 1 次评审 | 不批：遇 14 即整轮 rc=2；批错了则 R 级证据失真 |
| **B 清账** | 先批 44 条可批 + 返工 10/50（补 T 行）+ 裁定 14 | 22 条断言同步改（或分两次提交），全批后 240/262 绿 | 不批：每轮都带 `rules_pending_review=true`，不能当已复核判据 |
| **C 不批** | 维持 PENDING，窗口用 `--ignore-idents 14` 逐次显式处置 | 0 | `--ignore-idents` 不替代 approve，证据等级永远不足 |

**不替 owner 决定批不批**；只提示：若选 A，必须在同一提交里改 `selftest_impl.py:667`，否则门禁假红。

---

## 3. 三张卡需要 owner 一句话（多会话 / 替代判据）

| 卡 | 现状 | 证据 | 还差什么 | owner 的选项 |
|---|---|---|---|---|
| `openmir2-run-gap-checkdupobj` | 已实现（3 人同格推挤），`in-progress`；余量=真机窗口（3 人同格的身份序列 + AOI 相位） | 实现 `706c95cae`；ask 在 comment `40514dd66f` | 单 probe 构造不出 3 人会话 | **A** 等多会话探针 slice-2（卡 `openmir2-multi-session-probe`，T-221）；**B** 批准构造级替代并 close（需写清放弃哪些判据） |
| `openmir2-disappear-recipients-gridscan` | 已改为离开时刻 ±12 格扫，`in-progress`；余量=±12 内观察者的真机次序 | `8dea847c5`；ask 在 comment `cff88bf134` | 需要第二观察者 | **A** 等多会话探针；**B** 构造级收口（同上） |
| `openmir2-enter-map-cell-rejected-vs-oracle` | 实现 + 复验 PASS，`in-progress`；**真机不可达**（生产无 `switch_map` 调用者） | `df5a260ea`；建议在 comment `3730b07aa6`（bucket ③ 建议关闭） | 只剩「要不要显式关闭」 | **A** 直接 `closed`（R 级不可达应显式关闭）；**B** 开一张新卡跟踪未来出现的 `switch_map` 调用者，本卡仍关 |

> 三张卡的状态移动只能由 owner/coordinator 发签名 `status`；执行者只给材料。

---

## 4. whitelist `T-161..T-222` 的 owner 复核口径（旧账）

### 现状（现算）

| 口径 | 现算 | T 号 |
|---|---:|---|
| 区间内已登记行 | **58** | 缺号 185/197/198/200（区间内未登记） |
| 有**可核的非作者 approve** | **2** | `T-209`、`T-213` |
| 只有 review `request_changes`（无 approve） | **6** | `T-192`、`T-196`、`T-205`、`T-206`、`T-219`、`T-222` |
| 只有「待独立 principal 复核」字样 | **48** | 其余 |
| 无痕（无字样、无可解析 review） | **2** | `T-172`（写「已获口头批准，需在本行留痕」）、`T-208`（`window_required`，行内 `bdddbeba` 是 issue 不是 review） |

> 全文 T 行现在是 **255**（`t_reservation_gate check`：rows=255 whitelist=255 max=T-267；`goal_gate --json` 的 `whitelist_t_rows` 同值）；
> 上文 58 只覆盖 `T-161..T-222`。
> 四档口径与逐号 evidence 见 `docs/whitelist-review-status-audit-20261024.md` §2–§4
> （该审计快照为 head `151a7e822`，全表计数会漂，但**四档判定方法可复算**）。

复算命令（仓库根，离线）：

```sh
python -I -B -c "
import re
rows=[]
for ln in open('mir2-rs/tests/parity/whitelist.md',encoding='utf-8'):
    m=re.match(r'\|\s*(T-\d+)\s*\|',ln)
    if m and 161<=int(m.group(1)[2:])<=222:
        rows.append((int(m.group(1)[2:]), ln.rstrip().split('|')[-2].strip()))
print('rows',len(rows))
print('phrase',sum('待独立 principal 复核' in s for n,s in rows))
print('owner留痕',sum('待 owner 复核' in s for n,s in rows))
"
```

### 证据

- `T-209` ← review `8ab09811`（reviewer-1，approve，卡 `openmir2-gm-reachability-registry`）；
  `T-213` ← review `f6509d9a`（reviewer-5，approve，卡 `openmir2-script-action-var-text`）。
- 6 条 RC：`T-192`←`1de72e17`、`T-196`←`2c5cb955`、`T-205`←`39262b65`、`T-206`←`8c4da05a`+`48eae6ce`、
  `T-219`←`f015343d`、`T-222`←`24132993`（actor 与卡见审计 §3）。
- 复核方法：`git cat-file -p <blob 前缀>` 看 `kind/actor/id/body.decision`；`walgit collab thread <卡>` 看 `verified`。

### 还差什么

owner 的一句话「复核口径」：是接受「有可核非作者 approve 才算已复核」，还是接受「登记型 PASS 也可记账」。

### owner 复核要看哪几个文件

1. `mir2-rs/tests/parity/whitelist.md`（只看 `T-161..T-222` 这 58 行）；
2. `docs/whitelist-review-status-audit-20261024.md` §2–§4（四档方法与逐号 evidence）；
3. `mir2-rs/tests/parity/T-RESERVATIONS.md`（号的所有权；门禁 `python -I tools/whitelist/t_reservation_gate.py check`）；
4. 上述 8 张卡的线程（`walgit collab thread <卡>`）；
5. 需要复核数值面时再看 `docs/conformance-rules-202-203-642-709-20261023.md`。

### owner 的选项

- **A**：接受审计四档口径 ⇒「已复核」只认 2 行（T-209/T-213），其余 56 行按「有登记 ≠ 已复核」记账；
- **B**：让 reviewer 对 48 行仅字样批量补 approve（成本高，需逐行核证据）；
- **C**：逐号复核 58 行（最贵，但一次结清）。

---

## 5. 两条 Rust 接线缺口——**两条都可关闭**

| 旧条目 | 现状 | 证据（可复跑） | 还差什么 | 结论 |
|---|---|---|---|---|
| `@upgradenow` / `@getbackupgnow` 只 `glog` 不执行 | **已实现**：卡 `openmir2-upgrade-npc` 已合入 master，只剩 5 项真机窗口判据（R 卡保留 `in-progress`） | `git merge-base --is-ancestor 35c039c8d master` → 真；`mir2-rs/bins/game-svr/src/shop.rs:1288-1307`（标签接缝）/ `:1346 upgrade_request_for` / `:1369 upgrade_wapon_outcome`；宿主调用在 `mir2-rs/bins/game-svr/src/lib.rs:5809`（Lane A）/ `:5908`（提交）/ `:5949/5959`（取回）——行号取自 master `55a8e7fca` | 只剩窗口（同 §1） | **本条可关闭（不是 owner 的事）** |
| `DoDamageWeapon` / `SM_DURACHANGE(642)`（旧记「未接」） | **已合入 master `8dcb8c8ca`**（合并提交即该条）；卡 `openmir2-attack-weapon-wear-642` 为 R 卡，**真机窗口未跑**（保持 `in-progress`） | `git merge-base --is-ancestor 0a0655c24 master` → 真；`git grep -c attack_success_weapon_wear master -- mir2-rs/crates/world/src/combat.rs` → 9；`grep -cE '642\|DURACHANGE' mir2-rs/crates/world/src/combat.rs` → 23；复审 `dbcf22f8e`（reviewer-6，approve / tier=R / `window_required=true`） | 只剩窗口（`equip_struck_save` 已解锁） | **本条可关闭（只剩窗口，不是 owner 的事）** |

> 两句提醒：① `openmir2-ability-weaponstrong` 已合入 master `d38acf25b`（tier=R）；`goal_gate` 里现为
> `state=pass / category=window_required / merged=true`，**不是 FAIL**，只剩真机窗口（R 卡不 closed）。
> ② 642 的尾桶门保留了 oracle 的 `Math.Abs(nDura/1.03) != nDuraPoint` 怪癖（未取整 double vs int），返工说明见卡面 `765b21c4`。

---

## 6. 私有 C# 栈保留（低优先级）

### 现状

- `E:/tmp/conformance/csharp-root` **存在**；`E:/tmp/conformance` 共 **699 MB**
  （含 `baseline/`、`csharp-root/`、`rust-root/`、`runs/`、`restore-baseline*.sh`、`start-csharp*.ps1`）。
- 内置 `alt` profile 的 C# 侧直接指向它（`card_window.py:305-317`：`start-csharp3.ps1`、`restore-baseline.sh`、
  `binaries=E:/tmp/conformance/csharp-root/M2GameSvr/GameSrv.exe`、`envir_root=…/M2GameSvr`）。

### 证据

- `ls -d E:/tmp/conformance/csharp-root` → 存在；`du -sh E:/tmp/conformance` → `699M`（本卡实跑）。
- `tools/conformance/card_window.py:290-338` 的 `alt` profile 是窗口默认值（本快照行号）。

### 还差什么

只需要 owner 一句话。窗口要继续跑就**不能**现在删——`alt` 是唯一非生产库的 C# 剖面。

### owner 的选项

- **A 长期保留**（默认）：成本 699 MB；下次窗口零准备。
- **B 只在窗口期保留**：非窗口期把 `csharp-root/` + `baseline/` + `restore-baseline*.sh` 一起归档；
  成本=每次开窗前后各一次搬运，漏归档会导致下次必须重建。
- **C 退役**：回收 699 MB，但下次窗口要先重建 C# 私有根与基线库；
  **不能**用 `csharp-default` 顶替（它指向生产库 `mir2_db`，窗口默认拒绝）。

---

## 附：本卡的复算与残留

- 复算命令均在本文件各节内；`goal_gate --json` 较慢（>120s），建议直接读 `conditions[1].detail`。
- 本卡（`openmir2-needs-human-refresh-20261024`，`tier=L`）只改 `docs/`：新增本文件 + 在
  `docs/needs-human-20261023-night.md` 顶部加一行互指。未碰 `rules.json` / `whitelist.md` / 工具 / 场景 / 任何卡片状态。
- 本轮 v2 基于 master `55a8e7fca` 重打快照；动态数字现算值见 §1.5（`goal_gate`）与 §4（`whitelist`）。
- 残留物：无（批处理模拟副本、生成的 magic profile、dry-run 输出均在提交前删除，不留在 `E:/tmp` 专属路径）。
