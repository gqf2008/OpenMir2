# OpenMir2 文本编码面审计 v2（对 oracle C#）

- 基线：`master` @ `20497167b`（只读审计；worktree `/e/tmp/wt-encoding`，branch `encoding-audit-v2`）
- 审计日期：2026-10-21
- 等级：**tier=L**（纯文档 + 只读探测；不改 `src/**`、`sql/**`、`MirClient/`、
  `mir2-rs/crates/{protocol,storage}`、`E:/MirServer/**`；不起服务端/客户端/代理；不占端口；不写数据库）
- 取代：`mir2-rs/tests/parity/m3/audit-encoding-20261020.md`（v1 顶部已标注「已被 v2 取代」）
- 权威登记（只引用、不重复上报）：`whitelist.md:98` **B-105**、`whitelist.md:78` **A-14**、
  `whitelist.md:18` **D-2**、`whitelist.md:99` **B-103**；**B-1**（现网 `Server.conf` 的 UTF-8 BOM）
  登记在 `docs/M2验收真机报告-20261011-3.md` 与 `docs/M2顺序判据②-对账台账.md §4.7`——
  **不是** `whitelist.md:33` 的 B-1（那是 `MonGetRandomItems` 的 `itemName` 缺陷，与此无关）

> **本版结论**：仓内仍存在「同一 oracle 语义、多种实现」的分裂，且**编码器**没有任何一处按 cp936
> 全表实现。影响面是**字节/长度/字符串比较**，不是 UTF-8 直写。**现有部署语料（639 个 Envir 文件、
> 1357 个部署文本文件、46,102 个 DB 文本值）没有命中任何生产可达的编码分歧字符**
> （唯一命中 `U+E7C8` 的 `GuildRankNameFilter.txt` 无生产加载器，见 §4.1）；E-1/E-2 因此是
> 「影响高 / 可达性未证」，不是「现网必现」。

---

## 0. 评审 6 条的逐条落实

| # | 2026-10-20 评审要求 | 本版落实位置 | 结论 |
|---|---|---|---|
| 1 | E-2/E-4 引 **B-105**；**A-14 不是 decode-only**（正文已含 DBSrv 1100 的 `HUtil32.GetBytes(chrName)`） | §6、E-2、E-4 | 已改。A-14 明确按「编解码双向近似」引用；E-1/E-2 不再声称「A-14 只覆盖 decode」 |
| 2 | E-4 量级改为 **7032 对**；补 **UTF-32LE `FF FE 00 00`**；**Rust-B 的 BOM 判断顺序**（`FF FE` 先于 UTF-32LE）也错 | §2.1、§2.2、E-4 | 已实测：7032 = 6,930 多字符 + 101 PUA + 1 `FE FF` 被 encoding_rs 当 BOM 吞成空串（评审「6931 非单 `?`」= 6,930 + 该 1 对）。UTF-32LE 与顺序错误已用 .NET/Rust 对照证实 |
| 3 | E-1/E-2 补 **99 个 WHATWG-only 字符**、非 BMP 的 UTF-16 码元回退（`U+1F600 → 3F 3F` vs 1 个 `?`）；给 .NET 探测表生成编码表方案，实跑小程序贴命令与输出 | §1.1、§2.3、E-1、§5.2 | 已实跑 `dotnet 9.0.313` + `System.Text.Encoding.CodePages 8.0.0`，全 BMP 63,488 码点对照：**99 WHATWG-only / 496 cp936-only / 38,907 双不可映射**；命令与输出见 §1.1、§2.3 |
| 4 | E-7 改成：**ChrName 是 utf8 列，列值绑定解码后的 String；内存 key 用 String + 已登记折叠（D-2/B-103）** | E-7 | 已用 `information_schema` 证实 `characters.ChrName` / `characters_indexes.ChrName` = `utf8mb3_general_ci`；oracle 绑的是 string（`PlayDataStorage.Add.cs:107`、`PlayRecordStorage.cs:267/287`） |
| 5 | E-3 把 `db-srv/gbk.rs::arrest` 纳入同卡；`route.rs` 定性订正（本地 ASCII 配置，不是主要可达面） | E-3 | 已纳入；`route.rs` 只解析 `AddrTable/SelectID/ServerInfo/MapInfo` 本地行，非线上文本面 |
| 6 | 分级：E-1/E-2 改「**影响高 / 可达性未证**」；先扫 639 个 Envir 文件与 DB 列给可达性证据 | §4、E-1、E-2 | 已扫 639 Envir + 1357 部署文本 + 46,102 DB 值；命中 0 条生产可达面 |

---

## 1. 探测方法与可复现命令（全部离线、只读）

### 1.1 .NET cp936 编码表（本机实跑）

```powershell
# E:/tmp/enca-v2/encprobe-dotnet（net8.0 + System.Text.Encoding.CodePages 8.0.0）
dotnet run -c Release -- E:/tmp/enca-v2/cp936-encode-bmp.txt E:/tmp/enca-v2/cp936-extra.txt
```

输出（原样）：

```text
codepage=936 name=gb2312 encoderFallback=InternalEncoderBestFitFallback
wrote E:/tmp/enca-v2/cp936-encode-bmp.txt (BMP encode table) and E:/tmp/enca-v2/cp936-extra.txt (extra vectors)
```

`cp936-extra.txt`（逐字符 `Encoding.GetEncoding(936).GetBytes`）：

```text
U+FFFD	3F          U+00A0	3F          U+E7C8	A8BF
U+1F600	3F3F        U+2018	A1AE        U+20AC	80
U+01F9	3F          U+4E00	D2BB        U+00E9	A8A6
A	41              U+E4C5	FEFE        U+F8F5	FF
U+FFFF	3F          U+0080	3F          U+00FF	3F
```

同一批向量在 Rust `encoding_rs 0.8.42` 下（`Cargo.toml` 声明 `"0.8.35"`（caret 约束），
`Cargo.lock` 实际解析 **0.8.42**；复验在 0.8.42 下重跑同批向量，输出与下表逐字节一致）：

```text
U+FFFD  -> 26 23 36 35 35 33 33 3B  ("&#65533;")  err=true
U+00A0  -> "&#160;"   err=true        U+E7C8 -> "&#59336;"  err=true
U+1F600  -> "&#128512;" err=true      U+2018 -> A1AE  err=false
U+20AC  -> 80  err=false              U+01F9 -> A8BF  err=false
U+4E00  -> D2BB err=false             U+00E9 -> A8A6 err=false
U+E4C5  -> FEFE err=false             U+F8F5 -> "&#63733;" err=true
U+FFFF  -> "&#65535;" err=true        U+0080 -> "&#128;" err=true
```

> 结论：`encoding_rs::GBK.encode` 对不可映射字符输出 **HTML 数字字符引用**（5~10 字节）且不改变
> `had_errors` 之外的任何语义；oracle 是 **每字符查表 → 不可映射一个 `3F`**。cp936 的
> `InternalEncoderBestFitFallback` 还会做 best-fit（`U+00A6 → 7C`、`U+00AA → 61`、`U+F8F5 → FF` 等），
> 这些不是 `had_errors` 能表达的。

### 1.2 .NET BOM 模型（`StreamReader` 真实判定）

```powershell
dotnet run -c Release --no-build -- bom
```

