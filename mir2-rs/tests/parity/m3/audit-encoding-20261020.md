# OpenMir2 全仓文本编码面审计（对 oracle C#）

- 基线：`master` @ `3bdfa0888`（只读审计；未建 worktree、未改仓库、未起进程）
- 审计日期：2026-10-20
- 探测工件：`E:/tmp/encprobe/`（Rust，encoding_rs 0.8，offline）、`E:/tmp/encprobe.ps1`、`E:/tmp/encprobe2.ps1`、`E:/tmp/encprobe3.ps1`（.NET cp936）
- 已登记口径只引用、不计为新发现：`mir2-rs/tests/parity/whitelist.md:78` **A-14**（账号/口令/服务名/角色名域 `encoding_rs::GBK` ≈ .NET cp936；PUA 少量字节对映射、非法字节回退 `U+FFFD` vs `?`）、`whitelist.md:18/99` **D-2 / B-103**（OrdinalIgnoreCase / ToUpper 折叠）、B-1（现网 `Server.conf` 的 UTF-8 BOM）。

## 摘要（≤1000 字）

未发现“把 UTF-8 字节直接写上线”的路径：所有 s2c 文本帧最终都经 GBK 编码（`edcode::encode` 前），网关 s2c 中继是逐字节透传。真正的问题是**同一个 oracle 语义在仓内被多种实现分裂**，其中两处是硬伤：

1. `encoding_rs::GBK.encode` 对不可映射字符输出 HTML 数字字符引用 `&#NNNN;`（如 U+FFFD→`&#65533;`、U+E7C8→`&#59336;`），而 oracle `HUtil32.GetBytes`（cp936）输出 `?` 或可逆映射（U+E7C8→`A8 BF`）。该 one-shot 编码遍布 game-svr / db-srv / 三网关 / login-srv，且不检查 `had_errors`。任何 `U+FFFD`（非法 GBK 输入）或 GBK 域外字符都会改变帧长与字节。
2. 脚本解码器为对齐 cp936 收了 101 个“非 `?`”覆盖（`A8BF→U+E7C8` 等），但编码器是 WHATWG GBK，无法反写这些字符。**合法 GBK 字节 `A8 BF`** 经脚本 → NPC 说词（`main.rs:4486`）会变成 8 字节 ASCII `&#59336;`；C# 则逐字节回 `A8 BF`。脚本 `SaveToFile` 也把 `U+E7C8` 写成 `3F`，丢字节。

其余：DBSrv 字节级字符边界对“前导 + 非法后继”与 cp936 不一致（`[81 20 41]`：.NET 2 字符，Rust `char_spans` 3）；envir 与 script 两套 StringList 解码器不同（0xFF、BOM、覆盖表）；配置读取器口径互不一致（`Server.conf` 要求严格 UTF-8；`String.conf`/`AddrTable`/`UserLimit`/三网关只按 GBK、不认 BOM）；DB 6000 面用 MySQL `CONVERT(... USING gbk)` 做 GBK（第三种映射表）。A-14 只覆盖 decode 侧的近似，上述 encode 侧与实现分裂不在其内。

## 0. 探测证据（两侧实测）

C# 侧（Windows PowerShell 5.1，内置 cp936；oracle 为 net8 + `CodePagesEncodingProvider`，Windows 上同样落到 NLS cp936）：

```
enc U+FFFD -> 3F                    enc U+E7C8 -> A8 BF      enc U+20AC -> 80
enc U+2018 -> A1 AE                 enc U+00A0 -> 3F         enc U+1F600 -> 3F 3F
dec [81 20] -> len=1 U+003F         dec [81 2F] -> len=1 U+003F
dec [81 20 20 41] -> len=3 [003F 0020 0041]
dec [A8 BF] -> U+E7C8               dec [FF] -> U+F8F5       dec [81 41] -> U+4E04
```

Rust 侧（`encoding_rs` 0.8，`E:/tmp/encprobe`）：

