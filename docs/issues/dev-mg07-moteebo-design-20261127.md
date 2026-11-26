# DEV-MG-07（27 野蛮冲撞「真支」）设计报告 — 2026-11-27 夜班 openmir2-75

> 状态：**待 opc-27 批**。批前不写代码。分支 `card/dev-mg07-moteebo`（自 master `b8c5cbac0` 切）。

## 0. 基线与锚核对

- 派工单写基线 `698206b34`；master 实际 tip = **`b8c5cbac0`**（多 3 笔 = MG-36 结算面）。
  已核 `git diff --name-only 698206b34..HEAD`：只动 `docs/issues/parity-gap-magic.json`、
  `mir2-rs/bins/game-svr/src/lib.rs`、`MagicGolden/Program.cs`、`firecross.json`、
  `magic_fireball_parity.rs`——**与 DEV-MG-07 写集无重叠**，从当前 tip 切更安全（生成器/驱动取最新态）。
- 派工单锚全部复核成立：3s 门 `PlayObject.Attack.cs:1189`、调用点 `:1201 DoMotaebo(Dir, UserMagic.Level)`、
  实现 `PlayObject.Base.cs:1390 DoMotaebo` + `:1374 DoMotaeboCanMotaebo`、Rust 现状
  `warrskill.rs:359 Arm::Moteebo`、偏差钉 `warrskill_parity_tests.rs:782`。
- 清单错锚（`MagicManager.cs:93`）已在 `E:/OPC/docs/impl-gaps-for-owner-20261126.md` §1.3 更正
  （E:/OPC 非 git 仓库，直接改文件；本报告随分支 docs 提交入库）。

## 1. oracle 行为面（逐条，DoMotaebo 全文已读）

### 1.1 调用点（Attack.cs:1187-1217 `case SKILL_MOOTEBO`）

1. `result = true` 恒置 ⇒ `+GD` ack 无条件（现状已对）。
2. 3s 门 `(GetTickCount() - DoMotaeboTick) > 3000` 严格大于；门内零帧零扣（现状已对，golden A/C 支锁定）。
3. 门后：刷 `DoMotaeboTick`；`Dir = (byte)nTargetX`（**坐标当方向**，客户端真实传的是坐标——golden 注释原话）；
   `GetSpellPoint = Round(Spell/4*(L+1)) + DefSpell`（L1/Spell20 ⇒ 10）；
   `MP >= 10` ⇒ `DamageSpell` + `HealthSpellChanged`（53 帧）。
4. `DoMotaebo(Dir, Level)` 返回 **true（真冲到位）** 且 `Level < 3` 且 `TrainLevel[L] < Abil.Level` ⇒
   `TrainSkill(UserMagic, Random(3)+1)`；`!CheckMagicLevelUp` ⇒ `SendSelfDelayMsg(RM_MAGIC_LVEXP, 0,
   MagicId, Level, TranPoint, "", 1000)`（**640 延迟 1000ms**）；升级支在 `CheckMagicLevelUp` 内部
   `UpdateDelayMsg(640, …, 800)`（`PlayObject.cs:5216`；`TrainSkill :4393` 仅 `TranPoint += n`，
   FastTrain 未装载 ⇒ 无 ×3）。

### 1.2 DoMotaebo（Base.cs:1390-1503）三路径

**Path A（正前有对象，:1402-1441）**：循环 `MAX(2, Level+1)` 次：
- `GetPoseCreate()` = 正前格第一个活对象（`GetMovingObject(前格, true)`，跳 Ghost/Death，
  `Envirnoment.cs:1203-1239` 按格 `ObjList` 插入序取第一个）。**非法方向 ⇒ 前格 = 脚下 ⇒ 取到自己**
  （`GetFrontPosition` `Base.cs:977-1038` 对非法方向不改坐标）——这正是现退化支的机理。
- 每轮先 `n28 = 0`（自伤记账清零）；`DoMotaeboCanMotaebo`（:1374-1388）：`我等级 > 目标等级 且 !StickMode`
  ⇒ 骰 `Random(20) < L*4 + 6 + 等级差` ⇒ `IsProperTarget`。任一不过 ⇒ break
  （首轮即败 ⇒ `baseObject34` 保持 null ⇒ 无伤害段；`bo35` 保持 true ⇒ 红字）。
