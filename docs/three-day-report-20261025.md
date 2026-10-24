# 三天交付报告（草稿）— 2026-10-25
- 卡：`openmir2-three-day-report-refresh`（tier **L**；owner `openmir2-dev-2`；worktree `.worktrees/dev-2`；分支 `card/three-day-report-refresh`；就地刷新本文件，不新建）。
- 数据时点：**刷新快照 commit = `1c605ca4a`**（2026-10-25 04:27 +08，本地 master）；四条件以
  `goal_gate.py --skip-tests --json` 现算（`generated_ts=1792873815`，**条件④ 的 `cargo_test_workspace` 未跑**）。
  首版快照 `699c26e28`（`generated_ts=1792867854`）仅作对照保留。
- 写集：本文件（只写文档；未改代码/规则/卡片状态；未起栈/连库）。
- 复算口径：所有数字可由 §1/§2/§4 的命令现取；walgit 故障期 master 为本地主源；refs 计数为**活量**，以补推时重数为准。

## 0. 摘要

| 项 | 结果（快照 @ `1c605ca4a`） |
|---|---|
| 今日合并 | **33 笔**（`git log --first-parent --merges --since="2026-10-24 23:40" 1c605ca4a | wc -l` = 33（**终点须钉 `1c605ca4a`**，否则在后续 master 上会得更多）；§1 逐条） |
| 条件① 脚本派发 | **未判**（本轮 `--skip-tests` 且未跑 parse-stats ⇒ `need_human`；首版快照 `699c26e28` 为 **pass**，106/106） |
| 条件② R 卡证据 | **need_human** — R 卡 **213** 张（快照 @ `1c605ca4a`）：已完成 12 / `window_required` 145（另未合并 2）/ whitelist 48 / 已裁定 3 / `merged_not_closed` 1 / 未开工或未合并 2；需人工 1 张（唯一 `merged_not_closed` = `openmir2-gm-batch-5b-cross-server`，G11）。**登记型 PASS 194 张（快照 @ `1c605ca4a`） ≠ 已有真机证据** |
| 条件③ B 类清零 | **pass** — recon tracked=0（快照 @ `1c605ca4a`）（rows 191 条非空、counts 一致） |
| 条件④ 门禁恒绿 | **need_human** — 门禁 7 项：绿 5 / 红 0 / 需人工 2（未判：`cargo_test_workspace`（本轮跳过）、`master_sync`（walgit 500）） |
| 剩余 owner 决策 | G11 裁决 / Q1 窗口 / Q2–Q5 口径 / walgit 恢复（`docs/needs-human-20261025.md` §1–§4） |

## 1. 合并清单（33 笔）

