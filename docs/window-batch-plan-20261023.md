# 窗口批次执行清单（2026-10-23）

- 卡：`openmir2-window-batch-plan`；owner `openmir2-worker-66`；tier=L；write_set=`docs/`（零代码）
- 基线：master `058c3ad25`；worktree `E:/tmp/wt-worker-66`，分支 `card/window-batch-plan`
- 输入：全部 `refs/collab/inbox/*` 条目（6793 条，按 `git cat-file --batch` 直读，与 `walgit collab thread` 同源）、`docs/window-debt-reclass-20261023.md`、`docs/r-card-ledger-sweep-20261023.md`、`docs/conformance-a-card-scenario-map-20261023.md`、`docs/window-runbook-20261023.md` §4.5、`tools/probe/scenarios/*.json`（15 个）、`mir2-rs/tests/parity/m3/run_*_pair.ps1`（11 个）

## 0. 一句话结论

**窗口欠账不是 114 张，是 173 张**（机器可复现口径见 §1）。其中：**第 1 批 53 张**（9 个 probe 场景 40 + 既有夹具 13）、**第 2 批 4 张**、**第 3 批 81 张**、**不可协议对拍 35 张**。每一张都落到某一批并给了场景与前置，无遗漏。

| 批次 | 张数 | 开跑条件 |
|---|---:|---|
| 第 1 批 · A（probe 9 场景） | 40 | `card_window.py` + 参数发现接线（`openmir2-window-discovery-wiring`）；其中 1 张部分可验 |
| 第 1 批 · B（既有夹具） | 13 | 夹具在 master（含子目录，如取号用 `find -name`）；逐卡前置见行内 |
| 第 2 批 · A（move/king/combat 规则） | 2 | `openmir2-window-discovery-rules` 落地 |
| 第 2 批 · B（SM_BAGITEMS 规则） | 2 | **需新开卡**（现有规则卡不覆盖） |
| 第 3 批（新帧/新场景/新前置） | 81 | 逐卡前置，见行内 |
| 不可协议对拍 | 35 | 替代判据见行内 |
| **合计** | **173** | |

> 第 1 批全部跑完，**可闭环 52 张（30%）**：1P 40 张里 `openmir2-player-searchviewrange-cadence` 只有项 ① 可验（项 ③ 收件人近似需第二观察者），1F 13 张按卡面前置。**121 张**要等新场景/新帧/工具（第 2、3 批）或走替代判据（不可对拍）。把「第一批 9 个场景」当成整场窗口的解法会严重低估工作量。

## 1. 为什么看板说 114：差额逐项解释（可复现）

### 1.1 114 的真实口径 = 「看板文本里写着 已合入 master 的卡」

对 `refs/collab/inbox/*` 的全部不可变条目做**时点重建**（每卡取 ts ≤ `1792733280` 的最后一条 `status.work` + `issue.title/prose`，即 master `f7a32c2f` @ 2026-10-23 13:28 +0800 时的看板文本），按字面 grep：

| grep 字面 | 命中卡数 |
|---|---:|
| `已合入 master` | **114** ← 这就是 114 的出处 |
| `窗口项` | 128 |
| `window_required` | 114（与上一行同数，纯属巧合） |
| `窗口`（宽） | 237 |
| **`窗口欠项`** | **0** |

⇒ **`窗口欠项` 这四个字从未出现在任何看板条目里**，只存在于 Leader 的两份文档；`window-runbook-20261023.md:8` 与 `leader-goal-20261023.md:20` 的「提到窗口欠项 151 张」无法复现（我测到的近似命中是 128 / 148 / 156，没一个等于 151；无法复现其算法）。**114 是「已合入 master」的命中数，不是窗口欠账数。**

### 1.2 从 114 到 173 的桥

| 项 | 张数 | 说明 |
|---|---:|---|
| Leader 快照的 `已合入 master` 卡 | 114 | 见 §1.1 |
| − 其中**至今状态为 closed** | −70 | 多数是 M2/M3 实现卡、文档/工具卡（已收口）；含 1 张 tier=R 但 `window_required=false` |
| − 其中（非 closed 的）**tier=L**（本就不欠窗口） | −23 | 含本批的 `window-debt-reclass`、`tiered-acceptance-policy` 等文档卡 |
| = 114 里真正落在欠账集 | 21 | |
| + 窗口欠账但**看板文本没写「已合入 master」**的卡 | +149 | 它们用的是「R 级记账补登」「已落地（tier=R 不收卡）」「窗口 r3」等措辞（`r-card-ledger-sweep` 实测 35 张 B2 即此类） |
| + 快照之后才合入的 R 卡 | +3 | `frame-drain-order-audit`、`gm-impl-shutup`、`gm-script-host-seam` |
| **= 本清单的欠账集** | **173** | |

### 1.3 本清单的机器可复现口径

```text
欠账集 = { 卡 | 存在 merge_result{merged:true} 且 最后一条 status != closed
              且 (最新 approve review 的 tier == R  或  全线程没有任何 tier 声明) }
```

- 同口径实测：merged-not-closed **217** / 其中最新 approve tier=R **155** / 无 tier 声明 **18**（AGENTS.md「缺 tier 按 R」）⇒ **173**。（任意一条 review 声明过 R 的是 **162**；其中 7 张最新 approve 已改判 L，按「最新一次判定」不计入。）
- 155 张 R 卡的证据强度：approve 显式 `window_required=true` **105** 张、显式 `window_items` **69** 张；其余 50 张只有 `status.work` 里的文字；18 张无 tier 卡里有 15 张是 M3 夹具/对齐卡。**这 68 张「只有 work 文字」的卡是本清单置信度最低的一档**（行内已标）。
- 机械集合含 2 张最新 approve 为 `window_required=false`：`gate-gmclose-race`（§5.4 已单列为非窗口欠账，建议 owner closed）与 `login-frame-order-52-752`（后续「R 级记账补登」又把窗口项登记回来，故仍算欠账）。不影响 173 与分批。
- **卡数 ≠ 合并数**：173 张里有 14 张共用 5 个 merge 提交（`27d22a9f3`/`759320894`/`50dd3f2ef`/`6a774ea79`/`c518e075b`），按合并提交数只能数出约 164，按卡数才是 173——这是「看板聚合的重复计数」的实际来源。

> 复现命令见 §6。

## 2. 9 个可跑场景到底能验什么（决定第 1 批边界）

| 场景 | 观察面 | 能验的典型判据 | 验不到 |
|---|---|---|---|
| `login_only` | 50/52/658/654/657/100（累计口径） | 登录帧序/数值/首扫相位/DB 登录行 | 任何需要客户端主动操作的帧 |
| `npc_click` | `CM_CLICKNPC` → 643/100 | 固定 NPC `@main` 说词与 Recog | 需要继续选标签的链 |
| `npc_click_auto` | 同上，actor 取最近一条非 self `SM_TURN` | 同上（免 --var） | 同上 |
| `merchant_dialog` | 点商人 → 643 → `@buy` → 645 | 商人 `@main`/`[~@repair]`/`$STR` 展开/商品列表 | 升级、修理等其它标签 |
| `pickup_item` | `CM_PICKUP`（脚下）→ 200/201 | 拾取入包帧 | 需要先掉落的场景 |
| `dice_chain` | 点 NPC → `@kaiz`→`@ya5`→`@ya52` → 643/767 | 正是 `script-take-persist` 的脚本链 | 其它标签链 |
| `dice_chain_auto` | 同上（actor auto） | 767 分支 | 同上 |
| `gm_batch` | 5 条 GM 命令 → 100 | `@时间/@拒绝/@拒绝私聊/@help/@LockLogin` | 需 SETPERMISSION=10 的命令 |
| `gm_time` | `@时间` → 100 | 单条 GM 回包 | 同上 |

**共同限制（决定了「不可对拍」那一列）**：驱动器只有**一个客户端会话**。任何判据要求「±12 格收件人集合」「两个观察者」「两号同时在线」，9 个场景**一个都验不到**——不是参数问题，是会话数问题。

## 3. 逐卡清单（173 张）

列：卡 id / 状态 / 窗口项要点 / 场景 / 前置。状态取最后一条 `status`；`needs-review` 的两张是返工后未合入当前 head 的卡。

### 1P · 第 1 批 · A：9 个可跑 probe 场景（零新增代码）（40 张）

