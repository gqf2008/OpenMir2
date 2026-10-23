# GM `@MobFireBurn`（`MobFireBurnCommand`）实现（T-216）

- 卡：`openmir2-gm-impl-mobfireburn`（tier=R；父卡 `openmir2-gm-reachability-registry` 的 `script-reachable` 项）
- 分支：`card/openmir2-gm-impl-mobfireburn`，base master `ff4dc1e80`
- 作者：`openmir2-worker-24`（返工：`openmir2-worker-65`，按 reviewer-3 `97ee5978`）；worktree `OpenMir2-wt-cronclaim` / `wt-worker-65`

## 1. oracle 逐行（`src/Modules/GameCommand/Commands/MobFireBurnCommand.cs:17-60`）

| 行 | 语义 |
| --- | --- |
| `:19` | `@params == null` 死分支（`case 2` 传 `Split(' ')`） |
| `:23-28` | 取六参：地图 / X / Y / 光环效果 / 持续时间 / 未知 |
| `:29-33` | `IsNullOrEmpty(sMap) \|\| sMap != "" && sMap[1] == '?'`：空地图 ⇒ 帮助；**单码元地图名 ⇒ `sMap[1]` 抛 `IndexOutOfRangeException` ⇒ 被 `PlayObject.Chat.cs` 吞 ⇒ 零帧**（**构造级分支**；**部署形态不走这条**：`ActionOfGmExecute` 直接拼 `sParam1..6`、不展开 `<$…>`（T-165）⇒ 实参是 `<$CURRENTMAP>` 字面量（13 码元），`StrToInt("<$CURRENTX>",-1)=-1` ⇒ 落 `:43` 的帮助支）；第 2 个 UTF-16 码元为 `?` ⇒ 帮助 |
| `:34-38` | `(short)StrToInt(sX,-1)` / `(short)sY` / `(byte)sType` / `int sTime` / `int sPoint`（unchecked 截断） |
| `:39-42` | `nPoint < 0 ⇒ 1` ⇒ 后面 `nPoint < 0` 恒假；`nType < 0` 对 `byte` 也恒假 |
| `:43-48` | 其余非法 ⇒ 帮助，模板 `"命令格式: {0} {1} {2} {3} {4} {5} {6}"`（无 `@`、单空格、空槽照原样） |
| `:49-58` | `MapMgr.FindMap` 命中 ⇒ 临时改/还原 `PlayerActor.Envir`、`FireBurnEvent` 入 `EventMgr`，**不发帧** |
| `:59` | 未命中 ⇒ `string.Format("地图{0} 不存在", Command.Name, sMap)`——**单占位符** ⇒ 文案是 `地图MobFireBurn 不存在`（地图名被丢弃，BUG-REPLICA） |

名字口径：`Command.conf` 无 `[Command] MobFireBurn` 覆盖 ⇒ 注册名保持英文 `MobFireBurn`，部署写
`MOBFIREBURN` 靠 `OrdinalIgnoreCase` 命中。

## 2. 实现

- `Imp::MobFireBurn` + `mob_fire_burn(parameters, world)`：复用 `split_params` /
  `second_utf16_is_question` / `str_to_int` / `MapRegistry::map_by_name`；
- 命中地图支复用既有 `NOTE_EVENT_MGR`（`FireBurnEvent + EventMgr` 宿主未接，`@FireBurn` 同口径）
  ⇒ 零帧 + 登记；
- `gm_registry.json` 重算：Implemented 98 → 99、Unimplemented 87 → 86。

## 3. 用例（8）与牙（3）

1. `mob_fire_burn_is_wired_in_the_command_table`（小写 `MOBFIREBURN` 命中，name/imp/perm/params）。
2. `mob_fire_burn_empty_map_reports_help_with_trailing_slots`（`命令格式: MobFireBurn` + **6 个尾随空格**
   + **GBK 字节**；`m?` 帮助）。
3. `mob_fire_burn_single_char_map_is_zero_frames_construct_only`（**构造级**：`3 329 329 3 60 0` /
   `0 330 330 5 600 0` ⇒ 零帧 + 越界登记；复审 important-1 已改名并注明**不是部署形态**）。
