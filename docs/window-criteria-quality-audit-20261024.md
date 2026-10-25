# 窗口判据可执行性审计：`window_required` 桶（tier=L，纯审计）

> 卡：`openmir2-window-criteria-quality-audit`（principal `openmir2-worker-69`，worktree `E:/tmp/wt-worker-69`，分支 `card/window-criteria-audit`）。
> 性质：**审计，不是收口**。本文件只写 `docs/`；未改任何卡片状态、whitelist、工具或代码。
> **返工 v2（2026-10-24，review `626e0c60` request_changes 后）**：① 撤销原 §6-F6 第 4 条（`weapon-durability-frames`「窗口判据自相矛盾」）——whitelist **T-199** 已明写 654/709 本窗口不比对，原判断只读了 review 未读 T 行；② `openmir2-monster-showhp-53-broadcast` 由 ① 改判 ②（无具名命令/夹具）；分布 **11/18/1 → 10/19/1**（改前/改后对照见 §4.1）。其余发现已逐条回 whitelist T 行复核（§6.1）。

## 1. 快照与口径

| 项 | 值 |
| --- | --- |
| 仓库 | `E:/tmp/wt-worker-69`（基于 `walgit/master`） |
| 审计基线 HEAD | `2a510160613205538a74ad65988af5c077158cef`（截至 2026-10-24 01:06 本地时间；交付 head 以 patch 条目为准） |
| goal_gate 快照 | `generated_ts = 1792775018`（2026-10-24 01:03:38 本地时间） |
| 命令 | `python -I tools/goal/goal_gate.py --json > <tmp>/gg.json` |
| 模式 | 快速模式（`tests=false, script_coverage=false`；条件②的卡枚举不依赖这两项） |
| 条件② detail | `R 卡 193 张：已完成 11 / window_required 121 / whitelist 44 / 已裁定登记 3 / comment 证据 0 / 三者皆无 0 / merged 未 closed 且无登记(需人工) 9 / 未开工或未合并 0（不计缺口）/ 其它需人工 5` |
| 本审计总体 | `overall=need_human`（条件①未跑 parse-stats、条件④未跑 cargo test；与本审计无关） |

**桶的精确定义（重要）**：`window_required` 是 goal_gate 条件② 的**分类桶**，判据是
`r_category(ev) == "window_required"`（见 `tools/goal/goal_gate.py` 的 `r_category`/`classify_r_evidence`），
语义是「有 `window_required=true` 登记 ⇒ 条件② PASS」，**不等于**「所有 `window_required=true` 的卡」。

> 动态数字一律带「截至 `2a5101606` @ 2026-10-24 01:03:38」；重跑后桶大小可能变化。

- 本快照下 `r_cards` 共 193 条，其中 `category == "window_required"` 的 **121 张**（卡面写的「约 122 张」，差 1 属快照时刻/归类漂移）；
- 另有 **131 条**记录的 `window_required` 布尔为真，但其中 4 张已 `closed`、4 张 `not_in_board`、2 张被「最新 approve + `window_required=false` + 理由」改判为 `adjudicated` ⇒ 不计入本桶；
- 桶内 121 张的 board 分布：`in-progress` 118、`needs-review` 2、`blocked` 1；`merged=true` 116、**未合并 5**（详见 §6-F1）。


## 2. 抽样规则（先定规则，后抽样）

规则在读取任何卡片正文之前写定，可复算：

1. 取 `gg.json` 中 `conditions[id=2].evidence.r_cards` 里 `category == "window_required"` 的卡，得到池 `P`（本次 `|P| = 121`）。
2. **A 组（旧卡）**：按 `(last_status_ts, card_id)` 升序取前 20。
3. **B 组（哈希）**：对 `P \ A` 每张卡取 `sha1(card_id.encode("utf-8")).hexdigest()`，按 `(hex, card_id)` 升序取前 10。
4. 合计 30 张；不替换、不人工调整。

复算片段（只需 `gg.json`）：

```python
import json, hashlib
d = json.load(open('gg.json', encoding='utf-8'))
ev = [c for c in d['conditions'] if c['id'] == 2][0]['evidence']
rc = {r['card']: r for r in ev['r_cards']}
pool = sorted(c for c, r in rc.items() if r['category'] == 'window_required')
A = sorted(pool, key=lambda c: (rc[c]['last_status_ts'], c))[:20]
h = {c: hashlib.sha1(c.encode('utf-8')).hexdigest() for c in pool}
B = sorted((c for c in pool if c not in A), key=lambda c: (h[c], c))[:10]
```

### 2.1 池的时间分布（影响「旧卡」语义）

