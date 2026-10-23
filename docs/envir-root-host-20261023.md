# `HostNpc` 覆写 `envir_root`（文件列表类条件/动作）+ 两参口径边界（T-220）

- 卡：`openmir2-script-envir-root-host`（tier=R）；分支 `card/openmir2-script-envir-root-host`
- 作者：`openmir2-worker-24`；**返工**：`openmir2-worker-24`（review `51132242` / `ac78b56a` / `40109056`）
- **交付 head**：见本文件末「交付头」一节的实测值（本文件与代码同一提交；`npc.rs` / `actions.rs` 的
  sha256 均取自 `git show <head>:<path>`，即返工后的最终树）
- 合并基线：`refs/remotes/walgit/master` = `19009c6a0`（已并入本分支，冲突 0）

## 1. 问题

`HostNpc`（`bins/game-svr/src/npc.rs`）缺 `ScriptNpc::envir_root` 覆写时恒 `None` ⇒ `check_string_list` /
`envir_file_path` 全部按「文件不存在」处理：`CHECKNAMELIST`(15)、`CHECKACCOUNTLIST`(238) 恒 false，
`ADDNAMELIST`(9)/`DELNAMELIST`(2)/`DELACCOUNTLIST`(238) 不写盘（计数取自部署两个 Envir 根）。

oracle 有两种 `GetEnvirFilePath`（`src/Modules/SystemModule/SystemShare.cs:1989-1995`）：

| 形态 | 语义 |
|---|---|
| 单参 `GetEnvirFilePath(filePath)`（`:1989-1992`） | `filePath` 以 `..` 开头 ⇒ 剥前 3 字符再 `Path.Combine(BasePath, EnvirDir, rest)`；**否则按原样拼接（不插分隔符、不 normalize）** |
| 两参 `GetEnvirFilePath(dirPath, filePath)`（`:1994-1995`，实现见 `M2Share.cs:1979-1982`） | `filePath` 以 `..` 开头 ⇒ **忽略 `dirPath`**、剥前 3 字符；否则 `Path.Combine(BasePath, EnvirDir, dirPath, filePath)` |

**哪个站点用哪种口径（review `40109056` I-1 订正）**：

| 站点 | oracle | 口径 |
|---|---|---|
| `CHECKNAMEDATELIST` / `CHECKUSERDATE` | `ConditionProcessingSys.cs:642` | **两参**（`normNpc.Path` 作 `dirPath`） |
| `ADDUSERDATE`（code 335） | `ExecutionProcessingSys.cs:5842-5846`：先 `sListFileName = normNpc.Path + sParam1`，**再调单参** | **先拼后单参** |
| `DELUSERDATE`（code 336） | `ExecutionProcessingSys.cs:5887-5891`：同样先拼，再交给 `ActionOfDelList`（内部单参） | **先拼后单参** |
| `ADDNAMEDATELIST`（code 231，`ActionOfAddNameDateList`） | `ExecutionProcessingSys.cs:2572-2607`：`GetEnvirFilePath(normNpc.Path, sParam1)` | 两参——**但本仓未实现该动作**（不在 `IMPLEMENTED_ACTIONS`） |

> 旧版本文与旧 T-220 把 `:2576` 当成 `ADDUSERDATE` 的出处，并据此把动作改成两参——这是**错的**，
> 且造成真回归（见 §2 与 §3 的牙 C）。

**部署可达链（`ADDUSERDATE` 真回归面）**：`Market_Def/QFunction-0.txt:82/84`（`[@StdModeFunc11]` 月卡充值）
`DELUSERDATE/ADDUSERDATE ..\QuestDiary\月卡\月卡记录.txt`；生产入口 `lib.rs:6899-6901` 的
`host.goto_label("QFunction", …)` ⇒ `HostNpc` 注入了 `envir_root`。

- oracle：`"Market_Def" + "..\QuestDiary\月卡\月卡记录.txt"` = `Market_Def..\QuestDiary\…`，
  **不以 `..` 开头** ⇒ 单参不剥 ⇒ `<Envir>\Market_Def..\QuestDiary\…`（Win32 去掉组件尾点后是
  `<Envir>\Market_Def\QuestDiary\…`，该目录不存在）⇒ `File.Exists` false、`SaveToFile` 抛异常被
  `catch` ⇒ **不写盘**。
