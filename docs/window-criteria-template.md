# 窗口判据固定模板（`openmir2-window-criteria-template`）

- 卡：`openmir2-window-criteria-template`（tier=L）
- 目的：让「补窗口项」不再各写各的散文。**本模板是约定，不是门禁**——校验器只告警，绝不改 `goal_gate` 的 PASS/FAIL。
- 背景：卡 `openmir2-window-criteria-quality-audit`（head `3f0122c3`）抽样 30 张 `window_required` 卡，**63.3% 的判据不是机器可判**。

## 1. 六个必填字段

| # | 字段 | 要求 | 没有会怎样 |
|---|---|---|---|
| ① | **前置** | 需要什么：夹具（`placement` / `inventory` / `characters_magic`）、会话数、权限、部署件、profile | 到了窗口才发现跑不起来（今天已发生 3 次：BLOCKED 场景、缺 642、背包夹具未合） |
| ② | **可复跑命令** | 一条能照抄的命令（`card_window.py` / `mir2_probe.py` / `ps1`），**或**明确写「场景待建 + 承接卡 id」 | 判据无法复现，窗口只能靠记忆 |
| ③ | **期望** | 帧 `ident` + 关键字段/字节，或 DB 后态的字段级期望（表/列/值） | 「跑通」被当成「相等」 |
| ④ | **判据** | `compare_runs` 的 verdict（`diffs=0` / PASS）或具体 rc（`rc=0`/`rc=2` 且点名哪一个） | 没有通过/失败的分界 |
| ⑤ | **掩码/不可比字段** | 哪些字段不可比、为什么（指向 `rules.json` 的哪条规则） | 被误判成差异，或把真差异掩掉 |
| ⑥ | **负责人与状态** | 谁跑、当前状态（未跑 / 已跑通过 / 已跑失败） | 无法判断这条窗口项是「欠账」还是「已完成」 |

## 2. 合格样例（照这个写）

```
窗口项（R）：<一句话目标>
① 前置：profile=alt；场景 tools/probe/scenarios/login_only.json；placement map=0103 x=19 y=19；
   无需 inventory/magic 夹具；单会话。
② 命令：python -I -B tools/conformance/card_window.py --card <卡> --scenario tools/probe/scenarios/login_only.json --profile alt --out E:/tmp/window/<卡>
③ 期望：两侧登录进世界帧序逐字节一致；`SM_SENDUSEITEMS(621)` 与 `SM_SENDMYMAGIC(211)` 出现在同一位置；
   `SM_LOGON(50)` 的 recog=191/param=7/tag=12。
④ 判据：`compare_runs` verdict=PASS 且 `diffs=0`；任一侧 rc≠0 或出现 `UNCLASSIFIED` ⇒ 失败。
⑤ 掩码：`SM_TURN(10)` 的 series 整字（Dir+Light）按 rules.json ident 10 掩掉——两侧 WallClock 不同。
⑥ 负责人：<principal>；状态：未跑（等窗口排期）。
```

## 3. 反面样例（真实卡，逐条指出缺什么）

> **分析单元 = 该 status entry 的 `status.work` 散文段。** 结构化字段 `owner` / `status`（如 `"owner":"openmir2-orchestrator","status":"in-progress"`）是卡的**生命周期账本**，不是 ⑥；⑥ 要的是**这条窗口项的执行人**与**该判据的运行状态**（未跑 / 已跑通过 / 已跑失败）。下文两例的缺项都按 `work` 段判定。

**卡 `openmir2-monster-attack-cadence-lead` 的 status `3b73f23f`**（原文）：
> 「节拍门经定量核查无差异（两侧 min 间隔 3.147/3.190s，均 >3s）；原线索降级为「起始 HP 未控」。余量=按评审口径的 HP 受控重采样（真机窗口），并入窗口项」

逐条对照：

| 字段 | 缺什么 |
|---|---|
| ① 前置 | 只说「真机窗口」，**没写**夹具/摆位/会话数/权限（评审 comment `41b9163e` 里的 6 条前置/口径与 3 条判据都不在 status 里） |
| ② 命令 | **没有**可执行命令（照抄 status 的人不知道跑什么） |
| ③ 期望 | 只有阈值片段「3.147/3.190s」，**没写**要断言哪条 `SM_STRUCK(31)`、哪个字段 |
| ④ 判据 | **没有** rc / verdict（「无差异」是作者的结论，不是可复跑的判据） |
| ⑤ 掩码 | **没有**（该场景并不用 mask，但需显式写「无」） |
| ⑥ 负责人与状态 | `work` 段只写「并入窗口项」，**没写**窗口执行人、也没写该判据「未跑 / 已跑通过 / 已跑失败」；卡片结构化 `owner`/`status` 属生命周期字段，**不算** ⑥ |

**卡 `openmir2-gate-gmclose-race` 的 status `c77d9772`**：整句「已合入 master（`c83a9ae24`，tier=R）。无待窗口项；按 R 级规则不 closed，观察点=长跑下控制帧 flush」
——`work` 段缺 ①②③④⑤⑥ 全部六项（结构化 `owner`/`status` 不算 ⑥）；因为它本来就**不是窗口项**，正确处置是 `closed`，见卡 `openmir2-r-window-registry-closeout` 的 comment `6ace05bf`。

## 4. 只告警的校验（`tools/goal/window_criteria_lint.py`）

```sh
python -I -B tools/goal/window_criteria_lint.py --selftest          # 自测（合格 0 warning / 不合格点名）
python -I -B tools/goal/window_criteria_lint.py --text <status.txt> # 对一段 status 文本出 warning
```

- 输入 = 被检卡的 `status.work` 散文段（`--text -` 从 stdin 读）；喂整条 entry 的 body JSON 也一样，因为 ⑥ 只认「负责人 / 执行人 / 未跑 / 已跑 / window_required」，**不认**裸 `owner`、`status` 这类结构化生命周期字段。
- 输出 `MISSING <n>: <字段名>`；**exit code 恒为 0**（只告警，不参与任何门禁）。
- 它**不**读线程、**不**改任何文件、**不**触碰 `goal_gate` 的判据。

## 5. 与其它卡的关系

- `openmir2-window-criteria-quality-audit`：量出「多少不可执行」；本卡给「以后怎么写」。
- `openmir2-goal-gate-unmerged-window-bucket`：改 `goal_gate.py` 的报告分类；本卡只新增 `window_criteria_lint.py`（**不同文件**，无合并冲突）。
