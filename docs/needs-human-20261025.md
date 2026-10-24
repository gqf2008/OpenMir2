# 需要 owner 决策 / 授权（2026-10-25）

> **快照戳**：`master = bff927e11` @ 2026-10-25 01:18 +0800（本地；walgit 主源故障中）。
> `goal_gate` 取数：`python -I tools/goal/goal_gate.py --full --json` @ 2026-10-25 01:26 +0800（generated_ts 1792862761）。
> 本页取代 `docs/needs-human-20261024.md` 作为**当前**决策页（旧页保留只读，不覆盖）。
> 只列循环内无权自决的事；每项 ≤3 行，可按 §0 模板一行回复。

## 0. 一行回复模板

```text
①G11=A  ②Q1=A  ③Q2=D  ④Q3=A  ⑤Q4=丙  ⑥Q5=A  ⑦walgit=等恢复
```

（任一项不填 = 保持现状；`A` 均为本页推荐。）

---

## 1. ① G11 依赖裁决（条件② 最后 1 张 `openmir2-gm-batch-5b-cross-server`）

**现状**：B 片 6 条 GM 命令的 oracle 依赖清单已合入（`99a4d30c4`），实现 **blocked-by G11**
（跨服 `PlanesServer`/`PlanesClient` 传输 + 主/分服拓扑 + 跨服状态同步，仓内无实现）；
卡面 `blocked`，`goal_gate` 条件② 因此剩 **1 张需人工**（`merged_not_closed`）。

| 选项 | 含义 | 代价 |
|---|---|---|
| **A（推荐）** | 把该卡列入「基础设施阻塞排除项」，条件② 不再把它计入分母；G11 作为**独立基础设施卡**排期 | 需 owner 一句话 + 一处 goal_gate 脚注 |
| B | 保持 `need_human`，条件② 长期挂 1 张 | 条件② 永不 PASS（除非 G11 实施） |
| C | 实施 G11（传输 + 拓扑 + 6 条命令移植 + 怪癖牙） | 大工程；当前三日计划未排 |

**推荐 A 的脚注落点（实现时改这里，本卡不改代码）**：
- `tools/goal/goal_gate.py:767-780` 条件② `detail` 的 `merged 未 closed 且无登记(需人工) %d`
  计数（`counts.get("merged_not_closed")`）旁加 `（另排除 N 张基础设施阻塞：<卡 id>）`；
- 同处 `evidence` 增 `merged_not_closed_exempt: [{card, blocked_by, adjudication_oid}]`，
  只在**有签名 owner 裁定**时填充（不静默豁免）；
- 判据正文同步到 `docs/leader-goal-20261023.md` §1 条件②（一行脚注 + 排除项定义）。

> **回复**：`①G11=A`（推荐）／`①G11=B`／`①G11=C`

---

## 2. ② Q1 窗口授权（17 场景，`docs/rcard-evidence-plan-20261025.md` §6 Q1）

**现状**：`window_required` 141 张（已合并）+ 2 张未合并，`goal_gate` 按登记记 PASS ⇒
**条件② 的 gate 口径不要求真机窗口**（当前唯一需人工项是 §1 的 G11）。但 190 张
「登记型 PASS ≠ 真机证据」，窗口是唯一能把它们转成真证据的手段。

| 选项 | 含义 | 代价 |
|---|---|---|
| **A（推荐）** | 按 §5 W1 全量开一轮：**17 场景 + 6 夹具**（run-2 续跑，去掉已封存 `pickup_item`），40–90 分钟 | 240 GB 磁盘 + 独占机器；直接覆盖 49/137 张 |
| B | 最小集：`login_only + upgrade_retrieve + equip_struck_save + spell_cast`（≈31 张骨架） | 只需 1 小时窗口 |
| C | 暂不开窗，先做 Q2–Q5 / 规则表复核 | 条件② 仍可 PASS（gate 口径），但真证据继续为零 |