```text
FF FE 00 00 41 00 00 00 -> enc=utf-32   chars= U+0041
FF FE 41 00             -> enc=utf-16   chars= U+0041
FF FE FF 41             -> enc=utf-16   chars= U+41FF
FE FF 00 41             -> enc=utf-16BE chars= U+0041
00 00 FE FF 00 00 00 41 -> enc=utf-32BE chars= U+0041
EF BB BF 41             -> enc=utf-8    chars= U+0041
00 00 FE FF 00 11 00 00 -> enc=utf-32BE chars= U+FFFD      # 非法码点
FF FE 00 00 00 00 D8 00 -> enc=utf-32   chars= U+FFFD      # 代理码点
FF FE 00 D8 41 00       -> enc=utf-16   chars= U+FFFD U+0041
FF FE 00                -> enc=utf-16   chars= U+FFFD      # 不足 4B，退回 UTF-16LE
```

`StringList.LoadFromFile`（`Common/StringList.cs:276-291`）先按 `GetEncoding`（`:293-360`）给默认编码，
再显式传给 `new StreamReader(fileName, fs)`；`ConfigFile.Load`（`Common/ConfigFile.cs:263`）显式传
`Encoding.GetEncoding("GB2312")`。**两者在 `FF FE 00 00` 上不同**，原因在 `StreamReader.ReadBuffer` 的
顺序：`IsPreamble()` 先于 `DetectEncoding()`，而 `_checkPreamble = encoding.Preamble.Length > 0`
（runtime release/8.0 源码）：

- **StringList**：`GetEncoding` 对 `FF FE …` 返回 `Encoding.Unicode`（preamble 长度 2）⇒ `IsPreamble`
  命中、压缩 2 字节并关掉探测 ⇒ **UTF-16LE**（`U+0000 U+0041 U+0000`）。
- **ConfigFile**：GB2312（codepage 936）**preamble 长度 0** ⇒ 跳过 `IsPreamble`，`DetectEncoding`
  看到 `FF FE 00 00` ⇒ **UTF-32LE**（`U+0041`）。

2026-10-21 本机 .NET 8（与 oracle `TargetFramework` 一致）实测：

```text
gb2312 codepage=936 preamble_len=0    unicode preamble_len=2
string+gb2312 : enc=utf-32  chars=U+0041
stream+gb2312 : enc=utf-32  chars=U+0041
string+unicode: enc=utf-16  chars=U+0000 U+0041 U+0000
stream+unicode: enc=utf-16  chars=U+0000 U+0041 U+0000
```

⇒ 上表的裸构造器行为只对「**无 preamble 的编码**（如 GB2312）」成立；`whitelist.md:98` B-105 的
「`FF FE 00 00` 亦然（UTF-16LE）」描述的是 **StringList 路径**（成立），但**不适用于 ConfigFile**。

### 1.3 .NET 字节边界 / `Split`

```powershell
dotnet run -c Release --no-build -- decode
```

```text
81 20 41             -> len=2  ? A
81 2F                -> len=1  ?
81 7F                -> len=1  ?
81 00 41             -> len=2  ? A
81 FF                -> len=1  ?
81 40                -> len=1  U+4E02
81 0A 41             -> len=2  ? A          # LF 被前导字节吞掉
61 20 81 2F 20 62    -> len=5  a ? b
61 81 2F 62          -> len=3  a?b          # '/' 被吞
41 81                -> len=2  A?
FF                   -> U+F8F5     80 -> U+20AC     80 63 -> U+20AC U+0063
A8 BF                -> U+E7C8     81 20 20 41 -> len=3 ? A
split(' ') of [61 20 81 2F 20 62] -> [a | ? b]
split('/') of [61 81 2F 62]       -> [a?b]
```

### 1.4 Rust 侧探测

- `/e/tmp/enca-v2/encprobe-rust`（`encoding_rs` 声明 `"0.8.35"`、`Cargo.lock` 实际解析 **0.8.42**；
  下述结果均在 0.8.42 下产出，复验重跑同批向量一致）：
  - 全 32,256 个双字节对与 `crates/script/tests/data/cp936-pairs.txt` 对照；
  - 全 BMP 码点 `encode` 对照；
  - 639 个 Envir 文件逐文件对照（cp936 状态机 vs WHATWG）；
  - BOM 分支复刻 `crates/script/src/textfile.rs` 当前顺序。
- `/e/tmp/enca-v2/spanprobe`：`#[path]` 直接引用生产文件
  `mir2-rs/bins/db-srv/src/gbk.rs` 调用 `char_spans/char_count/split2_nonempty/arrest`
  （不是复制品，改生产文件即改探针行为）。
- `/e/tmp/enca-v2/dbprobe`（`MySqlConnector 2.3.5`）：只读 `information_schema` + `SELECT DISTINCT`。

### 1.5 语料与数据库扫描范围

| 范围 | 数量 | 方法 |
|---|---|---|
| `E:/MirServer/M2GameSvr/Envir/**/*.txt` | **639** | cp936 状态机 vs WHATWG 整文件解码 + 原始字节对/非法序列/BOM/PUA/非 BMP 扫描 |
| 部署文本（8 个服务目录的 `.txt/.conf/.ini/.cfg`） | **1357**（M2GameSvr 661 / Mir200 673 / DBServer 6 / LoginSrv 7 / LoginGate 2 / SelGate 3 / RunGate 4 / CloudGate 1） | 逐文件按 BOM/`cp936` 解码后查「cp936 编码 ≠ WHATWG 编码」字符 |
| `mir2_db` / `mir2_data` / `mir2_account` / `mir2_dbsrv6k_mkt_*` | **46,102 个文本值** | 只读 `SELECT DISTINCT`，逐字符对照编码表 |

---

## 2. 事实基线（同一 oracle 语义的两侧差异）

### 2.1 解码：cp936 vs WHATWG GBK = **7032 对**

```
pairs=32256 same=23839 same_after_norm=1385 diff=7032
diff_single_q_multi=6930   # .NET 单 '?' vs WHATWG 多字符
diff_other=102             # 101 PUA + 1 对 FE FF（encoding_rs 当 BOM，返回空串）
cp936_only_pua(single)=101
```

- `same_after_norm=1385`：WHATWG 给单个 `U+FFFD`，脚本解码器归一成 `?` 后与 .NET 相同。
- **7032 = 6,930 多字符 + 101 PUA + 1 `FE FF`**；评审说的「6931 对非单 `?`」= 6,930 多字符 + 该 1 对
  `FE FF`（0 字符也属于「非单 `?`」）。
- 该 7032 与 `crates/script/src/gbk_overrides.rs` 的 7032 条**逐条相等**（B-105 已登记并由穷举门禁覆盖）。
- 因此：**解码表本身不是新发现**；E-4 报的是仓内多个加载器**没有共用**这张表。

### 2.2 BOM 模型对照

| 前缀 | .NET 实际 | Rust-B `script/textfile.rs` 现状 | Rust-A `envir/text.rs` 现状 |
|---|---|---|---|
| `EF BB BF` | UTF-8 | UTF-8 ✓ | UTF-8 ✓ |
| `FF FE 00 00` | **StringList（Encoding.Unicode，preamble 2B）：UTF-16LE**；**ConfigFile（GB2312，preamble 0B）：UTF-32LE**（见 §1.2） | UTF-16LE ✓（enc-core 起） | UTF-32LE ✓（enc-core 起） |
| `FF FE`（非 `00 00`） | UTF-16LE | UTF-16LE ✓ | UTF-16LE ✓ |
| `FF FE FF` | UTF-16LE（`U+41FF`） | UTF-16LE ✓ | **GBK ✗**（`byte2 != 0xFF` 条件） |
| `FE FF` | UTF-16BE | UTF-16BE ✓ | UTF-16BE ✓ |
| `00 00 FE FF` | UTF-32BE | UTF-32BE ✓ | **无分支 ✗**（落 GBK） |
| 其它 | cp936 | cp936 ✓ | 裸 WHATWG ✗（见 2.1） |

