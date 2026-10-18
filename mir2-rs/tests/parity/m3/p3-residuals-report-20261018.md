# P3 收尾残余报告（2026-10-18）

基线：`a8f3becd0`（`0d0e1d408` + 两个文档提交）。实现 worktree `/e/tmp/impl-p3resb`（detached），
对照 worktree `/e/tmp/verify-p3resb`（同基线 detached）。
`CARGO_TARGET_DIR=/e/tmp/p3resb-target`（复验树 `/e/tmp/verify-p3resb-target`）。未改 `src/**`、`sql/**`、`MirClient/`、
`crates/{protocol,storage}/**`、`E:/MirServer/**`；未起服务/占端口/写库；未删除任何文件。

本批只动 5 个源文件 + 白名单 + 本报告：

- `mir2-rs/bins/game-svr/src/config.rs`
- `mir2-rs/bins/game-svr/src/main.rs`
- `mir2-rs/bins/game-svr/src/ability.rs`
- `mir2-rs/crates/world/src/world.rs`
- `mir2-rs/crates/world/src/entity.rs`

---

## 1. `MagTurnUndeadLevel` 从 `Server.conf` 装载

### oracle 对照

| 点 | oracle | 本仓 |
| --- | --- | --- |
| 配置键 | `ServerConf.cs:530` `ReadWriteInteger("Setup", "MagTurnUndeadLevel", …)` | `config.rs` 的 `MAGTURNUNDEADLEVEL`（放在 `"SETUP"` match 之前，避免 rustfmt 重排共享大 match；与 `ScriptGotoCountLimit` 同法） |
| 缺省 | `GameSvrConf.cs:1773` `MagTurnUndeadLevel = 50` | `unwrap_or(50)`；**空值也回落 50** |
| 消费点 | `MagicManager.cs:844` `if (targetObject.Abil.Level < SystemShare.Config.MagTurnUndeadLevel)` | `World::mag_turn_undead_level`（缺省 50）→ `handle_spell_cmd` 的 32 支 → `cast_turn_undead_spell(..., cfg_turn_undead_level)` → `mag_turn_undead` 的同名门 |

接线：`GateConfig.mag_turn_undead_level` → `EnterConfig` → `sim_loop` 的
`World::set_mag_turn_undead_level` → 施法时读取。删除了旧的
`const MAG_TURN_UNDEAD_LEVEL: i32 = 50`。

### 用例

- `config::tests::mag_turn_undead_level_and_pk_flag_time_read_with_defaults`
  四档：键=50、键=30、缺键、空值（键=30 必须取 30；缺键/空值回落 50）。
- `tests::p3res_spell_32_turn_undead_uses_configured_level_gate`（生产入口 `handle_spell_cmd`）：
  目标 Lv=40、施法者 Lv=60、固定 `B3_SEED`。
  - cfg=30 ⇒ 不抽尾 `R(100)`（且 `boTrain=false` ⇒ 训练尾 `R(3)` 也不抽），抽数恰
    `[R(300)=269, R(2)=0]`，target.HP 保持 100；
  - cfg=50 ⇒ 抽 `[R(300)=269, R(2)=0, R(100)=32, R(3)=1]`，target.HP=0。

---

## 1b. oracle 行号订正（3 处，逐条 grep 实测）

| 位置 | 原值 | 订正 | 实测依据 |
| --- | --- | --- | --- |
| `crates/world/src/entity.rs` + 本报告 §3 | `PlayObject.cs:247` | **`PlayObject.cs:243`** | `grep -n "PvpNameColorTick" src/M2Server/Player/PlayObject.cs` ⇒ `243: public int PvpNameColorTick { get; set; }`；接口声明在 `IPlayerActor.cs:257` |
| `bins/game-svr/src/ability.rs` + 本报告 §2 | `BaseObject.Abil.cs:18` | **`BaseObject.Abil.cs:17`** | `grep -n "AddAbil = new AddAbility" src/M2Server/Actor/BaseObject.Abil.cs` ⇒ `17:`（行 1 为 BOM+`using`） |
| `bins/game-svr/src/config.rs`（注释 + 测试消息） | `StartPointSize` 的 `GameSvrConf.cs:1398` | **`GameSvrConf.cs:1400`** | `StartPointSize = 2;` 在 `GameSvrConf.cs:1400`；`:1398` 是 `DecLightItemDrugTime = 500;`（`:1395` 是 `dwPKFlagTime`） |

