# 窗口批次彩排（2026-10-24）

- **卡**：`openmir2-window-batch-rehearsal`（tier=L；写集 `docs/`）
- **执行**：`openmir2-worker-67`，worktree `E:/tmp/wt-worker-67`，分支 `card/window-batch-rehearsal`
- **基线**：`walgit/master` = `8dcb8c8ca`（v1/v2 实测 head）。返工 v3 已 rebase 到 **`b3f48ab49`** 并在其上重跑全矩阵（20 场景 × `--check-scenario` 裸跑/补 var + `--dry-run` 默认 profile/带 magic profile，另加 inventory 五条证据链）；两次结果一致，唯一订正是 `combat_hit` 的 `check` 列（`0` → `2 → 0`）。
- **目的**：给 owner 一张开窗当天能**照着逐条跑**的表。原则是**宁少勿假**：跑不了的就写跑不了。
- **红线遵守**：未改任何场景 JSON、未改 `tools/**`、未改 `rules.json`、未改卡片状态；未起栈、未连库、未占端口；全部命令离线。

---

## 0. 结论（先看这五条）

1. **20 个场景的离线彩排结果**：15 个「离线绿」（与 `window-batch-plan` 的 15 场景枚举一致）、2 个 `blocked`（`combat_hit` / `combat_strike`）、2 个「dry-run 绿但不可信（假绿）」（`bag_equip` / `eat_item`）、1 个「默认 profile 加载期 fail-closed」（`spell_cast`：默认 `alt` 下 `--dry-run` **rc=2**，带 `magic` profile 才 rc=0 且 2 行夹具预览）。15 + 2 + 2 + 1 = 20。逐条见 §2。
2. **`spell_cast` 的假绿通道已被工具关掉（已 fail-closed）**：场景顶层声明 `requires: ["magic"]`（`tools/probe/scenarios/spell_cast.json`），`card_window` **加载期**按生效 profile（含 `sides.<side>` 覆盖）**逐侧**核段，缺任一侧的 `magic` ⇒ **rc=2 并点名**（场景 / profile / 缺哪侧 / 场景文件）。实测：`--profile alt` 下 `--dry-run` **rc=2、0 行魔法夹具**；带 `magic` 段的 profile 才 rc=0 且出现两行夹具预览。**「已 fail-closed」不等于「可以真跑」**——真机侧仍不可跑（Rust 魔法帧面 + 第二观察者）。
3. **`bag_equip` / `eat_item` 的 inventory 夹具当前没有可用路径**：`card_window.py` 已支持**场景级** `inventory`（已合 `bb4317a01`），但 `mir2_probe.py` 的顶层 schema 不收该键 ⇒ 一旦在场景里声明，`--check-scenario` 与真跑都 rc=2。**dry-run 对 inventory 也完全不预览**（只预览 placement 和 magic）。⇒ `openmir2-window-inventory-fixture`（已合 `bb4317a01`）产出的夹具**没有可达入口**；由新卡 **`openmir2-inventory-fixture-reachability`**（open）承接。详见 §3.2 与 §5 差异 3。
4. **`dice_chain` 有一个窗口级隐患**：`params.npc_actor = 119` 是 r9 抓包里的赌场 NPC actor id，而 C# 的 ActorId 每会话不同；它又不在 `required` 里 ⇒ `card_window` 不会为它做 discovery。**而 `card_window` 没有 `--var` 透传 ⇒ 119 无法从命令行覆盖**。**开窗时直接跑 `dice_chain_auto`**（actor 自动发现）；要跑 `dice_chain` 本尊只能改场景 `params`（红线禁止，见 §6.3）。
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
- `card_window --dry-run`：只打印完整命令序列，**不起栈、不建 out**（本次全部用的 `--out E:/tmp/w67-rehearsal/out/<scene>` 自始至终没有被创建）。`blocked` 场景在这里 rc=2；`required` 场景会先打印 discovery 命令，`--var` 值以 `<discovered:NAME>` 占位。场景顶层 `requires` 要求的夹具在**生效 profile** 里缺段时同样 **rc=2**（加载期 fail-closed，先于 discovery 与一切预览；与真跑同一条路径）。
- 临时目录 `E:/tmp/w67-rehearsal/` 只放日志与临时 profile，**不在仓库内**（残留清单见 §8）。

