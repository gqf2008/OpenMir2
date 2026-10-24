# 窗口卡 × 场景覆盖报告（2026-10-24）

- 卡：`openmir2-window-card-scenario-coverage`（tier=L，write_set=`docs/`）。
- 数据时点：`@ 917376a45 @ 2026-10-24 11:14 +08`；账本读数 `walgit collab board` 与 `goal_gate` 均在该 commit 上实跑。
- 事实源只读：`docs/window-batch-plan-20261023.md` 未改一行；本报告只引用其 §/行号。

## 1. 口径与总数

### 1.1 卡片全集（`window_required == true`，含未合并）

`python -I tools/goal/goal_gate.py run --json --skip-tests --skip-script` 的 `conditions[2].evidence.r_cards`
中 `window_required == true` 共 **139 张**（`@ 917376a45 @ 2026-10-24 11:14 +08`）。按 goal_gate 的 `category` 拆：

| category | 张数 |
|---|---:|
| `adjudicated` | 2 |
| `closed` | 4 |
| `window_required` | 131 |
| `window_required_unmerged` | 2 |
| **合计** | **139** |

### 1.2 与 goal_gate 详情行的差异（必须写清）

详情行写「window_required **131**（其中未合并 2）」。**131 是 `category=window_required` 的已合入桶**；
两张未合入卡在 `category=window_required_unmerged`（单独桶），并不在 131 里。按 `window_required` 旗标枚举是 **139**：
131 已合入 + 2 未合入 + 4 已 `closed` 仍带旗标 + 2 已裁定 L。本报告用 **139** 作覆盖率全集（卡面要求「含未合并的」），
并在 §5 单列 `closed` 的 4 张、在 §3 单列 `adjudicated` 的 2 张。

### 1.3 三分类总数

| 分类 | 张数 | 说明 |
|---|---:|---|
| 有场景（当前可跑） | 45 | 第 1/2 批 37 张 + §7 刷新后转可跑的 §3 卡 7 张 + §7.2 散文 1 张 |
| 有替代判据 | 27 | §4 不可协议对拍 25 + owner 裁定 L 2 |
| 无当前可跑场景 | 63 | blocked 5 / 不可达 4 / 需新场景 28 / 仅作依赖 1 / 文档无此卡 25 |
| **活跃合计** | **135** | 139 − 4 张已 `closed` |
| 已 `closed`（不再构成窗口债） | 4 | 见 §5 |

一句话：**开一次窗口最多覆盖 45 张；63 张没有任何当前可跑场景——其中 25 张文档完全无此卡，1 张仅作依赖被引。**

## 2. 有场景（45 张，当前可跑）

