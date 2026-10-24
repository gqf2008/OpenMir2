# 剩余工作盘点（2026-10-25 刷新）

> 取代 `docs/rewrite-remaining-inventory-20261022.md`（旧稿保留只读；其 §3.2 B 表 22 行与
> §3.2.1 B-补三行以本文件为准）。
> **快照**：`master = f6b3143b0` @ 2026-10-25 01:30 +0800（本地；walgit 主源故障中）。
> **行号一律按本快照重新 grep**，不沿用旧稿。判定口径沿用旧稿：**仍成立 / 描述失真 / 部分失真 /
> 已实现欠窗口**；新增四桶：**可派 L / 可派 R / 需窗口 / 需 owner 裁定**。

## 0. 一句话结论

旧 B 表 22 行里，**今天有 3 行从「接线未做」变成「已实现欠窗口」**：`castle-pure`、
`loader-fields`、`castle-tax-item-design`（后者设计早已合 `d10b07f4c`，实现随 `99156c2c6`
收口）。`script-playdice` 记为**部分**——⑨⑪ 已收、⑩②③ 已收，但 ⑩① 仍缺（见 §4.1 的 L 卡）。
真正仍是「代码未做」的只剩 **`castle-official-design`**（+ 机器人 `RobotProcessor` / `DROPGOLD` /
`T-127` 三处已知残余）。其余全部收敛为 **R 级真机窗口**或 **owner 裁定**。条件② 的机判只剩
**1 张需人工（G11）**，见 §5。

---

## 1. 重点重算：今天四次合并改变了什么

### 1.1 `99156c2c6` 商税四调用点接线 → `castle-pure` / `castle-tax-item-design`

- `grep -rn 'merchant_tax_intent' mir2-rs/bins | wc -l` ⇒ **5**（旧稿为 0）；
  `grep -n 'let _tax' shop.rs lib.rs` ⇒ **0**（旧稿 4 处丢弃）。
- 新消费者：`apply_merchant_tax` `mir2-rs/bins/game-svr/src/shop.rs:1581`；`apply_merchant_tax_to_world` / `apply_upgrade_weapon_tax`
  `mir2-rs/bins/game-svr/src/lib.rs:5385/5399`，升级调用点 `mir2-rs/bins/game-svr/src/lib.rs:5902`。
- **castle-pure 剩余**：W1 沙巴克商人买/卖/修/升级后 `TotalGold/TodayIncome`、W2 城主行会成员价、
  W3 `GetAllNpcTax=1` 三窗真机对拍（R 窗口）——**不再有接线工作**。
- **castle-tax-item-design 剩余**：设计已合 `d10b07f4c`；实现随本合并收口（`GetCastle` 前置
  = `mir2-rs/bins/game-svr/src/lib.rs:5575` 的 `merchant_castle_index(...).filter(...)`；`MasterGuild` **不是**税前置，
  合同测试已钉）⇒ 只剩 R 窗口，无新实现项。

### 1.2 `9a7b8c798` loader-fields → `openmir2-loader-fields`

- `homemap::load_start_points`（`bins/game-svr/src/homemap.rs:155`）现在**消费**
  `crates/envir::startpoint`；`mir2-rs/bins/game-svr/src/lib.rs:559` 调用它。旧稿的
  `grep -rn 'envir::startpoint' mir2-rs/bins mir2-rs/crates/world | wc -l` 现为 **5**（旧为 0）。
- **剩余**：R 真机窗口（8 列承载/出生点与 oracle `LocalDB.LoadStartPoint` 逐字节）+ 读取失败
  fail-closed 的真机复核；**无接线项**。

### 1.3 `bff927e11` playdice T-142 残余 → `openmir2-script-playdice-openybdeal`

- ⑨ GAMEPOINT：`mir2-rs/bins/game-svr/src/npc.rs:1580` `set_game_point` 已覆写；写回 + 存档已接（`docs/playdice-gamapoint-residual-20261025.md`）。
- ⑪ `lable_is_can_jmp`：引擎单实现 `crates/script/src/engine/mod.rs:857`；`mir2-rs/bins/game-svr/src/shop.rs:75` 仅 `pub use`。
- ⑩：`@back` 两轮 `GotoLable` 合并排水（`mir2-rs/bins/game-svr/src/lib.rs:5688` `is_back`）；`game_gold_changed`
  改 `SendUpdateMsg` 队首-only（`mir2-rs/bins/game-svr/src/npc.rs:1539`）。
