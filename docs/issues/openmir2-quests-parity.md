# openmir2-quests-parity

> 状态：**开工中（2026-11-20，分支 `card/quests-parity`，base head `9c84ea2265d1d5697a02166cf663bbb20949129a`，
> 记于 2026-11-20 00:25:50 +0800）**。派工：opc-07（任务线 2/9）。principal：`openmir2-ca`。
> 验收尺子：`E:/UserFleet/qa/module-harness-checklist-20261120.md`（42 条，**唯一标准**；要改只能找 opc-07）。
> 本文件当前只交**判定面清单 + 账本格式**，未铺量；§8 有一条需要先裁决的阻塞项。

> **必读（2026-11-20 查实，影响本线全部动作类条目）**：oracle 的 `#ACT` 块里**只有第一条已注册命令执行**——
> `GotoLableQuestActionProcess` 命中 `ExecutionProcessing.IsRegister` 后直接 `Execute(...); return;`
> （符号锚 `GotoLableQuestActionProcess`、`ExecutionProcessing.IsRegister`）。多行 `#ACT` 的后半段**不执行**。
> Rust `run_actions` 1:1 复刻；负例 QT05-D1/D2/D3 钉住，变异证据见 §10.3。

## 1. 分母与不变量

- 分母来源（机器清单，不手写）：`docs/issues/parity-gap-quests.json` 的 `denominator.count` = **9**。
  定义：任务专属脚本命令分支 = 选择门 1 + 条件 2 + 动作 5（通用 NPC 脚本命令归 NPC 块）。
- 现状（2026-11-20 收口）：**9 条中 8 条 `verified`、1 条 oracle 侧不可达（QT-01 `dead_in_oracle`），无遗漏**
  —— 即本块**可达到的全覆盖**。QT-01 的不可达性以探针 + 全仓枚举为证（§10），不计入分子。
- **不变量（交付时逐条自检）**：
  1. `denominator` / `entries` 的 id 集合与 count **一个字节都不动**（checklist R2）；
  2. 分母 9 不重算子，若认为分母有误 ⇒ 停下交证据给 opc-07，不自行改；
  3. 不改 C# 原版逻辑（`src/**` 只读，探针只读引用）；
  4. 不碰魔法线与物品线文件（magic 测试、golden、`openmir2-magic-parity.md`、`parity-gap-magic/items.json`）。

## 2. 判定面清单（状态量从哪读）

> 任务线**没有任务消息 id**：判定不是帧序列，而是「同脚本 + 同玩家状态 ⇒ 状态与分派结果的差分」。
> 下列 S1–S6 全部**一级直读**（实体/脚本对象字段），不得从回包或日志反推（checklist L3）。

| 编号 | 状态量 / 观测点 | oracle 读法（冻结面） | Rust 读法（产品类型） |
| --- | --- | --- | --- |
| S1 | `QuestFlag[128]` 原始字节 | `PlayObject.QuestFlag`，出口 `GetQuestFalgStatus` / `SetQuestFlagStatus`（`src/M2Server/Player/PlayObject.cs`，:3966 起） | `mir2_script::engine::quest_state::QuestBitmap::as_bytes()` |
| S2 | `QuestUnit[128]` 原始字节 | 同上文件 `Get/SetQuestUnitStatus` | 同上（第二个实例） |
| S3 | `QuestUnitOpen[128]` 原始字节 | 同上文件 `Get/SetQuestUnitOpenStatus` | 同上（第三个实例） |
| S4 | 位读写**调用账本** `(op, index, value)` | `IPlayerActor` 替身（`DispatchProxy`）在六个入口记录；本体用真 `PlayObject` | 测试玩家在 `ScriptPlayer` 六个方法里记录，本体用真 `QuestBitmap` |
| S5 | 控制结果：命中哪条支、`says`、`success` | `ScriptEngine` 的 `#IF` 求值与 `#SAY/#ELSESAY` 输出；条件处理器 `ref bool success` | 引擎 `run_*` 返回值 + `npc.says` |
| S6 | 解析结果 `(cmd_code, n_param1..6, s_param1..6)` | `ScriptParsers.cs` 的 `LoadScriptFileQuestAction` / 条件同名函数 | `crates/script/src/parser.rs` 的 `load_quest_action` |

**位语义（两侧必须逐位相同，QX-01 已用真身向量钉住）**：`nFlag -= 1`；`<0` ⇒ no-op / 返 0；
`idx = n/8`、`bit = 128 >> (n%8)`（**MSB-first**）；`idx >= 128` ⇒ no-op / 返 0；
写：`value == 0` 清位、否则置位（**只看是否等于 0**，`2` / `-1` 都算置位）；读：命中返 1 否则 0。

