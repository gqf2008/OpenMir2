# 窗口 kit 覆盖核对：台账 6 张卡「当场可执行」清单（2026-10-25）

- 卡：`openmir2-window-run-2` 的 L 子片（不单开卡；owner `openmir2-tester-1`，branch `card/window-kit-coverage`）。
- 目的：把 `docs/window-closure-ledger-20261025.md` 6 行的 W 项逐项对到 **kit 里真实存在的场景/夹具/命令**，
  缺的写明**手工步骤**或**新增场景提案**，把「窗口当场能跑完」从赌变成可执行清单。
- 方法：只读仓内 + 部署件（`E:/MirServer`），**未起栈、未占端口、未连库**；未跑全量测试（内存口径）。
- kit 基线：`tools/window/run3-scenarios.json`（17 场景）、`tools/conformance/card_window.py`（profile/夹具/DB 快照）、
  `tools/probe/scenarios/*.json`、`mir2-rs/tests/parity/m3/*.ps1`（pair 夹具）、`tools/gw-capture/gate_shadow_proxy.py`、
  `tools/conformance/rules.json`、`tools/dbsnap/dbsnap.py`。
- 标记：**现成** = 现有 kit 直接可跑；**部分** = 有相近场景但要改/加前置；**缺** = 没有对应资产，必须手工或新场景。

## 0. kit 能力边界（先看清限制，再看逐卡）

| 能力 | 现状 | 证据 |
|---|---|---|
| 单客户端场景驱动 | 有：17 场景 + probe 任意 `send/click_npc/dlg_select` | `tools/window/run3-scenarios.json`；`tools/probe/scenarios/` |
| 双栈对拍 | 有：`card_window.py`（每侧 restore→起栈→probe→DB 快照→compare） | `tools/conformance/card_window.py` |
| DB 后态快照 | 有：13 张表（含 `characters`、`characters_ablity`、`characters_bagitem` 等） | `card_window.py` 的 rust/db 段 |
| 资产前置声明 | **仅 `Gold`**（`ASSETS_COLUMNS = ("Gold",)`） | `card_window.py:2980` |
| 背包/魔法夹具 | 有：`inventory`、`magic` 声明（window-magic profile） | `card_window.py` 背包/魔法段；`tools/conformance/fixtures/spell_cast` |
| 出生点（新角色） | **缺**：probe 没有 `CM_NEWCHR` 步骤（协议常量存在 `CM_NEWCHR=101`） | `mir2-rs/crates/protocol/src/messages.rs:35`；`tools/probe/mir2_probe.py` 无 newchr 路径 |
| 可注入时钟 | Rust 有（`MIR2_CLOCK_MS`）；**C# 侧零命中** | `mir2-rs/bins/game-svr/src/clock.rs`；`grep -rIl MIR2_CLOCK src/` = 0 |
| RNG 流水账本 | 两侧都能采（Rust env；C# `rng-hook/StartupHook.cs`），但**窗口私栈启动脚本未接线 `-RngLog`** | `mir2-rs/tests/parity/m3/rng-hook/StartupHook.cs:207`；`E:/tmp/conformance/start-rust.ps1` 与 `tools/m2-acceptance/start_mir2_server.ps1` 无 RNG 参数 |
| 多会话/观察者 | **缺**：`card_window` 每侧只起一条 probe；`run_pair.ps1` 可两假人但不驱动协议帧序判据 | `card_window.py` 单 probe；`mir2-rs/tests/parity/m3/run_pair.ps1` |
| 文件/字段级对拍（非帧） | 无内建：需手工 sha256/字段比对（`SabukW.txt` 等） | `card_window.py` 只比帧 + DB 表 |

## 1. 逐卡覆盖

### 1.1 商税 `99156c2c6`（W-TAX-1..4）

