# 证据包：markers 诊断前缀收口（defect_2）——`M3*SEQ ... skip=` → `M3*SEQ_SKIP`

- **卡**：`openmir2-m3-fixture-marker-prefix-bug`（issue 见线程；本片 = 该卡剩余 DoD ② 的 defect_2）
- **车道**：principal `openmir2-worker-5` ／ worktree `.claude/worktrees/m3cs` ／ branch `worktree-m3-marker-skip-prefix`
- **base**：`origin/master 9c776129c`（= ⑧-10 `cf8bbf825` 合入后的 master）
- **写集**：`mir2-rs/tests/parity/m3/marker_prefix_gate.py` + `src/BotSrv/Player/RobotPlayer.cs`（T-41/42/45/46 夹具例外）+ 本证据目录；**零 `.rs`**
- **零起栈、零端口、不 merge、不自签**

## 1. 这条 defect 是什么（为什么必须先收）

window 车道第 2 轮真机实测（`openmir2-svc-1` comment `cf333434`）：前缀口径修好后，BotSrv 仍在**事件前缀
`M3*SEQ `**下混入**非 JSON 诊断行**（`M3SHOPSEQ op=sell skip=no_npc_in_view`、`M3QUESTSEQ skip=... step=...`）。
`run_pair.ps1` 的 markers 抽取正则是 `M3*SEQ [^\r\n]*` ⇒ 这些诊断行被当**事件**收进 `*_markers.jsonl`；
比较器 `_marker_events` 一律 `json.loads` ⇒ `Prereq("事件非法 JSON")`。即：**⑤商店/⑥任务等已实现系统在
「NPC 不在视野」这类前置失败时会以『夹具解析崩』而不是『干净的 PREREQ』收场**，⑤⑥ 的真机判决被夹具阻断。

修法（owner 在 `cf333434` 给的两选一）：**写侧换独立前缀**（本片采用）——诊断行一律写 `M3*SEQ_SKIP ...`，
比较器的 `line.find("M3*SEQ ")` 天然不匹配（下划线），无需放宽任何读侧判据；**fail-closed 保持**：
真·非法 JSON 混进事件流仍会被比较器以 `Prereq`（rc=2）拒比。

## 2. 改了什么

**(a) BotSrv 16 处 `skip=` 诊断全部改 `M3*SEQ_SKIP`**（覆盖 7 个序列族，含 ⑧-10 合入后的 guildcastle）：

| 族 | 处数 | 行（改前 `M3*SEQ ` → 改后 `M3*SEQ_SKIP `） |
| --- | --- | --- |
| `M3BAGSEQ` | 4 | 563/581/598/610 |
| `M3SHOPSEQ` | 1 | 883 |
| `M3QUESTSEQ` | 2 | 1202/1214 |
| `M3PAIRTRADESEQ` | 3 | 1484/1491/1511 |
| `M3SELLOFFSEQ` | 2 | 1740/1777 |
| `M3GUILDCSEQ` | 2 | 1982/1996 |
| `M3HEROPETSEQ` | 2 | 2223/2245 |

改后：`rg 'M3[A-Z]*SEQ_SKIP' src/BotSrv/Player/RobotPlayer.cs` = **16**，`rg 'MainOutMessage\("M3[A-Z]*SEQ [^"]*skip='` = **0**（见 `11-botsrv-skip-sites.txt`）。

**(b) `marker_prefix_gate.py` 增第 5 条判据（写侧 fail-closed 扫描）+ 运行时牙 + 改坏必红**：
- `SKIP_FAMILIES`（7 族）里不得再出现 `MainOutMessage("M3*SEQ ... skip=...")` 的裸前缀诊断；
- 运行时牙：带 `_SKIP` 前缀的诊断行**不干扰** `assemble`（rc=0）；裸前缀诊断行仍被当事件 ⇒ **rc=2**（fail-closed 未放宽）；
- 改坏必红：变异 `RobotPlayer_bareskip.cs`（把一条已迁诊断改回裸前缀）⇒ 本门 **RED**（`04-gate-mutation-red.txt`）；
- 约定写入 docstring：新增夹具族须把族名加进 `SKIP_FAMILIES` 并同步转换 `skip=` 诊断。

## 3. 判据读数（本片实跑；命令见 `00-commands.txt`）

