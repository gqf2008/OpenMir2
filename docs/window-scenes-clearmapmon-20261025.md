# 窗口新场景第一批：CLEARMAPMON T118 / Q001 探针场景（tier L，工具面）

- 卡：`openmir2-window-scenes-clearmapmon`（owner `openmir2-dev-2`，branch `card/window-scenes-clearmapmon`，base `02f83a1a2`）。
- 写集：`tools/probe/scenarios/clearmapmon_t118.json`、`tools/probe/scenarios/clearmapmon_q001.json`、
  `tools/window/run3-scenarios.json`（只追加 #18/#19，既有 17 条不动）、本报告。
- **不动**：`card_window.py` / `run_batch.sh` / `start_mir2_server.ps1`（tester 的 P0 卡写集）、生产代码。
- 来源：`docs/window-kit-coverage-20261025.md` §1.4（W-CM-1/W-CM-2 原为「部分」）。

## 1. 场景 → W 项映射

| 场景 | W 项 | 触发链（部署件行号） | 步数 |
|---|---|---|---|
| `clearmapmon_t118`（**blocked**） | W-CM-1 | `Market_Def/任务NPC/武士-0114.txt`：`[@main]`(1) → `@rw01` 标签(24) / 链接(`@main` 页 :16) → `@st` **标签 :36**（:32 是 `@rw01` 页里的链接）→ 赢页 `@rw02` 链接(:61) / 标签(117) → `@rw03` 标签(126) / 链接(:123) → `CLEARMAPMON T118`(:145) + `MONGENEX T118 14 17 命运教主 2 1 1`(:146) | 11 |
| `clearmapmon_q001` | W-CM-2 | `Market_Def/任务NPC/老人-1.txt`：`[@main]`(1；`@rw01` 链接 :20) → `@rw01` 标签(31) → `@rw02` 标签(38) / 链接(:35) → `CLEARMAPMON Q001`(:49) + `MONGENEX Q001 10 10 骷髅精灵5 2 1 1`(:50) | 7 |

**placement**（`Merchant.txt` 实测行）：

| 场景 | 部署行 | 目标格 |
|---|---|---|
| t118 | `:157 任务NPC/武士 0114 13 9 高级武士 …` | `0114` `(14,9)`（东邻格） |
| q001 | `:155 任务NPC/老人 1 235 305 老人 …` | `1` `(236,305)`（东邻格）。注意与 `:132 功能NPC/老人 E603 31 191` 不是同一 NPC |

## 1.5 前置（F2：两条都不在默认夹具里，未声明就会在第一步被 `LableIsCanJmp` 拒）

| 场景 | 需要的前置 | 依据 |
|---|---|---|
| `clearmapmon_t118` | **佛牌×1 + 等级 ≥ 43 + 不在 `命运之刃名单.txt`** | `武士-0114.txt` `[@main]` 三分支：`CHECKNAMELIST 命运之刃名单.txt` ⇒ 只有 MESSAGEBOX(767，无 `@rw01`)；`checkitem 佛牌 1` + `checklevel 43` ⇒ 才有 `@rw01`；否则 `#elsesay` 无链接 |
| `clearmapmon_q001` | **蛇毒×1** | `老人-1.txt` `[@main]`：`checkitem 精灵头盔` ⇒ 只有 `@rw03`；`checkitem 蛇毒` ⇒ 才有 `@rw01`（:20 链接）；否则 `#elsesay` 无链接 |

窗口执行时由 `card_window.py` 的 `assets`（白名单列）或手工 DB 预置提供；两条场景均**不写 `assets` 键**
（原因：本批先于 P0 白名单落地/或所列列不在 `ASSETS_COLUMNS`），前置在 `description` 与 `placement.note` 里写明。

## 1.7 seed 实测（2026-10-25；卡 `openmir2-clearmapmon-t118-unblock`）：**未找到稳定 seed，保持 blocked**

- 方法：私有 C# 栈（`tools/window/start-csharp.ps1 -Seed N -RngLog …`，`MIR2_RNG_SITE=1`，
  账本 `INJECT_OK`/`SEED_PROBE` 自证）；探针走到 `@rw01`（`movr M3 3`）后**立即停止，不选 `@st`**
  （避免输支扣佛牌/写名单）；从 RNG 流水按 site
  `ScriptSystem.Processings.ExecutionProcessingSys.ActionOfMovrData@ExecutionProcessingSys.cs:244`
  读取 `Next(max)=3` 的实抽结果。为在 winplay0（等级 1、无佛牌）上触达 `@rw01`，
  **只在私有副本** `csharp-root/.../武士-0114.txt` 注释了两条前置条件（备份
  `E:/tmp/scriptcov/t118/武士-0114.txt.bak`，演练后按 sha256 还原为 `ead2df9f…`）；
  `E:/MirServer` 一字未动。
