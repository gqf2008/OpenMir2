# 条件② 证据强度盘点（2026-10-25）

- 作者：`openmir2-reviewer-20`（只读盘点；未跑 cargo、未起栈、未改任何文件）
- **快照**：`ecf13b1cf`（2026-10-25 12:15 +08，整体基线）；各节另有自己的时点戳。
- 收录：由 `openmir2-reviewer-20` 撰写并签 comment `1541db4e9b`，Leader 原样搬入仓库（仅加本两行头部）。
- 定位：今天两轮审计（`recog` 全局掩码吞语义值 / 无条件回帧当成功判据）动摇了「既有证据可信」这个前提。
  本文把窗口证据按**三档**归类，并明确**哪些已跑过的证据因今天的改动必须重跑**。
- 分档口径：判据必须落在**成功分支**或**可观察副作用**（RNG 抽数 / 落库写 / 状态变更），且承重字段进了比较；
  「某帧出现」只有在**该帧本身是互斥成功支或字段被钉值**时才算成立。

## 1. 确认为真（判据在成功分支/可观察副作用，且字段进比较）

| W 项 / 判据 | ident | oracle 发射点 | 为什么成立 |
|---|---|---|---|
| W-TAX-1 | `SM_BUYITEM_SUCCESS(650)` | `src/GameSrv/Npc/Merchant.cs:1301-1304` | `if (n1C == 0)` 才发成功帧；`n1C=0` 只在 `AddItemToBag` 成功且 `Gold -= nPrice`（`:1254`）之后；失败走 `:1307` FAIL。且 `recog = 扣款后 Gold`（钉 9912） |
| W-DG-1 | `SM_GOLDCHANGED(653)` | `src/M2Server/Player/PlayObject.Operate.cs:222-247`（`GoldChanged()` 在 `:246`） | 四条早退门（安全区 `:224`、`CanDropGold :229`、`IsCanDrop/NoThrowItem :233`、`nGold >= Gold :240`）都在扣款 `:242` 之前 return。**承重的是 `recog=100` + `count_scope=all 恰 1`**：`DropGoldDown` 失败会回滚金额**仍发帧**，只有值能区分 |
| A 类 `plan-§2:529` | `SM_PASSOK_SELECTSERVER(529)` | `src/LoginSrv/Services/ClientSession.cs:250/271` | 只在 `if (nCode == 1 && accountRecord != null)` 发；失败走 `:277 SM_PASSWD_FAIL` |
| 615/616 | `SM_TAKEON_OK/FAIL` | `PlayObject.Operate.cs:458` / `:495` | 互斥成功支 |
| 635/636 | `SM_EAT_OK/FAIL` | `PlayObject.Operate.cs:690` / `:698` | 互斥成功支 |
| 648/649 | `SM_USERSELLITEM_OK/FAIL` | `Merchant.cs:1403`（`price>0 && sub && IncGold`）/ FAIL 支 | 互斥成功支 |
| 669/670 | `SM_USERREPAIRITEM_OK/FAIL` | `Merchant.cs:1658/1665`（`DecGold` 成功支） | 互斥成功支 |
| 638/639 | `SM_MAGICFIRE/FAIL` | `Magic/MagicManager.cs:405/652/656` | 施法成功支（失败走 639） |

**序/次类判据（成立，但只能当序锚点、不能当成功证据）**：
`657×2`（`SM_CHARSTATUSCHANGED`，`BaseObject.cs:1516` + `PlayObject.cs:2876`）、
`52→621→211`（`SM_ABILITY`，`BaseObject.cs:2161/2185`）、
`46→750→654`（`SM_CHANGELIGHT` 由 `RM_LOGON` 无条件发，`PlayObject.Message.cs:1683`）、
W-MQ-1 的 `643→767→44→645→5008`。这些帧是**状态同步**，断言的是「发了、顺序对」，不是「动作成功」。

## 2. 已修（今天发现、有卡或有 merge/commit oid）

