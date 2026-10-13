# 证据：RNG 场景报告「声明计数 vs 真实证据包」机判（reviewer-6 M-1 收口）

- 卡：`openmir2-m3-rng-trace-scene-groups`（issue `d9d03f47`）
- 车道：`m3bag` / principal `openmir2-worker-4` / 分支 `wip/m3bag-evidence-count-tooth`
- base = `origin/master` `ef3160df76a2710c0662bc88e535d543f34c097f`（开工与交付前均 `git fetch && git merge`）
- 写集：`mir2-rs/tests/parity/m3/**`；**零 `.rs`**；未起栈、未占端口

## 1. 缺口（reviewer-6 M-1，可复现）

reviewer-6 对 head `72c9258dc` 的 approve（entry `1fe13505`）携带一条非阻塞 Minor **M-1**：
`M3RNG场景DoD②组清单机判组必须真出现报告-20261013.md:9-10` 的证据头行写
`12 件 + SHA256SUMS；verify 12/12，eolcheck 13/13`，而真实证据包是 **16 文件**
（`00`–`13` + `README.md` + `SHA256SUMS.txt`）⇒ 实测 `verify --check-extra` **15/15**、`eolcheck` **16/16**。
根因同族第 N 次：**报告头行硬编码计数**，而证据包在同一片里继续增删文件（本片交付头追加 `11-`/`12-`/`13-`
三件后头行未回改）。先例：reviewer-5 F-1 `82/82` 实际 `81/82`、reviewer-4 O-2、本卡 scenegroupdiff
报告 §7（当时只改了证据 README 一处，没有机判）。

## 2. 修复

1. **订正**该报告证据头行计数为真实读数：`16 件含 SHA256SUMS.txt；verify GREEN 15/15，eolcheck GREEN 16/16`；
2. **新增机判工具** `rng-trace-scene/check_evidence_counts.py`：
   - `check --report <md> --dir <证据包>`：解报告证据头行声明的 `N 件` / `verify N/N` / `eolcheck M/M`，
     与**真实包**对账——`N 件` 与 `eolcheck` 对**目录文件总数**，`verify` 对 `SHA256SUMS.txt` **条目数**；
     不一致 ⇒ rc=1 逐个点名；未硬编码计数（只写规则）⇒ rc=0；缺件/坏读取 ⇒ rc=2；
   - `selftest` 7/7：一致绿 / 件数·verify·eolcheck 漂移必红且点名 / 只写计数规则绿 / 缺件 PREREQ /
     **交付后证据包长大必红**。
3. README `rng-trace-scene/README.md` 增 §5.1 + §6 命令（**不改** `run_rng_trace_pair.ps1` 自检项数，
   故历史报告的 `29/29` 等片时读数不受影响）。

## 3. 判据与读数（原始件见本包）

| 件 | 命令 | 读数 |
|---|---|---|
| 01 | 订正前 `check`（真实 master 报告，`git show origin/master` 取出） | **rc=1**：`declared_files=12 != actual_files=16`、`declared_verify=12/12 != 15/15`、`declared_eolcheck=13/13 != 16/16` |
| 02 | 订正后 `check`（工作树报告） | **rc=0** `OK 声明计数与真实证据包一致`（=`{'files':16,'verify':(15,15),'eolcheck':(16,16)}`） |
| 03 | `check_evidence_counts.py selftest` | **7/7 GREEN** |
| 04 | 改坏必红：订正后报告副本 `16 件 → 15 件` | **rc=1** `declared_files=15 != actual_files=16` |
| 05 | 报告 head 行 diff（`git diff origin/master -- <report>`） | 旧 `12/12`、`13/13` → 新 `16 件`、`15/15`、`16/16` |
| 06 | 边界（`git diff --name-only origin/master`） | 冻结面 `src/ sql/ MirClient/ mir2-rs/crates/ mir2-rs/bins` **空**；非 tests/parity 的 `*.rs` **0** |
| 07 | `cargo fmt --all -- --check` | rc=0 |
| 08 | `cargo clippy --workspace --all-targets --offline -- -D warnings` | rc=0 |
| 09 | `cargo test --workspace --offline --no-fail-fast` | rc=0 · **101 suites / 1185 passed / 0 failed / 22 ignored** |
| 10 | `cargo run --offline -p msgcodegen -- --check` | rc=0 · **604** 条常量 |

## 4. hash 口径

- `SHA256SUMS.txt` 按**仓库字节**（`mir2-rs/.gitattributes` 的 `text eol=lf` ⇒ LF 归一 / Git blob）记；
- `verify --check-extra` = **GREEN 12/12**；`eolcheck` = **GREEN 13/13**（本目录全部文本件与仓库口径在 EOL 上一致）。

## 5. 边界与残留

- 本包只含一手转录 + 读数，**不含**构件；`00-commands.txt` 给出全部可复算命令。
- 仓外临时件（可删）：`E:/tmp/m3bag-evcount/`（`pre-fix.md` / `mutated.md`）；隔离构建 target
  `E:/tmp/m3bag-target`（可复用/可删）。
- 本片不触碰任何判据本体、不改 `rng_ledger_diff.py` / `rng_scene_gate.py` / `run_rng_trace_pair.ps1`；
  卡级真机 DoD（同场景含怪两相 trace + `-SceneGroupDiff`）仍等 owner 独占窗口。
