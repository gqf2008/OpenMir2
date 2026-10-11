# M4① parsecount 两侧对拍数表

> 本文件由一条命令重生成，勿手改：
>
> ```
> python -I tests/parity/M4/m4_parity.py --envir "E:/MirServer/M2GameSvr/Envir"
> ```

- 语料（两侧同一份）：`E:\MirServer\M2GameSvr\Envir`
- 语料文件数：C# `639` / Rust `639`（加载 639/639，缺失 0/0，异常 0/0）
- 计数口径：Rust `mir2-script-tool parse-stats`；C# 参照 `tools/script-parity-cs`（只读引用 `src/Modules/ScriptEngine`，DispatchProxy 假 NPC/商人）。

## ① 四类计数（M4 验收② 原文：语句 / 条件 / 动作 / 宏展开）

| 类别 | 指标 | C# 参照 | Rust | 差 | 判定 |
| --- | --- | ---: | ---: | ---: | --- |
| 语句 | 过程 SayingProcedure（可执行语句块） | 3563 | 3563 | +0 | ✓ |
| 语句 | 标签 SayingRecord | 2386 | 2386 | +0 | ✓ |
| 语句 | 脚本 ScriptInfo | 178 | 178 | +0 | ✓ |
| 条件 | 条件 ConditionList | 2137 | 2137 | +0 | ✓ |
| 动作 | 动作 ActionList | 8869 | 8869 | +0 | ✓ |
| 动作 | 否则动作 ElseActionList | 953 | 953 | +0 | ✓ |
| 宏展开 | #DEFINE 常量替换（define_substitutions） | 0（语料无 #DEFINE） | 0 | — | ✓（见下注） |
| 其他 | 商品 goods | 574 | 574 | +0 | ✓ |
| 其他 | 物品类型项 / item_type_lines | 288 | 288 | +0 | ✓ |
| 其他 | 解析错误行 parse_errors | 427 | 427 | +0 | ✓ |
| 其他 | #CALL 加载失败 load_failures | 0 | 0 | +0 | ✓ |

## ①′ 逐文件内容与逐行错误（③ 门禁的另一面）

| 项 | C# 参照 | Rust | 判定 |
| --- | ---: | ---: | --- |
| 逐文件结构摘要（label/cmd/参数/宏展开后说词） | 639 文件 | 639 文件 | ✓ 逐字节相同 |
| 逐文件原文摘要（解码结果全文） | 639 文件 | 639 文件 | ✓ 逐字节相同 |
| 脚本错误行（逐行多重集） | 427 条 | 427 条 | ✓ 逐行相同（差 0） |

## 注

- **宏展开**：C# 参照实现本身不暴露『常量替换次数』计数器，故 C# 列取语料事实值 `0`（同语料下 `#DEFINE`/`#INCLUDE` 指令数 = 0，grep 见 `multiset-gate.txt` 上方命令）；Rust 计数器 `define_substitutions` 亦为 0。宏展开的**效果**由『逐文件结构摘要』（含展开后的命令/参数/说词全文）逐字节比对代偿；边界语料（`fixtures/macroenvir`，含 2 次 `#DEFINE` 替换）的两侧结构摘要逐字节相同，见 `macro-parity.txt`。
- Rust harness 侧另有 C# harness 未暴露的计数（`call_loads`/`price_rate_lines`/`merchant_cmd_lines`/`label_renames`/`include_loads` 等）：其可观测效果同样并入结构摘要。
- 427 条解析错误是 C# 参照在语料上的**真实行为**（`TakeOn`/`GAMEGIRD`/`ReadRandomLine` 等LEGM2 变体遗留命令在 C# 源码中不存在），两侧逐行多重集差 = 0；见 `whitelist.md` B-101。
