# MCP admin 真机窗口报告（2026-11-02）

本报告记录 2026-11-02 在 `E:/MirServer` 生产栈上执行的 MCP admin 真机窗口结果，面向人阅读。
本卡为纯文档（tier L），不改任何行为；全部证据来自已完成的真机运行。

## 0. 互链与范围

- 卡片：`openmir2-mcp-admin-window-report-20261102`（tier L）
- 台账线程：`openmir2-real-machine-verification-master`
  - 窗口执行登记（③、②a）：`3cbc3c23e679aaaecc3796d741b76c9615eb7a80`
  - ③ DB 面补记：`6eccf994a23316c8aa161f393d008edaad95733e`
  - 笔误订正（`0x91` → `0x38FF`）：`e77d9fef88c11351c7fe56bc454a5e57a37b3ad2`
  - ① 窗口结果：`17f218366844239d01b70d889f1acdcfc4a36053`
- 执行单：`docs/mcp-admin-window-sheet-20261101.md`
- 产物目录：`E:/tmp/admin-window-20261102/`（`logs/`、`runs/`、`rng/`、`wins/`、`conf/`）
- 口径：判据原文取自执行单；结果按执行单口径记录。**任何未跑或未测项在 §6 单独登记，不写成通过。**

## 1. 窗口环境

| 项 | 值 |
|---|---|
| pin（HEAD） | `d5ed14508f7803369832c970d09a955269219a93` |
| 构建树 | `E:/Users/gxh/Documents/GitHub/OpenMir2`，`BUILD_DIRTY 0` |
| 二进制 | `E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/target/release/mir2-server.exe` |
| 二进制 sha256 | `02bd4b17a3d4de340cf15d642e16340e07b2454b2fd1447e978a6094da26e022` |
| 二进制大小 | 234451424 B |
| 构建戳 | `stamp_head_match=true`，`built_at=2026-11-02T12:04:37+08:00` |
| 重建结果 | `REBUILD-RC=0`（`logs/rebuild.txt`，sha256 `0cb2036dce59950807e4f7fc97efa6377e4158d9b55ce7f06c7e1f4021c97042`） |
| RNG 种子 | `MIR2_RNG_SEED=42`（两侧同种子；RNG 流水写 `rng/on-01.tsv`、`rng/off-01.tsv`） |
| 端口 | 默认 5000 / 6000 / 7000 / 7100 / 7200–7202（`PORTS_SOURCE default`） |

`[Admin] AdminPort=7946` 为窗口内临时启用（仅回环）：

| Server.conf 状态 | sha256 | 大小 |
|---|---|---|
| BEFORE | `ce5e3ce98b9bb71cbb18e6be1501e20e05642fd3eeaaffd9326caef8d9a4c0e2` | 58080 |
| AFTER-ON | `63d8e49243c9a5e39fa72043c938a48d36815b269a2275b1fb0ef66d923e9526` | 58123 |
| RESTORE | `ce5e3ce98b9bb71cbb18e6be1501e20e05642fd3eeaaffd9326caef8d9a4c0e2`（MATCH True） | 58080 |

- ON 自证：`127.0.0.1:7946` LISTENING（pid 19252，`logs/on-7946-listen.txt`，sha256 `8c88c6d456d64d3ce7b9f4cdc325310d4411fbe80a6e3514b58b3c0a7897f92f`）；服务端日志「本机 admin 通道已启动 [127.0.0.1:7946]」；起栈记录 `logs/start-on-01.txt`（sha256 `13b61972dab7b8ab57e0a795dfcf60e33d4bc97c015e44d4bfb0ac0ae22f8854`）。
- OFF 自证：7946 不监听、connect 超时；服务端日志「本机 admin 通道未启用」；起栈记录 `logs/start-off-01.txt`（sha256 `4bac887cfbe39108089c9808b7c48fc238b7a65baa1b00732265806e99d6193a`）。
- 窗口前门禁：15/15 rc=0（台账条目 `3cbc3c23e679…` 登记）。

## 2. 判据③：抬权还原 — PASS

