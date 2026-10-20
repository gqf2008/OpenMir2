# 真机窗口第二轮 Runbook（OpenMir2，2026-10-23）

> **tier=L（纯文档 / 命令配方）**。本文件不启动任何栈、不占端口、不写数据库；它把
> **2026-10-22 实跑验证过**的「C# 影子 + 7600 影子网关 + 7200 代理 + `mir_flow -SkipLogout`」
> 配方固化成下一轮可照抄的步骤，并给出**受控登录场景**与 5 条机器可判的登录帧判据。
>
> - 代码基线：`master = 57452dba0`（本 worktree 创建时；开窗当天仍须重新解析，不得沿用）。
> - 今日证据：`E:/tmp/window/20261022/**`（**只读**；本文件没有修改其中任何文件）。
> - 上位计划件：`docs/window-run-sheet-20261022.md`（v3）。本文件只记录**今日实测通过/失败的东西**；
>   与 v3 冲突处按今日实测改写，并在 §6 登记。
> - 本文件不改变任何 R 卡口径；R 卡窗口未跑/未全绿仍不得 `closed`（`AGENTS.md` 判据分级）。

---

## 0. 今日结论（先看这一节）

**拓扑是通的，数据隔离是通的，C# oracle 的登录尾是完整的；Rust 侧被打死，登录尾没采到。**

| 项 | C# oracle（`csharp/`） | Rust（`rust/`） |
| --- | --- | --- |
| 起栈 | `start-csharp-hook.ps1`，pack 副本，影子 `GameSvr-shadow` | `start_rust_stack.ps1 -DbName mir2_test`，混合栈（C# DBSrv/LoginSrv） |
| 三库 | `mir2_test_data/_db/_account` | 同（`<LogDir>/db-target` 影子配置） |
| 网关拓扑 | 真 gate(7200) 停 → 影子 gate(7600) → 代理占 7200 | 同 |
| 客户端流程 | `FLOW_OK`（`mir_flow -SkipLogout`） | `FLOW_OK`（同） |
| s2c 帧数 | 47 | 53 |
| 15 帧入图前缀 | **匹配** | **匹配** |
| `654` | seq 28（走位对之后） | seq 16（46 之后） |
| `657×2 / 第二 42 / 100×4` | **全部存在** | **全部缺失** |
| `52 body_len` | **40**（oracle 布局） | **58**（client 布局） |
| 战斗 | 无 | 9 条 `victim=self SM_STRUCK`，HP 13→0，`SM_NOWDEATH(34)` |
| 结论 | 可作 oracle 基线 | **被打死**；登录尾不可判 |

### 0.1 今日为什么 Rust 侧会被打死

Rust 侧角色 `winplay0` 进世界在 `CurMap:0 (288,620)`；该点在 `MonGen.txt` 的
`(256,572) r=50` 刷怪框**内部**（`check_mongen_dense_points.py` 实测 Chebyshev=0），
同屏有主动怪。客户端默认 `-WalkClicks 2` 点 `(902,650)`，但服务端三次 `CM_WALK(3011)`
全部被判「被挡/越界 ⇒ 不动」，角色原地挨打：

- `rust/stack/game-svr.out.log`：`进世界 ChrName:winplay0 CurMap:0 (288,620) ... hp=14 max_hp=16`；
  三次 `Ident=3011 ... 移动：actor_id=1953 方向=4 1 格被挡/越界 ⇒ 不动`；
- `rust/proxy/conn-0001.s2c.capture.jsonl`：`SM_STRUCK(31)` 的 `recog=1953`（自己）依次
  `param=13,11,9,7,6,4,2,1,0`，随后 `SM_NOWDEATH(34)`（seq 51）；
- 收窗存档把 `Hp=0` 写回测试库（`mir2_test_db.characters_ablity.PlayerId=6228`）。

⇒ 下一轮必须**固定安全格、不走路、不打怪**（§2）。`(311,666)` 已用两个只读工具复核：
`check_reverse_point_clean.py` = `CLEAN`（最近对象 GuardList `(281,661)` 距离 30 > vr 12）；
`check_mongen_dense_points.py` = 框外、到最近生成框 Chebyshev=44 > 12。

### 0.2 今日证据里缺的一步：没有导出 judgment capture

今日两个 proxy 目录只有 `chunks.ndjson` + `conn-*.bin` + `session.json` + 一个**临时帧 dump**
（`csharp/proxy/capture.jsonl`、`rust/proxy/conn-0001.s2c.capture.jsonl`）。它们不是
`export_s2c.ps1` 的产物：

- 没有 `port` 字段 ⇒ `s2c_diff.py` 直接 `PREREQ_MISSING: Rust 侧 capture.jsonl 里没有 port=7200 dir=s2c 的帧`；
- `raw_hex` 是 12B 头+body 的**解码后**字节，不是 `#`…`!` 的线上帧 ⇒
  `check_client_s2c_frames.py` 报 `RED: 52 body 解码失败：帧形状不符（需 # 开头 / ! 结尾 / 头 16 字符）`。

这两条是今日实际复现的输出（见 §3.0）。下一轮**必须先 `export_s2c.ps1` 再判**，否则 15 帧前缀再对也不能判 52。

### 0.3 今日证据索引（只读）

| 证据 | 路径 | SHA256（今日） |
| --- | --- | --- |
| C# 起栈日志 | `E:/tmp/window/20261022/csharp/start3.log` | `（见文件）` |
| C# 客户端流程 | `E:/tmp/window/20261022/csharp/flow.log` | `（见文件）` |
| C# 影子身份 | `E:/tmp/window/20261022/csharp/GameSvr-shadow/!deployed-from.json` | `git_head=8a621f6a...` |
| C# GameSvr 连接串 | `E:/tmp/window/20261022/csharp/GameSvr-shadow/Server.conf` | `18b465373b15b0fa829402e201165d15d51438b9619826304faf8d004702cfd6` |
| C# DBSrv 连接串 | `E:/tmp/window/20261022/csharp/repo/src/DBSrv/bin/Release/dbsvr.conf` | `1bdec368af1d07fc29c3e3abf9960708fc4c1bfb219660a0f63fb02197aeb065` |
| C# LoginSrv 连接串 | `E:/tmp/window/20261022/csharp/repo/src/LoginSrv/bin/Release/logsrv.conf` | `ddfea338f0c5ef81b1d3bb4b5c592168c5b3d6d44b52c2e856c749535d6f2584` |
| C# 帧 dump | `E:/tmp/window/20261022/csharp/proxy/capture.jsonl` | `208900fa0594f6a02b19e9f1971bac38cb081acf810f3ef66de9c9f39be2dd5e` |
| Rust 起栈日志 | `E:/tmp/window/20261022/rust/start.log` | `（见文件）` |
| Rust game-svr | `E:/tmp/window/20261022/rust/stack/game-svr.out.log` | `5c804099006044d580b3931f19fc3d7e9ca4dbb08de74f205f4e6fa41e57dc26` |
| Rust 帧 dump | `E:/tmp/window/20261022/rust/proxy/conn-0001.s2c.capture.jsonl` | `0f2cef7a9944873d6f3b7ef3cfc8ca0f109340a7d1cc9cb57e0a97f08d98146e` |
| 今日定格的影子起栈脚本 | `E:/tmp/window/20261022/csharp/setup-capture.ps1`、`rust/setup-capture.ps1` | `（见文件）` |

> 说明：今日的真实命令行没有被逐条落盘；§1 的命令由上述日志的 `START ... exe=` 路径、
> `PIDFILE`、`session.json.argv`、`RUST_SEED_INJECTED=42`、`DB_TARGET ...` 与
> `csharp_isolation.json.seed=42` **反推**，并对每个路径做了存在性核对（§0.4）。

### 0.4 路径存在性核对（2026-10-23 本文件写作时）

| 路径 | 状态 |
| --- | --- |
| `E:/MirServer/{M2GameSvr,LoginGate,SelGate,RunGate}` | 存在 |
| `E:/MirServer/port-status.ps1` | 存在 |
| `E:/tmp/m2svr-window-target/debug/{game-svr,game-gate,login-gate,sel-gate}.exe` | 存在（2026-10-20 23:06） |
| `E:/tmp/m2svr-window-target/window-provenance.json` | 存在 |
| `D:/MirClient-run/{run-release.cmd,MirClinet.exe}` | 存在 |
| `D:/mysql/mariadb-10.11.19-winx64/bin/{mysql,mysqldump}.exe` | 存在 |
| `E:/Users/gxh/Documents/GitHub/OpenMir2/tools/{capture/mir2_proxy.py,m2-acceptance/export_s2c.ps1,client/mir_flow.ps1}` | 存在 |
| `.../mir2-rs/tests/parity/m3/start-csharp-hook.ps1` | 存在 |
| `.../mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/{check_client_s2c_frames.py,check_reverse_point_clean.py,check_mongen_dense_points.py}` | 存在 |
| `.../tools/capture/GoldenExport/bin/Release/net8.0/GoldenExport.exe` | 存在（2026-10-18） |

---

## 1. 今日验证通过的配方（C# 影子 → 7600 影子网关 → 7200 代理 → 客户端）

> 本节按**单侧**写；C# 与 Rust 的差别只在「起主栈」和「影子 gate 的可执行文件」两步，
> 其余（影子 conf、代理、客户端流程、导出）完全同形。
> 统一工作目录：`$W = E:/tmp/window/20261023`（下一轮；**今日**是 `E:/tmp/window/20261022`）。
> 所有 `Start-Process -RedirectStandardOutput/-RedirectStandardError` 的父目录必须**先存在**
> （见 §1.9 坑 1）。

