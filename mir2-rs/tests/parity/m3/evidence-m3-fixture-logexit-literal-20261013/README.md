# evidence — M3 夹具 defect_3 收口片②：把 Serilog 消息模板丢括号的修法落到**日志出口**（reviewer-5 M-1/M-2）

- 卡：`openmir2-m3-fixture-marker-prefix-bug`（该卡 `commit 56d397865` 的 defect_3 片带 3 条非阻塞 Minor，
  其中 **M-1**「根因修法应改日志出口而非逐字段转义」、**M-2**「新增 M3*SEQ 字段须同步过 escaper」由本片收口）。
- 车道 `m3cs` / `openmir2-worker-5`；分支 `worktree-m3-fixture-logexit-literal`；
  base = master `aacb4315f`（`origin`==`walgit`，本 worktree HEAD 已 ff 到该 tip）。
- tier 自判 = **L**（纯 C# 夹具工具；不触 oracle/Rust 行为、RNG、时序、落库、战斗数值；离线、零端口、零起栈）。

## 问题（reviewer-5 M-1）

defect_3 的**功能**修法（`56d397865`）把字面 `<`/`>` 逐字段写成 `\u003c`/`\u003e` 规避 Serilog 模板分词丢末尾 `}`。
reviewer-5 指出**层次不对**：根因是**日志出口** `MainOutMessage` → `LogService.Info(msg)` 把整条消息当
Serilog **消息模板**解析；逐字段转义意味着**每新增一个 `M3*SEQ` 字段都要重记一次**（M-2），漏一个就复发。

## 修法（本片）

在**日志出口**加一处「模板字面量化」：`RobotPlayer.SerilogTemplateLiteral` 把消息里的 `{`/`}` 双写
（`{{`→`{`、`}}`→`}`），`MainOutMessage` 改走它 ⇒ 整条消息按**字面**渲染、逐字节不变，**不再依赖逐字段转义**：

```csharp
internal static string SerilogTemplateLiteral(string message)
    => message == null ? null : message.Replace("{", "{{").Replace("}", "}}");

private void MainOutMessage(string msg)
    => LogService.Info(SerilogTemplateLiteral($"机器人:[{ChrName}] {msg}"));
```

- **行为保持**：`logexit-normal-marker-unchanged` 实测正常标记（无 `<`/`>` 触发词）出口修前/修后渲染**逐字节相同**
  （`raw-ok=True`，见 `02-*`）。本片**不是**顺手改渲染，只改出口的模板解析路径。
- **M-2 消解**：出口修好后新增字段自动安全；`QuestEsc`/`ShopEsc` 的尖括号转义**保留作 defense-in-depth**
  （`json.loads("\u003c") == "<"`，6 个比较器解码侧不变）。
- 范围仅 `MainOutMessage`（全部 `M3*SEQ` 标记的都走它）；`MainOutErrorMessage`/`MainOutWarnMessage` 及其它
  `LogService.Info` 直调点无嵌套 JSON 载荷，未动（最小面）。

## 判据与读数（全离线、零端口）

| 件 | 内容 | 读数 |
|---|---|---|
| `01-fix-diff.txt` | 生产改动 diff（`src/BotSrv/Player/RobotPlayer.cs` + `src/BotSrv/QuestSeqSelfCheck.cs`） | 41 insertions / 1 deletion |
| `02-selftest-questseq-green.txt` | `--selftest-questseq`（+3 条新检查：`logexit-brace-escape` / `logexit-normal-marker-unchanged` / `logexit-payload-intact`） | **GREEN**，含新检查 3/3 PASS，exit 0 |
| `03-mutation-logexit-removed-red.txt` | 改坏必红：`SerilogTemplateLiteral` 改成 `return message;` | **RED，2 failed**（`logexit-brace-escape`、`logexit-payload-intact`），exit 1；还原后 GREEN |
| `04-botsrv-all-selftests.txt` | 7 个 BotSrv 生产函数自检（quest/shop/guildcastle/heropet/pairtrade/selloff/attackrole） | 全 **GREEN**，exit 0 |
| `05-fixture-nostack.txt` | 6 个夹具 `-SelfCheckNoStack` | 全 **GREEN**（quest 19/19、shop 20/20、其余 19/19），exit 0 |
| `06a-gates-fmt-clippy-msgcodegen.txt` | 四门禁之三 | fmt rc=0 / clippy rc=0(-D warnings) / msgcodegen **604** rc=0 |
| `06b-gates-test.txt` | `cargo test --workspace --offline` | **101 suites / 1189 passed / 0 failed / 22 ignored**，rc=0 |
| `07-boundary.txt` | 边界与留痕 | 见件（改动仅 `src/BotSrv/**`（白名单 T-41/T-42 例外）+ 本证据目录；`crates/` `bins/` `sql/` `MirClient` `protocol` `storage` 空） |

新检查的牙齿语义：
- `logexit-brace-escape`：`SerilogTemplateLiteral("a{b}c{}") == "a{{b}}c{{}}"` —— 出口字面量化本身。
- `logexit-normal-marker-unchanged`：正常标记 `raw == literal == payload` —— **行为保持**（不是改渲染）。
- `logexit-payload-intact`：缺陷触发载荷（嵌套 `{}` + 字面 `<>`）`literal == payload` **且** `raw != payload`
  —— 证明旧出口确实丢 `}`（`raw-bytes=76` vs `payload=77`）、出口修法逐字节保留（`literal-bytes=77`）。

## 残留 / 未做（如实登记）

- `quest_state.jsonl` **无生产者**（`WRITER_FOUND=0`）：⑥ 真机 assemble 的第三重阻断，**不在本片**；
  已在本卡 worker-5 的 2026-10-13 comment（`3a665397c6deec22c07bac1ac61bc0dd53b62f1e`）中定位并
  提请 owner 二选一（开 `openmir2-m3-quest-state-dump` 或把会话任务态位子判据降为 informational）。
- `MainOutErrorMessage`/`MainOutWarnMessage` 未改（无嵌套 JSON 载荷；如需统一另开片）。
- 本片零起栈、零端口；未 merge、未自签 review。
