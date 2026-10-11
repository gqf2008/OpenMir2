# M2 入图帧 envir/配置层输入 —— C# oracle 对拍证据（2026-10-11）

卡：`openmir2-m2-envir-inputs`（owner `openmir2-worker-16`，分支 `worktree-m2-envir-inputs`）。
本目录是**判据②③**（逐项实测对拍 / 空缺项行为）的证据：C# 探针**实测输出** + 输入 hash + 人读期望值表。
Rust 侧的机器判据在 `mir2-rs/crates/envir/tests/csharp_parity.rs`（synthetic 仓内用例常驻门禁；
真机语料两条 `#[ignore]`）。

## 0. 逐字段来源（C# oracle 文件:行号，按 master `f5170a8e`）

| 目标 | 行号 | 语义 |
| --- | --- | --- |
| `MapInfoFlag` 字段清单 | `src/OpenMir2/Data/MapInfoFlag.cs:3-63` | record class，字段名/类型 1:1 |
| section 行 → `sMapName`/`sMapDesc`/`nServerIndex` | `src/GameSrv/Maps/Maps.cs:52-80` | `ArrestStringEx` + `GetValidStrCap` + `GetValidStr3` |
| Flag 命令解析循环 | `src/GameSrv/Maps/Maps.cs:82-320` | 全部命令（见 §3） |
| Flag/MapDesc 落到环境对象 | `src/GameSrv/Maps/MapManager.cs:64/84-90`、`src/M2Server/Maps/Envirnoment.cs:35/43`、`src/Modules/SystemModule/Maps/IEnvirnoment.cs:21/27` | `Envirnoment{MapName,MapFileName,MapDesc,ServerIndex,Flag}` |
| 帧 766 `SM_AREASTATE` | `src/M2Server/Player/PlayObject.Base.cs:285` | `n8 = FightZone?1 \| SafeArea?2 \| InGuildWarArea?4`（`RefUserState` :270） |
| 帧 54 `SM_MAPDESCRIPTION` | `src/M2Server/Player/PlayObject.Base.cs:894` | `nMusicid = Flag.Music ? Flag.MusicId : -1`；尾 = `Envir.MapDesc`（`SendMapDescription` :887） |
| 帧 46 `SM_DAYCHANGING` | `src/M2Server/Player/PlayObject.Message.cs:1741` | `Param=Bright`、`Tag=DayBright()`；`DayBright()` = `PlayObject.Base.cs:244-266`，用 `Flag.DayLight`/`Flag.boDarkness` |
| `GameGoldName`/`GamePointName` | `src/Modules/SystemModule/Conf/ServerConf.cs:108-109`；默认 `GameSvrConf.cs:1320-1321` | `[Names] GameGold`/`GamePoint` |
| `ClientFile1..3` | `ServerConf.cs:116-118`；默认 `GameSvrConf.cs:1298-1300` | `[Setup] ClientFile1..3` |
| `ClientFile*_CRC` | 声明 `GameSvrConf.cs:1148-1150`，读取 `PlayObject.Message.cs:1690`，**全仓无赋值点** | ⇒ oracle 实测恒 `0` |
| `BonusAbilof*`/`NakedAbilof*` | `ServerConf.cs:165-224`；默认 `GameSvrConf.cs:1413-1418` | `[Setup] BonusAbilof{Warr,Wizard,Taos}{DC,MC,SC,AC,MAC,HP,MP,Hit,Speed,X2}`（`*X2`→`Reserved`） |
| `ClientConf` | 声明 `GameSvrConf.cs:1104`，初值 `:1866-1878`，`ServerConf.LoadConfig` **不读任何键** | ⇒ 现网恒为初值 |
| 序列化口径 | `src/OpenMir2/SerializerUtil.cs:10-13`（MemoryPack/Utf16）+ `src/OpenMir2/EDcode.cs:211-226`（丢首字节后 `Encode`） | `ClientConf` 17B；`NakedAbility` 20B |
| ini 读取编码 | `src/OpenMir2/Common/ConfigFile.cs:263` | `StreamReader(..., GB2312)`，**带 BOM 探测** ⇒ 现网 UTF-8+BOM 的 `Server.conf` 按 UTF-8 解出 `元宝`/`成长` |

