# 片 ㊷ 证据：`MIR2_CLIENT_ABILITY_LAYOUT` 起栈管线（卡 `openmir2-client-e2e-enter-gaps`）

**tier = L**（纯工具 + 只读启动日志，零模拟/出帧语义改动）。
**base** = `origin/master` @ `68b8af6b1`（片㊶ 已合入）。

## 1. 这一片解决什么

生产默认（`bdd3a2ae9`）为帧 52/53 走**冻结客户端布局**（52 58B / 53 12B 尾）——真客户端可玩所必需。
但 M2 验收③（`s2c_diff.py` vs 金标准）比 `ident=52` 的 `body_len/body_sha256/raw_hex`，金标准是
**oracle 布局**（`body_len=40`）。`start_rust_stack.ps1` 此前没有 `MIR2_CLIENT_ABILITY_LAYOUT` 管线
⇒ 起栈默认 58B ⇒ **③ 在 52 上必红，且看不出原因**。

⇒ 本片给起栈器补**显式、可审计**的布局透传，并把「③ 对拍必须 `=0`、真机①②必须默认」写进 README §0b.4。
完整锚点与推导见 `01-oracle-anchors.txt`。

## 2. 变更（5 文件 + 1 新判据脚本）

| 文件 | 变更 |
| --- | --- |
| `tools/m2-acceptance/rust_seed_plumb.ps1` | `Resolve-Mir2ClientAbilityLayout` + `Get-ClientAbilityLayoutInjectedLine` + `Start-SeededProcess -ClientAbilityLayout`（注入/还原） |
| `tools/m2-acceptance/start_rust_stack.ps1` | `-ClientAbilityLayout` 参数 + 解析（fail-closed）+ 打印 + 传给 game-svr |
| `tools/m2-acceptance/check_client_ability_layout_plumb.ps1` | **新**判据脚本（20 条，含三个真 game-svr 起栈） |
| `tools/m2-acceptance/README.md` | 新增 §0b.4（③ 对拍必须 `-ClientAbilityLayout 0`） |
| `mir2-rs/bins/game-svr/src/smframe.rs` | `client_layout_probe_line()`（只读自证行） |
| `mir2-rs/bins/game-svr/src/main.rs` | 启动打一行 `CLIENT_ABILITY_LAYOUT layout=oracle-40|client-58` |

**零行为差**：不传 `-ClientAbilityLayout` 且未设环境变量 ⇒ 不注入、不打 `RUST_CLIENT_ABILITY_LAYOUT_INJECTED`
行；生产默认仍是冻结客户端布局（③c/③d 两向真栈实测）。

## 3. 证据四件套

| 件 | 文件 | 读数 |
| --- | --- | --- |
| 命令 + 输出 | `02-selftest.txt` | `SUMMARY total=20 ok=20 fail=0` ⇒ `CLIENT_ABILITY_LAYOUT_PLUMB GREEN` rc=0 |
| 改坏必红 | `03-redcheck.txt` | 删注入行 ⇒ ②a/②c/③a/⑤b FAIL，`total=20 ok=16 fail=4` rc=1；还原 sha256 逐字节回原 |
| 四门禁 | `04-four-gates.txt` | fmt rc=0 / clippy rc=0 / 101 suites 1231 passed 0 failed 22 ignored / msgcodegen 604 rc=0 |
| 边界 | `05-boundary.txt` | 5 改 + 2 新增；`src/ sql/ MirClient/ crates/protocol/ crates/storage/` 空；非本片 `.rs` 仅 game-svr 两文件 |
| 锚点 | `01-oracle-anchors.txt` | 金标准窗口 `ident=52 body_len=40`；客户端 `HIGHHP=True` 四个发点 |

判据脚本复跑命令：

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_client_ability_layout_plumb.ps1 `
  -GameSvrExe E:\tmp\m2svr-target\debug\game-svr.exe
```

## 4. 残留物清单

* `E:/tmp/m2svr-target/**`：本片编译 target（隔离 target，跨片复用）。
* `E:/tmp/m2svr-cal-check/`、`E:/tmp/m2svr-cal-check-mut/`：判据脚本的临时 work（free-port 临时 conf + 日志；
  脚本自行 `taskkill /T` 收进程，不占 5000/7000/7100/7200）。
* `E:/tmp/m2svr-test.txt`：`cargo test --workspace` 全文（`04-four-gates.txt` 只摘录摘要）。
* 本证据目录：仓库内入库件（LF/git-blob 口径 hash 见 `SHA256SUMS.txt`）。
* 零起栈、零游戏端口；共享主检出零改动；不 merge、不自签 review。
