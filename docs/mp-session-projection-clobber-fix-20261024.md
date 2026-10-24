# MP 会话投影被 P21 刷回覆盖：spell_cast 第二帧 15 应为 13 —— 修复报告

- **卡**：`openmir2-mp-session-projection-clobber`（tier=R，window_required=true）
- **owner**：openmir2-worker-10；**worktree**：`E:/tmp/wt-10-next`；**分支**：`card/mp-session-projection-clobber`
- **接卡确认**：collab status `bb8636b3b`（parent `15c7466dbc`，issue `84cc70aec`）
- **基线**：`walgit/master` = `fa8113253`（≥ 卡面要求的 `968ede63c`；含分诊合并
  `card/compare-ident53-diff-triage`，即本卡证据链的落库版）

## 1. Oracle 证据链（acceptance ①）

静态推导全文见 worker-10 review `bab63106f1`（线程 `openmir2-compare-ident53-diff-triage`，
F1 finding；6885a0b12 已按该 review 把报告/selftest ⑦ 注释的成因更正为本结论）：

1. **施法 wrapper 只扣投影**：`mir2-rs/bins/game-svr/src/lib.rs` `handle_spell_cmd_with_magic`
   的 MP 包装层（原 ：10426）`sg.state.wabil.mp = sg.state.wabil.mp.saturating_sub(n_spell_point)`
   ——不镜像 `entity.mp`。
2. **P21 每 tick 无条件刷回**：`drain_player_need_recheck_with_magic`（原 ：18498-18500）
   `sg.state.wabil.hp = e.hp; sg.state.wabil.mp = e.mp;`，`sim_loop` 每 tick 对每个在线会话执行；
   `entity.mp` 无生产写点恒 17（登录 `apply_ability_to_entity` 同步后不再变）。
3. **推演**（与冻结夹具 `tools/conformance/fixtures/spell_cast/{csharp,rust}/frames.ndjson`
   ident53 行逐位吻合）：cast1(t=2241) 投影 17→15 发 tag=15 → 下一 tick P21 刷回 17 →
   cast2(t=4557) 再 17→15 发 tag=15 → 软关前最后一次刷回 17 → `build_save_record`（:8671
   读会话投影）落库 `characters_ablity.MP=17`。夹具实测 rust tag 15/15、库行 MP=17。
4. **oracle 行为**（`PlayObject.Base.cs:1505-1531`）：单份 `BaseObject.WAbil`，
   `GetSpellPoint=2`（:1369-1372），蓝门 :1514 → `DamageSpell`（:1518 →
   `BaseObject.cs:1383-1405`，`MP -= n`）→ `HealthSpellChanged`（:1519 →
   `BaseObject.cs:700-709`，53 帧 `Tag=MP`=扣后值）——**连续扣减** ⇒ 17→15→13；
   软关 `MakeSaveRcd` 读同一份 `WAbil` ⇒ 落库 13。
5. **为什么 13 是 oracle 口径**：cast2 的 tag=13 反证 pre-cast2 MP 恰为 15——窗口内
   （t=2241→4557）没有任何恢复把 MP 送回 17/18（stamina SpellFillTime=800 模型与 oracle
   帧逐位吻合，见 6885a0b12 报告）。单份 WAbil 语义下 13 是唯一可取值；基线 MP=17 由
   dbsnap + 入图日志 + 帧 param/series/body 尾三方互证。

## 2. 修法与 ：10388 注释的关系

**选定改法：扣减镜像回 `entity.mp`（写点同步），P21 刷回保持原样。**

- 原注释段（「MP 权威存储」，原 ：10387-10393）自己就登记了这个义务：「将来接毒/魔法盾
  消耗时必须把 `wabil.mp` 同步进 `entity.mp` ……否则会出现第二份 MP（复验 surviving）」。
  施法 wrapper 正是**已存在**的「只写一侧的 MP 写点」——本修法在该写点补上同步，是把
  注释的既有登记兑现，不是新策略。
- **为什么不选「刷回跳过该字段」**：实体→投影的 mp 刷回不止 P21 一处（P32 复活段
  `drain_player_revival_with_magic` ：19323-19324、RefMyStatus 段 ：18810-18811、SuperMan
  同步 ×2），且 ：19321 明文登记对立教义「`WAbil.HP/MP` 以实体为唯一真源（spike §3.2）」。
  逐处跳过要动 4+ 个写点、还要给未来的毒/魔法盾消耗（`buff::damage_health` 写实体）另想办法
  传回投影——面大且与既有教义冲突。写点镜像则一处改动使**所有**刷回自然一致，P21/P32/
  RefMyStatus/SuperMan 全部不需要动。
