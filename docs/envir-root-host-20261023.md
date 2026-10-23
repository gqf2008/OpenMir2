# `HostNpc` 覆写 `envir_root` + `CHECKNAMELIST` 的 `..` 前缀剥离（T-220）

- 卡：`openmir2-script-envir-root-host`（tier=R）；分支 `card/openmir2-script-envir-root-host`
- 原作者：`openmir2-worker-24`；**返工**：`openmir2-worker-66`（review `51132242` / reviewer-5）

## 1. 问题

`HostNpc`（`bins/game-svr/src/npc.rs`）没有 `ScriptNpc::envir_root` 覆写 ⇒ 缺省 `None` ⇒
`check_string_list` / `envir_file_path` 全部按「文件不存在」处理：`CHECKNAMELIST`(15)、
`CHECKACCOUNTLIST`(238) 恒 false，`ADDNAMELIST`(9)/`DELNAMELIST`(2)/`DELACCOUNTLIST`(238)
不写盘（计数取自部署两个 Envir 根，二者逐字相同）。

**I-1（blocking，复审发现）**：只注入根并不够——部署语料里 **15/15 条 `CHECKNAMELIST`**
参数都是 `..\QuestDiary\…`；oracle `GetEnvirFilePath`（`SystemShare.cs:1989-1992`，被
`ProcessingBase.cs:1829` 调用）对以 `..` 开头的路径**丢弃前 3 个字符**再拼
`Path.Combine(BasePath, EnvirDir, rest)`。原 `check_string_list` 把 `..` 当普通段 `push` ⇒
得到 `<BasePath>/QuestDiary/…`（Envir 之外、不存在）⇒ **注入根后仍恒假**。reviewer 用卡自己的
用例把参数换成部署形态（`..\TestList.txt`）复现：`left ["miss"] / right ["hit"]`。

## 2. 实现

- `HostNpc` 新增 `envir_root: Option<String>`；`run_goto_label` 增加注入形参：
  `NpcHost::goto_label` 传 `self.envir_root`（空 `PathBuf` ⇒ `None`），
  **`MerchantScriptBook::goto_label` 传 `None`**（商人表未持 Envir 根，保持接线前行为）；
- `impl ScriptNpc for HostNpc` 新增 `fn envir_root`；
- **I-1 修复**：`check_string_list` 改为**复用** `envir_file_path`——以 `..` 开头时剥前 3 个字符
  再拼 Envir 根（照抄 oracle 的 `filePath[3..]`，**不做** `Path::normalize`/`canonicalize`）；
  未注入根时仍走 `PathBuf::from(rel)`（接线前行为）。剥离规则只此一份，条件与动作不再各写一套。
  该改动在 `crates/script`，属 Leader 授权的扩写集（部署 15/15 的公共前置）。

### 2.1 覆盖精度（部署形态）

| 面 | 部署处数 | 修复后 |
|---|---:|---|
| `CHECKNAMELIST`（`HostNpc` 承载：`QManage.txt:24/52`、`QFunction-0.txt:60`、以及经 `#CALL` 的 `假人登录脚本.txt:359/394`、`月卡系统.txt:13`） | 6/15 | **接通**（`..` 剥离后指向 `Envir/QuestDiary/…`） |
| `CHECKNAMELIST`（商人脚本：`假人上线NPC`、`任务-3`、`客栈-0148`、`武士-0114`、`影传送-h001`、`书店-0`） | 9/15 | 仍恒假（`MerchantScriptBook` 注入 `None`，**登记在案**） |
| `ADDNAMELIST`/`DELNAMELIST`/`DELACCOUNTLIST` | 249/249 | **两侧一致地不写盘**：动作路先拼 `npc.path()`（`Market_Def`/`MapQuest_def`）再交给 `GetEnvirFilePath`，拼出的 `"Market_Def..\QuestDiary\…"` **不以 `..` 开头** ⇒ oracle 也不剥 ⇒ 路径不存在。写盘路径由构造级用例覆盖（见 §3），真机只能判「不写盘」的 parity |
| `CHECKACCOUNTLIST`（O-1） | 238/238 | 同上：`rel = npc.path() + s_param1` ⇒ 两侧一致坏（parity 成立，但「接通」名不副实）。**(a) 不改变它**；正例需非 `..` 形态或工具卡新增夹具 |

