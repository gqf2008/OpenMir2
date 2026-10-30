# 切流执行报告（2026-10-30）

- 卡：`openmir2-cutover-report-20261030`（tier=L，纯文档；报告本体，规约「面向人的报告作为文件进仓库」）
- 操作手册：`docs/cutover-runbook-20261029.md`（本报告引用它，不重复它；步骤编号 S0–S8/R1–R5/G0–G6 均按该手册口径）
- 素材：台账线程 `openmir2-real-machine-verification-master`（75 条签名条目，本报告引用条目 oid 前 8 位）；
  窗口证据根 `E:/tmp/window/cutover-20261029/`；登录链六卡与 world-join/dealtry/659 卡线程尾部结论。
- 脱敏口径：探针账号名 `winplay0` 可提，密码一律写 `***`。

---

## §1 结论

**切流完成。** 2026-10-30 10:24 台账登记 `CUTOVER-COMPLETE`（条目 `2e439135`）：现网十端口业务
（5000/5100/5500/5600/5700/6000/7000/7100/7200）由 C# 六角色切到单二进制 `mir2-server.exe` 的同六角色
（db-srv / login-svr / game-svr / login-gate / sel-gate / game-gate），`login_only` 真机 completed，
Rust 栈留生产；同日 17:33 S8 存档抽查 PASS（条目 `071aaee3`）后 **S1–S8 全段完成**。

- **生产 exe 基线**：master `d3a22e63`（全 oid `d3a22e6307fac975bb8e8587327d48eb778be1b8`，binary sha
  前缀 `7935692e`，台账 `2e439135`），含登录链全部六层修复与 world-join A2（三面 WARN-not-disconnect）。
- **master 演进与生产行为零漂移**：master 现已演进至 `eca375900`，`git diff --numstat d3a22e63..eca375900`
  全部改动 = `docs/gm-unimpl-inventory-20261030.md`、`mir2-rs/bins/game-svr/src/lib.rs`（+187/-1，全部在
  `mod tests` 内的测试牙）、`mir2-rs/crates/world/tests/party_vectors.rs`（+49，测试向量）、
  `tools/hygiene/tool_selftest_gate.py`（工具自检）——**零生产行为漂移，生产 exe 无需重建**
  （台账 `071aaee3` 对 `26f88f48` 已同口径登记，本次核验延伸至现 master）。
- **跨端确认**：MirClientZig（独立逐像素复刻客户端）真机全链验收 PASS——登录→选服→查号→选角→进图
  （SM_LOGON/SM_NEWMAP 真发）、NPC/商人窗/商品/购买（643/645/1014/650）、组队 659 首次真机收到、
  像素对拍三轮全 PASS（条目 `d25b5ad0`，判语「切流可宣布完成」）。
- **交接边界**：本报告是执行记录，不是验收书；未收口项见 §7。

---

## §2 登录链六层修复（每层一卡，全部 R 级、reviewer-22 approve）