**推荐 A**：gate 不强制，但这是「登记型 PASS → 真机证据」的唯一路径；若只有 1 小时退 B。
前置包已备好：`docs/window-run2-prep-20261025.md`（`run_batch.sh --dry-run` 全绿）。

> **回复**：`②Q1=A`（推荐）／`②Q1=B`／`②Q1=C`

---

## 3. ③ Q2–Q5（摘自 `docs/rcard-evidence-plan-20261025.md` §6，保留推荐）

**Q2｜无场景 62 张的处置口径**
- A 全部补场景（≈4 轮开发）／B 全部登记不可对拍／C 从条件② 分母剔除／
  **D（推荐）混合分档**：46 张补场景（W2）、15 张视 Q3 解封（W3）、1 张 E2E 登记。
- 推荐 D：保留「可判的先判」，避免 B 的一刀切与 A 对 15 张不可达卡的无限投入。
- 回复：`③Q2=D`

**Q3｜combat 夹具投入（10 张直接 + 20+ 间接）**
- **A（推荐）**：开卡补「同屏怪物 + 伤害/配装夹具」，解封 `combat_hit`/`combat_strike`；
  B 登记不可对拍；C 只补同屏怪物一半（`combat_strike` 继续 blocked）。
- 推荐 A：62 张 B 类里唯一「一个夹具换 10 张卡」的杠杆。
- 回复：`④Q3=A`

**Q4｜跨栈 RNG 流位 flaky 类口径（影响 `npc_click` 与 `login_only` Weight）**
- 甲 改「结构等价 + N 跑方差」／乙 双栈 RNG 流位锚点同步（另批实现）／
  **丙（推荐）本轮按已知 flaky 登记**（D-13 口径），乙另开卡。
- 注意：`login_only` 的 `Weight 14vs0` **不是 flaky**，是真实数值缺口（`login-numeric-gaps`），W1 仍要跑。
- 回复：`⑤Q4=丙`

**Q5｜「登记型豁免」口径（C 类 26 张与执行/工具卡）**
- **A（推荐）**：认可 §3 的 25 张「不可协议对拍」替代判据 + `window-run-1` 执行记录 +
  `verify-tick-base` 登记/关闭；B 保留在窗口欠账；C 逐卡裁定。
- **必须保留** `goal_gate` 卫生清单口径：「登记型 PASS ≠ 真机证据」这句话不能删。
- 回复：`⑥Q5=A`

---

## 4. ④ walgit 主源故障与恢复途径

- **故障**：walgit 服务端 R2 存储缺 WAL 段 `log/0000000000003000.pb not found`；
  `fetch`/`push` 全 HTTP 500，重启服务无效。master 已备份到 GitHub `origin`（`43472c8a4`）。
  本机实测（2026-10-25 01:2x）：`git ls-remote walgit refs/heads/master` ⇒
  `remote: internal error: wal: object not found: repos/gqf2008/OpenMir2/log/0000000000003000.pb`，
  HTTP 500，rc=128。
- **恢复途径**：R2 控制台版本恢复 / 联系 walgit 维护者；无需 owner 改代码或配置。
- **恢复后 Leader 统一补推**（本地相对最后已知 walgit master `4ae3dc810` 领先 **22** 个提交、含今日 10 交付）：
  ```sh
  git push walgit master                                      # bff927e11
  git push walgit 'refs/collab/inbox/*:refs/collab/inbox/*'   # 本地 9042 条（未折叠）
  git push origin master                                      # GitHub 镜像（或恢复后人工同步）
  ```
  补推清单 = 今日 9 个 merge + 1 个 docs 提交（§5 表）+ 各 worktree 的本地 collab 条目
  （dev-1/dev-2/reviewer-*/tester-1/orchestrator）+ 仍在本地未推的 `card/*` 分支。
- **当前本地模式**（所有 agent 遵守）：跳过 `git fetch walgit`；`walgit collab entry` 只写本地
  ref、不加 `--push`；本地 commit/分支照常；push 失败忽略不重试。
