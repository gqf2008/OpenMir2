# 墙钟上界清点（2026-10-27）

卡：`openmir2-wallclock-upperbound-scan`（**清点卡**：不改任何判据/阈值、不判红）。

## 为什么清这个

自测里「断言某耗时 **<** 阈值」是**墙钟上界**：机器一忙、调度停顿一下，绿就变红（工具门记 NEED，
整门 rc=3，`goal_gate` 条件④ 报「需人工」）——开窗当天值守会去查一个不存在的缺陷。
`mir2_probe` 的 dwell 反例已经踩过一次（卡 `openmir2-mir2-probe-selftest-need-flake`）。

## 复跑命令（原样可复现）

```sh
python -I tools/hygiene/wallclock_scan.py --repo .
# 附录（卡面范围之外，仅参考）：把生产 crate 的单元测试也扫进来
python -I tools/hygiene/wallclock_scan.py --repo . --extra-root mir2-rs/crates
```

脚本 `tools/hygiene/wallclock_scan.py` **只读、不判红、不进工具门**（它刻意不含自测入口，
所以 `--inventory` 的 54/59 = 54 + 5 + 0 不变）。扫描面：`tools/**`（`.py`/`.ps1`）+
`mir2-rs/tests/**`（`.rs`），识别三种形态：

- 直接比较：`assert elapsed_ms < 300`、`if x["actual_ms"] < 300:`、`-lt 16`；
- **赋给变量再比**：先收集「时间点/耗时」变量（`t = time.monotonic()`、`$end = (Get-Date)…`、
  名字像耗时的 `*_ms` / `elapsed` / `waited` / `first_t` …），再判引用；
- 减法只认**两侧都是时间点**（`$end - $start` 里 `$start` 是字节下标的情形不算墙钟）。

分类只三类：**上界**（负载下更易失败）/ **下界**（负载下更易通过，安全）/ **等超时**。

## 本次输出（默认范围，7 条）

> 下为**分组摘要**：脚本每条还会多打一行 `      | <该行原文>`，这里省掉；条数与分组以实跑为准。

```
== 上界（4 条）==
tools/conformance/card_window.py              :8856  <   1200   and res5["elapsed_ms"] < 1200
tools/db-parity/newchr_delchr_selchr_probe.py :778   <=  1000   … st["delay_before_ms"] <= 1000:
tools/probe/mir2_probe.py                     :1167  <   450    if "track" in out and out["interval_ms"] < 450:
tools/probe/mir2_probe.py                     :4046  <   300    assert rec0["actual_ms"] < 300, rec0

== 下界（3 条）==
tools/probe/mir2_probe.py                     :4004  >=  300    assert rec["actual_ms"] >= 300, rec
tools/probe/mir2_probe.py                     :5078  >=  1      … waited_b >= 1
tools/probe/mir2_probe.py                     :6405  >=  30     assert waited >= 30, waited

== 等超时（0 条）==
```

## 逐条判定（工具给候选，语义靠人判）

| # | 位置 | 断言 | 工具分类 | **语义判定** | 它保护什么 | 建议 |
|---|---|---|---|---|---|---|
| 1 | `tools/probe/mir2_probe.py:4046` | `assert rec0["actual_ms"] < 300` | 上界 | **真墙钟上界**（唯一一条） | 「dwell=0 时不要等」——但用墙钟表达 | **已在 `card/probe-selftest-need-flake` 改为可观测同步点**（数 `pump` 调用 + 5s 兜底），本卡不动 |
| 2 | `tools/conformance/card_window.py:8856` | `res5["elapsed_ms"] < 1200` | 上界 | **不是墙钟**：`elapsed_ms` 来自 `make_clock()` 合成时钟 | 报错串必须用实际 elapsed、不能用 window 上限冒充 | 无需改（但仍建议在断言旁注明「合成时钟」） |
| 3 | `tools/db-parity/newchr_delchr_selchr_probe.py:778` | `st["delay_before_ms"] <= 1000` | 上界 | **不是墙钟**：是 plan 里的**配置值**（限频延迟必须 > 1000ms） | oracle 口径：NEWCHR/DELCHR 的限频延迟 | 无需改 |
| 4 | `tools/probe/mir2_probe.py:1167` | `out["interval_ms"] < 450` | 上界 | **不是墙钟**：场景参数校验（track 模式的连击间隔下限） | 相邻两次 `CM_HIT` 间隔必须 ≥ 部署 `HitIntervalTime=850` | 无需改 |
| 5 | `tools/probe/mir2_probe.py:4004` | `rec["actual_ms"] >= 300` | 下界 | 真墙钟**下界** | dwell 至少等满 300ms | 安全（负载只会让它更大） |
| 6 | `tools/probe/mir2_probe.py:5078` | `waited_b >= 1` | 下界 | **轮次计数器**下界 | 「等」这件事真的发生过（不是提前放行） | 安全 |
| 7 | `tools/probe/mir2_probe.py:6405` | `waited >= 30` | 下界 | 真墙钟**下界** | `wait_idents` 必须等满 timeout 才返回差集 | 安全 |

**结论**：默认范围里**真正的墙钟上界只有 1 条**（第 1 行），且已经在本轮另一张卡里改掉；
其余 3 条「上界」都是**非墙钟**（合成时钟 / 配置值 / 参数校验），保持原样。

## 附录：把生产 crate 的单元测试也扫进来（+1 条）

```
mir2-rs/crates/world/src/world.rs:5754  <  20000  && elapsed < 20000;
```

语义：`elapsed = now - e.dup_start_tick` 是**世界 tick 差**、不是墙钟（对应 C# `:280-281`
的 `(A && B || C && D) && E` 判据），负载不影响 tick 差 ⇒ **不是墙钟上界**，无需改。

## 扫不到（或故意不扫）的形态

1. **阈值不是字面量**：`row["t_wall_ms"] < last_t`（`tools/gw-capture/gate_shadow_proxy.py:252`）、
   `f["t_wall_ms"] <= frame["t_wall_ms"]`（`gate_timer_gate.py:203`）、
   `close_t - frame["t_wall_ms"] <= args.keep_open_ms`（`gate_timer_gate.py:227`）、
   `0 <= st_ts - h["ts_ms"] <= lead_ms`（`mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_sm_hit.py:162`）。
   这些是**帧序/单调性/窗口**判据，比的是数据里的时间戳（同一份输入 ⇒ 同结果），不是墙钟。
2. **字符串里的数字**（`"elapsed=%dms，上界 %dms"` 这类消息）——已先剥字符串再判，避免误报。
3. **等待本身**（`pump(1000)`、`wait_for(timeout_ms=…)`、`proc.wait(timeout=5)`）是**输入**，不是断言。
4. **行尾注释**（`// 60201: > 60000`）——已按注释切掉。
5. **宏/动态断言**（`assertLess(x, 100)`、`Assert-True ($e -lt 100)` 的第二参）——本仓未发现；
   若将来出现，扩规则即可（`wallclock_scan.py` 的 `CMP_RE`/`PS_CMP_RE`）。

## 维护约定

- 新增任何「耗时 < 阈值」型断言后，跑一次上面的命令自查：
  **判成「上界」的必须证明它不是墙钟**（合成时钟/配置值/参数校验），否则改成可观测同步点；
- 本清点**不是门**：不判红、不进 REGISTRY、不动认树锚。要固化成常规门需另开卡登记。
