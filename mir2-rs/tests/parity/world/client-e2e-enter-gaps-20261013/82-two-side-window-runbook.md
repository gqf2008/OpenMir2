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
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 `
  -RustBinDir E:/tmp/openmir2-window-target/debug -Seed 42 -LogDir E:/tmp/window/cliente2e/rust/stack

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

### W0-0 前提（任一不满足即停手；全程零副作用）

1. **十端口实拍**（含共享 3306）：`powershell -File E:\MirServer\port-status.ps1`
   期望：**九个游戏端口 5000/5100/5500/5600/5700/6000/7000/7100/7200 全 DOWN**（报告里列为「缺失端口」），
   仅 **3306 可 UP**（MySQL 共享）。任一游戏端口 UP ⇒ 有泄漏栈，先做 W0-0.2。
2. **无泄漏栈（R8）**：`bash mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/client-e2e-window.fixed.sh.txt guard`
   期望 rc=0、打 `WINDOW_GUARD_OK`；R8 RED ⇒ **照抄它打印的 `PREFLIGHT_STOP_CMD:`**（＝
   `start_rust_stack.ps1 -LogDir <持有 pid 的目录> -Stop`）。**起/停必须用同一个 `-LogDir`**（历史 21h 卡窗根因，见 `docs/交接-OpenMir2编排与联调-20261015.md` §1.2）。
3. **车队已停确认**：窗口脚本 `start` 第 1 步即 `lanes-pause.sh`（暂停车队＝其它 agent 车道的在跑实例）。
   人工开窗的等价动作＝确认无残留 BotSrv/game-svr/cargo/rustc（`Get-Process`）＋ 车队已 pause。
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
