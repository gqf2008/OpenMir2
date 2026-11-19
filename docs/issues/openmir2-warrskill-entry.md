# openmir2-warrskill-entry

> 状态：**入口缺口的登记卡**，2026-11-20，由卡 `openmir2-magic-parity` 的 18/28 接线期间发现。
> **2026-11-20 更新**：oracle 侧取证（A 段 `ClientSpellXY` 入口 + 9 条 golden、B 段 12/25/26/27/34/38
> 逐 case 读通）已落地，见文末；**Rust 侧等价路径仍待派**（本卡原始「不实现代码」指 Rust/产品代码）。

## 背景：一个「看起来绿其实没测」的陷阱

`MagicManager.DoSpell` 的**第一句**就是战士门：

- `src/M2Server/Magic/MagicManager.cs:151-154`：`if (IsWarrSkill(userMagic.MagIdx)) return false;`
  （`IsWarrSkill` 见 `:82-104`，命中 3/4/7/12/25/26/27/34/38 九个魔号）。

⇒ 对派对拍装置 `tests/parity/csharp/MagicGolden` 现有的 `RunMagicCase`（**直调 `DoSpell`**）：
这 9 个魔号会得到 **`SpellReturn=false` + 空账本（零 RM/零 SM/零 RNG）**。
**这不是 oracle 的真实线上行为**——真实帧面由**调用方**产生：

- `ClientSpellXY`（`src/M2Server/Player/PlayObject.Attack.cs:1054`）的 `default` 支调用
  `DoSpell(...)`，返回 false 时发 `RM_MAGICFIREFAIL`（`:1274-1283`）⇒ 处理器
  `PlayObject.Message.cs:1794-1800`（**无自守卫**）⇒ `SM_MAGICFIRE_FAIL(639)` 给自己 + 范围内玩家。
- 9 个魔号里 **只有 3/4/7** 走 `default`（因此有 639）；**12/25/26/27/34/38** 在
  `ClientSpellXY` 有独立 `case`（`:1120/1136/1168/1187/1218/1234`），**根本不调用 `DoSpell`**
  （另有 `SKILL_43` 的 case `:1253`，它不在 `IsWarrSkill` 名单内）。

**结论**：用 `DoSpell` 直调产出的 golden 对这 9 条是**假零行为**；拿它对照 Rust——即便 Rust
也「零帧」——也**不能**证明对拍（两侧都错在同一个入口上）。

## 要求（补入口前禁止升格）

1. **装置**：`MagicGolden` 增加 `ClientSpellXY` 入口（构造与 `PlayObject.Attack.cs:1054`
   一致的调用上下文：`wIdent`、`nKey`、坐标、目标、`boLateDelivery`），并按魔号分派到
   `default`（3/4/7）或各自 `case`（12/25/26/27/34/38）。
2. **判据**：3/4/7 的 golden 必须体现**恰一条 639**（收件人含自己与范围内玩家、零 RNG、
   MP 不变、无 53、无训练尾）；12/25/26/27/34/38 的 golden 按其 `case` 臂的实际帧面生成
   （各自 `case` 的行为需先逐行读 `PlayObject.Attack.cs` 的对应块并写进卡面）。
3. **清单**：在入口与 golden 落地前，`parity-gap-magic.json` 的 **MG-01..MG-09 一律不得**
   标 `verified`；也不得用「Rust 也零帧」作为 `dead_in_oracle` 的依据（oracle 的真实线上帧面
   是 639 或该 case 臂的帧，不是"死"）。
4. **Rust 侧**：宿主 `default_arm && is_warr_skill` 块目前只覆盖 3/4/7 的 639
   （`bins/game-svr/src/lib.rs` 的注释已写明「9 个魔号中只有 3/4/7 进 default 支」）；
   12/25/26/27/34/38 的 `ClientSpellXY` 等价路径若缺失，属 `missing`（需登记偏差）。
5. **交付**：本卡实现的验收 = 「同一魔号、同一输入，`ClientSpellXY` 入口的 golden 与 Rust
   等价路径逐帧一致」；不得借用 `DoSpell` 的账本。

