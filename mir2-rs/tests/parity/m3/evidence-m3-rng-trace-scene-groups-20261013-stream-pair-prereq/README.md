# 证据包：M3 RNG「制品级同种子」门——半给流水 fail-closed 收口（2026-10-13）

- 卡：`openmir2-m3-rng-trace-scene-groups`（issue `d9d03f47`）
- 车道 `m3bag` / principal `openmir2-worker-4` / 分支 `worktree-m3-rng-trace-scene-groups`
- 报告：`../M3RNG场景制品级同种子门半给流水fail-closed收口报告-20261013.md`
- 夹具：`../rng-trace-scene/`（scenario + 前置门 + 驱动 + README）
- base = merge-base(`origin/master`, HEAD) = `9c776129c30280ddc692d88d7507192dd94f20a0`（交付前已 `git merge origin/master`；在途片 `a992a09a9` 已合入 master `7dcbd0ada`）

## 这一片补的是什么

上一片（已合入 `7dcbd0ada` 的 `a992a09a9`，DoD①「制品级同种子」前置门）把门的种子证据从**场景
描述件自报**升级为 **trace 流水头的 `INJECT_OK <seed>`**。作者侧再自审，发现两处（均为非阻塞 Minor）：

1. **fail-closed 缺口（F-2，本片修）**：`rng_scene_gate.py gate` 的制品级种子分支条件是
   `if a_stream or b_stream:` —— **只给一侧**流水（`--a-stream` / `--b-stream` 单独）时只核已给侧、
   不核 `a-vs-b`、也不判 PREREQ ⇒ 返回 **GREEN**（半份证据被当绿）。修法：
   `bool(a_stream) != bool(b_stream)` ⇒ `fail_prereq("stream-pair-required", …)`（`exit 2`）。
2. **文档计数漂移（F-1，本片修）**：`rng-trace-scene/README.md` §6 命令注释上一片漏改，仍写
   `16/16`；§5 与实测均为 21/21（本片再 +1 = 22/22）。

**两侧都不给**流水仍走既有描述件级向后兼容口径；DoD① 入口 `-SceneGroupDiff` 本就恒要求两侧
（缺任一侧 `PREREQ_MISSING exit 2`）⇒ 生产路径行为不变，本牙只咬住直接 CLI 的半给用法。

## 件清单

| 文件 | 内容 |
|---|---|
| `00-commands.txt` | 全部可复算命令 |
| `01-gate-selftest.txt` | `rng_scene_gate.py selftest` ⇒ **22/22 GREEN**（原 21 + 新 1 `stream-pair-prereq-red`） |
| `02-scenario-green.txt` | `rng_scene_gate.py scenario` ⇒ **VERDICT GREEN**（未改语义） |
| `03-half-stream-prereq-a-only.txt` | **新牙**：只给 `--a-stream` ⇒ **rc=2 PREREQ**，点名 `stream-pair-required` |
| `04-half-stream-prereq-b-only.txt` | **新牙**：只给 `--b-stream` ⇒ **rc=2 PREREQ**，点名 `stream-pair-required` |
| `05-both-streams-green.txt` | 回归：两侧齐备 ⇒ **rc=0 GREEN** |
| `06-neither-backward-compatible.txt` | 回归：两侧都不给 ⇒ **rc=0 GREEN**，输出无任何 `stream-*` 检查 |
| `07-nostack-selfcheck.txt` | `run_rng_trace_pair.ps1 -SelfCheckNoStack` ⇒ **27/27 GREEN**（嵌入的门自检 22/22） |
| `08-regression.txt` | 回归：`rng_ledger_diff` 32/32 · `attack_rng_ledger` 32/32 |
| `09-mutation-remove-guard-red.txt` | **改坏必红**：删掉 `stream-pair-required` 守卫 ⇒ 门自检 **21/22**，`FAIL stream-pair-prereq-red` rc=1 |
| `10-readme-count-drift.txt` | README §6 `16/16` → `22/22` 的 before/after |
| `11-boundary-yuan-rs.txt` | 边界：冻结面（`src/ sql/ MirClient/ mir2-rs/crates/ mir2-rs/bins/`）零改动 + 零 `.rs` + 未动 `tests/parity/world` |
| `12-gates-merge-tree.txt` | 合并后交付树四门禁：fmt rc=0 · clippy rc=0 · test 99 suites 1159 passed 0 failed 22 ignored · msgcodegen 604 rc=0 |
| `13-eolcheck-scope.txt` | 全树 `eolcheck` RED 逐条归因（全在本包之外）；本包定向 `eolcheck` GREEN |

改坏在**仓外**临时树 `E:/tmp/m3bag-mut3/parity-m3/`（由仓库件复制）上做，仓库内文件未被污染。
**交付前已 `git merge origin/master`**（base = merge-base = `9c776129c30280ddc692d88d7507192dd94f20a0`），故按 merge-base 的 diff 只含本片。

## 复算

```sh
# cwd = 仓库根
python -I -B tools/evidence-hash/verify_sha256sums.py verify \
    mir2-rs/tests/parity/m3/evidence-m3-rng-trace-scene-groups-20261013-stream-pair-prereq/SHA256SUMS.txt --check-extra
python -I -B tools/evidence-hash/verify_sha256sums.py eolcheck mir2-rs/tests/parity/m3
# cwd = mir2-rs/tests/parity/m3
python -I -B rng-trace-scene/rng_scene_gate.py selftest          # 22/22
powershell -NoProfile -ExecutionPolicy Bypass -File rng-trace-scene/run_rng_trace_pair.ps1 -SelfCheckNoStack   # 27/27
```

## 残留物清单

- 仓外临时件：`E:/tmp/m3bag-trace-seed-evidence/`（合成场景件/流水）、
  `E:/tmp/m3bag-mut3/`（改坏临时树）—— 保留以备复算。
- worktree 内 8 个他线历史未跟踪目录 `sessions/csharp-20261012-23*`（非本片、未纳入、未清）。
- 本片未起栈、未占端口、未 merge、未自签 review、未动共享主检出。
