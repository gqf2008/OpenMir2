# tools/evidence-hash —— `SHA256SUMS.txt` 口径复核器

> 卡：`openmir2-evidence-hash-convention`（issue `915ea881`）。约定正文在
> `docs/Rust重写设计文档.md` §6.0-4；逐目录对账表在 [`RECONCILE.md`](RECONCILE.md) / [`RECONCILE.json`](RECONCILE.json)。

## 为什么有它

同一个坑在本仓库**已出现四次**，根因完全一样：`core.autocrlf=true` 的机器上
**工作树字节（CRLF）≠ 仓库字节（Git blob，LF）**，而 `SHA256SUMS.txt` 这类证据清单
到底按哪种字节记，此前从未写成约定：

| # | 出处 | 现象 | 现状 |
| --- | --- | --- | --- |
| 1 | C6 **F9-1** `tests/loadgate/**` | 清单按盘上 CRLF，记的全新检出 18/45 假 mismatch | 已用 `.gitattributes * -text` **冻结字节**修 |
| 2 | M4 **N-1** `tests/parity/evidence/M4/**` | 声明 LF(blob)，裸 `sha256sum` 对不上 | 已用 LF 归一清单 + 复核器修 |
| 3 | M2 会话生命周期 **N-1** `tools/m2-acceptance/evidence/session-lifecycle-{rerun,selftest}-20261012/**` | 清单按盘上 CRLF ⇒ 对仓库字节复核 8/11、5/6 mismatch | **本片改为仓库字节口径** |
| 4 | M2 envir `mir2-rs/tests/parity/envir-2026-10-10/parse-stats-cs.json` | 记的是 CRLF 值（该文件 `text eol=lf`，仓库字节是 LF）⇒ 两种口径都对不上 | **本片改为 LF 归一值** |
| 5 | m3cs `mir2-rs/tests/parity/m3/evidence-rng-isolate-attack-twophase-20261012/window/00-selfcheck-nostack.txt` | 清单记的是 **CRLF 字节**（`a9ca0b7c…`，blob 自己就带 CRLF），而 `mir2-rs/.gitattributes` 的 `*.txt text eol=lf` ⇒ 仓库字节是 **LF 归一值**（`68593747…`）；该片声明的 `verify GREEN 75/75` 在钉住 head 上实为 **RED 74/75** | **本片加「EOL 陷阱」fail-closed 牙**（见下；历史先例在 `170a51b9` 上可复现） |

## ③ 存量对账与逐条处置（2026-10-12）

全仓 `git ls-files` 命中的 `SHA256SUMS.txt` 现有 **8 个目录**（含合入 master `71bdb040` 后新增的
`M1/s4b-slice5-exitpaths`）。本片开工时是 **7 个**，修正前 `reconcile` = `4/7 GREEN；RED 3`
（原文：`evidence/2026-10-12/02-reconcile-before.txt`）；修正 + 合入 master 后 **8/8 GREEN**
（`04-reconcile-after.txt`）。表格（路径 / 件数 / 两种口径命中 / 缺件 / 结论）见 [`RECONCILE.md`](RECONCILE.md)
（由 `reconcile --md` 生成，不要手改；本节的处置登记为手工维护）。

