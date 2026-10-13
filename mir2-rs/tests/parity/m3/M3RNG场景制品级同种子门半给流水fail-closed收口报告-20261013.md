# M3 RNG 场景「制品级同种子」门：半给流水 fail-closed 收口报告（2026-10-13）

- 卡：`openmir2-m3-rng-trace-scene-groups`（issue `d9d03f47`）
- 车道 `m3bag` / principal `openmir2-worker-4` / 分支 `worktree-m3-rng-trace-scene-groups`
- 写集：`mir2-rs/tests/parity/m3/**`（零 `.rs`；未动 `crates/world`、`bins/**`、`src/**`、`tests/parity/world/**`）
- 证据包：`evidence-m3-rng-trace-scene-groups-20261013-stream-pair-prereq/**`
- base = `origin/master`（交付前已 `git merge origin/master`；具体 oid 见证据 `11-boundary-yuan-rs.txt`）；在途片 `a992a09a9` 已由 reviewer-5 `9dce8b2e` approve、合并为 `7dcbd0ada`

## 1. 这一片补的是什么（对本卡已合片自审发现的两条 Minor）

上一片（`a992a09a9`，DoD①「制品级同种子」前置门，patch `69a2c186`）已把门的种子证据从
**场景描述件自报** 升级为 **trace 流水头的 `INJECT_OK <seed>`**，并已合入 master `7dcbd0ada`。
作者侧对该片再自审，发现两处：

1. **fail-closed 缺口（F-2，本片修）**：`rng_scene_gate.py gate` 的制品级种子分支条件是
   `if a_stream or b_stream:` —— **只给一侧**流水（`--a-stream` 或 `--b-stream` 单独）时，它只核
   已给侧、**不核 `a-vs-b`**，且不判 PREREQ ⇒ 返回 **GREEN**。这与 DoD①「fail-closed」不符：
   半份证据不能证明**两侧**同种子。
   - 修法：`bool(a_stream) != bool(b_stream)` ⇒ `fail_prereq("stream-pair-required", …)`（`exit 2`）。
   - **两侧都不给**仍走既有描述件级向后兼容口径（历史命令/已入库证据可复算）；DoD① 入口
     `-SceneGroupDiff` 本就恒要求两侧（缺任一侧 `PREREQ_MISSING exit 2`）⇒ 生产路径行为不变。
2. **文档计数漂移（F-1，本片修）**：`rng-trace-scene/README.md` §5 已写「场景门 selftest（21/21）」，
   但 §6 命令注释仍写 `# 16/16 GREEN`（上一片把 gate selftest 从 16 加到 21 时漏改）⇒ 内部不一致；
   本片 gate selftest 再 +1（`stream-pair-prereq-red`）= 22/22，§5/§6 一并订正。

两件同源于本卡已合片的作者自审，一片交付。**无任何已生效 approve 被移动**：`a992a09a9` 的
approve 覆盖的是它自己的 head，且它已合并；本片 head 叠在**已含该片的 master**（`origin/master`）之上，
是该卡的一个**新** head，须由非作者 reviewer 重新审。

## 2. 判据（离线，零端口）

| # | 判据 | 读数 | 证据 |
|---|---|---|---|
| 1 | 门自检 | `SELFTEST ok=22/22` rc=0（原 21 + 新 1） | `01-gate-selftest.txt` |
| 2 | scenario 自洽（回归） | `VERDICT GREEN` | `02-scenario-green.txt` |
| 3 | **只给 `--a-stream`** ⇒ PREREQ | rc=2，点名 `stream-pair-required` + `PREREQ_MISSING` | `03-half-stream-prereq-a-only.txt` |
| 4 | **只给 `--b-stream`** ⇒ PREREQ | rc=2，点名 `stream-pair-required` + `PREREQ_MISSING` | `04-half-stream-prereq-b-only.txt` |
| 5 | 两侧齐备（回归） | rc=0 `VERDICT GREEN` | `05-both-streams-green.txt` |
| 6 | 两侧都不给（向后兼容） | rc=0 `VERDICT GREEN`，输出无 `stream-*` | `06-neither-backward-compatible.txt` |
| 7 | 无栈自检 | `ok=27/27 failures=` rc=0 | `07-nostack-selfcheck.txt` |
| 8 | 回归：账本/攻击账本 | `32/32` · `32/32` | `08-regression.txt` |
| 9 | **改坏必红** | 仓外副本删掉 `stream-pair-required` 守卫 ⇒ 门自检 **21/22**，`FAIL stream-pair-prereq-red` rc=1 | `09-mutation-remove-guard-red.txt` |
| 10 | README 计数订正 | §6 `16/16` → `22/22`（与 §5、实测一致） | `10-readme-count-drift.txt` |
| 11 | 边界 | 冻结面 / `*.rs` / `tests/parity/world` 全空；改动限 `rng-trace-scene/` + 本报告 + 本证据包 | `11-boundary-yuan-rs.txt` |
| 12 | 交付树四门禁 | fmt rc=0 · clippy rc=0 · test **98 suites 1155/0/21** rc=0 · msgcodegen 604 rc=0 | `12-gates-merge-tree.txt` |
| 13 | EOL 口径（同族坑第 5 次） | 证据包定向 `eolcheck` **GREEN**；全树 RED 27/3613 **全部在本包之外**（见归因） | `13-eolcheck-scope.txt` |

改坏在**仓外**临时树 `E:/tmp/m3bag-mut3/parity-m3/`（由仓库件复制）上做，仓库内文件未被污染。

**EOL 归因（`13-`）**：全树 `eolcheck mir2-rs/tests/parity/m3` = RED 27/3613，逐条核过全部落在
本片之外 —— 2 条是 m3hp 线已跟踪证据的 `*.rc`（未在 `mir2-rs/.gitattributes` 声明；reviewer-6 已
登记为该卡 M-3 Minor）、25 条是 `rng-hook/` 与 `shop-price-probe/` 下**未跟踪**的 .NET `bin/obj`
构建产物。**本片未新增任何路径进该 RED**；定向对本证据包跑 `eolcheck` = **GREEN**。

## 3. 未做 / 边界（如实登记）

- **真机同场景含怪两相 trace + `groupdiff --fold --strict`（卡级 DoD 真机半边）未跑**：需
  (a) owner 独占窗口、(b) 两侧「场景描述件 + 全场景抽数流水」驱动接线承接卡（写集预计含
  `crates/world` / `bins/**` / `src/**`，**超出**本卡禁令）。`run_rng_trace_pair.ps1` real-run 无 `-Go`
  仍 fail-closed `exit 2`（正确）。
- 跑前需补 `drop-exp/drop_exp_consumers.tsv` 的 **rust 侧 `measured` 行**（现 csharp 4 行 + rust 1 行
  wallclock）；该表自述「只填已由源码 + 真机确认的行」⇒ 必须窗口期实测补入，离线不猜填。
- 本片未起栈、未占端口（本刻机器上有他线 C#/Rust 栈在跑，本片纯离线）、未 merge、未自签 review、
  未动共享主检出。
