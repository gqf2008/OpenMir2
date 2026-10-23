# 对账门（coverage recon gates）—— 完成定义 §4-1 / §4-2

卡：`openmir2-coverage-recon-gates`。依据：`docs/移植计划重审-20261015.md` §1（红名单）/ §4-1 / §4-2。

本目录提供**两道常驻对账门**，把「计划内实现片合完 ≠ 该实现的都实现了」这一系统盲区变成**可复算、
可证伪的红检**；`docs/移植计划重审-20261015.md` §1 的三张红名单（R0 内容 / R1 入站派发 / R2 出站编码器）
即本门第一批输出。

## 文件

| 文件 | 作用 |
| --- | --- |
| `recon_gates.py` | 门本体（`content` / `dispatch` / `selftest` / `mutate` 四子命令，纯标准库，Python 3.8+）。 |
| `cards.json` | 卡登记：`known_cards`（看板上存在的卡 id，含少数在途实施卡）+ `assignments`（缺口 key → 卡 id）+ `known_unreachable`/`known_false`（人工豁免登记，**逐条带证据**，见下「豁免登记的证据门」）+ `assignment_roles`（出站 36 帧 triage 三分类归属）。 |

## 判红口径（缺一即不红）

- **内容门**：`oracle 有该 Load 调用 ∧ 部署有该内容文件 ∧ Rust 真装载实现 ∧ 无卡` ⇒ **RED**。
- **派发门**：`oracle 有该 case/帧 ∧ 该帧可达 ∧ Rust 无派发臂/编码器 ∧ 无卡` ⇒ **RED**。

「无卡」= `cards.json` 的 `assignments` 未给该缺口指派一个存在于 `known_cards` 的卡 id。
「Rust 实现」= 派发：去注释/字符串字面量后出现该标识符；内容：**真装载调用**（见下）。
「可达」（派发·出站） = 触发该 SM 帧的**任一** `RM_*` 在 `src/**/*.cs` 里有**生产者用法**（任何非「仅 `const` 定义 / `case` 标签」的出现，含经动态形参 / 间接发送；见下）。
「豁免」= 缺口的 key 在 `cards.json` 的 `known_unreachable` / `known_false` 里**且该条目通过证据门**（人工登记，与自动可达性互为兜底）。
空部署文件（0B/空目录）按 `n/a-empty` 处理——oracle 的 `File.Exists` 门后读 0 行，与「不加载」行为无差
（对齐 §1 备注：`MapEvent.txt`/`DisableMoveMap.txt` 空文件本部署无差）。

### 出站帧「触发 RM 可达性」判据（修 C 类假红；对抗验证 v3 收紧）

出站 `SM_*` 帧由 `PlayObject.Message.cs` 的某 `RM_*` 支产出。若**触发它的所有 `RM_*`** 在
`src/**/*.cs` 里**每一处出现都只是 `const` 定义或 `case` 标签**（即全仓无任何生产者用法）⇒ oracle
该支**永不发出** ⇒ 该帧为**死支**，Rust 无编码器不算缺口 ⇒ 状态 `n/a-unreachable`（不判 RED）。

- **可达判据（保守 / 欠豁免）**：一个 `RM_*` 只要在 `src/**/*.cs` 里出现**任何非「纯 `const int RM_x = …;`
  定义」、非「纯 `case Messages.RM_x:` 标签」的用法**即判「可达」——包括作为任意调用
  （`Send*` / `SendRefMsg` / `Walk(RM_WALK)` / `ClientChangeDir` / …）的实参、赋给局部变量后经**动态形参**
  间接发送（`wIdent = Messages.RM_BIGHIT; … SendRefMsg(wIdent,…)`）、参与比较等。⇒ **动态形参 / 间接发送
  一律不漏**；对无法文本判定者**一律按可达处理**，只有能证明「每一处出现都是 const 定义或 case 标签」
  时才允许豁免。注释 / 字符串先经 `strip_code` 剔除，文档里提一句 `RM_x` 不会误判可达。
