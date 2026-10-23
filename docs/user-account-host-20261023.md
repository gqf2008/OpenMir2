# `NpcPlayer::user_account` 覆写 + 账号单源注入（T-223）

- 卡：`openmir2-script-user-account`（tier=R）；分支 `card/openmir2-script-user-account`（叠在
  `card/openmir2-script-envir-root-host` 之上）
- 作者 / 返工：`openmir2-worker-24`（review `dd9f3223` / reviewer-1）
- **接管返工**：`openmir2-worker-67`（review `0ef0464d` F1；纪律 19：原车道 worker-24 不重复派活）
  —— 本次补上**组队漏斗**的账号注入（此前「四条漏斗」的说法与实现不符）。

## 1. oracle 口径

`playerActor.UserAccount` 是 `PlayObject` 的字段：登录时从角色记录 `humData.Account`
（`src/GameSrv/Word/WorldServer.cs:455`）与登录证书（`:1103`）装入，**整会话不变**。读点：

- `CHECKACCOUNTLIST`（`ConditionProcessingSys.cs:620-642`）：`check_string_list` —— 逐行 `Trim()` +
  `OrdinalIgnoreCase` 比对；
- `DELACCOUNTLIST`：`del_from_string_list` —— 命中行删除后按 CRLF 逐字节重写。

## 2. 实现（复审 `dd9f3223` 后改为**单源**）

| 位置 | 改动 |
|---|---|
| `bins/game-svr/src/npc.rs` | `NpcPlayer` 新增 `user_account: String` 字段 + `ScriptPlayer::user_account()` 覆写（缺省空串给其它实现方，不 Trim/不改大小写） |
| `bins/game-svr/src/lib.rs` | `SessionGround` 新增 `account: String`；登录建会话时由 `head.account` 装入（与 `@Login` 漏斗同源）；`npc_player_from_session` 统一 `p.user_account = sg.account.clone()` |
| 同上 | 删除冗余的三处注入：`run_merchant_goto` 的 `user_account` 形参（含 4 个调用点与 `FixedNpcScriptCtx` 字段）、组队 `run_party_goto_label` 里写在**实体查找块内**的 `entry.account` 赋值 |

⇒ 商人 / 固定 NPC（`Npc_def/`）/ QFunction（`@StdModeFunc*`）三条走 `npc_player_from_session`
（`p.user_account = sg.account.clone()`）；`@Login` 直接用同一个 `head.account`；
**组队漏斗**（`run_party_goto_label`）用 `npc_player_from` 逐字段建 player，本轮在其**会话 seed 块**里
补 `player.user_account = sg.account.clone();`（`lib.rs` 的 `abil_exp` seed 之后），与上面三处同源。
四条漏斗口径一致。

> 订正（review `0ef0464d` F1）：上一轮返工把组队漏斗里**实体查找块内的** `entry.account` 赋值删掉后，
> 组队漏斗就没有任何账号注入了（`npc_player_from` 不设 `user_account`），而 docs 仍写「四条漏斗」。
> 本轮改为在**会话 seed 块**里注入（与 `auto_get_exp_*` / `abil_exp` 同一处），并补端到端用例与牙（§3）。

## 3. 用例与牙（3 用例 / 2 牙）

1. `npc_player_user_account_reads_injected_account`（npc.rs）—— 缺省空串；注入后原样返回；
2. `npc_player_from_session_injects_session_account_for_all_funnels`（lib.rs，**复审 I-1**）——
   `sg.account = "ua"` ⇒ `NpcPlayer::user_account() == "ua"`；未注入 ⇒ 空串；
3. `npc_player_user_account_check_and_del_account_list_end_to_end`（npc.rs，依赖 T-220）——
   `CHECKACCOUNTLIST` 两点前缀三态（`  UA  ` 命中 / 不在名单 miss / 空账号 miss）+ `DELACCOUNTLIST`
   删行后逐字节 = `other\r\n`。
4. `party_funnel_injects_session_user_account_for_check_account_list`（lib.rs，**复审 F1**）——
   组队漏斗 `run_party_goto_label` 端到端：`QFunction-0.txt` 的 `[@GroupCreate]` 用
   `CHECKACCOUNTLIST ..\AccountList.txt` 选 `MESSAGEBOX HIT` / `ELSESAY MISS`；
   会话 `account="ua"` ⇒ **恰好一条** `SM_MENU_OK(767)`；`account="nobody"` ⇒ **零条**。

