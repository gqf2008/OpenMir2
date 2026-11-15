# BotSrv `login_conns.ndjson` 落盘缓冲复用（卡 `openmir2-botsrv-loginconns-buffer`）

- 日期：2026-11-16
- 卡：`openmir2-botsrv-loginconns-buffer`（tier=L，window_required=false）
- 分支：`card/loginconns-buffer`（worktree `worker-90`，基于 master `34c20e7d3`）
- 写集：`src/BotSrv/LoadMetrics.cs`、`src/BotSrv/LoadMetricsSelfCheck.cs`、`src/BotSrv/Program.cs`、`docs/**`
- 提交：实现 `870a23278`；P1 修复 `c1108d759`；测试加固 `6c3db993e`

## 1. 问题与改动

旧实现（`LoadMetrics.WriteLoginConnsLocked`）每次 flush（5s）都：

1. `new StringBuilder(LoginConns.Count * 160 + 64)` —— 500 档下 char 缓冲 80,064 chars = **160,128B**；
2. `sb.ToString()` —— 与文本同长的字符串（500 档下 500 行 ≈ **200KB 量级**）；
3. `File.WriteAllText(...)` —— 每次新建 StreamWriter/FileStream。

即每 5s 一块确定性的 LOH 垃圾；2026-11-15 的 LOH 归因把两轮 500 档 360s 的稳态 LOH 增量量子
（≈394KB / 5s，44/46 次间隔 5s）指到这条路径（归因报告在 `card/botsrv-loh-growth` 分支的
`docs/botsrv-loh-attribution-20261115.md`，随该卡合入；本卡不重复归因）。

改动（三选一中的「复用 buffer」）：

- `LoginConnsText`：进程级复用的 `StringBuilder`，每次 `Clear()` 后重填，稳态不再分配大块 char 缓冲；
- `_loginConnBytes`：进程级复用的 UTF-8 字节缓冲，按 `Encoding.GetMaxByteCount(chars)` 上界增长一次；
- 编码走 stateful `Encoder.GetBytes(ReadOnlySpan<char>, ...)` 逐 chunk 写入复用缓冲（跨 chunk 的
  代理对由 encoder 状态续接），不再 `ToString()`、不再 `Encoding.GetBytes(string)`；
  **不先测量字节数**（P1 复审修复，见 §2.1）；
- 落盘仍是**每次全量重写**：`new FileStream(path, FileMode.Create, FileAccess.Write, FileShare.Read)`
  + 一次 `Write(buffer, 0, n)`。

**没有改**：Flush 周期（5s）、字段集合、字段顺序、行序、字段值格式（`long.ToString()` 原文）、
`}\n` 换行、UTF-8 无 BOM、文件路径与打开模式、失败计数路径（`RecordMetricFlushFailure`）。
不是增量写、不降频、不裁剪内容。

## 2. 等价性证据（机器断言，不是人工比对）

新增无栈自检 `BotSrv.exe --selftest-loadmetrics`。其中 `WriteLegacy` 是**改动前
`WriteLoginConnsLocked` 的逐字复刻**（唯一差异是输出路径参数化；`git show HEAD:src/BotSrv/LoadMetrics.cs`
的 append 链与自检文件内的链逐字符相等，已用脚本比对确认）。

命令：

```
./src/BotSrv/bin/Release/BotSrv.exe --selftest-loadmetrics
```

输出（rc=0，15/15 PASS，ALL PASS failures=0）：

