# 窗口欠账重分档（2026-10-23）

- 卡：`openmir2-window-debt-reclass`（owner `openmir2-worker-30`，tier=L，write_set = 本文件）
- 输入：`docs/conformance-a-card-scenario-map-20261023.md`（152 张 A 卡的既有分类，**本文件在它基础上细化、不重做**）；
  每张卡的线程 `walgit collab thread <card-id>`（review `window_items` 原文 / `status.work` 原文）；
  `tools/conformance/rules.json` + `tools/conformance/README.md`（降噪与判定口径）；
  `tools/probe/scenarios/*.json`（现有场景，**以目录实际为准**）。

## 0. 一句话结论

- **① 现有场景已能覆盖 60 张**（直接进下一轮窗口的批量收口池）；
- **② 只差一个新场景 43 张**，按「同一配方能一起覆盖几张卡」聚合后**只需 7 个新场景**（R1–R7）；
- **③ 协议对拍不可替代 49 张** = ③-G GUI 1 + ③-O 离线/日志/长跑/工具能力缺口 26 + ③-N 无窗口项原文、判据不可判 22。
- 自检：60 + 43 + 49 = **152 = A 卡总数**，且去重后 152 个 id（见 §8 脚本输出）。

> **③-N 不是判死**：它是「卡上确实没有可判的窗口项原文」，下一轮窗口**不派**，先由卡 owner / Leader 补 `window_items`（那 5 张连 findings 都为空）。
> 本表绝不为它们臆造判据。

## 1. 依赖与前置（读表前必看）

1. **对照文档与本表引用的 15 个场景文件，当前在分支 `card/openmir2-dualrun-window-burndown` 上，尚未进 master `be9c5ac6b`。**
   master 的 `tools/probe/scenarios/` 只有 `login_only.json` / `gm_time.json` / `dice_chain.json` 三个；
   `npc_click(_auto)` / `merchant_dialog` / `bag_equip` / `eat_item` / `pickup_item` / `king_dialog` / `king_castle_war` /
   `gm_batch` / `dice_chain_auto` / `combat_hit` / `move_walk` 都在该分支。核验：
   ```sh
   git ls-tree -r --name-only card/openmir2-dualrun-window-burndown -- tools/probe/scenarios
   git show card/openmir2-dualrun-window-burndown:docs/conformance-a-card-scenario-map-20261023.md
   ```
   **派工前提**：该分支先合入（或窗口内以该分支为基线），否则 ① 的「现有场景」实际只有 3 个。
2. **判据口径沿用 mapping 表 §1 的证据通道（P/F/O/G/L）**，本表新增的只是「现有场景 / 缺一个新场景 / 对拍不可替代」这一刀。
3. **降噪能力（已有）**：`rules.json` 已覆盖 14（ignore）/ 10（series 掩码）/ 530（末段）/ 52（能力布局）/ 46/50/51（时间相关掩码）；
   `ident 100`（SM_SYSMESSAGE）走 `multiset_by_tick`（同 ident 连续段按多重集比，`tick_tolerance_ms` 可配）；
   复跑口径 N≥3 已写进 `tools/conformance/README.md`。**判据仍以 `compare_runs.py` 退出码 0/1/2 为准，2 不许当绿。**

## 2. 三档定义（本表的判定规则，先立规矩再判）

| 档 | 定义 | 判入条件 |
|---|---|---|
| **①** | 现有双栈场景（`tools/probe/scenarios/*.json`，含 `run_*_pair.ps1` 夹具）**跑一次就能给出该卡判据的证据** | 判据的观察面全落在 s2c 帧流 / `db_snapshot.json` / RNG 账本上，且**不需要新增场景文件**；只允许补运行时参数与前置（位形/物品/等级/行会/配置） |
| **②** | 判据可协议判定，但**没有任何现有场景会发那串 CM_\*** | 能写出「CM_* 序列 + 期望 SM_* 断言」；按配方聚合后组派工单 |
| **③** | **协议对拍不可替代** | 三子类：③-G 客户端渲染/面板/动画；③-O 文件/日志/长跑/环境注入/工具能力缺口（如多会话、连接关闭事件）；③-N 卡上**没有窗口项原文**，判据不可判（不臆造） |

**依据来源（防止拍脑袋）**：每行的「依据」列只取两种原文之一——
(a) 该卡 `review.window_items` 原文（标 `wi`）；(b) 该卡最新 `status.work` 里**点名**的可执行资产（现成 `run_*_pair.ps1` 脚本名 / 明确 frame ident / 明确 DB 列 / 明确 RNG 口径，标 `work`）。
两者皆无 ⇒ **③-N**，并在依据列逐字给出它 `work` 的泛化措辞。

## 3. 汇总表

| 档 | 卡数 | 前 10 张 |
|---|---:|---|
| ① 现有场景已覆盖 | 60 | `npc-def-loading`、`guard-ctor-constants`、`npc-ctor-constants`、`t138-request-castle-war`、`npc-display-table`、`login-charstatus-657`、`login-654-order`、`guild-g2-dispatch`、`login-numeric-gaps`、`oracle-humitems-align` … |
| ② 只差一个新场景 | 43 | `action-line-variable-text`、`script-take-persist`、`weapon-upgrade-values`、`npc-hit-frames`、`monster-ref-drains`、`a09-upgrade-item`、`merchant-race-bias`、`superman-normnpc`、`mb8-9-monsters`、`monster-walk-frames` … |
| ③-G 只能 GUI | 1 | `openybdeal-host-wiring` |
| ③-O 协议对拍不可替代 | 26 | `a22-castle-persistence`、`single-binary-roles`、`db-encoding`、`account-protection-13`、`strlist-bom`、`encoding-crate`、`hutil32-parity`、`dbsrv-scheme-a`、`encoding-core-e1-e2`、`chat-channels` … |
| ③-N 无窗口项原文 | 22 | `player-dir-supply`、`player-drop-useitems`、`rm-delaypushed`、`rm-delaymagic-six-ids`、`save-misc-gaps`、`db-error-reply`、`persist-single-sided-columns`、`smframe-minor-gaps`、`sm-merchantsay-recog`、`login-useitems-magic` … |
| **合计** | **152** | — |

**下一轮窗口要跑多少个场景**：

- ①：现成的 **15 个 probe 场景 + 11 个 `run_*_pair.ps1` 夹具**，按 §4 的派工表逐场景跑（同一场景可一次覆盖多张卡）；
- ②：**7 个新场景**（R1–R7），一次写、一次跑，覆盖 43 张卡；
- ③：**不派**（③-N 等补窗口项；③-G 走人工 GUI；③-O 另立夹具/工具卡）。

## 4. ① 派工表：现有场景 → 覆盖哪些卡

| 场景 | 覆盖卡数 | 卡 | 还缺什么前置 |
|---|---:|---|---|
| `login_only` | 21 | `guard-ctor-constants`、`npc-ctor-constants`、`npc-display-table`、`login-charstatus-657`、`login-654-order`、`guild-g2-dispatch`、`login-numeric-gaps`、`login-frame-order-52-752`、`player-searchviewrange-cadence`、`aoi-verify-maptime`、`login-notice-clienttick`、`enter-shift-loop`、`map-random-move-shift-loop`、`mapcell-attributes-loading`、`mongen-terrain-loop-live`、`ctor-barrier-a-draw`、`ctor-draws-fold`、`world-tick-expansion`、`window-collector-preflight`、`npcs-fixed-loading`、`guardlist-guards-loading` | 卫士在出生点视野内；MIR2_RNG_LOG+SEED 双侧 trace（1 张）；固定 NPC/商人首现；sleep 140s 覆盖 30s 续期（1 张）；同摆位 0122 (29,33)；走动覆盖 DISAPPEAR/APPEAR（1 张）；无（657×2 + 邻居相对序）（1 张）；…（其余前置逐卡见 §7） |
| `npc_click_auto / king_dialog` | 1 | `npc-def-loading` | 固定 NPC ActorId（auto:last_turn）/ king_actor（1 张） |
| `king_castle_war` | 1 | `t138-request-castle-war` | 掌门/城主行会角色各一个（W-T138-2..4）（1 张） |
| `bag_equip + db_snapshot` | 1 | `oracle-humitems-align` | bag_equip 后断线存档，比 hum_items 装备槽字段（1 张） |
| `move_walk (+login_only)` | 2 | `mapcell-consumer-teeth`、`transmap-maptrigger` | 走位/推/冲刺帧面 + NPC/商人出现帧（1 张）；踩格换图 + route 逐字节（走位可触发）（1 张） |
| `run_*_pair.ps1`（12 个脚本，逐卡见 §7：`run_*_pair.ps1`、`run_npc_shop_pair.ps1`、`run_party_trade_pair.ps1`、`run_drop_exp_pair.ps1`、`run_selloff_pair.ps1`、`run_quest_pair.ps1`、`run_guild_castle_pair.ps1`、`run_attack_pair.ps1`、`run_rng_scene_pair.ps1`、`run_rng_trace_pair.ps1`、`run_quest_pair.ps1 + scoped_diff.py`、`run_attack_pair.ps1 + scoped_diff.py`） | 34 | `m3-monster-active-seek`、`m3-drop-exp-wiring`、`m3-rng-scene-driver`、`m3-quest-align`、`m3-drop-exp-fixture-botsrv`、`m3-hero-pet-align`、`m3-fixture-chain-gate`、`m3-rng-trace-scene-groups`、`m3-pet-slave-masterdie-wire`、`m3-pet-slave-amulet-delitem-frame`、`m3-party-trade-align`、`m3-pet-slave-clone46`、`m3-shop-fixture-botsrv`、`m3-selloff-fixture-botsrv`、`m3-fixture-marker-prefix-bug`、`m3-drop-exp-align`、`m3-pet-slave-lifecycle`、`m3-hero-pet-fixture-botsrv`、`m3-guild-castle-fixture-botsrv`、`m3-selloff-goldsales-load-wire`、`m3-shop-dealing-gate`、`m3-shop-dialog-wiring`、`m3-quest-fixture-botsrv`、`m3-quest-persist-observability`、`m3-mail-auction-align`、`m3-party-deal-capacity-int`、`m3-pet-slave-summon`、`m3-party-gainexp-npe`、`m3-shop-scenario-authority`、`m3-fixture-matrix`、`m3-guild-castle-align`、`m3-npc-shop-align`、`m3-scoped-diff-ability`、`m3-party-trade-fixture-botsrv` | work 点名真机 W0/W9 对拍（run_attack_pair）（1 张）；work 点名真机 100 杀两相窗口（run_drop_exp_pair）（1 张）；work 点名真机两侧账本对拍 W9（run_rng_scene_pair）（1 张）；run_quest_pair；阻塞=markers 前缀卡 + C# k=0 截断 + quest_state 无生产者（1 张）；…（其余前置逐卡见 §7） |

> `run_*_pair.ps1` 是 mapping 表 §1 的 **F 夹具对拍**通道：判据是两侧同序列 `diff=0`，
> 与 probe 场景**不互相替代**，但它们**已经存在**（master `mir2-rs/tests/parity/m3/` 下 11 个 + 2 个子目录脚本），
> 所以归 ① 而不是 ②——② 的定义是「要新写场景」。

## 5. ② 派工单：7 个新场景覆盖 43 张卡

公共约定（所有配方）：
- 两侧各跑一次（同端口串行），产物 `frames.ndjson` + `run.json` + `db_snapshot.json`；
- 判定只用 `python -I -B tools/conformance/compare_runs.py --oracle A --rust B --out report.json`（0/1/2）；
- 步骤间默认 `--between-ms 200`，进世界后 `--dwell-ms 1200`；`CM_SOFTCLOSE(1009)` 由驱动器收尾时发出，**落库要等它之后**再抓 `db_snapshot.json`；
- 需要的 ActorId / MakeIndex / 坐标一律用 `--var` 传入，场景里写 `required`（缺参 rc=2），**不写死**；
- RNG 相关判据同时挂 `MIR2_RNG_LOG` + `MIR2_RNG_SEED`，用 `mir2-rs/tests/parity/m3/rng-trace-probe/` 的 groups 口径对账。