| 牙 | 改动 | 结果 |
|---|---|---|
| A | 删 `NpcPlayer` 的 `fn user_account` 覆写 | 用例 1、2 红（`""` vs `"ua"`） |
| B | 把 `check_string_list` 的比较改成大小写敏感 | 用例 3 的 `  UA  ` 行变 miss（红） |
| C | 删 `run_party_goto_label` 的 `player.user_account = sg.account.clone();` | 用例 4 的 `ua` 支变 **0 条** `SM_MENU_OK`（红） |

> 两牙均实做并逐字节还原；sha256 取**提交后树**（见 §5）。

## 4. 台账与门禁

- `script_host_noop_audit.json`：`user_account` 行 `needs_card` → `safe/overridden`，
  `evidence` 指向 `ScriptPlayer for NpcPlayer::user_account`，`residual_whitelist = T-223`；
  全部 npc.rs `evidence.line` 按源文件重算。`check` / `check --strict-lines` / `selftest` 全绿。
- `whitelist.md` / `T-RESERVATIONS.md`：新增 **T-223** 行（本卡），不含 T-207 的重钉
  （那已由 T-220 卡承担，复审 M-5 已消解）。

## 5. 门禁（本 head）

> 全部用**本卡独立 target dir**（`CARGO_TARGET_DIR=/e/tmp/w67-useracct-target`）实跑；
> 共用 target 时源码 mtime 早于产物会让 cargo 误判 fresh、复用到别的卡的测试二进制（今晚已实测）。

```
cargo fmt --all -- --check                                        rc=0
cargo clippy --workspace --all-targets -- -D warnings             rc=0
cargo test --workspace                                            141 suites / 3109 passed / 0 failed
cargo run -p msgcodegen -- --check                                604 条一致，rc=0
script_host_noop_audit.py check --strict-lines                    GREEN（168 行：safe 65 / needs_card 99 / covered 4）
script_host_noop_audit.py selftest                                GREEN
whitelist_anchor_gate.py check                                    ALL PASS（546 条锚）
m3_whitelist_registry_audit.py audit                              GREEN
t_reservation_gate.py check                                       GREEN（rows=221 whitelist=221）
mir2_probe.py --selftest                                          OK（9 checks）
conflict_marker_gate.py                                           0 markers / 7409 files，rc=0
```

牙 C 的还原 sha256：`mir2-rs/bins/game-svr/src/lib.rs` = `908f35e85a021227c3e48af5013ce83d0a17f71f4ae0edf5ec428c0a8214a1e8`（= 提交树 `git show <head>:…`）。

牙的 sha256（提交后树 `git show <head>:<path>`）：

- `npc.rs` = `05bfce3bfcf3e4d97b7bb4d9b324c25edf7a1c1617e0de5622554e40fc63bf11`（= `git show 8235f2276:mir2-rs/bins/game-svr/src/npc.rs`）
- `crates/script/src/engine/mod.rs` = `8800a326cc8daf05396eecba07162a49a18084b39a33c49a41c2479dab5eab0c`（= `git show 8235f2276:…`）

## 6. 残余 / 窗口项（R，未跑）

- 真机窗口（T-223 行）：三个充值脚本（`QuestDiary/锋锐充值/freenet.txt` 等）的 `CHECKACCOUNTLIST`
  真机双栈判定；`DELACCOUNTLIST` 执行前后名单文件逐字节。
- 部署现状：`CHECKACCOUNTLIST` 238 行里经 `@UserCmd1`（oracle 命令体整段注释、不可达）或
  `Market_Def` 商人脚本到达 —— 商人漏斗本轮已接通账号，但路径形态仍是 `npc.path()+..`（T-220 O-1），
  两侧一致不剥 ⇒ 今天无可观测差异；`@StdModeFunc*` 里若出现 `CHECKACCOUNTLIST`，本轮起两侧同源。
- 窗口未跑前本卡不得 `closed`。

## 7. 残留物

- `E:/tmp/ua_*.py`（重钉/行插入脚本）、`E:/tmp/w24-ua-*.log`（门禁输出）；未起栈、未占端口、未连库。
