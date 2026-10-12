# 证据包：`openmir2-m3-guild-load-config`（M3⑧-实现 2：行会/攻城配置项）

交付报告：[`../M3⑧行会攻城配置项报告-20261013.md`](../M3⑧行会攻城配置项报告-20261013.md)。
base = `origin/master a547619a`；写集 = `mir2-rs/crates/data/**`；未起栈、未占端口。

## 件目

| 文件 | 说明 |
| --- | --- |
| `00-commands.txt` | 本包所有命令（可复现） |
| `01-oracle-anchors.txt` | C# 读取点（`ServerConf.cs`）/ 初值（`GameSvrConf.cs`）/ BOM（`ConfigFile.cs`）逐行 grep |
| `02-defaults-vector.tsv` | 默认值向量（39 行，= `crates/data/tests/data/guild_castle_defaults.tsv`） |
| `03-deployed-read.txt` | `dump_guild_castle` 真读现网 `E:/MirServer/M2GameSvr/Server.conf` 的结构化输出 |
| `04-eolcheck.txt` | `verify_sha256sums.py eolcheck`（生产者侧 EOL 门） |
| `05-break-must-be-red.txt` | 改坏 `build_guild_price` 默认 ⇒ RED；还原 ⇒ GREEN（含改前/后 sha256） |
| `06-gate-fmt.txt` | `cargo fmt --all -- --check` rc=0 |
| `07-gate-clippy.txt` | `cargo clippy --offline --workspace --all-targets -- -D warnings` rc=0 |
| `08-gate-test.txt` | `cargo test --offline --workspace` rc=0（87 suites / 785 passed / 0 failed / 18 ignored） |
| `09-gate-msgcodegen.txt` | `cargo run --offline -p msgcodegen -- --check` rc=0（604） |
| `10-boundary.txt` | `git diff --stat` 证明 `src/ sql/ MirClient` 零改动 |
| `SHA256SUMS.txt` | 全包清单（仓库字节 / git blob 口径，`verify_sha256sums.py write` 生成） |
