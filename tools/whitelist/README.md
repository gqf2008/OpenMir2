# tools/whitelist — `T-*` 编号预约与校验

卡：`openmir2-t-number-reservation`（tier=L）。背景见 `mir2-rs/tests/parity/T-RESERVATIONS.md` 顶部。

`mir2-rs/tests/parity/whitelist.md` 的 `T-<n>` 是**全仓共用的线性编号空间**。2026-10-23 一天撞号 4 次，
根因是每条并行车道在**开工时**各自 grep「当前最大号」——并行时必然读到旧值；已有门
（`m3_whitelist_registry_audit.py`）只做事后检测。这里补的是**事前分配**。

## 唯一取号入口

```sh
python -I tools/whitelist/next_t.py --card <卡 id>     # stdout 一行 T-<n>，并原子追加认领行
python -I tools/whitelist/next_t.py --card <id> --dry-run
python -I tools/whitelist/next_t.py --card <id> --force-new   # 换号（旧「在用」行自动改「已释放」）
python -I tools/whitelist/next_t.py --bootstrap       # 一次性：whitelist 已有但预约表缺的号补「已落地」
python -I tools/whitelist/next_t.py selftest          # 24 项（并发 / 跨 worktree / 远端新鲜度 / 围栏）
```

号 = `max(共享 journal, 本地预约表, 本地 whitelist, 远端当前 master 的两份文件) + 1`。
**号只增不减**；`已释放` 不回收。同一 `--card` 重复调用幂等（返回已有号，不烧号）；
不同 card 的重复/并发调用必得不同号。

### 原子性（五层）

1. **同机跨 worktree 共享 journal**：认领先写 `$(git rev-parse --git-common-dir)/walgit/t-reservations.journal`
   ——**所有 worktree 共用这个目录**，所以号是整机全局序号，不是「各自表里的局部最大值」。
   journal 不进仓库、不影响 diff；崩溃最多浪费一个号，不会重号。
2. **跨进程互斥锁**：`msvcrt.locking` / `fcntl.flock` 锁 journal（无 journal 时锁预约表），
   在「读表 → 算号 → 写 journal → 写表」全程持有；进程崩溃由 OS 释放。
3. **单次追加 + fsync + 回读**：journal 与认领行都是一次 `write`，随后 `flush` + `fsync`，再回读校验。
4. **远端新鲜度（fail-closed）**：锁内 `git ls-remote` 拿 `walgit/master` 的当前 oid，与本地
   remote-tracking ref 不一致就 `git fetch` 再读；读到的永远是远端**当前** master。
   `ls-remote`/`fetch` 失败 ⇒ **拒绝发号 rc≠0**；只有显式 `--allow-stale` 才放行（大声告警）。
5. **号 = max(...)+1**（见上）。解析口径见下。

## 门禁（合并前必跑，**在合并结果树上**）

```sh
python -I tools/whitelist/t_reservation_gate.py check     # rc=0 才许合；1=违规；2=缺文件/空表
python -I tools/whitelist/t_reservation_gate.py selftest  # 18 项（含两份解析器 cross-check）
python -I tools/whitelist/t_reservation_gate.py teeth     # 5 颗牙 + 逐字节还原 + sha256
```

| 规则 | 检查 |
|---|---|
| ① | `whitelist.md` 的每个 `T-*`（**行首登记行**）必须在预约表有「在用」或「已落地」行（只剩「已释放」也红） |
| ② | 同一号不得有两个「在用」认领，也不得同号多行 |
| ③ | 预约表的「已落地」号必须真在 `whitelist.md` 出现 |
| ④ | `whitelist.md` 内部同一号出现多行（T-4 那一类） |
| ⑤ | 同一卡 id 不得有多个「在用」认领 |
| ⓪ | 坏行（带行号）、任一文件 ``` 围栏不成对（否则围栏后的行被静默跳过） |

缺文件 / 空表 / 空 whitelist ⇒ rc=2。

**为什么必须在合并结果树上跑**：在自己的分支 worktree 里，两条车道各自只见到自己的号，
`check` 恒绿；撞号只有在**合并结果**里才暴露（复审实测：两条分支各占 T-196，分支树各 rc=0，
合并树 ② rc=1）。做法：`git merge --no-commit` 后在结果树上跑，或用临时 worktree 合完再验。
同时跑一遍既有的 `m3_whitelist_registry_audit.py audit`（新门 ④ 已覆盖同类，双保险）。

`teeth` 在真实文件的副本上做五种破坏（②①③④⓪ 各一），每种都必须变红且点名对应规则，
随后**逐字节还原并核对 sha256**；正本全程不落笔（结束时再核一次正本 sha256 未变）。

## 状态流转

`在用`（取号即写）→ 卡片合并进 `whitelist.md` 后改 `已落地`；放弃改 `已释放`（**不要删行**）；
换号用 `--force-new`（旧行自动改 `已释放`，不会留下同卡两个「在用」）。

## 注

两个脚本各自解析预约表（`tools/` 下脚本按仓库惯例以 `python -I` 运行，`-I` 不把脚本目录放进
`sys.path`，共享模块反而不便）。**解析口径必须一致**：逐行、只认行首 `| T-n |`、跳过成对的 ```
围栏、围栏不成对即 fail-closed。门禁 `selftest` 里的 N-5 cross-check 就是为此钉的（两份解析器在
同一份含围栏/坏行/多状态文本上的号集合、行号、状态必须逐项相等）。`--bootstrap` 生成的 186 条
历史登记也是同一口径。
