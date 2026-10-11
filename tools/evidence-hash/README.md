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

# 牙齿：改 hash / 多件 / 少件 / 口径切换 / 冻结声明 各必红
python -I tools/evidence-hash/verify_sha256sums.py selftest
```

**退出码**：`0` = 绿；`1` = 红（mismatch / 缺件 / 未声明的盘上字节口径）；`2` = 用法或前置错误
（目标或清单不存在、清单无法解析、`git` 不可用）。**任何前置缺失都非零退出，绝不静默通过。**

`write` 是**显式**动作（改动证据后才跑），不会在 `verify`/`reconcile` 里被自动调用——
清单是「被审计证据」的锚点，若每次跑都自动改写，它就退化成恒绿。

## 结果字段（`--json`）

`verify` 输出数组，每项：`dir/manifest/mode/listed/ok/mismatch/missing/extra/green`。
`reconcile` 输出数组，每项：`manifest/dir/listed/repo_ok/disk_ok/missing/extra/external/
gitattributes/freeze_attr/verdict/why`。`repo_ok` = 仓库字节命中数，`disk_ok` = 盘上字节命中数，
`verdict` = `GREEN`/`RED`。

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

## 边界

- 只读证据、只写 `SHA256SUMS.txt`（`write`）与对账报告；**不碰** `src/**`、客户端、表结构、
  脚本语法、数据格式。
- 不依赖网络、不起服务、不占端口；需要 `git`（`check-attr` 解析 `text` 属性）。
