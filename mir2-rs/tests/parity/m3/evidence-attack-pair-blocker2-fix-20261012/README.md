# 证据：修 M3②③ 攻击夹具第二轮真机 blocker F-D + F-E + F-F（卡 `openmir2-m3-attack-pair-fixture`）

- 卡：`openmir2-m3-attack-pair-fixture`（issue `76cc08b8`）。
- 上游 finding：m3cs tick21 第二轮真机对拍（owner 路由 comment `460c2ac00d02ad6e5d89ba39e7f0d47735b66b9f`，2026-10-12 12:54）
  暴露三条 Important（F-D/F-E/F-F），全在夹具/驱动 ⇒ 派回本卡；可复现原始件 `E:/tmp/m3cs21-evidence/**`。
  本片修 **F-D + F-E + F-F**（F-F 选 option ③，见下）。
- 性质：**夹具/驱动/判据侧修复**；零端口（只用共享 MySQL 3306）、**未起服务端栈**、未 merge、未自签 review。
  写集 = `mir2-rs/tests/parity/m3/**` + `tools/botload/run_bots.ps1`，**零 `.rs`**；
  `src/**`、`crates/**`、`bins/**`、`sql/**`、`MirClient` 全 0 改。

## F-D（Important）——pin 抬高值超出 `smallint` 列域

`characters_ablity.Hp / MaxHP` 是 `smallint(11)` **有符号**（冻结 `sql/mir2_db.sql:6081-6084`，上限 32767）；
旧缺省 `100000` 经 `Get-M3AttackPinSql` 写该列必越界。`run_pair.ps1:186-187` 把 attacker 与 victim 的
`Hp/MaxHP` **都**设成该值 ⇒ 两条 UPDATE 都会炸。

**修法（三处缺省，语义不变）**：`run_attack_pair.ps1` / `run_pair.ps1` / `gate_guards.ps1::Get-M3AttackPinSql(*)` /
`Format-M3AttackPose` 的 `-AttackVictimHp` / `$script:M3AttackVictimHp` 缺省 `100000` ⇒ **`30000`**
（≤32767，且远超 120 次 1 级攻击的累计伤害；`attack_pose` 的 `vhp=` 两侧逐字节相同 ⇒ 不改跨侧仲裁力）。

**三道 fail-closed 护栏（改回 100000 必红）**：

1. **范围护栏**：`gate_guards.ps1::Test-M3AttackVictimHpRange`（唯一口径，`1..32767`）——
   `run_attack_pair.ps1`（真实侧运行入口，入口实现护栏之后）与 `run_pair.ps1`（`-AttackPinPair` 时）**启动即判**。
2. **无栈自检 ⑥c**：用本脚本自己的缺省断言合规，并断言 `100000` 必判红
   （改坏缺省 ⇒ `SELFTEST_RED RED: attack_victim_hp_default_out_of_range=100000`，见 `02-mutation-red.txt`）。
3. **⑥b 镜像表真执行**：见下 F-B 加固。

## F-B 加固——可执行性门从「prepare 期」扩到「数据/范围期」

原 `Test-M3PinSqlExecutable` 以 `WHERE ChrName='__m3_no_such_char__'` 真执行：JOIN 出 **0 行**时 MySQL
**不评估赋值表达式** ⇒ 只能抓 prepare 期错误（未知表/列/别名，如 F-A 的 `a.IncHealth`），抓不到范围错误（F-D）。

**改法（真实数据零改动）**：在当前会话内 `CREATE TEMPORARY TABLE mir2_db.tmp_characters /
tmp_characters_ablity LIKE mir2_db.<真表>`（列定义逐列等同冻结 schema），从真实表各拷 1 行、把主键/角色名
改成固定探针值（`Id=1`、`ChrName='__m3_pin_probe__'`、`PlayerId=1`），再把每条 pin `UPDATE` 的
`mir2_db.<真表>` 重写成 `mir2_db.tmp_<真表>` 后**真执行**。prepare 期（`ERROR 1054`）与数据期（`ERROR 1264`）
都被 STRICT 模式报错；改动只落临时表；引用未镜像表或两表为空 ⇒ fail-closed。
`-SelfCheckNoStack` **⑥b** 两向机判（生产 rc=0；改坏 `a.IncHealth` ⇒ 1054；改坏 `Hp=100000` ⇒ 1264）；真实侧 **1.5)** 预检同门。

## F-E（Important）——`-AttackSequence` 没有「等 M3ATTSEQ done」收尾分支

