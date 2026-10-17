# 82 — ③ 两侧对照窗口运行手册（`openmir2-client-e2e-enter-gaps`）

> 面向 **root 独占窗口**（2026-10-14 00:4x：「真机窗口由 root 开；判据/capture 工具你准备」）。
> 本手册把 ③ 收口所需的两侧对照做成**可照抄的步骤**，并显式登记唯一未证风险的环节（C# 侧抓包拓扑）。
> **车道自身不起栈**（m2-svr 纪律）；窗口里由 root 执行。

## 0. 要证的三件事（与 root 00:4x / 卡 DoD③ 逐条对应）

> ⚠ **片㊴（53 帧）追加前置**：①② 的「进图后 HUD 保持 / 能跑」在**满血进图**下证不出——**本窗口场景
> （进图后就地不动）**里 53 `SM_HEALTHSPELLCHANGED` 由**周期恢复段**发出，其前置为 `HP<MaxHP`。
> 本窗口**必须先压血**（§2b），并用 `check_healthspellchanged_trigger.py` 机判 T1/T2/T3。
> （oracle 另有不依赖该前置的 53 发点，见 `187-…md` §2 边界表；T1 取 `hp<max_hp` 是有意更严的**充分前置**。）

| # | 判据 | 期望读数 | 机判命令 |
| --- | --- | --- | --- |
| ①② | 53 帧按冻结客户端 `TCharDesc` 尾发（HUD 不归零 / 能跑） | 入图 `hp<max_hp`；s2c `53` ≥1 且 `body_len=12` | `check_healthspellchanged_trigger.py --log … --capture …` |
| ③a | 该点确实无怪（两侧一致） | 两侧 `appearance_pairs == 0`；Rust 服务端 `vis==0 && af==0` | `aoi_visibility_pair.py --expect none` + `check_aoi_visibility_diag.py --expect none` |
| ③b | 框内点 AOI 有算有下发（两侧一致） | 两侧 `appearance_pairs >= 1`；Rust 服务端 `vis>=1 && af>=2` | `aoi_visibility_pair.py --expect visible --min-pairs 1` |
| ③b | **能攻击造成伤害**（卡 DoD③ 第三半） | 两侧 c2s `CM_HIT(3014) >= 1` 且 s2c 有 `SM_STRUCK(31)` 或 `SM_NOWDEATH(34)` | `check_attack_observability.py --left … --right … --expect any --min-hits 1` |

坐标（用 `check_mongen_dense_points.py` 现场复核，别凭记忆）：

```powershell
python -I -B mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_mongen_dense_points.py `
  --mongen E:/MirServer/M2GameSvr/Envir/MonGen.txt --map 0 --view-range 12 `
  --points "291,666;291,651;289,618;287,620;650,631;330,266;311,666"
# 选点：窗口内期望出生怪数排行（片⑯；只算出生，不含游走）
python -I -B mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_mongen_dense_points.py `
  --mongen E:/MirServer/M2GameSvr/Envir/MonGen.txt --map 0 --view-range 12 --rank 10
```

- **用哪个「框内」点**：
  - 想一次拿**最多同屏怪** ⇒ `(605,578)`（片⑯ 实测 E=12.681，5 个重叠框中心；次密 `(211,629)`/`(256,572)` E=12.225）；
  - 想复用**既有证据** ⇒ `(289,618)`（= `Envir/StartPoint.txt` 的 oracle 出生点，E=6.754，也是新角色默认体验）
    或 `(287,620)`（片⑦ AFTER 已证客户端实收 13 对，E=5.617）；`(330,266)` 也框内（另一出生点）。
  - 两侧对照的**反向** ③a 点用 `(311,666)`（E=0.000，机判 `--max-expected 0`；**不得再用 `(291,666)`**：片68 起卫士 `(281,661)` d=10 ≤ vr=12 污染 ⇒ NOT_CLEAN，见 §2c/§2d W0-3）。
- **不要再用 `(291,666)` / `(291,651)` 当「有怪点」**：二者在 map "0" 的 **76 个生成框之外**
  （最近框 Chebyshev=30 / 29 > `ViewRange=12`）⇒ 该点同屏无怪是正确行为（见 `81-…txt` §B/§C）。
  `(291,666)` 现**只**用作 **GuardList 帧一致点**（§2d W0-4：卫士 `(281,661)` 两侧对称出帧），**不再**作 ③a 反向点（反向点已改 `(311,666)`）。

## 1. 前置（任一不满足就停手）

1. `E:/MirServer/port-status.ps1`：5000/5100/5500/5600/5700/6000/7000/7100/7200 全 DOWN（仅共享 3306 可 UP）。
2. 无残留 BotSrv / game-svr / cargo / rustc（`Get-Process`）。
3. `git fetch origin && git reset --hard origin/master`（窗口基 = 跑前 master oid，写进证据）。
4. 抓包栈一律**不用管道**：起栈命令 `... > <log> 2>&1`（禁 `| tee/tail/head/grep`，会把调用挂死）。
5. `characters_ablity.Hp < MaxHP`（§2b；否则本窗口场景下 53 永不发，①② 覆盖不到片㊴）。
6. **本机 mysqld 在听 3306**（这是**基线服务**、不是游戏端口）：`mir2-rs/tests/parity/m3/fixture_chain_gate.py check` 的**战斗 runner** 依赖 DB，未听时它会以 `ERROR 2002 ... (10061)` 报 `rc=1`——那是**环境未就绪**，不是代码红；先按本机 mysqld 配方（datadir `D:\mysql\data`、3306）恢复再跑门。

## 1b. 开窗前的前置门（`check_rust_window_preflight.py` R1..R8）与 **RED 恢复**（片64）

**开窗前必跑**（只读、零端口、零起栈；入库逐字替身 `client-e2e-window.fixed.sh.txt` 的 `guard`
子命令跑的就是它，且 `start` 在 `run_guard || exit 1` 处 fail-closed）：

```bash
bash mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/client-e2e-window.fixed.sh.txt guard
# ⇒ rc=0 打 WINDOW_GUARD_OK；rc!=0 打 WINDOW_PREFLIGHT_RED（不构建 / 不占端口 / 不暂停车队）
```

每条 RED 的**可照抄恢复动作**（片面命令；`R7`/`R8` 两条由工具自己打印到输出里，别再回翻证据包）：

| 门 | RED 含义 | 恢复动作 |
| --- | --- | --- |
| R1 | 构建源缺 HIGHHP 家族/③ 读数等修复提交 | `git fetch` + 把窗口源切到 `origin/master`（或含全部五片的 head） |
| R2 | 源码缺 `vr/new/af` 版式或可见集判据 | 源太旧 ⇒ 同 R1 |
| R3 | `MapInfo.txt` 第 N 个 section 的逻辑名 != 真机 `map_id` | 换 `--map-id/--map-name`，或核部署件 |
| R4 | 坐标与 MonGen 生成框不符 | 用 `check_mongen_dense_points.py --rank 10` 重新选点（③a 用 `(311,666)`） |
| R5 | 证据目录缺判据工具 | 补工具（不要拿老版式脚本顶上） |
| R6 | 构建源落后基线（生产路径有差异） | `git fetch` + `checkout --detach <基线>` + 重编译（跟 R7） |
| **R7** | 出处清单 ↔ exe 不一致 / 清单来自另一棵树 | **照抄工具打印的 `PREFLIGHT_RECORD_CMD:`**（重编六件 + 重录清单） |
| **R8** | 槽位被占（泄漏/修复前的栈） | **照抄工具打印的 `PREFLIGHT_STOP_CMD:`**（＝ `start_rust_stack.ps1 -LogDir <持有 pid 的目录> -Stop`） |

R7 的恢复命令形态（片64；`client-e2e-window.fixed.sh.txt guard` 会把它单独印一行）：

```bash
PREFLIGHT_RECORD_CMD: cd "<源>/mir2-rs" && CARGO_TARGET_DIR="<target-dir>" \
  cargo build --offline -p game-svr -p game-gate -p login-srv -p login-gate -p sel-gate -p db-srv && \
  python -I -B "<证据目录>/check_rust_window_preflight.py" --repo "<源>" --ref HEAD \
    --target-dir "<target-dir>" --record-provenance "<target-dir>/window-provenance.json"
```

- **为什么录制必须跟在编译成功之后**：`--record-provenance` 只是把**当前** `--ref` 盖到清单上，
  它**不校验** exe 是否真由该 ref 编出（这是该子命令的信任边界）⇒ 「编译失败也录制」会把旧件
  洗成新源、R7 随即假绿。故命令用 `&&` 串联：`cargo build` 非零 ⇒ 整条链短路，清单保持原样、
  R7 继续 RED。
- **`--record-provenance` 必须带值**（片65，reviewer-5 F-1）：它在本工具里是**带值**选项
  （`--record-provenance <输出清单路径>`，与 `--help` 用法一致）。旧版打印出的命令漏了该值 ⇒
  逐字节照抄得到 `error: argument --record-provenance: expected one argument`（rc=2）、R7 仍卡死。
  现在 selftest 有一条**执行性**牙 `r7-record-cmd-argv-parses`（把打印的尾部按 shell 词法切 token
  再喂给本工具自己的 argparse，不再只是子串断言）。
- **别用固定的 `E:/tmp/client-e2e/stack2` 当停栈目录**（片63）：`-LogDir` 必须是**持有占用端口那些 pid
  的** `rust-stack.pid.json` 所在目录（R8 会校验后才打印可照抄的命令；不持有则打 `<持有这些 pid 的目录>`）。
- **R6/R7 随每次落地转红**（2026-10-17 实测）：任何新提交都会让基线 `origin/master` 前移、已编 exe 的出处
  过期 ⇒ M1（对齐 mirror）+ M2（重编六件 + 重录清单）**不是一次性成就**。开窗前**最后一次**落地之后必须
  重跑二者，否则 guard RED（`R6: 构建源落后基线` / `R7: 出处清单 ↔ exe 不一致`）。

## 1c. 安装副本 ≠ 入库件（M4）与「真暂停」人工核对

**唯一真源 = 本仓库的入库件**：`mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/client-e2e-window.fixed.sh.txt`
（`guard` / `start` / `stop [LogDir]` 三子命令；`start` 在 `run_guard || exit 1` 处 fail-closed）。

| 维度 | 入库件（真源） | 安装副本 `/e/tmp/openmir2-watch/client-e2e-window.sh`（旧，2026-10-13） |
| --- | --- | --- |
| `guard` | **有**（R1..R8，零副作用） | **无**（可绕过前置门起窗） |
| `repo=` | `/e/tmp/m2svr-window-origin`（窗口 mirror） | `/e/Users/gxh/Documents/GitHub/OpenMir2`（**共享主检出**） |
| `tgt=` | `E:\tmp\m2svr-window-target` | `E:\tmp\openmir2-merge-target`（另一份 target） |
| `stop` | `stop [LogDir]`，与 `start` 同一 `repo`/`LogDir`（`stop` 不读 `tgt`） | **不接 `LogDir`**（起/停都硬编码 `E:\tmp\client-e2e\stack`） |
| `start` 收窗提示 | 指向**同一份入库件**的 `stop`（本片已改） | 指向旧脚本自身 |

**规则**

1. 起窗、收窗都用入库件，且**起/停同一 `-LogDir`**（R8 的 pid 文件目录；见 §1b / §W0-7）。
2. **不要用安装副本起/停窗**：它的 `start` 与 `stop` 都把 logdir 硬编码为 `E:\tmp\client-e2e\stack`
   （`start` 无 `$2` 分支、`stop` 无 `[ -n "${2:-}" ]`）⇒ 传 `-LogDir` 会被**静默忽略**，正是 21h 卡窗的根因。
   要用安装副本，先自行给它补 `LogDir` 支持；或只拿它做差异核对（下表命令）。同步它是 owner 动作，
   不在本 lane 写集；安装副本属 `/e/tmp` 运行副本，不得把它当判据。
3. **本方实测与镜像一致性（可复跑，只读）**：

```bash
REPO="$(git rev-parse --show-toplevel)/mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/client-e2e-window.fixed.sh.txt"
INST=/e/tmp/openmir2-watch/client-e2e-window.sh
sha256sum "$REPO" "$INST"
grep -nE '^w=|^repo=|^tgt=|^logdir=' "$REPO" "$INST"
diff -u --strip-trailing-cr "$REPO" "$INST" | head -120   # --strip-trailing-cr：两侧 CRLF/LF 不同不算漂移
# 入库件自身的收窗提示必须指向入库件（判据行，不能指向 $w/client-e2e-window.sh）：
grep -n "WINDOW_OPEN_OK" "$REPO"
```

判读：`sha256sum` 两值**不必相等**（两个文件本来就不同）；判据是 `grep` 的 `repo=`/`tgt=` 行是否符合上表、
`diff` 是否只剩已知差异（`guard` 整段缺失、`repo=`/`tgt=`、`stop` 分支、提示行、2026-10-17 新增的 5 行
「安装副本 ≠ 入库件」注释；出现其它 hunk ⇒ 安装副本已漂移），以及 `WINDOW_OPEN_OK` 那行引用的是
`$here/client-e2e-window.fixed.sh.txt`。若 `INST` 不存在 ⇒ `No such file or directory`，视为「未安装」，
仍以入库件为准。

**「暂停车队」不等于「停掉所有写手」（当前编排差异）**

- `lanes-pause.sh` 只对 **`node.exe` 且命令行含 `Tag: <tag>`** 的进程做 `Stop-Process`；
- 本机当前编排是 **`claude.exe`**（不是 node），并会拉起 `cargo`/`rustc` 构建；`lanes.conf` 最后修改
  2026-10-14，活动车道标签也可能不含在跑的车道；
- ⇒ 窗口前必须**人工核对并停掉 `cargo` / `rustc` / `claude` 写手**（以及任何 `game-svr`/`GameSvr`/`BotSrv`），
  再复查端口；**不能**只凭 `lanes-pause` 返回就判定「车队已停」：

```powershell
Get-CimInstance Win32_Process |
  ? { $_.Name -match 'claude|node|cargo|rustc|GameSvr|game-svr|DBSrv|BotSrv' } |
  Select-Object ProcessId,ParentProcessId,Name,CreationDate,CommandLine | Format-Table -Auto
