# M3②③ 真机同种子攻击对拍 —— 夹具第二轮阻塞与 finding（2026-10-12，m3cs / worker-5）

**结论：T-30 + F-A 两前置已齐（本 tick 真机实跑确认），但真机对拍仍被夹具的三处缺陷挡住，
本 tick 未产出 `attack_diff` 判据（未达成 DoD①②）。** 本片是 finding 与原件的入库，
**不是对拍结果**；`attack_diff.py diff` 从未运行（序列没跑完整，夹具自己先 RED）。

| finding | 层 | 位置 | 影响 |
| --- | --- | --- | --- |
| **F-D**（Important） | pin SQL 数据越界 | `gate_guards.ps1:436` / `run_attack_pair.ps1:68`（缺省 `-AttackVictimHp 100000`） | 真机 pin `ERROR 1264` ⇒ 侧运行在 SQL pin 相即中止，**硬阻塞** |
| **F-E**（Important） | 驱动不收尾 | `tools/botload/run_bots.ps1`（`-AttackSequence` 无「等 `M3ATTSEQ done`」分支） | 攻击序列在登录成功后立即被 kill，只跑到 **12/120** ⇒ `OPS_ATTACKSEQ attacks=12 done=0` ⇒ **硬阻塞** |
| **F-F**（Important） | 夹具前提失效 | `characters_ablity.Hp/MaxHP` pin vs oracle `PlayObject.RecalcLevelAbilitys()` | victim 进世界 MaxHP 被重算为 19，**19 击即死** ⇒ 120 击序列后半段无 struck，夹具「抬高 HP 防死亡」的前提不成立 |

三处全在**夹具/驱动**（m3bag 写集或共享 `tools/botload`）⇒ 本 lane（m3cs）**只读不写**，
只把「位置 + 可复现 + 修法」交回；未自签、未 merge、未改仪器。

**已确认修好并真机生效的**：F-A（`a.IncHealth/a.IncSpell → c.IncHealth/c.IncSpell`）本 tick
真机验证通过 —— `run_attack_pair.ps1` 的 **1.5) pin SQL 可执行性**（4 条 UPDATE，真实行）与
**F-C 种子注入断言**（`SEED_INJECT_OK csharp seed=42`）都绿，C# 半越过了历史上卡死的 pin 相。

---

## 本轮实测（可复现）

```text
# 起 C# oracle 栈（E:\MirServer\M2GameSvr-hook-m3cs16 影子 + m3/rng-hook，MIR2_RNG_SEED=42）
powershell -File .../start-csharp-hook.ps1 -Seed 42            # CSHARP_STACK_GREEN，十端口全听
# 攻击对拍（缺省 victim HP）
powershell -File mir2-rs/tests/parity/m3/run_attack_pair.ps1 -Side csharp -Bots 2 -AttackCount 120 -AttackPinPair
#   ⇒ ERROR 1264（F-D），见 04-repro-FD.txt / 02-schema.txt
# 对称改到 schema 合法值（避开 F-D，暴露 F-E/F-F）
powershell -File .../run_attack_pair.ps1 -Side csharp -Bots 2 -AttackCount 120 -AttackPinPair -AttackVictimHp 30000
#   ⇒ OPS_ATTACKSEQ attacks=12 done=0（F-E）；SM_STRUCK maxhp=19（F-F），见 05/06/07
```

`attack_raw.jsonl`（06）逐条：`arm count=120` → `attack k=1..12` → 仅 2 条 `struck dmg=1 hp=18 maxhp=19`。

---

## F-D（Important，夹具 pin 数据越界 —— 硬阻塞）

**位置**：`mir2-rs/tests/parity/m3/gate_guards.ps1:436` `$script:M3AttackVictimHp = 100000`
（与 `run_attack_pair.ps1:68` 的缺省 `-AttackVictimHp 100000` 同值）；经 `Get-M3AttackPinSql`
写进 `characters_ablity.Hp / MaxHP`；`run_pair.ps1:186-187` 对 attacker（序号 0）与
victim（序号 1）**都**用该值。

**根因**：`characters_ablity.Hp / MaxHP` 在冻结 schema 里是 **`smallint(11)` 有符号（上限 32767）**
（`sql/mir2_db.sql:6081`/`6083`，见 `02-schema.txt`），`100000` 必越界（`STRICT_TRANS_TABLES`）。

**实跑**：`ERROR 1264 (22003) at line 1: Out of range value for column 'Hp' at row 1`
（C# 半 `run_pair.ps1:203` throw ⇒ `RED: run_pair（csharp 侧）未通过`）。

**为什么 F-B 可执行性极小门没咬住**：`Test-M3PinSqlExecutable` 以
`WHERE ChrName='__m3_no_such_char__'` 真执行（**0 行改动**是刻意设计）⇒ JOIN 出 0 行，
MySQL 不评估赋值表达式 ⇒ **数据/范围类错误不触发**，它只覆盖 prepare 期错误（未知表/列/别名，
即 F-A 那一类）。

**最小复现**（`04-repro-FD.txt`）：对已存在角色直接执行 pin，`Hp=100000` ⇒ `ERROR 1264` rc=1；
`Hp=30000` ⇒ rc=0。

**建议修法（m3bag）**：① 缺省 `-AttackVictimHp` / `$script:M3AttackVictimHp` 降到 ≤32767；
② 让 F-B 有牙齿：pin 预检在**事务内对真实行**执行后 `ROLLBACK`，断言 rc=0（0 行口径抓不到范围错）。

---

## F-E（Important，驱动不收尾 —— 硬阻塞）

