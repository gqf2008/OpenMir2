# M2 验收运行手册（harness）

> 卡：`openmir2-m2-gamesvr-host`（DoD/S4 验收①②③）+ `openmir2-m2-enter-world-frames`（顺序判据②）。
> 本目录把「M2 最终验收」从一次人工探险变成**一条命令**：`run_acceptance.ps1`。
> 目标判据（设计文档 §6 M2 交付 + 卡内 `machine_checkable_acceptance`）：
>
> ① 7000/7100/7200（+ GameSvr 侧 5000）在验收时刻的监听者都是 **Rust 二进制**（`Get-NetTCPConnection` 实拍）；
> ② 真客户端可玩证据是 **7 张截图**（不许用假人日志替代）；
> ③ 同序列出包 **diff = 0**（或逐条白名单）+ 小退/掉线/服务关闭三条退出路径的 **DB 字段级 diff = 0**。

## 0. 一条命令

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/run_acceptance.ps1 `
  -ProxyDir   E:\tmp\m2acc\run1\proxy `
  -ShotsDir   E:\tmp\m2acc\run1\shots `
  -CsharpDbDir E:\tmp\m2acc\csharp `
  -RustDbDir   E:\tmp\m2acc\rust
```

`run_acceptance.ps1` **不启动任何服务**（不占端口、不起客户端）——只对「已跑着的栈 / 已采好的产物」取判据。
每一步的退出码口径统一：

| 退出码 | 含义 | 整体判定 |
| --- | --- | --- |
| `0` | GREEN，判据成立 | 通过 |
| `1` | RED，判据不成立（有差异/有端口被 C# 占/截图不全） | **不通过** |
| `2` | `PREREQ_MISSING`，前置未满足（栈没起 / 产物没采） | **不成立（非 0 退出，绝不静默通过）** |

**无栈自检**（本片交付时自己跑过的就是这条）：

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/run_acceptance.ps1 -SelfCheckNoStack
# 预期：ports(①)=PREREQ、s2c-export(③a)=PREREQ、shots(②)=PREREQ、db-paths(③c)=PREREQ，整体 exit 2
```

证据落在 `E:\tmp\m2-acceptance\run-<ts>\`：`ports.json` / `s2c/capture.jsonl` / `s2c-diff.json` /
`shots.json` / `db-paths.json` / `acceptance.json`。

---

## 1. ① 端口所有权实拍 —— `check_ports.ps1`

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_ports.ps1
```

判据：对 `5000/7000/7100/7200` 逐个 `Get-NetTCPConnection -State Listen` 拿 `OwningProcess`，
再用 `Get-CimInstance Win32_Process` 取 **PID + 命令行**，判定实现归属：

- GREEN：四端口都在听，且命令行/可执行体判定为 Rust（`*\mir2-rs\target\*` 或已知 Rust 二进制名）；
- RED：在听但判为 `csharp`（`E:\MirServer\*`）或 `unknown` ⇒ 验收①不成立；
- PREREQ：有端口不在听 ⇒ 退出码 2。

> 复用既有口径：`mir2-rs/tests/parity/m3/run_m3_gate.ps1` 的「实现识别护栏」同源；本脚本把 GameSvr 侧
> `5000` 也纳入，并要求四个端口**全为** Rust（混着 C# 即红）。

**失败怎么判红**：任意端口 `impl != rust` ⇒ exit 1；端口缺失 ⇒ exit 2。两者都不是 GREEN。

---

## 2. ② 真客户端 7 张截图 —— `check_screenshots.ps1`

入口：`D:\MirClient-run` + `run-release.cmd`（1024×768，参数 `127.0.0.1:7000`）。
驱动复用既有 `tools/client/mir_flow.ps1`（Win32 消息点击/打字/截图，阶段写 `stages.ndjson`）。

7 张画面 / 命名 / 截图时机（**画面内容由验收人逐张核对**；本工具只机判"交齐 + 非空"）：

