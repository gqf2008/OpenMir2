# spell_fill 恢复路径投影镜像修复（卡 `openmir2-spell-fill-projection-mirror`，tier R）

- **分支**：`card/spell-fill-projection-mirror`，base = `walgit/master` `67714217f`（含 mp-session 合并）
- **来源**：reviewer-3 review `ac6a585866`（卡 `openmir2-mp-session-projection-clobber`）F2（Important）+ 自造牙 tooth3；
  第 2 轮 review `94159ca7d1` F1（Important，request_changes）：同族第三个 `IncHealthSpell` 写点 CM_EAT 未镜像
- **性质**：F2 残余的生产修复；tier=R（战斗数值 + 持久化）⇒ 合并后 in-progress，窗口复拍（与 mp-session 卡同轮）后 closed

## 1. oracle 语义推导（acceptance ①）

C# 只有**一份** `BaseObject.WAbil`（字段在 `BaseObject` 上），三条恢复路径全部写同一对象：

| 路径 | C# 证据 | 写入 |
|---|---|---|
| 自然恢复（每 50ms `Run()`） | `BaseObject.Base.cs:44-56` HP 支（`MaxHP/75+1` 夹 `MaxHP`）/ `:57-68` MP 支（`MaxMP/18+1` 夹 `MaxMP`），随后 `HealthSpellChanged()` | `WAbil.HP` / `WAbil.MP`（同对象） |
| `IncHealthSpell(nHp,nMp)`（P1 池消费 / GetExp 尾 / 药品） | `BaseObject.cs:952-975`（`:956` HP 支、`:966` MP 支） | `WAbil.HP` 与 `WAbil.MP` **双写** |
| MP 石 | `CharacterObject.cs:206-224`（`IncSpell += dCount` 入池，再由 `PlayObject.Message.cs:92-111` 消费 ⇒ `IncHealthSpell`） | 入池 + 上述双写 |

`PlayObject.Message.cs:70-141`（P1 池）：`nHP/nMP = PerHealth/PerSpell`（`CharacterObject.cs:83-86` 初值 5）⇒ `IncHealthSpell(nHP,nMP)`（`:126`）。恢复后 oracle 的**下一个读者**（再施法读 `WAbil.MP`、`MakeSaveRcd` 落库读同一份、下一 `Run()` 的门）看到的都是恢复后的值 ⇒ **恢复永存，无双份语义**。

Rust 现状（修复前）逐点：

| 写点 | 行为 | oracle 对齐? |
|---|---|---|
| `stamina.rs` `stamina_tick`（P7 MP/HP 支、负值支） | 只写调用方传入的 `&mut sg.state.wabil` | 投影内对齐；**实体不镜像** |
| `lib.rs drain_stamina` 回写段 | 只 `e.hp = sg.state.wabil.hp`（注释称「mp 不在世界实体里」——已过时，`Entity.mp/max_mp` 在 `entity.rs:754-755`） | **mp 单写缺口** |
| `recover.rs` `recovery_tick` / `lib.rs drain_recovery` | 只写 `sg.state.wabil`，实体零回写 | **hp/mp 双缺口** |
| `lib.rs handle_eat_cmd`（CM_EAT 药品写点，review `94159ca7d1` F1） | `sg.state.wabil = state.wabil` 整体回写后**不写实体**；写点来源 `useitem.rs:306`（StdMode 0 Shape 1 `IncHealthSpell(AC,MAC)`）/ `:329`（Shape 3 百分比）/ `:360`（StdMode 1 食物 recalc） | **hp/mp 双缺口**（冻结内容 太阳水 `sql/mir2_data.sql:1013`、强效太阳水 `:1014` 生产可达） |
| `lib.rs` P21 `drain_player_need_recheck_with_magic` | 每 tick 无条件 `sg.state.wabil.hp = e.hp; sg.state.wabil.mp = e.mp` | 读实体权威（不动） |

⇒ 恢复写投影后，**下一 tick P21 把恢复擦回实体旧值**。reviewer tooth3 交错序列实测 left:15/right:17（cast1 17→15 → 恢复 +2 → P21 ⇒ 投影 15 ≠ oracle 17；其后 cast2 13/存档 13 ≠ oracle 15/15）。
**药品支（第 2 轮补）**：CM_EAT 同一缺口——review `94159ca7d1` 反例牙实测「药后投影 (15,15) / 实体 (5,5) / P21 后 (5,5)」，oracle 应 (15,15)。`IncHealthSpell` 的三个生产调用者（GetExp 尾 / P1 池 / **吃药**）至此全部在写点同步；oracle「恢复永存」结论对三支均成立（修复后）。

**hp 侧对称面核查**：oracle 无单侧意图（单份 `WAbil`，`IncHealthSpell` 明确双写）⇒ `drain_stamina` 只回写 `e.hp`、`drain_recovery` 不回写，均属同族单写，**同族同修**（补 `e.mp`；`drain_recovery` 连 hp 一起补）。无「确有单侧意图」的例外可引。

## 2. 修法（沿线 mp-session 先例「每个写点同步」）

