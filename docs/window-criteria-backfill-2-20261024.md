# backfill 第二批：未取的 10 张六字段回填（`openmir2-window-criteria-backfill-2`）

- 卡：`openmir2-window-criteria-backfill-2`（tier=L）；分支 `card/openmir2-window-criteria-backfill-2`；写集 `docs/`、`mir2-rs/tests/parity/whitelist.md`（本批未动 whitelist）。
- 基线：`walgit/master = 71420a145`（2026-10-24；接管返工时 rebase，v2 原基线 `8dcb8c8ca`）；判据模板 `docs/window-criteria-template.md`，只告警校验 `tools/goal/window_criteria_lint.py`。
- 上游：`docs/window-criteria-backfill-20261024.md` §2「未取的 10 张及理由」；本批把这 10 张逐张落到**机器可判的结论**。
- **真机项全部未跑**（本批只有离线/构造级命令实跑过）；不把「判据补好」写成「窗口已过」。

## 1. 分桶（互斥，合计 10）

| 结论类 | 张数 | 卡 |
|---|---:|---|
| ① 现在就能跑（离线命令今日 rc=0；真机待窗口授权） | **1** | `login-529-status-text` |
| ② 差一个具体前置（点名缺什么 + 承接卡/建议 id） | **7** | `castle-merchant-disappear-downlink`、`chat-channels`、`m3-castle-gold-tax`、`m3-clock-injection`、`gategate-keepalive`、`selgate-reconnect`、`monster-showhp-53-broadcast` |
| ③ 只能构造级（cargo 用例名 + 断言 + 真机不可跑原因） | **2** | `orphan-encoder-wiring`、`m3-monster-active-seek` |

## 2. 逐卡总表（卡片可见位置 = 该卡线程的签名 comment）

| 卡 | 类 | `items[].owner`（该卡最近 `status` 的 owner） | 签名 comment（r1；r2 为准 = 补 lint 缺项） | 今日实跑证据 | lint |
|---|---|---|---|---|---|
| `openmir2-castle-merchant-disappear-downlink` | ② | `openmir2-worker-33` | `345ab00d1c78cf3bb19938518e2a450bdb2b6dfa` | `mir2_probe --check-scenario king_castle_war.json` rc=2（缺 `--var king_actor`，fail-closed） | 0 warning |
| `openmir2-chat-channels` | ② | `openmir2-orchestrator` | `36a2998b7d0b8b8c0b330236caa70281afa4b827` | `mir2_probe --help \| grep -c -- --sessions` ⇒ 0（无多会话发起） | 0 warning |
| `openmir2-m3-castle-gold-tax` | ② | `openmir2-worker-5` | `d2380a41ceb23693a50343a35979c50b2dfc8059` | 同上 `--check-scenario` rc=2（缺 `--var`） | 0 warning |
| `openmir2-m3-clock-injection` | ② | `openmir2-worker-11` | `fe3e38839f93f92306aa9746125eb9ad12f56235` ／ r2 **`7ab25b13a891322462703559f236e168c17619b6`** | `git grep MIR2_CLOCK`：Rust 注入面在位；C# 侧待 owner 裁决 | r2 = 0 warning |
| `openmir2-orphan-encoder-wiring` | ③ | `openmir2-coordinator` | `9e5f9198c61dcc6d6d3798d18e0865b22f1cdc0b` | `cargo test -p game-svr --lib map_event_enter_sends_one_showevent` / `…leave_sends_one_hideevent` / `…butch_e2e…sends_skeleton` 各 **rc=0** | 0 warning |
| `openmir2-gategate-keepalive` | ② | `openmir2-orchestrator` | `f9e3cfda4348e7ef5949684dcc28b6e6b7ad98de` ／ r2 **`86a375718af499b94e3450217f8af7342b9dd3ad`** | 无现成场景（场景待建）；长跑 >3min 未跑 | r2 = 0 warning |
| `openmir2-selgate-reconnect` | ② | `openmir2-orchestrator` | `f5d384808aab132eb820b0f8076d9c042f17181a` ／ r2 **`eaa52442977d727193ad850bc1ab13ecdc35efb4`** | 无现成场景/工具（需 DBSrv 重启） | r2 = 0 warning |
| `openmir2-login-529-status-text` | ① | `openmir2-orchestrator` | `d4678e09ac51281665dfbda94460ac9522c5bf5f` | `mir2_probe --check-scenario login_only.json` **rc=0**；`card_window.py --dry-run --profile alt` **rc=0**（打印完整双栈序列） | 0 warning |
| `openmir2-monster-showhp-53-broadcast` | ② | `openmir2-worker-52` | `d368b3a2006690055c711a613f71cc206b8ca17e` | `--help \| grep -c -- --sessions` ⇒ 0（±12 收件人集合无法构造） | 0 warning |
| `openmir2-m3-monster-active-seek` | ③ | `openmir2-worker-22` | `eb097230f70c446aeaf6cd540da51b7c89f2956d` ／ r2 **`43da132c21b3e77d1dc807b397fde024d23f0766`** | `cargo test -p mir2-world --lib active_seek_race_searches_only_under_clock_gate` / `active_seek_is_rng_free_and_non_seek_race_never_searches` 各 **rc=0** | r2 = 0 warning |

