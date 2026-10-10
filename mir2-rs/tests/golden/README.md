# tests/golden —— 金标准抓包（C 线产出，A 线 M0 验收①②③的输入）

**状态：待 C 线交付。** 本目录一旦入库抓包文件，A 线的 replay 门禁即可对真实流量验收。

## 抓包要求（设计文档 §11.2）

真实客户端（`D:\MirClient-run\run-release.cmd`，冻结客户端逐字节不改）+ 现网 C# 服务端，
从进程启动到「登录 → 选服 → 选人 → 建角 → 进游戏 → 战斗 → Alt+X 小退 → 再进」全程：

- 客户端↔网关（7000/7100/7200）双向**线上原始字节**（含 `#`/`!` 定界符与 `*` 心跳）；
- 每个包记录 C# 侧同一时刻观测到的字段日志（网关 `ClientSession` 解出的
  ident/recog/param/tag/series 与 body）；
- 时间戳（毫秒，单调即可），用于与 `E:\MirServer\logs\*.out.log` 对时间线。

## 文件格式（replay 工具直接消费）

`*.jsonl`，每行一条，UTF-8：

```json
{"kind":"frame","dir":"c2s","seq":1,"ts_ms":12345,"raw_hex":"2331..21",
 "ident":2001,"recog":0,"param":0,"tag":0,"series":0,"body_len":19,
 "body_sha256":"<sha256 of 解码后明文body>"}
```

字段口径：

| 字段 | 含义 |
|---|---|
| `dir` | `c2s` = 客户端→网关（帧形 `#1...!`）；`s2c` = 网关→客户端（`#...!`） |
| `seq` | 单调递增序号（全局，不分方向） |
| `ts_ms` | 捕获时刻（毫秒） |
| `raw_hex` | **线上完整帧字节**（含首尾定界符）的小写 hex |
| `ident/recog/param/tag/series` | C# 侧同一时刻解出的头字段（12B 头） |
| `body_len` / `body_sha256` | 解码后明文体（GB2312 原始字节，不转码）的长度与 sha256 |
| `kind:"edcode"` | （可选）裸编解码向量：`plain_hex` / `encoded_hex` |

无法对应到头字段的包（如 1 字节心跳 `2a`）可记 `{"kind":"heartbeat","dir":"s2c","seq":N,"ts_ms":N}`，
replay 目前跳过非 frame 记录以外的校验（心跳不参与①②③计数）。

## 验收（A 线执行）

```powershell
cargo run -p replay -- tests/golden/<capture>.jsonl
# GREEN 需要：①字节差异=0 ②字段差异=0 ③八阶段全覆盖且未识别包号=0
```

## 防伪约定

抓包必须来自真客户端+真服务端；**禁止**用 replay 自己编码的产物回填当判据
（自造样本自证 = 恒绿假门禁）。每份抓包在下方登记：来源机器、客户端版本、时间、sha256。

| 文件 | 来源 | 时间 | sha256 |
|---|---|---|---|
| `c-line-baseline-20261010-092858.jsonl` | 本机真实客户端 `D:\MirClient-run`（冻结未改）+ 现网 C# 服务端 `E:\MirServer`（只读未改，网关为会话目录影子副本）；见 [C线-交付说明.md](C线-交付说明.md) | 2026-10-10 09:28:58 | `5d0ab4e66825dd133d5d7c3686db77986b465a4ed0d7179d215ede05afbcc64f` |

该抓包当前覆盖 **5/8 阶段**（登录/选服/选角/建角/进世界到"公告确认"），缺 移动/攻击/小退；
缺口原因与证据（服务端侧阻塞，非协议/非抓包）见 [C线-交付说明.md](C线-交付说明.md) 的"已知阻塞"一节。
