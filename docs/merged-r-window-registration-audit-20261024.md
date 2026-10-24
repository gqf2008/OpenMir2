# 5(+2) 张已合并未收口卡的窗口项登记核对（返工 v2）

- 卡：`openmir2-merged-r-cards-window-registration-audit`（tier=L）；写集 `docs/` + 各卡 `status`。
- 返工依据：reviewer-3 `bede2a6f9` 的 `request_changes`（5 Important + 4 Minor）。本版逐条闭环，编号沿用该 review。
- 基线：base = `walgit/master 6cb7d72b4`；上一版文档取自 head `6ed2670b2`。
- **快照戳**：`@ 6cb7d72b4 @ 2026-10-24 10:35 +0800`（本文件所有漂移计数均带戳）。

## 0. 本版相对上一版的实质变化（先看这里）

| review 项 | 问题 | 本版改法 |
|---|---|---|
| I-1 | 7 条新 status **没有 `work` 字段** ⇒ 看板投影回退到旧的机械登记文案，人看到的主视图没变 | 每条 status 都写 `work`（一句话「已按原 review 校正窗口项」+ 关键变化）；看板 `work` 已实际更新（见 §5 对账） |
| I-2 | `client-e2e` 少列 review 点名的 ③a/③b 两条 RED，并复列已 GREEN 的 52/752 | 按 `746c5f34b` + `08a3ebab1` + `ddad4abdd` + `8073c2ffd` 重写清单；52/752 移入 `not_counted` |
| I-3 | 本文件计数行缺快照戳 ⇒ 合入 master 后 `dynamic_count_stamp_gate` rc=1 | 计数行内补 `@ 6cb7d72b4`；`python -I tools/hygiene/dynamic_count_stamp_gate.py` rc=0 |
| I-4 | 两张 `tier_pending` 卡的 write_set 写成「`tools/botload` + 文档」，与 issue 原文不符 | 按 `937cb2d0a` / `0ddd513ba` 的 write_set 原文重写建议，并对 `server-state-dump` 明确「改了 game-svr 生产源为何仍建议 L」 |
| I-5 | `conformance-rng` 漏列 approve/R `46e479fc5`，且该卡在 goal_gate 无登记 | 报告补列该 review；回一条带 `window_required: true` 的 status（登记，不是签 tier），机器口径里改归 `window_required` |
| M-1 | 章节/行号引用错 | 卡表统一改「§3 逐卡表第 N 行 :L」，`§5.4` 与 `§7.3/§7.4`（场景表）分清；`:286`→`:126`；删「场景列未指派」；「第 2 条」→`M-4` |
| M-2 | 新 status 丢父条目的 `tier` / `tier_pending` | 5 张 R 卡 status 保留 `tier:"R"`；两张工具卡保留 `tier_pending:true` |
| M-3 | `ybdeal` 的 `window_items: []` 未与 `b1f716d58` 的 `window_required:true` 对账 | §2.1 明写「goal_gate 仍按 review 计入 window_required；要移出需 owner/reviewer 另行裁定」 |
| M-4 | 「530 末段」被挂在 F6 下，F6 只点名 14/10 | 530 改挂 `8a59cd6ca` 的 **F1**（规则 bug，已改 `[[20,null]]` 并由四端口 + SessionID 逐位实测验证） |

## 1. 对照表（原始 review → 准确清单）

> 引用口径：卡清单 = `docs/window-batch-plan-20261023.md` **§3 逐卡清单**（:81 起）；
> 「不是窗口欠账」表 = 同文件 **§5.4**（:347）；
> **§7.3/§7.4 是场景表**（:462 / :473），不含下列卡，本文件不再把它们当作卡清单来源。

