# 条件① 证据：脚本派发覆盖率全量实测（2026-10-25）

- 卡片：`openmir2-goal1-script-coverage-evidence`（tier=L，owner `openmir2-tester-1`）
- 基线 commit：`0ef4e3b9`（= `walgit/master`，worktree `.worktrees/tester-1`，分支 `card/goal1-script-coverage-evidence`）
- 结论：**语料派发目标覆盖 106/106，未实现 0，`--require-implemented` rc=0**；语料完整性计数无缺口 ⇒ 条件① 可判 PASS。
- 本文只产证据，不改 `src/**`、不改 Envir、不起服务端。

## 1. 复算命令与版本

```sh
cd mir2-rs
cargo build --release --locked -p mir2-script-tool
./target/release/mir2-script-tool.exe parse-stats \
  E:/MirServer/M2GameSvr/Envir \
  --json E:/tmp/scriptcov/parse-stats-20261025.json \
  --require-implemented \
  > E:/tmp/scriptcov/parse-stats-20261025.stdout.txt \
  2> E:/tmp/scriptcov/parse-stats-20261025.stderr.txt
echo $?   # 0
```

| 项 | 值 |
|---|---|
| cargo / rustc | 1.98.1（797e8a9bc）/ 1.98.1（48a229cea） |
| 二进制 | `mir2-rs/target/release/mir2-script-tool.exe`，sha256 `dbb3e35ec1d4d40fd87f7b4855f87644bedd61384b6b8930b8c60da1d720b382` |
| 工具源码基线 | `0ef4e3b9`（工作树干净，`git status` 无改动后构建） |
| 语料 | `E:/MirServer/M2GameSvr/Envir`（生产内容，只读） |
| 命令码表一致性 | `cargo run -q --release --locked -p mir2-script-tool -- gen-codes --check` ⇒ rc=0，`codes.rs` 与 C# `Consts/` 逐字节一致 |

## 2. 实测摘要（stdout 原文）

```
文件: 639 加载: 639 缺失: 0 异常: 0
脚本 178 标签 2386 过程 3563 条件 2137 动作 8869 否则动作 953
宏展开 0 #CALL成功 22 #CALL失败 0 #INCLUDE 0 商品 574
解析错误: 427；解码异常文件: 0
语料派发目标覆盖：已实现 106 个（11829 次调用）；未实现 0 个（按调用次数降序）：
```

按 goal_gate 的口径（`tools/goal/goal_gate.py` 的 `judge_script_coverage`），PASS 需要同时满足：

1. 覆盖行 `未实现 K 个` 中 `K == 0` —— 实测 `K = 0`；
2. `--require-implemented` 退出码 0 —— 实测 rc=0；
3. 语料完整性计数无缺失/异常 —— 实测 `缺失 0`、`异常 0`、`解码异常文件 0`。

⇒ 条件① 本次实测可判 PASS（数字不写死，复算时按上述命令重跑）。

## 3. 派发分类（同一份 stdout，语料实测）

| 分类 | 调用次数 | distinct target |
|---|---:|---:|
| 条件 / 已注册处理器 | 2035 | 36 |
| 条件 / `ScriptEngine` switch | 44 | 3 |
| 条件 / 未注册（C# 静默忽略） | 58 | 2 |
| 动作 / 已注册处理器 | 6400 | 57 |
| 动作 / `ScriptEngine` switch | 3350 | 10 |
| 动作 / 未注册（C# 静默忽略） | 72 | 1 |
| **合计** | **11959** | — |

覆盖判据只统计「C# 侧也注册了派发目标」的 106 个 target（`handler + engine-switch`），
对应调用 11829 次；余下 130 次属于第三个类别，见 §4。

## 4. 未实现清单：0（按族聚类为空）

`未实现 0 个` ⇒ 本卡不拆缺陷卡。为避免「100% 掩盖缺口」，把两个容易被误读的计数在这里说明清楚。

### 4.1 「未注册（C# 静默忽略）」130 次 / 3 个 code —— 非目标

- 定义见 `mir2-rs/crates/script/src/engine/dispatch.rs`：`condition_status` / `execution_status`
  仅对 C# `ConditionCodeDefMap` / `ExecutionCodeDefMap` 里注册的命令返回 `Some`；未注册返回 `None`，
  不计入未实现。`parsestats.rs` 的 `classify` 用同一口径归入 `ignored`。
- oracle 证据：C# 条件派发 `src/Modules/ScriptEngine/ScriptEngine.cs:544` 的
  `switch (questConditionInfo.CmdCode)`、动作派发 `ScriptEngine.cs:890` 起
  `ExecutionProcessing.IsRegister` → `switch (executionCode)` 均**无 `default` 分支**，
  switch 未命中的 code 静默落到下一次循环。Rust 侧按同一语义实现（返回默认结果）。
- 结论：这 3 个 code 在 C# 里同样不执行任何动作，**属非目标**；若要「补齐」等于改 oracle 行为，
  必须走 `T-*`/`D-*` 登记另开卡，不在本重写等价范围内。

### 4.2 427 条解析错误 —— 语料事实，C# 同样报「脚本错误」并跳过

