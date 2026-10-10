# mir2-script — Envir 脚本引擎（B 线 / M4）

OpenMir2 经典脚本语言的 Rust 1:1 移植。硬约束：客户端冻结、内容冻结（脚本语法与文件格式不改）、**行为等价优先于修 bug**（C# 参照的缺陷原样保留）。

参照实现：`src/Modules/ScriptEngine/`（ScriptParsers.cs / ScriptEngine.cs / Consts/*.cs / Processings/*.cs）。

## 本批范围（批次 1：解析层）

- 词法/语法解析：`parser.rs`（`ScriptParsers.LoadScriptFile` 全流程：`#CALL` / `#DEFINE` / `#INCLUDE` / 常量替换 / scriptType 状态机 / 条件动作行解析）。
- 字符串原语：`hutil32.rs`（`GetValidStr3` / `GetValidStrCap` / `CaptureString` / `ArrestStringEx` / `CompareLStr` / `StrToInt` / `IsStringNumber`），全部经 .NET 8 实机探测钉死语义。
- 码表：`codes.rs` 由 `script-tool gen-codes` 从 C# 源码机械生成（条件 207 / 动作 352 / 全局变量 158 对），漂移门禁 `codegen_drift` 测试。
- RNG：`random.rs` 复刻 `System.Random` 带种子构造（.NET 8 CompatPrng / Knuth 减算法），序列与 .NET 逐位一致。
- 对拍：`script-tool parse-stats`（Rust）与 `tools/script-parity-cs`（C# harness，DispatchProxy 假 NPC，只读引用参照实现，不改动任何 C# 参照代码）。
- 解码层判据：`tools/gbk-probe`（C#，只读引用参照实现）——`pairs` 导出全部 32256 个 cp936 字节对映射（入库为 `tests/data/cp936-pairs.txt`），`bom` 打印 BOM 与各分支回退字符实测值。
- 解码层（`textfile.rs` + `gbk_overrides.rs`）：BOM 按 `StringList`（`StreamReader`）实测模型——`EF BB BF`→UTF-8、`FF FE`→UTF-16LE（含 `FF FE 00 00`）、`FE FF`→UTF-16BE、`00 00 FE FF`→UTF-32BE，其余 gb2312；GB2312 解码为 **cp936 精确实现**——`0x80`→U+20AC、`0xFF`→U+F8F5、双字节对差异表（7032 条，由 `script-tool gen-gbk-overrides` 从 .NET 探测表机械生成），并由 `tests/gbk_decode.rs` 对全部 32256 个字节对做穷举门禁。
- 结构摘要门禁（`digest.rs`）：逐文件把 label（含重名改名后的后缀）、`cmd_code`、六个字符串参数、六个数值参数、opname、say 文本压成 FNV-1a 64 指纹，两侧逐文件比对——计数相同但内容不同的缺陷（如命令码位移、字段归属、改名随机序列）同样会变红。

## 已实证的 C# 怪行为（全部保留，改动即违规）

1. **`IsStringNumber` 恒 true**：`!IsNullOrEmpty(s) || regex.IsMatch(s)` 的 `||` 缺陷，且正则 `^[+-]?\d*[.]?\d*$` 匹配空串。
2. **命令码位移**：解析器字典存的是 `GetFields()` 字段序号（`value__` 占 0 号位，成员序号=声明序+1=枚举值）；普通命令存 `字段序号-1`，仅 `CHECK`/`CHECKOPEN`/`CHECKUNIT`（条件）与 `Set`/`ReSet`/`SetOpen`/`SetUnit`/`ResetUnit`（动作）存原值。而执行侧注册表以**枚举值**为键 ⇒ 脚本命令实际执行**前一个**枚举成员注册的处理器（例：脚本 `CHECKLEVEL`（枚举 7）→ CmdCode 6 → 派发 `ConditionCheckUnit`）。已反射 dump 全表实证（`tests/parity/envir-2026-10-10/handler-maps.txt`）。
3. **`CaptureString` 引号处理损坏**：从 `c=1` 起扫描（跳过 0 号引号），dest 含起始引号、不含闭合引号；无闭合引号或无空格时抛 `IndexOutOfRangeException`（穿透解析器）。
4. **`LoadScriptCallScript` 的 label 形同虚设**：`findLab` 只跳过匹配行本身；文件头到首个 `}` 行之间的所有非空非 `{` 行都会被并入（包括 label 之前的行）。
5. **`callList[i] = "#ACT"` 越界即抛**：同一文件二次 `#CALL` 时若此前展开未撑长列表则 `ArgumentOutOfRangeException`。
6. **常量替换**：仅 `#IF`/`#ACT`/`#ELSEACT` 之后的内容行参与；匹配位置必须在行首之后（`n24 <= 0` 退出）；每个 define 每行最多 10 次；`@HOME` 恒在替换表尾部。
7. **`scriptType == 1` 是死分支**（从不赋值 1），quest flag 头解析在生产中不执行。
7b. **`SayingRecord` 与 `Script` 相互独立**：`{Quest` 只切换当前 Script，不改当前记录 ⇒ 新段落开始前的内容行仍写入**上一个**记录（可能属于旧脚本）。已按 (script, record) 坐标解耦实现，并有回归用例。
8. **未知 `#` 指令行静默丢弃**；未知条件/动作命令记 `脚本错误` 并跳过该行。
8b. **`ArrestStringEx` 的写回是三态的**：源为空 → 不改写；已定位 span 但无闭合定界符 → 不改写、返回 `after+span`；**未定位到定界符（含 `len<2`）→ 经 catch 写回空串**。相关局部变量（`slabName`/`sLable`）在 C# 里是方法级、跨行复用，未改写时保留上一行残值——已一并复刻。
9. **重名 label** 追加 `RandomNumber.GetRandomNumber(1, 200)`（即 `Next(1, 201)`）后缀，仍冲突则 `Dictionary.Add` 抛 `ArgumentException`。
10. **`string.Split(sep, 2, RemoveEmptyEntries)`（.NET 8）语义**：空段不计数——首个 token 为 dest，返回值为“第二个 token 起点至串尾”。与 .NET Framework 直觉不同，已实机钉死。

## 对拍结果（2026-10-10，`E:\MirServer\M2GameSvr\Envir`，638 个 .txt + 根目录 1 个共 639）

| 指标 | C# | Rust | 一致 |
|---|---|---|---|
| 文件 / 加载 / 缺失 / 异常 | 639 / 639 / 0 / 0 | 639 / 639 / 0 / 0 | ✓ |
| 脚本 ScriptInfo | 178 | 178 | ✓ |
| 标签 SayingRecord | 2386 | 2386 | ✓ |
| 过程 SayingProcedure | 3563 | 3563 | ✓ |
| 条件 | 2137 | 2137 | ✓ |
| 动作 / 否则动作 | 8869 / 953 | 8869 / 953 | ✓ |
| 商品 goods | 574 | 574 | ✓ |
| 解析错误（脚本错误行） | 427 | 427 | ✓（**逐行**一致，多重集差 0/0） |
| 逐文件结构摘要（label/cmd_code/参数/opname/say 全文 + 商家段） | — | — | ✓ 639/639 指纹相同 |
| 逐文件原文摘要（解码结果全文） | — | — | ✓ 639/639 指纹相同 |
| 宏展开 | 0 | 0 | ✓（语料无 `#DEFINE`/`#INCLUDE`/`#SETHOME`，grep 佐证） |

证据：`tests/parity/envir-2026-10-10/{parse-stats-cs.json,parse-stats-rust.json,diff.txt,handler-maps.txt}`。

**已知边界（非差异）**：427 条解析错误是 C# 参照在语料上的真实行为——`TakeOn`/`GAMEGIRD`/`ReadRandomLine` 等命令在 C# 源码中不存在（LEGM2 等引擎变体遗留内容），C# 同样逐行报错并跳过。内容冻结约束下不可通过“实现这些命令”收敛（那会偏离 C# 行为）。详见 `tests/parity/whitelist.md` B 线节。

## 复现

```powershell
# Rust 侧（mir2-rs/ 目录下）
cargo run -p mir2-script-tool --release -- parse-stats E:\MirServer\M2GameSvr\Envir --json target\parse-stats-rust.json
# C# 侧（仓库根；只读引用参照实现）
dotnet run --project mir2-rs\tools\script-parity-cs\script-parity-cs.csproj -- E:\MirServer\M2GameSvr\Envir mir2-rs\target\parse-stats-cs.json
# 对拍
node mir2-rs\tools\diff-parity.js mir2-rs\target\parse-stats-rust.json mir2-rs\target\parse-stats-cs.json
# 码表漂移门禁
cargo test -p mir2-script-tool --test codegen_drift
```

## 红检（门禁可证伪）

1. 手改 `codes.rs` 任一字节 → `codegen_drift` 必红（已验证：FAILED → 恢复后绿）。
2. 脚本中写入未知命令 → 两侧 `parse_errors` 同步 +1（夹具 `target/redcheck`，双侧输出一致）。
3. 对拍 JSON 改一数 → `diff-parity.js` 必报 DIFF（已验证）。
4. 脚本参数改 1 个数（计数不变）→ 结构摘要变红（已验证：`a.txt` 的 `give 金币 5→6`，计数四项全同、指纹不同）。
5. 去掉解析器的 `字段序号-1` 位移 → 结构摘要 115 个文件变红（计数全不变）——证明该门禁确实能看见 B-8 一类缺陷（已验证，随后恢复并复绿）。
6. **原文摘要门禁上线即抓到真实差异**：`GuildRankNameFilter.txt` 含 cp936 特有映射（`A8BF` → U+E7C8），encoding_rs 会解成 U+FFFD ⇒ 该文件原文指纹两侧不同。已据 .NET 探测表生成覆盖表修正，现 639/639 一致。
7. GB2312 解码穷举门禁：改 `gbk_overrides.rs` 任一映射或 `textfile.rs` 的状态机 → `cargo test -p mir2-script --test gbk_decode` 必红（32256 个字节对全覆盖）。

## 后续批次（不在本批范围）

- 条件/动作求值与内建命令：`Processings/`（ConditionProcessingSys 4753 行 / ExecutionProcessingSys 5998 行 / GrobalVarProcessingSys 1827 行）+ `ScriptEngine.cs` 控制流，派发按上表位移语义 1:1。
- `Robot_def` 的 `RobotObject.LoadScript` 是另一套解析格式，不属于本解析器口径。
- 文件集口径：两侧同为 `Envir` 递归 `*.txt`；Rust 侧按 Win32 通配语义实现（扩展名前 3 字符为 txt 即命中，与 `Directory.GetFiles(root,"*.txt",AllDirectories)` 对齐），且摘要表以**文件相对路径为键**逐条比对，新增/缺失文件同样报差异。
- NPC 渲染侧 `InitializeSayMsg`/`InitializeVariable`（`<$VAR>` 展开，码表已生成）。
