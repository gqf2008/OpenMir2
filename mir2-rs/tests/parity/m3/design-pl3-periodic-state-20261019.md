# PL-3 实施简报（P25 + P26 + P27，1s 周期状态）

- 车道：Lane C（只读产出）；基线 `83beb2a2e`（master）。
- 输入：`plan-run-gap-batches-20261018.md` §1.2/§1.3、`spike-player-run-state-ownership-20261018.md` §3.2、oracle `PlayObject.Message.cs:569-657`。
- 性质：可直接派工的实施简报；所有 oracle 行号均 `sed -n`/`awk` 实测（§7 复现）。
- 判级：整批 **R**（HP/战斗数值/经验/持久化）。三段**当前生产全部不可达**（证据见各节），但 R 级结论不能由构造级替代，review 必须写 `window_required: true` + 内容不可达 + owner 裁决替代证据。

---

## 0. 一页结论

1. **P25**：地图 `DECHP/INCHP` 是 per-Run 定时事件，无 RNG。`INCHP` 照抄了 `DECHP` 的字段（`:585/:587` 用 `nDECHPPOINT`），后果是 **`INCHP` 单独出现时恒为 no-op + 每周期一条 53**；`nINCHPPOINT` 全仓只被解析、从未被消费。必须原样复刻。
2. **P26**：饥饿系统 = `HungerSystem` + `HungerDecHP` 两个开关（部署 `Server.conf:396/398 = 0`）；`HungerDecPower` 是**死键**（声明/读取/默认都有，全仓无消费者）。状态机含 `PerHealth/PerSpell` 的 **u8 回绕**、`HungerStatus` 无下限、无 `!Death` 门、跨档 `RefMyStatus = RecalcAbilitys + SM_MYSTATUS`。
3. **P27**：1s 门严格 `> 1000`，两个倒计时独立递减，归零各发一条红色 SysMsg；`win_exp` 改读 **per-player** `Entity.kill_mon_exp_rate`，`rate=100` 时既有数字逐字节不变（golden 不漂移是硬回归门）。
4. **插入点**：`bins/game-svr/src/main.rs` 的宿主段，P21 之后、P32 之前，顺序 **P22(预留) → P25 → P26 → P27 → P31(预留) → P32**（锚点见 §4.2）。
5. **写集扩张必须知会**：197/198 脚本动作需要 `crates/script/src/engine/{mod,dispatch,actions}.rs`、`crates/script/src/handlers.rs`、`bins/game-svr/src/npc.rs`——与 **PL-2 写集重叠**，只能串行（计划里 PL-3 只写了 `actions.rs`，不够）。
6. **帧面硬规则**：oracle 的 `SendUpdateMsg(RM_HEALTHSPELLCHANGED)` 在**同一 Run 内按 ident 去重**（后写覆盖前写）。P25 DECHP+INCHP、P25+P26 同 Run 只能出一条 53，载荷取最终 `WAbil`。实现方案见 §4.3（推荐 Option Q；最小替代会丢掉这条等价性）。

---

## 1. P25 —— 地图 `DECHP` / `INCHP`（`PlayObject.Message.cs:569-594`）

### 1.1 oracle 逐行（实测，`:569-594`）

```csharp
569  if (Envir.Flag.boDECHP && (GetTickCount() - DecHpTick) > (Envir.Flag.nDECHPTIME * 1000)) {
571      DecHpTick = GetTickCount();
572      if (WAbil.HP > Envir.Flag.nDECHPPOINT) { WAbil.HP -= (ushort)Envir.Flag.nDECHPPOINT; }
576      else                                   { WAbil.HP = 0; }
580      HealthSpellChanged();
581  }
582  if (Envir.Flag.boINCHP && (GetTickCount() - IncHpTick) > (Envir.Flag.nINCHPTIME * 1000)) {
584      IncHpTick = GetTickCount();
585      if (WAbil.HP + Envir.Flag.nDECHPPOINT < WAbil.MaxHP) {   // ← BUG: DECHP 的字段
587          WAbil.HP += (ushort)Envir.Flag.nDECHPPOINT;          // ← BUG: DECHP 的字段
589      } else { WAbil.HP = WAbil.MaxHP; }
593      HealthSpellChanged();
594  }
```

要点（逐条都必须照抄）：

- **无 `!Death` 门**：死玩家（`Death=true, HP=0`）也跑。`INCHP` 可以把死玩家的 HP 抬回 `MaxHP`，但 `Death` 不会被清——**死亡但仍带正 HP** 的怪态是可达的（`base.Run` 的 `if (HP == 0) Die()` 不再命中）。必须写一条牙。
- **`HealthSpellChanged()` 无条件调用**：即使 HP 数值没变（DECHP 打到 0）、即使 INCHP 加 0 点，也照发一次 53。按「变更才发」实现会红。
- **tick 初值 = 0**：`DecHpTick/IncHpTick` 只在 `PlayObject.cs:783-784` 声明，**ctor 不赋值** ⇒ 默认 0。⇒ 进图后**第一次 Run 立即触发**（`now - 0 > time*1000`），不是等 1s。必须写一条牙。
- 门是严格 `>`；`nDECHPTIME/nINCHPTIME` 是**秒**；`*1000` 在 C# 是 int 乘法（unchecked，异常值回绕，照抄时用 `i32` 乘法再转 `i64` 比较）。
- HP 运算是 **ushort 环绕**：`(ushort)point` 先截断；`HP -= (ushort)point` 与 `HP += (ushort)point` 都是模 65536。
- 比较是 **int 提升**：`i32::from(HP) > point`、`i32::from(HP) + point < i32::from(MaxHP)`——不要用 u16 比较。
- `nDECHPTIME < 0`（解析失败默认 `-1`）⇒ `time*1000 = -1000` ⇒ 门恒真 ⇒ 每个宿主 tick 都触发。照抄。

### 1.2 INCHP 的 bug：为什么会这样