| # | merge oid | 时间 (+08) | 卡 | approve oid | tier | 交付要点 |
|---:|---|---|---|---|---|---|
| 1 | `4ae3dc810` | 2026-10-24 23:51 | `openmir2-goal1-script-coverage-evidence` | `29cbb7a5e8` | L | 条件① 脚本覆盖证据（106/106、未实现 0） |
| 2 | `0183dca15` | 2026-10-25 00:02 | `openmir2-window-run2-prep` | `ec6c8d88df` | L | window-run-2 续跑准备包（run_batch/preflight/证据规范） |
| 3 | `87dc9847b` | 2026-10-25 00:09 | `openmir2-rcard-evidence-classification` | `54d915d765` | L | R 卡三分类 + owner 裁定包（A49/B62/C26=137） |
| 4 | `0be3407a2` | 2026-10-25 00:12 | `openmir2-check-pickup-unreachable-type-aware` | `2dd7913c70` | L | 类型感知检查器 P-1..P-6 全修 |
| 5 | `cb646b65a` | 2026-10-25 00:24 | `card/window-run3-scenarios-blocked（原卡 openmir2-window-run2-prep）` | `c278fe217a` | L | run3 清单 18→17（去封存 pickup）+ roster 双断言 |
| 6 | `a6fc02d0a` | 2026-10-25 00:26 | `openmir2-check-pickup-alias-p7` | `ad99d16642` | L | 检查器 P-7 var 别名同族假绿 |
| 7 | `0bac2ed2d` | 2026-10-25 00:40 | `openmir2-envir-redlist-teeth` | `869a3ab12c` | L | redlist T3 四颗常驻牙（缺行/重复/非法/边界） |
| 8 | `99156c2c6` | 2026-10-25 00:49 | `openmir2-castle-merchant-tax-wiring` | `e523aacee1` | R | 商税四调用点接线（window_required） |
| 9 | `9a7b8c798` | 2026-10-25 01:09 | `openmir2-loader-fields` | `c9089db97f` | R | game-svr 消费 envir::startpoint（8 列承载；window_required） |
| 10 | `bff927e11` | 2026-10-25 01:18 | `openmir2-script-playdice-openybdeal` | `54eeabc3c4` | R | T-142 ⑨GAMEPOINT 全链 + ⑪单实现 + ⑩可观测子集（window_required） |
| 11 | `e0ccf5d5b` | 2026-10-25 01:25 | `openmir2-merged-not-closed-5-sweep` | `f2da46e8d8` | L | 条件② 5 卡裁定（enter-map 改回 window_required） |
| 12 | `f6b3143b0` | 2026-10-25 01:30 | `openmir2-needs-human-refresh-20261025` | `96967e0729` | L | owner 决策页（G11/Q1/Q2–Q5/walgit） |
| 13 | `007ad730a` | 2026-10-25 01:36 | `openmir2-playdice-frame-queue-full` | `40568adb92` | L | ⑩-full 取证（四类动作帧全入队 + 6 面板直连 + says） |
| 14 | `6f46f0df9` | 2026-10-25 01:46 | `openmir2-playdice-queuehead-dedupe-tests` | `b06a52dee3` | L | game_gold_changed 去重改 victims 倒序删 + 两条牙 |
| 15 | `046c06457` | 2026-10-25 01:47 | `openmir2-inventory-refresh-20261025` | `a7b15764d9` | L | 剩余工作盘点刷新（B 表 2/5/15、四桶分类） |
| 16 | `e510e78e1` | 2026-10-25 01:56 | `openmir2-tax-apply-interface-hardening` | `b3e8db7d5c` | L | 商税 apply 接口加固（MerchantTaxDestination/Posting） |
| 17 | `86f8e32fa` | 2026-10-25 01:58 | `openmir2-playdice-emit-decouple` | `03e75fef71` | L | can_jmp 更新与发帧解耦 + 源码序门 |
| 18 | `844efb85a` | 2026-10-25 02:06 | `openmir2-envir-redlist-batch（branch consumption）` | `95ba4409` | L | redlist 8 类消费点复核 + Robot 解析器等价牙 |
| 19 | `c703f6a03` | 2026-10-25 02:11 | `openmir2-inbound-dispatch-red-batch（branch emitpoints）` | `5bc7a49f87` | L | 入站 10 帧出帧发射点取证表 |
| 20 | `d36532a7c` | 2026-10-25 02:16 | `openmir2-sitdown-powerhit-broadcast` | `71af59d5a1` | L | SITDOWN 成功支 + GD/<tick> 短包（M-2 落地） |
| 21 | `4681c685b` | 2026-10-25 02:28 | `openmir2-merge-tree-helper` | `9fcd40ca91` | L | merge_tree 生成脚本（vs_base/vs_master 双参照） |
| 22 | `ff8c0dc9a` | 2026-10-25 02:35 | `openmir2-clearmapmon-hook-audit` | `83d0f30753` | L | CLEARMAPMON no-op 审计（部署 2 处使用 ⇒ 需实现升 R） |
| 23 | `2c94b20ee` | 2026-10-25 02:38 | `openmir2-castle-war-config-slice` | `fa86b35b51` | L | T-132 四名字色接入 + T-131 相位合同 |
| 24 | `1747e7eaf` | 2026-10-25 02:43 | `openmir2-runbook-sync-20261025` | `c6b8aa36db` | L | runbook/batch-plan 口径同步（pickup 17 场景 / CLEARMAPMON / rc 双列 / W1=17） |
| 25 | `699c26e28` | 2026-10-25 02:47 | `openmir2-merge-tree-helper-gate` | `ca5af2b67b` | L | merge_tree_helper 自检纳入 CI fast 门禁 |

