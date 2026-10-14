# M3⑨-2 残余（46 分身）宿主接线 证据

- 卡：`openmir2-m3-pet-slave-clone46`（M3⑨-2 残余：`MagMakeClone`/`PlayCloneObject` 宿主接线 + `RM_TURN` 帧）。
- principal：`openmir2-worker-20`；worktree `m3hp`；分支 `worktree-m3-pet-slave-clone46`。
- base：`refs/heads/master` = `083dc1d6d`（开工前 `git fetch` 后与 `origin/master` / `walgit` 同点）。
- 代码提交：`8c4e0fc49`（本证据目录为其上追加的一层；patch `--head` 指向**证据提交**）。
- 写集：`mir2-rs/crates/world/**` + `mir2-rs/bins/game-svr/**` + `mir2-rs/tests/parity/m3/**`（证据/登记）。
- 边界：`git diff --name-only 083dc1d6d 8c4e0fc49 -- src sql MirClient` = **空**（见 `04-boundary.txt`）。
- tier：**R**（触 RNG 抽数序列 `TrainSkill(Random(3)+1)`，且新实体进 AOI/展示面）⇒ 真机两侧 diff=0 需
  owner 独占窗口，本片 `window_required=true`、卡保持 `in-progress`。
- 未起栈、未占端口、未 merge、未自签 review；共享主检出未写。

## 0. 一句话结论

把 ⑨-2 已登记为 `CLONE_DEFERRED` 的 **46 分身**落成实现：`crates/world::slave::spawn_clone` 造
玩家型克隆体（`name="Clone"`、`race=Play`、坐标=主人、`Dir=GetBackDir(主人.Dir)`、格占用门、
**不入 `SlaveList`**、不抽 RNG）；`bins/game-svr::slave::cast_clone_spell` 走 `MagMakeClone`
（恒 `true`）⇒ `boTrain` ⇒ 共享尾（`RM_MAGICFIRE` + `Random(3)+1` 熟练度）；`handle_spell_cmd`
的 46 支按 oracle 次序发 `SM_SPELL` → 克隆体 `SM_TURN`(+`SM_FEATURECHANGED`) → `SM_MAGICFIRE`
→ `SM_MAGIC_LVEXP`。`downlink::clone_display` 从主人 `feature` 无损反解 Gender/Hair、按副本
缺省 Dress/Weapon=0 重算。

**设计差登记**：oracle `PlayCloneObject` 是完整 `PlayObject`（会走基类 `Run`/AOI/buff），本骨架
克隆体是**静态可观测实体**（tick/AI 生命周期未建模）⇒ 白名单 **T-51**，真机项 `not_verified`。

## 1. oracle 锚点
见 [`01-oracle-anchors.txt`](01-oracle-anchors.txt)：`MagicConst.cs:175`、
`MagicManager.cs:589-592`/`1337-1341`/`151-159`/`164-166`/`644-669`、
`PlayCloneObject.cs:3-23`、`BaseObject.cs:820`、`Envirnoment.cs:252-296`、
`PlayObject.Attack.cs:1269-1276`、`PlayObject.Message.cs:1524-1560`/`1788-1793`、`M2Share.cs:1965`。

## 2. 改前 / 改后
见 [`02-change-summary.txt`](02-change-summary.txt)（代码提交 6 文件 +672/-14；另证据提交内 1 行锚点刷新）。
- **锚点刷新（§7）**：本片在 `handle_spell_cmd` 插入 46 支（~110 行，在 `enable_production_monster_ai`
  调用点之前）⇒ `rng-trace-scene/DRIVER-CONTRACT.md` 的 `main.rs:5488` → **`:5598`**（`check_anchors.py`
  口径「文档行号 == 当前源码首个匹配行」）；刷新后 `check_anchors` check 7/7 + selftest 13/13 GREEN、
  `run_rng_trace_pair.ps1 -SelfCheckNoStack` **37/37 GREEN**。

## 3. 判据（全离线；零起栈/零端口）
见 [`03-tests.txt`](03-tests.txt) 与 [`gates/14-clone-tests.txt`](gates/14-clone-tests.txt)：
- **crates/world 单测**：名字/坐标/朝向/格占用/不入表/RNG 不变；
- **game-svr lib 单测**：尾部 RNG 账本与 `do_spell_tail(boTrain=true)` 同序列；克隆展示 feature；
- **game-svr 进程级用例**（走生产 `handle_spell_cmd`）：克隆实体 + 展示 dir + 格占用 + 帧种类与名尾。

## 4. 四门禁
| 门禁 | 读数 | 日志 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | rc=0 | `gates/10-fmt.txt` |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 | `gates/11-clippy.txt` |
| `cargo test --workspace --no-fail-fast` | 101 suites · **1205 passed · 0 failed · 22 ignored** rc=0 | `gates/13-workspace-test.txt` |
| `cargo run -p msgcodegen -- --check` | **604 条** rc=0 | `gates/12-msgcodegen.txt` |

## 5. 改坏必红
见 [`redchecks/README.md`](redchecks/README.md)：RC1（dir 用正向）/ RC2（不发 `SM_TURN`）/
RC3（不造实体）→ 逐条 RED，还原后 GREEN。

## 6. 离线窗口前置 + not_verified
- `run_hero_pet_pair.ps1 -SelfCheckNoStack`（串行）**23/23 GREEN rc=0**（`gates/15-hero-pet-selfcheck.txt`）。
- **not_verified**：真机两侧 `run_hero_pet_pair.ps1` 同序列 diff=0（含 `make_clone` step）未跑 ——
  需 owner 独占窗口（`WINDOW-OPEN` 不存在）。克隆体 tick/AI 生命周期未建模（T-51），不在本片判据内。

## 7. 残留物清单
- 隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3hp-target`（保留复用）。
- 本目录证据（8 文件 + gates/6 + redchecks/8）。
- 分支 `worktree-m3-pet-slave-clone46`（未 push、未 merge）；worktree `m3hp` 干净（提交后）。
- 零端口/零栈；共享主检出未写；`lanes.conf` 未改。
