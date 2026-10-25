# 窗口真机证据回填（W1 21 条 + db-rerun `run`/`run2` 各 6 条；卡 `openmir2-window-evidence-backfill`）

- 卡：`openmir2-window-evidence-backfill`（issue `d12736b5`；owner `openmir2-dev-1`；
  branch `card/window-evidence-backfill`；base = 本 patch 的 `--base`）。tier **L**（纯记账，
  只读既有证据根，不改代码、不启服务端）。
- **只写这一个新文件**：不改台账 `docs/window-closure-ledger-20261025.md` 原文，也不改任何卡片状态
  （那是 Leader 的收口动作）。

## 0. 取数与复核口径（**数字全部自己从 `report.json` 取**，没有抄 tester 的表）

> **取数时点**：2026-10-25 **11:05 +08**（本次运行读 `E:/tmp/window/{w1c,db-rerun/run,db-rerun/run2}` 当次；master=`57d506fa9`；三批次各自的产物生成时间见其 `card-window.json`）。

证据根（只读）：`E:/tmp/window/w1c/<场景>/report.json`、`E:/tmp/window/db-rerun/run/<场景>/report.json`、
`E:/tmp/window/db-rerun/run2/<场景>/report.json`；`outer_rc` 取同目录 `card-window.json` 的 `exit_code`。

复核脚本（当次实跑，可复现）：

```python
import io, json, os
def row(base, name):
    rep = json.load(io.open(os.path.join(base, name, "report.json"), encoding="utf-8"))
    cw  = json.load(io.open(os.path.join(base, name, "card-window.json"), encoding="utf-8"))
    fr, db = rep.get("frames") or {}, rep.get("db") or {}
    d = rep.get("diffs") or []
    return dict(verdict=rep.get("verdict"), compare_exit=rep.get("exit_code"),
                outer_rc=cw.get("exit_code"), oracle=fr.get("oracle_total"),
                rust=fr.get("rust_total"), compared=fr.get("compared"),
                diffs_total=len(d),
                diffs_frame=sum(1 for x in d if x.get("source") == "frame"),
                diffs_db=sum(1 for x in d if x.get("source") == "db"),
                db_state=db.get("state"), db_records=len(db.get("diffs") or []))
```

**列口径**：`compare_rc` = compare 阶段自己的 `exit_code`（0/1/2）；`outer_rc` = `card_window` 的
`exit_code`（它在 compare 之外还会因编排/DB 面升到 2）；`帧 diff` 拆成 `f`(frame) / `d`(db)；
`未跑到 compare` 的场景（W1 中 **12 条**）帧数记为 `—`、`outer_rc=2`。

## 1. W1（21 条，旧工具链）

> **DB 面一律标「不可用」**：W1 的 DB 快照拍在**存档落定之前**（Rust 断线同步落库 / C# 异步且随后被强杀），
> 差异是**存档时机**造成的假差异，已被 harness 修复（卡 `openmir2-harness-save-time-symmetry`）取代；
> 本节仍保留当时记录条数，仅供追溯，**不得当作 DB 面判据**。