### 1.1 开窗前只读核对（不占端口、不起栈）

```powershell
# 1) 九游戏端口必须全 DOWN，3306 可 UP；7600 单独断言（port-status.ps1 不含 7600）
powershell -NoProfile -ExecutionPolicy Bypass -File E:/MirServer/port-status.ps1
#   期望：缺失端口 = 5000,5100,5500,5600,5700,6000,7000,7100,7200；mysqld pid 在 3306
[System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners() |
  Where-Object { $_.Port -in 7200,7600 } | Format-Table Address,Port
#   期望：无输出

# 2) 无残留客户端 / 代理 / 网关
Get-Process MirClinet,mir2_proxy,game-gate,GameGate,GameSvr,game-svr,DBSrv,LoginSrv -ErrorAction SilentlyContinue
#   期望：无输出

# 3) 主检出干净（本车道/本窗口不得写主检出）
git -C E:/Users/gxh/Documents/GitHub/OpenMir2 status --short
#   期望：无输出

# 4) Rust 二进制与 provenance
Test-Path E:/tmp/m2svr-window-target/debug/game-svr.exe
Test-Path E:/tmp/m2svr-window-target/window-provenance.json
#   期望：两个 True
```

> 若九端口不是全 DOWN：按 `docs/window-run-sheet-20261022.md` §A1 的 R8 处理；**禁止**
> `E:/MirServer/start-all.ps1 -Stop`（会 Force 且连带杀 `mysqld`）。外部 C# 全栈只能由起栈会话/owner 停。

### 1.2 复制 C# 包与仓库（不改 `E:/MirServer`、不改主检出）

今日实际布局（`E:/tmp/window/20261022/csharp/`）：

```text
csharp/
  pack/                 # E:/MirServer 的四个目录只读复制
    M2GameSvr/          # 影子 GameSvr 的源（start-csharp-hook 会再 robocopy 到 GameSvr-shadow）
    LoginGate/ SelGate/ RunGate/
  repo/                 # 只复制两份 C# Release + rng-hook 源（不要用 symlink，见 §1.9 坑 9）
    src/DBSrv/bin/Release/       # dbsvr.conf 在这里改
    src/LoginSrv/bin/Release/    # logsrv.conf 在这里改
    mir2-rs/tests/parity/m3/rng-hook/   # 供 start-csharp-hook 构建 RngSeedHook.dll
  GameSvr-shadow/       # 起栈时自动生成；旧的要先删
  stack/                # -LogDir
  shadow/               # 7600 影子 gate 的工作目录
  proxy/  flow/  s2c/   # 代理 / 客户端 / 导出
```

```powershell
$W = 'E:/tmp/window/20261023'          # 今日 = E:/tmp/window/20261022
$pack = "$W/csharp/pack"
$repo = "$W/csharp/repo"
New-Item -ItemType Directory -Force "$pack","$repo/src/DBSrv/bin","$repo/src/LoginSrv/bin",`
  "$repo/mir2-rs/tests/parity/m3" | Out-Null

# 1) 四个 C# 部署目录只读复制（源 E:/MirServer 一字不动）
foreach ($d in 'M2GameSvr','LoginGate','SelGate','RunGate') {
  robocopy "E:\MirServer\$d" (Join-Path $pack $d) /E /NFL /NDL /NJH /NJS /NP | Out-Null
  if ($LASTEXITCODE -gt 7) { throw "RED: robocopy $d rc=$LASTEXITCODE" }
}

# 2) 只复制 DBSrv/LoginSrv 的 Release（今日 repo 里就这两份 + hook 源）
robocopy 'E:\Users\gxh\Documents\GitHub\OpenMir2\src\DBSrv\bin\Release' `
  "$repo\src\DBSrv\bin\Release" /E /NFL /NDL /NJH /NJS /NP | Out-Null
robocopy 'E:\Users\gxh\Documents\GitHub\OpenMir2\src\LoginSrv\bin\Release' `
  "$repo\src\LoginSrv\bin\Release" /E /NFL /NDL /NJH /NJS /NP | Out-Null

# 3) rng-hook 源（真实复制；不要 symlink 到主检出，否则 dotnet build 会写主检出的 bin/obj）
robocopy 'E:\Users\gxh\Documents\GitHub\OpenMir2\mir2-rs\tests\parity\m3\rng-hook' `
  "$repo\mir2-rs\tests\parity\m3\rng-hook" /E /XD bin obj /NFL /NDL /NJH /NJS /NP | Out-Null
```

### 1.3 三处连接串指向 `mir2_test_*`（今日实际改的三个文件）

**只有这三处**（键名是上游的 `ConnctionString`，少一个 `e`，不要“修正”它，否则组件读不到）：

| 组件 | 文件（今日实际路径） | 行 | 期望值 |
| --- | --- | --- | --- |
| **GameSvr** | `<WIN>/csharp/pack/M2GameSvr/Server.conf`（影子里是 `GameSvr-shadow/Server.conf`） | 3 | `ConnctionString=server=127.0.0.1;uid=root;pwd=;database=mir2_test_data;` |
| **DBSrv** | `<WIN>/csharp/repo/src/DBSrv/bin/Release/dbsvr.conf` | 21 | `ConnctionString=server=127.0.0.1;uid=root;pwd=;database=mir2_test_db;` |
| **LoginSrv** | `<WIN>/csharp/repo/src/LoginSrv/bin/Release/logsrv.conf` | 25 | `ConnctionString=server=127.0.0.1;uid=root;pwd=;database=mir2_test_account;` |

> GameSvr 的 `Server.conf` 是 **UTF-8 with BOM**（含中文注释）；DBSrv/LoginSrv 的 conf 是纯 ASCII。
> 改 GameSvr 时必须写回 BOM，否则 oracle 可能读不到第一段。

```powershell
# 1) GameSvr：UTF-8 BOM 保真
$p = "$pack/M2GameSvr/Server.conf"
$t = [IO.File]::ReadAllText($p, [Text.Encoding]::UTF8)
$t = $t -replace '(?m)^ConnctionString=[^\r\n]*', 'ConnctionString=server=127.0.0.1;uid=root;pwd=;database=mir2_test_data;'
[IO.File]::WriteAllText($p, $t, (New-Object System.Text.UTF8Encoding($true)))

# 2) DBSrv：ASCII
$p = "$repo/src/DBSrv/bin/Release/dbsvr.conf"
$t = [IO.File]::ReadAllText($p)
$t = $t -replace '(?m)^ConnctionString=[^\r\n]*', 'ConnctionString=server=127.0.0.1;uid=root;pwd=;database=mir2_test_db;'
[IO.File]::WriteAllText($p, $t, [Text.Encoding]::ASCII)

# 3) LoginSrv：ASCII
$p = "$repo/src/LoginSrv/bin/Release/logsrv.conf"
$t = [IO.File]::ReadAllText($p)
$t = $t -replace '(?m)^ConnctionString=[^\r\n]*', 'ConnctionString=server=127.0.0.1;uid=root;pwd=;database=mir2_test_account;'
[IO.File]::WriteAllText($p, $t, [Text.Encoding]::ASCII)

# 断言：恰好 3 条，且三库各不相同
Select-String -Path "$pack/M2GameSvr/Server.conf",`
  "$repo/src/DBSrv/bin/Release/dbsvr.conf",`
  "$repo/src/LoginSrv/bin/Release/logsrv.conf" -Pattern 'ConnctionString=.*database=mir2_test_' |
  ForEach-Object { "{0}:{1}:{2}" -f $_.Path,$_.LineNumber,$_.Line }
# 期望三行：...Server.conf:3:...mir2_test_data; / dbsvr.conf:21:...mir2_test_db; / logsrv.conf:25:...mir2_test_account;
```

> **只改影子，不改生产**：`E:/MirServer/M2GameSvr/Server.conf` 与 `E:/MirServer/**` 一律不动。
> 混合栈（Rust 侧）的 C# DBSrv/LoginSrv 不用手工改；`start_rust_stack.ps1 -DbName mir2_test`
> 会在 `<LogDir>/db-target` 生成并改写自己的影子配置（§1.7）。

### 1.4 起 C# 影子栈（必须先删旧影子）

`start-csharp-hook.ps1:97-108` **只在 `<Shadow>/GameSrv.exe` 不存在时才 robocopy**。
旧影子存在 ⇒ 会复用旧 `GameSrv.exe` 和旧 `Server.conf`（可能还是生产连接串）⇒ 必须先归档/删除。

```powershell
$shadow = "$W/csharp/GameSvr-shadow"
if (Test-Path $shadow) {
  $bak = "$shadow.bak-" + (Get-Date -Format 'yyyyMMdd-HHmmss')
  Move-Item -LiteralPath $shadow -Destination $bak -ErrorAction Stop
  Write-Output "ARCHIVED_OLD_SHADOW $bak"
}
if (Test-Path "$shadow/GameSrv.exe") { throw 'RED: 旧影子仍存在，拒绝起窗' }

powershell -NoProfile -ExecutionPolicy Bypass `
  -File E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/m3/start-csharp-hook.ps1 `
  -Repo $repo -Pack $pack -Shadow $shadow `
  -LogDir "$W/csharp/stack" -Seed 42
