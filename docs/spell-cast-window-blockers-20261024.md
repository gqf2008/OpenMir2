# `spell_cast` 可跑性判定：两条排除理由的拆解

- 卡片：`openmir2-spell-cast-window-blockers`（tier=L）
- 作者：`openmir2-worker-58`（worktree `OpenMir2-wt-worker-58`，分支 `card/spellcast-blockers`）
- base = master `e377d7058`；本文所有行号/用例名/输出均在本 head 上**现取**（2026-10-24）
- 本卡**不改** `docs/window-batch-plan-20261023.md`（drift-sweep 领域）、`src/**`、`tools/conformance/**`、`rules.json`；
  `tools/probe/scenarios/spell_cast.json` 也**未改**（现有等待集合已是 638/639，不含 17，无需删）
- 被拆解的原文：`docs/window-batch-plan-20261023.md` §7.3 第 468 行 / §7.4 第 476、495 行
  「**Rust 侧魔法落点 + 第二观察者未接 ⇒ 真机仍跑不了**」

## 0. 结论摘要

| # | 原排除理由 | 判定 | 挡不挡开窗 |
|---|---|---|---|
| ① | 需要第二观察者（为了验 `SM_SPELL(17)`） | **不需要**：场景判据只等 638/639；`SM_SPELL(17)` 到施法者自己那条连接**恒不可见**（oracle self 守卫），单连接永远等不到，多连接才需要第二观察者 | **不挡**；应从排除项**删除**（或降级为「不覆盖项」） |
| ② | 缺 Rust 魔法落点 | **不成立（措辞已过期）**：Rust 生产侧 `CM_SPELL` 派发 → MP 包装层/53 → 17 广播 → 版本门 → 距离门/639 → 效果体/638 全部在 master；成功支 638、距离门失败支 639、无目标 `ActorId=0` 路径均有用例，本卡实跑 7 个关键用例 + 98 个 `spell_*` 用例全绿 | **不挡**；应删除该措辞 |
| ③ | （**本卡新发现，不在原两条里**） | `compare_runs.py` 对出现过的 ident 要求规则表覆盖；`rules.json` **没有 53/638/639** ⇒ 本次跑完必判 `UNDECIDABLE (exit 2)`，拿不到 PASS 判定 | **挡**（当前唯一真挡项）；属「缺判据规则」，既非缺实现也非缺用例 |

⇒ **「若要移入 §7.4 现在就能跑，还差哪一步」的一句话清单：还差 1 步 —— 给 `rules.json` 补 `53`（SM_HEALTHSPELLCHANGED）、`638`（SM_MAGICFIRE）、`639`（SM_MAGICFIRE_FAIL）三条比较规则（走评审），或经评审在 profile `compare_extra` 里显式 `--allow-unclassified 53,638,639`；否则对拍必 rc=2。**

> 口径声明：本文的「Rust 已实现」= 生产派发点 + 逐行移植点 + 构造级用例全绿；**真机两侧逐字节 diff=0 仍需开窗**，
> 这属于窗口判据本身，不是实现缺口。

---

## 1. ① 是否必须验 `SM_SPELL(17)` / 第二观察者

### 1.1 当前判据（现取，不靠转述）

`tools/probe/scenarios/spell_cast.json` 的等待集合（`python -I -c` 解析 steps）：

```text
name spell_cast requires ['magic']
0 ['send', 'note'] None
1 ['wait_for', 'timeout_ms', 'note'] 638
2 ['sleep', 'note'] None
3 ['send', 'note'] None
4 ['wait_for', 'timeout_ms', 'note'] 639
```

⇒ 等待集合 = **`{638, 639}`**，**不含 17**。场景的 5 条步骤里没有 `wait_idents`/目标帧集字段，
`README.md`（`tools/probe/scenarios/README.md:232-236`）与
`docs/spell-cast-scenario-20261023.md` §5 也把 17 明确列为「不覆盖」。**判据只认 638/639 这一前提成立。**

### 1.2 为什么 17 不能作为自己连接的判据（oracle 逐行）

`src/M2Server/Player/PlayObject.Message.cs:1454-1459`：

```text
case Messages.RM_SPELL:
    if (processMsg.ActorId != ActorId)      // ← 自己发的 RM_SPELL，ActorId == 自己
    { ClientMsg = MakeMessage(SM_SPELL, …); SendSocket(ClientMsg, nParam3.ToString()); }
```

