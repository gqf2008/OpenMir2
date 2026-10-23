# 窗口批次彩排（2026-10-24）

- **卡**：`openmir2-window-batch-rehearsal`（tier=L；写集 `docs/`）
- **执行**：`openmir2-worker-67`，worktree `E:/tmp/wt-worker-67`，分支 `card/window-batch-rehearsal`
- **基线**：`walgit/master` = `8dcb8c8ca`（本分支从它起；下文所有 rc 都是对 **这个 head** 实跑）
- **目的**：给 owner 一张开窗当天能**照着逐条跑**的表。原则是**宁少勿假**：跑不了的就写跑不了。
- **红线遵守**：未改任何场景 JSON、未改 `tools/**`、未改 `rules.json`、未改卡片状态；未起栈、未连库、未占端口；全部命令离线。

---

## 0. 结论（先看这五条）

1. **20 个场景的离线彩排结果**：15 个「离线绿」（与 `window-batch-plan` 的 15 场景枚举一致）、2 个 `blocked`（`combat_hit` / `combat_strike`）、3 个「dry-run 绿但不可信」（`spell_cast` / `bag_equip` / `eat_item`）。逐条见 §2。
2. **`spell_cast` 的默认 profile 是假绿**：`--profile alt` 下 dry-run rc=0 且**一行魔法夹具预览都没有**；换带 `magic` 段的 profile 才出现两行夹具预览。真机侧仍不可跑（Rust 魔法帧面 + 第二观察者）。
3. **`bag_equip` / `eat_item` 的 inventory 夹具当前没有可用路径**：`card_window.py` 已支持**场景级** `inventory`（已合 `bb4317a01`），但 `mir2_probe.py` 的顶层 schema 不收该键 ⇒ 一旦在场景里声明，`--check-scenario` 与真跑都 rc=2。**dry-run 对 inventory 也完全不预览**（只预览 placement 和 magic）。详见 §3.2。
4. **`dice_chain` 有一个窗口级隐患**：`params.npc_actor = 119` 是 r9 抓包里的赌场 NPC actor id，而 C# 的 ActorId 每会话不同；它又不在 `required` 里 ⇒ `card_window` 不会为它做 discovery。**开窗时要么带 `--var npc_actor=<本次发现值>`，要么直接跑 `dice_chain_auto`**。
5. **计数口径**：按 Leader 裁决（drift-sweep 线程 status `eb667db0`）只看**逐行枚举**，禁止 `18−N` 反推。落地位置是 **`docs/window-batch-plan-20261023.md` §7.4 @ `6ceb0689d`**（drift-sweep v5，**尚未合入 master**），枚举为 **10 行 / 15 个场景**。本次实跑与它一致；偏差逐条见 §5。（派单文字写的是「§7.5」，但 §7.5 是「判据口径」，枚举实际在 §7.4；master `8dcb8c8ca` 的 §7.4 仍是旧口径 14。）

---

## 1. 口径与命令

### 1.1 两个命令（逐场景实跑）

```sh
python -I -B tools/probe/mir2_probe.py --check-scenario --scenario tools/probe/scenarios/<scene>.json
python -I -B tools/conformance/card_window.py --dry-run --scenario tools/probe/scenarios/<scene>.json \
  --card <建议卡 id> --out <临时目录>
```

- `--check-scenario`：纯加载期校验（不连网）。**rc=0 = 场景形状合法**；**rc=2 = 场景非法或缺必填 `--var`**。`blocked` 是合法形状（仍 rc=0），由 `card_window` 拒。
- `card_window --dry-run`：只打印完整命令序列，**不起栈、不建 out**（本次全部用的 `--out E:/tmp/w67-rehearsal/out/<scene>` 自始至终没有被创建）。`blocked` 场景在这里 rc=2；`required` 场景会先打印 discovery 命令，`--var` 值以 `<discovered:NAME>` 占位。
- 临时目录 `E:/tmp/w67-rehearsal/` 只放日志与临时 profile，**不在仓库内**（残留清单见 §8）。

### 1.2 两态与「假绿」判据

- **假绿** = `dry-run rc=0`，但输出里**没有该场景所需夹具的预置行**（magic 夹具行形如 `[csharp] 魔法夹具（dry-run 不连库）…`）。
- 需要夹具的场景按两态跑：
  - **magic 类**（`spell_cast`）：默认 `--profile alt` vs `--profile E:/tmp/w67-rehearsal/profile-magic.json`（= alt + 顶层 `"magic": [{"magic_id":1,"level":0,"use_key":"1","curr_train":0}]`，与场景描述逐字一致）。
  - **inventory 类**（`bag_equip` / `eat_item`）：默认 vs「对应段」。**当前没有 profile 级 inventory 段**（`card_window.py` 只读场景顶层 `inventory`；profile 顶层未知键被静默忽略）。因此第二态用**仓库外的场景临时副本**（原文件 + `inventory` 段，仓库内场景未改）跑，见 §3.2。