| 26 | `18ee8003b` | 2026-10-25 02:52 | `openmir2-three-day-report-draft` | `d822fbcf7a` | L | 本报告首版（25 合并 / 四条件快照 / 缺口总表 / 补推清单） |
| 27 | `f08496192` | 2026-10-25 03:43 | `openmir2-clearmapmon-impl` | `ea4c42d4f8` | R | CLEARMAPMON 清怪 + MONGENEX regen 两半边（window_required） |
| 28 | `263e77d7e` | 2026-10-25 03:56 | `openmir2-m3-castle-siege-state`（T-127 片） | `da06ee4945` | R | 城堡四类帧按 `MsgQueue` 优先级升序上线（165→217→252）+ 跨源合同测试（window_required） |
| 29 | `a3bf5340f` | 2026-10-25 04:12 | `openmir2-inbound-dropgold-rng` | `e26812b48` | R | DROPGOLD 深体零抽数 + ButchEffect 两抽 RNG 合同（window_required） |
| 30 | `74ea5d593` | 2026-10-25 04:17 | `openmir2-runbook-stale-ref-master` | `925a3243` | L | runbook §6 故障期卫生门禁三类口径 |
| 31 | `fe14f0603` | 2026-10-25 04:18 | `openmir2-window-run2-castle-list-caliber` | `257c915c` | L | 窗口 kit 口径：比内容不比 mtime / 零触碰停 C# 栈 / preflight 无 mtime 依赖 |
| 32 | `1379a1833` | 2026-10-25 04:24 | `openmir2-test-writes-real-envir-path` | `1e4ca2af` | L | `Castle/List.txt` 十分钟重写事实定性（非违规）+ 行号三元组 |
| 33 | `1c605ca4a` | 2026-10-25 04:27 | `openmir2-window-kit-coverage` | `ef4aa9b3` | L | 窗口 kit 覆盖核对（6 卡 W 项可执行性 + P0/P1 缺口） |

> 取数（刷新后固定窗口）：`git log --first-parent --merges --since="2026-10-24 23:40" 1c605ca4a --format="%h|%ci|%s" | wc -l` ⇒ **33**（**终点 `1c605ca4a` 必带**：不带终点在后续 master 上会得 35 等新值）；
> 编号按**时间升序实测**（#26 起 8 笔为刷新新增）。其中 R 级 **6 笔**（`99156c2c6` 商税 / `9a7b8c798` loader-fields / `bff927e11` T-142 / `f08496192` CLEARMAPMON / `263e77d7e` T-127 / `a3bf5340f` inbound RNG）合并后保持 `in-progress` 等真机窗口。

## 2. 四条件机判快照（刷新：`1c605ca4a`）

### 2.1 快照（`goal_gate.py --skip-tests --json` @ `1c605ca4a` @ 2026-10-25 04:30 +08，`generated_ts=1792873815`）