> 三处均为**文档/注释**错误，无行为改动；`config.rs` 的两处 `:1398`（`start_point_size`
> 缺省注释与测试消息）已一并订正，Pvp 周期行的 `GameSvrConf.cs:1398`（`DecLightItemDrugTime`）
> **是正确的，未动**。

---

## 2. `HitSpeed` 生产写点接线

### oracle 对照

| 点 | oracle | 本仓 |
| --- | --- | --- |
| 物品累加 | `PlayObject.cs:4419`（StdMode 5/6：`RealAttackSpeed(Hi(MAC))`）、`:4520`（19）、`:4528`（20）、`:4536-4538`（21）、`:4547`（22）、`:4556-4558`（23） | `ability::apply_item_parameters` 新增 `AddAbility.hit_speed` 的逐次 `ushort` 回绕累加 |
| `RealAttackSpeed` | `GameItemSystem.cs:1140-1152`：`<=10 ⇒ -v`，否则 `v-10` | `ability::real_attack_speed` |
| 最终赋值 | `PlayObject.cs:2899` `HitSpeed = AddAbil.HitSpeed;` | `RecalcOutput::hit_speed` |
| 657 末参 | `PlayObject.Message.cs:1846` `SendDefMessage(..., this.HitSpeed)`（逐收件人） | `Entity.hit_speed`（既有读点）由 `apply_ability_to_entity` 写 |

补充说明：`BaseObject.Abil.cs:55-63` 的 `AddAbil.HitSpeed / 2` + `MIN(15, …)` 位于
`base.RecalcAbilitys()` 内、物品槽循环**之前**，此时 `AddAbil` 刚被
`BaseObject.Abil.cs:17` 重置为 0 ⇒ 对该终值无可观测影响（oracle 死路径，未复制）。
`ExtraAbil[EABIL_HITSPEEDUP]`（`:2832`）在其后的 `:2899` 被覆盖，同样无可观测影响。

接线：`recalc_session_ability` 写 `SessionGround.hit_speed`；
`apply_ability_to_entity(e, wabil, sub, hit_speed)` 写 `e.hit_speed`；登录、升级、
装备/卸下成功重算、B5 的 36 `RecalcAbilitys` 目标重算四个点都同步实体。

### 用例

1. `tests::p3res_hit_speed_written_from_recalc_drives_657_tail`（657 端到端）：
   两个收件人各装一把 StdMode 5 武器（`Hi(MAC)` 13/15 ⇒ HitSpeed 3/5），经
   `recalc_session_ability` + `apply_ability_to_entity` 落实体；随后走生产入口施放 18（隐身），
   断言 socket 911 / 912 各收到的 `SM_CHARSTATUSCHANGED(657)` 末参分别为 **3 / 5**
   （既有 B3-18 用例是手工写 `hit_speed`，盖不住生产写点）。
2. `tests::p3res_bag_take_on_off_syncs_hit_speed_to_entity`（**生产入口 `handle_bag_cmd`**）：
   `CM_TAKEONITEM` 穿上一把 `Hi(MAC)=13` 的 StdMode 5 武器（`RecalcAbilitys` ⇒ 3），断言
   会话态与 `Entity.hit_speed` 都为 3；随后 `CM_TAKEOFFITEM` 卸下，断言两者都回落 0
   （盖住 `handle_bag_cmd` 里 `e.hit_speed = sg.hit_speed` 这条此前无生产入口覆盖的写点）。
3. `tests::p3res_spell_36_target_recalc_syncs_hit_speed_to_entity`（**生产入口 36 支**）：
   目标（obs=912）装 `Hi(MAC)=15` 的 StdMode 5 武器（⇒ 5），施法前把目标实体 `hit_speed`
   预置哨兵 **99**；走 `handle_spell_cmd` 施放 36 后断言目标仍收到 1 条 52（重算确实发生）
   且 `Entity.hit_speed == 5`（哨兵被生产写点替换；盖住 36 目标重算后的同款同步）。

> **R 级判据（`window_required: true`，本批未占窗）**：`HitSpeed` 是**战斗/属性数值面**
> （`AddAbil.HitSpeed` → 657 末参），按 tier 判定属 R。构造级证据（上列 3 个用例 + 2 条牙）
> 只证明「生产写点接线正确」，**不替代**真机逐字节对拍。缺证清单见 §7-1。

