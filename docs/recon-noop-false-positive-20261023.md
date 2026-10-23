# recon 入站 no-op 判据假阳性：`CM_BUTCH` 与全表审计

- 卡：`openmir2-recon-noop-false-positive`（tier=L）
- 车道：`openmir2-worker-61` / worktree `wt-worker-61` / 分支 `card/recon-noop-fp`
- 基线：master `995fbddfd`
- 写集：`mir2-rs/tests/parity/coverage/`、`docs/`（零生产 .rs）

## 1. 根因复核（确认）

`mir2-rs/tests/parity/coverage/recon_gates.py`（改前 `:129`）：

```python
RE_CLIENT_LITERAL_ARG = re.compile(
    r"(?<![A-Za-z0-9_])client_[a-z0-9_]+\s*\([^)]*?(?:\bfalse\b|\btrue\b|(?<![A-Za-z0-9_.])\d+(?![A-Za-z0-9_.]))")
```

它是**字符级**匹配：`[^)]*?` 只躲 `)`，不认 `{}`/`[]`。`bins/game-svr/src/lib.rs:2732` 的
`CM_BUTCH` 臂实参是结构体字面量

```rust
inbound::client_butch(&inbound::ButchInputs {
    n_x, n_y, bt_dir, curr_x, curr_y,
    close_speed_hack_check: true,
    now_tick: world.clock_ms as i32,
    turn_interval_time: 600,
    valid_object, target_butcherable,
})
```

`true` / `600` 是**输入结构体的字段值**，不是被顶死的会话态；`ButchOutcome` 随后被完整消费
（`out.send_butch` → `Command::Butch` 入队、`out.butchered` → `Command::ButchEffect` 入队）。
旧判据仍命中 ⇒ `noop=True` ⇒ `inbound:CM_BUTCH` 落 `tracked`，终点条件③（`tracked == 0`）被工具 bug 卡死。
**根因成立，非业务缺口。**

## 2. 判据形态（改后）

形态 (b) 由「实参文本里出现裸字面量」改为「**某一段顶层实参整体就是一个裸字面量**」：

1. `RE_CLIENT_CALL = client_[a-z0-9_]+\s*\(` 找到调用；
2. `_call_top_level_args()` 从 `(` 起扫到配对的 `)`，`(`/`[`/`{` 计深度、`)`/`]`/`}` 减深度，
   字符串 `"…"` / 字符 `'…'` 整体跳过，**只按顶层逗号切分**实参；
3. `RE_BARE_LITERAL_ARG.fullmatch(arg)`：`true`/`false`/十进制·十六·八·二进制整数/浮点，
   可带 `i*/u*/f32/f64` 类型后缀，允许前后空白；
4. 任一段顶层实参命中 ⇒ 形态 (b) 成立。形态 (a)（`let (d, _frames) = …`）原样保留。

**为什么不会误伤真 no-op**：登记在案的三类真 no-op 形态都是「handler 的**顶层实参**就是常量」——
`client_open_door(false)`、`client_get_min_map(0)`、`client_adjust_bonus(cmd.recog, 0)`，改后仍全部命中
（`selftest` 各有牙，见 §5）。被排除的只是**嵌套表达式里的常量**：结构体字段初始化
（`ButchInputs{…true…}`）、嵌套调用实参（`foo(false)`）、字符串 `"false"`。这些都不是「handler 被常量
顶死」，而是输入/包装表达式。

**残余·欠报方向（有意，判据未放宽）**：
- `foo(false)` 这类「常量裹一层」的实参判不出恒假；
- 经 `if-else` 判等链派发的入站帧没有 `=>` 臂体，本判据根本不覆盖（§4.3 逐条核对过，当前无恒空者）。

**残余·过报方向**：顶层实参若是**合法配置常量**（`client_x(cmd.a, 0)` 而 0 有语义）**会被判 no-op**
（实测 `client_call_top_level_literal_args('client_x(cmd.a, 0)') == True`）；当前生产臂里形态 (b) 只命中
`CM_OPENDOOR`（本身是 `n/a-unreachable` 的真 no-op），无实际误伤，新增此类调用形态时须复核。

## 3. dispatch 改前 / 改后

命令：`cd mir2-rs && python -I -B tests/parity/coverage/recon_gates.py dispatch --repo ..`

