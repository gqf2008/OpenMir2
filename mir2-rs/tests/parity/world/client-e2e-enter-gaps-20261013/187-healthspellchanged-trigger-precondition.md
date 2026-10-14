187 — 片㊵：53 帧 R 窗口的触发前置（`HP<MaxHP`）+ 机判 `check_healthspellchanged_trigger.py`

卡 `openmir2-client-e2e-enter-gaps` / m2-svr / `openmir2-worker-11` / 2026-10-14
cwd = 本证据目录；零起栈、零游戏端口（仅共享 3306 只读观察）；未动共享主检出；不 merge、不自签 review
====================================================================================================

# 0) 结论（一句话）

片㊴ 的修复（head `c61b9627a`，已合入 master `24725dfd5`）**正确**，但本卡剩下的 R 窗口
**缺一条必要条件**：53 `SM_HEALTHSPELLCHANGED` **只在 `HP<MaxHP` 时**被发出；满血进图时
53 永不发 ⇒ 窗口既证不了修复、也复现不了回归。本片把该前置与机判补齐（**L 级：文档 + 夹具工具，
零生产代码**），并给出「改坏必红」的两向牙。

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

53 的三个发点（Rust 生产选路 `sm_healthspellchanged_frame`）都只在**血量/魔法变化**时触发；
oracle 侧对应 `HealthSpellChanged()` 的调用点是 `BaseObject.Base.cs:55/68/109/244`（`Run()` 体力/魔力段）
与 `BaseObject.cs:974`（`IncHealthSpell`）等 —— **满血满魔时就地不动 ⇒ 永不发 53**。

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

**selftest（本片现场，8/8）**：

```
[PASS] selftest green-client58               rc=0 (want 0)
[PASS] selftest green-oracle40-derived       rc=0 (want 0)
[PASS] selftest red-precondition-full-hp     rc=1 (want 1)   ← 满血进图 ⇒ T1 RED
[PASS] selftest red-53-absent                rc=1 (want 1)   ← 53 没发 ⇒ T2 RED
[PASS] selftest red-53-header-only           rc=1 (want 1)   ← 改坏必红：body_len=0 ⇒ T3 RED
[PASS] selftest red-53-mixed-lens            rc=1 (want 1)
[PASS] selftest red-min53-2                  rc=1 (want 1)
[PASS] selftest green-explicit-0             rc=0 (want 0)
SELFTEST OK (8/8)
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
