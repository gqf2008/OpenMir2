# 看板漏卡：`merged` 必须有一列（2026-10-24）

- 卡：`openmir2-board-merged-column`（tier=L）；写集 `.walgit/board.toml`、`tools/hygiene/`、`docs/`。
- 起因：`walgit collab board` 只显示**命中某一列谓词**的线程；`.walgit/board.toml` 里没有任何列的
  `status` 是 `merged`，而「最新一条状态条目是 `merge_result`」的线程会被投影成 `merged` ⇒
  这些卡**在所有列里都不出现**。实测漏 **11 张**（4 张最新条目就是 `merge_result`，7 张最新条目是
  `comment`、但更早的 `merge_result` 已把它投影成 `merged`）。
- 后果：漏掉的卡进不了「待认领 / 待审」队列 ⇒ 永远不会被再次派工或收口；而全项目判断队列只看 board。

## 1. 投影规则（本次实测确认的三条）

1. **第一个命中的列胜出**（`board.toml` 头注释原文）；字段留空 = 匹配任意。
2. **未命中任何列的线程不显示**——不是「显示在其它列」，而是彻底消失；`board --format json` 里也没有。
3. **board 读的是 `.walgit/board.toml` 的 HEAD 版本**：工作区里改了但没提交，plain `walgit collab board`
   看不到；预览未提交定义要用 `walgit collab board --board .walgit/board.toml`。
   （这也是本卡验收必须在 `--board` 下预演、合并后才算「现网生效」的原因。）

`merged` 的投影来源：线程里存在签名的 `merge_result`（`merged:true`）且其后没有把它改回其它状态的签名
`status` 时，board 把它折叠成 `merged`。它和 `open/in-progress/needs-review/needs-human/blocked/closed`
一样是**投影状态**，必须有列承接。

## 2. 加列与「只多 11 张」的逐列 diff

```toml
[[column]]
name = "已合并未收口"
status = "merged"
```

放在**所有既有列之后**（first-match，放前面会遮住别的列）。未提交时用 `--board` 预览；两次 board
调用间隔 **2.26s** 内实测（同一个脚本里连跑，避免看板在两次调用之间翻动）：

```sh
walgit collab board --repo . --format json > /tmp/before.json
walgit collab board --repo . --board .walgit/board.toml --format json > /tmp/after.json
```

| 列 | 加列前 | 加列后 | 集合与顺序 |
|---|---:|---:|---|
| 待认领 | 2 | 2 | **逐张、同序** |
| 进行中 | 236 | 236 | **逐张、同序** |
| 待审 | 5 | 5 | **逐张、同序** |
| 需要人 | 0 | 0 | **逐张、同序** |
| 受阻 | 2 | 2 | **逐张、同序** |
| 已完成 | 277 | 277 | **逐张、同序** |
| **已合并未收口**（新） | — | **11** | 见 §5 |

- `ids_gained` = 恰 11（§5 清单）；`ids_lost` = 空；总 id 数 522 → 533。
- 计数相对 reviewer 复核时（521→532）多 1，是**看板实时翻动**（已完成 276→277），不是加列的副作用；
  同一时刻连跑的两份输出里既有 6 列全部逐张同序。
- 覆盖门在 `--board` 下由 `FAIL (11 hidden)` 变为 `OK (0 hidden of 533 card threads; board shows 533
  ids; 29 non-card threads excluded; 1.7s)`。

## 3. 覆盖门 `tools/hygiene/board_coverage_gate.py`

```sh
python -I tools/hygiene/board_coverage_gate.py                      # 现网（HEAD 的 board.toml）
python -I tools/hygiene/board_coverage_gate.py --board .walgit/board.toml   # 预览未提交的列
python -I tools/hygiene/board_coverage_gate.py --json               # 机器可读
python -I tools/hygiene/board_coverage_gate.py --selftest           # 合成夹具 8/8
```

- **做法**：`git rev-parse --show-toplevel` 定位仓库根 → `git for-each-ref refs/collab/inbox` 取全部
  blob → **一次** `git cat-file --batch` 读完（8339 个 blob / 562 条线程，实测 **1.6–1.9s**；不对每个
  线程起子进程）→ 取每线程最新一条条目的 `kind/ts/actor` → 与
  `walgit collab board --format json` 出现的 id 求差集。
