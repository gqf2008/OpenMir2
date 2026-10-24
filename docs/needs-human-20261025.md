# 需要 owner 决策 / 授权（2026-10-25，第 2 次刷新）

> **快照戳**：`master = 27a6f27e0` @ 2026-10-25 06:40 +0800（本地；walgit 主源仍故障）。
> `goal_gate` 取数：`python -I tools/goal/goal_gate.py --skip-tests --script-bin <release mir2-script-tool> --json`
> （generated_ts `1792881659`，2026-10-25 06:40 +0800；**全量 `cargo test` 本轮未跑**，见 §5 口径）。
> 本页**就地取代**同路径的上一版（`bff927e11 @ 01:18`，其中「开窗前置缺口」今天已全部解决，照旧读会误导）。
> 只列循环内无权自决的事；每项 ≤3 行，可按 §0 模板一行回复。

## 0. 一行回复模板

```text
①G11=A  ②Q1=A  ③Q2=D  ④Q3=A  ⑤Q4=丙  ⑥Q5=A  ⑦walgit=等恢复
```

（任一项不填 = 保持现状；`A` 均为本页推荐。**②是今天的唯一新阻塞**：其余各项与上一版同口径。）

---

## 1. ① G11 依赖裁决（条件② 唯一一张 `merged_not_closed`）

**现状**：`goal_gate` 条件② 的 `merged 未 closed 且无登记(需人工)` 仍**只有 1 张**——
`openmir2-gm-batch-5b-cross-server`（B 片 6 条 GM 命令 oracle 清单已合 `99a4d30c4`，实现 blocked-by G11：
跨服 `PlanesServer`/`PlanesClient` + 主/分服拓扑 + 跨服状态同步，仓内无实现；看板 `受阻`）。

| 选项 | 含义 | 代价 |
|---|---|---|
| **A（推荐）** | 把该卡列入「基础设施阻塞排除项」，条件② 不再把它计入分母；G11 作为**独立基础设施卡**排期 | 需 owner 一句话 + 一处 goal_gate 脚注 |
| B | 保持 `need_human`，条件② 长期挂 1 张 | 条件② 永不 PASS（除非 G11 实施） |
| C | 实施 G11（传输 + 拓扑 + 6 条命令移植 + 怪癖牙） | 大工程；当前三日计划未排 |

**推荐 A 的脚注落点（实现时改这里，本页不改代码）**：`tools/goal/goal_gate.py` 条件② `detail` 的
`merged 未 closed 且无登记(需人工) %d` 旁加 `（另排除 N 张基础设施阻塞：<卡 id>）`；同处 `evidence` 增
`merged_not_closed_exempt: [{card, blocked_by, adjudication_oid}]`，**只在有签名 owner 裁定时填充**（不静默豁免）；
判据正文同步到 `docs/leader-goal-20261023.md` §1。

> **回复**：`①G11=A`（推荐）／`①G11=B`／`①G11=C`

---

## 2. ② Q1 窗口授权 —— **前置已全解，只差你给时段**

**上一版列的「开窗前置缺口」今天已全部关闭**（每项都有合并 oid）：

| # | 前置项 | 状态 | 依据 |
|---:|---|---|---|
| 1 | `assets` 白名单只有 `Gold` | **已解** | `aa94a89db`：`ASSETS_COLUMNS = ("Gold","GamePoint")` |
| 2 | 私栈未接 `-RngLog` | **已解（工具面）** | `aa94a89db`：`card_window.py --rng-log` → `run_batch.sh` → `start_mir2_server.ps1`；C# 侧 hook 由 `15dbbe843` 落地 |
| 3 | 批次场景数硬编码（17/18） | **已解** | `2135f6154`：`run_batch` + `preflight` 双处按清单实况校验、`n` 连续 |
| 4 | C# 栈无 seed/hook（RNG 类 R 卡不可验） | **已解** | `15dbbe843`：仓库版 `start-csharp.ps1` + `RngSeedHook.dll` + `MIR2_RNG_SITE` |
| 5 | 两侧 `Castle/List.txt` 可能不对称 | **已解** | `9c17e8eab`：`preflight` 新增 `castle_list_symmetry`（含 `M2GameSvr-hook` 影子）fail-closed |
| 6 | 动物/尸体夹具 | **已解（活体）** | `13013389e`：`profile fixtures.monsters` 单点刷怪（MonGen 第 7 列 `ZenTime=360>0`） |

