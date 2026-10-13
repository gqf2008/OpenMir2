# M3⑥-实现 2：玩家任务态位读写 + `CHECKOPEN`/`CHECKUNIT` 对齐报告（2026-10-13）

> 卡：`openmir2-m3-quest-state`（issue `eee2f057`）／车道：`openmir2-worker-5` @ `m3cs`
> base = `origin/master 5461c1b1`／写集：`mir2-rs/crates/script/**` + `mir2-rs/tests/parity/m3/quest-state-probe/**` +
> `mir2-rs/tests/parity/m3/evidence-m3-quest-state-20261013/**` + `whitelist.md`（仅新增一行 T-37）。
> 输入 = 盘点片 `M3⑥任务现状盘点报告-20261013.md` §2 第 5 项 / §3 `-quest-state` 行。

## 0. 一句话结论

把 oracle 的三组任务位图语义（`PlayObject.cs:3966-4110`）收成可复用的 `QuestBitmap` 并配 **真 PlayObject 探针向量**
（96 行、逐 op dump 128 字节），补齐脚本引擎缺的两个条件 `CHECKOPEN`（`:199`）/`CHECKUNIT`（`:4104`），
其中 **`CHECKOPEN` 恒假是 oracle 缺陷 1:1 复刻**（同族 `ConditionCheckUnit` 写 `success = true`，它写 `false`），
已登记 `whitelist.md` **T-37**。四门禁全绿；零 `crates/world`/`bins`/`src`/`sql` 改动。

## 1. oracle 链与行号（`src/` 实测，原始件 `00-oracle-anchors.txt`）

### 1.1 位图字段与六个入口（`src/M2Server/Player/PlayObject.cs`）

| 项 | 行号 |
| --- | --- |
| 三组字段声明 `byte[] QuestUnitOpen/QuestUnit/QuestFlag` | `:865-867` |
| 构造初始化 `new byte[128]` ×3 | `:1038-1040` |
| `GetQuestFalgStatus` / `SetQuestFlagStatus` | `:3966` / `:3990` |
| `GetQuestUnitOpenStatus` / `SetQuestUnitOpenStatus` | `:4013` / `:4037` |
| `GetQuestUnitStatus` / `SetQuestUnitStatus` | `:4060` / `:4084` |

位语义三组同形：`nFlag -= 1; if (nFlag < 0) return; idx = nFlag/8; bit = 128 >> (nFlag%8);`
`if ((idx - Length) < 0) { get: (bit & old)!=0 ? 1 : 0; set: value==0 ? (~bit & old) : (bit | old) }`。
（`flag 107 ⇒ nFlag 106 ⇒ 字节 13、位 32`。）

### 1.2 两个条件（`src/Modules/ScriptEngine/Processings/ConditionProcessingSys.cs`）

| 条件 | 注册 | 处理器 | 语义 |
| --- | --- | --- | --- |
| `CHECKOPEN` | `:52` | `ConditionOfCheckOpen :199` | **起始 `success = false`，三分支都不改回 ⇒ 恒假** |
| `CHECKUNIT` | `:53` | `ConditionCheckUnit :4104` | 起始 `success = true` ⇒ `(GetQuestUnitStatus(flag)!=0) == (value!=0)` |

> 引擎侧调用点 `src/Modules/ScriptEngine/ScriptEngine.cs:397-410`：`GotoLableQuestCheckCondition` 令 `result = true`，
> 对**第一条已注册条件** `ConditionScript.Execute(...)` 后**立即 `return result`** ⇒ 上表的起始值就是最终值。

### 1.3 会话载入 / 存档写回（写集边界，本卡未动）

| 环节 | 行号 |
| --- | --- |
| 登录载入 `playObject.Quest* = humData.Quest*` | `src/GameSrv/Word/WorldServer.cs:1160-1162` |
| 存档写回 `playerInfo.Quest* = playObject.Quest*` | `WorldServer.cs:1291-1293` |
| DB 编解码（已由 M2 `record-fields` stage6 落地） | `crates/storage/src/playerdata.rs` / `bins/game-svr/src/record.rs` |

`WorldServer` 的这两处属 `crates/world`/`bins/game-svr` 接线面；按 owner 08:10/08:20 的写集纪律，
**本卡不碰**（脚本引擎尚未接入服务端，⑥-3 `-quest-dialog-wiring` 才建这个接缝），见 §6。

## 2. Rust 实现（本片）

