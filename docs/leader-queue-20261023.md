# Leader 派工队列（2026-10-23）

> 依据：`docs/rewrite-remaining-inventory-20261022.md`（185 在飞卡 = A 窗口欠账 152 / B 未接线 22 / D 8 / C 2 / O 1）、
> `docs/conformance-first-dual-run-20261023.md`（对拍台首跑）、`docs/leader-loop-20261023.md`（Leader 模式）。
> **规则**：写集重叠即串行（尤其 `bins/game-svr/src/lib.rs`、`gmcmd.rs`）；一次并行 ≤3 条车道；
> R 卡合并后保持 in-progress，L 卡可 closed。

## 1. 正在跑（2 条）

| # | 卡 | 车道 | 写集 | tier |
|---|---|---|---|---|
| 1 | `openmir2-login-529-status-text` | wt-529 / login-529-status | `bins/login-srv/src/**` | R |
| 2 | `openmir2-conformance-harness`（规则表 + `run_dual.sh`） | wt-rules / conformance-rules | `tools/conformance/**` | L |

## 2. 队列（按优先级；#3/#4/#5 互斥，需串行）

| # | 卡 | 单元 | 写集 | tier | 依赖 |
|---|---|---|---|---|---|
| 3 | `openmir2-dualrun-triage-enterworld-frames` | 进世界 4 类帧（10/100/654/657）分诊 | `bins/game-svr/src/**` | R | 对拍台（已完成） |
| 4 | `openmir2-conformance-harness` | 规则首跑后补规则（按 §2.3 暴露逐条） | `tools/conformance/**` | L | #1 复跑 |
| 5 | `openmir2-gm-commands-design` | GM 第 4 批：数值/落库 44 条 | `bins/game-svr/src/gmcmd.rs` + `lib.rs` | R | 与 #3 串行（都动 game-svr） |
| 6 | `openmir2-envir-redlist-batch` | 内容加载剩余件（AdminList 等） | `crates/envir/**` | L | 无 |
| 7 | `openmir2-orphan-encoder-wiring` | 13 个孤儿编码器接生产者 | `bins/game-svr/src/{smframe_ext,pickwire}.rs` | L | 与 #3 串行（同 crate） |
| 8 | `openmir2-dualrun-window-burndown` | A 卡→协议场景映射，批量烧窗口欠账 | `tools/probe/scenarios/**`、`docs/` | L | 需先有 #4 的稳定规则表 |

## 3. 阻塞（需要 owner / 窗口，Leader 只登记不派）

- **A 类 152 张**（代码已合、缺真机证据）：由 #8 用协议对拍逐批替代；GUI 才能验的（渲染/点击几何/输入法）另列。
- **GM 132 条未实现**：现网 `Permission` 恒 0，命令体不可达（设计件 §2.4）——除非 owner 批准用 `SETPERMISSION` 脚本构造。
- **whitelist 待复核**：T-161..T-172（含 T-172 SelGate 端口，已获口头批准、待留痕）。
- **C# 真值栈**：私有 root `E:/tmp/conformance/csharp-root` 已能在 15xxx/17xxx 起；对拍时与 Rust 串行、同端口。

## 4. 每轮派活的 brief 必含（Leader 检查表）

1. 卡 id + 角色 + 唯一 owner + worktree/branch；
2. **oracle 判据**（`src/**` 文件:行号，逐条）；
3. 写集（越界需预先追认）；
4. 门禁：fmt / clippy / test --workspace / msgcodegen（动 whitelist 加 anchor gate + registry audit）；
5. **牙**：≥2 颗，先记 sha256 → 改坏 → 红 → 还原核对；
6. 纪律：不碰 `src/**`、不起进程/不占端口/不写生产库、不并发跑门禁；
7. 交付形态：commit + push + 线程 `patch`（base/head）+ 返回摘要。