`last_status_ts` 分布：min `1792712500`（10-23 07:41:40）、p25 `1792714729`、median `1792714866`、p75 `1792715006`、max `1792774760`（10-24 00:59:20）。

- **92/121（76%）** 的 `last_status_ts` 落在 **2026-10-23 08 时**；这是一次 **「R 级记账补登 20261023」批量 status**（actor `openmir2-worker-43`）把历史 R 卡统一补登记造成的，不是工作新鲜度。
- A 组「最旧 20 张」实际只跨 **07:41–08:10（约 29 分钟）**；用 `last_status_ts` 判断「欠窗口多久」在本桶内**没有区分度**（详见 §6-F2）。
- 30 张抽样**全部是 merged=true**；桶内 5 张未合并卡未被抽中（规则未分层），这是本次抽样的已知盲区（§6-F1、§7-R5）。

## 3. 三档判定标准（可复现）

判定单元 = 该卡**最新 status 的 `work`** 中的「窗口项 / 余量 / 剩余 DoD」段落，辅以最近一次 `window_required=true` 的 approve review 的 `tier_why` 与 merge_result 的 `note`，**并必须读该卡的 whitelist T 行（`strong_t`）**——T 行里明写的「显式不比对」/收窄项优先于 status 散文（返工 v2 起，见 §6-F6 撤销条）。若 status 未单列窗口项，则取全卡所有窗口相关文本。逐档判据：

- **① 可执行**：同时满足
  (a) **具名可执行载体**：脚本/工具路径（如 `run_*.ps1`、`run_attack_pair`），或精确到 `文件:行` / 地图格坐标 / 帧号 / 字段名的夹具；
  (b) **写明期望结果**：具体帧条数、字段值、字节相等、阈值或 DB 列，而不只是「一致」；
  (c) **前置写明且当前可用**，或写明「先完成什么」而该前置不使判据描述失效。
- **② 半可执行**：点到了场景或比较方向（如「两侧同序列 `diff=0`」「逐字节」），但 (a)/(b)/(c) 至少缺一项——常见是无命令/夹具、期望只有 `diff=0`、或前置未就绪/未裁定。
- **③ 不可执行**：只有窗口标签/类目（「真机窗口」「W0/W9 对拍」「需独占窗口」），没有可指认的场景、比较对象或期望。

**判不准 ⇒ 归 ②（保守）**。同一张卡若有多项窗口项，按「能否至少执行一项」定档，但表内注明缺口。

## 4. 结果：10 / 19 / 1

| 档位 | 张数 | 占 30 张比例 |
| --- | --- | --- |
| ① 可执行 | **10** | 33.3% |
| ② 半可执行 | **19** | 63.3% |
| ③ 不可执行 | **1** | 3.3% |
| **②+③** | **20** | **66.7%** |

- ①：`openmir2-m3-party-attribution`、`openmir2-m3-party-gainexp-npe`、`openmir2-m3-pet-slave-lifecycle`、`openmir2-m3-shop-dialog-wiring`、`openmir2-m3-pet-slave-clone46`、`openmir2-m3-pet-slave-masterdie-wire`、`openmir2-window-collector-preflight`、`openmir2-transmap-maptrigger`、`openmir2-maptrigger-monster-and-door`、`openmir2-action-line-variable-text`
- ②：`openmir2-castle-merchant-disappear-downlink`、`openmir2-chat-channels`、`openmir2-m3-castle-gold-tax`、`openmir2-m3-clock-injection`、`openmir2-m3-rng-scene-driver`、`openmir2-m3-drop-exp-wiring`、`openmir2-outbound-encoders-red-batch`、`openmir2-orphan-encoder-wiring`、`openmir2-world-tick-expansion`、`openmir2-buff-system`、`openmir2-gategate-keepalive`、`openmir2-selgate-reconnect`、`openmir2-mb7-monsters`、`openmir2-encoding-core-e1-e2`、`openmir2-weapon-upgrade-values`、`openmir2-login-529-status-text`、`openmir2-castle-guard-target`、`openmir2-weapon-durability-frames`、`openmir2-monster-showhp-53-broadcast`
- ③：`openmir2-m3-monster-active-seek`

### 4.1 改前/改后对照（返工 v2）

| 档位 | 改前（v1） | 改后（v2） | 变化原因 |
| --- | --- | --- | --- |
| ① 可执行 | 11（36.7%） | **10（33.3%）** | `openmir2-monster-showhp-53-broadcast` 无具名命令/夹具 ⇒ 按 §3(a) 落 ② |
| ② 半可执行 | 18（60.0%） | **19（63.3%）** | 同上（+1） |
| ③ 不可执行 | 1（3.3%） | 1（3.3%） | 不变 |
| **②+③** | **19（63.3%）** | **20（66.7%）** | +1 张 |