---

## 3. `PvpFlag` 60s 过期 + 656

### oracle 对照

| 点 | oracle | 本仓 |
| --- | --- | --- |
| 字段声明 | `PlayObject.cs:243` `public int PvpNameColorTick { get; set; }`（接口 `IPlayerActor.cs:257`） | `Entity.pvp_name_color_tick`（`i64` 虚拟毫秒） |
| 置位时刻 | `PlayObject.cs:4333` `targetObject.PvpNameColorTick = GetTickCount();`（在 `if (!targetObject.PvpFlag)` **之前**） | `Entity.pvp_name_color_tick`；`World::set_pk_flag` 无条件刷新，已带 flag 时零帧但刷新计时 |
| 过期判据 | `:4359` `PvpFlag && (now - PvpNameColorTick) > Config.dwPKFlagTime` | `World::check_pk_status` |
| 清位 + 广播 | `:4361-4362` `PvpFlag = false; RefNameColor();` | 清位并返回 `NameColorDelivery`（色 = 清位后 `GetChrColor`，玩家 ctor=255）；宿主 `drain_pk_flag_downlink` 出 `SM_CHANGENAMECOLOR(656)`，优先级 154，含自己 |
| 周期 | `PlayObject.Message.cs:257-262` 的 `DecLightItemDrugTime` 块（`GameSvrConf.cs:1398` 缺省 500） | `World::check_pk_status` 内按 `dec_light_item_drug_time_ms`（缺省 500）节流；宿主每 world tick 调一次 |
| 配置 | `ServerConf.cs:156` `PKFlagTime`；`:159` `DecLightItemDrugTime`；`GameSvrConf.cs:1395/1398` 缺省 60_000/500 | `PKFlagTime` 已从 `Server.conf` 装载（空值回落 60_000）；`DecLightItemDrugTime` 固定 oracle 缺省 500（未解析该键，登记） |

### 用例

`tests::p3res_pk_flag_expiry_59s_61s_one_656_and_refresh`（生产 drain + 世界时钟）：
- t=59s：`check_pk_status` 不清位、零 656；
- t=61s：清位；caster 自己（911）与观察者（912）**各恰一条** 656，范围外 914 零；
  656 载荷 `Recog=caster`、`Param=255`；
- 重复置位刷新：t=61s 重新置位；t=91s 再置一次（已带 flag ⇒ 返回 None、零 656）；
  t=150s（91+59）不清；t=152s（91+61）清且再次恰一条 656/观察者。

### T-78 修复：节流游标惰性初始化（2026-10-18）

**oracle**（`PlayObject.cs:969`）：`DecLightItemDrugTick = HUtil32.GetTickCount();`
—— **每玩家**构造时以创建时刻的墙钟起算；`PlayObject.Message.cs:257-259` 仅在
`(GetTickCount() - DecLightItemDrugTick) > Config.DecLightItemDrugTime`（严格 `>`）时进入周期块，
块内先 `DecLightItemDrugTick += Config.DecLightItemDrugTime` 再调 `CheckPkStatus()`。

**本仓缺陷**：`World::pk_check_tick` 构造时为 `0`。`clock_ms` 从 0 起（未注入）时与 oracle 同构；
但宿主用 `MIR2_CLOCK_MS` 把 `clock_ms` 注入成墙钟量级（≈1.7e12）后，`clock_ms - 0 > 500` **恒真**
⇒ 过期检查退化为每 world tick 一次（200ms），500ms 节流失效。

**修法（最小）**：`World::check_pk_status` 首次调用时把 `pk_check_tick` 惰性钉到当时的 `clock_ms`
（新增内部 `bool` 标志 `pk_check_tick_initialized`；公开字段 `pk_check_tick` 的类型与手工赋值语义
不变，宿主强制提前检查的写法照旧可用）。之后仍按 oracle 的严格 `>` 门 + `+= 周期` 推进。
未注入（从 0 起）时首调基线 = 0 ⇒ 与修复前逐位相同；60s 过期判定与 `dwPKFlagTime` 语义未动。

**用例（构造级 + 宿主级）**：

