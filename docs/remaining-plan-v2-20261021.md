# 剩余实现批次计划 v2（OpenMir2，2026-10-21）

- **卡片**：`openmir2-remaining-plan-v2`（tier=L，纯文档 + 只读）
- **基线**：`master = 1915b67a5`（本 worktree 建树时 HEAD；下文所有「当前状态」均以此为准）
- **写集**：仅本文件（`docs/remaining-plan-v2-20261021.md`）。未改 `mir2-rs/src/**`、`mir2-rs/bins/**`、
  `mir2-rs/crates/**`、`sql/**`、`MirClient/`、`E:/MirServer/**`；未起服务端/客户端/代理；未占端口；
  未写数据库；未动主检出；未 push/合并/master；未运行 cargo。
- **输入**（全部只读）：
  1. `docs/m3-remaining-inventory-20261022.md`（A/B/C/D 四表，写作基线 `c83a9ae24`）；
  2. `docs/window-run-sheet-20261022.md`（v3/v3.1，窗口计划件）；
  3. `docs/window-round2-runbook-20261023.md`（2026-10-22 实跑复盘的下一轮配方）；
  4. `mir2-rs/tests/parity/m3/design-{castle-official,castle-tax-and-item-name,market-def,gm-commands}-*.md`；
  5. walgit 看板（`walgit collab board --format json`，本文件写作时快照：309 卡 = 165 in-progress /
     143 closed / 1 open）。
- **口径**：
  1. 本文件**取代** `docs/m3-remaining-inventory-20261022.md` 的 A 表排期结论；B/C/D 表在该文件仍有效，
     本文件只更新其中已被本周提交改变的行。
  2. 「已完成」以**源码复核 + 合并提交**为准，不看板面文字。看板 `work` 字段已知 stale（例：
     `openmir2-m3-drop-exp-align` 仍写「master 无 run_drop_exp_pair.ps1」，而该文件在 `1915b67a5` 已存在）。
  3. 判级沿用 `AGENTS.md`「判据分级」；**判不准按 R**。R 卡可离线实现、可合入，但 `merge_result` 后
     `status ≠ closed`，窗口未绿不得收卡。
  4. 不把「部署不可达」读成「无需实现」，但也不为不可达面排窗口（见 §5 O-13）。

---

## 0. 结论摘要（先看这 10 条）

1. **库存基线与当前 master 已差 59 个提交**（`c83a9ae24..1915b67a5`，54 个生产文件变更，
   +10656/−3817 行）。A 表 22 条里 **6 条已交付/部分交付**、**3 条在飞**；真正待实现的是 **13 条**、
   A-18 的 5 个编码分项，以及 **4 条新登记宿主残余**（T-134/T-135/T-137/T-142）。
2. **本周交付（从待办删除）**：GuildOfficial NPC 标签（`0888fea97`/`2c0ea9bf0`/`f9b9b5f1a`）、
   MB-5（`b382a5876`/`2fa927175`）、MB-6（`00a039ee9`/`bfd5d1e2c`）、编码 crate 与 E-1/E-2/E-4
   （`22a2ff847`/`ba82032f4`/`a264e32b4`/`265fc19da`/`e88382c2f`/`f07eb5a97`）、
   PlayDiceLabel/OPENYBDEAL 引擎（`1495fd4b2`/`13769d591`）、数据加载器
   （`b38d28bca`/`fac227ddd`/`552303cab`）、HUtil32（`d84848a4a`/`a8eff0ef2`/`e6ddc9fb7`）、
   登录尾 654/第二 42/100×4（`79c6de691`/`547ba2a99`/`1915b67a5`）。
3. **下一批 3 个车道（互不冲突写集）**：P1 `castle-pure`（`crates/world` 城堡纯层 + 商人城堡下标）、
   P2 `db-encoding`（`bins/db-srv` 的 E-3 + E-7）、P3 `fixtures-window-prep`
   （`mir2-rs/tests/parity/m3/**` + `tools/**`）。三者与当前在飞的
   `bins/game-svr/**`、`crates/{envir,script,data}/**`、`crates/world/src/combat.rs`、`monai.rs` 零交集。
4. **`bins/game-svr/**` 是全局瓶颈**：Market_Def 车道（`wt-marketdef2`）未让出前，A-17 怪物 `SM_WALK`、
   T-142 宿主接线、T-134/T-135 排水、A-02/A-03 宿主、A-06 GM、A-09 物品名、A-10/A-21 全部只能排队。
   让出后按 **H1 → H2 → H3** 单线串行，**H1 先做 A-17**（当前客户端看不到怪物移动，是最大的生产可见缺口）。
5. **窗口前置仍是三红（2026-10-21 01:12 实测）**：
   - **R6 RED**：镜像 `/e/tmp/m2svr-window-origin` HEAD=`4a887354e`，落后 master `1915b67a5` **32** 个提交；
   - **R7 未绿**：`/e/tmp/m2svr-window-target/window-provenance.json` 的 `source_head=4a887354e`（旧基线）；
   - **R8 RED**：`port-status.ps1` 报**十个端口全部监听**（5000/5100/5500/5600/5700/6000/7000/7100/7200 + 3306）。
6. **E1/E4 已有明确 owner 裁决**（E1=测试库克隆、E4=允许重编）；**E2 有裁决记录但书面矛盾**；**E3（MySQL 停起）仍待裁决**。
   E2 存在**书面记录矛盾**：run sheet §E-0 记「2026-10-21 owner 会话『E2 可以』」，而 runbook §4 注记
   `b46805058` 仍记「E2/E3 待裁决」⇒ 见 §5 O-11，需要 owner 一行确认后才能开行会 750/G3b/G3c 窗口。
7. **本轮窗口只有 4 个块可收**：D.1（G0+G1）、D.2（G2a）、D.3（G3a）、D.4（G5a）。
   G4/G6/G7/G8/G9/G10/G11 在本文件写作时**不具备收口条件**（工具/授权/内容/独占时长）。
