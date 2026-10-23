# 真机窗口 Runbook（2026-10-23 定稿）

> 面向：第一次跑窗口的人 / 下一次跑窗口的我。本文只写**跑之前的必做动作**与**读结果的口径**，
> 工具用法见 `tools/conformance/README.md`，卡的判据见各卡 `whitelist.md` 行。

## 0. 现状（master `73c39cca` @ 2026-10-23 15:20 +0800）

- **欠窗口的卡：173 张**（口径见 `docs/leader-goal-20261023.md` §1.1 的订正：早前「grep 已合入 master = 114」是错的，
  那 114 里 70 张已 closed、23 张是 tier=L；真欠账 21 张，加上 149 张措辞不在 grep 里的 + 3 张快照后合入 = **173**）。
  分批与逐卡落点见 `docs/window-batch-plan-20261023.md`。
- 工具链：`card_window.py`（批处理）、`compare_runs.py`（帧+库比对）、`param_discovery.py`（参数发现）。
- **开跑前还剩一个前置没到位**：账本的 compare 诊断值**尚未加值断言**（卡 `openmir2-card-window-value-assertions`）。
  参数发现已接进 `card_window.py`（卡 `openmir2-window-discovery-wiring` 已合入 `e72fb9728`）；规则表另见 §4.5。
  账本值断言不落地就跑，账本可能写下错值（`openmir2-card-window-value-assertions` 仍在飞行）。

## 1. 开跑前必做（缺一不可）

1. **重建 Rust 栈并记录 sha256**。
   **要重建的是哪个二进制**：`start-rust.ps1` 委托给 `tools/m2-acceptance/start_mir2_server.ps1`，后者的 `-ServerExe` 缺省值是
   **`mir2-rs/target/release/mir2-server.exe`（其次 debug）**——「单二进制多角色」。它**只从 `-Root` 读配置文件**，
   不执行 `rust-root` 里的任何东西。
   ⇒ **`rust-root` 目录里那堆 `.NET` 程序集（`DBSvr.dll`/`LoginSvr.dll`/`GameSrv.dll`/`TouchSocket.dll`…）与 Rust 栈无关**，
   别被它们误导成「跑的是 C#」；也别以为换掉它们就换了栈。真正决定跑的是哪份代码的，是 `SERVER_EXE` 那一行打印出来的路径。
   **本轮实测的坑**（2026-10-23 16:0x 复核）：`mir2-rs/target/release/mir2-server.exe` = **10-23 09:46**、
   `target/debug/mir2-server.exe` = **10-23 11:54**，而 master 已到同日 15:20+。缺省优先 release ⇒ **不重建就会用 09:46 那份**，
   它早于 gm-registry / window-hardening / gm-shutup / frame-drain / gm-host-seam / param-discovery / 宿主审计等今天全部合并。
   用旧二进制跑出来的对拍结论没有意义，**而且它不会报错**——只会给你一份看起来正常的报告。
   做法：`cargo build --release -p mir2-server`；起栈时先跑一次 `start_mir2_server.ps1 -WhatIf`，
   **核对它打印的 `SERVER_EXE` 路径与文件的 sha256**，再把这个 sha256 写进本次窗口的 `README.txt`。
2. **端口全空**：`powershell -File E:\MirServer\port-status.ps1`，十端口必须全 DOWN。`card_window.py` 起栈前会预检并在占用时 rc=2。
3. **库基线复位**：C# 侧 `restore-baseline.sh`（`mir2_test_*`），Rust 侧 `restore-baseline-r.sh`（`mir2_test_*_r`）。**两套库不同名**，拿 `--db test` 给 Rust 侧拍快照会拍到 C# 的库 ⇒ 假差异。
4. **一次只跑一栈**：C# 与 Rust 用同一组备用端口，串行起停。

## 2. 跑的时候

- `card_window.py --card <卡 id> --scenario <场景> --out <目录> --profile alt`；R 卡窗口**必须** `--post-comment` 记账（在解禁之后）。
- **参数发现**：9 个场景声明了 `required`（`npc_actor`/`merchant_actor`/`make_index`/自身坐标…）。两侧**各自发现各自的值**，某侧推不出 ⇒ fail-closed，不许拿一侧的值跑另一侧。
- **手动步骤不要漏**：`login_only.json` 这类**不含发客户端命令的步骤**。若卡面要求「客户端直呼 `@禁言 <名> 1`」之类的探针，必须由操作者在真实客户端手动输入，脚本不会替你做。漏了不会报错，只会让这条判据看起来没红。

