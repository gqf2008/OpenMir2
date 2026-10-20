# HUtil32 全量 parity 清册与实现报告（2026-10-20）

- 车道：`hutil`（`hutil-parity` / worktree `/e/tmp/wt-hutil`）
- 基线：`master = c83a9ae24`
- oracle（只读）：`src/OpenMir2/HUtil32.cs`（866 行，`net8.0`）
- Rust 目标：`mir2-rs/crates/shared/src/hutil32.rs`（本批前 463 行 → 本批后约 1100 行）
- 配套：`mir2-rs/crates/shared/src/hutil.rs`（`round` 改为委托）

## 1. 结论（缺失 / 不等价项数）

oracle 共有 **76 个公共成员**（按重载逐个计）。

| 口径 | 数量 | 说明 |
|---|---:|---|
| 本批前共享层已有直接入口 | 14 | 其中 **2 个不等价**：`Round`（溢出饱和 vs `INT_MIN`）、`StrToInt`（Rust `trim()` 接受 NBSP/全角空格） |
| 本批后已覆盖且逐字节等价 | **54** | 新增 38 个公共函数 + 2 个常量 + 1 个错误枚举（共 41 个公共项）；4 个 `GetValidStr3` + 3 个 `GetValidStrCap` 重载分别收敛为 `&[char]` 入口 |
| 本批后仍未覆盖 | **22** | 12 个平台/类型不适用（不移植）+ **10 个 GB2312 族因 `shared` 零依赖契约延后**（bins 已有等价 gbk 模块） |
| 覆盖部分的不等价项 | **0** | 已覆盖成员全部按 §2 的 .NET 8 实测值对齐 |

**关键阻塞（未做项）**：任务要求补 `GetBytes`/`GetString`/`GetByteCount`/`StringToByte`/`StringToByteAry`。
实现需要 GB2312 编解码。首版引入 `encoding_rs = "0.8"` 后，门禁
`crates/world/tests/no_parallel.rs:181`（`shared_crate_has_no_dependencies_at_all`）立即变红：

```
crates/shared 必须保持零依赖（B 线也依赖它）；实际: ["encoding_rs"]
```

`crates/shared` 的零依赖是既有硬契约（本车道写集不含该测试），故撤销依赖、把这 10 个成员登记为
**延后**；部署路径已由 `bins/*/src/gbk.rs`（db-srv/game-gate/login-gate/login-srv/sel-gate）承担，
且既有差异登记 `mir2-rs/tests/parity/whitelist.md` A-14 已把 `encoding_rs::GBK` 近似 .NET cp936 记录在案。

## 2. 探针方法与实测输出

- 环境：本机 `dotnet --list-runtimes` 含 `Microsoft.NETCore.App 8.0.26/8.0.31`；探针项目 `net8.0`。
- 探针：`E:\tmp\hutil-probe`（独立目录，**不入库**），把 oracle 的对应方法逐字复制后打印边界输入。
- 输出留档：`E:\tmp\hutil-probe\probe-output.txt`、`probe-output2.txt`。

### 2.1 数值/解析（节选实测）

```
StrToInt(" 12 ")=12   StrToInt("+12")=12   StrToInt("\t12\n")=12
StrToInt("\u00A012\u00A0")=-99   StrToInt("\u300012")=-99
StrToInt("1,000")=-99   StrToInt("０１２")=-99   StrToInt("+ 7")=-99
StrToInt("12\0")=12   StrToInt("12\0\0")=12   StrToInt("1\02")=-99   StrToInt("\0")=-99
StrToUInt16("+12")=12  StrToUInt16("-1")=999  StrToUInt16("65536")=999
Round(2147483647)=2147483647   Round(2147483648)=-2147483648
Round(-3e9)=-2147483648        Round(NaN)=-2147483648   Round(±∞)=-2147483648
RangeInDefined(4,5,3)=False   RangeInDefined(5,5,3)=False   RangeInDefined(3,5,3)=False
```

