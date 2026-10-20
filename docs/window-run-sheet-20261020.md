> **已被 v2 取代**：`docs/window-run-sheet-20261021.md` 完整取代本文件；本文件只保留作评审对照，不再作为开窗依据。

> ⚠ **本文件是草稿：评审判定「当前版本不可直接开窗」**，请先读 `docs/window-run-sheet-review-20261020.md`（P0 阻断 A1–A5、
> 场景构造 B1–B12、判读工具 C1–C9、分组 D1–D4、授权 E1–E4）。修正后的版本才可作为开窗依据。

# 真机窗口 Run Sheet（OpenMir2，基线 `3bdfa0888`，2026-10-20）

> 性质：**计划件**。本文件只汇总「开一次窗口能采到什么、怎么采、先采什么」，不表示已执行；
> 全程遵守 `AGENTS.md` 判据分级：R 级卡未见真机证据前保持 `in-progress`，不得用「语义等价」替代。

## 摘要（≤1200 字）

基线 `3bdfa0888`。看板 in-progress 135 张，其中 **87 张带 tier=R**（23 张 issue 显式 `tier:R`，其余按 title/work 判 R）；
`whitelist.md` 另有 **7 条 `window_required`**：T-78/81/83/84/85/89/94。余量归并为 11 组：G0 前置、G1 登录/入图帧序、
G2 装备/技能/行会登录态、G3 战斗/AOI/怪物、G4 法术/延时帧、G5 持久化与存档、G6 DB 故障、G7 网关/会话生命周期、
G8 内容启用的周期状态（PL-2/3/4）、G9 RNG 账本/构造段/mapcell（整包 W0/W9）、G10 M3 夹具链、G11 长跑压测。
**一次窗口最多复用 G1+G2+G3+G5+G7**：同一 C#/Rust 栈、同一角色 `aaa`、同一地图 `0`，只改 DB 摆位/装备后重登；
C# 与 Rust 不能同时占 5000/7000/7100/7200，必须串行。硬前置：六件重编 + `--record-provenance`、guard
`WINDOW_GUARD_OK`、C# 抓包前置 `RESULT: GREEN`、九游戏端口 DOWN、无 `cargo/rustc/claude` 写手。
普通窗口**不能**收口的：G8（内容当前全部未启用，需 owner + 影子内容）、G9（O1–O7 未裁决，C# 时钟注入未落）、
G10（多项 PREREQ/夹具未就绪）、G11（需独占长窗）。**最小可行窗口 4 组**：G1（约 40 分钟/两侧）、G2（约 20 分钟/侧）、
G3（约 60 分钟/侧）、G5（约 30 分钟/侧）；若只剩 2 小时，做 G1 + G3 + G5 的 shutdown 段，可先取得
T-80/52-752、登录数值、怪物 `SM_HIT`、AttackTarget/AOI、存档往返等约 25–30 张 R 卡的窗口证据；其余仍按 R 保持
`in-progress`。

---

## 0. 输入、口径与工具清单

### 0.1 输入来源（均已只读核对）

- `walgit collab board --format json`：135 张 `in-progress`；15 张 review 带 `window_items`。
- `mir2-rs/tests/parity/whitelist.md`：`window_required` 7 条（T-78/81/83/84/85/89/94）+ D-12（关服存档待裁决）。
- `docs/window-shrink-run-20261019.md`：上一轮缩窗 W0-1..W0-5 的脚本、判读口径与已知缺陷（F1 52/752、F2 `-AttackTurn`、F3 D-9）。
- `docs/window-owner-decisions-20261018.md`：O1–O7 与缩窗选择。
- `mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/82-two-side-window-runbook.md`：§2d W0-00..W0-8、§2f 总表。

### 0.2 可用工具（现存，不需新写）

| 工具 | 路径 | 用途 |
| --- | --- | --- |
| 端口基线 | `E:/MirServer/port-status.ps1` | 十端口实拍（九游戏端口 DOWN / 3306 UP） |
| 窗口前置门 | `mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/client-e2e-window.fixed.sh.txt guard` | R1..R8，末行 `WINDOW_GUARD_OK` |
| Rust 前置 | 同目录 `check_rust_window_preflight.py` | 手跑必须显式 `--target-dir/--provenance`（否则 R7 假绿） |
| C# 抓包前置 | 同目录 `check_csharp_capture_preflight.py` | 影子 RunGate conf、部署件身份，`RESULT: GREEN` |
| 起栈 | `tools/m2-acceptance/start_rust_stack.ps1` | `-RustBinDir/-Seed/-StateLog/-RngLog/-IsolateAttack/-LogDir/-Stop` |
| C# 起栈/抓包 | `tools/capture/capture_baseline.ps1` | 影子网关 + 代理 + 真客户端全流程（会全停/恢复） |
| C# RNG/时钟钩子 | `mir2-rs/tests/parity/m3/start-csharp-hook.ps1` | `-Seed/-StateLog/-IsolateAttack/-LogDir/-Shadow` |
| 代理 | `tools/capture/mir2_proxy.py` | `--out <dir> --map 7200=127.0.0.1:7600`（可多 `--map`） |
| 帧切分/导出 | `tools/capture/segment_frames.py`、`tools/m2-acceptance/export_s2c.ps1` | `chunks.ndjson`→`frames.ndjson`→`capture.jsonl` |
| 真客户端驱动 | `tools/client/mir_flow.ps1` | 登录/选角/进图/走路/小退/再进/建角；`-AttackClicks/-AutoPlaySec` |
| 像素点击 | `tools/client/mir_click.ps1` | 截图 `-Shot`、右键 `-RightClick x,y`、左键 `-LeftClick x,y`；替代已知失效的 `-AttackTurn` |
| 帧判据 | `check_client_s2c_frames.py`、`check_attack_observability.py`、`check_healthspellchanged_trigger.py`、`check_aoi_visibility_diag.py`、`check_reverse_point_clean.py`、`aoi_visibility_pair.py`、`check_client_run_frames.py` | 均在 `client-e2e-enter-gaps-20261013/` |
| DB 快照/字段级 diff | `tools/dbsnap/dbsnap.py snap/diff/restore`、`mir2-rs/tests/parity/m3/scoped_diff.py`、`tools/m2-acceptance/check_db_paths.ps1` | 快照、抽取、`--canonical` diff |
| 攻击/技能对拍 | `mir2-rs/tests/parity/m3/run_attack_pair.ps1`、`run_bag_equip_pair.ps1` | 双侧固定攻击/背包序列；`-DiffOnly/-SelfCheckNoStack` 可离线 |
| M3 夹具 | `run_npc_shop_pair.ps1`、`run_quest_pair.ps1`、`run_party_trade_pair.ps1`、`run_guild_castle_pair.ps1`、`run_hero_pet_pair.ps1`、`run_selloff_pair.ps1`、`drop-exp/run_drop_exp_pair.ps1` | 各系统双侧同序列 |
| RNG 账本 | `m3/rng-trace-scene/run_rng_trace_pair.ps1`、`m3/rng-scene/run_rng_scene_pair.ps1`、`rng_trace_diff.py`、`check_window_batch.py` | 两侧 `seq/site/kind/arg/result` 与 `groupdiff` |
| 压测/长跑 | `tools/botload/run_bots.ps1`、`load_gate.ps1`、`load_report.py`、`tools/m5-acceptance/*` | M5/M3 长跑 |

### 0.3 共用前置（G0，所有组必须先过）

```powershell
# ① 十端口基线
powershell -NoProfile -ExecutionPolicy Bypass -File E:/MirServer/port-status.ps1
# 期望：缺失端口 = 5000,5100,5500,5600,5700,6000,7000,7100,7200；仅 mysqld:3306

# ② Rust 窗口前置
bash mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/client-e2e-window.fixed.sh.txt guard
# 期望：WINDOW_GUARD_OK

# ③ C# 抓包前置
python -I -B mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_csharp_capture_preflight.py `
  --repo E:/Users/gxh/Documents/GitHub/OpenMir2 --mirserver E:/MirServer `
  --shadow-config E:/tmp/window/cliente2e/shadow/run-gate/config.conf
# 期望：RESULT: GREEN
```

Rust 六件重编（任何新提交后必须重跑，禁止编译失败后录出处）：

```bash
cd E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs && CARGO_TARGET_DIR=E:/tmp/m2svr-window-target \
  cargo build --offline -p game-svr -p game-gate -p login-srv -p login-gate -p sel-gate -p db-srv && \
python -I -B mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_rust_window_preflight.py \
  --repo E:/Users/gxh/Documents/GitHub/OpenMir2 --ref HEAD --target-dir E:/tmp/m2svr-window-target \
  --record-provenance E:/tmp/m2svr-window-target/window-provenance.json
```

DB 备份/还原（每趟前必须做，任何写库都可回滚）：

```powershell
python -I tools/dbsnap/dbsnap.py --password= snap --out E:/tmp/window/db/before
# 收窗还原：python -I tools/dbsnap/dbsnap.py --password= restore --in E:/tmp/window/db/before
```

---

## 1. 分组 Run Sheet

