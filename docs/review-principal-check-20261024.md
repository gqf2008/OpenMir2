# `review_principal_check`：派复审 / 派第二个实例前的 10 秒机械核对

- 卡：`openmir2-review-principal-check`（tier=L）；写集 `tools/hygiene/`、`docs/`。
- 起因：今天三次把「请复审」发给**该卡作者本人**（`openmir2-rehearsal-magic-preview-count`、`openmir2-spell-cast-window-blockers`、
  `openmir2-rehearsal-refresh-478ff9a33`），作者都按 AGENTS.md §0b/§4 拒绝自签并停手；另有一次重复派单
  （`openmir2-leader-process-lessons-20261024` 认领后被再派给第二个实例）。根因都不是执行方抢卡，
  而是**派单前没机械核对**「候选审查者是谁、这张卡作者是谁、这个 worktree 是否正在被使用」。
- 本工具把这三问变成一条命令；**不新增规则**，只是把既有规则变成可执行检查。

## 1. 何时必须跑

| 场景 | 必带参数 |
|---|---|
| 派 `review` 前 | `--card <id> --reviewer <principal>` |
| 派第二个实例 / 接管前 | 同上 + `--worktree <目标 worktree 绝对路径>` |
| 接管一张「有过 claim 的卡」前 | 同上（同时看 `history_authors` 与 `busy_processes`） |

## 2. 判定口径

- **候选作者集合** = 该卡线程里**所有** `verified is True` 的 `patch` 或 `status{status:in-progress}` 条目的 `actor`，
  按**首次出现去重**、保持线程顺序。
  - `author=` 打印**最新**那个（当前 owner）；`history_authors=` 打印更早的候选（去重后一行，不再是一串交替重复）。
- **`SELF_REVIEW` 判定 = `--reviewer` 命中候选集合中的任意一个**（fail-closed，rc=1）。
  命中时额外打印 `self_review_match=<principal> (<角色>)`，说明**因哪个候选命中**：
  `current owner`（最新签名者）还是 `historical author`（更早的签名者）。
  这样 R 卡保活补发的 `status{in-progress}` 顶掉真实 patch 作者时，**真实作者仍会被拒绝**（M-7 升格项）。
- **`--reviewer` 为空串 / 纯空白 / 仅分隔符**（典型：`--reviewer "$R"` 而 shell 变量 `R` 未设置）是**参数错误 rc=2**，
  绝不落到「已给 reviewer」分支——否则 `"" in authors` 恒假，工具会对**真实作者**返回 `OK`（I-1，fail-open）。
  首尾空白先归一（`" p "` == `"p"`），避免真审查者因多余空格被误判成 `OK`。
- `review` / `status{needs-review|closed}` / `merge_result` / `comment` **不参与**作者判定（否则会把审查者/编排者也认成作者）。
- 只有 `verified is True` 的条目被采信；**缺 `verified` 键按未采信处理**（项目红线「不采信未验证条目」），
  并在输出里给出 `unverified_entries_skipped=N`。
- 采信条目里的 `actor` 必须是**非空字符串**。`123` / `null` / 列表等是账本损坏：不采信并计
  `malformed_entries_skipped=N`；若因此没有任何候选作者，结论是 `UNKNOWN` / rc=2（M-2）。