8. **最高价值的生产可见缺口**（按用户可见度，不按 tier）：
   A-17 怪物移动帧完全不下发 ＞ T-142 `PlayDiceLabel` 宿主未接（赌场脚本生产可达）＞
   T-137 固定 NPC `Npc_def` 说词未装载（点国王无菜单）＞ A-09 物品名/升级差量 ＞ T-134/T-135 排水。
   A-02/A-03/A-22 城堡面在 stock 部署下**不可达**（无城主/无攻击者），只做构造级，不应占窗口。
9. **A-09 的 tier 有冲突**：设计件 `design-castle-tax-and-item-name-20261022.md §2.10` 判 **L**，
   库存 A 表判 **R**（同族 T-95/B-39 均 R）。按 `AGENTS.md`「判不准按 R」⇒ 本计划按 **R** 排，
   实现可在离线完成，收卡等窗口。
10. **R 卡合入 ≠ 收卡**。本周新增的 R 卡（guild NPC、MB-5/6、encoding crate、login 尾、scriptpd、
    account-protection、MB-7、market-def）都已在/将在 master 上，但看板仍是 `in-progress`，
    这是制度不是遗漏（`AGENTS.md` 判据分级）。

---

## 1. 基线与只读复核

### 1.1 提交差与看板

```sh
cd /e/tmp/wt-planv2
git log --oneline c83a9ae24..1915b67a5 | wc -l        # 59
git diff --stat c83a9ae24..1915b67a5 -- mir2-rs/bins mir2-rs/crates | tail -1
# 54 files changed, 10656 insertions(+), 3817 deletions(-)
walgit collab board --format json                      # 309 卡：165 in-progress / 143 closed / 1 open
```

### 1.2 源码复核锚点（行号以 `1915b67a5` 为准，不沿用旧文档）

| 面 | 复核命令 | 当前读数 |
| --- | --- | --- |
| A-03 税丢弃 | `grep -n "let _tax" mir2-rs/bins/game-svr/src/shop.rs` | `937`/`1042`/`1217` 三处仍丢弃 |
| A-08 Market_Def | `grep -n "Market_Def 商人脚本未装载" mir2-rs/bins/game-svr/src/main.rs` | `4640`/`4680`/`4798` 仍只登记 |
| A-10 SETOFFLINE | `grep -rn "fn set_offline" mir2-rs` | 仅 `crates/script/src/engine/mod.rs:569` 默认实现；`NpcPlayer` 未覆写 |
| A-21 sys_msg | `grep -rn "fn sys_msg" mir2-rs` | 仅 `crates/script/src/engine/mod.rs:350` 空默认；`NpcPlayer` 未覆写 |
| A-17 怪物走帧 | `grep -n "fn walk_to" -A10 mir2-rs/crates/world/src/monai.rs` | `141-149` 只改 `e.x/e.y`，不产事件 |
| T-134/T-135 排水 | `sed -n '13632,13646p' mir2-rs/bins/game-svr/src/main.rs` | `drain_special_ref_msgs` 只处理 `RM_HIT`，其余 ident `continue` |
| A-04 13 列 | `grep -n "INSERT INTO account_protection" -A3 mir2-rs/crates/storage/src/account.rs` | `251-252` 13 列中 10 列写空串；`365` UPDATE 仍只 2 列 |
| A-02 纯层 | `grep -n "pub fn castle_official_action" mir2-rs/crates/world/src/castle.rs` | `510` 已有；宿主无派发 |
| A-22 落盘 | `grep -n "SaveAttackSabukWall" mir2-rs/bins/game-svr/src/main.rs` | `17656` 仅注释/日志，无排水 |
| A-03 城堡下标 | `grep -n "castle_index" mir2-rs/crates/world/src/merchant.rs` | 无该字段（只有 `castle_merchant`/`castle_under_war`） |
| T-137/T-138 | `grep -nE "T-13[78]" mir2-rs/bins/game-svr/src/guild_official.rs` | `453`/`612`/`615` 明写未接 |

### 1.3 已知 stale（写作时不得采信）

- `openmir2-m3-drop-exp-align` 的 `work` 写「master 无 `run_drop_exp_pair.ps1`」，实际该文件
  （543 行）与 BotSrv 固定种子 100 杀驱动已在 `c83a9ae24` 前存在；缺口只是「0137 上 pin 单只固定怪」。
- `openmir2-m3-quest-align` 的 `work` 写 `run_quest_pair.ps1` 缺失，实际 `quest_state_producer.py` 已在仓内。
- 看板 `m3-*-align` 系列多数 `work` 停在 `master d4d5b89c`，与 `1915b67a5` 差 30+ 提交。
  **开窗前必须重跑静态 readiness，不按板面文字排期。**

---

## 2. A 表按当前实际状态重排

### 2.1 已交付（本周，移出待办）