## 3. 读结果（最容易错的地方）

- **只看顶层 `verdict`，不要扫 body 里出现过的 `PASS`**。三态：`PASS` / `FAIL` / `UNDECIDABLE`；`UNDECIDABLE` **不是通过**。
- `verdict_reason` 与 `blockers` 写明为什么不是 PASS（如 `evidence_incomplete: README.txt write failed`、`compare_fail`）。
- compare 的帧数/库后态/差异计数在**嵌套 `compare{}`** 里，顶层没有这些字段。
- 证据包必须齐：`report.json` + `README.txt` + `card-window.json` + 两侧 `frames.ndjson`。缺件时工具 rc=2、comment 记 `UNDECIDABLE`。
- **`compare_rc` 的 2 是 UNDECIDABLE，不是绿**。

## 4. 窗口项本身的自检（跑之前 30 秒）

- `grep -n 'ident=' docs/ <卡相关文档>`：**帧号写错没有机器守卫**——今天出过 `ident=769`（实为 `SM_GUILDMAKEALLY_FAIL`）而真帧是 `767`（`SM_MENU_OK`）的事故。写窗口项时**帧号必须同时给 `SM_` 名字**。
- 场景文件里 `--var` 的值必须来自**发现**，不是 `params` 里的默认值（`dice_chain` 的 `npc_actor=119` 就是硬编码默认值，不计入发现能力）。

## 4.5 参数发现覆盖 12/15 —— 决定窗口战役能先推多少

> 2026-10-23 更新（卡 `openmir2-window-discovery-rules`）：规则表 4 条 → **5 条规则**（+ `npc_actor` 一条退化规则），另加 **1 类 computed**。

`param_discovery.py` 的规则表有 **5 条规则**：`self_x` / `self_y`（SM_LOGON 的 param/tag）、
`merchant_actor` / `npc_actor` / `king_actor`（SM_MERCHANTSAY(643).recog；`npc_actor` 另有一条 `SM_TURN` 退化规则），
外加第三类 **`computed`**（`move_x` / `move_y`：由 `self_x`/`self_y` + **场景 JSON 里显式声明的 `delta`** 算出）。
三栏**分开计数**，`kind` 分别是 `derived` / `rule` / `computed`：

| 栏 | 含义 | 计入「发现能力已验证」 |
|---|---|---|
| `derived` | 帧语义直接推出 | ✅ |
| `rule` | 退化规则 = 部署假设（点错就响亮超时） | ❌ |
| `computed` | 场景声明 + 已推导量的**确定性算术**（不是猜值） | ❌（另外记录，见 `discovered.json` 的 `kind`） |

对照 15 个场景的 `required`（2026-10-23 实测，`--allow-missing=3` 时 rc=0）：

| 场景 | required | 发现能否覆盖 |
|---|---|---|
| `merchant_dialog` | merchant_actor | ✅（derived，643） |
| `npc_click` | npc_actor | ✅（derived，643） |
| `pickup_item` | self_x, self_y | ✅（derived，SM_LOGON） |
| `king_dialog` / `king_castle_war` | king_actor | ⚠️ **rule（部署假设，不计入发现能力）**：643 只证明「该 actor 应答了点击」，**不证明是国王**——仓库里全部 643 录制都是赌场庄家（`〖庄〗…@kaiz`），且默认 discovery 点的是「最近的非自己 `SM_TURN`」（录制里就是庄家），**没有可执行步骤能让 discovery 点中国王**。点错的后果是末步响亮超时（`@@buildguildnow` 只存在于`比奇国王-0122.txt`，查不到 label ⇒ 零帧） |
| `move_walk` | move_x, move_y | ✅（**computed**：`self_x+1` / `self_y+0`，delta 写在场景 JSON 的 `computed` 里） |
| `bag_equip` | make_index, takeon_where, item_name | ❌ 需 `SM_BAGITEMS(201)` 规则 + `takeon_where` 来源，本版未实现 |
| `eat_item` | make_index, item_name | ❌ 同上 |
| `combat_hit` | target_x, target_y, hit_dir | ❌ **需新帧/新场景**：目标是**实体**，当前录制里没有任何一帧报出「附近怪物在第几格」。已知的唯一近似来源 `SM_TURN(10).param/tag` 是**转身者自己**的坐标（`BaseObject.cs:1375 SendRefMsg(RM_TURN, Dir, CurrX, CurrY, …)`），且帧里不区分怪物/NPC ⇒ 不能拿它冒充。缺的是一帧「AOI 出现/实体位置」且带种族或对象类型的字段；`hit_dir` 还要能算 `GetNextDirection(self, target)`（`M2Share.cs:299`） |
| `dice_chain` / `dice_chain_auto` / `gm_batch` / `gm_time` / `login_only` / `npc_click_auto` | 无 required | 直接可跑（6 个，已实测 `--check-scenario` 全 rc=0；`dice_chain` 绿来自 `params` 硬编码默认值，**不算发现**） |