补充：`decode_utf32_be` 对非法码点/尾部残余写 `?`，.NET UTF-32 分支写 `U+FFFD`（§1.2 实测）。

### 2.3 编码：cp936 全表 vs `encoding_rs::GBK.encode`

BMP 63,488 个非代理码点逐点对照（`E:/tmp/enca-v2/cp936-encode-bmp.txt` vs `whatwg-encode-bmp.txt`）：

| 类别 | 数量 | 说明 |
|---|---|---|
| 两侧字节相同 | 23,986 | 常用汉字/ASCII |
| **WHATWG-only**（.NET → `3F`，WHATWG → 真实字节） | **99** | 只查 `had_errors` **发现不了**（WHATWG 不报错），必须查表 |
| **cp936-only**（.NET → 真实字节，WHATWG → `&#N;`） | **496** | 84 个 PUA + 412 个其它（Latin best-fit `U+00A6→7C`、`U+00AA→61`、兼容汉字 `U+F900..FA2D` 等） |
| 双不可映射（.NET `3F`，WHATWG `&#N;`） | 38,907 | `had_errors` 只能识别这一类 |
| 两侧都有真实字节但不同（best-fit 差异） | 0 | 本机 cp936 与 WHATWG 无此交集 |

99 个 WHATWG-only 的码点范围（完整清单由 .NET 探测表生成，勿手抄）：

```text
U+01F9, U+1E3F, U+2E81, U+2E84, U+2E88, U+2E8B..U+2E8C, U+2E97, U+2EA7, U+2EAA,
U+2EAE, U+2EB3, U+2EB6..U+2EB7, U+2EBB, U+2ECA, U+2FF0..U+2FFB, U+303E, U+3447,
U+3473, U+359E, U+360E, U+361A, U+3918, U+396E, U+39CF..U+39D0, U+39DF, U+3A73,
U+3B4E, U+3C6E, U+3CE0, U+4056, U+415F, U+4337, U+43AC, U+43B1, U+43DD, U+44D6,
U+464C, U+4661, U+4723, U+4729, U+477C, U+478D, U+4947, U+497A, U+497D,
U+4982..U+4983, U+4985..U+4986, U+499B, U+499F, U+49B6..U+49B7, U+4C77,
U+4C9F..U+4CA3, U+4D13..U+4D19, U+4DAE, U+9FB4..U+9FBB, U+FE10..U+FE19
```

非 BMP 规则：.NET 按 **UTF-16 码元**逐元编码，`U+1F600` = `D83D DE00` = **`3F 3F`**；Rust 逐个
`char` 编码只得 1 个 `&#128512;`。统一编码器必须遍历 `str::encode_utf16()`，代理码元一律 `3F`。

### 2.4 字节边界：.NET 前导字节吞掉**任意**后继

.NET `GetString` 的行为（§1.3）：`0x81..=0xFE` 后**只要有下一字节就消费 2 字节**，不论后继是
`0x00/0x0A/0x20/0x2F/0x7F/0xFF`；只有文件末尾孤立前导才是 1 个 `?`。Rust `db-srv/gbk.rs`
当前要求后继落在 `0x40..=0xFE` 且非 `0x7F`，其余按单字节——**字符数、`Split` 边界都不同**（E-3）。

---

## 3. 逐条发现

> 每条固定五段：**oracle 依据 / Rust 现状 / 可机判判据 / 牙 / tier 与可达性**。
> 「牙」一律要求：**改坏生产路径 ⇒ 指定断言必红**；测试必须调用生产函数，不得只测测试内 helper；
> 不得用「不剥注释的源码文本断言」冒充。

### E-1　one-shot `GBK.encode` 的回退是 `&#NNNN;`，oracle 是逐字符 cp936 表

- **oracle 依据**
  - `src/OpenMir2/HUtil32.cs:689-692`：`GetBytes` = `Encoding.GetEncoding("gb2312").GetBytes(str)`。
  - `src/OpenMir2/EDcode.cs:175-185`：`EncodeString` 先 `HUtil32.GetBytes`。
  - 实测编码器类型 `InternalEncoderBestFitFallback`（§1.1）；不可映射 → `3F`；非 BMP → 每码元 `3F`；
    best-fit 表见 §2.3。
- **Rust 现状**
  - one-shot `encoding_rs::GBK.encode(...)` 且忽略 `had_errors` 的生产编码点：
    `bins/game-svr/src/{downlink.rs:69,npc.rs:65,itemwire.rs:65,main.rs:1275,1345,17481,3136}`、
    `bins/game-gate/src/gbk.rs:22`、`bins/login-gate/src/gbk.rs:22`、`bins/sel-gate/src/gbk.rs:22`、
    `bins/login-srv/src/gbk.rs:21`、`bins/db-srv/src/gbk.rs:187`、
    `crates/data/src/{guild.rs:639,exps.rs:52}`、`crates/envir/src/conf.rs:217`。
  - **订正（复验）**：`crates/script/src/textfile.rs:157`（`encode_gb2312`，`:153-160`）**不是**
    one-shot 点：它逐 `char` 调 `encoding_rs::GBK.encode`、检查 `had_errors` 后写 `?`，不会产生
    `&#N;`（复验者用临时探针复现）。它仍有 §2.3 的 496/99 类编码差异、归 E-1 判据，但不属于
    「忽略 `had_errors`」一类，故从上面的 one-shot 清单移除。
  - 后果：① 不可映射 → `&#65533;` 等 5~10 字节（帧长变）；② 99 个 WHATWG-only 字符被编成真实
    字节而 oracle 编 `?`；③ 496 个 cp936-only（含 best-fit/PUA）被编成 `&#N;` 而 oracle 有确定字节；
    ④ 非 BMP 只得 1 个数字引用而 oracle 是 2 个 `?`。
- **可机判判据**
  1. `tests/vectors/cp936-encode.json` 由 §1.1 的 .NET 表生成；对生产 helper（`downlink::gbk`、
     `npc::merchant_say_gbk`、`itemwire::write_ascii_string`、`db-srv::gbk::encode`、
     `textfile::encode_gb2312`）逐字节断言：`U+FFFD→3F`、`U+00A0→3F`、`U+E7C8→A8 BF`、
     `U+1F600→3F 3F`、`U+01F9→3F`、`U+00A6→7C`、`U+F8F5→FF`、`U+20AC→80`、`U+2018→A1 AE`、
     `U+FFFF→3F`。
  2. 全表判据：把 .NET 的 63,488 条表读入测试，对生产编码器逐点比较；任何一条不同即红。
  3. 端到端：CM_SAY 正文尾放孤立 `0x81 0x20`，比对两侧 `SM_HEAR` 尾（`ChrName:msg`）字节，diff=0。
- **牙**：把 `downlink::gbk`（或 `npc::merchant_say_gbk`）改回
  `encoding_rs::GBK.encode(s).0.into_owned()` ⇒ 判据 1 的 `U+FFFD/U+1F600/U+01F9` 三条必红；
  再把 `had_errors` 简单映射成 `?`（不查表）⇒ `U+00A6/U+F8F5/U+E7C8` 必红。两个红点分别钉住
  「数字引用」与「全表」。
- **tier / 可达性**：**tier=L**（纯逻辑，构造向量可判）；**影响高 / 可达性未证**（见 §4：
  639 Envir + 1357 部署文件 + 46,102 DB 值中 0 条命中；现网客户端只发合法 GBK）。

### E-2　cp936 解码 + WHATWG 编码的错配：合法 `A8 BF` 在说词/写回时损坏