### 1.2 两态与「假绿」判据

- **假绿** = `dry-run rc=0`，但输出里**没有该场景所需夹具的预置行**（magic 夹具行形如 `[csharp] 魔法夹具（dry-run 不连库）…`）。
- **`requires` 已把 magic 类的假绿关掉**：场景顶层声明 `requires` 后，缺夹具的 profile 在**加载期**就 **rc=2 并点名**（见 §3.1），不再产生「rc=0 但零夹具行」的假绿。本 head **只有 `spell_cast` 声明了 `requires`**；`bag_equip` / `eat_item` 没声明 ⇒ 它们的假绿通道**仍在**（inventory 夹具无任何可达路径，见 §3.2）。
- 需要夹具的场景按两态跑：
  - **magic 类**（`spell_cast`）：默认 `--profile alt`（实测 **rc=2 fail-closed**）vs `--profile E:/tmp/w67-rehearsal/profile-magic.json`（= alt + 顶层 `"magic": [{"magic_id":1,"level":0,"use_key":"1","curr_train":0}]`，与场景描述逐字一致；实测 **rc=0、2 行夹具预览**）。
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
| `hit_xy` | 场景 `computed.pack {lo: self_x, hi: self_y}`：`CM_HIT(3014)` 的 `recog` 是**攻击者自身格**（不是目标格） | `combat_hit.json` `computed`；`PlayObject.Attack.cs:883`、`PlayObject.Message.cs:1221-1230` |
| `hit_dir` | **不可推导**：无来源帧，只能**操作者显式** `--var` 给；**无发现规则** | `param_discovery.py:20`（表中「不可推导 / 无来源帧」）、`:246`（`UNDERIVABLE["hit_dir"]`） |

> **`hit_dir` 不属于发现能力**：`param_discovery.py` 的 `UNDERIVABLE` 明写 `target_x`/`target_y`/`hit_dir` 为「无来源帧（需操作者显式 `--var`）」。因此 `combat_hit` 的 `hit_dir` 承接写 **「无 / operator-supplied」**，**不是** `openmir2-window-param-discovery`；`hit_xy` 则由 `computed` 从 `self_x`/`self_y` 确定性算出（操作者的选择，不是帧事实）。

> `card_window` 对 `required` 场景会自动先跑 discovery（默认 `npc_click_auto.json`）再注入 `--var`；**单独跑 `mir2_probe --check-scenario` 时不会**，所以下表 `--check-scenario` 列出现 `2 → 0` 是正常的「缺变量 → 补 `--var` 后合法」。

---

## 2. 主表：20 个场景的实测 rc

**列说明**：

