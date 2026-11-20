# 657 `Series` 来源修正：spellbuf 两处（2026-11-20）

> base `master = 977d260a1`；分支 `card/657-series-spellbuf`。
> 上游：`docs/issues/qa-657-wiring-plan-20261120.md`（657 全量扫描：12 站点、2 处写死 0）。
> 与本卡**分开**的相邻问题：`docs/issues/openmir2-recalc-entity-mirror-gap.md`（实体镜像与站点时序）。

## 1. 问题

两处 spell 路径的 657 先造**单个 body**（`sm_charstatuschanged(..., 0)`，`Series` 写死 0），
再用 `push_broadcast_cached(..., &body)` 发给所有人 ⇒ 全部收件人（含自己）`Series` 恒 0。

oracle handler（`src/M2Server/Player/PlayObject.Message.cs:1845-1847`）：

```csharp
SendDefMessage(SM_CHARSTATUSCHANGED, processMsg.ActorId, LoWord(nParam1), HiWord(nParam1), this.HitSpeed);
```

⇒ `Series` = **该收件人自己**出队那一刻的 `HitSpeed`，不是源对象的、也不是常量 0。

## 2. 改动（只改来源，帧数不变）

两站点改用既有逐收件人 helper `push_charstatuschanged(&mut buf, world, online, actor, status)`
（每个收件人按**自己**的实体 `hit_speed` 重新编码）：

| 站点 | 位置 | 说明 |
| --- | --- | --- |
| `SKILL_SHIELD(31)` 的 `take_buff_applies` 支 | `bins/game-svr/src/lib.rs`（`handle_spell_cmd_with_magic` 的 `mir2_world_bevy::world::SKILL_SHIELD` 臂） | oracle 本支只 `StatusChanged()`、**无 `SendUpdateMsg`** ⇒ 每人恰 1 条 ⇒ 用逐收件人 helper，**不**用三动作 helper |
| buff-apply-kind 支（219 sysmsg 之后） | 同函数 | 广播副本改逐收件人；自更新副本仍 1 条（取自己实体值），自见合计仍 2 条 |

`Series` 的源 = **收件对象实体**的 `hit_speed`（裁定：实体为源、`SessionGround` 只是镜像）。

## 3. 证据边界（先写清楚，避免把"绿"读大）

master 侧的 magic golden **已经重生成**为 657 `series` 全 0，且**没有** `Fixture.hit_speed` 键；
带 `hit_speed` 旋钮、能产出 `series=7` 的是另一条线（`b3844fd22`，`card/magic-worldengine`）。

⇒ **在现行 golden 上，"单 body 恒 0" 与 "逐收件人实体值" 产出完全相同**，
六比较器（§5）对本次来源修正**没有区分力**。保留逐收件人来源的理由是 **oracle 的结构**
（handler 读 `this.HitSpeed`），不是 golden 的绿。区分力由 §4 的哨兵承担；
golden 侧拿到旋钮后另开卡（[`card/657-series-spellbuf`] 不覆盖 golden 重生成）。

## 4. 哨兵（设计 3：真实形态，非人造区分态）

`tests::spell_657_series_is_per_recipient_entity_hit_speed`：

- 装置：`fireball_rig(1000, magic=31, ...)` + `world.enable_buff_tick()`（magic 31 走 buff 生产链；
  关 tick 时本路径根本不产 657，见既有 `shield_buff_tick_gate_off_emits_no_buff_frames`）。
- 状态：**施法者实体 `hit_speed = 7`、目标实体 `0`、`obs_in` 实体 `0`** —— 真实形态
  （加速来源只挂在施法者身上；buff 施加只对目标做 `RecalcAbilitys`）。
- 断言（**收紧版**）：socket 911（施法者）收到的 657 **恰 1 条**且 `Series == [7]`；
  socket 912（目标/观察者）**必须非空**且 `Series` 全 `0`。
  > 初版只写 `contains(&7)` 与 `all(|v| v == 0)`：后者在**空集**上会整体空过
  > （`[[tooth-must-answer-empty-input]]`）。已改为精确相等 + 目标侧非空断言，交付证据一律取收紧后的实跑。

**旧实现（单 body、`Series` 恒 0）实跑原文**：

