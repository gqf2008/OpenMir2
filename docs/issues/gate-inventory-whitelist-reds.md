# gate-inventory-whitelist-reds — 两道白名单门长期红（只读调查，2026-11-21）

> 状态：**调查卡**（未改 `whitelist.md` / 预约表 / 任何锚；未起构建）。查询 head：master `a209f36fd`。
> 触发：opc-19 在 MG-47 收口时发现 `t_reservation_gate` / `whitelist_anchor_gate` 为红，而
> `whitelist.md` 就在 magic 批写集内 ⇒ 这两道门与我们的改动直接相关，却不在我们的五门合并门里。

## 1. 现状（直接运行，rc=1）

- `tools/whitelist/t_reservation_gate.py check` → **RED**：10 条 ① 违规——`T-287`..`T-296`
  在 `whitelist.md` 有登记行，但预约表（`T-RESERVATIONS.md`）没有对应行。表统计：rows=271 /
  whitelist=281 / max=T-286。
- `mir2-rs/tests/parity/m3/whitelist_anchor_gate.py check` → **FAILED**：4 条 red（3 条
  `ANCHOR_DRIFT` + 1 条 `ANCHOR_FILE_MISSING`），其余为 info（EXTERNAL / FROZEN-ORACLE / AMBIGUOUS）。

## 2. 什么时候开始红（时间线）

- **最后一次已知绿**：2026-10-24 CI 首跑（`docs/ci-first-master-run-20261024.md`：anchor
  ALL PASS 590、reservation GREEN 255/255）。
- **红起全部在 2026-11-20**：

| 门 | 首条变红来源 | 提交（日期） | 事实 |
| --- | --- | --- | --- |
| reservation | `T-287` 登记 | `747d542eb`（11-20） | 首条"有 whitelist 行、无预约行"（NP-11 AOI 批） |
| anchor `A-14` | `merchant_say_gbk` 移位 | `a7255cba9`（11-20，NP-10 隐藏 NPC） | 符号从 10-24 时的第 66 行移到第 71 行（锚第 65 行，±3 失配） |
| anchor `T-166` | `magic::front_position` 移位 | `7f96eedff`（11-20，b-attack 段 1） | 符号移到第 802 行（锚第 702 行） |
| anchor `T-207` | `set_abil_exp` 移位 | `a7255cba9`（11-20） | 符号移到第 1791 行（锚 1677–1680 区间） |
| anchor `T-292` | 裸 `Operate.cs` 引用 | `05ef84ecc`（11-20，657 批 A 收尾） | 仓内无名为 `Operate.cs` 的文件（实为 `*.Operate.cs`）⇒ 自登记起即红 |

> `A-14` 的时间线细节：锚第 65 行写入于 2026-10-20（当时符号第 63 行，±3 内）；10-23 曾移到
> 第 70 行，10-24 CI 时又回到第 66 行（仍绿）；11-20 起到第 71 行（红）。⇒ "红起 = 11-20"与
> 10-24 的绿不矛盾。

## 3. 为什么一直没人发现（门覆盖缺口）

1. **我们的合并门不含这两道门**：五门（fmt / clippy / --lib / msgcodegen / docs-anchor）与
   `parity-gate` 都不调用它们。
2. **它们已声明在 CI 里但没在跑**：`.walgit/ci.toml` 有 `fast-t-reservation` 与
   `fast-whitelist-anchor`（@hourly，refs 含 master），但 `wg ci_status` 读到 **runs=[]**
   （CI ledger 无运行记录）⇒ 自 10-24 之后没有 runner 真正执行；ci.toml 目前等于"已声明"。
3. **`t_reservation_gate` 的运行位置有硬约束**：其头注写明"在**自己分支的 worktree** 上跑永远是绿的，
   必须在**合并结果树**上跑" ⇒ 只有合并者/runner 能看见红；各作者自跑看不到。
4. **`whitelist.md` 在 magic 批写集内**（MG-47 等），但写集清单不触发这两道门 ⇒ "改了一个受门保护的文档，
   却没人跑那道门"。

## 4. `T-287`..`T-296` 是什么（10 条）