| W 项 | kit 资产 | 状态 | 缺什么 / 手工步骤 |
|---|---|---|---|
| W-TAX-1 四路径双栈 | 无城堡商人场景；`merchant_dialog` 点的是铁器店卫家店（0103），不是城堡商人 | **缺** | 手工：窗口专用配置副本（`CastleMerchant=1`、`CastleGetAllNpcTax` 按 W-TAX-2 分开跑）+ 客户端城堡商人买/卖/修/升级四次；`gate_shadow_proxy` 抓帧；手工比 `Castle/0/SabukW.txt` 的 `TotalGold`/`TodayIncome`/`SaveTick`。新场景提案：`castle_merchant_tax`（placement 到城堡商人邻格 + 4 个 `dlg_select` + `wait_for` 金币帧） |
| W-TAX-2 `GetAllNpcTax=1` 分支 | 同上 | **缺** | 手工：第二份配置副本（`CastleGetAllNpcTax=1`），两座城（`Castle/List.txt` = 0/1）各入账一次 |
| W-TAX-3 前置（不在攻城区/非城主） | 无 | **缺** | 手工摆位/换号；零税与仍收税两态各一次 |
| W-TAX-4 日上限/`SaveTick` 10min 门 | 需要同 tick 时钟 | **缺（被 C# 时钟注入阻塞）** | C# 侧 `MIR2_CLOCK_*` 零实现（`src/` 0 命中）⇒ ⑧-1 C# 半未落地前只能观察「同窗口内不刷新」，不能逐位对 `SaveTick` |

### 1.2 loader-fields `9a7b8c798`（W-LF-1..2）

| W 项 | kit 资产 | 状态 | 缺什么 / 手工步骤 |
|---|---|---|---|
| W-LF-1 出生点 11 键与落点 | `login_only` 用**既有角色**（基线 restore），不触发新建角色出生点 | **部分/缺** | 缺 `CM_NEWCHR` 驱动。手工：窗口内用客户端建新角色（三种职业各一）进图，记首包地图/坐标/朝向；或新提案：给 probe 加 `newchr` 步骤（`CM_NEWCHR=101` 已在协议表） |
| W-LF-2 5 新列消费 | 无消费点（卡面自述：`MakeSafePkZone`/`InSafeZone` 属后续片） | **构造级**（非窗口） | 构造测试已覆盖解析/回落/RNG（`mir2-rs/bins/game-svr/src/homemap.rs` 的 `#[test]` 组）；窗口不列；**当 `MakeSafePkZone`/`InSafeZone` 在服务端被消费（接线片合入）时重开为窗口项** |

### 1.3 playdice T-142 `bff927e11`（W-PD-1..2）

| W 项 | kit 资产 | 状态 | 缺什么 / 手工步骤 |
|---|---|---|---|
| W-PD-1 GAMEPOINT 落库 + 5008 | 无场景点 `真爱-G002`；`dice_chain` 点的是赌场 `赌场-B013`（`@kaiz/@ya5/@ya52`，不含 GAMEPOINT） | **缺** | 部署可达链：`真爱-G002` `[@main]` → `@yhua` → `@9hua`（`GAMEPOINT - 9`，前置 `CHECKGAMEPOINT > 8`；`Market_Def/功能NPC/真爱-G002.txt` 第 105/520 行段）。新场景提案：`gamepoint_exchange`（placement 到 `Merchant.txt:127` 的 G002/49,49 邻格；两步 `dlg_select @yhua → @9hua`；等 5008/643；DB 比 `characters.GamePoint`）。**阻塞**：`assets` 白名单只有 `Gold` ⇒ 前置 `GamePoint≥9` 无声明通道，需扩白名单（L 卡）或手工 DB 预置 |
| W-PD-2 `@back` 排水 | `npc_script_chain` 明确**不用 `@back`**（部署件无 `@back`，只有 `@back2`；见 `docs/npc-script-chain-scenario-20261023.md:64`） | **缺（按字面不可执行）** | 部署件 grep `@back>` = 0 命中；`真爱-G002`/`真爱666-G002` 只有 `@back2`（`[@back2]` 做 `mapmove`）。**需 dev 澄清**：W-PD-2 是否指 `@back2`；若指字面 `@back` 则为非目标/构造级 |

