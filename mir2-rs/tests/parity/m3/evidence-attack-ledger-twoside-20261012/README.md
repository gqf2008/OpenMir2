# M3②③ 残差①：同种子抽数账本**两侧化**（`--side csharp|rust`）· 证据

- 卡：`openmir2-m3-attack-pair-fixture`（issue `76cc08b8`）／车道 `m3bag`／principal `openmir2-worker-4`
- worktree：`.claude/worktrees/m3bag`／branch：`worktree-m3-bag-equip`
- base：`refs/heads/master` = `40166a82`（owner 17:35 片已 FF 合入）
- 任务来源：brief 顶部 **owner 2026-10-12 17:35** 第 2 条 —— 「把**两侧**同种子的抽数账本做成机判（D2 口径）」。
  上一片（`40166a82`）只落了 **C# 半**（`--marker` 只有一个 C# 站点），Rust 半缺站点表/缺省锚；
  m3cs 决策片的 `last-m3cs.md` 已点名「两侧账本直接 diff 仍差一处 **m3bag 写集小改**：`--side` 给缺省 marker + 站点正则」，
  owner 18:25 亦写明该配合「转给 m3bag」。本片即补这一格。

## 0. 本片交了什么（一句话）

`attack_rng_ledger.py` 增加 **`--side csharp|rust`**（缺省 marker + 攻击路径正则）、**`--attack-re`**（显式覆盖）、
站点分隔符归一（`crates\world\...` ↔ `crates/world/...`），并在 `selftest` 里补齐 **Rust 侧**用例与牙齿。
**不给 `--side` 时逐字节保持旧行为**（缺省命令形态与已入库旧账本 `cmp` 相同）。

Rust 站点表与 m3cs 决策片 `rng-trace-probe/analyze_rng_trace.py --attack-re` **同表**：
`combat\.rs:(67|109|111|579|587|606|608|736)|magic\.rs:`；缺省锚 = `crates\world\src\combat.rs:109`。

## 1. 读数（原始件 → 文件）

| 文件 | 内容 |
|---|---|
| `01-rust-summary-side.txt` | `summary --side rust`：`total=14946 attack_path=14946 noise=0`（与 m3cs 决策片 `08-rust-trace-single-threaded-interleave.txt` 的 `attack_path=14946/噪声 0` 一致） |
| `02-rust-segments-side.txt` | `segment --side rust`（缺省锚）：**segments=2996**（= m3cs 的 `attacks=2996`） |
| `03-csharp-default-regression.txt` | **缺省路径回归**：C# `summary` = `total=1670754 attack=275 noise=1670479`；`segment`（显式 marker 旧命令）与 `--side csharp` 逐字节相同，且与已入库 `evidence-attack-my-field-20261012/csharp-170110.rng-segments.jsonl` **`cmp` 相同** |
| `04-selftest.txt` | `attack_rng_ledger selftest` ⇒ **GREEN 18/18**（旧 8 条 + Rust 侧 10 条） |
| `05-mutation-rust-missing-draw.txt` | **改坏必红**：14 行 Rust 夹具删一次取数 ⇒ `diff` RED「k=2 … b 少抽 1」 |
| `06-crosstest-ledger-diff.txt` | 两侧账本 `diff` 现在**同一工具可跑**；当前态 C#=120 击 vs Rust 离线探针=2996 击 ⇒ RED（段数不等）——不是同会话，预期如此 |
| `07..10-gates-*.txt` | 四门禁：fmt 0 / clippy 0 / **81 suites 668 passed 0 failed** / msgcodegen 604 |
| `11-selfcheck-nostack.txt` | `run_attack_pair.ps1 -SelfCheckNoStack`（pwsh 7）rc=0，全绿 |

## 2. 判据与牙齿（都实跑）

1. **两侧同工具**：Rust 侧站点由 `--side rust` 分类，`summary`/`segment` 产出与 m3cs 分析器同读数（§1 `01`/`02`）。
2. **缺省不回归**：`--side` 缺省时 `summary`/`segment` 与旧版逐字节相同（§1 `03`，两条 `cmp`）。
3. **改坏必红**：Rust 段删一次取数 ⇒ RED 且定位到 k 与「少抽 1」（§1 `05`）；selftest 内亦有
   多抽/少抽/段数不等/未知 side fail-closed 四类牙齿（`04`）。
4. **fail-closed**：未知 `--side` ⇒ rc=2（不静默降级成「不过滤」）；`segment` 未给 marker 且无 side 缺省锚 ⇒ rc=2。
5. **干净检出复验**（F-1 口径）：`git clone --local` 到 `E:/tmp` 后在 clone 里跑 `verify_sha256sums.py --check-extra` = GREEN 15/15。
   首版 `11-selfcheck-nostack.txt` 由 pwsh 落盘时混入了 2 个 **lone CR**（Git 只归一 CRLF、不动 lone CR）⇒ 干净 clone 上假 mismatch；
   本片把全部证据文本 LF 归一后重记 `SHA256SUMS.txt`（不再有 lone CR）。

## 3. 边界 / 未做（如实登记）

- **零 `.rs`**：只动 `mir2-rs/tests/parity/m3/attack_rng_ledger.py` + 本证据目录；未动 `src/**`、未动他人文件。
- **未起栈、未占端口**：本片纯离线（十端口实拍全 DOWN，仅共享 3306）。
- **真实两侧同会话逐 k diff 仍未做**：需要 Rust **活栈**逐次 trace（离线探针 trace 不是同会话，段数天然不等）。
  该活栈 trace 由 m3cs 的下一片在窗口里产出（owner 18:25 已派）；本片把账本仪器两侧化，届时直接
  `segment --side csharp` / `segment --side rust` + `diff` 即可。
- 证据输入为仓外原始件：`E:/tmp/m3cs-trace-single.tsv`（m3cs 离线单线程探针）、
  `E:/tmp/m3cs-tick/csharp-stack/rng-stream.tsv`（17:01 C# 活栈）。

## 4. 残留物清单

- 仓外临时物（未入库、可删）：`E:/tmp/m3bag-ledger2/**`（本片分段 jsonl）；
  `E:/tmp/m3bag-myfield-target/**`（复用上一片的隔离 cargo target）。
- 本目录：`rust-trace-shape-14.tsv`（m3cs `09-rust-trace-shape.txt` 的前 14 行提取，去注释、LF）+
  `rust-trace-shape-14-mut.tsv`（删 1 行）——两件都随证据入库，供 §1 `05` 从仓库内复现。
