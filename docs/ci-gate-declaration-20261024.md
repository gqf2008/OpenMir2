# 声明式门禁 CI（`.walgit/ci.toml`，2026-10-24）

- 卡：`openmir2-ci-gate-declaration`（tier=L）；写集 `.walgit/ci.toml`、`docs/`。
- 目的：把「每个审查者自己记得跑门禁」变成**声明式、可回签**的 CI 任务；本文件只产**证据**。

## 1. 协议事实（全部由 `walgit ci validate` 实测确定，不靠猜）

顶层字段（多一个/少一个都会被 `validate` 点名）：

| 字段 | 类型 | 本项目取值 | 备注 |
|---|---|---|---|
| `version` | 整数 | `1` | 缺失 ⇒ `missing field version` |
| `claim_ttl` | **字符串**（时长） | `"15m"` | 写 `900` ⇒ `invalid type: integer, expected a string` |
| `timeout` | **字符串**（时长） | `"30m"` | 同上 |
| `max_attempts` | 整数 | `1` | — |

`[[task]]` 字段（`run`/`tasks` 等字段名会被点名拒绝）：

| 字段 | 类型 | 说明 |
|---|---|---|
| `name` | 字符串 | 任务 id（结果按 `(ref, commit, task)` 聚合） |
| `command` | **字符串**（整条 shell 命令） | 不接受数组（`invalid type: sequence, expected a string`） |
| `refs` | 字符串数组 | 该任务对哪些 ref 生效；本项目限定 `refs/heads/master` + 本卡分支 |
| `schedule` | 字符串 | **6 段 UTC cron**（`sec min hour dom mon dow`）或 `@hourly/@daily/@weekly`；写 `*/5 * * * *` ⇒ 被点名要求 6/7 段 |
| `timeout` | 字符串 | 单任务上限 |
| `max_attempts` / `env_allow` / `artifacts` | 可选 | 未用到（默认 `[]` / `[]`） |

CLI 契约（`walgit ci … --help`）：

- `validate --repo . [--file …]` → rc=0 + 逐任务摘要；任何字段错 rc≠0。
- `run --once --repo . --remote walgit --actor <principal> --key <PATH>`：**`--key` 是密钥文件路径**，不是 hex。
  实测传 64 位 hex ⇒ `Error: read key <hex> … 系统找不到指定的文件`。本项目传
  `<git-dir>/walgit/keys/<principal>.ed25519`（`<git-dir>` 用 `git rev-parse --absolute-git-dir` 取；
  不把种子内容写进命令行/历史）。
  **注意**：`walgit ci run --help` 至今把 `--key` 写成 `32 raw bytes as hex`，与实现不符（实现按**文件路径**读取）；
  以本文实测行为为准（上游 help 文案错误，另记给 walgit）。
- `run --once` 会**轮询远端所有 ref**，只执行**被测 commit 自带的** `.walgit/ci.toml`；没有该文件的 ref 打印
  `no .walgit/ci.toml` 后跳过。结果按 `(ref, commit, task)` 去重：**已发布生效结果（effective）的组合永不重跑**。
  本地状态文件 `<gitdir>/ci-run.json` 只是「该 tip 的任务都已到终态」的缓存（`processed: {ref: tip}`）；**删掉它
  只会让 runner 重新评估该 tip，不会重跑已结算的任务**——这正是删完状态再跑 `--once` 常常打印 `watching`、0 条任务的原因。
  **想真正重跑只有三条路**：① 换 tip（新 commit，run id 由 `(task, ref, commit)` 决定）；② 改任务定义
  （`command` 等写在 `ci.toml` 里，改了必然产生新 commit）；③ 等 runner 对新 tip 触发。
  另注意：带 `--task <name>` 的单任务跑在成功后也会把该 tip 记为 `processed`，之后同一 tick 不带 `--task` 会整轮跳过。
- 并发 claim 是**合法竞态**：多方同时 claim，只**一份结果生效**（`ci status` 的 `effective`）。
- 结果回签为签名条目：`ci status [--format json]` 可读 `conclusion/exit_code/duration_ms/log_sha256/oid`；
  `ci log <run> --repo . --remote walgit` 取完整日志，`ci artifacts` 下载产物（sha256 校验）。
  **`ci log` 的 `--remote` 默认是 `origin`**（GitHub 镜像，见 `--help` 的 `[default: origin]`）；本项目的运行结果
  在 `walgit` 远端 ⇒ 不加 `--remote walgit` 会 rc=1 并报 `Error: run <run-id> not found`。

## 2. 分组（协议没有 `group` 字段）

用两件事表达「快 / 慢」两组，**不改协议**：

