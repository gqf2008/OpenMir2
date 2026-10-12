# evidence-m3-hero-pet-inventory-20261013 —— M3⑨ 英雄/宠物现状盘点证据包

> 卡 `openmir2-m3-hero-pet-align`（M3 逐系统对齐 ⑨）；对应报告
> [`../M3⑨英雄宠物现状盘点报告-20261013.md`](../M3⑨英雄宠物现状盘点报告-20261013.md)。
> 仓库基线 `origin/master 304126ae`（本片分支 `worktree-m3-hero-pet-align`）。
> 本包**全部离线**：未起栈、未占端口、零 `.rs`。

## 件数与口径

| 文件 | 内容 |
| --- | --- |
| `00-oracle-chain-lines.txt` | oracle 宝宝/召唤/升级/阵亡/经验/上限 调用链行号锚点（`rg -n` + `sed -n`） |
| `01-rust-status-greps.txt` | Rust 现状只读 grep（[1] 常量 … [12] whitelist T-32/T-33；召唤/宝宝列表/等级/叛变 全 no match） |
| `02-frames-config-and-db.txt` | C#↔Rust 帧常量对照 + 配置默认值/现网 `Server.conf` + 落库事实 |
| `03-fixture-reuse-survey.txt` | `run_pair.ps1` / `run_attack_pair.ps1` peer 护栏 / `drop-exp` 夹具 / BotSrv 现成件调查 |
| `04-gate-fmt.txt` | `cargo fmt --all -- --check`（空 = 干净）rc=0 |
| `05-gate-clippy.txt` | `cargo clippy --offline --workspace --all-targets -- -D warnings` rc=0 |
| `06-gate-test.txt` | `cargo test --offline --workspace` rc=0（**86 suites / 735 passed / 0 failed / 18 ignored**） |
| `07-gate-msgcodegen.txt` | `cargo run --offline -p msgcodegen -- --check` rc=0（604 条常量） |

## 复现命令（在本 worktree 的 `mir2-rs/` 下）

```bash
cat mir2-rs/tests/parity/m3/evidence-m3-hero-pet-inventory-20261013/00-oracle-chain-lines.txt
cat mir2-rs/tests/parity/m3/evidence-m3-hero-pet-inventory-20261013/01-rust-status-greps.txt
cat mir2-rs/tests/parity/m3/evidence-m3-hero-pet-inventory-20261013/02-frames-config-and-db.txt
cat mir2-rs/tests/parity/m3/evidence-m3-hero-pet-inventory-20261013/03-fixture-reuse-survey.txt

cd mir2-rs
export CARGO_TARGET_DIR=E:/tmp/m3cs-heropet-target
cargo fmt --all -- --check
cargo clippy --offline --workspace --all-targets -- -D warnings
cargo test --offline --workspace
cargo run --offline -p msgcodegen -- --check
```

## 复核（仓库字节 / LF 归一 / git-blob 口径）

```bash
python -I tools/evidence-hash/verify_sha256sums.py --check-extra \
  mir2-rs/tests/parity/m3/evidence-m3-hero-pet-inventory-20261013
```

## 关键结论（详见报告）

- oracle **无玩家英雄实体**（无 `HeroObject`/`PetObject`；`HeroActor` 只在 BotSrv；`hero*` 配置是机器人）；§6 的
  「英雄/宠物」= **宝宝/下属 slave**（`Master`+`SlaveList`+`MakeSlave`），**不落库**（唯一跨服载荷 `SwitchDataInfo.SlaveArr`）。
- 「英雄经验」= `ExpHitter.Master`（宝宝击杀经验给主人，`BaseObject.cs:2434-2437`）——复核 **T-33 成立**。
- **新发现（死代码）**：`MonsterObject.GainSlaveExp`（宝宝按击杀数升级）全仓**只有定义、无调用点** ⇒ 忠实照搬 =
  宝宝等级**不**由击杀增长（召唤/跨服时设定）。
- Rust：主从链接/目标/AOI/掉落归属/诱惑之光/脚本宝宝计数 **已实现**；召唤 17/30/41/46、宝宝列表/等级/颜色、
  叛变/阵亡清理、经验给主人、配置项 **未实现**；「无英雄」「不落库」**与 oracle 一致**（非缺口）。

## 复核器（hash 口径）

`SHA256SUMS.txt` 按**仓库字节口径（LF 归一 = git blob 字节）**记（`docs/Rust重写设计文档.md` §6.0-4）。
本包生成方式：先建清单条目 → `python -I tools/evidence-hash/verify_sha256sums.py write <dir>` 回填
（默认 repo 口径）→ `verify --check-extra` 复核 GREEN。