| 层 | 卡 | 修复要点 | 复核 approve | 真机证据 |
|---|---|---|---|---|
| ① 绑定分离 | `openmir2-login-srv-bind-addr` | login-srv 按 5500=0.0.0.0（对客户端）/ 5600=127.0.0.1（对网关）分离绑定，不再绑广播地址 | `783e270a`（reviewer-22） | 演练窗日志行「监听 0.0.0.0:5500 + 监听 127.0.0.1:5600」（条目 `00ccdfa9` `bind_fix_verified`）；`E:/MirServer/logs/mir2-server/login-srv.out.log` |
| ② DSN 端点权威源 | `openmir2-login-srv-dsn-source` | host/port 取 `DBServer/dbsvr.conf` 权威源、database→mir2_account；凭据取 dbsvr.conf 全套（见 §4-3）；fail-soft 路径补底层错误链日志 | `753724e3` / `10c5ef39` / `4a62f305`（reviewer-22 ×3，对应 v1/v2/v3 三轮合并 `2a95a286`/`f2f955c9`/`b0861b2c`） | recut5 S4「连接SQL服务器成功 + 账号数据读取成功[7477]」（条目 `ef440c86`）；决定性实验见条目 `59e15ac6` |
| ③ 非法地址过滤器 | `openmir2-login-srv-illegal-addr-filter` | 白名单外地址（127.0.0.1 网关转发源）改 WARN 记录不断链，不再拒绝网关转发 | `f6464175`（reviewer-22） | recut2 S4「账号服务器链接成功、[登录] 帐号=winplay0 进 handlers」（条目 `9807cc4e`）；合并 `c61fa22d` |
| ④ 回帧链 | `openmir2-login-srv-reply-chain` | [登录] 处理后回帧链修复（此前处理后 20s 零回帧）；503 实达探针 | `ac3e3de9`（reviewer-22） | recut3 S4 探针收到 503「账号不存在」（条目 `668050b6`）；合并 `eb9e4d82` |
| ⑤ 选服路由按 ServerName | `openmir2-login-srv-select-route-match` | 服务器网关路由按 ServerName 匹配（对齐 AddrTable 语义），不再按 gateIP 字面硬配 | `a07c0544`（reviewer-22） | final-cut S7 530 选服 OK 22B 收到（条目 `69e3014a`）；合并 `f74975b6` |
| ⑥ 查号链 | `openmir2-selgate-dbsrv-query-chain` | CM_QUERYCHR 的 SelGate 转发 / 会话票校验 / DBSrv 查询链贯通到 520 回帧 | `e9737c14`（reviewer-22） | final-cut2 S7 里程碑：529→530→520 全链首次真机贯通（条目 `21294f00`）；完成切 `smoke-recut6-login`（520 24B）；合并 `d3a22e63` |

辅助设施：db-srv↔sel-gate 会话握手（白名单 WARN 不断链）在 ⑥ 开卡前已修通并真机验证
（条目 `69e3014a` infra 行：6000↔65510、5100↔65517 Established）；world-join A2
（准入/转发/查询三面 WARN-not-disconnect）随 `d3a22e63` 合并。

---

## §3 六次重切根因表（每剥一层洋葱）

首轮 RED 后的每次重切都按 runbook 同一序列执行（RED 即停 → R1–R3 回滚恢复 C# → 修复合并 → 重切）。