- 触发本判据的**门豁免**（`status=n/a-unreachable`）：`SM_41`/`RM_41`、`SM_43`/`RM_43`、
  `SM_PASSWORDSTATUS`/`RM_PASSWORDSTATUS`（`RM_*` 全仓仅 `const` 定义 + `case` 标签）。`dispatch --show-rm`
  对「无生产者的 RM」标 `← 死支（确证无生产者）`——该标记是 **RM 级** advisory：如 `RM_SYSMESSAGE2/3`
  亦无生产者，但其帧 `SM_HEAR` 仍由 `RM_HEAR` 触发 ⇒ **帧级**可达（`reach=yes`），故不豁免。
- **为何收紧（旧口径静默豁免真可达帧）**：旧口径「整行同时含 `Send*(` 与该 `RM_*` 字面量」会把经
  **动态形参 / 辅助函数**间接发出的 RM 误判死支 ⇒ 该帧被判 `n/a-unreachable` **静默豁免**（对抗验证 v3
  判 refuted）。真语料实测：旧口径把 **10 条**真可达帧误判死支——
  `SM_WALK`(`Walk(RM_WALK)`)、`SM_RUN`(`Walk(RM_RUN)`)、`SM_HORSERUN`(`Walk(RM_HORSERUN)`)、
  `SM_BIGHIT`/`SM_POWERHIT`/`SM_WIDEHIT`/`SM_LONGHIT`/`SM_FIREHIT`/`SM_CRSHIT`/`SM_TWINHIT`
  （`wIdent = Messages.RM_*` 后 `SendRefMsg(wIdent,…)` 动态形参）。收紧后**帧级死支（可达性=False）
  由 14 收敛到 4**（`SM_41`/`SM_43`/`SM_PASSWORDSTATUS` 三条 C 类 + `SM_DONATE_FAIL`；后者 `RM_DONATE_FAIL`
  同样仅 const+case，但 `SM_DONATE_FAIL` 已立卡 ⇒ `status=tracked`），**门 `n/a-unreachable` 计数归位为 3**，
  10 条全转可达。复现对照：对同一 commit 跑本补丁前后的 `dispatch --json` 比 `reach` 列；或跑 `mutate` 的 M4c。
- **2026-10-23 更新（卡 `openmir2-recon-unreachable-registry`）**：上段的「3」是片118 时的**自动判据**计数。
  该卡把已审计的登记项逐条带证据并入豁免表后，`dispatch` 实测
  `summary = {"implemented": 178, "tracked": 1, "n/a-unreachable": 11, "n/a-oracle-noop": 1}`，
  `tracked` 仅剩 `SM_SKELETON`（由卡 `openmir2-sm-skeleton-wiring` 实现中）。登记面：
  `known_unreachable` = 3 条 RM 死支 + `CM_OPENDOOR`/`SM_OPENDOOR_OK`/`SM_CLOSEDOOR`(T-65/T-176)
  + `SM_ITEMSHOW`(T-19/25/32/179) + `SM_CHANGEFACE`(T-200；订正后的真实链见下) + `SM_QUERYYBDEAL`/`SM_QUERYYBSELL`
  （部署级：Envir 从不派发脚本动作 343/344）；`known_false` = `SM_DONATE_FAIL`（非缺陷）。
  注意 `SM_QUERYYBDEAL`/`SM_QUERYYBSELL` 的自动可达性仍判 **可达**（`src/**` 有生产者），
  其不可达性来自**部署内容**（四棵 Envir 树 grep 0 命中）⇒ 只能人工登记，门无法自动推出。
