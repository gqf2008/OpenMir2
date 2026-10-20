# tools/loginfixture — 窗口受控登录场景夹具（tier=L）

**它解决什么**：真机窗口里做「同账号两侧（C# oracle / Rust）对比」时，两侧必须从**同一份角色行状态**
登录、从**同一条 7200 下行流**进入地图；否则 52 `SM_ABILITY` 的 HP/MaxHP、50 `SM_LOGON` 的朝向、
46 `SM_DAYCHANGING` 的 Bright 会因为上一侧登出写回/采集时刻不同而漂移。本目录把这两个动作固化成
可复用的离线夹具：

| 文件 | 作用 |
| --- | --- |
| `prepare_safe_login.py` | 对 `mir2_test_*` 库把指定角色摆到安全格（默认 map `0`, `(289,618)`）、Level、HP/MaxHP；**库名守卫** + 默认 dry-run + 单事务 + 改后校验；打印改动前后行快照。 |
| `capture_login_compare.py` | 给定两侧 `conn-*.s2c.bin`（或 `capture.jsonl`），输出入图 15 帧逐帧对比表（ident / body_len / body 字节），并把 **recog** 与 **52 的 40/58 布局差**按白名单排除；退出码 = 是否有非白名单差异。 |

> **状态（必须如实读）**：本车道是 **tier=L（tools/）**。两个脚本**没有**在真机窗口里跑过，
> 没有连 `mir2_test_*` 库、没有起服务端/客户端/代理、没有占端口、没有写数据库。`--selftest`
> 只证明「夹具与守卫逻辑会咬人」；真机证据必须由窗口执行人按下面的「窗口用法」采集。
> 本目录的离线集成用例只用仓库里的金标准（`tests/golden/session-20261010-c3-hook/` 与
> `mir2-rs/tests/golden/c-line-baseline-c3-20261010.jsonl`），**不是**真机验证。

---

## 1. 窗口用法（同账号两侧对比）

前置：按 `docs/window-run-sheet-20261022.md` §E1 已有 `mir2_test_db` / `mir2_test_account` /
`mir2_test_data`（从正式库复制）；窗口里所有 SQL 只写 `mir2_test_*`。**两侧串行**，不要同时占
5000/7000/7100/7200。

设窗口工作目录 `WIN=E:/tmp/window/<轮次>`。

### 1.1 摆位（每侧登录前都跑一次）

```bash
# 先看：只读 + 打印 BEFORE 行快照与计划 SQL（默认 dry-run，不写库）
python -I -B tools/loginfixture/prepare_safe_login.py \
  --db mir2_test_db --char <角色名> --level 1 --hp 19 --max-hp 19

# 确认后应用；会打印 BEFORE/AFTER 行快照并在改后逐列核对
python -I -B tools/loginfixture/prepare_safe_login.py \
  --db mir2_test_db --char <角色名> --level 1 --hp 19 --max-hp 19 --yes-apply
```

要点：

* **每侧登录前重跑**。C#/Rust 服务端在停栈/登出时会写回角色行；上一侧跑完后再摆一次，
  保证下一侧从同一行开始。
* `--db` 只允许 `mir2_test_*`（前缀后必须还有字符）。`mir2_db` / `mir2_account` / `mir2_data`
  / `mir2_test_` 一律 rc=2，且**不执行任何 SQL**。
* 默认安全格 `--map 0 --x 289 --y 618`，可用 `--map/--x/--y` 覆盖。
* `--hp` 必须 `<= --max-hp`，Level 0..255，HP 0..32767；越界在触库前 rc=2。
* 角色定位用 `--char <ChrName>` 或 `--char-id <characters.Id>`（二选一）。0 行/多行都 rc=2，
  缺 `characters_ablity` 行也 rc=2（不新建行）。
* 只改 `characters.MapName/CX/CY/Level` 与 `characters_ablity.Level/Hp/MaxHP`；默认不改
  `LASTUPDATE/ModifyTime`（要改加 `--touch-timestamps`）。`Dir`、`Mp`、`Gold`、背包/技能不动。