- 两块是**复制粘贴的对称体**：DECHP 块 569-581、INCHP 块 582-594。作者把 `boDECHP→boINCHP`、`DecHpTick→IncHpTick`、`nDECHPTIME→nINCHPTIME` 都换了，**漏换两处 `nDECHPPOINT`**（`:585` 判上限、`:587` 加血）。
- 证据：`grep -rn nINCHPPOINT src --include=*.cs` 只有 3 处——`Maps.cs:208`（解析）、`Envirnoment.cs:1440`（调试串打印）、`MapInfoFlag.cs:38`（声明）。**没有任何玩法读取点**。
- 后果（写进判据，按 oracle 原样）：
  1. **INCHP-only 地图是 no-op**：`nDECHPPOINT` 默认 0 ⇒ `HP + 0 < MaxHP` 为真 ⇒ 加 0 点，但每周期仍发一条 53（HP 不变）。这张图的「回血」完全不存在。
  2. **DECHP+INCHP 同图**：回血用的也是扣除数。若 `DECHP(1/10)` + `INCHP(1/7)`，实际净变化是 `-10 +10 = 0`（不是 -3）。
  3. 若有人「修好」成 `nINCHPPOINT`，必须登记 `T-*`/`B-*` 并单独开卡；本批**禁止修正**。
- 牙：`DECHP(1/10)+INCHP(1/7)`、HP=50、MaxHP=100、两门同时过 ⇒ 断言 **HP=50**（DECHP 后 40，INCHP 用 point=10 回到 50）；把 `:585/:587` 改成 `nINCHPPOINT` ⇒ 断言必红（得 47）。再来一条 INCHP-only（point=0）⇒ HP 不变、但每周期一条 53。

### 1.3 数据来源（MapInfo / 地图对象）

- oracle：`Envir.Flag`（当前地图对象）的 `boDECHP/nDECHPTIME/nDECHPPOINT`、`boINCHP/nINCHPTIME/nINCHPPOINT`（`MapInfoFlag.cs:35-38`，全部 `int/bool`）。
- **解析口径（重要，和直觉相反）**：`Maps.cs:198-209` 用 `GetValidStr3(sLine, ref sLine, '/')`（`HUtil32.Backslash = '/'`，`HUtil32.cs:12/384-389`）：
  - `nDECHPPOINT = 第二段`（`/` 之后），`nDECHPTIME = 第一段`（`/` 之前）。
  - 即 `DECHP(3/100)` ⇒ **time=3 秒、point=100 点**。Rust `crates/envir/src/mapinfo.rs::split_slash_point_time` 已按同一反转实现，单测 `dechp_point_and_time_are_swapped` 已绿 ⇒ **本批不改解析器**。
- Rust 运行期读取：`Entity.map_id`（`usize`）+ `MapRegistry::flag_of(map_id) -> Option<&MapInfoFlag>`（`crates/envir/src/registry.rs:351`）。
- **运行期唯一改旗标的入口**：脚本 `SETMPAMODE`（oracle `ScriptDefName` 本身就是这个拼写，`ExecutionCode.cs:438`）的 `DECHP` 支（`ExecutionProcessingSys.cs:4586-4600`）可置 `boDECHP/nDECHPTIME/nDECHPPOINT`；**没有 INCHP 支**。GM 命令的 DECHP 整段被注释（`SetMapModeCommamd.cs:228-234`）。Rust 有 CodeDef/名称映射（`codes.rs:448`、`handlers.rs:211`）但 `dispatch.rs` **没有 `ActionOfSetMapMode` 实现** ⇒ 即使脚本写了也不生效（既有缺口，另立卡 `openmir2-script-setmapmode`；本批只在报告登记）。

### 1.4 RNG 抽数点

**零**。P25 三个分支都不抽数；`HealthSpellChanged` 也不抽。`consumer_groups.tsv` 无需新增行。注意 P21 的 `DropItemDown`（已落地）与 P22 的 `MoveToHome`（PL-4）有抽数，本段不要在那两者之间插任何取数。

### 1.5 生产不可达证据（只读实测）

```sh
wc -l /e/MirServer/M2GameSvr/Envir/MapInfo.txt        # 2282 行 / 322 个 [section]
grep -a -ic 'DECHP\|INCHP' .../Envir/MapInfo.txt       # 0
for c in SETMPAMODE SETMAPMODE; do grep -a -ril "$c" .../Envir --include=*.txt | wc -l; done  # 0 / 0
```
⇒ 部署内容下 P25 永远不执行。**本批生产行为改变 = 0**；R 级窗口没有对象，需 owner 以「构造级 + 逐行/类型 + 内容 grep」裁决替代。

---

## 2. P26 —— 饥饿系统（`PlayObject.Message.cs:596-635`）

### 2.1 配置键全表

| 键 | C# 声明 | C# 默认 | 读取点 | 部署 `Server.conf` | 消费者 |
|---|---|---|---|---|---|
| `HungerSystem` | `GameSvrConf.cs:501` | `:1505 false` | `ServerConf.cs:282` | **`:396 = 0`** | P26 `:596` 总开关 |
| `HungerDecHP` | `:502` | `:1506 false` | `:283` | **`:398 = 0`** | P26 `:612` 扣血支 |
| `HungerDecPower` | `:503` | `:1507 false` | `:284` | **`:399 = 0`** | **无任何消费者（死键）** |

- 读取语义 = `ConfigFile.ReadBool`（`ConfigFile.cs:188-203`）：大写后仅 `是/YES/1/TRUE` 为真，其余（含 `False`/垃圾）为假。Rust 已有 `config.rs::read_bool_or_false`/`parse_bool`，直接用。
- Rust 落地：`bins/game-svr/src/config.rs` 的 `GateConfig` 加 `hunger_system: bool`、`hunger_dec_hp: bool`（缺键 ⇒ false，非错）；`EnterConfig` 透传；`crates/envir/src/conf.rs::GameSvrConf` 同步加三个键（`HungerDecPower` 也加上，保持配置面完整，标注「oracle 无消费者」）。
- `HungerStatus` 本身已在 `SessionGround.hunger_status`（`pickwire.rs:57-59`）并已存档/回读（`playdata.rs`）；`GetMyStatus = min(4, HungerStatus/1000)` 已有 `useitem::my_status`；`RM_MYSTATUS → SM_MYSTATUS(708)` 已有 `smframe_ext::sm_mystatus(status)`（`Recog=0`, `Param=(short)status`）。**不需要新字段装 HungerStatus。**