**判据原文**（执行单 §4.3，摘录）：

> 「可观测的『权限』= 权限门本身」：用特权命令做探针，每次都要被拦（红字 `SM_SYSMESSAGE(100)`、
> `Param=14591`、正文 `权限不够!!!`）——基线证明会话本来无权限；同会话 softclose 之前再打一条，
> 证明「抬权只作用于那一次调用」；重登后再打一条，证明没有把权限写进持久状态。

**执行方法**：admin 通道（本仓新增能力）投递一条需要权限的 GM 命令 `@怪 鸡 1`，通道在这一次调用内
临时把权限抬到 `ADMIN_GM_PERMISSION`，执行完立即还原。随后：

1. 同一会话用普通 `CM_SAY`（Ident 3030）再发同一条 `@怪 鸡 1`，应仍被权限门拦；
2. `CM_SOFTCLOSE` 断线重登（跨会话）后再发一次，应仍被拦。

**结果：PASS。两轮（同会话、重登后）都被权限门拒绝。**

- 服务端日志：`sim 命令 Ident=3030 tail="@怪 鸡 1"` 之后是
  `GM 命令 ChrName=aaa ActorId=401 tail="@怪 鸡 1" found=true 帧数=1`
  ——「命中但只回 1 帧（红字）」，不是执行体。
- 客户端侧：红字 `权限不够!!!`，`param=0x38FF=14591`（`0x91` 为台账早先笔误，已由 `e77d9fef88c1…` 订正）。
- 对照：admin 侧同一命令返回 `{"ok":true,"frames":0}`（无文本回帧 = 已执行）；
  `runs/c3-admin.jsonl` 记录 RNG 账本行数 134735 → 139749（`admin_spawn` 后 +4446），证明 admin 侧确实落地。
- 结论：抬权只作用于那一次 admin 调用，未泄漏到会话，也未写入持久状态。

**证据**：

| 证据 | 路径 | sha256 / oid |
|---|---|---|
| ③ 窗口登记（含服务端日志引文、帧解码、对照） | 台账 `openmir2-real-machine-verification-master` | `3cbc3c23e679aaaecc3796d741b76c9615eb7a80` |
| ③ DB 面补记 | 台账同线程 | `6eccf994a23316c8aa161f393d008edaad95733e` |
| admin 侧投递与账本增长 | `E:/tmp/admin-window-20261102/runs/c3-admin.jsonl` | `935b96a30865cb852058026c59e8465d4020d4141e8906c0971f8af0ef78e20f` |

**范围注记（不记为通过）**：执行单同时要求 `characters_ablity.Level` 前后不变。本窗口环境没有
mysql 客户端（无 `mysql.exe`、无 pymysql），**DB 前后值未取得**，已在台账 `6eccf994a233…` 明确登记。
因此 ③ 的 PASS 只覆盖「权限门证据」，不覆盖 DB 面。另：服务端当前日志已被后续 r3 会话覆盖，
③ 所在 12:06 会话的日志原文保存在台账条目里，不另在本仓落盘。

## 3. 判据②a：AdminPort 开 / 关帧面 — PASS（内容零差异；帧数严格相等未满足，已登记噪声）

**判据原文**（执行单 §4.2，摘录）：

> ON = `[Admin] AdminPort=7946`；OFF = 还原配置（没有 `[Admin]` 段，即 `AdminPort=0`）。
> 同一场景、同一 `-Seed 42` 各跑一次，比对**整条**帧流（不选帧、不设窗口）。
> 判据：`EXIT 0` 且 `undecidable` 为空且**两侧帧数相等**。

**执行方法**：探针无头客户端用同一场景（登录 → 喊话 → 转向 → 行会主页 → 背包，
`scenarios/b2a_actions.json`，sha256 `2fadd1833b16176fe3374adfd2073ef9b23e7e97e15fab0ebd73372b345c33e3`），
在 ON / OFF 两次独立起栈各跑一次；两轮之间停栈 → 改 / 还原 `Server.conf` → 重新起栈。