```

**期望输出（今日 `start3.log` 同形）**：

```text
SHADOW_COPY <pack>\M2GameSvr -> <shadow>
HOOK_DEPLOYED <shadow>\RngSeedHook.dll sha256=4c50037c4c36ffcc836a864b2f7e7fd7fb64b84bdb109706cbd5732f7343937e
START DBSrv pid=... ports=6000,5100,5700 exe=<repo>\src\DBSrv\bin\Release\DBSrv.exe
START LoginSrv pid=... ports=5500,5600 exe=<repo>\src\LoginSrv\bin\Release\LoginSrv.exe
ISOLATE_ATTACK on=false（默认关；行为逐字节不变）
START GameSvr pid=... ports=5000 exe=<shadow>\GameSrv.exe
START LoginGate pid=... ports=7000 exe=<pack>\LoginGate\LoginGate.exe
START SelGate pid=... ports=7100 exe=<pack>\SelGate\SelGate.exe
START RunGate pid=... ports=7200 exe=<pack>\RunGate\GameGate.exe
PIDFILE <LogDir>\csharp-stack.pid.json
ISOLATION_RECORDED ... isolate=False attack_seed=40545 sites_sha256=
CSHARP_STACK_GREEN
  ... 3306/6000/5100/5700/5500/5600/5000/7000/7100/7200 LISTEN
```

**影子身份（F1 前置）**：

```powershell
$dep = Get-Content "$shadow/!deployed-from.json" -Raw | ConvertFrom-Json
"git_head=$($dep.git_head)"          # 今日 8a621f6a87e7c206887d310b8315f74b4bb03aac
git -C E:/Users/gxh/Documents/GitHub/OpenMir2 merge-base --is-ancestor d7ead5b2a $dep.git_head
"merge-base rc=$LASTEXITCODE"        # 期望 0（d7ead5b2a 是 8a621f6a 的祖先）
Get-FileHash "$shadow/GameSrv.dll" -Algorithm SHA256 | Format-List Path,Hash
# 今日 c67d3b0b5d743b97697b1cf2075f60d42101158a9aa4dc0f2fab7a971b7300d7
```

- `merge-base rc != 0` ⇒ 该影子不含项链/头盔 1:1 修复（F1）⇒ **停窗**，不要拿它做 B3/HumItems。
- 今日 pack 的 `GameSrv.dll` 与 `E:/MirServer/M2GameSvr/GameSrv.dll` 逐字节相同（sha256 一致）；
  E4 虽允许重编，但**今日没有重编**，用的是部署件。

### 1.5 停真 RunGate(7200) → 起影子 gate(7600) → 代理占 7200（C# 侧）

今日 `csharp/setup-capture.ps1` 的实际做法（只停 RunGate，不动别的进程）：

```powershell
# 1) 只停本栈的 RunGate（按 pid 文件，绝不按进程名误杀别人的栈）
$pf = Get-Content "$W/csharp/stack/csharp-stack.pid.json" -Raw | ConvertFrom-Json
($pf | Where-Object { $_.name -eq 'RunGate' }).pid |
  ForEach-Object { Stop-Process -Id $_ -Force -ErrorAction SilentlyContinue; "STOPPED RunGate pid=$_" }
Start-Sleep -Milliseconds 800

# 2) 影子 conf：cp936 原样，只改 Count / GatePort1 两个数字
$enc = [Text.Encoding]::GetEncoding(936)
New-Item -ItemType Directory -Force "$W/csharp/shadow" | Out-Null
Copy-Item -LiteralPath "$pack/RunGate/config.conf" -Destination "$W/csharp/shadow/game-gate.conf" -Force
$t = [IO.File]::ReadAllText("$W/csharp/shadow/game-gate.conf", $enc)
$t = $t -replace '(?m)(?<=^Count=)\d+(?=\r?$)','1' `
       -replace '(?m)(?<=^GatePort1=)\d+(?=\r?$)','7600'
[IO.File]::WriteAllText("$W/csharp/shadow/game-gate.conf", $t, $enc)
Select-String -Path "$W/csharp/shadow/game-gate.conf" -Pattern '^Count=|^GatePort1='
#   期望：Count=1 / GatePort1=7600

# 3) C# GameGate 按 AppContext.BaseDirectory 读 config.conf ⇒ 必须整目录拷贝 + 覆盖 conf
Remove-Item -Recurse -Force "$W/csharp/shadow/run-gate" -ErrorAction SilentlyContinue
Copy-Item -Recurse -Force "$pack/RunGate" "$W/csharp/shadow/run-gate"
Copy-Item -Force "$W/csharp/shadow/game-gate.conf" "$W/csharp/shadow/run-gate/config.conf"

# 4) 起影子 gate(7600)
$g = Start-Process -FilePath "$W/csharp/shadow/run-gate/GameGate.exe" `
  -WorkingDirectory "$W/csharp/shadow/run-gate" -WindowStyle Hidden `
  -RedirectStandardOutput "$W/csharp/shadow/csharp-gate.out.log" `
  -RedirectStandardError  "$W/csharp/shadow/csharp-gate.err.log" -PassThru
$g.Id | Set-Content "$W/csharp/shadow/csharp-gate.pid"
Start-Sleep -Milliseconds 1500

