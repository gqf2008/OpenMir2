# `--check-scenario` R3 补方向：`send` 必须 `CM_*`、等待必须 `SM_*`（卡 `openmir2-probe-ident-direction`）

- 基线：`walgit/master` = `121a6b48b`；worktree `E:/tmp/wt-worker-69`，分支 `card/probe-ident-direction`。
- tier = **L**：只改 `tools/probe/mir2_probe.py` 的 R3 语义校验 + 文档；零生产代码、零帧。
- 工具 sha256（提交后树）：**`d093c4cb938840a5f7fe211454d495b0a4f1f154b22cd2a8d9e622d2be47922e`**

> **行号锚点**：本文凡引用 `mir2_probe.py` 的行号都是**本次 `grep -n` 实测**（会随改动漂移）；
> 稳定的锚点是**函数名**：`check_scenario` / `_selftest_scenario_validation` / `run_selftest`。

---

## 0. 结论先行

1. 旧 R3 只判「ident 在 `messages.rs` 的 **CM∪SM 并集**里」⇒ `send ident=4`（实为 `SM_41`）与
   `wait_for 80`（实为 `CM_QUERYUSERNAME`）都能 rc=0（reviewer-5 实测）。本卡补**方向**：
   - 发帧动作（`send` / `say` / `click_npc` / `dlg_select`，C2S）⇒ 必须 **`CM_*`**；
   - 观察动作（`wait_for` / `wait_frames.idents` / `wait_idents.idents`，等的是 S2C）⇒ 必须 **`SM_*`**。
   方向反了 ⇒ **rc=2** 并点名（`send ident 4 is SM_41, expected CM_*` /
   `wait_for ident 80 is CM_QUERYUSERNAME, expected SM_*`）。
2. **并集歧义**：`messages.rs` 里有 **6 个号**在 CM 与 SM 两个命名空间里各有名字
   （`42/100/101/102/103/104`），方向无法由号判定 ⇒ **两个方向都放行**（清单见 §1，处理见 §2）。
   `ISM_*`（跨服，49 个）**不进表**：解析器只认 `pub const (CM|SM)_`，所以不会混进并集。
3. **零误伤**：19 个场景逐个 `--check-scenario`（各带自己的 `required` 变量）**全部仍 rc=0**（§3）。
4. **牙**：两颗真实场景牙（§4.1/§4.2）+ 一颗「禁用方向校验 ⇒ `--selftest` 变红」的变异牙（§4.3）；
   场景文件逐字节还原（sha256 与提交树一致）。
5. 自检挂载：新断言在 `_selftest_scenario_validation`（`mir2_probe.py:1830`）里、由 `run_selftest`
   的 `check(...)` 收集，**rc 判定在其后**（`run_selftest` 的 `if fails: return 2`，`mir2_probe.py:2257-2262`）。

---

## 1. 并集口径的歧义清单（**不是静默放过**）

```sh
# 复现：把 messages.rs 的 CM_/SM_ 常量按「整行定义」解析后取交集
python -I -B - <<'EOF'
import re,io
rx=re.compile(r"pub const (CM|SM)_([A-Z0-9_]+): u16 = (\d+);\s*$")
cm={};sm={}
for raw in io.open("mir2-rs/crates/protocol/src/messages.rs",encoding="utf-8"):
    m=rx.match(raw.split("//",1)[0].strip())
    if m: (cm if m.group(1)=="CM" else sm).setdefault(int(m.group(3)),[]).append(m.group(1)+"_"+m.group(2))
print(sorted(set(cm)&set(sm)))
EOF
```

| ident | CM 名字 | SM 名字 | 说明 |
|---|---|---|---|
| 42 | `CM_42HIT` | `SM_USERNAME` | 攻击族 42 连击（C2S）与用户名（S2C）撞号 |
| 100 | `CM_QUERYCHR` | `SM_SYSMESSAGE` | SelGate 查询角色 vs 系统消息（`wait_for 100` 是合法等待） |
| 101 | `CM_NEWCHR` | `SM_GROUPMESSAGE` | 建角 vs 组队消息 |
| 102 | `CM_DELCHR` | `SM_CRY` | 删角 vs 喊话 |
| 103 | `CM_SELCHR` | `SM_WHISPER` | 选角 vs 私聊 |
| 104 | `CM_SELECTSERVER` | `SM_GUILDMESSAGE` | 选服 vs 行会消息 |