## 不做

- 本卡**不实现**任何代码；不动 `MagicGolden`、不动 Rust 宿主。
- 不把 MG-01..09 的 `status` 改成任何"已验证/死路径"。

---

## A 段交付：`ClientSpellXY` 入口 + 9 条 golden（2026-11-20）

**装置**（对应上文「要求 1」）
- 入口文件：`mir2-rs/tests/parity/csharp/MagicGolden/ClientSpellXYEntry.cs`（**全部逻辑在此**）。
  `Program.cs` 只加 **1 行**分派：`--case warrskill`，紧随 `fireball` 分派行；与
  `card/magic-parity` 的 `fireboom` 行（紧随 `bigcloak`）不同区域，合并无重叠。
- 调用上下文与 `PlayObject.Message.cs` 的 `case Messages.CM_SPELL` **同参**：
  `ClientSpellXY(wIdent=CM_SPELL, nKey=MagIdx, nTargetX/nTargetY, targetObject=ActorMgr.Get(nParam3),
  boLateDelivery=false)`；私有方法经反射调用，`delayTime` 出参入账。
- 夹具几何沿用既有用例（caster 100,100 / target 101,100 / obs_in 105,100 / obs_out 140,100）；
  `M2Share.NetChannel` 换只记录实现（三处 `SendSocket` 重载都汇到 `AddGateBuffer`）；
  排水「抽空为止」（上界 8，命中进产物）；等 1200ms 跨自延迟。
- **判据边界**：`CM_SPELL` 的调用方尾部（`ActionTick`、成功后的
  `SendSocket(M2Share.GetGoodTick)`（`+GD/<uptime>` 短包）、失败时的 `RM_MOVEFAIL`）**不属于**
  `ClientSpellXY`，且 `+GD` 是墙钟 ⇒ 不入 golden。RM 的 `delivery`、`NotDue` 的 `now` 等墙钟字段
  同样排除（章程 L5）。
- golden 每条落盘：`config`（5 个 conf 的 sha256、`conf_dir`、`0.map` 的 sha256、`repo_head`）、
  `fixture_spec`、以及每 case 的 `Fixture`（含 `entry_args` / `preset` / `entry_return` /
  `delay_time` / `dir`）。工具落盘统一 LF。
- 产物：`mir2-rs/tests/parity/golden/magic/warr-{3,4,7,12,25,26,27,34,38}.json`。
- 复现：
  `dotnet run --project mir2-rs/tests/parity/csharp/MagicGolden -c Release -- --case warrskill --out-dir mir2-rs/tests/parity/golden/magic`
  （`MAGICGOLDEN_CONF_DIR` 缺省 `E:\MirServer\M2GameSvr`，`MAGICGOLDEN_MAP_DIR` 缺省其 `Map`）。

**golden 摘要（C# 实测）**

| 魔号 | arm | case | 关键帧面（按接收者） |
| --- | --- | --- | --- |
| 3/4/7 | default | invoke | caster / target / obs_in **各恰 1 条** `SM_MAGICFIRE_FAIL(639)`（Recog=ActorId，Param/Tag/Series=0，无尾）；obs_out 无；**零 RNG**；MP 不变；无 53；无训练尾 |
| 12 ERGUM | case | off2on / on2off | 短包 `+LNG` / `+ULNG` + 自收 `SM_SYSMESSAGE(100)`（绿字；尾=EDCode 文本，Series=1） |
| 25 BANWOL | case | off2on / on2off | `+WID` / `+UWID` + 绿字 SM100 |
| 34 CROSSMOON | case | off2on / on2off | `+CRS` / `+UCRS` + 绿字 SM100 |
| 26 FIRESWORD | case | charge / busy / nomp | charge：绿字 SM100 + MP−10 + `SM_HEALTHSPELLCHANGED(53)`（Param=HP、Tag=MP、Series=MaxHP）+ `+FIR`；busy（10s 门内）：**红字** SM100，无 53/`+FIR`；nomp（MP<10）：绿字 SM100，无 53/`+FIR` |
| 27 MOOTEBO | case | coordinate_dir | 真实输入（`nTargetX=101` ⇒ `Dir=101`）：红字 SM100 + MP−10 + 53 + `RM_RUSHKUNG` ⇒ `SM_RUSHKUNG(7)` 给 caster/target/obs_in（Param/Tag=100/100，Series=MakeWord(101,Light)）；**零 RNG** |
| 38 TWINBLADE | case | go / nomp | go：绿字 SM100 + MP−10 + 53 + `+TWN`；nomp：绿字 SM100，无 53/`+TWN` |

