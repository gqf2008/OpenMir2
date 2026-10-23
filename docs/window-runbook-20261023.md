# 真机窗口 Runbook（2026-10-23 定稿）

> 面向：第一次跑窗口的人 / 下一次跑窗口的我。本文只写**跑之前的必做动作**与**读结果的口径**，
> 工具用法见 `tools/conformance/README.md`，卡的判据见各卡 `whitelist.md` 行。

## 0. 现状（master `73c39cca` @ 2026-10-23 15:20 +0800）

- **欠窗口的卡：约 114 张**（`walgit collab board` 全文 grep「窗口欠项」151 张、其中写明「已合入 master」114 张）。这是终点条件②的唯一缺口。
- 工具链：`card_window.py`（批处理）、`compare_runs.py`（帧+库比对）、`param_discovery.py`（参数发现）。
- **开跑前还有两个前置没到位**：① 参数发现**尚未接进** `card_window.py`（卡 `openmir2-window-discovery-wiring`）；
  ② 账本的 compare 诊断值**尚未加值断言**（卡 `openmir2-card-window-value-assertions`）。
  这两件不落地就跑，要么跑不起来（9 个 required 场景），要么账本可能写下错值。

## 1. 开跑前必做（缺一不可）

1. **重建 Rust 栈并记录 sha256**。`E:/tmp/conformance/rust-root` 里的二进制可能是旧的（本轮实测：二进制约 10-22 23:35，而 master 已到 10-23 15:20，中间合了十余张卡）。**用旧二进制跑出来的对拍结论没有意义，而且不会报错**——它只会给你一份正常的报告。
   构建后把四个二进制的 sha256 写进本次窗口的 `README.txt`（`card_window.py` 会自动记，但你要确认它记的是**新**的）。
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

## 4.5 参数发现只覆盖 3/15 —— 决定窗口战役能先推多少

`param_discovery.py` 的规则表只有 **4 条**：`self_x` / `self_y`（SM_LOGON 的 param/tag）、`merchant_actor` / `npc_actor`（SM_MERCHANTSAY(643).recog）。
对照 15 个场景的 `required`：

| 场景 | required | 发现能否覆盖 |
|---|---|---|
| `merchant_dialog` | merchant_actor | ✅ |
| `npc_click` | npc_actor | ✅ |
| `pickup_item` | self_x, self_y | ✅ |
| `bag_equip` | make_index, takeon_where, item_name | ❌ 需 `SM_BAGITEMS(201)` 规则，本版未实现 |
| `eat_item` | make_index, item_name | ❌ 同上 |
| `combat_hit` | target_x, target_y, hit_dir | ❌ 需新帧/新场景 |
| `move_walk` | move_x, move_y | ❌ 同上 |
| `king_dialog` / `king_castle_war` | king_actor | ❌ 需国王标识帧 |
| `dice_chain` / `dice_chain_auto` / `gm_batch` / `gm_time` / `login_only` / `npc_click_auto` | 无 required | 直接可跑（6 个，已实测 `--check-scenario` 全 rc=0） |

⇒ **接线落地后，能立即跑的只有 9 个场景**（3 个可发现 + 6 个无 required）；另外 6 个要等新规则/新帧/新场景。
所以「②45 次参数化 run」不会一次性解锁——**第一批按这 9 个推**，
其余按「缺哪个变量 → 补哪条规则/哪个帧」逐条开卡，不要笼统地指望发现工具。

> 计数订正：9 个 required 场景 + 6 个无 required = 15，本表初稿误写「其余 7 个」，已按实测改正。

## 5. 跑完

- 写 `--post-comment`（解禁后）+ 卡的 `status` 里写明：哪几个窗口项已验、证据路径、两侧二进制 sha256、以及**没验到的项和原因**。
- R 卡窗口全验完才 `closed`；只验了一部分就保持 `in-progress`，并在 `work` 里写明剩余项。
