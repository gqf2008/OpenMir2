# 可注入时钟（M3 横切前置 `openmir2-m3-clock-injection`）· **Rust 半**

> 交付片：Rust 侧 `MIR2_CLOCK_MS` 可注入时钟（`bins/game-svr`）；C# 影子 hook 半不在本片
> （需 owner 在 (a)/(b)/(c) 间裁决，见卡上 `68bbe41c…` 的盘点 comment）。

## 1. 做了什么

| 位置 | 变更 |
| --- | --- |
| `mir2-rs/bins/game-svr/src/clock.rs`（新增） | `MIR2_CLOCK_MS` 解析（`i64` 毫秒，未设/空=不启用、非法=fail-closed）+ 注入源（`origin + 单调经过`）+ `probe_line` |
| `mir2-rs/bins/game-svr/src/main.rs` | `main()` 起监听前 `clock::env_clock()?` 校验并打 `CLOCK_PROBE origin=<n>`；`now_ms()` 转发 `clock::now_ms()`；`sim_loop` 世界建好后 `world.clock_ms = origin` 并打第二条 probe |
| `mir2-rs/bins/game-svr/tests/clock_inject.rs`（新增） | 3 条进程级用例（注入有 probe + `world_clock_ms`；未设无 probe；非法值 ≤10s 非零退出且点名变数） |

**没做**：`crates/world` 一行未改（注入只改 `clock_ms` **起点**，`+200ms/tick` 节奏照 oracle）；
`src/**`、`tests/parity/m3/rng-hook/**`（C# hook）零改动。

## 2. 注入契约

```
MIR2_CLOCK_MS=<十进制 i64 毫秒>      # 起点；允许前后空白/正负号
  未设 / 空串   ⇒ 不启用：now_ms()=墙钟，world.clock_ms 从 0 起（与改动前逐字节相同）
  合法值        ⇒ now_ms()=origin+单调经过；world.clock_ms 起点=同一 origin
  非法值        ⇒ ClockError ⇒ main 非零退出（fail-closed，不许静默降级）
```

自证行：`CLOCK_PROBE origin=<n>`（启动）+ `CLOCK_PROBE origin=<n> world_clock_ms=<n>`（世界建好后）。

## 3. 证据四件套

| 文件 | 内容 |
| --- | --- |
| `01-oracle-anchors.txt` | oracle `HUtil32.GetTickCount/GetTimestamp` 行号 + 时间门消费者抽样 + Rust 注入点 + 契约 |
| `02-redchecks.txt` | **改坏必红 ×3**（RC1 去掉 world 起点 / RC2 忽略环境变数 / RC3 非法值 fail-open），各自原始输出 |
| `03-unit-tests.txt` | 构造级：`clock::tests::*` 5/5（含 parse 语义、单调性） |
| `04-process-tests.txt` | 进程级：`clock_inject` 6/6（含 support teeth） |
| `05-gates.txt` | 四门禁（合并树 `ad0bba301`）：fmt 0 / clippy 0 / 101 suites 1185 passed 0 failed 22 ignored / msgcodegen 604 |
| `06-boundary.txt` | 冻结面 `src/ sql/ MirClient/ protocol/ storage` 空 + 净改树清单 |

## 4. 判据小结

- **构造级**：`parse_clock` 语义 4 条 + `Injected` 起点/单调性 1 条 + probe 形状 1 条 —— 全绿。
- **进程级**：设 `MIR2_CLOCK_MS=1700000000000` ⇒ 日志含 `CLOCK_PROBE origin=1700000000000`
  **且** `world_clock_ms=1700000000000`（证明两处接同一注入）；不设 ⇒ 无 `CLOCK_PROBE`；
  非法值 ⇒ rc=1、~7ms、点名 `MIR2_CLOCK_MS`。
- **改坏必红**：三处变异各自 FAILED（见 `02-redchecks.txt`），还原后复绿。

## 5. 未完成 / 交给 owner

- **C# 影子 hook 半**：oracle 的 `HUtil32.GetTickCount`/`DateTime.Now` 是运行时方法，现有
  hook 的反射手法拦不住 ⇒ 需 owner 在 (a) Mono.Cecil IL patch（新 NuGet 依赖）/ (b) 影子源码重建 /
  (c) 不改 oracle、夹具按墙钟编排 之间裁决。**本片只定义 Rust 侧可判契约**，C# 半落地后按同变数/语义对齐。
- **车道归属**：卡 `owner_hint=openmir2-worker-11(m2-svr)`；m3cs(worker-5) 曾提议 Rust 半归它 ——
  若 owner 裁决改由 m3cs 做 Rust 半，本片可作废重做。
- **真机时间门对拍**：仍要独占窗口，归 ⑧/⑦/⑤ 各自的对拍卡。