- `Level >= 3`：先推 **2 格外**的第二目标 `CharPushed(Dir,1)`（同样过 CanMotaebo，:1414-1424）。
- `baseObject34 = poseCreate`（**先记后推** ⇒ 推失败也进伤害段）；`poseCreate.CharPushed(Dir,1)`
  （`Base.cs:854-894`：逐格 `CanWalk(false)` + `MoveToMovingObject` + `RM_PUSH(nBackDir,新x,新y)` +
  `Race>=Animal ⇒ WalkTick+=800`；成功置 `Dir=BackDir`，推进 0 格还原原方向）返回 ≠1 ⇒ break。
- 推成功 ⇒ `GetFrontPosition` + `MoveToMovingObject` 自移入其旧格：`RM_RUSH(nDir,新x,新y)`、
  `bo35=false`、`result=true`；`n24 -= 1`（推预算，无论自移成败都减）。

**Path B（正前无对象，:1443-1468）**：`bo35=false`；循环 `MAX(2, L+1)`：
前格可进 ⇒ `RM_RUSH` + `n28 -= 1`；不可进但 `CanWalk(nx,ny,true)`（活体挡）⇒ `n28=0` 继续；
**不可走（墙）⇒ `bo35=true; break`（n28 不清）**。`result` 恒 false ⇒ **Path B 永不触发训练**。

**尾段（:1470-1501）**：
- `baseObject34 != null` ⇒ `n24<0` 归 0；`nDmg = Random((n24+1)*10) + (n24+1)*10`；
  `GetHitStruckDamage(施法者, nDmg)`（目标 AC 折减）；`StruckDamage`；
  目标 `SendRefMsg(RM_STRUCK, nDmg, 目标HP, 目标MaxHP, 施法者ActorId, "")`；
  目标非 Play ⇒ **额外直投目标一条 RM_STRUCK**（怪物队列；宿主不建模怪物收件人 ⇒ golden 不可观测，
  B27 同族登记）。
- `bo35` ⇒ `GetFrontPosition` 后发 `RM_RUSHKUNG(Dir, 前格x, 前格y)` + 红字 `MateDoTooweak`。
- `n28 > 0` ⇒ `n24` 归 0；`nDmg = Random(n24*10) + (n24+1)*3`；`GetHitStruckDamage(self)`；
  `StruckDamage`；**自己** `SendRefMsg(RM_STRUCK, nDmg, 自己HP, 自己MaxHP, 0, "")`。

### 1.3 帧号 / 收件人 / 优先级

`SM_RUSH=6`（移动者自收）、`SM_BACKSTEP=9`（被推者自收）、`SM_STRUCK=31`（受击者视野内玩家含受击者）、
`SM_RUSHKUNG=7` + `SM_SYSMESSAGE=100`（红字）、`SM_HEALTHSPELLCHANGED=53`、`SM_MAGIC_LVEXP=640`（自收，
延迟 1000/800）。`SendRefMsg` 收件人 = 发送者 ±12 内玩家含自己 ⇒ golden `obs_out(140,100)` 全程盲
（结构负例）。同收件人出队序按 RM 低字节升序（`SpellFrameBuf` 优先级机制既有）：53(68) < 31(98)
< 640(139) < 6(168) < 7(169) < 100(252)——但 640 是延迟 1000ms 后第二轮排水才落 ⇒ `Sent` 里在最后
（golden 排水序为准，不预写）。

### 1.4 RNG 抽序（L=1、单怪、两轮全成）

`Random(20)`（iter1 骰）→ `Random(20)`（iter2 骰）→ `Random((n24+1)*10)=Random(10)`（目标伤害）→
`Random(3)`（训练）。wall 臂：仅 `Random(n24*10)=Random(20)` 一次（自伤）。
（`GetHitStruckDamage` 内部是否含抽——实现时核 `Base.cs`；golden 钉真值，`combat.rs:869` 已是过验镜像。）

## 2. Rust 现状与差距

