# 仓库卫生（同族第二片）：`tools/**` EOL 覆盖（卡 `openmir2-tools-eol-attributes`）

> 机器口径：Windows + `git core.autocrlf=true`（本机默认），Python 3（`python -I`）。
> base = **`363c0bd1`**（origin/master）；code commit = **`c75e6486`**（只含 `tools/.gitattributes`）。
>
> 第一片 = `openmir2-gitattributes-eol-coverage`（`mir2-rs/.gitattributes`，已合入 master `031ff37e`）；
> 本片是它的同族第二片，把覆盖扩到 `tools/**`。

## 结论（一句话）

新增 **`tools/.gitattributes`**，把 `tools/**` 下实际承载文本证据/夹具/脚本的扩展名声明为 `text eol=lf`，
使 `core.autocrlf=true` 的**干净检出**不再把这些文件落成 CRLF ⇒
`verify_sha256sums.py eolcheck tools` 由 **RED 500/539 → GREEN 540/540**，且**不改任何被跟踪文件的 blob 内容**
（`tools/**` 的文本 blob 本已全是 LF；本组扩展名没有一个 blob 自身含 CRLF）。

| 范围（一次性 clone，`--no-checkout` + checkout 该 commit） | base `363c0bd1` | code commit `c75e6486` |
| --- | --- | --- |
| `eolcheck tools` | **RED 500/539**（rc=1） | **GREEN 540/540**（rc=0） |
| `git ls-files --eol tools` | `22 i/-text / 507 i/lf / 10 i/none` | `22 i/-text / 508 i/lf / 10 i/none`（全部文本件 `w/lf`） |
| `git status`（新检出后） | — | **空**（无归一脏） |

## 根因

`tools/**` 此前**没有** `.gitattributes`（仓库根也没有）。在这台 `core.autocrlf=true` 的机器上，
**未声明**的文本扩展名在检出时被自动转成 CRLF（工作树 CRLF / 仓库 blob LF），
而 `tools/**` 承载证据、夹具与判据脚本，`verify_sha256sums.py eolcheck` 按「盘上字节 vs 仓库口径（LF）」
判红 ⇒ **干净检出恒假红**（同族坑，继 `mir2-rs/.gitattributes` 之后的 `tools/**` 部分）。

**与冻结目录的关系**：`tools/m2-acceptance/evidence/m2svr-featurechanged-20261012/.gitattributes` 的 `* -text`
（以及各 `shots/` 的裸字节产物）**优先级更高**，本文件不会改变那些冻结目录的口径；
`*.png` 等二进制扩展名**故意不声明**（git 二进制检测已按字节处理）。

## 逐条扩展名与理由（base 干净检出的 RED 命中数）

| 扩展名 | base RED 命中 | 承载什么（样例） | 为什么必须 LF |
| --- | --- | --- | --- |
| `*.txt` | 302 | 证据包原始读数、`SHA256SUMS`、门禁日志 | 逐行/逐字节比对；清单按 git blob 口径记 |
| `*.md` | 68 | `tools/**/README.md`、报告 | 文本可 diff |
| `*.ps1` | 34 | `tools/botload/run_bots.ps1`、`tools/oracle/*.ps1`、门禁驱动 | 脚本源码需可复现、可 diff |
| `*.json` | 26 | 判据/清单（`evidence-hashes.json` 等） | 逐字段比对 |
| `*.jsonl` | 22 | 逐帧/逐条捕获 | 新行分隔 JSON，文本逐行比对 |
| `*.py` | 20 | 判据脚本（`verify_sha256sums.py`、各 `*_diff.py`） | 按行读写并算 hash |
| `*.ndjson` | 9 | 金标准/流水捕获 | 文本逐帧比对 |
| `*.sh` | 4 | `tools/orchestration/*.sh` | shell 脚本需 LF |
| `*.cs` | 4 | `tools/oracle/**`、`tools/capture/GoldenExport` 探针源码 | 源码可复现 |
| `*.sha256` | 3 | 截图/日志校验和清单 | 清单文本 |
| `*.csproj` | 3 | 探针工程文件 | XML 文本 |
| `*.tsv` | 2 | 夹具向量 | 逐字段/逐字节对拍对象 |
| `*.dpr` | 2 | Delphi 探针工程 | 源码可复现 |
| `*.conf` | 1 | `tools/orchestration/lanes.example.conf` | 配置文本 |
| `*.patch` | 0（本树无） | 证据里的 diff（预留） | patch 文本必须原样可 apply |
| `*.log` | 0（本树无） | 日志（预留） | 文本逐行比对 |
| `.gitattributes` | — | 本文件自身 | 属性文件需 LF |