### 1.4 CLEARMAPMON `f08496192`（W-CM-1..4）

| W 项 | kit 资产 | 状态 | 缺什么 / 手工步骤 |
|---|---|---|---|
| W-CM-1 T118 | 无场景；部署触发点存在：`Market_Def/任务NPC/武士-0114.txt` `[@rw03]` → `CLEARMAPMON T118` + `MONGENEX T118 14 17 命运教主 2 1 1` | **部分** | 手工：`gate_shadow_proxy` 模板（`--map 7101=127.0.0.1:7100 --gate sel --direction s2c`）+ 客户端走到该 NPC 选到 `@rw03`；新场景提案：`clearmapmon_t118`（多步 `dlg_select`，仿 `npc_script_chain`） |
| W-CM-2 Q001 | 触发点：`Market_Def/任务NPC/老人-1.txt` `[@rw02]` → `CLEARMAPMON Q001` + `MONGENEX Q001 10 10 骷髅精灵5 2 1 1` | **部分** | 同上；新场景提案：`clearmapmon_q001` |
| W-CM-3 RNG 流水位置 | 窗口私栈未接 `-RngLog` | **缺（接线）** | 若部署出现「MONGENEX 后还有抽数动作」才可观测；当前 T118/Q001 不可观测 ⇒ 登记为观察项，不作关卡硬判据；**将来具备条件时（`-RngLog` 接线 + 部署出现「MONGENEX 后仍有抽数」脚本）重开为窗口项** |
| W-CM-4 失败门/切图序/Invisible | 构造级 | **构造级** | 已有单测；真机只在「落点与刷怪点重合」时才有差异，窗口不单独设步 |

> **不得假设 `cleared` 字段**：本库出现 `cleared` 字样的地方还有行会清槽日志（`lib.rs` 的 `guild` 清槽）
> 与 `crates/world` 的魔法/怪物 AI 同名局部量（`magic_effect.rs` 的 `target_creature_cleared`、`monai.rs` 的 `cleared_target`），均与 CLEARMAPMON 无关；CLEARMAPMON 的关卡证据必须是**实体集合/AOI 差集 + `SM_DISAPPEAR(30)` 帧序 + 刷怪位置/数量**，
> 不能读某个 `cleared` 标志。

### 1.5 T-127 `263e77d7e`（W-T127-1..5）

| W 项 | kit 资产 | 状态 | 缺什么 / 手工步骤 |
|---|---|---|---|
| W-T127-1 开战链帧序 | `king_castle_war` 场景只到「申请攻城缴金条门」（`@requestcastlewarA → @requestcastlewar`，644+767），**不进 UnderWar**；`run_guild_castle_pair.ps1` 无 castlewar 步骤（grep 0 命中） | **缺** | 需 castle fixture：窗口专用配置副本把 `StartCastlewarTime` 改到当前小时（`Server.conf:380`）+ `Castle/0/AttackSabukWall.txt` 置当日申请；`gate_shadow_proxy` 抓全服 `SM_SYSMESSAGE`/`SM_DIGDOWN`/`SM_USERNAME` |
| W-T127-2 结束预告/超时 | 同上 + 需要分钟级时钟 | **缺（被 C# 时钟注入阻塞）** | C# `MIR2_CLOCK_*` 未实现 ⇒ 只能挑真实时间点（如 21:50/22:00）窗口复拍，不能压时间 |
| W-T127-3 N1 观察者丢弃 | 需要第二客户端观察 | **缺** | `card_window` 单 probe；需手工双客户端（C# 侧一名观察者在攻城区），或用 `run_pair.ps1` 两假人 + 自定义帧判据 |
| W-T127-4 多城次序 | `Castle/List.txt` = 0/1（两城） | **部分** | 配置已具备；仍需 W-T127-1 的 fixture |
| W-T127-5 planes 零副作用 | 无 PlanesServer 连接即可 | **现成（观察项）** | 窗口内确认无 PlanesServer 连接 + oracle 日志无副作用 |

