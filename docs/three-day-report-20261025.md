# 三天交付报告（草稿）— 2026-10-25
- 卡：`openmir2-three-day-report-draft`（tier **L**；owner `openmir2-dev-1`；worktree `.worktrees/dev-1`；分支 `card/three-day-report-draft`）。
- 数据时点：`goal_gate --full --json` 实测时 master = `699c26e28` @ 02:50:54 +08（`generated_ts=1792867854`）；生成时本地 master = `699c26e28`。
- 写集：本文件（只写文档；未改代码/规则/卡片状态；未起栈/连库）。
- 复算口径：所有数字可由 §1/§2/§4 的命令现取；walgit 故障期 master 为本地主源；refs 计数为**活量**，以补推时重数为准。

## 0. 摘要

| 项 | 结果 |
|---|---|
| 今日合并 | **25 笔**（24 笔 2026-10-25 + 1 笔 2026-10-24 23:51；§1 逐条可复算） |
| 条件① 脚本派发 | **pass** — parse-stats 实测：目标 106/106（去重派发目标） |
| 条件② R 卡证据 | **need_human** — 唯一 need_human 是 `openmir2-gm-batch-5b-cross-server`（blocked-by G11） |
| 条件③ B 类清零 | **pass** — B 类清零：recon tracked=0（快照 @ `699c26e28`）（rows 191 条非空、counts 一致）（recon 实测 tracked=0（快照 @ `699c26e28`）） |
| 条件④ 门禁恒绿 | **need_human** — 7 项中仅 `master_sync` 因 walgit 500 待恢复；`cargo test --workspace` 通过 |
| 剩余 owner 决策 | G11 裁决 / Q1 窗口 / Q2–Q5 口径 / walgit 恢复（`docs/needs-human-20261025.md` §1–§4） |

## 1. 合并清单（25 笔）

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

> 取数（固定窗口）：`git log --first-parent --merges --since="2026-10-24 23:40" --until="2026-10-25 02:51" --format="%h|%ci|%s" | wc -l` ⇒ **25**。
> 其中 R 级 3 张（`99156c2c6` 商税 / `9a7b8c798` loader-fields / `bff927e11` T-142），合并后保持 `in-progress` 等真机窗口。

## 2. 四条件机判快照 + 证据索引

### 2.1 快照（`goal_gate --full --json` @ `699c26e28` @ 2026-10-25 02:50:54）

| # | 条件 | state | detail（原文） |
|---:|---|---|---|
| 1 | 脚本派发 100% | **pass** | parse-stats 实测：目标 106/106（去重派发目标）；调用 11829 次；未实现 0 个 |
| 2 | R 卡证据 | **need_human** | R 卡 207 张（快照 @ `699c26e28`）：已完成 12（快照 @ `699c26e28`） / window_required 141（另未合并 2） / whitelist 48 / 已裁定登记 3 / comment 证据 0 / 三者皆无 0 / merged 未 closed 且无登记(需人工) 1 / 未开工或未合并 0（不计缺口）/ 其它需人工 0；需人工 1 张（按 last_status_ts 最旧优先，见 JSON need_items）；merged 未 closed 但已凭登记 PASS：190 张（卫生清单，见 evidence.merged_open_hygiene；登记型 PASS ≠ 已有真机证据） |
| 3 | B 类清零 | **pass** | B 类清零：recon tracked=0（快照 @ `699c26e28`）（rows 191 条非空、counts 一致）（recon 实测 tracked=0（快照 @ `699c26e28`）） |
| 4 | 门禁恒绿 | **need_human** | 门禁 7 项：绿 6 / 红 0 / 需人工 1；未判：master_sync |

### 2.2 条件② 计数（evidence.r_cards 现算）

| 桶 | 张数 |
|---|---:|
| `closed` | 12 |
| `window_required` | 141 |
| `window_required_unmerged` | 2 |
| `whitelist_t` | 48 |
| `adjudicated` | 3 |
| `merged_not_closed` | 1 |
| 合计 | 207 |

> 唯一 `merged_not_closed`：`openmir2-gm-batch-5b-cross-server`。

### 2.3 证据索引（每条件）

