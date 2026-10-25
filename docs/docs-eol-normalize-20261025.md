# 两份混行尾文档整份转 LF — 2026-10-25

- 卡：`openmir2-docs-eol-normalize`（tier **L**，owner `openmir2-dev-2`，branch `card/docs-eol-normalize`）。
- 背景：reviewer-21 在 `ecf8ee51` 指出 `docs/HUtil32-parity-inventory-20261020.md`（255 CRLF + 3 裸 LF）与
  `docs/m3-remaining-inventory-20261022.md`（341 CRLF + 1 裸 LF）**混行尾**；此前改一行只能写 LF，
  否则 `git diff --check` 把新增行的 CR 当行尾空白。

## 1. 做了什么

**整份统一为 LF**（whitespace-only）：两个文件所有 `\r\n` → `\n`。

- `docs/HUtil32-parity-inventory-20261020.md`：255 处 CRLF → LF（17638 → 17383 字节）
- `docs/m3-remaining-inventory-20261022.md`：339 处 CRLF → LF（47413 → 47074 字节）

**内容等价自证**（`git diff --ignore-cr-at-eol -w` 对两文件为空 ⇒ 零内容差异；差异只在行尾）：

```sh
git diff --cached --ignore-cr-at-eol -w --stat -- docs/HUtil32-parity-inventory-20261020.md docs/m3-remaining-inventory-20261022.md
# （空输出）
```

**未改任何内容**：数字、路径、结论一字未动（卡面 ④）。

## 2. 是否加 `.gitattributes`：**不加**（理由）

候选方案与评估：

| 方案 | 结论 | 理由 |
|---|---|---|
| 全仓 `*.md text eol=lf` | **否** | 会让所有现存 CRLF 的 md 在下次 checkout/提交时被批量重写 ⇒ 数百行「假 diff」，且波及别人的写集 |
| 只给这两份加路径规则（`docs/HUtil32-… text eol=lf` 等） | **暂不加** | 语法可行，但把行尾策略变成**逐文件登记**，新文档仍会漂；真正的收益（防止再次混行尾）由「本次整份转 LF + 文档写明保持 LF」已覆盖 |
| 什么都不做 | **否** | 混行尾会继续制造「只能写 LF」的补丁，使 `git diff --check` 与真实内容纠缠 |

⇒ 本次只做**一次性整份转 LF**；若将来再出现混行尾，按同一张卡的方式一次性转，不引入全仓策略。

## 3. 门禁

`stale_pointer --ref master` / `dynamic_count_stamp_gate --ref master` / `conflict_marker` / `board_coverage` /
`scenario_undecidable` / `t_reservation` / `whitelist_anchor` / `registry` 全 rc=0；`git diff --check <base> <head>` rc=0；
`merge_tree_helper` rc=0。

## 4. 残留物

- 入库：两份文档（whitespace-only）、本文件。
- 未入库：无（转换用一次性脚本，未落盘）。
- 未触碰：`card_window.py` / `run_batch.sh` / `tools/probe/**` / `src/**`；未起栈、未跑 cargo。