```
enc U+FFFD -> 26 23 36 35 35 33 33 3B  ("&#65533;")   enc U+E7C8 -> "&#59336;"
enc U+00A0 -> "&#160;"                                enc U+1F600 -> "&#128512;"
enc U+01F9 -> A8 BF (WHATWG 可逆)                      enc U+E4C5 -> FE FE
dec [81 20] -> len=2 [U+FFFD, U+0020]                 dec [81 2F] -> len=2 [U+FFFD, U+002F]
dec [FF] -> U+FFFD                                    dec [81 41] -> U+4E04
```

> 注意：PowerShell 5.1 注册 `CodePagesEncodingProvider` 失败（类型不存在），但 .NET Framework 自带 cp936；net8 的 provider 在 Windows 上也使用同一 NLS 表。E-7（落库）仍建议在 R 窗口用 oracle 运行时复测一次。

## 1. 逐点对照表

### 1.1 服务端上/下行帧里的文本

| # | 语义 | oracle 行 | Rust 行 | 编码器 | 逐字节一致？ | 风险 |
|---|---|---|---|---|---|---|
| 1 | `EDCode.EncodeString` 的输入 | `src/OpenMir2/EDcode.cs:175-184` → `HUtil32.GetBytes` `HUtil32.cs:689-697` | `crates/protocol/src/edcode.rs:1-12`（只处理字节）；调用方各自 `GBK.encode` | cp936 vs WHATWG GBK | 合法 cp936 文本：是；含 `U+FFFD`/域外字符：否（长度也变） | **高（E-1）** |
| 2 | 客户端包内 Pascal 字符串 | `Packets/ClientPackets/ClientPacket.cs:22-31`（gb2312 GetString） | 网关不解文本、原样转 game-svr；`main.rs:17067 gbk_decode` | cp936 vs WHATWG GBK | 合法：是；非法：否（A-14 已登记 decode 替换符差） | 中 |
| 3 | 网关合成登录包 | `GameGate/Services/ClientSession.cs:1060-1224`（GetString→拼串→GetBytes） | `game-gate/src/login.rs:89,320`（decode→encode，320 还按重编码算长度） | cp936 vs WHATWG GBK | 正常登录：是（已有 golden）；非法字节：否 | 中（E-8） |
| 4 | 网关 s2c 中继 | `ClientSession.cs:897-920` | `game-gate/src/main.rs`（不 decode 业务文本） | 无（字节透传） | 是（负结果） | 低 |
| 5 | DBSrv `DBR_LOADHUMANRCD` 名字帧 | `DBSrv/Services/Impl/DataService.cs:192`（GetBytes） | `bins/db-srv/src/data.rs:309`（`gbk::encode`） | one-shot GBK | 常用汉字：是；`U+FFFD`/PUA：否 | 中（E-1） |

### 1.2 脚本 / 配置 / 数据文件读取

