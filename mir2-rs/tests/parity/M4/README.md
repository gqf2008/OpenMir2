# M4 收口门禁（parsecount / 关键 NPC 用例 / 逐行多重集差）

卡片：`openmir2-m4-parity-cases`（`docs/Rust重写设计文档.md` §6 M4、§17）。
本目录落地 M4 剩余三项**可判**验收：① parsecount 两侧数表、② 6 类关键 NPC 用例、
③ 『双侧逐行多重集差 = 0』门禁。原则同 §6.0：判据可测、能证伪、差异要么 0 要么登记、
证据四件套（命令 + 原始输出 + 路径 + hash）。

## 一条命令

```powershell
# 在 mir2-rs/ 下
python -I tests/parity/M4/m4_parity.py --envir "E:/MirServer/M2GameSvr/Envir"   # ① + ③
python -I tests/parity/M4/run_npc_cases.py                                      # ②
```

两条命令都不占端口、不起栈（`m2_parity.py` 只跑两侧 parse harness；
`run_npc_cases.py` 的可跑用例只跑两侧脚本 harness）。需要 5000/7000/7100/7200 的 ②
用例本轮**登记为等窗口**（见 `npc-cases.json` 的 `runner == "window"`）。

## ① parsecount 两侧对拍数表

- 口径：**同一份语料**（`E:\MirServer\M2GameSvr\Envir`，639 个 `.txt`）上分别跑
  - Rust：`cargo run -p mir2-script-tool -- parse-stats <Envir> --json …`
  - C# 参照：`dotnet run --project tools/script-parity-cs -- <Envir> …`
    （只读引用 `src/Modules/ScriptEngine`，DispatchProxy 假 NPC/商人，不改参照代码）
- 产出：`tests/parity/evidence/M4/parsecount-both-sides.md`
  （四类计数：语句(过程)/条件/动作/宏展开，逐项给两侧数字与差）。
- **宏展开**的口径：C# 参照不暴露『常量替换次数』计数器 ⇒ C# 列取语料事实值
  `0`（grep 见 `evidence/M4/corpus-directives.txt`），Rust 计数 `define_substitutions`
  同样为 0；宏展开**效果**由逐文件结构摘要（展开后的命令/参数/说词全文）逐字节比对代偿。
  边界语料 `fixtures/macroenvir`（含 2 次 `#DEFINE` 替换）两侧结构摘要逐字节相同，
  见 `evidence/M4/macro-parity.txt`。

## ② 6 类关键 NPC 用例

- 用例定义：`npc-cases.json`（6 类：商店 / 传送 / 修理 / 仓库 / 行会 / 任务）。
- 可跑（port-free）4 条：传送×2、行会、任务 —— 两侧 `script-parity-cs run` vs
  `script-tool flow-run`，同脚本 + 同 label + 同初始状态逐行 diff。
- 等窗口 3 条：商店、修理、仓库 —— 脚本含商人头 `(@buy …)`，C# 参照在
  `ScriptParsers.cs:713+` 把 NPC 转 `IMerchant`，port-free 夹具（`INormNpc`）不含 ⇒
  `InvalidCastException`；且这三类的副作用/DB 变更本就需要整栈观测。本轮只登记判据
  （行为逐行 + `scoped_diff.py diff --canonical` 作用域字段级 diff=0），**不计通过**。
- 产出：`evidence/M4/npc-cases.txt`。

## ③ 『双侧逐行多重集差 = 0』门禁

- 复用 M3 先例 `tests/parity/m3/scoped_diff.py` 的**多重集/计数式**口径
  （`multiset_delta` / `cmd_diff`），**不另造第二套计数实现**。
- 归一：把两侧 parse-stats JSON 压成 `scoped_diff` 行格式
  （`{table, row_sha256, line}`），行语料 = 全局计数 + 逐文件结构摘要 + 逐文件原文摘要
  + 逐条脚本错误行；再调 `scoped_diff.py diff` 逐表按**出现次数**比对。
  ⇒ `A=[x,x] vs B=[x]` 会判红（旧 set 差集实现会假绿，M3 已修）。