| 场景 | verdict | outer_rc | compare_rc | 帧 oracle/rust/compared | 帧 diff | DB 记录 | 证据路径 |
|---|---|---:|---:|---|---:|---:|---|
| `bag_equip` | UNDECIDABLE | 2 | 2 | —/—/— | 0（f0/d0） | — | `E:\tmp\window\w1c\bag_equip` |
| `castle_merchant_tax_potion` | UNDECIDABLE | 2 | 2 | —/—/— | 0（f0/d0） | — | `E:\tmp\window\w1c\castle_merchant_tax_potion` |
| `clearmapmon_q001` | UNDECIDABLE | 2 | 2 | —/—/— | 0（f0/d0） | — | `E:\tmp\window\w1c\clearmapmon_q001` |
| `dice_chain` | UNDECIDABLE | 2 | 2 | —/—/— | 0（f0/d0） | — | `E:\tmp\window\w1c\dice_chain` |
| `dice_chain_auto` | UNDECIDABLE | 2 | 2 | 34/33/33 | 0（f0/d0） | **不可用**（存档时机） | `E:\tmp\window\w1c\dice_chain_auto` |
| `dropgold_zero_draw` | UNDECIDABLE | 2 | 2 | —/—/— | 0（f0/d0） | — | `E:\tmp\window\w1c\dropgold_zero_draw` |
| `eat_item` | UNDECIDABLE | 2 | 2 | —/—/— | 0（f0/d0） | — | `E:\tmp\window\w1c\eat_item` |
| `equip_struck_save` | UNDECIDABLE | 2 | 2 | —/—/— | 0（f0/d0） | — | `E:\tmp\window\w1c\equip_struck_save` |
| `gamepoint_exchange` | UNDECIDABLE | 2 | 2 | —/—/— | 0（f0/d0） | — | `E:\tmp\window\w1c\gamepoint_exchange` |
| `gm_batch` | FAIL | 2 | 1 | 34/36/34 | 2（f1/d1） | **不可用**（存档时机）（记录 1 条，仅存档） | `E:\tmp\window\w1c\gm_batch` |
| `gm_time` | PASS | 0 | 0 | 30/30/30 | 0（f0/d0） | **不可用**（存档时机） | `E:\tmp\window\w1c\gm_time` |
| `king_castle_war` | UNDECIDABLE | 2 | 2 | —/—/— | 0（f0/d0） | — | `E:\tmp\window\w1c\king_castle_war` |
| `king_dialog` | UNDECIDABLE | 2 | 2 | —/—/— | 0（f0/d0） | — | `E:\tmp\window\w1c\king_dialog` |
| `login_only` | FAIL | 2 | 1 | 31/29/29 | 2（f1/d1） | **不可用**（存档时机）（记录 1 条，仅存档） | `E:\tmp\window\w1c\login_only` |
| `merchant_dialog` | FAIL | 2 | 1 | 34/32/29 | 4（f4/d0） | **不可用**（存档时机） | `E:\tmp\window\w1c\merchant_dialog` |
| `move_walk` | UNDECIDABLE | 2 | 2 | 40/39/37 | 19（f16/d3） | **不可用**（存档时机）（记录 3 条，仅存档） | `E:\tmp\window\w1c\move_walk` |
| `npc_click` | FAIL | 2 | 1 | 30/30/20 | 7（f7/d0） | **不可用**（存档时机） | `E:\tmp\window\w1c\npc_click` |
| `npc_click_auto` | FAIL | 2 | 1 | 30/30/30 | 1（f0/d1） | **不可用**（存档时机）（记录 1 条，仅存档） | `E:\tmp\window\w1c\npc_click_auto` |
| `npc_script_chain` | UNDECIDABLE | 2 | 2 | —/—/— | 0（f0/d0） | — | `E:\tmp\window\w1c\npc_script_chain` |
| `spell_cast` | PASS | 0 | 0 | 34/34/34 | 0（f0/d0） | **不可用**（存档时机） | `E:\tmp\window\w1c\spell_cast` |
| `upgrade_retrieve` | UNDECIDABLE | 2 | 2 | —/—/— | 0（f0/d0） | — | `E:\tmp\window\w1c\upgrade_retrieve` |

**W1 小结**（我自己数的，2+5+14=21）：**PASS 2**（快照 57d506fa9 @ 2026-10-25 11:05；`gm_time` / `spell_cast`）；**FAIL 5**（快照 57d506fa9）
（`gm_batch` / `login_only` / `merchant_dialog` / `npc_click` / `npc_click_auto`）；**UNDECIDABLE 14**，
其中 **12 条根本没跑到 compare**（`oracle=None`：`bag_equip` / `castle_merchant_tax_potion` /
`clearmapmon_q001` / `dice_chain` / `dropgold_zero_draw` / `eat_item` / `equip_struck_save` /
`gamepoint_exchange` / `king_castle_war` / `king_dialog` / `npc_script_chain` / `upgrade_retrieve`），
另 **2 条**是规则面 UNDECIDABLE（`dice_chain_auto`、`move_walk`：`pending_ignore_rule`）。

> 注：W1 里 `move_walk` 的 `outer_rc=2` 来自 `pending_ignore_rule`，而其**帧/DB 差异本身是实的**
> （19 条帧差 + 3 条 DB 记录），不要被 verdict 的 UNDECIDABLE 掩盖。

## 2. db-rerun `run`（6 条，**旧口径**）