- 统计：`CM_` **102** 个、`SM_` **243** 个、交集 **6** 个（其余 96/237 个方向唯一）。
- **`ISM_*` 不在表内**：`_MSG_CONST_RE = pub const (CM|SM)_…`，49 个 `ISM_*`（含
  `ISM_CHANGECASTLEOWNER=211` / `ISM_RELOADCASTLEINFO=212` / `ISM_WHISPER`）一律解析不到 ⇒
  卡面担心的「203 同时是 SM_UPDATEITEM 与 ISM_WHISPER」在本校验里**不构成歧义**
  （203 只在 SM 集合里，`ISM_WHISPER` 根本不同号也不进表）。

---

## 2. 实现与处理口径

落点（`mir2_probe.py`，本次实测行号）：`check_scenario`（`:783`）的 **R3 段 `:839-870`**。

```
发帧动作（SEND_KINDS = send/say/click_npc/dlg_select）：
    ident ∉ CM：
        ident ∈ SM  ⇒ rc=2「steps[i].<kind> ident N is SM_xxx, expected CM_*」
        否则        ⇒ rc=2「steps[i].<kind>.ident N is not a CM_*/SM_* constant in …」
观察动作（WAIT_KINDS = wait_for/wait_frames/wait_idents）：
    ident ∉ SM：
        ident ∈ CM  ⇒ rc=2「steps[i].<kind> ident N is CM_xxx, expected SM_*」
        否则        ⇒ rc=2「steps[i].<kind> ident N is not a CM_*/SM_* constant in …」
```

- **歧义号（6 个）**：既在 CM 又在 SM ⇒ 两边的 `∉` 都不成立 ⇒ **两个方向都放行**。
  理由：只凭 ident 无法判定作者意图，判死会误伤合法场景（如 `wait_for 100` = `SM_SYSMESSAGE`）；
  且这 6 个号**本来就双向存在**，真跑时按方向由连接决定，不是场景能写错的。
  **不是静默**：上表逐条列出，代码注释点名，README 也写了。
- `say`/`click_npc`/`dlg_select` 的 ident 由驱动器固定展开成 `CM_SAY(3030)` / `CM_CLICKNPC(1010)` /
  `CM_MERCHANTDLGSELECT(1011)`，本次一并纳入方向校验（现在是**正向**断言：它们必须仍在 CM 集合里，
  防止将来常量表漂移把它们写成 SM）。
- 未放宽任何既有 R1/R2/R4/R5/R6：R3 的「表外 ident」判定原样保留，只**加**方向。

---

## 3. 回归表：19 个场景逐个 `--check-scenario`（各带自己的 `required`）

| 场景 | 传入的 `--var` | rc |
|---|---|---|
| `bag_equip.json` | `make_index=1 takeon_where=1 item_name=屠龙` | **0** |
| `combat_hit.json` | `target_x=30 target_y=32 hit_dir=4` | **0** |
| `combat_strike.json` | （`params.self_pos` 自带；另有 `blocked`） | **0** |
| `dice_chain.json` | — | **0** |
| `dice_chain_auto.json` | — | **0** |
| `eat_item.json` | `make_index=1 item_name=屠龙` | **0** |
| `equip_struck_save.json` | — | **0** |
| `gm_batch.json` | — | **0** |
| `gm_time.json` | — | **0** |
| `king_castle_war.json` | `king_actor=119` | **0** |
| `king_dialog.json` | `king_actor=119` | **0** |
| `login_only.json` | — | **0** |
| `merchant_dialog.json` | `merchant_actor=119` | **0** |
| `move_walk.json` | `move_x=8 move_y=12` | **0** |
| `npc_click.json` | `npc_actor=119` | **0** |
| `npc_click_auto.json` | — | **0** |
| `npc_script_chain.json` | `npc_actor=119` | **0** |
| `pickup_item.json` | `self_x=289 self_y=618` | **0** |
| `upgrade_retrieve.json` | `merchant_actor=119` | **0** |

**19/19 rc=0，零误伤。** 没有任何一条规则被放宽。

---

## 4. 牙

