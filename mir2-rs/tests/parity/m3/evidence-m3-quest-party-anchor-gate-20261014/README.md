# evidence-m3-quest-party-anchor-gate-20261014

M3 **⑥/⑦ 夹具场景锚门族覆盖面收口**片（离线、零起栈、零端口、零 `.rs`）。卡：
`openmir2-m3-fixture-chain-gate`（夹具链深检门；owner_hint = `openmir2-worker-5` / 车道 `m3cs`），
承接 ⑥-5 `openmir2-m3-quest-fixture-botsrv` 与 ⑦-5 `openmir2-m3-party-trade-fixture-botsrv`。

tier：**L**（纯夹具/工具/文档；不触 RNG 抽数序列、时序、持久化落库、战斗数值 ⇒ 等价 + 抽样冒烟即放行）。

## 背景（为什么现在做）

M3 十副动作序列夹具的场景锚门族是「谁被 reviewer 点名谁补」：
⑧-6 / ⑧-10 / ⑨-5 / ⑩-3 各有 `castle_oracle_anchor_gate.py` / `guild_castle_anchor_gate.py` /
`hero_pet_anchor_gate.py` / `selloff_anchor_gate.py`，且都已登记进 `fixture_chain_gate.py` 常跑；
**⑥（任务）/ ⑦（组队交易）两个场景件同样带 `oracle_anchors`，却从来没有行级门**。
⇒ ⑥/⑦ 的 oracle 锚可以静默漂移、复制错行号、甚至写**根本不在树里的文件基名**而不被发现。
本片把 ⑥/⑦ 补齐到与 ⑧/⑨/⑩ 同口径，锚门族由此覆盖 ⑥⑦⑧⑨⑩ 全部带锚的场景。

## 本片做了什么

### A. 新增两个场景锚门（与 ⑧/⑨/⑩ 四锚门同形）

* `quest_anchor_gate.py`（⑥）：**14 锚 / 20 行点**。
* `party_trade_anchor_gate.py`（⑦）：**22 锚 / 47 行点**。

每个门含：
* **覆盖面（双向）**：`oracle_anchors` 解析出的每个 `file:spec`（`.cs` 与 `.sql` 两族）必须在
  `REGISTERED` 表内（新锚不登记 ⇒ RED）；表内多出 / 场景重复引用 ⇒ RED。
* **读侧行级**：每条锚声明的每个 `(line, token)` 必须命中**冻结** `src/`（`src/M2Server/**`、
  `src/GameSrv/**`、`src/OpenMir2/**`、`src/Modules/**`、`src/Storeages/**`、`sql/**`）；
  token 取自冻结行本体。
* **缺陷本体 / 常量 token 锁死**：
  * ⑥ —— oracle「任务态不落库」的缺陷本体被 token 钉住：`characters_quest` 的
    `DROP TABLE`、`SaveQuest` 空壳、`characters` UPDATE 列清单无任务列、`GetChrRecord`
    只 `SELECT * FROM characters`；谁顺手「补全」任务落库 ⇒ RED（行为等价优先，另开 `S-*`）。
  * ⑦ —— 配置上限/时钟常量 `GroupMembersMax = 10`、`TryDealTime = 3000`、`DealOKTime = 1000`、
    `CanNotGetBackDeal = true`、`DisableDeal = false` 被 token 钉住 ⇒ 改数值 ⇒ RED。
* **token 强度**：每条锚整组至少要有一个有辨识度 token（含字母/数字/下划线/CJK）⇒ 防只钉空壳行。
* `check` / `selftest`（7 类牙齿）/ `mutation`（⑥ MUT1–MUT6；⑦ MUT1–MUT7，含范围漂移 / 假基名 /
  读侧 token 漂移 / 强度牙各必红 + 真身 check 必绿）。

### B. 订正 7 处场景锚（本片逐条对冻结 `src/` 复核的产物）