`解析错误: 427` 是 `ScriptError` 行（命令名不在 C# 命令表中，解析不出 CmdCode）。
oracle 证据：C# 在 `src/Modules/ScriptEngine/ScriptParsers.cs:911 / 924 / 937`
对 `LoadScriptFileQuestCondition` / `LoadScriptFileQuestAction` 返回 false 的行打印
`脚本错误: …` 并**丢弃该行**；Rust 解析器逐行复刻该路径（`crates/script/src/parser.rs` 的
`ParseErrorKind::ScriptError`）。这些行从未进入派发，因此不影响覆盖判据，也不构成本地缺口。

按命令族聚类（19 个命令名、24 个文件；括号内为出错行数）：

| 族 | 命令（行数） | 性质 | 可行动单元 |
|---|---|---|---|
| 假人/挂机（LEGM2.CC 假人系统） | `H.TakeOn`(222)、`ReadRandomLine`(24)、`ChangeAttackMode`(18)、`RecallHero`(9)、`AbilityAdd`(6)、`SetScTimer`(5)、`OffLinePlayEx`(4)、`SendScrollMsg`(2)、`SetOffLinePlay`(1) | 第三方引擎（GameOfMir 系）扩展命令；OpenMir2 `Consts/` 无对应 `ScriptDefName`（`gen-codes --check` 证明 Rust 表与 C# 一致） | 无（非目标）。若未来要支持，须先改 oracle 并登记 `T-*` |
| 充值货币写法 | `GAMEGIRD`(41)、`GAMEDIAMOND`(37)、`CHECKGAMEGIRD`(18)、`CHECKGAMEDIAMOND`(18) | 语料把全局变量名当命令写；C# 中仅 `GrobalVarCode.cs:398/401` 有 `$GAMEDIAMOND`/`$GAMEGIRD` 变量名，且变量注册本身在 `ScriptParsers.cs:1722` 是注释状态 | 无（非目标）。变量替换语义另属文本展开，不在命令派发面 |
| 名单/行会/网页等杂项 | `AddLineList`(2)、`DelLineList`(1)、`IsSameGuildOnMap`(1)、`WebBrowser`(2) | 同族：非 OpenMir2 命令名 | 无（非目标） |
| 语料笔误/文本残片 | `}`(11)、`<color=clLime …`(5) | 出现在 `#ACT` 段的 JS 风格残片或颜色标记行，C# 同样按未知命令报错丢弃 | 无（内容冻结，不改 Envir） |

复算该聚类：读 `E:/tmp/scriptcov/parse-stats-20261025.json` 的 `errors[]`，按行首命令名（含 `H.` 前缀剥离）计数即可；
命令名是否在 C# 表中由 `crates/script/src/codes.rs`（`gen-codes` 生成、`--check` 门禁）判定。

## 5. 证据清单

| 文件 | 说明 | sha256 |
|---|---|---|
| `E:/tmp/scriptcov/parse-stats-20261025.json` | 全量 JSON（统计/错误明细/结构摘要/文本摘要） | `df5ce10f93b5857140e724118496e702c040d3cf4fe0d2a5133413619a210ad6` |
| `E:/tmp/scriptcov/parse-stats-20261025.stdout.txt` | 摘要 + 派发分类 + 覆盖行 + 错误前 20 条 | `9a519ae317d960d11ed91610803bf1179c4f6318957870a12fe1f31c8fc1c13c` |
| `E:/tmp/scriptcov/parse-stats-20261025.stderr.txt` | 空（0 字节，sha256 为空串摘要） | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `mir2-rs/target/release/mir2-script-tool.exe` | 本次构建的二进制 | `dbb3e35ec1d4d40fd87f7b4855f87644bedd61384b6b8930b8c60da1d720b382` |

## 6. 卫生门禁（本卡改动 = 仅本文档）

在本 worktree 基线 `0ef4e3b9` 上执行：

| 门禁 | 结果 |
|---|---|
| `python -I tools/whitelist/t_reservation_gate.py check` | rc=0 |
| `python -I tools/hygiene/conflict_marker_gate.py` | rc=0 |
| `python -I -B tools/hygiene/stale_pointer_gate.py` | rc=0 |
| `python -I mir2-rs/tests/parity/m3/whitelist_anchor_gate.py check` | rc=0 |
| `python -I mir2-rs/tests/parity/m3/m3_whitelist_registry_audit.py audit` | rc=0 |
| `python -I -B tools/hygiene/dynamic_count_stamp_gate.py` | **rc=1（改动前即红，非本卡引入）** |
| `python -I -B tools/hygiene/board_coverage_gate.py --board .walgit/board.toml` | rc=0 |

`dynamic_count_stamp_gate` 的两条命中都在 `docs/plan-3day-20261024.md:11-12`（缺快照戳），
该文件在本卡 `write_set` 之外、且改动前已在 `0ef4e3b9` 上为红。已在卡片线程登记，
建议由该文档 owner（Leader）补快照戳后转绿，本卡不越界修改。

`cargo fmt --all -- --check` 与 `git diff --check` 在提交后复跑（本卡无 `.rs` 改动，预期无差异）。

## 7. 残留物清单

- 保留：`E:/tmp/scriptcov/` 下 3 个证据文件（JSON/stdout/stderr），供评审复算 sha256。
- 清理：构建产物留在 worktree 的 `mir2-rs/target/`（gitignore，不随补丁）。
- 未启动：服务端、客户端、假人、抓包进程（本卡全程只读语料）。