### 1.3 需要 `--var` 的变量与来源（不是猜值）

| 变量 | 来源（工具口径） | 出处 |
|---|---|---|
| `npc_actor` | 优先 `SM_MERCHANTSAY(643).recog`；无 643 时退化为 `SM_TURN(10)` 首个 `recog != self`（**规则**） | `tools/probe/param_discovery.py` 表 |
| `merchant_actor` | `SM_MERCHANTSAY(643).recog`（推导） | 同上；`PlayObject.Message.cs:1721-1724` |
| `king_actor` | `SM_MERCHANTSAY(643).recog`（**部署假设**：发现 run 点中的是国王，`kind=rule`） | 同上；643 不证明「是国王」 |
| `self_x` / `self_y` | `SM_LOGON(50)` 的 `param` / `tag` | `PlayObject.cs:1128-1131` |
| `move_x`/`move_y`/`move_xy` | 场景 `computed`：`self_x+1` / `self_y+0`，再 `pack` 成 `(y<<16)\|x` | `move_walk.json`、`param_discovery.py` |
| `make_index`/`item_name`/`takeon_where` | 同一条 `SM_BAGITEMS(201)` 里**同一个物品槽**（`MakeIndex`/`Name`/`StdMode→ItemLocation`） | `param_discovery.py` 201 规则族 |

> `card_window` 对 `required` 场景会自动先跑 discovery（默认 `npc_click_auto.json`）再注入 `--var`；**单独跑 `mir2_probe --check-scenario` 时不会**，所以下表 `--check-scenario` 列出现 `2 → 0` 是正常的「缺变量 → 补 `--var` 后合法」。

---

## 2. 主表：20 个场景的实测 rc

**列说明**：

- `check rc`：`0` = 无变量依赖；`2 → 0` = 裸跑 rc=2（缺 `required` 变量）、按 §1.3 补 `--var` 后 rc=0。
- `dry rc（默认）` = `--profile alt`（不显式给 profile 时的默认）。
- `dry rc（带 profile）`：`0ᵐ` = magic profile 且**有**魔法夹具预览行；`0ᵛ` = 场景级 inventory 临时副本（rc=0 但**无**预览行）；`0ᵃ` = 无夹具段（带 magic profile 与默认完全相同）。
- `2ᵇ` = 场景自声明 `blocked`，`card_window` 在建 out/起栈前就拒。
- 「假绿」= rc=0 但没有夹具行 ⇒ **不能当作窗口可跑的证据**。

