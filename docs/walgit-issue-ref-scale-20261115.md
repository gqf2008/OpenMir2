# 给 walgit 维护方的问题报告（草案）

**标题**：`wg` 客户端随本地 collab ref 数量线性劣化：15.6k 条 ref 时单次调用 >120s，且删除会被 fetch 复原

## 环境
- Windows 10 Pro 19045；`wg 0.1.0 (walgit D1 collaboration client, zig)`；服务端 `v0.15.0`（`/healthz` ok）。
- 仓库：本地 `refs/collab/*` **15,672** 条（`inbox` 15,558、`meta` 114）。

## 复现
```bash
git for-each-ref --format="%(objectname) %(refname)" refs/collab/ | wc -l   # 15672
time git for-each-ref refs/collab/ > /dev/null                              # ≈ 88 ms
timeout 120 wg collab board; echo rc=$?                                     # rc=124（超时被杀）
```

## 关键观测
1. `git` 侧同规模操作 **88 ms**；`wg` 侧 **>120 s**——差三个数量级，≈ **每 ref 7.7 ms**，与「每个 ref 起一次 git 子进程」的 Windows 开销吻合。
2. **只含 3 条 ref 的 `git clone --shared` 克隆**里，`wg collab entry` **立即成功**且条目 `verified=true`。
3. **删除无效**：本地删 10,867 条（15,720→4,853）与删 1 条（15,674→15,673）两次实验，只要接着跑任何 `wg` 读命令，ref 数**自动恢复**——读取路径会先全量 `fetch refs/collab/*`。
4. 失败形态不稳定：早期快速返回 `GitFailed` / `RegistryReadFailed`，后期表现为**超时**；同一环境两种形态交替出现。

## 建议
1. **批量/增量读取**：`board`/`thread` 等只需少量 ref 的操作不应全量 fetch；改为按需 fetch 或一次性批量读。
2. **时间窗镜像**：允许配置「本地只镜像最近 N 天 inbox + meta」。
3. **明确的规模提示**：本地 ref 超阈值（建议 5000）时返回可操作错误（如「ref 过多，请归档」），而不是 `GitFailed`/超时。
4. 错误分类：`GitFailed` / `RegistryReadFailed` 应携带底层原因（哪条命令、哪个 ref、退出码），便于定位。

## 我们的临时缓解（不改客户端）
- 需要签名时在**低 ref 的 `git clone --shared` 克隆**里操作（`--key` 指向规范密钥路径，**不复制私钥**）；
- 减少调用次数，集中批量签名。