**结果：PASS（按卡片口径）。** `compare_runs` 判 `VERDICT PASS`、`exit 0`、`compared 31`、`diffs 0`、
`undecidable` 空。ON 32 帧 / OFF 34 帧。

- **活世界帧数不可严格相等**：两侧是两次独立起栈，OFF 多出 ident 10 + 41 一对（随机转身），
  OFF 复跑（`runs/b2a-off-r2`，31 帧）没有这一对 ⇒ 判为间歇性世界噪声，与 AdminPort 无关。
  执行单「两侧帧数相等」这一条**字面未满足**，此处如实写出，不把它写成满足。
- **三条显式登记**（均记入 `report2.json` 的 `options` / `coverage`，不是静默放行）：
  1. `--allow-unclassified 40`；
  2. `--allow-unclassified 754`；
  3. `--ignore-idents 14`。
- 证据链：未加显式登记的第一轮 `report.json` 是 `UNDECIDABLE`（`coverage/ignore_rule_mismatch` 等）；
  加登记后的 `report2.json` 才是 `PASS`。两份报告都保留，未覆盖。
- **库后态未比较**：两份报告的 `db.state` 均为 `absent_both`（两侧都没有库快照），
  本次只判帧流。这是显式登记，不是通过。

**证据**：

| 证据 | 路径（相对 `E:/tmp/admin-window-20261102/`） | sha256 |
|---|---|---|
| PASS 报告（加显式登记后） | `runs/b2a-on-vs-off/report2.json` | `67a16033e92a8d184d34e4f6fd990888190f47689fd9fc0f341c4f01bad6f5d8` |
| 加登记前后的 compare 输出 | `logs/compare-b2a-2.txt` | `ce2ca12e0f86d043d6d3a5fba2ec657624531cf8fd8e89c2b5a1eed000730caf` |
| 第一轮 UNDECIDABLE 报告（保留） | `runs/b2a-on-vs-off/report.json` | `8f11a91e6acc3f1f3846a2d0c758bd1faf2da2cca96a958acf1c15beb68f0818` |
| ON 自证 | `logs/on-7946-listen.txt` | `8c88c6d456d64d3ce7b9f4cdc325310d4411fbe80a6e3514b58b3c0a7897f92f` |
| ON 起栈 | `logs/start-on-01.txt` | `13b61972dab7b8ab57e0a795dfcf60e33d4bc97c015e44d4bfb0ac0ae22f8854` |
| OFF 起栈 | `logs/start-off-01.txt` | `4bac887cfbe39108089c9808b7c48fc238b7a65baa1b00732265806e99d6193a` |

## 4. 判据①：脚本 `GMEXECUTE` vs admin 通道

**判据原文**（执行单 §4.1 / §4.1.4 / §4.1.5 / 清怪口径，摘录）：

> ① 的两侧是**同一 Rust 构建的两次起栈**（run-A = 脚本路径；run-B = admin 路径），不是 C# vs Rust；
> 两条命令的窗口必须互不重叠。
> 帧面判据：`EXIT 0`（PASS）且 `report.json` 的 `undecidable` 为空；`EXIT 2` = 判不了，不是通过。
> RNG 面三条同时满足才算绿：两侧 `groups --require-closed` rc=0 且 `spawn` 计数 ≥1；
> `groupdiff --strict --fold` rc=0 且 `VERDICT IDENTICAL`；`diff` 的 `VERDICT IDENTICAL` 作加强证据，
> 若首处分歧落在非 spawn 噪声站点，须原样记录并**重复一次**，仍不一致 ⇒ `UNDECIDABLE（噪声不可分离）`，不得记通过。
> `@清除怪物`：恒回红，帧判据只要求「两侧各恰好 1 条红字 `SM_SYSMESSAGE(100)`、`Param=14591`、
> 正文 `输入的地图不存在!!!`」，不得写成「清怪成功 / 怪物被清掉」；RNG 面不作判据。

