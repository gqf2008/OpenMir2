# 窗口批次彩排（2026-10-24）

- **卡**：`openmir2-rehearsal-refresh-478ff9a33`（tier=L；写集 `docs/`）
- **执行**：`openmir2-worker-58`，worktree `E:/tmp/wt-worker-58`，分支 `card/rehearsal-refresh`
- **基线**：`walgit/master` = **`c4b66ddb1`**（本次全矩阵重跑的 head；返工从 `47fd96ca8` rebase 到它）。本表最初在 `b3f48ab49` 时代测过；此后 `spell_cast` 走完「假绿 → fail-closed → 可跑」三段、`rules.json` 补到 **45 条**（含 53/638/639）、`card_window` 增加 `--var` 透传，`c4b66ddb1` 又把 **inventory 夹具接通**（probe 收键 + `card_window` 出预览 + `bag_equip`/`eat_item` 场景自声明 `inventory`），故本版在 `c4b66ddb1` 上对 **20 场景 × 三态**全部重跑，逐格记录当次 rc 与输出。
- **目的**：给 owner 一张开窗当天能**照着逐条跑**的表。原则是**宁少勿假**：跑不了的就写跑不了。**本文件是唯一的窗口执行清单**（batch-plan §7.4 与 needs-human 的决策项都靠互链，不各自复制一套命令）。
- **红线遵守**：未改任何场景 JSON、未改 `tools/**`、未改 `rules.json`、未改 `whitelist.md`、未改 batch-plan；未起栈、未连库、未占端口；全部命令离线。

> ⚠ **本卡更新的是「前置与分类」，不是验收结论。** `--dry-run` 绿（rc=0）**≠ 真机已对拍**：它只证明「命令序列可生成、该场景所需夹具能预览出来」。真机证据只能来自开窗当天的双栈实跑与比较器报告。

---

## 0. 结论（先看这五条）

1. **20 个场景的离线彩排结果：18 个「可跑」、2 个 `blocked`（`combat_hit` / `combat_strike`）、0 个假绿。** 18 + 2 = 20。**假绿清零**是因为 `c4b66ddb1` 把 inventory 夹具接通：`bag_equip` / `eat_item` 现在 `--dry-run` rc=0 **且两侧各 1 行**「背包夹具…」预览。**但 batch-plan §7.4 目前仍写 11 行 / 16 个场景**（把这两条列在「仍不可跑」），已滞后于实测 ⇒ 需要另一张卡把它 **16 → 18**（本卡不改 batch-plan，已在卡上 comment 点名）。逐条见 §2。
2. **`spell_cast` 已从 fail-closed 变成「可跑」，但三态必须写全**：默认 `--profile alt` 下 `--dry-run` **rc=2**（场景 `requires:["magic"]`，加载期点名缺 csharp/rust 两侧）→ 带 `magic` 段 profile **rc=0、2 行魔法夹具预览** → 再叠加 D-9 pin（`sides.rust.env.MIR2_CLIENT_ABILITY_LAYOUT="0"`）**rc=0、2 行预览**。**开窗必须用第三种态**：pin 不写，`SM_HEALTHSPELLCHANGED(53)` 两侧 `body_len` 16 vs 0 ⇒ 比较器 fail-closed FAIL。profile 可直接照抄的写法**见 `tools/probe/scenarios/README.md`「窗口 profile 前置：`spell_cast` 必须 pin D-9」与「`requires`」两节**，本文件不重写。**「离线可跑」≠「真机已对拍」**——真机窗口仍未跑。
3. **`bag_equip` / `eat_item` 已脱离假绿（`c4b66ddb1`）**：inventory 夹具现在**有可达入口**——`mir2_probe.py` 顶层 schema 放行 `inventory` 并做形状校验，`card_window.py --dry-run` 走 `preview_inventory`（`:2175` 定义 / `:3440` dry-run 调用 / `:2210` 打印），两个场景 JSON 各自声明 `inventory`。实测 `--dry-run` 默认 `alt` **rc=0、两侧各 1 行**：`bag_equip` 为 `positions=[40] make_index=[5001]`（木剑 StdMode5），`eat_item` 为 `positions=[41] make_index=[5002]`（金创药小量 StdMode0），角色均 `winplay0`。**「预览齐了」≠「真机已对拍」**：真机窗口仍未跑。详见 §3.2。
4. **`dice_chain` 的 actor 隐患仍靠 `--var` 透传收口**：`params.npc_actor = 119` 是 r9 抓包里的赌场 NPC actor id，C# 的 ActorId 每会话不同；它不在 `required` 里 ⇒ **不会有 discovery 值**。`card_window --var npc_actor=<本次 actor id>` 会**原样**透传给两侧 probe（实测 dry-run 命令里出现 4 处 `--var npc_actor=12345`；不传时行为逐字节不变）⇒ 跑 `dice_chain` 时显式给本次 id（取法见 §6.3）；不想手工取 id 仍可直接跑 `dice_chain_auto`。**场景 JSON 不动**。
5. **计数口径**：只看**逐行枚举**，禁止 `18−N` 反推。`docs/window-batch-plan-20261023.md` §7.4 @ `c4b66ddb1` 仍是 **11 行 / 16 个场景**；本次实测是 **18 可跑 / 2 blocked / 0 假绿** ⇒ **§7.4 需由 16 更新到 18**（把 `bag_equip`/`eat_item` 移入「现在就能跑」；那是另一张卡的写集，本卡只在此点名）。差异与订正逐条见 §5。

---

## 1. 口径与命令

### 1.1 两个命令（逐场景实跑）

```sh
python -I -B tools/probe/mir2_probe.py --check-scenario --scenario tools/probe/scenarios/<scene>.json
python -I -B tools/conformance/card_window.py --dry-run --scenario tools/probe/scenarios/<scene>.json \
  --card <建议卡 id> --out <临时目录>
```

- `--check-scenario`：纯加载期校验（不连网）。**rc=0 = 场景形状合法**；**rc=2 = 场景非法或缺必填 `--var`**。`blocked` 是合法形状（仍 rc=0），由 `card_window` 拒。
- `card_window --dry-run`：只打印完整命令序列，**不起栈、不建 out**（本次 `--out E:/tmp/w58-rehearsal/out/<scene>` 自始至终没有被创建，已核）。`blocked` 场景在这里 rc=2；`required` 场景会先打印 discovery 命令，`--var` 值以 `<discovered:NAME>` 占位。场景顶层 `requires` 要求的夹具在**生效 profile** 里缺段时同样 **rc=2**（加载期 fail-closed，先于 discovery 与一切预览；与真跑同一条路径）。
- 临时目录 `E:/tmp/w58-rehearsal/` 只放日志与临时 profile / 场景副本，**不在仓库内**（残留清单见 §8）。