### 2.2 状态机（逐行等价伪码）

```text
if (!HungerSystem) return;                        // :596
if (now - DecHungerPointTick <= 1000) return;     // :598 严格 >
DecHungerPointTick = now;                          // :600
if (HungerStatus > 0) {                            // :601
    old = GetMyStatus();                           // min(4, HungerStatus/1000)，C# 截断
    HungerStatus -= 1;                             // :604 无下限，可无限负
    if (old != GetMyStatus()) RefMyStatus();       // :605-608
} else if (HungerDecHP) {                          // :610-612
    HealthTick -= 60;                              // :615 i32
    SpellTick  -= 10;                              // :616 i32
    SpellTick   = MAX(0, SpellTick);               // :617
    PerHealth  -= 1;                               // :618 u8 → 0 变 255（C# compound assign 环绕）
    PerSpell   -= 1;                               // :619 u8 → 0 变 255
    if (HP > HP / 100)                             // :620 int 除法
        HP -= (ushort)MAX(1, HP / 100);            // :622 ushort 环绕
    else if (HP <= 2) HP = 0;                      // :624-629（正 HP 不可达；HP=0 时是 no-op）
    HealthSpellChanged();                          // :631 无条件
}
```

必须照抄的 oracle 形态：

- **无 `!Death` 门**：死玩家也递减 `HealthTick/SpellTick/Per*` 并发 53（HP 保持 0）。
- `HP > HP/100` 对**任何正 HP 恒真**；`else` 只在 `HP == 0` 进入，且 `HP <= 2` 后仍置 0 ⇒ 整段 else 是 no-op（但别删，保持逐行形态）。
- `HungerStatus` 没有下限：一旦 ≤0，`HungerStatus > 0` 支永不再进，只有 `CM_EAT`（`StdItem.DuraMax/10`，上限 5000，`PlayObject.Base.cs:2086-2090`）能抬回去。`GetMyStatus` 只有 `>4 ⇒ 4` 的上限；`HungerStatus = -1001` ⇒ `-1`（C# 除法向零截断），并且照原样返回负数。**牙**。
- `RefMyStatus`（`PlayObject.Base.cs:288-292`）= `RecalcAbilitys()` + `SendMsg(RM_MYSTATUS)`。Rust 复刻：先把 `Entity.hp/mp` 同步进 `sg.state.wabil`（同 P21 复验 F1 的写法），`recalc_session_ability(sg, catalog, extra_abil)` → `apply_ability_to_entity`，再出 `SM_MYSTATUS`。重算**不修改** `PerHealth/PerSpell`（已核：`RecalcAbilitys` 不碰它们），所以 618/619 的改动不会被重算冲掉。
- `HealthTick/SpellTick` 在 `SessionGround.stamina`（i32），`PerHealth/PerSpell` 在 `SessionGround.recovery`（u8）。
- 持久化：`HungerStatus` 已随存档写回（`build_save_record` 已有 `rec.data.hunger_status = sg.hunger_status`）；本批只改内存值。

### 2.3 与 HP / 恢复 / 体力点的交互

- **同 Run 顺序**：oracle 顺序是「恢复段 70-142 → … → P25 → P26 → P27 → base.Run 的体力点段」。P26 的 `-60` 必须发生在 `base.Run` 体力点段**之前**。Rust 现在 `drain_recovery`/`drain_stamina` 都在 `tick_tail` 之后（`drain_recovery` 15015、`drain_run_downlinks` 15026 内含 `drain_stamina`）；P26 插在 `tick_tail` 之前 ⇒ **P26→体力点的相对顺序对了**，但「恢复段在 P26 之前」当前不成立（既有相位差，见 §6）。
- `HungerStatus` 跨档时 `RefMyStatus` 会做一次完整 Recalc（不是只发帧）。漏掉 Recalc 时 52/752/能力数值会停在旧值。
- DB 回读：`HungerStatus` 与内存一致性由既有 `drain_periodic_save` 覆盖；测试要断言存档字段（构造级可直调 `build_save_record`）。

### 2.4 生产不可达证据

- `Server.conf:396 HungerSystem=0` ⇒ `:596` 整段短路基；即使有人只开 `HungerSystem=1` 而 `HungerDecHP=0`，也只有状态帧/重算，无 HP 效果。
- `HungerSystem`/`HungerDecHP` 无脚本动作（oracle/Rust 都没有写这两个配置键的脚本）；只随 Server.conf 启动读取。
⇒ 生产零行为；R 级窗口无对象，owner 裁决。

---

## 3. P27 —— 经验/攻击倍率 1s 递减（`PlayObject.Message.cs:636-657`）

### 3.1 字段与生命周期

| 字段 | 类型 | oracle 声明 / ctor | 持久化 | 归属（本批） |
|---|---|---|---|---|
| `KillMonExpRate` | int | `:720` / ctor `:941 = 100` | 不落库 | **Entity**（`win_exp` 读） |
| `KillMonExpRateTime` | int | `:725` / 无 ctor ⇒ 0 | 不落库 | Entity（与 rate 同段） |
| `PowerRate` | int | `:724` / ctor `:943 = 100` | 不落库 | Entity（已有） |
| `PowerRateTime` | int | `:726` / 无 ctor ⇒ 0 | 不落库 | Entity（新增） |
| `ExpRateTick` | int | `:727` / ctor `:942 = now` | 不落库 | Entity（新增；**时钟域 = `clock::now_ms()`**） |

- 死亡不清、换图不清（spike §1.3 已核）；重新登录 = 新对象 ⇒ 重新 ctor 初始化。
- `ExpRateTick` 放 Entity 只为与 rate 同段（spike §3.2 的裁决）；**绝不能用 `world.clock_ms` 比较**——登录/首 tick 要 `now_ms()` 初始化，否则首秒立即触发或永不触发。
- 实体初始化点：`bins/game-svr/src/main.rs::new_player_entity`（生产 + 测试共用的玩家 ctor）设 `exp_rate_tick = clock::now_ms()`；`kill_mon_exp_rate=100`、两个 time=0 放 `Entity::new` 的默认块（`entity.rs` 925 附近）。
- 怪物也走 `Entity::new`：默认 100/0/0，不会被读（`win_exp` 只对玩家调用）。