| | summary | `inbound:CM_BUTCH` |
| --- | --- | --- |
| 改前（`995fbddfd`） | `{"implemented": 178, "tracked": 1, "n/a-unreachable": 11, "n/a-oracle-noop": 1}` | `noop` / **tracked** / `lib.rs:2732` |
| 改后 | `{"implemented": 179, "n/a-unreachable": 11, "n/a-oracle-noop": 1}` | `-` / **implemented** / `lib.rs:2732` |

`tracked` 变 **0**，且**没有其他 tracked 行**需要解释（改前唯一一条就是它）。全表只有两行的 `noop` 列
非 `-`：`CM_BUTCH`（本卡修掉）与 `CM_OPENDOOR`（真 no-op，见下）。

## 4. 全表审计（本卡重点）

对象 = 全部 58 个**生产活跃**的 `messages::CM_X => { … }` 臂体（`production_cm_arms`），外加 24 个
**无 match 臂、经判等链派发**的 oracle `CM_*` 派发点（`inbound_dispatch_at`）。

### 4.1 被判 no-op 但实际有实现（假阳性）

| # | 位置 | 判定 |
| --- | --- | --- |
| 1 | `mir2-rs/bins/game-svr/src/lib.rs:2732` `CM_BUTCH` | **唯一一条**。结构体字段值撞上字符级正则；实为完整实现（`client_butch` 出帧 + 两条 `world.enqueue`）。本卡修复后转 implemented。 |

### 4.2 被判非 no-op 但实际恒空（假阴性）

判据：臂体去掉 `glog!(…)` 后不剩任何调用/赋值/结构体（自动化扫描了全部 58 个生产臂）。

| # | 位置 | 事实 | 结论 |
| --- | --- | --- | --- |
| 1 | `mir2-rs/bins/game-svr/src/lib.rs:2796` `CM_USERMAKEDRUGITEM` | 臂体**只有 `glog!`**，且注释明示「该臂在 sim 循环里**不可达**（商店派发先命中）」。真正的生产实现在 `lib.rs:5682`（`handle_shop_cmd` 的 `game_svr::shop::CM_USERMAKEDRUGITEM` 臂）。 | **门证据行指向死兜底**（status=implemented 但 `rust_at` 指 `2796`）。帧本身没有丢，属「证据列弱」，不是真缺口。登记为残余。 |
| 2 | `mir2-rs/bins/game-svr/src/lib.rs:2491` `CM_1042` | 臂体**只有 `glog!`**（无出帧、无状态改动）。 | **正确**：oracle 自身就是 log-only（`src/M2Server/Player/PlayObject.Message.cs:996-998`，`LogService.Warn`），已登记 `known_oracle_noop` ⇒ `n/a-oracle-noop`。不是假阴性。 |

**不是恒空、无需登记的两类（核对过）**：

- **值映射臂**：`attackwire.rs:78-86`（`CM_HIT => Some(0)`、`CM_BIGHIT => Some(2)`、… `CM_TWINHIT => Some(9)`）、
  `movewire.rs:115-116`（`CM_RUN => 2`、`CM_HORSERUN => 3`）、`movewire.rs:125-126`
  （`CM_RUN => messages::SM_RUN`、`CM_HORSERUN => messages::SM_HORSERUN`）——返回值被调用方消费，
  是调度实现本身。
- **短委派臂**：`CM_DEAL*`/`CM_SELLOFF*`/`CM_GUILD*`/`CM_*STORE*` 等一行调用 (`lib.rs:3197-3224`、
  `lib.rs:4891-4915`、`guild_cmd.rs:266-274`)，效果在被调函数里（`guild_cmd.rs:466-810` 的 `ctx.*` 均出帧/改态）。

### 4.3 判据不覆盖的 24 个判等链派发点（无 `=>` 臂体，逐条核对均非恒空）

