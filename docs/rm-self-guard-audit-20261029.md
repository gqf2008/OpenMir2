# RM_* 自身守卫审计（A1）

- 卡：`openmir2-rm-self-guard-all-idents`（R，window_required=true）
- 基线：master `95b2e9e36`（`openmir2-rust-extra-ident14-hit` 的 `e8de0c93` 已合入，含 `678432c5` 的 `drop_self`）
- 方法：逐条读 `src/M2Server/Player/PlayObject.Message.cs` 的 `case Messages.RM_*` 块，记录
  `if (processMsg.ActorId != ActorId)` 是否存在与 `MakeMessage` 的 SM_*；再对照 Rust 的实际出帧路
  径（宿主 `broadcast_ref_msg*` / 世界 `send_ref_msg` → `drain_monster_ref_msgs`），核接收者集合是否
  含发起者。

## 1. 对照表

| RM_* | C# 行（守卫） | SM_* | Rust 出帧路径 | 发起者自己是否入帧 | 现状 |
|---|---|---|---|---|---|
| `RM_WALK` | 1403-1412（有） | `SM_WALK` | 宿主移动下行 `broadcast_ref_msg`（`lib.rs:10332`）→ `broadcast_ref_msg_opt`（`:14541`，`include_self=false`） | 否 | 已守 |
| `RM_HORSERUN` | 1413-1422（有） | `SM_HORSERUN` | 同上（`downlink::move_frame`） | 否 | 已守 |
| `RM_RUN` | 1423-1432（有，另 `baseObject != null`） | `SM_RUN` | 同上 | 否 | 已守 |
| `RM_HIT` | 1433-1439（有） | `SM_HIT` | ① 宿主动作帧 `broadcast_ref_msg`（`lib.rs:10729`，排除自己）；② **世界/drain** `drain_monster_ref_msgs`（`:19991`）按发帧快照 `recipients` 投递；③ 固定 NPC/商人 RM_HIT 走 `drain_npc_merchant_hits`（`:15635`）→ `broadcast_ref_msg`（排除自己） | ① 否；② **是**（`send_ref_msg` 用 `NO_SENDER_EXCLUSION`，`monai.rs:885/931`）；③ 否（sender 是 NPC/商人，不在 online） | ② 曾漏守，已由 `openmir2-rust-extra-ident14-hit` `e8de0c93`（含 `678432c5` 的 `drop_self`）修复并随 master `95b2e9e36` 合入 |
| `RM_HEAVYHIT` | 1440-1446（有） | `SM_HEAVYHIT` | 宿主动作帧 `attackwire::action_sm_ident`（`lib.rs:10717-10729`） | 否 | 已守（世界无 producer） |
| `RM_BIGHIT` | 1447-1453（有） | `SM_BIGHIT` | 同上 | 否 | 已守 |
| `RM_SPELL` | 1454-1460（有） | `SM_SPELL` | 世界 `drain_monster_ref_msgs`（`:20095`）；唯一 producer `FireballMonster`（`monai.rs:6828`）经 `monai::send_ref_msg`（`NO_SENDER_EXCLUSION = i32::MIN`，`send_ref_msg_targets_at` 的 `id == sender_id` 排除**不生效**，格扫含 sender 自己） | sender 是怪物（不在 online）⇒ 无自帧；注意这是「sender 不是玩家」而非「收件人集不含 sender」 | 不适用（当前无玩家 sender；若将来加玩家施法必须加同守卫） |
| `RM_SPELL2` | 1461-1467（有） | `SM_POWERHIT` | 宿主动作帧（`attackwire` 3 支；技能层未接 ⇒ 当前回落 `RM_HIT`） | 否 | 已守 / 未触发 |
| `RM_MOVEFAIL` | 1468-1474（**无**） | `SM_MOVEFAIL` | 未接（`movewire`/`attackwire` 模块头登记） | —（C# 不守卫） | 不适用；**不得加守卫**（A3 负例） |
| `RM_LONGHIT` | 1475-1481（有） | `SM_LONGHIT` | 宿主动作帧（4 支；`ergum` 未接 ⇒ 当前 `RM_HIT`） | 否 | 已守 / 未触发 |
| `RM_WIDEHIT` | 1482-1488（有） | `SM_WIDEHIT` | 宿主动作帧（5 支；`banwol` 未接） | 否 | 已守 / 未触发 |
| `RM_FIREHIT` | 1489-1495（有） | `SM_FIREHIT` | 宿主动作帧（7 支；`fire_hit_skill` 未接） | 否 | 已守 / 未触发 |
| `RM_CRSHIT` | 1496-1502（有） | `SM_CRSHIT` | 宿主动作帧（8 支；`crossmoon` 未接） | 否 | 已守 / 未触发 |
| `RM_41` | 1503-1509（有） | `SM_41` | 仅协议常量（`protocol/src/messages.rs:551`），无 producer | — | 不适用（未实现） |
| `RM_TWINHIT` | 1510-1516（有） | `SM_TWINHIT` | 宿主动作帧（9 支） | 否 | 已守 |
| `RM_43` | 1517-1523（有） | `SM_43` | 仅协议常量（`messages.rs:553`），无 producer | — | 不适用（未实现） |
| `RM_TURN` | 1524-1529（TURN 支有自身守卫） | `SM_TURN` | 玩家转身 `broadcast_ref_msg`（`lib.rs:10248-10249`，排除自己）；城堡门 `drain_monster_ref_msgs`（`:19939`，sender 是结构、无会话）；商人 `broadcast_ref_msg`（`:15615`） | 否 | 已守 / 不适用 |
| `RM_PUSH` | 1525/1533（复合条件对 PUSH **恒真**） | `SM_BACKSTEP` | W2 动作 `broadcast_ref_msg_with_self`（`lib.rs:15913`） | **是**（C# 也恒发） | 已守（恒含自己） |
| `RM_RUSH` | 1526/1535（恒真） | `SM_RUSH` | `broadcast_ref_msg_with_self`（`:15927`） | **是**（C# 也恒发） | 已守（恒含自己） |
| `RM_RUSHKUNG` | 1527/1538（恒真） | `SM_RUSHKUNG` | `broadcast_ref_msg_with_self`（`:15932`） | **是**（C# 也恒发） | 已守（恒含自己） |