### 3.2 1s 门语义（逐行）

```text
if (now - ExpRateTick) > 1000:            // :636 严格 >；未过门 ⇒ 什么都不做
    ExpRateTick = now;                    // :638 过门即刷新（即使两个 time 都是 0）
    if KillMonExpRateTime > 0:
        KillMonExpRateTime -= 1
        if KillMonExpRateTime == 0:
            KillMonExpRate = 100
            SysMsg("经验倍数恢复正常...", Red, Hint)     // :645
    if PowerRateTime > 0:
        PowerRateTime -= 1
        if PowerRateTime == 0:
            PowerRate = 100
            SysMsg("攻击力倍数恢复正常...", Red, Hint)   // :654
```

- 两个块**独立**、同一次 1s 门内都递减；两条 SysMsg 的顺序固定「经验 → 攻击」。
- **`Time = 0` 是永久倍率**：`> 0` 门永远不过，rate 永不回 100（脚本 `KILLMONEXPRATE 200 0` 的怪癖，写牙）。
- 每秒只减 1：不会因为 tick 间隔大而一次补多步（`if` 不是 `while`）——Rust 照抄单步，不要做「追帧」。
- `SysMsg` 走 `MsgColor.Red, MsgType.Hint` ⇒ `BaseObject.Message.cs:1588-1691` 的 else-default 支 ⇒ `RedMsgFColor/RedMsgBColor`（不是 Notice 支）。Rust：`sm_sysmessage(actor_id, red_f, red_b, &enter::gbk(字面量))`；字符串是**硬编码字面量**，不走 String.conf。
- `SM_SYSMESSAGE` 的 `Recog = player.ActorId`（`SendMsg(this,…)`，handler `:1714-1716`）。
- 时钟：`HUtil32.GetTickCount()` = `Environment.TickCount`（`HUtil32.cs:36-39`，i32，约 24.8 天回绕）。Rust `now_ms()` 是 i64 单调 ⇒ 回绕窗口有既有偏差（§6 登记）。

### 3.3 `win_exp` 改读 per-player：所有读 rate 的代码点（grep 实测）

| # | 代码点 | 现状 | 本批 |
|---|---|---|---|
| 1 | `crates/world/src/combat.rs:1297` `CombatConfig.kill_mon_exp_rate` | 全局字段 | **删除** |
| 2 | `crates/world/src/combat.rs:1317` `.. = 100` | 默认值 | **删除** |
| 3 | `crates/world/src/combat.rs:1399` `round(config.kill_mon_exp_rate/100.0 * dw_exp)` | 唯一生产读点 | 改 `round(killer.kill_mon_exp_rate/100.0 * dw_exp)` |
| 4 | `world.rs:5607/5615/5626` 调 `win_exp(k, …)` | 三处（组队成员/单人） | 不改签名；`k` 已是 `&mut Entity` |
| 5 | `combat.rs:1378` `award_kill_exp` | 无人调用，内部转 `win_exp` | 自动跟随 |

- `grep -rn "kill_mon_exp_rate" mir2-rs --include=*.rs` 只有上述 `combat.rs` 3 行 ⇒ 删除字段是编译期强制、无遗漏。
- **队伍语义**：oracle 对每个成员调各自的 `WinExp`（`PlayObject.Base.cs:764-821` 的 `Get(k)`），所以 **per-member rate**：队伍里 A=100、B=200，B 拿双倍。Rust `World::gain_exp` 对每个 `m` 用 `entities.get_mut(m)` 调 `win_exp` ⇒ 天然正确。写牙。
- `PowerRate` 生产读点（不动，仅供参考）：`combat.rs:363`（`AttackPowerMode::for_attacker` 快照）→ `:115`（乘算）→ `:132/:1283/:1576` 透传；`world.rs` 的 15 处 `for_attacker(&caster)`（法术/近战路径）。P27 只改字段值，不改任何签名。

### 3.4 既有 golden 影响面 + `rate=100` 回归门

- `rate=100` 时新公式 = `round(1.0 * dw_exp)` = 原公式 ⇒ **既有数字逐字节不变**。
- 受影响面（全部必须保持绿）：
  - `mir2-rs/tests/parity/world/golden/*.txt`（`basic/combat/combat_exp_hitter_ghost/combat_exp_nofix/session/aoi_viewrange12`）——跑前/跑后 `sha256sum` 必须一致；
  - `tests/parity/tests/exp_parity.rs`（经验曲线/标量）；
  - `crates/world/tests/party_attribution.rs`（840/280 等固定数）；
  - `crates/world/tests/combat_skill_vectors.rs`、`attack_dir_vectors.rs`（都显式 `power_rate=100`）；
  - `tests/parity/tests/common/mod.rs` 的 Scenario **没有** rate 字段（只有 `high_level_kill_mon_fix_exp`）⇒ 场景文件无需改。
- **回归门的设法（必须写进 PR 说明）**：
  1. 先记录基线 `sha256sum tests/parity/world/golden/*.txt`；
  2. 删 `CombatConfig.kill_mon_exp_rate`（编译期证明无遗留读点）；
  3. 加单测 `win_exp_rate_100_matches_legacy`：对 `dw_exp ∈ {0,1,3,25,99,100,101,999,12345}` 断言结果等于旧公式（硬编码期望值）；
  4. **红牙**：`killer.kill_mon_exp_rate = 200` ⇒ 每个期望值 ×2；`=0` ⇒ 0；队伍两成员 100/200 ⇒ 金额不同（证明读到的是 per-player，不是全局）。
- 反向禁止：保留 `CombatConfig.kill_mon_exp_rate` 但不用（静默分叉）——不允许。

### 3.5 脚本动作 197/198（`KILLMONEXPRATE` / `POWERRATE`）

oracle（`ExecutionProcessingSys.cs:5449-5464` / `:5539-5554`）：

