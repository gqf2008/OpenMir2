# A 类真机窗口欠账 → 协议对拍场景映射（2026-10-23 草案 v1）

- 卡：`openmir2-dualrun-window-burndown`（owner `openmir2-worker-24`，tier=L）
- 状态：**草案**。窗口未开（15000/15500/16000/17100 上跑着 `E:/tmp/conformance/csharp-root` 的 C# 栈），
  本文件先产出「A 卡 → 场景 / 证据通道」映射；run 证据待窗口。
- 输入：`docs/rewrite-remaining-inventory-20261022.md` §3.2 A 表（152 张，逐条 `card id` + 最新 status 的 `work` 摘要）
  + `docs/conformance-first-dual-run-20261023.md` + `tools/probe/scenarios/*.json`。

## 1. 证据通道（本表只判「证据从哪来」，不判语义是否等价）

| 通道 | 含义 | 判据（充分条件） |
|---|---|---|
| **P 协议对拍** | 同一串 `CM_*` 打进两栈，比 s2c 帧 + 库后态 | 证据只依赖「线上帧」或「库后态」；无 GUI 几何/渲染依赖 |
| **F 夹具对拍** | `run_*_pair.ps1` + 世界侧夹具（假人/怪/Bot） | 证据需要世界内实体与时钟控制；协议驱动器可参与但不是主判据 |
| **O 离线夹具** | 文件级/日志级（编码、BOM、日志逐行） | 证据不经过网络与库 |
| **G GUI** | 渲染、点击几何、输入法、面板布局 | 无头不可复现 |
| **L 长跑** | 24h/300 假人稳定性 | 时间维度，非单次 run |

> **置信度**：`高` = 最新 status 的 `work` 明确点出证据形态；`中` = 由卡名 + 词面可判；
> `低` = `work` 只有「余量=真机窗口 N 项」这类泛化表述，**必须回读该卡 `review.window_items` 才能定**（共 48 张此类）。

## 2. 首批（≥10 张）映射

| # | 卡 | 场景（`tools/probe/scenarios/`） | 期望证据 | 现状 |
|---:|---|---|---|---|
| 1 | `openmir2-login-654-order` | `login_only.json`（现有） | 654 相对序 + 750/811/46 位置 | 首跑已有：C# 有 654、Rust 无（差异 2） |
| 2 | `openmir2-login-frame-order-52-752` | `login_only.json` + 52 布局 (54,78) | 52 body 布局对 | 首跑 52=(54,78) 不在白名单 ⇒ 规则缺口 |
| 3 | `openmir2-login-numeric-gaps` | `login_only.json` | 登录数值帧逐字节 | 首跑 19 帧对齐，余差见规则表 |
| 4 | `openmir2-login-charstatus-657` | `login_only.json` | Rust 侧重采 657 + 邻居相对序 | 首跑：C# 657×2、Rust 无 |
| 5 | `openmir2-login-notice-clienttick` | `login_only.json` | 51/658 | 含 `SoftVersionDateEx==0` 两侧构造，需另一轮 |
| 6 | `openmir2-npc-def-loading` | `npc_click.json`（**已建，加载期 fail-closed 已验**） | 固定 NPC 出现帧 + `SM_MERCHANTSAY(643)` | 需 `--var npc_actor=<id>` |
| 7 | `openmir2-npcs-fixed-loading` | `npc_click.json`（`--var npc_actor=`） | 两侧 NPC 出现帧一致 | 同上 |
| 8 | `openmir2-market-def-impl` | `merchant_dialog.json`（**已建**，`--var merchant_actor`） | 说词/变量两条 `GetLineVariableText` 展开 | 需商人 `recog` |
| 9 | `openmir2-script-take-persist` | `bag_equip.json` / `pickup_item.json` / `eat_item.json`（**已建**）+ 库快照 | take 后 `characters_bagitem` 槽位 | r10 已 GREEN，可用对拍固化 |
| 10 | `openmir2-click-path-bag-writeback` | `npc_click.json` + 库快照 | 点击 → 背包/佩戴/`characters.Gold` | T-160 的 R 窗口项 |
| 11 | `openmir2-persist-useitems-wiring` | `bag_take.json` + 库快照 | 耐久磨损/破零 roundtrip | 需先有可用物品夹具 |
| 12 | `openmir2-gm-commands-design`（O 类，非 A） | `gm_time.json`（现有） | 6 条可达 GM 命令的帧面 | Permission 恒 0 ⇒ 需 GM 账号 |

> 场景文件 6/8/9 为**新增草稿**，本批窗口内先跑 `login_only` 与 `gm_time` 的复现（无需改场景），
> 再按实际 NPC/商人 `recog` 落 `--var` 跑 6/8/9。

## 3. 全量 152 张分类（机械分类 v1）

统计（v1.1，回读 window_items 后重算）：协议帧面 **95** / 夹具对拍(bot pair) **35** / 落库后态 **13** / 离线夹具(文件级) **6** / GUI 面板 **1** / 离线夹具(日志) **1** / 长跑(300 假人 24h) **1**。

