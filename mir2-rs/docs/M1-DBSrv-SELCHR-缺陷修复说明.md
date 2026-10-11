# M1 缺陷：Rust db-srv 未置 `chr_queryed` ⇒ `CM_SELCHR` 恒被拒（修复说明）

> 卡：`openmir2-m1-dbsrv-selchr-queryed`（issue `2e5d3b08`）｜worktree `dbsrv6k`｜分支 `worktree-m1-dbsrv-6000`
> 与卡 `openmir2-m1-dbsrv-6000-face`（同在 `bins/db-srv`）串行：本片是 S4b 片 1（S4a）之后的新 head。

## 1. 根因

C# `src/DBSrv/Services/Impl/UserService.cs:356-361`：

```csharp
if (QueryChr(sText, ref userInfo, ref gateInfo)) { userInfo.boChrQueryed = true; ... }
```

Rust `bins/db-srv/src/handler.rs` 的 `CM_QUERYCHR` 分支**丢弃了 `query_chr()` 的返回值**，
`GateUser.chr_queryed` 除初始化（`NEWCHR`/`DELCHR` 置回 false）外**从未被置 true** ⇒ `dispatch` 的
`CM_SELCHR` 分支 `if u.chr_queryed` 恒假，只打 `[WRN] Double send SELCHR <acct>/<ip>`、不回包。
M1⑤ 全 Rust 边界链实测：登录/选服/查角色 200/200，**选角 0/200**（`ident_hist` 无 525）。

## 2. 修复（一行）

```rust
if query_chr(server, db, slot, tx, u, &packet.body).await? {
    u.chr_queryed = true;
}
```

（新增于 `handler.rs` `CM_QUERYCHR` 分支；语义与 C# 的 `if (QueryChr(...)) { boChrQueryed = true; }` 一致。）

## 3. 判据与证据

### ① 进程级用例（伪 LoginSrv 推会话 + 伪 SelGate 走 QUERYCHR→SELCHR）

`bins/db-srv/tests/live_querychr.rs::exercise_5100_face` 新增 SELCHR 步：
先 `CM_QUERYCHR`（置 `chr_queryed`）→ `CM_SELCHR <acct>/aaa` ⇒ 断言回 **`SM_STARTPLAY`(525)**、
体 `127.0.0.1/7200`（夹具 `ServerInfo.txt` 路由 + `MapFile` 空 ⇒ `MapIndex=0`）、且下行帧与冻结布局逐字节相等。
随后再 `CM_QUERYCHR` 逐字节对 `GOLDEN_QUERYCHR_FRAME_HEX`（并借 SELCHR 把库 `SelectID` 归一到金标准态，
消除"共享库上一次谁选了别的角色"造成的 flaky）。

- 绿：`mir2-rs/tests/parity/evidence/M1/selchr/rust-selchr-green.txt`
  `[selchr] head={ ident: 525, ...} body="127.0.0.1/7200"`，`test result: ok. 1 passed`。
- **红检**（临时删掉 `u.chr_queryed = true` 后重跑，原始输出已入库）：
  `mir2-rs/tests/parity/evidence/M1/selchr/rust-selchr-red.txt`
  ⇒ 用例 FAILED（`等下行业务帧超时`——缺陷路径只记日志不回包），进程侧日志出现
  `[WRN] Double send SELCHR mir2test/127.0.0.1`。

### ② 跨实现对拍（C# DBSrv vs Rust db-srv，字节级 diff = 0）

**等窗口**：owner `2026-10-11 18:55` 指令本轮不起 C# 栈（端口窗口归 M2 真机验收）。
SELCHR 步已并入**两个实现共用的** `exercise_5100_face`；待窗口释放后跑
`cargo test -p db-srv --test live_querychr --offline -- --ignored --nocapture csharp_dbsrv_querychr_matches_csharp_golden`
即得 C# 侧 SELCHR 回包并与 Rust 逐字节对齐（同一断言、同一路由夹具），届时把 C# 捕获值补进金标准常量。

### ③ 四门禁（隔离 target）

见 `mir2-rs/tests/parity/evidence/M1/selchr/gates.txt`（本片树，含缺陷修复）：`fmt`/`clippy -D warnings` exit 0；
`cargo test --workspace --offline` **3 连跑** = 67 suites / 398 passed / 0 failed / 11 ignored；
`msgcodegen --check` = 604。

### ④ 回归：A2 既有 5100 面用例

同一次 `rust_dbsrv_querychr_matches_csharp_golden` 覆盖 QUERYCHR/NEWCHR（三条拒绝分支）/
DELCHR（账号不匹配仍 SUCCESS 的 `bocheck` 语义）/坏会话 ⇒ 全部通过（见绿日志）。C# 侧两个场景
（`csharp_dbsrv_...`、`csharp_selgate_to_rust_dbsrv`）本轮未跑（需 C# 栈 / 固定 5100+7100 窗口）。

## 4. 文件

- `bins/db-srv/src/handler.rs`（+4 行）：`sha256 = 03ad7f07fba37856a6f3f7deb2b45ecc7db898a37ddea96fd6fc3aa9c45850d4`
- `bins/db-srv/tests/live_querychr.rs`（SELCHR 步 + 归一化）：`sha256 = 817189d968d1eee563a69e10084cc8250ffb1ad038dfdc4660cf8c448655b6d9`

## 5. 副作用与残留

- 进程级用例会写 `mir2_db.characters_indexes.SelectID`（`aaa` 选中、其余置 0）——这是选角流程本身
  的写入，且把共享库归一到金标准态（幂等）；对测试账号 `mir2test/aaa|bbb`，无新增测试数据。
- `%TEMP%\a2-dbsrv-*` 夹具目录：用例成功即删；本轮复核为空。
- 未起任何长期进程；未改 `E:/MirServer`。
