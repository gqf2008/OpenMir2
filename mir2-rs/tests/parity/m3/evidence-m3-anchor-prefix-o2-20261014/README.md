# evidence-m3-anchor-prefix-o2-20261014 —— 锚门「扩展名由 PATHS 推导」O-2 fail-open 收口

本证据包对应卡 `openmir2-m3-fixture-chain-gate`（`owner_hint` = `openmir2-worker-5` / m3cs），
收口下列两条**非阻塞 Observation**：

> **reviewer-6** 在 approve `987aedf7`（`@reviewed_head d52d4a294`，已合并 master `50ecf691b`）后记
> **O-2**：`_PATHREF_RE` 只匹配 `.cs`/`.sql` 扩展名；若未来 `PATHS` 登记其它扩展名的真身文件，
> 其带前缀锚不会被本判据覆盖。
>
> **reviewer-5** 在 approve `2ce5d87b`（`@reviewed_head c989f3fdf`，已合并 master `6dba8719d`）登记的
> **O-B**：同一条，按 README 如实保留未混片。

采集环境：worktree `.claude/worktrees/m3cs`，branch `worktree-m3-anchor-prefix-o2`，
base `origin/master` = `6dba8719d`（含 O-1 片 `505a…`/`c989f3fdf`）。
**零起栈、零占端口。**

## 改了什么（全部在 `mir2-rs/tests/parity/**`，零 `.rs` / 零 `.toml` / 零 `.ps1`）

五个场景锚门（`quest_anchor_gate.py` / `party_trade_anchor_gate.py` /
`guild_castle_anchor_gate.py` / `hero_pet_anchor_gate.py` / `selloff_anchor_gate.py`）：

- **旧实现（硬编码扩展名）**：`FILE_RE`（覆盖面解析）与 `_PATHREF_RE`（路径前缀判据）
  把可接受扩展名写死为 `(?:cs|sql)`（`guild_castle`/`hero_pet` 的 `FILE_RE` 甚至只写 `.cs`）。
  ⇒ 未来 `PATHS` 增列其它扩展名（如 `.txt`/`.gd`）的冻结面文件时，该锚既不进入覆盖面解析、
  也不进入路径前缀判据 ⇒ **该锚的错前缀被整段跳过 = fail-open**。
- **新实现（由 `PATHS` 推导）**：新增 `_anchor_exts(pather)` / `_ext_group(exts)` /
  `_file_re(pather)` / `_pathref_re(pather)` 四个纯函数，扩展名集合 = `PATHS` 登记路径的后缀集
  （去重排序、`re.escape`）；`FILE_RE = _file_re(PATHS)`；`parse_refs(anchor_text, pather=PATHS)`
  与 `_path_prefix_findings(anchor_text, pather=PATHS)` 都按传入 `pather` 逐次构造正则。
  ⇒ 覆盖面解析与路径前缀判据**同时**随 `PATHS` 扩展名自动覆盖，不再有第二个真源。
- 每门 `cmd_selftest` 新增**扩展名推导牙**（O-2）：替身 `PATHS = {"Foo.gd": "src/Evil2/Foo.gd"}`
  下断言 `parse_refs("Foo.gd:1") == ["Foo.gd:1"]`（覆盖面吃）、错前缀 `Bad/Dir/Foo.gd:1` 必红、
  正确路径与裸基名不误伤。
- 每门 `cmd_mutation` 新增 **MUT-O2**：把 `_ext_group` 退化成旧的硬编码 `(?:cs|sql)` ⇒
  `Foo.gd` 错前缀不再被咬（fail-open 复现）⇒ 该牙可失败向、`mutation` 整体可失败向（RED）。

**未动**：`castle_oracle_anchor_gate.py`（⑧-6 工作表锚门，锚以裸基名 `File.cs:NNN` 形书写，
无目录前缀可校验）；各门既有覆盖面/读侧 token/token 强度/工具侧名字锚判据；链深检门与矩阵。

## 判据读数（件）

