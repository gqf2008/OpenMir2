# per-bot `MySelf` 接线牙（卡 `openmir2-botsrv-myself-wiring-tooth`）

> 背景（复审 `f9168b52` findings[0] / M3）：删掉 `ClientManager.RunBotRound` 里的
> `MShare.MySelf = robot?.MyActor` 后，`--selftest-shared-state` 仍 **6/6 全绿** ⇒
> per-bot 上下文接线没有测试覆盖。本卡只补测试与接缝断言，**不改生产接线**。

## 新增用例（`--selftest-shared-state`）

| 用例 | 构造 | 断言 |
| --- | --- | --- |
| `runbotround-context-per-bot` | 同一线程两个 `RobotPlayer`，各自 `MyActor`（CurrX=101/202）；先后 `ClientManager.RunBotRound(..., bot1)` / `bot2`，action 内捕获 `MShare.MySelf` | 两次捕获分别指向 `bot1.MyActor` / `bot2.MyActor`；调用后恢复为调用前（null） |
| `processonepacket-context-per-bot` | 同一线程 `ClientManager.AddClient` 两个 bot；先后 `ProcessOnePacket`（空包），`RobotPlayer.ProcessPacketEnteredForTest` 在 `ProcessPacket` 入口捕获 `MShare.MySelf` | 两次捕获分别指向各自 `MyActor` |

接缝（生产零行为变化）：
- `ClientManager.ProcessOnePacket` 由 `private` 改 `internal`（只为自检直接调用）；
- `RobotPlayer.ProcessPacketEnteredForTest`（`internal static Action<RobotPlayer>`，默认 null）
  在 `ProcessPacket` 入口调用；
- **生产接线 `MShare.MySelf = robot?.MyActor` / `= client.MyActor` 一行未改。**

## 变异（真跑，不是只写断言）

| 变异 | 结果 |
| --- | --- |
| **M3**：删 `RunBotRound` 的 `MShare.MySelf = robot?.MyActor;` | `runbotround-context-per-bot` **FAIL**（seen0=null / seen1=null），`--selftest-shared-state` **rc=1**；`processonepacket` 仍 PASS |
| **M3b**：删 `ProcessOnePacket` 的 `MShare.MySelf = client.MyActor;` | `processonepacket-context-per-bot` **FAIL**（seen1=null / seen2=null），**rc=1**；`runbotround` 仍 PASS |
| 恢复 | `ClientManager.cs` 恢复为变异前内容（sha256 `d05b5a724ec44b9d075014aee0cf17e97948efd8f4321a33cf14e82f7a90754c`）；`shared-state`/`scene-null`/`decode-compat`/`error-census`/`probecoords` 全 **rc=0** |

## 回归

- `BotSrv.exe --selftest-shared-state` → 8/8 PASS（原 6 + 本卡 2）；
- `--selftest-scene-null` / `--selftest-decode-compat` / `--selftest-error-census` /
  `--selftest-probecoords` → 全 rc=0；
- `git diff --check` rc=0；无生产逻辑改动（仅访问修饰符 + 默认 null 的测试钩子 + 自检用例）。
