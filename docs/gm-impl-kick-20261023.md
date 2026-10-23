# GM `@VV踢人`（`KickHumanCommand`）实现（T-214）

- 卡：`openmir2-gm-impl-kick`（tier=R；父卡 `openmir2-gm-reachability-registry` 的 `script-reachable` 项）
- 分支：`card/openmir2-gm-impl-kick`，base master `113f0c36e`
- 作者：`openmir2-worker-24`（返工：`openmir2-worker-65`，按 reviewer-3 `61b952f9`）；worktree `OpenMir2-wt-cronclaim` / `wt-worker-65`

## 1. oracle 逐行（`src/Modules/GameCommand/Commands/KickHumanCommand.cs:14-37`）

| 行 | 代码 | 语义 |
| --- | --- | --- |
| `:16` | `if (@params == null) return;` | **死分支**：`GameCommand.Handle` 的 `case 2` 传 `Split(' ')`（不返回 null） |
| `:20` | `sHumName = @params.Length > 0 ? @params[0] : "";` | 目标名 |
| `:21-24` | `if (string.IsNullOrEmpty(sHumName)) return;` | **静默 return**（无帮助文本、无帧） |
| `:25` | `WorldEngine.GetPlayObject(sHumName)` | `WorldServer.cs:937-954`：`ChrName` OrdinalIgnoreCase 命中首个、且 `!Ghost`（第三门 `IsPasswordLocked && ObMode && AdminMode` **对非管理员恒真**——只有「改密码中 + ObMode + AdminMode」的管理员会差异；与既有 `play_by_name` 同口径未建模） |
| `:26-31` | `mIPlayerActor.BoKickFlag = true; BoEmergencyClose = true;` | 只置标志（两行 `m_boPlayOffLine/m_boNotOnlineAddExp` 被注释）；**命令自身零回帧** |
| `:32-38` | `SysMsg("{0} 现在不在线，或在其它服务器上!!!", Red, Hint)` | 离线支 |

落地（`PlayObject.Message.cs:367-378`）：`BoEmergencyClose || BoKickFlag || BoSoftClose` ⇒ `MakeGhost()`；
其中 `BoKickFlag` ⇒ 先 `SendDefMessage(SM_OUTOFCONNECTION, 0, 0, 0, 0)`（header-only 528）。
`BoSoftClose` 支的 104/SS_208 不适用（本命令不置 `BoSoftClose`）。

## 2. 实现

- `GmSession.pending_kick: Option<String>`（与 `pending_entity` 同法：命令体纯函数、`GmWorld` 只读
  ⇒ 目标名进会话缓冲），`execute()` 每次清空；
- `Imp::KickHuman` + `kick_human(parameters, sess, world)`：空名 ⇒ `no_frames(NOTE_KICK_HUMAN_EMPTY_NAME)`；
  未注入世界 ⇒ `no_frames(NOTE_NO_WORLD)`（不伪造「不在线」）；`play_by_name` 命中 ⇒
  `pending_kick = Some(entity.name.clone())` + 零帧；未命中 ⇒ 一条 Red 文案；
  **命中时必须存实体规范名**（复审 important-1）：`play_by_name` 是 OrdinalIgnoreCase，而宿主
  `online_take` 用 `e.chr_name == chr` 精确匹配 ⇒ 存命令行输入的大小写（`@VV踢人 ViCtIm`）会
  「命令说命中、宿主在在线表里找不到」（只留 WRN、不发 528）；oracle 是直接置目标对象标志，
  与输入大小写无关；
- 宿主 `collect_gm_kick_pending(&mut ground_sessions, &mut kick_pending)`：在 sim 循环的
  deal-cancel / `drain_kicks` **之前**把缓冲并入本 tick 的 `kick_pending`，落地复用既有
  `drain_kicks`（528 → `KickUser` → 存档 → 清槽），与「重复进世界踢旧」同一支；
- `gm_registry.json` 重算：Implemented 98 → 99、Unimplemented 87 → 86，`桃源-N109.txt:49` /
  月卡系统 两处 script 证据翻为 implemented。

## 3. 用例（7）与牙（2）

用例：

1. `kick_human_is_wired_in_the_command_table`（imp / perm 10 / params 2）。
2. `kick_human_empty_name_is_silent_zero_frames`（`@VV踢人`、`@VV踢人  `）。
3. `kick_human_offline_target_reports_not_online`（含大小写不敏感命中）。
4. `kick_human_online_target_sets_pending_kick_without_frames`（含 Ghost ⇒ 不在线支）。
5. `kick_human_without_world_is_registered_no_world`。
6. `kick_human_script_gmexecute_path_sets_pending_kick`（`NpcPlayer` GMEXECUTE 跳线，部署
   `桃源-N109.txt:49` 同形）。
