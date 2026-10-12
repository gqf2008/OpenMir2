# 证据包 — M3②③ follow-up（F-2）：真机隔离两相 `scoped_diff` RED 的归因

- 卡：`openmir2-m3-scoped-diff-ability`（issue `05f9079a`，owner 2026-10-13 03:10）
- 车道：`m2-svr`（principal `openmir2-worker-11`）／本片分支 `worktree-m2-scoped-diff-ability`，base `master da94fcb7`
- 上游证据：`mir2-rs/tests/parity/m3/evidence-m3-isolate-twophase-dod2-20261013/**`（m3cs，master `55a29144`）
- 写集：**只新增本目录**（证据 + 报告）；**零 `.rs`**、**零夹具改动**、**未起栈、未占端口**（未要窗口）

## 1. 结论（TL;DR）

1. **方向不是「C# 没写、Rust 写了」**。C# 也写了断线存档，只是**写得比夹具固定 `Start-Sleep 3` 慢**：
   C# c2 掉线 `02:44:26.306` → 落库 `02:44:30`（受害 13552）/`02:44:34`（攻方 13551）（见 `15-*.txt`），
   而夹具在掉线后固定 3s（约 `02:44:29.3`）就取了 `snap_after`。Rust 的断线存档在**亚秒级**内落库
   （掉线 `18:47:41.959` → 请求 `+0.19s` → 应答 `+0.39s`，见 `16-*.txt`）。
   ⇒ 卡上列的 7 处差异里，**5 个能力列（Ac/MAxMP/MaxWeight/MaxWearWeight/MaxHandWeight）+ ModifyTime
   + 受害方 Hp** 全是这个**快照时机竞态**，不是实现差；`characters.Hair` 的 `2→1` 同因（C# 存档把 Hair
   覆写成运行态 1，见 `14-*.txt` vs 上游 `scope/50-*`）。
2. **用「两侧存档都落库」的终态重做一次 `scoped_diff --canonical --fixed-newchr`（见 `12-*.txt`），
   作用域内只剩 1 列**：`mir2_db.characters_ablity.Hp` 的**攻方**行（C# `20668` / Rust `20669`）。
   红检①把这一列对齐 ⇒ **整 9 表 GREEN**（`21-*.txt`），证明残留唯一。
3. 这 1 点是**真差**且**发生在 run 内的世界 tick**（不是存档路径）：C# 服务端权威逐击 dump
   （`17-*.txt`）显示攻方 HP 只在 **tick 86** 从 `20669` 掉到 `20668`，Rust 全程 `20669`（`18-*.txt`）；
   C# 存档记录的 `20668` 与该 dump 一致。⇒ 属「oracle 有、Rust 未建模」的世界 tick 扣血效果，
   具体路径需在 C# 侧插桩定位（候选见 `09-oracle-lines.txt` §C；已排除 DECHP/饥饿/回血）。

## 2. 复现口径（本轮未起栈，用可复算的静态证据）

本片**不占端口**：卡片 DoD 要求「先独立复现 + 定性」，我用的是 (a) 上游已入库的真机两相产物 +
(b) **仍然活着的 `mir2_db`**（上游存档是 `now()` 时间戳，时间线可判）。步骤：

```sh
# ① 重取活库快照（两侧存档都落库之后的终态）——只读 MySQL，不占游戏端口
python tools/dbsnap/dbsnap.py snap --out E:/tmp/m2svr-scopediff/postsavesnap      # -> 03-postsavesnap.txt
# ② 按前缀抽取两侧作用域（c2=C#，r1=Rust）并 diff
python -I mir2-rs/tests/parity/m3/scoped_diff.py extract E:/tmp/m2svr-scopediff/postsavesnap m3finc2 \
  mir2-rs/tests/parity/m3/evidence-m3-scoped-diff-ability-20261013/19-c2-scoped.jsonl --normalize   # -> 10-*.txt
python -I mir2-rs/tests/parity/m3/scoped_diff.py extract E:/tmp/m2svr-scopediff/postsavesnap m3finr1 \
  mir2-rs/tests/parity/m3/evidence-m3-scoped-diff-ability-20261013/20-r1-scoped.jsonl --normalize   # -> 11-*.txt
python -I mir2-rs/tests/parity/m3/scoped_diff.py diff <19> <20> --canonical --fixed-newchr          # -> 12-*.txt（RED 1 列）
python -I mir2-rs/tests/parity/m3/scoped_diff.py diff <19> <20> --canonical                        # -> 13-*.txt（同上）
```

