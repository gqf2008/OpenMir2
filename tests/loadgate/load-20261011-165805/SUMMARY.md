# 压测报告（C6 载荷门禁）

- 口径：tick 服务时延 = BotSrv 收到 #+GD/<rtime>! 的时刻 − rtime（Environment.TickCount，全机同域）；详见 tools/botload/load_report.py
- 档位：[200, 500, 1000]；连接错峰 20ms/个；稳态时长 60s
- 判据（§6 M5）：登录成功率 = 档位阈值（tier≥1000 为 95%，其余 100%，见 report.json 的 login_threshold_pct）/ 稳态掉线 0 / tick P99 ≤ 100ms

| 轮次 | 档位 | 登录成功率 | 阈值 | 稳态掉线 | in_world | tick P50 | P90 | P99 | max | 稳态样本(上线) | CPU峰% | 重进程峰 | 结论 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| r1 | 200 | 100.0% | 100.0% | 0 | 22 | 0 | 0 | 16 | 16 | 916 (10.0s) | 84 | 0 | GREEN |
| r1 | 500 | 100.0% | 100.0% | 0 | 18 | 0 | 0 | 15 | 16 | 949 (15.0s) | 90 | 0 | GREEN |
| r1 | 1000 | 100.0% | 95.0% | 0 | 17 | 0 | 0 | 16 | 32 | 704 (25.0s) | 96 | 0 | GREEN |
| r2 | 200 | 100.0% | 100.0% | 0 | 22 | 0 | 0 | 0 | 16 | 905 (5.0s) | 93 | 3 | GREEN |
| r2 | 500 | 100.0% | 100.0% | 0 | 22 | 0 | 15 | 31 | 31 | 1093 (15.0s) | 100 | 15 | GREEN |
| r2 | 1000 | 100.0% | 95.0% | 0 | 19 | 0 | 16 | 31 | 156 | 811 (25.1s) | 100 | 20 | GREEN |
| r3 | 200 | 100.0% | 100.0% | 0 | 27 | 0 | 16 | 62 | 62 | 1119 (10.0s) | 100 | 18 | GREEN |
| r3 | 500 | 100.0% | 100.0% | 0 | 20 | 0 | 0 | 16 | 16 | 736 (25.1s) | 100 | 18 | GREEN |
| r3 | 1000 | 100.0% | 95.0% | 0 | 20 | 0 | 0 | 16 | 16 | 688 (25.1s) | 93 | 3 | GREEN |

## 同命令多连跑取最差（owner 裁决 11dbde39 ①）

| 档位 | 轮数 | 阈值 | 最差登录率 | 结论 |
| --- | --- | --- | --- | --- |
| 200 | 3 | 100% | 100% | GREEN |
| 500 | 3 | 100% | 100% | GREEN |
| 1000 | 3 | 95% | 100% | GREEN |

## tick P99 离散度与本机负载（owner 裁决 82f8e0c3 ⑤）

tick 口径 = 服务端 `rtime` → 客户端收帧耗时（含 BotSrv 收包/调度）；P99 只在稳态窗、
样本基数 = `in_world`（真进世界的假人数，见下）。**本机负载探针**（CPU=Win32_Processor.LoadPercentage 
均值；heavy=rustc/cargo/cargo-clippy/msbuild 进程数，叧 tickbench）只用于**归属**：
CPU 峰 ≥ 40% 或 heavy 峰 ≥ 4 时 P99 **不可归属**（C# 参考栈 vs 同机并发编译）。

| 轮次 | 档位 | in_world | probe_sent | tick_samples | P99 | max | CPU均/峰% | heavy峰 | 负载窗? |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| r1 | 200 | 22 | 1206 | 916 | 16 | 16 | 67.2/84 | 0 | True |
| r1 | 500 | 18 | 1098 | 949 | 15 | 16 | 74.6/90 | 0 | True |
| r1 | 1000 | 17 | 1093 | 704 | 16 | 32 | 79.0/96 | 0 | True |
| r2 | 200 | 22 | 999 | 905 | 0 | 16 | 71.5/93 | 3 | True |
| r2 | 500 | 22 | 1235 | 1093 | 31 | 31 | 77.2/100 | 15 | True |
| r2 | 1000 | 19 | 1194 | 811 | 31 | 156 | 85.7/100 | 20 | True |
| r3 | 200 | 27 | 1320 | 1119 | 62 | 62 | 91.5/100 | 18 | True |
| r3 | 500 | 20 | 971 | 736 | 16 | 16 | 83.6/100 | 18 | True |
| r3 | 1000 | 20 | 1298 | 688 | 16 | 16 | 74.3/93 | 3 | True |