| # | 语义 | oracle 行 | Rust 行 | 编码器 | 一致？ | 风险 |
|---|---|---|---|---|---|---|
| 1 | 脚本 `StringList.LoadFromFile` | `Common/StringList.cs:276-291`（默认 gb2312）+ `293-360` + StreamReader BOM | `crates/script/src/textfile.rs:12,22-66,78-115` | cp936 状态机 + 7032 覆盖表 | 是（探测表 32256 对、0 条多字符，覆盖表 7032 条；单测锚定） | 低 |
| 2 | Envir 文本（MapInfo/Monster/Merchant/Castle/NPC 位置/GameSvrConf…） | 同上 StringList | `crates/envir/src/text.rs:9-24` | **裸 WHATWG GBK**，无覆盖表 | 否：0xFF、101 个 PUA 对、`FF FE FF`、`00 00 FE FF` | 中（E-4） |
| 3 | `Server.conf`（网关链路） | `Common/ConfigFile.cs:263`（GB2312 + BOM 探测） | `bins/game-svr/src/config.rs:301-312`（`read_to_string` 严格 UTF-8 + 剥首 BOM） | UTF-8 only | GBK/UTF-16 配置：**启动失败**；现网 UTF-8 BOM：是 | 中（E-5） |
| 4 | `Server.conf`（GameSvrConf） | 同上 | `crates/envir/src/conf.rs:211-222`（decode_text→GBK 重编码→IniFile） | 混合 | BOM 常见中文：是；域外字符：`&#N;` 注入 | 中（E-6） |
| 5 | `string.conf`（客户端可见） | `SystemShare.cs:187` + `StringConf.cs:7` 继承 ConfigFile | `bins/game-svr/src/strings.rs:183`（裸 GBK，不认 BOM） | WHATWG GBK | UTF-8 BOM 文件：否（乱码） | 中（E-6） |
| 6 | 三网关 `config.conf` | `GameGate/LoginGate/SelGate Conf/ConfigManager : ConfigFile` | `game-gate/src/config.rs:95`、`login-gate/src/config.rs:56`、`sel-gate/src/config.rs:56`（裸 GBK） | WHATWG GBK | BOM 文件：否 | 中（E-6） |
| 7 | LoginSrv `logsrv.conf`/`AddrTable.txt`/`UserLimit.txt`/`ServerAddr.txt` | `ConfigFile.cs:263`；`LoginSrv/Conf/ConfigManager.cs:100-127`（StringList） | `bins/login-srv/src/config.rs:132,165,235,259`（裸 GBK） | WHATWG GBK | 无 BOM GBK：是；BOM：否 | 中（E-6） |
| 8 | `crates/data` 列表（ItemNameList/MonItems/GuildList/…） | `StringList.cs:276-291` | `crates/data/src/lists.rs:18-21,487-497`、`monitems.rs:27`、`guild.rs:449-463,492`（裸 GBK；文档声称 BOM 但未实现） | WHATWG GBK | 无 BOM：是；BOM：否 | 低（E-6） |
| 9 | `Exps.conf` / `IniFile` | `ConfigFile.cs:263` | `crates/data/src/exps.rs:33-57`（decode→**GBK 重编码**→解析） | 混合 | 域外字符：否（`&#N;` 进入值） | 低（E-6） |
| 10 | 脚本/列表写回 `SaveToFile` | `StringList.cs:260-274`（GetBytes cp936） | `script/textfile.rs:153-163`（逐字符，不可映射→`?`） | cp936 解码 + WHATWG 编码 | 101 个 cp936-only 字符：**否**（C# 回 A8BF，Rust 写 3F） | 中（E-2） |

### 1.3 数据库 varchar（角色名/物品名/地图名）

| # | 路径 | oracle | Rust | 一致？ | 风险 |
|---|---|---|---|---|---|
| 1 | 列值本体（utf8 列） | `dr.GetString` / 参数 string（`PlayRecordStorage.cs:60,287`、`PlayDataStorage.Add.cs:94-107`） | sqlx `String` / `_text` | 是（两侧都走 utf8 连接字符集） | 低 |
| 2 | 6000 面 name→GBK 键 | `HUtil32.GetBytes`（cp936） | `bins/db-srv/src/playdata.rs:1072-1080` `CONVERT(? USING gbk)` | 常用汉字：是；PUA/非法：**否（MySQL gbk 表 ≠ cp936）** | **R（E-7）** |
| 3 | 5100 面 `characters_indexes` 写入/读键 | `@ChrName` 字符串 | `bins/db-srv/src/db.rs:333-334,402` 绑定 **GBK 二进制** + `CONVERT(? USING gbk)`；读侧 `INDEX_COLS` `CONVERT(ChrName USING gbk)` | 常用汉字：是；A8BF 类：否 | **R（E-7）** |
| 4 | 大小写折叠 | `StringComparer.OrdinalIgnoreCase` | `gbk::fold_key`（ASCII）+ `to_lowercase` | 现网语料等价 | 已登记 D-2/B-103 |
| 5 | 日志文本 | — | `db.rs:228,353` `from_utf8_lossy(gbk)`、`handler.rs:257` | 仅日志 | 低 |

### 1.4 客户端可见文本端到端

