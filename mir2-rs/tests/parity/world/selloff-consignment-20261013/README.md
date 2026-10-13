# M3⑩-实现 1：世界侧元宝寄售 —— 卡 `openmir2-m3-selloff-consignment-world`

- 分支：`selloff-world-host`（worktree `m2-svr`，principal `openmir2-worker-11`）。
- base（交付时 `refs/heads/master`）= `0f9fe9997a4ed717ee1361bfb2134329fd04472a`（交付 head 已 merge 它）。
- 实现 commit = `b0d0e432bb7da3fc6c7d1b0a8e5b59ca4b428280`；clippy 文档续行修复 = `70c195b860c66d32c233664ded6ef497cba3c45c`；
  **patch head = 本证据提交**（`git log -1 --format=%H -- mir2-rs/tests/parity/world/selloff-consignment-20261013`）。
- 写集：`mir2-rs/bins/game-svr/**`（`crates/world/**` 未改，理由见 `01-oracle-anchors.txt` §3 + 本文件 §2）。
- 边界见 [`20-boundary.txt`](20-boundary.txt)：`src/ sql/ MirClient / crates/protocol / crates/storage` = **空**。
- 窗口纪律：**未起栈、未占端口**（判据全在构造级/进程级 `cargo test`）。

## 0. 结论（先看这条）

按 `openmir2-m3-mail-auction-align` 盘点片（`M3⑩邮件拍卖现状盘点报告-20261013.md` §B）逐行落地：

1. **世界侧元宝寄售世界态**：`M2Share.SellOffItemList`（进程级）→ `SellOffBook`（宿主持有一实例、跨会话共享、
   零 `static`）；玩家 `SaleDeal`/`SellOffConfirm`/`SellOffItemList` → `SellOffState`（会话态，断线移除）。
2. **7 个 `CM_SELLOFF*` 派发**（`PlayObject.Message.cs:2057-2077`）走**生产路径** `handle_selloff_cmd` +
   `sim_loop` 接线；`RM_SENDDEALOFFFORM` 开窗成功支 + 12 个 `RM_*` → `SM_*` 帧（立即/排队两桶，优先级照 oracle）。
3. **oracle 缺陷照搬**（`01-oracle-anchors.txt` §5）：① `ClientBuySellOffItme` 首个空槽中止 ⇒
   `SM_SELLOFFBUY_OK` 恒不可达；② 金刚石标记写 65535/判 32767；③ 金刚石扣款帧错位；④ 取消退回的静默丢失分支；
   ⑤ `goldsales` 写面 oracle 调用点全注释 ⇒ 不实现。
4. **判据**：构造级 **23** + 进程级 **2** + 配置键 **1**；`cargo test --workspace --offline --no-fail-fast`
   = **95 suites · 992 passed · 0 failed · 21 ignored**（+ msgcodegen 604）。
5. **改坏必红 2 条**（RC-1 派发 / RC-2 缺陷①照搬），逐条 rc=101、还原后 ok（见 `redchecks/README.md`）。

## 1. 证据清单（SHA256 见 [`SHA256SUMS.txt`](SHA256SUMS.txt)）

| 文件 | 内容 |
| --- | --- |
| `01-oracle-anchors.txt` | oracle 行号锚点（派发/12 函数/世界态/队列优先级/缺陷照搬/未建模登记） |
| `02-change-summary.txt` | 写集与改前改后（含 base/head/合并） |
| `03-tests.txt` | 判据清单、命令与读数、未做项 |
| `20-boundary.txt` | 冻结面零改动（`src/ sql/ MirClient/ protocol/ storage` 空） |
| `gates/10-fmt.txt` `11-clippy.txt` `12-msgcodegen.txt` `13-workspace-test.txt` | 四门禁原始件 |
| `redchecks/` | RC-1 / RC-2 变异红件 + 还原 ok + README |
| `README.md` | 本文件 |

## 2. 为什么 `crates/world` 零改动（写集申报的说明）

卡写集允许 `crates/world/**` + `bins/game-svr/**`。实现后判定：oracle 的
`M2Share.SellOffItemList`（`M2Share.cs:79`）是**进程级静态列表**、`SaleDeal`/`SellOffConfirm` 是
**玩家会话散字段**（`PlayObject.cs:856/864`）——都不在地图/场景的权威模拟里，也不参与 AOI/tick。
本仓 ⑦-2 组队 / ⑦-3 交易同样落在 `bins/game-svr` 会话态（`SellOffBook` 与 `crate::trade`/`crate::party`
同形）⇒ 不把进程级全局或会话散字段塞进 `bevy_ecs` World（服务器端架构规范 §4.2.2「不得塞进 ECS」）。
⇒ `crates/world` 无改动是**设计结果**，非遗漏；若 reviewer 认为应改放 world，请给出与 `trade`/`party`
不一致的理由，owner 裁决。

## 3. 未做 / 待窗口（如实登记）

- **真机对拍**（`run_selloff_pair.ps1` + `selloff_diff.py` + `goldsales` 表快照 diff=0，同种子两侧）：
  需独占窗口 + `goldsales` 只读加载接线（⑩-2）⇒ 本片不起栈，登记待 owner 发窗口。
- `goldsales` **只读加载接线**归 `openmir2-m3-selloff-goldsales-load-wire`（⑩-2）；
  夹具归 `openmir2-m3-selloff-fixture-botsrv`（⑩-3，m3bag）。
- 脚本 label `@dealybme`/`sDealYBme` → 开窗：`crates/script` 不在本卡写集 ⇒ `open_deal_off_form`
  只落纯逻辑 + 帧（登记）。

## 4. 复核命令（干净检出可复现）

```sh
cd mir2-rs
export CARGO_TARGET_DIR=E:/tmp/m2svr-target
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo test --workspace --offline --no-fail-fast          # 95 suites · 992 passed · 0 failed · 21 ignored
cargo run --offline -p msgcodegen -- --check             # 604 条一致
cargo test -p game-svr --offline --lib selloff           # 23 passed
cargo test -p game-svr --offline --bin game-svr selloff  # 2 passed
```

哈希复核（仓库 LF/git-blob 口径）：

```sh
python tools/evidence-hash/verify_sha256sums.py verify \
  --dir mir2-rs/tests/parity/world/selloff-consignment-20261013 --check-extra
```
