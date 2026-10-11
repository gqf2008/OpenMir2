# 任务：A2/M1 —— Rust 版 LoginSrv（worker-1）

仓库/工作区：`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\a2m1`（分支 `worktree-a2-m1-services`）。
卡：`openmir2-a2-m1-services`（唯一事实来源），owner 身份 `openmir2-worker-1`（worktree 已 join，可省 --actor/--key）。

## 已有
Rust `bins/login-gate`（监听 7000→LoginSrv 5500）与 `bins/sel-gate`（7100→DBSrv 5100）已实现并混跑验证。
内部协议在 `mir2-rs/crates/protocol::internal`（`ServerDataMessage`/`ServerDataPacket`/`ServerDataType`）。

## 本轮目标
实现 Rust 版 **LoginSrv**：监听 **5500**，接受 LoginGate（C# 或 Rust）的内部连接（Enter/Data/Leave/KeepAlive + SocketId 复用），
对客户端登录帧（`CM_IDPASSWORD` 等，原样字节）做账号校验（接 `mir2_account`，`sqlx`；密码本工程约定明文），
回 `SM_PASSWD_OK/FAIL`、服务器列表、`SM_SELECTSERVER_OK` 等——**行为与 C# `src/LoginSrv/**` 一致**（只读参照，不改）。

## 验收（机器可判）
1. **混跑链**：C# LoginGate(7000) → Rust LoginSrv(5500) 完整登录成功（真客户端或 BotSrv 任一即可，给日志+截图/输出）。
2. **认证正反例**：`mir2test/mir2pass` 通过、错密码失败；返回码/文案与 C# 一致。
3. **失败要响**：连不上 MySQL/缺账号要明确报错，不静默空转。
4. 自测：进程级中继/登录自测（照 login-gate 的 tests 风格）。

## 硬门禁（自己跑，贴原始输出；RED 不许交）
```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo test --workspace --offline
```

## 交付
patch 挂卡（`--kind patch`，base refs/heads/master，head refs/heads/worktree-a2-m1-services）+ `status: needs-review`（parent 用上一条 status 的 oid）。
**不要自己 merge、不要签 review。** 证据四件套写进 body。

## 红线
不得改 `src/**`（C# oracle）行为；不得动客户端 MirClient；一次只动本卡范围。