| 目录 | 件数 | 修正前 | 处置 | 修正后 |
| --- | --- | --- | --- | --- |
| `mir2-rs/tests/parity/envir-2026-10-10` | 6 | 5/6（仓库与盘上都 5/6） | ① **改成 LF 归一**：`parse-stats-cs.json` 记的是 CRLF 值 `efc3671c…`，该文件 `*.json text eol=lf` ⇒ 仓库字节 `fa587e5c…`（`write` 改 1 行） | 6/6 |
| `mir2-rs/tests/parity/envir-2026-10-11` | 11 | 11/11 | 无需改：已按仓库字节（`MapInfo-synth.txt`/`Server.conf-synth` 由本目录 `.gitattributes` `-text` 冻结；`E:/MirServer/**` 为仓外输入，按盘上字节） | 11/11 |
| `mir2-rs/tests/parity/evidence/M1/s4b-slice4-crossread` | 51 | 51/51 | 无需改：整目录 `* -text` 冻结字节，`sha256sum -c` 亦可用 | 51/51 |
| `mir2-rs/tests/parity/evidence/M1/s4b-slice5-exitpaths` | 25 | 25/25 | 无需改：随 master `71bdb040` 合入，整目录 `* -text` 冻结字节 | 25/25 |
| `mir2-rs/tests/parity/m1-boundary-load/evidence` | 26 | 26/26 | 无需改：整目录 `* -text` 冻结字节 | 26/26 |
| `tools/m2-acceptance/evidence/m2svr-o3-pergateway-kick-20261012` | 4 | 4/4 | 无需改 | 4/4 |
| `tools/m2-acceptance/evidence/session-lifecycle-rerun-20261012` | 11 | 3/11（N-1） | ① **改成 LF 归一**：未加 `* -text`——原始捕获无需冻结字节，改清单比改 8 个 blob 改动更小；README §4 已同步口径（`write` 改 8 行） | 11/11 |
| `tools/m2-acceptance/evidence/session-lifecycle-selftest-20261012` | 6 | 1/6（N-1 同族） | ① **改成 LF 归一**（`write` 改 5 行） | 6/6 |

“未列入清单”的观察（信息列，**不作判据**）：`envir-2026-10-11` / `s4b-slice4-crossread` /
`s4b-slice5-exitpaths` / `m1-boundary-load/evidence` 的 `.gitattributes`（元数据）、
`session-lifecycle-rerun-20261012` 的 `README.md`。
若要严格的多件检查：`--check-extra --exclude .gitattributes --exclude 'README*'`。

## 口径（唯一权威定义）

`SHA256SUMS.txt` 里的 hash **一律按「仓库字节」记**：

```
SHA-256( Git blob 内容 )
```

- `text` 文件（`text eol=lf` / `auto` / 未指定 + `core.autocrlf=true`）⇒ **CRLF→LF 归一后的 sha256**；
- `.gitattributes` 里 `-text` **冻结**的文件 ⇒ **盘上原始字节**（blob 原样存）；
- **仓外输入**（如 `E:/MirServer/**`，没有 blob）⇒ **盘上原始字节**。

**确实要按盘上字节记的目录**（多为 GB2312/CRLF 原始捕获，需要 `sha256sum -c` 能跑）必须
**同时**放 `.gitattributes` 的 `* -text` 冻结字节，复核时显式 `--disk-bytes`。
先例：`tests/loadgate/.gitattributes`、`mir2-rs/tests/parity/m1-boundary-load/evidence/.gitattributes`。

## EOL 陷阱牙（2026-10-12，同族坑第 5 次；历史先例：第 4 次＝ `170a51b9`）

「仓库字节口径能算出正确值」还不够——**作者手抄清单时仍会用工作树原始字节**，于是得到一条只在自己
工作树/提交物里自洽、在干净检出上不可复现的清单。本工具因此加了一条**主动咬住 EOL 陷阱**的 fail-closed
判据（`find_eol_traps`）：

对清单里每个**在仓库内、会被 git 做行尾归一**（`text` 属性非 `-text`、且非二进制）的文件：

- 若清单里记的 hash 等于**工作树原始字节**的 sha256（盘上 CRLF），而仓库口径（LF 归一 / git blob）是另一个值；**或**
- 若清单里记的 hash 等于**该文件 git blob 的原始字节**（blob 自己就带 CRLF），而仓库口径是 LF 归一值

⇒ **直接判红并点名**（`EOL 陷阱: <path>`，列出「清单记的原始字节」与「仓库口径应为」两个值），
**在默认口径与 `--disk-bytes` 下都判红**——不许靠「清单自洽」拿到假绿（m3cs 片正是 `--disk-bytes`/
本地 `-text` 自洽而声明的 GREEN 在钉住 head 上不成立）。`-text` 冻结文件与仓外输入不适用本条
（按盘上字节就是它们声明的口径），所以**不会误伤** `reconcile` 里「盘上 CRLF ≠ blob」那批合规目录
（清单已按 LF 记，`reconcile` 仍全绿）。

修法：`write` 把清单改写成仓库字节口径；若确实要按盘上字节记，目录必须声明 `.gitattributes * -text`。
夹具与原始件：`tools/evidence-hash/fixtures/eol-trap/demo.py`（`python -I tools/evidence-hash/fixtures/eol-trap/demo.py`）
与 `tools/evidence-hash/evidence/2026-10-12-eol-tooth/`。

