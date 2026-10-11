# 任务：world-bevy **S2 调优**（worker-12）——按 owner 门禁把 ΔP50 / ΔP99 拉回绿

工作区：`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\wbevy`（分支 `worktree-world-bevy`，head `e6d25d23`）。
卡：`openmir2-world-bevy-migration`（needs-review；reviewer-3 本轮判 **request_changes**）。身份 `openmir2-worker-12`。

## 0. 先读（别重做已 approve 的部分）

- 卡内最新 review：**S1-P99-gate-RED（Important）**——按 owner 裁决 `ba5fb504` / `5e1daeee` 的**同一次会话交替 A/B**口径
  （tiers 500/1000/2000，各 3 轮取中位数）实测：**ΔP50 = +101.2% / +33.4% / +52.5%，ΔP99 = +81.7% / +56.5% / +19.0%**，
  逐轮配对 **9/9 都慢** ⇒ 远超 RED 线（P50 > 10% / P99 中位数 > 30%）。原始件 `E:\tmp\openmir2-watch\rev3-ev\wbevy-ab-{A,B}{1,2,3}.txt`。
- owner 裁决的口径是：**任一超阈 ⇒ 转 S2 调优（先调优，不直接回退）**；reviewer 也明确「不接受用『绝对 P99<100ms』绕开相对门禁」。

## 1. 要做的

1. **按 reviewer 指出的热点优化**（硬约束：三份 golden 必须逐字节不变）：
   - `crates/world-bevy/src/ecs.rs` 的 `EcsStore::iter/iter_mut/order_index`：目前每次调用都**建 HashMap + Vec 并 sort**（每 tick 多次）
     ⇒ 缓存 `ComponentId`、复用缓冲、只在 `order_index` 真的变化时重排；
   - `crates/world/src/entity.rs:200-260` 的薄适配层 + `world.get` 的 entity→location→table 间接层 ⇒ 预解析句柄 / 批量取引用 / 减少重复查询；
   - **不许把并行引进来**（§4.2.2 四条硬约束照旧；`no_parallel` 守卫必须仍绿）。
2. **用同一 A/B 口径重测**：同一次会话内交替跑 master(`50b8d774`) 与你的新 head 的 **release** `tick_bench`，
   tiers 500/1000/2000 各 3 轮取中位数；**目标 ΔP50 ≤ 10% 且 ΔP99 中位数 ≤ 30%**。
   - 测量纪律：先看机器上有哪些车道在跑（`Get-CimInstance Win32_Process -Filter "Name='node.exe'"`）与有没有人在起栈
     （现在 `m3` 车道在跑 M3 首验、占 5000/7000/7100/7200）；**你的 A/B 不吃端口**，但要在证据里写明"当时谁在跑"。
     若抖动大到无法判别方向，就等一个更安静的窗口再测，并如实记录。
3. **Minor**：`crates/world-bevy/Cargo.toml:14` 引用的 `tests/no_parallel_bevy.rs` **不存在** ⇒ 改成实际路径
   （`tests/bevy_drop_parity.rs::no_parallel_guard` 与 `crates/world/tests/no_parallel.rs`）。
4. 三份 golden 逐字节不变（`basic.txt` / `session.txt` / `combat.txt`，reviewer 已给 sha256）+ 四门禁全绿（隔离 target）。

## 2. 写集 / 红线

- 允许：`crates/world-bevy/**`、`crates/world/src/entity.rs`（薄适配层）、`crates/world-bevy/Cargo.toml` 注释、你自己的证据文件。
- 禁止：`crates/world` 里除 `entity.rs` 之外的面（别的线在用）、`src/**`（C# oracle）、`bins/**`、`MirClient`、表结构/脚本语法/数据格式。
- 不 merge、不签 review、不动别人的卡状态。

## 3. 交付

`patch --base refs/heads/master --head refs/heads/worktree-world-bevy` + `status: needs-review`（parent 用该线程最后一条 entry 的 oid）；
收工写 `E:\tmp\openmir2-watch\last-wbevy.md`：A/B 命令 + 原始输出路径 + 每轮数字与中位数 + golden sha256 对照 + 四门禁原始结果。