| # | 窗口（证据目录） | 症状 | 断点 | 根因 | 修复去向 |
|---|---|---|---|---|---|
| 首切 | 10-29 15:16–15:24（`snap-at-cutover`，回滚冒烟 `smoke-rollback-login` 59 帧 completed） | S4 RED | login-srv 启动失败 | 绑定广播地址失败（单地址绑两端口语义） | §2-① 卡；回滚恢复 C#（条目 `15959330`） |
| 演练 | 10-29 16:20–16:35（回滚冒烟 `smoke-rollback-login2` 59 帧） | ①修验证通过后暴露第二层 | login-srv 连 SQL 即 fail-closed 退出 | `config.conf` 失效 DSN（10.10.0.199/Mir2）被沿用 | §2-② 卡（派 worker-83）；条目 `00ccdfa9` |
| 重切1 | 17:52–18:05（`smoke-recut-login`，回滚 `smoke-rollback-login3` 62 帧） | login_only 仅 2 帧（503 不至） | login-srv 拒网关转发：「非法地址连接:127.0.0.1」 | 过滤器与同机部署 conf 耦合偏差 | §2-③ 卡；条目 `06763de9`；基线 `2a95a286` |
| 重切2 | 18:24–18:39（`smoke-recut2-login`，回滚 `smoke-rollback-login4` 63 帧） | [登录] 处理后 20s 无回帧（无 529/530） | 回帧断链（查询/校验/路由待定位） | 回帧链未接 | §2-④ 卡；条目 `9807cc4e`；基线 `c61fa22d` |
| 重切3 | 20:47–21:05（`smoke-recut3-login`，回滚 `smoke-rollback-login5` 69 帧） | 503 已实达但仍 [ERR] SQL 连接失败 | 权威源行正确（127.0.0.1:3306/mir2_account）但底层错误被吞 | fail-soft 只记 oracle 文本、不记底层 cause | §2-② 续作（补错误链日志+离线复现）；条目 `668050b6`；基线 `eb9e4d82` |
| 重切4 | 21:33–21:47（`smoke-recut4-login`，回滚 69 帧） | 仍 SQL 1045 using password: YES | 凭据面 | **决定性实验**：root/无口令 TCP=成功；root/123456（config.conf pwd）TCP=1045 ⇒ v2「凭据沿用 config.conf」把陈旧口令带进运行时 | §2-② v3：凭据源切 dbsvr.conf 全套；条目 `59e15ac6`；基线 `f2f955c9` |
| 重切5 | 21:52–22:05（`smoke-recut5-login`，回滚 `smoke-rollback-login6` 65 帧） | 529 首次收到（19B 服务器列表）但选服无 530；WRN「未匹配到服务器网关路由 gateIP=[127.0.0.1]」 | 选服路由匹配 | 路由匹配语义按 gateIP 硬配，不按 ServerName | §2-⑤ 卡；条目 `ef440c86`；基线 `b0861b2c`（DSN v3 真机验证：账号读取[7477]） |
| 重切6=final-cut | 10-30 00:37–00:55（`smoke-final-*` 三件，回滚 `smoke-rollback-final` 60 帧） | 529+530 都收到；CM_QUERYCHR 后 30s 无 520 | SelGate→DBSrv 查号链 | 转发/会话票/查询链断点 | §2-⑥ 卡（派 worker-84）；条目 `69e3014a`；基线 `f74975b6`（含握手修复） |
| 重切7=final-cut2 | 09:20–09:33（`smoke-final2-login` RED：exit=2 `SM_STARTPLAY port out of range '0'`；备份边界 `backup-final2` 水位 09:25:25；回滚 `smoke-rollback-login7` 58 帧） | 里程碑：529→530→520 全链首通；MirClientZig e2e 步骤 5–9 PASS；新阻断=进图 | SM_STARTPLAY 空（port=0） ⇒ 客户端拿不到游戏地址，世界会话 0 | worker-83 断点定性（`6f55ce28`）：`ServerInfo.txt` 的 sel_gate_ip=10.10.0.102 与同机连接源 127.0.0.1 不符 ⇒ db-srv `gate_route` 未命中 ⇒ 525 体编码恰 3 字节空路由；rust 侧 route 语义与 C# 同构（`GateRouteIp`），**修复=部署文件内容而非代码** | §4-2 部署修复（协调者批准内容冻结边界变更）；条目 `21294f00`；基线 `f74975b6`+AddrTable 已切 |
| 完成切（台账称第六次重切） | 10-30 10:24（冒烟 `smoke-recut6-login`：exit=0 completed，frames_total=65） | login_only 走完：529→530→520→525→SM_LOGON(50)→SM_ABILITY(52)→SM_NEWMAP(10) | — | 全部六层修复合并态（`d3a22e63`）+ §4 部署对齐 | `CUTOVER-COMPLETE`（条目 `2e439135`），栈留生产 |

> 引用勘误：条目 `2e439135` 的 evidence 指针写作 `smoke-final2-login`（65 帧 exit=0）；现盘该目录内容是
> 重切7 的 RED 跑（09:34，exit=2 port=0，4 帧），完成切冒烟现盘落在 `smoke-recut6-login`（10:19，
> exit=0，65 帧，529/530/520/525 齐）。以台账事件 + 现盘 `smoke-recut6-login` 为准。
> 另：11:05 `smoke-recut7-login`（66 帧 completed）为完成后的稳定性复测。