## 用法

```bash
# 复核单个清单 / 目录（默认 = 仓库字节口径）
python -I tools/evidence-hash/verify_sha256sums.py tools/m2-acceptance/evidence/session-lifecycle-rerun-20261012

# 冻结字节目录：显式切盘上字节口径
python -I tools/evidence-hash/verify_sha256sums.py --disk-bytes mir2-rs/tests/parity/m1-boundary-load/evidence

# 多件牙齿：目录里未列入清单的文件也判红
python -I tools/evidence-hash/verify_sha256sums.py --check-extra <目录>

# 全仓逐目录对账（③）
python -I tools/evidence-hash/verify_sha256sums.py reconcile \
  --md tools/evidence-hash/RECONCILE.md --json tools/evidence-hash/RECONCILE.json

# 改了口径 / 修历史遗留后，显式把清单改写到仓库字节口径
python -I tools/evidence-hash/verify_sha256sums.py write <目录> [<目录> ...]

# 牙齿：改 hash / 多件 / 少件 / 口径切换 / 冻结声明 / EOL 陷阱×2 / 不误伤 各必红
python -I tools/evidence-hash/verify_sha256sums.py selftest

# EOL 陷阱夹具（临时 git 仓库里构造「盘上 CRLF / blob LF」与「blob 含 CRLF」两种样例，红→绿）
python -I tools/evidence-hash/fixtures/eol-trap/demo.py
```

**退出码**：`0` = 绿；`1` = 红（mismatch / 缺件 / 未声明的盘上字节口径 / **EOL 陷阱**）；`2` = 用法或前置错误
（目标或清单不存在、清单无法解析、`git` 不可用）。**任何前置缺失都非零退出，绝不静默通过。**

`write` 是**显式**动作（改动证据后才跑），不会在 `verify`/`reconcile` 里被自动调用——
清单是「被审计证据」的锚点，若每次跑都自动改写，它就退化成恒绿。

## 结果字段（`--json`）

`verify` 输出数组，每项：`dir/manifest/mode/listed/ok/mismatch/missing/extra/eol_trap/green`。
`reconcile` 输出数组，每项：`manifest/dir/listed/repo_ok/disk_ok/missing/extra/external/
gitattributes/freeze_attr/verdict/why`。`repo_ok` = 仓库字节命中数，`disk_ok` = 盘上字节命中数，
`verdict` = `GREEN`/`RED`（有 EOL 陷阱时直接 `RED` 并点名）。

## 原始输出（证据四件套）

`evidence/2026-10-12/`：

| 文件 | 内容 |
| --- | --- |
| `01-selftest.txt` | `selftest` 6/6（口径 + 改 hash / 多件 / 少件 / 口径切换 / 冻结声明） |
| `02-reconcile-before.txt` | 修正前全仓对账：`4/7 GREEN；RED 3`（三个不一致目录 + 原因） |
| `03-write-fix.txt` | 显式 `write` 修正 3 个清单：`14 行 hash 变更` |
| `04-reconcile-after.txt` | 修正后全仓对账：`7/7 GREEN` |
| `05-demo-green.txt` | 未修的合规目录（`m1-boundary-load/evidence`）默认口径与 `--disk-bytes` 均 `26/26` |
| `06-redcheck-1byte.txt` | 红检：把被引件改 1 字节 ⇒ `RED 25/26`；`git checkout --` 还原后恢复 `26/26` |

以上原始件**不另附 `SHA256SUMS.txt`**：`04-reconcile-after.txt` 是本工具自己的输出，若列入清单再复核
会自引用（清单验证的输出文件正是输出本身）；它们以**交付 patch / 合并提交号**锚定（证据四件套的第四项）。

同一约定也适用于 `evidence/2026-10-12-eol-tooth/`（EOL 陷阱牙的原始件，同样不附 `SHA256SUMS.txt`）。

## 边界

- 只读证据、只写 `SHA256SUMS.txt`（`write`）与对账报告；**不碰** `src/**`、客户端、表结构、
  脚本语法、数据格式。
- 不依赖网络、不起服务、不占端口；需要 `git`（`check-attr` 解析 `text` 属性）。
