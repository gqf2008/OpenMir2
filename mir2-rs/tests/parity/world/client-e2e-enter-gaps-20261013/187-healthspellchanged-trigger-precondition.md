187 — 片㊵：53 帧 R 窗口的触发前置（`HP<MaxHP`）+ 机判 `check_healthspellchanged_trigger.py`

卡 `openmir2-client-e2e-enter-gaps` / m2-svr / `openmir2-worker-11` / 2026-10-14
cwd = 本证据目录；零起栈、零游戏端口（仅共享 3306 只读观察）；未动共享主检出；不 merge、不自签 review
====================================================================================================

# 0) 结论（一句话）

片㊴ 的修复（head `c61b9627a`，已合入 master `24725dfd5`）**正确**，但本卡剩下的 R 窗口
**缺一条必要条件**：**本 R 窗口（进图后就地不动、同一 socket）内** 53 `SM_HEALTHSPELLCHANGED` 由
**周期恢复段**发出，其**前置为 `HP<MaxHP`**；满血进图时 53 永不发 ⇒ 窗口既证不了修复、
也复现不了回归。本片把该前置与机判补齐（**L 级：文档 + 夹具工具，零生产代码**），并给出「改坏必红」的两向牙。

> 措辞边界（片㊶ 收紧，审查 reviewer-5 O-1 / reviewer-6 O-2）：**不要**读成「53 **只**在 `HP<MaxHP` 时发出」；
> oracle 另有不依赖该前置的发点，见 §2 的边界表。

# 1) 独立复核：修复本身逐字节成立（作者侧只读各读一遍）

三侧锚点（本片逐一重读，非采信转述）：

| 侧 | 文件:行 | 内容 |
| --- | --- | --- |
| 冻结客户端 | `MirClient@c61d912d` `Source/Common/Grobal2.pas:39` | `HIGHHP = True;` |
| 冻结客户端 | `Source/Common/Grobal2.pas:3515-3519` | `TCharDesc = packed record Feature: Integer; Status: Integer; StatusEx: Integer;` ⇒ **12B** |
| 冻结客户端 | `Source/MirClient/ClMain.pas:10220-10231` | 53 支 `DecodeBuffer(body, @desc, SizeOf(TCharDesc))` → `HP=desc.Feature / MP=desc.Status / MaxHP=desc.StatusEx`（**不读 12B 头**） |
| oracle | `src/M2Server/Actor/BaseObject.cs:700-710` | `HealthSpellChanged()` → `RM_HEALTHSPELLCHANGED`（`0,0,0,0`） |
| oracle | `src/M2Server/Player/PlayObject.Message.cs:1736-1737` | `SM_HEALTHSPELLCHANGED` ← `MakeMessage(…, WAbil.HP, WAbil.MP, WAbil.MaxHP)`（**header-only**，`SendSocket` 无尾） |
| Rust | `mir2-rs/bins/game-svr/src/smframe.rs:265-273` | `sm_healthspellchanged_client()` = 头 + `edcode::encode([HP i32][MP i32][MaxHP i32])` ⇒ 与 `TCharDesc` 逐字段一致 |
| Rust | `smframe.rs:219-221` | `sm_healthspellchanged()` = header-only（env=0 对拍口径，保留） |

⇒ 尾布局 `[HP][MP][MaxHP]` 小端 i32、EDCode 编码，与冻结客户端逐字段一致；oracle 的 header 四个字段
（`Recog/Param/Tag/Series = ActorId/HP/MP/MaxHP`）Rust 亦一致。修复成立。

# 2) 本片真正补的洞：R 窗口没有「保 `HP<MaxHP`」这条前置

**本 R 窗口的场景** = 进图后**就地不动、同一 socket、不战斗**；该场景下 53 由**周期恢复段**发出，
其守卫是 `HP < MaxHP`（`BaseObject.Base.cs:44-70`：`(HP<MaxHP && HealthTick>=HealthFillTime)` /
`(MP<MaxMP && …)` → `HealthSpellChanged()`）。Rust 生产选路 `sm_healthspellchanged_frame` 的三个
调用点也都只在**血量/魔法变化**时触发 —— **满血满魔就地不动 ⇒ 永不发 53**。

**边界表（措辞收紧，片㊶；这些发点不依赖上面的守卫，但不在本窗口场景内）**：