- **oracle 依据**
  - `HUtil32.cs:183-205`：`GetString([A8 BF]) = U+E7C8`；`HUtil32.cs:689`：`GetBytes(U+E7C8) = A8 BF`
    ——**可逆**。
  - 脚本读入走 `StringList.cs:276-291`（cp936）；说词出帧走 `PlayObject.Message.cs:1664`
    （`RM_USERNAME`）与 `:1730`（`RM_HEAR/RM_WHISPER/...` 组）的
    `SendSocket(..., EDCode.EncodeString(processMsg.Msg))`（cp936 编码）。
    `:1724` **不是** `SendSocket`，而是 `RM_MERCHANTSAY` 分支的 `MakeMessage`。
- **Rust 现状**
  - `crates/script/src/gbk_overrides.rs` 的 `GBK_OVERRIDES`（7032 条，**B-105**）把
    `A8BF → U+E7C8`（与 oracle 一致）；`crates/script/src/textfile.rs:22-66` 是解码状态机
    `decode_gb2312`（`:28` 起），只逐对调用该表，**表本身不在 `textfile.rs`**。
    但生产接点 `bins/game-svr/src/npc.rs:65`（`main.rs:4726` 调用）、`guild.rs:639`、
    `textfile.rs:157` 用 WHATWG 编码：`U+E7C8` 不可映射 → `&#59336;`（说词）或 `3F`（写回）。
  - 本质是**解码器 A（cp936）+ 编码器 B（WHATWG）错配**；WHATWG 自己 decode+encode 对
    `A8 BF` 是字节可逆的（解码成 `U+01F9` 再编回 `A8 BF`），所以单纯「网关透传」不炸，**跨
    String 边界**才炸：脚本/DB/配置解出的 `U+E7C8` 被重新编码时。
  - 同一错配也影响**比较**：C# 的 String 是 `U+E7C8`，Rust 的 `gbk_decode`（WHATWG）给 `U+01F9`，
    与脚本/DB 的 String 比较可能不相等。
- **可机判判据**
  1. 夹具脚本含真实字节 `A8 BF`，经 `script::textfile::decode_bytes` 解出后调用
     `npc::merchant_say_gbk`，断言尾字节含 `A8 BF`（不是 `&#59336;`、不是 `3F`）。
  2. `textfile::encode_gb2312` 对 `U+E7C8` 断言 `A8 BF`；`SaveToFile` 往返字节不变。
  3. 对 `U+01F9` 断言 `3F`（99 WHATWG-only 表），对 `U+E7C8` 断言 `A8 BF`（496 表），两者不能混。
- **牙**：删掉 `gbk_overrides.rs` 的 `(0xA8BF, '\u{E7C8}')` ⇒ 判据 1 的解码前置必红；把
  `npc::merchant_say_gbk` 改回 WHATWG ⇒ 判据 1 必红（得到 `&#59336;`）；把 `encode_gb2312` 改回
  WHATWG ⇒ 判据 2 必红（得到 `3F`）。
- **tier / 可达性**：**tier=L**；**影响高 / 可达性未证**。639 文件中只有
  `GuildRankNameFilter.txt` 含 `A8 BF`，而它的 C# 读取点被注释（`Player/PlayObject.Base.cs:3427`），
  Rust 只在 `script-tool/src/gbkoverrides.rs:6` 注释里提到，**无生产加载器**；DB 46,102 值无此字符。
  合法客户端聊天即使输入 `A8 BF`，WHATWG 往返字节仍等于 oracle（见上），所以现网无已知触发。

### E-3　DBSrv 字节级字符边界与 cp936 不符；`arrest` 同卡

- **oracle 依据**
  - `HUtil32.cs:183-205`（`GetString`）；`HUtil32.cs:488`（`ArrestStringEx` 在**解码后的 string**
    上找定界符）。
  - 实测（§1.3）：`81 20/2F/7F/00/FF/0A` 都被前导 `0x81` 成对吞掉。
- **Rust 现状**
  - `bins/db-srv/src/gbk.rs:29-57`：`is_trail` 只认 `0x40..=0xFE` 非 `0x7F`；注释
    「非法序列按单字节处理（.NET 会各自产出替代字符，**字符数一致**）」**是错的**（§1.3 实测不一致）。
  - `char_spans/char_count/split2_nonempty` 都用该边界；`arrest`（`:100-125`）是**裸字节扫描**，
    不识别「前导 + 定界符」会被 oracle 吞掉。
  - 线上可达点：`handler.rs:87`（`#...!` 取帧）、`handler.rs:419`（名字长度 ≥3）、
    `loginsrv.rs:181`（`(...)`）、`loginsrv.rs:191-199`（`/` 分段）。
  - **订正**：`route.rs:149-226` 只解析本地 `SelectID.txt/ServerInfo.txt/MapInfo.txt` 行（IP/端口/
    地图名），不是线上文本面；v1 把它列为主要调用点不准确。它仍用同一组函数，纳入同卡但判据用
    本地夹具即可。
- **可机判判据**（≥20 组向量，测试直接调生产函数）
  - `char_count`：`81 20 41→2`、`81 2F→1`、`81 7F→1`、`81 00 41→2`、`81 FF→1`、`81 40→1`、
    `81 0A 41→2`、`41 81→2`、`81 20 20 41→3`、`FF→1`、`80→1`、`80 63→2`、`A8 BF→1`。
  - `split2_nonempty(b"a \x81/ b", b" ")` = `(b"a", b"? b")`（C# `[a | ? b]`）；
    `split2_nonempty(b"a\x81/b", b"/")` 不切（C# `[a?b]`）。
  - `arrest(b"#a\x81!t!", b'#', b'!')`：C# 解成 `#a?t!` ⇒ `(b"a?t", b"")`；
    `arrest(b"(a\x81)b)", b'(', b')')`：C# ⇒ `(b"a?b", b"")`。
  - 5100 请求级：账号/角色名含 `81 2F` 的登录帧，两侧「接受/拒绝 + 回包字节」一致。
- **实现注记（本审计实跑）**：临时把 `char_spans` 改为「前导吞任意后继」后，`char_count`
  立刻从 `3/2/2/...` 变成 .NET 的 `2/1/1/...`（判据有牙）；但 `split2_nonempty` 仍返回原始字节
  `[61] [81,2F,20,62]`，**不是** C# 解码后的 `[61] [3F,20,62]`。因此只改 `char_spans` **不够**：
  要么按 C# 先 `cp936::decode` 整段再在 String 上 Split，要么在切出的片段上做一次 cp936
  解码（不可映射→`3F`）后再交给调用方。`arrest` 也完全不受 `char_spans` 影响，必须单独改成
  「先按 cp936 字符边界扫描定界符」。
- **牙**：把 `char_spans` 改回「合法后继才成对」⇒ 前 9 条必红；把 `arrest` 改成先解码再找定界符
  （或删掉「前导吞后继」处理）⇒ arrest 两条必红；把 `split2_nonempty` 的片段输出改回原始字节
  ⇒ `split` 两条必红。向量必须调生产 `gbk::*`，不能复制逻辑。
  本审计已做一次「临时改 `char_spans` → 跑探针 → `git checkout` 还原 → `sha256sum -c` 校验」
  的自检：还原后 SHA-256 = `a4f389a3…4623b4`，`git status` 干净。
- **tier / 可达性**：**tier=L**；影响中，触发面是畸形 GBK（第三方/损坏帧）。现有账号/角色名
  为 ASCII/常用汉字，语料 0 命中。

### E-4　两套 StringList 解码器：`envir`/`data` 与 `script` 对同一 oracle 结果不同