| 场景 | 锚 | 旧（漂移/歧义） | 新（订正） | 依据 |
| --- | --- | --- | --- | --- |
| ⑥ | `quest_bits` | `PlayObject.cs:3966-4110` | `:3966-4105` | 4110 已越到下一函数 `CheckWeaponUpgrade` 的签名；末个任务位函数 `SetQuestUnitStatus` 收在 4105 |
| ⑥ | `no_persist` | `Query.cs:192`/`:194` | `PlayDataStorage.Query.cs:192`/`:194` | 树里无 `Query.cs`；实为 `src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Query.cs` |
| ⑥ | `no_persist` | `Save.cs:493` | `PlayDataStorage.Save.cs:493` | 树里无 `Save.cs`；同一锚另一条本就写 `PlayDataStorage.Save.cs:45`（自相矛盾） |
| ⑦ | `group_membership` | `Base.cs:1339-1369` | `PlayObject.Base.cs:1339-1367` | 树里无 `Base.cs`；1369 已越到下一函数 `GetSpellPoint`；`SendGroupMembers` 收在 1367 |
| ⑦ | `deal_cancel` | `Base.cs:730-752`/`Base.cs:833-845` | `PlayObject.Base.cs:…` | 同上（基名补全；行号命中，未漂） |
| ⑦ | `deal_ops` | `PlayObject.Operate.cs:911-1203` | `:911-1202` | 1203 是函数后空行；`ClientDealEnd` 收在 1202 |
| ⑦ | `group_state`/`deal_state` | `PlayObject.cs:106 All/583 …`、`PlayObject.cs:114 DealCreat/118 …` | `PlayObject.cs:106/583/587`、`PlayObject.cs:90/114/118/122/126/130/203/739` | 原文是散文夹注，解析只认第一个引用 ⇒ 后 7 条从未被核对；规范化后逐条机判（行号全部命中，未漂） |

其余锚逐条复核后**本就正确**（⑥ 的 `Messages.cs:62`、`PlayObject.Base.cs:674`、`NormNpc.cs:68`、
`ScriptEngine.cs:987-998`、`PlayObject.Message.cs:1702-1724`、`WorldServer.cs:1160-1162`/`1291-1293`、
`PlayDataStorage.Save.cs:45`/`84-86`、`mir2_db.sql:371936`；⑦ 的 `IPlayerActor.cs:120/238/597/601`、
`PlayObject.Operate.cs:787-909`、`PlayObject.cs:5126-5183`、`PlayObject.Message.cs:914-935`/`946-960`/
`2138-2142`/`704-719`、`PlayObject.Base.cs` 余下两条、`WorldServer.cs:701`/`712`、`GameSvrConf.cs:1402`/
`1515`/`1516`/`1517`/`1518`、`ServerConf.cs:225/308-311`、`mir2_db.sql:56-57`）。

### C. 接入两处常跑面

* `fixture_chain_gate.py` 的 `SHARED_CHECKS` 新增 `quest_anchor_selftest` / `quest_anchor_check` /
  `party_trade_anchor_selftest` / `party_trade_anchor_check`（与 castle / guild / hero / selloff
  四族同口径），并加**防回退牙**「任务锚门已接入共享管线」「组队/交易锚门已接入共享管线」+
  `.py` 登记项下界 13→17（链门 selftest **37/37**）。
* `run_quest_pair.ps1` / `run_party_trade_pair.ps1` 的 `-SelfCheckNoStack` 各新增
  `anchor-gate-check` / `anchor-gate-selftest` / `anchor-gate-mutation-red` 三条（**19/19 → 22/22**），
  与 ⑧/⑨/⑩ 三族 runner 同形。

### D. 文档/登记

* `whitelist.md` 的 **T-42（⑥ 任务夹具）** 与 **T-43（⑦ 组队/交易夹具）** 证据列追加本证据包指针 +
  订正 `-SelfCheckNoStack` 计数 19/19→22/22。