**当前实况**：run3 = **23 条（blocked 2 ⇒ 入批 21）**（`run_batch --list` 实测）；操作单
`docs/window-exec-sheet-20261025.md` 已按 23 条刷新（`1af9d6647`）；台账
`docs/window-closure-ledger-20261025.md` 已二次刷新（`e3ac7c121`）。

**开窗当天只需勾这 5 项**（每条都有现成命令，全在操作单 §1）：

- [ ] `powershell -File E:/MirServer/port-status.ps1` ⇒ 5000–7200 全 DOWN、mysqld 3306 UP
- [ ] `python -I -B tools/window/preflight_private_stack.py` ⇒ 全绿（含新的 `castle_list_symmetry`）
- [ ] `bash tools/window/run_batch.sh --list` ⇒ `场景数 23（blocked 2 ⇒ 入批 21）`
- [ ] `bash tools/window/run_batch.sh --dry-run` ⇒ 自检全绿、不起栈
- [ ] 专用测试账号 + `assets`/`inventory`/`fixtures` 声明（`card_window` 在 restore 后、起栈前应用）

| 选项 | 含义 | 代价 |
|---|---|---|
| **A（推荐）** | 开一轮**全量**：23 条入批 21 条（含 `castle_merchant_tax_potion` 的 620s 落盘等待），独占机器 1.5–3 小时 | 需 disk + 独占；直接覆盖台账 #1/#3/#4/#6 的窗口行 |
| B | 最小集：`login_only + upgrade_retrieve + equip_struck_save + spell_cast + gamepoint_exchange` | 1 小时内可跑完，覆盖骨架 |
| C | 暂不开窗 | 条件② 的 gate 口径仍 PASS，但 196 张「登记型 PASS」继续没有真机证据 |

> **回复**：`②Q1=A`（推荐，请给时段）／`②Q1=B`／`②Q1=C`

---

## 3. ③ Q2–Q5（口径未变，保留推荐）

**Q2｜无场景卡的处置**：**D（推荐）混合分档**（46 张补场景 W2 / 15 张视 Q3 / 1 张 E2E 登记）。`③Q2=D`
**Q3｜combat 夹具投入**：**A（推荐）**补「同屏怪物 + 伤害/配装夹具」，解封 `combat_hit`/`combat_strike`。`④Q3=A`
**Q4｜跨栈 RNG flaky 类**：**丙（推荐）**本轮按已知 flaky 登记；`login_only` 的 `Weight 14vs0` **不是 flaky**（真实数值缺口）。`⑤Q4=丙`
**Q5｜登记型豁免口径**：**A（推荐）**；**「登记型 PASS ≠ 真机证据」这句必须留在 `goal_gate` 卫生清单里**。`⑥Q5=A`

---

## 4. ④ walgit 主源故障与恢复途径（今天仍 500）

- **现状**：`git fetch walgit` 仍 `HTTP 500`（`repos/gqf2008/OpenMir2/log/0000000000003000.pb not found`），
  今天每轮 cron 都复现；全员本地模式（`entry` 只写本地 ref、不 `--push`、不重试）。
- **两份兜底 bundle 都在**（`E:/tmp/openmir2-fallback/`，**恢复请用最新一份**）：

| 份 | 路径 | 大小 | sha256 | 包内 master |
|---|---|---:|---|---|
| 1 | `openmir2-20261025.bundle` | 166132533 | `24d39dac…306e927` | `dc0430eda` |
| **2（最新）** | `openmir2-2135f6154.bundle` | 167098305 | `34f65797…a5a857` | `2135f6154` |

> bundle 是**时点快照**：`2135f6154` 之后的新提交/协作条目不在包内，恢复后从本地 `.git` 补推即可。

- **恢复后 Leader 统一补推**：`git push walgit master` + `git push walgit 'refs/collab/inbox/*:refs/collab/inbox/*'`
  （本地条目已 9k+）+ 各 worktree 的 `card/*` 分支；GitHub `origin` 只是镜像。
- **owner 只需决定**：继续等 R2 恢复（推荐），还是**以 GitHub `origin` 为临时主源**（需 owner 明确批准，本仓默认不推 origin）。

