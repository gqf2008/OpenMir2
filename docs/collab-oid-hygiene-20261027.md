# collab 条目里的 oid：读侧机判 + 写侧 fail-closed（2026-10-27）

卡：`openmir2-collab-oid-resolvability-gate`（tier=L）。

## 为什么

walgit **对条目里的 oid 零校验**：短号、手补长号都会**原样落库**，`thread` / `board` / `report`
三视图全绿。2026-10-27 一晚，同一类错误在**五个入口**各犯一次、**四个人**：

| # | 入口 | 形态 | 后果 |
|---|---|---|---|
| 1 | review 的 `head` | 手补 31 位（前 9 位对、后 31 位臆造） | 那条恰好是 `request_changes`，只加强结论；**换成 `approve` 会让合并记录指向不存在的提交** |
| 2 | patch 的 `head` | 同上 | 评审读不到 diff |
| 3 | `--parent` | 截成 9 位 | 服务端照收、三视图不报错（`parent 孤儿隐形`） |
| 4 | `--parent` | 用户自己重签 | 同上 |
| 5 | `--base` / `--head` | 9 位短号原样进 `entry.refs` | `entryrefs.require_*` 会判「不是 40 位小写 hex」 |

评审与合并的全部可复算性都建立在「oid 指向真实对象」上 ⇒ 两头都要堵。

## 读侧：`tools/collab/oid_gate.py check`

```sh
python -I tools/collab/oid_gate.py check            # 默认扫最近 200 条
python -I tools/collab/oid_gate.py check --limit 1000 --json out.json
```

- **扫什么**：`refs/collab/inbox/**` 最近的条目（默认 200，`--limit` 可配），取条目里处于
  **commit 位置**的 oid：`entry.refs.base` / `entry.refs.head`、`body.base` / `body.head` /
  `body.oid` / `body.merged*` / `body.reviewed_head` / `body.merge_oid`，外加 `entry.parent`
  （**条目 oid = blob 对象**）。
- **怎么判**：commit 位置 ⇒ `git cat-file -e <oid>^{commit}` 必须成功；`parent` ⇒ 对象必须存在
  （条目 oid 是 blob）。
- **四态**：解析不到 ⇒ **rc=1**（点名 principal / 线程 / 字段 / 值 / 条目）；
  git 不可用 ⇒ **rc=2**；解析到 blob/tree（不该在 commit 位置）⇒ **跳过并计数**；全通过 ⇒ rc=0。
- **短 oid 放行并记 info**：今晚出事的从来不是「写短号」，是「**手补长号**」；
  空串 / 缺字段才判红（空 ≠ 短）。`parent: ""`（根条目）与 `merged: ""`（未合并）按约定跳过。
- **必须打印扫描条数**：`扫描 N 条条目 / M 个 oid：…`；**扫到 0 条 ⇒ rc=2**（扫了 0 个不许算绿）。
- **只报不改**：历史条目是签名事实，发现问题只点名，绝不回写。

**首次实跑（最近 300 条）**：`扫描 300 条条目 / 614 个 oid：可解析 612（短 oid 203）、解析不到 2`
——两处都是 `openmir2-dev-2` 的 `parent` 写成了 40 位臆造值（`4da5b01a…`、`66fda9a7…`）。
按卡的 forbidden「**只报不改**」，仅在此登记，不回写历史。

## 写侧：`tools/collab/sign_entry.sh`（fail-closed 包装）

```sh
tools/collab/sign_entry.sh --repo . --kind patch --id <thread> \
    --parent <entry-oid> --base <oid> --head <oid> --body-file /tmp/body.json \
    [--push walgit] [--check-only]
```

- `--base` / `--head` ⇒ 调 `oid_gate.py verify`：**必须 40 位小写 hex 且解析到 commit**；
- `--parent` ⇒ 调 `oid_gate.py verify-entry`：**必须非空 hex 且对象存在**；
- 任一不满足 ⇒ **rc=1，拒绝调用 walgit**（条目根本不会被写坏）；
- 通过后把**解析后的全 oid** 传给 `walgit collab entry`（短号也在这一步被补全，而不是手补）。

## 怎么保证团队都走这条路（可强制执行的部分）

1. **读侧目前是「自测已登记 + 真仓库 check 未接入门」**（review C1，2026-10-27）：
   `tool_selftest_gate` 的 REGISTRY 里跑的是 **`oid_gate --selftest`（合成夹具）**，不是真仓库的 `check`。
   **所以「写坏的条目会让工具门红」在当前注册下不会发生**——工具存在、牙存在、注册存在，
   但真正的判据没有被任何门调用。真仓库 `check` 现在是**报告模式**，要人工跑：
   ```sh
   python -I tools/collab/oid_gate.py check --limit 200
   ```
   首跑（最近 300 条）发现 **2 处历史 `parent` 指向不存在对象**（`openmir2-dev-2`，见上表下段），
   按「只报不改」登记在案；**谁/何时/以什么 rc 处置**由后续卡 `openmir2-oid-gate-realcheck-baseline`
   决定（要求 `--baseline <file>` 或 `--since` 只查新条目，**不许一刀切忽略全部 parent**、
   也不许直接接进门——历史红若永久存在，门就没人看了）。
2. **写侧建议统一走 `sign_entry.sh`**：它把「拒绝短号」变成工具行为；手工 `walgit collab entry`
   仍然可能写坏，此时**只有人工跑 `check` 才会点名**（这不是门，别当门用）。
3. 新增任何接受 oid 的脚本：一律先调 `oid_gate.py verify` / `verify-entry` 再落库，
   不要自己写 `len(x) == 40` 之类的半套校验。

## 与 `entryrefs` 的分工

- `entryrefs.py`：**读**条目时该从哪个字段取（`entry.refs.head`，不是 `body.head`），fail-closed；
- `oid_gate.py`：**写**进去的值是否真实存在（可解析）。
两者互补，docstring 互相引用。
