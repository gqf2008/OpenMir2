# M3④-实现 D 分片 D②：升级链（`RecalcLevelAbilitys`+`RecalcAbilitys` + `SM_LEVELUP(45)`/`SM_ABILITY(52)`/`SM_SUBABILITY(752)`/`SM_HEALTHSPELLCHANGED(53)`）

> 卡：`openmir2-m3-drop-exp-wiring`（issue `58825bcc`，承接 `openmir2-m3-drop-exp-align` 十项之一）。
> 本片是 D 家族在 **D①（击杀经验链）** 之后的**升级后果**这一段；**掉落落地 / `SM_ITEMSHOW(610)`** 仍属未做（见 §5）。
> base = `origin/master` **`5461c1b1`**（本片开发期间 owner 已把 D① `c059c086` 与 ⑧-2/⑤-3 等卡并入 master；交付前已 `git merge origin/master`）；head = 分支 `drop-exp-host`。
> 工作目录 `<worktree>/mir2-rs`；隔离 `CARGO_TARGET_DIR=E:/tmp/m2svr-d2-target`。

## 0. 一句话

D① 只把经验发出去（`SM_WINEXP(44)` + 会话同步），但 oracle `GetExp`（`PlayObject.Base.cs:149-166`）在
`Abil.Exp >= Abil.MaxExp` 时还会做三件事：`HasLevelUp`（重算等级派生能力 + `SM_LEVELUP`/`SM_ABILITY`/`SM_SUBABILITY`）、
升级 `IncHealthSpell(2000,2000)`（回满血蓝 → `SM_HEALTHSPELLCHANGED`）。本片把这**整段升级后果**按 oracle 补齐，
并把「升级块是否触发」做成 `exp_awards` 的显式位（满级也置位），交给宿主出帧。

## 1. oracle 锚点（原文见 `01-oracle-anchors.txt`）

| 步骤 | 行号 |
| --- | --- |
| `GetExp`（`Abil.Exp +=`、`RM_WINEXP`、升级块） | `PlayObject.Base.cs:149-166`：升级块 `:154`、`Level++` 门 `:157-159`、`HasLevelUp` `:161`、`IncHealthSpell(2000,2000)` `:164` |
| `MAXUPLEVEL = byte.MaxValue` | `MessageSettings.cs:7` |
| `HasLevelUp`（`MaxExp` / `RecalcLevelAbilitys` / `RecalcAbilitys` / `RM_LEVELUP`） | `PlayObject.cs:5242-5248` |
| `RecalcLevelAbilitys`（职业等级公式） | `PlayObject.cs:5254-5317`（Rust 复用已合入的 `mir2_formula::level_ability`） |
| `RM_LEVELUP` 处理器（45 → 52 → 752） | `PlayObject.Message.cs:1670-1676` |
| `IncHealthSpell` / `HealthSpellChanged` | `BaseObject.cs:952-975` / `:700-710` |
| `RM_HEALTHSPELLCHANGED` 处理器（53） | `PlayObject.Message.cs:1736-1739` |
| 消息号 | `Messages.cs:206/211/576/689` |

## 2. 本片实现（写集 `crates/world/**` + `bins/game-svr/**`）

- `crates/world/src/combat.rs`：`award_kill_exp` 的 `leveled` = **进了升级块**（oracle 的 `if Abil.Exp >= Abil.MaxExp`），
  `Abil.Level++` 用 `saturating_add(1)` 复刻 `< MAXUPLEVEL` 门；满级仍置位（宿主仍要重算/回血/出帧）。
- `crates/world/src/world.rs`：`TickRecord::exp_awards` 文档同步（第 3 位语义）。
- `bins/game-svr/src/smframe.rs`：新增 `sm_levelup(exp, level)`（`SM_LEVELUP(45)`，无尾）。
- `bins/game-svr/src/main.rs`：
  - 抽出 `recalc_session_ability(sg, catalog)`（原装备/卸下成功分支的内联块搬入，**单一实现**）；
  - 新增 `drain_levelup_downlink(...)`：对 `leveled` 获得者 ① 重算能力 ② `IncHealthSpell(2000,2000)`
    ③ 同步世界实体 ④ 出 `45 → 52 → 752 → 53`；
  - 运行循环中紧跟 `drain_combat_downlink`（SM_WINEXP）之后调用 ⇒ oracle 帧序保持。

