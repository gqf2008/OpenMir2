# 真机窗口第三轮 Runbook（OpenMir2，2026-10-21）

> **tier=L（纯文档 / 命令配方）**。本文件不启动任何栈、不占端口、不写数据库、不跑 cargo；
> 只把**本文件写作时（2026-10-21 18:20–18:40 +08）实读现仓 + 现场只读核对**得到的读数、
> 前置门、六条 R 卡判据与可照抄配方固化下来。
>
> - **代码基线**：`master = 8b8cb10d7c664f99d6d9e1e2bd567c9eaa06ee70`（本地 `master` 与
>   `refs/remotes/walgit/master` 同值；`origin/master` 已落后，见 §1.1）。开窗当天必须重新解析，
>   **不得沿用本文件的 oid 当长期合同**。
> - **窗口工作目录（本文件统一用 `$WIN`）**：`E:/tmp/window/20261021-r3`（新建，避免覆盖
>   `E:/tmp/window/20261023` 的第二轮证据）。
> - **构建源 / 产物**：`E:/tmp/m2svr-window-origin` → `E:/tmp/m2svr-window-target/debug`。
> - **判据脚本目录（`$CHK`）**：`E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013`。
> - **场景包（`$PKG`）**：`E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/m3/window-20261023`。
> - 判级沿用 `AGENTS.md`「判据分级」：本文件列出的六项都是 **R**；窗口未绿不得 `closed`。
> - **写集 = 本 runbook + 两个新脚本**：`docs/window-round3-runbook-20261021.md`（追加 §7「单二进制用法」，位置在 §6 之后、附录 A 之前）与 `tools/m2-acceptance/{start_mir2_server.ps1,stop_mir2_server.ps1}`；未改任何角色代码/配置/数据文件。

---

## 0. 开窗结论：当前 **NO-GO**（先看这一节）

本文件写作时逐项实测，**机器不满足开窗前置**。任何一项未清就开窗，得到的 capture 不能用于收卡。

| # | 门 | 本文件写作时实测 | 结论 |
|---|---|---|---|
| G1 | 镜像构建源 == 当前 master 的生产路径 | 镜像 HEAD=`ab44031ce`，`walgit/master`=`8b8cb10d7`，落后 **27 个提交 / 31 个生产文件** | **RED**（R6） |
| G2 | 预构建二进制 ↔ 构建源出处 | provenance `source_head=ab44031ce…`，六件 exe sha256 与清单逐字节一致 | **GREEN 但过期**（R7 自洽；源不是基线） |
| G3 | 九游戏端口全 DOWN / 7600 DOWN / 3306 UP | 九游戏端口**全部 LISTENING**（外部 C# 全栈），7600 DOWN，3306 UP | **RED**（R8） |
| G4 | 测试库 `mir2_test_*` 已从生产克隆 | 三库存在，表数 11/13/7，关键表行数 7338/7477/1000 与生产相等 | **GREEN**（手工只读 SQL；`preflight.py` P3 自身有 bug，见 §1.6） |
| G5 | `openmir2-merchant-race-bias` 已合入构建源 | **未合入**：只在 `/e/tmp/wt-mrace` 工作树里，`entity.rs`/`merchant.rs` 未提交 | **BLOCKED**（§2.5） |
| G6 | 城堡 SuperMan 门/墙伤害路径内容可达 | 部署件 `Castle/Castle/SabukW.txt` 的门/墙名不在怪物库 ⇒ 启动 **0 个城堡实体**；12 项掩码无客户端可达的直接 Die 入口 | **BLOCKED**（§2.6） |

> **一句话**：当前二进制是 `ab44031ce`（2026-10-21 11:34 构建），**本轮六条判据要收的修复全都不在里面**
> （§1.7 逐条 `--is-ancestor` 实测为 NO）。即使端口清空，也必须先按 §3.2 在 master 上重建，
> 否则窗口测的是旧件。

---

## 1. 前置条件与当前实测状态

### 1.1 镜像 HEAD 与 master 的差异

```bash
cd /e/tmp/m2svr-window-origin
git rev-parse HEAD                                   # ab44031ce59f40169fe3d473e267cba54aa8d82b
git status --porcelain                               # （空）
git rev-parse master                                 # 8b8cb10d7c664f99d6d9e1e2bd567c9eaa06ee70
git rev-parse refs/remotes/walgit/master             # 8b8cb10d7c664f99d6d9e1e2bd567c9eaa06ee70
git rev-parse refs/remotes/origin/master             # 189932857cff2a95420302c7862cfa4beffce285（GitHub 镜像，已落后）
git rev-list --count ab44031ce..8b8cb10d7            # 27
git diff --stat ab44031ce..8b8cb10d7 | tail -1       # 41 files changed, 12735 insertions(+), 6848 deletions(-)
```

- 镜像当前是 **detached HEAD**（`ab44031ce`），但共享仓库里的 `master` 引用已经是 `8b8cb10d7`；
  **不能**在该 worktree 里直接 `git checkout master`（master 已被主检出占用，git 会拒绝），
  必须 `git checkout --detach master`（或 `--detach refs/remotes/walgit/master`）。
- R6 的「生产路径」差异 = **31 件**（`mir2-rs/bins`、`mir2-rs/crates`、`mir2-rs/Cargo.lock`、
  `src`、`sql` 口径）；非生产面（docs/tests/parity 证据件）另有 10 件。
- **`origin/master` 不是基线**：它停在 `189932857`。`check_rust_window_preflight.py` 的默认
  `--baseline-ref origin/master` 会把一个更旧的起点当基线；本轮必须显式
  `--baseline-ref refs/remotes/walgit/master`（§3.1）。

### 1.2 `window-provenance.json` 的 source_head

```bash
cat /e/tmp/m2svr-window-target/window-provenance.json
# version=1
# source_head=ab44031ce59f40169fe3d473e267cba54aa8d82b
# target_dir=E:\tmp\m2svr-window-target
# bins: db-srv.exe / game-gate.exe / game-svr.exe / login-gate.exe / login-srv.exe / sel-gate.exe
```

本文件写作时逐件实算（`Get-FileHash -Algorithm SHA256`）：

| exe | bytes | sha256（前 16） | 与清单 | mtime |
|---|---:|---|---|---|
| `db-srv.exe` | 90494823 | `f02fe0607419a991` | match | 2026-10-21 11:34 |
| `game-gate.exe` | 38757516 | `f0d80a684880543c` | match | 2026-10-21 11:34 |
| `game-svr.exe` | 158171831 | `1b4772cc1d83260a` | match | 2026-10-21 11:34 |
| `login-gate.exe` | 31839422 | `ca6bd88d47e5ff9e` | match | 2026-10-21 11:34 |
| `login-srv.exe` | 84849902 | `27bf87bd9a3cc513` | match | 2026-10-21 11:34 |
| `sel-gate.exe` | 34931228 | `99538a6c07913717` | match | 2026-10-21 11:34 |

⇒ **R7 GREEN（自洽）**，但 `source_head` 是旧基线 ⇒ **R6 RED**（§1.1）。重建后必须重录清单
（§3.2），否则 R7 会把旧件洗成新源（工具注释已登记该信任边界）。

### 1.3 测试库 `mir2_test_*` 是否存在（只读核对）

本文件只用 `mysql.exe -e "SELECT …"` 读，**未连库写入**：

```bash
M=/d/mysql/mariadb-10.11.19-winx64/bin/mysql.exe
"$M" -h127.0.0.1 -P3306 -uroot -N -e "SHOW DATABASES;"
```

实测存在：`mir2_account` / `mir2_data` / `mir2_db`（生产）、`mir2_test_account` / `mir2_test_data` /
`mir2_test_db`（测试），另有 `mir2_dbsrv6k_mkt_1352_0` / `mir2_dbsrv6k_mkt_29292_0`（历史件，不参与本窗）。

| 断言 | 实测值 |
|---|---|
| 表数 | `mir2_test_db=11` / `mir2_test_account=13` / `mir2_test_data=7` |
| 关键表行数 | `mir2_db.characters=7338` = `mir2_test_db.characters`；`mir2_account.account=7477` = 测试；`mir2_data.stditems=1000` = 测试 |
| schema 默认字符集 | **测试三库 = `latin1` / `latin1_swedish_ci`；生产三库 = `utf8mb4` / `utf8mb4_general_ci`** |

> 字符集差是既有事实（第二轮 runbook §4.1 已登记）：表级 collation 与生产一致，中文当前没坏；
> 但**重置时必须按 §3.3 用 `utf8mb4` 重建**，不要沿用 latin1 默认。

### 1.4 十端口读数（`E:/MirServer/port-status.ps1`）

本文件写作时（2026-10-21 18:27）实跑：

```text
mysqld      pid=14964   port=3306
GameSrv     pid=19624   port=5000
DBSrv       pid=5144    port=5100
LoginSrv    pid=17780   port=5500
LoginSrv    pid=17780   port=5600
DBSrv       pid=5144    port=5700
DBSrv       pid=5144    port=6000
LoginGate   pid=25856   port=7000
SelGate     pid=6164    port=7100
GameGate    pid=4452    port=7200
十个端口全部在监听
```

- 这是**外部 C# 全栈**（进程启动时间 13:30:29–13:31:02），不是本文件起的栈；`7600` 不在该脚本
  的十端口内，单独断言为 **DOWN**。
- **与任务描述的口径差**：任务文字写「现在 8 个游戏端口在听」，实测是 **9 个游戏端口**
  （5000/5100/5500/5600/5700/6000/7000/7100/7200）**+ 3306**，共 10 个。以实测为准。
- 该外部栈**没有可发现的 `*-stack.pid.json`**：`start_rust_stack.ps1 -Stop` 在这里只会
  `NOTHING_TO_STOP`；**禁止 `E:/MirServer/start-all.ps1 -Stop`**（Force 且连带杀 `mysqld`）。
  只能由起栈会话/owner 按它自己的方式优雅停（`docs/window-run-sheet-20261022.md` §A1 R8 末段）。
  **停不掉 ⇒ 窗口不开**（§5 F1）。

### 1.5 客户端与工具实况

```powershell
Get-Item D:\MirClient-run\MirClinet.exe | Select-Object Length,LastWriteTime
# Length=6561280  LastWriteTime=2026/10/10 0:55:22
(Get-FileHash D:\MirClient-run\MirClinet.exe -Algorithm SHA256).Hash
# C3CB8F73D8872C5A…（前 16；与第二轮同一冻结客户端）
Get-Process MirClinet -ErrorAction SilentlyContinue   # 无输出（当前无客户端）
```

- 客户端地图件存在：`D:/MirClient-run/Map/{0122,3,b013}.map`（本轮三个固定场景都要用）。
- `MirClient` 源仓当前 **未提交文件=7**（`.gitignore` + 6 个未跟踪文件，见 `port-status.ps1` 输出）。
  不是本线改的；本窗只用冻结 exe，**不得**在 `MirClient/` 写任何文件。
- 工具离线自检（本文件写作时实跑，均不连库/不起栈）：`preflight.py --selftest` **GREEN 25 条**、
  `judge_login.py --selftest` **GREEN 35 条**、`run_scenario.ps1 -Selftest` **GREEN 116 条**。

### 1.6 两道前置门的实测读数（照抄即可复现）

**(a) 场景包 `preflight.py`**：

```powershell
$PKG='E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/m3/window-20261023'
python -I -B "$PKG/preflight.py" --win E:/tmp/window/20261023
```

本文件写作时实测：

```text
P1_PORTS RED
  九游戏端口仍有监听：5000,5100,5500,5600,5700,6000,7000,7100,7200（开窗前必须全 DOWN）
  INFO 7600 DOWN
  INFO 3306 UP
P2_MIRROR_PROVENANCE GREEN
  INFO mirror HEAD=ab44031ce…；provenance source_head == 镜像 HEAD；六件 exe sha256 OK
P3_TEST_DB_CLONE PREREQ_RED
  INFO 三库表集合与生产相同（11/13/7）
  关键表行数读不到：mir2_db.characters / mir2_test_db.characters（…account、…stditems 同）
RESULT: RED
```

> **P3 的工具缺陷（本文件写作时实测，必须登记）**：`preflight.py::collect_db_state()` 只对
> `KEY_TABLES`（**生产**三张）发 `SELECT COUNT(*)`，从未查测试库的对应表；`check_test_db()` 却
> 用 `db["counts"].get("mir2_test_db.characters")` 取测试行数 ⇒ 恒为 `None` ⇒ **P3 必然
> `PREREQ_RED`**。`--selftest` 因为注入的是含测试 counts 的合成数据，所以 25 条全绿，掩盖了该缺陷。
> 在工具修好前，P3 的**替代门**是 §1.3 + §3.3 的两条手工只读 SQL；不得因为 P3 红就跳过克隆断言，
> 也不得把 P3 的 `PREREQ_RED` 误读成「测试库缺失」。

**(b) 完整 Rust 窗口 guard**：

```powershell
$CHK='E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013'
python -I -B "$CHK/check_rust_window_preflight.py" `
  --repo E:/tmp/m2svr-window-origin --ref HEAD `
  --baseline-ref refs/remotes/walgit/master `
  --mirserver E:/MirServer `
  --target-dir E:/tmp/m2svr-window-target `
  --provenance E:/tmp/m2svr-window-target/window-provenance.json `
  --points "291,666:outside;291,651:outside;289,618:inside;287,620:inside" `
  --slot-log-dir E:/tmp/window/20261021-r3/rust/stack
```

本文件写作时实测（尾部）：

```text
[OK] R1 … 五片修复提交全部 --is-ancestor=YES
[OK] R2 … 源码含 ③ 读数标记
[OK] R3 … MapInfo 第 26 个 section 的逻辑名 == '0'
[OK] R4 … 四个坐标点 inside/outside 全 rc=0
[OK] R5 … 判据工具齐备（4 件）
[OK] R7 … source_head=ab44031ce5，6 件 exe sha256 逐字节一致
[OK] R8 无 game-svr.exe 在跑
RED R6: 构建源 HEAD=ab44031ce5 落后于基线 refs/remotes/walgit/master@8b8cb10d7c 的生产路径（31 件…）
RED R8: 窗口槽位被占 —— 游戏端口仍在 LISTENING: 5000(pid 19624)…7200(pid 4452)
RED R8: 占用端口的栈无法从推荐停栈目录 E:/tmp/client-e2e/stack2 找到 pid 文件…
RESULT: RED
```

### 1.7 六条判据的修复是否在当前二进制里（逐条实测）

当前产物由 `ab44031ce` 编出；下列提交都在它之后：

```bash
cd /e/Users/gxh/Documents/GitHub/OpenMir2
for c in 1cca49d22 b04e78e2a 4dce0717e 29356ae86 1828c6d7b 8b8cb10d7 14d4ec35d 3bdc9c09a; do
  printf '%-12s ' "$c"; git merge-base --is-ancestor "$c" ab44031ce && echo YES || echo NO