# 判据：开工时该表只剩本窗口自己的进程 / 必要基础服务（mysqld）；有 cargo/rustc/claude 写手 ⇒ 先停。
```

## 2. 站位（两侧必须逐字节同一坐标）

`mir2_db.characters.aaa` 的坐标就是进世界起点（真机日志 `进世界 ChrName:aaa CurMap:0 (x,y)`）。

> ⚠ **列名口径（片㉗ 订正）**：`characters` 表**没有** `x`/`y` 列；实测 `SHOW COLUMNS FROM mir2_db.characters`
> 只有 `MapName`(varchar) / `CX`(int) / `CY`(int)。旧写法 `SET x=…, y=…` 会 `ERROR 1054 (42S22):
> Unknown column 'x' in 'field list'`，**角色不移动** ⇒ 两趟都停在旧点位、窗口白烧（见
> `135-window-plan-position-columns.txt`）。且必须钉 `MapName='0'`，否则两侧可能不同图。
> 命令行（本机 mysql CLI 与 `run_pair.ps1:127` 同源；`--password=` = 空口令）：

```powershell
$mysql = 'D:/mysql/mariadb-10.11.19-winx64/bin/mysql.exe'
$margs = @('-h','127.0.0.1','-P','3306','-u','root','--password=','--batch','--skip-column-names')

# 1a. 每趟起栈前先记 before（还原用）
& $mysql @margs -e "SELECT MapName,CX,CY FROM mir2_db.characters WHERE ChrName='aaa';"
# 1b. 有怪点（片⑯ 最密）
& $mysql @margs -e "UPDATE mir2_db.characters SET MapName='0', CX=605, CY=578 WHERE ChrName='aaa';"
# 1c. 有怪点（保守：oracle 出生点 / 片⑦ 已证点，二选一）
& $mysql @margs -e "UPDATE mir2_db.characters SET MapName='0', CX=289, CY=618 WHERE ChrName='aaa';"   # 或 287,620
# 1d. 无怪点（③a 反向 = (311,666)；片68 起 (291,666) 被卫士污染 ⇒ 改 311；第二趟起栈前改）
& $mysql @margs -e "UPDATE mir2_db.characters SET MapName='0', CX=311, CY=666 WHERE ChrName='aaa';"
# 收工还原：把 1a 打印的 before 三值填回
& $mysql @margs -e "UPDATE mir2_db.characters SET MapName=BEFORE_MAP, CX=BEFORE_X, CY=BEFORE_Y WHERE ChrName='aaa';"
```

⚠ **每一趟起栈前都要重跑站位**：上一趟客户端走动 / 断线存档会改写 `aaa` 的坐标。跑完还原到
开工前值并记录（2026-10-14 实测开工前 = `MapName='0' CX=297 CY=629`；若已漂移以 §1a 的 before 为准）。

### 2b. 53 触发前置（片㊴ ①/② R 判定的**必要条件**）

片㊴ 的真凶是 `SM_HEALTHSPELLCHANGED(53)` 的布局：冻结客户端 `HIGHHP=True` 从**尾**解 12B
`TCharDesc`，而 oracle 发 header-only ⇒ 解出 `HP/MP/MaxHP=0`（HUD `0/0`、`CanRun` 拒跑）。
**本窗口场景（进图后就地不动、同一 socket、不战斗）里，该帧由周期恢复段发出，其前置为 `HP < MaxHP`**
（`BaseObject.Base.cs:44-70` 的恢复段；Rust 三个生产调用点也都只在血量/魔法变化时触发）
⇒ **满血进图永不发 53**，窗口既证不了修复也复现不了回归（既有 7 份真机 capture 里 53 计数**全为 0** 即此因，
见 `187-healthspellchanged-trigger-precondition.md`）。
> 措辞边界：**不要**读成「53 只在 `HP<MaxHP` 时发出」——oracle 另有非该前置的发点
> （`BaseObject.Base.cs:109` 饥饿 [守卫 `HP>1`]、`:244` `HP>MaxHP` 重算、`:628` `RM_MAGSTRUCK`、
> `BaseObject.cs:973-975` `IncHealthSpell` 无条件）；T1 取 `hp<max_hp` 是本「进图不动」窗口的**充分前置**。
**每趟起栈前压血**：

```powershell
$mysql = 'D:/mysql/mariadb-10.11.19-winx64/bin/mysql.exe'
$margs = @('-h','127.0.0.1','-P','3306','-u','root','--password=','--batch','--skip-column-names')
# before（还原用）：Hp / MaxHP
& $mysql @margs -e "SELECT c.ChrName,a.Hp,a.MaxHP FROM mir2_db.characters c JOIN mir2_db.characters_ablity a ON a.PlayerId=c.Id WHERE c.ChrName='aaa';"
# 压血：Hp = MaxHP - 2（留两格便于观察回血；smallint，MaxHP>=3 才动）
& $mysql @margs -e "UPDATE mir2_db.characters_ablity a JOIN mir2_db.characters c ON a.PlayerId=c.Id SET a.Hp = a.MaxHP - 2 WHERE c.ChrName='aaa' AND a.MaxHP >= 3;"
# 收工还原：把 before 的 Hp/MaxHP 写回
```

> ⚠ `characters_ablity` 的主键是 `PlayerId`（= `characters.Id`），**不是** `ChrName`；且 HP 列在
> `characters_ablity.Hp` / `characters_ablity.MaxHP`（`sql/mir2_db.sql:6073`），**不在** `characters`。

进世界后机判（T1 前置 + T2 已发 + T3 尾长）：

```powershell
cd mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013
python -I -B check_healthspellchanged_trigger.py `
  --log E:/tmp/window/cliente2e/rust/stack/game-svr.out.log `
  --capture E:/tmp/window/cliente2e/rust/s2c/capture.jsonl
```

**改坏必红**：拿修复前二进制（master `8e491f81e`）跑同一窗口 ⇒ T2 命中（53 发过）但 **T3 RED**
（`body_len=0` = oracle header-only）⇒ 客户端 HUD `0/0` + `CM_RUN=0`；修复后（`24725dfd5`+）T3 GREEN
（`body_len=12`）。用户级读数仍是 root 的 HUD 截图 + `check_client_run_frames.py` 的 `CM_RUN` 计数。

## 3. Rust 侧（已证可跑，照抄）

```powershell
# 3.1 整栈（Rust game-svr 5000 + Rust 三网关 7000/7100/7200 + C# DBSrv/LoginSrv + MySQL 3306）
# ⚠ target 唯一真源 = E:/tmp/m2svr-window-target（与入库窗口脚本的 tgt、window_handoff.py 的默认
#   --target-dir、R7 的 window-provenance.json 同路径）。**check_rust_window_preflight.py 本身无默认**：
#   入库窗口脚本的 preflight_cmd 会显式传 --target-dir/--provenance；手跑不传时 R7 只打印 INFO 跳过、
#   该门自身不判红（其余门绿时末尾仍是 `RESULT: GREEN`；假绿陷阱）——手跑必须显式带这两个参数。
#   旧文里的另一条 target 名（与本真源不同）是笔误，已删；不要再另建第二份 target。
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 `
  -RustBinDir E:/tmp/m2svr-window-target/debug -Seed 42 -LogDir E:/tmp/window/cliente2e/rust/stack

# 3.2 把真 game-gate(7200) 停掉，起**影子** game-gate(7600)（conf 造法见 §4.0：拷 `RunGate/config.conf`、
#     用 cp936 原样只改 Count/GatePort1 两个数字（逐字节只差 2 字节）；Rust 侧镜像同一份 conf）
#     再让代理占官方 7200：
python tools/capture/mir2_proxy.py --out E:/tmp/window/cliente2e/rust/proxy --map 7200=127.0.0.1:7600

# 3.3 真客户端（同一 MirClinet.exe / 同一账号 mir2test / 角色 aaa）
#     `-AttackClicks 12` = 进世界后绕角色点 12 下（点到怪 = c2s CM_HIT，覆盖 DoD③「能攻击造成伤害」；
#     点到地 = CM_WALK）。若 12 下全未命中，可加 `-AttackRing` 自定义环。
powershell -ExecutionPolicy Bypass -File tools/client/mir_flow.ps1 `
  -Account mir2test -Password mir2pass -OutDir E:/tmp/window/cliente2e/rust/shots `
  -WalkClicks 0 -AutoPlaySec 12 -AttackClicks 12 -SkipReenter -SkipLogout -SkipTalk

# 3.4 解码客户端收到的 s2c 字节
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/export_s2c.ps1 `
  -ProxyDir E:/tmp/window/cliente2e/rust/proxy -OutDir E:/tmp/window/cliente2e/rust/s2c
```

Rust 侧判据（三条）：

```powershell
cd mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013
# 服务端：新读数 vr/new/af（老二进制没有这三个字段 ⇒ PREREQ_RED，属正确 fail-closed）
python -I -B check_aoi_visibility_diag.py --log E:/tmp/window/cliente2e/rust/stack/game-svr.out.log `
  --expect visible --min-vis 1
# 客户端：出现帧对 >= 1
python -I -B check_client_s2c_frames.py --capture E:/tmp/window/cliente2e/rust/s2c/capture.jsonl `
  --min-appearance-pairs 1
# 攻击/伤害（DoD③ 第三半）：c2s CM_HIT>=1 且 s2c SM_STRUCK/SM_NOWDEATH（单侧先自证 Rust）
python -I -B check_attack_observability.py --capture E:/tmp/window/cliente2e/rust/s2c/capture.jsonl `
  --expect any --min-hits 1
# 截图留证（屏幕上要能看到怪）
```

## 4. C# 侧（先读 `85-csharp-capture-topology-rootcause.md` 再动手）

C 线（2026-10-10）把「`CM_LOGINNOTICEOK` 之后世界数据下不来」归因于**影子网关端口被挪**；
`85-…md` 按当前源码逐路径复核**推翻了该归因**：

- `!servertable.txt` 只在**多机转移**路径被消费（`GameShare.GetMultiServerAddrPort` →
  `AppService.cs:221` 关机转移 / `WorldServer.cs:1063-1074` `SM_RECONNECT`）；单机进世界不读它。
- GameSvr 对网关按**槽位**寻址（`TCPNetChannel` 的 `_gateSlotByConnection`/`_gameGates`），
  世界侧 `GateIdx` = 网关在认证串里自报的 `ServiceId`（`ClientSession.cs:1203`），**与端口无关**。
- 同症状的确定根因是 **S1 缺陷①**（槽位按 accept 序 + `CloseGate` 不释放槽位），
  修复 `607e9b773`（2026-10-10 17:01）**已进部署件**（`!deployed-from.json` = master `628d8024`）。

### 4.0 影子 conf + 前置门（起 C# 栈之前先跑；离线、零端口、fail-closed）

⚠ oracle 的 conf 路径 = `Path.Combine(AppContext.BaseDirectory, 'config.conf')`
（`src/GameGate/Conf/ConfigManager.cs:5`；`appsettings.json` 同法，`Program.cs:31`），且 `Main`
**不吃 conf 参数** ⇒ 影子必须是**自己的目录 + 自己目录下的 config.conf**。

```powershell
# 1) 共用一份影子 conf：从部署件拷、cp936 原样只改 Count 与 GatePort1 两个数字
#    （UTF-8 读写会改坏 [MagicInterval]/[Strings] 的中文 GBK 字节；直接照抄 Count=3 会再绑 7201/7202）
$enc = [Text.Encoding]::GetEncoding(936)
New-Item -ItemType Directory -Force 'E:/tmp/window/cliente2e/shadow' | Out-Null
Copy-Item -LiteralPath 'E:\MirServer\RunGate\config.conf' -Destination 'E:/tmp/window/cliente2e/shadow/game-gate.conf' -Force
$t = [IO.File]::ReadAllText('E:/tmp/window/cliente2e/shadow/game-gate.conf', $enc)
$t = $t -replace '(?m)(?<=^Count=)\d+(?=\r?$)','1' -replace '(?m)(?<=^GatePort1=)\d+(?=\r?$)','7600'
[IO.File]::WriteAllText('E:/tmp/window/cliente2e/shadow/game-gate.conf', $t, $enc)
Select-String -Path 'E:/tmp/window/cliente2e/shadow/game-gate.conf' -Pattern '^Count=|^GatePort1='

# 2) 影子 RunGate = 整目录拷贝（exe/dll/appsettings/nlog 全要），config.conf 换成上面那份
Remove-Item -Recurse -Force 'E:/tmp/window/cliente2e/shadow/run-gate' -EA SilentlyContinue
Copy-Item -Recurse -Force 'E:\MirServer\RunGate' 'E:/tmp/window/cliente2e/shadow/run-gate'
Copy-Item -Force 'E:/tmp/window/cliente2e/shadow/game-gate.conf' 'E:/tmp/window/cliente2e/shadow/run-gate/config.conf'

# 3) 前置门
cd mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013
python -I -B check_csharp_capture_preflight.py `
  --repo E:/Users/gxh/Documents/GitHub/OpenMir2 `
  --mirserver E:/MirServer --shadow-config E:/tmp/window/cliente2e/shadow/run-gate/config.conf
```

