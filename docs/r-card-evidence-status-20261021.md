# R 卡真机证据清单刷新（2026-10-21）

- **卡片**：`openmir2-evidence-refresh`（tier=L，纯文档 + 只读）
- **基线**：`master = ce61cdca4`（建树时 `git log -1` 复核）。窗口实跑的生产代码基线是 `ab44031ce`
  （`E:/tmp/m2svr-window-target/window-provenance.json` 的 `source_head`，sha256 `563de1ac…`）；
  `ce61cdca4` 相对 `ab44031ce` **只多一个 docs 提交**，生产路径未变。
- **写集**：仅本文件（`docs/r-card-evidence-status-20261021.md`）。未改 `src/**`、`sql/**`、`MirClient/`、
  `mir2-rs/crates/{protocol,storage}`、`E:/MirServer/**`；未起服务端/客户端/代理；未占端口；未写数据库；未动主检出；未 push/合并。
- **快照时间**：2026-10-21 13:34（+08）。看板快照 316 卡 = 170 in-progress / 146 closed；
  m3 B 表 69 卡（连同表后注记共抽取 74 个 id）**全部仍为 `in-progress`**。
- **输入**（全部只读）：
  1. `docs/window-round2-result-20261023.md`（2026-10-21 受控登录第二轮实跑结果）；
  2. `docs/m3-remaining-inventory-20261022.md`（A/B/C/D 四表，B 表 = 69 张待窗口 R 卡）；
  3. `docs/remaining-plan-v2-20261021.md`（窗口/离线划分、下一窗建议）；
  4. 看板 `walgit collab board --format json`（本文件写入时快照；各卡 `last_oid` 见附录 A）；
  5. 佐证件：`docs/window-round2-runbook-20261023.md` 附录 A、`docs/window-shrink-run-20261018.md`、
     `docs/window-shrink-run-20261019.md`、`mir2-rs/tests/parity/m3/audit-monster-attack-cadence-20261019.md`。
- **口径**：
  1. 只写有**文件/卡号/提交/证据路径**可追溯的条目；查不到写 **UNKNOWN**，不猜。
  2. 「已有真机证据」既包括 **GREEN**，也包括**缺陷侧 RED 证据**（真机暴露了差异但修复后尚未复采），
     后者在「结论」列显式标注，**不得**当作已收卡。
  3. 证据路径按 Windows 绝对路径记录（`E:/tmp/...`）；`E:/tmp` 属临时物（见 `docs/tmp-cleanup-proposal-20261017.md`），
     本文件是这些路径的索引，清理前须先归档到仓库或延长保留。
  4. 判级沿用 `AGENTS.md`「判据分级」；R 卡 `merge_result` 后仍须真机窗口，未绿不得 `closed`。
  5. 证据若早于当前 master，在「还缺」列标明其证据 head；**旧 head 的 GREEN 不自动等于当前 head 的 GREEN**。

> **当前机器状态（只读实测，2026-10-21 13:30–13:31）**：5000/5100/5500/5600/5700/6000/7000/7100/7200
> **十个端口全部 LISTENING**（`Get-NetTCPConnection`；进程启动时间 13:30:29–13:31:02），3306 亦 UP。
> 即此刻有另一条栈在跑，**R8 对“开新窗”是 RED**。本文件不据此改任何卡，也不触碰该栈；下一窗必须先取得独占。

---

## 0. 结论摘要

1. **登录前缀首次拿到全绿真机证据**：第二轮受控登录 `15/15` 帧、`0` 非白名单差异（`whitelisted=5`，
   仅 `recog` 与 `52` 布局），见 `E:/tmp/window/20261023/compare.json`（`pass=true`，sha256 `f14afc49…`）。
2. **654 / 第二 42 / 100×4 的修复在真机成立**：654 恰一条且在 46 之后、字段 `self/0/500/0`；
   第二 42 两侧 body 逐字节相同；100 两侧各 4 条、`param=65499/tag=0/series=1`，正文集合相同而**顺序不同**（已登记不确定）。
3. **新缺陷 `657×2` 仍在**：C# 在 654 后发两条 `SM_CHARSTATUSCHANGED(657)`，Rust 一条都没有；
   2026-10-22（旧二进制）与 2026-10-21（新二进制，本轮实跑）**两轮一致**（`20261023` 只是场景包文件名日期）⇒ 持续缺口，非本轮回归。
   已开卡 `openmir2-login-charstatus-657`（tier=R，P1，待派工）。
4. **第二 42 的绝对位置目前被 657 缺口掩盖**：C# 序为 `654,657,657,第二42`，Rust 为 `654,第二42`；
   657 补齐后必须复采，才能判 J3 的完整相对序（`i46 < i654 < 657[0] < 657[1] < i42[1]`）。
5. **上轮 289/288 的 50/51 差异是输入漂移，不是代码分叉**：本轮两侧 `param` 均为 289；`52` 的 18 个语义字段
   已逐项比较（`52_semantic_compared=true`、`unverified=[]`），40/58 仅布局差（D-9/client58）。