- `check rc`：`0` = 无变量依赖；`2 → 0` = 裸跑 rc=2（缺 `required` 变量）、按 §1.3 补 `--var` 后 rc=0。
- `dry rc（默认）` = `--profile alt`（不显式给 profile 时的默认）。`2ʳ` = 场景声明了 `requires` 而该 profile 缺段 ⇒ **加载期 rc=2**（fail-closed，不预览命令序列）。
- `dry rc（带 profile）`：`0ᵐ` = magic profile 且**该场景所需**的魔法夹具预览行存在（`spell_cast`）；`0ᵛ` = 场景级 inventory 临时副本（rc=0 但**无**预览行）；`0ᵃ` = rc=0、无**该场景专属**夹具。⚠ **魔法夹具是 profile 级「全局 DB 预置」**（`card_window.py:3163-3177`：profile 只要声明了 `magic`，就对该侧**每个非 blocked 场景**预览 1 行）⇒ 带 magic profile 时**所有** `0ᵃ` 场景也会多出 2 行「魔法夹具」预览（两侧各 1），**不是**「与默认完全相同」；返工 v3 实测 **18/20** 场景如此（`combat_hit`/`combat_strike` 两个 `2ᵇ` 各 0 行），不影响可跑性判定。**为什么是 18 而不是 20**：`blocked` 硬门（`card_window.py:3079`，返工基线 `b3f48ab49`）在 `--dry-run` 预览循环（`:3146-3147` 起）**之前**就 `return summary`（rc=2、`verdict=BLOCKED`，连命令序列都不预览）；magic 预览在循环内（`:3163-3177`），两个 `blocked` 场景根本走不到它 ⇒ 只有 **18 个非 blocked 场景 × 2 行 = 36 行**，不是 20 × 2 = 40 行。逐场景复跑计数（magic profile，两态各跑一遍）：`bag_equip`/`dice_chain`/`dice_chain_auto`/`eat_item`/`equip_struck_save`/`gm_batch`/`gm_time`/`king_castle_war`/`king_dialog`/`login_only`/`merchant_dialog`/`move_walk`/`npc_click`/`npc_click_auto`/`npc_script_chain`/`pickup_item`/`spell_cast`/`upgrade_retrieve` 各 2 行，`combat_hit`/`combat_strike` 各 0 行；默认 profile 下 19 个场景各 0 行（`combat_hit`/`combat_strike` 两个 `blocked` 也是 0 行），`spell_cast` **加载期 rc=2**（不预览，`2ʳ`）。⚠ 上述 `:3079` / `:3146-3147` / `:3163-3177` 是**返工基线 `b3f48ab49`** 的行号；代码基线 `1f1a69c84`（本卡 head `077b85a12` 未改代码，行号相同）上分别位移到 `:3154`（BLOCKED 硬门）、`:3228`（dry-run 预览循环）、`:3243-3254`（magic 预览），并新增 `requires` 硬门于 `:3170`（见 §3.1）。
- `2ᵇ` = 场景自声明 `blocked`，`card_window` 在建 out/起栈前就拒。
- `2ʳ` = 场景的 `requires` 没被生效 profile 满足（加载期 fail-closed，见 §1.2 / §3.1）；本 head 只有 `spell_cast` 会走到这一格。
- 「假绿」= rc=0 但没有夹具行 ⇒ **不能当作窗口可跑的证据**；声明了 `requires` 的场景不会落到这一格（缺段直接 `2ʳ`）。

