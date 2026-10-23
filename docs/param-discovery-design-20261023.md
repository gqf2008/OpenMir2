# 对拍台「参数发现」设计（卡 `openmir2-window-param-discovery`）

- 状态：**设计稿**。代码落点等待写集解锁（`tools/conformance/` 与 `openmir2-card-window-tool` 同写集，必须串行）。
- 红线（卡 acceptance）：推导不出就 fail-closed（`scenario-error`，rc=2），**绝不**用默认值/猜值；不改任何场景的 `required` 声明。

## 1. 现状

15 个场景里 9 个声明了 `required` 运行时变量；`--check-scenario` 对它们一律 rc=2（`refusing to send guessed values`）。
`card_window.py` / `run_dual.sh` 目前不传 `--var` ⇒ 参数化 run 起跑即卡住。

## 2. 推导源（每个变量一行：来源帧 + 字段 + oracle 出处）

| 变量 | 可否推导 | 来源 | oracle 出处 |
|---|---|---|---|
| `self_x` / `self_y` | **可** | `SM_LOGON(50)` 的 `param` / `tag` | 实测 `frames.ndjson`：`50 recog=191 param=7 tag=12`，与 DB `characters` `B013 CX=7 CY=12` 一致 |
| `npc_actor` | **可** | `SM_TURN(10)` 中 `recog != 自身` 的首个 actor | `PlayObject.Message.cs:1542` `MakeMessage(SM_TURN, ActorId, nParam1, nParam2, MakeWord(wParam, Light))` |
| `merchant_actor` | **可**（同一来源） | 同上；当前部署里首个可见 actor 就是赌场庄家（`SM_MERCHANTSAY(643)` 正文 `〖庄〗…`） | 同上 + `PlayObject.Operate.cs:120` `FindMerchant(nParam1) ?? FindNpc(nParam1)` |
| `king_actor` | **仅当视野内出现该 NPC** | `SM_TURN(10)` 的其它非自身 actor；无法从帧判别"是国王" | —（判别不出 ⇒ fail-closed） |
| `make_index` / `item_name` | **不可**（现阶段） | 需 `SM_BAGITEMS(201)` | 现网 22+ 个 run **无 201**（`PlayObject.Message.cs:785` 的 30s 节流 + 未拿到回包）⇒ 必然 fail-closed |
| `move_x` / `move_y` | **不可**（只能按规则构造） | 已录帧里**没有** `SM_SPACEMOVE/SM_WALK`，也没有自身 `SM_TURN` | 若采用「自位 + 固定方向（东）」的显式规则，须在本卡写明理由并接受"规则而非推导" |
| `target_x` / `target_y` | **不可** | 帧里没有目标格坐标 | 同上；**默认 fail-closed** |
| `takeon_where` / `hit_dir` | **不可** | 无来源帧 | fail-closed（可由操作者 `--var` 显式给） |

## 3. 发现阶段流程（拟）

```
discovery run（固定场景：login_only + 一次「不点击」的进世界收帧）
    → 读 frames.ndjson
    → derive(required_vars)：
         每个变量查「推导表」；命中 ⇒ 记录 (值, 来源帧 seq/ident/字段)
         未命中 ⇒ 记入 missing
    → missing 非空 ⇒ 直接 scenario-error（rc=2），消息逐个点名
    → 全部命中 ⇒ 生成 --var 列表交给后续场景
```

- **两侧各自发现**：Rust 与 C# 的 ActorId 每会话不同 ⇒ 各自跑 discovery、各用自己的值；比较器已默认掩码 `recog`，
  不会把两侧不同的 ActorId 当成差异（这是卡里"选哪条口径"的答案与理由）。
- **发现值要落盘**：`run.json` 同级写 `discovered.json`（`{var: {value, source:{seq,ident,field}}}`），便于复审复算。

## 4. 构造级用例（≥6）与牙（≥2）

用例（给定的 `frames.ndjson` 夹具）：
1. 含目标 NPC + 含商人 + 含自身坐标 ⇒ 推出的 `--var` 集合与值逐项断言；
2. 缺目标 NPC（只有自身）⇒ `npc_actor/merchant_actor` 缺席 ⇒ fail-closed；
3. 缺自身坐标（无 `SM_LOGON`）⇒ `self_x/self_y` 缺席 ⇒ fail-closed；
4. 有 `SM_BAGITEMS(201)` 的合成夹具 ⇒ 能推出 `make_index/item_name`（证明规则可扩展）；
5. 多个非自身 actor ⇒ 取"首个"并记录来源，断言确定性（同夹具两次跑同结果）；
6. 发现阶段返回空集合 ⇒ 后续场景 rc=2。

牙：
- ① 把推导表的字段改错一位（例如 `SM_LOGON` 取 `tag` 当 X）⇒ 对应用例必须红；
- ② 让发现阶段在 missing 时静默返回空 `--var` ⇒ 后续场景 rc=2 的用例必须红。

## 5. 与 `card_window.py` 的接线（等写集解锁后）

- `card_window.py` 增加 `--discover` 开关：先跑 discovery 场景，再把派生值拼进每个场景的 `--var`；
- `run_dual.sh` 两侧各自 discovery（不共享 ActorId）；
- `--check-scenario` 全 15 个文件在"补上发现值"后全部 rc=0（命令与输出贴回卡上）。
