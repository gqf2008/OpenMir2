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
| #21 `butch_two_draws` | 「能力齐（活体夹具 + kill 步 + C# RNG 接线），只缺一次实操」 | **真因=刷怪顺序**：夹具行被 `head + append` 落到 `MonGen.txt` **表尾**，`RegenMonstersTime=200ms` 且 `ProcessMonsterMultiThreadLimit` 未配=1 ⇒ 3081 行一轮 ≈ **616s** ≫ 本行 60s ⇒ `kill` 步稳定 `no target frames`。解封=**前置插入**（`openmir2-fixture-monsters-prepend`） |
| #22 `castle_merchant_tax` | 「买武器腿需 652 解码；**W-TAX-4 SaveTick 不落盘**」 | ① 买武器腿缺 **652 ClientItem buffer 解码**（`MakeIndex = GetItemNumber()` 运行时计数，645 不含）；② **「SaveTick/600s 门不可观测」已证伪**（SaveTick 只写时间戳；`ConsoleShowUserCountTime` 可被 profile 覆盖到 5s）；**真缺口=税入账原因未定位** |
| #24 `castle_merchant_tax_upgrade` | 「run3 内置 profile 不能声明 config_overrides ……用自定义 profile 可手工跑」 | **「不能进批」已证伪**（`extra` 拼在 `--profile` 之后 + `load_profile` 收 JSON 路径）；剩余缺口 = NPC 寻址的**对象类别校验**与 `SM_DISAPPEAR` 剔除（后续卡 `openmir2-probe-npc-addressing-followups`，未合）。profile 预置已入库（本卡），**尚未接入 run3** |

NPC 寻址能力本身已合入（`openmir2-probe-npc-discovery` @ `ee1a0a18f`）；#24 剩余动作 = 另一张卡清两处 blocked + 接 `extra`。

来源逐条：`docs/blocked-scenarios-audit-20261025.md` §1.2（616s 量化）、§2.2–§2.3（652/MakeIndex 与 SaveTick 证伪）、
§3.2–§3.3（进批通道已通、NPC 寻址仍缺）。**两处同步**：清单行是摘要（单行字符串）、场景 JSON 是 `{reason, missing_fixture}` 全文；
`butch_two_draws` / `castle_merchant_tax_upgrade` 的 `description` 里直接矛盾的尾句也一并订正。

**为什么不能靠加大 `--timeout-ms` 解封 #21**：616s 是**一轮**刷怪的下限，且它随 `MonGen.txt` 行数线性增长（3081 行 × 200ms）；
把 60s 调到 700s 只是把一个确定性失败换成 12 分钟的空等，并且让批次里一条注定不确定的场景长期占据窗口时间；
正确解是让夹具行**前置**到表头（`openmir2-fixture-monsters-prepend`），与「加大超时」是两类修法。

## 2. 两来源一致性机判（断言 token 表）

```sh
python -I -B tools/window/blocked_sources_check.py check
python -I -B tools/window/blocked_sources_check.py check --list-output E:/tmp/run3-list.txt
python -I -B tools/window/blocked_sources_check.py selftest
```

**为什么不是「只比真假」**（reviewer-20 F1）：两处文案本来就是两套措辞，只改内容不改真/假时，布尔判据永远绿
⇒ audit 口径静默漂移。本工具在 `blocked_sources_check.BLOCKED_TOKENS` 里维护一张**断言 token 表**：

| 场景 | 两侧都必须出现的 token | source 锚 |
|---|---|---|
| `butch_two_draws` | `3081`、`616`、`ProcessMonsterMultiThreadLimit` | `docs/blocked-scenarios-audit-20261025.md` §1.2 |
| `castle_merchant_tax` | `652`、`原因未定位`、`已证伪` | 同上 §2.2/§2.3 |
| `castle_merchant_tax_upgrade` | `已证伪`、`NPC` | 同上 §3.2/§3.3 |

判据（确定性、无阈值、不要求逐字）：

1. 逐场景比 **blocked 真假**（清单行 vs 场景 JSON）；
2. 两处都为真时：清单行必须是非空字符串、场景 JSON 必须有非空 `reason`；
3. **该场景的每个 token 必须同时出现在两处文案里**（缺一处 ⇒ 红；错误串点名哪一侧、哪个 token、表项的 source 锚）；
4. **表也要被核**：表项缺 `source` ⇒ rc=2；表与文案一起漂（token 在两侧都不再出现）⇒ 红（自一致性）；
5. **fail-closed**：出现未登记的 blocked 场景 ⇒ **rc=2 并点名**「该场景没有断言表条目」；
6. `--list-output`：`run_batch.sh --list > <file>` 的 **YES 集合 == 场景 JSON blocked 集合**。

| 退出码 | 含义 |
|---:|---|
| 0 | 两来源一致（真假相同 + token 两侧齐 + `--list` 集合一致） |
| 1 | 有漂移（点名场景：真假不一致 / 文案缺 token / 缺 reason / `--list` 集合不符） |
| 2 | 用法错误 / 文件读不了 / 场景 JSON 缺失 / **blocked 场景无断言表条目**（fail-closed） |

## 3. #24 的 profile 预置（**尚未接入 run3**）

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
| `card_window.py --selftest` | **GREEN 396/396**（本卡 base = master `ee1a0a18f`，已含 `prepend`；本卡不碰 `card_window.py`） |
| `blocked_sources_check.py selftest` | GREEN 11/11 |

