# 证据包 — M3②③ 观测口径收口：`align_vs_rng_hit_mismatch`（C# 20 k）+ 分析器 `(kind,arg)` 假阳性（2026-10-13）

- 卡：`openmir2-m3-attack-pair-fixture`（issue `76cc08b8`）
- 基准树：`master 17f55b2d`（含 m3cs 的 F-N1 后两相实跑 `5e502a97`、本车道 F-N1 `1e7f2280` / EOL 生产者侧 `59401d19`）
- 写集：`mir2-rs/tests/parity/m3/**`；**零 `.rs`**；**未起栈、未占端口**（纯离线复算，输入为 m3cs 实拍的两相原始件）
- 判据口径：**逐 k `hit`/`dmg` 以服务端权威 `SERVER_STATE` 为准**（owner 2026-10-13 02:15 明写「与服务端 SERVER_STATE/RNG 推出的命中」）；
  两侧**同一口径（各侧自身 dump）**——实测两侧受害 HP 轨迹逐点相同（非同一文件自比）⇒ 逐 k 可比。

## 结论（三条）

1. **`align_vs_rng_hit_mismatch`（C# 20 个 k）根因 = 观测/仪器，已定位并修到 0。**
   根因不是「取错字段」「AE `Series/Tag1` 语义」，而是 **`attack_diff.py align` 的 k 归属**：
   C# 侧 BotSrv 的攻击序列**不等 `SM_STRUCK` 响应**就发下一击（固定 `step_ms=500`），而服务端向
   **攻击方 + 受害方各发一份** `SM_STRUCK`。当响应跨过下一个 `role=attack` 标记时，旧口径
   「`struck` 归最近一次 `k`」会把**同一击的两个观察者帧分到相邻 k**（前一个 k 少算、后一个 k 多算），
   且按 k 分段的事件键因跨 k 而无法合并这对重复帧。
   实测（`01`/`03`）：同一条专用种子流（`stream_equiv` 651/651 逐次等价、C# 跨栈 `sha 0c7f44aa…` 可复现）
   之下，旧口径 C# `hits=81/75`、Rust `hits=57`；修后两侧 `hits=57 / sum=2425` 完全一致。
2. **修法 = 改用服务端权威逐击状态 dump 派生逐 k `hit`/`dmg`（同一口径）。**
   `SERVER_STATE k` = 处理第 k 击**之前**的服务端权威受害 HP（C# `StartupHook.DumpActors` 在玩家单元
   首抽时落；Rust `state_log.rs` 同形；两侧实测**逐字节同形**）⇒
   `dmg_k = preHP[k] − preHP[k+1]`（`k < N`）、`dmg_N = preHP[N] − min(客户端 struck 结果 HP)`；
   `hit = dmg > 0`。**与时序无关**、**两侧同一口径**（各侧自身 dump；实测 HP 轨迹逐点相同）⇒ 逐 k 可比。
   实现：`attack_diff.py align --server-state <dump>`（或 `attack_raw.jsonl` 同目录有
   `server_state_segment.tsv` 时**自动采用**；见 `04-align-after-server.txt` 的 `caliber=server-state`）。
3. **分析器 `(kind,arg)` 假阳性（`index=3`）已修。**
   Rust `random_range(0, n)` 记 `Next(min,max)`（packed arg 高 32 位 = min=0），C# 等价的 `Random(n)`
   记 `Next(max)`；二者语义相同（同为 `[0,n)` 均匀取数）。旧 `first_divergence` 按**原始标签**比 ⇒
   在 index=3 报「`player_path` 分歧」（两侧 `repr` 实为同一 `hit_struck_damage(37)->22`）。
   修法：按 `Next(min,max)` 且 **min==0** ⇒ 归一为 `Next(max)` 后比；**真的 `min>0` 不得抹平**。
   `per_strike_requests.py` / `compare_dedicated_streams.py` 两处同口径修复。

## DoD②（`attack_diff.py diff` 逐 k = 0）现状