退出码 `0`=GREEN / `1`=RED / `2`=PREREQ_RED。它咬五条：部署件含 `607e9b773`；真实与影子
`ServerWorkThread=1`（S1 §2.1 多网关错配边界）；真实 `GatePort1=7200`；`!servertable.txt`
GameGate 端口 `=7200`；上游 `127.0.0.1:5000`。**红灯先修，不要带着红灯起栈。**
（片㉖ 实测：把 `--shadow-config` 指到真实 conf ⇒ `RED 影子 GatePort1=7200 != 影子端口 7600`。）
（**片D3 订正**：`--repo` 必须是**含修复提交 `607e9b773` 的检出**；旧写死的 `.claude/worktrees/m2-svr` 路径已不存在 ⇒ 逐字照抄会 `PREREQ_RED` rc=2。改用主检出 `E:/Users/gxh/Documents/GitHub/OpenMir2` 实测 rc=0。）

### 4.1 影子 RunGate 配置（片㉖ 订正：**整目录拷贝**）

挪法见 §4.0：**整目录拷贝** `RunGate/` 到影子目录，再把 `config.conf` 换成「cp936 原样、只改
`Count=1`/`GatePort1=7600`」那份。不这么做（只拷 config.conf、仍起 `E:\MirServer\RunGate\GameGate.exe`）
⇒ oracle 按 `AppContext.BaseDirectory` 读回原 config ⇒ 影子仍绑 7200（代理绑不上），且前置门会**误判 GREEN**
（false-green）。绑定地址、上游地址、`ServerWorkThread=1` 一律不动；**不要**引入第 2/3 个活动网关
（本部署本就 `ServerWorkThread=1`）。

- **首选（需现场验证）**：照 §3 **只挪 game-gate**（真 LoginGate/SelGate 保持 7000/7100 不代理，
  game-gate 影子挪到 7600，代理占 7200）。Rust 侧同拓扑能拿到世界数据（§3 已证），
  C# 侧**仍须在窗口里实拍确认**（看代理 `chunks.ndjson` 里 7200 跳在公告之后是否还有 s2c 字节）——
  本片只把「端口=根因」这个旧归因订正掉，**不把「应该能」写成「已经能」**。
- **若 C# 世界数据仍下不来**（老现象复现，且 §4.0 前置门已 GREEN）⇒ 按 `C线-交付说明.md`「解除判据」#1：
  让 C# 自己把网关挂在可插桩端口，并把 `!servertable.txt` 与 LoginSrv `AddrTable.txt` 一起指向该端口。
  **不要在窗口里现造**——这是 oracle 侧配置改动，属 §6.0-3 登记项，先记 `comment` 请 owner 裁决。
- **降级判据（不依赖抓包）**：C# 与 Rust 各在**同一坐标**起一次真客户端，各留 `mir_flow.ps1` 截图
  （`-OutDir`），人眼/像素比对「屏幕上是否有怪」。这仍满足 root 的「同一位置、同一 viewrange 两侧对照」，
  但没有逐帧计数；若走降级路，必须在卡上写明「两侧对照 = 截图级（抓包拓扑受限）」。

## 5. 两侧对照（机判）

```powershell
cd mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013
# ③b 有怪点：两侧都 >=1 对 ⇒ AOI 有算有下发（④ 反证）
python -I -B aoi_visibility_pair.py `
  --left E:/tmp/window/cliente2e/rust/s2c/capture.jsonl `
  --right E:/tmp/window/cliente2e/csharp/s2c/capture.jsonl `
  --expect visible --min-pairs 1
# ③a 无怪点（311,666）：两侧都 0 对 ⇒ 「该点确实无怪」
python -I -B aoi_visibility_pair.py `
  --left E:/tmp/window/cliente2e/rust/s2c/capture.jsonl `
  --right E:/tmp/window/cliente2e/csharp/s2c/capture.jsonl --expect none
# ③b 攻击/伤害（两侧都 >=1 击且都看得到伤害帧）
python -I -B check_attack_observability.py `
  --left E:/tmp/window/cliente2e/rust/s2c/capture.jsonl `
  --right E:/tmp/window/cliente2e/csharp/s2c/capture.jsonl --expect any --min-hits 1
```

工具口径：默认按**计数**判（`--min-pairs`），`--strict-recogs`（默认关）会额外要求两侧 actor 集合逐项相等
（同图同种子才成立；C# 与 Rust 同种子由 `-Seed`/C# 侧 hook 注入保证，见 `openmir2-m3-rust-stack-seed-plumb`）。

## 6. 收工（DoD）

1. 停栈：`start_rust_stack.ps1 -Stop`；C# 侧按既有 `-Stop`；确认 5000/5100/5500/5600/5700/6000/7000/7100/7200 全 DOWN
   （仅共享 3306 可 UP）——实拍 `E:/MirServer/port-status.ps1`。
2. 还原 `characters.aaa` 到开工前坐标，记录 before/after。
3. 证据四件套（命令 + 输出 + 路径 + sha256；`SHA256SUMS` 按 LF/git-blob 口径用
   `python -I tools/evidence-hash/verify_sha256sums.py write <dir>` 生成并 `verify` 自检）；
   证据入 `mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/`。
4. 卡上 `patch`（base=跑前 master、head=你的提交）+ `status: in-progress`（work 写 ③a/③b 结论）。
5. 若走降级（截图级）⇒ 证据里如实写清，不许把截图级写成帧级。

## 7. 登记的风险 / 未做

- **③b 攻击/伤害（片㉜）**：判据 = `check_attack_observability.py`（c2s `CM_HIT(3014)` + s2c
  `SM_STRUCK(31)`/`SM_NOWDEATH(34)`）。生产侧同片修了 oracle 偏差：`mir2-rs/bins/game-svr` 的
  `drain_combat_downlink` ① 原先只把 `SM_STRUCK` 发给**受击者本人**，而 oracle
  （`BaseObject.Attack.cs:57` → `BaseObject.Base.cs:699-705` → `BaseObject.Message.cs:383+`）
  是 `SendRefMsg(RM_STRUCK)` **广播给受击者视野内全体 `Play`**（怪物目标也一样）⇒ 不修则真客户端
  看不到怪掉血/伤害。**登记（未 1:1）**：oracle 该帧经受击者 `MsgQueue` 以 `LateDelivery=200ms`
  延时投递，本实现即时发；本次不改。
- **C# 抓包拓扑**：旧归因（端口被挪）已由 `85-…md` 订正为 S1 缺陷①（已修 + 已部署）；
  §4.0 前置门可离线机判。**但世界数据能否经代理下来仍须窗口实拍**；未实拍前不得写「两侧帧级一致」。
- `RM_DISAPPEAR(48)` 无编码器（怪离视野无消失帧，`mir2-rs/bins/game-svr/src/downlink.rs` 已登记）与 ③ 无因果，
  本手册不含；归属见 `70-…txt` §F（等 owner 指派）。
  ⚠ **订正（2026-10-16）**：片103（`139ccedd3`）已接 `SM_DISAPPEAR(30)`（`downlink.rs:434-439` `disappear_frame`）⇒
  「怪离视野无消失帧」**已不成立**；本行只余「与 ③ 无因果、本手册不含」有效。
- **53 触发前置（片㊴，2026-10-14 收口）**：本手册旧版未要求 `HP<MaxHP` ⇒ 满血进图 53 永不发
  （既有 7 份 capture 全 0 条）。已补 §2b + 机判 `check_healthspellchanged_trigger.py`（T1/T2/T3，
  selftest 8/8，含「满血 ⇒ T1 RED / header-only ⇒ T3 RED」两向牙）；证据 `187-…md`。
- 本手册**零生产代码改动**；只补判据工具（`check_mongen_dense_points.py`、`check_csharp_capture_preflight.py`、
  `check_healthspellchanged_trigger.py`）与运行步骤。

## §2c 返工后的 ③b/③a 执行口径（片68，2026-10-15）

- **③b 攻击步**：两侧**必须钉同一 `(x,y)`**（每趟起栈前 `UPDATE characters SET MapName='0', CX=…, CY=…`，全程不让其漂移）；
  动作序列 = 先 `CM_TURN(dirToMonster)` 再 `CM_HIT(MakeLong(自x,自y), tag=dirToMonster)`——真客户端空地攻击**不先发** `CM_TURN`，
  单发换向环点每一击都用「上一动作遗留朝向」取前方格 ⇒ 必打空（Rust 与 oracle 同构，见 README §68）。
  快捷键：**`-AttackTurn "dx,dy"`** 一步到位（`mir_flow.ps1`，左键攻击前先**右键相邻格格心**触发 `CM_TURN`；
  `dx,dy`=指向怪的相邻格偏移『`1,0`东/`-1,0`西/`0,-1`北/`0,1`南/对角』，`|dx|,|dy|≤1`，`(0,0)` 非法）；
  或 `-AttackRing` 给**全同单向**且方向 = 上一步走步方向。**（`-AttackTurn` 已合入，W0 统一用它——见 §2d W0-1/2。）**
- **③b 第一步 = 阳性对照（C# 侧）**：两侧至今没有任何「玩家→怪」伤害帧 ⇒ 先在 **C#（oracle）** 侧打出
  **victim=怪** 的 `SM_STRUCK`，把 GREEN 定义钉在实拍足迹上，再比 Rust。
- **③b 判据**：`check_attack_observability.py`（已带 victim 过滤，出站 = victim ≠ 自己）。**旧版**会把「玩家被怪围殴」读成「玩家造成伤害」。
- **探针（未决项 → 2026-10-16 订正）**：玩家贴怪旁静止 ~10s，看 Rust 侧有无 **victim=自己** 的 `SM_STRUCK`（怪打玩家）。无 ⇒ 另立卡查怪物 AI 攻击支。
  ⚠ **订正（2026-10-16）**：**「主动索敌/游走攻击未实现」的旧登记已被片88（`b22295b1e`）推翻**——`crates/world/src/monai.rs`
  的 `monster_run` 现按时钟门调 `search_target`（`monai.rs:189`），门控 `is_active_seek_race`（`monai.rs:555`，
  = 不覆写 `Run` 的 `ATMonster` 族 {53,81,83,86,88,89}）。故测试点怪属**主动族**时，Rust 侧探针**应出** victim=自己的
  `SM_STRUCK`；**空转 = 缺陷**（不再判「可能仍空转、非缺陷」）。非主动族（`is_active_seek_race==false`）才可能合理地无主动攻击。
  判读前仍须先确认 oracle 侧该怪是否主动攻击（详见 §2d W0-5）。
- **③a 反向点**：不得再用 `(291,666)`（`GuardList` 卫士 `(281,661)` d=10 污染）；先过
  `check_reverse_point_clean.py --point <x,y> --map 0 --vr 12`（fail-closed，缺源文件即 RED），
  **建议 `(311,666)`**（最近确定性对象 d=30）。
  ⚠ **更新（2026-10-16，GuardList 已实现）**：Rust 现已加载 `GuardList.txt`（`mir2-rs/crates/envir/src/guardlist.rs` +
  `mir2-rs/bins/game-svr/src/guardlist.rs`，白名单 T-56 对应卡已落）⇒ `(291,666)` 上卫士 `(281,661)` 现**两侧都应有**
  （片68 时是『C# 有、Rust 无』的**对称性已修复**态）。故 `(291,666)` **仍不可**当反向点（仍污染），但它现在正好是
  **GuardList 帧一致的验证点**（两侧对称出帧，见 §2d W0-4）。
- **客户端驱动**：`mir_flow.ps1 … -CenterY 259`（角色屏幕中心；默认 `384` 会把命中环整体偏下 ~2 格）。

## §2d W0 单窗收口流程（2026-10-16）

> **W0 = 一个独占窗口内串行收口五件事**：③b「玩家→怪造成伤害」、③a 反向点、GuardList 帧一致、
> 「怪→玩家」探针、RNG 双侧账本。**顺序不可换**：② 阳性对照必须**最先**跑，它的产出是后续各项的 GREEN 定义。
> 车道自身不起栈（m2-svr 纪律），窗口由 root/owner 开。以下命令「就位即可照抄」，含期望值与失败分叉。

### W0-00 开窗首步三连（M10：端口 → guard → C# 抓包前置；顺序不可换）

> 只读、零起栈；三条全绿才进入 W0-0/W0-1。命令在**仓库根**执行（`check_csharp_capture_preflight.py`
> 的仓内路径按仓库根写；`--repo` 显式给当前检出，别用已不存在的旧 worktree 路径）。

```powershell
# ① 十端口基线：九个游戏端口 DOWN，仅 3306 UP
powershell -NoProfile -ExecutionPolicy Bypass -File E:/MirServer/port-status.ps1
# 期望：报告「缺失端口」= 5000,5100,5500,5600,5700,6000,7000,7100,7200；
#       进程表只有 mysqld ... port=3306（以及本步自己的 powershell）。

# ② Rust 窗口前置门（R1..R8；只读、零端口）
bash mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/client-e2e-window.fixed.sh.txt guard
# 期望 rc=0，末行 WINDOW_GUARD_OK。RED ⇒ 按 §1b 的 R1..R8 表恢复；
#       绝不允许绕过 guard 改用安装副本起窗（§1c）。

# ③ C# 抓包前置（含影子网关 conf；离线、零端口、fail-closed）
python -I -B mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_csharp_capture_preflight.py `
  --repo E:/Users/gxh/Documents/GitHub/OpenMir2 --mirserver E:/MirServer `
  --shadow-config E:/tmp/window/cliente2e/shadow/run-gate/config.conf