- **只比「卡片类线程」**（F1 裁决）：CI runner 会往 `refs/collab/inbox/*` 写**伪线程**
  （kind `ci_claim` / `ci_result`、id `ci-<hex>`、空 `parent`），它们不是卡片、board 永不显示。
  覆盖门用**封闭集合常量**把它们排除：

  ```python
  # 已知的非卡片（CI 运行记录）kind 封闭集合。集合之外的任何 kind 都按卡片处理——
  # 未知 kind 必须 fail-closed（报红），绝不能静默跳过。扩集合必须带 selftest 夹具 + 评审。
  NON_CARD_KINDS = frozenset(("ci_claim", "ci_result"))
  ```

  - 判定口径：一个线程**所有**条目的 kind 都在 `NON_CARD_KINDS` 里才算非卡片；只要有一条其它 kind
    （包括**未知 kind**）就按卡片处理。
  - 为什么不写成「看着不像卡片就跳过」：那会让**将来新出现的线程类型**静默消失，正是本卡要修的病。
  - selftest ⑤/⑥ 钉死：未知 kind `foo_bar` 必须算卡片，且它不在看板上时必须报红。
- **输出**：差集非空逐条打印 `thread_id <TAB> kind <TAB> ts <TAB> actor`，rc=1；为空 rc=0。
- **fail-closed**：`git`/`walgit` 不可用、`--repo` 不是 git 仓库、`--board` 不存在、board 输出不是
  JSON、形状异常、**任一 inbox blob 不是 JSON 或缺 `id`** ⇒ rc=2（`_run` 内部把超时/缺命令转成
  rc=124/127，不冒 traceback）。坏 blob 再也不会被静默跳过（F2）。
- **`--board`** 相对路径按**仓库根**解析（`git rev-parse --show-toplevel`），因此从仓库内任意子目录
  运行都等价（F4，见 §6）。`--board` 仍直接透传给 `walgit collab board --board`。

## 4. 牙（串行，改前 `git add`、改后 `git checkout --` 还原）

```text
# (a) 删掉新列（--board 指向 base 3738ad234 的 board.toml）⇒ rc=1，且列出 11 张卡（ci 已排除）
board-coverage gate: FAIL (11 hidden of 533 card threads; board shows 522 ids;
                            29 non-card threads excluded; 1.83s)
openmir2-client-e2e-enter-gaps  comment  1792771723  openmir2-worker-24
…（其余 10 条同 §5）…
rc=1

# (b) 新列 status 改成 "merge_result"（不存在的投影状态）⇒ rc=1
board-coverage gate: FAIL (11 hidden of 533 card threads; board shows 522 ids; …)   rc=1
# ⇒ 未知 status 被 board **静默忽略**（不报错、也不建列），覆盖门把它抓成「又漏了」

# (c) 差集判定改成恒空（hidden_threads 返回 []）⇒ selftest 红
FAIL ② thread absent from board ⇒ hidden
FAIL ⑥ unknown-kind card missing from board ⇒ reported
selftest: 6/8 passed                       # rc=1

# (d) 把未知 kind "foo_bar" 也塞进 NON_CARD_KINDS ⇒ selftest 红（F1 的牙）
NON_CARD_KINDS = frozenset(("ci_claim", "ci_result", "foo_bar"))
FAIL ⑤ an unknown kind (foo_bar) counts as a card thread
FAIL ⑥ unknown-kind card missing from board ⇒ reported
selftest: 6/8 passed                       # rc=1

# (e) 退出码接线变异 `return 1 if hidden else 0` → `return 0`（F3 的牙）
FAIL ⑦ printed FAIL wires to exit code 1
selftest: 7/8 passed                       # rc=1
# 同一变异下真实运行：打印 FAIL 却 rc=0 —— 正是 F3 描述的「打印 FAIL 但进程绿」，
# 现在 selftest ⑦ 会抓住它。
```

还原后：`--selftest` 8/8、`--board` 覆盖门 rc=0。

## 5. 漏掉的 11 张（本卡**只让它们可见**，不代收口）

| thread | 最新条目 | ts | 谁写的 |
|---|---|---|---:|---|
| `openmir2-client-e2e-enter-gaps` | comment | 1792771723 | openmir2-worker-24 |
| `openmir2-conformance-rng-nondeterminism` | comment | 1792771727 | openmir2-worker-24 |
| `openmir2-coverage-recon-gates` | merge_result | 1792136536 | openmir2-coordinator |
| `openmir2-gate-outbound-producer-dimension` | merge_result | 1792158485 | openmir2-coordinator |
| `openmir2-m2-rust-server-state-dump` | comment | 1791888714 | openmir2-coordinator |
| `openmir2-m2-rust-state-log-plumb` | comment | 1791888717 | openmir2-coordinator |
| `openmir2-m3-monster-ai-tick` | comment | 1792771720 | openmir2-worker-24 |
| `openmir2-menu-ok-unify-pickwire` | comment | 1792771731 | openmir2-worker-24 |
| `openmir2-noclear-mon-list-gate-red` | merge_result | 1792159855 | openmir2-coordinator |
| `openmir2-r-card-evidence-triage` | merge_result | 1792715343 | openmir2-orchestrator |
| `openmir2-ybdeal-frames` | comment | 1792771735 | openmir2-worker-24 |

