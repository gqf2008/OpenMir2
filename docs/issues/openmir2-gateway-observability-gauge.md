# openmir2-gateway-observability-gauge

> 状态：**实现完成（2026-11-19，分支 `card/gateway-observability-gauge`）**——只改日志/指标字段，
> 行为不变（决策路径未动：`claim_session_degrade` 的返回值只进日志）。
> 上游：`docs/issues/openmir2-gateway-degraded-sessions-cap.md`（定性：两个 4096 都是上限常量）。

## 改动（`mir2-rs/bins/game-svr/src/gate_backpressure.rs` + `lib.rs`）

### 1) 改名 + 当前值（汇总行 / NDJSON / 逐帧日志）

| 旧 | 新 | 语义 |
|---|---|---|
| `degraded_sessions` | `degraded_sessions_total_capped` | 去重集合**累计值**，上限 `MAX_CLOSED_SESSIONS`、从不清理（只增，到顶恒定） |
| `max_depth_frames` / `max_depth_bytes` | `max_depth_frames_hw` / `max_depth_bytes_hw` | 队列**历史高水位** |
| — | `degraded_sessions_current` | **当前**降级会话数（会话释放/重入时移除，可回落） |
| — | `depth_frames` / `depth_bytes` | **当前**队列深度（最近一次事件时采样） |
| — | `purged_frames` / `dequeued_oversize_frames` | 见下"账目闭合" |

### 2) `claim_session_degrade` 返回值修正

集合已满、**实际未插入**时返回 `false`（此前恒 `true` ⇒ 逐帧日志 `degraded_session=1` 虚报首次标记）。
返回值只用于日志，**行为不变**。

### 3) 当前降级 gauge 的回落点

新增 `note_session_released(session)`；`lib.rs` 新增 `release_session_residue(state, stats, session)`，
替换三处会话释放点（sim 清槽 `KickUser`、网关 `GM_CLOSE`、认证失败 `CertOutcome::Failed`）：
清队列残留（原有）+ `note_purged`（新）+ `note_session_released`（新）。

### 4) 队列账目闭合（4097–4144 的根因）

实测 `queued − flushed` 曾出现 4097–4144（略超 4096），**不是入队边界问题**：会话释放会用
`PendingQueue::retain` 清掉残留帧，这些帧计入 `queued` 但既不计 `flushed`、也从未进 `flush` ⇒
差值被当成"队列深度"读。加上 `purged_frames`（释放清理）与 `dequeued_oversize_frames`（补发时
按 oracle 单包超阈值规则弹出丢弃）后：

```
queued_frames = flushed_frames + purged_frames + dequeued_oversize_frames + 队列当前深度
```

## 测试与牙

- 新增/更新单测（`gate_backpressure::tests`，共 20 条）：`degrade_claim_returns_false_when_capped_and_split_from_total`、
  `session_release_clears_current_only`、`summary_reports_current_depth_and_capped_total`、
  `queue_accounting_closes_with_purge`；lib 接线牙 `release_session_residue_wires_purge_and_gauge`（清残留 + 计 purged + gauge 回落）。
- 全量：`cargo test -p game-svr --lib` → **1965 passed / 0 failed / 7 ignored**；`clippy -p game-svr --all-targets -D warnings`=0。
- 变异牙（恢复后源文件 sha 一致）：
  - `claim_session_degrade` 改回恒 true ⇒ `degrade_claim_returns_false_when_capped_...` 红
    （`panicked at …degrade_claim_returns_false_when_capped_and_split_from_total`）；
  - `note_session_released` 改 no-op ⇒ `session_release_clears_current_only` 红；
  - `release_session_residue` 去掉 `note_purged` ⇒ `release_session_residue_wires_purge_and_gauge` 红。

## 边界

- **行为不变**：不改丢弃/拒绝/降级/关闭任何分支；不改协议与帧字节；不改队列容量（"不扩容"结论见上游卡）。
- `mir2-rs/tests/parity/whitelist.md` 的 T-282 条目保留旧字段名（历史记录），本卡不改其文本。

## 合并前门禁

C# 五工程 build + TestProject1 全量；Rust workspace fmt/clippy/test 三元组；msgcodegen；anchor gate；
工具门 TOTAL 70（集合+总数，NEED 单列）。合并后在 M6 复跑。
