# OpenMir2 交付报告（2026-10-25 终版）

- **快照**：`a1576af35`（2026-10-25 13:10 +08）。**四条件中 ①③④ 已 PASS，仅 ② 待 owner 对 G11 的一句话。**
- **口径**：条件定义与判据见 `docs/leader-goal-20261023.md` §1；本节数字全部可由
  `python -I -B tools/goal/goal_gate.py --full --script-bin <mir2-script-tool>` 复算。

## 1. 四条件状态

- **快照**：`a1576af35`（2026-10-25 13:10 +08）

| 条件 | 状态 | 证据 |
|---|---|---|
| ① 脚本派发 100% | **PASS** | `parse-stats`：目标 **106/106**、调用 **11829**、未实现 **0**。**已在 `9a966b04b` 上用新构建有效重验**（`mir2-script-tool` = `/e/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/target/release/mir2-script-tool.exe`，mtime 12:46、sha256 `34ac7680b2be1d691a2991948389902f11dceed520f07e6322d58b122fabcb3e`；窗口侧留痕 `/e/tmp/final-window/script-tool.sha256`）；此前用 00:44 的旧二进制得出的同数结论曾作废（其后 3 笔改了 `crates/script`） |
| ② R 卡证据 | **待 owner 裁决** | R 卡 **222**：closed 14 / `window_required` **154**（另未合并 2）/ whitelist 48 / 已裁定登记 **3** / **`merged_not_closed` 仅 1**（`openmir2-gm-batch-5b-cross-server`，G11 跨服，仓内无实现）/ 其余缺口 0。加总 14+154+2+48+3+1 = 222 ✓（**逐卡重数 `conditions[1].evidence.r_cards` 得到，不是读 `detail` 字符串**） | 3 张 `adjudicated` 均为**审查者签署**的 `no_window`（`m3-fixture-marker-prefix-bug` ← reviewer-6；`m3-pet-slave-summon` / `m3-rng-trace-scene-groups` ← reviewer-5），**无作者代签**。 |
| ③ B 类清零 | **PASS** | recon `tracked=0`（rows 191 条非空、counts 一致） |
| ④ 门禁恒绿 | **PASS** | `goal_gate --full` 的 `conditions[3]` = **门禁 7 项：绿 7 / 红 0 / 需人工 0**（fmt / clippy / msgcodegen / whitelist_anchor_gate / m3 audit / `cargo_test_workspace` rc=0 / `master_sync`）。手工全量 `CARGO_BUILD_JOBS=2 cargo test --workspace --locked`：**running 3314 = passed 3251 + failed 0 + ignored 63，rc=0**，逐包自洽 `mismatches=0`；相对基线（141/3217/62）**+34 passed / +1 ignored / suite 数不变**。**两次 cargo test（手工与 `--full` 内部）rc 均为 0 ⇒ 一致 ⇒ 非 flaky**。`master_sync`：`local_master == remote_master == a1576af3577d`。窗口完整性：`PRE_HEAD == WANT == POST_HEAD = a1576af3577d`、pre/post `git status` 均空 ⇒ 窗口内无漂移；`stale .d` 检索为空（未 clean）。**计数差值可独立解释**（无需 `--list`）：`git diff 139d7b5e4 a1576af35 -- mir2-rs/` ⇒ 新增 `#[test]` 类属性 **35**、删除 **0**，其中 **1 条带 `#[ignore]`** ⇒ 预期 **+34 passed / +1 ignored**，与实测逐项吻合、suite 数不变。**跟踪文件普查**：`git ls-files` **7602** == `git ls-tree -r HEAD` **7602**（脚本里的 `TRACKED_FILE_OK` 只是单哨兵文件存在性检查、未落产物，**不得**据此写「全仓跟踪完整」）。**保管链**：证据包内 `cargo-test.log` 与运行日志逐字节相同、`goal-full.json` 两份 sha256 相同。 |

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
| butch 选靶 | 只在八向相邻候选里取最近（同格排除）+ 有界等待不注入移动帧 | `ecf13b1cf` |
| butch 判据 | 637 降为伴随证据，成功判据 = **RNG 账本恰两抽**（`Operate.cs:722/723`） | `a1576af35` |
| 夹具 `ani_count` | 两处按冻结 `Shape` 修正（旧值恰等于 `AniCount` ⇒ 原缺陷是取错了列） | `062d6beb5` |
| 判据表加固 | 见 §2 | `c0dbd1de2` + `29cfa1979` |
| whitelist 锚 | `lib.rs` 三条行号锚 → 符号锚、T-192 重钉 | `5726e85f6` |

## 4. 证据重算的结论（诚实版）

- **快照**：`a1576af35`（2026-10-25 13:10 +08）

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
- **真机窗口项**（不在代码交付范围内，按 `docs/window-closure-ledger-20261025.md` 执行）。
- **`openmir2-bag-weight-sink`**（L）：把 622 发射规则收进单点，排在交付窗口之后。
- **帧对齐改进**（`ea6ae394` 规格）：`compare_runs` 现按位置配对，一侧缺帧会整段错位（实测 50 条 diff 里 44 条是错位噪声）；实现在交付后评估。

- **工具缺口（登记）**：`goal_gate` 的 `conditions[0].evidence.command` 记了 `parse-stats` 的**完整 argv**（含 exe 绝对路径，
  语料是部署件 `E:\MirServer\M2GameSvr\Envir`），但 **`evidence` 里没有二进制的内容指纹（sha256）** ⇒ 路径上的文件可被后续重建覆盖，
  事后无法自证「当时的结论是哪个二进制跑出来的」。本次由窗口侧自留痕 `/e/tmp/final-window/script-tool.sha256` 补上这一环。
  **建议**：gate 在跑 `parse-stats` **前后各算一次 exe 的 sha256** 写进 `evidence.command_sha256`，前后不一致即判该次结论作废。