**订正（reviewer-21 R21-A1-1/2/3）**：① `RM_HIT` 补第三条 NPC/商人路径（`drain_npc_merchant_hits` → `broadcast_ref_msg`，排除自己）；② `RM_SPELL` 机制改为`monai::send_ref_msg` + `NO_SENDER_EXCLUSION`（自排除不生效、格扫含 sender；sender 非玩家故无自帧，若将来玩家施法必须加守卫）；③ A3 负例补 `RM_MAGICFIRE`/`RM_NOWDEATH`。

## 2. 结论

1. **当前唯一漏守是 `RM_HIT` 的世界/drain 路径**：宿主动作帧路径（`lib.rs:10729`）与固定
   NPC/商人路径（`drain_npc_merchant_hits`，`:15635` → `broadcast_ref_msg`）都已经排除自己；
   但世界 `send_ref_msg` 的接收者快照按 oracle 格扫包含 sender（`NO_SENDER_EXCLUSION`），
   `drain_monster_ref_msgs` 原来没有去掉这一份 ⇒ run32 玩家收到自己的 `SM_HIT(14)`。该修复
   已由 `openmir2-rust-extra-ident14-hit` `678432c5` 完成（`drop_self`），并随 `e8de0c93` /
   master `95b2e9e36` 合入；本卡 A2 在同一函数上补路径级牙。
2. **其余 C# 有守卫的 ident 当前无生产性漏守**：动作族走宿主 `broadcast_ref_msg`（排除自己）；
   `RM_SPELL` 唯一 sender 是怪物；`RM_TURN` 的玩家路径也走 `broadcast_ref_msg`；`RM_PUSH/RUSH/
   RUSHKUNG` 的 C# 复合条件恒真（发起者也收），Rust 用 `broadcast_ref_msg_with_self` 与之一致。
3. **不能加守卫的**：`RM_MOVEFAIL`（C# 无 `ActorId != ActorId`）、`RM_MAGICFIRE`（C#
   1788-1793 无守卫，Rust drain 注释也写明「无自己守卫」）、`RM_NOWDEATH`（C# 无守卫，Rust
   含自己）；`RM_PUSH/RUSH/RUSHKUNG` 的自收是 oracle 语义，不得过滤。
4. 未实现/无 producer 的 `RM_41`、`RM_43`、以及技能未接时的 `RM_SPELL2/LONGHIT/WIDEHIT/
   FIREHIT/CRSHIT` 只登记；将来若世界侧新增 `send_ref_msg` producer，必须同时加自身过滤。

## 3. 后续（本卡 A2/A4/A5；依赖已随 `e8de0c93`/`95b2e9e` 合入，窗口结束后执行）

**A2 已交（本提交，`mir2-rs/bins/game-svr/src/lib.rs` 测试）**：
`rm_path_teeth_broadcast_walk_turn_excludes_self`（① WALK/RUN/HORSERUN/TURN 自己 0 / 他人 1）、
`rm_path_teeth_broadcast_push_rush_includes_self`（② PUSH/RUSH/RUSHKUNG 自己 1 / 他人 1）、
`rm_path_teeth_drain_spell_monster_sender_no_self`（③ RM_SPELL 怪物 sender 无自帧）、
`rm_path_teeth_drain_magicfire_player_sender_not_filtered`（④ RM_MAGICFIRE 玩家 sender 不过滤）。
另有既有牙：RM_HIT 自身守卫 + 多收件人（`e8de0c93`）与 NOWDEATH 含自己（既有用例）；`RM_MOVEFAIL`
编码器未接（`movewire` 模块头登记），无从过滤。

- A2 牙（路径级，不重复写 14 卡已有牙）：
  1. 宿主动作帧/移动帧：sender 自己 0 帧、附近他人 1 帧（`WALK`/`RUN`/`HORSERUN`/`TURN`）；
  2. `PUSH/RUSH/RUSHKUNG`：sender 自己 1 帧（C# 恒真）；
  3. `RM_MOVEFAIL` / `RM_MAGICFIRE` / `RM_NOWDEATH`：不得因本卡被过滤（C# 无守卫；负例）；
  4. `RM_SPELL`：若构造玩家 sender，必须被守卫；当前怪物 sender 0 自帧（证明性负例）。
- A4 真机：至少一个重击/技能场景在同一 master 后代复跑；A5 全 workspace fmt/clippy/test、
  tool gate、merge_tree、diff --check 绿。真机未跑前不 closed。
