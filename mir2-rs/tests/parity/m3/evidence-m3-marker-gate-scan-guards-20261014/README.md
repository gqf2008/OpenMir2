# evidence-m3-marker-gate-scan-guards-20261014

卡 `openmir2-m3-fixture-chain-gate`（夹具族共享管线 / 常跑门 umbrella）的**残余收口片 #3**：
关闭 reviewer-6 在 markers 前缀口径卡（`openmir2-m3-fixture-marker-prefix-bug`，merge 检查点
`350cb923b`）上留下的**非阻塞 Observation O-1**（`scan_guards` 未走 `_code_lines` ⇒ 注释行可能误报）。
**离线、零起栈、零端口、零 `.rs`**。

tier：**L**（纯夹具/工具；不触 RNG 抽数序列/随机播种、不触时序、不落库、不改战斗数值 ⇒ 等价 + 抽样冒烟即放行）。

## 为什么有它（O-1：一个真实的 fail-open）

`marker_prefix_gate.py` 的判据 2 要求 `run_pair.ps1` 为 `COMPARATORS` 里每个序列都接上
`Assert-M3MarkerPrefix` 生产牙。但旧实现 `scan_guards()` 对**整份文件文本**做
`GUARD_RE.finditer(text)`——**注释行**里的同名调用也被当成真实牙：

```python
# 旧（fail-open）
return [(m.group(2), m.group(1)) for m in GUARD_RE.finditer(text)]
```

⇒ 把某族的牙**整行注释掉**（改代码时最常见的临时停用形态），门仍 `GREEN`、`selftest` 仍 28/28
（见 `01`：在 `origin/master` 的原门上实测 `rc=0`）——**在场却坏无声**，正是本仓库反复出现的
fail-open 形态。reviewer-6 的 O-1 因此登记「注释行可能误报」；本片把它从「可能」变成**已闭合的判据**。

## 改法（`marker_prefix_gate.py`，+35/−2）

1. `scan_guards()` 改**逐代码行**匹配：`for _i, line in _code_lines(text): GUARD_RE.finditer(line)`。
   `_code_lines()` 跳过整行 `#` 注释（与同文件 `scan_strips()`、`_scan_botsrv_text()` 同口径）。
   ⇒ **注释掉的牙 = 缺牙 ⇒ RED**；真实牙（单行调用）不受影响（`run_pair.ps1` 里 7 处牙全为单行）。
2. `_mutations()` 新增真实变异体 `run_pair_guard_commented.ps1`（把 shop 块牙前缀 `#` 整行注释），
   `cmd_mutation` 对其断言 `缺生产 fail-closed 牙 ... M3SHOPSEQ`（既有 else 分支覆盖）。
3. `selftest` 新增 3 颗牙（**28 → 31**）：
   * `scan-guards-catches-real`：真实单行牙被扫到（`[("M3SHOPSEQ","shopMarkersPath")]`）；
   * `scan-guards-ignores-comment`：**根因复现**——同样一行加 `#` 前缀 ⇒ 扫不到（旧实现会扫到）；
   * `mutation-guard-commented-red`：注释变异体必须 RED 且点名 `M3SHOPSEQ`。

## 判据与读数（L 级：等价 + 抽样冒烟）

| 项 | 读数 | 原始件 |
| --- | --- | --- |
| **改前 fail-open（根因）** | 原门（`origin/master 7caf97454`）+ `M3SHOPSEQ` 牙注释掉：`check` **GREEN rc=0**、`selftest` **28/28 rc=0** | `01` |
| **改后同变异体** | 本片 + 同一注释变异体：`check` **RED rc=1**（点名 `缺生产 fail-closed 牙 ... M3SHOPSEQ`）、`selftest` **29/31 rc=1**（`real-guards-wired`/`check-green` FAIL） | `02` |
| markers 门 | `selftest` **31/31 GREEN rc=0**；`check` **GREEN rc=0** | `10`/`11` |
| 改坏必红（内建） | `mutation` **GREEN rc=0**：4 个变异体（strip / noguard / **guard-commented** / bareskip）全 RED；PowerShell 真身牙 good rc=0 / bad rc=1 点名缺前缀 | `12` |
| 共享链深检门 | `fixture_chain_gate.py selftest` **34/34 GREEN rc=0**；`check --require-all` **FIXTURE_CHAIN GREEN runner_ok=10 prereq_or_skip=0 red=0**（含 `marker_prefix_selftest present rc=0 :: SELFTEST ok=31/31`） | `13`/`14` |
| 真身 sha256 前后一致 | 变异前后 `marker_prefix_gate.py` sha256 同为 `e30092de…`（注释变异在临时副本/已还原，未改入库件） | `00`/`03` |
| 四门禁 | fmt rc=0 / clippy rc=0(`-D warnings`) / **101 suites 1200 passed 0 failed 22 ignored** / msgcodegen **604** rc=0 | `20`–`23` |
| 边界 | 仅 `marker_prefix_gate.py`（M）+ 本证据目录；冻结面 / `*.rs` / `*.toml` 均空 | `30`/`31` |

## 未做 / 不宣称

- 卡级真机 DoD（各 READY 夹具 `run_*.ps1` 两侧同序列 diff=0）仍需 `openmir2-m3-clock-injection` + owner 独占窗口；
  本片 tier=L 纯离线，**不据此收卡**。
- 另一条与 `scan_guards` 同族的开放项（`audit --strict` 接线 / `T-42` 撞号重编）仍属 owner 已路由的跨车道写集，
  本片不碰（见上一片 `evidence-m3-chain-gate-o2-20261014` §「未接 audit --strict」）。
- `_code_lines()` 仅跳过整行 `#` 注释；PowerShell `<# ... #>` 块注释不在本片范围（`run_pair.ps1` 未使用）。
