# `MIR2_RNG_ISOLATE_ATTACK` 攻击路径隔离管线（m2-svr 半）——`start_rust_stack.ps1 -IsolateAttack`

卡 `openmir2-m2-rust-isolate-attack-plumb`（worker-11 / worktree `m2-svr` / branch `worktree-m2-isolate-plumb`，
base `origin/master 5221f98a` / head `a0160cf1`）。

## 1. 为什么需要这一片（缺口）

M3②③ 的「**活栈隔离口径**」（owner 2026-10-12 21:15 §6.0-3 签字）要求两侧把**攻击路径的取数**接到
各自一条专用种子流（其余消费者走原全局流）：

- **C# 半**：`mir2-rs/tests/parity/m3/start-csharp-hook.ps1 -IsolateAttack` 给 GameSvr 进程加
  `MIR2_RNG_ISOLATE_ATTACK=1`（+ `MIR2_RNG_ISOLATE_SITES`），影子 hook 按调用点分流；
- **Rust 半**：`crates/shared/src/rng.rs::attack_isolation_seed_from_env` 读**同一个**变量
  （`1/true/on/yes`=开；空/`0/false/off/no`=关；其它非空 ⇒ **panic fail-closed**），
  `crates/world` 在 `World::with_maps`（`world.rs:161-166`）构造时按它把攻击路径取数分流到
  `attack_seed = MIR2_RNG_SEED + 0x9E37` 的专用流。

但**启动器没有这条管线**：`tools/m2-acceptance/start_rust_stack.ps1` 起 `game-svr.exe` 只**继承调用方环境**
⇒ 只有 operator 手工在起栈 shell 里设过 `MIR2_RNG_ISOLATE_ATTACK` 才隔离。否则 **Rust 半走全局流、
C# 半走专用流**，`run_attack_pair.ps1:1074` 的 `ISOLATE_ATTACK` 行门会 RED，或者更糟：把「隔离后的逐 k=0」
伪装成口径错误——正是本仓库已反复收口的 `MIR2_RNG_SEED` / `MIR2_RNG_STATE_LOG` / `MIR2_RNG_LOG` 那条缺陷类
（T-30 同族：继承可用但不可见）。

## 2. 改了什么

| 文件 | 变更 |
| --- | --- |
| `tools/m2-acceptance/rust_seed_plumb.ps1` | 新增 `Resolve-Mir2IsolateAttack`（显式 `-IsolateSwitch` > `$env:MIR2_RNG_ISOLATE_ATTACK`；取值口径与 Rust/C# 逐字一致；**认不出 ⇒ 抛错 fail-closed**）、`Get-IsolateAttackInjectedLine`（`RUST_ISOLATE_ATTACK_INJECTED=1`）、`Assert-Mir2IsolateAttackSeed`（**交叉**守卫：开而无种子 ⇒ `$false`）；`Start-SeededProcess` 增可选 `-IsolateAttack`，与 `-Seed`/`-StateLog` 同一次 `Start-Process` 里注入规范值 `1`、起完还原（未传 ⇒ 逐字节同旧行为） |
| `tools/m2-acceptance/start_rust_stack.ps1` | 新增 `[switch]$IsolateAttack`；解析后打印 `ISOLATE_ATTACK: …`；`Start-One` 增 `-IsolateAttack`，**仅对 `game-svr`** 传 `$isolateInfo` 并打印 `RUST_ISOLATE_ATTACK_INJECTED=1`；开而无种子 ⇒ `RED` + `exit 1`（**拒绝带 panic 起栈**） |
| `tools/m2-acceptance/check_isolate_plumb.ps1` | **新**：17 项可判红/绿自检（见 §3），并带「每轮清 work 目录」的判据诚信牙（见 §5） |

**行为等价**：`-IsolateAttack` 未给且 `$env:MIR2_RNG_ISOLATE_ATTACK` 未设/空/「关」值 ⇒ **不注入、不打
`RUST_ISOLATE_ATTACK_INJECTED` 行**，与改动前逐字节一致（④a/④b 证明）。`-Seed` / `-StateLog` 路径一字未动
（回归 15/15 + 15/15，见 §6）。

## 3. 判据（`checks-green.txt` / `checks-table.txt`，rc=0）

`powershell -File tools/m2-acceptance/check_isolate_plumb.ps1 -GameSvrExe <exe>` ⇒ **17/17 GREEN**。其中：