`CM_CLICKNPC` `lib.rs:4596` → `handle_click_npc_cmd`；`CM_DROPITEM` `lib.rs:19057` → `handle_ground_cmd`；
`CM_EAT` `lib.rs:19169` → `handle_eat_cmd`；`CM_MAGICKEYCHANGE` `magicwire.rs:90` → 处理体 `lib.rs:11690+`；
`CM_MERCHANTDLGSELECT` `lib.rs:5045`；`CM_MERCHANTQUERYREPAIRCOST` `lib.rs:5582`；
`CM_MERCHANTQUERYSELLPRICE` `lib.rs:5517`；`CM_PICKUP` `lib.rs:2939`；
`CM_QUERYBAGITEMS` `lib.rs:19188` → `handle_query_bag_cmd`；`CM_QUERYUSERNAME` `lib.rs:4570`；
`CM_QUERYUSERSET` `lib.rs:4584`；`CM_QUERYUSERSTATE` `lib.rs:4577`；`CM_QUERYVAL` `lib.rs:4588`；
`CM_SAY` `game-gate/src/lib.rs:1044`；`CM_SOFTCLOSE` `lib.rs:4592`；
`CM_TAKEOFFITEM` `lib.rs:19134`；`CM_TAKEONITEM` `lib.rs:3500`；`CM_USERBUYITEM` `lib.rs:5021`；
`CM_USERGETDETAILITEM` `lib.rs:5563`；`CM_USERREPAIRITEM` `lib.rs:5600`；`CM_USERSELLITEM` `lib.rs:5535`；
`CM_USERSTORAGEITEM` `lib.rs:5813`；`CM_USERTAKEBACKSTORAGEITEM` `lib.rs:5833`；
`CM_WALK` `game-gate/src/lib.rs:1005`。

### 4.4 结论

- 假阳性**恰好 1 条**（`CM_BUTCH`），已修；
- 假阴性**0 条真缺口**；1 条证据列弱（`CM_USERMAKEDRUGITEM` 指向死兜底臂），1 条 oracle-noop（`CM_1042`）正确豁免；
- 24 个判等链派发点不在本启发式覆盖面内，逐条核对均委派给有出帧/改态的 handler。

## 5. 构造级正 / 负样本（`selftest` 夹具，均通过）

正样本（必须仍判 `True`，真 no-op）：

1. `let (d, _f) = client_open_door(false);`（形态 a）；
2. `let f = client_get_min_map(0); emit(f);`（形态 b，顶层整数）；
3. `let o = client_adjust_bonus(cmd.recog, 0); emit(o);`（形态 b，多实参里的顶层常量）；
4. `let o = client_open_door(true); emit(o);`（形态 b，顶层 bool）。

负样本（必须判 `False`，真实现）：

1. `let i = client_live(cmd.param); emit(i);`（动态入参 + 出帧）；
2. `let o = client_butch(&ButchInputs { n_x: 1, n_y: 2, close_speed_hack_check: true, turn_interval_time: 600 }); emit(o);`
   （结构体字面量**字段值**）；
3. `let o = client_wrap(make_for(cmd.param, 1)); emit(o);`（嵌套调用里的常量）；
4. `let o = client_note("false", cmd.param); emit(o);`（字符串不算裸字面量）。

## 6. 牙（逐字节还原 + sha256 从提交后树取）

判据文件提交后 `sha256 = 9355112dcd39dbc9d16bad8cb627b9c858a9f2e123542de578cc2d6c577fc179`。

| 牙 | 变异 | 结果（红） | 还原后 sha256 |
| --- | --- | --- | --- |
| ① 判据改回改前 | 形态 (b) 换回字符级正则 `client_x\([^)]*?(false\|true\|\d+)` | `CM_BUTCH` 重新 `noop`/`tracked`，summary `{"implemented":178,"tracked":1,…}`；`selftest` 2 FAIL（结构体字段值 / 嵌套调用被误判） | 一致 |
| ② 真 no-op 判成 False | `if any(RE_BARE_LITERAL_ARG.fullmatch(a) …)` → `if False and …` | `selftest` 3 FAIL（`CM_ZZDEAD2/3/4` 真 no-op 被判 False） | 一致 |
| ③ 真实现判成 no-op | 同一行改为 `if True:`（只要调 `client_*` 即 no-op） | `selftest` 4 FAIL（`CM_ZZLIVE` 动态臂 + `CM_ZZLIVE2/3/4` 真实现被判 no-op） | 一致 |

还原方式：用提交前的字节快照整体写回，逐次比对 `sha256`（三颗均 `OK`，最终树 = 提交树）。

## 7. 门禁

| 命令 | 改前 | 改后 |
| --- | --- | --- |
| `recon_gates.py selftest` | 91 PASS / 0 FAIL，`RESULT: ALL PASS` | **100 PASS / 0 FAIL**（+5 条判据牙 + 4 条 B-1 counts 契约牙），`RESULT: ALL PASS` |
| `recon_gates.py mutate` | 38 PASS / 0 FAIL，`RESULT: ALL PASS` | **38 PASS / 0 FAIL**（未受本卡影响） |
| `recon_gates.py dispatch` | `tracked=1` | **`tracked=0`**（`implemented=179`） |