| # | 文件 | 画面 | 截图时机 |
| --- | --- | --- | --- |
| 1 | `m2-01-enter-map.png` | 进图后 | 客户端进游戏、`SM_NEWMAP` 渲染完成（人物站在地图里，非黑屏）后 2–3s |
| 2 | `m2-02-walk.png` | 走路中 | 按住方向键走出数格、坐标已变化时 |
| 3 | `m2-03-talk.png` | 说话 | 聊天框输入并回车，本角色文本出现在聊天框后 |
| 4 | `m2-04-see-others.png` | 看见别人 | 另一角色进入屏内时 |
| 5 | `m2-05-attack.png` | 打怪 | 发起攻击、怪物受击（血条/伤害可见）时 |
| 6 | `m2-06-pickup.png` | 捡物 | 点击地面物品、物品消失或背包出现该物品后 |
| 7 | `m2-07-relogin.png` | 小退再进不黑屏 | 小退（CM_SOFTCLOSE）回选角 → 重进游戏、画面正常渲染后 |

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_screenshots.ps1 -ShotsDir E:\tmp\m2acc\run1\shots
```

**失败怎么判红**：`0` 张 ⇒ `PREREQ_MISSING`(exit 2)；缺 1–6 张或存在 <1KiB 的图 ⇒ exit 1；7 张齐全非空 ⇒ exit 0。

> 目录占位见 `tools/m2-acceptance/shots/`（大图归档到 `E:\tmp`，不入库）。

---

## 3. ③ 同序列出包 diff —— `export_s2c.ps1` + `s2c_diff.py`

**复用既有流水线，不新写一套导出器/解码器**：

```
客户端 ↔ 7200 经 tools/capture/mir2_proxy.py 抓包        （dump 目录：chunks.ndjson + conn-*.bin）
  → python tools/capture/segment_frames.py               （→ frames.ndjson + manifest.json）
  → python tools/capture/dump_body_layout.py             （→ body_layout.json，当前分段口径；防 A-9 假红）
  → dotnet run tools/capture/GoldenExport --layout … --verify-roundtrip  （→ capture.jsonl，schema 与金标准同口径）
  → cargo run -p replay -- capture.jsonl --no-coverage    （既有：逐字节往返 + 字段对拍，自洽门禁）
  → python tools/m2-acceptance/s2c_diff.py                （与金标准按入图序配对 + 归一化 + 差异报告）
```

```powershell
# ③a 导出 + replay 自洽
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/export_s2c.ps1 `
  -ProxyDir E:\tmp\m2acc\run1\proxy -OutDir E:\tmp\m2acc\run1\rust
# ③b 与金标准对账（同序 + 逐字节，diff 必须 0 或全部进白名单）
python tools/m2-acceptance/s2c_diff.py `
  --rust E:\tmp\m2acc\run1\rust\capture.jsonl `
  --golden mir2-rs/tests/golden/c-line-baseline-c3-20261010.jsonl `
  --start-ident 658 --upto-ident 10 --out E:\tmp\m2acc\run1\s2c-diff.json
```

> **金标准用哪份**：默认是 A 线登记、`golden_fresh_check.py` 保活的当前口径金标准
> `mir2-rs/tests/golden/c-line-baseline-c3-20261010.jsonl`（sha256=f34a6d11…；与当前
> `segment_frames.py` + `GoldenExport --layout` 重导件逐字节相同）。卡上提的
> `tests/golden/session-20261010-c3-hook/capture.jsonl` 是 **A-9 旧口径**（多段帧 811 被当 single，
> `body_len/body_sha256` 与分段解码不一致），拿它做金标准会在 811 出一对**假红**；
> A-9 已登记在 `mir2-rs/tests/parity/whitelist.md`，真要拿旧件对就先白名单该帧。

对拍口径（`s2c_diff.py`）：

- **同序**：`--start-ident 658` 起、到 `--upto-ident 10`（`SM_TURN`）止，两侧 ident 序列逐项相等；
- **等长**：逐帧 `body_len` 相等（同序天然等长）；
- **逐字节**：`raw_hex` 在**只归一化 `recog`**（=oracle `ActorId`，会话相关随机正数）后逐字节相等；
  归一化由既有 `mir2-rs/tests/parity/loginsrv/oracle_capture.py` 的 EDCode 完成（importlib 复用）；
  另比 `ident/param/tag/series/body_sha256`（把**尾**钉死，与 `recog` 无关）。
- **归一化清单**写进报告 `normalization`：本卡只允许 `recog` 一项。要加字段必须进 `--whitelist`，
  且每条必须带 `reason` + `reviewer`（§6.0-3：差异要么 0，要么逐条登记"差异点/为什么无害/谁审的"）。

**失败怎么判红**：缺 dump / 缺 dotnet ⇒ exit 2；切帧/导出/replay 失败 ⇒ exit 1；
对账有**未登记**差异 ⇒ exit 1；0 差异或全部落在白名单 ⇒ exit 0（金标准文件 `sha256=3052C778…`）。

---

## 4. ③ 三路径 DB 字段级 diff —— `check_db_paths.ps1`

复用既有 DB 工具：`tools/dbsnap/dbsnap.py`（13 表快照 + sha256 manifest）与
`mir2-rs/tests/parity/m3/scoped_diff.py`（M3 对拍判据本体：作用域内字段级 diff）。

三条退出路径：`softclose`（小退）/ `disconnect`（掉线）/ `shutdown`（服务关闭）。

一次「侧运行」（C# 与 Rust 各一遍，**同一前缀**）的采法（`-Mode snap`，在每步动作前后各跑一次）：

```powershell
# 动作前
powershell -File tools/m2-acceptance/check_db_paths.ps1 -Mode snap -Path softclose -Phase before `
  -Prefix m2acc1 -SnapDir E:\tmp\m2acc\csharp\snap-softclose-before -OutDir E:\tmp\m2acc\csharp
#   ← 验收人执行动作：真客户端点「小退」
# 动作后
powershell -File tools/m2-acceptance/check_db_paths.ps1 -Mode snap -Path softclose -Phase after `
  -Prefix m2acc1 -SnapDir E:\tmp\m2acc\csharp\snap-softclose-after -OutDir E:\tmp\m2acc\csharp
