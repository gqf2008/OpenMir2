# 压测报告（C6 载荷门禁）

- 口径：tick 服务时延 = BotSrv 收到 #+GD/<rtime>! 的时刻 − rtime（Environment.TickCount，全机同域）；详见 tools/botload/load_report.py
- 档位：[20]；连接错峰 200ms/个；稳态时长 60s
- 判据（§6 M5）：登录成功率 100% / 稳态掉线 0 / tick P99 ≤ 100ms

| 档位 | 登录成功率 | 稳态掉线 | 连不上 | tick P50 | P90 | P99 | max | 稳态样本 | 上线耗时 | 结论 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 20 | 95.0% | 0 | 0 | None | None | None | 0 | 0 | - | RED |

## 曲线 1：tick 服务时延 vs 假人数（目标 P99 ≤ 100ms）

```html
<svg xmlns="http://www.w3.org/2000/svg" width="560" height="180" viewBox="0 0 560 180" font-family="sans-serif" font-size="11">
<rect width="560" height="180" fill="#fff"/>
<text x="46" y="14" font-size="12" font-weight="bold">tick latency vs bots</text>
<line x1="46" y1="152" x2="548" y2="152" stroke="#94a3b8"/>
<line x1="46" y1="22" x2="46" y2="152" stroke="#94a3b8"/>
<text x="4" y="30">1</text>
<text x="4" y="152">0</text>
<text x="488" y="174">bots</text>
<text x="4" y="14">ms</text>
<path d="M46.0,152.0" fill="none" stroke="#2563eb" stroke-width="1.8"/>
<circle cx="46.0" cy="152.0" r="2.4" fill="#2563eb"/>
<text x="52" y="34" fill="#2563eb">P99</text>
<path d="M46.0,152.0" fill="none" stroke="#dc2626" stroke-width="1.8"/>
<circle cx="46.0" cy="152.0" r="2.4" fill="#dc2626"/>
<text x="144" y="34" fill="#dc2626">P50</text>
<path d="M46.0,152.0" fill="none" stroke="#16a34a" stroke-width="1.8"/>
<circle cx="46.0" cy="152.0" r="2.4" fill="#16a34a"/>
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
<text x="4" y="30">236</text>
<text x="4" y="152">0</text>
<text x="488" y="174">bots</text>
<text x="4" y="14">MB</text>
<path d="M46.0,22.0 L46.0,31.5" fill="none" stroke="#2563eb" stroke-width="1.8"/>
<circle cx="46.0" cy="22.0" r="2.4" fill="#2563eb"/>
<circle cx="46.0" cy="31.5" r="2.4" fill="#2563eb"/>
<text x="52" y="34" fill="#2563eb">20 bots</text>
</svg>
```

## 各进程内存（末档）

| 进程 | 峰值 WS | 均值 WS | 私有峰值 | 末段斜率 | 窗口 |
| --- | --- | --- | --- | --- | --- |
| DBSrv | 69.3MB | 66.9MB | 32.4MB | 0.048MB/s | 59.4s |
| GameGate | 49.8MB | 49.5MB | 42.1MB | 0.012MB/s | 59.4s |
| GameSrv | 235.6MB | 218.4MB | 252.0MB | 0.459MB/s | 59.4s |
| LoginGate | 44.9MB | 44.9MB | 14.7MB | 0.0MB/s | 59.4s |
| LoginSrv | 60.3MB | 59.6MB | 24.5MB | 0.024MB/s | 59.4s |
| SelGate | 49.6MB | 49.3MB | 19.5MB | 0.011MB/s | 59.4s |
| mysqld | 56.4MB | 56.2MB | 468.8MB | 0.01MB/s | 59.4s |
