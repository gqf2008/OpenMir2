# gw-capture — 网关影子抓包 + timer gate（C10#4，tier=L）

本目录补评审 C8 的**工具缺口**：LoginGate/SelGate 侧没有入库的影子抓包拓扑与计时判据。
全部离线可跑；自检只用本地假 socket，**不绑 7000/7100、不连 `E:/MirServer`、不起服务端**。

| 文件 | 作用 |
| --- | --- |
| `gate_shadow_proxy.py` | 任意端口 TCP 影子代理；capture 与 `tools/capture/mir2_proxy.py` 同 schema，额外逐帧 `frames.ndjson` |
| `gwcap_common.py` | 公共切帧（复用 `segment_frames.py`）、EDCode/ident 解码、chunks→帧时间、`ServerDataPacket` 解析 |
| `gate_timer_gate.py` | 60s 两阶段空闲超时 / 10s 重连 / 10s KeepAlive 判据；无观测帧判 `UNCLOSED` |
| `selftest_gate_shadow.py` | 代理自检 4/4（本地假 socket；改坏 wire/.bin 必红） |

## 1. 影子抓包

```powershell
# 客户端侧：影子 7001 -> 真 LoginGate 7000（客户端登录端口指向 7001）
python -I -B tools/gw-capture/gate_shadow_proxy.py --out E:/tmp/g7/login `
  --map 7001=127.0.0.1:7000 --gate login

# 客户端侧：影子 7101 -> 真 SelGate 7100
python -I -B tools/gw-capture/gate_shadow_proxy.py --out E:/tmp/g7/sel `
  --map 7101=127.0.0.1:7100 --gate sel

# 下游侧：SelGate 的 ServerPort0 指向 5510 -> 真 DBSrv 5100（看 10s 重连/心跳）
python -I -B tools/gw-capture/gate_shadow_proxy.py --out E:/tmp/g7/sel-db `
  --map 5510=127.0.0.1:5100 --gate sel-downstream
