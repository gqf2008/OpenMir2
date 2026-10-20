# 窗口 Run Sheet 评审（基线 3bdfa0888）

评审对象：`E:/tmp/window-run-sheet.md`（562 行 / 59,690 B）。
评审方式：只读核对仓库、部署件、DB 现状与看板；未起栈、未写库、未改仓库。
判据：`AGENTS.md` §判据分级；评审结论按 `FAIL / PARTIAL / OK` 标注。

## 结论摘要

方向（按组收口、G0 前置、R 卡保持 in-progress）是对的，但**当前版本不能直接开窗**。最硬的五个阻断：

1. G0 的 guard 固定检查 `/e/tmp/m2svr-window-origin`，该镜像 HEAD 是 `f3675f69a`（落后基线 `3bdfa0888` 60 个提交，且有一处未提交改动），而 run sheet 却在主检出构建并录 provenance；R6/R7 必红，`E:/tmp/m2svr-window-target` 也不存在。
2. Rust 抓包拓扑缺“停 game-gate(7200) → 起影子 game-gate(7600)”这一步；`start_rust_stack.ps1` 已占 7200/7201/7202，`mir2_proxy` 再绑 7200 会直接失败。
3. G5/G6 用了 `start_rust_stack.ps1`，它起的是 **C# DBSrv/LoginSrv**；而 `openmir2-dbsrv-fault-paths`、`openmir2-db-error-reply`、`openmir2-persist-*` 等卡要测的是 Rust `db-srv`，本窗口不会执行被测代码。
4. C# 关服用 `E:/MirServer/start-all.ps1 -Stop`：`Stop-Stack` 是 `Stop-Process -Force` 且会停 `mysqld`；`OnShutdown/SavePlayer` 根本不会跑，D-12 的“oracle 是否真落库”判不了，且会把共享 DB 带走。
5. DB 还原命令写错：`dbsnap.py restore --in` 不存在（实际是 `--dir`），且默认 dry-run，必须 `--yes-apply`；按现文收窗不会真正回滚。

## A. 窗口开不起来 / 起错栈（P0）

### A1. G0 guard 源/目标不一致
- Verdict: **FAIL**
- 证据：`client-e2e-window.fixed.sh.txt:26-30` 固定 `repo=/e/tmp/m2svr-window-origin`、`tgt=E:\tmp\m2svr-window-target`；该镜像 `git log -1` = `f3675f69a`，`git merge-base --is-ancestor f3675f69a 3bdfa0888` 为真（落后 60 个提交），且 `git status` 有 `M docs/window-shrink-run-20261019.md`；`E:/tmp/m2svr-window-target` 目录不存在。run sheet G0 却在 `E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs` 构建并录 `E:/tmp/m2svr-window-target/window-provenance.json`。
- 更正：G0 加 M1/M2：`git -C /e/tmp/m2svr-window-origin fetch && git -C /e/tmp/m2svr-window-origin checkout --detach 3bdfa0888`；在镜像里 `cargo build` 六件到 `E:/tmp/m2svr-window-target`；用 `--repo /e/tmp/m2svr-window-origin --ref 3bdfa0888 --target-dir E:/tmp/m2svr-window-target --record-provenance ...` 录清单；再跑 guard。若坚持从主检出构建，必须同步改 guard 的 `repo/tgt`（属脚本变更，不能在本窗临时改）。

### A2. Rust 抓包拓扑不可执行
- Verdict: **FAIL**
- 证据：`tools/m2-acceptance/start_rust_stack.ps1:393-395` 起 `game-gate` 于 `7200,7201,7202`；run sheet G1 驱动先起该栈，再 `python tools/capture/mir2_proxy.py --map 7200=127.0.0.1:7600`，代理无法绑定 7200。`82-two-side-window-runbook.md` §3.2 明确要“把真 game-gate(7200) 停掉，起影子 game-gate(7600)”，run sheet 省略了这一步。
- 更正：起栈后 `Stop-Process -Name game-gate`（或按 pid 文件停该项），用 `game-gate.exe E:/tmp/window/cliente2e/shadow/game-gate.conf` 起 7600；确认 7200 DOWN / 7600 UP 后再起代理。收窗要显式停代理与 7600 影子网关（`port-status.ps1` 的十端口不含 7600，现有收窗清单会漏）。

