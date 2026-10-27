# 窗口批次执行单（2026-10-27，对账 master `d470d7bd1`）

> 用途：窗口开启后按本单逐组跑定向批次，产出各 R 卡的窗口证据。
> **本单不构成任何 R 卡的「已验」**：所有 R 卡在真机批次跑完、按各自验收判据判定并回填签名证据前，保持 `in-progress`（AGENTS.md「判据分级」）。
> 写集：本文件。未改 `tools/window/**`、未改生产代码、未改任何卡状态、未起栈、未占端口、未跑 cargo。

## 0. 使用前提（开窗当天先读）

### 0.1 现值与「旧读数不得当现值」

| 项 | 现值 | 说明 |
|---|---|---|
| master | `d470d7bd1` | 本单的 base；分支 `card/window-run-sheet` 从此起 |
| workspace 读数 | **142 suites / 3326 passed / 63 ignored**（今天的合并门，由 `openmir2-coordinator` 派工给出） | 本单**未重跑** cargo；窗口前必须在当前 master 上重建并重测，把新条目写进 `<git-common-dir>/tester/last-workspace-test.json` |
| 证据文件里最新的历史条目 | `d315471a3`：141 suites / 3374 = 3311 passed + 63 ignored | **已过期**：其后 Rust 改动（player-ctor 7 + AOI 2 等）已超越该 tip。`d315471a3` 只能作为历史读数，**不得进判据、不得当现值** |
| `binary_stamp` | `mir2-rs/target/release/mir2-server.build.json` **当前不存在**（本单实测 MISS） | 严格 head 相等：窗口前任何提交（含 docs-only）都会打漂并逼 rebuild |
| 场景清单 | `tools/window/run3-scenarios.json`：26 项，**21 可跑 / 5 blocked** | blocked 单独列在 §4，**不计入可跑** |

**窗口前必须执行**（顺序固定）：

```sh
# 1) 停 C#：mysqld 必须优雅关闭（禁止强杀）
pwsh -NoProfile -ExecutionPolicy Bypass -File tools/window/stop-mysql-graceful.ps1

# 2) castle 三侧基线（停完 C# 后）
python -I -B tools/window/preflight_private_stack.py --castle-before-only --castle-before E:/tmp/window/castle-before.txt

# 3) 用当前 master 重建 release + 生成 binary_stamp + 对齐 start-rust
pwsh -NoProfile -ExecutionPolicy Bypass -File tools/window/rebuild-server.ps1 -SyncStartRust

# 4) 私有栈前置自检（含 head/sha256 对账、pin 冒烟、dry-run）
python -I -B tools/window/preflight_private_stack.py --json E:/tmp/window/preflight.json

# 5) 确认 binary_stamp.head == 当前 master，然后才跑 §2 的组
```

`run_batch.sh` 默认会重建 + 前置自检；**若用 `--no-rebuild/--no-preflight`，必须先在日志里登记理由并附第 4 步的 preflight JSON**（run3 的一轮就是这样跑的，见 `docs/window-run3-20261027.md` §1）。

### 0.2 数据面口径（tier / window_required 逐卡读线程尾）

- 扫描范围：`walgit collab board --format json` 中 `merge != null && status == in-progress` 的 **182** 张「已合并未 closed」卡。
- 逐张 `walgit collab thread <卡 id>`，取**线程尾最近的 `status` / `merge_result` / `review` 条目**，读该条目的 `tier` 与 `window_required`；**该条目缺字段就记「缺失」，不回填更早条目**（避免把历史判级当现值）。分布：

| tier / window_required | 张数 | 处理 |
|---|---:|---|
| `R` / `true` | **111** | 本版纳入 2 张（§2 G1/G3），其余 **109** 张列附录 A 待第二版分组 |
| `R` / 缺失 | 43 | **先补登** `window_required`（AGENTS.md：缺字段按 R 保守）再排期 |
| `R` / `false` | 1 | 与「缺字段」同处理：核实是否真的不需要窗口 |
| `tier` 缺失 / `true` | 11 | 先补登 `tier`（缺 tier 按 R 保守） |
| `tier` 缺失 / 缺失 | 0 | — |
| `L` / `true` | 6 | 本版纳入 4 张（G2/G4/G5），另 2 张未映射 |
| `L` / 缺失 | 7 | 不占窗口；按需补登 |
| `L` / `false` | 3 | **不占窗口**；本版纳入 1 张为非窗口行（G6） |