| 缺陷 | 处置 | oid | 状态 |
|---|---|---|---|
| `recog` 全局掩码吞语义值（650/622/601 等） | 规则取回被掩字段 | `8cf9deaf9`（7 条）、`4a6c5406d`(600)、`5980961da`(601)、`750ec003b`(99/622/650 转正 + 658 recog 取回) | 已合 master |
| `SM_STRUCK(31)` / `SM_WALK-RUN(11/13)` / `211` / `53·638·639` / `202·203·642·709` 规则缺口 | 逐条补/窄掩码 | `2567ab6b8`、`829219ca6`、`427aaaaed`、`bc4f33711`、`c8473b344` | 已合 master |
| DB 快照在停栈前拍 ⇒ 单侧落库假差异 | 快照前加「可观测静默」等待 | `356d04eb8` | 已合 master |
| 会话时区 8h ⇒ 时间戳假差异 | db-srv 会话时区对齐 oracle(SYSTEM) | `8a1102806` | 已合 master |
| 时间列见证 fail-open（缺 `time_columns` 当放行） | fail-closed 默认 + 限定本角色行 | `06b0164c4` | 已合 master |
| 夹具 `ani_count` 取了 AniCount（`bag_equip` 207、`eat_item` 3） | 按冻结 `Shape` 修正 | merge `062d6beb5` | 已合 master（**真机未复跑**） |
| butch 场景缺装备前置 + 8 格×4 多点夹具 | 场景/profile 改 | merge `9f0208157` | 已合 master（**真机未复跑**） |
| track 选靶「先最近再要求相邻」 | 相邻优先 + 有界等待 | merge `ecf13b1cf` | 已合 master（牙缺口见 `a10db670`） |
| **`SM_BUTCH(637)` 被当成功判据** | 判据改「账本恰两抽」 | 卡 `openmir2-butch-637-criterion-fix`（dev-1） | **in-progress，未合** |
| **`SM_GAMEGOLDNAME(5008)` 登录必发** | 改 `match:last` + `param/tag=0` | 卡 `openmir2-criteria-table-hardening`（tester） | **in-progress，未合** |
| **`SM_MENU_OK(767)` 成功/失败共用提示通道** | 补 `body_text`/`order` | 同上 | **in-progress，未合** |
| 判据表 `count_scope` / W-DG-2 旧口径 | 表更新 | `45e4bff33`、`8ddeca838`、`f56ae7a17`、`cfe03685b` | 已合 master |

## 3. 仍弱 / 待重跑（**最要紧**）

分两类：**离屏重算**（只要 `frames.ndjson` 还在，重跑工具即可，0 窗口成本）与**必须重跑栈**
（DB 面 / 场景·夹具变了 / 服务端二进制变了）。

### 3A 只需离屏重算（不需要起栈）

| 要重算的东西 | 为什么 | 依据 |
|---|---|---|
| **所有既有证据根的 `compare_runs` 结论** | 规则在 10-23 20:05 至 10-25 11:37 之间多次变化（见 §2 前两行）。凡在规则变更前算出的 diff/PASS 结论都按旧口径 | `829219ca6`/`2567ab6b8`/`c8473b344`/`bc4f33711`/`427aaaaed`/`4a6c5406d`/`5980961da`/`8cf9deaf9`/`750ec003b` |
| **`e23-reshot` 的帧面结论（11:12–11:24）** | 该批在 `750ec003b`（11:37：99/622/650 转正 + 658 recog 取回）**之前**判 ⇒ 650 的 recog 语义、622 的 presence、658 的 recog 都按旧规则 | 证据根 `E:/tmp/window/e23-reshot/{csharp,rust}` |
| **`card_evidence` 的 27 行判据结果** | 那次跑用的是 run2 的 `cs-probe`（10-24 21:13）＋旧表；表其后改了 `count_scope`、W-CM-2 改人工、W-DG-2 改口径 | `E:/tmp/card-evidence-run2-cs.txt`、`45e4bff33` |
| **帧对齐级联涉及的多帧块** | 位置配对在「一侧缺帧」时整段错位（一次实测：50 条 diff 里 44 条是错位噪声、4 条是真差异）。规格 `ea6ae394` 的实现在 `compare_runs.py` 里**尚未落地** | 我的 comment `ea6ae394` |

