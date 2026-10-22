# OpenMir2 Leader 长期目标与循环重启手册（2026-10-23）

> owner 裁决：Leader 循环是**长期任务，目标达成前不停**；队列枯竭时必须继续拆卡，不允许空转。

## 1. 终点定义（达成即停）

同时满足以下四条，才允许把循环标为 `DONE`：

1. **脚本派发**：部署语料用到的派发目标 **100% 已实现**（当前 106/106，保持）；
2. **R 卡证据**：所有 R 级卡要么有真机/协议对拍证据，要么有**显式登记 + 明确理由**（`window_required` 或 whitelist `T-*` 且 owner 复核过）；不允许「代码已合但无证据且无登记」；
3. **B 类清零**：盘点里的 B 类（未接线/未实现）卡全部关闭或有 owner 裁决的替代方案；
4. **门禁恒绿**：`cargo fmt/clippy/test --workspace/msgcodegen` + `whitelist_anchor_gate` + `m3_whitelist_registry_audit` 全绿，且 `walgit` 上 master 与本地一致。

进度来源：`walgit collab board --format json`（A/B/待审/在跑/待认领 五个数）+ `docs/rewrite-remaining-inventory-20261022.md`。

## 2. 循环职责（每次巡检至少推进一件）

优先级：**判死接手 > 合并 > 派活 > 拆卡 > NOOP 报告**。

- **判死接手**：agent transcript >20 分钟无写入 ⇒ 判死，在**原 worktree** 起 finisher，不重做；
- **合并**：patch → 不同 principal 复核 → `merge --no-ff` → 完整门禁 → push → `merge_result` + `status`；
- **派活**：在跑 < 4 且写集不重叠 ⇒ 派 1 张（worker-4/5/10 轮换，先签 status 认领）；
- **拆卡**：待认领 < 3 ⇒ 从盘点/差异/残余里拆出下一个可执行单元，建成签名 `issue` 卡（goal/write_set/oracle 行号/acceptance/forbidden/tier）。
- **清理**：只删分支（本地 + `git push walgit --delete`），**绝不删 worktree**（worktree = 身份 = Ed25519 私钥）；腾空间只清 `*-target`。

## 3. 会话重启后如何恢复循环

cron 是**会话内**的（7 天自动过期，会话结束即消失）。换会话/重启后按下面三步恢复：

1. 读本文件 + `docs/leader-loop-20261023.md` + `docs/leader-queue-20261023.md`（镜像）；
2. 重新创建 5 分钟循环（prompt 见 §2 的职责清单，可直接复用上一版 cron 文本）；
3. 先跑一次「判死接手」与「看板同步」，再从优先级最高的一件开始。

## 4. 已知的长期阻塞（要靠 owner 或窗口）

- **GM 132 条未实现**：现网 `Permission` 恒 0，命令体不可达（设计件 §2.4）；除非 owner 批准 `SETPERMISSION` 夹具。
- **GUI-only 证据**：渲染/点击几何/输入法类卡无法用协议对拍替代，需真机窗口。
- **C# 真值栈**：私有 root `E:/tmp/conformance/csharp-root`（15xxx/17xxx、`mir2_test_*`）可与 peer 的默认端口栈并存；对拍时与 Rust 串行、同端口。
- **whitelist 待复核**：T-161..T-173 需 owner 逐条复核留痕。