**不纳入判定面（写明理由，checklist L5）**：
- 帧/消息面（RM/SM ident、priority、late）：任务线无任务消息 id，客户端不参与判定；**不作为本线证据**。
- DB 落库：QX-04 已判 `dead_in_oracle`（`characters` 无任务列），不进本线。
- 墙钟字段（`built_at` / `delivery` / 日志时间戳）：一律排除。

## 3. 账本格式（oracle 产出 → Rust 比对；全部 TSV，LF）

> 生产者：`mir2-rs/tests/parity/m3/quests/quest-parity-probe`（新建，照 `quest-state-probe` 骨架：
> 只读引用 `M2Server` / `OpenMir2`，`dotnet run -c Release -- <向量目录>`）。
> 每个账本首行是 `#` 注释 + 列名；文件尾不写汇总（汇总由比较器算）。
> 每个账本旁落一份 `<name>.manifest.json`：`head`（40-hex）、`built_at`（实测 `date`）、
> `rows`、`sha256`（账本自身）。

### 3.1 `quest_action_ledger.tsv` —— 动作级（QT-05..09，**最终口径**）

```
# case_id  actions_parsed  flag_hex  unit_hex  unit_open_hex  exception  unchanged  script
```
- `script`：`#ACT` 正文（换行转义为字面量 `
`）——两侧跑**同一段**脚本，比较口径因此不依赖手抄参数表；
- `actions_parsed`：解析出的动作条数（抓「动作被 `nCMDCode=0` 丢弃」这一类差异）；
- 三张 `*_hex`：跑完后**从实体直读**的位图（oracle `PlayObject` / Rust `NpcPlayer.QuestBitmap`）；
- `exception`：oracle 侧若抛异常记类型名（正常为 `-`）；
- `unchanged`：缺参类用例（`SET [5]` / `RESET [5]` 等）专列——去掉末行再跑一遍前缀，三张位图
  **逐字节相等**记 `1`、不等记 `0`（两侧同口径都断言；其余用例写 `-`）。
- 起始状态一律全零（`init` 通过脚本正文里的前置动作构造），不另设 init 列。

### 3.2 `quest_gate_ledger.tsv` —— 选择门 / 条件（QT-01、QT-02）

```
# case_id  kind  script  init_flag_hex  init_unit_hex  rng_seq  selected  success  says
```
- `kind`：`quest_gate`（`{Quest}` 选择门）/ `check`（`CHECK` 条件）。
- `script`：参与判定的脚本原文（换行在 TSV 内转义为字面量）。
- `rng_seq`：`{Quest}` 的 `#RAND` 抽取序列（`Random(n)` 的逐次返回，逗号分隔；无抽写 `-`）。
- `selected`：门是否选中 / 条件是否走真支；`says`：输出文本按序（`|` 分隔，空写 `-`）。

### 3.3 `quest_parse_ledger.tsv` —— 解析锚（接线提示，**非行为证据**）

```
# case_id  line_text  cmd_code  n_param1..6 (6 列)  s_param1  s_param2
```
> 本表**只覆盖解析**：`RESET` / `RESETUNIT` 走 `n_param1/n_param2`，`SET` / `SETOPEN` / `SETUNIT` 走
> `s_param1/s_param2`——两侧解析口径不同就会在 3.1 的 `calls` 列暴露，3.3 只用于定位。

### 3.4 比较器口径（`tools/window/quests_compare.py`，与其它比较器同风格）

- 两侧逐列比：`init_*` 必须相等（否则夹具不一致 ⇒ rc=2 PREREQ）；`calls` 逐项相等；`final_*` 逐字节相等；
  `success` / `selected` 相等；`says` 逐条相等（顺序敏感）。
- 报告首行必须打计数：`# rows=<n> rust_rows=<n> mismatches=<k>`，**任何一边 0 行 ⇒ 红**（Z3）。
- 缺 golden ⇒ `PREREQ` rc=2（Z2）；空账本 ⇒ 红（Z1）；两侧全零位图且 golden 也全零 ⇒ 红（Z4）。
- 镜像差两侧都报：`golden 有 rust 无` 与 `rust 有 golden 无` 各一列计数（L4）。

## 4. 用例矩阵（每条 verified 的判定面）