# 5) 断言：7200 DOWN、7600 UP（port-status.ps1 的十端口不含 7600）
[System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners() |
  Where-Object { $_.Port -in 7200,7600 } | ForEach-Object { "LISTEN $($_.Port)" }
#   期望：只剩 7600

# 6) 代理占官方 7200 → 影子 7600
$proxy = Start-Process -FilePath python `
  -ArgumentList @('-I','-B','E:/Users/gxh/Documents/GitHub/OpenMir2/tools/capture/mir2_proxy.py',
                  '--out',"$W/csharp/proxy",'--map','7200=127.0.0.1:7600') `
  -WorkingDirectory 'E:/Users/gxh/Documents/GitHub/OpenMir2' -WindowStyle Hidden `
  -RedirectStandardOutput "$W/csharp/proxy.log" `
  -RedirectStandardError  "$W/csharp/proxy.err.log" -PassThru
$proxy.Id | Set-Content "$W/csharp/proxy.pid"
Start-Sleep -Seconds 1
Get-Content "$W/csharp/proxy.log"
#   期望：LISTEN 127.0.0.1:7200 -> 127.0.0.1:7600 / READY
Get-Content "$W/csharp/proxy/session.json" | ConvertFrom-Json | Select-Object pid,map
#   期望：pid = $proxy.Id；map = @{7200 = 127.0.0.1:7600}
```

> 若代理启动即报 `address in use`：7200 没释放（RunGate 没停干净，或另一个栈占着）。
> 不得把代理绑到别的端口“绕过去”；先查清 7200 的 owner。

### 1.6 客户端流程（今日 `mir_flow -SkipLogout`）

今日账号 `winplay0`（`LoginSrv` 日志：`[登录] 帐号=[winplay0] 密码长度=8`；按本机 BotSrv 约定账号=密码，今日用 `winplay0` 登录成功）。
客户端目录 `D:\MirClient-run`，入口 `run-release.cmd`（参数已指向 `127.0.0.1:7000`）。

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/client/mir_flow.ps1 `
  -Account winplay0 -Password winplay0 -RunDir D:\MirClient-run `
  -OutDir "$W/csharp/flow" -SkipLogout
```

**期望**（今日 `csharp/flow.log` 同形）：

```text
STAGE start / login_id / login_pwd / login_submit / charsel / enter_game
STAGE notice_poll / ingame / talk / talk_sent / walk_1 / walk_2
STAGE skip_logout / reenter / reentered / done
CLIENT_STOPPED
FLOW_OK
```

截图 `01_login.png ... 07b_skip_logout_inworld.png ... 09_reenter.png`。

- `-SkipLogout` 让客户端**留在世界里**（不触发 `CM_SOFTCLOSE(1009)`）；流程最后仍会
  `Stop-Process MirClinet`（除非 `-LeaveClientRunning`），这会触发断线存档。
- 今日默认 `-WalkClicks 2` 会点 `(902,650)` 走两步；**受控登录必须关掉**（§2）。
- 今日流程还有 `reenter` 阶段（对已在世界的客户端再点选角/进入，实际不产生新会话）；
  受控场景加 `-SkipReenter`，少两次无用点击。

### 1.7 导出 judgment capture（下一轮必须补的一步）

`mir2_proxy.py` 只落 `chunks.ndjson` + `conn-*.bin` + `session.json`；判据工具吃的是
`export_s2c.ps1` 产出的 `capture.jsonl`（带 `port/hop/ts_ms`，`raw_hex` 是 `#`…`!` 线上帧）。

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/m2-acceptance/export_s2c.ps1 `
  -ProxyDir "$W/csharp/proxy" -OutDir "$W/csharp/s2c"
#   期望：EXPORTED <...>\capture.jsonl
#         GREEN: Rust 侧 s2c 帧已导出为 capture.jsonl 且 replay 自洽
```

- 该脚本优先用仓库内已构建的
  `E:/Users/gxh/Documents/GitHub/OpenMir2/tools/capture/GoldenExport/bin/Release/net8.0/GoldenExport.exe`
  （今日存在），所以不会临时 `dotnet build`。
- 第三步 `cargo run -p replay` 是既有逐字节自洽门；若窗口不允许 cargo，可加 `-SkipReplay`，
  但要在证据里写明“replay 未跑”，不得把 `-SkipReplay` 说成自洽。
- 导出后确认 `capture.jsonl` 每行有 `port=7200`、`dir=s2c`、`raw_hex` 以 `#` 开头、`!` 结尾：
  ```powershell
  Get-Content "$W/csharp/s2c/capture.jsonl" -TotalCount 1
  #   期望含 "port": 7200, "dir": "s2c", "raw_hex": "#...
  ```

### 1.8 Rust 侧（混合栈 + 影子 7600 + 代理 7200）

#### 1.8.1 起混合栈并指向测试库

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/m2-acceptance/start_rust_stack.ps1 `
  -RustBinDir E:/tmp/m2svr-window-target/debug -Seed 42 `
  -LogDir "$W/rust/stack" -DbName mir2_test -DbHost 127.0.0.1
```

**期望输出（今日 `rust/start.log` 同形）**：

```text
DB_TARGET DBSrv database=mir2_test_db
DB_TARGET LoginSrv database=mir2_test_account
DB_TARGET game-svr database=mir2_test_data
DB_TARGET_SHADOW <WIN>\rust\stack\db-target
SEED: 将向 game-svr 注入 MIR2_RNG_SEED=42
...
START DBSrv pid=... ports=6000,5100,5700
START LoginSrv pid=... ports=5500,5600
START game-svr pid=... ports=5000
RUST_SEED_INJECTED=42
START login-gate pid=... ports=7000
START sel-gate pid=... ports=7100
START game-gate pid=... ports=7200,7201,7202
PIDFILE <WIN>\rust\stack\rust-stack.pid.json
...
RUST_STACK_OK <WIN>\rust\stack\rust-stack.pid.json
```

**断言影子配置真的指向测试库（三处）**：

```powershell
Select-String -Path "$W/rust/stack/db-target/DBSrv/dbsvr.conf",`
  "$W/rust/stack/db-target/LoginSrv/logsrv.conf",`
  "$W/rust/stack/db-target/M2GameSvr/Server.conf" -Pattern 'database=mir2_test_' |
  ForEach-Object { "{0}:{1}:{2}" -f $_.Path,$_.LineNumber,$_.Line }
#   期望：mir2_test_db / mir2_test_account / mir2_test_data 各一条
```

#### 1.8.2 只停 7200 的 game-gate → 起 Rust 影子 gate(7600)

`start_rust_stack.ps1` 会把 game-gate 起在 **7200/7201/7202**；代理只能抢 7200。
按 pid 文件停**整个 game-gate 进程**（它同时释放 7200/7201/7202）：

```powershell
$pf = Get-Content "$W/rust/stack/rust-stack.pid.json" -Raw | ConvertFrom-Json
($pf | Where-Object { $_.name -eq 'game-gate' }).pid |
  ForEach-Object { Stop-Process -Id $_ -Force -ErrorAction SilentlyContinue; "STOPPED game-gate pid=$_" }
Start-Sleep -Milliseconds 800

# 影子 conf：cp936 原样，只改 Count / GatePort1
$enc = [Text.Encoding]::GetEncoding(936)
New-Item -ItemType Directory -Force "$W/rust/shadow" | Out-Null
Copy-Item -LiteralPath 'E:\MirServer\RunGate\config.conf' -Destination "$W/rust/shadow/game-gate.conf" -Force
$t = [IO.File]::ReadAllText("$W/rust/shadow/game-gate.conf", $enc)
$t = $t -replace '(?m)(?<=^Count=)\d+(?=\r?$)','1' `
       -replace '(?m)(?<=^GatePort1=)\d+(?=\r?$)','7600'
[IO.File]::WriteAllText("$W/rust/shadow/game-gate.conf", $t, $enc)

# Rust game-gate 的 conf 是位置参数（bins/game-gate/src/main.rs:503-506）
$g = Start-Process -FilePath E:/tmp/m2svr-window-target/debug/game-gate.exe `
  -ArgumentList "$W/rust/shadow/game-gate.conf" `
  -WorkingDirectory "$W/rust/shadow" -WindowStyle Hidden `
  -RedirectStandardOutput "$W/rust/shadow/rust-gate.out.log" `
  -RedirectStandardError  "$W/rust/shadow/rust-gate.err.log" -PassThru
$g.Id | Set-Content "$W/rust/shadow/rust-gate.pid"
Start-Sleep -Milliseconds 1500

# 断言 7200 DOWN / 7600 UP
[System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners() |
  Where-Object { $_.Port -in 7200,7600 } | ForEach-Object { "LISTEN $($_.Port)" }
```

代理与客户端流程：把 §1.5 step 6 与 §1.6 的路径换成 `$W/rust/...` 即可；代理命令同形：

```powershell
$proxy = Start-Process -FilePath python `
  -ArgumentList @('-I','-B','E:/Users/gxh/Documents/GitHub/OpenMir2/tools/capture/mir2_proxy.py',
                  '--out',"$W/rust/proxy",'--map','7200=127.0.0.1:7600') `
  -WorkingDirectory 'E:/Users/gxh/Documents/GitHub/OpenMir2' -WindowStyle Hidden `
  -RedirectStandardOutput "$W/rust/proxy.log" `
  -RedirectStandardError  "$W/rust/proxy.err.log" -PassThru
$proxy.Id | Set-Content "$W/rust/proxy.pid"
```

> 今日 Rust 侧的 `setup-capture.ps1` 把 game-gate 的 pid 写死成 `15812`；本文件改成按
> pid 文件查 `name -eq 'game-gate'`，避免下一次 pid 变化后停错人。

### 1.9 今日踩过/核过的坑（按重要性排序）

1. **重定向日志的父目录必须先存在**。`Start-Process -RedirectStandardOutput/-RedirectStandardError`
   在父目录不存在时直接抛错；影子 gate、代理、客户端流程的日志目录都要先 `New-Item`。
   今日 `setup-capture.ps1` 先建 `shadow`，代理日志写 `<WIN>/<side>/proxy.log`，父目录是已有的
   `<WIN>/<side>`。`mir_flow -OutDir` / `export_s2c -OutDir` 会自建，但**重定向目录不会**。
2. **旧影子必须先删/归档**。`start-csharp-hook.ps1:97-108` 只在 `GameSrv.exe` 不存在时复制；
   复用旧影子 = 复用旧 `GameSvr.exe` + 旧 `Server.conf`（可能写生产库）。今日 `start3.log`
   有 `SHADOW_COPY`，`start.log`（上一趟 `stack2`）没有；以 `stack3` 为准。
3. **proxy dump 不是 judgment capture**。`mir2_proxy.py` 产 `chunks.ndjson`/`.bin`/`session.json`；
   判据要先 `export_s2c.ps1`。今日在原始 dump 上跑判据，实际得到：
   - `s2c_diff.py`：`PREREQ_MISSING: Rust 侧 capture.jsonl 里没有 port=7200 dir=s2c 的帧`（rc=2）；
   - `check_client_s2c_frames.py`：`RED: 52 body 解码失败：帧形状不符（需 # 开头 / ! 结尾 / 头 16 字符）`（rc=1）。
4. **7200 必须先释放**。C# 侧停 `RunGate`；Rust 侧停 `game-gate`（它同时占 7200/7201/7202）。
   代理启动报 `address in use` 就是这一步没做干净。
5. **gate conf 是 cp936，且只改两个数字**。`Count=1`、`GatePort1=7600`；其余 `ServerAddr/ServerPort`
   保持指向 5000。C# GameGate 读 `AppContext.BaseDirectory/config.conf`，所以必须**整目录拷贝**
   `RunGate` 再覆盖 conf，不能只改 `E:/MirServer/RunGate`。
6. **`ConnctionString` 是上游拼写**（少一个 `e`），不要改成 `ConnectionString`；改了组件读不到。
7. **账号/密码**：今日 `winplay0` 的密码长度 = 8（`LoginSrv` 日志），即密码也是 `winplay0`。
   账号必须在 LoginSrv 启动前存在于测试库（今日 `[QueryChr] 候选 account=winplay0 ... quickIndex=6231`）。
8. **MySQL 不是 Windows 服务**。`start_rust_stack.ps1 -Stop` 只在**它自己启动过 MySQL**时才登记并杀它；
   今日 3306 原本 UP（`MYSQL already up (3306)`），所以 `-Stop` 不碰 MySQL。若哪天 3306 原本 DOWN
   而由起栈器拉起，`-Stop` 会 Force 杀 `mysqld` ⇒ 改用 `start_full_rust_stack.ps1 -Stop`（跳过 MySQL）
   或按 §E3 授权手工处理。**永远禁止 `start-all.ps1 -Stop`**。
9. **不要用 symlink 把 `mir2-rs` 指向主检出**。今日 `<WIN>/csharp/repo/mir2-rs` 是符号链接，
   `start-csharp-hook.ps1` 的 `dotnet build` 因此把 `rng-hook/bin`、`obj` 写进了主检出
   （`git status` 仍是干净，因为是 ignored 构建产物，但仍违反“不改主检出”）。下一轮按 §1.2
   真实复制 `rng-hook` 源，或 `-SkipHookDeploy` + 预放 `RngSeedHook.dll`。
10. **7600 不在 `port-status.ps1` 的十端口里**。收窗前必须用 §1.5 step 5 的监听表单独断言 7600 DOWN。
11. **测试库会被客户端流程写脏**。今日 Rust 侧死亡后把 `Hp=0`、位置 `(288,620)` 写回
    `mir2_test_db`；下一轮每侧开始前必须 restore 到同一 base（§4.1），否则 C#/Rust 看到的不是同一输入。
12. **默认 `-WalkClicks 2` 会走路**。今日 Rust 的 `CM_WALK` 被挡/越界（没走成）但仍站着挨打；
    C# 走了 6 个 `10/41` 对并因此把 `654` 推到 seq28。判登录尾时走路会插入噪声帧，
   受控场景必须 `-WalkClicks 0`（§2）。

---

## 2. 受控登录场景（固定安全格、不走路、不打怪）

**目标**：让两侧在**同一 DB 状态、同一安全格**上只做「登录 → 进世界 → 站稳」，采到完整的登录帧尾
（`654 → 657×2 → 第二 42 → 100×4`），不被怪物/走位噪声污染。今日 Rust 侧在 `(288,620)` 被怪打死，
就是没有做这一步的代价。

### 2.1 选点：`(311,666)`（今日用两个只读工具复核过）

```powershell
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_reverse_point_clean.py `
  --point 311,666 --map 0 --vr 12 --envir-dir E:/MirServer/M2GameSvr/Envir
# 今日输出：最近对象 GuardList.txt (281,661) range=0，切比雪夫=30；VERDICT: CLEAN / RESULT: CLEAN

python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_mongen_dense_points.py `
  --mongen E:/MirServer/M2GameSvr/Envir/MonGen.txt --map 0 --view-range 12 --points "311,666;288,620"
# 今日输出：(311,666) 框外，最近生成框 Chebyshev=44 > 12；(288,620) 框内，Chebyshev=0
```

- **用 `(311,666)`**：无出生怪、无 AI 消费者（GuardList 距离 30 > vr 12）。
- **不要用 `(288,620)`/`(289,618)`**：出生点/刷怪框内，今日 Rust 就是在这里被围死。
- 若现场仍看到怪：用 `check_reverse_point_clean.py --scan-box x0,y0,x1,y1` 在城内再找净空点；
  不要凭记忆换点。

### 2.2 开窗前把角色钉到安全格（只写 `mir2_test_*`）

两侧用**同一个** base + **同一段** SQL；每次起栈前重跑：

```sql
SET @pid=(SELECT Id FROM mir2_test_db.characters WHERE ChrName='winplay0');

UPDATE mir2_test_db.characters
   SET MapName='0', CX=311, CY=666
 WHERE Id=@pid;

-- 满血，避免登录后立刻被环境伤害打死；MP 列名是上游的 MAxMP（大小写不敏感）
UPDATE mir2_test_db.characters_ablity
   SET Hp=MaxHP, Mp=MAxMP
 WHERE PlayerId=@pid;

-- 断言（两条都必须对）
SELECT MapName,CX,CY,Level,Job FROM mir2_test_db.characters WHERE Id=@pid;
--   期望：0 / 311 / 666 / 1 / 1（今日 winplay0 是 Level=1 Job=1）
SELECT PlayerId,Level,Hp,MaxHP,Mp,MAxMP FROM mir2_test_db.characters_ablity WHERE PlayerId=@pid;
--   期望：Hp=MaxHP、Mp=MAxMP
```

> 今日 `winplay0` 在测试库里的状态是 `(288,620)`、`Hp=0`、`MaxHP=16`（Rust 死亡存档写回）。
> 不重置就登录，会把「死亡状态」带进下一轮的 52/53/入图帧。
>
> 若本轮要采 `53 SM_HEALTHSPELLCHANGED`（**不在本文件 5 条判据内**），才把 `Hp` 设成
> `MaxHP-2`；否则满血更稳。

### 2.3 客户端参数（不走路、不说话、不打怪）

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/client/mir_flow.ps1 `
  -Account winplay0 -Password winplay0 -RunDir D:\MirClient-run `
  -OutDir "$W/<side>/flow" `
  -SkipTalk -WalkClicks 0 -SkipLogout -SkipReenter
```

- `-SkipTalk`：不发聊天，不产生 `SM_HEAR(40)` 回声。
- `-WalkClicks 0`：不点 `(902,650)`，不产生 `10 SM_TURN + 41 SM_FEATURECHANGED` 走位对，
  也不会有走进怪堆的风险。
- 不加 `-AttackClicks`、`-AutoPlaySec`、`-WalkPath`、`-AttackTurn`：不发 `CM_HIT/CM_TURN/CM_RUN`。
- `-SkipLogout`：留在世界里（不要小退/顶号）；`-SkipReenter`：不再点选角/进入。
- 流程结束仍会 `Stop-Process MirClinet`（除非 `-LeaveClientRunning`）⇒ 触发断线存档；
  这是预期行为，但会写测试库，所以每侧之间必须 restore（§2.4）。

### 2.4 侧间还原（今日没做，下一轮必须做）

今日 C# 先跑（23:21–23:29），Rust 紧接着跑（23:29–23:32），中间没有 restore；Rust 看到的是
C# 登录/走路写回后的状态（位置从 `289,618` 漂到 `288,620` 一类）。下一轮严格按：

```powershell
# C# 跑完 → 停 C# 栈 → 还原 base-test → 重新钉 (311,666) → 再起 Rust 栈
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/dbsnap/dbsnap.py --password= restore `
  --dir "$W/db/base-test" --db test --yes-apply
# 然后重跑 §2.2 的 SQL 与断言
```

`base-test` 在**任何一侧起栈之前**用 §4.1 的 `dbsnap snap --db test` 生成。

### 2.5 中止条件（任一命中即停，不判）

- `rust/stack/game-svr.out.log` 出现 `SM_NOWDEATH`/角色 HP 归零/`Death`；
- 导出 capture 里出现 `ident=34`，或 `ident=31` 且 `recog=self`（self = `SM_LOGON(50)` 的 recog）；
- 出现 `14 SM_HIT`/`31 SM_STRUCK` 的战斗对（即使是怪打怪，也会让登录尾时序抖动）；
- `52` 的 `body_len` 不是 40（C#）/58（Rust），或 `752` 缺失；
- 客户端流程没有 `FLOW_OK`。

命中后：停代理/影子 gate/主栈，restore base，换点或重钉后再来；**不得**拿带战斗噪声的 capture
去判 `654/第二 42/100×4`。

---

## 3. 判据清单（5 条，每条给命令与期望）

> 全部判据都吃 `export_s2c.ps1` 的 `capture.jsonl`（§1.7），**不是** proxy 目录里的临时 dump。
> 统一退出码：`0=GREEN / 1=RED / 2=PREREQ`；文本末行 `RESULT: GREEN|RED|PREREQ_RED`。

### 3.0 先导出（J0，所有判据的前置）

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/m2-acceptance/export_s2c.ps1 `
  -ProxyDir "$W/<side>/proxy" -OutDir "$W/<side>/s2c"
#   期望：EXPORTED ...\capture.jsonl
```

今日在**未导出**的 dump 上跑判据的实际报错（作为反例）：

```text
PREREQ_MISSING: Rust 侧 capture.jsonl 里没有 port=7200 dir=s2c 的帧
RED: 52 body 解码失败：帧形状不符（需 # 开头 / ! 结尾 / 头 16 字符）
```

### 3.1 J1：15 帧入图序

```powershell
$CHK = 'E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013'
# C# oracle 布局
python -I -B "$CHK/check_client_s2c_frames.py" `
  --capture "$W/csharp/s2c/capture.jsonl" `
  --expect-enter-seq default --expect-52-layout oracle40
# Rust client 布局
python -I -B "$CHK/check_client_s2c_frames.py" `
  --capture "$W/rust/s2c/capture.jsonl" `
  --expect-enter-seq default --expect-52-layout client58
```

**期望**（每侧）：

```text
enter_seq=658,51,50,41,213,11029,42,766,54,5008,1106,52,752,811,46
expect_enter_seq=658,51,50,41,213,11029,42,766,54,5008,1106,52,752,811,46
52_present=True body_len=40   # C#；Rust 为 58
752_present=True
RESULT: GREEN
```

**失败判据**：前缀任一帧缺失/多出/次序不对；`52` 或 `752` 缺失；`body_len` 与钉死的布局不符；
`RESULT: RED` 或 rc≠0。

> 今日在这条判据上：两侧的 `enter_seq` 都等于期望（15/15），唯一 RED 是 52 解码——因为喂的是
> 未导出的 dump（§3.0），不是序列本身的问题。

### 3.2 J2–J4：`654` 位置、第二 `42`、`100×4`

这三条是登录尾的相对次序，工具没有单独的 checker；用下面这段只读 Python 一次判完
（PowerShell 单引号 here-string，`$` 不会被展开）：

```powershell
$cap = "$W/<side>/s2c/capture.jsonl"
@'
import json, sys
p = sys.argv[1]
rows = [json.loads(l) for l in open(p, encoding='utf-8') if l.strip()]
# 只取 7200 s2c；单会话 capture 里 46 只出现一次
s = [r['ident'] for r in rows if r.get('dir') == 's2c' and r.get('port', 7200) == 7200]
PRE = [658,51,50,41,213,11029,42,766,54,5008,1106,52,752,811,46]
i46 = s.index(46)
assert s[max(0, i46-14):i46+1] == PRE, ('prefix', s[max(0, i46-14):i46+1])
i654s = [i for i, x in enumerate(s) if x == 654]
assert len(i654s) == 1, ('654 count', i654s)   # J2：654 恰一条且在 46 之后
i654 = i654s[0]
i657 = [i for i, x in enumerate(s) if x == 657]
i42  = [i for i, x in enumerate(s) if x == 42]
assert len(i42) == 2, ('42 count', i42)  # J3：恰两条 42
assert len(i657) == 2, ('657 count', i657)
assert i46 < i654 < i657[0] < i657[1] < i42[1], (i46, i654, i657, i42)
assert s[i42[1]+1:i42[1]+5] == [100,100,100,100], s[i42[1]+1:i42[1]+5]  # J4：100×4
print('LOGIN_TAIL_OK i46=%d i654=%d i657=%s i42=%s 100x4' % (i46, i654, i657, i42))
'@ | python -I -B - $cap
```

**期望**（下一轮受控登录）：

```text
LOGIN_TAIL_OK i46=14 i654=15 i657=[16, 17] i42=[6, 18] 100x4
```

（绝对下标取决于导出后 capture 里是否混入其它帧；判据只要求**相对次序**，不钉绝对下标。
今日 C# 因为走了两步，实际是 `i46=14 i654=27 i657=[28,29] i42=[6,30]`——同样通过相对次序判据；
今日 Rust 在 `42 count` 断言处 `AssertionError ('42 count', [6])` ⇒ RED，正是“被打死”的直接后果。）

**失败判据**：

- `654` 不在 `46` 之后，或出现超过 1 次；
- `42` 不是恰 2 条（少一条 = 没有 `RefShowName` 的第二条；多一条 = 混入别的会话/帧）；
- `657` 不是恰 2 条，或不在 `654` 与第二 `42` 之间；
- 第二 `42` 之后不是恰好 `100,100,100,100`（少/多/夹杂其它帧都算 RED）；
- 断言异常/`IndexError` ⇒ RED。

> **部署前提**：`100×4` 是 `TestServer=TRUE`（今日 `Server.conf:11`）+ `PayMent==3` 分支的形态
> （3 条登录 SysMsg + 1 条 `[攻击模式: 全体攻击]`）。若部署或角色付费态改了，`100×3`/`100×5`
> 可能是**正确**的另一种形态——但那属于新输入，必须重新登记，不得用本判据硬判 GREEN。
> 今日 C# 四条 100 的正文（GBK 解码）：
> `唯一客服QQ：79311185 唯一担保QQ：79311185` / `欢迎进入记忆传奇(月卡版)-官方网站：www.aresm2.com` /
> `禁止使用任何外挂、加速、辅助、脱机，一经发现，封号封IP` / `[攻击模式: 全体攻击]`。

### 3.3 J5：`52` 布局 40 vs 58（D-9 白名单）

C# oracle 的 `52 SM_ABILITY` 是 **40B**（`EncodeMessage(WAbil)`），Rust 生产默认是**冻结客户端
`TAbility` 58B**。`s2c_diff.py` 会逐字段比 `body_len/body_sha256/raw_hex`，三条全部必须登记到
D-9 白名单；只登记 `body_len`/`raw_hex` 不够（`body_sha256` 也会不同）。

白名单文件 `$W/judge/s2c-whitelist.jsonl`（每条必须带 `reason` + `reviewer`，缺一直接 RED）：

```jsonl
{"ident":52,"field":"body_len","reason":"D-9：oracle 40B vs Rust client-58 58B（owner 已裁决，whitelist.md D-9）","reviewer":"openmir2-reviewer-1"}
{"ident":52,"field":"body_sha256","reason":"D-9 同上：40B→58B 布局变化必然改变 body_sha256（s2c_diff:213 逐字段比）","reviewer":"openmir2-reviewer-1"}
{"ident":52,"field":"raw_hex","reason":"D-9 同上（52 body 布局差异）","reviewer":"openmir2-reviewer-1"}
```

```powershell
New-Item -ItemType Directory -Force "$W/judge" | Out-Null
# 把上面三行原样写入 $W/judge/s2c-whitelist.jsonl（UTF-8，无 BOM 也可以；每行一个 JSON）

python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/m2-acceptance/s2c_diff.py `
  --rust   "$W/rust/s2c/capture.jsonl" `
  --golden "$W/csharp/s2c/capture.jsonl" `
  --start-ident 658 --upto-ident 46 `
  --whitelist "$W/judge/s2c-whitelist.jsonl" `
  --out "$W/judge/g1-diff.json"
```

**期望**：

```text
GOLDEN idents(15): [658,51,50,41,213,11029,42,766,54,5008,1106,52,752,811,46]
RUST   idents(15): [658,51,50,41,213,11029,42,766,54,5008,1106,52,752,811,46]
WHITELISTED ident=52 field=body_len reviewer=openmir2-reviewer-1
WHITELISTED ident=52 field=body_sha256 reviewer=openmir2-reviewer-1
WHITELISTED ident=52 field=raw_hex reviewer=openmir2-reviewer-1
REPORT <...>/g1-diff.json
GREEN: 同序列逐字节 diff = 0
```

rc=0；`g1-diff.json` 里 `diffs == []`、`whitelisted_count == 3`（`whitelisted` 数组含 3 条）。

**失败判据**：

- 任一 `DIFF ident=52 field=...` 未登记 ⇒ RED；
- 白名单条目缺 `reason`/`reviewer` ⇒ 工具直接 RED（`§6.0-3`）；
- 同序列出现 `ident` 次序分叉 ⇒ `DIFF ident=... field=ident kind=order` ⇒ RED（这不是 D-9，
  是登录尾/入图序的问题，先回去看 J1/J2）；
- `--golden` 用了静态金标准（不是本次同 DB 状态的 C# 实时 capture）⇒ 可能带 A-9 假红；
  必须用本轮 C# capture。

**今日实测的 52 差异面**：

| 字段 | C# oracle | Rust |
| --- | --- | --- |
| `52 body_len` | **40** | **58** |
| `52 param/tag/series` | `25345 / 0 / 0` | `25345 / 0 / 0`（相同） |
| `52 body_sha256` | 与 40B 布局绑定 | 与 58B 布局绑定（必不同） |
| `52 raw_hex` | 40B body | 58B body（必不同） |

> 今日没有导出 capture，所以没有跑成 `s2c_diff`；但原始 dump 已能确认 `body_len=40/58` 与
> `param/tag` 相同。下一轮导出后按上面命令重跑即可。

### 3.4 判据工具自检（离线，不占窗口）

```powershell
python -I -B "$CHK/check_client_s2c_frames.py" --selftest     # 今日 28 条，SELFTEST GREEN
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/m2-acceptance/s2c_diff.py --selftest
#   今日 9 条，SELFTEST GREEN
python -I -B "$CHK/check_reverse_point_clean.py" --selftest  # 今日 GREEN
```

### 3.5 今日 vs 下一轮预期（速查）

| 判据 | 今日 C# | 今日 Rust | 下一轮期望（两侧） |
| --- | --- | --- | --- |
| J1 15 帧前缀 | 匹配 | 匹配 | GREEN |
| J2 `654` 位置 | 46 后（seq28，走位对之后） | 46 后（seq16） | 46 后、657 前 |
| J3 第二 `42` | 有（seq31） | **无** | 恰 2 条 |
| J4 `100×4` | 有（seq32–35） | **无** | 恰 4 条 |
| J5 `52` 布局 | 40B | 58B | 三条白名单 GREEN |
| 战斗噪声 | 无 | **9 条 self struck + NOWDEATH** | 0 |

---

## 4. E1–E4 状态（2026-10-23 本轮口径）

| 项 | 状态 | 本轮可用范围 |
| --- | --- | --- |
| **E1 测试库克隆** | **已建**（owner 2026-10-22 裁决） | 所有构造/起栈只准写 `mir2_test_*`；生产 `mir2_*` 不写 |
| **E2 影子内容** | **已批**（本轮口径） | 允许只读复制到影子包内改内容；本趟未用任何内容改动 |
| **E3 MySQL 停/起** | **待裁决** | G6 故障注入仍不可开窗 |
| **E4 C# 重编** | **允许**（owner 2026-10-22 裁决） | 可重编对比用 C#；今日**未用**（用部署件 pack） |

> 口径差异提示：仓库内最后一次书面记录 `b46805058` 的提交说明仍是
> `E1=测试库克隆、E4=允许重编；E2/E3 仍待裁决`。本表按**本轮任务口径**把 E2 记为已批；
> 开窗前请 owner 补一条签名 `status`/`comment` 消歧，避免“口头已批、板上待裁决”两套账。
> E1/E4 与 `docs/window-run-sheet-20261022.md` §E-0 一致。

### 4.1 E1：测试库克隆（已建；重置命令）

**今日只读核对（2026-10-23 本文件写作时）**：

```text
mir2_test_db       11 张表
mir2_test_account  13 张表
mir2_test_data      7 张表
mir2_test_db.characters       = 7338 行（与 mir2_db.characters 相同）
mir2_test_account.account     = 7477 行
mir2_test_data.stditems       = 1000 行
```

**一个必须登记的事实**：测试库的 **schema 默认字符集是 `latin1`**，而生产三库是
`utf8mb4_general_ci`；表级 collation 仍是 `utf8mb3_general_ci`（与生产表一致），所以今日
中文读写没有坏。**重置时请按生产 schema 重建 `utf8mb4`**，不要继续沿用 latin1 默认。

#### 4.1.1 克隆 / 重置（owner 侧；只读生产、只写测试库）

```powershell
$bin = 'D:\mysql\mariadb-10.11.19-winx64\bin'
$base = "$W/db/clone"
New-Item -ItemType Directory -Force $base | Out-Null

function Dump-Db([string]$db) {
  $sql = Join-Path $base "$db.sql"
  $p = Start-Process -FilePath "$bin\mysqldump.exe" `
    -ArgumentList '-h127.0.0.1','-uroot','--single-transaction','--routines','--triggers','--hex-blob',$db `
    -RedirectStandardOutput $sql -NoNewWindow -Wait -PassThru
  if ($p.ExitCode -ne 0) { throw "RED: mysqldump $db rc=$($p.ExitCode)" }
  $sql
}
function Reset-TestDb([string]$srcDb, [string]$testDb) {
  $sql = Dump-Db $srcDb
  & "$bin\mysql.exe" -h127.0.0.1 -uroot -e `
    "DROP DATABASE IF EXISTS $testDb; CREATE DATABASE $testDb DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci;"
  if ($LASTEXITCODE -ne 0) { throw "RED: create $testDb rc=$LASTEXITCODE" }
  $p = Start-Process -FilePath "$bin\mysql.exe" `
    -ArgumentList '-h127.0.0.1','-uroot',$testDb `
    -RedirectStandardInput $sql -NoNewWindow -Wait -PassThru
  if ($p.ExitCode -ne 0) { throw "RED: load $testDb rc=$($p.ExitCode)" }
}

