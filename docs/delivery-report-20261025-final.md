# OpenMir2 交付报告（2026-10-25 终版）

- **快照**：`f72dbce7`（2026-10-25 14:57 +08，最终 tip）。**四条件中 ①③④ 已 PASS，仅 ② 待 owner 对 G11 的一句话。**
- ④ 在最终 tip 用 `goal_gate --full` 复跑过：**门禁 7 项 绿 7 / 红 0 / 需人工 0**（144s；`cargo_test_workspace` rc=0 @117.0s；`master_sync` local==remote==`f72dbce7`），产物 `E:/tmp/window/goal-full-fd08ff3a9.json`（`.meta` 记 `GOAL_RC=2 / TIME_S=144 / HEAD=f72dbce7`；rc=2 来自 ② 的 need_human，不是红门）。
  `cargo test --workspace` 三段数：**running 3315 = passed 3252 + failed 0 + ignored 63**，**141 个 suite 全部 `ok`**。**透明记录**：门内那次（117.0s，`run_head=fd08ff3a9`）json 里只有 rc/耗时没有计数，tester 又**在 `f72dbce7` 上单独跑了一次**（100s，rc=0）取计数；两次 rc 一致、耗时相近 ⇒ 非 flaky。日志 `E:/tmp/window/cargo-test-f72dbce73.log`（sha256 `edf3a742…`）与逐行提取 `cargo-test-f72dbce73.results.txt`（141 行 `test result:`，sha256 `d18e1435…`）。
- **口径**：条件定义与判据见 `docs/leader-goal-20261023.md` §1；本节数字全部可由
  `python -I -B tools/goal/goal_gate.py --full --script-bin <mir2-script-tool>` 复算。

## 1. 四条件状态

- **快照**：`2cb84220f`（2026-10-25 13:47 +08）

| 条件 | 状态 | 证据 |
|---|---|---|
| ① 脚本派发 100% | **PASS** | `parse-stats`：目标 **106/106**、调用 **11829**、未实现 **0**。**已在 `9a966b04b` 上用新构建有效重验**（`mir2-script-tool` = `/e/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/target/release/mir2-script-tool.exe`，mtime 12:46、sha256 `34ac7680b2be1d691a2991948389902f11dceed520f07e6322d58b122fabcb3e`；窗口侧留痕 `/e/tmp/final-window/script-tool.sha256`）；此前用 00:44 的旧二进制得出的同数结论曾作废（其后 3 笔改了 `crates/script`） |
| ② R 卡证据 | **待 owner 裁决** | R 卡 **228**：closed **15** / `window_required` **158**（已合并）/ `window_required` **未合并 3** / whitelist **48** / 已裁定登记 **3** / **`merged_not_closed` 仅 1**（`openmir2-gm-batch-5b-cross-server`，G11 跨服，仓内无实现）/ 其余缺口 0。加总 15+158+3+48+3+1 = 228 ✓（`master 2119f817a` 上 `goal_gate --skip-tests` 实跑）。**口径注记**：未合并 3 张 = `gm-batch-5-guild-castle`/`makeghost-lifecycle`/`window-gate-two-runs`（今日已合三张 R：`dir-persistence-writeback`、`gm-target-state-write`、`gm-script-fixture`）；**closed 15 里有 6 张 `merged=False`**（`ability-weight-login-writeback`/`dualrun-triage-enterworld-frames`/`gm-integration`/`m3hp-lane-repin`/`script-user-account-host`/`window-529-dualrun`）⇒ **无 `merge_result` 佐证** | 3 张 `adjudicated` 均为**审查者签署**的 `no_window`，**无作者代签**。 |
| ③ B 类清零 | **PASS** | recon `tracked=0`（rows 191 条非空、counts 一致） |
| ④ 门禁恒绿 | **pass** | **最终 tip `52be82700`** 上 `goal_gate --full` 实跑：**门禁 7 项 绿 7 / 红 0 / 需人工 0**（146s；fmt 3.2s / clippy 2.9s / msgcodegen 0.3s / whitelist_anchor 5.0s / m3_registry 0.1s / **cargo_test_workspace rc=0 @109.3s** / master_sync PASS）。三段数：**running 3325 = passed 3262 + ignored 63 + failed 0**，**141 suites 全 ok**；对照上一轮 `f72dbce7`（3315/3252/63）**passed +10**（`--lib` 1503→1512 的 +9 加上其它包 +1），**高于判停线 ⇒ 没有吞测试**。`overall=need_human` 仅因条件②，`fail_items=[]`。**溯源**：`.meta` 记 `RUN_ATTEMPT=2` / `CARGO_BUILD_JOBS=2` / `PREBUILD_RC` / `HEAD_START==HEAD_END==52be82700`，附 `cargo-test-52be82700.log`（sha256 `890cb6e2…`）与 141 行 `test result:`（sha256 `495c6036…`）；`PEAK_MEM=n/a`（本轮未采样，如实登记）。**本轮前情**：第一次 `--full` 被宿主内存守卫杀掉、产物 0 字节 ⇒ 不记失败也不记通过；加 `CARGO_BUILD_JOBS=2` + 预构建后重跑通过。**热跑通过不等于冷跑通过。** 该次之后 master 继续前进，但**只用一条命令即可判定不失效**：`git diff --name-only 52be82700 HEAD | grep -E "\.rs$|Cargo\.(toml|lock)$"` **为空**（截至 `e61a68cc6` 只动 `tools/probe`、`tools/window`、`tools/hygiene` 与 `docs/`）⇒ `cargo_test_workspace` 结论继续成立；文档/卫生门已在新 tip 重跑全绿（六门 + `blocked_sources_check selftest` 39/39 + 工具门 registry 45/未分类 0）。 |

