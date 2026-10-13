# 证据包：`openmir2-m3-pet-slave-config`（M3⑨-实现 1：宝宝 / 召唤配置项）

交付报告：[`../M3⑨宝宝召唤配置项报告-20261013.md`](../M3⑨宝宝召唤配置项报告-20261013.md)。
base = `origin/master 5461c1b1`；写集 = `mir2-rs/crates/data/**` + 本目录；`src/ sql/ MirClient` 零改动；未起栈、未占端口。

## 件目

| 文件 | 说明 |
| --- | --- |
| `00-commands.txt` | 本包所有命令（可复现） |
| `01-oracle-anchors.txt` | C# 读取点（`ServerConf.cs`）/ 初值（`GameSvrConf.cs`）/ ini 语义（`ConfigFile.cs`）/ 常量（`Grobal2.cs`/`MagicManager.cs`）/ `RecallMigic` 类型逐行 grep |
| `02-defaults-vector.tsv` | 默认值向量（24 行，= `crates/data/tests/data/pet_slave_defaults.tsv`） |
| `03-deployed-read.txt` | `dump_pet_slave` 真读现网 `E:/MirServer/M2GameSvr/Server.conf` 的结构化输出 |
| `04-eolcheck.txt` | `verify_sha256sums.py eolcheck`（生产者侧 EOL 门） |
| `05-break-must-be-red.txt` | 三处改坏 ⇒ RED（默认值 / `MasterDieMutinySpeed` 缺陷键 / `MagTammingHPRate` 缺陷键）；还原后 sha256 逐字节一致 |
| `06-gate-fmt.txt` | `cargo fmt --all -- --check` rc=0 |
| `07-gate-clippy.txt` | `cargo clippy --offline --workspace --all-targets -- -D warnings` rc=0 |
| `08-gate-test.txt` | `cargo test --offline --workspace --no-fail-fast` rc=0（89 suites / 800 passed / 0 failed / 18 ignored） |
| `09-gate-msgcodegen.txt` | `cargo run --offline -p msgcodegen -- --check` rc=0（604） |
| `10-boundary.txt` | `git diff --cached` 证明 `src/ sql/ MirClient` 零改动 + 本片路径清单（无回退假差量：base = 当前 master） |
| `11-clean-checkout-repro.txt` | 把本片 `crates/data/**` 变更复制进**干净检出**（`origin/master` 新建 worktree）后 `cargo test -p mir2-data` 仍绿 |
| `SHA256SUMS.txt` | 全包清单（仓库字节 / git blob 口径，`verify_sha256sums.py write` 生成） |

## 判据（DoD）

- **默认值向量**：`crates/data/tests/data/pet_slave_defaults.tsv`（24 行，`GameSvrConf` 初值，逐行给 oracle 锚）
  + `crates/data/tests/pet_slave_config.rs` 逐字段比对 + 行数 = 24 fail-closed。
- **真读现网**：`cargo run -p mir2-data --example dump_pet_slave -- E:/MirServer/M2GameSvr/Server.conf`（见 `03`）。
- **oracle 缺陷照搬**：`ServerConf.cs:521` 的 `MasterDieMutinySpeed` 错读 `MasterDieMutinyPower` 键
  （现网读成 `0`）、`ServerConf.cs:533` 的 `MagTammingHPRate` 读 `MagTammingTargetHPRate` 键——各有一条专门用例。
- **改坏必红**：见 `05`（三处，rc=101；还原后 sha256 `0a06cc03…`）。
- **干净检出可复现**：见 `11`；`SHA256SUMS` 按 git blob 口径（任意检出 `--check-extra` GREEN）。