6. **已有真机证据的 R 卡 11 张**（表 1，含 9 张 m3 B 表卡 + 2 张窗口派生卡）；
   **仍无真机证据的 B 表卡 60 张**（表 2，51 行，按 E 依赖分组）。
7. **下一窗最小集 = 先补 657，再只跑 G1 登录**；G2a/G3a/G5a 作为时间富余项。前置是独占端口 +
   镜像/产物/provenance 与 guard 全绿（当前端口被另一栈占用，R8 RED）。

---

## 表 1：已有真机证据的 R 卡（11 条）

> 说明：`GREEN` = 判据在该场景成立；`RED/缺陷侧` = 真机已暴露差异、修复后尚未复采；`部分` = 只覆盖了卡的部分窗口项。
> 这些卡**都仍未 `closed`**；剩余项见「还缺什么」。

| # | 卡 id | 证据路径 | 覆盖的窗口项 | 结论 / 还缺什么 |
|---|---|---|---|---|
| T1-01 | `openmir2-smframe-minor-gaps` | `docs/window-round2-result-20261023.md §1`；`E:/tmp/window/20261023/compare.json`（`pass=true`/`diffs=0`/`whitelisted=5`，sha256 `f14afc49…`）；`E:/tmp/window/20261023/{csharp,rust}/proxy/conn-0001.s2c.bin`（sha256 `cb006aed…` / `68a307f4…`） | 入图 15 帧前缀（`658,51,50,41,213,11029,42,766,54,5008,1106,52,752,811,46`）逐帧 body 相等；白名单仅 `recog` 与 `52` 布局 | **GREEN（登录前缀）**。还缺：T-79/T-80 登记项；非 TestServer/发光装备/IsSpirit（见 T1-03）；`657×2`（表 3）；证据 head=`ab44031ce`，当前 master 仅多 docs 提交 |
| T1-02 | `openmir2-login-654-order` | `docs/window-round2-result-20261023.md §2/§3`；`compare.json` frames idx 11–14；`docs/window-round2-runbook-20261023.md` 附录 A（2026-10-22 首轮 47 vs 53 帧） | `654` 恰一条、在 46 后、字段 `Recog=self/Param=0/Tag=500/Series=0`；出现对 N=5(Rust)/6(C#) 下 `811,46` 相对序保持；第二 `42` body 逐字节相同；`100×4` 条数与字段一致；`50/51` 的 `param` 均为 289 | **GREEN（654/42/100）**。还缺：有会角色 750 槽两侧各一次；非 TestServer 部署的 SysMsg 条数；第二 42 的**绝对位置**（C# 在 `657×2` 之后，Rust 在 654 之后）；657 修复后复采 |
| T1-03 | `openmir2-login-frame-order-52-752` | `compare.json`（`52_semantic_compared=true`、`unverified=[]`、52/752 序一致）；`docs/window-shrink-run-20261019.md §10`；`E:/tmp/window/cliente2e/{csharp,rust}/proxy-621` | `52` 的 18 个语义字段（`winplay0` L30）逐项比较；`52,752` 相对序；`621/211` 实测：有武器 `52,752,621`、有技能 `52,752,621,211`，oracle 真实序 `52,752,621,211,811,46` | **部分（GREEN）**。还缺：E2 非 TestServer/发光装备/IsSpirit 槽位变化；`openmir2-login-magic-211-empty-slots` 的 211 空槽修复；`621/211` 在当前 head 复采；§10 未重述 head（同日 §8 为 `f3675f69a`；**UNKNOWN** 是否中途换过） |
| T1-04 | `openmir2-login-numeric-gaps` | `docs/window-round2-result-20261023.md §5`；`compare.json`；`docs/window-run-sheet-20261022.md §B2`（部署权重=14） | `50/51` 的 `param` 两侧相等（289）⇒ 上轮 289/288 为输入漂移；`52` 语义字段（L30）已比 | **部分**。还缺：新号首登 `52` 的 `Weight=14`/`MaxExp=10`（本轮用 L30 账号，**未覆盖新号**）；新号首杀 `44/45/52/752` 帧序（不得出现 45）；非零 `WalkWait` 怪（圣域牛魔王 1200）的行走间隔 |
| T1-05 | `openmir2-window-collector-preflight` | `docs/window-round2-result-20261023.md §1`；`compare.json`；`docs/window-round2-runbook-20261023.md` 附录 A；`judge_login.py --selftest`（GREEN 35 assertions, rc=0）与 `preflight.py --selftest`（GREEN 25 assertions, rc=0，2026-10-21 本机只读复跑） | 两侧采集 → `capture_login_compare.py` → `0` 非白名单差异的端到端链路跑通；proxy dump 的 `conn-*.s2c.bin` 可被判定器消费 | **部分（工具链 GREEN）**。还缺：两侧种子交叉断言；C# `site` 列；前置门在生产路径不误拦。另：`tools/loginfixture/capture_login_compare.py` 的模块 docstring 仍写「本车道没有在真机窗口里跑过」，与本轮事实不符，属注释漂移（是否有意 **UNKNOWN**，建议另开 L 卡订正） |
| T1-06 | `openmir2-aoi-verify-maptime` | `docs/window-shrink-run-20261019.md §3/§4`；`E:/tmp/window/cliente2e/{csharp,rust}/s2c-w03`（W0-3 反向点）、`E:/tmp/window/cliente2e/{csharp,rust}/s2c-w04`（W0-4 GuardList 点） | 反向点 `(311,666)` 两侧 `appearance_pairs=0`；GuardList 点 `(291,666)` 两侧各**恰 1 对** `SM_TURN(281,661)+SM_FEATURECHANGED` | **部分**。还缺：`vis>=1`/`nearest<=vr` 的显式读数；当前 master 复采（本行证据之后 world 有 MB-4/城堡等改动：§8 复验 `f3675f69a`、当前 master `ce61cdca4`）；**`SM_WALK=0`（W0-4 复跑）仍属本卡余量**（看板 `work` 明列）；`openmir2-monster-walk-frames` 只承接「怪物走位发 `SM_WALK(11)`」，其 acceptance 不含静态 GuardList 的 W0-4 检查；本行证据 head=`7b2a8e7d5`（不含 §8 的 `f3675f69a`） |
| T1-07 | `openmir2-guardlist-guards-loading` | `docs/window-shrink-run-20261019.md §4`（`E:/tmp/window/cliente2e/{csharp,rust}/s2c-w04`） | GuardList 帧一致：两侧各恰 1 对 `SM_TURN(281,661)`+`SM_FEATURECHANGED`；C#/Rust `recog` 不同已登记为会话 ID 生成差异 | **部分（GREEN）**。还缺（卡面 `work` 明列的剩余 DoD）：①**③a 反向点出现帧一致**（C# 侧现 1 对、修复后应两侧同）；②**`MIR2_RNG_SEED` 双侧 `site`/`order` 对拍**；两项并入下次联调窗口（与 ③b/③a 复跑同窗）。本行证据 head=`7b2a8e7d5`。另：该卡离线片 review 写 `tier=L`，本表按 m3 B 表把其**剩余真机项**登记为 R |
| T1-08 | `openmir2-monster-attack-hit-frame` | `mir2-rs/tests/parity/m3/audit-monster-attack-cadence-20261019.md §0-B2/§2`；`E:/tmp/window/cliente2e/{csharp,rust}/s2c-*`；`docs/window-shrink-run-20261019.md §1/§2/§5` | **缺陷侧真机证据**：Rust 的 5–66 条 `SM_STRUCK` capture 中 `SM_HIT(14)` **0 条**，C# 有；W0-1/W0-2 证明「真实客户端→服务端→出站伤害帧」端到端成立（数值不同，不作字节判据）；W0-5 怪→玩家入站 ≥1 两侧成立；本行证据 head 按来源分：10-19 缩窗（§1/§2/§5）=`7b2a8e7d5`、10-18 缩窗（W0-5 入站 ≥1）=`84b696b05`、审计文档基线 `049771a9b`（只读解析 capture，非采集 head） | **RED/缺陷侧（修复后未复采）**。还缺：`dcdcd807d` 之后的真机复采：每个受击者前 ~200ms 有同 `Recog` 的 `SM_HIT`、`Series=AttackDir`、逐字节头字段、收件人集 |
| T1-09 | `openmir2-monster-attack-cadence-lead` | `audit-monster-attack-cadence-20261019.md §0-C/§2`；看板线程 `openmir2-monster-attack-cadence-lead` 的定文 comment（`ts=1792389327`） | 原线索「Rust 14s 内被 5–8 次打死 vs C# 30s 仅 2 次」被定量证伪：Rust 92 个同怪间隔 min=3.147s、C# 25 个 min=3.190s，两侧 **0 条 <3.0s** | **部分（线索证伪）**。还缺：**HP 受控重采样**（起始 HP 未控是原线索的混淆项）；`recog=SELF` 过滤后的同怪 interval；尸体/复活构造 |
| T1-10 | `openmir2-delmagic-addmagic-chain` | `docs/window-shrink-run-20261019.md §10`；`E:/tmp/window/cliente2e/{csharp,rust}/proxy-621` | **缺陷侧真机证据**：角色带职业不可用技能时，C# 登录发 `SM_DELMAGIC(212)` 且落库；下线后 `characters_magic` 行消失；Rust 当时**未观察到 212** | **RED/缺陷侧（修复后未复采）**。还缺：`210/212` 的字节与序；会话内戴 115 发 210 / 打碎治愈戒指发 212；FlameRing 粘滞跨登录；若同场景出现 `657×2` 一并记；§10 未重述 head（同日 §8 为 `f3675f69a`；**UNKNOWN**） |
| T1-11 | `openmir2-monster-walk-frames`（A-17 派生，非 B 表卡） | `audit-monster-attack-cadence-20261019.md §0-B1/§4.2`；同批 `E:/tmp/window/cliente2e/**/s2c-*` capture | **缺陷侧真机证据**：怪物走位时 Rust **完全不下发** `SM_WALK(11)`；oracle 观察者回 `SM_WALK` | **RED/缺陷侧**。还缺：修复后每次走位一帧、字段/收件人/相对序的真机复采；该卡与 657 同 lane（`wt-657`）串行，见 `openmir2-monster-walk-frames` `work` |

> 表 1 计数：**11 条**。其中 `T1-02`（654-order）、`T1-11`（monster-walk）不在 m3 B 表 69 卡内，属窗口派生卡。
> `openmir2-login-charstatus-657` 的证据见 **表 3**（本窗口新发现），不在此重复。

---

## 表 2：仍待窗口的 R 卡（51 行，覆盖 60 卡；按 E 依赖与工具分组）

> 口径：本表是 m3 B 表 **69 卡减去表 1 已列 9 张 B 表卡**后的剩余（60 卡；B-49 合 7、B-55 合 2、B-56 合 3）。
> 「剩余窗口项」与「所需工具/状态」直接摘自 `docs/m3-remaining-inventory-20261022.md` B 表（基线 `c83a9ae24`），
> **不是**本文件重新实测；开窗前必须按 run sheet 重跑工具自检。
> 表 1 的 9 张 B 表卡**也仍未 closed**，其剩余项见表 1，本表不重复。

### E1（写库边界/克隆）+ E4（C# oracle 身份）

| B# | 卡 id | 剩余窗口项（B 表原文） | 所需工具/状态（m3 清单基线 `c83a9ae24`） |
|---|---|---|---|
| B-07 | `openmir2-outbound-encoders-red-batch` | 制药抽数流水 + 帧序 + MYSTATUS 派生落库 | 缺（G4 无法术驱动） |
| B-21 | `openmir2-oracle-humitems-align` | `HumItems` load/save 1:1（B3） | C6 就绪；E1-a 下只能行数/关键值 |
| B-23 | `openmir2-save-on-shutdown` | `DB_SAVEHUMANRCD` 帧与落库 diff、`-Stop` graceful 日志；D-12「oracle 是否真的落库」 | 部分（E1-a 不能快照/字段级 diff） |
| B-24 | `openmir2-persist-useitems-wiring` | 耐久磨损/破零/存档往返 | 部分 |
| B-25 | `openmir2-persist-create-rows` | 建角/兜底建角往返 | 部分 |
| B-26 | `openmir2-save-misc-gaps` | G11 传输面、G7 读到一半失败 | 部分 |
| B-27 | `openmir2-persist-item-attr-and-gold` | G-01 不落库（B-117）、G-02 丢失（B-116）、建角/存档往返 | 部分 |
| B-28 | `openmir2-snap-cell-fidelity` | 两个前置未解（评审 D2） | 缺；本窗不收 |
| B-32 | `openmir2-run-gap-pl4` | 元宝自动增减/DB 金币轨迹 3 项 | 部分 |
| B-39 | `openmir2-upgrade-npc` | 抽数序 / `Desc[10]` 字节 / 帧序 / 654·153 / 破零链 + T-95 | 部分 |
| B-45 | `openmir2-dbsrv-scheme-a` | W-DB-1..7（慢存档/全序/闸门/独立失败）+ T-120 | 部分（LB-08 判据已有） |
| B-59 | `openmir2-m3-final-acceptance` | 300 假人 24h 长跑（DoD②） | 方案/工具就绪；需独占 24h 窗口 |

### E4（仅需 C# oracle 身份；不写库、不改内容）

| B# | 卡 id | 剩余窗口项（B 表原文） | 所需工具/状态（m3 清单基线 `c83a9ae24`） |
|---|---|---|---|
| B-04 | `openmir2-healthspell-emission-model` | 53 的触发/尾长 4 项 | C4 部分；C10#3 服务端事件读数缺 |
| B-05 | `openmir2-orphan-encoder-wiring` | 种子断言与生产路径未覆盖项 | 部分（run sheet D.1 明列） |
| B-10 | `openmir2-player-dir-supply` | Dir 变化的可观测帧 | 就绪 |
| B-12 | `openmir2-monster-view-throttle` | GuardUnit 的 6 格/CanPassiveAttack、自扫相位、多玩家投递序 | 部分 |
| B-13 | `openmir2-attacktarget-overrides` | 82/119/130/91/107/105 的目标选择与抽数；114/117/118/215 不可达 | 部分（需构造特殊怪） |
| B-14 | `openmir2-active-seek-race-missing` | 9 个补入 seek race 的真机索敌 | 部分 |
| B-15 | `openmir2-proper-target-unfolded` | 攻击已死目标的抽数流水 | 部分（需尸体构造） |
| B-16 | `openmir2-search-target-clear-target` | `ClearTargetCreat`/`RemoveAt` 跳项怪癖 | 部分 |
| B-17 | `openmir2-run-gap-mb1` | race 101/95 各 ≥1、R38 抽样；`CellType.Event` 未落格 | 部分 |
| B-18 | `openmir2-run-gap-mb3` | 107/105/102/112；`flush_sculture_spawns` 排水时序 | 部分 |
| B-19 | `openmir2-run-gap-mb4` | race 92 的 SpaceMove/MapRandomMove 落点与狂暴 500/400 节拍；CastleDoor HoldPlace | 部分 |
| B-29 | `openmir2-run-gap-pl1` | 装备复检 + 复活戒指 4 项 | 部分 |
| B-30 | `openmir2-run-gap-pl2` | P31 经验帧 + P07 死代码结论 | 部分 |
| B-31 | `openmir2-run-gap-pl3` | 内容启用后的 HP/倍率/饥饿数值与帧序 + T-85 八条 | 部分（内容不可达） |
| B-33 | `openmir2-chat-channels` | `!!`/`!~`/`!` 的帧序 + 10s 门 + T-118 零帧确认 | 部分 |
| B-40 | `openmir2-gategate-keepalive` | 10s 周期、3min 窄竞态可达性（T-119②）、断链重连与 15 天清理 | C8 工具 v2.5 就绪；真机 capture 未采 |
| B-41 | `openmir2-gate-idle-timeout` | LoginGate/SelGate 60s 两阶段（含畸形帧）两项 | `gate_timer_gate.py` 就绪（32/32） |
| B-42 | `openmir2-selgate-reconnect` | KeepAlive + 下行 EOF 回连两项 | `gate_timer_gate.py` 就绪 |
| B-43 | `openmir2-gategate-slots-gm` | 槽位回收、GM_STOP、槽满复用三项 | C8 工具就绪 |
| B-44 | `openmir2-gategate-speed-rules` | 登录包相位、0-210ms 未丢帧先后、超长帧回归、ItemSpeed/SpeedLimit | 缺（0-210ms 丢帧/10ms 拍不在 C8 覆盖） |
| B-48 | `openmir2-encoding-core-e1-e2` | 非 ASCII 文本逐字节 + 两条 BOM 路径 | 就绪（dotnet 9 + CodePages 全表探测） |
| B-49 | `openmir2-run-gap-checkdupobj`、`openmir2-run-gap-corpse-ghost-revive`、`openmir2-run-gap-monster-state-expiry`、`openmir2-run-gap-monster-specials`、`openmir2-run-gap-monster-master-follow`、`openmir2-movetomovingobject-quirk`、`openmir2-disappear-recipients-gridscan`（7 卡合一） | 同格推挤身份序列/AOI 相位；3min 变尸/复活/5min 移除；Crazy 色变/FireHit-TwinHit；产仔抽数/SM_HIT；异图/超距/SlaveRelax；移动路径源/目标格；节流窗收件人 | 部分（需多 actor/尸体/特殊怪构造） |
| B-51 | `openmir2-ctor-draws-fold` | 68 抽/17 站点逐项 diff；barrier A 的 1 抽另立片 | 缺 |
| B-53 | `openmir2-default-arm-spell-surface` | 逐帧对拍 + 效果体（MP/53/MAGICFIRE/45 case） | 缺（G4 无法术驱动） |
| B-54 | `openmir2-spell-shared-tail` | 注入时钟下 656 帧序 + T-78 三偏差 | 部分 |
| B-55 | `openmir2-buff-system`、`openmir2-buff-producer-wiring` | 真机窗口 + A-5 生产接线（四块 snap 改 `cetin_near_xy`） | 部分 |
| B-56 | `openmir2-world-tick-expansion`、`openmir2-transmap-maptrigger`、`openmir2-maptrigger-monster-and-door` | RNG 流水/时钟/公告节拍；1658 route；W-R1-a/b/c | 部分 |

### E1 + E2 + E4（城堡/行会：写库 + 影子内容 + oracle）

| B# | 卡 id | 剩余窗口项（B 表原文） | 所需工具/状态（m3 清单基线 `c83a9ae24`） |
|---|---|---|---|
| B-34 | `openmir2-castle-war-area-impl` | W-1..W-5：四臂落点 RNG/766 bit4/名字色/10s Run + T-125..T-132 | 部分（内容/城主需构造） |
| B-35 | `openmir2-castle-entities` | 部署 0 实体/4 告警、夹具 16 实体 96 抽、毒攻门、目标选择、1-tick 相位 + T-96 | 部分 |
| B-36 | `openmir2-castle-guard-target` | 同城/盟友/UnderWar 目标选择 + T-96⑥④ | 部分 |
| B-37 | `openmir2-guild-g3-persistence` | 104 扇出/SS_208/四段字节/相位/实际收件人集 + T-97..T-117 | 部分 |
| B-38 | `openmir2-guild-g2-dispatch` | 750 真机出队序等 3 项 | 部分 |

### E1 + E3（MySQL 停/起）+ E4（DB 故障注入）

| B# | 卡 id | 剩余窗口项（B 表原文） | 所需工具/状态（m3 清单基线 `c83a9ae24`） |
|---|---|---|---|
| B-46 | `openmir2-dbsrv-fault-paths` | 真机故障注入 + LB-10（LB-08/09 已由 `53d7aacc8`/`f7dc09d31` 落地） | 部分 |
| B-47 | `openmir2-db-error-reply` | 失败注入的 C# 对拍 | 部分 |

### O1–O7（RNG 场景 pin，未裁决）+ E2 + E4

| B# | 卡 id | 剩余窗口项（B 表原文） | 所需工具/状态（m3 清单基线 `c83a9ae24`） |
|---|---|---|---|
| B-50 | `openmir2-m3-rng-scene-driver` | 两侧账本对拍（W9）+ BotSrv real wiring | 缺（`-Go -PinScene` 仍 `PREREQ_MISSING`） |

### E4 + owner 裁决

| B# | 卡 id | 剩余窗口项（B 表原文） | 所需工具/状态（m3 清单基线 `c83a9ae24`） |
|---|---|---|---|
| B-52 | `openmir2-npc-ctor-draws` | F/M 裁决（8 处）+ `Robot.txt` 行数实测 | 缺 |

### O3（C# 时钟注入，未裁决）

| B# | 卡 id | 剩余窗口项（B 表原文） | 所需工具/状态（m3 清单基线 `c83a9ae24`） |
|---|---|---|---|
| B-57 | `openmir2-m3-clock-injection` | C# 时钟注入（O3 的前置） | 缺 |

### 无 E 依赖（工具/独占窗口）

| B# | 卡 id | 剩余窗口项（B 表原文） | 所需工具/状态（m3 清单基线 `c83a9ae24`） |
|---|---|---|---|
| B-58 | `openmir2-m5-convergence-drills` | P99 三档收敛演练 | 部分（需独占窗口） |
| B-60 | `openmir2-gate-gmclose-race` | 无单独窗口项；观察点=长跑下控制帧 flush（tier=R 不 closed） | 随长跑 |


> 表 2 计数：**51 行 / 60 卡**。

---

## 表 3：本窗口新发现（`657×2`）与关联卡

### 3.1 新缺陷

| # | 发现 | 证据 | 判级/性质 | 关联卡 |
|---|---|---|---|---|
| N-01 | Rust 登录路径缺 `SM_CHARSTATUSCHANGED(657)×2`：C# 在 `654` 之后发两条（`Recog=self, Param=0, Tag=0, Series=0, body_len=0`），Rust 一条都没有 | `docs/window-round2-result-20261023.md §4`；`E:/tmp/window/20261023/{csharp,rust}/proxy/`；`docs/window-round2-runbook-20261023.md` 附录 A（2026-10-22 首轮也是 `657,657`） | **R（帧面/时序）· 新缺陷**；两轮窗口一致 ⇒ 非本轮二进制回归 | `openmir2-login-charstatus-657`（已开，P1，待派工） |
| N-02 | 第二 `42` 的绝对位置被 657 缺口掩盖：C# 序 `654,657,657,第二42`，Rust 序 `654,第二42`；body 本身逐字节相同 | `docs/window-round2-result-20261023.md §3` | R · N-01 的同根因后果（**不是**独立的 42 缺陷） | `openmir2-login-654-order`（复采项） |
| N-03 | 判定器已经把 657 写进硬判据：`judge_login.py` J3 要求 `657` 恰两条且 `i46 < i654 < 657[0] < 657[1] < i42[1]`，J4 要求第二 42 后恰 `100×4` | `mir2-rs/tests/parity/m3/window-20261023/judge_login.py:144-170`；`judge_login.py --selftest` = GREEN 35 assertions, rc=0（2026-10-21 复跑） | 工具判据（有牙）：657 未补时对 Rust capture 必 RED | 657 卡；G1 场景 |

### 3.2 同窗观察（非新缺陷，已登记）

| # | 观察 | 证据 | 处置 |
|---|---|---|---|
| O-01 | `50/51` 的 `param` 两侧均为 289；上轮 289/288 的差在两侧 DB 重置后消失 | `docs/window-round2-result-20261023.md §5` | 判定为**输入漂移**（角色位置不同），不作缺陷 |
| O-02 | `100×4` 正文集合相同、**顺序不同**（C#：客服QQ→欢迎→禁止外挂→攻击模式；Rust：攻击模式→…→客服QQ） | `docs/window-round2-result-20261023.md §3` | 已登记为观察者队列同优先级堆序的不确定性；**不作判据**（两次真机本身也不一致） |
| O-03 | `52` 布局 C# 40B / Rust 58B；`recog` C# 大整数 / Rust 序号 | `docs/window-round2-result-20261023.md §1/§5`；`compare.json` `whitelisted` | D-9/client58 与 recog 口径白名单；`52` 语义字段仍逐项比较（已比） |
| O-04 | 2026-10-22 首轮（旧二进制）C# 已有 `657,657`，Rust 也没有；首轮 Rust 还因 `(288,620)` 落进 MonGen 刷怪框被打死 | `docs/window-round2-runbook-20261023.md` 附录 A / §0.1 | 排除「本轮二进制更新引入 657 缺失」；下一窗固定 `(311,666)`、不走路、不打怪 |

### 3.3 657 的关联卡与影响面

| 卡 | 关系 | 当前状态（2026-10-21 快照） | 657 修复后的判据 |
|---|---|---|---|
| `openmir2-login-charstatus-657` | 新缺陷主卡 | `in-progress`，owner `openmir2-orchestrator`，`work=待派工（wave16）` | acceptance：补两条 657（`StatusChanged` 自回环 + `SendUpdateMsg`）；字段与 C# 一致；**不改变 15 帧前缀与 654/42/100 位置**；每条判据带牙；真机复采 |
| `openmir2-login-654-order` | 证据来源；第二 42 绝对位置依赖 657 | `in-progress`（已合 `1915b67a5`，tier=R 不 closed） | 657 补齐后复采：`i654 < 657[0] < 657[1] < i42[1]`，且第二 42 body 仍逐字节相同 |
| `openmir2-smframe-minor-gaps` | 登录尾帧集完整性 | `in-progress`（`27d22a9f3`，tier=R） | 657 修复不得改变 15 帧前缀；登录尾帧集补全后重跑 `capture_login_compare.py` |
| `openmir2-upgrade-npc` | 其窗口项显式包含 `657×2` 的帧序 | `in-progress`（`35c039c8d`，tier=R） | 提交/取回帧序 `202→52→41→653→[654]→657×2→709`；657 未补时该判据必 RED |
| `openmir2-monster-walk-frames` | **同 lane 操作关系**（`wt-657`/`login-657`，非 657 本身） | `in-progress`，`work=与 657 同 lane 串行（wave16）` | 657 修复合入/复采时不与 monster-walk 抢同一 game-svr 写集；lane 内串行 |

---

## 下一场窗口最小清单

> 目标：只把「登录链路 + 657 修复」判绿，不追 G4/G6/G7/G8/G9/G10/G11。每块 2 小时，块间收窗/还原。

1. **独占与前置（先做，缺一不开窗）**：确认九游戏端口 + `7600` 全 DOWN、`3306` UP。
   当前（13:30 实测）九个游戏端口被另一栈占用（3306 为共享 MySQL）⇒ **必须等其收窗**。随后重跑
   `client-e2e-window.fixed.sh.txt guard`，期望 `WINDOW_GUARD_OK`；R6 按**当天** `walgit/master` 重解析（`origin/master` 只是人工镜像，当前 `b46805058` 已落后 56 个提交、不含 `1915b67a5`/`ab44031ce`），R7 重编六件 + `--record-provenance`，R8 端口复核。
2. **代码前置**：合入 `openmir2-login-charstatus-657`（657×2），确认 **15 帧前缀、654、第二 42、100×4 均不变**；
   同 lane 的 `openmir2-monster-walk-frames` 串行，不并发改 `bins/game-svr`。
3. **镜像/产物**：`/e/tmp/m2svr-window-origin` 检出当天 `walgit/master`（仓库口径：walgit 为代码主源，`origin` 仅人工镜像）；`CARGO_TARGET_DIR=/e/tmp/m2svr-window-target`；
   重编 `game-svr/game-gate/login-srv/login-gate/sel-gate/db-srv`；`record_window_provenance.py --ref <HEAD> --record-provenance`。
4. **DB（E1-a）**：两侧跑前从生产 `mysqldump --single-transaction` 克隆 `mir2_test_data/_db/_account`；
   快照 `base`；**不写生产库**。
5. **起栈**：混合栈 `start_rust_stack.ps1 -RustBinDir E:/tmp/m2svr-window-target/debug -Seed 42 -DbName mir2_test` + C# DBSrv/LoginSrv 影子（E4）。
6. **抓包拓扑**：停真 gate(7200) → 起影子 gate(7600) → 代理占 7200；断言 `7200 DOWN / 7600 UP`（`port-status.ps1` 不含 7600）。
7. **G1 登录（唯一必跑块）**：角色钉 **`(311,666)`**（`check_reverse_point_clean.py` CLEAN；**不要**用 `(288,620)/(289,618)`），
   `mir_flow.ps1 -WalkClicks 0 -AutoPlaySec 0 -AttackClicks 0 -SkipReenter -SkipLogout`；
   先 `export_s2c.ps1`（当前 proxy dump 缺 `port` 字段，`s2c_diff.py` 会 `PREREQ_MISSING`），再 `judge_login.py`。
   判据：J1 15 帧、J2 654、J3 `657×2` + 42×2、J4 100×4；`capture_login_compare.py` 0 非白名单差异。
8. **时间富余才跑**：G2a（210/212/621/211）、G3a（`SM_HIT`/`SM_WALK`/AOI/GuardList）、G5a（存档/关服）。
   每组前先 `preflight.py`，缺同轮 C# capture 时 `run_scenario.ps1` 必须 `PREREQ_RED`（fail-closed）。
9. **收窗（顺序不可换）**：停代理 → 停 7600 → 停 Rust 栈（`-Stop` 跳过 MySQL）→ C# `CTRL_BREAK` 优雅停 →
   DB restore（`--db test --yes-apply`）→ 端口 + 7600 单独断言 → 证据归档。
10. **判据带牙**：`judge_login.py --selftest`（35 assertions）、`preflight.py --selftest`（25 assertions）、
    `capture_login_compare.py --selftest` 全 GREEN；另做一次负向对照（把合成 capture 中的 657 删掉 ⇒ J3 必 RED），
    证明判据不是假绿（见附录 B 的文档门禁同法）。

---

## 附录 A：只读复核命令（可复算）

```sh
# 1) 看板快照（本文件表 1/2/3 的卡状态来源）
cd /e/tmp/wt-evidence && walgit collab board --repo . --format json > E:/tmp/evidence-board.json

# 2) 第二轮 15 帧对拍（2026-10-21 复跑，rc=0）
cd /e/tmp/wt-evidence && python -I -B tools/loginfixture/capture_login_compare.py \
  --oracle E:/tmp/window/20261023/csharp/proxy \
  --rust   E:/tmp/window/20261023/rust/proxy
# 期望末行：RESULT GREEN: 15/15 帧，0 非白名单差异（whitelisted=5，head_diag=0）

# 3) 场景包自检（只读，不起栈）
cd /e/tmp/wt-evidence
python -I -B mir2-rs/tests/parity/m3/window-20261023/judge_login.py --selftest   # SELFTEST GREEN (35 assertions), rc=0
python -I -B mir2-rs/tests/parity/m3/window-20261023/preflight.py  --selftest   # SELFTEST GREEN (25 assertions), rc=0

# 4) 证据 hash（表 1 引用）
sha256sum E:/tmp/window/20261023/compare.json \
  E:/tmp/window/20261023/csharp/proxy/conn-0001.s2c.bin \
  E:/tmp/window/20261023/rust/proxy/conn-0001.s2c.bin \
  E:/tmp/m2svr-window-target/window-provenance.json
# f14afc49... / cb006aed... / 68a307f4... / 563de1ac...
```

关键卡 `last_oid`（2026-10-21 快照，用于重放）：
`openmir2-evidence-refresh`=`d723c08f6fe24e5e8f246470904fb77314537bb5`、
`openmir2-login-654-order`=`d7c72c7d557707d98a1037c3f1214f89d3be6546`、
`openmir2-login-charstatus-657`=`f4edee74e57c295338145c4598280a3f49b4fb4b`、
`openmir2-monster-walk-frames`=`0e1ac25d347661659a492ca49af09a40936c13e5`。

## 附录 B：文档门禁（带牙）与限制

**门禁（docs-only，tier=L）**：`/e/tmp/check_evidence_doc.py` 对本文件做 5 条机判：
1. **D1 卡号存在**：文中每个 `openmir2-*` 卡 id 必须出现在看板快照中；
2. **D2 证据路径存在**：表 1 引用的每个 `E:/tmp/...` 路径必须可 `stat`；
3. **D3 计数一致**：表 1 = 11 条、表 2 = 51 行、表 3 的 3.1 = 3 条、3.2 = 4 条、3.3 = 5 行，与正文声明一致；
4. **D4 B 表全覆盖**：m3 B 表 69 卡在表 1∪表 2 中**恰好出现一次**，不漏不重；
5. **D5 写集干净**：`git status --porcelain` 只允许本文件。

**负向对照（先红后绿）**：在临时副本上分别改坏一处 —— ① 把 `openmir2-login-charstatus-657` 改成不存在的 id；
② 把一条 `E:/tmp/...` 路径改成不存在；③ 删掉表 2 一行。三次门禁均以非零退出并打印对应 D 项；
从原文副本逐字节还原（`sha256sum -c` OK）后门禁恢复 `DOC_GATE_OK`。这与「改坏生产路径 ⇒ 指定断言必红」同法，只是对象是文档一致性。

**限制 / UNKNOWN**：
1. `657×2` 的根因未定位：是 `RecalcAbilitys` 的 `StatusChanged()` 自回环，还是 `SendUpdateMsg(RM_CHARSTATUSCHANGED)` 来源，
   **UNKNOWN**；卡里已列两条来源待查。
2. 表 1 的 `SM_HIT`/`SM_WALK`/`212` 证据都是**修复前**的 RED 证据；修复后没有真机复采 ⇒ 不能据此收卡。
3. 缩窗证据 head 按来源分：**2026-10-18** = `84b696b05`（`docs/window-shrink-run-20261018.md`）；**2026-10-19** §1–§7 = `7b2a8e7d5`、§8 复验 = `f3675f69a`；当前 master `ce61cdca4` 之后 `crates/world` 有 MB-4/城堡等改动，
   **旧 GREEN 不自动等价于当前 head GREEN**。
4. 当前栈是另一条 lane 在 13:30 启动的（占用 5000–7200）；其归属/预计收窗时间 **UNKNOWN**，本文件未触碰。
5. 看板是移动目标；本文件只对 13:34 快照负责，重放以附录 A 的 `last_oid` 为准。

## 残留物清单（本线专属）

- 仓库内：仅 `docs/r-card-evidence-status-20261021.md` 一个新增文件（本提交）。
- 仓库外临时物（未入库、未清）：`E:/tmp/evidence-board.json`、`E:/tmp/evidence-threads/`（74 个线程 JSON）、
  `E:/tmp/evidence-summary.tsv`、`E:/tmp/evidence-ids.txt`、`E:/tmp/gen_table2.py`、`E:/tmp/build_evidence_doc.py`、
  `E:/tmp/check_evidence_doc.py`。均为只读分析的中间物，保留供复核；按 D-10 清理前先归档本文件引用的证据路径。
- worktree：`E:/tmp/wt-evidence`（本线；收尾时移除）。