| 原# | 面 | 落地提交（本周） | 残余 |
| --- | --- | --- | --- |
| A-01 | GuildOfficial NPC 标签派发 | `0888fea97`（派发）、`2c0ea9bf0`（764/644 帧）、`f9b9b5f1a`（622 + 按 type 分流） | **T-137** 固定 NPC `Npc_def` 脚本装载（最大缺口）；**T-138** `@@requestcastlewarnow`；真机窗口（tier=R） |
| A-05 | `OPENYBDEAL` 引擎侧 | `1495fd4b2`（`ActionOfOpenSaleDeal`）、`13769d591`（复核订正） | **T-142** 宿主接线：`NpcPlayer` 未覆写 `set_play_dice_label`/`play_dice_label`/`sale_deal`/`set_sale_deal`/`config_game_gold_name`；`shop::lable_is_can_jmp` 仍两支 |
| A-11 | MB-2（87/93/104/94/52/96） | **更早已落地**（本周无新增） | 真机观察（tier=R） |
| A-12 | MB-5（208/209/206/210/203） | `b382a5876`（五类 Run）、`2fa927175`（透明帧排水 + 瞬移收件人） | **T-134** `RM_SPACEMOVE_FIRE` 未排水；**T-135** `RM_CHARSTATUSCHANGED` 未排水；真机 |
| A-13 | MB-6（200/133/131/117） | `00a039ee9`（三类 Run）、`bfd5d1e2c`（补牙 + 锚点） | **T-141** 命中骰站点（在飞 `wt-t141`）；真机 |
| A-18 部分 | E-1/E-2/E-4 | `22a2ff847`（E-1/E-2）、`ba82032f4`（encoding crate）、`a264e32b4`/`265fc19da`/`e88382c2f`（调用点）、`f07eb5a97`（callsites 台账） | **E-3/E-5/E-6/E-7/E-8** 仍未做；`crates/data` StringList BOM 由 `wt-strbom` 在飞 |
| 非 A | HUtil32 全量补齐 | `d84848a4a`（位打包/解析/切分/IP）、`a8eff0ef2`（round）、`e6ddc9fb7`（trim_parse） | 真机 round 调用点；10 个 GB2312 族成员（零依赖契约）；两份 Round 分叉（建议 T-136） |
| 非 A | 数据加载器 | `b38d28bca`（StartPoint 全字段）、`fac227ddd`（Server.conf 11 键）、`552303cab`（部署件判据） | `openmir2-loader-fields` 余量：其余加载器字段消费点核查（StartPoint 已在 `main.rs:517` 接线）+ 真机窗口 |
| 非 A | 登录尾 654/第二 42/100×4 | `79c6de691`、`547ba2a99`、`1915b67a5` | tier=R 窗口三项：改动后登录抓包、有会角色 811/46 与 750 位置、非 TestServer SysMsg 条数 |

> 说明：A-01 的派发逻辑与帧字节已按 oracle 接线，但**真实客户端点国王仍看不到菜单**（T-137）。
> 这是「代码已合、生产仍不可用」的典型，不应在摘要里算作已收口。

### 2.2 在飞（本波禁止触碰的写集）

| 卡 / 车道 | 写集 | 状态 |
| --- | --- | --- |
| `openmir2-account-protection-13`（`wt-acctprot`，wave14） | `crates/storage/src/account.rs`（**接口冻结面**） | 实现中；INSERT 13 列/UPDATE 字段级 |
| `openmir2-market-def-impl`（`wt-marketdef2`，wave13） | `crates/envir/src/merchant.rs`、`crates/script/**`、`bins/game-svr/{merchant.rs,npc.rs,shop.rs,main.rs}` | 实现中；含共用 `NpcDialogHost` 前置（T-137 的落点） |
| `openmir2-mb7-monsters`（`wt-mb7`，wave14） | `crates/world/src/{monai.rs,entity.rs}` | 实现中；MB-7（134/113/114） |
| `openmir2-strlist-bom`（`wt-strbom`，wave13） | `crates/data/src/{lists.rs,monitems.rs,guild.rs}` | 实现中；StringList 读侧 BOM |
| `openmir2-t141-hit-site`（`wt-t141`，wave13） | `crates/world/src/combat.rs`、`rng-trace-probe/**` | 实现中；RNG 站点对齐 |

### 2.3 待实现（按「依赖 / 写集 / tier / 预计批次」排序）

> 批次含义：P1–P3 = 下一批可并行的三条车道；H1–H3 = `bins/game-svr` 单线串行的宿主队列；
> W = `monai.rs` 世界队列（MB-7 让出后）；E = 网关/配置编码队列；O = 等 owner。

