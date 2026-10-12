# M3②③ 攻击夹具 · R-1「零 RNG 消费者」场景隔离证据包（2026-10-12，m3bag / worker-4）

卡：`openmir2-m3-attack-pair-fixture`（issue `76cc08b8`）。
来源：owner 2026-10-12 14:55 brief（`E:/tmp/openmir2-watch/brief-m3-bag-equip.md` 顶部）：
「为 R-1 的『零 RNG 消费者』场景做夹具支持（`-PinMapName` 透传 + 无怪图判定泛化）」。
**零 `.rs`**；改 `src/BotSrv/**` = 0；写集 = `mir2-rs/tests/parity/m3/{gate_guards,run_pair,run_attack_pair}.ps1`
+ 新 fixtures（`extract_mongen.py` / `mongen-map0137.tsv` / `mongen-provenance.tsv`）+ 本目录 + `README.md`。

## 1. 背景与判据（R-1）

owner 裁决：同种子「逐 k 完全一致」当前不成立，因为攻击判定取全局 `M2Share.RandomNumber`，
而该流被**全图怪 AI**（`AnimalObject.Wondering` / `MonGetRandomItems` 等）持续消耗。解法 = 让对拍场景
**除攻击路径外零 RNG 消费者**，最简 = **整张图零刷怪**。可判据：① 同侧同种子连跑两次 ⇒ (a) 汇总一致 **且**
(b) 逐 k 完全一致；② 再两侧同配置 ⇒ 逐 k 差异才可归因到实现。**判据不放宽**：`attack_diff.py diff` 仍求 0。

本片只做**夹具支持**（不起真栈、不跑正式判据——正式跑由 `m3cs` 执行）。

**第二轮补强（owner 2026-10-12 15:05 refine）**：MonGen 之外，守卫（`GuardList.txt`）、商人/NPC
（`MerChant.txt`/`Npcs.txt`）、地图事件（`MapEvent.txt`）、任务触发（`MapQuest.txt`）都可能跑 AI、
消耗全局 RNG ⇒ 除「MonGen 零行」外，另加**机器可判**口径 `Test-M3MapNoAiConsumers`（该图在这 5 个文件里
零引用）+ 逐源 SHA256 冻结，并给出可复现原始输出（`14`/`15` 两件）。

## 2. `-PinMapName` 透传（brief ①）

- `run_attack_pair.ps1` 新增 `-PinMapName`（缺省 = `gate_guards.ps1` 的 `$script:M3AttackPinMap`），
  并把 `-PinMapName` 同时传进 `-EmitOpsArgs` 预检与 `-AttackPinPair` 侧运行两条 `run_pair.ps1` 调用。
- `run_pair.ps1` 的 `-PinMapName` 改为「空 ⇒ `-AttackPinPair` 取场景常量，否则 `'0'`」；
  `-PinCX/-PinCY` 同样「`<0` ⇒ 攻击时取场景常量、否则 289/618」⇒ pin SQL 的 `MapName` 与角色落点口径**跟着地图走**，
  背包/装备（T-14）路径逐字节不变。两侧仍由 `ATTACK_POSE_OK` 保证 pose 逐字节相同（pose 串含 map）。

## 3. 无怪判定泛化（brief ②④）：按地图查提取件，判「整张零刷怪」

- 旧 `Test-M3AttackPinNoMonster`（几何净距 ≥50，只读 `fixtures/mongen-map0.tsv`）**已删除**——
  「离刷怪圈 99.2 格」**不等于**「全图无怪」（map 0 本身有 **53** 个刷怪点）。
- 新 `Test-M3MonGenEmpty`（`gate_guards.ps1`）：按目标地图查 `fixtures/mongen-map<map>.tsv` +
  `fixtures/mongen-provenance.tsv`，要求**该图刷怪行数 == 0**。四类 fail-closed：
  provenance 无该图 / 提取件缺失 / 提取件行数 ≠ 声明 / （`-VerifySource`）源文件缺失·hash 漂移·现数与提取件不符。
- `run_pair.ps1 -AttackPinPair` 启动时 **fail-closed**（不是无怪图就 `throw`，不等真机跑出随机红）；
  成功打 `MAP_HAS_NO_MONSTER ok map=0137 spawns=0 ...`。
- `-SelfCheckNoStack` **⑯** 四向：生产 map 绿 + `source_verified=True` / **换回 map 0 必红** /
  合成「0 行=绿、1 行=红、行数≠声明=红、缺 provenance=红」。

### 选图（brief ④）：`0137`（竞技场）

内容冻结（未新增/改任何数据文件）。选择依据（离线只读）：