### 1.2 三态与「假绿」判据

场景按「能不能在离线阶段证明夹具齐了」分三态：

- **可跑（18）**：`--check-scenario` rc=0（或补 `--var` 后 rc=0）、`--dry-run` rc=0、且**该场景所需的夹具行有预览**（无夹具需求的场景则不需要预览行）。可跑**只代表离线链路通**。
- **blocked（2）**：场景自声明 `blocked`，`card_window` 在建 out / 起栈前就 `rc=2`、`verdict=BLOCKED`，**连命令序列都不预览**。
- **假绿（当前 0）**：`--dry-run` rc=0，但输出里**没有该场景所需夹具的预置行** ⇒ **不能当作窗口可跑的证据**。`c4b66ddb1` 起 inventory 也出预览行，20 场景中已无假绿；仍保留该判据，防止后续回归。
- **fail-closed**：场景声明的 `requires` 在生效 profile 里缺段 ⇒ 加载期 rc=2 并点名（`spell_cast` 在默认 `alt` 下即此态）。它**不是**假绿，是工具明确拒绝。

夹具三态的具体跑法：

- **magic 类**（`spell_cast`）：必须用**带 `magic` 段且带 D-9 pin** 的 profile。本次三态分别是
  ① 默认 `alt`（实测 **rc=2 fail-closed**）；② `profile-magic.json`（= 内置 alt 深拷贝 + 顶层 `"magic": [{"magic_id":1,"level":0,"use_key":"1","curr_train":0}]`，实测 **rc=0、2 行夹具预览**）；③ `profile-magic-pin.json`（② 再给 `sides.csharp.env` / `sides.rust.env` 各加 `MIR2_CLIENT_ABILITY_LAYOUT="0"`，实测 **rc=0、2 行预览**）。**开窗用 ③**；写法不在此重写，见 README 的两节。
- **inventory 类**（`bag_equip` / `eat_item`）：**场景自描述**——`inventory` 声明在场景顶层，`card_window` 在每侧 restore 之后、起栈之前写进该侧测试库 `characters_bagitem`，`--dry-run` 同样两侧各打印 1 行预览（`preview_inventory`，`card_window.py:2175`）。**不需要 profile**；profile 顶层 `inventory` 仍被静默忽略（不是受支持通道）。详见 §3.2。

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
| `hit_dir` | **不可推导**：无来源帧，只能**操作者显式** `--var` 给；**无发现规则** | `param_discovery.py`（表中「不可推导 / 无来源帧」、`UNDERIVABLE`） |

> **`hit_dir` 不属于发现能力**：`param_discovery.py` 的 `UNDERIVABLE` 明写 `target_x`/`target_y`/`hit_dir` 为「无来源帧（需操作者显式 `--var`）」。因此 `combat_hit` 的 `hit_dir` 承接写 **「无 / operator-supplied」**，**不是** `openmir2-window-param-discovery`；`hit_xy` 则由 `computed` 从 `self_x`/`self_y` 确定性算出（操作者的选择，不是帧事实）。

> `card_window` 对 `required` 场景会自动先跑 discovery（默认 `npc_click_auto.json`）再注入 `--var`；**单独跑 `mir2_probe --check-scenario` 时不会**，所以下表 `--check-scenario` 列出现 `2 → 0` 是正常的「缺变量 → 补 `--var` 后合法」。

---

## 2. 主表：20 个场景的实测 rc（head `c4b66ddb1`）

**列说明**：

- `check rc`：`0` = 无变量依赖；`2 → 0` = 裸跑 rc=2（缺 `required` 变量）、按 §1.3 补 `--var` 后 rc=0。
- `dry rc（默认）` = `--profile alt`（不显式给 profile 时的默认）。
- `dry rc（带所需 profile）`：
  - `0ᵃ` = 场景本身**无需夹具 profile**，以默认 `alt` 为终态（rc=0）。另以 magic profile 复跑也 rc=0，并多出**两侧各 1 行、共 2 行**「魔法夹具」预览——那是 **profile 级全局 DB 预置**，**不代表本场景需要或已具备夹具**。
  - `0ᵐ` = `magic` profile；rc=0 且**该场景所需**的魔法夹具预览 2 行。
  - `0ᵐᵖ` = `magic` + D-9 pin（`sides.rust.env.MIR2_CLIENT_ABILITY_LAYOUT="0"`）；rc=0 且 2 行预览。**这是 `spell_cast` 开窗要用的写法。**
  - `0ⁱ` = 场景**自带 `inventory`**（`bag_equip` / `eat_item`）；rc=0 且两侧各 1 行背包夹具预览（默认 `alt` 即可，无需 profile）。
  - `2ᵇ` = 场景自声明 `blocked`，`card_window` 在建 out/起栈前就拒（`card_window.py:3349-3359`，`verdict=BLOCKED`，不预览命令序列）。
  - `2ʳ` = 场景的 `requires` 没被生效 profile 满足（加载期 fail-closed，`card_window.py:3366` 调用 `validate_scenario_requires`，`REQUIRES_KNOWN` 见 `:1366`）。
- 「假绿」= rc=0 但没有夹具行 ⇒ **不能当作窗口可跑的证据**；声明了 `requires` 的场景不会落到这一格（缺段直接 `2ʳ`）。`c4b66ddb1` 上 **20 个场景已无假绿**（inventory 也出预览行）。

