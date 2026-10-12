# 证据：清 M3②③ 攻击夹具合并时登记的两条 Minor（卡 `openmir2-m3-attack-pair-fixture`）

- 卡：`openmir2-m3-attack-pair-fixture`（issue `76cc08b8`），T-22 同族扩展 = `whitelist.md` 的 **T-29**。
- 上游：夹具骨架片 `568c6c47` 已由 reviewer-5 `76e433f0` 独立 approve，no-ff 合入 master **`dea0b2f8`**；
  该次 `merge_result` 登记了两条非阻塞 Minor（M-1/M-2），本片即清这两条。
- 性质：**夹具/驱动侧小修**；零端口、未起栈、未 merge、未自签 review。
- 写集：`mir2-rs/tests/parity/m3/{run_pair.ps1,run_attack_pair.ps1,README.md}` + `whitelist.md` +
  `src/BotSrv/**`（T-29 同族扩展，见 whitelist）。**未动** `mir2-rs/bins/game-svr/**`、`crates/world/**`、`sql/**`、
  `src/M2Server/**`、`MirClient`。

## 两条 Minor 与本片修法

| # | reviewer-5 `76e433f0` 原finding | 修法 |
|---|--------------------------------|------|
| M-1 | `run_pair.ps1` 的 `if ($AttackSequence -and -not $AttackPinPair -and -not $EmitOpsArgs) { WARN }` 整段位于 `if ($EmitOpsArgs)` 块内部且带 `-not $EmitOpsArgs` ⇒ 恒 false、永不触发（死代码） | 把 WARN **移出** `if ($EmitOpsArgs)` 块并去掉自否条件（`if ($AttackSequence -and -not $AttackPinPair)`）；真实跑与自检两条路径都可见 |
| M-2 | `ArmAttackSequence` 用 `LoginId.EndsWith(g_nAttackRole)` 选 attacker；`-Bots>10` 时序号 10/20… 也以 attacker 序号结尾 ⇒ 被误武装成多个 attacker，污染「第 k 次」对齐 | 改为 `MShare.IsAttackAttacker` 的**精确相等**（`LoginId == MShare.g_sAttackRoleLoginId`，`g_sAttackRoleLoginId = LoginAccount + AttackRole`，由 `AppService` 装配）；新增 `BotSrv.exe --selftest-attackrole` 生产函数级自检（8 用例含 `-Bots>10` 回归） |

两条均在 `run_attack_pair.ps1 -SelfCheckNoStack` 有**可判红**机判：
- ⑩ M-1：`-AttackSequence` 未配 `-AttackPinPair` ⇒ WARN 必须 ≥1 行；配了 ⇒ 必须 0 行。
- ⑪ M-2：跑 `BotSrv.exe --selftest-attackrole`（生产函数，不是源码扫描）；PASS⇒rc0，任一用例不符⇒rc1。

## 文件表

| 文件 | 内容 |
|------|------|
| `selfcheck-nostack.txt` | `run_attack_pair.ps1 -SelfCheckNoStack` 最终 GREEN（①–⑪ 全过；含 ⑦⑧ 改坏 dmg/hit 必红、⑨ `scoped_diff selftest` 14/14） |
| `bag-equip-selfcheck-regression.txt` | `run_bag_equip_pair.ps1 -SelfCheckNoStack` 回归 GREEN（`run_pair.ps1` 改动后，T-14 夹具语义未受影响） |
| `attackrole-selftest.txt` | 还原态 `BotSrv.exe --selftest-attackrole` 原始输出（8 PASS，rc=0） |
| `mutation-red-minors.txt` | **改坏必红**（生产级变异，已还原）：M-1 WARN 挪回死代码 ⇒ `SELFTEST_RED m1_warn_missing_when_no_pinpair`；M-2 改回 `EndsWith` ⇒ `ATTACKROLE SELFTEST RED (3 failed)` 且 `SELFTEST_RED m2_attackrole_selftest_rc=1` |
| `gates-fmt.txt` / `gates-clippy.txt` / `gates-test.txt` / `gates-msgcodegen.txt` | 四门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/m3bag-minors-target`；在 rebase 到当前 master `9456ed6f` 的 head 上重跑）：fmt rc=0；clippy `--workspace --all-targets --offline -D warnings` rc=0；test **78 suites / 638 passed / 0 failed / 16 ignored**、rc=0；msgcodegen `--check` 604 条 rc=0 |
| `botsrv-build.txt` | `dotnet build src/BotSrv/BotSrv.csproj -c Release`：0 错误（warning 既存） |
| `selfcheck-selfcontained.txt` | 证据包自包含性自检（照 F-1/`8abd8a0d` 口径）：逐件 `git check-ignore -v` 无输出 + `tracked == disk` + 无 `.log` |

## 复现命令

```sh
# 1) M-1 WARN 两向（无栈；无副作用）
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_pair.ps1 -EmitOpsArgs -AttackSequence
#   ⇒ stdout 首行 WARN: -AttackSequence 未配 -AttackPinPair ...
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_pair.ps1 -EmitOpsArgs -FixedNewChr -PinStartPose -AttackPinPair -AttackSequence -Bots 2
#   ⇒ 无该 WARN

# 2) M-2 生产函数级自检
dotnet build src/BotSrv/BotSrv.csproj -c Release
./src/BotSrv/bin/Release/BotSrv.exe --selftest-attackrole   # ⇒ ATTACKROLE SELFTEST GREEN，rc=0

# 3) 夹具自检（含 ⑩/⑪）
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_attack_pair.ps1 -SelfCheckNoStack   # rc=0

# 4) 证据 hash（仓库 LF/git-blob 口径；干净检出可复现）
python -I tools/evidence-hash/verify_sha256sums.py mir2-rs/tests/parity/m3/evidence-attack-pair-minors-20261012
```

## 未做（如实登记）

- **真机两侧同种子跑**仍未跑：前置 = owner 发窗口 + **T-30**（Rust `bins/game-svr` 同种子注入，属 m2-svr 写集）。
  本片**不假装**两侧数值可比，未起栈、未占端口（十端口只剩共享 3306）。
- M-2 的**功能面** `-Bots>10` 真机多 attacker 验证属窗口内的事；本片用生产函数级 selftest（8 用例）钉住选择语义，
  真机跑（`-Bots 2`）不触发该分支。

## 口径注记（F-1 同族陷阱）

本目录所有文本证据一律存为**纯 LF**（无 CRLF、无孤立 CR）。原因：`verify_sha256sums.py` 的仓库字节
= `CRLF→LF` 归一后的 hash；若文件里混入**孤立 CR**，git 检出（`autocrlf=true`）再做上述归一就**不幂等**
（本片初版 `selfcheck-nostack.txt` 含 2 个孤立 CR ⇒ 干净 clone 上 `verify` 实测 10/11，正是 F-1/`8abd8a0d`
同族）。已逐件归一为 LF 后重算 `SHA256SUMS.txt`，干净 `git clone --local` 上 `verify` / `--check-extra`
均 GREEN（命令见上）。
