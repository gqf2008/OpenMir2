# openmir2-quests-parity

> 状态：**开工中（2026-11-20，分支 `card/quests-parity`，base head `9c84ea2265d1d5697a02166cf663bbb20949129a`，
> 记于 2026-11-20 00:25:50 +0800）**。派工：opc-07（任务线 2/9）。principal：`openmir2-ca`。
> 验收尺子：`E:/UserFleet/qa/module-harness-checklist-20261120.md`（42 条，**唯一标准**；要改只能找 opc-07）。
> 本文件当前只交**判定面清单 + 账本格式**，未铺量；§8 有一条需要先裁决的阻塞项。

## 1. 分母与不变量

- 分母来源（机器清单，不手写）：`docs/issues/parity-gap-quests.json` 的 `denominator.count` = **9**。
  定义：任务专属脚本命令分支 = 选择门 1 + 条件 2 + 动作 5（通用 NPC 脚本命令归 NPC 块）。
- 现状：`verified` 2（QT-03 CHECKOPEN、QT-04 CHECKUNIT），待验 7（QT-01/02/05/06/07/08/09）。
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

### 3.1 `quest_action_ledger.tsv` —— 动作级（QT-05..09）

```
# case_id  cmd  script_line  init_flag_hex  init_unit_hex  init_open_hex  calls  final_flag_hex  final_unit_hex  final_open_hex  success
```
- `cmd`：`SET` / `RESET` / `SETOPEN` / `SETUNIT` / `RESETUNIT`。
- `init_*_hex`：全 256 字符十六进制（128 字节），用例起始位图（多为全零，边界用例给非零）。
- `calls`：按调用序，`Set:<arr>:<index>=<value>` / `Get:<arr>:<index>-><ret>` 用 `;` 连接，空串写 `-`。
- `final_*_hex`：动作跑完后的三张位图（**从实体直读 S1–S3**，不是重算）。
- `success`：`ref bool Success` 终值（`1` / `0`）。

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

> 关键**不对称**（读码查实，必须有用例钉住、写进条目 notes）：
> `SET` / `SETOPEN` / `SETUNIT` 用 `StrToInt(sParam1/sParam2, 0)`；`RESET` / `RESETUNIT` 用
> **`nParam1/nParam2`**，而 `nParam2` 的默认值是 **1**（`ScriptParsers.cs` 动作解析段：`nParam1` 默认 0、
> `nParam2` 默认 1），且**只有 `IsStringNumber` 为真时才解析** ⇒ `RESET 5 abc` 会清 1 个（不是 0 个也不报错）。

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