- 错误实现（两参 helper）：剥 `..`、忽略 `Market_Def` ⇒ `<Envir>\QuestDiary\月卡\月卡记录.txt`
  （目录存在）⇒ **真写盘**；随后 `QuestDiary/月卡/月卡系统.txt:5` 的 `CHECKUSERDATE` 两侧分叉。

## 2. 实现（本 head）

- `impl ScriptNpc for HostNpc`：`fn envir_root` 返回 `self.envir_root.clone()`（`NpcHost` 注入；
  `MerchantScriptBook::goto_label` 仍传 `None`，登记在案）。
- `crates/script/src/engine/actions.rs`：
  - `envir_file_path(rel)` —— 单参口径（`..` ⇒ 剥 3 字符，否则原样拼接）；
  - `envir_file_path_with_dir(dir, rel)` —— 两参口径（`..` ⇒ 忽略 `dir`、剥 3 字符；否则拼 `dir/rel`）；
  - **`action_of_add_use_date_list` 回退为「`npc.path() + s_param1` 后走单参」**（oracle `:5842-5846`；
    review `40109056` I-1）。两参 helper 只保留给 `cond_check_name_date_list`（oracle `:642`）。
    非 `..` 形态也随之对齐：oracle 无分隔符直接拼 ⇒ Rust 不再插入 `/`。
- `crates/script/src/engine/conditions.rs`：`cond_check_name_date_list` 用
  `envir_file_path_with_dir(npc.path(), s_param1)`（两参，oracle `:642`）。
- `engine/mod.rs`：`check_string_list` **复用** `envir_file_path`（没有第二份剥离逻辑；
  未注入根 ⇒ 记 `file not found => <rel>` 并返回 false，与接线前行为一致）。
- `npc.rs`：新增 I-1 回归用例；原文档注释里的 `impl ScriptNpc for HostNpc` **字面**已改写措辞
  （否则台账 `parse_impls` 会把它当成第二个 impl 块）。

### 2.1 覆盖精度（部署形态）

| 面 | 部署处数 | 本 head |
|---|---:|---|
| `CHECKNAMELIST`（`HostNpc` 承载：`QManage.txt:24/52`、`QFunction-0.txt:60`、经 `#CALL` 的 `假人登录脚本.txt:359/394`、`月卡系统.txt:13`） | 6/15 | **接通**（单参：`..` 剥离后指向 `Envir/QuestDiary/…`） |
| `CHECKNAMELIST`（商人脚本 9 处） | 9/15 | 仍恒假（`MerchantScriptBook` 注入 `None`，登记在案） |
| `CHECKNAMEDATELIST`/`CHECKUSERDATE`（`月卡系统.txt:5`） | 1 | **本 head 修好**：两参 ⇒ `<Envir>/QuestDiary/月卡/月卡记录.txt`（该文件当前 0 字节 ⇒ 今天两侧都 false；充值写入一行后 oracle 判真而旧 Rust 恒假） |
| `ADDNAMELIST`/`DELNAMELIST`/`DELACCOUNTLIST`（249 处） | 249/249 | 两侧**一致地**先拼 `npc.path()` 再单参 ⇒ `Market_Def..\…` 不剥 ⇒ 不写盘（parity 成立；写盘由构造级用例覆盖） |
| `ADDUSERDATE`/`DELUSERDATE`（`QFunction-0.txt:82/84`，**部署确实出现**） | 2 | **两侧一致不写盘**（先拼后单参；回归用例 `…add_use_date_list_deployment_form_does_not_write` 钉住） |
| `CHECKACCOUNTLIST`（O-1） | 238/238 | 单参站点、`npc.path()+..` 形态 ⇒ 两侧一致坏；正例需非 `..` 夹具（工具卡） |

## 3. 用例与牙（5 用例 / 3 牙）

用例（`bins/game-svr/src/npc.rs` tests 模块，均用临时 `Envir/` 夹具）：

1. `host_npc_envir_root_enables_check_name_list` —— `TestList.txt`：`Trim` + `OrdinalIgnoreCase` 命中；
   `..\TestList.txt`（部署形态）同样命中；未命中走 else；未注入根 ⇒ miss；