判据**未改动**：`scoped_diff.py` 的 `--canonical` / `--fixed-newchr` 白名单一字未动；
`21/22/23-*.txt` 是「改坏必红 + 对齐即绿」的牙齿。

## 3. 时刻轴（关键证据）

| 侧 | 掉线时刻 | 存档请求/落库 | 相对掉线 |
|---|---|---|---|
| C# c2（RunGate `断开链接`） | `02:44:26.306` | `02:44:30`（victim）/ `02:44:34`（attacker） | **+3.7s / +7.7s** |
| C# c1（同口径，存档时间取自活库） | — | `02:42:32` / `02:42:35` | 同样晚于 c1 的 3s 快照 |
| Rust r1（game-gate `断开链接`） | `18:47:41.959` | 请求 `18:47:42.147`、应答 `18:47:42.348` | **+0.19s / +0.39s** |

夹具 `run_pair.ps1:606-611`（注释即「等掉线存档落库」）用固定 `Start-Sleep 3`；
C# 侧 3.7/7.7s 的落库滞后 ⇒ **必然间歇性错过**（上游 c1/c2 两次都错过；Rust 因亚秒级从未错过）。
这是**夹具侧的确定性缺口**，不是 `bins/game-svr` 的实现差。

## 4. oracle 行号（`characters_ablity` 的写者）

- `src/Storeages/DBSrv.Storage.MySQL/PlayDataStorage.Save.cs:60` → `SaveAblity`（`:139-169` 的 `UPDATE ... ModifyTime=now()`）；
  **oracle 笔误** `Save.cs:146`：`@Ac` 绑的是 `Abil.Level`（⇒ `Ac` 落库 = Level = 255，两侧一致）。
- `src/DBSrv/Services/Impl/DataService.cs:136-137/207`：`DB_SAVEHUMANRCD(101)` → `SaveHumanRcd`。
- `src/GameSrv/Word/WorldServer.cs:1077-1091`：`SaveHumanRcd` → `MakeSaveRcd`（`:1231-1360`，HP 取 `WAbil.HP` `:1247`）
  → `FrontEngine.AddToSaveRcdList`；**断线触发**在 `:713`（`ProcessPlayObjectData` 的 Ghost 支），
  Ghost 由 `PlayObject.Message.cs:369-373` 的 `BoSoftClose/BoEmergencyClose` 支 `MakeGhost` 置起；
  周期触发 `:698-703`（缺省 10 分钟，本 run 无关）。
- Rust 对应：`mir2-rs/bins/game-svr/src/main.rs:1592-1637`（`save_human_rcd`）、
  `record.rs:843` 起（`MakeSaveRcd` 逐字段）、断线调用在清世界之前。**口径与 oracle 一致**，差异只在落库节奏。

## 5. 残留（唯一真差）与处置建议

**残留**：`mir2_db.characters_ablity.Hp` 攻方行 `C# 20668 / Rust 20669`（稳定：c1、c2 皆 20668；r1 20669）。

- 发生点 = **run 内 tick 86** 的攻方扣血（`17-*.txt`），不是存档路径；
- 已排除：地图 `DECHP`（`0137` 的 `MapInfo` 只有 `FIGHT`，Envir 脚本无 `DECHP/SETMAPMODE`）、
  饥饿（部署 `Server.conf`：`HungerSystem=0/HungerDecHP=0`）、回血 tick（只加血）；
- 候选（未插桩定位）：`BaseObject.StruckDamage→DamageHealth`、`ActorBuffSystem` 绿毒
  `DamageHealth(GreenPoisoningPoint+1)`（伤害恰为 1 时 `GreenPoisoningPoint==0`）。