- **oracle 依据**：`Common/StringList.cs:276-291` + `:293-360`；`Common/ConfigFile.cs:263`
  ——`StreamReader` 的 BOM 探测优先，有效模型见 §2.2。
- **Rust 现状**
  - **Rust-B**（`crates/script/src/textfile.rs:12-115`）：cp936 状态机 + 7032 覆盖表**正确**
    （B-105），但 BOM 顺序 `FF FE` 先于 `FF FE 00 00` ⇒ **UTF-32LE 被当 UTF-16LE**
    （实测 `U+0000 U+0041 U+0000` vs `U+0041`）；`decode_utf32_be` 非法码点给 `?` 而非 `U+FFFD`。
  - **Rust-A**（`crates/envir/src/text.rs:9-24`）：裸 `encoding_rs::GBK`，无覆盖表；`0xFF→U+FFFD`
    （oracle `U+F8F5`）；`FF FE FF` 落 GBK（oracle UTF-16LE `U+41FF`）；无 UTF-32BE；`FF FE 00 00`
    按 UTF-16LE。
  - **其它裸解码器**：`crates/data/src/lists.rs:495`（文档 `:18-21` 声称「有 BOM 时切 UTF-8/UTF-16」
    但未实现）、`monitems.rs:27`、`guild.rs:450,492`、`bins/game-svr/src/strings.rs:183`、
    三网关 `config.rs`、`bins/login-srv/src/config.rs:132/165/235/259`。
  - **量级**：§2.1 的 **7032 对**；其中 6,930 对会改变字符数/行边界（如 `81 0A` 在 .NET 是
    1 个 `?`，WHATWG 是 `?`+换行 ⇒ **多一行**）。
- **可机判判据**
  1. 固定向量经**每个生产加载器**（`envir::text::decode_text`、`script::textfile::decode_bytes`、
     `data::lists::read_gbk_if_file`、`strings::parse`）解码，逐字符等于 §1.2 的 oracle 值：
     `FF`、`80`、`A8 BF`、`81 20`、`FF FE FF 41`、`FF FE 00 00 41 00 00 00`、
     `00 00 FE FF 00 00 00 41`、`EF BB BF FF 41`、`00 00 FE FF 00 11 00 00`、
     `FF FE 00 00 00 00 D8 00`、`FF FE 00`。
  2. 639 文件逐文件解码 SHA-256：`envir::text`、`script::textfile`、`data` 三个入口必须相同
     （以 B-105 的 `tests/parity/envir-2026-10-10/` 摘录为基线；没有语料时跳过但记 `SKIP`，
     不得静默通过）。
- **牙**：把 `textfile.rs` 的 `FF FE` 分支移到 `FF FE 00 00` 之前 ⇒ 判据 1 的 UTF-32LE 条必红；
  把 `envir::text::decode_text` 改回裸 `encoding_rs::GBK` ⇒ 判据 1 的 `FF`/`FF FE FF`/UTF-32BE
  与判据 2 的 2 个分歧文件哈希必红。
- **tier / 可达性**：**tier=L**；影响中。639 文件中只有 2 个文件在 cp936 vs WHATWG 下不同
  （§4.1），其中 1 个不进生产、1 个走 script 解码器（本就正确）⇒ 现网 0 命中；自定义/新增
  Envir 文件可触发。

### E-5　`Server.conf` 被两条链用不同编码读，网关链要求严格 UTF-8

- **oracle 依据**：`Common/ConfigFile.cs:263` `StreamReader(..., GB2312)` + BOM 探测 ⇒
  GBK 无 BOM / UTF-8 BOM / UTF-16 / UTF-32 都能读。
- **Rust 现状**：`bins/game-svr/src/config.rs:333` `std::fs::read_to_string`（严格 UTF-8）+
  只剥首 `U+FEFF` ⇒ **GBK 或 UTF-16 的 `Server.conf` 直接启动失败**（oracle 正常）；同一文件随后
  `crates/envir/src/conf.rs:211-222` 又用 `decode_text`（GBK/UTF-8）读，两条链口径不同。
  `bins/db-srv/src/config.rs:125` 用 `fs::read` + 自写 INI，**不剥 UTF-8 BOM**；现网
  `dbsvr.conf` 是 UTF-8 BOM，首行 `[DataBase]` 被丢，但缺省值恰好等于部署值因而未暴露（见 §7-3）。
- **可机判判据**：4 份 `Server.conf`（GBK 无 BOM / UTF-8 BOM 现网 / UTF-16LE BOM / 含 `0xFF`）
  经 `game_svr::config::load` 与 `envir::GameSvrConf::from_bytes`，断言「能读 + `[DataBase]`
  `ConnctionString` + `[Server] ServerName`」与 oracle 一致。
- **牙**：把 `config.rs:333` 改回 `read_to_string` ⇒ GBK/UTF-16 夹具必红；把 `decode_text` 改回严格
  UTF-8 ⇒ UTF-8 BOM 夹具必红（现网已由 B-1 钉住，见 `docs/M2验收真机报告-20261011-3.md`）。
- **tier / 可达性**：**tier=L**；影响中。现网 `Server.conf` 是 UTF-8 BOM（B-1，见
  `docs/M2顺序判据②-对账台账.md §4.7`），当前部署可读；其它部署用 GBK 时会启动失败。

### E-6　`String.conf` / `AddrTable` / `UserLimit` / 三网关配置 / data 列表：裸 GBK、不认 BOM

- **oracle 依据**：`StringList.cs:276-291`、`ConfigFile.cs:263` 都做 BOM 探测；
  `LoginSrv/Conf/ConfigManager.cs:100-127`（`AddrTable` 走 StringList）。
- **Rust 现状**：`strings.rs:183`、`login-srv/src/config.rs:132,165,235,259`、
  `game-gate/src/config.rs:95`、`login-gate/src/config.rs:56`、`sel-gate/src/config.rs:56`、
  `data/src/lists.rs:487-497`、`monitems.rs:27`、`guild.rs:449-463,492` —— 全部 `GBK.decode`
  整文件、无 BOM 分支；`lists.rs:18-21` 的文档还声称「有 BOM 时切 UTF-8/UTF-16」。
- **影响**：UTF-8 BOM 的 `string.conf` ⇒ 客户端文案乱码；BOM 的 `AddrTable` ⇒ 选服列表/服务名
  乱码或匹配失败；`crates/data` 列表内容乱码。
- **可机判判据**：每类文件做 GBK / UTF-8 BOM / UTF-16LE 三份夹具，比对 C# 读出的值
  （及 `login_server.rs:481` 选服帧字节）与 Rust；红检：去掉 BOM 分支必红。
- **附带（低）**：`strings.rs:187` 与 `data/lists.rs:107,194,229,259,311,390,441` 用
  `str::lines()`，不按孤立 `\r` 切行；`ReadLine` 把 `\r`/`\n`/`\r\n` 都当行终止。639 个 Envir
  文件实测 **0 个孤立 CR**（§4.1），风险低；统一方案一并改用 `split_lines`。
- **tier / 可达性**：**tier=L**；影响中。1357 个部署文本里共 4 个 UTF-8 BOM：
  `M2GameSvr/Server.conf`、`DBServer/dbsvr.conf`、`CloudGate/config.conf`、`Mir200/Server.conf`，
  均不在 E-6 的加载器面；`String.conf`/`AddrTable`/`UserLimit`/`LoginGate`/`SelGate`/`RunGate`
  的配置均无 BOM ⇒ 当前部署 0 命中。

