# evidence-hashutil-hotfile-ledger-20261012 — M3 夹具 F1（SHA256 活文件）+ F2（攻击站点表同表）

- 卡：`openmir2-m3-combat-skills-align`（M3②③）／车道 `m3bag`／principal `openmir2-worker-4`。
- 来源：m3cs 在 `05ea1f21`（两相真机实跑）里登记的**两条夹具 finding**（都属 m3bag 写集，作者侧只登记）：
  - **F1（Important）**：`start-csharp-hook.ps1` 的 `Get-Sha256Hex` / `gate_guards.ps1` 的 `Get-M3Sha256Hex`
    用 `[System.IO.File]::ReadAllBytes`（`FileShare.Read`）读影子 GameSvr 以 `FileShare.ReadWrite` 持有的
    `rng-attack-stream.tsv` ⇒ Windows 共享冲突（`The process cannot access the file ... because it is being
    used by another process`），`start-csharp-hook.ps1 -IsolateAttack` 在写完 `ISOLATION_RECORDED` 前抛
    （栈已起/端口已就绪）⇒ **专用流 sha256 记不出来**（m3cs 只能置空 `attack_stream_sha256`）。
  - **F2（Minor）**：`attack_rng_ledger.py` 的 `RUST_ATTACK_RE` 仍是旧行号，漏
    `combat.rs:758/628/630/601/609/831/250` ⇒ 同一条 Rust 流水把攻击路径取数算成噪声（本片实测报
    `attack=2/段`）。

## 1. 改了什么（零 `.rs`）

| 文件 | 改动 |
| --- | --- |
| `mir2-rs/tests/parity/m3/m3hashutil.ps1`（新） | 唯一 SHA256 实现 `Get-M3Sha256Hex`：`FileStream(FileMode.Open, FileAccess.Read, FileShare.ReadWrite\|Delete)` 流式读；`Test-M3Sha256HotFileSelfTest` 钉住「活文件可读 + 旧 `ReadAllBytes` 必被拒」 |
| `mir2-rs/tests/parity/m3/gate_guards.ps1` | 删本地 `Get-M3Sha256Hex`，改 dot-source `m3hashutil.ps1`（函数名不变，所有调用点语义不变） |
| `mir2-rs/tests/parity/m3/start-csharp-hook.ps1` | 删本地 `Get-Sha256Hex`，改 dot-source；`Get-Sha256Hex` 调用点改 `Get-M3Sha256Hex`（hook dll / sites dump / 专用流 sha256） |
| `mir2-rs/tests/parity/m3/rng-baseline.ps1` | 删本地 `Get-Sha256`（保留 `Get-TextSha256`），改 dot-source + `Get-Sha256` 包装委托 |
| `mir2-rs/tests/parity/m3/run_attack_pair.ps1` | `-SelfCheckNoStack` 增**第 22 项**（F1 活文件 hash=标准锚 + 旧读法必被拒）与**第 22b 项**（`attack_rng_ledger.py selftest` 非绿即红）；汇总文案同步 |
| `mir2-rs/tests/parity/m3/attack_rng_ledger.py` | `RUST_ATTACK_RE`/`CSHARP_ATTACK_RE` 与 `rng-trace-probe/rng_ledger_diff.py` **逐字节同表**（Rust 补 `250/601/609/628/630/758/831`，C# 补 `BaseObject.GetAttackPower@`）；`selftest` 18→**27**（新站点必须分类为 attack、非站点为 noise） |
| 三份登记文档 | `evidence-rng-isolate-attack-20261012/README.md` §五、`evidence-rng-isolate-attack-csharp-20261012/README.md` §4.2、`M3②③活栈隔离口径Rust侧实现报告-20261012.md` §5 的 F1/F2 由「只登记/待修」改「已修（本片）」；`whitelist.md` T-29 增第十轮 |
| 本目录 | 证据 + `SHA256SUMS.txt` |

**默认行为无关**：这些改动只在「读文件 hash / 站点分类」的仪器路径上；对 oracle `src/**`、协议、DB、Rust 生产路径零影响（见 §4 门禁）。

## 2. 判据与原始证据