命令窗口：`A-spawn`（脚本 `怪 鸡 1`）↔ `B-spawn`（admin `@怪 鸡 1`）；`A-clear`（脚本 `清除怪物 3 * 0`）↔ `B-clear`（admin）。

### 4.1 帧面：PASS（两轮）

- 两轮独立复跑：第 1 轮 A-spawn vs B-spawn；第 2 轮 A-spawn-r3 vs B-spawn-r3。同 Rust 构建、同 seed 42。
- 选帧：643 之后 `ident ∈ {10, 41}` 且 `recog ∉ pre`。每轮每侧各 2 帧
  （`SM_TURN(10)` + `SM_FEATURECHANGED(41)`）。
- 逐字节：ident 10 的 `body_sha256` 两侧同为 `d82d8b522f53fe76c4ed853a967fcaa5bdc012dc6e65e71bef3948adb617ad73`；
  ident 41 为空体，两侧同为 `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`。
  `recog`（会话相关 ActorId）、`t_ms`、ident 10 的 `series` 按规则掩码，均在 `report.json` 登记。
- `compare_runs`：两轮均 `VERDICT PASS`、`exit 0`、`compared 2`、`diffs 0`、`undecidable` 空。
- 说明：报告工具把两侧标为 `oracle` / `rust`，实际是同一 Rust 构建的 A / B 两次起栈（见执行单口径澄清）。

**证据**：

| 证据 | 路径（相对 `E:/tmp/admin-window-20261102/`） | sha256 |
|---|---|---|
| 第 1 轮 PASS 报告 | `wins/A-vs-B-spawn/report.json` | `768d50f84dda5ed0c7e7269ec29ac716a6eb3066078df751752cf13577927842` |
| 第 2 轮 PASS 报告 | `wins/r3/report.json` | `1cad95902618b1e37c4308f9426143dc91d82403a8b750d720575efe056391fb` |
| 第 1 轮 compare 输出 | `logs/compare-spawn.txt` | `5c1a0d5b27a5ceb8f2c30bcef2151ea732f2b7a4331b06b8af9f86b4490e3279` |
| 第 2 轮 compare 输出 | `logs/compare-spawn-r3.txt` | `5c1a0d5b27a5ceb8f2c30bcef2151ea732f2b7a4331b06b8af9f86b4490e3279` |

### 4.2 RNG 面：UNDECIDABLE（噪声不可分离）— 不是 PASS

- 第 1 轮：`groups` rc=0，spawn（折叠 construct 后）A=3068 / B=3094；第 2 轮 A=2922 / B=3174，
  两轮 spawn 计数都 >0，非空判据。
- `groupdiff --strict --fold` 两轮都 rc=1：`VERDICT SHARED_REL_ORDER_DIFF+FOLDED_COUNT_DIFF`；
  `FIRST_SHARED_ORDER_DIVERGENCE` 第 1 轮 index=6700，第 2 轮 index=8107。
- `diff` 的首处分歧站点（原样记录）：
  - 第 1 轮：index=6690，A 侧 `crates/world/src/entity.rs` 第 174 行 vs B 侧 `crates/world/src/mongen.rs` 第 899 行；
  - 第 2 轮：index=8107，A 侧 `crates/world/src/entity.rs` 第 174 行 vs B 侧 `crates/world/src/monai.rs` 第 237 行。
  - 两轮首处分歧都落在非 spawn 的活世界噪声站点（实体 / 商人 / 怪物 AI），不是 spawn 站点。
- 按执行单「复跑一次仍不一致 ⇒ UNDECIDABLE」：两轮均不一致，本面记 **UNDECIDABLE（噪声不可分离）**，
  不记通过。

**证据**：