合计 500 = 302+68+34+26+22+20+9+4+4+3+3+2+2+1。未被点名的 39 件 = 22 个 `.png`（`* -text` 冻结）
+ 10 个 0 字节 `.txt` + 7 个更深冻结目录里的文件。

## 对照卡的 `machine_checkable_acceptance`

| # | 判据 | 本片证据 |
| --- | --- | --- |
| ① | 盘点 `tools/**` 扩展名清单 + 当前无覆盖证据（`git check-attr text` = `unspecified`；干净检出 `eolcheck` = RED） | `01-baseline-clean-clone-eolcheck.txt`（RED 500/539）+ `01b-baseline-breakdown.txt`（`check-attr` unspecified + 扩展名分布 + `i/crlf=0`） |
| ② | 覆盖：新增属性文件（**`tools/.gitattributes`**，逐条理由见上表）；**零 blob 内容改动**（`git diff --stat <base> HEAD -- ':!.gitattributes' ':!**/.gitattributes'` 为空） | `04-only-attributes-no-blob-change.txt`：code commit 只 `A tools/.gitattributes`；该命令**空**；`src/ sql/ MirClient` 空；5 个样例逐 blob oid 相同 |
| ③ | 新鲜检出 `eolcheck tools` ⇒ **GREEN**；改坏必红（漏一个扩展名 ⇒ 仍点名） | `02-fixed-clean-clone-eolcheck.txt`（GREEN 540/540）+ `02b-fixed-breakdown.txt`（`w/lf`、`git status` 空）+ `03-mutation-red-drop-ps1.txt`（删 `*.ps1` ⇒ **RED 34/540**，点名 `.ps1`） |
| ④ | 写清「属性只影响新检出；已存在工作树需新检出才变 LF」 | `05-worktree-status.txt`（本 worktree tracked 侧干净；主检出 `GIT_OPTIONAL_LOCKS=0 status` = 干净；含重检出做法/范围） |
| ⑤ | 四门禁 + 证据四件套 + 边界零改动；`patch` + 紧接 `status: needs-review`；不 merge、不自签；先 `git fetch && git merge master` | `06-gates.txt`（fmt0 / clippy0 / test **86 suites 771 passed 0 failed 18 ignored** / msgcodegen 604）；本目录 + 卡线程交付条目；分支自 `origin/master` 363c0bd1 直接新建 |

## 改坏必红（判据③）

`03-mutation-red-drop-ps1.txt`：从 `tools/.gitattributes` 删掉 `*.ps1     text eol=lf` 一行并重新检出后，
`eolcheck tools` ⇒ **RED 34/540**，逐条点名 `.ps1`（`git ls-files --eol` 显示 `w/crlf`）。⇒ 这条牙会咬住「漏扩展名」。

## 交付物分层（满足判据②③的字面口径）

- **code commit `c75e6486`**：只含 `tools/.gitattributes`（32 行）。判据②③在本提交上取读数。
- **evidence commit**（本目录）：只新增 `tools/evidence-hash/evidence/2026-10-13-tools-eol-attributes/**`，
  未改任何既有被跟踪文件（`git diff --stat c75e6486..HEAD -- ':!.gitattributes' ':!**/.gitattributes'` 只列本目录）。

## 残留（登记，不在本卡写集）

- **仓库根仍无 `.gitattributes`** ⇒ `src/**`、`sql/**`、`mir2-rs/` 外的其它树、以及根文件的未声明文本扩展名在**整仓** `eolcheck` 上仍是 CRLF。
  本卡写集只到 `tools/**`；`src/**` 是冻结 oracle，是否加根 `.gitattributes`（会改变工作树检出）应**另开卡由 owner 裁决**。
- **既有 worktree 的盘上文件**在补属性后不会自动变 LF；要本地 `eolcheck` 绿需新检出。本片只在 `E:/tmp` 的一次性 clone 上做，未动任何车道 worktree。
- 本 worktree 里 8 个**未跟踪**的 `mir2-rs/tests/parity/m3/sessions/csharp-20261012-*` 是别的卡残留，未纳入本片。