---

## §4 部署配置对齐（三处；备份与现值 sha256 已核）

生产根 `E:/MirServer` 内容冻结边界内的三项变更，均先备份后改；备份原件在窗口证据根。

| # | 文件 | 前 → 后 | 备份 sha256（前 8） | 现值 sha256（前 8） | 批准口径 |
|---|---|---|---|---|---|
| 1 | `LoginSrv/AddrTable.txt` | 每行 `热血传奇 Title1 10.10.0.102 10.10.0.102 10.10.0.102:7100`（两行，10.10.0.102 共 6 处）→ 同构换 `127.0.0.1`（6 处） | `8aa77811`（`addrtable-backup/AddrTable.txt.orig`） | `c104e949` | worker-83 提案 `6f55ce28`「需协调者批准，涉及内容冻结边界」→ 协调者批准并随重切7/完成切生效，台账 `21294f00`/`2e439135` 登记 |
| 2 | `DBServer/ServerInfo.txt` | `10.10.0.102 10.10.0.102 7200` → `127.0.0.1 127.0.0.1 7200` | `72602835`（`serverinfo-backup/ServerInfo.txt.orig`） | `7696ffb8` | 同上（`6f55ce28` `fix_proposal`；db-srv 重启后生效） |
| 3 | `LoginSrv/config.conf` 凭据（文件不改，改 login-srv 的凭据来源） | login-srv 运行时凭据沿用 config.conf（pwd=123456，陈旧）→ 改从 `DBServer/dbsvr.conf` 全套凭据、仅覆写 database→mir2_account | 不适用（代码+读取源变更；config.conf 原文件未动） | 不适用 | 重切4 决定性实验钉死陈旧口令（实验 A：root/无口令 TCP 成功；实验 C：pwd=123456 ⇒ 1045，离线测试 MUT-C 固化为红牙）；§2-② v3 合并 `b0861b2c` |

> 服务器名 `热血传奇`、对外端口、客户端（Delphi mirbeta）保持不变——对客户端透明的红线未被触碰
> （`127.0.0.1` 生效的前提是同机部署；生产多机化时的还原见 §7-1）。

---

## §5 S1–S8 时间线（步骤口径按 runbook；退出码取自台账与现盘 run.json）

- **G0–G6 前置**：G0 conf 映射漂移已解除（runbook §2.G0，选项 A 合并 `285a3540`）；G1 交付二进制按
  build stamp head+sha256 三道核验（首轮 stamp 见 `E:/tmp/window/cutover-20261029/g1-stamp.txt`：
  head=`8b390bd8`，sha256=`4bd4e776…`，2026-10-29T15:12:55+08）；G3 端口基线
  `port-baseline-before.txt`（十端口基线，2026-10-29 15:13）；G4 水位/快照/Castle sha
  （`watermark-lastupdate.txt`：7338 行；`castle-sha.txt`：List.txt=`9b38d284…`）；G5 探针账号 winplay0
  在册核验（`g5-winplay0-account.txt` / `g5-winplay0-characters.txt`，密码 `***`）。
- **S1 停 C# 六角色**：GameSrv CTRL_C 优雅停不可用（stop-csharp-graceful 超时，runbook §3.S1 已知退化
  路径）⇒ 各窗口均按退化路径强停并**登记「未走优雅停」**；其余五角色逐进程停；停后九端口 DOWN、
  3306 全程不动。
- **S2 存档落库确认**：强停下水位不上抬，照实记录；各备份边界水位 = 17:25:08 / 18:23:50 / 20:49:46 /
  21:36:50 / 21:58:03 / 00:38:49 / 09:25:25（backup-final2，完成切与生产态的回滚数据基线）。
- **S3 正式备份**：每窗口 prod-main + 分片×2（`Dump completed`=1）+ dbsnap 快照
  （`snap-at-recut*`、`snap-at-final`、`snap-at-final2`、完成切边界 `snap-at-cutover` 为首轮留档）。
