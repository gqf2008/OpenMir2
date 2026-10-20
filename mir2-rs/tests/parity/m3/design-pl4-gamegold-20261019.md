# Lane B 实施简报：PL-4（P22 元宝/游戏点自动增减）

- 日期：2026-10-19；基线：`c2ac5cccb`（master）；性质：**只读车道**（未建 worktree、未改仓库、未起服务、未碰 `E:/MirServer`）。
- 输入：`mir2-rs/tests/parity/m3/plan-run-gap-batches-20261018.md` §1.2/§1.3 PL-4、`…/spike-player-run-state-ownership-20261018.md` §1.2/§4、oracle `src/**`、Rust `mir2-rs/**`。
- 行号纪律：oracle 行号本次逐条 `awk` 实测；Rust 只给符号/文件，个别行号仅作定位提示（会位移）。
- 判级：本件 L（文档）；其服务的实现是 **R**（状态 + 落库面 + 时序）。

## 0. 结论摘要（可直接派工）

1. **P22 精确区间 = `PlayObject.Message.cs:442-529`**（442 `tObjCount = GameGold;`；529 是 `if` 的 `}`，526-529 是唯一的 `RM_GOLDCHANGED` 出帧点）。之后 530-547 是 `Fight3Zone`、548-568 是 P24——PL-4 不碰这两段。
2. **四支严格按序**：① 玩家级 DEC → ② 玩家级 INC → ③ 地图 `DECGAMEGOLD` → ④ 地图 `INCGAMEGOLD`；③/④ 的门都带 `!BoDec/!BoInc`。四条**共用** `DecGameGoldTick`/`IncGameGoldTick`（不是每支一个 tick）。
3. **比较符不对称（必须钉死）**：玩家级 INC 用 `< 2000000`（等于即封顶并清 flag）；地图 INC 用 `<= 2000000`（等于即加上、**且永不清 flag**）。地图 INC 的 `nInteger` 在赋值**之前**算（与玩家级相反），但两条日志都已注释 ⇒ 无可观测差异。
4. **脚本 192/193 的参数解析 bug 复证**：`ActionOfAutoAddGameGold`（`ExecutionProcessingSys.cs:2667-2688`）与 `ActionOfAutoSubGameGold`（`:2733-2754`）里 `nPoint/nTime` 硬编码 0、只读 `sParam1`；`if (nPoint > 0 && nTime > 0)` **恒假** ⇒ `START` 恒走 `ScriptActionError`（只 `LogService.Error`，无帧），只有 `STOP`（清 flag）可达。当前 oracle 树里 `BoInc/BoDecGameGold` 的写入者只有这两个动作 ⇒ 二者在当前树+内容下**均为空操作**。
5. **`MoveToHome` 有一个恒假死分支（本件实测订正 spike 表述）**：`PlayObject.Base.cs:2312-2333` 的 `IEnvirnoment envir = Envir;` 在移动**之后**才赋值，`if (envir != Envir …)` 恒 false ⇒ `IsTimeRecall = false` **永不执行**。PL-4 不需要该字段；`world.rs::cast_space_move_spell` 的「无该字段」登记继续有效。
6. **落库口径照抄**：`characters.Gold` 同列双读（`Query.cs:241` → `GameGold`；另一处读入 `Gold`），Save 只绑 `@Gold = hd.Gold`（`Save.cs:102`，源是 `MakeSaveRcd:1245` 的 `playerInfo.Gold`）⇒ **GameGold 改动不落库**。Rust `bins/db-srv/src/playdata.rs:104/117/448` 已照抄；P22 只改内存 `sg.game_gold` 与 `rec.data.game_gold`，**不得**改 `state.gold` 或 DB 绑定。
7. **插入点不是 `world.rs`**：`crates/world/src/world.rs` 只在 `tick()` 文档块（~4150）登记了宿主段槽位；实际驱动在 `bins/game-svr/src/main.rs::sim_loop`，P22 落在 `drain_player_need_recheck`（14933）之后、`drain_player_revival`（14941）之前（P24 将来插在 P22 与 P25 之间）。`tick_tail` 不动。
8. **生产不可达**（沿用规划件实测，本车道未复查 `E:/MirServer`）：`Envir` 中 `AUTOADDGAMEGOLD/AUTOSUBGAMEGOLD` 文件数 0；`MapInfo.txt` 的 `DECGAMEGOLD/INCGAMEGOLD` 0 条。即便如此仍按 **R**（一旦内容出现即改状态/触发换图/影响落库记录）。

## 1. P22 精确语义（oracle 实测）

### 1.1 段边界与总序

| 行 | 内容 |
|---:|---|
| 442 | `tObjCount = GameGold;`（本段唯一快照） |
| 443-462 | ① 玩家级 `BoDecGameGold` 支 |
| 463-481 | ② 玩家级 `BoIncGameGold` 支 |
| 482-504 | ③ 地图 `!BoDecGameGold && Envir.Flag.boDECGAMEGOLD` 支 |
| 505-525 | ④ 地图 `!BoIncGameGold && Envir.Flag.boINCGAMEGOLD` 支 |
| 526-529 | `if (tObjCount != GameGold) SendUpdateMsg(RM_GOLDCHANGED,0,0,0,0,"")` |
| 530-547 | `Fight3Zone` 死亡计数（不在 PL-4） |
| 548-568 | P24 `boINCGAMEPOINT`（不在 PL-4） |