```

输出（与 `mir2_proxy` 同格式）：

```
session.json        端口映射/网关标签/方向过滤/启动信息
chunks.ndjson       open/data/close/upstream_fail 事件；data 带 off/n/sha256/t_wall_ms/t_rel_us
conn-NNNN.c2s.bin   客户端→网关（或网关→下游）原始字节真值
conn-NNNN.s2c.bin   网关→客户端（或下游→网关）原始字节真值
frames.ndjson       逐帧：seq/conn/port/dir/off/len/sha256/kind/wire/ident/t_wall_ms/t_rel_us/hex
ready.flag          就绪标记
```

- `--direction s2c|c2s`：只落盘一个方向（**另一方向仍照常透传**），用于单向下行/上行取证。
- 帧时间戳 = **完成该帧的 TCP 读块**时间；精度是读粒度，不是单帧发送时刻。
- `--verify DIR`：chunks 与 frames 都必须能无损重建 `.bin`（sha256 逐块/逐帧），并**实算**
  `wire`/`ident` 与 frames 记录比对；每个 chunks `data` 事件方向都必须有对应 `.bin` 与帧
  （删掉整条方向不能再假绿）。对账失败 rc=1；chunks/frames 半截行（JSON 解析失败）
  属输入不完整 ⇒ `PREREQ_MISSING` rc=2。
- `--gate auto` 按上游端口推断：7000=login、7100=sel、5100=sel-downstream；影子端口不是 7000 时用
  `--gate login|sel|sel-downstream` 显式标注。

## 2. timer gate

```powershell
python -I -B tools/gw-capture/gate_timer_gate.py --capture E:/tmp/g7/login --mode login-idle
python -I -B tools/gw-capture/gate_timer_gate.py --capture E:/tmp/g7/sel   --mode sel-idle
python -I -B tools/gw-capture/gate_timer_gate.py --capture E:/tmp/g7/sel-db --mode sel-reconnect
python -I -B tools/gw-capture/gate_timer_gate.py --capture E:/tmp/g7/sel-db --mode sel-keepalive --dir c2s
python -I -B tools/gw-capture/gate_timer_gate.py --capture E:/tmp/g7/login --mode auto
```

| mode | 判据（oracle 逐行） | 观测物 |
| --- | --- | --- |
| `login-idle` | 计时从会话构造起、客户端数据**不刷新**；严格 `elapsed > 60s`；扫描 >1s；发 16B 缺陷帧（`'#'+15 字符`，**且 ident=528**）后**同一次扫描** close（第二阶段线上不可达） | s2c `wire=login-timeout-16`（形状 + ident 528）+ close |
| `sel-idle` | 仅 `CM_QUERYCHR/NEWCHR/DELCHR/SELCHR`（100..103）刷新；严格 `>60s`；扫描 >20s；发 18B 缺陷帧后 **socket 保持打开**（只摘槽；第二阶段不可达） | s2c `wire=sel-timeout-18`（ident 528） |
| `sel-reconnect` | 断链后**下一个 10s tick** 立即 `Start()` ⇒ 判据是 **close/upstream_fail→next-open 的间隔 `0 <= gap <= 10s`**（默认再 +2s 容差），不是相邻 open 间隔；有 KeepAlive 时还校验下一 tick 相位 | open/close/upstream_fail 事件；可选 KeepAlive 相位轨 |
| `sel-keepalive` | 就绪态每 10s tick 一条 `ServerDataMessage{Type=KeepAlive(3)}` | 6B `ServerDataPacket` 流 |

退出码：`0 GREEN` / `1 RED` / `2 PREREQ_MISSING` / `3 UNCLOSED`。
**没有对应帧一律 `UNCLOSED`（rc=3），不是通过。** 阈值可用
`--timeout-ms/--tolerance-ms/--reconnect-max-gap-ms`（reconnect）或
`--min-gap-ms/--max-gap-ms`（keepalive）显式覆盖；覆盖值会写进 `--json-out` 报告，便于审查复算。
`chunks.ndjson` 半截行 ⇒ `PREREQ_MISSING` rc=2（不抛 traceback）。

`auto` 按 session.json 的端口映射只跑有观测物的判据（7000→login-idle，7100→sel-idle，
5100→reconnect 或 keepalive）；显式 `--port` 时优先用 `session.json.port_meta.gate`
选模式，判不唯一则提示显式 `--mode`（不会再让同一个端口同时命中多类选择器）。
`all` 跑全部四条，缺观测物的项如实 UNCLOSED。

## 3. 自检

```powershell
python -I -B tools/gw-capture/selftest_gate_shadow.py          # 4/4，rc=0
python -I -B tools/gw-capture/gate_timer_gate.py --selftest   # 32/32，rc=0
```

代理自检：透传逐字节保真、chunks 复原、frames 复原并认出 16B 超时帧；篡改
`wire`/`ident`/`.bin`、删掉整条方向的 `.bin`+frames（chunks 仍留 data 事件）、半截
`chunks.ndjson` 后 `--verify` 必红或 `PREREQ`（rc=2）；`--direction s2c` 不落 c2s 且不破坏上行。
timer gate 自检：每条判据都有「会绿 + 会红 + 无观测物 UNCLOSED」；并覆盖严格边界
（59.5s 红 / 60.5s 绿）、16B 形状必须 ident=528（520 的截断正常帧 ⇒ UNCLOSED）、
16B 缺陷帧与下一帧同块时仍能切出（不假 UNCLOSED）、SelGate 刷新语义、失败重连也计一次尝试、
close→next-open 0/5/10s 绿与 20s 红、KeepAlive 相位轨（605s 偏离 tick ⇒ 红）、
`auto --port` 按 `port_meta.gate` 选模式、半截 chunks ⇒ rc=2。

## 4. 边界与残留

- 不覆盖 GameGate(7200) 的 0–210ms 丢帧、10ms 拍、>200ms drain、GM 10s 节拍/槽位——评审 C8/F6 的另一半，
  仍需 GameGate 侧注入/计时工具。
- 真机窗口必须在**影子端口**上采（真实 7000/7100 被网关占用时不要直接绑）；本工具只证明「判据会咬人」，
  不替代窗口 capture 证据。
- `frames.ndjson` 的帧时间是 TCP 读块时间；60s/10s 判据的容差已按扫描/tick 留出，不能用它判毫秒级相位。
- LoginGate 的 16B 缺陷帧没有 `'!'` 收尾，本身不自我定界：切帧器会在偏移 16 处见到
  `'#'`/`'*'` 时把「16B 缺陷 + 下一帧」拆开，尾包恰好 16B 时按形状（+ident 528）识别；
  因此同块的 `login16()+normal18()` 不会被吞成一帧。