| # | card id | 通道 | 置信度 | 依据（`status.work` 摘要；`win:` = `review.window_items` 原文） |
|---:|---|---|---|---|
| 1 | `openmir2-action-line-variable-text` | 协议帧面 | 中 | R 级窗口未跑：① C# SENDMSG 0 <$USERNAME>:<$STR(S1)> 151 235（QFunction-0.txt:… |
| 2 | `openmir2-script-take-persist` | 落库后态 | 中 | r10 真机窗口项 1 GREEN（筹码包 take 后存档槽位清零） |
| 3 | `openmir2-openybdeal-host-wiring` | GUI 面板 | 高 | 余量=真机掷骰/寄售面板 + 残余⑨-⑬ |
| 4 | `openmir2-npc-def-loading` | 协议帧面 | 高 | win: 点比奇国王(0122)/老人(3) 的说词面板与菜单可点性（@main 原文逐字） \|\| @@buildguildnow/@as… |
| 5 | `openmir2-weapon-upgrade-values` | 协议帧面 | 高 | win: 真机 @upgradenow 全量对拍四字段与 657 帧 \|\| 取回路径 Dura 档/n90 抽数逐抽 \|\| 202/2… |
| 6 | `openmir2-npc-hit-frames` | 协议帧面 | 高 | win: C# 本尊正面样本 ≥180s（同位置必须出现 ident=14 recog=国王 actor） \|\| 帧形逐字节（Recog/… |
| 7 | `openmir2-monster-ref-drains` | 协议帧面 | 高 | win: 真机四类帧逐字节与收件人集合（需先让 race 203/206/210/215 出现在部署数据） \|\| Khazard 玩家目标… |
| 8 | `openmir2-guard-ctor-constants` | 协议帧面 | 高 | win: W-guard-1 卫士自扫半径 5→7 的同 tick 可见集/事件序 \|\| W-guard-2 bins monster_d… |
| 9 | `openmir2-a22-castle-persistence` | 离线夹具(文件级) | 高 | win: W-A22-1 文件字节：C#/Rust 各跑启动+一次 10min Save，比 List.txt / Castle/SabukW… |
| 10 | `openmir2-npc-ctor-constants` | 协议帧面 | 高 | win: W1 真机固定 NPC/商人首次出现帧 SM_TURN series 低=4 高=2，与 oracle 逐字节 \|\| W2 首 … |
| 11 | `openmir2-t138-request-castle-war` | 协议帧面 | 高 | win: W-T138-1 非掌门/无行会 ⇒ SM_SYSMESSAGE(100)「你的请求被取消!!!」逐字节（Recog/Param 颜… |
| 12 | `openmir2-npc-display-table` | 协议帧面 | 高 | win: 同摆位 0122 (29,33) C# vs Rust 登录抓包逐字节对拍：出现帧带名尾、3 对 nameless 转身 serie… |
| 13 | `openmir2-single-binary-roles` | 离线夹具(日志) | 中 | 余量=RNG trace site groups 复验 + 旧 exe 与 mir2-server 日志逐行对比（窗口项） |
| 14 | `openmir2-login-charstatus-657` | 协议帧面 | 高 | 按 R 级不 closed：余量=Rust 侧重采 657 + 邻居相对序 |
| 15 | `openmir2-a09-upgrade-item` | 协议帧面 | 高 | win: 部署 ItemNameList 5 条在真机取同名物品，逐字节对拍 SM_BAGITEMS/SM_SAVEITEMLIST/买卖/取… |
| 16 | `openmir2-merchant-race-bias` | 协议帧面 | 中 | 余量=真机攻击商人/目标集/GetFeature/Push 时序四项窗口 |
| 17 | `openmir2-superman-normnpc` | 协议帧面 | 高 | win: 城门 110 连续受击到 HP=0：Death=false、Run 回满、无 SetMapXyFlag(2) \|\| World:… |
| 18 | `openmir2-mb8-9-monsters` | 协议帧面 | 中 | 余量=真机 race215 帧（需先接 T-148）+ NormNpc SuperMan（另卡 openmir2-superman-norm… |
| 19 | `openmir2-monster-walk-frames` | 协议帧面 | 高 | win: 真机重采怪物走位 SM_WALK 条数/时序（C# 基线 17–76 条） \|\| 同 tick 多步、±12 边界、传送误判（T… |
| 20 | `openmir2-login-654-order` | 协议帧面 | 中 | 按 R 级不 closed：余量=真机三项（改动后 Rust 登录抓包/有会角色 811,46 与 750 位置/非 TestServer … |
| 21 | `openmir2-db-encoding` | 离线夹具(文件级) | 高 | 按 R 级不 closed：余量=E-7 真机 oracle 对照 + 4 条 Low 台账 |
| 22 | `openmir2-market-def-impl` | 协议帧面 | 高 | 按 R 级不 closed：余量=真机说词/变量 + T-145 未接分支 |
| 23 | `openmir2-account-protection-13` | 落库后态 | 高 | 按 R 级不 closed：余量=真机 roundtrip 13/12 列逐列 |
| 24 | `openmir2-strlist-bom` | 离线夹具(文件级) | 高 | 余量=真机 BOM 文件读取 |
| 25 | `openmir2-mb7-monsters` | 协议帧面 | 中 | 按 R 级不 closed：余量=真机观察 + T-143 |
| 26 | `openmir2-encoding-crate` | 离线夹具(文件级) | 高 | 余量=真机逐字节 + data StringList 读侧 BOM 缺口（另开卡） |
| 27 | `openmir2-mb2-monsters` | 协议帧面 | 高 | win: 真机：三怪 Run 的分支与帧（数据可达时） |
| 28 | `openmir2-hutil32-parity` | 离线夹具(文件级) | 高 | 按 R 级不 closed：余量=round 调用点真机对拍 + 10 个 GB2312 族成员（零依赖契约）+ 两份 Round 分叉（建… |
| 29 | `openmir2-guild-official-npc` | 协议帧面 | 中 | 按 R 级不 closed：余量=真机窗口三项 + T-137（固定 NPC 脚本装载，最大缺口） |
| 30 | `openmir2-mb5-monsters` | 协议帧面 | 中 | 按 R 级不 closed：余量=真机观察 + T-135（host drain 10139） |
| 31 | `openmir2-castle-war-area-impl` | 协议帧面 | 中 | 按 R 级不 closed：余量=真机 W-1..W-5（四臂落点 rng/766 bit4/名字色/10s Run）+ T-125..T-… |
| 32 | `openmir2-dbsrv-scheme-a` | 落库后态 | 高 | 按 R 级不 closed：余量=真机 W-DB-1..7（慢存档/全序/闸门/独立失败）+ T-120 |
| 33 | `openmir2-encoding-core-e1-e2` | 离线夹具(文件级) | 高 | 余量=真机窗口（非 ASCII 文本逐字节 + 两条 BOM 路径） |
| 34 | `openmir2-chat-channels` | 协议帧面 | 高 | 按 R 级不 closed：余量=真机窗口三项 + T-118（NRE 已 1:1，意图投递待 owner 批准后另开 bug-fix 批次… |
| 35 | `openmir2-guild-g3-persistence` | 落库后态 | 中 | 按 R 级不 closed：余量=真机窗口五项（104 扇出/SS_208/四段字节/相位/实际收件人集）+ T-97..T-117 登记 |
| 36 | `openmir2-castle-guard-target` | 协议帧面 | 中 | 余量=真机窗口：同城/盟友/UnderWar 目标选择 + T-96⑥④（AnimalObject.Struck 派发边界） |
| 37 | `openmir2-castle-entities` | 协议帧面 | 中 | 按 R 级不 closed：余量=真机窗口五项（部署 0 实体/4 告警、夹具 16 实体 96 抽、毒攻门、同城/盟友/UnderWar … |
| 38 | `openmir2-upgrade-npc` | 协议帧面 | 中 | 按 R 级不 closed：余量=真机窗口五项（抽数序/Desc[10] 字节/帧序/654·153/破零链），另 T-95 的九条登记随卡 |
| 39 | `openmir2-guild-g2-dispatch` | 协议帧面 | 高 | win: 750 的真机出队序（46 之后、654 之前） \|\| 104 行会消息的线缆 \|\| ShowGuildName=1 的 4… |
| 40 | `openmir2-run-gap-pl4` | 协议帧面 | 中 | 余量=真机窗口三项（见 review.window_items） |
| 41 | `openmir2-selgate-reconnect` | 协议帧面 | 高 | win: 真机 DBSrv 重启后 SelGate 的重连与在途会话 \|\| 断链期间真实客户端的表现 |
| 42 | `openmir2-dbsrv-fault-paths` | 落库后态 | 高 | win: 真机 MySQL 停机/重启下的四种降级帧与客户端表现 \|\| 9 表与 characters_indexes 的跨实现逐字段 d… |
| 43 | `openmir2-gate-idle-timeout` | 协议帧面 | 高 | win: 真实客户端挂起 60s 的被踢时序与畸形超时帧的实际反应 \|\| 下行断链后网关的完整恢复路径（重连未实现，另卡） |
| 44 | `openmir2-run-gap-pl2` | 协议帧面 | 高 | win: 内容启用后的 P31 结算与 44/升级链帧序 \|\| SETAUTOGETEXP 两态在真机脚本链上的可观测行为 |
| 45 | `openmir2-gategate-slots-gm` | 协议帧面 | 高 | win: C# GameSvr + Rust GameGate 的 GM_CHECKCLIENT→SendKeepAlive→GM_CHECK… |
| 46 | `openmir2-login-numeric-gaps` | 协议帧面 | 中 | 余量=真机窗口三项（见 review.window_items） |
| 47 | `openmir2-oracle-humitems-align` | 落库后态 | 高 | 余量=真机窗口：部署基线已变，下次开窗重抓装备槽字段级证据 |
| 48 | `openmir2-run-gap-pl3` | 协议帧面 | 中 | 余量=真机窗口（内容启用后的数值/帧序）+ T-85 登记的八条偏差 |
| 49 | `openmir2-delmagic-addmagic-chain` | 协议帧面 | 中 | 余量=真机窗口三项（见 review.window_items） |
| 50 | `openmir2-login-frame-order-52-752` | 协议帧面 | 中 | 余量=非 TestServer 部署/发光装备/IsSpirit 的槽位变化未取样本 |
| 51 | `openmir2-gategate-speed-rules` | 协议帧面 | 中 | 余量=真机窗口四项（登录包相位、0-210ms 内未丢帧的先后、超长帧回归、ItemSpeed/SpeedLimit 接线） |
| 52 | `openmir2-healthspell-emission-model` | 协议帧面 | 中 | 余量=真机窗口四项（见 review.window_items） |
| 53 | `openmir2-monster-attack-hit-frame` | 协议帧面 | 中 | 余量=真机窗口（同 tick/200ms 顺序三项） |
| 54 | `openmir2-run-gap-pl1` | 协议帧面 | 中 | 余量=真机窗口四项（见 review.window_items） |
| 55 | `openmir2-monster-attack-cadence-lead` | 协议帧面 | 中 | 余量=按评审口径的 HP 受控重采样（真机窗口），并入窗口项 |
| 56 | `openmir2-run-gap-mb4` | 协议帧面 | 中 | 余量=真机窗口：race 92 的 SpaceMove/MapRandomMove 落点与狂暴 500/400 节拍 |
| 57 | `openmir2-magic-8-21-wiring` | 协议帧面 | 中 | 余量：① 真机窗口（37 的 Random(20)/Random(2) 流水 + 逐格 SM_BACKSTEP，可与 8 同场景） |
| 58 | `openmir2-player-dir-supply` | 协议帧面 | 低 | 余量：真机窗口 + T-81 登记 |
| 59 | `openmir2-player-drop-useitems` | 协议帧面 | 低 | 余量：真机窗口 + T-81 登记 |
| 60 | `openmir2-rm-delaypushed` | 协议帧面 | 低 | 余量：真机窗口 + T-81 登记 |
| 61 | `openmir2-rm-delaymagic-six-ids` | 协议帧面 | 低 | 余量：真机窗口 + T-81 登记 |
| 62 | `openmir2-save-misc-gaps` | 落库后态 | 低 | 余量：真机窗口 |
| 63 | `openmir2-db-error-reply` | 落库后态 | 低 | 余量：真机窗口（失败注入的 C# 对拍） |
| 64 | `openmir2-persist-single-sided-columns` | 落库后态 | 低 | 余量：真机窗口 |
| 65 | `openmir2-attacktarget-overrides` | 协议帧面 | 中 | 余量：真机窗口（82/119/130/91/107/105） |
| 66 | `openmir2-smframe-minor-gaps` | 协议帧面 | 低 | 余量：真机窗口 + T-79/T-80 登记项 |
| 67 | `openmir2-sm-merchantsay-recog` | 协议帧面 | 低 | 余量：真机窗口 + T-79/T-80 登记项 |
| 68 | `openmir2-login-useitems-magic` | 协议帧面 | 低 | 余量：真机窗口 + T-79/T-80 登记项 |
| 69 | `openmir2-sm-updateitem` | 协议帧面 | 低 | 余量：真机窗口 + T-79/T-80 登记项 |
| 70 | `openmir2-sm-additem-tail` | 协议帧面 | 低 | 余量：真机窗口 + T-79/T-80 登记项 |
| 71 | `openmir2-search-target-clear-target` | 协议帧面 | 低 | 余量：① 真机窗口 |
| 72 | `openmir2-proper-target-unfolded` | 协议帧面 | 低 | 余量：① 真机窗口（攻击已死目标的抽数流水） |
| 73 | `openmir2-active-seek-race-missing` | 协议帧面 | 低 | 余量：① 真机窗口 |
| 74 | `openmir2-rm-refmessage-delay-layer` | 协议帧面 | 中 | 余量：① **真机窗口**（200ms 后二次 `Struck` 的全局帧序） |
| 75 | `openmir2-save-on-shutdown` | 落库后态 | 中 | 余量：① **真机窗口**（`DB_SAVEHUMANRCD` 帧与落库 diff、`-Stop` graceful 日志 |
| 76 | `openmir2-run-gap-mb3` | 协议帧面 | 中 | 余量：① **真机窗口**（107/105/102/112） |
| 77 | `openmir2-persist-item-attr-and-gold` | 落库后态 | 中 | 余量：① G-01 功能上仍不落库（oracle 门决定，`B-117`） |
| 78 | `openmir2-persist-useitems-wiring` | 落库后态 | 中 | 余量：① **真机窗口**（耐久磨损/破零/存档往返） |
| 79 | `openmir2-persist-create-rows` | 落库后态 | 低 | 余量：① 真机窗口（建角/兜底建角往返） |
| 80 | `openmir2-monster-view-throttle` | 协议帧面 | 低 | 余量：① **真机窗口** |
| 81 | `openmir2-run-gap-mb1` | 协议帧面 | 中 | 余量：① **真机窗口**（race 101/95 各 ≥1、R38 抽样） |
| 82 | `openmir2-run-gap-checkdupobj` | 协议帧面 | 中 | 余量：① **真机窗口**——3 人同格推挤的源/目标格身份序列 + AOI 相位（1000ms 口径） |
| 83 | `openmir2-run-gap-monster-state-expiry` | 协议帧面 | 中 | 余量：① **真机窗口**（CrazyMode 色变 0xF9 / RefNameColor 帧面 / FireHit-TwinHit 到期… |
| 84 | `openmir2-run-gap-corpse-ghost-revive` | 协议帧面 | 中 | 余量：① **真机窗口**（3 分钟变尸 / 可复活支 ZenTime-20s / 5 分钟移除 / 两帧 RM_TURN） |
| 85 | `openmir2-run-gap-pr1-head-blocking` | 协议帧面 | 低 | 余量：① 真机窗口 |
| 86 | `openmir2-disappear-recipients-gridscan` | 协议帧面 | 中 | 余量：① **真机窗口**（节流窗口内进入 ±12 的观察者、离世广播与 AOI/移动帧的全局次序） |
| 87 | `openmir2-run-gap-monster-specials` | 协议帧面 | 中 | 余量：① **真机窗口**（同 tick 产仔抽数相对次序、`SM_HIT` 帧面、Clone 扣 MP） |
| 88 | `openmir2-run-gap-monster-master-follow` | 协议帧面 | 中 | 余量：① **真机窗口**（异图/超距/SlaveRelax 两侧流水） |
| 89 | `openmir2-movetomovingobject-quirk` | 协议帧面 | 中 | 余量：① **真机窗口**（多对象格移动时的源/目标格身份序列） |
| 90 | `openmir2-player-searchviewrange-cadence` | 协议帧面 | 中 | 余量：① **真机窗口**——首扫约 1.2s、之后每 1.2s |
| 91 | `openmir2-aoi-verify-maptime` | 协议帧面 | 中 | 余量：① **真机窗口**——同缩窗场景需验 vis>=1、nearest<=vr、capture 出 SM_TURN(281,661) +… |
| 92 | `openmir2-spell-shared-tail` | 协议帧面 | 低 | 余量：① 真机窗口（注入时钟下 656 帧序） |
| 93 | `openmir2-login-notice-clienttick` | 协议帧面 | 中 | 余量：① 真机窗口（含 `SoftVersionDateEx==0` 两侧构造） |
| 94 | `openmir2-spell-mp-wrapper` | 协议帧面 | 中 | 余量：① 真机窗口（失败回帧 + MP/53/尾时序） |
| 95 | `openmir2-enter-map-cell-rejected-vs-oracle` | 协议帧面 | 低 | 余量：① 真机窗口（切图失败路径 |
| 96 | `openmir2-enter-shift-loop` | 协议帧面 | 高 | 余量：① 真机窗口（墙格出生登录：RNG 流水 + 落点 |
| 97 | `openmir2-map-random-move-shift-loop` | 协议帧面 | 中 | 余量：① 真机窗口（商人到点移动：RNG 流水 + 落点） |
| 98 | `openmir2-recall-slave-shift-live` | 协议帧面 | 高 | 余量：① 真机窗口（W0 §2e：两侧 RNG 账本 `VERDICT IDENTICAL` + 回召落点同坐标） |
| 99 | `openmir2-mapcell-attributes-loading` | 协议帧面 | 中 | 余量：① 真机窗口（**必核**：route added=1649、刷怪位置与 RNG 流水、进图/落格/AOI/CanWalk(true)… |
| 100 | `openmir2-mapcell-consumer-teeth` | 协议帧面 | 中 | 余量：① 真机窗口核「走位/推/冲刺帧面 + NPC/商人出现帧」（W0 runbook §W0-6 已列 |
| 101 | `openmir2-mongen-terrain-loop-live` | 协议帧面 | 高 | 余量：真机窗口核「两侧出生坐标逐点一致 + RNG 流水」（W0 runbook §W0-6/§W0-8 已列，依赖 mapcell 装载 |
| 102 | `openmir2-ctor-barrier-a-draw` | 协议帧面 | 中 | 余量：① 真机窗口（**默认流水已变，须重取基线** |
| 103 | `openmir2-ctor-draws-fold` | 协议帧面 | 中 | 余量：① 屏障 A 的 1 抽（ServerConf.cs:114 ItemNumber += Random(10000)）——补上后段②/… |
| 104 | `openmir2-snap-cell-fidelity` | 协议帧面 | 低 | 余量：① 真机窗口项 |
| 105 | `openmir2-default-arm-spell-surface` | 协议帧面 | 低 | 余量：① 真机逐帧对拍（窗口项） |
| 106 | `openmir2-buff-producer-wiring` | 协议帧面 | 中 | 余量=真机窗口项 + 新开片：四块 snap 改用忠实镜像 `cetin_near_xy`（复验新发现的格子层差异）+ 补窗口宽度上界牙 |
| 107 | `openmir2-buff-system` | 协议帧面 | 低 | 余量=真机窗口项 + A-5 生产接线批 |
| 108 | `openmir2-world-tick-expansion` | 协议帧面 | 低 | 余量=真机窗口项（RNG 流水/时钟/公告节拍） |
| 109 | `openmir2-transmap-maptrigger` | 协议帧面 | 低 | 余量=真机窗口项 |
| 110 | `openmir2-maptrigger-monster-and-door` | 协议帧面 | 低 | 余量=真机窗口项 W-R1-a/b/c |
| 111 | `openmir2-orphan-encoder-wiring` | 协议帧面 | 低 | 余量=真机窗口项 |
| 112 | `openmir2-window-collector-preflight` | 协议帧面 | 中 | 余量=真机窗口项：两侧种子交叉断言 / C# site 列 / 前置门生产路径不误拦 |
| 113 | `openmir2-outbound-encoders-red-batch` | 协议帧面 | 中 | 按 tier=R **不 closed**——待窗口项：制药抽数流水 + 帧序 + MYSTATUS 派生落库 |
| 114 | `openmir2-m3-monster-active-seek` | 夹具对拍(bot pair) | 中 | 余量：真机 W0/W9 对拍 + 登记未建模面（覆写 Run 的子类、SuperGuard/ArcherGuard 自有 pvp 扫描） |
| 115 | `openmir2-m3-drop-exp-wiring` | 夹具对拍(bot pair) | 中 | 余量：真机 100 杀两相窗口（依赖 T-59 rng-scene 已落）+ quest 真机窗口 |
| 116 | `openmir2-m3-rng-scene-driver` | 夹具对拍(bot pair) | 中 | 余量：真机两侧账本对拍（W9 窗）+ BotSrv real wiring（进 0137/pin 怪/dump，属 W9 窗项） |
| 117 | `openmir2-npcs-fixed-loading` | 协议帧面 | 中 | 剩余 DoD = 真机两侧 NPC 出现帧一致（并入联调窗，与 ③a/GuardList 同窗） |
| 118 | `openmir2-m3-quest-align` | 夹具对拍(bot pair) | 中 | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑥（master 未动）：仍 BLOCKED，三重阻断与 r2 相同：①… |
| 119 | `openmir2-guardlist-guards-loading` | 协议帧面 | 高 | 剩余 DoD = 真机两侧项：③a 反向点出现帧一致（C# 侧现 1 对、修复后应两侧同）+ MIR2_RNG_SEED 双侧 site/o… |
| 120 | `openmir2-m3-drop-exp-fixture-botsrv` | 夹具对拍(bot pair) | 中 | **保持 in-progress（离线夹具片，不 closed）**：本 approve 只覆盖离线夹具/接线/登记片，卡级真机 DoD③⑤… |
| 121 | `openmir2-m3-hero-pet-align` | 夹具对拍(bot pair) | 中 | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑨（master 未动）：PREREQ（静态） |
| 122 | `openmir2-m3-fixture-chain-gate` | 夹具对拍(bot pair) | 中 | review do_not_close=true）**：剩余 DoD = 卡级真机两侧同序列 diff=0（各 READY 夹具 run_*… |
| 123 | `openmir2-m3-rng-trace-scene-groups` | 夹具对拍(bot pair) | 中 | **保持 in-progress（不 closed，review do_not_close 明示）**：本 approve 只覆盖离线证据/… |
| 124 | `openmir2-m3-pet-slave-masterdie-wire` | 夹具对拍(bot pair) | 中 | **按 tier=R 不 closed（保持 in-progress）**：剩余 DoD = ① 真机两侧 run_hero_pet_pai… |
| 125 | `openmir2-m3-pet-slave-amulet-delitem-frame` | 夹具对拍(bot pair) | 中 | **按卡级剩余 DoD 保持 in-progress（不 closed）**：剩余 = ① 真机 run_hero_pet_pair.ps1… |
| 126 | `openmir2-m3-party-trade-align` | 夹具对拍(bot pair) | 中 | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑦（master 未动）：PREREQ（静态） |
| 127 | `openmir2-m3-pet-slave-clone46` | 夹具对拍(bot pair) | 中 | **按 tier=R 不 closed（保持 in-progress）**：剩余 DoD = 真机两侧 run_hero_pet_pair.… |
| 128 | `openmir2-m3-shop-fixture-botsrv` | 夹具对拍(bot pair) | 中 | **保持 in-progress（不 closed）**：本片虽为 L 级（window_required=false），但卡级真机 DoD… |
| 129 | `openmir2-m3-selloff-fixture-botsrv` | 夹具对拍(bot pair) | 中 | 剩余 DoD②③ = 真机两侧同序列 diff=0（等 ⑩-1/⑩-2 落地 + 独占窗口） |
| 130 | `openmir2-m3-fixture-marker-prefix-bug` | 夹具对拍(bot pair) | 中 | **保持 in-progress（夹具/离线片，不 closed）**：剩余 DoD = 卡级 ① 真机两侧同序列 diff=0（阻塞 = … |
| 131 | `openmir2-m3-drop-exp-align` | 夹具对拍(bot pair) | 中 | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核④（master 未动）：PREREQ（静态） |
| 132 | `openmir2-m3-pet-slave-lifecycle` | 夹具对拍(bot pair) | 中 | **按 tier=R 不 closed（保持 in-progress）**：剩余 DoD = 真机两侧 run_hero_pet_pair.… |
| 133 | `openmir2-m3-hero-pet-fixture-botsrv` | 夹具对拍(bot pair) | 中 | **不 closed（保持 in-progress）**：剩余 DoD②③ = 真机两侧 run_hero_pet_pair.ps1 同序列… |
| 134 | `openmir2-m3-guild-castle-fixture-botsrv` | 夹具对拍(bot pair) | 中 | **夹具卡 ⇒ 保持 in-progress（不 closed）**：剩余 DoD②③ = 真机两侧 run_guild_castle_pa… |
| 135 | `openmir2-m3-selloff-goldsales-load-wire` | 夹具对拍(bot pair) | 中 | **不 closed（DoD 未全达，保守）**：剩余 = 真机两侧同序列对拍 run_selloff_pair.ps1 + selloff… |
| 136 | `openmir2-m3-shop-dealing-gate` | 夹具对拍(bot pair) | 中 | **不 closed（review OA-1 明示：本 approve 只覆盖 Dealing 前置门一片，卡级 DoD 真机两相未闭环）*… |
| 137 | `openmir2-m3-shop-dialog-wiring` | 夹具对拍(bot pair) | 中 | **按 tier=R 不 closed（保持 in-progress）**：剩余 DoD = 真机两侧 run_npc_shop_pair.… |
| 138 | `openmir2-m3-quest-fixture-botsrv` | 夹具对拍(bot pair) | 中 | 剩余 DoD②③ = 真机两侧同序列 diff=0（-Side rust preflight 仍 PREREQ，等 ⑥-3 落地 + 独占窗… |
| 139 | `openmir2-m3-quest-persist-observability` | 夹具对拍(bot pair) | 中 | run_pair.ps1 + scoped_diff.py --canonical）需独占窗口，按 not_verified 登记 |
| 140 | `openmir2-m3-mail-auction-align` | 夹具对拍(bot pair) | 中 | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑩（master 未动）：PREREQ（静态） |
| 141 | `openmir2-m3-party-deal-capacity-int` | 夹具对拍(bot pair) | 中 | **按 tier=L 但卡级真机 DoD 未达 ⇒ 不 closed（保持 in-progress，review note 明示「不因本片收… |
| 142 | `openmir2-m3-clock-injection` | 夹具对拍(bot pair) | 中 | **按 tier=R 不 closed（保持 in-progress）**：剩余 DoD = ① C# 影子 hook 半（HUtil32.… |
| 143 | `openmir2-m3-pet-slave-summon` | 夹具对拍(bot pair) | 中 | 合并检查点（role=merge, openmir2-coordinator）：⑨-2 M-3 收口片（本片 = 仓库卫生离线片：mir2-… |
| 144 | `openmir2-m3-party-gainexp-npe` | 夹具对拍(bot pair) | 中 | **按 tier=R 不 closed**：剩余 DoD = 真机两侧同序列 diff=0（归 openmir2-m3-party-trad… |
| 145 | `openmir2-m3-shop-scenario-authority` | 夹具对拍(bot pair) | 中 | **剩余 DoD = 真机**：run_npc_shop_pair.ps1 在独占窗口跑两侧同序列 diff=0（等 ⑤-4 第②片仓库存档… |
| 146 | `openmir2-m3-fixture-matrix` | 夹具对拍(bot pair) | 中 | **剩余 = 真机 DoD**：READY 夹具需 -Side rust preflight 前置落地 + 独占窗口跑两侧同序列 diff=… |
| 147 | `openmir2-m3-guild-castle-align` | 夹具对拍(bot pair) | 中 | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑧（master 未动）：PREREQ（静态） |
| 148 | `openmir2-m3-npc-shop-align` | 夹具对拍(bot pair) | 中 | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑤（master d4d5b89c 未动，与 r1/r2 相同）：仍 B… |
| 149 | `openmir2-m3-scoped-diff-ability` | 夹具对拍(bot pair) | 中 | 窗口 r3（master d4d5b89c 未动）真机加深复跑 ②③F-2（360 击）：attack_diff 仍 GREEN diff=… |
| 150 | `openmir2-m3-party-trade-fixture-botsrv` | 夹具对拍(bot pair) | 中 | 剩余 DoD②③ = 真机两侧同序列 diff=0（等 ⑦-2/⑦-3 落地 + 独占窗口） |
| 151 | `openmir2-m3-final-acceptance` | 长跑(300 假人 24h) | 中 | **剩余**：DoD② 300 假人 24h 长跑（**等独占窗口** |
| 152 | `openmir2-click-path-bag-writeback` | 协议帧面 | 中 | 余量=真机落库对拍（T-160：`characters_bagitem`/`hum_items`/`characters.Gold`）；ma… |