### 3B 必须重跑栈（真机）

| 批次 / 证据 | 时间窗（run.json `start_wall`） | 为什么作废 | 处置 |
|---|---|---|---|
| `run2`（22 根） | 10-24 21:13–22:04 | 全部早于 save-settle(`356d04eb8`, 10-25 09:09)、TZ(`8a1102806`, 10:21)、见证 fail-closed(`06b0164c4`, 10:50) ⇒ **DB 面一律不可采信** | DB 面重跑 |
| `w1c`（38 根） | 10-25 08:55–09:40 | 跨过 `356d04eb8` 的 09:09；工作副本何时更新无从判定 ⇒ 一律按旧 harness 处理。帧面可离屏重算，**DB 面作废** | DB 面重跑 |
| `harness-proof`（4 根） | 10-25 10:09–10:12 | 早于 TZ 修复（10:21）⇒ 只能证明「settle 生效」，DB 面值不可采信 | 仅作 harness 证据 |
| `probe-rc`（4 根） | 10-25 09:42–09:46 | 与 DB/规则无关 | 有效（probe 行为） |
| `rev21-tzreal`（2 根） | 10-25 10:29–10:30 | TZ 修复**之后** | 有效（反向对照） |
| `db-rerun/run`（15 根） | 10-25 10:23–10:35 | 在 TZ 之后、**见证 fail-closed（10:50）之前** | 见证相关行需重跑 |
| `db-rerun/run2`（16 根） | 10-25 10:55–11:09 | 在 save-settle + TZ + 见证**之后** | **DB 面可用**（唯一按新口径跑的 DB 面证据） |
| `e23-reshot`（4 根） | 10-25 11:12–11:24 | 栈面在 db-rerun 之后（可用）；**帧面需离屏重算**（见 3A） | DB 面可用 |
| `butch-run1/run2`（4 根） | 10-25 11:41–11:59 | 场景/profile 在 **12:00/12:02** 才合入、选靶在 **12:05** 才合入；`butch-run2` 的 `start_wall` 是 11:59:57，差 3 秒 | **必须重跑**（新夹具 + 新选靶 + 新令牌） |
| `bag_equip` / `eat_item` 的历史跑 | 全部 | 夹具 `ani_count` 在 `062d6beb5`（12:02）才修 | **必须重跑** |
| `move_walk` / `login_only` 的 `characters.Home*` | 全部 | home 卡（`openmir2-home-coords-divergence`，head `45a6b377e`）**尚未合并** | 合并后重跑 DB 面 |

### 3C 判据本身的弱项（已开卡，落地前不要拿它当证据）

- `SM_GAMEGOLDNAME(5008)`：现判据 `min_count:1, match:any` **无字段** ⇒ 登录帧就能满足。实证：三个证据根
  （`harness-proof/r1`、`probe-rc/direct`、`e23-reshot`）各有 1 条 `5008 @ seq 14`（recog 分别 100 / 100 / 10000）。
- `SM_MENU_OK(767)`：全仓 **28 处**发射、多数是错误弹窗（`Operate.cs:226/231/236/256/261/685/915/924/929/1005/1053`、`Sale.cs:371/550/585`）⇒ 出现不能证成功。
- `SM_BUTCH(637)`：`Operate.cs:749` 在函数尾、所有门之外（卡 `openmir2-butch-637-criterion-fix`）。

### 3D 3A 的实测结果（2026-10-25 12:2x 执行，reviewer-20 comment `434f6c81`；逐根 `E:/tmp/rev20-npc/recompare/summary.json`）

37 个配对根（31 有旧报告：30 份当前 schema + 1 份旧格式 stub；6 无旧报告），基线 master `704842738`、rules 51 条。

1. **帧面零变化**：30 个根的 `frame diffs` 数**逐个未变**（含 `e23-reshot` 的 48 条）
   ⇒ **规则改动对帧面结论零影响，3A 的「帧面重算」这部分可以跳过**。