**位置**：`tools/botload/run_bots.ps1`。它有 `-BagEquipSequence` 分支（`:190-202`：`$bagDone`
数 `M3BAGSEQ done`，满足才 `break`），但**没有**对应的 `-AttackSequence` 分支；收工条件落到
`:212` `elseif ($success -ge $Count) { break }`（登录成功即 break）。全文 `attack` 命中只有参数与
`CONFIG_ATTACKSEQ` 打印（见 `08-runbots-no-attack-wait.txt`）。

**影响**：run_bots 一看到 `login_ok=2/2` 就 kill BotSrv；而攻击序列要 `AttackStartDelayMs`(3000)
+ `AttackCount`(120) × `AttackStepMs`(500) ≈ 63s，且序列在进世界后才开始 ⇒ 只跑到 **k=12**。
`run_pair.ps1` 的同序列判据随即 RED：`M3ATTSEQ attacks=12 done=0`（见 `05`）。

**建议修法**：照 `-BagEquipSequence` 的先例加 `-AttackSequence` 分支：等
`M3ATTSEQ done`（每个假人一条）计数到 `$Count` 再收工，超时上限沿用 `-TimeoutSec`。

---

## F-F（Important，夹具前提失效）

**位置/依据**：夹具注释 `gate_guards.ps1:435` 写「victim 的 HP/MaxHP 抬高（防 100+ 次内死亡后无 struck）」，
pin 写 `characters_ablity.Hp/MaxHP`。但 oracle 在进世界后由 `PlayObject.RecalcLevelAbilitys()`
（`src/M2Server/Player/PlayObject.cs:5254`）按**等级公式**重算 `Abil.MaxHP`
（Warrior `:5295`），再把 `Abil.HP` clamp 到该 MaxHP（`:5309-5311`）——**与 DB 的 MaxHP 无关**。

**实测**：即使 pin 值合法（30000），受击方 `SM_STRUCK` 的 `maxhp=19`（level 1），
每击 `dmg=1` ⇒ **约 19 击后死亡**，120 击序列后半段不可能有有效 struck ⇒
判据「逐次伤害序列 ≥100 次」在这一 pin 口径下不可达。

**建议修法（择一，均需两侧同口径）**：把受害者钉到足够高**等级**（如 `characters.Level` /
`characters_ablity.Level` 抬到使等级公式 MaxHP ≥ 所需量级），或在 oracle 的等级/配置口径下
设计可承受 120 击的 victim；单纯写 `characters_ablity.Hp/MaxHP` 不生效。

---

## DoD 状态（本 tick）

- DoD① ②（真机同种子 120 次攻击伤害序列 + 命中/躲闪统计跨侧 diff=0）：**未达成**，
  夹具阻塞（F-D/F-E/F-F）。**未放宽**、未登记白名单（这些是夹具缺陷，不是行为差异）。
- 未起 Rust 栈（C# 半已先 RED，继续起 Rust 只会重复同一夹具阻塞；不并行起第二套栈）。
- 窗口：本 tick 归 m3cs，起栈前实拍、收工停栈 + 十端口实拍（`00`/`09`），**已释放**。
- 写集：本 lane 未改 `gate_guards.ps1` / `run_attack_pair.ps1` / `run_pair.ps1` /
  `tools/botload/run_bots.ps1`（夹具写集，只读）；未动 `src/**`、表结构、脚本语法、数据格式。

## 证据清单

| 文件 | 内容 |
| --- | --- |
| `00-ports-before.txt` | 起栈前端口实拍（仅 3306） |
| `01-csharp-stack-start.txt` | C# oracle 栈（影子 + rng-hook，seed=42）启动 GREEN |
| `02-schema.txt` | `characters_ablity.Hp/MaxHP smallint(11)` + live `SHOW COLUMNS` + `sql_mode` |
| `03-oracle-recalc-maxhp.txt` | `PlayObject.RecalcLevelAbilitys()` 原行（F-F 根因） |
| `04-repro-FD.txt` | F-D 最小复现（Hp=100000 红 / 30000 绿） |
| `05-run-vhp30000-attack-cut.txt` | C# 半完整输出：F-A/F-C 绿；`attacks=12 done=0` RED |
| `06-attack-raw-vhp30000.jsonl.txt` | 会话 `attack_raw.jsonl`（arm + k=1..12 + 2 条 struck） |
| `07-bot-log-attack-cut.txt` | BotSrv 日志：`M3ATTSEQ` + `maxhp:19` + NullReference 噪声 |
| `08-runbots-no-attack-wait.txt` | `run_bots.ps1` 无 attack 收尾分支（F-E 根因） |
| `09-ports-after.txt` | 停栈 + 十端口实拍（窗口已释放） |
| `10-gates-fmt.txt` | `cargo fmt --all -- --check` rc=0 |
| `11-gates-clippy.txt` | `cargo clippy --workspace --all-targets -D warnings` rc=0 |
| `12-gates-test-workspace.txt` | `cargo test --workspace`（79 suites / 648 passed / 0 failed / 16 ignored） |
| `13-gates-msgcodegen.txt` | `msgcodegen --check` 604 条一致 |

四门禁本片自跑（隔离 `CARGO_TARGET_DIR=<worktree>/mir2-rs/target`、`--offline`，2026-10-12 12:50）：
fmt rc=0；clippy rc=0；`cargo test --workspace` 79 suites / 648 passed / 0 failed / 16 ignored；
`msgcodegen --check` 604。本片只新增文本（报告 + 证据），未改任何 `.rs`/`.ps1`/schema。

原始运行目录（仓外，未入库；snap_before 的 21MB TSV 快照按既有会话惯例不入库，可由 `dbsnap` 复算）：
`E:/tmp/m3cs21-evidence/csharp-stack/**`、会话 `E:/tmp/m3cs21-evidence/sessions/csharp-20261012-124550/**`。