| 场景 | `--check-scenario` rc | `--dry-run` rc（默认 alt） | `--dry-run` rc（带所需 profile） | 阻塞原因 / 真机前置 | 承接卡 | 依赖的 R 卡（详见 §4） |
|---|---|---|---|---|---|---|
| `bag_equip` | 2 → 0（`make_index`/`takeon_where`/`item_name`） | **0** | `0ⁱ`（**两侧各 1 行**背包夹具预览） | 场景自带 `inventory`（木剑 StdMode5，position 40 / make_index 5001）；角色站位用 DB 现值（场景无 `placement`）；三个 `--var` 由 `SM_BAGITEMS(201)` 发现。**真机窗口未跑** | `openmir2-window-inventory-fixture`（已合 `bb4317a01`）；**可达入口已合 `c4b66ddb1`**（`openmir2-inventory-fixture-reachability`） | 1：`delmagic-addmagic-chain` |
| `combat_hit` | 2 → 0（`hit_xy`/`hit_dir`；**`hit_dir` 需操作者显式给**，见 §1.3） | **2ᵇ BLOCKED** | 2ᵇ | `blocked`：缺同屏怪物夹具 + 伤害夹具（随机护甲 `[AC,2AC-1]` 可能吃成 0、零帧）；`wait_for` 的 recog/来源过滤**已具备**（`8b40443b6`，等待已改 `{"idents": 31, "recog_is": "other"}`）⇒ 不再是「31/14 不可归因」 | **建议新卡** `openmir2-combat-hit-monster-fixture`（未开）；recog/来源过滤**已落地**（`8b40443b6`），原承接卡 `openmir2-probe-waitfor-recog-filter` **已 closed**；仍 blocked 只因缺怪物/装备夹具 | 12：`attacktarget-overrides`、`login-numeric-gaps`、`merchant-race-bias`、`monster-attack-hit-frame`、`proper-target-unfolded`、`rm-delaypushed`、`rm-refmessage-delay-layer`、`run-gap-mb3`、`run-gap-monster-specials`、`run-gap-pl1`、`superman-normnpc`、`weapon-durability-frames` |
| `combat_strike` | 0 | **2ᵇ BLOCKED** | 2ᵇ | `blocked`：无 `DC_min≥36..60` 且 `HitPoint≥17..34` 的配装（伤害门槛夹具）；`wait_for` recog 过滤**已具备**（`8b40443b6`，等待已改 `{"idents": 31, "recog_is": "other"}`） | **建议新卡** `openmir2-combat-strike-gear-fixture`（未开）；解封后一次性解锁 R3 的 13 张 | 13：见 §4.3（R3 全组） |
| `dice_chain` | 0 | 0 | `0ᵃ` | 赌场 NPC；**`params.npc_actor=119` 是 r9 抓包值、每会话不同且不在 `required`**。开窗时用 `--var npc_actor=<本次 actor id>` 覆盖；不给则仍按 119 点空 | `openmir2-card-window-var-passthrough`（已交付）；不想手工取 id 仍可用 `dice_chain_auto`（见 §6.3） | 2：`script-playdice-openybdeal`、`script-take-persist`；另有 T-211 |
| `dice_chain_auto` | 0 | 0 | `0ᵃ` | 赌场 NPC（actor 自动发现） | — | 2：`menu-ok-unify-pickwire`、`ya52-menu-ok-missing` |
| `eat_item` | 2 → 0（`make_index`/`item_name`） | **0** | `0ⁱ`（**两侧各 1 行**背包夹具预览） | 场景自带 `inventory`（金创药小量 StdMode0，position 41 / make_index 5002）；角色站位用 DB 现值；两个 `--var` 由 `SM_BAGITEMS(201)` 发现。**真机窗口未跑** | 同 `bag_equip` | 1：`unbindlist-consumer`；另有 T-211 |
| `equip_struck_save` | 0 | 0 | `0ᵃ` | 商店 NPC「卫家店」(0103,19,18) 摆位；R 级真机窗口未授权 | `openmir2-persist-useitems-wiring`（T-247）；`openmir2-attack-weapon-wear-642`（T-266）；场景 description 的旧文已由 `openmir2-642-stale-text-sweep`（`061312242`）订正为「已接线、真机窗口未跑」 | 2：`persist-useitems-wiring`、`attack-weapon-wear-642` |
| `gm_batch` | 0 | 0 | `0ᵃ` | GM 命令权限（角色 Permission） | — | 1：`cm-adjust-bonus` |
| `gm_time` | 0 | 0 | `0ᵃ` | 无 | — | 0 |
| `king_castle_war` | 2 → 0（`king_actor`） | 0 | `0ᵃ` | `king_actor` 是**部署假设**（643 不证明是国王）；城堡战 UnderWar 语境未备（`castle_zone` 无场景文件） | `openmir2-t138-request-castle-war` | 1：`t138-request-castle-war` |
| `king_dialog` | 2 → 0（`king_actor`） | 0 | `0ᵃ` | 同 `king_actor` 假设；需角色站位靠国王 | 无（`king_actor` 发现已合） | 0（方法见 §4） |
| `login_only` | 0 | 0 | `0ᵃ` | 角色预置位（场景未声明 `placement`，用 DB 现值） | — | 34：见 §4.12 |
| `merchant_dialog` | 2 → 0（`merchant_actor`） | 0 | `0ᵃ` | 商人 NPC + 角色站位 | — | 3：`encoding-core-e1-e2`、`market-def-impl`、`sm-merchantsay-recog`；另有 T-211 |
| `move_walk` | 2 → 0（`move_x`/`move_y`/`move_xy`） | 0 | `0ᵃ` | 目标格必须可走（`computed` 由 SM_LOGON 推） | — | 1：`transmap-maptrigger` |
| `npc_click` | 2 → 0（`npc_actor`） | 0 | `0ᵃ` | 角色站位（场景无 `placement`；`npc_actor` 走 643 发现） | — | 5：`click-path-bag-writeback`、`encoding-core-e1-e2`、`npc-def-loading`、`npc-display-table`、`sm-merchantsay-recog`；另有 T-211 |
| `npc_click_auto` | 0 | 0 | `0ᵃ` | **它是所有 `required` 场景的 discovery 载体**（点最近 NPC 取 643） | — | 0（但被 §2 中所有 `2 → 0` 行间接依赖） |
| `npc_script_chain` | 2 → 0（`npc_actor`） | 0 | `0ᵃ` | 场景自带 `placement` (0122,29,33)（贴着比奇国王） | — | 7：`action-line-variable-text`、`guild-official-npc`、`npc-def-loading`、`pickwire-remaining-frames`、`run-gap-pl2`、`run-gap-pl3`、`run-gap-pl4`；另有 T-211 |
| `pickup_item` | 2 → 0（`self_x`/`self_y`） | 0 | `0ᵃ` | 同格必须有可拾取地物（否则 200/201 不来） | — | 0 |
| `spell_cast` | 0 | **2ʳ FAIL-CLOSED**（`requires:["magic"]`，缺段即加载期 rc=2） | **`0ᵐ`（magic，2 行预览）/ `0ᵐᵖ`（magic + pin，2 行预览）** | **开窗 profile 必须同时含 `magic` 段与 `sides.rust.env.MIR2_CLIENT_ABILITY_LAYOUT="0"`（D-9 pin）**；写法照 `tools/probe/scenarios/README.md`「窗口 profile 前置：`spell_cast` 必须 pin D-9」与「`requires`」节。**真机窗口仍未跑**（离线绿 ≠ 已对拍） | `openmir2-spell-cast-unblock`（已合）、`openmir2-window-magic-fixture`（已合）、`openmir2-spell-cast-window-blockers`（`65d6e7134`）、`openmir2-spell-frame-compare-rules`（`89a3d02fd`，补 53/638/639） | 8：`buff-system`、`default-arm-spell-surface`、`healthspell-emission-model`、`magic-8-21-wiring`、`mb8-9-monsters`、`recall-slave-shift-live`、`spell-mp-wrapper`、`spell-shared-tail`；另有 T-228/230/233/234/260 |
| `upgrade_retrieve` | 2 → 0（`merchant_actor`） | 0 | `0ᵃ` | 商店 NPC 摆位（沙巴克铁匠 (332,317) 西南邻格 (320,328)）；**武器在手 / 金币≥10000 / 背包有黑铁矿**（缺一第 9 步 `wait_idents 709` 必红） | `openmir2-upgrade-npc`（T-95） | 4：`a09-upgrade-item`、`outbound-encoders-red-batch`、`upgrade-npc`、`weapon-upgrade-values` |

