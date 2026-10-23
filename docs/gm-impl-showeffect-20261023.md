# GM `@ShowEffect`（`ShowEffectCommand`）空实现（T-215）

- 卡：`openmir2-gm-impl-showeffect`（tier=R；父卡 `openmir2-gm-reachability-registry` 的 `script-reachable` 项）
- 分支：`card/openmir2-gm-impl-showeffect`，base master `ff4dc1e80`
- 作者：`openmir2-worker-24`；worktree `OpenMir2-wt-cronclaim`

## 1. oracle 命令体（`src/Modules/GameCommand/Commands/ShowEffectCommand.cs:12-71`）

| 行 | 内容 |
| --- | --- |
| `:8` | `[Command("ShowEffect", "播放特效", 10)]` —— 3 参构造 ⇒ **无 `Help`**；`Command.conf` 无 `[Command] ShowEffect` 覆盖 ⇒ 注册名保持英文 `ShowEffect`，部署脚本写 `showeffect` 靠 `OrdinalIgnoreCase` 命中 |
| `:14-17` | `if (@params == null) return;` —— **死分支**（`case 2` 传 `Split(' ')`） |
| `:18-19` | 取 `sEffect`/`sTime`，**之后都没被使用** |
| `:20-70` | **整段注释**：参数校验、79-85 烟花（`TFlowerEvent`）、87/89/90 `RM_MYSHOW`、`EventManager.AddEvent` |

⇒ online 行为 = **任何参数都零帧、零副作用**（含部署脚本 `Gmexecute showeffect 78/79/80`）。
orchestrator 在卡上的订正（`ea48be161`）明确要求按实际执行行收敛、不得把注释掉的广播/特效当行为。

## 2. 实现

- `Imp::ShowEffect` + `show_effect() -> no_frames(NOTE_SHOW_EFFECT_ALL_COMMENTED)`（最小 no-op）；
- 表项 `Imp::Unimplemented → Imp::ShowEffect`（`perm_min 10` / `params 2` 不变）；
- `gm_registry.json` 重算：Implemented 98 → 99、Unimplemented 87 → 86，ShowEffect 的 54 处
  script evidence 翻为 implemented。

**没有**做的事（有意）：不实现烟花 `TFlowerEvent`、不发 `RM_MYSHOW`、不建事件——这些都在注释里。

## 3. 用例（4）与牙（2）

1. `show_effect_is_wired_in_the_command_table`（查表用小写 `showeffect` 命中，`def.name == "ShowEffect"`）。
2. `show_effect_is_zero_frames_for_every_parameter`（`@ShowEffect` / `78` / `78 4000` / 小写 `showeffect 87` / 乱参：全部零帧 + 登记 + 权限不变）。
3. `show_effect_permission_gate_blocks_body`（perm 9 ⇒ `权限不够!!!`；10 ⇒ 零帧）。
4. `show_effect_script_gmexecute_path_is_zero_frames`（`NpcPlayer` GMEXECUTE，部署 `@StdModeFunc14` 同形：零帧 + 登记 + 抬权还原）。

牙（实做并还原，`gmcmd.rs` 与编辑前 sha256 一致）：

| 牙 | 改动 | 结果 |
| --- | --- | --- |
| 1 | 表项回退 `Imp::Unimplemented` | 表项用例红 |
| 2 | 空实现改成回一条帧（`red("TOOTH-2")`） | 用例 2/3/4 红 |

## 4. 门禁（本 head）

```
cargo fmt --all -- --check                              OK
cargo clippy --workspace --all-targets -- -D warnings   OK
cargo test --workspace                                  141 个测试二进制 / 0 FAILED
cargo run -p msgcodegen -- --check                      604 条一致
whitelist_anchor_gate.py check                          509 条锚 ALL PASS（含 T-215）
m3_whitelist_registry_audit.py audit                    GREEN
t_reservation_gate.py check                             GREEN
gm_registry.py build && gate && selftest                GREEN（Implemented 99 / Unimplemented 86）
```

取号插曲：首次 `next_t.py` 因远端快照 `ls-remote` 瞬时失败被 fail-closed 拒绝（这是设计行为），
重试一次即取得 **T-215**（`journal=on remote=fresh`）；未使用 `--allow-stale`。

## 5. 残余 / 窗口项（R，未跑）

- 真机双栈窗口未跑（T-215）：`@StdModeFunc14`（`Gmexecute showeffect 78/79/80`）、
  `比奇国王-0122`、`月老-M101` 的调用点两侧**零帧**且状态/DB 不变；`87/89/90` 对照支同样零帧。
- 前置：脚本触发需商人路 `GMEXECUTE` 跳线（`openmir2-gm-script-exec-seams`，head `c0a55368f`，
  needs-review）先合并。
- 未起栈、未占端口、未连库。

## 6. 残留物清单

- `E:/tmp/w24-se.sha`（牙的 sha256 基线）。
