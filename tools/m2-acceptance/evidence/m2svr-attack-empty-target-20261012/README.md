# Rust 活栈「84/120 击空打」归因（m2-svr 半）— 2026-10-12

- 卡：`openmir2-m3-attacker-attr-loading`（M3②③ m2-svr 半）／车道 `m2-svr`／principal `openmir2-worker-11`
- 来源：owner 2026-10-12 19:20 派活（brief 顶部；同文见 `openmir2-m3-combat-skills-align` 的 owner
  routing `bda10ca2`，ts 1791804148）：m3cs 活栈账本片（patch `f981157d` @ head `ddf3fbbb`，
  证据 `mir2-rs/tests/parity/m3/evidence-rng-ledger-diff-20261012/**`）报
  「Rust 侧 120 击里 84 击『前方格 (18,20) 无目标』」
- base = `refs/heads/master` 的一支（本片落在 `worktree-m2-gamesvr-host`；见 patch 条）

## 一、结论（先读）

**归类 = (c) 其它：不是「前方格/朝向算错」，也不是「目标查找只在 AOI 里找、victim 未同步」。**

* `front_position(dir=2, 17, 20, w, h) = (18, 20)`；oracle 的 `GetFrontPosition` + `Direction.Right=2`
  同样是 `(x+1, y) = (18,20)`。两侧前方格一致。
* 真机日志里 84 次空打的**前方格确实是 (18,20)**（= 受害者所在格），取目标路径**在工作**
  （中间 36 击进入了攻击路径）。空打**全部**发生在受害者的 HP 归零（`death_k=36`）之后：
  oracle `Envir.GetMovingObject(nX, nY, true)` 用 `!Ghost && !Death` 过滤，死亡目标**本就不该被取到**。
* ⇒ 84 空打是**受害者死亡后的正常 oracle 行为**；Rust 的 `death=true`（`crates/world/src/world.rs:516`）
  与 oracle `Die()`（`BaseObject.Base.cs:351`）一致。
* **同源**：受害者为何在 36 击内死，取决于两侧攻击路径的**取数结构/消费者集合**（C# `hit_gate=9` /
  Rust `hit_gate=36`，C# 有 `attack_success`/`atk_power_base` 站点、Rust 无）——
  正是 m3cs 本片在对齐的东西。详见 `05-same-source-with-m3cs.txt`。

## 二、oracle 的取目标路径（行号）

| 步 | 位置 | 说明 |
|---|---|---|
| 1 | `src/M2Server/Player/PlayObject.Attack.cs:26` | `IActor attackTarget = targetObject ?? GetPoseCreate();`（此句在 `Dir = nDir` **之前** ⇒ 用状态化朝向） |
| 2 | `src/M2Server/Actor/BaseObject.cs:1253-1261` | `GetPoseCreate()`：`GetFrontPosition(ref nX, ref nY)` → `Envir.GetMovingObject(nX, nY, true)` |
| 3 | `src/M2Server/Actor/BaseObject.cs:977-1030` | `GetFrontPosition`：按 `Dir` 取相邻格；`Direction.Right = 2` ⇒ `(x+1, y)` |
| 4 | `src/M2Server/Maps/Envirnoment.cs:1203-1234` | `GetMovingObject`：格子 `ObjList` 里**首个** `!Ghost && !Death`（`boFlag=true`） |
| 5 | `src/M2Server/Actor/BaseObject.Attack.cs:12` | `AttackDir`：`Dir = nDir` 在取目标**之后** |
| 6 | `src/M2Server/Actor/BaseObject.Base.cs:351` | `Die()`：`Death = true`（此后 `GetMovingObject(...,true)` 必不再返回它） |
| 7 | `src/OpenMir2/Enums/Direction.cs:5` | `Right = 2` |

原文摘录见 `01-oracle-attack-target-path.txt`。

## 三、Rust 的取目标路径（行号）

| 步 | 位置 | 说明 |
|---|---|---|
| 1 | `mir2-rs/bins/game-svr/src/main.rs:1902` | `pose_dir = displays.get(actor_id).map_or(dir, ...)`（状态化朝向，oracle `PlayObject.Attack.cs:26` 同序） |
| 2 | `mir2-rs/bins/game-svr/src/main.rs:1908` | `front_position(pose_dir, cur_x, cur_y, width, height)` |
| 3 | `mir2-rs/bins/game-svr/src/attackwire.rs:101-113` | `front_position`：`DIR_RIGHT` ⇒ `(x+1, y)`；越界/非法 ⇒ 原地（照 oracle 的 case 内 `if`） |
| 4 | `mir2-rs/bins/game-svr/src/main.rs:1812-1826` | `front_target`（本片新抽出）：格内首个 `!ghost && !death && id != self` |
| 5 | `mir2-rs/crates/world/src/world.rs:516-518` | 死亡标记 `e.death = true`（注释即「C# `Die()` 的 `Death = true`」） |

