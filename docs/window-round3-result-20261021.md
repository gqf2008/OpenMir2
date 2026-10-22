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

## 5. 续采（2026-10-21 深夜，同 master 606b32cf5 的二进制）

### 5.1 怪物 `SM_WALK` —— **验证通过**

- 摆位：`map 0 (333,247)`（`MonGen` 的 `(333,268) 鸡 r=20` 框外、视野 12 内），Level 1 / HP 20。
- Rust 栈：登录后 `进世界 CurMap:0 (333,247)`（未发生 enter 改写）；停留 20s。
- capture `E:/tmp/window/20261021-r3/walk/s2c/capture.jsonl`：
  `SM_APPEAR(41)=7`、**`SM_WALK(11)=7`**、`SM_TURN(10)=6`。
- 结论：上一轮「Rust 怪物走位 0 条」的缺陷已修复；本场景条数 7（上一轮 C# 基线区间 17–76，
  本轮未同窗口采 C# 对照，条数差异属停留时长/视野内怪物数差异，非帧面缺陷）。
  `SM_WALK` 的 recog/param/tag 与收件人集合仍以 T-146 的构造级判据为准。

### 5.2 固定 NPC 菜单（T-137）—— **未采到，且发现待查现象**

- 摆位 `0122 (29,33)`，服务器日志确认 `进世界 CurMap:0122 (29,33)`，`Npcs.txt` 3 条、
  `Npc_def` 脚本 3 条（缺失 0）。
- 但 s2c capture 只有 **1 条 `SM_APPEAR(41)`（recog=自己）**，没有 NPC 的出现帧；点击
  `(512,227)` / `(510,263)` 都被客户端判成 `CM_WALK(3011)`/`CM_TURN(3010)`，服务端没有
  `Ident=1010` 的点击记录。
- 服务端 sim 日志：`map=27 vr=12 vis=0 new=0 same_map=1 nearest=20` ⇒ 玩家视野内 0 个可见对象。
- **判定**：固定 NPC 未进入玩家视野（客户端没有精灵可点），与 T-137 的脚本装载无关。
  需用 C# oracle 在同一摆位复采确认是本仓缺陷还是夹具问题；结论已记入卡
  `openmir2-npc-def-loading` 的 comment（父 oid `11f1d6ae…`），T-137 窗口项保持 open。

### 5.3 收尾

- 本轮所有栈已停，九游戏端口与 7600 DOWN；MySQL 3306 保持 UP。
- 证据目录：`E:/tmp/window/20261021-r3/{walk,king2,king3}/**`。

## 6. 固定 NPC 可见性：C# oracle 对照（2026-10-21 深夜，决定性证据）

- 同一摆位（`0122 (29,33)`，测试库同为克隆态），C# 影子栈（`mir2_test_*`）跑同一受控登录，
  抓包 `E:/tmp/window/20261021-r3/cking/s2c/capture.jsonl`：
  - `SM_APPEAR(41)` **6 条**：1 条自己 + **5 条 `recog=31475780, param=50, tag=8`**
    —— `param=50` 即 `RaceImg=ActorRace.Merchant(50)`、`tag=8` 即 GuildOfficial 的 `Appr=8`
    （`GuildOfficial.cs:18-19`），与 `Envir/Npcs.txt` 的 `比奇国王 1 0122 29 32` 对应。
- Rust 同摆位：`SM_APPEAR(41)` 仅 1 条（自己），服务端 `vis=0`。
- **结论**：C# oracle 会把固定 NPC 下发给玩家视野，Rust 不下发 ⇒ **本仓缺陷（固定 NPC 可见性）**，
  不是夹具问题，也与 T-137 的脚本装载无关。已开专项车道（`npc-visibility`）定位根因并修复，
  修好后重跑本场景即可收 T-137 的窗口项。
- 附带确认：oracle 固定 NPC 的外观轴 = `RaceImg=50`、`Appr=8`，与本仓 `openmir2-superman-normnpc`
  落下的 `race_img=50`、`Appr` 落位一致。

## 7. 第四轮（备用端口 15xxx/17xxx，2026-10-22）

