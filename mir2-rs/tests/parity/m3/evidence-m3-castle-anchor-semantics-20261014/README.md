# ⑧-6 O-1 收口：锚语义级核对工作表（`worksheet`）+ 77 条补登锚逐条复核

卡：`openmir2-m3-castle-file-load`（M3⑧-6：`Castle/List.txt` + `SabukW.txt` + `AttackSabukWall.txt`
→ 门/墙/弓手/守卫实体；锚门 = `castle_oracle_anchor_gate.py`）。
tier：**L**（纯离线判据工具 + 证据；`*.rs = 0`，冻结面 `src/ sql/ MirClient/ crates/protocol/
crates/storage/ bins/ src/BotSrv` 全空）。

## 收的哪条 reviewer observation

来源 = reviewer-6 `7ef3a671`（⑧-6 全文件覆盖面片 `a196e0ef` 的 approve）的 **O-1**（非阻塞）：

> 「作者已在 README『残留 1』如实登记：76 条补登锚的 `(line,token)` 只机判『该行含该串』，
> 『该行**就是**散文所声称的构造』仍是抽样人核；且 `castle.rs` 既有散文可能还有本片未订正的历史
> 行号漂移。建议由后续片用 `audit --json` 清单逐条做**语义级**核对（不在本片范围，不阻塞）。」

## 本片做法（两件事）

1. **`worksheet` 子命令**（`castle_oracle_anchor_gate.py` 新增）：把语义级核对所需的**全部材料**
   渲染成确定性、可复算的一张表。每条锚一行：
   * `write_hits`：该锚文本在 `castle.rs` / `README.md` 的**散文落点**（行号 + 原文）——
     给出「散文声称的构造」；
   * `points`：表内声明的每个 `(line, token)` 与**冻结 `src/` 该行的实际原文**——
     给出「该行实际内容」。
   两种输出：人读文本（`01`）与 `--json`（`01b`，供机读/二次比对）。
   **只呈现材料、不新增判据**：语义层是主观面，做成机械判据会产生假红；判据仍只有
   `check` 的写侧 / 读侧 / 覆盖面 / token 强度四条（未放宽一个字）。
2. **逐条复核 77 条补登锚**（`03-semantic-audit.txt`）：按 `worksheet` 材料，对基线
   `git show 30c52f85d:.../castle_oracle_anchor_gate.py` 与本 head 的 `ANCHORS` 表做差集
   ⇒ 净增 **77** 条（补登 76 + 补 `:939`）、删 **0** 条；逐条比对「散文声称的构造 ⇄ 该行内容」。

## 读数

| 项 | 读数 | 证据 |
| --- | --- | --- |
| `worksheet` | 159 锚全部渲染、散文落点缺失 0、每个 point 命中冻结行 | `01` / `01b` |
| 逐条语义复核 | **77/77 MATCH，0 DRIFT** | `03` |
| `check` | **GREEN**：159 锚命中冻结 `src/` + 写侧在位 + 覆盖区间无未登记/裸锚 | `04` |
| `selftest` | **GREEN**（新增第 10 条牙：worksheet 覆盖全表 + 每条锚有散文落点 + point 可解析 + 缺失锚可辨认） | `05` |
| `mutation` | **GREEN**：MUT1–MUT11 全 RED（新增 MUT11：散文落点全抹掉 ⇒ `write_hits=[]`） | `06` |
| `audit` | **GREEN**：区间 `[(1,2807)]`、已登记 159、未登记 0、裸锚 0 | `07` |
| 外部改坏 | `UserCastle.cs:930`→`:931` ⇒ `check` **RED**（写侧缺锚 + 覆盖面未登记）+ `worksheet` 报 `prose: NOT FOUND`；还原后 GREEN、`castle.rs` 与改前逐字节一致 | `08` |
| 四门禁 | `fmt` rc=0 / `clippy -D warnings` rc=0 / `test` **101 suites 1200 passed 0 failed 22 ignored** / `msgcodegen` 604 rc=0 | `09`–`12` |
| 边界 | 净差量 = 1 个 `.py`（`castle_oracle_anchor_gate.py`）+ 本证据目录；`*.rs=0`、冻结面空 | `13` |

## 复核结论与保留

* **0 处新漂移**。早前已订正的 4 处行号（`UserCastle.cs:311-448→:311-446`、
  `RepairDoor` 的 `UnderWar` 守卫 `:920-923→:920-927`、60s 门 `:951→:930`、满血守卫 `:923→:924`）
  与 `eae27553` 的一处在本轮材料下继续成立。
* **覆盖面蕴含全量**：`COVERED_MARKERS` 已括住整个 `castle.rs`（`[(1,2807)]`），且覆盖面判据要求
  区间内每个 `文件.cs:NNN` 锚都必须在 `ANCHORS` 表内 ⇒ `worksheet` 渲染的 159 条**就是**散文里
  出现的全部 `.cs` 锚（“既有散文可能还有未订正漂移”的风险面已被清点覆盖）。
* **诚实保留（非本片能机判的层）**：`worksheet` 的材料是「散文行 + 冻结行」；「行内容是否**精确**
  表达散文的语义意图」（如「只写 HP 不写 Castle」这类**行为断言**）仍依赖人工阅读——本片已把它
  降为「按表逐条读」，但**不**宣称能机械证明。该层不属于 O-1 所指的「行号/构造对应」。
* 交付不 merge、未自签；本片不因 approve 收 ⑧-6 卡（卡级剩余 DoD = ① `bins/game-svr` 的
  `SabukWConfig→CastleDefenseSpecs` 接线，等 owner 对 `bins` 串行序裁决 ② 真机两侧
  `run_guild_castle_pair.ps1` 同序列 `diff=0`，归 ⑧-10 夹具 + clock-injection + 独占窗口）。

## 边界

见 `13-boundary.txt`：净差量 = `mir2-rs/tests/parity/m3/castle_oracle_anchor_gate.py`（工具）
+ 本证据目录；冻结面 `src/ sql/ MirClient/ crates/protocol/ crates/storage/ bins/ src/BotSrv` 空；
`*.rs = 0`。本片零起栈 / 零端口 / 未 merge / 未自签。
