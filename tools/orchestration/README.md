# tools/orchestration/ —— owner 侧编排运行时与约定

> 与 `AGENTS.md`、host 规范 `SKILL.md` §0c/§0d 配套：**协作发生在 walgit 看板；本目录只放「怎么把 agent 拉起来」的运行时与约定**，随仓库版本化、任何 agent 可复用。

## 模型（谁在跑）
```
外部 supervisor（launchd/systemd/Windows 计划任务/ABB watch）
        │  每 N 分钟一个 tick
        ▼
   supervise.sh ── ① 写状态快照 lane-status.log  ② 死掉的 worker 车道重拉（cap）
        │                                      ③ 有待审/新事件则拉 reviewer
        ▼
   start-lane.sh <tag> ── 在车道的 worktree 里起一个【无头 agent】（`pi -p` / `codex exec`）
        │
        ▼
   无头 agent：读 brief → 干活 → 走 walgit：`--kind patch` + `status: needs-review`（不 merge、不签 review）
        │
        ▼
   reviewer agent（另一 principal）：自己重跑判据 → `--kind review` approve/request_changes
        │
        ▼
   owner/coordinator：approve 后合并、写 `merge_result` + `status: closed`
```

**要点：控制台（交互会话）不在这个循环里。** 它只是人的面（观察 `lane-status.log`/看板、派活、验收）。循环必须由**独立进程**拥有——起在交互会话的进程树里会被误杀，这是本项目反复踩过的坑。

## 文件
| 文件 | 作用 |
| --- | --- |
| `lanes.example.conf` | 车道表：`tag\|worktree\|brief\|role\|card`。复制成实际 `lanes.conf` 使用。 |
| `start-lane.sh <tag>` | 按车道起一个无头 agent，输出到 `agent-<tag>.log`。 |
| `supervise.sh` | 一次 tick：状态快照 + worker/reviewer 重拉。 |
| `status.sh` | 手动看：各车道 alive / HEAD / ahead / 卡片。 |
| `park.sh` | 若用 `walgit collab watch --exec`，把事件落盘的 hook（只 park、返回 0）。 |
| `briefs/` | 各车道任务简报（agent 读它干活）。 |

运行目录（不入库）：`E:\tmp\openmir2-watch\`（日志、快照、pid、queue 等临时物）。

## 关键约定（踩坑沉淀）
1. **一个 worktree 一个身份**；agent 一卡一 worktree，交付只到 `needs-review`，**不自行 merge、不自签 review**。
2. **存活判定**：无头 `pi -p` 是**一次性**的，跑完即退。判存活按「进程命令行含 `Tag: <lane>`」；`alive=no` 但有新 commit（`AHEAD>0`）= 本轮已交活、不是没干。别用 pid 文件（npm shim 的 `$!` 是短命壳进程）。
3. **启动必须独立进程**：从交互 shell 直接 `… &` 起 agent，会让宿主等待其子进程而挂住；用外部 supervisor / `Start-Process` 起。
4. **路径用正斜杠**：`ABB watch` 等 busybox 工具会吃掉反斜杠（`C:\PROGRA~1\…` → `C:PROGRA~1…`），导致 tick 全失败。
5. **守卫扫代码不扫注释**：任何「禁止某 API/某写法」的文本扫描都要先剥注释与字符串，否则文档里的字样会假红。
6. **循环与状态可观测**：每 tick 先写 `lane-status.log`（alive 数 + 各分支 HEAD/AHEAD），这是「有几个 worker 在工作」的权威答案。

## 触发
Windows 上按 §0c：用 `watch --once`（ABB busybox `watch -t -n 600 …`）或计划任务调 `supervise.sh`，**`MultipleInstances=IgnoreNew`** 防叠加。