### 3.1 v1.1：低置信度回读结果（2026-10-23）

对 v1 里 48 张「置信度低」的卡逐张 `walgit collab thread` 回读 `review.window_items`：
**46 张线程可读，其中 19 张的 review 带 `window_items` 原文** ⇒ 已改用原文作依据、通道按原文改判、置信度升 `高`（表中 `win:` 前缀）；
其余 27 张的 review 只有 `findings`/`work`，**仍保持低置信度**，开工前需读该卡 `review.findings` 或向 Leader 要窗口清单。


### 3.2 v1.2：余 27 张回读 `review.findings` 的结果（2026-10-23）

对 §3.1 剩下的 27 张逐张回读 `review.findings`：**22 张有 findings、5 张完全没有**
（`smframe-minor-gaps` / `sm-merchantsay-recog` / `login-useitems-magic` / `sm-updateitem` / `sm-additem-tail`）。

但 findings 的内容是**评审期问题单**（`C1: world.item_catalog 生产从未填充`、`F1/F2 已在修复轮解决`…），
且多条卡复用同一段 note，**不含「证据长什么样」的信息** ⇒ 无法据此判定证据通道。
结论：这 27 张**保持低置信度**，通道需由卡 owner / Leader 补 `window_items` 或窗口清单；
本表不为它们臆造通道（对应卡上 comment 见 `openmir2-dualrun-window-burndown` 的 `evidence-gap` 条）。

