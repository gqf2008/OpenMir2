# 任务：M2 下一片（worker-11）——R4-1 收口 + 顺序判据② + **M2 验收运行手册**

工作区：`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\m2-svr`（分支 `worktree-m2-gamesvr-host`，head `55358fc2`）。
卡：`openmir2-m2-enter-world-frames`（你在待审）+ `openmir2-m2-gamesvr-host`（DoD 仍未达）。身份 `openmir2-worker-11`（已 join）。

## 0. 先读后写

`git fetch walgit '+refs/collab/*:refs/collab/*'`，读两张卡的线程（含 reviewer-3/reviewer-4 的 review 与逐条收口）。
**别重复交付已 approve 的内容**（16 条帧 + 生产路径下发已交 `55358fc2`）。

## 1. R4-1（owner 已登记为下一片必做，Minor）

`route_to_gate` 目前用 `u8::try_from(gate_idx).unwrap_or(0)` ⇒ **越界/非法线号被静默算成 0 号线**，而 oracle 的 `GetGate` 是返回 null（丢弃）。
改成「越界/非法线号 ⇒ 返回 None + 记 WRN」（与 N1 同族：不许静默挑线），并补：
① 单测（越界 `gate_idx` ⇒ 不投递、槽不被标）；② 红检（改回 `unwrap_or(0)` ⇒ 该用例必须红），原始输出留档。

## 2. 顺序判据②（能做的做掉，做不了的登记）

卡上原文：**入图序到 `SM_TURN` 同序且长度相等**。用 M0 金标准 s2c 的 ident 序
（`658,51,50,41,213,11029,42,766,54,5008,52,752,811,46,10…`）+ 你已实现的 16 条编码器做对账；
取不到的输入（真地图 `crates/envir`、记录字段 `openmir2-m2-record-fields`）逐条登记「缺哪个输入、由哪张卡提供、还差哪一步」。

## 3. 本轮重点产出：**M2 验收①②③的运行手册（harness）**

卡内 DoD 的四项证据（端口所有权实拍 / 真客户端 7 张截图 / 同序列出包 diff / 三路径 DB diff）现在就能先把**可复跑脚本 + 清单**做出来，
把"最终验收"从一次手工探险变成一条命令。要求：

- **端口所有权实拍**：一条脚本核对 5000/7000/7100/7200 的 PID + 命令行（证明是 Rust 入口在监听而不是 C# 栈），留证据文件。
- **真客户端截图**：写清入口（`D:\MirClient-run` + `run-release.cmd`）、7 张各自对应的画面与命名规则、截图时机；先交"清单 + 目录占位"。
- **同序列出包 diff**：把 Rust 侧 s2c 帧序列导出为 jsonl（字段口径**照** `tests/golden/session-20261010-c3-hook/capture.jsonl`：
  `dir/seq/port/hop/conn/raw_hex/ident/param/tag/series`），再与金标准对账的脚本（复用仓库既有 replay/对账工具，别新写一套）。
- **三路径 DB diff**：复用仓库既有的 dbsnap/DB 工具（见 M0/C 线工具说明），写出命令与期望。
- **每一步都要写清"失败怎么判红"**；脚本在没有栈时必须**明确报「前置未满足」并以非零退出**，不许静默通过。

**红线（本片）**：**不起栈、不起客户端、不占端口**——master 现在还没有 Rust 7200 入口（gamegate 未合），起栈也验不了；
本片只交付脚本/清单/自检 + 自己跑"无栈自检"证明脚本会正确报前置缺失。

## 4. 写集与门禁

- 允许：`mir2-rs/bins/game-svr/**`、`tools/**`（只新增你自己的 M2 验收脚本目录）、`docs/**`（只新增你那份手册，别改别人段）。
- 禁止：`crates/envir/**`（mapload 线）、`crates/world/**`、`src/**`（C# oracle）、`MirClient`、表结构/脚本语法/数据格式、别人的卡状态。
- 门禁（隔离 target `CARGO_TARGET_DIR=E:\tmp\openmir2-m2svr-target`）：fmt / clippy `-D warnings` / `test --workspace --offline` / msgcodegen `--check`。

## 5. 交付

按 §0b **一片一交**：`patch --base refs/heads/master --head refs/heads/worktree-m2-gamesvr-host` + `status: needs-review`
（parent 用该线程最后一条 entry 的 oid；R4-1 片交 `openmir2-m2-gamesvr-host`，手册片交 `openmir2-m2-enter-world-frames` 或在 m2-svr 卡上注明归属）。
**不 merge、不签 review。** 收工写 `E:\tmp\openmir2-watch\last-m2svr.md`。

## 现状（owner 侧更新，2026-10-11 13:42）

- **R4-1 你上一轮已收口**（head `992d03e8`，patch 已挂卡待审）⇒ **本轮不要再做 R4-1**，直接从 §2 的**顺序判据②**和 §3 的**M2 验收运行手册**开始。
- 分支已含 master `d22dd03d`（协调者更新的 §17 里程碑状态表）。
- 现在 `m3` 车道在跑 M3 首验并**独占 5000/7000/7100/7200** ⇒ 你的手册脚本本轮只交"无栈自检"（缺前置必须非零退出），**不要起栈**。
- 真图与记录字段两条依赖已有交付（`crates/envir` = mapload 卡待审、`record.rs` 全字段 = recf 卡待审），写前置时按卡号引用，别自己再造一份。