改后退出码：`selftest` rc=0（**100** PASS / 0 FAIL）、`mutate` rc=0（38 PASS / 0 FAIL）、
`dispatch` rc=0（无 RED；`tracked` 为空集）。本卡不触碰任何 `.rs`，`cargo fmt/clippy/test` 与
`msgcodegen --check` 不受影响（写集仅 `mir2-rs/tests/parity/coverage/` + `docs/`）。

## 7.1 复审返工（reviewer-6 `4246fee6`，request_changes）

### B-1（blocking）：tracked 清零后 `dispatch --json` 缺 `counts.tracked` ⇒ 条件③退回「需人工」

`summarize()` 只统计**实际出现过**的 status，`tracked=0` 时键消失；消费端
`tools/goal/goal_gate.py::judge_b_tracked` 要求 `counts.tracked` 是 int，`docs/leader-goal-20261023.md`
§1 明写「`counts.tracked == 0` 且行集非空 ⇒ PASS」。

**修法**（`cmd_dispatch`，本卡写集内）：`counts = dict(counts); counts.setdefault("tracked", 0)`，
使 dispatch 侧**恒定输出** int 的 `tracked`；有 tracked 行时值由 `summarize` 给出，语义不变。

改后实测：

```
$ python -I -B tests/parity/coverage/recon_gates.py dispatch --repo .. --json <tmp>
summary: {"n/a-oracle-noop": 1, "implemented": 179, "n/a-unreachable": 11, "tracked": 0}

$ # goal_gate 条件③ 的**同一代码路径**（load_recon_dispatch → judge_b_tracked，真实 board+whitelist）
condition3_state  = pass
condition3_detail = B 类清零：recon tracked=0（rows 191 条非空、counts 一致）（recon 实测 tracked=0）
recon_counts      = {'n/a-oracle-noop': 1, 'implemented': 179, 'n/a-unreachable': 11, 'tracked': 0}
board_cards_available = True   whitelist_rows = 196
```

> 条件①/②/④ 未在本车道重跑：本卡零 `.rs` 改动，且车道 worktree 无 `target/`（clippy 会全量重建）；
> 条件③ 的 verdict 由 goal_gate 自身函数实算（上），不是手写期望。

**新牙**（selftest，4 条）：用最小临时仓跑完 `main(["dispatch", ..., "--json"])`，断言
① JSON `counts.tracked == 0` 且 `isinstance(int)` 且行集非空；② stdout summary 同口径；
③ `goal_gate.judge_b_tracked(rows, [], None, counts)[0] == "pass"`（消费端集成，goal_gate 缺席则跳过不误红）；
④ 有 tracked 行时 `summarize` 计数不变。

### I-1（important）：README 把「配置常量仍会被判 no-op」写反

已在 `mir2-rs/tests/parity/coverage/README.md` 修正：把「合法配置常量**会被**判 no-op」从
「欠报方向」段移出，单列为「**残余·过报方向**」，并写实测
`client_call_top_level_literal_args('client_x(cmd.a, 0)') == True`。代码注释与 patch body 原本正确，未动。

### M-1 / M-2（reviewer 已接受，本卡知情留痕）

- **M-1** `rust_at` 不保证是权威处理点：73 条 inbound 里 5 条非权威（`CM_USERMAKEDRUGITEM` 指
  `lib.rs:2796` 死兜底臂、真实现 `lib.rs:5682`；`CM_SAY`/`CM_WALK`/`CM_FIREHIT`/`CM_SPELL` 指
  game-gate 限速代理行）。reviewer 扫全部 58 个生产臂，log-only 恰好 2 条，与本卡审计一致
  ⇒ 孤例、非系统性。**本卡不修**，建议另开「证据列权威性」小卡（`inbound_dispatch_at` 优先返回
  game-svr 的 `messages::CM_X =>` 臂头，或增列 `rust_at_all`）。
- **M-2** 24 条判等链派发的 inbound 不过 noop 启发式（臂体为空 ⇒ 恒 False），实现靠人工审计；
  reviewer 抽验 8 条全为真实处理。**本卡不修**，若要条件③完全机判需把 noop 判据扩展到判等链（另卡）。

## 8. 残留物清单

- 临时脚本/证据：`E:/tmp/w61-*.py|json|txt|log`（审计、牙、goal_gate 复现用，未入库，可删）。
- 未改任何生产 `.rs`、未改 `cards.json` 的 `known_*`、未删/未放松 no-op 判据、未起栈/占端口/连库。
- 未改 `tools/goal/goal_gate.py`（B-1 在 recon 侧修，goal_gate 仅被 selftest 以 importlib 只读引用）。
