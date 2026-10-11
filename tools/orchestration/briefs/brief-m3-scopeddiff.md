# 任务：openmir2-m3-scopeddiff-multiset —— scoped_diff.py 重复行「假绿」改多重集/计数式

工作区：本卡专用 worktree `.claude/worktrees/scopediff`（分支 `worktree-m3-scopeddiff`，基于 `refs/heads/master`）。
身份：**openmir2-worker-3**（先 `walgit collab join --repo . --principal openmir2-worker-3 --key C:\Users\gxh\.walgit\keys\openmir2-worker-3.ed25519 --push walgit`）。
卡：`openmir2-m3-scopeddiff-multiset`（**待认领**）——先 `git fetch walgit '+refs/collab/*:refs/collab/*'` 读卡内 issue，再发 `status: in-progress`。

## 0. 为什么现在做（不要当成低优先）

`m3` 车道此刻正用 `run_m3_gate.ps1` 出 M3 走**跨侧字段级 diff=0** 的里程碑证据，而本卡的缺陷会让**重复行**场景**假绿**
（`A=[x,x]` 对 `B=[x]` 判不出来）⇒ 那条里程碑证据现在带着这个口子。修好之后，首验已落地的 session 制品可以用**修好的比较器重跑比较**
（不必重跑整栈）——交付里请给出「如何用新工具重比既有产物」的确切命令。

## 1. 要做的（照卡内 machine_checkable_acceptance）

1. `scoped_diff.py cmd_diff` 改成**多重集/计数式**比对（同 `(表, 行 key)` 的**重复行数**纳入判定），不再用 set 差集。
2. **红检（必须留原始输出）**：`A=[x,x]` vs `B=[x]` ⇒ **RED / exit 1**；`A=[x,x]` vs `B=[x,x]` ⇒ GREEN。
   复现件在 `E:\tmp\openmir2-rev\dup_a.jsonl` / `dup_b.jsonl`（reviewer-1 留档）；若盘上已无，自己造等价件并入库为本 crate 的测试夹具。
3. **不回归**：现有四条判据仍绿——两个 C# 会话未 canonical=RED 8 行、`--canonical`=GREEN、selftest 5/5、
   改被比对列（Level）=RED 而改易变列（Hair）=GREEN。
4. 复跑 `run_m3_gate.ps1` 的两侧命令，确认 README / 设计文档 §6 的口径仍一致；四门禁全绿（**隔离 target**）。

## 2. 写集 / 红线

- 允许：`mir2-rs/tests/parity/m3/**`（`scoped_diff.py` + 它的 selftest/夹具 + README 与设计文档里**本卡相关**段落）。
- 禁止：`src/**`（C# oracle）、`bins/**`、`crates/**`、`MirClient`、表结构/脚本语法/数据格式、别人的卡状态。
- **现在 `m3` 车道在跑首验：别起栈、别占 5000/7000/7100/7200**（本卡是纯比较器工作，不需要栈）。
- 不 merge、不签 review。

## 3. 交付

`patch --base refs/heads/master --head refs/heads/worktree-m3-scopeddiff` + `status: needs-review`（parent 用该线程最后一条 entry 的 oid）；
收工写 `E:\tmp\openmir2-watch\last-scopediff.md`：改了什么、红检原始输出、四条不回归判据的实跑结果、重比既有产物的命令。