done
# 实测全部 NO
```

| 判据 | 关键提交 | 在 `ab44031ce` 里？ |
|---|---|---|
| 登录 `657×2` | `1cca49d22`（登录 RecalcAbilitys 补两条 657） | **NO** |
| 怪物 `SM_WALK(11)` | `b04e78e2a`（走位下发 SM_WALK） | **NO** |
| T-142 `SM_PLAYDICE` | `4dce0717e` + `29356ae86`（RM_PLAYDICE 排水 + wired） | **NO** |
| T-137 固定 NPC 菜单 | `1828c6d7b` + `8b8cb10d7` | **NO** |
| SuperMan（守卫/城门/城墙/固定 NPC/商人） | `14d4ec35d` + `3bdc9c09a` 等 | **NO** |
| 商人 race bias | 未合入（只在 `/e/tmp/wt-mrace` 未提交） | **NO** |

⇒ **不重建 = 六项全部不可收**。

---

## 2. 本轮要收的 R 卡判据（逐条给命令与期望）

> 统一前置：每侧起栈前重跑摆位（§3.3/§3.9），两侧**串行**（C# 先、Rust 后，中间 restore）；
> 判据一律吃 **`export_s2c.ps1` 产出的 `capture.jsonl`**，**不得**用 `mir2_proxy.py` 的临时
> dump（第二轮实测：临时 dump 会让 `s2c_diff.py` 报 `PREREQ_MISSING: …没有 port=7200 dir=s2c 的帧`，
> 让 `check_client_s2c_frames.py` 报 `RED: 52 body 解码失败：帧形状不符`）。

### 2.1 登录 `657×2`（卡 `openmir2-login-charstatus-657`，T-147，tier=R）

**判据**：C# oracle 实测尾序 `654,657,657,42`（金标准
`tests/golden/session-20261010-c3-hook/capture.jsonl` 的 index 47..50；两条 657 逐字节相同）。
Rust 侧修复后必须复采到**同序**：

- `654` 恰 1 条，且在 `46` 之后；
- `657` 恰 2 条，且 `i46 < i654 < 657[0] < 657[1] < i42[1]`；
- `42` 恰 2 条；第二 `42` 之后恰 `100,100,100,100`；
- `657` 载荷 `Recog=自己, Param=0, Tag=0, Series=0`（T-147：`Series` = 收件人 `HitSpeed`；
  部署角色为 0）。

**场景**：G1（`winplay0`，map `0`，`(311,666)`，Level 1，HP 16/16；不走路、不说话、不打怪）。

```powershell
# 1) 摆位（只写 mir2_test_db；两侧各跑一次）
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/loginfixture/prepare_safe_login.py `
  --db mir2_test_db --char winplay0 --level 1 --hp 16 --max-hp 16 --yes-apply
#   期望：BEFORE/AFTER 行快照；MapName=0 CX=311 CY=666；Hp=16 MaxHP=16

# 2) 起栈 + 影子 7600 + 代理 7200（§3.4–§3.6），客户端流程（§3.7）
# 3) 导出（§3.8）后判：
$PKG='E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/m3/window-20261023'
python -I -B "$PKG/judge_login.py" --scenario G1 --side csharp `
  --capture E:/tmp/window/20261021-r3/csharp/s2c/capture.jsonl `
  --out E:/tmp/window/20261021-r3/judge/g1-csharp.json
python -I -B "$PKG/judge_login.py" --scenario G1 --side rust `
  --capture E:/tmp/window/20261021-r3/rust/s2c/capture.jsonl `
  --golden  E:/tmp/window/20261021-r3/csharp/s2c/capture.jsonl `
  --out E:/tmp/window/20261021-r3/judge/g1-rust.json
```

**期望（每侧末行）**：`RESULT: GREEN`；逐条：

```text
J0 GREEN rows=… s2c7200=…
J1 GREEN enter_seq=658,51,50,41,213,11029,42,766,54,5008,1106,52,752,811,46
        52_present=True body_len=40   # C#；Rust 为 58（D-9/client58）
        752_present=True
J2 GREEN 654 恰一条且在 46 之后
J3 GREEN 42 恰两条；657 恰两条；i46 < i654 < 657[0] < 657[1] < i42[1]
J4 GREEN 第二 42 之后 100×4
J5 GREEN（同序列 diff=0）或 KNOWN_PENDING（= RED，待查，不得写成 GREEN）
```

**C# 侧先判**：C# 自己的 G1 必须 `J1..J5` 全绿（oracle 基线）。若 C# 侧 J3 就红，先查摆位/流程噪声，
**不要**继续判 Rust。

**RED 条件**：`657` 条数 ≠ 2、次序不符、`42` 条数 ≠ 2、`100×4` 不齐、`52` 布局不符、
`RESULT: RED|PREREQ_RED`。

**证据**：`$WIN/judge/g1-{side}.json` + 两侧 `s2c/capture.jsonl` + `flow/` 截图。

### 2.2 怪物 `SM_WALK(11)` 条数与时机（卡 `openmir2-monster-smwalk` / `openmir2-monster-walk-frames`，T-146，tier=R）

**缺陷基线**：审计 `audit-monster-attack-cadence-20261019.md` §B.1 实测 —— Rust 13 个 capture
**0 条** `SM_WALK`；C# 8 个有效 capture **17–76 条**。

**修复后的判据**（T-146 口径）：

1. Rust 侧 `SM_WALK(11)` **> 0**（原缺陷的直接否证）；
2. 每次识别出的走位**一帧**：同一 `recog` 的相邻 `SM_WALK` 的 `(param,tag)` 必须与上一次已知位置
   （`SM_TURN(10)`/`SM_RUN(13)`/上一条 `SM_WALK`）**切比雪夫距离 = 1**；
3. `series & 0xFF` 必须等于 `(dx,dy)` 对应的方向（0=上 … 7=左上；`walk_to` 先置 `Dir` 再走）；
4. 不得出现 `recog = 自己`（`SM_LOGON(50)` 的 recog）的 `SM_WALK`；
5. C# 侧同场景条数落在 **17–76**；Rust 与 C# **不要求条数相等**（世界状态/漫游随机不同），
   只要求「每次真走位一帧、字段/方向自洽」。

**场景**：G3A 的 `attack` 子场景（map `0`，`(605,578)`，Level 30，`-AttackClicks 20`）——
该点同屏有主动怪，能采到怪走位；**不要**用 G1 的 `(311,666)`（该点刻意选为无 AI 消费者）。

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File "$PKG/run_scenario.ps1" `
  -Scenario G3A -SubRun attack -Side csharp -Win E:/tmp/window/20261021-r3 -Go -Yes
# 停 C# 栈 → restore base → 再跑 -Side rust
```

导出后跑下面的只读判据（脚本见附录 B.1）：

```powershell
python -I -B E:/tmp/window/20261021-r3/judge/smwalk.py `
  E:/tmp/window/20261021-r3/csharp/g3a-attack/s2c/capture.jsonl | Tee-Object `
  E:/tmp/window/20261021-r3/judge/smwalk-csharp.txt
python -I -B E:/tmp/window/20261021-r3/judge/smwalk.py `
  E:/tmp/window/20261021-r3/rust/g3a-attack/s2c/capture.jsonl | Tee-Object `
  E:/tmp/window/20261021-r3/judge/smwalk-rust.txt
```

**期望**：

```text
# C#
SM_WALK=NN actors=K self_frames=0 non_adjacent=0 dir_mismatch=0      # 17 <= NN <= 76
# Rust
SM_WALK=NN actors=K self_frames=0 non_adjacent=0 dir_mismatch=0      # NN >= 1
```

**已知残余（T-146 登记，判据必须允许）**：① ≥2 格整段位移（瞬移两步）直接跳过 ⇒ 零帧；
② 收件人按 tick 末坐标算，±12 边界玩家可能与 oracle 差一格；③ 生成/进图后第一 tick 只建游标；
④ 恰好 1 格且方向一致的传送可能被误判为走位（当前怪路径不可达）。**这些残余不能用来解释
`SM_WALK=0` 或 `non_adjacent>0`**。

**RED 条件**：Rust `SM_WALK=0`；`non_adjacent>0`；`dir_mismatch>0`；`self_frames>0`；
C# 侧不在 17–76（说明场景没采到怪走位，判据不成立）。

### 2.3 掷骰动画 + `SM_PLAYDICE(8001)`（T-142 宿主接线，tier=R）

**生产路径**：`PlayObject.Message.cs:2034-2041` 的 `RM_PLAYDICE` 支 ⇒ `SM_PLAYDICE(8001)`：
`recog = 入队 NPC ActorId`、`wParam = 脚本 nParam1`、WL 三参 = `MDyVal` 打包、尾 = `EncodeString(sParam2)`。
Rust 侧由 `pickwire::script_frame` 的 `RM_PLAYDICE` 分支 + 脚本帧排水发射（`4dce0717e`）。

**可达脚本（部署件实读）**：

- NPC：`E:/MirServer/M2GameSvr/Envir/Merchant.txt:145`
  `赌博NPC/赌场  B013  8  10  〖庄〗  0  12  0` ⇒ 地图 `B013`（赌场，`MapInfo.txt:8` `SAFE`），
  坐标 `(8,10)`；脚本 `Envir/Market_Def/赌博NPC/赌场-B013.txt`（GBK）。
- 菜单链：`@main` → `开-始-下-注/@kaiz` → `押5个筹码包/@ya8` → `小/@ya82`；
  `@ya82` 的 `#ACT` = `take 筹码包 5; movr d0 6; inc d0 1; playdice 1 @cube82`
  ⇒ 这是部署件里**生产可达**的 `PLAYDICE` 点。
- 物品：`筹码包` 在 `mir2_test_data.stditems` 的 `Id=44`（`StdMode=31`、`NeedLevel=0`）；
  `筹码` **不在库**（脚本 `@yima6` 那条不可用）。因此走 `@ya8 → @ya82`。

**摆位与背包（只写 `mir2_test_db`）**：

```powershell
# 1) 角色摆到赌场 (8,11)，面北对 (8,10) 的 〖庄〗
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/loginfixture/prepare_safe_login.py `
  --db mir2_test_db --char winplay0 --map B013 --x 8 --y 11 --level 30 --hp 500 --max-hp 500 --yes-apply

# 2) 给 5 个 筹码包（StdIndex=44）到空背包位 1..5；--- 只在 mir2_test_db ---
$M = 'D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe'
$sql = @'
SET @pid=(SELECT Id FROM mir2_test_db.characters WHERE ChrName='winplay0');
UPDATE mir2_test_db.characters_bagitem SET MakeIndex=99000001, StdIndex=44, Dura=0, DuraMax=0 WHERE PlayerId=@pid AND Position=1;
UPDATE mir2_test_db.characters_bagitem SET MakeIndex=99000002, StdIndex=44, Dura=0, DuraMax=0 WHERE PlayerId=@pid AND Position=2;
UPDATE mir2_test_db.characters_bagitem SET MakeIndex=99000003, StdIndex=44, Dura=0, DuraMax=0 WHERE PlayerId=@pid AND Position=3;
UPDATE mir2_test_db.characters_bagitem SET MakeIndex=99000004, StdIndex=44, Dura=0, DuraMax=0 WHERE PlayerId=@pid AND Position=4;
UPDATE mir2_test_db.characters_bagitem SET MakeIndex=99000005, StdIndex=44, Dura=0, DuraMax=0 WHERE PlayerId=@pid AND Position=5;
SELECT Position,MakeIndex,StdIndex FROM mir2_test_db.characters_bagitem WHERE PlayerId=@pid AND Position BETWEEN 1 AND 5;
'@
& $M -h127.0.0.1 -uroot -e $sql
#   期望：5 行 StdIndex=44；Position 1..5
```

**客户端流程（冻结客户端 + 手动点菜单）**：

```powershell
# 登录并留在世界（不走路、不说话、不小退）
powershell -NoProfile -ExecutionPolicy Bypass -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/client/mir_flow.ps1 `
  -Account winplay0 -Password winplay0 -RunDir D:\MirClient-run `
  -OutDir E:/tmp/window/20261021-r3/<side>/flow-dice `
  -SkipTalk -WalkClicks 0 -SkipLogout -SkipReenter -LeaveClientRunning
#   期望：STAGE … ingame …；FLOW_OK；客户端进程仍在

# 截图定位 〖庄〗（角色在 (8,11)，NPC 在北侧一格；屏幕中心约 (512,259)）
powershell -NoProfile -ExecutionPolicy Bypass -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/client/mir_click.ps1 `
  -Shot E:/tmp/window/20261021-r3/<side>/shots/dice-01.png
# 点 NPC（北格近似 (512,263)；若没开对话，按 48x32 步长在 (512,259) 周围扫 3x3）
powershell -NoProfile -ExecutionPolicy Bypass -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/client/mir_click.ps1 `
  -LeftClick "512,263"
# 依次点菜单项（坐标以当帧截图为准）：
#   开-始-下-注  →  押5个筹码包  →  小
# 每次点击后截图：dice-02.png / dice-03.png / dice-04.png（骰子动画）
```

**判据（导出后）**：

```powershell
python -I -B E:/tmp/window/20261021-r3/judge/dice.py `
  E:/tmp/window/20261021-r3/<side>/s2c/capture.jsonl
```

期望（每侧）：

```text
MERCHANT_ACTOR <id>                     # 从 B013 (8,10) 的 SM_TURN(10) recog 取
SM_PLAYDICE count=1 recog=<merchant> param=1 tag=0 series=0 body_sha256=<hash>
SAY "正在投骰子…………"                   # 客户端截图也应出现
```

**跨侧比较**：C# 与 Rust 的 `SM_PLAYDICE` 的 `body_sha256` 必须**相同**（`recog` 是会话相关，
归一化后比）；`param/tag/series` 相同。客户端截图必须看到骰子动画。

**RED 条件**：没有 `8001`；`recog` 不是该 NPC 的 ActorId；`param≠1`；`body_sha256` 两侧不同；
没有骰子动画截图；`take 筹码包 5` 没有发生（可用 `SM_DELITEM(202)` 或背包行数变化核对）。

### 2.4 固定 NPC 菜单（T-137，tier=R；点比奇国王 / 老人 / type 2 负例）

**部署件实读**：

- `Envir/Npcs.txt`（GBK，三行）：
  `比奇国王 1 0122 29 32 0 8`；`沙巴克城堡管理 2 0150 7 16 0 8`；`老人 0 3 669 338 0 1`。
- 脚本名 = `Npc_def/<ChrName>-<MapName>`：
  `Npc_def/比奇国王-0122.txt`、`Npc_def/老人-3.txt`、`Npc_def/沙巴克城堡管理-0150.txt`。
- 说词尾 = `ChrName + '/' + sMsg`（`ScriptEngine.cs:991`；`npc.rs` 同口径）。

**场景 A：比奇国王（type 1 GuildOfficial）**

```powershell
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/loginfixture/prepare_safe_login.py `
  --db mir2_test_db --char winplay0 --map 0122 --x 29 --y 33 --level 30 --hp 500 --max-hp 500 --yes-apply
# 登录（-LeaveClientRunning），截图，点北侧 (512,263)
```

期望：**恰一条** `SM_MERCHANTSAY(643)`：
`recog = 比奇国王 ActorId`，尾以 **`比奇国王/我是比齐皇宫管理人`** 开头（脚本正文写「比齐」），
且包含 `@@buildguildnow`、`@guildwar`、`@buildguildexp`、`@guildwarexp`、`@requestcastlewarA`
（无「祈福项链」⇒ `#elsesay` 分支）。截图应显示菜单。

**场景 B：老人（type 0 Merchant）**

```powershell
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/loginfixture/prepare_safe_login.py `
  --db mir2_test_db --char winplay0 --map 3 --x 669 --y 339 --level 30 --hp 500 --max-hp 500 --yes-apply
```

期望：恰一条 `SM_MERCHANTSAY(643)`，尾以 **`老人/`** 开头，包含 `@aboutwar`
（`[查询最近攻城时间]`）。点 `@aboutwar` 应再产一条说词（内容含 `<$CASTLEWARDATE>` 展开/空值，
只要不是「零帧」即可）。

**场景 C：type 2 负例（`沙巴克城堡管理`，map 0150，`(7,16)`）**

期望：点击 / 选择 `@main` / `@ask` **零帧**、`GAMEGOLD` 不动、不进 `base.UserSelect` 回跳账本
（`t137_type2_castle_official_has_zero_frames_on_click_and_select` 的部署等价）。这是
`8b8cb10d7` 的收口点。

**判据**：同一顺序下 C# 与 Rust 的 `SM_MERCHANTSAY` 的 `body_sha256` 必须相同
（`recog` 归一化后），且客户端截图菜单文本一致。

**RED 条件**：零说词；尾不以 `NPC 名/` 开头；缺 `@@buildguildnow`/`@aboutwar`；type 2 出了说词；
两侧 `body_sha256` 不同。

### 2.5 商人不可攻击（`openmir2-merchant-race-bias`，tier=R）——**当前 BLOCKED**

**前置实测**：该修复**未合入 master**。本文件写作时：

```bash
cd /e/Users/gxh/Documents/GitHub/OpenMir2
git rev-parse merchant-race          # 8b8cb10d7（与 master 同点）
git -C /e/tmp/wt-mrace status --porcelain
#  M mir2-rs/crates/world/src/entity.rs
#  M mir2-rs/crates/world/src/merchant.rs
```

`/e/tmp/wt-mrace` 的 diff 把商人 `e.race` 从 `50` 改为 `ACTOR_RACE_NPC(10)`，新增外观轴
`e.race_img = 50`。**只有该 diff 合入 + 按 §3.2 重建后，本项才可开窗**。

**fail-closed 门**（合入后、开窗前逐条）：

```bash
cd E:/tmp/m2svr-window-origin
grep -n "race_img" mir2-rs/crates/world/src/entity.rs
grep -n "e.race = crate::entity::ACTOR_RACE_NPC" mir2-rs/crates/world/src/merchant.rs
git log -1 --format=%H            # 必须包含 merge 提交
python -I -B "$CHK/check_rust_window_preflight.py" --repo E:/tmp/m2svr-window-origin --ref HEAD \
  --baseline-ref refs/remotes/walgit/master --target-dir E:/tmp/m2svr-window-target \
  --provenance E:/tmp/m2svr-window-target/window-provenance.json