2. `host_npc_envir_root_name_list_action_writes_to_disk` —— `ADDNAMELIST`(45) 真写盘：
   两点前缀剥离、CRLF 重写逐字节、二次执行幂等；
3. `host_npc_envir_root_add_use_date_list_deployment_form_does_not_write`（**I-1 回归，本轮新增**）——
   `ADDUSERDATE`(335) 部署形态 `Market_Def` + `..\QuestDiary\月卡\月卡记录.txt`：**目标文件逐字节不变**；
   非 `..` 形态同样不写（无分隔符拼接路径不存在）；正向对照（`file_path=""` + `UserDate.txt`）
   断言盘上出现 `hero    <date>`；
4. `host_npc_envir_root_check_name_date_list_uses_two_arg_envir_path`（**I-3**）—— `CHECKNAMEDATELIST`(144)
   `..\QuestDiary\月卡\月卡记录.txt` 经两参口径命中；文件不存在 ⇒ miss；
5. `merchant_script_book_without_envir_root_keeps_check_name_list_false`（**M-1**）—— 商人漏斗不注入根
   ⇒ `CHECKNAMELIST` 恒假且**不写盘**（名单本来存在且能命中）。

牙（实做后 `git checkout --` 逐字节还原；sha256 取**交付树的提交后树** `git show <head>:<path>`）：

| 牙 | 改动 | 结果（本 head 实测） | 还原后 sha256 |
|---|---|---|---|
| A | 删 `impl ScriptNpc for HostNpc` 的 `fn envir_root` | **4 FAILED**（用例 1/2/3/4）/ 1 passed（用例 5 本来就期待 miss） | `npc.rs` = `c1f7ab01ce1258e43ae1c61d4b76b1612311ab84e2364c36575c1335d828668f` |
| B | `envir_file_path` 关掉 `..` 剥离（`let rel = rel;`） | **2 FAILED**（用例 1 的两点分支、用例 2）/ 3 passed | `actions.rs` = `346de17549e9bf95d5aee4ebd7f2bf7ec203136b97d8e79f5f4d1d7a8278f4c2` |
| C | `action_of_add_use_date_list` 改回两参 `envir_file_path_with_dir` | **1 FAILED**（用例 3：目标文件被写成 `hero    \r\n`）/ 4 passed | `actions.rs` = `346de17549e9bf95d5aee4ebd7f2bf7ec203136b97d8e79f5f4d1d7a8278f4c2` |

## 4. 门禁（交付树实测；命令在 `E:/tmp/w24-envir-wt/mir2-rs`）

```
cargo fmt --all -- --check                                        rc=0
cargo clippy --workspace --all-targets -- -D warnings             rc=0
cargo test --workspace                                            rc=0，**141 suites / 3115 passed / 0 failed / 62 ignored**
cargo run -p msgcodegen -- --check                                rc=0（604 条常量与 C# 源一致）
python -I -B mir2-rs/tests/parity/coverage/script_host_noop_audit.py check --strict-lines   GREEN 168 行（safe 64 / needs_card 100 / covered 4）
python -I -B mir2-rs/tests/parity/coverage/script_host_noop_audit.py selftest              GREEN（台账 sha256 前后一致 `2047afa2…`）
python -I -B mir2-rs/tests/parity/m3/whitelist_anchor_gate.py check        ALL PASS 558 条
python -I -B mir2-rs/tests/parity/m3/m3_whitelist_registry_audit.py audit  GREEN（无重复编号；夹具 token 唯一）
python -I -B mir2-rs/tests/parity/coverage/gm_registry.py gate            GREEN 74 条
python -I tools/whitelist/t_reservation_gate.py check                      GREEN（rows=251 / whitelist=251 / max=T-264 / malformed=0）
```

（台账 `envir_root` 行维持 `safe/overridden`，evidence 指向 `ScriptNpc for HostNpc::envir_root`；
本轮只在 `npc.rs` 的 **tests 模块**内新增用例，`envir_root` 覆写在第 610 行、位于新增点之前，
台账行号不受影响。）

## 5. 残余 / 窗口项（R，未跑）

