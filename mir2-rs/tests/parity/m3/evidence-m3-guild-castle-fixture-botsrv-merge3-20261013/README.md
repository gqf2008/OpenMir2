# M3⑧-10 公会/攻城夹具 —— 解 10 处冲突重交（merge master）证据包（merge3）

- 卡：`openmir2-m3-guild-castle-fixture-botsrv`（父卡 `openmir2-m3-guild-castle-align`；白名单 T-46）
- 车道：`openmir2-worker-5` / worktree `m3cs` / branch `worktree-m3-guild-castle-fixture-botsrv`
- base：`origin/master 8aebbcb850d5b084af56e6cb8dd5e03ecc3c1625`（交付时 master）
- head：`a7ebe57caa11f58c38537e19fd7cd8ac44b85393`
- 触发：合并检查点 comment（`skip=merge-conflict`）：`git merge-tree master(ab60452d0) x 6203b6888` rc=1，
  10 处 CONFLICT；required = 作者 rebase/merge 到 master 解冲突后重交（head 变化须重新非作者 review）。

## 本片做了什么

1. **解 10 处冲突**：master 的 **⑨ HeroPet 夹具** 与本片 **⑧ GuildCastle 夹具** 是「同一插入位的平行新增」，
   故全按**并集**解、零内容删除：
   - `mir2-rs/tests/parity/m3/gate_guards.ps1`：GuildCastle scope 函数族 + HeroPet scope 函数族并存；
   - `mir2-rs/tests/parity/m3/run_pair.ps1`：参数 / `planArgs` / 会话字段 / `-or` 条件链 / ValidateSet / assemble 块全并集；
   - `mir2-rs/tests/parity/whitelist.md`：T-47（⑤）在前、T-46（⑧）与 T-45（⑨）两段并存；
   - `src/BotSrv/{MShare,AppService,Program,RobotOptions,Player/RobotPlayer,Player/RobotPlayer.Operate}.cs`：两族区块并存；
   - `tools/botload/run_bots.ps1`：ValidateSet 并集（`guildcastle`+`heropet`）、计数/条件/末尾 GREEN 行并集。
2. **⑧/⑨ markers 写侧对齐「保前缀」口径**（reviewer-6 O-2；卡 `openmir2-m3-fixture-marker-prefix-bug` 同族）：
   `run_pair.ps1` 的 ⑧ `M3GUILDCSEQ` / ⑨ `M3HEROPETSEQ` 两段原本**去前缀**写 `*_markers.jsonl`（裸 JSON），
   而 `guild_castle_diff.py` / `hero_pet_diff.py` 的 `_marker_events` 按 `line.find('<M3*SEQ> ')` 扫描 ⇒
   `assemble` 恒 `PREREQ_RED`。改为保留 bot.log 带前缀原始行 + 接生产 fail-closed 牙 `Assert-M3MarkerPrefix`
   （与 shop/quest/pairtrade/selloff 一致）；`marker_prefix_gate.py` 的 `COMPARATORS` 纳入这两个比较器
   （该门注释本就预留此扩展）。
3. 在合并树上**重跑本卡全部自检 + 四门禁 + 两族自检**。

## 判据与读数

| 判据 | 命令 | 读数 | 原始件 |
| --- | --- | --- | --- |
| ⑧ 比较器 selftest | `guild_castle_diff.py selftest` | **36/36** rc=0 | `02-*.txt` |
| ⑨ 比较器 selftest | `hero_pet_diff.py selftest` | **36/36** rc=0 | `03-*.txt` |
| markers 前缀口径门 | `marker_prefix_gate.py check` | **GREEN** rc=0（6 生产牙 + 6 比较器；check 前 master 对 ⑨ 恒 RED） | `04-*.txt` |
| 同上 selftest | `marker_prefix_gate.py selftest` | **17/17** rc=0 | `05-*.txt` |
| 同上改坏必红 | `marker_prefix_gate.py mutation` | **MUTATION GREEN** rc=0 | `06-*.txt` |
| ⑧ 入口无栈自检 | `run_guild_castle_pair.ps1 -SelfCheckNoStack` | **19/19** rc=0 | `07-*.txt` |
| ⑨ 入口无栈自检 | `run_hero_pet_pair.ps1 -SelfCheckNoStack` | **19/19** rc=0 | `08-*.txt` |
| run_bots 收尾计数 | `run_bots.ps1 -SelfCheckNoStack` | **GREEN** rc=0 | `09-*.txt` |
| ⑧ BotSrv 生产函数 | `BotSrv.exe --selftest-guildcastle` | **GREEN** rc=0 | `11-*.txt` |
| ⑨ BotSrv 生产函数 | `BotSrv.exe --selftest-heropet` | **GREEN** rc=0 | `12-*.txt` |
| 四门禁 | fmt / clippy / workspace test / msgcodegen | fmt0 / clippy0 / **98 suites 1128 passed 0 failed 21 ignored** / **604** | `13..16-gate-*.txt` |
| 边界（冻结面） | `git diff --name-only master HEAD -- src/sql src/MirClient` | **空** | `17-boundary.txt` |
| 边界（宿主面） | `-- mir2-rs/crates mir2-rs/bins mir2-rs/protocol` | **空** | `17-boundary.txt` |
| 证据 hash | `verify_sha256sums.py --check-extra` / `eolcheck` | **GREEN**（仓库 LF/git-blob 口径） | `SHA256SUMS.txt` |

## 冲突解原文要点

- `gate_guards.ps1` / `run_pair.ps1` / `src/BotSrv/**`：两侧插入的是**同名不同族**的平行块，逐块并集。
  `run_pair.ps1` 的 `-or` 条件链解为**单行含两族开关**，例如
  `if ($BagEquipSequence -or $AttackSequence -or $ShopSequence -or $GuildCastleSequence -or $HeroPetSequence -or $QuestSequence -or $PairTradeSequence -or $SellOffSequence) {`；
  `run_bots.ps1` 的 `ValidateSet` 解为
  `('bag', 'attack', 'shop', 'quest', 'pairtrade', 'selloff', 'guildcastle', 'heropet')`。
- `whitelist.md`：`T-47` 段与 `T-45` + `T-46` 段并存。

## 未做 / 承接（不变）

- 真机两侧 `run_guild_castle_pair.ps1` diff=0 要 ⑧-3..⑧-9 实现 + `openmir2-m3-clock-injection` + 独占窗口。
- 本片**不 merge、不自签 review**；零起栈、零占端口。

## 残留物清单

- 本目录（证据）为唯一新增残留；无临时库/进程/端口占用（`git status` 除本目录与本片源码外干净）。