> 4 张的 r1 文本被 `window_criteria_lint.py` 点名缺 ②/③ 关键词（② 写「无现成场景」= 没有命令、③ 缺 `ident`/`帧序` 字样）；
> 已各补一条 **r2** comment（`backfill: window-criteria-backfill/2-r2`，`supersedes_prose_of` 指向 r1），r2 文本 lint **0 warning**。
> 其余 6 张 r1 即 0 warning。
> r3（返工）**最终为准**：owner 纯 principal、expect 补具体帧 ident、lint 输入原文随附（见 §8.1 / §9）。

## 3. `login-529-status-text` 单独说明（本批唯一「现在就能跑」的判据）

- 判据：`login_only` 两侧登录帧序逐字节 + `SM_SERVERINFO(529)` 的 `body_hex` 两侧**相同且非空**。
- 旧结论「Rust 不上报 `SS_SERVERINFO` ⇒ 状态段恒 0」**已作废**（`185a6c8cb` 合入；`mir2-rs/bins/game-svr/src/ids.rs:17-19` 已接）。
- 上游审计 `docs/window-criteria-quality-audit-20261024.md` §5.2 **#26** 已**就地订正**（保留原说法 + 订正日期 + 依据 commit）。
- 可复跑命令（今日离线 rc）：
  ```sh
  python -I -B tools/probe/mir2_probe.py --check-scenario --scenario tools/probe/scenarios/login_only.json      # rc=0
  python -I -B tools/conformance/card_window.py --dry-run --card openmir2-login-529-status-text \
      --scenario tools/probe/scenarios/login_only.json --profile alt                                          # rc=0
  # 真机（未跑，需窗口授权）：去掉 --dry-run 并给 --out <dir>
  ```

## 4. 多会话类判断（`chat-channels`、`monster-showhp-53-broadcast`）

- `tools/probe/mir2_probe.py` **不能**发起第二个客户端（`--help` 无 `--sessions`）；`openmir2-multi-session-probe`（`2a5101606`）合入的是**收件人断言 helper**（`recipients(..., sessions=/statuses=)`、`online_sessions()`），**不是**多客户端拉起。
- `gate_shadow_proxy.py` 是网关侧影子代理（抓/转帧），也不负责起会话。
- ⇒ 两张卡判 **②**，建议承接卡 id `openmir2-probe-multi-session-launch`（板面无，本卡**不自行开卡**）。

## 5. 口径不动（可复算）

- 未改 `tools/goal/goal_gate.py`、`rules.json`、`card_window.py`、场景 JSON、任何他人卡片 status。
- 本批只新增 comment（10 张 × r1 + 4 张 × r2 + 10 张 × r3 = **24 条签名 comment**，均 `verified=true`）与本文，并就地订正上游审计 `docs/window-criteria-quality-audit-20261024.md` §5.2 **#26 一行**（保留原说法 + 订正日期 + 依据 `185a6c8cb`）；`whitelist.md` 未改。
- `python -I -B tools/goal/goal_gate.py --json --skip-tests --skip-script` 的条件② detail 与改前一致（本批不动判据/登记）。

## 6. 门禁

| 门 | 结果 |
|---|---|
| `python -I -B tools/whitelist/t_reservation_gate.py check` | GREEN（255 行 / whitelist 255 号；max=T-267） |
| `python -I -B mir2-rs/tests/parity/m3/whitelist_anchor_gate.py check` | ALL PASS（587 条锚） |
| `python -I -B tools/hygiene/conflict_marker_gate.py` | OK（0 markers / 7443 tracked；tracked 数随 master 漂移） |
| `git merge-tree --write-tree walgit/master HEAD` | rc=0（tree `5aa281fc4ed8e216a2ae249b837237bdcf9773aa`） |

## 7. 残留