**结论**：`int.TryParse` 只认 `\t \n \v \f \r` 与空格（不认 U+00A0/U+3000），允许尾随 NUL；
`(int)Math.Round(...)` 对越界/NaN/∞ 走非检查转换得 `INT_MIN`（Rust `as i32` 饱和到 `INT_MAX`，不等价）；
`RangeInDefined` 在 `min > max` 时恒 false。

### 2.2 字符串切分 / 缺陷保持（节选实测）

```
GetValidStr3("  abc def", ' ')      => dest=abc  ret=def
GetValidStr3("a,,b", ',')           => dest=a    ret=b
GetValidStr3("a, ,b", ',')          => dest=a    ret=" ,b"
GetValidStr3("\u3000a b", ' ')      => dest="\u3000a" ret=b
GetValidStr3(string[]{"ab"}, "abXabY") => dest=bX ret=bY   // 只取每个元素首字符
ReplaceChar("abc", 'a', 'x')        => "abc"                // 缺陷：返回值未替换
IsStringNumber(null/""/"abc")       => True/True/True        // || 逻辑缺陷
GetDayCount(2026-10-20, 2025-01-01) => 649                   // min 年份被清零，月/日按 30 天
IpToInt("1.2.3.999")                => 16909287              // 无 0..255 校验
IpToInt("1.2.3.-4")                 => -4
IpToInt("1.2.3.4.5")                => 16909060              // 第 5 段起忽略
FormatBytesValue(1029.12)           => "1 KB"
FormatBytesValue(1294467.072)       => "1.23 MB"
FormatBytesValue(1152)              => "1.12 KB"             // Math.Round(1.125,2)=1.12（一半取偶）
FormatBytesValue(1e30)              => "867361737988.4 EB"
```

### 2.3 GB2312（实测，供延后项参考）

```
GetBytes("测试ABC") = B2-E2-CA-D4-41-42-43
GetBytes("ＡＢ")    = A3-C1-A3-C2
GetBytes("€")       = 80
GetBytes("😀")      = 3F-3F          // 每个 UTF-16 码元一个 '?'
GetBytes("\uFFFF")  = 3F
StringToByteAry("AB")   = 00-41-42, len=2   // 首字节固定 0
StringToByteAry("测试") = 00-B2-E2-CA-D4, len=4
StrPas(41 00 42) = "A\0B"      SpanToStr(80 FF) = "??"
```

## 3. 逐成员清册（76 项）

图例：✅ 已实现且等价；⚠️ 本批前有实现但不等价（已修）；❌ 缺失；— 平台/类型不适用（不移植）；
⊘ GB2312 族延后（`shared` 零依赖契约，bins 已有实现）。

