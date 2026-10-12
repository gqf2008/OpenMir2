# 证据：M3②③ 活栈隔离口径 —— C# 影子 hook 分流 + 夹具开关 + Get-FileHash 同族缺陷

- 卡：`openmir2-m3-combat-skills-align`（②③，issue `a2ef74aa`）／车道 `m3bag`／principal `openmir2-worker-4`
- 来源：owner 2026-10-12 21:15 裁决（§6.0-3 owner 签字）——DoD② 改为**活栈隔离口径**；本片 = **C# 半边 + 夹具开关**。
- 接口（同卡 m3cs 的 Rust 半，head `b9e37389`，此刻待审）：
  `tests/parity/m3/rng-isolate-attack/{attack_sites.tsv,check_predicates.py,csharp_predicate_dump.example.txt,README.md}`。
- 写集：`mir2-rs/tests/parity/m3/**`（**零 `.rs`**、未动 `src/**`/`sql/**`/`MirClient`；**未起栈、未占端口**）。
- 本片 **base = `b9e37389`**（含 m3cs 的 Rust 半，同卡待审；见 §5）。

## 1. 本片做了什么（交付物）

1. `rng-hook/StartupHook.cs`——影子 hook 按调用点分流：
   - `MIR2_RNG_ISOLATE_ATTACK`：空/`0`/`false`/`off`/`no`（大小写不敏感）⇒ **关**；`1`/`true`/`on`/`yes` ⇒ **开**；其它非空 ⇒ **fail-closed**。
   - 开启时命中 `divert-sites.txt` 的调用点取数委托给独立 `Random(attack_seed)`（`attack_seed = MIR2_RNG_SEED + 0x9E37`，i32 回绕）；其余消费者仍走原全局流。
   - **关闭时逐字节不变**（专用流不构造、不被取数；`_needSite=false` 时不取调用点）。
   - 流水头补 `ISOLATE_ATTACK <attack_seed> isolate_sites=<path>`（与 Rust `rng_trace.rs` 同形；既有解析器按前缀/无 tab 跳过，不污染 site 列表）。
   - 静态清单原样落盘 `MIR2_RNG_ISOLATE_SITES`；可选 `MIR2_RNG_ATTACK_LOG` 记专用流取数。
2. `rng-hook/divert-sites.txt`（新，**嵌入 hook**）——C# 侧 11 个 divert 站点，与 Rust 谓词表逐条一致；**路由与 dump 同源**（唯一事实源）。
3. `rng-hook/selftest/`（新）——无 oracle 依赖的离线自检（开关解析 / attack_seed 公式 / 清单 / 路由），编译同一份 `StartupHook.cs` + `divert-sites.txt`。
4. `run_attack_pair.ps1`——新增 `-IsolateAttack/-IsolateSites/-AttackStreamLog/-RustRngLog`：
   - 0.7 节：流水头必须有 `ISOLATE_ATTACK <seed>` 且 `seed == Seed + 0x9E37`（否则 RED）；C# 侧机判 `divert-sites.txt` vs `attack_sites.tsv`（`check_predicates.py static + table-vs-csharp`，缺一条/未登记 ⇒ RED）。
   - 会话产物 `attack_isolation.json`：隔离开关 + attack_seed + dump sha256 + 专用流 sha256 + 谓词表 sha256。
   - 跨侧护栏 `Get-M3PeerIsolationGuard`：两侧 `isolate_attack` 必须一致（开启时 `attack_seed` 一致），否则 PREREQ(exit 2)。
5. `start-csharp-hook.ps1`（新，仓库内规范化）——透传隔离 env（**默认关**）、构建/部署 hook、写 `csharp_isolation.json`。
6. **Get-FileHash 同族缺陷**：`gate_guards.ps1`（原 :555/:694）与 `rng-baseline.ps1`（原 :45）改 .NET `SHA256`（新增 `Get-M3Sha256Hex` / `Get-Sha256`）。本机 PS 5.1 缺 `Get-FileHash`（PSModulePath 被 PS7 污染），原路径走到 `Test-M3MonGenEmpty -VerifySource` 会 `CommandNotFoundException`（m3cs 首跑已复现）。

## 2. 判据（本片，全部离线；不占端口）

| # | 判据 | 结果 | 证据 |
| --- | --- | --- | --- |
| ① | `run_attack_pair.ps1 -SelfCheckNoStack`（**powershell 5.1**） | **rc=0 GREEN**（含新 `SELFTEST 21`） | `04-selfcheck-nostack.txt` |
| ② | C# hook 离线自检 `RngIsolateSelftest` | **37 PASS / SELFTEST GREEN** | `02-hook-selftest.txt` |
| ③ | C# 分流站点清单 vs Rust 谓词表（`check_predicates.py static` + `table-vs-csharp`） | **GREEN 11/11** | `03-predicates-table-vs-csharp.txt` |
| ④ | Get-FileHash 改前崩溃 / 改后两个 `-VerifySource` 调用点 GREEN | **BEFORE_CRASH → ok:true/source_verified:true** | `01-gethash-ps51-before-after.txt` |
| ⑤ | 改坏必红（清单删一条 / 加未登记 / hook 自检） | **删一条 rc=1（缺 `hit_gate`）/ 加未登记 rc=1（UNKNOWN）/ hook FAIL×3** | `03-*.txt`、`05-mutation-teeth.txt` |
| ⑥ | 四门禁（隔离 target，含同卡 m3cs Rust 半） | **fmt 0 / clippy 0 / 84 suites·682 passed·0 failed·16 ignored / msgcodegen 604** | `06-gates.txt` |