```
LOADMETRICS PASS empty-file-zero-bytes bytes=0
LOADMETRICS PASS empty-bytes-identical sha_new=e3b0c442...855 sha_legacy=e3b0c442...855
LOADMETRICS PASS canonical500-byte-identical bytes=104776 sha_new=69c46c69...ce11 sha_legacy=69c46c69...ce11
LOADMETRICS PASS canonical500-golden-sha256 sha=69c46c69...ce11 expected=69c46c69...ce11
LOADMETRICS PASS repeat-write-idempotent len1=104776 len2=104776 sha2=69c46c69...ce11
LOADMETRICS PASS shrink-500-to-7-truncates len_after=1450 len_before=104776
LOADMETRICS PASS unicode-keys-byte-identical bytes=1236 sha_new=a18662e6...e73d sha_legacy=a18662e6...e73d
LOADMETRICS PASS cross-chunk-fixture-splits-pair chunks=1,1,40
LOADMETRICS PASS cross-chunk-buffer-shrunk buffer=1
LOADMETRICS PASS cross-chunk-surrogate-no-throw
LOADMETRICS PASS cross-chunk-buffer-grew buffer=129 bytes=44
LOADMETRICS PASS cross-chunk-surrogate-bytes-equal bytes=44 sha_new=579af0fa...c7fa sha_legacy=579af0fa...c7fa
LOADMETRICS PASS single-flush-alloc-reduced before=434632B after=240B reduction=99.94% n=500
LOADMETRICS PASS flush-wiring-file-exists ...\wire\login_conns.ndjson
LOADMETRICS PASS flush-wiring-byte-identical bytes=104776 sha_wire=69c46c69...ce11 sha_legacy=69c46c69...ce11
LOADMETRICS ALL PASS failures=0
```

（完整 64 位 SHA 见运行日志与 golden 常量 `LoadMetricsSelfCheck.CanonicalSha256`；
canonical500 的 SHA256 = `69c46c69e82a61f0a8fc3550cf59cf38b8b8e47dd9a4199eb592aa87748ece11`。）

断言覆盖：

| 用例 | 牙（删/改什么会红） |
| --- | --- |
| `empty-file-zero-bytes` | 复用缓冲漏 `Clear()` ⇒ 空表仍吐旧行 |
| `canonical500-byte-identical` | 字段/顺序/换行/编码任一漂移 ⇒ 与旧实现复刻不等 |
| `canonical500-golden-sha256` | 两边一起改也逃不掉：golden 由改动前旧实现产物钉死 |
| `repeat-write-idempotent` | 漏 `Clear()` ⇒ 第二次追加成双份 |
| `shrink-500-to-7-truncates` | `FileMode` 改 `OpenOrCreate`/漏截断 ⇒ 残留旧尾 |
| `unicode-keys-byte-identical` | 加转义 / 换编码 ⇒ 字节不同 |
| `cross-chunk-fixture-splits-pair` | fixture 元断言：`GetChunks()` = [1,1,40]，代理对确实被切口拆开 |
| `cross-chunk-buffer-shrunk` / `-grew` | 先把字节缓冲缩到 1B 再走生产定容；不回缩会掩盖缺陷（假绿）；129B 证明增长发生 |
| `cross-chunk-surrogate-no-throw` / `-bytes-equal` | 退回「逐 chunk GetByteCount 定容」⇒ ArgumentException（红证见 §2.1） |
| `single-flush-alloc-reduced` | 回退成每次新 StringBuilder/ToString ⇒ 分配量打回 ~400KB |
| `flush-wiring-*` | `Flush()` 没接到新实现 / 路径或编码变了 |

## 2.1 P1 复审修复：跨 chunk 代理对的定容缺陷

复审 P1：`MeasureLoginConnsUtf8Bytes` 用逐 chunk `Encoder.GetByteCount(span, flush:false)` 计数，
与 stateful `GetBytes` 对「代理对被 `GetChunks()` 切口拆开」的处理不一致；按测量值定容会在
flush 时抛 `ArgumentException`，被 `Flush` 的 catch 吞成 `RecordMetricFlushFailure`（该次不重写）。
独立 probe（临时 net8 console，非生产代码）实测：

```
chunks=1,1,40
chars=42 measure=43 actual=44 exact=44 max=129
sized_to_measure: THREW ArgumentException: The output byte buffer is too small ... (Parameter 'bytes')
```

修法：删掉预测量；`_loginConnBytes` 按 `Encoding.GetMaxByteCount(chars)` 上界增长一次，
写盘长度取 stateful `GetBytes` 的返回值。

回归红/绿（同一 head 上的变异重放；变异只在 worker-90 worktree 内，随后 `git checkout` 还原并核对
blob sha）：

- **绿**（`GetMaxByteCount`）：`cross-chunk-buffer-shrunk buffer=1`、
  `cross-chunk-surrogate-no-throw`、`cross-chunk-buffer-grew buffer=129 bytes=44`、
  `cross-chunk-surrogate-bytes-equal bytes=44`
  （sha_new = sha_legacy = `579af0facf8b73bd7fdf10cf20220ad10148ea78157a19de5a003ec24d7fc7fa`）。