# disconnect / shutdown 同理（掉线=杀客户端进程；服务关闭=停栈）
```

`snap` 模式内部就是既有工具的两条命令（不新写）：
`dbsnap.py snap --out <SnapDir>` → `scoped_diff.py extract <SnapDir> <Prefix> <OutDir>\<path>-<phase>.jsonl --normalize`。
Rust 侧用**同一个 `-Prefix`**、`-OutDir E:\tmp\m2acc\rust` 再采一遍。

对拍：

```powershell
powershell -File tools/m2-acceptance/check_db_paths.ps1 -Mode diff `
  -CsharpDir E:\tmp\m2acc\csharp -RustDir E:\tmp\m2acc\rust
# 内部对三条路径各跑：python mir2-rs/tests/parity/m3/scoped_diff.py diff <csharp>-after.jsonl <rust>-after.jsonl
```

**期望**：三条路径的 `scoped_diff.py diff` 全部 **exit 0（字段级 diff = 0）**；任何差异要么修到 0，
要么按 §6.0-3 登记 `mir2-rs/tests/parity/whitelist.md`。

**失败怎么判红**：MySQL 3306 不可达 / 缺快照抽取件 ⇒ exit 2；任一路径 diff != 0 ⇒ exit 1。

---

## 5. 顺序判据②（本片状态）

卡上原文：**入图序到 `SM_TURN` 同序且长度相等**。已在编码层落地并入库（`55358fc2`）：
`mir2-rs/bins/game-svr/src/smframe.rs` 的 `EnterWorldInputs::frames()` 按金标准入图序
`658,51,50,41,213,11029,42,766,54,5008,1106,52,752,811,46,10` 产出 16 条；
用例 `enter_world_sequence_matches_golden_order_and_bytes` 用**金标准真字节回填输入**后断言
「ident 序相等 + 16 条 `client_frame` 逐字节相等」（长度自然相等）。
`s2c_diff.py --upto-ident 10` 就是这条判据在**整栈验收**时的同一口径。

**生产路径**目前只下发输入齐备的 `SM_NEWMAP`（`main.rs` 进世界后经 `gate_idx` 路由）；
其余 15 条由 `EnterWorldInputs` 的 `Option` 字段留空 ⇒ 不产出（**登记，不猜字节**）。

### 5.1 其余入图帧「缺哪个输入 / 由哪张卡提供 / 还差哪一步」