**交付的真缺口只剩一处**：② 的 G11 一张需 owner 裁决（列基础设施阻塞排除项 / 保持 need_human / 实施 G11）。

## 2. 今日修掉的「假绿」类别（这是本轮最主要的实质工作）

窗口证据的可信度今天被两轮审计系统性加固，**两类都是「工具报绿但判据根本没比」**：

1. **`recog` 全局掩码吞语义值**：`rules.json` 的 `global` 有一条 `{"fields":["recog"],"action":"mask_fields"}`（多数消息的 Recog 是会话 ActorId）。
   但 650 的 `recog` = 买入后 `Gold`、622 的 `recog` = `WAbil.Weight`、658 的 `recog` = 常量 2000
   ⇒ 不写 `head_compare: ["recog"]` 就**从未比过**，工具照常 PASS。
2. **「帧出现即成功」**：`SM_GAMEGOLDNAME(5008)` 每次登录必发（RM_LOGON）、`SM_MENU_OK(767)` 全仓 28 处发射多为错误弹窗、
   `SM_BUTCH(637)` 在 `Operate.cs:749` 函数尾**无条件**发 ⇒ 这些帧的出现**不携带成功信息**。
   真判据必须落在成功分支或可观察副作用（RNG 抽数 / 落库写 / 条件帧）。

配套的结构性守卫：`machine=true` 且 `ident ∈ {767,5008,637,653,654,657,52}` 的判据**必须带 `fields` 或 `order`**，
否则判据表不合法（`load_criteria` rc=2）——下一个同类错**在写表那一刻**就被拦住。

## 3. 行为修复（今日合入）