| 发点（oracle） | 守卫 | 说明 |
| --- | --- | --- |
| `BaseObject.Base.cs:44-70`（恢复段） | `HP<MaxHP` / `MP<MaxMP` | **本窗口的 53 来源**（T1 前置取 `hp<max_hp`） |
| `BaseObject.Base.cs:109` | `HealthTick<0 && HP>1` | 饥饿掉血；守卫是 `HP>1`，可在 `HP==MaxHP` 触发 |
| `BaseObject.Base.cs:244` | `needRecalc` 且 `HP>MaxHP` | 重算后钳到 `MaxHP-1` |
| `BaseObject.Base.cs:628` | — | `RM_MAGSTRUCK` 受击 |
| `BaseObject.cs:973-975` | — | `IncHealthSpell`（升级/属性重算），**无条件**调 `HealthSpellChanged()` |

⇒ T1 取 `hp<max_hp` 是本「进图不动」窗口的**充分前置**（有意更严），不是对全部 53 发点的等价刻画。

**既有 7 份真机 capture 的 53 计数（`ident==53`，本片现场重算）**：

```
capture-client-default-oracle40.jsonl       53count=0
capture-client-run5.jsonl                   53count=0
capture-client-switch58.jsonl               53count=0
capture-client-viewrange12-seed42.jsonl     53count=0
capture-client-viewrange5-seed42.jsonl      53count=0
capture-run-ability-after-client58.jsonl    53count=0
capture-run-ability-before-oracle40.jsonl   53count=0
```

⇒ 此前所有窗口都是**满血进图**（如 `52/752 入图输入 … hp=19 max_hp=19`），53 一条都没发过，
所以片㊴ 的缺陷在 window 前**不可见**（与 `183-…txt` §2.1 一致）。而 `client-e2e-window.sh` 与
`check_rust_window_preflight.py`（R1–R6）**都没有**这条前置 ⇒ 若照旧脚本重跑，窗口仍然覆盖不到 53。

**反例（root verify2，缺陷暴露的那次）**：`E:/tmp/client-e2e/stack2/game-svr.out.log:132`

```
07:01:43.222 [DBG] 52/752 入图输入 ChrName=aaa layout=client-58 tail_len=58
  WAbil[level=1 … hp=18 mp=15 max_hp=19 …] …
```

即**只要进图 `hp=18 < max_hp=19`，53 才会到点发出**（root 截图 `06run_6.png` = HUD `0/0`）。

# 3) 交付：机判 `check_healthspellchanged_trigger.py`（+ 运行手册 §2b）

新增 `check_healthspellchanged_trigger.py`（本目录，只读、零端口）：

* **T1** `--log`：至少一条 `52/752 入图输入` 的 `WAbil[...]` 满足 `hp < max_hp`（触发前置）；
* **T2** `--capture`：s2c 至少 `--min-53`（默认 1）条 `ident==53`（53 真发出）；
* **T3** `--capture`：53 的 `body_len == --expect-body-len`；缺省由日志 `layout=` 推得
  （`client-58 ⇒ 12`、`oracle-40 ⇒ 0`），即**冻结客户端 `TCharDesc` 尾 vs oracle header-only** 的机器判定。

退出码 `0`=GREEN / `1`=RED / `2`=PREREQ_RED（没输入、文件缺、日志无入图行、不可解析）。

**selftest（本片现场，8/8；片㊶ 收紧文案后为 9/9）**：

```
[PASS] selftest green-client58               rc=0 (want 0)
[PASS] selftest green-oracle40-derived       rc=0 (want 0)
[PASS] selftest red-precondition-full-hp     rc=1 (want 1)   ← 满血进图 ⇒ T1 RED
[PASS] selftest red-53-absent                rc=1 (want 1)   ← 53 没发 ⇒ T2 RED
[PASS] selftest red-53-header-only           rc=1 (want 1)   ← 改坏必红：body_len=0 ⇒ T3 RED
[PASS] selftest red-53-mixed-lens            rc=1 (want 1)
[PASS] selftest red-min53-2                  rc=1 (want 1)
[PASS] selftest green-explicit-0             rc=0 (want 0)
[PASS] selftest green-log-only-t23-skipped   rc=0 (want 0, msg~='INFO T2/T3 NOT CHECKED')  ← 片㊶ 新增（reviewer-6 O-1）
SELFTEST OK (9/9)   ← 片㊵ 时为 8/8
```

**真机数据上现场跑（只读）**：