- 商人路 `envir_root` 未注入（`MerchantScriptBook` 无字段）⇒ 商人脚本的同源条件仍恒假（另卡）。
- **O-1**：`CHECKACCOUNTLIST` 238/238 与动作路 249/249 都是 `npc.path()+..` 形态，oracle 与 Rust
  **两侧一致不剥/不写**；本卡的两参修复只影响 `CHECKNAMEDATELIST`。
- `CHECKACCOUNTLIST` 正例依赖现役子卡 `openmir2-script-user-account`（issue `8b3f7041`）。
- `filePath` 恰为 `..`（长度 2）时 oracle `filePath[3..]` 抛 `ArgumentOutOfRangeException`、
  Rust `get(3..)` 取空串 ⇒ **已知边界偏差**（部署不可达，登记在 T-220）。

### 5.1 窗口项①（可复跑命令，M-1）

`HostNpc` 承载的真实站点：`MapQuest_def/QManage.txt:24`（`[@Login]`）的
`checknamelist ..\QuestDiary\LEGM2.CC假人系统\假人名单列表.txt`（该名单 131 行存在）。
**两栈同一账号/角色、同一 scenario**，按下面四条命令跑：

```sh
# 1) Rust 栈：命中侧（--char-name 取「假人名单列表.txt」里已有的名字）
python -I -B tools/probe/mir2_probe.py --login-gate 127.0.0.1:7000 \
  --account <账号> --password <口令> --char-name <名单内的名字> \
  --scenario tools/probe/scenarios/login_only.json --dwell-ms 2000 \
  --out E:/tmp/win-envir-rust-hit

# 2) Rust 栈：未命中侧（同一账号换成不在名单里的角色名）
python -I -B tools/probe/mir2_probe.py --login-gate 127.0.0.1:7000 \
  --account <账号> --password <口令> --char-name <名单外的名字> \
  --scenario tools/probe/scenarios/login_only.json --dwell-ms 2000 \
  --out E:/tmp/win-envir-rust-miss

# 3) C# oracle 栈（alt 端口栈；端口按当日栈配置替换）用同一账号/角色与同一 scenario 各跑一次
python -I -B tools/probe/mir2_probe.py --login-gate <csharp-gate-host:port> \
  --account <账号> --password <口令> --char-name <名单内的名字> \
  --scenario tools/probe/scenarios/login_only.json --dwell-ms 2000 \
  --out E:/tmp/win-envir-cs-hit

# 4) 判据：hit 两侧帧序列逐帧一致（`#CALL @假人主号登录` ⇒ MAPMOVE jiaren / RecallHero /
#    CHANGEATTACKMODE 1 的出现位置一致）；miss 两侧都不出现这些帧；hit−miss 差分两栈一致。
```

窗口项②（`月卡系统.txt:5` 的 `CHECKUSERDATE` 正例）：先由充值流程（`QFunction-0.txt:82/84`）写一行，
或按同一格式（`<名字><4 空格><日期>` + CRLF、gb2312）**手动 seed** `Envir/QuestDiary/月卡/月卡记录.txt`，
再跑两侧同一场景。窗口项③（动作路）：部署 `ADDUSERDATE/DELUSERDATE` 两侧**都必须不写盘**
（比对 `E:/tmp` 前后文件的 sha256）。窗口未跑前本卡不得 `closed`。

## 6. 残留物

- `E:/tmp/w24-envir-wt`（本卡隔离 worktree；原 `OpenMir2-wt-cronclaim` 被另一张在飞卡占用）、
  `E:/tmp/w24-envir-target`（独立 `CARGO_TARGET_DIR`）、`E:/tmp/w24-envir/`（牙/还原备份与线程快照）
- 未起栈、未占端口、未连库。

## 7. 交付头

- head：见线程 `patch` 条目（本文件与代码同一提交；其中 `refs.head` 即交付头）
- 真实 merge-base（`git merge-base refs/remotes/walgit/master HEAD`）：`19009c6a0`
- `npc.rs`（`git show <head>:mir2-rs/bins/game-svr/src/npc.rs`）：`c1f7ab01ce1258e43ae1c61d4b76b1612311ab84e2364c36575c1335d828668f`
- `actions.rs`（`git show <head>:mir2-rs/crates/script/src/engine/actions.rs`）：`346de17549e9bf95d5aee4ebd7f2bf7ec203136b97d8e79f5f4d1d7a8278f4c2`
