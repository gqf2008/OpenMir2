# m2-svr 车道真机跑（2026-10-12 01:0x）：M2 验收 ① + ②（talk 补齐）

> 本目录是**证据**（真机实拍），不含 oracle/协议代码改动。交付片 = `main.rs`/驱动**无改动**，
> 只新增本证据目录 + 对应卡上 comment。

## 0. 环境与口径

| 项 | 值 |
| --- | --- |
| master / 栈 | `078ccb34`（含 talk 回声片 `a5726e2a` = `chat.rs` + `smframe::sm_hear` + `CM_SAY` 路由 + 驱动 talk 阶段） |
| 栈 | `tools/m2-acceptance/start_rust_stack.ps1 -RustBinDir E:\Users\gxh\Documents\GitHub\OpenMir2\mir2-rs\target\debug`：Rust 四入口 5000/7000/7100/7200 + C# `DBSrv`/`LoginSrv`（下盘 oracle）+ MariaDB 3306 |
| 账密 / 角色 | `mir2test` / `mir2pass` / `aaa`（沿用现网测试账号，未新建、未改库结构） |
| 客户端 | `D:\MirClient-run` + `run-release.cmd`（冻结，未改一行） |
| 驱动 | `powershell -ExecutionPolicy Bypass -File tools/client/mir_flow.ps1 -Account mir2test -Password mir2pass -OutDir E:/tmp/m2acc/m2svr-run1 -WalkClicks 3 -AutoPlaySec 20 -TalkText "m2 talk ok"` |
| 驱动结果 | `FLOW_OK` / `CLIENT_STOPPED`（末行，原始件 `mir_flow.txt`、`stages.ndjson`） |

## 1. ① 端口所有权 —— **GREEN**

- 命令：`powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_ports.ps1`（exit 0）
- 原始件 `ports.json`：5000/7000/7100/7200 监听者**全为 Rust** 且**入口对得上**：
  - 5000 `game-svr.exe` `"<…>\mir2-rs\target\debug\game-svr.exe" E:\MirServer\M2GameSvr\Server.conf` ⇒ `verdict=ok`
  - 7000 `login-gate.exe` ⇒ `ok`；7100 `sel-gate.exe` ⇒ `ok`；7200 `game-gate.exe` ⇒ `ok`
- 用现网 `E:\MirServer` 配置（只读引用，未改部署）。

## 2. ② 真客户端 7 图 —— **5/7（本轮新补 `m2-03-talk`）**

| 手册名 | 本目录文件 | 来源原档（`E:\tmp\m2acc\m2svr-run1\`） | 说明 |
| --- | --- | --- | --- |
| `m2-01-enter-map.png` | ✅ | `05c_ingame_ready.png` | 比奇省 291,666；城墙壁垒 + 角色 + 完整 HUD ⇒ **不黑屏** |
| `m2-02-walk.png` | ✅ | `06_walk.png` | 走路后（坐标变化） |
| **`m2-03-talk.png`** | ✅ **本轮新补** | `m2-03-talk.png` | **聊天框输入并回车后，本角色文本 `aaa:m2 talk ok` 出现在聊天框**（画面可核；服务端 `CM_SAY → SM_HEAR` 回声含说话者自己，见 `chat.rs`） |
| `m2-05-attack.png` | ✅ | `05c_autoplay_10s.png` | Ctrl+Alt+X 自动挂机 10s（走位+攻击） |
| `m2-07-relogin.png` | ✅ | `09_reenter.png` | 小退 → 回选角（`m2-07b-back-charsel.png`）→ **再进不黑屏** |
| `m2-04-see-others.png` | ❌ | — | 需第二角色进屏（第二客户端 / `tools/botload` 假人）；本轮未采（见 §4） |
| `m2-06-pickup.png` | ❌ | — | 需 `main.rs` 接线 `CM_PICKUP/CM_DROPITEM → ground::*`（`ground.rs` 已在 master）且地面有物；本轮未采（见 §4） |

- `m2-03-talk.png` 画面内容：聊天框末行 `aaa:m2 talk ok`，角色头顶同文（服务端回声），
  与驱动 `-TalkText "m2 talk ok"` 一致（帧编码口径见 `chat.rs` 模块头 / 台账 §4.11）。

## 3. ③ / ④ —— **未跑**（如实登记，不是"绿"）

- ③ 同序 s2c：本轮栈的客户端**直连 7200**，未挂 `tools/capture/mir2_proxy.py` ⇒ 无 dump ⇒ `PREREQ`。
  按 owner 2026-10-12 §22 裁决：入图序判据 = `658→46`（15 条自帧），`10 SM_TURN` 由
  `see-others` 一格单判（`10` 是 AOI 出包）。挂代理需先让出 7200（栈由外部 supervisor 管理，
  本轮未抢窗口）。
- ④ 三路径 DB diff：需两侧（C# 对照 + Rust）各落 `dbsnap` 三路径（小退/掉线/服务关闭）⇒ 本轮未采。

## 4. 残留物与下一步

- 原始件保留 `E:\tmp\m2acc\m2svr-run1\`（21 张原图 + `stages.ndjson`），大图不入库，只挑手册名件。
- **栈仍在跑**（端口快照见 `ports.json`，pid：5000=15408 / 7000=10768 / 7100=8772 / 7200=4516）。
  下一片若无需真机，由 owner/栈 owner 决定何时 `start_rust_stack.ps1 -Stop`。
- 下一步（照 `brief-m2svr-next.md` §21/§22 序）：
  1. `see-others`：第二角色（`tools/botload` 假人或第二客户端）⇒ 同时是 AOI 下行 `db84f98d` 的真机判据；
  2. ③ 挂代理抓 `658→46` 全序 s2c（`export_s2c.ps1` + `s2c_diff.py --upto-ident 46`）；
  3. ④ 三路径 DB diff（两侧同 `-Prefix`）；
  4. `pickup`：`main.rs` 接线 `CM_PICKUP/CM_DROPITEM → ground::*`（见卡上 comment 的输入清单与阻塞点）。