- **2026-10-23 订正（review `45715661` F1）**：`SM_CHANGEFACE` 的登记理由**不是**「现网造不出 race-114」——
  `sql/mir2_data.sql:372` 里 `神兽1`(Idx=147, **Race=114**) 确实存在，`MySqlDB.cs:171` 全表加载。真实链是：
  能刷出 `神兽1` 的面只有 MonGen/脚本（部署精确 token=0）与 GM 刷怪命令（`@Mob`/`@RecallMob`/`@MobPlace`），
  而 GM 命令被 `GameCommand.cs:37` 的 `PermissionMin` 门挡住；普通玩家 `Permission` 的唯一赋值点
  `PlayObject.cs:1188-1198`（`GetHumPermission`）**被整段注释** ⇒ Release 构建下恒 0。T 号由 T-175 订正为
  **T-200**（`docs/rewrite-remaining-inventory-20261022.md:547`）。
  **边界**：该结论只覆盖三套本地 Release 部署（`!deployed-from.json` 指向 `bin/Release`、DB=127.0.0.1/mir2_data）；
  `E:/MirServer/Mir200` 是 **Debug 构建**（`GameSvr.dll` 含 `GameCommand.cs:35` 的 DEBUG-only 串）且指向远端
  `10.10.0.199/Mir2`、无 `!deployed-from.json`、不在 `start-all.ps1`/`port-status.ps1` 的十端口栈里
  ⇒ 不在本判定的现网面内、也**未**被排除；若 owner 改以 Mir200/任一 Debug 构建为对拍面，该登记立即失效。

### 出站「证据 = 编码器生产调用点」判据 + 调用值未丢弃（片118）

`dispatch` 表新增 **`emit` 列**（`Emit(path, line, kind, used, encoder)`），回答「该帧到底从哪发」：

- `call_site`：编码器函数（签名返回帧类型、ident 出现在**值位**）在**生产活跃函数**内的调用点 ⇒
  证据列 `rust` 取该**调用点**（例：`SM_WHISPER` 由编码器定义体 `smframe_ext.rs:513` 改指 `main.rs:1081`）。
- `inline`：帧**就地**构造（ident 作派发臂 / 帧构造实参 / 结构体字段值），无独立编码器可查 ⇒ **不算 RED**。
- `definition_only`：编码器存在但生产调用点不可文本定位（跨 crate 动态派发等）⇒ 退回旧口径证据。
- **调用值未丢弃**（对称入站 no-op 臂）：`call_site` 另查调用点返回值是否被使用——**拒绝**裸语句
  `sm_x(…);` 与 `_` 前缀绑定（`let _ = sm_x(…)` / `let (d, _f) = …`），命中 ⇒ 不判 `implemented`
  并在 `emit` 列标注 `value_dropped`。**反向豁免**：`_` 前缀绑定若在**调用点之后**（同行剩余 +
  **最近生产函数 span 内**的后续行）仍被引用 ⇒ 视为已使用（`let _body = sm_x(1); gates_send(_body);`
  不误判丢值；探针不跨函数串名）。
- **残余（如实登记，判据未放宽）**：宏展开的发射点不可静态定位；返回值经**结构体字段中转**算「已使用」
  （字段从未读则假绿，需运行期断言）；多行裸语句、以及「语句同行有尾随记号」（`if c { sm_x(1); }`、
  `fn f() { sm_x(1); }`）的值丢弃形态落 `used`（**漏判方向，非假绿**）；`definition_only` 在真实语料
  计数为 **0**。**只收紧不放宽**：落片时真实语料 status 列 **0 差异**、`value_dropped` **0 条**、
  内容门 stdout 逐字节相同（片118，独立复验 CONFIRMED）。

### 内容门「真装载调用」判据（收紧旧「文件名串出现」口径）

旧口径「文件名串在源码里出现」可被**精确引号字面量**（如未用的 `const FAKE: &str = "X.txt";`）假绿。
新口径要求：**某一个 src 文件内同时命中** ①文件名字符串字面量 ②装载标记——`load…(` 调用 / `fn load…`
定义，或镜像模块路径（文件 stem = 内容件模块名且含函数定义）。⇒ 注释-only、以及「只把文件名写进
未用常量、无任何装载调用」都不再算实现。（`mutate` 的 M5、`selftest` 的内容三牙钉住此行为。）

### 部署前置（fail-closed）