### 1.2 四支逐行语义（按代码字面，含 bug）

```text
① 443: if (BoDecGameGold && (now - DecGameGoldTick) > DecGameGoldTime)
   445:   DecGameGoldTick = now;                  // 先刷新 tick，再判额度
   446:   if (GameGold >= DecGameGold) { GameGold -= DecGameGold; }
   451:   else { GameGold = 0; BoDecGameGold = false; MoveToHome(); }

② 463: if (BoIncGameGold && (now - IncGameGoldTick) > IncGameGoldTime)
   465:   IncGameGoldTick = now;
   466:   if (GameGold + IncGameGold < 2000000) { GameGold += IncGameGold; }
   471:   else { GameGold = 2000000; nInteger = 2000000 - GameGold /*恒 0*/; BoIncGameGold = false; }

③ 482: if (!BoDecGameGold && Envir.Flag.boDECGAMEGOLD)
   484:   if ((now - DecGameGoldTick) > Envir.Flag.nDECGAMEGOLDTIME * 1000)
   486:     DecGameGoldTick = now;
   487:     if (GameGold >= Envir.Flag.nDECGAMEGOLD) GameGold -= nDECGAMEGOLD;
   492:     else { GameGold = 0; BoDecGameGold = false; MoveToHome(); }

④ 505: if (!BoIncGameGold && Envir.Flag.boINCGAMEGOLD)
   507:   if ((now - IncGameGoldTick) > Envir.Flag.nINCGAMEGOLDTIME * 1000)
   509:     IncGameGoldTick = now;
   510:     if (GameGold + Envir.Flag.nINCGAMEGOLD <= 2000000) GameGold += nINCGAMEGOLD;
   515:     else { nInteger = 2000000 - GameGold; GameGold = 2000000; }   // 无 flag 清除
```

要点（每一条都应进单测）：

- 四条都用**严格 `>`**；`now - tick == time` 不触发。
- `DecGameGoldTick`/`IncGameGoldTick` **跨支共享**。因此：③ 能否在同一 pass 紧接 ① 再扣一次，取决于 ① 是否刚把 `DecGameGoldTick` 刷成 `now`（刷新后 `0 > time*1000` 仅在 `time < 0` 时成立）。
- ① 的 `DecGameGold` 与 ③ 的 `nDECGAMEGOLD` **可以 ≤ 0**（ctor=1；地图解析失败缺省 -1）：`GameGold >= -1` 恒真 ⇒ `GameGold -= -1` 即**每周期 +1**。④ 同理（负 point 每周期减少）。
- ② 封顶时清 `BoIncGameGold`；④ 封顶时**不清** ⇒ ④ 每个周期都重跑（值为 2,000,000 时不再变化，但 tick 被刷新）。
- ①② 的 tick 刷新发生在**额度判断之前**；因此"余额不足"那一 pass 也会刷新 tick（下一周期从该 pass 起算）。
- `nInteger` 仅被注释掉的 `AddEventLog` 使用 ⇒ 无帧/无副作用；不要在 Rust 引入日志字段。

### 1.3 字段、初值与生命周期

| 字段 | oracle | 初值 | 换图/登录 |
|---|---|---|---|
| `GameGold` | `PlayObject.cs:763`；载入 `WorldServer.cs:1142`（源 `characters.Gold`） | ctor `:964` = 0 | 不清 |
| `BoDecGameGold` | `:767` | ctor `:965` = false | 不清 |
| `DecGameGold` | `:770` | ctor `:966` = 1 | 不清 |
| `DecGameGoldTime` | `:768` | ctor `:968` = 60000 | 不清 |
| `DecGameGoldTick` | `:769` | ctor `:967` = now | **换图/登录都不复位** |
| `BoIncGameGold` | `:772` | ctor `:970` = false | 不清 |
| `IncGameGold` | `:776` | ctor `:971` = 1 | 不清 |
| `IncGameGoldTime` | `:774` | ctor `:973` = 60000 | 不清 |
| `IncGameGoldTick` | `:775` | ctor `:972` = now | `UserLogon:1484`、`EnterAnotherMap:3795` 复位 = now |

- **Rust 时钟域**：`HUtil32.GetTickCount()` = `Environment.TickCount`（`HUtil32.cs:36-39`，int 毫秒）。宿主段用 `now_ms()`（`clock::now_ms`，与 P32 同源），**不得**用 `world.clock_ms`。
- **登记差异**：oracle 是 `int`（≈24.8 天回绕，差值可为负）；Rust 用 `i64` 不回绕。只在超长 uptime 的相位上可见，按既有「时钟回绕」登记口径处理，不模拟回绕。
- 地图旗标源：`Envir.Flag` = 玩家**当前** `map_id` 的 flag。Rust `MapRegistry::flag_of(map_id)`（`crates/envir/src/registry.rs:351`）已解析 4 个字段（`crates/envir/src/mapinfo.rs:69-77`，默认 -1/0 与 `Maps.cs:212-218` 一致）。无 registry（S1 占位模式）时地图支按「无 flag」处理并登记，不得静默当作有 flag。