| ID | 原# | 面 | 依赖 | 写集 | tier | 批次 |
| --- | --- | --- | --- | --- | --- | --- |
| R-01 | A-02-A | CastleOfficial 纯层：标签分类、变量展开、门/修/墙动作结果、`@main` say 标签环 | 设计 `e0bbcde61` 已合；无代码依赖 | `crates/world/src/castle.rs`（+测试） | L（C5 帧面 R） | **P1** |
| R-02 | A-03-T1a | `MerchantState` 增 `castle_index`（spawn 时命中城堡，`MapRandomMove` 后不变） | 无 | `crates/world/src/merchant.rs`（+`world.rs` helper） | L | **P1** |
| R-03 | A-22-world | `CastleEvent` 的 `SaveAttackSabukWall`/`Save`/`Broadcast` 事件语义与单测（**不接宿主**） | `crates/envir::castle::serialize_attack_sabuk_wall` 已存在（`castle.rs:493`） | `crates/world/src/castle.rs` | R（持久化） | **P1** |
| R-04 | A-18/E-3 | DBSrv 字节边界：`char_spans` 前导吞任意后继、`split2_nonempty` 先解码、`arrest` 按 cp936 边界 | 无 | `bins/db-srv/src/{gbk.rs,handler.rs,loginsrv.rs,route.rs}` | L | **P2** |
| R-05 | A-18/E-7 | DB 名字/键去掉 `CONVERT(… USING gbk)`，按 `utf8mb3` 列绑定/读回 | 无（E1-a 测试库已有） | `bins/db-srv/src/{db.rs,playdata.rs}` | **R**（字段级 diff） | **P2** |
| R-06 | A-20 | M3 夹具残余：quest marker 前缀 + C# k=0 截断、drop-exp 单怪 pin 契约、静态 readiness 重跑 | 工具已在仓内；real-run 需窗口 | `mir2-rs/tests/parity/m3/**`、`tools/**` | R（real-run）；工具面 L | **P3** |
| R-07 | A-19 | LB-10 证据包：建/删/选角库后态、9 表 + `characters_indexes` 逐字段 diff 命令级证据 | `dbsnap_pair`/探针已合（`130240c8b`/`2702a552e`/`5a873412a`） | `tools/**`、`mir2-rs/tests/parity/m3/**` | R | **P3** |
| R-08 | A-17 | 怪物 AI 移动 `SM_WALK(11)` 下行（含观察者集合与 500ms 缓存口径） | Market_Def 让出 `bins/game-svr`；`monai.rs` 由 MB-7 让出 | `crates/world/src/monai.rs`、`bins/game-svr/{main.rs,downlink}` | **R**（时序/帧） | **H1** |
| R-09 | A-05/T-142 | `PlayDiceLabel` 三支宿主接线 + `OPENYBDEAL` 的 `SaleDeal`/`GameGold` 读写 | Market_Def 合入（同一 `npc.rs/shop.rs`） | `bins/game-svr/{npc.rs,shop.rs}` | **R** | **H1** |
| R-10 | T-134/T-135 | MB-5/6/7 宿主排水：`RM_SPACEMOVE_FIRE`(8062)、`RM_CHARSTATUSCHANGED`(10139) 及同族 | MB-7 合入 | `bins/game-svr/src/main.rs`（`drain_special_ref_msgs`/`drain_monster_ref_msgs`） | **R** | **H1** |
| R-22 | T-137 | 固定 NPC `Npc_def/<ChrName>-<MapName>` 脚本装载 + `GotoLable("@main")` 说词（点国王/城堡管理有菜单） | Market_Def 的 `NpcDialogHost`（MD-D）；`NpcHost` 现只装世界面 | `bins/game-svr/{npcs.rs,npc.rs,shop.rs}` | **R** | **H2** |
| R-12 | A-02-B | CastleOfficial `UserSelect` 宿主 + 复用固定 NPC 对话层 | **T-137/`NpcDialogHost` 必须先落地**（Market_Def MD-D） | `bins/game-svr/{npcs.rs,npc.rs,shop.rs,main.rs}` | L（构造级）/ R（C5/C11 帧序） | **H2** |
| R-11 | A-03-T1b..T4 | 城堡税/成员价宿主：spawn 写 `castle_index`、买/卖/修/升级四分支应用税、`castle_member` 接线 | R-02 + Market_Def 合入；`MasterGuild` 装载（T-131） | `bins/game-svr/{merchant.rs,shop.rs,main.rs}` | L（设计判）/ R（落盘则升） | **H2** |
| R-13 | A-22-host | `CastleMgr.Run/Save` 排水：`SaveAttackSabukWall`、10min `CastleMgr.Save`、`Broadcast`/`SS_204/212` | R-03；`crates/envir` 串行 `castle-file-load` | `bins/game-svr/{main.rs,castle.rs}`、`crates/envir/src/castle.rs` | **R** | **H2** |
| R-14 | A-09 | `GetUpgradeStdItem` Desc 差量 + `CustomItemSystem.GetItemName` 自定义名（T-24 残余） | R-11 的 `shop.rs` 边界；`ItemNameList` 解析 | `bins/game-svr/{bag.rs,itemwire.rs,trade.rs}`、`crates/data/src/lists.rs` | **R**（设计判 L，按保守取 R） | **H3** |
| R-15 | A-06 | GM `@` 命令：G1 派发骨架 + 6 条可达命令；G6 桩/死键；G2 会话开关 | Market_Def 合入（`main.rs`/`chat.rs`）；owner GM 权限裁决（O-02） | `bins/game-svr/{gmcmd.rs(新),chat.rs,main.rs}` | G1/G6/G2开关 = L；G2 落库/G3–G5 = R | **H3** |
| R-16 | A-10 | `SETOFFLINE` 的 `AutoGetExp*` 四字段 + `OffLineFlag`/`KickOffLineTick` 覆写 | Market_Def 让出 `crates/script`/`npc.rs` | `bins/game-svr/src/npc.rs`、`crates/script/src/engine/mod.rs` | **R** | **H3** |
| R-17 | A-21 | PL-3 残余：`ShowScriptActionMsg` 自消息、`SETMPAMODE DECHP`、`HungerDecPower` 消费者 | 内容不可达；需 owner 决定是否构造启用（O-03） | `crates/script/src/{engine,actions.rs}`、`bins/game-svr/src/npc.rs` | **R**（内容启用后） | **H3** |
| R-18 | A-15 | MB-8：214/132/120/`RunAway`/100 的 `Run` | MB-7 让出 `monai.rs`；数据不可达 | `crates/world/src/monai.rs` | R（R42=L） | **W** |
| R-19 | A-16 | MB-9：215 `FireballMonster` 全链（`MagicMonster.Run` 两 pass / `Think` / `AttackTarget`） | 同 R-18 | `crates/world/src/monai.rs` | R | **W** |
| R-20 | A-18/E-5/E-6/E-8 | `Server.conf` 双链编码、`String.conf`/`AddrTable`/三网关配置 BOM、网关合成登录包 decode→re-encode | 编码 crate 已合 | `bins/{game-svr,game-gate,login-gate,sel-gate,login-srv}` 的 `config.rs`/`strings.rs` | L | **E** |
| R-21 | A-07 | 跨服 `SS_205/206/207/211/212` | owner D-08（O-07） | — | L（若登记不实现） | **O** |

**待实现计数：22 行**（其中 P1/P2/P3 共 7 行、宿主队列 H1–H3 共 11 行、W 2 行、E 1 行、O 1 行）。

### 2.4 数据不可达 / 死面（C 表，不排期）

C 表 20 条结论不变（`docs/m3-remaining-inventory-20261022.md` §C）。本轮只补 3 条判读：

1. **数据不可达 ≠ 不实现**：A-15/A-16（MB-8/MB-9）仍按 R 实现、构造级判据；真机窗口无对象，
   review 写 `window_required: true` + owner 替代证据（O-13）。
2. **C-17 城堡 #4/#7/#8**：`AttackSabukWall.txt` 0 字节 / `OwnGuild=` 空 / `TotalGold=0`，
   无城主无攻击者 ⇒ A-02/A-03/A-22 的窗口项**不应占 2h 块**，只做构造级。
3. **C-20 `characters_quest` 永不落库**：任务验收应判「不落库 = 等价」，不要把它当缺口。

---

## 3. 下一批 3 个车道（互不冲突写集）

> 选择依据：① 与 §2.2 在飞写集零交集；② 都能在离线用构造级判据合入；③ 三条彼此零交集。
> 每车道一个 principal、一个 worktree、一个 `CARGO_TARGET_DIR=/e/tmp/<lane>-target`。

