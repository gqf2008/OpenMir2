# 推挤批实现记录：MG-16(8) / MG-57(37) / MG-35(21)

> 分支 `card/magic-push-batch-impl`，base `master 0e8988dd8`。
> 规划卡：`docs/issues/magic-push-batch-wiring-plan.md`（8/37）、`docs/issues/magic-47-windtebo-plan.md`（47，本轮不做）。
> 退化守卫判据：`docs/issues/degenerate-guard-criterion.md`。

## 1. 装置改动（`MagicGolden/Program.cs`，全部由旋钮门控 ⇒ 其它 stem 零变化）

| 旋钮 | 作用 | 缺省 |
| --- | --- | --- |
| `MakeVisible` | 建完对象后对每只目标调 `caster.UpdateVisibleGay(...)`（`MagPushArround` 遍历 `VisibleActors`，离线装置不跑 AOI） | `false` |
| `HomeMap` | 设 `caster.HomeMap`（21 的 `MapRandomMove→SpaceMove→FindMap(HomeMap)`） | `null` |

新 case：`firewind`(8, `MakeVisible`)、`energyrepulsor`(37, `MakeVisible`)、
`spacemove`(21, `EnableMaps` + `HomeMap="0"`)，均走 `BehaviorBatch`（s1/s5/s13）。

## 2. 落点可走性实测（一次性测量，结果写在这里）

按 `0.map` + 装置缺省落点（caster `(100,100)`、target `(101,100)`）：

- **`(102,100)`：可走** —— s1/s5 的 target 被推 1 格，`SM_BACKSTEP(9)` 落在 `(102,100)`；
- **`(103,100)`：可走** —— s13 的 target 被推 2 格，第二条 `9` 落在 `(103,100)`。
- 依据：`char_pushed_core` / `CharPushed` 逐格 `CanWalk` 通过才会发帧，帧落在该格即证明可走。
  **未依赖"看起来能走"，是帧面反证。**

## 3. 三份 golden 的非退化自检（对应退化守卫）

| stem | 效果帧 | 证据 |
| --- | --- | --- |
| `firewind` | `SM_BACKSTEP(9)` | s1/s5 各 1 条、s13 2 条（target/caster/obs_in 均见） |
| `energyrepulsor` | `SM_BACKSTEP(9)` | 同形（与 8 逐字同函数体） |
| `spacemove` | `SM_SPACEMOVE_SHOW2(807)`（落点 `param/tag`） | caster 侧 `807(121,353)`，另有 633/634/766/54/11029/640 |

**抽数账本**（写进 golden 的 `Rng`）：8/37 各 case 3 抽（`Random(20)` + `Random(2)` + 共享尾 `Random(3)`）；
21 各 case 4 抽（`Random(11)` 门 + 两次初始取数 + 移位环/尾）。

## 4. 确定性

同一 case 连跑两次，**仅 `delivery`（墙钟）不同**，比较面（`Sent` 的 ident/param/tag/series/tail、`Rm`、
`Rng`、`TargetHp`）逐字节一致 ⇒ 可作基准。

## 4b. `634.series` 的时刻输入（2026-11-21）

`SM_CHANGEMAP(634).Series` = `PlayObject.DayBright()`，输入是 `M2Share.GameTime`
（`src/M2Server/Player/PlayObject.Base.cs:244-269`：`1→0`、`3→1`、其它→2；`Envir.Flag` 的
`DayLight`/`boDarkness` 另有覆盖）。

- C# 装置**不跑** `SystemProcessor`（真机每 50ms 用 `DateTime.Now.Hour` 刷新 `GameTime`）
  ⇒ 生效值恒为静态默认 `0` ⇒ `DayBright()==2` ⇒ golden 三条 `634.series` 都是 2。
- Rust 驱动原先走生产 `drain_map_change`（`enter::game_time_now()` 读**真实本地小时**）。
  `game_time_from_hour` 在 `5..=10 / 16..=19 / 21` 给 1（⇒ `DayBright()==0`）、`11/20/23` 给 2、
  `4/15` 给 0、其余给 3 ⇒ **多数工作时段跑必红**（2026-11-21 09:12 实测 Rust `series=0` vs golden `2`）。
- 修法：golden `Fixture` 增加 `map_bright`（装置生效值，恒 0）与 `map_bright_hour`
  （`-1` = 无墙钟来源，仅标注）；驱动读 `Fixture.map_bright`（**缺键 panic、断言 `==0`**）并调
  `drain_map_change_at_time` 注入。**生产路径不变**（真机就该读真实时钟）。

## 4c. 8/37/21 的帧序 = oracle 收件人优先队列（2026-11-21）

oracle 的收件人队列是 `MsgQueue.Enqueue(msg, (byte)wIdent)` 的**四叉最小堆**
（`src/M2Server/Actor/BaseObject.Message.cs:181`）⇒ 同一收件人的帧按 `(byte)RM ident` 出队，
**不等于**调用序。三条 golden 的实证：

| stem | 收件人 | golden `Sent` | 调用序（旧实现） |
| --- | --- | --- | --- |
| firewind/energyrepulsor s1 | target | `17 → 638 → 9` | `17 → 9 → 638` |
| firewind/energyrepulsor s13 | target | `17 → 638 → 9(103) → 9(102)` | `17 → 9(102) → 9(103) → 638` |
| firewind/energyrepulsor s13 | caster（不收 17） | `638 → 9(102) → 9(103)` | 同左列错序 |
| spacemove s1 | caster | `806 → 638 → …` | `638 → 806` |
| spacemove s1 | target | `17 → 806 → 638` | `17 → 638 → 806` |

**排序键是 RM 的 `(byte)ident`，不是 SM 号**（SM 号会得到相反序）：`RM_SPELL(10008)=24`、
`RM_MAGICFIRE(10120)=136`、`RM_PUSH(8092)=156`、`RM_SPACEMOVE_FIRE2(8042)=106`。

做法：`SpellFrameBuf` 增 `push_broadcast_cached_at`（逐格 `RM_PUSH` 的显式坐标收件人）；
8/37/21 的 `RM_SPELL(17)` 在 `:159` 处进**同一个**缓冲（s13 的两种结果就是"队列里有没有 17"
不同造成的堆布局差），效果体帧全部入缓冲，臂末尾 `flush`；版本门失败支先 flush 再发 639。
其它魔号路径逐字不变。

## 5. 待办（本批后续）

- Rust 驱动 `firewind_parity_*` / `energyrepulsor_parity_*` / `spacemove_parity_*`（rig 需补地图与 `visible_actors`）；
- 比较器 `real_rust_ledger_{firewind,energyrepulsor,spacemove}`；
- 退化守卫（两侧 ≥1 效果帧）与双向牙；
- **既有 magic 比较器零漂移**全量跑。

## 6. 本批不覆盖的范围

- **瞬移后的视野状态**：oracle `SpaceMove` 会清空 `VisibleActors`，Rust 换图簇目前不清空/重建
  ⇒ 本批帧面不可见（见 §3 说明），但**本批不修、不覆盖**。详见
  `docs/issues/openmir2-spacemove-visible-actors-reset.md`。
