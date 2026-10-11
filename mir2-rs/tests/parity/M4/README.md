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
`run_npc_cases.py` 的全部 7 条用例都只跑两侧脚本 harness）。2026-10-12 owner 裁定
`e808ec99`（路线①）后，**商人 3 条也走 port-free**，`npc-cases.json` 里已无 `runner == "window"`。

## 写盘 vs 只回显（M4 收尾，卡 `openmir2-m4-readiness-write-mode`）

| 命令 | 写盘 | 说明 |
| --- | --- | --- |
| `npc_window_readiness.py`（默认参数） | **否** | 只把门结果回显到 stdout |
| `npc_window_readiness.py --no-run-cs` | **否** | 只做源码能力检查，仍只回显 |
| `npc_window_readiness.py --write-evidence [--out …]` | **是** | 唯一写 `--out`（默认 `evidence/M4/npc-window-readiness.txt`）的开关 |
| `npc_window_readiness.py selftest` / `selftest-write` | **否** | 只回显；`selftest-write` 就地合成回归副本、跑完删除 |
| `run_npc_cases.py` | **是** | 默认写 `evidence/M4/npc-cases.txt`（确定性，重跑字节不变） |
| `m4_parity.py --envir …` | **是** | 默认写 `evidence/M4/*`（①③ 证据；确定性） |
| `evidence_hashes.py` | **否** | 复核；重生成清单走显式 `--write` |

任何**默认只读**命令跑完，`git status --porcelain -- mir2-rs/tests/parity/evidence/M4` 必须为空
（`npc_window_readiness.py selftest-write` 机判这一点）。

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
- 可跑（port-free）**7 条**：传送×2、行会、任务、商店、修理、仓库 —— 两侧
  `script-parity-cs run` vs `script-tool flow-run`，同脚本 + 同 label + 同初始状态逐行 diff，
  全部 GREEN（`== M4② 结果：可跑用例 7（GREEN） / 等窗口用例 0 ==`）。
- 商人 3 条（商店 `@changeGold` / 修理 `@StdModeFunc26` / 仓库 `@zum_bind1`）原先 `runner == "window"`：
  脚本首行含商人头 `(@buy/@storage …)`，C# 参照在 `ScriptParsers.cs:713/733/763` 把 NPC 强转
  `IMerchant`，而 `tools/script-parity-cs` 的 NPC 代理是 `INormNpc` ⇒ `InvalidCastException`。
  **路线①（owner 授权写集扩到 `tools/script-parity-cs/**`）已修**：按脚本头选 `IMerchant` 代理、
  `IList<>` 型 `get_` 返非空集合、并把代理注册进 `SystemShare.ActorMgr`（`goto @label` 靠它按
  ActorId 取 NPC）；Rust 侧 `flow-run` 的 `take/give` 改**逐件一行**（与 oracle `SendDelItems`/
  `SendAddItem` 同粒度）。⇒ 3 条转 `runner == "flow"`，两侧逐行差 0。
- **Rust 服务侧**的 NPC/商人派发（`bins/game-svr`）**不属本卡** —— 属 §6 的 M3「NPC/商店」系统对齐；
  `npc_window_readiness.py` 把这一缺口机判登记为 `Rust 服务侧（M3 承接）`。
- 产出：`evidence/M4/npc-cases.txt`（7 条结果 + 可跑性门结果）、
  `evidence/M4/npc-window-readiness.txt`（商人 3 条可跑性门原始输出）。

### 商人用例可跑性门（`npc_window_readiness.py`）

商人 3 条「能不能 port-free 跑」不由人说了算，由本门机判（以 **C# 夹具路线①** 为裁决；
Rust 服务侧能力只登记为 M3 承接）：

