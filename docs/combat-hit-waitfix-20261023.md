# `combat_hit` 的等待目标：攻击者收不到自己的 `SM_HIT`（`openmir2-scenario-combat-hit-waitfix`）

- 卡：`openmir2-scenario-combat-hit-waitfix`（tier=L）；分支 `card/openmir2-scenario-combat-hit-waitfix`
  ——叠在 `card/openmir2-probe-computed-pack`（`512fa080e`）之上；合并顺序
  `recog-packing-fix → probe-computed-pack → 本卡`（三张都碰 `scenarios/README.md`）。
- 起因：reviewer-3 在 `openmir2-scenario-recog-packing-fix` 复审里指出，`combat_hit` 等的 `SM_HIT(14)`
  **有 self 过滤**，攻击者等不到自己的那一帧。

## 1. 攻击者在 `CM_HIT` 之后实际会收到什么（逐条 oracle）

| 事件 | 发送点 | 攻击者自己收得到？ | 证据 |
|---|---|---|---|
| `SM_HIT(14)` | `RM_HIT` → `SM_HIT` | **否**：`if (processMsg.ActorId != ActorId)` | `PlayObject.Message.cs:1433-1437` |
| `SM_TURN(10)` | `RM_TURN` → `SM_TURN` | **否**：同一 `ActorId != ActorId` 门 | `PlayObject.Message.cs:1528-1542`（`TurnTo` → `SendRefMsg(RM_TURN)`，`BaseObject.cs:1582`） |
| `SM_MOVEFAIL` | `RM_MOVEFAIL` → `SM_MOVEFAIL` | 仅**失败**支（超速/不可攻击），成功支不发 | `PlayObject.Message.cs:850/1018/1038` 与 RM_MOVEFAIL 处理 |
| socket 字符串（`sSTATUS_GOOD`、`+PWR`、`=DIG`） | `SendSocket(string)`（EDCode 字符串，不是 SM 二进制帧） | 是 | `PlayObject.Message.cs:860/1012/1044`、`M2Share.cs:205` |

⇒ **成功路径上攻击者收不到任何 SM 帧**。`wait_for: 14` 只能永远等到超时。

## 2. 顺带纠正：`CM_HIT` 的 Recog 是**攻击者自己的坐标**

- `ClientHitXY` 只在 `if (nX == CurrX && nY == CurrY)` 时执行攻击（`PlayObject.Attack.cs:883`）；
  不同格的目标根本不进这个分支。
- 同一口径的旁证：`CM_TURN` 的探针必须发 `MakeLong(自身 X, 自身 Y)`，否则 `ClientChangeDir` 判 false
  （`src/BotSrv/Player/RobotPlayer.cs:356-360`）。
- ⇒ `combat_hit.json` 的 `recog` 改为 `hit_xy = (self_y<<16)|self_x`（`computed.pack {lo: self_x, hi: self_y}`），
  方向继续放 `tag`。此前 recog 卡写的「目标格」措辞已在 `docs/scenario-recog-packing-20261023.md` 与本卡一并订正。

## 3. 交付：登记 BLOCKED（不参与窗口批次）

```json
"blocked": {
  "reason": "成功路径上攻击者收不到任何 SM 帧 …",
  "missing_fixture": "同屏第二会话（旁观者）或同格怪物夹具，用来观测 SM_HIT/SM_STRUCK"
}
```

- `mir2_probe --check-scenario` 仍 **rc=0**（形状合法）；
- `card_window --dry-run` **rc=2**，打印 `scenario is BLOCKED: <reason>` 与 `missing_fixture`，**不建目录、不起栈**。

## 4. 牙（实做；rc 为真实退出码）

| 牙 | 改动 | 真实结果 | 还原 |
|---|---|---|---|
| ① | 删掉 `combat_hit.json` 的 `blocked` 字段 | `card_window --dry-run` **rc=0**（回到可跑），日志里 `BLOCKED` 计数 0 | `git show HEAD:<path> > <path>` 逐字节还原；`sha256 = 19e888de350f952c1ac97653fd05533e9de48b2c03e69216eaf2a83cdecacec4`（= 提交后树） |

（牙 ① 按卡面：「若走 blocked 路线 —— 删 `blocked` ⇒ `--dry-run` 回 rc=0」。）

## 5. 门禁

```
mir2_probe.py --selftest        OK（9 checks）
param_discovery.py selftest     OK（38 cases + 11 teeth）
card_window.py --selftest       GREEN 186/186
t_reservation_gate.py check     GREEN（rows=208 whitelist=208）
check-all（SM_LOGON 帧）        move_walk rc=0 computed；combat_hit rc=2 missing: hit_dir（方向仍由操作者给）
```

## 6. 影响面

- `combat_hit.json` 现在**不可跑**（blocked）⇒ 任何把它列入窗口批次的卡都不会产生报告（`card_window` rc=2 点名原因）。
  解锁需要「第二会话旁观者」或「同格怪物夹具」——分别属多会话探针 slice-2 / 怪物夹具卡。
- `scenarios/README.md` 的 recog 打包节同步本卡的坐标口径与 blocked 状态。

## 7. 残留物

- `E:/tmp/hit-blocked.log`、`E:/tmp/hit-tooth.log`（两次 dry-run 的真实输出）、`E:/tmp/hit-blocked-saved.json`（牙用）。