* 不依赖 Python MySQL 驱动；直接调 `mysql.exe --batch`（默认
  `D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe`，用 `--mysql/--host/--port/--user/--password` 覆盖）。

### 1.2 抓两侧 7200 下行

按 run sheet 的代理映射，用 `tools/capture/mir2_proxy.py` 把客户端 7200 指到代理，dump 目录里应出现
`chunks.ndjson` + `conn-*.s2c.bin`。同一个账号、同一个角色、走完登录→选角→入图后停栈。

```text
<WIN>/csharp/proxy/conn-0003.s2c.bin   # C# oracle 侧
<WIN>/rust/proxy/conn-0003.s2c.bin     # Rust 侧
```

若已有 `capture.jsonl`（`tools/m2-acceptance/export_s2c.ps1` / GoldenExport 的产物），也可以直接
把它传给 `--oracle/--rust`；工具会自动识别目录里的 bin 或 `capture.jsonl`。

### 1.3 对账

```bash
python -I -B tools/loginfixture/capture_login_compare.py \
  --oracle <WIN>/csharp/proxy --rust <WIN>/rust/proxy \
  --out <WIN>/login-compare.json
```

输出示例（合成 40B vs 58B，仅示意列；真实窗口的 whitelisted 条数 = recog 非 0 且两侧不同的帧数 + 1）：

```text
idx o_ident r_ident    o_blen    r_blen body     note
  0    658    658       320       320 equal
 ...
 11     52     52        40        58 WL52     layout 40/58
 13    811    811        60        60 equal    layout=segmented
 ...
RESULT GREEN: 15/15 帧，0 非白名单差异（whitelisted=<recog 差异帧数 + 1>，head_diag=0）
```

退出码：`0` = GREEN（0 处非白名单差异）；`1` = RED（有差异）；`2` = PREREQ（输入缺/序列找不到/
多流歧义/decode 布局表缺失）。**绝不静默通过。**

* 15 帧默认序列 = `658,51,50,41,213,11029,42,766,54,5008,1106,52,752,811,46`（与
  `s2c_diff.py` / `check_client_s2c_frames.py` 同口径）。`--expect-seq` 可换。
* 工具在流里自动找「首个 658 → 首个 46」；目录里有多条流都含该序列时用 `--conn` 指定
  （bin 用文件名，jsonl 用 `conn` 字段）。
* `811` 是 3×20B 分段（`frame.rs::body_layout` 的 Segmented 契约）。工具默认从协议源码派生布局表
  （复用 `tools/capture/dump_body_layout.py`，不抄第二份）；也可 `--layout <body_layout.json>`。
  布局表缺失且遇到分段帧 ⇒ PREREQ，不静默平铺解码。

---

## 2. 判据与白名单（写死在报告里）

每帧**只比三列**：

| 列 | 规则 |
| --- | --- |
| `ident` | 两侧 12B 命令头的 ident 必须相等；整条序列必须等于 `--expect-seq`。 |
| `body_len` | 解出的 body 字节数必须相等（52 的 40/58 布局对除外）。 |
| `body` | body **解码后字节**必须逐字节相等（52 的 40/58 布局对除外）。 |

白名单（只有两条，逐条写进 JSON 报告的 `whitelisted`，不是静默忽略）：

| 白名单 | 为什么无害 | 工具怎么处理 |
| --- | --- | --- |
| `recog` | `CommandMessage.Recog` = 会话相关 ActorId（`ActorMgr.GetNextIdentity()`），两侧必然不同。 | 不参与比较；差异记 `whitelisted`。`param/tag/series` 默认也不参与（见下）。 |
| `52 layout` | C# oracle 的 `SM_ABILITY` body = **40B**（`Ability.cs` 声明序）；冻结客户端 `TAbility`（`HIGHHP=True`）的 Rust 侧 = **58B**（`record.rs::encode_ability_client`）。 | **仅当两侧 body_len 恰为 {40,58}** 时按布局白名单；**18 个语义字段仍逐项比**（level/ac/mac/dc/mc/sc/hp/mp/max_hp/max_mp/exp/max_exp/weight/max_weight/wear_weight/max_wear_weight/hand_weight/max_hand_weight）。语义不一致 ⇒ RED。 |