### E-7　DB 名字/键：`ChrName` 是 utf8 列，不要用 MySQL `CONVERT(… USING gbk)`

- **oracle 依据**
  - `src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Add.cs:107`：`@ChrName` 绑 **string**；
    `PlayRecordStorage.cs:267,287`：`characters_indexes` 的 `@ChrName` 也绑 **string**。
  - `PlayDataStorage.Query.cs:23`：`Dictionary<string,int>(StringComparer.OrdinalIgnoreCase)`；
    `:55,58`：`dr.GetString("ChrName")` → 内存键；`:84-90` `Index(string)`。
  - 列本体（只读 `information_schema` 实测）：`characters.ChrName` = `varchar utf8mb3_general_ci`；
    `characters_indexes.ChrName` = `varchar utf8mb3_general_ci`。**DB 路径没有 GBK**。
- **Rust 现状**
  - `bins/db-srv/src/db.rs:140,241`：`CAST(CONVERT(ChrName USING gbk) AS BINARY) AS name_gbk`；
    `db.rs:334,402,627`：`CONVERT(? USING gbk)` 绑定 **GBK 二进制**；
    `playdata.rs:1072-1080`：`SELECT CAST(CONVERT(? USING gbk) AS BINARY)`（1100 取档帧）；
    内存键 `fold_key(&name_gbk)`（`db.rs:205,254,322,345,380`）。
  - 风险：MySQL 的 `gbk` 字符集是**第三种映射表**；`A8BF`（cp936 ↔ `U+E7C8`，WHATWG ↔ `U+01F9`）
    在 MySQL 上的落地/查找语义未测。`db.rs:16-19` 注释「MySQL CONVERT ≡ .NET GetBytes」不成立。
  - 修正方向（评审第 4 条）：**列值绑定解码后的 String**（与 C# 一致），SQL 里删掉
    `CONVERT(… USING gbk)`；**内存 key 用 String + 已登记折叠（D-2/B-103）**，不要用 MySQL 生成的
    GBK 字节。1100 取档帧的 `ChrName` 是**线上字节**，应改用统一的本地 `cp936::encode`（E-1），
    与库可用性无关。
- **可机判判据（tier=R，必须真机窗口）**
  1. 隔离库中由 C# oracle 创建名字字节 `A8 BF`（以及 `U+01F9`、`U+FFFD` 对照）的角色，
     记录 `SELECT HEX(ChrName)`、`SELECT HEX(CONVERT(ChrName USING gbk))`、`SELECT Id`。
  2. Rust 用 String 绑定重建同一角色，`HEX(ChrName)` 与 oracle **逐字节相等**；
     5100 `Index/FindByAccount` 与 6000 `play_data_index_text` 命中**同一个 Id**。
  3. 名字大小写折叠与 D-2/B-103 同一口径；`characters_indexes` 与 `characters` 两表名字 String
     相等（不再经过 GBK 二进制中转）。
- **牙**：把 `db.rs` 的 `name_gbk` 键改回 `fold_key(name_gbk)`（或把绑定改回
  `CONVERT(? USING gbk)`）⇒ `A8 BF` 用例的 `HEX(ChrName)` 或 Id 命中必红。该用例必须走真实
  DB（R 窗口），不能只测 helper。
- **tier / 可达性**：**tier=R**（落库 + 字段级 diff）。现有数据 46,102 个文本值中 **0 个**
  含 cp936/WHATWG 编码分歧字符（§4.3），所以当前数据无差异；缺陷是**潜在的**，一旦出现
  `A8BF`/PUA 名字或 6000 键转换即触发。

### E-8　网关合成登录包：decode → re-encode，非法字节改变正文与 `PackLength`

- **oracle 依据**：`GameGate/Services/ClientSession.cs:1060-1101`（`GetString` 后按
  `string.Length` 门控）、`:1203-1204`（拼 `**{account}/{charName}/…` 后 `GetBytes`）。
- **Rust 现状**：生产编码点只有 `bins/game-gate/src/login.rs:89`（`synthetic_login_body` 内的
  `gbk::encode`）；`PackLength` 由 `main.rs:726` 的 `login_body.len()` 决定（body = `#0` +
  encode + `!`）。`:320` 在 `#[cfg(test)] mod tests`（`:161` 起）内，是测试里自证
  `PackLength = encodeLen + 3` 的代码，**不是生产编码点**。长度门用 `chars().count()`。
  畸形 GBK 时：① `[81 20]` C# 是 1 个 `?`，Rust 是 2 个字符 ⇒ 账号/角色名长度门可能一放一拒；
  ② 重编码 `?` vs `&#65533;`，正文与 `PackLength` 都变。
- **可机判判据**：在既有 oracle golden 登录帧上增加一条「账号含 `0x81 0x20`」的构造帧，断言
  `synthetic_login_body` 全字节与 `PackLength` 等于 oracle；另断言合法登录帧不变（已有 golden）。
- **牙**：把生产点 `login.rs:89` 改回 one-shot `encoding_rs::GBK.encode` ⇒ 构造帧的
  `PackLength` 必红（`:320` 只是测试自证式，不参与生产路径）；把 `chars().count()` 改成按字节/
  按 cp936 字符数（错误方向）⇒ 长度门用例必红。
- **tier / 可达性**：**tier=L**；影响中。冻结客户端只发合法 GBK，当前 golden 已覆盖；触发面是
  第三方/畸形登录帧。

---

## 4. 可达性证据（评审第 6 条）

### 4.1 639 个 Envir 文件（`E:/MirServer/M2GameSvr/Envir/**/*.txt`）

```text
files=639  bom=0  decode_diff_files=2  div_pair_files=2  div_pairs_distinct=2
pua_files=1  invalid_files=1  nonbmp_chars=0  lone_CR_files=0  0xFF_files=0
WHATWG_ENCODE_ERR_DISTINCT_CHARS=1 (U+E7C8)
```

| 文件 | 分歧 | 生产可达性 |
|---|---|---|
| `GuildRankNameFilter.txt` | `A8BF`：cp936 `U+E7C8` vs WHATWG `U+01F9` | **不可达**：C# 读取点 `Player/PlayObject.Base.cs:3427` 被注释；Rust 只在 `script-tool/src/gbkoverrides.rs:6` 注释中提到 |
| `QuestDiary/LEGM2.CC假人系统/假人充值抽签.txt` | `BC0D`：cp936 单 `?` vs WHATWG `?`+`CR`（多一行） | 走 **script** 解码器（cp936 精确，B-105），仅在 `#CALL` 时加载；不进 `envir::text` |

其余 637 个文件在 cp936 与 WHATWG 下逐字节解码相同；无 `0xFF`、无孤立 CR、无 PUA、无非 BMP。

### 4.2 1357 个部署文本文件（8 个服务目录的 `.txt/.conf/.ini/.cfg`）

```text
files scanned 1357   M2GameSvr 661 / Mir200 673 / DBServer 6 / LoginSrv 7 /
                     LoginGate 2 / SelGate 3 / RunGate 4 / CloudGate 1
hits 2   E:/MirServer/M2GameSvr/Envir/GuildRankNameFilter.txt  ['U+E7C8']
         E:/MirServer/Mir200/Envir/GuildRankNameFilter.txt     ['U+E7C8']  # 同内容客户端镜像
BOM (4，均 UTF-8): M2GameSvr/Server.conf、DBServer/dbsvr.conf、
                   CloudGate/config.conf、Mir200/Server.conf
```