Reset-TestDb 'mir2_db'      'mir2_test_db'
Reset-TestDb 'mir2_account' 'mir2_test_account'
Reset-TestDb 'mir2_data'    'mir2_test_data'
```

> `mysqldump <db>`（**不带** `--databases`）不会写 `CREATE DATABASE`/`USE`，因此可以安全灌进
> 另一个库名；**不要**用 `--databases` 直接灌 `mir2_test_*`，否则会把 `USE mir2_db` 带进去。
> `mysqldump` 的 `--hex-blob` 让二进制列以十六进制文本落盘，避免编码破坏。

#### 4.1.2 断言（克隆/重置后必须跑）

```powershell
& "$bin\mysql.exe" -h127.0.0.1 -uroot -N -e `
  "SELECT table_schema,COUNT(*) FROM information_schema.tables WHERE table_schema IN ('mir2_test_db','mir2_test_account','mir2_test_data') GROUP BY table_schema ORDER BY table_schema;"
#   期望：mir2_test_account 13 / mir2_test_data 7 / mir2_test_db 11

& "$bin\mysql.exe" -h127.0.0.1 -uroot -N -e `
  "SELECT (SELECT COUNT(*) FROM mir2_db.characters)=(SELECT COUNT(*) FROM mir2_test_db.characters), (SELECT COUNT(*) FROM mir2_account.account)=(SELECT COUNT(*) FROM mir2_test_account.account), (SELECT COUNT(*) FROM mir2_data.stditems)=(SELECT COUNT(*) FROM mir2_test_data.stditems);"