### 2.1 逐格实跑记录（本次 head，供复核）

| 场景 | check 裸跑 | check 补 `--var` | dry 默认 alt | dry 带 profile | 魔法夹具行 | 背包夹具行 | 判定 |
|---|---:|---:|---:|---:|---:|---:|---|
| `bag_equip` | 2 | 0 | 0 | 0ⁱ（场景自带 `inventory`） | 0（默认）/2（magic 复跑） | **2（默认）** | 可跑 |
| `combat_hit` | 2 | 0 | 2 | 2 | 0 | 0 | **blocked** |
| `combat_strike` | 0 | — | 2 | 2 | 0 | 0 | **blocked** |
| `dice_chain` | 0 | — | 0 | 0ᵃ | 0（默认）/2（magic） | 0 | 可跑 |
| `dice_chain_auto` | 0 | — | 0 | 0ᵃ | 0 / 2 | 0 | 可跑 |
| `eat_item` | 2 | 0 | 0 | 0ⁱ（场景自带 `inventory`） | 0 / 2 | **2（默认）** | 可跑 |
| `equip_struck_save` | 0 | — | 0 | 0ᵃ | 0 / 2 | 0 | 可跑 |
| `gm_batch` | 0 | — | 0 | 0ᵃ | 0 / 2 | 0 | 可跑 |
| `gm_time` | 0 | — | 0 | 0ᵃ | 0 / 2 | 0 | 可跑 |
| `king_castle_war` | 2 | 0 | 0 | 0ᵃ | 0 / 2 | 0 | 可跑 |
| `king_dialog` | 2 | 0 | 0 | 0ᵃ | 0 / 2 | 0 | 可跑 |
| `login_only` | 0 | — | 0 | 0ᵃ | 0 / 2 | 0 | 可跑 |
| `merchant_dialog` | 2 | 0 | 0 | 0ᵃ | 0 / 2 | 0 | 可跑 |
| `move_walk` | 2 | 0 | 0 | 0ᵃ | 0 / 2 | 0 | 可跑 |
| `npc_click` | 2 | 0 | 0 | 0ᵃ | 0 / 2 | 0 | 可跑 |
| `npc_click_auto` | 0 | — | 0 | 0ᵃ | 0 / 2 | 0 | 可跑 |
| `npc_script_chain` | 2 | 0 | 0 | 0ᵃ | 0 / 2 | 0 | 可跑 |
| `pickup_item` | 2 | 0 | 0 | 0ᵃ | 0 / 2 | 0 | 可跑 |
| `spell_cast` | 0 | — | **2ʳ** | **0ᵐ（magic）/ 0ᵐᵖ（magic+pin）** | 2 / 2 | 0 | 可跑（开窗须 pin） |
| `upgrade_retrieve` | 2 | 0 | 0 | 0ᵃ | 0 / 2 | 0 | 可跑 |

**总数**：可跑 **18**、blocked **2**、假绿 **0** = 20。带 magic profile 时 **18/20** 个非 blocked 场景各出 2 行魔法夹具（合计 36 行），两个 `blocked` 各 0 行（硬门在预览循环之前就 rc=2 返回，`card_window.py:3349-3359` 先于 `:3425` 的预览循环，magic 预览在 `:3457` 调用 / `:2252` 发出）；其中 `bag_equip`/`eat_item` 另各有 2 行背包夹具预览。

**计数核对**：上表「可跑」= `check` 0（或 2→0）且 `dry` 0 且非假绿：`bag_equip`、`dice_chain`、`dice_chain_auto`、`eat_item`、`equip_struck_save`、`gm_batch`、`gm_time`、`king_castle_war`、`king_dialog`、`login_only`、`merchant_dialog`、`move_walk`、`npc_click`、`npc_click_auto`、`npc_script_chain`、`pickup_item`、`spell_cast`、`upgrade_retrieve` = **18 个**；另 **2 个 `blocked`**（`combat_hit` / `combat_strike`）⇒ 18 + 2 = 20。**注意**：batch-plan §7.4 仍是 **11 行 / 16 个场景**（把 `bag_equip`/`eat_item` 留在「仍不可跑」）⇒ **§7.4 需从 16 更新到 18**，见 §5。

---

## 3. 夹具预览与 fail-closed 证据（命令 + 输出片段）

> 所有日志在 `E:/tmp/w58-rehearsal/logs/`（本机 temp，不入库）。下列 rc 均为真实退出码。

### 3.1 `spell_cast`：三态（默认 rc=2 / magic rc=0 / magic+pin rc=0）

**态 ①　默认 `alt`：加载期 fail-closed，rc=2。**

```sh
python -I -B tools/conformance/card_window.py --dry-run \
  --scenario tools/probe/scenarios/spell_cast.json \
  --card openmir2-rehearsal-refresh-478ff9a33 --out E:/tmp/w58-rehearsal/out/spell_cast
# rc=2；grep -c '魔法夹具' → 0（不打印命令序列）
```

实际报错（逐字）：