# 期望 rc=0 且 RESULT: GREEN（当前已 GREEN）。
```

**判定**：① 端口表 9 DOWN / 3306 UP；② 打印 `WINDOW_GUARD_OK`；③ 打印 `RESULT: GREEN`。
任一不满足 ⇒ **停手**，不要进入 W0-0（失败分叉见 §1b / §4.0）。

### W0-0 前提（任一不满足即停手；全程零副作用）

1. **十端口实拍**（含共享 3306）：`powershell -File E:\MirServer\port-status.ps1`
   期望：**九个游戏端口 5000/5100/5500/5600/5700/6000/7000/7100/7200 全 DOWN**（报告里列为「缺失端口」），
   仅 **3306 可 UP**（MySQL 共享）。任一游戏端口 UP ⇒ 有泄漏栈，先做 W0-0.2。
2. **无泄漏栈（R8）**：`bash mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/client-e2e-window.fixed.sh.txt guard`
   期望 rc=0、打 `WINDOW_GUARD_OK`；R8 RED ⇒ **照抄它打印的 `PREFLIGHT_STOP_CMD:`**（＝
   `start_rust_stack.ps1 -LogDir <持有 pid 的目录> -Stop`）。**起/停必须用同一个 `-LogDir`**（历史 21h 卡窗根因，见 `docs/交接-OpenMir2编排与联调-20261015.md` §1.2）。
3. **车队已停确认（不能只信 `lanes-pause`）**：窗口脚本 `start` 第 1 步即 `lanes-pause.sh`
   （暂停车队＝其它 agent 车道的在跑实例）。
   ⚠ 当前 `lanes-pause.sh` **只杀 `node.exe`（且命令行含 `Tag:`）**；本机编排是 **`claude.exe`** +
   其拉起的 `cargo`/`rustc`，且 `lanes.conf`（最后修改 2026-10-14）可能不含在跑车道 ⇒
   `lanes-pause` 返回**不等于**写手已停。人工开窗的等价动作＝
   ① 按 §1c 的 `Get-CimInstance` 清单核对并停掉 `cargo`/`rustc`/`claude` 写手与残留 `BotSrv`/`game-svr`；
   ② 再跑 `port-status.ps1` 确认 9 个游戏端口 DOWN；③ 确认没有别的车道在写 `mir2_db`。
4. **角色摆位 SQL（两侧必须逐字同一坐标）**——每趟起栈前重跑（形态见 §2）：
   ```powershell
   $mysql='D:/mysql/mariadb-10.11.19-winx64/bin/mysql.exe'
   $margs=@('-h','127.0.0.1','-P','3306','-u','root','--password=','--batch','--skip-column-names')
   & $mysql @margs -e "SELECT MapName,CX,CY FROM mir2_db.characters WHERE ChrName='aaa';"                 # before（还原用）
   & $mysql @margs -e "UPDATE mir2_db.characters SET MapName='0', CX=605, CY=578 WHERE ChrName='aaa';"   # ③b 有怪点（片⑯ 最密）
   ```
   ⚠ 列名是 `MapName/CX/CY`（**没有** `x/y`，片㉗），且必钉 `MapName='0'`；跑完还原到 before。
   另按 **§2b 压血**（`Hp=MaxHP-2`）——本窗口**仍要**覆盖 ①②（53 帧 T1/T2/T3）。
5. **二进制身份留证**：`guard` 通过即 R7 出处一致（`window-provenance.json`）；把该清单＋六件 exe 的 sha256 落证据。
   ⚠ `--record-provenance` 只把当前 `--ref` **盖上**、**不校验** exe 真由该 ref 编出 ⇒ 录制必须跟在 `cargo build`
   成功之后（§1b 的 `&&` 链）——否则「编译失败也录制」会把旧件洗成新源、R7 假绿。

### W0-1 第一步：阳性对照（**C# 侧先打出** victim=怪 的 `SM_STRUCK`）

> **为什么最先**：至今**两侧都没有过「玩家→怪」伤害帧**（两侧 `SM_STRUCK` 里 victim≠自己 的条数都是 **0**，见 README §68）。
> ⇒「能攻击造成伤害」的 GREEN **尚无实拍样本**：先把 GREEN 钉在 C#（oracle）的实拍足迹上，**此前任何「两侧 GREEN」结论都不得引用**。
> 采到样本 = 「判据校准完成」；校准完成前比 Rust = 无基准。

- 起 C# 栈（真 LoginGate/SelGate ＋ 影子 game-gate 7600 ＋ 代理占 7200，见 §4.0/§4.1）；进世界后把角色钉到
  **与某只怪相邻**的格（用 capture 里怪的帧坐标或截图 `06_attack.png` 判定相邻格，再用 W0-0.4 摆位钉住）。
- 驱动（**同一 `MirClinet.exe` / 同一账号 `mir2test` / 角色 `aaa`**）：
  ```powershell
  powershell -ExecutionPolicy Bypass -File tools/client/mir_flow.ps1 `
    -Account mir2test -Password mir2pass -OutDir E:/tmp/window/cliente2e/csharp/shots `
    -WalkClicks 0 -AutoPlaySec 12 -AttackClicks 1 -AttackTurn "dx,dy" -CenterY 259 `
    -SkipReenter -SkipLogout -SkipTalk
  ```
  - `-AttackTurn "dx,dy"` = **指向怪的相邻格偏移**（`"1,0"`东/`"-1,0"`西/`"0,-1"`北/`"0,1"`南/对角；`|dx|,|dy|≤1`，`(0,0)` 非法）：
    每次左键攻击前先**右键该相邻格格心**触发 `CM_TURN(dirToMonster)`，再左键攻击（取目标 = 新朝向的正前方格）。
  - `-CenterY 259` = 真机角色屏幕中心（默认 `384` 偏下 ~2 格、命中环整体错位，片67 受此影响）。
  - **订正（2026-10-18 缩窗实测）**：给 `-AttackTurn` 而**不给** `-AttackRing` 时，`mir_flow` 的左键现改为
    点在**转向目标格中心**（格心锚 `512/304` + 偏移×`48/32`）。旧实现会按默认环第 1 项（北侧 -40px）点，
    于是"转向朝南、点在北/地面" ⇒ `CM_WALK/RUN`，这也是 2026-10-18 缩窗 W0-1 五次尝试拿不到出站
    `SM_STRUCK` 的直接原因（一次拿到 `CM_TURN+CM_HIT` 但点空）。显式给 `-AttackRing` 仍按旧像素口径（逐字节不变）。
- 期望足迹（C# raw capture）：c2s `CM_TURN(3010)≥1` ＋ `CM_HIT(3014)≥1`；s2c **出站** `SM_STRUCK(31)`
  （victim=怪，`recog≠自己`）**≥1**（或其 `SM_NOWDEATH(34)`）。
- **失败分叉**：
  - 只有 `CM_HIT`、无出站伤害帧 ⇒ 没贴到邻怪（格不同/未朝向）⇒ 用截图/`-WalkPath` 重新贴近再试；
  - 出站有帧但判据 `PREREQ_RED`（探测不到 self）⇒ 显式给 `--self-recog <SM_LOGON 的 ActorId>`；
  - 无论如何都打不出 ⇒ 记录实际足迹，**不得**跳过阳性对照直接比 Rust。

### W0-2 第二步：Rust 侧同流程（同 (x,y)、先 `CM_TURN` 后 `CM_HIT`）

- 起 Rust 栈（§3.1/§3.2），把角色钉到**与 W0-1 完全相同的 (x,y)**（重跑摆位 SQL），同 `-AttackTurn "dx,dy"` ＋ `-CenterY 259`。
- 判据（Rust 单侧先自证，再两侧对照）：
  ```powershell
  python -I -B check_attack_observability.py --capture E:/tmp/window/cliente2e/rust/s2c/capture.jsonl --expect any --min-hits 1
  # 两侧：--left <rust> --right <csharp> --expect any --min-hits 1（两侧各按同一 --expect 判）
  ```
  `--expect damage` 只认**出站** `SM_STRUCK`；`any` 接受出站 struck 或击杀；**入站（victim=自己）一律不计入出站**。

### W0-3 第三步：③a 反向点 `(311,666)`（先过净空判据）

```powershell
python -I -B check_reverse_point_clean.py --point 311,666 --map 0 --vr 12
# 期望：CLEAN（四源 MonGen/GuardList/Merchant/Npcs 距该点全 > vr=12；最近确定性对象 d=30）
```
- **先过判据再起栈**（fail-closed，缺源文件即 RED）。`(291,666)` 现为 **NOT_CLEAN**（卫士 `(281,661)` d=10）⇒ **不得**用作反向点。
- 角色钉 `(311,666)`，两侧各跑一趟，`aoi_visibility_pair.py --expect none` 期望**两侧都 0 对**。
- **失败分叉**：仍非 0 对 ⇒ **先看是否卫士/NPC**：本窗口 GuardList 已实现，两侧应**对称**有该 actor ⇒
  两侧同 actor 出帧 = 「点不净」（换净空点，用 `check_reverse_point_clean.py --scan-box`）；只一侧出帧 = 实现差异，登记 RED。

### W0-4 第四步：GuardList 帧一致（新实现，两侧应**对称**）

> **新事实（GuardList 已实现）**：`mir2-rs/crates/envir/src/guardlist.rs` ＋ `mir2-rs/bins/game-svr/src/guardlist.rs` 已落地
> （白名单 T-56 对应卡 `openmir2-guardlist-guards-loading`）⇒ **Rust 世界现在也放守卫**。于是 `(291,666)`
> （卫士 `(281,661)` d=10 ≤ vr=12）**两侧都应出现卫士在其视野内**——与片67/§2c「C# 有、Rust 无」的旧态相反。

- 点位：`(291,666)`（视野内确定性卫士 `(281,661)` dir=5 = `Envir/GuardList.txt` 第 28 行「卫士 0 281, 661 : 5」）。
- 两侧各跑一趟，对照出现帧：期望**两侧都有**该卫士的 `SM_APPEAR`/`SM_TURN`（`param=281` / `tag=661`）⇒ 帧一致。
- **失败分叉**：Rust 侧仍 0 帧 ⇒ 查 GuardList 接线（`[Server] VentureServer` 门 ＋ 宿主调用点，见 `mir2-rs/bins/game-svr/src/guardlist.rs` 头注）；
  两侧都有但 Dir/位置不一致 ⇒ 逐字段 diff、登记。

### W0-5 第五步：怪打玩家探针（victim=自己 的 `SM_STRUCK`）

> **⚠ 订正（2026-10-16；原文「主动索敌/游走攻击未实现」已被片88 `b22295b1e` 推翻）**：Rust 怪物 AI 现**已实现主动索敌**——
> `crates/world/src/monai.rs` 的 `monster_run` 按时钟门调 `search_target`（`monai.rs:189`；`ATMonster.Run` 前缀见 `monai.rs:600-618`），
> 门控 `is_active_seek_race`（`monai.rs:555`，= 不覆写 `Run` 的 `ATMonster` 族 {53,81,83,86,88,89}）；`SearchTarget` 不消费 RNG，改的是分支导向。
> **判读口径（已收紧）**：测试点怪属**主动族**时，玩家贴怪静止**应**在 Rust 侧出现 victim=自己的 `SM_STRUCK`；**探针空转 = 缺陷**
> （不再是「可能仍空转、非缺陷」）。仍须**同点位两侧对照**：C# 有 Rust 无 ⇒ 立卡查 `monai` 目标选择；两侧都 0 ⇒ 该怪非主动族，换主动族怪重试。

- 动作：把玩家贴到怪旁**静止** ~10s，采 Rust 侧 **victim=自己** 的 `SM_STRUCK`：
  ```powershell
  python -I -B check_attack_observability.py --capture <rust capture> --expect any --allow-unknown-self
  # 报告行分列出站/入站；本步看「入站」栏（victim=自己）是否 ≥1
  ```
- 期望：**C# 侧**（同点位）应出现入站 `SM_STRUCK`（片68 已证 C# 同窗被怪围殴致死）；**Rust 侧**对**主动族**点怪**同样应出**（片88 已补主动索敌）⇒ 空转即缺陷（按上面收紧口径判读）。
- **失败分叉**：Rust 入站 0 且 **C# 也 0** ⇒ 该怪非主动族，换主动族怪重试；**C# 有、Rust 无** ⇒ 另立卡查 `mir2-rs/crates/world` 的怪物 AI 攻击支（`monai` 目标选择）。

### W0-6 第六步：RNG 双侧账本（`rng-scene` 驱动器）

> 驱动器已在 master（`mir2-rs/tests/parity/m3/rng-trace-scene/`：`run_rng_trace_pair.ps1` ＋ `rng_scene_gate.py`）；
> 场景 `rng_trace_scenario.json`：**图 `0137`（竞技场，MonGen 0 行）＋ 固定怪（稻草人 @ (18,20) dir=0 ×1）＋
> 起点 `(17,20) dir=2` ＋ 种子 `42` ＋ 600 ticks**。真机两相 trace **须占窗口**（可与 ④ 100 杀同图同起点同种子、**同批串行**）。
>
> **构造段基线（与场景账本分开，先重取）**：按 §2f 0.1b 采集并判定——部署件 **69 抽 / 18 站点**、
> 屏障 A = **第 17 条数据行**（1-based `seq==17`）、`Robot.txt` 有效行数 ×4 = 账本头、route `added=1649`。

```powershell
# 1) 窗口期两侧各采两相 trace（-Go 必须 owner 显式授权；-PinScene 钉含怪场景）
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/rng-trace-scene/run_rng_trace_pair.ps1 `
  -Go -Side csharp -PinScene -PeerSession <session>
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/rng-trace-scene/run_rng_trace_pair.ps1 `
  -Go -Side rust   -PinScene -PeerSession <session>
# 2) DoD① 单一入口：先场景前置门（GREEN）才 groupdiff --fold --strict
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/rng-trace-scene/run_rng_trace_pair.ps1 -SceneGroupDiff `
  -AScene <csharp-scene.json> -BScene <rust-scene.json> -AStream <csharp-rng-stream.tsv> -BStream <rust-rng-stream.tsv>