`tools/botload/run_bots.ps1` 的 `-BagEquipSequence` 已等 `M3BAGSEQ done`，但 `-AttackSequence` 没有对应分支
⇒ 假人在 `login_ok == Count` 处立即被 kill，序列只到 **12/120、done=0**
（原始件 `E:/tmp/m3cs21-evidence/csharp-stack/run-csharp.txt:73`）。

**修法（与背包序列同口径）**：新增唯一计数口径 `Get-M3SeqDoneCount`（attack = `M3ATTSEQ {"role":"done"...}`，
`RobotPlayer.cs:674`），`-AttackSequence` 分支在循环里等**每个假人一条 done** 才收工；
超时/未 done ⇒ 不 break，末尾如实报 `attacks/done` 并 **RED**（fail-closed）。
summary 增 `attack_seq_done` / `attack_sequence`；GREEN 文案带 `attack_seq_done=N/N`。
新增 `run_bots.ps1 -SelfCheckNoStack` 离线自检（合成日志计 attack done 2/1/0 + bag done 2/0；改坏计数正则 ⇒ RED），
由 `run_attack_pair.ps1 -SelfCheckNoStack` **⑭** 调用。

## F-F（Important）——pin 的 Hp/MaxHP 会被 `RecalcLevelAbilitys()` 重算 ⇒ 选 **option ③（显式建模死亡）**

`RecalcLevelAbilitys()`（`src/M2Server/Player/PlayObject.cs:5254-5312`；原始件
`evidence-attack-pair-blocker2-20261012/03-oracle-recalc-maxhp.txt`）只由 `Abil.Level` + 配置公式算
`MaxHP`，与 `characters_ablity.MaxHP` 无关，且 `HP` 再被 clamp 到该 `MaxHP` ⇒ 1 级战士 `MaxHP=19`，
pin 抬高**不生效**、约 19 击即死。

选型（写清）：**③ 显式把「第 k 击死亡」建模进判据**——选它的理由：① 进世界后第二遍 pin 改不动服务端
内存里的活值；② 靠 pin 输入把重算 MaxHP 抬到「120 击不死」需真机调参且可能把伤害压成 0（破坏非平凡门）；
③ 最稳且离线可判：两侧同种子 + 同 pin ⇒ 伤害序列逐项相同 ⇒ 死亡时点也必须相同，死亡后无 struck 的步
算 dodge，逐项比即可。

**改法**（`attack_diff.py`）：
- `align_events` 的 summary 增 **`death_k`**（首个「命中且 `hp<=0`」的 k）与 `min_hp`（唯一口径 `death_stats`）；
- `diff_seq` 除 `hit/dmg` 外**逐项比 `hp`/`maxhp`**，并在 summary 里比 **`death_k`**（死亡时点不等 ⇒ RED）；
- `mutate` 新增 `--field hp|maxhp|death_k`（造反例）；
- `selftest` 新增三条：死亡容忍（两侧同在 k=7 死 ⇒ GREEN）、死亡时点不等（`death_k 7→8` ⇒ RED）、
  单步 HP 不等（`k=3 hp: 12→999` ⇒ RED），**16/16 GREEN**（见 `12-attackdiff-selftest.txt`）。

> `attacked` 仍要求等于 N（攻击步照发）；死亡步算 dodge，非平凡门要求 `hits>=1 && sum>0`（死亡前必满足）。

## 判据原始输出（命令 + 输出 + 路径 + hash）