### R1 `npc_script_chain` —— 8 张

**要发的 CM_\***：`CM_CLICKNPC(1010, recog=npc_actor)` → 每步 `CM_MERCHANTDLGSELECT(1011, recog=npc_actor, body=标签文本)` ×n → （可选）`CM_QUERYBAGITEMS(81, recog=1)` → 驱动器收尾 `CM_SOFTCLOSE(1009)`。

**期望的 SM_\* 断言**（每步 `wait_for`，再全量逐字节比）：`SM_MERCHANTSAY(643)`（说词原文字节）、`SM_MENU_OK(767)`（尾字节）、
`SM_SYSMESSAGE(100)`（多重集比）、`SM_ADDITEM(200)`/`SM_BAGITEMS(201)`/`SM_DELITEM(202)`/`SM_UPDATEITEM(203)`/`SM_SENDUSEITEMS(621)`/`SM_SAVEITEMLIST(704)`、`SM_GOLDCHANGED(653)`。

**覆盖**：

| 卡 | 标签链（取自该卡 window_items） | 额外断言 |
|---|---|---|
| `action-line-variable-text` | `@main`（含 SENDMSG 0 支 / MESSAGEBOX 支） | 100 的 body = 命名变量**原样**；767 尾 = `EDCode(GBK 原文)` |
| `script-take-persist` | `@main→@kaiz→@ya5→@ya52` | `characters_bagitem` StdIndex=0/消失、`characters.Gold`、`hum_items` Index=0 且 MakeIndex/Dura/DuraMax 保留 |
| `click-path-bag-writeback` | 同上（click 路径） | `characters_bagitem` 行数/字段、`characters.Gold` 100→95 类、`TAKEW` 清槽、MakeIndex 连续 |
| `market-def-impl` | `@main` + `[~@repair]` | 27 处 `$STR` 变量展开后的 643 逐字节 |
| `guild-official-npc` | 建会成功/失败两支 | 622 负重、宣战战表、104 文案 |
| `run-gap-pl2` | 内容启用支（P31 / SETAUTOGETEXP 两态） | 44/升级链帧序 |
| `run-gap-pl3` | DECHP/INCHP/HungerSystem/KILLMONEXPRATE/POWERRATE 支 | 真机数值与帧序、197/198 自消息 |
| `run-gap-pl4` | 多角色 HomeMap 支 + `@MAP/@MAPMOVE` 支 | 653 同 tick 相位、800/633/634/801 帧序 |

### R2 `upgrade_retrieve` —— 4 张

**CM_\***：`CM_CLICKNPC(1010)` → `CM_MERCHANTDLGSELECT(1011, @upgradenow)` → `CM_MERCHANTDLGSELECT(1011, @back/@exit)` →（可选）`CM_TAKEONITEM(1003)`/`CM_EAT(1006)` → `CM_SOFTCLOSE(1009)`。
**SM_\***：`643`、`SM_DELITEM(202)`、`SM_ADDITEM(200)`、`SM_ABILITY(52)`、`SM_FEATURECHANGED(41)`、`SM_GOLDCHANGED(653)`、`SM_DAYCHANGING(46)`、`SM_UPDATESTATUS(? `657`)`、`709`、`100`。
同一 `MIR2_RNG_SEED` 下还要比抽数序（A 耐久 → B Random(3) → C/D/E 逐 k）与 `characters_bagitem` 落库。

**覆盖**：`weapon-upgrade-values`（四字段 + 657 + n90 逐抽）、`upgrade-npc`（Desc[10] 字节 + 提交/取回帧序）、`a09-upgrade-item`（ItemNameList 5 条 / 124B 尾 / 621）、`outbound-encoders-red-batch`（制药抽数流水 + MYSTATUS 派生落库）。

### R3 `combat_strike` —— 14 张

**CM_\***：`CM_WALK(3011, recog=move_x, param=move_y)` 靠近 → `CM_HIT(3014, recog=target_x, param=target_y, tag=dir)` ×N（N 与节奏由 `sleep`/`delay_ms` 控制）→（需要 GM 时）`CM_SAY(3030, "@kill"/"@hp")` → `CM_SOFTCLOSE(1009)`。
**SM_\***：`SM_TURN(10)`、`SM_WALK(11)`、`SM_RUN(13)`、`SM_HIT(14)`、`SM_STRUCK(31)`、`SM_DEATH(32)`、`SM_SPELL(17)`、`SM_MAGICFIRE(638)`、`SM_BACKSTEP(9)`、`SM_DELITEM(202)`、`SM_ADDITEM(200)`、`100`；受击帧的 `HP/MaxHP` 取观察者处理时刻实时值。

| 卡 | 专属断言（取自 window_items/work） |
|---|---|
| `monster-ref-drains` | race 203/206/210/215 四类帧 + 收件人集合；`SM_SPACEMOVE_HIDE(800)` 含本人 |
| `merchant-race-bias` | 各 HAM_* 模式打商人：不命中、damage=0、无 31；race 50 动物可命中 |
| `superman-normnpc` | 城门 110 打到 HP=0：Death=false、Run 回满、无 `SetMapXyFlag(2)` |
| `mb8-9-monsters` | race 215 施法帧（先接 T-148） |
| `monster-attack-hit-frame` | 受击前 ~200ms 同 Recog 的 14；Series 与 AttackDir 一致 |
| `run-gap-pl1` | P21 SendDelItems/SendAddItem 帧序、P32 复活链、`RM_DURACHANGE`、StdMode45 GetRandomLook |
| `run-gap-mb4` | race 92 的 SpaceMove/MapRandomMove 落点；829/837/856/858 站点 |
| `run-gap-mb1` | race 101/95 各 ≥1、R38 抽样 |
| `run-gap-mb3` | 107/105/102/112 |
| `run-gap-monster-specials` | 同 tick 产仔抽数相对序、Clone 扣 MP |
| `run-gap-monster-master-follow` | 异图/超距/SlaveRelax 两侧流水 |
| `attacktarget-overrides` | 82/119/130/91/107/105 |
| `proper-target-unfolded` | 攻击已死目标的抽数流水 |
| `rm-refmessage-delay-layer` | 200ms 后二次 Struck 的全局帧序 |

### R4 `world_observe` —— 8 张

**CM_\***：只有登录握手（`CM_LOGINNOTICEOK(1018)` / `CM_QUERYBAGITEMS(81)` 由驱动器自动），主体是 `sleep 60000–300000`。
**SM_\***：`wait_idents [10,11,13,14,31,32]`，全量逐字节比 + `MIR2_RNG_LOG` 账本。

**覆盖**：`npc-hit-frames`（180s 内 14 recog=国王）、`monster-walk-frames`（17–76 条 11）、`mb7-monsters`、`mb2-monsters`、`mb5-monsters`、
`monster-attack-cadence-lead`（HP 受控后 60s）、`run-gap-monster-state-expiry`（CrazyMode 0xF9 / RefNameColor / FireHit-TwinHit 到期）、
`run-gap-corpse-ghost-revive`（3 分钟变尸 / ZenTime-20s / 5 分钟移除 / 两帧 RM_TURN）。

### R5 `spell_cast` —— 6 张

**CM_\***：`CM_SPELL(3017, recog=MakeLong(魔号|等级), 目标坐标与目标 actor 走 param/tag/series)` ×N → `CM_SOFTCLOSE(1009)`。
字段样板取 Rust 侧解包 `game_svr::handle_spell_cmd(recog, tag, param, series)` 与其单测 `handle_spell_cmd(make_long(12,10), 99, 0, 0, …)`；
**落地时以该函数为准，不在场景里猜字段位置**。
**SM_\***：`SM_SPELL(17)`、`SM_MAGICFIRE(638)`、`SM_MAGICFIRE_FAIL(639)`、`SM_HEALTHSPELLCHANGED(53)`、`SM_CHANGENAMECOLOR(656)`、
`SM_ADDMAGIC(210)`/`SM_DELMAGIC(212)`、`SM_BACKSTEP(9)`、`100`；`characters_magic` 落库。

**覆盖**：`magic-8-21-wiring`（魔号 8/21/37，逐格 SM_BACKSTEP）、`spell-mp-wrapper`（失败回帧 + MP/53/尾时序）、
`spell-shared-tail`（注入时钟下 656 帧序）、`healthspell-emission-model`（整 tick 53 序列、SKILL_SHOWHP）、
`delmagic-addmagic-chain`（戴 115 发 210 / 打碎治愈戒指发 212）、`recall-slave-shift-live`（回召落点 + RNG 账本 `VERDICT IDENTICAL`）。

### R6 `castle_zone` —— 2 张

**CM_\***：`CM_WALK(3011)` 走进/走出皇宫与攻城区 →（可选）`CM_HIT(3014)` 打城门/守卫 → `sleep` 过 1s/10s 门 → `CM_SOFTCLOSE(1009)`。
**SM_\***：`SM_AREASTATE(766)`（bit4 置位与 1s 门）、`SM_TURN(10)` 名字色、`14/31`、`100`；名字色/行会/宣战/红名四臂各跑一次（不同角色）。

**覆盖**：`castle-war-area-impl`（四臂落点 + rng 账本、766 bit4、名字色、10s CastleMgr.Run）、`castle-guard-target`（同城/盟友/UnderWar 目标选择）。

### R7 `equip_struck_save` —— 1 张

**CM_\***：`CM_TAKEONITEM(1003, recog=make_index, param=槽位, body=物品名)` → `CM_WALK(3011)` 到怪旁 → 挨打 N 次（不还手）→ `CM_SOFTCLOSE(1009)`。
**SM_\***：`SM_TAKEON_OK(615)`、`SM_STRUCK(31)`、`SM_DURACHANGE(642)`、`SM_UPDATEITEM(203)`、`100`；
`db_snapshot` 比 `hum_items` 的 `Dura/DuraMax`（磨损 `Random(10)+5`、`wrapping_sub`、仅 `next==0` 破零）。

**覆盖**：`persist-useitems-wiring`。

### ② 派工顺序建议

1. **R1 → R2**（同一 NPC 点击/菜单底座，先跑通 643/767 基线）；
2. **R3 → R4**（R3 提供怪受击基线，R4 只靠 sleep，可与 R3 共用一个会话配置）；
3. **R5**（施法，需 R3 的坐标/目标样板）；
4. **R6 / R7**（角色态与装备态前置最多，放最后）。

## 6. ③ 清单

### ③-G 只能 GUI（1 张）

| 卡 | 为什么协议不可替代 |
|---|---|
| `openybdeal-host-wiring` | 窗口项 W-3「寄售面板 `SM_SENDDEALOFFFORM` **可见性**」与 W-SPD-1「掷骰 `SM_PLAYDICE(8001)` → 客户端出动画 → 回跳 `@cube82`」都是**客户端渲染/动画与回跳**；服务端只能证明「送了什么帧」。W-1 的 5008 字段与 DB diff 可用 `dice_chain_auto` 先取（本表已在 ② 的 R1/R5 里留位），但**卡级 DoD 含面板**。 |

### ③-O 协议对拍不可替代（26 张）