**已对且不动**：3s 门、MP 门、DamageSpell+53、退化红字支（coordinate_dir golden 现值 caster
`[53,7,100]` 已逐字节锁定——真算法实现后该支必须原样绿，是最硬的回归守卫）。

**缺**：
1. Path A 全部。现 WRN 分支用「同格扫描且**排除自己**」是登记偏差 hack；真实现 `GetPoseCreate`
   语义**必须包含自己**（非法方向 ⇒ 取到 ⇒ 等级门失败 ⇒ 红字，与 golden 一致）。
2. Path B 的 world 侧 `rush_now_impl` 已存在（`world.rs:12312`，注释明锚 DoMotaebo :1443-1467，
   含被挡 RushKung 事件语义）但 `#[allow(dead_code)]` 未接宿主。
3. 伤害段：`combat::get_hit_struck_damage`（:869）/ `struck_damage_apply`（:1203）已有，宿主 SM_STRUCK
   wire 路径已有（`lib.rs:25007` 区，实时 HP 口径）。
4. 训练尾：`magic::do_spell_tail`（`magic.rs:662`，`bo_spell_fire=false` ⇒ 无 638）+
   `world.send_self_delay_msg_full/update_self_delay_msg_full`（800/1000 双支）已有
   （`finish_spell_tail_with` P3-B3 同构；27 是调用点训练，独立调这两个原语）。
5. **世界侧缺公有 facade**：`push_now` 是 `pub(crate)` 宿主不可直调 ⇒ 仿 `cast_firewind_spell` 模式
   新增公有入口（下文 §4）。

**复用件清单**（不写新效果体）：`magic::front_position` + DIR 常量（magic.rs:706）、
`push_now_impl`/`char_pushed_core`、`try_step_impl`、`can_walk_ignore_objects_impl`（world.rs:12284，
注释就锚着 DoMotaebo:1461）、`combat::is_proper_target` 族（combat.rs:715）、上列伤害/训练/延迟件、
`downlink::backstep_frame/rush_frame`（downlink.rs:457/477）、`smframe::sm_magic_lvexp`、
`MapGrid::set_map_xy_flag`（map.rs，造墙）、RM 低字节优先级模式（P_PUSH=156 等，lib.rs:19696）。

## 3. 装置（oracle golden 生成器，ClientSpellXYEntry.cs）

现状 `warr-27.json` = 1 支 `coordinate_dir`（Dir=101 非法 ⇒ 红字支，`Rng=[]`）。装置改法全走
**条件键/缺省不变**（零变化纪律按 mg50 B31 口径：JSON 等值 + 逐路径 diff）：

- `SubCase` 增旋钮：`DirPreset:int?`（TargetX=方向值 0..7）、`FrontMonster:bool`（正前格放
  RecordingMonster——`Program.cs:5416` 已有造法，复用 magic stems 的被推怪同族；玩家 target 挪
  (110,100)）、`MonsterLevel/MonsterHp/MonsterAc`、`WallXy:int[]?`（生成器把该格 Attribute 改非 Walk；
  **两侧同判据**：Rust 侧 `MapGrid::set_map_xy_flag(x,y,false)` 同格；golden `Fixture.wall_xy` 记账）。
- 新增 golden 条件键：`monster_xy / monster_level / monster_hp_before / monster_hp_after`
  （现 golden 只记玩家 target 的 HP）。
- **生成时结构断言**（fail-closed，防摆位错静默生成红字支）：push_ok 必含 `RM_PUSH≥1 + RM_RUSH≥1 +
  RM_STRUCK≥1 + Rng 非空 + monster_hp_after < before`；wall 臂必含 `RM_RUSHKUNG + RM_STRUCK` 且无
  `RM_PUSH`；free_run 必含 `RM_RUSH` 且无 `RM_STRUCK/RM_RUSHKUNG/RM_MAGIC_LVEXP`。