| 文件 | 命令 | 结果 |
|------|------|------|
| `01-selfcheck-nostack-green.txt` | `powershell ... -File mir2-rs/tests/parity/m3/run_attack_pair.ps1 -SelfCheckNoStack` | **GREEN（rc=0）**；⑥ `vhp=30000`；⑥b 生产 rc=0、改坏 `a.IncHealth` 真 `ERROR 1054`、改坏 `Hp=100000` 真 `ERROR 1264`；⑥c 缺省合规；⑭ F-E rc=0 |
| `02-mutation-red.txt` | 同上，但先把 `run_attack_pair.ps1` 的 `-AttackVictimHp` 缺省改回 `100000` | **RED（rc=1）**；`SELFTEST_RED RED: attack_victim_hp_default_out_of_range=100000` |
| `03-repro-sql.txt` | 活库复现：镜像临时表 + 探针行，`Hp=100000` vs `Hp=30000` | 旧值 `ERROR 1264 (22003)` rc=1；新值 rc=0；真实表探针名计数 0、行数不变 |
| `04-selftest-gate-47of47.txt` | `powershell ... -File mir2-rs/tests/parity/m3/selftest_gate.ps1` | **47/0 GREEN（rc=0）** |
| `05-bagequip-selfcheck-green.txt` | `powershell ... -File mir2-rs/tests/parity/m3/run_bag_equip_pair.ps1 -SelfCheckNoStack` | **GREEN（rc=0）** |
| `06-gates-fmt.txt` | `CD mir2-rs && CARGO_TARGET_DIR=E:/tmp/m3bag-fd-target cargo fmt --all -- --check` | rc=0（无输出） |
| `07-gates-clippy.txt` | `cargo clippy --workspace --all-targets --offline -- -D warnings` | rc=0 |
| `08-gates-test.txt` | `cargo test --workspace --offline` | **648 passed / 0 failed / 16 ignored**，rc=0 |
| `09-gates-msgcodegen.txt` | `cargo run -p msgcodegen --offline -- --check` | 604 条一致，rc=0 |
| `10-runbots-selfcheck-green.txt` | `powershell ... -File tools/botload/run_bots.ps1 -SelfCheckNoStack` | **GREEN（rc=0）**；attack done 2/1/0 + bag done 2/0 |
| `11-runbots-mutation-red.txt` | 把收尾计数正则 `"role":"done"` 改成 `"DONE"` 后同上 | **RED（rc=1）**；`attack_done_2, attack_done_1` |
| `12-attackdiff-selftest.txt` | `python mir2-rs/tests/parity/m3/attack_diff.py selftest` | **16/16 GREEN（rc=0）**；含死亡容忍绿 + `death_k 7→8` 必红 + `hp 12→999` 必红 |

## oracle / 冻结锚点

- `sql/mir2_db.sql:47-49`：`IncHealth/IncSpell` 属 `characters`（F-A，已在 master `8f193a6c`）。
- `sql/mir2_db.sql:6081-6084`：`characters_ablity.Hp/Mp/MaxHP/MAxMP` = `smallint(11)` NOT NULL（F-D 列域锚）。
- `src/BotSrv/Player/RobotPlayer.cs:674`：攻击序列收尾标记 `M3ATTSEQ {"role":"done",...}`（F-E 锚）。
- `src/M2Server/Player/PlayObject.cs:5254-5312`：`RecalcLevelAbilitys()` 由 `Abil.Level` 重算 MaxHP/MP 并 clamp HP（F-F 锚）。

## 复现命令（本目录，无需起服务端栈）

```powershell
# 0) MySQL 3306 须在听（共享服务；本片未起未停）
# 1) 生产 GREEN
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_attack_pair.ps1 -SelfCheckNoStack
powershell -ExecutionPolicy Bypass -File tools/botload/run_bots.ps1 -SelfCheckNoStack
# 2) 改坏必红（复现后用 git checkout -- 还原）
sed -i 's/\[int\]\$AttackVictimHp = 30000,/  [int]$AttackVictimHp = 100000,/' mir2-rs/tests/parity/m3/run_attack_pair.ps1
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_attack_pair.ps1 -SelfCheckNoStack   # rc=1
git checkout -- mir2-rs/tests/parity/m3/run_attack_pair.ps1
# 3) 其它两条无栈自检
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/selftest_gate.ps1
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_bag_equip_pair.ps1 -SelfCheckNoStack
```

## 残留物清单

- 入库：本目录 + `gate_guards.ps1` / `run_attack_pair.ps1` / `run_pair.ps1` / `run_bag_equip_pair.ps1`
  / `tools/botload/run_bots.ps1` / `README.md`（m3）。
- 仓外临时物（可删）：`E:/tmp/m3bag-fd-evidence/**`（本次中间输出与 `.bak`）、
  `E:/tmp/m3bag-fd-target`（隔离 cargo target）、`E:/tmp/m3bag-fd-clone`（干净 clone 复核）、
  `E:/tmp/openmir2-m3gs/selftest-*`（`selftest_gate.ps1` 默认输出）。
- 端口/进程：本片**未起任何服务**；10 端口由 m3cs 本轮 ②③ 真机栈持有（与本片无关），仅共享 3306 被本片自检读取。
- source-mutation 已还原：`run_attack_pair.ps1` 的 `-AttackVictimHp` 缺省 = `30000`、`run_bots.ps1` 的 attack 正则 = `"role":"done"`（`git status` 干净）。
- 下一轮真机验证（m3cs）：owner `anti_repeat` 要求先跑 **`-AttackCount 20` 小规模冒烟**（确认序列完整、判据非平凡）再跑 120；
  本片已把 F-D/F-E/F-F 三处 fail-closed 就位，但**真机序列两相仍由 m3cs 在窗口内跑**（本片未起栈）。