| 场景 | `--check-scenario` rc | `--dry-run` rc（默认 alt） | `--dry-run` rc（带 profile） | 阻塞原因 / 真机前置 | 承接卡 | 依赖的 R 卡（详见 §4） |
|---|---|---|---|---|---|---|
| `bag_equip` | 2 → 0（`make_index`/`takeon_where`/`item_name`） | **0（假绿）** | 0ᵛ（**仍无夹具预览行**） | 缺 inventory 夹具**且无可用路径**（probe schema 拒 `inventory`）；201 发现无真实样本 | `openmir2-window-inventory-fixture`（已合 `bb4317a01`）；**建议新卡** `openmir2-probe-scenario-inventory-key`（未开） | 1：`delmagic-addmagic-chain` |
| `combat_hit` | 0 | **2ᵇ BLOCKED** | 2ᵇ | `blocked`：无同屏怪物夹具 + `wait_for` 无 recog/来源过滤 ⇒ 31/14 不可归因 | **建议新卡** `openmir2-combat-hit-monster-fixture`（未开）；recog 过滤已由 `openmir2-probe-scenario-validation-gaps` 合入 | 12：`attacktarget-overrides`、`login-numeric-gaps`、`merchant-race-bias`、`monster-attack-hit-frame`、`proper-target-unfolded`、`rm-delaypushed`、`rm-refmessage-delay-layer`、`run-gap-mb3`、`run-gap-monster-specials`、`run-gap-pl1`、`superman-normnpc`、`weapon-durability-frames` |
| `combat_strike` | 0 | **2ᵇ BLOCKED** | 2ᵇ | `blocked`：无 `DC_min≥36..60` 且 `HitPoint≥17..34` 的配装，且 `wait_for` 无 recog 过滤 | **建议新卡** `openmir2-combat-strike-gear-fixture`（未开）；影响 R3 的 13 张 | 13：见 §4.3（R3 全组） |
| `dice_chain` | 0 | 0 | 0ᵃ | 赌场 NPC；**`params.npc_actor=119` 是 r9 抓包值、每会话不同且不在 `required`** ⇒ 开窗须 `--var npc_actor=<发现值>` | 无（场景本身可跑；建议改用 `dice_chain_auto`） | 2：`script-playdice-openybdeal`、`script-take-persist`；另有 T-211 |
| `dice_chain_auto` | 0 | 0 | 0ᵃ | 赌场 NPC（actor 自动发现） | — | 2：`menu-ok-unify-pickwire`、`ya52-menu-ok-missing` |
| `eat_item` | 2 → 0（`make_index`/`item_name`） | **0（假绿）** | 0ᵛ（**仍无夹具预览行**） | 同 `bag_equip`（inventory 路径不可用 + 201 无样本） | 同 `bag_equip` | 1：`unbindlist-consumer`；另有 T-211 |
| `equip_struck_save` | 0 | 0 | 0ᵃ | 商店 NPC「卫家店」(0103,19,18) 摆位；**场景 description 仍写「Rust 未接 642」（已过时）**；R 级真机窗口未授权 | `openmir2-persist-useitems-wiring`（T-247）；`openmir2-attack-weapon-wear-642`（T-266）；文字订正 `openmir2-642-stale-text-sweep`（在办） | 2：`persist-useitems-wiring`、`attack-weapon-wear-642` |
| `gm_batch` | 0 | 0 | 0ᵃ | GM 命令权限（角色 Permission） | — | 1：`cm-adjust-bonus` |
| `gm_time` | 0 | 0 | 0ᵃ | 无 | — | 0 |
| `king_castle_war` | 2 → 0（`king_actor`） | 0 | 0ᵃ | `king_actor` 是**部署假设**（643 不证明是国王）；城堡战 UnderWar 语境未备（`castle_zone` 无场景文件） | `openmir2-t138-request-castle-war` | 1：`t138-request-castle-war` |
| `king_dialog` | 2 → 0（`king_actor`） | 0 | 0ᵃ | 同 `king_actor` 假设；需角色站位靠国王 | 无（`king_actor` 发现已合） | 0（方法见 §4） |
| `login_only` | 0 | 0 | 0ᵃ | 角色预置位（场景未声明 `placement`，用 DB 现值） | — | 34：见 §4.12 |
| `merchant_dialog` | 2 → 0（`merchant_actor`） | 0 | 0ᵃ | 商人 NPC + 角色站位 | — | 3：`encoding-core-e1-e2`、`market-def-impl`、`sm-merchantsay-recog`；另有 T-211 |
| `move_walk` | 2 → 0（`move_x`/`move_y`/`move_xy`） | 0 | 0ᵃ | 目标格必须可走（`computed` 由 SM_LOGON 推） | — | 1：`transmap-maptrigger` |
| `npc_click` | 2 → 0（`npc_actor`） | 0 | 0ᵃ | 角色站位（场景无 `placement`；`npc_actor` 走 643 发现） | — | 5：`click-path-bag-writeback`、`encoding-core-e1-e2`、`npc-def-loading`、`npc-display-table`、`sm-merchantsay-recog`；另有 T-211 |
| `npc_click_auto` | 0 | 0 | 0ᵃ | **它是所有 `required` 场景的 discovery 载体**（点最近 NPC 取 643） | — | 0（但被 §2 中所有 `2 → 0` 行间接依赖） |
| `npc_script_chain` | 2 → 0（`npc_actor`） | 0 | 0ᵃ | 场景自带 `placement` (0122,29,33)（贴着比奇国王） | — | 7：`action-line-variable-text`、`guild-official-npc`、`npc-def-loading`、`pickwire-remaining-frames`、`run-gap-pl2`、`run-gap-pl3`、`run-gap-pl4`；另有 T-211 |
| `pickup_item` | 2 → 0（`self_x`/`self_y`） | 0 | 0ᵃ | 同格必须有可拾取地物（否则 200/201 不来） | — | 0 |
| `spell_cast` | 0 | **0（假绿）** | **0ᵐ（magic profile，2 行夹具预览）** | 场景已解封；真机仍不可跑：Rust 魔法落点/638/639 + 第二观察者未接 | `openmir2-spell-cast-unblock`（已合）、`openmir2-window-magic-fixture`（已合）；Rust 帧面见下条 | 8：`buff-system`、`default-arm-spell-surface`、`healthspell-emission-model`、`magic-8-21-wiring`、`mb8-9-monsters`、`recall-slave-shift-live`、`spell-mp-wrapper`、`spell-shared-tail`；另有 T-228/230/233/234/260 |
| `upgrade_retrieve` | 2 → 0（`merchant_actor`） | 0 | 0ᵃ | 商店 NPC 摆位（沙巴克铁匠 (332,317) 西南邻格 (320,328)）；**武器在手 / 金币≥10000 / 背包有黑铁矿**（缺一第 9 步 `wait_idents 709` 必红） | `openmir2-upgrade-npc`（T-95） | 4：`a09-upgrade-item`、`outbound-encoders-red-batch`、`upgrade-npc`、`weapon-upgrade-values` |