- **剩余**：① 整 handler 队列（`out` 面板/关闭帧与脚本帧统一；需逐帧区分 `SendSocket`/`SendMsg`）
  = **可派 L 新卡**；R 真机窗口（GAMEPOINT 落库 / 5008 / `@back` 帧序）。

### 1.4 `castle-tax-item-design` 只剩什么

- 设计（`d10b07f4c`）与实现（`99156c2c6`）都已合；`GetCastle/MasterGuild 前置` 已在调用侧判定。
- **只剩** W1–W3 真机窗口；不再需要设计或接线卡。

---

## 2. B 表 22 行重算（当前 master `f6b3143b0`）

> 旧稿行号见 `docs/rewrite-remaining-inventory-20261022.md` §3.2（doc:419–440）；代码行号以下均为
> 本次重新 grep。判定列：`仍成立`＝未做；`部分`＝一部分已落地；`已实现欠窗口`＝代码已落地、
> R 级真机窗口未跑（**不得据此 closed**）。

| # | 卡 | 10-24 判定 | 10-25 现状（证据，当前行号） | 新判定 | 桶 |
|---:|---|---|---|---|---|
| 1 | `castle-pure` | 仍成立 | 99156c2c6：5 个消费者；`let _tax`=0；`mir2-rs/bins/game-svr/src/shop.rs:1581`、`mir2-rs/bins/game-svr/src/lib.rs:5399/5902` | **已实现欠窗口** | 需窗口 |
| 2 | `script-playdice-openybdeal` | 描述失真 | T-142 ⑨⑪ 已收、⑩②③ 已收；`mir2-rs/bins/game-svr/src/npc.rs:1580`、`mir2-rs/crates/script/src/engine/mod.rs:857`、`mir2-rs/bins/game-svr/src/lib.rs:5688`；⑩① 仍缺 | **部分** | 需窗口 + 可派 L（⑩①） |
| 3 | `market-def-design` | 已实现欠窗口 | `mir2-rs/crates/script/src/engine/actions.rs:889/905`；`mir2-rs/crates/envir/src/merchant.rs:217` | 已实现欠窗口 | 需窗口 |
| 4 | `castle-tax-item-design` | 描述失真 | 设计 `d10b07f4c`；实现 `99156c2c6`；§1.4 | **已实现欠窗口** | 需窗口 |
| 5 | `castle-official-design` | 仍成立 | `mir2-rs/bins/game-svr/src/lib.rs:4746/5558` 仍为「CastleOfficial 门未移植（T-40/T-140）」零帧分支 | **仍成立** | 可派 R |
| 6 | `loader-fields` | 仍成立 | 9a7b8c798：`mir2-rs/bins/game-svr/src/homemap.rs:155` 消费 `envir::startpoint`、`mir2-rs/bins/game-svr/src/lib.rs:559` | **已实现欠窗口** | 需窗口 |
| 7 | `castle-war-area-design` | 描述失真 | `mir2-rs/crates/world/src/castle.rs:2234` 每 tick 766 bit4 已实现；T-125..T-132 残余 | 已实现欠窗口 + 残余 | 需窗口 + 可派 L（T-131/T-132） |
| 8 | `dbsrv-connection-design` | 描述失真 | `mir2-rs/bins/db-srv/src/db.rs:111` 方案 A（去全局 Mutex）已实现 | 已实现欠窗口 | 需窗口（LB-10-E1） |
| 9 | `run-gap-mb2` | 仍成立 | 板面 work=`车道在飞（wf_6845f065-267）`；lane 存活需进程/心跳 | **仍成立** | 需 owner（车道去留） |
| 10 | `monster-run-overrides` | 描述失真 | `mir2-rs/crates/world/src/monai.rs:709/6700` 已实现；`RobotProcessor` 仅注释（`mir2-rs/bins/game-svr/src/lib.rs:16001`） | 已实现欠窗口 + P5 | 需窗口 + 可派 R（RobotProcessor） |
| 11 | `spell-frame-bypass-paths` | 部分失真 | ② `mir2-rs/crates/world/src/monai.rs:6792/6798/6804` 已实现；① 机器人路径仍缺（`mir2-rs/bins/game-svr/src/lib.rs:16001`） | **部分** | 可派 R（与 #10 同卡） |
| 12 | `inbound-dispatch-red-batch` | 部分失真 | `mir2-rs/bins/game-svr/src/lib.rs:4638/5003` 线缆映射已接、`run_merchant_goto` `mir2-rs/bins/game-svr/src/lib.rs:4768/5648`；出帧发射点+DROPGOLD 仍缺 | **部分** | 可派 L（发射点）+ 可派 R（DROPGOLD RNG） |
| 13 | `envir-redlist-batch` | 部分失真 | `mir2-rs/crates/data/src/lists.rs:106/389`；`mir2-rs/crates/envir/src/noclearmonlist.rs:34`；T3 四牙已合 `0bac2ed2d` | **部分**（消费点待核） | 可派 L（复核）+ 需 owner（cards W3） |
| 14 | `m3-selloff-consignment-world` | 描述失真 | `mir2-rs/crates/script/src/engine/actions.rs:905`、`mir2-rs/bins/game-svr/src/npc.rs:1570` 已实现 | 已实现欠窗口 | 需窗口 |
| 15 | `m3-guild-war` | 描述失真 | `mir2-rs/bins/game-svr/src/lib.rs:21679`（9 条 CM_GUILD*）、`mir2-rs/bins/game-svr/src/lib.rs:23352`（10s Run） | 已实现欠窗口 | 需窗口 |
| 16 | `m3-castle-file-load` | 描述失真 | `mir2-rs/bins/game-svr/src/castle.rs:987`、`mir2-rs/bins/game-svr/src/lib.rs:20613` | 已实现欠窗口 | 需窗口 |
| 17 | `m3-guild-file-store` | 描述失真 | `mir2-rs/bins/game-svr/src/guild_runtime.rs:120/356` | 已实现欠窗口 | 需窗口 |
| 18 | `m3-castle-door-repair` | 描述失真 | `mir2-rs/crates/world/src/monai.rs:5170` 每 tick、`mir2-rs/crates/world/src/castle.rs:329` | 已实现欠窗口 | 需窗口 |
| 19 | `m3-guild-state` | 描述失真 | `mir2-rs/bins/game-svr/src/lib.rs:21679`、`mir2-rs/bins/game-svr/src/lib.rs:20194`（startup） | 已实现欠窗口 | 需窗口 |
| 20 | `m3-castle-gold-tax` | 部分失真 | 商税四点已接（`let _tax`=0）；装载 `mir2-rs/bins/game-svr/src/castle.rs:987` | **已实现欠窗口** | 需窗口 |
| 21 | `m3-castle-siege-state` | 部分失真 | `mir2-rs/bins/game-svr/src/lib.rs:23391` run_tick、`mir2-rs/bins/game-svr/src/lib.rs:23417` Save；`mir2-rs/bins/game-svr/src/lib.rs:23426` 其余 CastleEvent 未接线（T-127） | **部分** | 可派 R（T-127）+ 需窗口 |
| 22 | `m3-party-attribution` | 仍成立 | 夹具已完整（`run_party_trade_pair.ps1` 等）；只差两侧同序列真机 | 已实现欠窗口 | 需窗口 |