- 10 张的**真机窗口全部未跑**；① 类也只有离线命令。
- ② 类的承接卡一律**只点名不建卡**：`openmir2-castle-under-war-writer`、`openmir2-probe-multi-session-launch`（×2：`chat-channels`、`monster-showhp-53-broadcast`）、`openmir2-window-selgate-restart-tool`、`openmir2-clock-injection-scenario`、`openmir2-gategate-keepalive-rule-decision`；`gategate-keepalive` 的**承接卡 = 本卡** `openmir2-window-criteria-backfill-2`（若需独立成卡则用上述建议 id）。
- ③ 类（2 张）只有在构造级用例变化时才需重跑；真机不可跑的结论登记在卡片 comment。

## 8. 返工 v2（review `0e8d75cc`，request_changes）

| 项 | 处置 |
|---|---|
| **R1（阻断）** | `openmir2-gategate-keepalive` 补承接卡：**承接卡 = 本卡**；若需独立成卡，建议 id **`openmir2-gategate-keepalive-rule-decision`**（未自行开卡）；已补进 §7 清单与该卡 r3 comment。 |
| **M1** | 10 张的 r3 comment 把 `items[].owner` 改为**纯 principal**（状态另置 `items[].status`）。 |
| **M2** | 冲突标记只写 **0 markers**，不写死 tracked 数（会随 master 漂移）；v2 head 实测 **7439**、base `8dcb8c8ca` 为 7438；接管复核（head `13fcd46d6`）实测 **0 markers / 7443 tracked**（base master `71420a145` = 7442）。 |
| **M3** | §5 已补「并就地订正审计 §5.2 #26」这一事实。 |
| **M4** | 每张 r3 comment 带 **`rendered_text`（lint 的输入原文）** 与命令 `python -I -B tools/goal/window_criteria_lint.py --text <rendered_text>`，可复算（10/10 OK）。 |
| **M5** | goal_gate 改前/改后快照：base `01d4f5877` 与 head 均 **`need_human`**，条件② detail **逐字节相同**（同一仓库状态上分别用 `--repo <base-worktree>` 与 `--repo .` 跑 `--json --skip-tests --skip-script`；10 张目标卡 category 全未变）。**接管复核快照（可复算）**：改前 `E:/tmp/w73-gg-before.json`（master `71420a145`，sha256 `8853e3753f34b69fbd98cd4de68a426a5fbc24ec5883f6ef2eefa560b9927621`）、改后 `E:/tmp/w73-gg-after.json`（head `13fcd46d6`，sha256 `301c213f1ea1e58734b3befd5cc0534b4eaab77876bdad8dff8309fe17bcd7cd`）；条件② detail 逐字节相同，`r_cards` 197 张逐卡 category diff = `{}`（10 张目标卡均为 `window_required`）。 |
| **M6** | `gategate` 的 expect 写死 **`SS_KEEPALIVE(104)`**（`Messages.cs:440`；网关 3min 超时表现为连接关闭、无 SM_ 帧）；`clock-injection` 的 expect 写 **`SM_TURN(10)`** 与 **`SM_HEALTHSPELLCHANGED(53)`**（可选加 `655`/`642`）。 |

### 8.1 r3 comment oid（owner 纯 principal + 渲染文本；本表为准）

| 卡 | r3 oid |
|---|---|
| `openmir2-castle-merchant-disappear-downlink` | `90802f6a73e6bf3da7db9096d8519e5a729b7782` |
| `openmir2-chat-channels` | `49d11751feae78957704ce4a2471ee103e7f33b4` |
| `openmir2-m3-castle-gold-tax` | `e21d555ec405e5a9474322a3ff2dbdd14ed2a3b9` |
| `openmir2-m3-clock-injection` | `88515588a1f5102fa9802f2c4897580e848a3937` |
| `openmir2-orphan-encoder-wiring` | `501958fca5c19c827111c77aed50d279a82568f3` |
| `openmir2-gategate-keepalive` | `db6598a57fc443996281fbd963a3cae000a0a5ec` |
| `openmir2-selgate-reconnect` | `419d1d4f4841ef51cf4fbc5c500ab88db487a34f` |
| `openmir2-login-529-status-text` | `f8501182085240f06cc2948dce02f155313fa58f` |
| `openmir2-monster-showhp-53-broadcast` | `18f14e03bf0a158434ff0f90562d985624e99e09` |
| `openmir2-m3-monster-active-seek` | `3074d4ba9107f228b0a218fe159e3159d93111f9` |


## 9. 接管复核（`openmir2-worker-73`，head `13fcd46d6`，base `71420a145`）

