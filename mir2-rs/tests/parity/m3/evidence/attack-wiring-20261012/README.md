# `openmir2-m3-attack-magic-wiring`（第一片：攻击派发/回帧）证据

- 卡片：`openmir2-m3-attack-magic-wiring`（issue `31322991`）；owner 排期 = T-28 之后。
- 工作树：`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\m2-svr`（分支 `worktree-m2-gamesvr-host`）
- base：`refs/heads/master` = **`376ea004`**（T-28 片已 approve `127d67b1` 并合入；本分支与它同一提交，攻击片叠在其上）
- 身份：`openmir2-worker-11`。**不 merge、不自签 review。**

## 1. 本片做了什么（写集 `mir2-rs/bins/game-svr/**`，未动 `crates/**`）

| 文件 | 内容 |
| --- | --- |
| `bins/game-svr/src/attackwire.rs`（新增） | 9 条 `CM_*HIT` 的纯函数接缝：`hit_mode`、`at_self`、`front_position`、`GetHitMode`→`SM_*` 两级映射（`rm_action_ident`/`sm_ident_for_rm`/`action_sm_ident`）；7 条单测 |
| `bins/game-svr/src/smframe.rs` | 新增 `sm_action_hit`（`SM_HIT` 族动作帧，12B 头无尾）、`sm_struck`（头 + 16B `MessageBodyWL` 尾）、`sm_death`（头 + 8B `CharDesc` 尾） |
| `bins/game-svr/src/main.rs` | sim 循环新增 9 条 `CM_*HIT` 派发 → `handle_hit_cmd`；tick 后新增 `drain_combat_downlink`（受击/死亡下行） |
| `bins/game-svr/src/lib.rs` | `pub mod attackwire;` |

### oracle 逐条行号

- 入帧映射（与移动同段）：`src/GameSrv/Word/WorldServer.cs:835-844`
  `SendMsg(playObject, Ident, Tag, LoWord(Recog), HiWord(Recog), 0)`
  ⇒ `nX=LoWord(Recog)`、`nY=HiWord(Recog)`、`nDir=(byte)Tag`。
- 派发：`src/M2Server/Player/PlayObject.Message.cs:1221-1230`（9 条 ident → `ClientHitXY`）；
  成功 ⇒ `ActionTick=GetTickCount(); SendSocket(M2Share.GetGoodTick)`；失败且 `delayTime==0`
  ⇒ `SendRefMsg(RM_MOVEFAIL,0,0,0,0,"")`。
- `ClientHitXY`：`src/M2Server/Player/PlayObject.Attack.cs:823-953`
  - `Death || STONE` 守卫：`:835`；
  - 成功门 `nX==CurrX && nY==CurrY`：`:884`；
  - `switch(wIdent)` 的 9 个 `AttackDir(null, mode, nDir)`：`:907-939`；
  - 成功支体力扣减 `PerHealth -= 2; PerSpell -= 2`：`:943-947`。
- `GetHitMode`（mode→`RM_*`）：`PlayObject.Attack.cs:78-120`；
  观察者 `RM_*`→`SM_*`：`PlayObject.Message.cs:1433-1521`。
- 取目标 `GetPoseCreate`：`src/M2Server/Actor/BaseObject.cs:1253-1261`；
  `GetFrontPosition`：`BaseObject.cs:977-1030`；`GetMovingObject(x,y,true)`：`src/M2Server/Maps/Envirnoment.cs:1203-1240`。
- 动作帧：`BaseObject.Attack.cs:173-176` `SendAttackMsg` → `SendRefMsg` ⇒ 广播附近观察者。
- 受击帧：`BaseObject.Attack.cs:61-64`（`Race>Play` ⇒ `SendMsg(target, RM_STRUCK, …)`）
  → `PlayObject.Message.cs:1562-1628`（`SM_STRUCK`，尾 = `MessageBodyWL`）。
- 死亡帧：`BaseObject.Base.cs:351` `SendRefMsg(RM_DEATH, Dir, CurrX, CurrY, 1, "")`
  → 观察者 `SM_NOWDEATH(34)`（`PlayObject.Message.cs:1630-1655`）。

## 2. 门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m2svr-target`，全 `--offline`）

| 门禁 | 结果 | 证据 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | rc=0 | `gates-fmt.txt` |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 | `gates-clippy.txt` |
| `cargo test --workspace` | **73 suites / 596 passed / 0 failed / 16 ignored**（rc=0） | `gates-test-workspace.txt` |
| `cargo run -p msgcodegen -- --check` | rc=0，604 条 | `gates-msgcodegen.txt` |
| `cargo test -p game-svr` ×3 | 各 **222 passed**（rc=0，可重复） | `gamesvr-3run-1..3.txt` |

## 3. 新用例与红检

**新用例（7 条）**
- `attackwire::tests`：`nine_hit_idents_and_modes` / `hit_mode_conditions_fall_back_to_rm_hit` /
  `action_sm_ident_matches_observer_table` / `front_position_clamps_at_border_like_oracle` /
  `at_self_only_when_same_tile`；