> **M5 待办（写实）**：§6 的「tick P99 ≤ 100ms」**目前在任何栈上都未验证通过**——
> C# 参考栈 1000 档实测 125/201ms（在共享负载窗口内，归属待空载复测）；待 Rust 栈接管后（M5 灰度）复测。
> 另：`in_world` 实测只有 ~10%（tier-500/1000 稳态 15~26），**不得**用这三档宣称「M5 三档：1000 假人在线」；
> 「假人真正进世界」是 M5 收尾缺口（归宿：卡 `openmir2-botsrv-world-evidence`）。

## 曲线 1：tick 服务时延 vs 假人数（目标 P99 ≤ 100ms）

```html
<svg xmlns="http://www.w3.org/2000/svg" width="560" height="180" viewBox="0 0 560 180" font-family="sans-serif" font-size="11">
<rect width="560" height="180" fill="#fff"/>
<text x="46" y="14" font-size="12" font-weight="bold">tick latency vs bots</text>
<line x1="46" y1="152" x2="548" y2="152" stroke="#94a3b8"/>
<line x1="46" y1="22" x2="46" y2="152" stroke="#94a3b8"/>
<text x="4" y="30">156</text>
<text x="4" y="152">0</text>
<text x="488" y="174">bots</text>
<text x="4" y="14">ms</text>
<path d="M46.0,138.7 L46.0,152.0 L46.0,100.3 L234.2,139.5 L234.2,126.2 L234.2,138.7 L548.0,138.7 L548.0,126.2 L548.0,138.7" fill="none" stroke="#2563eb" stroke-width="1.8"/>
<circle cx="46.0" cy="138.7" r="2.4" fill="#2563eb"/>
<circle cx="46.0" cy="152.0" r="2.4" fill="#2563eb"/>
<circle cx="46.0" cy="100.3" r="2.4" fill="#2563eb"/>
<circle cx="234.2" cy="139.5" r="2.4" fill="#2563eb"/>
<circle cx="234.2" cy="126.2" r="2.4" fill="#2563eb"/>
<circle cx="234.2" cy="138.7" r="2.4" fill="#2563eb"/>
<circle cx="548.0" cy="138.7" r="2.4" fill="#2563eb"/>
<circle cx="548.0" cy="126.2" r="2.4" fill="#2563eb"/>
<circle cx="548.0" cy="138.7" r="2.4" fill="#2563eb"/>
<text x="52" y="34" fill="#2563eb">P99</text>
<path d="M46.0,152.0 L46.0,152.0 L46.0,152.0 L234.2,152.0 L234.2,152.0 L234.2,152.0 L548.0,152.0 L548.0,152.0 L548.0,152.0" fill="none" stroke="#dc2626" stroke-width="1.8"/>
<circle cx="46.0" cy="152.0" r="2.4" fill="#dc2626"/>
<circle cx="46.0" cy="152.0" r="2.4" fill="#dc2626"/>
<circle cx="46.0" cy="152.0" r="2.4" fill="#dc2626"/>
<circle cx="234.2" cy="152.0" r="2.4" fill="#dc2626"/>
<circle cx="234.2" cy="152.0" r="2.4" fill="#dc2626"/>
<circle cx="234.2" cy="152.0" r="2.4" fill="#dc2626"/>
<circle cx="548.0" cy="152.0" r="2.4" fill="#dc2626"/>
<circle cx="548.0" cy="152.0" r="2.4" fill="#dc2626"/>
<circle cx="548.0" cy="152.0" r="2.4" fill="#dc2626"/>
<text x="144" y="34" fill="#dc2626">P50</text>
<path d="M46.0,138.7 L46.0,138.7 L46.0,100.3 L234.2,138.7 L234.2,126.2 L234.2,138.7 L548.0,125.3 L548.0,22.0 L548.0,138.7" fill="none" stroke="#16a34a" stroke-width="1.8"/>
<circle cx="46.0" cy="138.7" r="2.4" fill="#16a34a"/>
<circle cx="46.0" cy="138.7" r="2.4" fill="#16a34a"/>
<circle cx="46.0" cy="100.3" r="2.4" fill="#16a34a"/>
<circle cx="234.2" cy="138.7" r="2.4" fill="#16a34a"/>
<circle cx="234.2" cy="126.2" r="2.4" fill="#16a34a"/>
<circle cx="234.2" cy="138.7" r="2.4" fill="#16a34a"/>
<circle cx="548.0" cy="125.3" r="2.4" fill="#16a34a"/>
<circle cx="548.0" cy="22.0" r="2.4" fill="#16a34a"/>
<circle cx="548.0" cy="138.7" r="2.4" fill="#16a34a"/>
<text x="236" y="34" fill="#16a34a">max</text>
</svg>
```

## 曲线 2：GameSvr 内存（峰值/均值 vs 假人数）