| # | oracle 成员 | 语义要点 | 本批前 | 本批后 |
|---:|---|---|---|---|
| 1 | `Backslash` | 常量 `'/'`（正斜杠） | ❌ | ✅ `BACKSLASH` |
| 2 | `Separator` | `{' ', ',', '\t'}` | ❌ | ✅ `SEPARATOR` |
| 3 | `DelfautItem` | `UserItem` 包类型 | ❌ | — 属 `crates/protocol` |
| 4 | `DetailtMagicRcd` | `MagicRcd` 包类型 | ❌ | — 属 `crates/protocol` |
| 5 | `Sequence()` | `Guid.NewGuid` 取正 int | ❌ | — 运行期随机；调用方用确定性计数器 |
| 6 | `GetTickCount()` | `Environment.TickCount`（开机毫秒） | ❌ | — 运行期；bins `clock` 抽象（白名单 D-1） |
| 7 | `GetTimestamp()` | Unix 毫秒 | ❌ | ✅ `get_timestamp` |
| 8 | `GetTimestamp(TimeSpan)` | 当前 + 偏移 | ❌ | ✅ `get_timestamp_offset` |
| 9 | `MakeLong(short,short)` | `(ushort)lo \| hi<<16` | ✅ | ✅ `make_long` |
| 10 | `MakeLong(ushort,ushort)` | `lo \| hi<<16` | ✅ | ✅ `make_long_u16` |
| 11 | `MakeWord(byte,byte)` | `hi<<8 \| lo` | ❌ | ✅ `make_word` |
| 12 | `MakeWord(ushort,ushort)` | `(ushort)(lo \| hi<<8)`（截断） | ✅ | ✅ `make_word_u16` |
| 13 | `HiWord(ushort)` | 提升 int 后 `>>16` ⇒ 恒 0 | ❌ | ✅ `hi_word_u16` |
| 14 | `HiWord(int)` | `(ushort)(v>>16)` | ❌ | ✅ `hi_word_i32` |
| 15 | `LoWord(int)` | `(ushort)v` | ❌ | ✅ `lo_word_i32` |
| 16 | `HiByte(short)` | `(byte)(w>>8)` | ❌ | ✅ `hi_byte_i16` |
| 17 | `HiByte(int)` | `(byte)(w>>8)` | ❌ | ✅ `hi_byte_i32` |
| 18 | `LoByte(short)` | `(byte)w` | ❌ | ✅ `lo_byte_i16` |
| 19 | `LoByte(int)` | `(byte)w` | ❌ | ✅ `lo_byte_i32` |
| 20 | `LoWord(uint)` | `v & 0xFFFF` | ❌ | ✅ `lo_word_u32` |
| 21 | `HiWord(uint)` | `v>>16` | ❌ | ✅ `hi_word_u32` |
| 22 | `LoByte(ushort)` | `v & 0xFF` | ❌ | ✅ `lo_byte_u16` |
| 23 | `HiByte(ushort)` | `(byte)(v>>8)` | ❌ | ✅ `hi_byte_u16` |
| 24 | `IsVarNumber` | HUMAN/GUILD/GLOBAL 前缀（不区分大小写） | ❌ | ✅ `is_var_number` |
| 25 | `Round(double)` | AwayFromZero 后转 int；越界/NaN ⇒ `INT_MIN` | ⚠️ 饱和 | ✅ `round` |
| 26 | `RangeInDefined(byte,int,int)` | `Max(min,v)==Min(v,max)` | ❌ | ✅ `range_in_defined_u8` |
| 27 | `RangeInDefined(int,int,int)` | 同上 | ❌ | ✅ `range_in_defined_i32` |
| 28 | `RangeInDefined(long,int,int)` | 同上（long 比较） | ❌ | ✅ `range_in_defined_i64` |
| 29 | `EnterCriticalSections` | `Monitor.Enter` | ❌ | — Rust 所有权/异步模型 |
| 30 | `LeaveCriticalSections` | `Monitor.Exit` | ❌ | — 同上 |
| 31 | `EnterCriticalSection` | `Monitor.Enter` | ❌ | — 同上 |
| 32 | `LeaveCriticalSection` | `Monitor.Exit` | ❌ | — 同上 |
| 33 | `GetString(span)` | GB2312 解码 | ❌ | ⊘ |
| 34 | `GetString(span,len)` | 同上 | ❌ | ⊘ |
| 35 | `GetString(span,index,count)` | 同上 | ❌ | ⊘ |
| 36 | `GetString(Memory,index,count)` | 同上 | ❌ | ⊘ |
| 37 | `GetString(byte[],index,count)` | 同上 | ❌ | ⊘ |
| 38 | `DoubleToDateTime` | OLE 日期（1899-12-30 起） | ❌ | — 无 DateTime 类型；数据层用 chrono |
| 39 | `DateTimeToDouble` | 同上 | ❌ | — 同上 |
| 40 | `StrPas(byte[])` | 每字节 `(char)b`（NUL 保留） | ❌ | ✅ `str_pas` |
| 41 | `SpanToStr(span)` | `ASCII.GetString`，高位 ⇒ `?` | ❌ | ✅ `span_to_str` |
| 42 | `StrToInt(string,int)` | `int.TryParse` 失败给默认 | ⚠️ Unicode trim | ✅ |
| 43 | `StrToUInt16(string,ushort)` | `ushort.TryParse` | ❌ | ✅ `str_to_uint16` |
| 44 | `StrToInt16(string,short)` | `short.TryParse` | ✅ | ✅ |
| 45 | `StrToInt(span,int)` | 同 42 | ❌ | ✅（统一 `&str` 入口） |
| 46 | `StrToDate(string)` | 空 ⇒ Today，否则 `Convert.ToDateTime` | ❌ | — 无 DateTime 类型 |
| 47 | `GetValidStr3(char)` | `Split(...,2,RemoveEmptyEntries)` | ❌ | ✅ `get_valid_str3` |
| 48 | `GetValidStr3(char[])` | 同上 | ✅ | ✅ |
| 49 | `GetValidStr3(string[])` | 每元素取首字符 | ❌ | ✅ |
| 50 | `GetValidStr3(string)` | 每字符都是分隔符 | ❌ | ✅ |
| 51 | `GetValidStrCap(char)` | 空⇒("","")；`"`⇒CaptureString；否则 GVS3 | ❌ | ✅ `get_valid_str_cap` |
| 52 | `GetValidStrCap(char[])` | 同上 | ✅ | ✅ |
| 53 | `GetValidStrCap(string[])` | 同上 | ❌ | ✅ |
| 54 | `IsStringNumber` | `!IsNullOrEmpty \|\| regex` ⇒ 恒 true | ✅ | ✅（缺陷保持） |
| 55 | `ArrestStringEx` | 三态写回 + catch 吞异常 | ✅ | ✅ |
| 56 | `CompareLStr(src,targ)` | 以 targ 长度为前缀 | ✅ | ✅ `compare_lstr` |
| 57 | `CompareLStr(src,targ,compn)` | 显式长度 | ✅ | ✅ `compare_lstr_prefix` |
| 58 | `IsEngNumeric(char)` | ASCII 字母/数字 | ❌ | ✅ |
| 59 | `IsEnglishStr(string)` | **存在**任一 ASCII 字母 | ❌ | ✅ |
| 60 | `ReplaceChar` | **缺陷**：恒返回原串 | ❌ | ✅（缺陷保持） |
| 61 | `TagCount` | 计数 | ❌ | ✅ `tag_count` |
| 62 | `BoolToStr` | `"TRUE"/"FALSE"` | ❌ | ✅ |
| 63 | `_MIN(int,int)` | 较小值 | ✅ | ✅ `min_i32` |
| 64 | `_MAX(int,int)` | 较大值 | ✅ | ✅ `max_i32` |
| 65 | `BoolToCStr` | `"是"/"否"` | ❌ | ✅ |
| 66 | `BoolToIntStr` | `"1"/"0"` | ❌ | ✅ |
| 67 | `GetBytes(string)` | GB2312 编码 | ❌ | ⊘ |
| 68 | `GetBytes(int)` | `GetBytes(v.ToString())` | ❌ | ⊘ |
| 69 | `GetByteCount(char)` | GB2312 字节数 | ❌ | ⊘ |
| 70 | `GetDayCount` | `min` 年份被清零，月/日 ×30 | ❌ | ✅ `get_day_count`（`(y,m,d)` 入参） |
| 71 | `SBytePtrToString` | `unsafe` 指针 | ❌ | — workspace `unsafe_code = "forbid"` |
| 72 | `StringToByte` | GB2312 编码 | ❌ | ⊘ |
| 73 | `StringToByteAry` | GB2312 + 首字节 0 | ❌ | ⊘ |
| 74 | `CompareBackLStr` | 尾部 compn 字符比较 | ❌ | ✅ |
| 75 | `FormatBytesValue` | 1024 进制 + `Math.Round(x,2)`（一半取偶） | ❌ | ✅ |
| 76 | `IpToInt` | 前四段 `long.Parse` 后移位，无校验 | ❌ | ✅（返回 `Result`） |