> 合计 111+43+1+11+6+7+3 = **182**。
> 口径说明：同一张卡的历史 `status` 可能与后来的 `review` 不同（例如 `openmir2-makeghost-lifecycle-m3` 最终 review 已把 tier 更正为 `L`）；本表按**更晚的条目**计，且缺字段不向前回填。
> 派工口径里的「约 171 张」是**窗口桶**总量：`R/true` 111 + `R/缺失` 43 + `R/false` 1 + `tier 缺失/true` 11 + `L/true` 6 = **172**，与「约 171」吻合；明确 `R+true` 为 **111** 张。本单的「纳入」= §2 表里的 7 行，其余在 §4 逐类说明。

## 1. 六组执行单

> 每行：卡 / tier / window_required / 合并 oid / 场景（均在 `tools/window/run3-scenarios.json` 实况内）/ 两栈 / 判据命令 / 预期产物 / 验收 principal。
> 命令均在**仓库/worktree 根**执行；`<RUN>` 由执行人取 `E:/tmp/window/run-20261027` 或批次专用子目录。**口令走环境变量 `CONFORMANCE_PASSWORD`**，不写进命令历史。

| 组 | # | 卡 id（= 线程 id） | tier | window_required | 已合并 oid | 场景（run3 n/name） | 需要的栈 | 判据命令（可复制） | 预期产物 | 验收 principal |
|---|---|---|---:|---|---|---|---|---|---|---|
| G1 aoi 290 观察者复跑 | 1 | `openmir2-aoi-disappear-observer-scope` | R | true | `590bd9a6f7fa571afeaf85d585e8dd1f5fff71f2` | #8 `move_walk`（消失帧路径；290 观察者需双客户端，smoke 记录见 `docs/smoke-real-client-20261025.md` §4） | C# 私有栈（`window-magic`）+ Rust release（同 head） | `CONFORMANCE_PASSWORD=*** bash tools/window/run_batch.sh --only move_walk --run-dir <RUN>/aoi290 --rng-log` | `<RUN>/aoi290/move_walk/{report.json,card-window.json,README.txt,csharp/frames.ndjson,rust/frames.ndjson}`；`<RUN>/aoi290/batch-summary.json` | `openmir2-reviewer-20`（最近 review）+ `openmir2-coordinator` 记账 |
| G2 mirflow A4 三项 | 2 | `openmir2-mirflow-realmachine-flow-runnable` | L | true | `27a8417dfff448e84284bab56cce78ad11ff54c6` | 基线过场用 #9 `merchant_dialog` + #2 `npc_click_auto`；真机四类动作用手写流 `tools/client/mir_flow.ps1`（A1/A2/A3 = 阶段 A，A4 = 阶段 B） | C# 真客户端 + `mir2_proxy` + hook DLL + Rust release | 完整命令见 `docs/window-criteria-operational-20261026.md` §3.1（A1 买）/§3.2（A2 卖）/§3.3（A3 NPC 对话）/§3.4（A4 战斗）。本单只增列开关：阶段 A = `-TradeStage -TradeKind buy`（卖换 `sell`）+ `-NpcDialogStage`；阶段 B = `-CombatStage`；**`-TradeNpcPixel/-TradeMenuPixel` 等像素参数必须按 §3.1 一并给**（缺则 mir_flow rc=2） | `<RUN>/mirflow-A1/flow-stages-evidence.json`、`stages.ndjson`；proxy/hook 目录 | `openmir2-reviewer-21` |
| G3 player-ctor A6 RNG 流水 | 3 | `openmir2-player-ctor-rng-order` | R | true | `e49d460f3133e2866fde54d08bedebab6f646f92` | #1 `login_only` | C# 私有栈 + Rust release（两侧都要 RNG 流水） | `CONFORMANCE_PASSWORD=*** bash tools/window/run_batch.sh --only login_only --run-dir <RUN>/ctor-a6 --rng-log` | `<RUN>/ctor-a6/login_only/{report.json,card-window.json,csharp/rng-stream.tsv,rust/rng-stream.tsv}` | `openmir2-reviewer-21` |
| G4 softclose A4 两场景 | 4 | `openmir2-softclose-waitfor-ack` | L | true | `4a11996967d8ce98e64b6e2221c993fa4bc6da05` | #25 `softclose_save` + #26 `softclose_relogin` | C# 私有栈 + Rust release | `CONFORMANCE_PASSWORD=*** bash tools/window/run_batch.sh --only softclose_save,softclose_relogin --run-dir <RUN>/softclose` | `<RUN>/softclose/{softclose_save,softclose_relogin}/{report.json,card-window.json,csharp/db/,rust/db/}` | `openmir2-reviewer-21` |
| G4（同场景第二卡） | 5 | `openmir2-makeghost-lifecycle-m3` | L | true | `985b980b2`（**线程内 `merge_result` 原文只有短 oid**，未自行补全） | #25/#26（M-3 A5 落库证据；清单 note 明写为此追加） | 同上 | 同上（与 #4 同一次批次即可） | 同上（DB 面取 `<RUN>/softclose/*/{csharp,rust}/db/manifest.json`） | `openmir2-reviewer-21` |
| G5 run3 n13+n17 两栈重跑 | 6 | `openmir2-window-run3-prereq-triage` | L | true | `c4df54e74f3102c86199084c0dd6e809442674fe`（线程尾：`f12a95ac` 为第二轮返工 `in-progress`，尚未产生新的 `merge_result`） | #13 `equip_struck_save` + #17 `upgrade_retrieve`（#17 必须带 `--timeout-ms 720000`，清单 note 原文） | C# 私有栈 + Rust release | `CONFORMANCE_PASSWORD=*** bash tools/window/run_batch.sh --only equip_struck_save,upgrade_retrieve --run-dir <RUN>/n13n17`（`upgrade_retrieve` 的 extra 由清单自动追加） | `<RUN>/n13n17/{equip_struck_save,upgrade_retrieve}/{report.json,card-window.json,README.txt}` | `openmir2-reviewer-20`（最近 review） |
| G6 probe-c2s A4 同采 | 7 | `openmir2-probe-c2s-capture` | L | **false** | `88dfe6ceccb0208e1416c60aa5e72e0c72e1c8aa` | #1 `login_only` + #8 `move_walk`（A4：登录 + CM_TURN/CM_HIT 同一次运行的 c2s/s2c） | 只需一套栈（任一），产出 c2s 旁证 | `python -I -B tools/probe/mir2_probe.py --login-gate 127.0.0.1:7000 --scenario tools/probe/scenarios/login_only.json --out <RUN>/c2s` | `<RUN>/c2s/{frames.ndjson,c2s.ndjson,run.json}`（`c2s.ndjson` 每行带 `direction`） | `openmir2-reviewer-21` |