**不在白名单内、必须红**：ident 顺序/缺失、非 52 的 body_len/body 差异、52 语义字段差异、
52 两侧同布局（40/40 或 58/58）的 body 差异、decode 失败、序列不完整。

`param/tag/series`：默认只记 `HEAD_DIAG`，不改退出码（任务判据是 ident/body_len/body；
50 的 series=Dir、46 的 param=Bright 依赖采集时状态，见 `tools/m2-acceptance/whitelist.jsonl`）。
若窗口要把头部字段也钉死，加 `--strict-head`：`param/tag/series` 差异升级为 RED（`recog` 仍白名单）。
注意 `752 SM_SUBABILITY` 的 7 个通道在头部 `param` 里（body 为空），要判 752 必须用 `--strict-head`。

`--52-layout-only` 可退回「52 只看 40/58 长度、不比语义」，**不推荐**；默认开语义检查。

---

## 3. 离线自检（不连库/不起栈/不占端口）

```bash
python -I -B tools/loginfixture/prepare_safe_login.py --selftest
python -I -B tools/loginfixture/capture_login_compare.py --selftest
```

| 工具 | 断言/用例 | 末行 | rc |
| --- | --- | --- | --- |
| `prepare_safe_login.py` | **39 条** | `SELFTEST GREEN` | 0 |
| `capture_login_compare.py` | **18 条** | `SELFTEST GREEN` | 0 |

`capture_login_compare.py --selftest` 覆盖：52 40/58 语义相等 + recog 不同 ⇒ GREEN；bin 输入（含 811
分段与明文 act 帧跳过）；目录输入；非 52 body 改 1 字节 ⇒ RED；52 语义改 hp ⇒ RED；52 同布局
body 不同 ⇒ RED；ident 顺序错 ⇒ RED；缺 46 ⇒ PREREQ；`--strict-head` 前后 param 差异；52 白名单
不外溢到 ident 54；以及**离线金标准** `tests/golden/session-20261010-c3-hook/proxy` ↔
`mir2-rs/tests/golden/c-line-baseline-c3-20261010.jsonl` ⇒ GREEN（811 分段解出 60B）。

`prepare_safe_login.py --selftest` 用假 mysql runner 覆盖：库名守卫放行/拒绝、SQL 引号/反斜杠转义、
Level/HP/坐标边界、dry-run 不写、apply 两条 UPDATE + 单事务、改后 HP 不符 ⇒ RED、角色 0 行/多行
⇒ PREREQ、缺 ablity 行 ⇒ PREREQ、非 test 库不触库、默认安全格 `(289,618)`、`--char-id` 路径。

---

## 4. 改坏必红（本车道实际执行的变异 + 逐字节还原）

基线 sha256（本目录提交时的两份文件）：

```text
69d8f9102ddbed86bf56b8811c5bb998c63fe6cf879c28495955642f4de93223  tools/loginfixture/capture_login_compare.py
6c84510e60f424a6b12788cac6989d62928aa28e4b93dd92db18e4aeae2c5084  tools/loginfixture/prepare_safe_login.py
```

每个变异都走「复制备份 → sed 改坏 → `--selftest` 期望 rc=1 → `cp` 还原 → `sha256sum -c` 通过」。

