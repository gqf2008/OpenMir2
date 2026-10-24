# 窗口 kit P0 解阻（2026-10-25）

- 卡：`openmir2-window-kit-p0-unblock`（tier **L**，owner `openmir2-tester-1`，
  worktree `.worktrees/tester-1`，branch `card/window-kit-p0-unblock`）。
- 来源：`docs/window-kit-coverage-20261025.md` §2 的 P0 三条。P1 五条不在本卡。
- 红线：未改生产代码/oracle；未实现 C# 时钟钩子；未放宽既有场景判据；未起服务端、未占端口、未连库。

## 1. assets 白名单 `Gold` → `Gold + GamePoint`（P0-①）

### 1.1 改动

- `tools/conformance/card_window.py`：`ASSETS_COLUMNS = ("Gold", "GamePoint")`。
  `GamePoint` 实表口径：`sql/mir2_db.sql:38` `GamePoint int(11) NULL DEFAULT 0`（注释「金刚石」）；
  Rust db-srv 读写见 `mir2-rs/bins/db-srv/src/playdata.rs` 的 `game_point`。
- 用途：GAMEPOINT 真机窗口——`Market_Def/功能NPC/真爱-G002.txt` 的 `[@main] → @yhua → @9hua`
  （`GAMEPOINT - 9`，前置 `CHECKGAMEPOINT > 8`）。
- 通道语义不变：声明值必须是整数、`0..2147483647`；写前按白名单逐列 UPDATE、写后读回断言，
  其余列 MD5 指纹不变；**非白名单列/缺列一律 fail-closed**。

### 1.2 负向牙（白名单外/实列缺列必须拒跑，不是静默 0）

- **牙 1b**：把白名单退回旧值 `("Gold",)` 后，`assets: {"GamePoint": 9}` 必须被
  `ConfigError` 拒绝并点名「白名单」——证明白名单声明本身是判据，不是装饰
  （selftest 用 `globals()` 临时替换，跑完还原）。
- **牙 6e**：声明 `GamePoint` 但 `characters` 实表缺该列（假 runner 的 `SHOW COLUMNS` 不含）
  ⇒ `run_assets` **rc=2「缺列」**，不会带着 0 继续跑。

> 边界（review P-8）：本节的牙验的是「白名单外」与「实列缺列」两类；**场景 JSON 漏写
> `assets.GamePoint` 声明不会在配置期被拒**（runner 没有「必需资产」清单），只会在运行期由
> 脚本 `CHECKGAMEPOINT` 判红（业务失败，不是 fail-closed）。需要配置期拒跑时，应给场景加
> 必需资产声明机制（后续卡）。

实测（`python -I -B tools/conformance/card_window.py --selftest`）：

```text
PASS ASSET1b GamePoint=9 => 解析透传（真爱-G002 @9hua 前置 CHECKGAMEPOINT > 8）
PASS ASSET 牙1b 白名单声明缺失（退回只有 Gold）=> GamePoint 声明被拒（不做静默 0）
PASS ASSET 牙6e 声明 GamePoint 但 characters 缺列 => rc=2（拒绝静默 0）
GREEN: card_window.py selftest 292/292（新增 RNG1b、牙6）
```

## 2. 私栈 `-RngLog` 贯通（P0-②）

### 2.1 数据流

```text
card_window.py --rng-log
   └─ run_side(side) 在起栈前注入 env MIR2_RNG_LOG=<out>/<side>/rng-stream.tsv
        ├─ rust  stack: start-rust.ps1 → start_mir2_server.ps1（读 -RngLog / 环境回落）
        │     └─ 子进程 env: MIR2_RNG_SEED=42 + MIR2_RNG_LOG=<path>
        │           └─ crates/shared/src/rng_trace.rs 写 seq/kind/max/result/site TSV
        └─ csharp stack: start-csharp3.ps1（继承 env；钩子需按 §2.4 部署）
              └─ rng-hook/StartupHook.cs 读 MIR2_RNG_LOG 写同形 TSV
```

### 2.2 代码改动

| 文件 | 改动 |
|---|---|
| `tools/conformance/card_window.py` | 新增 `--rng-log`；`run_side` 在起栈前按侧注入 `MIR2_RNG_LOG`；成功后调用 `check_rng_ledger()`（存在/非空/头两行 `INJECT_OK`+`SEED_PROBE`/每行 5 列且 `site` 非空），不满足 ⇒ 该侧 rc=2 |
| `tools/m2-acceptance/start_mir2_server.ps1` | 新增 `-RngLog <path>`（显式 > `$env:MIR2_RNG_LOG`）；父目录不存在 ⇒ RED rc=2；注入子进程 env；机器行 `RNG_LOG: ...` |
| `tools/window/run_batch.sh` | 新增 `--rng-log`：每个场景给 `card_window.py` 追加 `--rng-log`；dry-run 的计划行显示 `rng_log=1` |

