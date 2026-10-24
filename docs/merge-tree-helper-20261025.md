# `merge_tree_helper.py`：patch body 的 merge_tree 由脚本生成（tier L）

- 卡：`openmir2-merge-tree-helper`（tier **L**，owner `openmir2-dev-2`，branch `card/merge-tree-helper`）。
- 基线：`master = c703f6a03`（2026-10-25）。
- 写集：`tools/hygiene/merge_tree_helper.py`（新）、`docs/team-20261024.md`（规则第 10 条一行）、本文档。
- 背景：复核连续发现 4 张卡的 patch body 手算 `merge-tree --write-tree` 记错
  （frame-queue×2 / 税 / inbound），评审无法按 body 复算。

## 1. 用法

```sh
python -I -B tools/hygiene/merge_tree_helper.py --base <base> --head <head>
python -I -B tools/hygiene/merge_tree_helper.py --base <base> --head <head> --json
```

输出（文本模式，逐行可 grep）：

```text
MERGE_TREE base=<40hex> head=<40hex> master=<40hex>
MERGE_TREE vs_base tree=<40hex> rc=<0|1>
MERGE_TREE vs_master tree=<40hex> rc=<0|1>
MERGE_TREE same=<yes|no>
```

- `vs_base` = `git merge-tree --write-tree <base> <head>`；`vs_master` = 对
  主源（默认 `refs/remotes/walgit/master` → `master`，可 `--master` 覆盖）。
- **两个值都输出**，正好覆盖 4 张卡混淆的「对 base / 对当前 master」。
- 冲突时 `rc=1` 且仍输出 tree oid（冲突 tree）；`--strict` 可让本进程 rc=1。
- `--json` 输出 `format=merge-tree-helper/1`，含 `repo/base/head/master/vs_base/vs_master/same`。
- 工具只调用 git，不改 git 行为；git 错误（非 0/1）⇒ 本进程 rc=2（fail-closed）。

## 2. 自测（`--selftest`，临时仓库）

```text
PASS clean 1：base→clean1 rc=0 且 tree 为 40hex
PASS clean 1：tree 与 git rev-parse 逐字一致
PASS clean 2：clean1 vs clean2 两个参照 rc=0
PASS clean 2：tree 与直接 git merge-tree 逐字一致
PASS conflict：c1 vs c2 两个参照 rc=1
PASS conflict：仍输出 tree oid（冲突 tree）
PASS conflict：两个参照 tree/rc 相同 ⇒ same=yes
GREEN: merge_tree_helper selftest 7/7
```

覆盖：两 clean 例（ff + 非冲突双分支）+ 一 conflict 例；clean 的 tree 与
`git rev-parse <head>^{tree}` / 直接 `git merge-tree` 输出逐字一致。

## 3. patch body 模板

```json
"merge_tree": {
  "vs_base":   {"tree": "<脚本输出>", "rc": 0},
  "vs_master": {"tree": "<脚本输出>", "rc": 0}
}
```

评审复算：`python -I -B tools/hygiene/merge_tree_helper.py --base <patch base> --head <head> --json`，
值必须与 body 逐字一致（`docs/team-20261024.md` 规则 10）。

## 4. 门禁 / 残留

- 纯 Python 工具 + 文档：卫生三门禁 + `git diff --check` + merge-tree（脚本自身输出）见 patch body。
- 未改既有 git 行为、未改门禁脚本；`--selftest` 用临时目录，结束即删。
- 不 merge、不自审。