- `world::tests::pk_check_throttle_holds_with_injected_wall_clock`：`clock_ms=1_700_000_000_000`、
  过期 61s 的 `PvpFlag` —— 首调只建基线（游标 = 1.7e12，不检查、不清位）；+200/+500ms 不检查；
  +600ms 恰一条 656 载荷并清位。
- `world::tests::pk_check_throttle_unchanged_when_clock_starts_at_zero`：从 0 起，200/500ms
  不检查、600ms 恰一条；游标 `0 → 500`（与修复前相同）。
- `world::tests::pk_check_expiry_boundary_strictly_greater_reports_once`：恰 500ms 不推进游标；
  恰 60s 不清位；60s+1ms 恰一条 656；再调不重复。
- `game-svr` `tests::p3res_pk_throttle_holds_under_injected_wall_clock`：同一场景走生产
  `drain_pk_flag_downlink`，断言 656 帧数为 0/0/1（自己）+1（观察者）+0（范围外）。

### 登记（计时面）——`window_required: true`（本批未占窗）

oracle 的 `CheckPkStatus` 跑在**真实墙钟**的玩家 `Run()` 周期里（`DecLightItemDrugTick` 是
**每玩家**的节流游标）；本仓世界时钟是 200ms 虚拟 tick，宿主每 tick 调一次、方法内按 500ms
节流（`World::pk_check_tick` 是**全世界单一**游标）⇒ 实际检查点在 600/1200/1600/2200ms…
（与 oracle 的 500/1000/1500 相差 200ms 量化）。过期判定本身用虚拟毫秒，构造级可稳定复现；
**真机帧时序**按 R 级窗口另行对拍（本批未占窗口）。

**T-78 修复（2026-10-18）**：上段未列出的「游标从 0 起」在 `MIR2_CLOCK_MS` 注入下使节流恒真，
已由「首调惰性对齐 `clock_ms`」消除（见上小节）；剩余偏差只有 ① 200ms 虚拟 tick 量化、
② 全世界单游标 vs 每玩家 `DecLightItemDrugTick`、③ `DecLightItemDrugTime` 键未解析。
三者都不改变 60s 过期判据与 656 载荷内容。

**R 级判据（`window_required: true`）**：`PvpFlag` 60s 过期与 656 广播的**帧序**属 R 面
（时序 + 帧字节）。本批只交**构造级**证据，**缺证清单**：

1. **60s 过期帧序**：真机（冻结客户端）在「置位 → 59s → 61s」窗口内，656
   `SM_CHANGENAMECOLOR` 相对其它出帧（移动/攻击/聊天）的**全局次序**与**帧数**未对拍；
2. **657 末参字节**：`SM_CHARSTATUSCHANGED` 的 `Series`（= `HitSpeed`）在真机逐收件人
   字节级未对拍（构造级已证明取自己的值，但未证明与 oracle 二进制流逐字节一致）。

---

## 4. 魔号 50 的 `Level=0` / `R(nLevel=0)` 单测

合同 `p3-b4-contract-20261017.md` §6-7：`.NET Random(0)` 仍推进一次 `Sample()`（值恒 0）；
Rust `random_below(0)` 同口径（`DotNetRandom::next_max` 先 `sample()`）。

`tests::spell_p3b4_50_mab_mabe_level_zero_advances_r_zero_once`（生产入口 `handle_spell_cmd`，
seed=4，夹具同 F50-D2，仅 `MagicRcd.level: 2 → 0`）：

- 抽数 max 序 `[0,0,7,10,10,2,100,21,1,0,3]`，值
  `[(0,0),(0,0),(7,1),(10,6),(10,1),(2,1),(100,4),(21,2),(1,0),(0,0),(3,2)]`
  ⇒ 第 10 抽 `R(nLevel=0)` 恰一次且恒 0，训练尾 `R(3)` 位置不变；
- 帧：target(913) `[17,638,656]`；caster(911) `[53,638,656]`；
- `RM_POISON` 恰一条，`actor_id = target`，`nPoint = 0`。

---

## 5. 牙（逐条注入 → 跑指定用例 → 字节级还原）