```
$ python -I -B check_healthspellchanged_trigger.py --log E:/tmp/client-e2e/stack2/game-svr.out.log \
      --capture capture-run-ability-after-client58.jsonl
OK  T1 53 触发前置：1/2 条入图行 hp<max_hp（首条 hp=18<19 max_hp=19）
RED T2 s2c 里 `53 SM_HEALTHSPELLCHANGED` 只有 0 条 < 要求 1：53 未触发/未下发
VERDICT RED   (rc=1)
```

（T1 GREEN = 该次日志确有 `hp<max_hp`；T2 RED = 该 capture 是另一次满血窗口的旧件，没 53 —— 正是本片要堵的洞。）

# 4) 「改坏必红」（两向）

| 方向 | 造法 | 期望 | 覆盖 |
| --- | --- | --- | --- |
| **改回**（回归侧） | 用修复前二进制（master `8e491f81e`，53 header-only）+ `HP<MaxHP` 进图 | 53 `body_len=0` ⇒ **T3 RED**；客户端 HUD `0/0`、`CM_RUN(3013)=0` | `red-53-header-only` 牙 + root verify2 既有实测 |
| **修好**（正向侧） | master `24725dfd5`+（`MIR2_CLIENT_ABILITY_LAYOUT` 默认 client-58）+ `HP<MaxHP` 进图 | 53 `body_len=12` ⇒ **T3 GREEN**；HUD `19/19`、`CM_RUN>=1` | `green-client58` 牙 |
| 前置缺失 | 满血进图 | **T1 RED**（阻止「没覆盖也判绿」的假绿） | `red-precondition-full-hp` 牙 |

# 5) 运行手册改动（`82-two-side-window-runbook.md`）

* §0 追加「片㊴ 前置」告警 + ①② 判据行（`check_healthspellchanged_trigger.py`）；
* §1 前置加第 5 条（`characters_ablity.Hp < MaxHP`）；
* **新增 §2b**：给出压血 SQL（`characters_ablity` 主键 `PlayerId` = `characters.Id`；HP 列在
  `characters_ablity.Hp`/`MaxHP`，`sql/mir2_db.sql:6073`，**不在** `characters`）+ 机判命令 + 改坏必红；
* §7 登记该前置已收口。

# 6) 边界与残留

* 生产代码**零改动**：本片只加 1 个只读判据脚本 + 1 份证据 + 运行手册/README 文案；
  `src/`(含 BotSrv) `sql/` `MirClient/` `mir2-rs/bins/` `mir2-rs/crates/` `*.toml` 空（见 188-boundary）。
* 未起栈、未占游戏端口；未动共享主检出；未 merge、未自签 review。
* 残留（归 root 窗口，不属本片）：真机 HUD/`CM_RUN`/`af` 读数与 C# 同点位 s2c 对照仍要 root 独占窗口；
  本片只是让那次窗口**必然覆盖** 53 路径。

# 7) 片㊶ 追补：审查 Observation 收口（2026-10-14）

| 来源 | 条目 | 处理 |
| --- | --- | --- |
| reviewer-5 O-1 | 「53 只在 `HP<MaxHP` 时发出」措辞比 oracle 强 | 已收紧：本文档 §0/§2 + 工具 docstring + `82-…runbook.md` §0/§1/§2b → 「本窗口内 53 由周期恢复段发出，其前置 `HP<MaxHP`」，并列出非该前置的 oracle 发点（§2 边界表） |
| reviewer-5 O-2 | 缺省 `body_len=12` 在未知 layout 时 | 仅记录（fail-closed 偏 RED 侧、不产生假绿）；工具 docstring 新增「已知边界」，未改行为 |
| reviewer-6 O-1 | 只给 `--log` 时 T1 过即静默 GREEN | 已改：无 `--capture` ⇒ 显式打印 `INFO T2/T3 NOT CHECKED（未给 --capture）`；selftest 新增 `green-log-only-t23-skipped` 文案牙（8/8 → 9/9），删掉该 INFO ⇒ 必红 |
| reviewer-6 O-2 | 同 reviewer-5 O-1 | 同 O-1 |
| reviewer-6 O-3 | T3 只判尾长不判内容 | 仅记录（内容由片㊴ `smframe` 单测 + 金标准覆盖）；工具 docstring 新增「已知边界」，未扩判据面 |

本条为 **L 级**（文档 + 只读夹具工具），判据 = `--selftest` 9/9 + 改坏必红（删 INFO ⇒ `green-log-only-t23-skipped` FAIL）。