### 1.4 GameGold 的边界总表（P22 之外的全部读写者，供对照）

| 写/读点 | 位置 | 上限/下限/不足 |
|---|---|---|
| P22 ① 玩家 DEC | `Message.cs:443-462` | `>=` 够则减；不足 ⇒ 0 + `bo=false` + `MoveToHome`；无下限钳制 |
| P22 ② 玩家 INC | `463-481` | `< 2,000,000` 才加；否则置 2,000,000 + `bo=false` |
| P22 ③ 地图 DEC | `482-504` | 同 ①（读地图 point/time） |
| P22 ④ 地图 INC | `505-525` | `<= 2,000,000` 才加；否则置 2,000,000，**不清 flag** |
| 脚本变量 `$GAMEGOLD` | `GrobalVarProcessingSys.cs:1066-1070` | 只读 `ToString`，无钳制 |
| 条件 `CHECKGAMEGOLD` | `ConditionOfCheckGameGold` | 只读比较 |
| 脚本动作 `GAMEGOLD` | `ExecutionProcessingSys.cs:3058-3093` | `=` 直写（>=0）；`-`/`+` 只夹 `>=0`，**无上限**；变化才 `GameGoldChanged()` |
| GM `AddGameGold` | `AddGameGoldCommand.cs:35-43` | `nPoint>0`；`GameGold+nPoint < 2000000`，否则封顶 |
| GM `DelGameGold` | `DelGameGoldCommand.cs:31-39` | `>` 够则减；否则 0 |
| GM `GameGold` | `GameGoldCommand.cs:27-58` | `=`/`+`/`-` 全不钳制（`-` 可负）；nGold 0..200,000,000 |

`GameGold` 的整数加法在 oracle 是未检查 `int`；Rust 用 `wrapping_add` 对齐（P22 内所有 `+` 都要）。

### 1.5 出帧与帧序

- P22 只在 526-529 发一条：`SendUpdateMsg(RM_GOLDCHANGED)`（`crates/protocol/src/messages.rs:457` = 10136）。处理器 `PlayObject.Message.cs:1833-1835`：`SendDefMessage(SM_GOLDCHANGED, Gold, LoWord(GameGold), HiWord(GameGold), 0)` ⇒ **`Recog` 是 `BaseObject.Gold`（不是 GameGold），后两个字段才是 GameGold 的两半**。Rust 现成编码器：`smframe::sm_goldchanged(gold, game_gold)`（`bins/game-svr/src/smframe.rs:379-388`）。
- **不是** `GameGoldChanged()`/`RM_GAMEGOLDCHANGED(8084)`/`SendGoldInfo`——不要顺手加。
- 帧相位：`SendUpdateMsg` 只是**入 `MsgQueue`**，在**下一次** `GetMessage` 才出队 ⇒ oracle 比 P22 晚一个 Run（≈200ms）。宿主按既有 P21 `WeightChanged` 口径**同 tick 直发**并登记 1-tick 相位差。
- 若 `MoveToHome` 触发，`SendRefMsg(RM_SPACEMOVE_FIRE)`（8062）→ 每个接收端 `SM_SPACEMOVE_HIDE(800)`；`SpaceMove` 内的 `SendMsg(RM_CLEAROBJECTS)`/`RM_CHANGEMAP`/`RM_SPACEMOVE_SHOW` 也被下一 Run 按 **`(byte)wIdent` 优先级**出队。两者合并后的自机顺序（括号内 = `(byte)wIdent` 优先级，**按 RM 号算**：`RM_SPACEMOVE_FIRE=8062⇒126`、`RM_GOLDCHANGED=10136⇒152`、`RM_CLEAROBJECTS=8097⇒161`、`RM_CHANGEMAP=8098⇒162`、`RM_SPACEMOVE_SHOW=8100⇒164`）：**SM 800 → SM 653 → SM 633 → SM 634 → SM 801**。Rust 只要「P22 的 653 在 `drain_map_change` 的 633/634/801 之前、800 在移动前广播」，同 tick 就得到同一相对序。
- `SendRefMsg` 的收件人 = 旧位置 `±12` 内全部 `Race==Play` 玩家、**含自己**（`BaseObject.Message.cs:383-449`）⇒ 用 `broadcast_ref_msg_with_self`，且必须在**移动之前**（用旧坐标/旧图）。不要用 `send_self_frame`。

### 1.6 `MoveToHome` / `BaseObjectMove`（`PlayObject.Base.cs:2306-2333`）