### 3.1 车道 P1：`castle-pure`（R-01 + R-02 + R-03-world）

- **性质**：tier=L（R-03 的持久化语义按 R 登记，但本车道只做世界层事件与单测，不碰文件写）。
- **写集**：`mir2-rs/crates/world/src/castle.rs`、`castle_tax.rs`（仅测试）、`merchant.rs`、`world.rs`、
  `crates/world/tests/**`。**不碰** `bins/**`、`crates/envir/**`、`guild.rs`、`monai.rs`、`combat.rs`。
- **交付**：
  1. `official_variable_text` / `CastleOfficialAction` 扩展 / `OfficialSelectOutcome`（设计 §5.1）；
  2. 标签派发表（含大小写、`@hireguardnow3`、`@hirearchernow12`、未知标签 `None`）；
  3. N1 金库 / N5 雇佣的 no-op 语义（T-38/T-40）；
  4. `MerchantState.castle_index` + `set_merchant_castle_index` / `merchant_castle_index`；
  5. `CastleEvent::{SaveAttackSabukWall,Save,Broadcast,GuildRefreshShowName}` 的事件语义与单测。
- **验收口径（tier=L，改坏必红）**：
  - 设计 §7 的 **C1、C2、C3、C4（帧面抽样）、C6、C7、C8、C9、C12、C13、C14** 全绿；
  - C5（修门/墙时序）、C10（`@@castlename` 落盘）、C11（复合出队序）**不在本车道收**，留给 H2；
  - 具体牙：去掉 `Permission>=3`/`MasterGuild` 门 ⇒ C1 帧数断言红；
    N1 实现真提取 ⇒ C6 红；N5 返回 `Ok` ⇒ C7 红；变量名改大小写不敏感 ⇒ C8 红；
    删 `can_jmp_labels` 的 `Clear` ⇒ C9 红；把门 `None` 当 `"closed"` ⇒ C8 红；
  - `cargo test -p mir2-world`（在本 worktree、隔离 target）全绿；变异后 `git checkout --` +
    `sha256sum -c` 还原。
- **规模**：1 波（设计件已把行号/常量/期望值写死，主要是搬运 + 单测）。

### 3.2 车道 P2：`db-encoding`（R-04 + R-05）

- **性质**：E-3 = L（畸形 GBK 边界）；E-7 = R（`ChrName` 落库/字段级 diff）。
- **写集**：`mir2-rs/bins/db-srv/src/{gbk.rs,handler.rs,loginsrv.rs,route.rs,db.rs,playdata.rs}` + 单测。
  **不碰** `crates/{protocol,storage,encoding}`、`bins/game-svr/**`。
- **交付**：
  1. `char_spans` 改为「前导 `0x81..=0xFE` 吞任意后继」；`char_count` 与 .NET 一致；
  2. `split2_nonempty` 先按 cp936 解码再切、片段按 cp936 不可映射 → `?`（`0x3F`）输出；
  3. `arrest` 按 cp936 字符边界扫描定界符（不再裸字节扫描）；
  4. DB 路径删除 `CAST(CONVERT(ChrName USING gbk) AS BINARY)` 与 `CONVERT(? USING gbk)`，
     直接以 `utf8mb3` 字符串绑定/读回（`db.rs`/`playdata.rs`）。
- **验收口径**：
  - E-3 的 **≥20 组向量**（审计 §E-3 原文）：`81 20/2F/7F/00/FF/0A`、`split2` 两条、`arrest` 两条；
  - 牙：`char_spans` 改回「合法后继才成对」⇒ 前 9 条红；`arrest` 恢复裸字节 ⇒ 2 条红；
    `split2` 输出原始字节 ⇒ 2 条红；DB 路径重新出现 `CONVERT` ⇒ E-7 断言红；
  - E-3 合入后可按 L 收卡；E-7 合入后 `status` 保持 `in-progress`，窗口用 E1-a 测试库
    做「建角/取档/改名/重登」四步的字段级 diff（`dbsnap_pair`）；
  - 5100 请求级夹具可选：账号/角色名含 `81 2F` 的登录帧两侧接受/拒绝 + 回包字节一致。
- **规模**：1 波。注意 `crates/storage` 是冻结面，**本车道不碰它**（A-04 由 `wt-acctprot` 负责）。

### 3.3 车道 P3：`fixtures-window-prep`（R-06 工具面 + R-07 命令面）

- **性质**：tier=L（只改夹具/工具/文档），但 real-run 收口仍等窗口。
- **写集**：`mir2-rs/tests/parity/m3/**`、`tools/**`、`docs/window-run-sheet-20261022.md` 的**只读引用**
  （不在本车道改 run sheet；如需改，另开 runsheet 卡）。**不碰**任何 `src/**`、`bins/**`、`crates/**`。
- **交付**：
  1. `marker_prefix_gate.py` 的 `GUARD_ONLY_MARKERS`/COMPARATORS 登记补齐，`check` 与 `selftest` 双绿；
  2. `quest_state_producer.py` + `run_quest_pair.ps1` 的静态 readiness 报告（列出 C# k=0 截断、
     marker 前缀两个阻断项的**当前实际状态**，不沿用板面文字）；
  3. `run_drop_exp_pair.ps1` 的 `-SelfCheckNoStack`/`-PurityOnly`/`-DiffOnly` 全绿；
     `-Go` 无 pin 怪时保持 fail-closed `exit 2`；
  4. LB-10 run sheet 的「可绿命令」重跑：`dbsnap_pair` 逐表逐行逐列 diff、建/删/选角后态探针；
  5. 窗口前置复算脚本输出（R6/R7/R8 三行状态），作为下次开窗的 §0.2 快照。
- **验收口径（tier=L）**：
  - 所有夹具自检 `GREEN n/n`、rc=0；`fixture_chain_gate.py` 全绿；
  - 牙：漏登记新夹具族 ⇒ `marker_prefix_gate.py check` rc=1；drop-exp 无 pin 跑 `-Go` ⇒ exit 2；
    镜像落后时 `check_rust_window_preflight.py` ⇒ RED R6（不得静默通过）；
  - 不产生任何生产路径改动；`git status --porcelain` 干净（除本卡文件）。
