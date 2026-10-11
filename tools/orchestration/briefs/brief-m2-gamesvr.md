# 任务简报：M2 主体（Rust GameSvr 世界宿主）——交给 worker-4（或 worker-6）

仓库：`E:\Users\gxh\Documents\GitHub\OpenMir2`
看板卡（唯一事实来源）：**「M2 主体：Rust GameSvr 世界宿主（真客户端经 Rust 链进图可玩 + M2 验收①②③证据）」**，
线程 id `openmir2-m2-gamesvr-host`（我以 `openmir2-orchestrator` 开的 issue `39d6ed20`，**待认领**）。

## 0. 为什么有这张卡

- M2 的 DoD（设计文档 §6 M2）就是「Rust GameGate + GameSvr 的最小可玩世界」，但 `mir2-rs/bins/` 下
  只有 `login-gate/sel-gate/login-srv/db-srv/game-gate`，**没有承载世界的 `game-svr` 宿主** ⇒
  gamegate 合入后也只是「入口=Rust、世界=C#」，M3 首验与 M5 压测都建在这个宿主之上。
- 世界本体（tick/实体/AOI/战斗/掉落/会话进世界）已在 master（`openmir2-m2-world-kickoff` /
  `openmir2-m2b-world-session` 两张卡），本卡是**把它跑起来对外服务**，不是重写。

## 1. 身份纪律（不可协商）

- 一个 agent 一个 principal：只用一个身份签条目，绝不借用 `openmir2-coordinator`（已退役）、
  `openmir2-reviewer-*`、或别的 worker 身份。审查由**不同 principal** 做。
- 交付只到 `needs-review`：**不要自己 merge**，不要签 review。
- 大任务按卡内 S1→S4 拆子线程，**每片自己的 worktree/身份/卡**（别一张卡塞四片）。

## 2. 开工前的固定动作

1. 读 `AGENTS.md` 与 `docs/Rust重写设计文档.md` §2（端口/oracle 对账）、§4.2（确定性四条硬约束）、
   §6（M2/M3 DoD）、§6.0（证据四件套与红检纪律）、§14。
2. `git fetch walgit '+refs/collab/*:refs/collab/*'` → 读 `collab board` 与 `collab thread openmir2-m2-gamesvr-host`。
3. 建独立 worktree：`git worktree add .claude\worktrees\m2-svr -b worktree-m2-gamesvr-host`（名字自定，别撞别人）。
4. `walgit collab join --repo <worktree> --principal <你的 principal> --key %USERPROFILE%\.walgit\keys\<你的 principal>.ed25519 --push walgit`，
   然后在本卡发 `status: in-progress` + `comment` 写清本轮范围（parent 用该线程最后一条 entry 的 oid），再动手。
   若已有别人更早的认领 ⇒ 停手回报。

## 3. 本轮建议范围（= 卡内 S1，先做协议级自测，别一上来追「客户端可玩」）

`bins/game-svr` 进程骨架 + 上游对接：§3 的 `GM_*`（GM_OPEN/GM_CLOSE/GM_CHECKSERVER/GM_DATA/
GM_SERVERUSERINDEX/GM_STOP）收发与会话表；单线程 tick 循环（§4.2 四条硬约束：无并行 executor、
遍历序只来自自己的有序索引、`no_parallel` 守卫、迁移验收不降级）；缺配置/连不上上游**明确报错非零退出**（M1④）。

判据：假 game-gate + 协议级自测（GM_OPEN → 会话表登记 → GM_SERVERUSERINDEX 回填 → GM_DATA 双向 → GM_CLOSE 清理），
含一条红检（把会话路由改坏必须红）。

## 4. 硬门禁（自己跑，贴原始输出；RED 不许交）

`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets --offline -- -D warnings`；
`cargo test --workspace --offline`；`cargo run -p msgcodegen -- --check`。

> 取判据用**隔离 target**（`CARGO_TARGET_DIR=E:\tmp\openmir2-m2svr-target`）。本机已有先例：
> worktree 的 `target/` 可能混入别的（已删除）worktree 的旧产物，直接跑会拿陈旧二进制报 `NotFound` 假红。

## 5. 交付

patch 挂卡（`--base refs/heads/master --head refs/heads/<你的分支>`，body 带 `files`/`why`/`evidence`/`remaining`）
+ `status: needs-review`（parent 用上一条 entry 的 oid）。证据四件套写进 body。

## 6. 红线

- 不得改 `MirClient`（冻结客户端）、不得改表结构/脚本语法/数据文件格式。
- 不得改 `src/**`（C# oracle）**行为**；BotSrv 属测试工具，要改按 `T-*` 登记并单独提交。
- 不改别人的卡状态、不写别人的 inbox。

## 7. 完成后回报（给 /root 控制台）

新增/改了哪些文件、四门禁与协议级自测的原始结果、patch/status 条目 oid、卡在哪（还有哪些剩余物）。