### A3. G5/G6 被测栈错误（C# DBSrv 代替 Rust db-srv）
- Verdict: **FAIL**
- 证据：`start_rust_stack.ps1:10-20,390-394` 的进程构成是 **C#** `src/DBSrv/bin/Release/DBSrv.exe`（6000/5100/5700）+ **C#** `LoginSrv.exe`；Rust 只有 game-svr 与三网关。`openmir2-dbsrv-fault-paths` issue 的 acceptance 是“6000 acquire 失败回 1100 recog=1……5100 四条命令 520/524/522/525”，`openmir2-persist-create-rows` 的 goal 是 Rust `db.rs::data_add`，用 C# DBSrv 全部不覆盖。`tools/m5-acceptance/start_full_rust_stack.ps1` 已能起 Rust db-srv/login-srv（6000/5100/5700/5500/5600）。
- 更正：G5/G6 拆两段：G5a（game-svr 存档记录、关服、耐久）可用混合栈；G5b/G6（Rust db-srv 持久化、故障路径、1100/520/524/522/525）必须用 `start_full_rust_stack.ps1`。G0 的“六件重编”也要说明：混合栈只用其中四件，db-srv/login-srv 的 provenance 目前没有对应的被测进程。

### A4. C# 关服方式会绕过 OnShutdown，并停掉共享 MySQL
- Verdict: **FAIL**
- 证据：`E:/MirServer/start-all.ps1:112-120` 的 `Stop-Stack` 对 `LoginGate/SelGate/GameGate/GameSvr/LoginSrv/DBSrv/mysqld` 一律 `Stop-Process -Id ... -Force`；`src/GameSrv/AppService.cs:347-353` 的 `StoppingAsync` 才调 `OnShutdown()`（`:206-210` -> `SavePlayer`）。`tools/m5-acceptance/start_full_rust_stack.ps1:12-16` 明确警告不要用 `start-all.ps1 -Stop`（会停共享 DB）。
- 更正：C# 侧用控制台优雅关闭（Ctrl+C / `Console.CancelKeyPress` / WM_CLOSE，并等待 `OnShutdown` 完成）替代 `start-all -Stop`；若必须停 MySQL，先记录 DB 快照并在关闭前完成 `DB_SAVEHUMANRCD` 观测。收窗顺序改为：先优雅停 GameSvr（等存档）→ 再停 DBSrv/网关 → 最后决定是否停 MySQL → 再 DB 还原。

### A5. `dbsnap restore` 命令错误且默认 dry-run
- Verdict: **FAIL**
- 证据：`tools/dbsnap/dbsnap.py:668-669` 的 restore 子命令参数是 `--dir`、`--yes-apply`；`cmd_restore` 在无 `--yes-apply` 时打印 `DRY-RUN` 并返回（`:370-372`）。run sheet §0.3/§7 写的是 `restore --in E:/tmp/window/db/before`。
- 更正：`python -I tools/dbsnap/dbsnap.py --password= restore --dir E:/tmp/window/db/before --yes-apply`；收窗必须核对 restore 的 rc/行数，且不能在建栈已停 MySQL 后执行。

## B. 场景构造（按部署件/DB 现状）

### B1. `characters_item` 基线不是 13 空槽
- Verdict: **FAIL**
- 证据：现库 `aaa`（Id=12215）`SELECT COUNT(*) FROM characters_item WHERE PlayerId=12215` = **0**；其他角色才是 13 行 `Position 0..12`。run sheet G1 说“清空该角色 characters_item……基线必须 13 空槽”，`DELETE` 只会得到 0 行。
- 更正：DELETE 后显式 INSERT 13 行空槽（Position 0..12，MakeIndex=0/StdIndex=0/Dura=0/DuraMax=0），并在登录前断言 count=13；G2 的 6 件装备状态也要补齐另外 7 个空槽（共 13 行）。

