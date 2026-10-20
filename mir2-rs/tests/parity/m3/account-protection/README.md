# `account_protection` 13 列真机 roundtrip —— 窗口 run sheet（R 级）

- 卡 / 索引：`openmir2-account-protection-fields`（audit #14）；
  A 表 A-04（`docs/m3-remaining-inventory-20261022.md`）。
- tier：**R**（持久化落库 + 字段级 diff ⇒ 必须有独占窗口）。
- review 口径：**`window_required: true`**；窗口未执行前不得写「已验证 / GREEN」。
- **本目录的作者没有连库、没有执行窗口用例**：本文件与 `expected_columns.json`、
  `run_account_protection_roundtrip.ps1` 只证明「用例存在、守卫会咬人、期望列已写死」。
  真机证据必须由窗口执行人按下面命令采集。

## 1. 判决对象

`audit #14`：造一个带全 13 字段的 `UserAccountPacket` → 走**生产路径**建号 →
`SELECT *` 与 C# 参照实现逐列同值。

oracle 入口：

- INSERT：`src/LoginSrv/Storage/AccountStorage.cs:299-316`
  （`AccountId, UserName, IdCard, Birthday, Phone, MobilePhone, ADDRESS1, ADDRESS2,
  EMail, Quiz1, Answer1, Quiz2, Answer2`）。
- UPDATE：`src/LoginSrv/Storage/AccountStorage.cs:496-513`
  （12 列 SET + `WHERE AccountId`；`Phone` 与 `MobilePhone` 都取 `UserEntryAdd.MobilePhone`，
  与 INSERT 的 `Phone=BirthDay` 口径不同）。
- 封包解析：`src/OpenMir2/Packets/ClientPackets/UserEntry.cs:51-65`；
  短读语义：`src/OpenMir2/Extensions/BinaryReaderExtension.cs:19-27`。

Rust 侧被测点（**用例直接调生产函数**，不是另写一份 SQL）：

- `bins/login-srv/src/login_server.rs`：`parse_user_account_packet` →
  `record_from_packet`（建号）／`apply_packet_to_record`（改资料）→
  `mir2_storage::AccountStore::{add, get, update_account}`。
- 用例：`login_server::tests::account_protection_roundtrip`（默认 `#[ignore]`）。

## 2. 硬约束（工具自己强制）

- **库名守卫**：`MIR2_ACCOUNT_TEST_DSN` 里的 `database` 必须**恰好**是
  `mir2_test_account`；否则用例在**任何 SQL 之前** panic，包装脚本直接拒绝运行。
- **不连生产库**：`mir2_account` / `mir2_db` / `mir2_data` 一律拒绝。
- **自建自删**：证据账号固定 `acctprot01`。开跑先删同名残留（仅这一个账号），
  结束再删 `account_protection` + `account` 两行并校验 `COUNT(*) = 0`。
  中途 panic 会残留该账号，但下次重跑会先清掉（幂等）。
- **只写测试库克隆**：前置是 `mir2_test_account` 的 schema/data 克隆；窗口授权按
  `docs/window-run-sheet-*.md` 的 E1 执行。

## 3. 命令（窗口执行）

```powershell
$env:CARGO_TARGET_DIR = "E:/tmp/wt-acctprot-target"   # 可选：窗口自己的 target
$env:MIR2_ACCOUNT_TEST_DSN = "server=127.0.0.1;uid=root;pwd=;database=mir2_test_account;"

# 0) 只做守卫/命令回显，不连库、不跑 cargo
powershell -ExecutionPolicy Bypass -File `
  mir2-rs/tests/parity/m3/account-protection/run_account_protection_roundtrip.ps1 -CheckOnly

# 1) 真机执行（输出 tee 到证据目录）
powershell -ExecutionPolicy Bypass -File `
  mir2-rs/tests/parity/m3/account-protection/run_account_protection_roundtrip.ps1 `
  -OutDir E:/tmp/<window>/account-protection