| 条目 | 判定面 | 必含用例 |
| --- | --- | --- |
| QT-02 CHECK | S1 + S5（`success`） | flag 命中/未命中 × value 0/非 0（4 组）；非法 flag 文本（`abc` / `0` / `-3` / `129`）各 1 |
| QT-05 SET | S1 + S4 | value 0/1/2/-1；flag 1/8/9/1024/1025/0/-3；sParam1 非数字、sParam2 非数字（各 1） |
| QT-06 RESET | S1 + S4 | nParam2 缺省（sParam2 非数字 ⇒ 默认 **1**）、count 0/1/5/负；nParam1 非数字（⇒ 0，flag 0 是 no-op） |
| QT-07 SETOPEN | S3 + S4 | 同 QT-05 的边界集 |
| QT-08 SETUNIT | S2 + S4 | 同 QT-05 的边界集 |
| QT-09 RESETUNIT | S2 + S4 | 同 QT-06 的边界集 |
| QT-01 {Quest} | S1 + S5 + RNG | 整条已判 `dead_in_oracle`（§10）：oracle 侧 `IsQuest` 恒 false，选择门不可达；`#RAND` 支同样不可达，维持 B-13 |

> **参数口径（读码查实，2026-11-20 更正版；必须有用例钉住）**：
> 1. 解析器对 `SET`/`RESET`/`SETOPEN`/`SETUNIT`/`RESETUNIT` 先 `ArrestStringEx(sParam1,"[","]")` 剥壳；
> 2. **这是 oracle 的 bug**：符号锚 `HUtil32.IsStringNumber` 的实现是 `!string.IsNullOrEmpty(s) || 正则匹配`
>    ——**任何非空字符串都为真**，正则分支被短路；只有**空参数**（缺参）才为假；
> 3. 只要 sParam1 或 sParam2 为空 ⇒ `nCMDCode = 0` ⇒ **整条动作被丢弃**（不进处理器，只记一条脚本错误）；
> 4. 处理器口径：`SET`/`SETOPEN`/`SETUNIT` 用 `StrToInt(sParam1,0)`/`StrToInt(sParam2,0)`
>    （非数字值 ⇒ **0 ⇒ 清位**）；`RESET`/`RESETUNIT` 用 `nParam1/nParam2`，而动作解析里
>    `nParam2` 的默认值是 **1** ⇒ 非数字 count ⇒ **清 1 个**。
> 用例：`SET [5] abc`（清 5）、`RESET [5] abc`（清 1 个）、`RESET [5]` / `SET [5]`（整条被丢）。

## 5. 批序（一次只交一项）

1. QT-05 SET（最小面，先打通装置：探针 → 账本 → 比较器 → 三颗牙）
2. QT-06 RESET（引入「默认 1」不对称）
3. QT-07 SETOPEN、QT-08 SETUNIT、QT-09 RESETUNIT（同构复用）
4. QT-02 CHECK（条件侧）
5. QT-01 {Quest} —— **已裁决为 `dead_in_oracle`**（不计入分子，证据见 §10）

每项交付：测试名（`--list` 可见）+ 命令 + rc + 全绿原文路径 + `coverage-report.ps1` 输出（只报脚本结果，不抄数）。

## 6. 牙与防空转（对照 42 条清单，逐条落点）

- **Z1/Z2/Z3/Z4**：比较器四条退化门（空账本红 / 缺 golden 为 PREREQ / 零用例红 / 全零互证红）。
- **Z6 + T1**：每个 verified 条目 ≥1 颗**行为牙**（删/改实现的关键判断 ⇒ 对应用例必红）；文本匹配只作接线提示并标注（T2）。
- **T3**：`RESET` / `RESETUNIT` 的循环体、`SET*` 的 `value == 0` 两臂，**逐臂**各变一次。
- **T4/T5**：变异复原后 `sha256` 相等 + `git status` 干净 + 复跑绿；变异跑在用例所在包并断言 `passed+failed >= 1`。
- **T6**：所有写文件牙只对 tempdir。
- **D1–D3（口径待确认，见 §8-3）**：本线无帧面，把「抽空」定义为**动作链 drain**（`#ACT` 多行 + `GOTO` 跳转环）：
  每 case 记 `MaxDrainPasses` + `DrainPassesHitCap`，超上限必须红或显式 WARN。
- **L3**：状态只从 S1–S3 直读；**L4** 对称差两侧都报；**L5** 排除清单见 §2 末。

## 7. 偏差登记（沿用既有编号，不新造）

- **B-13（QT-01 已据此改判 `dead_in_oracle`，探针证据见 §10）**：`scriptType == 1` 死分支——oracle 从不给 `scriptType` 赋 1（`ScriptParsers.cs` 该函数内 7 处赋值为
  10/20/11/12/10/13/14），`ScriptInfo.IsQuest = true` 仅存在于该不可达块内 ⇒ `{Quest}` 头解析不执行；
  Rust 侧 `check_quest_status` 对 `n_rand_rage > 0` 保守按失败、不抽 RNG。
