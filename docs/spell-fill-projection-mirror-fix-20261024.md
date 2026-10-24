# spell_fill 恢复路径投影镜像修复（卡 `openmir2-spell-fill-projection-mirror`，tier R）

- **分支**：`card/spell-fill-projection-mirror`，base = `walgit/master` `67714217f`（含 mp-session 合并）
- **来源**：reviewer-3 review `ac6a585866`（卡 `openmir2-mp-session-projection-clobber`）F2（Important）+ 自造牙 tooth3
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
| `lib.rs` P21 `drain_player_need_recheck_with_magic` | 每 tick 无条件 `sg.state.wabil.hp = e.hp; sg.state.wabil.mp = e.mp` | 读实体权威（不动） |

⇒ 恢复写投影后，**下一 tick P21 把恢复擦回实体旧值**。reviewer tooth3 交错序列实测 left:15/right:17（cast1 17→15 → 恢复 +2 → P21 ⇒ 投影 15 ≠ oracle 17；其后 cast2 13/存档 13 ≠ oracle 15/15）。

**hp 侧对称面核查**：oracle 无单侧意图（单份 `WAbil`，`IncHealthSpell` 明确双写）⇒ `drain_stamina` 只回写 `e.hp`、`drain_recovery` 不回写，均属同族单写，**同族同修**（补 `e.mp`；`drain_recovery` 连 hp 一起补）。无「确有单侧意图」的例外可引。

## 2. 修法（沿线 mp-session 先例「每个写点同步」）

1. `drain_stamina` 的 `!runs.is_empty()` 回写段：`e.hp=…` 旁补 `e.mp = sg.state.wabil.mp;`（P7 MP 支 / 负值支同段覆盖）；
2. `drain_stamina` 的 P8 钳制回写段：同样补 `e.mp`（mp_over 支生产当前不可达，登记不静默）；
3. `drain_recovery`：`recovery_tick` 命中后 `e.hp = …; e.mp = …;` 双侧镜像；签名 `&World → &mut World`（6 个调用点 + 1 处源码序断言同步更新）；
4. `stamina.rs` / `recover.rs` 模块头补「写点同步义务」注记；`drain_stamina` 头部过时的「mp 不在世界实体里」注释更正。

P21 / P32 / RefMyStatus / SuperMan 刷回与施法 wrapper **零改动**；C# 零改动；冻结 fixture 零触碰。

## 3. 合同测试（acceptance ②）

- `spell_recovery_interleaved_p7_stamina_survives_p21_tags_15_17_15_save_15`（P7 腿，tooth3 场景生产化）：cast1 17→15（Tag=15）→ P7 MP 支 +2 ⇒ 17（Tag=17）→ P21 仍 17 → cast2 17→15（Tag=15）→ 存档 MP=15。时序用 `SpellTick=798 + SpellFillTime=800` 的「到量清零」实现**抗停顿的恰一次恢复**。
- `spell_recovery_interleaved_p1_pool_survives_p21_hp_15_mp_16_save`（P1 池腿 + hp 侧对称面）：低血 (10,17) → cast1 15 → P1 `IncHealthSpell(5,5)` ⇒ (15,18)（Param/Tag 同帧 15/18）→ P21 仍 (15,18) → cast2 18→16（Tag=16）→ 落库 HP=15 / MP=16。

全链走生产入口 `handle_spell_cmd` / `drain_stamina` / `drain_recovery` / `flush_healthspell_slots` / `drain_player_need_recheck` / `build_save_record`。

## 4. 牙（acceptance ③，各自实测红后还原）

| 牙 | 变异 | 实测红 |
|---|---|---|
| ① | 单删 `drain_stamina` 回写段的 `e.mp` 镜像 | P7 腿：实体 15 ≠ 17（`left:15/right:17`，恰 F2 原值）；Tag 序列 [15,17,13]、存档 13 |
| ② | 单删 `drain_recovery` 的整个镜像块 | P1 腿：实体 hp 10 ≠ 15 被 P21 擦回 |
| ③ | 只删 `drain_recovery` 镜像块的 `e.hp` 一行 | P1 腿：hp 10 ≠ 15（mp 腿仍绿 ⇒ 单侧删除可定位） |

## 5. 门禁（全 rc=0，本机实跑）

`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace`（**3171 passed** = 基线 3169 + 2 新合同测试，141 组）；`cargo run -p msgcodegen -- --check`（604 条）；`tools/whitelist/t_reservation_gate.py check`（255 行/255 号）；`conflict_marker_gate`（0 marker/7494 文件）；`stale_pointer_gate`（OK）；`dynamic_count_stamp_gate`（OK）；`git diff --check` / `--cached --check`；`git merge-tree --write-tree walgit/master …` = 树 `38c2b2515`。

## 6. 残留物清单

- 临时物：`.edittmp/`（变异脚本与分段文本）、`.testout.txt` 已删；worktree 内无本线专属残留；
- 未建模（登记，不静默）：P8 的 mp_over 钳制在**当前生产写点集合下不可达**（MaxMP 只在装备重算时收缩；镜像按「每个写点同步」补齐）；
- R 级窗口项：恢复交错序列（P7/P1 两种）真机复拍，与 `openmir2-mp-session-projection-clobber` 同轮窗口执行后两卡一并 closed。
