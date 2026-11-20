# openmir2-uenhancer — 36 `SKILL_UENHANCER`（无极真气）对拍卡

**状态**：实现 + 657 接线完成；`verified`（**部分范围**：`dc` 依赖 `43` 的 ExtraAbil 修正卡）。
**代码 head**（已验证）：`09882d3d9`（rebase 到 master `3df3b0202`）；**本卡**为 docs-only 追加。

## 1. oracle 事实（树 `3df3b0202`）
- 案例：`src/M2Server/Magic/MagicManager.cs:610-630`（`case SKILL_UENHANCER`）。目标归一 `:612-617`：`targetObject==null ⇒ 自己`。
- 效果 **`AttPowerUp`**（`src/M2Server/Player/PlayObject.cs:5323-5331`）：`ExtraAbil[0]=nPower`、`SysMsg`、`RecalcAbilitys()`、`SendMsg(RM_ABILITY)`。
- **状态属主 = `targetObject`**（`:618-625` 对 target 调 `AttPowerUp`）⇒ 657 的属主是 target；self 夹具下 target==caster。
- `RecalcAbilitys()` 的 657：`PlayObject.cs:2874 CharStatus=GetCharStatus(); :2875 StatusChanged(); :2876 SendUpdateMsg(RM_CHARSTATUSCHANGED, HitSpeed, CharStatus, …)` ⇒ 广播（self+可见）+ 自更新（self）。

## 2. 夹具（`MagicGolden/Program.cs` 的 `UenhancerBatch`）
- `targetNull=true` + `ArmRingAmulet=true` + `CasterLoginOrderAbil=true`（**只** `RecalcLevelAbilitys()`，保留 `WAbil.SC=15` 供 `nTime=GetAttackPower(GetPower13(60)+Lo(SC)*10, Hi-Lo+1)`）。
- **非 0 状态夹具**：`CasterCharStatusEx=2` ⇒ `GetCharStatus()=2`（使 657 的 `CharStatus` 读取点**可证伪**，见 §4②）。
- golden：`mir2-rs/tests/parity/golden/magic/uenhancer.json`（s1/s5/s13）；实测 `nPower=2`、`nTime=250`；657 `param=2/tag=0/series=0`。

## 3. 657 接线（`mir2-rs/bins/game-svr/src/lib.rs`，36 臂）
- 复用 `SpellFrameBuf`（不新增发射函数、不改共享件），prio 155：
  - 广播 3（self+target+obs_in）：`push_charstatuschanged(&mut buf, world, online, out.target, status)`。
  - 自更新 1（self）：`buf.push_actor(..., RM_CHARSTATUSCHANGED&0xFF, smframe_ext::sm_charstatuschanged(out.target, status, e.hit_speed))`。
- `status = world.get_char_status(out.target)`（**oracle 同源 `GetCharStatus()`**，非陈旧 `entity.char_status`）。
- 落点由 RM 号算：`52(3)<638(136)<642(141)<657(155)<100(252)<640(255)` ⇒ caster stream `[52,638,642,657,657,100,640]`。

## 4. 证据（代码 head `09882d3d9`）
- `cargo check -p game-svr --lib` = **rc 0**。
- 驱动 `tests::uenhancer_parity_{s1,s5,s13}` 各 1 passed；比较器 `real_rust_ledger_uenhancer`：**657 全绿，仅剩 3 条**（每 seed 一条 `52` ability 尾 = `dc`）。
- **可证伪对照**：把读取改回 `entity.char_status`（陈旧 0）⇒ 比较器 **3→15 条**，`Sent[3/4].param golden=2 rust=0`（即 657 param 2→0）**红**；改回 `get_char_status` **绿**。⇒ 非 0 夹具确实暴露陈旧读（不验常量）。
- **两牙**：删广播3 ⇒ 9 条红（目标/observer 缺 657）；删自更新1 ⇒ 3 条红（caster 缺 1 条 657）；均恢复 sha=EQUAL、`MUTATION`=0。
- **逐 socket 657 计数**（§47，只计数不判跨 socket 序）：caster 2、target 1、obs_in 1（全 `param=2`）。
- 桩态：`git status --porcelain` 空、`grep -rn MUTATION` = 0。

- **牙的溯源**：两颗牙（删广播3 / 删自更新1）的执行 head = `09882d3d9`；之后 rebase 到 `648d545d1` **未改 657 接线代码**（只并入 master 的 ExtraAbil 修复），故交付 head 不再重跑牙——证据链可对：接线代码在 `09882d3d9` 与交付 head 逐字相同。

## 5. 已知差异（`dc`，依赖 `43`）
- `52` ability 尾的 `dc`：oracle `0x0605`（`RecalcLevelAbilitys` Warrior `MakeWord(max(lv/5-1,1),max(1,lv/5))`，lv30⇒Hi6） vs Rust `0x0805`（Hi8）。
- 根因：`bins/game-svr/src/ability.rs` 的 `apply_extra_abil` 把 `ExtraAbil[0]=nPower=2` 折进 `WAbil.DC.Hi`；oracle 的 52 帧 `WAbil` 不含该 +2（`ExtraAbil` 不进 `WAbil`）。
- 裁定（`3b`，`docs/issues/qa-extraabil-adjudication-20261120.md`）：**`ExtraAbil` 折进 `WAbil` 永远不该发生**，全局缺陷，统一由 `43` 修。**本卡不改 `apply_extra_abil`**；`43` 落地后本支应 `diff_fields=0`。
- ⇒ 本卡 `verified` 为**部分范围**：`dc` 待 `43`。

## 6. 依赖
- `43`：ExtraAbil 折进 WAbil 的全局修正（`dc` 部分范围的关闭条件）。