`--envir-dir`/`--notice-dir` **不存在 / 不是目录 / 不可读** ⇒ 内容门（与 `mutate`）判 **`PREREQ_RED`**：
印 `PREREQ_MISSING` + `RESULT: PREREQ_RED` 且退出码 `2`——**判不了缺口就拒跑**，绝不因部署面缺失而
静默报「无缺口」假绿（对抗验证 v5 F1：旧版目录缺失 ⇒ 每件 `find_deployed→None` ⇒ 全表 `n/a-deployed`
⇒ 假绿退出 0）。诊断需要看表时用 `--allow-missing-deploy` **显式**降级为 `WARN` 放行（仅诊断，缺口结论不可作数）。

## 用法

```sh
cd mir2-rs
python tests/parity/coverage/recon_gates.py content            # 内容加载对账门
python tests/parity/coverage/recon_gates.py dispatch --show-rm # 协议派发对账门（附 RM→SM 映射）
python tests/parity/coverage/recon_gates.py selftest           # 夹具自测（含「构造缺口必红」牙）
python tests/parity/coverage/recon_gates.py mutate             # 变异实测（真实输入构造缺陷必红）
```

- 退出码：`0` = 无 RED（缺口全部「已实现或已卡」）；`1` = 存在 RED；`2` = 用法/环境错误**或前置缺失
  `PREREQ_RED`**（部署目录不存在/不可读/不是目录）。`--allow-missing-deploy` = 诊断用逃生开关（降级为 WARN）。
- `--repo <OpenMir2 根>`（默认从脚本位置推）、`--envir-dir`/`--notice-dir`（默认 `E:/MirServer/M2GameSvr/{Envir,Notice}`）、
  `--cards <json>`、`--json <out>`。

## 数据来源与口径

| 面 | 来源 |
| --- | --- |
| oracle 内容 | `src/GameSrv/GameApp.cs` 全部 `Load*` 调用点（脚本解析，非硬编码行号）+ `RobotManage.cs:25`（显式声明行）。 |
| oracle 派发 | `src/M2Server/Player/PlayObject.Message.cs` 的 `case Messages.CM_*`（入站）与文件内 `Messages.SM_*` 引用（出站）；出站帧可达性另扫 `src/**/*.cs` 的 `RM_*` **生产者用法**（见上）。 |
| 部署面 | `E:/MirServer/M2GameSvr/{Envir,Notice}` 文件系统（大小写不敏感，Windows 语义）。 |
| Rust 面 | 工作区全部 `mir2-rs/{crates,bins}/*/src`（派发门排除 `crates/protocol/src`——那里只有常量定义与通用编解码，非发射点）。 |
| 卡 | `cards.json`（可 `walgit collab ls` 刷新 `known_cards`）。 |

`CONTENT_MAP` 把 oracle 方法名映射到内容件（kind/envir·notice·dir·db·other + 源锚），随 oracle 漂移时同步更新。

## 变异实测（`mutate`，≥2 必红）

1. **M1 注入缺口**：临时 oracle 加一条合成 `CM_ZZMUT_GAP` ⇒ 派发门新增 1 条 RED。
2. **M2 撤卡**：删掉 `outbound:SM_DISAPPEAR` 的卡指派 ⇒ 该项 `tracked → RED`。
3. **M3 读侧方向**：空掉 Rust 语料 ⇒ 真仓库里 `implemented` 的 `GuardList.txt` 必转 RED
   （证明门真读 Rust 代码，而非只看 oracle/部署面——针对 `docs/移植计划重审-20261015.md` §2 的「RED-1 读错对象」教训）。
