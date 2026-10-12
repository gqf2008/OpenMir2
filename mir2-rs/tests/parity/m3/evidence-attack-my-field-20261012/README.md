# M3②③ 攻击对拍夹具：`my` 字段（残差③）+ 同种子抽数账本（残差①）

- 卡：`openmir2-m3-attack-pair-fixture`（issue `76cc08b8`）／车道 `m3bag`／principal `openmir2-worker-4`
- 任务来源：owner 2026-10-12 17:35（brief 顶部，两条都在本卡写集）
- base：`refs/heads/master` = `324a182b1c6061ac2f7a42f85752bba6fd6ea426`
- 本片四门禁：fmt 0 / clippy 0 / test 全绿 0 failed / msgcodegen 604（见 `10..13-gates-*`）
- 本片**零 `.rs`、零 `src/` 行为改动默认路径**（`src/BotSrv/**` 属已登记 T-29 夹具，默认关不变）；未起服务端栈、未占端口

> ⚠ 本环境下 Windows PowerShell 5.1 缺 `Get-FileHash`（`run_attack_pair.ps1 -SelfCheckNoStack` 会在 `Test-M3MonGenEmpty` 抛
> `CommandNotFoundException`）；本片自检用 **`pwsh`（PowerShell 7）** 跑，见 `05-run-attack-pair-selfcheck-nostack.txt`
> 首部记录。这不是本片引入的问题（`gate_guards.ps1` 既有代码），复审请用 `pwsh` 复跑或确认本地 PS 5.1 有该 cmdlet。

---

## 0. 结论速览

| # | owner 要求 | 结果 | 证据 |
|---|---|---|---|
| ③-① | bot 侧把 `my` 写进 attack 行 | 完成（attack + struck 行都落 `my` = 本假人自身 ActorId） | `src/BotSrv/Player/RobotPlayer.cs`、`RobotPlayer.Operate.cs` |
| ③-② | 判据层缺字段 **fail-closed** | 完成：`attack` 行缺 `my` ⇒ `attack_diff.py align` **rc=2**，不静默降级 | `03-mutation-missing-my-failclosed.txt`、`04-...selftest.txt`（l 组） |
| ③-③ | 改坏必红 + 修前/修后 diff 项数 | 完成（见下 §1；**结论：80 项里 0 项是 `my` 造成的假阳性**） | `01-before-align-diff.txt` / `02-after-align-diff.txt` / `03-...` |
| ① | 两侧同种子抽数账本机判（D2 口径） | 仪器完成并跑通 C# 半（新 `attack_rng_ledger.py`，8/8 selftest）；**Rust 生产路径无 per-call RNG trace ⇒ 两账本逐 k `diff` 待 m2-svr/m3cs 补同形 trace**，定位结论见下 §2 | `06/07/08-rng-ledger-*` |

---

## 1. 残差③：`role=attack` 行缺 `my` ⇒ 判据静默失效 / 重复计数

### 1.1 现场（真机原始件）

同一击 `SM_STRUCK` 会被 **victim 与 attacker 两个假人各落一遍**，且 C# 栈对同一观察者**各投递两次**
（`src-logs/bots-20261012-170149-csharp.txt`，`机器人:[id]` 前缀为发射者）：

```text
17:02:00.940 [INF] 机器人:[m2svrk2c11] M3ATTSEQ {"role":"struck","dmg":1,"hp":17,"maxhp":19,"src":1808018587,"actor":1081367641}
17:02:01.251 [INF] 机器人:[m2svrk2c10] M3ATTSEQ {"role":"struck","dmg":1,"hp":17,"maxhp":19,"src":1808018587,"actor":1081367641}
17:02:01.251 [INF] 机器人:[m2svrk2c11] M3ATTSEQ {"role":"struck","dmg":1,"hp":17,"maxhp":19,"src":1808018587,"actor":1081367641}
17:02:01.438 [INF] 机器人:[m2svrk2c10] M3ATTSEQ {"role":"struck","dmg":1,"hp":17,"maxhp":19,"src":1808018587,"actor":1081367641}
```

