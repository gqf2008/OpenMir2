# 任务：A2/M1 —— Rust 版 DBSrv（worker-2）

仓库/工作区：`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\a2-dbsrv`（分支 `worktree-a2-dbsrv`）。
卡：`openmir2-a2-m1-services`，owner 身份 `openmir2-worker-2`（worktree 已 join）。

## 本轮目标
实现 Rust 版 **DBSrv**：监听 **5100**（SelGate 的服务端口），接受 SelGate（C# 或 Rust）的内部连接（`ServerDataMessage` Enter/Data/Leave），
对角色相关请求（查询角色列表 `CM_QUERYCHR`、建角 `CM_NEWCHR` 等，原样字节）经 `sqlx` 接 `mir2_db`（`characters_*` 表）服务，
**行为与 C# `src/DBSrv/**` + `src/Storeages/**` 一致**（只读参照，不改）。内部协议见 `mir2-rs/crates/protocol::internal`。
`bins/sel-gate` 可作 SelGate 参照；`bins/login-gate` 可作“内部协议中继/服务端骨架”参照。

## 验收（机器可判）
1. **混跑链**：C# SelGate(7100) → Rust DBSrv(5100) 能取到角色列表（真客户端经 C# SelGate 选角成功；给日志/输出）。
2. **跨实现互读**：Rust DBSrv 读 `mir2_db` 的角色列表与 C# DBSrv 同库同账号，字段级 diff = 0。
3. **失败要响**：连不上 MySQL/表缺字段要明确报错。
4. 自测：进程级查询自测。

## 硬门禁
```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo test --workspace --offline
```

## 交付
patch 挂卡（base refs/heads/master，head refs/heads/worktree-a2-dbsrv）+ `status: needs-review`。**不自行 merge / 不签 review。**

## 红线
不得改 `src/**`（oracle）；不得动客户端；DB 表结构不得改（内容冻结）。