> **标「旧口径，被 `run2` 取代」**：这一批在时间列见证与部分判据口径更新之前跑的。
> **注**：三条 PASS 的 `outer_rc=0` 里，**`gm_batch` 只在 `run` 这批成立**（`run2` 因 ident 100 条数差 FAIL）；
> `gm_time` / `npc_click_auto` 两批都 `outer_rc=0`。
> **但它的三条 PASS（`gm_batch` / `gm_time` / `npc_click_auto`）全部 `outer_rc=0`，是 rc 修复的首批
> 批次级证据**（此前只有单场景自证）——这一条价值独立于口径，保留。

| 场景 | verdict | outer_rc | compare_rc | 帧 oracle/rust/compared | 帧 diff | DB 记录 | 证据路径 |
|---|---|---:|---:|---|---:|---:|---|
| `gm_batch` | PASS | 0 | 0 | 34/34/34 | 0（f0/d0） | compared/0 | `E:\tmp\window\db-rerun\run\gm_batch` |
| `gm_time` | PASS | 0 | 0 | 30/30/30 | 0（f0/d0） | compared/0 | `E:\tmp\window\db-rerun\run\gm_time` |
| `login_only` | UNDECIDABLE | 2 | 2 | —/—/— | 0（f0/d0） | —/0 | `E:\tmp\window\db-rerun\run\login_only` |
| `merchant_dialog` | FAIL | 2 | 1 | 33/32/32 | 2（f2/d0） | compared/0 | `E:\tmp\window\db-rerun\run\merchant_dialog` |
| `move_walk` | FAIL | 2 | 1 | 38/39/37 | 20（f15/d5） | compared/5 | `E:\tmp\window\db-rerun\run\move_walk` |
| `npc_click_auto` | PASS | 0 | 0 | 30/30/30 | 0（f0/d0） | compared/0 | `E:\tmp\window\db-rerun\run\npc_click_auto` |

## 3. db-rerun `run2`（6 条，新口径）

| 场景 | verdict | outer_rc | compare_rc | 帧 oracle/rust/compared | 帧 diff | DB 记录 | 证据路径 |
|---|---|---:|---:|---|---:|---:|---|
| `gm_batch` | FAIL | 2 | 1 | 37/34/29 | 6（f6/d0） | compared/0 | `E:\tmp\window\db-rerun\run2\gm_batch` |
| `gm_time` | PASS | 0 | 0 | 30/30/30 | 0（f0/d0） | compared/0 | `E:\tmp\window\db-rerun\run2\gm_time` |
| `login_only` | PASS | 0 | 0 | 29/29/29 | 0（f0/d0） | compared/0 | `E:\tmp\window\db-rerun\run2\login_only` |
| `merchant_dialog` | FAIL | 2 | 1 | 32/32/32 | 2（f2/d0） | compared/0 | `E:\tmp\window\db-rerun\run2\merchant_dialog` |
| `move_walk` | FAIL | 2 | 1 | 37/39/37 | 20（f15/d5） | compared/5 | `E:\tmp\window\db-rerun\run2\move_walk` |
| `npc_click_auto` | PASS | 0 | 0 | 30/30/30 | 0（f0/d0） | compared/0 | `E:\tmp\window\db-rerun\run2\npc_click_auto` |

**run2 小结**：PASS 3（`gm_time` / `login_only` / `npc_click_auto`，均 `outer_rc=0` 且 DB 记录 0）；
FAIL 3（快照 57d506fa9 @ 2026-10-25 11:05；`gm_batch` 6 条帧差、`merchant_dialog` 2 条帧差、`move_walk` 20 条帧差 + 5 条 DB 记录）。
**`login_only` 在 `run` 里是 UNDECIDABLE（未跑到 compare），`run2` 里 PASS** ⇒ 这一格是「工具链修好后
同一场景变得可判」的直接对照。

## 4. 卡映射与三态（W1 与 run2）

三态定义（本文件的口径，**不是**卡片状态）：

- **已证（帧面）**：该场景 `verdict=PASS`（compare 帧面 0 diff）；
- **已证（DB 面）**：仅 `run2`（新口径）且 `db.state=compared` 且 DB 记录 0；
- **未覆盖**：该场景没跑到 compare（`oracle=None`）；
- **已采集、未过**：跑到了 compare 但有差异 —— 这是**证据**（指向差异），不是「已证」。