⇒ **规则表落地后，`check-all` 能覆盖 12 个场景**（3 derived + 2 rule + 1 computed + 6 无 required）；
其中 `king_*`（2 个）与 `move_walk`（1 个）是本次新解锁的。`move_walk` 的真机 run 还要等 `card_window.py`
透传 `computed`（见下方「接线现状」）；`king_*` **能跑但不该算绿**——`king_actor` 是部署假设（rule），
默认 discovery 推出来的是庄家，真跑会在末步超时（响亮失败），要让它真绿必须先有「点中国王」的 discovery 场景/步骤。
另外 3 个（`bag_equip` / `eat_item` / `combat_hit`）要等新规则/新帧。
所以「②45 次参数化 run」**第一批按这 12 个推**，
其余按「缺哪个变量 → 补哪条规则/哪个帧」逐条开卡，不要笼统地指望发现工具。

`computed` 的判据形态（复审要能复算）：

- 声明在场景 JSON 顶层：`"computed": {"move_x": {"derived_from": "self_x", "delta": 1}}`；
  **delta 必须显式写出**——缺 `delta` / 类型不对 / 键不在 `required` / `derived_from` 指向另一个 computed
  一律按**场景坏掉**处理（`cls=error`，rc=2），不是「缺变量」。
- 基准量推不出 ⇒ **连锁 fail-closed**（`move_x`/`move_y` 一起进 `missing`），绝不拿 0 顶替。
- 没有 `computed` 声明（单变量推导入口）⇒ 该类变量照旧 missing。
- `mir2_probe` 不读 `computed`（声明性注记，不是步骤 DSL），但顶层可选键放行，否则带声明的场景会被 `unknown key(s)` 拒绝。

> **接线现状（`card_window.py`）**：`computed` 的**规则与判据已落地**（`check-all` 覆盖 12 个场景），
> 但 `card_window.py` 的 discovery 仍只把 `required` 传进 `param_discovery.derive`，**没有透传场景的
> `computed` 声明** ⇒ 真机跑 `move_walk` 时 `move_x`/`move_y` 会 fail-closed（这比「用错值」安全，但跑不起来）。
> 该文件当时正被卡 `openmir2-card-window-value-assertions` 占用（同文件串行纪律），透传改动另开一行跟进；
> 改动点：`scenario_required` 同源读 `computed` → `discovery_cfg`/`side_ctx` 携带 → `derive_from_frames(frames, required, computed)` → `run_side` 调用点透传。

> 计数订正：本表初稿写「9 个场景（3 可发现 + 6 无 required）」，是规则表 4 条时的口径；
> 规则扩到 5 条 + computed 后实测为 **12 个**（`SUMMARY ... ok=12 rc2=3 derived=3 rule=2 computed=1
> no_required=5 params_default=1 missing=3 error=0`）。复审 `1e9f6315` 指出 king 记 derived 与本文件的分栏定义冲突，已改判 **rule**（`derived` 5→3、`rule` 0→2），`king_*` 不再计入「发现能力已验证」。

## 5. 跑完

- 写 `--post-comment`（解禁后）+ 卡的 `status` 里写明：哪几个窗口项已验、证据路径、两侧二进制 sha256、以及**没验到的项和原因**。
- R 卡窗口全验完才 `closed`；只验了一部分就保持 `in-progress`，并在 `work` 里写明剩余项。
