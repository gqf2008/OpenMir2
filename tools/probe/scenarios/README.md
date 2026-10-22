# 协议无头对拍：场景 DSL

本目录的 JSON 是 `tools/probe/mir2_probe.py`（无头客户端驱动器）的输入。
驱动器负责把同一串 `CM_*` 打进 Rust 栈与 C# oracle 栈，并把 s2c 帧逐帧落成证据；
**比较器是另一条车道**（比 `frames.ndjson` 与数据库后态），本目录不含判定逻辑。

```sh
python tools/probe/mir2_probe.py \
  --login-gate 127.0.0.1:7000 --account test01 --password test01 \
  --server-name 热血传奇 --char-name test01 \
  --scenario tools/probe/scenarios/gm_time.json \
  --out E:/tmp/probe-rust-gm-time
```

## 文件形状

```json
{
  "name": "唯一名字",
  "description": "给人看的一句话（可选）",
  "params": {"npc_actor": 119},
  "steps": [ ... ]
}
```

* 顶层只允许 `name` / `description` / `params` / `steps`；多写一个键即 **fail-closed**（退出码 2）。
* `steps` 非空；每一步是对象，**恰好一个动作键**，外加可选元键 `timeout_ms` / `delay_ms` / `note`。

## 步骤动作

| 动作 | 取值 | 线上展开 |
|---|---|---|
| `send` | 对象：必填 `ident`；可选 `recog`/`param`/`tag`/`series`；`text` 与 `raw_hex` **二选一** | 12B 头 + 体，按 `#1<EDCode>!` 发出 |
| `say` | 字符串 | `CM_SAY(3030)`，`recog=0`，文本按 GBK+EDCode |
| `click_npc` | int 或 `params` 名 | `CM_CLICKNPC(1010)`，`recog=<NPC actor id>` |
| `dlg_select` | 字符串 label，或 `{"label": "...", "recog": N}` | `CM_MERCHANTDLGSELECT(1011)`；`recog` 缺省取**最近一条 `SM_MERCHANTSAY(643)` 的 recog** |
| `wait_for` | int 或非空 int 数组 | 等**下一步开始之后**到达的第一条匹配 ident |
| `wait_frames` | `{"count": N, "idents": [...], "timeout_ms": M}` | 等 gamegate 连接**累计**帧数 ≥ N 且 idents 都出现过 |
| `sleep` | int 毫秒，或 `{"ms": N}` | 纯等待，不发包 |

`send.text` 的编码口径 = C# `EDCode.EncodeString`：先 `text.encode("gbk")`，再 EDCode。
`send.raw_hex` 是**编码前**的 body 字节（十六进制），驱动器负责 EDCode。
`dlg_select` 若在收到 643 之前使用且没给 `recog`，直接 fail-closed（退出码 2），不会瞎猜。

## 等待语义（对拍必须可复现）

* `wait_for` 只匹配 **`seq` 大于本步开始时** 的帧 —— 上一步已经收到的同号帧不会被误当成本步的回包。
* `wait_frames` 计的是 gamegate 连接的**累计**帧（含进世界前收到的 `658/51`），因此
  `login_only.json` 的 `count=15` 不会因为驱动先等 `SM_LOGON(50)` 而永远差几帧。
* 步骤间默认间隔 `--between-ms`（默认 200ms）；单步可用 `delay_ms` 覆盖。等待本身用 `timeout_ms`。

## 参数

`params` 里可放 int 或字符串；动作里的字符串会按「先查 params、再按十进制 int」解析：
`{"click_npc": "npc_actor"}`。命令行 `--var npc_actor=456` 覆盖场景默认值。
换一台实例跑 `dice_chain` 时，actor id 变了就只改 `--var`，文件不动。

## 驱动器的自动握手（对拍不要关）

照 `src/BotSrv/Player/RobotPlayer.Operate.cs`：