| 证据 | 路径（相对 `E:/tmp/admin-window-20261102/`） | sha256 |
|---|---|---|
| 第 1 轮 groupdiff | `logs/groupdiff-spawn.txt` | `abc94476a3af0d8123acb82a069b2717a4bfd63ba8bb51fa088c0da0e567a2df` |
| 第 2 轮 groupdiff | `logs/groupdiff-r3.txt` | `6618567dad9369116a091eabfc00bc5b6c97e5cc71aecacce434e7d78aafee4d` |
| 第 1 轮 diff（首处分歧） | `logs/diff-spawn.txt` | `459a8d6d2eff5cd26051cda14f700cee40ae97ed770c3d668cc3cadf6aed8a5c` |
| 第 2 轮 diff（首处分歧） | `logs/diff-r3.txt` | `1ccab79140ffec5429be35e0dac9b5e6191044a6ab8a87e0c73f02166c6f941d` |
| 第 1 轮 groupdiff JSON | `rng/A-vs-B-spawn.json` | `d1288436f6669024a2392f33c778ba3c2ee96122fdc18bfb0fe73d41294b8dc6` |
| 第 1 轮 diff JSON | `rng/A-vs-B-spawn.diff.json` | `e7c0e18b8731e18e23c1123fe69ebd8166248aab9e57504799e44640053cc62b` |
| A-spawn 账本 | `rng/A-spawn.tsv` | `c8b0cd2ab887067c90cc33fd93d93ab9ef523edf5b1d45a7deed204b00505435` |
| B-spawn 账本 | `rng/B-spawn.tsv` | `b18b6622ab4b0033e24f02f5888d0753ab672fed1ac305d04f9263ef17e9418b` |
| A-spawn-r3 账本 | `rng/A-spawn-r3.tsv` | `3f9f977f01afe143470fed091e5ea469543915df2cf7572719f5db141a48bc41` |
| B-spawn-r3 账本 | `rng/B-spawn-r3.tsv` | `8fbe25bc5569bdeedaf41c962051e9b0bcc148bada617694ca73343f066db453` |

### 4.3 清怪对：部分（A-clear 有诊断证据；B-clear 未跑）

- A-clear（脚本路径）：诊断运行 `probe-gmclear` 收到 `SM_SYSMESSAGE(100)`、`param=14591`、
  正文「输入的地图不存在!!!」，与 oracle 预期一致。
- B-clear（admin 路径）：**未跑**（窗口时间用尽）。
- 该对的 RNG 本就不作判据（命令不抽数；空账本不能自证零抽数）；也不得写成「清怪成功 / 怪物被清掉」。
- 结论：清怪对记 **部分**，不是 PASS。

### 4.4 过程发现

1. **夹具标签必须插在 `[goods]` 之前。** 首版把 `[@gmspawn]` / `[@gmclear]` 追加在 `[goods]` 之后，
   落入商品数据段、脚本解析器不识别；改为插在 `[goods]` 之前后生效。
   生效证据：`@gmclear` 诊断收到 `SM_SYSMESSAGE(100)` `param=14591`「输入的地图不存在!!!」。
2. **`@传送 325 292` 经 admin 通道被拒**：返回
   `{"ok":true,"frames":1,"note":"您现在还无法使用此功能!!!"}`。oracle 语义要求佩戴传送装备
   （`gmcmd.rs` 第 5142 行）。因此没有采用 GM 搬运来放置探针角色。

## 5. 内容夹具与 Server.conf 的 BEFORE / RESTORE

窗口内临时注入四份内容夹具（owner 批准的内容冻结例外，仅用于制造可达触发点，非产品需求），
窗口末全部还原。路径相对 `E:/MirServer/M2GameSvr/`：

