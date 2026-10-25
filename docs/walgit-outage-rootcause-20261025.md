# walgit 主源故障根因（2026-10-25）

- **快照戳**：本文取证于 `master = 57d506fa9`（2026-10-25 11:35 +0800）。
- **对象**：`gqf2008/OpenMir2`（原主源仓库）。**状态：不可读、不可写，原地保留未删。**
- **现用主源**：`gqf2008/OpenMir2-recover`（迁移过程见 `docs/walgit-outage-fallback-20261025.md`）。
- 证据来源：`~/.walgit/server.log.1`（故障发生时）、`~/.walgit/server.log`（此后持续失败）、`~/.walgit/walgit.toml`。
  下文时间均为**日志原始 UTC**，本地时间 = +8。

## 1. 故障形态

所有命令（`git ls-remote` / `fetch` / `push` / `walgit repo info` / `wal ls` / `wal materialize`）都返回同一个
HTTP 500，错误体同一个 key：

```
internal error: object not found: repos/gqf2008/OpenMir2/log/0000000000003000.pb
```

不是 R2 整体不可用（同桶其它仓库正常、`/healthz` 正常），也不是网络/凭据问题——**只有这一个 WAL 段缺失**。

## 2. 时间线（`server.log.1`，逐条可 grep）

| UTC 时间 | 事件 | 关键字段 |
|---|---|---|
| 16:06:22 | `store.put` `…/log/0000000000003000.pb` | `outcome=ok`, `bytes=413` |
| 16:06:23 | `maintain.pass` | `outcome=ok` |
| 16:06:23 | `wal.prefetch_packs` → `store.put` `…/log/0000000000002fb0.pb` → `wal.publish` | 段号**小于** head |
| 16:06:26 | `closed bundle slots settled` | `target=walgit_server::maintain` |
| **16:06:27** | **`store.delete` `…/log/0000000000003000.pb`** | **`outcome=ok`** |
| 16:06:28 | `store.get` 同一 key | `outcome=not_found` |
| 16:06:28 | `publisher batch failed: object not found …3000.pb` | `target=walgit_wal::publish` |
| 16:06:28 | `receive-pack publish failed` / `receive-pack failed` | `Corrupt(...)` |

此后（`server.log`，2026-10-25T03:05Z 起持续至今）每个维护周期都复现同一 key 的三类失败：
`request failed: wal: object not found …`、`maintenance: planning failed …`、`upstream follow round failed …`。

## 3. 结论

**服务器自己写、服务器自己删，间隔 5 秒。**

1. 一次 push 在 16:06:22 正常写入 WAL 段 `…3000`（413 B）。
2. 5 秒后——恰在维护循环打印 `closed bundle slots settled`（bundle 槽位结算）之后 1 秒——
   服务器对该段执行了 `store.delete` 并成功。
3. 同一次 push 的 publish 阶段回读该段，得到 `not_found`，push 失败。
4. WAL 段号连续，缺 `…3000` 等于把头砍断 ⇒ 之后所有读写与管理命令都在同一 key 上 fail-closed。

一句话：**维护循环把仍在使用中的 head WAL 段当成「已被 bundle 覆盖」回收了**，不是对象存储丢数据。

## 4. 未证实的加重因素（登记为推断，不作结论）

- 本机 `walgit.toml` 明写：本 Windows 部署与 **macOS 部署共用同一个 R2 桶 `git-gqf2008`**；
  本机 `[server].roles = []` 表示 serve / maintain / events **全开**。
- 16:06:23 出现了一个**比 head 更小**的段号 `…2fb0` 被 put ⇒ 像是**第二个写者/维护者**在同一仓库命名空间上
  按自己的（过期）视图动手，与回收动作撞车。
- 日志只能证明「谁删的」，不能证明「它为什么认为可以删」。要定性得看 walgit 上游的 bundle/compaction 源码或 mac 侧
  同时间段日志，两者都不在本机。

## 5. 复发预防（给运维，未改配置）

- **同一桶不要让两台机器同时跑 `maintain`**：要么本机改成 `roles = ["serve"]`，要么换独立桶/前缀。
- 我们不改 `walgit.toml`：这是环境配置，改了会影响 tray 与 mac 侧约定；**由 owner 决定**。

## 6. 能否修回原仓库

- 唯一修法是恢复那个 **413 字节**的 `…3000.pb`。可行途径只有「R2 对象版本历史」，
  前提是该桶开了版本控制且历史仍在——本机无 R2 管理凭据的版本查询能力，**未验证**。
- 我们**没有** WAL 段（protobuf 追加日志）的合成能力，**不得**用空对象或伪造段去补——那会把静默损坏变成数据损坏。
- 因此在原仓库上**不做任何写操作**，保持原地只读；已按 `docs/walgit-outage-fallback-20261025.md` 迁移到
  `gqf2008/OpenMir2-recover` 并全量补推（master / main / collab refs / 分支 / tag）。
