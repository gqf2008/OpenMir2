# M2 真机证据（2026-10-12 第 2 轮）——② `see-others` 达成 + ③ s2c 全序对账

- 卡：`openmir2-m2-enter-world-frames`（顺序判据②）+ `openmir2-m2-gamesvr-host`（M2 验收①②③）
- 本片写集：`tools/client/mir_flow.ps1`（新增 `-SeeOthersWaitSec` 阶段）、`tools/m2-acceptance/**`、`docs/**`
- **未改**：`crates/**`、`src/**`（C# oracle）、`MirClient`、表结构/脚本语法/数据格式；**未 merge、未签 review**
- 栈：`start_rust_stack.ps1`（MySQL 3306 + **C# DBSrv/LoginSrv** 下盘 oracle + **Rust** `game-svr`(5000)/三网关(7000/7100/7200)）
- 二进制：`E:\tmp\openmir2-m2svr-target\debug`（branch head 未改 `crates/**` ⇒ 与 master `078ccb34` 同源）
- 客户端：`D:\MirClient-run` + `run-release.cmd`（冻结，未改一行）
- 账号/角色：`mir2test`（观察者用 `bbb`@289,618 / ③ 用 `aaa`@287,618），第二角色 `loadbot0`@287,620（`tools/botload`）

## 1. ① 端口所有权 —— **GREEN**

`check-ports.txt` / `ports.json`：5000/7000/7100/7200 监听者均为 **Rust** 且入口基名与期望相符
（`game-svr`/`login-gate`/`sel-gate`/`game-gate`）。

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_ports.ps1
```

## 2. ② `see-others` —— **达成**（本片核心产出）

- 观察者：`mir2test` 的 **第二角色 `bbb`**（slot 2，@287·618），驱动
  `tools/client/mir_flow.ps1 -Account mir2test -Password mir2pass -CharSlot 2 -SeeOthersWaitSec 20 …`（新增阶段）。
- 同屏第二角色：`src/BotSrv` 假人 **`loadbot0`**（@287·620，与观察者相距 (2,2) ⇒ 在 AOI 内），
  用 `tools/botload/run_bots.ps1 -Count 1 -Prefix loadbot`（**注意：`run_bots.ps1` 达成计数后会杀 BotSrv**，
  要第二角色与观察者同时在屏，需直接常驻跑 `BotSrv.exe`，见本片报告 §2）。
- 证据：
  - `m2-04-see-others.png`（1134913 B）：观察者（裸男）+ **第二个角色**（品红外观、头顶红色名条）同屏；
    客户端只会通过 `SM_TURN(10)` + `SM_FEATURECHANGED(41)` 渲染其它 actor ⇒ 即 **AOI 下行真机成立**。
  - `see-others.log.txt`：`loadbot0` entity=5 进世界（17:25:05）后，`bbb` entity=6 才进世界（17:25:47）；
    两人同时在世界的服务端侧证据（源 `E:\tmp\m2-acceptance\stack\game-svr.out.log`）。
  - `stages-seeothers.ndjson`：驱动阶段时间线（`see_others_wait` → `see_others`）。
- 口径：owner §22 —— `10 SM_TURN` **不是自身入图帧**（金标准 `#16` 的 `recog=0x7AC11879 ≠ ActorId`），
  由「看见别人」这一格单独判（本片即该格）；入图序判据只看 `658→46`（见 §3）。

## 3. ③ 同序列出包 diff —— **同序 + 等长 GREEN；逐字节 6 处未登记差异（RED）**

采法（照手册 §3，**不新写导出器**）：影子网关 `game-gate`(7600) + `tools/capture/mir2_proxy.py --map 7200=127.0.0.1:7600`
→ `export_s2c.ps1`（`segment_frames` + `GoldenExport` + `replay` GREEN）→ `s2c_diff.py`。

```powershell
python tools/m2-acceptance/s2c_diff.py `
  --rust tools/m2-acceptance/evidence/m2svr-run-20261012-2/s2c/capture.jsonl `
  --golden mir2-rs/tests/golden/c-line-baseline-c3-20261010.jsonl `
  --start-ident 658 --upto-ident 46 `
  --out tools/m2-acceptance/evidence/m2svr-run-20261012-2/s2c-diff.json
# 15/15 ident 同序：658,51,50,41,213,11029,42,766,54,5008,1106,52,752,811,46
# exit 1：6 处未登记差异（见下）
```

