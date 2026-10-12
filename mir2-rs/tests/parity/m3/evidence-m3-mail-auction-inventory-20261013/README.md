# evidence-m3-mail-auction-inventory-20261013 —— M3⑩ 邮件/拍卖现状盘点证据包

> 卡 `openmir2-m3-mail-auction-align`（M3 逐系统对齐 ⑩）；对应报告
> [`../M3⑩邮件拍卖现状盘点报告-20261013.md`](../M3⑩邮件拍卖现状盘点报告-20261013.md)。
> 仓库基线 `origin/master 670a009b`（本片分支 `worktree-m3-mail-auction`）。
> 本包**全部离线**：未起栈、未占端口、零 `.rs` / 零 `src` / 零 `sql`。

## 件数与口径

| 文件 | 内容 |
| --- | --- |
| `00-oracle-chain-lines.txt` | oracle 邮件（无实现）/ 元宝寄售（活）/ `goldsales` 落库 的调用链行号锚点（`rg -n` + `sed -n`，含全部 `SaveSellOffItemList` 注释点） |
| `01-rust-status-greps.txt` | Rust 现状只读 grep（协议常量 / DBSrv 5700 / storage trait / goldsales 加载器 / 世界侧派发 no match / 邮件 no match / 白名单） |
| `02-market-5700-and-goldsales-db.txt` | 5700 寄售行**死接线**证据（无 csproj 引用 / 现网未部署 / 表缺失 / WebApi 空桩）+ `goldsales` 列名 bug（D-5/B-5） |
| `03-fixture-reuse-survey.txt` | `run_pair.ps1` 两相 pin / `run_attack_pair.ps1` peer 护栏 / `scoped_diff.py --canonical` / BotSrv `M3*SEQ` 框架 / `openmir2-m3-clock-injection` |
| `04-gate-fmt.txt` | `cargo fmt --all -- --check`（空 = 干净）rc=0 |
| `05-gate-clippy.txt` | `cargo clippy --offline --workspace --all-targets -- -D warnings` rc=0 |
| `06-gate-test.txt` | `cargo test --offline --workspace` rc=0（**86 suites / 735 passed / 0 failed / 18 ignored**） |
| `07-gate-msgcodegen.txt` | `cargo run --offline -p msgcodegen -- --check` rc=0（604 条常量） |

## 复现命令（在本 worktree 的 `mir2-rs/` 下）

```bash
cat tests/parity/m3/evidence-m3-mail-auction-inventory-20261013/00-oracle-chain-lines.txt
cat tests/parity/m3/evidence-m3-mail-auction-inventory-20261013/01-rust-status-greps.txt
cat tests/parity/m3/evidence-m3-mail-auction-inventory-20261013/02-market-5700-and-goldsales-db.txt
cat tests/parity/m3/evidence-m3-mail-auction-inventory-20261013/03-fixture-reuse-survey.txt

cd mir2-rs
export CARGO_TARGET_DIR=E:/tmp/m3cs-mailauction-target
cargo fmt --all -- --check
cargo clippy --offline --workspace --all-targets -- -D warnings
cargo test --offline --workspace
cargo run --offline -p msgcodegen -- --check
```

## 复核（仓库字节 / LF 归一 / git-blob 口径）

```bash
python -I tools/evidence-hash/verify_sha256sums.py --check-extra \
  mir2-rs/tests/parity/m3/evidence-m3-mail-auction-inventory-20261013
```

## 关键结论（详见报告）

- **邮件：oracle 无任何实现**（全仓仅 `boShowMail` 客户端配置；无实体/帧/表/调用链）⇒ §6 的「发送/到期/退回」
  **无 oracle 行为可对齐**，属 `oracle_absent`，不得自行发明（请 owner 按 §6.0-3 重划，报告 §3 N1）。
- **拍卖 = 两套，均非「带出价/佣金」的拍卖行**：
  - **5700 `MarketSystem` 寄售行：死接线**——无 `.csproj` 引用、现网 `M2GameSvr` 未部署、`marketitems`/`marketitem`
    表不在冻结 schema、四分支全空桩、`MarketItem` 未注册 MemoryPack。**Rust M1 S4b 已 1:1 实现该面**（非缺口）。
  - **`元宝寄售`（`goldsales`）：活代码但只读且坏**——启动加载读**错列名** ⇒ 恒 0 行；`SaveSellOffItemList`
    **调用点全注释** ⇒ 永不写库。Rust `crates/data` 加载器已 1:1（D-5/B-5），**世界侧 7 个 `CM_SELLOFF*` 派发 +
    `DealOffInfo` 世界态 + 帧 = 未实现**（`rg` no match）。
- **§6 的「到期 / 流拍 / 佣金 / 出价」在 oracle 活路径不存在**（无超时列/无出价/无佣金；`MARKET_COMMISION`
  只属死模块）⇒ 判据须由 owner 重划（报告 §3 N2）。
- **DB 面已覆盖**：5700 由 M1 S4b（`bins/db-srv`，已 closed）；`goldsales` 由 `crates/data` 加载器（D 线）。
  本项 **无需新开 `bins/db-srv` 卡**。

## 复核器（hash 口径）

`SHA256SUMS.txt` 按**仓库字节口径（LF 归一 = git blob 字节）**记（`docs/Rust重写设计文档.md` §6.0-4）。
本包生成方式：`python -I tools/evidence-hash/verify_sha256sums.py write <dir>` 回填（默认 repo 口径）
→ `verify --check-extra` 复核 GREEN。