- **seed → M3 实测表**（每行 = 一次全新起栈；`auto` = 场景内 `auto:last_turn` 点 NPC，
  无独立 discovery 会话；`disc` = 先跑 `npc_click_auto` 发现会话再跑主场景）：

  | seed | 路径 | 第 1 次 | 第 2 次 | 结论 |
  |---:|---|---:|---:|---|
  | 42 | auto | 1 | 1 | 稳定平局，但非赢 |
  | 42 | disc | 2 | 0 | **同 seed 两次不同** |
  | 43 | auto | 2（扫描）| 2 / 0（两次确认）| **同 seed 三次中两次 2、一次 0** |

- **结论**：**同 seed 下 M3 会随起栈/会话时序漂移**（`movr` 之前的取数次数受 NPC 转身、
  握手等时序影响）⇒ **seed 本身不能保证「首掷即赢」**；`M3=0` 的输支会扣佛牌并写
  `命运之刃名单.txt`，一次失败即破坏夹具。故**不满足解封条件 ②**，`clearmapmon_t118`
  **保持 `blocked`，不进 run3**。
- **额外的内容前置缺口**（本次实测新发现，需 owner/内容侧裁决）：`@main` 要 `checkitem 佛牌 1`
  且 `checklevel 43`，但——
  1. `佛牌` 在 `mir2_data` / `mir2_test_data` / `mir2_test_data_r` 三个 `stditems` 里**都不存在**
     （`SELECT ... WHERE Name LIKE '%佛牌%'` 0 行）⇒ 标准包里没有任何角色能持有该道具，
     `@rw01` 对正常客户端**不可达**；
  2. 窗口角色 `winplay0` 等级 1（`mir2_test_db.characters.Level=1`），`assets` 白名单只有
     `Gold/GamePoint`，**无法声明 Level≥43**。
  ⇒ 即使将来能固定 RNG，场景仍需「内容侧修脚本/补佛牌定义」或「窗口专用角色+道具夹具」，
  后者超出本卡写集。**建议另立卡**：给 t118 加受支持的 `Level`/任务道具夹具，或由 owner
  裁定把 `武士-0114` 的前置改成可声明。
- **F1 处置（reviewer-20）**：演练时只还原了基线副本，**影子副本**
  `csharp-root/M2GameSvr-hook/Envir/Market_Def/任务NPC/武士-0114.txt` 一度残留（sha `e621ba2d…` ≠ 基线
  `ead2df9f…`）；Leader 已用基线覆盖还原。复核：baseline / shadow / rust-root 三方该文件均
  `ead2df9f…`，且三方 `Envir` 目录逐文件比对 **642 文件、0 差异**。本卡不再残留改动；
  该类残留将以 preflight 新门 `envir_shadow_symmetry` 固化（卡 `openmir2-preflight-envir-shadow-symmetry`）。
- **证据文件**：`E:/tmp/scriptcov/t118/`（`seed-scan.tsv`、`seed42-csharp.tsv`、
  `seed42b-csharp.tsv`、`autoA/autoB/conf1/conf2-csharp.tsv` 的 head 与 movr 行；
  大账本已删，只保留扫描/确认日志与 `武士-0114.txt.bak`）。

---

## 1.6 分支随机（F1）：只能靠「两侧同 seed」，当前 `blocked`

`[@rw01]` 的 `#act movr M3 3`（`武士-0114.txt:27`）随机 0..2，只有赢页有 `<打听[命运之刃]的下落/@rw02>`(:61)；
输页只有 `/@exit`(:47)、平局页只有 `<再来/@rw01>`(:54)。`M3` 是**脚本局部量**、动作点覆盖任何预置 ⇒ 不可注入。

**唯一可执行办法 = 两侧同一 RNG seed**，但有一个**工具面前提**必须先解决：

1. **窗口 C# 栈要接 hook/seed**：`card_window.py` 的 `alt.sides.csharp.stack` 现用 `E:/tmp/conformance/start-csharp3.ps1`，
   它**没有 `-Seed` / `MIR2_RNG_SEED` / `DOTNET_STARTUP_HOOKS`** ⇒ oracle 仍按时间种子，M3 不可能与 Rust 对齐。
   可执行改法：改 `start-csharp3.ps1`，或把 profile 的 csharp stack 换成
   `mir2-rs/tests/parity/m3/start-csharp-hook.ps1`（`RngSeedHook.dll`，输出 `INJECT_OK <seed>` / `SEED_PROBE`）。
   **工具面归卡 `openmir2-window-csharp-rng`（tester）**。
   （注：`card_window.py:3305` 是 `env = os.environ.copy()` ⇒ 父进程的 `MIR2_RNG_SEED`/`MIR2_RNG_SITE` **会透传**；
   缺的是 oracle 侧 hook 接线，不是环境变量通道。）