```
- **口径**：**两侧场景前置门**（map/seed/start_pose ＋ MonGen 冻结件 ＋ Monster 集合同口径 ＋ 制品级 `INJECT_OK <seed>`）
  **全 GREEN** 才跑 `groupdiff --fold --strict`（C# `construct` 折叠到 Rust `spawn`：组集合 ＋ 折叠计数 ＋ 共享组相对次序全等）；
  **非 GREEN ⇒ 拒比**（`REFUSED`，绝不输出 groupdiff 结论）。
- **失败分叉**：门 RED（场景漂移/缺件/种子不符）⇒ fail-closed **拒比**，**不是**实现差异，查明后再跑；
  门 GREEN 但 `groupdiff` rc=1（`GROUP_SET_DIFF`/`MISSING_EXPECTED_GROUPS`/`MIN_GROUP_DRAWS`）⇒ **真账本分歧**，逐组 diff 登记。

> **严格账本校验（必须先过，2026-10-16 起）**：既有消费链（`rng-trace-scene/rng_scene_gate.py` + groupdiff）
> 对**断号/重复 seq 完全不看** ⇒「组集合相等」可能建立在被截断的流水上（静默假绿）。
> 窗口内两侧落账本后**先跑新严格校验器**。⚠ **两个近似同名目录别选错**：本步的**严格校验器/驱动器**在
> `mir2-rs/tests/parity/m3/rng-scene/`（`rng_trace_diff.py` ＋ 包装器 `run_rng_scene_pair.ps1`，片84 新增）；
> 上一步的 **DoD① driver** 在 `mir2-rs/tests/parity/m3/rng-trace-scene/`（`run_rng_trace_pair.ps1` ＋ `rng_scene_gate.py`）——**两者是不同目录**。
> **首选照抄该目录 README §3 认证的包装器形态**：
> `powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/rng-scene/run_rng_scene_pair.ps1 -TraceCheckOnly -Side <csharp|rust> -AStream <side>-rng-stream.tsv -Seed 42`
> 等价的直接 python 形态（`check` 收**位置参数** `stream`，**没有** `--trace` 旗标）：
> `python -I -B mir2-rs/tests/parity/m3/rng-scene/rng_trace_diff.py check <side>-rng-stream.tsv --side <csharp|rust> --seed 42`
> （5 条 fail-closed：头 `INJECT_OK <seed>` 必须存在且 == `--seed`（与 `MIR2_RNG_SEED` 注入同源）/ seq 必须 1..N 连续无缺无重 / 列数=5 / site 非空 / 无数据行 ⇒ PREREQ）；
> **任一侧 check 不 GREEN ⇒ 拒比**（不得调用比较器、不得输出结论行）；两侧都 GREEN 才 `-SceneGroupDiff`。
> **必核项（2026-10-17 起，卡 `openmir2-mongen-terrain-loop-live`）：刷怪位置 + RNG 流水 —— 依赖 mapcell 装载（`2ff02ac0e`/`b192bd9f3`）。**
> `.map` 格子属性此前未灌进 `world.maps`（每格皆 `Walk`）⇒ 刷怪的地形移位环（oracle `WorldServer.MonGen.cs:750-802`
> 的 31 次 `CanWalk` 环）由 `bins/game-svr/src/mongen.rs` 的 `cell_walkable_ignore_actors`/`can_walk`（读
> `get_cell_info(...).1`）驱动，门**恒假**、**环从不进入**。装载后真机可达：`Envir/MonGen.txt` **2827 条里 1072 条**
> 声明出生格是墙格（755 LowWall / 306 HighWall / 11 越界），其中 **40 条首轮即走边界支**
> （`Random(Width/2)`/`Random(Height/2)`）⇒ **刷怪位置与 RNG 流水都会变**。⇒ 本步对账除现有场景外，**必须**
> 至少含一个「墙格出生点」的 MonGen 框（或冻结件逐条列出 MonGen 行并在两侧同口径），并把**两侧出生坐标**一并
> 落账本；出生点 / RNG 若分歧，先核这一片的门（构造级已由 `bins/game-svr` 的 `mongen::tests::terrain_*`
> 把「环被进入 / 首轮即走边界支 / 抽数逐项 == oracle 期望表」钉死；真机仍需本窗口复核）。

> **必核项（2026-10-17 起，卡 `openmir2-mapcell-consumer-teeth`）：走位/推/冲刺帧面 + NPC/商人出现帧 —— 同依赖 mapcell 装载（`2ff02ac0e`/`b192bd9f3`）。**
> mapcell 装载让 `Valid==Walk` 真机生效（此前每格皆 `Walk`）⇒ **玩家走位碰撞**由
> `crates/world/src/world.rs::cell_free`（oracle `Envirnoment.CanWalk`，`Envirnoment.cs:442-486`）驱动，
> 目标格是墙 ⇒ **不移动 + 不发 `SM_WALK` + 不回 `+GD`**（此前恒可走）。构造级已由 `bins/game-svr` 的
> `tests::move_cmd_blocked_by_wall_cell_sends_no_move_frame`（走生产入口 `handle_move_cmd`）钉死：
> 摘 `cell_free` 的属性门 ⇒ 源格登记被 orphan 必红；`MapCellInfo::valid()` 恒真 ⇒ 走上墙格必红。
> ⇒ 窗口必核：两侧同点同方向**朝墙走**一次，比对「Rust 侧无 `SM_WALK` / 坐标不动」与 C# 侧一致
> （选**贴墙**格：出生点邻域墙格用 `crates/envir/tests/real_maps.rs` 的 `deployment_wall_census` 复核确认）；
> 推（`CharPushed`）/冲刺（`Rush` 的 `CanWalk(...,true)`）同族同口径。
> **NPC / 商人出现帧**：本片已用 `mir2_envir` 解析器普查真机（`crates/envir/tests/real_maps.rs::deployment_wall_census`）：
> `Envir/Npcs.txt` **3** 条、`Envir/Merchant.txt` **131** 条，**落墙格各 0 条** ⇒ 装载后真机**不发生任何
> 「静默移除」**（oracle `BaseObject.Initialize` 的 `CanWalk && AddToMap` 失败 ⇒ `RemoveAt`；Rust
> `attach_merchant_to_map`/`attach_npc_to_map` 同构），故本步**无需**比对「被移除清单」；两侧 NPC 出现帧 /
> 商人可交互性照常核。**若将来语料出现落墙格条，本项升为必核「被移除 NPC/商人清单」**（复核命令见
> `bins/game-svr/src/mapwire.rs` 模块头）。

### W0-7 收窗（缺一不算收）

1. **停栈（同 `-LogDir`）**：`start_rust_stack.ps1 -LogDir <本趟 LogDir> -Stop`；C# 侧按既有 `-Stop`。
   ⚠ **起/停必须同一 `-LogDir`**（否则 9 端口泄漏、R8 从此 RED）。
2. **十端口实拍回落**：`powershell -File E:\MirServer\port-status.ps1` ⇒ 九个游戏端口回 DOWN（仅 3306 可 UP）。
3. **DB 还原**：把 W0-0.4/§2b 的 before（`MapName/CX/CY` 与 `Hp/MaxHP`）写回 `aaa`。
4. **恢复车队**：`lanes-resume.sh`（窗口脚本 `stop` 末步）。
5. **证据落点与命名**：命令 ＋ 输出 ＋ 路径 ＋ sha256（按 LF/git-blob 口径用
   `python -I tools/evidence-hash/verify_sha256sums.py write <dir>` 生成并 `verify` 自检）；
   capture/日志/截图入 `mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/`，命名 `w0-<步>-<侧>-<件>`。
6. 卡上 `patch`（base=跑前 master、head=你的提交）＋ `status`（work 逐条写 ③a/③b/GuardList/探针/RNG 结论）。

### W0-8 判据与失败分叉速查

| 步 | 判据命令 | GREEN | RED 分叉 |
| --- | --- | --- | --- |
| ③b-阳性 | `check_attack_observability.py --capture <csharp> --expect any` | 出站 struck/击杀 ≥1 | 未贴邻怪/未朝向 ⇒ 重贴 |
| ③b-Rust | 同上 `--capture <rust>` | 出站 ≥1 | 探测 self 失败 ⇒ `--self-recog` |
| ③b-两侧 | `--left <rust> --right <csharp> --expect any` | 两侧各 ≥1 | 单侧 0 ⇒ 登记 RED |
| ③a | `check_reverse_point_clean.py --point 311,666` ＋ `aoi_visibility_pair.py --expect none` | CLEAN ＋ 两侧 0 对 | 非 0 对 ⇒ 换净点/看是否对称 |
| GuardList | 出现帧对照 `(291,666)` | 两侧对称有卫士帧 | 单侧无 ⇒ 查接线 |
| 探针 | 入站 `SM_STRUCK`（victim=自己） | 主动族点怪 Rust 侧应出（片88 已补主动索敌）；空转 = 缺陷 | C# 有 Rust 无 ⇒ 立卡 |
| RNG | `-SceneGroupDiff` | 门 GREEN ＋ `VERDICT IDENTICAL` | 门 RED ⇒ 拒比（非实现差异） |
| 刷怪地形 | 两侧出生坐标（冻结件含 ≥1 墙格出生点框）逐点一致 ＋ `cargo test -p game-svr --lib -- terrain_` 绿 | 两侧坐标一致、构造级 6 用例绿 | 分歧 ⇒ 先核 mapcell 装载（`2ff02ac0e`/`b192bd9f3`）＋ `mongen.rs:499-536` 地形环 |

---

## §2e W0 必核追加（2026-10-17）：召唤神兽回落 + 宝宝满 + 朝墙（R 级：RNG 流水）

> 本节为**独立追加**小节（另一条车道在同文件追加「走位碰撞」；本节不重排/不改写任何既有小节）。
> 卡：`openmir2-recall-slave-shift-live`。依据：`mir2-rs/tests/parity/m3/audit-valid-consumers-reachability-20261017.md` §2 第 1 条。

**为什么必核**：`.map` 格子属性灌进 `world.maps`（`2ff02ac0e` / `b192bd9f3`）后，`World::recall_slave`
的 `SpaceMoveGetRandXY` 从「每格皆 Walk、首轮即返回、**零抽数**」变成「正前方是墙 ⇒ 进移位支 ⇒
`world.rng` 抽数 + 落点变」。这是**真机可达**且**改 RNG 流水**的路径 ⇒ 按 R 级占用窗口必核。

**触发条件（与 oracle 同构，逐行锚）**

1. 魔号 30（`SKILL_SINSU` 召唤神兽）：`main.rs:5346 cast_slave_spell` → `bins/game-svr/src/slave.rs:280 summon_from_spell`
   → `crates/world/src/slave.rs:152 summon_slave`（`:202` 是它内部对 `World::recall_slave` 的调用点；`recall_slave` 定义在 `:631`）。
2. `MakeSlave` 返回 null：宝宝数已满（`SlaveList.Count >= nMaxMob`，`gate_ok == false`）**或**
   `RegenMonsterByName` 失败；oracle **只有神兽支**走 `RecallSlave`（`MagicManager.cs:1300-1304`）。
3. 主人正前方 `(fx,fy)` 是墙格（`GetFrontPosition`，`BaseObject.cs:977-1038`）。
4. `BaseObject.RecallSlave`（`:1850-1876`）→ `SpaceMove`（`:1103-1166`）→ `SpaceMoveGetRandXY`（`:1040-1096`）；
   Rust 对应 `World::recall_slave`（`slave.rs:631-685`）→ `space_move_get_rand_xy`（`:396-438`）。
   **首轮 `CanWalk(fx,fy,true)` 命中 ⇒ 不进移位支、零抽数**；未命中 ⇒ `Random(Width)` / `Random(Height)`。
   `CanWalk(x,y,true)` = `cellSuccess && Valid`（`Envirnoment.cs:442-486`）= Rust `get_cell_info(x,y).1`。

**窗口动作（造场景）**

1. 站位：让 `aaa` 站到某图**墙格正前方**且面朝该墙（朝向由 `CM_TURN` 钉住；两侧同坐标/同朝向）。
2. 宝宝：先召唤一只神兽（魔号 30）并确认 `SlaveList.Count == nMaxMob`（现网默认 `DragonCount=1`）⇒
   第二次施放魔号 30 时 `MakeSlave` 必返回 null ⇒ 走回召。
3. 记录两侧 RNG 流水（`rng-scene` 驱动器，见 §2d W0-6）与**回召落点**。

**判据（GREEN / RED）**

- 构造级（已入库，离线可复跑，零端口）：`cargo test -p mir2-world --lib recall_` ⇒ 6 条全绿，其中
  - `recall_shift_when_front_wall_and_slave_list_full`：宝宝满 + 朝墙 ⇒ **移位支被进入**，
    `world.rng` 抽数逐项 = oracle 期望（seed=42、10x10 图：`Next(10)` 前两项 = `[6, 1]`），落点 `(6,1)`，
    随后连续三抽 = `[1, 5, 1]`（证明**恰好** 2 抽；只断第 3 抽 = 1 不具判别力 —— seed=42 的 `Next(10)` 在多个索引上都是 1）；
  - `recall_shift_when_front_wall_and_spawn_fails`：造怪失败触发同一支，期望同上；
  - `recall_first_round_walkable_consumes_zero_draws`：**阴性对照**（正前方可走 ⇒ 首轮成功、零抽数）。
- 真机：两侧 RNG 账本（`-SceneGroupDiff`）**门 GREEN 且 `VERDICT IDENTICAL`**，且**回召落点同坐标**。
  RED 分叉：RNG 门 RED ⇒ 拒比（先修注入/门）；落点不同而 RNG 相同 ⇒ 核 `CanWalk` 的格子属性口径
  （`.map` 装载 `2ff02ac0e`/`b192bd9f3`）；RNG 抽数不同 ⇒ 先核移位支的分段/上限（`n18`/`n1C`/201）
  与取数次序（**先 `Random(Width)`，再 `Random(Height)`**）。

**同窗登记（不实现）**：`merchant_run` → `map_random_move` → `move_object`（**与 oracle 不同构，R 级**）

- 链（仅**商人**条件可达）：`main.rs:8049 enable_merchant_tick` → `merchant.rs:661 merchant_run`
  → `map_random_move`（`:748`/`:854`）→ `world.rs:1149` / `map.rs move_object`。
  **固定 NPC 不可达**：`npc_run` 的 `map_random_move` 受 `can_move` 门，`Npcs.txt` 装载的 `NormNpc` 的
  `BoCanMove` 恒 false（`npc.rs` 模块注释已登记）。
- 装载后变化：本仓 `merchant.rs:873-874` 抽 **2 次**后 `move_object(ignore_block=true)`，若落点是墙则
  `if !cell.valid() { return false }` ⇒ **不移动、不再抽数**；而 oracle `MapRandomMove`（`BaseObject.cs:1214-1237`）
  → `SpaceMove`（`:1103-1166`）→ `SpaceMoveGetRandXY`（`:1040-1096`）在落点不可走时**进移位环、继续抽
  `Random(Width)`/`Random(Height)`**，最后 `AddMapObject`。⇒ **落点与 RNG 流水都会分歧**（不是同构！
  原稿引的 `Envirnoment.cs:380` 属 `MoveToMovingObject`，不是这条路径 —— 独立复验 2026-10-17 否掉）。
- **判级：R**（RNG 抽数会变 + 移动结果会变）。~~这是真缺口，不只是窗口项 ⇒ 已立卡承接实现；本片只登记、不实现。~~
  **2026-10-17 订正**：该缺口**已由 `047709a9b` 实现**（复用 `space_move.rs` 的 `SpaceMoveGetRandXY`；复验 CONFIRMED，见总表 #5），
  窗口按「已实现、必核」读；201 轮回滚语义与出帧面仍未建模（见 #5 的 RED 栏）。

## §2f. 窗口必核项总表（2026-10-17 收敛；**编号避开既有 §0「要证的三件事」**）

> **指针 / 用法**：本节是**追加在文件末尾的总索引**，不重排、不改写任何既有小节（故 §1–§2e 的节号与既有引用不变）。
> 执行步骤一律以 §1–§2e 为准；本表只回答四件事：**开窗前必须核哪些 R 级项、GREEN/RED 怎么判、依赖什么、出处与既有覆盖在哪**。
> 收敛基线：`2fa749471`（master tip，2026-10-17）。卡片状态见 `walgit collab board`（下列卡片均 `in-progress`，按 tier=R **不 closed**）。

### 0.1 开窗前的前置（三项，缺一不开窗）

1. **本机 mysqld 在听 3306**（基线服务，不是游戏端口）。`mir2-rs/tests/parity/m3/fixture_chain_gate.py check` 的**战斗 runner** 依赖 DB；未听时它会以 `ERROR 2002 ... (10061)` 报 `rc=1`——那是**环境未就绪**，不是代码红；先按本机 mysqld 配方（datadir `D:\mysql\data`、3306）恢复再跑门。来源：本文件 §1 第 6 条（`82-two-side-window-runbook.md:48`）。
2. **先跑内存测试（或短程压测）再开窗**。本机近 **60 天崩溃 42 次**，其中含 `FAULTY_HARDWARE_CORRUPTED_PAGE`（内存/页损坏类）⇒ 独占窗口 + 长跑前先做内存自检；窗口内若出现无法归因的随机 FAIL，先怀疑宿主内存再怀疑代码。来源：**编排侧环境事实（本任务书）**——仓内无对应锚点，故本条**不冒充**代码/文档证据。
3. **两侧同种子 + 先「重取基线」，再比 `seq=1`（1-based）**。默认 RNG 流水已被构造段补齐改变（见下表 #12）：任何**旧基线**（`9a175a66f` 之前）与新树比都会自构造段起分歧；窗口内必须两侧各自用**当前树**、同一 `MIR2_RNG_SEED` 重采，先落 `INJECT_OK <seed>` 并通过 `rng-scene/rng_trace_diff.py check` 的 5 条 fail-closed，再逐项比 `seq=1` 起（1-based）。命令级采集 / 判定值见 **§0.1b**；来源：`9a175a66f`、`e6105f011`、`78416a932`；本文件 §W0-6。

### 0.1b 「重取基线」命令级步骤（M7）

> 适用：构造段 / route / RNG 相关窗口项在**比对之前**。任何**旧基线**（`9a175a66f` 之前）都不得再用于判定；
> 两侧必须用**当前树**（≥ `2fa749471`）与**同一** `MIR2_RNG_SEED` 重采。判据是「重取后的两侧账本」，
> 不是任何历史制品（§0.2 #1/#2/#12 的判定值以本小节为准）。

**0) 落盘目录（先建；`start_rust_stack.ps1 -RngLog` 对父目录不存在会 fail-closed）**

```bash
mkdir -p E:/tmp/window/cliente2e/ledger
```

**1) 构造段输入实测（部署件；决定账本头与总抽数）**

```bash
# Robot.txt 有效行数：skip 空行 / `;` 注释；必须两列；分隔符 = 空格 / TAB / '/'
awk 'BEGIN{FS="[ \t/]+"} {sub(/\r$/,"")} $0!="" && substr($0,1,1)!=";" && NF>=2 {n++} END{print "robot_valid_lines=" n ", ledger_head_draws=" n*4}' \
  E:/MirServer/M2GameSvr/Envir/Robot.txt
