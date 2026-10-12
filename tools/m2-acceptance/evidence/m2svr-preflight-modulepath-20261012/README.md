# M2 验收 preflight 判据诚信修复：`Get-FileHash` → .NET SHA256（PS7 优先 `PSModulePath` 下崩溃被误判为 RED）

- 卡：`openmir2-m2-acc-preflight-modulepath`（issue `3e0ce465`，作者按本仓库先例自建；owner 两次 comment 未派卡）
- 作者：`openmir2-worker-11` / worktree `m2-svr` / branch `worktree-m2-gamesvr-host`
- base：`refs/heads/master` = `6eec1e2e`（交付前已把 master FF 进本分支）
- 本片**只动** `tools/m2-acceptance/preflight.ps1`（本 lane 的 M2 验收交付物）；未动 `mir2-rs/**`、`src/**`、`MirClient`、别人的工具。

## 1. 缺陷（判据诚信，不是"环境问题就无视"）

`preflight.ps1` 是 M2 验收的「前置门」（⓪）。`selftest.ps1` 的自述口径明确写着
**"只看退出码会把『脚本崩了』误当『判红』"**，用一个「退出码 + 输出关键字」双断言把每条口径跑一遍。
但 `preflight.ps1:126` 用了 `Get-FileHash`：

```powershell
$sha = (Get-FileHash -Path $GoldenPath -Algorithm SHA256).Hash.ToLowerInvariant()
```

`Get-FileHash` 属 `Microsoft.PowerShell.Utility`。lane 进程（PS7 orchestrator）把 `PSModulePath`
注入成 **PS7 优先**（`e:\Users\gxh\Documents\PowerShell\Modules;…;C:\Windows\System32\WindowsPowerShell\v1.0\Modules`），
`selftest.ps1` 的 `Run-Ps` 起 `powershell`(5.1) 时会**继承**该变量 ⇒ 5.1 自动加载 PS7 的
`Microsoft.PowerShell.Utility 7.0.0.0`（**不导出** `Get-FileHash`）⇒ 脚本在 `golden:` 段抛
`CommandNotFoundException`；`preflight.ps1` 的 `$ErrorActionPreference = "Stop"` 让它**直接终止**（rc=1）。
`selftest.ps1` 因此把**崩溃**当成了 **RED**：本 lane 实测 46 条里 **5 条 FAIL**（全部由这一处崩溃派生——
崩溃发生在 golden schema 检查之前，所以 schema 那条也一起假红）。

原始输出：`preflight-before.txt`（`Get-FileHash : The term ... is not recognized`）、
`selftest-before.txt`（`SUMMARY total=46 failed=5`）。

## 2. 修法（最小、环境无关、行为等价）

改用 .NET 的 `System.Security.Cryptography.SHA256`（不依赖模块解析）；结果与
`Get-FileHash -Algorithm SHA256` 逐字节相同（对本金标准实测 = `f34a6d11…a7ed9`，即
`-ExpectedGoldenSha` 钉住值）。改后 `preflight.ps1:126-133`：

```powershell
$shaAlg = [System.Security.Cryptography.SHA256]::Create()
try {
  $sha = ([System.BitConverter]::ToString($shaAlg.ComputeHash([System.IO.File]::ReadAllBytes($GoldenPath))) -replace '-', '').ToLowerInvariant()
} finally { $shaAlg.Dispose() }
```

**没有**放宽任何判据、没有改 golden 钉住值、没有动 schema 检查。

## 3. 判据（绿）

| 判据 | 改前 | 改后 | 原始件 |
|---|---|---|---|
| `preflight.ps1` 直跑（本 lane 污染环境） | rc=1（崩溃） | **rc=0**，`SUMMARY ok=18 missing=0 red=0`，`GREEN: 前置齐备` | `preflight-before.txt` / `preflight-after.txt` |
| `selftest.ps1 -SkipLivePorts` | `total=46 failed=5` | **`total=46 failed=0`**（rc=0） | `selftest-before.txt` / `selftest-after.txt` |

## 4. 改坏必红（牙齿）

把 `.NET` 算出的 sha 改成常量 `"000…0"`（即让"金标准身份"检查必不匹配）⇒
`preflight/在库前置齐备=GREEN` 期望 rc=0 实得 rc=1 ⇒ **selftest FAIL**（并派生另外 3 条），
`SUMMARY total=46 failed=4`、`SELFTEST FAIL`。原始件 `selftest-redcheck.txt`。
⇒ 证明本处 sha 计算**真的被用于判据**且必须正确（不是"改了也不红"的死代码）。还原后复跑 46/46（`selftest-after.txt`）。

## 5. 四门禁（隔离 target `E:/tmp/openmir2-m2svr-amw-target` / `--offline`）

| 门 | rc | 结果 | 原始件 |
|---|---|---|---|
| `cargo fmt --all -- --check` | 0 | 无输出 | `gate-fmt.txt`（0B） |
| `cargo clippy --workspace --all-targets --offline -- -D warnings` | 0 | `Finished` | `gate-clippy-full.txt` |
| `cargo test --workspace --offline` | 0 | **82 suites / 669 passed / 0 failed / 16 ignored** | `gate-test-full.txt` |
| `cargo run -q -p msgcodegen --offline -- --check` | 0 | `604 条常量 … 与 C# 源一致` | `gate-msgcodegen.txt` |

> 目标目录坑（复现过）：`E:/tmp/openmir2-m2svr-target` 里的 `msgcodegen.exe` 是别处
> （`E:\tmp\openmir2-rev2\t30\…`，已删）编出的陈旧二进制 ⇒ `repo_root must exist` 假红。
> 本片改用**干净且路径匹配**的 `E:/tmp/openmir2-m2svr-amw-target`（其 `msgcodegen` 嵌入的 manifest
> 路径 = 本 worktree）取得上面四门禁。

## 6. 登记（未做，别偷偷扩大写集）

`Get-FileHash` 同写法另见 `tools/botload/load_gate.ps1:342`、`tools/oracle/deploy_gamesvr.ps1:128/172-173`、
`tools/oracle/reconcile_deployment.ps1:99-101`——**属别的车道**，本片只登记不改。
当前 M3②③ 活栈不受影响（`start_rust_stack.ps1` / `rust_seed_plumb.ps1` / `check_seed_plumb.ps1` 无该调用）。

## 7. 残留物清单

- 证据目录：本目录（`tools/m2-acceptance/evidence/m2svr-preflight-modulepath-20261012/**`）。
- `E:/tmp` 临时件：`_m2svr_pre_b8.out.txt`、`_m2svr_selftest_b8.out.txt`、`E:/tmp/m2acc-preflight-fix/**`、
  两处 `preflight-*.json`（`E:/tmp/m2-acceptance/`）——随本 burst 删除，不入库。
- 隔离 target 复用 `E:/tmp/openmir2-m2svr-amw-target`（既有，非本片专属）。
- 未起栈 / 未占端口（仅共享 MySQL 3306；起栈窗口归 m3cs 的 0137 两相）。