- **T-37**：`CHECKOPEN` 恒假（oracle 缺陷 1:1 复刻），不得修掉。

## 8. 裁决记录（opc-07，2026-11-20）

1. **QT-01 先做，探针必须防假绿**（已裁决）：证明「脚本真装进来了」后才允许断言 `IsQuest == false`；
   至少 2 份脚本、全仓枚举 `IsQuest` 赋值路径；结论为 `dead_in_oracle` 时**分母 9 不动、id 不动**、
   必须带 deviation id、**不计入分子**。见 §10 的四项证据。
2. **`basis` 可以加**（已裁决）：R2 只禁「改 `denominator.count` / 删改 id / 改 `status_enum` 语义」。
   已执行：`schema_version` 1→2 + `schema_changelog`；QT-03/QT-04 补 `basis: semantic`；
   值域固定 `semantic|bytes|mixed`，缺省 `semantic`。
3. **无帧面用「动作链 drain」，fixpoint 必须写死**（已裁决）：终止条件 = 状态量（S1–S3）+ 调用账本（S4）
   + 控制结果（S5）**三样都不再产生新值**；每 case 记 `MaxDrainPasses` / `DrainPassesHitCap`（两侧同上限）；
   **命中上限必须红**（不是 WARN）；GOTO 环必须有专门负例（人造环 ⇒ 命中上限 ⇒ 红）。
4. **`RESET 5 abc` 实清 1 个**（已裁决）：这条「oracle 怪癖」必须是**独立 verified 条目**，不许并进别的 case；
   行为牙 = 把 Rust 侧改成按 `sParam` 解析 ⇒ 该用例必红。

## 9. 边界与写集（交付前用 merge-base 核）

- 新增：本卡文件、`mir2-rs/tests/parity/m3/quests/quest-parity-probe/**`（csproj + Program.cs）、
  `mir2-rs/tests/parity/m3/quests/vectors/**`（后续账本）、`tools/window/quests_compare.py`（后续）。
- 改：`mir2-rs/crates/script/tests/parser_tests.rs`（QT-01 的 Rust 侧钉住用例；原计划新建
  `quests_parity.rs`，因 `MemFs`/`load` 夹具在同文件、避免复制，改为就地追加并在提交里点名）。
- 允许改：`docs/issues/parity-gap-quests.json` 的**条目 status/evidence/notes/basis** 与
  `schema_version`（不含 denominator 与 id 集合）。
- 不动：`src/**`（C# 冻结面）、魔法线/物品线文件、`mir2-rs/crates/script/src/**` 产品逻辑——
  若某条 verified 需要改实现才能对齐，先报 opc-07，不自行改。

## 10. QT-01 裁决证据（四项，全部可复跑）

| # | 证据 | 命令 | 产物 / 结果 |
| --- | --- | --- | --- |
| 1 | **装载证明** | `dotnet run --project mir2-rs/tests/parity/m3/quests/quest-parity-probe -c Release -- E:/UserFleet/qa/quests/qt01` | `E:/UserFleet/qa/quests/qt01/quest_gate_reachability.tsv`：两份脚本的 `loaded_delta` 与 `known_label_hit` 逐行见该文件（`@other` / `@main`） |
| 2 | **真实标签命中** | 同上 | 同上：`known_label=@other`、`@main` 均 `hit=1`（标签取自脚本原文，写死在探针里） |
| 3 | **全仓 `IsQuest` 枚举** | `grep -rn "IsQuest" --include=*.cs .`（仓库根） | `E:/UserFleet/qa/quests/qt01/grep_isquest_all_cs.txt`：对 `ScriptInfo.IsQuest` 的**唯一** `true` 赋值在 `ScriptParsers.cs` 的死块内；`NormNpc` 的 `IsQuest` 是 **NormNpc 自己的字段**（另一个类，`src/M2Server/Npc/NormNpc.cs` 内自读自写）；其余全是 `false` |
| 4 | **`IsQuest` 断言** | 同 1 | 同上 TSV：`is_quest=0`、`quest_info_null=1`、`verdict=PASS(dead_in_oracle)`；Rust 侧 `quest_gate_is_dead_in_oracle_like_oracle`（`cargo test -p mir2-script --test parser_tests`；原文 `E:/UserFleet/qa/quests/qt01/rust_parser_tests_full.log`，计数以该文件为准） |