```text
2306 MoveToHome:
2308   SendRefMsg(RM_SPACEMOVE_FIRE, 0,0,0,0,"")      // 旧位置 ±12（含自己）
2309   BaseObjectMove(HomeMap, HomeX, HomeY)
2312 BaseObjectMove:
2314   if (IsNullOrEmpty(sMap)) sMap = MapName;        // HomeMap 空 ⇒ 当前图名
2318   if (sX != 0 && sY != 0) SpaceMove(sMap, sX, sY, 0);   // 两轴都非 0 才走定坐标
2324   else MapRandomMove(sMap, 0);                    // 任一轴为 0 ⇒ 随机落点
2328   IEnvirnoment envir = Envir;                     // 移动后才赋值 ⇒ 下面恒假
2329   if (envir != Envir && Race == Play) IsTimeRecall = false;   // 死分支
```

- `MoveToHome` 的数据源：oracle 读**会话态** `PlayObject.HomeMap/HomeX/HomeY`——登录时从 `humData` 拷，新角色在 `:254-256` 落出生点；随后 `UserLogon`（`PlayObject.cs:1199`）与每 3s 的 `Run`（`PlayObject.Message.cs:266`）的 `GetStartPoint()`（`PlayObject.Base.cs:704-721`）还会按「红名 ⇒ `RedHomeMap` / 当前图在 `StartPointList` 的首条」重算并覆盖会话值。**本仓当前实现直接读 DB 存档记录** `saves[chr].record.data.home_map/home_x/home_y`（`bins/game-svr/src/record.rs:527-529`；回落写回在 `main.rs::write_home_fallback_to_record`，随后 `saves.insert`）——这是与 oracle 会话态的**登记分叉**（whitelist T-94），不是等价实现；本批次不另存会话态。
- `SpaceMove`（`BaseObject.cs:1103-1168`）同服支：`FindMap(sMap) == null` ⇒ 什么都不做（**0 抽数、不移动**）；否则删旧格 → 置 `Envir/CurrX/CurrY` → `SpaceMoveGetRandXY`（不可走则 3/10、2/15/50 步进 + `Random(Width/Height)`，至多 201 轮）→ 成功才 `AddMapObject` + `RM_CLEAROBJECTS`/`RM_CHANGEMAP`/`RM_SPACEMOVE_SHOW(nInt=0)`；失败回滚旧图旧格、**不出帧**。
- `MapRandomMove`（`BaseObject.cs:1214-1237`）：先两抽 `Random(Width-nEgdey-1)+nEgdey`、`Random(Height-nEgdey-1)+nEgdey`（`nEgdey` 只看**目标图**高度：<30⇒2，<150⇒20，否则 50），再 `SpaceMove(sMap, 随机点, 0)`。
- **RNG 账本（既有组，不需新组）**：
  - `merchant` 组 = `BaseObject.MapRandomMove@BaseObject.cs:1235/1236`；Rust `crates/world/src/merchant.rs::map_random_move_xy`（已登记）。
  - `space_move` 组 = `BaseObject.SpaceMoveGetRandXY@BaseObject.cs:1040`；Rust `crates/world/src/space_move.rs::space_move_get_rand_xy`（已登记）。
  - 走 `HomeX/HomeY` 支且落点可走：**0 抽**；走 MapRandomMove：**2 抽 + 可能的移位环**。
- `MapRandomMove` 的目标图必须是 **HomeMap**，不是当前图。现有 `merchant::map_random_move(world, actor_id)` 只对实体当前图 ⇒ **不能直接用**；需要在 `crates/world` 增加一个「目标图 + 目标坐标」的公共入口（见 §4.2）。

## 2. 脚本动作 `AUTOADDGAMEGOLD` / `AUTOSUBGAMEGOLD`（192/193）

### 2.1 实测与逐行证明（`src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs`）

`ActionOfAutoAddGameGold` = 2667-2688：

```text
2670: int nPoint = 0;
2671: int nTime  = 0;
2672: if (sParam1 OrdinalIgnoreCase == "START")
2674:   if (nPoint > 0 && nTime > 0)          // 恒假（两个局部变量从未被赋值）
2676-2679:   IncGameGold = nPoint; IncGameGoldTime = nTime*1000;
             IncGameGoldTick = GetTickCount(); BoIncGameGold = true; return;
2682: if (sParam1 == "STOP") { BoIncGameGold = false; return; }
2688: ScriptActionError(..., ExecutionCode.AutoAddGameGold);
```

`ActionOfAutoSubGameGold` = 2733-2754：逐字同构，差异只有 `DecGameGold/DecGameGoldTime/DecGameGoldTick = 0`（`:2742`，注意 **0 而非 now**）与最后一行错误码。

**证据**：对 2667-2754 全段做 `sParam` 检索，只命中 `:2671/:2682/:2737/:2748` 四处且**全是 `sParam1`**；`sParam2/sParam3` 在该段内**零命中**。同文件的兄弟动作 `ActionOfAutoGetExp`（`:2691-2711`）会 `StrToInt(sParam1/sParam2)`，对比即证这是漏写而非"参数在别处"。
⇒ **`nPoint/nTime` 恒 0 是常量折叠级别的事实，`START` 支不可达。**

### 2.2 START 变得可达的条件