`openmir2-weapon-durability-frames` 的档位**不变**（仍 ②）：撤销 F6 第 4 条只去掉 (b) 缺口，剩下的 (a)「无命令/夹具」仍成立 ⇒ 不产生第三次分布变动（见 §5.2 第 28 行、§6-F6 撤销条）。

## 5. 逐卡明细

> `status oid` = 该卡最新 status 条目；`review oid` = 该卡最近一条 `window_required=true` 的 approve review（即判定窗口要求的依据）。oid 为 refs 中的完整 40 位。

### 5.1 ① 可执行（10）

| # | 卡 | 窗口项要点（原文摘要） | status oid | review oid |
| --- | --- | --- | --- | --- |
| 1 | `openmir2-m3-party-attribution` | 真机两侧同序列 `diff=0`（`run_party_trade_pair.ps1` + 独占窗口，归 `openmir2-m3-party-trade-align`） | `2f24bdda0b0d6e917fefd1a464112382cfafe55f` | `c16a7365a553bee499e934f6fe9c2e30dd4e812e` |
| 2 | `openmir2-m3-party-gainexp-npe` | 同上：真机两侧同序列 `diff=0`（`run_party_trade_pair.ps1` + 独占窗口） | `32de96ab8e892ff2c749f102162a10d59fa19b9d` | `0f9afda9bdf77856a942557d51dbac3f3c4dd4a3` |
| 3 | `openmir2-m3-pet-slave-lifecycle` | 真机两侧 `run_hero_pet_pair.ps1` 同序列 `diff=0`（召唤/诱惑/叛变/幽灵各站点） | `32283b1bbe41414ba88bd1f59498d0aa36a1b896` | `89e5c4374621c66bac6b7ed6f04f2c72a8784cc3` |
| 4 | `openmir2-m3-shop-dialog-wiring` | 真机两侧 `run_npc_shop_pair.ps1` 同序列 `diff=0`（652 / 700–707 帧序与字节 + `characters_storageitem` 槽位 + `StorageItemList` 50 槽 DB 字段级） | `26da34a36242abe0fc7764825b8ac557e516a179` | `f84d086fc47db69f8748c2dfbef9bd85ff24ea10` |
| 5 | `openmir2-m3-pet-slave-clone46` | 真机两侧 `run_hero_pet_pair.ps1` 的 `make_clone` step 同序列 `diff=0` | `edc4bbc8501df9dfeafbec853ef7a84ccdab2497` | `96740bf78c6d2d01d37596407c3280e6884565ce` |
| 6 | `openmir2-m3-pet-slave-masterdie-wire` | 真机两侧 `run_hero_pet_pair.ps1` 的「主人死」step 同序列 `diff=0`（含叛变站点帧序/字节 + RNG `site/order`）；另列「混合队列未建模」 | `6cbf0b6c54dc5e68c445e3ef6c38bf38b7d13386` | `f3810e72d882aa8c93dc4afcb624718b5d594205` |
| 7 | `openmir2-window-collector-preflight` | 起栈后 C# 流水头 `INJECT_OK<seed>` == Rust `RUST_SEED_INJECTED`；`MIR2_RNG_SITE` 在 C# 半确实 5 列；前置门在 `run_attack_pair` / `drop_exp` 生产路径不误拦（放置点在起栈之后） | `169f7157ef95ab85145248e75020fdb09dcb92f9` | `7b0ab6e2e08b540995c2a61a663e52b3e3d009cb` |
| 8 | `openmir2-transmap-maptrigger` | 真实客户端走进 `M100A 29,81 → M100B 287,282`，两侧 `633/634` 逐字节一致；7 条 NEEDHOLE route 真机不换图；`D5069` 一格多 route 落点与 633 组数；失败回滚与 `RM_MOVEFAIL` 相对关系 | `0f46d4fbdda703b0ba7439e9329b3cff858d10b1` | `bf2805cc1ab5e50ccc02ef12314257aa92b55ff2` |
| 9 | `openmir2-maptrigger-monster-and-door` | W-R1-a：pin route 格怪物走位 ⇒ 两侧均停在走前格（无位移帧）；W-R1-b：门 + ±1 route ⇒ 两侧同样不发 633/634 且 `SM_WALK` 照发；W-R1-c：门注入 `Opened=true` 对照 ⇒ 两侧同样换图 | `98fad62029ae590a6761cf022156160b62b0b5dc` | `cff32beb54ea5e3ff30d3e6d7e803f89ae253318` |
| 10 | `openmir2-action-line-variable-text` | 真机 C# 跑 `E:/MirServer/Mir200/Envir/QFunction-0.txt:388` 的 `SENDMSG 0 <$USERNAME>:<$STR(S1)> 151 235`，确认系统广播里命名变量原样；MESSAGEBOX `玩家:<$USERNAME>元宝:<$GAMEGOLD>` 的 767 尾 = `EDCode(GBK)`，与生产入口用例逐字节一致；同文本两路不同 | `efe0e661ae54f9f84a3231be36a6c668b603cd4d` | `99aa5e2bf29f53f6e8d7417f474c67c1730ecf83` |