| 卡 | 不可替代的原因 |
|---|---|
| `a22-castle-persistence` | 文件级 sha256 + 10min Save tick + 跨午夜，协议对拍不可替代 |
| `single-binary-roles` | RNG trace groups + 旧 exe 日志逐行，属日志/账本面 |
| `db-encoding` | 建号/选角/存档路径（DBSrv），不是 game-gate probe |
| `account-protection-13` | 建号 13 列 roundtrip 走 DBSrv，非协议对拍 |
| `strlist-bom` | 带 BOM 文件读取，文件级 |
| `encoding-crate` | data/envir 文本逐字节，文件级 |
| `hutil32-parity` | NaN//x/>=2^31 分支的 round，离线判定 |
| `dbsrv-scheme-a` | 慢存档/闸门/独立失败注入，DBSrv 行为与计时 |
| `encoding-core-e1-e2` | 非 ASCII 逐字节 + 两条 BOM 路径，文件级 |
| `chat-channels` | 收件人集合需 >=2 会话并发（driver 单会话） |
| `guild-g3-persistence` | 四段文件字节（GBK+CRLF）不可协议判 |
| `castle-entities` | 启动 0 实体/4 告警属日志面；实体面另见 ② |
| `selgate-reconnect` | DBSrv 重启 + 真实客户端表现，非单次协议 run |
| `dbsrv-fault-paths` | MySQL 停机/重启 + 客户端表现 |
| `gate-idle-timeout` | 需 driver 记录连接关闭事件（frames.ndjson 无此事件）+ 真实客户端 |
| `gategate-slots-gm` | GameGate 侧 40KB/50ms 停顿与 GM_STOP 维护期，gate 行为面 |
| `gategate-speed-rules` | 登录包下发相位 0-210ms + gate 超长帧 Drop，属 gate 时序 |
| `save-on-shutdown` | work 点名 DB_SAVEHUMANRCD 帧 + -Stop graceful 日志，属进程/日志面 |
| `persist-item-attr-and-gold` | work 明示 G-01 功能上仍不落库（B-117），真机窗口为建角往返（DBSrv 路径） |
| `persist-create-rows` | 建角/兜底建角往返走 DBSrv，非 game-gate probe |
| `run-gap-checkdupobj` | 3 人同格推挤 + AOI 相位，需 3 路并发会话（driver 单会话） |
| `disappear-recipients-gridscan` | 节流窗口内进入 ±12 的观察者，需第二会话 |
| `movetomovingobject-quirk` | 多对象格身份序列，需 3 对象同格（并发会话） |
| `enter-map-cell-rejected-vs-oracle` | work 明示生产无 switch_map 调用者 ⇒ 真机不可达 |
| `m3-clock-injection` | 需 C# 影子 hook 半（HUtil32.GetTickCount 注入），非场景可补 |
| `m3-final-acceptance` | 300 假人 24h 长跑，非单次 run |

### ③-N 无窗口项原文、判据不可判（22 张，**不臆造、不派工**）

| 卡 | 卡上实际写了什么 |
|---|---|
| `player-dir-supply` | work 仅「真机窗口 + T-81」，无判据原文 |
| `player-drop-useitems` | work 仅「真机窗口 + T-81」，无判据原文 |
| `rm-delaypushed` | work 仅「真机窗口 + T-81」，无判据原文 |
| `rm-delaymagic-six-ids` | work 仅「真机窗口 + T-81」，无判据原文 |
| `save-misc-gaps` | work 仅「真机窗口」，无判据原文 |
| `db-error-reply` | work 只说「失败注入的 C# 对拍」，无具体帧/列 |
| `persist-single-sided-columns` | work 仅「真机窗口」，无判据原文 |
| `smframe-minor-gaps` | review 无 window_items、无 findings，work 仅「真机窗口 + T-79/T-80」 |
| `sm-merchantsay-recog` | review 无 window_items、无 findings，work 仅「真机窗口 + T-79/T-80」 |
| `login-useitems-magic` | review 无 window_items、无 findings，work 仅「真机窗口 + T-79/T-80」 |
| `sm-updateitem` | review 无 window_items、无 findings，work 仅「真机窗口 + T-79/T-80」 |
| `sm-additem-tail` | review 无 window_items、无 findings，work 仅「真机窗口 + T-79/T-80」 |
| `search-target-clear-target` | work 仅「真机窗口」，无判据原文 |
| `active-seek-race-missing` | work 仅「真机窗口」，且 118/130 数据不可达 |
| `monster-view-throttle` | work 仅「真机窗口」，无判据原文 |
| `run-gap-pr1-head-blocking` | work 仅「真机窗口」，无判据原文 |
| `snap-cell-fidelity` | work 仅「真机窗口项」，无判据原文 |
| `default-arm-spell-surface` | work 仅「真机逐帧对拍（窗口项）」，无具体帧 |
| `buff-producer-wiring` | work 仅「真机窗口项」+ 新开片 |
| `buff-system` | work 仅「真机窗口项 + A-5 生产接线批」 |
| `maptrigger-monster-and-door` | work 仅「真机窗口项 W-R1-a/b/c」，无展开 |
| `orphan-encoder-wiring` | work 仅「真机窗口项」，无判据原文 |

> `smframe-minor-gaps` / `sm-merchantsay-recog` / `login-useitems-magic` / `sm-updateitem` / `sm-additem-tail` 五张
> **review 里既无 `window_items` 也无 `findings`**（mapping 表 §3.2 已记录同一事实），本表逐张复核后确认。

## 7. 全量 152 张主表

「现分类」= mapping 表 §3 的通道（沿用，不改）；「新分档」= 本文件的三档。

