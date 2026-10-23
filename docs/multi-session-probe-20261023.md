# 多会话探针：会话标注 + 收件人集合断言（T-221，slice-1 离线）

- 卡：`openmir2-multi-session-probe`（tier=L：纯工具）；分支 `card/openmir2-multi-session-probe`，base `d3393f182`
- 作者：`openmir2-worker-24`；worktree `OpenMir2-wt-cronclaim`
- 设计先行：卡上 comment `bec70baf`（按 acceptance 要求，先落设计再实现）

## 设计要点

1. **会话标注**复用 `frames.ndjson` 的 `conn` 列：多会话标签 `<session>.<hop>`（`s1.gamegate` …），
   旧单会话标签（`gamegate`/`selgate`/`login`）**不含 `.`** ⇒ `session_of` 归 `"main"`
   ⇒ 旧字段逐字节不变、**`compare_runs.py` 口径零改动**。
2. **收件人集合断言**：`recipients(rows, pred, sessions=None)` → 有序会话名集合；
   `assert_recipients` 同时报 `missing`/`extra`，**绝不放松成「有人收到就算过」**；
   `recipient_first_seqs` 供「三会话次序一致」；`online_sessions` 把 `offline`/`closed` 剔除。
3. **边界**：±N 由用例把期望集合直接喂进来（`within_radius`，`RADIUS_CELLS = 12`），
   探针不推算坐标/AOI。

## 用例（5）与牙（2）

`mir2_probe --selftest` 新增 `multi-session` check（帧行手搓，不连服务端）：
① 两会话两格内都收到；② 一格超界只近的（期望含远者报 `missing`）；③ 三会话同格且首帧序一致；
④ 一个会话 `offline` 不参与；⑤ 会话 `closed` 从判定集合剔除（反证：不剔除会被它后来的帧污染）。
另含单会话回归（旧标签 ⇒ `main`）。

牙（实做并还原，`mir2_probe.py` sha256 `304aaefd…` 一致）：
A `assert_recipients` 放宽 ⇒ 用例②红；B `RADIUS_CELLS` 12→11 ⇒ `within_radius(12)` 红。

## 门禁（本 head）

```
python -I tools/probe/mir2_probe.py --selftest        9 checks OK
python -I tools/conformance/card_window.py --selftest 131/131
python -I tools/conformance/selftest_mutations.py     30/30
cargo fmt/clippy/test --workspace、msgcodegen、anchor 507、registry、T 预约表：见卡面结果
```

## 残余 / 窗口项

- **slice-2（真机）未做**：`--sessions N` 起 N 条真实连接并复用同一套断言（两会话「一发给另一个」、
  ±12 边界第 k 格、断线不再收），随窗口跑；本 slice 只交付离线函数与用例。
- 未起栈、未占端口、未连库。

## 残留物

- `E:/tmp/w24-msp.sha`（牙基线）、`E:/tmp/w24-msp-design.json`（设计 comment 正文）。
