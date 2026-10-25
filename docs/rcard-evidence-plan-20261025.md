# R 卡证据缺口三分类 + 窗口覆盖表 + owner 裁定包（2026-10-25）

- 卡：`openmir2-rcard-evidence-classification`（tier=L，write_set=`docs/rcard-evidence-plan-20261025.md`；只读分析）。
- 数据时点：master `0ef4e3b96` @ 2026-10-24 23:34 +08；`goal_gate` 实测 `generated_ts=1792856290`（2026-10-24 23:38:10 +08）。
- 事实源（只读，本卡一行未改）：`tools/goal/goal_gate.py`、`docs/window-batch-plan-20261023.md`、`docs/window-card-scenario-coverage-20261024.md`（已合并 `85ccbdc85`，reviewer-5 approve `4b573434e`）、`docs/needs-human-20261024.md`、`tools/probe/scenarios/*.json`、`refs/collab/*`。
- 本卡不做的事：不改卡片状态、不改 `src/**`/规则表/场景 JSON/`whitelist.md`、不起栈、不连库。

## 0. 一句话结论

137 张 `window_required`（已合入桶）按**可判性**分三档：**49 张现在就有场景可跑**（一次窗口最多消掉 35.8%）、**26 张已有登记可豁免**（25 张 batch-plan §4「不可协议对拍」+ 1 张窗口执行记录卡）、**62 张没有任何当前可跑场景**（其中 46 张有明确的新场景路线、15 张卡在 blocked/不可达/缺夹具、1 张是客户端 E2E）。owner 只需回答 5 个选择题（§6），其中 Q1（开窗）与 Q3（combat 夹具）直接决定 137 张里 59 张的命运。

## 1. 口径与复算（对应卡面 ④）

### 1.1 goal_gate 现取原文（逐字）

快照 截至 `0ef4e3b96` @ 2026-10-24 23:38 +08（goal_gate 实测；下方 detail 行逐字引用）。

命令：`python -I -B tools/goal/goal_gate.py --json --skip-tests --skip-script`（快速模式；条件①/④ 的 need_human 是本模式跳过所致，与条件② 无关）。`overall=need_human`，rc=2（rc=2 = 有「需人工」，不是脚本错误）。

```
R 卡 206 张：已完成 11 / window_required 137（另未合并 2） / whitelist 48 / 已裁定登记 3 / comment 证据 0 / 三者皆无 0 / merged 未 closed 且无登记(需人工) 5 / 未开工或未合并 0（不计缺口）/ 其它需人工 0；需人工 5 张（按 last_status_ts 最旧优先，见 JSON need_items）；merged 未 closed 但已凭登记 PASS：186 张（卫生清单，见 evidence.merged_open_hygiene；登记型 PASS ≠ 已有真机证据）
```

### 1.2 每个数字怎么来的（逐桶对账，可复算）

| 桶（`category`） | 张数 | 与 137 的关系 |
|---|---:|---|
| `window_required` | **137** | **本报告的分类全集**（已合入、无显式豁免裁定） |
| `window_required_unmerged` | 2 | 未合并，窗口在合并前跑不了（`gm-batch-5-guild-castle`、`window-gate-two-runs`），**不在 137 内** |
| `whitelist_t` | 48 | T-* 登记行点名本卡（登记 = 偏差登记，**≠ 豁免窗口**，见 §1.3） |
| `closed` | 11 | 已收口；其中 4 张仍带 `window_required` 旗标，但已不计窗口债 |
| `adjudicated` | 3 | 最新 approve 显式 `window_required=false` + 理由（owner 裁定 L） |
| `merged_not_closed` | 5 | 已合并、非 closed、无登记 ⇒ 条件② 的「需人工 5 张」 |
| **合计** | **206** | 137+2+48+11+3+5 = 206 ✓ |

另两个易混数：

- **`window_required` 旗标为 true 的共 145 张** = 137 + 2 未合并 + 4 已 closed 仍带旗标 + 2 已裁定 L 仍带旗标。本报告只用 137（卡面口径），不把未合并/已收口混进来。
- **卫生清单 186 张** = `merged=true` 且 board 非 closed、已凭登记 PASS 的卡 = 137（`window_required`）+ 46（`whitelist_t` 中 merged 的；48 − 2 张未合并）+ 3（`adjudicated`，全部 merged）。**186 是「登记型 PASS」，不是真机证据**——`window_required` 登记本身就是「这卡还欠窗口」的记录。

### 1.3 一个必须先讲清的口径：`window_required` 优先于 `whitelist_t`

`goal_gate.r_category()` 的优先级是 `closed → not_merged → unverified → adjudicated(no_window) →
window_required → whitelist_t → …`。因此：

- **有 T-* 登记行点名 ≠ 免除窗口**。T-* 是「对拍刻意不覆盖/已知限制」的偏差登记，很多 T 行本身还带窗口项
  （如 T-266、T-213、T-220；见 `mir2-rs/tests/parity/whitelist.md` 表头规则）。137 张里有 30+ 张同时有强关联 T 行，
  **本报告不把这些算进「已有登记可豁免」**，否则会把「还欠窗口」错算成「已豁免」。
- `adjudicated` 桶才是有 `window_required=false` + 理由的显式豁免；137 张里 **没有一张** 属此类。

### 1.4 三分类总数（137 = 49 + 62 + 26）

| 分类 | 张数 | 含义 | 处置 |
|---|---:|---|---|
| **A 场景可覆盖** | **49** | 现有可跑场景（§7.4 枚举）或既有 pair 夹具能产出卡面判据 | 下一轮窗口（W1） |
| **C 已有登记可豁免** | **26** | 25 张 batch-plan §4「不可协议对拍」（已有替代判据登记）+ `window-run-1`（窗口执行记录卡） | 不占协议对拍窗口；按替代判据/执行记录收口（Q5） |
| **B 无场景需裁定** | **62** | 没有当前可跑场景：blocked 5 / 不可达 4 / 需新场景 28 / 仅作依赖 1 / 文档无登记 23 / 新增 drop 1 | 补场景或以裁定收口（Q2/Q3/Q4） |
| **合计** | **137** | A∪B∪C，两两不交；复算见 §1.5 | |

### 1.5 复算命令（与 goal_gate 同口径）

```sh
# 1) 取当次 206 张 R 卡与桶计数（约 90s；rc=2 是 need_human，不是错误）
python -I -B tools/goal/goal_gate.py --json --skip-tests --skip-script > "$TEMP/gate.json"
python -I -c "import json,os,collections;d=json.load(open(os.path.join(os.environ['TEMP'],'gate.json')));rc=d['conditions'][1]['evidence']['r_cards'];print(len(rc));print(collections.Counter(x['category'] for x in rc))"

# 2) 137 张逐卡场景归属（本报告 §2/§3/§4 的行来自下面两次 grep）
grep -n "<卡 id>" docs/window-batch-plan-20261023.md      # 有行号 = 文档已登记
#     §4.5 的 23 张：grep 结果应为 0 行（本报告实测 0）

# 3) 现有可跑场景 = 20 个 JSON − 3 个 blocked（combat_hit / combat_strike / pickup_item）= 17
grep -l '"blocked"' tools/probe/scenarios/*.json          # 应为 3 行
ls tools/probe/scenarios/*.json | wc -l                   # 应为 20
```