补充事实（写进卡面备查）：`scriptType` 在 `LoadScriptFile` 内 7 处赋值为 10/20/11/12/10/13/14、**从不为 1**
（`E:/UserFleet/qa/quests/qt01/grep_scripttype.txt`）；`E:/MirServer` 下 `.txt` 全扫结果为 **0** 命中（`E:/UserFleet/qa/quests/qt01/corpus_quest_headers.txt`，计数以该文件为准）。
⇒ 该特性在 oracle 与语料两侧都不可达，**无法与 oracle 对拍**，按裁决记 `dead_in_oracle`、不计入分子。

## 11. 42 条清单在本线的落点（适用 / 排除，逐组写明）

> 清单 `E:/UserFleet/qa/module-harness-checklist-20261120.md` 是唯一验收标准；本表只说明**哪些条目在本线
> 不适用以及为什么**，不改任何判据。被排除项一律写明替代证据，不得靠"没验到=没要求"蒙混。

| 组 | 适用 | 排除（写理由 + 替代证据） |
| --- | --- | --- |
| P1–P4 | 全部适用 | — |
| R1–R4 | 全部适用 | — |
| Z1–Z4、Z6 | 全部适用 | — |
| Z5（RM/SM 各 >0） | — | **任务线无帧面**（无任务消息 id）：比较器改为报 `rows / rust_rows / mismatches` 与 `frames=0 by design`；行为面证据由 S1–S5 承担 |
| D1–D3 | 适用（口径见 §8-3）：动作链 drain，命中上限**必红** | 不从帧面取 drain |
| L1、L2、L2b | — | 帧序列/优先级序，本线无帧 |
| L3、L4、L5 | 适用 | — |
| L6（RNG 账本） | — | 可达路径不抽 RNG（`{Quest` 的 `#RAND` 在 oracle 死分支内，见 §10）；若将来判定可达再单列 |
| T1–T6 | 全部适用 | — |
| I1–I5 | — | 本线不产出被部署/被压测的二进制（探针是测试工具，不进生产件）；一旦有线引用本线产物再补 |
| E1–E4 | 全部适用 | — |
| S1–S4 | 全部适用 | — |

Z5 / L1–L2b / L6 / I 组的「不适用」若 opc-07 不认可，以 opc-07 裁定为准（我不与 QA 私下谈口径）。

### 10.1 门控判决（opc-07 追问：Rust 会不会按 `{Quest` 门控而 oracle 不会）

**结论**：**两侧均不可达 ⇒ 无玩家可见差异，不需要 divergence 登记**（以行为证据为准，读码为其依据）。

- **oracle 行为**：探针 `--gate` 反射调用真 `ScriptEngine.CheckGotoLableQuestStatus`——对**真实解析出的**
  `ScriptInfo` 返回 true（不门控）；另三条「把门臂装上」的对照必须能返回 false，实测各得预期
  （拦 / 放 / 放）⇒ 判据非恒真。产物 `E:/UserFleet/qa/quests/qt01/quest_gate_decision.tsv`，rc 见同目录
  `probe_gate_run.log`。
- **Rust 行为**：`quest_gate_only_applies_to_non_main_labels_like_oracle`
  （`cargo test -p mir2-script --test engine_tests`）：非 `@main` 路径不装臂时说得出话、装臂后必须拦；
  原文 `E:/UserFleet/qa/quests/qt01/rust_script_tests_full.log`。
- **负控逼出来的新事实**：`@main` 快路径在**两侧都不过任务门**——oracle `ScriptEngine.GotoLable` 的
  `@main` 分支直接按标签选脚本、不调 `CheckGotoLableQuestStatus`；Rust 1:1 同形，用例第 3 段钉住它。
- **`is_quest` 枚举的范围**（同源问题）：代码里有**三个**同名 `is_quest`——`mir2_script::ScriptInfo`
  （引擎读的那个：产品零写入，唯一 `= true` 出现在测试内，用于负控）、`bins/game-svr` 的 `NpcDef`
  （镜像 oracle 的 `NormNpc.IsQuest`，确实有 `true`）、`crates/envir` 的 mapquest 字段。
  命令与原文：`E:/UserFleet/qa/quests/qt01/grep_is_quest.txt`、`.../grep_is_quest_writes.txt`、
  `.../grep_scriptinfo_literals.txt`（覆盖 `is_quest =` 赋值、`is_quest:` 结构体字面量、`ScriptInfo {` 构造点）。
- **负控只在测试内注入**：Rust 侧是 `npc.scripts[0].is_quest = true` 一行测试代码；oracle 侧是探针在内存里
  构造 `ScriptInfo`。产品代码不加任何开关。

### 10.2 夹具入库（可复跑）

`mir2-rs/tests/parity/m3/quests/fixtures/`：`quest_probe_a.txt`、`quest_probe_b.txt`、`README.md`（合成出处
与「现网 0 命中」说明）、`SHA256SUMS.txt`（仓库字节口径）。探针改为**从仓库读夹具**
（`--fixtures <dir>`，缺文件 ⇒ rc=2 PREREQ），不再内嵌脚本字符串。