同一击被落 4 条（attacker `m2svrk2c10` 2 条 + victim `m2svrk2c11` 2 条），`hp=17` 全程不变 ⇒ 是**同一事件**。
历史 `attack_raw.jsonl` 由 `run_pair.ps1` 正则抽取时**丢掉了发射者前缀**，`attack_diff.py` 的
`my = ev.get("my", my)` 又拿不到 `my` ⇒ `src == my` 过滤**静默失效**、同帧被 `row["dmg"] += dmg` **累加**
（旧 align：C# `hits=2 sum=4`，而 C# 实际只有 **1 击**）。

### 1.2 修法（三处，均在本卡写集）

1. **BotSrv（T-29 扩展，默认关路径不变）**：`MarkInWorld` 记下 `_myActorId = SM_LOGON.Recog`；
   `AttackSequenceTick` 的 `attack` 行、`SM_STRUCK` 分支的 `struck` 行都落 `"my":<本假人 ActorId>`。
2. **`attack_diff.py::align_events` fail-closed**：`role=attack` 行缺 / 非 int `my` ⇒ `FailClosed` ⇒ rc=2；
   `struck` 行缺 `src` 同样报错。**绝不**降级成「不过滤」。
3. **去重口径**：`struck.src == my`（命中者=攻击方，排外来帧）后，按 **事件键** `(src, actor, dmg, hp, maxhp)`
   全局去重。**注意不按「观察者 == 攻击方」过滤**——Rust 栈只向 victim 发 `SM_STRUCK`
   （`src-logs/bots-20261012-172029-rust.txt` 里 19 条 struck 全来自 victim `m2svrk2r21`），
   按观察者过滤会把 Rust 侧全滤掉；事件键去重对「C# 双投递 + 双视角」与「Rust 单投递单视角」都成立。

### 1.3 修前 / 修后 diff 项数（owner 要的「80 项里多少是假阳性」）

两侧 = m2-svr k=2 会话 `csharp-20261012-170110` vs `rust-20261012-171957`（同一 base、同 seed=42、图 0137）。

| | C# align（本侧统计） | Rust align | 跨侧 diff |
|---|---|---|---|
| **修前**（旧 align，orig raw） | `hits=2 sum=4`（1 击被算 2 次） | `hits=19 sum=19` | **80 项** |
| **修后**（新 align，补 `my` 的 raw） | `hits=1 sum=1`（`deduped=3`） | `hits=19 sum=19` | **83 项** |

原始输出：`01-before-align-diff.txt`（80 项）、`02-after-align-diff.txt`（83 项）。

**判读（关键）**：修后项数**没有下降反而 +3**，因为修好的是 **C# 本侧统计**（把重复的 1 击还原为 1 击），
而 C# 真实命中数（1）与 Rust（19）本就差得更远。⇒ **80 项里由「缺 `my`」造成的假阳性 = 0 项**；
这 80+ 项是**真实的逐 k 命中模式分歧**（Rust 19 击 vs C# 1 击）。owner 可直接据此把残差③从
「80 项的假阳性来源」中划掉，把注意力集中到残差①②（RNG 流位置 / Dir）。

> C# 侧只有 1 击而 Rust 19 击，另有残差②（C# `GetPoseCreate` 用**状态化 Dir**、Rust 用消息 `Tag` 的 dir）
> 的贡献：m2-svr 归因尖刺实测 C# `AttackSuccess@223` 仅 3/120。两残差都留给 owner/m2-svr，不在本片判据放宽。

### 1.4 改坏必红（实跑）

- `03-mutation-missing-my-failclosed.txt`：只删 `attack` 行的 `my`（120 行）⇒
  `FAIL_CLOSED: attack 行缺 my 字段 …` **rc=2**；还原 ⇒ `ALIGN_OK … hits=1 …` rc=0。
- `04-attack-diff-selftest.txt`：`attack_diff.py selftest` **28/28**（新增 l 组：缺 `my` 的 `align_events` 与
  `cmd_align` 都 rc=2；m 组：外来帧过滤 / 多视角+重复投递按事件键去重；n 组：struck 缺 `my` 仍可对齐；o 组：无 attack 行 fail-closed）。