| 项 | 值 |
|---|---|
| MapInfo | `[0137\t竞技场]\tFIGHT`（允许 PvP；无 SAFE） |
| map 文件 | `E:/MirServer/Mir200/Map/0137.map`（11932 B，30×33，`SHA256 af987e96cd2b15a5eb3b6297a0465f031a3df63b21e50106d23328d6263c8b33`） |
| MonGen | 在 `Envir/MonGen.txt` 中**零行**（整张零刷怪） |
| NPC/守卫/任务 | `MerChant.txt`/`GuardList.txt`/`MapQuest.txt` 及 Envir 其它文件均**无引用**（无跑 AI 的实体；原始输出见 `14-map0137-no-ai-consumers.txt`） |
| 非 MonGen AI 消费者 | GuardList/MerChant/Npcs/MapEvent/MapQuest 对 0137 **逐文件引用行数全为 0**（`ai-consumer-map0137.tsv`，逐源 hash 见 `ai-consumer-provenance.tsv`） |
| 落点 | attacker `(17,20) dir=2` / victim `(18,20)`，两格均 walkable，7×7 邻域 43 格可走 |

存在其它同族无怪候选（`0110/0102/0100/0137/0138/...` 等 36+ 张，见 `10-map0137-not-in-mongen.txt`）；
选 `0137` 因其明确是**竞技场（FIGHT）**且无任何 NPC/守卫。

### 冻结算据（brief ③）

- 源文件：`E:/MirServer/Mir200/Envir/MonGen.txt`
  `SHA256 = e5c39fb7eda66b70f82a61a5c2cea25df9cf0b6892b916fb6d579f88ff43748f`
- 提取命令：`python mir2-rs/tests/parity/m3/fixtures/extract_mongen.py <MonGen.txt> <map>`
  （stdout 为 LF 字节；`x<TAB>y<TAB>range`，range = 第 6 列）。
- 提取件：`fixtures/mongen-map0137.tsv`（**0 数据行**）、`fixtures/mongen-map0.tsv`（53 行，既有）；
  行数与源 SHA256 记进 `fixtures/mongen-provenance.tsv`。
- 复核：`12-provenance-fixtures.txt` —— 提取器对 map 0 的输出与既有 fixture 数据行**逐字节相同**；
  对 map 0137 输出为空。`10-map0137-not-in-mongen.txt` 给出「0137 不在 MonGen 的 96 张图中」。
- **为什么这张图无怪**：MonGen 是刷怪点表的唯一来源，`0137` 在该表零行，且无 NPC/守卫/任务触发实体。

### 「非 MonGen AI 消费者」机器可判（owner 2026-10-12 15:05 refine）

「MonGen 零行」只排除**刷怪点**；守卫/商人/NPC/地图事件/任务触发器仍可能生成跑 AI 的实体。⇒ 新增：

- 提取器：`fixtures/extract_map_ai_consumers.py <EnvirDir> <map>`（按整词/整字段匹配，非子串；stdout = `file<TAB>ref_rows`）。
- 冻结件：`fixtures/ai-consumer-map0137.tsv`（5 个受检文件逐文件 0 行）+ `fixtures/ai-consumer-provenance.tsv`
  （源目录 `E:/MirServer/Mir200/Envir` + 逐源 SHA256 + `declared_total=0`）。
- 机判：`gate_guards.ps1::Test-M3MapNoAiConsumers` 要求全为 0；`run_pair.ps1 -AttackPinPair` 启动时**同样
  fail-closed**（`MAP_HAS_NO_AI_CONSUMERS ok`；`-VerifySource` 复算逐源 SHA256 与现数）。
- 反向对照：map 0 有引用（GuardList 22 / MerChant 131 / Npcs 3 / MapQuest 1）⇒ 判据不是恒绿。
- 原始输出：`14-map0137-no-ai-consumers.txt`（逐源 hash + 提取器输出 + 逐字节对账 + 全 Envir 扫描）；
  改坏必红：`15-mutation-ai-consumers-red.txt`。

## 4. 自检与牙齿（brief ⑤）