| 修复 | 说明 | merge |
|---|---|---|
| Home* 稳定差异 | `homemap::get_start_point()` 1:1 + 登录/3s 两调用点接线；**RedHome\* 复用 `envir::conf` 已解析的 `[Setup]`**（不新增配置） | `edca658d2` |
| 商人物价 ×1.1 | `ItemPriceList` 在**补货时**补 `Round(Price*1.1)`（不判 StdMode）；宿主 `price_tables()` 不再硬编码空表 ⇒ StdMode=31 的 6 件货**从买不成变可买** | `09cd24e14` |
| `SM_WEIGHTCHANGED(622)` | 买入路径补发；帧序 **`200 → 622 → 650`**（`SendAddItem` 直连先出、`WeightChanged` 入队后出）；满包不发 | `b2994843a` |
| 入包单点收敛 | 9 个生产调用点收敛到 `bag::add_item_to_bag`（零行为） | `9a966b04b` |
| 存档派生字段回落载入值 | `CM_SOFTCLOSE` 销毁实体后 `build_save_record` 的解析链返回 None，`cur_x/cur_y/dir/attatck_mode` 静默回落到**载入记录**；修法 = 离世前取实体快照（五项）并让存档走 实体→快照→记录 三档，第三档 WRN | `574a49e7f` |
| GM 命令名取错 conf 死键 | `MasterName`→`ChangeMasterName`、`ShowDenyCharNameLogon`→`ShowDenyChrNameLogon`（oracle 活名取属性默认名，conf 里是拼错的死键）；**负例承重**——reviewer-20 的区分性变异「改回死键 + 加别名」使正例仍绿、只有负例红 ⇒ 钉的是改名不是加别名 | `3d41ba2a1` |
| butch 选靶 | 只在八向相邻候选里取最近（同格排除）+ 有界等待不注入移动帧 | `ecf13b1cf` |
| butch 判据 | 637 降为伴随证据，成功判据 = **RNG 账本恰两抽**（`Operate.cs:722/723`） | `a1576af35` |
| 夹具 `ani_count` | 两处按冻结 `Shape` 修正（旧值恰等于 `AniCount` ⇒ 原缺陷是取错了列） | `062d6beb5` |
| 判据表加固 | 见 §2 | `c0dbd1de2` + `29cfa1979` |
| whitelist 锚 | `lib.rs` 三条行号锚 → 符号锚、T-192 重钉 | `5726e85f6` |

## 4. 证据重算的结论（诚实版）

- **快照**：`2cb84220f`（2026-10-25 13:47 +08）

- **`card_evidence` 重算**（行 = **证据根 × 判据**，且只取 `判据.scenario == 根的 run.json scenario` 的组合；68 个根里**仅 20 个有匹配判据**，其余贡献 0 行）：
  **runner 原样输出 82 行 = PASS 29 / FAIL 1 / UNDECIDABLE 52**（投影式：逐根跑 `card_evidence` 后只数 `detail` 不以「场景不符」开头的行；68 根里仅 20 根有匹配行）。**旧→新 transitions = 0** ⇒ 判据收紧未改变任何既有判据的 verdict。
  （本报告早前写的「81 行」是产物 `rows2.json` 把 `w1c/clearmapmon_q001/csharp` 上 **`W-CM-2` 的两条人工行合并为 1** 所致；两版判据表 **runner 输出都是 82**，表版本不是原因。唯一 FAIL = `e23-reshot/rust` 的 `W-TAX-1`（`650.recog` 9920≠9912），**正是 C 阶段修掉的价格差、属修前证据**。）
  **旧→新 transitions = 0** ⇒ 判据收紧未改变任何既有判据的 verdict。
  **29 个 PASS 的构成**：`657×2`×9、`529`×9、`767(pickwire)`×4、`767(ya52)`×4、`W-TAX-1`×2、`W-PD-1`×1 —— 全部是**登记型**证据，**不代表**这些 PASS 被重新验证过。
  产物：`E:/tmp/window/recompute/rows2.json`（逐行含 `root/scenario/card/w_item/kind/old/new/new_detail`）+ 逐根原始 `*.o.json`/`*.n.json`；旧口径与 reviewer 同代工具（`8ddeca838` 的 27 条表）复算产物在 `E:/tmp/window/recompute/v8/`。
  **68 个「保留根」的选取规则**（可原样复算）：以下 glob 的并集、要求根下有 `frames.ndjson`、排除 `_stale*` 与 `discovery/` ——
  `E:/tmp/window/w1c/*/{csharp,rust}`、`E:/tmp/window/run2/*`、`E:/tmp/window/db-rerun/{run,run2}/*`、`E:/tmp/window/e23-reshot/*`、`E:/tmp/window/harness-proof/{r1,r2}/*`。
- **帧面重算：37 个根逐根重跑，帧面 diff 数（`diffs[].source != "db"`）旧→新变化 0 个**；新帧面非 0 的 **17** 个根、旧报告口径 **13** 个（另有 1 根是旧格式 stub、无帧数据 ⇒ 30 个根可比 + 1 stub）；总差异（含 DB）最大是 `e23-reshot` 的 50 条（帧面 48 + DB 2）。产物：`E:/tmp/rev20-npc/recompare/frame-split.json`（字段 `root/old_frame/new_frame/new_db`）+ 每根完整 report 同目录；派生式 `frame = len([d for d in report["diffs"] if d.get("source") != "db"])`。⇒ 规则改动不改变帧面结论。
- **仍必须重跑真机**：DB 面（`run2`/`w1c`/`harness-proof` 全部早于 save-settle/TZ/见证修复，只有 `db-rerun/run2` 按新口径跑）、
  butch 链、`bag_equip`/`eat_item`（`ani_count`）、home 卡后的 `move_walk`/`login_only`。