1. 名字前缀：`fast-*` / `rust-*`；
2. schedule 错峰：`fast-*` `@hourly`；`rust-*` 依次 `03:00 / 03:01 / 03:02 / 03:03 UTC`。

单组单独跑：`walgit ci run --once --task fast-<name> …`。

| 任务 | 命令（原样可复制） | 实测耗时 |
|---|---|---:|
| `fast-t-reservation` | `python -I tools/whitelist/t_reservation_gate.py check` | 0.11s |
| `fast-conflict-marker` | `python -I tools/hygiene/conflict_marker_gate.py` | 27.6s（冷启动）/ ~1s 热 |
| `fast-stale-pointer` | `python -I -B tools/hygiene/stale_pointer_gate.py` | 2.6s |
| `fast-whitelist-anchor` | `python -I mir2-rs/tests/parity/m3/whitelist_anchor_gate.py check` | 1.7s |
| `fast-m3-registry` | `python -I mir2-rs/tests/parity/m3/m3_whitelist_registry_audit.py audit` | 0.14s |
| `fast-dynamic-count-stamp` | `python -I -B tools/hygiene/dynamic_count_stamp_gate.py` | 0.3s |
| `fast-board-coverage` | `python -I -B tools/hygiene/board_coverage_gate.py --board .walgit/board.toml` | 1.8s |
| `rust-fmt` | `cargo fmt --manifest-path mir2-rs/Cargo.toml --all -- --check` | 4.6s |
| `rust-clippy` | `cargo clippy --manifest-path mir2-rs/Cargo.toml --workspace --all-targets -- -D warnings` | 62.3s |
| `rust-test` | `cargo test --manifest-path mir2-rs/Cargo.toml --workspace` | 289.7s |
| `rust-msgcodegen` | `cargo run -q --manifest-path mir2-rs/Cargo.toml -p msgcodegen -- --check` | 5.3s |

**为什么这么分**：fast 全是秒级只读 python 检查，适合每次 ref 更新都跑；rust 组分钟级且吃内存，
本机曾因 clippy 与 test 并行被 OOM 杀过 ⇒ **四条** rust 任务**错峰声明**（3:00 / 3:01 / 3:02 / 3:03），不并行。

`fast-board-coverage` 需要在 runner 的临时 worktree 里读 `refs/collab/inbox/*` 并调用 `walgit collab board`：
runner 用 `git worktree add --detach` 挂在**自己的 checkout** 下，linked worktree 与主 checkout **共享 refs**
（含 `refs/collab/inbox/*`），因此该门在任务 worktree 内可读；`--board .walgit/board.toml` 相对仓库根解析。

## 3. 实跑证据

**① 九个任务一次全绿（首次声明，历史证据；返工后清单为 11 条，见 ④⑤）**
（`refs/heads/card/openmir2-ci-gate-declaration @ 7e932062`）：

```text
$ walgit ci status --repo .
ci-0878ba8026ce09c9 rust-msgcodegen   … @ 7e932062 done success attempt 1 by openmir2-worker-24
ci-12c79104776d1d1c fast-t-reservation … @ 7e932062 done success
ci-39af56b5985ac9ef rust-test         … @ 7e932062 done success
ci-401aa1737ccabe00 fast-conflict-marker … @ 7e932062 done success
ci-5f97964665cbc082 fast-stale-pointer   … @ 7e932062 done success
ci-636b1dcc3d4a9580 rust-fmt          … @ 7e932062 done success
ci-779bade7390b67aa rust-clippy       … @ 7e932062 done success
ci-bba570909e1b41ba fast-whitelist-anchor … @ 7e932062 done success
ci-ed1a64b216776c3b fast-m3-registry … @ 7e932062 done success
```

（result oid 例：`fast-t-reservation → f2d37706de5`、`rust-test → ccbaf9525db`；`--format json` 含耗时与
`log_sha256`。）

**② 牙 (a)（后被 ④ 取代）：把 `fast-conflict-marker` 换成 `python -I -c "raise SystemExit(1)"` 后**：

```text
$ walgit ci run --once … --task fast-conflict-marker
ci: run ci-7f0e72ae48389339 task fast-conflict-marker attempt 1: failure (exit 1, 58 ms) as de80f1a566c1e57c8143cfdec235a890f7b072d8
$ walgit ci status --repo . | grep conflict
ci-7f0e72ae48389339 fast-conflict-marker … @ 9a0245d0 done failure attempt 1 by openmir2-worker-24
```

**③ 牙 (b)：还原命令后**（新 tip `11e39e33`）：

```text
$ walgit ci run --once … --task fast-conflict-marker
ci: run ci-5b239659af61a102 task fast-conflict-marker attempt 1: success (exit 0, 28757 ms) as c350fb25462df90108bd537a9ae45d69f8a22339
```

