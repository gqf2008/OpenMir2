# MG-52 设计报告 — 43 破空剑三面收口（2026-11-27，openmir2-08）

> 基线 master `8892d50da`，分支 `card/mg52-poxongjian`。对账结论已由 opc-27 采信；
> 本报告回答批前问题（`SpellTick -= 450` 作用域）并给出产品笔/装置笔设计。
> 判级建议 **L**（模型线有真机对拍背书），tier 由 Leader 终裁。

## 0. 结论速览

- 清单 #4 三面：**MP 数值 = 已覆盖**（oracle 语义即「不扣」，现有断言已对）；
  **MBo43Kill 消费 = 已覆盖**（oracle 零消费读点，同构缺省）；
  **施法包装层 = 拆两半**——限速门家族是 **oracle 死代码**（登记修正即可），
  `SpellTick -= 450` 是唯一真实产品缺口。
- 作用域答案：`SpellTick -= 450` 作用域 = **全部 CM_SPELL 施法**（战士 case 臂与
  `default` 支 `DoSpell` 法术同价、每次施法恰一次），法术路径吃的是**同一处**减量，
  **无第二缺口**；本卡接线点在臂派发前，两侧一次修完，不缺半截。
- 产品改动实体：`stamina` 加 1 个方法 + 施法派发守卫后加 1 处调用 + 3 处登记修正。

## 1. 对账定案（证据链）

| 面 | 判定 | 证据 |
| --- | --- | --- |
| ① MP 数值 | 已覆盖 | oracle `case SKILL_43`（`src/M2Server/Player/PlayObject.Attack.cs:1263-1279`）不调 `GetSpellPoint`/`DamageSpell` ⇒ toggle 构造性恒不扣 MP；golden `warr-43.json` `Fixture.caster_mp=100` 已进比对面；Rust `warr_43_behavior_side_effects` 断言 `mp_after == mp_before == caster_mp`。清单「MP 数值未比对」实为已比对，oracle 值即「不扣」。 |
| ② 限速门 | 同构缺省（oracle 死代码） | `CloseSpeedHackCheck` 构造默认 `true`（`src/Modules/SystemModule/Conf/Model/GameSvrConf.cs:1920`）；`GameSvrConf` 仅构造实例化、无文件加载（`src/Modules/SystemModule/SystemShare.cs:191` `Config = new GameSvrConf()`）；字段全仓无配置读点 ⇒ 门条件 `!CloseSpeedHackCheck` 恒假。`CheckActionStatus`（`src/M2Server/Player/PlayObject.Base.cs:2662-2760`）、`MagicAttackTick`/`MagicAttackInterval`/`MagicAttackCount`/`OverSpeed` 踢线/`SendSelfDelayMsg` 延迟重投全部只被死门读写 ⇒ **不可达**。43 ∉ `IsWarrSkill`{3,4,7,12,25,26,27,34,38}（`src/Modules/SystemModule/Const/MagicConst.cs:16-166`）——若门活着会适用 43，但门在本 fork 不可达。窗口里生效的超速踢来自 GameGate 进程自己的 `SessionSpeedRule`（另一二进制、另一机制）。 |
| ② `SpellTick -= 450` | **未覆盖，需接线** | `src/M2Server/Player/PlayObject.Attack.cs:1109-1110`，在死门块之外、守卫（`:1057-1070`）之后、`switch` 之前**无条件执行**；语义 = 施法把回蓝 accumulator（`BaseObject.SpellTick`）倒拨 450（`max(0,…)`），可观测面 = 回蓝阈值（`SpellFillTime` 默认 800，`GameSvrConf.cs:1324`）穿越推后。Rust `stamina.rs` 已有 attack(`-100/-30`)/walk(`-10`)/run(`-60/-10`)/struck(清零) 全套变体与 50ms 重放模型，唯独无 spell 变体、无调用点。 |
| ③ MBo43Kill 消费 | 已覆盖（同构缺省） | oracle 全仓读点仅 2 处：toggle 判定（`PlayObject.Attack.cs:1256`）+ 开关文案（`PlayObject.Base.cs:643`）；攻击结算零读点（大小写变体已扫）。Rust 写点在 `warrskill` 43 臂、无消费者 = 同构；`MagicState::mbo43_kill` 字段注释已登记「同样无消费者」。 |

handoff 风险三条的归宿：#1（限速面 golden 不可达）坐实且升级——生成器
`tests/parity/csharp/MagicGolden/ClientSpellXYEntry.cs` 无任何 tick/限速复现，且限速门
本身是死代码；#2（消费点扩写集到 attack 线）排除；#3（450ms 连点门要虚拟钟）消解
——43 在部署 oracle 没有活的连点门。