```text
nRate = StrToInt(param1, -1); nTime = StrToInt(param2, -1);
if (nRate < 0 || nTime < 0) { ScriptActionError; return; }   // 0 合法
playerActor.KillMonExpRate = nRate;  playerActor.KillMonExpRateTime = nTime;
if (SystemShare.Config.ShowScriptActionMsg)
    playerActor.SysMsg(string.Format(MessageSettings.ChangeKillMonExpRateMsg, rate/100, time), Green, Hint);
```

- **部署事实**：`Server.conf:753 ShowScriptActionMsg=1`；`String.conf:235-236` 的模板是 **`%g`/`%d` 占位**（`经验倍数:%g 时长%d秒`），而代码是 `string.Format`（只认 `{0}/{1}`）⇒ 实发文本是**字面量 `%g`/`%d`**（不替换）。若实现消息，必须照抄这个怪癖。
- Rust 现状：`codes.rs:429-430` 已有 197/198 CodeDef；`handlers.rs` **没有** `(197,…)/(198,…)`；`dispatch.rs` 没有两个 handler 名；`actions.rs` 没有分支；`ScriptPlayer` 没有 setter。
- 需要新增（写集扩张）：
  - `crates/script/src/handlers.rs`：`(197,"ActionOfKillMonExpRate")`、`(198,"ActionOfPowerRate")`；
  - `crates/script/src/engine/dispatch.rs`：注册两个 handler 名；
  - `crates/script/src/engine/mod.rs`：`fn set_kill_mon_exp_rate(&mut self, rate:i32, time:i32) {}`、`fn set_power_rate(&mut self, rate:i32, time:i32) {}`（默认空实现）；
  - `crates/script/src/engine/actions.rs`：两个分支（含 `< 0` 错误分支与 `config_show_script_action_msg()` 门）；
  - `bins/game-svr/src/npc.rs::NpcPlayer`：4 个字段 + setter 实现 + 初始快照；
  - `bins/game-svr/src/main.rs`：`npc_player_from` 把 4 值从 Entity 带进去；两个脚本执行点（`@Login` 14621-14660、`run_party_goto_label` 1491-1525）执行后写回 **Entity**（P27 的 rate 真源）。
- 消息支：`NpcPlayer` 目前既没有 `sys_msg` 实现，宿主也不 drain `player.frames`（只 drain says）⇒ 即使打开 `ShowScriptActionMsg` 也发不出去。本批**建议**：动作消息按「不可观测」登记（部署 0 脚本使用），但字段设置/校验必须实现并写牙；若 review 要求完整等价，再加 `sys_msg` + 帧排水（另立卡）。
- 为什么 197/198 与 PL-2 串行：`engine/mod.rs`、`npc.rs` 是 PL-2 的写集；两个批次不能并行。

---

## 4. 实现顺序与写集

### 4.1 文件级写集

| 文件 | 改动 | 备注 |
|---|---|---|
| `crates/world/src/entity.rs` | +`kill_mon_exp_rate`(100)/`kill_mon_exp_rate_time`(0)/`power_rate_time`(0)/`exp_rate_tick`(0)；默认块 | PL-1 也改过此文件 ⇒ 必须基于已合并的 PL-1 |
| `crates/world/src/combat.rs` | `win_exp` 改读 `killer.kill_mon_exp_rate`；删 `CombatConfig.kill_mon_exp_rate` | 唯一读点 |
| `crates/world/src/world.rs` | **预期无功能改动**；只更新 `tick` 段序 doc（已列 P25/P26/P27，补「PL-3 已落宿主段」） | 避免与 PL-4 冲突 |
| `bins/game-svr/src/main.rs` | `SessionGround` +3 tick 字段 + `run_frames`；17 处字面量；3 个 drain 函数；调用点；登录初始化；`npc_player_from` 快照/写回；P32 的 53 改走 `push_update`；测试 | 最大写集 |
| `bins/game-svr/src/run_segments.rs`（**新增**）+ `bins/game-svr/src/lib.rs` | P25/P26/P27 纯逻辑 + 单测（仿 `recover.rs`/`stamina.rs`） | 便于构造级判据 |
| `bins/game-svr/src/config.rs` | `GateConfig` + `hunger_system`/`hunger_dec_hp`（+`show_script_action_msg` 若做消息）；解析/默认/测试 | `read_bool_or_false` |
| `bins/game-svr/src/main.rs` 的 `EnterConfig` | 透传上述 bool（red colors 已有） | |
| `crates/envir/src/conf.rs` | `GameSvrConf` +3 个 hunger 键（bool；照 `IniFile` 现有 API） | 保持配置面 |
| `crates/script/src/engine/actions.rs` | 197/198 分支 | |
| `crates/script/src/engine/mod.rs` | 两个 trait setter | **PL-2 同文件** |
| `crates/script/src/engine/dispatch.rs` | 两个 handler 名 | **PL-2 同文件** |
| `crates/script/src/handlers.rs` | `(197,…)/(198,…)` | |
| `bins/game-svr/src/npc.rs` | `NpcPlayer` 4 字段 + setter | **PL-2 同文件** |

**冻结面**：`src/**`、`sql/**`、`MirClient/`、`crates/protocol`、`crates/storage` 一律不碰。不碰 `/e/tmp/aihit` worktree 的 `monai.rs` / `game-svr/src/main.rs` 怪物攻击下行；不碰 `smframe.rs` 既有函数（本批不需要新帧编码；`sm_sysmessage`/`sm_mystatus`/`sm_healthspellchanged_frame` 均已存在）。

### 4.2 精确插入点

`bins/game-svr/src/main.rs::sim_loop`（基线锚点）：

```
14868  let mut rec = tick_world_head_with_specials(...);
14879  drain_player_need_recheck(...);     // P21（已落）
14887  drain_player_revival(...);          // P32（已落）
14893  world.tick_tail(&mut rec);
```

在 **14879 的调用之后、14887 之前**插入（顺序固定）：

```
drain_player_map_hp(...);        // P25
drain_player_hunger(...);        // P26
drain_player_run_rates(...);     // P27
```

留空位：PL-4 的 P22 插在 P21 与 P25 之间；PL-2 的 P31 插在 P27 与 P32 之间。段内**逐玩家按 name 排序**（与 P21/P32 一致），不要按 HashMap 迭代序。