施法者自己那条连接**永远**收不到自己的 17。相反，638/639 的处理器**没有**这个守卫
（`:1788-1793` / `:1794-1800`，本卡 `sed -n` 实读），所以单连接就能判成功/失败两支。

Rust 侧与此一致：`SM_SPELL` 走 `broadcast_ref_msg`（排除自己），`638/639` 走
`broadcast_ref_msg_with_self`（含自己）；用例
`tests::spell_frame_never_reaches_caster_connection`（`mir2-rs/bins/game-svr/src/lib.rs:47558`）
断言施法者收 `0` 条 `SM_SPELL`、仍收 `+GD` ack，观察者恰 `1` 条（`:47559-47573`）。

### 1.3 结论与建议

- **结论：第二观察者不是必需。** 现有判据（638/639）在**单探针连接**上即可判定；
  要把 17 写进等待集合才会恒超时（场景 `description` 已把这条写成「不能进等待集合」）。
- **建议（给 owner，本卡不改 batch-plan）**：把 §7.3/§7.4 的「第二观察者」从**排除理由**删除；
  若要保留这项能力，写成**不覆盖项**而不是「跑不了」的理由，并标注「17 需要第二会话，另卡」。
- **如果将来确需验 17，最小要求（本卡只给要求，不开卡）**：
  1. 同服同图 ±`SendRefMsgRange(=12)` 内**第二个在线玩家**（现基线角色可另取账号/角色，例如同库其它 `winplayN`），
     它**只收不发**即可（17 的观察者侧无回帧要求）；
  2. 帧流要能按会话区分（`s1.*` / `s2.*`），且场景 DSL 能对「观察者会话」做 `wait_for`；
  3. **现有 `mir2_probe` 不能直接满足**：CLI 只有一条 login→selgate→gamegate 链、无 `--session`/第二连接选项
     （`tools/probe/mir2_probe.py:1668-1701` 的 argparse）；多会话**断言**辅助函数已在
     （`session_of`/`recipients`/`within_square`，`:394-529`），但那是**离线行级断言**，不驱动第二条连接；
     `card_window.py` 每一侧只起一个探针（`:2729-2734`、资源 `PROBE` 在 `:147`）；
     多会话卡 `openmir2-multi-session-probe` 已 **closed**（合入 master `2a5101606`），scope 就是
     「probe 断言、不做第二会话」。
  4. 承接**建议 id**：`openmir2-spell-second-observer`（**当前不存在，未开卡，需 coordinator 派**；
     该建议名与 `docs/window-batch-rehearsal-20261024.md:290` 一致）。

---

## 2. ② 「缺 Rust 魔法落点」到底缺什么

### 2.1 生产侧链路（逐点现取）

| 环节 | 位置（master `e377d7058`） | 说明 |
|---|---|---|
| 入帧派发（生产） | `mir2-rs/bins/game-svr/src/lib.rs:21207-21238` | `magicwire::is_spell_ident(cmd.ident)` → `handle_spell_cmd_with_magic(...)`，传真实 `enter_cfg.magic_table` / `pet_cfg` / `magic_attack_rage` |
| 入口 | `lib.rs:10022`（`handle_spell_cmd`）/ `:10074`（`handle_spell_cmd_with_magic`） | 登录会话/世界实体/`GetMagicInfo` 守卫 |
| 入帧字段 | `bins/game-svr/src/magicwire.rs:127` `parse_spell` | `tag=魔号`、`recog=目标坐标`、`param+series=MakeLong → 目标 ActorId`（`:241-247` 用例 `parse_spell_maps_world_server_fields`） |
| 「已学魔法」来源 | `lib.rs:10128-10133` → `SavedHuman.record.data.magic`；登录装载 `lib.rs:21924/21945` | DB `characters_magic` → `bins/db-srv/src/playdata.rs:200-229 get_magic_record`（`SELECT * FROM characters_magic WHERE PlayerId=?`）→ 登录审计 `magicaudit::filter_for_logon`（`magicaudit.rs:71-81`） |
| 落点坐标（snap） | `lib.rs:9375 spell_snap_target` + `queryuser.rs:29 cetin_near_xy` | `target_actor_id=0` 永不命中（`o.cell_obj_id > 0` 门，`:40`）⇒ 与 oracle `ActorMgr.Get(0)==null` 同义，坐标保持点击格 |
| MP 包装层 / 53 | `lib.rs:10227-10268` | `get_spell_point`（`:10228`）→ 扣 MP（`:10244`）→ 53 发自己（`:10258`） |
| 17 广播 | `lib.rs:10353-10370` | `smframe::sm_spell(...)` + `broadcast_ref_msg`（排除自己） |
| 版本门 | `lib.rs:10397-10417` | 失败补一条 639 并返回；`soft_version_date` 由 gate 登录串写入（`lib.rs:23892`） |
| 距离门 / 639 | `lib.rs:10361`（`magic::spell_in_range_i32`，`crates/world/src/magic.rs:381-389`）→ 失败分支 `lib.rs:10430-10453` | 越界恰一条 639（含自己） |
| 成功支 638（魔号 1） | `lib.rs:11272`（B1 集合 `1|5|11|13|44|45`）→ `:11380 cast_fireball_spell` → `finish_spell_tail`（`:9743`，调用点 `:11466`） | 共享尾经 `crates/world/src/magic.rs::do_spell_tail` 产 638（`smframe::sm_magicfire`，`smframe.rs:795-811`，body = EDCode(int32 target ActorId)） |
| 魔表（`Magic.Effect`） | `bins/game-svr/src/lib.rs:653-667`（`magics::load_magics` → `MagicTable`） | 火球术行来自 `magics` 表 |