# 期望：robot_valid_lines=1, ledger_head_draws=4

# 段②：CHECKQUEST 条数（每条 = 1 站点 × 4 抽）
grep -o "CHECKQUEST" E:/MirServer/M2GameSvr/Envir/MapInfo.txt | wc -l
# 期望：9 ⇒ 36 抽

# 段③ M1：MapQuest.txt 有效行数（skip 空行 / `;`；三列非空）
awk 'BEGIN{FS="[ \t]+"} {sub(/\r$/,"")} $0!="" && substr($0,1,1)!=";" && NF>=3 {n++} END{print "mapquest_valid_rows=" n ", draws=" n*4}' \
  E:/MirServer/M2GameSvr/Envir/MapQuest.txt
# 期望：1 ⇒ 4 抽

# 段③ L2/L1/L3：隐藏 NPC 三件都在 ⇒ 3 站点 × 4 = 12 抽
ls -d E:/MirServer/M2GameSvr/Envir/MapQuest_def/QManage.txt \
      E:/MirServer/M2GameSvr/Envir/Market_Def/QFunction-0.txt \
      E:/MirServer/M2GameSvr/Envir/Robot_def
# 期望：三行都存在（Robot_def 是目录）
```

**判定值（部署件，2026-10-17 复验口径）**：
`4（Robot.txt 1 行×4） + 12（G1–G3 孤儿） + 1（屏障 A） + 36（CHECKQUEST 9×4） + 4（M1 1×4） + 12（L2/L1/L3 3×4） = 69 抽`
⇒ **站点数 = 1 + 3 + 1 + 9 + 1 + 3 = 18**。屏障 A 是**第 17 条数据行**（1-based `seq==17`；前 16 条 = 段① 4 站点 × 4 抽）。

**2) 两侧同种子采集（各起一次；端口互斥，不可同时起）**

Rust（`start_rust_stack.ps1 -RngLog` 会把 `MIR2_RNG_LOG` 注入 `game-svr`；父目录必须先存在）：

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 `
  -RustBinDir E:/tmp/m2svr-window-target/debug -Seed 42 `
  -RngLog E:/tmp/window/cliente2e/ledger/rust-ctor.tsv `
  -LogDir E:/tmp/window/cliente2e/rust/stack
# 期望启动输出含：RUST_RNG_LOG_INJECTED=E:/tmp/window/cliente2e/ledger/rust-ctor.tsv
```

C#（`start-csharp-hook.ps1` 把流水写到 `<LogDir>/rng-stream.tsv`，头 = `INJECT_OK 42` + `SEED_PROBE`）：

```powershell
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/start-csharp-hook.ps1 `
  -Seed 42 -LogDir E:/tmp/window/cliente2e/ledger/csharp-ctor
# 产物：E:/tmp/window/cliente2e/ledger/csharp-ctor/rng-stream.tsv
```

**3) 机判（两侧各跑一次；`seq` 是 1-based，头部 `INJECT_OK` 行不是数据行）**

```bash
# 3a) 头行种子自证：两侧首行都必须是 `1<TAB>INJECT_OK<TAB>42<TAB>0`
head -1 E:/tmp/window/cliente2e/ledger/rust-ctor.tsv | tr '\t' ' '; echo
head -1 E:/tmp/window/cliente2e/ledger/csharp-ctor/rng-stream.tsv | tr '\t' ' '; echo

# 3b) 构造段形状（前 69 条数据行）：
#     - 第 17 条 seq==17 且 kind=Next(max) / arg=10000（屏障 A）
#     - 其余 68 条，每 4 条一组的 arg 依次为 [1500,2000,3000,3000]（ctor 公共链）
#     - 站点数 = arg==1500 的条数（17 个 ctor 站点） + arg==10000 的条数（1） = 18
awk -F'\t' '
  $2=="INJECT_OK" || $2=="SEED_PROBE" || $1 !~ /^[0-9]+$/ {next}
  {n++
   if (n<=69) {
     if (n==17) { if ($1!=17 || $2!="Next(max)" || $3!=10000) bad_barrier=1; barrier++ }
     else {
       if (n<17) p=(n-1)%4; else p=(n-18)%4
       want=(p==0?1500:(p==1?2000:3000))
       if ($3!=want) bad_shape=1
       if ($3==1500) ctor_sites++
     }
   }}
  END {printf "data_rows_total=%d ctor_sites=%d barrier=%d bad_barrier=%d bad_shape=%d\n",
        n, ctor_sites, barrier, bad_barrier+0, bad_shape+0;
       exit (bad_barrier+0 || bad_shape+0 || ctor_sites!=17 || barrier!=1)}
' E:/tmp/window/cliente2e/ledger/rust-ctor.tsv
# 期望：data_rows_total>=69 ctor_sites=17 barrier=1 bad_barrier=0 bad_shape=0（rc=0）
# C# 侧同一条命令把路径换成 csharp-ctor/rng-stream.tsv；期望相同。

# 3c) 两侧构造段逐项比对（seq 1..69；site 列形态不同，先比 kind/arg/result）
diff -u \
  <(awk -F'\t' '$1~/^[0-9]+$/ && $2!="INJECT_OK" && $2!="SEED_PROBE" && $1<=69 {print $1"\t"$2"\t"$3"\t"$4}' E:/tmp/window/cliente2e/ledger/rust-ctor.tsv) \
  <(awk -F'\t' '$1~/^[0-9]+$/ && $2!="INJECT_OK" && $2!="SEED_PROBE" && $1<=69 {print $1"\t"$2"\t"$3"\t"$4}' E:/tmp/window/cliente2e/ledger/csharp-ctor/rng-stream.tsv)
# 期望：无输出、rc=0。有输出 ⇒ 真账本分歧，按首差 seq 定位：
#   seq≈17 ⇒ 先查屏障 A（`ServerConf.cs:114` 的位置/`+=`）；seq 1..16 ⇒ 查 Robot.txt 行数与 G1–G3。
```

> ⚠ `seq` 1-based 的根据：Rust `crates/shared/src/rng_trace.rs` 的 `SEQ.fetch_add(1)+1`；头部
> `1<TAB>INJECT_OK…` 是**声明行**（字面 1），不是取数行 ⇒ 机判必须先剔 `INJECT_OK`/`SEED_PROBE` 再谈 `seq`。
> ⚠ 两侧 `site` 列**不是同一字符串**（Rust 记 `crates\world\src\entity.rs:…`，C# 依赖 PDB）⇒ 3c 先比
> `kind/arg/result`；site 对齐用 `m3/rng-trace-probe/consumer_groups.tsv` 的组映射（DoD① 的
> `groupdiff --fold --strict`）。

**4) route 落格 `added=1649`**

```powershell
# Rust 生产日志（起栈后；日志可能是 GBK/UTF-8，用 -SimpleMatch 避免正则元字符）：
Select-String -Path E:/tmp/window/cliente2e/rust/stack/game-svr.out.log -SimpleMatch '地图格子属性已灌入'
Select-String -Path E:/tmp/window/cliente2e/rust/stack/game-svr.out.log -SimpleMatch '地图连接点已灌入（MapRoute）'
# 期望：routes=1658 unresolved=0 added=1649
```

离线等价复算（**零端口、无需起栈**；本仓 `#[ignore]` 普查，真机部署件向量）：

```bash
# CARGO_TARGET_DIR 用一次性测试目录：本命令只跑 #[ignore] 普查，不产出窗口栈二进制、
# 不影响 E:/tmp/m2svr-window-target 的出处核验；勿把测试 target 当窗口 target。
cd mir2-rs && CARGO_TARGET_DIR=E:/tmp/route-census-target \
  cargo test -p game-svr --bin game-svr -- --ignored --nocapture real_registry_route_cells_on_wall
# 期望末行：route 总数=1658 落在墙格=9 源图不可解析/未加载=0 ⇒ 生产 added 应为 1649（父提交 = 1658）
```

