# `merge_tree_helper` 纳入门禁调用面（tier L）

- 卡：`openmir2-merge-tree-helper-gate`（tier **L**，owner `openmir2-dev-2`，branch `card/merge-tree-helper-gate`）。
- 基线：`master = a300d9351`（2026-10-25）。
- 写集：`.walgit/ci.toml`（新增一条 fast task）、本文档。
- 来源：tester-1 提示 `tools/hygiene/merge_tree_helper.py` 不在 7 项门禁调用面内。

## 1. 路线选择：**甲（`.walgit/ci.toml` 新增 fast task）**

取证：

- `tools/hygiene/` 下**没有**聚合脚本（7 个门禁是独立 Python 文件；`grep -rln "run_all|aggregate" tools/hygiene` 无命中）。
- 现行门禁调用面 = `.walgit/ci.toml` 的 `fast-*` / `rust-*` 任务（`walgit ci validate` 的 §3.1 声明面）；
  另加本地手工跑的 7 项，但没有“卫生聚合”入口。
- 因此**乙不可用**（无既有聚合可并入）；选甲：新增独立 fast task，只跑 helper 的 `--selftest`。

## 2. 新增任务

```toml
[[task]]
name = "fast-merge-tree-helper"
command = "python -I -B tools/hygiene/merge_tree_helper.py --selftest"
refs = ["refs/heads/master"]
schedule = "@hourly"
timeout = "2m"
```

- 只覆盖 helper 自身退化（`--selftest` 在**临时仓库**跑两 clean + 一 conflict + R-4；
  不读/不改本仓库内容）⇒ 不构成「仓库内容强门」，符合卡面 forbidden。
- `refs` 只钉 `refs/heads/master`（与其它 fast task 的 master 面一致；不绑定已删除的旧卡分支）。

## 3. 实跑

```text
$ python -I -B tools/hygiene/merge_tree_helper.py --selftest
GREEN: merge_tree_helper selftest 12/12          # rc=0

$ walgit ci validate --repo .
  fast-merge-tree-helper refs [refs/heads/master] timeout 120s attempts 1 env_allow [] schedule "@hourly"
    python -I -B tools/hygiene/merge_tree_helper.py --selftest
# rc=0
```

## 4. 门禁 / 残留

- 卫生三门禁（conflict/stale/dynamic）+ `git diff --check` rc=0；merge-tree 由脚本生成（见 patch body）。
- 未改 `merge_tree_helper.py` 行为、未升级为仓库内容门；未改卡片状态。
- 不 merge、不自审。