| ident | 帧 | 缺的运行时输入 | 由哪张卡提供 | 还差哪一步 |
| --- | --- | --- | --- | --- |
| 658 | `SM_SENDNOTICE` | 公告文本（`NoticeMgr.GetNoticeMsg`） | 配置层（未开卡） | 读 Envir/Notice 文本并接入公告管理器 |
| 51 | `SM_NEWMAP` | ✅ 已有（记录 `CurMap`+坐标）；`DayBright()` 暂占位 0 | 记录字段（已用）/ `openmir2-m2-map-loading`（DayBright） | 真地图加载后取 `DayBright()` 真值 |
| 50 | `SM_LOGON` | `MessageBodyWL=(GetFeatureToLong,CharStatus,Tag1,Tag2)` | `openmir2-m2-record-fields` | 解析记录其余字段并构造 `MessageBodyWL` |
| 41 | `SM_FEATURECHANGED` | `GetFeatureToLong()` | `openmir2-m2-record-fields` | 同上 |
| 213 | `SM_ATTACKMODE` | `AttatckMode`（会话/玩家状态） | 本卡（S2b-3 骨架）+ world | 进世界时把攻击模式从会话/记录带进来 |
| 11029 | `SM_SERVERCONFIG` | `ClientConf` 序列化原文 | 配置层（未开卡） | `SystemShare.Config` 落地 + `EncodeMessage` |
| 42 | `SM_USERNAME` | `(targetId,nameColor,showName)` | `openmir2-m2-record-fields` + world | 取角色展示名/颜色/目标 id |
| 766 | `SM_AREASTATE` | 地图 `Flag` 位（`FightZone/SafeArea/InGuildWarArea`） | `openmir2-m2-map-loading` | 真地图 Flag 解析 |
| 54 | `SM_MAPDESCRIPTION` | `Envir.MapDesc` + `nMusicid=Flag.Music?MusicId:-1` | `openmir2-m2-map-loading` | 真地图描述/音乐 id |
| 5008 | `SM_GAMEGOLDNAME` | `GameGold/GamePoint` + 元宝名/金币名 | `openmir2-m2-record-fields` + 配置层 | 记录字段 + 金币名配置 |
| 1106 | `SM_VERSION_FAIL` | `ClientFile1_CRC/ClientFile2_CRC` | 配置层（未开卡） | `Config.ClientFile*_CRC` 落地 |
| 52 | `SM_ABILITY` | `Gold/Job/GameGold/WAbil` | `openmir2-m2-record-fields` | 记录字段→`WAbil` |
| 752 | `SM_SUBABILITY` | `AntiMagic/HitPoint/SpeedPoint/AntiPoison/PoisonRecover/HealthRecover/SpellRecover` | `openmir2-m2-record-fields` | 记录字段→能力项 |
| 811 | `SM_ADJUST_BONUS` | `BonusPoint` + A/B/C 能力模板 | `openmir2-m2-record-fields` + 配置层 | 记录字段 + `BonusAbilofXxx/NakedAbilofXxx` |
| 46 | `SM_DAYCHANGING` | `Bright/DayBright` | `openmir2-m2-map-loading` | 真地图昼夜 |
| 10 | `SM_TURN` | `CharDesc(Feature+Status)` + `msg` | `openmir2-m2-record-fields` + world | 记录字段→`CharDesc`、世界→`msg` |

> 表中"配置层"尚无独立卡：`658/11029/1106/5008/811` 的配置项需要在 M2 后续切片里开卡，
> 或在 `openmir2-m2-record-fields` / `map-loading` 收口后评估归并。

---

## 6. 门禁（改本目录后必须全绿）

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo test --workspace --offline
cargo run -p msgcodegen -- --check
```

本目录是脚本/手册，不引入 Rust 代码；R4-1 单测在 `mir2-rs/bins/game-svr/src/main.rs`（game-svr 包）。

## 7. 本片自检证据（无栈）与残留物清单

本片**不起栈、不起客户端、不占端口**；已跑过的自检（判据 + 原始输出）：

| 自检 | 命令 | 结果 | 原始输出 |
| --- | --- | --- | --- |
| 无栈总自检 | `run_acceptance.ps1 -SelfCheckNoStack` | 5 步全 `PREREQ`、整体 exit 2（不静默通过） | `E:\tmp\openmir2-watch\pi-m2svr-nostack-selfcheck.log` |
| s2c 导出+replay（用 M0 金标准裸 dump） | `export_s2c.ps1 -ProxyDir tests/golden/…/proxy` | replay `RESULT: GREEN`（byte 0 / field 0） | `E:\tmp\openmir2-watch\pi-m2svr-s2c-selftest.log` |
| s2c 对账（同序） | `s2c_diff.py --rust <重导件>` | `GREEN`（16 条同序 + 逐字节 0，归一化仅 recog） | 同上 |
| s2c 红检 | 改 recog ⇒ 仍 GREEN；改尾一字节 ⇒ RED | 归一化只吸 recog、字节差异必红 | 同日志 / `E:\tmp\rust-body-broken.jsonl` |
| 三路径 DB diff 驱动 | `check_db_paths.ps1 -Mode diff` 合成夹具 | 同件 GREEN / 改一字段 RED（复用 `scoped_diff.py`） | 控制台（rc 0 / rc 1） |
| R4-1 红检 | 改回 `unwrap_or(0)` | `route_to_gate_rejects_out_of_range_gate_idx` FAILED；还原后 passed | `E:\tmp\openmir2-watch\pi-m2svr-r41-redcheck.log` |

残留物清单：

- `E:\tmp\m2-acceptance\`：无栈自检产生的 `ports-*.json`/`shots-*.json`/`run-*`/`s2c-selftest/` 等，保留作证据；
- `E:\tmp\openmir2-watch\pi-m2svr-*.log`：四门禁、红检、自检原始输出；
- `E:\tmp\rust-recog-changed.jsonl` / `rust-body-broken.jsonl`：s2c 归一化红检输入（证据）；
- 未新增/未修改进程、端口、库内行（本片全程只读外部状态）。
