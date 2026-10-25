# 需要 owner 决策 / 授权（2026-10-25，第 2 次刷新）

> **快照戳**：`master = f72dbce7` @ 2026-10-25 14:57 +0800（walgit 主源 = 原仓库，已恢复；本页 §5 已按此 tip 重数）。
> `goal_gate` 取数：`python -I tools/goal/goal_gate.py --full --script-bin <release mir2-script-tool> --json`，
> 快照 `2cb84220f` @ 13:47（**含全量 `cargo test`**：3252 passed / 3315 running / 63 ignored、rc=0；①③④ 均在该 tip 实跑）。
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

## 1b. ①b `rules.json` 判据裁定的报告（**Leader 已裁定，owner 可推翻**）

**为什么它属于「条件②」**：`tools/conformance/rules.json` 是双栈对拍的判据表。08:25 实测：**53 行全部 `approved_by=PENDING`**
（global 4 + rules 48 + db 1）⇒ 每份窗口报告恒带 `rules_pending_review=true`，**窗口跑出来的 PASS/FAIL 不能当 R 级已复核证据**。
这是条件② 一直存在但未计入的一层折扣。

**Leader 裁定（2026-10-25，卡 `openmir2-rules-pending-adjudication`，issue `062972b6`）**：
- **转正 50 条**：global 4 + rules 45 + db 1（逐条校验 `tier`/`reason`/草稿人格式后才翻，任一不过不写）。
- **保留 PENDING 3 条**：`14`（reviewer-2 明文要求等 R 级窗口⑧，不能凭合并算落地）、`10`/`50`（`head_mask:["series"]` 把 `MakeWord(Dir,Light)` 整字掩掉、连 Dir 一起丢 ⇒ 证据等级不足；解法是工具支持只掩 Light，另卡）。
- 主标志 `rules_pending_review` **保持保守口径**（文件里还有任何 PENDING 就 true），另加附属证据 `pending_review_used`（本次实际涉及面）。

**落实情况（2026-10-25 09:00）**：已落地并合入 master `721ea45b8`。机判复核：`rules.json` 走行级最小 diff（50 insertions / 50 deletions，只动 `approved_by`）；剥掉 `approved_by` 后递归比对 `identical=True`；PENDING **53 → 恰 3**（10/14/50）；22 条「必须 PENDING」的自测牙改瞄未删；`compare_runs selftest` 316/316。

> **你要做的**：**不需要回复**——除非你不同意。不同意请一句话「rules=不同意（理由）」，我按 `docs/rules-pending-adjudication-20261025.md` 的回滚方式整体回退。

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
| 7 | 怪物夹具**写盘位置错**（追加到 MonGen.txt 末尾 ⇒ 窗口内刷不出） | **已解（合入 `6f35b049f`）** | 卡 `openmir2-fixture-monsters-prepend`：改为**前插**（index 0、首个闸门即取到）；真机已验证 prepend 生效（靶子刷出、5 条 `SM_STRUCK(31)`），kill→butch 三段结论复跑中 |
| 8 | 背包满导致「装武器」类前置无法声明 | **已解** | `a60e04ad9`：`fixtures.inventory_replace` / `inventory_drop`（真机往返两侧已 PASS，before/restored sha256 逐字节相同） |

**当前实况**：run3 = **24 条（blocked 3 ⇒ 入批 21）**（`run_batch --list` 实测 @ `362254623`）；操作单
`docs/window-exec-sheet-20261025.md` 已按 23 条刷新（`1af9d6647`）；台账
`docs/window-closure-ledger-20261025.md` 已二次刷新（`e3ac7c121`）。

**开窗当天只需勾这 5 项**（每条都有现成命令，全在操作单 §1）：

- [ ] `powershell -File E:/MirServer/port-status.ps1` ⇒ 5000–7200 全 DOWN、mysqld 3306 UP
- [ ] `python -I -B tools/window/preflight_private_stack.py` ⇒ 全绿（含新的 `castle_list_symmetry`）
- [ ] `bash tools/window/run_batch.sh --list` ⇒ `场景数 24（blocked 3 ⇒ 入批 21）`
- [ ] `bash tools/window/run_batch.sh --dry-run` ⇒ 自检全绿、不起栈
- [ ] 专用测试账号 + `assets`/`inventory`/`fixtures` 声明（`card_window` 在 restore 后、起栈前应用）

| 选项 | 含义 | 代价 |
|---|---|---|
| **A（推荐）** | 开一轮**全量**：23 条入批 21 条（含 `castle_merchant_tax_potion` 的 620s 落盘等待），独占机器 1.5–3 小时 | 需 disk + 独占；直接覆盖台账 #1/#3/#4/#6 的窗口行 |
| B | 最小集：`login_only + upgrade_retrieve + equip_struck_save + spell_cast + gamepoint_exchange` | 1 小时内可跑完，覆盖骨架 |
| C | 暂不开窗 | 条件② 的 gate 口径仍 PASS，但 196 张「登记型 PASS」继续没有真机证据 |