#   期望：1  1  1
```

#### 4.1.3 每趟 base 快照与还原（推荐，比每次重克隆快）

```powershell
# 开窗前（任何一侧起栈之前）：13 张测试库表
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/dbsnap/dbsnap.py --password= snap `
  --out "$W/db/base-test" --db test
#   期望：SNAP ... 13 张表；manifest.json 落地

# 每侧跑完、或收窗时：dry-run 先看计划
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/dbsnap/dbsnap.py --password= restore `
  --dir "$W/db/base-test" --db test
#   期望：RESTORE PLAN: ... + DRY-RUN: 未执行任何写库（加 --yes-apply 才真正灌库）

# 真正写测试库
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/dbsnap/dbsnap.py --password= restore `
  --dir "$W/db/base-test" --db test --yes-apply
```

> 今日**没有** `base-test` 快照（证据目录下没有 `db/`），测试库是脏的（`winplay0` 位置 288,620、
> `Hp=0`）。下一轮开窗前先按 4.1.1 重置一次，再按 4.1.3 拍 `base-test`；否则第一次 restore
> 没有可还原的干净基线。

### 4.2 E2：影子内容（本轮口径：已批；本趟未用）

- **允许**：把 `E:/MirServer` 的内容只读复制到 `<WIN>/csharp/pack` 或 `GameSvr-shadow`，在**影子内**
  改 `Envir/MonGen.txt`、`GuildBase/`、`Server.conf`（非 TestServer 等）；`E:/MirServer/**` 一字不动。