### 5.2 ② 半可执行（19）

| # | 卡 | 窗口项要点（原文摘要） | 缺什么（按 §3 的 a/b/c） | status oid | review oid |
| --- | --- | --- | --- | --- | --- |
| 11 | `openmir2-castle-merchant-disappear-downlink` | ⑧ 接通后城堡战进入/退出战中两侧帧集合 `diff=0` | (a) 无命令/夹具；(c) ⑧ 沙巴克攻城层未接线，`castle_under_war` 生产无写点 ⇒ 前置今日不可用 | `5c820bed2dbc9f88122aa6050a4e50c8a4081ab9` | `7f49d13036708ceafcfdb4d2fdad79b7f384abd5` |
| 12 | `openmir2-chat-channels` | 真机三前缀（`!!`/`!~`/`!`）的收件人集合与帧字节；10s 喊话门边界（严格 `>`）；喊话成功路径零帧（NRE 1:1）与 Error 日志 | (a) 无命令/夹具；(b) 收件人集合/日志未给具体期望 | `48e92eb74867c2742f8c1fe49b10c94c9f48299e` | `cc6fae31c7f02ec4cb02e078579144ba9fbdccca` |
| 13 | `openmir2-m3-castle-gold-tax` | 真机两侧「同一 10min SaveTick 时间门 → `TodayIncome`/`TotalGold` 落盘」`diff=0` | (a) 无命令/夹具；(c) 依赖 ⑧-1 C# `GetTickCount` 注入片 + owner 独占窗口，注入片未落地 | `7565076f7b902cc101c0df58ce1f07f506548e82` | `8669e2ebc5e9601c6b838dc677e5c2aee4f18977` |
| 14 | `openmir2-m3-clock-injection` | 真机两侧「同起点 + 同推进」时间门 `diff=0`；另列 C# 影子 hook 半（待 owner 在 IL patch / 影子源码重建 / 按墙钟编排间裁决） | (a) 无命令/夹具；(c) C# 半未定，前置不可用 | `a1b5d86db8447f6a99ba5e3ed807552365bda634` | `2ca7de877f3ff74d0146911ae28ea1e057363f3d` |
| 15 | `openmir2-m3-rng-scene-driver` | RNG 序列真机两侧对拍（W9 独占窗）；旧 status 另列 BotSrv real wiring（进 0137 / pin 怪 / dump） | (a) 无命令；(b) 未给账本字段/期望；(c) `BotSrv` real wiring 本期未接 | `e13059c5f404f98528202a612bf28f6f5f6c5d77` | `8467f3b3ed87b421f09ee2bd4e606c3688fbdb69` |
| 16 | `openmir2-m3-drop-exp-wiring` | 真机 100 杀两相窗口（依赖 T-59 rng-scene 已落）+ quest 真机窗口 | (a) 无命令/夹具；(b) 期望仅「同序列 diff=0」 | `d898ee293017bee405737a3edabacf4397a93e63` | `c5979402c13a50a23e5d50b05a8122bc65fd6d65` |
| 17 | `openmir2-outbound-encoders-red-batch` | 654 入队相对序必须真机定；制药链 `Merchant.ClientMakeDrugItem` 的 Random 次第、`SM_USERMAKEDRUGITEM`/`ADDITEM`/`DELITEMS` 与结果帧相对序、`MYSTATUS` 派生字段落库前后一致 | (a) 无命令/夹具；(b) 「一致/相对序」未给判定字段或次序表 | `d68b0466eb6d828df5dc4f2e1987fd0b0f48aa5c` | `4505ad8c4ffaa96af39065061a293a956f0b2322` |
| 18 | `openmir2-orphan-encoder-wiring` | 走出视野旧位置幽灵消失；两真客户端 `/名字` 私聊双向可见；离世广播收件人集合口径；同 tick 消失批先于出现批 | (a) 无命令/夹具；(b) 收件人集合仅定性；(c) 部分依赖未建模假设（视野已失效边界） | `fd606fc30e2b1b432374789d6a450e7bd27e5583` | `3ec7717fcee771a18bfb77df780a32af0ec68c5b` |
| 19 | `openmir2-world-tick-expansion` | 两侧 RNG `site/order` 账本按已声明的消费者序逐项一致（npc_tick 次序格在注释/契约用例里）；`SM_DAYCHANGING(46)` 重发时刻与对象；滚动公告 `ShowLineNoticeTime` 节拍与逐玩家滚行；Ghost 60s 与 `RunTime` 门 | (a) 无命令/夹具；(b) 未给账本字段/公告帧序列期望 | `42e0333e21917b7f0987b225bc3ff16cf48cf633` | `b8cd2d89a4e58bc59658023c0b89afe44a3889de` |
| 20 | `openmir2-buff-system` | 三怪癖真机表现（`break` 跳 `for(j)`、递减 50ms vs 1s 两读法、怪物 buff 永不到期）；毒伤 DoT 逐次流水与 `MIR2_RNG_LOG`；`CharStatus` 位序跨帧一致；魔法盾 MP 抵扣与 `LastHiter` 守卫 | (a) 无命令/夹具；(b) 窗口项①明写「两读法待窗口 A-R2 裁定」⇒ 判据未定 | `369886bb184a9a8e299d2919ea17f96cb7b750f0` | `0d878c122391855a64eacf4c8f7d48cb71c31479` |
| 21 | `openmir2-gategate-keepalive` | 10s 健康帧周期与首帧相位；3min 窄竞态分支可达性（需 owner 裁决）；断链重连与 15 天空闲清理 | (a) 无命令；(b) `==` vs `>` 边界与 3min 可达性未定；(c) 需 owner 裁决 | `eb82597425bd52f2f8e404fe6486cc86c3272af1` | `79709ef84a0b8aa3da862538d49f1006fa98497c` |
| 22 | `openmir2-selgate-reconnect` | 真机 DBSrv 重启后 SelGate 重连与在途会话；断链期间真实客户端表现 | (a) 无命令/夹具；(b) 未给期望（重连次数/在途请求结果/日志串） | `6f3202cf7de6d47eb051e424bf02c908d045ca79` | `60916e1983d661963258168b32cc775d504a7cea` |
| 23 | `openmir2-mb7-monsters` | race 113/114/134 的 Run/克隆/召唤（部署数据不可达时以构造级为准）；T-143 展示色与排水相位 | (b) 未给帧/字段期望；(c) 部署不可达，窗口只能验构造面；无命令 | `ed118ed86262404a08b77ed1128ff0fa0d6c76a1` | `108da7b78a82cdee85c42eab86440a4ae48a0fd1` |
| 24 | `openmir2-encoding-core-e1-e2` | 说词/物品名等非 ASCII 文本逐字节；`StringList` 与 `ConfigFile` 两条 BOM 路径实测 | (a) 无命令/夹具；(b) 「逐字节」未给样本/期望字节 | `968fdf5f5d77147049aeb39eacd9e0aa176aa297` | `b5c93e2045ee227dc6221118f683c5a57756ed36` |
| 25 | `openmir2-weapon-upgrade-values` | 真机 `@upgradenow` 全量对拍四字段与 657 帧；取回路径 Dura 档/n90 抽数逐抽；202/200 `ClientItem` 字节；零 RNG 探针；多件首饰三列独立 | (a) 无命令/夹具；(b) 未给具体期望值 | `7ceff41cd9067918b47d22c185c93c7e250168e1` | `36974a5df070273630536234e6933837a0afb4a0` |
| 26 | `openmir2-login-529-status-text` | 真机对拍台 `login_only` 复跑，529 两侧 body 逐字节相同 | (c) **当前不可能达成**：Rust GameSvr 不上报 `SS_SERVERINFO`（`mir2-rs/bins/game-svr/src/ids.rs:18` 登记不做）⇒ Rust 侧 `is_server_online=false` ⇒ 状态段恒 `0` | `98b65509d72ecc2a4de7f9c6f60481396172dc4e` | `3e1d4001525f7c26cb5c6ac8654125898df6922d`；**2026-10-24 订正（卡 `openmir2-window-criteria-backfill-2`，依据 `185a6c8cb` + master `01d4f5877`）**：上面「(c) 当前不可能达成 / `SS_SERVERINFO` 未上报 ⇒ 状态段恒 0」**已作废**——`SS_SERVERINFO` 已接（`mir2-rs/bins/game-svr/src/ids.rs:17-19`）；改判 **A 类 ① 离线可跑**：`mir2_probe --check-scenario --scenario login_only.json` rc=0、`card_window --dry-run --profile alt` rc=0，真机待窗口（判据与命令见 `docs/window-criteria-backfill-2-20261024.md` §3） |
| 27 | `openmir2-castle-guard-target` | 同城/盟友/UnderWar 下的守卫目标选择（真机）；`Castle!=null` 受害者经 `AnimalObject.Struck` 的合法性判定（T-96⑥④，另开卡） | (a) 无命令；(b) 未给期望判定；(c) 第二项另开卡未就绪 | `4be5ec56062ae16d81f5e735517062fcfc2cfb01` | `bebc44d758ac07641d44731dfbdc6d67cd5e0140` |
| 28 | `openmir2-weapon-durability-frames` | 655 真机帧面（含 `DecLampDura=false` 的 1500 → 每周期一条怪癖）、1102 真机 | (a) 无命令/夹具（无脚本/工具路径，也无可复制命令行）；(b) **不缺口**：whitelist **T-199** 已写死期望（655 子集条数/两支 `Recog`/12B、1102 帧面与 `characters_item` 字段），并明写 **654/709 本窗口显式不比对**⇒ 原「按字面必红」不成立（§6-F6（撤销条））；(c) 无阻塞前置 | `4756c3c589b8aac878634effb7cbe27fc6445d95` | `af754e4359c35f6865e2e32393ed9f5a4df6558f` |
| 29 | `openmir2-monster-showhp-53-broadcast` | ShowHp 怪/NPC ±12 帧集合、53/1100/1101 同 tick 序、ObMode 源自见 2 条、`VisibleHumanList` 500ms 缓存 | (a) 无具名命令/夹具：status 与 merge_result `note` 只给场景名词，未给脚本/工具路径、场景文件或可复制命令行（reviewer 复核同判）| `855663a75f6dbf592d97275ead08f9afbbef765e` | `593ae8bf163887e62137ec93fc19a44428caa5cd` |