`World::tick` 文档块（`crates/world/src/world.rs` 的 `[tick_head] … 宿主段 … [tick_tail]`）已经把 P21..P32 写进目标序；本批只把注释里的「PL-1 起由 A2 填 P21/P32」改成「PL-1 填 P21/P32、PL-3 填 P25/P26/P27」，不新增 world 侧代码。

测试调用点：现有 P21/P32 单测直接调段函数；P25/P26/P27 也按同一模式（`*_fixture` + `take_frame_idents`），不经过 `sim_loop`。

### 4.3 帧面：同一 Run 的 53 去重（必须实现，推荐 Option Q）

oracle 事实：

- `HealthSpellChanged()` → `SendUpdateMsg(RM_HEALTHSPELLCHANGED)`（`BaseObject.cs:700-709` → `BaseObject.Message.cs:283-303`）：入队前**删除队列里所有同 `wIdent` 的旧消息** ⇒ 同一 Run 内不管调用几次，最终只出**最后一条**，载荷是 handler 执行时的 `WAbil`（`PlayObject.Message.cs:1736-1739`）。
- `SendMsg` 入队优先级 = `(byte)wIdent`（`BaseObject.Message.cs:178` 的 `MsgQueue.Enqueue(sendMessage, (byte)wIdent)`）⇒ 53(68) 先于 `SM_MYSTATUS(708)`、`SM_DURACHANGE(141)`、`SM_SYSMESSAGE(252)`；同优先级保持插入序。
- `SendSocket` 类（P32 的 `SendDelItems`）是 Run 内即时发，早于上述队列帧。

实现（Option Q，推荐）：

1. `SessionGround` 加 `run_frames: Vec<(u16 /*ident*/, Vec<u8> /*body*/)>`。
2. `fn push_update(&mut self, ident: u16, body: Vec<u8>)`：`retain(|(i,_)| *i != ident)` 后 push（模拟 `SendUpdateMsg`）。
3. `fn flush_run_frames(&mut self, tx)`：`stable_sort_by_key(|(i,_)| (*i as u8))` 后逐条发（模拟 `MsgQueue` 的优先级 + 同优先级插入序）。
4. P25：每次 `HealthSpellChanged` → `push_update(SM_HEALTHSPELLCHANGED, sm_healthspellchanged_frame(actor, e.hp, e.mp, e.max_hp))`。
5. P26：`RefMyStatus` 的帧 → `push_update(SM_MYSTATUS, sm_mystatus(status))`；扣血支的 53 同上（P26 每 Run 只会命中其中一支）。
6. P32：把现在**立即发**的 53 改成 `push_update`（DELITEM 仍即时；`RM_DURACHANGE`/`RM_SYSMESSAGE` 也 push，ident 排序自然得到 53 → 141 → 252）。P32 现有测试断言的 `[53, DURACHANGE, SYSMESSAGE]` 由 flush 保持；在测试里补一次 flush（或让 `drain_player_revival` 末尾调用 flush，保持调用契约）。
7. 调用点：三个 PL-3 段与 P32 之后、`tick_tail` 之前，逐会话 `flush_run_frames`。
8. PL-2 的 P31 `GetExp` 53（`IncHealthSpell(2000,2000)`）将来必须走同一 `push_update`，否则会多一条 53（在 PL-2 的简报里互相引用）。

牙（没有它 R 级 review 应拒）：

- `DECHP(1/10)+INCHP(1/7)` 同 Run 两门都过 ⇒ 恰 **1** 条 53，载荷 HP=50（不是两条，也不是 DECHP 的 40）。
- P25 53 + P26 跨档 8102 同 Run ⇒ 帧序 `53 → 8102`（按 ident 优先级）。
- 删掉 `push_update` 的 retain（改普通 push）⇒ 上面第一条必红。

最小替代（不推荐）：每段立即发 53；必须在报告登记「同 ident 未去重 + 中间态载荷」的偏差，并由 owner 接受——R 级下通常会被拒。

### 4.4 串行约束

- PL-1/2/3/4 全部串行（同一 `bins/game-svr/src/main.rs` 宿主段序 + `SessionGround`）。
- **PL-3 ↔ PL-2**：本批额外触碰 `engine/mod.rs`、`engine/dispatch.rs`、`npc.rs` ⇒ 与 PL-2 写集重叠，不能并行；建议 PL-2 先合（或 PL-3 先合，另一方 rebase）。
- PL-3 ↔ PL-4：`SessionGround` 字面量/字段与 P22 的 Bo*/tick 字段同结构；串行。
- 其它线：`crates/world/src/combat.rs`/`entity.rs` 与任何战斗/Entity 线串行；`crates/envir/src/conf.rs`/`config.rs` 与任何配置线串行。
- **不碰** `/e/tmp/aihit` worktree；本批不新增 `smframe.rs` 函数。

### 4.5 建议 commit 顺序（每个 commit 必须自绿）

1. **C1 (R)**：`config.rs` + `envir/conf.rs` + `EnterConfig` 的 hunger 键（默认/部署值测试；不含行为）。
2. **C2 (R)**：`entity.rs` 4 字段 + `win_exp` per-player + 删全局字段 + 回归门/红牙（先锁 golden，避免后续段污染归因）。
3. **C3 (R)**：`SessionGround.run_frames` + `push_update`/`flush` + P32 改造（单独一提交，便于回滚帧面）。
4. **C4 (R)**：`run_segments.rs` 的 P25 纯逻辑 + 宿主 drain + 测试。
5. **C5 (R)**：P26（依赖 C3 的 `run_frames`；含 `RefMyStatus` 接 `recalc_session_ability`）。
6. **C6 (R)**：P27 宿主 drain + 两条 SysMsg + 测试。
7. **C7 (R)**：197/198 脚本动作 + `NpcPlayer` + 写回 + 引擎测试。

---

## 5. 判据草案（每段 >=4 条构造级 + 牙）

> 统一口径：固定时钟（直接传 `now`，不睡）；`CountingRng` 断言抽数次数（本批三段应恒 0）；帧断言用 `take_frame_idents`；HP 断言同时看 `Entity` 与 `SessionGround.state.wabil`。