- **臂清单**：
  1. `push_ok`（主臂）：Dir=2（右），正前 (101,100) 放怪（L10/HP100/AC0/StickMode=false）。
     预期：两轮推挤（每收件人 9×2）+ 两格 6 + 怪物 31 + 训练尾 640@1000 + MP 100→90 + caster 落
     (102,100)；RNG 4 抽（20/20/10/3）。L=1 骰门 `Random(20) < 1*4+6+20=30` 恒真 ⇒ **不依赖 seed**。
  2. `free_run`（对照臂）：Dir=6（左），正前无对象 ⇒ Path B 成功两格（6×2、caster 落 (98,100)），
     无红字/无伤害/无训练——`result=false` 的**结构负例**，钉「训练只在真冲后」。RNG 0 抽。
  3. `wall_selfdamage`（自伤臂）：Dir=6、(99,100) 置墙 ⇒ Path B 首格即挡：红字 7+100、自伤 31
     （`Random(20)+(1+1)*3`，扣自身 AC）、无 6/9/训练、caster 不移动。RNG 1 抽。
  4. （可选，批内时间富余才做）`levelup_l0`：UserMagicLevel=0 ⇒ SpellPoint=5（MP 100→95）、骰门
     `Random(20)<16`（seed 需挑，per-case 旋钮已有）、`CheckMagicLevelUp` 升级支 ⇒ **640@800ms**
     （`UpdateDelayMsg` 去重语义）。同时覆盖「骰门可败」面（L=1 恒真不可变异）。

## 4. 产品实现（单独成笔）

- **world-bevy**：新增公有 `World::do_motaebo_front_now(...)`（Path A：GetPoseCreate 等价扫描
  （含自己）→ CanMotaebo（level/stick/骰/is_proper_target）→（L≥3）二格推 → `push_now_impl` 推挤 →
  `try_step_impl` 自移 → RushEvent/PushEvent/伤害段（`get_hit_struck_damage`+`struck_damage_apply`，
  StruckEvent 记账与攻击路径 struck_log 同形）→ bo35/n24/n28/result 返回）；Path B 复用既有
  `rush_now_impl`（去掉 dead_code 或经公有 facade 转发）。**不写新推挤效果体**（按派工单）。
- **game-svr `warrskill.rs Arm::Moteebo`**：门/MP 面不动；门后调 world facade ⇒ 按 firewind 接线模式
  出帧（`backstep_frame`→`P_PUSH`@事件坐标（lib.rs:19792-19821 逐行同构）、`rush_frame`→`P_RUSH`、
  31 帧走既有 RM_STRUCK wire 路径、bo35 ⇒ `rush_frame(RushKung)`+红字 `strings.mate_do_tooweak`）；
  训练尾：`do_spell_tail(bo_spell_fire=false, bo_train=result)` ⇒ `levelup_msg` 时
  `send/update_self_delay_msg_full(…, 1000/800)`（升级支 800 由 `CheckMagicLevelUp` 语义给出）。
  WRN「真支未建模」分支删除。
- **偏差钉改写**：`warr_27_true_branch_is_registered_deviation` → 正向断言（改名如
  `warr_27_true_branch_pushes_struck_trains`）：blocker 移到正前格且低等级 ⇒ 断言 6/31 出现、施法者
  移动、怪物掉血（派工单批次纪律第 5 条）。
- **B27 同族登记**：怪物直投 RM_STRUCK 不建模（golden 不可观测），manifest 注明。

## 5. 驱动 / 比较器（warrskill_parity_tests.rs）

- `run_case` 增**条件能力**（缺键 = 零变化）：monster 生成（`EntitySpawn` race=Animal 族，
  level/hp/stick_mode 按 fixture）、`target_xy/monster_xy` 覆写、wall 置格、map 尺寸旋钮
  （现 `with_maps((32,32))` 而坐标 100+——现有 case 不动所以没事；新臂需要 ≥(142,32) 或按 golden
  caster_xy 建同等大图）、**640 泵**：调用后按 golden `wait_ms` 推进 sim 时钟 + 走
  `deliver_self_delay_msgs` 泵 + 对到期 `RM_MAGIC_LVEXP` 发 `sm_magic_lvexp` 到 caster socket
  （**基数 = 施法时刻**，MG-36 §21.1 教训；不用排水后的钟当基数）。