```

任一条不满足 ⇒ **BLOCKED**，不得用「商人打不死（SuperMan）」冒充「不可选中/不可攻击」。

**合入后的判据**：目标 = `B013 (8,10)` 的 `〖庄〗`（Market_Def 商人，不是固定 NPC）。
- 用冻结客户端的 **Shift+左键**强制攻击（`ClMain.pas:5582/5937`：`ssShift` 存在时对 NPC 也发
  `CM_HIT`）。`mir_click.ps1` 不带按键状态，需在点击前后注入 Shift：

```powershell
Add-Type @"
using System; using System.Runtime.InteropServices;
public class K { [DllImport("user32.dll")] public static extern void keybd_event(byte b, byte s, uint f, IntPtr e); }
"@
[K]::keybd_event(0x10,0,0,[IntPtr]::Zero)          # Shift down
powershell -NoProfile -ExecutionPolicy Bypass -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/client/mir_click.ps1 -LeftClick "512,263"
[K]::keybd_event(0x10,0,2,[IntPtr]::Zero)          # Shift up
# 重复 20 次
```

- 判据：c2s 至少有 **1 条 `CM_HIT(3014)`**（证明尝试成立）；s2c **0 条** `SM_HIT(14)` /
  `SM_STRUCK(31)` 的 `recog = 商人 ActorId`；商人 HP 不因攻击变化（SuperMan 本身也不掉血，
  所以**不能只用 HP 判**）。
- C# 侧同场景：C# oracle 的 `Merchant` 继承 `NormNpc`，`race=10`，本来就不该被攻击 ⇒
  C# 应天然 0 条伤害帧。**以 C# 实测为准**，不要用 Rust 修复前的行为当基线。

**RED/BLOCKED 条件**：拿不到 `CM_HIT`（客户端本地就不让打）⇒ 记 BLOCKED，不得记 GREEN；
有 `SM_HIT/STRUCK` 指向商人 ⇒ RED。

### 2.6 SuperMan 城门/守卫伤害路径（卡 `openmir2-superman-normnpc`，T-31，tier=R）

**oracle 事实（`14d4ec35d`/`3bdc9c09a` 已建模）**：

- `NormNpc()` ctor `SuperMan=true`（`NormNpc.cs:41`）⇒ 守卫 11/12/20/112、城门 110、城墙 111、
  固定 NPC、商人全部 `SuperMan`；
- `BaseObject.Die()` 首行 `if (SuperMan) return;`（`BaseObject.Base.cs:318-322`）；
- **伤害路径**：`BaseObject.Run` 的 `:32-36` 先回满，`:70-73` 的 `HP==0 ⇒ Die()` 不可达 ⇒
  **不调用 Die、不执行 `CastleDoor.Die` 子类段、不发 12 项 `SetMapXyFlag(2)`**；
- **直接 Die**（`Command::Kill` = GM `KillCommand.cs:40` / 脚本直接 Die）：`base.Die` 早退
  ⇒ `Death` 不置位、不改 HP，但 `CastleDoor.Die` 子类段仍产生**恰 12 项**掩码
  （3 门板 + 9 周边；`door_die_flag2_events`），且**每次**直接 Die 都重发（无永久闩锁）。

**可达性实测（本文件写作时）**：

- 部署件 `E:/MirServer/M2GameSvr/Castle/Castle/SabukW.txt`（`CastleDir=Castle`，两级同名目录）里
  `MainDoorName=MainDoor` / `LeftWallName=LeftWall` …；`mir2_data.monsters` 里**没有**这些英文名
  ⇒ 启动 **0 个城堡实体**、4 条告警（`design-castle-entities-20261020.md` §1.2 情景 A）。
- `Castle/0/SabukW.txt` 里的中文名（`沙巴克城门`/`沙巴克左城墙`…）在怪物库存在（race 110/111，
  HP 10000/5000），但 `Castle/List.txt` 只 append、不再 `Initialize` ⇒ `Castle/0` 的实体
  **永远不会在启动时刷出**。
- ⇒ **stock 部署没有可打的门/墙**。本轮能直接做的是**守卫**（`GuardList.txt` 的 `(281,661)`，
  map 0）的伤害路径；门/墙的 12 项掩码必须用**影子内容夹具**（E2，见下）才可达。

**场景 A：守卫伤害路径（stock 可达）**

```powershell
# 摆到 GuardList 守卫 (281,661) 附近；Level 30、HP 500
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/loginfixture/prepare_safe_login.py `
  --db mir2_test_db --char winplay0 --map 0 --x 281 --y 667 --level 30 --hp 500 --max-hp 500 --yes-apply
# 登录后对守卫方向攻击（-AttackClicks 20，必要时 60）
powershell -NoProfile -ExecutionPolicy Bypass -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/client/mir_flow.ps1 `
  -Account winplay0 -Password winplay0 -RunDir D:\MirClient-run `
  -OutDir E:/tmp/window/20261021-r3/<side>/flow-superman `
  -SkipTalk -WalkClicks 0 -SkipLogout -SkipReenter -AttackClicks 20
