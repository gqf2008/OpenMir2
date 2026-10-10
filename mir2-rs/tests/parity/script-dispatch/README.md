# script-dispatch/ —— 脚本派发回归门禁（S 线 ↔ B 线共用）

> 背景：`whitelist.md` **B-8 / T-4**。脚本命令码此前恒派发到**前一个枚举成员**的处理器
> （解析器存 `字段序号-1` 而执行注册表按枚举值建键）。S3 在 C# 侧修掉了它（`ScriptParsers.cs` 两处
> `code-1` → `code`）；Rust 侧按 T-4 **默认仍复刻旧位移**。本目录把这批缺陷的**最小复现集固化成门禁**，
> 免得它只活在某份报告里。

## 一条命令

```powershell
# Rust 已翻转（2026-10-11 起）⇒ 带上 -RequireRustFlipped，让"Rust 口径"也成为硬判据
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/script-dispatch/run_dispatch_regression.ps1 -RequireRustFlipped
powershell -ExecutionPolicy Bypass -File .../run_dispatch_regression.ps1 -SelfTestRed   # 阳性对照
```

判据（全部要过，任一不过即 exit 1）：

| 判据 | 含义 |
| --- | --- |
| `axis-precondition` | 两个枚举「`GetFields()` 字段序号 == 枚举值」且两套键取值等价 —— 这是"改解析器是唯一有效落点"的前提，前提塌了门禁本身就没意义 |
| `corpus-nonempty` | 语料行数 > 0（并校验 sha256 与 `expected.json` 一致 ⇒ 防误改/防悄悄放宽） |
| `no-index-out-of-range-under-current-rule` | **现行口径下 0 行抛 `IndexOutOfRangeException`** —— 修复的验收线 |
| `legacy-rule-still-throws` | 旧位移口径下仍是 120 行抛 —— 语料**仍有牙齿**（若语料被清空或规则写错，这条会掉） |
| `all-lines-affected-by-shift` | 每一行在两套口径下派发目标都不同 |

`-SelfTestRed` 用 `--force-legacy`（把"现行口径"当成旧位移来评估，**不碰源码**）跑同一套判据，
必须判红 —— 这就是"退回旧规则 ⇒ 门禁必红"的阳性对照。

## 文件

| 文件 | 说明 |
| --- | --- |
| `dispatch-regression-120.tsv` | 语料：`相对路径<TAB>脚本文本`，120 行（正本；`evidence/S2/crash-corpus-120.txt` 是同 sha256 的首次抓取件） |
| `expected.json` | 期望值 + 语料 sha256 + 语料来源与选择口径 |
| `run_dispatch_regression.ps1` | 门禁运行器（薄壳：构建探针 → 校验哈希 → 跑 `--gate`） |

## 语料是什么、怎么来的

- 扫运行中的内容 `E:/MirServer/M2GameSvr/Envir`（639 个 `.txt`）1433 条条件行，取**旧位移口径下**
  落到 `ConditionOfCheckRangeMonCount`、且第 5 个参数短于 2 字符的行 —— 该处理器取 `sParam5[1]`，
  空串索引 1 ⇒ 必抛 `IndexOutOfRangeException`（**120 行**）。
- 重新生成（内容变更时）：
  ```
  dotnet build tools/oracle/cond-crash-probe -c Release
  tools/oracle/cond-crash-probe/bin/Release/CondCrashProbe.exe --sweep "E:/MirServer/M2GameSvr/Envir" --dump <新文件>
  ```
  生成后**必须同步更新 `expected.json`**（sha256 + 行数 + 期望计数），并在提交说明里写清差异原因。
- 内容冻结（设计文档 §1 硬约束 2）⇒ 这套语料是稳定的；它变了通常意味着内容被改过。

## 边界与分工（谁负责哪一半）

- **本目录只管 C#/oracle 侧**（探针直接跑生产类型 `ConditionProcessingSys`，不依赖整栈、不需要 DB/客户端）。
- **Rust 侧**（`crates/script`）不在本门禁内。协调者已裁定 **Rust 侧同批翻转**
  （`crates/script/src/parser.rs` 的 `field_index - 1` → `field_index`，见设计文档 §17），执行归 B 线；
  **运行器最后会打印一行"Rust 侧当前口径"**（读 `crates/script/src/parser.rs`，只扫代码不扫注释）。

  **2026-10-11 状态：Rust 侧已由 B2/M4 同批翻转**（`parser.rs` 两处 `field_index-1` → `field_index`），
  故现在一律带 `-RequireRustFlipped` 把 Rust 口径变成硬判据（命令见上）；门禁实测 `gate GREEN（… + Rust 已翻转）`。
  注意：翻转本身只把"派发落点"对齐 C#，flow-diff 是否一致还取决于**处理器是否已实现**——
  翻转后 flow-diff 会从旧位移下的伪一致变成"命中正确处理器但 NotImplemented"，这是预期，靠补 handler 收敛。
- 动作侧（6286 行）同样 100% 受位移影响，但抽样 400 行实测**没有**"参数形状必抛"这一类
  （见 `expected.json.action_side`）⇒ 不另设崩溃夹具，由效果级对拍覆盖。