## B 段：12/25/26/27/34/38 的 `case` 臂逐行读通（依据 = 符号锚）

> 共同前缀（`PlayObject.Attack.cs::ClientSpellXY`）：`IsCanSpell` 假 / `Death` / `PoisonState.STONE`
> ⇒ 直接 false；`GetMagicInfo(nKey)` 按 `Magic.MagicId` 扫 `MagicList`，找不到 ⇒ false；战士技能
> （`MagicManager.IsWarrSkill`）**跳过**限速块；`SpellTick -= 450`（下夹 0）与
> `MagicAttackTick = now` 对所有支生效；战士技能**不更新** `MagicAttackInterval`。

### 12 `SKILL_ERGUM`（`case MagicConst.SKILL_ERGUM`）
- 条件：`MagicArr[SKILL_ERGUM] != null`（未学 ⇒ 什么都不做，但仍返回 true）。
- 行为：`!UseThrusting` ⇒ `ThrustingOnOff(true)` + `SendSocket("+LNG")`；否则 `ThrustingOnOff(false)` + `"+ULNG"`。
- `ThrustingOnOff`（`PlayObject.Base`）：写 `UseThrusting` 并 `SysMsg(MessageSettings.ThrustingOn/Off, Green)`；
  `SysMsg` 非公告支 ⇒ `SendMsg(RM_SYSMESSAGE, 0, GreenMsgFColor, GreenMsgBColor, 0, 文本)`（**只给自己**）。
- 收件人 = 自己；MP 不变；无 53；无 RNG。

### 25 `SKILL_BANWOL`（`case MagicConst.SKILL_BANWOL`）
- 与 12 同构：`HalfMoonOnOff` + `"+WID"/"+UWID"`，写 `UseHalfMoon`，绿字 `HalfMoonOn/Off`。

### 26 `SKILL_FIRESWORD`（`case MagicConst.SKILL_FIRESWORD`）
- 条件：`MagicArr[SKILL_FIRESWORD] != null`。
- `AllowFireHitSkill()`：`now - LatestFireHitTick > 10*1000` ⇒ 刷新 tick、`FireHitSkill=true`、
  绿字 `MessageSettings.FireSpiritsSummoned`、返回 true；否则红字 `FireSpiritsFail`、返回 false。
- true 支：`nSpellPoint = GetSpellPoint(um)`（`Round(Magic.Spell/4.0*(Level+1)) + Magic.DefSpell`）；
  `WAbil.MP >= nSpellPoint` 时：`nSpellPoint > 0` ⇒ `DamageSpell(nSpellPoint)`（MP 扣减、下夹 0）+
  `HealthSpellChanged()`（`Race==Play` ⇒ `SendUpdateMsg(RM_HEALTHSPELLCHANGED)` ⇒ 自收 `SM_HEALTHSPELLCHANGED(53)`），
  然后 `SendSocket("+FIR")`。
- MP 不足：**绿字已发**，但无扣减、无 53、无 `+FIR`。
- 返回恒 true（门没过也 true）。

