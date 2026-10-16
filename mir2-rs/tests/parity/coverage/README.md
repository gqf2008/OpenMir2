# 对账门（coverage recon gates）—— 完成定义 §4-1 / §4-2

卡：`openmir2-coverage-recon-gates`。依据：`docs/移植计划重审-20261015.md` §1（红名单）/ §4-1 / §4-2。

本目录提供**两道常驻对账门**，把「计划内实现片合完 ≠ 该实现的都实现了」这一系统盲区变成**可复算、
可证伪的红检**；`docs/移植计划重审-20261015.md` §1 的三张红名单（R0 内容 / R1 入站派发 / R2 出站编码器）
即本门第一批输出。

## 文件

| 文件 | 作用 |
| --- | --- |
| `recon_gates.py` | 门本体（`content` / `dispatch` / `selftest` / `mutate` 四子命令，纯标准库，Python 3.8+）。 |
| `cards.json` | 卡登记：`known_cards`（看板上存在的卡 id，含少数在途实施卡）+ `assignments`（缺口 key → 卡 id）+ `known_unreachable`/`known_false`（人工豁免登记）+ `assignment_roles`（出站 36 帧 triage 三分类归属）。 |

## 判红口径（缺一即不红）

- **内容门**：`oracle 有该 Load 调用 ∧ 部署有该内容文件 ∧ Rust 真装载实现 ∧ 无卡` ⇒ **RED**。
- **派发门**：`oracle 有该 case/帧 ∧ 该帧可达 ∧ Rust 无派发臂/编码器 ∧ 无卡` ⇒ **RED**。

「无卡」= `cards.json` 的 `assignments` 未给该缺口指派一个存在于 `known_cards` 的卡 id。
「Rust 实现」= 派发：去注释/字符串字面量后出现该标识符；内容：**真装载调用**（见下）。
「可达」（派发·出站） = 触发该 SM 帧的**任一** `RM_*` 在 `src/**/*.cs` 里有**生产者用法**（任何非「仅 `const` 定义 / `case` 标签」的出现，含经动态形参 / 间接发送；见下）。
「豁免」= 缺口的 key 在 `cards.json` 的 `known_unreachable` / `known_false` 里（人工登记，与自动可达性互为兜底）。
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
  `n/a-unreachable`；如需人工钉住一道已知死支/门误报，可在 `known_unreachable`/`known_false` 登记该
  key（与自动判据互为兜底）。注意可达判据**保守**：动态形参 / 间接发送一律按可达处理，宁可多判可达
  （造出可处置的 RED），也不静默豁免真可达帧。
- 出站 36 帧 triage 三分类（A 真缺编码器 / B 有编码器缺发射点 / C 门误报-oracle 死支）逐条落
  `assignment_roles`；A/B 指向承接卡、C 进 `known_unreachable`。
- 数值随实现推进**递减**：`tracked`/`RED` 计数是进度表（`docs/移植计划重审-20261015.md` §4-6）。
- oracle 漂移（新增 `Load*` 调用 / 新 `case`）⇒ 内容门会以 `unmapped` 行暴露未归类项；派发门自动纳入。