```

判据（导出后，附录 B.4 片段）：

```text
GUARD_ACTOR <id>                      # SM_TURN(10) param=281 tag=661 的 recog
SM_DEATH(32) for guard = 0            # 关键：伤害路径不得死
SM_HIT/STRUCK for guard >= 1          # 证明打到了
```

- C# 侧同场景：`SM_DEATH(32)` 对守卫也应为 0（oracle `SuperMan`）。
- **HP 回满不可直接观测**（状态 dump `state_log.rs` 只落玩家族；见模块头）。判据只到
  「打到 + 不死 + 守卫仍在（后续 `SM_STRUCK` 继续）」；**不得**把「没死」单独写成
  「回满已验证」。回满的构造级牙是
  `superman_guard_lethal_damage_refills_without_die`（`world.rs:10163`），真机窗口只判可见帧面。

**场景 B：门/墙 12 项掩码（stock 不可达；E2 夹具，当前 BLOCKED）**

- **前置**：E2（影子内容）**当前没有 owner 签名**（第二轮 runbook §4.2：仓库内最后书面记录
  `b46805058` 与 run sheet §E-0 都是「待裁决」）。**拿不到签名 ⇒ 本项 BLOCKED，不得开窗**。
- 授权后的夹具（只改影子副本，`E:/MirServer/**` 一字不动；C#/Rust 两侧同一份）：
  1. 在影子包 `Castle/0/` 下再建一级 `0/`，把 `Castle/0/SabukW.txt` 复制成
     `Castle/0/0/SabukW.txt`（因为路径 = `<BasePath>/<CastleDir>/<CastleDir>/SabukW.txt`，
     两级同名；`castle.rs:95-117` 已登记）；
  2. 影子 `Server.conf` 的 `CastleDir=Castle` 改为 `CastleDir=0`；
  3. 为了能一击致死，把 `Castle/0/0/SabukW.txt` 的 `MainDoorHP=1`（记录改前/改后 sha256）；
  4. 两侧都用这份影子内容起栈；玩家摆到 map 3（盟重省）`(671,330)`，门在 `(672,330)`。
- **伤害路径判据**：一击把门打到 0 后，下一 tick 应回满；`SM_DEATH(32)` 对门 = 0；
  **没有** 12 项掩码（走到 `(672,330)` 仍被挡：服务端日志应出现「被挡/越界 ⇒ 不动」）。
- **直接 Die 判据**：需要一个**直接 Die 入口**（GM `KillCommand` / 脚本 `Die`）作用到门。
  若当前宿主没有客户端可达的直接 Die 入口 ⇒ 本子项 **BLOCKED**，只能用构造级
  `superman_castle_door_direct_kill_emits_flag2_mask_without_death`（`world.rs:11187`）当证据，
  **不得**把构造级当窗口级。有入口时：门 `Death=false`、无 `SM_DEATH`、12 项掩码应用
  （3 门板 + 9 周边变为可走），且第二次直接 Die 再发 12 项。

---

## 3. 配方（可直接照抄）

### 3.1 开窗前置门（按顺序，任一不过即停）

```powershell
$WIN='E:/tmp/window/20261021-r3'
$CHK='E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013'
$PKG='E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/m3/window-20261023'
New-Item -ItemType Directory -Force $WIN,"$WIN/judge","$WIN/db" | Out-Null

# F1 端口：九游戏端口全 DOWN、7600 DOWN、3306 UP
powershell -NoProfile -ExecutionPolicy Bypass -File E:/MirServer/port-status.ps1
#   期望：缺失端口 = 5000,5100,5500,5600,5700,6000,7000,7100,7200；mysqld 在 3306
[System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners() |
  Where-Object { $_.Port -in 7200,7600 } | Format-Table Address,Port
#   期望：无输出

# F2 镜像：干净、HEAD == walgit/master
git -C E:/tmp/m2svr-window-origin status --porcelain          # 期望空
git -C E:/tmp/m2svr-window-origin rev-parse HEAD
git -C E:/tmp/m2svr-window-origin rev-parse refs/remotes/walgit/master
#   两者必须相等

# F3 provenance：source_head == 镜像 HEAD，六件 sha256 一致
python -I -B "$CHK/check_rust_window_preflight.py" `
  --repo E:/tmp/m2svr-window-origin --ref HEAD `
  --baseline-ref refs/remotes/walgit/master `
  --mirserver E:/MirServer --target-dir E:/tmp/m2svr-window-target `
  --provenance E:/tmp/m2svr-window-target/window-provenance.json `
  --slot-log-dir "$WIN/rust/stack"
#   期望：R1..R8 全 OK（R8 在外部栈停掉后才绿）

# F4 测试库：见 §3.3；F5 客户端：见 §1.5；F6 merchant / F7 E2：见 §2.5/§2.6
```

### 3.2 镜像更新到 master + 重建六件 + 重录 provenance

```powershell
cd E:/tmp/m2svr-window-origin
git fetch walgit master
git checkout --detach refs/remotes/walgit/master
git status --porcelain
git rev-parse HEAD
#   期望：HEAD == $(git rev-parse refs/remotes/walgit/master)（本文件写作时 = 8b8cb10d7…）

# 编译六件（debug；--offline；顺序与 provenance_record_command() 一致）
$env:CARGO_TARGET_DIR = 'E:/tmp/m2svr-window-target'
cd E:/tmp/m2svr-window-origin/mir2-rs
cargo build --offline -p game-svr -p game-gate -p login-srv -p login-gate -p sel-gate -p db-srv
#   期望：Finished `dev` profile；六件 exe 落在 E:/tmp/m2svr-window-target/debug/

# 只有编译成功才重录出处清单（失败不要录，否则旧件被洗成新源）
python -I -B "E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_rust_window_preflight.py" `
  --repo "E:/tmp/m2svr-window-origin" --ref HEAD `
  --target-dir "E:/tmp/m2svr-window-target" `
  --record-provenance "E:/tmp/m2svr-window-target/window-provenance.json"
#   期望：provenance 写入；source_head == 新 HEAD；六件 sha256/bytes

# 复验（R6/R7 必须绿）
python -I -B "$CHK/check_rust_window_preflight.py" `
  --repo E:/tmp/m2svr-window-origin --ref HEAD `
  --baseline-ref refs/remotes/walgit/master --mirserver E:/MirServer `
  --target-dir E:/tmp/m2svr-window-target `
  --provenance E:/tmp/m2svr-window-target/window-provenance.json `
  --slot-log-dir "$WIN/rust/stack"
```

> **注意**：镜像里 `master` 已被主检出占用，`git checkout master` 会报
> `fatal: 'master' is already checked out`；必须 `--detach`。**不要**在主检出里构建
> （guard 固定构建源 = 镜像；主检出构建会让 R6/R7 判的不是同一棵树）。

### 3.3 测试库：从生产克隆 / 重置 + 每趟 base 快照

```powershell
$bin='D:\mysql\mariadb-10.11.19-winx64\bin'
$base="$WIN/db/clone"
New-Item -ItemType Directory -Force $base | Out-Null

function Dump-Db([string]$db) {
  $sql = Join-Path $base "$db.sql"
  $p = Start-Process -FilePath "$bin\mysqldump.exe" `
    -ArgumentList '-h127.0.0.1','-uroot','--single-transaction','--routines','--triggers','--hex-blob',$db `
    -RedirectStandardOutput $sql -NoNewWindow -Wait -PassThru
  if ($p.ExitCode -ne 0) { throw "RED: mysqldump $db rc=$($p.ExitCode)" }
  $sql
}
function Reset-TestDb([string]$srcDb,[string]$testDb) {
  $sql = Dump-Db $srcDb
  & "$bin\mysql.exe" -h127.0.0.1 -uroot -e `
    "DROP DATABASE IF EXISTS $testDb; CREATE DATABASE $testDb DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci;"
  if ($LASTEXITCODE -ne 0) { throw "RED: create $testDb rc=$LASTEXITCODE" }
  $p = Start-Process -FilePath "$bin\mysql.exe" -ArgumentList '-h127.0.0.1','-uroot',$testDb `
    -RedirectStandardInput $sql -NoNewWindow -Wait -PassThru
  if ($p.ExitCode -ne 0) { throw "RED: load $testDb rc=$($p.ExitCode)" }
}
Reset-TestDb 'mir2_db'      'mir2_test_db'
Reset-TestDb 'mir2_account' 'mir2_test_account'
Reset-TestDb 'mir2_data'    'mir2_test_data'

# 断言（只读）
& "$bin\mysql.exe" -h127.0.0.1 -uroot -N -e `
  "SELECT table_schema,COUNT(*) FROM information_schema.tables WHERE table_schema IN ('mir2_test_db','mir2_test_account','mir2_test_data') GROUP BY table_schema ORDER BY table_schema;"
#   期望：mir2_test_account 13 / mir2_test_data 7 / mir2_test_db 11
& "$bin\mysql.exe" -h127.0.0.1 -uroot -N -e `
  "SELECT (SELECT COUNT(*) FROM mir2_db.characters)=(SELECT COUNT(*) FROM mir2_test_db.characters), (SELECT COUNT(*) FROM mir2_account.account)=(SELECT COUNT(*) FROM mir2_test_account.account), (SELECT COUNT(*) FROM mir2_data.stditems)=(SELECT COUNT(*) FROM mir2_test_data.stditems);"
#   期望：1  1  1

# 任何一侧起栈之前拍 base-test（13 张测试库表）
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/dbsnap/dbsnap.py --password= snap `
  --out "$WIN/db/base-test" --db test
#   期望：SNAP … 13 张表；manifest.json 落地
```

> `mysqldump <db>` **不带** `--databases`，否则会把 `USE mir2_db` 带进测试库。
> 当前测试三库是 `latin1` 默认；重置函数显式建 `utf8mb4`，与生产 schema 一致。

### 3.4 C# 影子包 + 三处连接串（只改影子，`E:/MirServer` 一字不动）

```powershell
$pack="$WIN/csharp/pack"; $repo="$WIN/csharp/repo"
New-Item -ItemType Directory -Force "$pack","$repo/src/DBSrv/bin","$repo/src/LoginSrv/bin","$repo/mir2-rs/tests/parity/m3" | Out-Null

foreach ($d in 'M2GameSvr','LoginGate','SelGate','RunGate') {
  robocopy "E:\MirServer\$d" (Join-Path $pack $d) /E /NFL /NDL /NJH /NJS /NP | Out-Null
  if ($LASTEXITCODE -gt 7) { throw "RED: robocopy $d rc=$LASTEXITCODE" }
}
robocopy 'E:\Users\gxh\Documents\GitHub\OpenMir2\src\DBSrv\bin\Release' "$repo\src\DBSrv\bin\Release" /E /NFL /NDL /NJH /NJS /NP | Out-Null
robocopy 'E:\Users\gxh\Documents\GitHub\OpenMir2\src\LoginSrv\bin\Release' "$repo\src\LoginSrv\bin\Release" /E /NFL /NDL /NJH /NJS /NP | Out-Null
robocopy 'E:\Users\gxh\Documents\GitHub\OpenMir2\mir2-rs\tests\parity\m3\rng-hook' "$repo\mir2-rs\tests\parity\m3\rng-hook" /E /XD bin obj /NFL /NDL /NJH /NJS /NP | Out-Null

# 1) GameSvr：UTF-8 BOM 保真
$p="$pack/M2GameSvr/Server.conf"
$t=[IO.File]::ReadAllText($p,[Text.Encoding]::UTF8)
$t=$t -replace '(?m)^ConnctionString=[^\r\n]*','ConnctionString=server=127.0.0.1;uid=root;pwd=;database=mir2_test_data;'
[IO.File]::WriteAllText($p,$t,(New-Object System.Text.UTF8Encoding($true)))

# 2) DBSrv：ASCII
$p="$repo/src/DBSrv/bin/Release/dbsvr.conf"
$t=[IO.File]::ReadAllText($p)
$t=$t -replace '(?m)^ConnctionString=[^\r\n]*','ConnctionString=server=127.0.0.1;uid=root;pwd=;database=mir2_test_db;'
[IO.File]::WriteAllText($p,$t,[Text.Encoding]::ASCII)

# 3) LoginSrv：ASCII
$p="$repo/src/LoginSrv/bin/Release/logsrv.conf"
$t=[IO.File]::ReadAllText($p)
$t=$t -replace '(?m)^ConnctionString=[^\r\n]*','ConnctionString=server=127.0.0.1;uid=root;pwd=;database=mir2_test_account;'
[IO.File]::WriteAllText($p,$t,[Text.Encoding]::ASCII)

# 断言：恰好三条，三库各一
Select-String -Path "$pack/M2GameSvr/Server.conf","$repo/src/DBSrv/bin/Release/dbsvr.conf","$repo/src/LoginSrv/bin/Release/logsrv.conf" -Pattern 'ConnctionString=.*database=mir2_test_' |
  ForEach-Object { "{0}:{1}:{2}" -f $_.Path,$_.LineNumber,$_.Line }
#   期望：Server.conf:3:…mir2_test_data; / dbsvr.conf:21:…mir2_test_db; / logsrv.conf:25:…mir2_test_account;
```

> `ConnctionString` 是上游拼写（少一个 `e`），不要“修正”。

### 3.5 起 C# 主栈 + 影子 7600 + 代理 7200（C# 侧）

```powershell
$shadow="$WIN/csharp/GameSvr-shadow"
if (Test-Path $shadow) {
  $bak="$shadow.bak-"+(Get-Date -Format 'yyyyMMdd-HHmmss')
  Move-Item -LiteralPath $shadow -Destination $bak -ErrorAction Stop
}
if (Test-Path "$shadow/GameSvr.exe") { throw 'RED: 旧影子仍存在' }

powershell -NoProfile -ExecutionPolicy Bypass `
  -File E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/m3/start-csharp-hook.ps1 `
  -Repo "$repo" -Pack "$pack" -Shadow "$shadow" -LogDir "$WIN/csharp/stack" -Seed 42
#   期望：SHADOW_COPY / HOOK_DEPLOYED / START … / PIDFILE / CSHARP_STACK_GREEN

# 影子身份（F1）
$dep=Get-Content "$shadow/!deployed-from.json" -Raw | ConvertFrom-Json
"git_head=$($dep.git_head)"          # 期望 8a621f6a87e7c206887d310b8315f74b4bb03aac
git -C E:/Users/gxh/Documents/GitHub/OpenMir2 merge-base --is-ancestor d7ead5b2a $dep.git_head
"merge-base rc=$LASTEXITCODE"        # 期望 0

# 停 RunGate（只按本栈 pid 文件）
$pf=Get-Content "$WIN/csharp/stack/csharp-stack.pid.json" -Raw | ConvertFrom-Json
($pf | Where-Object { $_.name -eq 'RunGate' }).pid | ForEach-Object { Stop-Process -Id $_ -Force -ErrorAction SilentlyContinue }
Start-Sleep -Milliseconds 800

# 影子 conf：cp936 往返 byte-exact，只改 Count / GatePort1
$enc=[Text.Encoding]::GetEncoding(936)
New-Item -ItemType Directory -Force "$WIN/csharp/shadow" | Out-Null
Copy-Item -LiteralPath "$pack/RunGate/config.conf" -Destination "$WIN/csharp/shadow/game-gate.conf" -Force
$t=[IO.File]::ReadAllText("$WIN/csharp/shadow/game-gate.conf",$enc)
$t=$t -replace '(?m)(?<=^Count=)\d+(?=\r?$)','1' -replace '(?m)(?<=^GatePort1=)\d+(?=\r?$)','7600'
[IO.File]::WriteAllText("$WIN/csharp/shadow/game-gate.conf",$t,$enc)

# C# GameGate 读 AppContext.BaseDirectory/config.conf ⇒ 整目录拷贝
Remove-Item -Recurse -Force "$WIN/csharp/shadow/run-gate" -ErrorAction SilentlyContinue
Copy-Item -Recurse -Force "$pack/RunGate" "$WIN/csharp/shadow/run-gate"
Copy-Item -Force "$WIN/csharp/shadow/game-gate.conf" "$WIN/csharp/shadow/run-gate/config.conf"
$g=Start-Process -FilePath "$WIN/csharp/shadow/run-gate/GameGate.exe" -WorkingDirectory "$WIN/csharp/shadow/run-gate" -WindowStyle Hidden `
  -RedirectStandardOutput "$WIN/csharp/shadow/csharp-gate.out.log" -RedirectStandardError "$WIN/csharp/shadow/csharp-gate.err.log" -PassThru
$g.Id | Set-Content "$WIN/csharp/shadow/csharp-gate.pid"
Start-Sleep -Milliseconds 1500

# 断言 7200 DOWN / 7600 UP
[System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners() |
  Where-Object { $_.Port -in 7200,7600 } | ForEach-Object { "LISTEN $($_.Port)" }
#   期望：只剩 7600

# 代理占 7200 → 7600
$proxy=Start-Process -FilePath python -ArgumentList @('-I','-B','E:/Users/gxh/Documents/GitHub/OpenMir2/tools/capture/mir2_proxy.py','--out',"$WIN/csharp/proxy",'--map','7200=127.0.0.1:7600') `
  -WorkingDirectory 'E:/Users/gxh/Documents/GitHub/OpenMir2' -WindowStyle Hidden `
  -RedirectStandardOutput "$WIN/csharp/proxy.log" -RedirectStandardError "$WIN/csharp/proxy.err.log" -PassThru
$proxy.Id | Set-Content "$WIN/csharp/proxy.pid"
Start-Sleep -Seconds 1
Get-Content "$WIN/csharp/proxy.log"   # 期望 LISTEN 127.0.0.1:7200 -> 127.0.0.1:7600 / READY
```

### 3.6 起 Rust 混合栈（`-DbName mir2_test`）+ 影子 7600 + 代理 7200

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/m2-acceptance/start_rust_stack.ps1 `
  -RustBinDir E:/tmp/m2svr-window-target/debug -Seed 42 `
  -LogDir "$WIN/rust/stack" -DbName mir2_test -DbHost 127.0.0.1
#   期望：DB_TARGET DBSrv database=mir2_test_db / LoginSrv=mir2_test_account / game-svr=mir2_test_data
#         DB_TARGET_SHADOW …；RUST_SEED_INJECTED=42；RUST_STACK_OK …

# 断言影子配置三处
Select-String -Path "$WIN/rust/stack/db-target/DBSrv/dbsvr.conf","$WIN/rust/stack/db-target/LoginSrv/logsrv.conf","$WIN/rust/stack/db-target/M2GameSvr/Server.conf" -Pattern 'database=mir2_test_' |
  ForEach-Object { "{0}:{1}:{2}" -f $_.Path,$_.LineNumber,$_.Line }

# 停 game-gate（它占 7200/7201/7202）
$pf=Get-Content "$WIN/rust/stack/rust-stack.pid.json" -Raw | ConvertFrom-Json
($pf | Where-Object { $_.name -eq 'game-gate' }).pid | ForEach-Object { Stop-Process -Id $_ -Force -ErrorAction SilentlyContinue }
Start-Sleep -Milliseconds 800

# 影子 conf：同 §3.5 的 cp936 往返，Count=1 / GatePort1=7600
$enc=[Text.Encoding]::GetEncoding(936)
New-Item -ItemType Directory -Force "$WIN/rust/shadow" | Out-Null
Copy-Item -LiteralPath 'E:\MirServer\RunGate\config.conf' -Destination "$WIN/rust/shadow/game-gate.conf" -Force
$t=[IO.File]::ReadAllText("$WIN/rust/shadow/game-gate.conf",$enc)
$t=$t -replace '(?m)(?<=^Count=)\d+(?=\r?$)','1' -replace '(?m)(?<=^GatePort1=)\d+(?=\r?$)','7600'
[IO.File]::WriteAllText("$WIN/rust/shadow/game-gate.conf",$t,$enc)

$g=Start-Process -FilePath E:/tmp/m2svr-window-target/debug/game-gate.exe -ArgumentList "$WIN/rust/shadow/game-gate.conf" `
  -WorkingDirectory "$WIN/rust/shadow" -WindowStyle Hidden `
  -RedirectStandardOutput "$WIN/rust/shadow/rust-gate.out.log" -RedirectStandardError "$WIN/rust/shadow/rust-gate.err.log" -PassThru
$g.Id | Set-Content "$WIN/rust/shadow/rust-gate.pid"
Start-Sleep -Milliseconds 1500

[System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners() |
  Where-Object { $_.Port -in 7200,7600 } | ForEach-Object { "LISTEN $($_.Port)" }
#   期望：只剩 7600

$proxy=Start-Process -FilePath python -ArgumentList @('-I','-B','E:/Users/gxh/Documents/GitHub/OpenMir2/tools/capture/mir2_proxy.py','--out',"$WIN/rust/proxy",'--map','7200=127.0.0.1:7600') `
  -WorkingDirectory 'E:/Users/gxh/Documents/GitHub/OpenMir2' -WindowStyle Hidden `
  -RedirectStandardOutput "$WIN/rust/proxy.log" -RedirectStandardError "$WIN/rust/proxy.err.log" -PassThru
$proxy.Id | Set-Content "$WIN/rust/proxy.pid"
```

> 混合栈 = **C# DBSrv/LoginSrv** + **Rust game-svr/三网关**（Rust `bins/db-srv` 还没有 6000 DataService
> 面；见 `start_rust_stack.ps1` 头注）。这是既有口径，不是本轮新增。

### 3.7 受控客户端流程

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/client/mir_flow.ps1 `
  -Account winplay0 -Password winplay0 -RunDir D:\MirClient-run `
  -OutDir "$WIN/<side>/flow" `
  -SkipTalk -WalkClicks 0 -SkipLogout -SkipReenter
#   期望：STAGE start / login_id / login_pwd / login_submit / charsel / enter_game
#         STAGE notice_poll / ingame / talk_sent / skip_logout / done
#         CLIENT_STOPPED / FLOW_OK
```

- G1：`-SkipTalk -WalkClicks 0 -SkipLogout -SkipReenter`（不走路、不说话、不打怪）。
- 要在进图后过坐标门/点 NPC，必须给证据源：live 用 `-ProxyDir <proxy --out>`（运行中读
  `chunks.ndjson`+`conn-*.bin`），完整命令链见 §9.3；只跑 §3.7 基线流程可以不给。
- **注意（F3 行为变更）**：没有证据源时，公告点击前必须有客户端侧信号；三者都没有就
  **不点公告**（只停鼠标、`NOTICE_POLL UNVERIFIED`），服务端 10s 后会 `BoEmergencyClose`。
  所以真实长流程请给 `-ProxyDir`（最可靠）或标定好的 `-NoticePixelProbe`/`-NoticeProbe`。
- G3A attack：加 `-AttackClicks 20`（默认攻击环；必要时 `-AttackRing` 指向目标）。
- 骰子/NPC/商人/SuperMan：`-LeaveClientRunning` + `mir_click.ps1`（§2.3–§2.6）。

### 3.8 导出 + 判据（**必须先导出，再判**）

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/m2-acceptance/export_s2c.ps1 `
  -ProxyDir "$WIN/<side>/proxy" -OutDir "$WIN/<side>/s2c"
#   期望：EXPORTED …\capture.jsonl；GREEN: … s2c 帧已导出 且 replay 自洽

# 导出件形状自检（s2c 行必须 # 开头 / ! 结尾）
$rows=Get-Content "$WIN/<side>/s2c/capture.jsonl" | ForEach-Object { $_ | ConvertFrom-Json }
$s=@($rows | Where-Object { $_.dir -eq 's2c' -and $_.port -eq 7200 })
if ($s.Count -eq 0) { throw 'PREREQ_MISSING: 没有 dir=s2c/port=7200 的帧' }
$bad=@($s | Where-Object { -not ($_.raw_hex.StartsWith('23') -and $_.raw_hex.EndsWith('21')) })
if ($bad.Count -ne 0) { throw "RED: $($bad.Count) 帧 raw_hex 不以 23 开头 / 21 结尾" }
"CAPTURE_OK s2c=$($s.Count) act=$(@($s | Where-Object { $_.frame_form -eq 'act' }).Count)"
```

- `-SkipReplay` 只在窗口不允许 cargo 时用；用了必须在证据里写「replay 未跑」，**不得**说成自洽。
- `frame_form=act`（`#+GD/<tick>!` 明文动作帧）**没有 `ident`**；所有判据都要先过滤
  `frame_form != 'act'`。

### 3.9 三个自定义场景的摆位与点击（骰子/NPC/商人/SuperMan）

| 场景 | 摆位（`prepare_safe_login.py`） | 点击 |
|---|---|---|
| 骰子 `〖庄〗` | `--map B013 --x 8 --y 11` | 北侧 `(512,263)`；菜单按截图点 |
| 比奇国王 | `--map 0122 --x 29 --y 33` | 北侧 `(512,263)` |
| 老人 | `--map 3 --x 669 --y 339` | 北侧 `(512,263)` |
| type 2 负例 | `--map 0150 --x 7 --y 17` | 北侧 `(512,263)` |
| 守卫 SuperMan | `--map 0 --x 281 --y 667` | 北侧；`-AttackClicks 20` |
| 商人攻击（条件） | `--map B013 --x 8 --y 11` | Shift+左键 `(512,263)`（§2.5） |

点击坐标是**客户端客户区坐标**。按冻结客户端 `CXYfromMouseXY`
（`PlayScn.pas:2951-2958`；`ShiftYOffset≈304`、`UNITY=32`）的逆映射：

- `(512,263)` ⇒ `round((263-304)/32) = -1` ⇒ **北侧一格**（角色 `(29,33)` 时即国王 `(29,32)`）；
- `(512,227)` ⇒ `round((227-304)/32) = -2` ⇒ **北侧两格**（y=31），是空地 —— 点下去只会 `CM_WALK`。

本文件旧版把 `(512,227)` 记作「北侧一格」，是换算错误（2026-10-22 诊断
`docs/npc-render-diagnosis-20261022.md` §2.4 订正）。上表已统一改为 `(512,263)`。
若未命中（对话没开/没出帧），用 `mir_click.ps1 -Shot` 截图后在 NPC 精灵上点，
或按 48x32 在中心周围扫 3x3（每次点击后等 600ms，看 capture 是否出现 `CM_CLICKNPC(3012)`）。

---

## 4. 回滚与收尾

### 4.1 停栈（按 pid 文件，绝不按进程名误杀）

```powershell
# 1) 客户端
Get-Process MirClinet -ErrorAction SilentlyContinue | Stop-Process -Force

# 2) 代理
if (Test-Path "$WIN/<side>/proxy.pid") { Get-Content "$WIN/<side>/proxy.pid" | ForEach-Object { Stop-Process -Id $_ -Force -ErrorAction SilentlyContinue } }

# 3) 影子 gate
if (Test-Path "$WIN/csharp/shadow/csharp-gate.pid") { Get-Content "$WIN/csharp/shadow/csharp-gate.pid" | ForEach-Object { Stop-Process -Id $_ -Force -ErrorAction SilentlyContinue } }
if (Test-Path "$WIN/rust/shadow/rust-gate.pid") { Get-Content "$WIN/rust/shadow/rust-gate.pid" | ForEach-Object { Stop-Process -Id $_ -Force -ErrorAction SilentlyContinue } }

# 4) 主栈（只停本脚本起的；不要碰外部 C# 栈）
powershell -NoProfile -ExecutionPolicy Bypass -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/m2-acceptance/start_rust_stack.ps1 -LogDir "$WIN/rust/stack" -Stop
powershell -NoProfile -ExecutionPolicy Bypass -File E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/m3/start-csharp-hook.ps1 -LogDir "$WIN/csharp/stack" -Stop
```

**收窗复核**：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File E:/MirServer/port-status.ps1
#   期望：九游戏端口 DOWN；仅 mysqld 3306 UP（若 3306 原本就 UP，不得由 -Stop 杀掉）
[System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners() |
  Where-Object { $_.Port -in 5000,5100,5500,5600,5700,6000,7000,7100,7200,7600 } | Format-Table Address,Port
#   期望：无输出
Get-Process MirClinet,mir2_proxy,game-gate,GameGate,GameSvr,game-svr,DBSrv,LoginSrv,LoginGate,SelGate -ErrorAction SilentlyContinue
#   期望：无输出
```

> **永远禁止 `E:/MirServer/start-all.ps1 -Stop`**（Force 且连带杀 `mysqld`）。
> `start_rust_stack.ps1 -Stop` 只适用于它自己起过的栈；外部 C# 全栈只能由起栈会话/owner 停。

### 4.2 还原测试库

```powershell
# 每侧之间 / 收窗时：先 dry-run，再真正灌
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/dbsnap/dbsnap.py --password= restore `
  --dir "$WIN/db/base-test" --db test
#   期望：RESTORE PLAN … + DRY-RUN: 未执行任何写库
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/dbsnap/dbsnap.py --password= restore `
  --dir "$WIN/db/base-test" --db test --yes-apply
```

- 若本轮改了背包（骰子场景的 5 个 筹码包）或角色等级/位置，restore 会一并回滚。
- 测试库保留（`mir2_test_*`），不删除；生产库一字不动。

### 4.3 临时物清单（收窗必须登记）

| 路径 | 处理 |
|---|---|
| `$WIN/csharp/{pack,repo,GameSvr-shadow,stack,shadow,proxy,flow,s2c}` | 证据/工作件；保留到收卡，登记后清理 |
| `$WIN/rust/{stack,shadow,proxy,flow,s2c}` | 同上 |
| `$WIN/judge/*.json`、`$WIN/judge/*.txt` | **证据**；保留 |
| `$WIN/db/{clone,base-test}` | 克隆件；可删（可重建） |
| `$WIN/db/clone/*.sql` | 生产库 dump；**含数据，收窗删除或登记保留** |
| `E:/tmp/wt-wr3` | 本 runbook 的 worktree；合并后可删 |
| `E:/tmp/m2svr-window-target` | 构建产物；保留（下一窗复用） |

- 禁止在 `E:/tmp` 留本线专属残留；`E:/MirServer/**` 一字不动。
- 冻结基线：`git -C E:/Users/gxh/Documents/GitHub/OpenMir2 status --short` 必须干净
  （本线只写本 worktree 的 runbook）。

### 4.4 证据落盘与 hash

```powershell
$files = Get-ChildItem "$WIN" -Recurse -File | Where-Object { $_.FullName -match '\\(judge|s2c|flow)\\' }
$files | ForEach-Object { $h=(Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower(); "{0}  {1}" -f $h,$_.FullName } |
  Set-Content "$WIN/evidence-sha256.txt"
Get-Content "$WIN/evidence-sha256.txt" | Measure-Object -Line
```

证据四件套（命令 / 输出 / 路径 / hash）写进最终报告；`capture.jsonl`、`judge/*.json`、关键截图
都必须有 sha256。

---

## 5. Fail-closed 门清单（任一命中即停，不判）

| # | 门 | 判定 | 处置 |
|---|---|---|---|
| F1 | 九游戏端口任一 LISTENING / 7600 LISTENING | RED | 外部 C# 栈由起栈会话/owner 停；禁止 `start-all.ps1 -Stop`；停不掉 ⇒ 不开窗 |
| F2 | 3306 DOWN 或测试库缺失 | PREREQ | 3306 DOWN ⇒ 不开窗（E3 未裁决，不得自行停/起 MySQL）；测试库缺失 ⇒ 按 §3.3 克隆 |
| F3 | 镜像 `status` 非空 / HEAD ≠ `walgit/master` | RED | 按 §3.2 `--detach` 到基线；不干净先查清 |
| F4 | provenance `source_head` ≠ 镜像 HEAD / 六件 sha256 不符 | RED | 重编译 + `--record-provenance`（§3.2） |
| F5 | 测试库表数/行数与生产不等 | RED | 重克隆（§3.3）；不得只凭 dbsnap 备份就写生产 |
| F6 | 客户端 exe 缺失 / 无 `Map/{0122,3,b013}.map` | PREREQ | 停；不要用别的客户端 |
| F7 | `openmir2-merchant-race-bias` 未合入构建源 | BLOCKED | §2.5 记 BLOCKED；不得用 SuperMan 冒充 |
| F8 | E2 影子内容无 owner 签名 | BLOCKED | §2.6-B 记 BLOCKED；不得改 `E:/MirServer/**` |
| F9 | 判据喂的是 proxy 临时 dump / `raw_hex` 非 `23…21` | PREREQ/RED | 先 `export_s2c.ps1`；不得用临时 dump 判 |
| F10 | C# 侧 G1 自己不是 GREEN | RED | 先修 C# 侧/摆位；不要判 Rust |
| F11 | 出现 `SM_NOWDEATH(34)` / 角色 HP=0 / `SM_STRUCK` 自击 | 中止 | G1 受控场景被污染；restore + 重钉后再来 |
| F12 | `judge_login` J5 = `KNOWN_PENDING` | RED | 不是 GREEN；按 §3.3 同起点重跑 |

---

## 6. 已知缺口 / 未验证（必须随报告登记）

1. **`preflight.py` P3 恒 `PREREQ_RED`**（§1.6 实测）：`collect_db_state()` 只查生产行数，
   `check_test_db()` 却取测试行数。修好前用 §1.3/§3.3 的手工 SQL 当门；建议另开 L 卡修工具
   （本文件不改代码）。
2. **`origin/master` 已落后**（`189932857` vs `walgit/master` `8b8cb10d7`）：
   `check_rust_window_preflight.py` 的默认 `--baseline-ref origin/master` 会误判；本轮必须显式
   `refs/remotes/walgit/master`。
3. **任务文字「8 个游戏端口」与实测不符**：实测 9 个游戏端口 + 3306（§1.4）。以实测为准。
4. **`openmir2-merchant-race-bias` 未合入**：只在 `/e/tmp/wt-mrace` 工作树（未提交）；
   本项当前 **BLOCKED**，不得用「商人 SuperMan 打不死」当「不可攻击」。
5. **城堡 stock 0 实体**：`Castle/Castle/SabukW.txt` 的门/墙英文名不在怪物库；`Castle/0` 只在
   `Castle/List.txt` 里 append、不 Initialize ⇒ 真机没有可打的门/墙。12 项掩码需要
   **E2 影子夹具 + 直接 Die 入口**；E2 当前无 owner 签名，本项 **BLOCKED**。
6. **守卫 HP 回满不可直接观测**：`state_log.rs` 只落玩家族；窗口只能判「打到 + 不死 + 仍在」。
   回满已有构造级用例（`world.rs:10163`），不得把构造级当窗口级。
7. **没有专用的 `SM_WALK` 判据工具**：本文件附录 B.1 提供只读片段；它不是仓库工具，
   收卡时应把片段输出（`smwalk-*.txt`）作为证据，并在 review 里写明「片段口径」。
8. **骰子/NPC/商人/SuperMan 不在 `scenarios.json`**（只有 G1/G2a/G3a/G5a）：
   这些场景用 `mir_flow -LeaveClientRunning` + `mir_click.ps1` 手动点击；点击坐标依赖截图
   （屏幕中心约 `(512,259)`，48x32 步长是近似），不是完全确定性的自动化。
9. **`scenarios.json` 的 `baseline` 是 `23748419d`**，落后当前 master；`judge_login.py` 不读它，
   但开窗当天应把期望序列与基线重新核对。
10. **测试库 schema 默认 `latin1`**（生产 `utf8mb4`）：重置时按 §3.3 用 utf8mb4 重建；
    不重建就在 latin1 上跑，中文当前没坏但属于已知偏差。
11. **`MirClient` 源仓未提交文件=7**：不是本线改的；本窗只用冻结 exe，不得在客户端仓写文件。
12. **C# 部署件 head = `8a621f6a`**（`!deployed-from.json`，2026-10-20 00:19）；E4 允许重编，
    但本轮仍用部署件（与第二轮一致）。若重编，必须重跑 `!deployed-from.json` 身份断言。
13. **`SM_DEATH(32)` 与 `SM_NOWDEATH(34)` 不是同一帧**：怪物/NPC/门用 `SM_DEATH(32)`；
    玩家自己死亡用 `SM_NOWDEATH(34)`（第二轮 Rust 角色被打死时出现的是 34）。判 SuperMan 时
    用 32，别把 34 当门/守卫的死亡。
14. **`capture.jsonl` 同时含 c2s 与 `frame_form=act` 行**：所有判据先筛
    `dir=s2c/port=7200/frame_form!='act'/ident 存在`，否则 `KeyError`（第二轮已踩）。
15. **本轮未验证**（没有真机跑）：所有命令都是按工具 `--help`/源码/第二轮实跑反推；
    开窗当天必须按 §3.1 逐条重跑，任何一条与本文读数不符以现场为准。

---

## 7. 单二进制用法（`mir2-server` 统一启停）

> **tier=L**：本节只讲运维配方与工具用法，不改任何角色行为、不改协议、不改配置格式。
> 工具本体 = `tools/m2-acceptance/{start_mir2_server.ps1,stop_mir2_server.ps1}`（纯 PowerShell 编排）。
> 旧六个 exe 仍然可用，行为不变；本工具不依赖也不替换它们。

### 7.1 背景与入口

`mir2-server` 是单二进制多角色入口（`mir2-rs/bins/mir2-server`）：

```text
mir2-server <role> [role argv...]
mir2-server --list-roles        # 恰六行；退出 0
```

- 六个角色：`game-svr | db-srv | login-srv | game-gate | login-gate | sel-gate`。
- 未知/缺省角色：用法写 stderr，退出 **2**。
- 每个角色仍是**独立进程**，端口、日志前缀、配置读取、启动参数口径与旧 exe 完全一致；
  第一个参数就是该角色的配置文件路径（各 `src/lib.rs` 的 `args.first()`）。
- 旧 exe（`game-svr.exe` / `db-srv.exe` / …）只是薄包装（`main` 调各自 lib 的 `run()`），
  argv 语义相同；需要逐进程手起时继续用旧 exe，**行为不变**。

### 7.2 起栈：`start_mir2_server.ps1`

```powershell
# 只打印将执行的命令行与顺序（不启动任何进程、不建 LogDir；缺配置退出 2）
powershell -NoProfile -ExecutionPolicy Bypass `
  -File tools/m2-acceptance/start_mir2_server.ps1 -WhatIf

# 实跑（默认 -Root E:\MirServer，六个角色全起；日志/pid 落 <Root>\logs\mir2-server）
powershell -NoProfile -ExecutionPolicy Bypass `
  -File tools/m2-acceptance/start_mir2_server.ps1 -Seed 42

# 只起一部分角色（仍按固定表顺序）
powershell -NoProfile -ExecutionPolicy Bypass `
  -File tools/m2-acceptance/start_mir2_server.ps1 -Roles db-srv,login-srv,game-svr
```

参数：

| 参数 | 默认 | 说明 |
|---|---|---|
| `-Root` | `E:\MirServer` | 部署根；六角色配置均相对它解析 |
| `-ServerExe` | `<repo>\mir2-rs\target\release\mir2-server.exe`（无 release 则 `target\debug`） | 单二进制路径 |
| `-Roles` | 六个全起 | 逗号分隔；按固定表顺序执行，未知角色退出 2 |
| `-Seed` | 空（回落 `$env:MIR2_RNG_SEED`，再回落 **42**） | 注入子进程 `MIR2_RNG_SEED`；非法 int 退出 2 |
| `-DbName` | 空（回落 `$env:MIR2_DB_NAME`） | 只做环境透传 `MIR2_DB_NAME`；**当前全仓无消费者**（`grep -r MIR2_DB_NAME` 只命中本脚本与本 runbook），仅保留与 `start_rust_stack.ps1` 同形，**不改写任何配置文件**，库名仍由各 conf 决定 |
| `-LogDir` | `<Root>\logs\mir2-server` | 日志与 pid 文件目录；**只在实跑前置全过后创建**（`-WhatIf`/前置失败不留目录） |
| `-WaitSeconds` | `60` | 每角色端口就绪等待上限 |
| `-PortsProfile` | 空 | 指向 `tools/window/alt_port_profile.py` 生成的 `alt-port-profile.json`，读其中 `ports_by_role` 替换忙检/就绪端口（15xxx/17xxx 窗口**必须**给） |
| `-Ports` | 空 | 直接按角色覆盖端口（`@{ 'game-svr' = @(15000); … }`）；与 `-PortsProfile` 同时给时本项优先 |
| `-WhatIf` | 关 | 只打印计划，不启动进程、**不创建 LogDir/pid 文件**（无副作用）；缺配置退出 2 |

> **端口剖面**：忙检（实跑前置）与就绪等待都只读 `$RoleTable[].ports`；给了 `-PortsProfile`/`-Ports`
> 就整体替换成新值，不会再误报旧端口（`:312` 忙检、`:343` 就绪等待的旧实现已随重构消失）。
> 清单缺角色/含未知角色/端口非 int/越界 ⇒ 退出 **2**；`-WhatIf` 会打印
> `PORTS_SOURCE …`、每个角色的 `PORTS <role>: …`，以及 `COMMAND … [ports …]`，可据此核对。

**固定启动顺序**（MySQL 是外部依赖，本工具不启动也不停止）：

```text
MySQL(3306, 外部) -> db-srv(6000/5100/5700) -> login-srv(5500/5600)
                  -> game-svr(5000) -> login-gate(7000) -> sel-gate(7100) -> game-gate(7200/7201/7202)
# 备用剖面（-PortsProfile）：db-srv(16000/15100/15700) -> login-srv(15500/15600)
#   -> game-svr(15000) -> login-gate(17000) -> sel-gate(17100) -> game-gate(17200/17201/17202)
```

**停止顺序 = 上面启动顺序的严格反向**（§7.3 按 pid 文件的 `stop_order` 降序执行）。

配置映射（与 `start_rust_stack.ps1` 的六角色口径一致）：

| 角色 | 配置（相对 `-Root`） |
|---|---|
| `db-srv` | `DBSrv\dbsvr.conf` |
| `login-srv` | `LoginSrv\logsrv.conf` |
| `game-svr` | `M2GameSvr\Server.conf` |
| `login-gate` | `LoginGate\config.conf` |
| `sel-gate` | `SelGate\config.conf` |
| `game-gate` | `RunGate\config.conf` |

> **部署布局提示（真机务必按这里做，别照抄 `E:\MirServer` 根上的旧文件）**
>
> `E:\MirServer` 现有布局与上表**不一致**，直接 `-Root E:\MirServer -WhatIf` 会按设计退出 2：
>
> | 上表要求 | `E:\MirServer` 实际 | 差异 |
> |---|---|---|
> | `DBSrv\dbsvr.conf` | `DBServer\dbsvr.conf` | 目录名不同；旧文件用 **C# 键**（`[DataBase] DbType=MySQL`，另有 `[Setup] DBName=HeroDB`），Rust `bins/db-srv` 读的是拼写照抄的 **`[DataBase] Storeage=MySQL`**（`config.rs`；缺该键时走默认值 MySQL） |
> | `LoginSrv\logsrv.conf` | `LoginSrv\config.conf` | 文件名不同；旧文件连接串指向 `server=10.10.0.199;database=Mir2`，不是本机 Rust 栈口径 |
>
> **真机请显式使用仓库内的两份配置**（与 `start_rust_stack.ps1` 的 `-CsharpRepo` 同源，键名与本地连接串都已是 Rust 口径）：
>
> ```text
> <repo>\src\DBSrv\bin\Release\dbsvr.conf      # [DataBase] Storeage=MySQL, database=mir2_db
> <repo>\src\LoginSrv\bin\Release\logsrv.conf   # [Server] 127.0.0.1；[DataBase] DbType=MySQL, database=mir2_account
> ```
>
> 即把它们按上表映射放进一个满足布局的 `-Root`（例如 `E:\tmp\mir2-server-root\DBSrv\dbsvr.conf`
> 与 `...\LoginSrv\logsrv.conf`，其余四份沿用 `E:\MirServer` 的 `M2GameSvr\Server.conf` /
> `LoginGate\config.conf` / `SelGate\config.conf` / `RunGate\config.conf`），**不要**照抄部署根旧文件。
> 本工具不会替你改写/生成配置。

运行产物：

```text
<LogDir>\<role>.out.log / <LogDir>\<role>.err.log
<LogDir>\mir2-server.pid.json
  roles[]: { role, pid, ports, stop_order, exe, args }
```

fail-closed 语义（任一命中即 RED 并退出，不静默）：

- 缺任一配置 ⇒ 退出 **2**（`-WhatIf` 同判据）。
- `-ServerExe` 不存在、`-Seed`/`-DbName` 非法、未知角色 ⇒ 退出 **2**。
- pid 文件里已有存活角色、MySQL 3306 未监听、任一目标端口已被监听 ⇒ 退出 **1**，不动别人的进程。
- 起栈过程中某角色端口未就绪 ⇒ RED、打印 err 日志路径、**按反向顺序回滚已起角色**、删除 pid 文件、退出 **1**。

### 7.3 停栈：`stop_mir2_server.ps1`

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File tools/m2-acceptance/stop_mir2_server.ps1
```

- 读 `<LogDir>\mir2-server.pid.json`，按 `stop_order` **降序**（启动的严格反向）停止。
- `game-svr` 先走命名管道 `\.\pipe\openmir2-gamesvr-shutdown-<pid>` 写 `SHUTDOWN`（内部 5s 存档上限），
  8s 内未退则 Force；其它角色直接 Force。
- 只结束进程名与记录一致（`mir2-server` 或记录 `exe` 的 basename）的进程；pid 复用/进程名不符时打印
  `SKIP`，不误杀。
- 全部停完后校验每条记录的端口是否 DOWN：全 DOWN ⇒ 删除 pid 文件并输出
  `MIR2_SERVER_STACK_STOPPED`；仍有端口 UP ⇒ RED、保留 pid 文件、退出 1。
- **跳过/未停掉 ≠ 已停止（累计 fail-closed）**：只要还有记录被 `SKIP`（pid 非法 / 进程名不符）
  或 Force 后仍存活，就打印 RED、**保留 pid 文件、不打印 `MIR2_SERVER_STACK_STOPPED`、退出 1**。
- 缺 pid 文件 / 空文件 / `[]` / `{"roles":[]}` ⇒ `NOTHING_TO_STOP`，退出 0；JSON 损坏 ⇒ RED，退出 2。

### 7.4 自检（真跑，不启动服务）

本次返工（复核 6 条 Low）在 `E:\tmp\wt-launcher` 上真跑过以下用例；**未启动任何服务、未占端口、未写库**：

```powershell
# 1) 语法解析（两个脚本都应 PARSE_ERRS=0）
foreach ($f in @('tools/m2-acceptance/start_mir2_server.ps1','tools/m2-acceptance/stop_mir2_server.ps1')) {
  $t=$null;$e=$null
  [System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path $f).Path,[ref]$t,[ref]$e) | Out-Null
  '{0} PARSE_ERRS={1}' -f (Split-Path $f -Leaf), $e.Count
}
# start_mir2_server.ps1 PARSE_ERRS=0
# stop_mir2_server.ps1 PARSE_ERRS=0

# 2) 六配置齐备的临时根：-WhatIf ⇒ 恰 6 条 COMMAND（db-srv..game-gate），退出 0，且不创建 LogDir
#    （临时根 E:/tmp/launcher-selftest-r3 下六个空 conf）
powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/start_mir2_server.ps1 -Root E:/tmp/launcher-selftest-r3 -WhatIf
# COMMAND 1 db-srv [ports 6000,5100,5700]: …  COMMAND 6 game-gate [ports 7200,7201,7202]: …
# WHATIF: 6 个角色；启动顺序 db-srv -> login-srv -> game-svr -> login-gate -> sel-gate -> game-gate
# 停止顺序 game-gate -> sel-gate -> login-gate -> game-svr -> login-srv -> db-srv
# exit=0；事后该临时根只有六个角色目录，无 logs/

# 3) 缺配置 / 未知角色 / 非法种子 ⇒ 退出 2
powershell … -File tools/m2-acceptance/start_mir2_server.ps1 -Root E:/MirServer -WhatIf
# RED: 缺配置：db-srv -> E:\MirServer\DBSrv\dbsvr.conf; login-srv -> E:\MirServer\LoginSrv\logsrv.conf
# exit=2
powershell … -File tools/m2-acceptance/start_mir2_server.ps1 -Root E:/tmp/launcher-selftest-r3 -Roles db-srv,bogus -WhatIf
# RED: 未知角色 bogus（允许：db-srv|login-srv|game-svr|login-gate|sel-gate|game-gate）；exit=2
powershell … -File tools/m2-acceptance/start_mir2_server.ps1 -Root E:/tmp/launcher-selftest-r3 -Seed xx -WhatIf
# RED: -Seed/MIR2_RNG_SEED 不是合法 int：xx；exit=2

# 4) start 读到 pid 非数字 ⇒ SKIP 继续（不再抛 [int] 异常中止）
#    临时根 pid 文件：{"roles":[{"role":"db-srv","pid":"abc","ports":[],"stop_order":1}]}
# SKIP db-srv pid 非法：abc（忽略该条记录，继续）
# 之后按既有前置退出 1（目标端口被外部 C# 栈占用；未启动任何进程）

# 5) stop 各分支
# 缺文件 / 空文件                       ⇒ NOTHING_TO_STOP，exit 0
# JSON 损坏                             ⇒ RED，exit 2
# 死 pid（ports 为空）                  ⇒ ALREADY_GONE + MIR2_SERVER_STACK_STOPPED，exit 0
# 伪造 live pwsh（pid=11152，名 powershell）+ 空 ports + 记录名 mir2-server：
#   SKIP db-srv pid=11152 进程名=powershell 与记录（mir2-server）不符 —— 防 pid 复用误杀，不结束
#   RED: 有存活记录被跳过、停止状态未确认：db-srv(pid=11152 进程名=powershell 不符)
#   RED: 栈未确认全部停止 —— 保留 pid 文件 …，请人工核查
#   exit=1；pid 文件保留；该 pwsh 进程未被误杀（DECOY_ALIVE）
# pid 非数字                            ⇒ SKIP + RED + 保留 pid 文件 + exit 1
# 名字相符的活进程（Force 分支）        ⇒ STOPPED … force + 删除 pid 文件 + exit 0

# 6) 端口剖面（本批新增；-Root 用 §8.3 的 deploy 根）
powershell … -File tools/m2-acceptance/start_mir2_server.ps1 `
  -Root E:/tmp/altport-deploy-check `
  -PortsProfile E:/tmp/altport-deploy-check/alt-port-profile.json -WhatIf
# PORTS_SOURCE profile:…/alt-port-profile.json
# PORTS db-srv: 16000,15100,15700 / login-srv 15500,15600 / game-svr 15000
# PORTS login-gate 17000 / sel-gate 17100 / game-gate 17200,17201,17202
# COMMAND 1..6 … [ports …]（exit=0；不建 logs/）
# 清单缺角色 / 未知角色 / 坏 JSON ⇒ RED + exit 2
# 实跑忙检取新值：-Roles db-srv -Ports @{ 'db-srv' = @(3306) } ⇒ RED: 目标端口已被监听：db-srv:3306（exit 1，未起进程）
```

> **-WhatIf 无副作用（本批修复）**：`LogDir` 只在实跑且全部前置通过后才创建。复核者此前
> `-Root E:\MirServer -WhatIf` 在部署根留下的空目录 `E:\MirServer\logs\mir2-server`（2026-10-22 01:00:54）
> **已删除**（`E:\MirServer\logs` 非空，保留）；删除后重跑同一命令，`E:\MirServer` 与 `E:\MirServer\logs`
> 的目录项均无新增（`logs` 下其余文件是外部运行中的 C# 栈日志，未触碰）。
>
> **部署布局**：`E:\MirServer` 的旧配置不符合本工具的固定映射，真机请按 §7.2「部署布局提示」
> 显式使用 `src\DBSrv\bin\Release\dbsvr.conf` 与 `src\LoginSrv\bin\Release\logsrv.conf`。

### 7.5 与旧起栈方式的关系

- 旧 exe、`start_rust_stack.ps1`、`E:\MirServer\start-all.ps1` 全部保持原样；本工具是**新增的可选路径**。
- 本工具不启停 MySQL（3306 由外部提供），也不做 `start_rust_stack.ps1 -Db*` 那套影子目录/连接串改写；
  库名选择仍由六份配置文件决定；`-DbName` 只把 `MIR2_DB_NAME` 透传给子进程（当前无消费者，见 §7.2 表）。
- 行为等价：六个角色进程的 argv 仍是 `<role> <conf>` 经 `mir2-server` 透传，等价于旧 exe 的 `<conf>`。

---

## 8. 备用端口剖面（15xxx/17xxx）

> **tier=L**：本节只新增离线工具与配方，不改任何角色代码、协议、配置格式或数据；工具不启动进程、
> 不占端口、不写数据库、不写 `E:\MirServer`（deploy 只**复制**角色树）。真机窗口本身的 R 级判据
> 不因本节而降级——本节只解决「开窗必须抢占 5000/7000/7200」这个运维冲突。
>
> 背景：外部 C# 全栈常驻 `5000/5100/5500/5600/5700/6000/7000/7100/7200`。备用剖面把它们整体平移到
> `15xxx/17xxx`，六份影子 conf、路由文件与一个新的客户端启动命令都由脚本生成，源文件只读。
>
> **两种产出，别混用**：
> - `generate --out-dir DIR` = **覆盖层**：只写六份 conf + 清单。`DIR` **不是**可独立起栈的 `-Root`，
>   必须与完整角色部署合并；只拷六份 conf **不会**让链路通（缺路由文件，见 §8.2）。
> - `generate --deploy DIR --mir-server-root E:\MirServer` = **完整部署根**：先只读复制六个角色树，
>   再写六份 conf、改写路由文件、把连接串切到测试库；产出可直接
>   `start_mir2_server.ps1 -Root DIR -PortsProfile DIR\alt-port-profile.json`。

### 8.1 工具

| 文件 | 作用 |
|---|---|
| `tools/window/startup_param.py` | 客户端启动参数（`base64(IV8 + Blowfish-CBC(656B))`）的 `decrypt / encrypt / port / roundtrip`；口令与布局只读引用 `MirClientZig/src/app/boot.zig:280-360`，固定向量逐字节自检 |
| `tools/window/alt_port_profile.py` | `generate`（`--out-dir` 覆盖层 / `--deploy` 完整根，`--db-mode test\|prod`）、`mapping`、`selftest`；可选生成 `run-release-alt.cmd` |

依赖：Python 3.12 + `cryptography`（本机实测 50.0.1，`cryptography.hazmat.decrepit.ciphers.algorithms.Blowfish` 可用）。
`alt_port_profile.py` 只 import 同目录的 `startup_param.py`，不依赖对端仓代码。

### 8.2 固定剖面与键名映射

键名/语义**以仓库内 Rust 解析器为准**（实读 `mir2-rs/bins/*/src/config.rs` 与 `bins/db-srv/src/route.rs`，不是按直觉猜）：

| 角色 | 影子 conf（输出布局同 §7.2） | 键 | 旧 → 新 | 含义 |
|---|---|---|---|---|
| `db-srv` | `DBSrv/dbsvr.conf` | `[Setup] ServerPort` | 6000 → **16000** | DataService（GameSvr 取档/存档） |
| `db-srv` | 同上 | `[Setup] GatePort` | 5100 → **15100** | SelGate 下游 |
| `db-srv` | 同上 | `[Server] MarketPort` | 5700 → **15700** | 拍卖行（S4b 预留） |
| `db-srv` | 同上 | `[Server] IDSPort` | 5600 → **15600** | 指向 login-srv IDS |
| `login-srv` | `LoginSrv/logsrv.conf` | `[Server] GatePort` | 5500 → **15500** | LoginGate 下游 |
| `login-srv` | 同上 | `[Server] ServerPort` | 5600 → **15600** | IDS（game-svr / db-srv 连入） |
| `login-srv` | 同上 | `[Server] RunPort` | 7200 → **16005** | **legacy/未消费**：C# 不解析、Rust 不读；保留只为与 SelGate `LsvServerPort=16005` 同形 |
| `login-srv` | 同上 | `[Server] DBSPort` | 6000 → **16000** | **legacy/未消费**：C# 只读进 `Config.nDBSPort`（全仓无消费者），Rust 不读 |
| `game-svr` | `M2GameSvr/Server.conf` | `[Server] GatePort` | 5000 → **15000** | GameSvr 监听（GameGate 下游） |
| `game-svr` | 同上 | `[Server] DBPort` | 6000 → **16000** | 指向 db-srv DataService |
| `game-svr` | 同上 | `[Server] IDSPort` | 5600 → **15600** | 指向 login-srv IDS |
| `login-gate` | `LoginGate/config.conf` | `[LoginGate] ServerPort0` | 5500 → **15500** | 指向 login-srv |
| `login-gate` | 同上 | `[LoginGate] GatePort0` | 7000 → **17000** | **客户端登录入口** |
| `sel-gate` | `SelGate/config.conf` | `[SelGate] ServerPort0` | 5100 → **15100** | 指向 db-srv |
| `sel-gate` | 同上 | `[SelGate] GatePort0` | 7100 → **17100** | **客户端选服入口** |
| `sel-gate` | 同上 | `[SelGate] LsvServerPort` | 16005（不变） | 解析器未消费；保持 16005（manifest 里是 no-op） |
| `game-gate` | `RunGate/config.conf` | `[GameGate] ServerPort1..3` | 5000 → **15000** | 三组都指向 game-svr |
| `game-gate` | 同上 | `[GameGate] GatePort1..3` | 7200/7201/7202 → **17200/17201/17202** | **客户端游戏入口**（保留 `Count=3` 间距） |

其它键（`GateAddr*`、`DBAddr`、`IDSAddr`、`ConnctionString`、`Count`、`[Integer]/[Switch]/[Strings]` 等）逐字节保留。
`MsgSrvPort=4900`、`LogServerPort=10000` 不在本剖面内（外部服务），保持原值。
本剖面 manifest 的 `port_map` 是 **21 条**（4+4+3+2+2+6；`LsvServerPort 16005→16005` 是 no-op，不入表）。

**路由文件（只有 `--deploy` 改写；缺一链路不通）**

`--out-dir` 覆盖层**不含**这些文件，所以「只拷六份 conf」时客户端拿不到 GameGate 地址
（`ServerInfo.txt` 为空路由）甚至 login-srv 直接起不来（缺 `AddrTable.txt`/`UserLimit.txt` 会硬错）。

| 文件 | 语义（Rust 解析器） | deploy 后的值 |
|---|---|---|
| `LoginSrv/AddrTable.txt` | `ServerName Title Remote Public Gate[:Port]…`（GB2312）；`get_sel_gate_info` 按 `ServerName+Public` 匹配，客户端收到的 SelGate 地址就是 `Gate` 列 | Remote/Public=`127.0.0.1`；`Gate=127.0.0.1:17100`；ServerName/Title 原字节保留 |
| `LoginSrv/ServerAddr.txt` | 允许连 IDS 5600 的白名单 | `127.0.0.1` |
| `LoginSrv/UserLimit.txt` | `ServerName Name LimitCountMax`（缺文件会让选服分支 NRE 静默挂起） | 首列对齐 `AddrTable` 的 ServerName；缺文件时生成 `热血传奇 热血传奇 10000` |
| `DBSrv/AddrTable.txt` | 允许接入 5100/6000 的服务器 IP 白名单（不在表里直接断连） | `127.0.0.1` |
| `DBSrv/ServerInfo.txt` | `角色网关IP 游戏网关IP 端口[,端口…]`；`gate_route` 选中行决定客户端去哪个 GameGate | `127.0.0.1 127.0.0.1 17200`；源里出现 7200/7201/7202 时按 17200/17201/17202 平移 |

> `M2GameSvr/!servertable.txt` / `!runaddr.txt` 是 C# GameSvr 的遗留路由文件；**Rust game-svr 不读**
> （`grep -rn 'servertable\|runaddr' mir2-rs` 无命中），deploy 原样复制、不改写。

**数据库目标（`--db-mode`，默认 `test`）**

`ConnctionString` 的值里 `;` 是字段分隔符（`_kv` 会把它当后缀），所以**不能**用通用键替换；
工具按行定位 `ConnctionString=`，只替换该行 `database=` token：

| 角色 | conf | 源（prod） | `--db-mode test`（默认） |
|---|---|---|---|
| `db-srv` | `DBSrv/dbsvr.conf` | `mir2_db` | `mir2_test_db` |
| `login-srv` | `LoginSrv/logsrv.conf` | `mir2_account` | `mir2_test_account` |
| `game-svr` | `M2GameSvr/Server.conf` | `mir2_data` | `mir2_test_data` |

生成后 fail-closed：`test` 模式下任一输出 conf 仍出现 `database=mir2_db;` / `mir2_account;` /
`mir2_data;` 即 RED 退出 1；工具同时打印 `DB_TARGET <role>: database=… (mode=test|prod)`。
测试角色 `winplay0` 只在 `mir2_test_db`，窗口请务必用默认 `test`。

### 8.3 生成（源文件只读）

```powershell
cd <检出>   # 任意检出；--repo-root 必须指向**主检出**（见下）
# 只读映射（不发任何写操作）
python tools/window/alt_port_profile.py mapping --repo-root E:\Users\gxh\Documents\GitHub\OpenMir2

