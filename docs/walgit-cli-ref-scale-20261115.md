# `wg` 客户端按 ref 数量线性劣化（2026-11-15 实测）

## 1. 现象

| 观测 | 数值 |
| --- | --- |
| 本地 `refs/collab/*` | **15,672**（`inbox` 15,558 / `meta` 114） |
| `git for-each-ref refs/collab` 耗时 | **88 ms** |
| `wg collab board` | **rc=124，>120s 超时被杀** |
| `wg collab entry` | `RegistryReadFailed`（早期为 `GitFailed`） |
| 只含 3 条 ref 的 `git clone --shared` 克隆 | `wg collab entry` **立即成功**，条目 `verified=true` |

⇒ 差值三个数量级；15,670 / 120s ≈ **每 ref 约 7.7 ms**，与「每个 ref 起一次 git 子进程」的 Windows 开销吻合。
`git` 通路（`ls-remote`/`fetch`/`push`）同期全部正常；服务端 `/healthz` ok、15,718 条 ref 缺失对象 = 0。

## 2. 一个必须记住的试验污染

2026-11-15 22:24 本地清理把 `refs/collab` 从 15,720 降到 4,853；随后执行 `wg collab board` 作为验证，
**该命令的读取路径会先 `fetch refs/collab/*`**，把刚删的 10,867 条全部从远端拉回（22:3x 复查 = 15,672）。
⇒ **「4,853 条仍失败」不是有效观测**：测 `wg collab entry` 时 refs 已回到 15k。

**规则**：凡以「本地 ref 数量」为自变量的实验，**任何会触发 fetch 的命令都会毁灭变量**；
删除后的第一个动作必须是**立即测目标命令**（`entry`），而不是先跑 board/thread 这类读取命令。

## 3. 由此推出的处置方向

1. **不是损坏，重装大概率无效**——除非二分证明「ref 很少也失败」。
2. **清理方案天然失效**：只要读写命令都会全量 `fetch refs/collab/*`，删完下一条命令就恢复。
   ⇒ 真正的修法只能是**客户端批量读 / 增量 fetch**，或**约定本地只保留最近 N 天 inbox + meta**（需要 walgit 侧支持跳过全量 fetch）。
3. 客户端应加**阈值提示**：本地 collab ref 超过阈值（建议 5000）时明确报「ref 过多，请归档」，
   而不是表现成 `GitFailed`/超时。
4. 建议把「每 ref ≈7.7ms、15,670 条 >120s、git 同规模 88ms」报给 walgit 维护方。

## 4. 复现（只读、无密钥）

```bash
git for-each-ref --format="%(objectname) %(refname)" refs/collab/ | wc -l     # 15,672
time git for-each-ref refs/collab/ > /dev/null                                # ~88ms
timeout 120 wg collab board; echo rc=$?                                       # rc=124，若已恢复会返回表
```

二分请在 `git clone --shared --no-checkout` 克隆里做（只 fetch 指定数量 ref），**不要在主检出做**——
主检出的任一 `wg` 读取命令都会把 ref 全量拉回。