**5) 判定值汇总（写进证据；不绿即 RED，不许用「语义等价」盖过）**

| 对象 | GREEN | RED |
| --- | --- | --- |
| `Robot.txt` 有效行 | **1**（⇒ 账本头 4 抽） | 0 / >1，或与账本头不符 |
| 构造段 | **69 抽 / 18 站点**；屏障 A `seq==17` 且 `arg=10000` | 68/17（旧基线）、`seq≠17`、站点数 ≠18 |
| 场景账本头 | 两侧 `INJECT_OK 42`；`rng-scene/rng_trace_diff.py check` 5 条 fail-closed 全 GREEN | 缺头 / 种子异值 / 断号 / 重复 |
| route | `routes=1658 unresolved=0 added=1649` | `added=1658`（装载未生效）/ `1648`（旧判据）/ `unresolved>0` |
| 两侧比对 | `seq=1..69` 的 `kind/arg/result` 逐项相等 | 首个分歧 seq 即定位起点（17 附近先查屏障 A） |

> 另：§1 / §1b / §W0-0 的既有前置门（十端口全 DOWN、`guard` = `WINDOW_GUARD_OK`、车队 pause、`start/stop` 同一 `-LogDir`）仍**全部适用**，本表不重复。

### 0.2 R 级必核项总表

| # | 必核项 | 判据（GREEN / RED） | 依赖（如 mapcell 装载 / 构造段基线） | 来源（卡 id / 文档 / commit） | 是否已被现有 runbook 覆盖（节号） |
| --- | --- | --- | --- | --- | --- |
| 1 | **构造段账本（`seq=1` 起逐项，1-based）**：RobotObject 头（`Robot.txt` 有效行数×4）→ G1/G2/G3 孤儿商人 → **屏障 A**（`ItemNumber += Random(10000)`，**恰 1 抽**）→ M2 → M1 → L2/L1/L3 → L4/L5；部署件 **69 抽 / 18 站点** | **GREEN**：两侧同种子 `MIR2_RNG_LOG` **自首个数据行 `seq=1` 起**（日志 `seq` 是 1-based：`rng_trace.rs` 先 `fetch_add(1)+1`）`site/kind/arg/result` 逐项相等；Rust 侧构造段 **69 抽/18 站点**；屏障 A 位于**第 17 条**（1-based，`seq==17`；零基下标 16）；`relay::starts_without_worldmaps_when_envir_present` **13 行**（前 12 行与父提交逐行相同，末行 = 屏障 A）。**窗口前先实测 `Robot.txt` 有效行数**（账本头 = 行数×4）。**RED**：首项即分歧；**抽数 ≠ 69 或站点数 ≠ 18**；屏障 A 不在第 17 条（`seq≠17`）；`Robot.txt` 行数 ≠ 账本头假设；用旧基线（68/17）比对 | **构造段基线（已变，须重取）**；oracle `new Merchant` 12 处中另 **9 处**未处置（实体缺口另立条目） | 卡 `openmir2-ctor-barrier-a-draw` / `openmir2-ctor-draws-fold` / `openmir2-npc-ctor-draws`；`9a175a66f`、`0a34719c9`、`e6105f011`、`78416a932`、`8b73f2a51`、`f1cffd3f1`；`m3/design-buff-and-tick-20261016.md:446,450,488-493` | **§2f 0.1b**（2026-10-17 已落命令级与判定值） |
| 2 | **route 落格 `added=1649`**（真注册表 1658 条中 9 条落墙格被 `AddMapRoute` 门丢弃：**7 HighWall + 2 LowWall**） | **GREEN**：**Rust 侧**启动日志 `INF 地图格子属性已灌入` 且 `routes=1658 / unresolved=0 / added=1649`；**C# 侧无同类日志/计数器**（`src/` 冻结面里没有这些打印）⇒ C# 侧用**只读复算**口径（`MapInfo.txt` + `.map` 二进制解出 route 落格，方法见 `mapwire.rs` 的 `#[ignore]` 普查）比对。**RED**：`added=1658`（装载未生效）或 `1648`（**旧判据，永远不可能绿**）；`unresolved>0`（`h006` 大小写敏感复发的征兆） | **mapcell 装载**（`2ff02ac0e` / `b192bd9f3`） | 卡 `openmir2-mapcell-attributes-loading`；`b192bd9f3`（订正 `2ff02ac0e` 的 1657→1648 为 1658→1649）；`m3/audit-valid-consumers-20261017.md` §1A #8、§3 #4 | **§2f 0.1b**（已落命令级：日志 + `#[ignore]` 离线复算） |
| 3 | **刷怪位置 + RNG**（MonGen 地形移位环）：`Envir/MonGen.txt` **2827 条里 1072 条**声明出生格是墙格（755 LowWall / 306 HighWall / 11 越界），其中 **40 条首轮即进边界支**（`Random(Width/2)` / `Random(Height/2)`） | **GREEN**：两侧**出生坐标逐点一致**；冻结件至少含 **1 个「墙格出生点」MonGen 框**（或逐条列 MonGen 行并两侧同口径）；构造级 `cargo test -p game-svr --lib -- terrain_` 绿（6 条）；RNG 账本 `spawn` 组 `IDENTICAL`。**RED**：坐标分歧；抽数分歧 ⇒ 先核 mapcell 装载与 `mongen.rs:499-536` 地形环 | **mapcell 装载**（`2ff02ac0e`/`b192bd9f3`）；真机 Envir 语料 | 卡 `openmir2-mongen-terrain-loop-live`；`b192bd9f3`；`649924131`；本文件 §W0-6 必核项、§W0-8 末行 | **§W0-6 + §W0-8**（2026-10-17 已列） |
| 4 | **召唤回落 RNG**（魔号 30；宝宝满 或 spawn 失败 **且主人正前方是墙** ⇒ 进 `SpaceMoveGetRandXY` 移位支） | **GREEN**：两侧 RNG 账本 `VERDICT IDENTICAL` **且回召落点同坐标**；构造级 `cargo test -p mir2-world --lib recall_` 6 条绿（阳性落点 `(6,1)`、随后三抽 `[1,5,1]`；阴性对照零抽、随后三抽 `[6,1,1]`）。**RED**：RNG 门 RED ⇒ 拒比；落点不同而 RNG 相同 ⇒ 核 `CanWalk` 口径；抽数不同 ⇒ 先核移位支分段/上限（`n18`/`n1C`/201）与取数次序（**先 `Random(Width)` 再 `Random(Height)`**） | **mapcell 装载**；宝宝数达 `nMaxMob`（现网 `DragonCount=1`）；朝向钉死 | 卡 `openmir2-recall-slave-shift-live`；`49fe49f05`；`m3/audit-valid-consumers-reachability-20261017.md` §1.4、§2 #1；本文件 §2e | **§2e**（完整覆盖） |
| 5 | **商人随机移动移位环**（`merchant_run` → `map_random_move` → `SpaceMove` → `SpaceMoveGetRandXY`）：首两抽 `Random(W-nEgdey-1)+nEgdey` / `Random(H-nEgdey-1)+nEgdey`，落点不可走则进环继续抽（至多 201） | **GREEN**：两侧**落点同坐标 + RNG 流水 `IDENTICAL`**；可走落点 = 恰 2 抽（阴性对照）。**RED**：落点/抽数分歧；201 轮**耗尽回滚**语义（Rust 现只 `return`，oracle 的 `DeleteFromMap`/`AddMapObject` 语义**未建模**）、落格 `CellType`、`SpaceMove` 出帧面（`RM_SPACEMOVE_SHOW2` 等）不在本层 | **mapcell 装载**；DB `CanMove!=0 && MoveTime>0`；`npc_run` 该支**不可达**（固定 NPC `BoCanMove` 恒 false），只 `merchant_run` 条件可达 | 卡 `openmir2-map-random-move-shift-loop`；`047709a9b`、`0f8390dbb`、`8e9244388`；`m3/audit-valid-consumers-reachability-20261017.md` §2 附表；本文件 §2e「同窗登记」 | **§2e 末段（部分）**——该段写「只登记、不实现」，**须按 `047709a9b` 读作「已实现、窗口必核」** |
| 6 | **进图前移位环**（`WorldServer.cs:375-394`：出生格不可走 ⇒ 每次 `CurrX = CurrX-3+Random(6)` / `CurrY = CurrY-3+Random(6)`、**至多 5 轮**、仍不可走回落 `[Setup] HomeMap/HomeX/HomeY`（`:388-394`）） | **GREEN**：墙格出生 ⇒ 进环、两轴各一抽、命停；5 轮耗尽 ⇒ 回落 `HomeMap`；**阴性对照：出生格可走 ⇒ 零抽、坐标不动**（用随后连续三抽断言）；构造级 3 用例绿（seed=42 前六抽 `[4,0,0,3,1,1]`）。**RED**：抽数/落点不符；`HomeMap` 不可解析分支（Rust 返回 `home_fallback_error`，oracle `goto ReGetMap` **未建模**，当前部署不可达）；「墙格出生不可达」依赖 `HomeMap=3 (839,668)` 为 Walk（实测成立） | **mapcell 装载**；`[Setup] HomeMap` 可解析且落点可走 | 卡 `openmir2-enter-shift-loop`；`2fa749471`；`m3/audit-valid-consumers-20261017.md` §3 #5、§5 | **未覆盖** |
| 7 | **Enter 落格**（墙格出生 ⇒ oracle 同形：**进世界但不入格表**，不移除角色） | **GREEN**：`enter_map` 返回 Ok、实体仍在、`add_to_mapped=false`、格计数 0、会话 `InWorld`；两侧一致表现为「他人 AOI 隐身 + 自身扫描/tick 继续」。**RED**：角色被移除/拒绝进图（旧 Rust 行为）；仅一侧隐身/一侧可见 | 宿主路径的墙格出生已由 #6 判为不可达；**切图缩图** / 直接调 `World::enter_map` 仍可产生该态；`switch_map_inner` 的 `CellRejected` 属既有、另行跟踪 | 卡 `openmir2-enter-map-cell-rejected-vs-oracle`；`ed9ab035d`；`m3/audit-valid-consumers-20261017.md` #23/#31、§3 #5、§5 | **未覆盖** |
| 8 | **走位/推/冲刺帧面 + NPC/商人出现帧**（mapcell 装载让 `Valid==Walk` 真机生效） | **GREEN**：贴墙格同点同方向朝墙走 ⇒ **坐标不动 + 不发 `SM_WALK` + 不回 `+GD` + 源格登记不 orphan**；冲刺（`can_walk_ignore_objects`）同口径；`Envir/Npcs.txt` **3** 条 / `Merchant.txt` **131** 条**落墙格各 0** ⇒ 无静默移除、两侧出现帧对称。**RED**：出帧/坐标分歧；将来语料落墙格 >0 ⇒ 升为「被移除 NPC/商人清单」必核。**注意**：推人 `Command::Push` / 魔号 21 `mag_sace_move` 目前**无生产者**（不可达）⇒ 不得用它们判 GREEN，接线后升为必核；`move_object` 的 `cells[0]` 耦合（322 图中 (0,0) 为墙 317 / Walk 5）与 oracle 同形，生产走位先拒墙 ⇒ 不触发 | **mapcell 装载**；`cell_free` 属性门；`crates/envir/tests/real_maps.rs::deployment_wall_census`（`#[ignore]`）复核 | 卡 `openmir2-mapcell-consumer-teeth`；`34af5b4ff`；`m3/audit-valid-consumers-20261017.md` §3 #2/#3、§4；本文件 §W0-6 第二个必核项 | **§W0-6**（已列；推/冲刺的生产可达性需按本表口径收窄） |
| 9 | **default 支 `RM_SPELL` 帧面 + 缺 53 的帧序**（`DoSpell:159` 无条件广播；宽口径缺口 **93 行 / 80 魔号**已接线） | **GREEN**：未建模魔号（55 字面 default / 81 `>80` 各一）⇒ 观察者**恰 1 帧** `SM_SPELL`；逐字段（尾 = `MagicId` ASCII、坐标 = **snap 后**的 `tx/ty`）；施法者自己**不收**；AOI 内非 Ghost 才收；`SM_SPELL` **早于** `SM_MAGICFIRE`；战士技能 0 帧；距离门越界 0 帧。**RED**：缺帧/双发/顺序反了/收件人集合不同。**帧序残差（2026-10-17 订正）**：MP 扣减 + `RM_HEALTHSPELLCHANGED`（**53**）**已接**（`18c7ffe56`：`PlayObject.Base.cs:1505-1531` 包装层，53 先于 `SM_SPELL`；蓝不足回 `SM_MAGICFIRE_FAIL(639)` 且仍 `+GD`）；仍缺：default 支的 `SM_MAGICFIRE` / 训练尾 `Random(3)` + 落库（8/21/37 效果支已接共享尾）、**距离门失败**的 639 已接（`a9c2a4c45`），战士门与 `boSpellFail`（召唤/诱惑/分身）支在飞；共享尾：**P1 已落地**（`568b0a962`：A 类 17 id 零抽 `RM_MAGICFIRE`、B 类 49/51–54 训练尾+`SM_MAGIC_LVEXP`+回写）；**P0 版本门在飞**（设计件 `m3/design-login-notice-and-clienttick-20261017.md` 方案 B1）；**P2 已落地**（`8ca4be1d9`：14/15 护身符门 + 14/15/31 共享尾，31 `bo_train` 恒 true）；**P3 全批（B1–B5，26 个 actionable 效果体）已落地**（B1 `ab73cc645`、B2 `711753bc6`、B3 `71f50ab7a`、B4 `a9ec7d35c`、B5 `84b696b05`；含 HAM_GROUP 团长表收口 `1a9d35501`）；合同/报告见 `m3/p3-b{1..5}-*`、机器人/怪物两条旁路。**旁路**：机器人 `AutoSpell`（触发集合不同）、怪物硬编码 `RM_SPELL(48)`（`FireballMonster.cs:39`）均未建模 ⇒ 接上之前**不得**判「帧面缺口已全清」 | `RM_HEALTHSPELLCHANGED` 生产者（缺）；两条旁路的解冻条件见界定件 §8 | 卡 `openmir2-default-arm-spell-surface` / `openmir2-spell-frame-bypass-paths`；`9e59246c7`、`888180c07`、`b65feca68`；`m3/design-default-arm-spell-frames-20261017.md` §5.1、§5.2、§8 | **部分**（§0 只覆盖 53 帧 HUD 前置与 §2b 压血；default 支帧面/帧序未列） |
| 10 | **A-5 buff（麻痹/防御系）与越界零抽**（技能 14/15/31 的 `DoSpell` 距离门 `MagicAttackRage`） | **GREEN**：in-range ⇒ 目标获 `DefensePower`/`MagicDefensePower`/`magic_shield` 且世界 RNG 确被推进；**越界（\|Δ\|>8）⇒ 无任何 buff、`magic_shield` 不置位、`draws_between == 0`（一个随机数都不抽）、`get_char_status` 不变、drain 后 gate 零 657**；阳性对照（buff 到期 ⇒ 657 确实发）证明「零 657」非假绿。**RED**：越界仍抽数/上 buff；`HAM_MASTER(3)`/`HAM_GUILD(5)` 友军面比 oracle **窄** ⇒ **漏加友军 buff**（方向已订正，不是「多加点」）；`AntiPoison` 恒 0 ⇒ 流水位置不漂但**抽样参数不同**（`Random(5)` vs `Random(AntiPoison+5)`）⇒ value 账本会红 | buff tick（生产已开）；麻痹戒指 `RING_MAKESTONE_ITEM(113)`；线上 `attack_mode` 可达 3/5 | 卡 `openmir2-buff-producer-wiring` / `openmir2-buff-system`；`68b0730df`、`ffac59d1b`、`2369d8ca3`；**判据锚 = `cargo test -p game-svr --bin game-svr spell_cmd_a5_`（5 条：in-range / 越界零抽 / 阳性对照）**；`ffac59d1b` 的复核 `review` 条目（`m3/audit-valid-consumers-20261017.md` §1B #35 指向 `slave.rs:657 recall_slave`，**与本行无关，勿引**）| **未覆盖** |
| 11 | **spell snap 的掩码格 / 窗口上界**（5 处 spell 块全部改走忠实镜像 `cetin_near_xy`） | **GREEN**：目标站在**被掩码格**上 ⇒ **不 snap**（用原始点击坐标过门 ⇒ 世界 RNG 必须前进）；点击与目标 \|Δ\|=2（出 3×3 窗口）⇒ 不 snap；**异图同坐标** ⇒ 不 snap（按施法者图过滤）；正常命中路径**仍 snap**（t1/t2 牙防「永不 snap」）。**RED**：多抽数/漏施放；窗口 ±1 改 ±2 逃逸 | **mapcell 装载**（掩码 = `set_map_xy_flag`，生产唯一写口是城门/城墙）；`queryuser.rs:29` 镜像不得改 | 卡 `openmir2-snap-cell-fidelity`；`befd3ad7c`；`m3/audit-valid-consumers-reachability-20261017.md` §1.7（`spell_snap_target` 5 处） | **未覆盖** |
| 12 | **基线重取要求**（默认 RNG 流水**已变**：构造段 **68→69 抽 / 17→18 站点**；屏障 A 插在第 17 条；`relay` **12→13**，此前 fold 已把它 **0→12**） | **GREEN**：窗口前两侧各自用**当前树**（≥ `2fa749471`）重采；构造段 69/18；屏障 A 第 17 条（`seq==17`）；场景账本头 `INJECT_OK <seed>` 两侧同、`seq` 1..N 连续无缺无重（`rng-scene/rng_trace_diff.py check` 5 条 fail-closed，**与 DoD① 的 `rng-trace-scene/` 是两个目录**）。**RED**：拿旧基线比 ⇒ 68/17 与 relay-12 基线自**第 17 条**起必红；**relay-0（fold 前）基线自第 1 条构造抽即分歧**；两侧种子不同/未落 `INJECT_OK` | **构造段基线**；严格校验器；`MIR2_RNG_SEED` 注入 | `9a175a66f`（68→69、relay 12→13）、`e6105f011`（relay 0→12、分歧自第 17 条）、`78416a932`（可复现事实：13 行、末行 = 屏障 A、前 12 行与父提交逐行相同）；本文件 §W0-6 | **§2f 0.1b / §W0-6**（重取基线已落命令级） |