### 2.3 自检（离线；不起栈）

```powershell
# ① 注入面：显式 -RngLog ⇒ 机器行 + 子进程 env（-WhatIf 只打印不启动）
powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/start_mir2_server.ps1 `
  -Root E:/tmp/conformance/rust-root -PortsProfile E:/tmp/conformance/rust-root/alt-port-profile.json `
  -ServerExe <任一现成 mir2-server.exe> -RngLog E:/tmp/scriptcov/rngtest/rust.tsv -WhatIf
# 输出：RNG_LOG: 将向角色注入 MIR2_RNG_LOG=E:/tmp/scriptcov/rngtest/rust.tsv（...）
#       SEED: 将向角色注入 MIR2_RNG_SEED=42（...）

# ② fail-closed：父目录不存在 ⇒ rc=2
# 输出：RED: -RngLog 的父目录不存在：E:/tmp/nonexistent-dir-xyz/rust.tsv（先建证据目录，fail-closed）

# ③ 未给路径 ⇒ 明确不落流水
# 输出：RNG_LOG: (none；未给 -RngLog/MIR2_RNG_LOG，不落流水)

# ④ run_batch 计划：带/不带 --rng-log 的命令差异
bash tools/window/run_batch.sh --dry-run --no-preflight --no-rebuild --rng-log --only login_only
#   PLAN  ... rng_log=1 ...
#   ... card_window.py ... --profile alt --rng-log
bash tools/window/run_batch.sh --dry-run --no-preflight --no-rebuild --only login_only
#   ... card_window.py ... --profile alt     （无 --rng-log）
```

**账本「非空 + site/order 齐」的判据**由 `check_rng_ledger()` 在每次真跑后执行（缺文件/空/
缺 `INJECT_OK`/缺 `SEED_PROBE`/列数不足/site 空 ⇒ 该侧 rc=2）。离线牙（RNG1 + 牙1..5）覆盖
校验器的正反面；**真实账本的非空验证发生在窗口真跑**（本卡不起栈）。

### 2.4 C# 侧副本同步（E:/tmp/conformance，非仓库文件）

- C# 私栈 `E:/tmp/conformance/start-csharp3.ps1` 本身不读 RNG 变量；它用 `Start-Process`
  启动六角色，子进程**继承父进程环境**。因此 `card_window.py` 注入的 `MIR2_RNG_LOG` 会随
  环境传到 C# `GameSrv.exe`。
- 还差**两步**：① 把 `mir2-rs/tests/parity/m3/rng-hook/` 的钩子构建产物部署到
  `E:/tmp/conformance/csharp-root/M2GameSvr/`；② 在 C# 侧启动环境里设
  `DOTNET_STARTUP_HOOKS=<hook dll 路径>`（rng-hook README 有构建命令）。
  `MIR2_RNG_SITE=1` 已由 `card_window.py` 的 `rng_env()` 与 `start_mir2_server.ps1` 的
  `$childEnv` 自动注入（**不设的后果**：C# 钩子 `StartupHook.cs:210` 只在 SITE=1 时写
  5 列，否则写 4 列 ⇒ `check_rng_ledger` 必红）。**本卡不改 `E:/tmp` 副本**，只登记同步
  说明；未部署钩子时 C# 账本为空，`--rng-log` 会让该侧 rc=2（fail-closed，不会静默把缺失当 0）。

## 3. C# 时钟注入缺失的降级口径（P0-③）

- C# 侧 `MIR2_CLOCK_*` 零实现（`grep -rIl MIR2_CLOCK src/` = 0）。
- 已写入 `docs/window-runbook-20261023.md` **§0.7**：W-TAX-4 与 W-T127-2 改为**真实时间点复拍**
  （如 21:50 与 22:00），`SaveTick`/计时器**不逐位对**；边界写清「只证帧内容与可见行为一致」。
- 将来补 C# 时钟钩子的切入点登记为 `rng-hook/StartupHook.cs` 的 StartupHook 模式（后续卡素材，
  本卡不实现）。

## 4. 门禁与自检

- `card_window.py --selftest`：**GREEN 290/290**（含本卡新增 ASSET1b/牙1b/牙6e、RNG1/牙1..5、RNG2/3）。
- `tools/conformance/selftest_impl.py`：rc=0（见 patch body 附输出）。
- `tools/conformance/selftest_mutations.py`：见 patch body 附输出（harness 会临时改写
  `card_window.py` 并逐字节还原，**跑它期间必须停止编辑该文件**）。
- 7 项卫生门禁（`stale_pointer --ref master`）+ `git diff --check <base> <head>` + `merge_tree_helper` 值：
  见 patch body。
- 未起栈、未占端口、未连库、未跑全量测试（内存口径）。

## 5. 残留

- C# rng-hook 部署与 `DOTNET_STARTUP_HOOKS` 设置属窗口准备动作（§2.4），未在本卡执行。
- 本地改动未 push（walgit 故障期）。
