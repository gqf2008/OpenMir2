# `HostNpc` 覆写 `envir_root` + 两参 `GetEnvirFilePath`（T-220）

- 卡：`openmir2-script-envir-root-host`（tier=R）；分支 `card/openmir2-script-envir-root-host`
- 原作者：`openmir2-worker-24`；**返工**：`openmir2-worker-24`（review `51132242` / `ac78b56a`）

> **本文件按 head 实测重写（review `ac78b56a` I-2）**：上一版描述的是 worker-66 的中间实现
> （用例名 / 牙 sha / 「剥离逻辑只此一份」的说法都不成立）。以下每条都能在本 head 的
> `git show HEAD:<path>` 上复核。

## 1. 问题

`HostNpc`（`bins/game-svr/src/npc.rs`）缺 `ScriptNpc::envir_root` 覆写时恒 `None` ⇒ `check_string_list` /
`envir_file_path` 全部按「文件不存在」处理：`CHECKNAMELIST`(15)、`CHECKACCOUNTLIST`(238) 恒 false，
`ADDNAMELIST`(9)/`DELNAMELIST`(2)/`DELACCOUNTLIST`(238) 不写盘（计数取自部署两个 Envir 根）。

本 head 在此之上还修了**两参**口径：oracle 有两种 `GetEnvirFilePath`（`src/M2Server/M2Share.cs:1970-1985`）：

| 形态 | 语义 |
|---|---|
| 单参 `GetEnvirFilePath(filePath)`（`:1970-1977`） | `filePath` 以 `..` 开头 ⇒ 剥前 3 字符再拼 Envir 根 |
| 两参 `GetEnvirFilePath(dirPath, filePath)`（`:1979-1985`） | `filePath` 以 `..` 开头 ⇒ **忽略 `dirPath`**、剥前 3 字符；否则拼 `Envir/dirPath/filePath` |

`CHECKNAMEDATELIST`（`ConditionProcessingSys.cs:642`）和 `ADDUSERDATE`（`ExecutionProcessingSys.cs:2576`）
走的是**两参**（`normNpc.Path` 作 `dirPath`）。旧实现先 `format!("{}{}", npc.path(), s_param1)` 再走单参 ⇒
`MapQuest_def..\QuestDiary\…` 不以 `..` 开头 ⇒ 不剥 ⇒ 指向不存在的路径。部署可达链：
`MapQuest_def/QManage.txt:36` `#CALL [\月卡\月卡系统.txt] @月卡` → `QuestDiary/月卡/月卡系统.txt:5`
`CHECKUSERDATE ..\QuestDiary\月卡\月卡记录.txt > 30 p0 p1`（QManage = `HostNpc` 漏斗）。

## 2. 实现（本 head）

- `impl ScriptNpc for HostNpc`：`fn envir_root` 返回 `self.envir_root.clone()`（`NpcHost` 注入；
  `MerchantScriptBook::goto_label` 仍传 `None`，登记在案）。
- `crates/script/src/engine/actions.rs`：
  - `envir_file_path(rel)` —— 单参口径（`..` ⇒ 剥 3 字符）；
  - **新增 `envir_file_path_with_dir(dir, rel)`** —— 两参口径（`..` ⇒ 忽略 `dir`、剥 3 字符；否则拼 `dir/rel`）。
- `crates/script/src/engine/conditions.rs`：`cond_check_name_date_list` 改用 `envir_file_path_with_dir(npc.path(), s_param1)`。
- `engine/actions.rs`：`action_of_add_use_date_list` 同上（两参）。
- `engine/mod.rs`：`check_string_list` 改为**复用** `envir_file_path`（删除内联的第二份剥离逻辑；
  未注入根 ⇒ 记 `file not found => <rel>` 并返回 false，与接线前行为一致）。
- `npc.rs`：用例文档注释里原有一处 `impl ScriptNpc for HostNpc` **字面**，会被台账 `parse_impls`
  当成第二个 impl 块（导致 `path` 误报「标已覆写但 impl 块里没有该方法」）——已改写措辞。

### 2.1 覆盖精度（部署形态）

| 面 | 部署处数 | 本 head |
|---|---:|---|
| `CHECKNAMELIST`（`HostNpc` 承载：`QManage.txt:24/52`、`QFunction-0.txt:60`、经 `#CALL` 的 `假人登录脚本.txt:359/394`、`月卡系统.txt:13`） | 6/15 | **接通**（单参：`..` 剥离后指向 `Envir/QuestDiary/…`） |
| `CHECKNAMELIST`（商人脚本 9 处） | 9/15 | 仍恒假（`MerchantScriptBook` 注入 `None`，登记在案） |
| `CHECKNAMEDATELIST`/`CHECKUSERDATE`（`月卡系统.txt:5`） | 1 | **本 head 修好**：两参 ⇒ `<Envir>/QuestDiary/月卡/月卡记录.txt`（该文件当前 0 字节 ⇒ 今天两侧都 false；充值写入一行后 oracle 判真而旧 Rust 恒假） |
| `ADDNAMELIST`/`DELNAMELIST`/`DELACCOUNTLIST`（249 处） | 249/249 | 两侧**一致地**先拼 `npc.path()` 再单参 ⇒ `Market_Def..\…` 不剥 ⇒ 不写盘（parity 成立；写盘由构造级用例覆盖） |
| `ADDUSERDATE`/`DELUSERDATE` | 部署未出现 | 已按两参实现（`ExecutionProcessingSys.cs:2576/2613`） |
| `CHECKACCOUNTLIST`（O-1） | 238/238 | 单参站点、`npc.path()+..` 形态 ⇒ 两侧一致坏；正例需非 `..` 夹具（工具卡） |

