# GM `@改属性`（`SmakeItemCommand`）实现（T-217）

- 卡：`openmir2-gm-impl-smake`（tier=R；父卡 `openmir2-gm-reachability-registry` 的 `script-reachable` 项）
- 分支：`card/openmir2-gm-impl-smake`，base master `ff4dc1e80`
- 作者：`openmir2-worker-24`；worktree `OpenMir2-wt-cronclaim`

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

**登记（未接）**：① **脚本路不落地待写缓冲**——与既有 `apply_pending_gm_entity`（`@1/@2/@3`）
同一限制（只在 `CM_SAY` 支落地）；② `:30-34` 的 `GetStdItem == null` 支未建模。

## 3. 用例（6）与牙（2）

gmcmd：表项；越界静默（4 型）；空槽红字；`Desc` 写 + 蓝字逐字节；14/15 耐久（含 `_MIN(65)`）。
lib：宿主落地（会话+实体同源、缓冲一次性、203 帧 `Recog=ActorId`/`Series=1`/带尾）。

牙（实做并还原，sha256 一致）：命令不登记 `pending_use_item` ⇒ 2 条红；宿主删实体写 ⇒ 落地用例红。

## 4. 门禁（本 head）

fmt / clippy `-D warnings` / `cargo test --workspace`（141 个测试二进制 0 FAILED）/ msgcodegen 604 /
anchor 510 ALL PASS / registry GREEN / T 预约表 GREEN / gm_registry `build+gate+selftest` GREEN。

## 5. 残余 / 窗口项（R，未跑）

- 真机双栈：`Npc_Def/比奇国王-0122.txt:71` 的 `@QF` 经 `Gmexecute 改属性` ⇒ 蓝字正文与
  `SM_UPDATEITEM(203)` 逐字节一致；DB 后态（`hum_items` 的 `Dura/DuraMax/Desc`）与属性重算
  （`characters_ablity`）一致；空槽支一条红字。窗口未跑前不 closed。
- 未起栈、未占端口、未连库。

## 6. 残留物清单

- `E:/tmp/w24-sm.sha`（牙的 sha256 基线）。