## 1. C# 探针

`mir2-rs/tests/parity/csharp/EnvirGolden/`：

- 链接**真实**源码：`Common/StringList.cs`、`Common/ConfigFile.cs`、`Data/MapInfoFlag.cs`、
  `Packets/ClientPackets/ClientConf.cs`、`NakedAbility.cs`；
- `HUtil32Copies.cs`：`ArrestStringEx`/`GetValidStr3`/`GetValidStrCap`/`CaptureString`/`CompareLStr`/`StrToInt`
  的**逐字拷贝**（带来源行号）；
- `Program.cs`：`Maps.cs:52-320` 的 section 解析循环 + `MapManager.AddMapInfo` 拆分**逐字拷贝**，
  只把 `SystemShare.MapMgr.AddMapInfo(...)` 换成收集结果。

```powershell
cd mir2-rs\tests\parity\csharp\EnvirGolden
dotnet build -v q -nologo
$dll = 'bin\Debug\net9.0\EnvirGolden.dll'
$out = '..\..\envir-2026-10-11'
dotnet $dll mapinfo ..\..\envir-2026-10-11\MapInfo-synth.txt ..\..\envir-2026-10-11\mapinfo-synth-cs.json
dotnet $dll mapinfo E:\MirServer\M2GameSvr\Envir\MapInfo.txt ..\..\envir-2026-10-11\mapinfo-real-cs.json
dotnet $dll conf ..\..\envir-2026-10-11\Server.conf-synth ..\..\envir-2026-10-11\serverconf-synth-cs.json
dotnet $dll conf <Server.conf 副本> ..\..\envir-2026-10-11\serverconf-real-cs.json
dotnet $dll clientconf ..\..\envir-2026-10-11\clientconf-cs.json
```

（真机 `Server.conf` 传**副本**，避免 `ConfigFile.ReadWrite*` 的缺键写回碰到现网文件。）

## 2. 输入 hash（判据「路径 + hash」）

见 `SHA256SUMS.txt`（本轮）：

```
f8964049…  mir2-rs/tests/parity/envir-2026-10-11/MapInfo-synth.txt
86a6a68c…  E:/MirServer/M2GameSvr/Envir/MapInfo.txt
ce5e3ce9…  E:/MirServer/M2GameSvr/Server.conf
```

## 3. 人读期望值表（从 C# 探针原始输出取出，非按源码公式重推）

### 3.1 真机 `MapInfo.txt`（322 条 section）

| 统计 | 值 |
| --- | --- |
| section 数 | 322 |
| `DAY`（`day_light`） | 206 |
| `FIGHT`（`fight_zone`） | 85 |
| `SAFE`（`safe_area`） | 7 |
| `MUSIC` | **0**（现网没有 MUSIC 图 ⇒ `nMusicid` 全 -1） |
| `DARK`（`bo_darkness`） | 4 |
| `NORECONNECT` | 47 |
| `SM_AREASTATE` 的 `n8` 分布 | `{0:230, 1:85, 2:7}`（无 FIGHT+SAFE 同图） |

帧锚点（`[0\t比奇省] DAY`，第 281 行）：

| 字段 | C# 实测值 | Rust 断言 |
| --- | --- | --- |
| `map_name` | `0` | 相等（`csharp_parity` 逐字段） |
| `map_file_name` | `0` | 相等 |
| `map_desc`（帧 54 尾） | `比奇省` | 相等 |
| `Flag.DayLight` | `true` | 相等 |
| `Flag.Music` / `MusicId`（帧 54 Recog） | `false` / `-1` | 相等 |
| `Flag.FightZone` / `SafeArea`（帧 766 Recog） | `false` / `false` ⇒ `n8=0` | 相等 |

### 3.2 真机 `Server.conf`