## 4. 不能对拍 / 暂不可判（逐条原因）

- **G 1 张**：`openmir2-openybdeal-host-wiring` —— 掷骰/寄售面板的可见性依赖客户端面板渲染，协议侧只能验「服务端送了什么帧」，
  面板布局与点击几何必须 GUI。
- **F 35 张（`m3-*` 族）**：判据是 `run_*_pair.ps1` 两侧同序列 `diff=0`，且多数卡自带前置
  （`openmir2-m3-clock-injection`、世界侧实现片、`-Side rust preflight`）。**这些不能靠本卡的 probe 场景替代**；
  本卡能为它们做的是：把可观察面里「协议帧」那一段先对拍绿（如 `run_npc_shop_pair` 的 652/700-707 帧序），
  但 `diff=0` 的最终判据仍在它们各自的窗口。
- **L 1 张**：`openmir2-m3-final-acceptance`（300 假人 24h），非单次 run。
- **低置信度 48 张**：`work` 只写「余量=真机窗口 N 项」，**本表不替它们下结论**；开工前必须回读该卡
  `review.window_items` / `needs-human` 条目再定通道。

## 5. 产物归档与索引约定

- run 产物：`E:/tmp/conformance/runs/<scenario>-<side>-<yyyymmdd-HHMM>/`（`frames.ndjson` + `run.json` + `db_snapshot.json`）
- 比较报告：同目录 `report.json`（`compare_runs.py` 输出）
- 归档进仓库：每批收口时把 `report.json` 摘要 + 差异结论写入本目录下的批次报告，原始产物留在 `E:/tmp` 并在报告里索引路径。

## 6. 遗留

1. 场景：`npc_click.json` / `merchant_dialog.json` / `bag_equip.json` / `eat_item.json` / `pickup_item.json`
   已全部落到 `tools/probe/scenarios/`，**消息号与参数位置均有 oracle 出处**（1010/1011 / 1003 / 1006 / 1001，见各文件 `note`）；
   运行时数据（NPC/商人 `ActorId`、`MakeIndex`、自身坐标）一律 `params: "REQUIRED"`，缺 `--var` 即加载期 fail-closed，绝不猜；

2. 规则表缺口（`100/520/525/529/530/654/657` + 52 的 (54,78)）属 `conformance-rules` 车道，本卡只引用不修改；
3. ~~48 张低置信度卡的通道判定待逐卡回读~~ ⇒ 已做（§3.1/§3.2）：19 张按 `window_items` 定；余 27 张的 findings 为评审问题单、不含证据形态 ⇒ 仍低置信度，待 owner/Leader 补窗口清单。