> 注：计数随 master 前进——初版 base `2abc84efe` 上是 **382/382**；`openmir2-fixture-monsters-prepend` 合入
> （`6f35b049f`）之后是 **396/396**。本卡返工时已 rebase 到 master `ee1a0a18f`，故报 **396/396**。

> `--dry-run` **带 preflight** 时本检出 rc=2：`binary_stamp`（release 二进制 stamp 指向 `7cf77712a9ac` ≠ HEAD）
> 与 `start_rust_exe`（`start-rust.ps1` 指向 `.worktrees/tester-1` 的 exe）两项失败——属台账 §5 已登记的
> 「preflight 由 run_batch 首步 rebuild 覆盖」类**环境项**，且当前 tester 私栈在跑、本卡禁止 rebuild/占端口，
> 故用 `--no-preflight` 验证计划面；两处失败与本次改动无关（失败项都不读 run3/scenarios）。

> `stale_pointer --ref master` 在初版 base 上的既有红（`docs/needs-human-20261025.md:68`）已由 Leader 在 `03f19c1f7` 收口；
> 本卡返工时 rebase 到 master `ee1a0a18f`，该处随之消失。另一处曾被本卡引入的红（把已合卡的 `未接进` 与卡号写在同一行）
> 已按 reviewer-21 F1 修法处理：卡名移到不含待定词的独立行。

## 5. 验证记录

```sh
python -I -B tools/window/blocked_sources_check.py selftest        # GREEN 11/11
python -I -B tools/window/blocked_sources_check.py check           # OK（24 行，blocked 3 == 3，token 两侧齐）
bash tools/window/run_batch.sh --list > E:/tmp/run3-list.txt
python -I -B tools/window/blocked_sources_check.py check --list-output E:/tmp/run3-list.txt   # rc=0
```

真实文件重放（改完即从备份还原，`git status` 复核干净）：

| 操作 | 结果 |
|---|---|
| **两处都 blocked，只把场景 JSON 文案换成完全不同内容**（reviewer-20 的复现） | rc=1，`DRIFT butch_two_draws：断言 token '3081' 未出现在 场景 JSON（断言表 source=…§1.2…）` |
| 只删清单行 #21 的 `blocked` | rc=1，`DRIFT …：两来源不一致（清单行 blocked=假，场景 JSON blocked=真）` |
| 只删场景 JSON 的 `blocked` | rc=1，`DRIFT …：（清单行 blocked=真，场景 JSON blocked=假）` |
| 两处都删 | rc=0，无漂移 |
| 新增 blocked 场景但不登记表项（临时副本） | **rc=2**，点名 `zz_fake：…没有断言表条目（fail-closed…）` |
| 篡改表（加一个文案里不存在的 token `NOPE_TOKEN`） | rc=1，`断言 token 'NOPE_TOKEN' 未出现在 清单行+场景 JSON` |
| 还原 | rc=0，无漂移 |

## 6. 复审修复记录（reviewer-20 `c03a51cb` + reviewer-21 `d1997f47`）

| 项 | 处置 |
|---|---|
| **F1 设计**（假绿） | 弃用「只比真假」；改为**断言 token 表**（§2）：每个 blocked 场景的一组 token 必须同时出现在两处文案里，表项自带 audit source 锚；新增「表 token 与文案不一致 ⇒ 红」「未登记 blocked 场景 ⇒ rc=2」；牙 a–f 全部可复现红（含 reviewer-20 的原复现） |
| **F1 门禁**（会把 master 打红） | `stale_pointer_gate` 的 `CARD_PENDING_WORDS` 含「未接」，且判定看**同一行**：本卡把「未接进 run3」与已 closed 的 `openmir2-probe-npc-discovery` 写在同一行 ⇒ 多 1 hit。修法：**把卡名移出该行**，并改成事实（NPC 寻址已合 `ee1a0a18f`；#24 剩余动作=另一张卡清两处 blocked + 接 extra）；「尚未接入 run3」留在不含已合卡名的行 |
| **F2**（过期措辞） | #24 文案不再写「按名字/坐标选 NPC 是缺口」（已合），改为「对象类别校验 + DISAPPEAR 剔除（后续卡）」；`尚未接入 run3` 替换 `未接进 run3` |

## 7. 残留物清单

- 入库：`tools/window/blocked_sources_check.py`、`tools/window/profiles/castle_tax_upgrade.json`、
  `tools/window/run3-scenarios.json`（3 行文案）、3 个场景 JSON（`blocked` + 两处 `description` 尾句）、本文件。
- 未入库（一次性/证据，路径登记）：`E:/tmp/r3_backup.json`、`E:/tmp/butch_backup.json`（重放牙时的原文备份）、
  `E:/tmp/r3_teeth.py`、`E:/tmp/r3_teeth2.py`、`E:/tmp/r3_texts.py`、`E:/tmp/run3-list.txt`、
  `E:/tmp/r3-batch{,.log}`、`E:/tmp/r3-dry{,-out.txt}`（dry-run 证据）。
- 未触碰：`run_batch.sh` 执行逻辑、`card_window.py`、`src/**`、mysqld；未起服务端、未占端口（tester 私栈在听）。
