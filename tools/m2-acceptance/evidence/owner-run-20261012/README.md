# owner 独立真机跑（2026-10-12 00:05–00:07）：M2 验收 ① + ②（4/7 画面）

> **为什么由 owner 跑**：当时车道编排出现"实例活不过几十秒"的环境故障（见
> `E:\tmp\openmir2-watch\handover-to-pi.md` §5.115/§5.116），m2-svr 车道连续几轮都在起完栈后被杀。
> 栈其实是它起来并**留在原地**的（下面 §1 的 pid/命令行就是它的），owner 直接接力跑完 ① 与 ② 能跑的部分。
> **本目录是证据，不是交付**：不含任何代码改动（`git diff src/ sql/ MirClient` 为空）。

## 0. 环境与口径

| 项 | 值 |
| --- | --- |
| master | `95e9d81a`（含 `db84f98d` = AOI 同屏下行；`1e81591c` 的 head 已在树内） |
| 账密 / 角色 | `mir2test` / `mir2pass` / `aaa`（**沿用现网测试账号**，未新建、未改库结构） |
| 客户端 | `D:\MirClient-run`（冻结，未改一行） |
| 驱动 | `tools/client/mir_flow.ps1 -Account mir2test -Password mir2pass -OutDir E:/tmp/m2acc/owner1 -WalkClicks 3 -AutoPlaySec 20` |
| 栈 | **m2-svr 车道先前起的**：Rust 四入口（`E:\tmp\openmir2-m2svr-target\debug\*`）+ C# `DBSrv`/`LoginSrv`（下盘 oracle）+ MariaDB 3306 |

## 1. ① 端口所有权 —— **GREEN**

- 命令：`powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_ports.ps1`
- 结果（原始件 `ports.json`，exit 0）：5000/7000/7100/7200 监听者**全为 Rust** 且**入口对得上**：
  - 5000 `game-svr.exe` `"<target>\debug\game-svr.exe" E:\MirServer\M2GameSvr\Server.conf` ⇒ `verdict=ok`
  - 7000 `login-gate.exe` `…E:\MirServer\LoginGate\config.conf` ⇒ `ok`
  - 7100 `sel-gate.exe` `…E:\MirServer\SelGate\config.conf` ⇒ `ok`
  - 7200 `game-gate.exe` `…E:\MirServer\RunGate\config.conf` ⇒ `ok`
- 注：**用现网 `E:\MirServer` 的配置**（只读引用，未改部署）。

## 2. ② 真客户端 7 图 —— **4/7**（覆盖：进图 / 走路 / 攻击 / 小退再进）

| 手册名 | 本目录文件 | 来源原档（`E:\tmp\m2acc\owner1\`） | 说明 |
| --- | --- | --- | --- |
| `m2-01-enter-map.png` | ✅ | `05c_ingame_ready.png` | 比奇省 291,666；城墙壁垒 + 角色 + 完整 HUD ⇒ **不黑屏** |
| `m2-02-walk.png` | ✅ | `06_walk.png` | 走路后（坐标变化） |
| `m2-05-attack.png` | ✅ | `05c_autoplay_10s.png` | Ctrl+Alt+X 自动挂机 10s（走位+攻击） |
| `m2-07-relogin.png` | ✅ | `09_reenter.png` | 小退 → 回选角（`08_back_charsel.png`）→ **再进不黑屏**；画面里还能看到服务端 658 公告原文（`Notice.txt`）被客户端渲染 |
| `m2-03-talk.png` | ❌ | — | `mir_flow.ps1` **没有聊天动作**（手册 §2 要求"聊天框输入并回车"）⇒ 需给驱动补一个 talk 阶段 |
| `m2-04-see-others.png` | ❌ | — | 需要**第二个角色进屏**（第二客户端或 `tools/botload` 假人）⇒ 见 §4 说明（这也是 AOI 下行 `db84f98d` 的实拍判据） |
| `m2-06-pickup.png` | ❌ | — | 需要 Rust 侧**拾取 handler**（`m3bag` 第 4 格，`40376d18` 正因 StdMode 40 耐久 wrap 被 request_changes）+ 地面物品 ⇒ 登记为依赖 |

- 原始件：`stages.ndjson`（`start→…→done`，末行 `FLOW_OK`/`CLIENT_STOPPED` 见 `E:\tmp\m2acc\owner1` 的同名流程日志）；全部 23 张原图留 `E:\tmp\m2acc\owner1\`（大图不入库，只挑 4 张手册名件）。

## 3. ③ / ④ —— **未跑**（如实登记，不是"绿"）

- ③ 同序 s2c：需要把 `tools/capture/mir2_proxy.py` 挂在 7200 前**再跑一遍**客户端（本次栈的客户端直连 7200，未挂代理）；且对账是 `658→10` **全序列**，而 `10 SM_TURN` 属 **world 侧 AOI 帧**（金标准 #16 `recog ≠ ActorId`，台账 §3/§4.9 已订正）⇒ 在"入图序列里不发 `10`"的口径被 owner 正式登记/白名单之前，③ **必然有未登记差异**。这也是下一片要先补 `see-others`（AOI 实拍）的原因。
- ④ 三路径 DB diff：需要两侧（C# 对照 + Rust）各落 `dbsnap` 三路径快照（小退/掉线/服务关闭），本次未采。

## 4. 残留物与下一步

- **栈仍在跑**（owner 特意没停：m2-svr 车道下一片可直接复查 ①/② 并接着做 ③④；若它要重跑，先 `start_rust_stack.ps1 -Stop`）。
- 本目录：4 张手册名截图 + `ports.json` + `stages.ndjson`（入库）；`E:\tmp\m2acc\owner1\` 23 张原图 + `stages.ndjson`（保留，不入库）。
- 下一步（写给 m2-svr 车道，见 `brief-m2svr-next.md` §20）：① 先补 `talk` 阶段（驱动侧小改）；② `see-others` 用第二客户端或 `tools/botload` 假人；③ 把 `10` 的口径登记/白名单定死后再跑 ③；④ 采三路径 DB 快照。