### B2. 新号 52 的 Weight=10 是构造级合成值，部署实际是 14
- Verdict: **FAIL**
- 证据：`Server.conf` 新手四件 = 蜡烛 / 金创药(小量) / 木剑 / 布衣(男)；`stditems` 权重分别为 1/1/7/5，合计 **14**。`add_newbie_items` 用的是 catalog（部署模板）权重；`design-numeric-gaps-20261019.md:70` 的 `weight == 10` 是测试里人为赋的 `1/1/5/3`，不是部署值。
- 更正：真机期望改为 14（或窗口前从 `stditems` 现算并写进证据）；10 只保留在构造级单测。MaxExp=10 仍正确。

### B3. HumItems 期望写反了：当前是 1:1，不是 save 交换
- Verdict: **FAIL**
- 证据：`src/GameSrv/Word/WorldServer.cs:1325-1331` 的 save 侧注释“与 GetHumData 的读取侧对齐：项链/头盔各取本槽”，save 已改为 1:1；Rust `main.rs:19647/19655/20251/20447` 的断言也是 1:1；卡 work 写明“a84c81aae + d7ead5b2a（两侧 1:1）”。run sheet G2 的“load 各取本槽、save 仍交换的漂移可复现”是旧 issue 文本。
- 更正：期望改为 load/save 均 1:1；历史漂移只做负例/回归，不作为预期。若部署 C# 二进制早于 `d7ead5b2a`，先记录 `!deployed-from.json`/DLL hash，再决定是否要求重编。

### B4. 行会文件路径错误；`GuildList.txt` 实际已存在
- Verdict: **FAIL**
- 证据：`E:/MirServer/M2GameSvr/Server.conf:61-62`：`GuildDir=GuildBase/Guilds`、`GuildFile=GuildBase/Guildlist.txt`；`E:/MirServer/M2GameSvr/GuildBase/GuildList.txt` 存在（0 字节），`GuildBase/Guilds/` 不存在。run sheet G2 写“影子 Envir 下建 `GuildList.txt` + `Guilds/<会名>.txt`”，且称“部署件当前无 GuildList.txt”。
- 更正：在影子包的 `M2GameSvr/GuildBase/Guildlist.txt`（可写会名）与 `GuildBase/Guilds/<会名>.txt` 构造；不要放 `Envir/`。注意 `start-csharp-hook.ps1` 每次 `robocopy` 会覆盖 `-Shadow`，若要改内容，需先把影子包准备好并作为 `-Pack` 源（不能拷完再改 `-Shadow`）。

### B5. G2 把“有魔法”和“无魔法 + 戒指”两种状态混在一次登录
- Verdict: **PARTIAL**
- 证据：`openmir2-delmagic-addmagic-chain` 要求 FlameRing 粘滞/RecoveryRing 每轮重置、210/212 在 658 后 51 前；`openmir2-login-useitems-magic` 要求登录 621/211。run sheet G2 同时插 `MagicId=1` 与 115/116 戒指，且只跑一次登录。
- 更正：拆两轮 DB 状态：① 无 `characters_magic` + 戒指 Dura>0 → 210；再登不带戒指/ Dura=0 → 212 与粘滞对照；② 有 magic 行 → 211。每轮前重摆 DB 并记录 `characters_item`/`characters_magic` 快照。

### B6. G4 法术在 `aaa`（战士 1 级/30 级）上不可施放
- Verdict: **FAIL**
- 证据：`magics` 表：1/2/8/31/37 的 `Job=1`（法师），14/15 的 `Job=2`（道士）；37 `NeedL1=50`，31 `NeedL1=31`，14/15 `NeedL1=22/25`。`aaa` 现库 `Job=0`、`Level=1`；G3 只把 `characters_ablity.Level` 调到 30，G4 没有改 Job/Level。
- 更正：G4 用独立法师（Job=1，Lv>=50 以覆盖 31/37）与道士（Job=2，Lv>=25 以覆盖 14/15），或同时改 `characters.Job/Level` 与 `characters_ablity.Level` 后重登；把角色 Job/Level 写进证据。否则 31/37 只会走失败门，无法采 37/8 推挤与 53/639 成功路径。