2. 16 个根三元组有变化，**全部源于非帧面**：
   - **DB 时间见证**：25 个根出现 `time_witness_stale`，delta 集中在 **28762–28770s ≈ 8h**（TZ bug 指纹），另 3 个根有 24 万秒级更旧行
     ⇒ 10 个根 PASS→FAIL 而 **frame diffs 仍为 0**。这是**工具现在把旧 DB 证据正确标红**，不是新缺陷。
   - **反证**：**12 个根完全没有见证红**（`db-rerun/run2/*` 六条、`e23-reshot`、五个 rules-teeth 夹具）⇒ 见证门在修后证据上不误报。
   - **`ident 14`（`SM_HIT`）的 ignore 规则仍 `approved_by: PENDING`**，本次两侧剔除数对不上（如 oracle=27 / rust=0）
     ⇒ 9 个根新增 `pending_ignore_rule`、其中 5 个 FAIL→UNDECIDABLE（**frame diffs 一条没变**）。
     该规则的转正**仍需 R 级窗口证据**（reviewer-2 的原裁定），本轮**不转正**；已登记进交付收口清单，代价是这 5 个根暂时停在 UNDECIDABLE。
3. **唯一帧面变多的根**：`w1c/castle_merchant_tax_potion` —— 旧 `report.json` 是**旧格式 stub**（frames 段空、rules=null），
   「旧 0 diffs」不是真结论。新跑 4 条：645 `param` 14 vs 0（Rust 空货架，E 类缺陷已由 castle-potion 卡修）、645 body_len 414 vs 0、
   ident 200 vs 651（错位）、block_align（oracle 112 vs rust 33）。
   ⇒ **不必重跑**，交付表里标「修前快照，已被 `e23-reshot` 取代」。

**对 §5 重跑清单的修订**：离屏「帧面」项可**跳过**（实测零变化）；仍必须重跑栈的是 ①DB 面 ②butch 链 ③`bag_equip`/`eat_item` ④home 卡合并后的 `move_walk`/`login_only`。
`card_evidence` 需在 `29cfa197`（criteria follow-up 落地）之后重算一次（判据 27→30 条）。

### 3E `card_evidence` 重算（master `c7ce673a9` 的 runner+表，含 F1–F4；旧判定用 `90572f5e2` 整套重跑，未混用）

产物 `E:/tmp/window/recompute/`；68 个保留根、81 行 scenario-matching。

- **快照**：`c7ce673a9`（2026-10-25 12:34 +08）；本节全部数字取自该时点。

| | PASS | FAIL | UNDECIDABLE |
|---|---:|---:|---:|
| 旧 | 29 | 1 | 51 |
| 新 | 29 | 1 | 51 |

⇒ **transitions = 0**：判据表收紧在**现有保留证据根**上**没有改变任何 verdict**。原因是没有任何保留根落在
「登录单帧 5008 / 767 无前置 643 / 657 无 52 前序 / 需账本」这些形态上。
**它的价值在结构守卫与构造复现**（`f4-repro/root` 单帧 5008：旧 PASS / 新 UNDECIDABLE / 新 runner+旧表 rc=2），
**不在回溯纠错**——这一点必须写清楚，否则会让人以为 29 个 PASS 被重新验证过。

UNDECIDABLE 51 行的构成：**人工（`machine=false`）31** / **帧序 ident 未齐 18** / ident 未齐或计数不足 2。
后两类（20 行）属**证据缺口**（帧对齐未落地，见 §3A 第四行），不是判据缺陷。

- **快照**：`c7ce673a9`（2026-10-25 12:34 +08）

`run2`（唯一新口径 DB 面证据，12 根）14 行：新 **PASS 4 / UNDECIDABLE 10 / 失败 0**（与旧一致）；4 个 PASS = 每侧 `529` 与 `657×2`。

`ident 14` 第三态：扫到 4 个 compare report 带 `pending_ignore_rule`（`e23-reshot`、`harness-proof/r2`、
`w1c/dice_chain_auto`、`w1c/move_walk`），**单列为「因已知未决规则而不可判」**，既不记缺陷也不记通过。

