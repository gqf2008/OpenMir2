# 残差② 归因 + 修复：取攻击目标用**状态化朝向**（oracle）而非本帧消息 dir（Rust 原实现）

- 卡：`openmir2-m3-attacker-attr-loading`（issue `f47306a7`，owner 2026-10-12 17:35 裁决残差②）
- 作者：`openmir2-worker-11` / worktree `m2-svr` / branch `worktree-m2-gamesvr-host`
- base：`refs/heads/master` = `88661505`（攻方属性装载片合并点）
- 本片只动 `mir2-rs/bins/game-svr/src/main.rs`（本 lane 写集）；未动 `crates/world`、`tests/parity/m3`、`src/**`

## 1. 结论（二选一：**该改 Rust 侧**）

oracle 的真机攻击路径用 **状态化朝向**（对象当前 `Dir`）取目标，**不是**本帧消息里的 dir；Rust 原实现
用 `tag & 0xFF`（本帧消息 dir）求前方格 ⇒ **两侧不一致**。按「行为等价优先」在 `bins/game-svr` 修。

## 2. oracle 证据链（行号）

| 站点 | 内容 |
|---|---|
| `src/GameSrv/Word/WorldServer.cs:835-843` | `CM_HIT` 分支 → `SendActionMsg(Ident, Tag, LoWord(Recog), HiWord(Recog), 0, "")`：`wParam = Tag = 本帧 dir` |
| `src/M2Server/Player/PlayObject.Message.cs:1225-1234` | `ClientHitXY(wIdent, nParam1, nParam2, (byte)wParam /*= 本帧 dir*/, …)` |
| `src/M2Server/Player/PlayObject.Attack.cs:909-911` | `case CM_HIT: AttackDir(null, 0, nDir);` |
| **`src/M2Server/Player/PlayObject.Attack.cs:26`** | `IActor attackTarget = targetObject ?? GetPoseCreate();` ← **取目标（读当前 `Dir`）** |
| `src/M2Server/Actor/BaseObject.cs:1253-1262` | `GetPoseCreate(){ if(GetFrontPosition(ref nX,ref nY)) return Envir.GetMovingObject(nX,nY,true); }` |
| `src/M2Server/Actor/BaseObject.cs:977-1030` | `GetFrontPosition` 的 `switch (Dir)` —— **读的是对象当前 `Dir`** |
| `src/M2Server/Player/PlayObject.Attack.cs:75` | `AttackDir(attackTarget, nPower, nDir);` → 落到 `BaseObject.Attack.cs:10` |
| **`src/M2Server/Actor/BaseObject.Attack.cs:12`** | `Dir = nDir;` ← **取目标之后**才把 `Dir` 置为消息 dir |
| `src/M2Server/Player/PlayObject.Attack.cs:76` | `SendAttackMsg(GetHitMode(wHitMode), Dir, CurrX, CurrY);` ← 动作帧用**新** `Dir` |

顺序不可颠倒：`GetPoseCreate()`（行 26）在 `Dir = nDir`（`BaseObject.Attack.cs:12`）**之前**。
`nDir` 只影响此后的动作帧（行 76）与后续朝向。原始件见 `oracle-chain.txt`。

## 3. Rust 原实现（改前）

`main.rs::handle_hit_cmd`：
```rust
let dir = u8::try_from(tag & 0x00FF).unwrap_or(0);
set_display_dir(displays, actor_id, dir);            // 先 Dir = nDir
let (fx, fy) = attackwire::front_position(dir, ...); // 再用本帧 dir 求前方格  <= 与 oracle 不同
```

## 4. 改后

```rust
let dir = u8::try_from(tag & 0x00FF).unwrap_or(0);
let pose_dir = displays.get(actor_id).map_or(dir, |d| d.dir);   // 状态化朝向（取目标用）
let (fx, fy) = attackwire::front_position(pose_dir, ...);        // oracle PlayObject.Attack.cs:26
set_display_dir(displays, actor_id, dir);                        // 之后才 Dir = nDir（BaseObject.Attack.cs:12）
```
- 动作帧仍用 `dir`（= 本帧消息 dir），对齐 `PlayObject.Attack.cs:76` 的 `SendAttackMsg(..., Dir, ...)`。
- 状态化 `Dir` 由 bin 侧 `downlink::DisplayTable` 承载（`crates/world::Entity` **无** `dir` 字段；
  进世界时 `ActorDisplay::new(inputs.dir, …)` 用 `record.dir` 初始化，`set_display_dir` 维护）。
  展示表缺失时退回本帧 dir（等价旧行为；生产路径进世界必登记展示表）。

## 5. 判据

- **绿**：`cargo test -p game-svr --bin game-svr hit_cmd_dispatches`
  ⇒ `ok`。用例段 ② 现设「状态化朝向 = 0(上)，本帧 dir = 4(下)」，怪在正上方 (10,9)：
  取目标必须用状态化朝向才命中；动作帧 `Series` = 新 Dir = 4。
- **红检（改坏必红）**：把 `front_position(pose_dir, …)` 改回 `front_position(dir, …)`
  ⇒ 前方格变成 (10,11)（下）、无目标 ⇒ 用例 `FAILED`（`attacks: []`）。原始输出
  `redcheck-stateful-vs-message-dir.txt`。还原后复跑 ⇒ `ok`。
- 四门禁：见 `gates.txt`（fmt 0 / clippy 0 / 80 suites 664 passed 0 failed 16 ignored / msgcodegen 604）。

## 6. 如实登记（**未放宽判据**）

- 本片修的是**取目标所用 Dir 的来源**这一处字节级行为不等价，**不声称**因此让
  `attack_diff.py diff` 归零。owner 裁决原文即「先做归因尖刺再定修/转」，且上一实例的尖刺已记：
  本 C# 流 `AttackSuccess@223` 本次仅 3/120，「旧 Dir」在 pin 摆位下理应为 2、119/120 都能找到 victim
  ⇒ **80 项的成因量级仍主要落在 ① RNG 流位置（m3bag 仪器账本）与 ③ 夹具缺 `my` 的重复计数（m3bag）**。
  本片只把「② 该改哪一侧」钉死并按 oracle 落修；DoD② 逐 k 归零仍以 0137 两相复跑为准。
- 未起栈/未占端口（窗口让给 0137 两相）；端口实拍见 `commands.txt` §6（仅共享 3306）。

## 7. 残留物清单

- 证据目录：`tools/m2-acceptance/evidence/m2svr-stateful-dir-20261012/**`（本目录）。
- 未新增 `E:/tmp` 专属残留；隔离 target 复用 `E:/tmp/m3cs-b1b2-target`（既有，跨片共享）。