**计数核对**：上表「离线绿」= `check` 0（或 2→0）且 `dry` 0 且非假绿：`dice_chain`、`dice_chain_auto`、`equip_struck_save`、`gm_batch`、`gm_time`、`king_castle_war`、`king_dialog`、`login_only`、`merchant_dialog`、`move_walk`、`npc_click`、`npc_click_auto`、`npc_script_chain`、`pickup_item`、`upgrade_retrieve` = **15 个**。与 §5 引用的枚举 **10 行 / 15 个场景** 一致。

---

## 3. 假绿证据（命令 + 输出片段）

> 所有日志在 `E:/tmp/w67-rehearsal/logs/`（本机 temp，不入库）。下列 rc 均为真实退出码。

### 3.1 `spell_cast`：默认 profile 假绿，magic profile 才真

```sh
python -I -B tools/conformance/card_window.py --dry-run \
  --scenario tools/probe/scenarios/spell_cast.json \
  --card openmir2-window-batch-rehearsal --out E:/tmp/w67-rehearsal/out/spell_cast
# rc=0；grep -c '魔法夹具' → 0（零夹具预览行 = 假绿）
```

对照（`profile-magic.json` = 内置 alt + 顶层 magic 段）：

```sh
python -I -B tools/conformance/card_window.py --dry-run \
  --scenario tools/probe/scenarios/spell_cast.json \
  --profile E:/tmp/w67-rehearsal/profile-magic.json \
  --card openmir2-window-batch-rehearsal --out E:/tmp/w67-rehearsal/out/spell_cast
# rc=0；grep -c '魔法夹具' → 2
```

输出片段（magic profile）：

```text
[csharp] 魔法夹具（dry-run 不连库）：db=mir2_test_db magic_ids=1
[rust]   魔法夹具（dry-run 不连库）：db=mir2_test_db_r magic_ids=1
```

⇒ **默认 alt 的 rc=0 不构成窗口证据**；场景 description 已写死「未声明 magic 夹具则 fail-closed 超时」。真机另有 Rust 侧缺口（见 §2 该行），所以本场景仍**不在**「今天就能进窗口」清单里。

### 3.2 `bag_equip` / `eat_item`：inventory 夹具「有实现、无通路」

证据链（每条都是实跑）：

1. **默认态 rc=0 但无夹具行**（假绿）：
   ```sh
   python -I -B tools/conformance/card_window.py --dry-run \
     --scenario tools/probe/scenarios/bag_equip.json \
     --card openmir2-window-batch-rehearsal --out E:/tmp/w67-rehearsal/out/bag_equip
   # rc=0；grep -c '背包夹具' → 0
   ```
2. **profile 级 `inventory` 段被静默忽略**：用 `profile-inventory.json`（alt + 顶层 `"inventory": [...]`）再跑，**rc=0、`grep -c inventory` → 0**，没有任何警告。`card_window.load_profile` 不校验顶层未知键，也不读取 profile 级 inventory。
3. **场景级 `inventory` 会被 `card_window` 接受**：把 `bag_equip.json` 复制到仓库外并加 `"inventory": [{"index":100,"make_index":5001,"position":3}]`，`card_window --dry-run` **rc=0**；但**仍然 `grep -c '背包夹具'` → 0** —— dry-run 的预览分支只覆盖 `placement` 与 `magic`（`card_window.py` 的 dry-run 段），**不预览 inventory**。
4. **同一个临时副本喂给 probe 直接 rc=2**：
   ```sh
   python -I -B tools/probe/mir2_probe.py --check-scenario \
     --scenario E:/tmp/w67-rehearsal/bag_equip-with-inventory.json \
     --var make_index=5001 --var takeon_where=3 --var item_name=木剑
   # rc=2
   # scenario-error: scenario has unknown key(s): ['inventory'] (fail-closed)
   ```
   `mir2_probe.load_scenario` 的顶层可选项只有 `description/params/required/computed/placement/blocked`（`tools/probe/mir2_probe.py:528-530`），**没有 `inventory`**；而真跑同样走 `load_scenario` ⇒ 并非只有 check 会红。
5. **`card_window` 自身的 inventory 形状校验是有效的**（对照实验）：把临时副本的 `make_index` 删掉，`card_window --dry-run` **rc=2**：
   ```text
   card_window.py: …bag_equip-bad-inv.json: scenario.inventory[0] 缺必填键 ['make_index']
   ```
   ⇒ 机制代码本身在，只是**声明进不去 probe**、**dry-run 看不见**。