4. **M4 可达性（正反成对 + 保守口径）**：
   - **M4a/M4b**：合成 oracle 造一个死支 `RM_ZZDEAD`（仅 const+case）帧 `SM_ZZDEAD` ⇒ `n/a-unreachable`
     （**不红**）；造一个可达 `RM_ZZLIVE`（有 `Send*` 发送点）帧 `SM_ZZLIVE` ⇒ **RED**。
   - **M4c 真语料回归**：`oracle_rm_producers` 必须把真仓库里 10 条经**动态形参 / 辅助函数**发出的 RM
     （`RM_WALK`/`RM_RUN`/`RM_HORSERUN`/`RM_BIGHIT`/`RM_SPELL2`/`RM_WIDEHIT`/`RM_LONGHIT`/`RM_FIREHIT`/
     `RM_CRSHIT`/`RM_TWINHIT`）全判可达——旧「同行 `Send*(` 字面量」口径会判死支 ⇒ 静默豁免。
   - **M4d 合成动态形参**：`Walk(RM_ZZWALK)` 与 `wIdent=RM_ZZHIT; SendRefMsg(wIdent,…)` 帧 ⇒ 生产者
     ⇒ 无编码器必 **RED**；仅 const+case 的 `RM_ZZDEAD2` ⇒ `n/a-unreachable`。
5. **M5 内容门收紧**：真 oracle + 只把 `"GuardList.txt"` 写进未用常量、无装载调用 ⇒ `GuardList.txt`
   必 **RED**（旧「文件名串出现」口径会 `implemented` 假绿）；补一条 `fn load_guard_list` ⇒ `implemented`。

## 接入本地门禁

`.walgit/ci.toml` 或合并前 checklist 追加（**report-only 起步**，RED 清零后翻 gate）：

```sh
python mir2-rs/tests/parity/coverage/recon_gates.py selftest   # 必绿
python mir2-rs/tests/parity/coverage/recon_gates.py mutate     # 必绿
python mir2-rs/tests/parity/coverage/recon_gates.py content    # 出表；RED 见快照
python mir2-rs/tests/parity/coverage/recon_gates.py dispatch   # 出表；RED 见快照
```

## 维护约定

- 缺口**要么实现要么立卡**：实现后门自动转 `implemented`；立卡后在 `cards.json` 加一行
  `assignments["<kind>:<name>"] = "<card-id>"`（`<card-id>` 须在 `known_cards` 里）。
- oracle **死支**（触发 `RM_*` 全仓每一处出现都只是 `const` 定义 + `case` 标签）⇒ 门自动判
  `n/a-unreachable`；如需人工钉住一道已知死支/门误报（含**部署级不可达**：冻结内容永远不会触发），
  在 `known_unreachable`/`known_false` 登记该 key（与自动判据互为兜底）。注意可达判据**保守**：动态
  形参 / 间接发送一律按可达处理，宁可多判可达（造出可处置的 RED），也不静默豁免真可达帧。
- **豁免登记的证据门**（卡 `openmir2-recon-unreachable-registry`）：每条豁免必须是**结构化条目**，
  而不是一句散文：

  ```json
  {"id": "outbound:SM_CLOSEDOOR",
   "kind": "oracle-structural",
   "reason": "oracle 为什么永不发这一帧（结构性 / 内容级 / 部署级）",
   "evidence": "src/GameSrv/Word/WorldServer.cs:1479",
   "t": "T-65", "refs": ["T-176"]}
  ```

  机判规则（`exemption_problem`，`dispatch`/`content` 均生效）：

  1. 必须是对象且 `id`/`reason`/`evidence` 非空；缺字段 ⇒ 该登记**判红且不生效**；
  2. `kind` 必须显式声明三分类之一：`oracle-structural`（oracle 代码结构性死支）/
     `deployment-level`（冻结内容或部署构建导致不可达）/ `gate-false-positive`（门误报、非缺陷）；
     **`kind=deployment-level` 必须同时写 `scope`**，写明结论覆盖哪几套部署（防「四棵部署」口径被默认化）；
  3. 必须有可追溯判定出处：`t` 形如 `T-<n>`，**或** `determination` 指向既有判定（`文档:行` 或线程 id+oid）；
  4. `evidence` 必须是 `file:line`，且该文件**在仓库里真实存在**、`line` 在 `1..文件行数` 内
     （`evidence` 只认仓库内路径；部署侧（`E:/MirServer/**`）的计数写进 `reason`/`scope`，不写进 `evidence`）。

  违反 1–4 任一 ⇒ 印 `REGISTRY_EVIDENCE_RED: <key> [<bucket>] <原因> ⇒ 该登记不生效（按未豁免处理）`
  且命令 **rc=1**；该 key 同时**不再享受豁免**（落回 `tracked`/`RED`）。旧式纯字符串条目一律按未豁免
  处理（迁移时补 `{id,kind,reason,evidence,t|determination}`）。