7. `gm_pending_kick_is_collected_and_landed_with_sm_outofconnection`（宿主收口：`collect` →
   `drain_kicks` ⇒ 恰好一条 528、在线槽清空、缓冲一次性/幂等）。
8. `gm_pending_kick_canonicalizes_mixed_case_target_and_lands_528`（复审 important-1 回归：
   `@VV踢人 ViCtIm` → 缓冲存 `victim` → `collect` → `drain_kicks` ⇒ 恰好一条 528、在线槽清空）。

用例 3 另含 **GBK 字节断言**（复审 minor）：`nobody 现在不在线，或在其它服务器上!!!` 的
`crate::downlink::gbk()` 输出逐字节（ASCII `nobody ` + 15 个 GBK 双字节字 + `0x21 0x21 0x21`）。

牙（实做并还原；牙的**基线** sha256：`gmcmd.rs` `9326e2dd9be2e1d2…`、`lib.rs` 见 §4，还原后与基线逐字节一致）：

| 牙 | 改动 | 结果 |
| --- | --- | --- |
| 1 | 命令体不写 `pending_kick` | 用例 3/4/6 红 |
| 2 | `collect_gm_kick_pending` 只读不 `take` | 用例 7 红 |
| 3 | 命中时改回存输入串 `name.to_string()`（复审 important-1） | 用例 8 红 |
| 4 | 把离线文案句末 `!!!` 改成 `。` | 用例 3 红（文本 + GBK 字节双断言） |

## 4. 门禁（本 head）

```
cargo fmt --all -- --check                              OK
cargo clippy --workspace --all-targets -- -D warnings   OK
cargo test --workspace                                  141 个测试二进制 / 0 FAILED
cargo run -p msgcodegen -- --check                      604 条一致
whitelist_anchor_gate.py check                          506 条锚 ALL PASS（含 T-214）
m3_whitelist_registry_audit.py audit                    GREEN
t_reservation_gate.py check                             GREEN
gm_registry.py build && gate && selftest                GREEN（Implemented 99 / Unimplemented 86）
```

## 5. 残余 / 窗口项（R，未跑）

- 真机双栈窗口未跑（T-214）。**判据按复审 important-2 改写成「脚本形态」与「客户端直呼」两条**：
  - **(a) 脚本形态（部署真实可达路径）**：`ActionOfGmExecute` 直接 `string.Format` 拼
    `sParam1..6`、**不调** `GetLineVariableText`（`ExecutionProcessingSys.cs:4933`），
    且 T-165 已登记「`$USERNAME`/`$STR(S1)` 在动作参数里原样」⇒ 部署脚本
    `桃源-N109.txt:49 GMEXECUTE VV踢人 <$STR(S6)>`、`月卡系统.txt:28 … <$USERNAME>` 传的是
    **字面量**。真机应为：两侧**各恰好一条** Red/Hint
    `「<$STR(S6)>」 现在不在线，或在其它服务器上!!!`（另一处为 `「<$USERNAME>」 …`），
    **无 528、无离场**。⚠️ 按旧判据（「目标恰好一条 528」）跑，两侧帧相等会**误报通过**，
    但它验证的是「不在线」分支，不是踢人。
  - **(b) 客户端直呼（验踢人本身）**：另设一条 Permission ≥ 10 的 GM 会话，由客户端直接发
    `@VV踢人 <真实在线名>`（**不走脚本**；自行起栈时必须在窗口记录里写明该权限如何获得）。
    断言：目标恰好一条 `SM_OUTOFCONNECTION(528)`（header-only）**先于**离场（MakeGhost）、
    说话者零额外帧；踢前 `SaveHumanRcd` 落库（坐标/物品与踢前一致）。
  - **(c)** 空名 ⇒ 两侧零帧；**(d)** 混合大小写目标（`ViCtIm`）⇒ 528（构造级已由用例 8 锤定，
    窗口可顺带覆盖）。
- 前置：脚本触发需要商人路 `GMEXECUTE` 跳线（卡 `openmir2-gm-script-exec-seams`，head
  `c0a55368f`，needs-review）先合并；`@VV踢人` 的客户端 `@` 命令被权限门挡（现网 Permission 0）。
- 未建模（与既有命令同口径，均已在 T-214 行登记）：`GetPlayObject` 的第三门
  （`IsPasswordLocked && ObMode && AdminMode`，**对非管理员恒真**）；`BoEmergencyClose` 在
  `BoKickFlag` 支只参与 `MakeGhost` 的或条件，落地效果与 `drain_kicks` 相同。
- 未起栈、未占端口、未连库。

## 6. 残留物清单

- `E:/tmp/w24-k.sha`（牙的 sha256 基线）。
