# 真机窗口第三轮结果（OpenMir2，2026-10-21 夜）

- **基线**：镜像 `/e/tmp/m2svr-window-origin` = master `27050471f`；六件二进制
  `/e/tmp/m2svr-window-target/debug`，provenance `source_head=27050471f…`（R7 已重录）。
- **环境**：MySQL 10.11.19 由 owner 裁决后重启（PID 7020，3306）；测试三库从生产克隆
  （11/13/7 表，关键表行数 1/1/1 相等）；C# 影子包（三 conf → `mir2_test_*`），
  `E:/MirServer` 未改；拓扑 = 影子 gate 7600 + 代理 7200；Rust 侧 = C# DBSrv/LoginSrv +
  Rust game-svr/三网关（混合栈，既有口径）。
- **判据脚本**：`window-20261023/judge_login.py --scenario G1`。

## 0. 结论

| 项 | C# oracle | Rust | 判定 |
| --- | --- | --- | --- |
| 15 帧入图前缀（J1） | GREEN（52 body_len=40） | GREEN（52 body_len=58，D-9） | **前缀 idents 完全一致** |
| `654`（J2） | 1 条、在 `46` 之后 | 同 | GREEN |
| **`657×2`（J3）** | `[16,17]`，序 `i46(14) < i654(15) < 657[0] < 657[1] < i42[1](18)` | **同序同位置** | **GREEN（上一轮缺失的缺陷已修复）** |
| `42×2` + `100×4`（J4） | GREEN | GREEN | GREEN |
| 逐帧 diff（J5） | — | 仅 `SM_DAYCHANGING(46)` `param 1→3`；`50/51` 的旧已知差异不再出现 | 见 §2 |
| 怪物 `SM_WALK` | 本场景 0 条 | 0 条 | **未覆盖**（G1 落点无怪，需怪物场景） |
| T-137 国王菜单 | 未采 | 点击落到地面（`CM_RUN`），未产生 `SM_MERCHANTSAY` | **未采**（定位问题，见 §3） |
| 掷骰 `SM_PLAYDICE` / 商人不可攻击 | 未采 | 未采 | **未采**（需要点击场景，未在本窗口执行） |

## 1. 证据

- C#：`E:/tmp/window/20261021-r3/csharp/{flow,proxy,s2c}`；`judge/g1-csharp-replay.json` = **RESULT: GREEN**
  （J0–J5；J5 用同一 capture 自比，`diff=0`）。
- Rust：`E:/tmp/window/20261021-r3/rust2/{flow,proxy,s2c}`（把摆位归一化后重跑）；
  `judge/g1-rust2.json`：J0–J4 GREEN，J5 RED 仅 1 项（见 §2）。
- 第一次 Rust 采集（未归一化 `Dir/Mp`）在 `rust/`：J5 另多 `50 series/raw` 差异；
  归一化（`characters.Dir=0`、`Mp/MAxMP=17`）后消失 ⇒ 该差异确属输入漂移。
- provenance：`E:/tmp/m2svr-window-target/window-provenance.json`（source_head 27050471f）。

## 2. J5 剩余唯一差异的解释

`SM_DAYCHANGING(46)` 的 `param`：C# 采集于 21:5x、Rust 于 22:0x，服务端昼夜相位跨点
（`GetMyLight`/`SendDayChanging` 路线的 hora 相位）；两侧**其余 14 帧字段逐字节相同**（52 的
body_len 为 owner 已裁决的 D-9，已 whitelist）。`judge_login` 另提示「已知差异不齐（少
50/51 的 param/raw_hex）」——这是摆位归一化后输入漂移消失的正常结果，不是新缺陷；
白名单可在下一轮把 50/51 条目标注为「历史输入漂移」以免误报。

## 3. 未采项与原因

- **怪物 `SM_WALK`**：G1 落点 `(311,666)` 视野内无怪 ⇒ 两侧都是 0 条，无法验证修复后的条数；
  需要怪物场景（靠近 MonGen 框且安全格）另采。
- **T-137 国王菜单**：摆位 `0122 (29,33)`、国王在 `(29,32)`；客户端登录后显示 `皇宫 26 25`，
  点击 `(512,227)` 被判为 `CM_RUN`（走位），服务端无 `CM_CLICKNPC` 日志 ⇒ 坐标/目标未对上。
  下一轮需先截图定位 NPC 像素再点，或从服务端读出 NPC 实际落点。
- **掷骰 / 商人攻击**：都是点击类场景，本轮未执行；runbook §2.3–§2.5 有配方。

## 4. 收尾

- 两侧栈按 stop_order 关闭，九游戏端口 DOWN，7600 DOWN；MySQL 保持 UP（owner 要求）。
- 临时物：`E:/tmp/window/20261021-r3/**`（本轮证据，保留）；测试库 `mir2_test_*` 保持克隆态。