### 27 `SKILL_MOOTEBO`（`case MagicConst.SKILL_MOOTEBO`）
- `result = true` 先置；`now - DoMotaeboTick > 3*1000` 才进动作体（3s 内二连 ⇒ 除公共前缀外无事发生）。
- 动作体：`DoMotaeboTick = now`；**`Dir = (byte)nTargetX`**（把目标 X 当方向——oracle 怪行为）；
  `nSpellPoint` 同 26；`MP >= nSpellPoint` 时扣减 + 53（`nSpellPoint > 0` 时），然后
  `if (DoMotaebo(Dir, UserMagic.Level)) { if (UserMagic.Level < 3) { if (Magic.TrainLevel[Level] < Abil.Level) {
  TrainSkill(um, M2Share.RandomNumber.Random(3) + 1); if (!CheckMagicLevelUp(um))
  SendSelfDelayMsg(RM_MAGIC_LVEXP, 0, MagicId, Level, TranPoint, "", 1000); } } }`。
- `DoMotaebo(Dir, level)`（`PlayObject.Base`）：`GetPoseCreate()`（按 `Dir` 取前方格对象）；
  有对象且 `DoMotaeboCanMotaebo`（`Abil.Level > 目标 Abil.Level && !StickMode`，且
  `Random(20) < level*4 + 6 + 等级差`，且 `IsProperTarget`）时：`CharPushed(Dir,1)`、移进目标格、
  `SendRefMsg(RM_RUSH, …)`（⇒ `SM_RUSH(6)` 给 12 格内 Play）；每推一次 `n24` 减 1；
  有推到时 `baseObject34` 记目标：`nDmg = Random((n24+1)*10) + (n24+1)*10` 过 `GetHitStruckDamage` 后
  `StruckDamage`，并让目标 `SendRefMsg(RM_STRUCK, nDmg, HP, MaxHP, ActorId)`（⇒ `SM_STRUCK(31)` 给目标周围）；
  前方不可走 ⇒ `SendRefMsg(RM_RUSHKUNG, …)`（⇒ `SM_RUSHKUNG(7)`）+ 红字 `MessageSettings.MateDoTooweak`；
  `n28 > 0` 时反向自伤（再一条 `Random` + 自 `RM_STRUCK`）。**仅当发生过移动才返回 true**。
- **真实输入**（`nTargetX` = 目标坐标 101）：`Dir = 101` 不是合法方向 ⇒ `GetFrontPosition` 的 `switch`
  不命中、坐标不变 ⇒ `GetPoseCreate` 取到**自己脚下那格**的首个对象（夹具里 = 施法者本人）；
  `Abil.Level > 自己等级` 不成立 ⇒ `DoMotaeboCanMotaebo` 假 ⇒ break（`n28=0`、`baseObject34=null`）；
  `bo35` 仍真 ⇒ 发 `RM_RUSHKUNG` + 红字，**不训练、零 RNG**，返回 false。
- 训练支（`Random(3)+1` 与 `RM_MAGIC_LVEXP → SM_MAGIC_LVEXP(640)`，延迟 1000ms）只在
  `DoMotaebo` 返回 true 且 `Level < 3` 且 `TrainLevel[Level] < Abil.Level` 时到达；
  **真实坐标输入到不了** ⇒ 本轮 golden 不覆盖（登记为未覆盖面，见下）。

### 34 `SKILL_CROSSMOON`（`case MagicConst.SKILL_CROSSMOON`）
- 与 12 同构：`SkillCrsOnOff` + `"+CRS"/"+UCRS"`，写 `CrsHitkill`，绿字 `CrsHitOn/Off`。

### 38 `SKILL_TWINBLADE`（`case MagicConst.SKILL_TWINBLADE`）
- 条件：`MagicArr[SKILL_TWINBLADE] != null`。
- `AllowTwinHitSkill()` **恒真**：刷新 `LatestTwinHitTick`、`TwinHitSkill = true`、绿字
  `"twin hit skill charged"`；随后同 26 的 MP 扣减 + 53 + `SendSocket("+TWN")`（MP 不足则只发绿字）。
- 注意：本臂**不**调 `SkillTwinOnOff`（该函数存在但此支不用）⇒ 没有 12/25/34 那种开关语义。