### 5.3 ③ 不可执行（1）

| # | 卡 | 窗口项要点（原文摘要） | 缺什么 | status oid | review oid |
| --- | --- | --- | --- | --- | --- |
| 30 | `openmir2-m3-monster-active-seek` | 「战斗数值/时序/RNG 真机对拍（W0/W9）」 | 无场景、无比较对象、无期望、无命令；仅窗口标签 + 类目。旧 status 另登记「覆写 Run 的子类、SuperGuard/ArcherGuard 自有 pvp 扫描」未建模，但未转成窗口项 | `26c41b0a71ef1120217549aa43d780d8f0efc92c` | `513cf39fab54741ba4d3ae969aa0ec3817030b14` |

## 6. 发现

**F1（桶语义与卡面不一致）**：卡面说「`window_required` 约 122 张」，实际快照是 **121 张分类桶**；更关键的是桶 ≠「`window_required=true` 全集」：131 条 wr=true 中 4 张已 closed、4 张 not_in_board、2 张被改判。桶内还混入 **5 张未合并卡**（`openmir2-gm-batch-5-guild-castle`、`openmir2-script-envir-root-host`、`openmir2-script-sendmsg-host`、`openmir2-script-user-account`、`openmir2-window-gate-two-runs`），但 `r_category` 把 `window_required` 排在 `merged` 之前、`classify_r_evidence` 直接 `PASS`，所以它们**因「有登记」而非「有真机证据」被计为已登记**。本次 30 张按规则抽样全部落在已合并的 116 张里，未覆盖这 5 张（已知盲区；补充阅读显示 `script-sendmsg-host` 的 W-1/W-2/W-3 已把命令指到 `docs/sendmsg-host-20261023.md §6`，按 §3 初判偏①，但未计入 30 张分布）。