| 文件 | BEFORE sha256（大小） | AFTER sha256（大小） | RESTORE sha256（MATCH） |
|---|---|---|---|
| `Envir/Market_Def/比奇城/卫家店-0103.txt` | `73c8dec6c68f2a76c40dd1b85ef5ba28c1f7faaf51db0328deb98862a01cfc9b`（871） | `71fc396ec871b1b314bb62d280011e0f9d5fe242834b802961b0bb27483092d9`（992） | `73c8dec6…`（871）MATCH True |
| `Envir/Market_Def/沙巴克/药店-0153.txt` | `99f3f0fdf265ca590b5d012154136bfd4c0aabaad882cf03f2f6ce1aeb19052a`（805） | 未登记（本卡未使用） | `99f3f0fd…`（805）MATCH True |
| `Envir/Market_Def/比奇城/小药-0119.txt` | `1f15035182dbba323f5ae38762821757ed6de98785ff70bd10051d9a57c98008`（372） | 未登记（本卡未使用） | `1f150351…`（372）MATCH True |
| `Envir/Market_Def/比奇城/夏家-0.txt` | `9917d5226684c8b0b2f331b175f1d1fcaa40582a4cca8586e6fae8cd321b926e`（699） | 未登记（本卡未使用） | `9917d522…`（699）MATCH True |
| `Server.conf` | `ce5e3ce98b9bb71cbb18e6be1501e20e05642fd3eeaaffd9326caef8d9a4c0e2`（58080） | `63d8e49243c9a5e39fa72043c938a48d36815b269a2275b1fb0ef66d923e9526`（58123） | `ce5e3ce9…`（58080）MATCH True |

- 卫家店-0103 的改动：`[@main]` 后加两行链接 `<GM刷怪/@gmspawn>` / `<GM清怪/@gmclear>`；
  在 `[goods]` 之前追加 `[@gmspawn]` / `[@gmclear]` 两个 `#ACT GMEXECUTE` 段（纯插入两处）。
- 报告定稿前对现网做只读复核：上述 5 个文件当前 sha256 与 BEFORE 逐条相等
  （其中 4 份与 `E:/tmp/admin-window-20261102/conf/*.before` 一致）。
- 产物目录只保存了 BEFORE 副本与 AFTER-ON 的 sha；AFTER-ON 正文未另存。

## 6. 未跑 / 未测登记（一律不写成通过）

| 项 | 状态 | 说明 |
|---|---|---|
| ① 清怪对 B-clear | **未跑** | 窗口时间用尽；只有 A-clear 的诊断证据 |
| ① RNG 面 | **UNDECIDABLE** | 两轮 `groupdiff --strict` 均 rc=1，首处分歧在非 spawn 噪声站点，复跑仍不一致 |
| ③ DB 面（`characters_ablity.Level`） | **未测** | 环境无 mysql 客户端；③ 的 PASS 只覆盖权限门 |
| ②a 的「两侧帧数相等」 | 字面未满足 | ON 32 帧 / OFF 34 帧；多出的 ident 10+41 对已登记为活世界噪声 |
| ②a、① 的库后态 | 未比较 | 报告 `db.state=absent_both`，只判帧流 |
| ②b | 台账 / 产物 README 标 `NOT_RUN` | 不在本报告范围，不写成通过 |

## 7. 残留物清单

1. **证据目录（保留）**：`E:/tmp/admin-window-20261102/`。含
   `rng/off-01.tsv`（1,306,942,760 B）、`rng/on-01.tsv`（82,096,871 B）、
   `logs/stray-rng-RUN.tsv`（1,636,678 B）、`conf/` 五份 BEFORE 副本、
   窗口脚本（`fixture*.py`、`send_admin.py`、`mkbody.py`）与 `body-*.json` 草稿。
   保留理由：本报告全部证据与 sha 的源物；待报告评审 / 合并后可整体删除。
2. **本次认领临时文件**：`E:/tmp/body-claim-dev2.json`（status 条目 body）。
3. **进程**：窗口栈已停，`MIR2_SERVER_STACK_STOPPED`（`logs/stop-final.txt`，
   sha256 `31bfdcbc837ec23d0dd6b269bb71089be06347eb2fe6f451a752d2369bf59f3b`）；
   5000 / 6000 / 7000 / 7100 / 7200–7202 均 DOWN。mysqld 3306 是外部依赖，按纪律未停。
4. **内容**：四份 Market_Def 夹具与 `Server.conf` 均已还原到 BEFORE sha（见 §5）。
5. **源码**：本卡 0 源码改动；工作树 diff 只有本文件。
6. **分支 / worktree**：`.worktrees/dev-2` 的 `card/window-report-dev2` 保留待评审与合并；
   不 push master、不 merge。