**`ident 14`（`SM_HIT`）的性质已查明（reviewer-20 comment `54fcdc51`）：(b) 纯 RNG + 「修前截断伪影」，不是行为缺口。**

- Rust **并非恒 0**：`e23-reshot/rust` 有 **38 条**（recog=80 商人）、`db-rerun/run2/move_walk/rust` 有 2 条（该根反过来 csharp=0 / rust=2）⇒ **不对称两个方向都出现过**，是 RNG 特征。
- 同场景同时长对照：`e23-reshot` 两侧各 ≈625s ⇒ C# 31 / Rust 38 条。
- 所谓 `oracle=27 / rust=0` 出在 `w1c/castle_merchant_tax_potion`：**C# wall=626s、Rust wall=8.2s**——Rust 在 step 5（`wait_idents [650,200]`）就收工，因为**修前**货架为空买不成。商人受击是 `Random(50)==0` 每次 `Run` 抽一次，8s ≈ 26 次 Run ⇒ 零命中概率 ≈ 59%。**是截断造成的伪影。**
- 代码面三条产出路径齐备（`lib.rs:18903` / `:19010` / `world/merchant.rs` 的随机支 + 城堡商人 `FixedHideMode` 解除的**确定性**补发，配两颗牙）。`lib.rs:14902`「无展示表 ⇒ 跳过」只是登记兜底，**AOI 假说被 e23 的 38 条否掉**。

⇒ **不开卡**；`ident 14` 保持 PENDING 的裁决不受影响（9 个根的 UNDECIDABLE 是 fail-closed，没有任何 PASS 依赖它）。
两条待办（已派 tester）：① PENDING ignore 的 `reason` 补一句「计数不可比——两侧 `wall_ms` 可差两个数量级，随机支要跑够久才累积；`27/0` 已查明为修前截断伪影」；② 工具侧建议「两侧 `wall_ms` 相差 > N 倍 ⇒ 直接 `undecidable(时长不可比)`」。

## 4. 结构性风险（未修，登记）

1. **`MakeIndex` 没有在 DB 段掩掉**：`characters_bagitem.MakeIndex` 属 D-1（`mir2-rs/tests/parity/whitelist.md`：
   C# `M2Share.GetItemNumber()` 含墙钟，两次 C# 运行都不一致），但 DB 对比没有对应掩码 ⇒ **每次 DB 面都会带一条永不消的差异**。建议开小卡。
2. **帧对齐未落地**（§3A 第四行）：跨多帧块的 diff 结论在实现前都要人工复核。
3. **判据表 ident 名映射有误（卡面）**：`openmir2-criteria-table-hardening` 的 `also_required` 写「`SM_CHARSTATUSCHANGED(654)`」——
   实际 `SM_CHANGELIGHT = 654`（`src/OpenMir2/Messages.cs:316`），`SM_CHARSTATUSCHANGED = 657`（`:319`）。守卫名单照抄会漏 657、误纳 654。

## 5. 建议的最小重跑清单（按代价从低到高）

1. **离屏**：用当前 `rules.json` 把所有保留的 `frames.ndjson` 重算一遍 `compare_runs`（0 窗口成本）。
2. **离屏**：用当前 `card-criteria.json` 重算 `card_evidence`（0 窗口成本）。
3. **栈**：DB 面重跑（save-settle + TZ + 见证三件都已合，`db-rerun/run2` 已示范）。
4. **栈**：butch 链（新夹具 + 新选靶 + `@last_kill_actor/x/y` 令牌）。
5. **栈**：`bag_equip` / `eat_item`（`ani_count`）。
6. **栈**：home 卡合并后 `move_walk` / `login_only` 的 `characters.Home*`。

## 6. 本盘点的证据边界

- 只读：未跑 cargo、未起栈、未占端口、未改任何文件。
- 时间以各证据根 `run.json` 的 `start_wall` 与 git 提交的**作者时间**为准（±数分钟）；跨过提交时刻的批次一律按「旧口径」处理，不猜工作副本何时更新。
- 未覆盖：A 类 49 卡里 `machine=false` 的行（本就 UNDECIDABLE，不构成假绿风险）。
