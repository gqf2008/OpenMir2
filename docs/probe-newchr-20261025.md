# probe `newchr` 步（CM_NEWCHR=101）— 让 W-LF-1「可跑」— 2026-10-25

- 卡：`openmir2-probe-newchr-step`（tier **L**；owner `openmir2-dev-1`；worktree `.worktrees/dev-1`；
  分支 `card/probe-newchr-step`）。
- 写集：`tools/probe/mir2_probe.py`、`tools/probe/scenarios/login_newchar.json`、本文件。
- 红线：不改 `card_window.py` / `run_batch.sh`；**不声称 W-LF-1「已覆盖」**——只把「新角色建号→进世界→
  首包记录」这条路径做成**可跑**；判据仍归真机窗口。

## 0. 一句话

`login_only` 固定用既有角色（`--char-name winplay0`），无法触发**出生点**路径 ⇒ W-LF-1 只能记「部分/缺」。
本片给 DSL 加 `newchr` 步（`CM_NEWCHR=101`）：在选角相位建新号（可策略化复用重名）→ 重新查列表 → 选中 →
进世界，并把**首包**（`SM_LOGON` 的 x/y/Dir/Light、`SM_CHANGEMAP` 的 MapFileName、`SM_MAPDESCRIPTION`
的 MapDesc）落到 `run.json.chain.first_pack`。

## 1. 协议依据（oracle 行号）

| 项 | oracle | 说明 |
|---|---|---|
| 消息号 | `src/OpenMir2/Messages.cs:30` `CM_NEWCHR = 101` | 客户端→服务端（SelGate） |
| 请求载荷 | `src/BotSrv/Scenes/Scene/SelectChrScene.cs:237-241` `SendNewChr(uid,uname,shair,sjob,ssex)` = `EDCode.EncodeMessage(msg) + EDCode.EncodeString(uid + "/" + uname + "/" + shair + "/" + sjob + "/" + ssex)` | 12B 头 ident=101/recog=0 + **`account/name/hair/job/sex`**（5 段 `/` 分隔） |
| SelGate 透传 | `src/SelGate/Services/ClientSession.cs:68`（`case Messages.CM_NEWCHR` 并入 QUERYCHR/SELCHR 一路转发 DB） | 无本地校验 |
| DB 处理 | `src/DBSrv/Services/Impl/UserService.cs:566-666` `NewChr` | 解析 5 段；`nCode` 判定见下 |
| 响应 | `UserService.cs:650-658`：成功 `MakeMessage(SM_NEWCHR_SUCCESS(521), 0, …)`，失败 `MakeMessage(SM_NEWCHR_FAIL(522), nCode, …)` | ⇒ **失败原因在 `Recog`** |
| 建号落库 | `UserService.cs:259-280` `NewChrData`：只写 `ChrName/Account/Sex/Job/Hair`（`(byte)` 截断），**不写 MapName** | ⇒ 首登地图由 StartPoint 决定 |
| 首包 `SM_LOGON` | `src/M2Server/Player/PlayObject.cs:1131` `MakeMessage(SM_LOGON, ActorId, CurrX, CurrY, HUtil32.MakeWord(Dir, Light))` | `param=x`、`tag=y`、`series` 低字节 `Dir`、高字节 `Light` |
| 地图文件名 | `PlayObject.cs:3778` `SendMsg(RM_CHANGEMAP, …, envir.MapFileName)` → `PlayObject.Message.cs:1776` `SendDefMessage(SM_CHANGEMAP(634), …, processMsg.Msg)` | body = `Envir.MapFileName`（**不是**逻辑 MapName） |
| 地图描述 | `PlayObject.Base.cs:894` `SendDefMessage(SM_MAPDESCRIPTION(54), …, Envir.MapDesc)` | body = `MapDesc` |
| 出生点 | `PlayObject.Base.cs:715-717` 以 `M2Share.StartPointList` 的 `MapName` 与角色当前 `Envir.MapName` 匹配（`!setup.txt` 11 键） | 新角色记录无 MapName ⇒ 走 StartPoint |

