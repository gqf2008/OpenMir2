# evidence — M3 夹具 defect_3：C# 侧 `M3*SEQ` 事件 JSON 被 Serilog 吞掉末尾 `}`（2026-10-13）

- 卡：`openmir2-m3-fixture-marker-prefix-bug`（该卡剩余 DoD ② 的 defect_3；见窗口车道 comment
  `cf333434`）；上游证据：`.../window/.../evidence-m3-quest-align-window-r2-20261013`。
- 车道：`m3cs` / `openmir2-worker-5`；分支 `worktree-m3-quest-frame-angle`；
  base/合并树：交付前 `git merge origin/master`（合并树 = `8ff8d4de0`），四门禁在**合并树**上重跑。
- tier 自判 = **L**（纯 C# 夹具工具转义 + 自检；不触 oracle/Rust 行为、RNG、时序、落库、战斗数值；不占窗口、不连端口）。

## 现象（真机 window r2，master d4d5b89c）

C# 侧任务 k=0 `phase=frame` 事件行 JSON **缺最外层 `}`**：
`{`=2 / `}`=1，payload len=457，`json.loads` = `Expecting ',' delimiter`；同 run 的 k=1 短文本 frame 正常 `}}`。
⇒ `run_quest_pair.ps1` 的 assemble `事件非法 JSON` 退出（⑥ 任务真机对拍 PREREQ）。
Rust 侧另有 `skip=no_npc_in_view` / `quest_state.jsonl` 无生产者等**独立**阻断（本片不处理，归承接卡）。

## 根因（本片独立定位，可复现）

`MainOutMessage` → oracle `LogService.Info(message)` → `Serilog.ILogger.Information(message)`：Serilog 把**整条消息当消息模板解析**。
当载荷同时满足「**嵌套 `{}`**」且「`text` 里有**字面 `<`/`>`**」时，Serilog 的模板分词会**丢掉末尾一个 `}`**。
最小复现（Serilog 3.1.1，与 oracle 同版本）：

```
Log.Information("AA {\"f\":{\"k\":\"v\"}}");        // 保留  }}   （无尖括号）
Log.Information("AA {\"f\":{\"k\":\"<v>\"}}");      // 输出 ...\"<v>\"}      （丢末尾 }）
Log.Information("AA {\"f\":{\"k\":\"<v>\"} \"z\":1}"); // 保留（内层 `}` 后还有内容）
```

字符矩阵（嵌套载荷里逐个替换 `k` 的值）：**只有 `<` 与 `>` 触发**；`! ( ) [ ] @ # $ % ^ & * ~ / - _ + = ? , ; : .` 等均不触发。
内层 `}` 后若还有内容（不紧邻外层的 `}`）也不触发 ⇒ 触发条件是「尖括号 + 末尾 `}}`」。

## 修法（本片）

生产 escaper `QuestEsc`/`ShopEsc` 把字面 `<`/`>` 以 **Unicode 转义** 写入（`<`/`>`）：
JSON 语义等价（`json.loads("\u003c") == "<"`），但消息里再无字面尖括号 ⇒ Serilog 模板分词退化为纯文本 ⇒ 载荷逐字节保留。
- 覆盖全部 `M3*SEQ` 族：`QuestEsc` = quest/pairtrade/selloff；`ShopEsc` = shop/guildcastle/heropet。
- 解码侧不变：全部 6 个比较器（`quest_diff.py` / `npc_shop_diff.py` / `guild_castle_diff.py` / `hero_pet_diff.py` /
  `party_trade_diff.py` / `selloff_diff.py`）都走 `json.loads` 再比字段值；两侧都用同一 BotSrv，转义后解码值逐字符相同。
- 两个 escaper 由 `private` 改 `internal`，**只为无栈自检直调生产函数**（不改调用方）。

## 判据与读数（全离线、零端口）

| 件 | 内容 | 读数 |
|---|---|---|
| `02-selftest-questseq-green.txt` | `--selftest-questseq`（+4 条新检查：尖括号转义×2 / frame JSON 解码还原 / Serilog 通路载荷逐字节不变） | **GREEN，21/21，exit 0** |
| `03-selftest-mutation1-questesc-red.txt` | 变异：删掉 `QuestEsc` 的尖括号转义 | **RED，2 failed**（`esc-angle-quest-no-raw`、`serilog-path-payload-intact`），exit 1 |
| `04-selftest-mutation2-shopesc-red.txt` | 变异：删掉 `ShopEsc` 的尖括号转义 | **RED，1 failed**（`esc-angle-shop-no-raw`），exit 1 |
| `05-botsrv-all-selftests.txt` | 7 个 BotSrv 生产函数自检（quest/shop/guildcastle/heropet/pairtrade/selloff/attackrole） | 全 **GREEN**，exit 0 |
| `06-fixture-nostack-quest.txt` | `run_quest_pair.ps1 -SelfCheckNoStack`（含 `botsrv-questseq-selftest rc=0`） | **ok=19/19**，exit 0 |
| `07-fixture-nostack-others.txt` | 其余 5 个夹具 `-SelfCheckNoStack` | 全 **GREEN**，exit 0 |
| `08-gates.txt` | 四门禁（合并树；隔离 `CARGO_TARGET_DIR`、`--offline`） | fmt 0 / clippy 0(-D warnings) / **test 1187 passed 0 failed 22 ignored** / msgcodegen 604 rc=0 |
| `10-real-k0-payload-analysis.txt` | 用窗口 r2 的**真机原始行**复算：未修 `{`=2/`}`=1、`json.loads` 失败；补括号 + 生产转义后 `{`=2/`}`=2、`json.loads` OK、解码 text 逐字符还原 | 见件 |

`serilog-path-payload-intact` 是**根因级**牙齿：它用**生产 escaper** 造载荷，经**真实 Serilog 通路**（内联 sink 取回
`RenderMessage()`）断言与入参逐字节相同 ⇒ 把转义改回去（尖括号回流）该载荷会被 Serilog 吞 `}` ⇒ 必 RED（已由 03 证明）。

## 残留 / 未做（如实登记）

- `quest_state.jsonl` **无生产者**（`WRITER_FOUND=0`）：⑥ 真机 assemble 的第三重阻断，**不在本片**（承接卡
  `openmir2-m3-quest-dialog-wiring` / `-quest-persist-observability`，按 comment `cf333434` 归 owner 转派）。
- `src/BotSrv` 里 `skip=` 诊断行的独立前缀（defect_2）已在 master `95291257c` 修；本片不重做。
- 其它 `M3*SEQ` 族若将来在**非 `text` 字段**（`op`/`npc`/`label`/`map`/`branch`）出现尖括号，本片已由两个 escaper 覆盖
  （这些字段同样走 `QuestEsc`/`ShopEsc`）；若新增字段不经 escaper，需同步。
- 真机两侧同序列 diff=0 仍等独占窗口（本片零起栈、零端口）。
- 临时物：`E:/tmp/m3cs-work/**`（scratch Serilog 试验、gate 日志、变异输出）为片外临时，不入库；无仓内残留。