| 卡 | 原始 review（oid / verdict / tier / 窗口登记） | 准确清单（本卡回写 `window_items`） | 差异与依据 |
|---|---|---|---|
| `openmir2-ybdeal-frames` | `b1f716d58` approve / R / `window_required=true` | **`window_items: []`**（不计真机窗口债） | §3 表第 32 行（`:283`）与 §5.4（`:355`）明写「部署级不可达（OPENYBDEAL=0/QUERYYB=0），走登记路径（T-203），替代 = 登记 + 构造用例」⇒ 今天**无真机发射点可采**；把「真机对拍」再列一次是重复计债。**⚠ 机器口径仍按 `b1f716d58` 的 `window_required:true` 计入**（见 §2.1） |
| `openmir2-menu-ok-unify-pickwire` | `ebd303308` approve / R / `window_required=true`；`ba93bcb90` request_changes / R / `window_required=true` | ① `dice_chain_auto` 末步 `SM_MENU_OK(767)` 真机复跑（§3 表第 38 行 **`:126`**，场景 `dice_chain_auto`）② 比奇国王 `@main→@rw02→[@rw03]` ELSEact 的 767 帧面与 C# 逐字节（`ebd303308` window ①）③ QManage `@Login` 整段次序（750/767/100）**且 767 与登录延迟尾（750=61 / 100 / 654=153）的相对出队序**（`ebd303308` window ③④ + `ba93bcb90` **M-4**） | 补记①「下沉到 pickwire」是实现范围（已 approve），不是新窗口债。QFunction 那一路**不计**：部署 `QFunction-0.txt` 无 `@Group*` 标签、`run_party_goto_label` 只跑 `@Group*` ⇒ 预防性（`ba93bcb90` M-1） |
| `openmir2-m3-monster-ai-tick` | `44d3e3477` approve / R / `window_required=true`；`7d960ce6c` approve（token 修正片） | ① 真机 **100 杀 RNG 抽数序列**（含构造期 5 抽）需窗口（`7d960ce6c` not_verified）② 怪→玩家伤害（战斗数值）+ 出队次序（时序）+ RNG 取数，真机两侧对拍（`44d3e3477` window_why）；场景 = **§3 表第 12 行 `:177`**「新：主动索敌观察（W0 前置）」 | 补记①「怪侧子集」是范围描述，非欠项。`n10=999` 初值差异是**代码行为**（构造级判据），不计真机窗口债 |
| `openmir2-client-e2e-enter-gaps` | R 组：`8073c2ffd` approve / R / `window_required=true`；`ddad4abdd` approve / R / `window_required=true`；`746c5f34b` approve / R / `window_required=true` + `window_why`；最新 `92bd914b0` approve / L（docs-only runbook） | **① ③b 受击步伤害链 RED-1 小窗复跑**：C# 侧先打出 `victim=怪` 的 `SM_STRUCK` 作阳性对照 → 两侧钉同一 `(x,y)` + `CM_TURN→CM_HIT` → 判据出站 ≥1（`746c5f34b` not_verified[0] + `08a3ebab1`）**② ③a 反向对照 RED-2 小窗复跑**，净空点 **(311,666)**（`746c5f34b` not_verified[1] + `08a3ebab1`）**③ 玩家贴怪静止、看 Rust 有无「怪打玩家」帧**（未归因探针，`08a3ebab1`）**④ 受击后 HUD 血条 / `SM_STRUCK` HP-MaxHP 形态**（`17/19` 与 `0/0`、`655360/25165829` 应消失；`ddad4abdd` not_verified[0]）**⑤ 跨侧 s2c 逐帧序/延时**（oracle 该 RM_STRUCK 走 `LateDelivery=200ms + Lowest`，本实现即时发，`200ms` 未 1:1；`8073c2ffd` not_verified[1] + missing_evidence） | ①「进图 52/752 未下发 + 世界 tick 无实体」**移入 `not_counted`**：`746c5f34b` 明写「①②③ GREEN 已由本片真机证据覆盖」，`93a349648` 记「修复族已被片 67 实拍闭合」②`8073c2ffd` O-3 的 display 表门降为 ③b 的子注，不单列成独立债。③ 最新一条 review 是 L，但只是 docs-only runbook，R 债不因最后一条 L 消失 |
| `openmir2-conformance-rng-nondeterminism` | `8a59cd6ca` request_changes / R；**`46e479fc5` approve / R**；`2f4b311f1` approve / L | ① ident 14 的 `ignore` 规则要在真机数据上**真正行使**（最终 green 那次 `ignored 0/0`，`8a59cd6ca` **F6**）② ident 10 的 series 掩码要在真机数据上行使（同上，F6：`masked_head` 无 series）③ 530 末段的 EDCode 线缆偏移（`8a59cd6ca` **F1**：`[[16,null]]` 在真实字节上掩掉的是「端口后三位 + 斜杠 + SessionID」，已改 `[[20,null]]` 并由四端口 + SessionID 逐位实测验证；**真机尚未行使**）④ 真机双栈同序列对拍 | ①F10/F11（`2f4b311f1`）是文档/计数问题，不计窗口② F11 的 ident 14 `approved_by` 保持 `PENDING` 是**前置依赖**（`openmir2-castle-merchant-fixedhide-hit` `1a807804`），不重复计债，但它是 ① 的前置③ **530 归 F1 不归 F6**（M-4） |
| `openmir2-m2-rust-state-log-plumb` | `7a490a60b`（建卡早于 tier 约定，无 tier） | **`window_items: []`**（工具卡） | §5.4（`:352`）「非窗口欠账（工具）」；`check_state_log_plumb` 15/15。**tier 建议 L**（见 §3） |
| `openmir2-m2-rust-server-state-dump` | `9795e068c`（无 tier；R-1 dump 形状 1:1 / R-2 触发点） | **`window_items: []`**（工具卡） | §5.4（`:351`）「非窗口欠账（工具）」；`server_state_gate.py` GREEN rc=0 + selftest 6/6。**tier 建议 L**（见 §3） |

