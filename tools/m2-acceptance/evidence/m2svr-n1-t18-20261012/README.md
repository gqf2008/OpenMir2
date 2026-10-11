# N-1：T-18 调用点接线证据（`CM_TAKEONITEM`/`CM_TAKEOFFITEM` 传 prior `SubAbility`）

- 卡：`openmir2-m2-gamesvr-host`（owner `f4a1b02f` 的 N-1；T-18 收口）
- 落点：`mir2-rs/bins/game-svr/src/main.rs::handle_bag_cmd`（+ `bag.rs` 的两条 `CM_*` 常量）
- 本片写集：`mir2-rs/bins/game-svr/**`、`tools/m2-acceptance/**`（证据）、`docs/**`；
  `mir2-rs/tests/parity/whitelist.md` 仅改 **T-18 行**（owner 指定）
- **未改**：`crates/**`、`src/**`（C# oracle）、`MirClient`、表结构/脚本语法/数据格式；**未 merge、未签 review**

## 1. 接线口径

oracle 的装备/卸下成功分支会再调一次 `RecalcAbilitys()`（`PlayObject.Operate.cs:458`/`:547`），
而 `RecalcAbilitys` 只重置 `AntiPoison=0`/`AntiMagic=1`（`BaseObject.Abil.cs:30-31`），
**不重置** `PoisonRecover/HealthRecover/SpellRecover`（`PlayObject.cs:2890-2895` 是 `+=`），
道士的 `HitPoint` 也不重置（`RecalcHitSpeed` 的道士分支不赋值）⇒ 同一对象**重复调用会累加**。

因此接线必须传**调用前的** `SubAbility`：

```rust
// main.rs::handle_bag_cmd（CM_TAKEONITEM=1003 / CM_TAKEOFFITEM=1004 派发）
let recalc = game_svr::ability::recalc_abilitys_with_prior(catalog, &inputs, &sg.sub);
//                                                                        ^^^^^^ 调用前的 SubAbility
sg.state.wabil = recalc.wabil;
sg.sub = recalc.sub;
```

`sg.sub` = 进世界时首次 `RecalcAbilitys` 留下的 `SubAbility`（`SessionGround.sub`，由 `enter::recalc_for_session(...).sub` 落库）；
`EquipInputs` 同批保留 `Abil`/`BonusAbil`/各职业 tick 除数（进世界后 `CharacterData` 即丢）。

出帧顺序（照 oracle，`Operate.cs:455-463`/`:539-557`）：
`SM_ADDITEM`（换下回背包）→ `SM_TAKEON_OK/FAIL`（或 `SM_TAKEOFF_OK/FAIL`）→ `SM_ABILITY(52)` → `SM_SUBABILITY(752)`。

> **登记（未接，不是差异）**：oracle 在成功分支 `SendMsg(RM_ABILITY/RM_SUBABILITY)` 走普通优先级队列
> （下一轮 flush），本宿主在同一批直接下发；`FeatureChanged()` 给附近玩家的 `SM_FEATURECHANGED(41)`
> （装备触发的 AOI 下行）未建模。判据（本片）= 会话态 `sub` 的累加语义 + 进程级出帧序。

## 2. 红检（改坏必红）

| 文件 | 说明 |
| --- | --- |
| `redcheck-prior-firstcall.txt` | 把 `&sg.sub` 换成 `&game_svr::ability::SubAbility::first_call()` ⇒ `bag_takeoff_recalc_uses_prior_subability` **FAILED**（`left: 0 / right: 7`，rc=101） |
| `redcheck-prior-restored.txt` | 还原为 `&sg.sub` ⇒ 同用例 **ok**（rc=0） |

红检命令（在 `mir2-rs` 下）：

```powershell
CARGO_TARGET_DIR=E:\tmp\openmir2-m2svr-target cargo test --offline -p game-svr `
  --bin game-svr bag_takeoff_recalc_uses_prior_subability
```

## 3. 用例判据

`bag_takeoff_recalc_uses_prior_subability`（`main.rs`）：

1. 会话里 `sub.poison_recover = 7`（登录首次重算的遗留值）+ 槽 0 有一件可卸下的物品；
2. 调 `handle_bag_cmd(CM_TAKEOFFITEM, make_index=60001, slot=0, name="木剑")`；
3. 断言出帧 ident 序 = `[200, 619, 620, 52, 752]`（`SM_TAKEOFF_OK` + oracle 既有形态的 `SM_TAKEOFF_FAIL(0)`）；
4. 断言槽清空、物品回背包；
5. 断言 `sg.sub.poison_recover == 7`（prior 累加 + `add=0`）——**换 `first_call()` ⇒ 0 ⇒ 必红**。

## 4. 门禁（隔离 `CARGO_TARGET_DIR=E:\tmp\openmir2-m2svr-target`，`--offline`）

`cargo fmt --all -- --check` rc=0；`cargo clippy --workspace --all-targets -- -D warnings` rc=0；
`cargo test --workspace --offline` 全绿；`cargo run -p msgcodegen -- --check` = 604 条一致。
原始件见 `E:\tmp\openmir2-watch\`（不入库，按惯例）。

## 5. 残留物清单

- 本目录（证据，入库）：两份红检原始件 `.txt`、本 README。
- 仓库外：`E:\tmp\openmir2-m2svr-target`（构建产物）、`E:\tmp\openmir2-watch\`（门禁日志）。
- 未起栈、未占端口、未改 DB、未动 `src/**`。