## 3. 用例与牙（4 用例 / 2 牙）

用例（`bins/game-svr/src/npc.rs`，均用临时 `Envir/` 夹具）：

1. `host_npc_envir_root_enables_check_name_list`（原作者）：`TestList.txt`，`Trim()` + 忽略大小写；
   未注入根 ⇒ 恒假；
2. `host_npc_envir_root_strips_dotdot_prefix_for_check_name_list`（**I-1 部署形态**）：
   `..\TestList.txt`（名单仍在 `Envir/TestList.txt`）⇒ 命中；
3. `host_npc_add_name_list_writes_gb2312_crlf_and_deletes_line`（**acceptance ③**）：
   `ADDNAMELIST` 真写盘；按 oracle 缺陷（`ActionOfAddList`/`ActionOfDelList` 逐字相同 ⇒ 实为**删行**）
   断言 `gb2312`（无 BOM）+ 每行 CRLF 的**逐字节**结果；同时覆盖动作路的 `..` 形态；
4. `merchant_script_book_without_envir_root_keeps_check_name_list_false`（**商人 `None` 路回归**）：
   即使别处存在同名名单，商人不注入根时仍恒假。

牙（逐字节还原；`sha256` 取**提交后的树** `git show <head>:<path>`）：

| 牙 | 改动 | 结果 | 还原 sha256 |
|---|---|---|---|
| A | 删 `impl ScriptNpc for HostNpc` 的 `fn envir_root` | 用例 1、2 均 `FAILED`（`left: ["miss"]`），rc=101 | `npc.rs` = `cda166fdaa27f20e3463a90a068de7dbd3ccf42dbfb19c170502a7af43b18963` |
| B | 关掉 `envir_file_path` 的 `..` 剥离（`let rel = rel;`） | 用例 2 `FAILED`；用例 3 `FAILED`（文件原样 `Hero\r\n其它\r\n`） | `actions.rs` = `40d15c48b22a844ed303a443713898286669781b9ef54654ccdc88c5541758dd` |

> 两个 sha 均与 `git show <head>:…` 的提交后树逐字节一致（reviewer-5 M-3：不再用工作区哈希）。

## 4. 门禁（真实合并树：master `43ce8dca` + 本 head；独立 `CARGO_TARGET_DIR=E:/tmp/envirroot-merge-target-w66`）

```
cargo fmt --all -- --check                                        rc=0
cargo clippy --workspace --all-targets --locked -- -D warnings    rc=0
cargo test --workspace --locked                                   rc=0（141 suites / 3059 passed / 0 failed / 62 ignored）
cargo test -p mir2-world --test no_parallel --locked              rc=0（5 passed）
cargo run --locked -p msgcodegen -- --check                       rc=0（604 条）
python -I -B mir2-rs/tests/parity/coverage/script_host_noop_audit.py check --strict-lines
                                                                  rc=0（168 行：safe 60 / needs_card 104 / covered 4）
python -I -B mir2-rs/tests/parity/m3/whitelist_anchor_gate.py check        ALL PASS（517 条锚）
python -I -B mir2-rs/tests/parity/m3/m3_whitelist_registry_audit.py audit  GREEN（200）
python -I -B tools/whitelist/t_reservation_gate.py check          GREEN（202 行 / 202 号）
python -I -B mir2-rs/tests/parity/coverage/gm_registry.py gate     GREEN（86）
```

