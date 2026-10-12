# 夹具 EOL「生产者写 LF」根因修 —— 证据包（2026-10-13，m3bag / openmir2-worker-4）

卡：`openmir2-m3-attack-pair-fixture`（issue `76cc08b8`）· worktree `m3bag` · branch `worktree-m3-bag-equip`。
owner 2026-10-13 01:15 块（「让**生产者**写 LF——EOL 陷阱同族第 5 次，且这次是我们自己的自检脚本造成的」）。

## 0. 修的是什么（根因）

同族坑（历史 5 次：C6 F9-1 / M4 N-1 / lifecycle N-1 / `170a51b9` / 本次）：
Windows PowerShell / Python 的**默认**文本写出是 CRLF，而 `mir2-rs/.gitattributes` 对
`*.txt/*.json/*.jsonl/*.log/*.md/*.cs/*.rs/...` 声明 `text eol=lf` ⇒ 提交进仓库的是 LF；
`SHA256SUMS.txt` 若按盘上 CRLF 记就**只在作者工作树自洽**，干净检出 / 仓库字节口径必红。
本次先例 = m3cs 片里的 `window/00-selfcheck-nostack.txt`，而它正是
`run_attack_pair.ps1 -SelfCheckNoStack` 的**自检转录件**（`pwsh ... > file` 重定向产生 CRLF）。

本片从**生产者侧**收口，而不是每条证据事后归一：

1. **`m3hashutil.ps1` 新增 LF 写出**：`Write-M3LfText` / `Write-M3LfLines` / `Set-M3LfContent`
   （UTF-8 无 BOM、把 CRLF 与孤立 CR 一律归一为 LF）；并在该文件里**显式覆盖内置 `Set-Content`**
   ⇒ 凡是 dot-source 它的夹具（全部 m3 脚本经 `gate_guards.ps1` 或直接 dot-source 拿到）写出的文本
   一律 LF，**无需逐个调用点手改、也防将来又写回 CRLF**。参数只覆盖夹具实际用到的
   `-Path/-LiteralPath/-Value/-Encoding` + 管道输入（矩阵见 `07`）。
2. **`run_attack_pair.ps1`**：`[IO.File]::WriteAllLines`（用 `Environment.NewLine`，绕过覆盖）12 处
   改用 `Write-M3LfLines`；新增 `-SelfCheckOut <path>`——把无栈自检的**完整转录由脚本自己**以 LF
   落盘（替代 `pwsh ... > window/...txt` 的 CRLF 重定向）；并在真机运行的落盘件后追加
   **EOL 护栏**（`eolcheck`）。
3. **Python 侧**：`attack_stream_gate.py` 3 处 `open(..., "w")` 补 `newline="\n"`；
   `server_state_gate.py` 落盘本已 `newline="\n"`，selftest 新增「写出件不得含 CRLF」断言。
4. **机判**：`tools/evidence-hash/verify_sha256sums.py` 新增 **`eolcheck`** 子命令——不看清单，
   直接咬**刚写出的文件**：受 git 行尾归一的文本件盘上含 CRLF，或其 git blob 含 CRLF ⇒
   **判红并点名**（`-text` 冻结件 / 仓外输入不适用）。`selftest` 由 11/11 扩到 **13/13**。

## 1. 判据（机器可判）与原始件