- ① 解析：未设/空/关值 ⇒ 关；显式开关 ⇒ 开；env `YES` ⇒ 开；非法值 ⇒ 抛错（`Resolve` 五条）；
- ② 注入：开 ⇒ `cmd` 子进程读出 `1`；起完还原调用方环境；关 ⇒ 不注入；
- ③ **真 game-svr**（空闲端口 + 临时 `Server.conf`）：传种子+隔离 ⇒ `MIR2_RNG_LOG` 流水头出现
  `ISOLATE_ATTACK 40545`（= 42 + `0x9E37`，证明环境到达**生产读取点**且解析正确），且同流仍有 `INJECT_OK 42`+`SEED_PROBE`；
- ④ 只传种子、不开隔离 ⇒ 真 game-svr 正常启动、流水**不含** `ISOLATE_ATTACK` 行（零行为差）；
- ⑤ `RUST_ISOLATE_ATTACK_INJECTED=1` 行形态；⑥ 结构接线 + 交叉守卫真值表。

全部**不占 5000/7000/7100/7200、不起完整栈**。

## 4. 改坏必红（四条，rc=1）

| # | 改坏 | 必红项 | 原始件 |
| --- | --- | --- | --- |
| A | 删注入行 `$env:MIR2_RNG_ISOLATE_ATTACK = '1'` | ②a / ③a / ⑥b | `redcheck-A-injection-removed.txt` |
| B | 非法取值静默当关（throw → `Present=$false`） | ①e | `redcheck-B-invalid-value-silent.txt` |
| C | 删 `Assert-Mir2IsolateAttackSeed` 交叉守卫调用 | ⑥c | `redcheck-C-guard-removed.txt` |
| D | 注入值 `'1'` → `'0'`（Rust 视角=关） | ②a / ③a / ⑥b | `redcheck-D-injected-zero.txt` |

**启动器本体 fail-closed**：`start_rust_stack.ps1 -Force -IsolateAttack`（无种子）⇒
`RED: 已给 -IsolateAttack/MIR2_RNG_ISOLATE_ATTACK 但无 MIR2_RNG_SEED/-Seed …`、**rc=1、START 行 0**
（`launcher-failclosed.txt`）——对应 `attack_isolation_seed_from_env` 开而无种子会 panic，
必须在起栈前拦住而不是把 panic 放出来。

## 5. 判据诚信：红检 D 暴露的「陈旧证据假绿」已修

红检 D 首跑时，改坏注入值后 **③a 仍报 OK**：`check_isolate_plumb.ps1` 复用同一 `-OutDir` 时，
`Wait-FileMatch` 在**本轮 game-svr 落盘之前**读到了上一轮 `work/` 里陈旧 `rng-stream.tsv` 的
`ISOLATE_ATTACK 40545` 行（`Wait-FileMatch` 只认「存在且匹配」）。这是**判据假绿**，不是产品缺陷。

修法：脚本开头**每轮先删 `work/` 目录**（`Remove-Item -Recurse -Force`）⇒ 判据只看本轮产物。
D 复跑后 ③a 正确判红。`redcheck-D-injected-zero.txt` 即修补后的原始件。

## 6. 回归 + 门禁

- `check_seed_plumb.ps1` 15/15 GREEN（`regression-seed-plumb.txt`）
- `check_state_log_plumb.ps1` 15/15 GREEN（`regression-state-log-plumb.txt`）
- 四门禁（隔离 `CARGO_TARGET_DIR=E:\tmp\openmir2-m2svr-iso-target`）：`fmt` rc=0 / `clippy -D warnings` rc=0 /
  `test --workspace` rc=0（**84 suites / 686 passed / 0 failed / 16 ignored**）/ `msgcodegen --check` rc=0（604）。

## 7. 窗口 / 边界 / 残留

- 本片**未起完整栈、未占真端口**；③④ 起的单个 game-svr 只绑**随机空闲端口**，用例结束即 `taskkill /T /F`。
  证据采集时的端口实拍：仅共享 MySQL `3306`，5000/5100/5500/5600/5700/6000/7000/7100/7200 全 FREE。
- **写集**：只动 `tools/m2-acceptance/**`（管线条 + 启动器 + 新判据 + 证据）。**零 `.rs`**、
  `src/`、`sql/`、`MirClient` 为空。
- 与 `start-csharp-hook.ps1 -IsolateAttack` 的**对称**：C# 半用的是 `MIR2_RNG_ISOLATE_ATTACK=1`
  （+ 只有 C# 需要的 `MIR2_RNG_ISOLATE_SITES`）；Rust 半只需前者，故本管线不引入 `ISOLATE_SITES`。
- 真机两侧同种子隔离两相仍由 M3②③ 夹具车道在窗口里跑（本片只保证 Rust 半**可用且可判据**）。