```html
<svg xmlns="http://www.w3.org/2000/svg" width="560" height="180" viewBox="0 0 560 180" font-family="sans-serif" font-size="11">
<rect width="560" height="180" fill="#fff"/>
<text x="46" y="14" font-size="12" font-weight="bold">GameSvr WS vs bots</text>
<line x1="46" y1="152" x2="548" y2="152" stroke="#94a3b8"/>
<line x1="46" y1="22" x2="46" y2="152" stroke="#94a3b8"/>
<text x="4" y="30">290</text>
<text x="4" y="152">0</text>
<text x="488" y="174">bots</text>
<text x="4" y="14">MB</text>
<path d="M46.0,24.9 L46.0,25.8" fill="none" stroke="#2563eb" stroke-width="1.8"/>
<circle cx="46.0" cy="24.9" r="2.4" fill="#2563eb"/>
<circle cx="46.0" cy="25.8" r="2.4" fill="#2563eb"/>
<text x="52" y="34" fill="#2563eb">200 bots</text>
<path d="M46.0,24.8 L46.0,26.1" fill="none" stroke="#dc2626" stroke-width="1.8"/>
<circle cx="46.0" cy="24.8" r="2.4" fill="#dc2626"/>
<circle cx="46.0" cy="26.1" r="2.4" fill="#dc2626"/>
<text x="144" y="34" fill="#dc2626">200 bots</text>
<path d="M46.0,24.7 L46.0,25.6" fill="none" stroke="#16a34a" stroke-width="1.8"/>
<circle cx="46.0" cy="24.7" r="2.4" fill="#16a34a"/>
<circle cx="46.0" cy="25.6" r="2.4" fill="#16a34a"/>
<text x="236" y="34" fill="#16a34a">200 bots</text>
<path d="M234.2,24.7 L234.2,25.4" fill="none" stroke="#9333ea" stroke-width="1.8"/>
<circle cx="234.2" cy="24.7" r="2.4" fill="#9333ea"/>
<circle cx="234.2" cy="25.4" r="2.4" fill="#9333ea"/>
<text x="328" y="34" fill="#9333ea">500 bots</text>
<path d="M234.2,23.7 L234.2,25.4" fill="none" stroke="#ea580c" stroke-width="1.8"/>
<circle cx="234.2" cy="23.7" r="2.4" fill="#ea580c"/>
<circle cx="234.2" cy="25.4" r="2.4" fill="#ea580c"/>
<text x="420" y="34" fill="#ea580c">500 bots</text>
<path d="M234.2,22.1 L234.2,24.9" fill="none" stroke="#2563eb" stroke-width="1.8"/>
<circle cx="234.2" cy="22.1" r="2.4" fill="#2563eb"/>
<circle cx="234.2" cy="24.9" r="2.4" fill="#2563eb"/>
<text x="512" y="34" fill="#2563eb">500 bots</text>
<path d="M548.0,23.6 L548.0,24.4" fill="none" stroke="#dc2626" stroke-width="1.8"/>
<circle cx="548.0" cy="23.6" r="2.4" fill="#dc2626"/>
<circle cx="548.0" cy="24.4" r="2.4" fill="#dc2626"/>
<text x="604" y="34" fill="#dc2626">1000 bots</text>
<path d="M548.0,22.0 L548.0,22.9" fill="none" stroke="#16a34a" stroke-width="1.8"/>
<circle cx="548.0" cy="22.0" r="2.4" fill="#16a34a"/>
<circle cx="548.0" cy="22.9" r="2.4" fill="#16a34a"/>
<text x="696" y="34" fill="#16a34a">1000 bots</text>
<path d="M548.0,22.1 L548.0,23.2" fill="none" stroke="#9333ea" stroke-width="1.8"/>
<circle cx="548.0" cy="22.1" r="2.4" fill="#9333ea"/>
<circle cx="548.0" cy="23.2" r="2.4" fill="#9333ea"/>
<text x="788" y="34" fill="#9333ea">1000 bots</text>
</svg>
```

## 各进程内存（末档）

| 进程 | 峰值 WS | 均值 WS | 私有峰值 | 末段斜率 | 窗口 |
| --- | --- | --- | --- | --- | --- |
| DBSrv | 89.6MB | 89.2MB | 48.9MB | -0.01MB/s | 59.5s |
| GameGate | 184.8MB | 180.2MB | 172.6MB | 0.161MB/s | 59.5s |
| GameSrv | 289.6MB | 287.3MB | 245.4MB | 0.088MB/s | 59.5s |
| LoginGate | 117.4MB | 116.0MB | 83.4MB | 0.025MB/s | 59.5s |
| LoginSrv | 73.5MB | 72.8MB | 34.6MB | -0.002MB/s | 59.5s |
| SelGate | 115.6MB | 115.2MB | 84.1MB | 0.008MB/s | 59.5s |
| mysqld | 154.8MB | 154.8MB | 469.4MB | 0.002MB/s | 59.5s |
