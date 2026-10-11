# 任务：M2 进图帧（卡 openmir2-m2-enter-world-frames）—— 接着做完剩余 11 条 + 接线进 sim 循环

工作区：`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\m2-svr`（分支 `worktree-m2-gamesvr-host`，head `dc33c1ca`）。
身份 **openmir2-worker-11**（该 worktree 已 join，命令可省略 `--actor`/`--key`）。

## 0. 现状（先读后写，别重做已完成的东西）

- 你在 `openmir2-m2-gamesvr-host` 上已交付：N1–N4 收口（head `d1a7bfd5`，含 master 合并，三条红检）
  与 S2b-4 第一片 `dc33c1ca`（`smframe.rs`，5 条帧逐字节对齐 M0 金标准，7 单测 + 2 红检）。
- 本卡 `openmir2-m2-enter-world-frames` 的 status 是**你**签的 `needs-review`（卡在待审）。
  **不要重复交付第一片**；先把卡上线程读完（含 owner 侧 comment `ae7e2e76`：我用**另一条路径**核过
  5 条常量与 ident 配对都成立——`capture.jsonl` sha256 `3052C778…`、SM_NEWMAP=51@seq16 / SM_LOGON=50@seq17 /
  SM_FEATURECHANGED=41@seq18 / SM_ATTACKMODE=213@seq19 / SM_VERSION_FAIL=1106@seq25）。

## 1. 本轮要做（按序，做不完就如实登记）

1. **剩余 11 条入图帧**（658 / 11029 / 42 / 766 / 54 / 5008 / 52 / 752 / 811 / 46 / 10）：
   尾字段来自玩家记录/地图/配置 ⇒ 需要**真地图**（`openmir2-m2-map-loading`，另一条车道在写 `crates/envir`）
   与**记录字段解析**（`openmir2-m2-record-fields`）。**不要为了凑数去猜尾字节**：
   能取到真输入的就逐字节钉住；取不到的就把「缺哪个输入、由哪张卡提供」写成登记项（含 oracle 行号）。
2. **接线进 sim 循环**：进世界后经网关（`gate_idx` 路由）下发 SM_* 首帧，把 `smframe` 从 `#[allow(dead_code)]`
   变成生产路径；判据：进程级用例——假 DBSvr 给记录 ⇒ 进世界 ⇒ 假网关在 7200 侧收到**逐字节等于金标准**的首帧
   （`bins/game-gate` 的 `client_frame` 已就绪）。红检：把 `client_frame` 的 Encode 去掉（或 Param/Tag 序写反）⇒ 必须红。
3. **顺序判据②**（卡上原文：入图序到 `SM_TURN` 同序且长度相等）：用 M0 金标准 s2c 的 ident 序列做对账
   （`658, 51, 50, 41, 213, 11029, 42, 766, 54, 5008, 52, 752, 811, 46, 10 …`），
   能做的部分做完，做不了的写明前置。

## 2. 写集与红线

- 只动 `mir2-rs/bins/game-svr/**`；**别动** `crates/envir/**`（map-loading 车道）、`crates/world/**`（wbevy 线）、
  `bins/game-gate/**`、`src/**`（C# oracle）、`MirClient`、表结构/脚本语法/数据格式。
- 四门禁自己跑（隔离 target `CARGO_TARGET_DIR=E:\tmp\openmir2-m2svr-target`）：`cargo fmt --all -- --check`、
  `cargo clippy --workspace --all-targets --offline -- -D warnings`、`cargo test --workspace --offline`、
  `cargo run -p msgcodegen -- --check`。
- 不起服务栈（端口由审查线按窗口用）；不 merge、不签 review、不动别人的卡状态。
- **别用只按源码公式重推的尾字节当判据**（本仓教训：常量名/适配器会骗人）——必须落到真字节。

## 3. 交付

本卡的 `patch --base refs/heads/master --head refs/heads/worktree-m2-gamesvr-host` + `status: needs-review`
（parent 用该线程最后一条 entry 的 oid；若你此刻也在 m2-svr 卡上有未收的差量，按 §0b 一片一交，别混在一起）；
收工写 `E:\tmp\openmir2-watch\last-m2svr.md`（做了什么、判据原始输出、卡在哪、剩余物清单）。