```

等价的原生命令（包装脚本只是加守卫 + tee）：

```powershell
cd mir2-rs
cargo test -p login-srv --bin login-srv account_protection_roundtrip -- --ignored --nocapture
```

## 4. 期望输出（逐列 before/after）

`before` = 封包 → C# 期望列值（用例里写死的 `expected_*`）；
`after` = 建号/改资料后 `SELECT *` 读回的库内值。

### 4.1 INSERT（oracle `AccountStorage.cs:299-316`）

| column | expected(C#) | 来源 |
|---|---|---|
| AccountId | 自增 Id（= `store.index("acctprot01")`） | `account.Id` |
| UserName | `tester01` | `UserEntry.UserName` |
| IdCard | `ID1234567890` | `UserEntry.SSNo` |
| Birthday | `19900101` | `UserEntryAdd.BirthDay` |
| Phone | `19900101` | **怪行为**：INSERT 取 `BirthDay` |
| MobilePhone | `19900101` | **怪行为**：INSERT 取 `BirthDay` |
| ADDRESS1 | ``（空串） | 恒空串 |
| ADDRESS2 | ``（空串） | 恒空串 |
| EMail | `t@example.com` | `UserEntry.EMail` |
| Quiz1 | `q1question` | `UserEntry.Quiz` |
| Answer1 | `answer1` | `UserEntry.Answer` |
| Quiz2 | `q2question` | `UserEntryAdd.Quiz2` |
| Answer2 | `answer2` | `UserEntryAdd.Answer2` |

同时断言 `account.PassWord = pwd0rd`（`UserEntry.Password`）。

### 4.2 UPDATE / `CM_UPDATEUSER`（oracle `AccountStorage.cs:496-513`）

| column | expected(C#) |
|---|---|
| UserName | `tester02` |
| IdCard | `ID0987654321` |
| Birthday | `19851231` |
| Phone | `13911112222`（= `MobilePhone`，与 INSERT 口径不同） |
| MobilePhone | `13911112222` |
| ADDRESS1 / ADDRESS2 | 空串 |
| EMail | `u@example.com` |
| Quiz1 / Answer1 | `uq1` / `ua1` |
| Quiz2 / Answer2 | `uq2` / `ua2` |

同时断言 `account.PassWord = newpwd`：C# `AccountUpdateUserInfo` 用封包整体替换
`UserEntry`，`UpdateAccount` 写封包里的 `PassWord`（**不是**库内旧值）。

### 4.3 绿判据

进程 rc = 0，且 stdout 末尾为：

```text
[cleanup] removed=[<id>] account_rows_left=0
RESULT: GREEN account_protection_roundtrip
```

任一列不符 ⇒ 用例 panic，stdout 打印 `MISMATCH` 行 + 末尾
`逐列不符（N 项）：...`。**不许**把「代码看起来一样 / 语义等价」当窗口证据。

## 5. 变异牙（先红 → sha256 还原）

窗口/审查都可以复跑（命令 + 输出 + hash 进证据目录）：

1. **短读牙**：把 `parse_user_account_packet` 的 `take()` 改回
   `buf.get(start..start + size)?`（要求读满 padding）⇒
   `trailing_short_read_follows_readbytes_semantics` 红
   （`buf[..248]` 从 `Some` 变 `None`）。
2. **密码牙**：删掉 `apply_packet_to_record` 里的
   `rec.password.clone_from(&packet.password);` ⇒
   `apply_packet_overwrites_all_protection_fields_and_password` 红。
3. **UPDATE 口径牙**：把 `protection_update_values` 的 `Phone` 改回 `birthday` ⇒
   storage 单测 `update_values_match_oracle_quirks` 红。

每颗牙记录：改前 `sha256sum <file>` → 变异 → `cargo test` 红 → 用改前 hash 还原 →
`sha256sum -c` / 重跑绿。

## 6. 本目录文件

- `README.md`（本文件）
- `expected_columns.json`：C# 期望列清单 + 用例输入值（机读）
- `run_account_protection_roundtrip.ps1`：库名守卫 + 窗口执行包装器

## 7. 执行状态

**未执行（NOT EXECUTED）**。本分支只交付用例与工具；R 级真机证据待窗口，
review 必须写 `window_required: true`，卡片在窗口跑绿前不得 `closed`。