| # | 工具 | 变异（生产路径被改坏） | 指定断言变红 |
| --- | --- | --- | --- |
| M1 | prepare | `DB_PREFIX = "mir2_test_"` → `"mir2_"` | `db_guard 拒绝 'mir2_db' FAIL 却放行了` / `CLI 非 mir2_test_* ⇒ rc=2 FAIL` / `SELFTEST RED`, rc=1 |
| M2 | prepare | 把 ablity 表的 UPDATE 误写成 characters 表（漏写 ablity 表；sed 见下方命令块） | `apply 两条 UPDATE 都在 FAIL` / `两表都被 UPDATE FAIL` / `SELFTEST RED`, rc=1 |
| M3 | compare | `if o.ident == 52 and r.ident == 52 and layout_pair:` → `if layout_pair:`（52 白名单外溢） | `非 52 的 40/58 长度差 ⇒ RED FAIL` / `SELFTEST RED`, rc=1 |
| M4 | compare | `if field == "recog":` → `if field == "recog" and False:`（recog 白名单失效） | `recog 差异记为白名单 FAIL` / `SELFTEST RED`, rc=1 |
| M5 | compare | `if mism:` → `if False and mism:`（52 语义检查失效） | `52 40/58 但 hp 不同 ⇒ RED FAIL` / `SELFTEST RED`, rc=1 |

还原后两份文件的 `sha256sum -c` 均为 `OK`，且 `--selftest` 重新 rc=0。

本车道实际执行的变异命令（`$F` 为工具路径；每条前后都 `cp` 备份/还原）：

```bash
# M1
sed -i 's/^DB_PREFIX = "mir2_test_"/DB_PREFIX = "mir2_"/' "$F"
# M2
sed -i 's/UPDATE %s\.`characters_ablity` SET/UPDATE %s.`characters` SET/' "$F"
# M3
sed -i 's/if o.ident == 52 and r.ident == 52 and layout_pair:/if layout_pair:/' "$F"
# M4
sed -i 's/if field == "recog":/if field == "recog" and False:/' "$F"
# M5
sed -i 's/^                        if mism:/                        if False and mism:/' "$F"
```

> 这些变异只证明**自检会咬人**，不替代真机窗口证据。窗口执行人仍须按 §1 采两侧真实
> `conn-*.s2c.bin` 并跑 §1.3。

---

## 5. 未做 / 已知边界

* **没有真机验证**：没连数据库、没起服务端/客户端/代理、没占端口、没写库、没在窗口里抓包。
* `prepare_safe_login.py` 只摆 7 个列（`characters` 的 MapName/CX/CY/Level + `characters_ablity` 的
  Level/Hp/MaxHP）；不改 `Dir/Mp/Gold/背包/技能`，不创建角色或 ablity 行。
* `prepare_safe_login.py` 用 `mysql.exe --batch` 的默认转义（反斜杠是转义符）。若窗口 sql_mode 含
  `NO_BACKSLASH_ESCAPES`，含反斜杠的角色名可能不同；ASCII/GBK 角色名不受影响，也可改用
  `--char-id`。
* `capture_login_compare.py` 只比 **7200 游戏服 s2c**；7000 LoginSrv / 7100 SelGate 的登录/选角帧不在
  本夹具范围（它们有各自的 parity 判据）。
* 15 帧必须是单流里的「658→46」；如果窗口的入图序列带可选帧（621/211/210/212），用
  `--expect-seq` 显式给出期望序列（`check_client_s2c_frames.py` 的 `equipped/delmagic/g2` 预设可参考）。
* 52 语义检查只比两套布局都有的 18 个字段；oracle 独有的保留/ExpCount 字段不比。
* body 布局表默认从**当前** `mir2-rs/crates/protocol/src/frame.rs` 派生。若 capture 由旧协议产生，
  用 `--layout` 指定与 capture 同批的 `body_layout.json`。
* 头部的 `param/tag/series` 默认只诊断；要钉死加 `--strict-head`（见 §2 的 50/46 采集时依赖）。
* 本目录不新增/修改 `src/**`、`sql/**`、`MirClient/`、`mir2-rs/crates/{protocol,storage}`；
  与窗口其它车道（`bins/game-svr/**`、`crates/world/src/{monai,monster*}.rs`、
  `docs/window-round2-runbook*`）无写集重叠。