## 4. 判据与牙（9/9 改坏必红）

命令（在 `/e/tmp/wt-hutil/mir2-rs`，`CARGO_TARGET_DIR=/e/tmp/wt-hutil-target`）：

```
cargo test -p mir2-shared --lib <filter>
```

脚本 `/e/tmp/hutil-teeth.sh` 对每个用例：备份 → 注入变异 → 跑指定 filter → 要求 rc=101 且点名断言 →
`cp` 还原 → `sha256sum -c`。实测结果：

| # | 变异（改坏生产路径） | 判据（必红） | 结果 |
|---:|---|---|---|
| 1 | `round` 删越界分支（退回 `as i32` 饱和） | `round_matches_dotnet_int_cast` | RED |
| 2 | `trim_parse` 改用 Rust `s.trim()`（接受 NBSP/全角） | `str_to_int_dotnet_semantics` | RED |
| 3 | `make_long` 低半部符号扩展（去掉 `(ushort)` 截断） | `bit_packers_match_oracle` | RED |
| 4 | `RangeInDefined` 改成开区间 `v>min && v<max` | `range_in_defined_matrix` | RED |
| 5 | `ReplaceChar` “修好”成真替换 | `replace_char_preserves_oracle_bug` | RED |
| 6 | `IpToInt` 给最低字节加 `& 0xFF` 掩码 | `ip_to_int_matrix` | RED |
| 7 | `GetDayCount` 月按 31 天折算 | `get_day_count_zeroes_min_year` | RED |
| 8 | `GetValidStr3` 连续分隔符只跳一个 | `get_valid_str3_dotnet_semantics` | RED |
| 9 | `FormatBytesValue` 一半取偶改成一半远离零 | `format_bytes_value_matrix` | RED |