### 1.6 inbound `a3bf5340f`（W-DG-1..2）

| W 项 | kit 资产 | 状态 | 缺什么 / 手工步骤 |
|---|---|---|---|
| W-DG-1 `CM_DROPGOLD` 零抽数 | 无 drop 场景（`pickup_item` 已封存；`dropitem_ack` fixture 是 `CM_DROPITEM` 600/601 的**构造件**，非本项） | **缺** | 手工：客户端丢金币（≥`CanDropGold`、`< Gold`）+ `gate_shadow_proxy` 抓 653；RNG 流水需私栈接 `-RngLog`（当前 `E:/tmp/conformance/start-rust.ps1` 只传 `-Seed 42`）。新场景提案：`dropgold_zero_draw`（`send CM_DROPGOLD` + `wait_for 653`，可无 corpse） |
| W-DG-2 `CM_BUTCH` 两抽流位 | 无屠宰场景；需要动物尸体与后续抽数动作 | **缺** | 手工：杀动物 → 屠宰到骨架（33→637）+ 后续抽数动作；需要 RNG 账本接线。新场景提案：`butch_two_draws`（需先有「单只固定怪」夹具，与 drop-exp 夹具同依赖） |

## 2. 汇总（窗口前要补的清单）

| 优先级 | 缺口 | 影响 W 项 | 建议动作 |
|---|---|---|---|
| P0 | `assets` 白名单只有 `Gold` | W-PD-1 | 扩白名单到 `GamePoint`（L 卡，改 `card_window.py` + selftest）或窗口手工 DB 预置（登记为手工步骤） |
| P0 | 私栈启动未接 `-RngLog` | W-DG-1/2、W-CM-3 | 窗口启动脚本加 `-RngLog <path>`（工具面，改 `E:/tmp/conformance/start-rust.ps1` + `start_mir2_server.ps1` 参数） |
| P0 | C# 无 `MIR2_CLOCK_*` | W-TAX-4、W-T127-2 | ⑧-1 C# 半落地前，改为「真实时间点复拍」并登记降级口径 |
| P1 | 无城堡商人税场景 / no castle fixture | W-TAX-1..3、W-T127-1..4 | 新场景提案 `castle_merchant_tax` + `castle_war_frames`；配置副本走 `E:/tmp/conformance`，不改 `E:/MirServer` |
| P1 | 无 `CM_NEWCHR` 驱动 | W-LF-1 | probe 加 `newchr` 步骤（协议常量已有）或窗口手工建号 |
| P1 | 无 GAMEPOINT 场景 | W-PD-1 | 新场景 `gamepoint_exchange`（`真爱-G002` `@yhua→@9hua`） |
| P1 | `@back` 字面不存在 | W-PD-2 | dev 澄清是否指 `@back2`；否则转构造级/非目标 |
| P1 | 无多会话/观察者探针（`card_window` 每侧只起一条 probe） | W-T127-3（N1 观察者丢弃） | 双客户端手工，或 `run_pair.ps1` 两假人 + 自定义帧判据 |
| P2 | 无 CLEARMAPMON 场景 | W-CM-1/2 | 新场景 `clearmapmon_t118/q001`（触发点已定位） |
| P2 | 无 drop/butch 场景 | W-DG-1/2 | 新场景 `dropgold_zero_draw` + `butch_two_draws`（依赖单怪夹具） |

## 3. 结论

- **现成即可跑**：W-LF-1 的「既有角色进图」腿（`login_only`）、W-T127-5 观察项；
  其余 W 项都需要**新场景/夹具**或**窗口手工步骤**，不是「开窗即全跑」。
- 本轮不新建场景、不改 kit（只做覆盖核对）；上面的提案按优先级另派 L/R 卡。
- 只读证据：本文件引用的命令/文件均可在仓内复核；未起栈、未占端口、未连库。