映射来源：`docs/rcard-evidence-plan-20261025.md` §2 表（逐卡「场景/夹具」列，行号随表给出）。
**同一场景服务多张卡**，卡的三态随场景结论走；未列在 §2 表的场景（`gm_batch` / `gm_time` /
`npc_script_chain` / `king_dialog` / `clearmapmon_q001` / `gamepoint_exchange` / `dropgold_zero_draw` /
`castle_merchant_tax_potion`）属台账 W 项或工具面，**需 Leader 归口**，我不在此擅自指定。

### 4.1 W1

| 场景 | 服务的 R 卡 | 本批三态 |
|---|---|---|
| `bag_equip` | `openmir2-delmagic-addmagic-chain`（§2 L100） | **未覆盖**（未跑到 compare） |
| `castle_merchant_tax_potion` | 未在 `docs/rcard-evidence-plan-20261025.md` §2 表内（属 W 项/工具面，需 Leader 归口） | **未覆盖**（未跑到 compare） |
| `clearmapmon_q001` | 未在 `docs/rcard-evidence-plan-20261025.md` §2 表内（属 W 项/工具面，需 Leader 归口） | **未覆盖**（未跑到 compare） |
| `dice_chain` | `openmir2-menu-ok-unify-pickwire`（§2 L122）、`openmir2-script-playdice-openybdeal`（§2 L128）、`openmir2-script-take-persist`（§2 L129）、`openmir2-ya52-menu-ok-missing`（§2 L140） | **未覆盖**（未跑到 compare） |
| `dice_chain_auto` | `openmir2-menu-ok-unify-pickwire`（§2 L122）、`openmir2-ya52-menu-ok-missing`（§2 L140） | **规则面 UNDECIDABLE**（`pending_ignore_rule` ident 14），非帧差异（本批 0 diff） |
| `dropgold_zero_draw` | 未在 `docs/rcard-evidence-plan-20261025.md` §2 表内（属 W 项/工具面，需 Leader 归口） | **未覆盖**（未跑到 compare） |
| `eat_item` | `openmir2-spell-fill-projection-mirror`（§2 L131）、`openmir2-unbindlist-consumer`（§2 L134） | **未覆盖**（未跑到 compare） |
| `equip_struck_save` | `openmir2-attack-weapon-wear-642`（§2 L94） | **未覆盖**（未跑到 compare） |
| `gamepoint_exchange` | 未在 `docs/rcard-evidence-plan-20261025.md` §2 表内（属 W 项/工具面，需 Leader 归口） | **未覆盖**（未跑到 compare） |
| `gm_batch` | 未在 `docs/rcard-evidence-plan-20261025.md` §2 表内（属 W 项/工具面，需 Leader 归口） | 已采集、**未定性（flaky 候选）**——见 §6.3 |
| `gm_time` | 未在 `docs/rcard-evidence-plan-20261025.md` §2 表内（属 W 项/工具面，需 Leader 归口） | 已证（帧面；DB 面不可用） |
| `king_castle_war` | `openmir2-t138-request-castle-war`（§2 L132） | **未覆盖**（未跑到 compare） |
| `king_dialog` | 未在 `docs/rcard-evidence-plan-20261025.md` §2 表内（属 W 项/工具面，需 Leader 归口） | **未覆盖**（未跑到 compare） |
| `login_only` | `openmir2-db-encoding`（§2 L99）、`openmir2-gamesvr-serverinfo-report`（§2 L102）、`openmir2-gm-impl-shutup`（§2 L103）、`openmir2-gm-script-host-seam`（§2 L104）、`openmir2-guard-ctor-constants`（§2 L105）、`openmir2-guild-g2-dispatch`（§2 L106）、`openmir2-login-529-status-text`（§2 L108）、`openmir2-login-654-order`（§2 L109）、`openmir2-login-charstatus-657`（§2 L110）、`openmir2-login-frame-order-52-752`（§2 L111）、`openmir2-login-numeric-gaps`（§2 L112）、`openmir2-npc-ctor-constants`（§2 L125）、`openmir2-npc-display-table`（§2 L126）、`openmir2-searchtick-origin-parity`（§2 L130）、`openmir2-verify-tick-base`（§2 L136）、`openmir2-window-collector-preflight`（§2 L138）、`openmir2-world-tick-expansion`（§2 L139） | 已采集、**未过**（见差异明细） |
| `merchant_dialog` | `openmir2-encoding-core-e1-e2`（§2 L101）、`openmir2-market-def-impl`（§2 L120） | 已采集、**未过**（见差异明细） |
| `move_walk` | `openmir2-transmap-maptrigger`（§2 L133） | 已采集、**未过**（见差异明细） |
| `npc_click` | `openmir2-click-path-bag-writeback`（§2 L97）、`openmir2-encoding-core-e1-e2`（§2 L101）、`openmir2-npc-click-frame-seq-fix`（§2 L124）、`openmir2-npc-display-table`（§2 L126） | 已采集、**未过**（见差异明细） |
| `npc_click_auto` | `openmir2-npc-click-frame-seq-fix`（§2 L124） | 帧面 **0 diff**（W1 唯一差异是 DB 面，已按 §1 作废）；`run2` 已 PASS |
| `npc_script_chain` | 未在 `docs/rcard-evidence-plan-20261025.md` §2 表内（属 W 项/工具面，需 Leader 归口） | **未覆盖**（未跑到 compare） |
| `spell_cast` | `openmir2-buff-system`（§2 L95）、`openmir2-compare-ident53-diff-triage`（§2 L98）、`openmir2-healthspell-emission-model`（§2 L107）、`openmir2-mb8-9-monsters`（§2 L121）、`openmir2-mp-session-projection-clobber`（§2 L123）、`openmir2-spell-fill-projection-mirror`（§2 L131） | 已证（帧面；DB 面不可用） |
| `upgrade_retrieve` | `openmir2-a09-upgrade-item`（§2 L92）、`openmir2-outbound-encoders-red-batch`（§2 L127）、`openmir2-upgrade-npc`（§2 L135）、`openmir2-weapon-upgrade-values`（§2 L137） | **未覆盖**（未跑到 compare） |

