# 证据包 — M3②③ 活栈隔离口径 · F-N1 合入后两相实跑（2026-10-13，m3cs）

- 卡：`openmir2-m3-combat-skills-align`（issue `a2ef74aa`）
- 基准树：`master 5134c7ee`（含 F-N1 `1e7f2280`、Rust `SERVER_STATE` `f0e5cfdb`、Rust `STATE_LOG` `02ae0ba4`）
- 夹具：`run_attack_pair.ps1` / `attack_diff.py`（m3bag 写集）＋ m3cs `per_strike_requests.py` / 本包 `stream_equiv.py`
- 场景：`0137`（零刷怪/零 AI 消费者，R-1），`MIR2_RNG_SEED=42`，`MIR2_RNG_ISOLATE_ATTACK=1`（`attack_seed=40545`），
  受害者 pin `L255 / Hp=20669`，attacker `(17,20) dir=2`、victim `(18,20)`；C#/Rust 两侧都启用服务端权威 `SERVER_STATE` dump。

## 结论（三条，判据口径一个字未动）

1. **F-N1 真机生效（正）**：C# 专用流纯度门 GREEN —— `player=120 == expected`、`base=0`（20 冒烟同样 `player=20 base=0`）。
   即「按调用点共享把非玩家攻方吸进专用种子流」这一污染面已被按攻方线程收窄消除。
2. **C# 侧专用流跨栈**逐字节可复现（正）：两次**各自新起 C# 栈**的 120 击，
   `attack_stream_sha256` 均为 `0c7f44aa1ff06c3f…`（见 `streams/36-csharp-c1-c2-stream-identity.txt`）。
   ⇒ 隔离口径下「C# 自己不可复现」已不成立（至少攻击路径取数侧）。
3. **两侧攻击路径取数逐次等价（正）**：`stream_equiv.py gate` 判 **GREEN 651/651** ——
   两侧 `(canon, 归一 kind, arg, result)` 序列逐 index 全等。

但 **`attack_diff.py diff`（逐 k）仍未 0**：`c1 vs c2 = 72`、`c2 vs r1 = 102`、`c1 vs r1 = 121`（`diffs/30..32`）。
定位（见下 §定位）指向**测量/仪器**而非 RNG/实现：**DoD② 未达成、未放宽、未声明达成**，带证据请 owner 裁决。

## 定位：为什么「流级 651/651 等价」而「逐 k 仍非 0」

### (a) `per_strike_requests.py analyze` 的 index=3「分歧」是**仪器假阳性**
两侧各 651 抽、站点计数完全相等（`requests/40-analyze.txt`）。`analyze` 的 `first_divergence` 按
`Entry.req() = (kind, arg)` 比；Rust 侧对 `random_range(0, n)` 记 kind=`Next(min,max)`（packed arg 高 32 位 min=0），
C# 侧对等价的 `Random(n)` 记 kind=`Next(max)`。二者**语义相同**（都均匀取 [0,n)），实测同 index 的 repr 完全相同
（同为 `hit_struck_damage(37)->22`）。⇒ 该 index=3 判「player_path_divergence」是**标签口径**造成的假阳性。
`stream_equiv.py` 归一 `min==0` 后判 651/651 全等，且带「真 `min>0` 不得抹平」「改坏一抽结果必红」两条牙齿
（`requests/44-stream-equiv-selftest.txt`）。

### (b) C# 侧 `attack_seq` 命中数被**测量**抬高（>实际取数事件数）
C# 专用流里带伤害的单元 = **57**（`atk_power_a/b` 各 120、`hit_gate` 120、`attack_success` 120，
`hit_struck_damage`/`hit_struck_damage2`/`struck_durability` 各 **57**）。但 C# `attack_seq` 逐 k 统计：
`c1 hits=81`、`c2 hits=75`；Rust `r1 hits=57`（与其流的 57 一致）。`align` 侧记 `other_observer=57`、
`deduped=24(c2)/15(c1)`。`jointable` 亦独立报
`info:align_vs_rng_hit_mismatch side=csharp ks=[1,5,9,16,20,32,35,48,54,60,63,69,86,89,93,100,105,108,112,119]`
（20 个 k 上「SM_STRUCK 派生的 hit」与「专用流派生的 hit」不一致，**Rust 侧 0 条**）。
⇒ 逐 k 差异的**主要来源是 C# 侧 SM_STRUCK 观测/对齐**（观测者/去重/时序），不是两侧 RNG 或战斗实现。
（`attack_diff.py` / `align` 属 **m3bag 写集**，本包只读、只登记，不自行修改。）

## 工件索引

| 路径 | 内容 |
|---|---|
| `window/00-selfcheck-nostack.txt` | 合入 F-N1 后的无栈自检（含 22c 纯度门 / 22d -Repo 绑定），GREEN |
| `window/01-ports-before.txt` / `23-ports-after.txt` | 窗口起前/收后十端口实拍（均仅 3306） |
| `window/10..16-*.txt` | C# 起栈 / 20 冒烟 / 120 c1 / 新栈 / 120 c2 / 停栈 |
| `window/17..22-*.txt` | Rust 起栈 / 20 冒烟 / 新栈 / 120 r1 / 停栈 |
| `streams/csharp-c2-attack-stream.tsv` | C# 专用流（c2；c1 与之逐字节相同，sha 见下） |
| `streams/36-csharp-c1-c2-stream-identity.txt` | c1/c2 `attack_stream_sha256` 相同（`0c7f44aa…`） |
| `streams/rust-r1-rng-stream.tsv` | Rust r1 RNG 流水（含 `ISOLATE_ATTACK 40545` 头 + 攻击站点抽数） |
| `streams/*-server-state.tsv` | 两侧服务端权威逐击状态 dump（目标可用性门两侧强制的原始件） |
| `diffs/30..32-*.txt` | `attack_diff.py diff`：c1×c2 / c2×r1 / c1×r1 |
| `requests/40-analyze.{txt,json}` | 两侧逐次取数请求序列（站点计数全等；index=3 假阳性） |
| `requests/42-jointable-*.txt` | 两侧逐击表（`state=server`）+ `align_vs_rng_hit_mismatch`（C# 20 个 k） |
| `requests/43-stream-equiv.{txt,json}` / `44-stream-equiv-selftest.txt` | 流级等价判据 GREEN 651/651 + 牙齿 |
| `sessions/{csharp-20261013-013848,csharp-20261013-015115,rust-20261013-015537}/` | 三个会话的 `attack_seq.jsonl` + `attack_isolation.json` |
| `commands.txt` | 全部命令与原始输出落点 |

## 忠实登记

- **DoD②（`attack_diff.py diff` 逐 k=0）未达成**；本包不主张达成，也不改判据/口径。
- 为让 F-N1 纯度门（`player_units == -AttackCount`）成立，被测 120 必须**每栈一次**（hook 以 `FileMode.Create`
  截断专用流）⇒ 「同栈连跑两次」与该门**当前不兼容**；本包用「两次各自新栈」验 C# 侧可复现（sha 相同）。
  这一「门与同栈重复性的张力」如实登记，处置归 owner / 夹具（m3bag）写集。
- `attack_diff.py` / `align` / `run_attack_pair.ps1` / `server_state_gate.py` / `attack_stream_gate.py` 属 m3bag 写集，
  本包**只读引用**；`stream_equiv.py` 为 m3cs 新工具（本包内）。
