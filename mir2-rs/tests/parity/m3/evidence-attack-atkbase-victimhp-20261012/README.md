# evidence-attack-atkbase-victimhp-20261012 — M3②③ 夹具两项：`atk_power_base` 归因 + 受害者 HP pin 真生效

- 卡：`openmir2-m3-combat-skills-align`（②③，issue `a2ef74aa`）／车道 `m3bag`／principal `openmir2-worker-4`。
- 来源：owner 2026-10-12 22:35 brief（`E:/tmp/openmir2-watch/brief-m3-bag-equip.md`）里「另外两件」：
  1. **`atk_power_base` 的 `C# 16 / Rust 0`**（m3cs 判读 = 进程级并发实体）⇒ 判定「谓词分错站点」还是「真有第二个攻方实体」；
  2. **`-AttackVictimHp 30000` 不生效**（oracle `RecalcLevelAbilitys` 按 L1 重算成 19）⇒ 让 pin 真抬起来 + fail-closed 门。
- 写集：`mir2-rs/tests/parity/m3/**`（**零 `.rs`**、未动 `src/**`/`sql/**`/`MirClient`；**未起栈、未占端口**）。

## 1. 改动

| 文件 | 改动 |
| --- | --- |
| `gate_guards.ps1` | `Get-M3AttackVictimLevelAbility`（oracle `RecalcLevelAbilitys` 公式移植）、`Test-M3AttackVictimHpSelfConsistent`、`Test-M3AttackVictimSurvives`、`Test-M3AttackVictimPostPinHp`；`$script:M3AttackVictimLevel=255`/`Job=0`/`Hp=20669`；`Get-M3AttackPinSql` 由等级反推 Hp + 钉 `c.Job` + 不自洽即 throw；`Format-M3AttackPose` 缺省改派生口径 |
| `run_pair.ps1` | `-AttackVictimHp`/`-AttackVictimLevel` 缺省 0（⇒ gate_guards 口径）；攻击 pin 与 `Get-M3AttackPoseLabel` 用该口径；攻击 pin 的 `$pinCtx.Level` 用受害等级 |
| `run_attack_pair.ps1` | 缺省 0 解析；自检 ⑥（pose=`level=255 vhp=20669`、pin 含 `a.Level=255`/`c.Job=0`）、⑥d（oracle 公式锚 + 自洽门 + 幸存上界 + 改坏必红）、⑥e（post-pin HP 门三向）；真机 run 后新增 `VICTIM_HP_POSTPIN` fail-closed 门 |
| `run_bag_equip_pair.ps1` | ⑪ 的 `Get-M3AttackPinSql` 调用去掉旧 `-Level 1 -Hp 30000`（用派生口径） |
| `whitelist.md` | T-29 第十一轮登记 |

## 2. 判据与证据

| 文件 | 判据 | 结果 |
| --- | --- | --- |
| `01-atk-power-base-attribution.txt` | 归因：cs:892 出现位置（1..24 在 attacker 第一发之前）+ 紧跟 cs:41 构成完整非玩家攻击循环 + oracle 只有非玩家 actor 走该覆写 | **确实有第二个（非玩家）攻方实体**；不是谓词分错站点 |
| `02-victim-hp-recalc.txt` | oracle 链路 + 真机旧会话说 `maxhp=19` ⇒ 新门 `ExpectedHp=20669` 判红 / `ExpectedHp=19` 判绿 | 旧 pin 不生效（根因确认）；修法 = Level/Job 与 Hp 一起 pin |
| `03-selfcheck-nostack.txt` | `run_attack_pair.ps1 -SelfCheckNoStack`（PS 5.1） | **rc=0 `SELFTEST_RED GREEN`**（含 ⑥d/⑥e） |
| `04-mutation-teeth.txt` | 改坏必红：`Level 255→1`（⇒ Hp 19）真跑 RED；合成 `maxhp=19`/零 struck 必红 | **rc=1**（`f_f_victim_may_die` 等 3 键）；还原后回 GREEN |
| `05-gates.txt` | 四门禁（隔离 `CARGO_TARGET_DIR`；本片零 `.rs` 仍复跑） | fmt=0 / clippy=0 / **84 suites·682 passed·0 failed·16 ignored** / msgcodegen=604 rc=0 |

## 3. 结论（两项）

1. **`atk_power_base`（C# 16 / Rust 0）= 真有第二个非玩家攻方实体**（别的地图上的守卫/怪物 AI），
   调用点级 divert 无法按实体隔离 ⇒ 它的取数被吸进同一专用流。**本片不改 m3cs 定的接口**（不动
   `attack_sites.tsv` / `divert-sites.txt` / Rust `combat.rs:67`），只做归因 + 登记，建议由 owner 裁决：
   (a) 把非玩家路径从本场景 divert 表摘掉（需两侧表 + Rust 代码同步），或 (b) 给 hook 加实体过滤。
   在落地前 DoD②「逐 k = 0」不可达（**本片不放宽任何判据**）。
2. **受害者 HP pin 现在真生效**：pin `Level=255/Job=0`（oracle 公式 ⇒ `MaxHP=20669`）而不是光写
   `Hp=30000`；新增 fail-closed 门（自检 ⑥d/⑥e + 真机 `VICTIM_HP_POSTPIN`）确保「pin 后 maxhp == 期望」。

## 4. 未做 / 边界（如实登记）

1. **未起真机栈**：F-F 的端到端（C#/Rust 两侧 `VICTIM_HP_POSTPIN ok`、序列完整、`attack_diff` 逐 k=0）
   需要 5000/7000/7100/7200 窗口，**本片不起栈**（m3cs 取窗跑）。本片证的是公式/门/改坏牙齿全绿。
2. `atk_power_base` 的正式处置（(a)/(b)）未做——需要 owner 裁决 + m3cs 写集（`crates/world`）。
3. 「共用站点（`hit_gate`/`struck_damage`）也被外来攻击污染」是同一根因的另一面，本片只在
   `01` 里登记，未做实体过滤。

## 5. 残留物清单

- 仓外临时物（可删）：`E:/tmp/m3bag-ff-*.txt`、`E:/tmp/m3bag-atkbase-attribution.txt`、
  `E:/tmp/gate_guards.ps1.bak`、`E:/tmp/m3bag-ff-postpin-real.txt`。
- 仓库内：仅本目录 + 上述 `.ps1`/`.md` 改动；未起栈 ⇒ 无进程/端口残留（仅共享 3306）。