- 产出：`evidence/M4/multiset-gate.txt`（`GREEN: 作用域内 640 张表逐行一致`）。
- `m4_parity.py` 的默认命令会**先跑 selftest**（插行/删行/重复行必红），不过则直接判红
  ⇒ 这条命令本身自带牙齿。

## 牙齿（§6.0-2：恒绿的门禁等于没有门禁）

```powershell
python -I tests/parity/M4/m4_parity.py selftest   # 多重集门禁自检 5/5：插行/删行/重复行必红
python -I tests/parity/M4/m4_parity.py redcheck   # 篡改副本：计数+1、错误行改 1 字符 → 真实门禁必红
python -I tests/parity/M4/evidence_hashes.py selftest  # 证据 hash 复核器自检 4/4（改 hash/多文件/少文件必红）
```

`m4_parity.py selftest` 的 5 个用例（不需要语料/端口）：

| 用例 | 期望 |
| --- | --- |
| 两侧行料相同 | exit 0（绿） |
| 一侧插入一行 | exit 1（红） |
| 一侧删除一行 | exit 1（红） |
| `A=[x,x]` vs `B=[x]`（重复行） | exit 1（红；旧 set 差集会假绿） |
| `A=[x,x]` vs `B=[x,x]` | exit 0（绿；防止「见重复即红」的过度收紧） |

## 证据 hash 口径（评审 N-1）

本仓库 `core.autocrlf=true`：工作树是 CRLF，Git blob 是 LF。卡/报告/清单里的证据 hash
**统一口径**为 `SHA-256(文件字节，先把 CRLF 归一为 LF)`，它等于
`git show <commit>:<path> | sha256sum`（git blob 内容 hash），**不等于**对工作树直接跑
`sha256sum` 得到的 CRLF 值。评审 N-1 就是这个口径差。

```powershell
python -I tests/parity/M4/evidence_hashes.py            # 复核：逐项比对 + 缺失/多余检查
python -I tests/parity/M4/evidence_hashes.py --write    # 改动证据后显式重生成清单
python -I tests/parity/M4/evidence_hashes.py selftest   # 牙齿：改 hash / 多文件 / 少文件必红
```

清单 `../evidence/M4/evidence-hashes.json` 是「被审计证据」的锚点，**不**在 `m4_parity.py`
里自动改写（自动改写会把它退化成恒绿）；`--write` 是显式动作。

两侧 harness 的 stdout 转录会先把本 worktree/repo 的绝对路径归一为 `<mir2-rs>`/`<OpenMir2>`，
故证据字节跨 worktree 可复现（同 N-1 类问题）。

## 产物与复现

| 文件 | 说明 |
| --- | --- |
| `m4_parity.py` | ① + ③ 门禁（跑两侧 + 出表 + 多重集比对 + 宏边界 + 自检/红检） |
| `evidence_hashes.py` | 证据 hash 口径复核器（LF 归一 = git blob；--write / verify / selftest） |
| `run_npc_cases.py` | ② runner（可跑用例 vs 等窗口用例） |
| `npc-cases.json` | ② 6 类用例定义（含判红判据） |
| `fixtures/macroenvir/` | 宏展开边界语料（2 条 `#DEFINE`） |
| `../evidence/M4/parsecount-both-sides.md` | ① 计数表 |
| `../evidence/M4/multiset-gate.txt` | ③ 逐行多重集比对原始输出 |
| `../evidence/M4/macro-parity.txt` | 宏边界语料两侧结构摘要 |
| `../evidence/M4/npc-cases.txt` | ② 用例运行结果 + 等窗口判据 |
| `../evidence/M4/parse-stats-{rust,cs}.json` | 两侧原始 JSON（证据四件套的原始件） |
| `../evidence/M4/corpus-directives.txt` | 语料 `#DEFINE`/`#INCLUDE`/`#SETHOME` 事实计数 |
| `../evidence/M4/evidence-hashes.json` | 证据 hash 清单（LF 归一口径；`evidence_hashes.py` 生成/复核） |
