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

**先验证「每一步真的会红」**（构造夹具，不起栈/不占端口）：

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/selftest.ps1
# 19 条口径全 OK ⇒ exit 0；任何一条不符 ⇒ exit 1（见 §8）
```

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

> **2026-10-11 更新**：两条依赖卡**已合入 master**——`openmir2-m2-map-loading`（`2a2e8d03`，`crates/envir`）
> 与 `openmir2-m2-record-fields`（`353a28e2`，`bins/game-svr/src/record.rs` 全字段）。下表据此重判
> 「还差哪一步」；**已合入 ≠ 已接线**（接线见 §5.2）。

| ident | 帧 | 缺的运行时输入 | 由哪张卡提供 | 还差哪一步（2026-10-11 重判） |
| --- | --- | --- | --- | --- |
| 658 | `SM_SENDNOTICE` | 公告文本（`NoticeMgr.GetNoticeMsg`） | 配置层（未开卡） | 读 Envir/Notice 文本并接入公告管理器 |
| 51 | `SM_NEWMAP` | ✅ 逻辑名+坐标（记录）；`map_id`/尺寸 ✅（envir）；**`DayBright()`** | `Env.Flag`（envir **未解析**） | envir 卡只给 cells/doors/points/尺寸/map_id，**没有 `Envir.Flag.DayLight/boDarkness`** ⇒ `DayBright()` 仍缺；占位 0 |
| 50 | `SM_LOGON` | `MessageBodyWL=(GetFeatureToLong,CharStatus,Tag1,Tag2)` | 物品/装备层（未开卡）+ world | `GetFeatureToLong` 要 `UseItems[Dress/Weapon]` 的 `StdItem.Shape`（`PlayObject.cs:3395`）⇒ 需物品系统 |
| 41 | `SM_FEATURECHANGED` | `GetFeatureToLong()` | 同上 | 同 50 |
| 213 | `SM_ATTACKMODE` | `AttatckMode` | ✅ `record.rs` 全字段（已合） | 只剩把 `CharacterData.attatck_mode` 带进 `EnterWorldInputs`（属 §5.2 的下一片） |
| 11029 | `SM_SERVERCONFIG` | `ClientConf` 序列化原文 | 配置层（未开卡） | `SystemShare.Config` 落地 + `EncodeMessage` |
| 42 | `SM_USERNAME` | `(targetId,nameColor,showName)` | ✅ `record.rs` + world | `GetChrColor`/展示名属 world 侧，未接线 |
| 766 | `SM_AREASTATE` | `Env.Flag.FightZone/SafeArea` + `InGuildWarArea` | `Env.Flag`（envir **未解析**） | envir 卡未解析 MapInfo 的 Flag 行 ⇒ 缺输入 |
| 54 | `SM_MAPDESCRIPTION` | `Envir.MapDesc` + `nMusicid=Flag.Music?MusicId:-1` | `Env.Flag`（envir **未解析**） | `MapInfoEntry.desc` 只是原文，Flag/音乐 id 未解析 ⇒ 缺输入 |
| 5008 | `SM_GAMEGOLDNAME` | `GameGold/GamePoint` ✅（记录）；元宝名/金币名 | 记录字段（已合）/ 配置层 | 只剩金币名配置（未开卡） |
| 1106 | `SM_VERSION_FAIL` | `ClientFile1_CRC/ClientFile2_CRC` | 配置层（未开卡） | `Config.ClientFile*_CRC` 落地 |
| 52 | `SM_ABILITY` | `Gold/Job/GameGold` ✅（记录）；`WAbil` 编码 | ✅ 记录字段（已合）+ 编码接缝（**本片补**） | **接缝已补**（`record::encode_ability`，序列化后丢成员数首字节，金标准 #12 锚定）；仍差**触发点接线**（`RM_ABILITY`/`RM_LEVELUP` 世界事件 → 下发） |
| 752 | `SM_SUBABILITY` | `AntiMagic/HitPoint/SpeedPoint/AntiPoison/PoisonRecover/HealthRecover/SpellRecover` | ❌ 这些是 **world 侧 `PlayObject`/`BaseObject` 状态**（`BaseObject.cs:105-121`，由 `RecalcAbilitys` 算出），并不在 `CharacterData`；`WAbil` 编码接缝已补 | 同 52 的触发点，且这 7 个值需先建 world 侧 `RecalcAbilitys` 产出（未建）⇒ 仍缺输入 |
| 811 | `SM_ADJUST_BONUS` | `BonusPoint` + A/B/C 模板 | ✅ `record.rs`（`bonus_point`/`bonus_abil`）+ 配置层 | A/B/C 取 `Config.BonusAbilofXxx/NakedAbilofXxx`（配置层未开卡） |
| 46 | `SM_DAYCHANGING` | `Bright/DayBright` | `Env.Flag`（envir **未解析**） | 同 51 的 `DayBright()`；`Bright` 属全局 |
| 10 | `SM_TURN` | `CharDesc(Feature+Status)` + `msg` | 物品层（未开卡）+ world | `Feature` 同 50 的 `GetFeatureToLong` ⇒ 需物品系统 |

> **本片新查明的两条精确阻塞**（比「哪张卡」更可操作）：
> 1. `crates/envir` **只解析格属性/门/点位/尺寸/`map_id`**，**没有** `Envir.Flag`（`FightZone/SafeArea/DayLight/boDarkness/Music/MusicId`）与 `MapDesc` 的语义 —— 帧 `51/766/54/46` 卡在这里；要么给 envir 补 `MapInfo` Flag 解析（触及 `crates/envir/**`，本片禁改），要么新开卡。
> 2. `record.rs` 全字段已入库，但**没有公开的 `Ability` 编码**（只有私有 `encode_opt_ability`）—— 帧 `52/752` 卡在这里（`EncodeMessage` 需「序列化后丢首字节」的半开区间口径）。
>    **2026-10-11 后续**：本片已补上公开 `record::encode_ability`（金标准 #12 逐字节锚定）⇒ **52 的输入接缝已解除**；752 另需 world 侧 `RecalcAbilitys` 产出（见上表）。
> 表中「配置层」（`658/11029/1106/5008/811`）尚无独立卡：需要在 M2 后续切片里开卡，或评估归并。

### 5.2 本片接线：`CurMap`（逻辑名）→ `map_id` + 真地图尺寸（集成点 ④）

`openmir2-m2-map-loading` 卡面 ④「集成到 `bins/game-svr`（`WorldMaps` 真尺寸 / `CurMap`→`map_id`）」本片落地：

- `bins/game-svr/src/mapwire.rs`（新增）：`base_path`（`Server.conf` 所在目录 = 生产下的可执行体目录，
  oracle `M2Share.BasePath`）/ `load_registry`（`base/<EnvirDir>/MapInfo.txt` + `base/<MapDir>/*.map`）/
  `sizes`（`map_id` = 注册表下标）/ `resolve_map_id`（`require_id`，找不到 ⇒ `Err`）；
- `config.rs`：新增 `[Share] EnvirDir`/`MapDir`（oracle `ServerConf.cs:75-76` 的键，缺省 `Envir`/`Map`，**不发明键名**）；
- `main.rs`：启动时加载注册表（有 ⇒ 用其尺寸建世界并把 `CurMap` 换成 `map_id`；无 ⇒ 保留 S1 `WorldMaps` 占位并记 WRN）。
  **有注册表时找不到 `CurMap` ⇒ 拒绝进图并记 WRN**（照卡面④「不许静默占位」）；
- `WorldMaps` 占位键**暂留**（无 Envir 时的回退，登记：真机验收——那时 Envir 必在——之后删）。

判据（`mapwire::tests`，不需要栈）：合成 `Envir/MapInfo.txt` + 合成 `.map` ⇒ 两张入表、`map_id` 按 section 序、
`[0129|0100]` 认**逻辑名** `0129`、未知名 ⇒ `Err`、无 Envir ⇒ `None`+`Ok(0)` 回退。
**红检**：把 `resolve_map_id` 改成 `id_of(cur_map).unwrap_or(0)` ⇒ `unknown_cur_map_is_rejected` 必红
（原始输出 `E:\tmp\openmir2-watch\pi-m2svr-wire-redcheck.log`）。

**R3 收口（reviewer-3 对 308396cd 的 4 条 Minor，2026-10-11 后续片）**：

| id | 问题 | 处置 | 判据（含红检） |
| --- | --- | --- | --- |
| R3-1 | `Placeholder` 同时吞掉「无 `MapInfo.txt`」与「文件在但 0 条有效 section」 | `load_registry` 用 `MapLoader::map_info_path().exists()` 区分：文件在但空表/全解析失败 ⇒ `Broken`（`main` 拒启），`Placeholder` 只留给**无文件** | `mapwire::tests::empty_map_info_file_is_broken_not_placeholder`；红检 `if info_path.exists()`→`if false` ⇒ 用例必红（`pi-m2svr-r3-redcheck-r31.log`） |
| R3-2 | `[Share]`/`EnvirDir` 按 ASCII 精确匹配，oracle `ConfigFile` 是 `StringComparer.OrdinalIgnoreCase` | section 与键名统一 `to_ascii_uppercase()` 后再匹配（**值不动**） | `config::tests::section_and_key_matching_is_ascii_case_insensitive`；红检 `"SHARE"`→`"Share"` ⇒ 必红（`pi-m2svr-r3-redcheck-r32.log`） |
| R3-3 | 显式相对 `--config`（`Server.conf`）⇒ `parent()=Some("")`，`Envir`/`Map` 相对 cwd | `base_path` 先把相对路径规范化到当前目录再取 parent（对齐 oracle `AppContext.BaseDirectory`） | `mapwire::tests::relative_conf_base_is_normalized_to_cwd`；红检回退 `conf.parent()` ⇒ 必红（`pi-m2svr-r3-redcheck-r33.log`） |
| R3-4 | `main.rs` 进世界分支缩进浅 4 格（长 `glog!` 使 rustfmt 跳过该块） | 手工重排缩进 + 把长 `glog!` 消息拆成 `concat!` 两段 | `cargo fmt --all -- --check` exit 0（改前 rustfmt 对该块**无输出**=stable，改后仍 stable） |

### 5.3 顺序判据② 的「生产路径覆盖率」与触发组口径（本片登记）

生产路径只下发**整个 oracle 触发组**输入都已齐备的帧；登记时容易漏掉「组」这一层：

- `SM_LOGON(50)` / `SM_FEATURECHANGED(41)` / `SM_ATTACKMODE(213)` 在 oracle 里是**同一个** `PlayObject.SendLogon()`
  （`PlayObject.cs:1127-1147`）里连发的三条；`213` 的输入（`AttatckMode`）虽已可从记录取到，
  但 `50/41` 的 `GetFeatureToLong()` 仍需物品/装备系统 ⇒ **本片不单独下发 213**（否则会在客户端上
  出现“有攻击模式、无登录帧”的半组，比不下发更坏），登记为**整组就绪后再发**。
- `SM_ABILITY(52)`/`SM_SUBABILITY(752)` 在 oracle 由 `RM_ABILITY`/`RM_LEVELUP` 触发（`PlayObject.Message.cs:1673/1732`）；
  52 的记录输入与编码接缝已齐，但触发点属 S3 世界事件接线；752 还缺 world 侧状态。
- 因此本片**不改生产路径的下发集合**（仍只有 `SM_NEWMAP(51)`）；`EnterWorldInputs::frames()` 的 16 条
  顺序判据是编码层判据，生产覆盖率按“组”推进。

> 结论口径：**“已合入/已备输入”不是接口“已下发”**。以后每接一帧，都要先确认它所在的
> oracle 触发组能产出的成员与顺序，再改生产集合，并同步 §5.1 与本表。

---

## 6. 门禁（改本目录后必须全绿）

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo test --workspace --offline
cargo run -p msgcodegen -- --check
```

本目录是脚本/手册，不引入 Rust 代码；R4-1 单测在 `mir2-rs/bins/game-svr/src/main.rs`（game-svr 包），
52 输入接缝的单测在 `mir2-rs/bins/game-svr/src/record.rs`（`encode_ability_matches_golden_frame_12_tail`）。

## 7. 本片自检证据（无栈）与残留物清单

本片**不起栈、不起客户端、不占端口**；已跑过的自检（判据 + 原始输出）：

| 自检 | 命令 | 结果 | 原始输出 |
| --- | --- | --- | --- |
| 无栈总自检 | `run_acceptance.ps1 -SelfCheckNoStack` | 5 步全 `PREREQ`、整体 exit 2（不静默通过） | `E:\tmp\openmir2-watch\pi-m2svr-nostack-selfcheck.log` |
| s2c 导出+replay（用 M0 金标准裸 dump） | `export_s2c.ps1 -ProxyDir tests/golden/…/proxy` | replay `RESULT: GREEN`（byte 0 / field 0） | `E:\tmp\openmir2-watch\pi-m2svr-s2c-selftest.log` |
| s2c 对账（同序） | `s2c_diff.py --rust <重导件>` | `GREEN`（16 条同序 + 逐字节 0，归一化仅 recog） | 同上 |
| s2c 红检 | 改 recog ⇒ 仍 GREEN；改尾一字节 ⇒ RED | 归一化只吸 recog、字节差异必红 | 同日志 / `E:\tmp\rust-body-broken.jsonl` |
| 三路径 DB diff 驱动 | `check_db_paths.ps1 -Mode diff` 合成夹具 | 同件 GREEN / 改一字段 RED（复用 `scoped_diff.py`） | 控制台（rc 0 / rc 1） |
| **手册判红自检** | `selftest.ps1` | **19/19 OK**：②截图 4 例、③b s2c 4 例（尾字节改坏⇒RED、只改 recog⇒GREEN）、③c DB 3 例、①端口实况判属 1 例、①a 分类器 5 例（R2-1）、聚合器 2 例 | `E:\tmp\openmir2-watch\pi-m2svr-selftest.log` |
| 52 输入接缝 | `cargo test -p game-svr --offline encode_ability` | 1/1 OK（`encode_ability` 逐字节等于金标准 #12 的 `EncodeMessage(WAbil)` 体） | `E:\tmp\openmir2-watch\pi-m2svr-ability-test.log` |
| R4-1 红检 | 改回 `unwrap_or(0)` | `route_to_gate_rejects_out_of_range_gate_idx` FAILED；还原后 passed | `E:\tmp\openmir2-watch\pi-m2svr-r41-redcheck.log` |
| **CurMap→map_id 接线单测** | `cargo test -p game-svr --offline mapwire` | 3/3 OK（逻辑名→id / 未知名必拒 / 无 Envir 回退） | 含在 `pi-m2svr-wire-test.log` |
| **CurMap→map_id 红检** | 把 `resolve_map_id` 改成 `id_of(cur_map).unwrap_or(0)` | `unknown_cur_map_is_rejected` FAILED（rc=101）；还原后 3/3 OK | `E:\tmp\openmir2-watch\pi-m2svr-wire-redcheck.log` |
| **R3-1/2/3 单测** | `cargo test -p game-svr --offline`（mapwire 6 / config 4） | 全 `ok`；新增 3 条（空 MapInfo ⇒ Broken / 大小写不敏感 / 相对 conf 规范化到 cwd） | `E:\tmp\openmir2-watch\pi-m2svr-r3-unittests.log` |
| **R3-1/2/3 红检** | 分别注入 `if false` / `"SHARE"→"Share"` / 回退 `conf.parent()` | 三条新用例各自 **FAILED**（rc=101）；还原后全绿 | `pi-m2svr-r3-redcheck-r31/r32/r33.log` |
| **R3-4 rustfmt** | `cargo fmt --all -- --check`（改前对进世界块无输出 ⇒ 只靠手工重排） | 改后 exit 0 | 门禁日志 |
| 四门禁（接线后 head） | fmt / clippy `-D warnings` / `test --workspace --offline` / msgcodegen `--check` | fmt 0 / clippy 0 / **51 目标 355 passed 0 failed 11 ignored** 0 / msgcodegen 604 OK 0 | `pi-m2svr-wire-test.log`、`pi-m2svr-wire-gates.log` |

残留物清单：

- `tools/m2-acceptance/selftest.ps1`、`tools/m2-acceptance/mutate_capture.py`（本片新增：判红/判绿自检 + s2c 红检夹具生成器）；
- `tools/m2-acceptance/portclassify.ps1`（本片新增：R2-1 的共享分类器，供 `check_ports.ps1`/`selftest.ps1` 同一口径使用）；
- `E:\tmp\m2-acceptance\`：无栈自检产生的 `ports-*.json`/`shots-*.json`/`run-*`/`s2c-selftest/` 等，保留作证据；
- `E:\tmp\m2-acceptance\selftest-*\`：`selftest.ps1` 的 19 条构造夹具与 `selftest.json`（证据，保留）；
- `E:\tmp\openmir2-watch\pi-m2svr-*.log`：四门禁、红检、自检原始输出；
- `E:\tmp\rust-recog-changed.jsonl` / `rust-body-broken.jsonl`：s2c 归一化红检输入（证据）；
- `E:\tmp\openmir2-watch\pi-m2svr-wire-redcheck.log`：`CurMap→map_id` 接线红检原始输出（证据，保留）；
- `E:\tmp\openmir2-watch\pi-m2svr-wire-test.log` / `pi-m2svr-wire-gates.log`：接线后四门禁原始输出（证据，保留）；
- 未新增/未修改进程、端口、库内行（本片全程只读外部状态）。

---

## 8. 每一步都验过会红：`selftest.ps1`

`run_acceptance.ps1` 的四步各有「失败怎么判红」的书面口径。本脚本用**构造输入**把每条口径都跑一遍，
断言「退出码 **和** 输出关键字」两者都对——**只看退出码会把“脚本崩了”误当“判红”**：

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/selftest.ps1
# 19 条全 OK ⇒ exit 0；任何一条不符 ⇒ exit 1（SELFTEST FAIL）
```

| # | 步骤 | 构造输入 | 期望 |
| --- | --- | --- | --- |
| 1 | ② | 空截图目录 | `2` + `PREREQ_MISSING` |
| 2 | ② | 只有 6/7 张 | `1` + `RED: 截图不全` |
| 3 | ② | 7 张但有一张 <1KiB | `1` + `RED: 截图不全` |
| 4 | ② | 7 张齐全非空 | `0` + `GREEN: 7 张` |
| 5 | ③b | 当前口径金标准自比 | `0` + `GREEN: 同序列` |
| 6 | ③b | `SM_NEWMAP(51)` 尾翻 1 字节（非 recog） | `1` + `RED: …未登记差异` |
| 7 | ③b | 只改 `Recog`（归一化清单唯一字段） | `0` + `GREEN: 同序列` |
| 8 | ③b | `--rust` 指向不存在的文件 | `2` + `PREREQ_MISSING` |
| 9 | ③c | 三路径字段级相同 | `0` + `GREEN: 小退/掉线…` |
| 10 | ③c | `shutdown` 改一个被比对字段 | `1` + `RED: 路径 DB diff 未归零` |
| 11 | ③c | 缺 `shutdown-after.jsonl` | `2` + `PREREQ_MISSING` |
| 12 | ① | **本机实况** 5000/7000/7100/7200 | 全 Rust `0` / 有非 Rust 在听 `1` / 有端口没听 `2`（期望由**共享分类器** `portclassify.ps1`——与 `check_ports.ps1` 同一函数——现场算出，不写死） |
| 12a | ①a | 分类器：`E:\MirServer\GameSvr.exe` | `csharp` |
| 12b | ①a | 分类器：`…\mir2-rs\target\…\game-svr.exe` | `rust` |
| 12c | ①a | 分类器：`E:\tmp\openmir2-m2svr-target\…\game-svr.exe` | `rust` |
| 12d | ①a | 分类器：`C:\dev\game-svr.exe`（基名回落） | `rust`（**R2-1**：旧内联判据在此会算成非 Rust） |
| 12e | ①a | 分类器：`C:\dev\foo.exe` | `unknown` |
| 13 | 聚合 | `run_acceptance.ps1 -SelfCheckNoStack` | `2` + `PREREQ_MISSING` |
| 14 | 聚合 | `-SkipPorts -SkipS2c -SkipDb -ShotsDir <6/7>` | `1` + `RED: 有步骤判红`（**红不许被说成前置**） |

全部是**只读/构造夹具**：不起栈、不起客户端、不占端口（①只 `Get-NetTCPConnection` 取实况）。
证据：`E:\tmp\m2-acceptance\selftest-*\selftest.json`（逐条 name/expected/actual）。

> **踩过的坑（写进脚本防复发）**：夹具 `.jsonl` 必须**无 BOM** 写。PS 5.1 的
> `Set-Content -Encoding utf8` 会写 UTF-8 BOM，python `json.loads` 直接 `Unexpected UTF-8 BOM` 抛错，
> 于是「相同件」也拿到 rc=1——**看起来像判红，其实是夹具崩了**（只看退出码就会假绿这条自检）。
> 脚本内改用 `[System.IO.File]::WriteAllText(..., New-Object System.Text.UTF8Encoding($false))`，
> 并同时断言输出关键字，正是为了区分「真判红」与「脚本崩」。