- **规模**：1 波；可与 P1/P2 并行。

### 3.4 宿主队列（H1 → H2 → H3，单线串行）

`bins/game-svr/**` 同一时刻只能一条线，顺序按「生产可见度 + 窗口块适配」：

| 序 | 批次 | 内容 | 理由 |
| --- | --- | --- | --- |
| 1 | **H1** | R-08 `SM_WALK` → R-09 T-142 → R-10 T-134/T-135 | 全是「客户端看不到/回跳被拒」的可见缺口，且都落 G3a/G2a 窗口块 |
| 2 | **H2** | R-22 T-137 固定 NPC 说词 → R-12 CastleOfficial 宿主 → R-11 城堡税宿主 → R-13 城堡落盘 | 先补共用对话层；stock 城堡不可达，只做构造级 + 字节判据 |
| 3 | **H3** | R-14 物品名 → R-15 GM → R-16 SETOFFLINE → R-17 PL-3 | 都是 `main.rs/chat.rs/npc.rs` 尾部；GM 只有 6 条现网可达 |

**启动条件**：`wt-marketdef2` 合入并让出 `bins/game-svr`；`wt-mb7` 合入并让出 `monai.rs`
（R-08 需要两者都让出，因为它同时改 `monai.rs` 与 `main.rs`）。

### 3.5 三条车道的验收总表

| 车道 | tier | 必须绿 | 不得触碰 | 收卡 |
| --- | --- | --- | --- | --- |
| P1 castle-pure | L | C1/C2/C3/C4/C6/C7/C8/C9/C12/C13/C14 + G-T1a；变异必红 | `bins/**`、`crates/envir/**`、`monai.rs`、`combat.rs` | 可 `closed` |
| P2 db-encoding | E-3=L / E-7=R | 20+ 向量 + arrest/split；DB 无 `CONVERT` | `crates/storage`、`crates/encoding` 源码 | E-3 可 `closed`；E-7 保持 `in-progress` |
| P3 fixtures | L（工具） / R（real-run） | marker gate + drop-exp 自检 + LB-10 命令 + preflight 快照 | 任何 `src/bins/crates` | 工具面可 `closed`；real-run 保持 `in-progress` |

---

## 4. 「必须等窗口」与「可离线做」

### 4.1 窗口前置实测（2026-10-21 01:12，只读）

| 门 | 实测 | 结论 |
| --- | --- | --- |
| R6 构建源 | 镜像 HEAD `4a887354e`；master `1915b67a5`；相差 **32** 提交 | **RED** |
| R7 产物/provenance | `/e/tmp/m2svr-window-target` 六件存在；`window-provenance.json.source_head=4a887354e` | **未绿**（旧基线） |
| R8 端口 | `port-status.ps1`：GameSrv 5000 / DBSrv 5100,5700,6000 / LoginSrv 5500,5600 / LoginGate 7000 / SelGate 7100 / GameGate 7200 全部监听；mysqld 3306 | **RED** |
| E1 | 测试库 `mir2_test_db/_account/_data` 已建（run sheet §E-0） | 已裁决 |
| E2 | run sheet §E-0 记「2026-10-21 E2 可以」；runbook §4 注记 `b46805058` 仍记待裁决 | **矛盾，见 O-11** |
| E3 | 未回答 | 待裁决 |
| E4 | 允许重编，但 runbook 记「今日未用」 | 已裁决 |

**结论**：今天**不能开窗**。最少要 owner 停栈（十端口 DOWN）→ 镜像 `git fetch` 到当天
`origin/master` → 重编六件 + 重录 provenance → guard `WINDOW_GUARD_OK`。

### 4.2 可离线完成（不需要窗口）

| 类型 | 项 |
| --- | --- |
| 纯 L 实现 + 单测 | R-01、R-02、R-04、R-20；R-11 的纯算术/帧向量；R-12 的纯层与构造级；R-15 的 G1/G6/G2 开关 |
| 工具/夹具 | R-06 的工具面、R-07 的命令面 |
| R 的**实现**（可合入、不可收卡） | R-03、R-05、R-08、R-09、R-10、R-13、R-14、R-16、R-17、R-18、R-19、R-22 |
| 文档/设计 | 本文件；如实现中发现口径冲突，另开设计卡 |

### 4.3 必须等窗口（代码已无缺口或缺口只能真机证）

| 窗口块 | 能收/部分收 | 说明 |
| --- | --- | --- |
| D.1 G0+G1 | `smframe-minor-gaps`（全收）；`login-frame-order-52-752`、`login-numeric-gaps`、`healthspell-emission-model`、`orphan-encoder-wiring`、`window-collector-preflight`、`outbound-encoders-red-batch`（部分） | 登录/入图基线；52 布局 Rust `client58` / C# `oracle40` |
| D.2 G2a | B5 R1/R2/R3 的 210/212/211+621 | 装备/技能登录态；全机判 |
| D.3 G3a | `monster-attack-hit-frame`、`monster-attack-cadence-lead`、`player-dir-supply`（全收）；AOI/目标选择若干（部分） | 单客户端战斗/AOI/GuardList；**R-08 的 `SM_WALK` 应并入本块** |
| D.4 G5a | `oracle-humitems-align`、`delmagic-addmagic-chain`（E1-a 可收）；`save-on-shutdown`（E1-a 只能部分） | 优雅关服/存档；字段级 diff 需 E1-b |
| 后续新块 | E-7 字段级 diff、R-22（NPC 菜单）、R-09/R-10、R-13、R-14、R-16、R-17、R-18/R-19 的构造级抽样 | 需各自场景/构造 |

### 4.4 本轮**不排**的组（工具/授权/内容/时长未齐）

