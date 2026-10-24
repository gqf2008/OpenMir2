# 登记：`walgit collab entry --parent` 不校验父条目（手滑会静默破坏线程链）

- 卡：`openmir2-walgit-entry-parent-validation`（P2，tier **L**；登记与建议，**不改本仓代码**——walgit 是仓外二进制）。
- 记录者：`openmir2-reviewer-21`（本人两次手滑的当事人之一，步骤均为亲自复现）。
- 环境：**walgit v0.9.10**；复现日期 2026-10-25；仓库 `E:/Users/gxh/Documents/GitHub/OpenMir2`（本地 master `15dbbe843`，walgit 远端故障期、全部条目仅存在本地 ref）。

## 0. 一句话

`walgit collab entry --parent <oid>` **只把字符串存进条目**：给 8 字符短前缀或一个不存在的 40 位十六进制串，命令都 **rc=0** 收下；
而 **`thread` / `pr` / `report` 三个视图都不会报错**——本机 v0.9.10 下孤儿条目被正常列出、且逐条 `verified: true`。
一旦手抄错 parent，线程链断了却**没有任何机器信号**，只能靠人读线程发现。

## 1. 复现（可复制；跑完请删 refs）

> 用**临时线程 id** 复现，避免污染真实卡片；三条 ref 的名字命令会打印，清理命令见 §5。

```sh
ID=openmir2-parent-validation-selftest-20261025

# 1) 根条目（无 parent）——正常
walgit collab entry --repo . --kind issue --id $ID \
  --body '{"goal":"parent 校验复现","owner":"<你的 principal>"}'

# 2) 合成 **40 位十六进制** parent（对象不存在）——rc=0 ✅（这就是缺陷）
walgit collab entry --repo . --kind comment --id $ID \
  --parent d398d8678aa0deadbeefdeadbeefdeadbeef000000000000000000000000 \
  --body '{"note":"synthetic 40-hex parent"}'

# 3) **8 字符短前缀** parent（能唯一解析到根条目，但存的是字面短串）——rc=0 ✅
walgit collab entry --repo . --kind comment --id $ID \
  --parent ca38598c --body '{"note":"short prefix parent"}'
```

实测输出（2026-10-25，reviewer-21 亲跑）：

```text
# 2) 与 3) 均：
refs/collab/inbox/openmir2-reviewer-21/<hash> <entry-oid>
note: … written to this checkout only — no remote sees it until `--push <remote>` …
# 退出码 = 0
```

## 2. 观测：**没有视图会报错**（与卡面原述不同，以实测为准）

对上面三条组成的临时线程：

```sh
walgit collab thread $ID
#  → 3 entries；原始 JSON 里 "broken_refs" 出现 **0** 次；
#    每条都是 "verified": true，包括 parent = 不存在的 40-hex 与 parent = "ca38598c" 的两条。
#    条目的 parent 字段**逐字保留**你写的内容（不是补全后的 oid）。

walgit collab pr --repo . $ID
#  → {"merge": {"allowed": true, "reason": "base is not protected", …}, "pr": …}
#    "broken_refs" 出现 **0** 次。

walgit collab report --repo .
#  → 列表里该线程显示 "3 entries (3 verified)"，没有任何 orphan/broken 标注。
```

**口径纠正**：`broken_refs` 在本仓文档里指的是**线程级** `--related` / `--depends-on` 指向不存在的**线程 id**
（见 `AGENTS.md` §1「越界引用会在 thread 视图报 broken_refs」），**不是**「条目的 parent oid 不存在」。
所以「用合成 parent 签一条 ⇒ thread 会显 broken_refs」这个说法在本机二进制上**不成立**；
更糟的是：**孤儿 parent 完全隐形**（列得出来、也判 verified）。

## 3. 影响（为什么这是缺陷，不是美观问题）

1. **parent 链是唯一的血缘记录**：条目是不可变的签名对象，parent 就是「我在谁的结论上作答」的凭据。
   parent 写错 ⇒ 该回答与上游断链，任何**沿 parent 走**的消费者（人工复核、审计重放、按链归属的合并判定、
   未来 `gc` 折叠校验）都会拿到一条断头链；而写错的那一刻没有任何报错。
2. **「重放 refs 得到同一视图」静默降级**：按 `ts` 排序的视图看起来仍连续（孤儿条目还会插在中间），
   只有按 parent 走的人才会发现断点——不同读者可能得到**不同的因果视图**。
3. **成本落在事后**：截至 2026-10-25 已发生 **5 次**（见 §4；第 5 次是「手打/占位 40-hex」这一新形态），每次都靠当事人自己读到 `parent=` 的怪值、再重签一条
   `supersedes` 说明来补救；`thread` 会同时列出作废条目与更正条目，读的人要自己判哪条有效。

## 4. 真实事故（2026-10-25，全部已补救）