# A. 覆盖层（只六份 conf；不是 -Root，需与完整角色部署合并）
python tools/window/alt_port_profile.py generate `
  --repo-root E:\Users\gxh\Documents\GitHub\OpenMir2 `
  --out-dir E:\tmp\altport-profile -WhatIf
python tools/window/alt_port_profile.py generate `
  --repo-root E:\Users\gxh\Documents\GitHub\OpenMir2 `
  --out-dir E:\tmp\altport-profile

# B. 完整部署根（推荐；链路可用）：复制角色树 + 改写路由 + 连接串切测试库
python tools/window/alt_port_profile.py generate `
  --deploy E:\tmp\altport-deploy `
  --mir-server-root E:\MirServer `
  --repo-root E:\Users\gxh\Documents\GitHub\OpenMir2 `
  --db-mode test -WhatIf
# 去掉 -WhatIf 实跑；deploy 根已存在且非空时必须加 --force
python tools/window/alt_port_profile.py generate `
  --deploy E:\tmp\altport-deploy `
  --mir-server-root E:\MirServer `
  --repo-root E:\Users\gxh\Documents\GitHub\OpenMir2 `
  --db-mode test --force
```

- **`--repo-root` 必须指向主检出**：`src/DBSrv/bin/Release/dbsvr.conf` 与
  `src/LoginSrv/bin/Release/logsrv.conf` 是未跟踪的构建产物，worktree 里没有；工具对 `db-srv/login-srv`
  **只认这两份 Rust 口径配置**（部署根 `DBServer/dbsvr.conf` 缺 `MarketPort`，`LoginSrv/config.conf`
  地址是 `10.10.0.102`，都不适合做默认源）。缺失时报错并提示 `--source db-srv=<path>`。
- `--deploy` 复制六个 `--mir-server-root/<角色目录>`（`DBServer/LoginSrv/M2GameSvr/LoginGate/SelGate/RunGate`，
  源只读），输出目录名按 §7.2 的 `-Root` 布局（`DBSrv/LoginSrv/M2GameSvr/LoginGate/SelGate/RunGate`）。
  已存在且非空需 `--force`；不会删除未知文件。（`--mirserver` 是 `--mir-server-root` 的别名。）
- **端到端路由断言**：生成后自动复核覆盖层，打印
  `ROUTE_VERIFY OK client_login=127.0.0.1:17000 -> selgate=127.0.0.1:17100 -> gamegate=127.0.0.1:17200`
  （即客户端登录 17000 → 选服被路由到 17100 → 游戏被路由到 17200）。
- `-WhatIf` **不建目录、不写文件**；实跑才创建输出根。
- 客户端只写 `D:\MirClient-run\run-release-alt.cmd`（新文件）；`run-release.cmd` 只读，写后工具会
  重新哈希原文件做 fail-closed 断言。已存在 `run-release-alt.cmd` 时需 `--force` 才覆盖。
- 起栈（launcher 必须带剖面，否则忙检/就绪等待会看旧端口）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File tools/m2-acceptance/start_mir2_server.ps1 `
  -Root E:\tmp\altport-deploy `
  -PortsProfile E:\tmp\altport-deploy\alt-port-profile.json -WhatIf
# PORTS_SOURCE profile:…；PORTS db-srv: 16000,15100,15700；…
# COMMAND 1..6（每条带 [ports …]）；exit=0，不建 logs/
```

