# GM `@改属性`（`SmakeItemCommand`）实现（T-217）

- 卡：`openmir2-gm-impl-smake`（tier=R；父卡 `openmir2-gm-reachability-registry` 的 `script-reachable` 项）
- 分支：`card/openmir2-gm-impl-smake`（已合 master `f3716d0c6`，含 exec-seams/ShowEffect）
- 作者：`openmir2-worker-24`（返工：`openmir2-worker-65`，按 reviewer-3 `41a99a47`）；worktree `OpenMir2-wt-cronclaim` / `wt-worker-65`

## 1. oracle 逐行（`src/Modules/GameCommand/Commands/SmakeItemCommand.cs:16-80`）

- `:26` 守卫 `0<=nWhere<=12 && 0<=nValueType<=15 && 0<=nValue<=255`，不满足 ⇒ **静默 return**；
  **`nValueType <= 15` 让 `:36-43` 的 `nValueType == 16` 吸伤支不可达**（死支，照抄省略）。
- `:28` 空槽 ⇒ 红字 `身上没指定物品!!!`；`:30-34` 模板缺失 ⇒ 静默（`GmWorld` 无物品表 ⇒ 登记）。
- `:35` `Desc[9] = _MIN(255, Desc[9]+1)`（**无条件**）。
- `:44-55` 14 ⇒ `Dura = (ushort)(_MIN(65,nValue)*1000)`、15 ⇒ `DuraMax` 同式；`:56-59` 其余写 `Desc[n]`。
- `:60-61` `RecalcAbilitys()` + `SendUpdateItem`（`SM_UPDATEITEM(203)`，`Recog=ActorId`、`Series=1`、
  尾 = `EncodeBuffer(ClientItem)`；`StdMode==50` 名字追加 `" #"+Dura`）。
- `:62-69` 蓝字回执 `"{Index}-{MakeIndex} {Dura}/{DuraMax} {Desc0}/…/{Desc13}"`
  （`ItemAttr.WeaponUpgrade = 10` ⇒ 13 槽按 0..=13 全列）。
- `:70-73` `ShowMakeItemMsg` 只写服务端日志（无帧）。

## 2. 实现

- `GmSession.pending_use_item: Option<GmUseItemWrite>`（整槽差量，与 `pending_entity` 同法）+
  `Imp::SmakeItem` + `smake_item()`（纯函数：守卫 → 空槽红字 → 算新槽 → 蓝字回执）；
- 宿主 `apply_pending_gm_use_item()`（`lib.rs`）：写会话 `use_items` + 实体 `UseItems`
  → `recalc_session_ability`（同一份 `RecalcAbilitys` 实现）→ 返回 `SM_UPDATEITEM(203)` 帧体；
  `CM_SAY` 支落地并下发；
- `measure`：`gm_registry.json` 重算（Implemented 98 → 99 / Unimplemented 87 → 86）。

**复审 blocking 修复（脚本路落地）**：原先只在 `CM_SAY` 支 apply ⇒ 本卡唯一的生产可达路径
（固定 NPC `Npc_def/比奇国王-0122.txt:71` 的 `@QF → GMEXECUTE 改属性`）**从不落地**，且 pending
会残留到玩家下次聊天。现在：

- 抽 `apply_gm_use_item_write(sg, w, world, catalog)`（单会话版本）+ `apply_pending_gm_use_item_in`
  （从 `sg.gm` 取 pending）；
- **五个脚本接缝**在**脚本帧排水之前**各落一次：商人 `@main`/`1011`/修理/`@back` 四处
  （`run_merchant_goto` 返回后、`emit_script_frames` 之前）、QFunction `run_party_goto_label`
  （`gm_sess` 归位后、脚本帧之前）、QFunction `@StdModeFunc*`、`@Login`/QManage、
  固定 NPC `guild_official::handle_select`（`@QF` 生产路径，两处 `goto_label`）；