### B7. G3 60 分钟/侧装不下 26 张卡，且多数构造未给
- Verdict: **FAIL**
- 证据：G3 场景只有单角色 `(605,578)`、GuardList `(291,666)`、反向 `(311,666)`、特殊怪刷点。卡面要求还包括：3 人同格（`run-gap-checkdupobj`）、±12 观察者（`disappear-recipients-gridscan`）、玩家被玩家/宝宝击杀（`player-drop-useitems`/T-81）、宝宝跟随/异图（`monster-master-follow`）、BeeQueen/SpiderHouse（`monster-specials`）、3 分钟变尸/5 分钟移除（`corpse-ghost-revive`）、攻击已死目标（`proper-target-unfolded`/`search-target-clear-target`）、多对象同格（`movetomovingobject-quirk`）、CastleDoor/829/837/856/858（`mb4`）、FireHit/TwinHit（`monster-state-expiry`）。这些都没有对应客户端/步骤。
- 更正：拆 G3a（单客户端：攻击计数、Guard、反向、AOI、dir）约 60 min/侧；G3b（多客户端：3 人同格、观察者、击杀者）需要 2–3 个客户端/账号；G3c（长生命周期：尸体/幽灵、特殊怪、宝宝）单独排 30–60 min 并写明等待。没有构造的卡从本组移除或标 `not_collectible`。

### B8. G1/G2 的 item/merchant 卡没有触发动作
- Verdict: **FAIL**
- 证据：`openmir2-sm-merchantsay-recog` 需要 NPC 对话；G1 驱动显式 `-SkipTalk`，G2 也没有对话步骤。`openmir2-sm-updateitem` 触发于穿/换装，`openmir2-sm-additem-tail` 触发于卸下/换装/商店/地面拾取；G1/G2 只登录，没有这些动作。
- 更正：G1 增加一次 NPC 对话（去掉 `-SkipTalk`，指定 NPC），G2 增加穿/卸/拾取/商店步骤（可用 `run_bag_equip_pair.ps1` 的固定序列或 `mir_flow` 的物品序列）；否则这四张卡不能计入“25–30 张可收”。

### B9. G2 用 `check_client_s2c_frames.py` 判 18 帧入图序必红
- Verdict: **FAIL**
- 证据：`check_client_s2c_frames.py:24` 的 `ENTER_SEQ` 固定为 `658,51,50,41,213,11029,42,766,54,5008,1106,52,752,811,46`；`:71-73` 用 `got[:len(ENTER_SEQ)] != ENTER_SEQ`。有装备/技能时正确序是 `52,752,621,211,811,46`（18 帧），第 15 位变成 621/211，工具必红。工具没有“可变入图序”参数。
- 更正：G2 的主判据改用 `s2c_diff.py`（对照同 DB 状态的 C# capture）或给工具加 `--expect-enter-seq`；G1 无装备时仍可用现工具。不要拿现工具的 GREEN 当 G2 的收卡证据。

### B10. `TestServer=TRUE` 部署无法收“非 TestServer”余量
- Verdict: **PARTIAL**
- 证据：`E:/MirServer/M2GameSvr/Server.conf:11-12` `TestServer=TRUE`、`TestLevel=1`；`openmir2-login-frame-order-52-752` 的 work 明确余量含“非 TestServer 部署/发光装备/IsSpirit 槽位变化”。G1/G2 没有构造发光装备/IsSpirit，也没有非 TestServer 影子。
- 更正：把该卡标为 partial；要收全，需 owner 授权非 TestServer 影子 conf 或另开部署窗口；否则窗口后保持 `in-progress` 并在 `work` 写明未取项。

### B11. 新账号 SQL 开号后必须重启 LoginSrv
- Verdict: **FAIL**
- 证据：`mir2-rs/tests/parity/m3/README.md:505`“`-ProvisionSql`（SQL 直开号）必须重启 LoginSrv 后才可登录”；`account_provision.ps1` 直接写 `mir2_account`；`start_rust_stack.ps1` 只在起栈时启动一次 C# LoginSrv。
- 更正：G1/G5 在 `account_provision.ps1` 后重启 LoginSrv（或用 `run_bots.ps1 -NewAccount` 让服务端在线建号）；账号名/角色名两侧分别命名（<=10 字符），避免第二次建角撞名。