| 对 | 旧 client 口径（`08`） | 权威口径（`05/06/07`） |
|---|---|---|
| c1 vs c2 | RED 72 | **GREEN 0** |
| c2 vs r1 | RED 102 | **GREEN 0** |
| c1 vs r1 | RED 121 | **GREEN 0** |

（旧口径的 72/102/121 与 m3cs 在其报告 `M3②③活栈隔离F-N1后两相实跑与流级等价定位报告-20261013.md`
里报的数**逐项一致** ⇒ 本次复算确为同一现象。）
`jointable`：旧口径 **RED** 点名 `side=csharp caliber=client-smstruck ks=[1,5,9,16,20,32,35,48,54,60,63,69,86,89,93,100,105,108,112,119]`（20 个）；
权威口径 **GREEN**，0 条 mismatch（`09`/`10`）。

> **口径诚实说明**：逐 k 判据的权威源是**服务端** dump；客户端 `SM_STRUCK`（异步、双观察者）保留为
> **诊断**（`summary.filtered_foreign/other_observer/deduped` 仍由它统计，`03` 可见 c2 `other_observer=57
> deduped=24`）。改口径是**收紧到服务端权威**，不是放宽；`attack_diff.py diff` 的**非平凡门**（`attacked≥1
> / hits≥1 / sum>0`）与逐项 `hit/dmg/hp/maxhp` 比较一字未动。

## 工件索引

| 路径 | 内容 |
|---|---|
| `01-analyze-before.txt` / `02-analyze-after.txt` | 旧实现 index=3 假分歧 / 修后 GREEN（`class=none`） |
| `03-align-before-client.txt` / `04-align-after-server.txt` | `align` 旧 client 口径（c1=81/c2=75）/ 权威口径（三侧 57/2425） |
| `05/06/07-diff-*.txt` | 权威口径三对逐 k diff = **0** |
| `08-diff-client-caliber-red.txt` | 旧口径三对 RED 72/102/121（改坏必红） |
| `09-jointable-before-client.txt` | 旧口径 jointable RED，点名 20 个 k |
| `10-jointable-after-server.txt` | 权威口径 jointable GREEN |
| `11/12/13-selftest-*.txt` | 三工具自检 GREEN 41/41、29/29、9/9 |
| `14-teeth-mutations.txt` | 三处**改坏必红**（关权威口径 / 退回 raw `(kind,arg)` / mismatch 降回 info） |
| `15..18-gate-*.txt` | 四门禁 fmt / clippy / test / msgcodegen（全 rc=0；隔离 target） |
| `19-selfcheck-nostack.txt` | `run_attack_pair.ps1 -SelfCheckNoStack` 全绿（含新增 ⑮b：`good_rc=0 bad_rc=2 caliber_ok=True`） |
| `20-smstruck-broadcast-scope-diff.txt` | **登记（实现侧，非本片写集）**：C# `SM_STRUCK` 2 帧/击（广播到观察者）vs Rust 1 帧/击 |
| `21-followup-reviewer6-minors.txt` | 复核订正（reviewer-6 M-1/M-2/M-3）后的重验证：`verify --check-extra` + `eolcheck` + 三工具 selftest 全 GREEN |
| `commands.txt` | 全部命令与原始输出落点 |

## 复核订正（reviewer-6 `88f120f3`，2026-10-13）

> 本目录内容随本片一并订正；改动**只碰证据件本身**（`01`/`GATES`/`README`/`commands` + 重算 `SHA256SUMS.txt`），
> **未改任何夹具/生产函数**（`attack_diff.py` / `per_strike_requests.py` / `compare_dedicated_streams.py` /
> `run_attack_pair.ps1` 一字未动），结论不变。