结论：**「落点」所指的四件事——入帧映射、目标坐标 snap、638/639 生产、登录魔法装载——在当前 master 都有生产实现点。**

### 2.2 用例与断言（本卡实跑，`cargo test -q -p game-svr --lib <名>` 各 `1 passed`）

| 覆盖 | 用例（`mir2-rs/bins/game-svr/src/lib.rs`） | 关键断言 |
|---|---|---|
| 成功支 638（魔号 1/5，**带目标**） | `tests::spell_p3b1_fireball_family_draw_order_and_frames`（`:50429`） | 观察者恰 1 条 `SM_MAGICFIRE`（`:50447-50453`）、自己恰 1 条（`:50454-50458`）、638 的 `ActorId=目标 id`（`:50469-50471`）、坐标 = snap 后 `(11,10)`、恰 6 抽且与手写 oracle 序逐项相同（`:50472-50490`） |
| 成功支 638（**魔号 1 + 无目标 `ActorId=0`**） | `tests::spell_cmd_version_gate_double_zero_then_1018_recovers`（`:51203`） | 1018 后帧面逐条 `[(912,SM_SPELL),(911,SM_MAGICFIRE),(912,SM_MAGICFIRE)]`（`:51306-51314`），`param=series=0 ⇒ target=0`；**零抽**（`:51315-51319`，注释「魔号 1 无目标 ⇒ 效果体零抽；门已放行、尾帧照发」） |
| 成功支 638（A 类无 case 魔号 + 无目标） | `tests::spell_tail_p1_no_switch_fire_only_zero_draws`（`:49854`） | 观察者 1 条 17 + 1 条 638 且 17 在前（`:49864-49886`）、自己 1 条 638 / 0 条 17（`:49888-49899`）、`638 ActorId=0`（`:49914-49919`）、坐标 `(12,11)`、零抽（`:49929`） |
| 成功支 638（魔号 1 命中门失败 ⇒ 目标归一 0） | `tests::spell_p3b1_failure_branches_null_rules_and_zero_draws`（`:50822`） | 骰失败/不可攻击/死目标三支 `638 ActorId=0`（`:50902-50910` 等）、零抽 |
| 距离门失败 639（生产入口、逐字节） | `tests::spell_cmd_distance_gate_failure_emits_exactly_one_639`（`:48436`） | 捕获队列**恰 1 帧**（`:48445-48450`）、发给施法者自己（`:48451`）、ident=639（`:48452-48456`）、**逐字节 = `smframe_ext::sm_magicfire_fail(caster)`**（`:48457-48461`）、无 `SM_SPELL`（`:48462-48467`）、无 53（`:48468-48473`）、MP 不变（`:48474`）、世界 RNG **零抽**（`:48477-48484`）、越界仍回 `+GD`（`:48492-48499`） |
| 距离门失败 639（另一批魔号） | `tests::spell_tail_p1_out_of_range_sends_only_639`（`:50157`） | 自己+观察者各恰 1 条 639、0 条 638/640、零抽（`:50166-50193`） |
| 17 自己不可见（①的 Rust 侧证据） | `tests::spell_frame_never_reaches_caster_connection`（`:47558`） | 自己 0 条 17、有 `+GD`、观察者恰 1 条 17（`:47559-47573`） |