- `check_golden` 放宽：**零抽断言改条件式**（golden `Rng` 空 ⇒ 沿用现状零抽两侧断言；非空 ⇒ 逐值比
  golden `Rng` vs Rust 抽账）。其余（三键契约、逐收件人逐帧、布局宽度）不动——现有 9 份 golden
  21 支行为不变。
- 结构断言（行为面，失败名可定位）：push_ok caster mp 100→90、monster HP 降量=golden、caster
  坐标=(102,100)、640 存在；free_run 无 31/7/100/640、坐标=(98,100)；wall 臂 caster HP 降、坐标
  不变、有 7+100、无 6/9。

## 6. 变异牙（三颗，逐支删臂复红）

1. 删推挤（world facade 去掉 `CharPushed` 调用）⇒ push_ok 红（少 9 帧 + 怪不掉血 + RNG 流错位）。
2. 删自伤（尾段 n28 支短路）⇒ wall_selfdamage 红（少 31 帧 + caster HP 不降）。
3. 删训练尾（调用点 bo_train 短路）⇒ push_ok 红（少 640）。
每颗按记忆纪律：本人 worktree 内跑、红在目标 case、还原后 sha 相等 + 无 MUTATION 残留、复跑绿、
断言 passed+failed≥1（证明用例真跑）。骰门负例：L=1 恒真不可变异——由（可选）levelup_l0 覆盖，
不做则在 manifest 登记豁免理由。

## 7. 提交切分与门

| 笔 | 内容 | 门 |
|---|---|---|
| ① docs | 本报告入库（docs/issues/）+ 清单错锚更正（E:/OPC 非 repo，文件直改） | 锚门（如涉 .md path:line 锚则 `--prose`） |
| ② 装置笔 | ClientSpellXYEntry.cs + warr-27.json 只追加 2-3 支 | dotnet build 0；**全部 golden 重出 JSON 等值**（既有 9 份 warr + 触及的 magic 份）；不碰 `src/` 与 mir2-rs 产品码 |
| ③ 产品笔 | world-bevy facade + warrskill.rs + 偏差钉改写 | **`cargo test --workspace` 全量 + passed/failed + 耗时**（含产品改动，按派工单纪律）；clippy workspace `--all-targets -D warnings`；fmt；msgcodegen `--check`；锚门 `--prose --base master` |
| ④ 驱动/牙笔 | run_case 条件能力 + check_golden 放宽 + 三牙红证 | 同 ③（驱动在 game-svr lib，接 ③ 后跑全量） |

每笔贴 sha + 门原文；红了就停、给原文。不动 master；不自行合并（needs-review 交 opc-27）。

## 8. 风险

1. **SM_STRUCK/SM_BACKSTEP 帧尾的怪物 CharDesc**：取受击者/被推者的 Feature/Status——firewind 已对
   被推怪出 9 帧过验（先例），但 31 帧尾两侧必须同源：RecordingMonster 的 Feature 进 Fixture，
   Rust rig 按 fixture 建 display。这是最可能的逐字节红点；golden 钉真值。
2. **GetHitStruckDamage 内部抽风**：实现时核 Base.cs；两侧同源（combat.rs 已过验）风险低。
3. **格占有模拟**：Rust actors 不真落格（world.rs:4984 登记），「第一个活对象」按扫描序模拟——
   单怪单格无序问题；coordinate_dir 取自己是关键回归点。
4. **时钟泵基数**：640 due = 施法时刻 + 1000（见 §5）。
5. **oracle 侧改格属性 API**：若 `MapCellInfo.Attribute` 直改不便（private/封装），fallback =
   生成器扫真图找 caster 附近墙向（地图 sha 已钉，确定性等价）；先试直改。
6. **行为等价白名单**：若实现中发现 oracle 怪行为（如 `n24` 在自移失败时也递减）与直觉冲突，
   照抄不修（byte 等级），卡面登记。
7. **`dir` 键语义**：golden `Fixture.dir` 是**调用后**的 caster.Dir；run_case 现按它预置并断言不变——
   对真支仍成立（arm 把 Dir 写成同一 nTargetX），无需改 harness，但写实现时留意。