| # | 条件 | state | detail（原文摘要） |
|---:|---|---|---|
| 1 | 脚本派发 100% | **need_human（本轮未判）** | `--skip-tests` 且未开 parse-stats ⇒ 未运行；首版快照 `699c26e28` 为 **pass**（目标 106/106、调用 11829、未实现 0），刷新不改该事实但**不以旧值冒充实测** |
| 2 | R 卡证据 | **need_human** | R 卡 **213** 张（快照 @ `1c605ca4a`）：已完成 12 / `window_required` 145（另未合并 2）/ whitelist 48 / 已裁定登记 3 / comment 证据 0 / 三者皆无 0 / `merged_not_closed` 且无登记 1 / 未开工或未合并 2 / 其它需人工 0；需人工 1 张（最旧优先）；`merged_open_hygiene` 194 张（快照 @ `1c605ca4a`）（**登记型 PASS ≠ 真机证据**） |
| 3 | B 类清零 | **pass** | recon tracked=0（快照 @ `1c605ca4a`）（rows 191 条非空、counts 一致） |
| 4 | 门禁恒绿 | **need_human** | 门禁 7 项：绿 5 / 红 0 / 需人工 2（**未判**：`cargo_test_workspace`〔本轮跳过〕、`master_sync`〔walgit 500〕） |

> 口径：本轮为**轻量快照**（跳过全量测试以避开内存压力）。条件① 与 ④ 的 `cargo_test_workspace` 需在窗口/低负载时重跑；
> **不把「登记型 PASS」写成已有真机证据**（194 张 `merged_open_hygiene` 即此类）。

### 2.2 条件② 计数（`evidence.r_cards`，快照 @ `1c605ca4a`）

| 桶 | 张数 |
|---|---:|
| `closed` | 12 |
| `window_required` | 145 |
| `window_required_unmerged` | 2 |
| `whitelist_t` | 48 |
| `adjudicated` | 3 |
| `merged_not_closed` | 1 |
| 未开工或未合并 | 2 |
| 合计 | 213 |

> 唯一 `merged_not_closed`：`openmir2-gm-batch-5b-cross-server`（G11）。
> R 桶是**活量**（随卡片移动），复算请重跑 §2.4 的命令，不要引用本表数字做新结论。

### 2.3 证据索引（每条件）

| 条件 | 证据文件 / 产物 |
|---|---|
| ① 脚本派发 | `docs/script-coverage-20261025.md`（合并 `4ae3dc810`）；`mir2-script-tool parse-stats`；`goal_gate` 的 `parse-stats` 行 |
| ② R 卡证据 | `docs/rcard-evidence-plan-20261025.md`（A49/B62/C26）；`docs/merged-not-closed-sweep-20261025.md`；`docs/needs-human-20261025.md` §1/§5；`goal_gate` 的 `evidence.r_cards` |
| ③ B 类清零 | `mir2-rs/tests/parity/coverage/recon_gates.py dispatch`（tracked=0（快照 @ `1c605ca4a`））；`docs/goal-gate-full-20261024.md` |
| ④ 门禁恒绿 | `.walgit/ci.toml`（7 项，含 `merge_tree_helper --selftest` fast task）；`docs/ci-first-master-run-20261024.md` |

### 2.4 复算命令

```sh
cd <本检出>
python -I -B tools/goal/goal_gate.py --skip-tests --json > "$TEMP/gate.json"   # 轻量（不跑 cargo test）
python -I -c "import json,os;d=json.load(open(os.path.join(os.environ['TEMP'],'gate.json')));print(d['overall']);[print(c['id'],c['state'],c['detail']) for c in d['conditions']]"
# 完整版（含 cargo test）在低负载/窗口时段跑：去掉 --skip-tests
```

### 2.5 窗口真正的前置（**owner 开窗 ≠ 窗口能跑完**）

> 依据：`docs/window-kit-coverage-20261025.md`（合并 `1c605ca4a`）对 6 张待窗口卡的 W 项逐条对 kit 资产。