批量实跑（同一 head）：

```text
$ cargo test -p game-svr --lib spell_
test result: ok. 98 passed; 0 failed; 0 ignored; ... (0.12s)

$ cargo test -q -p game-svr --lib <上表 7 个名字>   # 逐个
test result: ok. 1 passed; 0 failed; ...            # ×7
```

### 2.3 「无目标地面施法（目标 ActorId=0）」这条路径

**已有用例**，而且是两条独立证据：

1. `spell_cmd_version_gate_double_zero_then_1018_recovers`：`param=0, series=0` ⇒
   `parse_spell.target_actor_id = MakeLong(0,0) = 0`（`magicwire.rs:127-134`），魔号 1，
   断言 638 且零抽（见上表）；
2. `spell_tail_p1_no_switch_fire_only_zero_draws`：`rig.cast(magic, (12,11), 0)`，
   显式断言 `638` 的 `ActorId=0`。

`cetin_near_xy` 对 `target_id=0` 恒不命中（`queryuser.rs:40` 的 `o.cell_obj_id > 0`），
与 oracle `ActorMgr.Get(0)==null` 一致 ⇒ 不会因为「世界里恰好有 id=0 的实体」而误 snap。

### 2.4 缺什么？逐条点名

- **缺实现？没有找到**。本场景路径（魔号 1 / 无目标 / 地面 / 距离门失败）所需的每个生产点在 master 都在（§2.1）。
- **缺用例？只有两处「组合级」空白，不构成挡开窗**，点名如下：
  1. **没有**「魔号 1 + 等级 0（场景夹具 `level=0`）」的端到端单测——现有 638 用例用 `level=1`。
     等级 0 只影响 `GetSpellPoint` 与命中伤害；`get_spell_point(level=0)` 已由
     `mir2-rs/crates/world/tests/dispatch_surface_vectors.rs:399` 对 oracle 向量复核，且本场景无命中/零抽 ⇒
     判定为**不影响**（非缺口）。
  2. **没有**「魔号 1 的距离门 639」直接用例——现有 639 用例用魔号 14/8/55/49。距离门在
     `switch (MagicId)` **之前**（`MagicManager.cs:155` 早于 `:171`；Rust `lib.rs:10361` 同样先于 B1 块），
     判据与魔号无关 ⇒ 判定为**由构造覆盖**（非缺口）。
- **真正的空白不在这两条里**：见 §3（比较器规则），那是「缺判据登记」，**不是**「缺 Rust 实现」也不是「缺用例」。

---

## 3. 新发现（本卡必须报）：比较器未分类会挡开窗

`tools/conformance/compare_runs.py` 的判据 4（文件头 `:47-49`、实现 `:1062-1067`、`:1435-1437`）：

> 某 ident 在两侧任何一帧里出现、规则表却没有它的条目 ⇒ **UNCLASSIFIED ⇒ 退出码 2**。

`tools/conformance/rules.json` 的 `rules[].ident` 集合（本卡 Python 现取）**不含** `17/53/638/639/28`：

```text
$ python -I -c "…读 rules.json，检查 17/53/638/639/28/3017 是否在规则 ident 集合…"
17 False
53 False
638 False
639 False
28 False
3017 False
```

而 `spell_cast` 单会话跑必然会出 **53** 与 **638/639**：

- **53**：`HealthSpellChanged()` 对 `Race==Play` **无条件** `SendUpdateMsg`（自己）——
  `src/M2Server/Actor/BaseObject.cs:700-709`；Rust 对应 `lib.rs:10247-10260`（`send_self_frame`），
  两发各一条（火球术 level 0 耗蓝 `Round(4/4*1)+1 = 2`）。
- **638/639**：见 §2。
- **17 不出现在帧流里**（单连接、无观察者，自己被排除）⇒ 当前不会触发 17 的 UNCLASSIFIED；
  若将来加第二观察者，`17` 也必须一并分类。

### 3.1 离线复现（构造两条只含 53/638/639 的 `frames.ndjson`）

