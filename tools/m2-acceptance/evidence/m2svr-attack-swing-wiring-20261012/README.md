# m2-svr 接线片：无目标一击接到 `World::attack_swing`（M3②③ 关键路径，2026-10-12）

- 任务来源：`brief-m2svr-next.md` 顶部 **owner 2026-10-12 20:10**（新活，②③ 关键路径）
- 车道：`m2svr`／principal `openmir2-worker-11`／worktree `.claude/worktrees/m2-svr`／分支 `worktree-m2-gamesvr-host`
- base = `refs/heads/master` `de0ea592`（已合 m3cs 的 `World::attack_swing` `614f5407` + m3bag M-1 `de0ea592`）
- 写集：**仅 `mir2-rs/bins/game-svr/src/main.rs`**（含单测）；未动 `crates/world/**`（m3cs 写集）、未动夹具
- 窗口：**本片未起栈、未占端口**（判据全在单测层；端口实拍见 §6）
- 卡：`openmir2-m2-attack-swing-wiring`（issue 见卡）；**不 merge、不自签 review**

---

## §1 结论

`handle_hit_cmd` 的**无目标支**（前方格无 `!Ghost && !Death` 目标）此前只记日志、**不抽任何随机数**；
oracle 在该支**仍先算一次攻击力**。本片把该支改调 m3cs 新增的 `World::attack_swing(actor_id)`
（`crates/world/src/world.rs:199`，仅抽攻击力、不抽命中门、不抽 `attack_success`），
补回真机那 **84/120 击空打**在 Rust 侧整段缺失的取数（C# 120 段 vs Rust 36 段）。

## §2 oracle 锚（逐行，原文见 `00-oracle-attack-path-lines.txt` / `01-oracle-powersite-lines.txt`）

`PlayObject.AttackDir(IActor targetObject, short wHitMode, byte nDir)`：

| 步 | 行号 | 行为 | 无目标（`targetObject == null` 且 `GetPoseCreate()==null`） |
|---|---|---|---|
| 1 | `PlayObject.Attack.cs:26` | `attackTarget = targetObject ?? GetPoseCreate()` | `attackTarget = null`（**无取数**） |
| 2 | `PlayObject.Attack.cs:73` | `GetAttackPowerHit(wHitMode, GetBaseAttackPoewr(), attackTarget, ref canHit)` —— 实参先求值 | **`GetBaseAttackPoewr()` 照抽**：玩家 `Race==Play` ⇒ `PlayObject.cs:2909` override ⇒ `Luck<=0` 时 `Random(LoByte(DC)+1)` + `Random(10)` **共 2 抽** |
| 3 | `BaseObject.Attack.cs:30` | `AttackDir(attackTarget, nPower, nDir)` = `_Attack` | `:32` 目标 null ⇒ `return false`（命中门 `:41` **不抽**） |
| 4 | `PlayObject.Attack.cs:76` | `SendAttackMsg(...)` | 无取数（动作帧照发） |
| 5 | `PlayObject.Attack.cs:77` | `AttackSuccess(wHitMode, nPower, canHit, attackTarget)` | `:215-218` 目标 null ⇒ 开头 `return`（`Random(5)` **不抽**） |

⇒ **无目标一击 = 恰好 2 抽（玩家）/ 1 抽（怪物）**，全部来自第 2 步；这也是
`M3RNG攻击路径三差归因报告-20261012.md` §4 与 `World::attack_swing` 文档注释的口径。

## §3 改前 / 改后读数（构造层，可复现）

单测 `hit_cmd_without_target_still_draws_attack_power`（`main.rs`）走**真实 `handle_hit_cmd` 路径**
（前方格留空 ⇒ `front_target==None`），用 `RandomNumber` 的完整 state 比对数出**内部采样次数**
（每次小范围 `random_below` 恰 1 采样）：