原文摘录见 `02-rust-attack-target-path.txt`。

## 四、真机证据（读数）

源：`E:/tmp/m3cs-r2/rust-stack/game-svr.out.log`（m3cs 留档，只读引用）+ m3cs 证据 `sessions/**`。
本次**独立复算**：`grep -c '无目标'` = **84**（全部 `前方格 (18,20)`）、`grep -c 'Ident=3014'` = **120**。

* 攻方 `actor_id=3`（`m3csr2r20`）11:00:28.717 进 `0137 (17,20) Dir:2`。
* 受害者 `actor_id=4`（`m3csr2r21`）11:00:31.924 进 `0137 (18,20) Dir:6`。
* 11:00:31.923 的第 1 击空打：受害者早 1ms 尚未入世界。
* 之后 36 击进入攻击路径（命中/躲避都无日志）；受害者 HP 归零（`death_k=36`）。
* 从 11:00:50.562 起 83 击全部空打；受害者 11:01:34.554 才断线，其间**无移动/无其它命令** ⇒ 不是走开。
* 时间线摘录见 `03-live-stack-log-timeline.txt`；两侧站点计数与 `attack_seq` summary 见 `04-two-readings.txt`。

**两个读数（本片要求）**：

| 读数 | Rust | C# 对照 |
|---|---:|---:|
| 进入攻击路径的击数 | **36** | 120（只有 9 击命中门被求值） |
| 空打击数 | **84** | 0（受害者未死） |

## 五、顺带坐实的一条真接线缺口（**不在本片改**，见 `05`）

oracle 对**空打**仍会抽「基础攻击力」：`PlayObject.Attack.cs:73` 的 `GetBaseAttackPoewr()` 是实参、
**无条件求值**（玩家 override `PlayObject.cs:2909` 抽 1–2 次）；目标为空时 `_Attack(null)` 直接 false、
`AttackSuccess(..., null)` 直接 return ⇒ 空打只抽这几下。Rust 的「无目标」支（`main.rs:1918-1923`）
**不消费任何 RNG**，这解释了两侧 `atk_power_a/b` 的 **36 vs 120**。

但 oracle 是在**消息处理线程同步**抽数、Rust 有目标时是在 **tick** 里抽（`Command::Attack` 入队）⇒
只在无目标分支同步补抽会得到混合次序，比现状更不等价；正确修点在 `crates/world`（= m3cs 本片界面）。
按 §0b 与 owner 19:20「相关」条，本片**不动**，交 owner 协调。

## 六、本片做了什么 / 没做什么

* **代码**：把 `handle_hit_cmd` 里内联的取目标查找抽成 `front_target`（**行为不变**，逐字节同语义），
  并加判据 `front_target_dir2_finds_victim_and_excludes_ghost_or_dead`（dir=2 ⇒ (18,20)、
  自己/`ghost`/`death` 三种排除）；**红检**：删 `!t.death` ⇒ 用例 FAILED（原始件 `06-redcheck-remove-death-filter.txt`）。
  仅为把「空打 = 死亡后正常 oracle 行为」钉成可判红，**不改生产语义**。
* **没做**：未改 `crates/world/**`、`tests/parity/m3/**`（m3cs/m3bag 写集）、`src/**`（oracle）、
  `MirClient`、表结构/脚本/数据格式；未放宽任何门或白名单。
* **窗口**：本片判据全在单测层，**未起栈、未占端口**（窗口归 0137 两相）。

## 七、证据文件

| 文件 | 内容 |
|---|---|
| `01-oracle-attack-target-path.txt` | oracle 取目标路径原文 + 行号（GetPoseCreate / GetFrontPosition / GetMovingObject / Die / Direction） |
| `02-rust-attack-target-path.txt` | Rust 对应实现原文 + 行号（front_position / front_target / death 标记） |
| `03-live-stack-log-timeline.txt` | 真机日志关键行 + 计数（84 / 120 / 时间线） |
| `04-two-readings.txt` | 两侧攻击路径站点计数 + `attack_seq` summary（两个读数 + 对照） |
| `05-same-source-with-m3cs.txt` | 与 m3cs 取数结构片的同源关系 + 缺口定位 + 归属建议 |
| `06-redcheck-remove-death-filter.txt` | 改坏必红原始输出（FAILED）+ 还原复跑 GREEN |
| `SHA256SUMS.txt` | 本目录件 hash（LF 归一/git blob 口径） |

复核：`python -I tools/evidence-hash/verify_sha256sums.py --check-extra tools/m2-acceptance/evidence/m2svr-attack-empty-target-20261012`
