# equip_struck_save 武器 Dura 差异分诊（2026-10-28）

卡：`openmir2-equip-struck-dura-db-triage`；**R，`window_required=true`**。

证据为 run6 `E:/tmp/window/run6/equip_struck_save/`（场景 head `df6e404c2`）。本卡只读
frames / c2s / fixture / save_settle / DB TSV，不改生产实现、冻结场景或生效规则。

## 结论先行

- 两侧都是**一次 `CM_HIT` → 一次武器磨损 → 一条 642 → DB 末值等于该 642**。
- 4000→3997 与 4000→3994 分别唯一对应 `Random(5)+2` 的 3 与 6，即裸 `Random(5)` 的 1 与 4。
- 因而事件结构符合**单次 RNG 流位差**，不支持“命中次数不同”“Rust 多扣一次”或“DB 快照
  没等到新值”。run6 没有 `--rng-log`，所以这只是结构性判定，**不是最终 R 收口证据**；
  最终仍需当前 master 同场景带 RNG 流水复跑。
- 本轮没有确认 Rust 实现回归，不开实现修复卡；也不新增 `ignore_columns`。

可复现分析器：`tools/window/equip_struck_dura_triage.py`。真实输出：
`E:/tmp/dev-2/dura-run6-analysis-fixed.json`；合成负对照：`E:/tmp/dev-2/dura-check.txt`。
分析器已登记 `tool_selftest_gate`（`--check` 入口），工具门因此为 **59/59**、清点
**64 = 59 + 5 + 0**。

```sh
python -I -B tools/window/equip_struck_dura_triage.py \
  --root E:/tmp/window/run6/equip_struck_save \
  --json E:/tmp/dev-2/dura-run6-analysis-fixed.json
python -I -B tools/window/equip_struck_dura_triage.py --check
```

## 1. A1：从初始夹具到最终 DB 的完整事件链

夹具前置（`{csharp,rust}/inventory.json`）在两侧完全一致：PlayerId=6228、木剑
StdIndex=207、MakeIndex=5001、背包 Position=40、`Dura=DuraMax=4000`。场景随后把它穿到
武器位 1。run6 没有攻击前整库 before 快照，故“初始 4000”引用的是**实际夹具声明与写入**，
不伪称有一张不存在的 before DB 快照。

| 事件（按各侧帧序） | C# oracle | Rust |
|---|---|---|
| 初始武器夹具 | Dura=4000 | Dura=4000 |
| `CM_TAKEONITEM(1003)` / 615 | c2s seq38@3925 / seq39@4126 | c2s seq38@2121 / seq39@2322 |
| 点击卫家店 / 643 | seq44@4440 / seq45@4682 | seq44@2648 / seq45@2849 |
| `CM_TURN(3010)` | seq46@5004 | seq46@3165 |
| **`CM_HIT(3014)`** | **恰 1 条：seq47@5806** | **恰 1 条：seq47@3967** |
| **`SM_DURACHANGE(642)`** | **恰 1 条：seq52@6006** | **恰 1 条：seq50@4168** |
| 642 字段 | `recog=3997,param=1,tag=4000,series=0` | `recog=3994,param=1,tag=4000,series=0` |
| 14 / 31 / 202 / 203 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 |
| 最终 `characters_item` | `106,6228,1,5001,207,3997,4000` | `106,6228,1,5001,207,3994,4000` |

因此：

```text
C#：4000 - (Random(5)=1 + 2) = 3997 → 642.recog=3997 → DB.Dura=3997
Rust：4000 - (Random(5)=4 + 2) = 3994 → 642.recog=3994 → DB.Dura=3994
```

两侧 `hit_count == len(642) == 1`，没有第二条攻击、额外命中、破零 202 或升级 203 可补进
事件序列。差值 `3997-3994=3` 不是三次额外攻击：若真是攻击次数差，帧面必须出现相应的
`CM_HIT` 与 642，但实际没有。

## 2. A2：三种可能逐条判定

### 2.1 RNG / 流位差：结构上成立，流水证明仍缺

oracle `src/M2Server/Player/PlayObject.Attack.cs:215-229`：目标非空后抽一次
`Random(5)+2-WeaponStrong`，本木剑 `WeaponStrong=0`，随后 `DoDamageWeapon`；
`:1436-1470` 扣当前 Dura 并发 642。

Rust 对应：

- `mir2-rs/crates/world/src/combat.rs:1150-1152`：`random_below(5)+2`；
- `:1235-1271`：玩家有目标时只在 AttackSuccess 尾段调用一次磨损结算；
- `:1755-1783`：`WeaponStrong=0` 的包装、槽非空门与 `do_damage_weapon`；
- `mir2-rs/crates/world/src/world.rs:7278-7295`：攻方 `use_items` 写回并记录一次
  `pending_weapon_wears`；
- `mir2-rs/bins/game-svr/src/lib.rs:17793-17836`：每个事件按世界侧帧列排 642；
- `lib.rs:9904-9944`：存档以实体 `use_items` 为权威覆盖 `Index/Dura`。