```text
card_window.py: 场景 `spell_cast` 要求 `magic` 夹具，但 profile `alt` 未声明 `magic` 段（或某侧被覆盖成空）：缺 csharp、rust 侧（场景文件 …/tools/probe/scenarios/spell_cast.json）
```

**态 ②　`magic` profile：rc=0、2 行夹具预览。**

```sh
python -I -B tools/conformance/card_window.py --dry-run \
  --scenario tools/probe/scenarios/spell_cast.json \
  --profile E:/tmp/w58-rehearsal/profile-magic.json \
  --card openmir2-rehearsal-refresh-478ff9a33 --out E:/tmp/w58-rehearsal/out/spell_cast
# rc=0；grep -c '魔法夹具' → 2
```

```text
[csharp] 魔法夹具（dry-run 不连库）：db=mir2_test_db magic_ids=1
[rust]   魔法夹具（dry-run 不连库）：db=mir2_test_db_r magic_ids=1
```

**态 ③　`magic` + D-9 pin（**开窗用这个**）：rc=0、2 行夹具预览。**

```sh
python -I -B tools/conformance/card_window.py --dry-run \
  --scenario tools/probe/scenarios/spell_cast.json \
  --profile E:/tmp/w58-rehearsal/profile-magic-pin.json \
  --card openmir2-rehearsal-refresh-478ff9a33 --out E:/tmp/w58-rehearsal/out/spell_cast
# rc=0；grep -c '魔法夹具' → 2
```

**profile 写法不要在这里重写**：`magic` 段与 `sides.<side>.env.MIR2_CLIENT_ABILITY_LAYOUT`（值必须是字符串 `"0"`）的照抄片段、以及「为什么必须 pin」（53 规则按 `body_len` fail-closed），见 **`tools/probe/scenarios/README.md`「窗口 profile 前置：`spell_cast` 必须 pin D-9」**；`requires` 的加载期语义见同文件「`requires`：夹具前置」节。

> **机制（head `47fd96ca8` 行号）**：场景顶层 `requires` 由 `scenario_requires`（`card_window.py:1369`）加载并做形状/取值校验（`REQUIRES_KNOWN = ("magic",)`，`:1366`）；`validate_scenario_requires`（`:1403`）在 **`:3366`** 被调用，按**生效 profile**（含 `sides.<side>` 覆盖）**逐侧**核 `profile_magic`，任一侧为 `None` 即 `ConfigError` ⇒ **rc=2**。调用点位于 `blocked` 硬门（`:3349-3359`）之后、建 out / 起栈 / discovery / 写 report 之前 ⇒ **`--dry-run` 与真跑走同一条路径**，不会再出现「dry-run rc=0 但零夹具行」的假绿。**这是工具侧把假绿关掉，不是场景已经验收**：真机窗口仍未跑（本卡不改变这一点）。

### 3.2 `bag_equip` / `eat_item`：inventory 夹具已可达（`c4b66ddb1`）

证据链（每条都是本次在 `c4b66ddb1` 上实跑）：

1. **场景自声明 `inventory`**：`bag_equip.json` → `{"index":207,"make_index":5001,"position":40,"std_mode":5,"dura":4000,...}`（木剑）；`eat_item.json` → `{"index":3,"make_index":5002,"position":41,"std_mode":0,"ani_count":102,...}`（金创药小量）。声明落在**场景顶层**，`card_window` 在每侧 restore 之后、起栈之前写进该侧测试库 `characters_bagitem`（顺序 restore → placement → inventory → magic → stack）。
2. **`--check-scenario` 不再因该键 rc=2**：裸跑 rc=2 **只是缺 `required` 变量**；补 `--var` 后 **rc=0**（probe 顶层 schema 已放行 `inventory` 并做形状校验）。
3. **`--dry-run` 默认 `alt` rc=0 且两侧各 1 行预览**（本次实测，逐字）：
   ```text
   [csharp] 背包夹具（dry-run 不连库）：db=mir2_test_db   声明 1 件 positions=[40] make_index=[5001] 角色=winplay0
   [rust]   背包夹具（dry-run 不连库）：db=mir2_test_db_r 声明 1 件 positions=[40] make_index=[5001] 角色=winplay0
   ```
   `eat_item` 同理，`positions=[41] make_index=[5002]`。预览由 `preview_inventory`（`card_window.py:2175` 定义、`:3440` dry-run 调用、`:2210` 打印）产生；dry-run 不建 out、不连库。
4. **形状校验两侧同口径（fail-closed）**：把副本的 `make_index` 删掉 ⇒ `card_window --dry-run` **rc=2**（`scenario.inventory[0] 缺必填键 ['make_index']`），`mir2_probe --check-scenario` **rc=2**（`scenario.inventory[0] missing required key(s): ['make_index']`）。
5. **profile 级 `inventory` 仍被静默忽略**（不是受支持通道）：拿不带 `inventory` 的 `login_only` 配 `profile-inventory.json`（alt + 顶层 `inventory`）跑 dry-run ⇒ **rc=0、0 行背包预览**，无警告。**正确通道是场景自描述**，不要写进 profile。

**结论**：`bag_equip`/`eat_item` 在 `c4b66ddb1` 上**已脱离假绿**，按实测移入「可跑」（18 条之一）。真机前置：角色站位用 DB 现值（场景无 `placement`）；三个/两个 `--var` 由 `SM_BAGITEMS(201)` 发现——**夹具在起栈前已把背包写成非空（1 件）**，因此登录期 `CM_QUERYBAGITEMS(81)` 的 201 有样本（旧版「录制里无 201」的缺口由本夹具在窗口期补上）。**「预览齐了」≠「真机已对拍」**：真机窗口仍未跑。

### 3.3 其他 rc=2（都点明原因）

