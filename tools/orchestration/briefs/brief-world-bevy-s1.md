# 任务：World → bevy_ecs 迁移 · 批次 S1（worker-12）

工作区：`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\wbevy`（分支 `worktree-world-bevy`）。
卡：`openmir2-world-bevy-migration`（唯一事实来源）。身份：`openmir2-worker-12`（worktree 已 join）。

## 先做
`git fetch walgit '+refs/heads/*:refs/remotes/walgit/*'` 并 `git merge --ff-only master`；读卡 + 读 `docs/Rust重写设计文档.md` §4.2.2/§4.2.3 与 `mir2-rs/tests/parity/world/D3试验田报告.md`、`P99复测报告.md`、`crates/world-bevy/src/lib.rs`。

## 本轮范围（只做 S1，不越界）
把 `crates/world` 的**实体存储 + tick 骨架**迁入 `bevy_ecs`（`crates/world-bevy`）：
- 用 `World` + 单线程 `Schedule`（`SingleThreadedExecutor`）承载实体/组件与 tick；
- **保留既有实体索引 API**（薄适配层，`crates/world` 的对外行为不变）——目标是把内核换成 ECS，不是改语义；
- 处理顺序来自**自己的有序索引**（如 `OrderIndex`），**不迭代 Query 求顺序**。

## 四条硬约束（缺一 parity 就 flaky，见 §4.2.2）
1. 单线程 executor：禁 `MultiThreadedExecutor`/`bevy_tasks` 并行/`rayon`/`crossbeam`；
2. 遍历顺序显式化：不许依赖 Query 迭代序；
3. 守卫升级：`no_parallel` 白名单放宽为「只允许 `bevy_ecs`/`bevy_app`」，保留 tick 线程断言，**扫代码不扫注释**；
4. 原判据不降级：14 实体×10 tick 与 session/drop 金标准跨侧 diff=0；P99 对比 `tests/parity/world/p99-baseline*.txt` 不劣化（如劣化须登记并说明）。

`bevy_ecs` 版本锁 **=0.19.1**。

## 硬门禁
`cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets --offline -- -D warnings` / `cargo test --workspace --offline`；
阳性对照：故意加并行、故意依赖 Query 序，各须让守卫变红。

## 交付
patch（base refs/heads/master，head refs/heads/worktree-world-bevy）+ `status: needs-review`。**不自行 merge / 不签 review。** 证据四件套写进 body。

## 红线
不得改 `src/**`（oracle）行为；不得动客户端；不得把并行引进 `crates/world`；一次只动 S1 范围（aoi/combat/session 留后续批次）。

## owner 裁定（2026-10-11）：终点是**退役 crates/world**
- 不是「world 与 world-bevy 长期并存」；`world-bevy` 承接全部职责后，**删掉 `crates/world`**（退役批 **S5**）。
- `mir2-world` 的依赖方只有三处：`crates/world-bevy`（适配层）、`tests/parity`（common/mod.rs、world_aoi_parity.rs）、`crates/world/src/bin/tick_bench.rs`。
- 因此 S1 的适配层是**过渡**：目标是让 S5 能整体切到 world-bevy 并删除 world。**不要加深对 mir2-world 的依赖**；每批都朝「world 可删」推进。