| 级别 | 缺口 | 影响的 W 项 | 现状 |
|---|---|---|---|
| **P0** | `assets` 白名单**仅 `Gold`**（`card_window.py:2980`） | W-PD-1（GAMEPOINT 前置无法声明） | L 卡扩白名单或窗口手工 DB 预置 |
| **P0** | 私栈启动**未接 `-RngLog`**（`E:/tmp/conformance/start-rust.ps1`、`tools/m2-acceptance/start_mir2_server.ps1` 无该参数） | W-DG-1/2、W-CM-3（RNG 流水账本） | 工具面接线（不改 `E:/MirServer`） |
| **P0** | **C# 无 `MIR2_CLOCK_*`**（`grep -rIl MIR2_CLOCK src/` = 0） | W-TAX-4、W-T127-2（时钟/`SaveTick` 逐位对拍） | ⑧-1 C# 半未落地 ⇒ 只能真实时间点复拍并登记降级 |
| P1 | 无城堡商人税场景 / 无 castle fixture | W-TAX-1..3、W-T127-1..4 | 新场景提案 `castle_merchant_tax` / `castle_war_frames` |
| P1 | 无 `CM_NEWCHR` 驱动（协议常量已有 `CM_NEWCHR=101`） | W-LF-1（新角色出生点） | probe 加 `newchr` 步骤或窗口手工建号 |
| P1 | 无 GAMEPOINT 场景 | W-PD-1 | 新场景 `gamepoint_exchange` |
| P1 | `@back` 字面不存在（部署只有 `@back2`） | W-PD-2 | 澄清后转构造级/**非目标** |
| P1 | 无**多会话/观察者**驱动（`card_window` 每侧仅一条 probe；`run_pair.ps1` 可两假人但不驱动帧序判据） | 需多观察者的 W 项（如广播/名牌帧收件人） | probe 多会话扩展或 `run_pair` 加判据（与 `docs-minors-sweep` `90bf475a1` 同批落 master 后本表一致） |
| P2 | 无 CLEARMAPMON / drop-butche 场景 | W-CM-1/2、W-DG-1/2 | 触发点已定位，待新场景 |

⇒ **开窗前的动作**归属卡 `openmir2-window-kit-p0-unblock`（branch `card/window-kit-p0-unblock`，head `30c6de6f1`，**未合并**）；
P0 三条未清前，即使 owner 给出独占窗口，W-PD-1 / W-DG / W-TAX-4 / W-T127-2 仍**跑不出判据**。

## 3. 剩余缺口总表

| # | 类别 | 项 | 状态 / 证据 | 需要谁 |
|---:|---|---|---|---|
| 1 | 条件② | `openmir2-gm-batch-5b-cross-server`（G11 跨服基础设施未实现） | `blocked`；goal_gate 唯一 `merged_not_closed`；依赖清单已合 `99a4d30c4` | **owner**：`docs/needs-human-20261025.md` §1（A 排除/B 保持/C 实施） |
| 2 | 窗口 | Q1 窗口授权（17 场景 + 6 夹具；run-2 续跑） | `window_required` 145（另未合并 2）；194 张「登记型 PASS ≠ 真机证据」 | **owner**：needs-human §2（A 全量/B 最小集/C 暂缓） |
| 3 | 口径 | Q2–Q5：无场景 62 张分档 / combat 夹具 / RNG flaky / 登记型豁免 | `docs/rcard-evidence-plan-20261025.md` §6 | **owner**：needs-human §3（推荐 D/A/丙/A） |
| 4 | 基础设施 | walgit 主源故障（R2 WAL 段缺失，fetch/push 500） | `git ls-remote walgit` rc=128；本地 master 领先最后已知远端：**截至 `02f83a1a2` 为 91**（快照 @ `02f83a1a2`；首版 `699c26e28` 时点 60）。**该值是活量**，复算 `git rev-list --count refs/remotes/walgit/master..master`（最后已知远端 `4ae3dc810` 不变） | **owner**：needs-human §4（等恢复 / GitHub 临时主源） |
| 5 | 规则表 | `rules.json` PENDING 50（global 4 + rules 45 + db 1）；`T-161..T-222` 复核 58 行 | needs-human `docs/needs-human-20261024.md` §2/§4（现行口径） | **owner** |
| 6 | 已合并待窗口 R | `openmir2-clearmapmon-impl`（CLEARMAPMON world 清怪 + MONGENEX regen，共用动作序队列） | 已合并 `f08496192`（tier R、window_required；快照 @ f08496192 @ 2026-10-25 04:10）；审计 `docs/clearmapmon-hook-audit-20261025.md` 证明部署 2 处使用 | 真机 T118/Q001 复拍 |
| 7 | 待开工 R | T-142 ⑩① handler 级 `MsgQueue`（`out` 6 直连点 + says + script_frames 统一出队） | `docs/playdice-frame-queue-full-20261025.md` §3.2/§4；部署当前不可观测 | 排队（建议 R 卡） |
| 8 | 受阻 | `openmir2-npc-click-rng-stream-flaky`（跨栈 RNG 流位 flaky 口径） | 板【受阻】；等 owner 窗口口径（needs-human §3 Q4） | **owner** |
| 9 | 受阻 | `openmir2-window-gate-two-runs`（窗口门两次跑） | 板【受阻】；未合并（window_required 未合并桶） | 合并/裁决 |
| 10 | 窗口 | 多会话探针 / `castle_zone` / combat 夹具等窗口依赖 | `docs/rcard-evidence-plan-20261025.md` §3/§4；15 张 blocked/不可达 + 46 张需新场景 | 窗口/开发排期 |
| 11 | 返工中 | `openmir2-script-handler-msgqueue`（R）**7 Important 返工** | branch `card/script-handler-msgqueue` head `911e17d8e`；review 见该卡线程 | 作者返工 → 重审 |
| 12 | 待审 | `openmir2-castle-official-design`（R）返工 2（F1 宿主 sticky 用例 + Click 读 sticky + changedate 补零） | head `edf2d5107`；reviewer-20 在 `review/castleofficial3` 复审 | 合并后立即开 `openmir2-castle-official-actions` |
| 13 | 非目标 | `@back` 字面：部署件 grep `@back>` = 0 命中（只有 `@back2`） | `docs/window-kit-coverage-20261025.md` §1.3；W-PD-2 转构造级/非目标 | 无（口径已澄清） |
| 14 | 非违规 | `E:/MirServer/M2GameSvr/Castle/List.txt` **每 600s 重写** | 运行中的 C# `GameSrv`（PID 14040）的 10 分钟 `CastleMgr.Save`；证据 `docs/test-writes-real-envir-path-20261025.md`（合并 `1379a1833`） | 巡检只比内容不比 mtime；需零触碰时停 C# 栈 |
| 15 | 待合并 | 窗口 P0 三条（assets 白名单 / `-RngLog` / C# 时钟） | `openmir2-window-kit-p0-unblock`，branch `card/window-kit-p0-unblock` head `30c6de6f1`；§2.5 | 合并 + 窗口排期 |

> 快照 @ `1c605ca4a` @ 2026-10-25 04:32 +08（`walgit collab board --format json`）
> 板面：待认领 0（快照 @ `1c605ca4a`） / 进行中 253（快照 @ `1c605ca4a`） / 待审 4（快照 @ `1c605ca4a`） / 需要人 0 / 受阻 3 / 已完成 345（快照 @ `1c605ca4a`） / 已合并未收口 0（快照 @ `1c605ca4a`）

## 4. walgit 恢复后的补推清单（刷新：`1c605ca4a`）

- 最后已知 walgit master：`4ae3dc810`（2026-10-24 23:51）；本地 master：`1c605ca4a`；
  `git rev-list --count refs/remotes/walgit/master..master` ⇒ **91**（截至 `02f83a1a2`；活量，补推前重数）。
  镜像侧同理：`git rev-list --count origin/master..master` ⇒ **21**（截至同一时点，活量）。
- 本地 `refs/heads/card` 共 **7** 条（`git for-each-ref refs/heads/card`）：

| 分支 | head | 状态 |
|---|---|---|
| `card/castle-official-door` | `edf2d5107` | **未合并**（design 卡返工 2 待审） |
| `card/script-handler-msgqueue` | `911e17d8e` | **未合并**（R 卡 7 Important 返工中） |
| `card/three-day-report-refresh` | 本卡交付 head | **未合并**（本卡待审；合并后补推） |
| `card/window-closure-ledger` | `b62c5c429` | **未合并**（台账，随窗口线） |
| `card/window-kit-coverage` | `18900a260` | 已合并 `1c605ca4a`（推送为审计/身份留存） |
| `card/window-kit-p0-unblock` | `30c6de6f1` | **未合并**（P0 三条，窗口前置） |
| `card/window-run2-castle-list-caliber` | `fa24260b0` | 已合并 `fe14f0603`（推送为审计） |

```sh
# 1) 主源与全部本地 card 分支（walgit 恢复后）
git push walgit master
git push walgit card/castle-official-door card/script-handler-msgqueue card/three-day-report-refresh \
               card/window-closure-ledger card/window-kit-coverage card/window-kit-p0-unblock \
               card/window-run2-castle-list-caliber

# 2) 协作条目（活量：补推前重数）
git push walgit 'refs/collab/inbox/*:refs/collab/inbox/*'
git push walgit 'refs/collab/meta/*:refs/collab/meta/*'

# 3) GitHub 镜像（人工同步或临时主源）
git push origin master

# 4) 恢复核对
git ls-remote walgit refs/heads/master   # 应等于本地 1c605ca4a（或届时 master）
git for-each-ref refs/collab/inbox | wc -l   # 推送后与本地一致
```

> 说明：`refs/collab/inbox` 与 `refs/collab/meta` 是**活量**，补推前请重数；上表 head 以刷新时点为准，
> 补推前用 `git for-each-ref` 重取。

## 5. 门禁与复算

| 检查 | 结果 |
|---|---|
| `cargo test --workspace`（goal_gate --full 同次） | 见 §2.1 条件④ detail |
| fmt / clippy / msgcodegen / 7 项卫生门禁 | 全 rc=0（本报告为纯文档，写完后逐项复跑） |
| `git diff --check <base> <head>` | **必须带范围**（无参数只查未提交改动，会伪绿）。本卡：`git diff --check 1c605ca4a <head>` ⇒ rc=0（见 patch body） |
| `git merge-tree`（vs master） | 见 patch body（`tools/hygiene/merge_tree_helper.py`） |

## 6. 残余 / 判不准

- 本报告是**快照**：本轮用 `goal_gate --skip-tests`（条件①/④ 的 cargo test 未跑）；`master_sync` 在 walgit 故障期必然 need_human，恢复后应重跑完整四条件再定稿。
- 合并清单的 approve oid 以合并提交题注/`merge_result` 为准；双 approve 的卡只列主 approve（题注所载）。
- 「进行中 253」含大量 R 卡等窗口（非全部在飞），未逐卡复核；以 `walgit collab board` 为准。
- 无临时物；本文件是唯一仓库改动。
- **刷新增量**：合并 25 → **33** 笔；四条件快照换到 `1c605ca4a`（轻量口径，条件①/④ 的 cargo test 未判）；
  新增 §2.5「窗口真正的前置」（P0 三条 + P1 缺口，指向 `openmir2-window-kit-p0-unblock`）。
- **在飞（不算缺口）**：`openmir2-script-handler-msgqueue`（R，7 Important 返工中）、
  `openmir2-castle-official-design`（R，返工 2 在 reviewer-20 手里，head `edf2d5107`）。
- **口径澄清**：`@back` 字面在部署不存在（只有 `@back2`）⇒ W-PD-2 转构造级/非目标；
  `Castle/List.txt` 每 600s 重写是运行中 C# `GameSrv` 的周期 Save（**非违规**，只比内容不比 mtime）。
