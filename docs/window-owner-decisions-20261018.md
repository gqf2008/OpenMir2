# 真机窗口：owner 决策清单与缩窗开窗步骤（2026-10-18）

> 本件给 owner 一页拍板用。**不含任何自动执行**；开窗本身需 owner 明确授权。
> 背景与逐项判据见 `docs/window-dryrun-20261017.md`（§7）、
> `mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/82-two-side-window-runbook.md`（§W0-00 / §2d / §2f）。

## 0. 现状（截至 `84b696b05`，2026-10-18）

- `master = origin/master = walgit/master = 84b696b05`（+ 文档提交），三端同步。
- **P3 全批（B1–B5）已完成**：26/26 个 actionable 魔号 + 共用基础设施（有序枚举、`Entity.mac`、
  `get_mag_struck_damage`、延迟队列、`SpellFrameBuf` 优先级、状态面），每批独立对抗复验多轮。
- 窗口前置已就绪：mirror `/e/tmp/m2svr-window-origin` 对齐基线、六件二进制在
  `E:/tmp/m2svr-window-target`、`window-provenance.json` 已重录、`guard` = **`WINDOW_GUARD_OK`**
  （九游戏端口全 DOWN、仅既有 mysqld:3306）。
- 门禁：112 suites / 1688 passed / 0 failed / 26 ignored；对账门 149 implemented / 38 tracked；
  锚点 176；五批 trace 全 `GROUP/ROW_COVERAGE closed`。
- **所有 R 级卡仍 `in-progress`**：卡上窗口项在真机窗口跑完前不得 `closed`。

## 1. 两条路线

| 路线 | 范围 | 能否 GO | 代价 |
| --- | --- | --- | --- |
| **整包 W0** | 缩窗 W0-1..W0-5 **+ RNG 两侧账本**（构造段 69 抽/18 站点、屏障 A `seq==17`、route `added=1649`、P3 逐魔号逐抽） | **当前 NO-GO**：RNG 两侧驱动未接线（`-Go -PinScene` 仍 `PREREQ_MISSING`），需先裁决 O1–O7 并实现承接卡 | 需改 `src/GameSrv/**`（T-* 白名单）+ C# 时钟注入 + 影子 Envir 策略 |
| **缩窗 W0-1..W0-5** | ③b 阳性对照 / Rust 同流程 / ③a 反向点 `(311,666)` / GuardList 帧 / 怪打玩家探针 | **前置已齐备，可 GO** | 不含 RNG 逐抽账本；P3 的取数流水仍只能构造级 |

## 2. O1–O7 决策表（整包 W0 的前置）

> 来源：`mir2-rs/tests/parity/m3/design-rng-scene-pin-path-20261017.md` §7。逐条只需回「批准 / 拒绝 / 改选」。

| # | 决策 | 推荐 | 拒绝的后果 |
| --- | --- | --- | --- |
| **O1** | 是否允许新增 **T-*** 白名单改 `src/GameSrv/**`（启动期场景 pin + 场景模式下推迟怪物线程启动） | **批准**（建议 T-73，编号以 `m3_whitelist_registry_audit.py` 为准） | 只能退回 hook 反射方案（时序不稳，R 级 flaky 风险） |
| **O2** | scenario 的 `dir=0` 是否保持；是否批准「生成后覆盖 `Dir`」（与 `LoadGuardList` 同形，0 抽） | **批准保持 + 覆盖** | scenario 得改成实际抽值，pin 的置向步骤失效，M1 牙失效 |
| **O3** | `monster_ai` 组的确定性前置：先落 **C# 时钟注入**（`MIR2_CLOCK_MS` C# 半）+ Rust 的 `RunTime=250ms` 门（B-L2），再开窗 | **批准先落两项** | `monster_ai` 预期 `SHARED_REL_ORDER_DIFF`，DoD② 不能算达成（或另立「固定 AI 抽数预算」替代口径） |
| **O4** | 非场景消费者（其它图的 Merchant/NPC/MapEvent/MonGen）的隔离：窗口用**影子 Envir**（`start-csharp-hook.ps1` 已是 robocopy 影子，**不改 `E:/MirServer` 原文件**） | **批准影子 Envir** | 全局 RNG 混入非场景抽取，`MISSING_EXPECTED_GROUPS`/`MIN_GROUP_DRAWS` 不可达 |
| **O5** | 是否需要**真实客户端会话**（决定 `start_pose` 是观察值还是声明值） | **不启用**（避开 F1：Rust 登录缺 5 个 ctor 抽） | 若启用：必给 Rust 补登录 5 抽（R 级，影响所有含登录流），设计件 §2.6-F1/§5.5/§6.2 全部改写 |
| **O6** | 窗口批次编排：RNG 场景与 ④ `drop-exp` **同窗串行**（`check_window_batch.py`） | **按既有契约串行** | 两个场景各自起栈，窗口时长翻倍、且跨场景相对次序不可比 |
| **O7** | C# 场景描述件的 producer 归属：由 O1 的 `ScenePin` 直接 dump，还是复用 BotSrv（T-59 real wiring） | **由 ScenePin dump**（不扩 BotSrv） | 须先落 **T-59 real wiring**（W9 独占窗口 + R 级真机判据），未落地前该路径不可判 |