```text
$ python -I -B tools/conformance/compare_runs.py \
    --oracle E:/tmp/w58-cmp/oracle --rust E:/tmp/w58-cmp/rust \
    --out E:/tmp/w58-cmp/report.json --rules tools/conformance/rules.json
idents: [53, 638, 639]
DIFFS: 0
UNDECIDABLE: 3
  coverage/unclassified_ident: ident 53 在产物里出现但规则表没有条目（UNCLASSIFIED）
  coverage/unclassified_ident: ident 638 在产物里出现但规则表没有条目（UNCLASSIFIED）
  coverage/unclassified_ident: ident 639 在产物里出现但规则表没有条目（UNCLASSIFIED）
VERDICT: UNDECIDABLE (exit 2)          # rc=2
```

显式放行（只证明分类口径，不证明真机逐字节相等）：

```text
$ ... 同上 ... --allow-unclassified 53,638,639
whitelisted: 3 条（allowed_unclassified ×3）
VERDICT: PASS (exit 0)                  # rc=0
```

⇒ **即便 ①② 两条都解除，`spell_cast` 现在跑完也只会得到 `UNDECIDABLE (2)`，不是 PASS。**
`card_window.py` 组 `compare` 命令时不带 `--allow-unclassified`（`:2729-2734`、`:3304-3307`），
只读仓库 `rules.json` ⇒ 这是**执行侧必现**，不是理论问题。

### 3.2 这一步归谁

- `rules.json` 在本卡**禁止改**（「改规则 = 改判据，必须走评审」，见 `rules.json` 的 `note` 与
  `compare_runs.py:65-66`）。
- 可行两条路（**都要 owner/评审拍板，本卡不实施**）：
  1. 建议 id `openmir2-spell-frame-compare-rules`（**未开**）〔2026-10-24 订正：该 id 已立卡并在 `89a3d02fd` 合入 master（tier=L，approve reviewer-3 `0b8dec26`）；`rules.json` 已补 53/638/639〕：给 `53/638/639` 各写一条
     `action: compare`（逐字节比，不掩码）——首选，因为这三帧都是确定性帧
     （53 = hp/mp；638 = 坐标+ActorId 的 EDCode 体；639 = header-only + Recog）；
  2. 或在**开窗 profile** 的 `compare_extra` 里加 `["--allow-unclassified","53,638,639"]`
     （`card_window.py:2732-2733` 会原样追加），报告里会登记 `allowed_unclassified`；
     这仍需评审，且报告会留痕。

---

## 4. 给 owner 的批次/文档修改建议（本卡不改 batch-plan）

1. `docs/window-batch-plan-20261023.md`
   - §7.3 第 468 行「缺什么」列：删除「**Rust 侧魔法落点 + 第二观察者未接 ⇒ 真机仍跑不了**」，
     改为「**比较器规则表缺 53/638/639 ⇒ 对拍 UNDECIDABLE（见 `docs/spell-cast-window-blockers-20261024.md` §3）**」；
   - §7.4 第 476 行括号、第 495 行「仍不可跑」表的依赖列：同上替换；
   - 若 Leader 决定「能跑」口径=§7.5 四条（不含比较器），则 §7.4 可把 `spell_cast` 移入上表，
     但在「跑前还需」写「`rules.json` 补 53/638/639（否则 UNDECIDABLE）」。
2. 散落同句同步（drift-sweep 领域，本卡只登记）：`docs/docs-state-drift-sweep-20261024.md:133/142`、
   `docs/needs-human-20261024.md:152/178/217`、`docs/window-batch-rehearsal-20261024.md:93/290`、
   `docs/window-criteria-backfill-20261024.md:262-263`（后者已用「真机待窗口」表述，只需把「第二观察者」删掉）。
3. `docs/spell-cast-scenario-20261023.md` §7 顶部「dry-run 默认是假绿」已**过期**（见 §5.4 实测：
   现在默认 `alt` 是 **rc=2 fail-closed**）——建议 drift-sweep 一并订正为「`requires:["magic"]` 已 fail-closed，
   默认 alt 直接 rc=2」。
4. 17 的未来覆盖：见 §1.3 的最小要求与建议 id（`openmir2-spell-second-observer`，未开）。
5. 比较器规则：建议 id `openmir2-spell-frame-compare-rules`（未开）；或 profile `compare_extra` 方案（§3.2）。〔2026-10-24 订正：该 id 已立卡并在 `89a3d02fd` 合入 master（tier=L），`rules.json` 已补 53/638/639〕

---

## 5. 门禁与实跑证据（本 head = `card/spellcast-blockers`，base `e377d7058`）

### 5.1 卡片要求的三项