- **处置**：本片**只登记不修**。要钉死这 1 点需要在 C# 影子栈给 `DamageHealth`/`StruckDamage` 插一条
  站点日志（**需两侧栈窗口**）——按卡片 DoD §4，**已在卡上登记申请窗口**，等 owner 排（一次只放一条吃窗口的车道）。
  在窗口到位前，任何「顺手改 Rust 世界 tick」都是猜，违反「口径以实测为准」。

## 6. 对夹具的修复建议（**属 m3bag 写集，本片不改，交 owner 转派**）

把 `mir2-rs/tests/parity/m3/run_pair.ps1:606-611` 的固定 `Start-Sleep 3` 换成**确定性等待**：

- 取本次前缀两条角色的 `characters.LASTUPDATE`（或 `characters_ablity.ModifyTime`）在 **snap_before 后是否被刷新**；
- 轮询到刷新为止（带**上界**，如 30s），超时 ⇒ **fail-closed 红**（`PREREQ_MISSING`/RED），**绝不静默通过**；
- 非存档序列（无 `-BagEquipSequence`/`-AttackSequence`）行为逐字节不变。
- 不建议只调大 `Start-Sleep`：仍是竞态；应「等状态变化」而不是「等固定时长」。

> 另：上游 `scope/50-*` 把 C# `Hair 2→1` 读成「夹具输入差」——实际是 C# 存档把运行态 Hair(1) 覆写回库
> （`14-*.txt`）；夹具修好后 `--fixed-newchr` 下 `characters` 应一起归零（本片 `12-*.txt` 已证）。

## 7. 红检 / 牙齿（原始输出见文件）

| 文件 | 动作 | 期望 | 实得 |
|---|---|---|---|
| `21-redcheck-attacker-hp-matched-GREEN.txt` | 把 C# 攻方 Hp `20668`→`20669`（对齐 Rust） | GREEN（证明残留唯一） | **GREEN 9 表** |
| `22-redcheck-attacker-hp-tampered-RED.txt` | 把 C# 攻方 Hp `20668`→`20670` | RED | **RED 2 行** |
| `23-redcheck-victim-hp-tampered-RED.txt` | 把 C# 受害方 Hp `18244`→`18245` | RED | **RED 4 行** |
| `tools/evidence-hash/verify_sha256sums.py` `selftest` | 改 hash / 多件 / 少件 / 口径 | 各必红 | 见 `32-verify.txt` |

## 8. 工件索引

| 文件 | 内容 |
|---|---|
| `09-oracle-lines.txt` | oracle 存档链行号 + 攻方 Hp 候选路径与排除 |
| `10/11-extract-*.txt`、`19/20-*-scoped.jsonl` | 两侧作用域抽取（含原始 jsonl，可复算） |
| `12/13-postsave-diff-*.txt` | post-save `scoped_diff`（RED 1 列 / canonical 两种开关） |
| `03-postsavesnap.txt` | 活库 13 表快照 manifest（含 sha） |
| `14-live-db-rows.txt` | 活库 `characters` / `characters_ablity` 原始行（C# 落库时间戳） |
| `15/16-*-disconnect-vs-save.txt` | C# / Rust 的「掉线→落库」时刻轴 |
| `17/18-attacker-hp-trajectory-*.txt` | 两侧逐击服务端权威 HP 轨迹（C# tick 86 掉 1） |
| `21/22/23-redcheck-*.txt` | 改坏必红 / 对齐即绿 |
| `31-four-gates.txt`、`32-verify.txt` | 四门禁 + 证据 hash 复核 |

## 9. 残留物清单

- `E:/tmp/m2svr-scopediff/**`（本轮工作目录：`postsavesnap` 快照、`redcheck` 变异件；**未入库**）。
- 未起栈、未占端口：`5000/5100/5500/5600/5700/6000/7000/7100/7200` 本轮**未使用**；仅只读 `3306`（共享 MySQL）。
- 未改任何夹具/生产代码；本目录外零改动。
