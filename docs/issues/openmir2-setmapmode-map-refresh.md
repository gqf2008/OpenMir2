# openmir2-setmapmode-map-refresh

> 状态：**已立卡（2026-11-20，opc-07 派工）**——真差异，修；但触发面是**脚本动作**，不是 GM 命令。
> 来源：opc-07 记录的未落卡疑点（「`@SetMapMode` 后 `world.maps` 不刷新」）。

## 疑点辨析（先判真伪）

**GM 命令 `@SetMapMode` 两侧都是 no-op，不构成差异（疑点在 GM 面证伪）**：

- oracle：`src/Modules/GameCommand/Commands/SetMapModeCommamd.cs` 的 `Execute` 在权限门之后
  **整个分支体被注释掉**，不写任何 Flag。
- Rust：`gmcmd.rs` 的 `CommandDef { name: "SetMapMode", imp: Imp::Unimplemented }` ⇒
  `GmResult { found: true, msgs/frames 皆空, 无副作用 }`。

⇒ 同一操作序列（GM 命令）下两侧地图 Flag 都不变。

**活路径是脚本动作 `SETMPAMODE`（`ExecutionCode.SetMapMode = 216`），这里确有差异**：

- oracle：`ExecutionProcessingSys.cs` 的 `ActionOfSetMapMode` 在 `FindMap` 命中后**当场**写
  同一个 `envir.Flag.*`（SAFE/DARK/FIGHT/FIGHT3/EXPRATE/NEEDHOLE/NODROPITEM… 共 30 余支）；
  所有读者共享同一份 `Envir.Flag`，立即生效。
- Rust：脚本动作只把写记录进 `NpcPlayer` 队列，宿主 `flush_map_mode_writes` 落到
  `MapRegistry`（`Envir.Flag` 的运行时镜像）；但 `world.maps` / `world.map_exp_rate` 是
  **启动时**从注册表灌入的独立副本，运行时**不回填**。
  ⇒ 宿主侧读者读注册表（新值），世界侧读者读旧副本。

## 判据（直接状态读；不靠日志文本、不靠帧推断）

用例 `setmpamode_fight_refreshes_world_map_flags_and_world_consumers`
（`T142Rig` 跑真实脚本动作 → 宿主落库，读注册表 / `world.maps` / `World::set_pk_flag`）：

- **修复前**（`card/setmapmode-map-refresh` 首跑，rc=101）：
  ```
  panicked at ...: world.maps 必须同步到新值（oracle 只有一份 Envir.Flag）
  test result: FAILED. 0 passed; 1 failed
  ```
  即同一时刻 `registry.flag_of(0).fight_zone == true` 而 `world.maps[0].is_fight_zone() == false`。
- **修复后**：同一动作序列（`FIGHT / NEEDHOLE / NODROPITEM / EXPRATE` 四条脚本写）在**未经过
  下一 tick** 的落库点，宿主侧（`registry.flag_of`）与世界侧（`world.maps` / `map_exp_rate`）
  **逐项相等**（比"最终一致"严，正是 oracle 单对象语义）；且 `World::set_pk_flag` 的战区门
  生效（返回 `None`、不置攻击方 `PvpFlag`）。

## 影响面（世界侧消费者，均读 `world.maps` / `map_exp_rate` 镜像）

| Flag | 世界侧读者（Rust） | oracle 同义读者 | 玩家可感知 |
|---|---|---|---|
| `FightZone`/`Fight3Zone` | `World::set_pk_flag_impl`、`settle_deaths` 的 `!FightZone && !Fight3Zone` | `PlayObject.SetPkFlag`、`BaseObject.DieDropItems` | 和平区 PvP 是否置红名 / 是否掉物品 |
| `NoDropItem` | `settle_deaths` | `BaseObject.DieDropItems` | 死亡是否掉物品 |
| `boNEEDHOLE` | `route_enter` 的落地门 | `PlayObject` 的落地门 | 洞口走位能否通过 |
| `boEXPRATE`/`ExpRate` | `win_exp` | `BaseObject.GetExp` 系 | 该图杀怪经验倍率 |

触发面：NPC/任务脚本里的 `SETMPAMODE` 动作——玩家与含该动作的 NPC 对话即可触发，
**不是 GM 专属**。仅宿主读注册表的其它 Flag（SAFE/DARK/DECHP/…）本来就即时生效，不在本卡范围。

## 修复

- 新增 `sync_map_flags_into_world`：逐图把注册表的 `Envir.Flag` 回填世界镜像，只镜像
  **世界建了存储的四项**（`ExpRate`/`NeedHole`/`FightZone`+`Fight3Zone`/`NoDropItem`），
  与启动灌入用同一组 setter、同一取数；
- 新增 `flush_map_mode_writes_into_world`：**先落注册表、后回填世界**（同一 tick）；
  `sim_loop` 改调它；
- 顺序是语义：先 `flush_map_mode_writes` 后 `sync_map_flags_into_world`，否则回填到的是旧 Flag。

## 牙

1. 删 `sync_map_flags_into_world` 调用 ⇒ 新用例在「世界镜像明确期望值」断言处红；
2. 把 `sync_map_flags_into_world` 挪到 `flush_map_mode_writes` **之前** ⇒ 新用例红
   （回填读到旧 Flag）；
3. 只镜像 FIGHT、其余三项不同步 ⇒ 新用例红：`(is_fight_zone, need_hole, no_drop_item,
   map_exp_rate[0])` 的期望 `(true, true, true, Some(2))` 与实际一次打印（期望/实际逐项可见）。

还原一律 `git checkout -- <path>` + `hash-object == HEAD:path` 断言，且只在本人 worktree 执行。

## 验收

- `cargo fmt/clippy/test` 三项 rc；全量测试的构建窗口由 Leader 排期（2d/3b 占用中）。
- 定点用例红→绿原文；三颗牙红→还原→`hash-object == HEAD:path` 相等。