```text
$ python -I -B tools/probe/mir2_probe.py --selftest
mir2_probe selftest: OK (11 checks; fixtures replayed at 1/2/3/5/7/11/13/17/64-byte chunks)   # rc=0

$ python -I -B tools/conformance/card_window.py --selftest
GREEN: card_window.py selftest 226/226                                                       # rc=0

$ python -I -B tools/whitelist/t_reservation_gate.py check
T_RESERVATION_GATE table=...\mir2-rs\tests\parity\T-RESERVATIONS.md
  rows=255 whitelist=255 live=64 landed=191 released=0 malformed=0 wl_dups=0 fence_ok=True/True max=T-267
T_RESERVATION_GATE GREEN（①-⑤ 与 ⓪ 全过：预约表 255 行 / whitelist 255 号）                # rc=0
```

### 5.2 冲突标记与合并卫生

```text
$ git grep -n -E '^(<{7}|={7}|>{7})( |$)' -- . ':!*.md'
# 0 行（git grep rc=1 = 无匹配）
$ git merge-tree $(git merge-base HEAD walgit/master) HEAD walgit/master ; echo $?
0
```

### 5.3 场景形状 + 关键用例

```text
$ python -I -B tools/probe/mir2_probe.py --check-scenario --scenario tools/probe/scenarios/spell_cast.json
scenario OK: spell_cast (5 steps, params=far_pos=2752541,magic_id=1,near_pos=1835037,self_pos=2162717)  # rc=0

$ cargo test -p game-svr --lib spell_        → 98 passed; 0 failed
$ cargo test -q -p game-svr --lib <§2.2 七个用例> → 各 1 passed
```

### 5.4 dry-run（夹具现状，顺带证伪「默认假绿」）

```text
# ① 默认内置 alt（无 magic 段）——现在是 fail-closed，不是假绿
$ python -I -B tools/conformance/card_window.py --dry-run \
    --scenario tools/probe/scenarios/spell_cast.json --out E:/tmp/w58-spellcast-dry-default
card_window.py: 场景 `spell_cast` 要求 `magic` 夹具，但 profile `alt` 未声明 `magic` 段（或某侧被覆盖成空）：
缺 csharp、rust 侧（场景文件 ...\spell_cast.json）
rc=2；`grep -c 魔法夹具` = 0

# ② 复用已有 `E:/tmp/spellcast-profile.json`（= 内置 alt + magic 段）
$ python -I -B tools/conformance/card_window.py --dry-run \
    --scenario tools/probe/scenarios/spell_cast.json --profile E:/tmp/spellcast-profile.json \
    --out E:/tmp/w58-spellcast-dry-magic
[csharp] 预置位（dry-run 不连库）：map=0122 x=29 y=33 … 可走
[csharp] 魔法夹具（dry-run 不连库）：db=mir2_test_db magic_ids=1
[rust]   预置位（dry-run 不连库）：map=0122 x=29 y=33 … 可走
[rust]   魔法夹具（dry-run 不连库）：db=mir2_test_db_r magic_ids=1
rc=0；`grep -c 魔法夹具` = 2
```

（`--dry-run` 不建 `--out` 目录——两次跑完 `E:/tmp/w58-spellcast-dry-{default,magic}` 均不存在。）

---

## 6. 残留物清单（本卡）

| 路径 | 说明 | 处置 |
|---|---|---|
| `E:/tmp/w58-cmp/{oracle,rust}/frames.ndjson`、`report.json`、`report2.json` | §3.1 构造级 UNCLASSIFIED 复现（只含 53/638/639 的三行帧流） | 本卡收尾**已删** |
| `C:/Users/gxh/AppData/Local/Temp/w58-dry-default.txt`、`w58-dry-magic.txt` | §5.4 两条 dry-run 的 stdout | 本卡收尾**已删** |
| `E:/tmp/spellcast-profile.json` | **非本卡创建**（`docs/spell-cast-scenario-20261023.md` §9-6 已登记的历史临时物），本次仅复用做 dry-run | 保留，归属前卡登记 |
| `E:/tmp/w58-spellcast-dry-default`、`E:/tmp/w58-spellcast-dry-magic` | dry-run 不创建 | 不存在 |

未起栈、未连库、未改 `src/**` / `tools/conformance/**` / `rules.json` / `whitelist.md` /
`docs/window-batch-plan-20261023.md`；`tools/probe/scenarios/spell_cast.json` 未改。