- 修后注释段改写为「本块改投影并镜像进实体」，并保留毒/魔法盾未来义务（按「写点同步」双写）。

生产 diff（`mir2-rs/bins/game-svr/src/lib.rs`，最小面）：
1. 「MP 权威存储」注释段改写（说明镜像原因 + ident53 证据指针）；
2. 扣减行后追加镜像块：`if let Some(e) = world.entities.get_mut(actor_id) { e.mp = sg.state.wabil.mp; }`
   （入口 :10292 已保证实体存在；P21/P32/RefMyStatus/SuperMan 零改动）。

## 3. 合同测试（acceptance ②）

`tests::spell_cast_two_consecutive_fires_mp_17_15_13_and_save_13`（同文件，`mp_clobber_harness`
夹具，全走生产入口 `handle_spell_cmd`）：

- 基线钉窗口口径：投影/实体 MP=17、HP=16/MaxHP=16/MaxMP=18（冻结夹具三方互证基线）；
  spell=0/def_spell=2/level=1 ⇒ `GetSpellPoint=2`/发，越界点击（MP 包装层先于距离门，效果体零帧）。
- 序列：cast1（53.Tag=15、投影 15、实体 15）→ P21 刷回（投影仍 15、P21 零出帧自检）→
  cast2（53.Tag=13、投影 13）→ P21 刷回（13）→ `build_save_record` ⇒ `abil.mp == 13`。
- **spell_cast 冻结窗口 fixture 历史帧未动**（`git diff` 零触碰）；`tools/conformance/selftest_impl.py`
  ⑦ anatomy 归档注释按卡补一句：「tag 差异已由本卡
  （openmir2-mp-session-projection-clobber）修复……窗口复拍验证 13/13 后刷新 fixture」，
  断言元组不变。

## 4. 两颗变异牙（acceptance ③，实测）

| 牙 | 变异 | 实测结果 |
| --- | --- | --- |
| ① | 单删扣减点的实体镜像（=「P21 无条件刷回重新赢得覆盖」，即 master 行为） | 红：`panicked ... 扣减镜像进实体 ⇒ P21 刷回不再回跳: left: 17, right: 15`（实体回跳即后续 15/15+17 之因） |
| ② | 单删 `saturating_sub` 扣减 | 红：`panicked ... cast1：投影 17-2=15 ⇒ 53.Tag=MP=15: left: [17], right: [15]`（两帧恒 17、存档 17） |

两者各自还原后合同测试绿（`1 passed`）。卡面牙①措辞「把 P21 刷回改回无条件」按所选修法
映射为「删镜像」：本修法未改 P21（它本来就是无条件），删镜像即精确重现「无条件刷回覆盖扣减」。

## 5. 门禁（acceptance ④，全在钉住的最终 commit 上重放）

| 门禁 | 命令 | rc |
| --- | --- | --- |
| fmt | `cargo fmt --all -- --check` | 0 |
| clippy | `cargo clippy --workspace --all-targets -- -D warnings` | 0 |
| test | `cargo test --workspace` | 0（**3169 passed / 0 failed** = 基线 3168 + 本卡新合同测试 1，141 个 result 组全 ok） |
| msgcodegen | `cargo run -p msgcodegen -- --check` | 0（604 条常量与 C# 源一致） |
| t_reservation | `python -I tools/whitelist/t_reservation_gate.py check` | 0 |
| conflict_marker | `python -I tools/hygiene/conflict_marker_gate.py` | 0 |
| stale_pointer | `python -I -B tools/hygiene/stale_pointer_gate.py` | 0 |
| dynamic_count_stamp | `python -I -B tools/hygiene/dynamic_count_stamp_gate.py` | 0 |
| diff --check | `git diff --check`（含 --cached 复核） | 0 |
| merge-tree | vs `walgit/master`=`fa8113253` | 0 冲突 |

## 6. R 级窗口验收登记（window_required=true）

修复触战斗数值（53 帧 MP）+ 持久化（`characters_ablity.MP`）⇒ 按判级政策必须真机窗口复拍：
spell_cast 场景复跑，验收 = rust 两帧 tag 逐位 = oracle 15/13（修复前 15/15）+ 落库 MP=13
（修复前 17）+ 其余帧不回归。合并后卡片保持 in-progress，窗口复拍 13/13 并按 ⑦ 注释刷新
fixture 后才 closed。

## 7. 残留物清单

无本线专属残留：无临时目录/进程/日志；`E:\tmp` 无新残留；冻结 fixture 未触碰；C# 侧未触碰。