```text
test tests::spell_657_series_is_per_recipient_entity_hit_speed ... FAILED
thread 'tests::spell_657_series_is_per_recipient_entity_hit_speed' (39096) panicked at bins\game-svr\src\lib.rs:71112:9:
assertion `left == right` failed: 施法者收到的 657 必须恰 1 条且 Series = 自己实体 7（旧单 body 会得 0）
  left: [0]
 right: [7]
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2229 filtered out; finished in 0.03s
```

**新实现实跑原文**（同一收紧版用例）：

```text
test tests::spell_657_series_is_per_recipient_entity_hit_speed ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2229 filtered out; finished in 0.02s
```

两条都是**单跑**（`2229 filtered out` 证明不是被别的用例顺带跑过），命令：
`CARGO_BUILD_JOBS=2 cargo test -p game-svr --lib spell_657_series_is_per_recipient_entity_hit_speed`。

## 4.5 变异流程事故留痕（同一批，已恢复）

一键变异脚本的 `trap restore EXIT` 里用的是**相对路径**，而脚本中途 `cd` 到了 `mir2-rs/`
⇒ 触发时 `cp` 报 `cannot create regular file 'mir2-rs/bins/game-svr/src/lib.rs': No such file or directory`，
**变异留在了树里**（该次 `lib.rs` sha `92077d30…`、`MUTATION-657-OLDBODY` 标记 = 1；快照当时为 `d4be4275…`）。

发现方式：跑完立刻 `sha256sum` + `grep -c MUTATION`（脚本外部独立核对，不看脚本自述）。
恢复：`cp` 快照 + `touch` ⇒ sha 回 `d4be4275…`、标记 = 0；随后重跑哨兵仍绿（上面那条新实现原文
即为恢复**之后**的实跑）。脚本已改为 `( cd "$WT" && cp … )`，与执行时的 cwd 无关。

**教训**：`trap` 里的恢复动作必须**不依赖当前工作目录**；且"脚本说自己会恢复"不算证据，
要在脚本外独立核 sha 与标记。

**第二次（收紧哨兵后重放变异）**：同一条脚本、修复后的 `trap` —— 恢复成功，
脚本外复核 `sha256sum = 93cb2f63…`（= 收紧后快照）、`MUTATION` 标记 = 0。

## 5. 门禁

- 六比较器（`fix-final` 运行 id）：`uenhancer` / `shield` / `bigcloak` / `cloak` / `dejiwonho` / `hangmajinbub`
  各 3 seed（s1/s5/s13）驱动 + 对应 `real_rust_ledger_*` 比较器 —— 全绿。
  台账 `run.json` 的 `run_id` 均为 `fix-final`（防空转侧车命中）。
- 单包门禁（`fmt --check` / `cargo test -p game-svr --lib` / `clippy --all-targets`）在最终 HEAD 的实跑数字见 §5.1。

## 5.1 单包门禁（最终内容实跑）

| 命令（在 `mir2-rs/` 下） | 结果 |
| --- | --- |
| `cargo fmt --all -- --check` | **rc=0** |
| `CARGO_BUILD_JOBS=2 cargo test -p game-svr --lib` | **2157 passed / 0 failed / 73 ignored**，rc=0 |

- `clippy -p game-svr --all-targets -- -D warnings` **不在本卡跑**：合并门由 opc-19 在独占窗口统一跑
  （本节只登记本卡实际执行的与未执行的，不把别人的门记成自己的绿）。
- 起跑条件：`free ≥ 7G`（实测 7512 MB 时起，车道 2 条）；`--all-targets` 需 ≥9G，本卡未起。

## 6. 本卡**未**做的事（与相邻卡的分界）

- **实体镜像**（recalc 后把 `sg.hit_speed` 写回实体）与**站点时序**（657 编码发生在 recalc 之前）：
  本卡既不做也不依赖，归属 `docs/issues/openmir2-recalc-entity-mirror-gap.md`
  （16 个 `recalc_session_ability` 调用点里 6 处无镜像）。
- **rig intake**（golden `Fixture.hit_speed` 同时写会话与实体）：master golden 无该键 ⇒ 现在不需要；
  随 `b3844fd22` 的旋钮落地时一并做。
- 未改任何清单 `status`；未动其它站点。

## 7. 禁区自证

- 只改 `mir2-rs/bins/game-svr/src/lib.rs`（两个 spell 站点 + 一颗哨兵）与本卡。
- 未改 golden、未改比较器、未改任何清单/看板状态。