| 文件 | 命令 | 结果 |
| --- | --- | --- |
| `01-selfcheck-nostack-green.txt` | `run_attack_pair.ps1 -SelfCheckNoStack` | **rc=0 `SELFTEST_RED GREEN`**；第 22 项 `hash=fdeab9ac…1108（锚同）旧 ReadAllBytes 必被拒=True ok=True`；第 22b 项 `rc=0` |
| `03-ledger-selftest-green.txt` | `python attack_rng_ledger.py selftest` | **rc=0 `SELFTEST GREEN 27/27`** |
| `06-hotfile-repro.txt` | 直接复现 | holder(`FileShare.ReadWrite`) 下：旧 `File.ReadAllBytes` ⇒ `EACCES ... being used by another process`；新 `Get-M3Sha256Hex` ⇒ `fdeab9ac…1108`（=标准锚）；本机 `Get-FileHash exists=False`（佐证 .NET SHA256 口径） |
| `07-gates.txt` | `cargo fmt/clippy/test/msgcodegen`（隔离 `CARGO_TARGET_DIR`，从 `mir2-rs/`） | fmt_rc=0 / clippy_rc=0 / **test_rc=0（84 suites・682 passed・0 failed・16 ignored）** / msgcodegen_rc=0（604 条与 C# 源一致） |

证据文本口径：`01`–`06` 已 **LF 归一**（去掉所有 `\r`）。首稿的 `01/02` 含 `\r\r\n`（PowerShell 重定向包裹嵌套进程的 CRLF）⇒ repo 口径的 `normalize_lf` 不幂等、干净 clone `verify_sha256sums` 假红（同族坑见 `openmir2-evidence-hash-convention`）；去 CR 后 git blob 与检出字节一致，`recon`/`verify` 任一侧都绿。

## 3. 改坏必红（真跑 + `cmp` 还原逐字节相同）

| 文件 | 改坏 | 结果 |
| --- | --- | --- |
| `02-selfcheck-nostack-mut-f1-red.txt` | `m3hashutil.ps1::Get-M3Sha256Hex` 改回 `[System.IO.File]::ReadAllBytes` | `-SelfCheckNoStack` **rc=1 `SELFTEST_RED RED: sha256_hot_file_selftest=exception:...being used by another process`** |
| `04-ledger-mut-rust-red.txt` | `RUST_ATTACK_RE` 改回旧表 | `selftest` **rc=1 `SELFTEST RED 7/27`**（`rust-site-{758,628,630,601,609,831,250}-is-attack`） |
| `05-ledger-mut-csharp-red.txt` | `CSHARP_ATTACK_RE` 去掉 `BaseObject.GetAttackPower@` | `selftest` **rc=1 `SELFTEST RED 1/27`**（`csharp-atk-power-base-is-attack`） |

三次改坏后均 `cp` 还原 + `cmp` 逐字节相同、`grep -c MUTATION` = 0。

## 4. 未做 / 边界（如实登记）

1. **未起真机栈**：F1 的端到端效果（`start-csharp-hook.ps1 -IsolateAttack` 不再抛 + `attack_isolation.json`
   的 `attack_stream_sha256` 非空）需要 5000/7000/7100/7200 窗口，**本片不起栈**（端口实拍见 `00`：本片开工时
   九端口全 DOWN，仅共享 3306）。本片证的是**共享读语义**在生产实现上成立（第 22 项 + `06`），以及
   旧读法必被拒。真机复核归 m3cs 的下一轮取窗。
2. **F2 的「两文件同表」是人工对齐 + selftest 钉住站点分类**，未写「读对端源文件比对正则」的自动漂移门
   （避免跨文件源码解析脆弱）；若 m3cs 再改 `rng_ledger_diff.py` 的行号表，需同步本表（本片已把两侧拉到一致）。
3. 本片**未动** `attack_diff.py` / `run_pair.ps1` / `gate_guards.ps1` 的既有判据语义，也未放宽任何门。
4. F1 的 holder 语义复现依赖 Windows 共享模式；本机为 Windows + PS 5.1（`06` 实测），Linux/mono 下
   `FileShare` 语义不同，此自检不作为跨平台判据。

## 5. 残留物清单

- 仓外临时物（可删）：`E:/tmp/m3bag-f1-selftest.txt`、`E:/tmp/m3bag-f1-mut-red.txt`、
  `E:/tmp/m3hashutil.bak`、`E:/tmp/attack_rng_ledger.bak`、`E:/tmp/led.bak`、`E:/tmp/mk06.ps1`、
  `E:/tmp/repro_hot.ps1`、`E:/tmp/t_helper.ps1`、`E:/tmp/t_parse.ps1`、`E:/tmp/t_guard.ps1`、
  `E:/tmp/m3bag-thread-now.json`、`E:/tmp/openmir2-m3bag-target`（隔离 target）。
- 仓库内：`git status` 只含本片改动；未起栈 ⇒ 无进程/端口残留（仅共享 3306）。