> 每组 = 一次「同栈、同角色、同点位」可连续采完的最小批次。**C# 与 Rust 必须分两趟**（端口互斥）。
> 共用拓扑：真 RunGate(7200) 停 → 影子 RunGate(7600，cp936 只改 `Count=1/GatePort1=7600`）→
> `mir2_proxy` 占 7200；角色固定 `mir2test/aaa`、地图 `0`（除特别说明）；每趟起栈前重跑摆位 SQL。

### G1 登录/入图帧序基线（无装备、无技能、无会）

> 顺序：**先 C#（oracle 基线），再 Rust**；Rust 侧必须与 C# 同一点位、同一账号/角色。

- **覆盖卡**：`openmir2-login-frame-order-52-752`、`openmir2-login-numeric-gaps`、`openmir2-smframe-minor-gaps`、
  `openmir2-sm-merchantsay-recog`、`openmir2-login-useitems-magic`、`openmir2-sm-updateitem`、`openmir2-sm-additem-tail`、
  `openmir2-login-notice-clienttick`、`openmir2-outbound-encoders-red-batch`、`openmir2-orphan-encoder-wiring`、
  `openmir2-window-collector-preflight`；部分 `openmir2-healthspell-emission-model`、`openmir2-guild-g2-dispatch`（750）。

- **场景构造**：
  1. `characters` 钉 `MapName='0', CX=296, CY=653`；`characters_ablity.Hp = MaxHP - 2`（53 前置）。
  2. 清空该角色 `characters_item`、`characters_magic`（基线必须 13 空槽、无技能）。
  3. 新号数值项用 `tools/client/account_provision.ps1 -Account <new> ...` + `mir_flow -CreateChar -CharName <new>`；
     新号首登后采 52 的 `Weight=10 / MaxExp=10`，再首杀采 44/45/52/752 序。
- **驱动**（每侧）：
  ```powershell
  # Rust 侧
  powershell -File tools/m2-acceptance/start_rust_stack.ps1 -RustBinDir E:/tmp/m2svr-window-target/debug `
    -Seed 42 -LogDir E:/tmp/window/cliente2e/rust/stack
  python tools/capture/mir2_proxy.py --out E:/tmp/window/cliente2e/rust/proxy --map 7200=127.0.0.1:7600
  powershell -File tools/client/mir_flow.ps1 -Account mir2test -Password mir2pass `
    -OutDir E:/tmp/window/cliente2e/rust/shots -WalkClicks 0 -SkipReenter -SkipLogout -SkipTalk
  powershell -File tools/m2-acceptance/export_s2c.ps1 `
    -ProxyDir E:/tmp/window/cliente2e/rust/proxy -OutDir E:/tmp/window/cliente2e/rust/s2c
  ```
  C# 侧同拓扑；或直接用 `tools/capture/capture_baseline.ps1 -WalkClicks 0 -KeepShadow`（会全停/恢复）。
- **判读**：
  ```powershell
  cd mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013
  # Rust（client-58）：--expect-52-body-len 58；C#（oracle-40）：--expect-52-body-len 40 或省略
  python -I -B check_client_s2c_frames.py --capture E:/tmp/window/cliente2e/rust/s2c/capture.jsonl `
    --expect-52-body-len 58 --min-appearance-pairs 0
  python -I -B check_healthspellchanged_trigger.py `
    --log E:/tmp/window/cliente2e/rust/stack/game-svr.out.log `
    --capture E:/tmp/window/cliente2e/rust/s2c/capture.jsonl
  python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/m2-acceptance/s2c_diff.py `
    --rust <rust capture.jsonl> --golden <csharp capture.jsonl> `
    --start-ident 658 --upto-ident 46 --out E:/tmp/window/s2c-g1.json
  ```
- **预期输出**：
  - `enter_seq = 658,51,50,41,213,11029,42,766,54,5008,1106,52,752,621,211,811,46`（无装备时为 `52,752,811,46`；有武器/技能见 G2）。
  - `52` body_len=58；`53` body_len=12（HP<MaxHP 时）；无 `45`（新号首杀不得伪升级）。
  - 有会角色：`750` 恰 1 条且 `46 < 750 < 654`；无会：0 条。
  - `check_client_s2c_frames.py` GREEN；`s2c_diff.py` 0 差异或已登记。**注意**：`52` 的 body
    在 C# 是 40B、Rust 是 58B（D-9 已裁决），逐字节对拍该帧必红，按 D-9 排除，不当作新差异。

### G2 装备/技能/行会登录态（同一登录路径，改 DB 后重登）

- **覆盖卡**：`openmir2-delmagic-addmagic-chain`、`openmir2-oracle-humitems-align`、`openmir2-login-frame-order-52-752`（621/211）、
  `openmir2-persist-useitems-wiring`（装载半）、`openmir2-guild-g2-dispatch`（750/42 尾标）；
  部分 `openmir2-sm-updateitem`/`openmir2-sm-additem-tail`。
- **场景构造**（SQL 示例；`@pid` = `characters.Id`）：
  ```sql
  SET @pid=(SELECT Id FROM mir2_db.characters WHERE ChrName='aaa');
  DELETE FROM mir2_db.characters_item WHERE PlayerId=@pid;
  INSERT INTO mir2_db.characters_item(PlayerId,Position,MakeIndex,StdIndex,Dura,DuraMax) VALUES
    (@pid,0,900001,97,5000,5000),         -- 槽 0 布衣(男)
    (@pid,1,900002,207,5000,5000),        -- 槽 1 木剑（发 621）
    (@pid,3,900003,298,5000,5000),        -- 槽 3 金项链（HumItems 交换点）
    (@pid,4,900004,192,5000,5000),        -- 槽 4 青铜头盔（HumItems 交换点）
    (@pid,7,900005,354,5000,5000),        -- 槽 7 火焰戒指 Shape=115（发 210）
    (@pid,8,900006,355,5000,5000);        -- 槽 8 治愈戒指 Shape=116（发 212）
  DELETE FROM mir2_db.characters_magic WHERE PlayerId=@pid;
  INSERT INTO mir2_db.characters_magic(PlayerId,MagicId,Level,UseKey,CurrTrain)
    VALUES(@pid,1,3,'F1',0);              -- 基本剑术/火球，按目标验 211/210
  ```
  行会：影子 Envir 下建 `GuildList.txt` + `Guilds/<会名>.txt`，把 `aaa` 写成成员（部署件当前无 `GuildList.txt/Guilds/`，
  **属内容改动，需 owner 授权**）；另跑一次无会对照。
- **驱动/判读**：同 G1（登录即采）；`check_client_s2c_frames.py` 看 621/211/210/212 位置与 body；
  `s2c_diff.py` 看 750 与 42 尾串；`dbsnap.py` 对 `characters_item`/`characters_magic` 做前后字段级 diff。
- **预期**：`52,752,621,[211],811,46`；`210/212` 在 `658` 之后、`51` 之前；`Dura>0` 的戒指才出 210；
  HumItems 槽 3/4 与 oracle 一致（load 各取本槽、save 仍交换的漂移可复现）；750 尾 = `会名/封号`。

### G3 战斗/AOI/怪物（主战场：`(605,578)` + `(291,666)` + `(311,666)`）

- **覆盖卡**（本组最大）：`openmir2-monster-attack-hit-frame`、`openmir2-monster-attack-cadence-lead`、`openmir2-attacktarget-overrides`、
  `openmir2-active-seek-race-missing`、`openmir2-proper-target-unfolded`、`openmir2-search-target-clear-target`、
  `openmir2-movetomovingobject-quirk`、`openmir2-run-gap-mb1/mb3/mb4`、`openmir2-run-gap-checkdupobj`、
  `openmir2-run-gap-monster-specials`、`openmir2-run-gap-monster-master-follow`、`openmir2-run-gap-monster-state-expiry`、
  `openmir2-run-gap-corpse-ghost-revive`、`openmir2-run-gap-pr1-head-blocking`、`openmir2-rm-refmessage-delay-layer`、
  `openmir2-disappear-recipients-gridscan`、`openmir2-monster-view-throttle`、`openmir2-player-searchviewrange-cadence`、
  `openmir2-aoi-verify-maptime`、`openmir2-player-dir-supply`、`openmir2-player-drop-useitems`、`openmir2-m3-monster-active-seek`；
  部分 `openmir2-healthspell-emission-model`、`openmir2-run-gap-pl1`。
- **场景构造**：
  1. 摆位与等级（每趟起栈前重跑；C#/Rust 必须同值）：
     ```sql
     UPDATE mir2_db.characters SET MapName='0', CX=605, CY=578 WHERE ChrName='aaa';
     UPDATE mir2_db.characters_ablity a JOIN mir2_db.characters c ON a.PlayerId=c.Id
       SET a.Level=30 WHERE c.ChrName='aaa';   -- MaxHP 由登录 RecalcAbilitys 重算
     ```
     若需更高存活，再按 Recalc 后的 MaxHP 压血/回满；不要直接改 MaxHP（会被重算覆盖）。
  2. GuardList 点 `(291,666)`（卫士 `(281,661)` dir=5）；反向净空点 `(311,666)`（先 `check_reverse_point_clean.py`）。
  3. 特殊怪（race 82/119/130/91/107/105、CowKing 92、MB-1 101/95）：在**影子 Envir/MonGen.txt** 按 MonGen 头格式
     加一条同点刷怪，两侧同影子；或使用现有该怪地图/点位（需先 `check_mongen_dense_points.py` 确认）。
- **驱动**：
  ```powershell
  # 出站：不要用 -AttackTurn（F2 已知落空），用截图逐像素点怪
  powershell -File tools/client/mir_click.ps1 -Shot E:/tmp/window/g3/shot.png
  powershell -File tools/client/mir_click.ps1 -RightClick <怪格心x,y> -LeftClick <怪格心x,y> -LeftClick2 <怪格心x,y>
  # 或 mir_flow 自动挂机（命中率低但可采入站）
  powershell -File tools/client/mir_flow.ps1 -Account mir2test -Password mir2pass `
    -OutDir E:/tmp/window/g3/shots -WalkClicks 0 -AutoPlaySec 30 -AttackClicks 12 -SkipReenter -SkipLogout -SkipTalk
  ```
  入站探针：贴怪静止 10–60s；反向点/GuardList 点各重登一次。
- **判读**：
  ```powershell
  cd mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013
  python -I -B check_attack_observability.py --capture <side>/capture.jsonl --expect any --allow-unknown-self
  python -I -B check_attack_observability.py --left <rust> --right <csharp> --expect any
  python -I -B check_reverse_point_clean.py --point 311,666 --map 0 --vr 12
  python -I -B aoi_visibility_pair.py --left <rust> --right <csharp> --expect none
  python -I -B aoi_visibility_pair.py --left <rust> --right <csharp> --expect visible --min-pairs 1
  python -I -B check_aoi_visibility_diag.py --log <rust game-svr.out.log> --expect visible --min-vis 1
  ```
- **预期**：`cm_hit>=1`、出站 `sm_struck>=1`；怪普攻时每个受击者 `SM_STRUCK` 前 ~200ms 有同 `Recog` 的 `SM_HIT`，
  `SM_HIT.Series` 与 `AttackDir` 一致；反向点两侧 0 对；GuardList 两侧对称出 1 对 `SM_TURN(281,661)+SM_FEATURECHANGED`；
  入站 `sm_struck>=1`（主动族；空转=缺陷）；`check_attack_observability.py` GREEN。

### G4 法术/延时/帧面

- **覆盖卡**：`openmir2-magic-8-21-wiring`（37/8 推挤）、`openmir2-rm-delaymagic-six-ids`、`openmir2-rm-delaypushed`、
  `openmir2-spell-shared-tail`、`openmir2-spell-mp-wrapper`、`openmir2-default-arm-spell-surface`、
  `openmir2-snap-cell-fidelity`、`openmir2-buff-producer-wiring`、`openmir2-buff-system`、`openmir2-healthspell-emission-model`；
  部分 `openmir2-rm-refmessage-delay-layer`、`openmir2-login-notice-clienttick`（版本门/639）。
- **场景构造**：`characters_magic` 插入目标魔法（14/15/31/37/1/2 等，按卡）；MP 足够；目标怪/玩家在同屏；
  37 与 8 同图同种子同场景（便于逐抽对照）；A-5 buff 用越界 `|Δ|>8` 负例（零抽）+ 在界正例。
- **驱动**：**现存脚本缺法术按键驱动**（`run_attack_pair` 只发 `CM_HIT`；BotSrv `AutoSpell` 的 real wiring 属 W9）。
  可用真客户端魔法键 + `mir_flow` 的截图/消息驱动，或等 `openmir2-m3-rng-scene-driver` 的 BotSrv 接线；
  未能驱动时本组只能记构造级证据，**不得**判 R 收口。
- **判读**：`export_s2c.ps1` → `check_healthspellchanged_trigger.py`（53 尾长 12）、`check_client_s2c_frames.py`、
  `s2c_diff.py`（帧序/尾字节）；延时帧看 `sm_struck` 二次时序；推挤看逐格 `SM_BACKSTEP(9)` 条数/顺序/收件人。
- **预期**：`53` 先于 `SM_SPELL`；失败恰 1 条 `639` 且 MP/53 先行；`SM_BACKSTEP` 与 `Random(20)/Random(2)` 逐项一致；
  A-5 越界 `draws_between==0`；`check_healthspellchanged_trigger.py` GREEN。

### G5 持久化/存档往返（建角、装备耐久、三路径、关服）

- **覆盖卡**：`openmir2-persist-create-rows`、`openmir2-persist-useitems-wiring`、`openmir2-persist-single-sided-columns`、
  `openmir2-persist-item-attr-and-gold`、`openmir2-save-misc-gaps`、`openmir2-save-on-shutdown`、
  `openmir2-oracle-humitems-align`；部分 `openmir2-db-error-reply`、`openmir2-dbsrv-fault-paths`。
- **场景构造**：
  1. 新号：`account_provision.ps1` 建账号 → `mir_flow -CreateChar` → 进世界 → 小退 → 再进；检查三类行（characters/ablity/item）。
  2. 装备耐久：G2 的装备 + 攻击 20–50 次（木剑耐久下降）；记录 `characters_item.Dura`。
  3. 关服：玩家在世界中，Rust `start_rust_stack.ps1 -LogDir <同目录> -Stop`；C# 用 `E:/MirServer/start-all.ps1 -Stop`；
     抓 `DB_SAVEHUMANRCD` 与 DBSrv 日志。
- **判读**：
  ```powershell
  powershell -File tools/m2-acceptance/check_db_paths.ps1 -Mode snap -Path softclose -Phase before `
    -Prefix g5 -SnapDir E:/tmp/window/db/g5-before -OutDir E:/tmp/window/db/g5
  # 动作后 after；对侧同样；再：
  powershell -File tools/m2-acceptance/check_db_paths.ps1 -Mode diff `
    -CsharpDir E:/tmp/window/db/csharp -RustDir E:/tmp/window/db/rust -Canonical
  # 或 scoped_diff.py diff --canonical --fixed-newchr <csharp> <rust>
  ```
- **预期**：`scoped_diff.py` 字段级 diff=0（`--canonical` 只归一化墙钟列）；新号三类行齐备；
  装备槽/耐久/`characters_item_attr` 与 oracle 一致；关服 `DB_SAVEHUMANRCD` 有应答且落库；D-12 的「oracle 是否真落库」有结论。

### G6 DB 故障/降级（破坏性，放在窗口最后）

- **覆盖卡**：`openmir2-dbsrv-fault-paths`、`openmir2-db-error-reply`、`openmir2-selgate-reconnect`（DB 半）、
  `openmir2-save-on-shutdown`（失败路径）；相关 `LB-08/09/10`。
- **场景构造**：先 `dbsnap.py snap` 全库；起栈并登录；停 MySQL；执行取档/存档/建角；
  观察 6000/5100 降级帧（1100、520/524/522/525）、客户端表现、建角回滚；再启 MySQL。
  ```powershell
  Get-Service -Name *maria*,*mysql* | Stop-Service -Force   # 或 Stop-Process -Name mysqld
  powershell -File tools/client/mir_flow.ps1 -Account mir2test -Password mir2pass `
    -OutDir E:/tmp/window/g6/shots -WalkClicks 0 -CreateChar -CharName g6new -SkipLogout
  Get-Service -Name *maria*,*mysql* | Start-Service
  ```
- **驱动/判读**：真客户端 + `mir2_proxy`（若抓包拓扑可用）；DBSrv 日志 + MySQL 慢/错误日志；
  `dbsnap.py diff` 检查「不写 0」与整次回滚；`scoped_diff.py` 对 9 表 + `characters_indexes`。
- **预期**：降级帧与 oracle 逐字段一致；失败时**不写 0**；建角失败整次回滚、无半套行；MySQL 恢复后无需重启网关即可继续。

### G7 网关/会话生命周期（可与 G1/G3 并行走第二客户端）

- **覆盖卡**：`openmir2-gate-idle-timeout`、`openmir2-selgate-reconnect`、`openmir2-gategate-slots-gm`、
  `openmir2-gategate-speed-rules`；部分 `openmir2-window-collector-preflight`（种子断言）。
- **场景构造**：
  1. **60s 空闲**：登录到选角/进图后挂起；LoginGate 连接年龄 60s、SelGate 仅 100..103 刷新；记录踢前/后帧与 socket。
  2. **SelGate 重连**：登录后停 DBSrv 20–30s（≥2 个 10s 周期），观察 `建立链接.失败次数:[N]`/`链接超时.失败次数:[N]`；
     再启 DBSrv，发新选角/Enter，确认恢复。
  3. **GameGate GM**：混合 C# GameSvr + Rust game-gate；观察 `GM_CHECKCLIENT` 10s 节拍、`GM_RECEIVE_OK` 次数、
     >40KB 下行 50ms 停顿、`GM_STOP` 维护期。
  4. **速度规则**：0–210ms 登录窗口内发 `CM_SAY`/移动；记录丢帧/惩罚延迟；超长帧 `Drop` 回归。
- **判读**：现有**无专用网关判据脚本**；用 `mir2_proxy`（需影子 LoginGate/SelGate 端口，暂无入库脚本，属工具缺口）
  + 日志 + 截图。示例：
  ```powershell
  # 60s 空闲：登录到选角/进图后不操作，等待 65s，再看 proxy/日志/客户端是否被踢
  powershell -File tools/client/mir_flow.ps1 -Account mir2test -Password mir2pass `
    -OutDir E:/tmp/window/g7/shots -WalkClicks 0 -SkipReenter -SkipLogout -SkipTalk
  # SelGate 重连：杀 DBSrv → 等 25s → 重启 DBSrv → 再选角
  Stop-Process -Name DBSrv -Force
  Start-Process E:/Users/gxh/Documents/GitHub/OpenMir2/src/DBSrv/bin/Release/DBSrv.exe
  ```
  `port-status.ps1` 确认九端口回落。
- **预期**：60s 被踢时序与畸形 16B/18B 帧一致；SelGate 重连日志逐字；GM 节拍 10s、RECEIVE_OK 即时、无 50ms 停顿；
  0–210ms 丢帧但 count=1 不踢；`ItemSpeed/SpeedLimit` 仍恒 0/false（已知未接线，只能记录，不能判绿）。

### G8 内容启用的周期状态（PL-2/PL-3/PL-4 + 时钟）——需 owner

- **覆盖卡**：`openmir2-run-gap-pl2`、`openmir2-run-gap-pl3`、`openmir2-run-gap-pl4`、`openmir2-run-gap-pl1`（P32/Revival）、
  `openmir2-run-gap-player-run-quantification`、`openmir2-spell-shared-tail`（T-78）、`openmir2-m3-clock-injection`；
  whitelist T-85/T-89/T-94。
- **场景构造**（全部需**影子内容**，不改 `E:/MirServer` 原文件）：
  - `Envir/MapInfo.txt` 加 `DECHP/INCHP`；`Server.conf` `HungerSystem=1/HungerDecHP=1`；
    `KILLMONEXPRATE/POWERRATE` 非 100；脚本加 `SETAUTOGETEXP`/`SETOFFLINE`/`AUTOADDGAMEGOLD`；
    `MoveToHome` 用多图 `StartPoint.txt` + 红名。
  - 两侧同影子（C# `-Shadow`/影子 Envir；Rust `-GameSvrConf` 指向影子 conf，Envir 同树）。
  - 时钟：Rust `MIR2_CLOCK_MS` 可注入；**C# 半未落（O3）** ⇒ 656/周期状态只能真实等待，或先补 C# 时钟注入再开。
- **驱动/判读**：
  ```powershell
  powershell -File tools/client/mir_flow.ps1 -Account mir2test -Password mir2pass `
    -OutDir E:/tmp/window/g8/shots -WalkClicks 0 -SkipReenter -SkipLogout -SkipTalk
  # 进图后原地静止 3–10 分钟（饥饿/DECHP/倍率各 1s 周期）；NPC 脚本两态另跑
  ```
  `s2c_diff.py` + `check_healthspellchanged_trigger.py` + DB HP/MP/Exp 前后 diff；`653` 同 tick 相位、
  `800/633/634/801` 序、`708` 饥饿、`642/100` 倍率。
- **预期**：T-89 653 在 800 后、633/634 前；T-94 `GetStartPoint` 四条分叉按 oracle；P31 结算不乘倍率/不组队均分；
  P25/P26/P27 数值与帧序一致。**内容不启用 ⇒ 只能构造级替代，不能收卡**。

### G9 RNG 账本/构造段/mapcell（整包 W0 + W9）——需 owner O1–O7

- **覆盖卡**：`openmir2-ctor-barrier-a-draw`、`openmir2-ctor-draws-fold`、`openmir2-npc-ctor-draws`、
  `openmir2-window-collector-preflight`、`openmir2-m3-rng-scene-driver`、`openmir2-rng-consumer-structure-parity`（已完成但留痕）、
  `openmir2-mapcell-attributes-loading`、`openmir2-mapcell-consumer-teeth`、`openmir2-mongen-terrain-loop-live`、
  `openmir2-enter-shift-loop`、`openmir2-map-random-move-shift-loop`、`openmir2-recall-slave-shift-live`、
  `openmir2-enter-map-cell-rejected-vs-oracle`、`openmir2-transmap-maptrigger`、`openmir2-maptrigger-monster-and-door`、
  `openmir2-world-tick-expansion`、`openmir2-orphan-encoder-wiring`、`openmir2-m3-clock-injection`。
- **硬前置**：O1–O7 全部裁决（改 `src/GameSrv/**` 的 T-*、`dir=0`、C# 时钟注入、影子 Envir、不启用真实会话、
  与 ④ 同窗串行、ScenePin dump）；`run_rng_trace_pair.ps1 -Go -PinScene` 不得再 `PREREQ_MISSING`。
- **构造段基线**（命令见 runbook §2f 0.1b）：
  - `Robot.txt` 有效行=1 ⇒ 账本头 4 抽；`CHECKQUEST` 9 ⇒ 36；`MapQuest` 1 ⇒ 4；隐藏 NPC 3 ⇒ 12；
    合计 **69 抽 / 18 站点**；屏障 A `seq==17, arg=10000`。
  - 两侧 `-Seed 42` + `-RngLog`；`head -1` 均 `INJECT_OK 42`；`diff seq 1..69 kind/arg/result` 无输出。
  - route：`routes=1658 unresolved=0 added=1649`；离线复算
    `cargo test -p game-svr --bin game-svr -- --ignored --nocapture real_registry_route_cells_on_wall`。
- **场景账本**：`rng_trace_scenario.json`（图 0137、固定稻草人 (18,20)、起点 (17,20)、600 ticks）；
  `run_rng_trace_pair.ps1 -Go -Side csharp|rust -PinScene -PeerSession <s>`；先 `rng-scene/rng_trace_diff.py check` 5 条 fail-closed，
  再 `-SceneGroupDiff`（`VERDICT IDENTICAL`）；`check_window_batch.py` 与 ④ drop-exp 同窗串行。
- **mapcell/刷怪**：同窗核两侧出生坐标逐点一致（MonGen 2827 中 1072 墙格、40 首轮边界支），
  `cargo test -p game-svr --lib -- terrain_` 6 条绿；`recall_` 6 条绿；走位朝墙不移动/无 `SM_WALK`。
- **预期**：所有组 `IDENTICAL`、无 `GROUP_SET_DIFF/MISSING_EXPECTED_GROUPS/MIN_GROUP_DRAWS`；route/出生点/回召落点同坐标。

### G10 M3 逐系统夹具链（另一类独立窗口）

- **覆盖卡**：M3 ④/⑤/⑥/⑦/⑧/⑨/⑩ 的 `*-align`、`*-fixture-botsrv`、`*-wiring`、`*-gate` 卡
  （`openmir2-m3-drop-exp-align`、`m3-npc-shop-align`、`m3-quest-align`、`m3-party-trade-align`、`m3-guild-castle-align`、
  `m3-hero-pet-align`、`m3-mail-auction-align` 及其实现/夹具卡；含 `m3-shop-dialog-wiring`、`m3-castle-gold-tax`、
  `m3-party-attribution`、`m3-party-gainexp-npe`、`m3-pet-slave-*`、`m3-selloff-consignment-world`）。
- **前置**：`python mir2-rs/tests/parity/m3/fixture_chain_gate.py check`（期望 `runner_ok=10 prereq_or_skip=0 red=0`）；
  各 runner `-SelfCheckNoStack`；quest 仍受 markers/frame JSON/quest_state 三重阻断（见 `m3-quest-align` work）；
  Rust 侧交易/寄售常量缺失时 runner `-Side rust` fail-closed。
- **驱动**：同一栈内串行 `run_drop_exp_pair.ps1`、`run_npc_shop_pair.ps1`、`run_quest_pair.ps1`、`run_party_trade_pair.ps1`、
  `run_guild_castle_pair.ps1`、`run_hero_pet_pair.ps1`、`run_selloff_pair.ps1`（各 `-Side csharp` → `-Side rust`）。
- **判读**：各 runner 自带 `scoped_diff`/比较器；`attack_diff.py`、`drop_exp_diff.py`、`npc_shop_diff.py`、`quest_diff.py`、
  `party_trade`/`guild_castle`/`hero_pet`/`selloff` 的 diff；统一 `--canonical`。
- **预期**：每个 runner 两侧同序列 `diff=0`；M3-report DoD 逐项 GREEN。**本组是另一档期，不建议塞进短窗。**

### G11 M5/M3 长跑与压测（独占长窗）

- **覆盖卡**：`openmir2-m5-convergence-drills`、`openmir2-m3-final-acceptance`（DoD② 300 假人 24h / DoD④）。
- **驱动**：`tools/botload/run_bots.ps1 -Count 300 -NewAccount -Prefix m3final`（24h）；M5 72h、三档 `load_gate.ps1`、
  `stage_gray_rollback.ps1`、`start_full_rust_stack.ps1`。
- **判读**：`load_report.py`（P50/P90/P99、内存斜率）、`error_curve.py`（ERROR 不上升）、`dbsnap.py` 灰度前后 diff。
- **预期**：无 panic/掉线；ERROR 曲线不上升；DB 快照一致。**与短窗互斥，必须单独排期。**

---

## 2. 依赖与顺序（窗口内执行顺序）

| 顺序 | 动作 | 依赖 | 可合并 |
| --- | --- | --- | --- |
| 1 | G0 前置 + 六件重编/provenance | 无新提交；端口全 DOWN | 每次开窗一次 |
| 2 | DB 全量备份 + 摆位/压血 | `dbsnap snap` | 所有组 |
| 3 | **G1**（C# 先，Rust 后） | 无装备/无技能/无会；HP<MaxHP | 与 G7 的 60s 空闲探针并行（第二客户端） |
| 4 | **G2**（改 DB 后重登，不必重启栈） | G1 同栈；装备/技能/会构造 | 与 G1 同一趟栈；行会需影子内容 |
| 5 | **G3**（C# 阳性对照先） | 摆位 `(605,578)`；等级/HP 同级；`mir_click` | 与 G2 同栈；GuardList/反向点各重登 |
| 6 | **G4**（法术/延时） | 魔法行/戒指；**缺法术驱动** | 与 G3 同栈（若驱动可用） |
| 7 | **G5**（建角/耐久/三路径） | 新账号；DB 快照 | G2/G3 之后；关服放最后 |
| 8 | **G6**（MySQL 停/启、建角回滚） | 破坏性；全库备份 | G5 之后，收窗前 |
| 9 | **G7**（60s/重连/GM/速度） | 第二客户端；DBSrv 重启；混合栈 | 60s 可与 G1 并行；DBSrv 重启放 G6 |
| 10 | **G8**（内容周期） | owner + 影子 Envir/Server.conf + C# 时钟 | 独立栈，不能与 G1–G7 同内容 |
| 11 | **G9**（RNG/整包 W0） | O1–O7 + C# 时钟 + ScenePin | 独立长窗；与 ④ drop-exp 同窗串行 |
| 12 | **G10**（M3 夹具链） | fixture_chain_gate + 各 runner PREREQ | 独立窗口，逐 runner 串行 |
| 13 | **G11**（长跑） | 独占机器/24h+ | 独立档期 |

**必须串行/互斥**：C# 与 Rust 同一端口组；G8 影子内容与 G1–G7 生产内容不能同栈；G9 的种子/场景 pin 与真实会话
不能同栈；G10 使用独立账号前缀，不能与 `aaa` 的窗口数据混用。

---

## 3. 风险与前置（owner 授权清单）

| 项 | 为什么需要授权/前置 | 影响组 |
| --- | --- | --- |
| **O1–O7（整包 W0）** | 改 `src/GameSrv/**`、`dir` 覆盖、C# 时钟注入、影子 Envir、真实会话取舍、与 ④ 同窗、ScenePin dump | G9 |
| **C# 时钟注入 `MIR2_CLOCK_MS`** | 当前只有 Rust 半；C# 半未落（O3）⇒ 656/周期状态/T-78 只能真实等待或先补 R 卡 | G8/G9 |
| **影子 Envir / 内容启用** | `DECHP/INCHP/HungerSystem/KILLMONEXPRATE/POWERRATE/SETAUTOGETEXP/GameGold/GuildList` 均改内容；不得改 `E:/MirServer` 原件 | G8/G2（行会） |
| **DB 写/MySQL 停启** | 摆位、装备、技能、新号、故障注入；共享 3306，必须独占窗口 | G1–G6 |
| **影子 RunGate / LoginGate / SelGate** | 标准 game-gate 已有 runbook；LoginGate/SelGate 抓包无入库影子脚本 ⇒ G7 目前只能日志+截图，属工具缺口 | G7 |
| **特殊怪/行会内容** | 影子 MonGen 刷 race 92/101/95 等；影子 GuildList/Guilds | G3/G2 |
| **部署件身份** | C# GameSvr 固定部署件（`!deployed-from.json`）；DBSrv/LoginSrv 若 `src/` 有改动须 `dotnet build -c Release`，否则旧 DLL | 全部 |
| **任何新提交/合并** | 每次生产落地后重跑 M1/M2（mirror + 六件 + provenance），否则 R6/R7 红 | 全部 |
| **机器稳定性** | 近 60 天多次 bugcheck/内存页损坏；长跑前先 memtest/短压测 | G9/G11 |
| **已知工具缺陷** | `-AttackTurn` 落空（F2）⇒ 用 `mir_click.ps1`；`52` 必须 client-58 布局；`recog` 不可作严格判据 | G1/G3 |

### 3.1 工具缺口（开窗前需决定：补驱动 / 降级判据 / 移出本窗）

| 缺口 | 影响 | 现有替代 |
| --- | --- | --- |
| 无法术按键驱动（BotSrv `AutoSpell` real wiring 属 W9） | G4 的 37/8 推挤、延时法术、639/MP/53 只能部分构造 | 真客户端手动魔法键 + `mir_flow` 截图；或先按构造级登记，不判 R |
| LoginGate/SelGate 无入库影子抓包拓扑 | G7 的畸形 16B/18B 超时帧只能看日志/截图，不能逐字节 | 日志 + `mir2_proxy` 改造（需新脚本，属工具缺口） |
| GameGate GM/40KB 背压无混合栈启动脚本 | G7 的 GM_CHECKCLIENT/RECEIVE_OK 只能手工拼 C# GameSvr + Rust gate | 手工起进程 + 日志计数 |
| 内容影子（Envir/Server.conf/Guild/MonGen）无起栈封装 | G8/G2/G3 特殊怪需手工复制与改 conf | 手工影子目录；严格不改 `E:/MirServer` |
| C# `MIR2_CLOCK_MS` 未实现 | T-78/656/PL-3 节拍无法注入时钟 | 真实等待；或先立 R 卡补 C# 半 |
| 无通用“窗口结果 → 卡片证据”打包脚本 | 收窗后需手工把命令/输出/sha256 写线程 | `python -I tools/evidence-hash/verify_sha256sums.py write <dir>` |

---

## 4. 最小可行窗口（只开一次短窗）

**建议 4 组，按此顺序（约 3.5–4 小时，C#/Rust 各一趟）：**

1. **G1 登录/入图基线**（40 分钟/两侧）：先 C# 后 Rust；采 `enter_seq`、52/752、53、新号 Weight/MaxExp、750（若有会）。
2. **G2 装备/技能/行会登录态**（20 分钟/侧）：改 DB 后重登，采 621/211/210/212、HumItems 槽 3/4、42 尾标。
3. **G3 战斗/AOI/怪物**（60 分钟/侧）：先 C# 阳性对照 `(605,578)`，再 Rust 同点；随后 `(291,666)` GuardList、
   `(311,666)` 反向点；入站探针贴怪静止。
4. **G5 持久化/关服**（30 分钟/侧）：建角、装备耐久、`check_db_paths.ps1` 三路径、`start_rust_stack -Stop` 关服。

**若只剩 2 小时**：做 G1 + G3（只做 `(605,578)` 与 GuardList）+ G5 的 shutdown 段；G2/G4/G6/G7 留到下一次。
**能先取得收卡证据的卡（约 25–30 张 R）**：
`login-frame-order-52-752`、`login-numeric-gaps`、`smframe-minor-gaps`、`sm-merchantsay-recog`、`login-useitems-magic`、
`sm-updateitem`、`sm-additem-tail`、`healthspell-emission-model`、`monster-attack-hit-frame`、`attacktarget-overrides`、
`active-seek-race-missing`、`proper-target-unfolded`、`search-target-clear-target`、`movetomovingobject-quirk`、
`run-gap-mb1/mb3/mb4`、`run-gap-checkdupobj`、`run-gap-monster-specials`、`run-gap-monster-master-follow`、
`run-gap-monster-state-expiry`、`run-gap-corpse-ghost-revive`、`rm-refmessage-delay-layer`、`disappear-recipients-gridscan`、
`monster-view-throttle`、`player-searchviewrange-cadence`、`aoi-verify-maptime`、`player-dir-supply`、`player-drop-useitems`、
`persist-*`、`save-on-shutdown`、`oracle-humitems-align`、`delmagic-addmagic-chain`、`guild-g2-dispatch`（750）。
**仍不能收**：G8（内容未启用）、G9（O1–O7/时钟）、G10（PREREQ）、G11（长跑），以及 G4 中无法驱动的法术项。
> 收卡口径：以上只是「窗口证据已具备」；仍须独立 principal 的 `review` 判定，R 卡在 `merge_result` 后保持
> `in-progress` 直到窗口证据与复核都写入线程。

---

## 5. 附录 A：87 张 R 卡 → 组映射

> 「余量」列直接取自看板 `work` 字段（截断）；`G?` 中的 `/` 表示需两组成员或可分两次采。

| 卡 id | 组 | 余量（看板原文摘要） |
| --- | --- | --- |
| openmir2-guild-g2-dispatch | G1/G2 | 已合入 master（d797489cc，G2a=L / G2b=R）。余量=750 真机出队序等三项；G3 持久化另开 |
| openmir2-run-gap-pl4 | G8 | 已合入 master（bd5dc1bc3，tier=R）。余量=真机窗口三项（见 review.window_items）；源码牙不剥注释的 Minor 已登记 |
| openmir2-selgate-reconnect | G7 | 已合入 master（5da55b5c6，tier=R）。余量=真机窗口两项 |
| openmir2-dbsrv-fault-paths | G6 | 已合入 master（de71cc01d，tier=R）。余量=真机窗口 + LB-08/09/10 |
| openmir2-gate-idle-timeout | G7 | 已合入 master（d7dfc1055，tier=R）。余量=真机窗口两项 |
| openmir2-run-gap-pl2 | G8 | 已合入 master（41eb7fa54，tier=R）。余量=真机窗口两项 |
| openmir2-gategate-slots-gm | G7 | 已合入 master（201d64f1a，tier=R）。余量=真机窗口三项 |
| openmir2-login-numeric-gaps | G1 | 已合入 master（e114d7ff5，tier=R）。余量=真机窗口三项（见 review.window_items） |
| openmir2-oracle-humitems-align | G2/G5 | 已对齐 a84c81aae + d7ead5b2a（两侧 1:1，master 76e5f66f8）。余量=真机窗口：部署基线已变，下次开窗重抓装备槽字段级证据 |
| openmir2-run-gap-pl3 | G8 | 已合入 master（886741b00，tier=R）。余量=真机窗口（内容启用后的数值/帧序）+ T-85 登记的八条偏差 |
| openmir2-delmagic-addmagic-chain | G2 | 已合入 master（0c21b2dde，tier=R）。余量=真机窗口三项（见 review.window_items） |
| openmir2-login-frame-order-52-752 | G1/G2 | 已合入 master（447440093 + ed73a6a73，tier=R，模型 D）。余量=非 TestServer 部署/发光装备/IsSpirit 的槽位变化未取样本；621/211 的 Rust 侧还依赖 211 空槽修复（卡 openmir2-login-magic-211-em… |
| openmir2-gategate-speed-rules | G7 | 已合入 master（9f8f6f896，tier=R）。余量=真机窗口四项（登录包相位、0-210ms 内未丢帧的先后、超长帧回归、ItemSpeed/SpeedLimit 接线） |
| openmir2-healthspell-emission-model | G1/G3/G4 | 已合入 master（d8ed70cc7 + 注释订正 7c711b212，tier=R）。余量=真机窗口四项（见 review.window_items） |
| openmir2-monster-attack-hit-frame | G3 | 已合入 master（dcdcd807d，tier=R）。余量=真机窗口（同 tick/200ms 顺序三项） |
| openmir2-run-gap-pl1 | G3/G8 | PL-1 已合入 master（83beb2a2e，4 提交，tier=R）。余量=真机窗口四项（见 review.window_items）；finding 4/5 登记 |
| openmir2-monster-attack-cadence-lead | G3 | 节拍门经定量核查无差异（两侧 min 间隔 3.147/3.190s，均 >3s）；原线索降级为「起始 HP 未控」。余量=按评审口径的 HP 受控重采样（真机窗口），并入窗口项 |
| openmir2-run-gap-mb4 | G3 | MB-4 已合入 master（8b2ccbe55，tier=R）。余量=真机窗口：race 92 的 SpaceMove/MapRandomMove 落点与狂暴 500/400 节拍；CastleDoor HoldPlace 阻挡支；带装备玩家触达的 829/837/856/858 真机 t… |
| openmir2-magic-8-21-wiring | G4 | 实现 + 独立复验 CONFIRMED（318bd1d81，tier=R 不收卡）。余量：① 真机窗口（37 的 Random(20)/Random(2) 流水 + 逐格 SM_BACKSTEP，可与 8 同场景）；② 能量排斥的「37 是否可学/可放」属数据面；③ 本卡另两项既有余量不变：魔… |
| openmir2-player-dir-supply | G3 | 玩家 `Dir` 权威切换到 `Entity.ai.dir`（展示表只做投影），所有读点改读实体 已落地（c518e075b，tier=R；首轮 REFUTED → 修复 CONFIRMED）。余量：真机窗口 + T-81 登记。 |
| openmir2-player-drop-useitems | G3/G5 | 玩家死亡 `DropUseItems` 两段（`ItemDesc&8` 清槽 + 全槽 13 抽 + `GetRandomLook`；`KillTargetTrigger` 击杀者门） 已落地（c518e075b，tier=R；首轮 REFUTED → 修复 CONFIRMED）。余量：真机窗… |
| openmir2-rm-delaypushed | G3/G4 | 44 推挤的 `RM_DELAYPUSHED` 入队与投递（`char_pushed_now` + 逐格 `SM_BACKSTEP`） 已落地（c518e075b，tier=R；首轮 REFUTED → 修复 CONFIRMED）。余量：真机窗口 + T-81 登记。 |
| openmir2-rm-delaymagic-six-ids | G4 | 六魔号 `RM_DELAYMAGIC`（1/5、11、13、44、45）入队与到点结算链 已落地（c518e075b，tier=R；首轮 REFUTED → 修复 CONFIRMED）。余量：真机窗口 + T-81 登记。 |
| openmir2-save-misc-gaps | G5 | 存档杂项已落地（50dd3f2ef，tier=R）。余量：真机窗口；G11 传输面未移植；G7 读到一半失败未构造。 |
| openmir2-db-error-reply | G6 | DB 错误应答已落地（50dd3f2ef，tier=R）。余量：真机窗口（失败注入的 C# 对拍）。 |
| openmir2-persist-single-sided-columns | G5 | 单边列与 Rust-only 补齐已落地（50dd3f2ef，tier=R）。余量：真机窗口；失败移动后 `DisplayTable.dir` 未随实体（建议单卡）。 |
| openmir2-attacktarget-overrides | G3 | 10 个 AttackTarget 覆写已落地（bbf9694aa，tier=R；首轮 F1 Critical → 修复 CONFIRMED）。余量：真机窗口（82/119/130/91/107/105）；114/117/118/215 不可达；`UndeadPower` 口径另卡。 |
| openmir2-smframe-minor-gaps | G1 | 随帧面批次落地（27d22a9f3，tier=R）。余量：真机窗口 + T-79/T-80 登记项。 |
| openmir2-sm-merchantsay-recog | G1 | 随帧面批次落地（27d22a9f3，tier=R）。余量：真机窗口 + T-79/T-80 登记项。 |
| openmir2-login-useitems-magic | G1/G2 | 随帧面批次落地（27d22a9f3，tier=R）。余量：真机窗口 + T-79/T-80 登记项。 |
| openmir2-sm-updateitem | G1/G2 | 随帧面批次落地（27d22a9f3，tier=R）。余量：真机窗口 + T-79/T-80 登记项。 |
| openmir2-sm-additem-tail | G1/G2 | 随帧面批次落地（27d22a9f3，tier=R）。余量：真机窗口 + T-79/T-80 登记项。 |
| openmir2-search-target-clear-target | G3 | SearchTarget 的 ClearTarget/RemoveAt 怪癖已照抄（6a774ea79，tier=R）。余量：① 真机窗口；② 其余折入版调用点未扩大范围。 |
| openmir2-proper-target-unfolded | G3 | proper-target 折入拆开、抽数恢复（6a774ea79，tier=R）。余量：① 真机窗口（攻击已死目标的抽数流水）；② 其余折入版调用点（特殊怪覆写/显示路径）未扩大范围。 |
| openmir2-active-seek-race-missing | G3 | 9 个 seek race 已补（6a774ea79，tier=R）。余量：① 真机窗口；② 这些 race 的 `AttackTarget` 覆写属 `openmir2-attacktarget-overrides`；③ 118/130 数据不可达。 |
| openmir2-rm-refmessage-delay-layer | G3/G4 | RM_REFMESSAGE 延时层已落地（56524b74c，tier=R；复验 REFUTED 两条 → 修复 + 回归）。余量：① **真机窗口**（200ms 后二次 `Struck` 的全局帧序）；② 未接的 7 参调用点（AttackDirect/Fireball/Explosion… |
| openmir2-save-on-shutdown | G5/G6 | 关服存档已落地（296155f4e，tier=R；复验 CONFIRMED，偏离登记 D-12）。余量：① **真机窗口**（`DB_SAVEHUMANRCD` 帧与落库 diff、`-Stop` graceful 日志；并裁决 D-12 的「oracle 是否真的落库」）；② `CTRL_C… |
| openmir2-run-gap-mb3 | G3 | MB-3 已落地（f4c16f674，tier=R；首轮 REFUTED 两条偏离 → 修复 CONFIRMED）。余量：① **真机窗口**（107/105/102/112）；② **`SendStruckDelayMsg` 延时面**（归 `openmir2-rm-refmessage-d… |
| openmir2-persist-item-attr-and-gold | G5 | G-01/G-02 等价实现+登记已落地（71f4baa48，tier=R）。余量：① G-01 功能上仍不落库（oracle 门决定，`B-117`）；若要修，单开批次（upsert + 按 MakeIndex 关联 + 读全 14 列）；② G-02 仍丢失（`B-116`，schema … |
| openmir2-persist-useitems-wiring | G2/G5 | 会话装备已装载进世界实体（03208e20e，tier=R；复验 CONFIRMED）。余量：① **真机窗口**（耐久磨损/破零/存档往返）；② 破零后的 oracle 收尾未接（能力重算/`SendDelItems`/`FeatureChanged`/`RM_DURACHANGE`）；③ … |
| openmir2-persist-create-rows | G5 | G-20 兜底建角 ablity INSERT 已落地（aa32b197c，tier=R）；**G-03 经复核证伪**（oracle 同样不建 bonusability 行，按硬约束③照抄，撤回偏离与 D-11）。余量：① 真机窗口（建角/兜底建角往返）；② `name_quick`/`na… |
| openmir2-monster-view-throttle | G3 | 怪物视野已改 oracle 口径（0ebdbbf54，tier=R；复验 CONFIRMED，合并 MB-1 时人工解冲突）。余量：① **真机窗口**；② GuardUnit 的 6 格/CanPassiveAttack；③ 自扫相位差 ≤1 tick；④ parity 驱动不启 AI；⑤ … |
| openmir2-run-gap-mb1 | G3 | MB-1 四类 Run 覆写已落地（a1e387a77，tier=R；复验 REFUTED(F1 丢帧) → 发帧时刻收件人快照修复 + 两条回归用例）。余量：① **真机窗口**（race 101/95 各 ≥1、R38 抽样）；② `CellType.Event` 格子对象未落格；③ 通用… |
| openmir2-run-gap-player-run-quantification | G8 | 证据设计件已入库（e56379b9d，tier=L；两轮复验：13 条 must_fix 全修 + 复核轮 2 处落地前改正）。余量：① **裁定证据**——按 §3 方案取数（B1 静置压血看 53 填充间隔：50/s≈6.0s vs 40/s≈7.5s）；② 若判 50/s ⇒ 改 `st… |
| openmir2-run-gap-checkdupobj | G3 | CheckDupObj 重叠推挤已落地（706c95cae，tier=R；首轮 REFUTED 因 MoveToMovingObject 怪癖 → 怪癖卡先落地后 rebase + 跨卡判据）。余量：① **真机窗口**——3 人同格推挤的源/目标格身份序列 + AOI 相位（1000ms 口… |
| openmir2-run-gap-monster-state-expiry | G3 | 四项状态到期已落地（58da49071，tier=R；独立复验 CONFIRMED）。余量：① **真机窗口**（CrazyMode 色变 0xF9 / RefNameColor 帧面 / FireHit-TwinHit 到期消息）；② FireHit/TwinHit 写入点依赖未移植的技能层… |
| openmir2-run-gap-corpse-ghost-revive | G3 | 尸体 MakeGhost 双门 + 复活链 + Ghost 5min 清理已落地（b12b59ca8，tier=R；首轮 REFUTED → 三条修复 → 修复后独立复核 CONFIRMED）。余量：① **真机窗口**（3 分钟变尸 / 可复活支 ZenTime-20s / 5 分钟移除 /… |
| openmir2-run-gap-pr1-head-blocking | G3/G4 | PR-1 已落地（7e8d47426，tier=R；两轮 REFUTED → 四叉堆 + 500 场景差分 0 不一致 → CONFIRMED）。**MB-1/MB-3/MB-6/MB-9 前置解冻**。余量：① 真机窗口；② 跨 actor 泵序（首次入队序 vs 实体序）；③ 宿主 `pe… |
| openmir2-disappear-recipients-gridscan | G3 | RM_DISAPPEAR 收件人已改为离开时刻 ±12 格扫（8dea847c5，tier=R；复验 CONFIRMED）。余量：① **真机窗口**（节流窗口内进入 ±12 的观察者、离世广播与 AOI/移动帧的全局次序）；② 冷路径 vs 缓存的既有偏差；③ `ObMode/FixedHi… |
| openmir2-run-gap-monster-specials | G3 | BeeQueen/SpiderHouse 产仔 + Clone 扣主 MP 已落地（f46e4f03f，tier=R；两条 Important 收口后复验 CONFIRMED）。余量：① **真机窗口**（同 tick 产仔抽数相对次序、`SM_HIT` 帧面、Clone 扣 MP）；② `R… |
| openmir2-run-gap-monster-master-follow | G3 | 宝宝跟随 / SpaceMove / SlaveRelax / RunAway 到期已落地（209023120，tier=R；首轮 REFUTED(F1) → 已修 + 回归用例）。余量：① **真机窗口**（异图/超距/SlaveRelax 两侧流水）；② 玩家主的 `Dir` 供给另卡 `… |
| openmir2-movetomovingobject-quirk | G3 | MoveToMovingObject 怪癖已复刻（4cb9bb5dc，tier=R；复验 CONFIRMED，含 >50k 例穷举对拍）。余量：① **真机窗口**（多对象格移动时的源/目标格身份序列）；② `WorldGolden` 桩的怪物 CellType 仍硬编码 Play（若未来 d… |
| openmir2-player-searchviewrange-cadence | G3 | 玩家 AOI 节拍已对齐（9e27ac39b，tier=R；独立复验 CONFIRMED + 2 LOW 处理）。余量：① **真机窗口**——首扫约 1.2s、之后每 1.2s；② 非玩家扫描节拍仍每 tick（另卡）；③ `RM_DISAPPEAR` 收件人近似（误差窗口随节流放大）——应… |
| openmir2-aoi-verify-maptime | G3 | VerifyMapTime 30s 刷新 + viewer 过期分流 + AOI 诊断口径已落地（8b4f5df5e，tier=R；独立复验 approve）。余量：① **真机窗口**——同缩窗场景需验 vis>=1、nearest<=vr、capture 出 SM_TURN(281,661… |
| openmir2-spell-shared-tail | G4/G8 | T-78 节流修复落地（2dac5c7bb，tier=R）。余量：① 真机窗口（注入时钟下 656 帧序）；② T-78 剩余三偏差；③ `vmt`（VerifyMapTime/玩家 viewer 过期/AOI 诊断）在飞。 |
| openmir2-login-notice-clienttick | G1/G4 | B1 已落地（326c6431f，tier=R）：1018→`ClientTick`（零出帧）+ `SM_SPELL` 后接版本门（失败恰 1 条 639、MP/53 先行、+GD 照发）。**T-73** 登记 B1 非阻塞窗口等未建模面。余量：① 真机窗口（含 `SoftVersionDa… |
| openmir2-spell-mp-wrapper | G4 | 战士门与召唤护身符门 639 已接（0c4fe0e5d，tier=R）。余量：① 真机窗口（失败回帧 + MP/53/尾时序）；② 6/13/14/15/16/18/19/36 的护身符/失败门未建（14/15 的 A-5 路径连 CheckAmulet 都未建）；③ `:167` 版本门（`… |
| openmir2-enter-map-cell-rejected-vs-oracle | G9 | 实现 + 独立复验 PASS（df5a260ea，tier=R 不收卡）。余量：① 真机窗口（切图失败路径；当前生产无 `switch_map` 调用者 ⇒ 真机不可达）；② **帧面**：oracle 在失败前已发 `RM_CLEAROBJECTS`/`RM_CHANGEMAP`、切图路径的… |
| openmir2-enter-shift-loop | G9 | 实现 + 独立复验 CONFIRMED + must_fix 处理（2fa749471，tier=R 不收卡）。余量：① 真机窗口（墙格出生登录：RNG 流水 + 落点；默认零抽）；② HomeMap 不可解析分支的 oracle `goto ReGetMap` 语义未建模（当前不可达，已登记… |
| openmir2-map-random-move-shift-loop | G9 | 实现 + 独立复验 CONFIRMED + 合并重放（047709a9b，tier=R 不收卡）。余量：① 真机窗口（商人到点移动：RNG 流水 + 落点）；② 登记未做：201 轮回滚的 `DeleteFromMap`/`AddMapObject` 语义（现只 return）、落格 `Cel… |
| openmir2-recall-slave-shift-live | G9 | 判据落地 + 复验 CONFIRMED（49fe49f05，tier=R 不收卡）。余量：① 真机窗口（W0 §2e：两侧 RNG 账本 `VERDICT IDENTICAL` + 回召落点同坐标）；② 同窗登记的 `merchant_run → map_random_move` 实为**真缺… |
| openmir2-mapcell-attributes-loading | G9 | 实现 + 独立复验 + 数字/判据订正（2ff02ac0e + b192bd9f3，tier=R 不收卡）。余量：① 真机窗口（**必核**：route added=1649、刷怪位置与 RNG 流水、进图/落格/AOI/CanWalk(true)/snap）；② `castle.rs::ap… |
| openmir2-ctor-barrier-a-draw | G9 | 实现 + 独立复验 CONFIRMED + 日志/登记收尾（9a175a66f + 78416a932，tier=R 不收卡）。余量：① 真机窗口（**默认流水已变，须重取基线**；构造段账本 69 抽/18 站点）；② 宿主那一行绑定的进程级牙（需「新角色登录后首包 SM_BAGITEMS … |
| openmir2-ctor-draws-fold | G9 | fold 实现 + 陈述订正已落地（0a34719c9 + e6105f011，tier=R 不收卡）。余量：① 屏障 A 的 1 抽（ServerConf.cs:114 ItemNumber += Random(10000)）——补上后段②/段③ 与 oracle 等距，须另立片；② 真机窗… |
| openmir2-snap-cell-fidelity | G4/G9 | 实现+独立复验 CONFIRMED（befd3ad7c，tier=R 不收卡）。余量：① 真机窗口项；② 两个前置（.map 格子属性未灌进 world.maps；apply_structure_events 无生产调用方）——卡 openmir2-mapcell-attributes-loa… |
| openmir2-default-arm-spell-surface | G4 | 帧面实现已落地（`9e59246c7` + 计数订正 `888180c07`，tier=R 不收卡）。余量：① 真机逐帧对拍（窗口项）；② 效果体层未接（MP 扣减 + `RM_HEALTHSPELLCHANGED` 先于 RM_SPELL ⇒ 仍缺一帧、相对序前移；版本门无生产调用点；`RM… |
| openmir2-buff-producer-wiring | G4 | A-5 修复已落地并由独立复验 CONFIRMED（ffac59d1b，tier=R 不收卡）。余量=真机窗口项 + 新开片：四块 snap 改用忠实镜像 `cetin_near_xy`（复验新发现的格子层差异）+ 补窗口宽度上界牙。 |
| openmir2-npc-ctor-draws | G9 | 片117 + §4.4.2 处置件订正已落地（tier=R 不 closed）。余量：① owner 裁决 8 处按 F（只补抽）还是 M（建实体）；② 真机构造段对齐窗口项——**窗口前须先实测 `Robot.txt` 行数**（账本头 = 行数×4 个 RobotObject 抽），否则『… |
| openmir2-buff-system | G4 | 片112 落地（tier=R，不 closed）。余量=真机窗口项 + A-5 生产接线批。 |
| openmir2-world-tick-expansion | G9 | 片107 落地（tier=R，不 closed）。余量=真机窗口项（RNG 流水/时钟/公告节拍）。 |
| openmir2-transmap-maptrigger | G9 | 片101 已落地（tier=R，不 closed）。触发链三层齐：1658 条 route 逐字节等价 + 落格 + 踩格换图；余量=真机窗口项。 |
| openmir2-maptrigger-monster-and-door | G9 | 片110 落地（tier=R，不 closed）。余量=真机窗口项 W-R1-a/b/c。 |
| openmir2-orphan-encoder-wiring | G1/G9 | 片103 落地（tier=R，不 closed）。余量=真机窗口项。 |
| openmir2-window-collector-preflight | G0/G9 | 片98 已落地（tier=R，不 closed）。余量=真机窗口项：两侧种子交叉断言 / C# site 列 / 前置门生产路径不误拦。 |
| openmir2-outbound-encoders-red-batch | G1/G4 | WB（片91）已并入 master b08e065fc。按 tier=R **不 closed**——待窗口项：制药抽数流水 + 帧序 + MYSTATUS 派生落库。本卡（outbound 编码器 + 发帧点接线）离线面至此全部交付；剩余为真机窗口（W0 联调窗起）。 |
| openmir2-m3-monster-active-seek | G3 | 实现已合入（tier=R 不 closed）。余量：真机 W0/W9 对拍 + 登记未建模面（覆写 Run 的子类、SuperGuard/ArcherGuard 自有 pvp 扫描）。 |
| openmir2-m3-rng-scene-driver | G9 | 已合入 master 3a5855b9e980d8ab972f53bb3a40343f6c4a2d13（tier=R 不 closed）。余量：真机两侧账本对拍（W9 窗）+ BotSrv real wiring（进 0137/pin 怪/dump，属 W9 窗项）。 |
| openmir2-m5-convergence-drills | G11 | 阶段0（零窗口 L 级工具）已合入 master 2f0b102c5ec98bffd0f5a6121cf64f4bc7cb4219：①load_report --mem-tail-sec+峰值-均值（CLI 缝两向牙）②新 error_curve.py（19/19 牙，桶界/窗口牙；真实日志机… |
| openmir2-m3-selloff-consignment-world | G10 | 合并检查点（role=merge, openmir2-coordinator）：M3⑩-1 收口片 #2（@dealybme 开窗接线 + SM_MERCHANTSAY 两失败支）已合入 master 057d7dc9e（approve openmir2-reviewer-5 8d685f7f… |
| openmir2-m3-pet-slave-masterdie-wire | G10 | 合并检查点（role=merge, openmir2-coordinator）：M3⑨-3 宝宝叛变帧面 rework（reviewer-6 F-1：NetPriorityQueue .NET8 四叉最小堆逐操作复刻 + drain 按 oracle 入队序 + SM_CHANGENAMECO… |
| openmir2-m3-pet-slave-clone46 | G10 | 合并检查点（role=merge, openmir2-coordinator）：⑨-2 残余 46 分身（MagMakeClone/PlayCloneObject 宿主接线 + RM_TURN 帧）已合入 master 73df92d26（approve openmir2-reviewer-6… |
| openmir2-m3-pet-slave-lifecycle | G10 | 合并检查点（role=merge, openmir2-coordinator）：⑨-3 收口片 #2（宝宝可观测效果落地：apply_slave_level_abilitys 三类支/else 支 DC+MaxHP 银行家舍入 + Race>=Animal(50) 门；apply_derive… |
| openmir2-m3-shop-dialog-wiring | G10 | 合并检查点（role=merge, openmir2-coordinator）：⑤-4 第②片（8 条商店 CM_* 派发 + 帧：买/卖/修理/查询/仓库/详情652/仓库存取700-707，rebase 到 master 9863d57b0 的收口 head 7f0befb0f）已合入 m… |
| openmir2-m3-clock-injection | G8/G9 | 合并检查点（role=merge, openmir2-coordinator）：M3⑧ 横切前置 Rust 半（MIR2_CLOCK_MS 可注入时钟：env_clock 解析/fail-closed + CLOCK_PROBE + now_ms()/world.clock_ms 接同一注入；… |
| openmir2-m3-castle-gold-tax | G10 | 合并检查点（role=merge, openmir2-coordinator）：⑧-9 第①片（离线 crates/world 税收/金库：IncRateGold 税比/日上限/金库上限/10min SaveTick + CastleManager + 商人税判定）已合入 master 510… |
| openmir2-m3-party-gainexp-npe | G10 | 合并检查点（role=merge, openmir2-coordinator）：⑦ 收口 M-2（GainExp 组队缺态对齐 oracle NPE 吞错口径：group owner 不在注册表 ⇒ 零经验/零授奖）已合入 master 029d23eae（approve openmir2-r… |
| openmir2-m3-party-attribution | G10 | 合并检查点：本片（M3⑦-4 归属/分配：pickup 接真注册表 + 交易态接真会话 + HAM_GROUP IsGroupMember 剔除 + GainExp 组队分配）已合入 master 751a20a06（approve reviewer-6 c16a7365 @reviewed_… |

> 未纳入本表：`openmir2-run-gap-mb2`（看板 work=「车道在飞」，无窗口余量描述）；M3 ④–⑩ 的
> `*-align`/`*-fixture` 卡多为看板无显式 tier 字段，统一归 G10，不逐条列。

## 6. 附录 B：whitelist `window_required` 条目

| 条目 | 组 | 窗口判据 | 构造/命令 |
| --- | --- | --- | --- |
| **T-78** Pvp 过期/656 节拍 | G8 | 注入时钟下 656 帧序；200ms 虚拟 tick vs 墙钟；单游标 vs 每玩家 | `MIR2_CLOCK_MS`（Rust 已有，C# 未落）；两玩家 PVP 置位/清位；`s2c_diff` 看 656 |
| **T-81** 玩家死亡 DropUseItems | G3/G5 | 玩家被玩家/宝宝击杀时 `SM_DELITEMS(709)` 帧序；两轮抽数 | 两客户端同点，一人击杀；`check_attack_observability` + `s2c_diff`；DB `characters_item` diff |
| **T-83** RM_DELAYMAGIC/PUSHED 六魔号 | G4 | 14 张卡的延迟帧序（600/1200ms）与推挤落点 | 魔法行 + 两玩家/怪；`s2c_diff` 看 `sm_struck`/`SM_BACKSTEP` 到时序 |
| **T-84** 玩家 Dir 单一真源 | G3 | 被挡后下一次取目标方向；宝宝 `GetBackPosition` 落格 | 贴墙走失败 + 攻击；`mir_click` 右键转向；`s2c_diff` |
| **T-85** PL-3 八条偏差 | G8 | 内容启用后 P25/P26/P27 数值/帧序；`ShowScriptActionMsg` 197/198；G-B flush | 影子 MapInfo/Server.conf/脚本；见 G8 |
| **T-89** P22 653 同 tick | G8 | 653 在 `800` 后、`633/634` 前；玩家级/地图级封顶 | 影子 `DECGAMEGOLD/INCGAMEGOLD`；`s2c_diff` |
| **T-94** MoveToHome 会话 HomeMap | G8 | `GetStartPoint` 四条分叉；红名 `RedHomeMap`；跨图 | 影子 `StartPoint.txt` 多图 + 红名；`800/633/634/801` 落点 |
| **D-12** 关服存档 | G5/G6 | oracle `-Stop` 是否真落库；`DB_SAVEHUMANRCD` 帧 | 玩家在世界中停栈；DBSrv 日志 + `dbsnap` |

---

## 7. 收窗清单（每趟必须）

1. `start_rust_stack.ps1 -LogDir <本趟同目录> -Stop`（起/停同 `-LogDir`）；C# 按 `start-all.ps1 -Stop`。
2. `E:/MirServer/port-status.ps1`：九游戏端口全 DOWN，仅 3306。
3. DB 还原：`dbsnap restore` 或逐项写回 `MapName/CX/CY`、`Hp/MaxHP`、删除临时 `characters_item`/`characters_magic` 行、
   删除影子 Guild 文件；`git status` 干净。
4. 证据：命令 + 输出 + 路径 + sha256；capture/日志/截图落 `E:/tmp/window/<组>/<side>/`；
   报告末段列「残留物清单」。
5. 线程：`patch(base,head)` + `review`（独立 principal，写 `tier`+`tier_why`+窗口证据）+ `merge_result`；
   R 卡窗口未跑不得 `closed`。