- `--check-scenario` 的 **11 个** `2 → 0`（`bag_equip`/`combat_hit`/`eat_item`/`king_castle_war`/`king_dialog`/`merchant_dialog`/`move_walk`/`npc_click`/`npc_script_chain`/`pickup_item`/`upgrade_retrieve`）：全部是**缺 `required` 变量**（工具原文 `refusing to send guessed values`），补 §1.3 来源的 `--var` 后 rc=0；`card_window` 会先跑 discovery 注入。其中 **10 个**的变量有发现规则，承接卡 = `openmir2-window-param-discovery`（已合、已 closed；来源表见 §1.3）。**`combat_hit` 是例外**：`hit_xy` 由 `computed.pack(self_x,self_y)` 算出，**`hit_dir` 无来源帧、只能操作者显式 `--var`**（`param_discovery.py` 的 `UNDERIVABLE`）⇒ 它的承接写 **「无 / operator-supplied」**，**不归** `openmir2-window-param-discovery`。**都不是场景损坏**。
- `combat_hit` / `combat_strike`：`card_window` 原文 `scenario is BLOCKED: …` + `VERDICT: BLOCKED`，rc=2，且**不预览命令序列**（设计如此）。
- 非法 `--var`：缺 `=` / 空名 / 空值 / 同名不同值 ⇒ 加载期 rc=2 并点名。实测 `--var npc_actor`（缺 `=`）⇒ rc=2：`--var 第 1 个缺少 '='（必须是 NAME=VALUE）：'npc_actor'`。

---

## 4. 逐场景 R 卡交叉引用（快照 `c4b66ddb1`）

**方法（可复算）**：