- **本趟没有做任何内容改动**：今日只改了三个 DB 连接串（§1.3），`GameSvr-shadow/Envir`、`GuildBase`
  都是 pack 的原样复制。
- 若下一轮要做行会 750/特殊怪/非 TestServer：
  - `GameSvr-shadow/Server.conf:11` 现在是 `TestServer=TRUE`；非 TestServer 要在**影子**副本改，
    并重新记录 `!deployed-from.json` 之外的“内容改动清单”（文件名 + sha256）。
  - `GuildBase/` 在 `GameSvr-shadow` 下建，不要手写 `Guildlist.txt` 的格式；优先用游戏内建会。
  - 任何内容改动都要单独登记，不能混进“只登录”的 capture。

### 4.3 E3：MySQL 停/起（待裁决；G6 不可开窗）

- 本机 `mysqld` 是 `D:\mysql\mariadb-10.11.19-winx64\bin\mysqld.exe` **进程**，不是 Windows 服务
  （`Get-Service *maria*,*mysql*` 无结果）。
- 未裁决前：**不得**停/起 3306；G6（DB 故障路径）整组 `not_collectible`。
- `start-all.ps1 -Stop` 永远禁用（会 Force 到 `mysqld`）。
- 若 owner 裁决，操作口径仍是 run sheet §E3：`Stop-Process -Name mysqld`（不要 `Stop-Service`）；
  起回 `Start-Process ...\mysqld.exe -ArgumentList '--datadir=D:\mysql\data','--port=3306','--bind-address=127.0.0.1','--console'`；
  并在证据里写“3306 独占、无其它车道、恢复后校验”。

### 4.4 E4：C# 重编（允许；今日未用）

- **允许**：重编对比用 C# 程序。今日没有重编：pack 直接复制 `E:/MirServer`，`GameSrv.dll` 的
  sha256 与部署件一致（`c67d3b0b...`），`!deployed-from.json.git_head=8a621f6a`（含 `d7ead5b2a`、
  `e114d7ff5`，F1 满足）。
- 若下一轮确实要重编（例如修 oracle 侧 bug 后要新基线）：
  ```powershell
  # DBSrv/LoginSrv：编到窗口的 repo 副本，不写主检出
  dotnet build E:/Users/gxh/Documents/GitHub/OpenMir2/src/DBSrv/DBSrv.csproj -c Release -o "$repo/src/DBSrv/bin/Release"
  dotnet build E:/Users/gxh/Documents/GitHub/OpenMir2/src/LoginSrv/LoginSrv.csproj -c Release -o "$repo/src/LoginSrv/bin/Release"
  # GameSvr：编到 pack/M2GameSvr（作为 -Pack 源），再让 start-csharp-hook 复制到全新 -Shadow
  dotnet build E:/Users/gxh/Documents/GitHub/OpenMir2/src/GameSrv/GameSrv.csproj -c Release -o "$pack/M2GameSvr"
  ```
  - 构建前记 `git -C E:/Users/gxh/Documents/GitHub/OpenMir2 rev-parse HEAD` 与 dirty 状态；
  - 构建后把 `DBSrv.dll`/`LoginSrv.dll`/`GameSrv.dll` 的 SHA256 + 构建时 HEAD 写入
    `<WIN>/csharp-provenance.json`（只写 `E:/tmp` 证据，不提交）；
  - **重编后必须重新跑 §1.4 的影子身份核对**：`!deployed-from.json` 可能仍指向旧 head，
    以 DLL hash + 构建记录为准，不能只看 json。
- `dotnet build` 的 `-o` 指向窗口副本；如果走 `start-csharp-hook` 的默认 `-Repo`，它会编到主检出
  `mir2-rs/tests/parity/m3/rng-hook/bin`（今日的 symlink 就发生了这件事，见 §1.9 坑 9）。

### 4.5 外部 C# 全栈与 oracle 源码漂移（E-0 记录）

- 5000–7200 的外部 C# 全栈是 **owner 自己启动的**（2026-10-20 12:35）；窗口需要停它时由 owner
  操作或经 owner 同意后停。**禁止** `start-all -Stop`；C# GameSvr 用
  `mir2-rs/tests/parity/evidence/M1/s4b-slice5-exitpaths/send_ctrlc.ps1 -Event 1`（CTRL_BREAK）。
- oracle 以**部署件/影子包二进制**为准；窗口 provenance 记录 C# 源码 HEAD。若另一条 zig 移植
  会话改了 `src/**` 并重新部署，对拍基线随之变化 ⇒ 必须重新钉并重跑相关判据。
- 今日窗口开始时九端口全 DOWN、7600 DOWN、3306 UP（2026-10-20 23:49 复核），说明 owner 栈当时
  已停；下一轮开窗前还要再复核一次。

---

## 5. 收窗清单（顺序不可换：代理 → 7600 影子 → 主栈 → 端口核对 → 测试库）

### 5.1 停代理（先释放 7200）

```powershell
# 用 session.json 的 pid，别按进程名误杀别的代理
$pid7200 = (Get-Content "$W/<side>/proxy/session.json" -Raw | ConvertFrom-Json).pid
Stop-Process -Id $pid7200 -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500
[System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners() |
  Where-Object { $_.Port -in 7200,7600 } | Format-Table Address,Port
#   期望：只剩 7600（影子 gate 还在）
```