### 10.3 动作级对拍（QT-05..09）与三条新事实

金标准 `mir2-rs/tests/parity/m3/quests/vectors/quest_action_ledger.tsv`（探针 `--actions`，31 个 case；
真 `ScriptEngine.GotoLable` + 真解析器 + 真 `PlayObject`）。Rust 用例 `quest_actions_match_oracle_ledger`
（`cargo test -p game-svr --test quests_parity`，2 passed / 0 failed，原文见仓外 `E:/UserFleet/qa/quests/qt01/rust_actions_test.log`）。

1. **`#ACT` 里只有第一条已注册命令执行**：`GotoLableQuestActionProcess` 命中 `IsRegister` 后
   `Execute(...); return result;` ⇒ 同块后续动作全部不执行（符号锚 `GotoLableQuestActionProcess` /
   `ExecutionProcessing.IsRegister`）。Rust `run_actions` 1:1 复刻。用例 QT05-02/03/04、QT05-D1/D2/D3 钉住。
   ——这也解释了首跑的「`SET [5] 1; SET [5] 0` 终值仍置位」：第二条根本没执行，**不是**位运算问题。
2. **引号包空格的值会让 oracle 解析器崩**：`SET [5] " "` ⇒ `HUtil32.CaptureString` 的
   `for (i = c + 1; i <= len; i++)` 在 `i == len` 时取 `source[i]` ⇒ `IndexOutOfRangeException`（解析期，
   动作一条都没执行）。Rust 对应结构化 `LoadPanic::Capture(CaptureStringPanic)`（显示文本同样含
   `IndexOutOfRangeException`），比较器按「同崩对拍」断言失败类型一致。
   **副作用面**：`NormNpc.LoadNPCScript` 及其调用点（`WorldServer` 的 NPC/商人初始化循环）**没有 try/catch**
   ⇒ 该异常会中止整批脚本装载。建议单独立卡（是否作为 oracle 缺陷 1:1 复刻需 opc-07 裁定）。
3. **缺参不是「整条丢弃」**（更正先前口径）：`IsStringNumber("")` 因正则 `^[+-]?\d*[.]?\d*$` **匹配空串**而为真
   ⇒ `nCMDCode` 照常设置；处理器按 `StrToInt` 默认值执行（`SET` 值 0 ⇒ 清位、`RESET` count 1 ⇒ 清 1 个）。
   当前用例起始位图为全零，因此这两条只证明「被执行且不崩」，**未单独观测清位**；要观测需加「预置位」用例（下一步）。

### 10.4 预置位与变异证据（2026-11-20 补）

- **缺参不是丢弃**（§10.3-3）的观测证据：预置位用例先由探针直接 `PlayObject.SetQuestFlagStatus(5,1)`
  把 bit5 置上（初始 `0800…`），再跑缺参动作：
  `QT05-10S`（`SET [5]`）⇒ 终值 `0000…`；`QT06-06S`（`RESET [5]`）⇒ 终值 `0000…`
  （探针 stdout 的 `SEEDED … init=08000000 final=00000000 changed=1`，原文
  `E:/UserFleet/qa/quests/qt01/probe_actions_run.log`）。两侧比较器都带同一 seed 重放。
- **`#ACT` 只有第一条执行的变异证据**：把 Rust `run_actions` 的 `return ok` 改成 `continue`
  （让两条都执行）⇒ `cargo test -p game-svr --test quests_parity` **必红**，红的正是多行用例
  QT05-02/09/D1/D2/D3（原文 `E:/UserFleet/qa/quests/qt01/mutation_first_action.log`）；
  用 `git checkout --` 复原后 `sha256` 与变异前相等（`8ae0eff1…`），复跑 **2 passed / 0 failed**
  （`…/mutation_restored.log`）。
- **仍未做**：opc-07 要求的「坏行混在脚本集里时两侧最终可用 NPC/商家集合相同」这条**中止范围**等价证据，
  下一批补。

### 10.5 QT-02 CHECK 条件对拍（2026-11-20）