| # | 改坏 | 结果 | 还原校验（sha256） |
| --- | --- | --- | --- |
| ① | 删除 `MAGTURNUNDEADLEVEL` 解析块 | `config::tests::mag_turn_undead_level_and_pk_flag_time_read_with_defaults` **FAILED**（键=30 读成 50） | `4f8b4681…11592f59`（备份与还原后逐字节相同） |
| ② | `apply_ability_to_entity` 的 `e.hit_speed = hit_speed` 改成 `let _ = hit_speed` | `p3res_hit_speed_written_from_recalc_drives_657_tail` **FAILED**（left 0 / right 3） | `ff0e76a3…e76fdf9` |
| ③ | `check_pk_status` 的过期支改成 `if true { continue; }` | `p3res_pk_flag_expiry_59s_61s_one_656_and_refresh` **FAILED**（61s 自己 656 = 0） | `70345f8c…c5c22743` |
| ④ | `handle_bag_cmd` 的 `e.hit_speed = sg.hit_speed` 改成 `let _ = (e, sg.hit_speed)` | `p3res_bag_take_on_off_syncs_hit_speed_to_entity` **FAILED**：`穿装重算后必须同步 Entity.hit_speed`（left 0 / right 3） | `f06a45a8f078ca7129ec78fb7341fe0a250f2167b7d4cb79e01a818abcf7724`（备份与还原后逐字节相同） |
| ⑤ | 删除 36 支里 `e.hit_speed = sg.hit_speed`（改 `let _ = sg.hit_speed`） | `p3res_spell_36_target_recalc_syncs_hit_speed_to_entity` **FAILED**：`36 目标重算后的 HitSpeed 必须同步到实体`（left 99 / right 5） | 同上 `f06a45a8…bcf7724`（还原后逐字节相同） |
| ⑥ | `check_pk_status` 的惰性初始化块 `if !pk_check_tick_initialized` 改成 `if false`（等价游标从 0 起） | `pk_check_throttle_holds_with_injected_wall_clock` / `pk_check_expiry_boundary_strictly_greater_reports_once` / 宿主 `p3res_pk_throttle_holds_under_injected_wall_clock` **FAILED**（首调即 1 条 656；`pk_check_tick` left 500 / right 1700000000000） | `world.rs` / `main.rs` 还原后 sha256 与改动后相同（`28c55dfe…d6cfed9`） |
| ⑦ | 节流门 `<=` 改成 `<` | 三条世界用例全 **FAILED**（`T0+500` 即推进游标：left `1700000000500` / right `1700000000000`） | 同上（`28c55dfe…d6cfed9`） |

五处均按原文件字节还原后才进入门禁复跑（④/⑤ 的 `main.rs` 之后仅经 `cargo fmt` 重排测试字面量）。

---

## 6. 门禁读数（cwd=`mir2-rs`，worktree `/e/tmp/impl-p3resb`，`CARGO_TARGET_DIR=/e/tmp/p3resb-target`）

| 门 | 读数 |
| --- | --- |
| `cargo fmt --all -- --check` | rc=0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 |
| `cargo test --workspace` | rc=0；**112 个 suite（92 个非空）/ 1695 passed / 0 failed / 26 ignored**（输入补丁为 1693 passed；本批新增 2 条生产入口用例 ⇒ 1695） |
| `cargo run -q -p msgcodegen -- --check` | rc=0；`604 条常量` |
| `recon_gates.py content` | rc=0；路径归一后与 `/e/tmp/verify-p3resb`（a8f3becd0）**IDENTICAL**（diff 0 行）；summary 不变 |
| `recon_gates.py dispatch` | rc=0；路径+行号+空白归一后 **IDENTICAL**（diff 0 行）；summary `implemented 149 / tracked 38 / n/a-unreachable 3 / n/a-oracle-noop 1` |
| `recon_gates.py selftest` / `mutate` | 均 rc=0 `RESULT: ALL PASS`（impl 与基线两树） |
| `whitelist_anchor_gate.py check` | rc=0 `RESULT: ALL PASS（176 条锚）`（T-78 只用符号锚 ⇒ 锚数不变） |
| `m3_whitelist_registry_audit.py audit` | rc=0 `WHITELIST_REGISTRY GREEN`（entries=41，含 T-78；duplicates=none） |
| `fixture_chain_gate.py check` | rc=0 `FIXTURE_CHAIN GREEN: runner_ok=10 prereq_or_skip=0 red=0` |
| `rng-trace-scene/check_anchors.py check / selftest` | rc=0 `GREEN 7/7` / rc=0 `SELFTEST GREEN 17/17` |
| RNG selftest ×4 | consumer_groups **20/20**、ledger_diff **37/37**、scene_gate **22/22**（`SELFTEST ok=22/22`）、attack_rng_ledger **51/51**；均 rc=0 |
| 五批 trace `closed` | B1 31（19 magic_effect + 12 attack）、B2 45（33+12）、B3 18（18 magic_effect）、B4 89（75+14）、B5 35（27+8）；`rng_ledger_diff.py groups <tsv> --side rust --require-closed` 全部 rc=0 `GROUP_COVERAGE closed` + `ROW_COVERAGE closed rows_dropped=0` |

