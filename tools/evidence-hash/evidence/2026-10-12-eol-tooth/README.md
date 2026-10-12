# EOL 陷阱牙 —— `SHA256SUMS.txt` 复核器 fail-closed（2026-10-12）

> 卡：`openmir2-m3-attack-pair-fixture`（owner 2026-10-12 23:30「同族坑第 4 次」）。
> 写集：`tools/evidence-hash/**`（零 `.rs`、零 `src`、零 `sql`、零 `MirClient`）。纯离线，未起栈、未占端口。

## 做了什么

`tools/evidence-hash/verify_sha256sums.py` 新增 **EOL 陷阱** fail-closed 判据（`find_eol_traps`）：

对清单里每个**在仓库内、会被 git 做行尾归一**（`text` 属性非 `-text`、且非二进制）的文件：

1. 清单记的 hash == **工作树原始字节**（盘上 CRLF）的 sha256，而仓库口径（LF 归一 / git blob）是另一个值；**或**
2. 清单记的 hash == 该文件 **git blob 原始字节**（blob 自己就带 CRLF）的 sha256，而仓库口径是 LF 归一值

⇒ **直接判红并点名**（`EOL 陷阱: <path>`，列出「清单记的原始字节」与「仓库口径应为」两个值）。
**默认口径与 `--disk-bytes` 下都判红**——不许靠「清单自洽」拿到假绿。

- `-text` 冻结文件与仓外输入**不适用**（按盘上字节就是它们声明的口径）⇒ 不会误伤
  `reconcile` 里「盘上 CRLF ≠ blob」那批**已按 LF 记账**的合规目录（回归见 `03`）。
- 修法：`write` 改写清单到仓库字节口径；若确实要按盘上字节记，目录必须声明 `.gitattributes * -text`。

同时更新 `tools/evidence-hash/README.md`：口径段 + 新「EOL 陷阱牙」节 + 用法/退出码/结果字段。
夹具：`tools/evidence-hash/fixtures/eol-trap/demo.py`（临时 git 仓库里构造两种样例，红→绿，自带断言）。

## 原始件（四件套：命令 + 输出 + 路径 + hash/提交号）

| 文件 | 内容 |
| --- | --- |
| `00-commands.txt` | 全部命令（自检 / 夹具 / 对账 / 历史复现 / 四门禁） |
| `01-selftest.txt` | `selftest` **11/11**（含 EOL 陷阱×2 + 不误伤；原 6/6 已扩为 11/11） |
| `02-fixture-demo.txt` | 夹具：样例1「盘上 CRLF / blob LF」与样例2「blob 含 CRLF / 盘上 LF」各红→绿；`--disk-bytes` 也红 |
| `03-reconcile-after.txt` | 回归：全仓逐目录对账仍 **72/72 GREEN**（新牙未误伤任何现有目录） |
| `04-historical-170a51b9.txt` | 历史先例复现：m3cs 片提交 `170a51b9` 上判 **RED 74/75** 并点名 `window/00-selfcheck-nostack.txt` |
| `05-gates.txt` | 四门禁：fmt / clippy / test（84 suites/682 passed/0 failed/16 ignored）/ msgcodegen=604 |

产物 hash / 提交号：见交付 `patch` 条目与合并提交（本目录**不另附 `SHA256SUMS.txt`**，原因同
`evidence/2026-10-12/`：`reconcile` 输出是本工具自己的输出，列入清单会自引用）。

## 为什么这条牙是必要的

「仓库字节口径能算出正确值」不等于作者会按它记清单。m3cs 片（`170a51b9`）把
`window/00-selfcheck-nostack.txt` 的 **CRLF 原始字节** `a9ca0b7c…` 抄进清单，而
`mir2-rs/.gitattributes` 的 `*.txt text eol=lf` 决定了仓库口径是 LF 归一的 `68593747…`；
该片声明 `verify GREEN 75/75`，在**钉住 head** 上实为 `RED 74/75`。这类清单在作者自己的工作树 /
提交物里「自洽」（`--disk-bytes` 更是直接判绿），却无法从干净检出复现 —— 本牙把它变成
**主动、点名、且在两种口径下都红** 的失败，堵住「清单自洽 = 假绿」。

## 残留物

- `E:/tmp/eol-*.txt`、`E:/tmp/eol-gates-*.txt`、`E:/tmp/eol-test-full*.txt`：本片仓外临时中间物，可删。
- `E:/tmp/eol-hist-170a51b9`：历史复现用的临时 worktree，已 `git worktree remove` + `prune`（`worktree list` 无残留）。
- `CARGO_TARGET_DIR=E:/tmp/openmir2-m3bag-target`：隔离构建目录（四门禁用），按既有口径保留。