| 文本 | 生产链 | 编码口径 | 一致？ | 风险 |
|---|---|---|---|---|
| 普通聊天 `SM_HEAR` | 客户端 GBK → 网关透传 → `main.rs:17067 gbk_decode` → `handle_say` `main.rs:1257 gbk_encode` → `sm_hear` → `edcode::encode` | cp936→String→WHATWG | 合法：是；含 0x81 孤立/非法对：**否**（`?` vs `&#65533;`，帧长变） | **高（E-1）** |
| 私聊 `SM_WHISPER` | `chat::parse_whisper`→`handle_whisper` | 同上 | 同上 | 高（E-1） |
| NPC 说词 `SM_MERCHANTSAY` | 脚本（cp936 解码）→ `main.rs:4486 npc::merchant_say_gbk` → `sm_merchantsay` | cp936→String→WHATWG | **合法 GBK `A8 BF`→`&#59336;`（C#→`A8 BF`）** | **高（E-2）** |
| 公告 `SM_SENDNOTICE` | `Notice.txt`（envir 解码）→ `notice.rs/line_notice.rs` → `enter::gbk` | 裸 GBK→WHATWG | 常见中文：是；0xFF/PUA：否 | 中（E-1/E-4） |
| 系统消息 `SM_SYSMESSAGE` | `string.conf`→`strings.rs`；或源码常量 | 裸 GBK→WHATWG | 常见中文：是；BOM 文件：否 | 中（E-6） |
| 行会 750/753/756/104 | `guild*.rs` → `downlink::gbk` | 裸 GBK→WHATWG | 常见中文：是；PUA：否 | 中（E-1） |
| `ClientItem` 物品名（14B） | `stditems`（sqlx String）→ `itemwire.rs:63-80` | WHATWG | 常见中文：是；不可映射：长度/截断分支不同（C# 3F 短，Rust `&#N;` 长） | 中（E-1） |

**负结果（不是发现）**：网关 s2c 中继不改文本（`game-gate/src/main.rs` 只对登录/速度规则做 decode）；`frame.rs:389` 的 `from_utf8` 只解析 `+GD/<digits>` ASCII；DB 列值本体两侧都是 utf8；`WriteAsciiString` 的“按字节截断可切半个汉字”在 C#（`BinaryWriterExtension.cs:30-37`）与 Rust（`itemwire.rs:63-80`）**行为相同**。

## 2. 可派工发现（8 条，含 tier 与判据草案）

### E-1（高）one-shot `GBK.encode` 的回退是 `&#NNNN;`，oracle 是 `?`/可逆映射
- **oracle**：`HUtil32.cs:689-697,758-760`（cp936 编码器，默认 ReplacementFallback=`?`，best-fit 如 U+2018→A1AE）。
- **Rust**：`bins/game-svr/src/main.rs:17074`、`downlink.rs:69-71`、`npc.rs:63-66`、`itemwire.rs:63-80`、`guild_frames.rs:63`、`bins/db-srv/src/gbk.rs:186-189`、`bins/game-gate/src/gbk.rs:24-28`、`login-gate/gbk.rs`、`sel-gate/gbk.rs`、`bins/login-srv/src/gbk.rs:20`、`crates/data/src/{guild.rs:636-643,exps.rs:52}`、`crates/envir/src/conf.rs:219` —— 全部 `let (bytes, _, _) = GBK.encode(...)`，**不检查 `had_errors`**。
- **判据草案（tier=L；不触 RNG/时序/落库/战斗，纯逻辑可构造向量）**：
  1. oracle probe（net8 + CodePagesEncodingProvider）对 `{U+FFFD, U+00A0, U+E7C8, U+1F600, U+2018}` 的字节向量写入 `tests/vectors/gbk-encode.json`；Rust 单测逐字节等于该向量；红检：把回退改回 one-shot 必红。
  2. 构造 CM_SAY 帧，正文末尾放孤立 `0x81`（或 `0x81 0x20`）跑两侧 GameSvr，比对 `SM_HEAR` 尾（`ChrName:msg`）字节；要求 diff=0。
- **可达性**：普通客户端发的是合法 GBK，不会触发；触发面是①畸形/第三方客户端发的非法字节（CM_SAY/登录/交易等所有文本接点），②经 `envir` 裸 GBK 解码后含 `0xFF` 或 PUA 对的内容（再被 `gbk()` 重编码），③配置/脚本里的 GBK 域外字符（emoji 等）。
- **修复建议**：抽一个唯一的 `hutil_get_bytes(&str)->Vec<u8>`：逐字符 `GBK.encode` + `had_errors→b'?'` + 对 cp936-only PUA 先查反向覆盖表；所有助手改调它。