- **`ident 14`(SM_HIT) 的 `oracle=27/rust=0` 已定性为伪影**：Rust 并非恒 0（`e23-reshot/rust` 38 条 vs C# 31 条）；
  那个 27/0 出在 `w1c/castle_merchant_tax_potion`，**C# wall 626s vs Rust 8.2s**（修前货架为空买不成、提前收工），
  而商人受击是 `Random(50)==0` ⇒ 8s 内零命中概率 59%。该规则保持 PENDING，9 个根停在 UNDECIDABLE（fail-closed）。
- **`characters_bagitem.MakeIndex`** 属 D-1（oracle 含 `GetTickCount()`，不可对拍）；DB 面掩码与结构牙在 `openmir2-db-makeindex-mask`。
  **证据引法（重要）**：`run2` 六场景的 MakeIndex 两侧**逐 Id 完全相同**（6938 占用、0 差异）⇒ 那批只能证明「掩码不改变 rc」，
  **不能当「掩码必要性」的证据**。必要性要看 **`e23-reshot`** 这一对：旧 rules ⇒ db 差异 **2 条**
  （`characters.Gold 9912/9920` + `characters_bagitem.MakeIndex 645768579/16281`）；新 rules ⇒ **1 条**（Gold）+ 1 条 masked
  （occupied 6939/6939）。

### 4b. 一条会影响真机判据的 harness 风险（2026-10-25 发现，登记）

`card_window.trim_rng_ledger` 超限时**只保留 head**（200,000 行 / 64 MiB），而 butch 的两抽发生在运行**后半段**。
实测 C# 账本 **151,877 行 / 11.9 MB / 21 s ≈ 7.2k 行/s** ⇒ **C# 运行超过约 28 s 就可能把两抽裁掉**，
届时账本看起来「没有两抽」——那是**证据被裁掉，不是行为不符**。

⇒ **凡是靠 RNG 账本判定的场景（butch / dropgold / 抽数类），跑完必须先看 `rng_log_trim.trimmed`**：
为 `true` ⇒ **该次证据作废、需重跑，不得判 FAIL**。清单写在 `docs/butch-ledger-run-checklist-20261025.md` §4。
把裁剪策略改成「保留 head + 关键段」属 harness 改动，**登记未做**。

## 5. 残留物与待办