> **G6 特别标注（不得计入 R 行数）**：该卡线程尾 `window_required: false`（review 与 merge_result 均如此）⇒ **不占窗口、不是 R 行**。列入本单是因为它可以**搭车**在 #1/#8 的同一窗口里同采，不额外占机时；不得为凑数把它算进 R 组（卡面 forbidden）。

## 2. 组内执行顺序与互斥

1. **先 G3（`login_only`）**：它最快，且能先验证 binary_stamp/两栈/compare 链是否通。
2. **再 G4（`softclose_save`、`softclose_relogin`）**：会改测试库会话/存档，必须在其它场景**之前或之后整组跑**，不要与 G5 交错（两者都动背包/存档）。
3. **再 G5（n13/n17）**：n13 需武器槽夹具、n17 需空槽 40/41；跑前按 `docs/window-run3-prereq-triage-20261027.md` 的 gate 结果确认。
4. **再 G1（`move_walk`）**：需双客户端/观察者形态，放在前四组稳定后。
5. **G2（mir_flow）最后**：它要真客户端 + proxy + hook，机时最长、环境最脆。
6. **G6 任意时刻搭车**，不改批次结果。

## 3. 判据命令的可执行性核验（A3）

逐个核过存在（`[ -e <path> ]`，输出见右列）：