- **证据门的边界（必须连同上面一起读；review `45715661` F2）**：本门只校验登记的**形状 / 字段完备性 /
  分类 / evidence 的 file:line 是否存在且界内**，**不校验证据与结论的相关性**。反例（reviewer 已实测）：
  把确实可达的 `SM_SKELETON` 从 `assignments` 摘掉、按结构化格式写进 `known_unreachable`，`evidence`
  随便填一条仓库里真实存在的行、`t=T-65` ⇒ `dispatch` **rc=0**、该帧变 `n/a-unreachable`、`tracked` 1→0、
  **无** `REGISTRY_EVIDENCE_RED`。⇒ **门绿 ≠ 该条已由机器证明不可达**；可达性结论仍须由 reviewer 在钉住
  commit 上人工复核（这正是本门把「理由 + 证据 + 分类」结构化、要求 `scope` 的原因：让人工复核有据可查，
  而不是宣告机器已判）。
- 出站 36 帧 triage 三分类（A 真缺编码器 / B 有编码器缺发射点 / C 门误报-oracle 死支）逐条落
  `assignment_roles`；A/B 指向承接卡、C 进 `known_unreachable`。
- 数值随实现推进**递减**：`tracked`/`RED` 计数是进度表（`docs/移植计划重审-20261015.md` §4-6）。
- oracle 漂移（新增 `Load*` 调用 / 新 `case`）⇒ 内容门会以 `unmapped` 行暴露未归类项；派发门自动纳入。

---

## GM 命令可达性登记（`gm_registry.py` / `gm_registry.json`）

卡：`openmir2-gm-reachability-registry`（返工件，reviewer-1 `ad13dc87` 的 B1/B2/I1/I2/I3/M1/M2）。
目的：把「88 条未实现 GM 命令」从一句「GM 不可达」变成**逐条可复算、能与源码逐字段对上的静态证据**。

```sh
python -I -B mir2-rs/tests/parity/coverage/gm_registry.py build     # 生成/刷新 gm_registry.json（utf-8 + LF，可逐字节复现）
python -I -B mir2-rs/tests/parity/coverage/gm_registry.py gate      # 门禁（红=1 / 绿=0 / 判不了=2）
python -I -B mir2-rs/tests/parity/coverage/gm_registry.py summary   # 一条命令复算汇总
python -I -B mir2-rs/tests/parity/coverage/gm_registry.py selftest  # 14 颗牙（逐颗校验登记表 sha256 未变）
```

数据源路径可用 `--conf`（部署 `Command.conf`）、`--envir`（部署 `Envir` 根）、`--deploy`（部署根，判构型）覆盖；
缺任一必需源 ⇒ `build` rc=2 / `gate` rc=2（fail-closed，绝不产出「看起来绿」的登记表）。
`build` 还会自校验推导表与 `gmcmd.rs` 的 `COMMANDS` 完全一致，不一致同样 rc=2。

**四种判定**（写死在工具里，不靠人工结论）：

| verdict | 判据 | 处置 |
|---|---|---|
| `reachable` | `perm_min == 0`（玩家自然可达） | **必须开实现卡**，禁止登记成不可达 |
| `script-reachable` | `0 < perm_min ≤ 10` ∧ 部署脚本存在**可达**的 `GMEXECUTE <cmd>` 调用 | **必须开实现卡** |
| `unreachable` | `perm_min > 0` ∧ 部署脚本里没有该命令的可达调用 | 与现网等价，不实现 |
| `structural-unreachable` | 解析不出任何命令体（无字段/无 `[Command]` 类） | 必须能证明；有 handler 却登记成它 ⇒ 红 |
| `undetermined` | 任一必需数据源缺失 / 解析歧义 | 门禁红（fail-closed） |