- **G11**（`openmir2-gm-batch-5b-cross-server`）：条件② 唯一 `merged_not_closed`，需 owner 裁决是否列基础设施阻塞排除项。
- **真机窗口已开始收口**（交付后推进，不影响四条件）：
  - ✅ **`openmir2-butch-two-draws-unblock` 已 `closed`** —— 今天第一张凭真机证据收口的 R 卡（账本恰两抽 + site + 次序 + 值域，三重独立验证）。
  - `openmir2-bag-weight-sink`（L）：主体与 O1/O2 follow-up 已合；**真机 buy 帧序比对已完成，四项判据全 PASS**（622 各 1 条、帧序 `[200,622,650]` 两侧一致、622 四字段 `recog=1/param=0/tag=0/series=36682` 逐字段相同、`650.recog` 9912 两侧）。**但整场景 verdict 仍是 `UNDECIDABLE`（19 条 diffs）**，全部落在本卡之外的三类既有差异：ident 14 的 ignore 规则 `approved_by` 仍 PENDING 且本次剔除数 16 vs 27、ident 10/41 对齐 8+8、ident 53 `body_len` 0 vs 16（`MIR2_CLIENT_ABILITY_LAYOUT` 未显式 pin，D-9）、ident 100 block 1；**这 19 条不含 200/622/650**。本次 `--skip-pin-smoke` ⇒ pin 链未冒烟。**最有力的证据形态**：同一场景在 B/C 修复前（`e23-reshot`）的 50 条 diffs 含 622/645/650，本次这三条已从 diffs 消失 ⇒ 修复由同一比对器见证。`pick_up` 结构性不可达（T-19/T-25）；`P21` 的 GM 前置**不是「未实现」而是「生产权限下不可达」**（见下条）。
  - `openmir2-dir-persistence-writeback`（**R**）：`characters.Dir` 存档写回不随实体更新。**根因**：角色在 402ms 内走了两条退出路径——`CM_SOFTCLOSE` 先 `world.soft_close` 销毁实体，随后 `GateEvent::Closed` 臂才发存档，其自述前置条件（「必须在摘世界前拿实时态」）被上一条破坏。**已合并 `574a49e7f`**（approve `e33fc801`）：单一接缝 `entity_derived_snapshot(&Entity)` 五项全取 + `SavedHuman.derived` + 优先级 实体→derived→rec（第三档 WRN）；牙 T1–T8，其中**T8 钉 derived 侧 `use_items_loaded` 哨兵**（与实体侧哨兵互不替代，删任一只有对应那颗红）。门禁实跑 fmt/clippy/`--lib` **1512 passed**/msgcodegen/六卫生门全绿。**保持 `in-progress` + `window_required`**：唯一未闭项 = 真机复拍（跑法见下窗口队列）。
  - `openmir2-makeghost-lifecycle`（**R**，新开）：oracle `MakeGhost`（`BaseObject.Base.cs:484-495`）**只置 Ghost/Invisible、不移出对象表**，Rust `World::soft_close` 直接销毁实体 —— 上一条的快照层是**绕开**，本卡才是模型层终态（落地后快照层可删）。
  - `openmir2-gm-target-state-write`（**R**）：Rust 侧 `@Level0` 一族 GM 命令曾是 no-op，挡着「靠 GM 改数值再验行为」的场景。**已合并 `a5407e2be`**（approve `84c3b7d7`；门禁实跑 fmt/clippy/`--lib` **1504 passed**/msgcodegen/`t_reservation` 256 行/六卫生门全绿），**保持 `in-progress` + `window_required`**：唯一未闭项是栈级 G-gate 真机证据（跑法见下「剩余窗口队列」）。
    **但 2026-10-25 复核出一条比「未实现」更要紧的事实：这些命令在 oracle 生产路径上本来就不可达。**
    `GetHumPermission` 全仓仅一个调用点且被注释（`PlayObject.cs:1188-1194`）⇒ `Permission` 永不被写，`StartPermission=0`；
    有效命令表 **185 条 = 180×`perm=10` + 5×`perm=0`**（`Command.conf` 的 171 个 `[Permission]` 键里只有 148 个被 loader 读，其余是死键；命令数三方一致 185 = oracle 属性类 = Rust `COMMANDS` = 有效表）⇒ `say "@…"` 只有那 5 条能进命令体，其余一律停在 `权限不够!!!`（Red 255/56）。
    唯一可达路径是脚本 `GMEXECUTE` 抬权（`ExecutionProcessingSys.cs:4938-4947`）。
    ⇒ **任何用聊天 `say` 驱动非零权限 GM 命令的场景/夹具都结构性地跑不出东西**，P21 只是第一个撞上的；
    本卡裁决改为「命令体 + 构造级牙 + 权限门等价真机判据（两侧都记拒绝）」，脚本夹具另卡。裁决 `4e95fd56`。
