# M1 收口（S4b）DBSrv 6000 面 —— 片 1（S4a）交付说明

> 卡：`openmir2-m1-dbsrv-6000-face`（issue `370e332f`）｜worktree `dbsrv6k`｜分支 `worktree-m1-dbsrv-6000`｜base `f5170a8e`
> 本片范围 = 计划文档 §5 的 **S4a**（`crates/storage` 记录字段 + C# oracle 字节向量）。
> 6000 `DataService` 面、5700 `MarketService`、`TimedService` 缓存回写（卡内 ②③④）**尚未实现**，
> 逐条登记见下 §3；缺陷卡 `openmir2-m1-dbsrv-selchr-queryed` 另起一片交付。

---

## 1. 本片做了什么（S4a）

| 文件 | 内容 |
| --- | --- |
| `crates/storage/src/playerdata.rs`（新，1245 行） | DBSrv 6000 面与 5100 面共用的记录/消息类型：`RecordHeader` / `Ability` / `NakedAbility` / `MagicRcd` / `ServerUserItem` / `CharacterData`(52 成员) / `CharacterDataInfo` / `PlayerRecordData` / `LoadCharacterData` / `SaveCharacterData` / `LoadPlayerDataPacket` / `ServerRequestMessage` / `ServerRequestData`；**与 C# MemoryPack（`MemoryPackSerializerOptions.Utf16`）逐字节兼容**（对象头/声明序/null 编码/集合编码）。 |
| `crates/storage/tests/playerdatainfo_vectors.rs`（新） | 读 `tests/parity/vectors/oracle_vectors.jsonl` 的 `kind:"playerdatainfo"` 向量，在 Rust 侧**重建同一组字段值**，要求 `to_bytes` 逐字节相同 + `from_bytes` 往返回同值；另含破坏者自检与条数锚点（14 条）。 |
| `crates/storage/src/{lib,traits,types}.rs` | `CharacterDataInfo`/`PlayerRecordData` 从 `types` 的占位迁到 `playerdata` 的**全字段**实现；`lib.rs` 重导出与"冻结范围"注释更新（字段现由 oracle 向量钉住 ⇒ 也是冻结面）。 |
| `tests/parity/oracle/Program.cs` | 新增 `AddPlayerDataVectors`：用 C# 真身 `SerializerUtil` 生成 14 条向量（不写库、不碰现网）。 |
| `tests/parity/vectors/oracle_vectors.jsonl` | +14 行；文件 1147 → 1161 行。 |
| `crates/protocol/tests/oracle_vectors.rs`、`tools/replay/src/main.rs` | 向量文件由多 crate 共用：遇 `playerdatainfo` 跳过（仍不认识的 kind 会 `panic`/`bail`；条数锚点在 storage 侧）。 |

**为什么这是 S4b 的前置**：卡内 ① 明确要求"为这些大结构补 **C# oracle 字节向量**（别只写'能解析'）"。
6000 面的取档/存档体就是这组结构，字节面必须先冻结并逐字节可验证。

## 2. 判据与证据（四件套：命令 + 输出 + 路径 + hash）

- **向量可复现（C# 真身）**：`cd mir2-rs/tests/parity/oracle && dotnet build -c Release && dotnet run -c Release --no-build -- /tmp/oracle_regen.jsonl`
  ⇒ `vectors: 1161 lines`；LF 归一后与入库文件**逐字节相同**（`sha256 = 4e76d755889c26e6fcff4671d5fbf050da0fa873a8f233d16f97b8f797545b5f`）。
  原始输出：`mir2-rs/tests/parity/evidence/M1/s4a-oracle-vectors.txt`。
- **Rust 侧逐条对拍**：`cargo test -p mir2-storage --test playerdatainfo_vectors` ⇒ 3 passed（含 14 条向量 + 破坏者自检 + 条数锚点）。
- **四门禁**：`mir2-rs/tests/parity/evidence/M1/s4b-gates.txt`
  - `cargo fmt --all -- --check` → exit 0
  - `cargo clippy --workspace --all-targets --offline -- -D warnings` → exit 0
  - `cargo test --workspace --offline` **3 连跑** → 67 suites / **398 passed / 0 failed / 11 ignored**（基线 `f5170a8e` 复核 = 66 suites / 392 passed / 0 failed / 11 ignored；本片 +1 suite / +6 用例：`playerdata.rs` 内联 3 + `playerdatainfo_vectors.rs` 3）
  - `cargo run -p msgcodegen --offline -- --check` → 604 常量
  - `cargo run -p replay --offline -- tests/parity/vectors/oracle_vectors.jsonl` → **GREEN**（`外部 kind 向量（storage 等）: 14`，0 mismatch）
