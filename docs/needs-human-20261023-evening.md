# needs-human 清单（2026-10-23 晚）

> 触发：Leader 循环连续三次巡检**无合并、无派活**（并发 6→4 已满、待审 9 全在审或返工）。
> 下面每条都是**本循环的硬约束或权限边界内解不了**的；其余可判问题已于当日自行判完并记 `comment`。
> 快照：master `ff88bdf2` @ 2026-10-23 18:1x +0800。

## 1. 真机窗口需要栈权限（**最大的一条**）

- **欠账规模**：**173 张** R 卡欠真机窗口（口径已订正并由 reviewer 复算：114 里含 70 closed + 23 tier=L，真欠账 21，另 149 措辞不在 grep 里 + 3 快照后合入）。分批见 `docs/window-batch-plan-20261023.md`：第 1 批 **52 可闭环**、第 2 批 4、第 3 批 81、不可协议对拍 35。
- **本循环禁止起栈**（`硬约束：不起服务端/客户端/代理、不占端口`）⇒ 窗口只能由**有栈权限的执行者**跑。
- **开跑前三件已就绪/在办**：① 账本 `--post-comment` **已解禁**（compare 诊断值有逐值断言）；② 参数发现已接线（`move_walk` 解锁、`eat_item` **仍阻塞**于 `openmir2-discovery-bagitems-201`）；③ **角色预置位**工具在办（`openmir2-window-character-placement`）——**没有它，多数 NPC 点击窗口会以超时静默失败**。
- **需要人做**：给一次独占栈窗口，并先跑**两道真窗口门禁**（`openmir2-window-gate-two-runs`：`pickup_item` 验两次登录、`npc_click` 验 actor id 跨会话稳定）。

## 2. 35 张「不可协议对拍」需要裁决

批量计划第 4 类：GUI 渲染 / 300 假人 24h 长跑 / C# 时钟注入 / 多会话收件人集合 / 工具缺口。
其中**多会话**已有卡（`openmir2-multi-session-probe`，slice-1 离线已交付）；其余需要：

- **C# 时钟注入**：`HUtil32.GetTickCount`/`DateTime.Now` 不可替换字段，需 owner 在 Mono.Cecil IL patch 与「离线等价」之间裁决（`openmir2-m3-clock-injection`）。
- **300 假人 24h 长跑**：需独占窗口与时长承诺（`openmir2-m3-final-acceptance` DoD②）。
- **GUI 类**：协议对拍不可替代，需真机人工判读。

## 3. 外部车道的返工吞吐

- 9 张待审里多张是 `request_changes`，其中 **kick / mobfireburn / smake** 三张同根因（`GMEXECUTE` 不展开 `<$…>` 导致窗口判据错），已由 worker-65 接管批量返工；`shutup-release` 待返工。
- 原作者车道 `worker-24` 不在本会话、发不了消息 ⇒ Leader 只能另派工人接管。**若这条外部车道继续高速产出而不返工，审查积压会持续**。

## 4. whitelist 待 owner 复核（延续项）

- **T-161..T-213** 的登记行需 owner 逐条复核留痕（历史欠账，不影响当前门禁，但终点条件②要求「owner 复核过」）。
- 其中 T-213（`action_variable_text` 21 码登记）与 T-221（多会话）是今日新增。

## 5. 私有 C# 栈的长期保留

- `E:/tmp/conformance/csharp-root` 15xxx/17xxx 与 `mir2_test_*` 库：窗口依赖它作为 oracle。**是否长期保留、谁负责重建**需 owner 明确。

---

## 本循环已自行判完、**不需要人**的三条（记录以免重复上报）

1. **`tracked` 前提**：早前以「tracked 永远 ≥1 ⇒ 条件③ 卡死」立项，实测 tracked=1 时条件③ 本就 PASS；已订正判读并写进 `leader-goal` §1.1。
2. **窗口欠账数**：从 114 订正为 **173**（时点重建，reviewer 两轮独立复算）。
3. **`m3-drop-exp-align` 夹具**：「master 无 `run_drop_exp_pair.ps1`」为假——文件在 `drop-exp/` 子目录；`git ls-tree <commit>:path` 只列顶层，三方各踩一次，已把教训写进批量计划 §4。