| # | 卡 id | 状态 | 窗口项要点 | 场景 | 前置 / 说明 |
|---:|---|---|---|---|---|
| 1 | `openmir2-world-tick-expansion` | in-progress | ① 两侧 RNG 逐次流水（site/order 账本）在同一声明的消费者序下逐项一致——本批已把 npc_tick 的次序格写进注释与契约用例，真机按该格对拍；② 跨桶 `SM_DAYCH… | login_only + MIR2_RNG_LOG | RNG trace 双开；npc_tick 次序已写进契约 |
| 2 | `openmir2-ctor-draws-fold` | in-progress | ① 屏障 A 的 1 抽（ServerConf.cs:114 ItemNumber += Random(10000)）——补上后段②/段③ 与 oracle 等距，须另立片；② 真机窗口（两… | login_only + MIR2_RNG_LOG | 期望 68 抽/17 站点；屏障 A 的 1 抽另立片 |
| 3 | `openmir2-ctor-barrier-a-draw` | in-progress | ① 真机窗口（**默认流水已变，须重取基线**；构造段账本 69 抽/18 站点）；② 宿主那一行绑定的进程级牙（需「新角色登录后首包 SM_BAGITEMS 的 make_index」用例… | login_only + MIR2_RNG_LOG | 默认流水已变须重取基线；构造段 69 抽/18 站点 |
| 4 | `openmir2-mapcell-attributes-loading` | in-progress | ① 真机窗口（**必核**：route added=1649、刷怪位置与 RNG 流水、进图/落格/AOI/CanWalk(true)/snap）；② `castle.rs::apply_s… | login_only + db_snapshot | route added=1649 / 刷怪位置 / 落格 / AOI |
| 5 | `openmir2-enter-shift-loop` | in-progress | ① 真机窗口（墙格出生登录：RNG 流水 + 落点；默认零抽）；② HomeMap 不可解析分支的 oracle `goto ReGetMap` 语义未建模（当前不可达，已登记）；③ `ed… | login_only | 墙格出生：RNG 流水 + 落点（默认零抽） |
| 6 | `openmir2-login-notice-clienttick` | in-progress | ① 真机窗口（含 `SoftVersionDateEx==0` 两侧构造）；② **方案 A**（完整等待环 + 10s 超时踢人 + 等待期丢弃）另开 R 卡；③ 8 个显式 MagIdx… | login_only | 含 SoftVersionDateEx==0 两侧构造 |
| 7 | `openmir2-aoi-verify-maptime` | in-progress | ① **真机窗口**——同缩窗场景需验 vis>=1、nearest<=vr、capture 出 SM_TURN(281,661) + 配对 SM_FEATURECHANGED、SM_WAL… | login_only | vis>=1/nearest<=vr、SM_TURN(281,661)+41、SM_WALK=0 |
| 8 | `openmir2-player-searchviewrange-cadence` | in-progress | ① **真机窗口**——首扫约 1.2s、之后每 1.2s；② 非玩家扫描节拍仍每 tick（另卡）；③ `RM_DISAPPEAR` 收件人近似（误差窗口随节流放大）——应改格扫或窗口验证… | login_only（部分可验：仅项 ①） | 项 ① 首扫约 1.2s/之后每 1.2s 可闭环；项 ③ RM_DISAPPEAR 收件人近似需第二观察者 ⇒ 移多会话批次 |
| 9 | `openmir2-persist-create-rows` | in-progress | ① 真机窗口（建角/兜底建角往返）；② `name_quick`/`name_text` 分裂的同名判重（另卡） | login_only + db_snapshot | 建角/兜底建角往返 |
| 10 | `openmir2-persist-item-attr-and-gold` | in-progress | ① G-01 功能上仍不落库（oracle 门决定，`B-117`）；若要修，单开批次（upsert + 按 MakeIndex 关联 + 读全 14 列）；② G-02 仍丢失（`B-11… | login_only + db_snapshot | 建角/存档往返；G-01/G-02 口径另记 |
| 11 | `openmir2-save-on-shutdown` | in-progress | ① **真机窗口**（`DB_SAVEHUMANRCD` 帧与落库 diff、`-Stop` graceful 日志；并裁决 D-12 的「oracle 是否真的落库」）；② `CTRL_C… | login_only + 停栈后 db_snapshot | DB_SAVEHUMANRCD 帧与落库 diff、-Stop 日志；同时裁决 D-12 |
| 12 | `openmir2-sm-additem-tail` | in-progress | 真机窗口 + T-79/T-80 登记项 | login_only | T-79/T-80 帧尾逐字节 |
| 13 | `openmir2-sm-updateitem` | in-progress | 真机窗口 + T-79/T-80 登记项 | login_only | T-79/T-80 帧尾逐字节 |
| 14 | `openmir2-login-useitems-magic` | in-progress | 真机窗口 + T-79/T-80 登记项 | login_only | T-79/T-80：621/211 登录装载 |
| 15 | `openmir2-sm-merchantsay-recog` | in-progress | 真机窗口 + T-79/T-80 登记项 | npc_click / merchant_dialog | 643 的 Recog 口径 |
| 16 | `openmir2-smframe-minor-gaps` | in-progress | 真机窗口 + T-79/T-80 登记项 | login_only | T-79/T-80 帧面小缺口 |
| 17 | `openmir2-persist-single-sided-columns` | in-progress | 真机窗口；失败移动后 `DisplayTable.dir` 未随实体（建议单卡） | login_only + db_snapshot | 单边列/仅 Rust 列的落库 |
| 18 | `openmir2-save-misc-gaps` | in-progress | 真机窗口；G11 传输面未移植；G7 读到一半失败未构造 | login_only + db_snapshot | 存档杂项；G11 传输面与 G7 半失败另记 |
| 19 | `openmir2-login-frame-order-52-752` | in-progress | 非 TestServer 部署/发光装备/IsSpirit 的槽位变化未取样本；621/211 的 Rust 侧还依赖 211 空槽修复（卡 openmir2-login-magic-211… | login_only | 52 布局(54,78)/621/211 相对序 |
| 20 | `openmir2-login-numeric-gaps` | in-progress | 新号首登 52 的 Weight=10 / MaxExp=10；新号首杀 44/45/52/752 帧序（不得出现 45）（共 3 项） | login_only（首杀项另需 combat_hit） | 首登 52 Weight/MaxExp=10 可先验；44/45/52/752 首杀序需第 2 批 |
| 21 | `openmir2-guild-g2-dispatch` | in-progress | 750 的真机出队序（46 之后、654 之前）；104 行会消息的线缆（共 3 项） | login_only | 750 出队序（46 之后、654 之前） |
| 22 | `openmir2-encoding-core-e1-e2` | in-progress | 真机：说词/物品名等非 ASCII 文本的逐字节；StringList 与 ConfigFile 两条 BOM 路径的实测 | npc_click / merchant_dialog | 非 ASCII 说词逐字节；BOM 两路属离线（见不可对拍） |
| 23 | `openmir2-script-playdice-openybdeal` | in-progress | 真机：赌博 NPC 的 playdice 标签放行（接 bins 后）；OPENYBDEAL 成功/不足/已开通三支文案与扣费（内容可达时） | dice_chain | 赌博 NPC playdice 标签放行；OPENYBDEAL 三支部署不可达另记 |
| 24 | `openmir2-login-654-order` | in-progress | 改动后 Rust 侧登录抓包（654 跨 tick、第二 42、100×4）与 C# 同场景对拍；有会角色 C#+Rust 各一次（811/46 与 750 位置）（共 3 项） | login_only | 654 跨 tick、第二 42、100×4；有会角色 811/46/750 |
| 25 | `openmir2-market-def-impl` | in-progress | 真机：商人 @main/[~@repair] 说词与变量展开（27 处 $STR）；未接分支：@exit/@@sendmsg/@upgradenow/升级武器说词（T-145） | merchant_dialog | @main/[~@repair] 27 处 $STR 展开 |
| 26 | `openmir2-db-encoding` | in-progress | E-7：真机 oracle 对照跑一次（建号/选角/存档的 ChrName 列值） | login_only + db_snapshot | ChrName 列值（建号/选角/存档） |
| 27 | `openmir2-login-charstatus-657` | in-progress | Rust 侧重采 657×2（重编后）与 C# 对拍；邻居 657 在观察者同 tick 其它帧之间的相对序（T-147③） | login_only | 657×2 与邻居相对序 |
| 28 | `openmir2-npc-display-table` | in-progress | 同摆位 0122 (29,33) C# vs Rust 登录抓包逐字节对拍：出现帧带名尾、3 对 nameless 转身 series 0x…；0122 点击国王出 @main 菜单；商人可点开面板（共 3 项） | login_only + npc_click | 同摆位 0122 (29,33) 出现帧/3 对 nameless 转身/点击 @main |
| 29 | `openmir2-npc-ctor-constants` | in-progress | W1 真机固定 NPC/商人首次出现帧 SM_TURN series 低=4 高=2，与 oracle 逐字节；W2 首 TurnTo 前后序列与 RNG 站点账本一致（共 4 项） | login_only | SM_TURN series 低=4 高=2、Merchant 格 30s 续期 |
| 30 | `openmir2-guard-ctor-constants` | in-progress | W-guard-1 卫士自扫半径 5→7 的同 tick 可见集/事件序；W-guard-2 bins monster_display 接 light 后帧 Series 高字节=4（共 4 项） | login_only | 卫士自扫半径 5→7 的同 tick 可见集/事件序 |
| 31 | `openmir2-script-take-persist` | in-progress | r9 原夹具复跑：角色 winplay0（PlayerId=6228），characters_bagitem Position=0 Make…；take 超额：gold=100 跑 `take 金币 200`，DB characters.Gold 必须仍 100（共 8 项） | dice_chain + db_snapshot | @main→@kaiz→@ya5→@ya52 链正是 dice_chain；比 characters_bagitem/Gold/useitems |
| 32 | `openmir2-click-path-bag-writeback` | in-progress | 真机：固定 NPC 点击（CM_CLICKNPC）触发 @main 的 `take <物品> N` 后断线，DBSvr 落库 charact…；真机：同上触发 `take 金币 N` ⇒ characters.Gold 与 oracle 一致（100→95 类）（共 4 项） | npc_click + db_snapshot | 点击 @main take/give 后断线，落库行/列比对 |
| 33 | `openmir2-login-529-status-text` | in-progress | 【依赖 SS_SERVERINFO 卡】真机 529 逐字节对拍当前不可能达成：Rust GameSvr 连上 login-srv 后不上报…；后果：Rust login-srv 的 servers 槽 server_name 恒空 ⇒ is_server_online=false…（共 4 项） | login_only | 前置 gamesvr-serverinfo-report 已合入 master 185a6c8cb（SS_SERVERINFO 上报已接）；本卡窗口=用当前二进制复跑 login_only 比 529 body |
| 34 | `openmir2-searchtick-origin-parity` | in-progress | 双栈真机同时刻：用持续到首扫之后的场景（wait_frames ≥ 登录帧数+首扫帧数，或 sleep ≥2000ms）跑 tools/co…；口径收窄（本次复核结论）：生产 ctor 与进图同刻 ⇒ 本 patch 不引入首扫时刻回归；上述窗口项实质是 T-73/T-184 的相对… | login_only（--dwell-ms ≥2000） | 抓首 SM_TURN/41 与 654/657×2 的到达 tick 窗口 |
| 35 | `openmir2-verify-tick-base` | in-progress | 登录等待环（RunNotice→CM_LOGINNOTICEOK→UserLogon）建模后，用双栈核对首个 VerifyMapTime（B…；口径收窄：今日生产路径 ctor_tick==clock_ms（代码级可证，见 same_instant_conclusion）⇒ 本 pa… | login_only | 判据实质待 T-73 登录等待环建模；今日生产路径无可观测差异 |
| 36 | `openmir2-gamesvr-serverinfo-report` | in-progress | 【必跑】双栈 529 逐字节：Rust 栈 ident=529 的 body_hex 必须逐字节 = 545479513e404342564…；【建议】5600 上行抓包：核对 (103/<服务名>/0/<在线数>/1)——第 3 段恒 0（M2Share.ServerIndex 无…（共 3 项） | login_only | 必跑 ident 529 逐字节（body_hex 对照） |
| 37 | `openmir2-ya52-menu-ok-missing` | in-progress | 用本 head 构建重跑 dice_chain_auto 末步断言 767。**残余**：N-1 T-193 末尾双句号、N-2 白名单未写承接卡号；三条未覆盖漏斗由 openmir2-me… | dice_chain_auto | 末步断言 SM_MENU_OK(767) |
| 38 | `openmir2-menu-ok-unify-pickwire` | needs-review | 按 review ba93bcb9 返工：I-1 选①抽 emit_login_script_frames 并补 @Login 生产用例 2（删排水则红）、I-2 订正 T-142 ⑧末段/… | dice_chain_auto | 767 分支；回稿后同场景复跑 |
| 39 | `openmir2-gm-impl-shutup` | in-progress | 已合入 master b8826ad0（approve reviewer-3 841be8bf，tier=R）。门禁：fmt/clippy/msgcodegen 604/test 141 s… | login_only + 手动 @禁言 | 前置 openmir2-gm-script-host-seam 已合（只通 @Login 路径） |
| 40 | `openmir2-gm-script-host-seam` | in-progress | 新角色登录帧 + **手动在客户端直呼 @禁言 <名> 1** 双侧对拍 `权限不够!!!`（探针不能靠 login_only 自动跑，需 runbook 写明） | login_only + 手动 @禁言 <名> 1 | 探针不发 GM 命令，runbook 要求操作者手输；双侧比「权限不够!!!」 |

### 1F · 第 1 批 · B：master 上既有的 `run_*_pair.ps1` 夹具（零新增场景）（13 张）

| # | 卡 id | 状态 | 窗口项要点 | 场景 | 前置 / 说明 |
|---:|---|---|---|---|---|
| 1 | `openmir2-m3-drop-exp-align` | in-progress | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核④（master 未动）：PREREQ（静态）。master 无 run_drop_exp_pair.ps1，BotSrv… | run_drop_exp_pair.ps1 | 夹具在子目录 mir2-rs/tests/parity/m3/drop-exp/（058c3ad25 有，32290B，10-16）；卡面称 BotSrv 无固定种子 100 杀驱动 ⇒ 开工前按脚本参数复核，若缺再降级 |
| 2 | `openmir2-m3-fixture-matrix` | in-progress | 剩余 = 真机 DoD**：READY 夹具需 -Side rust preflight 前置落地 + 独占窗口跑两侧同序列 diff=0；⑧-10/⑨-5 夹具（本刻待审）合入后 read… | 8 个 READY 夹具逐条 | ⑧-10/⑨-5 待审合入后 8→10 |
| 3 | `openmir2-m3-quest-persist-observability` | in-progress | 剩余 DoD = 真机两相**（任务交互后两侧 characters_quest=0 行 + characters 无任务列变更；run_pair.ps1 + scoped_diff.py… | run_pair.ps1 + scoped_diff.py --canonical | 任务交互后 characters_quest=0 行 |
| 4 | `openmir2-m3-party-attribution` | in-progress | 剩余 DoD = 真机两侧同序列 diff=0（run_party_trade_pair.ps1 + 独占窗口，归 openmir2-m3-party-trade-align），按 not_… | run_party_trade_pair.ps1 | 组队归属/分赃：前置 ⑦-2/⑦-3（卡面称待合，开工前复核） |
| 5 | `openmir2-m3-party-gainexp-npe` | in-progress | 剩余 DoD = 真机两侧同序列 diff=0（归 openmir2-m3-party-trade-align + 独占窗口），按 not_verified 登记。订正：本 burst 早前… | run_party_trade_pair.ps1 | 同上 |
| 6 | `openmir2-m3-pet-slave-lifecycle` | in-progress | 剩余 DoD = 真机两侧 run_hero_pet_pair.ps1 同序列 diff=0（召唤/诱惑/叛变/幽灵各站点，归 openmir2-m3-hero-pet-align + ow… | run_hero_pet_pair.ps1 | 脚本已在 master；前置 ⑨-2/⑨-3 实现 |
| 7 | `openmir2-m3-shop-dialog-wiring` | in-progress | 剩余 DoD = 真机两侧 run_npc_shop_pair.ps1 同序列 diff=0（详情 652 / 仓库 700-707 帧序与字节 + characters_storageit… | run_npc_shop_pair.ps1 | ⑤-4 商店派发/帧；storageitem 50 槽 |
| 8 | `openmir2-m3-pet-slave-clone46` | in-progress | 剩余 DoD = 真机两侧 run_hero_pet_pair.ps1 make_clone step 同序列 diff=0（待真机判据（窗口）），按 review not_verified… | run_hero_pet_pair.ps1 | make_clone step |
| 9 | `openmir2-m3-pet-slave-masterdie-wire` | in-progress | 剩余 DoD = ① 真机两侧 run_hero_pet_pair.ps1 主人死 step 同序列 diff=0（含叛变站点帧序/字节 + RNG 抽数 site/order）② 混合队列… | run_hero_pet_pair.ps1 | 主人死 step + 混合队列 |
| 10 | `openmir2-m3-rng-scene-driver` | in-progress | RNG 序列真机两侧对拍（W9 独占窗）。approve.window_required=true | run_rng_scene_pair.ps1 | W9 RNG 序列 |
| 11 | `openmir2-window-collector-preflight` | in-progress | 真机两侧需验证：① 起栈后 C# 流水头 INJECT_OK<seed> 与 Rust RUST_SEED_INJECTED 相等（交叉断言在真栈上首次生效）；② MIR2_RNG_SITE… | card_window.py 真栈一次（login_only） | 起栈后 INJECT_OK/RUST_SEED_INJECTED 交叉断言 |
| 12 | `openmir2-account-protection-13` | in-progress | 真机执行 account_protection_roundtrip（建号 13 列逐列 + 改资料 12 列 + PassWord 覆盖） | run_account_protection_roundtrip.ps1 | 夹具在 mir2-rs/tests/parity/m3/account-protection/（既有）；比建号 13 列 + 改资料 12 列 + PassWord 覆盖 |
| 13 | `openmir2-card-window-tool` | in-progress | 一次真 alt 剖面窗口的 card-window.json + log，要求两侧 stop rc=0、端口全释放、report_kind=compare。后续 hardening（F-1/… | card_window.py 真 alt 剖面一次 | 两侧 stop rc=0、端口全释放、report_kind=compare |

### 2A · 第 2 批 · A：需 `openmir2-window-discovery-rules` 新增规则（move / king / combat）（2 张）

| # | 卡 id | 状态 | 窗口项要点 | 场景 | 前置 / 说明 |
|---:|---|---|---|---|---|
| 1 | `openmir2-transmap-maptrigger` | in-progress | ① 真实客户端走进 MapInfo 链接格（M100A 29,81 → M100B 287,282）两侧 633/634 逐字节一致；② NEEDHOLE 目标图（Q004/R001/H00… | move_walk | move_x/move_y 需新规则（self_x/self_y + 场景声明 delta） |
| 2 | `openmir2-t138-request-castle-war` | in-progress | W-T138-1 非掌门/无行会 ⇒ SM_SYSMESSAGE(100)「你的请求被取消!!!」逐字节（Recog/Param 颜色/Ta…；W-T138-2 掌门+城主行会 ⇒ 同帧且祖玛头像不扣（共 4 项） | king_castle_war | king_actor 需新规则（643 规则 + 标签约束，待实测确认） |

### 2B · 第 2 批 · B：需**新开卡**补 `SM_BAGITEMS(201)` 发现规则（既有卡不覆盖）（2 张）

| # | 卡 id | 状态 | 窗口项要点 | 场景 | 前置 / 说明 |
|---:|---|---|---|---|---|
| 1 | `openmir2-delmagic-addmagic-chain` | in-progress | 登录后 characters_magic 行消失/新增与 212/210 真机字节与序；会话内戴上 115 即发 210 / 打碎治愈戒指即发 212（共 3 项） | bag_equip | SM_BAGITEMS(201) 发现规则**已落地**（`openmir2-discovery-bagitems-201`，构造级 `check-all` rc=0）；**剩余前置** = 非空背包的录制/夹具（背包预置尚未接进 `card_window.py`） |
| 2 | `openmir2-unbindlist-consumer` | in-progress | 真机客户端：吃一个 StdMode31/AniCount0 捆装包 ⇒ 客户端依次收到 6 条 SM_ADDITEM(200)（Recog=…；断线存档：hum_items（等价存档）含解包出的 6 件（MakeIndex 逐件递增、Dura=DuraMax=tpl.DuraMax、…（共 3 项） | eat_item | 规则同 `bag_equip`（已落地，构造级）；另需 StdMode31/AniCount0 捆装包夹具 |

### 3 · 第 3 批 · 需新帧 / 新场景 / 新夹具（含前置未清的夹具卡）（81 张）

| # | 卡 id | 状态 | 窗口项要点 | 场景 | 前置 / 说明 |
|---:|---|---|---|---|---|
| 1 | `openmir2-m3-npc-shop-align` | in-progress | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑤（master d4d5b89c 未动，与 r1/r2 相同）：仍 BLOCKED，与 r2 逐条相同。静态（00-st… | run_npc_shop_pair.ps1（前置未清） | ⑤-4/NPC 入视野 + markers 前缀口径 |
| 2 | `openmir2-m3-quest-align` | in-progress | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑥（master 未动）：仍 BLOCKED，三重阻断与 r2 相同：①markers 前缀口径（卡 openmir2-m… | run_quest_pair.ps1（前置未清） | markers 前缀 + C# k=0 截断 + quest_state 无生产者 |
| 3 | `openmir2-m3-party-trade-align` | in-progress | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑦（master 未动）：PREREQ（静态）。run_party_trade_pair.ps1 在，但 Rust bin… | run_party_trade_pair.ps1（前置未清） | Rust 侧 CM_GROUPMODE 等常量缺失（r3 实测） |
| 4 | `openmir2-m3-guild-castle-align` | in-progress | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑧（master 未动）：PREREQ（静态）。master 无 run_guild_castle_pair.ps1；实现… | run_guild_castle_pair.ps1（前置未清） | ⑧-3..⑧-9 待认领 + 时钟注入前置 |
| 5 | `openmir2-m3-hero-pet-align` | in-progress | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑨（master 未动）：PREREQ（静态）。master 无 run_hero_pet_pair.ps1；⑨-2/⑨-… | run_hero_pet_pair.ps1（前置未清） | ⑨-2/⑨-3 待认领、⑨-5 夹具待审 |
| 6 | `openmir2-m3-mail-auction-align` | in-progress | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑩（master 未动）：PREREQ（静态）。run_selloff_pair.ps1 在，但 Rust bins/ga… | run_selloff_pair.ps1（前置未清） | ⑩-1/⑩-2 待认领 |
| 7 | `openmir2-m3-quest-fixture-botsrv` | in-progress | 剩余 DoD②③ = 真机两侧同序列 diff=0（-Side rust preflight 仍 PREREQ，等 ⑥-3 落地 + 独占窗口） | run_quest_pair.ps1（前置未清） | 等 ⑥-3 + rust preflight |
| 8 | `openmir2-m3-party-trade-fixture-botsrv` | in-progress | 剩余 DoD②③ = 真机两侧同序列 diff=0（等 ⑦-2/⑦-3 落地 + 独占窗口） | run_party_trade_pair.ps1（前置未清） | 等 ⑦-2/⑦-3 |
| 9 | `openmir2-m3-selloff-fixture-botsrv` | in-progress | 剩余 DoD②③ = 真机两侧同序列 diff=0（等 ⑩-1/⑩-2 落地 + 独占窗口） | run_selloff_pair.ps1（前置未清） | 等 ⑩-1/⑩-2 |
| 10 | `openmir2-m3-shop-scenario-authority` | in-progress | 剩余 DoD = 真机**：run_npc_shop_pair.ps1 在独占窗口跑两侧同序列 diff=0（等 ⑤-4 第②片仓库存档面 + ⑤-5 / NPC 入视野），离线判据本体已完… | run_npc_shop_pair.ps1（前置未清） | 等 ⑤-4 第②片 + ⑤-5/NPC 入视野 |
| 11 | `openmir2-m3-castle-gold-tax` | in-progress | 剩余 DoD = ① 真机两侧「同一 10min SaveTick 时间门 → TodayIncome/TotalGold 落盘」diff=0（依赖 ⑧-1 C# GetTickCount… | 新：10min Save 文件 + 时钟注入 | 前置 ⑧-1 C# GetTickCount 注入 + 商人税 4 调用点接线 |
| 12 | `openmir2-m3-monster-ai-tick` | needs-review | 主动索敌（新卡，W0 前置）+ Animal RNG + 守卫静止 + 真机对拍。未 merge、未自签 review | 新：主动索敌观察（W0 前置） | 未合入当前 head（needs-review）；W0 前置 + Animal RNG |
| 13 | `openmir2-outbound-encoders-red-batch` | in-progress | 时序/RNG 面真机对拍（W0 窗或系统对拍窗）；帧序/时序真机对拍（654 的入队相对序必须真机定）；制药链的抽数流水（`Merchant.ClientMakeDrugItem` 的 Ra… | 新：R2 upgrade_retrieve | 制药链 Random 流水 + 帧相对序 + MYSTATUS 落库 |
| 14 | `openmir2-maptrigger-monster-and-door` | in-progress | W-R1-a：pin 一张「怪物路线上的 route 格」真图，两侧同驱怪物走位 ⇒ 判据=两侧怪物坐标均停在走前格（无位移帧）；W-R1-b：pin 一张「门 + ±1 相邻 route… | 新：怪物走 route / 门格 | W-R1-a/b/c 三臂 |
| 15 | `openmir2-buff-system` | in-progress | ① 三条怪癖的真机表现（`break` 跳 for(j) 的 SysMsg 与递减；递减节奏 50ms vs 1s 两读法——**设计件标为待窗口 A-R2 裁定**；怪物 buff 永不到… | 新：R5 spell_cast + DoT trace | 毒伤逐次 MIR2_RNG_LOG；buff 递减节奏待 A-R2 裁定 |
| 16 | `openmir2-buff-producer-wiring` | in-progress | 真机窗口项 + 新开片：四块 snap 改用忠实镜像 `cetin_near_xy`（复验新发现的格子层差异）+ 补窗口宽度上界牙 | 新：snap/宠物场景 | 余量只写「真机窗口项」，需先补具体判据 |
| 17 | `openmir2-default-arm-spell-surface` | in-progress | ① 真机逐帧对拍（窗口项）；② 效果体层未接（MP 扣减 + `RM_HEALTHSPELLCHANGED` 先于 RM_SPELL ⇒ 仍缺一帧、相对序前移；版本门无生产调用点；`RM_M… | 新：R5 spell_cast | 逐帧对拍；效果体层 MP/帧序仍缺 |
| 18 | `openmir2-snap-cell-fidelity` | in-progress | 真机窗口项；② 两个前置（.map 格子属性未灌进 world.maps；apply_structure_events 无生产调用方）——卡 openmir2-mapcell-attribu… | 新：mapcell + 异图 spell | 前置 mapcell 卡；异图 actorspell 不得 snap |
| 19 | `openmir2-recall-slave-shift-live` | in-progress | ① 真机窗口（W0 §2e：两侧 RNG 账本 `VERDICT IDENTICAL` + 回召落点同坐标）；② 同窗登记的 `merchant_run → map_random_move`… | 新：R5 spell_cast（回召） | 宝宝 + 异图；RNG 账本 VERDICT IDENTICAL |
| 20 | `openmir2-map-random-move-shift-loop` | in-progress | ① 真机窗口（商人到点移动：RNG 流水 + 落点）；② 登记未做：201 轮回滚的 `DeleteFromMap`/`AddMapObject` 语义（现只 return）、落格 `Cel… | 新：商人到点移动 | RNG 流水 + 落点 |
| 21 | `openmir2-magic-8-21-wiring` | in-progress | ① 真机窗口（37 的 Random(20)/Random(2) 流水 + 逐格 SM_BACKSTEP，可与 8 同场景）；② 能量排斥的「37 是否可学/可放」属数据面；③ 本卡另两项既… | 新：R5 spell_cast | 魔号 8/21/37：逐格 BACKSTEP、806 ±12 |
| 22 | `openmir2-spell-mp-wrapper` | in-progress | ① 真机窗口（失败回帧 + MP/53/尾时序）；② 6/13/14/15/16/18/19/36 的护身符/失败门未建（14/15 的 A-5 路径连 CheckAmulet 都未建）；③… | 新：R5 spell_cast | 失败回帧 639 + MP/53/尾时序 |
| 23 | `openmir2-spell-shared-tail` | in-progress | ① 真机窗口（注入时钟下 656 帧序）；② T-78 剩余三偏差；③ `vmt`（VerifyMapTime/玩家 viewer 过期/AOI 诊断）在飞 | 新：R5 spell_cast | 注入时钟下 656 帧序 |
| 24 | `openmir2-run-gap-monster-state-expiry` | in-progress | ① **真机窗口**（CrazyMode 色变 0xF9 / RefNameColor 帧面 / FireHit-TwinHit 到期消息）；② FireHit/TwinHit 写入点依赖未… | 新：world_observe 60–180s | CrazyMode 色变/RefNameColor/FireHit-TwinHit 到期 |
| 25 | `openmir2-run-gap-corpse-ghost-revive` | in-progress | ① **真机窗口**（3 分钟变尸 / 可复活支 ZenTime-20s / 5 分钟移除 / 两帧 RM_TURN）；② `ReAliveEx` 未建模面（`RecalcAbilitys`… | 新：world_observe ≥300s | 3 分钟变尸/5 分钟移除/RM_TURN×2 |
| 26 | `openmir2-movetomovingobject-quirk` | in-progress | ① **真机窗口**（多对象格移动时的源/目标格身份序列）；② `WorldGolden` 桩的怪物 CellType 仍硬编码 Play（若未来 dump 格内类型需另卡）；③ 该怪癖同样… | 新：多对象同格移动 | 源/目标格身份序列；覆盖走/跑/SpaceMove/召回 |
| 27 | `openmir2-run-gap-monster-master-follow` | in-progress | ① **真机窗口**（异图/超距/SlaveRelax 两侧流水）；② 玩家主的 `Dir` 供给另卡 `openmir2-player-dir-supply`；③ 怪物 SpaceMove… | 新：world_observe（宝宝异图/超距） | SlaveRelax/RunAway 流水 |
| 28 | `openmir2-run-gap-monster-specials` | in-progress | ① **真机窗口**（同 tick 产仔抽数相对次序、`SM_HIT` 帧面、Clone 扣 MP）；② `RM_RANDOMSPACEMOVE`/`RM_DELAYPUSHED` 投递未建… | 新：combat_hit + 产仔观察 | BeeQueen/SpiderHouse/Clone 扣主 MP |
| 29 | `openmir2-run-gap-pr1-head-blocking` | in-progress | ① 真机窗口；② 跨 actor 泵序（首次入队序 vs 实体序）；③ 宿主 `pending_mag_struck` 排水序与 7 参 `SendStruckDelayMsg` 生产者（M… | 新：队列序观察 | 跨 actor 泵序 / pending_mag_struck 排水序 |
| 30 | `openmir2-run-gap-mb1` | in-progress | ① **真机窗口**（race 101/95 各 ≥1、R38 抽样）；② `CellType.Event` 格子对象未落格；③ 通用怪 `RM_HIT` 未接；④ `MeltStoneAl… | 新：world_observe race 101/95 | 四类 Run 覆写 + R38 抽样 |
| 31 | `openmir2-monster-view-throttle` | in-progress | ① **真机窗口**；② GuardUnit 的 6 格/CanPassiveAttack；③ 自扫相位差 ≤1 tick；④ parity 驱动不启 AI；⑤ 多玩家投递序 | 新：world_observe | 怪物视野口径 + 自扫相位 |
| 32 | `openmir2-persist-useitems-wiring` | in-progress | ① **真机窗口**（耐久磨损/破零/存档往返）；② 破零后的 oracle 收尾未接（能力重算/`SendDelItems`/`FeatureChanged`/`RM_DURACHANGE… | 新：R7 equip_struck_save | 耐久磨损/破零/存档往返 |
| 33 | `openmir2-run-gap-mb3` | in-progress | ① **真机窗口**（107/105/102/112）；② **`SendStruckDelayMsg` 延时面**（归 `openmir2-rm-refmessage-delay-laye… | 新：combat_hit race 107/105/102/112 | + 排水时序 |
| 34 | `openmir2-rm-refmessage-delay-layer` | in-progress | ① **真机窗口**（200ms 后二次 `Struck` 的全局帧序）；② 未接的 7 参调用点（AttackDirect/Fireball/ExplosionSpider/DoubleC… | 新：combat_hit（200ms 二次 Struck） | 全局帧序 |
| 35 | `openmir2-active-seek-race-missing` | in-progress | ① 真机窗口；② 这些 race 的 `AttackTarget` 覆写属 `openmir2-attacktarget-overrides`；③ 118/130 数据不可达 | 新：world_observe（9 seek race） | 另接 attacktarget-overrides |
| 36 | `openmir2-proper-target-unfolded` | in-progress | ① 真机窗口（攻击已死目标的抽数流水）；② 其余折入版调用点（特殊怪覆写/显示路径）未扩大范围 | 新：combat_hit（攻击已死目标） | 抽数流水逐抽 |
| 37 | `openmir2-search-target-clear-target` | in-progress | ① 真机窗口；② 其余折入版调用点未扩大范围 | 新：world_observe / combat | ClearTarget/RemoveAt 怪癖 |
| 38 | `openmir2-attacktarget-overrides` | in-progress | 真机窗口（82/119/130/91/107/105）；114/117/118/215 不可达；`UndeadPower` 口径另卡 | 新：combat_hit（82/119/130/91/107/105 race） | 114/117/118/215 数据不可达 |
| 39 | `openmir2-rm-delaymagic-six-ids` | in-progress | 真机窗口 + T-81 登记 | 新：spell 延时帧 | T-81 六 id |
| 40 | `openmir2-rm-delaypushed` | in-progress | 真机窗口 + T-81 登记 | 新：combat_hit 延时 | T-81/T-83 |
| 41 | `openmir2-player-drop-useitems` | in-progress | 真机窗口 + T-81 登记 | 新：drop 场景（CM_DROPITEM） | T-81/T-82 |
| 42 | `openmir2-player-dir-supply` | in-progress | 真机窗口 + T-81 登记 | 新：移动/转向场景 | T-81/T-84 |
| 43 | `openmir2-run-gap-mb4` | in-progress | race 92 真机：SpaceMove/MapRandomMove 落点与帧序、狂暴 500/400 节拍；SiegeLockCount 的 CastleDoor HoldPlace 阻挡支未建模（共 3 项） | 新：world_observe race 92 | SpaceMove/MapRandomMove 落点与帧序、狂暴 500/400 |
| 44 | `openmir2-run-gap-pl1` | in-progress | P21 的 SendDelItems/SendAddItem 帧序与背包别名结果（真机对拍）；P32 复活链：HP/耐久/RM_DURACHANGE 与 DB 落库（共 4 项） | 新：combat_hit + 存档 | P21 帧序 / P32 复活链 / 53 同 tick |
| 45 | `openmir2-monster-attack-hit-frame` | in-progress | 两侧同点 (605,578)/L1/60s 各采一趟：每个受击者的 SM_STRUCK 前 ~200ms 内有同 Recog 的 SM_HI…；SM_HIT.Series 与该怪 AttackDir 的方向一致（共 3 项） | 新：combat_hit（同点 605,578 / L1 / 60s） | 14/31 相对序与 AttackDir |
| 46 | `openmir2-healthspell-emission-model` | in-progress | 整 tick 53 序列的真机对拍（尤其跨 tick G-C pending 被下一 Run 替换）；生产槽的 <68 blocker（RM_HIT=20 等）混合队列场景（共 4 项） | 新：R5 spell_cast | 整 tick 53 序列 + SKILL_SHOWHP |
| 47 | `openmir2-gategate-speed-rules` | in-progress | 登录包下发相位：oracle 入 MsgList 后到 drain 才 Send（0-210ms 不确定），Rust 仍立即下发——真机同相…；登录后 0-210ms 内未被丢的帧（如 CM_SAY）相对登录包的先后（共 4 项） | 新：login 相位场景 | 登录包 drain 相位 0-210ms |
| 48 | `openmir2-run-gap-pl3` | in-progress | 内容一旦启用（DECHP/INCHP/HungerSystem/KILLMONEXPRATE/POWERRATE）时的真机数值与帧序；197/198 的 ShowScriptActionMsg 自消息（共 3 项） | 新：R1 npc_script_chain（内容启用支） | 197/198 自消息 + G-B 跨 actor flush 序 |
| 49 | `openmir2-run-gap-pl2` | in-progress | 内容启用后的 P31 结算与 44/升级链帧序；SETAUTOGETEXP 两态在真机脚本链上的可观测行为 | 新：R1 npc_script_chain | P31 结算 / 44 升级链 / SETAUTOGETEXP 两态 |
| 50 | `openmir2-run-gap-pl4` | in-progress | 653 同 tick 相位；会话 HomeMap（GetStartPoint）四条分叉（共 3 项） | 新：R1 npc_script_chain | 653 同 tick 相位 / HomeMap 四支 / 800-801 帧序 |
| 51 | `openmir2-upgrade-npc` | in-progress | 取回两侧同种子抽数序 A(耐久)→B(Random(3))→C/D/E 逐 k diff=0；Desc[10] 字节（含 12/22/32 与 1）与客户端落库字段（共 5 项） | 新：R2 upgrade_retrieve | 抽数序 A→B→C/D/E + Desc[10] + 帧序 |
| 52 | `openmir2-castle-entities` | in-progress | 部署 A 情景：启动后 0 实体、4 条告警（Castle/Castle/SabukW.txt）；夹具 B：16 实体、24 次 Regen、8 告警、96 抽（同种子两侧）（共 5 项） | 新：castle fixture（16 实体/96 抽） | 4 条告警 + 毒攻门 + 弓手目标 |
| 53 | `openmir2-castle-guard-target` | in-progress | 同城/盟友/UnderWar 下的守卫目标选择（真机）；Castle!=null 受害者经 AnimalObject.Struck 的合法性判定（T-96⑥④，另开卡） | 新：R6 castle_zone | 同城/盟友/UnderWar 三态 |
| 54 | `openmir2-castle-war-area-impl` | in-progress | 真机 W-1..W-5：非成员进皇宫/成员/死亡/红名四臂落点与 rng 账本；766 bit4 每 tick 置位与 1s 门（共 4 项） | 新：R6 castle_zone | 766 bit4/1s 门/四臂落点 |
| 55 | `openmir2-loader-fields` | in-progress | 真机：StartPoint/Server.conf 11 键的取值与落点；接线后 game-svr 消费新加载器（当前仍用旧三列解析） | login_only（部分）+ 新加载器接线 | game-svr 仍用旧三列解析 ⇒ 新加载器真机不可观测 |
| 56 | `openmir2-mb5-monsters` | in-progress | 真机：五类怪的 Run 分支与帧（数据不可达时以构造级为准）；FrostTiger 657 帧在 host drain 接通后观察 | 新：world_observe 五类怪 | FrostTiger 657 帧待 host drain |
| 57 | `openmir2-guild-official-npc` | in-progress | 真机：建会成功/失败各支帧字节与 622 负重；宣战战表与 104 文案（共 3 项） | 新：R1 npc_script_chain | 建会成功/失败两支 + 宣战战表 |
| 58 | `openmir2-mb2-monsters` | in-progress | 真机：三怪 Run 的分支与帧（数据可达时） | 新：world_observe 三怪 | Run 分支与帧 |
| 59 | `openmir2-mb7-monsters` | in-progress | 真机：race 113/114/134 的 Run/克隆/召唤（部署数据不可达时以构造级为准）；的展示色与排水相位 | 新：world_observe race 113/114/134 | Run/克隆/召唤 |
| 60 | `openmir2-castle-pure` | in-progress | W1 沙巴克商人买/卖/修/升级后 TotalGold/TodayIncome 增量；W2 城主行会成员价（共 3 项） | 新：castle fixture（商人税三窗） | W1/W2/W3 |
| 61 | `openmir2-monster-walk-frames` | in-progress | 真机重采怪物走位 SM_WALK 条数/时序（C# 基线 17–76 条）；同 tick 多步、±12 边界、传送误判（T-146 四条残余） | 新：world_observe 60–180s | SM_WALK 条数/时序（基线 17–76） |
| 62 | `openmir2-mb8-9-monsters` | in-progress | 真机对拍 race 215 施法帧 SM_SPELL/SM_MAGICFIRE（需先接线 T-148）与抽数序；守卫/城门/城墙/NPC 的 SuperMan 打不死行为（另卡 openmir2-superman-normnpc 落地后） | 新：R5 spell_cast race 215 | 需先接 T-148 |
| 63 | `openmir2-superman-normnpc` | in-progress | 城门 110 连续受击到 HP=0：Death=false、Run 回满、无 SetMapXyFlag(2)；World::kill 直接 Die：12 项掩码、Death 仍 false；两次 kill 各发一次（共 6 项） | 新：combat_hit + GM Kill | 城门/守卫/固定 NPC 的 SuperMan 行为 |
| 64 | `openmir2-npc-def-loading` | in-progress | 点比奇国王(0122)/老人(3) 的说词面板与菜单可点性（@main 原文逐字）；@@buildguildnow/@ask/@back/@exit 的交互序列与说词/帧出队序（共 4 项） | 新：R1 npc_script_chain（国王 0122/老人 3） | npc_click 只发一次 @main，验不到 @@buildguildnow/@ask/@back/@exit 链 |
| 65 | `openmir2-merchant-race-bias` | in-progress | 真机玩家以各 HAM_* 模式攻击商人：两侧都应不命中、damage=0、无 SM_STRUCK，且该拍攻击 RNG 少一次 Random(…；NastyMode 怪与城堡守卫对商人：不进目标集（共 4 项） | 新：combat_hit（HAM_* 模式打商人） | 对照 race 50；RNG 少一次 Random(SpeedPoint) |
| 66 | `openmir2-a09-upgrade-item` | in-progress | 部署 ItemNameList 5 条在真机取同名物品，逐字节对拍 SM_BAGITEMS/SM_SAVEITEMLIST/买卖/取回/详情…；升级武器/衣服：SM_ADDITEM/SM_DELITEM/SM_UPDATEITEM 尾字段与线上 Desc 全 0（共 5 项） | 新：R2 upgrade_retrieve | ItemNameList 5 条 + 124B 尾 + 穿戴门槛 |
| 67 | `openmir2-weapon-upgrade-values` | in-progress | 真机 @upgradenow 全量对拍四字段与 657 帧；取回路径 Dura 档/n90 抽数逐抽（共 5 项） | 新：R2 upgrade_retrieve | @upgradenow 四字段 + n90 逐抽 |
| 68 | `openmir2-action-line-variable-text` | in-progress | 真机 C# 跑部署现成素材 `E:/MirServer/Mir200/Envir/QFunction-0.txt:388` 的 `SENDM…；真机 C# 跑 MESSAGEBOX `玩家:<$USERNAME>元宝:<$GAMEGOLD>`（最好再加 `<进入/@main>` 在前…（共 3 项） | 新：R1 npc_script_chain（SENDMSG/MESSAGEBOX 支） | QFunction-0.txt:388 素材 |
| 69 | `openmir2-gm-commands-design` | in-progress | 真机 SETPERMISSION=10 + 戴传送装备（T-169 前置）跑 @传送：两次调用间隔卡在 UserMoveTime*1000+…；两号（DB characters.AllowGroupReCall 分别为 1 / 0）进世界后跑 @天地合一：=1 成员必须过允许门，=0…（共 4 项） | 新：GM 特权命令场景（SETPERMISSION=10）+ 多会话 | gm_batch 的 5 条命令不覆盖本卡任何一项：7 项均需 SETPERMISSION=10 后跑 @f/@ff/@传送/@bb；第 3 项「两号 @天地合一（AllowGroupReCall 1/0）」另需第二会话 |
| 70 | `openmir2-setoffline-field-writeback` | in-progress | ① SETOFFLINE → P31：构造 QFunction-0.txt 的 @OnLogout_OffLinePlaying（`OffL…；② ISONMAP：同一窗口内验证在图上 true / 不在图上 false（部署仅 QFunction-0.txt:265 `IsOnMa…（共 3 项） | 新：脚本夹具（@OnLogout_OffLinePlaying） | SETOFFLINE/P31/ISONMAP/T-100 |
| 71 | `openmir2-setmapmode-dechp` | in-progress | ① 真机 SETMPAMODE DECHP 全量对拍：`now-DecHpTick > nDECHPTIME*1000` 秒门 + HP-=…；② SM_SYSMESSAGE(100) 逐字节：Recog=收帧玩家 ActorId、Param=MakeWord(FColor,BCol…（共 3 项） | 新：脚本夹具（SETMPAMODE DECHP） | 部署 0 处 ⇒ 必须构造 |
| 72 | `openmir2-door-chain-wiring` | in-progress | ① C# oracle 真机：部署图 `[0 比奇省]`（0.map，700×700，79 门/21 状态）对真实门格发 `CM_OPEND…；② 城堡子系统覆盖：`CastleDoor` Open/Close（`RM_DIGUP`/`RM_DIGDOWN`）期间同样无 612/61…（共 3 项） | 新：CM_OPENDOOR 场景 | 门格样例 (106,315)/(106,316)/(107,315) |
| 73 | `openmir2-castle-merchant-fixedhide-hit` | in-progress | ⑧ 接通后的城堡战实战（进入/退出战中两侧帧集合 diff=0）。**残余**：`unhide_hit` 注释「恒为客户端可见」严格说只在 !ObMode 成立（商人不可达，当前无偏差）；T… | 新：R6 castle_zone 城堡战 | ⑧ 战中帧集合 diff=0 |
| 74 | `openmir2-castle-merchant-disappear-downlink` | in-progress | ⑧ 接通后城堡战进入/退出战中两侧帧集合 diff=0。**残余**：M-1 帧序无判据（把排水挪到 hits 之前仍全绿，建议补静态/构造断言）；M-2 登录尾 bundle 与战转同 t… | 新：R6 castle_zone 城堡战 | ⑧ 战中帧集合 diff=0 |
| 75 | `openmir2-gm-admin-mode-entity-seam` | in-progress | ①-④）**：5 命令真机帧面/行为、@3 挨打零低血 53 + 登出满血存档、改档→登出→复登记录比对、@2 RM_DISAPPEAR。残余 m-1（@Rest 回执滤 Ghost）/m-… | 新：GM 5 命令场景（@3/@2 等） | gm_batch 不含这些命令 |
| 76 | `openmir2-cm-adjust-bonus` | in-progress | 真机对拍 752→52→657→252 的字节、条数与次序（安静队列前提）。残余：lib.rs:3476 旧注释 RM_ABILITY(19)/RM_FEATURECHANGED(68) 应… | 新：adjust-bonus 场景（752→52→657→252） | 需触发 RM_ADJUST_BONUS 的动作源 |
| 77 | `openmir2-weapon-durability-frames` | in-progress | 655 真机帧面（含 DecLampDura=false 的 1500→每周期一条怪癖）、1102 真机。残余：smframe_ext 帧目录两行改「已接线」；复审 N-2 提醒 merge… | 新：combat_hit + 装备耐久 | 655/1102 帧面 |
| 78 | `openmir2-sendrefmsg-range-config` | in-progress | 部署改 15 双栈三场景对拍。残余：crates/world 两处仍用常量 12（需 World 配置通道，另开卡）；oracle 缺省确认为 12 非 15 | 新：部署改 15 后三场景 | 依赖 World 配置通道（crates/world 两处仍常量 12） |
| 79 | `openmir2-map-event-frames` | in-progress | 真机双栈火墙进出 5 格比对 804/805 字节与到达窗口 + PileStones 折叠槽提示 +「采 804 前确认没挖过矿/没先建过任何事件」。残余：复审 N-a journal 坏… | 新：火墙进出 5 格场景 | 804/805 字节与到达窗口 |
| 80 | `openmir2-pickwire-remaining-frames` | in-progress | ①三条脚本帧真机逐字节；②CHANGEEXP ⇒ Recog=加完后总量（依赖 openmir2-script-changexp-exp-writeback fbd26fd8，**该卡落地前… | 新：R1 npc_script_chain | 三条脚本帧；② 依赖 changexp-exp-writeback |
| 81 | `openmir2-frame-drain-order-audit` | in-progress | 5 场景，gw-capture 可复跑）：施法多条 SM_STRUCK(31) 序 [0,2,1]/[0,3,2,1]、交易 4 条红字(252)、寄售多条 MENU_OK(**767**… | 新：5 个场景（施法/交易/寄售/组队/NPC） | gw-capture 可复跑；767 非 769 |

### 4 · 不可协议对拍：GUI / 长跑 / 离线文件日志 / 多会话收件人集合 / 工具缺口 / 非窗口欠账（35 张）

| # | 卡 id | 状态 | 窗口项要点 | 场景 | 前置 / 说明 |
|---:|---|---|---|---|---|
| 1 | `openmir2-world-bevy-migration` | in-progress | （owner 收口条目，非新交付）World→bevy_ecs **第 1 批（S1+S2）已合入 master `f5170a8e` 并达标**：合并树四门禁复跑全绿（66 suites… | — | 非窗口欠账（架构/性能）；替代=drop_bench ΔP50/P99 基线 |
| 2 | `openmir2-m2-rust-server-state-dump` | needs-review | Rust 侧 SERVER_STATE 逐击状态 dump 已交：真 server_state_gate.py 判 Rust 真实产出 GREEN rc=0 + selftest 6/6；改… | — | 非窗口欠账（工具）；server_state_gate.py 已 GREEN |
| 3 | `openmir2-m2-rust-state-log-plumb` | needs-review | Rust 栈 MIR2_RNG_STATE_LOG 管线已交：start_rust_stack.ps1 -StateLog 注入 game-svr 子进程环境 + 打 RUST_STATE_… | — | 非窗口欠账（工具）；check_state_log_plumb 15/15 |
| 4 | `openmir2-m3-scoped-diff-ability` | in-progress | 窗口 r3（master d4d5b89c 未动）真机加深复跑 ②③F-2（360 击）：attack_diff 仍 GREEN diff=0（360/157/203/6576）；scope… | — | stamina 时序量化 ⇒ 需两侧注入时钟；工具缺口（已登记不修） |
| 5 | `openmir2-m3-final-acceptance` | in-progress | 剩余**：DoD② 300 假人 24h 长跑（**等独占窗口**；turn-key 手册与就绪度包在本卡 comment）与 DoD④ 真机 `run_pair`+`scoped_diff… | — | 300 假人 24h 长跑（DoD②）+ 真机 run_pair（DoD④） |
| 6 | `openmir2-m3-clock-injection` | in-progress | 剩余 DoD = ① C# 影子 hook 半（HUtil32.GetTickCount/DateTime.Now 不可替换字段，待 owner 在 Mono.Cecil IL patch… | — | C# HUtil32.GetTickCount/DateTime.Now 不可替换 ⇒ 时钟注入工具缺口；替代=owner 裁决 + 离线等价 |
| 7 | `openmir2-m3-monster-active-seek` | in-progress | 战斗数值/时序/RNG 真机对拍（W0/W9）。approve.window_required=true | mir_click.ps1（G3 真实客户端） | GUI/夹具驱动，非协议对拍；替代=W0/W9 夹具 + G3 窗口 |
| 8 | `openmir2-orphan-encoder-wiring` | in-progress | ① 玩家/怪物走出视野时旧位置幽灵应消失（原缺口症状）；② 两真客户端间 /名字 正文 私聊双向可见；③ 离世广播收件人集合口径（本实现=visible_actors 含离开者；oracle… | — | ②/③ 需两真客户端 + ±12 收件人集合，单客户端验不到；替代=构造级收件人断言 |
| 9 | `openmir2-enter-map-cell-rejected-vs-oracle` | in-progress | ① 真机窗口（切图失败路径；当前生产无 `switch_map` 调用者 ⇒ 真机不可达）；② **帧面**：oracle 在失败前已发 `RM_CLEAROBJECTS`/`RM_CHAN… | — | 生产无 switch_map 调用者 ⇒ 真机不可达；替代=构造级 |
| 10 | `openmir2-run-gap-checkdupobj` | in-progress | ① **真机窗口**——3 人同格推挤的源/目标格身份序列 + AOI 相位（1000ms 口径）；② `player_run` 组在五批 trace 中未出现，需活栈覆盖相对次序；③ `G… | — | 3 人同格推挤 ⇒ 多会话；替代=构造级源/目标格序列 |
| 11 | `openmir2-disappear-recipients-gridscan` | in-progress | ① **真机窗口**（节流窗口内进入 ±12 的观察者、离世广播与 AOI/移动帧的全局次序）；② 冷路径 vs 缓存的既有偏差；③ `ObMode/FixedHideMode` 只发自己未… | — | ±12 收件人集合 ⇒ 需第二观察者；替代=构造级格扫集合 |
| 12 | `openmir2-db-error-reply` | in-progress | 真机窗口（失败注入的 C# 对拍） | — | 需 DB 失败注入；替代=离线注入 + 帧构造 |
| 13 | `openmir2-gategate-slots-gm` | in-progress | C# GameSvr + Rust GameGate 的 GM_CHECKCLIENT→SendKeepAlive→GM_CHECKSERV…；大于 40KB 下行不再出现 50ms 停顿（共 3 项） | — | GameGate 多进程 + GM_CHECKCLIENT/RECEIVE_OK 节拍；工具缺口 |
| 14 | `openmir2-gate-idle-timeout` | in-progress | 真实客户端挂起 60s 的被踢时序与畸形超时帧的实际反应；下行断链后网关的完整恢复路径（重连未实现，另卡） | — | 真实客户端挂起 60s；长跑/客户端驱动缺口 |
| 15 | `openmir2-dbsrv-fault-paths` | in-progress | 真机 MySQL 停机/重启下的四种降级帧与客户端表现；9 表与 characters_indexes 的跨实现逐字段 diff | — | 需停/重启 MySQL（纪律禁止）；替代=故障注入夹具 |
| 16 | `openmir2-selgate-reconnect` | in-progress | 真机 DBSrv 重启后 SelGate 的重连与在途会话；断链期间真实客户端的表现 | — | 需 DBSrv 重启；工具缺口 |
| 17 | `openmir2-guild-g3-persistence` | in-progress | 窗口 A：两个在线行会成员一方断线，对端恰收一条 104（Recog/Param/Series/GBK 尾，ShowPreFixMsg=0…；SS_208 事件一行且 payload=<会>//<名> has exited the game.（共 5 项） | — | 两在线会话一方断线 + 文件字节；多会话 + 文件 |
| 18 | `openmir2-chat-channels` | in-progress | 真机：三前缀的收件人集合与帧字节；10s 门边界（严格 >）（共 3 项） | — | 三前缀收件人集合 ⇒ 多会话 |
| 19 | `openmir2-gategate-keepalive` | in-progress | 真机：10s 健康帧周期与首帧相位；3min 窄竞态分支的可达性（需 owner 裁决）（共 3 项） | — | 10s 健康帧/15 天空闲；长跑 |
| 20 | `openmir2-dbsrv-scheme-a` | in-progress | 真机 W-DB-1..7：慢存档下的 QUERYCHR/SELCHR/LOAD 时延；5100 回包全序（共 4 项） | — | 慢存档时延与 5100 全序；工具缺口 |
| 21 | `openmir2-gate-gmclose-race` | in-progress | 已合入 master（c83a9ae24，tier=R）。无待窗口项；按 R 级规则不 closed，观察点=长跑下控制帧 flush | — | 无待窗口项（卡面自述）；建议 owner 裁决后 closed，观察点=长跑 flush |
| 22 | `openmir2-hutil32-parity` | in-progress | b1_six_replay 的 round 在 NaN/／x／≥2^31 分支上的真机对拍 | — | b1_six_replay 的 NaN/／x／≥2^31 分支；离线夹具 |
| 23 | `openmir2-encoding-crate` | in-progress | 真机：data/envir 承载的文本（行会文件/物品名/配置）逐字节；不可映射字符与 BOM 两模型的现网语料复核 | — | data/envir 文本逐字节 + BOM 两模型；离线语料 |
| 24 | `openmir2-strlist-bom` | in-progress | 真机：带 BOM 的行会/物品/列表文件读取 | — | BOM 行会/物品/列表文件；离线夹具 |
| 25 | `openmir2-openybdeal-host-wiring` | in-progress | W-SPD-1 真机掷骰：playdice 1 @cube82 → 客户端出动画（SM_PLAYDICE 8001）→ 回发 @cube82…；W-1/W-2/W-3 同前 | — | 掷骰/寄售面板与客户端动画；GUI |
| 26 | `openmir2-single-binary-roles` | in-progress | 真实 game-svr（MIR2_RNG_LOG + MIR2_RNG_SEED，startup 场景）取 trace，rng_ledger…；旧 exe 与 mir2-server game-svr 同参数启动，日志逐行一致（除 trace site 文件名） | — | RNG trace groups + 日志逐行；离线日志 |
| 27 | `openmir2-a22-castle-persistence` | in-progress | W-A22-1 文件字节：C#/Rust 各跑启动+一次 10min Save，比 List.txt / Castle/SabukW.txt…；W-A22-2 tick 时序：600000 不写、600001 写；Save 不随 10s 门（共 4 项） | — | 文件字节 sha256 + 10min Save + 跨午夜；离线文件 |
| 28 | `openmir2-monster-ref-drains` | in-progress | 真机四类帧逐字节与收件人集合（需先让 race 203/206/210/215 出现在部署数据）；Khazard 玩家目标作 sender 的 SM_SPACEMOVE_HIDE 收件人含本人（共 4 项） | — | 四类帧收件人集合 ⇒ 多会话 |
| 29 | `openmir2-npc-hit-frames` | in-progress | C# 本尊正面样本 ≥180s（同位置必须出现 ident=14 recog=国王 actor）；帧形逐字节（Recog/param/tag/series/12B 无尾）（共 5 项） | — | ≥180s 观察 + ±12 收件人集合 |
| 30 | `openmir2-sm-closehealth-wire` | in-progress | 玩家目标与怪物目标各跑一次：心灵启示 → 1100 出现 → ShowHpInterval 到点 → 1101（Recog=到期者、±12…；同 tick 有回血/扣血：玩家侧 53（优先级 68）先于 1101（171）。怪物侧注意 pre-existing 差异：oracle…（共 3 项） | — | ±12 观察者 53/1101 集合 ⇒ 多会话 |
| 31 | `openmir2-gm-batch-4-numeric` | in-progress | 接 SessionGround 数值态（不是 Entity；Entity.gold 恒 0）+ lib.rs 落用；SETPERMISSIO…；两号同时在线，比目标侧 SysMsg 与 RM_GOLDCHANGED/RM_GAMEGOLDCHANGED/RM_ADJUST_BONUS…（共 6 项） | — | T-187 两号同时在线比目标侧帧；单号可先验 T-186（gm_batch 不含 @加钱） |
| 32 | `openmir2-ybdeal-frames` | needs-review | 可达性实测 OPENYBDEAL=0/QUERYYB=0（四条部署 Envir），判部署级不可达 ⇒ 走登记路径（未硬造发射点）：T-203 登记 + 3 条用例 + 2 个真牙 + 四门禁… | — | 部署 OPENYBDEAL=0/QUERYYB=0 ⇒ 部署级不可达；替代=登记 + 构造用例 |
| 33 | `openmir2-sm-reconnect-wiring` | in-progress | 真机换服/重连帧面 + GateCount==1 的 RNG 流水。残余：M-2 nServerIndex 源偏差（G11 落地时改为 ChangeSpaceMove 目标图）、OffLin… | — | 换服/重连帧面；重连未实现 + 工具缺口 |
| 34 | `openmir2-monster-showhp-53-broadcast` | in-progress | ShowHp 怪/NPC ±12 帧集合、53/1100/1101 同 tick 序、ObMode 源自见 2 条、VisibleHumanList 500ms 缓存。接管说明：原 owne… | — | ±12 帧集合 + VisibleHumanList 缓存 ⇒ 多会话 |
| 35 | `openmir2-sm-skeleton-wiring` | in-progress | 双栈同一账号连续屠宰「鹿(150)/鸡(50)」直到出骨架，判 SM_SKELETON(33) 头+8B CharDesc 逐字节、收件人集合（±12 格 Race==Play && !Gh… | — | ±12 收件人集合 + 12 格边界各放静止号 ⇒ 多会话 |

## 4. 抽样复核（10 张，逐卡读了线程原文）

方法：`walgit collab thread <id>` 取 `review.window_items` 与最后一条 `status.work` 原文，逐条判断所选场景**能否产生该判据所需的观察面**。

| # | 卡 | 原文要点（节选） | 我给的场景 | 结论 |
|---:|---|---|---|---|
| 1 | `openmir2-script-take-persist` | 「点 @main→@kaiz→@ya5→@ya52 后断线存档…characters_bagitem Position=0 MakeIndex=70000001 StdIndex=44；同一夹具对 C# 复跑」 | dice_chain + db_snapshot | **能**。`dice_chain.json` 的 CM 序列逐字就是 @main→@kaiz→@ya5→@ya52；db_snapshot 能拍到该行。但「金币不足/GIVE 满包」两项需要另造脚本夹具（行内已写）。 |
| 2 | `openmir2-click-path-bag-writeback` | 「固定 NPC 点击（CM_CLICKNPC）触发 @main 的 take <物品> N 后断线，DBSvr 落库 characters_bagitem 行数/字段」 | npc_click + db_snapshot | **能**。npc_click 发的正是 CM_CLICKNPC；断线存档由驱动器收尾 + 停栈后的 db_snapshot 覆盖。 |
| 3 | `openmir2-npc-def-loading` | 「点比奇国王(0122)/老人(3)…@@buildguildnow/@ask/@back/@exit 的交互序列与说词/帧出队序」 | （不入第 1 批）新 R1 npc_script_chain | **不能**用 npc_click 验：它只发一次 CLICKNPC 等 643/100，不会发后续 `dlg_select`。需新场景支持多步标签链 + 两个指定 NPC（0122、3）。⇒ 第 3 批。 |
| 4 | `openmir2-orphan-encoder-wiring` | 「两真客户端间 /名字 正文 私聊双向可见」「离世广播收件人集合…oracle=±12 格扫描」 | （不可对拍） | **单客户端验不到**：② 要两个会话；③ 要 ±12 的第二观察者。⇒ 不可协议对拍，替代判据=构造级收件人断言。 |
| 5 | `openmir2-sm-skeleton-wiring` | 「收件人集合（±12 格 Race==Play && !Ghost，12 格边界各放静止号）」 | （不可对拍） | **单客户端验不到**：12 格边界要放静止号，至少 4 个额外会话。⇒ 不可协议对拍。 |
| 6 | `openmir2-monster-ref-drains` | 「真机四类帧逐字节与收件人集合」「SendRefMsg 500ms 缓存支未建模的收件人差异」 | （不可对拍） | **单客户端验不到**收件人集合；且 race 203/206/210/215 需先出现在部署数据。⇒ 不可协议对拍。 |
| 7 | `openmir2-cm-adjust-bonus` | 「真机对拍 752→52→657→252 的字节、条数与次序（安静队列前提）」 | （第 3 批）新 adjust-bonus 场景 | **9 个场景都不触发 RM_ADJUST_BONUS**（没有 GM/脚本动作源）；`gm_batch` 也不含。需新场景。 |
| 8 | `openmir2-login-529-status-text` | 「先开 game-svr 卡接上 SS_SERVERINFO 上报，再复跑对拍台 login_only 的 529」 | login_only | **能**。前置 `gamesvr-serverinfo-report` 已合入 master `185a6c8cb`（SS_SERVERINFO 上报已接），当前二进制应已能采到 529；窗口只需复跑 login_only 比 body_sha256。 |
| 9 | `openmir2-frame-drain-order-audit` | 「施法多条 SM_STRUCK(31) 序、交易 4 条红字(252)、寄售多条 MENU_OK(767)、组队散组、npc 说词」 | （第 3 批）5 个新场景 | **不能**：5 条判据分别要施法/交易/寄售/组队/多实例 NPC 五类主动操作，9 个场景一个都不覆盖。 |
| 10 | `openmir2-gm-script-host-seam` | 「新角色登录帧 + 手动在客户端直呼 @禁言 <名> 1 双侧对拍 权限不够!!!（探针不能靠 login_only 自动跑）」 | login_only + 手动步骤 | **部分能**：login_only 负责新角色登录段；@禁言 必须操作者手输（runbook §2 已要求）。若漏手动步骤不会报错，只会看起来没红——已在行内标注。 |

**抽样结论**：10 张里 4 张（脚本链/点击路径/529/GM 手动）确实能用所选场景验到，6 张按原判据**单客户端或单场景验不到**，已分别改判第 3 批或不可对拍。特别地，凡是原文出现「收件人集合」「±12」「两个在线成员」「两号同时在线」的卡，一律不进第 1/2 批——这是本次复核最有价值的一条筛选规则。

> **I-1 规则反查（第 1 批全部 53 张）**：命中 2 张——`openmir2-gm-commands-design`（7 项窗口项**没有一项**由 `gm_batch` 覆盖：全需 SETPERMISSION=10 后跑 @f/@ff/@传送/@bb，第 3 项「两号 @天地合一」另需第二会话 ⇒ **整卡移第 3 批**）；`openmir2-player-searchviewrange-cadence`（项 ① 可闭环；项 ③ RM_DISAPPEAR 收件人近似需第二观察者 ⇒ **标部分可验**）。⇒ 第 1 批「可闭环」是 52 张，不是 53 张。

> 抽样还发现**卡面结论过时** 5 张（按 058c3ad25 逐项复核）：`m3-guild-castle-align`、`m3-hero-pet-align`（`run_*_pair.ps1` 在）、`m3-mail-auction-align`（bins 有 `CM_SELLOFFADDITEM`）、`m3-party-trade-align`（bins 有 `CM_GROUPMODE/CM_CREATEGROUP`）、`m3-drop-exp-align`（`run_drop_exp_pair.ps1` 在 `m3/drop-exp/` 子目录，32290 字节，10-16 已有；卡面「master 无该夹具」过时 ⇒ 移第 1 批 B 组）。另 `login-529` 的前置 SS_SERVERINFO 已在 `185a6c8cb`。**唯一不成立的是 `m3-quest-align`**：其三条阻断（markers 前缀未认领 / C# k=0 JSON 截断 / quest_state.jsonl 无生产者）仍全部成立。**坑**：`run_*_pair.ps1` 散落在子目录（`drop-exp/`、`rng-scene/`、`rng-trace-scene/`），查夹具要用 `find mir2-rs/tests/parity/m3 -name "run_*pair*.ps1"`，只看 `m3/` 顶层会漏（reviewer 与我都踩过）。**开窗前仍应重跑前置检查，不要照抄卡片旧结论。**

## 5. 缺口登记（不改场景、不改工具，只登记）

### 5.1 需新发现规则 → 已有卡

| 缺口 | 覆盖场景 | 卡 |
|---|---|---|
| `move_x/move_y`（self_x/self_y + 场景声明 delta） | `move_walk` | `openmir2-transmap-maptrigger` |
| `king_actor`（643 规则 + 标签约束） | `king_dialog` / `king_castle_war` | `openmir2-t138-request-castle-war` |
| `target_x/target_y/hit_dir`（combat） | `combat_hit` | 第 3 批 20+ 张（施法/命中/怪物观察类） |

> 以上三项全部指向 `openmir2-window-discovery-rules`（仍 open）。按该卡口径，combat 若实测无可靠帧来源，应**如实登记为「需新帧」而不是硬做**——本清单据此把 combat 类卡同时列在第 3 批的前置里。

### 5.2 需新开卡（现有卡不覆盖）

| 缺口 | 影响卡 | 建议 |
|---|---|---|
| ~~**`SM_BAGITEMS(201)` 发现规则**~~（`make_index`/`item_name`/`takeon_where`） | `delmagic-addmagic-chain`、`unbindlist-consumer`，以及一切 `bag_equip`/`eat_item` 场景 | **已落地**：卡 `openmir2-discovery-bagitems-201`（`param_discovery.py` 的 201 规则族 + 合成帧夹具；`bag_equip`/`eat_item` 在合成 201 帧上 `check-all` rc=0）。**剩余前置**：真实录制里至今**没有一条 201**（`E:/tmp` + `E:/MirServer` 下 798 份 `frames.ndjson` 实测 0 命中）⇒ 需要一次**非空背包**的 discovery run / 背包 DB 夹具，见 `window-runbook §4.5.1` |
| **多会话/多观察者驱动器**（≥3 会话、可摆放 ±12 格边界静止号） | 第 4 批里 11 张收件人集合类 | 需新开「窗口多会话探针」卡；否则这 11 张永远只能构造级 |
| **C# 时钟注入**（HUtil32.GetTickCount / DateTime.Now） | `m3-clock-injection`、`m3-castle-gold-tax`、`m3-scoped-diff-ability` | 已在 `m3-clock-injection` 卡上，需 owner 裁决三选一 |
| **DB 故障注入**（停机/半失败/慢存档） | `db-error-reply`、`dbsrv-fault-paths`、`dbsrv-scheme-a` | 需新开故障注入夹具卡（纪律禁止真停 mysqld） |

### 5.3 需新场景（第 3 批聚合，按模板归并）

| 模板 | 覆盖卡数 | 说明 |
|---|---:|---|
| R1 `npc_script_chain`（多步标签链） | 9 | `action-line-variable-text`、`npc-def-loading`、`guild-official-npc`、`run-gap-pl2/pl3/pl4`、`pickwire-remaining-frames`、`setoffline-field-writeback`、`setmapmode-dechp` |
| R2 `upgrade_retrieve` | 4 | `weapon-upgrade-values`、`upgrade-npc`、`a09-upgrade-item`、`outbound-encoders-red-batch` |
| R3 `combat_strike` | 12+ | `merchant-race-bias`、`superman-normnpc`、`mb8-9-monsters`、`monster-attack-hit-frame`、`run-gap-pl1/mb1/mb3/mb4`、`attacktarget-overrides`、`proper-target-unfolded`、`rm-refmessage-delay-layer`、`weapon-durability-frames` 等 |
| R4 `world_observe` | 8+ | `npc-hit-frames`、`monster-walk-frames`、`mb2/mb5/mb7-monsters`、`run-gap-monster-state-expiry`、`run-gap-corpse-ghost-revive`、`monster-view-throttle` |
| R5 `spell_cast` | 6+ | `magic-8-21-wiring`、`spell-mp-wrapper`、`spell-shared-tail`、`healthspell-emission-model`、`default-arm-spell-surface`、`recall-slave-shift-live` |
| R6 `castle_zone` | 4 | `castle-war-area-impl`、`castle-guard-target`、`castle-merchant-fixedhide-hit`、`castle-merchant-disappear-downlink` |
| R7 `equip_struck_save` | 1 | `persist-useitems-wiring` |
| 其它单卡新场景 | 30+ | 见 §3 第 3 批行内「新：…」 |

> 模板复用 ≠ 判据复用：每张卡的等待集与断言不同，**一卡一次 run**（与 `window-debt-reclass` §5 口径一致）。

### 5.4 第 4 批里「其实不是窗口欠账」的 5 张

| 卡 | 原因 |
|---|---|
| `openmir2-world-bevy-migration` | 架构/性能卡，无 tier，余量是迁移冻结条件；替代判据=drop_bench ΔP50/P99 |
| `openmir2-m2-rust-server-state-dump` | 工具卡，`server_state_gate.py` 已 GREEN |
| `openmir2-m2-rust-state-log-plumb` | 工具卡，`check_state_log_plumb` 15/15 |
| `openmir2-gate-gmclose-race` | 卡面自述「无待窗口项」；建议 owner 裁决后 closed |
| `openmir2-ybdeal-frames` | 部署级不可达（OPENYBDEAL=0/QUERYYB=0），走登记路径 |

这 5 张按口径仍在「merged-not-closed」集合里，但不是真机窗口能消掉的欠账；把它们从 173 里扣掉，**真正要靠窗口消掉的约 168 张**。

## 6. 复现（只读，零起栈）

```sh
# 1) 导出全部协作条目（与 walgit collab thread 同源；6793 条 / 429 线程）
git for-each-ref refs/collab/inbox --format="%(objectname)" > /e/tmp/oids.txt
git cat-file --batch < /e/tmp/oids.txt > /e/tmp/entries.batch
```

```py
# 2) python -I 以下脚本，复算 §1 的全部数字
import json
from collections import Counter
raw = open("E:/tmp/entries.batch", "rb").read()
es, i = [], 0
while i < len(raw):
    j = raw.find(b"\n", i)
    hdr = raw[i:j].decode().split()
    i = j + 1
    size = int(hdr[2]); es.append(json.loads(raw[i:i+size])); i += size + 1
g = {}
for e in es: g.setdefault(e.get("id", ""), []).append(e)
def last(xs, kind, t=None):
    xs = [e for e in xs if e.get("kind") == kind and (t is None or e.get("ts", 0) <= t)]
    xs.sort(key=lambda e: (e.get("ts", 0), e.get("sig", "")))
    return xs[-1] if xs else None
def approved(e):
    return (e["body"].get("verdict") or e["body"].get("decision") or "").lower() in ("approve", "approved")

T = 1792733280  # master f7a32c2f, 2026-10-23 13:28 +0800
# (a) 114 的出处：时点重建后按字面 grep
snap = {}
for tid, xs in g.items():
    s = last(xs, "status", T); q = last(xs, "issue", T)
    snap[tid] = ((s["body"].get("work", "") if s else "") +
                 ((q["body"].get("title", "") + q["body"].get("prose", "")) if q else ""))
for kw in ["已合入 master", "窗口项", "window_required", "窗口", "窗口欠项"]:
    print("SNAP", kw, sum(1 for v in snap.values() if kw in v))

# (b) 欠账集 173
pop = {}
for tid, xs in g.items():
    mr = [e for e in xs if e.get("kind") == "merge_result" and e["body"].get("merged") is True]
    st = last(xs, "status")
    if not mr or (st and st["body"].get("status") == "closed"):
        continue
    ap = [e for e in xs if e.get("kind") == "review" and approved(e)]
    ap.sort(key=lambda e: (e.get("ts", 0), e.get("sig", "")))
    tier = ap[-1]["body"].get("tier") if ap else None
    anyR = any(e["body"].get("tier") == "R" for e in xs if e.get("kind") in ("review", "issue"))
    if tier == "L" or (tier is None and anyR):
        continue
    pop[tid] = tier
print("POP", len(pop), Counter(pop.values()))

# (c) 114 → 173 的桥
A = {t for t, v in snap.items() if "已合入 master" in v}
print("A", len(A), "A∩P", len(A & set(pop)), "A−P", len(A - set(pop)), "P−A", len(set(pop) - A))
```

> 逐卡「场景 / 批次 / 前置」是人工判断（本文件 §3），不是脚本生成；判断依据逐卡写在行内，抽样复核见 §4。生成期用于覆盖断言的辅助脚本留在 `E:/tmp/w66/`（不入库）。

## 7. 与本卡验收的对应

| 验收项 | 落点 |
|---|---|
| 逐卡一行（id/状态/要点/场景/批次/前置） | §3（173 行） |
| 分批原则写清楚 | §0 表 + §2 场景能力表 + §3 各节标题 |
| 每张卡必须落到某一批或不可对拍 | §3 覆盖 173/173（生成期脚本断言 missing=0，见 §6 脚注） |
| 解释 114 的差额 | §1（70 closed + 23 L + 21 真欠账；+149 措辞不同 +3 快照后合入） |
| 抽样复核 ≥8 张 | §4（10 张，逐卡读线程原文） |
| 缺场景只登记不改 | §5（规则缺口指向已有卡；新开卡建议 4 条） |

## 7. 2026-10-24 刷新：哪些现在能跑、哪些跑了也没用（卡 `openmir2-window-batch-plan-refresh`）

**快照戳**：`walgit/master = 38ffb0bfe`（2026-10-24）。本节所有「现算」数字都可用下面两条命令复现；引用别处数字的，均标注来源。

```sh
python -I tools/goal/goal_gate.py --json
python -I tools/conformance/preflight.py --profile alt   # 开窗前五步自查（本日新增）
```

### 7.1 三类事实（今晚合入，改变批次可行性）

| 类别 | 事实 | 证据 |
|---|---|---|
| **新规则已合** | 7 条被 `recog` 掩码吞掉的规则、`202/203/642/709` 补登记、`SM_STRUCK(31)` 窄掩码 | `b42f20a8` / `6c2da8bf` / `57302d3df`；`tools/conformance/rules.json` 现含 ident 31/201/202/203/642/709（本条为**实测算**：脚本 enumerate 42 条规则） |
| **新场景已合** | `upgrade_retrieve`、`equip_struck_save`、`spell_cast`、`combat_strike`、`combat_hit`（改 blocked） | `tools/probe/scenarios/` 现 **20** 个场景（实测算）；其中 3 个声明 `blocked`（见 7.3） |
| **新夹具已合** | `characters_magic` 预置、背包 `inventory` 预置、BLOCKED 硬门、`-WhatIf` sha/构建类型 | `512d9ecf5`（magic）、`openmir2-window-inventory-fixture`（inventory）、`8dac04eb0`（blocked 门）、`821492abf`（whatif） |

### 7.2 旧批次（§3.1P / 1F / 2A / 2B / 3 / 4）怎么变

- **2A（需新发现规则）**：规则已合 ⇒ 其中依赖 `202/203/642/709` 的条目**不再需要第 2 批**，可直接进「现在能跑」。
- **2B（需 `SM_BAGITEMS(201)` 规则）**：规则已在 `rules.json`（ident 201），且背包夹具已接 ⇒ 该批场景可跑（声明 `inventory` 即可）。
- **3（需新帧/新场景/新夹具）**：`equip_struck_save`、`upgrade_retrieve` 两场景**规则面已解**；`spell_cast` 仍因 BLOCKED 不可跑（缺 Rust 魔法 + 第二观察者，见 7.3）。
- **1P/1F**：不受影响；但注意 `combat_hit` 已从「可跑」移入「跑也没用」（7.3）。
- **4（不可协议对拍 35 张）**：本卡不改其口径；其中多会话收件人集合类仍等 `openmir2-multi-session-probe` slice-2。

### 7.3 「跑也没用」清单（BLOCKED / 不可达）

| 场景 | 状态（本 commit 实测） | 覆盖卡数 | 缺什么 |
|---|---|---:|---|
| `combat_hit` | **`blocked`**（本卡 `f56a391db` 合入）：攻击者收不到自己的 `SM_HIT(14)` | 引用 5 行 T（`T-242/249/251/253/255`，实测算） | 同屏第二会话或同格怪物夹具 |
| `combat_strike` | **`blocked`**（`2a69cd6e0`）：无满足条件的配装，且 DSL 无 recog 过滤 | 引用 **13** 张（源：`docs/window-debt-reclass-20261023.md` §R3，经 `docs/goal-snapshot-late-20261023.md:245` 转引） | `DC_min≥36..60` 且 `HitPoint≥17..34` 的配装，或 `wait_for` 的 recog 过滤 |
| `spell_cast` | **`blocked`** | 引用 **8** 张（源：卡 `openmir2-scenario-spell-cast` issue 的 R5 口径） | Rust 侧魔法落点 + 第二观察者；`characters_magic` 夹具已在位（`512d9ecf5`） |
| `castle_zone`（R6） | **不可达登记，仓库内无该场景文件**（实测算：`tools/probe/scenarios/` 无 castle_zone） | 引用 **4** 张（源：`docs/goal-snapshot-late-20261023.md:246`、`docs/castle-zone-unreachable-20261023.md`） | 双行会 DB + 私有根 `AttackSabukWall.txt` + 20 点开战钟点 + 多会话探针 |

> **不再在「跑也没用」里**：`upgrade_retrieve`、`equip_struck_save`——它们的 202/203/642/709 规则已补（7.1），原先「开窗必 UNDECIDABLE」的结论**已过期**（旧口径见 `docs/runbook-tool-consistency-audit-20261023.md` §3.1）。

### 7.4 「现在就能跑」最小清单（现算）

删掉 3 个 blocked 后，仓库内可用场景 **17** 个（20 − 3）；castle_zone **不在**这 20 个文件里（它是不可达登记，见 7.3）；其中带 `required` 的场景由 `card_window` 先做 discovery（两侧各自推导、推不出即 fail-closed）。下表「引用 T 行」是 `whitelist.md` 里**点名该场景行数**的现算值：

| 场景 | `required` | 引用 T 行（现算） | 跑前还需 |
|---|---|---:|---|
| `login_only` | — | 14 | 角色预置位（场景可声明 `placement`） |
| `pickup_item` | `self_x/self_y`（SM_LOGON 推导） | 0（被 T-211 等泛引用） | 同格地物 |
| `move_walk` | `move_x/move_y/move_xy`（`computed.pack`，本日已支持） | 0 | 空格目标格 |
| `bag_equip` / `eat_item` | `make_index/item_name(/takeon_where)`（201 推导） | 3 / 1 | 背包 `inventory` 声明（本日已接） |
| `equip_struck_save` | —（placement 内建） | 1（T-247） | 装备受击存活路径 |
| `upgrade_retrieve` | `merchant_actor`（643 推导） | 0 | 商店 NPC 摆位 |
| `dice_chain` / `dice_chain_auto` | — / — | 3 / 2 | 赌场 NPC |
| `npc_click` / `npc_click_auto` | `npc_actor` | 1 / 0 | 场景 `placement` |
| `king_dialog` / `merchant_dialog` / `npc_script_chain` | `king_actor` / `merchant_actor` / `npc_actor` | 0 / 1 / 1 | 场景 `placement` |
| `king_castle_war` | `king_actor` | 0 | **城堡战语境**（UnderWar 面见 7.3 的 castle_zone 缺口，跑前先确认语境） |
| `gm_time` / `gm_batch` | — | 0 / 0 | 无 |

### 7.5 判据口径（写给开窗的 owner）

- 「能跑」= 场景不被 `blocked` 拒 + `required` 可发现 + 两侧栈起得来；**判据仍看各卡 T-* 行**，本表只解决「跑谁」。
- 开窗前先跑一条命令：`python -I tools/conformance/preflight.py --profile alt`（runbook §0.5）。
- 107/178/173 这类历史量**不要再用**：本节所有动态量以 `goal_gate --json` 现取为准（快照戳：`38ffb0bfe`）。