### E-2（高）cp936-only 字符“可解码不可编码”：合法 GBK 字节在说词/写回时损坏
- **oracle**：`GetString(A8BF)=U+E7C8`（`HUtil32.cs:183-205`）、`GetBytes(U+E7C8)=A8 BF`（`:689`）——可逆。
- **Rust**：`crates/script/src/textfile.rs:22-66` 的覆盖表把 `A8BF→U+E7C8`；但 `npc.rs:63-66`（生产接点 `main.rs:4486`）、`guild_frames.rs:63`、`textfile.rs:153-163` 都用 WHATWG 编码器：`U+E7C8` 不可映射。
- **实测**：Rust `enc U+E7C8 → "&#59336;"`；`textfile::encode_gb2312` 则写 `3F`。
- **判据草案（tier=L）**：夹具脚本含真实字节 `A8 BF`（该对已被 `script-tool/src/gbkoverrides.rs:6` 记为现网语料命中）；跑 `GotoLable`，比对 `SM_MERCHANTSAY` 尾与 oracle 逐字节；再跑一次脚本 `SaveToFile` 往返，断言字节不变。红检：把覆盖表对应项删掉必红（解码侧），把编码器换成 cp936 反向表必绿。
- **可达性/证据边界**：`A8 BF` 是**合法 cp936 字节对**，解码→编码在 oracle 可逆、在 Rust 不可逆，这是确定缺陷；现网脚本语料是否真的含这对字节尚未证（唯一已知命中 `GuildRankNameFilter.txt` 的 C# 读取点 `PlayObject.Base.cs:3427` 被注释掉）。因此本条先用构造向量判据，不宣称现网必然触发。
- **修复建议**：编码器增加 `U+E000..U+F8FF` 反向覆盖（由现有 101 条 `cp936 对→PUA` 表反转生成）；`encode_gb2312` 共用同一实现。

### E-3（中）DBSrv 字节级字符边界与 cp936 不符（文档断言错误）
- **oracle 实测**：`GetString([81 20 41])` = `"?A"`（2 字符；实测后继 `0x20/0x2F/0x7F/0x00/0xFF` 均被成对消耗）；`[81 20 20 41]` = 3 字符。Rust `char_spans([81 20 20 41])` = 4 个 span。
- **Rust**：`bins/db-srv/src/gbk.rs:29-57`（`is_trail` 只认 `0x40..=0xFE` 非 `0x7F`）及其注释“非法序列按单字节处理，字符数一致”。
- **调用点**：`handler.rs:419`（名字长度 ≥3）、`loginsrv.rs:187-195`（`split2_nonempty` 逐段）、`route.rs:153-223`、`server.rs:251,326`。
- **影响**：畸形 GBK 中“前导字节紧贴分隔符”（如 `aaa 81 2F bbb`）时，C# 把 `81 2F` 当 1 个 `?`（分隔符被吞），Rust 仍按 `/` 切开 ⇒ 账号/名字/路由解析不同。
- **判据草案（tier=L）**：把 oracle 的 `string.Length`/`Split` 结果做成向量表（≥20 组，含 `81 20`、`81 2F`、`81 7F`、`81 00`、`81 FF`、`FF`、`80`）；Rust `char_spans/char_count/split2_nonempty` 全等；再加一条 5100 请求级用例。红检：把 `is_lead`/`is_trail` 改回“合法后继才成对”必红。
- **修复建议**：`char_spans` 改为 .NET 语义——前导字节后**只要有下一字节就消费 2 字节**（`0x80/0xFF` 单字节）；或直接复用 `script::textfile` 的 cp936 状态机。