### 5.1 P25（>=8 条）

1. `DECHP(1/10)`（time=1, point=10）、HP=100 ⇒ `now=tick0+1001` 后 HP=90、`DecHpTick=now`、一条 53；再推进 1000（==1000）⇒ **不触发**（严格 >）。
2. `DECHP(1/10)`、HP=10 ⇒ **不是** `HP>point` ⇒ HP=0（不是 0-10 环绕）。
3. `DECHP(1/10)`、HP=5 ⇒ 0；同一 Run 内 `tick_tail` 结算前 `Death` 仍未置（集成口径）。
4. **tick 初值 0**：新进图（`dec_hp_tick=0`）第一次段调用（now≫0）⇒ **立即**触发；不是 1s 后。
5. `DECHP(3/100)` 解析反转：MapInfo 写 `(3/100)` ⇒ time=3、point=100；Rust `MapInfoFlag` 的既有单测已钉，本段测试夹具直接用 `n_dechptime/n_dechppoint`。
6. **INCHP bug（核心牙）**：`DECHP(1/10)+INCHP(1/7)`、HP=50/MaxHP=100、两门都过 ⇒ HP=**50**（40+10）；改成 `nINCHPPOINT` ⇒ 断言红（47）。
7. **INCHP-only**：point 默认 0 ⇒ HP 不变、但每周期一条 53（证明「无条件调用」）。
8. `INCHP` 死玩家：`Death=true, HP=0, MaxHP=100` ⇒ HP=100，`Death` 仍 true；P32 的 `!Death` 门不再救（同 tick 断言 `death` 未清）。
9. 任一分支触发都是 `HealthSpellChanged` ⇒ `show_hp=true` 时同图 `ref_msg_range` 内每个在线玩家各收一份（收件人集合断言）。
10. RNG：`CountingRng` 抽数 = 0。

### 5.2 P26（>=9 条）

1. `HungerSystem=1`、`HungerStatus=1001`、elapsed=1001 ⇒ 1000；`GetMyStatus` 1→1 ⇒ **零帧、零 Recalc**。
2. `HungerStatus=1000` ⇒ 999（1→0）⇒ 恰一条 `SM_MYSTATUS(708)`、`Param=0`、`Recog=0`；`RecalcAbilitys` 被调用（用重算后 `Entity.max_hp/dc` 变化证明）。
3. `HungerSystem=0` ⇒ 推进 10s：`HungerStatus`、HP、Per*、帧全部不变（生产回归门）。
4. `HungerSystem=1, HungerDecHP=0, HungerStatus=0` ⇒ 每秒只跑门，无 HP/Per* 改动、无 53。
5. `HungerDecHP=1, HungerStatus=0, HP=100` ⇒ 一次门：HP=99、`HealthTick-60`、`SpellTick=max(0,-10)`、`PerHealth/PerSpell` 各 -1、一条 53。
6. **u8 回绕牙**：`PerHealth=0` ⇒ 255；`PerSpell=0` ⇒ 255。
7. **死玩家牙**：`Death=true, HP=0` 仍递减 `HealthTick/SpellTick/Per*` 并发 53（HP=0）。
8. **负值牙**：`HungerStatus=-1001` ⇒ `GetMyStatus()=-1`（C# 向零截断 + 只有 >4 上限）；`HungerStatus` 无下限（1→0→-1）。
9. 门严格：elapsed==1000 不触发；1001 触发；`DecHungerPointTick` 只在触发时刷新。
10. DB：`build_save_record` 的 `hunger_status` 与内存一致。
11. RNG：0 抽。

### 5.3 P27（>=9 条）

1. `KillMonExpRate=200, Time=2`：elapsed 1001 ⇒ time=1、rate=200、零帧；再一次（elapsed 1001）⇒ time=0、rate=100、一条红色 SysMsg（`Recog=actor`、`Param=MakeWord(red_f, red_b)`、GBK 字面量）。
2. `PowerRate=150, Time=1` ⇒ 一次门后 rate=100 + 一条 SysMsg；两条同时到期 ⇒ 帧序「经验 → 攻击」。
3. **Time=0 永久牙**：`rate=200, time=0` ⇒ 推进 10s，rate 仍 200、rate_time 仍 0（未过期）。
4. 门严格 + `ExpRateTick` 无条件刷新：elapsed==1000 不触发、不刷新；1001 触发并刷新（即使两个 time 都是 0）。
5. 单步牙：一次 `now` 跳 5001ms ⇒ time 只减 **1**（`if` 非 `while`）。
6. `win_exp` rate=100 ⇒ 旧数字（参数化表）；rate=200 ⇒ ×2；rate=0 ⇒ 0。
7. 队伍 per-member：A=100/B=200 同一次击杀 ⇒ B 的 `exp_awards` 金额是 A 的两倍。
8. 脚本动作：`KILLMONEXPRATE 200 5` ⇒ Entity 两字段；负值 (`-1`/`-1`) ⇒ `ScriptActionError`、字段不变；`POWERRATE 150 0` ⇒ 永久；`200 0` 永久。
9. `win_exp` 改读后 `sha256sum tests/parity/world/golden/*.txt` 不变（回归门）。
10. RNG：0 抽。

### 5.4 真机窗口标注（review 必写）

- P25：`window_required: true`；`MapInfo.txt` 0 条 `DECHP/INCHP`、`SETMPAMODE` 0 文件 ⇒ 无对象；owner 裁决「构造级 + 逐行/类型 + 内容 grep」替代。
- P26：`window_required: true`；`HungerSystem=0/HungerDecHP=0`（`Server.conf:396/398`）⇒ 无对象；同上。
- P27：`window_required: true`；`KILLMONEXPRATE/POWERRATE` 0 文件 ⇒ 数值分支无对象；但 `rate=100` 的既有数字可由 golden 机判全绿（这是本批唯一可机判的等价证据）。
- 三个 R 级卡合并后 `status != closed`（保持 in-progress，等窗口或 owner 裁决）——按 AGENTS.md 判据分级。

---

## 6. 已知偏差 / 登记（不得静默）

