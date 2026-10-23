# 门禁基线快照（master `478ff9a33`）

- 卡：`openmir2-gate-baseline-snapshot`（tier=L）；分支 `card/openmir2-gate-baseline-snapshot`；写集 `docs/`。
- **本文件所有数字只对 commit `478ff9a33` 有效**（2026-10-24 实测）。master 一动即过期，引用前先 `git rev-parse --short walgit/master`。
- 用途：条件④「门禁恒绿」的现取证据；也是 `docs/needs-human-20261024.md` 里门禁数字的对照源（另在 `openmir2-needs-human-count-sync` 卡留了回指 comment）。
- 环境：Windows / PowerShell 7 + Git Bash；工作树 `E:/Users/gxh/Documents/GitHub/OpenMir2-wt-cronclaim`（clean，HEAD=`478ff9a33`）。
- **本卡只观测**：未改任何文件（除本快照）；红项按「如实记 + 承接建议」处理（本次无红项）。

## 1. 总表

| # | 门禁 | 命令（仓库根执行，Rust 的在 `mir2-rs/`） | rc | 关键计数 |
|---:|---|---|---:|---|
| 1 | 格式 | `cd mir2-rs && cargo fmt --all -- --check` | **0** | — |
| 2 | Clippy | `cd mir2-rs && cargo clippy --workspace --all-targets -- -D warnings` | **0** | 0 warning |
| 3 | 测试 | `cd mir2-rs && cargo test --workspace` | **0** | **141 suites / 3153 passed / 0 failed / 62 ignored** |
| 4 | 消息号漂移 | `cd mir2-rs && cargo run -q -p msgcodegen -- --check` | **0** | — |
| 5 | T 预约表 | `python -I -B tools/whitelist/t_reservation_gate.py check` | **0** | 预约表 **255** 行 / whitelist **255** 号；max **T-267** |
| 6 | 锚点门 | `python -I -B mir2-rs/tests/parity/m3/whitelist_anchor_gate.py check` | **0** | **589** 条锚 ALL PASS |
| 7 | m3 白名单注册审计 | `python -I -B mir2-rs/tests/parity/m3/m3_whitelist_registry_audit.py audit` | **0** | GREEN（无重复编号） |
| 8 | GM 注册表 | `python -I -B mir2-rs/tests/parity/coverage/gm_registry.py gate` | **0** | **74** 条与源码逐字段一致 |
| 9 | GM 注册表自测 | `…/gm_registry.py selftest` | **0** | 14 teeth |
| 10 | 脚本宿主 no-op 台账 | `python -I -B mir2-rs/tests/parity/coverage/script_host_noop_audit.py check --strict-lines` | **0** | **168** 行（safe 67 / needs_card 99 / covered 2） |
| 11 | no-op 台账自测 | `…/script_host_noop_audit.py selftest` | **0** | GREEN |
| 12 | `card_window` 自测 | `python -I -B tools/conformance/card_window.py --selftest` | **0** | **234/234** |
| 13 | `mir2_probe` 自测 | `python -I -B tools/probe/mir2_probe.py --selftest` | **0** | **11 checks** |
| 14 | 变异牙清单 | `python -I -B tools/conformance/selftest_mutations.py --list` | **0** | **51** 颗（只数清单；**未跑**，见 §3） |
| 15 | `goal_gate` 自测 | `python -I -B tools/goal/goal_gate.py selftest` | **0** | **70** 条断言 / 70 通过 |
| 16 | 冲突标记 | `python -I -B tools/hygiene/conflict_marker_gate.py` | **0** | **0** markers / **7449** tracked |
| 17 | 合并树 | `git merge-tree --write-tree walgit/master HEAD` | **0** | 无冲突；HEAD=master |
| 18 | 规则表规模（参考） | `python -I -c "import json;d=json.load(open('tools/conformance/rules.json'));print(len(d['rules']))"` | **0** | **45** 条规则 |
| 19 | T 表规模（参考） | `grep -c '^\| T-' mir2-rs/tests/parity/whitelist.md` | **0** | **255** 行 |

## 2. 与卡面预期数字的差异（如实记）

卡面 goal 写「`rules.json` 42→45、`card_window` 226→237、`mir2_probe` 12→13 checks、anchors 583→589、T 表 255」。在 `478ff9a33` 实测：

| 项 | 卡面 | 实测 `478ff9a33` | 说明 |
|---|---|---|---|
| `rules.json` | 45 | **45** | 一致 |
| anchors | 589 | **589** | 一致 |
| T 表 | 255 | **255**（max T-267） | 一致 |
| `card_window --selftest` | 237 | **234** | 卡面数字取自某条**未合入**分支（inventory 卡返工在 worker-73 手上，其 +3 未合）；master 上是 234 |
| `mir2_probe --selftest` | 13 | **11** | 同上：卡面把未合入分支的 +2 算进来了；master 上是 11 |

⇒ 引用门禁数字时**以本表为准**，不要用卡面/报告里的「预期」数字。

## 3. 未做与原因

- **`selftest_mutations.py` 全量未跑**：本机并行/串行跑多颗牙会因内存压力被系统终止（今天两次）；本卡只需「牙数」，故用 `--list` 计数（**51**）。若要「全红」证据，请单卡单跑、逐牙 `--only`，并在跑前确认无其它车道在跑变异。
- 未起栈、未连库、未占端口（本卡只读仓库与账本）。

## 4. 复算

把 §1 的命令按行照抄即可；所有脚本只读仓库。`cargo test --workspace` 是最慢的一项（本次约 3–5 分钟）。

## 5. 残留

- 本快照对 `478ff9a33` 有效；master 前进后请在新卡上重打（或按本文件命令逐条复跑）。
- 卡面数字与实测不符的两项（`card_window` 237 vs 234、`mir2_probe` 13 vs 11）已在本文件点名；若后续合入 inventory 返工，数字会再变，属于预期。