| 读数 | 改前（删接线，红检） | 改后 |
|---|---|---|
| 单次无目标一击 | `ATTACK_SWING_DRAWS_PER_EMPTY_HIT=0` | `ATTACK_SWING_DRAWS_PER_EMPTY_HIT=2` |
| 84 击空打（真机形态：受害者 36 击后 `death=true`） | 0 抽 / 0 段 | `ATTACK_SWING_DRAWS_84_EMPTY_HITS=168`（= 84 段） |
| 攻击路径段数（36 带目标 + 84 空打） | 36 | **120** |

- 带目标支仍走 `world.attack_cmd`（下一 world tick 由 `attack_dir` 结算，m3cs 的 `Random(5)` 已补）。
- **真机两相逐 k 读数（DoD② 的 `attack_diff.py diff=0`）不在本片**：仓口要求两侧栈 + 0137 夹具 +
  同种子窗口，属 m3cs（`openmir2-m3-combat-skills-align` remaining ②）／m3bag（`openmir2-m3-attack-pair-fixture` remaining ①）的取窗复跑。
  本片已把 Rust 侧生产路径的取数补齐，**未声明 DoD② 达成、未放宽门**。

## §4 证据件（命令 + 输出 + 路径 + hash）

| # | 文件 | 内容 |
|---|---|---|
| 00 | `00-oracle-attack-path-lines.txt` | `PlayObject.Attack.cs:25-77 / 215-226` 原文（行号锚） |
| 01 | `01-oracle-powersite-lines.txt` | `BaseObject.Attack.cs:25-44`、`PlayObject.cs:2909-2935` 原文 |
| 02 | `02-prod-diff.txt` | `git diff de0ea592..HEAD -- mir2-rs/bins/game-svr/src/main.rs`（生产改动 + 单测） |
| 03 | `03-green-targeted.txt` | 绿：`cargo test -p game-svr --offline hit_cmd_without_target_still_draws_attack_power -- --nocapture` ⇒ 1 passed / 读数 2、168 |
| 04 | `04-redcheck-remove-swing.txt` | **改坏必红**：删 `let _ = world.attack_swing(actor_id);` ⇒ `...PER_EMPTY_HIT=0`、断言 `left:0 right:2` FAILED |
| 05 | `05-gate-fmt.txt` | `cargo fmt --all -- --check` ⇒ `FMT_RC=0` |
| 06 | `06-gate-clippy.txt` | `cargo clippy --workspace --all-targets --offline -- -D warnings` ⇒ `CLIPPY_RC=0` |
| 07 | `07-gate-test.txt` | `cargo test --workspace --offline` ⇒ `TEST_RC=0`（83 suites／**675 passed**／0 failed／16 ignored） |
| 08 | `08-gate-msgcodegen.txt` | `cargo run -p msgcodegen --offline -- --check` ⇒ `MSGCODEGEN_RC=0`（604 常量） |
| 09 | `09-commands.txt` | 上述命令的原文与隔离 target 口径 |
| — | `SHA256SUMS.txt` | 本目录件的仓库字节（LF 归一）sha256 |

## §5 门禁（合 master `de0ea592` 后在**交付树**上复跑）

`CARGO_TARGET_DIR=E:/tmp/openmir2-m2svr-target`（隔离）：
`fmt 0 / clippy 0 / 83 suites・675 passed・0 failed・16 ignored / msgcodegen 604`。
改动面 `src/`（`src/M2Server/**`）**零改动**；本片路径仅 `mir2-rs/bins/game-svr/src/main.rs`。

## §6 窗口与残留物

- **未起栈、未占端口**（判据全在单测；`cargo test -p game-svr` 用测试自带临时端口/无网络）。
- 现场端口实拍（本片开工前）见 `09-commands.txt`（5000/5100/5500/5600/5700/6000/7000/7100/7200 FREE，仅共享 MySQL `3306`）。
- 残留物：仅隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m2svr-target`（构建缓存，非证据；不随交付）。
- 未改 `crates/world/**`、`tests/parity/m3/**`、夹具、白名单、判据；未 merge、未自签 review。