- `05-...-selfcheck-nostack.txt`：`run_attack_pair.ps1 -SelfCheckNoStack` **rc=0**，新增 **⑲** 断言
  `Invoke-AttackAlign` 在 attack 行缺 `my` 时返回 **`[int] 2`**（改坏 ⇒ `attack_my_missing_not_rc2` 必红）。

---

## 2. 残差①：同种子抽数账本（D2 口径）——仪器 + 定位

### 2.1 仪器 `attack_rng_ledger.py`（新，本卡写集）

把 rng-hook 的 `rng-stream.tsv`（`seq \t kind \t max \t result \t site`）按**每次攻击恰调用一次的站点**
（C# = `M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2929`，真机 120 击实测恰 120 次）
分段，得到「第 k 次攻击窗口」的取数账本：

- `summary <trace>`：总数 + 攻击路径取数 vs 噪声取数占比 + Top 噪声站点；
- `segment <trace> --marker <site>`：逐 k 输出 `{seg, draws_total, draws_attack, draws_noise, first_seq, last_seq}`；
- `diff <a.segments.jsonl> <b.segments.jsonl>`：逐 k 对齐，打印「**第几步 / b 多抽或 b 少抽 X**」，不等即 rc=1；
- `selftest`：同账本绿 / 多抽、少抽必红且定位 / 段数不等必红（**8/8**，`08-rng-ledger-selftest.txt`）。

### 2.2 C# 半实跑（`06/07-rng-ledger-*`）

```text
RNG_LEDGER_SUMMARY TOTAL=1,670,754  attack_path=275  noise=1,670,479  noise_ratio=0.9998
  NOISE 1,060,321  M2Server.Actor.AnimalObject.Wondering@AnimalObject.cs:427
  NOISE   195,913  GameSrv.Word.WorldServer.MonGetRandomItems@WorldServer.MonGen.cs:451
  NOISE   128,717  GameSrv.Npc.Merchant.Run@Merchant.cs:132
  ...
RNG_LEDGER_SEGMENTS segments=120
  SEG k=1 total=1574 attack=4 noise=1570
  SEG k=2 total=1819 attack=6 noise=1813
  ...
```

### 2.3 定位（机判结论）

1. **攻击路径取数是有界的**：120 段里 `draws_attack ∈ {2,4,6}`（miss=2；命中再串 `_Attack@41` /
   `GetHitStruckDamage@792` / `StruckDamage@2957` / `AttackSuccess@223`）。⇒ 攻击路径本身**没有**
   「多抽/少抽」级缺陷可定位（若 Rust 生产路径多抽/少抽，其账本会在此列上给出差的段号）。
2. **噪声占 99.98%**：每段 `draws_noise` 在 ~800…3000+，且由**钟/线程驱动**的怪 AI（`AnimalObject.Wondering`）
   / NPC（`Merchant.Run`）/ 刷怪（`MonGetRandomItems`）产生。⇒ 「同种子 ⇒ 逐 k 相同」的前提被
   **非攻击 RNG 消费者**打破。按 owner 的归类规则，这属**夹具/两侧起点类**（非 Rust 生产路径多抽）：
   R-1 只隔离了 **pin 图**的怪（0137 零刷怪），**没有**隔离全局 `M2Share.RandomNumber` 上其它图/其它系统的消耗。
3. **两侧账本逐 k `diff` 当前跑不了**：Rust 生产路径**没有** per-call RNG trace（`bins/game-svr` 只有
   `SEED_PROBE` 那 8 个值，见 `start_rust_stack.ps1` / `seed.rs`）。补一份**同形** `rng-stream.tsv`
   是 `.rs` 改动（`crates/shared/src/rng.rs` 记录钩子），不在本卡写集 ⇒ **交 owner，转 m2-svr/m3cs**。
   仪器已按两侧同形实现，补齐后一条命令即可给出「第几步、多抽还是少抽」。

---

## 3. 边界（改动面 / 未做）