还原校验：

```
ORIG_SHA=85691169424ec2bdd5dc4107c0f005746348cb362c2adeff5904cad9fcbb18bb
crates/shared/src/hutil32.rs: OK
TEETH_RC=0
```

## 5. 门禁（本地，全绿）

| 门禁 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | rc=0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 |
| `cargo test --workspace` | rc=0（`mir2-shared` 32 个 lib 测试全过） |
| `cargo run -p msgcodegen -- --check` | rc=0（604 条常量一致） |
| `python -I whitelist_anchor_gate.py check` | rc=0（234 条锚 ALL PASS） |
| `python -I m3_whitelist_registry_audit.py audit --strict` | rc=0（WHITELIST_REGISTRY GREEN） |

## 6. 未做项 / 残留

1. **GB2312 族 10 个成员延后**（#33–37、#67–69、#72–73）：`shared` 零依赖契约
   （`crates/world/tests/no_parallel.rs:181`）禁止引入 `encoding_rs`；bins 的 `gbk.rs` 已有等价实现。
   若后续要把它们上收共享层，需要 owner 先裁决「shared 零依赖」例外或采用生成表方案。
2. **平台/类型不适用 12 个**（#3–6、#29–32、#38–39、#46、#71）：包类型/运行期时钟/锁/`DateTime`/`unsafe`，
   不在 `shared` 职责内；对应的 Rust 形态已由 `crates/protocol`、bins `clock`、chrono、tokio 承担。
3. 探针工程 `E:\tmp\hutil-probe`（含 `probe-output*.txt`）为**本线临时物**，按收尾清单登记保留，
   便于复查实测值；不属于仓库写集。
4. 未改动 `src/**`、`sql/**`、`MirClient/`、`crates/{protocol,storage}`、`E:/MirServer/**`；
   未起服务、未占端口、未写数据库。