金标准 `mir2-rs/tests/parity/m3/quests/vectors/quest_check_ledger.tsv`（探针 `--checks`，12 case）：
真解析器取 `QuestConditionInfo`（`CHECK` 的 `[...]` 剥壳走 `ScriptParsers` 的 `ConditionCode.CHECK` 特判），
再走真 `ConditionProcessingSys.Execute`；取数口是**预置位的真 `PlayObject`**（不是替身）。
Rust 用例 `quest_check_conditions_match_oracle_ledger`（`cargo test -p game-svr --test quests_parity`，
3 passed / 0 failed，原文 `E:/UserFleet/qa/quests/qt01/rust_checks_test.log`）。
行为牙：删掉 `cond_check` 的「标记为 0 而期望非 0 ⇒ 假」一支 ⇒ 4 例必红（QT02-01/05/09/11，
`…/mutation_check.log`）；复原后 `sha256` 相等、复跑绿（`…/mutation_check_restored.log`）。

### 10.6 未完成条目（不计入 QT-05..09 的 verified）

- **中止范围等价**（opc-07 裁定为独立命题）：一组脚本里混入一条会触发解析器越界的坏行时，
  **两侧最终可用的 NPC/商家集合必须相同**。**当前结论：不成立（两侧不等价）**，证据如下，待 opc-07 裁定。
  - oracle：`WorldServer.Initialize` → `MerchantInitialize()` → `NpCinitialize()` 全链**没有 try/catch**；
    `NormNpc.LoadNPCScript` / `Merchant.LoadMerchantScript` → `ScriptParsers.LoadScriptFile` 抛
    `IndexOutOfRangeException` 时异常**穿出整条初始化链** ⇒ 该 NPC 之后的所有 NPC/商家都不再装载，
    世界初始化中止（符号锚 `WorldServer.Initialize`、`NpCinitialize`、`NormNpc.LoadNPCScript`）。
  - Rust：`NpcHost` 的装载循环对 `load_script_file` 的 `Err` **逐条捕获**，记入 `failures` 后
    **继续装载后续 NPC**（符号锚 `NpcHost` 的装载循环 / `NpcLoadFailure`）⇒ 坏脚本只影响它自己。
  - ⇒ 这是**真实差异**：坏脚本在 oracle 里是「整批中止」，在 Rust 里是「单条失败、其余照常」。
    本条**不计入 QT-05..09 的 verified**；修法（让 Rust 也中止）会改变 Rust 现状，需 opc-07 裁定。

### 10.8 A1 落地（2026-11-20，实现完成；不合 master，待 opc-07 排 QA）

**实现落点**
- `NpcHost::load` 的 `Err` = `Box<NpcLoadAbort { panic, partial, failing }>`
  （符号锚 `NpcHost::load`、`NpcLoadAbort`，`bins/game-svr/src/npc.rs`）：中止时保留异常前的前缀宿主
  与失败脚本身份（`chr_name/file_path/script_name/reason`）。
- `WorldStartGate`（符号锚 `WorldStartGate`，`bins/game-svr/src/lib.rs`）三态 `Booting/Ready/Aborted`：
  - `sim_loop` 序言末尾（首个 tick 之前）`mark_ready()`；NPC 装载中止 ⇒ `abort()`（**不再 `panic!`**），
    前缀宿主照常写入 `npc_host`（oracle 的异常只中止后续装载，已 `new` 的 NPC 留在内存）。
  - 网关 accept 循环：`world_start.admitted()`（Booting 挂起）⇒ `Aborted` 时按 oracle
    `src/M2Server/Net/TCP/TCPNetChannel.cs:73-78` 打 `服务器未就绪: {peer}` 并关 socket（不分配槽位）。
  - sim 事件排水：`Aborted` 下拒绝 `Opened/Command/CertificationPending/LoadDbRequest`
    （「入世界必须 Ready」；sim 序言结束前不处理事件 ⇒ 本处只可能见 Ready/Aborted）。

**最小口径 / 未建模（登记 D-21）**
- oracle 在 `StartReady` 未置的**启动窗口**也拒网关连接（`src/M2Server/Net/TCP/TCPNetChannel.cs:73-78`）；本仓 listener 早于
  世界初始化 bind，`Booting` 窗口内**挂起**（不拒不受理）——玩家可见差异仅「连接时机」，不是玩法。
- `src/GameSrv/TimedService.cs:61` 的未就绪早退与 `src/M2Server/Net/TCP/TCPNetChannel.cs:414` 的 `Run` 未建模（本仓无等价物；
  中止后 tick 继续跑，但无会话可进）。
- oracle 中位于 `WorldEngine.Initialize()` 之后的 `GuildMgr/CastleMgr/AutoBot` 与
  `src/GameSrv/GameApp.cs:248` 的 `StartReady = true` 同属 try/catch 尾部；本仓次序不同且其后各步不可失败
  ⇒ 「就绪位 = 序言末尾」的最小口径以 **NPC 装载中止为唯一失败源**。