→ 683 恰是前 6 个目录之和；新增的 CloudGate/Mir200 674 个文件只多出 `CloudGate/config.conf`、
`Mir200/Server.conf` 两个 BOM 和 `GuildRankNameFilter.txt` 的同一份镜像，**没有引入新的分歧字符**。
4 个 BOM 全在配置面，E-6 的加载器面（`String.conf`/`AddrTable.txt`/`UserLimit.txt`/三网关
`config.conf`）**都没有 BOM** ⇒ E-6 当前部署 0 命中。除 `GuildRankNameFilter.txt` 的两份副本外，
没有字符会让 cp936 编码 ≠ WHATWG 编码。

### 4.3 DB 文本列（只读 `SELECT DISTINCT`，无写入）

| 库/列 | 值数 | 编码分歧值 |
|---|---|---|
| `mir2_db.characters`（LoginID/ChrName/MapName/HomeMap/MasterName/DearName） | 14,682 | **0** |
| `mir2_db.characters_indexes`（Account/ChrName） | 14,667 | **0** |
| `mir2_data.stditems.Name` | 1,000 | **0** |
| `mir2_data.monsters.Name` | 705 | **0** |
| `mir2_data.magics.MagName/Descr` | 113 | **0** |
| `mir2_account.account.Account/PassWord` | 14,927 | **0** |
| `mir2_dbsrv6k_mkt_1352_0` + `_29292_0` `marketitems` | 8 | **0** |
| **合计** | **46,102** | **0** |

列类型实测：`characters.ChrName` / `characters_indexes.ChrName` = `utf8mb3_general_ci`；
`marketitems` 的 `MarketName/ItemName/SellName` = `latin1_swedish_ci`（值为 ASCII，未纳入 E-7 的
utf8 口径，见 §7-4）。

### 4.4 可达性结论

- **E-1/E-2**：影响高，**可达性未证**。现有 639 Envir + 1357 部署文件与 46,102 DB 值均未命中
  **生产可达面**；
  触发条件是畸形/第三方帧、或未来内容引入 cp936-only PUA / 不可映射字符。
- **E-4**：现网 0 命中（唯一分歧文件不进生产），但自定义 Envir 可触发。
- **E-5/E-6**：现网 `Server.conf`/`dbsvr.conf` 的 UTF-8 BOM 已被（或恰好被缺省值）处理；
  其它 BOM/GBK 配置部署会触发。
- **E-7**：现有名字 0 分歧；潜在触发（未来 `A8BF`/PUA 名字、6000 键）。

---

## 5. 最小统一方案（供实现线直接照做）

### 5.1 唯一编码模块

新增 `crates/shared/src/cp936.rs`（**不要放 `crates/protocol`**：接口冻结，且这不是协议层语义）：

```rust
pub fn decode(bytes: &[u8]) -> String;              // .NET cp936；前导吞任意后继；0x80→U+20AC；0xFF→U+F8F5
pub fn encode(s: &str) -> Vec<u8>;                  // 逐 UTF-16 码元查表；不可映射→0x3F；绝不输出 &#N;
pub fn decode_string_list(bytes: &[u8]) -> String;  // BOM 顺序 + 上述 decode
pub fn split_lines(text: &str) -> Vec<String>;      // StreamReader.ReadLine：\r\n / \r / \n
```

- `decode` = 现 `crates/script/src/textfile.rs::decode_gb2312` + `gbk_overrides.rs`（7032 条）
  **原样搬迁**；`script::textfile` 改为 `pub use`/委托（B-105 的穷举门禁保留，不得降级）。
- `decode_string_list` 的 BOM 顺序（§2.2）：
  1. `EF BB BF` → UTF-8；
  2. `FF FE 00 00` → UTF-32LE（**必须排在 `FF FE` 前**）；
  3. `FF FE` → UTF-16LE；
  4. `FE FF` → UTF-16BE；
  5. `00 00 FE FF` → UTF-32BE；
  6. 其余 → `decode`。
  UTF-8/16/32 分支的非法序列用 `U+FFFD`；只有 cp936 分支用 `?`（B-105）。
- `encode` 的数据结构建议：生成 `cp936_encode.bin`（65,536 × `u16`，大端存 1/2 字节结果，
  `0xFFFF` = 不可映射），`include_bytes!` 进模块；或生成 `&[(u16,u16)]` 排序表 + 二分。
  表由 .NET 探测表机械生成，**不手抄**。

### 5.2 编码表生成（本机已实跑，落库为工具）

1. 给现有 `mir2-rs/tools/gbk-probe` 增加 `encode <out>` 子命令：对 BMP 0x0000..0xFFFF（跳过代理）
   逐字符 `Encoding.GetEncoding(936).GetBytes`，输出 `XXXX\tHEX`（与 §1.1 相同）。
   ```powershell
   dotnet run --project mir2-rs/tools/gbk-probe -- encode target/cp936-encode.txt
   ```
2. 给 `mir2-rs/crates/script-tool` 增加 `gen-cp936-encode --probe <file> --out <rs|bin>`：
   - 读探测表 → 65,536 项数组；不可映射/99 个 WHATWG-only 写 `0xFFFF` 哨兵；
   - 生成文件头写明 `codepage=936, InternalEncoderBestFitFallback`、源文件 SHA-256、生成命令；
   - 同时生成一份 `tests/vectors/cp936-encode.json`（挑 1,000 个代表点 + 全部边界向量）。
3. 门禁：`cargo test` 里加「生成表 vs 运行表」逐项相等；表文件缺失/哈希不符 ⇒ fail-closed。

### 5.3 回退策略（一句话）

**编码：逐 UTF-16 码元查 cp936 表；命中→字节；未命中（含全部 38,907 个双不可映射、99 个
WHATWG-only、代理码元）→ 单个 `0x3F`。绝不输出 `&#N;`，绝不 panic。**
**解码：cp936 分支前导吞任意后继、非法/末位给 `?`；BOM 分支非法给 `U+FFFD`。**

### 5.4 调用点清单（生产面；测试/夹具行不列）

**解码 → `cp936::decode` 或 `decode_string_list`**

| 模块 | 位置 | 用途 |
|---|---|---|
| game-svr | `main.rs:17474`（`gbk_decode`）、`17301`、`17457` | CM_SAY/命令尾/认证文本 |
| game-svr | `ids.rs:219` | IDS 会话通知 |
| game-svr | `strings.rs:183` | `String.conf`（需 BOM） |
| game-gate / login-gate / sel-gate | `gbk.rs:11`（各） | 网关文本/配置 |
| login-srv | `gbk.rs:14` | `logsrv.conf`/`AddrTable`/`UserLimit`/`ServerAddr`（需 BOM） |
| db-srv | `gbk.rs:181-190`（`decode`） | LoginSrv/DBSrv 文本、1100 名 |
| data | `lists.rs:495`、`monitems.rs:27`、`guild.rs:450,492`、`exps.rs:58` | 列表/行会/经验配置（需 BOM） |
| envir | `text.rs:23` | 全部 Envir 文本（需 BOM） |
| script | `textfile.rs:62` | 脚本逐对解码（改为委托，保留 7032 表） |
| config | `game-svr/config.rs:333`、`db-srv/config.rs:125`、各网关 `config.rs`、`login-srv/config.rs` | 统一用 `decode_string_list`（GBK/UTF-16/UTF-32 都可读） |

**编码 → `cp936::encode`**