- 接管链：takeover `fff41f45aa510f25fca436cb41b4ad885722d6f8` → `status: in-progress` `6f87edef4f1157eea2e8c312741324b079f50a0b`；worker-24 自 v2 起停止本分支写入。
- rebase：v2 `825cf63ac` → **`13fcd46d6`**（`git rebase walgit/master` rc=0，无冲突）；本 §9 的每个数字都在该 head 上重跑。
- **R1（承接/建议 id）**：`openmir2-gategate-keepalive` 的 r3 `db6598a57fc443996281fbd963a3cae000a0a5ec` 写死「承接卡 = 本卡 `openmir2-gategate-keepalive`；建议 id `openmir2-gategate-keepalive-rule-decision`（未自行开卡）」。
  - 复算：`git cat-file -p db6598a57 | grep -o 'openmir2-[a-z0-9-]*' | sort -u` → `openmir2-gategate-keepalive` / `openmir2-gategate-keepalive-rule-decision` / `openmir2-orchestrator` / `openmir2-worker-24`。
  - 板面确无该建议卡：`walgit collab thread openmir2-gategate-keepalive-rule-decision` → `Error: no entries for thread …`。
- **M1（owner 纯 principal）**：10 条 r3（§8.1）的 `items[0].owner` 全部为纯 principal；逐卡与该卡线程**最后一条 `status` 的 owner** 比对 **10/10 MATCH**（worker-33 / orchestrator / worker-5 / worker-11 / coordinator / orchestrator / orchestrator / orchestrator / worker-52 / worker-22），状态另置 `items[0].status`。
- **M2（conflict marker）**：`python -I -B tools/hygiene/conflict_marker_gate.py` → rc=0 `OK (0 markers across 7443 tracked files)`；base master `71420a145` = 7442。
- **M3（写集披露）**：§5 已写「本文 + 就地订正审计 §5.2 #26 一行」；`git diff --name-status walgit/master 13fcd46d6` 仅 `A docs/window-criteria-backfill-2-20261024.md`、`M docs/window-criteria-quality-audit-20261024.md`。
- **M4（lint 可复算）**：10 条 r3 的 `rendered_text` 已逐张落盘到 `E:/tmp/w73-lint/<card>.txt`，逐张执行
  `python -I -B tools/goal/window_criteria_lint.py --text E:/tmp/w73-lint/<card>.txt` → **10/10 rc=0 `OK: 六字段齐全`**
  （r1 的 4 条缺项也可用同样方式复现）。
- **M5（goal_gate 改前/改后）**：见 §8 M5 行的两个快照路径与 sha256；命令
  `python -I -B tools/goal/goal_gate.py --repo <master-worktree|.> --json --skip-tests --skip-script`（两处 rc=2、`overall=need_human`）；
  条件② detail 逐字节相同，`r_cards` 逐卡 category diff = `{}`。
- **M6（expect 帧 ident）**：`gategate` r3 = `SS_KEEPALIVE(104)`（`src/OpenMir2/Messages.cs:440`）；`clock-injection` r3 = 最小集 `SM_TURN(10)`（`Messages.cs:161`）与 `SM_HEALTHSPELLCHANGED(53)`（`Messages.cs:211`），可选 `SM_LAMPCHANGEDURA(655)` / `SM_DURACHANGE(642)`。
- **门禁实跑（本 head）**：`t_reservation_gate` rc=0 GREEN 255/255；`whitelist_anchor_gate` rc=0 ALL PASS 587；`conflict_marker_gate` rc=0 0/7443；`git merge-tree --write-tree --name-only walgit/master HEAD` rc=0（tree `5aa281fc…`）。
- **离线/构造级命令实跑（本 head）**：`mir2_probe --check-scenario login_only.json` rc=0；`card_window --dry-run … --profile alt` rc=0；`king_castle_war.json` rc=2（缺 `--var king_actor`，fail-closed）；`mir2_probe --help | grep -c -- --sessions` = 0；5 条 cargo 用例（`map_event_enter_sends_one_showevent`、`map_event_leave_sends_one_hideevent`、`cm_butch_e2e_enqueues_butch_effect_and_sends_skeleton`、`active_seek_race_searches_only_under_clock_gate`、`active_seek_is_rng_free_and_non_seek_race_never_searches`）各 rc=0。
- **残留**：10 张真机窗口仍未跑（结论登记在卡 comment）；`E:/tmp/w73-gg-before.json`、`E:/tmp/w73-gg-after.json`、`E:/tmp/w73-lint/` 为复核快照，合并后可删。
