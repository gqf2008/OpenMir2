# tools/whitelist — `T-*` 编号预约与校验

卡：`openmir2-t-number-reservation`（tier=L）。背景见 `mir2-rs/tests/parity/T-RESERVATIONS.md` 顶部。

`mir2-rs/tests/parity/whitelist.md` 的 `T-<n>` 是**全仓共用的线性编号空间**。2026-10-23 一天撞号 4 次，
根因是每条并行车道在**开工时**各自 grep「当前最大号」——并行时必然读到旧值；已有门
（`m3_whitelist_registry_audit.py`）只做事后检测。这里补的是**事前分配**。

## 唯一取号入口

```sh
python -I tools/whitelist/next_t.py --card <卡 id>     # stdout 一行 T-<n>，并原子追加认领行
python -I tools/whitelist/next_t.py --card <id> --dry-run
python -I tools/whitelist/next_t.py --bootstrap       # 一次性：whitelist 已有但预约表缺的号补「已落地」
python -I tools/whitelist/next_t.py selftest          # 15 项，含 4 进程并发竞速
```

号 = `max(本地预约表, 本地 whitelist, walgit/master 预约表, walgit/master whitelist) + 1`。
**号只增不减**；`已释放` 的号不回收。同一 `--card` 重复调用幂等（返回已有号，不烧号），
不同 card 的重复/并发调用必得不同号。

### 原子性（四层）

1. **跨进程互斥**：`msvcrt.locking`（Windows）/ `fcntl.flock`（POSIX），锁文件按预约表绝对路径 hash 落在
   系统临时目录，锁在「读表 → 算号 → 追加」全程持有；进程崩溃由 OS 释放，无死锁。
2. **单次追加 + fsync**：一行、一次 `write`、`newline="\n"` 不翻译，随后 `flush` + `os.fsync`。
3. **写后回读**：追加后立刻重读并要求该行可解析，否则非零退出。
4. **跨 worktree 新鲜度**：并集 `walgit/master` 的两份文件；取不到时降级为本地并在 stderr 明说
   （不是分布式锁——最终兜底是下面的门禁）。

## 门禁（合并前必跑）

```sh
python -I tools/whitelist/t_reservation_gate.py check     # rc=0 才许合；1=违规；2=缺文件
python -I tools/whitelist/t_reservation_gate.py selftest  # 10 项
python -I tools/whitelist/t_reservation_gate.py teeth     # 3 颗牙 + 逐字节还原 + sha256
```

| 规则 | 检查 |
|---|---|
| ① | `whitelist.md` 的每个 `T-*` 必须在预约表有「在用」或「已落地」行（只剩「已释放」也判红） |
| ② | 同一号不得有两个「在用」认领，也不得同号多行 |
| ③ | 预约表的「已落地」号必须真在 `whitelist.md` 出现 |
| ⓪ | 预约表里以 `\| T-` 开头但解析不成四列的行 = 坏行（含行号） |

`teeth` 在**真实**表的副本上做三种破坏，每种都必须变红且点名对应规则，随后**逐字节还原并核对 sha256**；
正本全程不落笔（结束时再核一次正本 sha256 未变）。

## 状态流转

`在用`（取号即写）→ 卡片合并进 `whitelist.md` 后改 `已落地`；放弃时改 `已释放`（**不要删行**）。

## 注

两个脚本各自解析预约表（`tools/` 下脚本按仓库惯例以 `python -I` 运行，`-I` 不把脚本目录放进
`sys.path`，共享模块反而不便），解析口径必须一致：逐行、跳过 ``` 围栏代码块、四列固定格式。
`next_t.py` 的 `--bootstrap` 与 `T-RESERVATIONS.md` 里的 186 条历史登记就是这么一次生成的。