其中 7 张最新条目是 `comment`（不是 `merge_result`）却被投影成 `merged`——所以「加 `merged` 列」并不只是
「给 `merge_result` 找个家」，而是**把 `merged` 这个投影状态整体接回看板**。

另有 **29 条 CI 伪线程**（`ci-*`）永远不进看板，按 §3 的封闭集合排除，不计入差集。

## 6. 历史口径与现在口径

- 作者首交时的证据 `OK (0 hidden of 526 threads)` 是**在 CI 记录出现之前**测的：作者 status
  `ts=1792801959`，最早 ci 条目 `ts=1792803921`，晚 **32.7 分钟**。此后任何 fetch 过 inbox 的工作区
  都会看到这些 `ci-*` 记录，按字面「全量 inbox 线程」口径门会**恒红**（29/29 全是 ci）。一个结构上
  恒红的门等于没有门，因此 orchestrator 裁决（comment `483109497`）：口径改为「**卡片类线程**全量
  覆盖」，CI 伪线程用封闭集合排除，未知 kind 按卡片处理。**不是**「只比 open 卡」——卡片集合仍全量。
- **合并后 plain 的期望**：`OK (0 hidden of N card threads; …; 29 non-card threads excluded)`，rc=0。
  合并前 plain 仍是 `FAIL (11 hidden …)`（HEAD 的 `board.toml` 还没有新列），这是**预期**，用
  `--board .walgit/board.toml` 预演才是绿的。
- **F4 的选择**：按 docstring 修代码（不是改 docstring）——`--board` 相对路径先经
  `git rev-parse --show-toplevel` 解析到仓库根。理由：门会被合并门禁/巡检从不同目录调用，按 cwd
  解析会让同一命令在不同目录给出 rc=0/rc=2 两种结果，违背「可复跑」。

## 7. 覆盖门抓不到什么（别当它是内容校验）

- 只判「**卡片线程上不上看板**」，不判「卡片内容对不对」：漏卡抓得到，列名写错、卡片进错列、
  状态条目写错都抓不到。
- 只看**本地 refs**：别处未 fetch 的 inbox 条目不在差集里。
- 只看 board 输出里有没有这个 id：如果某天 board 改成「重复 id 去重/截断」，门也只会看到 id 存在。
- **非卡片判定依赖 `NON_CARD_KINDS` 常量**：新出现的 CI 类 kind 若既不在集合、又确实永不进看板，
  门会**报红**（这是有意的 fail-closed）——此时应评审后把该 kind 加入集合并补 selftest 夹具，
  不是放宽判定。
- **HEAD 语义**：未提交的 `board.toml` 改动在 plain board 下不可见（用 `--board` 预览）。
- 「最新条目 kind」取的是**最大 ts**；同一秒内并发写入时以 ts 相等者先见者为准（若真出现，需在线上补
  parent 序判定）。

## 8. 复算命令

```sh
git -C . show HEAD:.walgit/board.toml | tail -5        # 合并后应能看到新列
walgit collab board --repo . --board .walgit/board.toml --format json | \
  python -I -c "import json,sys;d=json.load(sys.stdin);print({c['name']:len(c['cards']) for c in d['columns']})"
python -I tools/hygiene/board_coverage_gate.py --board .walgit/board.toml; echo rc=$?   # 期望 0
python -I tools/hygiene/board_coverage_gate.py; echo rc=$?                              # 合并前 1 / 合并后 0
python -I tools/hygiene/board_coverage_gate.py --selftest                               # 期望 8/8
(cd tools/hygiene && python -I board_coverage_gate.py --board .walgit/board.toml)       # 子目录等价（F4）
```

一次 `cat-file --batch` 的复核（`GIT_TRACE` 写到文件，避免被门捕获的 stderr 吞掉）：

```sh
GIT_TRACE=E:/tmp/gittrace.log python -I tools/hygiene/board_coverage_gate.py --board .walgit/board.toml
grep -E "rev-parse|for-each-ref|cat-file --batch" E:/tmp/gittrace.log
# 门自身只有 rev-parse --show-toplevel、for-each-ref refs/collab/inbox、cat-file --batch 各一次；
# 之后出现的 git 调用属于 walgit board 子进程。
```
