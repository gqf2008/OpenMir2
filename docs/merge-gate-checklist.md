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