| 条件 | 证据文件 / 产物 |
|---|---|
| ① 脚本派发 | `docs/script-coverage-20261025.md`（合并 `4ae3dc810`）；`mir2-script-tool parse-stats` 输出（报告内登记）；`goal_gate --full` 的 `parse-stats` 行 |
| ② R 卡证据 | `docs/rcard-evidence-plan-20261025.md`（A49/B62/C26）；`docs/merged-not-closed-sweep-20261025.md`（5 卡裁定）；`docs/needs-human-20261025.md` §1/§5；`goal_gate` 的 `evidence.r_cards` / `merged_open_hygiene` |
| ③ B 类清零 | `mir2-rs/tests/parity/coverage/recon_gates.py dispatch`（tracked=0，快照 @ `699c26e28`）；`docs/goal-gate-full-20261024.md`；`docs/conformance-*` |
| ④ 门禁恒绿 | `.walgit/ci.toml`（7 项；含新 `merge_tree_helper --selftest` fast task）；`docs/ci-first-master-run-20261024.md`；`docs/goal-gate-full-20261024.md` §4；`goal_gate` 的 `conditions[4]`（除 `master_sync`） |

### 2.4 复算命令

```sh
cd <本检出>
python -I -B tools/goal/goal_gate.py --full --json > "$TEMP/gate.json"   # 约 3–6 分钟（含 cargo test）
python -I -c "import json,os;d=json.load(open(os.path.join(os.environ['TEMP'],'gate.json')));print(d['overall']);[print(c['id'],c['state'],c['detail']) for c in d['conditions']]"
```

## 3. 剩余缺口总表

| # | 类别 | 项 | 状态 / 证据 | 需要谁 |
|---:|---|---|---|---|
| 1 | 条件② | `openmir2-gm-batch-5b-cross-server`（G11 跨服基础设施未实现） | `blocked`；goal_gate 唯一 `merged_not_closed`；依赖清单已合 `99a4d30c4` | **owner**：`docs/needs-human-20261025.md` §1（A 排除/B 保持/C 实施） |
| 2 | 窗口 | Q1 窗口授权（17 场景 + 6 夹具；run-2 续跑） | `window_required` 141 + 未合并 2；190 张「登记型 PASS ≠ 真机证据」 | **owner**：needs-human §2（A 全量/B 最小集/C 暂缓） |
| 3 | 口径 | Q2–Q5：无场景 62 张分档 / combat 夹具 / RNG flaky / 登记型豁免 | `docs/rcard-evidence-plan-20261025.md` §6 | **owner**：needs-human §3（推荐 D/A/丙/A） |
| 4 | 基础设施 | walgit 主源故障（R2 WAL 段缺失，fetch/push 500） | `git ls-remote walgit` rc=128；本地 master 领先最后已知远端 **60** 个提交 | **owner**：needs-human §4（等恢复 / GitHub 临时主源） |
| 5 | 规则表 | `rules.json` PENDING 50（global 4 + rules 45 + db 1）；`T-161..T-222` 复核 58 行 | needs-human `docs/needs-human-20261024.md` §2/§4（现行口径） | **owner** |
| 6 | 待开工 R | `openmir2-clearmapmon-impl`（CLEARMAPMON world 清怪 + host 队列桥） | 板【待认领】；今日审计 `docs/clearmapmon-hook-audit-20261025.md` 证明部署 2 处使用 ⇒ 需实现且升 R | 排队（可派 R 车道） |
| 7 | 待开工 R | T-142 ⑩① handler 级 `MsgQueue`（`out` 6 直连点 + says + script_frames 统一出队） | `docs/playdice-frame-queue-full-20261025.md` §3.2/§4；部署当前不可观测 | 排队（建议 R 卡） |
| 8 | 受阻 | `openmir2-npc-click-rng-stream-flaky`（跨栈 RNG 流位 flaky 口径） | 板【受阻】；等 owner 窗口口径（needs-human §3 Q4） | **owner** |
| 9 | 受阻 | `openmir2-window-gate-two-runs`（窗口门两次跑） | 板【受阻】；未合并（window_required 未合并桶） | 合并/裁决 |
| 10 | 窗口 | 多会话探针 / `castle_zone` / combat 夹具等窗口依赖 | `docs/rcard-evidence-plan-20261025.md` §3/§4；15 张 blocked/不可达 + 46 张需新场景 | 窗口/开发排期 |