## 2. 作用域判定（批前问题的答案）

**`SpellTick -= 450` 的作用域 = 全部 CM_SPELL 施法，法术路径吃同一处，无第二处减量。**

1. `ClientSpellXY` 全仓**唯一调用点** = CM_SPELL 处理器
   （`src/M2Server/Player/PlayObject.Message.cs:1340`）。
2. `-450` 位于 `switch (UserMagic.MagIdx)` **之前** ⇒ `default` 支（`DoSpell` 法术）
   与 8 条独立 case 臂**同价**：每次通过守卫的 CM_SPELL 恰扣一次，与臂内成败无关
   （26 蓝不足 639、default 臂失败，-450 都已发生）。
3. `DoSpell` 侧无第二处写点：全仓 `SpellTick` 写点枚举（grep 全量）——
   回复循环 `BaseObject.Base.cs:42/57/78/80`；挖矿 `Attack.cs:899`(-50)；
   攻击 `:956`(-100)；骑跑 `:1042`(-10)；**施法 `:1109`(-450)**；跑步 `:1353`(-10)；
   饥饿 `PlayObject.Message.cs:616`(-10)；受击清零 `:1588`；
   buff 清零 `ActorBuffSystem.cs:254`；怪物 AI 计时 `FireballMonster.cs`（异义复用）。
   **`MagicManager.cs` 与 `PlayObject.Base.cs`（`PlayObject.DoSpell` 包装层
   `:1505-1531`）零出现**。
4. Rust 侧人口一致：`magicwire::is_spell_ident` 仅 `CM_SPELL`（`CM_MAGICKEYCHANGE`
   1008 独立分流，oracle 换键路径不碰 `SpellTick`）⇒ `handle_spell_cmd_with_magic`
   的人口 = oracle `ClientSpellXY` 的人口。

⇒ **接线点（守卫后、臂派发前）覆盖战士臂 + default 臂，法术侧与战士侧同一笔修完**，
不存在「法术侧另缺口」，无需拆卡单独登记。

## 3. 产品笔设计

### 3.1 `stamina.rs`：新方法（唯一产品逻辑）

```rust
/// 施法包装层（`src/M2Server/Player/PlayObject.Attack.cs:1109-1110`）：
/// `SpellTick -= 450; SpellTick = _MAX(0, SpellTick)`。
/// 作用域 = 全部 CM_SPELL 施法（8 条 case 臂与 `default` 支 DoSpell 法术同价，
/// 每次恰一次；与臂内成败无关——26 蓝不足 639 / default 失败都已扣）。
/// 不触 `health_tick`（oracle 只动 SpellTick）。
pub fn on_spell_cast(&mut self) {
    self.spell_tick = (self.spell_tick - 450).max(0);
}
```

命名取 `on_spell_cast` 而非 `on_spell_success`：oracle 在守卫通过后**无条件**执行，
与该次施法最终成败无关，`success` 会误导。

### 3.2 `lib.rs`：接线点（一处调用）

`handle_spell_cmd_with_magic` 内，`magicwire::spell_reject` 守卫块之后、
`warrskill::arm` 派发之前：

```rust
// oracle `ClientSpellXY`（PlayObject.Attack.cs:1109-1110）：守卫后、switch 前无条件
// 执行；作用域与人口论证见 docs/issues/mg52-spelltick450-design-20261127.md §2。
if let Some(sg) = ground_sessions.get_mut(chr_name) {
    sg.stamina.on_spell_cast();
}
```

短作用域借用，不影响后续 `ground_sessions` 使用；无地面会话时不扣（与臂的 WRN
路径同语义，oracle 无此态）。

### 3.3 登记修正（3 处，注释级）

1. `magicwire.rs` 模块头「8 支共用的包装层」条目：拆写——限速门 = oracle 死代码
   （`CloseSpeedHackCheck` 恒 true 证据链）⇒ 同构缺省不接；`SpellTick -= 450` 已接
   （指本卡）。同条的 `LateDelivery` 登记随之改写：门块恒不进入 ⇒
   `boLateDelivery` 取值无行为后果（原「按直通并登记」偏差失效）。
2. `lib.rs` 上方 `效果体限速 / SpellTick -= 450 / MP 消耗仍按 magicwire 模块头登记未接`
   一行：与 magicwire 头现状不符（MP 消耗已接）——顺带修正为仅「效果体限速 =
   死代码定性」。