- **M-1（已修）**：`01-analyze-before.txt` 原为一次 Python `FileNotFoundError` traceback（旧实现被 copy 到
  `E:/tmp/psr-old.py` 后 `load_site_map()` 相对 `__file__` 解析成 `E:\rng-isolate-attack\attack_sites.tsv`），
  并非索引所写的「旧实现 index=3 假分歧」。现改为**在 `attack-request-sequence/` 内原位执行**从 base
  `17f55b2d` 取出的旧实现（`load_site_map()` 正常解析站点表）⇒ 真输出：
  `index(0based)=3 class=player_path`，两侧 `repr` 同为 `hit_struck_damage(37)->22`，`判读 RED player_path_divergence@draw_index=4`。
  命令已同步修订进 `commands.txt`（含「必须原位执行」的原因）。
- **M-2（已修）**：`GATES.txt` 惰性判定原记「本片改动仅 4 个文件」⇒ 订正为 **5** 个（补 `attack-request-sequence/README.md`）。
- **M-3（已修）**：本 README 原写「两侧用**同一个 dump** ⇒ 逐 k 可比」，易被误读为同一文件自比 ⇒
  改为「两侧**同一口径（各侧自身 dump）**；实测两侧受害 HP 轨迹逐点相同」。

订正后重验证件：`21-followup-reviewer6-minors.txt`（`verify --check-extra` GREEN 24/24、`eolcheck` GREEN 25/25、
三工具 selftest 41/41 + 29/29 + 9/9 全 rc=0）。

## 复现（干净检出，全离线）

1. `git checkout <本片 head>`；
2. 按 `commands.txt` 逐步跑（真机输入 = `../evidence-m3-isolate-twophase-fn1-20261013/**` 的追踪件；
   运行前把会话 `attack_raw.jsonl` + `server_state_segment.tsv` 复制到临时目录，**不改追踪件**）；
3. `attack_diff.py selftest` / `per_strike_requests.py selftest` / `compare_dedicated_streams.py selftest` 全 GREEN；
4. 四门禁全 rc=0。

## 忠实登记 / 残留

- **未做**：真实两相重跑（需窗口）。本片结论全部由 m3cs 已入追踪件的**实拍原始件离线复算**得出；
  若 owner 要真机确认，`run_attack_pair.ps1` 的 `Invoke-AttackAlign` 已在会话目录有
  `server_state_segment.tsv` 时自动走权威口径（`19` 的 ⑮b 钉住），无需改命令行。
- **口径边界（如实登记）**：第 N 击之后没有 `SERVER_STATE` 行（dump 一行/玩家攻击单元，共 N 行），
  故 `dmg_N` 取 `preHP[N] − min(客户端 struck 结果 HP)`；这**只用于最后一击**，且若第 N 击之后
  HP 无进一步下降则恒为 0（本会话第 120 击为 dodge，`sum` 与 Rust 旧口径一致）。
  若将来需要完全摆脱客户端帧，宜由 dump 侧多落一行（归 `bins/**` / 影子 hook 写集，非本片）。
- **受伤者 HP 回升（治疗/重置）**：权威口径 **fail-closed rc=2**（HP 差口径不适用），已在
  `attack_diff.py selftest` 的 `(r)` 与 `run_attack_pair.ps1` ⑮b 钉住，`14` 有红检。
- **发现并登记一条实现侧差异（只登记、不修，见 `20`）**：C# 服务端把 `RM_STRUCK` **广播给附近对象**
  （`BaseObject.Attack.cs:63` → 每个 `PlayObject` 各自发 `SM_STRUCK`），而 Rust downlink
  （`bins/game-svr/src/main.rs:2229`）**只向受害者**发。实测：C# 114 帧 = 57 受害视角 + 57 攻击方视角；
  Rust 57 帧。这**是双观察者帧的来源**（也是旧口径重复计数的一个输入），但**不是**本片修的那个
  归属/仪器缺陷（即使只看受害一方的帧，异步跨 k 仍会错位）。它**不影响** RNG 抽数与服务端权威状态；
  若 M2 的「同序 s2c diff」按广播口径判，Rust 缺的这一帧就是真差异 ⇒ 应转 **m2-svr**。
- 本片未改 `crates/**` / `bins/**` / `src/**` / 任何 `.rs`；未动 m3cs 的证据包（只读引用）。