| 文件 | 内容 |
| --- | --- |
| `crates/script/src/engine/quest_state.rs`（新） | `QuestBitmap`：`get/set` 逐行复刻 `PlayObject.cs` 位语义 + `from_bytes/as_bytes/into_bytes`（会话载入/写回接缝）+ 3 条模块内单测 |
| `crates/script/src/engine/mod.rs` | `ScriptPlayer` 新增取数口 `quest_unit_status`（`GetQuestUnitStatus`）/ `quest_unit_open_status`（`GetQuestUnitOpenStatus`），默认 0 |
| `crates/script/src/engine/conditions.rs` | `cond_check_unit` / `cond_check_open`（含 oracle 缺陷照搬注释）+ 派发分支 `"ConditionCheckUnit"` / `"ConditionOfCheckOpen"` |
| `crates/script/src/engine/dispatch.rs` | `IMPLEMENTED_CONDITIONS` 登记两名（`unimplemented_handlers()` 相应减 2） |
| `crates/script/src/lib.rs` | 重导出 `QuestBitmap` / `QUEST_ARRAY_LEN` 供世界接线复用 |
| `crates/script/tests/quest_state_vectors.rs`（新） | 读真身向量逐 op 逐字节核对 + 两条红检 |
| `crates/script/tests/engine_tests.rs` | 解析锚点 + `CHECKUNIT` 左右分支 + `CHECKOPEN` 恒假 + 条件真值表（读探针 TSV）+ `MockPlayer` 覆写两个取数口 |

## 3. 真身向量（判据③）

- 探针：`mir2-rs/tests/parity/m3/quest-state-probe/`（`dotnet run`，引用真 `src/M2Server`）。
  - `new PlayObject()` + 直调六个公开入口 ⇒ `vectors/quest_state_vectors.{json,tsv}`：**32 op × 3 数组 = 96 行**，
    每行给 `get` 与原始 128 字节 hex；覆盖 `0/1/8/9/31/32/33/107/255/256/1024/1025/-3`、清位、越界 no-op。
  - 真 `ConditionProcessingSys` + `DispatchProxy` 假 `IPlayerActor` ⇒ `vectors/quest_condition_vectors.tsv`：
    2 handler × 2 value × 2 status = 8 行；`ConditionOfCheckOpen` 4 行**全 `success=0`**。
- Rust 核对：`quest_state_vectors.rs`（逐 op 回放：`set` 才写、每步比 `get` 与 128 字节）；
  `engine_tests::condition_truth_table_matches_oracle`（读条件 TSV，用真引擎跑 `CHECKUNIT [5] v` / `CHECKOPEN [5] v`）。
- 解析锚点（易踩坑）：`CHECK/CHECKOPEN/CHECKUNIT` 的 `sParam1` 在解析器走 `ArrestStringEx(...,"[","]")`
  ⇒ 现网语料写作 `checkunit [5] 1`；无方括号会把 `sParam1` 剥成空串（本片用 `[5]`）。

## 4. 改坏必红（原始件 `03-redcheck-break-must-be-red.txt`）

把 `quest_state.rs` 的位方向 `128u8 >> n14` 改成 `1u8 << n14`（改前/改后各跑一次）：

- 改坏 ⇒ `cargo test -p mir2-script --test quest_state_vectors` **RED**（`quest_bitmap_matches_oracle_byte_for_byte`
  点名第 1 行 `oracle=8000…` vs `rust=0100…`）；
- 还原 ⇒ 同命令 **GREEN 3/3**（`cmp` 与改前文件相同）。

另两条自动红检在用例内：`redcheck_bit_direction`（位方向反的实现与向量必不符）、
`redcheck_comparator_detects_mutated_vector`（把 oracle 向量改一 nibble ⇒ 比较器必红）。

## 5. 四门禁（`02-gates.txt`，cwd=`mir2-rs`，`CARGO_TARGET_DIR=E:/tmp/m3cs-target`）

| 门 | 读数 |
| --- | --- |
| `cargo fmt --all -- --check` | rc=0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 |
| `cargo test --workspace` | rc=0（全部 `test result: ok`，0 failed） |
| `cargo run -p msgcodegen -- --check` | rc=0（604 条常量与 C# 源一致） |

## 6. 写集边界 / 未实现项（交 owner）

1. **会话载入/写回（`WorldServer.cs:1160-1162`/`:1291-1293`）**：需动 `crates/world`/`bins/game-svr`，
   且要等脚本引擎接入服务端（⑥-3 `-quest-dialog-wiring`）才有可观测路径 ⇒ **本卡未动，按纪律交 owner 排m2svr 之后**。
   本片已给出接缝：`QuestBitmap::{from_bytes,as_bytes,into_bytes}`。
2. **`CHECKOPEN` 恒假**：非"未实现"，是 oracle 缺陷照搬（T-37）；修 bug 须另开 `S-*` 批次。
3. `crates/data`、`crates/world`、`bins/*`、`src/**`、`sql/**`：**零改动**。

## 7. 残留物清单

- 本线临时编译目录 `E:/tmp/m3cs-target`（按纪律使用，不入库）。
- 探针构建产物 `mir2-rs/tests/parity/m3/quest-state-probe/{bin,obj}`（由 `dotnet run` 生成；`git status` 已忽略/未跟踪列表外，
  提交时只加 `Program.cs`/`csproj`/`vectors/`）。
- 无端口占用、无起栈、无现网文件写入。