**F2（`last_status_ts` 不表示欠窗口时长）**：92/121 的时间戳来自 2026-10-23 08 时的「R 级记账补登」批量 status；A 组「最旧 20 张」只跨 29 分钟。拿它的排序当「窗口欠账优先级」会得出错误结论。

**F3（多数窗口判据仍是散文）**：30 张里 ②+③ 占 **20 张（66.7%）**；③ 1 张。池级佐证：**121 张里只有 9 张（7.4%）** 的线程文本提到任何 `run_*.ps1`；绝大多数窗口项没有可执行载体。

**F4（① 的可执行性多来自外部脚本，而非卡面自洽）**：10 张①中 6 张靠 `mir2-rs/tests/parity/m3/` 下真实存在的 pair 脚本（本次核过 `run_party_trade_pair.ps1`、`run_hero_pet_pair.ps1`、`run_npc_shop_pair.ps1`、`run_attack_pair.ps1`、`run_drop_exp_pair.ps1`），1 张（window-collector-preflight）点名 `run_attack_pair`/`drop_exp` + 精确期望，其余 3 张靠精确夹具/期望。**即便是①，多数卡面也没有给出实际命令行与参数**——读者必须去读脚本 header 才知道 `-Side`/`-Go`/步骤串怎么传。建议②/③ 回填时直接抄脚本 header 的调用示例。