2. 两侧同 `-Seed N` + `MIR2_RNG_SITE=1`，且 `INJECT_OK` 证种子生效；用流水确认该 seed 下 `movr` 取到 **2**。
3. 把 seed 号记进窗口报告后解封。

**破坏性副作用（操作约束）**：`[@st]` 的 M3=0（输）分支在 `#SAY` 之前执行 `#act take 佛牌 1; set [105] 0; addnamelist 命运之刃名单.txt`（`武士-0114.txt:37-43`）⇒ 一次「抽出 0」的尝试会**扣掉佛牌并把角色写进名单**，之后点该 NPC 只命中 `CHECKNAMELIST`（得 767、无 @rw01）⇒ 永久卡住。故必须**先用离线/`MIR2_RNG_SITE` 流水确认 seed 再跑第一次**，且**每次尝试前恢复夹具**（佛牌×1 + 从 `命运之刃名单.txt` 移除角色）。

**当前状态：`blocked`** —— `clearmapmon_t118.json` 带顶层 `blocked{reason, missing_fixture}`，**不进 run3 批次**
（runner 见到即 rc=2）。
**失败形态**：解封后若**第 8 步** `wait_for 643` 超时 ⇒ **该 seed 未命中赢页**（不是场景写错）——`@st` 后三条分支都会回 643，第 6 步不会超时；真正失败点是被 CanJmp 拒的 `dlg_select @rw02`（第 7 步）及随后的第 8 步；
**C# 未接线时任何 seed 都不生效**，故那种情况下失败也不能断定 seed 没选对。

## 2. 期望证据（**不得假设 `cleared` 字段**）

- **清怪**：实体集合/AOI 差集（T118/Q001 内存活怪在 `CLEARMAPMON` 后置 `NoItem`/`Ghost` 并删格），
  外加 `SM_DISAPPEAR(30)` 帧序（oracle `DisappearA` → `SendRefMsg(RM_DISAPPEAR)`，泵 `PlayObject.Message.cs:1651-1654`）。
  **帧数与实体数不写死**：以两侧差集一致为准，帧序用于核对（场景最后一步 `wait_for: 30` 只等首帧）。
- **刷怪**：`MONGENEX` 后的刷怪位置/数量（`命运教主` ×2 于 T118 (14,17) 邻域；`骷髅精灵5` ×2 于 Q001 (10,10) 邻域）。
- **出队序**：动作帧与脚本帧在同 handler 按 `MsgQueue` 优先级（`(byte)wIdent`）出队，口径归
  `openmir2-script-handler-msgqueue`；本卡**不自行发明**编号，比对时以该卡结论为准。

## 3. 离线自证（构造级，不启服务端/不占端口）

```text
$ python -I -B tools/probe/mir2_probe.py --check-scenario \
    --scenario tools/probe/scenarios/clearmapmon_t118.json --var npc_actor=1
note: scenario.placement 声明了 map=0114 x=14 y=9，但本驱动器不预置位（由 card_window.py 在起栈前改测试库完成）
scenario OK: clearmapmon_t118 (11 steps, params=npc_actor=1)      # rc=0

$ python -I -B tools/probe/mir2_probe.py --check-scenario \
    --scenario tools/probe/scenarios/clearmapmon_q001.json --var npc_actor=1
note: scenario.placement 声明了 map=1 x=236 y=305，但本驱动器不预置位（由 card_window.py 在起栈前改测试库完成）
scenario OK: clearmapmon_q001 (7 steps, params=npc_actor=1)       # rc=0
```

`run3-scenarios.json`：**只追加 `{"n":19,"name":"clearmapmon_q001","profile":"alt"}` 一行**（#18 已由先合并的 `gamepoint_exchange` 占用），既有 17 条逐字未动；
`clearmapmon_t118` 因 `blocked` **不入批**（解封条件见 §1.6）。

## 4. 真栈执行（窗口期才可执行）

- 需要真栈 + 双栈对拍：`card_window.py` 按 `placement` 改**测试库**角色坐标后起栈驱动。
- **前置缺口（见 kit §2 P0）**：RNG 流水账本依赖私栈接 `-RngLog`；若窗口只看「实体差集 + 帧序」，
  W-CM-3（RNG 流水位置）仍不可判 —— 本场景把它标为**观察项**，不作关卡硬判据。

## 5. 门禁

- 场景 JSON：`--check-scenario` 两条 rc=0；`run3-scenarios.json` 仍为合法 JSON 且只增两行。
- 卫生门禁 + `git diff --check <base> <head>` + `merge_tree_helper` 见 patch body / status。