- **文件 hash**：`playerdata.rs = b7e130fc…8861`、`playerdatainfo_vectors.rs = c86ea8c5…6312`、`oracle_vectors.jsonl = 4e76d755…5b5f`。

## 3. 卡内 ②③④ 的剩余范围（**逐条登记，未做**）

| 项 | 内容 | 状态 / 阻塞 |
| --- | --- | --- |
| ② | 6000 `DataService` 面（`src/DBSrv/Services/Impl/DataService.cs`：`PlayerDataFixedHeaderDataHandlingAdapter` 6B 头 + `ServerRequestData`；EDCode 签名校验；`DB_LOADHUMANRCD`/`DB_SAVEHUMANRCD`/`DB_SAVEHUMANRCDEX`） | **未实现**。依赖 `crates/storage` 的 `PlayDataStorage`/`CacheStorage` 的 MySQL 实现（C# `Storeages/DBSvr.Storage.MySQL/PlayDataStorage.{Query,Save,Add}.cs` ≈1680 行 + `PlayRecordStorage.cs` 403 行）——本片只落了**记录类型与字节面**。 |
| ③ | 存档字段级 diff = 0（小退/掉线/服务关闭三路径，跨实现互读） | **未实现**，且整链判据 ②③ 需**真 C# GameSvr/DBSrv 做对端** ⇒ 按 owner `2026-10-11 18:55` 指令**等 M2 验收释放端口窗口**（本轮不起 C# 栈）。 |
| ④ | 5700 `MarketService` + `TimedService`（`ProcessCacheStorage` 5 分钟回写 `ICacheStorage`） | **未实现**，可拆子片。`ICacheStorage` 的 C# 实现有一处怪行为（`CacheStorageService.Add` 只在键**已存在**时替换 ⇒ 新键永不入缓存），移植时须 1:1 复刻并登记；`crates/storage` 的 trait 名参是 `&[u8]` 而 C# 用 `string`+`OrdinalIgnoreCase`，接口口径先要评审（6000 面为 Unicode 名，5100 面为 GBK 字节）。 |

> 结论（与 owner 的授权一致）：S4b 整头过大，本轮**先交 S4a 片 + 缺陷卡**（缺陷一行解锁 M1⑤ 选角判据，收益更高）；
> ②③④ 由后续片承接。计划文档 §5 的 S4b 行可据此更新为"片 1 已交"。

## 4. 接口冻结线通知（§12.4：B2/D2/M2）

`crates/storage` 是接口冻结线。本片**动了记录类型**（把 `CharacterDataInfo`/`PlayerRecordData` 从 `types` 占位迁到
`playerdata` 全字段实现，并新增若干 6000 面类型与 `MemPack` trait）——**trait 方法签名未动**。
全仓检索确认只有 `crates/storage` 与 `bins/db-srv` 引用这两个类型（`bins/game-svr/src/record.rs` 的
`CharacterDataInfo` 是其自有的本地解码器，非本 crate 类型），**未触及其他线**。已在 B2/D2/M2 三条线留 `comment` 通知。

## 5. 残留物清单

- 证据（入库）：`tests/parity/evidence/M1/s4a-oracle-vectors.txt`、`s4b-gates.txt`（S4b 门禁）、
  `M1/selchr/*`（缺陷卡证据）。
- `E:\tmp\openmir2-dbsrv6k-target`：隔离 target 目录（保留，复跑门禁用）。
- `src/OpenMir2/bin|obj`、`tests/parity/oracle/bin|obj`：`dotnet build` 产物（未入库，`src/**` 跟踪文件 diff 为空）。
- `E:\tmp\oracle_regen.jsonl`（及 LF 归一版）：向量复现中间物（保留，报告引用）。
- 已清理：`%TEMP%\a2-dbsrv-*` 测试夹具目录为空。
- 未改：`E:/MirServer`、表结构、脚本语法、数据格式；未起任何 C# 服务进程（端口窗口留给 M2 验收）。