### RM → SM 映射与收件人面
- `SendRefMsg`（`BaseObject.Message`）：首次（或距上次 ≥500ms / `VisibleHumanList` 空）扫
  `CurrX/Y ± SendRefMsgRange` 格；`Race == Play` 的接收者都投递（**含施法者自己**——自己也在自己格子里）；
  非 Play 只投 `RM_STRUCK`/`RM_HEAR`/`RM_DEATH`；缓存路径按 `<11` 格过滤。
- `RM_MAGICFIREFAIL` → `SM_MAGICFIRE_FAIL(639)`（Recog = `processMsg.ActorId` = 施法者，**无自守卫**）；
- `RM_SYSMESSAGE` → `SM_SYSMESSAGE(100)`（Param = `MakeWord(色F, 色B)`、Series=1、尾 = EDCode 文本）；
- `RM_HEALTHSPELLCHANGED` → `SM_HEALTHSPELLCHANGED(53)`（Recog = 处理者，Param=HP、Tag=MP、Series=MaxHP）；
- `RM_RUSH` / `RM_RUSHKUNG` → `SM_RUSH(6)` / `SM_RUSHKUNG(7)`（Param/Tag=x/y、Series=`MakeWord(Dir, Light)`）；
- `RM_MAGIC_LVEXP` → `SM_MAGIC_LVEXP(640)`。

## 零变化证明与「入库 golden vs 当前工具」分类（按 opc-07 2026-11-20 裁决）

- **本批改动的零影响（已证）**：同一棵树两次构建——`Program.cs` 含 / 去掉 `warrskill` 分派行
  （其余逐字节相同）——各重跑 14 条（`fireball`、`lightning11`、`lightning45`、`healing`、`firecharm`、
  `hbfireball`、`fireball5`、`hangmajinbub`、`dejiwonho`、`shield`、`showhp`、`cloak`、`bighealing`、
  `bigcloak`），**14/14 逐字段一致**（仅 RM 的 `delivery` / `NotDue` 的 `now` 等墙钟随运行变化）。
  分派行对既有用例是惰性的。
- **入库 golden 与当前工具重生成（裁决要求的两类分类；未入库、未改任何文件）**：
  - **元数据类**：`Fixture` 缺 `wait_ms`/`caster_hp`/`caster_max_hp`（工具在 `e1c2a7952` 之后有 7 个
    `Program.cs` 提交）；`DrainPasses` 2 vs 4；墙钟字段。
  - **行为类：非空** —— 6/14 文件（`lightning-11`、`lightning-45`、`fireball-5`、`firecharm`、
    `hbfireball`、`healing`），共 104 处观测。例：`lightning-11` miss 入库 target 仅 2 条 RM、无伤害
    （HP 100）；重生成 6 条 RM / 5 条 SM（含 `SM_STRUCK(31)`、`SM 656`）、HP 85；`Rng` `[2]` vs
    `[2,0,0,0,6]`；53/31 帧的 `MaxHP` 字段 100 → 224；638 的尾编码文本也不同。
  - **初判根因（待 opc-07 裁决）**：现行工具的登录序夹具（`RecalcLevelAbilitys` / `RecalcAbilitys`）
    会重算目标能力——`AntiMagic` 被覆盖（miss 变成实伤）、`MaxHP` 100 → 224 ⇒ miss/edge/hit 的语义面
    与入库 golden 不再同一。
- **状态**：按裁决第 4 条**已停下上报**；本批**未**重生成入库、**未**改 Rust 驱动期望。

## 未覆盖面（本批 golden 不覆盖，登记）

1. **27 的推/训练支**：真实客户端输入下 `Dir=(byte)nTargetX` 恒为坐标值（夹具 101），到不了
   `DoMotaebo` 的推动路径与 `Random(3)+1` 训练 / `SM_MAGIC_LVEXP(640)`；本条按「先读通、不假设」
   记录，是否另造方向型输入由 opc-07 裁决。
2. **26/38 的 10s/恒真门二次调用**：每 case 用新施法者对象，未覆盖「同角色短时间重复施法」的状态残留。
3. **调用方尾部**（`+GD/<uptime>`、`RM_MOVEFAIL`）按上文判据边界不入 golden。