| 场景 | `--check-scenario` rc | `--dry-run` rc（默认 alt） | `--dry-run` rc（带 profile） | 阻塞原因 / 真机前置 | 承接卡 | 依赖的 R 卡（详见 §4） |
|---|---|---|---|---|---|---|
| `bag_equip` | 2 → 0（`make_index`/`takeon_where`/`item_name`） | **0（假绿）** | 0ᵛ（**仍无夹具预览行**） | 缺 inventory 夹具**且无可用路径**（probe schema 拒 `inventory`）；201 发现无真实样本 | `openmir2-window-inventory-fixture`（已合 `bb4317a01`）；**可达入口缺口 = `openmir2-inventory-fixture-reachability`**（open）；**建议新卡** `openmir2-probe-scenario-inventory-key`（未开） | 1：`delmagic-addmagic-chain` |
| `combat_hit` | 2 → 0（`hit_xy`/`hit_dir`；**`hit_dir` 需操作者显式给**，见 §1.3） | **2ᵇ BLOCKED** | 2ᵇ | `blocked`：缺同屏怪物夹具 + 伤害夹具（随机护甲 `[AC,2AC-1]` 可能吃成 0、零帧）；`wait_for` 的 recog/来源过滤**已具备**（`8b40443b6`，等待已改 `{"idents": 31, "recog_is": "other"}`；`14`→`31` 缘由见 §6.2）⇒ 不再是「31/14 不可归因」 | **建议新卡** `openmir2-combat-hit-monster-fixture`（未开）；recog/来源过滤**已落地**（`8b40443b6`，selftest **10→11**），原承接卡 `openmir2-probe-waitfor-recog-filter` **已 closed**；仍 blocked 只因缺怪物/装备夹具 | 12：`attacktarget-overrides`、`login-numeric-gaps`、`merchant-race-bias`、`monster-attack-hit-frame`、`proper-target-unfolded`、`rm-delaypushed`、`rm-refmessage-delay-layer`、`run-gap-mb3`、`run-gap-monster-specials`、`run-gap-pl1`、`superman-normnpc`、`weapon-durability-frames` |
| `combat_strike` | 0 | **2ᵇ BLOCKED** | 2ᵇ | `blocked`：无 `DC_min≥36..60` 且 `HitPoint≥17..34` 的配装（伤害门槛夹具）；`wait_for` recog 过滤**已具备**（`8b40443b6`，等待已改 `{"idents": 31, "recog_is": "other"}` ⇒ 不再误认「自己被守卫打」的 `SM_STRUCK(31)`） | **建议新卡** `openmir2-combat-strike-gear-fixture`（未开）；影响 R3 的 13 张 | 13：见 §4.3（R3 全组） |
| `dice_chain` | 0 | 0 | 0ᵃ | 赌场 NPC；**`params.npc_actor=119` 是 r9 抓包值、每会话不同且不在 `required`，而 `card_window` 无 `--var` 透传 ⇒ 会点空** | 无（场景本身可跑；**窗口改用 `dice_chain_auto`**，见 §6.3） | 2：`script-playdice-openybdeal`、`script-take-persist`；另有 T-211 |
| `dice_chain_auto` | 0 | 0 | 0ᵃ | 赌场 NPC（actor 自动发现） | — | 2：`menu-ok-unify-pickwire`、`ya52-menu-ok-missing` |
| `eat_item` | 2 → 0（`make_index`/`item_name`） | **0（假绿）** | 0ᵛ（**仍无夹具预览行**） | 同 `bag_equip`（inventory 路径不可用 + 201 无样本） | 同 `bag_equip` | 1：`unbindlist-consumer`；另有 T-211 |
| `equip_struck_save` | 0 | 0 | 0ᵃ | 商店 NPC「卫家店」(0103,19,18) 摆位；R 级真机窗口未授权 | `openmir2-persist-useitems-wiring`（T-247）；`openmir2-attack-weapon-wear-642`（T-266）；场景 description 曾写「Rust 未接 642」，已由 `openmir2-642-stale-text-sweep`（合入 `061312242`）订正为「已接线、真机窗口未跑」 | 2：`persist-useitems-wiring`、`attack-weapon-wear-642` |
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
| `spell_cast` | 0 | **2ʳ FAIL-CLOSED**（`requires:["magic"]`，缺段即加载期 rc=2） | **0ᵐ（magic profile，2 行夹具预览；rc=0）** | 场景已解封；真机仍不可跑：Rust 魔法落点/638/639 + 第二观察者未接 | `openmir2-spell-cast-unblock`（已合）、`openmir2-window-magic-fixture`（已合）；Rust 帧面见下条 | 8：`buff-system`、`default-arm-spell-surface`、`healthspell-emission-model`、`magic-8-21-wiring`、`mb8-9-monsters`、`recall-slave-shift-live`、`spell-mp-wrapper`、`spell-shared-tail`；另有 T-228/230/233/234/260 |
| `upgrade_retrieve` | 2 → 0（`merchant_actor`） | 0 | 0ᵃ | 商店 NPC 摆位（沙巴克铁匠 (332,317) 西南邻格 (320,328)）；**武器在手 / 金币≥10000 / 背包有黑铁矿**（缺一第 9 步 `wait_idents 709` 必红） | `openmir2-upgrade-npc`（T-95） | 4：`a09-upgrade-item`、`outbound-encoders-red-batch`、`upgrade-npc`、`weapon-upgrade-values` |

**计数核对**：上表「离线绿」= `check` 0（或 2→0）且 `dry` 0 且非假绿：`dice_chain`、`dice_chain_auto`、`equip_struck_save`、`gm_batch`、`gm_time`、`king_castle_war`、`king_dialog`、`login_only`、`merchant_dialog`、`move_walk`、`npc_click`、`npc_click_auto`、`npc_script_chain`、`pickup_item`、`upgrade_retrieve` = **15 个**；另 **2 个假绿**（`bag_equip` / `eat_item`）+ **2 个 `blocked`**（`combat_hit` / `combat_strike`）+ **1 个 `2ʳ` fail-closed**（`spell_cast`）⇒ 15 + 2 + 2 + 1 = 20。与 §5 引用的枚举 **10 行 / 15 个场景** 一致。

---

## 3. 假绿与 fail-closed 证据（命令 + 输出片段）

> 所有日志在 `E:/tmp/w67-rehearsal/logs/`（本机 temp，不入库）。下列 rc 均为真实退出码。