> **回复**：`②Q1=A`（推荐，请给时段）／`②Q1=B`／`②Q1=C`

---

## 2b. ②a 窗口首轮（W1）的两个**口径前提**——读证据前必须先看这两条

**本页写于 W1 开跑后 20 分钟**（master `cb65a9d34`）。W1 的 21 条正在跑，两类证据的可用性**不同**：

1. **帧面证据有效**：ident/字段/帧序/收件人集合等，逐帧对拍产出的判定可直接用于 R 卡收口。
2. **DB 面证据暂时全部不可用**：实测根因是 **harness 的存档时机不对称**——Rust 在断线时**同步**落库（`mir2-rs/bins/game-svr/src/lib.rs:22452`），C# 走异步离线支（`src/GameSrv/Word/WorldServer.cs:710-715` → FrontEngine→DBSrv→MySQL），而两侧的 DB 快照都在**停栈之前**拍，随后 C# 还被强杀。
   ⇒ 表现是「只有一侧的 DB 变了、另一侧仍是基线值」（典型：`characters_ablity.Weight` C#=14 / Rust=0，但两侧 `SM_ABILITY(52)` 帧里的 weight **都是 0**）。
   ⇒ **这不是行为差异**。修法已合入 `7e2d04b6c`（`wait_save_settled`：probe 后、拍库前等待存档落定，`require_changes` 默认要求观测到至少 1 次变化）；**真机自证尚未做**，完成后 **DB 面需整批重跑一遍**。
   ⇒ **注意连「DB 面 PASS」也不可信**：`gm_time` 与 `spell_cast` 的 DB diff 都是 0，但 0 也可能只是「**两侧都没落库**」（都停在基线）——与 FAIL 同一根因的另一面。所以本轮的 DB 面**既不能用来判红、也不能用来判绿**，只有加装「存档落定」证据之后才有意义。

> 换句话说：**今天这批能立刻用于收卡的是帧面判据；DB 面要等 harness 修好后的重跑。** 我不会拿 DB 面的 FAIL 去关任何 R 卡。

---

## 3. ③ Q2–Q5（口径未变，保留推荐）

**Q2｜无场景卡的处置**：**D（推荐）混合分档**（46 张补场景 W2 / 15 张视 Q3 / 1 张 E2E 登记）。`③Q2=D`
**Q3｜combat 夹具投入**：**A（推荐）**补「同屏怪物 + 伤害/配装夹具」，解封 `combat_hit`/`combat_strike`。`④Q3=A`
**Q4｜跨栈 RNG flaky 类**：**丙（推荐）**本轮按已知 flaky 登记；`login_only` 的 `Weight 14vs0` **不是 flaky**（真实数值缺口）。`⑤Q4=丙`
**Q5｜登记型豁免口径**：**A（推荐）**；**「登记型 PASS ≠ 真机证据」这句必须留在 `goal_gate` 卫生清单里**。`⑥Q5=A`

---

## 4. ④ walgit 主源——**已恢复（迁到 `OpenMir2-recover`），无需你回复**

- **原仓库 `gqf2008/OpenMir2` 已不可修复**：故障是**服务端自己写、5 秒后自己删**了 head WAL 段
  `…/log/0000000000003000.pb`（`store.put` 16:06:22 → `closed bundle slots settled` 16:06:26 →
  `store.delete` 16:06:27），WAL 头断 ⇒ 全部命令 500。根因取证见 `docs/walgit-outage-rootcause-20261025.md`（合并 `77ef50ddf`）。
- **已迁**：`gqf2008/OpenMir2-recover`，master / `main` / 10384 条 `refs/collab/*` / 101 分支 / 12 tag 全量补推，
  `ls-remote`、`fetch`、`push` 均 rc=0；本机 `remote walgit` 已指向新仓库（worktree 共享 config，各车道无需改动）。
- **原仓库原地保留只读**（不删、不写、不用空对象补 WAL）。
- **仍建议你做一件事（可选）**：两台机器（本 Windows + macOS 部署）**共用同一个 R2 桶**且都开 `maintain`
  ⇒ 有二次踩踏风险。要么本机 `walgit.toml` 的 `[server].roles` 改成 `["serve"]`，要么换独立桶/前缀。**我不擅自改环境配置。**

## 5. ⑤ 状态快照（`goal_gate` 实测）

- **快照**：`f72dbce7`（2026-10-25 14:57 +08，`goal_gate --full` 实跑）

