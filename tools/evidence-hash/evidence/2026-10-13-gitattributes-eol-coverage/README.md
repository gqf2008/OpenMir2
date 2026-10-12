# 仓库卫生：`mir2-rs/.gitattributes` EOL 覆盖（卡 `openmir2-gitattributes-eol-coverage`）

> 机器口径：Windows + `git core.autocrlf=true`（本机默认），Python 3（`python -I`）。
> base = `384379fb`（origin/master）；code commit = **`0dc48618`**。

## 结论（一句话）

给 `mir2-rs/.gitattributes` 补上实际承载文本证据/脚本的扩展名（`*.py *.tsv *.ps1 *.patch *.ndjson *.js`）
为 `text eol=lf`，使 `core.autocrlf=true` 的干净检出不再把这些文件落成 CRLF ⇒
`verify_sha256sums.py eolcheck` 由 **RED → GREEN**，且**不改任何被跟踪文件的 blob 内容**。

| 范围（clean clone，`--no-checkout` + checkout 该 commit） | base `384379fb` | code commit `0dc48618` |
| --- | --- | --- |
| `eolcheck mir2-rs/tests/parity/m3` | **RED 79/1706**（`.tsv`52 `.py`15 `.ps1`11 `.patch`1） | **GREEN 1706/1706** |
| `eolcheck mir2-rs` | **RED 98/2278**（`.tsv`53 `.py`23 `.ps1`13 `.ndjson`7 `.patch`1 `.js`1） | **GREEN 2278/2278** |

> 卡上引的「RED 77/1648」是本仓库更早一轮的读数（树更小）；本片以当前 base 的干净检出新测为准。

## 根因

`mir2-rs/.gitattributes` 此前只声明了 `*.rs/*.toml/*.md/*.json(l)/*.log/*.txt/*.cs/*.csproj/Cargo.lock/.gitignore/.gitattributes`。
在这台 `core.autocrlf=true` 的机器上，**未声明**的文本扩展名在检出时被自动转成 CRLF（工作树 CRLF / 仓库 blob LF），
而夹具与证据把 `.py/.tsv/.ps1/.patch/.ndjson/.js` 当**文本逐行/逐字节**用 ⇒ `eolcheck` 在干净检出厂恒假红。**同族坑第 5 次**
（C6 F9-1 / M4 N-1 / lifecycle N-1 / `170a51b9` / 本次）。

## 逐条扩展名与理由

| 扩展名 | 承载什么（clean clone 点名） | 为什么必须 LF |
| --- | --- | --- |
| `*.py` | m3/M4/parity 判据脚本（`attack_diff.py`、`scoped_diff.py`、`server_state_gate.py`、`check_predicates.py` …） | 判据/夹具按行读写并算 hash；CRLF 会让证据 hash 与仓库口径不一致 |
| `*.tsv` | 夹具向量、两侧流 provenance、`server_state_segment.tsv`（79 件里 52 件） | 逐字段/逐字节对拍的对象本身 |
| `*.ps1` | 两侧栈驱动/门禁（`run_pair.ps1`、`run_attack_pair.ps1`、`gate_guards.ps1`、`start_*.ps1`） | 脚本源码要可复现、可 diff |
| `*.patch` | 证据包里的 diff 证据（如 `05-wording-diff.patch`） | patch 文本必须原样可 apply |
| `*.ndjson` | 金标准/基线帧捕获（`c-line-baseline-*.frames.ndjson`、S1 stages） | 新行分隔 JSON，文本逐帧比对 |
| `*.js` | `mir2-rs/tools/diff-parity.js` | 工具脚本源码 |

> 更具体的冻结规则不受影响：`mir2-rs/tests/parity/.gitattributes` 的 `fixtures/** -text` / `golden/** -text` /
> `m3/fixtures/** -text` 在更深目录、优先级更高，GBK/原始捕获仍按字节冻结。

## 对照卡的 `machine_checkable_acceptance`