4. `mob_fire_burn_deployment_literals_report_help`（**部署形态**：`@MOBFIREBURN <$CURRENTMAP>
   <$CURRENTX> <$CURRENTY> 5 600 0` ⇒ **一条红字帮助**，六个字面量原样回填 + **GBK 字节**）。
5. `mob_fire_burn_bad_coords_report_help`（非数字 X；`(short)40000` 截断为负；`(byte)` 死条件对照）。
6. `mob_fire_burn_existing_map_registers_event_and_stays_silent`（`map` 地图命中 ⇒ 零帧 + 事件登记，含缺省 `nPoint`）。
7. `mob_fire_burn_map_not_found_prints_command_name`（`地图MobFireBurn 不存在` + **GBK 字节**；未接注册表 ⇒ 登记零帧）。
8. `mob_fire_burn_script_gmexecute_path_reports_help`（`NpcPlayer` GMEXECUTE，`@StdModeFunc15/16` 同形
   ⇒ **一条红字帮助**，不是零帧）。

牙（实做并还原，`gmcmd.rs` `ec40b0c59444fe40…` 与编辑前一致）：

| 牙 | 改动 | 结果 |
| --- | --- | --- |
| 1 | 删 `sMap[1]` 越界支 | 构造级用例红 |
| 2 | not-found 文案改用地图名 | 用例 7 红（文本 + GBK） |
| 3 | 帮助模板加 `@` 前缀 | 用例 2/4/5/8 红（文本 + GBK） |

## 4. 门禁（本 head）

```
cargo fmt --all -- --check                              OK
cargo clippy --workspace --all-targets -- -D warnings   OK
cargo test --workspace                                  141 个测试二进制 / 0 FAILED
cargo run -p msgcodegen -- --check                      604 条一致
whitelist_anchor_gate.py check                          511 条锚 ALL PASS（含 T-216）
m3_whitelist_registry_audit.py audit                    GREEN
t_reservation_gate.py check                             GREEN
gm_registry.py build && gate && selftest                GREEN（Implemented 99 / Unimplemented 86）
```

## 5. 残余 / 窗口项（R，未跑）

- 真机双栈窗口未跑（T-216）。**判据按复审 important-1 改写**：`ActionOfGmExecute`
  （`ExecutionProcessingSys.cs:4933`）直接拼 `sParam1..6`、不调 `GetLineVariableText`；T-165
  已登记动作参数里的 `$` 变量原样 ⇒ `<$…>` 是**字面量**：
  - ① **部署形态**：`@StdModeFunc15/16`（`Gmexecute MOBFIREBURN <$CURRENTMAP> <$CURRENTX>
    <$CURRENTY> 5 600 0`）⇒ 两侧**各恰好一条**红字帮助
    `命令格式: MobFireBurn <$CURRENTMAP> <$CURRENTX> <$CURRENTY> 5 600 0`——**不是零帧**；
    ⚠️ 按旧判据「两侧零帧」跑会因两侧相等等而**误报通过**。
  - ② 构造级对照（脚本写两字符地图 `aa 10 10 5 600 0`）⇒ 两侧同帧 `地图MobFireBurn 不存在`。
  - ③ 「地图存在 ⇒ 零帧 + C# `EventMgr` 收到 `FireBurnEvent`」**不作脚本形态判据**（到不了该支）；
    若要用须另设客户端直呼/构造判据，并显式登记 Rust 事件队列未接。
- **未接**：`SystemShare.EventMgr` / `FireBurnEvent`（与既有 `@FireBurn`、`@TestFire` 同一欠账，
  已由 `NOTE_EVENT_MGR` 登记）；`PlayerActor.Envir` 的临时切换在 oracle 只为构造事件对象，无帧面。
- 前置：脚本触发需商人路 `GMEXECUTE` 跳线（`openmir2-gm-script-exec-seams`，head `c0a55368f`，
  needs-review）先合并。
- 未起栈、未占端口、未连库。

## 6. 残留物清单

- `E:/tmp/w24-mf.sha`（牙的 sha256 基线）。