- **剩余窗口队列（四张 R 卡，都不贵；均需 owner 给时段，我不自行起栈）**：
  | 卡 | 跑什么 | 判据 | 代价 |
  |---|---|---|---|
  | `openmir2-gm-target-state-write` | `card_window.py --profile alt --scenario gm_level0_gate.json --out …` → `gm_gate_check.py check --run-dir … --before/--after --chr-name winplay0` | 逐侧恰 1 条 `权限不够!!!`（Red 255/56）、零 200/202/622/45/52/752、`characters_ablity.Level` 跑前后逐字节相同；**只跑 frames 会判 UNDECIDABLE(rc=2)**，G3 必须带前后 DB 快照 | 零夹具、~2 分钟 |
  | `openmir2-dir-persistence-writeback` | 与今天 `dir-diag` **同一条命令**：`card_window.py --card openmir2-dir-persistence-writeback --profile E:/tmp/window/butch-run/profile.json --scenario tools/probe/scenarios/butch_two_draws.json --rng-log --out …` | 两侧 `characters.Dir` 相等，且 `CX/CY`＝实时坐标（不是载入值）；另核 `characters_item.Dura` 仍归 T-266 磨损 RNG | 复用现成场景与 profile，~3 分钟 |
  | `openmir2-softclose-cleanup-residue`（仅 A1） | **场景已就绪**（`e57024dbc` 合入 `softclose_relogin.json` + probe `relogin` 步：等自身 41 → walk → `CM_SOFTCLOSE` → 重连 → 段2 等新的 41，`assert_same_actor` 默认 true）；**但可观测面只到条件 ①**——条件 ②（名字/行会取自新实体）**单客户端看不到**（oracle 给自身的 `SM_FEATURECHANGED` 五参、无名字尾；带名尾只走观察者 `RM_TURN`+`GetShowName()`） | ① 两段 ActorId 相同 + 展示表清空；② **需观察者客户端**（另开卡） | ① 本窗口可跑；② 未就绪 |
  | `openmir2-gm-script-fixture`（A1 示范） | `card_window.py --profile tools/window/profiles/gm_script_level0.json --scenario gm_script_level0.json --out …` | **每侧 `characters_ablity.Level == 5`（期望值，不是两侧相等）** + 帧面 `45→52→752` 逐字段 + A2 权限门对照同轮复跑 | 需 `files` 夹具注入（已合 `d9a0dd2fe`），~3 分钟 |
  > **用法订正（2026-10-25 复核）**：`--profile` 的取值是**内置 profile 名**（`alt` / `window-magic` / `window-testdb`，`card_window.py:460`）**或一个 `.json` 路径**；`--profile alt.json` 这种写法**不存在该文件**（`tools/window/profiles/` 下只有 `bag_equip` / `butch_monsters` / `castle_tax_upgrade` / `eat_item` 等派生件）。GM 那行用内置 `alt`，Dir 那行用现存的 `E:/tmp/window/butch-run/profile.json`。
  > 为什么列出来：这四项是 ② 里**仅剩的「已有实现、只差真机」**的 R 卡；其余未合并 R 卡要么 blocked（`makeghost`/`gm-script-fixture`）、要么在 dev 手上。
  > softclose 那张的 **A1（展示表）已在 `9224c2ebd` 合并**；**A2 组队归 `makeghost`**（成员关系挂团长实体 `party.rs:428-456` + 退组时点晚 ~402ms）。**关闭前还差 4 项**（`merge_result 19dd38e4`）：两项真机 + 两颗牙（F1 world/host 优先级无牙；**F2 接线点无牙——删掉 `:22704` 调用后 1505 全绿**）。

