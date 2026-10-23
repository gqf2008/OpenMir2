# 白名单 `T-*` 的**复核状态**审计（`openmir2-whitelist-review-status-audit`）

- 卡：`openmir2-whitelist-review-status-audit`（tier=L）；分支 `card/openmir2-whitelist-review-status-audit`
- **快照戳**：master `dcdce2f43`（2026-10-24）。所有计数可用 §6 的命令复算；本审计**未改** whitelist 与任何卡片状态。

## 1. 先订正头条数字

| 口径 | 命令 | 结果 |
|---|---|---:|
| 含「待独立 principal 复核」的**所有行** | `grep -c '待独立 principal 复核' mir2-rs/tests/parity/whitelist.md` | **245** |
| 其中**属于 T-* 登记表的行** | `grep '^\| T-' … \| grep -c '待独立 principal 复核'` | **224** |
| T-* 行总数 | `grep -c '^\| T-' mir2-rs/tests/parity/whitelist.md` | **247** |

⇒ 卡面写的「**247 行里 245 行**带待复核字样」**不成立**：245 是**全文行数**，其中 **21 行是 `B-*` / `D-*` 偏离/缺陷行**（不属于 R 卡登记）。
正确的说法是：**247 行 T-* 里 224 行（90.7%）带「待独立 principal 复核」。**

## 2. 三档分类（可复算）

分类脚本规则（详见 §6 的 `/e/tmp/wl_audit.py`）：遍历 `^| T-` 行；含「待独立 principal 复核」→ ②；
否则若含 8–40 位十六进制 oid → ①；否则 ③。① 再逐行人读复核语义。

| 档 | 定义 | 行数 | T 号 |
|---|---|---:|---|
| ① | 行内引用了 reviewer/review oid 且线程可核 | **3** | `T-6`、`T-7`、`T-213` |
| ①′ | 行内有 oid 但**没有** reviewer/复核字样，或 oid 不可核 | **2** | `T-28`、`T-208` |
| ② | 只有「待独立 principal 复核」字样 | **224** | （其余全部；含今晚四批） |
| ③ | 既无该字样、也无任何 oid | **18** | `T-1`、`T-5`、`T-71`、`T-104`…`T-117`、`T-172`（历史行，早于该约定） |

合计 3+2+224+18 = **247** ✓

## 3. 抽样核 ① / ①′ 共 5 行

| 行 | 卡 | 行内引用 | 线程实核（非作者 review） |
|---|---|---|---|
| `T-6` | `openmir2-c6-e2e-loadgate` | 「复核 `openmir2-reviewer-4` @`7fd20f70` 判 **request_changes**」 | 条目 `a9e5ec0d` = reviewer-4 `request_changes` **成立**；该卡随后还有 reviewer-3 **`approve` `d3583dc7`**，**行内没有引用这条 approve** |
| `T-7` | 同上 | 同上 | 同上（两行共用同一组引用） |
| `T-213` | `openmir2-script-action-var-text` | 只写了 `f6509d9a`，没有「复核」字样 | 条目 `f6509d9a` = **reviewer-5 `approve`** ⇒ ① 成立（但行文没把它标成复核） |
| `T-28` | —（历史行） | `505f8d16`/`376ea004`（commit）+ `127d67b1`（blob） | **没有 reviewer 字样**，是证据指针不是复核 ⇒ **不算复核留痕**（①′） |
| `T-208` | `openmir2-frame-drain-order-audit` | `bdddbeba`（blob） | 在该卡线程里**查不到**这个 oid（可能属其它卡或已 fold）⇒ **不可核**（①′） |

**结论**：247 行里能直接追到「独立 principal approve」的只有 **1 行（T-213）**；`T-6`/`T-7` 的**卡**有 approve 但**行**引用的是 request_changes；
`T-28` 是误分类（无 reviewer 痕迹）、`T-208` 的引用不可核。⇒ 「登记型 PASS」实际可信度应按「**有登记**」而非「**已复核**」计。

## 4. 按来源分组（今晚四批 vs 历史）

| 来源 | T 范围 | 行数 | ① / ①′ | ② | ③ |
|---|---|---:|---:|---:|---:|
| 今晚四批登记 | `T-227`…`T-262` | **36** | 0 | **36** | 0 |
| 历史行 | 其余 | 211 | 3 / 2 | 188 | 18 |

⇒ **今晚四批 36 行全部带「待复核」**（作者已按批复审的是**卡**，不是 whitelist 行的复核引用）。

## 5. 建议（供 `docs/needs-human-20261023-night.md` §4 可裁决引用）

- **选项 A（严格）**：owner 逐条批 242 行（②+③）。成本高，除非按「批」批：四批 36 行作为 4 个批次一次性批。
- **选项 B（推荐，增量）**：把「复核」定义为「**独立 principal 复核 + 每行引用 review oid**」；先给今晚四批 36 行补上 approve oid
  （它们刚被 reviewer 独立看过，只差引用），历史 188 行由 owner 按批抽样（每批 ≥3 行）裁定。
- **选项 C（口径）**：把条件② 的文字改成「登记 + `reviewed` 状态字段」两件事分开——
  登记型 PASS 只表示「有登记」，`reviewed: <oid>` 才表示「已复核」；未复核的登记不再隐含已复核。
- **我的建议**：**B + C**。这样条件② 的可信度是量化的（`reviewed` 计数），且不需要 owner 逐条批 242 行。

## 6. 复算命令

```sh
grep -c '待独立 principal 复核' mir2-rs/tests/parity/whitelist.md                    # 245（含 B-/D- 行）
grep '^| T-' mir2-rs/tests/parity/whitelist.md | grep -c '待独立 principal 复核'      # 224
grep -c '^| T-' mir2-rs/tests/parity/whitelist.md                                     # 247
python -I -B /e/tmp/wl_audit.py                                                       # 三档分类 + 清单（A/B/C）
```

## 7. 残留物

- `E:/tmp/wl_audit.py`（分类脚本，只读）、`E:/tmp/wl-audit.json`（分类结果）。
