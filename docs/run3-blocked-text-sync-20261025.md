# run3 blocked 文案与两来源同步 — 2026-10-25

- 卡：`openmir2-run3-blocked-text-sync`（tier **L**，owner `openmir2-dev-2`，branch `card/run3-blocked-text-sync`，base `2abc84efe`）。
- 定位：`blocked` 有**两个来源**——清单行 `tools/window/run3-scenarios.json` 的 `blocked`（管**批次排除**）与
  场景 JSON `tools/probe/scenarios/<name>.json` 的 `blocked`（管**直接跑 card_window** 的人工口径）。
  本卡做三件事：① 按 `docs/blocked-scenarios-audit-20261025.md` 的裁定改写 #21/#22/#24 的文案（**两处同步**）；
  ② 新增 `tools/window/blocked_sources_check.py` 把「两处一致」变成机判；③ 为 #24 预置 profile JSON（**不接进 run3、不清 blocked**）。
- 红线：不清任何 blocked（解封另行）、不改 `run_batch.sh` 执行逻辑、不改 `card_window.py`、不起服务端不占端口。

## 1. 三处文案：前 → 后

| 场景 | 改前（已被证伪） | 改后（audit 裁定） |
|---|---|---|
| #21 `butch_two_draws` | 「能力齐（活体夹具 + kill 步 + C# RNG 接线），**只差实测**」 | **真因=刷怪顺序**：夹具行被 `head + append` 落到 `MonGen.txt` **表尾**，`RegenMonstersTime=200ms` 且 `ProcessMonsterMultiThreadLimit` 未配=1 ⇒ 3081 行一轮 ≈ **616s** ≫ 本行 60s ⇒ `kill` 步稳定 `no target frames`。解封=**前置插入**（`openmir2-fixture-monsters-prepend`） |
| #22 `castle_merchant_tax` | 「买武器腿需 652 解码；**W-TAX-4 SaveTick 不落盘**」 | ① 买武器腿缺 **652 ClientItem buffer 解码**（`MakeIndex = GetItemNumber()` 运行时计数，645 不含）；② **「SaveTick/600s 门不可观测」已证伪**（SaveTick 只写时间戳；`ConsoleShowUserCountTime` 可被 profile 覆盖到 5s）；**真缺口=税入账原因未定位** |
| #24 `castle_merchant_tax_upgrade` | 「run3 内置 profile 不能声明 config_overrides ……用自定义 profile 可手工跑」 | **「不能进批」已证伪**（`extra` 拼在 `--profile` 之后 + `load_profile` 收 JSON 路径）；**真缺口=按名字/坐标选 NPC**（`openmir2-probe-npc-discovery`）。profile 预置已入库（本卡），未接进 run3 |

来源逐条：`docs/blocked-scenarios-audit-20261025.md` §1.2（616s 量化）、§2.2–§2.3（652/MakeIndex 与 SaveTick 证伪）、
§3.2–§3.3（进批通道已通、NPC 寻址仍缺）。**两处同步**：清单行是摘要（单行字符串）、场景 JSON 是 `{reason, missing_fixture}` 全文；
`butch_two_draws` / `castle_merchant_tax_upgrade` 的 `description` 里直接矛盾的尾句也一并订正。

**为什么不能靠加大 `--timeout-ms` 解封 #21**：616s 是**一轮**刷怪的下限，且它随 `MonGen.txt` 行数线性增长（3081 行 × 200ms）；
把 60s 调到 700s 只是把一个确定性失败换成 12 分钟的空等，并且让批次里一条注定不确定的场景长期占据窗口时间；
正确解是让夹具行**前置**到表头（`openmir2-fixture-monsters-prepend`），与「加大超时」是两类修法。

## 2. 两来源一致性机判

```sh
python -I -B tools/window/blocked_sources_check.py check
python -I -B tools/window/blocked_sources_check.py check --list-output E:/tmp/run3-list.txt
python -I -B tools/window/blocked_sources_check.py selftest
```

| 退出码 | 含义 |
|---:|---|
| 0 | 两来源一致（且两处 blocked 都带非空文案/reason） |
| 1 | 有漂移（点名场景：`DRIFT <name>：两来源不一致（清单行 blocked=真/假，场景 JSON blocked=真/假）`） |
| 2 | 用法错误 / 读不到文件 / 清单行的场景 JSON 不存在（fail-closed，不猜） |

- 判据：逐场景比 **blocked 真假**；两处都为真时还要求**清单行是非空字符串、场景 JSON 有非空 `reason`**（空文案也是漂移）。
- `--list-output`：先 `bash tools/window/run_batch.sh --list > <file>` 再喂进来，额外要求
  **`--list` 标 `YES` 的集合 == 场景 JSON 里 blocked 为真的集合**——这正是「批次排除只看清单行」那条链的闭环检查。
- 牙（`selftest` 8 项 + 真实文件重放）：
  只改清单行 ⇒ 红；只改场景 JSON ⇒ 红；两处都改 ⇒ 绿；另含「场景 JSON 缺 reason ⇒ 红」「缺场景 JSON ⇒ rc=2」
  「`--list` YES 集合与场景 JSON 不一致 ⇒ 红」。真实文件重放记录见 §5。

## 3. #24 的 profile 预置（**不接进 run3**）

- 文件：`tools/window/profiles/castle_tax_upgrade.json`（`format=card-window-profile/1`）。
  **是 `alt` 的静态副本** + 唯一差异 `fixtures.config_overrides.Server.conf.CastleGetAllNpcTax = 1`
  （默认 0=零税；键白名单见 `card_window.py` 的 `FIXTURE_CONFIG_KEYS`）。