### 7.1 准备
- 基线 `641d50782`（镜像重建六件 + provenance）；
- `alt_port_profile.py generate --deploy E:/tmp/window/r5/root --db-mode test ... --client-port 17000`：
  六角色 conf、路由文件（`AddrTable` 17100 / `ServerInfo` 17200）、`run-release-alt.cmd`；
- 为抓包把 game-gate 绑定改到 17600（`Count=1`），`ServerInfo` 仍广播 17200，代理 17200→17600；
- 客户端用**私有运行目录**（`E:/tmp/window/r5/client`，资源用 junction 指回共享目录，`run-release.cmd`
  拷自 `run-release-alt.cmd`），不再改共享 `D:/MirClient-run` 的启动脚本。

### 7.2 结果
- 整栈在备用端口全部起来（15000/16000/15100/15700/15500/15600/17000/17100/17600/17200），
  与外部 C# 栈（5000/7000/7200）**互不干扰**；登录链 `17000→15500`、`17100→15100` 均通。
- 客户端 `FLOW_OK` 并成功进世界：`进世界 ChrName:winplay0 CurMap:0122 (29,33)`。
- **固定 NPC 帧已按 oracle 下发**（抓包 `E:/tmp/window/r5/s2c/capture.jsonl`）：
  - `SM_TURN(10) recog=132 param=29 tag=32 series=518/515/515`（0x0206/0x0203）；
  - 紧随 `SM_FEATURECHANGED(41) recog=132 param=50 tag=8 series=0`（RaceImg=50、Appr=8）；
  - 3 对转身中 1 对带名尾、2 对 nameless —— 与 C# oracle 捕获同形（§6）。
- **仍未达成**：客户端画面未渲染国王精灵，点击 `(512,263)/(512,227)` 仍被判成 `CM_WALK(3011)`，
  服务端无 `Ident=1010`。协议侧已证明帧正确到达；问题在客户端对角色的注册/坐标映射
  （客户端显示 `皇宫 29,44` 与服务器 `(29,33)` 存在偏移），需要客户端侧排查或改用「走到 NPC
  像素再点」的定位法。

### 7.3 收尾
- 备用端口栈全部停止，15xxx/17xxx 全 DOWN；外部 C# 栈（5000/7000/7200）保持未动；
- 共享 `D:/MirClient-run/run-release.cmd` 已恢复原文件（sha256 `1ab506be…`）；
- 证据：`E:/tmp/window/r5/**`。

### 7.4 C# 对照实验（未完成，含一个新事实）

目的：同摆位把客户端放到 C# oracle 上，判断「客户端不渲染国王」是客户端/坐标问题还是 Rust 缺帧。

- C# 侧在备用端口起栈（GameSvr 15000 / DBSrv 16000·15100·15700 / LoginSrv 15500·15600），
  GameSvr 与 DBSrv、LoginSrv 均连接成功（日志 `数据库服务器[tcp://127.0.0.1:16000/]连接成功`、
  `登录服务器[tcp://127.0.0.1:15600/]连接成功`）。
- **C# `SelGate` 无法换端口**：`src/SelGate/GateShare.cs:15` 是 `public const int GatePort = 7100`，
  `ServerService.Start()` 直接绑这个常量，**不读** `config.conf` 的 `GatePort0` ⇒ 备用端口窗口里
  C# SelGate 必然去抢 7100（外部栈占用 ⇒ `SocketException 10048`）。
- 用 Rust login-gate/sel-gate 代替后：客户端能选服（LoginSrv 返回 `127.0.0.1:17100`），但 Rust
  sel-gate 与 C# DBSrv 的连接在角色查询后立即被对端断开（`数据库服务器断开链接`），客户端停在黑屏，
  C# GameSvr 未收到会话 ⇒ **对照未得出结论**。
- 结论与后续：
  1. 备用端口剖面对 **Rust 侧**完备；对 **C# 侧** 受 `GatePort` 常量限制（E4 允许重编，但改 `src/**`
     属冻结面，须先走白名单/owner 裁决，不在本窗口范围）。
  2. 客户端不渲染的判定仍待 C# 对照；替代路径是读 `MirClientZig` 的角色注册代码（收到
     `SM_TURN/SM_FEATURECHANGED` 后未建 actor 的原因），或在 7100/7200 空闲时用默认端口做 C# 对照。