**三条验收（证据路径；数字以产物为准）**
1. ①**前缀集合逐条相等（含失败脚本名）**：用例
   `quests_abort_scope.rs::abort_scope_prefix_set_matches_oracle_ledger`；金标准
   `mir2-rs/tests/parity/m3/quests/vectors/quest_abort_scope_ledger.tsv`（探针 `--abort-scope` 生成，
   2 好 1 坏夹具 `quests_abort_good_a/bad/good_b`；原始产物 `E:/UserFleet/qa/quests/qt01/abort_scope/`）。
2. ②**中止后不对外服务（准入被拒）**：用例
   `relay.rs::world_init_abort_refuses_admission_and_keeps_prefix_npcs`——真二进制 + 假网关，
   **Booting 窗口内建连** → 中止后读到 EOF、无 `GM_SERVERUSERINDEX`，日志 `服务器未就绪: <peer>`；
   同一行 ERR 可观测「保留异常前已装载的前缀 NPC 1 条（ChrName=QManage）」⇒ `partial` 在生产路径被使用。
3. ③**正常路径不变**：同一用例正常配置半段（`GM_OPEN` ⇒ `GM_SERVERUSERINDEX`）+ 既有 `relay` 全绿 +
   `cargo test -p game-svr --lib` 全绿（三元组原文见证据目录）。

**两颗牙（红→绿；原文 `E:/UserFleet/qa/quests/qt01/abort_scope/`）**
- 牙 A（致命分支改回「记录后继续」）：`mutation_a.log` —— ① 用例红（`expect_err` 拿到 `Ok`，
  `good_b` 也被装载）。
- 牙 B（中止后仍置就绪）：`mutation_b.log` —— ② 用例红（无「世界初始化中止」落账）；
  加强 B2（accept 不检查准入，`admitted()` 恒放行）：`mutation_b2.log` —— 红在「中止后必须读到 EOF」。
- 还原：`git checkout -- <path>` 后 `git hash-object` 与 `HEAD:<path>` 逐字节相等（章程 §15 例外）。


## 12. 自清理留痕

- **2026-11-20（QT-02 提交 `7df84717` 前后）**：探针参数一次未被识别，`--checks` 被当作输出目录，
  在 worktree 根建出 `--checks/`（内含 `quest_gate_reachability.tsv`、`scripts_applied/` 两件）。
  属**本任务自造的误产物**，已整目录删除（`rm -rf -- "--checks"`），并核对删除后
  `git status --porcelain` 为空、无残留 `./-*` 目录。

### 10.7 中止范围等价的落地设计（opc-07 裁定，2026-11-20）

**已查清的事实**（决定验收口径，不再重查）：
- `M2Share.StartReady` 的实际作用：`TCPNetChannel.Connecting` 先判它，为假 ⇒ **拒绝准入**
  （常量文本 `服务器未就绪: {0}`）；`TCPNetChannel.Run` 整段不放行；`TimedService.ExecuteInternal`
  直接 `return`。⇒ 未置就绪 = **玩家连不上、进程活着、内存里留着前缀 NPC**。
- 顶层捕获点：`GameApp.InitializeWorld`（`src/GameSrv/GameApp.cs:222`）整块 `try/catch`，只 `LogService.Error(ex)`；
  异常发生在 `SystemShare.WorldEngine.Initialize()`（`src/GameSrv/GameApp.cs:244`）⇒ 其后的
  `GuildMgr/CastleMgr/AutoBot` 与 **`M2Share.StartReady = true`** 都不执行。

**裁定与落地形态**：
1. `NpcHost::load` 的 `Err` 携带部分状态：`Err(NpcLoadAbort { panic, partial })`，调用方保留部分宿主；
2. `lib.rs` 生产路径**去掉 `panic!`**：记 `ERR` + **不置世界就绪**（最小就绪门，只在**中止路径**生效；
   正常路径行为一个字节都不变，用既有测试 + 定点证明）；若 Rust 无通道级/定时服级等价物，
   就只做"不置就绪 + 不放行准入"这一层，卡面写明**未建模**，不冒充等价；
3. 探针 `--abort-scope`：**不捕获异常**、记录"异常前成功装载名单"，外加失败脚本身份；
   夹具 = 两份好脚本 + 一份会触发 `CaptureString` 越界的坏脚本；
4. 验收三条：①前缀集合逐条相等（含失败脚本名）②中止后准入被拒（观测）③正常路径不变；
   牙 A：致命分支改回"记录后继续" ⇒ ①红；牙 B：中止后仍置就绪/仍放行准入 ⇒ ②红。

**状态**：设计已定；`37ef010c1` 的 `Result` 形态保留。**部分状态与就绪门已实现**，见 §10.8（待 QA）。
