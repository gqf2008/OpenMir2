# 合并门禁清单（Leader 实际执行口径）

> **为什么有这份文件**：`AGENTS.md`「本地门禁」只列了 fmt / clippy / test / msgcodegen 四条，
> 而实际合并还跑卫生门、工具自测门与 merge-tree。**靠记性就会漏**——2026-10-25 就漏了
> `tool_selftest_gate`，直接导致 master 变红（tick `5050a7ec`，由 tester-1 发现）。
> 本清单是 Leader 合并时**逐条实跑**的口径；`AGENTS.md` 未列不代表不用跑。

## 1. 编译/测试面（在 `mir2-rs/` 下跑；**仓库根没有 Cargo.toml**）

| # | 命令 | 判据 |
|---:|---|---|
| 1 | `cargo fmt --all -- --check` | rc=0 |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 |
| 3 | `cargo run -q -p msgcodegen -- --check` | rc=0 |
| 4 | `cargo test --workspace` | rc=0，并记 **running = passed + ignored** 三元组与 suite 数（单报一个数必对不上） |

## 2. 卫生门（仓库根）

`conflict_marker_gate` / `stale_pointer_gate` / `dynamic_count_stamp_gate` /
`docs_delivery_gate` / `board_coverage_gate` —— 逐条 rc=0。

## 3. 工具自测门（**2026-10-25 漏过这条**）

> 2026-10-26 起它已是 `goal_gate` 条件④ 的**第 8 项**（含 `--inventory` 未分类检查），`--full`/快速模式都会跑；本清单保留为**手工复核**入口（reviewer 自己重跑，不采信作者自述）。

```sh
python -I -B tools/hygiene/tool_selftest_gate.py --inventory   # 必须 rc=0：无「未分类」
python -I -B tools/hygiene/tool_selftest_gate.py               # 全跑必须全绿
```

**新增带 selftest 的脚本 ⇒ 必须同时登记进 REGISTRY**，否则 `--inventory` 报「未分类」rc≠0。

## 3.5 m3 门（**只要改动涉及 `mir2-rs/**` 就必须跑**）

> 2026-10-27 漏过这一节：合 `turn-order-login`（`f0d44ceeb`）时只跑了 §2 的四道门，
> 而该卡给 `bins/game-svr/src/smframe.rs` 加了约 150 行，把 `sm_skeleton` 从 2012 顶到 2081，
> `whitelist.md` 的 T-206 锚当场漂移 ⇒ **master 上 `whitelist_anchor_gate` RED**，
> 由 tester-1 独立复现发现，`1e6ab9f89` 用 follow-up 修掉（不改已推送的合并提交）。

```sh
python -I -B mir2-rs/tests/parity/m3/whitelist_anchor_gate.py check    # RESULT: ALL PASS + 锚条数
python -I -B mir2-rs/tests/parity/m3/m3_whitelist_registry_audit.py audit   # WHITELIST_REGISTRY GREEN
```

判据：

- `check` ⇒ **`RESULT: ALL PASS`**，并**记下锚条数**（与卡的 base 比，只应增加本卡新登记的那些）；
  出现 `ANCHOR_DRIFT` = 有人的行号锚被别的改动顶走了，**必须当次修**，不能「下次再说」。
- `audit` ⇒ `WHITELIST_REGISTRY GREEN`（`T-*` 编号不重复）。
- **`HIGH_CHURN` 清单里有名字的文件禁止写 `path:<line>` 锚**（门会报 `ANCHOR_LINE_NOT_ALLOWED`）——
  `lib.rs` / `world.rs` / `entity.rs` / `combat.rs` 都在清单里；`smframe.rs` 目前**不在**，
  但它刚被证明是增长文件（卡 `openmir2-smframe-high-churn-anchor`）。

## 4. 结构面

```sh
python -I -B tools/hygiene/merge_tree_helper.py --base <base> --head <head>   # same=yes
git diff --check <base> <head>                                                # rc=0
```

## 5. 两条纪律

1. **rc 不取管道尾**：`cmd | tail` 之后的 `$?` 是 `tail` 的。要判 rc 就把输出重定向到文件再 `echo $?`。
2. **写集核对用 merge-base**：`git diff --stat $(git merge-base HEAD master) HEAD`，不要用 `git status`；
   对**在飞的分支**取 base 时要确认它的父提交是哪个（今天有一次按旧基线取 diff，差点把 Leader 自己的提交误判成车道越界）。

## 6. 合并后的自证（不是可选项）

- 合并后**在 master 上**复跑该卡涉及的工具自检（不是只在卡分支上跑）；
- 若卡改了某个门/脚本，跑一次**该门的反向用例**，确认它在 master 上也成立；
- 把 `merge_result` 里的门禁结果写成**可复算**的形式（命令 + rc + 计数）。