- **临时物清单（DoD 要求：清掉或登记；下列均**保留**，因为它们是证据）**：
  | 路径 | 是什么 | 为什么留 |
  |---|---|---|
  | `E:/tmp/window/dir-diag/` | Dir 根因诊断轮（日志 + 双栈帧 + 报告） | 证明「存档晚于离世 402ms」的原始日志在 `logs/rust-game-svr.log` L148–L150 |
  | `E:/tmp/window/bws-p622/castle_merchant_tax_potion/` | buy 帧序证据根（14:35） | 四项判据与 19 条 diffs 归属的原始 `report.json` |
  | `E:/tmp/window/goal-full-52be82700.{json,err,meta}` + `cargo-test-52be82700.{log,results.txt}` | 条件④ 全量实跑产物 | `.meta` 记 `RUN_ATTEMPT=2 / CARGO_BUILD_JOBS=2 / HEAD_START==HEAD_END`；两份日志带 sha256 |
  | `E:/tmp/rev20-fixture/MonGen.dirty.bak` | **事故留证**：被夹具变异写脏的生产 MonGen 内容 | 证明「生产确曾被写入」，不是仅凭口述；生产已恢复为 `e5c39fb7…`（与 10-09 hook 副本一致） |
  | `E:/tmp/final-recheck/{orphan-mutations.json,gate-full-45.json,run3-list.txt}` | orphan 门禁卡证据 | `--inventory` 45 项、9/9 谓词变异、`--list` 对齐 |
  | `E:/tmp/dev1-patch/*.log` | Dir 卡门禁日志（clippy/fmt/msgcodegen/workspace） | 门禁 rc 的原始输出 |
  | `E:/tmp/window/reviewer-21-review/buckets.py` | R 卡分桶复算脚本（直调 goal_gate 纯函数、零 cargo） | ② 桶数字可复算；`goal_gate --skip-tests` 仍跑 cargo 的替代法 |
  > 生产侧当前核对（内容级，2026-10-25）：`M2GameSvr/Envir/MonGen.txt` = `e5c39fb7…`、`M2GameSvr/Server.conf` = `ce5e3ce9…`、`Envir/x.txt` 不存在、无 `.bak`/`.fixture-tmp` 残留。

  - `openmir2-gm-script-fixture`（**R**，已合并 `d9a0dd2fe`，保持 `in-progress` + `window_required`）：给 `card_window` 加 `fixtures.files` 通道（往 `Envir/` 注入脚本，逐目标 realpath 生产根守卫 + 写前登记 + 失败回滚 + 两侧对称），加示范 profile/场景走脚本 `GMEXECUTE` 抬权，并复用 GM 卡判据（钉 commit `a5407e2be` + 机判器 sha256）。**期间发生过一次「测试写生产」**：`FX-F14`/`FX-F17` 拿**真实生产根**当靶，变异重放时把 1 行刷怪数据写进了 `Envir/MonGen.txt`、并创建了 `Envir/x.txt`（作者自查与 reviewer 重放各中一次）。**已恢复**（内容级：`MonGen` `e5c39fb7…` 与 10-09 hook 副本逐字节一致、`x.txt` 已删），并把修法做成三重：假根全局切换 + 还原断言 + 假根自合成（不 copyfile 生产）。**教训**：破坏性测试必须以假目标为靶——**测试的安全性不能依赖被测代码是正确的**；且「已还原」必须用**内容 sha** 证明（`E:/MirServer` 不在 git 里，`git status` 看不见）。**关闭前还差**：真机每侧 `Level == 5`（**期望值**不是两侧相等）、帧面 45→52→752、A2 权限门对照；以及 H1（`FX-MON/FX-CONF` 应移到 `if fail_n` 之前，否则前半段一红就不检查生产）。
- **工具面待办（均 L，交付后）**：
  - ✅ `openmir2-tool-selftest-gate`：**已合并 `24248798a` 并 closed**（tier L，approve `637aa96b5` delta + `1ec19eae`）。工具自测门 44 项入 merge 门禁；门自超时硬化（进程树终止 + 10s 兜底 + 420s 总墙钟 + NOT-RUN 点名）；`--inventory` 49 = 门内 44 / 出域 5 / 未分类 0；合并后产物 `--selftest` **14/14 GREEN**。**留三条 carry-over**（`merge_result 9f742dc7`）：M1 计数缺「计数规则」定义（按调用点 `timeout=` 计 35，按进程是否有界计 31，两者都对）、TSG10c 缺牙（守卫位置可静默回归）、M3 超时文案纯观感。
  - `openmir2-orphan-scenario-gate`：`blocked_sources_check` **只扫 run3 行、不扫场景目录** ⇒ 5 个 blocked 孤儿永不被校验而门仍 rc=0（P21 标本是活样本）。
  - **帧对齐改进**（`ea6ae394` 规格）：`compare_runs` 现按位置配对，一侧缺帧会整段错位（实测 50 条 diff 里 44 条是错位噪声）。
  - **harness 前置建议**：`move_walk` 那次 UNDECIDABLE 是「存档在途 ⇒ 服务端拒动作」，与「RNG 账本被裁」同族——**环境状态伪装成行为差异**，建议加 pre-action「存档已落定」前置。
- **工具缺口（登记）**：`goal_gate` 的 `conditions[0].evidence.command` 记了 `parse-stats` 的**完整 argv**（含 exe 绝对路径，
  语料是部署件 `E:\MirServer\M2GameSvr\Envir`），但 **`evidence` 里没有二进制的内容指纹（sha256）** ⇒ 路径上的文件可被后续重建覆盖，
  事后无法自证「当时的结论是哪个二进制跑出来的」。本次由窗口侧自留痕 `/e/tmp/final-window/script-tool.sha256` 补上这一环。
  **建议**：gate 在跑 `parse-stats` **前后各算一次 exe 的 sha256** 写进 `evidence.command_sha256`，前后不一致即判该次结论作废。