## 2. 「不该算窗口债」的逐条依据

### 2.1 `ybdeal-frames`：登记型替代，但**机器口径仍计入**
- 依据：§3 第 32 行 `:283` + §5.4 `:355`（T-203 部署级不可达，替代 = 登记 + 构造用例）。
- **对账（M-3）**：`b1f716d58` 带 `window_required:true` 且警告「不要因零帧用例全绿把它当已验证 R 关掉」。
  `goal_gate` 只读 `body.window_required`，**仍把本卡归入 `window_required` 桶**（PASS，不是无债）。
  要把它移出需要 owner/reviewer 出一条 approve + `window_required:false` + `why_not_window` 裁定；
  **本卡无权也不应代判**，因此 `window_items: []` 只表示「没有新的真机采集项」，不改变机器计数。

### 2.2 两张工具卡：§5.4 明写非窗口欠账
- `state-log-plumb`（`:352`）与 `server-state-dump`（`:351`）都在 §5.4「其实不是窗口欠账」表内，且各有可跑的 GREEN 判据。
- 它们不动 R 枚举（issue 无 `tier`），本卡只保留 `tier_pending:true` + 建议，不签 `tier`。

### 2.3 实现范围不是欠项
- `menu-ok` 的「下沉到 pickwire」、`m3-monster` 的「怪侧子集」是**做了什么**，不是**还欠什么**。

### 2.4 已闭合项
- `client-e2e` 的「进图 52/752 未下发 + 世界 tick 无实体」：`746c5f34b` 判「①②③ GREEN 已由本片真机证据覆盖」，
  `93a349648` 记「修复族已被片 67 实拍闭合」。

### 2.5 文档/计数/预防性项
- `conformance-rng` F10/F11（`2f4b311f1`）= 文档与计数；`menu-ok` 的 QFunction 路 = 预防性（部署无 `@Group*` 标签）。

### 2.6 前置依赖
- `conformance-rng` 对 `castle-merchant-fixedhide-hit(1a807804)` 的依赖是排期依赖，不重复计债。

## 3. 两张 `tier_pending` 卡（只给建议，不改 tier）

- `openmir2-m2-rust-state-log-plumb`
  - **issue write_set 原文**（`937cb2d0a`）：仅 `tools/m2-acceptance/**`（`rust_seed_plumb.ps1` / `start_rust_stack.ps1` / `check_state_log_plumb.ps1` / `README.md` / `evidence/m2svr-rust-state-log-plumb-20261013/**`）；未动 `mir2-rs/**`、`crates/**`、`src/**`、`tests/parity/m3/**`。
  - **建议 L**：纯工具 + 证据 + 文档，不触 RNG/时序/落库/战斗数值；判据是 `check_state_log_plumb` 的进程内断言（15/15）。
  - 本版**不再**引用 `tools/botload`（那是 BotSrv 假人护栏目录，与本卡无关）。
- `openmir2-m2-rust-server-state-dump`
  - **issue write_set 原文**（`0ddd513ba`）：仅 `mir2-rs/bins/game-svr/src/{state_log.rs(新),main.rs}` 与 `tools/m2-acceptance/evidence/m2svr-rust-server-state-dump-20261013/**`（证据）；未动 `crates/world`。
  - **这一点必须说清**：本卡**动了 game-svr 生产源码**（新增 `state_log.rs` + 改 `main.rs`），不是「工具 + 文档」。
    仍建议 **L** 的理由只能是：新增的是**诊断面**（服务端权威状态 dump / `SERVER_STATE`），
    不改 RNG 抽数、不改时序/出队序、不落库、不改战斗数值；证据是 `server_state_gate.py` GREEN + selftest 6/6。
  - **重申保留**：`9795e068c` 的 R-1「C#/Rust dump 形状是否 1:1」若 owner 判**需真机比对**，就应按 R 重判；本卡只给建议。