| 文件 | 内容 |
| --- | --- |
| `01-boundary.txt` | 边界：`git diff --name-status origin/master` + 冻结面 / `*.rs` / `*.toml` / `*.ps1` 空 |
| `10-<gate>-check/selftest/mutation.txt` | 五门 `check`（真身锚 GREEN：14/22/31/27/19 锚，与改前逐数不变）、`selftest`（GREEN，含新 `扩展名推导牙齿(O-2)`）、`mutation`（GREEN，含 `MUT-O2 … RED（漏判=是）`） |
| `20-o2-before-after.txt` | **定值机判**：同一组输入在「改前（`git show HEAD:`）/ 改后」两版上逐格比对 —— 错前缀(含字母/纯非字母)=1/1、spec 区间=0/0、正确路径=0/0、裸基名=0/0、`parse_refs(真身场景)`=锚数相同 **MATCH**；未来扩展名 `.gd` 错前缀 **0→1**（fail-open 堵上）、`parse_refs("Foo.gd:1")` **[]→["Foo.gd:1"]** |
| `21-tool-o2-reverted-selftest-red.txt` | **改坏必红①（工具）**：外部副本把 `_ext_group` 退回硬编码 `cs|sql` ⇒ 五门 `selftest` 全 **rc=1** 且 `扩展名推导牙齿(O-2): RED` |
| `22-tool-neutered-mutation-red.txt` | **改坏必红②（工具）**：外部副本 `_path_prefix_findings` 直接 `return []` ⇒ 五门 `mutation` 全 **rc=1**（`MUT-O1` 显示 GREEN 0+0 表示牙不可满足、`MUT-O2` 显示 RED 表示退化被咬） |
| `30-chain-gate-require-all.txt` | `fixture_chain_gate.py check --require-all` = **FIXTURE_CHAIN GREEN runner_ok=10 prereq_or_skip=0 red=0**（含五门 `*_anchor_selftest/_check`） |
| `32-chain-gate-selftest.txt` | 链深检门 `selftest` = **37/37** |
| `31-fixture-matrix-require-all.txt` | `fixture_matrix.py check --require-all` = **ready=10 pending=0 red=0** |
| `40/41/43/44` | 四门禁：`fmt` rc=0、`clippy -D warnings` rc=0、`test` **101 suites / 1205 passed / 0 failed / 22 ignored**、`msgcodegen --check` 604 rc=0 |

## 口径

- **只读**：`check`/`selftest`/`mutation` 只读仓库文件 + 自身 selftest，不写仓库文件、不起栈、不占端口；
  唯一写盘是 `--out` 临时目录（`E:/tmp/m3cs-work/o2/mut-*/`，不入库）。
- **真身未被放宽**：`check` 改后仍 GREEN 且锚数逐门不变（14/22/31/27/19），
  覆盖面/读侧 token/token 强度/工具侧名字锚**均未改**；`guild_castle`/`hero_pet` 的 `FILE_RE`
  由 `\.cs` 变为由 `PATHS` 推导的 `\.(?:cs)`（语义等价，两门 `PATHS` 全为 `.cs`）。
  本片只把「未登记扩展名的错前缀」从 fail-open 收紧到 fail-closed（新增 `.gd` 向）。
- **tier = L**：纯离线夹具/工具（`*.rs=0`、`*.toml=0`、`*.ps1=0`、冻结面空；不触 RNG 抽数序列/
  时序/落库/战斗数值）⇒ 等价即放行 + 抽样冒烟（本片判据全离线静态机判 + 两向改坏必红）。
- **证据 hash 口径**：`SHA256SUMS.txt` 一律按仓库口径（LF 归一 / git blob）；
  `verify --check-extra` 与 `eolcheck` 读数见 review 归档（复核命令见下）。

复核命令：

```bash
python -I tools/evidence-hash/verify_sha256sums.py --check-extra mir2-rs/tests/parity/m3/evidence-m3-anchor-prefix-o2-20261014
python -I tools/evidence-hash/verify_sha256sums.py eolcheck mir2-rs/tests/parity/m3/evidence-m3-anchor-prefix-o2-20261014
```

## 残留

- 卡级真机 DoD（各 READY 夹具 `run_*.ps1` 两侧同序列 `diff=0`）仍待世界侧实现卡 +
  `openmir2-m3-clock-injection` + owner 独占 `-Go` 窗口；本片不据此收卡。
- reviewer-5 `2ce5d87b` 的 **O-A**（真实目录名恰形如 `3966-4105` 且紧跟 `:` 会被当 spec 区间）
  与 **O-C**（路径计数文档口径）非本片范围；O-C 已在 O-1 片 `610d7258` 订正。