### 3.1 `spell_cast`：默认 profile 已 fail-closed（rc=2），magic profile 才 rc=0

```sh
python -I -B tools/conformance/card_window.py --dry-run \
  --scenario tools/probe/scenarios/spell_cast.json \
  --card openmir2-window-batch-rehearsal --out E:/tmp/w67-rehearsal/out/spell_cast
# rc=2；grep -c '魔法夹具' → 0（加载期 fail-closed；不打印命令序列）
```

实际报错（逐字）：

```text
card_window.py: 场景 `spell_cast` 要求 `magic` 夹具，但 profile `alt` 未声明 `magic` 段（或某侧被覆盖成空）：缺 csharp、rust 侧（场景文件 …/tools/probe/scenarios/spell_cast.json）
```

> **机制（代码基线 `1f1a69c84`；本卡 head `077b85a12` 为 docs-only，行号相同）**：场景顶层 `requires` 由 `scenario_requires`（`card_window.py:1322`）加载并做形状/取值校验（`REQUIRES_KNOWN = ("magic",)`，`:1319`；未知夹具值同样 fail-closed）；`validate_scenario_requires`（`:1356`）在 `:3170` 被调用，按**生效 profile**（含 `sides.<side>` 覆盖）**逐侧**核 `profile_magic`，任一侧为 `None` 即 `ConfigError` ⇒ **rc=2**。调用点位于 `blocked` 硬门之后、建 out / 起栈 / discovery / 写 report 之前 ⇒ **`--dry-run` 与真跑走同一条路径**，不会再出现「dry-run rc=0 但零夹具行」的假绿。**这是工具侧把假绿关掉，不是场景已经可跑**：真机仍缺 Rust 魔法帧面与第二观察者。

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

⇒ **默认 alt 已 rc=2 fail-closed，不构成窗口证据**；带 magic profile 才 rc=0（仅代表离线夹具预览齐了）。场景 description 已写死「未声明 magic 夹具则 fail-closed 超时」。真机另有 Rust 侧缺口（见 §2 该行），所以本场景仍**不在**「今天就能进窗口」清单里。

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

**结论**：`bag_equip`/`eat_item` 当前**没有任何受支持的夹具路径**；batch-plan §7.4 写的「或先在场景里补 `inventory`」在本 head 上**不可行**，必须先给 probe 顶层 schema 加键（建议卡 `openmir2-probe-scenario-inventory-key`，未开卡）。**两条独立证据**（① probe 拒键 rc=2；② profile 顶层 `inventory` 静默忽略）已由审查者 reviewer-10 独立复现；「夹具已合但**无可达入口**」的缺口由新卡 **`openmir2-inventory-fixture-reachability`**（open，coordinator 已开）承接，`card_window --dry-run` 不预览 inventory 的缺口归 card_window 线。另有一个独立前置：现有 798 份录制里**一条 `SM_BAGITEMS(201)` 都没有**（`docs/window-runbook-20261023.md:376`），`make_index`/`item_name`/`takeon_where` 的 discovery 在真机没有样本 ⇒ 需要非空背包的一次 discovery/DB 夹具。

### 3.3 其他 rc=2（都点明原因）

- `--check-scenario` 的 **11 个** `2 → 0`（`bag_equip`/`combat_hit`/`eat_item`/`king_castle_war`/`king_dialog`/`merchant_dialog`/`move_walk`/`npc_click`/`npc_script_chain`/`pickup_item`/`upgrade_retrieve`）：全部是**缺 `required` 变量**（工具原文 `refusing to send guessed values`），补 §1.3 来源的 `--var` 后 rc=0；`card_window` 会先跑 discovery 注入。其中 **10 个**的变量有发现规则，承接卡 = `openmir2-window-param-discovery`（已合、已 closed；来源表见 §1.3）。**`combat_hit` 是例外**：`hit_xy` 由 `computed.pack(self_x,self_y)` 算出，**`hit_dir` 无来源帧、只能操作者显式 `--var`**（`param_discovery.py` 的 `UNDERIVABLE`）⇒ 它的承接写 **「无 / operator-supplied」**，**不归** `openmir2-window-param-discovery`。**都不是场景损坏**。
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