> **计数口径的一个实测发现**：`docs/plan-3day-20261024.md` / `needs-human §0.1` / `window-run-2` 卡面写的「18 场景」是
> **封存 `pickup_item` 之前**的数。`582024877`（2026-10-24 晚）把 `pickup_item` 封存为「结构性不可判」后，
> 逐行枚举 `tools/probe/scenarios/` 是 **20 − 3 blocked = 17 个可跑场景**（`combat_hit`/`combat_strike` 仍 blocked）。
> 本报告若写「18」会与事实源冲突；下文一律用 **17**，并在 §7 给出与 run-2 卡面的逐场景对照。

## 2. 分类 A：场景可覆盖（49 张，逐卡有据）

判定规则：卡面判据所需的观察面 ∈ 现有 17 个可跑场景之一，或 ∈ master 上既有 `run_*_pair.ps1`/`card_window.py`
夹具（batch-plan §7.4 / §3.1P / §3.1F / §3.2A / §3.2B）。「来源」列给出 batch-plan 行号；标「新卡」的 4 张是
@ `917376a45` 之后合入、尚无 batch-plan 行的卡，依据取其 review 的 `window_items/tier_why`（已注 review oid 前 8 位）。

**场景分布**：login_only 17 / spell_cast 6 / upgrade_retrieve 4 / npc_click(+auto) 3 / run_hero_pet_pair 3 /
run_party_trade_pair 2 / dice_chain 2 / dice_chain_auto 2 / 其余 10 个场景或夹具各 1。

