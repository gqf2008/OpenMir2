# M3②③ 真机同种子攻击对拍 · 第三轮夹具 blocker 证据片（F-H / F-I / F-J）

- 卡：`openmir2-m3-combat-skills-align`（issue `a2ef74aa`）／车道 `m3cs`／principal `openmir2-worker-5`
- base：`refs/heads/master` = `0889ccf7e344fbcbfe4daedfdc12fda5f8a4226c`（HEAD，可 FF）
- 触发：条件四条全命中（T-30 closed + `gate_guards.ps1:546 c.IncHealth=0,c.IncSpell=0`
  + `gate_guards.ps1:442 M3AttackVictimHp=30000` + `run_bots.ps1` 有 `M3ATTSEQ role=done` 收尾）
  ⇒ `T30_MERGED`。tick24（2026-10-12 13:22–13:35）按 brief 真机跑了 C# 20 击冒烟（seed=42），
  结果 rc=1 且与 m3bag 13:07 冒烟逐行同形 ⇒ 定位出本片三条**夹具 blocker**。
- 本片**零代码/零仪器改动**，**未起栈、未占端口**（只读源码 + 已有真机原始件 + 离线最小复现）。
  三条 finding 全在 m3bag 写集（`tests/parity/m3/**`、`tools/botload/run_bots.ps1`）⇒ 本车道只读、
  提 finding，请 owner 路由 m3bag 出一片「夹具可 GREEN 化」修复。

> 本片**不产出 DoD①② 的判据**（被三条 blocker 卡住）；它是 blocker 的可复现原件入库，
> 目的与 `evidence-attack-pair-blocker2-20261012/**` 相同：让夹具缺陷可审、可路由、可回归。

---

## 1. 三条 finding

| ID | 级别 | 位置 | 问题 | 最小复现 |
|----|------|------|------|----------|
| **F-J** | **Critical（恒 RED）** | `mir2-rs/tests/parity/m3/run_attack_pair.ps1:148-155`（函数）+ `:645-648` / `:182-183`（调用点） | `Invoke-AttackAlign` 把 `python` 的 stdout 混进返回值 ⇒ `$alignRc`/`$rc` 是 `Object[]`；PowerShell 里 `Object[] -ne 0` 过滤出非零元素 ⇒ **恒为真** ⇒ C# 侧无论 align 成败都 `RED …（rc=ALIGN_OK …）`，`-Side rust -PeerSession` 恒 `PREREQ_MISSING` ⇒ 跨侧 `attack_diff.py diff` **永远走不到** | `05-fj-minimal-repro.txt` |
| **F-H** | Important（阻塞） | `tools/botload/run_bots.ps1:243-245`（break）+ `:283`（GREEN 门）+ `:291-292`（RED） | `-AttackSequence` 收尾等 `done` 的阈值写成 `$Count`（=2），但 T-29/`RobotPlayer.cs:633` 只武装 attacker（账号序号 0，`AppService.cs:68`）⇒ victim 永不发 `role=done` ⇒ `attackDone` 恒 1 ⇒ **永不 break（白等满 `TimeoutSec`=600s）且恒 RED** | `06-fh-minimal-repro.txt` |
| **F-I** | Important（阻塞） | attack_pose 钉点 + `attack_diff.py:104/109` 的 `my` 过滤（F-G）+ oracle 全局 `RandomNumber` 抽数账本 | 同配置、同种子（42）、同 master 三次运行 attacker 命中 struck 数 = **0 / 2 / 7**；`align` 的 `hits/dodges/sum` 因 attack 事件缺 `my` 字段把**怪物对 victim 的 SM_STRUCK** 也算进当前 k ⇒ DoD②「命中/躲闪统计两侧一致」**当前无仲裁力**；钉点 (289,618) 在怪物活动区，死等期间世界态漂移 | `10-fi-hit-reproducibility.txt` |

### 建议修法（供 m3bag 采用；本片未改）

- **F-J**：`Invoke-AttackAlign` 内 `& python … | Out-Null`（或 `$null = & python …`）后再
  `return [int]$LASTEXITCODE`；并把**无栈自检从「绕过生产函数」改为「调用生产函数」**——
  现在自检 ⑧（`:376-379`）自己 `| Out-Null` 后用 `$LASTEXITCODE`，因此**测不出**这个 bug。
  机器可判验收：把生产 `Invoke-AttackAlign` 的 `| Out-Null` 去掉 ⇒ `-SelfCheckNoStack` 必红。
