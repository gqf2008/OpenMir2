# tools/oracle 哈希环境无关化：`Get-FileHash` → .NET `SHA256`（同 preflight 口径）

> 卡：`openmir2-m2-acc-hash-env-independence`（本文即该片的证据四件套）。
> 上一片：`openmir2-m2-acc-preflight-modulepath`（`2f58086c` → master `677347d4`，reviewer-6 `39f0ca6b` approve）。
> 本片收口 reviewer-6 在该片登记的同类 Info（`tools/oracle/**` 两处），并**只登记**别车道其余同类点。

## 1. 缺陷类（同一根因，preflight 已证）

`Get-FileHash` 属于 `Microsoft.PowerShell.Utility`。当父进程（PS7 lane）把 `PSModulePath`
指成 PS7 优先时，**5.1 子进程**会加载 PS7 的 Utility（该模块不导出 `Get-FileHash`）⇒ 脚本在
`$ErrorActionPreference = 'Stop'` 下抛 `CommandNotFoundException` 崩溃 ⇒ **同一命令按环境变色**
（本机 lane 实况：`Get-FileHash MISSING`，见 `env-and-equivalence.txt` 第一段）。

`[System.Security.Cryptography.SHA256]` 不依赖模块解析，与
`(Get-FileHash -LiteralPath <path> -Algorithm SHA256).Hash` **逐字节相同**
（同一文件实测一致，见 `env-and-equivalence.txt`）。

## 2. 改动（改前 / 改后，行号为改前）

| 位置 | 改前 | 改后 |
| --- | --- | --- |
| `tools/oracle/deploy_gamesvr.ps1:128`（`-WhatIf` 比较） | `(Get-FileHash -LiteralPath $src).Hash -eq (Get-FileHash -LiteralPath $dst).Hash` | `(Get-Sha256Hex $src) -eq (Get-Sha256Hex $dst)` |
| `tools/oracle/deploy_gamesvr.ps1:172-173`（铺后复核） | `(Get-FileHash -LiteralPath ...).Hash.Substring(0, 16)` | `(Get-Sha256Hex ...).Substring(0, 16)` |
| `tools/oracle/reconcile_deployment.ps1:99-101`（`Get-FileHash16`） | `(Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.Substring(0, 16)` | `(Get-Sha256Hex $Path).Substring(0, 16)` |
| 新增 `tools/oracle/hashutil.ps1` | — | `Get-Sha256Hex` / `Get-Sha256Hex16`（.NET）+ `Test-HashUtilSelfTest`（钉住标准 SHA256） |

**不放宽任何判据**：比较语义、16 位截断、`MATCH/DRIFT` 判定、`SINGLE-BUILD` 结论全部原样；
只把「哈希从哪来」换成环境无关实现。

## 3. 判据（红 / 绿，原始输出）

### 3.1 改前（污染环境，崩溃）

- `before-deploy-whatif.txt`：`deploy_gamesvr.ps1 -WhatIf` 在 `:128` 抛
  `Get-FileHash : The term 'Get-FileHash' is not recognized ...`，**rc=1**。
- `before-reconcile-selftestred.txt`：`reconcile_deployment.ps1 -SelfTestRed` 在 `:101` 抛同一异常，**rc=1**。

### 3.2 改后（同一污染环境，GREEN）

- `after-deploy-selftestred.txt`：`-SelfTestRed` **rc=0**（钉住值 `fdeab9ac…1108`）。
- `after-reconcile-selftestred.txt`：`-SelfTestRed` **rc=0**
  （`identity=MATCH bitflip=DRIFT missing-src=DEPLOY-ONLY` + 标准值被钉住）。
- `after-deploy-whatif.txt`：`-WhatIf` 集成路径 **rc=0**，10 个文件逐条打印 `已一致`
  （此前该路径在 `:128` 崩溃）。

### 3.3 等价性（新旧实现同值）

- `env-and-equivalence.txt`：`E:\MirServer\M2GameSvr\M2Server.dll` 上
  `Get-FileHash` = `Get-Sha256Hex` = `1f450d1e902909f40243a33bfd0b45789ed55b9c1f1e1b384b58c0cdb494d124`。

### 3.4 改坏必红（改完已还原）

- `redcheck-constant.txt`：`Get-Sha256Hex` 返回常量 `'0'*64` ⇒ deploy/reconcile 的 `-SelfTestRed` **都 rc=1**。
- `redcheck-md5.txt`：SHA256 换成 MD5 ⇒ `-SelfTestRed` **rc=1**
  （「翻一位」这条牙抓不到，靠「钉住标准值」这条抓到——证明钉住值必要）。
- 还原后复跑 `deploy_gamesvr.ps1 -SelfTestRed` = GREEN（`after-deploy-selftestred.txt` 即还原态）。

## 4. 四门禁（合并 master `fa456481` 后在交付树上复跑；隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m2svr-target`，`--offline`）

| 门禁 | 结果 | 原始件 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | rc=0 | `gate-fmt.txt` |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 | `gate-clippy.txt` |
| `cargo test --workspace --offline` | **82 suites · 670 passed · 0 failed · 16 ignored** | `gate-test.txt` |
| `cargo run -p msgcodegen -- --check` | **604** 常量一致 | `gate-msgcodegen.txt` |

（工作区根在 `mir2-rs/`；在仓库根跑会报「找不到 Cargo.toml」——本片如实记录。）

## 5. 边界与登记

**本片改动（写集）**：仅 `tools/oracle/**`（2 个脚本 + 1 个新工具 + 本证据目录 + README 一行）。

**只登记、不改**（不在本 lane 写集；各自属主需另开片，同一根因，修法同本片）：

| 位置 | 归属车道 |
| --- | --- |
| `tools/botload/load_gate.ps1:342`（`artifacts.sha256.json` 产物清单） | 夹具/压测（C6/loadgate） |
| `mir2-rs/tests/parity/m3/gate_guards.ps1:555`、`:694` | `m3bag`/`m3cs`（`tests/parity/m3/**`） |
| `mir2-rs/tests/parity/m3/rng-baseline.ps1:45` | 同上 |
| `mir2-rs/tests/parity/script-dispatch/run_dispatch_regression.ps1:47` | S/B 线 |

**未改**：`src/**`、`MirClient`、表结构、脚本语法、数据格式；未起栈、未占端口（本片全程单测/构造层）。

## 6. 复核

```bash
python -I tools/evidence-hash/verify_sha256sums.py --check-extra tools/oracle/evidence/m2svr-hash-unify-20261012
```

证据清单按**仓库字节（LF 归一 / git blob）**口径；本目录无需 `.gitattributes` 冻结。

**干净检出复核**：`clean-clone-verify.txt`（`git worktree add --detach E:/tmp/m2svr-clean-verify 27b75c45`
全新 checkout 上）——`--check-extra` 14/14 GREEN + 两个 `-SelfTestRed` GREEN + `-WhatIf` 集成 rc=0。

作者 = `openmir2-worker-11`；**不 merge、不自签 review**。