| # | 卡 id | 现分类 | 新分档 | 依据（oid + 原文摘要） | 场景 / 配方 | 阻塞项 / 前置 |
|---:|---|---|---|---|---|---|
| 1 | `openmir2-action-line-variable-text` | 协议帧面 | ② | `99aa5e2bf29f53f6e8d7417f474c67c1730ecf83`（review.window_items）: 真机 C# 跑部署现成素材 `E:/MirServer/Mir200/Envir/QFunction-0.txt:388` 的 `SENDMSG 0 <$USERNAME>:<$STR(S1)> 151 235`：确认系统广播里命名变量**… | R1 npc_script_chain | 部署脚本 QFunction-0.txt:388 可达；@main 链能选到该支 |
| 2 | `openmir2-script-take-persist` | 落库后态 | ② | `a2e84cb8f0cb4afc0d14e7b608b77a3c332d8a3f`（review.window_items）: r9 原夹具复跑：角色 winplay0（PlayerId=6228），characters_bagitem Position=0 MakeIndex=70000001 StdIndex=44；点 @main→@kaiz→@ya5→@ya5… | R1 npc_script_chain | 指定角色 winplay0(PlayerId=6228) + characters_bagitem 预置行 |
| 3 | `openmir2-openybdeal-host-wiring` | GUI 面板 | ③-G | `112e0518564005ba2a9e2af67b3d591768172d51`（review.window_items）: W-SPD-1 真机掷骰：playdice 1 @cube82 → 客户端出动画（SM_PLAYDICE 8001）→ 回发 @cube82 被一次性放行 | - | 面板可见性/掷骰动画属客户端；W-1/W-2 帧面可由 dice_chain_auto 另取 |
| 4 | `openmir2-npc-def-loading` | 协议帧面 | ① | `d8a33e10382b7ee9f49dd568f8ed966231b90728`（review.window_items）: 点比奇国王(0122)/老人(3) 的说词面板与菜单可点性（@main 原文逐字） | npc_click_auto / king_dialog | 固定 NPC ActorId（auto:last_turn）/ king_actor |
| 5 | `openmir2-weapon-upgrade-values` | 协议帧面 | ② | `36974a5df070273630536234e6933837a0afb4a0`（review.window_items）: 真机 @upgradenow 全量对拍四字段与 657 帧 | R2 upgrade_retrieve | 武器升级 NPC + 可升级武器 + n90 抽数 |
| 6 | `openmir2-npc-hit-frames` | 协议帧面 | ② | `cc97be5b59db046661105879e4a975b5e648f968`（review.window_items）: C# 本尊正面样本 ≥180s（同位置必须出现 ident=14 recog=国王 actor） | R4 world_observe | 同位置 180s；国王 actor 在视野内 |
| 7 | `openmir2-monster-ref-drains` | 协议帧面 | ② | `1a073c115e45a5d5b4bcde7c8fb75c8ffea450d0`（review.window_items）: 真机四类帧逐字节与收件人集合（需先让 race 203/206/210/215 出现在部署数据） | R3 combat_strike | race 203/206/210/215 出现在部署数据 |
| 8 | `openmir2-guard-ctor-constants` | 协议帧面 | ① | `fe6662199a78ceaab5c338407976a2761c0643eb`（review.window_items）: W-guard-1 卫士自扫半径 5→7 的同 tick 可见集/事件序 | login_only | 卫士在出生点视野内；MIR2_RNG_LOG+SEED 双侧 trace |
| 9 | `openmir2-a22-castle-persistence` | 离线夹具(文件级) | ③-O | `03d54984b24b29394bf00e70becd78e94896d080`（review.window_items）: W-A22-1 文件字节：C#/Rust 各跑启动+一次 10min Save，比 List.txt / Castle/SabukW.txt / 0/AttackSabukWall.txt / 0/SabukW.txt 的 sha256；含… | - | 文件级 sha256 + 10min Save tick + 跨午夜，协议对拍不可替代 |
| 10 | `openmir2-npc-ctor-constants` | 协议帧面 | ① | `a7de06c254ef7040534af9cc4f40f7201153c6c2`（review.window_items）: W1 真机固定 NPC/商人首次出现帧 SM_TURN series 低=4 高=2，与 oracle 逐字节 | login_only | 固定 NPC/商人首现；sleep 140s 覆盖 30s 续期 |
| 11 | `openmir2-t138-request-castle-war` | 协议帧面 | ① | `30c79447db63d740f6693554ae55e3cbbca16a16`（review.window_items）: W-T138-1 非掌门/无行会 ⇒ SM_SYSMESSAGE(100)「你的请求被取消!!!」逐字节（Recog/Param 颜色/Tag/Series/尾） | king_castle_war | 掌门/城主行会角色各一个（W-T138-2..4） |
| 12 | `openmir2-npc-display-table` | 协议帧面 | ① | `9ac89a629c47f894b5a7e1569d77bc0f3e88d3be`（review.window_items）: 同摆位 0122 (29,33) C# vs Rust 登录抓包逐字节对拍：出现帧带名尾、3 对 nameless 转身 series 0x0203/0x0205/0x0207、每对后 SM_FEATURECHANGED(50,8,0) | login_only | 同摆位 0122 (29,33)；走动覆盖 DISAPPEAR/APPEAR |
| 13 | `openmir2-single-binary-roles` | 离线夹具(日志) | ③-O | `4ba2e1f2ac06f89a9d73699c83dd2e2108f94361`（review.window_items）: 真实 game-svr（MIR2_RNG_LOG + MIR2_RNG_SEED，startup 场景）取 trace，rng_ledger_diff groups --side rust --require-closed 须 GROUP_… | - | RNG trace groups + 旧 exe 日志逐行，属日志/账本面 |
| 14 | `openmir2-login-charstatus-657` | 协议帧面 | ① | `6714c2eda550139f88fb03ac47d469cf8a391363`（review.window_items）: Rust 侧重采 657×2（重编后）与 C# 对拍 | login_only | 无（657×2 + 邻居相对序） |
| 15 | `openmir2-a09-upgrade-item` | 协议帧面 | ② | `84659f7a095c3688fc75fc79d58cc028622b29b3`（review.window_items）: 部署 ItemNameList 5 条在真机取同名物品，逐字节对拍 SM_BAGITEMS/SM_SAVEITEMLIST/买卖/取回/详情的名与 124B 尾 | R2 upgrade_retrieve | ItemNameList 5 条；背包内同名物品 |
| 16 | `openmir2-merchant-race-bias` | 协议帧面 | ② | `7445a2eb8db1bdfadee2a1b3f3cc075563e22951`（review.window_items）: 真机玩家以各 HAM_* 模式攻击商人：两侧都应不命中、damage=0、无 SM_STRUCK，且该拍攻击 RNG 少一次 Random(SpeedPoint) 逐次一致；对照 race 50 动物仍可命中 | R3 combat_strike | HAM_* 各模式；race 50 动物作对照 |
| 17 | `openmir2-superman-normnpc` | 协议帧面 | ② | `6b20bff1eb4c6a32edbcf485ac78d2ff7920b61f`（review.window_items）: 城门 110 连续受击到 HP=0：Death=false、Run 回满、无 SetMapXyFlag(2) | R3 combat_strike | 城门 110/守卫 11/12/20/112/固定 NPC；GM Kill 入口 |
| 18 | `openmir2-mb8-9-monsters` | 协议帧面 | ② | `94f51f3101c14b194413a3b523deae7b1f125ffc`（review.window_items）: 真机对拍 race 215 施法帧 SM_SPELL/SM_MAGICFIRE（需先接线 T-148）与抽数序 | R3 combat_strike | race 215 施法（先接 T-148） |
| 19 | `openmir2-monster-walk-frames` | 协议帧面 | ② | `b09d2db15c8a59e859abc7f9d25501bace57f9f3`（review.window_items）: 真机重采怪物走位 SM_WALK 条数/时序（C# 基线 17–76 条） | R4 world_observe | 同屏怪；60-180s 观察 |
| 20 | `openmir2-login-654-order` | 协议帧面 | ① | `c26d96015ca3bf84cfeadf458d1ea8688d51f972`（review.window_items）: 改动后 Rust 侧登录抓包（654 跨 tick、第二 42、100×4）与 C# 同场景对拍 | login_only | 有行会角色一个（811/46/750 位置） |
| 21 | `openmir2-db-encoding` | 离线夹具(文件级) | ③-O | `55a005eda7b41c53f6e591bd70d7948e5b478b7d`（review.window_items）: E-7：真机 oracle 对照跑一次（建号/选角/存档的 ChrName 列值） | - | 建号/选角/存档路径（DBSrv），不是 game-gate probe |
| 22 | `openmir2-market-def-impl` | 协议帧面 | ② | `59c396a6579f96d79dd04a89bf074b26c6ad2a6e`（review.window_items）: 真机：商人 @main/[~@repair] 说词与变量展开（27 处 $STR） | R1 npc_script_chain | Market_Def 商人脚本；@main/[~@repair] 标签 |
| 23 | `openmir2-account-protection-13` | 落库后态 | ③-O | `cc328bb95f28f6fbddf08b98746f73f913755b65`（review.window_items）: 真机执行 account_protection_roundtrip（建号 13 列逐列 + 改资料 12 列 + PassWord 覆盖） | - | 建号 13 列 roundtrip 走 DBSrv，非协议对拍 |
| 24 | `openmir2-strlist-bom` | 离线夹具(文件级) | ③-O | `c90858a68667fc391a90806b0a3e7f5bef1d1278`（review.window_items）: 真机：带 BOM 的行会/物品/列表文件读取 | - | 带 BOM 文件读取，文件级 |
| 25 | `openmir2-mb7-monsters` | 协议帧面 | ② | `108da7b78a82cdee85c42eab86440a4ae48a0fd1`（review.window_items）: 真机：race 113/114/134 的 Run/克隆/召唤（部署数据不可达时以构造级为准） | R4 world_observe | race 113/114/134 可达（否则构造级） |
| 26 | `openmir2-encoding-crate` | 离线夹具(文件级) | ③-O | `e58a7c1a28e73b0a7a77bf8648b0023d2af55b6c`（review.window_items）: 真机：data/envir 承载的文本（行会文件/物品名/配置）逐字节 | - | data/envir 文本逐字节，文件级 |
| 27 | `openmir2-mb2-monsters` | 协议帧面 | ② | `7f9170ed3f2dcdcfa1979058a6570db2ef1b1df4`（review.window_items）: 真机：三怪 Run 的分支与帧（数据可达时） | R4 world_observe | 三怪 Run 数据可达 |
| 28 | `openmir2-hutil32-parity` | 离线夹具(文件级) | ③-O | `34812dd3ea4e6403e095a00d027c17828d96d427`（review.window_items）: b1_six_replay 的 round 在 NaN/\|x\|≥2^31 分支上的真机对拍 | - | NaN//x/>=2^31 分支的 round，离线判定 |
| 29 | `openmir2-guild-official-npc` | 协议帧面 | ② | `12f3e5ac9e9dd879e9c07b8d82dbfb9936bf41fd`（review.window_items）: 真机：建会成功/失败各支帧字节与 622 负重 | R1 npc_script_chain | 建会 NPC + 行会角色；622 负重 |
| 30 | `openmir2-mb5-monsters` | 协议帧面 | ② | `2c2bce1b78b2c159f7c03841e2ffd5ff8a81d47a`（review.window_items）: 真机：五类怪的 Run 分支与帧（数据不可达时以构造级为准） | R4 world_observe | 五类怪；FrostTiger 657 需 T-135 |
| 31 | `openmir2-castle-war-area-impl` | 协议帧面 | ② | `a49984c2e436b4b2e003e933a9c847c7e6f037ba`（review.window_items）: 真机 W-1..W-5：非成员进皇宫/成员/死亡/红名四臂落点与 rng 账本 | R6 castle_zone | 攻城状态 + 非成员/成员/死亡/红名四臂各一角色 |
| 32 | `openmir2-dbsrv-scheme-a` | 落库后态 | ③-O | `910cca3df143525e6eecbbd802fcad81b08d58f3`（review.window_items）: 真机 W-DB-1..7：慢存档下的 QUERYCHR/SELCHR/LOAD 时延 | - | 慢存档/闸门/独立失败注入，DBSrv 行为与计时 |
| 33 | `openmir2-encoding-core-e1-e2` | 离线夹具(文件级) | ③-O | `b5c93e2045ee227dc6221118f683c5a57756ed36`（review.window_items）: 真机：说词/物品名等非 ASCII 文本的逐字节 | - | 非 ASCII 逐字节 + 两条 BOM 路径，文件级 |
| 34 | `openmir2-chat-channels` | 协议帧面 | ③-O | `cc6fae31c7f02ec4cb02e078579144ba9fbdccca`（review.window_items）: 真机：三前缀的收件人集合与帧字节 | - | 收件人集合需 >=2 会话并发（driver 单会话） |
| 35 | `openmir2-guild-g3-persistence` | 落库后态 | ③-O | `8a576ee6eeebd0ccf81f441b9cf66cb0a243d4ea`（review.window_items）: 窗口 A：两个在线行会成员一方断线，对端恰收一条 104（Recog/Param/Series/GBK 尾，ShowPreFixMsg=0 无前缀），离场方零帧 | - | 四段文件字节（GBK+CRLF）不可协议判 |
| 36 | `openmir2-castle-guard-target` | 协议帧面 | ② | `bebc44d758ac07641d44731dfbdc6d67cd5e0140`（review.window_items）: 同城/盟友/UnderWar 下的守卫目标选择（真机） | R6 castle_zone | 同城/盟友/UnderWar 三态 + 守卫 |
| 37 | `openmir2-castle-entities` | 协议帧面 | ③-O | `c6ca84b6781f3fcfee21ed2a5eed5d840894db7c`（review.window_items）: 部署 A 情景：启动后 0 实体、4 条告警（Castle/Castle/SabukW.txt） | - | 启动 0 实体/4 告警属日志面；实体面另见 ② |
| 38 | `openmir2-upgrade-npc` | 协议帧面 | ② | `2d06ae59b5f64edd822165d7dd9c65968bb4443f`（review.window_items）: 取回两侧同种子抽数序 A(耐久)→B(Random(3))→C/D/E 逐 k diff=0 | R2 upgrade_retrieve | 同 R2；同种子抽数序 |
| 39 | `openmir2-guild-g2-dispatch` | 协议帧面 | ① | `f5166c9bcb4c91d8c50cd60333f770e10ea89fd8`（review.window_items）: 750 的真机出队序（46 之后、654 之前） | login_only | 有行会角色（750 出队序 46 后 654 前） |
| 40 | `openmir2-run-gap-pl4` | 协议帧面 | ② | `6c55b9910d53c5ceca26e5ea17092502d62878c1`（review.window_items）: T-89 653 同 tick 相位 | R1 npc_script_chain | 多角色 HomeMap；脚本支 800/633/634/801 |
| 41 | `openmir2-selgate-reconnect` | 协议帧面 | ③-O | `60916e1983d661963258168b32cc775d504a7cea`（review.window_items）: 真机 DBSrv 重启后 SelGate 的重连与在途会话 | - | DBSrv 重启 + 真实客户端表现，非单次协议 run |
| 42 | `openmir2-dbsrv-fault-paths` | 落库后态 | ③-O | `22e1d998c9e0d50ed76471d4e8cea31ccb789ce5`（review.window_items）: 真机 MySQL 停机/重启下的四种降级帧与客户端表现 | - | MySQL 停机/重启 + 客户端表现 |
| 43 | `openmir2-gate-idle-timeout` | 协议帧面 | ③-O | `f45272acb992ca584e6f02fb1f5546e9bbb548c6`（review.window_items）: 真实客户端挂起 60s 的被踢时序与畸形超时帧的实际反应 | - | 需 driver 记录连接关闭事件（frames.ndjson 无此事件）+ 真实客户端 |
| 44 | `openmir2-run-gap-pl2` | 协议帧面 | ② | `aca468b255477a81d1c065a3516f15ac8d6227d5`（review.window_items）: 内容启用后的 P31 结算与 44/升级链帧序 | R1 npc_script_chain | 内容启用（P31/SETAUTOGETEXP）脚本链 |
| 45 | `openmir2-gategate-slots-gm` | 协议帧面 | ③-O | `b84d1e58d3f70be5b76c8952d5b56e2fe0285f4c`（review.window_items）: C# GameSvr + Rust GameGate 的 GM_CHECKCLIENT→SendKeepAlive→GM_CHECKSERVER 节拍与 RECEIVE_OK 次数 | - | GameGate 侧 40KB/50ms 停顿与 GM_STOP 维护期，gate 行为面 |
| 46 | `openmir2-login-numeric-gaps` | 协议帧面 | ① | `97180d9749d632ad68979099bec28039e4310214`（review.window_items）: 新号首登 52 的 Weight=10 / MaxExp=10 | login_only | 新号首登 + 新号首杀（配 combat_hit）+ 非零 WalkWait 怪 |
| 47 | `openmir2-oracle-humitems-align` | 落库后态 | ① | `5f78c0c6602f5f9624dea4f693c754044199ef4c`（status.work）: 已对齐 a84c81aae + d7ead5b2a（两侧 1:1，master 76e5f66f8）。余量=真机窗口：部署基线已变，下次开窗重抓装备槽字段级证据 | bag_equip + db_snapshot | bag_equip 后断线存档，比 hum_items 装备槽字段 |
| 48 | `openmir2-run-gap-pl3` | 协议帧面 | ② | `262f0e696d740520826d8cb894d540c1dfc83786`（review.window_items）: 内容一旦启用（DECHP/INCHP/HungerSystem/KILLMONEXPRATE/POWERRATE）时的真机数值与帧序 | R1 npc_script_chain | 内容启用（DECHP/INCHP/HungerSystem/KILLMONEXPRATE/POWERRATE） |
| 49 | `openmir2-delmagic-addmagic-chain` | 协议帧面 | ② | `3b78d2ba3186bdb40e253ba3f38a73091fd74f0f`（review.window_items）: 登录后 characters_magic 行消失/新增与 212/210 真机字节与序 | R5 spell_cast | 戴 115 戒指 / 打碎治愈戒指；characters_magic 后态 |
| 50 | `openmir2-login-frame-order-52-752` | 协议帧面 | ① | `b63a74e802c432f694add6fd58938003e1f980e8`（status.work）: 已合入 master（447440093 + ed73a6a73，tier=R，模型 D）。余量=非 TestServer 部署/发光装备/IsSpirit 的槽位变化未取样本；621/211 的 Rust 侧还依赖 2… | login_only | 非 TestServer 部署 + 发光装备/IsSpirit 槽位 |
| 51 | `openmir2-gategate-speed-rules` | 协议帧面 | ③-O | `9423d8fcf4b47b48bb24d1ba6a102dc7cf7c4dc5`（review.window_items）: 登录包下发相位：oracle 入 MsgList 后到 drain 才 Send（0-210ms 不确定），Rust 仍立即下发——真机同相位对拍 | - | 登录包下发相位 0-210ms + gate 超长帧 Drop，属 gate 时序 |
| 52 | `openmir2-healthspell-emission-model` | 协议帧面 | ② | `1e85628e98da5160c4ecb3cda9909c7bc5a19274`（review.window_items）: 整 tick 53 序列的真机对拍（尤其跨 tick G-C pending 被下一 Run 替换） | R5 spell_cast | 整 tick 53 序列；SKILL_SHOWHP |
| 53 | `openmir2-monster-attack-hit-frame` | 协议帧面 | ② | `ae0e154cc4914380100083de8ee87600383b6aed`（review.window_items）: 两侧同点 (605,578)/L1/60s 各采一趟：每个受击者的 SM_STRUCK 前 ~200ms 内有同 Recog 的 SM_HIT | R3 combat_strike | 怪在同点 (605,578) 60s 采样 |
| 54 | `openmir2-run-gap-pl1` | 协议帧面 | ② | `ac5b2b4a7fbaf9861fcc2d8b5ca95a64e3f73f7a`（review.window_items）: P21 的 SendDelItems/SendAddItem 帧序与背包别名结果（真机对拍） | R3 combat_strike | P21/P32 复活链 + 耐久；同 tick 53 实拍 |
| 55 | `openmir2-monster-attack-cadence-lead` | 协议帧面 | ② | ``（review.window_items）:  | R4 world_observe | HP 受控（GM 设 HP）后 60s 采样 |
| 56 | `openmir2-run-gap-mb4` | 协议帧面 | ② | `342e560f6da8e68182acc000663a59febd1ac2da`（review.window_items）: race 92 真机：SpaceMove/MapRandomMove 落点与帧序、狂暴 500/400 节拍 | R3 combat_strike | race 92；带装备玩家触达 829/837/856/858 |
| 57 | `openmir2-magic-8-21-wiring` | 协议帧面 | ② | ``（review.window_items）:  | R5 spell_cast | 魔号 8/21/37；逐格 SM_BACKSTEP |
| 58 | `openmir2-player-dir-supply` | 协议帧面 | ③-N | `8b8b67f63d6d63bb82bc5c36c9500331711d8f37`（status.work）: 玩家 `Dir` 权威切换到 `Entity.ai.dir`（展示表只做投影），所有读点改读实体 已落地（c518e075b，tier=R；首轮 REFUTED → 修复 CONFIRMED）。余量：真机窗口 + T-8… | - | work 仅「真机窗口 + T-81」，无判据原文 |
| 59 | `openmir2-player-drop-useitems` | 协议帧面 | ③-N | `dd59d66271ce822b605cfd86e6f1f853f6bd2e08`（status.work）: 玩家死亡 `DropUseItems` 两段（`ItemDesc&8` 清槽 + 全槽 13 抽 + `GetRandomLook`；`KillTargetTrigger` 击杀者门） 已落地（c518e075b，tie… | - | work 仅「真机窗口 + T-81」，无判据原文 |
| 60 | `openmir2-rm-delaypushed` | 协议帧面 | ③-N | `a4e7b231412e996a38b73c9f2dc57449c8288270`（status.work）: 44 推挤的 `RM_DELAYPUSHED` 入队与投递（`char_pushed_now` + 逐格 `SM_BACKSTEP`） 已落地（c518e075b，tier=R；首轮 REFUTED → 修复 CONFI… | - | work 仅「真机窗口 + T-81」，无判据原文 |
| 61 | `openmir2-rm-delaymagic-six-ids` | 协议帧面 | ③-N | `51b69da2fcbb4295026e0fcebee96a07169bae45`（status.work）: 六魔号 `RM_DELAYMAGIC`（1/5、11、13、44、45）入队与到点结算链 已落地（c518e075b，tier=R；首轮 REFUTED → 修复 CONFIRMED）。余量：真机窗口 + T-81 登记… | - | work 仅「真机窗口 + T-81」，无判据原文 |
| 62 | `openmir2-save-misc-gaps` | 落库后态 | ③-N | `08ae888f48600f8dd57cffd4c4b1df89ce1abdc0`（status.work）: 存档杂项已落地（50dd3f2ef，tier=R）。余量：真机窗口；G11 传输面未移植；G7 读到一半失败未构造。 | - | work 仅「真机窗口」，无判据原文 |
| 63 | `openmir2-db-error-reply` | 落库后态 | ③-N | `4685a4bc936721c163fb4a42b3833d8c5380587f`（status.work）: DB 错误应答已落地（50dd3f2ef，tier=R）。余量：真机窗口（失败注入的 C# 对拍）。 | - | work 只说「失败注入的 C# 对拍」，无具体帧/列 |
| 64 | `openmir2-persist-single-sided-columns` | 落库后态 | ③-N | `30c4ae0f0975169d4c3ea19e8874d95400d34d27`（status.work）: 单边列与 Rust-only 补齐已落地（50dd3f2ef，tier=R）。余量：真机窗口；失败移动后 `DisplayTable.dir` 未随实体（建议单卡）。 | - | work 仅「真机窗口」，无判据原文 |
| 65 | `openmir2-attacktarget-overrides` | 协议帧面 | ② | `fbfe582628678bb7998ff96348521aa443670d69`（status.work）: 10 个 AttackTarget 覆写已落地（bbf9694aa，tier=R；首轮 F1 Critical → 修复 CONFIRMED）。余量：真机窗口（82/119/130/91/107/105）；114/117… | R3 combat_strike | work 点名帧 82/119/130/91/107/105 |
| 66 | `openmir2-smframe-minor-gaps` | 协议帧面 | ③-N | `cba70285c0de5000e830492d73fe077e1a50e088`（status.work）: 随帧面批次落地（27d22a9f3，tier=R）。余量：真机窗口 + T-79/T-80 登记项。；review 无 window_items/findings | - | review 无 window_items、无 findings，work 仅「真机窗口 + T-79/T-80」 |
| 67 | `openmir2-sm-merchantsay-recog` | 协议帧面 | ③-N | `a49f03995ee9f047eb7608aefa05927720a1d623`（status.work）: 随帧面批次落地（27d22a9f3，tier=R）。余量：真机窗口 + T-79/T-80 登记项。；review 无 window_items/findings | - | review 无 window_items、无 findings，work 仅「真机窗口 + T-79/T-80」 |
| 68 | `openmir2-login-useitems-magic` | 协议帧面 | ③-N | `a5481baab0e4c65b930f02bbce1fa11166000892`（status.work）: 随帧面批次落地（27d22a9f3，tier=R）。余量：真机窗口 + T-79/T-80 登记项。；review 无 window_items/findings | - | review 无 window_items、无 findings，work 仅「真机窗口 + T-79/T-80」 |
| 69 | `openmir2-sm-updateitem` | 协议帧面 | ③-N | `dbd25387c0574a2a65b1a007327658d0482396c2`（status.work）: 随帧面批次落地（27d22a9f3，tier=R）。余量：真机窗口 + T-79/T-80 登记项。；review 无 window_items/findings | - | review 无 window_items、无 findings，work 仅「真机窗口 + T-79/T-80」 |
| 70 | `openmir2-sm-additem-tail` | 协议帧面 | ③-N | `8b252336efaca93c1394fd646cbb69131fe01ab3`（status.work）: 随帧面批次落地（27d22a9f3，tier=R）。余量：真机窗口 + T-79/T-80 登记项。；review 无 window_items/findings | - | review 无 window_items、无 findings，work 仅「真机窗口 + T-79/T-80」 |
| 71 | `openmir2-search-target-clear-target` | 协议帧面 | ③-N | `d63ea986b23a46d05471cc87d9e7625245976034`（status.work）: SearchTarget 的 ClearTarget/RemoveAt 怪癖已照抄（6a774ea79，tier=R）。余量：① 真机窗口；② 其余折入版调用点未扩大范围。 | - | work 仅「真机窗口」，无判据原文 |
| 72 | `openmir2-proper-target-unfolded` | 协议帧面 | ② | `70779c78cf731407aac8ac68ffb005eebb15c43c`（status.work）: proper-target 折入拆开、抽数恢复（6a774ea79，tier=R）。余量：① 真机窗口（攻击已死目标的抽数流水）；② 其余折入版调用点（特殊怪覆写/显示路径）未扩大范围。 | R3 combat_strike | work 点名「攻击已死目标的抽数流水」 |
| 73 | `openmir2-active-seek-race-missing` | 协议帧面 | ③-N | `0be96efceadc743ad6daba1f6b454bf6948f657f`（status.work）: 9 个 seek race 已补（6a774ea79，tier=R）。余量：① 真机窗口；② 这些 race 的 `AttackTarget` 覆写属 `openmir2-attacktarget-overrides`；… | - | work 仅「真机窗口」，且 118/130 数据不可达 |
| 74 | `openmir2-rm-refmessage-delay-layer` | 协议帧面 | ② | `8f965cf0ea68b761d7058ecfd118b56c576483bf`（status.work）: RM_REFMESSAGE 延时层已落地（56524b74c，tier=R；复验 REFUTED 两条 → 修复 + 回归）。余量：① **真机窗口**（200ms 后二次 `Struck` 的全局帧序）；② 未接的 7… | R3 combat_strike | work 点名「200ms 后二次 Struck 的全局帧序」 |
| 75 | `openmir2-save-on-shutdown` | 落库后态 | ③-O | `e06df3ce4f4a1c86e671753a17dd113f83bf9d70`（status.work）: 关服存档已落地（296155f4e，tier=R；复验 CONFIRMED，偏离登记 D-12）。余量：① **真机窗口**（`DB_SAVEHUMANRCD` 帧与落库 diff、`-Stop` graceful 日志… | - | work 点名 DB_SAVEHUMANRCD 帧 + -Stop graceful 日志，属进程/日志面 |
| 76 | `openmir2-run-gap-mb3` | 协议帧面 | ② | `94d5a5375e377fc51d63d4cfbe35c431e7b4229f`（status.work）: MB-3 已落地（f4c16f674，tier=R；首轮 REFUTED 两条偏离 → 修复 CONFIRMED）。余量：① **真机窗口**（107/105/102/112）；② **`SendStruckDelayM… | R3 combat_strike | work 点名帧 107/105/102/112 |
| 77 | `openmir2-persist-item-attr-and-gold` | 落库后态 | ③-O | `e9e732ee06ee5ed4dc9854e07325bd9e437a71a7`（status.work）: G-01/G-02 等价实现+登记已落地（71f4baa48，tier=R）。余量：① G-01 功能上仍不落库（oracle 门决定，`B-117`）；若要修，单开批次（upsert + 按 MakeIndex 关联 … | - | work 明示 G-01 功能上仍不落库（B-117），真机窗口为建角往返（DBSrv 路径） |
| 78 | `openmir2-persist-useitems-wiring` | 落库后态 | ② | `f7a2a1cd84f0118db26a03f9752d5159e7c2bb0f`（status.work）: 会话装备已装载进世界实体（03208e20e，tier=R；复验 CONFIRMED）。余量：① **真机窗口**（耐久磨损/破零/存档往返）；② 破零后的 oracle 收尾未接（能力重算/`SendDelItems`… | R7 equip_struck_save | work 点名「耐久磨损/破零/存档往返」 |
| 79 | `openmir2-persist-create-rows` | 落库后态 | ③-O | `9b0ff6894ea191f2e6a077f2ab82d9532c6c75eb`（status.work）: G-20 兜底建角 ablity INSERT 已落地（aa32b197c，tier=R）；**G-03 经复核证伪**（oracle 同样不建 bonusability 行，按硬约束③照抄，撤回偏离与 D-11）。余量… | - | 建角/兜底建角往返走 DBSrv，非 game-gate probe |
| 80 | `openmir2-monster-view-throttle` | 协议帧面 | ③-N | `6764b63826f159b7f6d99aa39ebd4ea2a049d7f7`（status.work）: 怪物视野已改 oracle 口径（0ebdbbf54，tier=R；复验 CONFIRMED，合并 MB-1 时人工解冲突）。余量：① **真机窗口**；② GuardUnit 的 6 格/CanPassiveAttac… | - | work 仅「真机窗口」，无判据原文 |
| 81 | `openmir2-run-gap-mb1` | 协议帧面 | ② | `12d3f1ea6c0d1b595fd87c15a6841fe2bb645be9`（status.work）: MB-1 四类 Run 覆写已落地（a1e387a77，tier=R；复验 REFUTED(F1 丢帧) → 发帧时刻收件人快照修复 + 两条回归用例）。余量：① **真机窗口**（race 101/95 各 ≥1、R3… | R3 combat_strike | work 点名 race 101/95 各 >=1、R38 抽样 |
| 82 | `openmir2-run-gap-checkdupobj` | 协议帧面 | ③-O | `d758a92ff637e4a44a73e3d539335b50a6976df7`（status.work）: CheckDupObj 重叠推挤已落地（706c95cae，tier=R；首轮 REFUTED 因 MoveToMovingObject 怪癖 → 怪癖卡先落地后 rebase + 跨卡判据）。余量：① **真机窗口**… | - | 3 人同格推挤 + AOI 相位，需 3 路并发会话（driver 单会话） |
| 83 | `openmir2-run-gap-monster-state-expiry` | 协议帧面 | ② | `0add6624f0bb55f58f85474d70ea012806543c15`（status.work）: 四项状态到期已落地（58da49071，tier=R；独立复验 CONFIRMED）。余量：① **真机窗口**（CrazyMode 色变 0xF9 / RefNameColor 帧面 / FireHit-TwinHit… | R4 world_observe | work 点名 CrazyMode 色变 0xF9 / RefNameColor / FireHit-TwinHit 到期 |
| 84 | `openmir2-run-gap-corpse-ghost-revive` | 协议帧面 | ② | `b37880c0ef620e79bf7b928aae8272380cd9b836`（status.work）: 尸体 MakeGhost 双门 + 复活链 + Ghost 5min 清理已落地（b12b59ca8，tier=R；首轮 REFUTED → 三条修复 → 修复后独立复核 CONFIRMED）。余量：① **真机窗口**… | R4 world_observe | work 点名 3 分钟变尸 / ZenTime-20s / 5 分钟移除 / 两帧 RM_TURN |
| 85 | `openmir2-run-gap-pr1-head-blocking` | 协议帧面 | ③-N | `4a92013550ae1b4fc69cbd1e5518019e12a2bab4`（status.work）: PR-1 已落地（7e8d47426，tier=R；两轮 REFUTED → 四叉堆 + 500 场景差分 0 不一致 → CONFIRMED）。**MB-1/MB-3/MB-6/MB-9 前置解冻**。余量：① 真机窗… | - | work 仅「真机窗口」，无判据原文 |
| 86 | `openmir2-disappear-recipients-gridscan` | 协议帧面 | ③-O | `2f85770698c56f5ead5ab9762409dfd45db36599`（status.work）: RM_DISAPPEAR 收件人已改为离开时刻 ±12 格扫（8dea847c5，tier=R；复验 CONFIRMED）。余量：① **真机窗口**（节流窗口内进入 ±12 的观察者、离世广播与 AOI/移动帧的全局次… | - | 节流窗口内进入 ±12 的观察者，需第二会话 |
| 87 | `openmir2-run-gap-monster-specials` | 协议帧面 | ② | `ece643c506a86f5b9dabf1fa6ec50e059062ab88`（status.work）: BeeQueen/SpiderHouse 产仔 + Clone 扣主 MP 已落地（f46e4f03f，tier=R；两条 Important 收口后复验 CONFIRMED）。余量：① **真机窗口**（同 tick … | R3 combat_strike | work 点名同 tick 产仔抽数序 / SM_HIT 帧面 / Clone 扣 MP |
| 88 | `openmir2-run-gap-monster-master-follow` | 协议帧面 | ② | `15847cd8fe83a24092ecb809185073a3a37dc920`（status.work）: 宝宝跟随 / SpaceMove / SlaveRelax / RunAway 到期已落地（209023120，tier=R；首轮 REFUTED(F1) → 已修 + 回归用例）。余量：① **真机窗口**（异图/超距… | R3 combat_strike | work 点名异图/超距/SlaveRelax 两侧流水 |
| 89 | `openmir2-movetomovingobject-quirk` | 协议帧面 | ③-O | `aceda41633a8800f68f047a8715f069b89e18a63`（status.work）: MoveToMovingObject 怪癖已复刻（4cb9bb5dc，tier=R；复验 CONFIRMED，含 >50k 例穷举对拍）。余量：① **真机窗口**（多对象格移动时的源/目标格身份序列）；② `World… | - | 多对象格身份序列，需 3 对象同格（并发会话） |
| 90 | `openmir2-player-searchviewrange-cadence` | 协议帧面 | ① | `0550c8c490f4a5e84725ea07bbf2938839ed931b`（status.work）: 玩家 AOI 节拍已对齐（9e27ac39b，tier=R；独立复验 CONFIRMED + 2 LOW 处理）。余量：① **真机窗口**——首扫约 1.2s、之后每 1.2s；② 非玩家扫描节拍仍每 tick（另卡）… | login_only | work 点名首扫 1.2s 与之后每 1.2s（dwell+t_ms 可判） |
| 91 | `openmir2-aoi-verify-maptime` | 协议帧面 | ① | `02cc8fb6298beb6144f3489dcc01d02bc6c6525a`（status.work）: VerifyMapTime 30s 刷新 + viewer 过期分流 + AOI 诊断口径已落地（8b4f5df5e，tier=R；独立复验 approve）。余量：① **真机窗口**——同缩窗场景需验 vis>=1、… | login_only | work 点名 vis>=1 / nearest<=vr / SM_TURN(281,661) / SM_WALK=0；需缩窗配置 |
| 92 | `openmir2-spell-shared-tail` | 协议帧面 | ② | `f9bc711a2b9cf59550223f79e50f6a4681e1dad1`（status.work）: T-78 节流修复落地（2dac5c7bb，tier=R）。余量：① 真机窗口（注入时钟下 656 帧序）；② T-78 剩余三偏差；③ `vmt`（VerifyMapTime/玩家 viewer 过期/AOI 诊断）在… | R5 spell_cast | work 点名注入时钟下 656 帧序 |
| 93 | `openmir2-login-notice-clienttick` | 协议帧面 | ① | `46b350f5cc51e6a7b046f24a769ba3a24e785e95`（status.work）: B1 已落地（326c6431f，tier=R）：1018→`ClientTick`（零出帧）+ `SM_SPELL` 后接版本门（失败恰 1 条 639、MP/53 先行、+GD 照发）。**T-73** 登记 B1 … | login_only | work 点名含 SoftVersionDateEx==0 构造（send raw 1018） |
| 94 | `openmir2-spell-mp-wrapper` | 协议帧面 | ② | `a4823d889c88cdd53cdbb4cf733875afee16e8f1`（status.work）: 战士门与召唤护身符门 639 已接（0c4fe0e5d，tier=R）。余量：① 真机窗口（失败回帧 + MP/53/尾时序）；② 6/13/14/15/16/18/19/36 的护身符/失败门未建（14/15 的 A-… | R5 spell_cast | work 点名失败回帧 + MP/53/尾时序 |
| 95 | `openmir2-enter-map-cell-rejected-vs-oracle` | 协议帧面 | ③-O | `8c5f59e1654893f8a52e26ade65785d376129ae6`（status.work）: 实现 + 独立复验 PASS（df5a260ea，tier=R 不收卡）。余量：① 真机窗口（切图失败路径；当前生产无 `switch_map` 调用者 ⇒ 真机不可达）；② **帧面**：oracle 在失败前已发 `… | - | work 明示生产无 switch_map 调用者 ⇒ 真机不可达 |
| 96 | `openmir2-enter-shift-loop` | 协议帧面 | ① | `3ef3520d67321804322927647b5c9f9b7d06f824`（status.work）: 实现 + 独立复验 CONFIRMED + must_fix 处理（2fa749471，tier=R 不收卡）。余量：① 真机窗口（墙格出生登录：RNG 流水 + 落点；默认零抽）；② HomeMap 不可解析分支的 o… | login_only | 墙格出生角色 + MIR2_RNG_LOG/SEED |
| 97 | `openmir2-map-random-move-shift-loop` | 协议帧面 | ① | `483a6375cf31823c3c9eb536a08fa3bc00c2348a`（status.work）: 实现 + 独立复验 CONFIRMED + 合并重放（047709a9b，tier=R 不收卡）。余量：① 真机窗口（商人到点移动：RNG 流水 + 落点）；② 登记未做：201 轮回滚的 `DeleteFromMap`… | login_only | 商人到点移动帧 + RNG 账本 |
| 98 | `openmir2-recall-slave-shift-live` | 协议帧面 | ② | `f2cbc88069347a07ae462f1a27dd03ad94be5eda`（status.work）: 判据落地 + 复验 CONFIRMED（49fe49f05，tier=R 不收卡）。余量：① 真机窗口（W0 §2e：两侧 RNG 账本 `VERDICT IDENTICAL` + 回召落点同坐标）；② 同窗登记的 `m… | R5 spell_cast | 回召落点 + 两侧 RNG 账本 VERDICT IDENTICAL |
| 99 | `openmir2-mapcell-attributes-loading` | 协议帧面 | ① | `4dda39d08ec20346f3ff62f80046ebc3ae8a9d03`（status.work）: 实现 + 独立复验 + 数字/判据订正（2ff02ac0e + b192bd9f3，tier=R 不收卡）。余量：① 真机窗口（**必核**：route added=1649、刷怪位置与 RNG 流水、进图/落格/AOI… | login_only | route added=1649 / 刷怪位置 / 进图落格 AOI |
| 100 | `openmir2-mapcell-consumer-teeth` | 协议帧面 | ① | `e6a12bef278e32f5756c5d3187a0df83a170f7a6`（status.work）: 牙 + 真机普查 + 登记落地（34af5b4ff，tier=L 片；卡因 R 级窗口项不 closed）。余量：① 真机窗口核「走位/推/冲刺帧面 + NPC/商人出现帧」（W0 runbook §W0-6 已列；当前… | move_walk (+login_only) | 走位/推/冲刺帧面 + NPC/商人出现帧 |
| 101 | `openmir2-mongen-terrain-loop-live` | 协议帧面 | ① | `46fd6208b71f7b06b828bf384ba8052d9ab69d4d`（status.work）: 判据落地（649924131，tier=L 片；卡因 R 级窗口项不 closed）。余量：真机窗口核「两侧出生坐标逐点一致 + RNG 流水」（W0 runbook §W0-6/§W0-8 已列，依赖 mapcell … | login_only | 两侧出生坐标逐点一致 + RNG 流水（1072/2827 墙格） |
| 102 | `openmir2-ctor-barrier-a-draw` | 协议帧面 | ① | `8fc1ccd967a483166580f349c99df983c21f8f95`（status.work）: 实现 + 独立复验 CONFIRMED + 日志/登记收尾（9a175a66f + 78416a932，tier=R 不收卡）。余量：① 真机窗口（**默认流水已变，须重取基线**；构造段账本 69 抽/18 站点）；②… | login_only | 构造段账本 69 抽/18 站点（须重取基线） |
| 103 | `openmir2-ctor-draws-fold` | 协议帧面 | ① | `34442325143187a0c0a8fa53242b5f1b19f884d2`（status.work）: fold 实现 + 陈述订正已落地（0a34719c9 + e6105f011，tier=R 不收卡）。余量：① 屏障 A 的 1 抽（ServerConf.cs:114 ItemNumber += Random(100… | login_only | 两侧同种子 seq/site/result 逐项 diff，期望 68 抽/17 站点 |
| 104 | `openmir2-snap-cell-fidelity` | 协议帧面 | ③-N | `dca13e43a3710c32135c93296a7811244e7ba133`（status.work）: 实现+独立复验 CONFIRMED（befd3ad7c，tier=R 不收卡）。余量：① 真机窗口项；② 两个前置（.map 格子属性未灌进 world.maps；apply_structure_events 无生产调用… | - | work 仅「真机窗口项」，无判据原文 |
| 105 | `openmir2-default-arm-spell-surface` | 协议帧面 | ③-N | `baaab5e99f69f51d7d927149ee2511922dd40fb2`（status.work）: 帧面实现已落地（`9e59246c7` + 计数订正 `888180c07`，tier=R 不收卡）。余量：① 真机逐帧对拍（窗口项）；② 效果体层未接（MP 扣减 + `RM_HEALTHSPELLCHANGED` 先… | - | work 仅「真机逐帧对拍（窗口项）」，无具体帧 |
| 106 | `openmir2-buff-producer-wiring` | 协议帧面 | ③-N | `40ffdbb80c95a9a528f7b470c8d40985b954fa6f`（status.work）: A-5 修复已落地并由独立复验 CONFIRMED（ffac59d1b，tier=R 不收卡）。余量=真机窗口项 + 新开片：四块 snap 改用忠实镜像 `cetin_near_xy`（复验新发现的格子层差异）+ 补窗… | - | work 仅「真机窗口项」+ 新开片 |
| 107 | `openmir2-buff-system` | 协议帧面 | ③-N | `64c3a2e9821985b0b3057ddfad368d342ced7343`（status.work）: 片112 落地（tier=R，不 closed）。余量=真机窗口项 + A-5 生产接线批。 | - | work 仅「真机窗口项 + A-5 生产接线批」 |
| 108 | `openmir2-world-tick-expansion` | 协议帧面 | ① | `7dc530ddf840b212bf5cbfe854a771c7041d3bba`（status.work）: 片107 落地（tier=R，不 closed）。余量=真机窗口项（RNG 流水/时钟/公告节拍）。 | login_only | work 点名 RNG 流水/时钟/公告节拍 |
| 109 | `openmir2-transmap-maptrigger` | 协议帧面 | ① | `de9919f6419927bc366d16f11413fed7f2608035`（status.work）: 片101 已落地（tier=R，不 closed）。触发链三层齐：1658 条 route 逐字节等价 + 落格 + 踩格换图；余量=真机窗口项。 | move_walk (+login_only) | 踩格换图 + route 逐字节（走位可触发） |
| 110 | `openmir2-maptrigger-monster-and-door` | 协议帧面 | ③-N | `33f90eeac22662a2ebd949fdd94e5004515dd31b`（status.work）: 片110 落地（tier=R，不 closed）。余量=真机窗口项 W-R1-a/b/c。 | - | work 仅「真机窗口项 W-R1-a/b/c」，无展开 |
| 111 | `openmir2-orphan-encoder-wiring` | 协议帧面 | ③-N | `95084a0e9411bd9baadc4704c059c8b19e897bd1`（status.work）: 片103 落地（tier=R，不 closed）。余量=真机窗口项。 | - | work 仅「真机窗口项」，无判据原文 |
| 112 | `openmir2-window-collector-preflight` | 协议帧面 | ① | `a2922b8cd2886c5803349b2b08db4c735433bfbf`（status.work）: 片98 已落地（tier=R，不 closed）。余量=真机窗口项：两侧种子交叉断言 / C# site 列 / 前置门生产路径不误拦。 | login_only | work 点名两侧种子交叉断言 / C# site 列 |
| 113 | `openmir2-outbound-encoders-red-batch` | 协议帧面 | ② | `abfb587aec18408120f9e0cdafc9a3d1406c18c0`（status.work）: WB（片91）已并入 master b08e065fc。按 tier=R **不 closed**——待窗口项：制药抽数流水 + 帧序 + MYSTATUS 派生落库。本卡（outbound 编码器 + 发帧点接线）离线… | R2 upgrade_retrieve | work 点名制药抽数流水 + 帧序 + MYSTATUS 派生落库 |
| 114 | `openmir2-m3-monster-active-seek` | 夹具对拍(bot pair) | ① | `21af799ca1642307145502114e570f8e1022801d`（status.work）: 实现已合入（tier=R 不 closed）。余量：真机 W0/W9 对拍 + 登记未建模面（覆写 Run 的子类、SuperGuard/ArcherGuard 自有 pvp 扫描）。 | run_attack_pair.ps1 | work 点名真机 W0/W9 对拍（run_attack_pair） |
| 115 | `openmir2-m3-drop-exp-wiring` | 夹具对拍(bot pair) | ① | `bf09af50641e7ff30186dd2922112c105a71c3cf`（status.work）: 采集面已修复（T-57 扩字段 + T-58 quest 生产/防回归）。余量：真机 100 杀两相窗口（依赖 T-59 rng-scene 已落）+ quest 真机窗口。 | run_drop_exp_pair.ps1 | work 点名真机 100 杀两相窗口（run_drop_exp_pair） |
| 116 | `openmir2-m3-rng-scene-driver` | 夹具对拍(bot pair) | ① | `32c163d229710fbb909300b9eff43fc070c4a8ea`（status.work）: 已合入 master 3a5855b9e980d8ab972f53bb3a40343f6c4a2d13（tier=R 不 closed）。余量：真机两侧账本对拍（W9 窗）+ BotSrv real wiring（进 0… | run_rng_scene_pair.ps1 | work 点名真机两侧账本对拍 W9（run_rng_scene_pair） |
| 117 | `openmir2-npcs-fixed-loading` | 协议帧面 | ① | `ff22b0909b5900dc6ebdaf379753d2eec40ad9f6`（status.work）: 已合入 master b8b99926224292b1e159edf29cb7d0924334d022（tier=L，离线面完整：解析+世界面+宿主接线，15 单测+6 变异牙）。剩余 DoD = 真机两侧 NPC 出现… | login_only | 两侧 NPC 出现帧一致（login_only / npc_click） |
| 118 | `openmir2-m3-quest-align` | 夹具对拍(bot pair) | ① | `dd6b5c95381e676bc90696439b15250756d62da8`（status.work）: 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑥（master 未动）：仍 BLOCKED，三重阻断与 r2 相同：①markers 前缀口径（卡 openmir2-m3-fixture-marke… | run_quest_pair.ps1 | run_quest_pair；阻塞=markers 前缀卡 + C# k=0 截断 + quest_state 无生产者 |
| 119 | `openmir2-guardlist-guards-loading` | 协议帧面 | ① | `7fecbb54ca30f38ec887de0e29b1c0bdedaf2f0b`（status.work）: 放置半已合入 master 58924e7c1a1f5c5616ad6491a33ee329282df503（离线面完整：解析+放置+5 单测+双路变异牙）。剩余 DoD = 真机两侧项：③a 反向点出现帧一致（C# 侧… | login_only | ③a 反向点出现帧一致 + MIR2_RNG_SEED 双侧 site/order |
| 120 | `openmir2-m3-drop-exp-fixture-botsrv` | 夹具对拍(bot pair) | ① | `71625aaaf9e09e5c3fc1187021930ea0376403c0`（status.work）: 合并检查点（role=merge, openmir2-coordinator）：M3④ 掉落/经验动作序列夹具（BotSrv）+ run_drop_exp_pair.ps1 real-run 接线，含 merge 准入③… | run_drop_exp_pair.ps1 | run_drop_exp_pair 真机两侧同序列 |
| 121 | `openmir2-m3-hero-pet-align` | 夹具对拍(bot pair) | ① | `5e17007415d165d8a974b46df02c6e50b7fe2b7d`（status.work）: 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑨（master 未动）：PREREQ（静态）。master 无 run_hero_pet_pair.ps1；⑨-2/⑨-3 待认领、⑨-5（夹具）待审… | run_*_pair.ps1 | run_hero_pet_pair（master 已有该脚本） |
| 122 | `openmir2-m3-fixture-chain-gate` | 夹具对拍(bot pair) | ① | `8c34208994900992fce0a94290f4f4eaf9885afc`（status.work）: 合并检查点（role=merge, openmir2-coordinator）：夹具链深检门 O-1/O-2 收口片（o1-retro README 三处 base 515c6b5ea→f43364c45 + 追记/§5… | run_*_pair.ps1 | 各 READY 夹具 run_* pair 同序列 |
| 123 | `openmir2-m3-rng-trace-scene-groups` | 夹具对拍(bot pair) | ① | `ec7cb27a74c2821df93c4972b5d4fdccfa246eec`（status.work）: 合并检查点（role=merge, openmir2-coordinator）：本片（reviewer-5 F-1 收口：check_anchors.py symbol_unique 计数由匹配行数改匹配总次数，fail… | run_rng_trace_pair.ps1 | run_rng_trace_pair |
| 124 | `openmir2-m3-pet-slave-masterdie-wire` | 夹具对拍(bot pair) | ① | `79f2dd370394bf6a0c813e6b77283fd147d7f39c`（status.work）: 合并检查点（role=merge, openmir2-coordinator）：M3⑨-3 宝宝叛变帧面 rework（reviewer-6 F-1：NetPriorityQueue .NET8 四叉最小堆逐操作复刻 +… | run_*_pair.ps1 | run_hero_pet_pair |
| 125 | `openmir2-m3-pet-slave-amulet-delitem-frame` | 夹具对拍(bot pair) | ① | `3c12dd50cd505ba2c6fb92627bd3a9bbc47fcc02`（status.work）: 合并检查点（role=merge, openmir2-coordinator）：⑨-2 护身符扣光删除帧面（smframe::sm_delitem(202) + slave::deleted_amulet_body + … | run_*_pair.ps1 | run_hero_pet_pair |
| 126 | `openmir2-m3-party-trade-align` | 夹具对拍(bot pair) | ① | `6fec4c4156a6481063a4ef440ce35271330aa3cc`（status.work）: 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑦（master 未动）：PREREQ（静态）。run_party_trade_pair.ps1 在，但 Rust bins/game-svr 无 CM… | run_party_trade_pair.ps1 | run_party_trade_pair；阻塞=Rust 无 CM_GROUPMODE 族常量（⑦-2 未合） |
| 127 | `openmir2-m3-pet-slave-clone46` | 夹具对拍(bot pair) | ① | `2dc80f0e11f61b2d1d879de3660e7980b2fcd23f`（status.work）: 合并检查点（role=merge, openmir2-coordinator）：⑨-2 残余 46 分身（MagMakeClone/PlayCloneObject 宿主接线 + RM_TURN 帧）已合入 master … | run_*_pair.ps1 | run_hero_pet_pair |
| 128 | `openmir2-m3-shop-fixture-botsrv` | 夹具对拍(bot pair) | ① | `7ad164902f87a700937428878dd6e9ce0b6404ce`（status.work）: 合并检查点（role=merge, openmir2-coordinator）：⑤-6 残余 Minor 收口片（MShare.ClassifyShopFrames/ExtractShopPrice 由 JSON 子串定… | run_npc_shop_pair.ps1 | run_npc_shop_pair |
| 129 | `openmir2-m3-selloff-fixture-botsrv` | 夹具对拍(bot pair) | ① | `91c85d44ff564cc59940507da26ce10f2e8c6947`（status.work）: 已合入 master 78ae6319（approve reviewer-6 @reviewed_head 39f270c0；合并树四门禁 93/853/0/21 + 604）。剩余 DoD②③ = 真机两侧同序列 di… | run_selloff_pair.ps1 | run_selloff_pair |
| 130 | `openmir2-m3-fixture-marker-prefix-bug` | 夹具对拍(bot pair) | ① | `a3fae1e4cd6f5ad4d450edb2c98d50275049c637`（status.work）: 合并检查点（role=merge, openmir2-coordinator）：④ drop-exp 族（M3DROPEXPSEQ）漏登记收口片（marker_prefix_gate.py 新增 GUARD_ONLY_M… | run_*_pair.ps1 | 夹具链门 + run_* pair |
| 131 | `openmir2-m3-drop-exp-align` | 夹具对拍(bot pair) | ① | `71ba9f7dbbc21926edefc50de9ced008bf6ff0eb`（status.work）: 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核④（master 未动）：PREREQ（静态）。master 无 run_drop_exp_pair.ps1，BotSrv 无固定种子 100 杀驱动 … | run_drop_exp_pair.ps1 | run_drop_exp_pair（master 已有该脚本） |
| 132 | `openmir2-m3-pet-slave-lifecycle` | 夹具对拍(bot pair) | ① | `64ff0478693cd1472ec0a745137536e3b643f496`（status.work）: 合并检查点（role=merge, openmir2-coordinator）：⑨-3 收口片 #2（宝宝可观测效果落地：apply_slave_level_abilitys 三类支/else 支 DC+MaxHP 银行… | run_*_pair.ps1 | run_hero_pet_pair |
| 133 | `openmir2-m3-hero-pet-fixture-botsrv` | 夹具对拍(bot pair) | ① | `4fb7f524229dc974ca9b8f8b3796e2cdbd3234a2`（status.work）: 合并检查点（role=merge, openmir2-coordinator）：⑨-5 残余 Minor 收口片（锚行号 hero_pet_anchor_gate.py + make_clone(46) 用例 + pee… | run_*_pair.ps1 | run_hero_pet_pair |
| 134 | `openmir2-m3-guild-castle-fixture-botsrv` | 夹具对拍(bot pair) | ① | `838f017515154fe9eacbbebc8cbb255c7fd6a3e9`（status.work）: 合并检查点（role=merge, openmir2-coordinator）：⑧-10 残余 Minor「锚行号」收口 + reviewer-5 O-1（guild_castle_scenario.json 34 条 … | run_guild_castle_pair.ps1 | run_guild_castle_pair |
| 135 | `openmir2-m3-selloff-goldsales-load-wire` | 夹具对拍(bot pair) | ① | `94e186f649a265dab09e5a10eedcf8038e54c8da`（status.work）: 合并检查点（role=merge, openmir2-coordinator）：⑩-2 goldsales 只读加载接线已合入 master 72adb2a2e（approve openmir2-reviewer-5 1… | run_selloff_pair.ps1 | run_selloff_pair |
| 136 | `openmir2-m3-shop-dealing-gate` | 夹具对拍(bot pair) | ① | `3a5e18ea76651db083ebbb7cfde5f4505d2882e6`（status.work）: 合并检查点（role=merge, openmir2-coordinator）：T-47 ⑦ 收口『Dealing 前置门』（handle_shop_cmd 的 CM_USERBUYITEM/CM_USERGETDETA… | run_npc_shop_pair.ps1 | run_npc_shop_pair |
| 137 | `openmir2-m3-shop-dialog-wiring` | 夹具对拍(bot pair) | ① | `3253c25cda5eab3d6f1aeaa56a0ca2a252c6dad6`（status.work）: 合并检查点（role=merge, openmir2-coordinator）：⑤-4 第②片（8 条商店 CM_* 派发 + 帧：买/卖/修理/查询/仓库/详情652/仓库存取700-707，rebase 到 mast… | run_npc_shop_pair.ps1 | run_npc_shop_pair |
| 138 | `openmir2-m3-quest-fixture-botsrv` | 夹具对拍(bot pair) | ① | `fb74f09fd0816af9ff5cb28e7bc9b470a8b5ee23`（status.work）: 已合入 master 7322510b（approve reviewer-5 bc0dc557 @reviewed_head 813e9c1e；合并树四门禁 93/853/0/21 + 604；boundary 仅 te… | run_quest_pair.ps1 | run_quest_pair；-Side rust preflight 仍 PREREQ |
| 139 | `openmir2-m3-quest-persist-observability` | 夹具对拍(bot pair) | ① | `e23857ec2a3e5fd0f573cfd9e5f760d94abf82fa`（status.work）: 合并检查点：M3⑥-4「固化 oracle 任务态不落库语义 + 门禁」已合入 master 67e9f6c3e（approve openmir2-reviewer-5 3dc49a83 @reviewed_head 0… | run_quest_pair.ps1 + scoped_diff.py | run_quest_pair + scoped_diff --canonical |
| 140 | `openmir2-m3-mail-auction-align` | 夹具对拍(bot pair) | ① | `f4bc3d73d67d66bcb71bf0dd26f73b81cda5b6ae`（status.work）: 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑩（master 未动）：PREREQ（静态）。run_selloff_pair.ps1 在，但 Rust bins/game-svr 无 CM_SEL… | run_selloff_pair.ps1 | run_selloff_pair；阻塞=Rust 无 CM_SELLOFF* 常量 |
| 141 | `openmir2-m3-party-deal-capacity-int` | 夹具对拍(bot pair) | ① | `167a5c115d13898d007f405dfd7b3b98f4dd5eb9`（status.work）: 合并检查点（role=merge, openmir2-coordinator）：M3⑦-3 · M-3 收口（trade.rs::deal_end 两条背包容量检改 oracle 精确 int 算术，新 fn bag_c… | run_party_trade_pair.ps1 | run_party_trade_pair |
| 142 | `openmir2-m3-clock-injection` | 夹具对拍(bot pair) | ③-O | `979ae58ca4abaeea9dfa6b54163f018dbeb4f3c0`（status.work）: 合并检查点（role=merge, openmir2-coordinator）：M3⑧ 横切前置 Rust 半（MIR2_CLOCK_MS 可注入时钟：env_clock 解析/fail-closed + CLOCK_P… | - | 需 C# 影子 hook 半（HUtil32.GetTickCount 注入），非场景可补 |
| 143 | `openmir2-m3-pet-slave-summon` | 夹具对拍(bot pair) | ① | `a57ff928e6620fa8ba00638f23d72d34be278ab4`（status.work）: 合并检查点（role=merge, openmir2-coordinator）：⑨-2 M-3 收口片（本片 = 仓库卫生离线片：mir2-rs/.gitattributes 追加 `*.rc text eol=lf`，… | run_*_pair.ps1 | run_hero_pet_pair |
| 144 | `openmir2-m3-party-gainexp-npe` | 夹具对拍(bot pair) | ① | `8b8d1cf31aaf0a567923d5c53ed7877dbeb8bd79`（status.work）: 合并检查点（role=merge, openmir2-coordinator）：⑦ 收口 M-2（GainExp 组队缺态对齐 oracle NPE 吞错口径：group owner 不在注册表 ⇒ 零经验/零授奖）已合… | run_party_trade_pair.ps1 | run_party_trade_pair |
| 145 | `openmir2-m3-shop-scenario-authority` | 夹具对拍(bot pair) | ① | `82345d07b57f712b428446135d2470e39ff26e52`（status.work）: 合并检查点：本片（⑤-6 reviewer-6 M-3 收口：npc_shop_diff.py --scenario 单一真源 + runner 透传 + fail-closed rc=2）已合入 master 4adf… | run_npc_shop_pair.ps1 | run_npc_shop_pair |
| 146 | `openmir2-m3-fixture-matrix` | 夹具对拍(bot pair) | ① | `ba38dae0e6b840696b70c0b93918c328360966ba`（status.work）: 合并检查点：M3 十项夹具矩阵与就绪度门已合入 master 3f0b5bc0（approve reviewer-5 @reviewed_head 075a9d63；合并树四门禁 fmt0/clippy0/94 suit… | run_*_pair.ps1 | 十项夹具 run_* pair |
| 147 | `openmir2-m3-guild-castle-align` | 夹具对拍(bot pair) | ① | `a62b5539fc27826df6a7e6ca2ddc0c5b0d28e8eb`（status.work）: 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑧（master 未动）：PREREQ（静态）。master 无 run_guild_castle_pair.ps1；实现侧 ⑧-3..⑧-9 待认领、… | run_guild_castle_pair.ps1 | run_guild_castle_pair；前置 ⑧-3..⑧-9 |
| 148 | `openmir2-m3-npc-shop-align` | 夹具对拍(bot pair) | ① | `53dae62898fe418a47962cf7364a1dad71c904a3`（status.work）: 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑤（master d4d5b89c 未动，与 r1/r2 相同）：仍 BLOCKED，与 r2 逐条相同。静态（00-static-readiness.… | run_npc_shop_pair.ps1 | run_npc_shop_pair；前置「NPC 入视野/商店接口」 |
| 149 | `openmir2-m3-scoped-diff-ability` | 夹具对拍(bot pair) | ① | `b3d4a788c195d19375c5d91f4ad980863d33f6fd`（status.work）: 窗口 r3（master d4d5b89c 未动）真机加深复跑 ②③F-2（360 击）：attack_diff 仍 GREEN diff=0（360/157/203/6576）；scoped_diff --canoni… | run_attack_pair.ps1 + scoped_diff.py | run_attack_pair + scoped_diff --canonical |
| 150 | `openmir2-m3-party-trade-fixture-botsrv` | 夹具对拍(bot pair) | ① | `872e8662fa9f02fcedef138b632a0df40d93be11`（status.work）: 已合入 master 12039f60（approve reviewer-5 4901e222 @reviewed_head d29d017b；合并树四门禁 93/853/0/21 + 604）。剩余 DoD②③ = 真… | run_party_trade_pair.ps1 | run_party_trade_pair；等 ⑦-2/⑦-3 |
| 151 | `openmir2-m3-final-acceptance` | 长跑(300 假人 24h) | ③-O | `362deb36cd8a6d0754770cda9d2a436f9907a3c1`（status.work）: **DoD① 已完成**：十项表逐行补实（含 ⑩ 拆邮件/拍卖两栏）+ whitelist T-38..T-40（⑧ N1/N2/N5）合入 master 451af1bc（approve reviewer-5 @077… | - | 300 假人 24h 长跑，非单次 run |
| 152 | `openmir2-click-path-bag-writeback` | 协议帧面 | ② | `bcb0c89afccf95f77401b030f1e9a28b9876be61`（review.window_items）: 真机：固定 NPC 点击（CM_CLICKNPC）触发 @main 的 `take <物品> N` 后断线，DBSvr 落库 characters_bagitem 行数/字段（MakeIndex/Index/Dura/DuraMax）与 o… | R1 npc_script_chain | 同 script-take-persist：click 支 take/GIVE/TAKEW 后断线落库 |

## 8. 自检（机判：三类之和 = 152 且每卡只出现一次）

自检脚本**不单独入库**（本卡 write_set 只有本文件）；把它存成临时文件、在仓库根跑即可：
`python -I -B <临时路径>/selfcheck.py`。

```python
import re
from collections import Counter

doc = open("docs/window-debt-reclass-20261023.md", encoding="utf-8").read()
sec = doc.split("## 7. 全量 152 张主表")[1].split("## 8.")[0]
rows = [l for l in sec.splitlines() if re.match(r"^\| \d+ \| `openmir2-", l)]
ids = [re.match(r"^\| \d+ \| `(openmir2-[a-z0-9-]+)`", l).group(1) for l in rows]
bucket = [l.split("|")[4].strip() for l in rows]
c = Counter(bucket)
dup = [k for k, v in Counter(ids).items() if v > 1]

print("rows =", len(rows))
print("distinct ids =", len(set(ids)))
print("1 =", c["①"], "2 =", c["②"])
print("3G =", c["③-G"], "3O =", c["③-O"], "3N =", c["③-N"])
print("sum =", c["①"] + c["②"] + c["③-G"] + c["③-O"] + c["③-N"])
print("dups =", dup)

assert len(rows) == 152 and len(set(ids)) == 152 and not dup
assert c["①"] + c["②"] + c["③-G"] + c["③-O"] + c["③-N"] == 152
print("OK: three classes sum to 152, every card exactly once")
```

实测输出（本文件生成时）：

```
rows = 152
distinct ids = 152
1 = 60
2 = 43
3G = 1 3O = 26 3N = 22
sum = 152
dups = []
OK: three classes sum to 152, every card exactly once
```

## 9. 遗留

1. **③-N 的 22 张要补 `window_items`**：卡 owner / Leader 补完后可直接重跑本表（本文件的判定规则是机械的，重判不需要改文档结构）。
2. **③-O 里 5 张是「工具能力缺口」而不是「本质不可对拍」**：`chat-channels`（收件人集合）、`run-gap-checkdupobj` / `movetomovingobject-quirk`（多对象同格）、`disappear-recipients-gridscan`（第二观察者）、`gate-idle-timeout`（连接关闭事件）——
   补 driver 的「多会话 / 连接事件」能力后可降级到 ②，本表不预判。
3. **① 的 34 张 m3 夹具卡**依赖各 `run_*_pair.ps1` 的 `-Side rust` 前置（多数卡自己写明了未落项），
   本表把它们归 ① 是因为**判据与夹具都已存在**，不是因为它们已经能跑绿。
4. **本文件不改任何代码/场景 JSON/规则表**；② 的 7 个配方是派工建议，落地另开卡。