**结论**：`bag_equip`/`eat_item` 当前**没有任何受支持的夹具路径**；batch-plan §7.4 写的「或先在场景里补 `inventory`」在本 head 上**不可行**，必须先给 probe 顶层 schema 加键（建议卡 `openmir2-probe-scenario-inventory-key`，未开卡）。另有一个独立前置：现有 798 份录制里**一条 `SM_BAGITEMS(201)` 都没有**（`docs/window-runbook-20261023.md:376`），`make_index`/`item_name`/`takeon_where` 的 discovery 在真机没有样本 ⇒ 需要非空背包的一次 discovery/DB 夹具。

### 3.3 其他 rc=2（都点明原因）

- `--check-scenario` 的 10 个 `2 → 0`（`bag_equip`/`eat_item`/`king_castle_war`/`king_dialog`/`merchant_dialog`/`move_walk`/`npc_click`/`npc_script_chain`/`pickup_item`/`upgrade_retrieve`）：全部是**缺 `required` 变量**（工具原文 `refusing to send guessed values`），补 §1.3 来源的 `--var` 后 rc=0；`card_window` 会先跑 discovery 注入。**承接卡 = `openmir2-window-param-discovery`（已合、已 closed；`param_discovery.py` 的来源表见 §1.3）**。**不是场景损坏**。
- `combat_hit` / `combat_strike`：`card_window` 原文 `scenario is BLOCKED: …` + `VERDICT: BLOCKED`，rc=2，且**不预览命令序列**（设计如此）。

---

## 4. 逐场景 R 卡交叉引用

**方法（可复算）**：