`persist-useitems-wiring`（T-247）、`attack-weapon-wear-642`（T-266，R 级真机窗口未采）。T-247 正文曾写「Rust 642 未接」，该说法已由 T-266/`8dcb8c8ca` 取代，并已由 `openmir2-whitelist-t247-642-text`（合入 `05ca859bb`）在 `mir2-rs/tests/parity/whitelist.md:970` 逐字订正为「642 已接线…真机窗口未跑」。

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

**引用口径**：见 `docs/window-batch-plan-20261023.md` **§7.4 @ `6ceb0689d`**（drift-sweep v5；Leader 裁决见 drift-sweep 线程 status `eb667db0`）——**枚举 10 行 / 15 个场景**，禁 `18−N`。本次实跑表与它**一致**（15 个离线绿、2 个 blocked、2 个假绿不可信、1 个默认 profile fail-closed；`spell_cast` 已因 `requires:["magic"]` 从「假绿」移出、归 fail-closed）。以下是必须写清的差异/订正：

1. **master 文本仍写 14，不能直接引用**。master `8dcb8c8ca` 的 §7.4 写「18 − 4 = 14」，§7.2/§7.3 还写「642 未接 / inventory 夹具未合」——这是 **doc-vs-code 漂移**：代码侧 642 已在 `8dcb8c8ca` 接线（`crates/world/src/combat.rs:767` wear_durability、`:1615-1619` `RM_DURACHANGE ⇒ SM_DURACHANGE(642)`），inventory 夹具已随 `bb4317a01` 合入，但 plan 文本仍旧。**注意**：master §7.1 关于 642 的句子是「`rules.json` 已补登记 642（**本条只影响比较器**）」——它只说比较器补登记，**没有**说 Rust 已接线；真正的缺口是 plan 文本落后于代码（doc-vs-code），不是同一份文件前后打架。v5（`6ceb0689d`，patch `8b810c894`，status `cfc96f1b`）尚未合入（`git merge-base --is-ancestor 6ceb0689d HEAD` ⇒ rc=1）。**开窗计数以 v5 为准；master 文本等 v5 合并后再读**。
2. **派单写「§7.5」是笔误**：v5 的逐行枚举在 **§7.4**；§7.5 是「判据口径」（v5 已把 preflight 改成「已合入 `2e3b8500a`」）。本文件按 §7.4 引用。
3. **v5 给 `bag_equip`/`eat_item` 的补救路径不可行（结论写死）**：v5 写「或先在场景里补 `inventory`」，但在本 head 上是**死路**，两条独立证据：① probe 顶层 schema 不收 `inventory`，`load_scenario` 以 `scenario has unknown key(s): ['inventory'] (fail-closed)` **rc=2 拒**（真跑同走该函数，§3.2 第 4 条）；② profile 顶层 `inventory` 被 `load_profile` **静默忽略**（rc=0、零效果、无警告，§3.2 第 2 条）。⇒ `openmir2-window-inventory-fixture`（已合 `bb4317a01`）产出的夹具**没有可达入口**。承接：新卡 **`openmir2-inventory-fixture-reachability`**（open；前置 `openmir2-scenario-fixture-precondition` 已合入 master `9083f4863`，`requires` 加载期机制已生效——本 head 上 `spell_cast` 因此 fail-closed，而 `bag_equip`/`eat_item` 未声明 `requires`，这处死路不变）。这条应回写 batch-plan，但该文件被 `openmir2-docs-state-drift-sweep` 持有，**本卡不改**。
4. **`dice_chain` 的「引用 T 行 = 3」是子串计数**：whitelist 里带反引号精确点名 `dice_chain` 的只有 T-211 一行；3 = T-211 + T-193 + T-194，其中 T-193/T-194 点名的是 **`dice_chain_auto`**（`dice_chain` 是其子串）。同理 `login_only` 在 master §7.4 写 14，本 head 精确匹配为 **12**（T-211/232/235/236/246/248/250/256/257/258/259/261），宽匹配 16。`equip_struck_save` 按**同一精确整词口径**为 **1**（T-247），与 master §7.4 的 1 一致；宽匹配（路径子串）才得 2（+ T-266，其正文写的是路径 `tools/probe/scenarios/equip_struck_save.json`）。**本文件全程采用精确整词口径，故记 1**。这些是**登记行漂移**，不影响「能跑」判定，但 v5 合入时值得顺手重算。
5. **v5 把 `equip_struck_save` 计入「能跑」是正确**（对 `8dcb8c8ca` 核过）：`combat.rs:767 wear_durability` + `:1615-1619` 的 `RM_DURACHANGE` 已接，T-266 记 `window_required=true`。本次 dry-run rc=0。**但「能跑」≠「已验收」**：R 级真机窗口仍未采；场景 description 的「Rust 未接 642」旧文已由 `061312242` 订正（见 §2 `equip_struck_save` 行与 §4.7），但「已接线」不等于「已对拍」。
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
| `dice_chain` | **不要用本尊**：`card_window` 无 `--var` 透传，默认 119 是 r9 的会话 id ⇒ 会点空。**改用 `dice_chain_auto`**（见 §6.3） |
| `upgrade_retrieve` | 铁匠摆位 (320,328)+武器在手+金币≥10000+黑铁矿（三项缺一必红） |
| `equip_struck_save` | 「卫家店」(0103,19,18) 摆位；R 级窗口授权（description 已由 `061312242` 订正为「已接线、真机窗口未跑」） |
| `gm_time` / `gm_batch` | 角色 GM Permission |