| 卡 id | 场景 / 夹具 | 来源 | 依据要点 / 备注 |
|---|---|---|---|
| `openmir2-a09-upgrade-item` | 新：R2 upgrade_retrieve | §3 第 3 批（需新帧/新场景/新夹具） L231 | **§7.2/§7.4 刷新后转可跑**；ItemNameList 5 条 + 124B 尾 + 穿戴门槛 |
| `openmir2-account-protection-13` | run_account_protection_roundtrip.ps1 | §3.1F 第 1 批 B（既有 pair 夹具） L145 | 夹具在 mir2-rs/tests/parity/m3/account-protection/（既有）；比建号 13 列 + 改资料 12… |
| `openmir2-attack-weapon-wear-642` | `equip_struck_save`（§7.2 明确移入 §7.4） | §7.2（散文，无表格行） | **§7.2/§7.4 刷新后转可跑**；642 攻击方武器磨损已接线；§7.4 有 `equip_struck_save` 行 |
| `openmir2-buff-system` | 新：R5 spell_cast + DoT trace | §3 第 3 批（需新帧/新场景/新夹具） L180 | **§7.2/§7.4 刷新后转可跑**；毒伤逐次 MIR2_RNG_LOG；buff 递减节奏待 A-R2 裁定 |
| `openmir2-card-window-tool` | card_window.py 真 alt 剖面一次 | §3.1F 第 1 批 B（既有 pair 夹具） L146 | 两侧 stop rc=0、端口全释放、report_kind=compare |
| `openmir2-click-path-bag-writeback` | npc_click + db_snapshot | §3.1P 第 1 批 A（可跑 probe 场景） L120 | 点击 @main take/give 后断线，落库行/列比对 |
| `openmir2-compare-ident53-diff-triage` | spell_cast（复拍 + pin 转发链） | 新卡（本批新增，无 batch-plan 行） | tag=MP 数值/时序 + characters_ablity 落库；窗口=复拍验证修复（review f7fcf33b） |
| `openmir2-db-encoding` | login_only + db_snapshot | §3.1P 第 1 批 A（可跑 probe 场景） L114 | ChrName 列值（建号/选角/存档） |
| `openmir2-delmagic-addmagic-chain` | bag_equip | §3.2B 第 2 批 B（新发现规则） L159 | SM_BAGITEMS(201) 发现规则**已落地**（`openmir2-discovery-bagitems-201`，构造级 `c… |
| `openmir2-encoding-core-e1-e2` | npc_click / merchant_dialog | §3.1P 第 1 批 A（可跑 probe 场景） L110 | 非 ASCII 说词逐字节；BOM 两路属离线（见不可对拍） |
| `openmir2-gamesvr-serverinfo-report` | login_only | §3.1P 第 1 批 A（可跑 probe 场景） L124 | 必跑 ident 529 逐字节（body_hex 对照） |
| `openmir2-gm-impl-shutup` | login_only + 手动 @禁言 | §3.1P 第 1 批 A（可跑 probe 场景） L127 | 前置 openmir2-gm-script-host-seam 已合（只通 @Login 路径） |
| `openmir2-gm-script-host-seam` | login_only + 手动 @禁言 <名> 1 | §3.1P 第 1 批 A（可跑 probe 场景） L128 | 探针不发 GM 命令，runbook 要求操作者手输；双侧比「权限不够!!!」 |
| `openmir2-guard-ctor-constants` | login_only | §3.1P 第 1 批 A（可跑 probe 场景） L118 | 卫士自扫半径 5→7 的同 tick 可见集/事件序 |
| `openmir2-guild-g2-dispatch` | login_only | §3.1P 第 1 批 A（可跑 probe 场景） L109 | 750 出队序（46 之后、654 之前） |
| `openmir2-healthspell-emission-model` | 新：R5 spell_cast | §3 第 3 批（需新帧/新场景/新夹具） L211 | **§7.2/§7.4 刷新后转可跑**；整 tick 53 序列 + SKILL_SHOWHP |
| `openmir2-login-529-status-text` | login_only | §3.1P 第 1 批 A（可跑 probe 场景） L121 | 前置 gamesvr-serverinfo-report 已合入 master 185a6c8cb（SS_SERVERINFO 上报已接）… |
| `openmir2-login-654-order` | login_only | §3.1P 第 1 批 A（可跑 probe 场景） L112 | 654 跨 tick、第二 42、100×4；有会角色 811/46/750 |
| `openmir2-login-charstatus-657` | login_only | §3.1P 第 1 批 A（可跑 probe 场景） L115 | 657×2 与邻居相对序 |
| `openmir2-login-frame-order-52-752` | login_only | §3.1P 第 1 批 A（可跑 probe 场景） L107 | 52 布局(54,78)/621/211 相对序 |
| `openmir2-login-numeric-gaps` | login_only（首杀项另需 combat_hit） | §3.1P 第 1 批 A（可跑 probe 场景） L108 | 首登 52 Weight/MaxExp=10 可先验；44/45/52/752 首杀序需第 2 批 |
| `openmir2-m3-party-attribution` | run_party_trade_pair.ps1 | §3.1F 第 1 批 B（既有 pair 夹具） L137 | 组队归属/分赃：前置 ⑦-2/⑦-3（卡面称待合，开工前复核） |
| `openmir2-m3-party-gainexp-npe` | run_party_trade_pair.ps1 | §3.1F 第 1 批 B（既有 pair 夹具） L138 | 同上 |
| `openmir2-m3-pet-slave-clone46` | run_hero_pet_pair.ps1 | §3.1F 第 1 批 B（既有 pair 夹具） L141 | make_clone step |
| `openmir2-m3-pet-slave-lifecycle` | run_hero_pet_pair.ps1 | §3.1F 第 1 批 B（既有 pair 夹具） L139 | 脚本已在 master；前置 ⑨-2/⑨-3 实现 |
| `openmir2-m3-pet-slave-masterdie-wire` | run_hero_pet_pair.ps1 | §3.1F 第 1 批 B（既有 pair 夹具） L142 | 主人死 step + 混合队列 |
| `openmir2-m3-rng-scene-driver` | run_rng_scene_pair.ps1 | §3.1F 第 1 批 B（既有 pair 夹具） L143 | W9 RNG 序列 |
| `openmir2-m3-shop-dialog-wiring` | run_npc_shop_pair.ps1 | §3.1F 第 1 批 B（既有 pair 夹具） L140 | ⑤-4 商店派发/帧；storageitem 50 槽 |
| `openmir2-market-def-impl` | merchant_dialog | §3.1P 第 1 批 A（可跑 probe 场景） L113 | @main/[~@repair] 27 处 $STR 展开 |
| `openmir2-mb8-9-monsters` | 新：R5 spell_cast race 215 | §3 第 3 批（需新帧/新场景/新夹具） L227 | **§7.2/§7.4 刷新后转可跑**；需先接 T-148 |
| `openmir2-menu-ok-unify-pickwire` | dice_chain_auto | §3.1P 第 1 批 A（可跑 probe 场景） L126 | 767 分支；回稿后同场景复跑 |
| `openmir2-mp-session-projection-clobber` | spell_cast | 新卡（本批新增，无 batch-plan 行） | 53 帧两帧 15/13 + 落库 13；窗口=复拍验证修复（review ac6a5858） |
| `openmir2-npc-click-frame-seq-fix` | npc_click / npc_click_auto | 新卡（本批新增，无 batch-plan 行） | 帧序 41/643 与 index 29 错位；属 RNG 流位 flaky 类（见 §6 Q4） |
| `openmir2-npc-ctor-constants` | login_only | §3.1P 第 1 批 A（可跑 probe 场景） L117 | SM_TURN series 低=4 高=2、Merchant 格 30s 续期 |
| `openmir2-npc-display-table` | login_only + npc_click | §3.1P 第 1 批 A（可跑 probe 场景） L116 | 同摆位 0122 (29,33) 出现帧/3 对 nameless 转身/点击 @main |
| `openmir2-outbound-encoders-red-batch` | 新：R2 upgrade_retrieve | §3 第 3 批（需新帧/新场景/新夹具） L178 | **§7.2/§7.4 刷新后转可跑**；制药链 Random 流水 + 帧相对序 + MYSTATUS 落库 |
| `openmir2-script-playdice-openybdeal` | dice_chain | §3.1P 第 1 批 A（可跑 probe 场景） L111 | 赌博 NPC playdice 标签放行；OPENYBDEAL 三支部署不可达另记 |
| `openmir2-script-take-persist` | dice_chain + db_snapshot | §3.1P 第 1 批 A（可跑 probe 场景） L119 | @main→@kaiz→@ya5→@ya52 链正是 dice_chain；比 characters_bagitem/Gold/useit… |
| `openmir2-searchtick-origin-parity` | login_only（--dwell-ms ≥2000） | §3.1P 第 1 批 A（可跑 probe 场景） L122 | 抓首 SM_TURN/41 与 654/657×2 的到达 tick 窗口 |
| `openmir2-spell-fill-projection-mirror` | spell_cast + eat_item | 新卡（本批新增，无 batch-plan 行） | 恢复交错序列 + CM_EAT 药品/死亡拒绝复拍（review 4093b152） |
| `openmir2-t138-request-castle-war` | king_castle_war | §3.2A 第 2 批 A（新发现规则） L153 | king_actor 需新规则（643 规则 + 标签约束，待实测确认） |
| `openmir2-transmap-maptrigger` | move_walk | §3.2A 第 2 批 A（新发现规则） L152 | move_x/move_y 需新规则（self_x/self_y + 场景声明 delta） |
| `openmir2-unbindlist-consumer` | eat_item | §3.2B 第 2 批 B（新发现规则） L160 | 规则同 `bag_equip`（已落地，构造级）；另需 StdMode31/AniCount0 捆装包夹具 |
| `openmir2-upgrade-npc` | 新：R2 upgrade_retrieve | §3 第 3 批（需新帧/新场景/新夹具） L216 | **§7.2/§7.4 刷新后转可跑**；抽数序 A→B→C/D/E + Desc[10] + 帧序 |
| `openmir2-verify-tick-base` | login_only | §3.1P 第 1 批 A（可跑 probe 场景） L123 | 判据实质待 T-73 登录等待环建模；今日生产路径无可观测差异 |
| `openmir2-weapon-upgrade-values` | 新：R2 upgrade_retrieve | §3 第 3 批（需新帧/新场景/新夹具） L232 | **§7.2/§7.4 刷新后转可跑**；@upgradenow 四字段 + n90 逐抽 |
| `openmir2-window-collector-preflight` | card_window.py 真栈一次（login_only） | §3.1F 第 1 批 B（既有 pair 夹具） L144 | 起栈后 INJECT_OK/RUST_SEED_INJECTED 交叉断言 |
| `openmir2-world-tick-expansion` | login_only + MIR2_RNG_LOG | §3.1P 第 1 批 A（可跑 probe 场景） L89 | RNG trace 双开；npc_tick 次序已写进契约 |
| `openmir2-ya52-menu-ok-missing` | dice_chain_auto | §3.1P 第 1 批 A（可跑 probe 场景） L125 | 末步断言 SM_MENU_OK(767) |

> **A 类里必须带进窗口 runbook 的 4 个「部分可验」**：`gm-impl-shutup` / `gm-script-host-seam` 需操作者手输 `@禁言`；
> `login-numeric-gaps` 的首杀序（44/45/52/752）另需 combat_hit（blocked），本轮只能验首登 52 Weight/MaxExp；
> `verify-tick-base` 卡面自述「今日生产路径无可观测差异」（建议 Q5 一并裁定为登记/关闭，见 §6）。
> `spell_cast` 6 张统一前置：profile 带 `magic` 段 **且** `sides.rust.env.MIR2_CLIENT_ABILITY_LAYOUT="0"`。

## 3. 分类 C：已有登记可豁免（26 张，逐卡有据）

判定规则：batch-plan §4「不可协议对拍」已为卡登记了替代判据（不做协议双栈逐帧对拍），或卡本身就是窗口执行记录。
**这一类的「豁免」是「免协议对拍窗口」，不等于「免全部证据」**：替代判据（文件字节 sha256 / 离线 trace /
构造级断言 / 长跑观察）仍要按行内口径产出。是否认可这 25 张的替代判据为条件② 的充分证据，是 owner 的选择题（§6 Q5）。

| 卡 id | 替代判据 / 豁免依据 | 来源 | 裁定入口 |
|---|---|---|---|
| `openmir2-a22-castle-persistence` | 文件字节 sha256 + 10min Save + 跨午夜；离线文件；窗口项：W-A22-1 文件字节：C#/Rust 各跑启动+一次 10min Save… | §3 第 4 批（不可协议对拍） L278 |  |
| `openmir2-chat-channels` | 三前缀收件人集合 ⇒ 多会话；窗口项：真机：三前缀的收件人集合与帧字节；10s 门边界（严格 >）（共 3 项） | §3 第 4 批（不可协议对拍） L269 |  |
| `openmir2-dbsrv-fault-paths` | 需停/重启 MySQL（纪律禁止）；替代=故障注入夹具；窗口项：真机 MySQL 停机/重启下的四种降级帧与客户端表现；9 表与 characters_ind… | §3 第 4 批（不可协议对拍） L266 |  |
| `openmir2-dbsrv-scheme-a` | 慢存档时延与 5100 全序；工具缺口；窗口项：真机 W-DB-1..7：慢存档下的 QUERYCHR/SELCHR/LOAD 时延；5100 回包全序（共 … | §3 第 4 批（不可协议对拍） L271 |  |
| `openmir2-encoding-crate` | data/envir 文本逐字节 + BOM 两模型；离线语料；窗口项：真机：data/envir 承载的文本（行会文件/物品名/配置）逐字节；不可映射字符与… | §3 第 4 批（不可协议对拍） L274 |  |
| `openmir2-gate-idle-timeout` | 真实客户端挂起 60s；长跑/客户端驱动缺口；窗口项：真实客户端挂起 60s 的被踢时序与畸形超时帧的实际反应；下行断链后网关的完整恢复路径（重连未实现，另卡） | §3 第 4 批（不可协议对拍） L265 |  |
| `openmir2-gategate-keepalive` | 10s 健康帧/15 天空闲；长跑；窗口项：真机：10s 健康帧周期与首帧相位；3min 窄竞态分支的可达性（需 owner 裁决）（共 3 项） | §3 第 4 批（不可协议对拍） L270 |  |
| `openmir2-gategate-slots-gm` | GameGate 多进程 + GM_CHECKCLIENT/RECEIVE_OK…；窗口项：C# GameSvr + Rust GameGate 的 GM_C… | §3 第 4 批（不可协议对拍） L264 |  |
| `openmir2-gm-batch-4-numeric` | T-187 两号同时在线比目标侧帧；单号可先验 T-186（gm_batch 不…；窗口项：接 SessionGround 数值态（不是 Entity；Ent… | §3 第 4 批（不可协议对拍） L282 |  |
| `openmir2-guild-g3-persistence` | 两在线会话一方断线 + 文件字节；多会话 + 文件；窗口项：窗口 A：两个在线行会成员一方断线，对端恰收一条 104（Recog/Param/Series/G… | §3 第 4 批（不可协议对拍） L268 |  |
| `openmir2-hutil32-parity` | b1_six_replay 的 NaN/／x／≥2^31 分支；离线夹具；窗口项：b1_six_replay 的 round 在 NaN/／x／≥2^31 分… | §3 第 4 批（不可协议对拍） L273 |  |
| `openmir2-m3-clock-injection` | C# HUtil32.GetTickCount/DateTime.Now 不可替…；窗口项：剩余 DoD = ① C# 影子 hook 半（HUtil32.G… | §3 第 4 批（不可协议对拍） L257 |  |
| `openmir2-m3-monster-active-seek` | GUI/夹具驱动，非协议对拍；替代=W0/W9 夹具 + G3 窗口；窗口项：战斗数值/时序/RNG 真机对拍（W0/W9）。approve.window_r… | §3 第 4 批（不可协议对拍） L258 |  |
| `openmir2-monster-ref-drains` | 四类帧收件人集合 ⇒ 多会话；窗口项：真机四类帧逐字节与收件人集合（需先让 race 203/206/210/215 出现在部署数据）；Khazard 玩家目… | §3 第 4 批（不可协议对拍） L279 |  |
| `openmir2-monster-showhp-53-broadcast` | ±12 帧集合 + VisibleHumanList 缓存 ⇒ 多会话；窗口项：ShowHp 怪/NPC ±12 帧集合、53/1100/1101 同 tic… | §3 第 4 批（不可协议对拍） L285 |  |
| `openmir2-npc-hit-frames` | ≥180s 观察 + ±12 收件人集合；窗口项：C# 本尊正面样本 ≥180s（同位置必须出现 ident=14 recog=国王 actor）；帧形逐字节… | §3 第 4 批（不可协议对拍） L280 |  |
| `openmir2-openybdeal-host-wiring` | 掷骰/寄售面板与客户端动画；GUI；窗口项：W-SPD-1 真机掷骰：playdice 1 @cube82 → 客户端出动画（SM_PLAYDICE 8001… | §3 第 4 批（不可协议对拍） L276 |  |
| `openmir2-orphan-encoder-wiring` | ②/③ 需两真客户端 + ±12 收件人集合，单客户端验不到；替代=构造级收件人…；窗口项：① 玩家/怪物走出视野时旧位置幽灵应消失（原缺口症状）；② 两真客… | §3 第 4 批（不可协议对拍） L259 |  |
| `openmir2-selgate-reconnect` | 需 DBSrv 重启；工具缺口；窗口项：真机 DBSrv 重启后 SelGate 的重连与在途会话；断链期间真实客户端的表现 | §3 第 4 批（不可协议对拍） L267 |  |
| `openmir2-single-binary-roles` | RNG trace groups + 日志逐行；离线日志；窗口项：真实 game-svr（MIR2_RNG_LOG + MIR2_RNG_SEED，start… | §3 第 4 批（不可协议对拍） L277 |  |
| `openmir2-sm-closehealth-wire` | ±12 观察者 53/1101 集合 ⇒ 多会话；窗口项：玩家目标与怪物目标各跑一次：心灵启示 → 1100 出现 → ShowHpInterval 到点 →… | §3 第 4 批（不可协议对拍） L281 |  |
| `openmir2-sm-reconnect-wiring` | 换服/重连帧面；重连未实现 + 工具缺口；窗口项：真机换服/重连帧面 + GateCount==1 的 RNG 流水。残余：M-2 nServerIndex … | §3 第 4 批（不可协议对拍） L284 |  |
| `openmir2-sm-skeleton-wiring` | ±12 收件人集合 + 12 格边界各放静止号 ⇒ 多会话；窗口项：双栈同一账号连续屠宰「鹿(150)/鸡(50)」直到出骨架，判 SM_SKELETON(3… | §3 第 4 批（不可协议对拍） L286 |  |
| `openmir2-strlist-bom` | BOM 行会/物品/列表文件；离线夹具；窗口项：真机：带 BOM 的行会/物品/列表文件读取 | §3 第 4 批（不可协议对拍） L275 |  |
| `openmir2-window-run-1` | 窗口执行记录卡：`docs/window-run-1-20261024.md` + `E:/tmp/window/run1/` 18 场景矩阵 | 新卡（本批新增） | 该卡的交付物就是 run-1 真机报告；不再作为代码窗口欠账（需 owner 认可，见 §6 Q5） |
| `openmir2-ybdeal-frames` | 部署 OPENYBDEAL=0/QUERYYB=0 ⇒ 部署级不可达；替代=登记…；窗口项：可达性实测 OPENYBDEAL=0/QUERYYB=0（四条部署… | §3 第 4 批（不可协议对拍） L283 |  |

> `window-run-1` 是**执行卡**：交付物就是 `docs/window-run-1-20261024.md`（18 场景矩阵：PASS 1 / FAIL 5（快照 @ `efb8c9314`）/ UNDECIDABLE 12）
> 与 `E:/tmp/window/run1/` 证据根。它不是「代码等窗口」的卡，本报告按已有登记归 C；若 owner 认为执行卡应改判 L 或 closed，
> 属状态操作，本卡不代改（只在这里登记）。

## 4. 分类 B：无场景需裁定（62 张，逐卡有据）

判定规则：现有 17 个可跑场景与既有夹具**都不能**产出卡面判据（场景 blocked / 场景不可达 / 场景未落地 / 只是依赖引用 /
batch-plan 完全无登记）。B 类内部按「缺什么」再分 6 组；每张都给候选场景与缺口，**不代 owner 裁定值不值得补**。

### 4.1 场景 blocked（5 张）

| # | 卡 id | 计划场景 / 候选场景 | 来源（batch-plan / 报告） | 缺什么 / 依据 |
|---:|---|---|---|---|
| 1 | `openmir2-merchant-race-bias` | 新：combat_hit（HAM_* 模式打商人） | §3 第 3 批 L230 | 对照 race 50；RNG 少一次 Random(SpeedPoint) |
| 2 | `openmir2-monster-attack-hit-frame` | 新：combat_hit（同点 605,578 / L1 / 60s） | §3 第 3 批 L210 | 14/31 相对序与 AttackDir |
| 3 | `openmir2-run-gap-pl1` | 新：combat_hit + 存档 | §3 第 3 批 L209 | P21 帧序 / P32 复活链 / 53 同 tick |
| 4 | `openmir2-superman-normnpc` | 新：combat_hit + GM Kill | §3 第 3 批 L227 | 城门/守卫/固定 NPC 的 SuperMan 行为 |
| 5 | `openmir2-weapon-durability-frames` | 新：combat_hit + 装备耐久 | §3 第 3 批 L242 | 655/1102 帧面 |

### 4.2 场景不可达（4 张）

| # | 卡 id | 计划场景 / 候选场景 | 来源（batch-plan / 报告） | 缺什么 / 依据 |
|---:|---|---|---|---|
| 1 | `openmir2-castle-guard-target` | 新：R6 castle_zone | §3 第 3 批 L218 | 同城/盟友/UnderWar 三态 |
| 2 | `openmir2-castle-merchant-disappear-downlink` | 新：R6 castle_zone 城堡战 | §3 第 3 批 L239 | ⑧ 战中帧集合 diff=0 |
| 3 | `openmir2-castle-merchant-fixedhide-hit` | 新：R6 castle_zone 城堡战 | §3 第 3 批 L238 | ⑧ 战中帧集合 diff=0 |
| 4 | `openmir2-castle-war-area-impl` | 新：R6 castle_zone | §3 第 3 批 L219 | 766 bit4/1s 门/四臂落点 |

### 4.3 文档已登记、需新场景 / 新帧 / 新夹具（28 张）

| # | 卡 id | 计划场景 / 候选场景 | 来源（batch-plan / 报告） | 缺什么 / 依据 |
|---:|---|---|---|---|
| 1 | `openmir2-action-line-variable-text` | 新：R1 npc_script_chain（SENDMSG/MESSAGEBOX 支） | §3 第 3 批 L233 | 真机 C# 跑部署现成素材 `E:/MirServer/Mir200/Envir/QFunction-0.txt:388` 的 `SENDM…… |
| 2 | `openmir2-castle-entities` | 新：castle fixture（16 实体/96 抽） | §3 第 3 批 L217 | 部署 A 情景：启动后 0 实体、4 条告警（Castle/Castle/SabukW.txt）；夹具 B：16 实体、24 次 Regen、… |
| 3 | `openmir2-castle-pure` | 新：castle fixture（商人税三窗） | §3 第 3 批 L225 | W1 沙巴克商人买/卖/修/升级后 TotalGold/TodayIncome 增量；W2 城主行会成员价（共 3 项） |
| 4 | `openmir2-cm-adjust-bonus` | 新：adjust-bonus 场景（752→52→657→252） | §3 第 3 批 L241 | 真机对拍 752→52→657→252 的字节、条数与次序（安静队列前提）。残余：lib.rs:3476 旧注释 RM_ABILITY(19)… |
| 5 | `openmir2-door-chain-wiring` | 新：CM_OPENDOOR 场景 | §3 第 3 批 L237 | ① C# oracle 真机：部署图 `[0 比奇省]`（0.map，700×700，79 门/21 状态）对真实门格发 `CM_OPEND…… |
| 6 | `openmir2-frame-drain-order-audit` | 新：5 个场景（施法/交易/寄售/组队/NPC） | §3 第 3 批 L246 | 5 场景，gw-capture 可复跑）：施法多条 SM_STRUCK(31) 序 [0,2,1]/[0,3,2,1]、交易 4 条红字(25… |
| 7 | `openmir2-gategate-speed-rules` | 新：login 相位场景 | §3 第 3 批 L212 | 登录包下发相位：oracle 入 MsgList 后到 drain 才 Send（0-210ms 不确定），Rust 仍立即下发——真机同相…… |
| 8 | `openmir2-gm-admin-mode-entity-seam` | 新：GM 5 命令场景（@3/@2 等） | §3 第 3 批 L240 | ①-④）**：5 命令真机帧面/行为、@3 挨打零低血 53 + 登出满血存档、改档→登出→复登记录比对、@2 RM_DISAPPEAR。残余… |
| 9 | `openmir2-gm-commands-design` | 新：GM 特权命令场景（SETPERMISSION=10）+ 多会话 | §3 第 3 批 L234 | 真机 SETPERMISSION=10 + 戴传送装备（T-169 前置）跑 @传送：两次调用间隔卡在 UserMoveTime*1000+…… |
| 10 | `openmir2-guild-official-npc` | 新：R1 npc_script_chain | §3 第 3 批 L222 | 真机：建会成功/失败各支帧字节与 622 负重；宣战战表与 104 文案（共 3 项） |
| 11 | `openmir2-loader-fields` | login_only（部分）+ 新加载器接线 | §3 第 3 批 L220 | 真机：StartPoint/Server.conf 11 键的取值与落点；接线后 game-svr 消费新加载器（当前仍用旧三列解析） |
| 12 | `openmir2-m3-castle-gold-tax` | 新：10min Save 文件 + 时钟注入 | §3 第 3 批 L176 | 剩余 DoD = ① 真机两侧「同一 10min SaveTick 时间门 → TodayIncome/TotalGold 落盘」diff=0… |
| 13 | `openmir2-m3-monster-ai-tick` | 新：主动索敌观察（W0 前置） | §3 第 3 批 L177 | 主动索敌（新卡，W0 前置）+ Animal RNG + 守卫静止 + 真机对拍。**2026-10-24 订正：已合入 master（最新 … |
| 14 | `openmir2-map-event-frames` | 新：火墙进出 5 格场景 | §3 第 3 批 L244 | 真机双栈火墙进出 5 格比对 804/805 字节与到达窗口 + PileStones 折叠槽提示 +「采 804 前确认没挖过矿/没先建过任… |
| 15 | `openmir2-maptrigger-monster-and-door` | 新：怪物走 route / 门格 | §3 第 3 批 L179 | W-R1-a：pin 一张「怪物路线上的 route 格」真图，两侧同驱怪物走位 ⇒ 判据=两侧怪物坐标均停在走前格（无位移帧）；W-R1-b… |
| 16 | `openmir2-mb2-monsters` | 新：world_observe 三怪 | §3 第 3 批 L223 | 真机：三怪 Run 的分支与帧（数据可达时） |
| 17 | `openmir2-mb5-monsters` | 新：world_observe 五类怪 | §3 第 3 批 L221 | 真机：五类怪的 Run 分支与帧（数据不可达时以构造级为准）；FrostTiger 657 帧在 host drain 接通后观察 |
| 18 | `openmir2-mb7-monsters` | 新：world_observe race 113/114/134 | §3 第 3 批 L224 | 真机：race 113/114/134 的 Run/克隆/召唤（部署数据不可达时以构造级为准）；的展示色与排水相位 |
| 19 | `openmir2-monster-walk-frames` | 新：world_observe 60–180s | §3 第 3 批 L226 | 真机重采怪物走位 SM_WALK 条数/时序（C# 基线 17–76 条）；同 tick 多步、±12 边界、传送误判（T-146 四条残余） |
| 20 | `openmir2-npc-def-loading` | 新：R1 npc_script_chain（国王 0122/老人 3） | §3 第 3 批 L229 | 点比奇国王(0122)/老人(3) 的说词面板与菜单可点性（@main 原文逐字）；@@buildguildnow/@ask/@back/@e… |
| 21 | `openmir2-pickwire-remaining-frames` | 新：R1 npc_script_chain | §3 第 3 批 L245 | ①三条脚本帧真机逐字节；②CHANGEEXP ⇒ Recog=加完后总量（依赖 openmir2-script-changexp-exp-wr… |
| 22 | `openmir2-run-gap-mb4` | 新：world_observe race 92 | §3 第 3 批 L208 | race 92 真机：SpaceMove/MapRandomMove 落点与帧序、狂暴 500/400 节拍；SiegeLockCount 的… |
| 23 | `openmir2-run-gap-pl2` | 新：R1 npc_script_chain | §3 第 3 批 L214 | 内容启用后的 P31 结算与 44/升级链帧序；SETAUTOGETEXP 两态在真机脚本链上的可观测行为 |
| 24 | `openmir2-run-gap-pl3` | 新：R1 npc_script_chain（内容启用支） | §3 第 3 批 L213 | 内容一旦启用（DECHP/INCHP/HungerSystem/KILLMONEXPRATE/POWERRATE）时的真机数值与帧序；197/… |
| 25 | `openmir2-run-gap-pl4` | 新：R1 npc_script_chain | §3 第 3 批 L215 | 653 同 tick 相位；会话 HomeMap（GetStartPoint）四条分叉（共 3 项） |
| 26 | `openmir2-sendrefmsg-range-config` | 新：部署改 15 后三场景 | §3 第 3 批 L243 | 部署改 15 双栈三场景对拍。残余：crates/world 两处仍用常量 12（需 World 配置通道，另开卡）；oracle 缺省确认为… |
| 27 | `openmir2-setmapmode-dechp` | 新：脚本夹具（SETMPAMODE DECHP） | §3 第 3 批 L236 | ① 真机 SETMPAMODE DECHP 全量对拍：`now-DecHpTick > nDECHPTIME*1000` 秒门 + HP-=…… |
| 28 | `openmir2-setoffline-field-writeback` | 新：脚本夹具（@OnLogout_OffLinePlaying） | §3 第 3 批 L235 | ① SETOFFLINE → P31：构造 QFunction-0.txt 的 @OnLogout_OffLinePlaying（`OffL…… |

### 4.4 文档仅作依赖引用（1 张）

| # | 卡 id | 计划场景 / 候选场景 | 来源（batch-plan / 报告） | 缺什么 / 依据 |
|---:|---|---|---|---|
| 1 | `openmir2-script-changexp-exp-writeback` | 并入 R1 `npc_script_chain`（建议） | 仅作依赖引用 L245 | 补进 R1 `npc_script_chain` 批次表，或登记为不可对拍 |

### 4.5 文档完全未登记（23 张，batch-plan grep=0 实测）

**GM 命令族 — 8 张**

| # | 卡 id | 候选场景 | 来源 | 窗口项依据（卡面原文节选） |
|---:|---|---|---|---|
| 1 | `openmir2-gm-impl-clearbag` | 新：GM 特权命令场景（SETPERMISSION=10 / DB 后态 / 多会… | batch-plan 无登记；前序报告 §4.5 | 目标背包清空（持久化面）+ 玩家可见 709 帧；R 级真机窗口未跑。本轮只核我自己上轮 review daa2368… |
| 2 | `openmir2-gm-impl-kick` | 新：GM 特权命令场景（SETPERMISSION=10 / DB 后态 / 多会… | batch-plan 无登记；前序报告 §4.5 | GM 踢人命令体 + 宿主 528/离场/踢前存档；R 级真机窗口未跑。本轮只核复审 61b952f9 的 2 imp… |
| 3 | `openmir2-gm-impl-mobfireburn` | 新：GM 特权命令场景（SETPERMISSION=10 / DB 后态 / 多会… | batch-plan 无登记；前序报告 §4.5 | GM MobFireBurn 命令体 + 玩家可见帮助/not-found 帧；R 级真机窗口未跑。本轮只核复审 97… |
| 4 | `openmir2-gm-impl-showeffect` | 新：GM 特权命令场景（SETPERMISSION=10 / DB 后态 / 多会… | batch-plan 无登记；前序报告 §4.5 | oracle 命令体 20-70 行整段注释 ⇒ 任何参数零帧零副作用；本卡只落最小 no-op 并证明 Rust 不… |
| 5 | `openmir2-gm-impl-shutup-release` | 新：GM 特权命令场景（SETPERMISSION=10 / DB 后态 / 多会… | batch-plan 无登记；前序报告 §4.5 | 命令体实现 + R 窗口项（真机双栈帧字节/DB 后态）；按 R 判。窗口未跑前卡保持 in-progress。 |
| 6 | `openmir2-gm-impl-smake` | 新：GM 特权命令场景（SETPERMISSION=10 / DB 后态 / 多会… | batch-plan 无登记；前序报告 §4.5 | 装备属性写/Recalc/落库 + 帧序（203 先于 100）；R 级真机窗口未跑。本轮只核复审 41a99a47 … |
| 7 | `openmir2-gm-impl-startquest` | 新：GM 特权命令场景（SETPERMISSION=10 / DB 后态 / 多会… | batch-plan 无登记；前序报告 §4.5 | 命令体逐行等价不变（空名帮助 / 非空零帧 / perm 门）；本轮只核返工的 1 blocking + 1 impo… |
| 8 | `openmir2-gm-script-exec-seams` | 新：GM 特权命令场景（SETPERMISSION=10 / DB 后态 / 多会… | batch-plan 无登记；前序报告 §4.5 | 最终增量：只改 §8/T-211 的窗口措辞与两处卡引用，代码零改动（lib.rs sha 仍 6c156b61…）。… |

**脚本链族 — 9 张**

| # | 卡 id | 候选场景 | 来源 | 窗口项依据（卡面原文节选） |
|---:|---|---|---|---|
| 1 | `openmir2-merchant-script-load` | R1 `npc_script_chain` 扩展（支路/文件落盘/账号名单） | batch-plan 无登记；前序报告 §4.5 | 本片把两个「只登记、不执行」分支改为真实执行 GotoLable，改变客户端可见帧（SM_MERCHANTSAY(64… |
| 2 | `openmir2-qfunction-hidden-merchant-world-wiring` | R1 `npc_script_chain` 扩展（支路/文件落盘/账号名单） | batch-plan 无登记；前序报告 §4.5 | 触及 RNG 抽数位置与时序：QFunction 进世界 MerchantList 首位并参与 Merchant.Ru… |
| 3 | `openmir2-script-action-var-text` | R1 `npc_script_chain` 扩展（支路/文件落盘/账号名单） | batch-plan 无登记；前序报告 §4.5 | 改变脚本动作/条件参数发给玩家的文本内容（SENDMSG/LINEMSG/MESSAGEBOX/条件参数），属玩家可见… |
| 4 | `openmir2-script-broadcast-tx-gating` | R1 `npc_script_chain` 扩展（支路/文件落盘/账号名单） | batch-plan 无登记；前序报告 §4.5 | 结论是玩家可见全服广播的**收件人集合**在「触发者自己网关线缺席」时是否仍然正确（网络状态触发 + 帧是否发出）；作… |
| 5 | `openmir2-script-dyn-var-host` | R1 `npc_script_chain` 扩展（支路/文件落盘/账号名单） | batch-plan 无登记；前序报告 §4.5 | VAR/CALCVAR/LOADVAR 改变脚本分支走向与变量内容；本轮改的是 CALCVAR / 0 的中止范围（求… |
| 6 | `openmir2-script-envir-root-host` | R1 `npc_script_chain` 扩展（支路/文件落盘/账号名单） | batch-plan 无登记；前序报告 §4.5 | CHECKNAMELIST/CHECKNAMEDATELIST 的判定改变脚本走向；ADDUSERDATE/ADDNA… |
| 7 | `openmir2-script-sendmsg-host` | R1 `npc_script_chain` 扩展（支路/文件落盘/账号名单） | batch-plan 无登记；前序报告 §4.5 | 玩家可见的全服/附近广播 + 收件人集合 + 同 ident 多帧时序（作者自报 R、issue 也是 R）；复审只判… |
| 8 | `openmir2-script-user-account` | R1 `npc_script_chain` 扩展（支路/文件落盘/账号名单） | batch-plan 无登记；前序报告 §4.5 | 脚本行为面：CHECKACCOUNTLIST/DELACCOUNTLIST 的账号条件会改变脚本走向、发奖与名单文件落… |
| 9 | `openmir2-sendmsg-login-qmanage-cases` | R1 `npc_script_chain` 扩展（支路/文件落盘/账号名单） | batch-plan 无登记；前序报告 §4.5 | 补的是玩家可见广播（SM_SYSMESSAGE）在 @Login/QManage 排水点的收件人集合与时序；作者自报 … |

**战斗数值 / RNG 族 — 5 张**

| # | 卡 id | 候选场景 | 来源 | 窗口项依据（卡面原文节选） |
|---:|---|---|---|---|
| 1 | `openmir2-ability-weaponstrong` | combat_hit（blocked）或 equip_struck_save | batch-plan 无登记；前序报告 §4.5 | 改战斗数值（磨损量 `(ushort)(Random(5)+2-AddAbil.WeaponStrong)`）并间接影… |
| 2 | `openmir2-conformance-rng-nondeterminism` | RNG 场景组（R3） | batch-plan 无登记；前序报告 §4.5 | 有 window_required=true 登记 |
| 3 | `openmir2-m3-drop-exp-wiring` | 100 杀 RNG 场景 / 战斗场景 | batch-plan 无登记；前序报告 §4.5 | 本片触及①时序（同一 MsgQueue 的出队序 53→44→45/52/752 与同 tick 53 去重）②持久化… |
| 4 | `openmir2-npc-ctor-draws` | 构造段窗口（启动 trace） | batch-plan 无登记；前序报告 §4.5 | RNG 站点账本 + 构造段窗口判据 |
| 5 | `openmir2-weaponstrong-dura-zero-skip` | combat_hit（blocked）或 equip_struck_save | batch-plan 无登记；前序报告 §4.5 | 改的是 `AddAbil.WeaponStrong` 的装备口径（`Dura==0` 整槽跳过），它直接决定攻击磨损量… |

**客户端 E2E — 1 张**

| # | 卡 id | 候选场景 | 来源 | 窗口项依据（卡面原文节选） |
|---:|---|---|---|---|
| 1 | `openmir2-client-e2e-enter-gaps` | 不可协议对拍（真机 E2E） | batch-plan 无登记；前序报告 §4.5 | 卡级 R（真机时序/帧序/数值）。本片零代码（19 文件全在卡证据目录），审查法 = 钉在 head 36622639… |

### 4.6 新增卡：drop 场景（1 张）

| # | 卡 id | 计划场景 / 候选场景 | 来源 | 缺什么 / 依据 |
|---:|---|---|---|---|
| 1 | `openmir2-dropitem-recog-param-mixup` | 新：drop 场景（CM_DROPITEM 600/601）；pickup 链**已封存** | 本批新增；`582024877` 封存 pickup_item | 改 CM_DROPITEM 的 nParam1 来源（线缆 Recog=MakeIndex，i32 全宽），判 600/601 回帧与查包键；见 review 70af4403 |

**B 类小结**：62 = blocked 5 + 不可达 4 + 需新场景 28 + 仅依赖 1 + 无登记 23 + 新增 drop 1。
其中 **46 张有明确的新场景路线**（4.3 的 28 + 4.4 的 1 + 4.5 的脚本 9 + GM 8），
**15 张卡在夹具/场景不可达**（4.1 的 5 + 4.2 的 4 + 4.5 的战斗/RNG 5 + 4.6 的 1），**1 张 E2E** 只能走不可对拍登记。

## 5. 窗口覆盖表（对应卡面 ②）

「轮」按**开跑前置**切，不按日历切。每轮独立占窗口（40–90 分钟/轮，mysqld 不动、跑完恢复十端口）。

| 轮 | 场景 / 夹具 | 覆盖卡 | 预期证据形态 | 前置 |
|---|---|---:|---|---|
| **W1 现有场景**（run-2 续跑） | 17 场景：login_only / move_walk / upgrade_retrieve / equip_struck_save / bag_equip / eat_item / dice_chain / dice_chain_auto / npc_click / npc_click_auto / king_dialog / merchant_dialog / npc_script_chain / king_castle_war / gm_time / gm_batch / spell_cast；+ 6 夹具：run_account_protection_roundtrip / run_party_trade_pair / run_hero_pet_pair / run_rng_scene_pair / run_npc_shop_pair / card_window.py | **49**（A） | `compare_runs` verdict=PASS + diff=0 + 两侧 `frames.ndjson` + `report.json` + `card-window.json/log`；持久化卡（db-encoding / click-path-bag-writeback / script-take-persist / mp-session / spell-fill）加 `db_snapshot` 九表 diff；RNG 卡（world-tick-expansion / buff-system）加 `MIR2_RNG_LOG` site/order 账本 | 开窗授权；`preflight.py --profile alt` rc=0；Rust 二进制按 head 重建（防旧二进制假绿）；spell_cast 带 `magic` 段 + `MIR2_CLIENT_ABILITY_LAYOUT="0"` |
| **W2 新场景（第一批）** | R1 `npc_script_chain` 扩展（9）、world_observe（5）、castle fixture（2）、GM 特权命令场景（8+1）、单卡新场景（12） | **46**（B3 28 + B4 1 + B5 脚本 9 + B5 GM 8） | 新场景各自的 `card_window` run：协议 diff=0 + 帧字节/收件人集合；文件类（script-envir-root / user-account）加文件 sha256；GM 类加 DB 后态 `db_snapshot` | 场景/夹具开发与评审（R1–R7 模板见 batch-plan §5.3）；部分依赖 `castle fixture` 与脚本素材 |
| **W3 夹具/不可达**（第二批） | combat_hit / combat_strike 解封；R6 `castle_zone`；drop 场景（CM_DROPITEM）；RNG 场景组；构造段窗口 | **15**（B1 5 + B2 4 + B5 战斗/RNG 5 + B6 1） | 解封后同 W1；castle_zone 加多会话收件人集合；drop 加 600/601 回帧与查包键；RNG 加 site/order 账本 | 同屏怪物夹具 + 配装/伤害夹具（Q3）；双行会 DB + 私有根 + 开战钟点 + 多会话；`drop` 场景 |
| **W4 客户端 E2E** | 真机 E2E（客户端进图全链路） | **1**（B5 E2E） | 不可协议对拍登记 + 客户端侧观察 | owner 裁定口径 |
| **C 不占窗口** | 无（替代判据） | **26**（C） | 文件 sha256 / 离线 trace / 构造级断言 / 长跑观察；按 §3 行内 | owner 认可替代判据（Q5） |

**W1 的关卡清单就是 §2 的 49 行**（逐卡逐场景）；W2/W3 的关卡清单是 §4.3/B1/B2/B5/B6 的行。窗口执行完一张卡，
按 AGENTS.md 分级：**R 卡仍不 closed**（保持 `in-progress` 并在 `work` 写明窗口项已跑/待补）。

## 6. owner 裁定包（≤5 个选择题，按可判性收敛）

> 只列「循环内无权自决」的。每题给选项 + 推荐；推荐理由都绑定到上面的卡数。

**Q1｜W1 窗口授权（决定 49 张）**
- A（推荐）：按 §5 W1 全量开一轮：17 场景 + 6 夹具，40–90 分钟。→ 直接给 49/137（35.8%）产出真机证据。
- B：最小集先跑 `login_only + upgrade_retrieve + equip_struck_save + spell_cast`（≈ 31 张的骨架），其余下轮。
- C：暂不开窗，先做不占窗口的清理（Q2–Q5 + rules.json/T-* 复核）。
- **推荐 A**；若只有 1 小时窗口，退 B（B 的 4 个场景覆盖登录序/升级/磨损/施法四类最高频判据）。

**Q2｜无场景 62 张的处置口径**
- A：全部补场景（62 张，约 4 轮开发）。
- B：全部登记「不可协议对拍」（条件② 靠登记型 PASS 收口）。
- C：把 62 张从条件② 分母剔除（改判据口径）。
- D（推荐）：**混合分档**——46 张补场景（W2，路线已在 batch-plan §5.3）；15 张视 Q3 解封（W3，解不开就转 B）；1 张 E2E 登记。
- **推荐 D**：它保留「可判的先判」，只对真不可达的少数走登记，避免 B 的「一刀切放弃」和 A 的「为 15 张不可达卡无限投入」。

**Q3｜combat 夹具投入（决定 10 张直接 + 20+ 张间接）**
- A（推荐）：开卡补「同屏怪物夹具 + 伤害/配装夹具」解封 `combat_hit` / `combat_strike`（§4.1 的 5 张 + §4.5 战斗/RNG 5 张直接受益；§4.3 的 world_observe/战斗类间接受益）。
- B：登记不可对拍，接受构造级证据。
- C：只补「同屏怪物」一半（解 `combat_hit`，`combat_strike` 继续 blocked）。
- **推荐 A**：这是 62 张 B 类里唯一「一个夹具换 10 张卡」的杠杆；C 只能拿到 31 帧面，拿不到伤害数值。

**Q4｜跨栈 RNG 流位 flaky 类口径（影响 npc_click 与 login Weight）**
- 甲：窗口口径改「结构等价 + N 跑方差」（放弃逐帧 diff=0）。
- 乙：双栈 RNG 流位锚点同步（M3 clock/seed 同步扩展，另批实现）。
- 丙（推荐）：**本轮按已知 flaky 登记**（D-13 口径：不逐帧比、不据此抑制分支），把乙另开卡。
- **推荐 丙**：`npc-click-frame-seq-fix` 与 `login_only` 的 Weight 14vs0 都不是「Rust 多写代码」类缺陷；
  用甲会降低判据强度，用乙会把窗口拖成实现批次。先登记、保窗口推进，乙单独排期。
- 附注：`login_only` 在 run-2 是 FAIL（Weight 14vs0，归 `login-numeric-gaps`），**它不是 flaky**，是真实数值缺口，W1 仍要跑并应转 PASS。

**Q5｜「登记型豁免」的口径（决定 C 类 26 张与执行/工具卡）**
- A（推荐）：认可 §3 的 25 张「不可协议对拍」替代判据为条件② 的充分证据；`window-run-1` 按执行记录收口；
  `verify-tick-base`（A 类但生产路径无可观测差异）一并裁定为登记/关闭。
- B：保留在窗口欠账分母，继续按 `window_required` 记账。
- C：逐卡裁定（26 张每张一句话）。
- **推荐 A**，但**必须保留 `goal_gate` 的卫生清单口径**：186 张「登记型 PASS ≠ 真机证据」这句话不能删，
  否则条件② 会从「证据齐」退化成「登记齐」。

## 7. 与 `openmir2-window-run-2` 卡面预期的一致性（对应 acceptance ②）

run-2 卡面（issue `bb330d432`）预期「18 场景」，并在 `status 89a07872a` 记录了已完成的有效证据：
`login_only` FAIL（唯一差 Weight 14vs0 → `login-numeric-gaps`）、`npc_click_auto` PASS（30/30 diff=0）。
本报告的 W1 与该卡面的差异只有一处，且是**事实源推进**造成的：

| 场景 | run-2 卡面 | 本报告 W1 | 说明 |
|---|---|---|---|
| `pickup_item` | 在 18 场景内 | **不在 W1** | `582024877` 已封存为「结构性不可判」（T-19/T-25：oracle 无 `MapItem.ItemId` 生产赋值点，地面永无物）；跑它只会得到结构性 UNDECIDABLE |
| 其余 17 | 在 18 场景内 | 在 W1 | 逐场景一致（含 `spell_cast` 复拍、`gm_time/gm_batch` 掩码后 PASS 预期、`bag_equip/eat_item` 解锁、`upgrade_retrieve` 709 预期） |

⇒ W1 = run-2 的续跑，但**按 17 场景排**，不是 18。run-2 卡面「18」是封存前的旧数；建议 Leader 在 run-2 续跑时
以本表为准（本卡不改 run-2 卡片状态，只在此登记）。run-2 已跑完的 2 个场景（login_only FAIL / npc_click_auto PASS）
不重跑，W1 从剩余 16 个起跑（其中 `pickup_item` 已封存，故实际 15 个待跑场景 + 6 个夹具）。

## 8. 判不准 / 残余 / 残留物清单

**判不准（已如实标注，未硬分类）**：

- §2 A 类的「能跑」按 batch-plan §7.4 枚举 + 既有夹具判定，**没有逐张重跑 `card_window --dry-run`**；
  §1F 的 pair 夹具前置（⑦-2/⑦-3、⑨-2/⑨-3）若未落地，该卡实际仍跑不了——开窗前按 §5 前置逐行复核。
- §4.5 的 23 张分组依据是卡名族 + 卡面 `window_required` 理由（前序报告 §4.5 口径），**未逐张重读线程全文**；
  个别卡（尤其 `client-e2e-enter-gaps`）可能应归 C（不可对拍），已在 Q5 给裁定入口。
- `openmir2-dropitem-recog-param-mixup` 归 B（需 drop 场景）而不是 A：它的窗口项原文是「真机 pickup 链对拍」，
  而 `pickup_item` 已封存；若 owner 认可「CM_DROPITEM 的 600/601 回帧」可用现有 `drop` 路线验，可转 A。
- `openmir2-window-run-1` 归 C 是「执行卡不算代码窗口欠账」的判断，需 owner 在 Q5 认可；若 owner 不认可，应转 B。

**未做**：未改 `goal_gate.py`、未改 batch-plan/场景/规则表/whitelist、未起栈、未连库、未改任何卡片状态。
本报告是**排期与裁定材料**，不代替窗口执行；「A 类 49 张」≠「已真机验证」。

**残留物**：无。仅在 `E:/UserTemp`（`$TEMP`）下保留本次复算的 `gate-dev1.json`（232 KB，可删）；仓库内只新增本文件。