**计数**：仍成立 2（#5、#9）／部分 5（#2、#11、#12、#13、#21）／已实现欠窗口 15（其余）；旧稿的
「未开工/接线未做」表述在 20/22 行已被代码推翻（较 10-24 的 17 行进一步收敛）。

## 3. §3.2.1 B-补三行（无卡可派的代码面）

| 面 | 现状（当前行号） | 判定 | 桶 |
|---|---|---|---|
| `SETOFFLINE` AutoGetExp* 四字段 | `mir2-rs/bins/game-svr/src/npc.rs:1796` 覆写；`mir2-rs/crates/script/src/engine/actions.rs:680` 调用 | 已实现欠窗口 | 需窗口 |
| `SETMPAMODE DECHP` / `sys_msg` | `mir2-rs/crates/script/src/engine/actions.rs:665/678`；`mir2-rs/bins/game-svr/src/lib.rs:18857` `drain_player_map_hp`；`mir2-rs/bins/game-svr/src/run_segments.rs:41` `map_hp_tick` | 已实现欠窗口 | 需窗口 |
| `Market_Def` 商人脚本装载/说词 | `mir2-rs/bins/game-svr/src/lib.rs:7307` `run_merchant_goto`、`:7440` `emit_merchant_goto_result`；`mir2-rs/crates/envir/src/merchant.rs:217` | 已实现欠窗口 | 需窗口 |

三行均**不再建议新建实现卡**。

---

## 4. 四桶分类（仍未完成的工作）

### 4.1 可派 L（纯逻辑/工具/文档，不触 RNG/时序/持久化/战斗数值）