| 件 | 命令 | 结果 |
|---|---|---|
| `01-selfcheck-r1-green.txt` | `run_attack_pair.ps1 -SelfCheckNoStack` | **rc=0**；⑥ pose=`map=0137 (17,20)/(18,20)`；⑯ 生产 `spawns=0 source_verified=True`、map0 `spawns=53` 必红、合成四向有牙 |
| `02-mutation-map0-red.txt` | 把 `$script:M3AttackPinMap` 改回 `'0'` 后同上 | **rc=1**；`emit_pair_rc=non-zero, attack_pin_map_not_monsterless=0:该图有 53 个刷怪点, attack_pin_map_spawns=53, attack_pin_map_source_recheck_red` |
| `03-mutation-judgment-red.txt` | 把 `$ok = ($entry.spawn_rows -eq 0)` 改成 `$ok = $true` 后同上 | **rc=1**；`attack_pin_map0_not_red, synth_nonempty_not_red` |
| `04-selftest-gate-47of47.txt` | `selftest_gate.ps1` | `SUMMARY total=47 failed=0` GREEN |
| `05-bagpair-selfcheck-green.txt` | `run_bag_equip_pair.ps1 -SelfCheckNoStack` | rc=0（pin settle+hold / pin SQL 列校验两向 / 旧 peer 必红仍在） |
| `11/12/10` | 见 §3 | 离线只读证明 |
| `14-map0137-no-ai-consumers.txt` | 提取器 + `sha256sum` + `grep -rlI` | 0137 逐文件 0；map 0 非 0；全 Envir 仅 MapInfo 引用 |
| `15-mutation-ai-consumers-red.txt` | 把 `$ok=($entry.declared_total -eq 0)` 改成 `$ok=$true` 后 `-SelfCheckNoStack` | **rc=1**；`RED: synth_ai_ref_not_red` |
| `01/04/05`（本轮重跑） | `-SelfCheckNoStack` / `selftest_gate.ps1` / `bagpair -SelfCheckNoStack` | rc=0 / 47-0 / rc=0（含新门 **⑰**） |

两处 mutation 均已**逐字节还原**（`gate_guards.ps1`/`run_attack_pair.ps1`/`run_pair.ps1` 与 green 备份 `diff` 为空）。

## 5. 四门禁（本片零 `.rs`；隔离 `CARGO_TARGET_DIR=E:/tmp/m3bag-fij-target`，在 `mir2-rs/` 下跑）

> 2026-10-12 15:37 在**合并 master `fd6a02e1` 后的 head**上重跑（`06`–`09` + `04`/`13` 同步刷新）：
> 本片零 `.rs` ⇒ 门禁结果只随 master 新增用例变化（`648 → 654 passed`）。

| 件 | 命令 | 结果 |
|---|---|---|
| `06-gate-fmt.txt` | `cargo fmt --all -- --check` | rc=0 |
| `07-gate-clippy.txt` | `cargo clippy --workspace --all-targets --offline -- -D warnings` | rc=0 |
| `08-gate-test.txt` | `cargo test --workspace --offline` | **79 suites / 654 passed / 0 failed / 16 ignored**，rc=0 |
| `09-gate-msgcodegen.txt` | `cargo run -q -p msgcodegen --offline -- --check` | 604 条一致，rc=0 |

## 6. 复现命令

```powershell
# 无栈自检（本片核心；rc=0）
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_attack_pair.ps1 -SelfCheckNoStack
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/selftest_gate.ps1            # 47/0
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_bag_equip_pair.ps1 -SelfCheckNoStack
# 单点复核（离线；map 0137 应 ok、map 0 应 red）
powershell -NoProfile -Command ". mir2-rs/tests/parity/m3/gate_guards.ps1; Test-M3MonGenEmpty -MapName 0137 -VerifySource; Test-M3MonGenEmpty -MapName 0"
# 单点复核（非 MonGen AI 消费者；0137 应 ok + source_verified；map 0 无冻结件 ⇒ fail-closed red）
powershell -NoProfile -Command ". mir2-rs/tests/parity/m3/gate_guards.ps1; Test-M3MapNoAiConsumers -MapName 0137 -VerifySource; Test-M3MapNoAiConsumers -MapName 0"
# 真机正式跑（由 m3cs 执行；本片不起栈）：两侧同种子 + -AttackPinPair（缺省 map=0137）
#   run_attack_pair.ps1 -Side csharp -Bots 2 -AttackCount 120 -AttackPinPair -Prefix <p>
#   换 Rust 栈：-Side rust ... -PeerSession <csharp 会话>
python mir2-rs/tests/parity/m3/attack_diff.py align <各侧 raw> --out <seq>; python mir2-rs/tests/parity/m3/attack_diff.py diff <cs> <rs>
```

## 7. 残留物清单

- 入库：本目录 + `fixtures/{extract_mongen.py,mongen-map0137.tsv,mongen-provenance.tsv,extract_map_ai_consumers.py,ai-consumer-map0137.tsv,ai-consumer-provenance.tsv}`
  + `gate_guards.ps1` / `run_pair.ps1` / `run_attack_pair.ps1` / `m3/README.md`。
- 仓外临时物（可删）：`E:/tmp/m3bag-r1/**`（green/mutation 原始输出 + 三份 green 备份 + gates 日志）。
- 端口/进程：本车道**未起也未停**任何服务端栈；快照时十端口被**另一车道**的栈占用（`13-ports-snapshot.txt`，如实记录）。
- 源数据：`E:/MirServer/**` 只读引用，未改一行（内容冻结）。
