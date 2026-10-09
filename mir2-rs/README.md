# mir2-rs — OpenMir2 → Rust 重写

施工图：`docs/Rust重写设计文档.md`（仓库根）。本目录是唯一 Rust 工作区。

## 布局

```
crates/protocol/   # M0 产出：消息号表 + 帧 + EDCode（接口已冻结，改动须评审）
crates/storage/    # 存储接口（trait 冻结版；记录字段随 D 线/M1 落地）
tools/msgcodegen/  # 从 src/OpenMir2/Messages.cs 生成 messages.rs（含 --check 门禁）
tools/replay/      # 金标准抓包/测试向量回放门禁（逐字节 decode→encode 比对）
tests/golden/      # 金标准抓包（C 线产出，格式见其中 README）
tests/parity/      # 对拍资产：oracle 向量、whitelist.md
```

## 本地门禁（合并前必须全绿）

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p msgcodegen -- --check   # 消息号表与 C# 源漂移门禁
```

## 编码约定

协议层一律按**原始字节**处理文本（线上是 GB2312/GBK 字节流），不在本层做 UTF-8 转换；
需要展示时由上层显式解码（参照 `HUtil32.GetString` 的 gb2312 语义）。