> 板面快照（`walgit collab board --format json`；快照 @ `699c26e28` @ 2026-10-25 02:49）：
> 待认领 1（快照 @ `699c26e28`）/ 进行中 249（快照 @ `699c26e28`）/ 待审 0（快照 @ `699c26e28`）/ 需要人 0 / 受阻 3 / 已完成 342（快照 @ `699c26e28`）/ 已合并未收口 0。

## 4. walgit 恢复后的补推清单（可执行）

- 最后已知 walgit master：`4ae3dc810`（2026-10-24 23:51）；本地 master：`699c26e28`；`git rev-list --count refs/remotes/walgit/master..master` ⇒ **60**。
- 本地 `refs/heads/card` 共 **10** 条（`git for-each-ref refs/heads/card`）：

| 分支 | head | 是否已在 master |
|---|---|---|
| `card/castle-war-config-slice` | `30d626104` | 已合并（推送为审计/身份留存） |
| `card/check-pickup-alias-p7` | `ffeec2b42` | 已合并（推送为审计/身份留存） |
| `card/clearmapmon-hook-audit` | `1f790f8ff` | 已合并（推送为审计/身份留存） |
| `card/envir-redlist-consumption` | `0a7073888` | 已合并（推送为审计/身份留存） |
| `card/inbound-dispatch-emitpoints` | `8e444a20c` | 已合并（推送为审计/身份留存） |
| `card/merge-tree-helper` | `72af08920` | 已合并（推送为审计/身份留存） |
| `card/merge-tree-helper-gate` | `8833d3483` | 已合并（推送为审计/身份留存） |
| `card/runbook-sync-20261025` | `3477600bb` | 已合并（推送为审计/身份留存） |
| `card/sitdown-powerhit-broadcast` | `683e0e36e` | 已合并（推送为审计/身份留存） |
| `card/three-day-report-draft` | `待交付` | **未合并**（本卡待审；合并后补推） |

```sh
# 1) 主源与全部本地 card 分支（walgit 恢复后）
git push walgit master
git push walgit card/castle-war-config-slice card/check-pickup-alias-p7 card/clearmapmon-hook-audit \
               card/envir-redlist-consumption card/inbound-dispatch-emitpoints card/merge-tree-helper \
               card/merge-tree-helper-gate card/runbook-sync-20261025 card/sitdown-powerhit-broadcast \
               card/three-day-report-draft

# 2) 协作条目（活量：补推前重数；本报告取数时 inbox 9190 / meta 99）
git push walgit 'refs/collab/inbox/*:refs/collab/inbox/*'
git push walgit 'refs/collab/meta/*:refs/collab/meta/*'

# 3) GitHub 镜像（人工同步或临时主源）
git push origin master

# 4) 恢复核对
git ls-remote walgit refs/heads/master   # 应等于本地 699c26e28
git for-each-ref refs/collab/inbox | wc -l   # 推送后与本地一致
```

> 说明：`refs/collab/inbox` 与 `refs/collab/meta` 是**活量**（agent 持续写入），上表数值只代表本报告取数时点，补推前请重数；`card/three-day-report-draft` 待本卡合并后由 Leader 补推。

## 5. 门禁与复算

| 检查 | 结果 |
|---|---|
| `cargo test --workspace`（goal_gate --full 同次） | 见 §2.1 条件④ detail |
| fmt / clippy / msgcodegen / 7 项卫生门禁 | 全 rc=0（本报告为纯文档，写完后逐项复跑） |
| `git diff --check` | rc=0 |
| `git merge-tree`（vs master） | 见 patch body（`tools/hygiene/merge_tree_helper.py`） |

## 6. 残余 / 判不准

- 本报告是**草稿**：`goal_gate --full` 的 `master_sync` 在 walgit 故障期必然 need_human；恢复后应重跑四条件再定稿。
- 合并清单的 approve oid 以合并提交题注/`merge_result` 为准；双 approve 的卡只列主 approve（题注所载）。
- 「进行中 249」含大量 R 卡等窗口（非全部在飞），未逐卡复核；以 `walgit collab board` 为准。
- 无临时物；本文件是唯一仓库改动。
