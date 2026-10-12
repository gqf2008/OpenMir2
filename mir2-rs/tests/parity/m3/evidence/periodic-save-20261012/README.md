# 卡 `openmir2-m3-periodic-save` 证据（Rust game-svr **周期存档** `SaveHumanRcdTime`）

**issue**：`a1ba1461`（`walgit collab thread openmir2-m3-periodic-save`）
**patch**：`patch --base refs/heads/master --head refs/heads/worktree-m2-gamesvr-host`（head 提交记录在 walgit patch 条目里）
**base**：`refs/heads/master`，交付时已 fast-forward 到 **`beca77c3`**（M3 ②③ 第十片合入后）
**worktree**：`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\m2-svr`（分支 `worktree-m2-gamesvr-host`）
**写集**：`mir2-rs/bins/game-svr/**` + 本证据目录（未动 `crates/**`、`src/**`、`MirClient`、表结构/脚本/数据格式、`mir2-rs/tests/parity/m3/**` 的夹具脚本）

## 0. 为什么是这条

`openmir2-m3-gamesvr-save` 收卡时如实登记：oracle 除 `MakeGhost`/断线/顶号三处事件触发外，
还有 **`Config.SaveHumanRcdTime`（默认 10 分钟）的周期存档**；Rust game-svr 此前没有它
⇒ M5 的 72h 长跑 / 灰度前后 DB 快照对比里，C# 会周期性写角色行而 Rust 不会，DB 侧必然偏离；
崩溃恢复也少一层。本片按 oracle 逐行补齐。

## 1. oracle 逐行口径（只读核对，原始件 `oracle-periodic-lines.txt`）

```text
GameSvrConf.SaveHumanRcdTime            src/Modules/SystemModule/Conf/Model/GameSvrConf.cs:423/1484
                                        （字段 int；构造缺省 = 10 * 60 * 1000 毫秒）
ServerConf 从 [Setup] 覆盖                src/Modules/SystemModule/Conf/ServerConf.cs:566
                                        ReadWriteInteger("Setup","SaveHumanRcdTime", 现默认值)
PlayObject.SaveRcdTick 声明/起算          src/M2Server/Player/PlayObject.cs:601 / 886
                                        （`new PlayObject()` 里 `= GetTickCount()`；每次登入
                                         `WorldServer.cs:234` 都 `new PlayObject()`，不复用 free list）
周期检查与重置                            src/GameSrv/Word/WorldServer.cs:698-703
   !FrontEngine.IsFull()
     && (GetTickCount() - playObject.SaveRcdTick) > Config.SaveHumanRcdTime
   ⇒ playObject.SaveRcdTick = GetTickCount();   // 先重置
     playObject.DealCancelA();
     SaveHumanRcd(playObject);                  // 复用与事件触发同一条存档路径
FrontEngine.IsFull()                      src/GameSrv/Services/FrnEngn.cs:120-133
                                        （实现是 m_SaveRcdList.Count >= int.MaxValue ⇒ 实际恒 false）
DealCancelA()                             src/M2Server/Player/PlayObject.Base.cs:748-752
                                        （Abil.HP = WAbil.HP; DealCancel();）
```

**计时口径（钉死）**：单位**毫秒**；缺省 **600000**；**进世界时起算**（= `new PlayObject()`），
**每次触发时先重置再发请求**；**断线/换图不复用对象** ⇒ 下次进世界重新起算；门是
**严格 `>`**（相等不触发）+ `!IsFull()`（恒真）。

## 2. Rust 侧实现（写集 = `mir2-rs/bins/game-svr/**`）

| 文件 | 内容 |
| --- | --- |
| `src/config.rs` | `GateConfig.save_human_rcd_time: i32`：读 `[Setup] SaveHumanRcdTime`，缺键用 oracle 缺省 `10*60*1000` |
| `src/load.rs` | `FrontEngine::is_full()`：1:1 映射 oracle `FrnEngn.IsFull()`（`Count >= int.MaxValue`，恒 false，**不“修”成有意义的容量上限**） |
| `src/main.rs` | `EnterConfig.save_human_rcd_time` 透传；`SessionGround.save_rcd_tick`（= `PlayObject.SaveRcdTick`，进世界取 `now_ms()`，随会话建/删）；`periodic_save_due()`（严格 `>` + 先重置的纯函数）；`drain_periodic_save()`（逐会话按键排序，复用 `save_human_rcd` 同一口径）；在 `world.tick()` 后、紧跟 `drain_recovery` 调用（oracle 的周期检查在同一次 `Run()` 之后 ⇒ 存档体读到本 tick 恢复后的 `WAbil`） |

**同一记录体口径**：`drain_periodic_save` 直接复用既有三处事件触发所用的 `save_human_rcd`
⇒ `build_save_record`（`MakeSaveRcd` 逐字段 + `HumItems` Helmet↔Necklace 交换）+ `DB_SAVEHUMANRCD(101)`
请求 + `engine.begin_save`（in-save 配对）+ `save_outstanding` 轮询（仅 `DBR_SAVEHUMANRCD(1102) && recog==1`
才 `end_save`）——**没有第二份记录实现**。

**登记（未建模，不静默）**：

- `playObject.DealCancelA()`：交易层未移植；且 `MakeSaveRcd` 读 `WAbil.HP`（`WorldServer.cs:1247`）
  ⇒ `Abil.HP` 的同步对存档体无影响。