| 引用 | 核验结果 |
|---|---|
| `tools/window/rebuild-server.ps1` | OK |
| `tools/window/preflight_private_stack.py` | OK（`--help` 实测含 `--castle-before-only/--castle-before/--json/--manifest`） |
| `tools/window/run_batch.sh` | OK（`--only/--run-dir/--rng-log/--dry-run/--list/--no-rebuild/--no-preflight`） |
| `tools/window/stop-mysql-graceful.ps1` | OK |
| `tools/window/invoke-bounded.ps1` | OK |
| `tools/conformance/card_window.py` | OK（`--card/--scenario/--out/--profile/--timeout-ms/--rng-log/--dry-run`） |
| `tools/conformance/compare_runs.py` | OK |
| `tools/probe/mir2_probe.py` | OK（`--login-gate/--scenario/--out/--timeout-ms`） |
| `tools/probe/scenarios/login_only.json`、`move_walk.json`、`merchant_dialog.json`、`npc_click_auto.json`、`softclose_save.json`、`softclose_relogin.json`、`equip_struck_save.json`、`upgrade_retrieve.json` | 均在 `tools/probe/scenarios/` 下存在；对应 n 见 §1 |
| `tools/client/mir_flow.ps1` | OK（`-OutDir/-ProxyDir/-HookOut/-TradeStage/-TradeKind/-NpcDialogStage/-CombatStage/-StageEvidenceCheck`） |
| `tools/dbsnap/dbsnap.py` | OK（`card_window.py` 内部调用，写 `<out>/<side>/db/`） |
| `tools/goal/goal_gate.py`、`tools/hygiene/scenario_manifest_gate.py` | OK |
| `E:/MirServer/port-status.ps1` | OK |
| `LoginGate` 端口 `7000` | 读 `E:/MirServer/LoginGate/config.conf:11`（`GatePort0=7000`） |
| `mir2-rs/target/release/mir2-server.build.json` | **MISS**（预期：窗口前第 3 步 rebuild 后生成） |
| `<git-common-dir>/tester/last-workspace-test.json` | OK（当前最新条目 `d315471a3`，见 §0.1） |

## 4. blocked 与排除项（A1 的「排除项及理由」）

### 4.1 场景清单里的 blocked（不计入可跑）

| n | 场景 | blocked 理由（清单原文摘要） |
|---:|---|---|
| 10 | `king_dialog` | 比奇国王是 `Npcs.txt` type=1 ⇒ oracle `(IMerchant)actor` 硬转抛 `InvalidCastException` ⇒ 0 帧；Rust 放行 type=1 会回 643 ⇒ R 项 |
| 11 | `king_castle_war` | 同 #10（同一 type=1 根因） |
| 12 | `npc_script_chain` | 场景语义不成立：链宿主是 type=1 `GuildOfficial` ⇒ 恒 0 帧 |
| 22 | `castle_merchant_tax` | 买武器腿缺 652 ClientItem buffer 解码取 MakeIndex；真缺口=税入账原因未定位（买药腿已由 #23 覆盖） |
| 24 | `castle_merchant_tax_upgrade` | 「不能进批」已证伪，但 `tools/window/profiles/castle_tax_upgrade.json` 尚未接入 run3 |

⇒ 可跑 **21** 个场景；本单引用的 8 个场景（`login_only`、`npc_click_auto`、`merchant_dialog`、`move_walk`、`equip_struck_save`、`upgrade_retrieve`、`softclose_save`、`softclose_relogin`）**全部在可跑集合内**。

### 4.2 卡级排除项

| 类别 | 张数 | 理由与去向 |
|---|---:|---|
| `R+true` 但窗口项不在本清单 26 场景内 | **109** | 本版不臆造场景映射；逐卡 id 见附录 A，第二版按 `docs/r-window-registry-batch-*/` 与各卡验收原文分组 |
| `R` 但线程尾缺 `window_required` | 43 | 先补登字段（AGENTS.md：缺字段按 R 保守），补登前不得排期 |
| `R` 但线程尾 `window_required: false` | 1 | 核实是否真的不需要窗口；未核实前按 R 保守处理 |
| `tier` 缺失但 `window_required: true` | 11 | 先补登 `tier` |
| `L+true` 未映射 | 2 | 非 R 窗口项：`openmir2-bag-weight-sink`、`openmir2-bag-equip-ani-count`；待确认是否仍需真机 |
| `L` 但缺 `window_required` | 7 | 不占窗口；按需补登 |
| `L+false` | 3 | **不占窗口**（G6 是其中一张，已单列标注） |

> 另有 **31** 张卡的线程里出现过本清单的场景名（`login_only`/`move_walk`/`softclose_*` 等），多数是顺带提及（例如「登录后 GM 命令」），**不构成「同场景可复跑」的证据**；本版不并入，第二版须逐条读该卡验收原文再定组。

## 5. 跑完后的收口（不得越界）

1. 每组把 `report.json`/`card-window.json`/`batch-summary.json` 的路径与 sha256 回填到**对应卡**的签名 `comment`（不改卡状态）。
2. 只有「该卡验收原文全部命中且证据齐」才由验收 principal 签 `status: closed`；PASS 的 `compare_rc=0` 不等于 R 卡收口。
3. `UNDECIDABLE` **不是通过**：批次 rc=2 时按 `docs/window-run3-20261027.md` §2 的归因写 `comment`。
4. 本单产出后**不 merge**（窗口期），等批次跑完由 `openmir2-coordinator` 合。