- **owner 只需**：若故障超过今天，决定「等 R2 恢复」还是「先以 GitHub 为临时主源」。
- 回复：`⑦walgit=等恢复`（推荐）／`⑦walgit=GitHub 临时主源`

---

## 5. ⑤ 状态快照（goal_gate 实测）

| 条件 | 状态 | 实测（复算口径） |
|---|---|---|
| ① 脚本派发 100% | **pass** | `parse-stats`：语料派发目标覆盖 **已实现 106 个（11829 次调用）/ 未实现 0**（`E:/tmp/scriptcov/parse-stats-20261025.stdout.txt`；full 模式实测 pass） |
| ② R 卡证据 | **need_human** | R 级 **207** 张：closed 12 / `window_required` **141**（另未合并 **2**，单列）/ whitelist 48 / 已裁定 3 / 三者皆无 0 / **merged 未 closed 需人工 1（仅 `openmir2-gm-batch-5b-cross-server`，即 §1 的 G11）** / 其它 0；卫生清单「登记型 PASS ≠ 真机证据」**190** 张 |
| ③ B 类清零 | **pass** | recon **tracked=0**（@ bff927e11；rows 191 条非空、counts 一致） |
| ④ 门禁恒绿 | **need_human** | 7 项：绿 **6** / 红 0 / 需人工 **1**（仅 `master_sync`：walgit 500，`git ls-remote` rc=128）；`cargo test --workspace` **rc=0（3190 passed / 0 failed）** @ `bff927e11`（full 模式 165.2s；日志 `E:/tmp/scriptcov/test-bff927e11.txt`） |

> 口径说明：walgit 故障期间 `goal_gate --full` 的 `master_sync` 必然 `need_human`（无法比对面）；
> 条件① 由 `--full` 的 `parse-stats` 复算、条件④ 的 `cargo test` 同一提交实测通过，二者均可复跑。
> 四条件的**唯一真缺口**是 ② 的 G11 1 张（§1）；`window_required` 未合并 2 张按规则单列，不计 FAIL。


### 今日 10 交付（本地 `master`，2026-10-25 00:02–01:18）

| # | 提交 | 卡/内容 | tier |
|---:|---|---|---|
| 1 | `0183dca15` | `window-run2-prep`：window-run-2 续跑准备包 | L |
| 2 | `43472c8a4` | window-prep 报告两处 Minor（直接 docs 提交） | L |
| 3 | `87dc9847b` | `rcard-evidence-classification`：R 卡三分类 + 裁定包（A49/B62/C26） | L |
| 4 | `0be3407a2` | `check-pickup-unreachable-type-aware`：检查器 P-1..P-6 | L |
| 5 | `cb646b65a` | `window-run3-scenarios-blocked`：run3 清单 18→17 | L |
| 6 | `a6fc02d0a` | `check-pickup-alias-p7`：检查器 P-7 | L |
| 7 | `0bac2ed2d` | `envir-redlist-teeth`：redlist T3 四颗常驻牙 | L |
| 8 | `99156c2c6` | `castle-merchant-tax-wiring`：商税四调用点接线 | **R** |
| 9 | `9a7b8c798` | `loader-fields`：game-svr 消费 envir startpoint 加载器 | **R** |
| 10 | `bff927e11` | `playdice-gamapoint-residual`：T-142 ⑨⑩(子集)⑪ | **R** |

> 今日 9 个 merge + 1 个直接 docs 提交 = 10；其中 R 级 3 张（商税/loader/T-142），合并后保持
> `in-progress`，窗口复拍后 closed。

---

## 6. 本页不包含（避免重复消耗 owner 注意力）

- `rules.json` PENDING 复核、`T-161..T-222` 复核口径、三张替代判据卡、私有 C# 栈去留：
  已由 `docs/needs-human-20261024.md` §0 覆盖且有现行裁定入口，本页不重复（如仍未决，沿旧页走）。
- 本页只新增 G11、窗口、Q2–Q5、walgit 四类**今天发生变化**的决策点。