| 项 | 建议卡 | 依据 |
|---|---|---|
| T-142 ⑩①：整 handler `MsgQueue`（`out`/直连帧与脚本帧统一） | **新 L 卡**（`openmir2-script-handler-msgqueue`） | 需逐帧区分 `SendSocket` 直连 vs `SendMsg` 入队；部署当前 0 处触发（`docs/playdice-gamapoint-residual-20261025.md` §2.2） |
| castle-war T-131 `MasterGuild` 装载相位 / T-132 `CastleNameColors` 配置源 | 归 `openmir2-castle-war-area-impl`（L 片） | 装载/配置/状态建模，不是窗口项（旧稿 §3.2 行 7） |
| envir-redlist 消费点复核（T3 四牙已合 `0bac2ed2d`） | 归 `openmir2-envir-redlist-batch` 剩余 L 片 | `mir2-rs/crates/data/src/lists.rs:106/389`、`mir2-rs/crates/envir/src/noclearmonlist.rs:34` |
| inbound-dispatch 各 handler 出帧发射点 | 归 `openmir2-inbound-dispatch-red-batch`（L 片） | `mir2-rs/bins/game-svr/src/lib.rs:4638/5003` 已接，发射点未逐帧列 |

### 4.2 可派 R（实现，触 RNG/时序/持久化/战斗数值）

| 项 | 建议卡 | 依据 |
|---|---|---|
| `castle-official-design`：CastleOfficial 门 + 固定 NPC 对话层 | 原卡（`openmir2-castle-official-design`） | `mir2-rs/bins/game-svr/src/lib.rs:4746/5558` 零帧分支；T-40/T-140 |
| `RobotProcessor` 未移植（P5 机器人 AutoSpell） | 原卡 `openmir2-monster-run-overrides`（R 片） | `mir2-rs/bins/game-svr/src/lib.rs:16001`；阻塞 #11 ① |
| `DROPGOLD` 深体 RNG（inbound-dispatch） | 归 `openmir2-inbound-dispatch-red-batch`（R 片） | 旧稿行 12；RNG 抽数序列 |
| `m3-castle-siege-state` T-127：CastleEvent 落帧/时序 | 原卡（R 片） | `mir2-rs/bins/game-svr/src/lib.rs:23426` 「城堡状态机事件（未接线）」 |

### 4.3 需窗口（R 级真机证据；不占窗口的代码工作已完成）

`castle-pure`（W1–W3）、`castle-tax-item-design`、`market-def-design`、`loader-fields`、
`castle-war-area-design`（R 部分）、`dbsrv-connection-design`（LB-10-E1）、`monster-run-overrides`、
`m3-selloff-consignment-world`、`m3-guild-war`、`m3-castle-file-load`、`m3-guild-file-store`、
`m3-castle-door-repair`、`m3-guild-state`、`m3-castle-gold-tax`、`m3-party-attribution`、
`script-playdice-openybdeal`（GAMEPOINT/5008/@back）、§3.2.1 三行。

窗口口径与场景：`docs/window-run2-prep-20261025.md`（17 场景 + `run_batch.sh`）；
排期与覆盖：`docs/rcard-evidence-plan-20261025.md` §5。

### 4.4 需 owner 裁定

| 项 | 入口 |
|---|---|
| G11（`gm-batch-5b-cross-server` 排除/保持/实施） | `docs/needs-human-20261025.md` §1（推荐 A） |
| Q1 窗口授权（17 场景，A/B/C） | 同页 §2（推荐 A） |
| Q2–Q5（无场景 62 张 / combat 夹具 / RNG flaky / 登记豁免） | 同页 §3（推荐 D/A/丙/A） |
| `run-gap-mb2` 车道存活与去留 | 板面 work；需进程/心跳或重派 |
| `envir-redlist-batch` 的 `cards.json` W3 分组（原文无法复算） | 需 owner 指定分组口径 |
| rules.json PENDING / T-161..T-222 复核 / 私有 C# 栈去留 | `docs/needs-human-20261024.md` §0（旧页仍适用） |
| walgit 恢复策略（等 R2 / GitHub 临时主源） | `docs/needs-human-20261025.md` §4 |

---

## 5. 条件② 机判快照（与 goal_gate 一致性）

`goal_gate` @ `f6b3143b0`（`generated_ts 1792863386`，快速模式 `--skip-tests --skip-script`；①/④ 用同提交补充证据，见下行）：