| 组 | 原因 | 对应卡 |
| --- | --- | --- |
| G4 法术/延时 | 无法术按键/`AutoSpell` 驱动（C10#6） | `default-arm-spell-surface`、`rm-refmessage-delay-layer` 等 |
| G6 DB 故障 | 需 E3 停/起 mysqld + 6000/5100 抓包（C9） | `dbsrv-fault-paths`、`db-error-reply` |
| G7 网关 | 工具 v2.5 已闭环，真机 capture 未采；0-210ms 丢帧不在覆盖 | `gate-idle-timeout`、`selgate-reconnect`、`gategate-speed-rules` |
| G8 内容周期 | 内容未启用 + C# 时钟未落（O3） | `world-tick-expansion`、`transmap-maptrigger` |
| G9 整包 W0 | O1–O7 未裁决 | `m3-rng-scene-driver`、`ctor-draws-fold` |
| G10 M3 夹具链 | readiness 未重跑；④/⑥ 仍 BLOCKED | `m3-*-align`、`m3-fixture-chain-gate` |
| G11 24h 长跑 | 需独占 24h+ | `m3-final-acceptance`、`m5-convergence-drills` |

> **round2 runbook 的教训必须带进下一窗**：Rust 侧 `winplay0` 在 `(288,620)` 落进 `MonGen` 刷怪框
> （Chebyshev=0），三次 `CM_WALK` 被挡后原地被打死，`Hp=0` 写回测试库。下一轮固定 `(311,666)`
> （`check_reverse_point_clean.py` = CLEAN；距最近生成框 Chebyshev=44）、**不走路、不打怪**，
> 且必须先 `export_s2c.ps1` 再判帧（当前 proxy dump 缺 `port` 字段，`s2c_diff.py` 直接 PREREQ_MISSING）。

---

## 5. Owner 待裁决

### 5.1 已裁决（不必再问）

| ID | 项 | 裁决 | 影响 |
| --- | --- | --- | --- |
| E1 | 写库边界 | 从正式库克隆 `mir2_test_*`（E1-a） | 所有构造只准写测试库；`scoped_diff.TABLE_COLUMNS` 未登记测试库 ⇒ G5 只能行数/关键值 |
| E2 | 影子内容 | 「E2 可以」（2026-10-21 会话，run sheet §E-0） | 行会 750 / 特殊怪 / 非 TestServer 可开窗（仍需 O-11 确认） |
| E4 | C# 重编 | 允许重编对比用 C# | 影子包可 `-Shadow` 编译并记 HEAD/DLL hash |
| D-01 | E1 具体选型 | = E1-a | 同 E1 |
| D-14 | 编码 cp936 全表 | 已由 `mir2-encoding` crate 落地（`ba82032f4`） | E-1/E-2 已合；E-3/E-5/E-6/E-7/E-8 仍按卡推进 |

### 5.2 待裁决（按阻塞面排序）

| ID | 裁决项 | 现状 / 事实 | 不裁决的后果 | 建议 |
| --- | --- | --- | --- | --- |
| O-01 | **E3 MySQL 停/起** | mysqld 是进程（`D:\mysql\mariadb-10.11.19-winx64\bin\mysqld.exe`），`start-all -Stop` 会连带杀掉 | G6 不可开窗；`dbsrv-fault-paths`/`db-error-reply` 永远缺真机证据 | 暂缓 G6，先做离线；若要跑，明确「停：`Stop-Process -Name mysqld`；起：指定 datadir/port」并独占 |
| O-02 | **GM Permission** | 现网 `Permission` 恒 0（`GetHumPermission` 被注释、`AdminList` 空、`StartPermission=0`）：185 命令里只有 5 条 perm 0 + 内建 `@help` 可达 | GM G2–G5 的 180 条在现网**不可验**；Rust 连分发都没有，`@时间` 这类已可见分歧 | 建议 **(a) 1:1 复刻 perm=0 锁死**；要验非 0 权限，owner 需批准一次性 `SETPERMISSION` 测试脚本或影子 `Server.conf`（内容变更，走 E2） |
| O-03 | **内容启用范围** | P22/P25/P26/P27/P31、Hunger、`KILLMONEXPRATE` 等部署 0 命中 | A-21/R-17 与 C-05..C-07 只能构造级；「内容启用后按 R 对拍」永远不触发 | 明确哪些可在影子 Envir 构造启用样本（E2 已批），哪些只登记 |
| O-04 | **RNG 场景 pin O1–O7** | `docs/window-owner-decisions-20261018.md §2` 勾选栏全空；状态 UNKNOWN | G9/W0、`m3-rng-scene-driver`、`ctor-draws`、`npc-ctor-draws` 无路线 | 重签或作废；作废则 G9 改「构造级 + 抽样」 |
| O-05 | **NPC ctor draws F/M（8 处）** | 看板 `npc-ctor-draws` 明写待裁；`Robot.txt` 行数未实测 | RNG 账本 `seq=0` 不可信，整包 W0 对拍无锚 | 先只读实测 `Robot.txt` 行数，再裁 F（只补抽）/M（建实体） |
| O-06 | **`save-on-shutdown` oracle 是否真落库（D-12）** | 看板 `save-on-shutdown` 明写待裁 | B-23 的「金标准」不确定 | 用部署件抓包 + 库后态确认，形成一行结论 |
| O-07 | **跨服 `SS_*`（D-08）** | 单服无接收端（T-113） | R-21 无法收口 | 建议**登记「单服不实现」**，不排期 |
| O-08 | **城堡优先级重裁（D-07）** | `AttackSabukWall.txt` 0 字节、`OwnGuild=` 空、`TotalGold=0`；无城主/无攻击者 | 若按 P0 排窗口，2h 块会花在不可达面 | 确认 A-02/A-03/A-22 **只做构造级**，不占 D.1–D.4 |
| O-09 | **T-119② 3min `TimeoutStop` 分支** | 只在写失败竞态可达 | keepalive 卡无法判「3min 档」等价还是缺陷 | 接受构造级边界，窗口只观察 10s/15 天两档 |
| O-10 | **`GateCount>1` 多实例（D-13）** | 三网关 config 只读 `*0`，单 listener；现网 1 | 配置 >1 即不可用（当前不可达） | 登记「单实例」，不开卡 |
| O-11 | **E2 书面记录矛盾** | run sheet §E-0 说已裁决；runbook §4 注记 `b46805058` 仍「待裁决」 | 行会 750/G3b/G3c/B10 是否可以开窗不确定 | owner 用一行签名（commit 或 thread entry）钉死；在此之前按**未裁决**处理 |
| O-12 | **oracle 源码漂移** | 部署件 `!deployed-from.json.git_head=8a621f6a`；另一 zig 会话正在改 `src/**`（源码 HEAD `d7ead5b2a`） | 同一窗口两侧对照可能指向不同 oracle | 开窗期间冻结 `src/**` 与 `E:/MirServer` 二进制；每次窗口重钉 HEAD |
| O-13 | **不可达面的投入边界** | MB-8/MB-9/A-22/A-07 部署/内容不可达 | 会持续消耗实现与评审 | 明确「构造级实现 + 不排窗口 + review 写 `window_required: true`」 |
| O-14 | **`E:/tmp` 清理（D-10）** | A 类可回收 448.1 GiB（cargo target 443.6 GiB）；D 类 29.5 GiB 须人核 | 磁盘压力；D 类误删不可逆 | 批准 A 类自动回收；D 类逐项人工确认 |