**F5（`weak_t` 不是登记）**：样本中只有 7/30 张有 `strong_t`；其余大量 `weak_t`（如 castle-merchant 的 T-142/T-154/T-193）只是行文引用别的卡的 T 行，不是本卡的验收判据。审查时若把 `weak_t` 当窗口判据来源会误判。

**F6（② 的典型缺口；原第 4 条已撤销）**：
- **前置不可用**：castle-merchant（⑧ 未接线）、castle-gold-tax（⑧-1 时钟注入未落）、clock-injection（C# hook 待裁决）、login-529（`SS_SERVERINFO` 未上报，字面「当前不可能达成」）。这 4 张应标 `blocked_by`，不应进入「可开窗」队列。
- **期望只有「diff=0/一致」**：drop-exp、rng-scene、encoding-core、weapon-upgrade 等，没有「比哪些字段/帧/列」的清单。
- **判据自身未定**：buff-system 的 50ms vs 1s 两读法明写「待窗口 A-R2 裁定」；gategate-keepalive 的 `==` vs `>` 边界与 3min 可达性待 owner 裁决。
- ~~**判据自相矛盾**~~ **【返工 v2 撤销，F6 第 4 条不成立】**：原写「`weapon-durability-frames` 的窗口判据与它自己的 review I-1 自相矛盾」——**错**。whitelist **T-199** 原文已明写「**654（`RM_CHANGELIGHT`）与 709（`SendDelItems`）在本窗口显式不比对**（Rust 未实现，T-79⑥/T-95① 承接）——判据写成『655 子集两侧逐字节一致，且 654/709 两侧同为不比对』，**不得留不可满足的表述**」；review I-1 也正是记录该收窄。**错因：判定单元只读了 review 里的旧问题描述，未读 whitelist 的 T 行原文**（原 §3 判定单元未把 T 行列为必读来源）。**自本返工起**：判定单元必须同时读该卡的 whitelist T 行（`strong_t`），判据缺口与档位以 T 行原文为准。

**F7（与已核实事实的呼应，非本次样本）**：`gate-gmclose-race`（当前 `merged_not_closed`，不在本桶）的 status 只写「无待窗口项」，是纯定性；`combat_strike` 的封口一度只在 description、后来才被 BLOCKED 硬门技术化——与本审计的 F3/F6 同型：**登记存在 ≠ 判据可执行**。

### 6.1 回核 whitelist T 行（返工 v2，F6 同类漏读排查）

起因：F6 第 4 条错在「只读 review、未读 T 行」。据此把其余发现逐条回到 `mir2-rs/tests/parity/whitelist.md` 原文核了一遍（只读，未改 whitelist）：

| 发现 | 回核的 T 行 | 结论 |
| --- | --- | --- |
| F1 桶语义 / 5 张未合并卡 | 5 个卡 id 在 whitelist 全表检索：仅 T-208 以「**承接卡** `openmir2-script-sendmsg-host`」形式出现，按 `goal_gate.py::build_whitelist` 的 `_ref_cued`（见/原/已由/承接/立卡/引用/占用/详见/另见）不计归属；其余 4 张无任何 T 行 | **成立**（5 张均无 T 行归属；`walgit collab thread` 亦确认 5 张都无 `merged:true`） |
| F2 `last_status_ts` 不表示欠账时长 | whitelist 全表检索「补登」无相关 T 行（唯一命中 T-58 与本批无关） | **成立**（时间戳来自 goal_gate 快照，不依赖 T 行） |
| F3 判据多为散文 | 无对应 T 行；池级 9/121 来自线程文本统计 | **成立**（本返工分布改为 ②+③ 20/30 = 66.7%） |
| F4 ① 依赖外部脚本 | 5 个脚本均 `git ls-files` 命中（`run_drop_exp_pair.ps1` 在 `m3/drop-exp/` 子目录） | **成立**（① 变为 10 后：6 + 1 + 3） |
| F5 `weak_t` 不是登记 | `classify_r_evidence` 对 `weak_t` 返回 `NEED`（不是 PASS）；whitelist 归属只认「卡 `X`」/第二列/作者登记组。抽查 castle-merchant 的 T-142（点名 scriptpd 车道）、T-154（点名 `openmir2-npc-display-table`）、T-193（MESSAGEBOX）均不点名本卡；本卡真正的 strong_t 是 T-192 | **成立** |
| F7 `gate-gmclose-race` / `combat_strike` | whitelist「B2 批 4」末段明写「本批未登记 1 张（无窗口判据）：`openmir2-gate-gmclose-race`」并引 status/review/comment oid；`combat_strike` 的 BLOCKED 技术化见 `docs/goal-snapshot-late-20261023.md:245` | **成立** |