1. **200ms vs 50ms 节拍（D02，已登记）**：oracle 每 50ms 一次 `Run()`；P27 的 `>1000` 门实际每 **1050ms** 触发一次，Rust 宿主 200ms ⇒ 每 **1200ms**。同理 P26、P25 的门。直接 `now_ms()` 比较会继承这个既有 D02 偏差；若 owner 要求 oracle 精确节拍，需像 `stamina.rs` 那样按 50ms 边界重放（本批不做，登记）。
2. **恢复段顺序**：oracle 恢复（70-142）在 P21/P25/P26 之前；Rust `drain_recovery` 目前在 `tick_tail` 之后（PL-1 已落形态）。P25/P26 与恢复同 tick 时 HP 轨迹/53 帧会与 oracle 差（构造可达、生产不可达）。spike §4.2 已建议把恢复段移到宿主段之前；本批**不移动**（blast radius 跨 PL-1），登记并建议另立卡 `openmir2-run-recovery-order`。若 review 要求本批解决，先做「恢复状态前移 + 53 统一」再落 P25/P26。
3. **`Environment.TickCount` int 回绕**：C# i32（~24.8 天）vs Rust i64 单调；所有时间门共有的既有偏差。
4. **跨批 53 去重**：本批的 `run_frames` 只覆盖 P25/P26/P32；恢复段/升级段的 53 仍独立发送 ⇒ 同一 oracle Run 内可能多一条（生产不可达组合）。另立卡或在 PL-2 一并收口。
5. **脚本自消息未排水**：`NpcPlayer::sys_msg` 默认空实现、宿主只 drain says ⇒ 197/198 的确认消息（`ShowScriptActionMsg=1`、`%g/%d` 字面量）发不出去；部署 0 使用，登记。
6. **`SETMPAMODE DECHP` 未实现**：Rust 无 `ActionOfSetMapMode` ⇒ P25 的 DECHP 只有 MapInfo 一条可达路径（仍为 0）；INCHP 本来就只有 MapInfo。另立卡。
7. **`HungerDecPower` 死键**：照抄声明/读取，但无消费者；不要为它造行为。

---

## 7. 复现命令（本简报全部行号）

```sh
# oracle P25/P26/P27 全文
sed -n '569,657p' src/M2Server/Player/PlayObject.Message.cs
# 字段/ctor
grep -n 'HungerStatus\|DecHungerPointTick\|KillMonExpRate\|PowerRate\|ExpRateTick\|DecHpTick\|IncHpTick' src/M2Server/Player/PlayObject.cs
sed -n '887p;941,943p' src/M2Server/Player/PlayObject.cs
# SendUpdateMsg 去重 + 优先级
sed -n '283,304p' src/M2Server/Actor/BaseObject.Message.cs
sed -n '168,180p' src/M2Server/Actor/BaseObject.Message.cs
# INCHP bug 证据：nINCHPPOINT 无消费者
grep -rn 'nINCHPPOINT' src --include=*.cs
grep -rn 'nDECHPPOINT' src --include=*.cs
# MapInfo 解析反转
sed -n '198,210p' src/GameSrv/Maps/Maps.cs
grep -n 'Backslash =' src/OpenMir2/HUtil32.cs
# 饥饿配置
sed -n '501,503p;1505,1507p' src/Modules/SystemModule/Conf/Model/GameSvrConf.cs
sed -n '282,284p' src/Modules/SystemModule/Conf/ServerConf.cs
grep -a -n 'HungerSystem\|HungerDecHP\|HungerDecPower\|ShowScriptActionMsg' /e/MirServer/M2GameSvr/Server.conf
# 部署不可达
wc -l /e/MirServer/M2GameSvr/Envir/MapInfo.txt
grep -a -ic 'DECHP\|INCHP' /e/MirServer/M2GameSvr/Envir/MapInfo.txt
for c in SETMPAMODE SETMAPMODE KILLMONEXPRATE POWERRATE; do echo -n "$c="; grep -a -ril "$c" /e/MirServer/M2GameSvr/Envir --include=*.txt | wc -l; done
# Rust 读点
grep -rn 'kill_mon_exp_rate' mir2-rs --include=*.rs
grep -rn 'power_rate' mir2-rs/crates/world/src/entity.rs mir2-rs/crates/world/src/combat.rs
# 帧/会话/插入点
grep -n 'drain_player_need_recheck(\|drain_player_revival(' mir2-rs/bins/game-svr/src/main.rs
grep -n 'pub struct Recovery\|pub struct Stamina\|hunger_status:' mir2-rs/bins/game-svr/src/recover.rs mir2-rs/bins/game-svr/src/stamina.rs mir2-rs/bins/game-svr/src/main.rs
```

---

## 8. 残留物

- 本件只写 `/e/tmp/laneC-pl3.md`；未建 worktree、未改仓库、未起服务/客户端、未碰 `E:/MirServer`（仅 `grep -a`/`wc` 只读）。
- 未跑 `cargo`（本车道只读产出）。


---

## 评审员更正（2026-10-19，**以本节为准**）

- **P25/P26/P27 的 oracle 事实全部支持**（INCHP 漏换字段、MapInfo point/time 反转、`DecHpTick/IncHpTick` ctor=0、
  `HungerStatus/1000` 向零截断、P27 严格 `>1000` 且过门即刷新、`win_exp` 唯一读点 `combat.rs:1399` 与 per-member）。
- **§0.6/§4.3 的 `SendUpdateMsg` 语义错误**：不是「删除队列里所有同 wIdent」——只 peek 队首 + `Count<=i` 预算
  ⇒ 必须按头-only 模型，或显式登记为简化。
- **「同优先级保持插入序」错误**：.NET `PriorityQueue` 同优先级不稳定出队。
- **SM/RM ident 混用**：排序键必须是 **RM ident 的低字节**（例：`RM_HEALTHSPELLCHANGED=68 → RM_DURACHANGE=141 →
  RM_MYSTATUS=166 → RM_SYSMESSAGE=252`），不能拿 SM ident 排。
- **P25-8 / P26-8 判据、handlers.rs 事实、`E:/MirServer` 证据越界**需按评审原文修正。
