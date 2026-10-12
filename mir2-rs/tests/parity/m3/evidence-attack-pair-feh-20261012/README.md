# M3②③ 攻击夹具 · F-E + F-H 证据包（2026-10-12，m3bag / worker-4）

卡：`openmir2-m3-attack-pair-fixture`（issue `76cc08b8`）。
来源：owner 2026-10-12 13:45 brief 里 m3cs 第三轮真机审计 **F-H**（`run_bots.ps1` 等 `done >= $Count`，
T-29 只武装 attacker ⇒ `done≡1` ⇒ 确定性 RED + 600s 空等）与 **F-J 本体**
（`Invoke-AttackAlign` 把 python stdout 混进返回值 ⇒ `$rc -ne 0` 恒真 ⇒ C# 侧恒 RED）。
两条的修复在本地提交 `263a0175`。

> F-J 的**自检缺口**（`-SelfCheckNoStack ⑧` 吞返回值）与 F-I（无怪 pin）由同一片的后续改动补齐，
> 证据见 `../evidence-attack-pair-fij-20261012/**`。本目录只留 F-E/F-H 的历史原始件与驱动侧红检。

## 1. F-H（`tools/botload/run_bots.ps1` F-E 收尾口径）

`-AttackSequence` 的收尾条件旧写成 `attackDone >= $Count`，而攻击序列**只由 attacker 一个假人武装**
（`RobotOptions.AttackRole` 缺省 0 ⇒ exact `LoginId = LoginAccount+0`）⇒ `-Bots 2` 永远等不到第 2 条
`M3ATTSEQ role=done`，把**已经跑完整**的序列误判成截断并 600s 空等。
**修**：唯一口径 `Get-M3SeqExpectedDone`（attack=1 / bag=$Bots）+ `Test-M3SeqComplete`，
等待/判据/汇总统一用它；`-SelfCheckNoStack` 增「预期数 + 完备性两向」。

- `01-pre-fix-smoke-raw.txt`：修前真机冒烟逐字节副本（rc=1；`attack_seq_done=1/2` 卡到超时 +
  `RED: attack_diff align 失败`，即 F-E + F-J 本体的合成症状）。
- `03-selfcheck-runbots-green.txt`：修后 `run_bots.ps1 -SelfCheckNoStack` rc=0。
- `04-mutation-FE-done-count-red.txt`：把 attack 预期改回 `$Bots` ⇒ `attack_expected_not_1,
  attack_expected_not_1_multibot, attack_1of2_not_complete` RED。

## 2. F-J 本体（`run_attack_pair.ps1::Invoke-AttackAlign`）

旧函数体把 `& python … align …` 的 stdout 一并当返回值 ⇒ `$rc = Invoke-AttackAlign` 得
`@(<输出>, $LASTEXITCODE)`、`-ne 0` 恒真。**修**：python 输出走 `Write-Host`，返回值只留
`[int]$LASTEXITCODE`。

- `02-pre-fix-align-rc-leak-raw.txt`：修前原始行（`rc=ALIGN_OK …` 说明 python 已成功，PowerShell 仍报 RED）。
- 修后自检 GREEN / 改坏必红见 `../evidence-attack-pair-fij-20261012/07-selfcheck-fij-green.txt` 与
  `.../08-mutation-fj-align-rc-red.txt`（⑮ + ⑧ 两处并发报红）。

## 复现命令（本目录，无需起服务端栈）

```powershell
powershell -ExecutionPolicy Bypass -File tools/botload/run_bots.ps1 -SelfCheckNoStack
# 改坏必红：把 Get-M3SeqExpectedDone 的 attack 分支改回 $Bots ⇒ rc=1（用 git checkout -- 还原）
```

真机冒烟需要 C# hook 栈 + 窗口（见 `../evidence-attack-pair-fij-20261012/README.md`）。
