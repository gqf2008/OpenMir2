# evidence-m3-mail-auction-closeout-20261013

卡 `openmir2-m3-mail-auction-align`（issue `4125cc72`，M3 逐系统对齐 ⑩ 邮件/拍卖）的 **离线收口登记**证据包。

## 本片做什么

owner 于 **2026-10-13 06:24** 在卡上落 comment **`1bd1361c`**（`kind=comment`，§6.0-3 口径重划）与
status **`1764853319b5`**，要求把两条重划**同时**写进：

1. `mir2-rs/tests/parity/whitelist.md` —— **邮件 = `oracle_absent`** 的新 `T-*`（本片编号 **T-36**）；
2. `mir2-rs/tests/parity/M3-report.md` —— §6 第 ⑩ 行**拆成邮件 / 拍卖两栏**。

本片即执行上述两条（外加把 owner 裁决作为 **§0.1** 补记进盘点报告，使该件自洽）；**不动实现写集**：
`src/` / `sql/` / `MirClient` / `mir2-rs/crates` / `mir2-rs/bins` 的 `git diff --name-only` **为空**（见 `05`）。
零 `.rs`、未起栈、未占端口。

> 本片**不**宣布卡片收口：DoD②③（夹具 = 承接卡 `-selloff-fixture-botsrv` + 真机两侧对拍）仍**未做**，
> 其中真机对拍**要窗口**，且阻塞于承接卡 `-selloff-consignment-world`(1) → `-selloff-goldsales-load-wire`(2) → `(3)`。

## 本片登记的两个口径（owner 裁决，非自行发明）

| 系统 | owner 裁决 | 本片落点 |
| --- | --- | --- |
| **邮件** | `oracle_absent`：oracle 全仓无邮件实现（只有客户端开关 `boShowMail`）⇒ **不开 `openmir2-m3-mail-*` 实现卡**，Rust 与 oracle 一致地不实现 | `whitelist.md` **T-36** + `M3-report.md` 10a 行 + §1.10 |
| **拍卖** | 按 oracle 实际语义重划为「**元宝寄售（SellOff）**」：判据 = 上架/成交/取消/退回逐项一致（含只读/坏列名/无 save 照搬） | `M3-report.md` 10b 行 + §1.10（承接卡 `-selloff-consignment-world` / `-selloff-goldsales-load-wire` / `-selloff-fixture-botsrv`） |

## 文件

| 文件 | 内容 |
| --- | --- |
| `00-commands.txt` | 本片命令（含 window/base、四门禁、边界、hash 复核） |
| `01-gate-fmt.txt` | `cargo fmt --all -- --check`（rc=0） |
| `02-gate-clippy.txt` | `cargo clippy --workspace --all-targets --offline -- -D warnings`（rc=0） |
| `03-gate-test.txt` | `cargo test --workspace --offline`（86 suites / 743 passed / 0 failed / 18 ignored，rc=0） |
| `04-gate-msgcodegen.txt` | `cargo run --offline -p msgcodegen -- --check`（604 条常量，rc=0） |
| `05-boundary-and-diff.txt` | `git diff --stat` + 边界（`src sql MirClient crates bins` = 空） |
| `06-owner-ruling.txt` | 卡上 owner 的 comment `1bd1361c` 与 status `1764853319b5` 原文（JSON） |
| `SHA256SUMS.txt` | 上述件的 hash（**仓库字节 = LF 归一 / git blob 口径**） |

## 复核命令（干净检出可复现）

```sh
python -I tools/evidence-hash/verify_sha256sums.py --check-extra mir2-rs/tests/parity/m3/evidence-m3-mail-auction-closeout-20261013
```

## 门禁读数（本片交付树）

- `cargo fmt --all -- --check` → **rc=0**
- `cargo clippy --workspace --all-targets --offline -- -D warnings` → **rc=0**
- `cargo test --workspace --offline` → **86 suites / 743 passed / 0 failed / 18 ignored**，rc=0
- `cargo run --offline -p msgcodegen -- --check` → **604 条常量**，rc=0
- `CARGO_TARGET_DIR=E:/tmp/m3cs-mailauction-target`（隔离；未在共享主检出编译）

## 残留物清单

- 本 worktree 未跟踪件（**非本片产生**，属 ②③ 真机跑会话，本片不动）：
  `mir2-rs/tests/parity/m3/sessions/{csharp-20261013-023941,csharp-20261013-024050,csharp-20261013-024249,rust-20261013-024454,rust-20261013-024604}/`。
- 仓外临时物：仅编译目标目录 `E:/tmp/m3cs-mailauction-target`（编译缓存，已登记）。