- oracle 只在最内层 `LoginNoticeOk && BoReadyRun` 的 `else` 里查周期（`WorldServer.cs:685-698`）：
  本宿主 `SessionGround` 在入图帧全部下发后才建 ⇒ 建即已就绪；构造→就绪的亚秒窗口与 10 分钟量级无关。
- `IsRobot` 早返回：本宿主不建机器人会话（`RobotProcessor` 未移植）。

## 3. 用例与红检（DoD ③）

- `tests::periodic_save_due_is_strict_gt_and_resets_on_fire`（纯函数）：差值恰等于阈值**不触发**；
  超 1ms 触发且 tick 重置；刚重置不再触发；再满一个周期 +1 第二次触发。
- `tests::periodic_save_fires_repeatedly_and_record_is_well_formed`（构造性/进程级）：把周期参数调到
  **5s**，一个在线会话上**观测到 2 次** `DB_SAVEHUMANRCD` 请求；请求体经 `ReplyParser` +
  `parse_reply_message` + `edcode::decode`，再用 **DBSvr 侧同一类型**
  `mir2_storage::SaveCharacterData::from_bytes` 解回，逐字段核对（`account`/`chr_name`/`cur_map`
  /实时坐标 `11,22`/`gold=649`）；并断言计时器已推进到第二次触发时刻。
- `config::tests::setup_drop_pickup_keys_and_defaults`：`SaveHumanRcdTime=1200000` 实值 +
  缺键 ⇒ `600000` 缺省。

**红检（改坏 ⇒ 必红；每步跑完立即还原，末次还原后 2 条用例复跑绿）**：

| 文件 | 改法 | 原始输出 |
| --- | --- | --- |
| `red-strict-gt.txt` | `periodic_save_due` 的 `>` → `>=` | exit 101（`assertion failed: !periodic_save_due(..., 6_000)`） |
| `red-no-reset.txt` | 删掉 `*save_rcd_tick = now;`（不重置） | exit 101（周期用例观测 3 次请求，期望 2；纯函数 tick 断言红） |
| `red-record-col.txt` | `build_save_record` 的 `rec.data.gold = sg.state.gold` → `= 0` | exit 101（`left=0 right=649`，落库记录带实时 Gold） |
| `green-after-red.txt` | 三次改坏全部还原后复跑 | `2 passed; 0 failed` |

## 4. 门禁（DoD ⑤，隔离 `CARGO_TARGET_DIR=E:\tmp\openmir2-m2svr-target`，全 `--offline`）

**base = master `beca77c3`（fast-forward 后）**：

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| fmt | `cargo fmt --all -- --check` | rc=0（`gates-fmt.txt`，空 = 无 diff） |
| clippy | `cargo clippy --workspace --all-targets --offline -- -D warnings` | rc=0（`gates-clippy.txt`） |
| test | `cargo test --workspace --offline` | **77 suites / 633 passed / 0 failed / 16 ignored**（`gates-test-workspace.txt`） |
| msgcodegen | `cargo run -p msgcodegen --offline -- --check` | rc=0，604 条（`gates-msgcodegen.txt`） |

新增/改动用例 **3 连跑**：`gamesvr-3run-1..3.txt`，每次 `139 passed`（bin unit，含本片 2 条）
+ `80 passed` + `13 passed`（relay）/ `0 failed`。

> 坑（已记录）：共享 target 里 msgcodegen.exe 可能是**别的检出**编的旧产物（`repo root must exist`
> / rc=101）；`cargo clean -p msgcodegen` 后从本树重编 ⇒ rc=0 / 604。

## 5. 真机 / 端到端——**本片未起栈（判据全在单测/构造层），如实登记**

- 卡上明示：本片判据都在单测/构造层，**不需要起栈**。开工实拍端口：5000/5100/5500/5600/5700/
  6000/7000/7100/7200 **全空**；3306 = 共享 MySQL（pid 26652）——见 `ports-preflight.txt`。
  未起栈 ⇒ 无 `-Stop`；收工端口与开工一致（**窗口未被本线占用**）。
- 本片的终局判据（真机周期性落库）由 **M5 长跑 / 灰度 DB 快照对比**承接；`SaveHumanRcdTime`
  默认 10 分钟，本片用 5s 周期在构造层证明「计时 + 请求 + 记录体」这条链正确。
- **判据不降级**：记录体与既有三处事件触发**同一实现**，所以本片不会引入第二份存档口径；
  事件触发侧的既有红检（`red-gold/red-bag/red-ident/red-save-reply`）仍适用。

## 6. 边界与残留物

- 未动 `src/**`（C# oracle）、`crates/**`、`MirClient`、表结构/脚本语法/数据格式、`crates/storage`；
  未动 `mir2-rs/tests/parity/m3/**` 的夹具脚本（m3bag 写集）；未动别人的卡状态。
- 证据入库路径：`mir2-rs/tests/parity/m3/evidence/periodic-save-20261012/`；
  `SHA256SUMS.txt` 按仓库口径（LF 归一 = Git blob 字节）记，`tools/evidence-hash/verify_sha256sums.py --check-extra` 复核 GREEN，
  并在**干净 clone**（`core.autocrlf=true`）上复跑 GREEN 16/16——原始件 `clean-clone-verify.txt`（见 `commands.txt` §4）。
- 残留物：无（红检改坏均已还原；`git status` 仅本片源码 + 证据目录；未起栈/未起客户端）。