实跑把 `-WhatIf` 换成 `-Seed 42` 即可；停栈用 `stop_mir2_server.ps1`（§7.3）。
`-WhatIf` 不再是唯一支持的模式；手工起栈时六角色顺序与配置路径见 §7.2，
即 `mir2-server <role> <conf>`（`db-srv → login-srv → game-svr → login-gate → sel-gate → game-gate`，
停止严格反向），并自行确保 `MIR2_RNG_SEED` 与剖面端口一致。

### 8.4 编码与差异判据

- 读写走 **latin-1 逐字节**视图（同 `tools/m2-acceptance/db_target_plumb.ps1`），只替换目标键的值 token。
- BOM 实况（2026-10-22 实测）：
  - `M2GameSvr/Server.conf`：**有 UTF-8 BOM**（LF，3104 行）；
  - 默认源 `src/DBSrv/bin/Release/dbsvr.conf`、`src/LoginSrv/bin/Release/logsrv.conf`：**无 BOM**（CRLF）；
  - 部署根 `E:\MirServer\DBServer\dbsvr.conf`：有 BOM，但**不是默认源**（缺 `MarketPort`）；
  - `RunGate/config.conf` 的 `[Strings]` 段实测有 `EF BC 3F`（既非合法 UTF-8 也非合法 GBK）。
  BOM/非 UTF-8 字节/CRLF/LF/段序/注释全部 1:1 保留。
