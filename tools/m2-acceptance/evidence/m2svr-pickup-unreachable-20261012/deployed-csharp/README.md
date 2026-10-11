# 部署 C# 参照栈上的「地面物是否会出现」实拍（2026-10-12）

owner 裁决 `073f0dac` 的 `evidence_required` 要求：除 oracle 源码行号依据外，
还要给「**部署 C# 栈上的实拍/抓包结论**（是否出现过地面物）+ 可复现命令」。
本目录是那次实验的原始产物。

## 实验（部署 C# 栈 + BotSrv 固定背包/装备序列）

栈：`E:\MirServer\start-all.ps1`（C# 全套：`M2GameSvr\GameSrv.exe`(5000) + `src\DBSrv\bin\Release`(6000/5100/5700)
+ `src\LoginSrv\bin\Release`(5500/5600) + `E:\MirServer\{LoginGate,SelGate,RunGate}`(7000/7100/7200)）+ 共享 MySQL 3306。

```powershell
# 1) 装配置：打开固定背包/装备序列（拾取→装备→卸下→丢弃→用药），只写 AppSetting.json
powershell -File tools/botload/run_bots.ps1 -Count 1 -Prefix loadbot -BagEquipSequence `
  -BagEquipStartDelayMs 3000 -BagEquipStepMs 800 -WriteConfigOnly -SkipBuild
# 2) 直跑 BotSrv（别用 run_bots.ps1 本体：它判据达成即杀，会截断序列）
$env:MIR2_BOT_OUT = '<本目录>'
Start-Process src\BotSrv\bin\Release\BotSrv.exe -WorkingDirectory src\BotSrv\bin\Release -PassThru
# 3) 等 ~30s 后 Stop-Process
```

（注：第一次因 `src\BotSrv\bin\Release\BotSrv.exe` 是 **17:32 的陈旧件**、不含 T-22 序列，无任何 `M3BAGSEQ`；
`dotnet build -c Release` 重编后才有下面的输出。）

## 观测结论

`m3bagseq.txt`（bot 原始件，见 `botsrv.out.log`）：

```
固定背包/装备序列: 启用 起始等待=3000ms 步间隔=800ms（拾取→装备→卸下→丢弃→用药；…）
机器人:[loadbot0] 成功进入游戏
机器人:[loadbot0] M3BAGSEQ arm start_delay_ms=3000 step_ms=800
机器人:[loadbot0] M3BAGSEQ step=0 op=CM_PICKUP x=281 y=617
机器人:[loadbot0] M3BAGSEQ step=1 op=CM_TAKEONITEM skip=no_equippable_bag_item
机器人:[loadbot0] M3BAGSEQ step=2 op=CM_TAKEOFFITEM skip=no_equipped_item
机器人:[loadbot0] M3BAGSEQ step=3 op=CM_DROPITEM skip=no_bag_item
机器人:[loadbot0] M3BAGSEQ step=4 op=CM_EAT skip=no_drug_in_bag
机器人:[loadbot0] M3BAGSEQ done
```

- **部署 C# 栈上，bot 真的发了一条 `CM_PICKUP`**（步 0，落点 281,617）：服务端处理后**没有**任何
  地面物产生、**没有** `SM_ADDITEM`(200) / `RM_ITEMSHOW`，bot 会话照常继续 —— 与「无地面物」一致。
- **未在部署栈上实跑出 `CM_DROPITEM` 的 `SM_DROPITEM_FAIL`**：本次 bot 的 `MShare.ItemArr` 背包视图为空
  （步 1/3/4 全 skip），故丢弃/用药未发出。**如实登记该限制**：丢弃恒失败在**真身 oracle 代码**上由
  `tests/parity/m3/ground-probe`（`ItemId` 由夹具显式给值，用例 `drop_fail_item_id_zero`）钉住（T-19），
  本目录不重复声称"部署栈上实测了丢弃"。
- `load_stats.ndjson`：`login_ok=1`、`spawned=1`、无 `conn_lost`/`internal_errors`。

## 与源码依据的关系

源码依据（`../oracle-*.txt` + `../pickup-unreachable.txt`）是**结论性**的：`MapItem.ItemId` 全仓无赋值点 ⇒
恒 0 ⇒ `AddItemToMap` 恒 false ⇒ 地面无物。本目录是部署栈上的**佐证**（`CM_PICKUP` 命中空地面）。
**残余风险**：部署二进制 `E:\MirServer\M2GameSvr\GameSrv.exe` 与 `src/` 的版本一致性未做二进制级校验
（本仓规范以 `src/**` 为 oracle 参照实现）——若 reviewer 认为需要，可另开一条"二进制 provenance"检查。

## 复现命令（可整段跑）

见上「实验」；配置件落在 `src/BotSrv/bin/Release/AppSetting.json`（`BotPlay.BagEquipSequence=true`）。

## 文件

- `m3bagseq.txt`：bot `M3BAGSEQ` 序列原始行。
- `botsrv.out.txt` / `botsrv.err.txt`：BotSrv 原始 stdout/stderr（原名 `.log` 被 `.gitignore` 吞，入库用 `.txt`；
  工作树原作 `botsrv.out.log`）。
- `load_stats.ndjson` / `login_conns.ndjson`：BotSrv 内部计数（登录/进世界/错误）。
- `../console/csharp-gamesvr-exit.txt`：同栈 `E:\MirServer\logs\GameSvr.out.log` 摘录（该栈不逐条记拾取）。