## 8. 第五轮（r6，2026-10-22）：固定 NPC 菜单 **GREEN**

- 基线 `229d52cd9`（镜像重建六件 + provenance）；备用端口栈（15000/16000/15100/15700/
  15500/15600/17000/17100/17600 + 代理 17200）；私有客户端目录（`run-release-alt.cmd`，冻结
  Delphi 客户端 sha256 `c3cb8f73…`）。
- 夹具：新 `mir_flow` 的 `-ProxyDir` live 证据 + `-ExpectPos 29,33` 坐标门 + `-ClickNpc`
  （默认 `512,263`）；测试库重置后摆位 `0122 (29,33)`。
- 结果（证据 `E:/tmp/window/r6/**`）：
  - 服务端日志：`CM_CLICKNPC：到达 Click 点 ChrName=winplay0 npc=132（@main 说词 1 条）`，
    客户端发出 `Ident=1010`；
  - 抓包：`SM_MERCHANTSAY(643) recog=132` 1 条（GBK 正文以「比奇国王/…」开头），
    `SM_TURN(10) recog=132 param=29 tag=32 series=0x0206` 与 `SM_FEATURECHANGED(41) param=50 tag=8`；
  - 客户端截图 `flow/05d_npc_click.png`：对话框显示「我是比齐皇管理员，掌管着许多事物…」与
    菜单项（请求创建行会 / 申请行会战 / 查看行会战事 / 申请攻城战事）；
  - `manifest.json` 记录 `client_exe=…/MirClinet.exe`、sha256 `c3cb8f73…`。
- **判定**：T-137 的窗口项（点击固定 NPC 出 `@main` 菜单）**GREEN**。此前的「未渲染」确认为
  夹具（公告盲点把角色走离视窗）所致，非服务端/客户端缺陷。
- 收尾：栈全部停止，15xxx/17xxx 全 DOWN；外部 C# 栈未动。

## 9. 第六轮（r7，2026-10-22）：掷骰 NPC 对话框 **部分 GREEN**，发现两个问题

- 基线 `e71845f9e`（二进制为 `229d52cd9` 重建件）；备用端口栈；私有客户端目录。
- 夹具：`B013 (8,11)`，`-ExpectPos 8,11 -ClickNpc`。**坐标门按设计拦下**：登录后服务器把角色放到
  `(9,10)`（B013 有进图位移），gate 报 `POSITION_GATE RED actual=9,10 expect=8,11`，未盲点 —
  夹具门行为符合预期。
- 手工点击：西侧格 `(464,295)` → 服务端 `CM_CLICKNPC 到达 Click 点 npc=119（@main 说词 1 条）`；
  客户端显示赌场 @main 对话框（开始-下注 / 筹码-兑现 / 返回土城），截图 `E:/tmp/window/r7/shot-dice5.png`。
- **问题 1（~~真缺陷~~ → 2026-10-22 复核撤销：oracle 行为，非缺陷）**：对话框里 `<$GAMEGOLD>`
  **原样显示**。当时判为「未做变量替换」，复核后确认这正是 oracle 语义：`NormNpc.GetLineVariableText`
  （`NormNpc.cs:245-273`）丢掉 `ArrestStringEx` 的剩余串 ⇒ 扫描不前进，首个 `<...>` 是链接
  （本页首 token `<开-始-下-注/@kaiz>`）时后面变量一律原样。详见
  `docs/say-variable-expansion-reversal-20261022.md`；本仓 `line_variable_text` 保持该字面循环。
- **问题 2（未达成）**：点击「开始-下注」链接后客户端**没有**发 `CM_MERCHANTDLGSELECT(1011)`
  服务端无记录，掷骰流程无法进入；需继续定位（像素命中或客户端对链接文本的识别，与说词文本的
  空格/编码呈现有关）。
- 收尾：栈全部停止，15xxx/17xxx 全 DOWN；证据 `E:/tmp/window/r7/**`。