- 每份文件写完后断言：**差异行集合恰为计划键所在行**；差异行只允许值 token 变化（前缀/后缀逐字节相同）；
  行数不变；目标键读回等于新值；连接串只允许 `database=` token 变化。任一不满足即 `RED` 退出 1。
- 2026-10-22 本机真跑（`generate --deploy`，相对默认源的行号）：

```text
db-srv      diff_lines=[4,6,14,16,21]      BOM=False CRLF=24
login-srv   diff_lines=[7,9,15,19,25]      BOM=False CRLF=25
game-svr    diff_lines=[3,19,21,47]        BOM=True  LF
login-gate  diff_lines=[9,11]              BOM=False CRLF=31
sel-gate    diff_lines=[5,6]               BOM=False CRLF=23
game-gate   diff_lines=[4,5,7,8,10,11]     BOM=False CRLF=236
VERIFY db-srv: diff_set=['[DataBase] ConnctionString (database)', '[Server] IDSPort', '[Server] MarketPort', '[Setup] GatePort', '[Setup] ServerPort'] OK
VERIFY login-srv: diff_set=['[DataBase] ConnctionString (database)', '[Server] DBSPort', '[Server] GatePort', '[Server] RunPort', '[Server] ServerPort'] OK
VERIFY game-svr: diff_set=['[DataBase] ConnctionString (database)', '[Server] DBPort', '[Server] GatePort', '[Server] IDSPort'] OK
VERIFY login-gate: diff_set=['[LoginGate] GatePort0', '[LoginGate] ServerPort0'] OK
VERIFY sel-gate: diff_set=['[SelGate] GatePort0', '[SelGate] ServerPort0'] OK
VERIFY game-gate: diff_set=['[GameGate] GatePort1', '[GameGate] GatePort2', '[GameGate] GatePort3', '[GameGate] ServerPort1', '[GameGate] ServerPort2', '[GameGate] ServerPort3'] OK
DB_TARGET db-srv: database=mir2_test_db (mode=test, 源 mir2_db, 测试 mir2_test_db)
DB_TARGET login-srv: database=mir2_test_account (mode=test, 源 mir2_account, 测试 mir2_test_account)
DB_TARGET game-svr: database=mir2_test_data (mode=test, 源 mir2_data, 测试 mir2_test_data)
```

（`diff_set` 里连接串键显示为 `[DataBase] ConnctionString (database)`，表示只改 `database=` token。）

### 8.5 客户端启动参数

`startup_param.py` 的固定向量（源自 `boot.zig` 的 `oracle_base64` / `fixed_plaintext_hex`）：

```text
$ python tools/window/startup_param.py roundtrip
PLAINTEXT_OK=true bytes=656
ROUNDTRIP_OK=true base64_len=888
IV_HEX=38bcd80f9a040ef7 ADDR=127.0.0.1 PORT=7000
```

`alt_port_profile.py` 生成 `run-release-alt.cmd` 时只改 656B 明文里的地址（offset 31 短串）与端口
（offset 248 i32），其余字段逐字节不变；IV 默认**沿用原参数的前 8 字节**（`--iv random` 可改）。
写入后重新解密断言，工具实际输出：

```text
CLIENT D:\MirClient-run\run-release.cmd (127.0.0.1:7000) -> D:\MirClient-run\run-release-alt.cmd (127.0.0.1:17000)
VERIFY client: decrypt_port=17000 decrypt_addr=127.0.0.1 param_len=888 OK
```

（原文件保护是内部断言：写后重新哈希 `run-release.cmd`，变了就 RED；没有单独的 `ORIGINAL_CMD_UNCHANGED` 打印行。）

### 8.6 离线自检与门禁

```powershell
python tools/window/alt_port_profile.py selftest --repo-root E:\Users\gxh\Documents\GitHub\OpenMir2
# ROUNDTRIP_OK ... / 六份 VERIFY ... OK / DB_TARGET ... / DEPLOY ... /
# ROUTE_VERIFY OK client_login=127.0.0.1:17000 -> selgate=127.0.0.1:17100 -> gamegate=127.0.0.1:17200 /
# ROUTE_ASSERT … / DEPLOY_GUARD_OK … / CLIENT_OK decrypt_addr=127.0.0.1 decrypt_port=17000 param_len=888 /
# ALT_PORT_SELFTEST_OK
```

`selftest` 在系统临时目录构造一个小型 `-MirServer`（六角色配置 + 路由文件；真实路由文件存在时优先复制），
生成完整 deploy 根后校验连接串/路由/覆盖保护，然后自动删除；不碰 `E:\MirServer` 与 `D:\MirClient-run` 的原文件。
本节改动为纯 Python 工具 + PowerShell 编排 + 文档，不改 Rust 代码；仍按 AGENTS.md 跑全套本地门禁
（2026-10-22 本 worktree 实测）：
`cargo fmt` rc=0、`cargo clippy --workspace --all-targets -- -D warnings` rc=0、
`cargo test --workspace` **2760 passed / 0 failed**、
`cargo run -p msgcodegen -- --check` 604 条一致、`whitelist_anchor_gate.py check` 331 条 ALL PASS、
`m3_whitelist_registry_audit.py audit --strict` GREEN。

### 8.7 残留物

- `E:\tmp\altport-profile\`：**覆盖层**六份影子 conf + `alt-port-profile.json` + `mapping.txt` +
  `selftest.txt`（本次/上批证据，保留；不属于仓库）。**该根不是可独立起栈的 `-Root`**，必须与完整角色
  部署合并后才能起栈；要完整根请用 §8.3 的 `--deploy`。
- `E:\tmp\altport-deploy-check\`：本次返工的完整 deploy 根证据（复制角色树 + 路由文件 + 测试库 conf）；
  验证后已删除，不在仓库内。
- `D:\MirClient-run\run-release-alt.cmd`：新增的备用启动命令（原 `run-release.cmd` 未改）。
- 未启动任何服务端/客户端/代理，未占端口，未写数据库；`E:\MirServer` 零写入（deploy 只读复制）。

## 9. 夹具修正：公告轮询与 NPC 点击坐标门（2026-10-22，卡 fixture-harden）

> 根因与证据：`docs/npc-render-diagnosis-20261022.md`（§2.3 公告轮询走位、§2.4 点击变走位、§2.5 客户端身份）。
> 本节只描述**夹具**改了什么、以后怎么跑；服务器/客户端行为零改动（tier=L：纯工具/文档）。

### 9.1 事故回顾（一句话）

第四轮窗口「国王没渲染」不是服务端缺帧，而是 `mir_flow.ps1` 的 `notice_poll` 在固定点 `(522,525)`
**盲点 12 次**：公告（`SM_SENDNOTICE 658`）只吃第一次，其余 11 次落在世界区，每次都重设
`g_nTargetX/Y`，角色从 `(29,33)` 自动走到 `(29,44)` 并落库，国王 `(29,32)` 出视窗。

### 9.2 `mir_flow.ps1` 新行为

| 项 | 旧 | 新 |
|---|---|---|
| 证据源 | 无（只能事后 `export_s2c`） | **`-ProxyDir <proxy --out>`**：运行中直接读 `chunks.ndjson` 的 data 事件 + `conn-*.bin`，按 `#…!`/`!$` 切帧、EDCode 解头块；`-CapturePath` 只作离线/事后证据 |
| 证据新鲜度 | 无 | **F2**：proxy 用 `session.json.start_wall_ms` 作下界；capture 用本流程启动墙钟作下界；只吃 `ts_ms >= SinceMs` 的帧，旧证据一律 RED（旧 capture 在启动客户端之前就被拒） |
| 公告点击 | 固定点盲点 12 次（1.2s 间隔） | **`-NoticePollClicks` 只允许 1**（0 会与 1018 校验矛盾，>1 会走位 ⇒ 都 fail-closed） |
| 等公告 | 无 | 有证据源：等**本场** `s2c SM_SENDNOTICE(658)`（超时 ⇒ `NOTICE_POLL RED` + 中止）；无证据源：必须拿到**客户端侧信号**（`-NoticePixelProbe` / `-NoticeProbe` / `-NoticeWindowProbe`），拿不到就 **只 Move-Mouse、一次也不点**，打印 `NOTICE_POLL UNVERIFIED clicks=0 reason=no_client_signal` |
| 校验 | 无 | 有证据源：点后只认**点击时刻之后**的 `c2s CM_LOGINNOTICEOK(1018)`（取最新帧，旧 1018 不会假通过）；无证据源 ⇒ `UNVERIFIED` |
| 点后鼠标 | 留在世界区继续盲点 | `Move-Mouse` 到 `-NoticeParkPoint`（默认 `522,760`，底部面板区；**只移动不点击**） |
| 公告未出现 | 照点（风险） | 有证据源且超时未见本场 658 ⇒ `NOTICE_POLL RED` + 中止，一次也不点世界区 |

### 9.3 live 证据链（**可复制**；F1）

`export_s2c.ps1` 是**会话结束后**的归档导出器：运行中的 proxy 目录只有 `chunks.ndjson` +
`conn-*.bin`，没有 `capture.jsonl`。所以 live 流程（进图后要过坐标门/点 NPC）用
**同一个 proxy 目录**给 `mir_flow.ps1 -ProxyDir`，不要等导出：