| # | 判据 | 结果 | 原始件 |
| --- | --- | --- | --- |
| ① | `verify_sha256sums.py selftest`（含新增 eolcheck×3） | **GREEN 13/13**（rc=0） | `01-...txt` |
| ② | `server_state_gate.py --selftest`（含 writer-lf-no-crlf） | **GREEN 7/7**（rc=0） | `02-...txt` |
| ③ | `attack_stream_gate.py selftest` | **GREEN**（rc=0） | `03-...txt` |
| ④ | `run_attack_pair.ps1 -SelfCheckNoStack`（含 23a/23b） | **rc=0 / SELFTEST_RED GREEN**（23a ok=True、23b LF rc=0 / CRLF rc=1 点名 bad.txt=True） | `04-...txt` |
| ⑤ | 改坏：`Set-Content` 覆盖改回默认 cmdlet ⇒ 机判必红点名 | **`MUTATED_CRLF_PRESENT=True`**、`LF_SELFTEST ok=False reason=setcontent_override_not_lf`、`eolcheck RED ... mutated.txt`、`EOLCHECK_RC=1` | `05-...txt` |
| ⑥ | `-SelfCheckOut` 自落转录 | `transcript_bytes=8114 crlf=0 cr=0 lf=48`；`eolcheck GREEN 1/1`；自检 rc=0 | `06-...txt` |
| ⑦ | 覆盖参数形态矩阵（array/pipeline/Get-Content 管道/LiteralPath/内嵌 CRLF/jsonl/无值） | 全部 `crlf=False`；故意绕过覆盖写 CRLF 的 `src.txt` 被 `eolcheck` RED 点名 | `07-...txt` |
| ⑧ | 共用脚本回归（bag 自检 / M3 门禁自检） | bag **rc=0 GREEN**；`selftest_gate` **total=65 failed=0 GREEN**，输出无 CRLF | `08-...txt` |
| ⑨ | 四门禁（零 `.rs`） | fmt 0 / clippy 0 / **test 84 suites 682 passed 0 failed 16 ignored** / msgcodegen 604 | `09-...txt` |
| ⑩ | 覆盖的调用点清单 | m3 夹具内 `Set-Content` 全部经覆盖；`WriteAllLines` 已改 `Write-M3LfLines` | `10-...txt` |

## 2. 口径与范围

- **零 `.rs`**；未改 `src/**`（C# oracle）、表结构 / 脚本语法 / 数据文件格式。
- `eolcheck` 只看**仓库内、受 git 行尾归一的文本件**；`-text` 冻结字节目录与仓外输入不适用（不误报，见 `01` 的 13/13）。
- `-Encoding` 参数仅为兼容既有调用点保留；实现固定 UTF-8 无 BOM（纯 ASCII 内容字节不变；比 PS 5.1 的 utf8-BOM 对 Python 读取更安全）。

## 3. 登记（不阻塞）

- **未覆盖的写集**（不 dot-source `m3hashutil.ps1`，故 `Set-Content` 覆盖不生效）：
  `tools/botload/load_gate.ps1`、`tools/botload/run_bots.ps1`、`mir2-rs/tests/parity/m3/start_rust_gates.ps1`
  的少量落盘（bot 摘要 / artifacts.sha256.json / pid 文件）。它们不在本卡写集，**已由 `eolcheck`
  在交付时兜底**（一旦进证据包/清单就会点名）；如需要同样收口请另开卡。
- `run_pair.ps1` / `run_m3_gate.ps1` / `rng-baseline.ps1` / `start-csharp-hook.ps1` /
  `run_bag_equip_pair.ps1` / `selftest_gate.ps1` 的 `Set-Content` **未改调用点**，经覆盖自动 LF
  （回归见 `08`）。
- `Tee-Object -FilePath` 写的 `sessions/*.log` 仍走 cmdlet（CRLF）；属未跟踪会话日志、不进证据清单。
- 真机两侧攻击对拍（DoD②）仍等窗口，不在本片。

## 4. 残留物清单

- 本目录 12 件（含 `README.md`/`SHA256SUMS.txt`），`eolcheck` GREEN、`verify --check-extra` GREEN。
- `E:/tmp/m3bag-eol-target/`（四门禁隔离 target，**保留**供快速复跑）。
- `E:/tmp/m3bag-ev-*.txt`、`E:/tmp/m3bag-eol-*.txt`、`E:/tmp/m3bag-*-probe.ps1`（本 tick 原始件，未入包）。
- 未新建真机会话目录；未起栈、未占端口。