### E-4（中）两套 StringList 解码器：`envir` 与 `script` 对同一 oracle 语义结果不同
- **oracle**：`StringList.cs:276-291` + `293-360`（GetEncoding）+ `StreamReader` 默认 BOM 探测（UTF-8/UTF-16LE/BE/UTF-32BE）。
- **Rust-A**：`crates/envir/src/text.rs:9-24` —— 裸 `encoding_rs::GBK`；无覆盖表；`0xFF→U+FFFD`（C# `U+F8F5`）；`FF FE FF` 落到 GBK（C# 按 UTF-16LE，实测 `U+41FF`）；无 `00 00 FE FF`（UTF-32BE）。
- **Rust-B**：`crates/script/src/textfile.rs:12,22-115` —— cp936 状态机 + 覆盖表 + `0xFF→U+F8F5` + `FF FE FF`/UTF-32BE。
- **判据草案（tier=L）**：固定向量 8 条（`FF`、`80`、`A8 BF`、`81 20`、`FF FE FF 41`、`00 00 FE FF 00 00 00 41`、UTF-8 BOM+`FF`、GBK 普通中文）+ 现网 639 文件**逐文件解码哈希**，要求 envir/script/oracle 三者一致（或显式登记差异）。**建议直接让 `envir::text` 调用 `script::textfile::decode_bytes` 的同一实现**（把解码器下沉到共享 crate）。
- 备注：A-14 登记的是“GBK 近似 cp936”的域，不覆盖“同一仓内两套近似”和 `0xFF`/`FF FE FF`/UTF-32BE 这三个具体分支。

### E-5（中）`Server.conf` 被两条读取链用不同编码读，且网关链路要求严格 UTF-8
- **oracle**：`ConfigFile.cs:263` `StreamReader(..., GB2312)`，BOM 探测；GBK 无 BOM/GBK+BOM/UTF-8 BOM/UTF-16 都能读。
- **Rust**：`bins/game-svr/src/config.rs:301-312` `std::fs::read_to_string`（严格 UTF-8）+ 剥首 U+FEFF ⇒ **GBK 或 UTF-16 的 `Server.conf` 直接启动失败**（oracle 正常）；同一文件随后 `crates/envir/src/conf.rs:211-222` 又用 decode_text（GBK/UTF-8）读，两条链口径不同。
- **判据草案（tier=L）**：4 份 `Server.conf` 变体（GBK 无 BOM / UTF-8 BOM 现网 / UTF-16LE BOM / 含 0xFF）经 C# harness 与 Rust 启动，断言“能启动 + `[DataBase]`/`[Server]` 取值 + 最终上线 `SM_*` 文本字节”一致；红检：把 `read_to_string` 改回 `fs::read`+`decode_text` 应绿。
- 备注：B-1 只登记“现网 UTF-8 BOM 被剥离”；本条的“oracle 也接受 GBK”未登记。

### E-6（中）`String.conf`/`AddrTable`/`UserLimit`/三网关配置/data 列表：裸 GBK、不认 BOM
- **oracle**：`StringList.cs:276-291`、`ConfigFile.cs:263` 都做 BOM 探测；`LoginSrv/Conf/ConfigManager.cs:112-115` 明确用 StringList 读 AddrTable。
- **Rust**：`strings.rs:183`、`login-srv/src/config.rs:132,165,235,259`、`game-gate/src/config.rs:95`、`login-gate/src/config.rs:56`、`sel-gate/src/config.rs:56`、`data/src/lists.rs:487-497`、`monitems.rs:27`、`guild.rs:449-463,492` —— 全部 `GBK.decode` 整文件，无 BOM 分支；`lists.rs:18-21` 文档还声称“有 BOM 时切 UTF-8/UTF-16”。
- **影响**：UTF-8 BOM 的 `string.conf` ⇒ `YouPoisoned` 等客户端文案乱码；BOM 的 AddrTable ⇒ 选服列表/服务名乱码或匹配失败；`crates/data` 的列表内容乱码。
- **判据草案（tier=L）**：每类文件做 GBK / UTF-8BOM / UTF-16LE 三份夹具，比对 C# 读出的值（及 `login_server.rs:481` 的选服帧字节）与 Rust；红检：去掉 BOM 分支必红。
- **附带（同族，低）**：`crates/data` 与 `envir` 多个加载器用 `str::lines()`，不按孤立 `\r` 切行，而 C# `StreamReader.ReadLine` 把 `\r`、`\n`、`\r\n` 都当行终止；现网无孤立 CR 文件，风险低。`script::textfile::split_lines` 已正确实现。

