# 窗口 C# 栈 RNG 接线（2026-10-25）

- 卡：`openmir2-window-csharp-rng`（tier **L**，P0；owner `openmir2-tester-1`，
  worktree `.worktrees/tester-1`，branch `card/window-csharp-rng`）。
- 来源：CLEARMAPMON 第三轮 review `3c3591d8` 的 F1。
- 红线：只改启动/注入面（工具），未改生产代码与 oracle 行为；未 push。

## 1. 缺口（review F1 的事实）

- 窗口 C# 栈不是 `start_mir2_server.ps1`，而是 `card_window.py` 的 alt profile 里
  `sides.csharp.stack` 指向的 **`E:/tmp/conformance/start-csharp3.ps1`**。
- 该脚本没有 `-Seed` / `MIR2_RNG_SEED` / `DOTNET_STARTUP_HOOKS` ⇒ oracle 按**时间种子**，
  RNG 流位不可能与 Rust 对齐；P0 卡往 `start_mir2_server.ps1` 的 `$childEnv` 注入
  `MIR2_RNG_LOG`/`MIR2_RNG_SITE` 对窗口实际 C# 栈**不生效**。
- `E:/tmp/conformance` 会被外部清扫（已发生三次）⇒ 启动脚本必须收进仓库。

## 2. 改动

| 文件 | 改动 |
|---|---|
| `tools/window/start-csharp.ps1`（新） | 仓库内 C# 私有栈启动器（csharp-root / 15xxx）。默认行为与旧 `start-csharp3.ps1` 相同；仅当给 **`-RngLog`（或 env `MIR2_RNG_LOG`）** 时：影子复制 `<Root>/M2GameSvr → <Root>/M2GameSvr-hook`、打开 runtimeconfig 的 `System.StartupHookProvider.IsSupported`、`dotnet build` `mir2-rs/tests/parity/m3/rng-hook/RngSeedHook.csproj` 并部署 dll，只给 GameSvr 注入 `DOTNET_STARTUP_HOOKS` + `MIR2_RNG_SEED` + `MIR2_RNG_LOG` + `MIR2_RNG_SITE=1`（其余角色不注入）。支持 `-Plan`/`-Stop`；父目录缺失 fail-closed rc=2 |
| `tools/conformance/card_window.py` | alt profile 的 `csharp.stack` 改指**仓库内** `tools/window/start-csharp.ps1`（仍走 `start-stack-wmi.ps1` WMI 包装，避免管道死锁）；`--rng-log` 时 `run_side` 已注入 `MIR2_RNG_LOG`+`MIR2_RNG_SITE=1`，env 透传到该启动器 |
| `docs/window-kit-coverage-20261025.md` | §1.6 W-DG-1 与 §2 P0 行同步：RNG 流水已接（Rust `-RngLog` + C# hook） |

> **直跑注意**：不要从交互 shell 直接 `powershell -File start-csharp.ps1` 后等它返回——子进程
> 继承管道会让调用方等 EOF（本项目已知坑）。窗口路径走 `start-stack-wmi.ps1`（profile 已如此）；
> 手工排查用 `Start-Process` 或用 `-Plan`。

## 3. 实测证据（2026-10-25，本机私有栈）

### 3.1 C# 侧（`tools/window/start-csharp.ps1 -Seed 42 -RngLog …`）

```text
CSHARP_ROOT E:\tmp\conformance\csharp-root
SEED: 将向 GameSvr 注入 MIR2_RNG_SEED=42
RNG_LOG: 将向 GameSvr 注入 MIR2_RNG_LOG=E:/tmp/scriptcov/csharprng/ledger.tsv + MIR2_RNG_SITE=1 + DOTNET_STARTUP_HOOKS=…\RngSeedHook.dll
START GameSvr pid=… ports=15000 exe=…\M2GameSvr-hook\GameSrv.exe
CSHARP_STACK_GREEN
```

账本头（`E:/tmp/scriptcov/csharprng/csharp-head4.txt`）：

```text
1	INJECT_OK	42	0
SEED_PROBE 1434747710 302596119 269548474 1122627734 361709742 563913476 1555655117 1101493307
1	Next(max)	1500	1002	M2Server.Actor.BaseObject..ctor@BaseObject.cs:438
2	Next(max)	2000	281	M2Server.Actor.BaseObject..ctor@BaseObject.cs:441
```

