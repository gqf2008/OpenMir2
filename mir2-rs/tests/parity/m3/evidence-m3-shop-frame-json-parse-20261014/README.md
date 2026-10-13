# evidence-m3-shop-frame-json-parse-20261014

卡 `openmir2-m3-shop-fixture-botsrv`（⑤-6 NPC/商店动作序列夹具）的**残余 Minor 收口片**：
关闭 reviewer-6 在 approve（`reviewed_head c2af796c`）留下的非阻塞 **M-1**：

> **M-1**：`MShare.ClassifyShopFrames/ExtractShopPrice` 用 JSON 子串定位（`"sm":651` / `"a":`），
> 对字段重排/嵌套 JSON 脆，但 selftest 已钉当前形态。

**离线、零起栈、零端口、零 `.rs`**。tier：**L**（纯夹具/工具；不触 RNG 抽数序列/随机播种、
不触时序、不落库、不改战斗数值 ⇒ 等价 + 抽样冒烟即放行）。

## 为什么改（子串口径的具体误判面）

生产帧由 `RobotPlayer.ShopOnFrame` 恒定生成为扁平整数对象
`{"sm":N,"a":N,"b":N,"c":N,"d":N}`；旧消费侧却按**子串**找键，于是在下列输入上误判：

| 输入 | 旧（子串） | 新（顶层 JSON 键） |
| --- | --- | --- |
| `{"sm":651,"a":30,...}` | `"a":3` 命中 `"a":30` ⇒ `fail_nomoney`（**错**） | `a=30` ⇒ `fail_buy_reject` |
| `{"sm":6510,"a":3,...}` | `"sm":651` 前缀命中 ⇒ `fail_nomoney`（**错**） | `sm=6510` ⇒ `frames_unclassified` |
| `{"sm":648,"a":1,"x":{"a":3,"sm":651}}` | 内层 `"sm":651`/`"a":3` 命中 ⇒ `fail_nomoney`（**错**） | 顶层 `sm=648` ⇒ `ok` |
| `{ "sm" : 651 , "a" : 3 }` | 无 `"sm":651` 子串 ⇒ 跳过（**漏**） | `sm=651,a=3` ⇒ `fail_nomoney` |
| `{ "sm" : 647 , "a" : 99 }` | 无 `"sm":647` 子串 ⇒ `null`（**漏**） | ⇒ `99` |

## 逐条改法

- `src/BotSrv/MShare.cs`：新增纯函数 `TryGetFrameInt(frame, key, out int value)`（`System.Text.Json`
  顶层对象整数键；非 JSON/非对象/缺键/键值非整数 ⇒ false，**不抛**——`JsonException` 吞掉，
  与旧口径「不产生分类」一致）。`ClassifyShopFrames` / `ExtractShopPrice` 改走它，
  去掉全部 `"sm":NNN` / `"a":` 子串搜索。与 `DropExpSeqSelfCheck`/`QuestSeqSelfCheck` 同库口径。
- `src/BotSrv/ShopSeqSelfCheck.cs`：新增 7 条牙（**19 → 26**）：`branch-field-reordered`、
  `branch-a-not-prefix`、`branch-nested-key-ignored`、`branch-sm-prefix-not-match`、
  `branch-whitespace`、`price-field-reordered`、`price-whitespace`。其中 5 条在旧子串实现下必 RED。
- `mir2-rs/tests/parity/whitelist.md`：T-41 证据列追加本包指针；`-SelfCheckNoStack` 计数
  由过时的 **17/17** 订正为当前实读 **20/20**。

## 判据（L 级：等价 + 抽样冒烟）

| 项 | 读数 | 原始件 |
| --- | --- | --- |
| **改坏必红**（MShare 回退到 `origin/master` 子串实现，保留新牙） | `SHOPSEQ SELFTEST RED (5 failed)`：`branch-a-not-prefix`/`branch-nested-key-ignored`/`branch-sm-prefix-not-match`/`branch-whitespace`/`price-whitespace` | `01` |
| 还原后 | `SHOPSEQ SELFTEST GREEN`（26/26）rc=0 | `02` |
| 两侧 runner 无栈自检 | `run_npc_shop_pair.ps1 -SelfCheckNoStack` **20/20 rc=0**（含 `botsrv-shopseq-selftest` rc=0） | `10` |
| 比较器 | `npc_shop_diff.py selftest` **32/32 rc=0** | `11` |
| 十条护栏 | `selftest_gate.ps1` **SUMMARY total=65 failed=0** rc=0 | `12` |
| 夹具链深检门 | `FIXTURE_CHAIN GREEN runner_ok=10 prereq_or_skip=0 red=0（--require-all）` rc=0 | `13` |
| 四门禁 | fmt rc=0 / clippy rc=0(-D warnings) / **101 suites 1200 passed 0 failed 22 ignored** / msgcodegen **604** rc=0 | `20`–`23` |
| 边界 | `git diff --name-only origin/master` = `src/BotSrv/MShare.cs` + `src/BotSrv/ShopSeqSelfCheck.cs`（+ 未跟踪证据目录）；（`src/ ':!src/BotSrv'`、`sql`、`MirClient`、`crates/protocol`、`crates/storage`、`bins`）空；`*.rs=0` | `30` |
| 源码差量 | MShare.cs +74/−31、ShopSeqSelfCheck.cs +26 | `31` |

## not_verified

- 真机两侧同序列 `npc_shop_diff` diff=0（卡级 DoD②③）仍**未做**：等 ⑤-2/⑤-4/⑤-5 世界侧落地 +
  owner `-Go` 独占窗口。本片只硬化夹具消费侧的解析，不声明卡级 DoD 达成 ⇒ 卡保持 in-progress。
- 生产帧形状由 `RobotPlayer.ShopOnFrame` 固定 ⇒ 新旧实现在**实际帧上逐字节等价**（未做端到端真机验证，
  仅在 `--selftest-shopseq` 纯函数层验证 + `run_npc_shop_pair.ps1 -SelfCheckNoStack` 复跑）。

## 纪律 / 残留物

- 只在本 worktree（m3cs）写/提交/推；`CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target`（检出外）；
  **共享主检出未动**；`src/BotSrv/bin|obj` 为 gitignore 构建产物，不入库。
- **零起栈、零占端口**（九端口 5000/5100/5500/5600/5700/6000/7000/7100/7200 未起用）。
- 未 merge、未自签 review。