## 3. 判据（新增 2 条，构造级）

| 用例 | 位置 | 钉住 |
| --- | --- | --- |
| `levelup_downlink_sends_levelup_frames_heals_and_syncs_entity` | `bins/game-svr/src/main.rs` | 帧序 44→45→52→752→53、`SM_LEVELUP` 头 `(exp,level)`、`MaxHP` 从 0 提到等级值、回血按新 Max 截断（满血满蓝）、世界实体同步 |
| `kill_exp_marks_levelup_block_even_at_max_level` | `crates/world/src/world.rs` | `leveled` = 进升级块；level→2；满级 255 仍置位但等级不变 |

## 4. 改坏必红（原始件 `40/41/42`，还原核对 `43`）

| 编号 | 变异 | 必红用例 | rc |
| --- | --- | --- | --- |
| RC-D2-A | `sm_levelup` 头 recog/param 交换 | `levelup_downlink_...` | 101 |
| RC-D2-B | 删掉 `IncHealthSpell(2000,2000)` | 同上 | 101 |
| RC-D2-C | 删掉 `recalc_session_ability` 调用 | 同上 | 101 |

## 5. 未做 / 登记（交 owner）

- **掉落落地 + `SM_ITEMSHOW(610)`（D① 已登记）**：oracle 全仓无 `MapItem.ItemId` 赋值点（`MapItem.cs:41`）
  + `Envirnoment.AddItemToMap` 首行 `ItemId==0 ⇒ false`（`:88-92`）⇒ oracle 死亡掉落**恒不落地**、
  `SM_ITEMSHOW` **永不执行**；Rust `world.rs::settle_deaths → place_floor_item` 会落地 ⇒ 两侧「地面物可达面」不同源。
  本片**不擅改**，等 owner §6.0-3 裁决（牵动 T-19/T-25/T-31 与 100 杀口径）。
- **`AddBodyLuck`**：`BodyLuck`/`BodyLuckLevel` 在 oracle 里**无读取点**（仅 `PlayObject.cs:520-521/4304-4323`）⇒ 死字段，不建（登记）。
- **`WinExp` 的 `LimitExpLevel` 支**：默认 `LimitExpLevel=1000 > 255`（`GameSvrConf.cs:1838`）⇒ 生产不可达（登记）。
- **`ShowHp` 的 53 广播**：`ShowHp` 由技能「心灵启示」置位，技能层未移植 ⇒ 生产恒 false（与既有登记同口径）。
- **`GainExp` 组队分支**（`PlayObject.Base.cs:764-817`）：组队系统（M3⑦）未落地，无输入，登记。
- **`LastHiter`/`ExpHiter` 墙钟 30s/6s 过期**（D① 已登记）：仍属后续。
- **真机 100 杀**（卡 DoD②③⑤）：要独占窗口 ⇒ 等 owner 发放（起栈禁接管道）。

## 6. 门禁 / 证据

- 四门禁（隔离 target，`--offline`）：fmt rc=0 / clippy `-D warnings` rc=0 / msgcodegen rc=0（604）/
  workspace **88 suites / 793 passed / 0 failed / 18 ignored**（基线 master `5461c1b1` 独立实测 **88 / 791 / 0 / 18** ⇒ 本片 D② **+2**）。
- 证据 hash：`verify_sha256sums.py --check-extra` + `eolcheck`（见 `SHA256SUMS.txt` 与 command 输出）。
- 边界：`git diff origin/master..HEAD -- src sql MirClient` 空（oracle 冻结）。
- 未起栈 / 未占端口 / 未连 MySQL（本片判据全在构造层）。