### E-7（R，落库）DB 名字/键的 GBK 转换由 MySQL 完成，是第三种映射表
- **oracle**：`HUtil32.GetBytes`（cp936）写字符串参数；`PlayRecordStorage.cs:60,287`、`PlayDataStorage.Add.cs:94-107`。
- **Rust**：`db-srv/src/playdata.rs:1072-1080`（`SELECT CAST(CONVERT(? USING gbk) AS BINARY)`）、`1122`；`db.rs:139-140`（读侧 `CAST(CONVERT(ChrName USING gbk) AS BINARY)`）、`333-334`（`INSERT ... CONVERT(? USING gbk)` 绑定 GBK 二进制）。
- **风险**：MySQL 的 `gbk` 字符集表与 .NET cp936 不是同一张表（第三种实现）。已知分歧点 `A8BF`：cp936 给 `U+E7C8`（可逆），WHATWG 给 `U+01F9`；MySQL 的 `gbk` 具体落到哪个码点/是否报错需在窗口实测。若 MySQL 给 U+01F9，C# 建号会把 `A8BF` 存成 U+E7C8 的 utf8，而 Rust 存成 U+01F9（或 `?`），两侧 DB 值不同。读侧同理：C# 存的 U+E7C8 名字，Rust 的 `INDEX_COLS` 转回 GBK 时可能得到 `?`，导致 5100 索引与 6000 文本键不一致。
- **判据草案（tier=R，必须真机窗口）**：在隔离库中由 C# oracle 创建名字 `A8 BF`（及 `U+FFFD`；4 字节 emoji 在 utf8mb3 列上两侧都应失败，另记）的角色，`SELECT HEX(ChrName)` / `HEX(CONVERT(ChrName USING gbk))` 存档；Rust 对同一字节流重建，逐字段 diff=0；再验证 5100 `Index/FindByAccount` 与 6000 `play_data_index_text` 对同一名命中一致。
- **修复建议**：Rust 侧统一用本地 cp936 反向表（E-2 的实现）产生 GBK 字节，绑定为二进制列值/键，不再依赖 MySQL 的 `gbk` 表；读侧对列值用 sqlx String（utf8）再本地转 GBK。

### E-8（中）网关合成登录包：decode→re-encode，非法字节会改变正文与 `PackLength`
- **oracle**：`GameGate/Services/ClientSession.cs:1060-1224`（`HUtil32.GetString` 解，拼 `**{account}/…`，`HUtil32.GetBytes` 编）。
- **Rust**：`game-gate/src/login.rs:89`（`gbk::encode(&text)`）、`:320`（`encode_len = edcode::encode(&gbk::encode(&text)).len()`）。
- **影响**：登录文本含非法 GBK 字节时有两层差异：① **字符数不同**（C# `GetString([81 20])`=1 个 `?`；Rust `gbk::decode`=2 个字符），`parse_client_login` 的 account/charName 长度门（4..12、2<len<15）可能一放一拒；② **重编码不同**（`?` vs `&#65533;`），合成包正文与 `PackLength` 随之不同。合法文本两侧逐字节一致（已有 golden 锚定）。
- **判据草案（tier=L）**：在既有 oracle golden 登录帧基础上加一条“账号含 `0x81 0x20`”的构造帧，比对 `synthetic_login_body` 全字节与 `PackLength`；红检：改回 one-shot 编码必红。
- 与 E-1 同根因，但这是唯一**用重编码结果计算长度**的接点，单列便于派工。

## 3. 已登记项的正确引用方式（不要重复上报为新发现）

- **A-14**：`encoding_rs::GBK` ≈ cp936 的 decode 近似（PUA 对、非法字节 `U+FFFD` vs `?`）。E-1/E-2 报的是 **encode 侧**（`&#NNNN;`、PUA 不可反写）与 **同一仓内解码器分裂**，不属于 A-14；E-3 的“字符边界/`Split`”也不是 A-14 的替换符差异。
- **D-2 / B-103**：`OrdinalIgnoreCase`/`ToUpper` 折叠，见 `db-srv/src/gbk.rs:174` 与 `data/src/lists.rs:269-270`；仅引用。
- **B-1**：现网 `Server.conf` 的 UTF-8 BOM 被剥；E-5 是“GBK 配置被严格 UTF-8 拒绝”，不是 B-1 的重复。

## 4. 范围覆盖确认（任务四项）