`script-reachable` 的依据（B1）：脚本动作 `GMEXECUTE`（`ExecutionCode.cs:464` /
`ExecutionProcessingSys.cs:4925-4947`）先把 `playerActor.Permission = 10`，再 `ProcessUserLineMsg("@<cmd> …")`，
落到 `CommandSystem.Execute` → `GameCommand.Handle`（`GameCommand.cs:37`）的**同一条权限门** ⇒ 只要命令被
部署脚本真实调用、且所在 label 可达，Release 现网就可达（`perm_min ≤ 10`）。

**数据源（五方交叉）**

1. `mir2-rs/bins/game-svr/src/gmcmd.rs` 的 `COMMANDS`（现网最终名 / perm / imp）；
2. `src/Modules/GameCommand/GameCommands.cs`：字段构造默认名 + `PerMissionMin` + `[RegisterCommand(typeof(T))]`；
3. `src/Modules/GameCommand/Conf/GameCmdConf.cs`：`commands.<Field>.CmdName = ReadWriteString("Command","<Key>",…)`
   —— **`<Key>` 是字段键，不是 CmdName**（`StorageLock`/`Flag`/`SuperMake`/`SpiritStart`/`Map`/`MapInfo`…），
   按出现顺序叠加、缺键回落上一层（`Level` 有两条：先 `Level` 后 `Level0`，后写覆盖）；
4. `src/Modules/GameCommand/Commands/*.cs`：每个类的 `[Command(...)]` 行（handler file:line）；
5. 部署 `Envir`（GBK）：按 oracle 的**加载规则**求已加载脚本集合 + **label 可达性** + `GMEXECUTE` 实参。

**命令名 / handler 的推导顺序**（复刻 `CommandSystem.cs:41-117`）：

```
customCommandMap: 键 = 字段的 [RegisterCommand] 类 T 的 [Command] 属性名
                  值 = GameCmd{ CmdName = conf[Command][字段键] ?? 构造默认,
                               PerMissionMin = conf[Permission][权限键] ?? 构造默认 }
RegisterCommandGroups: 遍历 Commands 程序集里所有带 [Command] 的类；属性名命中 customCommandMap ⇒
                       最终名/权限被上述值覆盖；未命中 ⇒ 用类属性名/属性权限
```

> **自校验**：推导出的 185 条名字与权限必须与 `gmcmd.rs` 的 `COMMANDS` **完全一致**（`crosscheck.ok`），
> 不一致 ⇒ 门禁红。这是「取数口径对不对」的机器判据 —— `Map` ↔ `MapInfo` 正是被这条钉住的
> （`Map → MapInfoCommand.cs:8`、`MapInfo → ShowMapInfoCommand.cs:6`）。

**部署脚本的加载集合**（不比 oracle 宽松，也不凭名字猜）：

- 根：`Market_Def/QFunction-0.txt`（`LocalDB.QFunctionNpc`）、`MapQuest_def/QManage.txt`（`QMangeNpc`）、
  `Robot_def/RobotManage.txt`（`RobotNpc`）、`MerChant.txt` 每行（`Market_Def/<col0>-<col1>.txt`）、
  `Npcs.txt` 里 code=1 的 Quest NPC（`Npc_def/<名>-<地图>.txt`）；
- 边：`#CALL [路径] @label` 传递闭包（路径按 Windows 大小写不敏感解析，落在 `QuestDiary/` 下）；
- **未加载的文件不计证据**（例如 `Market_Def/QFunction-0 - 副本.txt`：没有任何引用 ⇒ 42 行 GMEXECUTE 是死副本）。

**label 可达性**（证据只认「会真的执行到」的 label）：