1. `drain_stamina` 的 `!runs.is_empty()` 回写段：`e.hp=…` 旁补 `e.mp = sg.state.wabil.mp;`（P7 MP 支 / 负值支同段覆盖）；
2. `drain_stamina` 的 P8 钳制回写段：同样补 `e.mp`（mp_over 支生产当前不可达，登记不静默）；
3. `drain_recovery`：`recovery_tick` 命中后 `e.hp = …; e.mp = …;` 双侧镜像；签名 `&World → &mut World`（6 个调用点 + 1 处源码序断言同步更新）；
4. `stamina.rs` / `recover.rs` 模块头补「写点同步义务」注记；`drain_stamina` 头部过时的「mp 不在世界实体里」注释更正；
5. **（第 2 轮补，review `94159ca7d1` F1）** `handle_eat_cmd` 会话回写段后补 hp/mp 双侧实体镜像
   （guard 保留：纯函数测试夹具无实体）——一处覆盖 Shape 1 / Shape 3 / StdMode 1 recalc 三个写点来源。

P21 / P32 / RefMyStatus / SuperMan 刷回与施法 wrapper **零改动**；C# 零改动；冻结 fixture 零触碰。

## 3. 合同测试（acceptance ②）

- `spell_recovery_interleaved_p7_stamina_survives_p21_tags_15_17_15_save_15`（P7 腿，tooth3 场景生产化）：cast1 17→15（Tag=15）→ P7 MP 支 +2 ⇒ 17（Tag=17）→ P21 仍 17 → cast2 17→15（Tag=15）→ 存档 MP=15。时序用 `SpellTick=798 + SpellFillTime=800` 的「到量清零」实现**抗停顿的恰一次恢复**。
- `spell_recovery_interleaved_p1_pool_survives_p21_hp_15_mp_16_save`（P1 池腿 + hp 侧对称面）：低血 (10,17) → cast1 15 → P1 `IncHealthSpell(5,5)` ⇒ (15,18)（Param/Tag 同帧 15/18）→ P21 仍 (15,18) → cast2 18→16（Tag=16）→ 落库 HP=15 / MP=16。

- `eat_sun_water_shape1_shape3_survive_p21_and_save_hp70_mp140`（**第 2 轮补，review F1**）：
  太阳水 StdMode 0 Shape 1（AC=30/MAC=40）(10,20)/max(100,200) ⇒ `IncHealthSpell(30,40)` ⇒ (40,60)，
  53 Param/Tag=40/60 → P21 不擦回 → Shape 3 百分比 `Round(100/100*30)/Round(200/100*40)` ⇒ (70,140)，
  53 Param/Tag=70/140 → P21 → `build_save_record` 落库 **HP=70 / MP=140**。

全链走生产入口 `handle_spell_cmd` / `handle_eat_cmd` / `drain_stamina` / `drain_recovery` / `flush_healthspell_slots` / `drain_player_need_recheck` / `build_save_record`。

## 4. 牙（acceptance ③，各自实测红后还原）

| 牙 | 变异 | 实测红 |
|---|---|---|
| ① | 单删 `drain_stamina` 回写段的 `e.mp` 镜像 | P7 腿：实体 15 ≠ 17（`left:15/right:17`，恰 F2 原值）；Tag 序列 [15,17,13]、存档 13 |
| ② | 单删 `drain_recovery` 的整个镜像块 | P1 腿：实体 hp 10 ≠ 15 被 P21 擦回 |
| ③ | 只删 `drain_recovery` 镜像块的 `e.hp` 一行 | P1 腿：hp 10 ≠ 15（mp 腿仍绿 ⇒ 单侧删除可定位） |
| ④（第 2 轮补） | 单删 `handle_eat_cmd` 的实体镜像块 | CM_EAT 腿：实体 (10,20) ≠ (40,60)（`left:(10,20)/right:(40,60)`，恰 F1 形状） |

## 5. 门禁（全 rc=0，本机实跑）

`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace`（**3172 passed** = 基线 3169 + 本卡 3 条合同测试，141 组）；`cargo run -p msgcodegen -- --check`（604 条）；`tools/whitelist/t_reservation_gate.py check`（255 行/255 号）；`conflict_marker_gate`（0 marker/7495 文件）；`stale_pointer_gate`（OK/161 docs）；`dynamic_count_stamp_gate`（OK）；`git diff --check` / `--cached --check`；`git merge-tree --write-tree walgit/master <head>` = 钉住 head 的树（N1 更正：第 1 轮报告写的 `38c2b2515` 是 docs 提交前取数，应为当时 head `c8b7663b6` 的树 `67d6e0041`；本轮以第 2 轮 head 实跑值为准，见 patch/status 条目）。

## 6. 残留物清单

- 临时物：`.edittmp/`（变异脚本与分段文本）、`.testout.txt` 已删；worktree 内无本线专属残留；
- 未建模（登记，不静默）：P8 的 mp_over 钳制在**当前生产写点集合下不可达**（MaxMP 只在装备重算时收缩；镜像按「每个写点同步」补齐）；
- R 级窗口项：恢复交错序列（P7/P1 两种 + CM_EAT 药品）真机复拍，与 `openmir2-mp-session-projection-clobber` 同轮窗口执行后两卡一并 closed；
- 第 2 轮（review `94159ca7d1` F1）已闭合 `IncHealthSpell` 三个生产调用者（GetExp 尾 / P1 池 / CM_EAT 药品）的写点同步；§1 的「药品」行结论已按实测更正。