---

## 6. 排期总表

| 批次 | 车道 | 内容 | tier | 验收 | 开窗? |
| --- | --- | --- | --- | --- | --- |
| **P1** | castle-pure | R-01/R-02/R-03-world | L（R-03 语义） | 设计 §7 C1–C4/C6–C9/C12–C14 + G-T1a；变异必红 | 否 |
| **P2** | db-encoding | R-04/R-05 | L / R | 20+ E-3 向量 + 无 `CONVERT(gbk)`；E-7 窗口字段级 | 否（E-7 收卡要） |
| **P3** | fixtures | R-06/R-07 | L / R | marker gate + drop-exp 自检 + LB-10 命令 + preflight 快照 | 否（real-run 要） |
| **H1** | game-svr host | R-08/R-09/R-10 | R | `SM_WALK` 帧字节/收件人；PlayDiceLabel 三支；10139/8062 排水 | 实现否 / 收卡要 |
| **H2** | game-svr + castle | R-22/R-12/R-11/R-13 | L/R | T-137 说词、G-T1..T6、C1..C14、C10/C11；构造级 + 字节 | 实现否 / 收卡要 |
| **H3** | game-svr + script | R-14/R-15/R-16/R-17 | L/R | G-N1..N7、GM §7.1–7.2、SETOFFLINE 四字段、PL-3 八条 | 实现否 / 收卡要 |
| **W** | monai | R-18/R-19 | R | MB-8/MB-9 构造级 Run 序列 | 否（数据不可达） |
| **E** | gateways/config | R-20 | L | E-5/E-6/E-8 三份 BOM/编码夹具 + 登录包字节 | 否 |
| **O** | owner | R-21 | L | 登记「单服不实现」 | 否 |

**关键路径**：`wt-marketdef2` 合入 → H1（R-08 还需 `wt-mb7` 合入）→ H2 → H3。
P1/P2/P3 可在 Market_Def 合入前并行跑完。

---

## 7. 风险与对策

1. **看板 stale**：`m3-*-align`/drop-exp/quest 的 `work` 字段落后 30+ 提交。对策：开窗前重跑
   `fixture_chain_gate.py`、`marker_prefix_gate.py`、`run_drop_exp_pair.ps1 -SelfCheckNoStack`，
   以输出为准（P3 的交付物之一）。
2. **E2 记录矛盾（O-11）**：在 owner 签名前，行会 750、G3b/G3c、B10 一律按**未裁决**处理。
3. **oracle 源码漂移（O-12）**：`src/**` 正被另一会话修改修 bug；窗口 provenance 必须记录
   C# 源码 HEAD + 部署件 hash；开窗时段要求冻结。
4. **写集瓶颈**：`bins/game-svr/**` 是本轮所有高价值项的公共写集；任何并行都会把
   `main.rs` 的 hunk 打散。对策：H1→H2→H3 单线，P1/P2/P3 先行。
5. **R 卡制度成本**：本周新增 R 卡全部 `in-progress`，看板「进行中」会长期维持 160+。
   对策：`work` 写清「已合入（oid，tier=R）；余量=窗口项」，不要让读者误判为未开工。
6. **master 与 origin 漂移**：`port-status.ps1` 记 master 有 **31** 个未推送提交；
   `57452dba0` 起代码主源改为 walgit，GitHub 只是人工镜像。对策：窗口基线与发布以 walgit 为准，
   不要把 `origin` 当唯一真相。
7. **E1-a 的字段级限制**：`scoped_diff.TABLE_COLUMNS` 未登记 `mir2_test_*`；G5 在 E1-a 下只能
   行数/关键值。对策：A-04/E-7/A-19 的字段级证据要么走 E1-b，要么先补登记（工具卡）。
8. **测试点事故**：round2 已在 `(288,620)` 被怪打死并写回 `Hp=0`。对策：下一窗固定 `(311,666)`，
   不走路、不打怪，先 `export_s2c.ps1`。

---

## 8. 残留物清单

- 本车道只新增 `docs/remaining-plan-v2-20261021.md`；worktree `/e/tmp/wt-planv2` 内无 cargo target、
  无临时库、无进程。
- 仓外临时物：`E:/tmp/planv2-board.json`（看板 JSON 只读快照，约 600 KB），可随时删除；
  本文件不依赖它存在。
- 未触碰：`E:/MirServer`、主检出、`mir2-rs/src/**`、`mir2-rs/bins/**`、`mir2-rs/crates/**`、
  `sql/**`、`MirClient/`、`crates/{protocol,storage,encoding}` 源码、任何数据库。
- 未运行：服务端/客户端/代理、cargo、任何写库命令。