```powershell
# 0) 变量（按本窗口的栈改端口映射；示例口径见 §3.5/§3.6：游戏网关 7600 影子、proxy 监听 7200）
$WIN  = "E:/tmp/window/r6"
$PROXY = "$WIN/proxy"

# 1) 起 proxy（会话期间一直跑；--map 的键是 proxy 监听端口，值是真实上游）
#    与 §3.5/§3.6 同口径：只代理游戏跳（proxy 7200 → 影子 game-gate 7600）；
#    登录/选服 7000/7100 仍由客户端直连，不要映射成 7000→7000（会自环/抢端口）。
python E:/Users/gxh/Documents/GitHub/OpenMir2/tools/capture/mir2_proxy.py `
  --out $PROXY `
  --map 7200=127.0.0.1:7600
#   就绪标志：$PROXY/ready.flag；会话元信息：$PROXY/session.json（含 start_wall_ms/pid）
#   客户端的游戏跳要连 proxy 的 7200（RunGate/GameGate 配置里指向 7200），不是上游 7600。

# 2) 同一个目录给 mir_flow（运行中读 chunks.ndjson + conn-*.bin；无需 capture.jsonl）
powershell -NoProfile -ExecutionPolicy Bypass `
  -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/client/mir_flow.ps1 `
  -Account winplay0 -Password winplay0 -RunDir D:\MirClient-run `
  -OutDir "$WIN/flow" -SkipTalk -WalkClicks 0 -SkipLogout -SkipReenter -LeaveClientRunning `
  -ProxyDir $PROXY -ExpectPos "29,33" -ClickNpc
#   期望：NOTICE_POLL OK clicks=1 dismissed=1018 signal='evidence:658 …' mode=proxy
#         POSITION_GATE GREEN actual=29,33 … trusted_walks=N ignored_far_walks=0
#         NPC_CLICK x=512 y=263 expect=29,33
#   任一证据不足 ⇒ NOTICE_POLL RED / POSITION_GATE RED + 非零退出（不会盲点世界区）。

# 3) 会话结束后再归档（判据/对账仍吃这份 capture.jsonl；与 live 证据同源同 ts）
powershell -NoProfile -ExecutionPolicy Bypass `
  -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/m2-acceptance/export_s2c.ps1 `
  -ProxyDir $PROXY -OutDir "$WIN/s2c" -RunDir D:\MirClient-run
```

要点：

- `-ProxyDir` 与 `-CapturePath` **互斥**；live 用前者，离线回填/事后分析才用后者。
- `-CapturePath` 在 live 流程里也接受，但它的最新 `ts_ms` 必须 >= 本流程启动墙钟（F2）；
  拿上一轮的 capture 会在**启动客户端之前**报 `STALE_CAPTURE`，不会出现“刚登录就立刻看到 658/1018”。
- `-ProxyDir` 在启动客户端前校验 `session.json` + `ready.flag` + `pid` 存活；登录后还要求
  最新 data 事件落在本流程启动之后（`PROXY_STALE` 防上一场遗留目录）。
- 没有任何证据源时，公告点击前必须有客户端侧信号：`-NoticePixelProbe "x,y,RRGGBB,tol"`
  （屏幕取色）、`-NoticeProbe <脚本>`（exit 0=可见/1=不可见/其它=unknown）或
  `-NoticeWindowProbe`（本进程除主窗口外的可见顶层窗口 >0）。三者都没有 ⇒ 不点击。

### 9.4 NPC 点击坐标

- 默认 `-NpcClick "512,263"`：角色 `(29,33)` ⇒ 该点对应 NPC `(29,32)`；
- 旧值 `(512,227)` 换算成 y=31（**两格北的空地**），已在 §3.9 表中统一订正。

### 9.5 manifest 记客户端身份（本轮误判的直接原因）

- `mir_flow.ps1` 启动客户端前写 `<OutDir>/manifest.json`：
  `client_exe` / `client_exe_sha256` / `client_exe_mtime` / `client_exe_size`（另记 `run_dir`、`launcher`）；
- `tools/m2-acceptance/export_s2c.ps1` 新增 `-ClientExe` / `-RunDir`（或环境变量 `MIR2_CLIENT_EXE`），
  在切帧后把同名字段合并进 `$OutDir/manifest.json`；找不到时记 `null` 并 WARN。
  另写 `proxy_session_start_wall_ms`（来自 proxy `session.json`），供事后核对 capture 的 `ts_ms` 是否同一场。
- 注：本机 Windows PowerShell 5.1 **没有 `Get-FileHash`**（实测 `CommandNotFoundException`），
  两处取证都用 .NET `SHA256` 实现，避免静默退化成 null。
- 实测（2026-10-22，假 proxy 目录真跑 `export_s2c.ps1`）：
  `client_exe=D:\MirClient-run\MirClinet.exe`、
  `client_exe_sha256=c3cb8f73d8872c5ad3e53f0f55a96d03139fb49397ebcacb4b3573c3e1c07996`、
  `client_exe_size=6561280` —— 与诊断文档 §2.5 的同哈希/同大小一致（坐实 r5 跑的是冻结 Delphi 客户端）。

### 9.6 离线自检（不启客户端）

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/client/mir_flow.selftest.ps1
```

新增/更新用例（均真跑，不启动客户端、不占端口；2026-10-22 全绿 `SELFTEST_PASS=89`）：

1. `-FixtureDryRun -NoticePollSim dismiss1|slow|absent|nosignal`：sim 与真机共用
   `Invoke-NoticePollCore`（F4）；`slow` 仍只点 **1** 次且不补点；`absent`/`nosignal` ⇒ 世界区 0 点击；
2. `-NoticePollClicks 2` **和** `0` ⇒ 启动前 fail-closed 拒绝（F5）；
3. `-FixtureDryRun -ExpectPos … -CapturePath <假 capture>`：F6 的三种情形——
   单个远目标 `CM_WALK(29,44)` 被忽略并回退到 `SM_LOGON(29,33)`、多格目标不能过门、
   相邻链 `(29,34)…(29,44)` 能推进到真实格；无 `SM_LOGON/NEWMAP` 锚点时 `CM_WALK` 不作证据；
4. F2：`-EvidenceSinceMs` 之后的证据才作数（旧 capture/旧 proxy 均 RED）；
   `-FixturePreflight`（不启客户端）验证旧 capture 在启动前就报 `STALE_CAPTURE`、
   proxy 缺 `ready.flag` 直接拒绝；
5. F1：假 proxy 目录（`chunks.ndjson` + EDCode 编码的 `conn-*.bin`）真跑 live 解码，
   `658/51/3011/1018` 的 `ident/param/tag/recog` 与 `export_s2c` 导出的 `capture.jsonl` 逐字段一致；
6. 源码断言：12 次盲点循环已删除、`-NoticePollClicks = 1`、`-NpcClick = "512,263"`、
   无代码级 `512,227`、manifest 含 `client_exe_sha256`、`-ProxyDir`/EDCode/`ignored_far_walks` 存在、
   阶段表含 `notice_wait/notice_poll`。

### 9.7 仍是离线证据的部分（必须随报告登记）

- 本次返工**没有起真机栈**（写集只含夹具与文档）；live 路径的端到端真机读数仍归独占窗口。
- 公告对话框在 r5 的 `PrintWindow` 截图里不可见（`05_ingame.png` 只有聊天框文字），
  因此 `-NoticeWindowProbe`/`-NoticePixelProbe` 的阈值需要真机现场标定；标定前默认不点
  （F3 的 fail-closed 方向）。
- F6 的“相邻步”规则能处理逐格自动走位（每次目标相邻 ⇒ 链式推进），也能挡住单次远目标；
  若服务端/内存侧以后提供真实坐标源，应优先替换这条推断。

## 附录 A：本文件写作时的只读读数命令（可复现）

```powershell
# 镜像
git -C E:/tmp/m2svr-window-origin rev-parse HEAD
git -C E:/tmp/m2svr-window-origin rev-parse refs/remotes/walgit/master
git -C E:/tmp/m2svr-window-origin rev-list --count ab44031ce..8b8cb10d7
git -C E:/tmp/m2svr-window-origin diff --stat ab44031ce..8b8cb10d7 | Select-Object -Last 1

# provenance + 六件 sha256
Get-Content E:/tmp/m2svr-window-target/window-provenance.json -Raw | ConvertFrom-Json | Select-Object source_head
Get-ChildItem E:/tmp/m2svr-window-target/debug -Filter *.exe | ForEach-Object {
  "{0} {1} {2}" -f $_.Name,$_.Length,(Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower()
}

# 测试库（只读）
& 'D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe' -h127.0.0.1 -uroot -N -e `
  "SELECT table_schema,COUNT(*) FROM information_schema.tables WHERE table_schema IN ('mir2_test_db','mir2_test_account','mir2_test_data') GROUP BY table_schema;"
& 'D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe' -h127.0.0.1 -uroot -N -e `
  "SELECT (SELECT COUNT(*) FROM mir2_db.characters),(SELECT COUNT(*) FROM mir2_test_db.characters),(SELECT COUNT(*) FROM mir2_account.account),(SELECT COUNT(*) FROM mir2_test_account.account),(SELECT COUNT(*) FROM mir2_data.stditems),(SELECT COUNT(*) FROM mir2_test_data.stditems);"
& 'D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe' -h127.0.0.1 -uroot -N -e `
  "SELECT SCHEMA_NAME,DEFAULT_CHARACTER_SET_NAME FROM information_schema.schemata WHERE SCHEMA_NAME LIKE 'mir2%';"

# 端口 / 进程
powershell -NoProfile -ExecutionPolicy Bypass -File E:/MirServer/port-status.ps1

# 前置门
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/m3/window-20261023/preflight.py --win E:/tmp/window/20261023
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_rust_window_preflight.py `
  --repo E:/tmp/m2svr-window-origin --ref HEAD --baseline-ref refs/remotes/walgit/master `
  --mirserver E:/MirServer --target-dir E:/tmp/m2svr-window-target `
  --provenance E:/tmp/m2svr-window-target/window-provenance.json `
  --points "291,666:outside;291,651:outside;289,618:inside;287,620:inside"
```

本文件写作时的关键读数（摘要）：

- 镜像 HEAD `ab44031ce`；`walgit/master` `8b8cb10d7`；27 提交 / 41 文件 / +12735 −6848；R6 生产路径 31 件。
- provenance `source_head=ab44031ce…`；六件 sha256 全 match；mtime 2026-10-21 11:34。
- 测试库 11/13/7；行数 7338/7477/1000 与生产相等；测试库默认 latin1、生产 utf8mb4。
- 十端口全 LISTENING（3306 + 九个游戏端口）；7600 DOWN；外部 C# 栈 pid 见 §1.4。
- `preflight.py`：P1 RED / P2 GREEN / P3 PREREQ_RED（工具 bug）；guard：R1–R5/R7 OK，R6/R8 RED。
- 六条判据的修复提交全部不在 `ab44031ce`（§1.7）。
- merchant-race 只在 `/e/tmp/wt-mrace` 未提交；城堡 stock 0 实体。

## 附录 B：判据片段（只读；放进 `$WIN/judge/`）

### B.1 `smwalk.py`（SM_WALK 条数/相邻/方向）

```python
# 用法: python -I -B smwalk.py <capture.jsonl>
import json, sys
rows = [json.loads(l) for l in open(sys.argv[1], encoding='utf-8') if l.strip()]
s = [r for r in rows if r.get('dir') == 's2c' and r.get('port', 7200) == 7200
     and r.get('frame_form') != 'act' and 'ident' in r]
self_id = next((r.get('recog') for r in s if r['ident'] == 50), None)
last = {}; walk = 0; bad = []; dir_bad = []; self_frames = []
dmap = {(0,-1):0,(1,-1):1,(1,0):2,(1,1):3,(0,1):4,(-1,1):5,(-1,0):6,(-1,-1):7}
for r in s:
    if r['ident'] in (10, 13):
        last[r.get('recog')] = (r.get('param'), r.get('tag'))
    elif r['ident'] == 11:
        walk += 1
        if r.get('recog') == self_id:
            self_frames.append(r.get('seq'))
        prev = last.get(r.get('recog'))
        if prev:
            px, py = prev; x, y = r.get('param'), r.get('tag')
            dx, dy = x - px, y - py
            if max(abs(dx), abs(dy)) != 1:
                bad.append((r.get('recog'), px, py, x, y))
            elif (r.get('series') & 0xFF) != dmap.get((dx, dy), -1):
                dir_bad.append((r.get('recog'), dx, dy, r.get('series') & 0xFF))
        last[r.get('recog')] = (r.get('param'), r.get('tag'))
actors = len({r.get('recog') for r in s if r['ident'] == 11})
print("SM_WALK=%d actors=%d self_frames=%d non_adjacent=%d dir_mismatch=%d" % (
    walk, actors, len(self_frames), len(bad), len(dir_bad)))
if bad: print("BAD", bad[:20])
if dir_bad: print("DIRBAD", dir_bad[:20])
if self_frames: print("SELF", self_frames[:20])
```

### B.2 `frame_compare.py`（骰子/NPC 的 body_sha256 跨侧比较）

```python
# 用法: python -I -B frame_compare.py <csharp.jsonl> <rust.jsonl> <ident>
import json, sys
def load(path, ident):
    rows = [json.loads(l) for l in open(path, encoding='utf-8') if l.strip()]
    return [r for r in rows if r.get('dir') == 's2c' and r.get('port', 7200) == 7200
            and r.get('frame_form') != 'act' and r.get('ident') == ident]
a, b, ident = load(sys.argv[1], int(sys.argv[3])), load(sys.argv[2], int(sys.argv[3])), int(sys.argv[3])
print("C#  %d 帧: %s" % (len(a), [(r.get('recog'), r.get('param'), r.get('tag'), r.get('series'), r.get('body_sha256')) for r in a]))
print("Rust %d 帧: %s" % (len(b), [(r.get('recog'), r.get('param'), r.get('tag'), r.get('series'), r.get('body_sha256')) for r in b]))
ha = sorted(r.get('body_sha256') for r in a)
hb = sorted(r.get('body_sha256') for r in b)
print("BODY_SHA_EQUAL=%s" % (ha == hb))
```

### B.3 `dice.py`（SM_PLAYDICE + 商人 ActorId）

```python
# 用法: python -I -B dice.py <capture.jsonl>
import json, sys
rows = [json.loads(l) for l in open(sys.argv[1], encoding='utf-8') if l.strip()]
s = [r for r in rows if r.get('dir') == 's2c' and r.get('port', 7200) == 7200
     and r.get('frame_form') != 'act' and 'ident' in r]
merchant = next((r.get('recog') for r in s if r['ident'] == 10 and r.get('param') == 8 and r.get('tag') == 10), None)
dice = [r for r in s if r['ident'] == 8001]
print("MERCHANT_ACTOR", merchant)
print("SM_PLAYDICE count=%d recog=%s param=%s tag=%s series=%s body_sha256=%s" % (
    len(dice), [r.get('recog') for r in dice], [r.get('param') for r in dice],
    [r.get('tag') for r in dice], [r.get('series') for r in dice],
    [r.get('body_sha256') for r in dice]))
```

### B.4 `superman.py`（守卫/门 的 SM_DEATH 与伤害帧）

```python
# 用法: python -I -B superman.py <capture.jsonl> <actor_id>
import json, sys
rows = [json.loads(l) for l in open(sys.argv[1], encoding='utf-8') if l.strip()]
aid = int(sys.argv[2])
s = [r for r in rows if r.get('dir') == 's2c' and r.get('port', 7200) == 7200
     and r.get('frame_form') != 'act' and 'ident' in r]
death = [r for r in s if r['ident'] == 32 and r.get('recog') == aid]
hits = [r for r in s if r['ident'] in (14, 31) and r.get('recog') == aid]
print("ACTOR", aid, "SM_DEATH=%d SM_HIT/STRUCK=%d" % (len(death), len(hits)))
print("DEATH_SEQ", [r.get('seq') for r in death])
print("HIT_SEQ", [r.get('seq') for r in hits][:20])
```

### B.5 `nom_merchant_attack.py`（商人不可攻击：0 条伤害帧 + c2s 有 CM_HIT）

```python
# 用法: python -I -B nom_merchant_attack.py <capture.jsonl> <merchant_actor_id>
import json, sys
rows = [json.loads(l) for l in open(sys.argv[1], encoding='utf-8') if l.strip()]
aid = int(sys.argv[2])
s = [r for r in rows if r.get('dir') == 's2c' and r.get('port', 7200) == 7200
     and r.get('frame_form') != 'act' and 'ident' in r]
c = [r for r in rows if r.get('dir') == 'c2s' and r.get('port', 7200) == 7200
     and r.get('frame_form') != 'act' and 'ident' in r]
dmg = [r for r in s if r['ident'] in (14, 31) and r.get('recog') == aid]
cmhit = [r for r in c if r['ident'] == 3014]
print("MERCHANT", aid, "SM_HIT/STRUCK=%d CM_HIT=%d" % (len(dmg), len(cmhit)))
print("DAMAGE_SEQ", [r.get('seq') for r in dmg][:20])
```