## 附录 A：其余 `R+window_required=true` 的 109 张（第二版分组输入）

> 来源：§0.2 的逐卡线程尾扫描（2026-10-27）。本版不为其编造场景映射。

```
openmir2-npc-click-no-response-family
openmir2-ignore-rule-count-compare
openmir2-kick-sessions-residue
openmir2-merchant-dlgselect-type1
openmir2-castle-official-actions
openmir2-turn-order-login
openmir2-click-npc-guildofficial-cast
openmir2-dir-persistence-writeback
openmir2-kick-display-residue
openmir2-softclose-cleanup-residue
openmir2-gm-script-fixture
openmir2-gm-target-state-write
openmir2-home-coords-divergence
openmir2-castle-potion-buy-response
openmir2-castle-merchant-tax-wiring
openmir2-dbsrv-session-timezone
openmir2-compare-ident53-diff-triage
openmir2-mp-session-projection-clobber
openmir2-spell-fill-projection-mirror
openmir2-inbound-dispatch-red-batch
openmir2-script-playdice-openybdeal
openmir2-neighbor-actor-frame-drift
openmir2-castle-official-design
openmir2-script-handler-msgqueue
openmir2-click-path-bag-writeback
openmir2-m3-castle-siege-state
openmir2-clearmapmon-impl
openmir2-loader-fields
openmir2-dropitem-recog-param-mixup
openmir2-npc-click-frame-seq-fix
openmir2-conformance-rng-nondeterminism
openmir2-monster-showhp-53-broadcast
openmir2-login-529-status-text
openmir2-selgate-reconnect
openmir2-gategate-keepalive
openmir2-chat-channels
openmir2-castle-merchant-disappear-downlink
openmir2-castle-guard-target
openmir2-mb7-monsters
openmir2-encoding-core-e1-e2
openmir2-weapon-durability-frames
openmir2-weapon-upgrade-values
openmir2-sm-reconnect-wiring
openmir2-map-event-frames
openmir2-sendrefmsg-range-config
openmir2-ya52-menu-ok-missing
openmir2-cm-adjust-bonus
openmir2-gm-admin-mode-entity-seam
openmir2-castle-merchant-fixedhide-hit
openmir2-door-chain-wiring
openmir2-gm-batch-4-numeric
openmir2-setmapmode-dechp
openmir2-gamesvr-serverinfo-report
openmir2-setoffline-field-writeback
openmir2-unbindlist-consumer
openmir2-sm-closehealth-wire
openmir2-verify-tick-base
openmir2-searchtick-origin-parity
openmir2-gm-commands-design
openmir2-action-line-variable-text
openmir2-script-take-persist
openmir2-npc-hit-frames
openmir2-monster-ref-drains
openmir2-guard-ctor-constants
openmir2-a22-castle-persistence
openmir2-npc-ctor-constants
openmir2-t138-request-castle-war
openmir2-npc-display-table
openmir2-single-binary-roles
openmir2-a09-upgrade-item
openmir2-merchant-race-bias
openmir2-npc-def-loading
openmir2-openybdeal-host-wiring
openmir2-superman-normnpc
openmir2-mb8-9-monsters
openmir2-monster-walk-frames
openmir2-login-charstatus-657
openmir2-db-encoding
openmir2-castle-pure
openmir2-market-def-impl
openmir2-account-protection-13
openmir2-strlist-bom
openmir2-login-654-order
openmir2-encoding-crate
openmir2-mb2-monsters
openmir2-hutil32-parity
openmir2-guild-official-npc
openmir2-mb5-monsters
openmir2-castle-war-area-impl
openmir2-dbsrv-scheme-a
openmir2-guild-g3-persistence
openmir2-castle-entities
openmir2-upgrade-npc
openmir2-guild-g2-dispatch
openmir2-run-gap-pl4
openmir2-dbsrv-fault-paths
openmir2-gate-idle-timeout
openmir2-run-gap-pl2
openmir2-gategate-slots-gm
openmir2-login-numeric-gaps
openmir2-run-gap-pl3
openmir2-delmagic-addmagic-chain
openmir2-gategate-speed-rules
openmir2-healthspell-emission-model
openmir2-monster-attack-hit-frame
openmir2-run-gap-pl1
openmir2-run-gap-mb4
openmir2-m3-pet-slave-masterdie-wire
openmir2-m3-shop-dialog-wiring
```