逐行对照没有发现抽数上界、加 2、调用次数或写回路径差。实际 3/6 又都在一次
`Random(5)+2` 的 `[2,6]` 值域内，并分别精确等于两侧 642 与 DB 的磨损量。

**限制**：run6 的 `csharp/`、`rust/` 下都没有真实账本 `<side>/rng-stream.tsv`。分析器按该
固定文件路径判定，并校验头四行含 `INJECT_OK`/`SEED_PROBE`、数据行至少 5 列且 site 非空；
它**不读 probe argv**（`--rng-log` 是 card_window 的参数，probe argv 从来不会有它）。故本卡把
分类记为 `SINGLE_RANDOM5_FLOW_DIFFERENCE_CANDIDATE`，不冒充已有逐站点/逐序流水证明。
C# 的随机源是墙钟播种 `src/OpenMir2/RandomNumber.cs:35`，Rust 是独立进程固定种子流；
两边本就不要求同一次运行抽到相同裸值。最终 R 收口要补实际流水或等价流位锚。

### 2.2 Rust 实现回归：本轮证据不支持

若 Rust 多扣、重复结算或落库写错，至少应出现一种可观测形态：一次攻击多条 642、642 与
DB 不一致、磨损超出 `[2,6]`、或源码存在第二次 `Random(5)`/额外写槽。四种形态均未出现；
相反，Rust 的 `6 = 4+2` 与单次抽数完全闭合。

所以没有最小复现可据以开实现卡。这里是“未确认回归”，不是“证明永远没有回归”；后续
流水若显示同一 `Random(5)` 输入却被算成不同磨损，再另开实现卡。

### 2.3 DB 快照时点差：对这条记录可排除

- C# probe `end_wall=08:42:05`；save_settle 在 `08:42:06` 观测到一次变化，继续到尾段
  quiet=800ms，`elapsed=1797ms, changes=1, rc=0` 后才拍 DB。
- Rust probe `end_wall=08:43:15`；save_settle 同样在 `08:43:15` 观测到一次变化，
  `elapsed=1125ms, changes=1, rc=0` 后拍 DB。
- 最关键地，**最终 DB 各自等于此前最后一条 642**，不是旧的 4000，也不是另一次未见帧的
  第三值。

因此这条 Dura 差不是“快照拍早了一侧”的形态。save_settle 只能证明存档观测与快照顺序，
不能替代 §2.1 的 RNG 流水。

## 3. A3：负对照与分析器牙

`python -I -B tools/window/equip_struck_dura_triage.py --check` 实测 **9/9 PASS**：

1. 无成功攻击、无 642：两侧 `Dura=4000` 不变，分类 `NO_ATTACK_NO_CHANGE`；
2. 两侧同为 4000→3997：`SAME_SINGLE_WEAR`，不得伪报随机差；
3. run6 形态：3997/3994 唯一推出裸 `Random(5)=1/4`；
4. **变异牙①**：最后 642=3994 而 DB=4000 ⇒ `snapshot_consistent=false`，不得判 RNG；
5. **变异牙②**：一次攻击却两条 642 ⇒ `event_count_consistent=false`，不得编单次 RNG 解释；
6. 负对照：同值输入与应报差输入得到不同分类，证明比较器不恒等；
7. RNG 账本无文件 ⇒ `rng_log_present=false`；
8. tempdir 内合法 `rng-stream.tsv`（`INJECT_OK`/`SEED_PROBE` + 5 列 site）⇒ true；
9. **变异牙③**：文件存在但缺自证头 ⇒ false，不能把坏文件当流水证据。

`expected_checks=9` 是硬断言：删掉任一条用例会变成 `8/9`，不会在 8/8 下静默空过。
分析器已登记进 `tool_selftest_gate`；`--only equip_struck_dura_triage` 可单跑，完整门
**59/59**、`--inventory` **64 = 59 + 5 + 0**。把 `--check` 改坏时工具门会真实报红。

分析器对真实 run6 输出 `final_db_equals_last_642_both=true`、
`rng_log_present=false`、`wear_difference_csharp_minus_rust=-3`。空事件不会被当成一次 RNG；
快照不一致与事件数不一致都 fail-closed 到 UNDECIDABLE。

## 4. A4：剩余真机判据

由 Leader 排一次当前 master 的 `equip_struck_save` 复跑，建议显式带 `--rng-log` 或等价的
攻击流位锚：

1. 仍先断言每侧恰一次 `CM_HIT` 与一次武器 642，无额外 202/203；
2. 用流水直接定位 `PlayObject.Attack.cs:225` / `combat.rs:1151` 的 `Random(5)`，核对裸值、
   `+2`、642.recog 与最终 DB.Dura 四点一致；
3. 验收是 `db.diffs=[]`，或每个残余项都有已批准去向。不同裸 RNG 值造成的 Dura 差不得用
   `ignore_columns` 洗绿，应凭流水作 R 级 adjudication；
4. 656 与 201/10/41 的帧面事项仍各归原卡，本卡不替它们收口。

本卡 triage 完成不等于实现或场景已 closed；没有流水前，最终分类保留 R 未决边界。