## 读数（本片实跑，原始件见同目录）

| 项 | 读数 | 原始件 |
| --- | --- | --- |
| ⑥ 锚门 check | `GREEN: quest 场景锚门通过（14 锚 / 20 行点…）` rc=0 | `01` |
| ⑥ 锚门 selftest / mutation | `SELFTEST GREEN` / `MUTATION GREEN`（MUT1–MUT6）rc=0 | `01` |
| ⑦ 锚门 check | `GREEN: party/trade 场景锚门通过（22 锚 / 47 行点…）` rc=0 | `01` |
| ⑦ 锚门 selftest / mutation | `SELFTEST GREEN` / `MUTATION GREEN`（MUT1–MUT7）rc=0 | `01` |
| 修前 RED 复现 | 旧场景对新门：⑥ **RED 8 项** / ⑦ **RED 12 项** rc=1（漂移「未登记」+「表过期」） | `02` |
| 外部改坏必红 | ⑥ `:3966-4105`→`:3966-4110` ⇒ RED 2 项 rc=1，还原 ⇒ GREEN rc=0；⑦ 同形（`:1339-1367`→`:1339-1369`） | `02` |
| ⑥ 夹具 runner 无栈自检 | `SELFTEST ok=22/22`（含 3 条锚门）rc=0 | `03` |
| ⑦ 夹具 runner 无栈自检 | `SELFTEST ok=22/22`（含 3 条锚门）rc=0 | `04` |
| 夹具链深检门（runner 级） | `FIXTURE_CHAIN GREEN runner_ok=10 prereq_or_skip=0 red=0（--require-all）`（含 ⑥⑦ 四向 rc=0） | `05` |
| 链门 selftest | `SELFTEST GREEN: fixture_chain_gate selftest 37/37` | `06` |
| 四门禁 | fmt rc=0 / clippy rc=0(-D warnings) / 101 suites 1200 passed 0 failed 22 ignored / msgcodegen 604 rc=0 | `07`–`10` |
| 证据四件套 | `verify --check-extra` GREEN（仓库 LF/git-blob 口径）+ `eolcheck` GREEN | `SHA256SUMS.txt` |

## 边界

见 `11-boundary.txt`：base = `origin/master` `4244cb69503cbbccec8803f6332c49facd9861d3`
（含 ⑧-6 O-1 片 `5ffd215b` 的合入）；净差量全在 `mir2-rs/tests/parity/**`
（新 `m3/quest_anchor_gate.py` + `m3/party_trade_anchor_gate.py` + `m3/quest_scenario.json` +
`m3/party_trade_scenario.json` + `m3/fixture_chain_gate.py` + `m3/run_quest_pair.ps1` +
`m3/run_party_trade_pair.ps1` + `whitelist.md` + 本目录）；冻结面
`src sql MirClient mir2-rs/crates/protocol mir2-rs/crates/storage mir2-rs/bins` = **空**；`*.rs` 计数 = **0**。

## 残留（如实登记）

1. 锚门「行内容 ⇄ 散文声称构造」的**语义层**仍是抽样人核（可机判部分 = 覆盖面 + 行点 token + 强度 + 常量/缺陷本体）。
2. ⑥/⑦ 的 `*_frames` 两条锚是**纯帧常量清单**（无 `File.cs:NNN` 引用）⇒ 与 ⑧/⑨/⑩ 各门的散文锚同口径，
   本片不为其新增行号引用（若将来要机判帧常量，应单开片给 `Messages.cs` 行号锚并逐条核）。
3. ⑥/⑦-5 的**卡级真机 DoD**（两侧同序列 `diff=0`、`openmir2-m3-clock-injection` + owner 独占窗口）
   **不在本片范围**，两卡保持 `in-progress`。
4. `run_quest_pair.ps1` / `run_party_trade_pair.ps1` 的 `-SelfCheckNoStack` 计数从 19/19 变 22/22
   （T-42 / T-43 已同步）。