- **当前 oracle 树 + 任何内容下：永不。** 脚本内容不能改变 C# 局部变量；也不存在别的入口写 `BoInc/BoDecGameGold`（全树写入者只有 `:2678/:2684/:2744/:2750` 与 P22 自身）。
- 唯一可能：上游 oracle 源码被修改（补上 `sParam2/sParam3` 解析）。届时按 R 重开（时序 + 状态）。
- 因此「内容出现即等价」这句话对 192/193 **不成立**：内容再多也只能命中 `START`（→ 错误日志）或 `STOP`（→ 清一个恒 false 的 flag）。

### 2.3 STOP 的可观测性

- `STOP` 清 `BoIncGameGold/BoDecGameGold`，而初值就是 false、无其它写入者 ⇒ 当前树里 **STOP 也是空操作**。
- `ScriptActionError` 只 `LogService.Error`（`:5987-5992`），**不发帧、不改 `Success`** ⇒ `START` 的可观测效果仅为一条错误日志。
- Rust 里这条可判：`EngineError::ScriptError`（`crates/script/src/engine/mod.rs:836` 等）。

### 2.4 Rust 落地形态（等价、不"修好"）

1. `crates/script/src/engine/actions.rs` 增两个分支（放 `ActionOfAutoGetExp` 附近）：
   - `"ActionOfAutoAddGameGold"`：`s_param1` 与 `START`（ASCII 忽略大小写）⇒ 直接 `script_action_error("AutoAddGameGold", info)`（**不解析 `s_param2/3`**）；`STOP` ⇒ `set_bo_inc_game_gold(false)`；其它 ⇒ 错误。
   - `"ActionOfAutoSubGameGold"`：同构，写 `set_bo_dec_game_gold(false)`，错误名 `AutoSubGameGold`。
   - 注释写明「oracle 从不解析参数，START 恒错误；不要为了可用性解析 `s_param2/3`」。
2. `crates/script/src/engine/mod.rs`：trait 增 `fn set_bo_inc_game_gold(&mut self, _v: bool) {}` / `set_bo_dec_...`（默认空实现，与 `set_game_gold` 同风格）。
3. `crates/script/src/engine/dispatch.rs` 的 `IMPLEMENTED_EXECUTIONS` 追加两个名字（否则仍报 `NotImplemented`）。
4. `bins/game-svr/src/npc.rs::NpcPlayer`：加 `bo_inc_game_gold/bo_dec_game_gold` 字段 + 两个 override。
5. 回写：`bins/game-svr/src/main.rs` 的 NPC 脚本体（`npc_player_from` 的 3 个调用点：1452 附近 / 14675 / 18050；有 `ground_sessions` 的两处）把两个 bool 回写 `SessionGround`。**注意**：因为 START 死、STOP 只会写 false，回写在当前树上必然是 no-op；仍建议接线，避免"字段存在但永不生效"的假实现。

## 3. 与 P24 / 金币落库的交互

### 3.1 `characters.Gold` 双读、GameGold 不落库（既有口径，照抄）

- 读：`PlayDataStorage.Query.cs:241` `humInfoData.GameGold = dr.GetInt32("Gold")`；`WorldServer.cs:1142` → `PlayObject.GameGold`。同一列另有 `Gold` 读入（`WorldServer.cs:1112`）。
- 写：`MakeSaveRcd:1245` `playerInfo.Gold = playObject.Gold`、`:1274` `playerInfo.GameGold = playObject.GameGold`；但 `Save.cs:102` 只 `@Gold = hd.Gold`（`playerInfo.Gold`）⇒ **`playerInfo.GameGold` 从不进 SQL**。
- Rust：`bins/db-srv/src/playdata.rs:104` 读 `Gold`→`gold`、`:117` 读同一列→`game_gold`、`:448` 绑 `d.gold`；`build_save_record`（`main.rs:4881`）写 `rec.data.game_gold = sg.game_gold`、`rec.data.gold = sg.state.gold`。
- ⇒ P22 只改 `sg.game_gold`；存档记录里 `game_gold` 变化、`gold` 不变；DB 列不变；重登后 `game_gold == gold`（回到金币列）。**不得修**。

### 3.2 与 P24（GamePoint）的接口

- P24 = `548-568`，只读 `Envir.Flag.boINCGAMEPOINT/nINCGAMEPOINT/nINCGAMEPOINTTIME` 与 `IncGamePointTick`；`<= 2,000,000`；**不出任何帧**。它与 P22 不共享字段，只共享「同一 Run 的段位」。
- 若 P22 的 `MoveToHome` 换图，**同一 Run 的 P24/P25/P26/P31 读到的是新图的 `Envir.Flag`**（oracle 的 `Envir` 已被同步改掉）。段序必须保持 P22 → P24 → P25 → P26 → P27 → P31 → P32。
- `MoveToHome` 走 `BaseObject.SpaceMove`（不是 `PlayObject.EnterAnotherMap`），**不会**触发 `:3794-3796` 的 `IncGamePointTick/IncGameGoldTick/AutoGetExpTick` 复位。`IncGameGoldTick` 的复位点只有 `UserLogon:1484` 与 `EnterAnotherMap:3795`。Rust 的对应：`drain_map_change` 里按 `MapChange.space_move_show == None`（= EnterAnotherMap）复位 `IncGameGoldTick`（以及将来 P24 的 `IncGamePointTick`）；`Some(_)`（= SpaceMove，含 MoveToHome）**不复位**。
- **既有缺口（登记，不在 PL-4 修）**：`build_save_record` 从不写 `rec.data.game_point`（spike §2.3/§5.1）⇒ GamePoint 运行期改动不回写。P24 时再补。