⇒ 失败会被回签成 `failure`，还原后同任务回签 `success`；不是「打印变红但结论恒绿」。

**④ 返工轮 I-2：旧牙为何不算数 + 语义失败牙对照**（对照跑在 temp tip `1ddf307e5`）

旧牙（②）在结论上确实是 `failure/exit 1`，但 `ci log` 显示 Python 收到的是 `"raise`：
`cmd /C` 没有把命令串里的双引号透传给 Python ⇒ 实际失败原因是 `SyntaxError: unterminated string literal`，
**不是**「进程主动 `sys.exit(1)`」。也就是说它只证明了「命令写坏也会失败」，没证明「语义失败会被捕获并回签」。
返工改用**磁盘脚本**（避免任何引号透传问题）：

```text
$ python -I -B tools/hygiene/_tooth_exit1.py ; echo $?     # 本地：干净退出，无输出
1

$ walgit ci run --once --repo . --remote walgit --actor openmir2-worker-58 \
    --key "$GD/walgit/keys/openmir2-worker-58.ed25519" --task fast-tooth-exit1
ci: run ci-950e5602b26ebe80 task fast-tooth-exit1 attempt 1: failure (exit 1, 57 ms) as 610c641755b4…
$ walgit ci log ci-950e5602b26ebe80 --repo . --remote walgit
（空输出）
# ci_result: conclusion=failure  exit_code=1  duration_ms=57  log_summary=""
#            log_sha256=e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855（空串 sha256）

$ walgit ci run --once … --task fast-tooth-syntax         # 原牙命令，故意保留对照
ci: run ci-b19547bfbf4d69a2 task fast-tooth-syntax attempt 1: failure (exit 1, 57 ms) as …
$ walgit ci log ci-b19547bfbf4d69a2 --repo . --remote walgit
  File "<string>", line 1
    "raise
    ^
SyntaxError: unterminated string literal (detected at line 1)
# ci_result: conclusion=failure  exit_code=1  log_sha256=6ca7406a4a65…e69866
```

⇒ 两种失败在**结论字段上完全一样**（`failure` / `exit_code=1`）；区别只在日志：语义牙**零输出**
（`log_sha256` 为空串哈希），引号牙的日志是 `SyntaxError`。前者才证明「任务主动返回非零 ⇒ CI 捕获并回签失败」。
**约束**：后续任务命令一律避免内嵌双引号（Windows runner 走 `cmd /C`，引号不保证透传）；确需脚本时写文件再执行。
temp 任务与 `_tooth_exit1.py` 只存在于 temp tip `1ddf307e5`，紧随的 `aecf95c98` 已删除；交付树（本分支）不含该文件，`[[task]]` 为 11 条。

**⑤ 返工 head 全量 11/11**：交付 head（`card/ci-gate-declaration-rework` tip）上跑一次
`walgit ci run --once --repo . --remote walgit …` 后，`walgit ci status --repo .` 显示 11 条任务全部 `success`
（含新增的 `fast-dynamic-count-stamp`、`fast-board-coverage`，均由 `openmir2-worker-58` 签名回签）。
复核：`walgit ci status --repo . | grep -c ' done .*success'`。

## 4. 这个 CI 不许做什么

- **不接自动合并 / 自动关卡 / 自动移看板**：本卡只声明任务与证据；合并判据仍是 `AGENTS.md` 的人审流程。
  协议里若存在「CI 绿即自动合并」的开关，本项目**不打开**（本配置文件也不声明任何合并动作）。
- **CI 绿 ≠ R 卡可 closed**：真机窗口（双栈对拍、帧字节、落库、RNG 序）**不在 CI 覆盖范围**；
  R 卡仍按 tier=R 保持 `in-progress` 等窗口。
- **抓不到**：机器/环境差异（只在你跑 runner 的这台机上）、外部依赖（MySQL/端口）、以及人工判据。

## 5. 复现

```sh
walgit ci validate --repo .                                   # rc=0，11 tasks
GD=$(git rev-parse --absolute-git-dir)
walgit ci run --once --repo . --remote walgit --actor <principal> \
  --key "$GD/walgit/keys/<principal>.ed25519" [--task fast-<name>]
walgit ci status --repo . [--format json]                      # 每任务的 conclusion/exit_code/耗时/oid
walgit ci log <run-id> --repo . --remote walgit                # 完整日志（默认 remote=origin 取不到）
# 重跑同一 tip 不存在「删状态」这条捷径（见 §1 去重规则）：删状态只重新评估，已结算任务不会重跑；
# 要重跑请换 tip（新 commit）或改任务定义（同样产生新 commit）。
rm -f "$GD/ci-run.json"                                        # 仅清本地缓存，不触发重跑
```
