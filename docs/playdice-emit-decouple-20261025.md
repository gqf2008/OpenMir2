# playdice emit 解耦 + 源码序门收窄（P-2/P-3，tier L）

- 卡：`openmir2-playdice-emit-decouple`（tier **L**，owner `openmir2-dev-2`，branch `card/playdice-emit-decouple`）。
- 基线：`master = 007ad730a`（2026-10-25；含 `playdice-frame-queue-full` 取证合并）。
- 写集：`mir2-rs/bins/game-svr/src/lib.rs`、测试、本文档。**零行为改动**（纯重构）。
- 来源：reviewer-21 在 `openmir2-script-playdice-openybdeal` review `54eeabc3` 报的 P-2/P-3（均 info）。

## 1. P-2：can_jmp 更新与发帧解耦

### 1.1 旧耦合

`emit_merchant_goto_result` 同时做两件事：① 按说词更新 `session.can_jmp_labels`
（`GotoLableSendMerChantSayMsg`，`ScriptEngine.cs:990-1000`）；② 发说词/脚本帧。
`@back` 把第一轮发帧延后到第二轮之后，导致 **round 2 的脚本运行期间**看不到 round 1 说词
更新的 `can_jmp_labels`（旧顺序是 round 1 发帧时才更新）。当前等价，但后续若脚本内部读该表会踩。

### 1.2 新结构

| 位置 | 改动 |
|---|---|
| `mir2-rs/bins/game-svr/src/lib.rs:7299` | 新增 `fn update_can_jmp_labels(session, says)`（逐说词 `get_script_label`，后到者胜） |
| `mir2-rs/bins/game-svr/src/lib.rs:7394` | 在 `run_merchant_goto` **脚本跑完、`player.frames` 返回之前**调用 |
| `emit_merchant_goto_result` | 删除说词更新循环；`session` 形参随之移除（5 个调用点同步） |
| `handle_click_npc_cmd` | 删除 `@main` 的重复说词更新循环（`run_merchant_goto` 已统一更新） |

### 1.3 为什么行为逐位不变

- `run_merchant_goto` 函数体内**不读** `can_jmp_labels`（reviewer-21 P-2 已核：`grep` 无读取点）；
- oracle 的嵌套 `GotoLable`（`ScriptEngine.cs:131-144`）不重查 `LableIsCanJmp`——该门只在
  `Merchant.UserSelect` / `GuildOfficial.UserSelect` 入口；
- 最终值不变：旧路径 = 合并说词逐条更新（最后一句胜）；新路径 = 每轮 run 逐条更新（最后一句胜）；
- 固定 NPC 路径（`guild_official.rs:623/831`）本来就在 goto 后更新，现在多一次同值更新，幂等。

**证据**：`cargo test -p game-svr --lib t142` ⇒ **11/11 绿**（含 `643/5008` 帧序用例
`t142_back_merges_two_goto_batches_into_one_handler_drain`、`t142_repair_says_precede_script_frames`、
`t142_click_npc_main_seeds_and_writes_back_gold_deal`）；`cargo test --workspace` ⇒ **3191 passed / 0 failed**。

## 2. P-3：源码序门收窄 + 新增 P-2 顺序门

| 测试 | 内容 |
|---|---|
| `t142_lable_is_can_jmp_has_single_definition`（`lib.rs:69547`） | 由 `str::contains("pub fn lable_is_can_jmp")` 改为**按行首锚定**：`lines().filter(trim_start().starts_with(...))`；注释里出现同名文本不再误红（reviewer P-3 建议的正则等价实现，不引入 `regex` 依赖） |
| `t142_can_jmp_update_precedes_frame_return_in_run_merchant_goto`（`lib.rs:69571`） | 新增：截取 `run_merchant_goto` 到 `emit_merchant_goto_result` 之间的函数体，断言 `update_can_jmp_labels(session, &says)` 出现在 `std::mem::take(&mut player.frames)` **之前**；并断言 `emit_merchant_goto_result` 体内不再出现 `can_jmp_labels =` |

守卫强度**未降低**：旧门只防「第二份实现」，新门同时钉「更新时机在帧返回前」与「emit 不再改状态」。

## 3. 牙（≥2，各还原）

| 牙 | 变异 | 结果 |
|---|---|---|
| T1（P-2 顺序） | 把 `update_can_jmp_labels(session, &says)` 从 `let out = (…)` 前挪到之后（用 `&[]` 保持可编译） | **RED**：`t142_can_jmp_update_precedes...` 失败（`0 passed; 1 failed`）；还原后绿 |
| T2a（P-3 收窄负向） | 在 `shop.rs` 加一行**注释** `// pub fn lable_is_can_jmp(...)` | **GREEN**：单实现门不再误红（收紧目的达成） |
| T2b（P-3 活性） | 在 `shop.rs` 加**真实** `pub fn lable_is_can_jmp(_: bool) -> bool` | **RED**：单实现门失败（`0 passed; 1 failed`）；还原后绿 |

三颗均 `cp` 备份 + 还原，最终 `grep` 无残留。

## 4. 门禁

| 检查 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | rc=0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 |
| `cargo test --workspace` | **rc=0，3191 passed / 0 failed** |
| `cargo run -p msgcodegen -- --check` | rc=0（604 条） |
| `conflict_marker` / `stale_pointer` / `dynamic_count_stamp` / `t_reservation` | rc=0 |
| `git diff --check` | rc=0 |
| `git merge-tree --write-tree master HEAD` | 提交后复跑，见 patch body |

## 5. 残留

- 无行为/判据变化；未改 `whitelist.md`（L 重构，不新增 T 行）。
- 临时物：`E:/tmp/lib-decouple.bak`、`E:/tmp/shop-decouple.bak`（牙备份，已还原；可删）、
  `/tmp/tooth-p2.log`、`/tmp/tooth-p3a.log`、`/tmp/tooth-p3b.log`、`/tmp/decouple-*.out`。
- 不 merge、不自审。