- **S4/S5 起栈与端口复验**：`start_mir2_server.ps1` 六角色 fail-closed；各轮 RED 点见 §3，S5 端口
  与基线逐端口一致（无端口面 RED）。
- **S7 冒烟**：probe 一次一场景；RED 即回滚——R1 `stop_mir2_server.ps1`（`MIR2_SERVER_STACK_STOPPED`）
  → R2 dbsnap 判读（首轮各窗口 Rust 侧零业务写入或仅存档族，未触发回灌）→ R3 `start-all.ps1` 恢复 C#
  → 十端口回齐 → R4 login_only completed（回滚冒烟帧数序列：首切与演练各 59 / 62 / 63 / 69 / 69 /
  65 / 60 / 58，目录 `smoke-rollback-*`，exit 全部=0）。
- **完成切 S7**：`smoke-recut6-login` exit=0 completed（65 帧）；11:05 `smoke-recut7-login` 复测 66 帧
  completed；17:1x recut8 三件套复测见 §6。
- **S8 观察段**：
  - Castle 600s 重写：内容不变（`castle-sha.txt` `9b38d284…` 与 runbook 判据一致，按内容不比 mtime）。
  - **存档抽查 PASS**（条目 `071aaee3`）：`dbsnap diff snap-at-cutover vs snap-post` 差异面 =
    存档族 6 表（account / characters / characters_ablity / characters_bagitem / characters_indexes /
    characters_magic），**无存档族外写库，不触发回滚**。
  - 差异归因：admin 账号 14:17(+0800) 经登录链真客户端登录（`E:/MirServer/logs/mir2-server/login-srv.out.log`
    `[登录] 帐号=[admin] gateIP=[127.0.0.1] routeCount=2`，另有 03:06/06:17 UTC 两次）；gm01 进图存档
    （挪位 324,327→289,618、bagitem 购买占用、ability 数值增长、magic 2268→2298）——真客户端游玩数据；
    probe winplay0 的 recut8 三件套（17:1x）写库同属存档族。
  - 日志巡检：各角色 err 日志无持续刷屏异常（`E:/MirServer/logs/mir2-server/`）。

---

## §6 切流后观测（recut8 批次，2026-10-30 17:1x；台账 `29917acf`/`96e1d334`/`2d885edb`/`152a96e2`）

冒烟三件套（`E:/tmp/window/cutover-20261029/smoke-recut8-*`，ServerInfo 部署修复生效后栈重启）：

| 场景 | run.json | 帧面（frames.ndjson） |
|---|---|---|
| `smoke-recut8-login` | exit=0 completed，frames_total=67 | 529（19B 服务器列表）/530（22B）/520（24B）/525 各 1 → SM_LOGON(50)、SM_ABILITY(52)、SM_NEWMAP(10)×15 |
| `smoke-recut8-walk` | exit=1（steps[1] 3s 空等待超时；台账按帧面判读） | GD 族移动回执 11×5 + 14×1 在场 |
| `smoke-recut8-merchant` | exit=0 completed，frames_total=43 | 商人对话 643×2 → 645×1（探针角色 placement 钉 0103(19,19)，discovery merchant_actor=22）；另 `smoke-recut8-npcdisc` exit=0（41 帧，643×1） |

台账 V-* 三项状态（写「调查中」的不下结论）：

- **V-querybagitems-81：CONFIRMED。** probe 观测（`smoke-81-obs`，exit=1=预期：等待新 201 超时）：
  发 CM_QUERYBAGITEMS(81) 后 4s 窗口无新 201（唯一 201 为登录自动下发）——首查空表不回 ✓；MirClientZig
  独立互证（空包 81 零回包、30s 节流、铁器店 goods 全有货）；代码面由 81 卡 A2 牙覆盖
  （`openmir2-querybagitems-81-throttle`，牙已合并 `20fd3bd7`）。