- 改动：`src/BotSrv/Player/RobotPlayer.cs`、`RobotPlayer.Operate.cs`（T-29 夹具，仅 `AttackSequence` 打开时生效；
  默认关 ⇒ 不新增任何字段/发送）、`mir2-rs/tests/parity/m3/{attack_diff.py, run_attack_pair.ps1, whitelist.md, rebuild_attack_my.py, attack_rng_ledger.py}` + 本证据目录。
- 未做（如实登记）：Rust 侧 RNG 抽数 trace（`.rs`，转 m2-svr/m3cs）；Rust 侧真实 k=2 复跑（要窗口，判据轮归 m3cs）。
- `rebuild_attack_my.py` 只用于**离线复算历史会话**，不参与生产判据；其产物与原 `attack_raw.jsonl`
  除 `my` 外**逐事件相同**（`02-after-align-diff.txt` 记录了 `same_minus_my=True`）。

## 4. 复现命令

```sh
# 修前（旧 align）
git show 324a182b:mir2-rs/tests/parity/m3/attack_diff.py > /tmp/old.py
python /tmp/old.py align sessions/csharp-20261012-170110/attack_raw.orig.jsonl --out /tmp/c.seq.jsonl
python /tmp/old.py align sessions/rust-20261012-171957/attack_raw.orig.jsonl --out /tmp/r.seq.jsonl
python /tmp/old.py diff /tmp/c.seq.jsonl /tmp/r.seq.jsonl --max-diffs 200        # 80 项

# 修后（补 my + 新判据）
python mir2-rs/tests/parity/m3/rebuild_attack_my.py src-logs/bots-20261012-170149-csharp.txt sessions/csharp-20261012-170110/attack_raw.jsonl
python mir2-rs/tests/parity/m3/rebuild_attack_my.py src-logs/bots-20261012-172029-rust.txt sessions/rust-20261012-171957/attack_raw.jsonl
python mir2-rs/tests/parity/m3/attack_diff.py align sessions/csharp-20261012-170110/attack_raw.jsonl --out sessions/csharp-20261012-170110/attack_seq.jsonl
python mir2-rs/tests/parity/m3/attack_diff.py align sessions/rust-20261012-171957/attack_raw.jsonl  --out sessions/rust-20261012-171957/attack_seq.jsonl
python mir2-rs/tests/parity/m3/attack_diff.py diff sessions/csharp-20261012-170110/attack_seq.jsonl sessions/rust-20261012-171957/attack_seq.jsonl --max-diffs 200   # 83 项

# 自检
python mir2-rs/tests/parity/m3/attack_diff.py selftest          # 28/28
python mir2-rs/tests/parity/m3/attack_rng_ledger.py selftest    # 8/8
pwsh -File mir2-rs/tests/parity/m3/run_attack_pair.ps1 -SelfCheckNoStack   # rc=0

# 抽数账本（C# 半）
python mir2-rs/tests/parity/m3/attack_rng_ledger.py summary <rng-stream.tsv>
python mir2-rs/tests/parity/m3/attack_rng_ledger.py segment <rng-stream.tsv> --marker "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2929" --json csharp-170110.rng-segments.jsonl
```

## 5. 残留物清单

- 本目录内所有原始件均已入库（`SHA256SUMS.txt` 覆盖）；`src-logs/*.txt` 是从 m2-svr 车道
  `tests/botload/bots-20261012-170149.log` / `bots-20261012-172029.log` 复制并改后缀（`.log` 受 `.gitignore` 限制），
  内容逐字节相同、只读留档。
- `E:/tmp/k2repro/`、`E:/tmp/attack_diff.old.py`、`E:/tmp/before-*.seq.jsonl`、`E:/tmp/mut.seq.jsonl`、`E:/tmp/restore.seq.jsonl`、
  `E:/tmp/m3bag-myselfcheck.log`、`E:/tmp/m3bag-myfield-target/`（隔离 cargo target）为本片临时物，**未入库**、可随时删除。
- 未新增/改任何数据文件（`Envir/**`、`sql/**`）、未动 `src/OpenMir2/**` 与 `MirClient`。