| 卡 id | goal_gate 分类 | 文档归属（行） | 场景 / pair 夹具 | 备注 |
|---|---|---|---|---|
| `openmir2-a09-upgrade-item` | window_required | §3 第 3 批（需新帧/新场景/新夹具） L231 | 新：R2 upgrade_retrieve | **§7.2/§7.4 刷新后转可跑**；ItemNameList 5 条 + 124B 尾 + 穿戴门槛 |
| `openmir2-account-protection-13` | window_required | §3.1F 第 1 批 B（既有 pair 夹具） L145 | run_account_protection_roundtrip.ps1 | 夹具在 mir2-rs/tests/parity/m3/account-protection/（既有）；比建号 13 列 + 改资料 12 列 + PassWo… |
| `openmir2-attack-weapon-wear-642` | window_required | §7.2（散文，无表格行） | `equip_struck_save`（§7.2 明确移入 §7.4） | **§7.2/§7.4 刷新后转可跑**；642 攻击方武器磨损已接线；§7.4 有 `equip_struck_save` 行 |
| `openmir2-buff-system` | window_required | §3 第 3 批（需新帧/新场景/新夹具） L180 | 新：R5 spell_cast + DoT trace | **§7.2/§7.4 刷新后转可跑**；毒伤逐次 MIR2_RNG_LOG；buff 递减节奏待 A-R2 裁定 |
| `openmir2-card-window-tool` | window_required | §3.1F 第 1 批 B（既有 pair 夹具） L146 | card_window.py 真 alt 剖面一次 | 两侧 stop rc=0、端口全释放、report_kind=compare |
| `openmir2-click-path-bag-writeback` | window_required | §3.1P 第 1 批 A（可跑 probe 场景） L120 | npc_click + db_snapshot | 点击 @main take/give 后断线，落库行/列比对 |
| `openmir2-db-encoding` | window_required | §3.1P 第 1 批 A（可跑 probe 场景） L114 | login_only + db_snapshot | ChrName 列值（建号/选角/存档） |
| `openmir2-delmagic-addmagic-chain` | window_required | §3.2B 第 2 批 B（新发现规则） L159 | bag_equip | SM_BAGITEMS(201) 发现规则**已落地**（`openmir2-discovery-bagitems-201`，构造级 `check-all` r… |
| `openmir2-encoding-core-e1-e2` | window_required | §3.1P 第 1 批 A（可跑 probe 场景） L110 | npc_click / merchant_dialog | 非 ASCII 说词逐字节；BOM 两路属离线（见不可对拍） |
| `openmir2-gamesvr-serverinfo-report` | window_required | §3.1P 第 1 批 A（可跑 probe 场景） L124 | login_only | 必跑 ident 529 逐字节（body_hex 对照） |
| `openmir2-gm-impl-shutup` | window_required | §3.1P 第 1 批 A（可跑 probe 场景） L127 | login_only + 手动 @禁言 | 前置 openmir2-gm-script-host-seam 已合（只通 @Login 路径） |
| `openmir2-gm-script-host-seam` | window_required | §3.1P 第 1 批 A（可跑 probe 场景） L128 | login_only + 手动 @禁言 <名> 1 | 探针不发 GM 命令，runbook 要求操作者手输；双侧比「权限不够!!!」 |
| `openmir2-guard-ctor-constants` | window_required | §3.1P 第 1 批 A（可跑 probe 场景） L118 | login_only | 卫士自扫半径 5→7 的同 tick 可见集/事件序 |
| `openmir2-guild-g2-dispatch` | window_required | §3.1P 第 1 批 A（可跑 probe 场景） L109 | login_only | 750 出队序（46 之后、654 之前） |
| `openmir2-healthspell-emission-model` | window_required | §3 第 3 批（需新帧/新场景/新夹具） L211 | 新：R5 spell_cast | **§7.2/§7.4 刷新后转可跑**；整 tick 53 序列 + SKILL_SHOWHP |
| `openmir2-login-529-status-text` | window_required | §3.1P 第 1 批 A（可跑 probe 场景） L121 | login_only | 前置 gamesvr-serverinfo-report 已合入 master 185a6c8cb（SS_SERVERINFO 上报已接）；本卡窗口=用当前二进… |
| `openmir2-login-654-order` | window_required | §3.1P 第 1 批 A（可跑 probe 场景） L112 | login_only | 654 跨 tick、第二 42、100×4；有会角色 811/46/750 |
| `openmir2-login-charstatus-657` | window_required | §3.1P 第 1 批 A（可跑 probe 场景） L115 | login_only | 657×2 与邻居相对序 |
| `openmir2-login-frame-order-52-752` | window_required | §3.1P 第 1 批 A（可跑 probe 场景） L107 | login_only | 52 布局(54,78)/621/211 相对序 |
| `openmir2-login-numeric-gaps` | window_required | §3.1P 第 1 批 A（可跑 probe 场景） L108 | login_only（首杀项另需 combat_hit） | 首登 52 Weight/MaxExp=10 可先验；44/45/52/752 首杀序需第 2 批 |
| `openmir2-m3-party-attribution` | window_required | §3.1F 第 1 批 B（既有 pair 夹具） L137 | run_party_trade_pair.ps1 | 组队归属/分赃：前置 ⑦-2/⑦-3（卡面称待合，开工前复核） |
| `openmir2-m3-party-gainexp-npe` | window_required | §3.1F 第 1 批 B（既有 pair 夹具） L138 | run_party_trade_pair.ps1 | 同上 |
| `openmir2-m3-pet-slave-clone46` | window_required | §3.1F 第 1 批 B（既有 pair 夹具） L141 | run_hero_pet_pair.ps1 | make_clone step |
| `openmir2-m3-pet-slave-lifecycle` | window_required | §3.1F 第 1 批 B（既有 pair 夹具） L139 | run_hero_pet_pair.ps1 | 脚本已在 master；前置 ⑨-2/⑨-3 实现 |
| `openmir2-m3-pet-slave-masterdie-wire` | window_required | §3.1F 第 1 批 B（既有 pair 夹具） L142 | run_hero_pet_pair.ps1 | 主人死 step + 混合队列 |
| `openmir2-m3-rng-scene-driver` | window_required | §3.1F 第 1 批 B（既有 pair 夹具） L143 | run_rng_scene_pair.ps1 | W9 RNG 序列 |
| `openmir2-m3-shop-dialog-wiring` | window_required | §3.1F 第 1 批 B（既有 pair 夹具） L140 | run_npc_shop_pair.ps1 | ⑤-4 商店派发/帧；storageitem 50 槽 |
| `openmir2-market-def-impl` | window_required | §3.1P 第 1 批 A（可跑 probe 场景） L113 | merchant_dialog | @main/[~@repair] 27 处 $STR 展开 |
| `openmir2-mb8-9-monsters` | window_required | §3 第 3 批（需新帧/新场景/新夹具） L227 | 新：R5 spell_cast race 215 | **§7.2/§7.4 刷新后转可跑**；需先接 T-148 |
| `openmir2-menu-ok-unify-pickwire` | window_required | §3.1P 第 1 批 A（可跑 probe 场景） L126 | dice_chain_auto | 767 分支；回稿后同场景复跑 |
| `openmir2-npc-ctor-constants` | window_required | §3.1P 第 1 批 A（可跑 probe 场景） L117 | login_only | SM_TURN series 低=4 高=2、Merchant 格 30s 续期 |
| `openmir2-npc-display-table` | window_required | §3.1P 第 1 批 A（可跑 probe 场景） L116 | login_only + npc_click | 同摆位 0122 (29,33) 出现帧/3 对 nameless 转身/点击 @main |
| `openmir2-outbound-encoders-red-batch` | window_required | §3 第 3 批（需新帧/新场景/新夹具） L178 | 新：R2 upgrade_retrieve | **§7.2/§7.4 刷新后转可跑**；制药链 Random 流水 + 帧相对序 + MYSTATUS 落库 |
| `openmir2-script-playdice-openybdeal` | window_required | §3.1P 第 1 批 A（可跑 probe 场景） L111 | dice_chain | 赌博 NPC playdice 标签放行；OPENYBDEAL 三支部署不可达另记 |
| `openmir2-script-take-persist` | window_required | §3.1P 第 1 批 A（可跑 probe 场景） L119 | dice_chain + db_snapshot | @main→@kaiz→@ya5→@ya52 链正是 dice_chain；比 characters_bagitem/Gold/useitems |
| `openmir2-searchtick-origin-parity` | window_required | §3.1P 第 1 批 A（可跑 probe 场景） L122 | login_only（--dwell-ms ≥2000） | 抓首 SM_TURN/41 与 654/657×2 的到达 tick 窗口 |
| `openmir2-t138-request-castle-war` | window_required | §3.2A 第 2 批 A（新发现规则） L153 | king_castle_war | king_actor 需新规则（643 规则 + 标签约束，待实测确认） |
| `openmir2-transmap-maptrigger` | window_required | §3.2A 第 2 批 A（新发现规则） L152 | move_walk | move_x/move_y 需新规则（self_x/self_y + 场景声明 delta） |
| `openmir2-unbindlist-consumer` | window_required | §3.2B 第 2 批 B（新发现规则） L160 | eat_item | 规则同 `bag_equip`（已落地，构造级）；另需 StdMode31/AniCount0 捆装包夹具 |
| `openmir2-upgrade-npc` | window_required | §3 第 3 批（需新帧/新场景/新夹具） L216 | 新：R2 upgrade_retrieve | **§7.2/§7.4 刷新后转可跑**；抽数序 A→B→C/D/E + Desc[10] + 帧序 |
| `openmir2-verify-tick-base` | window_required | §3.1P 第 1 批 A（可跑 probe 场景） L123 | login_only | 判据实质待 T-73 登录等待环建模；今日生产路径无可观测差异 |
| `openmir2-weapon-upgrade-values` | window_required | §3 第 3 批（需新帧/新场景/新夹具） L232 | 新：R2 upgrade_retrieve | **§7.2/§7.4 刷新后转可跑**；@upgradenow 四字段 + n90 逐抽 |
| `openmir2-window-collector-preflight` | window_required | §3.1F 第 1 批 B（既有 pair 夹具） L144 | card_window.py 真栈一次（login_only） | 起栈后 INJECT_OK/RUST_SEED_INJECTED 交叉断言 |
| `openmir2-world-tick-expansion` | window_required | §3.1P 第 1 批 A（可跑 probe 场景） L89 | login_only + MIR2_RNG_LOG | RNG trace 双开；npc_tick 次序已写进契约 |
| `openmir2-ya52-menu-ok-missing` | window_required | §3.1P 第 1 批 A（可跑 probe 场景） L125 | dice_chain_auto | 末步断言 SM_MENU_OK(767) |

