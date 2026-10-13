# evidence — M3④ 掉落/经验动作序列夹具（BotSrv）DoD① 片

> 卡：`openmir2-m3-drop-exp-fixture-botsrv`（owner `openmir2-worker-11`，worktree `m2-svr`，
> branch `worktree-m3-drop-exp-fixture-botsrv`）。本片 **DoD① + DoD④**；DoD②（run_pair.ps1 两相接线）
> 与 DoD③⑤（真机 100 杀，需 owner `-Go` 窗口）留给下一片/窗口。
> 零起栈、零占端口；未 merge、未自签 review。

## 交付内容

### DoD① BotSrv「掉落/经验序列」驱动 + `--selftest-dropseq`
- `src/BotSrv/RobotOptions.cs`：`DropExpSequence` / `DropExpKills`(100) / `DropExpStepMs`(500) /
  `DropExpStartDelayMs`(3000) / `DropExpDir`(2) / `DropExpRole` / `DropExpMonster` / `DropExpOut`。
  默认关 ⇒ 与改动前逐字节一致（不新增任何发送/日志/文件）。
- `src/BotSrv/MShare.cs`：新增 `g_boDropExpSequence` 等开关 + 纯函数
  `IsDropExpAttacker`（精确相等，非 `EndsWith`）/ `ClassifyDropExpFrames` /
  `ExtractDropExpTotal` / `ExtractDropExpDelta`（`HUtil32.MakeLong(Param,Tag)` 低/高 16 位，
  原宽度不截断）/ `BuildDropExpDropsJson` / `BuildDropExpRecord`（必填
  `k/gold/drops/exp_delta/exp_total/level`）。
- `src/BotSrv/Player/RobotPlayer.cs`：`ArmDropExpSequence`（SM_LOGON 后武装唯一 attacker）+
  `DropExpSequenceTick`（每 `DropExpStepMs` 发 `CM_HIT`，`Recog=MakeLong(x,y)`、`Tag=dir`，照攻击序列
  线上形状）+ `DropExpOnServerMessage`（单一收包钩子：`SM_WINEXP(44)`=击杀 ⇒ 落一行；
  `SM_ITEMSHOW(610)`=掉落出现帧；`SM_LEVELUP(45)` 更新等级）。逐行按 LF 写 `DropExpOut`
  指定的 `drop_exp_raw.jsonl`（空路径 ⇒ 只落 `M3DROPEXPSEQ {json}` 标记）。
  **只写本假人自己的 socket + 指定输出文件**（同 T-22/T-29 纪律）。
- `src/BotSrv/Player/RobotPlayer.Operate.cs`：在既有单一收包点上挂 `DropExpOnServerMessage`
  （默认关时空操作，与其它 `*SeqSelfCheck` 同族）。
- `src/BotSrv/AppService.cs`：把 `RobotOptions` 灌进 `MShare` 开关 + 开机日志一行。
- `src/BotSrv/Program.cs`：`BotSrv.exe --selftest-dropseq`（不启服务/不连端口）。
- `src/BotSrv/DropExpSeqSelfCheck.cs`：26 条断言（attacker 精确选择 8 / 击杀归类 5 /
  字段抽取 6（含 `MakeLong` 拼法与原宽度）/ drops 组装 3 / 行组装 4）。

> 口径说明：oracle 本代码库死亡掉落（物品+金币）**恒不落地**（`AddItemToMap` 首行
> `ItemId==0 ⇒ return false`，owner 2026-10-13 09:35 裁决「方案 A」）⇒ `SM_ITEMSHOW(610)` 恒不发。
> 驱动如实记录客户端可观测量：`drops=[]`、`gold=0`，跨侧比较的实质内容 = `SM_WINEXP(44)` 的
> `exp_delta/exp_total/level`。这是 owner 已裁定的口径，不是本片降格。

### DoD④ 无栈自检 + 改坏必红
- `mir2-rs/tests/parity/m3/drop-exp/run_drop_exp_pair.ps1` `-SelfCheckNoStack` 由 13 → **16** 项：
  新增 ⑭ no-Go real-run fail-closed（rc=2）、⑮ 给 `-Go` 但 DoD② 未落地 ⇒ 仍 fail-closed rc=2
  并点名 `PLANNED`、⑯ BotSrv `--selftest-dropseq` GREEN。
- **改坏必红**（两条，见 `02`/`03`）：
  1. `IsDropExpAttacker` 退回旧 `EndsWith(角色序号末位)` ⇒ 2 项 FAIL，`SELFTEST RED (2 failed)` rc=1；
  2. `ExtractDropExpDelta` 的 `MakeLong` 低/高 16 位对调 ⇒ 2 项 FAIL，rc=1。
  两次改动后均按字节还原（`05-boundary.txt` 的 sha256 前后一致）。
- 真机 real-run 未接线 ⇒ `-Go` 仍 fail-closed（不静默起栈/占端口），文案已订正为
  「驱动已实现、DoD② 两相接线路未落地」。

## 文件

| 文件 | 内容 |
| --- | --- |
| `00-selfcheck-nostack.txt` | `run_drop_exp_pair.ps1 -SelfCheckNoStack -SelfCheckOut` 的 LF 转录（ok=16/16 rc=0） |
| `01-botsrv-selftest-dropseq.txt` | BotSrv 构建（0 error）+ `--selftest-dropseq` 全绿 |
| `02-redcheck-attacker-exact.txt` | 红检①（attacker 精确选择退回 EndsWith） |
| `03-redcheck-makelong-order.txt` | 红检②（MakeLong 低/高对调） |
| `04-gates.txt` | 四门禁（fmt/clippy/test/msgcodegen）原始汇总 |
| `05-boundary.txt` | base/branch、冻结面、净差量、未跟踪文件、还原证明 |

## 复现命令

```bash
# 无栈自检（离线、零端口；16/16）
powershell -NoProfile -ExecutionPolicy Bypass -File \
  mir2-rs/tests/parity/m3/drop-exp/run_drop_exp_pair.ps1 -SelfCheckNoStack

# BotSrv 纯函数自检（离线）
dotnet build src/BotSrv/BotSrv.csproj -c Release --nologo -v q
src/BotSrv/bin/Release/BotSrv.exe --selftest-dropseq
```

## 未做（下一片/窗口）

- **DoD②**：`tools/botload/run_bots.ps1` 的 `DropExp*` 配置透传 + `run_pair.ps1` 的
  `-DropExpSequence` 两相 pin 接线 + `run_drop_exp_pair.ps1` real-run 分支落原始件。
- **DoD③⑤**：真机 100 杀两侧同序列对拍 + 纯度门实测（占 5000/7000/7100/7200，须 owner 发 `-Go` 窗口）。
- 卡级真机 DoD 未达 ⇒ **不因本片收卡**。