### 6.2 还差一步（差什么 / 谁负责）

| 场景 | 差什么 | 谁负责 / 承接 |
|---|---|---|
| `bag_equip` / `eat_item` | ① probe 顶层 schema 收 `inventory`；② `card_window --dry-run` 预览 inventory（否则永远看不出假绿）；③ 非空背包的一次 discovery（201 样本） | **可达入口缺口 = `openmir2-inventory-fixture-reachability`**（open，coordinator 已开；现有 `openmir2-window-inventory-fixture` 已合但夹具无入口）；另建议卡 `openmir2-probe-scenario-inventory-key`（未开，需 coordinator 派） |
| `spell_cast` | 离线：默认 `alt` 已 **fail-closed（rc=2）**，须带 `magic` profile 才 rc=0 且 2 行夹具预览；真机：Rust 侧魔法落点 + 638/639 生产帧 + 第二观察者 | 场景已由 `openmir2-spell-cast-unblock` 解封、夹具由 `openmir2-window-magic-fixture` 合入；帧面在 **in-progress** 的 `magic-8-21-wiring` / `spell-mp-wrapper` / `default-arm-spell-surface`；**第二观察者仍无人承接**（`openmir2-multi-session-probe` 已 closed `2a5101606`，其 scope 只是 probe 断言、不做第二会话）⇒ 建议 id `openmir2-spell-second-observer`（未开，需 coordinator 派） |
| `combat_hit` | ① 同屏怪物夹具（保证正前方有目标且可排除其反击）；② 伤害夹具（随机护甲 `[AC,2AC-1]` 可能把伤害吃成 0、零帧）。**recog/来源过滤已具备**（`8b40443b6`，selftest 10→11），不再是缺口 | 建议新卡 `openmir2-combat-hit-monster-fixture`（未开）；`openmir2-combat-strike-gear-fixture`（未开）。**过滤已具备（`8b40443b6`）**：`mir2_probe.py:294` `wait_for(..., recog_filter=None)`、`:313` `wait_frames`、`:331` `wait_idents` 均带 `recog_filter`，`:690-719` 新增 `recog` / `recog_is`（互斥；两者都不写 = 旧语义不变）。本场景 `steps[1]` 已由 `wait_for 14` 改为 `{"idents": 31, "recog_is": "other"}`：`SM_HIT(14)` 的 recog 是**攻击者** ActorId、且本方攻击的 14 被 oracle 自过滤（`PlayObject.Message.cs:1433-1437`）⇒ 改等 `SM_STRUCK(31)` 并只认 recog=other，排除「自己被打」的 31。原承接卡 `openmir2-probe-waitfor-recog-filter` **已合入 `8b40443b6` 并 closed**；真正还缺的是夹具，承接为上述两张**未开**卡。**后果**：过滤已具备，仍 blocked 只是因为缺怪物/装备夹具（`--dry-run` 仍 rc=2）；过滤不再是缺口 |
| `combat_strike` | `DC_min≥36..60` 且 `HitPoint≥17..34` 的配装与伤害门槛（`wait_for` recog 过滤**已具备** `8b40443b6`，等待已改 `{"idents": 31, "recog_is": "other"}`） | 建议卡 `openmir2-combat-strike-gear-fixture`（未开）；解封后一次性解锁 R3 的 13 张 |