> 同时按 `build_whitelist` 实算，30 张样本里有 whitelist 归属（`strong_t`）的共 7 张（showhp=T-31、action-line=T-165、castle-merchant=T-192、rng-scene=T-59、drop-exp=T-32、mb7=T-143/T-144、weapon-durability=T-199，与 F5 的 7/30 一致）：逐条读原文后，只有 T-199 改变了判据缺口的判断（即 F6 撤销 + §5.2 第 28 行 (b) 改判），**其余均不改变档位**。

## 7. 建议

**R1（推荐先做，成本最低）：开一张「窗口判据补齐」卡（可按窗口队列分片）**，把 ②/③ 回填成模板，不改代码：

```text
窗口项 <W-id>
  场景/夹具：地图 <id> 格 (<x>,<y>) / 脚本 <file:line> / 步骤串 / 种子
  命令：<可直接复制的命令行，含 -Side/-Go/参数>
  期望：<帧 ident + 条数 + 头字段字节 / DB 表.列值 / 日志串>（至少一项可自动 diff）
  前置：<哪张卡/哪个开关必须先就绪>
  blocked_by：<卡 id 或 none>
  不比对：<明确写死本窗口不覆盖的面，防「按字面必红」>
```

优先顺序：③（`m3-monster-active-seek`）→ ②中**前置已就绪**的（gategate-keepalive、selgate-reconnect、encoding-core、orphan-encoder、world-tick、outbound、drop-exp、weapon-upgrade、castle-guard、mb7、buff-system 中可先定口径的部分）→ 前置被阻塞的（castle-merchant、castle-gold-tax、clock-injection、login-529）先补 `blocked_by`，不排窗口。

**R2（结构性，治本）：把窗口项从 `work` 散文升级为结构化字段**，例如 status/approve 里的 `window_items: [{id, scenario, fixture, command, expect, prereq, blocked_by, not_compared}]`。这样「可执行性」可以被程序判，而不是靠审计员读散文。短期先作为**可选**字段，老卡回填时逐步迁移。

**R3（goal_gate 启发式，只告警不改判）**：条件② 可为 `window_required` 分类额外计算一个**仅供报告**的 `window_criteria_quality`，**绝不影响 PASS/FAIL/need_human**（避免把「判据写得好不好」混进终点门禁）：

- 输入 = 最新 status 的 `work` + 最近 merge_result `note` + 最近 `window_required=true` review 的 `tier_why`；
- `executable`：命中 `run_[A-Za-z0-9_]+\.ps1` 或 `[\w./-]+\.(ps1|py|sh|rs):\d+` 或 `[A-Z]\d{3,4}[A-Z]?` 地图号 + 坐标 `\d{2,4}\s*[,，]\s*\d{2,4}`，**且**命中期望词（`逐字节|diff\s*=\s*0|恰好\s*\d+\s*条|槽位|字段级|字节一致`）；
- `semi`：命中窗口词（`窗口项|待真机|真机.*对拍|W\d+\s*窗`）但不满足 executable；
- `non_exec`：只命中窗口词且 `work` 长度 < 阈值/无场景名词；
- 在 `detail` 追加 `窗口判据：可执行 N / 半 M / 不可执行 K`，K>0 或可执行占比 < 50% 时给 `warnings`。
- 注意：正则启发式会误判（例如把 revision 号当帧号、把 `run_*` 当脚本），所以**只作提示**；R2 落地后以结构化字段为准。

**R4（队列卫生）**：`window_required` 桶内 5 张未合并/受阻卡不应与「已合并待窗口」混排。建议在 gate 的 `window_required` 分支输出里拆出 `merged` 子计数，或在卡面要求「未合并卡的 wr=true 只作预登记，不计入待窗口队列」。

**R5（下一轮审计）**：按 `merged` 分层抽样（已合并 116 / 未合并 5 各抽），并把 ① 卡的「实际命令行/参数是否在卡面」单独统计；回填后重跑本文件 §2 的规则即可对比。

## 8. 复算与残留物

复算命令（本机，PowerShell 或 Git Bash 均可）：

```sh
# 1. 生成快照（需要约 35–60s；rc=2 是 overall=need_human，JSON 仍完整）
python -I tools/goal/goal_gate.py --json > <tmp>/gg.json
# 2. 按 §2 片段取 30 张
# 3. 逐卡读 status/review/whitelist 行：
#    walgit collab thread <card>  或  git cat-file -p <refs/collab/inbox/...>
```

本次审计的临时文件均在 `E:/tmp/worker69-audit/`（`gg.json`、`sample.json`、`digest.txt` 等），**已删除**，仓库内只保留本文件。未启动任何服务端/客户端进程，未占端口，未改卡片/whitelist/工具。