## 4. 写集与插入点

### 4.1 基线实况（修正派工描述）

- `crates/world/src/world.rs`：只在 `tick()` 文档块（~4150）**登记** `P22 … PlayObject.Run:442-529` 的槽位；`tick_head` 尾部（~4444）注释说明「宿主段在两段之间」。**PL-4 不改 `world.rs` 的段序**（除非采用 §4.2 的 world 侧移动入口）。
- 当前 master 已有 PL-1：`drain_player_need_recheck` / `drain_player_revival` 在 `sim_loop` 内、`tick_tail` 之前。
- `downlink.rs` **不需要改**：P22 的 653 用 `smframe::sm_goldchanged` + `route_to_gate` 直发（P21 的 `immediate` 同法）；`pickwire::notice_frame` 的 `RM_GOLDCHANGED` 映射只服务拾取/丢弃路径，别复用成 P22 的通知通道。

### 4.2 文件级写集

| 文件 | 改动 |
|---|---|
| `bins/game-svr/src/main.rs` | ① `SessionGround` 增 `p22: P22GameGold`（1 个字段，默认值 = oracle ctor：false/1/60000/tick 0；生产构造点 `14462` 用 `P22GameGold::new(now_ms())`；16 个测试字面量补 `Default`）；② 新增 `drain_player_game_gold(...)` 并插在 14933 与 14941 之间；③ `build_save_record` 已有 `rec.data.game_gold`，不动；④ `drain_map_change` 里补 `IncGameGoldTick` 复位（仅 `space_move_show == None`）；⑤ NPC 脚本回写（§2.4-5）。 |
| `bins/game-svr/src/gamegold.rs`（**新增**，建议） | 纯函数 `auto_tick(&mut P22GameGold, game_gold, flag: Option<&MapInfoFlag>, now) -> AutoTickOutcome { game_gold, changed, move_to_home }`，逐行复刻 442-529；单测放这里（注入 now/flags，不依赖 world）。 |
| `bins/game-svr/src/homemap.rs` | 新增 `resolve_move_home(...)`：`HomeMap` 空 ⇒ 当前图名；`x/y` 是否同时非 0 ⇒ 分支枚举；`FindMap` 解析失败 ⇒ `NoMap`。**不在这里抽数**（抽数在 world）。 |
| `crates/world/src/world.rs`（**新增公共方法**，替代任务给的「全部在 homemap.rs」） | `World::base_object_move_player(chr, actor_id, target_map_id: Option<usize>, x: i16, y: i16) -> MoveHomeOutcome`：`None` ⇒ 不抽不动；`x!=0&&y!=0` ⇒ `space_move_get_rand_xy` 起步 `(x,y)`；否则 `map_random_move_xy` 两抽 + 移位环；成功 ⇒ `switch_map_space(..., show2=false)`（记 `MapChange{Some(false)}`），失败/201 轮 ⇒ 不落地、不记 MapChange。**必须用 `world.rng`**。 |
| `crates/script/src/engine/actions.rs` | 192/193 两个分支（§2.4）。 |
| `crates/script/src/engine/mod.rs` | 两个 trait setter（默认空）。 |
| `crates/script/src/engine/dispatch.rs` | `IMPLEMENTED_EXECUTIONS` 两行。 |
| `bins/game-svr/src/npc.rs` | `NpcPlayer` 两个 bool 字段 + override（`empty` 里初始化）。 |
| `bins/db-srv/src/playdata.rs` | **不改**（只加一致性测试或用例引用）。 |
| `mir2-rs/tests/parity/m3/rng-trace-probe/consumer_groups.tsv` | **不改**（`merchant`/`space_move` 已覆盖）；若新增 world 公共方法产生新站点，按文件归组后跑 `--require-closed` 自证。 |

### 4.3 `sim_loop` 插入点

```rust
drain_player_need_recheck(...);                 // P21（14933）
drain_player_game_gold(                         // P22 ← 新增
    &mut world, &mut ground_sessions, &online, &gates,
    registry.as_ref(), &saves, now_ms(),
);
drain_player_revival(...);                      // P32（14941）
world.tick_tail(&mut rec);                      // Die + VerifyMapTime（14955）
```

`drain_player_game_gold` 内部逐玩家（`names.sort()`，与 P21/P32 同构）：

1. 快照 `t_obj_count = sg.game_gold`；
2. 取 `flag = registry.flag_of(entity.map_id)`；
3. `gamegold::auto_tick(&mut sg.p22, sg.game_gold, flag, now)`；
4. `sg.game_gold = outcome.game_gold`；
5. 若 `outcome.move_to_home`：先 `broadcast_ref_msg_with_self(world, online, gates, actor_id, &[sm_spacemove_hide(SM_SPACEMOVE_HIDE, actor_id)])`，再 `homemap::resolve_move_home(...)` + `world.base_object_move_player(...)`；
6. 若 `outcome.changed`：`route_to_gate(...)` 发 `smframe::sm_goldchanged(sg.state.gold, sg.game_gold)`（**改后**值；`Recog = state.gold`）。