### 5.2 停 7600 影子 gate

```powershell
# C# 侧
Stop-Process -Id (Get-Content "$W/csharp/shadow/csharp-gate.pid") -Force -ErrorAction SilentlyContinue
# Rust 侧
Stop-Process -Id (Get-Content "$W/rust/shadow/rust-gate.pid") -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500
[System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners() |
  Where-Object { $_.Port -eq 7600 } | Format-Table Address,Port
#   期望：无输出
```

### 5.3 停主栈

**Rust 侧**（`-Stop` 按 pid 文件逆序停；game-svr 走命名管道优雅停；MySQL 若没被它启动则不碰）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/m2-acceptance/start_rust_stack.ps1 `
  -LogDir "$W/rust/stack" -Stop
#   期望：STOPPED ... 或 ALREADY_GONE ...；最后十端口表
```

**C# 侧**（先优雅停 GameSvr，至少等 3 秒，再清网关/DBSrv/LoginSrv）：

```powershell
$pf = Get-Content "$W/csharp/stack/csharp-stack.pid.json" -Raw | ConvertFrom-Json
$gs = ($pf | Where-Object { $_.name -eq 'GameSvr' }).pid
powershell -NoProfile -ExecutionPolicy Bypass `
  -File E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/evidence/M1/s4b-slice5-exitpaths/send_ctrlc.ps1 `
  -TargetPid $gs -Event 1
#   期望：ATTACH ... / CTRL_EVENT event=1 sent=True；GameSvr 日志出现 Ctrl+C pressed
Get-Process -Id $gs -ErrorAction SilentlyContinue   # 期望：无输出
Start-Sleep -Seconds 3
powershell -NoProfile -ExecutionPolicy Bypass `
  -File E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/m3/start-csharp-hook.ps1 `
  -LogDir "$W/csharp/stack" -Stop
```

> 若这一趟只做登录帧、不需要 shutdown 存档证据，也仍按上面优雅停；`start-csharp-hook -Stop`
> 本身是 `Stop-Process -Force`，只用于 GameSvr 已退出后的清理。

### 5.4 端口核对

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File E:/MirServer/port-status.ps1
#   期望：缺失端口 = 5000,5100,5500,5600,5700,6000,7000,7100,7200；仅 3306 UP

[System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners() |
  Where-Object { $_.Port -in 7201,7202,7600 } | Format-Table Address,Port
#   期望：无输出（Rust game-gate 的 7201/7202 也必须随进程退出）
```

### 5.5 测试库：重置或保留（二选一，写进证据）

```powershell
# 默认：还原到本趟 base-test
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/dbsnap/dbsnap.py --password= restore `
  --dir "$W/db/base-test" --db test --yes-apply
# 断言关键行数/状态
& 'D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe' -h127.0.0.1 -uroot -N -e `
  "SELECT MapName,CX,CY FROM mir2_test_db.characters WHERE ChrName='winplay0'; SELECT Hp,MaxHP FROM mir2_test_db.characters_ablity WHERE PlayerId=(SELECT Id FROM mir2_test_db.characters WHERE ChrName='winplay0');"
```

- **保留**（例如要继续排查今日 Rust 死亡）：把 `winplay0` 的 `MapName/CX/CY`、`Hp/MaxHP`
  与 `characters_item` 行数写进证据，并注明“**下一轮开窗前必须先重置**”。
- **重置**：跑上面的 restore；若没有 base-test，按 §4.1.1 重克隆。
- 下一轮的默认处置：**重置**（今日测试库是脏的，不能带进下一轮；本文件写作时**没有**执行写库）。

### 5.6 工作树 / 证据 / 残留物

```powershell
git -C E:/Users/gxh/Documents/GitHub/OpenMir2 status --short      # 期望：无输出
git -C E:/Users/gxh/Documents/GitHub/OpenMir2 diff --stat -- src/ sql/   # 期望：无输出
```

- 证据落 `<WIN>/<side>/{stack,proxy,s2c,flow}/`；报告末段列「残留物清单」。
- 本文件所在的 w2runbook 车道只提交 `docs/window-round2-runbook-20261023.md`；
  不提交 `E:/tmp` 证据、不 push、不动 master。

---

## 6. 残留与未闭环（下一轮开始前先读）

1. **今日没有 judgment capture**：两个 proxy 目录只有 `chunks.ndjson`/`.bin`/临时 dump；
   `export_s2c.ps1` 没跑。下一轮 J1/J5 的第一步就是导出（§1.7）。
2. **今日没有 base-test 快照**：测试库已被 Rust 死亡存档写脏（`winplay0` `(288,620)`、`Hp=0`）。
   下一轮开窗前先 §4.1.1 重置 + §4.1.3 拍 base-test。
3. **Rust 登录尾未证**：今日 Rust 在安全格之外被打死，`654` 之后是 `10/41 + 14/31` 噪声，
   没有 `657×2 / 第二 42 / 100×4`。这是下一轮的主要目标，不是“已通过”。
4. **E2 书面记录不一致**：本表按本轮口径记“已批”，但 `b46805058` 仍写待裁决；开窗前补 owner 签名。
5. **E4 未实际使用**：今日用部署件 pack（`git_head=8a621f6a`），没有重编。若下一轮重编，
   必须重新记录 DLL hash + 构建 HEAD（§4.4）。
6. **`100×4` 的输入前提**：`TestServer=TRUE` + `PayMent==3`；非 TestServer/其它付费态是不同输入，
   不能拿本判据硬判。
7. **53/healthspell 不在本清单**：今日 C# 有 3 条 `53`，Rust 0 条；本文件只判 15 帧 + 登录尾 + 52 布局，
   不把 `53` 写成通过。若下一轮要收 `53`，按 runbook 82 §0 压血（`Hp=MaxHP-2`）。
8. **G3/G4/G6/G7 不在本文件**：战斗/法术/DB 故障/网关计时仍按 run sheet v3 的 `not_collectible`
   或另开窗口；本文件不借登录窗口的时间。
9. **“不改主检出”的今日偏差**：今日 `<WIN>/csharp/repo/mir2-rs` 是 symlink，`dotnet build` 把
   `rng-hook/bin|obj` 写进主检出（ignored，`git status` 仍干净）。下一轮按 §1.2 真实复制；
   这是流程偏差，不是代码改动。
10. **R 卡收口**：本文件只产出窗口证据；`review` 要写 `tier` + `tier_why`；R 卡在 `merge_result`
    后仍保持 `in-progress`，直到窗口项全绿。

---

## 附录 A：今日两侧帧序（只读 dump，仅供对照）

### A.1 C# oracle（47 条 s2c）

```text
1:658 2:51 3:50 4:41 5:213 6:11029 7:42 8:766 9:54 10:5008 11:1106 12:52 13:752 14:811 15:46
16:10 17:41 18:10 19:41 20:10 21:41 22:10 23:41 24:10 25:41 26:10 27:41
28:654 29:657 30:657 31:42 32:100 33:100 34:100 35:100
36:53 37:30 38:53 39:30 40:30 41:53 42:30 43:30 44:40 45:100 46:100 47:11
```

- 15 帧入图前缀 = `658,51,50,41,213,11029,42,766,54,5008,1106,52,752,811,46`。
- `654` 在 6 个走位对（10/41）之后；`657×2`、第二 `42`、`100×4` 紧随其后。
- `52 body_len=40`；`53` 3 条；无 `14/31/34`。
- seq45–46 还有两条 `100`（正文 `权限不够!!!`），是客户端发 `@AttackMode` 的提示，**不属于登录尾**；
  J4 只判第二 42 之后的连续四条。

### A.2 Rust（53 条 s2c）

```text
1:658 2:51 3:50 4:41 5:213 6:11029 7:42 8:766 9:54 10:5008 11:1106 12:52 13:752 14:811 15:46
16:654 17:10 18:41 19:10 20:41 21:10 22:41 23:10 24:41 25:10 26:41
27:14 28:31 29:14 30:14 31:31 32:14 33:31 34:14 35:14 36:31 37:14 38:14 39:14 40:14 41:14 42:14
43:31 44:14 45:31 46:14 47:31 48:14 49:31 50:14 51:34 52:31 53:40
```

- 15 帧前缀同样匹配；`654` 在 seq16（46 之后）。
- 之后没有 `657/第二 42/100×4`，而是 `10/41` 与 `14/31`；`31` 的 `recog=1953`（自己），
  `param=13..0`，seq51 `34 SM_NOWDEATH`。
- `52 body_len=58`。

### A.3 今日四类关键帧的字段（C#，供下一轮对照）

| ident | 含义 | 今日 C# 字段 |
| --- | --- | --- |
| 654 | `SM_CHANGELIGHT` | `param=0, tag=500, series=0, body_len=0` |
| 657 | `SM_CHARSTATUSCHANGED` | `param=0, tag=0, series=0, body_len=0`（两条） |
| 42（第二） | `SM_USERNAME` | `param=255, tag=0, series=0, body_len=5`，body `25 73 5c 5c 5c`（`%s\\\`），与第一条 42 同 body |
| 100（×4） | `SM_SYSMESSAGE` | `param=65499, tag=0, series=1`；body_len 41/49/54/20 |

> 下一轮受控登录的期望是：两侧都出现同样的相对次序；字段值以各自侧为准（C# 40B / Rust 58B
> 只影响 52，不影响 654/657/42/100 的头字段）。