> **回复**：`⑦walgit=等恢复`（推荐）／`⑦walgit=GitHub 临时主源`

---

## 5. ⑤ 状态快照（`goal_gate` 实测 @ `27a6f27e0`）

| 条件 | 状态 | 实测（复算口径） |
|---|---|---|
| ① 脚本派发 100% | **pass** | `parse-stats`：目标 **106/106**、调用 **11829**、未实现 **0**（语料 639/639/0/0） |
| ② R 卡证据 | **need_human** | R 卡 **213 张**（快照 @ `27a6f27e0`）：closed **12** / `window_required` **147**（另**未合并 2**）/ whitelist **48** / 已裁定登记 **3** / `merged_not_closed` **1**（仅 G11）/ `not_merged` **0**；卫生清单「登记型 PASS ≠ 真机证据」**196** 张 |
| ③ B 类清零 | **pass** | recon `tracked=0`（快照 @ 27a6f27e0；rows 191 条非空、counts 一致） |
| ④ 门禁恒绿 | **need_human** | 7 项：绿 **5** / 红 **0** / 需人工 **2** —— `master_sync`（walgit 500）+ `cargo_test_workspace`（**本轮未跑**；最近一次全量证据为 tester 在 `2eb147e6e` 的 **3237 passed / 0 failed**，其后 7 笔合并我逐笔核过，仅 `b42a8701d` 触及生产文件且只加两条 `#[cfg(test)]`） |

> 口径：walgit 故障期 `master_sync` 必然 `need_human`；②的「未合并 2」按规则单列、不计 FAIL。
> **四条件的真缺口只有 ② 的 G11 1 张（§1）**；另有一条仓内不一致已开卡处理（`script_host_noop_audit --strict-lines`
> 行号基线，卡 `openmir2-ledger-line-baseline-resync`，已合 `b06040264`）。

---

## 6. 本轮 `blocked` 项与解封条件（新表；每项写谁在做）

| 场景（run3） | 为什么还 blocked | 缺什么 | 谁在做 / 预计产出 |
|---|---|---|---|
| `butch_two_draws`（**#21**） | 需要先有**尸体**，而 oracle 不持久化尸体 | **场景内击杀步**：`CM_HIT(1004)` 打到死 + `wait_for` 死亡帧，再发 `CM_BUTCH(1007, recog=MakeLong(X,Y))`；profile 同格声明 1 只活体 | 卡 `openmir2-probe-kill-step`（`openmir2-dev-2` 在办）；活体夹具已合 `13013389e`，C# RNG 已合 `15dbbe843` |
| `clearmapmon_t118`（**不在 run3**） | `movr M3 3` 分支随机；输支会扣佛牌 + 写名单（终局） | 两侧同 `-Seed N` 且确认 `movr=2` → 记 seed → **owner/Leader 裁决入批** | 夹具与 hook 已合 `15dbbe843`；判 seed 的生命周期归窗口执行（`docs/window-scenes-clearmapmon-20261025.md` §1.6） |
| `castle_merchant_tax`（**#22**） | 升级腿要 `CastleGetAllNpcTax=1` 的**配置覆盖通道** + 与武器铺不同图的**第二个 placement**；且税只经 600s 存档落盘 | 配置覆盖通道（`card_window` 现无）+ `castle_merchant_tax` 的第二腿场景 | 卡 `openmir2-window-fixture-castle`（`openmir2-dev-1` 在办）；买腿已可跑（**#23** `castle_merchant_tax_potion`，税 **4** / 判据 `650.recog=9912`） |

> **红线（本表遵守）**：不把「等 owner / 等卡」写成已解决；不把**登记型 PASS** 写成真机证据；
> 上表三行都是**当前真跑不了**的场景，解封后须按 `openmir2-blocked-two-sources-doc` 同时改**场景 JSON 与 run3 清单行**两处。

---

## 7. 本页不包含（避免重复消耗 owner 注意力）

- `rules.json` PENDING 复核、`T-161..T-222` 复核口径、三张替代判据卡、私有 C# 栈去留：
  见 `docs/needs-human-20261024.md` §0（旧页只读保留）。
- 本页只列**今天发生变化**的决策点：G11、窗口（Q1）、Q2–Q5 口径、walgit、blocked 三项。