### 1.1 `SM_NEWCHR_FAIL.Recog(nCode)` 语义（`UserService.cs:566-646`）

| nCode | 含义 |
|---:|---|
| 0 | 解析门：多余字段 / 名字长度 <3 / 含非法字符（`EnglishNames` 时非英文） |
| 2 | 名称已被使用（`Index>=0`）或 `CheckDenyChrName`/`Add` 失败 |
| 3 | 该账号已有两个角色（`ChrCountOfAccount >= 2`） |
| 4 | 建号数据落库失败（`NewChrData` false，回滚索引） |

> **服务端不校验 job/hair/sex 的职业合法性**：`NewChrData` 直接 `(byte)nJob` 落库。故 probe 的
> 「职业非法」负向是**客户端 fail-closed**（schema 0..2），不是服务端帧；「重名」负向才由服务端
> `SM_NEWCHR_FAIL(Recog=2)` 判定并打印帧。

## 2. DSL 语义

### 2.1 形状（fail-closed）

```json
{"newchr": {"name": "newchr_name", "job": 0, "hair": 0, "sex": 0, "on_exists": "reuse"}}
```

- `name`：字面量或 params 名（解析后非空、3..20 字符、不含 `/` 与 `\`；oracle 长度<3 ⇒ nCode 0）；
- `job` 0..2、`hair` 0..255、`sex` 0..1（int 或 params 名）；
- `on_exists`：`"reuse"`（默认）或 `"fail"`；未知键/越界/重名缺省值外的枚举 ⇒ 加载期 `ScenarioError`；
- **位置**：必须 `steps[0]` 且整场景至多一次（它在选角相位执行；后面写等于不可达，`check_scenario` 拒）。

### 2.2 执行序（SelGate 相位）

```
连接 SelGate → CM_QUERYCHR → SM_QUERYCHR（列表）
  └ newchr 步：
       CM_NEWCHR(account/name/hair/job/sex)
       ├ SM_NEWCHR_SUCCESS(521) ⇒ 记 created ⇒ **重新 CM_QUERYCHR**（oracle Bot 行为）
       ├ SM_NEWCHR_FAIL(522, Recog=nCode)
       │    ├ on_exists=reuse ∧ nCode=2 ∧ 名字已在列表 ⇒ 记 reused，继续
       │    └ 其它 ⇒ StepFailure：nCode 语义 + 服务端帧清单（ident/recog/body）
       ├ SM_OUTOFCONNECTION(528) ⇒ StepFailure
       └ 超时 ⇒ StepFailure（打印已收帧）
  → CM_SELCHR（新名）→ SM_STARTPLAY → GameGate 登录 → SM_LOGON
```

- 成功但刷新后的 `SM_QUERYCHR` 列表里没有该名 ⇒ StepFailure（fail-closed）。
- `--char-name` 与场景 `newchr.name` 不一致 ⇒ ScenarioError（不允许「建 A 却选 B」）。

### 2.3 首包记录（`run.json.chain.first_pack`）

```json
{"seq":…, "actor_id":…, "x":…, "y":…, "series":…, "dir":…, "light":…,
 "body_hex":"…", "map_file":"<SM_CHANGEMAP body>", "map_desc":"<SM_MAPDESCRIPTION body>",
 "map_name": null, "map_name_note":"客户端线上不含逻辑地图名…"}