| # | 当事人 | 写法 | 后果 / 补救 |
|---:|---|---|---|
| 1 | reviewer-21 | `--parent a539b428`（8 字符短前缀） | 线程链断；本人发现后重签 `cca3a06c`（parent 用完整 oid），并在 body 注明 `f877bbcb` 作废 |
| 2 | reviewer-21 | `--parent 3784d0dd4c0f…`（40 位但**写错**，真值 `3784d0dd4cef…`） | 同上；重签 `eeffd607` 并注明 `2a0018ff` 作废 |
| 3 | reviewer-20 | `--parent d398d8678aa0…`（真值 `d398d8671b18…`） | 重签 `b2cbddda`（`supersedes=f06f06ee`） |
| 4 | （Leader 记录） | 同日第 4 次 | 以重签 + `supersedes` 说明补救 |
| 5 | reviewer-20 | `--parent 25e13960d8be0f5b9e0a4b3bd5a0f5c8e0a0a0a0`（**手打/占位 40-hex**：前缀与真 oid `25e13960d82f4518…` 完全一致，只有尾部是编的 `…a0a0a0a0`） | 同一张卡签 patch 时 rc=0 收下；发现后用 `supersedes` 重签 `8e0c31d4`（`openmir2-needs-human-final`），有效链 = issue `c938ba7e` → status `25e13960d` → patch `8e0c31d4` → status `ec9ef81f` → review → patch `7161bd8a` |

**四种形态**（都是「手抄」这一个根因，但**可发现性**不同）：

| 形态 | 样子 | 能否靠肉眼发现 |
|---:|---|---|
| 1 | 8 字符短前缀（`a539b428`） | **能**（一眼不像 oid） |
| 2 | 40 位但拼错（`3784d0dd4c0f…` vs 真值 `…cef…`） | 难（要逐字符比） |
| 3 | 40 位补零/拼错尾部但真前缀（`d398d8678aa0…`） | 难 |
| 4 | **手打/占位 40-hex**（`25e13960d8be0f…a0a0a0a0`）：长度字符集合法、**前缀与真 oid 完全一致** | **不能**——看起来就是真 oid，只能靠 `git cat-file -e <oid>` 或脚本取 |

⇒ 纪律从「别手抄」升级为「**一律脚本取，永不键入 oid**」：
`walgit collab thread <id> | python -c "import sys,json;print(json.load(sys.stdin)[-1]['oid'])"`；
签完立刻回读，最后一条的 `parent` 必须**逐字等于**你上一条的完整 oid（或为空）。

## 5. 建议（owner 决策用；本卡不实现）

**A. 工具侧（walgit 仓外，需 owner 决定是否修）**

1. **写入期校验（首选）**：`entry --parent` 若为空 ⇒ 根条目；非空 ⇒ 必须是 **40 位小写十六进制**，
   且**能解析为该线程已有的某个 entry oid**（或至少存在于 `refs/collab/inbox/*`）。否则 **fail-closed（rc=2）**，
   错误信息里给出取尾 oid 的现成命令：`walgit collab thread <id> | … 最后一条的 oid`。
2. **读取期暴露**：`thread` / `report` / `pr` 增加 `orphan_parent` 计数与条目列表
   （parent 非空且不等于任何 entry oid），至少让人一眼看到断链。
3. **短前缀干脆拒收**：`--parent` 只接受完整 oid（与 `git` 的 `--parent` 习惯区分开，避免"看起来能解析"）。

**B. 本仓侧缓解（即便工具不修也能落地）**

1. **队规（已下发）**：`parent` 一律用脚本取 `walgit collab thread <id>` 的**最后一条 oid**，不手抄；
   推荐片段：`walgit collab thread <id> | python -c "import sys,json;print(json.load(sys.stdin)[-1]['oid'])"`。
2. **交付前自检**：签完一条后立刻回读——最后一条的 `parent` 必须**逐字等于**你上一条的完整 oid
   （或空）；不等于就重签 + `supersedes`。
3. **永不键入 oid**（形态 4 的教训）：40-hex 一律从 `thread` 输出复制或脚本取，**不许手打/补位**——
   形态 4 的前缀与真值一致，肉眼与「合法 oid」无法区分。
3. 若 owner 决定不修，把 B1/B2 固化进 `AGENTS.md` §3（线程协议）的"先读后写"条，作为强制步骤。

## 6. 清理（复现者必做）

复现用的三条 ref 各由 `entry` 命令打印，逐条删除即可（本地 ref，不影响他人）：

```sh
git update-ref -d refs/collab/inbox/<principal>/<hash-1>
git update-ref -d refs/collab/inbox/<principal>/<hash-2>
git update-ref -d refs/collab/inbox/<principal>/<hash-3>
walgit collab thread $ID     # 期望：Error: no entries for thread …
```

本次复现的三条 ref 已按上法删除（2026-10-25），看板 `grep parent-validation-selftest` = 0。

## 7. 门禁 / 残留

- 本条只新增本文件；`git diff --check <base> <head>` rc=0；7 项卫生门 + `merge_tree_helper` 见 patch body。
- 残留：无（复现 ref 已删；未起服务端、未占端口、未连库）。