- **红**（把定容改回「按 measure 精确分配」）：
  `FAIL cross-chunk-surrogate-no-throw ArgumentException: The output byte buffer is too small ...`、
  `FAIL cross-chunk-buffer-grew exception`、`FAIL cross-chunk-surrogate-bytes-equal exception`，
  `LOADMETRICS FAILED failures=3`，rc=1。
- 首版跨切口用例曾**假绿**：大夹具先把复用缓冲撑大，退回旧定容不会抛；现已加「先缩到 1B」的
  接缝与 `buffer_shrunk` / `buffer_grew` 元断言（memory：牙要回答退化形态）。

## 3. 分配量前后对比与测量方法

测量方法：在同一进程、同一线程上，对**同一个 500 条 + 1 个 null 槽**的夹具连续落盘，
用 `GC.GetAllocatedBytesForCurrentThread()` 取单次调用的托管分配增量（精确、与 GC 时机无关）：

- 改动前：调用旧实现复刻 `WriteLegacy`（预热 1 次后测 1 次）⇒ **434,632B / 次**；
- 改动后：调用 `SelfCheckWriteLoginConns()`（缓冲按峰值增长后预热 2 次，再测 1 次）⇒ **240B / 次**；
- 下降 **99.94%**，远超卡面的 ≥70% 目标；每次 flush 仍发生一次（频率未变）。

说明：

- 434,632B 与 2026-11-15 归因的 394KB/5s 量子同量级；差异来自夹具文本长度（本夹具 104,776B，
  归因归档的 500 行长度略长）。
- 稳态只剩 `FileStream`/`SafeFileHandle` 等小对象（240B）；`StringBuilder` 与 byte 缓冲已复用。
- P1 修复后字节缓冲按 `GetMaxByteCount` 上界常驻（500 档 ≈ 307KB）；这是首次增长的一次性分配，
  不影响上表的稳态单次分配（仍 240B）。
- 该测量是**有界合成序列**上的单次分配量（卡面允许：window_required=false）；不声称真机 500 档
  LOH 曲线已在窗口内复测——真机复核可并入下一次已排期窗口（见 §5）。

## 4. 门禁与回归

- `dotnet build src/BotSrv -c Release`：**rc=0，0 errors**（13 个既有 warning，与本次改动无关）。
- `BotSrv.exe --selftest-*` 全部 15 个入口：**全部 rc=0**（attackrole / decode-compat / dropseq /
  error-census / guildcastle / heropet / loadmetrics / pairtrade / probecoords / questseq /
  scene-null / sceneseq / selloff / shared-state / shopseq）。
- 构建与自检结束后已执行 `dotnet build-server shutdown`。
- P1 修复/测试加固后重跑（最终树）：`dotnet build src/BotSrv -c Release` rc=0、0 errors；
  `--selftest-loadmetrics` 15/15 PASS；全部 15 个 `--selftest-*` rc=0；变异红版 rc=1（见 §2.1）。
- 卡面「五门禁」是 Rust workspace 的 `cargo` 命令；本卡按派工纪律**未跑 cargo**（不改 Rust），
  对应验证为上面的 dotnet build + 全量 `--selftest-*`。

## 5. 未做 / 残留

- 未做真机 500 档窗口复测（卡面明示不需要；R 级真机证据可并入下次窗口）。
- 复用缓冲按运行期峰值常驻（500 档：文本 ≈ 100KB + 字节缓冲 ≈ 307KB 上界）；这是
  「用常驻换定时垃圾」的明确取舍，不是泄漏。
- 逃生路径备注（coordinator 认领补文档）：`wg collab join --principal <p>` 只写本地
  `refs/collab/meta/principals/<p>`，**不会 push host**；本轮必须单条 push 该 principal meta
  到 walgit 后，签名才不再报 `ActorUnregistered`。建议补进 `docs/walgit-escape-path-20261115.md`。
- 自检临时目录 `%TEMP%/botsrv-loadmetrics-selftest-<guid>/` 在用例结束时删除；运行日志不含临时物。
- 本卡不改输出格式/字段/顺序/周期，不改协议/帧/golden/DB，不动 `src/OpenMir2`。