- 进程级 `tests::hit_cmd_dispatches_ack_action_frame_and_world_attack`：目标≠自身不回 ack/不发帧；
  目标=自身 ⇒ 观察者收 `SM_HIT`（12B 无尾）+ 自己收 `+GD` + `PerHealth -= 2` + tick 后攻击落到正上方怪；
  `CM_HEAVYHIT` ⇒ `SM_HEAVYHIT`；`Death` 守卫 ⇒ 不发帧/不回 ack。
- 进程级 `tests::combat_downlink_sends_struck_and_nowdeath`：`rec.attacks` ⇒ 受击者收 `SM_STRUCK`
  （头 `HP/MaxHP/伤害` + 16B `MessageBodyWL` 尾）、世界 HP 同步回会话 `WAbil`；`rec.deaths` ⇒
  同屏（**含死者自己**）收 `SM_NOWDEATH`（头 `X/Y/Dir` + 8B `CharDesc` 尾）。

**可判红（4 条；改坏 ⇒ 必红，原始输出见 `red-*.txt`）**

| # | 改坏点 | 结果 |
| --- | --- | --- |
| 1 | `attackwire::at_self` 恒 `true` | `hit_cmd_*` FAILED（`red-1-*`） |
| 2 | `hit_mode` 把 `CM_HEAVYHIT` 映射成 `0` | `hit_cmd_*` FAILED：left 14 / right 15（`red-2-*`） |
| 3 | `front_position` 的 `DIR_UP` 支改 `(x, y-2)` | `hit_cmd_*` FAILED（取不到正上方怪）（`red-3-*`） |
| 4 | `sm_struck` 头 `hp/max_hp` 互换 | `combat_downlink_*` FAILED：left (1,100,0,7) / right (1,0,100,7)（`red-4-*`） |

改后均 `cp` 还原、`git status` 干净。

## 4. 端口 / 残留

- **本片不起栈、不占端口**：`ports-preflight.txt` 实拍 = 仅共享 MySQL `3306` LISTENING，
  5000/5100/5500/5600/5700/6000/7000/7100/7200 全 FREE。
- 未动 `crates/**`、`src/**`、`MirClient`、表结构/脚本语法/数据格式、`tests/parity/m3/**` 夹具、`whitelist.md`。

## 5. 登记（本片未接，不静默）

- **限速/超速**（`CheckActionStatus` + `AttackTick`/`HitIntervalTime`/`HitSpeed`/`AttackCount`/
  `OverSpeedCount`/`BoEmergencyClose` + `!boLateDelivery` 支）：未实现；本片把攻击视作直通（等价
  `boLateDelivery`）。oracle 在限速不命中时改发 `RM_MOVEFAIL` 或延时重投，本片恒回 `+GD`。
- **`RM_MOVEFAIL` 编码器**：未接（`movewire` 模块头同源登记）⇒ 失败支只记日志。
- **`STONE`**（`StatusTimeArr[PoisonState.STONE]`）：状态位面未移植 ⇒ `Death` 已接、`STONE` 未接。
- **`GetHitMode` 的技能/装备态**（`PowerHit`/`FireHitSkill`/`MagicArr[SKILL_ERGUM|BANWOL|CROSSMOON]`）：
  技能层未移植 ⇒ 条件恒假，模式 3/4/5/7/8 回落 `RM_HIT`（与「无技能角色」的 oracle 结果一致）。
- **目标筛选**：沿用 `GetMovingObject` 的 `!Ghost && !Death`，**不**做 `IsProperTarget`/`InSafeZone`/
  攻击模式/PK 红名过滤；`GetMovingObject` 的格子 `ObjList` 次序用**实体槽位序**近似。
- **怪物帧面**：`SM_STRUCK` 只发给有连接的玩家目标；怪物无连接 ⇒ 不发（oracle 同）；怪物 death 帧的
  `CharDesc`（`GetFeature`/`CharStatus`）未建模 ⇒ 跳过并 WRN。
- **`HealthTick`/`SpellTick`**（`:943-947` 的 `-30/-100`）：体力/魔法点数面未移植（登记）；
  `PerHealth/PerSpell -= 2` 已按 `byte` 无符号回绕作用于 `recover::Recovery`。
- **改技能态后的动作帧**（`RM_SPELL2`/`RM_LONGHIT`/`RM_WIDEHIT`/`RM_FIREHIT`/`RM_CRSHIT`）：
  映射与编码已就绪（`sm_action_hit` 通用），等技能层接上即可生效。

## 6. 本卡下一步（第二片：魔法派发）

- `CM_SPELL(3017)` 派发（入帧映射见 `WorldServer.cs:783-790`：`wParam=Tag`、`nParam1/nParam2=Lo/Hi(Recog)`、
  `nParam3=MakeLong(Param,Series)`）→ `ClientSpellXY` 等价入口（`PlayObject.Message.cs:1339-1400`）；
- `CM_MAGICKEYCHANGE(1008)` → `ClientChangeMagicKey`（`PlayObject.Operate.cs:774`）；
- `LateDelivery`/`delayTime`（`MagicAttackInterval`）与技能效果面（`crates/world/src/magic.rs` 的
  纯函数已由 m3cs 离线片建立）——本片**未动**，留第二片。