### 0.3 其它 R 级窗口项（按卡片归并，不逐条展开）

> 下列卡片同样按 tier=R **不 closed**，其窗口项不在本批 mapcell/ctor/帧面簇内，另行排窗；列此以保证「总表」不漏项。

| # | 必核项（判据要点） | 来源卡（看板 id） | runbook 覆盖 |
| --- | --- | --- | --- |
| A1 | 掉落/经验：固定种子 **100 杀**两相窗口（掉落列表逐项相同；经验曲线含英雄逐级相同，**不得 i32 截断**）；quest 真机窗口 | `M3④-实现 D…`、`M3④ 掉落/经验动作序列夹具…`、`M3 逐系统对齐 ④` | 未覆盖 |
| A2 | NPC/商店：买/卖/修理/仓库价格与库存变化 + 钱不够/负重满等失败分支两侧一致 | `M3⑤-实现 6`、`M3 逐系统对齐 ⑤`、`M3⑤ 商店夹具…` | 未覆盖 |
| A3 | 任务：接/交/条件判定 + **任务态不落库**语义（oracle 口径） | `M3⑥-实现 4/5`、`M3 逐系统对齐 ⑥` | 未覆盖（该卡现被 marker 前缀 / frame JSON / quest_state 生产者三重阻断） |
| A4 | 组队/交易：归属/分配/取消/掉线中断结果一致；`GainExp` 组队缺态吞错口径 | `M3⑦-实现 4/5`、`M3⑦-3`、`M3 逐系统对齐 ⑦` | 未覆盖 |
| A5 | 公会/攻城：建会/宣战费用时限/城门状态/税收金库/攻城状态机 + **两侧可注入时钟** | `M3⑧-实现 5/6/7/8/9/10`、`M3⑧ 横切前置…`、`M3 逐系统对齐 ⑧` | 未覆盖 |
| A6 | 宝宝/英雄：生命周期、叛变到期、主人死幽灵、46 分身 `RM_TURN`、诱惑 20 | `M3⑨-实现 2/3/5`、`M3⑨-2 残余`、`openmir2-m3-pet-slave-*`、`M3 逐系统对齐 ⑨` | **§2e 只覆盖魔号 30 召唤回落**；其余未覆盖 |
| A7 | 邮件/拍卖/元宝寄售：DB **字段级 diff=0** + 全部帧 | `M3⑩-实现 1/2/3`、`M3 逐系统对齐 ⑩` | 未覆盖 |
| A8 | 怪物主动索敌（ATMonster 族）：主动族点怪 Rust **应出** victim=自己的 `SM_STRUCK`（空转 = 缺陷）；子类 `Run` 覆写 / SuperGuard / ArcherGuard 未建模 | `怪物主动索敌（ATMonster 族…）—— W0 联调窗硬阻塞` | **§2d W0-5**（已列，含 2026-10-16 收紧口径） |
| A9 | 入站派发 R1（16 条派发臂）/ 出站编码器 R2（`RM_*→SM_*` 缺失族）：制药抽数流水 + 帧序 + `MYSTATUS` 派生落库 | `入站派发红名单…（R1）`、`出站编码器红名单…（R2）` | 未覆盖 |
| A10 | M5 收敛：72h 长跑 / mem-probe 内存斜率 / 三档压测曲线 / 灰度-回滚演练 / 灰度前后 DB 快照 | `M5 收敛/压测/灰度收口…` | 未覆盖（另档期；本表不排期） |
| A11 | Claim B 全服 RNG 消费者：**同场景/同起点/同种子**含怪两相 trace → `groupdiff` 组序；`construct→spawn` folded 规则**折叠后计数与相对次序必须相等**；另含 **BotSrv real wiring**（0137/pin 怪/dump 的窗口项） | `全服 RNG 消费者结构一致性（Claim B）…`、`M3 RNG 站点组同场景两侧 trace…`、`openmir2-m3-rng-scene-driver` | **§W0-6 部分**（账本机制），组序判据与 BotSrv wiring 未展开 |
| A12 | world-tick 扩展（RNG/时钟/公告节拍）；transmap/maptrigger（**1658 条 route 逐字节** + 落格 + 踩格换图） | `openmir2-world-tick-expansion`、`openmir2-transmap-maptrigger`、`openmir2-maptrigger-monster-and-door` | 未覆盖 |
| A13 | 编码器孤儿接线 / 窗口采集前置：两侧种子交叉断言、C# `site` 列、前置门生产路径不误拦 | `openmir2-orphan-encoder-wiring`、`openmir2-window-collector-preflight` | **§1b / §W0-0 部分** |

| A14 | **M3②③ F-2：真机隔离两相 `scoped_diff`** —— 攻方 `characters_ablity.Hp` C#=20663 / Rust=20658（时序量化，stamina 回收率 47.9/s vs 40.0/s），`M3-report.md:306`（§4）已登记为**已知 RED** | `openmir2-m3-scoped-diff-ability` | 未覆盖 |
| A15 | **M3 总验收**：DoD② **300 假人 24h 长跑** + DoD④ 真机 `run_pair` + `scoped_diff --canonical` diff=0（`M3-report.md:281` 明写「24h 长跑为 R 级窗口项」；A10 是 M5 的 72h，**不能替代**） | `openmir2-m3-final-acceptance` | 未覆盖 |
| A16 | **GuardList 真机窗口项**：③a 反向点两侧出帧一致 + `MIR2_RNG_SEED` 双侧 `site/order` 账本 | `openmir2-guardlist-guards-loading` | **§W0-4 / §W0-8 部分** |
| A17 | **交易门**：交易中发 `CM_USERBUYITEM`/`CM_USERGETDETAILITEM` ⇒ **零帧 + characters/gold 不变**（`openmir2-m3-shop-dealing-gate`）；**仓库 50 槽** `characters_storageitem` **字段级 diff**（`M3⑤-实现 4`，tier=R 持久化判据） | `openmir2-m3-shop-dealing-gate`、`M3⑤-实现 4` | 未覆盖（仅 A2 的失败分支泛化） |
| A18 | **魔号 37（能量排斥）推挤**：oracle 与魔号 8 函数体逐字相同（`MagicManager.cs:601-606` == `:259-264`，`MagPushArround`）⇒ 真机核「37 与 8 同场景出帧/抽数一致」：`Random(20)`/`Random(2)` 逐项、逐格 `SM_BACKSTEP(9)` 条数/顺序/收件人。**GREEN**：两侧账本 `IDENTICAL` 且 37/8 帧序列逐字节相同（离线已钉：`cargo test -p game-svr --bin game-svr spell_cmd_energyrepulsor_`）。**RED**：多抽/少抽、帧多/漏、收件人不同 | 窗口场景：可并入 8/21 同图同种子批次 | 卡 `openmir2-magic-8-21-wiring`；`318bd1d81`；`m3/audit-valid-consumers-reachability-20261017.md` §1.1/§1.2 订正块 | 未覆盖 |
| A19 | **切图失败回滚（switch_map / route_enter）**：目标落格失败（越界/墙）⇒ oracle `EnterAnotherMap` false ⇒ `Walk` 回滚旧图旧格、不发移动帧（`PlayObject.cs:1878-1881`/`:1911-1913`；`BaseObject.cs:685-688`）。**GREEN**：两侧一致表现为「角色留旧图旧格 + 坐标回滚 + 无 `SM_WALK` + 格表登记恢复」；route 目标越界 ⇒ `Failed` 而非静默 `Skipped`。**RED**：角色悬空（两图都不在格表）、坐标未回滚、仍发移动帧。**登记残差**：oracle 失败前已发 `RM_CLEAROBJECTS`/`RM_CHANGEMAP`（帧面未建模）；多 route「一成功一失败」的回滚落图差异（真实数据不可达） | 宿主构造（越界 route hop / 落格失败）；当前生产无直接 `switch_map` 调用者 ⇒ 真机可达性低 | 卡 `openmir2-enter-map-cell-rejected-vs-oracle`、`openmir2-route-enter-failed-classification`；`ed9ab035d`、`df5a260ea`、`35e3cf468` | 未覆盖 |

### 0.4 本表维护约定

- **新增 R 级窗口项 ⇒ 先在 §0.2 / §0.3 加一行，再去改执行步骤**；不要只把判据写在卡片或提交信息里。
- 行内数字**只有两类可写**：① 有 `file:line` / 提交 / 文档锚的；② 明确标「编排侧环境事实」。禁止口语化「大概/应该」。
- 每行 GREEN 必须能由**已入库的命令或构造级用例**判；不能只靠肉眼。RED 分叉必须写「先怀疑哪一片」。
- 卡片 `closed` 前先删/改本表对应行的「未覆盖」状态；**R 级卡 `merge_result` 后保持 `in-progress`**，本表不因合并而删行。