1. **窗口判据登记**：`mir2-rs/tests/parity/whitelist.md` 中行首 `| T-…` 的行，按 `` `场景名` ``（带反引号、整词）精确匹配；取行内「卡 \`openmir2-…\`」作为 R 卡。这是「窗口判据里点名该场景」的最强证据。
2. **批次归属**：`docs/window-batch-plan-20261023.md` §1 的 R 卡表（列 = 卡 / 窗口项 / 场景 / 备注）+ `docs/window-debt-reclass-20261023.md` §R1..R9 分组。
3. **卡线程**：扫 `refs/collab/inbox/*` 全部 7779 条 entry 的 body（`git cat-file --batch`）搜场景名，作为补充（未做 tier 过滤，噪声大，**只作线索**）。

> **未穷举声明**：① 卡线程侧只按场景名全文匹配，未按 tier 过滤；② `docs/` 里大量「泛引用」（例如 T-211 一行点了多个场景）会被计入；③ 行数会随 whitelist 增行漂移（本表快照 = `8dcb8c8ca`）。

### 4.1 `bag_equip` / 4.2 `eat_item`

- `bag_equip`：`delmagic-addmagic-chain`（plan §1 表）；T 行精确 0（T-264 是 closeout 汇总行，泛引用）。
- `eat_item`：`unbindlist-consumer`（plan）；T 行精确 0（T-211 泛引用）。
- 两者是「一切需要背包内容的场景」的底座，plan §5.2 把 `delmagic-addmagic-chain`、`unbindlist-consumer` 与「一切 bag_equip/eat_item 场景」并列。

### 4.3 `combat_strike`

R3 全组 **13 张**（`docs/window-debt-reclass-20261023.md` §R3）：`merchant-race-bias`、`superman-normnpc`、`mb8-9-monsters`、`monster-attack-hit-frame`、`run-gap-pl1`、`attacktarget-overrides`、`proper-target-unfolded`、`rm-refmessage-delay-layer`、`run-gap-mb3`、`run-gap-mb1`、`run-gap-mb4`、`run-gap-monster-specials`、`run-gap-monster-master-follow`（各带 `openmir2-` 前缀）。⇒ 该场景一天不解封，这 13 张的真机判据全部无法开窗。

### 4.4 `combat_hit`

12 张（plan §1 表）+ T 行 5 条（T-242/249/251/253/255）：`attacktarget-overrides`、`login-numeric-gaps`、`merchant-race-bias`、`monster-attack-hit-frame`、`proper-target-unfolded`、`rm-delaypushed`、`rm-refmessage-delay-layer`、`run-gap-mb3`、`run-gap-monster-specials`、`run-gap-pl1`、`superman-normnpc`、`weapon-durability-frames`。

### 4.5 `dice_chain` / 4.6 `dice_chain_auto`

- `dice_chain`：`script-playdice-openybdeal`、`script-take-persist`（plan）；T-211（泛）。
- `dice_chain_auto`：`menu-ok-unify-pickwire`、`ya52-menu-ok-missing`；T-193、T-194。

### 4.7 `equip_struck_save`

`persist-useitems-wiring`（T-247）、`attack-weapon-wear-642`（T-266，R 级真机窗口未采）。注意 T-247 正文仍写「Rust 642 未接」，已被 T-266/`8dcb8c8ca` 取代。

### 4.8 `gm_batch` / 4.9 `gm_time`

- `gm_batch`：`cm-adjust-bonus`（plan）；T 行精确 0。
- `gm_time`：0。

### 4.10 `king_castle_war` / 4.11 `king_dialog`

- `king_castle_war`：`t138-request-castle-war`（plan）；T 行精确 0。
- `king_dialog`：plan 表 0；卡线程线索中有 `npc-def-loading`、`openybdeal-host-wiring`，但均未把 `king_dialog` 写进自己的窗口项 ⇒ 本表记 0（未穷举）。

### 4.12 `login_only`

**34 张**（plan §1 表列；按 T 行精确匹配为 12 条）：`aoi-verify-maptime`、`ctor-barrier-a-draw`、`ctor-draws-fold`、`db-encoding`、`enter-shift-loop`、`gamesvr-serverinfo-report`、`gm-impl-shutup`、`gm-script-host-seam`、`guard-ctor-constants`、`guild-g2-dispatch`、`loader-fields`、`login-529-status-text`、`login-654-order`、`login-charstatus-657`、`login-frame-order-52-752`、`login-notice-clienttick`、`login-numeric-gaps`、`login-useitems-magic`、`mapcell-attributes-loading`、`npc-ctor-constants`、`npc-display-table`、`persist-create-rows`、`persist-item-attr-and-gold`、`persist-single-sided-columns`、`player-searchviewrange-cadence`、`save-misc-gaps`、`save-on-shutdown`、`searchtick-origin-parity`、`sm-additem-tail`、`sm-updateitem`、`smframe-minor-gaps`、`verify-tick-base`、`window-collector-preflight`、`world-tick-expansion`（均 `openmir2-` 前缀）。T 行：T-211/232/235/236/246/248/250/256/257/258/259/261。

### 4.13 `merchant_dialog` / 4.14 `move_walk`

- `merchant_dialog`：`encoding-core-e1-e2`、`market-def-impl`、`sm-merchantsay-recog`；T-211。
- `move_walk`：`transmap-maptrigger`；T 行精确 0。

### 4.15 `npc_click` / 4.16 `npc_click_auto`

- `npc_click`：`click-path-bag-writeback`、`encoding-core-e1-e2`、`npc-def-loading`、`npc-display-table`、`sm-merchantsay-recog`；T-211。
- `npc_click_auto`：自身没有卡把它当窗口判据；但它是**所有 `required` 场景的 discovery 载体**（643/SM_TURN 来源），属于基础设施。

### 4.17 `npc_script_chain`

7 张：`action-line-variable-text`、`guild-official-npc`、`npc-def-loading`、`pickwire-remaining-frames`、`run-gap-pl2`、`run-gap-pl3`、`run-gap-pl4`；T-211。

### 4.18 `pickup_item`

0（plan 表与 T 行都没有把 `pickup_item` 写进窗口项；仅泛引用）。

### 4.19 `spell_cast`

8 张（plan §5.3 R5 + §1 表）：`buff-system`、`default-arm-spell-surface`、`healthspell-emission-model`、`magic-8-21-wiring`、`mb8-9-monsters`、`recall-slave-shift-live`、`spell-mp-wrapper`、`spell-shared-tail`；T 行 5 条 T-228/230/233/234/260。

### 4.20 `upgrade_retrieve`

4 张（R2）：`a09-upgrade-item`、`outbound-encoders-red-batch`、`upgrade-npc`、`weapon-upgrade-values`。T 行精确 0（T-95 是 `upgrade-npc` 自己的登记，正文未写场景名）。

---

## 5. 与 batch-plan 计数对照（差异逐条）

**引用口径**：见 `docs/window-batch-plan-20261023.md` **§7.4 @ `6ceb0689d`**（drift-sweep v5；Leader 裁决见 drift-sweep 线程 status `eb667db0`）——**枚举 10 行 / 15 个场景**，禁 `18−N`。本次实跑表与它**一致**（15 个离线绿、2 个 blocked、3 个不可信）。以下是必须写清的差异/订正：

1. **master 文本仍写 14，不能直接引用**。master `8dcb8c8ca` 的 §7.4 写「18 − 4 = 14」，§7.2/§7.3 还写「642 未接 / inventory 夹具未合」——这些是 drift-sweep 之前的旧文本；同一份 master 的 §7.1 又已写 642 接线，**文件内部自相矛盾**。v5（`6ceb0689d`，patch `8b810c894`，status `cfc96f1b`）尚未合入（`git merge-base --is-ancestor 6ceb0689d HEAD` ⇒ rc=1）。**开窗计数以 v5 为准；master 文本等 v5 合并后再读**。
2. **派单写「§7.5」是笔误**：v5 的逐行枚举在 **§7.4**；§7.5 是「判据口径」（v5 已把 preflight 改成「已合入 `2e3b8500a`」）。本文件按 §7.4 引用。
3. **v5 给 `bag_equip`/`eat_item` 的补救路径不可行**：v5 写「或先在场景里补 `inventory`」，但实测 probe 顶层 schema 拒 `inventory`（§3.2 第 4 条，rc=2）⇒ 必须先改 probe schema（建议卡）。这条应回写 batch-plan，但该文件被 `openmir2-docs-state-drift-sweep` 持有，**本卡不改**。
4. **`dice_chain` 的「引用 T 行 = 3」是子串计数**：whitelist 里带反引号精确点名 `dice_chain` 的只有 T-211 一行；3 = T-211 + T-193 + T-194，其中 T-193/T-194 点名的是 **`dice_chain_auto`**（`dice_chain` 是其子串）。同理 `login_only` 在 master §7.4 写 14，本 head 精确匹配为 **12**（T-211/232/235/236/246/248/250/256/257/258/259/261），宽匹配 16。`equip_struck_save` 在 master §7.4 写 1（T-247），本 head 已是 **2**（T-247 + T-266，642 接线后新增）。这些是**登记行漂移**，不影响「能跑」判定，但 v5 合入时值得顺手重算。
5. **v5 把 `equip_struck_save` 计入「能跑」是正确**（对 `8dcb8c8ca` 核过）：`combat.rs:767 wear_durability` + `:1615-1619` 的 `RM_DURACHANGE` 已接，T-266 记 `window_required=true`。本次 dry-run rc=0。**但「能跑」≠「已验收」**：R 级真机窗口未采，且场景 description 仍是旧文（见 §2 `equip_struck_save` 行与 §4.7）。
6. **`npc_click_auto` 在 v5 的 15 场景里，但没有任何 R 卡把它当窗口判据**：它作为 discovery 载体是必需的，建议在开窗 runbook 里单列为「基础设施」，避免被误当成一条独立判据。

---

## 6. 开窗清单（给 owner）

### 6.1 今天就能进窗口（离线彩排绿；真机仍需 owner 授权与夹具）

| 场景 | 开窗前必须确认的真机前置 |
|---|---|
| `login_only` | 角色 DB 现值可登录；如需固定落点，先给场景补 `placement`（当前无） |
| `npc_click_auto` | 作为 discovery 载体跑（可单独跑，也可被其它场景自动触发） |
| `npc_click` | 角色站位能点到目标 NPC（无 `placement`，靠 DB 现值/手动摆放） |
| `npc_script_chain` | 场景自带 0122 (29,33) 摆位；角色须在比奇国王旁 |
| `merchant_dialog` | 角色站位靠商人 |
| `king_dialog` / `king_castle_war` | `king_actor` 是**部署假设**；`king_castle_war` 还需城堡战语境（UnderWar），否则只验到「未开放」分支 |
| `move_walk` | 相邻目标格可走（由 SM_LOGON 推） |
| `pickup_item` | 脚下有可拾取地物（否则 200/201 不来） |
| `dice_chain_auto` | 赌场 NPC 在场 |
| `dice_chain` | **必须带 `--var npc_actor=<本次发现值>`**（默认 119 是 r9 的会话 id，可能点空） |
| `upgrade_retrieve` | 铁匠摆位 (320,328)+武器在手+金币≥10000+黑铁矿（三项缺一必红） |
| `equip_struck_save` | 「卫家店」(0103,19,18) 摆位；R 级窗口授权；接受 description 旧文 |
| `gm_time` / `gm_batch` | 角色 GM Permission |

### 6.2 还差一步（差什么 / 谁负责）

| 场景 | 差什么 | 谁负责 / 承接 |
|---|---|---|
| `bag_equip` / `eat_item` | ① probe 顶层 schema 收 `inventory`；② `card_window --dry-run` 预览 inventory（否则永远看不出假绿）；③ 非空背包的一次 discovery（201 样本） | 建议卡 `openmir2-probe-scenario-inventory-key`（未开，需 coordinator 派）；现有 `openmir2-window-inventory-fixture` 已合但只解决 card_window 侧 |
| `spell_cast` | Rust 侧魔法落点 + 638/639 生产帧 + 第二观察者 | 场景已由 `openmir2-spell-cast-unblock` 解封、夹具由 `openmir2-window-magic-fixture` 合入；帧面在 `magic-8-21-wiring` / `spell-mp-wrapper` / `default-arm-spell-surface`；第二观察者走 `openmir2-multi-session-probe` |
| `combat_hit` | ① 同屏怪物夹具（保证正前方有目标且可排除其反击）；② `wait_for` 的 recog/来源过滤 | 建议卡 `openmir2-combat-hit-monster-fixture`（未开）；recog 过滤机制已由 `openmir2-probe-scenario-validation-gaps` 合入（场景未用） |
| `combat_strike` | `DC_min≥36..60` 且 `HitPoint≥17..34` 的配装，或 `wait_for` recog 过滤 | 建议卡 `openmir2-combat-strike-gear-fixture`（未开）；解封后一次性解锁 R3 的 13 张 |

> **不要用 rc=0 当窗口证据**：上表 6.2 的 5 个场景里，`bag_equip`/`eat_item`/`spell_cast` 的 dry-run 都是 rc=0。

---

## 7. 开窗时可照抄的命令

```sh
# 0) 先做开窗自查（preflight 已合入 master 2e3b8500a）
python -I tools/goal/goal_gate.py --json
python -I tools/conformance/preflight.py --profile alt

# 1) 复制 magic profile（仓库外；内容 = 内置 alt + 顶层 magic 段）
#    见 §7.1；不建议把 profile 提交进仓库

# 2) 无 required / 无夹具的场景
python -I -B tools/conformance/card_window.py --card <卡 id> \
  --scenario tools/probe/scenarios/login_only.json \
  --profile alt --out E:/tmp/win/login_only

# 3) 有 required 的场景（card_window 自动 discovery）
python -I -B tools/conformance/card_window.py --card <卡 id> \
  --scenario tools/probe/scenarios/npc_script_chain.json \
  --profile alt --out E:/tmp/win/npc_script_chain

# 4) dice_chain：改用 dice_chain_auto（actor 自动发现）——card_window 没有 --var 透传，
#    所以 dice_chain 的默认 actor 119 无法从命令行覆盖；要跑 dice_chain 只能手工改场景 params（红线禁止）
python -I -B tools/conformance/card_window.py --card <卡 id> \
  --scenario tools/probe/scenarios/dice_chain_auto.json \
  --profile alt --out E:/tmp/win/dice_chain_auto

# 5) spell_cast：必须带 magic profile（否则假绿）
python -I -B tools/conformance/card_window.py --card <卡 id> \
  --scenario tools/probe/scenarios/spell_cast.json \
  --profile E:/tmp/win/profile-magic.json --out E:/tmp/win/spell_cast

# 6) 离线单场景校验（不改库、不连网）
python -I -B tools/probe/mir2_probe.py --check-scenario \
  --scenario tools/probe/scenarios/upgrade_retrieve.json --var merchant_actor=119
```

> `card_window --help` 目前**没有 `--var` 参数**；需要手工给 `dice_chain` 指定 actor 时，只能改场景 `params`（被本卡红线禁止）或改用 `dice_chain_auto`。**这是本卡发现的一个工具缺口**：建议在 `card_window` 增加透传 `--var`（建议卡 `openmir2-cardwindow-var-passthrough`，未开）。

### 7.1 magic profile 内容（复用内置 alt，不复制整份）

```json
{
  "format": "card-window-profile/1",
  "name": "alt+magic",
  "magic": [{"magic_id": 1, "level": 0, "use_key": "1", "curr_train": 0}]
}
```

⚠ 该片段**不能单独作为 profile 使用**：`load_profile` 要求完整的 `sides.{csharp,rust}`（`stack/stop/restore/db/binaries`）。本次实跑用的是「`BUILTIN_PROFILES['alt']` 的深拷贝 + 上面的 `magic` 键」，落在 `E:/tmp/w67-rehearsal/profile-magic.json`（temp，未入库）。

---

## 8. 门禁与残留物

- 写集：仅 `docs/window-batch-rehearsal-20261024.md`。
- 门禁（见卡上 patch 条目，附 rc）：`t_reservation_gate check`、冲突标记门、`git merge-tree --write-tree refs/remotes/walgit/master HEAD`。
- 残留物（全部在仓库外，可随时删）：
  - `E:/tmp/w67-rehearsal/logs/`（逐场景两份日志 + magic/inventory 对照日志）
  - `E:/tmp/w67-rehearsal/profile-alt-dump.json`、`profile-magic.json`、`profile-inventory.json`
  - `E:/tmp/w67-rehearsal/bag_equip-with-inventory.json`、`bag_equip-bad-inv.json`、`eat_item-with-inventory.json`（仓库外场景副本，**仓库内场景 JSON 未动**）
  - `E:/tmp/w67-rehearsal/thread-map.txt`、`r-cards-by-scene.txt`（R 卡交叉引用的原始扫描）
  - `E:/tmp/w67-rehearsal/out/` **未创建**（dry-run 不建 out，已核）
- 未起栈、未连库、未占用任何端口；未改卡片状态。