### B12. G5 新号/DB 状态两侧共用，未做侧间还原
- Verdict: **PARTIAL**
- 证据：G5 只写“新号”，未给 C#/Rust 各自的角色名；两侧串行共用 `mir2_db`，C# 小退/关服会写回 `characters/characters_ablity/characters_item`，Rust 侧看到的已不是同一输入。
- 更正：C# 用 `g5c`、Rust 用 `g5r`（或每侧前 `dbsnap restore` 到同一基线）；在每侧登录前记录 `characters_item`/`characters_ablity` 的字段快照，再比 after。

## C. 判读工具是否真的能判

### C1. `check_attack_observability.py` 只数条数，判不了 SM_HIT(14) 的头/收件人/顺序/200ms
- Verdict: **FAIL**
- 证据：`check_attack_observability.py:104-145` 只统计 `CM_HIT/SM_STRUCK/SM_NOWDEATH` 的条数；`openmir2-monster-attack-hit-frame` 的 acceptance 明确“不能只数条数”，要断言 `SM_HIT` 的 Recog/Param/Tag、收件人集合、与 `SM_STRUCK` 的同 tick 顺序。G3 预期还写了“SM_HIT.Series 与 AttackDir 一致”和“~200ms 前同 Recog 的 SM_HIT”，现工具都不看。
- 更正：新增/复用帧级判据（`s2c_diff` 对照 C# capture，或写一个小 checker）检查 ident 14 的头部与收件人；用 `capture.jsonl` 的 `t_rel_us` 检查 200ms 先后。否则 G3 的 `monster-attack-hit-frame` 只能算“攻击计数通过”，不能算 R 收口。

### C2. 入站 `sm_struck>=1` 不是 `--expect any` 的判据
- Verdict: **FAIL**
- 证据：`check_attack_observability.py:120-129` 的 `expect=="any"` 只看出站 `struck`/`kills`；错误文本明确“入站受击不计”，`struck_in/kills_in` 只放在 facts 里。G3 的“入站 sm_struck>=1（主动族；空转=缺陷）”用 `--expect any --allow-unknown-self` 不会失败。
- 更正：加 `--expect incoming`（或加一个只判 `facts["sm_struck_incoming"]>=1` 的包装）；正式判据不要用 `--allow-unknown-self`（工具自己标为仅诊断）。

### C3. GuardList 判据不是帧级
- Verdict: **PARTIAL**
- 证据：G3 用 `aoi_visibility_pair.py --expect visible --min-pairs 1`，该工具数的是通用 `10+41` 出现对；W0-4 要的是 `SM_TURN(281,661)` + `SM_FEATURECHANGED` 的特定位点/recog。
- 更正：增加对 ident 10/41 的 recog/坐标过滤（或 `s2c_diff` 对照 C#），把 `aoi_visibility_pair` 只作辅助。

### C4. 所有时间判据都没有读取时间戳的判据
- Verdict: **FAIL**
- 证据：现列入的 checker（`check_client_s2c_frames`/`check_attack_observability`/`check_healthspellchanged_trigger`/`aoi_visibility_pair`/`check_aoi_visibility_diag`/`s2c_diff`）都是计数或序列；`capture.jsonl` 有 `t_rel_us`，但没有脚本用它判 200ms、1.2s、30s、60s、600/1200ms、3/5min、50/s vs 40/s。
- 更正：先补一个时间戳判据（或明确用日志时间 + `s2c_diff` 加自定义 gate）；在补之前，`monster-attack-cadence-lead`、`player-searchviewrange-cadence`、`aoi-verify-maptime`、`run-gap-player-run-quantification`、`rm-delay*`、`corpse-ghost-revive` 都只能算构造级/未判。

### C5. G5 只跑了 softclose，没跑三路径
- Verdict: **PARTIAL**
- 证据：`check_db_paths.ps1` 支持 `softclose|disconnect|shutdown`；run sheet G5 命令只给 `-Path softclose`，但文字说“三路径”。
- 更正：三条路径各自 snap before/after（`-SnapDir` 分开），再 `-Mode diff -CsharpDir ... -RustDir ... -Canonical`；否则 `save-misc-gaps`/`save-on-shutdown` 的三路径余量没收。