既有 B1–B5 用例期望未被改变（全 workspace 0 failed）；本批新增的期望只落在新用例内。

---

## 7. 未决项 / 已知偏差（登记，不忽略）

1. **Pvp 过期计时面（R 级，`window_required: true`，未占窗）**：见 §3「登记」——200ms
   虚拟时钟量化 + 全世界单游标 `World::pk_check_tick` vs oracle 每玩家 `DecLightItemDrugTick`
   + `DecLightItemDrugTime` 未解析（固定 oracle 缺省 500）。已在 `whitelist.md` 登记 **T-78**。
   **缺证清单**：① 真机 60s 过期窗口的 656 **帧序**（相对移动/攻击/聊天帧的全局次序与帧数）；
   ② 真机 657 `SM_CHARSTATUSCHANGED` **末参字节**（`HitSpeed` 逐收件人逐字节）。
2. **`DecLightItemDrugTime` 键**：`Server.conf` 若覆盖该键，本仓仍按 500。属于计时配置面，
   与 §3 同一登记（T-78）；部署值 500/60000 下当前无可观测差异。
3. **`WAbil` 其余字段的装备重算实体同步**：本批只把 `hit_speed` 在装备/卸下重算后同步到
   `Entity`；`dc/ac/hit_point/speed_point/hp/max_hp/level/anti_magic/mac` 的装备后同步仍是既有
   缺口（登录/升级点已同步）。
4. **`HitSpeed` 的 `RealAttackSpeed` 输入**：沿用既有口径「有效物品 = 模板」
   （`GetUpgradeStdItem` 恒等），升级差量未建模（既有登记，本批不扩大）。
5. **`ExtraAbil[EABIL_HITSPEEDUP]`**：oracle 在 `:2832` 加到 `HitSpeed` 后被 `:2899` 覆盖，
   本批按 oracle 原样未建模该死支。
6. 本批未起服务、未占端口、未写库；未改冻结面。

---

## 8. 残留物清单（保留，不删）

- worktree：`/e/tmp/impl-p3resb`（实现）、`/e/tmp/verify-p3resb`（基线对照）。
- target：`/e/tmp/p3resb-target`（实现）、`/e/tmp/verify-p3resb-target`（复验；按批未用则不存在）。
- 证据：`/e/tmp/p3resb-fmt.log`、`/e/tmp/p3resb-clippy.log`、`/e/tmp/p3resb-test.log`、
  `/e/tmp/p3resb-msgcodegen.log`、`/e/tmp/p3resb-{content,dispatch}-{base,impl}.{txt,norm}`、
  `/e/tmp/p3resb-{content,dispatch}.diff`、`/e/tmp/p3resb-recon-{selftest,mutate}*.txt`、
  `/e/tmp/p3resb-{fixture_chain,rng-anchors,rng-anchors-selftest}.log`、
  `/e/tmp/p3resb-rng-*-selftest.log`、`/e/tmp/p3resb-b{1,2,3,4,5}.tsv` 及对应
  `p3resb-b*-test.log`。
- 牙备份：`/e/tmp/p3resb-main.bak`（仅作还原校验；内容不在仓库）。
- 驱动脚本：`/e/tmp/p3resb-norm.py`、`/e/tmp/p3resb-newtests.txt`、`/e/tmp/p3resb-t78.md`
  （不落库）。

---

## 9. T-78 节流游标修复（2026-10-18，patch `pktick`）

基线 `429c714a4`；实现 worktree `/e/tmp/impl-pktick`，复验 `/e/tmp/verify-pktick`（同基线）。
本片只动两处：`crates/world/src/world.rs`（惰性初始化 + 3 条世界用例）与
`bins/game-svr/src/main.rs`（1 条端到端用例）；未动 `src/**`、`sql/**`、`MirClient/`、
`crates/{protocol,storage}/**`、`E:/MirServer/**`，未起服务、未占端口、未写库。