- **worktree 忙**：本机存在 **python / cargo / rustc** 进程，其命令行包含该 worktree 的绝对路径
  （`\`↔`/`、盘符、大小写归一）**或该 worktree 的叶子名**（作为独立路径分量，覆盖短路径/映射盘/混合斜杠的写法），
  或该 worktree 的 `git status --porcelain` 非空。本进程及其**祖先进程**一律排除——它们的命令行本来就带着这个路径参数。
- **进程表读不到**时打印 `busy_processes=unavailable` + WARN 并 **rc=2**；不把「不知道」当「空闲」。
- **分支提示**：期望分支取该卡**最新 `verified patch` 的 `body.branch`**；该卡没有 patch 时才退回最新
  `status{in-progress}` 的 `body.branch`（合并后的保活 status 常声明 `master`，不能当卡分支）。
  detached HEAD ⇒ `branch=<detached>` 且**跳过**占用提示；账本未声明分支 ⇒ `branch_hint=skip`，**不下结论**。

## 3. 用法与退出码

```sh
python -I tools/hygiene/review_principal_check.py --card <id> --reviewer <principal>
python -I tools/hygiene/review_principal_check.py --card <id> --reviewer <principal> --worktree <path>
python -I tools/hygiene/review_principal_check.py --card <id>          # 只打印作者
python -I tools/hygiene/review_principal_check.py --selftest
```

| rc | 含义 |
|---:|---|
| 0 | 作者可见；`--reviewer` 不在候选作者集合里；worktree 空闲（或无 `--worktree`） |
| 1 | `SELF_REVIEW`（reviewer 命中任一候选作者）或 `BUSY`（有进程 / `git status` 非空） |
| 2 | 账本读不到 / 线程不是 JSON / 形状异常、**账本输出不是合法 UTF-8（严格解码；绝不 replace 成合法 U+FFFD 后继续判定）**、**walgit 不存在或超时（内部 rc=124/127，不再 traceback）**、**其它意外异常（rc=125，外层 `guarded()`）**、reader 抛异常、`--worktree` 不是 git worktree / `git status` 失败、进程表读不到、**参数错误（含 `--reviewer` 空串 / 纯空白 / 仅分隔符）**（全部 fail-closed） |

**三种 rc 必须能靠退出码本身区分**（契约以文档为准，实现按文档改）：

```text
$ checker --card openmir2-spell-cast-window-blockers --reviewer openmir2-reviewer-6 ; echo rc=$?
verdict=OK                 rc=0
$ checker --card openmir2-rehearsal-magic-preview-count --reviewer openmir2-worker-63 ; echo rc=$?
verdict=SELF_REVIEW        rc=1
$ checker --card openmir2-no-such-card-xyz --reviewer openmir2-reviewer-6 ; echo rc=$?
review-principal check: cannot read ledger thread openmir2-no-such-card-xyz (rc=1): Error: no entries for thread …
rc=2
```

`verdict=UNCHECKED` 只出现在没给 `--reviewer` 时（rc=0），表示「只取了作者，没做自签判定」。

> **输出契约（v3 起）**：最终结论**只有一行** `verdict=`；中间结论改名为 `review_verdict=`（`OK` / `SELF_REVIEW` / `UNCHECKED`）与 `worktree_verdict=BUSY`。worktree 读不到 ⇒ `verdict=WORKTREE_UNREADABLE`；进程表读不到 ⇒ `verdict=PROCESS_TABLE_UNAVAILABLE`（两者 rc=2）。

```text
# walgit 不在 PATH（只留 python + system32）⇒ 不再 traceback，rc=2
review-principal check: cannot read ledger thread openmir2-inventory-top5-audit (rc=127): cannot execute 'walgit': [WinError 2] 系统找不到指定的文件。
rc=2
```

### 3.1 失败路径穷举（除 #17 行内注明外，每条实测 rc=2）

「无法完成判定」的每条路径都归到 **rc=2** —— **rc=1 只剩 `SELF_REVIEW` / `BUSY` 两个结论**。下表是 **v4** 实现里**实际枚举到**的全部路径（不是只修被点名的两条）：

| # | 失败路径 | 触发方式（本机实测） | 内部信号 / 落点 | 实测 rc |
|---:|---|---|---|---:|
| 1 | 参数错误（未知参数 / 值缺失 / 缺 `--card`） | `--bogus x`、`--card`、省略 `--card` | `main()` 参数校验 | 2 |
| 2 | `walgit` 不存在 | PATH 只留 python + system32 | `_run` `OSError` → 127 | 2 |
| 3 | `walgit` 子进程超时（>20s） | 注入 rc=124；selftest ⑫ 用 0.3s 真超时复现 `TimeoutExpired` → 124 | `_run` → 124 | 2 |
| 4 | 账本线程读不到（卡不存在 / snapshot-inbox 读失败 / 权限） | `--card openmir2-no-such-card-xyz` | 子进程 rc≠0 | 2 |
| 5 | 线程输出不是 JSON | 注入 walgit stdout=`hello not json` | `json.loads` `ValueError` | 2 |
| 6 | 线程 JSON 形状异常（不是 list） | 注入 stdout=`{}` | `isinstance(data, list)` | 2 |
| 7 | 线程没有任何可信作者 | 注入 stdout=`[]` | `report()` → `UNKNOWN` | 2 |
| 8 | reader 自身抛异常 | evaluate seam 注入 `RuntimeError` | `except Exception` | 2 |
| 9 | **账本子进程输出不可解码**（权威输入） | 假 `walgit` 输出 actor 里带原始字节 `\xff` | `_run` 返回 **bytes**；`decode_ledger_stdout()` 严格 UTF-8，解码失败走 error 通道；**v3.1 的 `errors="replace"` 会把 `\xff` 变成合法 U+FFFD、`json.loads` 成功、author 被污染 ⇒ rc=0** | 2 |
| 10 | 其它未预期异常（含控制台编码错误） | selftest ⑮ 注入 `RuntimeError` | `guarded()` 兜底 | 2 |
| 11 | `--worktree` 不存在 / 不是目录 | `E:/tmp/.../no-such-dir` | `isdir` | 2 |
| 12 | `--worktree` 不是 git worktree | 普通目录 | `rev-parse --is-inside-work-tree` 失败 | 2 |
| 13 | `--worktree` 路径过短（无法有意义比对） | `--worktree C:/` | `len(_norm) < 4` | 2 |
| 14 | `git status` 失败 | `GIT_INDEX_FILE` 指向一个目录 | status rc≠0 → `WORKTREE_UNREADABLE` | 2 |
| 15 | 进程表读不到（无 `powershell` 且无 `ps`，或两者都失败） | PATH 去掉 system32 与 `/usr/bin` | `busy_processes=unavailable` → `PROCESS_TABLE_UNAVAILABLE` | 2 |
| 16 | **`--reviewer` 空串 / 纯空白 / 仅分隔符** | `--reviewer ""`、`--reviewer "   "`、`--reviewer ","` | `normalize_reviewer()` → None ⇒ 参数错误（main 与 report 双层） | 2 |
| 17 | **账本元素 `actor` 非字符串 / 空白** | 假 `walgit` 输出 `"actor":123` | 不采信 + `malformed_entries_skipped=N`；无候选 ⇒ `UNKNOWN` | 2（若还有有效作者则按其判定） |

> 第 9 条只约束**权威账本**。v3.1 的 `errors="replace"` 曾把这条路径从「崩溃 rc=1」改成「静默错的 rc=0」：
> `\xff` 变成 U+FFFD 后仍是合法 JSON 字符串，author 列表被污染而 `reviewer in authors` 永假，真实作者被判 OK。
> 现在 `_run` 一律返回 **bytes**（不再有 reader-thread 解码），账本侧严格 `decode("utf-8")` ⇒ rc=2。
> 进程表 / `git` 输出仍用 `_text(..., errors="replace")`：它们只用于比较与打印，且坏掉的进程表会退化为
> `unavailable` ⇒ rc=2，不可能反过来变成「空闲」。

## 4. 与 `docs/leader-queue-20261023.md` 规则 14–20 的关系

- **规则 14**（派活前先查「同 principal 是否已有活人在干」）：`busy_processes` / `worktree_dirty` / `last_commit`
  是它「三查」里 git 层的可执行版本；三查里的协作层由下面的规则 19 覆盖。
- **规则 15**（「打印变红」≠「门禁变红」，自检必须挂在 rc 路径前）：本工具的 **20 条 selftest 全部走生产函数**（`report()` / `evaluate()` / `worktree_state()` / `guarded()` / `main()`），
  不是断言复制出来的辅助函数。改坏真实判定（把 `reviewer in authors` 改成永假）⇒ `--selftest` 立刻 **18/20、rc=1**（见 §5.1 变异证据）。
- **规则 16**（git author ≠ collab principal）：作者一律取 `refs/collab/*` 签名条目的 `actor`，**不读 `git log` 的 author**——
  主检出与多个 worktree 共用一份 `git config user`，那只是噪声。
- **规则 17**（mutation runner 会弄脏树）：本工具只读（`git status` / `rev-parse` / `log`），不改文件、不写索引；
  对自身 worktree 跑不会把自己判忙（`git status` 只读）。
- **规则 18**（worktree 身份固定、别拿错 worktree）：`branch` / `declared_branch` / `hint` 三行用于发现「拿错 worktree」；
  detached 或账本未声明分支时**不下结论**（避免误报）。
- **规则 19**（派同身份前查最近几分钟有没有同 principal 的写入）：`author` / `history_authors` 回答「**这条线程谁写过**」；
  但**recency 不在本工具输出里**（没有条目的 `ts`），「最近 10 分钟」仍须 `walgit collab thread <id>` 自己看。
- **规则 20**（长命 agent）：本工具不判上下文长度，只提醒「每张卡起新实例」的前提是**新实例要知道这卡作者是谁**。
- AGENTS.md §0b/§4（审查必须是不同 principal、作者自审不算数）是 `SELF_REVIEW` 判定的规范来源；本工具不改规则、不代审。

## 5. 自检（`--selftest`，20/20，当天实跑）

```text
$ python -I -B tools/hygiene/review_principal_check.py --selftest
PASS ① reviewer == author => SELF_REVIEW (rc=1，经 report())
PASS ② reviewer != author => OK (rc=0)；needs-review 的 status 不是作者
PASS ③ 接管：owner=最新签名 patch 作者，历史作者按首次出现去重保留
PASS ④ 脏 worktree => BUSY 且 rc=1（端到端经 report()）
PASS ⑤ 进程表不可读 => WARN + busy_processes=unavailable + rc=2
PASS ⑥ verified=false 的 patch 不采信
PASS ⑦ verified 缺键 => 默认不采信（fail-closed）
PASS ⑧ 账本读不到 => rc=2（经 evaluate()，与 0/1 可区分）
PASS ⑨ 保活 status 顶掉 owner 时，真实 patch 作者仍判 SELF_REVIEW (rc=1)
PASS ⑩ detached HEAD => branch=<detached>（不打印 branch=HEAD）
PASS ⑪ 占用提示按账本声明分支比对：相同不提示 / 不同才提示 / 未声明不下结论
PASS ⑫ _run 超时/缺命令不抛异常（rc=124/127）；reader 抛异常 => evaluate rc=2
PASS ⑬ branch 带中文注释 => 只取首个 token（不把说明当分支名）
PASS ⑭ 子进程输出按字节返回；展示侧 _text 才 replace（权威账本侧严格解码）
PASS ⑮ guarded()：意外异常 => rc=2，正常返回原样透传
PASS ⑯ main() 外层 guarded 真的接在 evaluate 上：抛异常 => rc=2（拆掉 guarded 立即红）
PASS ⑰ 空串/纯空白/仅分隔符 reviewer => INVALID_REVIEWER rc=2（不是 OK）
PASS ⑱ --reviewer 首尾空白归一（" p " == "p"），否则真审查者会被判成 OK
PASS ⑲ 非字符串/空白 actor 不采信（计入 malformed；无作者 => UNKNOWN rc=2）
PASS ⑳ 账本严格 UTF-8：不可解码 => 不返回文本（走 rc=2 通道）
selftest: 20/20 passed
```

①–③ 是卡面要求的三条；**④–⑳ 全部走生产函数**（`report()` / `evaluate()` / `worktree_state()` / `guarded()` / `main()`），没有为测试另写一份判定逻辑。
精确口径：**①②④⑤⑨⑰⑲ 经 `report()`，⑧ 经 `evaluate()`，③⑥⑦⑩⑪⑬⑭⑳ 断言生产函数（`author_from_entries` / `declared_branch_from_entries` /
`worktree_state` / `clean_branch` / `_run` / `_text` / `decode_ledger_stdout`）的数据出口，⑫ 断言 `_run()` 与 `evaluate()` 的异常边界，
⑮⑯ 断言外层 `guarded()` 的兜底与 **main 的接线**，⑱ 断言 reviewer 归一**。
`read_thread=` 与 `table=` 只是注入外部输入的接缝（账本、进程表），判定本身只有一份。

### 5.1 牙的验证（规则 15：改坏它，rc 会变吗？）

串行跑、`git checkout -- <file>` 还原、还原后 `sha256` 与 `git show :<file>` 一致（本轮实测六条全绿→红→还原）。

```text
# 变异①：让真实 SELF_REVIEW 永不成立（v3.1 起的老牙，v4 计数随测试数更新）
$ sed -i 's/        if reviewer in authors:/        if reviewer in authors and False:/' tools/hygiene/review_principal_check.py
FAIL ① reviewer == author => SELF_REVIEW (rc=1，经 report())
FAIL ⑨ 保活 status 顶掉 owner 时，真实 patch 作者仍判 SELF_REVIEW (rc=1)
selftest: 18/20 passed                      # rc=1（红）

# 变异②：把候选集合缩回单一作者（回到 M-7 的假阴性）
$ sed -i 's/        if reviewer in authors:/        if reviewer in authors[-1:]:/' tools/hygiene/review_principal_check.py
FAIL ⑨ 保活 status 顶掉 owner 时，真实 patch 作者仍判 SELF_REVIEW (rc=1)
selftest: 19/20 passed                      # rc=1（红）

# 变异③：拆掉 I-1 的空值守卫（normalize_reviewer 不再拒绝空串/空白/分隔符）
#   改前 v3.1 rc=0；变异的 v4 仍 rc=0（说明牙钉的就是这条路径）
FAIL ⑰ 空串/纯空白/仅分隔符 reviewer => INVALID_REVIEWER rc=2（不是 OK）
FAIL ⑱ --reviewer 首尾空白归一（" p " == "p"），否则真审查者会被判成 OK
selftest: 18/20 passed                      # rc=1（红）
$ checker --card openmir2-attack-weapon-wear-642 --reviewer "" | tail -2
review_verdict=OK
verdict=OK                                  # rc=0 —— 真实作者被放行的 fail-open 复现

# 变异④：把 I-2 的严格账本解码换回 errors="replace"
FAIL ⑳ 账本严格 UTF-8：不可解码 => 不返回文本（走 rc=2 通道）
selftest: 19/20 passed                      # rc=1（红）
$ PATH=<假 walgit> checker --repo <fake> --card c1 --reviewer openmir2-worker-a   # actor 带原始 0xFF
author=openmir2-worker-a<U+FFFD>
review_verdict=OK
verdict=OK                                  # rc=0 —— 真实作者被判 OK 的 fail-open 复现

# 变异⑤（M-1）：只把 main 里的 guarded(...) 拆掉（guarded 函数保留）
FAIL ⑯ main() 外层 guarded 真的接在 evaluate 上：抛异常 => rc=2（拆掉 guarded 立即红）
selftest: 19/20 passed                      # rc=1（红）
$ PATH=<假 walgit 输出 [{"entry":"x","verified":true}]> checker --repo <fake> --card c1 --reviewer p
AttributeError: 'str' object has no attribute 'get'
rc=1                                        # traceback 退化（生产 head 是 rc=2）

# 变异⑥（M-2）：拆掉 actor 类型守卫（回到 v3.1 的 if not actor: continue）
FAIL ⑲ 非字符串/空白 actor 不采信（计入 malformed；无作者 => UNKNOWN rc=2）
selftest: 19/20 passed                      # rc=1（红）
$ PATH=<假 walgit 输出 "actor":123> checker --repo <fake> --card c1 --reviewer openmir2-worker-a
author=123
verdict=OK                                  # rc=0 —— 非字符串 actor 被当作者
```

六条变异都只动**生产判定行**；selftest 的 rc 随之变红并给出 FAIL 行（M-1 的变异在 v3.1 曾 15/15 存活，
v4 由 ⑯ 钉死 main 的接线），说明这些牙不再是「假牙」。

## 6. 真机冒烟（三张卡全部复现 `SELF_REVIEW`）

```text
$ python -I -B tools/hygiene/review_principal_check.py --card openmir2-rehearsal-magic-preview-count --reviewer openmir2-worker-63
card=openmir2-rehearsal-magic-preview-count
author=openmir2-worker-63
reviewer=openmir2-worker-63
verdict=SELF_REVIEW
self_review_match=openmir2-worker-63 (current owner (newest verified patch/in-progress signer))
rc=1

$ … --card openmir2-spell-cast-window-blockers --reviewer openmir2-worker-58      # author=openmir2-worker-58  verdict=SELF_REVIEW rc=1
$ … --card openmir2-rehearsal-refresh-478ff9a33 --reviewer openmir2-worker-58     # author=openmir2-worker-58  verdict=SELF_REVIEW rc=1
```

作者判定与卡面 evidence 一致（worker-63 / worker-58 / worker-58）。

### 6.1 M-7 实卡：保活 status 不再顶掉真实 patch 作者

```text
$ … --card openmir2-weaponstrong-dura-zero-skip --reviewer openmir2-worker-63
card=openmir2-weaponstrong-dura-zero-skip
author=openmir2-orchestrator
history_authors=openmir2-worker-63
reviewer=openmir2-worker-63
verdict=SELF_REVIEW
self_review_match=openmir2-worker-63 (historical author (earlier verified signer in this thread))
rc=1

$ … --card openmir2-weaponstrong-dura-zero-skip --reviewer openmir2-reviewer-6    # verdict=OK rc=0（真审查者仍放行）
```

改前这里是 `verdict=OK rc=0`（假阴性：真实 patch 作者被放行自审）；改后 fail-closed。

### 6.2 `history_authors` 去重

`openmir2-m3-combat-skills-align` 原输出 46 项、约 1 KB；现在：

```text
$ … --card openmir2-m3-combat-skills-align --reviewer openmir2-reviewer-6 | head -3
card=openmir2-m3-combat-skills-align
author=openmir2-worker-4
history_authors=openmir2-worker-5,openmir2-orchestrator
```

## 7. worktree 判定：负控与正控

干净临时仓（`git init` + 空提交）上跑：

```text
$ … --card openmir2-attack-weapon-wear-642 --reviewer openmir2-reviewer-9 --worktree <干净临时 worktree，分支=card/attack-weapon-wear-642>
card=openmir2-attack-weapon-wear-642
author=openmir2-orchestrator
history_authors=openmir2-worker-24,openmir2-worker-67
reviewer=openmir2-reviewer-9
review_verdict=OK
worktree=E:/UserTemp/tmp.PFysr33zOC
branch=card/attack-weapon-wear-642
last_commit=74bc0a7 2026-10-24T08:22:52+08:00
busy_processes=0
worktree_dirty=no
verdict=OK
rc=0
# ⇒ 该卡账本里的 branch 带中文注释（`card/attack-weapon-wear-642（**force-with-lease 推送**…）`），
#   v3 清洗后与当前分支相同 ⇒ **不发 hint**（M-2 的误报形状已消除）

$ … --worktree <同一 worktree，另起 python 探针（按 PID 清理）>
review_verdict=OK
branch=card/attack-weapon-wear-642
busy_processes=1
  pid=27428 name=python.exe cmd=…python.exe C:/Users/gxh/AppData/Local/Temp/probe_sleep.py E:/UserTemp/tmp.PFysr33zOC
worktree_dirty=no
worktree_verdict=BUSY (进程/脏工作树)
verdict=BUSY
rc=1
# ⇒ 同一次输出只有一行 `verdict=`（最终结论），中间结论叫 review_verdict / worktree_verdict
```

探针按 **PID** 收尾（`taskkill //PID <pid>`），不用 `/IM`（见 §8 最后一条）。

补充实测：

```text
# 叶子名 + 大小写/斜杠归一（argv 写的是 C:\SOMEPLACE\OPENMIR2-WT-CRONCLAIM\probe.py）
busy_processes=2 … verdict=BUSY rc=1
# detached worktree：不再打印 branch=HEAD，也不发占用提示
branch=<detached> / branch_hint=skip (detached HEAD) / rc=0
# 账本声明分支 == 当前分支：不提示
branch=card/openmir2-script-envir-root-host / declared_branch=card/openmir2-script-envir-root-host / 无 hint / rc=0
```

### 7.1 FINDING-NEW-1 的 2 张真实卡（v2 误报 → v3 正确）

这两张卡的账本 `branch` 字段把说明写进了分支名（reviewer-6 全量扫 232 张有 patch 的卡，只命中 2 张）；
v2 原样取 `body.branch`，在**卡片自己的分支**上仍报「可能被其它卡占用」。v3 的 `clean_branch()` 只取首个 token：

```text
openmir2-attack-weapon-wear-642
  raw     = card/attack-weapon-wear-642（**force-with-lease 推送**：返工把 e717dffdc amend 成新 head，已推历史被改写，特此声明）
  cleaned = card/attack-weapon-wear-642
openmir2-weaponstrong-dura-zero-skip
  raw     = card/weaponstrong-dura-zero（已推 walgit）
  cleaned = card/weaponstrong-dura-zero
```

在各自分支的干净临时仓上跑（`--reviewer openmir2-reviewer-6`）：

```text
# v2（误报）
branch=card/attack-weapon-wear-642
declared_branch=card/attack-weapon-wear-642（**force-with-lease 推送**…）
hint: … => 可能被其它卡占用                      rc=0
# v3.1（正确：清洗后与当前分支相同 ⇒ 自比排除，不发 hint）
branch=card/attack-weapon-wear-642
verdict=OK                                       rc=0

# v2（误报）
branch=card/weaponstrong-dura-zero
declared_branch=card/weaponstrong-dura-zero（已推 walgit）
hint: … => 可能被其它卡占用                      rc=0
# v3.1（正确）
branch=card/weaponstrong-dura-zero
verdict=OK                                       rc=0
```

规范化后仍判不准时（账本从未声明分支 / detached）：`branch_hint=skip`，**不猜**。

## 8. 已知限制（写在这，别当它是万能判据）

- **cwd 盲区（未覆盖，必须另查）**：`cd <worktree> && cargo test` 的 `cargo` 命令行只有 `cargo test --workspace`，
  路径只在**当前工作目录**里，argv 里没有。实测：从 worktree 内部启动 `python /tmp/probe_sleep_cwd.py`
  ⇒ 本工具 `busy_processes=0`（而进程确实在跑）。本轮只做到**路径归一 + 叶子名二次匹配**，
  **cwd 维度拿不到**（Windows 上 CIM 不暴露 cwd）⇒ 仍须配合规则 14 的 `Get-Process cargo,rustc` 或看 `worktree_dirty`。
  这条与 `docs/leader-process-lessons-20261024.md` 的 Important 2 是同一形状，**不要当成已覆盖**。
- 只查**本机**进程；别的机器/容器里跑同一 worktree 看不到。
- 进程只认 `python*` / `cargo*` / `rustc*`；其它语言/工具在跑不会被判忙（`worktree_dirty` 仍可能命中）。
- 相对路径、8.3 短路径、网络盘映射路径可能匹配不上；`--worktree` 请给 `git worktree list` 里的原始路径。
- **进程表读不到 ⇒ rc=2 + `busy_processes=unavailable`**；此时输出里**不会**出现 `busy_processes=0`，但也不要把它读成「忙」——
  它是「不知道」。
- 分支提示依赖账本声明：卡从未声明分支、或 worktree detached ⇒ **不下结论**（宁可不提示，也不误报「可能被其它卡占用」）。
- **账本 `branch` 字段可能是自由文本**（真实存在 `card/x（**force-with-lease 推送**…）`、`card/y（已推 walgit）`）：
  v3 起先做清洗（取首个空白/括号前的 token，要求匹配 `^[A-Za-z0-9._/-]+$`，否则视为未声明并继续找更早的干净值），
  否则本卡自己的干净分支也会被报「可能被其它卡占用」。selftest ⑬ 钉住这条。
- **工具缺失/超时不再冒 traceback**：`walgit` 不在 PATH、`walgit` 卡住 >20s、PowerShell CIM >40s 都会变成
  「账本不可读/进程表不可读」⇒ rc=2（内部 rc=124/127），不会与 SELF_REVIEW/BUSY 的 rc=1 混淆；selftest ⑫ 钉住这条。
- **子进程输出一律按字节返回**（`_run(..., text=False)`），因此不会再有 Windows reader-thread 解码失败导致的
  `stdout=None`。**权威账本**用 `decode_ledger_stdout()` 严格 UTF-8：不可解码 ⇒ rc=2（绝不把 `\xff` 替换成合法
  U+FFFD 后继续判定）；**进程表 / git 输出**才用 `_text(..., errors="replace")`，且坏进程表只会退化为
  `unavailable` ⇒ rc=2。外层 `main → guarded()` 兜底任何未预期异常（内部 rc=125），统一 rc=2。
  selftest ⑭⑯⑳ 钉住这条；§3.1 是逐路径实测表。
- 本工具**不判**「这条 review 是否已经发生过」，也不判 review 质量；不输出条目 `ts`（规则 19 的 recency 要自己看线程）。
- 收尾清理探针进程请**按 PID**（`kill <pid>` / `Stop-Process -Id <pid>` / `taskkill //PID <pid>`）；本轮验证时曾用
  `taskkill /F /IM python.exe`（按镜像名杀全部 python），可能误伤其它车道，已在卡上 comment `70abe235` 报备。

## 9. 复算命令

```sh
python -I -B tools/hygiene/review_principal_check.py --selftest
python -I -B tools/hygiene/review_principal_check.py --card openmir2-review-principal-check          # 本卡作者含 openmir2-worker-24
python -I -B tools/hygiene/review_principal_check.py --card openmir2-weaponstrong-dura-zero-skip --reviewer openmir2-worker-63   # M-7 正控 ⇒ SELF_REVIEW rc=1
python -I -B tools/hygiene/review_principal_check.py --card openmir2-no-such-card-xyz --reviewer openmir2-reviewer-6              # rc=2
python -I -B tools/hygiene/review_principal_check.py --card openmir2-attack-weapon-wear-642 --reviewer ""                         # rc=2（I-1：空 reviewer 不是 OK）
PATH=<只含假 walgit 的目录> python -I -B tools/hygiene/review_principal_check.py --repo <fake> --card c1 --reviewer openmir2-worker-a   # rc=2（I-2：账本 0xFF 严格解码失败）
python -I -B tools/hygiene/review_principal_check.py --card openmir2-attack-weapon-wear-642 --reviewer openmir2-reviewer-9 --worktree <干净临时仓，分支同名>   # 无 hint，rc=0（branch 注释已清洗）
python -I -B tools/hygiene/review_principal_check.py --card openmir2-weaponstrong-dura-zero-skip --reviewer openmir2-reviewer-9 --worktree <干净临时仓，分支 card/weaponstrong-dura-zero>   # 无 hint，rc=0
# 失败路径矩阵（§3.1）：卡不存在 / 无 walgit / 非法 worktree / 进程表不可读 …
python -I -B tools/hygiene/review_principal_check.py --card openmir2-no-such-card-xyz --reviewer openmir2-reviewer-6          # rc=2
PATH=<python>;<system32> python -I -B tools/hygiene/review_principal_check.py --card <id> --reviewer <p>                     # 无 walgit ⇒ rc=2
GIT_INDEX_FILE=<一个目录> python -I -B tools/hygiene/review_principal_check.py --card <id> --reviewer <p> --worktree <wt>     # git status 失败 ⇒ rc=2
env PATH="$(dirname $(which python)):/c/Windows/system32" python -I -B tools/hygiene/review_principal_check.py --card openmir2-inventory-top5-audit --reviewer openmir2-reviewer-6   # 无 walgit ⇒ rc=2，无 traceback
walgit collab thread --repo . openmir2-rehearsal-refresh-478ff9a33 | python -I -c "import json,sys;print([e['entry']['actor'] for e in json.load(sys.stdin) if e['entry']['kind']=='patch'])"
```