### C6. `scoped_diff.py diff` 少 `--out`
- Verdict: **FAIL（命令）**
- 证据：`mir2-rs/tests/parity/m3/scoped_diff.py:664` `d.add_argument("--out", required=True)`；run sheet 的“`scoped_diff.py diff --canonical --fixed-newchr <csharp> <rust>`”没有 `--out`，argparse 会直接报错。
- 更正：补 `--out E:/tmp/window/db/g5-diff.json`（或 `--summary-out` 对应的子命令参数），并在收窗证据里保留。

### C7. D-9 的 52 差异没有 whitelist 文件
- Verdict: **PARTIAL**
- 证据：`s2c_diff.py` 只支持 `--whitelist <jsonl>`，每条要有 `ident/field/reason/reviewer`；D-9 不会自动识别。run sheet 只写“按 D-9 排除”。
- 更正：建 `E:/tmp/window/s2c-whitelist.jsonl`，为 ident 52 的 `body_len`/`raw_hex` 登记 D-9 + reviewer，再传 `--whitelist`；否则 40B vs 58B 必红。

### C8. G7 没有网关判据，R 卡无法收口
- Verdict: **FAIL**
- 证据：run sheet §3.1 自认“无专用网关判据脚本”“只能看日志/截图”；但 `gate-idle-timeout`/`selgate-reconnect`/`gategate-slots-gm`/`gategate-speed-rules` 的 DoD 是时序/分支/队列级（0–210ms 丢帧、10ms 拍、>200ms drain、GM 10s 节拍）。日志/截图不能证明逐分支。
- 更正：给 LoginGate/SelGate/GameGate 7100/7200 加代理抓包或帧日志 gate，至少覆盖帧时间戳/丢弃/延迟队列；在此之前把 G7 从“一次窗口可复用”里去掉，四张卡保持 `in-progress`。

### C9. G6 的 6000/5100 帧没有抓取路径
- Verdict: **FAIL**
- 证据：`mir2_proxy` 的命令只 `--map 7200=...`；G6 要看的 1100 是 `DBR_LOADHUMANRCD`（DB 协议，6000 面），520/524/522/525 是 5100 面；客户端 7200 抓包看不到 1100/5100。
- 更正：要么在 6000/5100 加 server-server 代理/日志 gate，要么只判 DBSrv 日志中的协议字段；把“客户端降级帧”和“DB 协议帧”分开列。配合 A3 用 Rust db-srv 全栈。

## D. 分组与“构造级已覆盖”项

### D1. “87 张 R 卡”映射不完整且混入非 R 行
- Verdict: **FAIL**
- 证据：看板 135 in-progress；其中 23 张 issue 显式 `tier=R`、3 张显式 `tier=L`、109 张无 tier。附录表只有 87 行，另有 48 张 in-progress 未映射，包括 `openmir2-guardlist-guards-loading`（真机反向点）、`openmir2-npcs-fixed-loading`（NPC 出现帧）、`openmir2-mapcell-consumer-teeth`、`openmir2-mongen-terrain-loop-live`（G9 正文提到但表里没有）等；表内又有 8 张 G10 “合并检查点”（无 tier/无窗口余量）和 `openmir2-m5-convergence-drills` 的阶段0 L 描述。
- 更正：把附录改成“映射行（非全集）”，或补全 135 张并加 `issue_tier / window_item / no_window_reason` 三列；8 张 merge checkpoint 单独列“状态/已合并”，不计入 R 窗口卡；`run-gap-player-run-quantification` 的 B1 证据移到 G1/G3（不需要内容启用），不要放 G8。

### D2. 构造级已完成/不可达的项仍塞在窗口
- Verdict: **PARTIAL/FAIL**
- 证据/更正：
  - `openmir2-enter-map-cell-rejected-vs-oracle`：work 明确“当前生产无 switch_map 调用者 ⇒ 真机不可达”。从 G9 窗口项移除，只保留构造级。
  - `openmir2-rng-consumer-structure-parity`：work 标 `tier=L 片`、已 CONFIRMED；G9 只应“留痕”，不要当 R 窗口项。
  - `openmir2-movetomovingobject-quirk`（>50k 穷举）与 `openmir2-run-gap-pr1-head-blocking`（500 场景差分 0）：核心已构造级覆盖；只有“多对象同格/跨 actor 泵序”这类剩余项才进窗口，且要写清剩余项。
  - `openmir2-persist-item-attr-and-gold`：G-01 受 `B-117`、G-02 受 `B-116` 阻塞，窗口不会改变结论；从“可收”列表移除。
  - `openmir2-snap-cell-fidelity`：两个前置（.map 格子属性未进 world.maps、`apply_structure_events` 无生产调用方）未解；标 blocked。
  - `openmir2-gategate-speed-rules` 的 ItemSpeed/SpeedLimit 未接线：只能记录，不能判绿，卡保持 partial。