```powershell
python -I tests/parity/M4/npc_window_readiness.py                    # 只回显 stdout（默认不写盘）
python -I tests/parity/M4/npc_window_readiness.py --no-run-cs        # 只做源码能力检查（快；仍不写盘）
python -I tests/parity/M4/npc_window_readiness.py --write-evidence  # 显式写 evidence/M4/npc-window-readiness.txt
python -I tests/parity/M4/npc_window_readiness.py selftest          # 牙齿（裁决逻辑）5/5
python -I tests/parity/M4/npc_window_readiness.py selftest-write    # 牙齿（写盘模式）3/3：默认不写盘 + 回归必 RED
```

- 退出码：`0 = READY`（路线① 就绪，商人 3 条可 port-free）/ `3 = BLOCKED`（夹具未修）/ 
  `2 = PREREQ`（缺 dotnet/源码/用例）。
- 裁决以 **C# 夹具（路线①）** 为准；「Rust 服务侧」行只登记为 **M3 承接**，不阻塞本卡。
- 牙齿：合成的 5 种组合里，C# 代理 = `INormNpc` ⇒ `BLOCKED`，改成 `IMerchant` ⇒ `READY`
  （Rust 派发/依赖不再影响本卡裁决）⇒ 不能靠改一半蒙过去。
- **写盘必须显式**（卡 `openmir2-m4-readiness-write-mode`，R-1 残留）：默认**只回显 stdout**，
  `--no-run-cs` 与普通运行都不写盘；只有 `--write-evidence` 才写 `--out`。
  `selftest-write` 机判这一点，并把「改回默认写盘」的合成回归跑成 RED 原始输出
  （留档于 `../evidence/M4/M4收口报告-20261012.md` §26）。
- **证据幂等**（评审 N-1 同类）：显式 `--write-evidence` 重跑（含端口起落）得到的
  `../evidence/M4/npc-window-readiness.txt` **逐字节相同** —— 端口快照是易变信息项，只回显到
  stdout 并落到 `mir2-rs/target/m4/npc-window-readiness-ports.txt`（`target/` 不入库、不入 hash 清单）；
  源码路径用仓库相对路径。否则评审按 `reviewer_note` 跑本门再跑 `evidence_hashes.py` 会误判为红。

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
| `run_npc_cases.py` | ② runner（7 条 port-free 用例 + 商人可跑性门结果） |
| `npc_window_readiness.py` | 商人 3 条可跑性门（C# 夹具路线①裁决 + Rust 服务侧 M3 承接登记；写盘显式 `--write-evidence`） |
| `npc-cases.json` | ② 6 类用例定义（含判红判据） |
| `fixtures/macroenvir/` | 宏展开边界语料（2 条 `#DEFINE`） |
| `../evidence/M4/parsecount-both-sides.md` | ① 计数表 |
| `../evidence/M4/multiset-gate.txt` | ③ 逐行多重集比对原始输出 |
| `../evidence/M4/macro-parity.txt` | 宏边界语料两侧结构摘要 |
| `../evidence/M4/npc-cases.txt` | ② 7 条用例运行结果 + 可跑性门结果 |
| `../evidence/M4/npc-window-readiness.txt` | ② 商人 3 条可跑性门原始输出（路线① READY） |
| `../evidence/M4/parse-stats-{rust,cs}.json` | **两侧原始件 JSON**（下游 ①③ 门禁与 `lines-*.jsonl` 都消费它；权威） |
| `../evidence/M4/parse-stats-{rust,cs}.stdout.txt` | 两侧 harness **stdout 转录**（仅供人读；worktree/repo 绝对路径已归一为 `<mir2-rs>`/`<OpenMir2>`，故与 JSON 原始件不是同一物） |
| `../evidence/M4/macro/parse-stats-{rust,cs}.json` | 宏边界语料的**两侧原始件 JSON** |
| `../evidence/M4/macro/parse-stats-{rust,cs}.stdout.txt` | 宏边界语料的 **stdout 转录**（路径已归一） |
| `../evidence/M4/corpus-directives.txt` | 语料 `#DEFINE`/`#INCLUDE`/`#SETHOME` 事实计数 |
| `../evidence/M4/evidence-hashes.json` | 证据 hash 清单（LF 归一口径；`evidence_hashes.py` 生成/复核） |