1. **窗口判据登记**：`mir2-rs/tests/parity/whitelist.md` 中行首 `| T-…` 的行，按 `` `场景名` ``（带反引号、整词）精确匹配；取行内「卡 \`openmir2-…\`」作为 R 卡。这是「窗口判据里点名该场景」的最强证据。
2. **批次归属**：`docs/window-batch-plan-20261023.md` §1 的 R 卡表（列 = 卡 / 窗口项 / 场景 / 备注）+ `docs/window-debt-reclass-20261023.md` §R1..R9 分组。
3. **卡线程**：扫 `refs/collab/inbox/*` 全部 entry 的 body 搜场景名，作为补充（未做 tier 过滤，噪声大，**只作线索**）。

> **未穷举声明**：① 卡线程侧只按场景名全文匹配，未按 tier 过滤；② `docs/` 里大量「泛引用」（例如 T-211 一行点了多个场景）会被计入；③ 行数会随 whitelist 增行漂移（本表快照 = `c4b66ddb1`；本次复核精确整词命中与 `8dcb8c8ca` 快照一致）。

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

`persist-useitems-wiring`（T-247）、`attack-weapon-wear-642`（T-266，R 级真机窗口未采）。T-247 正文曾写「Rust 642 未接」，该说法已由 T-266/`8dcb8c8ca` 取代，并已由 `openmir2-whitelist-t247-642-text`（合入 `05ca859bb`）在 `mir2-rs/tests/parity/whitelist.md` 逐字订正为「642 已接线…真机窗口未跑」。

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

**引用口径**：`docs/window-batch-plan-20261023.md` **§7.4 @ `c4b66ddb1`**（本次已打开该文件逐行数过）——**枚举 11 行 / 16 个场景**，禁 `18−N`。**本表实跑是 18 可跑 / 2 blocked / 0 假绿** ⇒ **§7.4 滞后 2 个场景**：`bag_equip` / `eat_item` 已被 `c4b66ddb1` 接通，但 §7.4 仍把它们列在「仍不可跑」。**§7.4 需要从 16 更新到 18**（`spell_cast` 已在表内、无需再动）；这条属另一张卡的写集，本卡不改 batch-plan，只在卡上 comment 点名。**11 行 = 16 场景的展开**是自己逐行加出来的：`login_only`(1) + `pickup_item`(1) + `move_walk`(1) + `upgrade_retrieve`(1) + `equip_struck_save`(1) + `dice_chain`/`dice_chain_auto`(2) + `npc_click`/`npc_click_auto`(2) + `king_dialog`/`merchant_dialog`/`npc_script_chain`(3) + `king_castle_war`(1) + `gm_time`/`gm_batch`(2) + `spell_cast`(1) = **16**；补上 `bag_equip`/`eat_item`(2) ⇒ **18 个场景（行粒度由 batch-plan 卡定：合成 1 行则 12 行，拆 2 行则 13 行）**。以下是必须写清的差异/订正：

1. **§7.4 现在是 11 行 / 16 场景，但对本次实测已不完整**。旧 master 的「`18 − 4 = 14`」与未合入 v5（`6ceb0689d`，10 行 / 15 场景）**都不再是口径来源**；开窗计数以 §7.4 的逐行枚举为骨架，再按本表实测确认 **18**。§7.4 里 `spell_cast` 的条目（`magic` 段 + `sides.rust.env.MIR2_CLIENT_ABILITY_LAYOUT=0`）与本次一致。
2. **§7.4 的 `bag_equip`/`eat_item` 条目已过期（本卡实测推翻）**：§7.4「仍不可跑」表写「或先在场景里补 `inventory`」——`c4b66ddb1` 已把这条路径做实：两个场景**已声明 `inventory`**、probe 顶层 schema 放行并形状校验、`card_window --dry-run` 两侧各出 1 行预览（§3.2 五条证据）。⇒ 这两条应移入「现在就能跑」，**§7.4 需从 16 更新到 18**。本卡不改 batch-plan（写集红线），已在卡上 comment 点名。
3. **`dice_chain` 的「引用 T 行 = 3」是子串计数**：whitelist 里带反引号精确点名 `dice_chain` 的只有 T-211 一行；3 = T-211 + T-193 + T-194，其中 T-193/T-194 点名的是 **`dice_chain_auto`**（`dice_chain` 是其子串）。同理 `login_only` 在 §7.4 写 14，本 head 精确整词匹配为 **12**（T-211/232/235/236/246/248/250/256/257/258/259/261），宽匹配 16。`equip_struck_save` 按**同一精确整词口径**为 **1**（T-247），与 §7.4 的 1 一致；宽匹配（路径子串）才得 2（+ T-266，其正文写的是路径 `tools/probe/scenarios/equip_struck_save.json`）。这些是**登记行漂移**，不影响「能跑」判定。
4. **`equip_struck_save` 计入「能跑」是正确的**：`crates/world/src/combat.rs:767` wear_durability + `:1615-1619` 的 `RM_DURACHANGE` 已接（T-266），本次 dry-run rc=0。**但「能跑」≠「已验收」**：R 级真机窗口仍未采；「已接线」不等于「已对拍」。
5. **`npc_click_auto` 在 18 场景里，但没有任何 R 卡把它当窗口判据**：它作为 discovery 载体是必需的，建议在开窗 runbook 里单列为「基础设施」，避免被误当成一条独立判据。
6. **`spell_cast` 从「fail-closed 的例外」变成「18 可跑之一」，`bag_equip`/`eat_item` 从「假绿」变成「18 可跑之一」**：两处分类变化**只反映离线链路**（前者 = `requires` 加载期强制 + 比较器补 53/638/639 + magic/pin；后者 = inventory 夹具可达 + dry-run 出预览），**都不代表真机已验证**；开窗时 `spell_cast` 必须带 pin（§3.1），`bag_equip`/`eat_item` 走场景自带 `inventory`（§3.2），且三者真机窗口仍未跑。

---

## 6. 开窗清单（给 owner）

### 6.1 今天就能进窗口（18 个；离线彩排绿，真机仍需 owner 授权与夹具）

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
| `dice_chain` | 须显式给本次 actor：`card_window --var npc_actor=<本次 id>`（默认 119 是 r9 会话值，不给会点空）。不想取 id 就用 `dice_chain_auto`（见 §6.3） |
| `upgrade_retrieve` | 铁匠摆位 (320,328)+武器在手+金币≥10000+黑铁矿（三项缺一必红） |
| `equip_struck_save` | 「卫家店」(0103,19,18) 摆位；R 级窗口授权（description 已订正为「已接线、真机窗口未跑」） |
| `gm_time` / `gm_batch` | 角色 GM Permission |
| `bag_equip` | 角色站位用 DB 现值（场景无 `placement`）；背包由场景自带 `inventory` 在起栈前预置（木剑，position 40 / make_index 5001）；`make_index`/`takeon_where`/`item_name` 由 201 发现（非空背包 ⇒ 201 有样本）。**真机窗口仍未跑** |
| `eat_item` | 同上；`inventory` = 金创药小量（position 41 / make_index 5002）；`make_index`/`item_name` 由 201 发现。**真机窗口仍未跑** |
| `spell_cast` | **窗口 profile 必须同时有 `magic` 段与 `sides.rust.env.MIR2_CLIENT_ABILITY_LAYOUT="0"`（D-9 pin）**——缺 magic ⇒ 加载期 rc=2；缺 pin ⇒ 53 规则 `body_len` fail-closed FAIL。写法照 `tools/probe/scenarios/README.md`「窗口 profile 前置：`spell_cast` 必须 pin D-9」；**真机窗口仍未跑** |

### 6.2 还差一步（差什么 / 谁负责）

| 场景 | 差什么 | 谁负责 / 承接 |
|---|---|---|
| `combat_hit` | ① 同屏怪物夹具（保证正前方有目标且可排除其反击）；② 伤害夹具（随机护甲 `[AC,2AC-1]` 可能把伤害吃成 0、零帧）。**recog/来源过滤已具备**（`8b40443b6`，selftest 10→11），不再是缺口 | 建议新卡 `openmir2-combat-hit-monster-fixture`（未开）；过滤已具备（`mir2_probe.py:294` `wait_for(..., recog_filter=None)`、`:313` `wait_frames`、`:331` `wait_idents`；`:694` `_plan_recog_filter`）。本场景 `steps[1]` 已改为 `{"idents": 31, "recog_is": "other"}` |
| `combat_strike` | `DC_min≥36..60` 且 `HitPoint≥17..34` 的配装与伤害门槛（`wait_for` recog 过滤**已具备** `8b40443b6`） | 建议卡 `openmir2-combat-strike-gear-fixture`（未开）；解封后一次性解锁 R3 的 13 张 |

> ⚠ **不要用 rc=0 当窗口证据**：`bag_equip`/`eat_item` 的默认 dry-run 现在是 rc=0 **且有两侧各 1 行背包夹具预览**（已不是假绿），但那仍只是离线预览；`spell_cast` 的 rc=0 **只在带 `magic` 段 profile 时成立**（默认 `alt` 已 rc=2 fail-closed），且开窗还须叠加 D-9 pin；`combat_hit`/`combat_strike` 是 `blocked` rc=2。**离线 rc=0 只说明夹具预览齐了，不构成窗口证据；dry-run 绿 ≠ 真机已对拍。**

### 6.3 窗口操作提示（照做，不解释）

- **`dice_chain`：用 `--var npc_actor=<本次 actor id>` 跑本尊**（卡 `openmir2-card-window-var-passthrough` 起支持）：`dice_chain.json` 的 `params.npc_actor` **硬编码为 119**（r9 抓包会话值，C# `ActorId` 每会话不同），而 `npc_actor` **不在 `required`** ⇒ `card_window` 不会为它做 discovery。`--var` 是**覆值通道**（只覆盖已声明变量，不声明新变量），会**原样**出现在两侧 probe 命令行、顺序 = 命令行顺序；不传时行为逐字节不变。取本次 id：在同一会话先跑 `npc_click_auto`（或读上一轮 `frames.ndjson` 里 `CM_CLICKNPC(1010)` 的 `recog/param`），再把该数字喂给 `--var`：

  ```sh
  python -I -B tools/conformance/card_window.py --card <卡 id> \
    --scenario tools/probe/scenarios/dice_chain.json \
    --var npc_actor=<本次 actor id> \
    --profile alt --out E:/tmp/win/dice_chain
  ```

  `--dry-run` 会把 `--var npc_actor=<id>` 打在同一行命令里（先核对再开窗）。**场景 JSON 不动**。不想手工取 id 仍可跑 `dice_chain_auto`（`click_npc="auto:last_turn"` 自动发现）。
- **非法 `--var` fail-closed**：缺 `=` / 空名 / 空值 / 同名不同值 ⇒ 加载期 rc=2 并点名（不会静默丢一个）。
- **`bag_equip` / `eat_item` 可以开窗，但**要确认背包夹具生效：场景顶层已声明 `inventory`（§3.2），`card_window` 会在每侧 restore 之后、起栈之前写进该侧 `characters_bagitem`，dry-run 应看到**两侧各 1 行**「背包夹具…」。**profile 里写 `inventory` 无效**（静默忽略）。
- **`spell_cast` 必须带 `magic` + D-9 pin**：`--profile alt` 下已 **fail-closed（rc=2，加载期点名缺 csharp/rust 侧）**；带 `magic` 段 rc=0 且 2 行夹具预览；**再加 `sides.rust.env.MIR2_CLIENT_ABILITY_LAYOUT="0"` 才是开窗用的形态**（否则 53 比较 fail-closed）。rc=0 只代表离线夹具预览齐了，**真机仍须实跑**。
- **别把 magic profile 当「只影响 spell_cast」**：profile 一旦声明 `magic`，dry-run 会为**每个非 blocked 场景**预览两侧各 1 行魔法夹具（全局 DB 预置）；`blocked` 场景没有预览行——硬门（`:3349-3359`）在预览循环（`:3425`）之前就 rc=2 返回，实测 **18/20**（`combat_hit`/`combat_strike` 为 0），别按「20×2=40 行」对账。

---

## 7. 开窗时可照抄的命令

```sh
# 0) 先做开窗自查（preflight 已合入 master 2e3b8500a）
python -I tools/goal/goal_gate.py --json
python -I tools/conformance/preflight.py --profile alt

# 1) 复制 magic profile（仓库外）
#    组成 = 内置 alt 深拷贝 + 顶层 magic 段 + 两侧 env 的 D-9 pin。
#    照抄写法与原因见 tools/probe/scenarios/README.md
#    「窗口 profile 前置：spell_cast 必须 pin D-9」与「requires」两节。
#    ⚠ 不要提交进仓库；本文件不重写该片段。

# 2) 无 required / 无夹具的场景
python -I -B tools/conformance/card_window.py --card <卡 id> \
  --scenario tools/probe/scenarios/login_only.json \
  --profile alt --out E:/tmp/win/login_only

# 3) 有 required 的场景（card_window 自动 discovery）
python -I -B tools/conformance/card_window.py --card <卡 id> \
  --scenario tools/probe/scenarios/npc_script_chain.json \
  --profile alt --out E:/tmp/win/npc_script_chain

# 4a) dice_chain：用 --var 覆盖本次 actor id（卡 openmir2-card-window-var-passthrough）
#     默认 params.npc_actor=119 是 r9 会话值；id 的取法见 §6.3
python -I -B tools/conformance/card_window.py --card <卡 id> \
  --scenario tools/probe/scenarios/dice_chain.json \
  --var npc_actor=<本次 actor id> \
  --profile alt --out E:/tmp/win/dice_chain

# 4b) 不想手工取 id：dice_chain_auto（actor 自动发现）
python -I -B tools/conformance/card_window.py --card <卡 id> \
  --scenario tools/probe/scenarios/dice_chain_auto.json \
  --profile alt --out E:/tmp/win/dice_chain_auto

# 4c) bag_equip / eat_item：场景自带 inventory，无需额外 profile
#     dry-run 应先看到两侧各 1 行「背包夹具…」预览，再开窗
python -I -B tools/conformance/card_window.py --card <卡 id> \
  --scenario tools/probe/scenarios/bag_equip.json \
  --profile alt --out E:/tmp/win/bag_equip
python -I -B tools/conformance/card_window.py --card <卡 id> \
  --scenario tools/probe/scenarios/eat_item.json \
  --profile alt --out E:/tmp/win/eat_item

# 5) spell_cast：profile 必须含 magic 段 + D-9 pin（否则加载期 rc=2 或比较器 fail-closed）
python -I -B tools/conformance/card_window.py --card <卡 id> \
  --scenario tools/probe/scenarios/spell_cast.json \
  --profile E:/tmp/win/profile-magic-pin.json --out E:/tmp/win/spell_cast

# 6) 离线单场景校验（不改库、不连网）
python -I -B tools/probe/mir2_probe.py --check-scenario \
  --scenario tools/probe/scenarios/upgrade_retrieve.json --var merchant_actor=119
```

> **`card_window --var NAME=VALUE`（卡 `openmir2-card-window-var-passthrough`）**：可重复，原样透传给两侧（csharp/rust）probe；`--dry-run` 打印的命令里可见。加载期 fail-closed：缺 `=`、空名、空值、同名不同值 ⇒ rc=2 点名。**不传时行为与产物逐字节不变**。场景 JSON 未改。

### 7.1 窗口 profile 的组成（只给指针，不重写片段）

- **`magic` 段**：值 = 场景 `spell_cast.json` 的 `description` 写死的 `magic_id=1 / level=0 / use_key="1" / curr_train=0`（逐字见该文件）；语义由 `card_window` 在 restore 与 placement 之后、起栈之前写进该侧测试库 `characters_magic`（`profile_magic`/`run_magic`）。`requires` 的加载期判定见 `tools/probe/scenarios/README.md`「`requires`」节。
- **D-9 pin**：`sides.csharp.env` / `sides.rust.env` 里各写 `"MIR2_CLIENT_ABILITY_LAYOUT": "0"`（**字符串**，不是数字）；真正必需的是 `sides.rust.env`。片段与 fail-closed 原因见 README「窗口 profile 前置：`spell_cast` 必须 pin D-9」。
- 两者都并入**内置 `alt` 的深拷贝**（profile 必须有完整 `sides.{csharp,rust}` 的 `stack/stop/restore/db/binaries`）⇒ 本次实跑用 `E:/tmp/w58-rehearsal/profile-magic-pin.json`（temp，未入库）。

---

## 8. 门禁与残留物

- 写集：仅 `docs/window-batch-rehearsal-20261024.md`。
- 返工基线：`c4b66ddb1`（inventory 可达合入后）；前一版基于 `47fd96ca8`，其 `*-r2` 复跑覆盖全矩阵。
- 门禁（见卡上 patch 条目，附 rc）：`t_reservation_gate check`、冲突标记门、`git merge-tree --write-tree refs/remotes/walgit/master HEAD`（对 `c4b66ddb1`）。
- 本次复跑日志：`E:/tmp/w58-rehearsal/logs/`（`*-r2` 后缀 = `c4b66ddb1` 上的全矩阵：每场景 `check-bare` / `check-var` / `dry-alt` / `dry-magic` `.log` + inventory 对照日志；仓库外）。
- 残留物（全部在仓库外，可随时删）：
  - `E:/tmp/w58-rehearsal/logs/`（含 r1 `47fd96ca8` 与 r2 `c4b66ddb1` 两轮）
  - `E:/tmp/w58-rehearsal/profile-magic.json`、`profile-magic-pin.json`、`profile-inventory.json`
  - `E:/tmp/w58-rehearsal/bag_equip-with-inventory.json`、`eat_item-with-inventory.json`、`bag_equip-bad-inv.json`、`bag_equip-bad-inv-r2.json`（仓库外场景副本，**仓库内场景 JSON 未动**）
  - `E:/tmp/w58-rehearsal/out/` **未创建**（dry-run 不建 out，已核）
- 未起栈、未连库、未占用任何端口；未改卡片状态（除本卡自己的签名条目）。