| 条件 | 状态 | 实测 |
|---|---|---|
| ① 脚本派发 100% | **pass** | `parse-stats`：已实现 **106/106**、调用 **11829**、未实现 **0**（`E:/tmp/scriptcov/parse-stats-20261025.stdout.txt`） |
| ② R 卡证据 | **need_human** | R 级 **207**：closed 12 / `window_required` **141**（另未合并 **2**）/ whitelist 48 / 已裁定 3 / 三者皆无 0 / **merged 未 closed 需人工 1（仅 G11）** / 其它 0；卫生清单 **190** |
| ③ B 类清零 | **pass** | recon **tracked=0**（@ f6b3143b0；rows 191 条非空、counts 一致） |
| ④ 门禁恒绿 | **need_human** | 7 项：绿 **6** / 红 0 / 需人工 **1**（仅 `master_sync`：walgit 500）；`cargo test --workspace` **3190 passed / 0 failed** @ `bff927e11`（同一提交族，`E:/tmp/scriptcov/test-bff927e11.txt`） |

> 与 `docs/needs-human-20261025.md` §5 同口径；唯一真缺口 = G11（§4.4）。

---

## 6. 今日 12 交付（本地 `master`，2026-10-25 00:02–01:30）

| # | 提交 | 内容 | tier |
|---:|---|---|---|
| 1 | `0183dca15` | window-run-2 续跑准备包（`run_batch.sh` + 私栈两坑自检） | L |
| 2 | `43472c8a4` | window-prep 报告两处 Minor（直接 docs） | L |
| 3 | `87dc9847b` | R 卡证据三分类 + 裁定包（A49/B62/C26） | L |
| 4 | `0be3407a2` | check_pickup_unreachable P-1..P-6 | L |
| 5 | `cb646b65a` | run3 场景清单 18→17（封存 pickup） | L |
| 6 | `a6fc02d0a` | check_pickup_unreachable P-7 var 别名 | L |
| 7 | `0bac2ed2d` | envir redlist T3 四颗常驻牙 | L |
| 8 | `99156c2c6` | 商税四调用点接线（`merchant_tax_intent` 消费） | **R** |
| 9 | `9a7b8c798` | loader-fields：game-svr 消费 `envir::startpoint` | **R** |
| 10 | `bff927e11` | playdice T-142 ⑨⑩(子集)⑪ | **R** |
| 11 | `e0ccf5d5b` | 条件② 5 张 merged_not_closed 逐张裁定 | L |
| 12 | `f6b3143b0` | owner 决策页 2026-10-25（G11/Q1/Q2–Q5/walgit/快照） | L |

> 11 个 merge + 1 个直接 docs 提交 = **12**；其中 R 级 3 张（#8/#9/#10），合并后保持
> `in-progress`，窗口复拍后 closed。

---

## 7. 复算命令（本文件所有数字）

```sh
# 快照
git rev-parse --short HEAD                      # f6b3143b0
python -I tools/goal/goal_gate.py --full --json # 条件快照（见 §5）

# 行号（节选；全部为本次重新 grep）
grep -rn 'merchant_tax_intent' mir2-rs/bins | wc -l                 # 5
grep -n 'let _tax' mir2-rs/bins/game-svr/src/shop.rs mir2-rs/bins/game-svr/src/lib.rs  # 无输出
grep -n 'pub fn apply_merchant_tax' mir2-rs/bins/game-svr/src/shop.rs                  # 1581
grep -n 'apply_upgrade_weapon_tax' mir2-rs/bins/game-svr/src/lib.rs | head            # 5399/5902
grep -n 'pub fn load_start_points' mir2-rs/bins/game-svr/src/homemap.rs               # 155
grep -rn 'envir::startpoint' mir2-rs/bins mir2-rs/crates/world | wc -l                # 5
grep -n 'fn set_game_point\|fn game_gold_changed' mir2-rs/bins/game-svr/src/npc.rs    # 1580/1539
grep -n 'pub fn lable_is_can_jmp' mir2-rs/crates/script/src/engine/mod.rs             # 857
grep -n 'pub use mir2_script::engine::lable_is_can_jmp' mir2-rs/bins/game-svr/src/shop.rs  # 75
grep -n 'is_back' mir2-rs/bins/game-svr/src/lib.rs                                    # 5688
LC_ALL=C.UTF-8 grep -nP '(*UTF)CastleOfficial 门未移植' mir2-rs/bins/game-svr/src/lib.rs  # 4746/5558
```

残留物：`E:/tmp/gate-inv.json`、`E:/tmp/board-inv.json`（本次复算中间件，可删）；
`E:/tmp/gate-inv-full.json`（若完成则保留作 §5 依据）。本文件与旧稿指针是唯一仓库改动。
