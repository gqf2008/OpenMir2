# IT-08 升级支 —— 宿主级对拍 + 吃物品帧序修正 的 §16 行为牙证据

对象：`tests::it08_levelup_host_parity`（`bins/game-svr/src/lib.rs`）。
对拍 = golden **全帧线上序** `got == immediate_frames ++ pumped`（探针把直发帧与泵出帧收在同一条
`CaptureNetChannel.Frames`，先直发后泵出，故拼接序即真实线上序）。golden 见
`mir2-rs/tests/parity/m3/fixtures/eat_vectors.json` 的 `eat_ok_stdmode3_shape13_exp_levelup`；
重生成经过见 `golden-regen.txt`。

## oracle 发射点（符号锚，非行号）

- `SM_EAT_OK(635)`：`PlayObject.Operate.cs::ClientUseItems` → `SendDefMessage(SM_EAT_OK,…)`
  —— **直发**（`SendDefMessage`→`SendSocket` 立即写 gate）。
- `SM_HEALTHSPELLCHANGED(53)`：`BaseObject.IncHealthSpell` → `SendMsg(RM_HEALTHSPELLCHANGED)`
  —— **入队**；由 `PlayObject.Message.cs` case `RM_HEALTHSPELLCHANGED` 泵出。
- `SM_WEIGHTCHANGED(622)`：handler case `RM_WEIGHTCHANGED` 泵出（`use_item` 每次成功吃都入队 RM_WEIGHTCHANGED）。
- `SM_WINEXP(44)`：handler case `RM_WINEXP`。
- `SM_LEVELUP(45)/SM_ABILITY(52)/SM_SUBABILITY(752)`：handler case `RM_LEVELUP`。

**序律**：`SendMsg` 入 `MsgQueue`（优先级 = `(byte)wIdent`，游戏循环按升序出队）；`SendDefMessage` 直发。
⇒ 直发 635 **先于**一切泵出帧；泵出内部按优先级 53(68) < 622(131) < 44(139) < 45/52/752(150)。

## 分账（req-1）

- 本目录对应**吃物品帧序修正**（独立提交）；IT-08 升级支宿主层实现另有其提交。
- **修前**：53 被放在 immediate_frames(635) **之前** ⇒ 线上序 `[53,635,622,44,…]`（错）。
- **修后**：53 在直发帧之后 ⇒ `[635,53,622,44,45,52,752]`（与 golden 全序一致）。

## 影响面（req-4）

- 该**缺陷**（53 错位）**仅升级支**：53 只在 `if leveled` 分支发射，非升级吃物品无 53 ⇒ 普通路径不受影响，属 IT-08。
- 但 `SM_WEIGHTCHANGED(622)` 是**每次成功吃**都发的**通用帧**（`useitem.rs` 成功尾部）；本夹具此前未覆盖其 golden，
  现通过泵 10115 纳入。这是"通用吃物品帧面覆盖"的增强，不是缺陷修复。（如另需非升级用例的 622 全序 golden，另立夹具增强，不属本卡。）

## 变异（§16，见 teeth-red-ABCDEF.txt；均 ran=(0,1) = 1 测跑 1 失败）

| 变异 | 红处 |
|---|---|
| A 去 recalc_session_ability（旧实现不重算 WAbil） | 53/45/622 值错（WAbil 陈旧） |
| B 去 send_levelup_frames | 缺 45/52/752 |
| C 去 53 发射 | 缺 53 |
| D 去 inc_health_spell(2000,2000) | 53 hp/mp=(0,0)≠(52,32) |
| E 去升级 add_body_luck(100) | body_luck 0.2≠100.2 |
| **F 把 53 移回 635 之前（序修正的反向）** | 全序 `[53,635,…]`≠`[635,53,…]` |

快照-还原-复跑见 `restore-and-baseline.txt`。事故记录：变异 E 首次（旧 golden）在编译中被低内存看守杀掉，
已 cp 快照还原核 sha；本轮（最终 golden）A–F 全部重放取得红原文。