- 行数 **3,469,374**；**每行 5 列且 site 非空**（3,469,372 行含 site）。
- sha256 `40fc9b52cf28f8709aa1f0f69ae66acce1ac14d43e6c7a61041dca66fc9f7859`。

### 3.2 Rust 侧（`start_mir2_server.ps1 -Seed 42 -RngLog …`，同 seed 42）

```text
1	INJECT_OK	42	0
SEED_PROBE 1434747710 302596119 269548474 1122627734 361709742 563913476 1555655117 1101493307
1	Next(max)	1500	1002	crates\world\src\entity.rs:174
2	Next(max)	2000	281	crates\world\src\entity.rs:175
```

- 行数 **3,585,270**；5 列且 site 非空（3,585,268 行含 site）。
- sha256 `2d2da24e92b27ff34d7fc0f39076e988713bab41c9625600c26f5dc5e6e02be6`。

### 3.3 两侧对照结论

- 两侧 `INJECT_OK 42` 与 **`SEED_PROBE` 八项向量逐项相同**；前两抽输出同为 `1002` / `281`
  ⇒ **种子贯通成立**。
- `site` 列按各自代码路径（C# `M2Server.*.cs` / Rust `crates\world\src\*.rs`）分别记录，
  证明账本可做 site/order 对照（帧完全一致不在本卡判据内）。
- 大账本（约 280MB + 187MB）已在取证后删除；仓库外保留 `E:/tmp/scriptcov/csharprng/`
  的 `summary.txt`、两个 `*-head4.txt` 与 `hashes.txt`。

## 4. 复算命令

```powershell
# C#：私有栈 + hook + 同种子（-Plan 可先看命令/env，不启动）
powershell -NoProfile -ExecutionPolicy Bypass -File tools/window/start-csharp.ps1 `
  -Plan -Seed 42 -RngLog E:/tmp/scriptcov/csharprng/ledger.tsv
powershell -NoProfile -ExecutionPolicy Bypass -File tools/window/start-csharp.ps1 `
  -Seed 42 -RngLog E:/tmp/scriptcov/csharprng/ledger.tsv
powershell -NoProfile -ExecutionPolicy Bypass -File tools/window/start-csharp.ps1 -Stop

# Rust：同 seed 私有栈（用本检出或指定 -ServerExe 的 mir2-server）
powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/start_mir2_server.ps1 `
  -Root E:/tmp/conformance/rust-root -PortsProfile E:/tmp/conformance/rust-root/alt-port-profile.json `
  -ServerExe <mir2-server.exe> -Seed 42 -RngLog E:/tmp/scriptcov/csharprng/rust-ledger.tsv

# 账本校验（与 card_window 的 check_rng_ledger 同口径）
head -2 <ledger>   # INJECT_OK <seed> + SEED_PROBE
awk -F'\t' 'NF>=5 && $5!="" {n++} END{print n}' <ledger>
```

## 5. 门禁

代码/工具改动：`card_window --selftest` 292/292；7 项卫生门禁（`stale_pointer --ref master`）+
`git diff --check <base> <head>` + `merge_tree_helper` 值见 patch body。未起正式服务端（3306 默认栈
未动，仅起 15xxx 私有栈并已停）；未 push。

## 6. 残留

- `E:/tmp/conformance/csharp-root/M2GameSvr-hook/`：影子目录（hook dll + runtimeconfig 开
  StartupHookProvider），由启动器按需创建；`E:/tmp` 不是事实源，仓库脚本可重建。
- 账本增长快（空闲 NPC RNG 也记）：窗口只对需要 RNG 的一侧开 `--rng-log`，跑完及时停栈并归档
  head+hash；大文件不要留在磁盘。
- C# 时钟注入仍未实现（本卡只接 RNG 种子）；`SaveTick` 等时序降级口径见 runbook §0.7。
- **reviewer-21 独立复跑快照**（2026-10-25 05:56:42，`start-csharp.ps1 -Seed 42 -RngLog`）：
  `lines=2237185 rows_with_site=2237183`、sha256 `1b8947a9d79afd1f35dac1244fb047891444dfa11c2ec25d39c80828ff32374a`；
  头 4 行同 `INJECT_OK 42` + `SEED_PROBE` + `1002/281`，第二抽 site 实测为 `BaseObject.cs:441`。
  原始件：`E:/tmp/scriptcov/reviewer21-rng/reviewer21-evidence.txt`。本仓自己的大账本已在取证后删除，
  两份证据均只保留 head+hash+行数（磁盘纪律：账本增长速度以百万行/十分钟计）。