### D3. 组划分过粗，G3/G4/G6/G7 实际不可收
- Verdict: **FAIL**
- 更正：G3 拆 G3a/G3b/G3c（见 B7）；G4 在法术驱动落地前整组标 `not_collectible`；G6 在 Rust db-srv 全栈 + 6000/5100 gate 之前不可收；G7 在网关 gate 之前不可收。不要再用“G1+G2+G3+G5+G7 一次窗口最多复用”的表述。

### D4. 最小窗口的“25–30 张 R 卡”高估，2 小时方案自相矛盾
- Verdict: **FAIL**
- 证据：2 小时 fallback 写“G1 + G3（只 605,578 与 GuardList）+ G5 shutdown”，但 25–30 张清单里包含 G2 的 `sm-updateitem`/`sm-additem-tail`/`sm-merchantsay-recog`、G5 的 `persist-*` 等，这些在 fallback 里没有执行；G5 的 C# 关服方式又无效。
- 更正：按“构造 + 判据 + 授权”三列重列可收卡；以现工具计，完全可收的更接近 8–12 张（G1 的登录帧/53、G3 的 dir/guard/reverse/attack 计数、G5 的 Rust 关服/耐久），修完 C1/C2/C4 后可到 15–20 张。最小窗口建议改为 G0 + G1 + G3a + G5a（Rust game-svr graceful shutdown），G2 只做无行会/无动态物品的登录半，G6/G7 另开工具窗。

## E. 授权边界

### E1. 多张 G5 卡明确“不写生产库”，run sheet 却写 `mir2_db`
- Verdict: **FAIL**
- 证据：`openmir2-save-misc-gaps`、`openmir2-persist-create-rows`、`openmir2-persist-single-sided-columns`、`openmir2-save-on-shutdown` 的 acceptance 都写“不写生产库（测试库/替身）”；run sheet G5 直接对 `mir2_db` 做 DELETE/INSERT/关服写回，只靠 `dbsnap` 备份。
- 更正：克隆 `mir2_db`/`mir2_account`/`mir2_data` 到测试库（或独立 datadir），把 DBSrv/LoginSrv 配置指向测试库；若必须写生产库，先取得 owner 的书面授权，并在证据里写明授权条目。仅“有备份”不等于满足卡的验收边界。

### E2. G2 行会内容在最小窗口中未加条件
- Verdict: **PARTIAL**
- 更正：`guild-g2-dispatch` 的 750 依赖影子 `GuildBase` 内容（owner 授权）；最小窗口要么取得授权，要么把 750 从“可收”列表移除，只做无会对照。

### E3. G6 的 MySQL 停启是破坏性且命令不成立
- Verdict: **FAIL/conditional**
- 证据：本机 `Get-Service -Name *maria*,*mysql*` 无结果；`mysqld` 是 `D:\mysql\mariadb-10.11.19-winx64\bin\mysqld.exe` 进程（pid 19724），不是 Windows 服务；`start-all.ps1 -Stop` 会连带停它。
- 更正：用 `Stop-Process -Name mysqld` / `Start-Process ... mysqld.exe --datadir=D:\mysql\data --port=3306 --bind-address=127.0.0.1 --console`；确认 3306 独占、无其他 lane；owner 明确授权；不要用 `Get-Service/Start-Service`，也不要用 `start-all -Stop`。

