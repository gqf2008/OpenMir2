# tests/golden/ —— 会话产物（**历史件，勿当判据**）

本目录放的是**每一次抓包会话的原始产物**：`session-<时间戳>/` 下有 `proxy/`（原始 dump + 分帧）、
`frames.ndjson`、`shots/`（截图）、`stages.ndjson`（阶段时间线）、`capture.jsonl`（导出件副本）。
`LATEST` 指向最新会话，供 `tools/e2e/run_e2e.ps1` 的 `golden-verify` 使用（**只读这一行路径，勿加注释**）。

## ⚠️ 为什么"勿当判据"

`session-*/capture.jsonl` 是 **C4 改导出器口径之前** 的旧 schema 副本（缺 `frame_form`/`layout`/`body_segments`）。
用**当前** replay 跑它会出现**假红**：act 帧口径错报 + ② 字段对拍错报——而它并不是当前口径的产物。

**canonical 金标准**（当前口径、已登记 sha256、`golden_fresh_check.py` 覆盖）在
`mir2-rs/tests/golden/*.jsonl`，登记表与口径见 `mir2-rs/tests/golden/README.md`。

## 要跑金标准门禁时用什么

```powershell
python tools/capture/golden_fresh_check.py     # 集合对账 + 新鲜度重导比对 + 登记 sha256 一致性
```

需要"重抓一份会话"时走 `tools/capture/capture_baseline.ps1`（它会顺带更新 `LATEST`）；
要把它升级成 canonical 件，必须走 `golden_fresh_check.py --regen --update-registry`（重导 + 更新登记行）。
