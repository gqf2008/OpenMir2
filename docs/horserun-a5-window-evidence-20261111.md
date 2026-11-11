# 骑乘跑 A5 真机证据存档（2026-11-11）

- 卡片：`openmir2-horserun-sm-ident-5`（A5 真机窗口；本文件只做原始行归档，不改变卡状态）。
- 代码基线：`daf72ac87`（当前测试栈 `game-svr` 对应此提交）。
- 目的：把 F1（3009 被挡 + `SM_MOVEFAIL`）与 F3（旁观者收到 `ident=5`）的原始行落到仓库内，避免只存在于 `/tmp` 或会话任务输出中。
- **限制（必读）**：
  - “外观不变”是**推断**：依据是 `SM_MOVEFAIL` 帧尾带 8B CharDesc（编码长 27 / 解码长 20）且客户端未报告外观重置；**没有 F1 的独立像素截图**。不得把本条当作已由像素验证的事实。
  - F1 的原始客户端日志随后被 15:20 那轮运行覆盖；本文件中的 F1 客户端行来自当时的任务输出
    `E:/UserTemp/claude/.../tasks/be0u45or0.output`（由 `mirclientzig-da` 采集）与同轮回报。原始日志原件已不存。
  - 服务端日志仍在 `E:/tmp/m2-acceptance/stack/game-svr.out.log`；时间戳为 UTC，客户端日志为 +0800。

## 1. F1：3009 被挡，服务端拒绝并回 `SM_MOVEFAIL(28)`

### 1.1 客户端原始行（+0800）

```text
[2026-11-11 15:14:00.203] [走位] 发送走位帧 CM_HORSERUN dir=7 目标=(262,328)
[2026-11-11 15:14:14.985] [输入] 右键点击格=(262,328) 命中演员=-1 演员总数=1
[2026-11-11 15:14:14.985] [移动诊断] 走位发射 action=horserun dst=(262,328) dir=7 鼠标逻辑=(368,208) mouseCell=(261,327) cam=(rx=265,ry=331,shiftX=-24,shiftY=-16) 自身=(262,328) onHorse=true actionId=5 walking=true holdTarget=(262,328) holdDir=7 focus=null
[2026-11-11 15:14:14.986] [走位] 发送走位帧 CM_HORSERUN dir=7 目标=(262,328)
[2026-11-11 15:14:15.224] [net] play ← ident=28 kind=header recog=36772 param=265 tag=331 series=7 编码长=27 解码长=20
```

> 注：`[移动诊断] 自身=(262,328)` 是客户端当时的本地预测/显示值；服务端权威坐标见下方 `from=(265,331)` 与 `after=Some((265, 331))`。收到 `ident=28` 后客户端回到服务端坐标，下一动作从 `(265,331)` 起算。

### 1.2 服务端原始行（UTC；`game-svr.out.log`）

```text
44616:07:14:00.492 [DBG] sim: index=1 命令 Recog=21496070 Ident=3009 Param=0 Tag=7 Series=0 tail=""
44616:07:14:00.492 [DBG] MIR2_MOVE ident=3009 dir=7 steps=3 req=(262,328) from=(265,331) moved=false rolled_back=false after=Some((265, 331))
44617:07:14:00.492 [DBG] 移动：actor_id=36765 方向=7 3 格被挡/越界 ⇒ 不动（已回 SM_MOVEFAIL(28)）ChrName=aaa
44661:07:14:15.214 [DBG] MIR2_MOVE ident=3009 dir=7 steps=3 req=(262,328) from=(265,331) moved=false rolled_back=false after=Some((265, 331))
44662:07:14:15.215 [DBG] 移动：actor_id=36772 方向=7 3 格被挡/越界 ⇒ 不动（已回 SM_MOVEFAIL(28)）ChrName=aaa
```

## 2. F3：旁观者收到 `ident=5` 骑乘跑帧

### 2.1 服务端原始行（UTC）

```text
45059:07:20:31.697 [DBG] MIR2_MOVE ident=3009 dir=2 steps=3 req=(268,331) from=(265,331) moved=true rolled_back=false after=Some((268, 331))
```

同一轮登录/同屏上下文：

```text
45000:07:20:02.710 [INF] 进世界 ChrName:winplay0 CurMap:0 (264,342) Dir:3 entity=36800
45047:07:20:30.114 [INF] 进世界 ChrName:aaa CurMap:0 (265,331) Dir:7 entity=36801
```

### 2.2 旁观者客户端原始行（+0800；`E:/UserTemp/obs-run/!debug.txt:349`）

```text
[2026-11-11 15:20:31.710] [net] play ← ident=5 kind=header recog=36801 param=268 tag=331 series=258 编码长=27 解码长=20
```

`series=258=0x0102` ⇒ 低字节 dir=2、高字节 light=1；`param=268 tag=331` 与服务端 `after=(268,331)` 一致；帧长 27/20 ⇒ 12B 头 + Encode(8B CharDesc)。

## 3. 骑乘攻击掉血原始行（同批 A5 判据）

### 3.1 服务端（UTC；`game-svr.out.log`）

```text
37209:04:56:40.941 [INF] MIR2_ATTACK_RECV ident=3015 actor=36368 on_horse=true horse_type=3 target=Some(2032)
37211:04:56:41.943 [INF] MIR2_ATTACK_RECV ident=3015 actor=36368 on_horse=true horse_type=3 target=Some(2032)
37228:04:56:50.435 [INF] MIR2_ATTACK_RECV ident=3015 actor=36368 on_horse=true horse_type=3 target=Some(1287)
```

### 3.2 客户端（+0800；`MirClientZig/tools/e2e/artifacts/combat-20261111-125608-348/!debug.txt`）

```text
[2026-11-11 12:56:39.833] [攻击] 发送攻击帧 CM_HEAVYHIT 自身=(272,349) 目标=(273,350) 合并点击=1
[2026-11-11 12:56:40.165] [伤害数字] 绘制统计（帧 600；drawn=0 fallback=0；0/0=还没画过任何数字）
[2026-11-11 12:56:42.251] [net] play ← ident=31 kind=header recog=2032 param=0 tag=0 series=32 编码长=38 解码长=28
[2026-11-11 12:56:42.290] [net] play ← ident=34 kind=header recog=2032 param=273 tag=350 series=0 编码长=27 解码长=20
[2026-11-11 12:56:43.489] [伤害数字] 绘制统计（帧 800；drawn=66 fallback=66；0/0=还没画过任何数字）
```

`ident=31=SM_STRUCK`、`ident=34=SM_NOWDEATH`；目标 2032 受击后倒地，伤害数字统计 `0→66`。

## 4. 代码侧语义（防止把 `rolled_back=false` 误读成“没有回滚”）

- `try_step_now` 在 `world.rs::try_step_now` 中先逐格做 `cell_free` 预检；任一格被挡即返回 `false`，此时坐标尚未修改。
- `step_with_route` 在该支返回 `moved=false, rolled_back=false`；`rolled_back` 专指 `EnterAnotherMap` 已进落地段、`AddToMap` 失败后的真实回滚。
- 因此 F1 是“服务端拒绝、位置从未应用”，不是“服务端回滚了但标志未置位”。客户端收到的 `SM_MOVEFAIL(28)` 让它回滚自己的预测位置。
- `SM_MOVEFAIL` 的 `param=x`、`tag=y`、`series=dir` 来自 12B 命令头；CharDesc 只是尾 8B 的 `feature/status`，不含坐标/方向。