| 模块 | 位置 | 用途 |
|---|---|---|
| game-svr | `downlink.rs:69`、`npc.rs:65`、`itemwire.rs:65`、`main.rs:1275,1345,17481,3136` | 下行文本/说词/物品名/聊天 |
| game-gate / login-gate / sel-gate | `gbk.rs:22`（各） | 登录/选服合成包 |
| login-srv | `gbk.rs:21` | 选服/账号包 |
| db-srv | `gbk.rs:187` | 1100 取档帧名下字节（本地编码，不查库） |
| data | `guild.rs:639`、`exps.rs:52` | 行会文件写回、配置重编码 |
| envir | `conf.rs:217` | `Server.conf` 解码后重编码 |
| script | `textfile.rs:157` | `SaveToFile` |

**DB（E-7）**

- `db.rs:140,241`：删 `CAST(CONVERT(ChrName USING gbk) AS BINARY)`，读 `text_col(row,"ChrName")`。
- `db.rs:334,402,627`：`CONVERT(? USING gbk)` → 普通 `?`，绑定 `&str`。
- `playdata.rs:1072-1080`：`gbk_bytes` 改为 `cp936::encode`（本地），不再 `SELECT CONVERT`。
- 内存键：`fold_text(&name_text)`（String + D-2/B-103 折叠），删除 `fold_key(name_gbk)` 的键路径；
  `name_gbk` 只在需要线上字节的出口保留（由 `cp936::encode` 生成）。

### 5.5 结构守卫（真牙，不是文本断言）

- 把 `encoding_rs` 从**除 `crates/shared` 外**所有 crate 的 `Cargo.toml` 移除（或用 workspace
  `public = false` 的私有依赖）；任何 crate 再直接调 `encoding_rs::GBK` **编译失败**。
  这比 grep 注释更硬，也避免「不剥注释/字符串」的假牙。
- DB：`CONVERT(… USING gbk)` 只允许出现在 `tests/parity` 的**反例夹具**里；生产 SQL 常量由
  R 窗口的 `A8 BF` 用例钉住（不是文本 grep）。
- BOM/边界：`tests/vectors/cp936-*.json` 由生成器产出，门禁比对哈希。

### 5.6 迁移顺序（每步可单独回归）

1. **只加模块/表/生成器/单测**（不改调用点；B-105 穷举门禁保持绿）。
2. 切**解码**调用点（合法 GBK 字节不变；顺带修 UTF-32LE/`FF FE FF`/UTF-32BE/`FE FF`）。
3. 切**编码**调用点（E-1/E-2/E-8）。
4. 修 **E-3** 边界 + `arrest`（向量 + 5100 请求级用例）。
5. 修 **E-5/E-6** 配置 BOM（先统一读函数，再逐文件回归）。
6. **E-7** 在 R 窗口做（隔离库 + oracle 对照），不与 L 级片混。

---

## 6. 已登记项的正确引用（不要重复上报）

- **B-105**（`whitelist.md:98`）：cp936 解码的 **7032 条**差异、101 个 PUA、`0x80/0xFF`、BOM 模型、
  `A8BF` 语料命中，`gbk_overrides.rs` + `textfile.rs` 状态机，穷举门禁 32,256 对。
  **E-2/E-4 均引 B-105**：E-2 是「B-105 的解码表 + WHATWG 编码器」错配；E-4 是「B-105 的实现
  未被其它加载器共用」。E-4 不重报 7032 条本身。
  ⚠️ B-105 的「`FF FE 00 00` 亦然（UTF-16LE）」与本机 .NET 实测不符，应按 §2.2 订正为 UTF-32LE。
- **A-14**（`whitelist.md:78`）：账号/口令/服务名/角色名域用 `encoding_rs::GBK` 近似 cp936，
  **不是 decode-only**——正文已含 DBSrv 1100 取档帧的 `HUtil32.GetBytes(chrName)`。E-1/E-2 与
  A-14 同族；本版把 E-1/E-2 定位为该近似的**编码侧可观测后果 + 统一修法**，不是新差异类别。
- **D-2 / B-103**（`whitelist.md:18/99`）：`OrdinalIgnoreCase`/`ToUpper` 折叠。E-7 的内存 key
  明确复用该口径，不另立条目。
- **B-1**：现网 `Server.conf` 的 UTF-8 BOM 被剥（登记于 `docs/M2验收真机报告-20261011-3.md`、
  `docs/M2顺序判据②-对账台账.md §4.7`；**勿引 `whitelist.md:33`**，那里的 B-1 是
  `MonGetRandomItems` 的 `itemName` 缺陷）；E-5 是「GBK/UTF-16 配置被严格 UTF-8 拒绝」，
  不是 B-1 的重复。
- **B-104**：639 vs 634 文件集口径；本审计的 639 指 `M2GameSvr/Envir` 递归 `.txt`，与 B-104 一致。

---

## 7. 未决问题

1. **B-105 的 BOM 条目需订正**：`FF FE 00 00` 按 .NET `StreamReader` 实测是 **UTF-32LE**，
   不是 UTF-16LE。建议 B 线在 whitelist 补一句「本行以 audit-encoding-v2 §2.2 为准」，并把
   `textfile.rs` 的 BOM 顺序修正（E-4）。本审计不改 whitelist（归 B 线）。
2. **MySQL `gbk` 表与 cp936 的具体分歧**：`A8BF`、`U+01F9`、`U+E7C8` 在 MariaDB 10.11 的
   `CONVERT(… USING gbk)` 下落到哪个码点/是否报错，需 R 窗口实测。E-7 的修法（绑 String）使该
   问题不再影响正确性，但订正前的行为记录仍应补。
3. **`dbsvr.conf` 的 UTF-8 BOM**：`db-srv/src/config.rs:125` 用 `fs::read` 自写 INI，未剥 BOM，
   首行 `[DataBase]` 丢失；现网值恰好等于代码缺省值，所以未暴露。建议并入 E-5/E-6 的
   `decode_string_list` 改造，并加一条「首行段名带 BOM」夹具。
4. **`marketitems` 是 latin1 列**（`mir2_dbsrv6k_mkt_*`）：当前值 ASCII；若未来写入非 ASCII
   物品名，会走第四种编码口径。本审计未把它并入 E-7（E-7 限定 utf8 的 `ChrName`），建议单列
   一条待评估项。
5. **oracle 运行时复测**：本机用的是 dotnet 9 + CodePages 8.0.0；oracle 是 net8。Windows 上
   两者都落 NLS cp936，但按 v1 的约定，E-7 的落库差异仍应在 R 窗口用 oracle 程序集复测一次
   （不是本审计的 L 级范围）。
6. **冻结客户端是否会发出 cp936-only PUA 字节**（`A8BF` 等）：需要 Delphi 端输入法/剪贴板
   路径分析或抓包；本审计只能给语料结论（0 命中）。

---

## 8. 残留物

- `E:/tmp/enca-v2/encprobe-dotnet/`：.NET 9 探针（cp936 BMP 表、BOM、边界/Split）。
- `E:/tmp/enca-v2/encprobe-rust/`：`encoding_rs` 对照与 639 文件扫描。
- `E:/tmp/enca-v2/spanprobe/`：`#[path]` 直接引用 `db-srv/src/gbk.rs` 的边界/`arrest` 探针。
- `E:/tmp/enca-v2/dbprobe/`：`MySqlConnector 2.3.5` 只读 DB 探针。
- `E:/tmp/enca-v2/*.txt`：`cp936-encode-bmp.txt`、`whatwg-encode-bmp.txt`、
  `pair-diff-full.txt`、`envir-scan.txt`、`db-*.txt`、`encode-diff-only-{whatwg,cp936}.txt`。
- 以上为证据工件，保留可复现；**未改动仓库代码/配置，未访问 `E:/MirServer` 的写路径，未起任何
  服务端/客户端/代理，未写数据库，未占端口**。
- 仓库内残留：本文件 + v1 顶部一行标注；无其它改动。