### 4.1 牙 ①：`send` 用 SM 号 ⇒ rc=2
`combat_strike.json` 的 `steps[0].send.ident` 3010（`CM_TURN`）临时改成 **4**（`SM_41`）：

```text
$ python -I -B tools/probe/mir2_probe.py --check-scenario \
    --scenario tools/probe/scenarios/combat_strike.json
scenario-error: steps[0].send ident 4 is SM_41, expected CM_* (send/say/click_npc/dlg_select are
client→server frames; an SM_* ident is server→client and can never be sent)
rc=2
```

### 4.2 牙 ②：`wait_for` 用 CM 号 ⇒ rc=2
`upgrade_retrieve.json` 的首个 `wait_for 643`（`SM_MERCHANTSAY`）临时改成 **80**（`CM_QUERYUSERNAME`）：

```text
$ python -I -B tools/probe/mir2_probe.py --check-scenario \
    --scenario tools/probe/scenarios/upgrade_retrieve.json --var merchant_actor=119
scenario-error: steps[1].wait_for ident 80 is CM_QUERYUSERNAME, expected SM_* (a wait can only match
server→client frames; a CM_* ident is client→server and can never arrive)
rc=2
```

**逐字节还原**（从提交后的树取 sha256）：

| 场景 | 提交树 sha256（= 还原后工作树 sha256） |
|---|---|
| `combat_strike.json` | `057fc3a2e3f5cec066e0323af57e66e17ef4e3c034de580961276629b7be1044` |
| `upgrade_retrieve.json` | `2a8323660fc77f2e31583056bc3ae9de83530db5f1b55d7eb957fd8063054934` |

### 4.3 牙 ③（变异牙）：禁用方向校验 ⇒ `--selftest` 变红
把 R3 的两个方向条件临时降回「并集口径」（`ident not in CM` → `ident not in CM∪SM`）：

```text
$ python -I -B tools/probe/mir2_probe.py --selftest
mir2_probe selftest: FAIL
  - scenario-validation: bad scenario accepted (tooth broken): 'send ident 4 is SM_41, expected CM_*'
rc=2
```

还原后 `--selftest` 回到 **0（9 checks）**，工具 sha256 与基线一致。

### 4.4 自检挂载位置（纪律 15）
新断言在 `_selftest_scenario_validation`（`mir2_probe.py:1830`）里，标记 `②c R3-direction`（`:1890`）
与 `②d/②e`（歧义放行、C2S 三动作正向）；`run_selftest`（`:2231`）用 `check(...)` 收集，
**`if fails: return 2`（`:2257-2262`）在所有 check 之后** ⇒ 新断言确实挂在 rc 判定路径之前。

---

## 5. 门禁（实跑）

| 命令 | 结果 |
|---|---|
| `python -I -B tools/probe/mir2_probe.py --selftest` | **0（9 checks）** |
| 19 场景 `--check-scenario`（§3） | **19/19 rc=0** |
| `python -I -B tools/conformance/card_window.py --selftest` | **0（GREEN 186/186）** |
| `python -I -B tools/whitelist/t_reservation_gate.py check` | **0（GREEN：208 行 / 208 号）** |
| `python -I -B tools/hygiene/conflict_marker_gate.py` | **0（0 markers）** |

---

## 6. 覆盖 / 不覆盖

- **覆盖**：R3 的方向维度（C2S vs S2C）；并集歧义清单与放行口径；19 场景零误伤；三类牙。
- **不覆盖**：
  1. **帧与命令的因果对应**（「这条 wait 等的真的是这条 send 产生的回包吗」）——属兄弟卡
     `openmir2-probe-producer-map` 的维度，本卡不扩张。
  2. `send.tag` 的字段级校验（攻击族 0..7 是 R6 的 WARN 级），本卡不动。
  3. 歧义 6 号的**意图判定**：只凭号不可能，已如实列出而不猜。

## 7. 残留

1. 歧义 6 号（`42/100/101/102/103/104`）永不可能被方向校验判死——这是**登记在案**的限制，不是漏洞；
   若将来 `messages.rs` 把某一号改成单向，交集会变小、校验自动变严（无需改本卡代码）。
2. 本卡只改 `tools/probe/mir2_probe.py` + 两份文档；未改 `card_window.py`/`param_discovery.py`/
   `rules.json`/`scenarios/*.json`/`src/**`；零起栈、零连库、零端口。