- **F-H**：done 阈值改为**被武装的 attacker 数**（`-AttackPinPair` 下 =1；非 pin 场景下 = `$Count`），
  由 `run_pair` 显式传入（如 `-AttackBots`/`-AttackRole`），或让 BotSrv 两个 bot 都武装（二选一，owner 裁）。
  机器可判验收：`-AttackPinPair` 下 `attackDone` 期望=1；改回 `$Count` ⇒ 自检必红。
- **F-I**：① attack 标记补 `"my":<本假人 ActorId>`（`RobotPlayer.cs:667`）或 align 改按 `id` 关联，
  使 `src==my` 过滤真生效（F-G）；② 保证**攻击格无第三方**（换安全钉点/清怪）或把世界态冻结；
  ③ 自证「同一栈实例内 20 击序列可复现」再谈跨侧 diff。

---

## 2. 本轮真机原始件（tick24 实跑，随本片入库）

- `01-smoke-csharp-20.txt`：`run_attack_pair.ps1 -Side csharp -Bots 2 -AttackCount 20 -AttackPinPair -Prefix m3csm1323 -Seed 42`
  的完整 stdout（rc=1）。两条 RED 与本片 finding 对应：
  - `RED: 2 个假人的攻击序列只跑完 1/2（缺 M3ATTSEQ role=done；count=20）`（F-H）
  - `RED: attack_diff align 失败（rc=ALIGN_OK … attacked=20 hits=0 dodges=20 sum=0）`（F-J，且命中平凡）
- `02-attack-raw-20261012-132228.txt`：该会话 `attack_raw.jsonl`（arm=1 / attack=20 / **struck=0** / done=1）。
- `03-attack-seq-20261012-132228.txt`：`align` 产物（全 `hit=false`、`summary hits=0 sum=0`）。
- `04-bots-20261012-132258.txt`：BotSrv `bot.log`（只有 attacker `m3csm13230` 有 arm/attack/done）。

## 3. 离线最小复现 / 锚点 / 四门禁

- `05-fj-minimal-repro.txt`：F-J 生产函数逐字复制的最小复现（返回 `System.Object[]`、`-ne 0` = True；
  正确写法返回 `Int32` 0）。
- `06-fh-minimal-repro.txt`：F-H 判据复现（`success=2 attackDone=1 Count=2` ⇒ 不 break + RED）。
- `07-fj-code-anchor.txt` / `08-fh-code-anchor.txt`：逐行锚点（含无栈自检 ⑧ 的绕过点）。
- `10-fi-hit-reproducibility.txt`：F-I 三次运行的 role/src 统计 + align summary（含 m3bag 两会话路径）。
- `11-gates-fmt.txt` ~ `14-gates-msgcodegen.txt`：四门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/m3cs12-target`、`--offline`）
  —— fmt rc=0；clippy rc=0；`cargo test --workspace` **79 suites / 648 passed / 0 failed / 16 ignored**；
  msgcodegen `--check` **604** 一致。本片证据面零 `.rs` 改动，门禁结果与 master 0889ccf7 同。

## 4. 口径纪律

- **不新增白名单**：F-H/F-I/F-J 都是夹具缺陷，不是差异豁免。
- **未改** `run_attack_pair.ps1` / `run_pair.ps1` / `run_bots.ps1` / `gate_guards.ps1` / `attack_diff.py`
  （m3bag 写集）、`bins/game-svr/**`、`crates/**`、`src/**`、`sql/**`、表结构/脚本语法/数据格式。
- 本片**未起任何服务端/客户端/假人进程**；仅只读共享 MySQL 之外的仓内文件（连 3306 都未连）。
  窗口实拍见 `09-ports-free-134511.txt`（十端口全 FREE，仅共享 3306 pid=29372）。

## 5. 残留物清单

- 仓内新增：本目录（`README.md` + 14 件编号原始件/复现/门禁 + `selfcheck-selfcontained.txt` + `SHA256SUMS.txt`，共 17 件）。
- 仓内临时物：无（`git status` 只剩本片新增）。
- 仓外：`E:/tmp/m3cs25/{fj-repro.ps1,fh-repro.ps1,repro/,*.seq.jsonl}` 为复现脚本/产物（可删）；
  `E:/tmp/m3cs24/**` 为 tick24 真机栈现场（可删，本片只复制其中 3 件）。
- 未占用端口；未起进程。
