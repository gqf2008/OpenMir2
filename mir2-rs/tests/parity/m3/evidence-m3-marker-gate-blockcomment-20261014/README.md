# evidence-m3-marker-gate-blockcomment-20261014

卡 `openmir2-m3-fixture-chain-gate`（夹具族共享管线 / 常跑门 umbrella）的**残余收口片 #4**：
关闭 reviewer-5 在上一片（`worktree-m3-scan-guards-codelines`，head `0992ac700e389`，
review `d853bd1a` approve）里登记的两条**非阻塞 Observation**：

- **O-1'**：旧片只说 `_code_lines()` 跳过整行 `#` 注释，并称「`run_pair.ps1` 未使用 `<# ... #>`
  块注释」——**与事实不符**：`run_pair.ps1:2-34` 就是一段 PowerShell 头部块注释。于是
  `scan_guards()` 对**块注释里的**同名 `Assert-M3MarkerPrefix` 仍会当真牙 ⇒ 块注释掉某族的牙
  时门保持 GREEN（fail-open，与 O-1 同族、只是注释写法不同）。
- **O-2'**：上一片证据 `30-boundary.txt` 的叙述把 30/SHA256SUMS 列为「自指新增件」，
  而正文只列 11 个 A、实际 diff A=13 ⇒ 计数措辞小差。

本片把 O-1' 从「隐患」变成**已闭合的判据**（扩展 `_code_lines()` 到可嵌套块注释），
并订正 O-2'/O-1' 的历史措辞。**离线、零起栈、零端口、零 `.rs`**。

tier：**L**（纯夹具/工具；不触 RNG 抽数序列/随机播种、不触时序、不落库、不改战斗数值 ⇒ 等价 + 抽样冒烟即放行）。

## 改法（`marker_prefix_gate.py`）

1. `_code_lines()` 升级为**注释感知**的两族判定（均按代码行计）：
   * 整行 `#` 注释（`lstrip()` 后以 `#` 开头；`#>` 除外，它是块注释闭合符）——沿用；
   * Windows PowerShell **块注释 `<# ... #>`**：**支持嵌套**、可跨行、可出现在行中段；
     块注释内容抹成空格、保留换行 ⇒ 藏在块注释里的牙 = **缺牙 ⇒ RED**。
   fail-closed 取向：无法判定是否注释时按「注释」处理（宁可把真牙判成缺牙 ⇒ RED，
   也不把注释当代码 ⇒ 假绿）。块注释**闭合后同行**的真实牙仍被扫到（不误杀）。
2. `_mutations()` 新增真实变异体 `run_pair_guard_blockcommented.ps1`：把 shop 块的
   fail-closed 牙裹进 `<# ... #>`；`cmd_mutation` 对其断言 `缺生产 fail-closed 牙 ... M3SHOPSEQ`。
3. `selftest` 新增 9 颗牙（**31 → 40**），覆盖：同行/跨行/嵌套块注释里的牙被忽略、
   块注释闭合后的真实牙不误杀、块注释里的去前缀截断被忽略、`_code_lines` 行号/内容口径、
   真身 `run_pair.ps1` 确有块注释（订正旧措辞）、块注释变异体必须 RED。

## 判据与读数（L 级：等价 + 抽样冒烟）

| 项 | 读数 | 原始件 |
| --- | --- | --- |
| **改前 fail-open（O-1' 根因）** | 旧 `_code_lines()`（只跳整行 `#`）对 3 类块注释形态（同行/跨行/嵌套）均把注释里的牙扫成真牙 ⇒ 5 用例中 **3 判错**；新实现 **0/5 判错** | `01` |
| **改后同变异体** | 新门 + `run_pair_guard_blockcommented.ps1`（shop 块牙裹进 `<# ... #>`）：`check` **RED rc=1**（点名 `缺生产 fail-closed 牙 ... M3SHOPSEQ`） | `02` |
| markers 门 | `selftest` **40/40 GREEN rc=0**；`check` **GREEN rc=0** | `10`/`11` |
| 改坏必红（内建） | `mutation` **GREEN rc=0**：5 个变异体（strip / noguard / **guard-commented** / **guard-blockcommented** / bareskip）全 RED；PowerShell 真身牙 good rc=0 / bad rc=1 点名缺前缀 | `12` |
| 共享链深检门 | `fixture_chain_gate.py selftest` **34/34 GREEN rc=0**；`check --require-all` **FIXTURE_CHAIN GREEN runner_ok=10 prereq_or_skip=0 red=0**（含 `marker_prefix_selftest present rc=0 :: SELFTEST ok=40/40`） | `13`/`14` |
| 真身 sha256 前后 | 改前 `e30092de…`（= 上一片合并件）、改后 `16b3a44a…` | `00`/`03` |
| 四门禁 | fmt rc=0 / clippy rc=0(`-D warnings`) / **测试** / msgcodegen **604** rc=0 | `20`–`23` |
| 边界 | 仅 `marker_prefix_gate.py`（M）+ 本证据目录（A）+ 上一片两个证据件措辞订正（M 30-boundary/README + M SHA256SUMS） | `30`/`31` |

## 未做 / 不宣称

- 卡级真机 DoD（各 READY 夹具 `run_*.ps1` 两侧同序列 diff=0）仍需 `openmir2-m3-clock-injection` + owner 独占窗口；
  本片 tier=L 纯离线，**不据此收卡**。
- 另一条同族开放项（`audit --strict` 接线 / `T-42` 撞号重编）仍属 owner 已路由的跨车道写集，本片不碰。
- `_code_lines()` 非完整 PowerShell 词法分析：**行内 `#` 注释**（代码后跟 `#`）不在处理范围。
  当前 `run_pair.ps1` 的 7 处生产牙均为整行调用，无活洞；新增写侧一律整行成牙。