## 3. 用例与牙（4 用例 / 2 牙）

用例（`bins/game-svr/src/npc.rs` tests 模块，均用临时 `Envir/` 夹具）：

1. `host_npc_envir_root_enables_check_name_list` —— `TestList.txt`：`Trim` + `OrdinalIgnoreCase` 命中；
   `..\TestList.txt`（部署形态）同样命中；未注入根 ⇒ miss；
2. `host_npc_envir_root_name_list_action_writes_to_disk` —— `ADDNAMELIST`(45) 真写盘：
   两点前缀剥离、CRLF 重写逐字节、二次执行幂等；
3. `host_npc_envir_root_check_name_date_list_uses_two_arg_envir_path`（**I-3**）—— `CHECKNAMEDATELIST`(144)
   `..\QuestDiary\月卡\月卡记录.txt` 经两参口径命中；文件不存在 ⇒ miss；
4. `merchant_script_book_without_envir_root_keeps_check_name_list_false`（**M-1**）—— 商人漏斗不注入根
   ⇒ `CHECKNAMELIST` 恒假且**不写盘**（名单本来存在且能命中）。

牙（实做后 `git checkout --` 逐字节还原；sha256 取**提交后树**）：

| 牙 | 改动 | 结果 | 还原后 sha256（`git show f35418b41:<path>`） |
|---|---|---|---|
| A | 删 `impl ScriptNpc for HostNpc` 的 `fn envir_root` | **3 FAILED**（用例 1、2、3）/ 1 passed（用例 4 本来就期待 miss） | `npc.rs` = `260298317242baa21d24c86c840ef87ef0611e2025efbe7d230aff8084cfff25` |
| B | `envir_file_path` 关掉剥离（`let rel = rel;`） | **2 FAILED**（用例 1 的两点分支、用例 2）；用例 3 仍绿（它走两参 helper） | `actions.rs` = `134a1e92b43c535f551838892114f0e984753e4a90506cb87cc98133ed2d8107` |

## 4. 门禁（合并树 `f7c70aad8` = master `a9beb4c44` + 本卡）

```
cargo fmt --all -- --check                                        rc=0
cargo clippy --workspace --all-targets -- -D warnings             rc=0
cargo test --workspace                                            141 suites / 3105 passed / 0 failed
cargo run -p msgcodegen -- --check                                rc=0（604 条）
python -I -B mir2-rs/tests/parity/coverage/script_host_noop_audit.py check                  GREEN（168 行：safe 60 / needs_card 104 / covered 4）
python -I -B mir2-rs/tests/parity/coverage/script_host_noop_audit.py check --strict-lines   GREEN（同上）
python -I -B mir2-rs/tests/parity/coverage/script_host_noop_audit.py selftest               GREEN（牙后 sha256 不变）
python -I -B mir2-rs/tests/parity/m3/whitelist_anchor_gate.py check        ALL PASS（544 条锚）
python -I -B mir2-rs/tests/parity/m3/m3_whitelist_registry_audit.py audit  GREEN
python -I tools/whitelist/t_reservation_gate.py check                      GREEN（rows=209 whitelist=209）
```

**台账重钉**（I-1，纪律第 13 条）：`envir_root` 行由 `needs_card` 改为 `safe/overridden`，`evidence`
指向 `ScriptNpc for HostNpc::envir_root`，`subcard` 清空；本 head 在 `npc.rs` 前部新增行 ⇒ **56 条
`evidence.line` 按源文件重新计算**（脚本按 `impl` 块内 `fn` 声明行重钉，不手改）。

## 5. 残余 / 窗口项（R，未跑）

- 商人路 `envir_root` 未注入（`MerchantScriptBook` 无字段）⇒ 商人脚本的同源条件仍恒假（另卡）。
- **O-1**：`CHECKACCOUNTLIST` 238/238 与动作路 249/249 都是 `npc.path()+..` 形态，oracle 与 Rust
  **两侧一致不剥/不写**；本卡的两参修复只影响 `CHECKNAMEDATELIST`/`ADDUSERDATE`。
- `CHECKACCOUNTLIST` 正例依赖现役子卡 `openmir2-script-user-account`（issue `8b3f7041`）。
- `filePath` 恰为 `..`（长度 2）时 oracle `filePath[3..]` 抛 `ArgumentOutOfRangeException`、
  Rust `get(3..)` 取空串 ⇒ **已知边界偏差**（部署不可达，登记在 T-220）。
- **真机窗口**（T-220 行）：① `MapQuest_def/QManage.txt:24`（`[@Login]`）用 `login_only` + 名单内/外两个
  角色做差分（命中 ⇒ `#CALL @假人主号登录` ⇒ `MAPMOVE jiaren` + `RecallHero` + `CHANGEATTACKMODE 1`）；
  ② `月卡系统.txt:5` 的 `CHECKUSERDATE` 需先由充值流程写入一行才有正例（当前两侧皆 false）；
  ③ 动作路只判「不写盘」的 parity。窗口未跑前本卡不得 `closed`。

## 6. 残留物

- `E:/tmp/fix_ledger.py`（台账行号重钉脚本，不入库）、`E:/tmp/w24-er-test.log`（全量测试输出）
- 未起栈、未占端口、未连库。
