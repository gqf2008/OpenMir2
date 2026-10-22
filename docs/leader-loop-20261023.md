# 协作模式切换：Leader 循环（owner 2026-10-23 裁决）

## 角色

| 角色 | 谁 | 职责 |
| --- | --- | --- |
| **Owner** | 人 | 派活、拍板（冻结点/端口/优先级） |
| **Leader** | 本会话（console agent） | **拆卡 → 派车道 → 独立复核 → 合并 → 收卡**；不亲自写业务代码 |
| **Worker 车道** | 子代理（一次性） | 一个 worktree / 一个 principal / 一张卡；只交付到 `needs-review` |
| **Reviewer** | 子代理（不同 principal） | 钉住 head 自跑门禁与牙，出 `review`（带 tier / tier_why） |

## 触发：cron `*/5 * * * *`（会话内，7 天自动过期）

每次巡检**只做一件事**，优先级：合并 > 派活 > NOOP。硬约束：

- 一次只处理**一张卡**；在跑车道 < 3 才派新活；
- 复核者 principal 必须 ≠ 作者（reviewer-3 / reviewer-6 轮换；worker-5 / worker-4 轮换）；
- **同一时刻只跑一个** workspace 门禁；合并前必须 fmt / clippy / test --workspace / msgcodegen 全绿；
- 不改 `src/**`（owner 批准 + whitelist 登记的唯一例外：T-172 SelGate 端口）；
- 不起服务端/客户端/代理、不占端口、不写生产库；R 级真机窗口项只登记不跑；
- R 级卡 merge 后保持 `in-progress`，L 级可 `closed`；连续 3 次 NOOP ⇒ 输出 needs-human 清单。

## 首个用该模式跑的批次

- `openmir2-login-529-status-text`（R）：529 服务器列表状态段 `Idle` vs `0`，由对拍台首跑发现；
- `openmir2-conformance-harness` 规则表补齐 + `run_dual.sh`（L）。

## 判据分级仍然生效

Leader 合并时按 `TIER-POLICY.md`：R 级必须有真机证据或显式 `window_required`；L 级需等价证明 + 抽样冒烟。