| 键 | C# 探针实测值 |
| --- | --- |
| `[Names] GameGold` | `元宝` |
| `[Names] GamePoint` | `成长` |
| `[Setup] ClientFile1/2/3` | `mir.dat` / `mir.2` / `mir.3` |
| `ClientFile1_CRC/2_CRC/3_CRC` | `0` / `0` / `0`（无赋值点） |
| `BonusAbilofWarr`（A 段/战士） | DC17 MC20 SC20 AC20 MAC20 HP1 MP1 Hit20 Speed35 Reserved0 |
| `BonusAbilofWizard` | DC17 MC25 SC30 AC20 MAC15 HP2 MP1 Hit25 Speed35 Reserved0 |
| `BonusAbilofTaos` | DC20 MC30 SC17 AC20 MAC15 HP2 MP1 Hit30 Speed30 Reserved0 |
| `NakedAbilofWarr`（C 段/战士） | DC512 MC2560 SC20 AC768 MAC1280 HP0 MP0 Hit0 Speed0 Reserved0 |
| `NakedAbilofWizard` | DC512 MC512 SC2560 AC1280 MAC768 HP0 MP0 Hit5 Speed0 Reserved0 |
| `NakedAbilofTaos` | DC513 MC2560 SC512 AC512 MAC512 HP0 MP0 Hit0 Speed0 Reserved0 |
| `ClientConf`（帧 11029 尾的内层 17B） | `01 01 01 00 01 01 01 00 00 00 00 00 00 00 00 01 00` |

`NakedAbility` 20B 序列化（`Encode(..., pos:1)` 前）：7×u16 LE + Hit u8 + Speed i32 LE + Reserved u8。
金标准 `session-20261010-c3-hook` 的 #14（帧 811）三段单解各 20B，A 段 = `1100140014001400140001000100142300000000`，
与本表 `BonusAbilofWarr` 一致；C 段 = `0002000a14000003000500000000000000000000`，与 `NakedAbilofWarr` 一致。

### 3.3 synthetic 夹具（覆盖真机没有的命令 + oracle 既有形态）

- `DECHP(100/3)` ⇒ `n_dechppoint=3`、`n_dechptime=100`（**point/time 与名字相反**，`Maps.cs:200-205`）；
- `NOHORSE` ⇒ `bo_nopositionmove=true`、`no_horse=false`（`Maps.cs:286-290` 的既有形态）；
- `PKWINLEVEL(6`（无 `)`）⇒ `n_pkwinlevel=5`（沿用上一节 `NEEDSET_ON(5)` 的 `sLine`）；
- `NORECONNECT(5`（无 `)`）⇒ `s_no_reconnect_map=G002`（沿用第 3 行的 `sReConnectMap`）；
- `["G" 辛地 0]` ⇒ `map_name="G`（`CaptureString` 路径）；
- `[X10 "桃源" 1]` ⇒ `map_desc=桃源`、`server_index=0`（引号剥离后服务器号位丢失）；
- `Fight2`/`Fight5`/`STALL`/`NOTALLOWUSEMAGIC(...)` 被忽略；`L5` ⇒ `request_level=5`。

## 4. 红检（判据④）

把 `Maps.cs` 命令循环里的 `DAY` 支改坏（不再置 `day_light`）后：

```
$ cargo test -p mir2-envir --offline
test synthetic_mapinfo_matches_csharp_probe ... FAILED
  assertion `left == right` failed: 第 0 个 section 与 C# 探针不等
test result: FAILED. 2 passed; 1 failed; 2 ignored

$ cargo test -p mir2-envir --test csharp_parity --offline -- --ignored
test real_mapinfo_matches_csharp_probe ... FAILED
  assertion failed: map0.flag.day_light
test result: FAILED. 1 passed; 1 failed
```

原始输出：`E:\tmp\openmir2-watch\pi-m2env-redcheck-day.log`、`pi-m2env-redcheck-day-real.log`；
还原后：`pi-m2env-redcheck-restored.log`（全绿）。