| # | 判据 | 本片证据 |
| --- | --- | --- |
| ① | 覆盖 `*.py/*.tsv/*.ps1/*.patch` 等并逐条给理由 | 见上表；`04-only-attributes-no-blob-change.txt` 给出 diff |
| ② | `eolcheck` RED → GREEN | `01-baseline-clean-clone-eolcheck.txt`（RED）+ `01b-baseline-breakdown.txt` + `02-fixed-clean-clone-eolcheck.txt`（GREEN），均 rc 明确 |
| ③ | `git diff --stat <base> HEAD -- ':!mir2-rs/.gitattributes'` 为空 | `04-…txt`：该命令**空**；`--diff-filter=M` 只列 `mir2-rs/.gitattributes`；无新增内容文件 |
| ④ | 各 worktree `git status` 干净；重检出范围给清 | `05-worktree-status.txt`：主检出 + 3 个车道 worktree tracked 侧干净；含「属性变更不弄脏已有 worktree」的实测与重检出步骤/范围 |
| ⑤ | 证据四件套 + 四门禁 + `src/ sql/ MirClient` 零改动 | 本目录 + `06-gates.txt`；`git diff --stat` 无 `src/ sql/ MirClient` |
| ⑥ | `patch` + 紧接 `status: needs-review`；不 merge、不自签 | 见卡线程（交付条目） |

**改坏必红**：`03-mutation-red-drop-tsv.txt`——从 code commit 删掉 `*.tsv text eol=lf` 并重新检出 ⇒
`eolcheck mir2-rs/tests/parity/m3` **RED 52/1706**、`eolcheck mir2-rs` **RED 53/2278**，点名 `.tsv`，rc=1。

## 交付物分层（为满足判据③的字面口径）

- **code commit `0dc48618`**：只含 `mir2-rs/.gitattributes`（`git diff --stat 384379fb 0dc48618 -- ':!mir2-rs/.gitattributes'` 为空）。
- **evidence commit**：只新增本目录（`tools/evidence-hash/evidence/2026-10-13-gitattributes-eol-coverage/**`），未改任何既有被跟踪文件。

## 复现

见 `00-commands.txt`。要点：

```sh
git clone --local --quiet --no-checkout . E:/tmp/eol-base3
git clone --local --quiet --no-checkout . E:/tmp/eol-fix4
(cd E:/tmp/eol-base3 && git checkout -q 384379fb && python -I tools/evidence-hash/verify_sha256sums.py eolcheck mir2-rs)   # RED 98/2278, rc=1
(cd E:/tmp/eol-fix4  && git checkout -q 0dc48618 && python -I tools/evidence-hash/verify_sha256sums.py eolcheck mir2-rs)   # GREEN 2278/2278, rc=0
```

## 残留（登记，不在本卡写集）

- **仓库根没有 `.gitattributes`** ⇒ `src/**`、`sql/**`、`tools/**`、`tests/**`、根文件里的未声明文本扩展名在**整仓** `eolcheck` 上仍是 CRLF
  （当前 base 的干净检出实测整仓 **RED 1601/4371**，其中 `mir2-rs/` 外 **1503** 件，最大宗是 `src/**` 的 `.cs` 898 件）。
  本卡写集只到 `mir2-rs/.gitattributes`；`src/**` 是冻结 oracle，是否加根 `.gitattributes`（会改变工作树检出）应**另开卡由 owner 裁决**。
- **既有 worktree 的盘上文件**在补属性后不会自动变 LF；要本地 `eolcheck` 绿需按 `05-worktree-status.txt` 的范围重检出
  （仅 `mir2-rs` 下被跟踪文件；无内容改动）。本片只在自己的 worktree（`.claude/worktrees/m3bag`）做了，未动共享主检出。
- worktree 里既有的**未跟踪/忽略**临时件（旧 `sessions/**`、probe `obj/bin`）不属于干净检出，也不属于本卡判据；
  本片仅把它在本 worktree 内归一为 LF 以让本地 `eolcheck` 全绿（不改任何被跟踪内容）。