## 4. 与 `goal_gate` 机器口径的对账（返工前 / 返工后）

命令（快速模式，仓库根 = 本 worktree）：

```sh
python -I tools/goal/goal_gate.py --json --skip-tests --skip-script
```

| 口径 | 返工前（本卡 base `6cb7d72b4`） | 返工后（本卡返工状态生效） | 变化 |
|---|---:|---:|---|
| R 卡总数 | 200 | 200 | 0 |
| `window_required`（另未合并 2） | 130 | **131** | **+1** |
| `whitelist` | 48 | 48 | 0 |
| 已裁定登记 | 3 | 3 | 0 |
| `merged_not_closed_no_registration` | 6 | **5** | **−1** |
| 其它需人工 | 0 | 0 | 0 |
| 条件②「需人工」张数 | 6 | **5** | **−1** |

> 〔2026-10-24 订正：上表 `window_required` 行的标签原写「（含未合并 2）」——误；`window_required` 与未合并 2 张是两个独立计数（`r_category` 只返回其一，分区和 `11+131+2+48+3+5 = 200` 恰好铺满），130/131 **不含**那 2 张，故改「另未合并 2」。〕

- **唯一的机器口径变化**来自 `openmir2-conformance-rng-nondeterminism`：返工前它三条 review 都无 `window_required` 布尔值
  ⇒ 落 `merged_not_closed`；本卡补一条 `window_required: true` 的 status（**登记**，不是签 tier）
  ⇒ 改归 `window_required`（PASS，真机窗口仍欠）。这**不是**让它变成「不需要窗口」。
- 其余 4 张 R 卡原本就由 review 的 `window_required:true` 计入 `window_required`，
  本卡回写的 status 不再改变计数（只把 `work` 与 `window_items` 文案改准）。
- 两张工具卡 issue 无 `tier`，本就**不在** R 枚举内；本卡**不**给它们签 `tier`。
- **诚实声明**：`goal_gate` 不读 `window_items`。因此本卡对另外 4 张 R 卡的贡献是
  「人读登记变准 + 看板 `work` 真正更新」，对条件② **计数为零**；唯一计数变化是 I-5 那一张。
  若要让「窗口项条数」进入机器口径，需要另一张卡改 `goal_gate`（本卡写集不含 `tools/`）。

## 5. 看板 `work` 字段对账（I-1）

返工前（reviewer-3 实测，本版复现）：7 张卡的 `last_oid` 已是上一版 status，但 `work` 仍是
「补记 R 级状态：… **R 级不 closed**：真机窗口未跑。」这类机械文案。

返工后：7 条 status 均带 `work`，看板投影取到新文案（逐卡内容见 §1 与各卡 status 的 `work` 字段）。

## 6. 复核命令

```sh
walgit collab thread <card> | python -I -c "import json,sys;[print(e['oid'][:9],e['entry']['kind'],(e['entry'].get('body') or {}).get('tier')) for e in json.load(sys.stdin)[-5:]]"
walgit collab board --format json | python -I -c "import json,sys;d=json.load(sys.stdin);print([(c['id'],c.get('work','')[:40]) for col in d['columns'] for c in col['cards'] if c['id'] in ('openmir2-ybdeal-frames','openmir2-menu-ok-unify-pickwire','openmir2-m3-monster-ai-tick','openmir2-client-e2e-enter-gaps','openmir2-conformance-rng-nondeterminism','openmir2-m2-rust-state-log-plumb','openmir2-m2-rust-server-state-dump')])"
python -I tools/hygiene/board_coverage_gate.py --board .walgit/board.toml; echo rc=$?   # 须 0
python -I tools/hygiene/stale_pointer_gate.py; echo rc=$?                               # 须 0
python -I tools/hygiene/dynamic_count_stamp_gate.py; echo rc=$?                         # 须 0
python -I tools/goal/goal_gate.py --json --skip-tests --skip-script                      # 条件② 数字见 §4
```