- **V-dealtry-e2e：调查中**（FAIL→调查卡 `openmir2-dealtry-empty-name-orientation`）。MirClientZig
  相邻双端 CM_DEALTRY(1025，空名) 零回音；Leader 已核 rust `pose_create` 与 oracle `GetPoseCreate`
  同构（实时朝向+前排格）、oracle 无相邻扫描分支（前排格无人时同样零回音），「补相邻扫描分支」的
  建议不采纳；本卡转朝向证据 + actor_dir 链定性（dealtry 卡 `openmir2-dealtry-adjacent-resolution`
  本体已按新规收口，条目 `8406656e`/`077e4aa7`）。
- **V-groupmode-659-g2：调查中**（PARTIAL→调查卡 `openmir2-groupmode-659-party-path`）。probe 单连接
  双态必回已证（`smoke-659-obs`，exit=0 completed：开组 659 Param=1 @2310ms、关组 659 Param=0 @2838ms，
  两态各回恰好一条，与 master 牙 `party_cm_groupmode_close_replies_659_zero` 语义一致）；服务端侧
  零回音疑云排除；MirClientZig 双端组队多轮到达率 2/6，缺口在有组路径/队列去重/场景朝向之一。

---

## §7 残余项

1. **AddrTable.txt 生产切流还原（owner 待办）**：现值 `127.0.0.1`×6（sha `c104e949`）是同机切流的临时
   路由；生产多机部署时按 owner 指示还原 LAN 地址 `10.10.0.102`×6（备份原件
   `E:/tmp/window/cutover-20261029/addrtable-backup/AddrTable.txt.orig`，sha `8aa77811`，可直接回写）。
   `DBServer/ServerInfo.txt` 同理保留备份（`serverinfo-backup/ServerInfo.txt.orig`，sha `72602835`）。
2. **探针角色 placement 还原（owner 待办）**：winplay0（密码 `***`）现钉 0103(19,19)，原始 placement
   为 0/287/616；商人对话冒烟依赖该钉位，多机还原或冒烟退役后一并还原。
3. **台账 V-* 后续**：V-dealtry-e2e、V-groupmode-659-g2 各自的调查卡（§6）待认领深挖；
   V-querybagitems-81 已 CONFIRMED 无余项。
4. **台账登记的其它后续**：SelGate↔DBSrv >10 分钟稳定性观测（握手 A3 后续，条目 `2e439135` remaining）；
   商人 [goods] 补货配置（Envir 域内容红线，owner 裁决项，条目 `d25b5ad0` remaining）。

---

## 附：证据与残留物清单

- 台账：线程 `openmir2-real-machine-verification-master`（引用条目 oid：`2e439135` `071aaee3` `29917acf`
  `96e1d334` `2d885edb` `152a96e2` `d25b5ad0` `15959330` `00ccdfa9` `06763de9` `9807cc4e` `668050b6`
  `59e15ac6` `ef440c86` `69e3014a` `21294f00`）；登录链六卡与 `openmir2-game-svr-world-join`
  （closed，`0de4b76a`）、`openmir2-dealtry-adjacent-resolution`（closed，`077e4aa7`）线程。
- 窗口证据根：`E:/tmp/window/cutover-20261029/`（备份 `backup*`、快照 `snap-*`、冒烟 `smoke-*`、
  部署备份 `addrtable-backup/` `serverinfo-backup/`、基线抓取 `port-baseline-before.txt`
  `castle-sha.txt` `watermark-lastupdate.txt` `g1-stamp.txt` `precheck-sha256.txt`）。
  为窗口回滚边界与 §3/§4 引用的第一现场，**建议保留**（属登记残留物，非本线专属临时物）。
- 运行日志：`E:/MirServer/logs/mir2-server/`（生产现役，不入仓）。
- 本报告写集：仅 `docs/cutover-report-20261030.md`。