| 号 | 作者 / 卡 | 引入提交（日期） | 摘要 |
| --- | --- | --- | --- |
| T-287 | `openmir2-bd` / `openmir2-visiblehumanlist-cache`（部分消解） | `747d542eb`（11-20） | `SendRefMsg` 500ms `VisibleHumanList` 缓存 + `<11` 二次过滤未建模（边界观察者多收帧） |
| T-288 | `openmir2-2f` / `openmir2-charstatuschanged-wiring` 批 A #2 | `8ae250588`（11-20） | GM 改属性（`SmakeItemCommand`）帧内序偏差 |
| T-289 | 同上 | `05ef84ecc`（11-20） | 商店升级武器（`UpgradeWapon`）缺 657 |
| T-290 | 同上 | `05ef84ecc` | 战斗升级 657（已接，链末近似） |
| T-291 | 同上 | `05ef84ecc` | GM `@删除技能` 缺 657 |
| T-292 | 同上 | `05ef84ecc` | 装备复检（`drain_player_need_recheck_with_magic`）缺 657 |
| T-293 | 同上 | `05ef84ecc` | 饥饿跨档缺 657 |
| T-294 | 同上 | `05ef84ecc` | 复活戒指打碎缺 657 |
| T-295 | `openmir2-2f` / `openmir2-tick-frame-priority-queue` | `7ab5651aa`（11-20） | T-290 近似的**链前**残差（方向 A，9 处） |
| T-296 | 同上 | `7ab5651aa` | T-290 近似的**链后**残差（方向 B，9 处 + 待核） |

- **预约表里从未出现过这 10 个号**：`git log -S "T-287" -- T-RESERVATIONS.md` 与 `T-296` 同为空
  ⇒ **不是被清理**（历史无增删痕迹），是登记时没有记账。
- **预约机制 2026-10-23 已存在**（`f06984267`：预约表 + 取号器 `next_t.py` + 三向门禁）；
  **先例** `bbe1aebdd` 曾补 `T-194` 预约行（"预约机制落地前取号，合并时由 merger 记账"）
  ⇒ 本次更像"取号后（或未取号）没有在合并时补行"，而不是"机制不存在"。
- **待对账**（本卡不动数据）：号码是否经 `next_t.py` 取过——journal 在 `.git` common dir、不进仓，
  需取号者或 merge runner 提供；若从未取号，则属绕过唯一入口，需与 `openmir2-2f`/`openmir2-bd` 对账后
  再补行（否则可能撞 ②同号多认领）。

## 5. 四条锚：现在应该是什么

| 条目 | 现状（门点名的锚） | 建议形态 | 依据 |
| --- | --- | --- | --- |
| `A-14` | `bins/game-svr/src/npc.rs` 第 65 行；`merchant_say_gbk` 现第 71 行 | **符号锚**（只写文件 + `merchant_say_gbk`） | 高 churn 文件禁行号锚（门自带规则 §14） |
| `T-166` | `crates/world-bevy/src/magic.rs` 第 702 行；`front_position` 现第 802 行 | 符号锚 `front_position` | 同上 |
| `T-207` | `npc.rs` 第 1677–1680 行；`set_abil_exp` 现第 1791 行 | 符号锚 `set_abil_exp` | 同上 |
| `T-292` | 裸文件名 `Operate.cs` 的 455/544 | **`src/M2Server/Player/PlayObject.Operate.cs` 的 455/544**（两处均为 `RecalcAbilitys()`，与条目"穿脱后复检发 657"一致；BotSrv 的 `RobotPlayer.Operate.cs` 第 455 行是 EDCode 解析，与本题无关） | 补全路径 |

## 6. 修法与建议（本卡不改，先定"它们是什么"）

1. **锚**：三处行号锚改**符号锚**（门规则要求；改前逐条确认条目仍有效——`T-166` 所属 S5 已退役？
   `A-14` 是"编码分项台账"）。`T-292` 补全路径。
2. **预约表**：`T-287`..`T-296` 补 10 行（"已落地"或"在用"，日期取引入提交日），或走取号器的补记流程；
   **先与两位作者对账 journal**，避免补行后出现 ②同号多认领。
3. **门覆盖**：把 `fast-t-reservation` 与 `fast-whitelist-anchor` 纳入**合并门**（本地、秒级、无网络），
   并写明"改了 `whitelist.md` 的批次必须跑"；CI 侧需要 runner 实际执行（当前 `runs=[]`），否则
   `.walgit/ci.toml` 继续是"声明即遗忘"。
4. **立场**：这两道门红与 magic 批的写集直接相关（`whitelist.md` 在写集内）⇒ 属
   "改了一个受门保护的文档，却没人跑那道门"的**系统性缺口**，不只是别人的历史债。

## 7. 证据（命令）

- 直接跑（rc=1）：`python -I tools/whitelist/t_reservation_gate.py check`；
  `python -I mir2-rs/tests/parity/m3/whitelist_anchor_gate.py check`。
- 预约史：`git log -S "T-287" -- mir2-rs/tests/parity/whitelist.md`（`747d542eb`）；
  `git log -S "T-287" -- mir2-rs/tests/parity/T-RESERVATIONS.md`（**空**）；
  先例 `git log -S "T-194" -- mir2-rs/tests/parity/T-RESERVATIONS.md`（`bbe1aebdd`）。
- 锚起红：对每个锚，逐提交 `git show <rev>:<file>` 取声明符号的行号（结果见 §2 表）；
  例：`merchant_say_gbk` 在 `a7255cba9` 起为第 71 行。
- CI：`wg ci_status` → `runs: []`（无运行记录）。