## 3. 缩窗 W0-1..W0-5（若选缩窗，现在即可执行）

| 步 | 内容 | 关键判据 |
| --- | --- | --- |
| W0-1 | C# 侧阳性对照：打出 victim=怪 的 `SM_STRUCK` | `-AttackTurn` 全同单向 |
| W0-2 | Rust 同流程（同 (x,y)、先 `CM_TURN` 后 `CM_HIT`） | 帧集合与 W0-1 一致 |
| W0-3 | ③a 反向点 `(311,666)`（E=0.000，`--max-expected 0`） | **不得用 `(291,666)`**（卫士污染） |
| W0-4 | GuardList 帧一致（`(291,666)` 仅作此点） | 两侧对称出帧 |
| W0-5 | 怪打玩家探针（victim=自己 的 `SM_STRUCK`） | 主动族怪应出帧；空转=缺陷 |

前置三连（只读、零起栈）见 runbook §W0-00：`port-status.ps1` → `client-e2e-window.fixed.sh.txt guard`
→ `check_csharp_capture_preflight.py`，三条全绿才允许 W0-1。

## 4. 开窗 / 收窗（owner 授权后由编排执行）

```powershell
# 开窗前最后一次生产落地之后必须重跑 M1/M2（任何新提交都会让 R6/R7 转红）
# M1: /e/tmp/m2svr-window-origin fetch + checkout --detach <origin/master>
# M2: cargo build --offline 六件 -> E:/tmp/m2svr-window-target + --record-provenance
bash mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/client-e2e-window.fixed.sh.txt guard  # 期望 WINDOW_GUARD_OK

# 起栈（C# 影子 + Rust + C# DBSrv/LoginSrv/MySQL）
powershell -File tools/m2-acceptance/start_rust_stack.ps1 -RustBinDir E:/tmp/m2svr-window-target/debug `
  -Seed 42 -LogDir E:/tmp/window/cliente2e/rust/stack

# 收窗（必须同一 -LogDir）
powershell -File tools/m2-acceptance/start_rust_stack.ps1 -LogDir E:/tmp/window/cliente2e/rust/stack -Stop
E:\MirServer\port-status.ps1   # 期望九端口全 DOWN
```

**中止条件**：guard 非 GREEN；端口被占；证据写入失败；两侧帧/抽数出现未登记分歧。任一条成立即收窗并回卡。

## 5. 决策记录（owner 勾选）

- [ ] O1 批准 T-* 改 `src/GameSrv/**`（编号：______）
- [ ] O2 保持 `dir=0` + 批准生成后覆盖
- [ ] O3 先落 C# 时钟注入 + Rust `RunTime=250ms` 门
- [ ] O4 批准影子 Envir（不改 `E:/MirServer`）
- [ ] O5 不启用真实客户端会话
- [ ] O6 与 ④ drop-exp 同窗串行
- [ ] O7 由 `ScenePin` dump 描述件
- [ ] 本轮只开**缩窗 W0-1..W0-5**（整包 W0 待 O1–O7 落地后再排）

> 未勾选前，编排侧保持零起栈、零端口；P3/B1–B5 的 R 级卡维持 `in-progress`。
