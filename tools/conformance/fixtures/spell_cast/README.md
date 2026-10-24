# spell_cast 对拍 fixture（卡 openmir2-spell-cast-controlframe-filter，tier=L）

`compare_runs.py selftest` 的**真机冻结输入**：window-run-1（2026-10-24）spell_cast
双栈运行的 `frames.ndjson` 副本。原始抓帧在 `E:/tmp/window/run1/spell_cast/`
（易失），此处副本让 selftest 离线可复现。

| 侧 | 来源（CRLF 原样） | 源 sha256 | 库内副本（LF 行尾） |
| --- | --- | --- | --- |
| csharp（oracle） | `E:/tmp/window/run1/spell_cast/csharp/frames.ndjson` | `fc4f3d6ac6a7505f031ff545a5c787fb8d3432fff48179159726acb68eeca68d` | `97ddc3b790493fe88ae1c11eced4ed790d69e3d792b6a2a20c5564ae95457b82` |
| rust | `E:/tmp/window/run1/spell_cast/rust/frames.ndjson` | `8a5382306e783c86a9b2e2b8a2480bb34e5cc6f765d4405f4808d20003326908` | `0eafd0d76187a0f5650ea72a4a3ddd5e27f2bab7e9fd9d486170c1764f9426a7` |

库内副本与源**仅行尾不同**（`\r\n` → `\n`，避免 `git diff --check` 尾随空白告警）；
每行内容逐字节一致、行数不变（36=36）。比较器 `load_stream` 对行做 `strip()`，
行尾不参与判定。

内容：各 36 行 = 34 条协议帧 + 2 条 `ident=null` 控制帧（GameGate `+GD/<rtime>` 时间应答，
`frame_class=server_ack_time`；oracle 在文件行 31/34，rust 在行 33/36）。控制帧没有 u16
消息号，比较器按 ident 对齐 ⇒ 曾把整轮判成 UNDECIDABLE（「缺合法 ident」）。
本 fixture 冻结的就是那个**修复前必红**的输入；compare_runs 在其上的确定性结论
（冻结 rules.json 前提下）见 `selftest_impl.py::t_control_frames` 的断言。

只冻结 `frames.ndjson`（比较输入）；`run.json`/`db_snapshot.json`/`db/` 不入库
（db 指针指向抓帧机本机路径，帧流判定不依赖）。

字节保真：同目录 `.gitattributes` 对 `*.ndjson` 设 `-text`（关闭 EOL 归一），
库内 blob 与上表「库内副本」sha256 逐字节一致（LF，与平台无关）；改动 fixture
必须保持该一致性。