### 4.2 run2

| 场景 | 服务的 R 卡 | 本批三态 |
|---|---|---|
| `gm_batch` | 未在 `docs/rcard-evidence-plan-20261025.md` §2 表内（属 W 项/工具面，需 Leader 归口） | 已采集、**未定性（flaky 候选）**——见 §6.3 |
| `gm_time` | 未在 `docs/rcard-evidence-plan-20261025.md` §2 表内（属 W 项/工具面，需 Leader 归口） | 已证（帧面 + DB 面） |
| `login_only` | `openmir2-db-encoding`（§2 L99）、`openmir2-gamesvr-serverinfo-report`（§2 L102）、`openmir2-gm-impl-shutup`（§2 L103）、`openmir2-gm-script-host-seam`（§2 L104）、`openmir2-guard-ctor-constants`（§2 L105）、`openmir2-guild-g2-dispatch`（§2 L106）、`openmir2-login-529-status-text`（§2 L108）、`openmir2-login-654-order`（§2 L109）、`openmir2-login-charstatus-657`（§2 L110）、`openmir2-login-frame-order-52-752`（§2 L111）、`openmir2-login-numeric-gaps`（§2 L112）、`openmir2-npc-ctor-constants`（§2 L125）、`openmir2-npc-display-table`（§2 L126）、`openmir2-searchtick-origin-parity`（§2 L130）、`openmir2-verify-tick-base`（§2 L136）、`openmir2-window-collector-preflight`（§2 L138）、`openmir2-world-tick-expansion`（§2 L139） | 已证（帧面 + DB 面） |
| `merchant_dialog` | `openmir2-encoding-core-e1-e2`（§2 L101）、`openmir2-market-def-impl`（§2 L120） | 已采集、**未过**（见差异明细） |
| `move_walk` | `openmir2-transmap-maptrigger`（§2 L133） | 已采集、**未过**（见差异明细） |
| `npc_click_auto` | `openmir2-npc-click-frame-seq-fix`（§2 L124） | 已证（帧面 + DB 面） |

## 5. `spell_cast` 的 PASS 有**两层折扣**（必须写明）

`spell_cast` 在 W1 是 PASS（34/34/34、0 diff），但它**不是无条件 PASS**：

1. **排除区**：每侧各有 **2 帧** `frame_class=server_ack_time` 的控制帧被先剔除
   （`frames.filtered_control_frames`）——oracle `body_len=13`（seq 31/34，t=4156/6483ms），
   rust **`body_len=17`**（seq 33/36，t=2316/4647ms）。这些帧**不参与比较**，两侧 body 长度不同
   也**不会被判红**。