> 首跑 `game-gate --test relay::header_prefixed_login_is_kicked` 红（`left [] / right [1,2]`），
> 单跑该用例绿、整套复跑 3059/0/62 绿 ⇒ 并行负载下的时序 flaky（该文件不属本卡写集）。
> 按队列纪律第 11 条记为环境噪声，未改测试。
>
> **strict-lines（纪律第 13 条）**：本卡在 `npc.rs` 前部新增 19 行 ⇒ 台账 56 条 `evidence.line`
> 与 `envir_root` 行（`needs_card` → `safe/overridden`、`subcard` 清空）按**合并树**重钉；
> 重钉后 `check --strict-lines` GREEN，`line_baseline` 已改为本合并树。
> 另：用例文档注释里原有第二处 ``impl ScriptNpc for HostNpc`` 字面会被台账解析器误当 impl 块
> （导致 `path` 误报「不在 impl 块内」），已改写措辞。

## 5. 残余 / 窗口项（R，未跑）

- 商人路 `envir_root` 未注入（`MerchantScriptBook` 无字段）⇒ 商人脚本的同源条件仍恒假（另卡）。
- **O-1**：`CHECKACCOUNTLIST` 238/238 与动作路 249/249 都是 `npc.path()+..` 形态，oracle 与 Rust
  **两侧一致不剥/不写**；本卡的 `..` 修复只影响「无 path 前缀」的 `CHECKNAMELIST`。正例需
  非 `..` 形态夹具（工具卡）——即使账号注入也仍然不通。
- `CHECKACCOUNTLIST` 正例依赖**现役**子卡 `openmir2-script-user-account`（issue `8b3f7041`）；
  原指向的 `openmir2-script-user-account-host` 已作废（重复卡），本返工已改。
- **真机窗口**（见 T-220 行）：
  ① **HostNpc 站点点名：`MapQuest_def/QManage.txt:24`**（`[@Login]` 块）。可达性链：
  `QManage` 是 `SystemShare.ManageNPC`（宿主 `NpcHost`，`run_login` → `HostNpc::goto_label` 注入
  `Some(envir_root)`）→ 条件 `rel` 就是 `s_param1`（条件路不拼 `npc.path()`）→ `..\QuestDiary\…`
  剥前 3 字符后指向 `<Envir>/QuestDiary/LEGM2.CC假人系统/假人名单列表.txt`（131 行，存在）。
  用 `login_only` + 一个**名字在名单里**的角色（从表里挑，**不改表**）⇒ 命中 ⇒
  `#CALL @假人主号登录` ⇒ `MAPMOVE jiaren` + `RecallHero` + `CHANGEATTACKMODE 1`（地图/帧可观测）；
  再用**不在名单**的角色跑同一场景作差分（应不 `MAPMOVE`）。这是 `HostNpc` 承载的合法
  `@Login` 路径，`login_only` 真能触发——不是拿登录场景糊过去；
  ② **O-1**：部署动作路 **249/249**（ADDNAME 9 / DELNAME 2 / DELACCOUNT 238）与
  `CHECKACCOUNTLIST` **238/238** 都是 `npc.path()+..` 形态 ⇒ 拼成 `Market_Def..\…`，oracle 与
  Rust **两侧一致地不剥/不写盘** ⇒「ADDNAMELIST 落盘逐字节」在部署形态下**不可达**，写盘只能由
  构造级用例 3 覆盖，真机只判「不写盘」的 parity；
  ③ `CHECKACCOUNTLIST` 正例待子卡 + 非 `..` 夹具，暂不派。
- 窗口未跑前本卡不得 `closed`。

## 6. 残留物

- `E:/tmp/envirroot-tooth.py`（牙 A/B 改树/还原脚本，不入库）
- `E:/tmp/envirroot-target-w66/`（门禁构建缓存，可删）
- `E:/tmp/w24-er.sha`（原作者牙基线，已被本返工的提交后 sha 取代）
- 未起栈、未占端口、未连库。

## 复审 51132242 返工（M-1）

新增 `ADDNAMELIST`(45) 真写盘用例：两点前缀剥离后删行、CRLF 逐字节重写、二次执行幂等
（`host_npc_envir_root_name_list_action_writes_to_disk`）。用例合计 5 条、牙 2 颗。