注意：`auto_tick` 每 pass 只返回一个 `move_to_home` 布尔；oracle 同 pass 内两次 `MoveToHome` 只在「① 触发后 ③ 的 `time<0`」这一病态组合下可能（§1.2），实现时可返回 bool 或计数；**按字面应允许两次**，建议返回 `u8` 以钉住该组合。

### 4.4 RNG / trace 影响

- 内容未启用 ⇒ P22 不抽数，五批 trace 组集合/次序不变。
- 一旦 `MoveToHome` 触发：新增 `merchant` 2 抽（MapRandomMove 支）与 `space_move` 移位环。两组已登记；但**相对次序**变为「玩家 Run 段末尾」，需在构造场景下重跑 `rng_consumer_groups.py --require-closed` 与 `rng_ledger_diff.py groupdiff --fold`。这属于 R 窗口证据的一部分。

### 4.5 串行与依赖

- 硬前置：PR-3 spike 已出结论（本件直接消费）；P21（PL-1）已在本基线落地。
- 串行：PL-4 与 PL-1/PL-2/PL-3 全部改 `bins/game-svr/src/main.rs` 的同一宿主段与 `SessionGround` ⇒ 单线；若采用 §4.2 的 `world.rs` 新方法，还要与任何改 `crates/world` 的车道串行。
- 依赖：无新配置键；无新协议号；`smframe::sm_spacemove_hide`/`sm_goldchanged`/`broadcast_ref_msg_with_self` 全部已有。

## 5. 判据草案（构造级，全部带牙）

> 口径：固定 `now`（毫秒，注入）；`CountingRng` 记每次 `random_below` 的入参；断言**次数/参数/相对顺序**，不只看结果。

1. **玩家 DEC 的时钟与额度**（R）：`bo_dec=true, point=30, time=1000, game_gold=100, tick=now`；推进 1000ms ⇒ 仍 100（严格 `>`）；1001ms ⇒ 70 且 tick=now；再过 1001ms ⇒ 40。牙：把 `>` 改 `>=` ⇒ 1000ms 用例红；删 tick 刷新 ⇒ 第二次不扣红。
2. **玩家 DEC 不足 ⇒ 清零 + 清 flag + 恰好一次 MoveToHome**（R）：`game_gold=10, point=30` ⇒ 0、`bo_dec=false`、移动一次；再推进 10 个周期 ⇒ 仍 0、不再移动。牙：不清 flag ⇒ 第二次移动红；把"不足"改成 clamp 到 0 但保留 flag ⇒ 红。
3. **玩家 INC 封顶的严格边界**（R）：`point=100, game_gold=1_999_900` ⇒ 2,000,000 且 `bo_inc=false`；`1_999_899` ⇒ 1,999,999 且 flag 仍 true。牙：`<` ↔ `<=` 互换 ⇒ 两个用例各红一个。
4. **地图 DEC 与共享 tick**（R）：`bo_dec=false, dec_tick=now-1001, flag DECGAMEGOLD(30/1), game_gold=100` ⇒ 70；随后同 pass 把玩家级 `bo_dec=true` 且 `dec_tick=now-5000` ⇒ 玩家级先跑（扣 30、刷 tick），地图支**不再**扣。牙：给地图支独立 tick 字段 ⇒ 第二条红。
5. **地图 INC 不清 flag + 负 point 可观测**（R）：`flag INCGAMEGOLD(-1/1), game_gold=100` ⇒ 每 1001ms 减 1（99/98/…），证明每周期都重跑；对照 `INCGAMEGOLD(200/1), game_gold=1_999_900` ⇒ 置 2,000,000 且下一周期仍会进入分支（值不变、tick 刷新）。牙：照抄玩家级 `bo_inc=false` ⇒ 负 point 只减一次，红。
6. **帧的内容与只发一次**（R）：`game_gold 100→70`、`state.gold=1234` ⇒ 恰一条 `SM_GOLDCHANGED(653)`，`Recog=1234`、`Param=LoWord(70)`、`Tag=HiWord(70)`；无变化 pass ⇒ 0 条。牙：把 `Recog` 写成 `game_gold` ⇒ 红；无条件发帧 ⇒ 红。
7. **MoveToHome 的分支选择与抽数**（R）：`HomeX/HomeY != 0` 且落点可走 ⇒ **0 抽**、目标格=Home 坐标、`MapChange{show=false}`；落点不可走 ⇒ `space_move` 组抽数（参数 Width/Height）；`HomeX=0` ⇒ `merchant` 恰 2 抽（`Random(W-nEgdey-1)`，nEgdey 按目标图高）+ 可能的 `space_move` 环。牙：用当前图的宽高算抽数 ⇒ 红；`SX!=0 && SY!=0` 写成 `||` ⇒ 用例红。
8. **MoveToHome 的广播与顺序**（R）：在旧图 + 旧坐标、±12 内的两个观察者 + 自己各收一条 `SM_SPACEMOVE_HIDE(800)`；移动后新位置的玩家**不**收；且本机 socket 上 653 先于 633/634/801。牙：移动后再广播 ⇒ 旧观察者收不到、红；把 800 放到移动后 ⇒ 红；把 653 放到 `drain_map_change` 之后 ⇒ 顺序红。
9. **`FindMap` 为空 ⇒ 不动不抽**（R）：HomeMap 解析失败 ⇒ 0 条 633/634/801、0 抽、坐标/图不变（`MoveToHome` 的 800 已发，照 oracle 照发）。牙：提前 return 掉 800 ⇒ 红。
10. **脚本动作 192/193**（L：纯逻辑/分支）：`START 100 1` ⇒ 恰一条 `ScriptError`、`bo_inc` 仍 false；`STOP` ⇒ 0 错误、`bo_inc=false`；`FOO` ⇒ 1 错误。对 SUB 同构。牙：解析 `s_param2/3` 让 START 生效 ⇒ 红；START 不报错 ⇒ 红。
11. **落库一致性**（R）：P22 把 `game_gold` 100→70 后：`build_save_record` 的 `rec.data.game_gold==70`、`rec.data.gold` 保持原值；`playdata.rs` 的 UPDATE 仍绑 `d.gold`。牙：把 `rec.data.gold` 也改成 game_gold ⇒ 红；把 DB 绑定换成 `game_gold` ⇒ 红。
12. **登录/换图的 tick 相位**（R）：`P22GameGold::new(now)` ⇒ inc/dec tick 都 = now；`EnterAnotherMap`（`MapChange{None}`）⇒ `inc_tick` 复位、`dec_tick` 不变；`SpaceMove`（`Some`，含 MoveToHome）⇒ 两者都不复位。牙：把两种 MapChange 一起复位 ⇒ 红。