- `marker_prefix_gate.py check` ⇒ **GREEN**（无去前缀截断 + 6 生产牙 + 6 比较器按前缀扫描 + BotSrv 已迁 7 族无裸前缀 skip；teeth bare=2 prefixed=0 empty=2 skip_prefixed=0 skip_bare=2）
- `marker_prefix_gate.py selftest` ⇒ **ok=24/24**
- `marker_prefix_gate.py mutation` ⇒ **GREEN**（strip / noguard / **bareskip** 三向皆 RED）
- 比较器 selftest：`npc_shop_diff 32/32` / `quest_diff 36/36` / `party_trade_diff 27/27` / `selloff_diff 33/33` / `hero_pet_diff 36/36` / `guild_castle_diff 36/36`
- 8 个 runner `-SelfCheckNoStack` 全 **GREEN**（bag/attack/npc_shop/quest/party_trade/selloff/hero_pet/guild_castle）
- 四门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target`）：`fmt rc=0` / `clippy -D warnings rc=0` / workspace **99 suites 1159 passed 0 failed 22 ignored** / `msgcodegen 604 rc=0`

> **『改前』对照**：pristine `origin/master 9c776129c` 上 `marker_prefix_gate.py check` 已 GREEN（⑧-10 合入时
> 顺带修了 ⑨ hero-pet 的同一去前缀缺陷，见其 O-2），但**本门当时没有第 5 条判据**、`skip=` 裸前缀诊断 16 处仍在
> ⇒ 本片补的正是「**把这条约定机判住**」。见 `01-before-pristine-master.txt`。
> （更早的 `72adb2a2e` 上本门确曾 RED：`run_pair.ps1:758` 的 ⑨ hero-pet 去前缀截断——由 ⑧-10 合入时修掉。）

## 4. 文件索引

| 文件 | 内容 |
| --- | --- |
| `01-before-pristine-master.txt` | 『改前』pristine master 9c776129c：门 GREEN（无第 5 条）+ 16 处裸前缀诊断 |
| `01b-earlier-master-red-72adb2a2e.txt` | 更早的 72adb2a2e：门 RED（⑨ hero-pet 去前缀截断，⑧-10 合入时修掉） |
| `02-gate-check-green.txt` / `03-gate-selftest.txt` / `04-gate-mutation-red.txt` | 门 check / selftest / 改坏必红 |
| `05-comparator-selftest-*.txt` | 6 个比较器自检 |
| `10-runner-selfchecks.txt` | 8 个 runner 的无栈自检 |
| `11-botsrv-skip-sites.txt` | BotSrv 16 处 `_SKIP` + 0 处裸前缀 |
| `12-boundary.txt` | 变更边界（含冻结面核查） |
| `13-gate-fmt.txt` / `14-gate-clippy.txt` / `15-gate-test.txt` / `16-gate-msgcodegen.txt` | 四门禁原始输出 |
| `SHA256SUMS.txt` | 仓库字节（LF 归一 = git blob）口径 |

## 5. 复现

```sh
cd mir2-rs/tests/parity/m3
python -I -B marker_prefix_gate.py check
python -I -B marker_prefix_gate.py selftest
python -I -B marker_prefix_gate.py mutation
python -I tools/evidence-hash/verify_sha256sums.py verify <本目录>              # 仓库字节口径
python -I tools/evidence-hash/verify_sha256sums.py eolcheck <本目录>            # EOL 口径
```

## 6. 残留 / 后续（如实登记，未做）

- **真机两侧 diff=0**（本卡的 DoD①）仍需**独占窗口**跑 `run_npc_shop_pair.ps1` / `run_quest_pair.ps1`（以及 ⑦⑧⑨⑩ 各族），
  本片**未起栈、未占端口**。
- **defect_3**（C# 侧任务 `phase=frame` 事件 JSON 截断 `{=2/}=1`，window r2 实测）与 **`quest_state.jsonl` 无生产者**：
  本片**未修**（前者缺原始件不可离线复现，后者在服务端会话 dump 面、可能落 `bins/game-svr`）。按 `cf333434` 仍归 owner 转派。
- 本片**未放宽任何读侧判据**：裸前缀/非法 JSON 事件仍 `Prereq`（rc=2）。
