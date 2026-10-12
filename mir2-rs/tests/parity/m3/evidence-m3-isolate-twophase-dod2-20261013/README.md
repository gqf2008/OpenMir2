# 证据包 — M3②③ 隔离口径真机两相复跑：DoD② 逐 k = 0（2026-10-13）

- 卡：`openmir2-m3-combat-skills-align`（issue `a2ef74aa`）
- 基准树：`master 5221f98a`（含 m3cs stream_equiv `f0e5bd5a` + m3bag 观测口径收口 `f906ac98`，即 owner 2026-10-13 02:15 要求的「先修仪器」片）
- 写集：`mir2-rs/tests/parity/m3/**`；**零 `.rs`**；本片**起真栈、占端口**（已按纪律实拍九端口 + `-Stop` + 十端口实拍）
- 种子/口径：`MIR2_RNG_SEED=42` / `MIR2_RNG_ISOLATE_ATTACK=1`（`attack_seed=40545`）/ `0137` /
  attacker `(17,20,dir=2)` / victim `(18,20) L255 Hp20669`（F-F pin）/ `-AttackPinPair -IsolateAttack`

## 结论

### ✅ DoD②（攻击逐 k）达成：`attack_diff.py diff ... = 0`

三对逐 k 全 GREEN（命令 + 原始件见下）：

| 对 | 口径 | 结果 |
|---|---|---|
| c1 vs c2（C# 同侧，两次独立起栈） | server-state（权威） | **GREEN 0**（attacked=120 hits=57 dodges=63 sum=2425） |
| c2 vs r1（跨侧，判据本体） | server-state（权威） | **GREEN 0**（同上） |
| c1 vs r1（跨侧） | server-state（权威） | **GREEN 0**（同上） |

- **两侧纯度门 GREEN**：C# 专用流 `player=120 == expected, base=0`（`attack_stream_gate`，F-N1）；
  两侧目标可用性门 `120/120`（受害在场且 `!ghost && !death`，`server_state_gate`）。
- **C# 专用流跨栈逐字节可复现**：c1 与 c2 的 `rng-attack-stream.tsv` `cmp` 无输出，
  sha256 均为 `0c7f44aa1ff06c3f…`（`streams/36-csharp-c1-c2-stream-identity.txt`）。
- **两侧攻击路径取数逐次等价**：`stream_equiv.py gate` **GREEN 651/651**（`(canon,归一 kind,arg,result)`）；
  `per_strike_requests.py analyze` GREEN（第一处分歧 `index=None`）。
- `jointable`（bot 命中/躲闪 × 服务端 RNG 推出的命中）**GREEN 0 mismatch**。

> 与上一片（F-N1 后两相 `evidence-m3-isolate-twophase-fn1-20261013`）同口径同判定轴：当时
> 旧 `align`（client-smstruck）三对 RED 72/102/121，现由 m3bag 的**权威口径**（`SERVER_STATE`
> 派生逐 k）判 0。本片是**真机复跑**（非离线复算），是 owner 02:15 要求的收口读数。

### ⚠️ 残留（非 DoD② 轴，一次新暴露）：`scoped_diff.py`（DB 侧作用域）RED

`run_attack_pair.ps1` 在攻击逐 k GREEN 之后还会跑 `scoped_diff.py diff --canonical --fixed-newchr`
（DB 侧作用域字段级）⇒ 本片该步 **RED：作用域内 8 行不一致**，故整脚本 rc=1。逐列定位：

| 表 | 差异列（csharp → rust） | 判读 |
|---|---|---|
| `mir2_db.characters` | `Hair` 2→1；`Id`/`CREATEDATE`/`LASTUPDATE`（易变） | `Hair` = 夹具 BotSrv `Random(1)`（**无种子**，`--fixed-newchr` 下不置占位）⇒ **夹具输入差**，非实现差（`--canonical` 单开关下 `characters` 即一致，见 `scope/51-*`） |
| `mir2_db.characters_ablity` | `Ac` 0→255、`MAxMP` 15→904、`MaxWeight` 0→21725、`MaxWearWeight` 0→255、`MaxHandWeight` 0→150、行1 `Hp` 20669→18244、`ModifyTime` NULL→ts | **在 `--canonical`（掩掉 Hair/Sex/Job）下仍在** ⇒ 非随机输入artifact，需 owner 归口 |