```

- `dir/light` 由 `series` 位拆（`MakeWord` ⇒ 低字节/高字节）；
- **逻辑 MapName 不上线**：客户端只需要文件与描述。W-LF-1 的「地图」项请用 DB 快照
  `characters.MapName`（`card_window` 的 dbsnap 面）或 `!setup.txt` `StartPoint` 交叉核；
  probe 给出 `map_file/map_desc` 作为**过程证据**。

## 3. 失败模式（不静默）

| 场景 | 行为 |
|---|---|
| 重名 + `on_exists=fail` | `StepFailure`：`SM_NEWCHR_FAIL(522) Recog(nCode)=2 ⇒ 角色名已被使用…; server frames since newchr: [seq/ident/recog/body…]` |
| 重名 + `on_exists=reuse` 且列表已有 | 记 `reused`，继续选该角色（重跑友好） |
| 已两个角色（nCode=3） | 即使 `reuse` 也 StepFailure（`reuse` 只对 nCode=2 生效） |
| 名字 <3/含 `/`、job/hair/sex 越界 | **加载期** `ScenarioError`（不发帧；oracle 对 job 不校验，见 §1.1） |
| 无回包/断线 | StepFailure，附已收帧 |
| 成功但重查列表无该名 | StepFailure |

## 4. 与 W-LF-1 的映射（只说「可跑」）

| W-LF-1 项 | probe 证据 | 说明 |
|---|---|---|
| 出生点坐标 x/y | `first_pack.x/y`（`SM_LOGON.param/tag`） | 真机两侧对拍 = 逐值 diff=0 |
| 朝向 | `first_pack.dir`（`series & 0xFF`） | 来自 `humData.Dir`（`NewChrData` 不写方向 ⇒ ctor 缺省 0 待真机核） |
| 光照 | `first_pack.light`（`series >> 8`） | 过程证据 |
| 地图 | `first_pack.map_file/map_desc` + **DB `characters.MapName`**（dbsnap）+ `!setup.txt` StartPoint | 逻辑 MapName 不上线，必须 DB/配置侧核 |

> **可跑 ≠ 覆盖**：本片只解除「只能既有角色」的限制，让 W-LF-1 有入口；判据仍是真机同 tick 的
> 首包字节对拍 + DB 后态。

## 5. 真机可跑性

- **需要**：完整栈（LoginGate/SelGate/DB/GameSvr/GameGate）+ 可登录账号 + 测试库；
  无额外夹具（不需要 `--char-name`，新名由 `--var newchr_name=…` 给）。
- **副作用（重要）**：建号**持久落库**且每账号上限 **2 个角色**（nCode=3）。重复跑请用
  `on_exists=reuse`（同名字复用）或换名；窗口结束后按需在测试库清理新角色。
- **本卡未跑真机**；窗口验证点：`login_newchar` 场景在 `alt`/`window-magic` 两 profile 下各跑一次，
  核对 `first_pack` 与 C# 侧同场景逐字节/逐值一致（W-LF-1 窗口行）。

## 6. 测试与牙

- `--selftest`：**16 checks**，新增 `[ok] dsl-newchr 形状 2 正 + 10 坏例；位置 2 坏例；reuse 4 象限；
  运行期 created/reused/fail(code2)/code3 4 例；S-2 与 B1 各 1 钉`。
- 场景：`login_newchar` `--check-scenario --var newchr_name=abc123` ⇒ rc=0；**26/26** 场景全 rc=0。
- 牙（实跑：变异 → 红 → 还原）：
  1. `job` 上界 2→3 ⇒ 坏例（job=3 被接受）红；
  2. `newchr_should_reuse` 去掉 nCode==2 门 ⇒ reuse 象限（nCode=3）红；
  3. `_do_newchr` 失败支改「记 failed 后 return name」（静默继续）⇒ 运行期 fail 用例红；
  4. 去掉「newchr 必须 steps[0]」校验 ⇒ 位置坏例红；
  5. **S-2**：从 `SEND_KINDS` 删 `dlg_select_any` ⇒ 「只有 dlg_select_any+wait」语义用例红（R1 无 send）；
  6. **B1**：把 `dlg_select_any` 缺 643 的分支改成 `recog=0` 继续 ⇒ B1 钉红。

## 7. 门禁

fmt/clippy 不涉（纯 Python 工具）；卫生门禁（conflict/stale `--ref master`/dynamic `--ref master`/
board）+ `git diff --check <base> <head>` + `merge_tree_helper` 见 patch body。
