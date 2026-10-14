# evidence-m3-anchor-prefix-o1-20261014 —— 锚门「路径前缀」判据 O-1 fail-open 收口（纯非字母错误前缀）

本证据包对应卡 `openmir2-m3-fixture-chain-gate`（`owner_hint` = `openmir2-worker-5` / m3cs），
收口 reviewer-6 在 approve `987aedf7`（`@reviewed_head d52d4a294`，已合并为 master `50ecf691b`）登记的
**非阻塞 Observation O-1**：

> O-1（非阻塞）：`_path_prefix_findings` 为避开 `:3966-4105/File.cs` 这类 spec 区间误报，要求每个目录段含
> `[A-Za-z_]`；后果是「纯数字/点/横线组成的错误前缀」（如 `1/2/Messages.cs:62`）仍会被跳过（实测 0 finding）。

采集环境：worktree `.claude/worktrees/m3cs`，branch `worktree-m3-anchor-prefix-o1`，
base `origin/master` = `a4058a19a`（⑧/⑨/⑩ 全部夹具族 + ②③ 客户端联调片均已合入）。
**零起栈、零占端口。**

## 改了什么（全部在 `mir2-rs/tests/parity/**`，零 `.rs` / 零 `.toml` / 零 `.ps1`）

五个场景锚门（`quest_anchor_gate.py` / `party_trade_anchor_gate.py` /
`guild_castle_anchor_gate.py` / `hero_pet_anchor_gate.py` / `selloff_anchor_gate.py`）的
`_path_prefix_findings`：

- **旧 guard（fail-open）**：`if not comps or not all(re.search(r'[A-Za-z_]', c) for c in comps): continue`
  —— 为规避 `:3966-4105/File.cs` 的 spec 区间误报，把**所有**不含字母的目录段整段跳过，
  于是 `1/2/Messages.cs:62` 这类**纯非字母错误前缀**也一起被放过。
- **新 guard（只剔 spec 区间首段）**：仅当首段**紧跟在锚文本 `:` 之后**且形如 `数字[-数字]`
  （新常量 `_SPEC_RANGE_COMP_RE = ^\d+(?:-\d+)?$`）时，才把它当作 spec 区间剔除；
  其余目录段（含纯数字/点/横线）一律按目录前缀判 —— 错前缀必 RED，spec 区间不误伤。
- 每门的 `cmd_mutation` 把 `MUT-O1` 牙加宽：除既有 `src/Evil/<基名>:1`（含字母错前缀）外，
  新增 `1/2/<基名>:1`（纯非字母错前缀）**必须 RED**，并断言 `:3966-4105/<基名>:1`
  （spec 区间）与正确路径 / 裸基名**均不得产生 finding**。输出改为 `RED（1+1 项）`。

**未动**：`castle_oracle_anchor_gate.py`（⑧-6 工作表锚门，锚以裸基名 `File.cs:NNN` 形书写，无目录前缀可校验）、
各门既有覆盖面/读侧 token/token 强度/工具侧名字锚判据、`FILE_RE`（覆盖面解析）、链深检门与矩阵。

### O-2（未收口，如实登记）

reviewer-6 同批的 **O-2**：`_PATHREF_RE` 只匹配 `.cs`/`.sql` 扩展名，未来 `PATHS` 登记其它扩展名时其带前缀锚
不被覆盖。**本片未动**：当前五门 `PATHS` 全为 `.cs`（10/10），且真正的扩展名泛化必须同时改
`FILE_RE`（覆盖面解析，五门中 `guild_castle`/`hero_pet` 写死 `.cs`、其余写死 `(?:cs|sql)`），
属**跨门覆盖面口径**的独立变更，不宜与 O-1 混片；保持 reviewer-6 原判「无可观测差异」。

## 判据读数（件）

| 文件 | 内容 |
| --- | --- |
| `01-boundary.txt` | 边界：`git diff --name-status origin/master` + 冻结面 / `*.rs` / `*.toml` / `*.ps1` 空 |
| `10-<gate>-check/selftest/mutation.txt` | 五门 `check`（真身锚 GREEN）、`selftest`（GREEN）、`mutation`（GREEN，含 `MUT-O1 … RED（1+1 项）`） |
| `20-o1-failopen-before-after.txt` | **定值机判**：同一组锚文本在「改前（`git show HEAD:` 版本，含旧 guard）/ 改后」两版 `_path_prefix_findings` 上逐格比对 —— `1/2/` 前缀 0→1（fail-open 复现并堵上）、`src/Evil/` 1→1、`:3966-4105/` 0→0、正确路径 0→0、裸基名 0→0 |
| `21-scenario-badprefix-red.txt` | **改坏必红①**：真数据 `quest_scenario.json` 一条真锚 `src/OpenMir2/Messages.cs:62`→`1/2/Messages.cs:62` ⇒ `check` RED 点名「锚路径前缀」；还原后 sha256 逐字节一致（`1a863f9a…`） |
| `22-tool-neutered-mutation-red.txt` | **改坏必红②**：`_path_prefix_findings` 首行插 `return []` ⇒ `mutation` RED 且 `MUT-O1` 显示 GREEN（牙可失败向）；以副本还原，sha256 回到修复版 `a09cd9c3…` |
| `30-chain-gate-require-all.txt` | `fixture_chain_gate.py check --require-all` = **FIXTURE_CHAIN GREEN runner_ok=10 prereq_or_skip=0 red=0** |
| `32-chain-gate-selftest.txt` | 链深检门 `selftest` = **37/37** |
| `31-fixture-matrix-require-all.txt` | `fixture_matrix.py check --require-all` = **ready=10 pending=0 red=0** |
| `40/41/43/44` | 四门禁：`fmt` rc=0、`clippy -D warnings` rc=0、`test` **101 suites / 1200 passed / 0 failed / 22 ignored**、`msgcodegen --check` 604 rc=0 |

## 口径

- **只读**：`check`/`selftest`/`mutation` 只读仓库文件 + 自身 selftest，不写仓库文件、不起栈、不占端口；
  唯一写盘是 `21` 的真场景变异（**当次还原并复核 sha256**）与 /tmp 临时 `--out`。
- **真身未被放宽**：`check` 在改后仍 GREEN（14/22/31/27/19 锚），覆盖面/读侧 token/token 强度/工具侧名字锚**均未改**；
  本片只把「错前缀」判据从 fail-open 收紧到 fail-closed（新增 `1/2/` 向）。
- **tier = L**：纯离线夹具/工具（`*.rs=0`、冻结面空；不触 RNG 抽数序列/时序/落库/战斗数值）
  ⇒ 等价即放行 + 抽样冒烟（本片判据全离线静态机判 + 真数据改坏必红）。

## 残留

- 卡级真机 DoD（各 READY 夹具 `run_*.ps1` 两侧同序列 `diff=0`）仍待世界侧实现卡 +
  `openmir2-m3-clock-injection` + owner 独占 `-Go` 窗口；本片不据此收卡。
- reviewer-6 **O-2**（扩展名由 `PATHS` 推导）按上文如实保留，未在本片混片。