| # | 帧 | 字段 | 金标准 | Rust | 归因（本片判定） |
| --- | --- | --- | --- | --- | --- |
| 1 | `50 SM_LOGON` | `series` | `0` | `4` | 夹具：观察者记录 `Dir=4`（金标准角色 `Dir=0`）。`series = MakeWord(Dir, Light)`。改 `Dir=0` 即消。 |
| 2 | `52 SM_ABILITY` | 尾 `WAbil` | `dc=257 hp=15 max_hp=19 max_mp=15 weight=14` | `dc=0 hp=19 max_hp=0 max_mp=0 weight=0` | **待裁**：见 `s2c/field-diff-52.txt`。`max_hp/max_mp=0` 与角色库 `MaxHP=19/MAxMP=15` 不符，疑记录→能力输入链缺口；`dc/weight` 疑金标准角色带装（不可见槽）。**不自行改口径**。 |
| 3 | `46 SM_DAYCHANGING` | `param`(=Bright) | `1`（白天） | `3`（夜晚） | **墙钟**：`Bright = M2Share.GameTime`（按真实小时）；金标准 09:28 采、本片 01:28 采。C# 侧同时刻也会得 3 ⇒ 需「同刻采」或白名单。 |

- `s2c/capture.jsonl`（Rust 侧导出，15 条入图帧）、`s2c/body_layout.json`、`export.txt`（replay GREEN）、`s2c-diff.txt`、`s2c-diff.json`、`s2c/field-diff-52.txt`。
- **未把 `10` 塞进 `--upto-ident 10`**：`10` 归 §2 的 see-others 单独判（owner §22 二选一，本片取「收窄到 46」）。
- 归一化清单只有 `recog`（会话相关 ActorId），由既有 `oracle_capture.py` 的 EDCode 完成；无白名单。

## 4. ④ 三路径 DB diff —— **未跑（PREREQ）**

需 C# 对照栈（`E:\MirServer\start-all.ps1`）与 Rust 栈各自跑 小退/掉线/服务关闭 三条退出路径并各落 `dbsnap`
（`check_db_paths.ps1 -Mode snap|diff`，两侧同 `-Prefix`）。本片窗口只够 ②/③，如实登记为 PREREQ。

## 5. 复现（一条命令的顺序）

```powershell
# 0) 无栈自检（缺前置必须非零退出）
powershell -File tools/m2-acceptance/selftest.ps1                 # 33/33 GREEN
powershell -File tools/m2-acceptance/run_acceptance.ps1 -SelfCheckNoStack
# 1) 起栈（Rust 链；provision 必须在起 LoginSrv 之前——本片账号已存在，未新建）
powershell -File tools/m2-acceptance/start_rust_stack.ps1 -RustBinDir E:\tmp\openmir2-m2svr-target\debug
# 2) ② see-others：常驻跑一个假人 + 驱动客户端
tools/botload/run_bots.ps1 → 改直跑 src/BotSrv/bin/Release/BotSrv.exe（常驻，勿被 run_bots 达成即杀）
powershell -File tools/client/mir_flow.ps1 -Account mir2test -Password mir2pass -CharSlot 2 `
  -OutDir E:\tmp\m2acc\seeothers\shots2 -SeeOthersWaitSec 20 -SkipTalk -SkipReenter -LeaveClientRunning
# 3) ③：影子网关(7600) + proxy(7200→7600) → export_s2c.ps1 → s2c_diff.py --upto-ident 46
# 4) 收工停栈
powershell -File tools/m2-acceptance/start_rust_stack.ps1 -Stop
```

## 6. 残留物清单

- 本目录：截图 1 张（1.1 MB）、`s2c/capture.jsonl`、`s2c/body_layout.json`、`s2c/field-diff-52.txt`、
  `s2c-diff.{json,txt}`、`export.txt`、`check-ports.txt`、`ports.json`、`see-others.log.txt`、`stages-seeothers.ndjson`（证据，入库）。
- 仓库外（保留作原始证据，不入库）：`E:\tmp\m2acc\seeothers\**`（shots1/shots2 + 假人日志）、
  `E:\tmp\m2acc\run1\**`（proxy dump / rust 导出 / gate 日志）、`E:\tmp\m2-acceptance\stack\*.log`（栈日志，`game-svr.out.log` sha256=`46047cc7…`）。
- 测试库 `mir2_db`：为对齐金标准起点，本片把 `mir2test/aaa` 的 `CX/CY` 由 `291,666` 临时改为 `287,618`，
  **收工已还原**（见报告 §7）。未改表结构、未新增账号。
