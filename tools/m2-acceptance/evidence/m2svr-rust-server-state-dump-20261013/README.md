# M3②③ 夹具：Rust 侧**服务端权威逐击状态 dump**（`SERVER_STATE`）——m2-svr 半

> 卡归属：`bins/game-svr/**`（m2-svr 写集）。owner 依据：`brief-m3-combat-skills.md` 顶部块第 2 条
> （「补 Rust 侧的同形『服务端状态 dump』…需要 `bins/game-svr` 就报 owner 转 m2-svr」）+ owner
> `handover-to-pi.md` §5.254（「同时补 Rust 侧同形 dump，让这道门两侧都强制」）；夹具
> `mir2-rs/tests/parity/m3/run_attack_pair.ps1:1164` 亦明写「-Side rust 无服务端状态 dump（**归 m2-svr**）」。
> 本片**不 merge、不自签 review**。

## 1. 为什么需要它

C# 侧「目标格 `(18,20)` 从 `k=4` 起为空」**不是格子空了**——C# 影子 hook 的**服务端** dump
（`rng-hook/StartupHook.cs::DumpActors`）证明受害者一直站在 `(18,20)`、map `0137`，只是它的 `Ghost`
标志被某条 oracle 路径静默置 1；`Envir.GetMovingObject(x,y,true)` 对 `Ghost` 直接返回 null ⇒
`GetPoseCreate()` 为 null ⇒ `_Attack`/`AttackSuccess` 提前返回。**客户端视图（`attack_state.jsonl`）
看不到这个 ghost** ⇒ 目标可用性门必须用**服务端权威状态**判，否则会拿「假的有目标窗口」去比。

`server_state_gate.py`（夹具侧的 fail-closed 门，master `0f4e141b` 已合入）只认 C# 侧 dump ⇒
**Rust 侧缺这一段，这道门只保护一半**。本片补上 Rust 半。

## 2. 口径（与 `StartupHook.cs::DumpActors` 同形）

| 维度 | 口径 |
| --- | --- |
| 触发点 | 每个**玩家攻击单元**（C# = 玩家 `GetAttackPower@PlayObject.cs:2929` 首抽；Rust = `handle_hit_cmd` 判定「本击进入攻击路径/抽攻击力」的那一点），**与玩家单元同序** |
| 内容 | 权威世界 `world.entities` 里全部**玩家族**（`race == 0` = `ActorRace.Play`）对象的 `id/name/x/y/dir/ghost/death/hp/maxhp/map` |
| 格式 | 一行 `SERVER_STATE <seq> <json 数组>`，`seq` 进程内单调；JSON 对象键序与 C# 一致 |
| 开关 | 环境变量 `MIR2_RNG_STATE_LOG`（与 C# `start-csharp-hook.ps1 -StateLog` 同名）；**未设/空 ⇒ 完全不写**（不建文件、不取数、不打印）⇒ 生产路径与改动前**逐字节相同** |

**登记（与 C# 的微差，均被 `server_state_gate.py` 的 `line.strip()` 归一，不影响判据）**：
① 行结束：C# `WriteLine` 在 Windows 写 `\r\n`，本实现写 `\n`；② `name`/`map`：C# 不转义、本实现做最小
JSON 转义（真实角色名/地图名无这些字符 ⇒ 逐字节同形，且避免名字带引号时把整行打成非法 JSON）；
③ `dir`：C# dump 在 `Dir = nDir`（`BaseObject.Attack.cs:12`）**之前**（旧朝向），本实现在本击处理返回后
dump（`DisplayTable` 已按本帧 `dir` 更新）；gate 不读 `dir`。

## 3. 实现点

- 新模块 `mir2-rs/bins/game-svr/src/state_log.rs`（`StateLog::{disabled,from_env,from_path,enabled,record}`）。
- `handle_hit_cmd` 返回 `bool`（`true` = 本击进入攻击路径；被守卫拒的支 `false`）——**零签名调用点改动**。
- `sim_loop`：`let mut state_log = StateLog::from_env();` + `if attacked { state_log.record(&world, &displays, registry.as_ref()); }`。
- 只在设了开关时打一行启动日志（未设 ⇒ 一行不多）。

## 4. 判据与读数（改前 / 改后）

**判据 ①（真 gate 直接消费 Rust 产出）**：`gate-on-rust-output.txt` = 真 `server_state_gate.py`
（master 版本）对 **Rust 真实产出**（`rust-server-state.tsv`）判 **GREEN rc=0**
（「5 击里 5 击受害在场且 !ghost && !death」）；`gate-selftest.txt` = gate 自带 selftest **6/6**。

**改前**：Rust 侧 `grep -rn 'SERVER_STATE\|MIR2_RNG_STATE_LOG' mir2-rs/bins/game-svr mir2-rs/crates` = **0 命中**
⇒ `-Side rust` 分支只打 NOTE、**目标可用性门只在 C# 侧强制**（`run_attack_pair.ps1:1164`）。

**改后**：玩家攻击单元逐行落 `SERVER_STATE`（真实产出见 `rust-server-state.tsv`，含 attacker `(17,20)`
与 victim `(18,20)`，怪物**不在**其中）。

### 改坏必红（三条，原始输出入库）

| # | 改坏 | 期望 | 证据 |
| --- | --- | --- | --- |
| 1 | 删掉 `race != ACTOR_RACE_PLAY` 过滤（怪物也落） | 用例 FAILED（dump 出现 `mon`） | `redcheck-monster-not-filtered.txt` |
| 2 | `handle_hit_cmd` 恒返回 `false`（撤掉状态 dump 的桥） | 用例 FAILED（「进入攻击路径的一击必须返回 true」） | `redcheck-return-false.txt` |
| 3 | 对 **Rust 产出**做「受害从 `k=4` 起 `ghost=1`」注入 ⇒ 真 gate | **RED rc=1** 且点名 `#4(seq=4), #5(seq=5)` | `redcheck-gate-ghost-injection.txt` |

## 5. 四门禁（隔离 target `E:/tmp/openmir2-m2svr-target`，base = master `0f4e141b`）

见 `gates.txt`：fmt rc=0 / clippy `--workspace --all-targets -D warnings` rc=0 /
`cargo test --workspace --offline` **84 suites / 686 passed / 0 failed / 16 ignored** /
`msgcodegen --check` **604** 条一致。

## 6. 边界与依赖

- **写集**：仅 `mir2-rs/bins/game-svr/src/{state_log.rs(新),main.rs}` + 本证据目录；`src/`（C# oracle）、
  `sql/`、`MirClient`、`crates/world/**`（m3cs）、`mir2-rs/tests/parity/m3/**`（夹具）**零改动**。
- **未做的对侧（依赖登记，不是本片缺陷）**：夹具 `run_attack_pair.ps1` 目前只在 `-Side csharp` 分支
  调 `server_state_gate.py`（`:1137-1165`），`-Side rust` 分支仍是 NOTE（`:1164`）。要真正「两侧都强制」，
  需夹具侧把 `-Side rust` 也接到同一个 gate（一行分支，属夹具写集）——本片把**Rust 产出**备好，
  夹具侧接线后即可直接判。

## 7. 残留物清单

- 本证据目录（入库）。
- `E:/tmp/openmir2-m2svr-target`（隔离 cargo target，既有，非本片专属）。
- 本片**未起栈、未占端口**（落片前实拍：仅共享 `3306` LISTENING；5000/7000/7100/7200 全空）。