2. **证据折扣（掩码）**：`rules.pending_review = ['rules[0]:ident=[10]', 'rules[2]:ident=[14]',
   'rules[5]:ident=[50]']` 仍是 PENDING。其中 `series` 字段的**实际命中**是：
   - 本批 `masked_head` 里 **`series` 只出现在 ident 10**；
   - **ident 50 本批无差异记录 ⇒ 未比过**，不等于「比过且相同」。

⇒ 结论口径：`spell_cast` 的 PASS 覆盖的是**除上述两项之外**的帧面；**ident 10 的 Dir/朝向与两条控制帧
不在判据内**（范围**只对 ident 10 成立**，不要扩读成全局）。

## 6. 稳定差异 vs flaky（三类分开）

### 6.1 `move_walk` 的 `Home*`：**三次同形 ⇒ 稳定差异候选**（另开卡 `openmir2-home-coords-divergence`）

| 批次 | HomeMap | HomeX | HomeY |
|---|---|---|---|
| W1 | 3 → 0 | 330 → 289 | 330 → 618 |
| `run` | 3 → 0 | 330 → 289 | 330 → 618 |
| `run2` | 3 → 0 | 330 → 289 | 330 → 618 |

（`a` = oracle、`b` = rust；三批**方向与数值完全一致**）⇒ 不是时序抖动，按**稳定差异**处理。

### 6.2 `move_walk` 的 `CX` / `Dir`：只在 `run`/`run2` 出现 ⇒ **走位 flaky**

`CX`：321 → 320；`Dir`：2 → 3 —— 两批同形但 W1 没有这两条（W1 的角色落点不同），
且它们随「走位」变化 ⇒ 归 **flaky 类**（与卡 `openmir2-neighbor-actor-frame-drift` 的
「同场景重复跑 ≥3 轮」同族），**不并入 Home\* 的稳定结论**。

### 6.3 `gm_batch`：**flaky 候选**（未定性；不进台账、不带判据）

`run2` 的 6 条 = **5 条 `multiset_extra`（ident **100** `SM_SYSMESSAGE`，全部 `absent_on=oracle`，
即 **rust 多 5 帧**）+ 1 条 `block_align`**（尾部多出 ident `[10, 41, 100, 100, 100, 100, 100]`，
kept 帧数 oracle 36 / rust 34）。
W1 的 2 条是 `block_align`（尾部多出 `[10, 41]`）+ 1 条 DB `Weight 14→0`（**W1 DB 面已作废**）；
`run` 是 **PASS（0 diff）**。

**判定（Leader 裁决）：`gm_batch` 记为「flaky 候选」，未定性**——
- 三次真跑形态各不相同（`run` PASS → W1 `block_align` + DB `Weight` → run2 5 条 `multiset_extra`
  ident 100）⇒ **不是稳定差异**；
- 差异主体是 **ident=100 的条数差**，与 `openmir2-neighbor-actor-frame-drift` 记账的 10/41 **稳定**多帧
  是两回事，**不并入**那张卡（混在一起会互相污染证据），**也不新开卡**；
- **原因子**：ident 100 回显条数 / 帧块对齐；
- **解封**：同一场景 **≥3 轮真机**（窗口）；本轮**不进台账、不带判据**。

## 7. 未覆盖清单（避免后来人以为覆盖过）

| 场景 | 现状 | 需先解决什么 |
|---|---|---|
| `#18 gamepoint_exchange` | W1 **未跑到 compare**（`oracle=None`、`outer_rc=2`） | 夹具/前置缺失档；需先让该场景跑到 probe 完成（夹具或流程），再谈判据 |
| `#20 dropgold_zero_draw` | 同上 | 同上 |
| `#23 castle_merchant_tax_potion` | W1 未跑到 compare；E 类修复后**复拍中** | 等 tester 的复拍结果；复拍前不得引用任何旧数字 |

> `#` 号取 `tools/window/run3-scenarios.json` 的 `n`（18=gamepoint_exchange、20=dropgold_zero_draw、
> 23=castle_merchant_tax_potion）。

## 8. 残留物与复现

- 本文件为**唯一新增产物**；未改台账、未改卡片状态、未改任何代码。
- 复核数据快照：`E:/tmp/backfill-raw.json`（三批次逐场景抽取结果）、`E:/tmp/scen2cards.json`
  （§2 表反查的场景→卡映射）——两者都是**过程物**，不入库；按 §0 的脚本可随时重算。
- 证据根为 tester 的运行产物（只读引用，路径见各表）。
