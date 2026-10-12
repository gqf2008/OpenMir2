# evidence-m3-mail-auction-minors-20261013 — M3⑩ 收口片两条非阻塞 Minor 的措辞订正证据

- **卡**：`openmir2-m3-mail-auction-align`（issue `4125cc72`）；本片 = 清 reviewer-5 在 approve
  （review oid `a4825e35`，`reviewed_head = 5cd950a6`）里给的两条**非阻塞 Minor**。
- **车道**：principal `openmir2-worker-5` ／ worktree `.claude/worktrees/m3cs` ／ branch `worktree-m3-mail-auction`。
- **base**：`origin/master e2b94234`（= `5cd950a6` 的 master 合并提交；本片先 `git merge origin/master` 跟齐后开工）。
- **写集**：仅 `mir2-rs/tests/parity/M3-report.md` + `mir2-rs/tests/parity/whitelist.md` + 本证据包。
  **零 `.rs` / 零 `src/**` / 零 `sql/**` / 零 `MirClient`**；**未起栈、未占端口**。

## 改了什么（逐条）

| Minor | 位置 | 改前 → 改后 |
| --- | --- | --- |
| **M-1** 计数口径易误算 | `M3-report.md` 顶部口径句 / ⑩ 拆栏说明 / §4 索引「总判」行（3 处） | 四数「3 covered / 2 partial / 1 oracle_absent / 5 not_covered」后补明 **⑩ 拆 10a/10b 两个子行、合计仍计作一项 ⇒ 11 子行 = 十项**，并点名 **⑩ 为混合行 = 10a `oracle_absent` + 10b `partial`**。数据未变，仅补口径说明。 |
| **M-2** T-36 取证括注不严谨 | `whitelist.md` T-36 行 | `rg -i 'mail' src/ --glob '*.cs'`（**仅配置开关**）→ （**无游戏邮件实体**；命中仅客户端界面开关 `ShowMail`/`boShowMail` + 账号保护 `EMail`，后者归 `sql/account_protection`、非游戏邮件）。 |

> 逐条原文与处置见 `06-review-findings.txt`（含本片 unified diff）；计数 / 边界见 `05-boundary-and-diff.txt`。

## 四门禁（实跑，隔离 `CARGO_TARGET_DIR=E:/tmp/m3cs-mailauction-target`，`--offline`）

| 门 | 命令 | 原始件 | 结果 |
| --- | --- | --- | --- |
| fmt | `cargo fmt --all -- --check` | `01-gate-fmt.txt` | rc=0（无输出） |
| clippy | `cargo clippy --workspace --all-targets --offline -- -D warnings` | `02-gate-clippy.txt` | rc=0 |
| test | `cargo test --workspace --offline` | `03-gate-test.txt` | rc=0；**86 suites / 759 passed / 0 failed / 18 ignored** |
| msgcodegen | `cargo run --offline -p msgcodegen -- --check` | `04-gate-msgcodegen.txt` | rc=0；604 条常量 |

## 复核命令（仓库字节 = LF 归一 / git blob 口径）

```sh
python -I tools/evidence-hash/verify_sha256sums.py --check-extra mir2-rs/tests/parity/m3/evidence-m3-mail-auction-minors-20261013
```

## 范围声明

- 本片**不**实现拍卖（元宝寄售 SellOff）世界侧派发 / 世界态 / 帧（那是承接卡
  `-selloff-consignment-world` / `-selloff-goldsales-load-wire` / `-selloff-fixture-botsrv` 的工作，属**未实现**，非本片范围）。
- 本片**不**宣布卡 `openmir2-m3-mail-auction-align` 的 DoD②③ 达成：DoD②③（元宝寄售夹具 + 真机两侧对拍）仍**要窗口**，
  且阻塞于上述三张实现卡；卡片保持 `in-progress`。
- 未自签 review、未 merge。