- 重新生成（`alt` 改动后必须重跑，避免静默漂移）：

  ```sh
  python -I -B -c "import json,sys;sys.path.insert(0,'tools/conformance');import card_window as cw;
  p=json.loads(json.dumps(cw.BUILTIN_PROFILES['alt']));p['name']='castle_tax_upgrade';
  p['fixtures']={'config_overrides':{'Server.conf':{'CastleGetAllNpcTax':1}},'note':'…'};
  open('tools/window/profiles/castle_tax_upgrade.json','w',encoding='utf-8',newline='\n').write(json.dumps(p,ensure_ascii=False,indent=2)+'\n')"
  ```

- 只读验证（不起栈、不写文件）：用一个**可跑场景**（blocked 场景会被 card_window 硬门拒为 `VERDICT: BLOCKED`，属预期）：

  ```sh
  python -I -B tools/conformance/card_window.py \
      --profile tools/window/profiles/castle_tax_upgrade.json \
      --scenario tools/probe/scenarios/castle_merchant_tax_potion.json \
      --card openmir2-castle-merchant-tax-wiring --dry-run
  ```

  实测 rc=0，打印两侧 `夹具 baseline/Server.conf: - CastleGetAllNpcTax=0 → + CastleGetAllNpcTax=1`（shadow 同），
  且 dry-run 前后 `/e/tmp/r3-dry` 下**文件数 0**。

- **不接进 run3**：`run3-scenarios.json` 的 #24 行 `extra` 仍是空；接入需要**同一张卡同时**清掉两处 blocked
  （`blocked_sources_check` 会挡住单改）。真缺口（NPC 寻址）未解前接入 ⇒ 批次里多一条注定超时的场景。

## 4. 不变式（本卡只改文本/新增工具，行为不变）

| 项 | 实测 |
|---|---|
| `bash tools/window/run_batch.sh --list` | **场景数 24（blocked 3 ⇒ 入批 21）** |
| `blocked_sources_check check --list-output` | rc=0（`--list` YES 集合 == 场景 JSON blocked 集合） |
| `run_batch.sh --dry-run --no-rebuild --no-preflight` | rc=0，**21** 条 PLAN（blocked 3 条未入批） |
| `card_window.py --selftest` | **GREEN 382/382**（base `2abc84efe`；本卡不碰 `card_window.py`） |
| `blocked_sources_check.py selftest` | GREEN 8/8 |

> 注：`card_window.py --selftest` 的计数随 master 前进：本卡 base `2abc84efe` 上实测 **382/382**；
> `openmir2-fixture-monsters-prepend` 合入（`6f35b049f`）之后的 master 为 **396/396**（Leader 2026-10-25 更正）。
> 两个数都对，取决于 base；本卡不碰 `card_window.py`，故报告的是本 base 上的 382/382。

> `--dry-run` **带 preflight** 时本检出 rc=2：`binary_stamp`（release 二进制 stamp 指向 `7cf77712a9ac` ≠ HEAD）
> 与 `start_rust_exe`（`start-rust.ps1` 指向 `.worktrees/tester-1` 的 exe）两项失败——属台账 §5 已登记的
> 「preflight 由 run_batch 首步 rebuild 覆盖」类**环境项**，且当前 tester 私栈在跑、本卡禁止 rebuild/占端口，
> 故用 `--no-preflight` 验证计划面；两处失败与本次改动无关（失败项都不读 run3/scenarios）。

> `stale_pointer --ref master` 的既有红已由 Leader 在 `03f19c1f7` 收口（`docs/needs-human-20261025.md:68` 的 prepend 状态改为「已解（合入 `6f35b049f`）+ 真机证据」）；
> 本卡不修改该文件。本分支 base 仍是 `2abc84efe`（未 rebase），所以在**本分支树**上该门仍会红一次——
> 这是 base 落后于 master 的既有项，不是本卡引入的差异；合入后的 master 上 rc=0。

## 5. 验证记录

```sh
python -I -B tools/window/blocked_sources_check.py selftest        # GREEN 8/8
python -I -B tools/window/blocked_sources_check.py check           # OK（24 行，blocked 3 == 3）
bash tools/window/run_batch.sh --list > E:/tmp/run3-list.txt
python -I -B tools/window/blocked_sources_check.py check --list-output E:/tmp/run3-list.txt   # rc=0
```

真实文件重放（改完即从备份还原，`git status` 复核干净）：

| 操作 | 结果 |
|---|---|
| 只删清单行 #21 的 `blocked` | rc=1，`DRIFT butch_two_draws：两来源不一致（清单行 blocked=假，场景 JSON blocked=真）` |
| 只删场景 JSON 的 `blocked` | rc=1，`DRIFT butch_two_draws：…（清单行 blocked=真，场景 JSON blocked=假）` |
| 两处都删 | rc=0，无漂移 |
| 还原 | rc=0，无漂移 |

## 6. 残留物清单

- 入库：`tools/window/blocked_sources_check.py`、`tools/window/profiles/castle_tax_upgrade.json`、
  `tools/window/run3-scenarios.json`（3 行文案）、3 个场景 JSON（`blocked` + 两处 `description` 尾句）、本文件。
- 未入库（一次性/证据，路径登记）：`E:/tmp/r3_backup.json`、`E:/tmp/butch_backup.json`（重放牙时的原文备份）、
  `E:/tmp/r3_teeth.py`、`E:/tmp/r3_teeth2.py`、`E:/tmp/r3_texts.py`、`E:/tmp/run3-list.txt`、
  `E:/tmp/r3-batch{,.log}`、`E:/tmp/r3-dry{,-out.txt}`（dry-run 证据）。
- 未触碰：`run_batch.sh` 执行逻辑、`card_window.py`、`src/**`、mysqld；未起服务端、未占端口（tester 私栈在听）。