| 条件 | 状态 | 实测（可复算） |
|---|---|---|
| ① 脚本派发 100% | **pass** | `parse-stats`：目标 **106/106**、调用 **11829**、未实现 **0**。**本轮已用最终 master 的新构建有效重验**（`mir2-script-tool` 于 `9a966b04b` 重建，sha256 `34ac7680…`）——此前的 PASS 用的是 00:44 的旧二进制、而之后有 3 笔改 `crates/script`，故旧证据曾作废 |
| ② R 卡证据 | **need_human** | R 卡 **227 张**：已完成 **15** / `window_required` **154**（另未合并 **5**）/ whitelist **48** / 已裁定登记 **3** / `merged_not_closed` **1**（仅 G11）/ 未开工或未合并 **1**（`openmir2-gm-script-fixture`，今天新开）/ 其余缺口 **0**。加总 15+154+5+48+3+1+1 = 227 ✓ |
| ③ B 类清零 | **pass** | recon `tracked=0`（rows 191 条非空、counts 一致） |
| ④ 门禁恒绿 | **pass** | **最终 tip `f72dbce7`** 上 `goal_gate --full` 复跑：**门禁 7 项 绿 7 / 红 0 / 需人工 0**（总 144s）。逐项：fmt / clippy / msgcodegen / whitelist_anchor_gate / m3 audit / `master_sync` 全 rc=0，`cargo_test_workspace` **rc=0 @117.0s**，三段数 **running 3315 = passed 3252 + failed 0 + ignored 63**、**141 个 suite 全 `ok`**（门内那次无计数，tester 在 `f72dbce7` 另跑一次 100s 取数，两次 rc 一致 ⇒ 非 flaky）；`master_sync` local==remote==`f72dbce7`。产物 `E:/tmp/window/goal-full-fd08ff3a9.json`（`.meta`：`GOAL_RC=2` 来自 ② 的 need_human、不是红门）。此前在 `2cb84220f` 的全量结论（绿 7/0/0、running 3315 = passed 3252 + ignored 63、两次 rc 一致非 flaky）仍作对照保留 |

> **四条件现在的真缺口只剩一处**：② 的 G11 1 张（§1 需你裁决）。①③ PASS；④ 在 `2cb84220f` 已 PASS、**最终 tip 待复跑一次 `--full`**（只是复跑，不是新缺口）。证据与可复算口径见 `docs/delivery-report-20261025-final.md`。

## 6. 本轮 `blocked` 项与解封条件（新表；每项写谁在做）

| 场景（run3） | 为什么还 blocked | 缺什么 | 谁在做 / 预计产出 |
|---|---|---|---|
| `butch_two_draws`（**#21**） | **击杀步已合**（`2c271ddb9`）；真原因是**怪物夹具追加到 MonGen.txt 末尾**：刷怪线程每 `RegenMonstersTime=200ms` 只前进一条（部署 `ProcessMonsterMultiThreadLimit` 未配=1），单表 3081 行 ⇒ 一轮 ≈616s，而 #21 只给 60s ⇒ 追加行永远轮不到 | **前置插入**（不是加大超时） | 卡 `openmir2-fixture-monsters-prepend`（tester，`5af1068c4` 双审中）；解封后 dev-1 复跑 kill→butch（`openmir2-butch-two-draws-unblock`） |
| `clearmapmon_t118`（**不在 run3**） | `movr M3 3` 分支随机；输支会扣佛牌 + 写名单（终局） | 两侧同 `-Seed N` 且确认 `movr=2` → 记 seed → **owner/Leader 裁决入批** | 夹具与 hook 已合 `15dbbe843`；判 seed 的生命周期归窗口执行（`docs/window-scenes-clearmapmon-20261025.md` §1.6） |
| `castle_merchant_tax`（**#22**） | ① 买**武器**腿需 652 的 ClientItem buffer 取 `MakeIndex`（运行时计数，内容推不出）；② 「`SaveTick`/600s 门不可观测」**已证伪**（SaveTick 不落盘；写盘门 `ConsoleShowUserCountTime` 可被 `config_overrides` 覆盖）⇒ 真缺口是**税入账原因未定位** | ① probe 的 EDCode buffer 解码（另卡）；② 两行诊断日志 `已读取 [N] 个城堡信息` / `在线数: <N>` | 买药腿已可跑（**#23**，税 4 / 判据 `650.recog=9912`）；② 归 castle 卡 |

> **红线（本表遵守）**：不把「等 owner / 等卡」写成已解决；不把**登记型 PASS** 写成真机证据；
> 上表三行都是**当前真跑不了**的场景，解封后须按 `openmir2-blocked-two-sources-doc` 同时改**场景 JSON 与 run3 清单行**两处。

---

## 7. 本页不包含（避免重复消耗 owner 注意力）

- `rules.json` PENDING 复核、`T-161..T-222` 复核口径、三张替代判据卡、私有 C# 栈去留：
  见 `docs/needs-human-20261024.md` §0（旧页只读保留）。
- 本页只列**今天发生变化**的决策点：G11、窗口（Q1）、Q2–Q5 口径、walgit、blocked 三项。