- 根 = C# 里所有 `GotoLable(...)` 的字面量标签与 `ScriptFlagConst` 常量（含 `@main` 点击根、`@StdModeFunc<n>`
  / `@OnTimer<n>` / `@KillPlay*` 前缀根）——由工具扫 `src/` 现算，不写死；
- 边 = label 体内的 NPC 超链接 `<…/@x>`、`#ACT` 行里的 label 实参（`goto` / `DelayCall` / `DelayGoto` /
  `QUERYVALUE` …）、`#CALL`（跨文件）；
- 既不是根、也没有任何入边的 label ⇒ 它的 `#ACT` 永不执行，**其 GMEXECUTE 不算证据**，
  并逐条登记进 `dead_gmexecute_labels`（附 `incoming=0` 的穷举说明）。

**部署面（I1，必须显式）**：oracle = `E:/MirServer/M2GameSvr`，**Release**：

- `!deployed-from.json` 的 `build_dir` 以 `\bin\Release` 结尾；
- `GameSrv.dll` / `CommandSystem.dll` **不含** UTF-16LE 串「当前运行调试模式」
  （`GameCommand.cs:33-36` 的 `#if DEBUG` 分支才会写）。

> **若把 Debug 构建当对拍面（如 `E:/MirServer/Mir200/GameSvr.dll`，含该 UTF-16LE 串）**：
> `#if DEBUG` 会把每个调用者的 `Permission` 直接置 10 ⇒ 所有 `perm_min ≤ 10` 的命令对普通玩家全部可达，
> **本表全部 `unreachable` 结论立即失效（全部改判 `reachable`）**。工具把这一条写进
> `deployment.debug_caveat`，`gate` 要求它非空。

**门禁判红口径**（校验「与源码一致」，不只是自洽）：

- `format` 必须是 `gm-registry/2`；登记表必须是本工具的规范序列化（utf-8 + LF + indent=1）；
- **与 `build()` 的结果逐字段比对**（handler / perm_min / perm_source / verdict / script_evidence / summary 全字段）——
  A 在界指错命令、B perm 10→8、C 改 cmd 名、D `register_command=true`+`handler=null`、E `perm=null`+`unreachable`
  这五类「自洽但错」的登记全部当场变红；
- handler 文件存在 / 行号在界 / 该行含 `[Command(` / **从该行解析出的类的注册名必须等于本行 `cmd`**（挡「在界指错命令」）；
- `register_command=true ⇒ handler 非空`；`perm_min=null ⇒ verdict=undetermined`；`perm_min==0 ⇒ verdict=reachable`；
- `script-reachable ⇒ 证据非空`，且每条证据的 `file:line` 必须是**已加载**脚本里的 `GMEXECUTE` 行、实参解析到本命令；
- `unreachable ⇒ 本次源码扫描里没有该命令的可达脚本调用`；`structural-unreachable ⇒ 命令表里确实解析不出 handler`；
- `crosscheck.ok` 必须为真（推导表 vs `gmcmd.rs`）；`deployment.oracle.build` 与 `debug_caveat` 必须显式。

**14 颗牙**（`selftest`，每颗跑完都校验登记表 sha256 不变）：

```
A 在界指错命令   B perm 10→8      C 改 cmd 名      D register_command=true + handler=null
E perm=null+unreachable            F perm_min=0 却判 unreachable
G handler 行号越界                 H script-reachable 无证据
I 伪造脚本证据（非 GMEXECUTE 行）  J 把 conf 键当 CmdName（B2 原缺陷）
K 证据指向未加载的副本脚本         L StartPermission=1/10 重算（结构性行不被翻、perm=10 不被 sp=1 翻）
M Map / MapInfo handler 未对调     N script-reachable 未链接子卡
```

> 改了 `gmcmd.rs`、`GameCommands.cs`、`GameCmdConf.cs`、`Commands/*.cs`、部署 `Command.conf` **或部署 Envir 脚本**
> 之后，都要重跑 `build` + `gate` + `selftest`。