### E4. 六件 provenance 不覆盖实际运行的 C# DBSrv/LoginSrv
- Verdict: **FAIL**
- 证据：混合栈的 6000/5100/5700/5500/5600 由 `src/DBSrv/bin/Release/DBSrv.exe` / `LoginSrv.exe` 提供；`check_csharp_capture_preflight.py` 只查 GameSvr 的 `!deployed-from.json` 与 RunGate conf，不查 DBSrv/LoginSrv；run sheet §3 只说“若 src 有改动须 dotnet build”，G0 没有这一步。
- 更正：G0 增加 C# DBSrv/LoginSrv 的构建/身份步骤（`dotnet build -c Release` + 记录 DLL hash/`!deployed-from`），或改用全 Rust 栈；不要把六件 Rust provenance 当成整栈身份。

## F. 其他具体更正

- **F1** `error_curve.py` 实际在 `tools/logdiff/error_curve.py`，不在 `tools/botload/`；G11 补全路径。
- **F2** G2 的 `355` 名称是“防御戒指”，不是“治愈戒指”；Shape=116 是 `RING_HEALING_ITEM`，按 Shape 判即可。
- **F3** G1 “无 45（新号首杀不得伪升级）”不是恒真：MaxExp=10 后首杀若经验 >=10 就会合法升级；要指定目标怪/经验并写期望。
- **F4** G3 的 `Level=30` 与上一轮 W0-1 的 level 1 基线不同；要明确这是新基线并重采，不要拿旧 level 1 证据混比。`monster-attack-cadence-lead` 还要两侧起手 HP 相同（登录后压到同值），否则“HP 未控”问题仍在。
- **F5** G7 的 `Stop-Process -Name DBSrv` 只匹配 C# `DBSrv`；全 Rust 栈进程是 `db-srv`。先写清当前栈，再给进程名/pid 文件。`Start-Process .../DBSrv.exe` 建议带 `-WorkingDirectory`。
- **F6** G7 的 `0–210ms` 与 `10ms/200ms` 判据没有注入/计时工具；`mir_flow` 无法精确控制登录后 0–210ms；需要网关侧时间戳或专门的注入脚本。
- **F7** G6 的“6000/5100 降级帧”中 1100 是 `DBR_LOADHUMANRCD`，不是客户端帧；不要和 520/524/522/525 混在一个“客户端表现”里。
- **F8** G5 的 `check_db_paths.ps1 -Mode diff` 需要 `-CsharpDir`/`-RustDir` 下存在 `<path>-after.jsonl`；三条路径都要先做 snap，否则 diff 会缺件。
- **F9** G8/G9 的 C# 影子内容：`start-csharp-hook.ps1` 每次都 `robocopy` 源到 `-Shadow`，拷完再改 `-Shadow` 会被下次启动覆盖；要用预制的影子 `-Pack` 或单独的手工起栈步骤。
- **F10** G9 的 `npc-ctor-draws` 还有“owner 裁决 8 处按 F 还是 M”的前置，run sheet 只列了 O1–O7；把这条加进 G9 硬前置。

## 重估后的最小窗口（建议）

- **G0**：同步镜像 `3bdfa0888` → 在镜像构建六件 → 录 provenance → guard；预计首次 30–45 分钟。
- **G1**：无装备/无技能 `aaa` 登录帧 + 53（HP<MaxHP）+ 新号 52（Weight=14/MaxExp=10）；约 40 分钟/两侧。新号开号后重启 LoginSrv。
- **G3a**：单客户端 `(605,578)` 攻击计数 + `(291,666)` Guard + `(311,666)` 反向 + 入站计数；约 40–60 分钟/侧。不要在本段宣称 SM_HIT/200ms/多客户端卡已收。
- **G5a**：Rust `start_rust_stack -Stop` 的 graceful 关服 + 新角色行/耐久；约 20–30 分钟。C# 关服另用优雅关闭，不要 `start-all -Stop`。
- **G2**：只做无行会、无动态物品的登录半（621/211、HumItems 1:1），并把 `check_client_s2c_frames` 换成 `s2c_diff`；约 20 分钟/侧。
- **G4/G6/G7**：先补法术驱动、全 Rust db-srv + 6000/5100 gate、网关 timer gate，再单独排期；不要塞进这次短窗。

预计这次短窗可完整收口的 R 卡约 **8–12 张**（G1 登录帧/53、G3 dir/Guard/反向/攻击计数、G5 Rust 关服/耐久），修完 C1/C2/C4 后可到 15–20 张；不是 run sheet 写的 25–30 张。