### oracle 差（修前 → 修后）

| 点 | oracle | 修前本仓 | 修后本仓 |
| --- | --- | --- | --- |
| 游标初值 | 每玩家构造时 `GetTickCount()`（`PlayObject.cs:969`） | 全世界单一游标，构造时为 `0` | 全世界单一游标，**首调惰性对齐 `clock_ms`** |
| 注入 `MIR2_CLOCK_MS`（≈1.7e12） | 不受影响（初值即当时墙钟） | `clock - 0 > 500` 恒真 ⇒ 每 tick 检查 | 500ms 节流成立（首调基线 = 注入时钟） |
| 未注入（`clock_ms` 从 0 起） | 500ms 周期 | 500ms 周期 | **逐位不变**（首调基线 = 0） |
| 过期门 | `(now - PvpNameColorTick) > dwPKFlagTime`（严格 `>`） | 同（`<=` 早退） | 未动 |
| 周期推进 | `DecLightItemDrugTick += 周期` | `pk_check_tick += 周期` | 未动 |

剩余登记（T-78 未修部分）：200ms 虚拟 tick 量化、全世界单游标 vs 每玩家游标、
`DecLightItemDrugTime` 键未从 `Server.conf` 解析。真机帧序仍 `window_required: true`。

### 门禁读数（cwd=`mir2-rs`，`CARGO_TARGET_DIR=/e/tmp/pktick-target`）

| 门 | 读数 |
| --- | --- |
| `cargo fmt --all -- --check` | rc=0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 |
| `cargo test --workspace` | rc=0；112 suites / **1699 passed / 0 failed / 26 ignored**（基线 1695 ⇒ 本片 +4 条用例） |
| `cargo run -q -p msgcodegen -- --check` | rc=0；604 条常量 |
| `recon_gates.py content` / `dispatch` / `selftest` / `mutate` | 均 rc=0；路径（+ dispatch 行号）+ 空白归一后与 `/e/tmp/verify-pktick`（429c714a4）**IDENTICAL**（4/4 diff 0 行） |
| `whitelist_anchor_gate.py check` | rc=0 `RESULT: ALL PASS（176 条锚）` |
| `m3_whitelist_registry_audit.py audit` | rc=0 `WHITELIST_REGISTRY GREEN` |
| `fixture_chain_gate.py check` | rc=0 `FIXTURE_CHAIN GREEN: runner_ok=10 prereq_or_skip=0 red=0` |
| `rng-trace-scene/check_anchors.py check / selftest` | rc=0 `GREEN 7/7` / rc=0 `SELFTEST GREEN 17/17` |
| RNG selftest ×4 | consumer_groups **20/20**、ledger_diff **37/37**、scene_gate **22/22**、attack_rng_ledger **51/51**；均 rc=0 |
| 五批 trace `closed` | B1/B2/B3/B4/B5 均 rc=0 `GROUP_COVERAGE closed` + `ROW_COVERAGE closed rows_dropped=0` |

### 牙（逐条注入 → 跑用例 → 字节级还原）

| # | 改坏 | 结果 | 还原校验 |
| --- | --- | --- | --- |
| ① | 惰性初始化块改 `if false`（等价游标从 0 起） | 世界 2 条 + 宿主 1 条 **FAILED**（首调即 1 条 656；`pk_check_tick` left 500 / right 1.7e12） | `world.rs` / `main.rs` sha256 与改动后相同 |
| ② | 节流门 `<=` → `<` | 世界 3 条全 **FAILED**（`T0+500` 推进游标，left `1700000000500` / right `1700000000000`） | 同上 |

### 残留物清单（保留，不删）

- worktree：`/e/tmp/impl-pktick`（实现）、`/e/tmp/verify-pktick`（基线对照）。
- target：`/e/tmp/pktick-target`（实现；复验树未单独建 target）。
- 临时日志/备份：`/e/tmp/pktick-test.log`、`/e/tmp/impl-recon-*.txt`、`/e/tmp/verify-recon-*.txt`、
  `/e/tmp/pktick-world.rs.bak`、`/e/tmp/pktick-main.rs.bak`（牙还原备份）。