## 3. 有替代判据（27 张）

| 卡 id | goal_gate 分类 | 文档归属（行） | 替代判据 / 为什么不能单窗口协议对拍 |
|---|---|---|---|
| `openmir2-a22-castle-persistence` | window_required | §3 第 4 批（不可协议对拍） L278 | 文件字节 sha256 + 10min Save + 跨午夜；离线文件；窗口项：W-A22-1 文件字节：C#/Rust 各跑启动+一次 10min Save，比 List.txt / Castle/SabukW.txt…；W-A22-2 … |
| `openmir2-chat-channels` | window_required | §3 第 4 批（不可协议对拍） L269 | 三前缀收件人集合 ⇒ 多会话；窗口项：真机：三前缀的收件人集合与帧字节；10s 门边界（严格 >）（共 3 项） |
| `openmir2-dbsrv-fault-paths` | window_required | §3 第 4 批（不可协议对拍） L266 | 需停/重启 MySQL（纪律禁止）；替代=故障注入夹具；窗口项：真机 MySQL 停机/重启下的四种降级帧与客户端表现；9 表与 characters_indexes 的跨实现逐字段 diff |
| `openmir2-dbsrv-scheme-a` | window_required | §3 第 4 批（不可协议对拍） L271 | 慢存档时延与 5100 全序；工具缺口；窗口项：真机 W-DB-1..7：慢存档下的 QUERYCHR/SELCHR/LOAD 时延；5100 回包全序（共 4 项） |
| `openmir2-encoding-crate` | window_required | §3 第 4 批（不可协议对拍） L274 | data/envir 文本逐字节 + BOM 两模型；离线语料；窗口项：真机：data/envir 承载的文本（行会文件/物品名/配置）逐字节；不可映射字符与 BOM 两模型的现网语料复核 |
| `openmir2-gate-idle-timeout` | window_required | §3 第 4 批（不可协议对拍） L265 | 真实客户端挂起 60s；长跑/客户端驱动缺口；窗口项：真实客户端挂起 60s 的被踢时序与畸形超时帧的实际反应；下行断链后网关的完整恢复路径（重连未实现，另卡） |
| `openmir2-gategate-keepalive` | window_required | §3 第 4 批（不可协议对拍） L270 | 10s 健康帧/15 天空闲；长跑；窗口项：真机：10s 健康帧周期与首帧相位；3min 窄竞态分支的可达性（需 owner 裁决）（共 3 项） |
| `openmir2-gategate-slots-gm` | window_required | §3 第 4 批（不可协议对拍） L264 | GameGate 多进程 + GM_CHECKCLIENT/RECEIVE_OK…；窗口项：C# GameSvr + Rust GameGate 的 GM_CHECKCLIENT→SendKeepAlive→GM_CHECKSERV…；大于 40KB … |
| `openmir2-gm-batch-4-numeric` | window_required | §3 第 4 批（不可协议对拍） L282 | T-187 两号同时在线比目标侧帧；单号可先验 T-186（gm_batch 不…；窗口项：接 SessionGround 数值态（不是 Entity；Entity.gold 恒 0）+ lib.rs 落用；SETPERMISSIO…；两号同时在线，比… |
| `openmir2-guild-g3-persistence` | window_required | §3 第 4 批（不可协议对拍） L268 | 两在线会话一方断线 + 文件字节；多会话 + 文件；窗口项：窗口 A：两个在线行会成员一方断线，对端恰收一条 104（Recog/Param/Series/GBK 尾，ShowPreFixMsg=0…；SS_208 事件… |
| `openmir2-hutil32-parity` | window_required | §3 第 4 批（不可协议对拍） L273 | b1_six_replay 的 NaN/／x／≥2^31 分支；离线夹具；窗口项：b1_six_replay 的 round 在 NaN/／x／≥2^31 分支上的真机对拍 |
| `openmir2-m3-clock-injection` | window_required | §3 第 4 批（不可协议对拍） L257 | C# HUtil32.GetTickCount/DateTime.Now 不可替…；窗口项：剩余 DoD = ① C# 影子 hook 半（HUtil32.GetTickCount/DateTime.Now 不可替换字段，待 owner 在 Mono.… |
| `openmir2-m3-monster-active-seek` | window_required | §3 第 4 批（不可协议对拍） L258 | GUI/夹具驱动，非协议对拍；替代=W0/W9 夹具 + G3 窗口；窗口项：战斗数值/时序/RNG 真机对拍（W0/W9）。approve.window_required=true |
| `openmir2-m3-pet-slave-summon` | adjudicated | 账本 `adjudicated`（owner 裁定 L） | 纯仓库卫生/属性口径片：只动 mir2-rs/.gitattributes（新增 `*.rc text eol=lf` + 注释）与新增证据目录；零 .rs、零 src/sql/MirClient/crates/bins/tools 改动，… |
| `openmir2-m3-rng-trace-scene-groups` | adjudicated | 账本 `adjudicated`（owner 裁定 L） | 零 .rs / 零 .toml：差量只在 mir2-rs/tests/parity/m3/rng-trace-scene/**（check_anchors.py 计数口径 + 文档数字 + 一处注释）+ 新证据目录；不触 RNG 抽数序列、… |
| `openmir2-monster-ref-drains` | window_required | §3 第 4 批（不可协议对拍） L279 | 四类帧收件人集合 ⇒ 多会话；窗口项：真机四类帧逐字节与收件人集合（需先让 race 203/206/210/215 出现在部署数据）；Khazard 玩家目标作 sender 的 SM_SPACE… |
| `openmir2-monster-showhp-53-broadcast` | window_required | §3 第 4 批（不可协议对拍） L285 | ±12 帧集合 + VisibleHumanList 缓存 ⇒ 多会话；窗口项：ShowHp 怪/NPC ±12 帧集合、53/1100/1101 同 tick 序、ObMode 源自见 2 条、VisibleHumanList 500ms… |
| `openmir2-npc-hit-frames` | window_required | §3 第 4 批（不可协议对拍） L280 | ≥180s 观察 + ±12 收件人集合；窗口项：C# 本尊正面样本 ≥180s（同位置必须出现 ident=14 recog=国王 actor）；帧形逐字节（Recog/param/tag/series/12… |
| `openmir2-openybdeal-host-wiring` | window_required | §3 第 4 批（不可协议对拍） L276 | 掷骰/寄售面板与客户端动画；GUI；窗口项：W-SPD-1 真机掷骰：playdice 1 @cube82 → 客户端出动画（SM_PLAYDICE 8001）→ 回发 @cube82…；W-1/W-2/… |
| `openmir2-orphan-encoder-wiring` | window_required | §3 第 4 批（不可协议对拍） L259 | ②/③ 需两真客户端 + ±12 收件人集合，单客户端验不到；替代=构造级收件人…；窗口项：① 玩家/怪物走出视野时旧位置幽灵应消失（原缺口症状）；② 两真客户端间 /名字 正文 私聊双向可见；③ 离世广播收件人集合口径（本实现=visible_act… |
| `openmir2-selgate-reconnect` | window_required | §3 第 4 批（不可协议对拍） L267 | 需 DBSrv 重启；工具缺口；窗口项：真机 DBSrv 重启后 SelGate 的重连与在途会话；断链期间真实客户端的表现 |
| `openmir2-single-binary-roles` | window_required | §3 第 4 批（不可协议对拍） L277 | RNG trace groups + 日志逐行；离线日志；窗口项：真实 game-svr（MIR2_RNG_LOG + MIR2_RNG_SEED，startup 场景）取 trace，rng_ledger…；旧 exe 与 … |
| `openmir2-sm-closehealth-wire` | window_required | §3 第 4 批（不可协议对拍） L281 | ±12 观察者 53/1101 集合 ⇒ 多会话；窗口项：玩家目标与怪物目标各跑一次：心灵启示 → 1100 出现 → ShowHpInterval 到点 → 1101（Recog=到期者、±12…；同 tick 有回… |
| `openmir2-sm-reconnect-wiring` | window_required | §3 第 4 批（不可协议对拍） L284 | 换服/重连帧面；重连未实现 + 工具缺口；窗口项：真机换服/重连帧面 + GateCount==1 的 RNG 流水。残余：M-2 nServerIndex 源偏差（G11 落地时改为 ChangeSpaceM… |
| `openmir2-sm-skeleton-wiring` | window_required | §3 第 4 批（不可协议对拍） L286 | ±12 收件人集合 + 12 格边界各放静止号 ⇒ 多会话；窗口项：双栈同一账号连续屠宰「鹿(150)/鸡(50)」直到出骨架，判 SM_SKELETON(33) 头+8B CharDesc 逐字节、收件人集合（±12 格 Ra… |
| `openmir2-strlist-bom` | window_required | §3 第 4 批（不可协议对拍） L275 | BOM 行会/物品/列表文件；离线夹具；窗口项：真机：带 BOM 的行会/物品/列表文件读取 |
| `openmir2-ybdeal-frames` | window_required | §3 第 4 批（不可协议对拍） L283 | 部署 OPENYBDEAL=0/QUERYYB=0 ⇒ 部署级不可达；替代=登记…；窗口项：可达性实测 OPENYBDEAL=0/QUERYYB=0（四条部署 Envir），判部署级不可达 ⇒ 走登记路径（未硬造发射点）：T-203 登记 + 3 条用… |

## 4. 无当前可跑场景（63 张）

### 4.1 场景 `blocked`（5 张）

依据 §7.3：`combat_hit` / `combat_strike` 自身 `blocked`。**建议**：先补「同屏第二会话/同格怪物夹具」或
`wait_for` 的 recog 过滤；在此之前这些卡只能登记为「跑也没用」。

| 卡 id | 文档归属（行） | 场景 | 窗口项要点（截断） |
|---|---|---|---|
| `openmir2-merchant-race-bias` | §3 第 3 批（需新帧/新场景/新夹具） L230 | 新：combat_hit（HAM_* 模式打商人） | 对照 race 50；RNG 少一次 Random(SpeedPoint) |
| `openmir2-monster-attack-hit-frame` | §3 第 3 批（需新帧/新场景/新夹具） L210 | 新：combat_hit（同点 605,578 / L1 / 60s） | 14/31 相对序与 AttackDir |
| `openmir2-run-gap-pl1` | §3 第 3 批（需新帧/新场景/新夹具） L209 | 新：combat_hit + 存档 | P21 帧序 / P32 复活链 / 53 同 tick |
| `openmir2-superman-normnpc` | §3 第 3 批（需新帧/新场景/新夹具） L228 | 新：combat_hit + GM Kill | 城门/守卫/固定 NPC 的 SuperMan 行为 |
| `openmir2-weapon-durability-frames` | §3 第 3 批（需新帧/新场景/新夹具） L242 | 新：combat_hit + 装备耐久 | 655/1102 帧面 |

### 4.2 场景不可达（4 张）

依据 §7.3：`castle_zone`（R6）仓库内无场景文件。**建议**：登记为「需双行会 DB + 私有根 + 开战钟点 + 多会话」的
不可达场景；在场景落地前不得计入可覆盖数。

| 卡 id | 文档归属（行） | 场景 | 窗口项要点（截断） |
|---|---|---|---|
| `openmir2-castle-guard-target` | §3 第 3 批（需新帧/新场景/新夹具） L218 | 新：R6 castle_zone | 同城/盟友/UnderWar 三态 |
| `openmir2-castle-merchant-disappear-downlink` | §3 第 3 批（需新帧/新场景/新夹具） L239 | 新：R6 castle_zone 城堡战 | ⑧ 战中帧集合 diff=0 |
| `openmir2-castle-merchant-fixedhide-hit` | §3 第 3 批（需新帧/新场景/新夹具） L238 | 新：R6 castle_zone 城堡战 | ⑧ 战中帧集合 diff=0 |
| `openmir2-castle-war-area-impl` | §3 第 3 批（需新帧/新场景/新夹具） L219 | 新：R6 castle_zone | 766 bit4/1s 门/四臂落点 |

### 4.3 文档已登记、需新场景 / 新帧 / 新夹具（28 张）

§3 表格给了「新：…」场景名，但场景尚未落地（或关键帧未接）。**建议**：按 §5.3 的模板归并到新场景卡；
每张卡的场景名已足够开卡，不需要重新分类。

| 卡 id | 文档归属（行） | 计划场景 | 窗口项要点（截断） |
|---|---|---|---|
| `openmir2-action-line-variable-text` | §3 第 3 批（需新帧/新场景/新夹具） L233 | 新：R1 npc_script_chain（SENDMSG/MESSAGEBOX 支） | 真机 C# 跑部署现成素材 `E:/MirServer/Mir200/Envir/QFunction-0.txt:388` 的 `SENDM…；真机 C# 跑 … |
| `openmir2-castle-entities` | §3 第 3 批（需新帧/新场景/新夹具） L217 | 新：castle fixture（16 实体/96 抽） | 部署 A 情景：启动后 0 实体、4 条告警（Castle/Castle/SabukW.txt）；夹具 B：16 实体、24 次 Regen、8 告警、96 抽… |
| `openmir2-castle-pure` | §3 第 3 批（需新帧/新场景/新夹具） L225 | 新：castle fixture（商人税三窗） | W1 沙巴克商人买/卖/修/升级后 TotalGold/TodayIncome 增量；W2 城主行会成员价（共 3 项） |
| `openmir2-cm-adjust-bonus` | §3 第 3 批（需新帧/新场景/新夹具） L241 | 新：adjust-bonus 场景（752→52→657→252） | 真机对拍 752→52→657→252 的字节、条数与次序（安静队列前提）。残余：lib.rs:3476 旧注释 RM_ABILITY(19)/RM_FEATU… |
| `openmir2-door-chain-wiring` | §3 第 3 批（需新帧/新场景/新夹具） L237 | 新：CM_OPENDOOR 场景 | ① C# oracle 真机：部署图 `[0 比奇省]`（0.map，700×700，79 门/21 状态）对真实门格发 `CM_OPEND…；② 城堡子系统覆… |
| `openmir2-frame-drain-order-audit` | §3 第 3 批（需新帧/新场景/新夹具） L246 | 新：5 个场景（施法/交易/寄售/组队/NPC） | 5 场景，gw-capture 可复跑）：施法多条 SM_STRUCK(31) 序 [0,2,1]/[0,3,2,1]、交易 4 条红字(252)、寄售多条 M… |
| `openmir2-gategate-speed-rules` | §3 第 3 批（需新帧/新场景/新夹具） L212 | 新：login 相位场景 | 登录包下发相位：oracle 入 MsgList 后到 drain 才 Send（0-210ms 不确定），Rust 仍立即下发——真机同相…；登录后 0-21… |
| `openmir2-gm-admin-mode-entity-seam` | §3 第 3 批（需新帧/新场景/新夹具） L240 | 新：GM 5 命令场景（@3/@2 等） | ①-④）**：5 命令真机帧面/行为、@3 挨打零低血 53 + 登出满血存档、改档→登出→复登记录比对、@2 RM_DISAPPEAR。残余 m-1（@Res… |
| `openmir2-gm-commands-design` | §3 第 3 批（需新帧/新场景/新夹具） L234 | 新：GM 特权命令场景（SETPERMISSION=10）+ 多会话 | 真机 SETPERMISSION=10 + 戴传送装备（T-169 前置）跑 @传送：两次调用间隔卡在 UserMoveTime*1000+…；两号（DB ch… |
| `openmir2-guild-official-npc` | §3 第 3 批（需新帧/新场景/新夹具） L222 | 新：R1 npc_script_chain | 真机：建会成功/失败各支帧字节与 622 负重；宣战战表与 104 文案（共 3 项） |
| `openmir2-loader-fields` | §3 第 3 批（需新帧/新场景/新夹具） L220 | login_only（部分）+ 新加载器接线 | 真机：StartPoint/Server.conf 11 键的取值与落点；接线后 game-svr 消费新加载器（当前仍用旧三列解析） |
| `openmir2-m3-castle-gold-tax` | §3 第 3 批（需新帧/新场景/新夹具） L176 | 新：10min Save 文件 + 时钟注入 | 剩余 DoD = ① 真机两侧「同一 10min SaveTick 时间门 → TodayIncome/TotalGold 落盘」diff=0（依赖 ⑧-1 C… |
| `openmir2-m3-monster-ai-tick` | §3 第 3 批（需新帧/新场景/新夹具） L177 | 新：主动索敌观察（W0 前置） | 主动索敌（新卡，W0 前置）+ Animal RNG + 守卫静止 + 真机对拍。**2026-10-24 订正：已合入 master（最新 merge `71… |
| `openmir2-map-event-frames` | §3 第 3 批（需新帧/新场景/新夹具） L244 | 新：火墙进出 5 格场景 | 真机双栈火墙进出 5 格比对 804/805 字节与到达窗口 + PileStones 折叠槽提示 +「采 804 前确认没挖过矿/没先建过任何事件」。残余：复… |
| `openmir2-maptrigger-monster-and-door` | §3 第 3 批（需新帧/新场景/新夹具） L179 | 新：怪物走 route / 门格 | W-R1-a：pin 一张「怪物路线上的 route 格」真图，两侧同驱怪物走位 ⇒ 判据=两侧怪物坐标均停在走前格（无位移帧）；W-R1-b：pin 一张「门… |
| `openmir2-mb2-monsters` | §3 第 3 批（需新帧/新场景/新夹具） L223 | 新：world_observe 三怪 | 真机：三怪 Run 的分支与帧（数据可达时） |
| `openmir2-mb5-monsters` | §3 第 3 批（需新帧/新场景/新夹具） L221 | 新：world_observe 五类怪 | 真机：五类怪的 Run 分支与帧（数据不可达时以构造级为准）；FrostTiger 657 帧在 host drain 接通后观察 |
| `openmir2-mb7-monsters` | §3 第 3 批（需新帧/新场景/新夹具） L224 | 新：world_observe race 113/114/134 | 真机：race 113/114/134 的 Run/克隆/召唤（部署数据不可达时以构造级为准）；的展示色与排水相位 |
| `openmir2-monster-walk-frames` | §3 第 3 批（需新帧/新场景/新夹具） L226 | 新：world_observe 60–180s | 真机重采怪物走位 SM_WALK 条数/时序（C# 基线 17–76 条）；同 tick 多步、±12 边界、传送误判（T-146 四条残余） |
| `openmir2-npc-def-loading` | §3 第 3 批（需新帧/新场景/新夹具） L229 | 新：R1 npc_script_chain（国王 0122/老人 3） | 点比奇国王(0122)/老人(3) 的说词面板与菜单可点性（@main 原文逐字）；@@buildguildnow/@ask/@back/@exit 的交互序列… |
| `openmir2-pickwire-remaining-frames` | §3 第 3 批（需新帧/新场景/新夹具） L245 | 新：R1 npc_script_chain | ①三条脚本帧真机逐字节；②CHANGEEXP ⇒ Recog=加完后总量（依赖 openmir2-script-changexp-exp-writeback f… |
| `openmir2-run-gap-mb4` | §3 第 3 批（需新帧/新场景/新夹具） L208 | 新：world_observe race 92 | race 92 真机：SpaceMove/MapRandomMove 落点与帧序、狂暴 500/400 节拍；SiegeLockCount 的 CastleDo… |
| `openmir2-run-gap-pl2` | §3 第 3 批（需新帧/新场景/新夹具） L214 | 新：R1 npc_script_chain | 内容启用后的 P31 结算与 44/升级链帧序；SETAUTOGETEXP 两态在真机脚本链上的可观测行为 |
| `openmir2-run-gap-pl3` | §3 第 3 批（需新帧/新场景/新夹具） L213 | 新：R1 npc_script_chain（内容启用支） | 内容一旦启用（DECHP/INCHP/HungerSystem/KILLMONEXPRATE/POWERRATE）时的真机数值与帧序；197/198 的 Sho… |
| `openmir2-run-gap-pl4` | §3 第 3 批（需新帧/新场景/新夹具） L215 | 新：R1 npc_script_chain | 653 同 tick 相位；会话 HomeMap（GetStartPoint）四条分叉（共 3 项） |
| `openmir2-sendrefmsg-range-config` | §3 第 3 批（需新帧/新场景/新夹具） L243 | 新：部署改 15 后三场景 | 部署改 15 双栈三场景对拍。残余：crates/world 两处仍用常量 12（需 World 配置通道，另开卡）；oracle 缺省确认为 12 非 15 |
| `openmir2-setmapmode-dechp` | §3 第 3 批（需新帧/新场景/新夹具） L236 | 新：脚本夹具（SETMPAMODE DECHP） | ① 真机 SETMPAMODE DECHP 全量对拍：`now-DecHpTick > nDECHPTIME*1000` 秒门 + HP-=…；② SM_SYS… |
| `openmir2-setoffline-field-writeback` | §3 第 3 批（需新帧/新场景/新夹具） L235 | 新：脚本夹具（@OnLogout_OffLinePlaying） | ① SETOFFLINE → P31：构造 QFunction-0.txt 的 @OnLogout_OffLinePlaying（`OffL…；② ISONMA… |

### 4.4 文档仅作依赖引用（1 张）

| 卡 id | 文档归属 | 说明 | 建议 |
|---|---|---|---|
| `openmir2-script-changexp-exp-writeback` | §3 `pickwire-remaining-frames` 行 note（L245） | 只写「② 依赖 changexp-exp-writeback」 | 补进 R1 `npc_script_chain` 批次表，或登记为不可对拍 |

### 4.5 文档完全未登记（25 张）

复现：`for id in <下列 id>; do grep -c "$id" docs/window-batch-plan-20261023.md; done` ⇒ 全 0。
按性质分组（判定依据 = 卡自身 `window_required` 理由 + 卡名族）：

**GM 命令族（需 GM 命令场景；`gm_batch` 只覆盖 5 条） — 9 张**

| 卡 id | goal_gate 分类 | 窗口项理由（截断） |
|---|---|---|
| `openmir2-gm-batch-5-guild-castle` | window_required_unmerged | 有 window_required 登记但**尚未合并**（窗口在合并前跑不了，单列计数；登记本身有效，仍算 PASS） |
| `openmir2-gm-impl-clearbag` | window_required | 目标背包清空（持久化面）+ 玩家可见 709 帧；R 级真机窗口未跑。本轮只核我自己上轮 review daa2368b 的 blocking×2/important×2/minor。 |
| `openmir2-gm-impl-kick` | window_required | GM 踢人命令体 + 宿主 528/离场/踢前存档；R 级真机窗口未跑。本轮只核复审 61b952f9 的 2 important + minor/info。 |
| `openmir2-gm-impl-mobfireburn` | window_required | GM MobFireBurn 命令体 + 玩家可见帮助/not-found 帧；R 级真机窗口未跑。本轮只核复审 97ee5978 的 important/minor/info。 |
| `openmir2-gm-impl-showeffect` | window_required | oracle 命令体 20-70 行整段注释 ⇒ 任何参数零帧零副作用；本卡只落最小 no-op 并证明 Rust 不多发帧。窗口未跑（R 卡保持 in-progress）。 |
| `openmir2-gm-impl-shutup-release` | window_required | 命令体实现 + R 窗口项（真机双栈帧字节/DB 后态）；按 R 判。窗口未跑前卡保持 in-progress。 |
| `openmir2-gm-impl-smake` | window_required | 装备属性写/Recalc/落库 + 帧序（203 先于 100）；R 级真机窗口未跑。本轮只核复审 41a99a47 的 blocking/important/minor。 |
| `openmir2-gm-impl-startquest` | window_required | 命令体逐行等价不变（空名帮助 / 非空零帧 / perm 门）；本轮只核返工的 1 blocking + 1 important + 3 minor，tier=R 的真机窗口仍未跑。 |
| `openmir2-gm-script-exec-seams` | window_required | 最终增量：只改 §8/T-211 的窗口措辞与两处卡引用，代码零改动（lib.rs sha 仍 6c156b61…）。R 面核心（权限唯一抬权、pwd_busy、QFunction 生产入口、五路用例… |

**脚本链族（可并入 R1 `npc_script_chain` 扩展） — 9 张**

| 卡 id | goal_gate 分类 | 窗口项理由（截断） |
|---|---|---|
| `openmir2-merchant-script-load` | window_required | 本片把两个「只登记、不执行」分支改为真实执行 GotoLable，改变客户端可见帧（SM_MERCHANTSAY(643) 说词帧集合与次序）与商人交互行为；说词帧集合/Recog/尾字节/跨组帧序只… |
| `openmir2-qfunction-hidden-merchant-world-wiring` | window_required | 触及 RNG 抽数位置与时序：QFunction 进世界 MerchantList 首位并参与 Merchant.Run（Random(50) 流水），RunTick 取构造段账本第 1 抽 Rand… |
| `openmir2-script-action-var-text` | window_required | 改变脚本动作/条件参数发给玩家的文本内容（SENDMSG/LINEMSG/MESSAGEBOX/条件参数），属玩家可见行为面；题目定的 tier=R 亦一致。真机窗口未跑 ⇒ window_requi… |
| `openmir2-script-broadcast-tx-gating` | window_required | 结论是玩家可见全服广播的**收件人集合**在「触发者自己网关线缺席」时是否仍然正确（网络状态触发 + 帧是否发出）；作者自报 R、issue 也是 R。复审只判证伪/修法/五个漏斗/用例/门禁，真机窗… |
| `openmir2-script-dyn-var-host` | window_required | VAR/CALCVAR/LOADVAR 改变脚本分支走向与变量内容；本轮改的是 CALCVAR / 0 的中止范围（求值、控制流路径，归时序类）且动到引擎侧 abort_goto ⇒ 按 R 判。真机… |
| `openmir2-script-envir-root-host` | window_required | CHECKNAMELIST/CHECKNAMEDATELIST 的判定改变脚本走向；ADDUSERDATE/ADDNAMELIST 等文件列表动作涉及文件落盘（持久化）⇒ R 级。构造级已覆盖，真机窗… |
| `openmir2-script-sendmsg-host` | window_required | 玩家可见的全服/附近广播 + 收件人集合 + 同 ident 多帧时序（作者自报 R、issue 也是 R）；复审只判返工 I-1..I-4 与门禁，窗口仍未跑 ⇒ 合并后保持 in-progress… |
| `openmir2-script-user-account` | window_required | 脚本行为面：CHECKACCOUNTLIST/DELACCOUNTLIST 的账号条件会改变脚本走向、发奖与名单文件落盘；本返工只补生产构造点的账号装入与进程内用例，真机窗口仍未跑 ⇒ tier=R。 |
| `openmir2-sendmsg-login-qmanage-cases` | window_required | 补的是玩家可见广播（SM_SYSMESSAGE）在 @Login/QManage 排水点的收件人集合与时序；作者自报 R、issue 也是 R。复审只判枚举/接线/用例/台账/门禁，真机窗口未跑。 |

**战斗数值 / RNG 族（需 combat / RNG 场景或构造段窗口） — 5 张**

| 卡 id | goal_gate 分类 | 窗口项理由（截断） |
|---|---|---|
| `openmir2-ability-weaponstrong` | window_required | 改战斗数值（磨损量 `(ushort)(Random(5)+2-AddAbil.WeaponStrong)`）并间接影响 642 帧与存档耐久；不新增抽数，但须证明取数站点逐字节不变。 |
| `openmir2-conformance-rng-nondeterminism` | window_required | 有 window_required=true 登记 |
| `openmir2-m3-drop-exp-wiring` | window_required | 本片触及①时序（同一 MsgQueue 的出队序 53→44→45/52/752 与同 tick 53 去重）②持久化落库（characters.BodyLuck）③战斗数值（AddBodyLuck … |
| `openmir2-npc-ctor-draws` | window_required | RNG 站点账本 + 构造段窗口判据 |
| `openmir2-weaponstrong-dura-zero-skip` | window_required | 改的是 `AddAbil.WeaponStrong` 的装备口径（`Dura==0` 整槽跳过），它直接决定攻击磨损量 `(ushort)(Random(5)+2-WeaponStrong)` 与 6… |

**客户端 E2E（可能属「不可协议对拍」，需 owner 裁定） — 1 张**

| 卡 id | goal_gate 分类 | 窗口项理由（截断） |
|---|---|---|
| `openmir2-client-e2e-enter-gaps` | window_required | 卡级 R（真机时序/帧序/数值）。本片零代码（19 文件全在卡证据目录），审查法 = 钉在 head 36622639f 对入库原始件独立重跑全部机判 + 哈希子集校验 + boundary 实测，不… |

**工具 / 门卡（非协议帧面，建议登记为非窗口欠账） — 1 张**

| 卡 id | goal_gate 分类 | 窗口项理由（截断） |
|---|---|---|
| `openmir2-window-gate-two-runs` | window_required_unmerged | 有 window_required 登记但**尚未合并**（窗口在合并前跑不了，单列计数；登记本身有效，仍算 PASS） |

处置建议：

- **GM 族**：补一个 GM 命令批次场景（`gm_batch` 只覆盖 5 条；这些卡要 `SETPERMISSION=10` / 多会话 / DB 后态），
  或按卡逐条登记为「需新场景」；不要改 `gm_batch` 去硬塞。
- **脚本族**：多数可并入 R1 `npc_script_chain` 的支路（SENDMSG/@Login/QManage 排水点）；
  需要新文件落盘或账号名单的（`script-user-account`、`script-envir-root-host`）单列。
- **战斗数值 / RNG 族**：`ability-weaponstrong` / `weaponstrong-dura-zero-skip` 可挂 `combat_hit`（当前 blocked）
  或 642/耐久场景；`m3-drop-exp-wiring` 需 100 杀 RNG 场景；`npc-ctor-draws` 需构造段窗口；
  `conformance-rng-nondeterminism` 需 RNG 场景组。
- **客户端 E2E**：`client-e2e-enter-gaps` 按 §4 口径大概率应登记为「不可协议对拍」（真机 E2E），
  需 owner 裁定；本卡不代判。
- **工具 / 门卡**：`window-gate-two-runs` 不是协议帧面，建议登记为非窗口欠账（与 §5.4 同类）。

## 5. 已 `closed` 但仍带 `window_required` 标记（4 张，不计入窗口债）

| 卡 id | category | merged | 说明 |
|---|---|---|---|
| `openmir2-encoding-callsites` | `closed` | true | 账本已收口；若仍需窗口，应由 owner 重开卡 |
| `openmir2-gm-integration` | `closed` | false | 账本已收口；若仍需窗口，应由 owner 重开卡 |
| `openmir2-m3hp-lane-repin` | `closed` | false | 账本已收口；若仍需窗口，应由 owner 重开卡 |
| `openmir2-multi-session-probe` | `closed` | true | 账本已收口；若仍需窗口，应由 owner 重开卡 |

> 其中 `openmir2-multi-session-probe` 在 §7.2 被写成「仍等 slice-2」，但账本 `category=closed`、`merged=true`；
> **文档与账本不一致**，本卡只登记不改事实源，已在卡上 comment 申请 owner 复核。

## 6. 判不准 / 残余

- 「有场景（当前可跑）」按 §7.4 的 18 个场景名单 + §7.2 的转可跑声明判定；**没有**逐张重跑 `card_window --dry-run`。
  若某卡的前置夹具（§1F 行内 note：⑦-2/⑦-3、⑨-2/⑨-3 等）仍未落地，该卡实际仍跑不了 —— 需 owner 在排窗时逐行复核。
- 未合入的 2 张（`gm-batch-5-guild-castle`、`window-gate-two-runs`）在合并前窗口跑不了；本报告按旗标计入，§4.5 另有标注。
- §4 的「不可协议对拍」口径本卡不改；其中多会话收件人集合类仍等 slice-2（见 §5 的账本不一致登记）。
- 「文档完全未登记」的 25 张，分组依据是卡名族 + `window_required` 理由；**未逐张读线程原文**，
  个别卡（尤其 `client-e2e-enter-gaps`）可能应归 §4，已在组内标「判不准」并给裁定入口。

## 7. 复算命令

```sh
# 1) 取当次 window_required 全集（139 张）
python -I tools/goal/goal_gate.py run --json --skip-tests --skip-script
# 2) 逐卡在批次计划里的归属（§/行号）
grep -n "<卡 id>" docs/window-batch-plan-20261023.md
# 3) §7.4 的 18 个可跑场景（逐行枚举，不要用 18-N 反推）
sed -n '473,500p' docs/window-batch-plan-20261023.md
```

## 8. 残留

- 未改事实源、场景 JSON、`rules.json`、`whitelist.md`、工具代码；未起栈、未占端口、未连库。
- 本报告是**覆盖率盘点**，不代替窗口执行；「有场景」不等于「已真机验证」。