1. **上/下行帧文本**：已覆盖 EDCode 输入字节来源、网关、game-svr、db-srv（§1.1）。结论：无 UTF-8 直写；风险在编码器回退（E-1）与 re-encode 接点（E-8）。
2. **脚本/配置/数据文件 GBK 解码**：已覆盖 script、envir、data、game-svr/gates/login-srv 四条链（§1.2）。结论：四套口径，E-4/E-5/E-6。
3. **DB varchar 编码路径**：已覆盖列值读写、GBK 键转换、大小写折叠（§1.3）。结论：列值 utf8 等价；GBK 键经 MySQL 是第三种表（E-7，R）。
4. **客户端可见文本端到端**：已覆盖聊天/私聊/公告/NPC 说词/系统消息/行会/物品名（§1.4）。结论：普通文本字节等价；E-1/E-2 在异常字符与 cp936-only 字符上破坏字节与长度。

## 5. 建议的最小统一方案（供派工参考）

1. 新建共享编码模块（`crates/shared` 或 `crates/protocol`）：
   - `gbk_decode(bytes) -> String`：采用 `script::textfile::decode_gb2312` 的 cp936 状态机 + 覆盖表（含 0xFF/0x80）；
   - `gbk_encode(&str) -> Vec<u8>`：逐字符 cp936（含 PUA 反向表；不可映射→`?`），**永不**输出 `&#N;`；
   - `string_list_decode(bytes)`：decode + BOM 判定（UTF-8/UTF-16LE/BE/UTF-32BE）单一入口。
2. 所有 `encoding_rs::GBK.encode(...)` / `GBK.decode(...)` 直接调用点改为上述两函数；CI 用 `grep` 守卫禁止新增裸调用。
3. DB 侧改用本地 `gbk_encode`，去掉 `CONVERT(... USING gbk)`。

## 6. 残留物

- `E:/tmp/encprobe/`（Cargo 探测工程，encoding_rs 0.8）、`E:/tmp/encprobe.ps1`、`E:/tmp/encprobe2.ps1`、`E:/tmp/encprobe3.ps1`：保留作为证据，可复现上述向量。
- 未改动仓库；未访问 `E:/MirServer`；未起任何服务端/客户端。


---

## 评审员更正（2026-10-20，**以本节为准；request_changes**）

1. **已登记项引用错**：`whitelist.md:98` 的 **B-105** 已登记 cp936 vs WHATWG 的 **7032** 条解码差异、101 个 PUA、
   `0x80/0xFF`、BOM 模型与 `A8BF` 语料命中——E-2/E-4 必须引 B-105；`whitelist.md:78` 的 **A-14 不是 decode-only**
   （正文已含「DBSrv 1100 取档帧的 HUtil32.GetBytes(chrName) 走同一近似」）。
2. **E-4 的量级**：不是 101 对，是 **7032 对**，其中 **6931 对**是「.NET 单 `?` vs WHATWG 多字符」（改变字符数与行边界，
   如 `[81 0A]`）；必须补 **UTF-32LE**（`FF FE 00 00`），且 Rust-B 的 BOM 判断顺序（`FF FE` 先于 UTF-32LE）也错。
3. **E-1/E-2 的修复不完整**：还有 **99 个 WHATWG-only 字符**（.NET 编 `?`、encoding_rs 编回原字节）与非 BMP 的
   UTF-16 码元回退（`U+1F600 → 3F 3F` vs 逐 chars 只得 1 个 `?`）；需用 .NET 探测表生成编码表，不能只查 `had_errors`。
4. **E-7 的修复建议不成立**：`ChrName` 是 **utf8** 列，正确做法是列值绑定**解码后的 String**（与 C# 一致），
   内存 key 用 String + 已登记的折叠（D-2/B-103），不要用 MySQL 生成的 GBK 字节。
5. **E-3 需把 `db-srv/gbk.rs::arrest` 纳入同卡**（它仍是裸字节扫描）；`route.rs` 读的是本地 ASCII 配置，不是主要可达面。
6. 分级：E-1/E-2 的「高」与自身可达性描述自相矛盾，建议「影响高 / 可达性未证」并先扫 639 个 Envir 文件与 DB 列。