- **每 tick 兜底** `collect_gm_use_item_pending()`（`world.tick()` 前的 sim 循环级）：任何残留
  pending 也在本 tick 落地，不再拖到下次聊天（复审 blocking 的残留面）。

**复审 important-2（帧序）**：oracle `SendUpdateItem` 走 `SendDefMessage`（立即），`SysMsg`
进 `MsgQueue`（命令结束后排水）⇒ 线上 **203 先、蓝字 100 后**。实现：`handle_gm_command`
**只返回** `SysMsg` 与非 `SysMsg` 帧（不直接发），调用方按「203 → 100 → 其它直发帧
（如 `@AttackMode` 的 213，oracle 排在绿字之后）」统一排水。

**登记（未接）**：`:30-34` 的 `GetStdItem == null` 支未建模（`GmWorld` 无物品表；窗口用有效 index）。

## 3. 用例（9）与牙（5）

gmcmd：表项；越界静默（4 型）；空槽红字（**字面量 + GBK 字节**，不再用常量自证）；`Desc` 写 +
蓝字逐字节（**含 GBK 字节**）；14/15 耐久（含 `_MIN(65)`）。
lib：宿主落地（会话+实体同源、缓冲一次性、203 帧 `Recog=ActorId`/`Series=1`/带尾）；
**帧序** `[203, 100]`（`GmRig` 走生产入口）；**空槽字面量 + GBK 字节**；
**脚本路 `@QF`**：`GuildNpcFixture` 跑 `GMEXECUTE 改属性 3 2 <$STR(M3)>` ⇒ 203 先于蓝字、
会话+实体 `Desc[9]+1`、`Desc[2]=0`（字面量 ⇒ 值 0）、蓝字 dump 逐字节。

牙（实做并还原，sha256 一致）：

| 牙 | 改动 | 结果 |
| --- | --- | --- |
| 1 | 命令体不登记 `pending_use_item` | 2 条用例红 |
| 2 | 宿主删实体写 | 落地用例红 |
| 3 | 删固定 NPC 接缝的落地调用（复审 blocking 回归牙） | 脚本路 `@QF` 用例红（装备不改、无 203） |
| 4 | `handle_gm_command` 改回「当场发 100」（复审 important-2 回归牙） | 帧序用例红（`[100, 203]`） |
| 5 | 空槽文案少一个 `!` | 字面量 + GBK 用例红 |

## 4. 门禁（本 head）

fmt / clippy `-D warnings` / `cargo test --workspace`（141 个测试二进制 0 FAILED）/ msgcodegen 604 /
anchor 510 ALL PASS / registry GREEN / T 预约表 GREEN / gm_registry `build+gate+selftest` GREEN。

## 5. 残余 / 窗口项（R，未跑）

- 真机双栈：`Npc_Def/比奇国王-0122.txt:71` 的 `@QF` 经 `Gmexecute 改属性 3 2 <$STR(M3)>`。
  **判据按「参数是字面量」写**（`ActionOfGmExecute` 不展开动作参数，T-165）：
  - 两侧**各一条** `SM_UPDATEITEM(203)` **先于**蓝字 `SM_SYSMESSAGE(100)`；
  - 蓝字 dump 逐字节 = `"{Index}-{MakeIndex} {Dura}/{DuraMax} 0/0/0/…/1/0/0/0/0"`，
    其中 **`Desc[2]` = 0**（`StrToInt("<$STR(M3)>")` 失败 ⇒ 0，**不是** M3 的目标值），
    `Desc[9]` 无条件 +1；
  - DB 后态（`hum_items` 的 `Dura/DuraMax/Desc`）与属性重算（`characters_ablity`）两侧一致；
  - 空槽支一条红字 `身上没指定物品!!!`。窗口未跑前不 closed。
- 未起栈、未占端口、未连库。

## 6. 残留物清单

- `E:/tmp/w24-sm.sha`（牙的 sha256 基线）。
