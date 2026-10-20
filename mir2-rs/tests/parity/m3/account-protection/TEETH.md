# 变异牙记录（`account-protection` 验问题修复）

- 日期：2026-10-21（本分支 `account-protection`）
- 范围：本批三处修复的**先红后还原**证据。全部为离线单测，**未连数据库、未占端口**。
- 还原判据：变异前 `sha256sum <file>` = 还原后 `sha256sum <file>`。

## 0. 文件基线 hash（= 本分支提交树的实际文件）

```text
3dd038eb29873170147c06a14f2c853d83287688679314c96081a856bb1721c2 *mir2-rs/bins/login-srv/src/login_server.rs
33ace00c894da5dc7a2269b755b0e392aa744a35c74a95993c35d414835e938a *mir2-rs/crates/storage/src/account.rs
```

## 1. 牙 1：解析尾段短读（oracle `ReadBytes` 短读不抛）

**变异**：把 `parse_user_account_packet` 的 `take()` 改回「必须读满 padding」：

```rust
if buf.len().saturating_sub(start) < size {
    return None;
}
let read = size;
```

**命令 / 结果**（rc=101 ⇒ RED）：

```text
cargo test -p login-srv --bin login-srv trailing_short_read_follows_readbytes_semantics
test login_server::tests::trailing_short_read_follows_readbytes_semantics ... FAILED
panicked at bins\login-srv\src\login_server.rs:1345:56:
短读 padding 应成功
test result: FAILED. 0 passed; 1 failed
```

**还原**：`sha256sum` 回到 `3dd038eb…`；重跑同一用例 `ok`。

## 2. 牙 2：`CM_UPDATEUSER` 覆盖 `PassWord`

**变异**：删掉 `apply_packet_to_record` 里的 `rec.password.clone_from(&packet.password);`
（回到「保留库内旧值」）。

**命令 / 结果**（rc=101 ⇒ RED）：

```text
cargo test -p login-srv --bin login-srv apply_packet_overwrites_all_protection_fields_and_password
test login_server::tests::apply_packet_overwrites_all_protection_fields_and_password ... FAILED
assertion `left == right` failed: C# 改资料会用封包 PassWord 覆盖（login_server.rs:1419）
test result: FAILED. 0 passed; 1 failed
```

**还原**：`sha256sum` 回到 `3dd038eb…`；重跑同一用例 `ok`。

## 3. 牙 3：UPDATE 的 `Phone` 口径

**变异**：`protection_update_values` 的 `Phone`（数组第 4 项）改回 `rec.birthday.clone()`
（与 INSERT 同口径）。

**命令 / 结果**（rc=101 ⇒ RED）：

```text
cargo test -p mir2-storage update_values_match_oracle_quirks
test account::tests::update_values_match_oracle_quirks ... FAILED
assertion `left == right` failed（account.rs:660）
test result: FAILED. 0 passed; 1 failed
```

**还原**：`sha256sum` 回到 `33ace00c…`；重跑同一用例 `ok`（1 passed）。

## 4. 结论

三颗牙都能在对应修复回退时变红，还原后 hash 与基线逐字节一致、用例复绿。
R 级真机窗口证据（`account_protection_roundtrip`）**未执行**，见本目录 `README.md`。