## 3. 改坏必红（真跑 + 还原）

- 由 `05-mutation-teeth.txt`：删掉 `divert-sites.txt` 的 `hit_gate` 行 ⇒
  `hook selftest rc=1`（`FAIL sites/count=11 (got 10)`、`FAIL route/hit_gate`）且 `table-vs-csharp rc=1`（缺 `hit_gate`）；
  `cmp` 还原逐字节相同、两判据回绿。
- 由 `03-*.txt`：加一条未登记 site ⇒ `check_predicates.py` 报 `UNKNOWN ... fail-closed`（rc=1）。
- 由 `01-*.txt`：把 .NET SHA256 换回 `Get-FileHash` 的调用形态（本机 PS 5.1）= `CommandNotFoundException`（改前崩溃）。

## 4. 残留 / 未核（登记，不静默）

1. **真机 on-path 运行未在本片执行**：C# 影子栈隔离开 → 30/120 击两相 → `attack_diff.py diff` 逐 k=0 需要
   5000/7000/7100/7200 窗口（owner 派 m3cs 取窗跑 ②③ 收口）。本片只证仪器链路（hook 可编译 + 离线解析/路由/清单 + 夹具透传/记账）。
2. **[已修 · 2026-10-12 第十轮片 `evidence-hashutil-hotfile-ledger-20261012`]** `attack_rng_ledger.py` 的
   `RUST_ATTACK_RE` 当时仍是旧行号（m3cs 在其 patch finding 里已登记）；本片当时未改该正则（避免与 m3cs 的
   Rust 行号表二次漂移）。现表已与 `rng_ledger_diff.py` **逐字节同表** + `selftest` **27/27**（改回旧表必红）；
   本片权威两侧比对仍用 `check_predicates.py align`，结论不变。
   - 同类 **[已修 · 同片]**：本片 §3 用 .NET SHA256 取代 `Get-FileHash`，但旧实现仍用 `File.ReadAllBytes`
     （`FileShare.Read`）⇒ 读不了 GameSvr 以 `FileShare.ReadWrite` 持有的 `rng-attack-stream.tsv`（m3cs tick22 F1）；
     现统一到 `m3hashutil.ps1`（`FileShare.ReadWrite|Delete`）。
3. `atk_power_luck_a/b`、`struck_durability_slot` 在现有夹具（`Luck=0`、无装备）**未触发**；行号按谓词表源码行登记，
   待隔离口径真机实测校准（谓词表内已注明）。
4. `attack_paralysis`（`PlayObject.Attack.cs:221`）`divert=0`（Rust 未建模）：hook 清单故意不含它；
   若误分流，`table-vs-csharp` 会报未登记 site fail-closed（§3 已覆盖 UNKNOWN 红检）。
5. `start-csharp-hook.ps1` 是**仓库内规范化**（历史为各 lane 仓外自备）；其真机起栈路径未在本片执行（同 §1）。

## 5. base 依赖说明

本片要求 `rng-isolate-attack/attack_sites.tsv` + `check_predicates.py` 在树内（机判的 Rust 谓词表）。
该接口来自**同卡** m3cs 的 Rust 半 `b9e37389`（此刻 **待审、未入 master**）⇒ 本片 patch 的 base 取 `b9e37389`。
若 owner 先合 m3cs 的 Rust 半再合本片，则 `git diff master..worktree-m3-bag-equip` 恰为本片改动。

## 6. 命令与原始输出

| 文件 | 内容 |
| --- | --- |
| `01-gethash-ps51-before-after.txt` | PS 5.1 缺 Get-FileHash；改前 `CommandNotFoundException`；改后 `Get-M3Sha256Hex` + 两调用点 `-VerifySource` GREEN；零调用扫描 |
| `02-hook-selftest.txt` | `RngIsolateSelftest` 37 PASS；`RngSeedHook.csproj` 编译 0 错 |
| `03-predicates-table-vs-csharp.txt` | `static`/`table-vs-csharp` 生产 GREEN；删一条/加未登记 RED；`check_predicates.py selftest` rc=0 |
| `04-selfcheck-nostack.txt` | `run_attack_pair.ps1 -SelfCheckNoStack`（PS 5.1）全量输出，`SELFTEST_RED GREEN` rc=0 |
| `05-mutation-teeth.txt` | 清单删一条 ⇒ hook 自检 + 谓词机判双红；还原逐字节相同 |
| `06-gates.txt` | 四门禁（fmt/clippy/test/msgcodegen）rc 与计数 |

## 7. 残留物清单

- 仓外临时物（可删）：`E:/tmp/m3bag-iso/**`（本片离线命令原始输出、谓词变异件、`divert-sites.txt.bak`）、
  `E:/tmp/openmir2-m3bag-target/**`（隔离 cargo target）。
- 未起栈／未占端口：本片全程离线，`5000/5100/5500/5600/5700/6000/7000/7100/7200` 未动用（仅共享 MySQL 3306 他线在用）。
- 未自签 review、未 merge、未写他人 inbox。