> **不要用 rc=0 当窗口证据**：上表 6.2 的 5 个场景里，`bag_equip`/`eat_item` 的默认 dry-run 是 rc=0（假绿），`spell_cast` 只有带 `magic` profile 才 rc=0（2 行夹具预览；默认 `alt` 已是 rc=2 fail-closed）——**离线 rc=0 只说明夹具预览齐了，不构成窗口证据**；`combat_hit`/`combat_strike` 是 `blocked` rc=2。

### 6.3 窗口操作提示（照做，不解释）

- **`dice_chain` 会点空 ⇒ 窗口里改用 `dice_chain_auto`**：`card_window` **没有 `--var` 透传**（argparse 列表 `:5640-5667` 无该参数；`--help` 里唯一的 "var" 是 `--discovery-frames` 的说明文字），而 `dice_chain.json` 的 `params.npc_actor` **硬编码为 119**（r9 抓包会话值，C# `ActorId` 每会话不同）⇒ 默认跑是对着本次不存在的 actor 点。`dice_chain_auto` 用 `click_npc="auto:last_turn"` 自动发现，是窗口唯一可用形态。要跑 `dice_chain` 本尊只能改场景 `params`（红线禁止）或等 `card_window` 加 `--var`（建议 id `openmir2-cardwindow-var-passthrough`，未开）。
- **`bag_equip` / `eat_item` 不要开窗**：inventory 夹具在本 head **无任何可达入口**（§3.2 / §5 差异 3），dry-run rc=0 是假绿。
- **`spell_cast` 必须带 magic profile**：`--profile alt` 下已 **fail-closed（rc=2，加载期点名缺 csharp/rust 侧）**，不再是假绿；带 `magic` profile 才 rc=0 且 2 行夹具预览（§3.1）。rc=0 只代表离线夹具预览齐了，**真机仍不可跑**。
- **别把 magic profile 当「只影响 spell_cast」**：profile 一旦声明 `magic`，dry-run 会为**每个非 blocked 场景**预览两侧各 1 行魔法夹具（全局 DB 预置，§2 列说明）；`blocked` 场景没有预览行——硬门在预览循环之前就 rc=2 返回，实测 **18/20**（`combat_hit`/`combat_strike` 为 0），别按「20×2=40 行」对账。

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

# 5) spell_cast：必须带 magic profile（否则加载期 rc=2 并点名缺段；已 fail-closed）
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
- 返工 v3 的复跑日志：`E:/tmp/w67-rehearsal/rerun-v3/`（`check.tsv`、`dry.tsv` + 每场景 `.log`；仍在仓库外）。
- 订正卡（`openmir2-rehearsal-spellcast-classification`）复跑日志：`E:/tmp/w63-spell/`（`alt.tsv`、`magic.tsv` + `logs/<scene>.<alt|magic>.log`；仓库外，可随时删）。
- 残留物（全部在仓库外，可随时删）：
  - `E:/tmp/w67-rehearsal/logs/`（逐场景两份日志 + magic/inventory 对照日志）
  - `E:/tmp/w67-rehearsal/profile-alt-dump.json`、`profile-magic.json`、`profile-inventory.json`
  - `E:/tmp/w67-rehearsal/bag_equip-with-inventory.json`、`bag_equip-bad-inv.json`、`eat_item-with-inventory.json`（仓库外场景副本，**仓库内场景 JSON 未动**）
  - `E:/tmp/w67-rehearsal/thread-map.txt`、`r-cards-by-scene.txt`（R 卡交叉引用的原始扫描）
  - `E:/tmp/w67-rehearsal/out/` **未创建**（dry-run 不建 out，已核）
- 未起栈、未连库、未占用任何端口；未改卡片状态。