**tier 与窗口**

- tier = **R**：涉及状态、时序（严格比较/共享 tick）、换图（RNG + 帧）与存档记录。
- 窗口：`window_required: true`。生产不可达（`Envir` 0 文件、`MapInfo.txt` 0 条）⇒ 真机无可触发对象，需 owner 裁决「构造级 + 逐行/抽数 + 内容可达性证据」替代；review 必须显式写缺什么证据，不得用构造级绿冒充窗口结论。
- 脚本 192/193 本身可按 **L** 收（纯逻辑分支），但其宿主回写与 `SessionGround` 字段属于同批 R 面的附庸。

## 6. 未决 / 需注意的既有登记（不重复当新发现）

1. P22 生产不可达：规划件 §1.2/§1.3 的 `grep` 实测（本车道按只读约束未复查 `E:/MirServer`）。
2. 脚本 192/193 参数 bug：spike §4 第 1 条；本件补了逐行证据与「START 永不可达」的结论。
3. GameGold 不落库：spike §1.3/§5.1；Rust 已照抄。
4. `game_point` 不回写：spike §2.3/§5.1；PL-4 不修（P24 时补）。
5. `IsTimeRecall` 无字段：`World::cast_space_move_spell` 文档已登记；本件证明 `BaseObjectMove` 里该赋值本身是死分支。
6. 1-tick 帧相位（`SendUpdateMsg` 下一 Run 才出队）：与 P21 `WeightChanged` 同族，按既有登记口径。
7. `int` tick 回绕：oracle `Environment.TickCount` vs Rust `i64`；按既有「时钟回绕」登记，不模拟。
8. 地图旗标解析的缺省 -1（`Maps.cs:216-218` / `mapinfo.rs:523-528`）：构造负 point/time 用例时按 -1 设，不要擅自 clamp 到 0。

## 7. 只读与残留物声明

- 未建 worktree、未改仓库任何文件、未起服务/客户端、未碰 `E:/MirServer`、未连 MySQL。
- 唯一产出：`E:/tmp/laneB-pl4.md`（本文件）；无其它临时脚本/日志/库。
- 行号均来自 `c2ac5cccb` 的 `src/**` 与 `mir2-rs/**` 只读实测；Rust 行号会随并行车道位移，以符号名为准。


---

## 评审员更正（2026-10-19，**以本节为准**）

- **P22 主体支持**（442-529 四支顺序、严格 `>`、共享 tick、玩家级 `< 2,000,000` vs 地图 `<= 2,000,000`、
  地图 INC 永不清 flag、653 映射、MoveToHome 死分支、`characters.Gold` 双读且 GameGold 不落库）。
- **判据 4/5 的地图旗标语法写反**：实际是 `(time/point)`，不是 `(point/time)`；判据 4 的玩家级时钟门默认值不成立。
- **漏了 `SendUpdateMsg` 的同 ident 去重语义**（653 的第二次会替换第一次）与 **`STOP` 大小写不敏感**。
- 脚本 192/193 在当前 oracle 源码下 **START 恒 ScriptActionError、STOP 空操作**；只在源码改成解析参数后才可达。
- tier=R + `window_required: true` 成立；生产不可达（Envir 0 文件、MapInfo 0 条）需 owner 裁决替代证据。
