# openmir2-parity-assert-frozen-constants — 断言常量化 + 静默默认清理（follow-up 卡）

**状态**：草案落地（**只读盘点 + 文档**；未改产品代码、未跑构建）。
**扫描 head**：master `9ff231ba7`（本卡行号均对此 head）。
**来源**：`3b` 世代卡复核发现同族问题；`snowwind` 的 `TargetHpBefore==100`/`TargetHp==85` 是已腐烂实例（已修，见 §2）。

## 1. 判据（两族同根：缺省值代替显式输入）
> **A 数值面常量化**：只有「帧序 / 帧数」这类语义不变式可以写成常量；凡是从 golden 抄来的数值面（HP / 伤害 / series / DrainPasses / Rng 长度…）必须从 golden 取。
>
> **B 静默默认**：`golden_fixture_opt(...).unwrap_or(<默认>)` 把**缺字段**静默成默认值，与审计 §7「缺字段必须报错、不得默认」冲突；应改 `golden_fixture_required`（已存在，`lib.rs:72463`）。

## 2. 已修（原 11 条中的 4 条）—— 对 `9ff231ba7` 源码核实
- `snowwind` `TargetHpBefore==Some(100)` / `TargetHp==Some(85)` → 现读 golden：`72912` `c["TargetHpBefore"]==g["Fixture"]["target_hp"]`、`72916` `c["TargetHp"]==g["TargetHp"]`。修于 `675b74866`，随 `4ed717dad` 合入。
- `cloak_assert` / `big_cloak_assert` 的 657 `(param,tag,series)==(Some(0),Some(0),Some(0))` → 现从 golden 读（`grep '(Some(0), Some(0), Some(0))'` 于 `9ff231ba7` = **0 处**）。
- **来龙去脉**：历史草案基于旧树列了 11 条；截至 `9ff231ba7` 已修掉上述 4 条 ⇒ **应改缩为 7 条**（见 §3）。后人勿再按旧 11 条理解。

## 3. 表 A：应改（7 条，行号= `9ff231ba7`）
| 行 | 断言（当前常量） | 面 | 应从 golden 取 | 备注 |
|---|---|---|---|---|
| 72096 | `fire["param"] == 100` | param | 该支 golden `Sent` 对应帧（ident=638）`param` | |
| 72097 | `fire["tag"] == 100` | tag | 同上 `tag` | |
| 72812 | `rust_abil[0] == 20` | Level | `Fixture.construction.level` | **下一行 `==gold_abil[0]`（72813）⇒ 冗余副本，可删** |
| ~72819 | `fld(&rust_abil,16) == 224`（msg@72820） | MaxHP | `Fixture.derived.max_hp` | **随后 `==fld(gold_abil,16)` ⇒ 可删** |
| ~72829 | `fld(&rust_abil,18) == 81`（msg@72830） | MaxMP | `Fixture.derived.max_mp` | 同上 |
| ~72839 | `fld(&rust_abil,6) == 0x0403`（msg@72840） | DC | `Fixture.derived.dc` | 同上 |
| 72805 | `rust_abil.len() == 40` | WAbil 长度 | oracle 40B（布局常量） | **borderline**：可保留但注明 `layout=oracle/env0` |

> 省力点：WAbil 那 4 条（72812/72819/72829/72839）**紧跟 golden `==gold_abil` 比对**——删掉字面量断言、保留 golden 比对即可，无需新取数。

## 4. 表 B：可保留（语义不变式，示例）
- 帧序 vec：`seq==vec![638,642,657,640]`、`&seq[..2]==&[638,642]`、`seq[last]==640`。
- 帧数/计数：`healing_count(...)==N`、`n("target","Rm",8044)==1`、`sf_count(...)==0`。
- 存在性：`obs_out.len()==0`、`.filter(ident==17).count()==0`、`n(who,"Sent",53)==0`。
- 魔号常量：`p3_of_10008==Some(33)`、`p1_of_10123==Some(33)`。
- seq 连续 / seed：`pair[1].0==pair[0].0+1`、`trace_header_seed==seed`。

## 5. 静默默认（同族 B；`golden_fixture_opt(...).unwrap_or(<default>)`，行号=`9ff231ba7`）
| 行 | 字段 | 静默默认 | 建议 |
|---|---|---|---|
| 69881 | `caster_hit_point` | 5 | 改 `golden_fixture_required`（若夹具必带）|
| 69883 | `caster_dc` | 0 | 改 `golden_fixture_required`（**`3b` 点名**）|
| 69885 | `caster_attack_mode` | 0 | 改 `golden_fixture_required` |
| 71790 | `caster_max_hp`（healing） | 100 | 改 `required`，或注明"该支可选"的理由 |
> 注：`if let Some(x) = golden_fixture_opt(...)` 的用法**不是**静默默认（缺则跳过），不在此列。

## 6. 工作量
- 应改 7 条（其中 WAbil 4 条多为**删除冗余副本**，改动极小）+ 静默默认 4 条改 `required`（可能需给缺失支补夹具字段）。
- 集中 ~5 个 helper（`firecharm_order_assert` / `snowwind_assert` / `defence_assert` / `healing_assert` 等）。估算 **约半天 + 复跑比较器确认绿**。

## 7. 边界
只读盘点 + 本文档；**未改产品代码、未编译**；行号对 `9ff231ba7`，若后续 rebase 需按内容（断言串）重定位而非死认行号。

## 8. 实施与合并（收录，勿抹）
- 实施 commit：`2dacd95dc`（分支 `card/assert-consts-impl`，base `0b42eb25c`）。
- 合并进 master：`b61a5ad7d`（合并树 `99b88ef45f`）。
- **静默默认 4 处已改 `golden_fixture_required`**；**唯一漏掉的 `caster_max_hp` 下游残留行（`u16::try_from(cmax).unwrap_or(100)`）由合并侧删除**——`clippy useless-conversion` 暴露：改 `required` 后该行既多余、又残留一个 `.unwrap_or(100)` 默认。分支 follow-up commit 同步删除该行，使分支 tip 与合并代码一致。
- 教训：改静默默认要连下游使用行一起看（`required` 返真值后，`.unwrap_or/.try_from/if let Ok` 包裹都是残渣）；门要四项全跑（本次漏跑 `clippy --all-targets`）。