3. `attackwire.rs` / `movewire.rs` 模块头的同源「限速未接」登记：同一死代码定性
   适用，但属攻击/移动线登记文本——**本卡不动**，报 Leader 决定（另开一行注释
   登记小卡，或搭其它攻击线卡顺手改）。

## 4. 装置笔设计（测试与牙）

### 4.1 `stamina.rs` 单测（4 条，随方法同文件）

1. **空输入/退化**（牙要回答空输入）：`spell_tick=0` → cast → `0`（clamp 不产生负值）。
2. **精确减量**：`800→350`、`500→50`、`450→0`、`449→0`（边界两侧都钉）；
   且 `health_tick` 全程不变。
3. **回蓝推后对照**（可观测面）：`SpellFillTime=800`、预置 `spell_tick=796`；
   A 支不施法 → `stamina_tick` 推进 +100ms（2 个 Run，+4 ⇒ 800 ≥ 800）MP 恢复；
   B 支先 `on_spell_cast`（796-450=346）→ 同样 +100ms MP 不动（350 < 800）。断言两侧分叉。
4. **互斥面**：`on_spell_cast` 后再 `on_struck` ⇒ 清零照旧（与既有变体共存）。

### 4.2 集成牙（`warrskill_parity_tests` rig，走全派发）

预置 `sg.stamina.spell_tick = 800` → 经 `handle_spell_cmd_with_magic` cast 43 →
断言 `sg.stamina.spell_tick == 350`；同断言既有不变式不破（`mp_after == mp_before`、
帧面不变）。该 rig 现就是走全派发驱动的（非直调 `warrskill::run`）⇒ 牙钉的是
**接线点本体**，不是臂内行为。

### 4.3 变异牙（改坏必红，重放纪律）

- 变异 ①：删 `sg.stamina.on_spell_cast()` 调用 ⇒ 集成牙红（`800 != 350`）。
- 变异 ②：`450 → 0` ⇒ 单测精确减量红。
- 纪律：变异只在本人 worktree 跑；恢复后断言 `sha` 相等且无 `MUTATION` 残留；
  每次变异断言 `passed+failed ≥ 1`（证用例真跑了）；**定稿后对交付 head 全量重放**
  全部变异。

## 5. 不做什么（防静默扩界）

- 限速门家族（`CheckActionStatus`/`MagicAttackTick`/`SendSelfDelayMsg` 重投/
  `OverSpeed` 踢线）：不接——oracle 死代码，实现它才是偏离部署 oracle。
- golden 不动：生成器无 wrapper/tick 层，限速与 -450 结构性 golden 不可达，不硬造。
- 虚拟钟不需要：断言面是 accumulator 值；`stamina_tick` 的时钟本来就是显式参数。
- `MBo43Kill` 不加消费点（oracle 零读点）；MP 断言不改（已对）；
  `warr-43` 既有两用例与 golden 不动（golden 只追加纪律不触发）。

## 6. 判级 L 理由与窗口项

- `stamina` 模型线已有真机对拍背书（86 击/`20668` 证据包，见 `stamina.rs` 头）；
  本卡增量 = 同一模型上的纯减量接线，无新口径、无新帧面。
- 继承既有窗口登记（53 帧早 0-50ms / 跨 tick 替换未建模）——-450 不新增窗口项。
- 如需 N=1 复验：窗内施法后 MP 恢复 53 帧时序对比（-450 使恢复时刻推后
  450-points/40-per-sec ≈ 11.25s，预置满 accumulator 才可观测）；现不排窗，登记即可。

## 7. 验收命令

```bash
cd mir2-rs && cargo fmt --all -- --check
cd mir2-rs && cargo clippy --workspace --all-targets -- -D warnings   # 报告必贴 rc
cd mir2-rs && cargo test --workspace                                  # 报告贴 running/passed/ignored 三元组
cd mir2-rs && cargo run -p msgcodegen -- --check
python -I -B tools/hygiene/docs_anchor_gate.py --prose --base master
```

测试集合差：`--list` 前后对称差 = +6（stamina 4 + 集成 1 + 对照支并入单测计），
以实际输出为准；门与证据跑在待合并同树（`Cargo.lock` 有变随笔提交）。

## 8. 批次结构

1. 产品笔：`stamina.rs` 方法 + 4 条单测；`lib.rs` 接线 + 集成牙 + 登记修正 3 处
   （1/2 处）。单独成笔，全量 workspace 门。
2. 变异笔：两变异重放 + 恢复核验（red 证据 + sha 断言）。
3. 文档笔：本文件随产品笔入库；`attackwire`/`movewire` 登记修正的去向由 Leader 定。