* 收到 `SM_SENDNOTICE(658)` -> 回 `CM_LOGINNOTICEOK(1018)`；
* 收到 `SM_LOGON(50)` -> 发 `CM_QUERYBAGITEMS(81, recog=1)`；
* 服务端单字节 `'*'` 心跳 -> 回 `'*'`。

`CM_LOGINNOTICEOK` 的 `recog` 在 BotSrv 里是客户端 tick（1284 行），驱动器固定 **0**：
服务端不读该字段，固定值才能让 Rust / C# 两次运行的字节可比。登记为 **D-probe-1**。
调试可用 `--no-auto-handshake` 关掉，但那样两侧会缺同样的应答，不适合对拍。

## 证据文件

### `frames.ndjson`（一行一条 s2c 帧）

固定字段：`seq`、`t_ms`（相对本次运行起点）、`phase`（`login`/`selgate`/`gamegate`）、
`conn`、`ident`、`recog`、`param`、`tag`、`series`、`body_len`、`body_hex`、`body_sha256`。
附加：`frame_class`、`tail`（`!` 或 `!$`）、`raw_len`、`body_text`（GBK 展示，仅供人看）。

**`body_hex` 是线上原样字节**，不是「解码后的明文」：GameSvr 有的消息把多段
`EDCode.EncodeMessage(...)` 用**原样 `'/'`** 拼起来（如 `SM_ADJUST_BONUS(811)`），
所以 body 里允许出现不在 `0x3C..0x7B` 的字节。`body_text` 只是尽力解码的预览，
比较器请比 `body_hex` / `body_sha256`。

`frame_class` 取值：

* `protocol`：正常 12B 头协议帧；
* `server_ack_time`：服务端明文动作 ack `#+GD/<rtime>!`
  （`src/Modules/SystemModule/MessageSettings.cs` 的 `sSTATUS_GOOD`）。它不是协议帧，
  `ident/recog/param/tag/series` 为 `null`，`body_hex` 是 `+GD/<rtime>` 明文。
  其中的 `rtime` 是服务端时间，**不能逐字节对拍**，比较器应按 `frame_class` 排除或只比前缀。
  任何**其它**解析不了的头仍然 fail-closed（退出码 2），不会被当成帧吞掉。

### `run.json`

`argv`（`--password` 已打码）、`env`、起止墙钟、`chain`（登录四步的 ident/网关地址/session/角色列表/
`SM_LOGON` 的 actor id 与落点）、`steps`（每条步骤的发送帧十六进制与等待结果）、`auto`（自动握手）、
`frames_total` / `frames_by_conn` / `ident_hist`、`unclassified`、`exit`。

## 退出码

* `0` 全绿；
* `1` 链路或场景失败（网关不可达、登录被拒、`wait_for` 超时、角色不存在等）；
* `2` UNCLASSIFIED：帧解析失败（越界字节/未闭合帧）、规则没覆盖（`531` 改密、
  `dlg_select` 无 643 锚点）、场景 DSL 非法、内部错误。

工具是 fail-closed 的：解析不了就退出 2 并写进 `unclassified`，**绝不静默当绿**。

## 自测

```sh
python tools/probe/mir2frame.py selftest
python tools/probe/mir2_probe.py --selftest
```

`--selftest` 用 `../fixtures/r9/` 的离线 s2c 抓包跑「切帧 -> 解析 -> 落 ndjson」全链路，
不连网、不起进程；含「改坏必红」的负向用例（载荷字节改成越界值必须报错）。

## fixtures 来源

`../fixtures/r9/` 是 r9 真机窗口的代理抓包（`E:/tmp/window/r9/proxy/`）原样复制：
`conn-0001.s2c.bin` 44 帧、`conn-0002.s2c.bin` 106 帧（含 4 条 `SM_MERCHANTSAY(643)`，
即赌场骰子链的服务端回包）。`*.c2s.bin` 是同一窗口的客户端字节，用于核对帧构造的已知答案向量。
`sha256` 见提交说明；不要在 fixture 上做任何改写（自测里的 tamper 是内存副本）。