**观察线索（供定位，不作结论）**：C# 两行的 `LASTUPDATE == CREATEDATE` 且 `ModifyTime=NULL`，
即 **C# 侧本 run 未见断线存档更新**；Rust 两行 `LASTUPDATE/ModifyTime` 被刷新 ⇒ 差异至少部分是
「谁在 run 内写了角色存档」。该轴属**持久化/断线存档**（M2 / `openmir2-m3-gamesvr-save` 一带），
**不是** ②③ 的攻击 RNG 路径（后者已由 651/651 取数等价 + 逐 k 0 证明）。⇒ **本片只登记、不修**
（落点若属 `bins/game-svr`/`crates/world` 会撞他线写集），请 owner 定归属（很可能转 m2-svr）。

## 与 DoD② 判据的关系（口径诚实说明）

- owner 2026-10-13 02:15 明写：**②③ 的 DoD② = 活栈隔离口径下 `attack_diff.py diff` 逐 k = 0**。
  本片该轴 **0**，故写「DoD①② 达成（命令 + 原始件 + diff=0 + 两侧纯度 GREEN + 651/651 取数等价）」。
- `scoped_diff` 是夹具既有的**第二条独立门**（DB 侧持久化），**不是** DoD② 的判据；本片如实登记其 RED
  及其逐列定位，**不**据此宣布整脚本 GREEN（整脚本 rc=1 就是这条门给的）。
- **判据一个字未放宽**：`attack_diff.py diff` 的逐项 `hit/dmg/hp/maxhp` 比较与非平凡门（`attacked≥1 /
  hits≥1 / sum>0`）一字未动。

## 工件索引

| 路径 | 内容 |
|---|---|
| `window/00-selfcheck-nostack.txt` | 夹具无栈自检（SELFTEST_RED GREEN） |
| `window/01-ports-before.txt` / `23-ports-after.txt` | 起栈前/收工后十端口实拍（均只有共享 3306） |
| `window/10..16-*csharp*` | C# 栈起停 + 20 冒烟 + 120 c1/c2（含纯度门/目标可用性门/align 行） |
| `window/17..22-*rust*` | Rust 栈起停 + 20 冒烟 + 120 r1（同种子注入行 + 同门） |
| `streams/csharp-c{1,2}-attack-stream.tsv` | C# 专用攻击流（两栈逐字节相同，sha `0c7f44aa…`） |
| `streams/csharp-c2-server-state.tsv` / `rust-r1-server-state.tsv` | 两侧服务端权威逐击状态 dump（同形） |
| `streams/rust-r1-rng-stream.tsv` | Rust 逐次 rng 流水 |
| `diffs/31/32/33-*.txt` | 三对逐 k diff（全 GREEN 0） |
| `requests/40-analyze.txt` | 两侧取数请求序列分析（GREEN；`index=None`） |
| `requests/42-jointable.txt` | bot 命中/躲闪 × 服务端 RNG 命中（GREEN 0 mismatch） |
| `requests/43-stream-equiv.txt` | 取数流语义等价门（GREEN 651/651） |
| `requests/44-stream-equiv-selftest.txt` | `stream_equiv.py selftest` 10/10 |
| `scope/50/51/54-*` | 作用域 RED 的三方视角 + 逐列定位（本片登记的残留） |
| `sessions/<sid>/` | 三个会话的 `attack_raw/seq/state.jsonl`、`server_state_segment.tsv`、`scoped_*_norm.jsonl` 等原始件 |

## 复现（真机，需窗口）

见 `commands.txt`。要点：① 夹具无栈自检；② 九端口实拍；③ C# 栈（`-Seed 42 -IsolateAttack`）→
20 冒烟 → 每次 120 **新起栈**（hook 以 `FileMode.Create` 截断专用流）→ 存 c1/c2 专用流与 dump；
④ `-Stop` C# → Rust 栈（同种子 + `-RngLog` 环境变量 + `-StateLog`）→ 20 冒烟 → 120 `-PeerSession <c2>`；
⑤ `attack_diff.py diff`（三对）；⑥ `scoped_diff.py`（登记 RED）。

## 残留物清单

- `E:/tmp/m3cs-fin/**`（本片工作目录：csharp/rust 栈日志、会话、`logs/` 转录；**未入库**；窗口已释放）。
- `mir2-rs/tests/parity/m3/sessions/{csharp-20261013-024050,csharp-20261013-024249,rust-20261013-024604}/`
  运行时目录（未跟踪；已把所需原始件复制进本包 `sessions/`，运行时目录**保留**供复核，未删）。
- `refs/remotes/walgit-m3bag`（本片只读取用的 m3bag 分支 ref，未改远端）。
